//! Deterministic authenticated HTTP/CORS clock and health fixture for real browser tests.

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
use alumina_runtime::stack::{StackDomain, StackWatermarkFlags, StackWatermarkSnapshot};
use alumina_service::capability::CapabilityDocumentService;
use alumina_service::diagnostics::DiagnosticServiceState;
use alumina_service::health::{RuntimeHealthService, RuntimeQueueHealth};
use alumina_service::{NativeRequest, ResponseMedia, ServiceRequest, ServiceResponse};

use crate::capability;
use crate::diagnostics::{
    SIMULATED_DIAGNOSTIC_PROVIDERS, simulated_immediate_waveform_capture,
    simulated_resource_overview,
};

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
    runtime_health: RuntimeHealthService,
    runtime_health_epoch: Option<DeviceCycle>,
    runtime_health_samples: u32,
    diagnostics: FixtureDiagnosticService,
    simulated_telemetry_provider: bool,
    simulated_waveform_provider: bool,
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
            runtime_health: RuntimeHealthService::new(policy.frequency_hz.saturating_mul(2)),
            runtime_health_epoch: None,
            runtime_health_samples: 0,
            diagnostics: FixtureDiagnosticService::new(
                diagnostic_context,
                SIMULATED_DIAGNOSTIC_PROVIDERS,
                DiagnosticTransportLimits::native_control(),
                DiagnosticLimits::interactive(),
            ),
            simulated_telemetry_provider: false,
            simulated_waveform_provider: false,
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

    /// Enables deterministic completion of admitted immediate waveform captures.
    ///
    /// Tests default to explicit provider injection; the standalone HTTP
    /// simulator opts in so a production browser worker can exercise the full
    /// authenticated acquisition and range-download lifecycle.
    pub const fn enable_simulated_waveform_provider(&mut self) {
        self.simulated_waveform_provider = true;
    }

    /// Enables deterministic overview production for admitted telemetry polls.
    pub const fn enable_simulated_telemetry_provider(&mut self) {
        self.simulated_telemetry_provider = true;
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
        self.runtime_health = RuntimeHealthService::new(self.policy.frequency_hz.saturating_mul(2));
        self.runtime_health_epoch = None;
        self.runtime_health_samples = 0;
        self.diagnostics = FixtureDiagnosticService::new(
            diagnostic_context(boot_id, self.policy.frequency_hz)?,
            SIMULATED_DIAGNOSTIC_PROVIDERS,
            DiagnosticTransportLimits::native_control(),
            DiagnosticLimits::interactive(),
        );
        Ok(())
    }

    /// Applies firmware-equivalent route, CORS, HMAC, replay, clock, and health framing.
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
        let context = self.diagnostics.context();
        let device_id = lower_hex(&context.device_id.0);
        let capability_digest = lower_hex(&context.capability.digest.0);
        let body = format!(
            "{{\"protocol_version\":1,\"board_id\":\"{}\",\"credential_source\":\"development-fallback\",\"production_armable\":false,\"device_id\":\"{device_id}\",\"capability_digest\":\"{capability_digest}\",\"capability_document_bytes\":{}}}",
            capability::BOARD_ID,
            context.capability.byte_len
        )
        .into_bytes();
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
        let service = self.native_response(&request.body, receive_cycle, transmit_cycle);
        self.authenticated_response(metadata.proof.counter, metadata.origin, service)
    }

    fn native_response(
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
            let operation = native.message.operation;
            if self.simulated_telemetry_provider && operation == Operation::TelemetryPoll {
                let overview = self.diagnostics.telemetry_provider_request().and_then(
                    |(subscription, sequence)| {
                        simulated_resource_overview(subscription, sequence, transmit_cycle).ok()
                    },
                );
                if let Some(overview) = overview {
                    let _ = self.diagnostics.publish_overview(&overview);
                }
            }
            let Ok(request) = ServiceRequest::native(bytes) else {
                return ServiceResponse::invalid_native();
            };
            let response = self.diagnostics.dispatch(&request, transmit_cycle);
            if self.simulated_waveform_provider && operation == Operation::WaveformArm {
                let capture =
                    self.diagnostics
                        .armed_waveform_configuration()
                        .and_then(|configuration| {
                            simulated_immediate_waveform_capture(configuration, transmit_cycle)
                                .map_err(|_| {
                                    alumina_service::diagnostics::DiagnosticServiceError::Capacity
                                })
                        });
                if let Ok(capture) = capture {
                    let _ = self.diagnostics.retain_waveform_capture(&capture);
                }
            }
            return response;
        }
        if native.frame.kind == FrameKind::Capabilities {
            let Ok(request) = ServiceRequest::native(bytes) else {
                return ServiceResponse::invalid_native();
            };
            return CapabilityDocumentService::dispatch(
                &capability::package(),
                &request,
                transmit_cycle,
            );
        }
        if native.frame.kind == FrameKind::Health {
            let valid_snapshot_request = native.message.operation == Operation::HealthSnapshot
                && native.frame.config_digest == Digest::ZERO
                && native.body.is_empty();
            let Ok(request) = ServiceRequest::native(bytes) else {
                return ServiceResponse::invalid_native();
            };
            return self.runtime_health_response(&request, transmit_cycle, valid_snapshot_request);
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

    fn runtime_health_response(
        &mut self,
        request: &ServiceRequest,
        now: DeviceCycle,
        update_measurement: bool,
    ) -> ServiceResponse {
        const COMMAND_CAPACITY: u16 = 8;
        const WORK_CAPACITY: u16 = 8;
        const TELEMETRY_CAPACITY: u16 = 32;

        let command_free = u16::try_from(
            self.policy
                .command_queue_free
                .min(u32::from(COMMAND_CAPACITY)),
        )
        .expect("bounded command credits fit u16");
        let work_depth = u16::try_from(self.policy.work_queue_depth.min(u32::from(WORK_CAPACITY)))
            .expect("bounded work occupancy fits u16");
        let queues = RuntimeQueueHealth {
            command_depth: COMMAND_CAPACITY - command_free,
            command_capacity: COMMAND_CAPACITY,
            work_depth,
            work_capacity: WORK_CAPACITY,
            telemetry_depth: 1,
            telemetry_capacity: TELEMETRY_CAPACITY,
        };
        if !update_measurement {
            return self.runtime_health.dispatch(request, now, queues, None);
        }
        let Some(samples) = self.runtime_health_samples.checked_add(1) else {
            return self.runtime_health.dispatch(request, now, queues, None);
        };
        let epoch = *self.runtime_health_epoch.get_or_insert(now);
        let service_stack =
            simulated_stack_snapshot(StackDomain::ServiceCore, 20 * 1_024, samples, epoch, now);
        let realtime_stack =
            simulated_stack_snapshot(StackDomain::RealtimeCore, 24 * 1_024, samples, epoch, now);
        if self
            .runtime_health
            .observe_realtime(samples, now, now, realtime_stack)
            .is_ok()
        {
            self.runtime_health_samples = samples;
        }
        self.runtime_health
            .dispatch(request, now, queues, Some(service_stack))
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

fn simulated_stack_snapshot(
    domain: StackDomain,
    minimum_headroom_bytes: u32,
    samples: u32,
    epoch_cycle: DeviceCycle,
    sampled_at: DeviceCycle,
) -> StackWatermarkSnapshot {
    StackWatermarkSnapshot {
        domain,
        flags: StackWatermarkFlags(
            StackWatermarkFlags::INITIALIZED
                | StackWatermarkFlags::PARTIAL_BOOT_EPOCH
                | StackWatermarkFlags::CURRENT_POINTER_BOUND,
        ),
        allocated_bytes: 32 * 1_024,
        excluded_low_bytes: 256,
        painted_bytes: 28 * 1_024,
        minimum_headroom_bytes,
        samples,
        completed_sweeps: 0,
        epoch_cycle,
        sampled_at,
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
        capability: calculate_identity(&capability::package())
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
    use alumina_capability::{
        CapabilityReadRequest, CapabilityReadResponse, MAX_CAPABILITY_CHUNK_BYTES,
        calculate_identity,
    };
    use alumina_net::{
        AUTH_PROOF_HEADER, AUTH_TAG_HEX_BYTES, CORS_REQUEST_HEADERS_HEADER,
        CORS_REQUEST_METHOD_HEADER, HttpMethod, RequestProof, parse_request_proof, sign_request,
        verify_response_proof,
    };
    use alumina_protocol::{FrameHeader, MessageDirection, MessageHeader};
    use alumina_runtime::health::{RuntimeHealthFlags, RuntimeHealthSnapshot};

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
        native_request(
            counter,
            FrameKind::ClockSample,
            Operation::ClockHeartbeat,
            &clock,
        )
    }

    fn native_health_request(counter: u64) -> FixtureHttpRequest {
        native_request(counter, FrameKind::Health, Operation::HealthSnapshot, &[])
    }

    fn native_capability_request(
        counter: u64,
        expected_digest: Digest,
        offset: u32,
    ) -> FixtureHttpRequest {
        let body = CapabilityReadRequest {
            expected_digest,
            offset,
            maximum_bytes: u16::try_from(MAX_CAPABILITY_CHUNK_BYTES).unwrap(),
        }
        .encode()
        .unwrap();
        native_request(
            counter,
            FrameKind::Capabilities,
            Operation::CapabilitiesGet,
            &body,
        )
    }

    fn native_request(
        counter: u64,
        kind: FrameKind,
        operation: Operation,
        native_body: &[u8],
    ) -> FixtureHttpRequest {
        let message = MessageHeader::request(
            operation,
            u32::try_from(counter).unwrap(),
            u32::try_from(native_body.len()).unwrap(),
        );
        let payload_len = MessageHeader::WIRE_LEN + native_body.len();
        let frame = FrameHeader::new(
            kind,
            u32::try_from(payload_len).unwrap(),
            u32::try_from(counter).unwrap(),
            DeviceCycle(0),
            Digest::ZERO,
        );
        let mut body = Vec::new();
        body.extend_from_slice(&frame.encode());
        body.extend_from_slice(&message.encode());
        body.extend_from_slice(native_body);
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

        let identity_request = FixtureHttpRequest {
            method: HttpMethod::Get,
            path: "/api/v1/identity".to_owned(),
            headers: vec![(
                CORS_ORIGIN_HEADER.as_bytes().to_vec(),
                ORIGIN.as_bytes().to_vec(),
            )],
            body: Vec::new(),
        };
        let identity_response =
            fixture.handle(&identity_request, 0, DeviceCycle(0), DeviceCycle(0));
        let identity_text = core::str::from_utf8(&identity_response.body).unwrap();
        let capability = calculate_identity(&capability::package()).unwrap();
        assert_eq!(identity_response.status, 200);
        assert!(identity_text.contains("\"board_id\":\"sim-mks-tinybee-v1\""));
        assert!(identity_text.contains("\"device_id\":\"414c554d2d53494d3a54494e59424545\""));
        assert!(identity_text.contains(&format!(
            "\"capability_digest\":\"{}\"",
            lower_hex(&capability.digest.0)
        )));
        assert!(identity_text.contains(&format!(
            "\"capability_document_bytes\":{}",
            capability.byte_len
        )));

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
    fn authenticated_runtime_health_uses_production_service_and_monotonic_stacks() {
        let mut fixture = fixture();
        let first = fixture.handle(
            &native_health_request(72),
            10,
            DeviceCycle(1_000_000),
            DeviceCycle(1_000_100),
        );
        assert_eq!(first.status, 200);
        assert_eq!(fixture.accepted_probes(), 0);
        let counter = first
            .headers
            .iter()
            .find(|(name, _)| name == AUTH_COUNTER_HEADER)
            .map(|(_, value)| value.as_str())
            .unwrap();
        let tag = first
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
            &first.body,
        )
        .unwrap();
        let frame = FrameHeader::decode(&first.body[..FrameHeader::WIRE_LEN], 1_024).unwrap();
        assert_eq!(frame.kind, FrameKind::Health);
        let message = MessageHeader::decode_and_validate(
            &first.body[FrameHeader::WIRE_LEN..FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN],
            frame.kind,
            frame.payload_len,
        )
        .unwrap();
        assert_eq!(message.direction, MessageDirection::Response);
        assert_eq!(message.operation, Operation::HealthSnapshot);
        let first_snapshot = RuntimeHealthSnapshot::decode(
            &first.body[FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN..],
        )
        .unwrap();
        assert_eq!(first_snapshot.snapshot_cycle, DeviceCycle(1_000_100));
        assert_eq!(first_snapshot.command_queue_depth, 0);
        assert_eq!(first_snapshot.command_queue_capacity, 8);
        assert_eq!(first_snapshot.work_queue_depth, 0);
        assert_eq!(first_snapshot.telemetry_queue_depth, 1);
        assert_eq!(first_snapshot.service_stack.samples, 1);
        assert_eq!(first_snapshot.realtime_stack.samples, 1);
        assert_ne!(
            first_snapshot.flags.0 & RuntimeHealthFlags::REALTIME_STACK_PRESENT,
            0
        );
        assert_ne!(
            first_snapshot.flags.0 & RuntimeHealthFlags::REALTIME_STACK_FRESH,
            0
        );

        let second = fixture.handle(
            &native_health_request(73),
            11,
            DeviceCycle(1_010_000),
            DeviceCycle(1_010_100),
        );
        assert_eq!(second.status, 200);
        let second_snapshot = RuntimeHealthSnapshot::decode(
            &second.body[FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN..],
        )
        .unwrap();
        assert_eq!(second_snapshot.service_stack.samples, 2);
        assert_eq!(second_snapshot.realtime_stack.samples, 2);
        assert_eq!(
            second_snapshot.service_stack.epoch_cycle,
            first_snapshot.service_stack.epoch_cycle
        );
        assert!(second_snapshot.service_stack.sampled_at > first_snapshot.service_stack.sampled_at);
        assert!(
            second_snapshot.realtime_stack.sampled_at > first_snapshot.realtime_stack.sampled_at
        );
    }

    #[test]
    fn authenticated_capability_ranges_use_the_shared_production_dispatcher() {
        let mut fixture = fixture();
        let identity = calculate_identity(&capability::package()).unwrap();
        let first = fixture.handle(
            &native_capability_request(74, Digest::ZERO, 0),
            10,
            DeviceCycle(1_020_000),
            DeviceCycle(1_020_100),
        );
        assert_eq!(first.status, 200);
        let frame = FrameHeader::decode(&first.body[..FrameHeader::WIRE_LEN], 1_024).unwrap();
        assert_eq!(frame.kind, FrameKind::Capabilities);
        let message_end = FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN;
        let message = MessageHeader::decode_and_validate(
            &first.body[FrameHeader::WIRE_LEN..message_end],
            frame.kind,
            frame.payload_len,
        )
        .unwrap();
        assert_eq!(message.direction, MessageDirection::Response);
        assert_eq!(message.operation, Operation::CapabilitiesGet);
        assert_eq!(message.status, StatusCode::Ok);
        let (metadata, chunk) = CapabilityReadResponse::decode_body(&first.body[message_end..])
            .expect("shared service emits a canonical range body");
        assert_eq!(metadata.identity, identity);
        assert_eq!(metadata.offset, 0);
        assert_eq!(chunk.len(), MAX_CAPABILITY_CHUNK_BYTES);
        assert!(!metadata.complete);

        let second = fixture.handle(
            &native_capability_request(75, identity.digest, metadata.chunk_len.into()),
            11,
            DeviceCycle(1_030_000),
            DeviceCycle(1_030_100),
        );
        assert_eq!(second.status, 200);
        let (second_metadata, _) =
            CapabilityReadResponse::decode_body(&second.body[message_end..]).unwrap();
        assert_eq!(second_metadata.identity, identity);
        assert_eq!(second_metadata.offset, u32::from(metadata.chunk_len));
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
