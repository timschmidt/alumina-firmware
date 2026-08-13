//! Deterministic evidence-only diagnostic fixtures.
//!
//! These records are deliberately marked `Simulated` at both the document and
//! per-value/acquisition layers. They open no device, GPIO, network interface,
//! command path, diagnostic lease, or output authority.

use alumina_board::ResourceId;
use alumina_capability::{CapabilityIdentity, calculate_identity};
use alumina_clock::{BOOT_ID_BYTES, BootId};
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
    use super::*;

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
}
