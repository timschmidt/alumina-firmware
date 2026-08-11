#![no_std]
#![doc = "Small deterministic safety-state core for Alumina devices."]

use alumina_board::ResourceId;
use alumina_protocol::DeviceCycle;

/// Maximum safety inputs representable by one fixed status mask.
pub const MAX_SAFETY_INPUTS: usize = 32;

/// Top-level real-time safety state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum SafetyState {
    /// Hardware initialization has not yet established safe outputs.
    #[default]
    Boot = 0,
    /// Outputs are at board-declared safe values and no configuration is active.
    Safe = 1,
    /// A complete configuration is active but cannot yet produce motion or energy.
    Configured = 2,
    /// Local preconditions pass and the selected job may be started.
    Armed = 3,
    /// Scheduled real-time work is executing.
    Running = 4,
    /// Motion/process work is stopped or decelerating under a validated hold policy.
    Hold = 5,
    /// A latched fault requires an explicit physically meaningful reset.
    Fault = 6,
}

impl SafetyState {
    /// Returns the exact safety-snapshot wire value.
    pub const fn wire_value(self) -> u8 {
        self as u8
    }

    /// Decodes one exact safety-snapshot wire value.
    pub const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Boot),
            1 => Some(Self::Safe),
            2 => Some(Self::Configured),
            3 => Some(Self::Armed),
            4 => Some(Self::Running),
            5 => Some(Self::Hold),
            6 => Some(Self::Fault),
            _ => None,
        }
    }
}

/// Stable high-level reason for a latched fault.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FaultCode {
    /// Physical emergency-stop input asserted.
    EmergencyStop = 1,
    /// Hard limit asserted outside its allowed operation.
    HardLimit = 2,
    /// Driver or power-stage fault input asserted.
    Driver = 3,
    /// Command or SD-prefetch horizon could not be maintained safely.
    QueueUnderrun = 4,
    /// A real-time deadline was missed.
    Deadline = 5,
    /// Configuration or job identity changed unexpectedly.
    Identity = 6,
    /// Motor current exceeded its qualified bound.
    OverCurrent = 7,
    /// Sensor plausibility or following-error check failed.
    Feedback = 8,
    /// Watchdog or maximum-output-duration limit expired.
    Watchdog = 9,
    /// Board-safe output transaction could not be established or retained.
    SafeOutput = 10,
    /// Required physical guard, door, or machine interlock opened.
    SafetyInterlock = 11,
}

impl FaultCode {
    /// Returns the exact nonzero safety-snapshot wire value.
    pub const fn wire_value(self) -> u8 {
        self as u8
    }

    /// Decodes one exact nonzero safety-snapshot wire value.
    pub const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::EmergencyStop),
            2 => Some(Self::HardLimit),
            3 => Some(Self::Driver),
            4 => Some(Self::QueueUnderrun),
            5 => Some(Self::Deadline),
            6 => Some(Self::Identity),
            7 => Some(Self::OverCurrent),
            8 => Some(Self::Feedback),
            9 => Some(Self::Watchdog),
            10 => Some(Self::SafeOutput),
            11 => Some(Self::SafetyInterlock),
            _ => None,
        }
    }
}

/// Safety-relevant meaning assigned to one physical digital input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SafetyInputRole {
    AxisLimitMinimum = 1,
    AxisLimitMaximum = 2,
    AxisMotorFault = 3,
    Probe = 4,
    EmergencyStop = 5,
    SafetyInterlock = 6,
    FocFault = 7,
}

impl SafetyInputRole {
    /// First-release arming gate before operation-specific homing/probing
    /// policies exist. Only a probe may be stably asserted while arming.
    pub const fn conservatively_requires_clear_to_arm(self) -> bool {
        !matches!(self, Self::Probe)
    }
}

/// Electrical level that means a safety input is asserted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputPolarity {
    ActiveHigh,
    ActiveLow,
}

/// Configured on-chip bias for a digital safety input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputBias {
    Floating,
    PullUp,
    PullDown,
}

/// Compact executable safety-input facts derived from one exact configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetyInputSpec {
    pub instance: u16,
    pub role: SafetyInputRole,
    pub resource: ResourceId,
    pub polarity: InputPolarity,
    pub bias: InputBias,
    /// Input must be known, fresh, and inactive before arming.
    pub required_for_arm: bool,
    /// Minimum asserted observation interval, with no contradictory or stale
    /// sample, before accepting assertion.
    pub minimum_active_cycles: u32,
    /// Minimum inactive observation interval, with no contradictory or stale
    /// sample, before accepting release or the initial clear state.
    pub minimum_inactive_cycles: u32,
    /// Maximum interval between samples before local control fails closed.
    pub maximum_sample_gap_cycles: u32,
}

impl SafetyInputSpec {
    /// Rejects specs that cannot provide a finite fail-closed observation.
    pub const fn validate(self) -> Result<(), SafetyInputError> {
        if !matches!(
            self.resource,
            ResourceId::Gpio(_) | ResourceId::SafetyInput(_)
        ) || self.maximum_sample_gap_cycles == 0
            || self.role.conservatively_requires_clear_to_arm() && !self.required_for_arm
        {
            return Err(SafetyInputError::Spec);
        }
        Ok(())
    }

    const fn active(self, level_high: bool) -> bool {
        match self.polarity {
            InputPolarity::ActiveHigh => level_high,
            InputPolarity::ActiveLow => !level_high,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MonitoredInput {
    known: bool,
    stable_active: bool,
    candidate_valid: bool,
    candidate_active: bool,
    candidate_since: u64,
    last_sample: Option<u64>,
}

impl MonitoredInput {
    const EMPTY: Self = Self {
        known: false,
        stable_active: false,
        candidate_valid: false,
        candidate_active: false,
        candidate_since: 0,
        last_sample: None,
    };
}

/// One exact debounced transition from unknown/inactive/active state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetyInputTransition {
    pub at: DeviceCycle,
    pub slot: u8,
    pub spec: SafetyInputSpec,
    /// `None` means this transition established the first stable observation.
    pub previous_active: Option<bool>,
    pub active: bool,
    pub generation: u32,
}

impl SafetyInputTransition {
    /// Conservative first-release reaction. Operation-specific homing and
    /// probing policy may consume a limit/probe before applying this fallback.
    pub const fn conservative_reaction(self) -> SafetyInputReaction {
        if !self.active {
            return SafetyInputReaction::Clear;
        }
        match self.spec.role {
            SafetyInputRole::Probe => SafetyInputReaction::Hold,
            SafetyInputRole::EmergencyStop => SafetyInputReaction::Fault(FaultCode::EmergencyStop),
            SafetyInputRole::SafetyInterlock => {
                SafetyInputReaction::Fault(FaultCode::SafetyInterlock)
            }
            SafetyInputRole::AxisLimitMinimum | SafetyInputRole::AxisLimitMaximum => {
                SafetyInputReaction::Fault(FaultCode::HardLimit)
            }
            SafetyInputRole::AxisMotorFault | SafetyInputRole::FocFault => {
                SafetyInputReaction::Fault(FaultCode::Driver)
            }
        }
    }
}

/// Immediate policy-neutral result expected by the local safety coordinator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyInputReaction {
    Clear,
    Hold,
    Fault(FaultCode),
}

/// One input whose finite sampling promise has expired.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetyInputWatchdogFault {
    pub at: DeviceCycle,
    pub slot: u8,
    pub spec: SafetyInputSpec,
}

impl SafetyInputWatchdogFault {
    /// Sampling loss is always a local watchdog fault, independent of the
    /// electrical role whose observation expired.
    pub const fn conservative_reaction(self) -> SafetyInputReaction {
        SafetyInputReaction::Fault(FaultCode::Watchdog)
    }
}

/// Bounded live input facts suitable for arming and diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetyInputStatus {
    pub input_count: u8,
    pub known_mask: u32,
    pub active_mask: u32,
    pub required_mask: u32,
    pub stale_mask: u32,
    pub transition_generation: u32,
    /// First cycle at which some currently sampled input becomes stale.
    pub next_watchdog_deadline: Option<DeviceCycle>,
}

impl SafetyInputStatus {
    /// All configured inputs are known/fresh and every required input is clear.
    pub const fn ready_to_arm(self) -> bool {
        if self.input_count == 0 || self.input_count > 32 {
            return false;
        }
        let expected = if self.input_count == 32 {
            u32::MAX
        } else {
            (1_u32 << self.input_count) - 1
        };
        self.known_mask == expected
            && self.stale_mask == 0
            && self.active_mask & self.required_mask == 0
    }
}

/// Allocation-free, exact-cycle debounce and sampling-watchdog monitor.
pub struct SafetyInputMonitor<const INPUTS: usize> {
    specs: [Option<SafetyInputSpec>; INPUTS],
    states: [MonitoredInput; INPUTS],
    count: usize,
    transition_generation: u32,
}

impl<const INPUTS: usize> SafetyInputMonitor<INPUTS> {
    /// Constructs a monitor in canonical slot order. Resource and
    /// `(role, instance)` identities must be unique.
    pub fn new(specs: &[SafetyInputSpec]) -> Result<Self, SafetyInputError> {
        Self::from_specs(specs.iter().copied())
    }

    /// Constructs from an allocation-free source such as an executable
    /// configuration-profile iterator.
    pub fn from_specs(
        specs: impl IntoIterator<Item = SafetyInputSpec>,
    ) -> Result<Self, SafetyInputError> {
        if INPUTS == 0 || INPUTS > MAX_SAFETY_INPUTS {
            return Err(SafetyInputError::Capacity);
        }
        let mut retained: [Option<SafetyInputSpec>; INPUTS] = [None; INPUTS];
        let mut count = 0;
        for spec in specs {
            if count >= INPUTS {
                return Err(SafetyInputError::Capacity);
            }
            spec.validate()?;
            let mut prior = 0;
            while prior < count {
                let previous = retained[prior].expect("prior slots are initialized");
                if previous.resource == spec.resource {
                    return Err(SafetyInputError::DuplicateResource(spec.resource));
                }
                if previous.role == spec.role && previous.instance == spec.instance {
                    return Err(SafetyInputError::DuplicateIdentity {
                        role: spec.role,
                        instance: spec.instance,
                    });
                }
                prior += 1;
            }
            if count != 0 {
                let previous = retained[count - 1].expect("prior slot is initialized");
                if (previous.instance, previous.role as u8) >= (spec.instance, spec.role as u8) {
                    return Err(SafetyInputError::Order);
                }
            }
            retained[count] = Some(spec);
            count += 1;
        }
        if count == 0 {
            return Err(SafetyInputError::Capacity);
        }
        Ok(Self {
            specs: retained,
            states: [MonitoredInput::EMPTY; INPUTS],
            count,
            transition_generation: 0,
        })
    }

    /// Number of configuration-stable input slots.
    pub const fn len(&self) -> usize {
        self.count
    }

    /// Whether no inputs are monitored. A successfully constructed monitor is
    /// never empty, but this keeps the bounded collection API conventional.
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Configuration-stable physical mapping for one sample slot.
    pub const fn spec(&self, slot: usize) -> Option<SafetyInputSpec> {
        if slot < self.count {
            self.specs[slot]
        } else {
            None
        }
    }

    /// Observes one physical level at an exact local cycle. Repeated or
    /// backwards timestamps are rejected without changing retained state.
    pub fn observe(
        &mut self,
        slot: usize,
        level_high: bool,
        at: DeviceCycle,
    ) -> Result<Option<SafetyInputTransition>, SafetyInputError> {
        let spec = self.spec(slot).ok_or(SafetyInputError::Slot { slot })?;
        let current = self.states[slot];
        if let Some(previous) = current.last_sample
            && at.0 <= previous
        {
            return Err(SafetyInputError::TimeOrder {
                slot,
                previous: DeviceCycle(previous),
                observed: at,
            });
        }
        if let Some(previous) = current.last_sample
            && at.0 - previous > u64::from(spec.maximum_sample_gap_cycles)
        {
            return Err(SafetyInputError::SampleGap {
                slot,
                previous: DeviceCycle(previous),
                observed: at,
                maximum_gap_cycles: spec.maximum_sample_gap_cycles,
            });
        }

        let active = spec.active(level_high);
        let mut next = current;
        next.last_sample = Some(at.0);
        if current.known && current.stable_active == active {
            next.candidate_valid = false;
            self.states[slot] = next;
            return Ok(None);
        }

        if !current.candidate_valid || current.candidate_active != active {
            next.candidate_valid = true;
            next.candidate_active = active;
            next.candidate_since = at.0;
        }
        let debounce = if active {
            spec.minimum_active_cycles
        } else {
            spec.minimum_inactive_cycles
        };
        let accept_at = next
            .candidate_since
            .checked_add(u64::from(debounce))
            .ok_or(SafetyInputError::Arithmetic)?;
        if at.0 < accept_at {
            self.states[slot] = next;
            return Ok(None);
        }

        let previous_active = current.known.then_some(current.stable_active);
        next.known = true;
        next.stable_active = active;
        next.candidate_valid = false;
        self.states[slot] = next;
        self.transition_generation = next_nonzero_generation(self.transition_generation);
        Ok(Some(SafetyInputTransition {
            at,
            slot: u8::try_from(slot).expect("constructor limits slots to 32"),
            spec,
            previous_active,
            active,
            generation: self.transition_generation,
        }))
    }

    /// Returns current masks and exact first watchdog-failure cycle. Unknown
    /// and time-inconsistent inputs are stale and therefore fail closed.
    pub fn status(&self, now: DeviceCycle) -> SafetyInputStatus {
        let mut known_mask = 0_u32;
        let mut active_mask = 0_u32;
        let mut required_mask = 0_u32;
        let mut stale_mask = 0_u32;
        let mut next_watchdog_deadline = None;
        let mut slot = 0;
        while slot < self.count {
            let bit = 1_u32 << slot;
            let spec = self.specs[slot].expect("constructor fills every retained slot");
            let state = self.states[slot];
            if state.known {
                known_mask |= bit;
            }
            if state.known && state.stable_active {
                active_mask |= bit;
            }
            if spec.required_for_arm {
                required_mask |= bit;
            }
            let deadline = state.last_sample.and_then(|sample| {
                sample
                    .checked_add(u64::from(spec.maximum_sample_gap_cycles))
                    .and_then(|last_healthy| last_healthy.checked_add(1))
                    .map(DeviceCycle)
            });
            let stale = match state.last_sample {
                Some(sample) => {
                    now.0 < sample || now.0 - sample > u64::from(spec.maximum_sample_gap_cycles)
                }
                None => true,
            };
            if stale {
                stale_mask |= bit;
            } else if let Some(deadline) = deadline {
                next_watchdog_deadline = minimum_device_cycle(next_watchdog_deadline, deadline);
            }
            slot += 1;
        }
        SafetyInputStatus {
            input_count: u8::try_from(self.count).expect("constructor limits inputs to 32"),
            known_mask,
            active_mask,
            required_mask,
            stale_mask,
            transition_generation: self.transition_generation,
            next_watchdog_deadline: if stale_mask == 0 {
                next_watchdog_deadline
            } else {
                None
            },
        }
    }

    /// First stale input in stable configuration order, if any.
    pub fn watchdog_fault(&self, now: DeviceCycle) -> Option<SafetyInputWatchdogFault> {
        let stale = self.status(now).stale_mask;
        if stale == 0 {
            return None;
        }
        let slot = stale.trailing_zeros() as usize;
        Some(SafetyInputWatchdogFault {
            at: now,
            slot: u8::try_from(slot).expect("mask width is 32"),
            spec: self.specs[slot].expect("stale bit names a retained slot"),
        })
    }
}

/// Rejected input configuration or non-monotonic observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafetyInputError {
    Capacity,
    Spec,
    DuplicateResource(ResourceId),
    DuplicateIdentity {
        role: SafetyInputRole,
        instance: u16,
    },
    Order,
    Slot {
        slot: usize,
    },
    TimeOrder {
        slot: usize,
        previous: DeviceCycle,
        observed: DeviceCycle,
    },
    SampleGap {
        slot: usize,
        previous: DeviceCycle,
        observed: DeviceCycle,
        maximum_gap_cycles: u32,
    },
    Arithmetic,
}

const fn next_nonzero_generation(previous: u32) -> u32 {
    let next = previous.wrapping_add(1);
    if next == 0 { 1 } else { next }
}

const fn minimum_device_cycle(
    left: Option<DeviceCycle>,
    right: DeviceCycle,
) -> Option<DeviceCycle> {
    match left {
        Some(left) if left.0 <= right.0 => Some(left),
        Some(_) | None => Some(right),
    }
}

/// Version-independent identity of the exact board safe-output transaction.
///
/// This is a semantic identifier, not a cryptographic digest. Board composition
/// must change it whenever pin mode, shifted width/order, or safe image changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetyContractId(pub [u8; 16]);

impl SafetyContractId {
    /// Sentinel that no production board composition may use.
    pub const ZERO: Self = Self([0; 16]);
}

/// Safe-output hardware was successfully placed in its declared state.
pub const SNAPSHOT_FLAG_SAFE_OUTPUTS_ESTABLISHED: u8 = 1 << 0;
/// Deterministic execution or prefetch currently owns immutable job storage.
pub const SNAPSHOT_FLAG_REALTIME_JOB_ACTIVE: u8 = 1 << 1;
const SNAPSHOT_KNOWN_FLAGS: u8 =
    SNAPSHOT_FLAG_SAFE_OUTPUTS_ESTABLISHED | SNAPSHOT_FLAG_REALTIME_JOB_ACTIVE;
const SNAPSHOT_MAGIC: [u8; 4] = *b"ALMS";
const SNAPSHOT_VERSION: u8 = 1;

/// Fixed, allocation-free core-1 safety publication.
///
/// The outer inter-core frame supplies publication sequence and production
/// cycle. This payload carries the state transition generation and binds the
/// observation to the exact board safe-output contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetySnapshot {
    /// Authoritative core-1 state.
    pub state: SafetyState,
    /// Retained fault reason; present exactly in [`SafetyState::Fault`].
    pub fault: Option<FaultCode>,
    /// Known `SNAPSHOT_FLAG_*` bits only.
    pub flags: u8,
    /// Wrapping nonzero generation incremented for every state/fact transition.
    pub transition_generation: u32,
    /// Exact board safe-output contract used by core 1.
    pub safe_output_contract: SafetyContractId,
    /// Largest sampled real-time scheduling lateness in device cycles.
    pub maximum_lateness_cycles: u64,
}

impl SafetySnapshot {
    /// Exact encoded payload size. Rust layout is never placed on a queue.
    pub const WIRE_LEN: usize = 40;

    /// Returns whether the board-safe output transaction is retained.
    pub const fn safe_outputs_established(self) -> bool {
        self.flags & SNAPSHOT_FLAG_SAFE_OUTPUTS_ESTABLISHED != 0
    }

    /// Returns whether deterministic execution/prefetch owns the job cache.
    pub const fn realtime_job_active(self) -> bool {
        self.flags & SNAPSHOT_FLAG_REALTIME_JOB_ACTIVE != 0
    }

    /// Validates cross-field invariants before publication or admission.
    pub fn validate(self) -> Result<(), SnapshotError> {
        if self.flags & !SNAPSHOT_KNOWN_FLAGS != 0 {
            return Err(SnapshotError::Flags {
                received: self.flags,
            });
        }
        if self.safe_output_contract == SafetyContractId::ZERO {
            return Err(SnapshotError::ZeroContract);
        }
        if self.state == SafetyState::Boot {
            if self.transition_generation != 0 || self.fault.is_some() {
                return Err(SnapshotError::StateInvariant);
            }
        } else if self.transition_generation == 0 {
            return Err(SnapshotError::StateInvariant);
        }
        if (self.state == SafetyState::Fault) != self.fault.is_some() {
            return Err(SnapshotError::StateInvariant);
        }
        if matches!(
            self.state,
            SafetyState::Safe
                | SafetyState::Configured
                | SafetyState::Armed
                | SafetyState::Running
                | SafetyState::Hold
        ) && !self.safe_outputs_established()
        {
            return Err(SnapshotError::StateInvariant);
        }
        if self.state == SafetyState::Running && !self.realtime_job_active() {
            return Err(SnapshotError::StateInvariant);
        }
        Ok(())
    }

    /// Encodes an exact little-endian safety payload.
    pub fn encode(self) -> Result<[u8; Self::WIRE_LEN], SnapshotError> {
        self.validate()?;
        let mut encoded = [0_u8; Self::WIRE_LEN];
        encoded[0..4].copy_from_slice(&SNAPSHOT_MAGIC);
        encoded[4] = SNAPSHOT_VERSION;
        encoded[5] = self.state.wire_value();
        encoded[6] = self.fault.map_or(0, FaultCode::wire_value);
        encoded[7] = self.flags;
        encoded[8..12].copy_from_slice(&self.transition_generation.to_le_bytes());
        encoded[12..28].copy_from_slice(&self.safe_output_contract.0);
        encoded[28..36].copy_from_slice(&self.maximum_lateness_cycles.to_le_bytes());
        // Bytes 36..40 are reserved and remain zero.
        Ok(encoded)
    }

    /// Decodes and validates one exact safety payload.
    pub fn decode(encoded: &[u8]) -> Result<Self, SnapshotError> {
        if encoded.len() != Self::WIRE_LEN {
            return Err(SnapshotError::Length {
                received: encoded.len(),
                expected: Self::WIRE_LEN,
            });
        }
        if encoded[0..4] != SNAPSHOT_MAGIC {
            return Err(SnapshotError::Magic);
        }
        if encoded[4] != SNAPSHOT_VERSION {
            return Err(SnapshotError::Version {
                received: encoded[4],
            });
        }
        if encoded[36..40] != [0; 4] {
            return Err(SnapshotError::Reserved);
        }
        let state = SafetyState::from_wire(encoded[5]).ok_or(SnapshotError::State {
            received: encoded[5],
        })?;
        let fault = match encoded[6] {
            0 => None,
            value => {
                Some(FaultCode::from_wire(value).ok_or(SnapshotError::Fault { received: value })?)
            }
        };
        let mut contract = [0_u8; 16];
        contract.copy_from_slice(&encoded[12..28]);
        let snapshot = Self {
            state,
            fault,
            flags: encoded[7],
            transition_generation: read_u32(encoded, 8),
            safe_output_contract: SafetyContractId(contract),
            maximum_lateness_cycles: read_u64(encoded, 28),
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
}

/// Malformed or internally inconsistent safety publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    /// Encoded payload length was not exact.
    Length { received: usize, expected: usize },
    /// Payload magic did not identify an Alumina safety snapshot.
    Magic,
    /// Payload version is not this greenfield protocol version.
    Version { received: u8 },
    /// State byte was unknown.
    State { received: u8 },
    /// Fault byte was unknown.
    Fault { received: u8 },
    /// Unknown flag bits were set.
    Flags { received: u8 },
    /// Reserved bytes were nonzero.
    Reserved,
    /// Safe-output contract used the forbidden zero sentinel.
    ZeroContract,
    /// State, fault, generation, and safe-output facts disagree.
    StateInvariant,
}

/// Freshness and identity policy applied by the service core.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetyObservationPolicy {
    /// Only snapshots from this exact board safe-output implementation qualify.
    pub expected_contract: SafetyContractId,
    /// Maximum accepted age in the shared monotonic device-cycle domain.
    pub maximum_age_cycles: u64,
}

/// Fail-closed effective safety facts exposed to service admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveSafety {
    /// Fresh core-1 state, otherwise [`SafetyState::Boot`].
    pub state: SafetyState,
    /// True only for a fresh, validated publication.
    pub fresh: bool,
    /// Fails closed to true when no fresh observation exists.
    pub realtime_job_active: bool,
    /// Core-1 maximum lateness, or zero without a fresh observation.
    pub maximum_lateness_cycles: u64,
}

impl EffectiveSafety {
    const UNOBSERVED: Self = Self {
        state: SafetyState::Boot,
        fresh: false,
        realtime_job_active: true,
        maximum_lateness_cycles: 0,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AcceptedObservation {
    frame_sequence: u32,
    produced_at_cycle: u64,
    snapshot: SafetySnapshot,
}

/// Core-0 validator for lossy, freshness-bounded core-1 safety publications.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafetyObserver {
    policy: SafetyObservationPolicy,
    accepted: Option<AcceptedObservation>,
}

impl SafetyObserver {
    /// Starts without an observation; callers fail closed until one is admitted.
    pub const fn new(policy: SafetyObservationPolicy) -> Self {
        Self {
            policy,
            accepted: None,
        }
    }

    /// Decodes and admits one publication or immediately invalidates the prior one.
    pub fn observe_encoded(
        &mut self,
        frame_sequence: u32,
        produced_at_cycle: u64,
        observed_at_cycle: u64,
        encoded: &[u8],
    ) -> Result<(), ObservationError> {
        let snapshot = match SafetySnapshot::decode(encoded) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.invalidate();
                return Err(ObservationError::Snapshot(error));
            }
        };
        self.observe(
            frame_sequence,
            produced_at_cycle,
            observed_at_cycle,
            snapshot,
        )
    }

    /// Admits one decoded publication or immediately invalidates the prior one.
    pub fn observe(
        &mut self,
        frame_sequence: u32,
        produced_at_cycle: u64,
        observed_at_cycle: u64,
        snapshot: SafetySnapshot,
    ) -> Result<(), ObservationError> {
        let result = self.validate_observation(
            frame_sequence,
            produced_at_cycle,
            observed_at_cycle,
            snapshot,
        );
        match result {
            Ok(()) => {
                self.accepted = Some(AcceptedObservation {
                    frame_sequence,
                    produced_at_cycle,
                    snapshot,
                });
                Ok(())
            }
            Err(error) => {
                self.invalidate();
                Err(error)
            }
        }
    }

    /// Revokes the retained observation after a fault-channel or frame failure.
    pub fn invalidate(&mut self) {
        self.accepted = None;
    }

    /// Returns fresh facts or the fail-closed unobserved state.
    pub fn effective(&self, now_cycle: u64) -> EffectiveSafety {
        let Some(accepted) = self.accepted else {
            return EffectiveSafety::UNOBSERVED;
        };
        let Some(age) = now_cycle.checked_sub(accepted.produced_at_cycle) else {
            return EffectiveSafety::UNOBSERVED;
        };
        if age > self.policy.maximum_age_cycles {
            return EffectiveSafety::UNOBSERVED;
        }
        EffectiveSafety {
            state: accepted.snapshot.state,
            fresh: true,
            realtime_job_active: accepted.snapshot.realtime_job_active(),
            maximum_lateness_cycles: accepted.snapshot.maximum_lateness_cycles,
        }
    }

    fn validate_observation(
        &self,
        frame_sequence: u32,
        produced_at_cycle: u64,
        observed_at_cycle: u64,
        snapshot: SafetySnapshot,
    ) -> Result<(), ObservationError> {
        snapshot.validate().map_err(ObservationError::Snapshot)?;
        if snapshot.safe_output_contract != self.policy.expected_contract {
            return Err(ObservationError::Contract {
                received: snapshot.safe_output_contract,
                expected: self.policy.expected_contract,
            });
        }
        let Some(age) = observed_at_cycle.checked_sub(produced_at_cycle) else {
            return Err(ObservationError::Future {
                produced_at_cycle,
                observed_at_cycle,
            });
        };
        if age > self.policy.maximum_age_cycles {
            return Err(ObservationError::Expired {
                age_cycles: age,
                maximum_age_cycles: self.policy.maximum_age_cycles,
            });
        }
        if frame_sequence == 0 {
            return Err(ObservationError::FrameSequence);
        }
        if let Some(previous) = self.accepted {
            if !serial_is_newer(frame_sequence, previous.frame_sequence) {
                return Err(ObservationError::FrameSequence);
            }
            let generation_changed =
                snapshot.transition_generation != previous.snapshot.transition_generation;
            if generation_changed
                && !serial_is_newer(
                    snapshot.transition_generation,
                    previous.snapshot.transition_generation,
                )
            {
                return Err(ObservationError::TransitionGeneration);
            }
            let safety_facts_changed = snapshot.state != previous.snapshot.state
                || snapshot.fault != previous.snapshot.fault
                || snapshot.flags != previous.snapshot.flags;
            if safety_facts_changed && !generation_changed {
                return Err(ObservationError::TransitionGeneration);
            }
        }
        Ok(())
    }
}

/// Rejected core-1 safety observation. Rejection revokes any prior observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationError {
    /// Payload decoding or cross-field validation failed.
    Snapshot(SnapshotError),
    /// Safe-output implementation identity did not match this board image.
    Contract {
        received: SafetyContractId,
        expected: SafetyContractId,
    },
    /// Producer timestamp was later than the observer timestamp.
    Future {
        produced_at_cycle: u64,
        observed_at_cycle: u64,
    },
    /// Publication was already too old when dequeued.
    Expired {
        age_cycles: u64,
        maximum_age_cycles: u64,
    },
    /// Frame sequence was zero, duplicate, old, or ambiguously far ahead.
    FrameSequence,
    /// State/fact transition generation was absent or non-monotonic.
    TransitionGeneration,
}

const fn serial_is_newer(candidate: u32, previous: u32) -> bool {
    let distance = candidate.wrapping_sub(previous);
    distance != 0 && distance < (1_u32 << 31)
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("fixed field"))
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("fixed field"))
}

/// Preconditions sampled and validated before a state transition.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Conditions {
    /// Board initialization has installed every safe output.
    pub safe_outputs_established: bool,
    /// Configuration has passed service- and real-time-side validation.
    pub configuration_valid: bool,
    /// Physical safety chain is closed and plausible.
    pub interlocks_closed: bool,
    /// Sufficient verified work is buffered for the requested operation.
    pub buffer_ready: bool,
    /// A local reset action satisfying machine policy has occurred.
    pub physical_reset_confirmed: bool,
}

/// Requested transition from the service boundary, job executor, or safety ISR.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Event {
    /// Finish boot-time safe-output establishment.
    Initialize,
    /// Install a completely validated real-time configuration; job authority is
    /// gated separately by durable selection.
    Configure,
    /// Remove the active configuration after its real-time resources are safe.
    Unconfigure,
    /// Enter the armed state.
    Arm,
    /// Begin scheduled work.
    Start,
    /// Request a controlled hold.
    Hold,
    /// Resume held work.
    Resume,
    /// Finish or cancel work after all outputs reached their declared terminal state.
    Finish,
    /// Disarm without executing work.
    Disarm,
    /// Latch an asynchronous fault.
    Fault(FaultCode),
    /// Reset a latched fault to the unconfigured safe state.
    ResetFault,
}

/// Deterministic state plus its retained fault reason.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SafetyMachine {
    state: SafetyState,
    fault: Option<FaultCode>,
}

impl SafetyMachine {
    /// Creates a machine in [`SafetyState::Boot`].
    pub const fn new() -> Self {
        Self {
            state: SafetyState::Boot,
            fault: None,
        }
    }

    /// Returns the current state.
    pub const fn state(self) -> SafetyState {
        self.state
    }

    /// Returns the retained fault reason, if any.
    pub const fn fault(self) -> Option<FaultCode> {
        self.fault
    }

    /// Applies one event atomically or leaves the machine unchanged on failure.
    pub fn apply(&mut self, event: Event, conditions: Conditions) -> Result<SafetyState, Error> {
        if let Event::Fault(code) = event {
            self.state = SafetyState::Fault;
            self.fault = Some(code);
            return Ok(self.state);
        }

        let next = match (self.state, event) {
            (SafetyState::Boot, Event::Initialize) if conditions.safe_outputs_established => {
                SafetyState::Safe
            }
            (SafetyState::Safe, Event::Configure) if conditions.configuration_valid => {
                SafetyState::Configured
            }
            (SafetyState::Configured, Event::Configure) if conditions.configuration_valid => {
                SafetyState::Configured
            }
            (SafetyState::Safe, Event::Unconfigure)
            | (SafetyState::Configured, Event::Unconfigure) => SafetyState::Safe,
            (SafetyState::Configured, Event::Arm)
                if conditions.configuration_valid
                    && conditions.interlocks_closed
                    && conditions.buffer_ready =>
            {
                SafetyState::Armed
            }
            (SafetyState::Armed, Event::Start)
                if conditions.configuration_valid
                    && conditions.interlocks_closed
                    && conditions.buffer_ready =>
            {
                SafetyState::Running
            }
            (SafetyState::Armed, Event::Disarm) => SafetyState::Configured,
            (SafetyState::Running, Event::Hold) => SafetyState::Hold,
            (SafetyState::Hold, Event::Resume)
                if conditions.configuration_valid
                    && conditions.interlocks_closed
                    && conditions.buffer_ready =>
            {
                SafetyState::Running
            }
            (SafetyState::Running | SafetyState::Hold, Event::Finish) => SafetyState::Configured,
            (SafetyState::Fault, Event::ResetFault)
                if conditions.safe_outputs_established && conditions.physical_reset_confirmed =>
            {
                self.fault = None;
                SafetyState::Safe
            }
            _ => {
                return Err(Error {
                    state: self.state,
                    event,
                });
            }
        };

        self.state = next;
        Ok(next)
    }
}

/// Rejected state transition. The state is unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Error {
    /// State in which the request was rejected.
    pub state: SafetyState,
    /// Rejected event.
    pub event: Event,
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTRACT: SafetyContractId = SafetyContractId(*b"test-safe-v00001");

    fn safe_snapshot(generation: u32) -> SafetySnapshot {
        SafetySnapshot {
            state: SafetyState::Safe,
            fault: None,
            flags: SNAPSHOT_FLAG_SAFE_OUTPUTS_ESTABLISHED,
            transition_generation: generation,
            safe_output_contract: CONTRACT,
            maximum_lateness_cycles: 17,
        }
    }

    fn observer(maximum_age_cycles: u64) -> SafetyObserver {
        SafetyObserver::new(SafetyObservationPolicy {
            expected_contract: CONTRACT,
            maximum_age_cycles,
        })
    }

    fn ready() -> Conditions {
        Conditions {
            safe_outputs_established: true,
            configuration_valid: true,
            interlocks_closed: true,
            buffer_ready: true,
            physical_reset_confirmed: true,
        }
    }

    fn running_machine() -> SafetyMachine {
        let mut machine = SafetyMachine::new();
        machine.apply(Event::Initialize, ready()).unwrap();
        machine.apply(Event::Configure, ready()).unwrap();
        machine.apply(Event::Arm, ready()).unwrap();
        machine.apply(Event::Start, ready()).unwrap();
        machine
    }

    fn input_spec(role: SafetyInputRole, resource: ResourceId) -> SafetyInputSpec {
        SafetyInputSpec {
            instance: 0,
            role,
            resource,
            polarity: InputPolarity::ActiveHigh,
            bias: InputBias::Floating,
            required_for_arm: role.conservatively_requires_clear_to_arm(),
            minimum_active_cycles: 0,
            minimum_inactive_cycles: 0,
            maximum_sample_gap_cycles: 10,
        }
    }

    #[test]
    fn safety_input_specs_require_finite_unique_fail_closed_routes() {
        assert!(
            core::mem::size_of::<SafetyInputMonitor<MAX_SAFETY_INPUTS>>() <= 2_048,
            "the full monitor must remain suitable for static core-1 ownership"
        );
        let emergency = SafetyInputSpec {
            polarity: InputPolarity::ActiveLow,
            minimum_inactive_cycles: 3,
            maximum_sample_gap_cycles: 5,
            ..input_spec(SafetyInputRole::EmergencyStop, ResourceId::Gpio(33))
        };
        assert!(SafetyInputMonitor::<2>::new(&[emergency]).is_ok());
        assert_eq!(
            SafetyInputMonitor::<2>::new(&[SafetyInputSpec {
                maximum_sample_gap_cycles: 0,
                ..emergency
            }])
            .err(),
            Some(SafetyInputError::Spec)
        );
        assert_eq!(
            SafetyInputMonitor::<2>::new(&[SafetyInputSpec {
                required_for_arm: false,
                ..emergency
            }])
            .err(),
            Some(SafetyInputError::Spec)
        );
        assert!(matches!(
            SafetyInputMonitor::<2>::new(&[
                emergency,
                SafetyInputSpec {
                    role: SafetyInputRole::SafetyInterlock,
                    ..emergency
                },
            ])
            .err(),
            Some(SafetyInputError::DuplicateResource(ResourceId::Gpio(33)))
        ));
        assert_eq!(
            SafetyInputMonitor::<2>::new(&[
                emergency,
                input_spec(SafetyInputRole::AxisLimitMinimum, ResourceId::Gpio(32)),
            ])
            .err(),
            Some(SafetyInputError::Order)
        );
    }

    #[test]
    fn exact_debounce_polarity_readiness_and_reactions_are_deterministic() {
        let emergency = SafetyInputSpec {
            polarity: InputPolarity::ActiveLow,
            minimum_inactive_cycles: 3,
            maximum_sample_gap_cycles: 5,
            ..input_spec(SafetyInputRole::EmergencyStop, ResourceId::Gpio(33))
        };
        let limit = SafetyInputSpec {
            instance: 1,
            minimum_active_cycles: 2,
            minimum_inactive_cycles: 1,
            maximum_sample_gap_cycles: 4,
            ..input_spec(SafetyInputRole::AxisLimitMinimum, ResourceId::Gpio(32))
        };
        let mut monitor = SafetyInputMonitor::<4>::new(&[emergency, limit]).unwrap();

        assert_eq!(monitor.observe(0, true, DeviceCycle(10)).unwrap(), None);
        assert_eq!(monitor.observe(0, true, DeviceCycle(12)).unwrap(), None);
        let clear = monitor.observe(0, true, DeviceCycle(13)).unwrap().unwrap();
        assert_eq!(clear.previous_active, None);
        assert_eq!(clear.conservative_reaction(), SafetyInputReaction::Clear);
        assert_eq!(monitor.observe(1, false, DeviceCycle(10)).unwrap(), None);
        let clear_limit = monitor.observe(1, false, DeviceCycle(11)).unwrap().unwrap();
        assert_eq!(clear_limit.generation, 2);

        let ready = monitor.status(DeviceCycle(13));
        assert!(ready.ready_to_arm());
        assert_eq!(ready.known_mask, 0b11);
        assert_eq!(ready.required_mask, 0b11);
        assert_eq!(ready.next_watchdog_deadline, Some(DeviceCycle(16)));

        assert_eq!(monitor.observe(1, true, DeviceCycle(14)).unwrap(), None);
        assert_eq!(monitor.observe(1, true, DeviceCycle(15)).unwrap(), None);
        let asserted_limit = monitor.observe(1, true, DeviceCycle(16)).unwrap().unwrap();
        assert_eq!(
            asserted_limit.conservative_reaction(),
            SafetyInputReaction::Fault(FaultCode::HardLimit)
        );
        assert!(!monitor.status(DeviceCycle(16)).ready_to_arm());

        let asserted_stop = monitor.observe(0, false, DeviceCycle(14)).unwrap().unwrap();
        assert_eq!(
            asserted_stop.conservative_reaction(),
            SafetyInputReaction::Fault(FaultCode::EmergencyStop)
        );
        assert!(!monitor.status(DeviceCycle(16)).ready_to_arm());
    }

    #[test]
    fn sampling_watchdog_fails_on_the_first_cycle_beyond_the_bound() {
        let spec = SafetyInputSpec {
            maximum_sample_gap_cycles: 4,
            ..input_spec(SafetyInputRole::SafetyInterlock, ResourceId::SafetyInput(0))
        };
        let mut monitor = SafetyInputMonitor::<1>::new(&[spec]).unwrap();
        monitor.observe(0, false, DeviceCycle(100)).unwrap();
        assert!(monitor.status(DeviceCycle(104)).ready_to_arm());
        assert_eq!(monitor.watchdog_fault(DeviceCycle(104)), None);
        assert_eq!(
            monitor.status(DeviceCycle(104)).next_watchdog_deadline,
            Some(DeviceCycle(105))
        );
        let fault = SafetyInputWatchdogFault {
            at: DeviceCycle(105),
            slot: 0,
            spec,
        };
        assert_eq!(monitor.watchdog_fault(DeviceCycle(105)), Some(fault));
        assert_eq!(
            fault.conservative_reaction(),
            SafetyInputReaction::Fault(FaultCode::Watchdog)
        );
        assert!(!monitor.status(DeviceCycle(105)).ready_to_arm());
        assert_eq!(
            monitor.observe(0, false, DeviceCycle(105)),
            Err(SafetyInputError::SampleGap {
                slot: 0,
                previous: DeviceCycle(100),
                observed: DeviceCycle(105),
                maximum_gap_cycles: 4,
            })
        );
        assert!(monitor.watchdog_fault(DeviceCycle(105)).is_some());
    }

    #[test]
    fn bounce_and_nonmonotonic_samples_cannot_create_false_transitions() {
        let spec = SafetyInputSpec {
            minimum_active_cycles: 3,
            ..input_spec(SafetyInputRole::SafetyInterlock, ResourceId::Gpio(22))
        };
        let mut monitor = SafetyInputMonitor::<1>::new(&[spec]).unwrap();
        let initial = monitor.observe(0, false, DeviceCycle(10)).unwrap().unwrap();
        assert_eq!(initial.generation, 1);
        assert_eq!(monitor.observe(0, true, DeviceCycle(11)).unwrap(), None);
        assert_eq!(monitor.observe(0, false, DeviceCycle(12)).unwrap(), None);
        assert_eq!(monitor.observe(0, true, DeviceCycle(13)).unwrap(), None);
        assert_eq!(monitor.observe(0, true, DeviceCycle(15)).unwrap(), None);
        let asserted = monitor.observe(0, true, DeviceCycle(16)).unwrap().unwrap();
        assert_eq!(asserted.generation, 2);
        assert_eq!(
            asserted.conservative_reaction(),
            SafetyInputReaction::Fault(FaultCode::SafetyInterlock)
        );
        let before = monitor.status(DeviceCycle(16));
        assert!(matches!(
            monitor.observe(0, false, DeviceCycle(16)),
            Err(SafetyInputError::TimeOrder { .. })
        ));
        assert_eq!(monitor.status(DeviceCycle(16)), before);
    }

    #[test]
    fn nominal_run_requires_every_ordered_gate() {
        let mut machine = running_machine();
        assert_eq!(machine.state(), SafetyState::Running);
        assert_eq!(machine.apply(Event::Hold, ready()), Ok(SafetyState::Hold));
        assert_eq!(
            machine.apply(Event::Resume, ready()),
            Ok(SafetyState::Running)
        );
        assert_eq!(
            machine.apply(Event::Finish, ready()),
            Ok(SafetyState::Configured)
        );
    }

    #[test]
    fn unconfigure_returns_only_quiescent_states_to_safe() {
        let mut machine = SafetyMachine::new();
        machine.apply(Event::Initialize, ready()).unwrap();
        assert_eq!(
            machine.apply(Event::Unconfigure, Conditions::default()),
            Ok(SafetyState::Safe)
        );
        machine.apply(Event::Configure, ready()).unwrap();
        assert_eq!(
            machine.apply(Event::Unconfigure, Conditions::default()),
            Ok(SafetyState::Safe)
        );

        let mut running = running_machine();
        assert_eq!(
            running.apply(Event::Unconfigure, Conditions::default()),
            Err(Error {
                state: SafetyState::Running,
                event: Event::Unconfigure,
            })
        );
    }

    #[test]
    fn arm_rejects_open_interlock_without_mutating_state() {
        let mut machine = SafetyMachine::new();
        machine.apply(Event::Initialize, ready()).unwrap();
        machine.apply(Event::Configure, ready()).unwrap();
        let mut conditions = ready();
        conditions.interlocks_closed = false;

        assert_eq!(
            machine.apply(Event::Arm, conditions),
            Err(Error {
                state: SafetyState::Configured,
                event: Event::Arm
            })
        );
        assert_eq!(machine.state(), SafetyState::Configured);
    }

    #[test]
    fn asynchronous_fault_latches_from_any_state() {
        for initial in [SafetyMachine::new(), running_machine()] {
            let mut machine = initial;
            assert_eq!(
                machine.apply(
                    Event::Fault(FaultCode::EmergencyStop),
                    Conditions::default()
                ),
                Ok(SafetyState::Fault)
            );
            assert_eq!(machine.fault(), Some(FaultCode::EmergencyStop));
        }
    }

    #[test]
    fn browser_style_reset_without_physical_confirmation_is_rejected() {
        let mut machine = running_machine();
        machine
            .apply(Event::Fault(FaultCode::Watchdog), Conditions::default())
            .unwrap();
        let mut conditions = ready();
        conditions.physical_reset_confirmed = false;

        assert!(machine.apply(Event::ResetFault, conditions).is_err());
        assert_eq!(machine.state(), SafetyState::Fault);
        assert_eq!(machine.fault(), Some(FaultCode::Watchdog));
    }

    #[test]
    fn snapshot_wire_encoding_is_exact_and_rejects_reserved_bytes() {
        let snapshot = safe_snapshot(3);
        let encoded = snapshot.encode().unwrap();
        assert_eq!(encoded.len(), SafetySnapshot::WIRE_LEN);
        assert_eq!(SafetySnapshot::decode(&encoded), Ok(snapshot));

        let mut malformed = encoded;
        malformed[39] = 1;
        assert_eq!(
            SafetySnapshot::decode(&malformed),
            Err(SnapshotError::Reserved)
        );
        assert!(matches!(
            SafetySnapshot::decode(&encoded[..encoded.len() - 1]),
            Err(SnapshotError::Length { .. })
        ));

        let mut interlock = safe_snapshot(4);
        interlock.state = SafetyState::Fault;
        interlock.fault = Some(FaultCode::SafetyInterlock);
        assert_eq!(
            SafetySnapshot::decode(&interlock.encode().unwrap()),
            Ok(interlock)
        );
    }

    #[test]
    fn inconsistent_fault_output_and_running_facts_are_rejected() {
        let mut snapshot = safe_snapshot(1);
        snapshot.state = SafetyState::Fault;
        assert_eq!(snapshot.validate(), Err(SnapshotError::StateInvariant));

        snapshot = safe_snapshot(1);
        snapshot.flags = 0;
        assert_eq!(snapshot.validate(), Err(SnapshotError::StateInvariant));

        snapshot = safe_snapshot(1);
        snapshot.state = SafetyState::Running;
        assert_eq!(snapshot.validate(), Err(SnapshotError::StateInvariant));
    }

    #[test]
    fn fresh_observation_expires_to_fail_closed_boot_and_busy_cache() {
        let mut observer = observer(500);
        observer.observe(1, 1_000, 1_100, safe_snapshot(1)).unwrap();
        assert_eq!(
            observer.effective(1_500),
            EffectiveSafety {
                state: SafetyState::Safe,
                fresh: true,
                realtime_job_active: false,
                maximum_lateness_cycles: 17,
            }
        );
        assert_eq!(observer.effective(1_501), EffectiveSafety::UNOBSERVED);
    }

    #[test]
    fn malformed_wrong_contract_and_old_frames_revoke_prior_safety() {
        let mut observer = observer(500);
        observer.observe(1, 1_000, 1_000, safe_snapshot(1)).unwrap();

        let mut wrong = safe_snapshot(1);
        wrong.safe_output_contract = SafetyContractId(*b"wrong-safe-v0001");
        assert!(matches!(
            observer.observe(2, 1_001, 1_001, wrong),
            Err(ObservationError::Contract { .. })
        ));
        assert_eq!(observer.effective(1_001), EffectiveSafety::UNOBSERVED);

        observer.observe(3, 1_002, 1_002, safe_snapshot(1)).unwrap();
        assert_eq!(
            observer.observe(3, 1_003, 1_003, safe_snapshot(1)),
            Err(ObservationError::FrameSequence)
        );
        assert_eq!(observer.effective(1_003), EffectiveSafety::UNOBSERVED);

        observer.observe(4, 1_004, 1_004, safe_snapshot(1)).unwrap();
        assert!(matches!(
            observer.observe_encoded(5, 1_005, 1_005, &[0; 3]),
            Err(ObservationError::Snapshot(SnapshotError::Length { .. }))
        ));
        assert_eq!(observer.effective(1_005), EffectiveSafety::UNOBSERVED);
    }

    #[test]
    fn changed_safety_facts_require_a_new_transition_generation() {
        let mut observer = observer(500);
        observer.observe(1, 10, 10, safe_snapshot(7)).unwrap();
        let mut fault = safe_snapshot(7);
        fault.state = SafetyState::Fault;
        fault.fault = Some(FaultCode::EmergencyStop);
        assert_eq!(
            observer.observe(2, 11, 11, fault),
            Err(ObservationError::TransitionGeneration)
        );

        observer.observe(3, 12, 12, safe_snapshot(7)).unwrap();
        fault.transition_generation = 8;
        assert_eq!(observer.observe(4, 13, 13, fault), Ok(()));
        assert_eq!(observer.effective(13).state, SafetyState::Fault);
    }

    #[test]
    fn sequence_wrap_is_admitted_but_ambiguous_jump_is_rejected() {
        let mut observer = observer(500);
        observer
            .observe(u32::MAX, 10, 10, safe_snapshot(u32::MAX))
            .unwrap();
        let mut wrapped = safe_snapshot(1);
        wrapped.maximum_lateness_cycles = 18;
        assert_eq!(observer.observe(1, 11, 11, wrapped), Ok(()));

        assert_eq!(
            observer.observe(1_u32.wrapping_add(1 << 31), 12, 12, wrapped),
            Err(ObservationError::FrameSequence)
        );
    }
}
