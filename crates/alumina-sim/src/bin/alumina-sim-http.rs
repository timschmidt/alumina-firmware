//! Localhost-only authenticated clock MCU used by real-browser qualification.

use core::fmt;
use core::net::SocketAddr;
use core::time::Duration;
use std::io::{self, BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::time::Instant;

use alumina_net::{HttpMethod, MAX_AUTHENTICATED_BODY_BYTES};
use alumina_protocol::DeviceCycle;
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
    secret: Vec<u8>,
    processing_delay_ms: u64,
    response_delay_ms: u64,
    drift_ppm: i32,
    drop_initial_control_requests: u64,
    drop_control_request: Option<u64>,
    reboot_control_request: Option<u64>,
}

impl ServerOptions {
    fn parse() -> Result<Self, ServerError> {
        let mut options = Self {
            bind: DEFAULT_BIND
                .parse()
                .expect("static socket address is valid"),
            secret: DEFAULT_SECRET.as_bytes().to_vec(),
            processing_delay_ms: 1,
            response_delay_ms: 2,
            drift_ppm: 37,
            drop_initial_control_requests: 0,
            drop_control_request: None,
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
        "alumina-sim-http [--bind IP:PORT] [--secret TEXT] \
         [--processing-delay-ms N] [--response-delay-ms N] [--drift-ppm N] \
         [--drop-initial-control-requests N] [--drop-control-request N] \
         [--reboot-control-request N]"
    );
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
    rebooted: bool,
}

fn main() -> Result<(), ServerError> {
    let options = ServerOptions::parse()?;
    let listener = TcpListener::bind(options.bind)?;
    let clock = AffineHostClock::new(
        ClockFixturePolicy::HEALTHY_1MHZ.frequency_hz,
        options.drift_ppm,
    );
    let mut fixture = ClockHttpFixture::new(
        options.secret.clone(),
        [0x31; 16],
        ClockFixturePolicy::HEALTHY_1MHZ,
    )
    .map_err(|error| ServerError::Fixture(error.to_string()))?;
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
    if initial_outage || single_drop {
        return Ok(());
    }
    if !options.response_delay_ms.eq(&0) {
        std::thread::sleep(Duration::from_millis(options.response_delay_ms));
    }
    write_response(stream, &response)
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
