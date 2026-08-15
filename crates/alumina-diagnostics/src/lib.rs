#![no_std]
#![doc = "Canonical bounded diagnostic evidence shared by firmware, simulation, and UI."]
#![warn(missing_docs)]

use core::fmt;

use alumina_board::ResourceId;
use alumina_capability::{CapabilityIdentity, decode_resource_id, encode_resource_id};
use alumina_clock::{BOOT_ID_BYTES, BootId};
use alumina_protocol::{DeviceCycle, DeviceId, Digest};
use alumina_safety::SafetyInputStatus;

/// Canonical authenticated telemetry and waveform transport bodies.
pub mod transport;

/// Exact resource-overview document magic.
pub const RESOURCE_OVERVIEW_MAGIC: [u8; 8] = *b"ALMOVW01";
/// Exact digital-capture document magic.
pub const DIGITAL_CAPTURE_MAGIC: [u8; 8] = *b"ALMDIG01";
/// Exact resource-overview schema version.
pub const RESOURCE_OVERVIEW_VERSION: u16 = 1;
/// Exact digital-capture schema version.
pub const DIGITAL_CAPTURE_VERSION: u16 = 1;
/// Bytes before the first resource-overview sample.
pub const RESOURCE_OVERVIEW_HEADER_BYTES: usize = 160;
/// Bytes in one resource-overview sample.
pub const RESOURCE_OVERVIEW_SAMPLE_BYTES: usize = 40;
/// Bytes before the first digital-capture channel.
pub const DIGITAL_CAPTURE_HEADER_BYTES: usize = 224;
/// Bytes in one digital-capture channel record.
pub const DIGITAL_CAPTURE_CHANNEL_BYTES: usize = 16;
/// Bytes in one digital transition record.
pub const DIGITAL_TRANSITION_BYTES: usize = 16;
/// Exact real-time input-snapshot document magic.
pub const REALTIME_INPUT_SNAPSHOT_MAGIC: [u8; 8] = *b"ALMRTI01";
/// Exact real-time input-snapshot schema version.
pub const REALTIME_INPUT_SNAPSHOT_VERSION: u16 = 1;
/// Bytes before the first real-time input record.
pub const REALTIME_INPUT_SNAPSHOT_HEADER_BYTES: usize = 48;
/// Bytes in one real-time input record.
pub const REALTIME_INPUT_SNAPSHOT_RECORD_BYTES: usize = 16;
/// Maximum records admitted by the fixed mask representation.
pub const MAX_REALTIME_INPUT_SNAPSHOT_INPUTS: usize = 32;
/// Format-level maximum digital channels in one capture.
///
/// This is independent of a caller selecting an even smaller decode policy and
/// bounds the fixed validation state used before any allocation.
pub const MAX_DIGITAL_CAPTURE_CHANNELS: usize = 256;

const OVERVIEW_FLAG_SIMULATED: u16 = 1 << 0;
const CAPTURE_FLAG_SIMULATED: u16 = 1 << 0;
const SAMPLE_QUALITY_KNOWN: u16 = SampleQualityFlags::DEBOUNCED
    | SampleQualityFlags::FAULT_LATCHED
    | SampleQualityFlags::SATURATED
    | SampleQualityFlags::OUT_OF_RANGE
    | SampleQualityFlags::TIMESTAMP_UNCERTAIN;
const CHANNEL_FLAGS_KNOWN: u16 = DigitalChannelFlags::INVERTED | DigitalChannelFlags::DEBOUNCED;
const CAPTURE_QUALITY_KNOWN: u32 = CaptureQualityFlags::OVERFLOW
    | CaptureQualityFlags::PRETRIGGER_TRUNCATED
    | CaptureQualityFlags::POSTTRIGGER_TRUNCATED
    | CaptureQualityFlags::DECIMATED
    | CaptureQualityFlags::CLOCK_UNQUALIFIED
    | CaptureQualityFlags::DISCONTINUITY
    | CaptureQualityFlags::SOFTWARE_SAMPLED;
const TRANSITION_FLAGS_KNOWN: u8 = DigitalTransitionFlags::TIMESTAMP_UNCERTAIN;
const NO_TRIGGER_CHANNEL: u16 = u16::MAX;
const NO_TRIGGER_TRANSITION: u32 = u32::MAX;

/// Caller-selected decode ceilings applied before iteration or UI allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticLimits {
    /// Largest admitted complete overview document.
    pub maximum_overview_bytes: u32,
    /// Largest admitted sample table.
    pub maximum_overview_samples: u32,
    /// Largest admitted complete digital-capture document.
    pub maximum_capture_bytes: u32,
    /// Largest admitted channel table.
    pub maximum_capture_channels: u16,
    /// Largest admitted transition table.
    pub maximum_capture_transitions: u32,
}

impl DiagnosticLimits {
    /// Conservative interactive native/WASM inspection policy.
    pub const fn interactive() -> Self {
        Self {
            maximum_overview_bytes: 4 * 1_024 * 1_024,
            maximum_overview_samples: 4_096,
            maximum_capture_bytes: 16 * 1_024 * 1_024,
            maximum_capture_channels: 256,
            maximum_capture_transitions: 1_000_000,
        }
    }
}

impl Default for DiagnosticLimits {
    fn default() -> Self {
        Self::interactive()
    }
}

/// Identity and clock domain common to every diagnostic record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticContext {
    /// Stable physical or explicitly simulated device identity.
    pub device_id: DeviceId,
    /// Nonzero boot identity invalidating evidence across resets.
    pub boot_id: BootId,
    /// Complete canonical board-capability identity.
    pub capability: CapabilityIdentity,
    /// Active configuration digest, or zero while no configuration is active.
    pub config_digest: Digest,
    /// Integer frequency of the shared monotonic `DeviceCycle` domain.
    pub clock_frequency_hz: u64,
}

/// Resource-overview document flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct OverviewFlags(pub u16);

impl OverviewFlags {
    /// Every value is deterministic simulator evidence, never a measurement.
    pub const SIMULATED: u16 = OVERVIEW_FLAG_SIMULATED;

    /// Whether one flag bit is present.
    pub const fn contains(self, flag: u16) -> bool {
        self.0 & flag != 0
    }
}

/// How a low-rate value was obtained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SampleProvenance {
    /// Sampled from a physical peripheral.
    Measured = 1,
    /// Retained from a hardware or software latch.
    Latched = 2,
    /// Derived from other observations rather than directly sampled.
    Inferred = 3,
    /// Last value accepted by an output owner; not physical feedback.
    LastCommanded = 4,
    /// Produced by a deterministic simulator with no physical claim.
    Simulated = 5,
}

impl SampleProvenance {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Measured),
            2 => Some(Self::Latched),
            3 => Some(Self::Inferred),
            4 => Some(Self::LastCommanded),
            5 => Some(Self::Simulated),
            _ => None,
        }
    }
}

/// Semantic validity of one overview value at its captured cycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SampleQuality {
    /// Value satisfies the producing resource's current validity policy.
    Valid = 1,
    /// Value was retained beyond its declared freshness interval.
    Stale = 2,
    /// Resource could not supply a value.
    Unavailable = 3,
    /// Value is retained alongside a resource or acquisition fault.
    Faulted = 4,
}

impl SampleQuality {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Valid),
            2 => Some(Self::Stale),
            3 => Some(Self::Unavailable),
            4 => Some(Self::Faulted),
            _ => None,
        }
    }
}

/// Orthogonal quality annotations for one overview value.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct SampleQualityFlags(pub u16);

impl SampleQualityFlags {
    /// Input passed its configured stable/debounce policy.
    pub const DEBOUNCED: u16 = 1 << 0;
    /// Value is retained with a latched fault.
    pub const FAULT_LATCHED: u16 = 1 << 1;
    /// Acquisition saturated at a representable endpoint.
    pub const SATURATED: u16 = 1 << 2;
    /// Value violates its configured valid range.
    pub const OUT_OF_RANGE: u16 = 1 << 3;
    /// Timestamp has uncertainty beyond the normal clock model.
    pub const TIMESTAMP_UNCERTAIN: u16 = 1 << 4;

    /// Whether one quality bit is present.
    pub const fn contains(self, flag: u16) -> bool {
        self.0 & flag != 0
    }
}

/// Exact scalar carried by a low-rate overview sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceValue {
    /// No value is available; all payload bytes are canonical zero.
    Unavailable,
    /// One logical level.
    Boolean(bool),
    /// One exact unsigned integer in configuration-defined units.
    Unsigned(u64),
    /// One exact signed integer in configuration-defined units.
    Signed(i64),
    /// One reduced exact ratio in configuration-defined units.
    ExactRatio {
        /// Signed numerator.
        numerator: i64,
        /// Positive denominator; numerator and denominator must be coprime.
        denominator: u64,
    },
}

/// One typed resource value and the cycle at which it was obtained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceOverviewSample {
    /// Exact board resource selector.
    pub resource: ResourceId,
    /// Source of the value.
    pub provenance: SampleProvenance,
    /// Semantic validity at capture time.
    pub quality: SampleQuality,
    /// Additional nonexclusive quality facts.
    pub quality_flags: SampleQualityFlags,
    /// Cycle at which the value was sampled, latched, inferred, or commanded.
    pub captured_cycle: DeviceCycle,
    /// Exact scalar, or `Unavailable` with unavailable quality.
    pub value: ResourceValue,
}

/// Borrowed resource-overview document supplied to the canonical encoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceOverviewDocument<'a> {
    /// Device, boot, capability, configuration, and clock identity.
    pub context: DiagnosticContext,
    /// Document-wide provenance flags.
    pub flags: OverviewFlags,
    /// Cycle at which the producing service assembled this snapshot.
    pub snapshot_cycle: DeviceCycle,
    /// Monotonic overview sequence within this boot and subscription.
    pub sequence: u64,
    /// Strictly increasing typed resource records; omission conveys no value.
    pub samples: &'a [ResourceOverviewSample],
}

/// One configuration-stable real-time input slot and its exact latest sample cycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeInputRecord {
    /// Exact board resource sampled by this configuration slot.
    pub resource: ResourceId,
    /// Most recent physical acquisition cycle, including pre-debounce samples.
    pub sampled_at: Option<DeviceCycle>,
}

/// Borrowed core-1 input snapshot supplied to the canonical encoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeInputSnapshotDocument<'a> {
    /// Aggregate semantic input facts in the same slot order as `records`.
    pub status: SafetyInputStatus,
    /// Exact configuration-stable resource mapping and acquisition cycles.
    pub records: &'a [RealtimeInputRecord],
}

/// Allocation-free validated view of one core-1 input snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeInputSnapshotView<'a> {
    status: SafetyInputStatus,
    records: &'a [u8],
    record_count: usize,
}

impl<'a> RealtimeInputSnapshotView<'a> {
    /// Aggregate semantic facts for every retained slot.
    pub const fn status(self) -> SafetyInputStatus {
        self.status
    }

    /// Number of configuration-stable input slots.
    pub const fn record_count(self) -> usize {
        self.record_count
    }

    /// Iterates validated records in configuration-stable slot order.
    pub fn records(self) -> RealtimeInputRecordIter<'a> {
        RealtimeInputRecordIter {
            records: self.records,
            offset: 0,
        }
    }

    /// Returns one validated record by its configuration-stable slot.
    pub fn record(self, slot: usize) -> Option<RealtimeInputRecord> {
        self.records().nth(slot)
    }
}

/// Iterator over validated fixed-size real-time input records.
#[derive(Clone, Debug)]
pub struct RealtimeInputRecordIter<'a> {
    records: &'a [u8],
    offset: usize,
}

impl Iterator for RealtimeInputRecordIter<'_> {
    type Item = RealtimeInputRecord;

    fn next(&mut self) -> Option<Self::Item> {
        let end = self
            .offset
            .checked_add(REALTIME_INPUT_SNAPSHOT_RECORD_BYTES)?;
        let record = self.records.get(self.offset..end)?;
        self.offset = end;
        Some(decode_realtime_input_record(record).expect("validated input record remains valid"))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.records.len() - self.offset) / REALTIME_INPUT_SNAPSHOT_RECORD_BYTES;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for RealtimeInputRecordIter<'_> {}

/// Allocation-free validated view of a canonical resource overview.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceOverviewView<'a> {
    context: DiagnosticContext,
    flags: OverviewFlags,
    snapshot_cycle: DeviceCycle,
    sequence: u64,
    records: &'a [u8],
    sample_count: usize,
}

impl<'a> ResourceOverviewView<'a> {
    /// Device, boot, capability, configuration, and clock identity.
    pub const fn context(self) -> DiagnosticContext {
        self.context
    }

    /// Document-wide provenance flags.
    pub const fn flags(self) -> OverviewFlags {
        self.flags
    }

    /// Snapshot assembly cycle.
    pub const fn snapshot_cycle(self) -> DeviceCycle {
        self.snapshot_cycle
    }

    /// Monotonic overview sequence.
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    /// Number of explicit records.
    pub const fn sample_count(self) -> usize {
        self.sample_count
    }

    /// Iterates validated records without allocation.
    pub fn samples(self) -> ResourceOverviewIter<'a> {
        ResourceOverviewIter {
            records: self.records,
            offset: 0,
        }
    }

    /// Returns one exact resource record if it is explicitly present.
    pub fn sample(self, resource: ResourceId) -> Option<ResourceOverviewSample> {
        self.samples().find(|sample| sample.resource == resource)
    }
}

/// Iterator over validated overview records.
#[derive(Clone, Debug)]
pub struct ResourceOverviewIter<'a> {
    records: &'a [u8],
    offset: usize,
}

impl Iterator for ResourceOverviewIter<'_> {
    type Item = ResourceOverviewSample;

    fn next(&mut self) -> Option<Self::Item> {
        let end = self.offset.checked_add(RESOURCE_OVERVIEW_SAMPLE_BYTES)?;
        let record = self.records.get(self.offset..end)?;
        self.offset = end;
        Some(decode_overview_sample(record).expect("validated overview record remains valid"))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.records.len() - self.offset) / RESOURCE_OVERVIEW_SAMPLE_BYTES;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for ResourceOverviewIter<'_> {}

/// Digital-capture document flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct DigitalCaptureFlags(pub u16);

impl DigitalCaptureFlags {
    /// Every edge is deterministic simulator evidence, never a measurement.
    pub const SIMULATED: u16 = CAPTURE_FLAG_SIMULATED;

    /// Whether one flag bit is present.
    pub const fn contains(self, flag: u16) -> bool {
        self.0 & flag != 0
    }
}

/// Stable nonzero identity of one capture attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct CaptureId([u8; 16]);

impl CaptureId {
    /// Constructs an identity, rejecting the all-zero sentinel.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, DiagnosticError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(DiagnosticError::Identity)
    }

    /// Returns the exact wire bytes.
    pub const fn as_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// Initial or transitioned digital level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DigitalLevel {
    /// Level was not established at the beginning of the retained window.
    Unknown = 0,
    /// Logical low.
    Low = 1,
    /// Logical high.
    High = 2,
}

impl DigitalLevel {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Unknown),
            1 => Some(Self::Low),
            2 => Some(Self::High),
            _ => None,
        }
    }
}

/// Physical or simulated mechanism that acquired a digital channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DigitalAcquisitionSource {
    /// Deterministic host simulator.
    Simulated = 1,
    /// ESP RMT edge/timestamp acquisition.
    Rmt = 2,
    /// ESP pulse-counter acquisition.
    Pcnt = 3,
    /// Peripheral DMA acquisition.
    Dma = 4,
    /// Qualified bounded software sampling.
    Software = 5,
    /// Imported external analyzer evidence.
    ExternalAnalyzer = 6,
}

impl DigitalAcquisitionSource {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Simulated),
            2 => Some(Self::Rmt),
            3 => Some(Self::Pcnt),
            4 => Some(Self::Dma),
            5 => Some(Self::Software),
            6 => Some(Self::ExternalAnalyzer),
            _ => None,
        }
    }
}

/// Digital-channel annotations fixed for one capture.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct DigitalChannelFlags(pub u16);

impl DigitalChannelFlags {
    /// Logical levels are inverted from the physical sampled voltage.
    pub const INVERTED: u16 = 1 << 0;
    /// Channel records a configured debounced semantic input.
    pub const DEBOUNCED: u16 = 1 << 1;

    /// Whether one channel bit is present.
    pub const fn contains(self, flag: u16) -> bool {
        self.0 & flag != 0
    }
}

/// One capture channel in strict typed-resource order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DigitalCaptureChannel {
    /// Exact board resource selector.
    pub resource: ResourceId,
    /// Level at `start_cycle`, before retained transitions.
    pub initial_level: DigitalLevel,
    /// Acquisition mechanism.
    pub source: DigitalAcquisitionSource,
    /// Channel-level annotations.
    pub flags: DigitalChannelFlags,
}

/// Trigger predicate for one digital capture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DigitalTriggerCondition {
    /// Capture began without waiting for a channel event.
    Immediate = 0,
    /// Low-to-high transition.
    Rising = 1,
    /// High-to-low transition.
    Falling = 2,
    /// Either logical transition.
    Either = 3,
}

impl DigitalTriggerCondition {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Immediate),
            1 => Some(Self::Rising),
            2 => Some(Self::Falling),
            3 => Some(Self::Either),
            _ => None,
        }
    }
}

/// Terminal state of the retained capture attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DigitalCaptureState {
    /// Trigger fired and acquisition completed its retained window.
    TriggeredComplete = 1,
    /// Acquisition ended without observing the trigger.
    UntriggeredComplete = 2,
    /// Caller stopped acquisition before completion.
    Stopped = 3,
    /// Acquisition terminated because of a device-side fault.
    Faulted = 4,
}

impl DigitalCaptureState {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::TriggeredComplete),
            2 => Some(Self::UntriggeredComplete),
            3 => Some(Self::Stopped),
            4 => Some(Self::Faulted),
            _ => None,
        }
    }
}

/// Capture-wide loss and confidence annotations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct CaptureQualityFlags(pub u32);

impl CaptureQualityFlags {
    /// Acquisition produced more events than its fixed buffer could retain.
    pub const OVERFLOW: u32 = 1 << 0;
    /// Retained pretrigger interval is shorter than requested.
    pub const PRETRIGGER_TRUNCATED: u32 = 1 << 1;
    /// Retained posttrigger interval is shorter than requested.
    pub const POSTTRIGGER_TRUNCATED: u32 = 1 << 2;
    /// Only every declared event stride was retained.
    pub const DECIMATED: u32 = 1 << 3;
    /// Clock frequency/error has not reached the required qualification level.
    pub const CLOCK_UNQUALIFIED: u32 = 1 << 4;
    /// A known gap separates retained portions of the event stream.
    pub const DISCONTINUITY: u32 = 1 << 5;
    /// At least one channel used software sampling.
    pub const SOFTWARE_SAMPLED: u32 = 1 << 6;

    /// Whether one quality bit is present.
    pub const fn contains(self, flag: u32) -> bool {
        self.0 & flag != 0
    }
}

/// Per-transition timestamp annotations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct DigitalTransitionFlags(pub u8);

impl DigitalTransitionFlags {
    /// Transition timestamp has uncertainty beyond the normal clock model.
    pub const TIMESTAMP_UNCERTAIN: u8 = 1 << 0;

    /// Whether one transition bit is present.
    pub const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

/// One retained logical transition relative to the capture start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DigitalTransition {
    /// Offset from `start_cycle` in the declared device clock.
    pub offset_cycles: u64,
    /// Zero-based index into the capture's channel table.
    pub channel_index: u16,
    /// New known logical level.
    pub level: DigitalLevel,
    /// Timestamp annotations.
    pub flags: DigitalTransitionFlags,
}

/// Borrowed digital edge capture supplied to the canonical encoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DigitalCaptureDocument<'a> {
    /// Device, boot, capability, configuration, and clock identity.
    pub context: DiagnosticContext,
    /// Document-wide provenance flags.
    pub flags: DigitalCaptureFlags,
    /// Nonzero capture-attempt identity.
    pub capture_id: CaptureId,
    /// First cycle in the retained window.
    pub start_cycle: DeviceCycle,
    /// First cycle after the retained window.
    pub end_cycle_exclusive: DeviceCycle,
    /// Requested cycles preceding the trigger.
    pub requested_pretrigger_cycles: u64,
    /// Requested cycles following the trigger.
    pub requested_posttrigger_cycles: u64,
    /// Actual trigger cycle, or zero when no trigger fired.
    pub trigger_cycle: DeviceCycle,
    /// Trigger channel index, or `u16::MAX` for immediate/no trigger.
    pub trigger_channel_index: u16,
    /// Trigger predicate.
    pub trigger_condition: DigitalTriggerCondition,
    /// Terminal acquisition state.
    pub state: DigitalCaptureState,
    /// Triggering transition index, or `u32::MAX` for immediate/no trigger.
    pub trigger_transition_index: u32,
    /// Fixed acquisition transition capacity.
    pub transition_capacity: u32,
    /// One for full retention, otherwise the deterministic retained-event stride.
    pub retained_event_stride: u32,
    /// Capture-wide loss and confidence annotations.
    pub quality_flags: CaptureQualityFlags,
    /// Strictly increasing typed resource channel table.
    pub channels: &'a [DigitalCaptureChannel],
    /// Canonically ordered transition table.
    pub transitions: &'a [DigitalTransition],
}

/// Allocation-free validated view of one canonical digital capture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DigitalCaptureView<'a> {
    context: DiagnosticContext,
    flags: DigitalCaptureFlags,
    capture_id: CaptureId,
    start_cycle: DeviceCycle,
    end_cycle_exclusive: DeviceCycle,
    requested_pretrigger_cycles: u64,
    requested_posttrigger_cycles: u64,
    trigger_cycle: DeviceCycle,
    trigger_channel_index: u16,
    trigger_condition: DigitalTriggerCondition,
    state: DigitalCaptureState,
    trigger_transition_index: u32,
    transition_capacity: u32,
    retained_event_stride: u32,
    quality_flags: CaptureQualityFlags,
    channel_records: &'a [u8],
    transition_records: &'a [u8],
    channel_count: usize,
    transition_count: usize,
}

impl<'a> DigitalCaptureView<'a> {
    /// Device, boot, capability, configuration, and clock identity.
    pub const fn context(self) -> DiagnosticContext {
        self.context
    }

    /// Document-wide provenance flags.
    pub const fn flags(self) -> DigitalCaptureFlags {
        self.flags
    }

    /// Nonzero capture-attempt identity.
    pub const fn capture_id(self) -> CaptureId {
        self.capture_id
    }

    /// Retained `[start, end)` cycle interval.
    pub const fn cycle_window(self) -> (DeviceCycle, DeviceCycle) {
        (self.start_cycle, self.end_cycle_exclusive)
    }

    /// Requested pretrigger and posttrigger intervals in device cycles.
    pub const fn requested_window_cycles(self) -> (u64, u64) {
        (
            self.requested_pretrigger_cycles,
            self.requested_posttrigger_cycles,
        )
    }

    /// Actual trigger cycle, channel, predicate, and transition index.
    pub const fn trigger(self) -> (DeviceCycle, u16, DigitalTriggerCondition, u32) {
        (
            self.trigger_cycle,
            self.trigger_channel_index,
            self.trigger_condition,
            self.trigger_transition_index,
        )
    }

    /// Terminal acquisition state.
    pub const fn state(self) -> DigitalCaptureState {
        self.state
    }

    /// Fixed transition capacity and retained-event stride.
    pub const fn retention(self) -> (u32, u32) {
        (self.transition_capacity, self.retained_event_stride)
    }

    /// Capture-wide loss and confidence annotations.
    pub const fn quality_flags(self) -> CaptureQualityFlags {
        self.quality_flags
    }

    /// Number of channels.
    pub const fn channel_count(self) -> usize {
        self.channel_count
    }

    /// Number of retained transitions.
    pub const fn transition_count(self) -> usize {
        self.transition_count
    }

    /// Iterates validated channel records without allocation.
    pub fn channels(self) -> DigitalCaptureChannelIter<'a> {
        DigitalCaptureChannelIter {
            records: self.channel_records,
            offset: 0,
        }
    }

    /// Iterates validated transitions without allocation.
    pub fn transitions(self) -> DigitalTransitionIter<'a> {
        DigitalTransitionIter {
            records: self.transition_records,
            offset: 0,
        }
    }

    /// Returns a channel by its exact capture-local index.
    pub fn channel(self, index: u16) -> Option<DigitalCaptureChannel> {
        self.channels().nth(usize::from(index))
    }
}

/// Iterator over validated digital-capture channels.
#[derive(Clone, Debug)]
pub struct DigitalCaptureChannelIter<'a> {
    records: &'a [u8],
    offset: usize,
}

impl Iterator for DigitalCaptureChannelIter<'_> {
    type Item = DigitalCaptureChannel;

    fn next(&mut self) -> Option<Self::Item> {
        let end = self.offset.checked_add(DIGITAL_CAPTURE_CHANNEL_BYTES)?;
        let record = self.records.get(self.offset..end)?;
        self.offset = end;
        Some(decode_channel(record).expect("validated channel record remains valid"))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.records.len() - self.offset) / DIGITAL_CAPTURE_CHANNEL_BYTES;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for DigitalCaptureChannelIter<'_> {}

/// Iterator over validated digital transitions.
#[derive(Clone, Debug)]
pub struct DigitalTransitionIter<'a> {
    records: &'a [u8],
    offset: usize,
}

impl Iterator for DigitalTransitionIter<'_> {
    type Item = DigitalTransition;

    fn next(&mut self) -> Option<Self::Item> {
        let end = self.offset.checked_add(DIGITAL_TRANSITION_BYTES)?;
        let record = self.records.get(self.offset..end)?;
        self.offset = end;
        Some(decode_transition(record).expect("validated transition record remains valid"))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.records.len() - self.offset) / DIGITAL_TRANSITION_BYTES;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for DigitalTransitionIter<'_> {}

/// Canonical diagnostic encode/decode rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticError {
    /// Complete byte length or caller buffer length is invalid.
    Length,
    /// Magic does not identify the requested record family.
    Magic,
    /// Schema version is unsupported.
    Version,
    /// Unknown document, channel, sample, transition, or quality flags were set.
    Flags,
    /// Reserved bytes were nonzero.
    Reserved,
    /// A caller-selected count or byte ceiling was exceeded.
    Limit(&'static str),
    /// Device, boot, capability, or capture identity is invalid.
    Identity,
    /// Clock frequency or cycle relation is invalid.
    Clock,
    /// Resource identifier is malformed or records are not strictly ordered.
    Resource,
    /// Scalar representation is invalid or noncanonical.
    Value,
    /// Provenance and quality fields contradict each other or document flags.
    Quality,
    /// Capture interval or requested pre/post interval is invalid.
    Window,
    /// Channel table or transition channel index is invalid.
    Channel,
    /// Transition order, level, or timestamp is invalid.
    Transition,
    /// Trigger fields do not identify the declared event and edge.
    Trigger,
}

impl fmt::Display for DiagnosticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "diagnostic record rejected: {self:?}")
    }
}

/// Returns the exact real-time input-snapshot length for `record_count`.
pub fn realtime_input_snapshot_encoded_len(record_count: usize) -> Result<usize, DiagnosticError> {
    if record_count > MAX_REALTIME_INPUT_SNAPSHOT_INPUTS {
        return Err(DiagnosticError::Limit("real-time input records"));
    }
    REALTIME_INPUT_SNAPSHOT_HEADER_BYTES
        .checked_add(
            record_count
                .checked_mul(REALTIME_INPUT_SNAPSHOT_RECORD_BYTES)
                .ok_or(DiagnosticError::Length)?,
        )
        .ok_or(DiagnosticError::Length)
}

/// Encodes one exact core-1 input snapshot into caller-owned memory.
pub fn encode_realtime_input_snapshot(
    document: &RealtimeInputSnapshotDocument<'_>,
    output: &mut [u8],
) -> Result<usize, DiagnosticError> {
    validate_realtime_input_snapshot(document.status, document.records)?;
    let total = realtime_input_snapshot_encoded_len(document.records.len())?;
    if output.len() < total {
        return Err(DiagnosticError::Length);
    }

    let encoded = &mut output[..total];
    encoded.fill(0);
    encoded[0..8].copy_from_slice(&REALTIME_INPUT_SNAPSHOT_MAGIC);
    encoded[8..10].copy_from_slice(&REALTIME_INPUT_SNAPSHOT_VERSION.to_le_bytes());
    encoded[12..16].copy_from_slice(
        &u32::try_from(total)
            .map_err(|_| DiagnosticError::Length)?
            .to_le_bytes(),
    );
    encoded[16] = document.status.input_count;
    encoded[17] = u8::from(document.status.next_watchdog_deadline.is_some());
    encoded[20..24].copy_from_slice(&document.status.known_mask.to_le_bytes());
    encoded[24..28].copy_from_slice(&document.status.active_mask.to_le_bytes());
    encoded[28..32].copy_from_slice(&document.status.required_mask.to_le_bytes());
    encoded[32..36].copy_from_slice(&document.status.stale_mask.to_le_bytes());
    encoded[36..40].copy_from_slice(&document.status.transition_generation.to_le_bytes());
    if let Some(deadline) = document.status.next_watchdog_deadline {
        encoded[40..48].copy_from_slice(&deadline.0.to_le_bytes());
    }
    for (index, record) in document.records.iter().copied().enumerate() {
        let start =
            REALTIME_INPUT_SNAPSHOT_HEADER_BYTES + index * REALTIME_INPUT_SNAPSHOT_RECORD_BYTES;
        encode_realtime_input_record(
            record,
            &mut encoded[start..start + REALTIME_INPUT_SNAPSHOT_RECORD_BYTES],
        );
    }
    Ok(total)
}

/// Decodes and independently validates one complete core-1 input snapshot.
pub fn decode_realtime_input_snapshot(
    encoded: &[u8],
) -> Result<RealtimeInputSnapshotView<'_>, DiagnosticError> {
    if encoded.len() < REALTIME_INPUT_SNAPSHOT_HEADER_BYTES {
        return Err(DiagnosticError::Length);
    }
    if encoded[0..8] != REALTIME_INPUT_SNAPSHOT_MAGIC {
        return Err(DiagnosticError::Magic);
    }
    if read_u16(encoded, 8) != REALTIME_INPUT_SNAPSHOT_VERSION {
        return Err(DiagnosticError::Version);
    }
    if encoded[10..12] != [0; 2] || encoded[18..20] != [0; 2] || encoded[17] & !1 != 0 {
        return Err(DiagnosticError::Reserved);
    }
    if read_u32(encoded, 12) != u32::try_from(encoded.len()).map_err(|_| DiagnosticError::Length)? {
        return Err(DiagnosticError::Length);
    }
    let record_count = usize::from(encoded[16]);
    let expected = realtime_input_snapshot_encoded_len(record_count)?;
    if encoded.len() != expected {
        return Err(DiagnosticError::Length);
    }
    if encoded[17] == 0 && encoded[40..48] != [0; 8] {
        return Err(DiagnosticError::Reserved);
    }
    let status = SafetyInputStatus {
        input_count: encoded[16],
        known_mask: read_u32(encoded, 20),
        active_mask: read_u32(encoded, 24),
        required_mask: read_u32(encoded, 28),
        stale_mask: read_u32(encoded, 32),
        transition_generation: read_u32(encoded, 36),
        next_watchdog_deadline: (encoded[17] != 0).then(|| DeviceCycle(read_u64(encoded, 40))),
    };
    let records = &encoded[REALTIME_INPUT_SNAPSHOT_HEADER_BYTES..];
    validate_encoded_realtime_input_snapshot(status, records)?;
    Ok(RealtimeInputSnapshotView {
        status,
        records,
        record_count,
    })
}

/// Returns the exact resource-overview encoded length for `sample_count`.
pub fn resource_overview_encoded_len(sample_count: usize) -> Result<usize, DiagnosticError> {
    RESOURCE_OVERVIEW_HEADER_BYTES
        .checked_add(
            sample_count
                .checked_mul(RESOURCE_OVERVIEW_SAMPLE_BYTES)
                .ok_or(DiagnosticError::Length)?,
        )
        .ok_or(DiagnosticError::Length)
}

/// Encodes one canonical overview into caller-owned memory and returns bytes used.
pub fn encode_resource_overview(
    document: &ResourceOverviewDocument<'_>,
    output: &mut [u8],
) -> Result<usize, DiagnosticError> {
    validate_context(document.context)?;
    validate_overview_flags(document.flags)?;
    let sample_count =
        u32::try_from(document.samples.len()).map_err(|_| DiagnosticError::Length)?;
    let total = resource_overview_encoded_len(document.samples.len())?;
    if output.len() < total {
        return Err(DiagnosticError::Length);
    }
    validate_overview_samples(document.samples, document.snapshot_cycle, document.flags)?;

    let encoded = &mut output[..total];
    encoded.fill(0);
    encoded[0..8].copy_from_slice(&RESOURCE_OVERVIEW_MAGIC);
    encoded[8..10].copy_from_slice(&RESOURCE_OVERVIEW_VERSION.to_le_bytes());
    encoded[10..12].copy_from_slice(&document.flags.0.to_le_bytes());
    encoded[12..16].copy_from_slice(
        &u32::try_from(total)
            .map_err(|_| DiagnosticError::Length)?
            .to_le_bytes(),
    );
    encode_context(document.context, &mut encoded[16..128]);
    encoded[128..136].copy_from_slice(&document.snapshot_cycle.0.to_le_bytes());
    encoded[136..144].copy_from_slice(&document.sequence.to_le_bytes());
    encoded[144..148].copy_from_slice(&sample_count.to_le_bytes());
    for (index, sample) in document.samples.iter().copied().enumerate() {
        let start = RESOURCE_OVERVIEW_HEADER_BYTES + index * RESOURCE_OVERVIEW_SAMPLE_BYTES;
        encode_overview_sample(
            sample,
            &mut encoded[start..start + RESOURCE_OVERVIEW_SAMPLE_BYTES],
        );
    }
    Ok(total)
}

/// Decodes and independently validates one complete canonical resource overview.
pub fn decode_resource_overview(
    encoded: &[u8],
    limits: DiagnosticLimits,
) -> Result<ResourceOverviewView<'_>, DiagnosticError> {
    if encoded.len() < RESOURCE_OVERVIEW_HEADER_BYTES {
        return Err(DiagnosticError::Length);
    }
    if encoded[0..8] != RESOURCE_OVERVIEW_MAGIC {
        return Err(DiagnosticError::Magic);
    }
    if read_u16(encoded, 8) != RESOURCE_OVERVIEW_VERSION {
        return Err(DiagnosticError::Version);
    }
    if read_u32(encoded, 12) != u32::try_from(encoded.len()).map_err(|_| DiagnosticError::Length)? {
        return Err(DiagnosticError::Length);
    }
    if u32::try_from(encoded.len()).map_err(|_| DiagnosticError::Length)?
        > limits.maximum_overview_bytes
    {
        return Err(DiagnosticError::Limit("overview bytes"));
    }
    if encoded[148..160].iter().any(|byte| *byte != 0) {
        return Err(DiagnosticError::Reserved);
    }
    let flags = OverviewFlags(read_u16(encoded, 10));
    validate_overview_flags(flags)?;
    let context = decode_context(&encoded[16..128])?;
    let snapshot_cycle = DeviceCycle(read_u64(encoded, 128));
    let sequence = read_u64(encoded, 136);
    let sample_count = read_u32(encoded, 144);
    if sample_count > limits.maximum_overview_samples {
        return Err(DiagnosticError::Limit("overview samples"));
    }
    let sample_count = usize::try_from(sample_count).map_err(|_| DiagnosticError::Length)?;
    let expected = resource_overview_encoded_len(sample_count)?;
    if encoded.len() != expected {
        return Err(DiagnosticError::Length);
    }
    let records = &encoded[RESOURCE_OVERVIEW_HEADER_BYTES..];
    validate_encoded_overview_samples(records, snapshot_cycle, flags)?;
    Ok(ResourceOverviewView {
        context,
        flags,
        snapshot_cycle,
        sequence,
        records,
        sample_count,
    })
}

/// Returns the exact digital-capture encoded length for the two table counts.
pub fn digital_capture_encoded_len(
    channel_count: usize,
    transition_count: usize,
) -> Result<usize, DiagnosticError> {
    DIGITAL_CAPTURE_HEADER_BYTES
        .checked_add(
            channel_count
                .checked_mul(DIGITAL_CAPTURE_CHANNEL_BYTES)
                .ok_or(DiagnosticError::Length)?,
        )
        .and_then(|length| {
            transition_count
                .checked_mul(DIGITAL_TRANSITION_BYTES)
                .and_then(|transitions| length.checked_add(transitions))
        })
        .ok_or(DiagnosticError::Length)
}

/// Encodes one canonical digital capture into caller-owned memory.
pub fn encode_digital_capture(
    document: &DigitalCaptureDocument<'_>,
    output: &mut [u8],
) -> Result<usize, DiagnosticError> {
    validate_context(document.context)?;
    validate_capture_flags(document.flags)?;
    let channel_count =
        u16::try_from(document.channels.len()).map_err(|_| DiagnosticError::Length)?;
    let transition_count =
        u32::try_from(document.transitions.len()).map_err(|_| DiagnosticError::Length)?;
    let total = digital_capture_encoded_len(document.channels.len(), document.transitions.len())?;
    if output.len() < total {
        return Err(DiagnosticError::Length);
    }
    validate_capture(document)?;

    let encoded = &mut output[..total];
    encoded.fill(0);
    encoded[0..8].copy_from_slice(&DIGITAL_CAPTURE_MAGIC);
    encoded[8..10].copy_from_slice(&DIGITAL_CAPTURE_VERSION.to_le_bytes());
    encoded[10..12].copy_from_slice(&document.flags.0.to_le_bytes());
    encoded[12..16].copy_from_slice(
        &u32::try_from(total)
            .map_err(|_| DiagnosticError::Length)?
            .to_le_bytes(),
    );
    encode_context(document.context, &mut encoded[16..128]);
    encoded[128..144].copy_from_slice(&document.capture_id.as_bytes());
    encoded[144..152].copy_from_slice(&document.start_cycle.0.to_le_bytes());
    encoded[152..160].copy_from_slice(&document.end_cycle_exclusive.0.to_le_bytes());
    encoded[160..168].copy_from_slice(&document.requested_pretrigger_cycles.to_le_bytes());
    encoded[168..176].copy_from_slice(&document.requested_posttrigger_cycles.to_le_bytes());
    encoded[176..184].copy_from_slice(&document.trigger_cycle.0.to_le_bytes());
    encoded[184..186].copy_from_slice(&document.trigger_channel_index.to_le_bytes());
    encoded[186] = document.trigger_condition as u8;
    encoded[187] = document.state as u8;
    encoded[188..192].copy_from_slice(&document.trigger_transition_index.to_le_bytes());
    encoded[192..194].copy_from_slice(&channel_count.to_le_bytes());
    encoded[196..200].copy_from_slice(&transition_count.to_le_bytes());
    encoded[200..204].copy_from_slice(&document.transition_capacity.to_le_bytes());
    encoded[204..208].copy_from_slice(&document.retained_event_stride.to_le_bytes());
    encoded[208..212].copy_from_slice(&document.quality_flags.0.to_le_bytes());
    for (index, channel) in document.channels.iter().copied().enumerate() {
        let start = DIGITAL_CAPTURE_HEADER_BYTES + index * DIGITAL_CAPTURE_CHANNEL_BYTES;
        encode_channel(
            channel,
            &mut encoded[start..start + DIGITAL_CAPTURE_CHANNEL_BYTES],
        );
    }
    let transitions_start =
        DIGITAL_CAPTURE_HEADER_BYTES + document.channels.len() * DIGITAL_CAPTURE_CHANNEL_BYTES;
    for (index, transition) in document.transitions.iter().copied().enumerate() {
        let start = transitions_start + index * DIGITAL_TRANSITION_BYTES;
        encode_transition(
            transition,
            &mut encoded[start..start + DIGITAL_TRANSITION_BYTES],
        );
    }
    Ok(total)
}

/// Decodes and independently validates one complete canonical digital capture.
pub fn decode_digital_capture(
    encoded: &[u8],
    limits: DiagnosticLimits,
) -> Result<DigitalCaptureView<'_>, DiagnosticError> {
    if encoded.len() < DIGITAL_CAPTURE_HEADER_BYTES {
        return Err(DiagnosticError::Length);
    }
    if encoded[0..8] != DIGITAL_CAPTURE_MAGIC {
        return Err(DiagnosticError::Magic);
    }
    if read_u16(encoded, 8) != DIGITAL_CAPTURE_VERSION {
        return Err(DiagnosticError::Version);
    }
    if read_u32(encoded, 12) != u32::try_from(encoded.len()).map_err(|_| DiagnosticError::Length)? {
        return Err(DiagnosticError::Length);
    }
    if u32::try_from(encoded.len()).map_err(|_| DiagnosticError::Length)?
        > limits.maximum_capture_bytes
    {
        return Err(DiagnosticError::Limit("capture bytes"));
    }
    if encoded[194..196].iter().any(|byte| *byte != 0)
        || encoded[212..224].iter().any(|byte| *byte != 0)
    {
        return Err(DiagnosticError::Reserved);
    }
    let flags = DigitalCaptureFlags(read_u16(encoded, 10));
    validate_capture_flags(flags)?;
    let context = decode_context(&encoded[16..128])?;
    let mut capture_id = [0_u8; 16];
    capture_id.copy_from_slice(&encoded[128..144]);
    let capture_id = CaptureId::new(capture_id)?;
    let start_cycle = DeviceCycle(read_u64(encoded, 144));
    let end_cycle_exclusive = DeviceCycle(read_u64(encoded, 152));
    let requested_pretrigger_cycles = read_u64(encoded, 160);
    let requested_posttrigger_cycles = read_u64(encoded, 168);
    let trigger_cycle = DeviceCycle(read_u64(encoded, 176));
    let trigger_channel_index = read_u16(encoded, 184);
    let trigger_condition =
        DigitalTriggerCondition::from_wire(encoded[186]).ok_or(DiagnosticError::Trigger)?;
    let state = DigitalCaptureState::from_wire(encoded[187]).ok_or(DiagnosticError::Window)?;
    let trigger_transition_index = read_u32(encoded, 188);
    let channel_count = read_u16(encoded, 192);
    if channel_count > limits.maximum_capture_channels
        || usize::from(channel_count) > MAX_DIGITAL_CAPTURE_CHANNELS
    {
        return Err(DiagnosticError::Limit("capture channels"));
    }
    let transition_count = read_u32(encoded, 196);
    if transition_count > limits.maximum_capture_transitions {
        return Err(DiagnosticError::Limit("capture transitions"));
    }
    let channel_count = usize::from(channel_count);
    let transition_count =
        usize::try_from(transition_count).map_err(|_| DiagnosticError::Length)?;
    let expected = digital_capture_encoded_len(channel_count, transition_count)?;
    if encoded.len() != expected {
        return Err(DiagnosticError::Length);
    }
    let channels_end = DIGITAL_CAPTURE_HEADER_BYTES
        .checked_add(
            channel_count
                .checked_mul(DIGITAL_CAPTURE_CHANNEL_BYTES)
                .ok_or(DiagnosticError::Length)?,
        )
        .ok_or(DiagnosticError::Length)?;
    let channel_records = &encoded[DIGITAL_CAPTURE_HEADER_BYTES..channels_end];
    let transition_records = &encoded[channels_end..];
    let view = DigitalCaptureView {
        context,
        flags,
        capture_id,
        start_cycle,
        end_cycle_exclusive,
        requested_pretrigger_cycles,
        requested_posttrigger_cycles,
        trigger_cycle,
        trigger_channel_index,
        trigger_condition,
        state,
        trigger_transition_index,
        transition_capacity: read_u32(encoded, 200),
        retained_event_stride: read_u32(encoded, 204),
        quality_flags: CaptureQualityFlags(read_u32(encoded, 208)),
        channel_records,
        transition_records,
        channel_count,
        transition_count,
    };
    validate_capture_view(view)?;
    Ok(view)
}

fn validate_context(context: DiagnosticContext) -> Result<(), DiagnosticError> {
    if context.device_id.is_zero()
        || context.capability.byte_len == 0
        || context.capability.digest.is_zero()
    {
        return Err(DiagnosticError::Identity);
    }
    if context.clock_frequency_hz == 0 {
        return Err(DiagnosticError::Clock);
    }
    Ok(())
}

fn encode_context(context: DiagnosticContext, output: &mut [u8]) {
    output.fill(0);
    output[0..16].copy_from_slice(&context.device_id.0);
    output[16..32].copy_from_slice(&context.boot_id.as_bytes());
    output[32..36].copy_from_slice(&context.capability.byte_len.to_le_bytes());
    output[40..72].copy_from_slice(&context.capability.digest.0);
    output[72..104].copy_from_slice(&context.config_digest.0);
    output[104..112].copy_from_slice(&context.clock_frequency_hz.to_le_bytes());
}

fn decode_context(encoded: &[u8]) -> Result<DiagnosticContext, DiagnosticError> {
    if encoded.len() != 112 {
        return Err(DiagnosticError::Length);
    }
    if encoded[36..40].iter().any(|byte| *byte != 0) {
        return Err(DiagnosticError::Reserved);
    }
    let mut device_id = [0_u8; 16];
    device_id.copy_from_slice(&encoded[0..16]);
    let mut boot_id = [0_u8; BOOT_ID_BYTES];
    boot_id.copy_from_slice(&encoded[16..32]);
    let mut capability_digest = [0_u8; 32];
    capability_digest.copy_from_slice(&encoded[40..72]);
    let mut config_digest = [0_u8; 32];
    config_digest.copy_from_slice(&encoded[72..104]);
    let context = DiagnosticContext {
        device_id: DeviceId(device_id),
        boot_id: BootId::new(boot_id).map_err(|_| DiagnosticError::Identity)?,
        capability: CapabilityIdentity {
            byte_len: read_u32(encoded, 32),
            digest: Digest(capability_digest),
        },
        config_digest: Digest(config_digest),
        clock_frequency_hz: read_u64(encoded, 104),
    };
    validate_context(context)?;
    Ok(context)
}

fn validate_overview_flags(flags: OverviewFlags) -> Result<(), DiagnosticError> {
    if flags.0 & !OVERVIEW_FLAG_SIMULATED != 0 {
        Err(DiagnosticError::Flags)
    } else {
        Ok(())
    }
}

fn validate_realtime_input_snapshot(
    status: SafetyInputStatus,
    records: &[RealtimeInputRecord],
) -> Result<(), DiagnosticError> {
    status.validate().map_err(|_| DiagnosticError::Quality)?;
    if usize::from(status.input_count) != records.len()
        || records.len() > MAX_REALTIME_INPUT_SNAPSHOT_INPUTS
    {
        return Err(DiagnosticError::Length);
    }
    for (slot, record) in records.iter().copied().enumerate() {
        if records[..slot]
            .iter()
            .any(|previous| previous.resource == record.resource)
        {
            return Err(DiagnosticError::Resource);
        }
        validate_realtime_input_record_state(status, slot, record)?;
    }
    Ok(())
}

fn validate_encoded_realtime_input_snapshot(
    status: SafetyInputStatus,
    records: &[u8],
) -> Result<(), DiagnosticError> {
    status.validate().map_err(|_| DiagnosticError::Quality)?;
    if !records
        .len()
        .is_multiple_of(REALTIME_INPUT_SNAPSHOT_RECORD_BYTES)
        || records.len() / REALTIME_INPUT_SNAPSHOT_RECORD_BYTES != usize::from(status.input_count)
    {
        return Err(DiagnosticError::Length);
    }

    for (slot, encoded) in records
        .chunks_exact(REALTIME_INPUT_SNAPSHOT_RECORD_BYTES)
        .enumerate()
    {
        let record = decode_realtime_input_record(encoded)?;
        for previous in records[..slot * REALTIME_INPUT_SNAPSHOT_RECORD_BYTES]
            .chunks_exact(REALTIME_INPUT_SNAPSHOT_RECORD_BYTES)
        {
            if decode_realtime_input_record(previous)?.resource == record.resource {
                return Err(DiagnosticError::Resource);
            }
        }
        validate_realtime_input_record_state(status, slot, record)?;
    }
    Ok(())
}

fn validate_realtime_input_record_state(
    status: SafetyInputStatus,
    slot: usize,
    record: RealtimeInputRecord,
) -> Result<(), DiagnosticError> {
    let bit = 1_u32 << slot;
    if (status.known_mask & bit != 0 || status.stale_mask & bit == 0) && record.sampled_at.is_none()
    {
        return Err(DiagnosticError::Quality);
    }
    Ok(())
}

fn encode_realtime_input_record(record: RealtimeInputRecord, output: &mut [u8]) {
    output.fill(0);
    output[0..4].copy_from_slice(&encode_resource_id(record.resource));
    output[4] = u8::from(record.sampled_at.is_some());
    if let Some(sampled_at) = record.sampled_at {
        output[8..16].copy_from_slice(&sampled_at.0.to_le_bytes());
    }
}

fn decode_realtime_input_record(encoded: &[u8]) -> Result<RealtimeInputRecord, DiagnosticError> {
    if encoded.len() != REALTIME_INPUT_SNAPSHOT_RECORD_BYTES {
        return Err(DiagnosticError::Length);
    }
    if encoded[4] & !1 != 0
        || encoded[5..8].iter().any(|byte| *byte != 0)
        || encoded[4] == 0 && encoded[8..16].iter().any(|byte| *byte != 0)
    {
        return Err(DiagnosticError::Reserved);
    }
    Ok(RealtimeInputRecord {
        resource: decode_resource_id(&encoded[0..4]).map_err(|_| DiagnosticError::Resource)?,
        sampled_at: (encoded[4] != 0).then(|| DeviceCycle(read_u64(encoded, 8))),
    })
}

fn validate_overview_samples(
    samples: &[ResourceOverviewSample],
    snapshot_cycle: DeviceCycle,
    flags: OverviewFlags,
) -> Result<(), DiagnosticError> {
    let mut previous = None;
    for sample in samples {
        if previous.is_some_and(|resource| resource >= sample.resource) {
            return Err(DiagnosticError::Resource);
        }
        validate_overview_sample(*sample, snapshot_cycle, flags)?;
        previous = Some(sample.resource);
    }
    Ok(())
}

fn validate_encoded_overview_samples(
    records: &[u8],
    snapshot_cycle: DeviceCycle,
    flags: OverviewFlags,
) -> Result<(), DiagnosticError> {
    let mut previous = None;
    for record in records.chunks_exact(RESOURCE_OVERVIEW_SAMPLE_BYTES) {
        let sample = decode_overview_sample(record)?;
        if previous.is_some_and(|resource| resource >= sample.resource) {
            return Err(DiagnosticError::Resource);
        }
        validate_overview_sample(sample, snapshot_cycle, flags)?;
        previous = Some(sample.resource);
    }
    Ok(())
}

fn validate_overview_sample(
    sample: ResourceOverviewSample,
    snapshot_cycle: DeviceCycle,
    flags: OverviewFlags,
) -> Result<(), DiagnosticError> {
    if sample.quality_flags.0 & !SAMPLE_QUALITY_KNOWN != 0 {
        return Err(DiagnosticError::Flags);
    }
    if sample.captured_cycle > snapshot_cycle {
        return Err(DiagnosticError::Clock);
    }
    let simulated = flags.contains(OverviewFlags::SIMULATED);
    if (sample.provenance == SampleProvenance::Simulated) != simulated {
        return Err(DiagnosticError::Quality);
    }
    if (sample.quality == SampleQuality::Unavailable)
        != matches!(sample.value, ResourceValue::Unavailable)
    {
        return Err(DiagnosticError::Quality);
    }
    if sample
        .quality_flags
        .contains(SampleQualityFlags::FAULT_LATCHED)
        && sample.quality != SampleQuality::Faulted
    {
        return Err(DiagnosticError::Quality);
    }
    validate_resource_value(sample.value)
}

fn validate_resource_value(value: ResourceValue) -> Result<(), DiagnosticError> {
    if let ResourceValue::ExactRatio {
        numerator,
        denominator,
    } = value
        && (denominator == 0
            || numerator == 0 && denominator != 1
            || gcd(numerator.unsigned_abs(), denominator) != 1)
    {
        return Err(DiagnosticError::Value);
    }
    Ok(())
}

fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn encode_overview_sample(sample: ResourceOverviewSample, output: &mut [u8]) {
    output.fill(0);
    output[0..4].copy_from_slice(&encode_resource_id(sample.resource));
    output[4] = sample.provenance as u8;
    output[5] = sample.quality as u8;
    output[6..8].copy_from_slice(&sample.quality_flags.0.to_le_bytes());
    output[8..16].copy_from_slice(&sample.captured_cycle.0.to_le_bytes());
    match sample.value {
        ResourceValue::Unavailable => {}
        ResourceValue::Boolean(value) => {
            output[16] = 1;
            output[24] = u8::from(value);
        }
        ResourceValue::Unsigned(value) => {
            output[16] = 2;
            output[24..32].copy_from_slice(&value.to_le_bytes());
        }
        ResourceValue::Signed(value) => {
            output[16] = 3;
            output[24..32].copy_from_slice(&value.to_le_bytes());
        }
        ResourceValue::ExactRatio {
            numerator,
            denominator,
        } => {
            output[16] = 4;
            output[24..32].copy_from_slice(&numerator.to_le_bytes());
            output[32..40].copy_from_slice(&denominator.to_le_bytes());
        }
    }
}

fn decode_overview_sample(encoded: &[u8]) -> Result<ResourceOverviewSample, DiagnosticError> {
    if encoded.len() != RESOURCE_OVERVIEW_SAMPLE_BYTES {
        return Err(DiagnosticError::Length);
    }
    let resource = decode_resource_id(&encoded[0..4]).map_err(|_| DiagnosticError::Resource)?;
    let provenance = SampleProvenance::from_wire(encoded[4]).ok_or(DiagnosticError::Quality)?;
    let quality = SampleQuality::from_wire(encoded[5]).ok_or(DiagnosticError::Quality)?;
    let quality_flags = SampleQualityFlags(read_u16(encoded, 6));
    if encoded[17..24].iter().any(|byte| *byte != 0) {
        return Err(DiagnosticError::Reserved);
    }
    let value = match encoded[16] {
        0 if encoded[24..40].iter().all(|byte| *byte == 0) => ResourceValue::Unavailable,
        1 if encoded[25..40].iter().all(|byte| *byte == 0) && encoded[24] <= 1 => {
            ResourceValue::Boolean(encoded[24] == 1)
        }
        2 if encoded[32..40].iter().all(|byte| *byte == 0) => {
            ResourceValue::Unsigned(read_u64(encoded, 24))
        }
        3 if encoded[32..40].iter().all(|byte| *byte == 0) => {
            ResourceValue::Signed(read_i64(encoded, 24))
        }
        4 => ResourceValue::ExactRatio {
            numerator: read_i64(encoded, 24),
            denominator: read_u64(encoded, 32),
        },
        _ => return Err(DiagnosticError::Value),
    };
    validate_resource_value(value)?;
    Ok(ResourceOverviewSample {
        resource,
        provenance,
        quality,
        quality_flags,
        captured_cycle: DeviceCycle(read_u64(encoded, 8)),
        value,
    })
}

fn validate_capture_flags(flags: DigitalCaptureFlags) -> Result<(), DiagnosticError> {
    if flags.0 & !CAPTURE_FLAG_SIMULATED != 0 {
        Err(DiagnosticError::Flags)
    } else {
        Ok(())
    }
}

fn validate_capture(document: &DigitalCaptureDocument<'_>) -> Result<(), DiagnosticError> {
    validate_capture_parts(
        document.flags,
        document.start_cycle,
        document.end_cycle_exclusive,
        document.requested_pretrigger_cycles,
        document.requested_posttrigger_cycles,
        document.trigger_cycle,
        document.trigger_channel_index,
        document.trigger_condition,
        document.state,
        document.trigger_transition_index,
        document.transition_capacity,
        document.retained_event_stride,
        document.quality_flags,
        document.channels.iter().copied(),
        document.transitions.iter().copied(),
        document.channels.len(),
        document.transitions.len(),
    )
}

fn validate_capture_view(view: DigitalCaptureView<'_>) -> Result<(), DiagnosticError> {
    validate_capture_parts(
        view.flags,
        view.start_cycle,
        view.end_cycle_exclusive,
        view.requested_pretrigger_cycles,
        view.requested_posttrigger_cycles,
        view.trigger_cycle,
        view.trigger_channel_index,
        view.trigger_condition,
        view.state,
        view.trigger_transition_index,
        view.transition_capacity,
        view.retained_event_stride,
        view.quality_flags,
        view.channels(),
        view.transitions(),
        view.channel_count,
        view.transition_count,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "one linear audit surface mirrors every canonical capture-header field"
)]
fn validate_capture_parts<C, T>(
    flags: DigitalCaptureFlags,
    start_cycle: DeviceCycle,
    end_cycle_exclusive: DeviceCycle,
    requested_pretrigger_cycles: u64,
    requested_posttrigger_cycles: u64,
    trigger_cycle: DeviceCycle,
    trigger_channel_index: u16,
    trigger_condition: DigitalTriggerCondition,
    state: DigitalCaptureState,
    trigger_transition_index: u32,
    transition_capacity: u32,
    retained_event_stride: u32,
    quality_flags: CaptureQualityFlags,
    channels: C,
    transitions: T,
    channel_count: usize,
    transition_count: usize,
) -> Result<(), DiagnosticError>
where
    C: Clone + Iterator<Item = DigitalCaptureChannel>,
    T: Clone + Iterator<Item = DigitalTransition>,
{
    if flags.0 & !CAPTURE_FLAG_SIMULATED != 0 || quality_flags.0 & !CAPTURE_QUALITY_KNOWN != 0 {
        return Err(DiagnosticError::Flags);
    }
    if channel_count == 0 || channel_count > MAX_DIGITAL_CAPTURE_CHANNELS {
        return Err(DiagnosticError::Channel);
    }
    if end_cycle_exclusive <= start_cycle {
        return Err(DiagnosticError::Window);
    }
    if transition_capacity < u32::try_from(transition_count).map_err(|_| DiagnosticError::Length)?
        || retained_event_stride == 0
        || (retained_event_stride > 1) != quality_flags.contains(CaptureQualityFlags::DECIMATED)
    {
        return Err(DiagnosticError::Quality);
    }

    let simulated = flags.contains(DigitalCaptureFlags::SIMULATED);
    let mut previous_resource = None;
    let mut any_software = false;
    for channel in channels.clone() {
        if channel.flags.0 & !CHANNEL_FLAGS_KNOWN != 0
            || previous_resource.is_some_and(|resource| resource >= channel.resource)
        {
            return Err(DiagnosticError::Channel);
        }
        if (channel.source == DigitalAcquisitionSource::Simulated) != simulated {
            return Err(DiagnosticError::Quality);
        }
        any_software |= channel.source == DigitalAcquisitionSource::Software;
        previous_resource = Some(channel.resource);
    }
    if any_software != quality_flags.contains(CaptureQualityFlags::SOFTWARE_SAMPLED) {
        return Err(DiagnosticError::Quality);
    }

    let duration = end_cycle_exclusive
        .0
        .checked_sub(start_cycle.0)
        .ok_or(DiagnosticError::Window)?;
    let mut levels = [DigitalLevel::Unknown; 256];
    for (index, channel) in channels.enumerate() {
        levels[index] = channel.initial_level;
    }
    let mut previous_key = None;
    let mut trigger_before = DigitalLevel::Unknown;
    let mut trigger_transition = None;
    for (index, transition) in transitions.enumerate() {
        if transition.flags.0 & !TRANSITION_FLAGS_KNOWN != 0
            || transition.level == DigitalLevel::Unknown
            || usize::from(transition.channel_index) >= channel_count
            || transition.offset_cycles >= duration
        {
            return Err(DiagnosticError::Transition);
        }
        let key = (transition.offset_cycles, transition.channel_index);
        if previous_key.is_some_and(|previous| previous >= key) {
            return Err(DiagnosticError::Transition);
        }
        let level = &mut levels[usize::from(transition.channel_index)];
        let before = *level;
        if before == transition.level {
            return Err(DiagnosticError::Transition);
        }
        *level = transition.level;
        if u32::try_from(index).ok() == Some(trigger_transition_index) {
            trigger_before = before;
            trigger_transition = Some(transition);
        }
        previous_key = Some(key);
    }

    match state {
        DigitalCaptureState::TriggeredComplete => match trigger_condition {
            DigitalTriggerCondition::Immediate => {
                if trigger_cycle != start_cycle
                    || trigger_channel_index != NO_TRIGGER_CHANNEL
                    || trigger_transition_index != NO_TRIGGER_TRANSITION
                    || requested_pretrigger_cycles != 0
                    || quality_flags.contains(CaptureQualityFlags::PRETRIGGER_TRUNCATED)
                {
                    return Err(DiagnosticError::Trigger);
                }
                let actual_post = end_cycle_exclusive
                    .0
                    .checked_sub(trigger_cycle.0)
                    .ok_or(DiagnosticError::Window)?;
                validate_retained_interval(
                    actual_post,
                    requested_posttrigger_cycles,
                    quality_flags.contains(CaptureQualityFlags::POSTTRIGGER_TRUNCATED),
                )?;
            }
            DigitalTriggerCondition::Rising
            | DigitalTriggerCondition::Falling
            | DigitalTriggerCondition::Either => {
                let transition = trigger_transition.ok_or(DiagnosticError::Trigger)?;
                let exact_trigger_cycle = start_cycle
                    .0
                    .checked_add(transition.offset_cycles)
                    .ok_or(DiagnosticError::Window)?;
                if transition.channel_index != trigger_channel_index
                    || trigger_cycle.0 != exact_trigger_cycle
                    || !edge_matches(trigger_condition, trigger_before, transition.level)
                {
                    return Err(DiagnosticError::Trigger);
                }
                let actual_pre = trigger_cycle
                    .0
                    .checked_sub(start_cycle.0)
                    .ok_or(DiagnosticError::Window)?;
                let actual_post = end_cycle_exclusive
                    .0
                    .checked_sub(trigger_cycle.0)
                    .ok_or(DiagnosticError::Window)?;
                validate_retained_interval(
                    actual_pre,
                    requested_pretrigger_cycles,
                    quality_flags.contains(CaptureQualityFlags::PRETRIGGER_TRUNCATED),
                )?;
                validate_retained_interval(
                    actual_post,
                    requested_posttrigger_cycles,
                    quality_flags.contains(CaptureQualityFlags::POSTTRIGGER_TRUNCATED),
                )?;
            }
        },
        DigitalCaptureState::UntriggeredComplete
        | DigitalCaptureState::Stopped
        | DigitalCaptureState::Faulted => {
            if trigger_condition == DigitalTriggerCondition::Immediate
                || trigger_cycle.0 != 0
                || trigger_channel_index != NO_TRIGGER_CHANNEL
                || trigger_transition_index != NO_TRIGGER_TRANSITION
            {
                return Err(DiagnosticError::Trigger);
            }
        }
    }
    Ok(())
}

fn validate_retained_interval(
    actual: u64,
    requested: u64,
    truncated: bool,
) -> Result<(), DiagnosticError> {
    if actual > requested || (actual < requested) != truncated {
        Err(DiagnosticError::Window)
    } else {
        Ok(())
    }
}

const fn edge_matches(
    condition: DigitalTriggerCondition,
    before: DigitalLevel,
    after: DigitalLevel,
) -> bool {
    match condition {
        DigitalTriggerCondition::Immediate => false,
        DigitalTriggerCondition::Rising => {
            matches!((before, after), (DigitalLevel::Low, DigitalLevel::High))
        }
        DigitalTriggerCondition::Falling => {
            matches!((before, after), (DigitalLevel::High, DigitalLevel::Low))
        }
        DigitalTriggerCondition::Either => matches!(
            (before, after),
            (DigitalLevel::Low, DigitalLevel::High) | (DigitalLevel::High, DigitalLevel::Low)
        ),
    }
}

fn encode_channel(channel: DigitalCaptureChannel, output: &mut [u8]) {
    output.fill(0);
    output[0..4].copy_from_slice(&encode_resource_id(channel.resource));
    output[4] = channel.initial_level as u8;
    output[5] = channel.source as u8;
    output[6..8].copy_from_slice(&channel.flags.0.to_le_bytes());
}

fn decode_channel(encoded: &[u8]) -> Result<DigitalCaptureChannel, DiagnosticError> {
    if encoded.len() != DIGITAL_CAPTURE_CHANNEL_BYTES {
        return Err(DiagnosticError::Length);
    }
    if encoded[8..16].iter().any(|byte| *byte != 0) {
        return Err(DiagnosticError::Reserved);
    }
    let channel = DigitalCaptureChannel {
        resource: decode_resource_id(&encoded[0..4]).map_err(|_| DiagnosticError::Resource)?,
        initial_level: DigitalLevel::from_wire(encoded[4]).ok_or(DiagnosticError::Channel)?,
        source: DigitalAcquisitionSource::from_wire(encoded[5]).ok_or(DiagnosticError::Channel)?,
        flags: DigitalChannelFlags(read_u16(encoded, 6)),
    };
    if channel.flags.0 & !CHANNEL_FLAGS_KNOWN != 0 {
        return Err(DiagnosticError::Flags);
    }
    Ok(channel)
}

fn encode_transition(transition: DigitalTransition, output: &mut [u8]) {
    output.fill(0);
    output[0..8].copy_from_slice(&transition.offset_cycles.to_le_bytes());
    output[8..10].copy_from_slice(&transition.channel_index.to_le_bytes());
    output[10] = transition.level as u8;
    output[11] = transition.flags.0;
}

fn decode_transition(encoded: &[u8]) -> Result<DigitalTransition, DiagnosticError> {
    if encoded.len() != DIGITAL_TRANSITION_BYTES {
        return Err(DiagnosticError::Length);
    }
    if encoded[12..16].iter().any(|byte| *byte != 0) {
        return Err(DiagnosticError::Reserved);
    }
    let transition = DigitalTransition {
        offset_cycles: read_u64(encoded, 0),
        channel_index: read_u16(encoded, 8),
        level: DigitalLevel::from_wire(encoded[10]).ok_or(DiagnosticError::Transition)?,
        flags: DigitalTransitionFlags(encoded[11]),
    };
    if transition.flags.0 & !TRANSITION_FLAGS_KNOWN != 0 {
        return Err(DiagnosticError::Flags);
    }
    Ok(transition)
}

const fn read_u16(encoded: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([encoded[offset], encoded[offset + 1]])
}

const fn read_u32(encoded: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        encoded[offset],
        encoded[offset + 1],
        encoded[offset + 2],
        encoded[offset + 3],
    ])
}

const fn read_u64(encoded: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        encoded[offset],
        encoded[offset + 1],
        encoded[offset + 2],
        encoded[offset + 3],
        encoded[offset + 4],
        encoded[offset + 5],
        encoded[offset + 6],
        encoded[offset + 7],
    ])
}

const fn read_i64(encoded: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes([
        encoded[offset],
        encoded[offset + 1],
        encoded[offset + 2],
        encoded[offset + 3],
        encoded[offset + 4],
        encoded[offset + 5],
        encoded[offset + 6],
        encoded[offset + 7],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> DiagnosticContext {
        DiagnosticContext {
            device_id: DeviceId(*b"ALUM-SIM:TINYBEE"),
            boot_id: BootId::new([0x53; BOOT_ID_BYTES]).unwrap(),
            capability: CapabilityIdentity {
                byte_len: 3_435,
                digest: Digest([0x71; 32]),
            },
            config_digest: Digest([0x72; 32]),
            clock_frequency_hz: 1_000_000,
        }
    }

    fn samples() -> [ResourceOverviewSample; 4] {
        [22, 32, 33, 35].map(|gpio| ResourceOverviewSample {
            resource: ResourceId::Gpio(gpio),
            provenance: SampleProvenance::Simulated,
            quality: SampleQuality::Valid,
            quality_flags: SampleQualityFlags(SampleQualityFlags::DEBOUNCED),
            captured_cycle: DeviceCycle(990 + u64::from(gpio)),
            value: ResourceValue::Boolean(gpio == 32 || gpio == 35),
        })
    }

    fn channels() -> [DigitalCaptureChannel; 4] {
        [22, 32, 33, 35].map(|gpio| DigitalCaptureChannel {
            resource: ResourceId::Gpio(gpio),
            initial_level: if gpio == 32 || gpio == 35 {
                DigitalLevel::High
            } else {
                DigitalLevel::Low
            },
            source: DigitalAcquisitionSource::Simulated,
            flags: DigitalChannelFlags(DigitalChannelFlags::DEBOUNCED),
        })
    }

    fn transitions() -> [DigitalTransition; 6] {
        [
            DigitalTransition {
                offset_cycles: 100,
                channel_index: 0,
                level: DigitalLevel::High,
                flags: DigitalTransitionFlags(0),
            },
            DigitalTransition {
                offset_cycles: 200,
                channel_index: 0,
                level: DigitalLevel::Low,
                flags: DigitalTransitionFlags(0),
            },
            DigitalTransition {
                offset_cycles: 300,
                channel_index: 1,
                level: DigitalLevel::Low,
                flags: DigitalTransitionFlags(0),
            },
            DigitalTransition {
                offset_cycles: 400,
                channel_index: 1,
                level: DigitalLevel::High,
                flags: DigitalTransitionFlags(0),
            },
            DigitalTransition {
                offset_cycles: 500,
                channel_index: 2,
                level: DigitalLevel::High,
                flags: DigitalTransitionFlags(0),
            },
            DigitalTransition {
                offset_cycles: 600,
                channel_index: 2,
                level: DigitalLevel::Low,
                flags: DigitalTransitionFlags(0),
            },
        ]
    }

    fn capture<'a>(
        channels: &'a [DigitalCaptureChannel],
        transitions: &'a [DigitalTransition],
    ) -> DigitalCaptureDocument<'a> {
        DigitalCaptureDocument {
            context: context(),
            flags: DigitalCaptureFlags(DigitalCaptureFlags::SIMULATED),
            capture_id: CaptureId::new(*b"TINYBEE-SIM-0001").unwrap(),
            start_cycle: DeviceCycle(2_000),
            end_cycle_exclusive: DeviceCycle(4_000),
            requested_pretrigger_cycles: 500,
            requested_posttrigger_cycles: 1_500,
            trigger_cycle: DeviceCycle(2_500),
            trigger_channel_index: 2,
            trigger_condition: DigitalTriggerCondition::Rising,
            state: DigitalCaptureState::TriggeredComplete,
            trigger_transition_index: 4,
            transition_capacity: 64,
            retained_event_stride: 1,
            quality_flags: CaptureQualityFlags(CaptureQualityFlags::CLOCK_UNQUALIFIED),
            channels,
            transitions,
        }
    }

    #[test]
    fn realtime_input_snapshot_round_trips_slot_mapping_and_exact_sample_cycles() {
        let status = SafetyInputStatus {
            input_count: 4,
            known_mask: 0b1111,
            active_mask: 0b1001,
            required_mask: 0b0111,
            stale_mask: 0,
            transition_generation: 7,
            next_watchdog_deadline: Some(DeviceCycle(2_500)),
        };
        let records = [
            RealtimeInputRecord {
                resource: ResourceId::Gpio(33),
                sampled_at: Some(DeviceCycle(2_000)),
            },
            RealtimeInputRecord {
                resource: ResourceId::Gpio(32),
                sampled_at: Some(DeviceCycle(2_001)),
            },
            RealtimeInputRecord {
                resource: ResourceId::Gpio(22),
                sampled_at: Some(DeviceCycle(2_002)),
            },
            RealtimeInputRecord {
                resource: ResourceId::Gpio(35),
                sampled_at: Some(DeviceCycle(2_003)),
            },
        ];
        let mut encoded =
            [0_u8; REALTIME_INPUT_SNAPSHOT_HEADER_BYTES + 4 * REALTIME_INPUT_SNAPSHOT_RECORD_BYTES];
        assert_eq!(
            encode_realtime_input_snapshot(
                &RealtimeInputSnapshotDocument {
                    status,
                    records: &records,
                },
                &mut encoded,
            ),
            Ok(encoded.len())
        );
        assert_eq!(&encoded[..8], b"ALMRTI01");
        let decoded = decode_realtime_input_snapshot(&encoded).unwrap();
        assert_eq!(decoded.status(), status);
        assert_eq!(decoded.record_count(), records.len());
        assert_eq!(
            decoded.records().collect::<heapless_test::Vec4<_>>(),
            heapless_test::Vec4(records)
        );
    }

    #[test]
    fn realtime_input_snapshot_rejects_ambiguous_slots_and_noncanonical_wire() {
        let status = SafetyInputStatus {
            input_count: 2,
            known_mask: 0b01,
            active_mask: 0,
            required_mask: 0b11,
            stale_mask: 0b10,
            transition_generation: 1,
            next_watchdog_deadline: None,
        };
        let mut records = [
            RealtimeInputRecord {
                resource: ResourceId::Gpio(22),
                sampled_at: Some(DeviceCycle(10)),
            },
            RealtimeInputRecord {
                resource: ResourceId::Gpio(32),
                sampled_at: None,
            },
        ];
        let mut encoded =
            [0_u8; REALTIME_INPUT_SNAPSHOT_HEADER_BYTES + 2 * REALTIME_INPUT_SNAPSHOT_RECORD_BYTES];
        assert_eq!(
            encode_realtime_input_snapshot(
                &RealtimeInputSnapshotDocument {
                    status,
                    records: &records,
                },
                &mut encoded,
            ),
            Ok(encoded.len())
        );

        records[1].resource = records[0].resource;
        assert_eq!(
            encode_realtime_input_snapshot(
                &RealtimeInputSnapshotDocument {
                    status,
                    records: &records,
                },
                &mut encoded,
            ),
            Err(DiagnosticError::Resource)
        );
        records[1].resource = ResourceId::Gpio(32);
        records[0].sampled_at = None;
        assert_eq!(
            encode_realtime_input_snapshot(
                &RealtimeInputSnapshotDocument {
                    status,
                    records: &records,
                },
                &mut encoded,
            ),
            Err(DiagnosticError::Quality)
        );

        records[0].sampled_at = Some(DeviceCycle(10));
        encode_realtime_input_snapshot(
            &RealtimeInputSnapshotDocument {
                status,
                records: &records,
            },
            &mut encoded,
        )
        .unwrap();
        encoded[REALTIME_INPUT_SNAPSHOT_HEADER_BYTES + 5] = 1;
        assert_eq!(
            decode_realtime_input_snapshot(&encoded),
            Err(DiagnosticError::Reserved)
        );

        encoded[REALTIME_INPUT_SNAPSHOT_HEADER_BYTES + 5] = 0;
        let second_record =
            REALTIME_INPUT_SNAPSHOT_HEADER_BYTES + REALTIME_INPUT_SNAPSHOT_RECORD_BYTES;
        encoded.copy_within(
            REALTIME_INPUT_SNAPSHOT_HEADER_BYTES..REALTIME_INPUT_SNAPSHOT_HEADER_BYTES + 4,
            second_record,
        );
        assert_eq!(
            decode_realtime_input_snapshot(&encoded),
            Err(DiagnosticError::Resource)
        );
    }

    #[test]
    fn overview_round_trips_exact_identity_quality_age_and_ratio() {
        let mut records = samples();
        records[0].value = ResourceValue::ExactRatio {
            numerator: -7,
            denominator: 3,
        };
        let document = ResourceOverviewDocument {
            context: context(),
            flags: OverviewFlags(OverviewFlags::SIMULATED),
            snapshot_cycle: DeviceCycle(2_000),
            sequence: 9,
            samples: &records,
        };
        let mut encoded =
            [0_u8; RESOURCE_OVERVIEW_HEADER_BYTES + 4 * RESOURCE_OVERVIEW_SAMPLE_BYTES];
        assert_eq!(
            encode_resource_overview(&document, &mut encoded),
            Ok(encoded.len())
        );
        assert_eq!(&encoded[..8], b"ALMOVW01");
        let decoded = decode_resource_overview(&encoded, DiagnosticLimits::interactive()).unwrap();
        assert_eq!(decoded.context(), document.context);
        assert_eq!(decoded.snapshot_cycle(), DeviceCycle(2_000));
        assert_eq!(decoded.sequence(), 9);
        assert_eq!(
            decoded.samples().collect::<heapless_test::Vec4<_>>(),
            heapless_test::Vec4(records)
        );
        assert_eq!(decoded.sample(ResourceId::Gpio(33)), Some(records[2]));
    }

    #[test]
    fn overview_rejects_order_provenance_ratio_bounds_and_trailing_bytes() {
        let mut encoded =
            [0_u8; RESOURCE_OVERVIEW_HEADER_BYTES + 4 * RESOURCE_OVERVIEW_SAMPLE_BYTES];
        let mut records = samples();
        records.swap(0, 1);
        assert_eq!(
            encode_resource_overview(
                &ResourceOverviewDocument {
                    context: context(),
                    flags: OverviewFlags(OverviewFlags::SIMULATED),
                    snapshot_cycle: DeviceCycle(2_000),
                    sequence: 1,
                    samples: &records,
                },
                &mut encoded,
            ),
            Err(DiagnosticError::Resource)
        );

        let mut records = samples();
        records[0].provenance = SampleProvenance::Measured;
        assert_eq!(
            encode_resource_overview(
                &ResourceOverviewDocument {
                    context: context(),
                    flags: OverviewFlags(OverviewFlags::SIMULATED),
                    snapshot_cycle: DeviceCycle(2_000),
                    sequence: 1,
                    samples: &records,
                },
                &mut encoded
            ),
            Err(DiagnosticError::Quality)
        );
        records[0].provenance = SampleProvenance::Simulated;
        records[0].value = ResourceValue::ExactRatio {
            numerator: 2,
            denominator: 4,
        };
        assert_eq!(
            encode_resource_overview(
                &ResourceOverviewDocument {
                    context: context(),
                    flags: OverviewFlags(OverviewFlags::SIMULATED),
                    snapshot_cycle: DeviceCycle(2_000),
                    sequence: 1,
                    samples: &records,
                },
                &mut encoded
            ),
            Err(DiagnosticError::Value)
        );

        let records = samples();
        encode_resource_overview(
            &ResourceOverviewDocument {
                context: context(),
                flags: OverviewFlags(OverviewFlags::SIMULATED),
                snapshot_cycle: DeviceCycle(2_000),
                sequence: 1,
                samples: &records,
            },
            &mut encoded,
        )
        .unwrap();
        let tight = DiagnosticLimits {
            maximum_overview_samples: 3,
            ..DiagnosticLimits::interactive()
        };
        assert_eq!(
            decode_resource_overview(&encoded, tight),
            Err(DiagnosticError::Limit("overview samples"))
        );
        let mut trailing = encoded.to_vec();
        trailing.push(0);
        assert_eq!(
            decode_resource_overview(&trailing, DiagnosticLimits::interactive()),
            Err(DiagnosticError::Length)
        );
    }

    #[test]
    fn triggered_digital_capture_round_trips_without_allocation_in_decoder() {
        let channels = channels();
        let transitions = transitions();
        let document = capture(&channels, &transitions);
        let mut encoded = [0_u8;
            DIGITAL_CAPTURE_HEADER_BYTES
                + 4 * DIGITAL_CAPTURE_CHANNEL_BYTES
                + 6 * DIGITAL_TRANSITION_BYTES];
        assert_eq!(
            encode_digital_capture(&document, &mut encoded),
            Ok(encoded.len())
        );
        let decoded = decode_digital_capture(&encoded, DiagnosticLimits::interactive()).unwrap();
        assert_eq!(decoded.context(), document.context);
        assert_eq!(decoded.capture_id(), document.capture_id);
        assert_eq!(
            decoded.cycle_window(),
            (DeviceCycle(2_000), DeviceCycle(4_000))
        );
        assert_eq!(decoded.trigger().0, DeviceCycle(2_500));
        assert_eq!(
            decoded.channels().collect::<heapless_test::Vec4<_>>(),
            heapless_test::Vec4(channels)
        );
        assert_eq!(
            decoded.transitions().collect::<heapless_test::Vec6<_>>(),
            heapless_test::Vec6(transitions)
        );
    }

    #[test]
    fn capture_rejects_trigger_order_limits_reserved_and_decimation_mismatch() {
        let channels = channels();
        let mut events = transitions();
        events[4].level = DigitalLevel::Low;
        let document = capture(&channels, &events);
        let mut encoded = [0_u8;
            DIGITAL_CAPTURE_HEADER_BYTES
                + 4 * DIGITAL_CAPTURE_CHANNEL_BYTES
                + 6 * DIGITAL_TRANSITION_BYTES];
        assert_eq!(
            encode_digital_capture(&document, &mut encoded),
            Err(DiagnosticError::Transition)
        );

        let events = transitions();
        let mut document = capture(&channels, &events);
        document.retained_event_stride = 2;
        assert_eq!(
            encode_digital_capture(&document, &mut encoded),
            Err(DiagnosticError::Quality)
        );

        document = capture(&channels, &events);
        encode_digital_capture(&document, &mut encoded).unwrap();
        let tight = DiagnosticLimits {
            maximum_capture_transitions: 5,
            ..DiagnosticLimits::interactive()
        };
        assert_eq!(
            decode_digital_capture(&encoded, tight),
            Err(DiagnosticError::Limit("capture transitions"))
        );
        encoded[192..194].copy_from_slice(&257_u16.to_le_bytes());
        assert_eq!(
            decode_digital_capture(&encoded, DiagnosticLimits::interactive()),
            Err(DiagnosticError::Limit("capture channels"))
        );
        encoded[192..194].copy_from_slice(&4_u16.to_le_bytes());
        encoded[212] = 1;
        assert_eq!(
            decode_digital_capture(&encoded, DiagnosticLimits::interactive()),
            Err(DiagnosticError::Reserved)
        );
    }

    #[test]
    fn immediate_trigger_requires_an_exact_posttrigger_window() {
        let channels = channels();
        let transitions = transitions();
        let mut document = capture(&channels, &transitions);
        document.trigger_condition = DigitalTriggerCondition::Immediate;
        document.trigger_cycle = document.start_cycle;
        document.trigger_channel_index = NO_TRIGGER_CHANNEL;
        document.trigger_transition_index = NO_TRIGGER_TRANSITION;
        document.requested_pretrigger_cycles = 0;
        document.requested_posttrigger_cycles = 2_000;
        let mut encoded = [0_u8;
            DIGITAL_CAPTURE_HEADER_BYTES
                + 4 * DIGITAL_CAPTURE_CHANNEL_BYTES
                + 6 * DIGITAL_TRANSITION_BYTES];
        assert_eq!(
            encode_digital_capture(&document, &mut encoded),
            Ok(encoded.len())
        );

        document.requested_posttrigger_cycles = 2_001;
        assert_eq!(
            encode_digital_capture(&document, &mut encoded),
            Err(DiagnosticError::Window)
        );
        document.quality_flags.0 |= CaptureQualityFlags::POSTTRIGGER_TRUNCATED;
        assert_eq!(
            encode_digital_capture(&document, &mut encoded),
            Ok(encoded.len())
        );
    }

    mod heapless_test {
        #[derive(Debug, Eq, PartialEq)]
        pub struct Vec4<T>(pub [T; 4]);

        impl<T> FromIterator<T> for Vec4<T> {
            fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
                let mut iter = iter.into_iter();
                let result = [(); 4].map(|()| iter.next().expect("four fixture records"));
                assert!(iter.next().is_none());
                Self(result)
            }
        }

        #[derive(Debug, Eq, PartialEq)]
        pub struct Vec6<T>(pub [T; 6]);

        impl<T> FromIterator<T> for Vec6<T> {
            fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
                let mut iter = iter.into_iter();
                let result = [(); 6].map(|()| iter.next().expect("six fixture records"));
                assert!(iter.next().is_none());
                Self(result)
            }
        }
    }
}
