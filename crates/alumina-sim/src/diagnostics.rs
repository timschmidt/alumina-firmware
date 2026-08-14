//! Deterministic evidence-only diagnostic fixtures.
//!
//! These records are deliberately marked `Simulated` at both the document and
//! per-value/acquisition layers. They open no device, GPIO, network interface,
//! command path, diagnostic lease, or output authority.

use alumina_board::ResourceId;
use alumina_capability::{CapabilityIdentity, calculate_identity};
use alumina_clock::{BOOT_ID_BYTES, BootId};
use alumina_diagnostics::transport::WaveformConfigureView;
use alumina_diagnostics::{
    CaptureId, CaptureQualityFlags, DiagnosticContext, DiagnosticError, DiagnosticLimits,
    DigitalAcquisitionSource, DigitalCaptureChannel, DigitalCaptureDocument, DigitalCaptureFlags,
    DigitalCaptureState, DigitalCaptureView, DigitalChannelFlags, DigitalLevel, DigitalTransition,
    DigitalTransitionFlags, DigitalTriggerCondition, OverviewFlags, ResourceOverviewDocument,
    ResourceOverviewSample, ResourceOverviewView, ResourceValue, SampleProvenance, SampleQuality,
    SampleQualityFlags, decode_digital_capture, decode_resource_overview,
    digital_capture_encoded_len, encode_digital_capture, encode_resource_overview,
    resource_overview_encoded_len,
};
use alumina_protocol::{DeviceCycle, DeviceId, Digest};

/// Stable simulator-only device namespace used by the TinyBee fixture.
pub const TINYBEE_SIMULATOR_DEVICE_ID: DeviceId = DeviceId(*b"ALUM-SIM:TINYBEE");
/// Exact simulated monotonic clock frequency.
pub const TINYBEE_SIMULATOR_CLOCK_HZ: u64 = 1_000_000;
/// Number of graph-readable TinyBee inputs represented in the overview/capture.
pub const TINYBEE_SIMULATOR_CHANNELS: usize = 4;
/// Number of retained edge events in the deterministic capture.
pub const TINYBEE_SIMULATOR_TRANSITIONS: usize = 14;

/// Construction failure for the deterministic diagnostic fixture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticFixtureError {
    /// Current TinyBee package could not produce a canonical identity.
    Capability,
    /// Fixed host allocation could not be reserved.
    Allocation,
    /// Canonical diagnostic validation or encoding failed.
    Diagnostic(DiagnosticError),
}

impl From<DiagnosticError> for DiagnosticFixtureError {
    fn from(error: DiagnosticError) -> Self {
        Self::Diagnostic(error)
    }
}

/// Immutable canonical TinyBee overview and edge capture generated without I/O.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TinyBeeDiagnosticFixture {
    capability: CapabilityIdentity,
    overview: Vec<u8>,
    digital_capture: Vec<u8>,
}

impl TinyBeeDiagnosticFixture {
    /// Exact current 8 MiB TinyBee package identity bound into both records.
    pub const fn capability(&self) -> CapabilityIdentity {
        self.capability
    }

    /// Complete canonical `ALMOVW01` bytes.
    pub fn overview_bytes(&self) -> &[u8] {
        &self.overview
    }

    /// Complete canonical `ALMDIG01` bytes.
    pub fn digital_capture_bytes(&self) -> &[u8] {
        &self.digital_capture
    }

    /// Replays the privately retained canonical overview under interactive limits.
    pub fn overview(&self) -> ResourceOverviewView<'_> {
        decode_resource_overview(&self.overview, DiagnosticLimits::interactive())
            .expect("fixture construction retained a validated overview")
    }

    /// Replays the privately retained canonical digital capture.
    pub fn digital_capture(&self) -> DigitalCaptureView<'_> {
        decode_digital_capture(&self.digital_capture, DiagnosticLimits::interactive())
            .expect("fixture construction retained a validated digital capture")
    }
}

/// Builds the current deterministic TinyBee diagnostic fixture.
///
/// The package identity is calculated from the current sibling source. The
/// resulting records contain only simulated values and are safe to construct
/// on a host with a real board attached.
pub fn tinybee_diagnostic_fixture() -> Result<TinyBeeDiagnosticFixture, DiagnosticFixtureError> {
    let capability = calculate_identity(&board_mks_tinybee::PACKAGE)
        .map_err(|_| DiagnosticFixtureError::Capability)?;
    let context = DiagnosticContext {
        device_id: TINYBEE_SIMULATOR_DEVICE_ID,
        boot_id: BootId::new([0x53; BOOT_ID_BYTES])
            .map_err(|_| DiagnosticFixtureError::Capability)?,
        capability,
        config_digest: Digest([0x43; 32]),
        clock_frequency_hz: TINYBEE_SIMULATOR_CLOCK_HZ,
    };
    tinybee_diagnostic_fixture_for_context(context)
}

/// Builds the deterministic records for an explicit service-owned context.
///
/// This is used by authenticated HTTP simulations whose boot identity differs
/// from the standalone fixture. The current 8 MiB TinyBee capability identity
/// remains mandatory; callers cannot relabel these resources as another board.
pub fn tinybee_diagnostic_fixture_for_context(
    context: DiagnosticContext,
) -> Result<TinyBeeDiagnosticFixture, DiagnosticFixtureError> {
    let capability = calculate_identity(&board_mks_tinybee::PACKAGE)
        .map_err(|_| DiagnosticFixtureError::Capability)?;
    if context.capability != capability {
        return Err(DiagnosticFixtureError::Capability);
    }

    let samples = tinybee_samples();
    let overview_document = ResourceOverviewDocument {
        context,
        flags: OverviewFlags(OverviewFlags::SIMULATED),
        snapshot_cycle: DeviceCycle(1_250_000),
        sequence: 1,
        samples: &samples,
    };
    let overview_len = resource_overview_encoded_len(samples.len())?;
    let mut overview = reserved_zero_vec(overview_len)?;
    let used = encode_resource_overview(&overview_document, &mut overview)?;
    overview.truncate(used);
    let overview_view = decode_resource_overview(&overview, DiagnosticLimits::interactive())?;
    if overview_view.context().capability != capability {
        return Err(DiagnosticFixtureError::Capability);
    }

    let channels = tinybee_channels();
    let transitions = tinybee_transitions();
    let capture_document = DigitalCaptureDocument {
        context,
        flags: DigitalCaptureFlags(DigitalCaptureFlags::SIMULATED),
        capture_id: CaptureId::new(*b"TINYBEE-SIM-0001")?,
        start_cycle: DeviceCycle(2_000_000),
        end_cycle_exclusive: DeviceCycle(2_002_000),
        requested_pretrigger_cycles: 500,
        requested_posttrigger_cycles: 1_500,
        trigger_cycle: DeviceCycle(2_000_500),
        trigger_channel_index: 2,
        trigger_condition: DigitalTriggerCondition::Rising,
        state: DigitalCaptureState::TriggeredComplete,
        trigger_transition_index: 4,
        transition_capacity: 64,
        retained_event_stride: 1,
        quality_flags: CaptureQualityFlags(CaptureQualityFlags::CLOCK_UNQUALIFIED),
        channels: &channels,
        transitions: &transitions,
    };
    let capture_len = digital_capture_encoded_len(channels.len(), transitions.len())?;
    let mut digital_capture = reserved_zero_vec(capture_len)?;
    let used = encode_digital_capture(&capture_document, &mut digital_capture)?;
    digital_capture.truncate(used);
    let capture_view = decode_digital_capture(&digital_capture, DiagnosticLimits::interactive())?;
    if capture_view.context().capability != capability {
        return Err(DiagnosticFixtureError::Capability);
    }

    Ok(TinyBeeDiagnosticFixture {
        capability,
        overview,
        digital_capture,
    })
}

/// Builds deterministic simulated input evidence for one exact armed request.
///
/// The first host provider intentionally supports immediate captures only. It
/// mirrors every requested context, channel, duration, capacity, and attempt
/// identity, so the ordinary service and client validators remain authoritative.
pub fn simulated_immediate_waveform_capture(
    configuration: WaveformConfigureView<'_>,
    not_before: DeviceCycle,
) -> Result<Vec<u8>, DiagnosticFixtureError> {
    let (requested_pretrigger_cycles, requested_posttrigger_cycles) =
        configuration.requested_window_cycles();
    let (trigger_channel_index, trigger_condition) = configuration.trigger();
    if requested_pretrigger_cycles != 0
        || requested_posttrigger_cycles == 0
        || trigger_channel_index != u16::MAX
        || trigger_condition != DigitalTriggerCondition::Immediate
    {
        return Err(DiagnosticFixtureError::Diagnostic(DiagnosticError::Trigger));
    }
    let (earliest, latest) = configuration.trigger_deadline_window();
    let start_cycle = DeviceCycle(not_before.0.max(earliest.0));
    if start_cycle > latest {
        return Err(DiagnosticFixtureError::Diagnostic(DiagnosticError::Window));
    }
    let end_cycle_exclusive = DeviceCycle(
        start_cycle
            .0
            .checked_add(requested_posttrigger_cycles)
            .ok_or(DiagnosticFixtureError::Diagnostic(DiagnosticError::Window))?,
    );

    let channel_count = configuration.channel_count();
    let mut channels = Vec::new();
    channels
        .try_reserve_exact(channel_count)
        .map_err(|_| DiagnosticFixtureError::Allocation)?;
    let mut levels = Vec::new();
    levels
        .try_reserve_exact(channel_count)
        .map_err(|_| DiagnosticFixtureError::Allocation)?;
    for (index, resource) in configuration.channels().enumerate() {
        let initial_level = if index % 2 == 0 {
            DigitalLevel::Low
        } else {
            DigitalLevel::High
        };
        channels.push(simulated_channel(resource, initial_level));
        levels.push(initial_level);
    }

    let capacity = usize::try_from(configuration.transition_capacity())
        .map_err(|_| DiagnosticFixtureError::Allocation)?;
    let duration_events =
        usize::try_from(requested_posttrigger_cycles.saturating_sub(1)).unwrap_or(usize::MAX);
    let event_count = capacity
        .min(channel_count.saturating_mul(4))
        .min(duration_events);
    let mut transitions = Vec::new();
    transitions
        .try_reserve_exact(event_count)
        .map_err(|_| DiagnosticFixtureError::Allocation)?;
    let denominator = u64::try_from(event_count.saturating_add(1))
        .map_err(|_| DiagnosticFixtureError::Allocation)?;
    for event in 0..event_count {
        let channel_index = event % channel_count;
        let level = match levels[channel_index] {
            DigitalLevel::Low => DigitalLevel::High,
            DigitalLevel::High | DigitalLevel::Unknown => DigitalLevel::Low,
        };
        levels[channel_index] = level;
        let ordinal = u64::try_from(event.saturating_add(1))
            .map_err(|_| DiagnosticFixtureError::Allocation)?;
        let offset_cycles = requested_posttrigger_cycles.saturating_mul(ordinal) / denominator;
        transitions.push(DigitalTransition {
            offset_cycles,
            channel_index: u16::try_from(channel_index)
                .map_err(|_| DiagnosticFixtureError::Allocation)?,
            level,
            flags: DigitalTransitionFlags(0),
        });
    }

    let document = DigitalCaptureDocument {
        context: configuration.context(),
        flags: DigitalCaptureFlags(DigitalCaptureFlags::SIMULATED),
        capture_id: configuration.capture_id(),
        start_cycle,
        end_cycle_exclusive,
        requested_pretrigger_cycles,
        requested_posttrigger_cycles,
        trigger_cycle: start_cycle,
        trigger_channel_index,
        trigger_condition,
        state: DigitalCaptureState::TriggeredComplete,
        trigger_transition_index: u32::MAX,
        transition_capacity: configuration.transition_capacity(),
        retained_event_stride: 1,
        quality_flags: CaptureQualityFlags(CaptureQualityFlags::CLOCK_UNQUALIFIED),
        channels: &channels,
        transitions: &transitions,
    };
    let encoded_len = digital_capture_encoded_len(channels.len(), transitions.len())?;
    let mut encoded = reserved_zero_vec(encoded_len)?;
    let used = encode_digital_capture(&document, &mut encoded)?;
    encoded.truncate(used);
    Ok(encoded)
}

fn reserved_zero_vec(length: usize) -> Result<Vec<u8>, DiagnosticFixtureError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| DiagnosticFixtureError::Allocation)?;
    bytes.resize(length, 0);
    Ok(bytes)
}

fn tinybee_samples() -> [ResourceOverviewSample; TINYBEE_SIMULATOR_CHANNELS] {
    [
        simulated_boolean_sample(ResourceId::Gpio(22), 1_249_930, false),
        simulated_boolean_sample(ResourceId::Gpio(32), 1_249_940, true),
        simulated_boolean_sample(ResourceId::Gpio(33), 1_249_950, false),
        simulated_boolean_sample(ResourceId::Gpio(35), 1_249_960, true),
    ]
}

const fn simulated_boolean_sample(
    resource: ResourceId,
    captured_cycle: u64,
    value: bool,
) -> ResourceOverviewSample {
    ResourceOverviewSample {
        resource,
        provenance: SampleProvenance::Simulated,
        quality: SampleQuality::Valid,
        quality_flags: SampleQualityFlags(SampleQualityFlags::DEBOUNCED),
        captured_cycle: DeviceCycle(captured_cycle),
        value: ResourceValue::Boolean(value),
    }
}

fn tinybee_channels() -> [DigitalCaptureChannel; TINYBEE_SIMULATOR_CHANNELS] {
    [
        simulated_channel(ResourceId::Gpio(22), DigitalLevel::Low),
        simulated_channel(ResourceId::Gpio(32), DigitalLevel::High),
        simulated_channel(ResourceId::Gpio(33), DigitalLevel::Low),
        simulated_channel(ResourceId::Gpio(35), DigitalLevel::High),
    ]
}

const fn simulated_channel(
    resource: ResourceId,
    initial_level: DigitalLevel,
) -> DigitalCaptureChannel {
    DigitalCaptureChannel {
        resource,
        initial_level,
        source: DigitalAcquisitionSource::Simulated,
        flags: DigitalChannelFlags(DigitalChannelFlags::DEBOUNCED),
    }
}

fn tinybee_transitions() -> [DigitalTransition; TINYBEE_SIMULATOR_TRANSITIONS] {
    [
        transition(100, 0, DigitalLevel::High),
        transition(160, 0, DigitalLevel::Low),
        transition(250, 1, DigitalLevel::Low),
        transition(310, 1, DigitalLevel::High),
        transition(500, 2, DigitalLevel::High),
        transition(620, 2, DigitalLevel::Low),
        transition(700, 3, DigitalLevel::Low),
        transition(780, 3, DigitalLevel::High),
        transition(900, 0, DigitalLevel::High),
        transition(960, 0, DigitalLevel::Low),
        transition(1_100, 1, DigitalLevel::Low),
        transition(1_250, 1, DigitalLevel::High),
        transition(1_400, 2, DigitalLevel::High),
        transition(1_500, 2, DigitalLevel::Low),
    ]
}

const fn transition(
    offset_cycles: u64,
    channel_index: u16,
    level: DigitalLevel,
) -> DigitalTransition {
    DigitalTransition {
        offset_cycles,
        channel_index,
        level,
        flags: DigitalTransitionFlags(0),
    }
}

#[cfg(test)]
mod tests {
    use alumina_diagnostics::transport::{
        DiagnosticTransportLimits, SubscriptionId, TelemetryEventView, TelemetryPhase,
        TelemetrySessionRequest, TelemetrySubscribeFlags, TelemetrySubscribeRequest,
        WaveformConfigureFlags, WaveformConfigureRequest, WaveformPhase, WaveformReadRequest,
        WaveformSessionRequest, decode_telemetry_event, decode_telemetry_subscribe,
        decode_waveform_chunk, decode_waveform_configure, encode_telemetry_subscribe,
        encode_waveform_configure,
    };
    use alumina_service::diagnostics::{
        DiagnosticProviderPolicy, DiagnosticServiceError, DiagnosticServiceState,
    };

    use super::*;

    const SELECTED_RESOURCES: [ResourceId; 4] = [
        ResourceId::Gpio(22),
        ResourceId::Gpio(32),
        ResourceId::Gpio(33),
        ResourceId::Gpio(35),
    ];

    type TestService = DiagnosticServiceState<176, 432, 208, 2_048>;
    type DisabledService = DiagnosticServiceState<0, 0, 0, 0>;

    fn encode_subscription(context: DiagnosticContext, id: u64) -> [u8; 176] {
        let mut encoded = [0_u8; 176];
        let used = encode_telemetry_subscribe(
            &TelemetrySubscribeRequest {
                subscription_id: SubscriptionId::new(id).unwrap(),
                context,
                flags: TelemetrySubscribeFlags(TelemetrySubscribeFlags::LATEST_ONLY),
                minimum_period_cycles: 10_000,
                maximum_event_bytes: 432,
                resources: &SELECTED_RESOURCES,
            },
            &mut encoded,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        assert_eq!(used, encoded.len());
        encoded
    }

    fn encode_configuration(context: DiagnosticContext) -> [u8; 208] {
        let mut encoded = [0_u8; 208];
        let used = encode_waveform_configure(
            &WaveformConfigureRequest {
                capture_id: CaptureId::new(*b"TINYBEE-SIM-0001").unwrap(),
                context,
                flags: WaveformConfigureFlags(WaveformConfigureFlags::EDGE_TIMESTAMPS),
                requested_pretrigger_cycles: 500,
                requested_posttrigger_cycles: 1_500,
                earliest_trigger_cycle: DeviceCycle(2_000_400),
                latest_trigger_cycle: DeviceCycle(2_000_600),
                transition_capacity: 64,
                maximum_chunk_bytes: 168,
                trigger_channel_index: 2,
                trigger_condition: DigitalTriggerCondition::Rising,
                channels: &SELECTED_RESOURCES,
            },
            &mut encoded,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        assert_eq!(used, encoded.len());
        encoded
    }

    fn encode_immediate_configuration(context: DiagnosticContext) -> [u8; 208] {
        let mut encoded = [0_u8; 208];
        let used = encode_waveform_configure(
            &WaveformConfigureRequest {
                capture_id: CaptureId::new(*b"SIM-IMMEDIATE-01").unwrap(),
                context,
                flags: WaveformConfigureFlags(WaveformConfigureFlags::EDGE_TIMESTAMPS),
                requested_pretrigger_cycles: 0,
                requested_posttrigger_cycles: 2_000,
                earliest_trigger_cycle: DeviceCycle(2_000_000),
                latest_trigger_cycle: DeviceCycle(2_100_000),
                transition_capacity: 64,
                maximum_chunk_bytes: 168,
                trigger_channel_index: u16::MAX,
                trigger_condition: DigitalTriggerCondition::Immediate,
                channels: &SELECTED_RESOURCES,
            },
            &mut encoded,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        assert_eq!(used, encoded.len());
        encoded
    }

    #[test]
    fn tinybee_fixture_is_canonical_capability_bound_and_explicitly_simulated() {
        let fixture = tinybee_diagnostic_fixture().unwrap();
        let expected = calculate_identity(&board_mks_tinybee::PACKAGE).unwrap();
        assert_eq!(fixture.capability(), expected);
        assert_eq!(fixture.overview_bytes().len(), 320);
        assert_eq!(fixture.digital_capture_bytes().len(), 512);

        let overview = fixture.overview();
        assert!(overview.flags().contains(OverviewFlags::SIMULATED));
        assert_eq!(overview.sample_count(), 4);
        let selected = overview.sample(ResourceId::Gpio(33)).unwrap();
        assert_eq!(selected.provenance, SampleProvenance::Simulated);
        assert_eq!(selected.value, ResourceValue::Boolean(false));
        assert_eq!(overview.snapshot_cycle().0 - selected.captured_cycle.0, 50);

        let capture = fixture.digital_capture();
        assert!(capture.flags().contains(DigitalCaptureFlags::SIMULATED));
        assert_eq!(capture.channel_count(), 4);
        assert_eq!(capture.transition_count(), 14);
        assert_eq!(capture.trigger().0, DeviceCycle(2_000_500));
        assert_eq!(
            capture.channel(capture.trigger().1).unwrap().resource,
            ResourceId::Gpio(33)
        );
        assert_eq!(capture.trigger().2, DigitalTriggerCondition::Rising);
        assert!(
            capture
                .quality_flags()
                .contains(CaptureQualityFlags::CLOCK_UNQUALIFIED)
        );
    }

    #[test]
    fn fixture_bytes_are_deterministic_and_do_not_depend_on_runtime_state() {
        let left = tinybee_diagnostic_fixture().unwrap();
        let right = tinybee_diagnostic_fixture().unwrap();
        assert_eq!(left, right);
    }

    #[test]
    fn latest_only_service_session_reports_replacement_and_exact_acknowledgement() {
        let fixture = tinybee_diagnostic_fixture().unwrap();
        let context = fixture.overview().context();
        let request_bytes = encode_subscription(context, 7);
        let subscription =
            decode_telemetry_subscribe(&request_bytes, DiagnosticTransportLimits::native_control())
                .unwrap();
        let reference = TelemetrySessionRequest {
            subscription_id: subscription.subscription_id(),
            subscription_digest: subscription.digest(),
        };
        let mut service = TestService::new(
            context,
            DiagnosticProviderPolicy::SIMULATED,
            DiagnosticTransportLimits::native_control(),
            DiagnosticLimits::interactive(),
        );

        let admitted = service.subscribe(&request_bytes).unwrap();
        assert_eq!(admitted.phase, TelemetryPhase::Active);
        assert_eq!(admitted.next_event_sequence, 1);
        assert_eq!(service.subscribe(&request_bytes).unwrap(), admitted);

        service.publish_overview(fixture.overview_bytes()).unwrap();
        let first = service.pending_telemetry_event().unwrap();
        let first =
            decode_telemetry_event(first, subscription, DiagnosticLimits::interactive()).unwrap();
        assert_eq!(first.event_sequence(), 1);
        assert_eq!(first.dropped_events(), 0);

        let mut too_early = fixture.overview_bytes().to_vec();
        too_early[128..136].copy_from_slice(&1_259_999_u64.to_le_bytes());
        too_early[136..144].copy_from_slice(&2_u64.to_le_bytes());
        assert_eq!(
            service.publish_overview(&too_early),
            Err(DiagnosticServiceError::Deadline)
        );
        let mut replacement = fixture.overview_bytes().to_vec();
        replacement[128..136].copy_from_slice(&1_260_000_u64.to_le_bytes());
        replacement[136..144].copy_from_slice(&2_u64.to_le_bytes());
        service.publish_overview(&replacement).unwrap();
        let second: TelemetryEventView<'_> = decode_telemetry_event(
            service.pending_telemetry_event().unwrap(),
            subscription,
            DiagnosticLimits::interactive(),
        )
        .unwrap();
        assert_eq!(second.event_sequence(), 2);
        assert_eq!(second.dropped_events(), 1);
        assert_eq!(
            service.acknowledge_telemetry_event(reference, 1),
            Err(DiagnosticServiceError::Conflict)
        );
        let acknowledged = service.acknowledge_telemetry_event(reference, 2).unwrap();
        assert_eq!(acknowledged.published_events, 1);
        assert_eq!(acknowledged.dropped_events, 1);
        assert!(!acknowledged.pending);

        let stopped = service.unsubscribe(reference).unwrap();
        assert_eq!(stopped.phase, TelemetryPhase::Unsubscribed);
        assert_eq!(service.unsubscribe(reference).unwrap(), stopped);
        let other = encode_subscription(context, 8);
        assert_eq!(
            service.subscribe(&other).unwrap().phase,
            TelemetryPhase::Active
        );

        let mut changed_context = context;
        changed_context.config_digest = Digest([0x99; 32]);
        assert!(service.rebind_context(changed_context));
        assert_eq!(
            service.telemetry_status(TelemetrySessionRequest {
                subscription_id: SubscriptionId::new(8).unwrap(),
                subscription_digest: decode_telemetry_subscribe(
                    &other,
                    DiagnosticTransportLimits::native_control(),
                )
                .unwrap()
                .digest(),
            }),
            Err(DiagnosticServiceError::NotFound)
        );
        assert_eq!(
            service.subscribe(&other),
            Err(DiagnosticServiceError::Conflict)
        );
    }

    #[test]
    fn waveform_service_retains_exact_record_and_exposes_loss_recoverable_ranges() {
        let fixture = tinybee_diagnostic_fixture().unwrap();
        let context = fixture.digital_capture().context();
        let configuration_bytes = encode_configuration(context);
        let configuration = decode_waveform_configure(
            &configuration_bytes,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        let reference = WaveformSessionRequest {
            capture_id: configuration.capture_id(),
            configure_digest: configuration.digest(),
        };
        let mut service = TestService::new(
            context,
            DiagnosticProviderPolicy::SIMULATED,
            DiagnosticTransportLimits::native_control(),
            DiagnosticLimits::interactive(),
        );

        let configured = service.configure_waveform(&configuration_bytes).unwrap();
        assert_eq!(configured.phase, WaveformPhase::Configured);
        assert_eq!(
            service.configure_waveform(&configuration_bytes).unwrap(),
            configured
        );
        let armed = service
            .arm_waveform(reference, DeviceCycle(2_000_100))
            .unwrap();
        assert_eq!(armed.phase, WaveformPhase::Armed);
        assert_eq!(armed.generation, 1);
        assert_eq!(
            service
                .arm_waveform(reference, DeviceCycle(2_000_200))
                .unwrap(),
            armed
        );

        let complete = service
            .retain_waveform_capture(fixture.digital_capture_bytes())
            .unwrap();
        assert_eq!(complete.phase, WaveformPhase::Complete);
        assert_eq!(complete.record_bytes, 512);
        assert!(complete.triggered);

        let mut chunk_bytes = [0_u8; 312];
        let first_len = service
            .pending_waveform_chunk(&mut chunk_bytes)
            .unwrap()
            .unwrap();
        assert_eq!(first_len, 312);
        let first = decode_waveform_chunk(
            &chunk_bytes[..first_len],
            reference,
            complete.record_digest,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        assert_eq!(first.offset(), 0);
        assert_eq!(first.end_offset(), 168);
        let lost = service.drop_waveform_chunk(reference, 168).unwrap();
        assert_eq!(lost.dropped_chunks, 1);
        assert_eq!(lost.published_bytes, 168);

        let second_len = service
            .pending_waveform_chunk(&mut chunk_bytes)
            .unwrap()
            .unwrap();
        let second = decode_waveform_chunk(
            &chunk_bytes[..second_len],
            reference,
            complete.record_digest,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        assert_eq!(second.offset(), 168);
        let progressed = service
            .acknowledge_waveform_chunk(reference, second.end_offset())
            .unwrap();
        assert_eq!(progressed.published_bytes, 336);
        assert_eq!(progressed.dropped_chunks, 1);

        let read = WaveformReadRequest {
            capture_id: reference.capture_id,
            configure_digest: reference.configure_digest,
            record_digest: complete.record_digest,
            offset: 500,
            maximum_bytes: 12,
        };
        let recovered_len = service.read_waveform(read, &mut chunk_bytes).unwrap();
        let recovered = decode_waveform_chunk(
            &chunk_bytes[..recovered_len],
            reference,
            complete.record_digest,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        assert_eq!(recovered.bytes(), &fixture.digital_capture_bytes()[500..]);
        assert!(recovered.is_final());

        let stopped = service.stop_waveform(reference).unwrap();
        assert_eq!(stopped.phase, WaveformPhase::Stopped);
        assert_eq!(stopped.record_bytes, 0);
        assert_eq!(service.stop_waveform(reference).unwrap(), stopped);
    }

    #[test]
    fn dynamic_immediate_provider_mirrors_the_admitted_configuration_exactly() {
        let fixture = tinybee_diagnostic_fixture().unwrap();
        let context = fixture.digital_capture().context();
        let configuration_bytes = encode_immediate_configuration(context);
        let configuration = decode_waveform_configure(
            &configuration_bytes,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        let reference = WaveformSessionRequest {
            capture_id: configuration.capture_id(),
            configure_digest: configuration.digest(),
        };
        let mut service = TestService::new(
            context,
            DiagnosticProviderPolicy::SIMULATED,
            DiagnosticTransportLimits::native_control(),
            DiagnosticLimits::interactive(),
        );
        service.configure_waveform(&configuration_bytes).unwrap();
        service
            .arm_waveform(reference, DeviceCycle(2_000_100))
            .unwrap();
        let record = simulated_immediate_waveform_capture(
            service.armed_waveform_configuration().unwrap(),
            DeviceCycle(2_000_100),
        )
        .unwrap();
        let status = service.retain_waveform_capture(&record).unwrap();
        let capture = decode_digital_capture(&record, DiagnosticLimits::interactive()).unwrap();
        assert_eq!(status.phase, WaveformPhase::Complete);
        assert_eq!(capture.capture_id(), configuration.capture_id());
        assert_eq!(capture.context(), context);
        assert_eq!(capture.requested_window_cycles(), (0, 2_000));
        assert_eq!(capture.cycle_window().0, DeviceCycle(2_000_100));
        assert_eq!(capture.channel_count(), SELECTED_RESOURCES.len());
        assert_eq!(capture.transition_count(), 16);
        assert!(capture.flags().contains(DigitalCaptureFlags::SIMULATED));
    }

    #[test]
    fn unconnected_hardware_policy_never_accepts_an_unserviceable_session() {
        let fixture = tinybee_diagnostic_fixture().unwrap();
        let context = fixture.overview().context();
        let mut service = DisabledService::new(
            context,
            DiagnosticProviderPolicy::NONE,
            DiagnosticTransportLimits::native_control(),
            DiagnosticLimits::interactive(),
        );
        assert_eq!(
            service.subscribe(&encode_subscription(context, 7)),
            Err(DiagnosticServiceError::Unsupported)
        );
        assert_eq!(
            service.configure_waveform(&encode_configuration(context)),
            Err(DiagnosticServiceError::Unsupported)
        );
    }
}
