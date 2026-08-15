//! Localhost-only authenticated clock MCU used by real-browser qualification.

use core::fmt;
use core::net::SocketAddr;
use core::time::Duration;
use std::io::{self, BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::time::Instant;

use alumina_net::{HttpMethod, MAX_AUTHENTICATED_BODY_BYTES};
use alumina_protocol::{
    DeviceCycle, DeviceId, FrameHeader, MessageDirection, MessageHeader, Operation, StatusCode,
};
use alumina_sim::http_fixture::{
    ClockFixturePolicy, ClockHttpFixture, FixtureHttpRequest, FixtureHttpResponse,
};

const DEFAULT_BIND: &str = "127.0.0.1:8098";
const DEFAULT_SECRET: &str = "alumina-development";
const MAXIMUM_REQUEST_LINE_BYTES: usize = 1_024;
const MAXIMUM_HEADER_LINE_BYTES: usize = 2_048;
const MAXIMUM_HEADER_BYTES: usize = 8 * 1_024;
const MAXIMUM_HEADERS: usize = 32;
const IO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Eq, PartialEq)]
struct ServerOptions {
    bind: SocketAddr,
    device_id: DeviceId,
    secret: Vec<u8>,
    processing_delay_ms: u64,
    response_delay_ms: u64,
    drift_ppm: i32,
    drop_initial_control_requests: u64,
    drop_control_request: Option<u64>,
    drop_operation_response: Option<Operation>,
    reboot_control_request: Option<u64>,
}

impl ServerOptions {
    fn parse() -> Result<Self, ServerError> {
        let mut options = Self {
            bind: DEFAULT_BIND
                .parse()
                .expect("static socket address is valid"),
            device_id: DeviceId(*b"ALUM-SIM:TINYBEE"),
            secret: DEFAULT_SECRET.as_bytes().to_vec(),
            processing_delay_ms: 1,
            response_delay_ms: 2,
            drift_ppm: 37,
            drop_initial_control_requests: 0,
            drop_control_request: None,
            drop_operation_response: None,
            reboot_control_request: None,
        };
        let mut arguments = std::env::args().skip(1);
        while let Some(argument) = arguments.next() {
            if argument == "--help" {
                print_help();
                std::process::exit(0);
            }
            let value = arguments
                .next()
                .ok_or_else(|| ServerError::Argument(format!("{argument} requires a value")))?;
            match argument.as_str() {
                "--bind" => {
                    options.bind = value
                        .parse()
                        .map_err(|_| ServerError::Argument("invalid --bind address".to_owned()))?;
                }
                "--secret" => {
                    if value.is_empty() || value.len() > 256 {
                        return Err(ServerError::Argument(
                            "--secret must contain 1 through 256 bytes".to_owned(),
                        ));
                    }
                    options.secret.fill(0);
                    options.secret = value.into_bytes();
                }
                "--device-id" => {
                    options.device_id = parse_device_id(&value)?;
                }
                "--processing-delay-ms" => {
                    options.processing_delay_ms = bounded_milliseconds(&value)?;
                }
                "--response-delay-ms" => {
                    options.response_delay_ms = bounded_milliseconds(&value)?;
                }
                "--drift-ppm" => {
                    options.drift_ppm = value
                        .parse()
                        .ok()
                        .filter(|ppm: &i32| (-10_000..=10_000).contains(ppm))
                        .ok_or_else(|| {
                            ServerError::Argument(
                                "--drift-ppm must be -10000 through 10000".to_owned(),
                            )
                        })?;
                }
                "--drop-initial-control-requests" => {
                    options.drop_initial_control_requests = value.parse().map_err(|_| {
                        ServerError::Argument(
                            "--drop-initial-control-requests must be an unsigned integer"
                                .to_owned(),
                        )
                    })?;
                }
                "--drop-control-request" => {
                    options.drop_control_request = Some(nonzero_u64(&value, &argument)?);
                }
                "--drop-operation-response" => {
                    options.drop_operation_response = Some(parse_operation(&value)?);
                }
                "--reboot-control-request" => {
                    options.reboot_control_request = Some(nonzero_u64(&value, &argument)?);
                }
                _ => {
                    return Err(ServerError::Argument(format!(
                        "unknown argument {argument}"
                    )));
                }
            }
        }
        Ok(options)
    }
}

impl Drop for ServerOptions {
    fn drop(&mut self) {
        self.secret.fill(0);
    }
}

fn bounded_milliseconds(value: &str) -> Result<u64, ServerError> {
    value
        .parse()
        .ok()
        .filter(|milliseconds| *milliseconds <= 30_000)
        .ok_or_else(|| ServerError::Argument("delay must be 0 through 30000 ms".to_owned()))
}

fn nonzero_u64(value: &str, argument: &str) -> Result<u64, ServerError> {
    value
        .parse()
        .ok()
        .filter(|value| *value != 0)
        .ok_or_else(|| ServerError::Argument(format!("{argument} must be a nonzero integer")))
}

fn print_help() {
    println!(
        "alumina-sim-http [--bind IP:PORT] [--device-id 32_HEX_DIGITS] [--secret TEXT] \
         [--processing-delay-ms N] [--response-delay-ms N] [--drift-ppm N] \
         [--drop-initial-control-requests N] [--drop-control-request N] \
         [--drop-operation-response NAME_OR_WIRE] [--reboot-control-request N]"
    );
}

fn parse_operation(value: &str) -> Result<Operation, ServerError> {
    let named = match value {
        "storage-inspect" => Some(Operation::StorageInspect),
        "storage-begin-upload" => Some(Operation::StorageBeginUpload),
        "storage-put-chunk" => Some(Operation::StoragePutChunk),
        "storage-finalize" => Some(Operation::StorageFinalize),
        "job-prepare" => Some(Operation::JobPrepare),
        "job-status" => Some(Operation::JobStatus),
        "job-commit" => Some(Operation::JobCommit),
        "job-confirm" => Some(Operation::JobConfirm),
        "job-abort" => Some(Operation::JobAbort),
        "job-cancel" => Some(Operation::JobCancel),
        _ => None,
    };
    if let Some(operation) = named {
        return Ok(operation);
    }
    let wire = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .and_then(|digits| u16::from_str_radix(digits, 16).ok())
        .or_else(|| value.parse::<u16>().ok());
    wire.and_then(Operation::from_wire).ok_or_else(|| {
        ServerError::Argument(
            "--drop-operation-response requires a supported canonical name or assigned u16 wire value"
                .to_owned(),
        )
    })
}

fn parse_device_id(value: &str) -> Result<DeviceId, ServerError> {
    if value.len() != 32 {
        return Err(ServerError::Argument(
            "--device-id must contain exactly 32 hexadecimal digits".to_owned(),
        ));
    }
    let mut bytes = [0_u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hexadecimal_nibble(pair[0]).ok_or_else(|| {
            ServerError::Argument("--device-id contains a non-hexadecimal digit".to_owned())
        })?;
        let low = hexadecimal_nibble(pair[1]).ok_or_else(|| {
            ServerError::Argument("--device-id contains a non-hexadecimal digit".to_owned())
        })?;
        bytes[index] = (high << 4) | low;
    }
    if bytes.iter().all(|byte| *byte == 0) {
        return Err(ServerError::Argument(
            "--device-id may not use the all-zero sentinel".to_owned(),
        ));
    }
    Ok(DeviceId(bytes))
}

const fn hexadecimal_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Debug)]
struct AffineHostClock {
    start: Instant,
    base_cycle: u64,
    nominal_frequency_hz: u64,
    rate_ppm: u64,
}

impl AffineHostClock {
    fn new(nominal_frequency_hz: u64, drift_ppm: i32) -> Self {
        let rate_ppm = u64::try_from(1_000_000_i64 + i64::from(drift_ppm))
            .expect("validated drift retains a positive rate");
        Self {
            start: Instant::now(),
            base_cycle: 10_000_000,
            nominal_frequency_hz,
            rate_ppm,
        }
    }

    fn cycle(&self) -> DeviceCycle {
        const NANOS_PER_SECOND: u128 = 1_000_000_000;
        const PPM: u128 = 1_000_000;

        let elapsed_ns = self.start.elapsed().as_nanos();
        let advanced = elapsed_ns
            .saturating_mul(u128::from(self.nominal_frequency_hz))
            .saturating_mul(u128::from(self.rate_ppm))
            / (NANOS_PER_SECOND * PPM);
        DeviceCycle(
            self.base_cycle
                .saturating_add(u64::try_from(advanced).unwrap_or(u64::MAX)),
        )
    }

    fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.start.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

#[derive(Debug, Default)]
struct FaultState {
    control_requests: u64,
    single_drop_completed: bool,
    operation_drop_completed: bool,
    rebooted: bool,
}

fn main() -> Result<(), ServerError> {
    let options = ServerOptions::parse()?;
    let listener = TcpListener::bind(options.bind)?;
    let clock = AffineHostClock::new(
        ClockFixturePolicy::HEALTHY_1MHZ.frequency_hz,
        options.drift_ppm,
    );
    let mut fixture = ClockHttpFixture::new_for_device(
        options.secret.clone(),
        [0x31; 16],
        ClockFixturePolicy::HEALTHY_1MHZ,
        options.device_id,
    )
    .map_err(|error| ServerError::Fixture(error.to_string()))?;
    fixture.enable_simulated_telemetry_provider();
    fixture.enable_simulated_waveform_provider();
    let mut faults = FaultState::default();
    println!(
        "alumina-sim-http ready origin=http://{} drift_ppm={} processing_ms={} response_ms={}",
        options.bind, options.drift_ppm, options.processing_delay_ms, options.response_delay_ms
    );

    for connection in listener.incoming() {
        let mut stream = match connection {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("accept failed: {error}");
                continue;
            }
        };
        if let Err(error) =
            handle_connection(&mut stream, &options, &clock, &mut fixture, &mut faults)
        {
            eprintln!("connection rejected: {error}");
        }
    }
    Ok(())
}

fn handle_connection(
    stream: &mut TcpStream,
    options: &ServerOptions,
    clock: &AffineHostClock,
    fixture: &mut ClockHttpFixture,
    faults: &mut FaultState,
) -> Result<(), ServerError> {
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let request = read_request(stream)?;
    let control = request.method == HttpMethod::Post && request.path == "/api/v1/control";
    let operation = control.then(|| request_operation(&request)).flatten();
    if control {
        faults.control_requests = faults.control_requests.saturating_add(1);
        if !faults.rebooted && options.reboot_control_request == Some(faults.control_requests) {
            fixture
                .reboot([0x52; 16])
                .map_err(|error| ServerError::Fixture(error.to_string()))?;
            faults.rebooted = true;
        }
    }
    let receive_cycle = clock.cycle();
    if !options.processing_delay_ms.eq(&0) {
        std::thread::sleep(Duration::from_millis(options.processing_delay_ms));
    }
    let transmit_cycle = clock.cycle();
    let response = fixture.handle(&request, clock.elapsed_ms(), receive_cycle, transmit_cycle);
    let initial_outage =
        control && faults.control_requests <= options.drop_initial_control_requests;
    let single_drop = control
        && !faults.single_drop_completed
        && options.drop_control_request == Some(faults.control_requests);
    if single_drop {
        faults.single_drop_completed = true;
    }
    let operation_drop = !faults.operation_drop_completed
        && options.drop_operation_response.is_some()
        && options.drop_operation_response == operation
        && operation.is_some_and(|operation| native_response_succeeded(&response, operation));
    if operation_drop {
        faults.operation_drop_completed = true;
        if let Some(operation) = operation {
            eprintln!(
                "fault injection: dropped applied response for operation 0x{:04x}",
                operation.wire_value()
            );
        }
    }
    if initial_outage || single_drop || operation_drop {
        return Ok(());
    }
    if !options.response_delay_ms.eq(&0) {
        std::thread::sleep(Duration::from_millis(options.response_delay_ms));
    }
    write_response(stream, &response)
}

fn request_operation(request: &FixtureHttpRequest) -> Option<Operation> {
    if request.method != HttpMethod::Post || request.path != "/api/v1/control" {
        return None;
    }
    let message = native_message(&request.body)?;
    (message.direction == MessageDirection::Request).then_some(message.operation)
}

fn native_response_succeeded(response: &FixtureHttpResponse, operation: Operation) -> bool {
    response.status == 200
        && native_message(&response.body).is_some_and(|message| {
            message.direction == MessageDirection::Response
                && message.operation == operation
                && message.status == StatusCode::Ok
        })
}

fn native_message(encoded: &[u8]) -> Option<MessageHeader> {
    if encoded.len() < FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN {
        return None;
    }
    let maximum_payload = u32::try_from(MAX_AUTHENTICATED_BODY_BYTES).ok()?;
    let frame = FrameHeader::decode(&encoded[..FrameHeader::WIRE_LEN], maximum_payload).ok()?;
    let payload_len = usize::try_from(frame.payload_len).ok()?;
    if encoded.len() != FrameHeader::WIRE_LEN.checked_add(payload_len)? {
        return None;
    }
    let message_end = FrameHeader::WIRE_LEN.checked_add(MessageHeader::WIRE_LEN)?;
    MessageHeader::decode_and_validate(
        &encoded[FrameHeader::WIRE_LEN..message_end],
        frame.kind,
        frame.payload_len,
    )
    .ok()
}

fn read_request(stream: &mut TcpStream) -> Result<FixtureHttpRequest, ServerError> {
    let mut reader = BufReader::new(stream);
    let request_line = read_line(&mut reader, MAXIMUM_REQUEST_LINE_BYTES)?;
    let request_text = core::str::from_utf8(&request_line)
        .map_err(|_| ServerError::Protocol("request line is not ASCII"))?;
    let mut fields = request_text.split(' ');
    let method = parse_method(fields.next().unwrap_or_default())?;
    let path = fields.next().unwrap_or_default();
    let version = fields.next().unwrap_or_default();
    if fields.next().is_some()
        || !matches!(version, "HTTP/1.1" | "HTTP/1.0")
        || path.is_empty()
        || path.contains('?')
    {
        return Err(ServerError::Protocol("request target/version is invalid"));
    }

    let mut headers = Vec::new();
    let mut header_bytes = 0_usize;
    loop {
        let line = read_line(&mut reader, MAXIMUM_HEADER_LINE_BYTES)?;
        if line.is_empty() {
            break;
        }
        header_bytes = header_bytes
            .checked_add(line.len())
            .filter(|bytes| *bytes <= MAXIMUM_HEADER_BYTES)
            .ok_or(ServerError::Protocol("request headers exceed their bound"))?;
        if headers.len() == MAXIMUM_HEADERS {
            return Err(ServerError::Protocol("request has too many headers"));
        }
        let colon = line
            .iter()
            .position(|byte| *byte == b':')
            .ok_or(ServerError::Protocol("header is missing a colon"))?;
        let name = &line[..colon];
        if name.is_empty()
            || !name
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
        {
            return Err(ServerError::Protocol("header name is invalid"));
        }
        let value = trim_ows(&line[colon + 1..]);
        headers.push((name.to_vec(), value.to_vec()));
    }
    let body_len = canonical_content_length(&headers)?;
    if body_len > MAX_AUTHENTICATED_BODY_BYTES {
        return Err(ServerError::Protocol("request body exceeds fixture bound"));
    }
    let mut body = vec![0_u8; body_len];
    reader.read_exact(&mut body)?;
    Ok(FixtureHttpRequest {
        method,
        path: path.to_owned(),
        headers,
        body,
    })
}

fn read_line(
    reader: &mut BufReader<&mut TcpStream>,
    maximum_bytes: usize,
) -> Result<Vec<u8>, ServerError> {
    let mut line = Vec::new();
    let read = reader.read_until(b'\n', &mut line)?;
    if read == 0 || line.len() > maximum_bytes || !line.ends_with(b"\r\n") {
        return Err(ServerError::Protocol(
            "HTTP line is incomplete or oversized",
        ));
    }
    line.truncate(line.len() - 2);
    Ok(line)
}

fn parse_method(method: &str) -> Result<HttpMethod, ServerError> {
    match method {
        "GET" => Ok(HttpMethod::Get),
        "POST" => Ok(HttpMethod::Post),
        "PUT" => Ok(HttpMethod::Put),
        "DELETE" => Ok(HttpMethod::Delete),
        "OPTIONS" => Ok(HttpMethod::Options),
        _ => Err(ServerError::Protocol("HTTP method is unsupported")),
    }
}

fn canonical_content_length(headers: &[(Vec<u8>, Vec<u8>)]) -> Result<usize, ServerError> {
    let mut values = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case(b"Content-Length"));
    let Some((_, value)) = values.next() else {
        return Ok(0);
    };
    if values.next().is_some()
        || value.is_empty()
        || (value.len() > 1 && value[0] == b'0')
        || !value.iter().all(u8::is_ascii_digit)
    {
        return Err(ServerError::Protocol("content length is not canonical"));
    }
    let text = core::str::from_utf8(value)
        .map_err(|_| ServerError::Protocol("content length is not ASCII"))?;
    text.parse()
        .map_err(|_| ServerError::Protocol("content length overflows"))
}

fn trim_ows(mut value: &[u8]) -> &[u8] {
    while value
        .first()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        value = &value[1..];
    }
    while value
        .last()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        value = &value[..value.len() - 1];
    }
    value
}

fn write_response(
    stream: &mut TcpStream,
    response: &FixtureHttpResponse,
) -> Result<(), ServerError> {
    write!(
        stream,
        "HTTP/1.1 {} {}\r\n",
        response.status, response.reason
    )?;
    for (name, value) in &response.headers {
        write!(stream, "{name}: {value}\r\n")?;
    }
    stream.write_all(b"Connection: close\r\n\r\n")?;
    stream.write_all(&response.body)?;
    stream.flush()?;
    Ok(())
}

#[derive(Debug)]
enum ServerError {
    Io(io::Error),
    Argument(String),
    Protocol(&'static str),
    Fixture(String),
}

impl From<io::Error> for ServerError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl fmt::Display for ServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O failed: {error}"),
            Self::Argument(error) => write!(formatter, "argument rejected: {error}"),
            Self::Protocol(error) => write!(formatter, "HTTP request rejected: {error}"),
            Self::Fixture(error) => write!(formatter, "clock fixture failed: {error}"),
        }
    }
}

impl core::error::Error for ServerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alumina_protocol::Digest;

    fn native_request(message: MessageHeader) -> FixtureHttpRequest {
        let frame = FrameHeader::new(
            message.operation.frame_kind(),
            u32::try_from(MessageHeader::WIRE_LEN).unwrap(),
            1,
            DeviceCycle(2),
            Digest::ZERO,
        );
        let mut body = Vec::new();
        body.extend_from_slice(&frame.encode());
        body.extend_from_slice(&message.encode());
        FixtureHttpRequest {
            method: HttpMethod::Post,
            path: "/api/v1/control".to_owned(),
            headers: Vec::new(),
            body,
        }
    }

    #[test]
    fn operation_selector_accepts_names_and_assigned_wire_values_only() {
        assert_eq!(
            parse_operation("storage-put-chunk").unwrap(),
            Operation::StoragePutChunk
        );
        assert_eq!(parse_operation("0x0503").unwrap(), Operation::JobCommit);
        assert_eq!(parse_operation("1289").unwrap(), Operation::JobConfirm);
        assert!(parse_operation("0x090b").is_err());
        assert!(parse_operation("put-something").is_err());
    }

    #[test]
    fn operation_selector_requires_one_canonical_native_control_request() {
        let request = native_request(MessageHeader::request(Operation::JobCommit, 7, 0));
        assert_eq!(request_operation(&request), Some(Operation::JobCommit));

        let response = native_request(MessageHeader::response(
            Operation::JobCommit,
            7,
            StatusCode::Ok,
            0,
        ));
        assert_eq!(request_operation(&response), None);

        let mut trailing = request.clone();
        trailing.body.push(0);
        assert_eq!(request_operation(&trailing), None);

        let mut foreign_path = request;
        foreign_path.path = "/api/v1/storage".to_owned();
        assert_eq!(request_operation(&foreign_path), None);
    }

    #[test]
    fn operation_drop_requires_a_successful_matching_native_response() {
        let body = native_request(MessageHeader::response(
            Operation::StoragePutChunk,
            7,
            StatusCode::Ok,
            0,
        ))
        .body;
        let successful = FixtureHttpResponse {
            status: 200,
            reason: "OK",
            headers: Vec::new(),
            body,
        };
        assert!(native_response_succeeded(
            &successful,
            Operation::StoragePutChunk
        ));
        assert!(!native_response_succeeded(
            &successful,
            Operation::JobCommit
        ));

        let mut rejected = successful.clone();
        rejected.status = 401;
        assert!(!native_response_succeeded(
            &rejected,
            Operation::StoragePutChunk
        ));

        let mut native_failure = successful;
        native_failure.body = native_request(MessageHeader::response(
            Operation::StoragePutChunk,
            7,
            StatusCode::Conflict,
            0,
        ))
        .body;
        assert!(!native_response_succeeded(
            &native_failure,
            Operation::StoragePutChunk
        ));
    }
}
