//! Deterministic authenticated HTTP/CORS clock fixture for real browser tests.

use std::fmt;

use alumina_capability::calculate_identity;
use alumina_clock::{
    BootId, ClockFlags, ClockHeartbeatRequest, ClockHeartbeatResponse, ClockSource,
};
use alumina_diagnostics::transport::DiagnosticTransportLimits;
use alumina_diagnostics::{DiagnosticContext, DiagnosticLimits};
use alumina_net::{
    AUTH_COUNTER_HEADER, AUTH_DISCOVERY_BODY_BYTES, AUTH_DISCOVERY_CONTENT_LENGTH,
    AUTH_RESPONSE_HEADER, AuthError, AuthHeaderAccumulator, AuthRateLimit, AuthenticatedMedia,
    AuthenticationState, BootNonce, CORS_ALLOW_ORIGIN_HEADER, CORS_ALLOW_PRIVATE_NETWORK_HEADER,
    CORS_ORIGIN_HEADER, CorsOrigin, CorsPreflightAccumulator, HttpAdmissionError, HttpMethod,
    Route, classify_route, sign_response, write_lower_hex,
};
use alumina_protocol::{DeviceCycle, DeviceId, Digest, FrameKind, Operation, StatusCode};
use alumina_service::diagnostics::{DiagnosticProviderPolicy, DiagnosticServiceState};
use alumina_service::{NativeRequest, ResponseMedia, ServiceRequest, ServiceResponse};

const AUTHENTICATION_SCHEME: &str = "hmac-sha256-v2";
const NATIVE_FRAME_MEDIA_TYPE: &str = "application/vnd.alumina.frame";
const JSON_MEDIA_TYPE: &str = "application/json";
const EXPOSED_RESPONSE_HEADERS: &str = "X-Alumina-Counter, X-Alumina-Response-Authorization";
const ALLOWED_REQUEST_HEADERS: &str = "Content-Type, X-Alumina-Counter, X-Alumina-Authorization";
const PREFLIGHT_VARY: &str = "Origin, Access-Control-Request-Method, Access-Control-Request-Headers, Access-Control-Request-Private-Network";

/// Fixed diagnostic budgets exercised by the authenticated host fixture.
pub type FixtureDiagnosticService = DiagnosticServiceState<176, 432, 208, 2_048>;

/// One owned HTTP request after bounded socket parsing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureHttpRequest {
    /// Parsed exact method family.
    pub method: HttpMethod,
    /// Exact path without query aliases.
    pub path: String,
    /// Ordered raw field names and values with duplicates retained.
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
    /// Exact request body.
    pub body: Vec<u8>,
}

impl FixtureHttpRequest {
    /// Returns the first case-insensitive header value only when it is unique.
    pub fn unique_header(&self, name: &[u8]) -> Option<&[u8]> {
        let mut matches = self
            .headers
            .iter()
            .filter(|(candidate, _)| candidate.eq_ignore_ascii_case(name));
        let value = matches.next().map(|(_, value)| value.as_slice())?;
        matches.next().is_none().then_some(value)
    }
}

/// Complete HTTP response emitted by the simulator socket adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureHttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Canonical reason phrase used only by the host adapter.
    pub reason: &'static str,
    /// Ordered response headers.
    pub headers: Vec<(String, String)>,
    /// Exact response body.
    pub body: Vec<u8>,
}

impl FixtureHttpResponse {
    fn new(status: u16, reason: &'static str, content_type: Option<&str>, body: Vec<u8>) -> Self {
        let mut headers = Vec::new();
        if let Some(content_type) = content_type {
            headers.push(("Content-Type".to_owned(), content_type.to_owned()));
        }
        headers.push(("Content-Length".to_owned(), body.len().to_string()));
        headers.push(("Cache-Control".to_owned(), "no-store".to_owned()));
        headers.push(("X-Content-Type-Options".to_owned(), "nosniff".to_owned()));
        Self {
            status,
            reason,
            headers,
            body,
        }
    }

    fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    fn with_cors(self, origin: CorsOrigin) -> Self {
        self.with_header(CORS_ALLOW_ORIGIN_HEADER, origin.as_str())
            .with_header("Vary", CORS_ORIGIN_HEADER)
    }
}

/// Stable device facts exposed by one simulated boot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockFixturePolicy {
    /// Nominal counter frequency declared to the client.
    pub frequency_hz: u64,
    /// Minimum local lead required by future scheduling.
    pub minimum_lead_cycles: u64,
    /// Maximum local scheduling horizon.
    pub maximum_schedule_horizon_cycles: u64,
    /// Already queued future work horizon.
    pub queue_horizon_cycles: u64,
    /// Free bounded real-time command slots.
    pub command_queue_free: u32,
    /// Current service-to-real-time work depth.
    pub work_queue_depth: u32,
    /// Whether local deadline evidence is healthy.
    pub deadline_healthy: bool,
    /// Whether the simulated safety owner reports unhealthy state.
    pub safety_unhealthy: bool,
}

impl ClockFixturePolicy {
    /// Healthy one-megahertz diagnostic fixture used by browser integration.
    pub const HEALTHY_1MHZ: Self = Self {
        frequency_hz: 1_000_000,
        minimum_lead_cycles: 100_000,
        maximum_schedule_horizon_cycles: 60_000_000,
        queue_horizon_cycles: 0,
        command_queue_free: 8,
        work_queue_depth: 0,
        deadline_healthy: true,
        safety_unhealthy: false,
    };

    fn validate(self) -> Result<(), ClockFixtureError> {
        if self.frequency_hz == 0
            || self.minimum_lead_cycles == 0
            || self.maximum_schedule_horizon_cycles < self.minimum_lead_cycles
        {
            Err(ClockFixtureError::Policy)
        } else {
            Ok(())
        }
    }
}

/// Authenticated, replay-protected simulated MCU HTTP state.
pub struct ClockHttpFixture {
    secret: Vec<u8>,
    nonce: BootNonce,
    boot_id: BootId,
    authentication: AuthenticationState,
    policy: ClockFixturePolicy,
    accepted_probes: u64,
    diagnostics: FixtureDiagnosticService,
}

impl ClockHttpFixture {
    /// Creates one deterministic boot from the supplied nonzero 128-bit identity.
    ///
    /// # Errors
    ///
    /// Rejects an empty secret, zero boot identity, or invalid clock policy.
    pub fn new(
        secret: Vec<u8>,
        boot_bytes: [u8; 16],
        policy: ClockFixturePolicy,
    ) -> Result<Self, ClockFixtureError> {
        if secret.is_empty() {
            return Err(ClockFixtureError::Secret);
        }
        policy.validate()?;
        let nonce = BootNonce::new(boot_bytes).map_err(ClockFixtureError::Authentication)?;
        let boot_id = BootId::new(boot_bytes).map_err(|_| ClockFixtureError::Boot)?;
        let diagnostic_context = diagnostic_context(boot_id, policy.frequency_hz)?;
        let authentication = AuthenticationState::new(nonce, AuthRateLimit::INITIAL)
            .map_err(ClockFixtureError::Authentication)?;
        Ok(Self {
            secret,
            nonce,
            boot_id,
            authentication,
            policy,
            accepted_probes: 0,
            diagnostics: FixtureDiagnosticService::new(
                diagnostic_context,
                DiagnosticProviderPolicy::SIMULATED,
                DiagnosticTransportLimits::native_control(),
                DiagnosticLimits::interactive(),
            ),
        })
    }

    /// Number of authenticated canonical heartbeat requests accepted this boot.
    pub const fn accepted_probes(&self) -> u64 {
        self.accepted_probes
    }

    /// Borrow the same fixed diagnostic owner reached by authenticated control.
    pub const fn diagnostics(&self) -> &FixtureDiagnosticService {
        &self.diagnostics
    }

    /// Mutably borrow the diagnostic owner to inject deterministic simulator evidence.
    pub const fn diagnostics_mut(&mut self) -> &mut FixtureDiagnosticService {
        &mut self.diagnostics
    }

    /// Complete context accepted by the authenticated diagnostic owner.
    pub const fn diagnostic_context(&self) -> DiagnosticContext {
        self.diagnostics.context()
    }

    /// Reboots into an explicit new nonzero identity and clears replay state.
    ///
    /// # Errors
    ///
    /// Rejects a zero or unchanged boot identity.
    pub fn reboot(&mut self, boot_bytes: [u8; 16]) -> Result<(), ClockFixtureError> {
        let nonce = BootNonce::new(boot_bytes).map_err(ClockFixtureError::Authentication)?;
        let boot_id = BootId::new(boot_bytes).map_err(|_| ClockFixtureError::Boot)?;
        if nonce == self.nonce {
            return Err(ClockFixtureError::Boot);
        }
        self.nonce = nonce;
        self.boot_id = boot_id;
        self.authentication = AuthenticationState::new(nonce, AuthRateLimit::INITIAL)
            .map_err(ClockFixtureError::Authentication)?;
        self.accepted_probes = 0;
        self.diagnostics = FixtureDiagnosticService::new(
            diagnostic_context(boot_id, self.policy.frequency_hz)?,
            DiagnosticProviderPolicy::SIMULATED,
            DiagnosticTransportLimits::native_control(),
            DiagnosticLimits::interactive(),
        );
        Ok(())
    }

    /// Applies firmware-equivalent route, CORS, HMAC, replay, and clock framing.
    ///
    /// `receive_cycle` and `transmit_cycle` are exact samples from the host
    /// adapter's affine device clock. `now_ms` drives only the authentication
    /// token bucket.
    #[must_use]
    pub fn handle(
        &mut self,
        request: &FixtureHttpRequest,
        now_ms: u64,
        receive_cycle: DeviceCycle,
        transmit_cycle: DeviceCycle,
    ) -> FixtureHttpResponse {
        let route = classify_route(request.method, &request.path);
        match route {
            Route::CorsPreflight => self.preflight(request),
            Route::Authentication => self.authentication_discovery(request),
            Route::Identity => self.identity(request),
            Route::Health => self.health(request),
            Route::ControlCommand => self.control(request, now_ms, receive_cycle, transmit_cycle),
            Route::MethodNotAllowed => plain_response(405, "Method Not Allowed", "method\n"),
            Route::NotFound => plain_response(404, "Not Found", "not found\n"),
            Route::Bootstrap | Route::Network | Route::StorageStatus => {
                plain_response(501, "Not Implemented", "fixture route unavailable\n")
            }
        }
    }

    fn preflight(&self, request: &FixtureHttpRequest) -> FixtureHttpResponse {
        let mut accumulator = CorsPreflightAccumulator::new();
        if request
            .headers
            .iter()
            .try_for_each(|(name, value)| accumulator.observe(name, value))
            .is_err()
        {
            return plain_response(400, "Bad Request", "bad preflight\n");
        }
        let Ok(preflight) = accumulator.finish(&request.path) else {
            return plain_response(400, "Bad Request", "bad preflight\n");
        };
        let method = match preflight.target_method {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            _ => return plain_response(400, "Bad Request", "bad preflight\n"),
        };
        let mut response = FixtureHttpResponse::new(204, "No Content", None, Vec::new())
            .with_header(CORS_ALLOW_ORIGIN_HEADER, preflight.origin.as_str())
            .with_header("Access-Control-Allow-Methods", method)
            .with_header("Access-Control-Allow-Headers", ALLOWED_REQUEST_HEADERS)
            .with_header("Access-Control-Max-Age", "600")
            .with_header("Vary", PREFLIGHT_VARY);
        if preflight.private_network {
            response = response.with_header(CORS_ALLOW_PRIVATE_NETWORK_HEADER, "true");
        }
        response
    }

    fn authentication_discovery(&self, request: &FixtureHttpRequest) -> FixtureHttpResponse {
        let origin = public_origin(request);
        if request.body.is_empty() {
            let nonce = lower_hex(&self.nonce.as_bytes());
            let body = format!(
                "{{\"scheme\":\"{AUTHENTICATION_SCHEME}\",\"origin_bound\":true,\"boot_nonce\":\"{nonce}\",\"counter_window\":64,\"rate_burst\":32,\"rate_per_second\":50,\"request_proof_header\":\"X-Alumina-Authorization\",\"response_proof_header\":\"X-Alumina-Response-Authorization\"}}"
            )
            .into_bytes();
            if body.len() != AUTH_DISCOVERY_BODY_BYTES
                || body.len().to_string() != AUTH_DISCOVERY_CONTENT_LENGTH
            {
                return plain_response(500, "Internal Server Error", "fixture invariant\n");
            }
            let response = FixtureHttpResponse::new(200, "OK", Some(JSON_MEDIA_TYPE), body);
            return origin.map_or(response.clone(), |origin| response.with_cors(origin));
        }
        plain_response(400, "Bad Request", "unexpected body\n")
    }

    fn identity(&self, request: &FixtureHttpRequest) -> FixtureHttpResponse {
        let body = b"{\"protocol_version\":1,\"board_id\":\"sim-clock\",\"credential_source\":\"development-fallback\",\"production_armable\":false}".to_vec();
        let response = FixtureHttpResponse::new(200, "OK", Some(JSON_MEDIA_TYPE), body);
        public_origin(request).map_or(response.clone(), |origin| response.with_cors(origin))
    }

    fn health(&self, request: &FixtureHttpRequest) -> FixtureHttpResponse {
        let body = format!(
            "{{\"service_core\":0,\"network\":\"host-sim\",\"state\":\"boot\",\"accepted_probes\":{}}}",
            self.accepted_probes
        )
        .into_bytes();
        let response = FixtureHttpResponse::new(200, "OK", Some(JSON_MEDIA_TYPE), body);
        public_origin(request).map_or(response.clone(), |origin| response.with_cors(origin))
    }

    fn control(
        &mut self,
        request: &FixtureHttpRequest,
        now_ms: u64,
        receive_cycle: DeviceCycle,
        transmit_cycle: DeviceCycle,
    ) -> FixtureHttpResponse {
        let mut accumulator = AuthHeaderAccumulator::new();
        if request
            .headers
            .iter()
            .try_for_each(|(name, value)| accumulator.observe(name, value))
            .is_err()
        {
            return plain_response(400, "Bad Request", "bad headers\n");
        }
        let metadata = match accumulator.finish(request.method, Route::ControlCommand) {
            Ok(metadata) if metadata.body_len == request.body.len() => metadata,
            Ok(_) => return plain_response(400, "Bad Request", "body length\n"),
            Err(HttpAdmissionError::Credentials) => {
                return unauthorized_response(public_origin(request));
            }
            Err(_) => return plain_response(400, "Bad Request", "bad request\n"),
        };
        if let Err(error) =
            self.authentication
                .authorize(&self.secret, metadata, &request.body, now_ms)
        {
            return match error {
                AuthError::RateLimited => rate_limited_response(Some(metadata.origin)),
                _ => unauthorized_response(Some(metadata.origin)),
            };
        }
        let service = self.clock_response(&request.body, receive_cycle, transmit_cycle);
        self.authenticated_response(metadata.proof.counter, metadata.origin, service)
    }

    fn clock_response(
        &mut self,
        bytes: &[u8],
        receive_cycle: DeviceCycle,
        transmit_cycle: DeviceCycle,
    ) -> ServiceResponse {
        let Ok(native) = NativeRequest::decode(bytes) else {
            return ServiceResponse::invalid_native();
        };
        if matches!(
            native.frame.kind,
            FrameKind::Telemetry | FrameKind::Waveform
        ) {
            let Ok(request) = ServiceRequest::native(bytes) else {
                return ServiceResponse::invalid_native();
            };
            return self.diagnostics.dispatch(&request, transmit_cycle);
        }
        if native.frame.kind != FrameKind::ClockSample
            || native.message.operation != Operation::ClockHeartbeat
            || native.frame.config_digest != Digest::ZERO
        {
            return ServiceResponse::native(native, transmit_cycle, StatusCode::Unsupported, &[])
                .unwrap_or_else(|_| ServiceResponse::invalid_native());
        }
        let heartbeat = match ClockHeartbeatRequest::decode(native.body) {
            Ok(heartbeat) => heartbeat,
            Err(_) => {
                return ServiceResponse::native(
                    native,
                    transmit_cycle,
                    StatusCode::InvalidRequest,
                    &[],
                )
                .unwrap_or_else(|_| ServiceResponse::invalid_native());
            }
        };
        let mut flags = ClockFlags::MONOTONIC | ClockFlags::SHARED_BETWEEN_CORES;
        if self.policy.deadline_healthy {
            flags |= ClockFlags::DEADLINE_HEALTHY;
        }
        if self.policy.safety_unhealthy {
            flags |= ClockFlags::SAFETY_UNHEALTHY;
        }
        let response = ClockHeartbeatResponse {
            flags: ClockFlags(flags),
            counter_bits: 64,
            source: ClockSource::EmbassyMonotonic,
            probe_id: heartbeat.probe_id,
            ui_send_ns: heartbeat.ui_send_ns,
            boot_id: self.boot_id,
            receive_cycle,
            transmit_cycle,
            frequency_hz: self.policy.frequency_hz,
            minimum_lead_cycles: self.policy.minimum_lead_cycles,
            maximum_schedule_horizon_cycles: self.policy.maximum_schedule_horizon_cycles,
            queue_horizon_cycles: self.policy.queue_horizon_cycles,
            maximum_lateness_cycles: 0,
            missed_deadlines: 0,
            command_queue_free: self.policy.command_queue_free,
            work_queue_depth: self.policy.work_queue_depth,
        };
        let body = match response.encode() {
            Ok(body) => body,
            Err(_) => return ServiceResponse::invalid_native(),
        };
        self.accepted_probes = self.accepted_probes.saturating_add(1);
        ServiceResponse::native(native, transmit_cycle, StatusCode::Ok, &body)
            .unwrap_or_else(|_| ServiceResponse::invalid_native())
    }

    fn authenticated_response(
        &self,
        counter: u64,
        origin: CorsOrigin,
        response: ServiceResponse,
    ) -> FixtureHttpResponse {
        let (media, content_type) = match response.media {
            ResponseMedia::Json => (AuthenticatedMedia::Json, JSON_MEDIA_TYPE),
            ResponseMedia::NativeFrame => {
                (AuthenticatedMedia::NativeFrame, NATIVE_FRAME_MEDIA_TYPE)
            }
        };
        let Ok(proof) = sign_response(
            &self.secret,
            self.nonce,
            counter,
            response.http_status,
            media,
            origin,
            response.bytes(),
        ) else {
            return plain_response(500, "Internal Server Error", "fixture signing\n");
        };
        let tag = lower_hex(&proof.tag);
        FixtureHttpResponse::new(
            response.http_status,
            response_reason(response.http_status),
            Some(content_type),
            response.bytes().to_vec(),
        )
        .with_header(AUTH_COUNTER_HEADER, &counter.to_string())
        .with_header(AUTH_RESPONSE_HEADER, &tag)
        .with_header(CORS_ALLOW_ORIGIN_HEADER, origin.as_str())
        .with_header("Access-Control-Expose-Headers", EXPOSED_RESPONSE_HEADERS)
        .with_header("Vary", CORS_ORIGIN_HEADER)
    }
}

impl Drop for ClockHttpFixture {
    fn drop(&mut self) {
        self.secret.fill(0);
    }
}

fn diagnostic_context(
    boot_id: BootId,
    clock_frequency_hz: u64,
) -> Result<DiagnosticContext, ClockFixtureError> {
    Ok(DiagnosticContext {
        device_id: DeviceId(*b"ALUM-SIM:TINYBEE"),
        boot_id,
        capability: calculate_identity(&board_mks_tinybee::PACKAGE)
            .map_err(|_| ClockFixtureError::Capability)?,
        config_digest: Digest::ZERO,
        clock_frequency_hz,
    })
}

fn public_origin(request: &FixtureHttpRequest) -> Option<CorsOrigin> {
    let value = request.unique_header(CORS_ORIGIN_HEADER.as_bytes())?;
    let value = core::str::from_utf8(value).ok()?;
    CorsOrigin::parse(value).ok()
}

fn plain_response(status: u16, reason: &'static str, body: &str) -> FixtureHttpResponse {
    FixtureHttpResponse::new(status, reason, Some("text/plain"), body.as_bytes().to_vec())
}

fn unauthorized_response(origin: Option<CorsOrigin>) -> FixtureHttpResponse {
    let response = plain_response(401, "Unauthorized", "unauthorized\n")
        .with_header("WWW-Authenticate", "Alumina-HMAC-SHA256-V2");
    origin.map_or(response.clone(), |origin| response.with_cors(origin))
}

fn rate_limited_response(origin: Option<CorsOrigin>) -> FixtureHttpResponse {
    let response =
        plain_response(429, "Too Many Requests", "rate limited\n").with_header("Retry-After", "1");
    origin.map_or(response.clone(), |origin| response.with_cors(origin))
}

const fn response_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Service Response",
    }
}

fn lower_hex(bytes: &[u8]) -> String {
    let mut encoded = vec![0_u8; bytes.len() * 2];
    write_lower_hex(bytes, &mut encoded).expect("fixed lowercase hex output has exact capacity");
    String::from_utf8(encoded).expect("lowercase hexadecimal is UTF-8")
}

/// Invalid deterministic clock fixture construction or reboot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockFixtureError {
    /// Authentication secret was empty.
    Secret,
    /// Boot identity was zero or unchanged across reboot.
    Boot,
    /// Clock capability policy was inconsistent.
    Policy,
    /// Current TinyBee capability package could not produce an identity.
    Capability,
    /// Portable authentication state rejected its boot/rate facts.
    Authentication(AuthError),
}

impl fmt::Display for ClockFixtureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Secret => formatter.write_str("fixture secret must be nonempty"),
            Self::Boot => formatter.write_str("fixture boot identity is invalid"),
            Self::Policy => formatter.write_str("fixture clock policy is invalid"),
            Self::Capability => formatter.write_str("fixture capability identity is invalid"),
            Self::Authentication(error) => {
                write!(formatter, "fixture authentication policy failed: {error:?}")
            }
        }
    }
}

impl core::error::Error for ClockFixtureError {}

#[cfg(test)]
mod tests {
    use alumina_net::{
        AUTH_PROOF_HEADER, AUTH_TAG_HEX_BYTES, CORS_REQUEST_HEADERS_HEADER,
        CORS_REQUEST_METHOD_HEADER, HttpMethod, RequestProof, parse_request_proof, sign_request,
        verify_response_proof,
    };
    use alumina_protocol::{FrameHeader, MessageDirection, MessageHeader};

    use super::*;

    const SECRET: &[u8] = b"alumina-development";
    const ORIGIN: &str = "http://127.0.0.1:8097";

    fn fixture() -> ClockHttpFixture {
        ClockHttpFixture::new(
            SECRET.to_vec(),
            [0x31; 16],
            ClockFixturePolicy::HEALTHY_1MHZ,
        )
        .unwrap()
    }

    fn native_clock_request(counter: u64) -> FixtureHttpRequest {
        let clock = ClockHeartbeatRequest {
            probe_id: counter,
            ui_send_ns: counter * 1_000_000,
        }
        .encode()
        .unwrap();
        let message = MessageHeader::request(
            Operation::ClockHeartbeat,
            u32::try_from(counter).unwrap(),
            u32::try_from(clock.len()).unwrap(),
        );
        let payload_len = MessageHeader::WIRE_LEN + clock.len();
        let frame = FrameHeader::new(
            FrameKind::ClockSample,
            u32::try_from(payload_len).unwrap(),
            u32::try_from(counter).unwrap(),
            DeviceCycle(0),
            Digest::ZERO,
        );
        let mut body = Vec::new();
        body.extend_from_slice(&frame.encode());
        body.extend_from_slice(&message.encode());
        body.extend_from_slice(&clock);
        let origin = CorsOrigin::parse(ORIGIN).unwrap();
        let nonce = BootNonce::new([0x31; 16]).unwrap();
        let proof = sign_request(
            SECRET,
            nonce,
            counter,
            HttpMethod::Post,
            "/api/v1/control",
            origin,
            &body,
        )
        .unwrap();
        let mut tag = [0_u8; AUTH_TAG_HEX_BYTES];
        write_lower_hex(&proof.tag, &mut tag).unwrap();
        FixtureHttpRequest {
            method: HttpMethod::Post,
            path: "/api/v1/control".to_owned(),
            headers: vec![
                (
                    b"Content-Type".to_vec(),
                    NATIVE_FRAME_MEDIA_TYPE.as_bytes().to_vec(),
                ),
                (
                    b"Content-Length".to_vec(),
                    body.len().to_string().into_bytes(),
                ),
                (
                    AUTH_COUNTER_HEADER.as_bytes().to_vec(),
                    counter.to_string().into_bytes(),
                ),
                (AUTH_PROOF_HEADER.as_bytes().to_vec(), tag.to_vec()),
                (
                    CORS_ORIGIN_HEADER.as_bytes().to_vec(),
                    ORIGIN.as_bytes().to_vec(),
                ),
            ],
            body,
        }
    }

    #[test]
    fn public_challenge_and_cors_preflight_match_browser_contract() {
        let mut fixture = fixture();
        let auth = FixtureHttpRequest {
            method: HttpMethod::Get,
            path: "/api/v1/auth".to_owned(),
            headers: vec![(
                CORS_ORIGIN_HEADER.as_bytes().to_vec(),
                ORIGIN.as_bytes().to_vec(),
            )],
            body: Vec::new(),
        };
        let response = fixture.handle(&auth, 0, DeviceCycle(0), DeviceCycle(0));
        assert_eq!(response.status, 200);
        assert_eq!(response.body.len(), AUTH_DISCOVERY_BODY_BYTES);
        assert!(
            response
                .body
                .windows(32)
                .any(|window| window == b"31313131313131313131313131313131")
        );

        let preflight = FixtureHttpRequest {
            method: HttpMethod::Options,
            path: "/api/v1/control".to_owned(),
            headers: vec![
                (
                    CORS_ORIGIN_HEADER.as_bytes().to_vec(),
                    ORIGIN.as_bytes().to_vec(),
                ),
                (
                    CORS_REQUEST_METHOD_HEADER.as_bytes().to_vec(),
                    b"POST".to_vec(),
                ),
                (
                    CORS_REQUEST_HEADERS_HEADER.as_bytes().to_vec(),
                    b"content-type,x-alumina-counter,x-alumina-authorization".to_vec(),
                ),
            ],
            body: Vec::new(),
        };
        let response = fixture.handle(&preflight, 0, DeviceCycle(0), DeviceCycle(0));
        assert_eq!(response.status, 204);
        assert!(
            response
                .headers
                .iter()
                .any(|(name, value)| { name == CORS_ALLOW_ORIGIN_HEADER && value == ORIGIN })
        );
    }

    #[test]
    fn authenticated_clock_response_is_native_signed_and_replay_protected() {
        let mut fixture = fixture();
        let request = native_clock_request(71);
        let response = fixture.handle(&request, 10, DeviceCycle(1_000_000), DeviceCycle(1_000_100));
        assert_eq!(response.status, 200);
        assert_eq!(fixture.accepted_probes(), 1);
        let counter = response
            .headers
            .iter()
            .find(|(name, _)| name == AUTH_COUNTER_HEADER)
            .map(|(_, value)| value.as_str())
            .unwrap();
        let tag = response
            .headers
            .iter()
            .find(|(name, _)| name == AUTH_RESPONSE_HEADER)
            .map(|(_, value)| value.as_str())
            .unwrap();
        let RequestProof { counter, tag } = parse_request_proof(counter, tag).unwrap();
        verify_response_proof(
            SECRET,
            BootNonce::new([0x31; 16]).unwrap(),
            alumina_net::ResponseProof { counter, tag },
            200,
            AuthenticatedMedia::NativeFrame,
            CorsOrigin::parse(ORIGIN).unwrap(),
            &response.body,
        )
        .unwrap();
        let frame = FrameHeader::decode(&response.body[..FrameHeader::WIRE_LEN], 1_024).unwrap();
        let message = MessageHeader::decode_and_validate(
            &response.body[FrameHeader::WIRE_LEN..FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN],
            frame.kind,
            frame.payload_len,
        )
        .unwrap();
        assert_eq!(message.direction, MessageDirection::Response);
        assert_eq!(message.operation, Operation::ClockHeartbeat);
        let clock = ClockHeartbeatResponse::decode(
            &response.body[FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN..],
        )
        .unwrap();
        assert_eq!(clock.receive_cycle, DeviceCycle(1_000_000));
        assert_eq!(clock.transmit_cycle, DeviceCycle(1_000_100));

        let replay = fixture.handle(&request, 11, DeviceCycle(1_001_000), DeviceCycle(1_001_100));
        assert_eq!(replay.status, 401);
        assert_eq!(fixture.accepted_probes(), 1);
    }

    #[test]
    fn reboot_invalidates_old_authentication_and_clock_identity() {
        let mut fixture = fixture();
        fixture.reboot([0x52; 16]).unwrap();
        let rejected = fixture.handle(
            &native_clock_request(90),
            20,
            DeviceCycle(2_000_000),
            DeviceCycle(2_000_100),
        );
        assert_eq!(rejected.status, 401);
        assert_eq!(fixture.accepted_probes(), 0);
    }
}
