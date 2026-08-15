//! Single-owner, fixed-memory diagnostic transport state.

use alumina_diagnostics::transport::{
    DiagnosticTransportError, DiagnosticTransportLimits, TelemetryEvent, TelemetryPhase,
    TelemetryPollRequest, TelemetrySessionRequest, TelemetrySubscribeView,
    TelemetrySubscriptionStatus, WaveformChunk, WaveformConfigureView, WaveformPhase,
    WaveformReadRequest, WaveformSessionRequest, WaveformStatus, decode_telemetry_subscribe,
    decode_waveform_configure, encode_telemetry_event, encode_waveform_chunk,
    validate_retained_capture,
};
use alumina_diagnostics::{
    CaptureQualityFlags, DiagnosticContext, DiagnosticLimits, decode_digital_capture,
    decode_resource_overview, digital_capture_encoded_len,
};
use alumina_protocol::{DeviceCycle, Digest, FrameKind, Operation, StatusCode};

use crate::{NativeRequest, ServiceRequest, ServiceRequestKind, ServiceResponse};

/// Largest native operation body admitted by the current authenticated HTTP
/// request envelope (`1148 - 56-byte frame - 16-byte message`).
pub const MAX_NATIVE_DIAGNOSTIC_REQUEST_BYTES: usize = 1_076;
/// Deliberately retained waveform chunk envelope bound (144-byte header plus
/// at most 168 range bytes), independent of the larger telemetry response slot.
pub const MAX_NATIVE_WAVEFORM_CHUNK_ENVELOPE_BYTES: usize = 312;
/// Largest operation body fitting the enlarged bounded native service response.
pub const MAX_NATIVE_DIAGNOSTIC_RESPONSE_BODY_BYTES: usize = crate::MAX_SERVICE_RESPONSE_BYTES
    - alumina_protocol::FrameHeader::WIRE_LEN
    - alumina_protocol::MessageHeader::WIRE_LEN;

/// Service-side diagnostic operation rejection with stable protocol mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticServiceError {
    /// Canonical operation body was malformed or violated admission policy.
    Invalid(DiagnosticTransportError),
    /// Exact session identity was not present.
    NotFound,
    /// Identity was reused with different canonical content or acknowledgement.
    Conflict,
    /// Another unreleased fixed-memory session occupies the service.
    Busy,
    /// Lifecycle state forbids the requested operation.
    ForbiddenState,
    /// Fixed storage, event, counter, or response capacity was exhausted.
    Capacity,
    /// Requested trigger or acquisition deadline has already elapsed.
    Deadline,
    /// This composition has no qualified overview or acquisition provider.
    Unsupported,
}

impl DiagnosticServiceError {
    /// Stable native protocol status returned for this rejection.
    pub const fn status_code(self) -> StatusCode {
        match self {
            Self::Invalid(DiagnosticTransportError::Integrity) => StatusCode::Integrity,
            Self::Invalid(_) => StatusCode::InvalidRequest,
            Self::NotFound => StatusCode::NotFound,
            Self::Conflict => StatusCode::Conflict,
            Self::Busy => StatusCode::Busy,
            Self::ForbiddenState => StatusCode::ForbiddenState,
            Self::Capacity => StatusCode::Capacity,
            Self::Deadline => StatusCode::Deadline,
            Self::Unsupported => StatusCode::Unsupported,
        }
    }
}

/// Compile-time composition policy for evidence-producing providers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticProviderPolicy {
    /// A resource owner can assemble canonical overview samples.
    pub resource_overview: bool,
    /// A capture owner can execute and retain configured digital acquisitions.
    pub digital_capture: bool,
}

impl DiagnosticProviderPolicy {
    /// No provider is connected; protocol operations fail honestly as unsupported.
    pub const NONE: Self = Self {
        resource_overview: false,
        digital_capture: false,
    };
    /// Deterministic host providers used only by explicit simulation fixtures.
    pub const SIMULATED: Self = Self {
        resource_overview: true,
        digital_capture: true,
    };
}

impl From<DiagnosticTransportError> for DiagnosticServiceError {
    fn from(error: DiagnosticTransportError) -> Self {
        Self::Invalid(error)
    }
}

#[derive(Debug)]
struct TelemetrySession<const REQUEST_BYTES: usize, const EVENT_BYTES: usize> {
    occupied: bool,
    phase: TelemetryPhase,
    request_len: usize,
    request: [u8; REQUEST_BYTES],
    next_event_sequence: u64,
    published_events: u64,
    last_acknowledged_sequence: u64,
    dropped_events: u64,
    last_event_cycle: DeviceCycle,
    pending_len: usize,
    pending_event: [u8; EVENT_BYTES],
}

impl<const REQUEST_BYTES: usize, const EVENT_BYTES: usize>
    TelemetrySession<REQUEST_BYTES, EVENT_BYTES>
{
    const fn empty() -> Self {
        Self {
            occupied: false,
            phase: TelemetryPhase::Unsubscribed,
            request_len: 0,
            request: [0; REQUEST_BYTES],
            next_event_sequence: 1,
            published_events: 0,
            last_acknowledged_sequence: 0,
            dropped_events: 0,
            last_event_cycle: DeviceCycle(0),
            pending_len: 0,
            pending_event: [0; EVENT_BYTES],
        }
    }

    fn view(
        &self,
        limits: DiagnosticTransportLimits,
    ) -> Result<TelemetrySubscribeView<'_>, DiagnosticServiceError> {
        if !self.occupied {
            return Err(DiagnosticServiceError::NotFound);
        }
        decode_telemetry_subscribe(&self.request[..self.request_len], limits)
            .map_err(DiagnosticServiceError::Invalid)
    }

    fn status(
        &self,
        limits: DiagnosticTransportLimits,
    ) -> Result<TelemetrySubscriptionStatus, DiagnosticServiceError> {
        let view = self.view(limits)?;
        Ok(TelemetrySubscriptionStatus {
            phase: self.phase,
            pending: self.pending_len != 0,
            subscription_id: view.subscription_id(),
            subscription_digest: view.digest(),
            minimum_period_cycles: view.minimum_period_cycles(),
            maximum_event_bytes: view.maximum_event_bytes(),
            resource_count: u16::try_from(view.resource_count())
                .map_err(|_| DiagnosticServiceError::Capacity)?,
            next_event_sequence: self.next_event_sequence,
            published_events: self.published_events,
            dropped_events: self.dropped_events,
            last_event_cycle: self.last_event_cycle,
            pending_event_sequence: if self.pending_len == 0 {
                0
            } else {
                self.next_event_sequence - 1
            },
            pending_event_bytes: u32::try_from(self.pending_len)
                .map_err(|_| DiagnosticServiceError::Capacity)?,
        })
    }

    fn matches(
        &self,
        reference: TelemetrySessionRequest,
        limits: DiagnosticTransportLimits,
    ) -> Result<(), DiagnosticServiceError> {
        let view = self.view(limits)?;
        if reference.subscription_id != view.subscription_id() {
            return Err(DiagnosticServiceError::NotFound);
        }
        if reference.subscription_digest != view.digest() {
            return Err(DiagnosticServiceError::Conflict);
        }
        Ok(())
    }
}

#[derive(Debug)]
struct WaveformSession<const CONFIGURE_BYTES: usize, const RECORD_BYTES: usize> {
    occupied: bool,
    phase: WaveformPhase,
    configure_len: usize,
    configure: [u8; CONFIGURE_BYTES],
    generation: u64,
    arm_cycle: DeviceCycle,
    trigger_cycle: DeviceCycle,
    triggered: bool,
    record_len: usize,
    record: [u8; RECORD_BYTES],
    record_digest: Digest,
    published_bytes: u32,
    dropped_chunks: u64,
    quality_flags: CaptureQualityFlags,
}

impl<const CONFIGURE_BYTES: usize, const RECORD_BYTES: usize>
    WaveformSession<CONFIGURE_BYTES, RECORD_BYTES>
{
    const fn empty() -> Self {
        Self {
            occupied: false,
            phase: WaveformPhase::Stopped,
            configure_len: 0,
            configure: [0; CONFIGURE_BYTES],
            generation: 0,
            arm_cycle: DeviceCycle(0),
            trigger_cycle: DeviceCycle(0),
            triggered: false,
            record_len: 0,
            record: [0; RECORD_BYTES],
            record_digest: Digest::ZERO,
            published_bytes: 0,
            dropped_chunks: 0,
            quality_flags: CaptureQualityFlags(0),
        }
    }

    fn view(
        &self,
        limits: DiagnosticTransportLimits,
    ) -> Result<WaveformConfigureView<'_>, DiagnosticServiceError> {
        if !self.occupied {
            return Err(DiagnosticServiceError::NotFound);
        }
        decode_waveform_configure(&self.configure[..self.configure_len], limits)
            .map_err(DiagnosticServiceError::Invalid)
    }

    fn matches(
        &self,
        reference: WaveformSessionRequest,
        limits: DiagnosticTransportLimits,
    ) -> Result<(), DiagnosticServiceError> {
        let view = self.view(limits)?;
        if reference.capture_id != view.capture_id() {
            return Err(DiagnosticServiceError::NotFound);
        }
        if reference.configure_digest != view.digest() {
            return Err(DiagnosticServiceError::Conflict);
        }
        Ok(())
    }

    fn status(
        &self,
        limits: DiagnosticTransportLimits,
    ) -> Result<WaveformStatus, DiagnosticServiceError> {
        let view = self.view(limits)?;
        Ok(WaveformStatus {
            phase: self.phase,
            triggered: self.triggered,
            capture_id: view.capture_id(),
            configure_digest: view.digest(),
            generation: self.generation,
            arm_cycle: self.arm_cycle,
            trigger_cycle: self.trigger_cycle,
            record_bytes: u32::try_from(self.record_len)
                .map_err(|_| DiagnosticServiceError::Capacity)?,
            published_bytes: self.published_bytes,
            record_digest: self.record_digest,
            dropped_chunks: self.dropped_chunks,
            quality_flags: self.quality_flags,
        })
    }
}

/// Fixed-memory, single-owner diagnostic state used by core 0 and simulation.
///
/// `REQUEST_BYTES` and `CONFIGURE_BYTES` should not exceed
/// [`MAX_NATIVE_DIAGNOSTIC_REQUEST_BYTES`]. `EVENT_BYTES` and `RECORD_BYTES`
/// are explicit board/composition memory budgets.
#[derive(Debug)]
pub struct DiagnosticServiceState<
    const REQUEST_BYTES: usize,
    const EVENT_BYTES: usize,
    const CONFIGURE_BYTES: usize,
    const RECORD_BYTES: usize,
> {
    context: DiagnosticContext,
    providers: DiagnosticProviderPolicy,
    transport_limits: DiagnosticTransportLimits,
    record_limits: DiagnosticLimits,
    telemetry: TelemetrySession<REQUEST_BYTES, EVENT_BYTES>,
    waveform: WaveformSession<CONFIGURE_BYTES, RECORD_BYTES>,
    next_waveform_generation: u64,
}

impl<
    const REQUEST_BYTES: usize,
    const EVENT_BYTES: usize,
    const CONFIGURE_BYTES: usize,
    const RECORD_BYTES: usize,
> DiagnosticServiceState<REQUEST_BYTES, EVENT_BYTES, CONFIGURE_BYTES, RECORD_BYTES>
{
    /// Creates empty service state with explicit decode and memory policy.
    pub const fn new(
        context: DiagnosticContext,
        providers: DiagnosticProviderPolicy,
        transport_limits: DiagnosticTransportLimits,
        record_limits: DiagnosticLimits,
    ) -> Self {
        Self {
            context,
            providers,
            transport_limits,
            record_limits,
            telemetry: TelemetrySession::empty(),
            waveform: WaveformSession::empty(),
            next_waveform_generation: 1,
        }
    }

    /// Complete authority against which every session context is checked.
    pub const fn context(&self) -> DiagnosticContext {
        self.context
    }

    /// Evidence providers installed by this board/simulator composition.
    pub const fn providers(&self) -> DiagnosticProviderPolicy {
        self.providers
    }

    /// Rebinds to a changed boot/configuration authority and atomically drops
    /// every prior session, pending event, and retained capture.
    ///
    /// Returns `true` when invalidation occurred. Callers should invoke this
    /// before dispatch whenever the active configuration digest may change.
    pub fn rebind_context(&mut self, context: DiagnosticContext) -> bool {
        if self.context == context {
            return false;
        }
        self.context = context;
        self.telemetry = TelemetrySession::empty();
        self.waveform = WaveformSession::empty();
        self.next_waveform_generation = 1;
        true
    }

    /// Whether one queued authenticated request belongs to this service.
    pub fn handles(request: &ServiceRequest) -> bool {
        request.kind() == ServiceRequestKind::NativeFrame
            && NativeRequest::decode(request.bytes()).is_ok_and(|native| {
                matches!(
                    native.frame.kind,
                    FrameKind::Telemetry | FrameKind::Waveform
                )
            })
    }

    /// Dispatches one authenticated telemetry/waveform request and always
    /// preserves its frame sequence, correlation identity, and operation.
    pub fn dispatch(&mut self, request: &ServiceRequest, now: DeviceCycle) -> ServiceResponse {
        let Ok(native) = NativeRequest::decode(request.bytes()) else {
            return ServiceResponse::invalid_native();
        };
        if !matches!(
            native.frame.kind,
            FrameKind::Telemetry | FrameKind::Waveform
        ) {
            return ServiceResponse::invalid_native();
        }
        if native.frame.config_digest != self.context.config_digest {
            return native_response(native, now, StatusCode::Conflict, &[]);
        }
        let mut response_body = [0_u8; MAX_NATIVE_DIAGNOSTIC_RESPONSE_BODY_BYTES];
        let result = match native.message.operation {
            Operation::TelemetrySubscribe => self
                .subscribe(native.body)
                .and_then(|status| encode_response(status.encode(), &mut response_body)),
            Operation::TelemetryUnsubscribe => TelemetrySessionRequest::decode(native.body)
                .map_err(DiagnosticServiceError::Invalid)
                .and_then(|reference| self.unsubscribe(reference))
                .and_then(|status| encode_response(status.encode(), &mut response_body)),
            Operation::TelemetryStatus => TelemetrySessionRequest::decode(native.body)
                .map_err(DiagnosticServiceError::Invalid)
                .and_then(|reference| self.telemetry_status(reference))
                .and_then(|status| encode_response(status.encode(), &mut response_body)),
            Operation::TelemetryPoll => TelemetryPollRequest::decode(native.body)
                .map_err(DiagnosticServiceError::Invalid)
                .and_then(|poll| self.poll_telemetry_event(poll, &mut response_body)),
            Operation::WaveformConfigure => self
                .configure_waveform(native.body)
                .and_then(|status| encode_response(status.encode(), &mut response_body)),
            Operation::WaveformArm => WaveformSessionRequest::decode(native.body)
                .map_err(DiagnosticServiceError::Invalid)
                .and_then(|reference| self.arm_waveform(reference, now))
                .and_then(|status| encode_response(status.encode(), &mut response_body)),
            Operation::WaveformStop => WaveformSessionRequest::decode(native.body)
                .map_err(DiagnosticServiceError::Invalid)
                .and_then(|reference| self.stop_waveform(reference))
                .and_then(|status| encode_response(status.encode(), &mut response_body)),
            Operation::WaveformStatus => WaveformSessionRequest::decode(native.body)
                .map_err(DiagnosticServiceError::Invalid)
                .and_then(|reference| self.waveform_status(reference))
                .and_then(|status| encode_response(status.encode(), &mut response_body)),
            Operation::WaveformRead => {
                WaveformReadRequest::decode(native.body, self.transport_limits)
                    .map_err(DiagnosticServiceError::Invalid)
                    .and_then(|read| self.read_waveform(read, &mut response_body))
            }
            _ => return native_response(native, now, StatusCode::Unsupported, &[]),
        };
        match result {
            Ok(response_len) => {
                native_response(native, now, StatusCode::Ok, &response_body[..response_len])
            }
            Err(error) => native_response(native, now, error.status_code(), &[]),
        }
    }

    /// Admits or idempotently reconciles one canonical subscription request.
    pub fn subscribe(
        &mut self,
        encoded: &[u8],
    ) -> Result<TelemetrySubscriptionStatus, DiagnosticServiceError> {
        if !self.providers.resource_overview {
            return Err(DiagnosticServiceError::Unsupported);
        }
        if encoded.len() > REQUEST_BYTES || encoded.len() > MAX_NATIVE_DIAGNOSTIC_REQUEST_BYTES {
            return Err(DiagnosticServiceError::Capacity);
        }
        let incoming = decode_telemetry_subscribe(encoded, self.transport_limits)?;
        if incoming.context() != self.context {
            return Err(DiagnosticServiceError::Conflict);
        }
        if usize::try_from(incoming.maximum_event_bytes())
            .map_err(|_| DiagnosticServiceError::Capacity)?
            > EVENT_BYTES
        {
            return Err(DiagnosticServiceError::Capacity);
        }
        if self.telemetry.occupied {
            let current = self.telemetry.view(self.transport_limits)?;
            if current.subscription_id() == incoming.subscription_id() {
                if current.digest() == incoming.digest() {
                    return self.telemetry.status(self.transport_limits);
                }
                return Err(DiagnosticServiceError::Conflict);
            }
            if self.telemetry.phase == TelemetryPhase::Active {
                return Err(DiagnosticServiceError::Busy);
            }
        }
        self.telemetry.request.fill(0);
        self.telemetry.request[..encoded.len()].copy_from_slice(encoded);
        self.telemetry.request_len = encoded.len();
        self.telemetry.occupied = true;
        self.telemetry.phase = TelemetryPhase::Active;
        self.telemetry.next_event_sequence = 1;
        self.telemetry.published_events = 0;
        self.telemetry.last_acknowledged_sequence = 0;
        self.telemetry.dropped_events = 0;
        self.telemetry.last_event_cycle = DeviceCycle(0);
        self.telemetry.pending_len = 0;
        self.telemetry.status(self.transport_limits)
    }

    /// Fetches status only when both session identity and request digest match.
    pub fn telemetry_status(
        &self,
        reference: TelemetrySessionRequest,
    ) -> Result<TelemetrySubscriptionStatus, DiagnosticServiceError> {
        self.telemetry.matches(reference, self.transport_limits)?;
        self.telemetry.status(self.transport_limits)
    }

    /// Idempotently removes an exact subscription and discards any pending event.
    pub fn unsubscribe(
        &mut self,
        reference: TelemetrySessionRequest,
    ) -> Result<TelemetrySubscriptionStatus, DiagnosticServiceError> {
        self.telemetry.matches(reference, self.transport_limits)?;
        if self.telemetry.phase == TelemetryPhase::Active {
            if self.telemetry.pending_len != 0 {
                self.telemetry.dropped_events = self
                    .telemetry
                    .dropped_events
                    .checked_add(1)
                    .ok_or(DiagnosticServiceError::Capacity)?;
            }
            self.telemetry.pending_len = 0;
            self.telemetry.phase = TelemetryPhase::Unsubscribed;
        }
        self.telemetry.status(self.transport_limits)
    }

    /// Publishes one complete overview into the latest-only event slot.
    ///
    /// The overview sequence must equal the status record's
    /// `next_event_sequence`. Replacing an unacknowledged event increments the
    /// exact cumulative drop count before the replacement is encoded.
    pub fn publish_overview(&mut self, overview: &[u8]) -> Result<(), DiagnosticServiceError> {
        if self.telemetry.phase != TelemetryPhase::Active {
            return Err(DiagnosticServiceError::ForbiddenState);
        }
        let decoded = decode_resource_overview(overview, self.record_limits).map_err(|error| {
            DiagnosticServiceError::Invalid(DiagnosticTransportError::Record(error))
        })?;
        let sequence = self.telemetry.next_event_sequence;
        if decoded.sequence() != sequence {
            return Err(DiagnosticServiceError::Conflict);
        }
        if sequence > 1 {
            let period = self
                .telemetry
                .view(self.transport_limits)?
                .minimum_period_cycles();
            if decoded
                .snapshot_cycle()
                .0
                .checked_sub(self.telemetry.last_event_cycle.0)
                .is_none_or(|elapsed| elapsed < period)
            {
                return Err(DiagnosticServiceError::Deadline);
            }
        }
        let dropped_events = if self.telemetry.pending_len == 0 {
            self.telemetry.dropped_events
        } else {
            self.telemetry
                .dropped_events
                .checked_add(1)
                .ok_or(DiagnosticServiceError::Capacity)?
        };
        let next_event_sequence = sequence
            .checked_add(1)
            .ok_or(DiagnosticServiceError::Capacity)?;
        let request_len = self.telemetry.request_len;
        let subscription = decode_telemetry_subscribe(
            &self.telemetry.request[..request_len],
            self.transport_limits,
        )?;
        let event = TelemetryEvent {
            subscription_id: subscription.subscription_id(),
            subscription_digest: subscription.digest(),
            event_sequence: sequence,
            dropped_events,
            overview,
        };
        let pending_len = encode_telemetry_event(
            &event,
            subscription,
            &mut self.telemetry.pending_event,
            self.record_limits,
        )?;
        self.telemetry.pending_len = pending_len;
        self.telemetry.next_event_sequence = next_event_sequence;
        self.telemetry.dropped_events = dropped_events;
        self.telemetry.last_event_cycle = decoded.snapshot_cycle();
        Ok(())
    }

    /// Returns the exact pending event without acknowledging transport delivery.
    pub fn pending_telemetry_event(&self) -> Option<&[u8]> {
        (self.telemetry.pending_len != 0)
            .then_some(&self.telemetry.pending_event[..self.telemetry.pending_len])
    }

    /// Borrows the admitted subscription and next sequence only while a
    /// provider may safely create a new latest-only sample.
    pub fn telemetry_provider_request(&self) -> Option<(TelemetrySubscribeView<'_>, u64)> {
        if self.telemetry.phase != TelemetryPhase::Active || self.telemetry.pending_len != 0 {
            return None;
        }
        self.telemetry
            .view(self.transport_limits)
            .ok()
            .map(|subscription| (subscription, self.telemetry.next_event_sequence))
    }

    /// Acknowledges only caller-admitted evidence and returns the current
    /// retained event without consuming it. Repeating a poll after a lost
    /// response therefore returns the same event or a strictly newer one.
    pub fn poll_telemetry_event(
        &mut self,
        request: TelemetryPollRequest,
        output: &mut [u8],
    ) -> Result<usize, DiagnosticServiceError> {
        self.telemetry
            .matches(request.reference, self.transport_limits)?;
        if self.telemetry.phase != TelemetryPhase::Active {
            return Err(DiagnosticServiceError::ForbiddenState);
        }
        let accepted = request.accepted_event_sequence;
        let pending_sequence = if self.telemetry.pending_len == 0 {
            None
        } else {
            self.telemetry.next_event_sequence.checked_sub(1)
        };
        if accepted == 0 {
            // Zero makes no acknowledgement claim. This lets a freshly
            // reconstructed client reattach to the same exact subscription
            // after a page/worker loss without consuming unseen evidence.
        } else if pending_sequence == Some(accepted) {
            self.acknowledge_telemetry_event(request.reference, accepted)?;
        } else if accepted != self.telemetry.last_acknowledged_sequence {
            return Err(DiagnosticServiceError::Conflict);
        }
        let Some(event) = self.pending_telemetry_event() else {
            return Ok(0);
        };
        if event.len() > output.len() {
            return Err(DiagnosticServiceError::Capacity);
        }
        output[..event.len()].copy_from_slice(event);
        Ok(event.len())
    }

    /// Acknowledges exactly the currently pending event after successful send.
    pub fn acknowledge_telemetry_event(
        &mut self,
        reference: TelemetrySessionRequest,
        event_sequence: u64,
    ) -> Result<TelemetrySubscriptionStatus, DiagnosticServiceError> {
        self.telemetry.matches(reference, self.transport_limits)?;
        if event_sequence == self.telemetry.last_acknowledged_sequence {
            return self.telemetry.status(self.transport_limits);
        }
        if self.telemetry.pending_len == 0
            || event_sequence != self.telemetry.next_event_sequence - 1
        {
            return Err(DiagnosticServiceError::Conflict);
        }
        self.telemetry.published_events = self
            .telemetry
            .published_events
            .checked_add(1)
            .ok_or(DiagnosticServiceError::Capacity)?;
        self.telemetry.last_acknowledged_sequence = event_sequence;
        self.telemetry.pending_len = 0;
        self.telemetry.status(self.transport_limits)
    }

    /// Admits or idempotently reconciles one canonical capture configuration.
    pub fn configure_waveform(
        &mut self,
        encoded: &[u8],
    ) -> Result<WaveformStatus, DiagnosticServiceError> {
        if !self.providers.digital_capture {
            return Err(DiagnosticServiceError::Unsupported);
        }
        if encoded.len() > CONFIGURE_BYTES || encoded.len() > MAX_NATIVE_DIAGNOSTIC_REQUEST_BYTES {
            return Err(DiagnosticServiceError::Capacity);
        }
        let incoming = decode_waveform_configure(encoded, self.transport_limits)?;
        if incoming.context() != self.context {
            return Err(DiagnosticServiceError::Conflict);
        }
        let maximum_record = digital_capture_encoded_len(
            incoming.channel_count(),
            usize::try_from(incoming.transition_capacity())
                .map_err(|_| DiagnosticServiceError::Capacity)?,
        )
        .map_err(|error| {
            DiagnosticServiceError::Invalid(DiagnosticTransportError::Record(error))
        })?;
        if maximum_record > RECORD_BYTES {
            return Err(DiagnosticServiceError::Capacity);
        }
        if self.waveform.occupied {
            let current = self.waveform.view(self.transport_limits)?;
            if current.capture_id() == incoming.capture_id() {
                if current.digest() == incoming.digest() {
                    return self.waveform.status(self.transport_limits);
                }
                return Err(DiagnosticServiceError::Conflict);
            }
            if matches!(
                self.waveform.phase,
                WaveformPhase::Configured | WaveformPhase::Armed | WaveformPhase::Complete
            ) {
                return Err(DiagnosticServiceError::Busy);
            }
        }
        self.waveform.configure.fill(0);
        self.waveform.configure[..encoded.len()].copy_from_slice(encoded);
        self.waveform.configure_len = encoded.len();
        self.waveform.occupied = true;
        self.waveform.phase = WaveformPhase::Configured;
        self.waveform.generation = 0;
        self.waveform.arm_cycle = DeviceCycle(0);
        self.waveform.trigger_cycle = DeviceCycle(0);
        self.waveform.triggered = false;
        self.waveform.record_len = 0;
        self.waveform.record_digest = Digest::ZERO;
        self.waveform.published_bytes = 0;
        self.waveform.dropped_chunks = 0;
        self.waveform.quality_flags = CaptureQualityFlags(0);
        self.waveform.status(self.transport_limits)
    }

    /// Fetches waveform status only for an exact configuration reference.
    pub fn waveform_status(
        &self,
        reference: WaveformSessionRequest,
    ) -> Result<WaveformStatus, DiagnosticServiceError> {
        self.waveform.matches(reference, self.transport_limits)?;
        self.waveform.status(self.transport_limits)
    }

    /// Arms one exact configuration, assigning one monotonic service generation.
    pub fn arm_waveform(
        &mut self,
        reference: WaveformSessionRequest,
        now: DeviceCycle,
    ) -> Result<WaveformStatus, DiagnosticServiceError> {
        self.waveform.matches(reference, self.transport_limits)?;
        match self.waveform.phase {
            WaveformPhase::Configured => {
                let configuration = self.waveform.view(self.transport_limits)?;
                if now > configuration.trigger_deadline_window().1 {
                    return Err(DiagnosticServiceError::Deadline);
                }
                let generation = self.next_waveform_generation;
                self.next_waveform_generation = generation
                    .checked_add(1)
                    .ok_or(DiagnosticServiceError::Capacity)?;
                self.waveform.phase = WaveformPhase::Armed;
                self.waveform.generation = generation;
                self.waveform.arm_cycle = now;
            }
            WaveformPhase::Armed | WaveformPhase::Complete => {}
            WaveformPhase::Stopped | WaveformPhase::Faulted => {
                return Err(DiagnosticServiceError::ForbiddenState);
            }
        }
        self.waveform.status(self.transport_limits)
    }

    /// Borrows the exact configuration only while the acquisition owner is armed.
    ///
    /// This is the narrow provider seam used by hardware capture tasks and the
    /// deterministic host simulator. It grants no network client access to the
    /// privately retained configuration storage.
    pub fn armed_waveform_configuration(
        &self,
    ) -> Result<WaveformConfigureView<'_>, DiagnosticServiceError> {
        if self.waveform.phase != WaveformPhase::Armed {
            return Err(DiagnosticServiceError::ForbiddenState);
        }
        self.waveform.view(self.transport_limits)
    }

    /// Retains one complete canonical capture produced by the acquisition owner.
    pub fn retain_waveform_capture(
        &mut self,
        encoded: &[u8],
    ) -> Result<WaveformStatus, DiagnosticServiceError> {
        if self.waveform.phase != WaveformPhase::Armed {
            return Err(DiagnosticServiceError::ForbiddenState);
        }
        if encoded.len() > RECORD_BYTES {
            return Err(DiagnosticServiceError::Capacity);
        }
        let configuration = self.waveform.view(self.transport_limits)?;
        let record_digest = validate_retained_capture(
            encoded,
            configuration,
            self.record_limits,
            self.transport_limits,
        )?;
        let capture = decode_digital_capture(encoded, self.record_limits).map_err(|error| {
            DiagnosticServiceError::Invalid(DiagnosticTransportError::Record(error))
        })?;
        let (trigger_cycle, _, _, _) = capture.trigger();
        self.waveform.record.fill(0);
        self.waveform.record[..encoded.len()].copy_from_slice(encoded);
        self.waveform.record_len = encoded.len();
        self.waveform.record_digest = record_digest;
        self.waveform.phase = WaveformPhase::Complete;
        self.waveform.trigger_cycle = trigger_cycle;
        self.waveform.triggered = trigger_cycle.0 != 0;
        self.waveform.published_bytes = 0;
        self.waveform.dropped_chunks = 0;
        self.waveform.quality_flags = capture.quality_flags();
        self.waveform.status(self.transport_limits)
    }

    /// Stops and releases an exact capture while retaining lifecycle identity.
    pub fn stop_waveform(
        &mut self,
        reference: WaveformSessionRequest,
    ) -> Result<WaveformStatus, DiagnosticServiceError> {
        self.waveform.matches(reference, self.transport_limits)?;
        if self.waveform.phase != WaveformPhase::Stopped {
            self.waveform.phase = WaveformPhase::Stopped;
            self.waveform.record_len = 0;
            self.waveform.record_digest = Digest::ZERO;
            self.waveform.published_bytes = 0;
            self.waveform.record.fill(0);
        }
        self.waveform.status(self.transport_limits)
    }

    /// Encodes an authoritative range from the retained record for recovery.
    /// This read is side-effect free and does not claim live publication.
    pub fn read_waveform(
        &self,
        request: WaveformReadRequest,
        output: &mut [u8],
    ) -> Result<usize, DiagnosticServiceError> {
        let reference = WaveformSessionRequest {
            capture_id: request.capture_id,
            configure_digest: request.configure_digest,
        };
        self.waveform.matches(reference, self.transport_limits)?;
        if self.waveform.phase != WaveformPhase::Complete {
            return Err(DiagnosticServiceError::ForbiddenState);
        }
        if request.record_digest != self.waveform.record_digest {
            return Err(DiagnosticServiceError::Conflict);
        }
        let offset =
            usize::try_from(request.offset).map_err(|_| DiagnosticServiceError::Capacity)?;
        if offset >= self.waveform.record_len {
            return Err(DiagnosticServiceError::Invalid(
                DiagnosticTransportError::Range,
            ));
        }
        let requested =
            usize::try_from(request.maximum_bytes).map_err(|_| DiagnosticServiceError::Capacity)?;
        let end = offset
            .saturating_add(requested)
            .min(self.waveform.record_len);
        let chunk = WaveformChunk {
            capture_id: reference.capture_id,
            configure_digest: reference.configure_digest,
            record_digest: self.waveform.record_digest,
            record_bytes: u32::try_from(self.waveform.record_len)
                .map_err(|_| DiagnosticServiceError::Capacity)?,
            offset: request.offset,
            bytes: &self.waveform.record[offset..end],
        };
        encode_waveform_chunk(&chunk, output, self.transport_limits)
            .map_err(DiagnosticServiceError::Invalid)
    }

    /// Encodes the next reproducible live chunk without acknowledging delivery.
    pub fn pending_waveform_chunk(
        &self,
        output: &mut [u8],
    ) -> Result<Option<usize>, DiagnosticServiceError> {
        if self.waveform.phase != WaveformPhase::Complete {
            return Ok(None);
        }
        let offset = usize::try_from(self.waveform.published_bytes)
            .map_err(|_| DiagnosticServiceError::Capacity)?;
        if offset == self.waveform.record_len {
            return Ok(None);
        }
        let configuration = self.waveform.view(self.transport_limits)?;
        let chunk_bytes = usize::try_from(configuration.maximum_chunk_bytes())
            .map_err(|_| DiagnosticServiceError::Capacity)?;
        let end = offset
            .saturating_add(chunk_bytes)
            .min(self.waveform.record_len);
        let chunk = WaveformChunk {
            capture_id: configuration.capture_id(),
            configure_digest: configuration.digest(),
            record_digest: self.waveform.record_digest,
            record_bytes: u32::try_from(self.waveform.record_len)
                .map_err(|_| DiagnosticServiceError::Capacity)?,
            offset: self.waveform.published_bytes,
            bytes: &self.waveform.record[offset..end],
        };
        encode_waveform_chunk(&chunk, output, self.transport_limits)
            .map(Some)
            .map_err(DiagnosticServiceError::Invalid)
    }

    /// Advances the live cursor after successful delivery of exactly one chunk.
    pub fn acknowledge_waveform_chunk(
        &mut self,
        reference: WaveformSessionRequest,
        end_offset: u32,
    ) -> Result<WaveformStatus, DiagnosticServiceError> {
        self.advance_waveform_chunk(reference, end_offset, false)
    }

    /// Advances the live cursor after a failed delivery and records exact loss.
    /// The retained record remains available through authoritative range reads.
    pub fn drop_waveform_chunk(
        &mut self,
        reference: WaveformSessionRequest,
        end_offset: u32,
    ) -> Result<WaveformStatus, DiagnosticServiceError> {
        self.advance_waveform_chunk(reference, end_offset, true)
    }

    fn advance_waveform_chunk(
        &mut self,
        reference: WaveformSessionRequest,
        end_offset: u32,
        dropped: bool,
    ) -> Result<WaveformStatus, DiagnosticServiceError> {
        self.waveform.matches(reference, self.transport_limits)?;
        if self.waveform.phase != WaveformPhase::Complete {
            return Err(DiagnosticServiceError::ForbiddenState);
        }
        let configuration = self.waveform.view(self.transport_limits)?;
        let record_bytes = u32::try_from(self.waveform.record_len)
            .map_err(|_| DiagnosticServiceError::Capacity)?;
        let expected_end = self
            .waveform
            .published_bytes
            .saturating_add(configuration.maximum_chunk_bytes())
            .min(record_bytes);
        if end_offset != expected_end || end_offset <= self.waveform.published_bytes {
            return Err(DiagnosticServiceError::Conflict);
        }
        if dropped {
            self.waveform.dropped_chunks = self
                .waveform
                .dropped_chunks
                .checked_add(1)
                .ok_or(DiagnosticServiceError::Capacity)?;
        }
        self.waveform.published_bytes = end_offset;
        self.waveform.status(self.transport_limits)
    }
}

fn encode_response<const N: usize>(
    encoded: Result<[u8; N], DiagnosticTransportError>,
    output: &mut [u8],
) -> Result<usize, DiagnosticServiceError> {
    let encoded = encoded.map_err(DiagnosticServiceError::Invalid)?;
    if encoded.len() > output.len() {
        return Err(DiagnosticServiceError::Capacity);
    }
    output[..encoded.len()].copy_from_slice(&encoded);
    Ok(encoded.len())
}

fn native_response(
    request: NativeRequest<'_>,
    now: DeviceCycle,
    status: StatusCode,
    body: &[u8],
) -> ServiceResponse {
    ServiceResponse::native(request, now, status, body)
        .expect("diagnostic response respects the fixed operation-body bound")
}
