#![no_std]
#![doc = "Canonical heartbeat and conservative exact clock mapping for Alumina."]

use alumina_protocol::DeviceCycle;

/// Exact request body carried by `ClockHeartbeat`.
pub const CLOCK_HEARTBEAT_REQUEST_BYTES: usize = 32;
/// Exact response body carried by `ClockHeartbeat`.
pub const CLOCK_HEARTBEAT_RESPONSE_BYTES: usize = 128;
/// Exact core-1 deadline telemetry carried across the owned inter-core ring.
pub const REALTIME_CLOCK_REPORT_BYTES: usize = 40;
/// Public boot identity bytes, shared with the authenticated HTTP boot nonce.
pub const BOOT_ID_BYTES: usize = 16;

const REQUEST_MAGIC: [u8; 8] = *b"ALMCLKQ1";
const RESPONSE_MAGIC: [u8; 8] = *b"ALMCLKR1";
const REALTIME_REPORT_MAGIC: [u8; 8] = *b"ALMCRTR1";
const CLOCK_WIRE_VERSION: u16 = 1;
const NANOS_PER_SECOND: u128 = 1_000_000_000;
const PPM_SCALE: u128 = 1_000_000;
const RATE_DENOMINATOR: u128 = NANOS_PER_SECOND * PPM_SCALE;
const MAX_DECLARED_FREQUENCY_HZ: u64 = 10_000_000_000;

/// Nonzero random identity that invalidates every model and prepared job at
/// reboot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootId([u8; BOOT_ID_BYTES]);

impl BootId {
    /// Rejects the all-zero sentinel.
    pub const fn new(bytes: [u8; BOOT_ID_BYTES]) -> Result<Self, ClockWireError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(ClockWireError::BootId)
    }

    /// Exact public boot identity bytes.
    pub const fn as_bytes(self) -> [u8; BOOT_ID_BYTES] {
        self.0
    }
}

/// Physical/driver source of the exported counter domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ClockSource {
    /// Embassy's monotonic hardware-backed time-driver domain.
    EmbassyMonotonic = 1,
}

impl ClockSource {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::EmbassyMonotonic),
            _ => None,
        }
    }
}

/// Clock and executor facts sampled into one heartbeat.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct ClockFlags(pub u16);

impl ClockFlags {
    /// Counter is monotonic for this boot.
    pub const MONOTONIC: u16 = 1 << 0;
    /// Both ESP application cores observe the same counter domain.
    pub const SHARED_BETWEEN_CORES: u16 = 1 << 1;
    /// Latest real-time deadline observation is within local policy.
    pub const DEADLINE_HEALTHY: u16 = 1 << 2;
    /// A cached job currently owns preparation state.
    pub const JOB_PREPARED: u16 = 1 << 3;
    /// A future local start cycle is installed but not necessarily confirmed.
    pub const JOB_COMMITTED: u16 = 1 << 4;
    /// A job is executing in the local clock domain.
    pub const JOB_RUNNING: u16 = 1 << 5;
    /// The safety owner currently reports a fault or stale observation.
    pub const SAFETY_UNHEALTHY: u16 = 1 << 6;

    const KNOWN: u16 = Self::MONOTONIC
        | Self::SHARED_BETWEEN_CORES
        | Self::DEADLINE_HEALTHY
        | Self::JOB_PREPARED
        | Self::JOB_COMMITTED
        | Self::JOB_RUNNING
        | Self::SAFETY_UNHEALTHY;

    /// Tests one known bit.
    pub const fn contains(self, flag: u16) -> bool {
        self.0 & flag != 0
    }
}

/// Browser-originated timestamp echo request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockHeartbeatRequest {
    /// Nonzero caller probe identity, increasing within one browser model.
    pub probe_id: u64,
    /// Browser-worker monotonic send timestamp in nanoseconds.
    pub ui_send_ns: u64,
}

impl ClockHeartbeatRequest {
    /// Encodes the unique fixed request representation.
    pub fn encode(self) -> Result<[u8; CLOCK_HEARTBEAT_REQUEST_BYTES], ClockWireError> {
        self.validate()?;
        let mut encoded = [0_u8; CLOCK_HEARTBEAT_REQUEST_BYTES];
        encoded[0..8].copy_from_slice(&REQUEST_MAGIC);
        encoded[8..10].copy_from_slice(&CLOCK_WIRE_VERSION.to_le_bytes());
        // Bytes 10..16 are reserved zero.
        encoded[16..24].copy_from_slice(&self.probe_id.to_le_bytes());
        encoded[24..32].copy_from_slice(&self.ui_send_ns.to_le_bytes());
        Ok(encoded)
    }

    /// Decodes and re-encodes to reject alternate representations.
    pub fn decode(encoded: &[u8]) -> Result<Self, ClockWireError> {
        if encoded.len() != CLOCK_HEARTBEAT_REQUEST_BYTES {
            return Err(ClockWireError::Length);
        }
        if encoded[0..8] != REQUEST_MAGIC {
            return Err(ClockWireError::Magic);
        }
        if read_u16(encoded, 8) != CLOCK_WIRE_VERSION {
            return Err(ClockWireError::Version);
        }
        if encoded[10..16].iter().any(|byte| *byte != 0) {
            return Err(ClockWireError::Reserved);
        }
        let request = Self {
            probe_id: read_u64(encoded, 16),
            ui_send_ns: read_u64(encoded, 24),
        };
        request.validate()?;
        if request.encode()? != encoded {
            return Err(ClockWireError::Noncanonical);
        }
        Ok(request)
    }

    fn validate(self) -> Result<(), ClockWireError> {
        if self.probe_id == 0 {
            return Err(ClockWireError::ProbeId);
        }
        Ok(())
    }
}

/// One device-side receive/transmit timestamp pair and bounded scheduler facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockHeartbeatResponse {
    pub flags: ClockFlags,
    pub counter_bits: u8,
    pub source: ClockSource,
    pub probe_id: u64,
    pub ui_send_ns: u64,
    pub boot_id: BootId,
    pub receive_cycle: DeviceCycle,
    pub transmit_cycle: DeviceCycle,
    pub frequency_hz: u64,
    pub minimum_lead_cycles: u64,
    pub maximum_schedule_horizon_cycles: u64,
    pub queue_horizon_cycles: u64,
    pub maximum_lateness_cycles: u64,
    pub missed_deadlines: u64,
    pub command_queue_free: u32,
    pub work_queue_depth: u32,
}

impl ClockHeartbeatResponse {
    /// Encodes one strict 128-byte response.
    pub fn encode(self) -> Result<[u8; CLOCK_HEARTBEAT_RESPONSE_BYTES], ClockWireError> {
        self.validate()?;
        let mut encoded = [0_u8; CLOCK_HEARTBEAT_RESPONSE_BYTES];
        encoded[0..8].copy_from_slice(&RESPONSE_MAGIC);
        encoded[8..10].copy_from_slice(&CLOCK_WIRE_VERSION.to_le_bytes());
        encoded[10..12].copy_from_slice(&self.flags.0.to_le_bytes());
        encoded[12] = self.counter_bits;
        encoded[13] = self.source as u8;
        // Bytes 14..16 and 120..128 are reserved zero.
        encoded[16..24].copy_from_slice(&self.probe_id.to_le_bytes());
        encoded[24..32].copy_from_slice(&self.ui_send_ns.to_le_bytes());
        encoded[32..48].copy_from_slice(&self.boot_id.as_bytes());
        encoded[48..56].copy_from_slice(&self.receive_cycle.0.to_le_bytes());
        encoded[56..64].copy_from_slice(&self.transmit_cycle.0.to_le_bytes());
        encoded[64..72].copy_from_slice(&self.frequency_hz.to_le_bytes());
        encoded[72..80].copy_from_slice(&self.minimum_lead_cycles.to_le_bytes());
        encoded[80..88].copy_from_slice(&self.maximum_schedule_horizon_cycles.to_le_bytes());
        encoded[88..96].copy_from_slice(&self.queue_horizon_cycles.to_le_bytes());
        encoded[96..104].copy_from_slice(&self.maximum_lateness_cycles.to_le_bytes());
        encoded[104..112].copy_from_slice(&self.missed_deadlines.to_le_bytes());
        encoded[112..116].copy_from_slice(&self.command_queue_free.to_le_bytes());
        encoded[116..120].copy_from_slice(&self.work_queue_depth.to_le_bytes());
        Ok(encoded)
    }

    /// Decodes and validates the exact fixed response.
    pub fn decode(encoded: &[u8]) -> Result<Self, ClockWireError> {
        if encoded.len() != CLOCK_HEARTBEAT_RESPONSE_BYTES {
            return Err(ClockWireError::Length);
        }
        if encoded[0..8] != RESPONSE_MAGIC {
            return Err(ClockWireError::Magic);
        }
        if read_u16(encoded, 8) != CLOCK_WIRE_VERSION {
            return Err(ClockWireError::Version);
        }
        if encoded[14..16].iter().any(|byte| *byte != 0)
            || encoded[120..128].iter().any(|byte| *byte != 0)
        {
            return Err(ClockWireError::Reserved);
        }
        let mut boot_id = [0_u8; BOOT_ID_BYTES];
        boot_id.copy_from_slice(&encoded[32..48]);
        let response = Self {
            flags: ClockFlags(read_u16(encoded, 10)),
            counter_bits: encoded[12],
            source: ClockSource::from_wire(encoded[13]).ok_or(ClockWireError::Source)?,
            probe_id: read_u64(encoded, 16),
            ui_send_ns: read_u64(encoded, 24),
            boot_id: BootId::new(boot_id)?,
            receive_cycle: DeviceCycle(read_u64(encoded, 48)),
            transmit_cycle: DeviceCycle(read_u64(encoded, 56)),
            frequency_hz: read_u64(encoded, 64),
            minimum_lead_cycles: read_u64(encoded, 72),
            maximum_schedule_horizon_cycles: read_u64(encoded, 80),
            queue_horizon_cycles: read_u64(encoded, 88),
            maximum_lateness_cycles: read_u64(encoded, 96),
            missed_deadlines: read_u64(encoded, 104),
            command_queue_free: read_u32(encoded, 112),
            work_queue_depth: read_u32(encoded, 116),
        };
        response.validate()?;
        if response.encode()? != encoded {
            return Err(ClockWireError::Noncanonical);
        }
        Ok(response)
    }

    fn validate(self) -> Result<(), ClockWireError> {
        if self.flags.0 & !ClockFlags::KNOWN != 0
            || !self.flags.contains(ClockFlags::MONOTONIC)
            || !self.flags.contains(ClockFlags::SHARED_BETWEEN_CORES)
            || self.flags.contains(ClockFlags::JOB_RUNNING)
                && (!self.flags.contains(ClockFlags::JOB_COMMITTED)
                    || !self.flags.contains(ClockFlags::JOB_PREPARED))
            || self.flags.contains(ClockFlags::JOB_COMMITTED)
                && !self.flags.contains(ClockFlags::JOB_PREPARED)
        {
            return Err(ClockWireError::Flags);
        }
        if self.counter_bits != 64 {
            return Err(ClockWireError::CounterBits);
        }
        if self.probe_id == 0 {
            return Err(ClockWireError::ProbeId);
        }
        if self.transmit_cycle.0 < self.receive_cycle.0 {
            return Err(ClockWireError::CycleOrder);
        }
        if self.frequency_hz == 0 || self.frequency_hz > MAX_DECLARED_FREQUENCY_HZ {
            return Err(ClockWireError::Frequency);
        }
        if self.minimum_lead_cycles == 0
            || self.maximum_schedule_horizon_cycles < self.minimum_lead_cycles
        {
            return Err(ClockWireError::Horizon);
        }
        Ok(())
    }
}

/// Canonical heartbeat rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockWireError {
    Length,
    Magic,
    Version,
    Reserved,
    Noncanonical,
    ProbeId,
    BootId,
    Source,
    Flags,
    CounterBits,
    CycleOrder,
    Frequency,
    Horizon,
    DeadlineCounters,
}

/// Cumulative real-time scheduler evidence for this boot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeClockReport {
    pub samples: u64,
    pub missed_deadlines: u64,
    pub maximum_lateness_cycles: u64,
}

impl RealtimeClockReport {
    /// Encodes one canonical fixed report.
    pub fn encode(self) -> Result<[u8; REALTIME_CLOCK_REPORT_BYTES], ClockWireError> {
        self.validate()?;
        let mut encoded = [0_u8; REALTIME_CLOCK_REPORT_BYTES];
        encoded[0..8].copy_from_slice(&REALTIME_REPORT_MAGIC);
        encoded[8..10].copy_from_slice(&CLOCK_WIRE_VERSION.to_le_bytes());
        // Bytes 10..16 are reserved zero.
        encoded[16..24].copy_from_slice(&self.samples.to_le_bytes());
        encoded[24..32].copy_from_slice(&self.missed_deadlines.to_le_bytes());
        encoded[32..40].copy_from_slice(&self.maximum_lateness_cycles.to_le_bytes());
        Ok(encoded)
    }

    /// Decodes and re-encodes to enforce one representation.
    pub fn decode(encoded: &[u8]) -> Result<Self, ClockWireError> {
        if encoded.len() != REALTIME_CLOCK_REPORT_BYTES {
            return Err(ClockWireError::Length);
        }
        if encoded[0..8] != REALTIME_REPORT_MAGIC {
            return Err(ClockWireError::Magic);
        }
        if read_u16(encoded, 8) != CLOCK_WIRE_VERSION {
            return Err(ClockWireError::Version);
        }
        if encoded[10..16].iter().any(|byte| *byte != 0) {
            return Err(ClockWireError::Reserved);
        }
        let report = Self {
            samples: read_u64(encoded, 16),
            missed_deadlines: read_u64(encoded, 24),
            maximum_lateness_cycles: read_u64(encoded, 32),
        };
        report.validate()?;
        if report.encode()? != encoded {
            return Err(ClockWireError::Noncanonical);
        }
        Ok(report)
    }

    fn validate(self) -> Result<(), ClockWireError> {
        if self.samples == 0
            || self.missed_deadlines > self.samples
            || self.missed_deadlines != 0 && self.maximum_lateness_cycles == 0
        {
            Err(ClockWireError::DeadlineCounters)
        } else {
            Ok(())
        }
    }
}

/// Browser-side constraints for a conservative affine device-clock model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockEstimationPolicy {
    /// Maximum accepted browser round trip.
    pub maximum_round_trip_ns: u64,
    /// Maximum time spent between device receive/transmit samples.
    pub maximum_device_processing_cycles: u64,
    /// Symmetric oscillator/rate envelope around declared frequency.
    pub maximum_drift_ppm: u32,
    /// Model expires this long after the newest browser receive timestamp.
    pub maximum_sample_age_ns: u64,
    /// UI target may be no farther into the future than this.
    pub maximum_schedule_horizon_ns: u64,
    /// UI target must be at least this far into the future.
    pub minimum_schedule_lead_ns: u64,
    /// Minimum intersecting samples before mapping is admitted.
    pub minimum_samples: u8,
}

impl ClockEstimationPolicy {
    pub fn validate(self) -> Result<(), ClockEstimateError> {
        if self.maximum_round_trip_ns == 0
            || self.maximum_sample_age_ns == 0
            || self.maximum_schedule_horizon_ns < self.minimum_schedule_lead_ns
            || self.minimum_schedule_lead_ns == 0
            || self.minimum_samples == 0
            || self.maximum_drift_ppm >= 1_000_000
        {
            return Err(ClockEstimateError::Policy);
        }
        Ok(())
    }
}

/// One completed browser/device four-timestamp exchange.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockObservation {
    pub response: ClockHeartbeatResponse,
    /// Browser-worker monotonic timestamp immediately after response receipt.
    pub ui_receive_ns: u64,
}

/// Conservative mapped interval for one future browser time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockPrediction {
    pub boot_id: BootId,
    pub latest_probe_id: u64,
    /// Browser-worker time at which freshness and local deadline admission were checked.
    pub now_ui_ns: u64,
    pub target_ui_ns: u64,
    /// Earliest retained device cycle at [`Self::now_ui_ns`].
    pub now_earliest_cycle: DeviceCycle,
    /// Latest retained device cycle at [`Self::now_ui_ns`].
    pub now_latest_cycle: DeviceCycle,
    pub earliest_cycle: DeviceCycle,
    pub scheduled_cycle: DeviceCycle,
    pub latest_cycle: DeviceCycle,
    pub uncertainty_cycles: u64,
    pub accepted_samples: u32,
}

/// Conservative mapped interval at one browser monotonic observation time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockInstantEstimate {
    /// Boot identity owning every retained causal sample.
    pub boot_id: BootId,
    /// Newest heartbeat identity contributing deadline evidence.
    pub latest_probe_id: u64,
    /// Browser monotonic instant being mapped.
    pub ui_ns: u64,
    /// Earliest possible device cycle at [`Self::ui_ns`].
    pub earliest_cycle: DeviceCycle,
    /// Integer midpoint used only as a representative display value.
    pub midpoint_cycle: DeviceCycle,
    /// Latest possible device cycle at [`Self::ui_ns`].
    pub latest_cycle: DeviceCycle,
    /// Maximum distance from the midpoint to either exact interval endpoint.
    pub uncertainty_cycles: u64,
    /// Number of intersecting causal samples in this estimate.
    pub accepted_samples: u32,
}

/// Allocation-free exact interval estimator. It assumes only a bounded rate
/// drift and causal request/response ordering; it does not assume symmetric
/// Wi-Fi delay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockEstimator {
    policy: ClockEstimationPolicy,
    boot_id: Option<BootId>,
    frequency_hz: u64,
    rate_low: u128,
    rate_high: u128,
    offset_low_scaled: i128,
    offset_high_scaled: i128,
    latest_probe_id: u64,
    latest_ui_send_ns: u64,
    latest_ui_receive_ns: u64,
    latest_receive_cycle: u64,
    latest_flags: ClockFlags,
    minimum_lead_cycles: u64,
    maximum_schedule_horizon_cycles: u64,
    accepted_samples: u32,
    rejected_samples: u32,
}

impl ClockEstimator {
    /// Starts without a boot identity or usable sample.
    pub fn new(policy: ClockEstimationPolicy) -> Result<Self, ClockEstimateError> {
        policy.validate()?;
        Ok(Self {
            policy,
            boot_id: None,
            frequency_hz: 0,
            rate_low: 0,
            rate_high: 0,
            offset_low_scaled: 0,
            offset_high_scaled: 0,
            latest_probe_id: 0,
            latest_ui_send_ns: 0,
            latest_ui_receive_ns: 0,
            latest_receive_cycle: 0,
            latest_flags: ClockFlags(0),
            minimum_lead_cycles: 0,
            maximum_schedule_horizon_cycles: 0,
            accepted_samples: 0,
            rejected_samples: 0,
        })
    }

    /// Explicitly drops every boot-scoped model fact.
    pub fn reset(&mut self) {
        self.boot_id = None;
        self.frequency_hz = 0;
        self.rate_low = 0;
        self.rate_high = 0;
        self.offset_low_scaled = 0;
        self.offset_high_scaled = 0;
        self.latest_probe_id = 0;
        self.latest_ui_send_ns = 0;
        self.latest_ui_receive_ns = 0;
        self.latest_receive_cycle = 0;
        self.latest_flags = ClockFlags(0);
        self.minimum_lead_cycles = 0;
        self.maximum_schedule_horizon_cycles = 0;
        self.accepted_samples = 0;
        self.rejected_samples = 0;
    }

    /// Intersects one causal sample with the current exact rate/offset envelope.
    pub fn observe(&mut self, observation: ClockObservation) -> Result<(), ClockEstimateError> {
        observation
            .response
            .validate()
            .map_err(ClockEstimateError::Wire)?;
        let response = observation.response;
        if observation.ui_receive_ns <= response.ui_send_ns {
            return self.reject(ClockEstimateError::UiOrder);
        }
        let round_trip = observation.ui_receive_ns - response.ui_send_ns;
        if round_trip > self.policy.maximum_round_trip_ns {
            return self.reject(ClockEstimateError::RoundTrip);
        }
        let processing = response.transmit_cycle.0 - response.receive_cycle.0;
        if processing > self.policy.maximum_device_processing_cycles {
            return self.reject(ClockEstimateError::DeviceProcessing);
        }
        if let Some(boot_id) = self.boot_id {
            if boot_id != response.boot_id {
                return self.reject(ClockEstimateError::BootChanged);
            }
            if self.frequency_hz != response.frequency_hz {
                return self.reject(ClockEstimateError::FrequencyChanged);
            }
            if response.probe_id <= self.latest_probe_id
                || response.ui_send_ns <= self.latest_ui_send_ns
                || observation.ui_receive_ns <= self.latest_ui_receive_ns
                || response.receive_cycle.0 < self.latest_receive_cycle
            {
                return self.reject(ClockEstimateError::Sequence);
            }
        }

        let drift = u128::from(self.policy.maximum_drift_ppm);
        let frequency = u128::from(response.frequency_hz);
        let rate_low = frequency
            .checked_mul(PPM_SCALE - drift)
            .ok_or(ClockEstimateError::Arithmetic)?;
        let rate_high = frequency
            .checked_mul(PPM_SCALE + drift)
            .ok_or(ClockEstimateError::Arithmetic)?;
        let sample_low = scaled_offset(
            response.transmit_cycle.0,
            rate_high,
            observation.ui_receive_ns,
        )?;
        let sample_high = scaled_offset(response.receive_cycle.0, rate_low, response.ui_send_ns)?;
        if sample_low > sample_high {
            return self.reject(ClockEstimateError::Inconsistent);
        }

        let (offset_low, offset_high) = if self.accepted_samples == 0 {
            (sample_low, sample_high)
        } else {
            (
                self.offset_low_scaled.max(sample_low),
                self.offset_high_scaled.min(sample_high),
            )
        };
        if offset_low > offset_high {
            return self.reject(ClockEstimateError::Inconsistent);
        }

        self.boot_id = Some(response.boot_id);
        self.frequency_hz = response.frequency_hz;
        self.rate_low = rate_low;
        self.rate_high = rate_high;
        self.offset_low_scaled = offset_low;
        self.offset_high_scaled = offset_high;
        self.latest_probe_id = response.probe_id;
        self.latest_ui_send_ns = response.ui_send_ns;
        self.latest_ui_receive_ns = observation.ui_receive_ns;
        self.latest_receive_cycle = response.receive_cycle.0;
        self.latest_flags = response.flags;
        self.minimum_lead_cycles = response.minimum_lead_cycles;
        self.maximum_schedule_horizon_cycles = response.maximum_schedule_horizon_cycles;
        self.accepted_samples = self
            .accepted_samples
            .checked_add(1)
            .ok_or(ClockEstimateError::Arithmetic)?;
        Ok(())
    }

    /// Maps a sufficiently future UI time to a local integer-cycle interval and
    /// midpoint, rejecting an uncertainty wider than the caller's certificate.
    pub fn predict(
        &self,
        now_ui_ns: u64,
        target_ui_ns: u64,
        maximum_uncertainty_cycles: u64,
    ) -> Result<ClockPrediction, ClockEstimateError> {
        let boot_id = self
            .boot_id
            .ok_or(ClockEstimateError::InsufficientSamples)?;
        if self.accepted_samples < u32::from(self.policy.minimum_samples) {
            return Err(ClockEstimateError::InsufficientSamples);
        }
        if !self.latest_flags.contains(ClockFlags::DEADLINE_HEALTHY) {
            return Err(ClockEstimateError::Unhealthy);
        }
        let age = now_ui_ns
            .checked_sub(self.latest_ui_receive_ns)
            .ok_or(ClockEstimateError::UiOrder)?;
        if age > self.policy.maximum_sample_age_ns {
            return Err(ClockEstimateError::Stale);
        }
        let lead = target_ui_ns
            .checked_sub(now_ui_ns)
            .ok_or(ClockEstimateError::Deadline)?;
        if lead < self.policy.minimum_schedule_lead_ns
            || lead > self.policy.maximum_schedule_horizon_ns
        {
            return Err(ClockEstimateError::Deadline);
        }
        let (now_earliest, now_latest) = self.cycle_bounds(now_ui_ns)?;
        let (earliest, latest) = self.cycle_bounds(target_ui_ns)?;
        if latest < earliest {
            return Err(ClockEstimateError::Inconsistent);
        }
        let required_start = now_latest
            .checked_add(self.minimum_lead_cycles)
            .ok_or(ClockEstimateError::Arithmetic)?;
        let latest_admitted = now_earliest
            .checked_add(self.maximum_schedule_horizon_cycles)
            .ok_or(ClockEstimateError::Arithmetic)?;
        if earliest < required_start || latest > latest_admitted {
            return Err(ClockEstimateError::DeviceDeadline);
        }
        let width = latest - earliest;
        let scheduled = earliest
            .checked_add(width / 2)
            .ok_or(ClockEstimateError::Arithmetic)?;
        let uncertainty = (scheduled - earliest).max(latest - scheduled);
        if uncertainty > maximum_uncertainty_cycles {
            return Err(ClockEstimateError::Uncertainty);
        }
        Ok(ClockPrediction {
            boot_id,
            latest_probe_id: self.latest_probe_id,
            now_ui_ns,
            target_ui_ns,
            now_earliest_cycle: DeviceCycle(now_earliest),
            now_latest_cycle: DeviceCycle(now_latest),
            earliest_cycle: DeviceCycle(earliest),
            scheduled_cycle: DeviceCycle(scheduled),
            latest_cycle: DeviceCycle(latest),
            uncertainty_cycles: uncertainty,
            accepted_samples: self.accepted_samples,
        })
    }

    /// Maps a browser monotonic instant without imposing future-start lead.
    ///
    /// This is the deadline-reconciliation counterpart to [`Self::predict`].
    /// It retains the same boot, sample-count, health, freshness, arithmetic,
    /// and uncertainty gates but does not claim that `ui_ns` is schedulable as
    /// a new hardware start.
    pub fn estimate_at(
        &self,
        ui_ns: u64,
        maximum_uncertainty_cycles: u64,
    ) -> Result<ClockInstantEstimate, ClockEstimateError> {
        let boot_id = self
            .boot_id
            .ok_or(ClockEstimateError::InsufficientSamples)?;
        if self.accepted_samples < u32::from(self.policy.minimum_samples) {
            return Err(ClockEstimateError::InsufficientSamples);
        }
        if !self.latest_flags.contains(ClockFlags::DEADLINE_HEALTHY) {
            return Err(ClockEstimateError::Unhealthy);
        }
        let age = ui_ns
            .checked_sub(self.latest_ui_receive_ns)
            .ok_or(ClockEstimateError::UiOrder)?;
        if age > self.policy.maximum_sample_age_ns {
            return Err(ClockEstimateError::Stale);
        }
        let (earliest, latest) = self.cycle_bounds(ui_ns)?;
        if latest < earliest {
            return Err(ClockEstimateError::Inconsistent);
        }
        let width = latest - earliest;
        let midpoint = earliest
            .checked_add(width / 2)
            .ok_or(ClockEstimateError::Arithmetic)?;
        let uncertainty = (midpoint - earliest).max(latest - midpoint);
        if uncertainty > maximum_uncertainty_cycles {
            return Err(ClockEstimateError::Uncertainty);
        }
        Ok(ClockInstantEstimate {
            boot_id,
            latest_probe_id: self.latest_probe_id,
            ui_ns,
            earliest_cycle: DeviceCycle(earliest),
            midpoint_cycle: DeviceCycle(midpoint),
            latest_cycle: DeviceCycle(latest),
            uncertainty_cycles: uncertainty,
            accepted_samples: self.accepted_samples,
        })
    }

    /// Number of samples retained in the exact intersection.
    pub const fn accepted_samples(&self) -> u32 {
        self.accepted_samples
    }

    /// Number of observations rejected without changing the model.
    pub const fn rejected_samples(&self) -> u32 {
        self.rejected_samples
    }

    fn reject<T>(&mut self, error: ClockEstimateError) -> Result<T, ClockEstimateError> {
        self.rejected_samples = self.rejected_samples.saturating_add(1);
        Err(error)
    }

    fn cycle_bounds(&self, ui_ns: u64) -> Result<(u64, u64), ClockEstimateError> {
        let target = i128::from(ui_ns);
        let rate_low = i128::try_from(self.rate_low).map_err(|_| ClockEstimateError::Arithmetic)?;
        let rate_high =
            i128::try_from(self.rate_high).map_err(|_| ClockEstimateError::Arithmetic)?;
        let denominator =
            i128::try_from(RATE_DENOMINATOR).map_err(|_| ClockEstimateError::Arithmetic)?;
        let low_scaled = self
            .offset_low_scaled
            .checked_add(
                rate_low
                    .checked_mul(target)
                    .ok_or(ClockEstimateError::Arithmetic)?,
            )
            .ok_or(ClockEstimateError::Arithmetic)?;
        let high_scaled = self
            .offset_high_scaled
            .checked_add(
                rate_high
                    .checked_mul(target)
                    .ok_or(ClockEstimateError::Arithmetic)?,
            )
            .ok_or(ClockEstimateError::Arithmetic)?;
        let earliest = u64::try_from(div_floor(low_scaled, denominator))
            .map_err(|_| ClockEstimateError::Range)?;
        let latest = u64::try_from(div_ceil(high_scaled, denominator))
            .map_err(|_| ClockEstimateError::Range)?;
        Ok((earliest, latest))
    }
}

/// Conservative estimator rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockEstimateError {
    Policy,
    Wire(ClockWireError),
    UiOrder,
    RoundTrip,
    DeviceProcessing,
    BootChanged,
    FrequencyChanged,
    Sequence,
    Inconsistent,
    InsufficientSamples,
    Stale,
    Deadline,
    DeviceDeadline,
    Unhealthy,
    Uncertainty,
    Range,
    Arithmetic,
}

fn scaled_offset(
    device_cycle: u64,
    rate_numerator: u128,
    ui_ns: u64,
) -> Result<i128, ClockEstimateError> {
    let device = u128::from(device_cycle)
        .checked_mul(RATE_DENOMINATOR)
        .ok_or(ClockEstimateError::Arithmetic)?;
    let time = rate_numerator
        .checked_mul(u128::from(ui_ns))
        .ok_or(ClockEstimateError::Arithmetic)?;
    let device = i128::try_from(device).map_err(|_| ClockEstimateError::Arithmetic)?;
    let time = i128::try_from(time).map_err(|_| ClockEstimateError::Arithmetic)?;
    device
        .checked_sub(time)
        .ok_or(ClockEstimateError::Arithmetic)
}

fn div_floor(numerator: i128, denominator: i128) -> i128 {
    numerator.div_euclid(denominator)
}

fn div_ceil(numerator: i128, denominator: i128) -> i128 {
    let quotient = numerator.div_euclid(denominator);
    if numerator.rem_euclid(denominator) == 0 {
        quotient
    } else {
        quotient + 1
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    const BOOT: BootId = match BootId::new([0x44; BOOT_ID_BYTES]) {
        Ok(boot) => boot,
        Err(_) => panic!("test boot identity is nonzero"),
    };

    fn response(
        probe_id: u64,
        ui_send_ns: u64,
        receive: u64,
        transmit: u64,
    ) -> ClockHeartbeatResponse {
        ClockHeartbeatResponse {
            flags: ClockFlags(
                ClockFlags::MONOTONIC
                    | ClockFlags::SHARED_BETWEEN_CORES
                    | ClockFlags::DEADLINE_HEALTHY,
            ),
            counter_bits: 64,
            source: ClockSource::EmbassyMonotonic,
            probe_id,
            ui_send_ns,
            boot_id: BOOT,
            receive_cycle: DeviceCycle(receive),
            transmit_cycle: DeviceCycle(transmit),
            frequency_hz: 1_000_000,
            minimum_lead_cycles: 100_000,
            maximum_schedule_horizon_cycles: 60_000_000,
            queue_horizon_cycles: 500_000,
            maximum_lateness_cycles: 17,
            missed_deadlines: 0,
            command_queue_free: 3,
            work_queue_depth: 1,
        }
    }

    fn policy() -> ClockEstimationPolicy {
        ClockEstimationPolicy {
            maximum_round_trip_ns: 2_000_000,
            maximum_device_processing_cycles: 1_000,
            maximum_drift_ppm: 100,
            maximum_sample_age_ns: 50_000_000,
            maximum_schedule_horizon_ns: 5_000_000_000,
            minimum_schedule_lead_ns: 100_000_000,
            minimum_samples: 2,
        }
    }

    #[test]
    fn heartbeat_wire_images_are_fixed_and_canonical() {
        let request = ClockHeartbeatRequest {
            probe_id: 7,
            ui_send_ns: 1_000_000_000,
        };
        let encoded = request.encode().unwrap();
        assert_eq!(encoded.len(), CLOCK_HEARTBEAT_REQUEST_BYTES);
        assert_eq!(ClockHeartbeatRequest::decode(&encoded), Ok(request));

        let heartbeat = response(7, 1_000_000_000, 1_050_200, 1_050_250);
        let encoded = heartbeat.encode().unwrap();
        assert_eq!(encoded.len(), CLOCK_HEARTBEAT_RESPONSE_BYTES);
        assert_eq!(ClockHeartbeatResponse::decode(&encoded), Ok(heartbeat));

        let mut reserved = encoded;
        reserved[127] = 1;
        assert_eq!(
            ClockHeartbeatResponse::decode(&reserved),
            Err(ClockWireError::Reserved)
        );

        let realtime = RealtimeClockReport {
            samples: 100,
            missed_deadlines: 2,
            maximum_lateness_cycles: 117,
        };
        let encoded = realtime.encode().unwrap();
        assert_eq!(encoded.len(), REALTIME_CLOCK_REPORT_BYTES);
        assert_eq!(RealtimeClockReport::decode(&encoded), Ok(realtime));
        let mut invalid = realtime;
        invalid.missed_deadlines = invalid.samples + 1;
        assert_eq!(invalid.encode(), Err(ClockWireError::DeadlineCounters));
    }

    #[test]
    fn exact_causal_intersection_contains_the_known_future_cycle() {
        // True clock: floor(ui_ns / 1_000) + 50_000 cycles. Each exchange has
        // deliberately different asymmetric request/response delay.
        let mut estimator = ClockEstimator::new(policy()).unwrap();
        estimator
            .observe(ClockObservation {
                response: response(1, 1_000_000_000, 1_050_200, 1_050_250),
                ui_receive_ns: 1_000_700_000,
            })
            .unwrap();
        estimator
            .observe(ClockObservation {
                response: response(2, 2_000_000_000, 2_050_120, 2_050_180),
                ui_receive_ns: 2_000_500_000,
            })
            .unwrap();

        let target_ui_ns = 2_500_000_000;
        let prediction = estimator
            .predict(2_000_500_000, target_ui_ns, 1_000)
            .unwrap();
        let exact = target_ui_ns / 1_000 + 50_000;
        let exact_now = prediction.now_ui_ns / 1_000 + 50_000;
        assert!(prediction.now_earliest_cycle.0 <= exact_now);
        assert!(prediction.now_latest_cycle.0 >= exact_now);
        assert!(prediction.earliest_cycle.0 <= exact);
        assert!(prediction.latest_cycle.0 >= exact);
        assert!(prediction.uncertainty_cycles <= 1_000);
        assert_eq!(prediction.accepted_samples, 2);

        let current = estimator.estimate_at(2_000_500_000, 1_000).unwrap();
        assert_eq!(current.boot_id, BOOT);
        assert_eq!(current.ui_ns, 2_000_500_000);
        assert!(current.earliest_cycle.0 <= exact_now);
        assert!(current.latest_cycle.0 >= exact_now);
        assert!(current.uncertainty_cycles <= 1_000);
    }

    #[test]
    fn delay_boot_sequence_and_uncertainty_fail_without_mutating_model() {
        let mut estimator = ClockEstimator::new(policy()).unwrap();
        estimator
            .observe(ClockObservation {
                response: response(1, 1_000_000_000, 1_050_200, 1_050_250),
                ui_receive_ns: 1_000_700_000,
            })
            .unwrap();
        let accepted = estimator.accepted_samples();
        assert_eq!(
            estimator.observe(ClockObservation {
                response: response(2, 2_000_000_000, 2_050_200, 2_050_250),
                ui_receive_ns: 2_003_000_001,
            }),
            Err(ClockEstimateError::RoundTrip)
        );
        let mut changed = response(2, 2_000_000_000, 2_050_200, 2_050_250);
        changed.boot_id = BootId::new([0x55; BOOT_ID_BYTES]).unwrap();
        assert_eq!(
            estimator.observe(ClockObservation {
                response: changed,
                ui_receive_ns: 2_000_700_000,
            }),
            Err(ClockEstimateError::BootChanged)
        );
        assert_eq!(estimator.accepted_samples(), accepted);
        assert_eq!(estimator.rejected_samples(), 2);
        assert_eq!(
            estimator.predict(1_000_700_000, 1_500_000_000, 1_000),
            Err(ClockEstimateError::InsufficientSamples)
        );
    }

    #[test]
    fn unhealthy_deadline_evidence_and_device_lead_are_authoritative() {
        let mut unhealthy = ClockEstimator::new(policy()).unwrap();
        unhealthy
            .observe(ClockObservation {
                response: response(1, 1_000_000_000, 1_050_200, 1_050_250),
                ui_receive_ns: 1_000_700_000,
            })
            .unwrap();
        let mut unhealthy_response = response(2, 2_000_000_000, 2_050_120, 2_050_180);
        unhealthy_response.flags.0 &= !ClockFlags::DEADLINE_HEALTHY;
        unhealthy
            .observe(ClockObservation {
                response: unhealthy_response,
                ui_receive_ns: 2_000_500_000,
            })
            .unwrap();
        assert_eq!(
            unhealthy.predict(2_000_500_000, 2_500_000_000, 1_000),
            Err(ClockEstimateError::Unhealthy)
        );

        let mut lead_limited = ClockEstimator::new(policy()).unwrap();
        for (probe, send, receive, transmit, browser_receive) in [
            (1, 1_000_000_000, 1_050_200, 1_050_250, 1_000_700_000),
            (2, 2_000_000_000, 2_050_120, 2_050_180, 2_000_500_000),
        ] {
            let mut heartbeat = response(probe, send, receive, transmit);
            heartbeat.minimum_lead_cycles = 1_000_000;
            lead_limited
                .observe(ClockObservation {
                    response: heartbeat,
                    ui_receive_ns: browser_receive,
                })
                .unwrap();
        }
        assert_eq!(
            lead_limited.predict(2_000_500_000, 2_500_000_000, 1_000),
            Err(ClockEstimateError::DeviceDeadline)
        );
    }
}
