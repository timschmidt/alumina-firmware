//! Single-owner, fixed-memory diagnostic transport state.

use alumina_board::{DigitalCaptureDescriptor, DigitalCaptureSourceKind, DigitalCaptureTriggerSet};
use alumina_diagnostics::transport::{
    DiagnosticTransportError, DiagnosticTransportLimits, TelemetryEvent, TelemetryPhase,
    TelemetryPollRequest, TelemetrySessionRequest, TelemetrySubscribeView,
    TelemetrySubscriptionStatus, WaveformChunk, WaveformConfigureView, WaveformPhase,
    WaveformReadRequest, WaveformSessionRequest, WaveformStatus, decode_telemetry_subscribe,
    decode_waveform_configure, encode_telemetry_event, encode_waveform_chunk,
    validate_retained_capture,
};
use alumina_diagnostics::{
    CaptureQualityFlags, DIGITAL_CAPTURE_VERSION, DiagnosticContext, DiagnosticError,
    DiagnosticLimits, DigitalAcquisitionSource, DigitalTriggerCondition, OverviewFlags,
    RealtimeInputRecord, ResourceOverviewDocument, ResourceOverviewSample, ResourceValue,
    SampleProvenance, SampleQuality, SampleQualityFlags, decode_digital_capture,
    decode_realtime_input_snapshot, decode_resource_overview, digital_capture_encoded_len,
    encode_resource_overview,
};
use alumina_protocol::{DeviceCycle, Digest, FrameKind, Operation, StatusCode};
use alumina_safety::SafetyInputStatus;

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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticProviderPolicy {
    /// A resource owner can assemble canonical overview samples.
    pub resource_overview: bool,
    /// A capture owner can execute and retain configured digital acquisitions.
    pub digital_capture: DigitalCaptureDescriptor<'static>,
}

impl DiagnosticProviderPolicy {
    /// No provider is connected; protocol operations fail honestly as unsupported.
    pub const NONE: Self = Self {
        resource_overview: false,
        digital_capture: DigitalCaptureDescriptor::NONE,
    };
}

fn admit_waveform_configuration(
    configuration: WaveformConfigureView<'_>,
    descriptor: DigitalCaptureDescriptor<'_>,
    context: DiagnosticContext,
    configure_len: usize,
) -> Result<usize, DiagnosticServiceError> {
    if !descriptor.is_implemented() || descriptor.schema_version != DIGITAL_CAPTURE_VERSION {
        return Err(DiagnosticServiceError::Unsupported);
    }
    let configure_bytes = usize::try_from(descriptor.configure_bytes)
        .map_err(|_| DiagnosticServiceError::Capacity)?;
    if configure_len > configure_bytes
        || configuration.channel_count() > usize::from(descriptor.maximum_channels)
        || configuration.transition_capacity() > descriptor.maximum_transitions
        || configuration.maximum_chunk_bytes() > descriptor.maximum_chunk_bytes
    {
        return Err(DiagnosticServiceError::Capacity);
    }
    if configuration.flags().0 & !descriptor.configure_flags.0 != 0 {
        return Err(DiagnosticServiceError::Invalid(
            DiagnosticTransportError::Flags,
        ));
    }
    let trigger_bit = match configuration.trigger().1 {
        DigitalTriggerCondition::Immediate => DigitalCaptureTriggerSet::IMMEDIATE,
        DigitalTriggerCondition::Rising => DigitalCaptureTriggerSet::RISING,
        DigitalTriggerCondition::Falling => DigitalCaptureTriggerSet::FALLING,
        DigitalTriggerCondition::Either => DigitalCaptureTriggerSet::EITHER,
    };
    if !descriptor.trigger_kinds.contains(trigger_bit) {
        return Err(DiagnosticServiceError::Invalid(
            DiagnosticTransportError::Window,
        ));
    }
    if configuration.channels().any(|resource| {
        !descriptor
            .resources
            .iter()
            .any(|candidate| candidate.resource == resource)
    }) {
        return Err(DiagnosticServiceError::Invalid(
            DiagnosticTransportError::Resource,
        ));
    }

    let (pretrigger, posttrigger) = configuration.requested_window_cycles();
    let duration = pretrigger
        .checked_add(posttrigger)
        .ok_or(DiagnosticServiceError::Invalid(
            DiagnosticTransportError::Window,
        ))?;
    let (earliest, latest) = configuration.trigger_deadline_window();
    let arm_horizon = latest
        .0
        .checked_sub(earliest.0)
        .ok_or(DiagnosticServiceError::Invalid(
            DiagnosticTransportError::Window,
        ))?;
    if pretrigger
        > micros_to_cycles_floor(
            descriptor.maximum_pretrigger_micros,
            context.clock_frequency_hz,
        )
        || duration
            > micros_to_cycles_floor(
                descriptor.maximum_duration_micros,
                context.clock_frequency_hz,
            )
        || arm_horizon
            > micros_to_cycles_floor(descriptor.arm_horizon_micros, context.clock_frequency_hz)
    {
        return Err(DiagnosticServiceError::Invalid(
            DiagnosticTransportError::Limit("digital capture timing"),
        ));
    }

    digital_capture_encoded_len(
        configuration.channel_count(),
        usize::try_from(configuration.transition_capacity())
            .map_err(|_| DiagnosticServiceError::Capacity)?,
    )
    .map_err(|error| DiagnosticServiceError::Invalid(DiagnosticTransportError::Record(error)))
}

fn admit_retained_capture_sources(
    capture: alumina_diagnostics::DigitalCaptureView<'_>,
    descriptor: DigitalCaptureDescriptor<'_>,
) -> Result<(), DiagnosticServiceError> {
    for channel in capture.channels() {
        let Some(expected) = descriptor
            .resources
            .iter()
            .find(|candidate| candidate.resource == channel.resource)
        else {
            return Err(DiagnosticServiceError::Invalid(
                DiagnosticTransportError::Resource,
            ));
        };
        if digital_capture_source(channel.source) != Some(expected.source) {
            return Err(DiagnosticServiceError::Invalid(
                DiagnosticTransportError::Resource,
            ));
        }
    }
    Ok(())
}

const fn digital_capture_source(
    source: DigitalAcquisitionSource,
) -> Option<DigitalCaptureSourceKind> {
    match source {
        DigitalAcquisitionSource::Simulated => Some(DigitalCaptureSourceKind::Simulated),
        DigitalAcquisitionSource::Rmt => Some(DigitalCaptureSourceKind::Rmt),
        DigitalAcquisitionSource::Pcnt => Some(DigitalCaptureSourceKind::Pcnt),
        DigitalAcquisitionSource::Dma => Some(DigitalCaptureSourceKind::Dma),
        DigitalAcquisitionSource::Software => Some(DigitalCaptureSourceKind::Software),
        DigitalAcquisitionSource::ExternalAnalyzer => None,
    }
}

fn micros_to_cycles_floor(micros: u32, frequency_hz: u64) -> u64 {
    let cycles = u128::from(micros) * u128::from(frequency_hz) / 1_000_000;
    u64::try_from(cycles).unwrap_or(u64::MAX)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AcceptedRealtimeInputSnapshot<const INPUTS: usize> {
    frame_sequence: u32,
    produced_at: DeviceCycle,
    config_digest: Digest,
    status: SafetyInputStatus,
    records: [Option<RealtimeInputRecord>; INPUTS],
    record_count: usize,
}

/// Rejection while validating a lossy core-1 diagnostic input publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealtimeInputObservationError {
    /// Canonical snapshot bytes were malformed.
    Snapshot(DiagnosticError),
    /// Snapshot contains more slots than this board composition reserves.
    Capacity,
    /// Frame sequence was zero, duplicated, old, or ambiguously far ahead.
    FrameSequence,
    /// Producer or per-slot acquisition time was later than observer time.
    Future,
    /// Snapshot was already outside the declared freshness window when dequeued.
    Expired,
    /// A configuration digest retained a different slot-to-resource mapping.
    ResourceMapping,
    /// A per-slot acquisition timestamp moved backwards.
    SampleTime,
    /// Debounced semantic state changed without a newer monitor generation.
    TransitionGeneration,
}

/// Rejection while translating an admitted input snapshot into an overview.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealtimeInputOverviewError {
    /// Subscription and caller-supplied evidence context differed.
    Context,
    /// Caller scratch space could not hold every selected resource.
    Capacity,
    /// Canonical overview encoding rejected the assembled evidence.
    Record(DiagnosticError),
}

/// Freshness- and identity-checking core-0 observer for passive input diagnostics.
///
/// Publications are lossy and latest-only. Rejection revokes only diagnostic
/// evidence; this observer neither grants safety authority nor drives outputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeInputObserver<const INPUTS: usize> {
    maximum_age_cycles: u64,
    accepted: Option<AcceptedRealtimeInputSnapshot<INPUTS>>,
}

impl<const INPUTS: usize> RealtimeInputObserver<INPUTS> {
    /// Creates an observer with no retained evidence.
    pub const fn new(maximum_age_cycles: u64) -> Self {
        Self {
            maximum_age_cycles,
            accepted: None,
        }
    }

    /// Decodes and admits one complete core-1 publication.
    ///
    /// The outer inter-core frame supplies the exact configuration identity and
    /// production cycle; the payload supplies slot mapping and acquisition time.
    pub fn observe_encoded(
        &mut self,
        frame_sequence: u32,
        produced_at: DeviceCycle,
        observed_at: DeviceCycle,
        config_digest: Digest,
        encoded: &[u8],
    ) -> Result<(), RealtimeInputObservationError> {
        let view = match decode_realtime_input_snapshot(encoded) {
            Ok(view) => view,
            Err(error) => {
                self.invalidate();
                return Err(RealtimeInputObservationError::Snapshot(error));
            }
        };
        if view.record_count() > INPUTS {
            self.invalidate();
            return Err(RealtimeInputObservationError::Capacity);
        }
        if frame_sequence == 0
            || self
                .accepted
                .is_some_and(|previous| !serial_is_newer(frame_sequence, previous.frame_sequence))
        {
            self.invalidate();
            return Err(RealtimeInputObservationError::FrameSequence);
        }
        let Some(age) = observed_at.0.checked_sub(produced_at.0) else {
            self.invalidate();
            return Err(RealtimeInputObservationError::Future);
        };
        if age > self.maximum_age_cycles {
            self.invalidate();
            return Err(RealtimeInputObservationError::Expired);
        }

        let mut records = [None; INPUTS];
        for (slot, record) in view.records().enumerate() {
            if record
                .sampled_at
                .is_some_and(|sampled_at| sampled_at > produced_at)
            {
                self.invalidate();
                return Err(RealtimeInputObservationError::Future);
            }
            records[slot] = Some(record);
        }
        let candidate = AcceptedRealtimeInputSnapshot {
            frame_sequence,
            produced_at,
            config_digest,
            status: view.status(),
            records,
            record_count: view.record_count(),
        };
        if let Some(previous) = self.accepted
            && previous.config_digest == candidate.config_digest
        {
            let mapping_changed = previous.record_count != candidate.record_count
                || previous.status.required_mask != candidate.status.required_mask
                || (0..previous.record_count).any(|slot| {
                    previous.records[slot].map(|record| record.resource)
                        != candidate.records[slot].map(|record| record.resource)
                });
            if mapping_changed {
                self.invalidate();
                return Err(RealtimeInputObservationError::ResourceMapping);
            }
            if candidate.produced_at < previous.produced_at {
                self.invalidate();
                return Err(RealtimeInputObservationError::SampleTime);
            }
            for slot in 0..candidate.record_count {
                let previous_sample = previous.records[slot].and_then(|record| record.sampled_at);
                let candidate_sample = candidate.records[slot].and_then(|record| record.sampled_at);
                let regressed = match (previous_sample, candidate_sample) {
                    (Some(previous), Some(candidate)) => candidate < previous,
                    (Some(_), None) => true,
                    (None, _) => false,
                };
                if regressed {
                    self.invalidate();
                    return Err(RealtimeInputObservationError::SampleTime);
                }
            }
            let generation_changed =
                candidate.status.transition_generation != previous.status.transition_generation;
            if generation_changed
                && !serial_is_newer(
                    candidate.status.transition_generation,
                    previous.status.transition_generation,
                )
            {
                self.invalidate();
                return Err(RealtimeInputObservationError::TransitionGeneration);
            }
            let semantic_changed = candidate.status.known_mask != previous.status.known_mask
                || candidate.status.active_mask != previous.status.active_mask;
            if semantic_changed && !generation_changed {
                self.invalidate();
                return Err(RealtimeInputObservationError::TransitionGeneration);
            }
        }
        self.accepted = Some(candidate);
        Ok(())
    }

    /// Revokes retained diagnostic evidence without affecting safety state.
    pub fn invalidate(&mut self) {
        self.accepted = None;
    }

    /// Assembles one canonical overview for an admitted subscription.
    ///
    /// Missing, expired, or differently configured evidence becomes explicit
    /// unavailable values. Known stale values retain their exact last sample time.
    pub fn encode_overview(
        &self,
        context: DiagnosticContext,
        subscription: TelemetrySubscribeView<'_>,
        sequence: u64,
        snapshot_cycle: DeviceCycle,
        samples: &mut [ResourceOverviewSample],
        output: &mut [u8],
    ) -> Result<usize, RealtimeInputOverviewError> {
        if subscription.context() != context {
            return Err(RealtimeInputOverviewError::Context);
        }
        if subscription.resource_count() > samples.len() {
            return Err(RealtimeInputOverviewError::Capacity);
        }
        let accepted = self.accepted.filter(|accepted| {
            accepted.config_digest == context.config_digest
                && snapshot_cycle
                    .0
                    .checked_sub(accepted.produced_at.0)
                    .is_some_and(|age| age <= self.maximum_age_cycles)
        });
        for (index, resource) in subscription.resources().enumerate() {
            samples[index] = accepted
                .and_then(|accepted| accepted.sample(resource))
                .unwrap_or(ResourceOverviewSample {
                    resource,
                    provenance: SampleProvenance::Inferred,
                    quality: SampleQuality::Unavailable,
                    quality_flags: SampleQualityFlags(0),
                    captured_cycle: DeviceCycle(0),
                    value: ResourceValue::Unavailable,
                });
        }
        encode_resource_overview(
            &ResourceOverviewDocument {
                context,
                flags: OverviewFlags(0),
                snapshot_cycle,
                sequence,
                samples: &samples[..subscription.resource_count()],
            },
            output,
        )
        .map_err(RealtimeInputOverviewError::Record)
    }
}

impl<const INPUTS: usize> AcceptedRealtimeInputSnapshot<INPUTS> {
    fn sample(self, resource: alumina_board::ResourceId) -> Option<ResourceOverviewSample> {
        let slot = self.records[..self.record_count]
            .iter()
            .position(|record| record.is_some_and(|record| record.resource == resource))?;
        let record = self.records[slot].expect("located record is present");
        let bit = 1_u32 << slot;
        let known = self.status.known_mask & bit != 0;
        let stale = self.status.stale_mask & bit != 0;
        Some(ResourceOverviewSample {
            resource,
            provenance: SampleProvenance::Measured,
            quality: if known {
                if stale {
                    SampleQuality::Stale
                } else {
                    SampleQuality::Valid
                }
            } else {
                SampleQuality::Unavailable
            },
            quality_flags: SampleQualityFlags(if known {
                SampleQualityFlags::DEBOUNCED
            } else {
                0
            }),
            captured_cycle: record.sampled_at.unwrap_or(DeviceCycle(0)),
            value: if known {
                ResourceValue::Boolean(self.status.active_mask & bit != 0)
            } else {
                ResourceValue::Unavailable
            },
        })
    }
}

const fn serial_is_newer(candidate: u32, previous: u32) -> bool {
    let distance = candidate.wrapping_sub(previous);
    distance != 0 && distance < (1_u32 << 31)
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

    /// Borrows a provider request only when its minimum publication period is due.
    ///
    /// This prevents a fast service loop from repeatedly assembling evidence
    /// that the canonical event publisher would reject as premature.
    pub fn telemetry_provider_request_at(
        &self,
        now: DeviceCycle,
    ) -> Option<(TelemetrySubscribeView<'_>, u64)> {
        let (subscription, sequence) = self.telemetry_provider_request()?;
        if sequence > 1
            && now
                .0
                .checked_sub(self.telemetry.last_event_cycle.0)
                .is_none_or(|elapsed| elapsed < subscription.minimum_period_cycles())
        {
            return None;
        }
        Some((subscription, sequence))
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
        if !self.providers.digital_capture.is_implemented() {
            return Err(DiagnosticServiceError::Unsupported);
        }
        if encoded.len() > CONFIGURE_BYTES || encoded.len() > MAX_NATIVE_DIAGNOSTIC_REQUEST_BYTES {
            return Err(DiagnosticServiceError::Capacity);
        }
        let incoming = decode_waveform_configure(encoded, self.transport_limits)?;
        if incoming.context() != self.context {
            return Err(DiagnosticServiceError::Conflict);
        }
        let maximum_record = admit_waveform_configuration(
            incoming,
            self.providers.digital_capture,
            self.context,
            encoded.len(),
        )?;
        if maximum_record > RECORD_BYTES
            || maximum_record
                > usize::try_from(self.providers.digital_capture.record_bytes)
                    .map_err(|_| DiagnosticServiceError::Capacity)?
        {
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
        if encoded.len() > RECORD_BYTES
            || encoded.len()
                > usize::try_from(self.providers.digital_capture.record_bytes)
                    .map_err(|_| DiagnosticServiceError::Capacity)?
        {
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
        admit_retained_capture_sources(capture, self.providers.digital_capture)?;
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
        let configuration = self.waveform.view(self.transport_limits)?;
        let offset =
            usize::try_from(request.offset).map_err(|_| DiagnosticServiceError::Capacity)?;
        if offset >= self.waveform.record_len {
            return Err(DiagnosticServiceError::Invalid(
                DiagnosticTransportError::Range,
            ));
        }
        let requested =
            usize::try_from(request.maximum_bytes).map_err(|_| DiagnosticServiceError::Capacity)?;
        let configured_chunk = usize::try_from(configuration.maximum_chunk_bytes())
            .map_err(|_| DiagnosticServiceError::Capacity)?;
        let advertised_chunk = usize::try_from(self.providers.digital_capture.maximum_chunk_bytes)
            .map_err(|_| DiagnosticServiceError::Capacity)?;
        if requested > configured_chunk || requested > advertised_chunk {
            return Err(DiagnosticServiceError::Capacity);
        }
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

#[cfg(test)]
mod tests {
    use alumina_capability::CapabilityIdentity;
    use alumina_clock::{BOOT_ID_BYTES, BootId};
    use alumina_diagnostics::transport::{
        DiagnosticTransportLimits, SubscriptionId, TelemetryPollRequest, TelemetrySessionRequest,
        TelemetrySubscribeFlags, TelemetrySubscribeRequest, decode_telemetry_event,
        decode_telemetry_subscribe, encode_telemetry_subscribe,
    };
    use alumina_diagnostics::{
        REALTIME_INPUT_SNAPSHOT_HEADER_BYTES, REALTIME_INPUT_SNAPSHOT_RECORD_BYTES,
        RealtimeInputSnapshotDocument, decode_resource_overview, encode_realtime_input_snapshot,
    };
    use alumina_protocol::DeviceId;

    use super::*;

    const RESOURCES: [alumina_board::ResourceId; 4] = [
        alumina_board::ResourceId::Gpio(22),
        alumina_board::ResourceId::Gpio(32),
        alumina_board::ResourceId::Gpio(33),
        alumina_board::ResourceId::Gpio(35),
    ];

    fn context() -> DiagnosticContext {
        DiagnosticContext {
            device_id: DeviceId(*b"ALUM-SIM:TINYBEE"),
            boot_id: BootId::new([0x41; BOOT_ID_BYTES]).unwrap(),
            capability: CapabilityIdentity {
                byte_len: 3_435,
                digest: Digest([0x42; 32]),
            },
            config_digest: Digest([0x43; 32]),
            clock_frequency_hz: 1_000_000,
        }
    }

    fn subscription_bytes(context: DiagnosticContext) -> [u8; 176] {
        let request = TelemetrySubscribeRequest {
            subscription_id: SubscriptionId::new(9).unwrap(),
            context,
            flags: TelemetrySubscribeFlags(TelemetrySubscribeFlags::LATEST_ONLY),
            minimum_period_cycles: 100_000,
            maximum_event_bytes: 432,
            resources: &RESOURCES,
        };
        let mut encoded = [0_u8; 176];
        let used = encode_telemetry_subscribe(
            &request,
            &mut encoded,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        assert_eq!(used, encoded.len());
        encoded
    }

    fn input_records() -> [RealtimeInputRecord; 4] {
        [
            RealtimeInputRecord {
                resource: alumina_board::ResourceId::Gpio(33),
                sampled_at: Some(DeviceCycle(990)),
            },
            RealtimeInputRecord {
                resource: alumina_board::ResourceId::Gpio(32),
                sampled_at: Some(DeviceCycle(991)),
            },
            RealtimeInputRecord {
                resource: alumina_board::ResourceId::Gpio(22),
                sampled_at: Some(DeviceCycle(992)),
            },
            RealtimeInputRecord {
                resource: alumina_board::ResourceId::Gpio(35),
                sampled_at: Some(DeviceCycle(993)),
            },
        ]
    }

    fn input_status() -> SafetyInputStatus {
        SafetyInputStatus {
            input_count: 4,
            known_mask: 0b1111,
            active_mask: 0b1001,
            required_mask: 0b0111,
            stale_mask: 0b0010,
            transition_generation: 4,
            next_watchdog_deadline: None,
        }
    }

    fn encode_input_snapshot(
        status: SafetyInputStatus,
        records: &[RealtimeInputRecord],
    ) -> [u8; REALTIME_INPUT_SNAPSHOT_HEADER_BYTES + 4 * REALTIME_INPUT_SNAPSHOT_RECORD_BYTES] {
        let mut encoded =
            [0_u8; REALTIME_INPUT_SNAPSHOT_HEADER_BYTES + 4 * REALTIME_INPUT_SNAPSHOT_RECORD_BYTES];
        encode_realtime_input_snapshot(
            &RealtimeInputSnapshotDocument { status, records },
            &mut encoded,
        )
        .unwrap();
        encoded
    }

    #[test]
    fn input_observer_preserves_resource_mapping_quality_and_sample_time() {
        let context = context();
        let subscription_bytes = subscription_bytes(context);
        let subscription = decode_telemetry_subscribe(
            &subscription_bytes,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        let encoded = encode_input_snapshot(input_status(), &input_records());
        let mut observer = RealtimeInputObserver::<4>::new(500);
        observer
            .observe_encoded(
                1,
                DeviceCycle(1_000),
                DeviceCycle(1_010),
                context.config_digest,
                &encoded,
            )
            .unwrap();

        let mut samples = [ResourceOverviewSample {
            resource: alumina_board::ResourceId::Gpio(0),
            provenance: SampleProvenance::Inferred,
            quality: SampleQuality::Unavailable,
            quality_flags: SampleQualityFlags(0),
            captured_cycle: DeviceCycle(0),
            value: ResourceValue::Unavailable,
        }; 4];
        let mut overview = [0_u8; 320];
        let used = observer
            .encode_overview(
                context,
                subscription,
                1,
                DeviceCycle(1_100),
                &mut samples,
                &mut overview,
            )
            .unwrap();
        assert_eq!(used, overview.len());
        let overview =
            decode_resource_overview(&overview, DiagnosticLimits::interactive()).unwrap();
        let samples = overview
            .samples()
            .collect::<heapless::Vec<ResourceOverviewSample, 4>>();
        assert_eq!(samples[0].resource, alumina_board::ResourceId::Gpio(22));
        assert_eq!(samples[0].captured_cycle, DeviceCycle(992));
        assert_eq!(samples[0].value, ResourceValue::Boolean(false));
        assert_eq!(samples[0].quality, SampleQuality::Valid);
        assert_eq!(samples[1].captured_cycle, DeviceCycle(991));
        assert_eq!(samples[1].quality, SampleQuality::Stale);
        assert_eq!(samples[2].captured_cycle, DeviceCycle(990));
        assert_eq!(samples[2].value, ResourceValue::Boolean(true));
        assert_eq!(samples[3].captured_cycle, DeviceCycle(993));
        assert_eq!(samples[3].value, ResourceValue::Boolean(true));
        assert!(
            samples
                .iter()
                .all(|sample| { sample.quality_flags.contains(SampleQualityFlags::DEBOUNCED) })
        );
    }

    #[test]
    fn target_style_observer_publishes_one_exact_authenticated_event() {
        type TargetService = DiagnosticServiceState<176, 432, 0, 0>;

        let context = context();
        let subscription_bytes = subscription_bytes(context);
        let subscription = decode_telemetry_subscribe(
            &subscription_bytes,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        let reference = TelemetrySessionRequest {
            subscription_id: subscription.subscription_id(),
            subscription_digest: subscription.digest(),
        };
        let mut service = TargetService::new(
            context,
            DiagnosticProviderPolicy {
                resource_overview: true,
                digital_capture: DigitalCaptureDescriptor::NONE,
            },
            DiagnosticTransportLimits::native_control(),
            DiagnosticLimits::interactive(),
        );
        service.subscribe(&subscription_bytes).unwrap();

        let mut observer = RealtimeInputObserver::<4>::new(500);
        let input = encode_input_snapshot(input_status(), &input_records());
        observer
            .observe_encoded(
                1,
                DeviceCycle(1_000),
                DeviceCycle(1_010),
                context.config_digest,
                &input,
            )
            .unwrap();
        let (provider_request, sequence) = service
            .telemetry_provider_request_at(DeviceCycle(1_100))
            .unwrap();
        let mut samples = [ResourceOverviewSample {
            resource: alumina_board::ResourceId::Gpio(0),
            provenance: SampleProvenance::Inferred,
            quality: SampleQuality::Unavailable,
            quality_flags: SampleQualityFlags(0),
            captured_cycle: DeviceCycle(0),
            value: ResourceValue::Unavailable,
        }; 4];
        let mut overview = [0_u8; 320];
        let overview_len = observer
            .encode_overview(
                context,
                provider_request,
                sequence,
                DeviceCycle(1_100),
                &mut samples,
                &mut overview,
            )
            .unwrap();
        service.publish_overview(&overview[..overview_len]).unwrap();

        let mut event = [0_u8; MAX_NATIVE_DIAGNOSTIC_RESPONSE_BODY_BYTES];
        let event_len = service
            .poll_telemetry_event(
                TelemetryPollRequest {
                    reference,
                    accepted_event_sequence: 0,
                },
                &mut event,
            )
            .unwrap();
        assert_eq!(event_len, 432);
        let event = decode_telemetry_event(
            &event[..event_len],
            subscription,
            DiagnosticLimits::interactive(),
        )
        .unwrap();
        assert_eq!(event.event_sequence(), 1);
        assert_eq!(event.dropped_events(), 0);
        assert_eq!(event.overview().snapshot_cycle(), DeviceCycle(1_100));
        assert_eq!(
            event
                .overview()
                .sample(alumina_board::ResourceId::Gpio(33))
                .unwrap()
                .value,
            ResourceValue::Boolean(true)
        );
    }

    #[test]
    fn input_observer_revokes_mapping_generation_and_time_substitution() {
        let context = context();
        let subscription_bytes = subscription_bytes(context);
        let subscription = decode_telemetry_subscribe(
            &subscription_bytes,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        let records = input_records();
        let encoded = encode_input_snapshot(input_status(), &records);
        let mut observer = RealtimeInputObserver::<4>::new(500);
        observer
            .observe_encoded(
                1,
                DeviceCycle(1_000),
                DeviceCycle(1_010),
                context.config_digest,
                &encoded,
            )
            .unwrap();

        let mut changed_status = input_status();
        changed_status.active_mask ^= 1;
        let changed = encode_input_snapshot(changed_status, &records);
        assert_eq!(
            observer.observe_encoded(
                2,
                DeviceCycle(1_100),
                DeviceCycle(1_110),
                context.config_digest,
                &changed,
            ),
            Err(RealtimeInputObservationError::TransitionGeneration)
        );

        let mut changed_records = records;
        changed_records[0].resource = alumina_board::ResourceId::Gpio(21);
        let changed = encode_input_snapshot(input_status(), &changed_records);
        observer
            .observe_encoded(
                3,
                DeviceCycle(1_200),
                DeviceCycle(1_210),
                context.config_digest,
                &encoded,
            )
            .unwrap();
        assert_eq!(
            observer.observe_encoded(
                4,
                DeviceCycle(1_300),
                DeviceCycle(1_310),
                context.config_digest,
                &changed,
            ),
            Err(RealtimeInputObservationError::ResourceMapping)
        );

        let mut future_records = records;
        future_records[0].sampled_at = Some(DeviceCycle(2_001));
        let future = encode_input_snapshot(input_status(), &future_records);
        assert_eq!(
            observer.observe_encoded(
                5,
                DeviceCycle(2_000),
                DeviceCycle(2_010),
                context.config_digest,
                &future,
            ),
            Err(RealtimeInputObservationError::Future)
        );

        let mut samples = [ResourceOverviewSample {
            resource: alumina_board::ResourceId::Gpio(0),
            provenance: SampleProvenance::Inferred,
            quality: SampleQuality::Unavailable,
            quality_flags: SampleQualityFlags(0),
            captured_cycle: DeviceCycle(0),
            value: ResourceValue::Unavailable,
        }; 4];
        let mut overview = [0_u8; 320];
        observer
            .encode_overview(
                context,
                subscription,
                1,
                DeviceCycle(2_020),
                &mut samples,
                &mut overview,
            )
            .unwrap();
        let overview =
            decode_resource_overview(&overview, DiagnosticLimits::interactive()).unwrap();
        assert!(overview.samples().all(|sample| {
            sample.quality == SampleQuality::Unavailable
                && sample.value == ResourceValue::Unavailable
        }));
    }
}
