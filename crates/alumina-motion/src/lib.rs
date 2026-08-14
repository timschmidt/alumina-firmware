#![no_std]
#![doc = "Allocation-free exact integer event execution for Alumina motion streams."]

use alumina_board::ResourceId;
use alumina_config::{
    AxisDriverControl, ConfigurationIdentity, RealtimeConfigurationProfile, SignalPolarity,
    StepperAxisProfile,
};
use alumina_job::{AdmittedBlock, REALTIME_BLOCK_WINDOW};
use alumina_machine_ir::{
    BlockError, ExecutionSegment, FINITE_DIFFERENCE_ONE_STEP, FiniteDifferenceAxis,
    FiniteDifferenceError, FiniteDifferenceSegment, FiniteDifferenceSegmentSummary,
    FiniteDifferenceValidationLimits, MAX_EXECUTION_AXES, StreamTick,
};
use alumina_protocol::DeviceCycle;

/// Maximum number of axes representable by the fixed logical event mask.
pub const MAX_STEPPER_AXES: usize = MAX_EXECUTION_AXES;
const _: () = assert!(MAX_STEPPER_AXES == alumina_config::MAX_EXECUTABLE_STEPPER_AXES);
/// Exact fixed core-1 motion-status bytes.
pub const REALTIME_MOTION_REPORT_BYTES: usize = 128;

const MOTION_REPORT_MAGIC: [u8; 8] = *b"ALMMOT01";
const MOTION_REPORT_VERSION: u16 = 1;
const MOTION_REPORT_FLAG_NEXT_DEADLINE: u16 = 1 << 0;

/// A fixed-width logical-axis set. Bits at and above the selected axis width
/// are always rejected or left clear.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct AxisMask(u16);

impl AxisMask {
    /// Empty set.
    pub const NONE: Self = Self(0);

    /// Raw mask for diagnostics and board-backend translation.
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Whether no logical axis is selected.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether this set contains one logical axis.
    pub const fn contains(self, axis: usize) -> bool {
        axis < u16::BITS as usize && self.0 & (1_u16 << axis) != 0
    }

    /// Constructs a mask only when every set bit names an axis in the declared
    /// logical width.
    pub fn from_bits(bits: u16, axis_count: usize) -> Result<Self, MotionError> {
        if axis_count == 0 || axis_count > MAX_STEPPER_AXES {
            return Err(MotionError::AxisCount);
        }
        let allowed = (1_u16 << axis_count) - 1;
        if bits & !allowed != 0 {
            return Err(MotionError::AxisCount);
        }
        Ok(Self(bits))
    }

    fn insert(&mut self, axis: usize) -> Result<(), MotionError> {
        if axis >= MAX_STEPPER_AXES || axis >= u16::BITS as usize {
            return Err(MotionError::AxisCount);
        }
        self.0 |= 1_u16 << axis;
        Ok(())
    }

    fn remove(&mut self, axis: usize) -> Result<(), MotionError> {
        if axis >= MAX_STEPPER_AXES || axis >= u16::BITS as usize {
            return Err(MotionError::AxisCount);
        }
        self.0 &= !(1_u16 << axis);
        Ok(())
    }

    fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Electrical timing requirements for one logical step/direction axis.
///
/// Values are expressed in the same device-cycle domain used by the compiled
/// stream. Board backends must prove that domain maps to their hardware timer
/// or serializer without weakening these bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AxisTiming {
    /// Minimum time for which a step output remains asserted.
    pub pulse_high_cycles: u32,
    /// Minimum inactive time between consecutive pulses.
    pub pulse_low_cycles: u32,
    /// Required stable direction time before the next rising step edge.
    pub direction_setup_cycles: u32,
    /// Required stable direction time after the preceding falling step edge.
    pub direction_hold_cycles: u32,
    /// Required enabled time before the first rising step edge.
    pub enable_setup_cycles: u32,
    /// Required enabled time after the final falling step edge.
    pub enable_hold_cycles: u32,
    /// Configuration-qualified maximum rising-edge frequency.
    pub maximum_step_frequency_hz: u32,
}

impl AxisTiming {
    /// A conservative portable fixture, not a board qualification claim.
    pub const TEST_1_CYCLE: Self = Self {
        pulse_high_cycles: 1,
        pulse_low_cycles: 1,
        direction_setup_cycles: 1,
        direction_hold_cycles: 1,
        enable_setup_cycles: 1,
        enable_hold_cycles: 1,
        maximum_step_frequency_hz: 100_000,
    };

    fn validate(self) -> Result<(), MotionError> {
        if self.pulse_high_cycles == 0
            || self.pulse_low_cycles == 0
            || self.maximum_step_frequency_hz == 0
        {
            return Err(MotionError::Timing);
        }
        u64::from(self.pulse_high_cycles)
            .checked_add(u64::from(self.pulse_low_cycles))
            .ok_or(MotionError::Arithmetic)?;
        Ok(())
    }
}

/// Complete executor timing policy, independently derived from the active
/// configuration and qualified board backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StepperTiming<const AXES: usize> {
    pub axes: [AxisTiming; AXES],
    /// Frequency of the `DeviceCycle` domain used by every stream tick.
    pub device_cycle_hz: u64,
    /// Smallest physically addressable output interval. Every ordinary
    /// direction, enable, and step edge must lie on this cycle lattice.
    pub output_quantum_cycles: u32,
    /// Maximum time after a scheduled edge at which the backend may still
    /// apply it. Exceeding this value faults before returning that edge.
    pub maximum_lateness_cycles: u32,
}

impl<const AXES: usize> StepperTiming<AXES> {
    /// Validates the fixed axis width and every electrical timing bound.
    pub fn validate(self) -> Result<(), MotionError> {
        validate_axis_count::<AXES>()?;
        if self.device_cycle_hz == 0 || self.output_quantum_cycles == 0 {
            return Err(MotionError::Timing);
        }
        for timing in self.axes {
            timing.validate()?;
            if !timing
                .pulse_high_cycles
                .is_multiple_of(self.output_quantum_cycles)
            {
                return Err(MotionError::Timing);
            }
        }
        Ok(())
    }
}

/// Exact active-configuration routing plus the timing policy consumed by one
/// step/direction executor. Backends translate only these resource IDs; numeric
/// GPIO-like aliases never enter the machine stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StepperExecutionProfile<const AXES: usize> {
    pub axes: [StepperAxisProfile; AXES],
    pub timing: StepperTiming<AXES>,
}

impl<const AXES: usize> StepperExecutionProfile<AXES> {
    /// Derives a dense initial stepper profile from an independently validated
    /// configuration identity. Mixed stepper/FOC execution is deliberately not
    /// accepted by this first step-only executor.
    pub fn from_configuration(
        identity: &ConfigurationIdentity,
        realtime_profile: &RealtimeConfigurationProfile,
        device_cycle_hz: u64,
        output_quantum_cycles: u32,
        maximum_lateness_cycles: u32,
    ) -> Result<Self, MotionError> {
        validate_axis_count::<AXES>()?;
        if realtime_profile.timer_tick_hertz() != Some(device_cycle_hz)
            || realtime_profile.stepper_output_quantum_cycles() != Some(output_quantum_cycles)
        {
            return Err(MotionError::Timing);
        }
        if identity.summary.stepper_axes as usize != AXES || identity.summary.foc_axes != 0 {
            return Err(MotionError::ConfigurationAxisCount {
                configured: identity.summary.stepper_axes,
                expected: AXES,
            });
        }
        let first = realtime_profile
            .stepper_axis(0)
            .ok_or(MotionError::ConfigurationAxisLayout { axis: 0 })?;
        let mut profiles = [first; AXES];
        let mut timing = [AxisTiming::TEST_1_CYCLE; AXES];
        let mut axis = 0;
        while axis < AXES {
            let profile = realtime_profile
                .stepper_axis(axis)
                .ok_or(MotionError::ConfigurationAxisLayout { axis })?;
            profiles[axis] = profile;
            timing[axis] = AxisTiming {
                pulse_high_cycles: profile.step.minimum_active_cycles,
                pulse_low_cycles: profile.step.minimum_inactive_cycles,
                direction_setup_cycles: profile.direction.minimum_active_cycles,
                direction_hold_cycles: profile.direction.minimum_inactive_cycles,
                enable_setup_cycles: profile.driver_control.minimum_active_cycles,
                enable_hold_cycles: profile.driver_control.minimum_inactive_cycles,
                maximum_step_frequency_hz: profile.step.maximum_frequency_hz,
            };
            axis += 1;
        }
        while axis < alumina_config::MAX_EXECUTABLE_STEPPER_AXES {
            if realtime_profile.stepper_axis(axis).is_some() {
                return Err(MotionError::ConfigurationAxisLayout { axis });
            }
            axis += 1;
        }
        let timing = StepperTiming {
            axes: timing,
            device_cycle_hz,
            output_quantum_cycles,
            maximum_lateness_cycles,
        };
        timing.validate()?;
        Ok(Self {
            axes: profiles,
            timing,
        })
    }
}

/// Logical output changes that must be applied atomically at one device cycle.
///
/// A backend applies fields in this semantic order: lower completed step
/// pulses, update direction, update enable, then raise new step pulses. The
/// validator prevents one axis from requiring incompatible changes together.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StepperEvent {
    /// Exact planned cycle, independent of when software happened to poll it.
    pub at: DeviceCycle,
    /// Axes whose direction output changes at this cycle.
    pub direction_change: AxisMask,
    /// Positive-direction levels after applying `direction_change`.
    pub direction_positive: AxisMask,
    /// Axes whose driver enable becomes active.
    pub enable: AxisMask,
    /// Axes whose driver enable becomes inactive.
    pub disable: AxisMask,
    /// Step outputs raised at this cycle.
    pub step_high: AxisMask,
    /// Step outputs lowered at this cycle.
    pub step_low: AxisMask,
}

impl StepperEvent {
    /// Whether this event changes no physical logical output.
    pub const fn is_empty(self) -> bool {
        self.direction_change.is_empty()
            && self.enable.is_empty()
            && self.disable.is_empty()
            && self.step_high.is_empty()
            && self.step_low.is_empty()
    }
}

/// Physical complete-image contract for one serialized shift-register engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShiftImageContract {
    pub engine: u8,
    pub width: u8,
    pub defined_mask: u32,
    pub safe_image: u32,
}

/// One exact complete-image update after translating a logical event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShiftImageUpdate {
    pub at: DeviceCycle,
    pub image: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ShiftAxisRoute {
    step_bit: u8,
    direction_bit: u8,
    control_bit: u8,
    step_polarity: SignalPolarity,
    direction_polarity: SignalPolarity,
    control_polarity: SignalPolarity,
    control_action: AxisDriverControl,
}

/// Stateful translator from logical step events to complete serialized output
/// images. Every update preserves unrelated bits and is checked against the
/// same full-width safety image established before Wi-Fi starts.
pub struct ShiftImageMapper<const AXES: usize> {
    contract: ShiftImageContract,
    routes: [ShiftAxisRoute; AXES],
    image: u32,
}

impl<const AXES: usize> ShiftImageMapper<AXES> {
    /// Binds a validated dense stepper profile to one complete-image engine.
    pub fn new(
        profile: &StepperExecutionProfile<AXES>,
        contract: ShiftImageContract,
    ) -> Result<Self, ShiftImageError> {
        if AXES == 0 || AXES > MAX_STEPPER_AXES || contract.width == 0 || contract.width > 32 {
            return Err(ShiftImageError::Width);
        }
        let complete_mask = if contract.width == 32 {
            u32::MAX
        } else {
            (1_u32 << contract.width) - 1
        };
        if contract.defined_mask != complete_mask || contract.safe_image & !complete_mask != 0 {
            return Err(ShiftImageError::IncompleteImage);
        }

        let placeholder = ShiftAxisRoute {
            step_bit: 0,
            direction_bit: 0,
            control_bit: 0,
            step_polarity: SignalPolarity::ActiveHigh,
            direction_polarity: SignalPolarity::ActiveHigh,
            control_polarity: SignalPolarity::ActiveHigh,
            control_action: AxisDriverControl::Disable,
        };
        let mut routes = [placeholder; AXES];
        let mut claimed = 0_u32;
        let mut axis = 0;
        while axis < AXES {
            let configured = profile.axes[axis];
            if usize::from(configured.instance) != axis {
                return Err(ShiftImageError::AxisLayout { axis });
            }
            let step_bit = shift_bit(configured.step.resource, contract.engine, contract.width)?;
            let direction_bit = shift_bit(
                configured.direction.resource,
                contract.engine,
                contract.width,
            )?;
            let control_bit = shift_bit(
                configured.driver_control.resource,
                contract.engine,
                contract.width,
            )?;
            for bit in [step_bit, direction_bit, control_bit] {
                let mask = 1_u32 << bit;
                if claimed & mask != 0 {
                    return Err(ShiftImageError::DuplicateBit { bit });
                }
                claimed |= mask;
            }
            let route = ShiftAxisRoute {
                step_bit,
                direction_bit,
                control_bit,
                step_polarity: signal_polarity(configured.step.polarity)?,
                direction_polarity: signal_polarity(configured.direction.polarity)?,
                control_polarity: signal_polarity(configured.driver_control.polarity)?,
                control_action: configured.driver_control_action,
            };
            if bit_is_active(contract.safe_image, route.step_bit, route.step_polarity)
                || logical_driver_enabled(contract.safe_image, route)
            {
                return Err(ShiftImageError::UnsafeImage { axis });
            }
            routes[axis] = route;
            axis += 1;
        }
        Ok(Self {
            contract,
            routes,
            image: contract.safe_image,
        })
    }

    /// Applies one already validated logical transaction to the retained
    /// complete image. Conflicting or out-of-width masks change no state.
    pub fn apply(&mut self, event: StepperEvent) -> Result<ShiftImageUpdate, ShiftImageError> {
        let allowed = (1_u16 << AXES) - 1;
        let fields = event.direction_change.bits()
            | event.direction_positive.bits()
            | event.enable.bits()
            | event.disable.bits()
            | event.step_high.bits()
            | event.step_low.bits();
        if fields & !allowed != 0
            || event.enable.bits() & event.disable.bits() != 0
            || event.step_high.bits() & event.step_low.bits() != 0
        {
            return Err(ShiftImageError::EventMask);
        }
        let mut image = self.image;
        let mut axis = 0;
        while axis < AXES {
            let route = self.routes[axis];
            let step_was_active = bit_is_active(image, route.step_bit, route.step_polarity);
            if event.step_low.contains(axis) != step_was_active
                && (event.step_low.contains(axis) || event.step_high.contains(axis))
            {
                return Err(ShiftImageError::EventState { axis });
            }
            if event.step_low.contains(axis) {
                set_active(&mut image, route.step_bit, route.step_polarity, false);
            }
            if event.direction_change.contains(axis) {
                set_active(
                    &mut image,
                    route.direction_bit,
                    route.direction_polarity,
                    event.direction_positive.contains(axis),
                );
            }
            if event.enable.contains(axis) {
                set_active(
                    &mut image,
                    route.control_bit,
                    route.control_polarity,
                    route.control_action == AxisDriverControl::Enable,
                );
            }
            if event.disable.contains(axis) {
                set_active(
                    &mut image,
                    route.control_bit,
                    route.control_polarity,
                    route.control_action == AxisDriverControl::Disable,
                );
            }
            if event.step_high.contains(axis) {
                set_active(&mut image, route.step_bit, route.step_polarity, true);
            }
            if bit_is_active(image, route.step_bit, route.step_polarity)
                && !logical_driver_enabled(image, route)
            {
                return Err(ShiftImageError::EventState { axis });
            }
            axis += 1;
        }
        if image & !self.contract.defined_mask != 0 {
            return Err(ShiftImageError::IncompleteImage);
        }
        self.image = image;
        Ok(ShiftImageUpdate {
            at: event.at,
            image,
        })
    }

    /// Restores the complete fail-safe image immediately and locally.
    pub fn force_safe(&mut self, at: DeviceCycle) -> ShiftImageUpdate {
        self.image = self.contract.safe_image;
        ShiftImageUpdate {
            at,
            image: self.image,
        }
    }

    pub const fn image(&self) -> u32 {
        self.image
    }
}

/// Complete-image construction or event-translation rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShiftImageError {
    Width,
    IncompleteImage,
    WrongEngine { expected: u8, received: u8 },
    Resource,
    Polarity,
    AxisLayout { axis: usize },
    DuplicateBit { bit: u8 },
    UnsafeImage { axis: usize },
    EventMask,
    EventState { axis: usize },
}

/// Exact completion facts for one contiguous integer segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SegmentCompletion<const AXES: usize> {
    pub at: DeviceCycle,
    pub end_tick: StreamTick,
    pub delta_steps: [i64; AXES],
    pub position: [i64; AXES],
    /// Conservative exact timing-quantization bound. A value of one denotes
    /// one half of a device tick; zero denotes a segment with no step edges.
    pub maximum_half_tick_error: u32,
}

/// Allocation-free result of replaying a complete canonical segment sequence
/// through the same electrical validator used by the live executor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StepperPreflightSummary<const AXES: usize> {
    /// Exclusive terminal stream tick.
    pub end_tick: StreamTick,
    /// Absolute terminal step-lattice position.
    pub position: [i64; AXES],
    /// Number of rising step edges on every axis.
    pub emitted_steps: [u64; AXES],
    /// Number of admitted canonical segments.
    pub segment_count: u32,
    /// Earliest aligned cycle at which normal driver disable is legal.
    pub earliest_finish_cycle: DeviceCycle,
}

/// Configuration/backend bounds for direct third-order finite-difference IR.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteDifferenceExecutionLimits {
    /// Longest one-record ownership horizon in device ticks.
    pub maximum_segment_ticks: u64,
    /// Largest dense update count accepted in one record.
    pub maximum_update_count: u32,
    /// Largest rounded displacement on one axis in one record.
    pub maximum_steps_per_segment: u64,
}

/// Allocation-free replay result for a direct finite-difference segment stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteDifferencePreflightSummary<const AXES: usize> {
    /// Exclusive terminal stream tick.
    pub end_tick: StreamTick,
    /// Absolute terminal integer command-lattice position.
    pub position: [i64; AXES],
    /// Exact terminal relative Q31.32 coordinate carried across records.
    pub terminal_finite_position: [i64; AXES],
    /// Number of rising step edges on every axis.
    pub emitted_steps: [u64; AXES],
    /// Number of admitted direct finite-difference records.
    pub segment_count: u32,
    /// Total dense update frames represented without iterating them.
    pub update_count: u64,
    /// Earliest aligned cycle at which normal driver disable is legal.
    pub earliest_finish_cycle: DeviceCycle,
    /// Exact nearest-lattice projection bound in Q31.32 step units.
    pub maximum_position_error: u64,
}

/// Direct finite-difference stream rejection, preserving either the exact
/// integer-IR failure or the production electrical failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FiniteDifferencePreflightError {
    /// No records were supplied.
    Empty,
    /// Record-local canonical/bounded validation failed.
    Segment {
        /// Zero-based record index.
        index: usize,
        /// Exact finite-difference failure.
        error: FiniteDifferenceError,
    },
    /// Existing production electrical timing rejected the reconstructed edges.
    Timing(MotionError),
    /// Absolute machine position overflowed while applying one record.
    PositionOverflow {
        /// Zero-based record index.
        index: usize,
        /// Axis index.
        axis: usize,
    },
    /// Checked counters, ticks, or fixed-width conversions overflowed.
    Arithmetic,
}

impl FiniteDifferencePreflightError {
    /// Whether a longer exact schedule may address this rejection.
    ///
    /// Structural, coefficient, identity, continuity, and arithmetic failures
    /// are never reclassified as duration pressure.
    pub const fn is_time_dilation_candidate(self) -> bool {
        match self {
            Self::Timing(error) => error.is_time_dilation_candidate(),
            Self::Segment {
                error: FiniteDifferenceError::UpdateRate { .. },
                ..
            } => true,
            _ => false,
        }
    }
}

/// Nonblocking result from checking the next exact event deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionPoll<const AXES: usize> {
    /// No segment is currently installed.
    Idle,
    /// Nothing may be emitted until this exact cycle.
    Future { at: DeviceCycle },
    /// One due logical output transaction and its observed software lateness.
    Event {
        event: StepperEvent,
        lateness_cycles: u32,
    },
    /// The segment horizon was reached with every pulse returned low.
    SegmentComplete(SegmentCompletion<AXES>),
}

/// Lifecycle of one allocation-free step/direction executor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ExecutorState {
    Idle = 0,
    Ready = 1,
    Segment = 2,
    Complete = 3,
    Faulted = 4,
}

impl ExecutorState {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Idle),
            1 => Some(Self::Ready),
            2 => Some(Self::Segment),
            3 => Some(Self::Complete),
            4 => Some(Self::Faulted),
            _ => None,
        }
    }
}

/// Bounded status suitable for simulator assertions and later telemetry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StepperStatus<const AXES: usize> {
    pub state: ExecutorState,
    pub epoch: DeviceCycle,
    pub next_tick: StreamTick,
    pub position: [i64; AXES],
    pub direction_known: AxisMask,
    pub direction_positive: AxisMask,
    pub enabled: AxisMask,
    pub step_high: AxisMask,
    pub completed_segments: u32,
    pub emitted_steps: [u64; AXES],
    pub maximum_lateness_cycles: u32,
    pub deadline_misses: u64,
}

/// Fixed, canonical real-time execution report. Position slots above
/// `axis_count` are required zero so the same wire shape serves every board.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeMotionReport {
    pub state: ExecutorState,
    pub axis_count: u8,
    pub epoch: DeviceCycle,
    pub next_tick: StreamTick,
    pub next_deadline: Option<DeviceCycle>,
    pub completed_segments: u32,
    pub maximum_lateness_cycles: u32,
    pub deadline_misses: u64,
    pub enabled: AxisMask,
    pub step_high: AxisMask,
    pub direction_known: AxisMask,
    pub direction_positive: AxisMask,
    pub position: [i64; MAX_STEPPER_AXES],
}

impl RealtimeMotionReport {
    /// Takes a bounded snapshot without changing executor state.
    pub fn from_executor<const AXES: usize>(
        executor: &StepperExecutor<AXES>,
    ) -> Result<Self, MotionReportError> {
        validate_axis_count::<AXES>().map_err(|_| MotionReportError::AxisCount)?;
        let status = executor.status();
        let mut position = [0_i64; MAX_STEPPER_AXES];
        position[..AXES].copy_from_slice(&status.position);
        let report = Self {
            state: status.state,
            axis_count: u8::try_from(AXES).map_err(|_| MotionReportError::AxisCount)?,
            epoch: status.epoch,
            next_tick: status.next_tick,
            next_deadline: executor.next_deadline(),
            completed_segments: status.completed_segments,
            maximum_lateness_cycles: status.maximum_lateness_cycles,
            deadline_misses: status.deadline_misses,
            enabled: status.enabled,
            step_high: status.step_high,
            direction_known: status.direction_known,
            direction_positive: status.direction_positive,
            position,
        };
        report.validate()?;
        Ok(report)
    }

    pub fn encode(self) -> Result<[u8; REALTIME_MOTION_REPORT_BYTES], MotionReportError> {
        self.validate()?;
        let mut encoded = [0_u8; REALTIME_MOTION_REPORT_BYTES];
        encoded[0..8].copy_from_slice(&MOTION_REPORT_MAGIC);
        encoded[8..10].copy_from_slice(&MOTION_REPORT_VERSION.to_le_bytes());
        encoded[10] = self.state as u8;
        encoded[11] = self.axis_count;
        if let Some(deadline) = self.next_deadline {
            encoded[12..14].copy_from_slice(&MOTION_REPORT_FLAG_NEXT_DEADLINE.to_le_bytes());
            encoded[32..40].copy_from_slice(&deadline.0.to_le_bytes());
        }
        // Bytes 14..16 are reserved zero.
        encoded[16..24].copy_from_slice(&self.epoch.0.to_le_bytes());
        encoded[24..32].copy_from_slice(&self.next_tick.0.to_le_bytes());
        encoded[40..44].copy_from_slice(&self.completed_segments.to_le_bytes());
        encoded[44..48].copy_from_slice(&self.maximum_lateness_cycles.to_le_bytes());
        encoded[48..56].copy_from_slice(&self.deadline_misses.to_le_bytes());
        encoded[56..58].copy_from_slice(&self.enabled.bits().to_le_bytes());
        encoded[58..60].copy_from_slice(&self.step_high.bits().to_le_bytes());
        encoded[60..62].copy_from_slice(&self.direction_known.bits().to_le_bytes());
        encoded[62..64].copy_from_slice(&self.direction_positive.bits().to_le_bytes());
        for (axis, position) in self.position.into_iter().enumerate() {
            let offset = 64 + axis * 8;
            encoded[offset..offset + 8].copy_from_slice(&position.to_le_bytes());
        }
        Ok(encoded)
    }

    pub fn decode(encoded: &[u8]) -> Result<Self, MotionReportError> {
        if encoded.len() != REALTIME_MOTION_REPORT_BYTES {
            return Err(MotionReportError::Length);
        }
        if encoded[0..8] != MOTION_REPORT_MAGIC {
            return Err(MotionReportError::Magic);
        }
        if read_u16(encoded, 8) != MOTION_REPORT_VERSION {
            return Err(MotionReportError::Version);
        }
        let flags = read_u16(encoded, 12);
        if flags & !MOTION_REPORT_FLAG_NEXT_DEADLINE != 0
            || encoded[14..16].iter().any(|byte| *byte != 0)
        {
            return Err(MotionReportError::Reserved);
        }
        let mut position = [0_i64; MAX_STEPPER_AXES];
        for (axis, value) in position.iter_mut().enumerate() {
            *value = read_i64(encoded, 64 + axis * 8);
        }
        let report = Self {
            state: ExecutorState::from_wire(encoded[10]).ok_or(MotionReportError::State)?,
            axis_count: encoded[11],
            epoch: DeviceCycle(read_u64(encoded, 16)),
            next_tick: StreamTick(read_u64(encoded, 24)),
            next_deadline: if flags & MOTION_REPORT_FLAG_NEXT_DEADLINE != 0 {
                Some(DeviceCycle(read_u64(encoded, 32)))
            } else {
                if read_u64(encoded, 32) != 0 {
                    return Err(MotionReportError::Reserved);
                }
                None
            },
            completed_segments: read_u32(encoded, 40),
            maximum_lateness_cycles: read_u32(encoded, 44),
            deadline_misses: read_u64(encoded, 48),
            enabled: AxisMask(read_u16(encoded, 56)),
            step_high: AxisMask(read_u16(encoded, 58)),
            direction_known: AxisMask(read_u16(encoded, 60)),
            direction_positive: AxisMask(read_u16(encoded, 62)),
            position,
        };
        report.validate()?;
        if report.encode()? != encoded {
            return Err(MotionReportError::Noncanonical);
        }
        Ok(report)
    }

    fn validate(self) -> Result<(), MotionReportError> {
        let axes = usize::from(self.axis_count);
        if axes == 0 || axes > MAX_STEPPER_AXES {
            return Err(MotionReportError::AxisCount);
        }
        let allowed = (1_u16 << axes) - 1;
        if (self.enabled.bits()
            | self.step_high.bits()
            | self.direction_known.bits()
            | self.direction_positive.bits())
            & !allowed
            != 0
            || self.step_high.bits() & !self.enabled.bits() != 0
            || self.direction_positive.bits() & !self.direction_known.bits() != 0
        {
            return Err(MotionReportError::Mask);
        }
        if self.position[axes..].iter().any(|position| *position != 0) {
            return Err(MotionReportError::Position);
        }
        if (self.state == ExecutorState::Segment) != self.next_deadline.is_some() {
            return Err(MotionReportError::State);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionReportError {
    Length,
    Magic,
    Version,
    Reserved,
    State,
    AxisCount,
    Mask,
    Position,
    Noncanonical,
}

/// Fail-closed scheduler rejection. Segment validation never partially mutates
/// executor state and deadline failure never returns the late edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionError {
    AxisCount,
    Timing,
    State,
    SegmentFlags,
    SegmentOrder,
    EmptySegment,
    PositionOverflow {
        axis: usize,
    },
    EpochOverflow,
    Arithmetic,
    /// A normal output cycle was outside the backend's exact timing lattice.
    OutputGrid {
        cycle: u64,
        quantum_cycles: u32,
    },
    Rate {
        axis: usize,
    },
    PulseBoundary {
        axis: usize,
    },
    PulseLow {
        axis: usize,
    },
    DirectionSetup {
        axis: usize,
    },
    DirectionHold {
        axis: usize,
    },
    EnableSetup {
        axis: usize,
    },
    EnableHold {
        axis: usize,
    },
    ConfigurationAxisCount {
        configured: u8,
        expected: usize,
    },
    ConfigurationAxisLayout {
        axis: usize,
    },
    Deadline {
        scheduled: DeviceCycle,
        observed: DeviceCycle,
        maximum_lateness_cycles: u32,
    },
    OutputInvariant,
}

impl MotionError {
    /// Whether increasing one or more segment durations can address this
    /// failure without changing coordinates, stream topology, or electrical
    /// policy.
    ///
    /// This is a classification of timing pressure, not permission to accept
    /// the stream. A planner must construct a new exact schedule and replay it
    /// through the complete production validator. Structural, identity,
    /// arithmetic, grid, state, and deadline failures are never reclassified
    /// as time-dilation candidates.
    pub const fn is_time_dilation_candidate(self) -> bool {
        matches!(
            self,
            Self::Rate { .. }
                | Self::PulseBoundary { .. }
                | Self::PulseLow { .. }
                | Self::DirectionSetup { .. }
                | Self::DirectionHold { .. }
                | Self::EnableSetup { .. }
                | Self::EnableHold { .. }
        )
    }
}

#[derive(Clone, Copy)]
struct ActiveSegment<const AXES: usize> {
    segment: ExecutionSegment<AXES>,
    start: DeviceCycle,
    end: DeviceCycle,
    steps: [u64; AXES],
    next_index: [u64; AXES],
    next_rise: [Option<DeviceCycle>; AXES],
    next_fall: [Option<DeviceCycle>; AXES],
    direction_change: AxisMask,
    direction_positive: AxisMask,
    enable: AxisMask,
    boundary_pending: bool,
}

impl<const AXES: usize> ActiveSegment<AXES> {
    fn next_output_cycle(self) -> Option<DeviceCycle> {
        let mut next = if self.boundary_pending {
            Some(self.start)
        } else {
            None
        };
        let mut axis = 0;
        while axis < AXES {
            next = minimum_cycle(next, self.next_rise[axis]);
            next = minimum_cycle(next, self.next_fall[axis]);
            axis += 1;
        }
        next
    }
}

/// Deterministic N-axis interpolator for independently validated machine-IR
/// segments. It owns no pins or timers and cannot energize hardware by itself.
pub struct StepperExecutor<const AXES: usize> {
    timing: StepperTiming<AXES>,
    state: ExecutorState,
    epoch: DeviceCycle,
    next_tick: StreamTick,
    position: [i64; AXES],
    direction_known: AxisMask,
    direction_positive: AxisMask,
    enabled: AxisMask,
    step_high: AxisMask,
    last_fall: [Option<DeviceCycle>; AXES],
    last_rise: [Option<DeviceCycle>; AXES],
    active: Option<ActiveSegment<AXES>>,
    completed_segments: u32,
    emitted_steps: [u64; AXES],
    maximum_lateness_cycles: u32,
    deadline_misses: u64,
}

impl<const AXES: usize> StepperExecutor<AXES> {
    /// Constructs an inert executor after validating all timing facts.
    pub fn new(timing: StepperTiming<AXES>) -> Result<Self, MotionError> {
        timing.validate()?;
        Ok(Self {
            timing,
            state: ExecutorState::Idle,
            epoch: DeviceCycle(0),
            next_tick: StreamTick(0),
            position: [0; AXES],
            direction_known: AxisMask::NONE,
            direction_positive: AxisMask::NONE,
            enabled: AxisMask::NONE,
            step_high: AxisMask::NONE,
            last_fall: [None; AXES],
            last_rise: [None; AXES],
            active: None,
            completed_segments: 0,
            emitted_steps: [0; AXES],
            maximum_lateness_cycles: 0,
            deadline_misses: 0,
        })
    }

    /// Installs a committed epoch and exact starting lattice position. This
    /// changes no output and does not imply safety authorization.
    pub fn start_job(
        &mut self,
        epoch: DeviceCycle,
        position: [i64; AXES],
    ) -> Result<(), MotionError> {
        if !matches!(self.state, ExecutorState::Idle | ExecutorState::Complete)
            || self.active.is_some()
            || !self.step_high.is_empty()
            || !self.enabled.is_empty()
        {
            return Err(MotionError::State);
        }
        require_output_grid(epoch.0, self.timing.output_quantum_cycles)?;
        self.state = ExecutorState::Ready;
        self.epoch = epoch;
        self.next_tick = StreamTick(0);
        self.position = position;
        self.direction_known = AxisMask::NONE;
        self.direction_positive = AxisMask::NONE;
        self.last_fall = [None; AXES];
        self.last_rise = [None; AXES];
        self.completed_segments = 0;
        self.emitted_steps = [0; AXES];
        self.maximum_lateness_cycles = 0;
        self.deadline_misses = 0;
        Ok(())
    }

    /// Validates and installs the next contiguous segment without changing any
    /// prior state when validation fails.
    pub fn load_segment(&mut self, segment: ExecutionSegment<AXES>) -> Result<(), MotionError> {
        if self.state != ExecutorState::Ready || self.active.is_some() {
            return Err(MotionError::State);
        }
        if segment.flags != 0 {
            return Err(MotionError::SegmentFlags);
        }
        if segment.start_tick != self.next_tick {
            return Err(MotionError::SegmentOrder);
        }
        if segment.end_tick.0 <= segment.start_tick.0 {
            return Err(MotionError::EmptySegment);
        }
        let start = DeviceCycle(
            self.epoch
                .0
                .checked_add(segment.start_tick.0)
                .ok_or(MotionError::EpochOverflow)?,
        );
        let end = DeviceCycle(
            self.epoch
                .0
                .checked_add(segment.end_tick.0)
                .ok_or(MotionError::EpochOverflow)?,
        );
        require_output_grid(start.0, self.timing.output_quantum_cycles)?;
        require_output_grid(end.0, self.timing.output_quantum_cycles)?;
        let duration = segment.end_tick.0 - segment.start_tick.0;
        let mut steps = [0_u64; AXES];
        let mut next_rise = [None; AXES];
        let next_fall = [None; AXES];
        let mut direction_change = AxisMask::NONE;
        let mut direction_positive = self.direction_positive;
        let mut enable = AxisMask::NONE;
        let mut position = self.position;

        let mut axis = 0;
        while axis < AXES {
            let delta = segment.delta_steps[axis];
            position[axis] = position[axis]
                .checked_add(delta)
                .ok_or(MotionError::PositionOverflow { axis })?;
            let count = delta.unsigned_abs();
            steps[axis] = count;
            if count == 0 {
                axis += 1;
                continue;
            }

            let positive = delta > 0;
            let changes_direction = !self.direction_known.contains(axis)
                || self.direction_positive.contains(axis) != positive;
            if changes_direction {
                direction_change.insert(axis)?;
                if positive {
                    direction_positive.insert(axis)?;
                } else {
                    direction_positive.remove(axis)?;
                }
            }
            if !self.enabled.contains(axis) {
                enable.insert(axis)?;
            }

            let timing = self.timing.axes[axis];
            self.emitted_steps[axis]
                .checked_add(count)
                .ok_or(MotionError::Arithmetic)?;
            let first_offset = centered_step_offset_on_grid(
                duration,
                count,
                0,
                self.timing.output_quantum_cycles,
            )?;
            let first = DeviceCycle(
                start
                    .0
                    .checked_add(first_offset)
                    .ok_or(MotionError::EpochOverflow)?,
            );
            let last_offset = centered_step_offset_on_grid(
                duration,
                count,
                count - 1,
                self.timing.output_quantum_cycles,
            )?;
            let last = start
                .0
                .checked_add(last_offset)
                .ok_or(MotionError::EpochOverflow)?;
            let last_fall = last
                .checked_add(u64::from(timing.pulse_high_cycles))
                .ok_or(MotionError::EpochOverflow)?;
            if first.0 < start.0 || last_fall > end.0 {
                return Err(MotionError::PulseBoundary { axis });
            }

            if count > 1 {
                let minimum_rise_spacing = duration / count;
                let pulse_period = u64::from(timing.pulse_high_cycles)
                    .checked_add(u64::from(timing.pulse_low_cycles))
                    .ok_or(MotionError::Arithmetic)?;
                let frequency_period = minimum_frequency_period(
                    self.timing.device_cycle_hz,
                    timing.maximum_step_frequency_hz,
                )?;
                let required = pulse_period.max(frequency_period);
                if minimum_rise_spacing < required {
                    return Err(MotionError::Rate { axis });
                }
            }
            if let Some(previous_rise) = self.last_rise[axis] {
                let frequency_ready = previous_rise
                    .0
                    .checked_add(minimum_frequency_period(
                        self.timing.device_cycle_hz,
                        timing.maximum_step_frequency_hz,
                    )?)
                    .ok_or(MotionError::Arithmetic)?;
                if first.0 < frequency_ready {
                    return Err(MotionError::Rate { axis });
                }
            }
            if let Some(previous_fall) = self.last_fall[axis] {
                let low_ready = previous_fall
                    .0
                    .checked_add(u64::from(timing.pulse_low_cycles))
                    .ok_or(MotionError::Arithmetic)?;
                if first.0 < low_ready {
                    return Err(MotionError::PulseLow { axis });
                }
                if changes_direction {
                    let hold_ready = previous_fall
                        .0
                        .checked_add(u64::from(timing.direction_hold_cycles))
                        .ok_or(MotionError::Arithmetic)?;
                    if start.0 < hold_ready {
                        return Err(MotionError::DirectionHold { axis });
                    }
                }
            }
            if changes_direction && first.0 - start.0 < u64::from(timing.direction_setup_cycles) {
                return Err(MotionError::DirectionSetup { axis });
            }
            if !self.enabled.contains(axis)
                && first.0 - start.0 < u64::from(timing.enable_setup_cycles)
            {
                return Err(MotionError::EnableSetup { axis });
            }
            next_rise[axis] = Some(first);
            axis += 1;
        }

        let boundary_pending = !direction_change.is_empty() || !enable.is_empty();
        self.completed_segments
            .checked_add(1)
            .ok_or(MotionError::Arithmetic)?;
        self.active = Some(ActiveSegment {
            segment,
            start,
            end,
            steps,
            next_index: [0; AXES],
            next_rise,
            next_fall,
            direction_change,
            direction_positive,
            enable,
            boundary_pending,
        });
        self.state = ExecutorState::Segment;
        Ok(())
    }

    /// Exact next deadline, including a no-output segment-completion boundary.
    pub fn next_deadline(&self) -> Option<DeviceCycle> {
        let active = self.active?;
        active.next_output_cycle().or(Some(active.end))
    }

    /// Earliest exact cycle at which normal terminal disable can satisfy every
    /// configured enable-hold bound. This is not used for asynchronous faults.
    pub fn earliest_finish_cycle(&self) -> Result<DeviceCycle, MotionError> {
        if self.state != ExecutorState::Ready || self.active.is_some() {
            return Err(MotionError::State);
        }
        let mut ready = self
            .epoch
            .0
            .checked_add(self.next_tick.0)
            .ok_or(MotionError::EpochOverflow)?;
        let mut axis = 0;
        while axis < AXES {
            if self.enabled.contains(axis) {
                let last_fall = self.last_fall[axis].ok_or(MotionError::OutputInvariant)?;
                let axis_ready = last_fall
                    .0
                    .checked_add(u64::from(self.timing.axes[axis].enable_hold_cycles))
                    .ok_or(MotionError::Arithmetic)?;
                ready = ready.max(axis_ready);
            }
            axis += 1;
        }
        Ok(DeviceCycle(align_up_to_output_grid(
            ready,
            self.timing.output_quantum_cycles,
        )?))
    }

    /// Returns one event only when its cycle is due. A late event outside the
    /// configured bound faults without returning or applying that event.
    pub fn poll(&mut self, observed: DeviceCycle) -> Result<MotionPoll<AXES>, MotionError> {
        let Some(active) = self.active else {
            return Ok(MotionPoll::Idle);
        };
        let output_at = active.next_output_cycle();
        let scheduled = output_at.unwrap_or(active.end);
        if observed < scheduled {
            return Ok(MotionPoll::Future { at: scheduled });
        }
        let lateness = observed.0 - scheduled.0;
        if lateness > u64::from(self.timing.maximum_lateness_cycles) {
            self.state = ExecutorState::Faulted;
            self.active = None;
            self.deadline_misses = self.deadline_misses.saturating_add(1);
            return Err(MotionError::Deadline {
                scheduled,
                observed,
                maximum_lateness_cycles: self.timing.maximum_lateness_cycles,
            });
        }
        let lateness_u32 = u32::try_from(lateness).map_err(|_| MotionError::Arithmetic)?;
        self.maximum_lateness_cycles = self.maximum_lateness_cycles.max(lateness_u32);

        if output_at.is_none() {
            return self.complete_segment(active);
        }
        let event = match self.pop_output_event(active, scheduled) {
            Ok(event) => event,
            Err(error) => {
                self.state = ExecutorState::Faulted;
                self.active = None;
                return Err(error);
            }
        };
        if event.is_empty() {
            self.state = ExecutorState::Faulted;
            self.active = None;
            return Err(MotionError::OutputInvariant);
        }
        Ok(MotionPoll::Event {
            event,
            lateness_cycles: lateness_u32,
        })
    }

    /// Completes a job and emits the exact normal disable transaction after
    /// configured hold times. Safety faults should instead call [`Self::fault`].
    pub fn finish_job(&mut self, at: DeviceCycle) -> Result<StepperEvent, MotionError> {
        if self.state != ExecutorState::Ready || self.active.is_some() {
            return Err(MotionError::State);
        }
        require_output_grid(at.0, self.timing.output_quantum_cycles)?;
        let mut axis = 0;
        while axis < AXES {
            if self.enabled.contains(axis)
                && let Some(last_fall) = self.last_fall[axis]
            {
                let ready = last_fall
                    .0
                    .checked_add(u64::from(self.timing.axes[axis].enable_hold_cycles))
                    .ok_or(MotionError::Arithmetic)?;
                if at.0 < ready {
                    return Err(MotionError::EnableHold { axis });
                }
            }
            axis += 1;
        }
        let event = StepperEvent {
            at,
            direction_change: AxisMask::NONE,
            direction_positive: self.direction_positive,
            enable: AxisMask::NONE,
            disable: self.enabled,
            step_high: AxisMask::NONE,
            step_low: AxisMask::NONE,
        };
        self.enabled = AxisMask::NONE;
        self.state = ExecutorState::Complete;
        Ok(event)
    }

    /// Produces the immediate logical safe transaction. Pulse width, enable
    /// hold, and direction timing never delay a safety-authority shutdown.
    pub fn fault(&mut self, at: DeviceCycle) -> StepperEvent {
        let event = StepperEvent {
            at,
            direction_change: AxisMask::NONE,
            direction_positive: self.direction_positive,
            enable: AxisMask::NONE,
            disable: self.enabled,
            step_high: AxisMask::NONE,
            step_low: self.step_high,
        };
        self.step_high = AxisMask::NONE;
        self.enabled = AxisMask::NONE;
        self.active = None;
        self.state = ExecutorState::Faulted;
        event
    }

    /// Current bounded execution facts.
    pub const fn status(&self) -> StepperStatus<AXES> {
        StepperStatus {
            state: self.state,
            epoch: self.epoch,
            next_tick: self.next_tick,
            position: self.position,
            direction_known: self.direction_known,
            direction_positive: self.direction_positive,
            enabled: self.enabled,
            step_high: self.step_high,
            completed_segments: self.completed_segments,
            emitted_steps: self.emitted_steps,
            maximum_lateness_cycles: self.maximum_lateness_cycles,
            deadline_misses: self.deadline_misses,
        }
    }

    /// Private logical-state copy used only to prove an entire cached block
    /// before its unique ownership token crosses into the live executor. The
    /// executor deliberately owns no hardware resources, but keeping this
    /// operation private prevents a second public event producer from being
    /// created accidentally.
    fn preflight_snapshot(&self) -> Self {
        Self {
            timing: self.timing,
            state: self.state,
            epoch: self.epoch,
            next_tick: self.next_tick,
            position: self.position,
            direction_known: self.direction_known,
            direction_positive: self.direction_positive,
            enabled: self.enabled,
            step_high: self.step_high,
            last_fall: self.last_fall,
            last_rise: self.last_rise,
            active: self.active,
            completed_segments: self.completed_segments,
            emitted_steps: self.emitted_steps,
            maximum_lateness_cycles: self.maximum_lateness_cycles,
            deadline_misses: self.deadline_misses,
        }
    }

    /// Advances one already validated segment in time proportional to the axis
    /// count, not the number of emitted steps. This is only used on the private
    /// admission snapshot; live execution must still emit every edge.
    fn preflight_complete_segment(&mut self) -> Result<(), MotionError> {
        if self.state != ExecutorState::Segment || !self.step_high.is_empty() {
            return Err(MotionError::State);
        }
        let active = self.active.ok_or(MotionError::State)?;
        self.direction_known = self.direction_known.union(active.direction_change);
        self.direction_positive = active.direction_positive;
        self.enabled = self.enabled.union(active.enable);

        let duration = active.segment.end_tick.0 - active.segment.start_tick.0;
        let mut axis = 0;
        while axis < AXES {
            let count = active.steps[axis];
            if count != 0 {
                self.position[axis] = self.position[axis]
                    .checked_add(active.segment.delta_steps[axis])
                    .ok_or(MotionError::PositionOverflow { axis })?;
                self.emitted_steps[axis] = self.emitted_steps[axis]
                    .checked_add(count)
                    .ok_or(MotionError::Arithmetic)?;
                let last_offset = centered_step_offset_on_grid(
                    duration,
                    count,
                    count - 1,
                    self.timing.output_quantum_cycles,
                )?;
                let rise = DeviceCycle(
                    active
                        .start
                        .0
                        .checked_add(last_offset)
                        .ok_or(MotionError::EpochOverflow)?,
                );
                let fall = DeviceCycle(
                    rise.0
                        .checked_add(u64::from(self.timing.axes[axis].pulse_high_cycles))
                        .ok_or(MotionError::EpochOverflow)?,
                );
                self.last_rise[axis] = Some(rise);
                self.last_fall[axis] = Some(fall);
            }
            axis += 1;
        }
        self.next_tick = active.segment.end_tick;
        self.completed_segments = self
            .completed_segments
            .checked_add(1)
            .ok_or(MotionError::Arithmetic)?;
        self.active = None;
        self.state = ExecutorState::Ready;
        Ok(())
    }

    fn pop_output_event(
        &mut self,
        mut active: ActiveSegment<AXES>,
        at: DeviceCycle,
    ) -> Result<StepperEvent, MotionError> {
        let mut event = StepperEvent {
            at,
            direction_change: AxisMask::NONE,
            direction_positive: self.direction_positive,
            enable: AxisMask::NONE,
            disable: AxisMask::NONE,
            step_high: AxisMask::NONE,
            step_low: AxisMask::NONE,
        };
        if active.boundary_pending && active.start == at {
            event.direction_change = active.direction_change;
            event.direction_positive = active.direction_positive;
            event.enable = active.enable;
            self.direction_known = self.direction_known.union(active.direction_change);
            self.direction_positive = active.direction_positive;
            self.enabled = self.enabled.union(active.enable);
            active.boundary_pending = false;
        }

        let mut axis = 0;
        while axis < AXES {
            if active.next_fall[axis] == Some(at) {
                if !self.step_high.contains(axis) {
                    return Err(MotionError::OutputInvariant);
                }
                event.step_low.insert(axis)?;
                self.step_high.remove(axis)?;
                self.last_fall[axis] = Some(at);
                active.next_fall[axis] = None;
            }
            axis += 1;
        }

        axis = 0;
        while axis < AXES {
            if active.next_rise[axis] == Some(at) {
                if self.step_high.contains(axis) || event.step_low.contains(axis) {
                    return Err(MotionError::OutputInvariant);
                }
                event.step_high.insert(axis)?;
                self.step_high.insert(axis)?;
                let positive = active.segment.delta_steps[axis] > 0;
                self.position[axis] = if positive {
                    self.position[axis]
                        .checked_add(1)
                        .ok_or(MotionError::PositionOverflow { axis })?
                } else {
                    self.position[axis]
                        .checked_sub(1)
                        .ok_or(MotionError::PositionOverflow { axis })?
                };
                self.emitted_steps[axis] = self.emitted_steps[axis]
                    .checked_add(1)
                    .ok_or(MotionError::Arithmetic)?;
                self.last_rise[axis] = Some(at);
                let fall = DeviceCycle(
                    at.0.checked_add(u64::from(self.timing.axes[axis].pulse_high_cycles))
                        .ok_or(MotionError::EpochOverflow)?,
                );
                active.next_fall[axis] = Some(fall);
                let next_index = active.next_index[axis]
                    .checked_add(1)
                    .ok_or(MotionError::Arithmetic)?;
                active.next_index[axis] = next_index;
                active.next_rise[axis] = if next_index < active.steps[axis] {
                    let duration = active.segment.end_tick.0 - active.segment.start_tick.0;
                    let offset = centered_step_offset_on_grid(
                        duration,
                        active.steps[axis],
                        next_index,
                        self.timing.output_quantum_cycles,
                    )?;
                    Some(DeviceCycle(
                        active
                            .start
                            .0
                            .checked_add(offset)
                            .ok_or(MotionError::EpochOverflow)?,
                    ))
                } else {
                    None
                };
            }
            axis += 1;
        }
        self.active = Some(active);
        Ok(event)
    }

    fn complete_segment(
        &mut self,
        active: ActiveSegment<AXES>,
    ) -> Result<MotionPoll<AXES>, MotionError> {
        if !self.step_high.is_empty()
            || active.boundary_pending
            || active.next_rise.iter().any(Option::is_some)
            || active.next_fall.iter().any(Option::is_some)
        {
            self.state = ExecutorState::Faulted;
            self.active = None;
            return Err(MotionError::OutputInvariant);
        }
        let mut axis = 0;
        while axis < AXES {
            if active.next_index[axis] != active.steps[axis] {
                self.state = ExecutorState::Faulted;
                self.active = None;
                return Err(MotionError::OutputInvariant);
            }
            axis += 1;
        }
        self.next_tick = active.segment.end_tick;
        self.completed_segments = self
            .completed_segments
            .checked_add(1)
            .ok_or(MotionError::Arithmetic)?;
        self.active = None;
        self.state = ExecutorState::Ready;
        Ok(MotionPoll::SegmentComplete(SegmentCompletion {
            at: active.end,
            end_tick: active.segment.end_tick,
            delta_steps: active.segment.delta_steps,
            position: self.position,
            maximum_half_tick_error: if active.steps.iter().any(|steps| *steps != 0) {
                self.timing.output_quantum_cycles
            } else {
                0
            },
        }))
    }
}

/// Replay a complete segment slice through the production executor's exact
/// rate, pulse, direction, enable, output-grid, continuity, and overflow
/// checks without emitting an output event or owning hardware.
///
/// This is intentionally proportional to `segments.len() * AXES`, not to the
/// number of commanded step edges. It is suitable for browser/WASM compiler
/// evidence and firmware admission tests while leaving live edge generation
/// exclusively inside [`StepperExecutor`].
pub fn preflight_stepper_segments<const AXES: usize>(
    timing: StepperTiming<AXES>,
    initial_position: [i64; AXES],
    segments: &[ExecutionSegment<AXES>],
) -> Result<StepperPreflightSummary<AXES>, MotionError> {
    let mut executor = StepperExecutor::new(timing)?;
    executor.start_job(DeviceCycle(0), initial_position)?;
    for segment in segments {
        executor.load_segment(*segment)?;
        executor.preflight_complete_segment()?;
    }
    let status = executor.status();
    let earliest_finish_cycle = executor.earliest_finish_cycle()?;
    executor.finish_job(earliest_finish_cycle)?;
    Ok(StepperPreflightSummary {
        end_tick: status.next_tick,
        position: status.position,
        emitted_steps: status.emitted_steps,
        segment_count: status.completed_segments,
        earliest_finish_cycle,
    })
}

fn finite_difference_record_limits<const AXES: usize>(
    timing: StepperTiming<AXES>,
    limits: FiniteDifferenceExecutionLimits,
    segment: &FiniteDifferenceSegment<AXES>,
) -> Result<FiniteDifferenceValidationLimits<AXES>, FiniteDifferencePreflightError> {
    if segment.update_period_ticks == 0 {
        return Err(FiniteDifferencePreflightError::Segment {
            index: 0,
            error: FiniteDifferenceError::UpdatePolicy,
        });
    }
    let mut maximum_absolute_first_difference = [0_u64; AXES];
    let mut axis = 0;
    while axis < AXES {
        let axis_timing = timing.axes[axis];
        let pulse_period = u64::from(axis_timing.pulse_high_cycles)
            .checked_add(u64::from(axis_timing.pulse_low_cycles))
            .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
        let frequency_period = minimum_frequency_period(
            timing.device_cycle_hz,
            axis_timing.maximum_step_frequency_hz,
        )
        .map_err(FiniteDifferencePreflightError::Timing)?;
        let required_cycles = pulse_period.max(frequency_period);
        let update_period = u64::from(segment.update_period_ticks);
        let minimum_updates = div_ceil_u64(required_cycles, update_period)
            .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
        let maximum = (FINITE_DIFFERENCE_ONE_STEP.unsigned_abs() - 1)
            .checked_div(minimum_updates)
            .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
        if maximum == 0 {
            return Err(FiniteDifferencePreflightError::Timing(MotionError::Rate {
                axis,
            }));
        }
        maximum_absolute_first_difference[axis] = maximum;
        axis += 1;
    }
    Ok(FiniteDifferenceValidationLimits {
        maximum_segment_ticks: limits.maximum_segment_ticks,
        maximum_update_count: limits.maximum_update_count,
        maximum_steps_per_segment: limits.maximum_steps_per_segment,
        maximum_absolute_first_difference,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FiniteDifferenceAdmissionState<const AXES: usize> {
    next_tick: StreamTick,
    finite_position: [i64; AXES],
    relative_steps: [i64; AXES],
    position: [i64; AXES],
    direction_known: AxisMask,
    direction_positive: AxisMask,
    enabled: AxisMask,
    last_rise: [Option<DeviceCycle>; AXES],
    last_fall: [Option<DeviceCycle>; AXES],
    emitted_steps: [u64; AXES],
    segment_count: u32,
    update_count: u64,
}

impl<const AXES: usize> FiniteDifferenceAdmissionState<AXES> {
    const fn new(position: [i64; AXES]) -> Self {
        Self {
            next_tick: StreamTick(0),
            finite_position: [0; AXES],
            relative_steps: [0; AXES],
            position,
            direction_known: AxisMask::NONE,
            direction_positive: AxisMask::NONE,
            enabled: AxisMask::NONE,
            last_rise: [None; AXES],
            last_fall: [None; AXES],
            emitted_steps: [0; AXES],
            segment_count: 0,
            update_count: 0,
        }
    }

    fn earliest_finish_cycle(
        self,
        timing: StepperTiming<AXES>,
        epoch: DeviceCycle,
    ) -> Result<DeviceCycle, FiniteDifferencePreflightError> {
        let mut earliest = epoch
            .0
            .checked_add(self.next_tick.0)
            .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
        let mut axis = 0;
        while axis < AXES {
            if self.enabled.contains(axis) {
                let last = self.last_fall[axis].ok_or(FiniteDifferencePreflightError::Timing(
                    MotionError::OutputInvariant,
                ))?;
                earliest = earliest.max(
                    last.0
                        .checked_add(u64::from(timing.axes[axis].enable_hold_cycles))
                        .ok_or(FiniteDifferencePreflightError::Arithmetic)?,
                );
            }
            axis += 1;
        }
        Ok(DeviceCycle(
            align_up_to_output_grid(earliest, timing.output_quantum_cycles)
                .map_err(FiniteDifferencePreflightError::Timing)?,
        ))
    }
}

fn preflight_finite_difference_segment<const AXES: usize>(
    timing: StepperTiming<AXES>,
    limits: FiniteDifferenceExecutionLimits,
    epoch: DeviceCycle,
    state: &mut FiniteDifferenceAdmissionState<AXES>,
    segment: &FiniteDifferenceSegment<AXES>,
    index: usize,
) -> Result<FiniteDifferenceSegmentSummary<AXES>, FiniteDifferencePreflightError> {
    if segment.update_period_ticks == 0 {
        return Err(FiniteDifferencePreflightError::Segment {
            index,
            error: FiniteDifferenceError::UpdatePolicy,
        });
    }
    require_output_grid(segment.start_tick.0, timing.output_quantum_cycles)
        .map_err(FiniteDifferencePreflightError::Timing)?;
    require_output_grid(segment.end_tick.0, timing.output_quantum_cycles)
        .map_err(FiniteDifferencePreflightError::Timing)?;
    require_output_grid(
        u64::from(segment.update_period_ticks),
        timing.output_quantum_cycles,
    )
    .map_err(FiniteDifferencePreflightError::Timing)?;

    let record_limits = finite_difference_record_limits(timing, limits, segment)?;
    let summary = segment
        .validate(state.next_tick, state.finite_position, record_limits)
        .map_err(|error| FiniteDifferencePreflightError::Segment { index, error })?;
    let mut next = *state;
    next.update_count = next
        .update_count
        .checked_add(u64::from(segment.update_count))
        .ok_or(FiniteDifferencePreflightError::Arithmetic)?;

    let segment_start = epoch
        .0
        .checked_add(segment.start_tick.0)
        .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
    let segment_end = epoch
        .0
        .checked_add(segment.end_tick.0)
        .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
    let mut axis = 0;
    while axis < AXES {
        if summary.start_steps[axis] != next.relative_steps[axis] {
            return Err(FiniteDifferencePreflightError::Segment {
                index,
                error: FiniteDifferenceError::PositionContinuity { axis },
            });
        }
        let delta = summary.delta_steps[axis];
        next.position[axis] = next.position[axis]
            .checked_add(delta)
            .ok_or(FiniteDifferencePreflightError::PositionOverflow { index, axis })?;
        let count = delta.unsigned_abs();
        next.emitted_steps[axis] = next.emitted_steps[axis]
            .checked_add(count)
            .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
        if count == 0 {
            axis += 1;
            continue;
        }

        let positive = delta > 0;
        let changes_direction = !next.direction_known.contains(axis)
            || next.direction_positive.contains(axis) != positive;
        let first_target = summary.start_steps[axis]
            .checked_add(if positive { 1 } else { -1 })
            .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
        let first_update =
            finite_difference_crossing_update(segment, index, axis, first_target, positive)?;
        let last_update = finite_difference_crossing_update(
            segment,
            index,
            axis,
            summary.end_steps[axis],
            positive,
        )?;
        let first = epoch
            .0
            .checked_add(finite_difference_update_cycle(segment, first_update)?)
            .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
        let last = epoch
            .0
            .checked_add(finite_difference_update_cycle(segment, last_update)?)
            .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
        let fall = last
            .checked_add(u64::from(timing.axes[axis].pulse_high_cycles))
            .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
        if first < segment_start || fall > segment_end {
            return Err(FiniteDifferencePreflightError::Timing(
                MotionError::PulseBoundary { axis },
            ));
        }
        if let Some(previous_rise) = next.last_rise[axis] {
            let frequency_ready = previous_rise
                .0
                .checked_add(
                    minimum_frequency_period(
                        timing.device_cycle_hz,
                        timing.axes[axis].maximum_step_frequency_hz,
                    )
                    .map_err(FiniteDifferencePreflightError::Timing)?,
                )
                .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
            if first < frequency_ready {
                return Err(FiniteDifferencePreflightError::Timing(MotionError::Rate {
                    axis,
                }));
            }
        }
        if let Some(previous_fall) = next.last_fall[axis] {
            let low_ready = previous_fall
                .0
                .checked_add(u64::from(timing.axes[axis].pulse_low_cycles))
                .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
            if first < low_ready {
                return Err(FiniteDifferencePreflightError::Timing(
                    MotionError::PulseLow { axis },
                ));
            }
            if changes_direction {
                let hold_ready = previous_fall
                    .0
                    .checked_add(u64::from(timing.axes[axis].direction_hold_cycles))
                    .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
                if segment_start < hold_ready {
                    return Err(FiniteDifferencePreflightError::Timing(
                        MotionError::DirectionHold { axis },
                    ));
                }
            }
        }
        if changes_direction
            && first - segment_start < u64::from(timing.axes[axis].direction_setup_cycles)
        {
            return Err(FiniteDifferencePreflightError::Timing(
                MotionError::DirectionSetup { axis },
            ));
        }
        if !next.enabled.contains(axis)
            && first - segment_start < u64::from(timing.axes[axis].enable_setup_cycles)
        {
            return Err(FiniteDifferencePreflightError::Timing(
                MotionError::EnableSetup { axis },
            ));
        }
        next.direction_known
            .insert(axis)
            .map_err(FiniteDifferencePreflightError::Timing)?;
        if positive {
            next.direction_positive
                .insert(axis)
                .map_err(FiniteDifferencePreflightError::Timing)?;
        } else {
            next.direction_positive
                .remove(axis)
                .map_err(FiniteDifferencePreflightError::Timing)?;
        }
        next.enabled
            .insert(axis)
            .map_err(FiniteDifferencePreflightError::Timing)?;
        next.last_rise[axis] = Some(DeviceCycle(last));
        next.last_fall[axis] = Some(DeviceCycle(fall));
        axis += 1;
    }
    next.next_tick = summary.end_tick;
    next.relative_steps = summary.end_steps;
    next.finite_position = summary.terminal_position;
    next.segment_count = next
        .segment_count
        .checked_add(1)
        .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
    *state = next;
    Ok(summary)
}

/// Replay a complete direct third-order stream without iterating its dense
/// update frames or emitting hardware events.
///
/// Each axis is monotonic inside one record. The validator derives a strict
/// Q31.32 progress ceiling from pulse high/low and maximum-frequency spacing:
/// if `M` update periods are required between rises, each first difference is
/// at most `floor((one_step - 1) / M)`. Therefore even a threshold-crossing
/// update's worst overshoot cannot reach the next rounding boundary in fewer
/// than `M` updates. First/last crossings are then found by exact monotonic
/// binary search, keeping admission proportional to
/// `segments * AXES * log2(updates)` rather than to path duration or edge count.
pub fn preflight_finite_difference_segments<const AXES: usize>(
    timing: StepperTiming<AXES>,
    limits: FiniteDifferenceExecutionLimits,
    initial_position: [i64; AXES],
    segments: &[FiniteDifferenceSegment<AXES>],
) -> Result<FiniteDifferencePreflightSummary<AXES>, FiniteDifferencePreflightError> {
    timing
        .validate()
        .map_err(FiniteDifferencePreflightError::Timing)?;
    if segments.is_empty() {
        return Err(FiniteDifferencePreflightError::Empty);
    }
    if limits.maximum_segment_ticks == 0
        || limits.maximum_update_count == 0
        || limits.maximum_steps_per_segment == 0
    {
        return Err(FiniteDifferencePreflightError::Segment {
            index: 0,
            error: FiniteDifferenceError::Limits,
        });
    }

    let mut state = FiniteDifferenceAdmissionState::new(initial_position);
    for (index, segment) in segments.iter().enumerate() {
        preflight_finite_difference_segment(
            timing,
            limits,
            DeviceCycle(0),
            &mut state,
            segment,
            index,
        )?;
    }
    let earliest_finish_cycle = state.earliest_finish_cycle(timing, DeviceCycle(0))?;
    Ok(FiniteDifferencePreflightSummary {
        end_tick: state.next_tick,
        position: state.position,
        terminal_finite_position: state.finite_position,
        emitted_steps: state.emitted_steps,
        segment_count: state.segment_count,
        update_count: state.update_count,
        earliest_finish_cycle,
        maximum_position_error: FINITE_DIFFERENCE_ONE_STEP.unsigned_abs() / 2,
    })
}

fn finite_difference_crossing_update<const AXES: usize>(
    segment: &FiniteDifferenceSegment<AXES>,
    segment_index: usize,
    axis: usize,
    target: i64,
    increasing: bool,
) -> Result<u32, FiniteDifferencePreflightError> {
    let terminal = segment
        .step_at(axis, segment.update_count)
        .map_err(|error| FiniteDifferencePreflightError::Segment {
            index: segment_index,
            error,
        })?;
    if (increasing && terminal < target) || (!increasing && terminal > target) {
        return Err(FiniteDifferencePreflightError::Arithmetic);
    }
    let mut low = 1_u32;
    let mut high = segment.update_count;
    while low < high {
        let middle = low + (high - low) / 2;
        let coordinate = segment.step_at(axis, middle).map_err(|error| {
            FiniteDifferencePreflightError::Segment {
                index: segment_index,
                error,
            }
        })?;
        let reached = if increasing {
            coordinate >= target
        } else {
            coordinate <= target
        };
        if reached {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Ok(low)
}

fn finite_difference_update_cycle<const AXES: usize>(
    segment: &FiniteDifferenceSegment<AXES>,
    update: u32,
) -> Result<u64, FiniteDifferencePreflightError> {
    u64::from(update)
        .checked_mul(u64::from(segment.update_period_ticks))
        .and_then(|offset| segment.start_tick.0.checked_add(offset))
        .ok_or(FiniteDifferencePreflightError::Arithmetic)
}

fn div_ceil_u64(numerator: u64, denominator: u64) -> Option<u64> {
    if denominator == 0 {
        return None;
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    quotient.checked_add(u64::from(remainder != 0))
}

/// One dense finite-difference update suitable for deterministic DMA/image
/// expansion after the complete stream has passed production preflight.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteDifferenceFrame<const AXES: usize> {
    /// One-based update index inside the current record.
    pub update_index: u32,
    /// Exact stream-relative update boundary.
    pub at: StreamTick,
    /// Axes whose rounded command coordinate advanced by one step.
    pub step: AxisMask,
    /// Positive direction for the subset in `step`.
    pub direction_positive: AxisMask,
    /// Current rounded relative command coordinate.
    pub relative_steps: [i64; AXES],
    /// Current unrounded Q31.32 relative coordinate.
    pub finite_position: [i64; AXES],
}

/// Allocation-free dense executor for one independently validated direct
/// finite-difference record.
///
/// This primitive performs the canonical three checked additions per axis and
/// yields exact step-rise decisions. It owns no pins, pulse state, safety
/// authority, or cache token. A production composition must first run
/// [`preflight_finite_difference_segments`] over the complete contiguous
/// stream, then translate these frames through the same qualified output-image
/// and physical-commit layers used by ordinary motion.
pub struct FiniteDifferenceFrameGenerator<const AXES: usize> {
    segment: FiniteDifferenceSegment<AXES>,
    summary: FiniteDifferenceSegmentSummary<AXES>,
    state: [FiniteDifferenceAxis; AXES],
    relative_steps: [i64; AXES],
    update_index: u32,
}

impl<const AXES: usize> FiniteDifferenceFrameGenerator<AXES> {
    /// Revalidates this record against the production timing-derived progress
    /// limits and exact incoming stream state before any dense update exists.
    pub fn new(
        timing: StepperTiming<AXES>,
        limits: FiniteDifferenceExecutionLimits,
        expected_start_tick: StreamTick,
        expected_initial_position: [i64; AXES],
        segment: FiniteDifferenceSegment<AXES>,
    ) -> Result<Self, FiniteDifferencePreflightError> {
        timing
            .validate()
            .map_err(FiniteDifferencePreflightError::Timing)?;
        require_output_grid(segment.start_tick.0, timing.output_quantum_cycles)
            .map_err(FiniteDifferencePreflightError::Timing)?;
        require_output_grid(segment.end_tick.0, timing.output_quantum_cycles)
            .map_err(FiniteDifferencePreflightError::Timing)?;
        require_output_grid(
            u64::from(segment.update_period_ticks),
            timing.output_quantum_cycles,
        )
        .map_err(FiniteDifferencePreflightError::Timing)?;
        let record_limits = finite_difference_record_limits(timing, limits, &segment)?;
        let summary = segment
            .validate(
                expected_start_tick,
                expected_initial_position,
                record_limits,
            )
            .map_err(|error| FiniteDifferencePreflightError::Segment { index: 0, error })?;
        Ok(Self {
            segment,
            summary,
            state: segment.axes,
            relative_steps: summary.start_steps,
            update_index: 0,
        })
    }

    /// Advance exactly one dense update. `None` means every declared update was
    /// already produced and the caller may require [`Self::finish`].
    pub fn advance(
        &mut self,
    ) -> Result<Option<FiniteDifferenceFrame<AXES>>, FiniteDifferencePreflightError> {
        if self.update_index == self.segment.update_count {
            return Ok(None);
        }
        let next_index = self
            .update_index
            .checked_add(1)
            .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
        let mut step = AxisMask::NONE;
        let mut direction_positive = AxisMask::NONE;
        let mut axis = 0;
        while axis < AXES {
            let state = &mut self.state[axis];
            state.initial_position = state
                .initial_position
                .checked_add(state.first_difference)
                .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
            state.first_difference = state
                .first_difference
                .checked_add(state.second_difference)
                .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
            state.second_difference = state
                .second_difference
                .checked_add(state.third_difference)
                .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
            let target =
                alumina_machine_ir::round_finite_difference_position(state.initial_position);
            let delta = target
                .checked_sub(self.relative_steps[axis])
                .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
            if !(-1..=1).contains(&delta) {
                return Err(FiniteDifferencePreflightError::Timing(
                    MotionError::OutputInvariant,
                ));
            }
            if delta != 0 {
                step.insert(axis)
                    .map_err(FiniteDifferencePreflightError::Timing)?;
                if delta > 0 {
                    direction_positive
                        .insert(axis)
                        .map_err(FiniteDifferencePreflightError::Timing)?;
                }
                self.relative_steps[axis] = target;
            }
            axis += 1;
        }
        let at = StreamTick(finite_difference_update_cycle(&self.segment, next_index)?);
        self.update_index = next_index;
        Ok(Some(FiniteDifferenceFrame {
            update_index: next_index,
            at,
            step,
            direction_positive,
            relative_steps: self.relative_steps,
            finite_position: self.state.map(|state| state.initial_position),
        }))
    }

    /// Require every dense update and exact terminal integer/fixed-point state.
    pub fn finish(
        &self,
    ) -> Result<FiniteDifferenceSegmentSummary<AXES>, FiniteDifferencePreflightError> {
        if self.update_index != self.segment.update_count
            || self.relative_steps != self.summary.end_steps
            || self.state.map(|state| state.initial_position) != self.summary.terminal_position
        {
            return Err(FiniteDifferencePreflightError::Timing(
                MotionError::OutputInvariant,
            ));
        }
        Ok(self.summary)
    }

    /// Dense updates not yet produced.
    pub const fn remaining_updates(&self) -> u32 {
        self.segment.update_count - self.update_index
    }
}

/// Exact terminal facts from one densely executed finite-difference record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteDifferenceSegmentCompletion<const AXES: usize> {
    /// Absolute device cycle of the record horizon.
    pub at: DeviceCycle,
    /// Exclusive stream-relative record horizon.
    pub end_tick: StreamTick,
    /// Rounded relative displacement produced by this record.
    pub delta_steps: [i64; AXES],
    /// Absolute terminal command-lattice position.
    pub position: [i64; AXES],
    /// Exact terminal Q31.32 stream-relative coordinate.
    pub terminal_finite_position: [i64; AXES],
    /// Number of dense recurrence updates consumed by this record.
    pub update_count: u32,
}

/// One bounded result from the direct finite-difference real-time executor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FiniteDifferenceMotionPoll<const AXES: usize> {
    /// No record is installed.
    Idle,
    /// The next recurrence/output boundary is in the future.
    Future { at: DeviceCycle },
    /// One dense recurrence update was consumed. `event` is present only when
    /// that update also changes a physical logical output.
    Update {
        frame: FiniteDifferenceFrame<AXES>,
        event: Option<StepperEvent>,
        lateness_cycles: u32,
    },
    /// A direction/enable boundary or pulse fall occurred between updates.
    Event {
        event: StepperEvent,
        lateness_cycles: u32,
    },
    /// Every update and pulse fall in the installed record completed exactly.
    SegmentComplete(FiniteDifferenceSegmentCompletion<AXES>),
}

struct ActiveFiniteDifferenceSegment<const AXES: usize> {
    generator: FiniteDifferenceFrameGenerator<AXES>,
    terminal: FiniteDifferenceAdmissionState<AXES>,
    start: DeviceCycle,
    end: DeviceCycle,
    next_update: Option<DeviceCycle>,
    next_fall: [Option<DeviceCycle>; AXES],
    direction_change: AxisMask,
    direction_positive: AxisMask,
    enable: AxisMask,
    boundary_pending: bool,
}

impl<const AXES: usize> ActiveFiniteDifferenceSegment<AXES> {
    fn next_output_cycle(&self) -> Option<DeviceCycle> {
        let mut next = if self.boundary_pending {
            Some(self.start)
        } else {
            self.next_update
        };
        if self.boundary_pending {
            next = minimum_cycle(next, self.next_update);
        }
        let mut axis = 0;
        while axis < AXES {
            next = minimum_cycle(next, self.next_fall[axis]);
            axis += 1;
        }
        next
    }
}

/// Allocation-free direct third-order recurrence and logical step-event owner.
///
/// Every dense update is an explicit deadline, including updates that do not
/// cross an integer step boundary. The executor owns no pins, DMA descriptors,
/// cache publication, or safety authorization; callers must map each returned
/// event through the same single-owner physical commit path as ordinary motion.
pub struct FiniteDifferenceStepperExecutor<const AXES: usize> {
    timing: StepperTiming<AXES>,
    limits: FiniteDifferenceExecutionLimits,
    state: ExecutorState,
    epoch: DeviceCycle,
    live: FiniteDifferenceAdmissionState<AXES>,
    step_high: AxisMask,
    active: Option<ActiveFiniteDifferenceSegment<AXES>>,
    maximum_lateness_cycles: u32,
    deadline_misses: u64,
}

impl<const AXES: usize> FiniteDifferenceStepperExecutor<AXES> {
    /// Construct an inert direct executor after validating fixed timing and
    /// finite-difference ownership bounds.
    pub fn new(
        timing: StepperTiming<AXES>,
        limits: FiniteDifferenceExecutionLimits,
    ) -> Result<Self, FiniteDifferencePreflightError> {
        timing
            .validate()
            .map_err(FiniteDifferencePreflightError::Timing)?;
        if limits.maximum_segment_ticks == 0
            || limits.maximum_update_count == 0
            || limits.maximum_steps_per_segment == 0
        {
            return Err(FiniteDifferencePreflightError::Segment {
                index: 0,
                error: FiniteDifferenceError::Limits,
            });
        }
        Ok(Self {
            timing,
            limits,
            state: ExecutorState::Idle,
            epoch: DeviceCycle(0),
            live: FiniteDifferenceAdmissionState::new([0; AXES]),
            step_high: AxisMask::NONE,
            active: None,
            maximum_lateness_cycles: 0,
            deadline_misses: 0,
        })
    }

    /// Install the deterministic local epoch and absolute starting coordinate
    /// while all logical outputs remain inactive.
    pub fn start_job(
        &mut self,
        epoch: DeviceCycle,
        position: [i64; AXES],
    ) -> Result<(), FiniteDifferencePreflightError> {
        if !matches!(self.state, ExecutorState::Idle | ExecutorState::Complete)
            || self.active.is_some()
            || !self.step_high.is_empty()
            || !self.live.enabled.is_empty()
        {
            return Err(FiniteDifferencePreflightError::Timing(MotionError::State));
        }
        require_output_grid(epoch.0, self.timing.output_quantum_cycles)
            .map_err(FiniteDifferencePreflightError::Timing)?;
        self.state = ExecutorState::Ready;
        self.epoch = epoch;
        self.live = FiniteDifferenceAdmissionState::new(position);
        self.maximum_lateness_cycles = 0;
        self.deadline_misses = 0;
        Ok(())
    }

    /// Validate and install one contiguous record without partially mutating
    /// live numerical or output state on rejection.
    pub fn load_segment(
        &mut self,
        segment: FiniteDifferenceSegment<AXES>,
    ) -> Result<(), FiniteDifferencePreflightError> {
        if self.state != ExecutorState::Ready || self.active.is_some() || !self.step_high.is_empty()
        {
            return Err(FiniteDifferencePreflightError::Timing(MotionError::State));
        }
        let mut terminal = self.live;
        let summary = preflight_finite_difference_segment(
            self.timing,
            self.limits,
            self.epoch,
            &mut terminal,
            &segment,
            0,
        )?;
        let generator = FiniteDifferenceFrameGenerator::new(
            self.timing,
            self.limits,
            self.live.next_tick,
            self.live.finite_position,
            segment,
        )?;
        let start = DeviceCycle(
            self.epoch
                .0
                .checked_add(segment.start_tick.0)
                .ok_or(FiniteDifferencePreflightError::Arithmetic)?,
        );
        let end = DeviceCycle(
            self.epoch
                .0
                .checked_add(segment.end_tick.0)
                .ok_or(FiniteDifferencePreflightError::Arithmetic)?,
        );
        let next_update = Some(DeviceCycle(
            start
                .0
                .checked_add(u64::from(segment.update_period_ticks))
                .ok_or(FiniteDifferencePreflightError::Arithmetic)?,
        ));
        let mut direction_change = AxisMask::NONE;
        let mut direction_positive = self.live.direction_positive;
        let mut enable = AxisMask::NONE;
        let mut axis = 0;
        while axis < AXES {
            let delta = summary.delta_steps[axis];
            if delta != 0 {
                let positive = delta > 0;
                if !self.live.direction_known.contains(axis)
                    || self.live.direction_positive.contains(axis) != positive
                {
                    direction_change
                        .insert(axis)
                        .map_err(FiniteDifferencePreflightError::Timing)?;
                    if positive {
                        direction_positive
                            .insert(axis)
                            .map_err(FiniteDifferencePreflightError::Timing)?;
                    } else {
                        direction_positive
                            .remove(axis)
                            .map_err(FiniteDifferencePreflightError::Timing)?;
                    }
                }
                if !self.live.enabled.contains(axis) {
                    enable
                        .insert(axis)
                        .map_err(FiniteDifferencePreflightError::Timing)?;
                }
            }
            axis += 1;
        }
        let boundary_pending = !direction_change.is_empty() || !enable.is_empty();
        self.active = Some(ActiveFiniteDifferenceSegment {
            generator,
            terminal,
            start,
            end,
            next_update,
            next_fall: [None; AXES],
            direction_change,
            direction_positive,
            enable,
            boundary_pending,
        });
        self.state = ExecutorState::Segment;
        Ok(())
    }

    /// Exact next recurrence, edge, or segment-horizon deadline.
    pub fn next_deadline(&self) -> Option<DeviceCycle> {
        if self.state == ExecutorState::Faulted {
            return None;
        }
        let active = self.active.as_ref()?;
        active.next_output_cycle().or(Some(active.end))
    }

    /// Consume at most one dense recurrence update or logical output boundary.
    /// Late observations fault before returning the uncommitted update/event.
    pub fn poll(
        &mut self,
        observed: DeviceCycle,
    ) -> Result<FiniteDifferenceMotionPoll<AXES>, FiniteDifferencePreflightError> {
        let result = self.poll_inner(observed);
        if result.is_err() {
            self.state = ExecutorState::Faulted;
            self.active = None;
        }
        result
    }

    fn poll_inner(
        &mut self,
        observed: DeviceCycle,
    ) -> Result<FiniteDifferenceMotionPoll<AXES>, FiniteDifferencePreflightError> {
        if self.state == ExecutorState::Faulted {
            return Err(FiniteDifferencePreflightError::Timing(MotionError::State));
        }
        let Some(mut active) = self.active.take() else {
            return Ok(FiniteDifferenceMotionPoll::Idle);
        };
        let output_at = active.next_output_cycle();
        let scheduled = output_at.unwrap_or(active.end);
        if observed < scheduled {
            self.active = Some(active);
            return Ok(FiniteDifferenceMotionPoll::Future { at: scheduled });
        }
        let lateness = observed.0 - scheduled.0;
        if lateness > u64::from(self.timing.maximum_lateness_cycles) {
            self.deadline_misses = self.deadline_misses.saturating_add(1);
            return Err(FiniteDifferencePreflightError::Timing(
                MotionError::Deadline {
                    scheduled,
                    observed,
                    maximum_lateness_cycles: self.timing.maximum_lateness_cycles,
                },
            ));
        }
        let lateness_cycles =
            u32::try_from(lateness).map_err(|_| FiniteDifferencePreflightError::Arithmetic)?;
        self.maximum_lateness_cycles = self.maximum_lateness_cycles.max(lateness_cycles);
        if output_at.is_none() {
            return self.complete_finite_difference_segment(active);
        }

        let mut event = StepperEvent {
            at: scheduled,
            direction_change: AxisMask::NONE,
            direction_positive: self.live.direction_positive,
            enable: AxisMask::NONE,
            disable: AxisMask::NONE,
            step_high: AxisMask::NONE,
            step_low: AxisMask::NONE,
        };
        if active.boundary_pending && active.start == scheduled {
            event.direction_change = active.direction_change;
            event.direction_positive = active.direction_positive;
            event.enable = active.enable;
            self.live.direction_known = self.live.direction_known.union(active.direction_change);
            self.live.direction_positive = active.direction_positive;
            self.live.enabled = self.live.enabled.union(active.enable);
            active.boundary_pending = false;
        }

        let mut axis = 0;
        while axis < AXES {
            if active.next_fall[axis] == Some(scheduled) {
                if !self.step_high.contains(axis) {
                    return Err(FiniteDifferencePreflightError::Timing(
                        MotionError::OutputInvariant,
                    ));
                }
                event
                    .step_low
                    .insert(axis)
                    .map_err(FiniteDifferencePreflightError::Timing)?;
                self.step_high
                    .remove(axis)
                    .map_err(FiniteDifferencePreflightError::Timing)?;
                self.live.last_fall[axis] = Some(scheduled);
                active.next_fall[axis] = None;
            }
            axis += 1;
        }

        let frame = if active.next_update == Some(scheduled) {
            let frame =
                active
                    .generator
                    .advance()?
                    .ok_or(FiniteDifferencePreflightError::Timing(
                        MotionError::OutputInvariant,
                    ))?;
            let frame_at = DeviceCycle(
                self.epoch
                    .0
                    .checked_add(frame.at.0)
                    .ok_or(FiniteDifferencePreflightError::Arithmetic)?,
            );
            if frame_at != scheduled {
                return Err(FiniteDifferencePreflightError::Timing(
                    MotionError::OutputInvariant,
                ));
            }
            active.next_update = if active.generator.remaining_updates() == 0 {
                None
            } else {
                Some(DeviceCycle(
                    scheduled
                        .0
                        .checked_add(u64::from(active.generator.segment.update_period_ticks))
                        .ok_or(FiniteDifferencePreflightError::Arithmetic)?,
                ))
            };

            axis = 0;
            while axis < AXES {
                let delta = frame.relative_steps[axis]
                    .checked_sub(self.live.relative_steps[axis])
                    .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
                if frame.step.contains(axis) != (delta != 0)
                    || !(-1..=1).contains(&delta)
                    || (delta > 0) != frame.direction_positive.contains(axis) && delta != 0
                {
                    return Err(FiniteDifferencePreflightError::Timing(
                        MotionError::OutputInvariant,
                    ));
                }
                if delta != 0 {
                    let positive = delta > 0;
                    if !self.live.enabled.contains(axis)
                        || !self.live.direction_known.contains(axis)
                        || self.live.direction_positive.contains(axis) != positive
                        || self.step_high.contains(axis)
                        || event.step_low.contains(axis)
                    {
                        return Err(FiniteDifferencePreflightError::Timing(
                            MotionError::OutputInvariant,
                        ));
                    }
                    event
                        .step_high
                        .insert(axis)
                        .map_err(FiniteDifferencePreflightError::Timing)?;
                    self.step_high
                        .insert(axis)
                        .map_err(FiniteDifferencePreflightError::Timing)?;
                    self.live.position[axis] = self.live.position[axis].checked_add(delta).ok_or(
                        FiniteDifferencePreflightError::PositionOverflow { index: 0, axis },
                    )?;
                    self.live.emitted_steps[axis] = self.live.emitted_steps[axis]
                        .checked_add(1)
                        .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
                    self.live.last_rise[axis] = Some(scheduled);
                    active.next_fall[axis] = Some(DeviceCycle(
                        scheduled
                            .0
                            .checked_add(u64::from(self.timing.axes[axis].pulse_high_cycles))
                            .ok_or(FiniteDifferencePreflightError::Arithmetic)?,
                    ));
                }
                axis += 1;
            }
            self.live.relative_steps = frame.relative_steps;
            self.live.finite_position = frame.finite_position;
            self.live.update_count = self
                .live
                .update_count
                .checked_add(1)
                .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
            Some(frame)
        } else {
            None
        };

        if frame.is_none() && event.is_empty() {
            return Err(FiniteDifferencePreflightError::Timing(
                MotionError::OutputInvariant,
            ));
        }
        let physical_event = (!event.is_empty()).then_some(event);
        self.active = Some(active);
        if let Some(frame) = frame {
            Ok(FiniteDifferenceMotionPoll::Update {
                frame,
                event: physical_event,
                lateness_cycles,
            })
        } else {
            Ok(FiniteDifferenceMotionPoll::Event {
                event: physical_event.expect("a non-update deadline must change outputs"),
                lateness_cycles,
            })
        }
    }

    fn complete_finite_difference_segment(
        &mut self,
        active: ActiveFiniteDifferenceSegment<AXES>,
    ) -> Result<FiniteDifferenceMotionPoll<AXES>, FiniteDifferencePreflightError> {
        if !self.step_high.is_empty()
            || active.boundary_pending
            || active.next_update.is_some()
            || active.next_fall.iter().any(Option::is_some)
        {
            return Err(FiniteDifferencePreflightError::Timing(
                MotionError::OutputInvariant,
            ));
        }
        let summary = active.generator.finish()?;
        self.live.next_tick = summary.end_tick;
        self.live.segment_count = self
            .live
            .segment_count
            .checked_add(1)
            .ok_or(FiniteDifferencePreflightError::Arithmetic)?;
        if self.live != active.terminal {
            return Err(FiniteDifferencePreflightError::Timing(
                MotionError::OutputInvariant,
            ));
        }
        self.state = ExecutorState::Ready;
        Ok(FiniteDifferenceMotionPoll::SegmentComplete(
            FiniteDifferenceSegmentCompletion {
                at: active.end,
                end_tick: summary.end_tick,
                delta_steps: summary.delta_steps,
                position: self.live.position,
                terminal_finite_position: summary.terminal_position,
                update_count: active.generator.segment.update_count,
            },
        ))
    }

    /// Earliest aligned cycle at which normal driver disable is legal.
    pub fn earliest_finish_cycle(&self) -> Result<DeviceCycle, FiniteDifferencePreflightError> {
        if self.state != ExecutorState::Ready || self.active.is_some() {
            return Err(FiniteDifferencePreflightError::Timing(MotionError::State));
        }
        self.live.earliest_finish_cycle(self.timing, self.epoch)
    }

    /// Emit the normal terminal disable transaction after all hold times.
    pub fn finish_job(
        &mut self,
        at: DeviceCycle,
    ) -> Result<StepperEvent, FiniteDifferencePreflightError> {
        if self.state != ExecutorState::Ready || self.active.is_some() {
            return Err(FiniteDifferencePreflightError::Timing(MotionError::State));
        }
        require_output_grid(at.0, self.timing.output_quantum_cycles)
            .map_err(FiniteDifferencePreflightError::Timing)?;
        let earliest = self.live.earliest_finish_cycle(self.timing, self.epoch)?;
        if at < earliest {
            let mut axis = 0;
            while axis < AXES {
                if self.live.enabled.contains(axis)
                    && let Some(last_fall) = self.live.last_fall[axis]
                    && at.0
                        < last_fall
                            .0
                            .checked_add(u64::from(self.timing.axes[axis].enable_hold_cycles))
                            .ok_or(FiniteDifferencePreflightError::Arithmetic)?
                {
                    return Err(FiniteDifferencePreflightError::Timing(
                        MotionError::EnableHold { axis },
                    ));
                }
                axis += 1;
            }
            return Err(FiniteDifferencePreflightError::Timing(
                MotionError::OutputInvariant,
            ));
        }
        let event = StepperEvent {
            at,
            direction_change: AxisMask::NONE,
            direction_positive: self.live.direction_positive,
            enable: AxisMask::NONE,
            disable: self.live.enabled,
            step_high: AxisMask::NONE,
            step_low: AxisMask::NONE,
        };
        self.live.enabled = AxisMask::NONE;
        self.state = ExecutorState::Complete;
        Ok(event)
    }

    /// Immediately lower every active pulse and disable every enabled axis.
    pub fn fault(&mut self, at: DeviceCycle) -> StepperEvent {
        let event = StepperEvent {
            at,
            direction_change: AxisMask::NONE,
            direction_positive: self.live.direction_positive,
            enable: AxisMask::NONE,
            disable: self.live.enabled,
            step_high: AxisMask::NONE,
            step_low: self.step_high,
        };
        self.step_high = AxisMask::NONE;
        self.live.enabled = AxisMask::NONE;
        self.active = None;
        self.state = ExecutorState::Faulted;
        event
    }

    /// Bounded logical state shared with ordinary step/direction telemetry.
    pub const fn status(&self) -> StepperStatus<AXES> {
        StepperStatus {
            state: self.state,
            epoch: self.epoch,
            next_tick: self.live.next_tick,
            position: self.live.position,
            direction_known: self.live.direction_known,
            direction_positive: self.live.direction_positive,
            enabled: self.live.enabled,
            step_high: self.step_high,
            completed_segments: self.live.segment_count,
            emitted_steps: self.live.emitted_steps,
            maximum_lateness_cycles: self.maximum_lateness_cycles,
            deadline_misses: self.deadline_misses,
        }
    }

    /// Exact stream-relative Q31.32 coordinate reached by dense recurrence.
    pub const fn finite_position(&self) -> [i64; AXES] {
        self.live.finite_position
    }

    fn preflight_snapshot(&self) -> Result<Self, FiniteDifferencePreflightError> {
        if self.state != ExecutorState::Ready || self.active.is_some() || !self.step_high.is_empty()
        {
            return Err(FiniteDifferencePreflightError::Timing(MotionError::State));
        }
        Ok(Self {
            timing: self.timing,
            limits: self.limits,
            state: self.state,
            epoch: self.epoch,
            live: self.live,
            step_high: self.step_high,
            active: None,
            maximum_lateness_cycles: self.maximum_lateness_cycles,
            deadline_misses: self.deadline_misses,
        })
    }

    fn preflight_complete_segment(&mut self) -> Result<(), FiniteDifferencePreflightError> {
        if self.state != ExecutorState::Segment || !self.step_high.is_empty() {
            return Err(FiniteDifferencePreflightError::Timing(MotionError::State));
        }
        let active = self
            .active
            .take()
            .ok_or(FiniteDifferencePreflightError::Timing(
                MotionError::OutputInvariant,
            ))?;
        self.live = active.terminal;
        self.state = ExecutorState::Ready;
        Ok(())
    }
}

/// One independently admitted finite-difference block rejected before the
/// dense executor took ownership.
pub struct RejectedFiniteDifferenceBlock<const AXES: usize> {
    error: CachedFiniteDifferenceError,
    admitted: AdmittedBlock<AXES>,
}

impl<const AXES: usize> RejectedFiniteDifferenceBlock<AXES> {
    /// Exact admission failure.
    pub const fn error(&self) -> CachedFiniteDifferenceError {
        self.error
    }

    /// Recover the unchanged unique job-actor token.
    pub fn into_block(self) -> AdmittedBlock<AXES> {
        self.admitted
    }
}

impl<const AXES: usize> core::fmt::Debug for RejectedFiniteDifferenceBlock<AXES> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("RejectedFiniteDifferenceBlock")
            .field("error", &self.error)
            .field("header", &self.admitted.header())
            .finish()
    }
}

/// Nonblocking result from one independently admitted direct-motion block.
#[allow(
    clippy::large_enum_variant,
    reason = "completion returns the unique fixed block token to its job actor"
)]
pub enum CachedFiniteDifferencePoll<const AXES: usize> {
    /// No block is owned.
    Idle,
    /// The next recurrence/output boundary is in the future.
    Future { at: DeviceCycle },
    /// One dense update was consumed, with an optional logical output change.
    Update {
        frame: FiniteDifferenceFrame<AXES>,
        event: Option<StepperEvent>,
        lateness_cycles: u32,
    },
    /// One direction/enable or pulse-fall transaction occurred between updates.
    Event {
        event: StepperEvent,
        lateness_cycles: u32,
    },
    /// The exact complete block trace has returned its unique ownership token.
    BlockComplete {
        admitted: AdmittedBlock<AXES>,
        completion: FiniteDifferenceSegmentCompletion<AXES>,
    },
}

/// Cached direct-motion correlation, validation, or execution failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CachedFiniteDifferenceError {
    /// Runner lifecycle or unique ownership was invalid.
    State,
    /// The admitted block did not expose the required record family.
    Block(BlockError),
    /// Sparse admission or dense recurrence/output execution failed.
    FiniteDifference(FiniteDifferencePreflightError),
    /// Declared and decoded record counts diverged.
    SegmentCount,
    /// Job admission progress and dense terminal facts diverged.
    Progress,
    /// Absolute command position overflowed.
    PositionOverflow { axis: usize },
    /// A bounded counter overflowed.
    Arithmetic,
}

/// Owns one direct finite-difference cached block until all dense updates and
/// logical pulse transactions match its independently admitted terminal facts.
pub struct CachedFiniteDifferenceExecutor<const AXES: usize> {
    stepper: FiniteDifferenceStepperExecutor<AXES>,
    admitted: Option<AdmittedBlock<AXES>>,
    next_segment: u32,
    segment_count: u32,
    origin: [i64; AXES],
    block_terminal_finite_position: [i64; AXES],
    faulted: bool,
    safe_transaction_issued: bool,
}

impl<const AXES: usize> CachedFiniteDifferenceExecutor<AXES> {
    /// Construct an inert cached runner with the exact direct-motion policy.
    pub fn new(
        timing: StepperTiming<AXES>,
        limits: FiniteDifferenceExecutionLimits,
    ) -> Result<Self, FiniteDifferencePreflightError> {
        Ok(Self {
            stepper: FiniteDifferenceStepperExecutor::new(timing, limits)?,
            admitted: None,
            next_segment: 0,
            segment_count: 0,
            origin: [0; AXES],
            block_terminal_finite_position: [0; AXES],
            faulted: false,
            safe_transaction_issued: false,
        })
    }

    /// Install the committed local epoch and current absolute lattice position.
    pub fn start_job(
        &mut self,
        epoch: DeviceCycle,
        position: [i64; AXES],
    ) -> Result<(), FiniteDifferencePreflightError> {
        if self.admitted.is_some() || self.faulted {
            return Err(FiniteDifferencePreflightError::Timing(MotionError::State));
        }
        self.stepper.start_job(epoch, position)?;
        self.origin = position;
        self.next_segment = 0;
        self.segment_count = 0;
        self.block_terminal_finite_position = [0; AXES];
        self.safe_transaction_issued = false;
        Ok(())
    }

    /// Sparse-preflight the complete block before installing its first dense
    /// record. Rejection returns the unchanged admission token.
    #[allow(
        clippy::result_large_err,
        reason = "rejection preserves unique inline block ownership"
    )]
    pub fn admit_block(
        &mut self,
        admitted: AdmittedBlock<AXES>,
    ) -> Result<(), RejectedFiniteDifferenceBlock<AXES>> {
        let (first, segment_count, terminal_finite_position) = match self.validate_block(&admitted)
        {
            Ok(validated) => validated,
            Err(error) => {
                return Err(RejectedFiniteDifferenceBlock { error, admitted });
            }
        };
        if let Err(error) = self.stepper.load_segment(first) {
            return Err(RejectedFiniteDifferenceBlock {
                error: CachedFiniteDifferenceError::FiniteDifference(error),
                admitted,
            });
        }
        self.admitted = Some(admitted);
        self.next_segment = 1;
        self.segment_count = segment_count;
        self.block_terminal_finite_position = terminal_finite_position;
        Ok(())
    }

    /// Consume at most one dense update or output boundary, retaining the
    /// block token through every final pulse fall and terminal comparison.
    pub fn poll(
        &mut self,
        observed: DeviceCycle,
    ) -> Result<CachedFiniteDifferencePoll<AXES>, CachedFiniteDifferenceError> {
        if self.faulted {
            return Err(CachedFiniteDifferenceError::State);
        }
        loop {
            let polled = match self.stepper.poll(observed) {
                Ok(polled) => polled,
                Err(error) => {
                    self.faulted = true;
                    return Err(CachedFiniteDifferenceError::FiniteDifference(error));
                }
            };
            match polled {
                FiniteDifferenceMotionPoll::Idle => {
                    if self.admitted.is_some() {
                        self.faulted = true;
                        return Err(CachedFiniteDifferenceError::State);
                    }
                    return Ok(CachedFiniteDifferencePoll::Idle);
                }
                FiniteDifferenceMotionPoll::Future { at } => {
                    return Ok(CachedFiniteDifferencePoll::Future { at });
                }
                FiniteDifferenceMotionPoll::Update {
                    frame,
                    event,
                    lateness_cycles,
                } => {
                    return Ok(CachedFiniteDifferencePoll::Update {
                        frame,
                        event,
                        lateness_cycles,
                    });
                }
                FiniteDifferenceMotionPoll::Event {
                    event,
                    lateness_cycles,
                } => {
                    return Ok(CachedFiniteDifferencePoll::Event {
                        event,
                        lateness_cycles,
                    });
                }
                FiniteDifferenceMotionPoll::SegmentComplete(completion) => {
                    if self.next_segment < self.segment_count {
                        let segment = match self.segment(self.next_segment) {
                            Ok(segment) => segment,
                            Err(error) => {
                                self.faulted = true;
                                return Err(error);
                            }
                        };
                        if let Err(error) = self.stepper.load_segment(segment) {
                            self.faulted = true;
                            return Err(CachedFiniteDifferenceError::FiniteDifference(error));
                        }
                        self.next_segment = match self.next_segment.checked_add(1) {
                            Some(next) => next,
                            None => {
                                self.faulted = true;
                                return Err(CachedFiniteDifferenceError::Arithmetic);
                            }
                        };
                        continue;
                    }
                    if self.next_segment != self.segment_count {
                        self.faulted = true;
                        return Err(CachedFiniteDifferenceError::SegmentCount);
                    }
                    let admitted = match self.admitted.as_ref() {
                        Some(admitted) => admitted,
                        None => {
                            self.faulted = true;
                            return Err(CachedFiniteDifferenceError::State);
                        }
                    };
                    let progress = admitted.progress();
                    let expected_position =
                        match finite_difference_absolute_position(self.origin, progress.position) {
                            Ok(position) => position,
                            Err(error) => {
                                self.faulted = true;
                                return Err(error);
                            }
                        };
                    let status = self.stepper.status();
                    if completion.end_tick != progress.end_tick
                        || completion.position != expected_position
                        || status.next_tick != progress.end_tick
                        || status.position != expected_position
                        || self.stepper.finite_position() != self.block_terminal_finite_position
                    {
                        self.faulted = true;
                        return Err(CachedFiniteDifferenceError::Progress);
                    }
                    let admitted = match self.admitted.take() {
                        Some(admitted) => admitted,
                        None => {
                            self.faulted = true;
                            return Err(CachedFiniteDifferenceError::State);
                        }
                    };
                    self.next_segment = 0;
                    self.segment_count = 0;
                    return Ok(CachedFiniteDifferencePoll::BlockComplete {
                        admitted,
                        completion,
                    });
                }
            }
        }
    }

    /// Exact next dense update, output, or segment-horizon deadline.
    pub fn next_deadline(&self) -> Option<DeviceCycle> {
        if self.faulted {
            None
        } else {
            self.stepper.next_deadline()
        }
    }

    /// Earliest normal terminal-disable cycle after block ownership returns.
    pub fn earliest_finish_cycle(&self) -> Result<DeviceCycle, FiniteDifferencePreflightError> {
        if self.admitted.is_some() || self.faulted {
            return Err(FiniteDifferencePreflightError::Timing(MotionError::State));
        }
        self.stepper.earliest_finish_cycle()
    }

    /// Complete a direct job after its final admitted block was acknowledged.
    pub fn finish_job(
        &mut self,
        at: DeviceCycle,
    ) -> Result<StepperEvent, FiniteDifferencePreflightError> {
        if self.admitted.is_some() || self.faulted {
            return Err(FiniteDifferencePreflightError::Timing(MotionError::State));
        }
        self.stepper.finish_job(at)
    }

    /// Latch execution and return the immediate logical safe transaction.
    pub fn fault(&mut self, at: DeviceCycle) -> StepperEvent {
        self.faulted = true;
        self.safe_transaction_issued = true;
        self.stepper.fault(at)
    }

    /// Release the unacknowledgeable token only after requesting safe outputs.
    pub fn take_faulted_block(&mut self) -> Option<AdmittedBlock<AXES>> {
        if self.faulted
            && self.safe_transaction_issued
            && self.stepper.status().state == ExecutorState::Faulted
        {
            self.admitted.take()
        } else {
            None
        }
    }

    /// Bounded logical executor status.
    pub const fn status(&self) -> StepperStatus<AXES> {
        self.stepper.status()
    }

    /// Exact stream-relative Q31.32 coordinate reached so far.
    pub const fn finite_position(&self) -> [i64; AXES] {
        self.stepper.finite_position()
    }

    /// Whether the runner currently owns a block token.
    pub const fn has_admitted_block(&self) -> bool {
        self.admitted.is_some()
    }

    fn validate_block(
        &self,
        admitted: &AdmittedBlock<AXES>,
    ) -> Result<(FiniteDifferenceSegment<AXES>, u32, [i64; AXES]), CachedFiniteDifferenceError>
    {
        if self.faulted
            || self.admitted.is_some()
            || self.stepper.status().state != ExecutorState::Ready
        {
            return Err(CachedFiniteDifferenceError::State);
        }
        let header = admitted.header();
        let progress = admitted.progress();
        if header.segment_count == 0
            || header.start_tick != self.stepper.status().next_tick
            || header.end_tick != progress.end_tick
        {
            return Err(CachedFiniteDifferenceError::Progress);
        }
        let mut segments = admitted
            .finite_difference_segments()
            .map_err(CachedFiniteDifferenceError::Block)?;
        let first = segments
            .next()
            .ok_or(CachedFiniteDifferenceError::SegmentCount)?;
        let mut preflight = self
            .stepper
            .preflight_snapshot()
            .map_err(CachedFiniteDifferenceError::FiniteDifference)?;
        preflight
            .load_segment(first)
            .map_err(CachedFiniteDifferenceError::FiniteDifference)?;
        preflight
            .preflight_complete_segment()
            .map_err(CachedFiniteDifferenceError::FiniteDifference)?;
        let mut count = 1_u32;
        for segment in segments {
            preflight
                .load_segment(segment)
                .map_err(CachedFiniteDifferenceError::FiniteDifference)?;
            preflight
                .preflight_complete_segment()
                .map_err(CachedFiniteDifferenceError::FiniteDifference)?;
            count = count
                .checked_add(1)
                .ok_or(CachedFiniteDifferenceError::Arithmetic)?;
        }
        let expected_position =
            finite_difference_absolute_position(self.origin, progress.position)?;
        let status = preflight.status();
        if count != header.segment_count
            || status.next_tick != progress.end_tick
            || status.position != expected_position
        {
            return Err(CachedFiniteDifferenceError::Progress);
        }
        Ok((first, count, preflight.finite_position()))
    }

    fn segment(
        &self,
        index: u32,
    ) -> Result<FiniteDifferenceSegment<AXES>, CachedFiniteDifferenceError> {
        let admitted = self
            .admitted
            .as_ref()
            .ok_or(CachedFiniteDifferenceError::State)?;
        let index = usize::try_from(index).map_err(|_| CachedFiniteDifferenceError::Arithmetic)?;
        admitted
            .finite_difference_segments()
            .map_err(CachedFiniteDifferenceError::Block)?
            .nth(index)
            .ok_or(CachedFiniteDifferenceError::SegmentCount)
    }
}

fn finite_difference_absolute_position<const AXES: usize>(
    origin: [i64; AXES],
    displacement: [i64; AXES],
) -> Result<[i64; AXES], CachedFiniteDifferenceError> {
    let mut position = origin;
    let mut axis = 0;
    while axis < AXES {
        position[axis] = position[axis]
            .checked_add(displacement[axis])
            .ok_or(CachedFiniteDifferenceError::PositionOverflow { axis })?;
        axis += 1;
    }
    Ok(position)
}

/// Owns one independently admitted cached block until every segment has
/// produced its exact logical event trace. The block token is returned only
/// after its terminal tick and cumulative lattice position agree with the
/// independent stream validator.
pub struct CachedStepperExecutor<const AXES: usize> {
    stepper: StepperExecutor<AXES>,
    admitted: Option<AdmittedBlock<AXES>>,
    next_segment: u32,
    segment_count: u32,
    origin: [i64; AXES],
    faulted: bool,
    safe_transaction_issued: bool,
}

impl<const AXES: usize> CachedStepperExecutor<AXES> {
    /// Constructs an inert cached-stream runner with the same electrical
    /// validation contract as [`StepperExecutor`].
    pub fn new(timing: StepperTiming<AXES>) -> Result<Self, MotionError> {
        Ok(Self {
            stepper: StepperExecutor::new(timing)?,
            admitted: None,
            next_segment: 0,
            segment_count: 0,
            origin: [0; AXES],
            faulted: false,
            safe_transaction_issued: false,
        })
    }

    /// Installs the deterministic local epoch and current absolute lattice
    /// position before any cached block is accepted.
    pub fn start_job(
        &mut self,
        epoch: DeviceCycle,
        position: [i64; AXES],
    ) -> Result<(), MotionError> {
        if self.admitted.is_some() || self.faulted {
            return Err(MotionError::State);
        }
        self.stepper.start_job(epoch, position)?;
        self.origin = position;
        self.next_segment = 0;
        self.segment_count = 0;
        self.safe_transaction_issued = false;
        Ok(())
    }

    /// Preflights every segment and retains one admitted block, installing only
    /// its first segment in the live executor. A rejection returns ownership of
    /// the unchanged block and leaves the live executor exactly as it was
    /// before this call.
    #[allow(
        clippy::result_large_err,
        reason = "rejection must return the unique inline-owned block without allocation or aliasing"
    )]
    pub fn admit_block(
        &mut self,
        admitted: AdmittedBlock<AXES>,
    ) -> Result<(), RejectedMotionBlock<AXES>> {
        let (first, segment_count) = match self.validate_block(&admitted) {
            Ok(validated) => validated,
            Err(error) => return Err(RejectedMotionBlock { error, admitted }),
        };
        if let Err(error) = self.stepper.load_segment(first) {
            return Err(RejectedMotionBlock {
                error: CachedMotionError::Motion(error),
                admitted,
            });
        }
        self.admitted = Some(admitted);
        self.next_segment = 1;
        self.segment_count = segment_count;
        Ok(())
    }

    /// Polls one exact deadline. Segment boundaries are internal; when two
    /// contiguous segments meet, the next boundary transaction may be returned
    /// from this same call. The admitted token leaves this owner only after the
    /// complete block trace matches its independently validated progress.
    pub fn poll(
        &mut self,
        observed: DeviceCycle,
    ) -> Result<CachedMotionPoll<AXES>, CachedMotionError> {
        if self.faulted {
            return Err(CachedMotionError::State);
        }
        loop {
            let polled = match self.stepper.poll(observed) {
                Ok(polled) => polled,
                Err(error) => {
                    self.faulted = true;
                    return Err(CachedMotionError::Motion(error));
                }
            };
            match polled {
                MotionPoll::Idle => {
                    if self.admitted.is_some() {
                        self.faulted = true;
                        return Err(CachedMotionError::State);
                    }
                    return Ok(CachedMotionPoll::Idle);
                }
                MotionPoll::Future { at } => return Ok(CachedMotionPoll::Future { at }),
                MotionPoll::Event {
                    event,
                    lateness_cycles,
                } => {
                    return Ok(CachedMotionPoll::Event {
                        event,
                        lateness_cycles,
                    });
                }
                MotionPoll::SegmentComplete(completion) => {
                    if self.next_segment < self.segment_count {
                        let segment = match self.segment(self.next_segment) {
                            Ok(segment) => segment,
                            Err(error) => {
                                self.faulted = true;
                                return Err(error);
                            }
                        };
                        if let Err(error) = self.stepper.load_segment(segment) {
                            self.faulted = true;
                            return Err(CachedMotionError::Motion(error));
                        }
                        self.next_segment = match self.next_segment.checked_add(1) {
                            Some(next) => next,
                            None => {
                                self.faulted = true;
                                return Err(CachedMotionError::Arithmetic);
                            }
                        };
                        continue;
                    }
                    if self.next_segment != self.segment_count {
                        self.faulted = true;
                        return Err(CachedMotionError::SegmentCount);
                    }
                    let admitted = match self.admitted.as_ref() {
                        Some(admitted) => admitted,
                        None => {
                            self.faulted = true;
                            return Err(CachedMotionError::State);
                        }
                    };
                    let progress = admitted.progress();
                    let expected_position = match absolute_position(self.origin, progress.position)
                    {
                        Ok(position) => position,
                        Err(error) => {
                            self.faulted = true;
                            return Err(error);
                        }
                    };
                    if completion.end_tick != progress.end_tick
                        || completion.position != expected_position
                        || self.stepper.status().next_tick != progress.end_tick
                    {
                        self.faulted = true;
                        return Err(CachedMotionError::Progress);
                    }
                    let admitted = match self.admitted.take() {
                        Some(admitted) => admitted,
                        None => {
                            self.faulted = true;
                            return Err(CachedMotionError::State);
                        }
                    };
                    self.next_segment = 0;
                    self.segment_count = 0;
                    return Ok(CachedMotionPoll::BlockComplete {
                        admitted,
                        completion,
                    });
                }
            }
        }
    }

    /// Exact next event or segment-horizon deadline.
    pub fn next_deadline(&self) -> Option<DeviceCycle> {
        if self.faulted {
            None
        } else {
            self.stepper.next_deadline()
        }
    }

    /// Exact normal-disable deadline after the terminal block is returned.
    pub fn earliest_finish_cycle(&self) -> Result<DeviceCycle, MotionError> {
        if self.admitted.is_some() || self.faulted {
            return Err(MotionError::State);
        }
        self.stepper.earliest_finish_cycle()
    }

    /// Completes a job only after the admitted block token has been returned to
    /// its job actor and the configured driver hold time has elapsed.
    pub fn finish_job(&mut self, at: DeviceCycle) -> Result<StepperEvent, MotionError> {
        if self.admitted.is_some() || self.faulted {
            return Err(MotionError::State);
        }
        self.stepper.finish_job(at)
    }

    /// Latches the runner and returns the immediate logical safe transaction.
    /// The caller must apply this transaction to the hardware backend before
    /// reporting safe outputs.
    pub fn fault(&mut self, at: DeviceCycle) -> StepperEvent {
        self.faulted = true;
        self.safe_transaction_issued = true;
        self.stepper.fault(at)
    }

    /// Releases an unacknowledgeable block only after [`Self::fault`] has issued
    /// the safe output transaction. The caller remains responsible for applying
    /// that returned transaction before calling this method and for faulting the
    /// corresponding job actor rather than acknowledging this block.
    pub fn take_faulted_block(&mut self) -> Option<AdmittedBlock<AXES>> {
        if self.faulted
            && self.safe_transaction_issued
            && self.stepper.status().state == ExecutorState::Faulted
        {
            self.admitted.take()
        } else {
            None
        }
    }

    /// Bounded exact executor status.
    pub const fn status(&self) -> StepperStatus<AXES> {
        self.stepper.status()
    }

    /// Whether this owner currently holds a block that the job actor must not
    /// acknowledge or replace.
    pub const fn has_admitted_block(&self) -> bool {
        self.admitted.is_some()
    }

    /// Canonical logical executor snapshot. Backend telemetry must separately
    /// prove whether a returned output transaction was physically applied.
    pub fn report(&self) -> Result<RealtimeMotionReport, MotionReportError> {
        RealtimeMotionReport::from_executor(&self.stepper)
    }

    fn validate_block(
        &self,
        admitted: &AdmittedBlock<AXES>,
    ) -> Result<(ExecutionSegment<AXES>, u32), CachedMotionError> {
        if self.faulted
            || self.admitted.is_some()
            || self.stepper.status().state != ExecutorState::Ready
        {
            return Err(CachedMotionError::State);
        }
        let header = admitted.header();
        let progress = admitted.progress();
        if header.segment_count == 0
            || header.start_tick != self.stepper.status().next_tick
            || header.end_tick != progress.end_tick
        {
            return Err(CachedMotionError::Progress);
        }
        let mut segments = admitted.segments().map_err(CachedMotionError::Block)?;
        let first = segments.next().ok_or(CachedMotionError::SegmentCount)?;
        let mut preflight = self.stepper.preflight_snapshot();
        let mut count = 1_u32;
        preflight_segment(&mut preflight, first)?;
        for segment in segments {
            preflight_segment(&mut preflight, segment)?;
            count = count.checked_add(1).ok_or(CachedMotionError::Arithmetic)?;
        }
        let cumulative = absolute_position(self.origin, progress.position)?;
        let preflight_status = preflight.status();
        if count != header.segment_count
            || preflight_status.next_tick != progress.end_tick
            || preflight_status.position != cumulative
        {
            return Err(CachedMotionError::Progress);
        }
        Ok((first, count))
    }

    fn segment(&self, index: u32) -> Result<ExecutionSegment<AXES>, CachedMotionError> {
        let admitted = self.admitted.as_ref().ok_or(CachedMotionError::State)?;
        let index = usize::try_from(index).map_err(|_| CachedMotionError::Arithmetic)?;
        admitted
            .segments()
            .map_err(CachedMotionError::Block)?
            .nth(index)
            .ok_or(CachedMotionError::SegmentCount)
    }
}

fn preflight_segment<const AXES: usize>(
    executor: &mut StepperExecutor<AXES>,
    segment: ExecutionSegment<AXES>,
) -> Result<(), CachedMotionError> {
    executor
        .load_segment(segment)
        .map_err(CachedMotionError::Motion)?;
    executor
        .preflight_complete_segment()
        .map_err(CachedMotionError::Motion)
}

/// A block rejected before ownership entered the cached executor.
pub struct RejectedMotionBlock<const AXES: usize> {
    error: CachedMotionError,
    admitted: AdmittedBlock<AXES>,
}

impl<const AXES: usize> RejectedMotionBlock<AXES> {
    pub const fn error(&self) -> CachedMotionError {
        self.error
    }

    pub fn into_block(self) -> AdmittedBlock<AXES> {
        self.admitted
    }
}

impl<const AXES: usize> core::fmt::Debug for RejectedMotionBlock<AXES> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("RejectedMotionBlock")
            .field("error", &self.error)
            .field("header", &self.admitted.header())
            .finish()
    }
}

/// Nonblocking result from executing one independently admitted cached block.
#[allow(
    clippy::large_enum_variant,
    reason = "completion transfers the unique inline-owned block back to its job actor"
)]
pub enum CachedMotionPoll<const AXES: usize> {
    Idle,
    Future {
        at: DeviceCycle,
    },
    Event {
        event: StepperEvent,
        lateness_cycles: u32,
    },
    BlockComplete {
        admitted: AdmittedBlock<AXES>,
        completion: SegmentCompletion<AXES>,
    },
}

/// Cached-block/executor correlation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CachedMotionError {
    State,
    Block(BlockError),
    Motion(MotionError),
    SegmentCount,
    Progress,
    PositionOverflow { axis: usize },
    Arithmetic,
}

/// Unforgeable boot-local correlation for one complete-image transaction.
/// Tokens are deliberately process-local: the hardware owner consumes them
/// immediately and only the resulting bounded report crosses a core boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputCommitToken(u32);

impl OutputCommitToken {
    /// Nonzero diagnostic representation.
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// One logical event translated to a complete shifted image, but not yet
/// acknowledged as physically committed by the sole output owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PendingShiftOutput {
    pub token: OutputCommitToken,
    pub update: ShiftImageUpdate,
    /// Software lateness observed before the backend transaction began.
    pub generation_lateness_cycles: u32,
}

/// Exact upper-bound observation returned after one image commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommittedShiftOutput {
    pub token: OutputCommitToken,
    pub update: ShiftImageUpdate,
    pub committed_at: DeviceCycle,
    /// Total lateness from the planned image-commit cycle.
    pub commit_lateness_cycles: u32,
}

/// Construction failure before any cached block or output image is owned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShiftedExecutorBuildError {
    Motion(MotionError),
    Image(ShiftImageError),
    /// A scheduled backend requested no storage for its required output ring.
    HorizonCapacity,
}

/// Fail-closed generated-output or image-mapping failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShiftedMotionError {
    State,
    Cached(CachedMotionError),
    Motion(MotionError),
    Image(ShiftImageError),
}

/// A physical commit observation that cannot correspond to the sole pending
/// output transaction. Any rejection latches the coordinator until
/// [`ShiftedCachedStepper::fault`] returns the complete safe image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputCommitError {
    State,
    Token {
        expected: OutputCommitToken,
        received: OutputCommitToken,
    },
    Early {
        scheduled: DeviceCycle,
        committed: DeviceCycle,
    },
    Deadline {
        scheduled: DeviceCycle,
        committed: DeviceCycle,
        maximum_lateness_cycles: u32,
    },
    Arithmetic,
}

/// Nonblocking result from the generated/physically-committed output boundary.
#[allow(
    clippy::large_enum_variant,
    reason = "block completion returns unique inline ownership to the job actor"
)]
pub enum ShiftedMotionPoll<const AXES: usize> {
    Idle,
    Future {
        at: DeviceCycle,
    },
    /// A new image transaction that the hardware owner must apply exactly once.
    Output(PendingShiftOutput),
    /// The prior output has not been committed; it must never be applied again.
    AwaitingCommit(PendingShiftOutput),
    BlockComplete {
        admitted: AdmittedBlock<AXES>,
        completion: SegmentCompletion<AXES>,
    },
}

/// Couples cached exact event generation to a two-phase complete-image commit.
///
/// The logical executor may advance far enough to construct one image, but it
/// cannot generate another event, complete a block, or release its unique token
/// until the target backend reports the first image's physical commit cycle.
/// Invalid or late acknowledgements latch the coordinator and require the
/// caller to apply the complete safe image returned by [`Self::fault`].
pub struct ShiftedCachedStepper<const AXES: usize> {
    cached: CachedStepperExecutor<AXES>,
    mapper: ShiftImageMapper<AXES>,
    pending: Option<PendingShiftOutput>,
    next_token: u32,
    maximum_commit_lateness_cycles: u32,
    committed_updates: u64,
    maximum_commit_lateness_observed: u32,
    output_faulted: bool,
}

impl<const AXES: usize> ShiftedCachedStepper<AXES> {
    /// Binds one independently derived execution profile to its only complete
    /// shifted-image engine.
    pub fn new(
        profile: StepperExecutionProfile<AXES>,
        contract: ShiftImageContract,
    ) -> Result<Self, ShiftedExecutorBuildError> {
        let cached = CachedStepperExecutor::new(profile.timing)
            .map_err(ShiftedExecutorBuildError::Motion)?;
        let mapper =
            ShiftImageMapper::new(&profile, contract).map_err(ShiftedExecutorBuildError::Image)?;
        Ok(Self {
            cached,
            mapper,
            pending: None,
            next_token: 0,
            maximum_commit_lateness_cycles: profile.timing.maximum_lateness_cycles,
            committed_updates: 0,
            maximum_commit_lateness_observed: 0,
            output_faulted: false,
        })
    }

    /// Installs the exact local epoch while the physical image remains safe.
    pub fn start_job(
        &mut self,
        epoch: DeviceCycle,
        position: [i64; AXES],
    ) -> Result<(), ShiftedMotionError> {
        if self.output_faulted || self.pending.is_some() {
            return Err(ShiftedMotionError::State);
        }
        self.cached
            .start_job(epoch, position)
            .map_err(ShiftedMotionError::Motion)
    }

    /// Transfers one independently admitted block into the exact generator.
    #[allow(
        clippy::result_large_err,
        reason = "rejection preserves unique inline block ownership"
    )]
    pub fn admit_block(
        &mut self,
        admitted: AdmittedBlock<AXES>,
    ) -> Result<(), RejectedMotionBlock<AXES>> {
        if self.output_faulted || self.pending.is_some() {
            return Err(RejectedMotionBlock {
                error: CachedMotionError::State,
                admitted,
            });
        }
        self.cached.admit_block(admitted)
    }

    /// Generates at most one new complete image. A pending image is reported
    /// distinctly and never regenerated or reapplied.
    pub fn poll(
        &mut self,
        observed: DeviceCycle,
    ) -> Result<ShiftedMotionPoll<AXES>, ShiftedMotionError> {
        if self.output_faulted {
            return Err(ShiftedMotionError::State);
        }
        if let Some(pending) = self.pending {
            return Ok(ShiftedMotionPoll::AwaitingCommit(pending));
        }
        match self
            .cached
            .poll(observed)
            .map_err(ShiftedMotionError::Cached)?
        {
            CachedMotionPoll::Idle => Ok(ShiftedMotionPoll::Idle),
            CachedMotionPoll::Future { at } => Ok(ShiftedMotionPoll::Future { at }),
            CachedMotionPoll::Event {
                event,
                lateness_cycles,
            } => {
                let update = match self.mapper.apply(event) {
                    Ok(update) => update,
                    Err(error) => {
                        self.output_faulted = true;
                        return Err(ShiftedMotionError::Image(error));
                    }
                };
                self.next_token = next_output_token(self.next_token);
                let pending = PendingShiftOutput {
                    token: OutputCommitToken(self.next_token),
                    update,
                    generation_lateness_cycles: lateness_cycles,
                };
                self.pending = Some(pending);
                Ok(ShiftedMotionPoll::Output(pending))
            }
            CachedMotionPoll::BlockComplete {
                admitted,
                completion,
            } => Ok(ShiftedMotionPoll::BlockComplete {
                admitted,
                completion,
            }),
        }
    }

    /// Acknowledges the upper-bound cycle at which the target backend finished
    /// committing the sole pending image. Early, wrong-token, or late reports
    /// latch the coordinator without clearing the transaction.
    pub fn commit_output(
        &mut self,
        token: OutputCommitToken,
        committed_at: DeviceCycle,
    ) -> Result<CommittedShiftOutput, OutputCommitError> {
        if self.output_faulted {
            return Err(OutputCommitError::State);
        }
        let pending = match self.pending {
            Some(pending) => pending,
            None => {
                self.output_faulted = true;
                return Err(OutputCommitError::State);
            }
        };
        if token != pending.token {
            self.output_faulted = true;
            return Err(OutputCommitError::Token {
                expected: pending.token,
                received: token,
            });
        }
        if committed_at < pending.update.at {
            self.output_faulted = true;
            return Err(OutputCommitError::Early {
                scheduled: pending.update.at,
                committed: committed_at,
            });
        }
        let lateness = committed_at.0 - pending.update.at.0;
        if lateness > u64::from(self.maximum_commit_lateness_cycles) {
            self.output_faulted = true;
            return Err(OutputCommitError::Deadline {
                scheduled: pending.update.at,
                committed: committed_at,
                maximum_lateness_cycles: self.maximum_commit_lateness_cycles,
            });
        }
        let lateness = match u32::try_from(lateness) {
            Ok(lateness) => lateness,
            Err(_) => {
                self.output_faulted = true;
                return Err(OutputCommitError::Arithmetic);
            }
        };
        self.committed_updates = match self.committed_updates.checked_add(1) {
            Some(updates) => updates,
            None => {
                self.output_faulted = true;
                return Err(OutputCommitError::Arithmetic);
            }
        };
        self.maximum_commit_lateness_observed = self.maximum_commit_lateness_observed.max(lateness);
        self.pending = None;
        Ok(CommittedShiftOutput {
            token,
            update: pending.update,
            committed_at,
            commit_lateness_cycles: lateness,
        })
    }

    /// Produces and retains the normal terminal disable image. The result uses
    /// the same two-phase physical commit boundary as a step edge.
    pub fn finish_job(
        &mut self,
        at: DeviceCycle,
    ) -> Result<PendingShiftOutput, ShiftedMotionError> {
        if self.output_faulted || self.pending.is_some() {
            return Err(ShiftedMotionError::State);
        }
        let event = self
            .cached
            .finish_job(at)
            .map_err(ShiftedMotionError::Motion)?;
        let update = match self.mapper.apply(event) {
            Ok(update) => update,
            Err(error) => {
                self.output_faulted = true;
                return Err(ShiftedMotionError::Image(error));
            }
        };
        self.next_token = next_output_token(self.next_token);
        let pending = PendingShiftOutput {
            token: OutputCommitToken(self.next_token),
            update,
            generation_lateness_cycles: 0,
        };
        self.pending = Some(pending);
        Ok(pending)
    }

    /// Latches logical execution and returns the complete safe image for an
    /// immediate local hardware transaction. This invalidates every pending
    /// output token; it does not claim that the returned image was applied.
    pub fn fault(&mut self, at: DeviceCycle) -> ShiftImageUpdate {
        self.output_faulted = true;
        self.pending = None;
        let _ = self.cached.fault(at);
        self.mapper.force_safe(at)
    }

    /// Releases an unacknowledgeable block after the caller has applied the
    /// safe image returned by [`Self::fault`].
    pub fn take_faulted_block(&mut self) -> Option<AdmittedBlock<AXES>> {
        self.cached.take_faulted_block()
    }

    /// Exact next generator deadline. A pending transaction retains its own
    /// scheduled cycle until physically committed.
    pub fn next_deadline(&self) -> Option<DeviceCycle> {
        self.pending
            .map(|pending| pending.update.at)
            .or_else(|| self.cached.next_deadline())
    }

    /// Exact cycle at which a requested normal terminal disable may commit.
    pub fn earliest_finish_cycle(&self) -> Result<DeviceCycle, MotionError> {
        if self.output_faulted || self.pending.is_some() {
            return Err(MotionError::State);
        }
        self.cached.earliest_finish_cycle()
    }

    /// Bounded logical executor status.
    pub const fn status(&self) -> StepperStatus<AXES> {
        self.cached.status()
    }

    /// Number of target-confirmed complete-image updates.
    pub const fn committed_updates(&self) -> u64 {
        self.committed_updates
    }

    /// Largest exact target-reported image commit lateness.
    pub const fn maximum_commit_lateness_observed(&self) -> u32 {
        self.maximum_commit_lateness_observed
    }

    /// Current mapped complete image, including a pending logical update.
    pub const fn image(&self) -> u32 {
        self.mapper.image()
    }
}

/// One future complete-image transaction generated on its exact schedule but
/// not necessarily staged in or observed from a hardware timing engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledShiftOutput {
    /// Boot-local ordered correlation token.
    pub token: OutputCommitToken,
    /// Exact complete image and intended physical latch cycle.
    pub update: ShiftImageUpdate,
}

/// Result of advancing a scheduled complete-image producer toward a requested
/// future cycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduledShiftPlan {
    /// No cached block currently owns the logical executor.
    Idle,
    /// The requested planning boundary was reached before the next event.
    Future { at: DeviceCycle },
    /// The fixed output ring must be drained before generation can continue.
    HorizonFull {
        next_at: DeviceCycle,
        queued: usize,
        staged: usize,
    },
    /// One block's complete logical trace is planned. Its unique ownership is
    /// retained behind a per-block physical-commit barrier while a successor
    /// may enter the bounded planning window.
    BlockPlanned {
        sequence: u32,
        completion_at: DeviceCycle,
    },
}

/// A cached block whose complete-image transactions have all been physically
/// acknowledged in order.
pub struct ScheduledBlockCompletion<const AXES: usize> {
    admitted: AdmittedBlock<AXES>,
    completion: SegmentCompletion<AXES>,
}

impl<const AXES: usize> ScheduledBlockCompletion<AXES> {
    /// Independently correlated terminal progress for the block.
    pub const fn completion(&self) -> SegmentCompletion<AXES> {
        self.completion
    }

    /// Returns the unique admitted block to its owning job actor.
    pub fn into_block(self) -> AdmittedBlock<AXES> {
        self.admitted
    }

    /// Returns both the unique token and its exact terminal progress.
    pub fn into_parts(self) -> (AdmittedBlock<AXES>, SegmentCompletion<AXES>) {
        (self.admitted, self.completion)
    }
}

impl<const AXES: usize> core::fmt::Debug for ScheduledBlockCompletion<AXES> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ScheduledBlockCompletion")
            .field("header", &self.admitted.header())
            .field("completion", &self.completion)
            .finish()
    }
}

/// A logically complete block and the exact generated-update prefix that must
/// be physically observed before its unique admission token may be returned.
struct ScheduledBlockBarrier<const AXES: usize> {
    completed: ScheduledBlockCompletion<AXES>,
    required_committed_updates: u64,
}

/// Failure while advancing the future logical event/image plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduledShiftError {
    State,
    Cached(CachedMotionError),
    Motion(MotionError),
    Image(ShiftImageError),
    /// Two complete images cannot occupy the same hardware lattice boundary.
    OutputOrder {
        previous: DeviceCycle,
        received: DeviceCycle,
    },
    Arithmetic,
}

/// A hardware-staging acknowledgement did not match the next generated image.
/// Every rejection latches the scheduled backend until its safe image is
/// requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputStageError {
    State,
    Mismatch {
        expected: ScheduledShiftOutput,
        received: ScheduledShiftOutput,
    },
    Arithmetic,
}

/// Bounded future-output coordinator for timer/DMA-backed shift engines.
///
/// Logical generation, hardware staging, and physical latch observation are
/// three distinct monotonically ordered transitions. Generation may run ahead
/// to fill `OUTPUTS` and may cross one block boundary, but each cached block
/// returns only after its own generated prefix is committed and its terminal
/// cycle is observed. This owner is distinct
/// from [`ShiftedCachedStepper`], whose single synchronous transaction policy is
/// useful for GPIO/bootstrap backends but cannot maintain a DMA horizon.
pub struct ScheduledShiftedStepper<const AXES: usize, const OUTPUTS: usize> {
    cached: CachedStepperExecutor<AXES>,
    mapper: ShiftImageMapper<AXES>,
    outputs: [Option<ScheduledShiftOutput>; OUTPUTS],
    head: usize,
    len: usize,
    staged: usize,
    completed: [Option<ScheduledBlockBarrier<AXES>>; REALTIME_BLOCK_WINDOW],
    completed_head: usize,
    completed_len: usize,
    faulted_block: Option<AdmittedBlock<AXES>>,
    next_token: u32,
    output_quantum_cycles: u32,
    last_generated_at: Option<DeviceCycle>,
    maximum_commit_lateness_cycles: u32,
    committed_updates: u64,
    maximum_commit_lateness_observed: u32,
    finish_token: Option<OutputCommitToken>,
    job_completion_pending: bool,
    output_faulted: bool,
}

impl<const AXES: usize, const OUTPUTS: usize> ScheduledShiftedStepper<AXES, OUTPUTS> {
    /// Binds one exact executor/image profile to a nonempty future-output ring.
    pub fn new(
        profile: StepperExecutionProfile<AXES>,
        contract: ShiftImageContract,
    ) -> Result<Self, ShiftedExecutorBuildError> {
        if OUTPUTS == 0 {
            return Err(ShiftedExecutorBuildError::HorizonCapacity);
        }
        let cached = CachedStepperExecutor::new(profile.timing)
            .map_err(ShiftedExecutorBuildError::Motion)?;
        let mapper =
            ShiftImageMapper::new(&profile, contract).map_err(ShiftedExecutorBuildError::Image)?;
        Ok(Self {
            cached,
            mapper,
            outputs: [None; OUTPUTS],
            head: 0,
            len: 0,
            staged: 0,
            completed: [const { None }; REALTIME_BLOCK_WINDOW],
            completed_head: 0,
            completed_len: 0,
            faulted_block: None,
            next_token: 0,
            output_quantum_cycles: profile.timing.output_quantum_cycles,
            last_generated_at: None,
            maximum_commit_lateness_cycles: profile.timing.maximum_lateness_cycles,
            committed_updates: 0,
            maximum_commit_lateness_observed: 0,
            finish_token: None,
            job_completion_pending: false,
            output_faulted: false,
        })
    }

    /// Installs one exact future local epoch while the physical image remains
    /// under a separately established safe stream.
    pub fn start_job(
        &mut self,
        epoch: DeviceCycle,
        position: [i64; AXES],
    ) -> Result<(), ScheduledShiftError> {
        if self.output_faulted
            || self.len != 0
            || self.completed_len != 0
            || self.faulted_block.is_some()
            || self.finish_token.is_some()
            || self.job_completion_pending
        {
            return Err(ScheduledShiftError::State);
        }
        self.cached
            .start_job(epoch, position)
            .map_err(ScheduledShiftError::Motion)?;
        self.last_generated_at = None;
        Ok(())
    }

    /// Transfers one independently admitted block into the future generator.
    #[allow(
        clippy::result_large_err,
        reason = "rejection preserves unique inline block ownership"
    )]
    pub fn admit_block(
        &mut self,
        admitted: AdmittedBlock<AXES>,
    ) -> Result<(), RejectedMotionBlock<AXES>> {
        if self.output_faulted
            || self.completed_len == REALTIME_BLOCK_WINDOW
            || self.faulted_block.is_some()
            || self.finish_token.is_some()
            || self.job_completion_pending
        {
            return Err(RejectedMotionBlock {
                error: CachedMotionError::State,
                admitted,
            });
        }
        self.cached.admit_block(admitted)
    }

    /// Generates every exact output due no later than `through`, stopping at a
    /// full fixed ring or the logical block boundary. Generation is evaluated
    /// at each event's scheduled cycle and therefore records no fictitious
    /// software lateness; the target's later commit observation is authoritative.
    pub fn plan_through(
        &mut self,
        through: DeviceCycle,
    ) -> Result<ScheduledShiftPlan, ScheduledShiftError> {
        if self.output_faulted || self.finish_token.is_some() || self.job_completion_pending {
            return Err(ScheduledShiftError::State);
        }
        if !self.cached.has_admitted_block()
            && let Some(completed) = self.newest_completed()
        {
            return Ok(ScheduledShiftPlan::BlockPlanned {
                sequence: completed.completed.admitted.header().sequence,
                completion_at: completed.completed.completion.at,
            });
        }
        loop {
            let Some(next_at) = self.cached.next_deadline() else {
                return Ok(ScheduledShiftPlan::Idle);
            };
            if next_at > through {
                return Ok(ScheduledShiftPlan::Future { at: next_at });
            }
            if self.len == OUTPUTS {
                return Ok(ScheduledShiftPlan::HorizonFull {
                    next_at,
                    queued: self.len,
                    staged: self.staged,
                });
            }
            let polled = match self.cached.poll(next_at) {
                Ok(polled) => polled,
                Err(error) => {
                    self.output_faulted = true;
                    return Err(ScheduledShiftError::Cached(error));
                }
            };
            match polled {
                CachedMotionPoll::Idle => return Ok(ScheduledShiftPlan::Idle),
                CachedMotionPoll::Future { at } => {
                    if at <= next_at {
                        self.output_faulted = true;
                        return Err(ScheduledShiftError::State);
                    }
                    if at > through {
                        return Ok(ScheduledShiftPlan::Future { at });
                    }
                }
                CachedMotionPoll::Event { event, .. } => {
                    let update = match self.mapper.apply(event) {
                        Ok(update) => update,
                        Err(error) => {
                            self.output_faulted = true;
                            return Err(ScheduledShiftError::Image(error));
                        }
                    };
                    self.next_token = next_output_token(self.next_token);
                    let output = ScheduledShiftOutput {
                        token: OutputCommitToken(self.next_token),
                        update,
                    };
                    if let Err(error) = self.push_output(output) {
                        self.output_faulted = true;
                        return Err(error);
                    }
                }
                CachedMotionPoll::BlockComplete {
                    admitted,
                    completion,
                } => {
                    let sequence = admitted.header().sequence;
                    let queued = match u64::try_from(self.len) {
                        Ok(queued) => queued,
                        Err(_) => {
                            self.output_faulted = true;
                            return Err(ScheduledShiftError::Arithmetic);
                        }
                    };
                    let required_committed_updates =
                        match self.committed_updates.checked_add(queued) {
                            Some(required) => required,
                            None => {
                                self.output_faulted = true;
                                return Err(ScheduledShiftError::Arithmetic);
                            }
                        };
                    if let Err(error) = self.push_completed(ScheduledBlockBarrier {
                        completed: ScheduledBlockCompletion {
                            admitted,
                            completion,
                        },
                        required_committed_updates,
                    }) {
                        self.output_faulted = true;
                        return Err(error);
                    }
                    return Ok(ScheduledShiftPlan::BlockPlanned {
                        sequence,
                        completion_at: completion.at,
                    });
                }
            }
        }
    }

    /// Next generated image not yet accepted by the sole hardware timeline.
    pub fn next_unstaged_output(&self) -> Option<ScheduledShiftOutput> {
        if self.staged >= self.len {
            return None;
        }
        let index = (self.head + self.staged) % OUTPUTS;
        self.outputs[index]
    }

    /// Records that the exact image was accepted into the target's immutable
    /// future hardware timeline. Tokens must be staged in generation order.
    pub fn stage_output(
        &mut self,
        output: ScheduledShiftOutput,
    ) -> Result<ScheduledShiftOutput, OutputStageError> {
        if self.output_faulted || self.staged >= self.len {
            self.output_faulted = true;
            return Err(OutputStageError::State);
        }
        let index = (self.head + self.staged) % OUTPUTS;
        let expected = match self.outputs[index] {
            Some(output) => output,
            None => {
                self.output_faulted = true;
                return Err(OutputStageError::State);
            }
        };
        if output != expected {
            self.output_faulted = true;
            return Err(OutputStageError::Mismatch {
                expected,
                received: output,
            });
        }
        self.staged = match self.staged.checked_add(1) {
            Some(staged) => staged,
            None => {
                self.output_faulted = true;
                return Err(OutputStageError::Arithmetic);
            }
        };
        Ok(expected)
    }

    /// Retires the oldest staged image from a physical latch observation.
    /// Commit order, token, lower bound, and maximum lateness are all exact.
    pub fn commit_output(
        &mut self,
        token: OutputCommitToken,
        committed_at: DeviceCycle,
    ) -> Result<CommittedShiftOutput, OutputCommitError> {
        if self.output_faulted || self.len == 0 || self.staged == 0 {
            self.output_faulted = true;
            return Err(OutputCommitError::State);
        }
        let pending = match self.outputs[self.head] {
            Some(output) => output,
            None => {
                self.output_faulted = true;
                return Err(OutputCommitError::State);
            }
        };
        if token != pending.token {
            self.output_faulted = true;
            return Err(OutputCommitError::Token {
                expected: pending.token,
                received: token,
            });
        }
        if committed_at < pending.update.at {
            self.output_faulted = true;
            return Err(OutputCommitError::Early {
                scheduled: pending.update.at,
                committed: committed_at,
            });
        }
        let lateness = committed_at.0 - pending.update.at.0;
        if lateness > u64::from(self.maximum_commit_lateness_cycles) {
            self.output_faulted = true;
            return Err(OutputCommitError::Deadline {
                scheduled: pending.update.at,
                committed: committed_at,
                maximum_lateness_cycles: self.maximum_commit_lateness_cycles,
            });
        }
        let lateness = match u32::try_from(lateness) {
            Ok(lateness) => lateness,
            Err(_) => {
                self.output_faulted = true;
                return Err(OutputCommitError::Arithmetic);
            }
        };
        let committed_updates = match self.committed_updates.checked_add(1) {
            Some(updates) => updates,
            None => {
                self.output_faulted = true;
                return Err(OutputCommitError::Arithmetic);
            }
        };
        let next_head = if self.len == 1 {
            0
        } else {
            (self.head + 1) % OUTPUTS
        };
        self.outputs[self.head] = None;
        self.head = next_head;
        self.len -= 1;
        self.staged -= 1;
        self.committed_updates = committed_updates;
        self.maximum_commit_lateness_observed = self.maximum_commit_lateness_observed.max(lateness);
        if self.finish_token == Some(token) {
            self.finish_token = None;
            self.job_completion_pending = true;
        }
        Ok(CommittedShiftOutput {
            token,
            update: pending.update,
            committed_at,
            commit_lateness_cycles: lateness,
        })
    }

    /// Returns the oldest logically complete block after its own generated
    /// prefix and exact terminal cycle have both been physically observed.
    /// Later-block images may remain queued or staged without delaying this
    /// independent ownership barrier.
    pub fn take_completed_block(
        &mut self,
        observed: DeviceCycle,
    ) -> Option<ScheduledBlockCompletion<AXES>> {
        if self.output_faulted || self.completed_len == 0 {
            return None;
        }
        let ready = self.completed[self.completed_head]
            .as_ref()
            .is_some_and(|barrier| {
                self.committed_updates >= barrier.required_committed_updates
                    && observed >= barrier.completed.completion.at
            });
        if !ready {
            return None;
        }
        let barrier = self.completed[self.completed_head].take()?;
        self.completed_head = if self.completed_len == 1 {
            0
        } else {
            (self.completed_head + 1) % REALTIME_BLOCK_WINDOW
        };
        self.completed_len -= 1;
        Some(barrier.completed)
    }

    /// Exact earliest cycle at which normal terminal disable may be scheduled.
    pub fn earliest_finish_cycle(&self) -> Result<DeviceCycle, MotionError> {
        if self.output_faulted
            || self.len != 0
            || self.completed_len != 0
            || self.finish_token.is_some()
            || self.job_completion_pending
        {
            return Err(MotionError::State);
        }
        self.distinct_finish_cycle()
    }

    /// Exact normal-disable cycle after the current block's complete logical
    /// trace has been planned but before its output ring is physically drained.
    ///
    /// This permits a continuous DMA target to materialize the final disable
    /// with sufficient hardware lead while the unique block token remains
    /// retained.
    pub fn planned_finish_cycle(&self) -> Result<DeviceCycle, MotionError> {
        if self.output_faulted
            || self.completed_len == 0
            || self.cached.has_admitted_block()
            || self.finish_token.is_some()
            || self.job_completion_pending
        {
            return Err(MotionError::State);
        }
        self.distinct_finish_cycle()
    }

    /// Adds the normal terminal-disable image to the same generated/staged/
    /// committed pipeline used by motion edges.
    pub fn schedule_finish(
        &mut self,
        at: DeviceCycle,
    ) -> Result<ScheduledShiftOutput, ScheduledShiftError> {
        if self.output_faulted
            || self.len != 0
            || self.completed_len != 0
            || self.finish_token.is_some()
            || self.job_completion_pending
        {
            return Err(ScheduledShiftError::State);
        }
        self.append_finish(at)
    }

    /// Adds final disable while a logically complete final block and all of
    /// its physical output ownership remain retained.
    pub fn schedule_planned_finish(
        &mut self,
        at: DeviceCycle,
    ) -> Result<ScheduledShiftOutput, ScheduledShiftError> {
        if self.output_faulted
            || self.completed_len == 0
            || self.cached.has_admitted_block()
            || self.finish_token.is_some()
            || self.job_completion_pending
            || self.len == OUTPUTS
        {
            return Err(ScheduledShiftError::State);
        }
        self.append_finish(at)
    }

    /// Consumes the one-shot job-complete fact after terminal disable was
    /// physically acknowledged.
    pub fn take_job_complete(&mut self) -> bool {
        let complete = self.job_completion_pending;
        self.job_completion_pending = false;
        complete
    }

    /// Latches logical execution, invalidates all future tokens, and returns
    /// the complete safe image for an immediate local target transaction.
    pub fn fault(&mut self, at: DeviceCycle) -> ShiftImageUpdate {
        self.output_faulted = true;
        self.outputs = [None; OUTPUTS];
        self.head = 0;
        self.len = 0;
        self.staged = 0;
        self.finish_token = None;
        self.job_completion_pending = false;
        let _ = self.cached.fault(at);
        let cached_block = self.cached.take_faulted_block();
        if self.faulted_block.is_none() && self.completed_len != 0 {
            self.faulted_block = self.completed[self.completed_head]
                .take()
                .map(|barrier| barrier.completed.into_block());
        }
        self.completed = [const { None }; REALTIME_BLOCK_WINDOW];
        self.completed_head = 0;
        self.completed_len = 0;
        if self.faulted_block.is_none() {
            self.faulted_block = cached_block;
        }
        self.mapper.force_safe(at)
    }

    /// Releases at most one unacknowledgeable block for fault diagnostics after
    /// [`Self::fault`] issued the complete logical safe transaction. Every
    /// admission token was already invalidated; any other retained blocks are
    /// destroyed, never acknowledged.
    pub fn take_faulted_block(&mut self) -> Option<AdmittedBlock<AXES>> {
        if self.output_faulted {
            self.faulted_block.take()
        } else {
            None
        }
    }

    /// Generated outputs retained until physical observation.
    pub const fn queued_outputs(&self) -> usize {
        self.len
    }

    /// Prefix of queued outputs already accepted into the hardware timeline.
    pub const fn staged_outputs(&self) -> usize {
        self.staged
    }

    /// Whether the logical executor is between blocks with at least one exact
    /// completion barrier retained.
    pub const fn block_completion_planned(&self) -> bool {
        self.completed_len != 0 && !self.cached.has_admitted_block()
    }

    /// Number of independently retained per-block physical-commit barriers.
    pub const fn retained_block_completions(&self) -> usize {
        self.completed_len
    }

    /// Number of target-confirmed complete-image updates.
    pub const fn committed_updates(&self) -> u64 {
        self.committed_updates
    }

    /// Largest exact target-reported image commit lateness.
    pub const fn maximum_commit_lateness_observed(&self) -> u32 {
        self.maximum_commit_lateness_observed
    }

    /// Current future logical image; physical visibility is tracked separately.
    pub const fn image(&self) -> u32 {
        self.mapper.image()
    }

    /// Bounded future logical executor status, not a physical output snapshot.
    pub const fn planned_status(&self) -> StepperStatus<AXES> {
        self.cached.status()
    }

    /// Next exact logical generation deadline. Physical latch wakeups remain
    /// the target timeline's separate responsibility.
    pub fn next_deadline(&self) -> Option<DeviceCycle> {
        if self.output_faulted {
            None
        } else {
            self.cached.next_deadline()
        }
    }

    fn push_output(&mut self, output: ScheduledShiftOutput) -> Result<(), ScheduledShiftError> {
        if let Some(previous) = self.last_generated_at
            && output.update.at <= previous
        {
            return Err(ScheduledShiftError::OutputOrder {
                previous,
                received: output.update.at,
            });
        }
        if self.len == OUTPUTS || OUTPUTS == 0 {
            return Err(ScheduledShiftError::State);
        }
        let tail = self
            .head
            .checked_add(self.len)
            .ok_or(ScheduledShiftError::Arithmetic)?
            % OUTPUTS;
        if self.outputs[tail].is_some() {
            return Err(ScheduledShiftError::State);
        }
        self.outputs[tail] = Some(output);
        self.len += 1;
        self.last_generated_at = Some(output.update.at);
        Ok(())
    }

    fn push_completed(
        &mut self,
        barrier: ScheduledBlockBarrier<AXES>,
    ) -> Result<(), ScheduledShiftError> {
        if self.completed_len == REALTIME_BLOCK_WINDOW {
            return Err(ScheduledShiftError::State);
        }
        let tail = self
            .completed_head
            .checked_add(self.completed_len)
            .ok_or(ScheduledShiftError::Arithmetic)?
            % REALTIME_BLOCK_WINDOW;
        if self.completed[tail].is_some() {
            return Err(ScheduledShiftError::State);
        }
        self.completed[tail] = Some(barrier);
        self.completed_len += 1;
        Ok(())
    }

    fn newest_completed(&self) -> Option<&ScheduledBlockBarrier<AXES>> {
        if self.completed_len == 0 {
            return None;
        }
        let index = (self.completed_head + self.completed_len - 1) % REALTIME_BLOCK_WINDOW;
        self.completed[index].as_ref()
    }

    fn distinct_finish_cycle(&self) -> Result<DeviceCycle, MotionError> {
        let ready = self.cached.earliest_finish_cycle()?;
        let Some(previous) = self.last_generated_at else {
            return Ok(ready);
        };
        let next_boundary = previous
            .0
            .checked_add(u64::from(self.output_quantum_cycles))
            .ok_or(MotionError::Arithmetic)?;
        Ok(DeviceCycle(ready.0.max(next_boundary)))
    }

    fn append_finish(
        &mut self,
        at: DeviceCycle,
    ) -> Result<ScheduledShiftOutput, ScheduledShiftError> {
        if let Some(previous) = self.last_generated_at
            && at <= previous
        {
            return Err(ScheduledShiftError::OutputOrder {
                previous,
                received: at,
            });
        }
        let event = self
            .cached
            .finish_job(at)
            .map_err(ScheduledShiftError::Motion)?;
        let update = match self.mapper.apply(event) {
            Ok(update) => update,
            Err(error) => {
                self.output_faulted = true;
                return Err(ScheduledShiftError::Image(error));
            }
        };
        self.next_token = next_output_token(self.next_token);
        let output = ScheduledShiftOutput {
            token: OutputCommitToken(self.next_token),
            update,
        };
        if let Err(error) = self.push_output(output) {
            self.output_faulted = true;
            return Err(error);
        }
        self.finish_token = Some(output.token);
        Ok(output)
    }
}

const fn next_output_token(previous: u32) -> u32 {
    let next = previous.wrapping_add(1);
    if next == 0 { 1 } else { next }
}

fn apply_delta<const AXES: usize>(
    position: &mut [i64; AXES],
    delta: [i64; AXES],
) -> Result<(), CachedMotionError> {
    let mut axis = 0;
    while axis < AXES {
        position[axis] = position[axis]
            .checked_add(delta[axis])
            .ok_or(CachedMotionError::PositionOverflow { axis })?;
        axis += 1;
    }
    Ok(())
}

fn absolute_position<const AXES: usize>(
    origin: [i64; AXES],
    displacement: [i64; AXES],
) -> Result<[i64; AXES], CachedMotionError> {
    let mut position = origin;
    apply_delta(&mut position, displacement)?;
    Ok(position)
}

fn centered_step_offset(duration: u64, steps: u64, index: u64) -> Result<u64, MotionError> {
    if duration == 0 || steps == 0 || index >= steps {
        return Err(MotionError::Arithmetic);
    }
    let odd = u128::from(index)
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .ok_or(MotionError::Arithmetic)?;
    let numerator = odd
        .checked_mul(u128::from(duration))
        .ok_or(MotionError::Arithmetic)?;
    let denominator = u128::from(steps)
        .checked_mul(2)
        .ok_or(MotionError::Arithmetic)?;
    let rounded = numerator
        .checked_add(denominator / 2)
        .ok_or(MotionError::Arithmetic)?
        / denominator;
    u64::try_from(rounded).map_err(|_| MotionError::Arithmetic)
}

fn centered_step_offset_on_grid(
    duration: u64,
    steps: u64,
    index: u64,
    quantum_cycles: u32,
) -> Result<u64, MotionError> {
    let quantum = u64::from(quantum_cycles);
    if quantum == 0 || !duration.is_multiple_of(quantum) {
        return Err(MotionError::OutputGrid {
            cycle: duration,
            quantum_cycles,
        });
    }
    let frame_offset = centered_step_offset(duration / quantum, steps, index)?;
    frame_offset
        .checked_mul(quantum)
        .ok_or(MotionError::Arithmetic)
}

fn require_output_grid(cycle: u64, quantum_cycles: u32) -> Result<(), MotionError> {
    let quantum = u64::from(quantum_cycles);
    if quantum == 0 || !cycle.is_multiple_of(quantum) {
        Err(MotionError::OutputGrid {
            cycle,
            quantum_cycles,
        })
    } else {
        Ok(())
    }
}

fn align_up_to_output_grid(cycle: u64, quantum_cycles: u32) -> Result<u64, MotionError> {
    let quantum = u64::from(quantum_cycles);
    if quantum == 0 {
        return Err(MotionError::Timing);
    }
    let remainder = cycle % quantum;
    if remainder == 0 {
        Ok(cycle)
    } else {
        cycle
            .checked_add(quantum - remainder)
            .ok_or(MotionError::Arithmetic)
    }
}

fn minimum_frequency_period(
    device_cycle_hz: u64,
    maximum_step_frequency_hz: u32,
) -> Result<u64, MotionError> {
    if device_cycle_hz == 0 || maximum_step_frequency_hz == 0 {
        return Err(MotionError::Timing);
    }
    let frequency = u64::from(maximum_step_frequency_hz);
    device_cycle_hz
        .checked_add(frequency - 1)
        .ok_or(MotionError::Arithmetic)
        .map(|numerator| numerator / frequency)
}

fn shift_bit(resource: ResourceId, engine: u8, width: u8) -> Result<u8, ShiftImageError> {
    match resource {
        ResourceId::I2sOut {
            engine: received,
            bit,
        } if received == engine && bit < width => Ok(bit),
        ResourceId::I2sOut {
            engine: received, ..
        } if received != engine => Err(ShiftImageError::WrongEngine {
            expected: engine,
            received,
        }),
        _ => Err(ShiftImageError::Resource),
    }
}

const fn signal_polarity(polarity: SignalPolarity) -> Result<SignalPolarity, ShiftImageError> {
    match polarity {
        SignalPolarity::ActiveHigh | SignalPolarity::ActiveLow => Ok(polarity),
        SignalPolarity::NotApplicable => Err(ShiftImageError::Polarity),
    }
}

const fn bit_is_active(image: u32, bit: u8, polarity: SignalPolarity) -> bool {
    let high = image & (1_u32 << bit) != 0;
    match polarity {
        SignalPolarity::ActiveHigh => high,
        SignalPolarity::ActiveLow => !high,
        SignalPolarity::NotApplicable => false,
    }
}

const fn logical_driver_enabled(image: u32, route: ShiftAxisRoute) -> bool {
    let asserted = bit_is_active(image, route.control_bit, route.control_polarity);
    match route.control_action {
        AxisDriverControl::Enable => asserted,
        AxisDriverControl::Disable => !asserted,
    }
}

fn set_active(image: &mut u32, bit: u8, polarity: SignalPolarity, active: bool) {
    let high = match polarity {
        SignalPolarity::ActiveHigh => active,
        SignalPolarity::ActiveLow => !active,
        SignalPolarity::NotApplicable => return,
    };
    if high {
        *image |= 1_u32 << bit;
    } else {
        *image &= !(1_u32 << bit);
    }
}

const fn minimum_cycle(
    left: Option<DeviceCycle>,
    right: Option<DeviceCycle>,
) -> Option<DeviceCycle> {
    match (left, right) {
        (Some(left), Some(right)) => {
            if left.0 <= right.0 {
                Some(left)
            } else {
                Some(right)
            }
        }
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

const fn validate_axis_count<const AXES: usize>() -> Result<(), MotionError> {
    if AXES == 0 || AXES > MAX_STEPPER_AXES || AXES > u16::BITS as usize {
        Err(MotionError::AxisCount)
    } else {
        Ok(())
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn read_i64(bytes: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alumina_board::OwnerDomain;
    use alumina_config::{BindingFlags, BindingRole, ResourceBinding};
    use alumina_job::{
        AdmittedBlock, JobDescriptor, RealtimeJob, RealtimeJobState, RealtimePoll, WorkSource,
    };
    use alumina_machine_ir::{BlockValidationLimits, ExecutionBlock, StreamId, ValidationLimits};
    use alumina_storage::{ContentId, ObjectKind, PublishedObject, StoredObject};
    use std::vec::Vec;

    use super::*;

    fn timing<const AXES: usize>(maximum_lateness_cycles: u32) -> StepperTiming<AXES> {
        StepperTiming {
            axes: [AxisTiming {
                pulse_high_cycles: 1,
                pulse_low_cycles: 1,
                direction_setup_cycles: 2,
                direction_hold_cycles: 2,
                enable_setup_cycles: 2,
                enable_hold_cycles: 2,
                maximum_step_frequency_hz: 500_000,
            }; AXES],
            device_cycle_hz: 1_000_000,
            output_quantum_cycles: 1,
            maximum_lateness_cycles,
        }
    }

    fn segment<const AXES: usize>(
        start: u64,
        end: u64,
        delta_steps: [i64; AXES],
    ) -> ExecutionSegment<AXES> {
        ExecutionSegment {
            start_tick: StreamTick(start),
            end_tick: StreamTick(end),
            delta_steps,
            flags: 0,
        }
    }

    #[test]
    fn allocation_free_preflight_replays_the_production_electrical_contract() {
        let segments = [
            segment(0, 100, [3, 0]),
            segment(100, 200, [0, 0]),
            segment(200, 300, [-2, 4]),
        ];
        let summary = preflight_stepper_segments(timing(0), [10, -4], &segments).unwrap();
        assert_eq!(summary.end_tick, StreamTick(300));
        assert_eq!(summary.position, [11, 0]);
        assert_eq!(summary.emitted_steps, [5, 4]);
        assert_eq!(summary.segment_count, 3);
        assert_eq!(summary.earliest_finish_cycle, DeviceCycle(300));

        assert_eq!(
            preflight_stepper_segments(timing(0), [0], &[segment(0, 4, [3])]),
            Err(MotionError::Rate { axis: 0 })
        );
    }

    fn finite_segment<const AXES: usize>(
        start: u64,
        updates: u32,
        initial_position: [i64; AXES],
        first_difference: [i64; AXES],
    ) -> FiniteDifferenceSegment<AXES> {
        let mut axes = [FiniteDifferenceAxis::default(); AXES];
        for axis in 0..AXES {
            axes[axis] = FiniteDifferenceAxis {
                initial_position: initial_position[axis],
                first_difference: first_difference[axis],
                second_difference: 0,
                third_difference: 0,
            };
        }
        FiniteDifferenceSegment {
            start_tick: StreamTick(start),
            end_tick: StreamTick(start + u64::from(updates)),
            update_period_ticks: 1,
            update_count: updates,
            axes,
            flags: 0,
        }
    }

    fn finite_limits(maximum_update_count: u32) -> FiniteDifferenceExecutionLimits {
        FiniteDifferenceExecutionLimits {
            maximum_segment_ticks: u64::from(maximum_update_count),
            maximum_update_count,
            maximum_steps_per_segment: 1_000_000,
        }
    }

    #[test]
    fn finite_difference_preflight_replays_edges_without_dense_iteration() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        let first_difference = (step - 1) / 4;
        let first = finite_segment(0, 16, [0, 0], [first_difference, 0]);
        let second = finite_segment(
            16,
            16,
            [first.position_at(0, 16).unwrap(), 0],
            [first_difference, 0],
        );
        let summary = preflight_finite_difference_segments(
            timing(0),
            finite_limits(1_000),
            [10, -4],
            &[first, second],
        )
        .unwrap();
        assert_eq!(summary.end_tick, StreamTick(32));
        assert_eq!(summary.position, [18, -4]);
        assert_eq!(summary.emitted_steps, [8, 0]);
        assert_eq!(summary.segment_count, 2);
        assert_eq!(summary.update_count, 32);
        assert_eq!(summary.earliest_finish_cycle, DeviceCycle(34));
        assert_eq!(
            summary.maximum_position_error,
            FINITE_DIFFERENCE_ONE_STEP.unsigned_abs() / 2
        );

        let billion_updates = finite_segment(0, 1_000_000_000, [0], [1]);
        let summary = preflight_finite_difference_segments(
            timing(0),
            finite_limits(1_000_000_000),
            [0],
            &[billion_updates],
        )
        .unwrap();
        assert_eq!(summary.update_count, 1_000_000_000);
        assert_eq!(summary.emitted_steps, [0]);
        assert_eq!(summary.position, [0]);
    }

    #[test]
    fn finite_difference_preflight_classifies_timing_and_structure_separately() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        let too_fast = finite_segment(0, 8, [0], [step / 2]);
        let error =
            preflight_finite_difference_segments(timing(0), finite_limits(100), [0], &[too_fast])
                .unwrap_err();
        assert!(matches!(
            error,
            FiniteDifferencePreflightError::Segment {
                index: 0,
                error: FiniteDifferenceError::UpdateRate { axis: 0, .. }
            }
        ));
        assert!(error.is_time_dilation_candidate());

        let mut reversing = finite_segment(0, 8, [0], [step / 4]);
        reversing.axes[0].second_difference = -(step / 8);
        let error =
            preflight_finite_difference_segments(timing(0), finite_limits(100), [0], &[reversing])
                .unwrap_err();
        assert!(matches!(
            error,
            FiniteDifferencePreflightError::Segment {
                error: FiniteDifferenceError::DirectionReversal { axis: 0 },
                ..
            }
        ));
        assert!(!error.is_time_dilation_candidate());
    }

    #[test]
    fn finite_difference_preflight_checks_boundaries_and_exact_phase_continuity() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        let first_difference = (step - 1) / 4;
        let pulse_at_end = finite_segment(0, 15, [0], [first_difference]);
        assert_eq!(
            preflight_finite_difference_segments(
                timing(0),
                finite_limits(100),
                [0],
                &[pulse_at_end],
            ),
            Err(FiniteDifferencePreflightError::Timing(
                MotionError::PulseBoundary { axis: 0 }
            ))
        );

        let first = finite_segment(0, 16, [0], [first_difference]);
        let wrong_second = finite_segment(16, 16, [step * 4], [first_difference]);
        assert_eq!(
            preflight_finite_difference_segments(
                timing(0),
                finite_limits(100),
                [0],
                &[first, wrong_second],
            ),
            Err(FiniteDifferencePreflightError::Segment {
                index: 1,
                error: FiniteDifferenceError::PositionContinuity { axis: 0 },
            })
        );
    }

    #[test]
    fn dense_finite_difference_generator_matches_closed_form_and_preflight() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        let segment = FiniteDifferenceSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(8),
            update_period_ticks: 1,
            update_count: 8,
            axes: [
                FiniteDifferenceAxis {
                    initial_position: 0,
                    first_difference: step / 16,
                    second_difference: 0,
                    third_difference: step / 128,
                },
                FiniteDifferenceAxis {
                    initial_position: 0,
                    first_difference: -(step / 16),
                    second_difference: 0,
                    third_difference: -(step / 128),
                },
            ],
            flags: 0,
        };
        let preflight = preflight_finite_difference_segments(
            timing(0),
            finite_limits(100),
            [20, -20],
            &[segment],
        )
        .unwrap();
        assert_eq!(preflight.position, [21, -21]);
        assert_eq!(preflight.emitted_steps, [1, 1]);

        let mut generator = FiniteDifferenceFrameGenerator::new(
            timing(0),
            finite_limits(100),
            StreamTick(0),
            [0, 0],
            segment,
        )
        .unwrap();
        let mut step_frames = 0;
        while let Some(frame) = generator.advance().unwrap() {
            assert_eq!(
                frame.finite_position,
                [
                    segment.position_at(0, frame.update_index).unwrap(),
                    segment.position_at(1, frame.update_index).unwrap(),
                ]
            );
            assert_eq!(frame.at, StreamTick(u64::from(frame.update_index)));
            if !frame.step.is_empty() {
                step_frames += 1;
                assert_eq!(frame.update_index, 6);
                assert_eq!(frame.step.bits(), 0b11);
                assert_eq!(frame.direction_positive.bits(), 0b01);
                assert_eq!(frame.relative_steps, [1, -1]);
            }
        }
        assert_eq!(step_frames, 1);
        assert_eq!(generator.remaining_updates(), 0);
        let terminal = generator.finish().unwrap();
        assert_eq!(terminal.delta_steps, [1, -1]);
        assert_eq!(
            terminal.terminal_position,
            preflight.terminal_finite_position
        );
    }

    #[test]
    fn dense_finite_difference_generator_requires_complete_validated_state() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        let segment = finite_segment(0, 8, [0], [(step - 1) / 4]);
        let mut generator = FiniteDifferenceFrameGenerator::new(
            timing(0),
            finite_limits(100),
            StreamTick(0),
            [0],
            segment,
        )
        .unwrap();
        generator.advance().unwrap();
        assert_eq!(
            generator.finish(),
            Err(FiniteDifferencePreflightError::Timing(
                MotionError::OutputInvariant
            ))
        );

        let too_fast = finite_segment(0, 8, [0], [step / 2]);
        assert!(matches!(
            FiniteDifferenceFrameGenerator::new(
                timing(0),
                finite_limits(100),
                StreamTick(0),
                [0],
                too_fast,
            ),
            Err(FiniteDifferencePreflightError::Segment {
                error: FiniteDifferenceError::UpdateRate { axis: 0, .. },
                ..
            })
        ));
    }

    #[test]
    fn direct_finite_difference_executor_consumes_every_update_and_exact_edge() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        let segment = FiniteDifferenceSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(8),
            update_period_ticks: 1,
            update_count: 8,
            axes: [
                FiniteDifferenceAxis {
                    initial_position: 0,
                    first_difference: step / 16,
                    second_difference: 0,
                    third_difference: step / 128,
                },
                FiniteDifferenceAxis {
                    initial_position: 0,
                    first_difference: -(step / 16),
                    second_difference: 0,
                    third_difference: -(step / 128),
                },
            ],
            flags: 0,
        };
        let mut executor =
            FiniteDifferenceStepperExecutor::new(timing(0), finite_limits(100)).unwrap();
        executor.start_job(DeviceCycle(100), [20, -20]).unwrap();
        executor.load_segment(segment).unwrap();

        let mut updates = 0_u32;
        let mut rise_at = None;
        let mut fall_at = None;
        let completion = loop {
            let deadline = executor.next_deadline().unwrap();
            match executor.poll(deadline).unwrap() {
                FiniteDifferenceMotionPoll::Event { event, .. } => {
                    assert_eq!(event.at, DeviceCycle(100));
                    assert_eq!(event.direction_change.bits(), 0b11);
                    assert_eq!(event.direction_positive.bits(), 0b01);
                    assert_eq!(event.enable.bits(), 0b11);
                }
                FiniteDifferenceMotionPoll::Update { frame, event, .. } => {
                    updates += 1;
                    assert_eq!(frame.update_index, updates);
                    if frame.step.bits() == 0b11 {
                        let event = event.unwrap();
                        assert_eq!(event.step_high.bits(), 0b11);
                        assert_eq!(event.at, DeviceCycle(106));
                        rise_at = Some(event.at);
                    } else if let Some(event) = event {
                        assert_eq!(event.step_low.bits(), 0b11);
                        assert_eq!(event.at, DeviceCycle(107));
                        fall_at = Some(event.at);
                    }
                }
                FiniteDifferenceMotionPoll::SegmentComplete(completion) => break completion,
                FiniteDifferenceMotionPoll::Idle | FiniteDifferenceMotionPoll::Future { .. } => {
                    panic!("exact-deadline direct executor cannot be idle or future")
                }
            }
        };
        assert_eq!(updates, 8);
        assert_eq!(rise_at, Some(DeviceCycle(106)));
        assert_eq!(fall_at, Some(DeviceCycle(107)));
        assert_eq!(completion.at, DeviceCycle(108));
        assert_eq!(completion.end_tick, StreamTick(8));
        assert_eq!(completion.delta_steps, [1, -1]);
        assert_eq!(completion.position, [21, -21]);
        assert_eq!(completion.update_count, 8);
        assert_eq!(executor.status().emitted_steps, [1, 1]);
        assert_eq!(
            executor.finite_position(),
            [
                segment.position_at(0, 8).unwrap(),
                segment.position_at(1, 8).unwrap(),
            ]
        );
        assert_eq!(executor.earliest_finish_cycle(), Ok(DeviceCycle(109)));
        let disable = executor.finish_job(DeviceCycle(109)).unwrap();
        assert_eq!(disable.disable.bits(), 0b11);
        assert_eq!(executor.status().state, ExecutorState::Complete);
    }

    #[test]
    fn direct_finite_difference_executor_deadline_faults_on_empty_update() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        let segment = finite_segment(0, 8, [0], [(step - 1) / 4]);
        let mut executor =
            FiniteDifferenceStepperExecutor::new(timing(0), finite_limits(100)).unwrap();
        executor.start_job(DeviceCycle(0), [0]).unwrap();
        executor.load_segment(segment).unwrap();
        assert!(matches!(
            executor.poll(DeviceCycle(0)),
            Ok(FiniteDifferenceMotionPoll::Event { .. })
        ));
        assert_eq!(executor.next_deadline(), Some(DeviceCycle(1)));
        assert_eq!(
            executor.poll(DeviceCycle(2)),
            Err(FiniteDifferencePreflightError::Timing(
                MotionError::Deadline {
                    scheduled: DeviceCycle(1),
                    observed: DeviceCycle(2),
                    maximum_lateness_cycles: 0,
                }
            ))
        );
        assert_eq!(executor.status().state, ExecutorState::Faulted);
        assert_eq!(executor.status().deadline_misses, 1);
        let safe = executor.fault(DeviceCycle(2));
        assert_eq!(safe.disable.bits(), 0b1);
        assert!(safe.step_low.is_empty());
    }

    #[test]
    fn only_electrical_duration_pressure_is_a_time_dilation_candidate() {
        for error in [
            MotionError::Rate { axis: 0 },
            MotionError::PulseBoundary { axis: 0 },
            MotionError::PulseLow { axis: 0 },
            MotionError::DirectionSetup { axis: 0 },
            MotionError::DirectionHold { axis: 0 },
            MotionError::EnableSetup { axis: 0 },
            MotionError::EnableHold { axis: 0 },
        ] {
            assert!(error.is_time_dilation_candidate());
        }
        for error in [
            MotionError::Timing,
            MotionError::OutputGrid {
                cycle: 1,
                quantum_cycles: 2,
            },
            MotionError::Arithmetic,
            MotionError::SegmentOrder,
            MotionError::Deadline {
                scheduled: DeviceCycle(1),
                observed: DeviceCycle(2),
                maximum_lateness_cycles: 0,
            },
            MotionError::OutputInvariant,
        ] {
            assert!(!error.is_time_dilation_candidate());
        }
    }

    fn shifted_binding(role: BindingRole, bit: u8, polarity: SignalPolarity) -> ResourceBinding {
        ResourceBinding {
            instance: 0,
            role,
            resource: ResourceId::I2sOut { engine: 0, bit },
            owner: OwnerDomain::Realtime,
            polarity,
            flags: BindingFlags::default(),
            minimum_active_cycles: 2,
            minimum_inactive_cycles: 2,
            maximum_frequency_hz: if role == BindingRole::AxisStep {
                100_000
            } else {
                0
            },
            watchdog_cycles: 100_000,
        }
    }

    fn shifted_profile() -> StepperExecutionProfile<3> {
        StepperExecutionProfile {
            axes: core::array::from_fn(|axis| {
                let base = u8::try_from(axis * 3).unwrap();
                StepperAxisProfile {
                    instance: u16::try_from(axis).unwrap(),
                    step: ResourceBinding {
                        instance: u16::try_from(axis).unwrap(),
                        ..shifted_binding(
                            BindingRole::AxisStep,
                            base + 1,
                            SignalPolarity::ActiveHigh,
                        )
                    },
                    direction: ResourceBinding {
                        instance: u16::try_from(axis).unwrap(),
                        ..shifted_binding(
                            BindingRole::AxisDirection,
                            base + 2,
                            SignalPolarity::ActiveHigh,
                        )
                    },
                    driver_control: ResourceBinding {
                        instance: u16::try_from(axis).unwrap(),
                        ..shifted_binding(
                            BindingRole::AxisDisable,
                            base,
                            SignalPolarity::ActiveHigh,
                        )
                    },
                    driver_control_action: AxisDriverControl::Disable,
                }
            }),
            timing: timing(0),
        }
    }

    const fn shifted_contract() -> ShiftImageContract {
        ShiftImageContract {
            engine: 0,
            width: 24,
            defined_mask: 0x00ff_ffff,
            safe_image: 0x0000_1249,
        }
    }

    struct OneBlock(Option<ExecutionBlock>);

    impl WorkSource for OneBlock {
        fn try_receive(&mut self) -> Option<ExecutionBlock> {
            self.0.take()
        }

        fn depth(&self) -> usize {
            usize::from(self.0.is_some())
        }
    }

    struct TwoBlocks {
        first: Option<ExecutionBlock>,
        second: Option<ExecutionBlock>,
    }

    impl WorkSource for TwoBlocks {
        fn try_receive(&mut self) -> Option<ExecutionBlock> {
            self.first.take().or_else(|| self.second.take())
        }

        fn depth(&self) -> usize {
            usize::from(self.first.is_some()) + usize::from(self.second.is_some())
        }
    }

    fn admitted_finite_difference_block(
        segments: &[FiniteDifferenceSegment<2>],
    ) -> (RealtimeJob<2>, AdmittedBlock<2>) {
        let stream_id = StreamId::new([0x61; 16]).unwrap();
        let capability_digest = alumina_protocol::Digest([0x62; 32]);
        let config_digest = alumina_protocol::Digest([0x63; 32]);
        let block = ExecutionBlock::encode_finite_difference(
            stream_id,
            capability_digest,
            config_digest,
            0,
            alumina_protocol::Digest::ZERO,
            segments,
        )
        .unwrap();
        let descriptor = JobDescriptor {
            prepare_id: 17,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId::from_sha256(alumina_protocol::Digest([0x64; 32])),
                    byte_len: 512,
                },
                manifest: ContentId::from_sha256(alumina_protocol::Digest([0x65; 32])),
            },
            stream_id,
            capability_digest,
            config_digest,
            axis_count: 2,
            execution_kind: alumina_machine_ir::ExecutionKind::FiniteDifference,
            maximum_finite_difference_updates: 100,
            block_count: 1,
            first_tick: StreamTick(0),
            initial_position: [20, -20, 0, 0, 0, 0, 0, 0],
            limits: BlockValidationLimits {
                maximum_block_ticks: 1_000,
                segment: ValidationLimits {
                    maximum_segment_ticks: 1_000,
                    maximum_steps_per_segment: 100,
                },
            },
        };
        let mut job = RealtimeJob::prepare(descriptor).unwrap();
        let admitted = match job.poll(&mut OneBlock(Some(block))).unwrap() {
            RealtimePoll::Block(admitted) => admitted,
            RealtimePoll::Empty | RealtimePoll::Outstanding => {
                panic!("one direct block must be admitted")
            }
        };
        (job, admitted)
    }

    fn admitted_block(segments: &[ExecutionSegment<3>]) -> (RealtimeJob<3>, AdmittedBlock<3>) {
        let stream_id = StreamId::new([0x11; 16]).unwrap();
        let capability_digest = alumina_protocol::Digest([0x22; 32]);
        let config_digest = alumina_protocol::Digest([0x33; 32]);
        let block = ExecutionBlock::encode_motion(
            stream_id,
            capability_digest,
            config_digest,
            0,
            alumina_protocol::Digest::ZERO,
            segments,
        )
        .unwrap();
        let descriptor = JobDescriptor {
            prepare_id: 7,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId::from_sha256(alumina_protocol::Digest([0x44; 32])),
                    byte_len: 512,
                },
                manifest: ContentId::from_sha256(alumina_protocol::Digest([0x55; 32])),
            },
            stream_id,
            capability_digest,
            config_digest,
            axis_count: 3,
            execution_kind: alumina_machine_ir::ExecutionKind::Motion,
            maximum_finite_difference_updates: 0,
            block_count: 1,
            first_tick: StreamTick(0),
            initial_position: [0; alumina_machine_ir::MAX_EXECUTION_AXES],
            limits: BlockValidationLimits {
                maximum_block_ticks: 1_000,
                segment: ValidationLimits {
                    maximum_segment_ticks: 1_000,
                    maximum_steps_per_segment: 100,
                },
            },
        };
        let mut job = RealtimeJob::prepare(descriptor).unwrap();
        let admitted = match job.poll(&mut OneBlock(Some(block))).unwrap() {
            RealtimePoll::Block(admitted) => admitted,
            RealtimePoll::Empty | RealtimePoll::Outstanding => {
                panic!("one validated block must be admitted")
            }
        };
        (job, admitted)
    }

    fn admitted_block_pair(
        first_segments: &[ExecutionSegment<3>],
        second_segments: &[ExecutionSegment<3>],
    ) -> (RealtimeJob<3>, AdmittedBlock<3>, AdmittedBlock<3>) {
        let stream_id = StreamId::new([0x11; 16]).unwrap();
        let capability_digest = alumina_protocol::Digest([0x22; 32]);
        let config_digest = alumina_protocol::Digest([0x33; 32]);
        let first = ExecutionBlock::encode_motion(
            stream_id,
            capability_digest,
            config_digest,
            0,
            alumina_protocol::Digest::ZERO,
            first_segments,
        )
        .unwrap();
        let second = ExecutionBlock::encode_motion(
            stream_id,
            capability_digest,
            config_digest,
            1,
            first.header().block_digest,
            second_segments,
        )
        .unwrap();
        let descriptor = JobDescriptor {
            prepare_id: 7,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId::from_sha256(alumina_protocol::Digest([0x44; 32])),
                    byte_len: 1_024,
                },
                manifest: ContentId::from_sha256(alumina_protocol::Digest([0x55; 32])),
            },
            stream_id,
            capability_digest,
            config_digest,
            axis_count: 3,
            execution_kind: alumina_machine_ir::ExecutionKind::Motion,
            maximum_finite_difference_updates: 0,
            block_count: 2,
            first_tick: StreamTick(0),
            initial_position: [0; alumina_machine_ir::MAX_EXECUTION_AXES],
            limits: BlockValidationLimits {
                maximum_block_ticks: 1_000,
                segment: ValidationLimits {
                    maximum_segment_ticks: 1_000,
                    maximum_steps_per_segment: 100,
                },
            },
        };
        let mut source = TwoBlocks {
            first: Some(first),
            second: Some(second),
        };
        let mut job = RealtimeJob::prepare(descriptor).unwrap();
        let first = match job.poll(&mut source).unwrap() {
            RealtimePoll::Block(admitted) => admitted,
            RealtimePoll::Empty | RealtimePoll::Outstanding => {
                panic!("first validated block must be admitted")
            }
        };
        let second = match job.poll(&mut source).unwrap() {
            RealtimePoll::Block(admitted) => admitted,
            RealtimePoll::Empty | RealtimePoll::Outstanding => {
                panic!("second validated block must be admitted")
            }
        };
        (job, first, second)
    }

    fn drain_segment<const AXES: usize>(
        executor: &mut StepperExecutor<AXES>,
    ) -> (Vec<StepperEvent>, SegmentCompletion<AXES>) {
        let mut events = Vec::new();
        loop {
            let deadline = executor.next_deadline().unwrap();
            match executor.poll(deadline).unwrap() {
                MotionPoll::Event { event, .. } => events.push(event),
                MotionPoll::SegmentComplete(completion) => return (events, completion),
                MotionPoll::Idle | MotionPoll::Future { .. } => panic!("unexpected poll state"),
            }
        }
    }

    #[test]
    fn coordinated_trace_has_exact_counts_positions_and_centered_edges() {
        let mut executor = StepperExecutor::new(timing::<3>(0)).unwrap();
        executor.start_job(DeviceCycle(1_000), [10, -2, 7]).unwrap();
        executor.load_segment(segment(0, 20, [4, 2, -1])).unwrap();
        let (events, completion) = drain_segment(&mut executor);

        assert_eq!(events[0].at, DeviceCycle(1_000));
        assert_eq!(events[0].direction_change.bits(), 0b111);
        assert_eq!(events[0].direction_positive.bits(), 0b011);
        assert_eq!(events[0].enable.bits(), 0b111);

        let rises: Vec<(u64, u16)> = events
            .iter()
            .filter(|event| !event.step_high.is_empty())
            .map(|event| (event.at.0, event.step_high.bits()))
            .collect();
        assert_eq!(
            rises,
            [
                (1_003, 0b001),
                (1_005, 0b010),
                (1_008, 0b001),
                (1_010, 0b100),
                (1_013, 0b001),
                (1_015, 0b010),
                (1_018, 0b001),
            ]
        );
        assert_eq!(completion.at, DeviceCycle(1_020));
        assert_eq!(completion.position, [14, 0, 6]);
        assert_eq!(completion.maximum_half_tick_error, 1);
        assert_eq!(executor.status().emitted_steps, [4, 2, 1]);
        assert!(executor.status().step_high.is_empty());
    }

    #[test]
    fn cached_runner_returns_token_only_after_exact_block_progress() {
        let (mut job, admitted) =
            admitted_block(&[segment(0, 10, [1, 0, 0]), segment(10, 20, [1, -1, 0])]);
        let mut runner = CachedStepperExecutor::new(timing::<3>(0)).unwrap();
        runner.start_job(DeviceCycle(100), [5, 7, -2]).unwrap();
        runner.admit_block(admitted).unwrap();
        assert!(runner.has_admitted_block());
        assert_eq!(job.status().state, RealtimeJobState::Admitted);

        let mut rises = [0_u64; 3];
        let admitted = loop {
            let deadline = runner.next_deadline().unwrap();
            match runner.poll(deadline).unwrap() {
                CachedMotionPoll::Event { event, .. } => {
                    for (axis, count) in rises.iter_mut().enumerate() {
                        *count += u64::from(event.step_high.contains(axis));
                    }
                }
                CachedMotionPoll::BlockComplete {
                    admitted,
                    completion,
                } => {
                    assert_eq!(completion.end_tick, StreamTick(20));
                    assert_eq!(completion.position, [7, 6, -2]);
                    break admitted;
                }
                CachedMotionPoll::Idle | CachedMotionPoll::Future { .. } => {
                    panic!("exact-deadline runner cannot be idle or future")
                }
            }
        };
        assert!(!runner.has_admitted_block());
        assert_eq!(job.status().state, RealtimeJobState::Admitted);
        assert_eq!(rises, [2, 1, 0]);
        let status = job.acknowledge(admitted).unwrap();
        assert_eq!(status.state, RealtimeJobState::Complete);
        assert_eq!(status.completed_progress.unwrap().position, [2, -1, 0]);
        assert_eq!(runner.status().position, [7, 6, -2]);
        assert_eq!(
            runner.finish_job(DeviceCycle(120)).unwrap().disable.bits(),
            0b011
        );
    }

    #[test]
    fn cached_direct_runner_preflights_dense_updates_and_returns_exact_token() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        let first = FiniteDifferenceSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(8),
            update_period_ticks: 1,
            update_count: 8,
            axes: [
                FiniteDifferenceAxis {
                    initial_position: 0,
                    first_difference: step / 16,
                    second_difference: 0,
                    third_difference: step / 128,
                },
                FiniteDifferenceAxis {
                    initial_position: 0,
                    first_difference: -(step / 16),
                    second_difference: 0,
                    third_difference: -(step / 128),
                },
            ],
            flags: 0,
        };
        let terminal = [
            first.position_at(0, 8).unwrap(),
            first.position_at(1, 8).unwrap(),
        ];
        let second = finite_segment(8, 8, terminal, [(step - 1) / 4, -((step - 1) / 4)]);
        let (mut job, admitted) = admitted_finite_difference_block(&[first, second]);
        let mut runner =
            CachedFiniteDifferenceExecutor::new(timing(0), finite_limits(100)).unwrap();
        runner.start_job(DeviceCycle(100), [20, -20]).unwrap();
        runner.admit_block(admitted).unwrap();
        assert!(runner.has_admitted_block());

        let mut updates = 0_u32;
        let mut rising_edges = [0_u64; 2];
        let admitted = loop {
            let deadline = runner.next_deadline().unwrap();
            match runner.poll(deadline).unwrap() {
                CachedFiniteDifferencePoll::Update { event, .. } => {
                    updates += 1;
                    if let Some(event) = event {
                        for (axis, count) in rising_edges.iter_mut().enumerate() {
                            *count += u64::from(event.step_high.contains(axis));
                        }
                    }
                }
                CachedFiniteDifferencePoll::Event { event, .. } => {
                    for (axis, count) in rising_edges.iter_mut().enumerate() {
                        *count += u64::from(event.step_high.contains(axis));
                    }
                }
                CachedFiniteDifferencePoll::BlockComplete {
                    admitted,
                    completion,
                } => {
                    assert_eq!(completion.end_tick, StreamTick(16));
                    assert_eq!(completion.position, [23, -23]);
                    break admitted;
                }
                CachedFiniteDifferencePoll::Future { .. } => {}
                CachedFiniteDifferencePoll::Idle => {
                    panic!("a cached direct block cannot become idle before completion")
                }
            }
        };
        assert_eq!(updates, 16);
        assert_eq!(rising_edges, [3, 3]);
        assert_eq!(runner.status().emitted_steps, [3, 3]);
        assert!(!runner.has_admitted_block());
        let status = job.acknowledge(admitted).unwrap();
        assert_eq!(status.state, RealtimeJobState::Complete);
        assert_eq!(status.completed_progress.unwrap().position, [3, -3]);
        assert_eq!(runner.earliest_finish_cycle(), Ok(DeviceCycle(118)));
        assert_eq!(
            runner.finish_job(DeviceCycle(118)).unwrap().disable.bits(),
            0b11
        );
    }

    #[test]
    fn cached_direct_runner_rejects_later_electrical_failure_without_live_mutation() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        let first = finite_segment(0, 8, [0, 0], [(step - 1) / 4, 0]);
        let terminal = [first.position_at(0, 8).unwrap(), 0];
        let second = finite_segment(8, 8, terminal, [step / 2, 0]);
        let (_job, admitted) = admitted_finite_difference_block(&[first, second]);
        let mut runner =
            CachedFiniteDifferenceExecutor::new(timing(0), finite_limits(100)).unwrap();
        runner.start_job(DeviceCycle(100), [20, -20]).unwrap();
        let before = runner.status();
        let rejected = runner.admit_block(admitted).unwrap_err();
        assert!(matches!(
            rejected.error(),
            CachedFiniteDifferenceError::FiniteDifference(
                FiniteDifferencePreflightError::Segment {
                    error: FiniteDifferenceError::UpdateRate { axis: 0, .. },
                    ..
                }
            )
        ));
        assert_eq!(runner.status(), before);
        assert!(!runner.has_admitted_block());
        let _admitted = rejected.into_block();
    }

    #[test]
    fn cached_runner_preflights_later_segments_without_live_state_change() {
        let (mut job, admitted) =
            admitted_block(&[segment(0, 10, [1, 0, 0]), segment(10, 20, [6, 0, 0])]);
        let mut runner = CachedStepperExecutor::new(timing::<3>(0)).unwrap();
        runner.start_job(DeviceCycle(100), [0; 3]).unwrap();
        let before = runner.status();
        let rejected = runner.admit_block(admitted).unwrap_err();
        assert_eq!(
            rejected.error(),
            CachedMotionError::Motion(MotionError::Rate { axis: 0 })
        );
        assert_eq!(runner.status(), before);
        assert!(!runner.has_admitted_block());
        assert_eq!(rejected.into_block().header().sequence, 0);
        assert_eq!(job.status().state, RealtimeJobState::Admitted);
        job.cancel();
        assert_eq!(job.status().state, RealtimeJobState::Cancelled);
    }

    #[test]
    fn block_preflight_cost_does_not_scale_with_emitted_step_count() {
        let mut live = StepperExecutor::new(timing::<3>(0)).unwrap();
        live.start_job(DeviceCycle(100), [7, -9, 2]).unwrap();
        let mut preflight = live.preflight_snapshot();
        preflight
            .load_segment(segment(0, 4_000_000_004, [1_000_000_000, -500_000_000, 0]))
            .unwrap();
        preflight.preflight_complete_segment().unwrap();

        assert_eq!(live.status().position, [7, -9, 2]);
        assert_eq!(live.status().next_tick, StreamTick(0));
        assert_eq!(
            preflight.status().position,
            [1_000_000_007, -500_000_009, 2]
        );
        assert_eq!(
            preflight.status().emitted_steps,
            [1_000_000_000, 500_000_000, 0]
        );
        assert_eq!(preflight.status().next_tick, StreamTick(4_000_000_004));
        assert_eq!(preflight.status().state, ExecutorState::Ready);
    }

    #[test]
    fn cached_runner_retains_faulted_block_until_safe_transaction_is_requested() {
        let (mut job, admitted) = admitted_block(&[segment(0, 20, [2, 0, 0])]);
        let mut runner = CachedStepperExecutor::new(timing::<3>(2)).unwrap();
        runner.start_job(DeviceCycle(100), [0; 3]).unwrap();
        runner.admit_block(admitted).unwrap();
        assert!(matches!(
            runner.poll(DeviceCycle(100)).unwrap(),
            CachedMotionPoll::Event { .. }
        ));
        let rise = runner.next_deadline().unwrap();
        match runner.poll(DeviceCycle(rise.0 + 3)) {
            Err(CachedMotionError::Motion(MotionError::Deadline {
                scheduled,
                observed,
                maximum_lateness_cycles,
            })) => {
                assert_eq!(scheduled, rise);
                assert_eq!(observed, DeviceCycle(rise.0 + 3));
                assert_eq!(maximum_lateness_cycles, 2);
            }
            _ => panic!("late cached edge must fault without returning an event"),
        }
        assert!(runner.has_admitted_block());
        assert!(runner.take_faulted_block().is_none());
        let safe = runner.fault(DeviceCycle(rise.0 + 3));
        assert_eq!(safe.disable.bits(), 1);
        let faulted = runner.take_faulted_block().unwrap();
        assert_eq!(faulted.header().sequence, 0);
        job.cancel();
        assert_eq!(job.status().state, RealtimeJobState::Cancelled);
    }

    #[test]
    fn shifted_runner_releases_block_only_after_every_physical_commit() {
        let (mut job, admitted) = admitted_block(&[segment(0, 20, [2, 0, 0])]);
        let mut profile = shifted_profile();
        profile.timing = timing(2);
        let mut runner = ShiftedCachedStepper::new(profile, shifted_contract()).unwrap();
        runner.start_job(DeviceCycle(100), [0; 3]).unwrap();
        runner.admit_block(admitted).unwrap();

        let mut output_count = 0_u64;
        let admitted = loop {
            let deadline = runner.next_deadline().unwrap();
            match runner.poll(deadline).unwrap() {
                ShiftedMotionPoll::Output(pending) => {
                    assert!(matches!(
                        runner.poll(deadline).unwrap(),
                        ShiftedMotionPoll::AwaitingCommit(repeated) if repeated == pending
                    ));
                    let committed_at = DeviceCycle(deadline.0 + output_count % 2);
                    let committed = runner.commit_output(pending.token, committed_at).unwrap();
                    assert_eq!(committed.update, pending.update);
                    assert_eq!(
                        committed.commit_lateness_cycles,
                        u32::try_from(output_count % 2).unwrap()
                    );
                    output_count += 1;
                }
                ShiftedMotionPoll::BlockComplete {
                    admitted,
                    completion,
                } => {
                    assert_eq!(completion.at, DeviceCycle(120));
                    assert_eq!(completion.position, [2, 0, 0]);
                    break admitted;
                }
                ShiftedMotionPoll::Idle
                | ShiftedMotionPoll::Future { .. }
                | ShiftedMotionPoll::AwaitingCommit(_) => {
                    panic!("exact-deadline shifted runner made no progress")
                }
            }
        };
        assert_eq!(output_count, 5);
        assert_eq!(runner.committed_updates(), 5);
        assert_eq!(runner.maximum_commit_lateness_observed(), 1);
        assert_eq!(runner.earliest_finish_cycle(), Ok(DeviceCycle(120)));
        assert_eq!(job.status().state, RealtimeJobState::Admitted);
        assert_eq!(
            job.acknowledge(admitted).unwrap().state,
            RealtimeJobState::Complete
        );

        let terminal = runner.finish_job(DeviceCycle(120)).unwrap();
        assert_eq!(terminal.update.at, DeviceCycle(120));
        runner
            .commit_output(terminal.token, DeviceCycle(121))
            .unwrap();
        assert_eq!(runner.committed_updates(), 6);
        assert_eq!(runner.status().state, ExecutorState::Complete);
        assert!(runner.status().enabled.is_empty());
    }

    #[test]
    fn shifted_normal_finish_waits_for_exact_enable_hold_before_commit() {
        let (_job, admitted) = admitted_block(&[segment(0, 20, [2, 0, 0])]);
        let mut profile = shifted_profile();
        profile.timing = timing(0);
        profile.timing.axes[0].enable_hold_cycles = 20;
        let mut runner = ShiftedCachedStepper::new(profile, shifted_contract()).unwrap();
        runner.start_job(DeviceCycle(100), [0; 3]).unwrap();
        runner.admit_block(admitted).unwrap();

        loop {
            let deadline = runner.next_deadline().unwrap();
            match runner.poll(deadline).unwrap() {
                ShiftedMotionPoll::Output(pending) => {
                    runner.commit_output(pending.token, deadline).unwrap();
                }
                ShiftedMotionPoll::BlockComplete { completion, .. } => {
                    assert_eq!(completion.at, DeviceCycle(120));
                    break;
                }
                ShiftedMotionPoll::Idle
                | ShiftedMotionPoll::Future { .. }
                | ShiftedMotionPoll::AwaitingCommit(_) => {
                    panic!("exact-deadline shifted runner made no progress")
                }
            }
        }

        assert_eq!(runner.earliest_finish_cycle(), Ok(DeviceCycle(136)));
        assert!(matches!(
            runner.finish_job(DeviceCycle(135)),
            Err(ShiftedMotionError::Motion(MotionError::EnableHold {
                axis: 0
            }))
        ));
        let terminal = runner.finish_job(DeviceCycle(136)).unwrap();
        assert_eq!(terminal.update.at, DeviceCycle(136));
        assert!(matches!(
            runner.poll(DeviceCycle(136)).unwrap(),
            ShiftedMotionPoll::AwaitingCommit(repeated) if repeated == terminal
        ));
        runner
            .commit_output(terminal.token, DeviceCycle(136))
            .unwrap();
        assert_eq!(runner.status().state, ExecutorState::Complete);
        assert!(runner.status().enabled.is_empty());
    }

    #[test]
    fn invalid_output_commit_latches_until_complete_safe_image_is_requested() {
        let (_job, admitted) = admitted_block(&[segment(0, 20, [2, 0, 0])]);
        let mut profile = shifted_profile();
        profile.timing = timing(2);
        let mut runner = ShiftedCachedStepper::new(profile, shifted_contract()).unwrap();
        runner.start_job(DeviceCycle(100), [0; 3]).unwrap();
        runner.admit_block(admitted).unwrap();
        let pending = match runner.poll(DeviceCycle(100)).unwrap() {
            ShiftedMotionPoll::Output(pending) => pending,
            _ => panic!("boundary image must be generated"),
        };
        let wrong = OutputCommitToken(pending.token.0 + 1);
        assert_eq!(
            runner.commit_output(wrong, DeviceCycle(100)),
            Err(OutputCommitError::Token {
                expected: pending.token,
                received: wrong,
            })
        );
        assert_eq!(
            runner.commit_output(pending.token, DeviceCycle(100)),
            Err(OutputCommitError::State)
        );
        assert!(matches!(
            runner.poll(DeviceCycle(100)),
            Err(ShiftedMotionError::State)
        ));
        assert!(runner.take_faulted_block().is_none());
        assert_eq!(
            runner.fault(DeviceCycle(101)),
            ShiftImageUpdate {
                at: DeviceCycle(101),
                image: shifted_contract().safe_image,
            }
        );
        assert_eq!(runner.take_faulted_block().unwrap().header().sequence, 0);
    }

    #[test]
    fn early_and_late_physical_commits_never_clear_the_pending_image() {
        for (committed_at, expected) in [
            (
                DeviceCycle(99),
                OutputCommitError::Early {
                    scheduled: DeviceCycle(100),
                    committed: DeviceCycle(99),
                },
            ),
            (
                DeviceCycle(103),
                OutputCommitError::Deadline {
                    scheduled: DeviceCycle(100),
                    committed: DeviceCycle(103),
                    maximum_lateness_cycles: 2,
                },
            ),
        ] {
            let (_job, admitted) = admitted_block(&[segment(0, 20, [2, 0, 0])]);
            let mut profile = shifted_profile();
            profile.timing = timing(2);
            let mut runner = ShiftedCachedStepper::new(profile, shifted_contract()).unwrap();
            runner.start_job(DeviceCycle(100), [0; 3]).unwrap();
            runner.admit_block(admitted).unwrap();
            let pending = match runner.poll(DeviceCycle(100)).unwrap() {
                ShiftedMotionPoll::Output(pending) => pending,
                _ => panic!("boundary image must be generated"),
            };
            assert_eq!(
                runner.commit_output(pending.token, committed_at),
                Err(expected)
            );
            assert_eq!(runner.committed_updates(), 0);
            assert!(runner.take_faulted_block().is_none());
            assert_eq!(
                runner.fault(DeviceCycle(104)).image,
                shifted_contract().safe_image
            );
            assert!(runner.take_faulted_block().is_some());
        }
    }

    #[test]
    fn scheduled_runner_plans_future_images_but_releases_only_after_latches() {
        let (mut job, admitted) = admitted_block(&[segment(0, 40, [2, 0, 0])]);
        let mut profile = shifted_profile();
        profile.timing = timing(2);
        profile.timing.output_quantum_cycles = 4;
        for axis in &mut profile.timing.axes {
            axis.pulse_high_cycles = 4;
            axis.pulse_low_cycles = 4;
        }
        let mut runner = ScheduledShiftedStepper::<3, 8>::new(profile, shifted_contract()).unwrap();
        runner.start_job(DeviceCycle(100), [0; 3]).unwrap();
        runner.admit_block(admitted).unwrap();

        assert_eq!(
            runner.plan_through(DeviceCycle(140)).unwrap(),
            ScheduledShiftPlan::BlockPlanned {
                sequence: 0,
                completion_at: DeviceCycle(140),
            }
        );
        assert_eq!(runner.queued_outputs(), 5);
        assert_eq!(runner.staged_outputs(), 0);
        assert_eq!(runner.planned_status().position, [2, 0, 0]);
        assert!(runner.take_completed_block(DeviceCycle(140)).is_none());

        let mut staged = Vec::new();
        while let Some(output) = runner.next_unstaged_output() {
            assert_eq!(runner.stage_output(output), Ok(output));
            staged.push(output);
        }
        assert_eq!(staged.len(), 5);
        assert_eq!(runner.staged_outputs(), 5);
        for (index, output) in staged.into_iter().enumerate() {
            let committed_at = DeviceCycle(output.update.at.0 + u64::from(index == 4));
            assert_eq!(
                runner
                    .commit_output(output.token, committed_at)
                    .unwrap()
                    .update,
                output.update
            );
            if index != 4 {
                assert!(runner.take_completed_block(committed_at).is_none());
            }
        }
        assert_eq!(runner.committed_updates(), 5);
        assert_eq!(runner.maximum_commit_lateness_observed(), 1);
        assert!(runner.take_completed_block(DeviceCycle(139)).is_none());
        let completed = runner.take_completed_block(DeviceCycle(140)).unwrap();
        assert_eq!(completed.completion().at, DeviceCycle(140));
        assert_eq!(completed.completion().position, [2, 0, 0]);
        assert_eq!(completed.completion().maximum_half_tick_error, 4);
        assert_eq!(job.status().state, RealtimeJobState::Admitted);
        assert_eq!(
            job.acknowledge(completed.into_block()).unwrap().state,
            RealtimeJobState::Complete
        );

        assert_eq!(runner.earliest_finish_cycle(), Ok(DeviceCycle(140)));
        let terminal = runner.schedule_finish(DeviceCycle(140)).unwrap();
        assert_eq!(runner.next_unstaged_output(), Some(terminal));
        runner.stage_output(terminal).unwrap();
        runner
            .commit_output(terminal.token, DeviceCycle(140))
            .unwrap();
        assert!(runner.take_job_complete());
        assert!(!runner.take_job_complete());
        assert_eq!(runner.planned_status().state, ExecutorState::Complete);
        assert!(runner.planned_status().enabled.is_empty());
    }

    #[test]
    fn scheduled_runner_prefills_successor_before_releasing_prior_block() {
        let (mut job, first, second) =
            admitted_block_pair(&[segment(0, 40, [2, 0, 0])], &[segment(40, 80, [2, 0, 0])]);
        let mut profile = shifted_profile();
        profile.timing = timing(2);
        profile.timing.output_quantum_cycles = 4;
        for axis in &mut profile.timing.axes {
            axis.pulse_high_cycles = 4;
            axis.pulse_low_cycles = 4;
        }
        let mut runner =
            ScheduledShiftedStepper::<3, 16>::new(profile, shifted_contract()).unwrap();
        runner.start_job(DeviceCycle(100), [0; 3]).unwrap();
        runner.admit_block(first).unwrap();
        assert_eq!(
            runner.plan_through(DeviceCycle(180)).unwrap(),
            ScheduledShiftPlan::BlockPlanned {
                sequence: 0,
                completion_at: DeviceCycle(140),
            }
        );
        let first_prefix = runner.queued_outputs();
        assert_eq!(first_prefix, 5);
        assert_eq!(runner.retained_block_completions(), 1);

        runner.admit_block(second).unwrap();
        assert_eq!(
            runner.plan_through(DeviceCycle(180)).unwrap(),
            ScheduledShiftPlan::BlockPlanned {
                sequence: 1,
                completion_at: DeviceCycle(180),
            }
        );
        assert_eq!(runner.retained_block_completions(), 2);
        assert!(runner.queued_outputs() > first_prefix);
        assert!(runner.take_completed_block(DeviceCycle(180)).is_none());

        let mut staged = Vec::new();
        while let Some(output) = runner.next_unstaged_output() {
            runner.stage_output(output).unwrap();
            staged.push(output);
        }
        assert_eq!(staged.len(), 9);
        assert!(
            staged
                .windows(2)
                .all(|pair| pair[0].update.at < pair[1].update.at)
        );
        for output in staged.iter().take(first_prefix).copied() {
            runner
                .commit_output(output.token, output.update.at)
                .unwrap();
        }
        assert_eq!(runner.queued_outputs(), staged.len() - first_prefix);
        let completed = runner.take_completed_block(DeviceCycle(140)).unwrap();
        assert_eq!(completed.completion().at, DeviceCycle(140));
        let first = completed.into_block();
        assert_eq!(first.header().sequence, 0);
        assert_eq!(
            job.acknowledge(first).unwrap().state,
            RealtimeJobState::Admitted
        );

        for output in staged.iter().skip(first_prefix).copied() {
            runner
                .commit_output(output.token, output.update.at)
                .unwrap();
        }
        let completed = runner.take_completed_block(DeviceCycle(180)).unwrap();
        assert_eq!(completed.completion().position, [4, 0, 0]);
        assert_eq!(
            job.acknowledge(completed.into_block()).unwrap().state,
            RealtimeJobState::Complete
        );
    }

    #[test]
    fn scheduled_final_disable_can_be_owned_before_block_release() {
        let (mut job, admitted) = admitted_block(&[segment(0, 8, [1, 0, 0])]);
        let mut profile = shifted_profile();
        profile.timing = timing(0);
        profile.timing.output_quantum_cycles = 4;
        for axis in &mut profile.timing.axes {
            axis.pulse_high_cycles = 4;
            axis.pulse_low_cycles = 4;
        }
        let mut runner = ScheduledShiftedStepper::<3, 8>::new(profile, shifted_contract()).unwrap();
        runner.start_job(DeviceCycle(100), [0; 3]).unwrap();
        runner.admit_block(admitted).unwrap();
        assert_eq!(
            runner.plan_through(DeviceCycle(108)).unwrap(),
            ScheduledShiftPlan::BlockPlanned {
                sequence: 0,
                completion_at: DeviceCycle(108),
            }
        );
        assert_eq!(runner.planned_finish_cycle(), Ok(DeviceCycle(112)));
        assert_eq!(
            runner.schedule_planned_finish(DeviceCycle(108)),
            Err(ScheduledShiftError::OutputOrder {
                previous: DeviceCycle(108),
                received: DeviceCycle(108),
            })
        );
        let terminal = runner.schedule_planned_finish(DeviceCycle(112)).unwrap();
        assert_eq!(terminal.update.at, DeviceCycle(112));
        assert_eq!(runner.queued_outputs(), 4);
        assert_eq!(runner.planned_status().state, ExecutorState::Complete);
        assert!(runner.take_completed_block(DeviceCycle(108)).is_none());

        let mut staged = Vec::new();
        while let Some(output) = runner.next_unstaged_output() {
            runner.stage_output(output).unwrap();
            staged.push(output);
        }
        for output in staged.iter().copied() {
            runner
                .commit_output(output.token, output.update.at)
                .unwrap();
        }
        assert_eq!(staged.len(), 4);
        assert!(runner.take_job_complete());
        let completed = runner.take_completed_block(DeviceCycle(108)).unwrap();
        assert_eq!(completed.completion().position, [1, 0, 0]);
        assert_eq!(
            job.acknowledge(completed.into_block()).unwrap().state,
            RealtimeJobState::Complete
        );
    }

    #[test]
    fn output_quantum_is_an_exact_executor_lattice() {
        let mut grid_timing = timing::<1>(0);
        grid_timing.output_quantum_cycles = 4;
        grid_timing.axes[0].pulse_high_cycles = 4;
        grid_timing.axes[0].pulse_low_cycles = 4;
        let mut executor = StepperExecutor::new(grid_timing).unwrap();

        assert_eq!(
            executor.start_job(DeviceCycle(101), [0]),
            Err(MotionError::OutputGrid {
                cycle: 101,
                quantum_cycles: 4,
            })
        );
        executor.start_job(DeviceCycle(100), [0]).unwrap();
        assert_eq!(
            executor.load_segment(segment(0, 42, [2])),
            Err(MotionError::OutputGrid {
                cycle: 142,
                quantum_cycles: 4,
            })
        );
        executor.load_segment(segment(0, 40, [2])).unwrap();
        let (events, completion) = drain_segment(&mut executor);
        assert!(events.iter().all(|event| event.at.0.is_multiple_of(4)));
        assert_eq!(
            events
                .iter()
                .filter(|event| !event.step_high.is_empty())
                .map(|event| event.at)
                .collect::<Vec<_>>(),
            [DeviceCycle(112), DeviceCycle(132)]
        );
        assert_eq!(completion.at, DeviceCycle(140));
        assert_eq!(completion.maximum_half_tick_error, 4);
        assert_eq!(executor.earliest_finish_cycle(), Ok(DeviceCycle(140)));
        assert!(matches!(
            executor.finish_job(DeviceCycle(139)),
            Err(MotionError::OutputGrid {
                cycle: 139,
                quantum_cycles: 4,
            })
        ));
        executor.finish_job(DeviceCycle(140)).unwrap();

        grid_timing.axes[0].pulse_high_cycles = 2;
        assert!(matches!(
            StepperExecutor::new(grid_timing),
            Err(MotionError::Timing)
        ));
    }

    #[test]
    fn scheduled_ring_stops_exactly_at_capacity_and_resumes_after_commit() {
        let (_job, admitted) = admitted_block(&[segment(0, 20, [2, 0, 0])]);
        let mut profile = shifted_profile();
        profile.timing = timing(0);
        let mut runner = ScheduledShiftedStepper::<3, 2>::new(profile, shifted_contract()).unwrap();
        runner.start_job(DeviceCycle(100), [0; 3]).unwrap();
        runner.admit_block(admitted).unwrap();

        assert_eq!(
            runner.plan_through(DeviceCycle(120)).unwrap(),
            ScheduledShiftPlan::HorizonFull {
                next_at: DeviceCycle(106),
                queued: 2,
                staged: 0,
            }
        );
        let first = runner.next_unstaged_output().unwrap();
        runner.stage_output(first).unwrap();
        let second = runner.next_unstaged_output().unwrap();
        runner.stage_output(second).unwrap();
        runner.commit_output(first.token, first.update.at).unwrap();

        assert_eq!(
            runner.plan_through(DeviceCycle(120)).unwrap(),
            ScheduledShiftPlan::HorizonFull {
                next_at: DeviceCycle(115),
                queued: 2,
                staged: 1,
            }
        );
        let third = runner.next_unstaged_output().unwrap();
        assert_eq!(third.update.at, DeviceCycle(106));
        runner.stage_output(third).unwrap();
        runner
            .commit_output(second.token, second.update.at)
            .unwrap();
        runner.commit_output(third.token, third.update.at).unwrap();

        assert_eq!(
            runner.plan_through(DeviceCycle(114)).unwrap(),
            ScheduledShiftPlan::Future {
                at: DeviceCycle(115),
            }
        );
        assert_eq!(runner.queued_outputs(), 0);
    }

    #[test]
    fn scheduled_staging_mismatch_latches_and_preserves_block_for_fault() {
        let (_job, admitted) = admitted_block(&[segment(0, 20, [2, 0, 0])]);
        let mut runner =
            ScheduledShiftedStepper::<3, 4>::new(shifted_profile(), shifted_contract()).unwrap();
        runner.start_job(DeviceCycle(100), [0; 3]).unwrap();
        runner.admit_block(admitted).unwrap();
        assert_eq!(
            runner.plan_through(DeviceCycle(100)).unwrap(),
            ScheduledShiftPlan::Future {
                at: DeviceCycle(105),
            }
        );
        let expected = runner.next_unstaged_output().unwrap();
        let received = ScheduledShiftOutput {
            update: ShiftImageUpdate {
                image: expected.update.image ^ 1,
                ..expected.update
            },
            ..expected
        };
        assert_eq!(
            runner.stage_output(received),
            Err(OutputStageError::Mismatch { expected, received })
        );
        assert_eq!(
            runner.plan_through(DeviceCycle(120)),
            Err(ScheduledShiftError::State)
        );
        assert!(runner.take_faulted_block().is_none());
        assert_eq!(
            runner.fault(DeviceCycle(101)),
            ShiftImageUpdate {
                at: DeviceCycle(101),
                image: shifted_contract().safe_image,
            }
        );
        assert_eq!(runner.queued_outputs(), 0);
        assert_eq!(runner.take_faulted_block().unwrap().header().sequence, 0);
    }

    #[test]
    fn scheduled_zero_capacity_and_commit_before_stage_fail_closed() {
        assert!(matches!(
            ScheduledShiftedStepper::<3, 0>::new(shifted_profile(), shifted_contract()),
            Err(ShiftedExecutorBuildError::HorizonCapacity)
        ));

        let (_job, admitted) = admitted_block(&[segment(0, 20, [2, 0, 0])]);
        let mut runner =
            ScheduledShiftedStepper::<3, 1>::new(shifted_profile(), shifted_contract()).unwrap();
        runner.start_job(DeviceCycle(100), [0; 3]).unwrap();
        runner.admit_block(admitted).unwrap();
        runner.plan_through(DeviceCycle(100)).unwrap();
        let generated = runner.next_unstaged_output().unwrap();
        assert_eq!(
            runner.commit_output(generated.token, generated.update.at),
            Err(OutputCommitError::State)
        );
        assert_eq!(runner.staged_outputs(), 0);
        runner.fault(DeviceCycle(101));
        assert!(runner.take_faulted_block().is_some());
    }

    #[test]
    fn complete_shift_image_maps_enable_direction_and_steps_without_losing_other_bits() {
        let profile = shifted_profile();
        let contract = shifted_contract();
        let mut mapper = ShiftImageMapper::new(&profile, contract).unwrap();

        let boundary = mapper
            .apply(StepperEvent {
                at: DeviceCycle(1_000),
                direction_change: AxisMask::from_bits(0b111, 3).unwrap(),
                direction_positive: AxisMask::from_bits(0b011, 3).unwrap(),
                enable: AxisMask::from_bits(0b111, 3).unwrap(),
                disable: AxisMask::NONE,
                step_high: AxisMask::NONE,
                step_low: AxisMask::NONE,
            })
            .unwrap();
        assert_eq!(boundary.image, 0x0000_1224);

        let raised = mapper
            .apply(StepperEvent {
                at: DeviceCycle(1_010),
                direction_change: AxisMask::NONE,
                direction_positive: AxisMask::from_bits(0b011, 3).unwrap(),
                enable: AxisMask::NONE,
                disable: AxisMask::NONE,
                step_high: AxisMask::from_bits(0b101, 3).unwrap(),
                step_low: AxisMask::NONE,
            })
            .unwrap();
        assert_eq!(raised.image, 0x0000_12a6);
        assert_eq!(raised.image & ((1 << 9) | (1 << 12)), (1 << 9) | (1 << 12));

        let lowered = mapper
            .apply(StepperEvent {
                at: DeviceCycle(1_012),
                direction_change: AxisMask::NONE,
                direction_positive: AxisMask::from_bits(0b011, 3).unwrap(),
                enable: AxisMask::NONE,
                disable: AxisMask::NONE,
                step_high: AxisMask::NONE,
                step_low: AxisMask::from_bits(0b101, 3).unwrap(),
            })
            .unwrap();
        assert_eq!(lowered.image, 0x0000_1224);

        let disabled = mapper
            .apply(StepperEvent {
                at: DeviceCycle(1_020),
                direction_change: AxisMask::NONE,
                direction_positive: AxisMask::from_bits(0b011, 3).unwrap(),
                enable: AxisMask::NONE,
                disable: AxisMask::from_bits(0b111, 3).unwrap(),
                step_high: AxisMask::NONE,
                step_low: AxisMask::NONE,
            })
            .unwrap();
        assert_eq!(disabled.image, 0x0000_126d);
        assert_eq!(mapper.force_safe(DeviceCycle(1_021)).image, 0x0000_1249);
    }

    #[test]
    fn shift_image_rejects_unsafe_duplicate_and_conflicting_contracts() {
        let profile = shifted_profile();
        let contract = ShiftImageContract {
            engine: 0,
            width: 24,
            defined_mask: 0x00ff_ffff,
            safe_image: 0x0000_1248,
        };
        assert!(matches!(
            ShiftImageMapper::new(&profile, contract),
            Err(ShiftImageError::UnsafeImage { axis: 0 })
        ));

        let mut duplicate = profile;
        duplicate.axes[1].step.resource = duplicate.axes[0].step.resource;
        assert!(matches!(
            ShiftImageMapper::new(
                &duplicate,
                ShiftImageContract {
                    safe_image: 0x0000_1249,
                    ..contract
                }
            ),
            Err(ShiftImageError::DuplicateBit { bit: 1 })
        ));

        let mut mapper = ShiftImageMapper::new(
            &profile,
            ShiftImageContract {
                safe_image: 0x0000_1249,
                ..contract
            },
        )
        .unwrap();
        let before = mapper.image();
        assert_eq!(
            mapper.apply(StepperEvent {
                at: DeviceCycle(9),
                direction_change: AxisMask::NONE,
                direction_positive: AxisMask::NONE,
                enable: AxisMask::NONE,
                disable: AxisMask::NONE,
                step_high: AxisMask(0b1000),
                step_low: AxisMask::NONE,
            }),
            Err(ShiftImageError::EventMask)
        );
        assert_eq!(mapper.image(), before);

        assert_eq!(
            mapper.apply(StepperEvent {
                at: DeviceCycle(10),
                direction_change: AxisMask::NONE,
                direction_positive: AxisMask::NONE,
                enable: AxisMask::NONE,
                disable: AxisMask::NONE,
                step_high: AxisMask::from_bits(1, 3).unwrap(),
                step_low: AxisMask::NONE,
            }),
            Err(ShiftImageError::EventState { axis: 0 })
        );
        assert_eq!(mapper.image(), before);

        assert_eq!(
            mapper.apply(StepperEvent {
                at: DeviceCycle(11),
                direction_change: AxisMask::NONE,
                direction_positive: AxisMask::NONE,
                enable: AxisMask::NONE,
                disable: AxisMask::NONE,
                step_high: AxisMask::NONE,
                step_low: AxisMask::from_bits(1, 3).unwrap(),
            }),
            Err(ShiftImageError::EventState { axis: 0 })
        );
        assert_eq!(mapper.image(), before);
    }

    #[test]
    fn every_centered_edge_is_within_an_exact_half_tick() {
        for duration in 3_u64..200 {
            for steps in 1_u64..=duration / 2 {
                for index in 0..steps {
                    let rounded = centered_step_offset(duration, steps, index).unwrap();
                    let exact_numerator = u128::from(2 * index + 1) * u128::from(duration);
                    let denominator = u128::from(2 * steps);
                    let rounded_numerator = u128::from(rounded) * denominator;
                    let error = rounded_numerator.abs_diff(exact_numerator);
                    assert!(error * 2 <= denominator);
                }
            }
        }
    }

    #[test]
    fn overspeed_is_rejected_before_executor_state_changes() {
        let mut executor = StepperExecutor::new(timing::<1>(0)).unwrap();
        executor.start_job(DeviceCycle(100), [4]).unwrap();
        let before = executor.status();
        assert_eq!(
            executor.load_segment(segment(0, 10, [6])),
            Err(MotionError::Rate { axis: 0 })
        );
        assert_eq!(executor.status(), before);
        assert_eq!(executor.next_deadline(), None);

        executor.load_segment(segment(0, 20, [4])).unwrap();
        let (_, completion) = drain_segment(&mut executor);
        assert_eq!(completion.position, [8]);
    }

    #[test]
    fn reversal_requires_hold_and_setup_across_a_contiguous_dwell() {
        let mut executor = StepperExecutor::new(timing::<1>(0)).unwrap();
        executor.start_job(DeviceCycle(0), [0]).unwrap();
        executor.load_segment(segment(0, 4, [1])).unwrap();
        let (_, first) = drain_segment(&mut executor);
        assert_eq!(first.position, [1]);

        assert_eq!(
            executor.load_segment(segment(4, 8, [-1])),
            Err(MotionError::DirectionHold { axis: 0 })
        );
        executor.load_segment(segment(4, 8, [0])).unwrap();
        let (events, _) = drain_segment(&mut executor);
        assert!(events.is_empty());

        executor.load_segment(segment(8, 12, [-1])).unwrap();
        let (events, completion) = drain_segment(&mut executor);
        assert_eq!(events[0].direction_change.bits(), 1);
        assert_eq!(events[0].direction_positive.bits(), 0);
        assert_eq!(completion.position, [0]);
    }

    #[test]
    fn late_edge_faults_without_emitting_it_and_force_safe_is_immediate() {
        let mut executor = StepperExecutor::new(timing::<2>(2)).unwrap();
        executor.start_job(DeviceCycle(50), [0, 0]).unwrap();
        executor.load_segment(segment(0, 20, [2, 0])).unwrap();

        let boundary = executor.next_deadline().unwrap();
        let first = executor.poll(boundary).unwrap();
        assert!(matches!(first, MotionPoll::Event { .. }));
        let rise = executor.next_deadline().unwrap();
        assert_eq!(
            executor.poll(DeviceCycle(rise.0 + 3)),
            Err(MotionError::Deadline {
                scheduled: rise,
                observed: DeviceCycle(rise.0 + 3),
                maximum_lateness_cycles: 2,
            })
        );
        assert_eq!(executor.status().state, ExecutorState::Faulted);
        assert_eq!(executor.status().emitted_steps, [0, 0]);
        assert_eq!(executor.status().deadline_misses, 1);
        assert_eq!(executor.next_deadline(), None);

        let report = RealtimeMotionReport::from_executor(&executor).unwrap();
        assert_eq!(report.state, ExecutorState::Faulted);
        assert_eq!(report.next_deadline, None);
        assert_eq!(report.enabled.bits(), 1);
        assert_eq!(report.deadline_misses, 1);
        assert_eq!(
            RealtimeMotionReport::decode(&report.encode().unwrap()),
            Ok(report)
        );

        let safe = executor.fault(DeviceCycle(rise.0 + 3));
        assert_eq!(safe.disable.bits(), 1);
        assert!(safe.step_high.is_empty());
        assert!(executor.status().enabled.is_empty());
    }

    #[test]
    fn realtime_report_round_trips_a_live_exact_segment() {
        let mut executor = StepperExecutor::new(timing::<3>(2)).unwrap();
        executor
            .start_job(DeviceCycle(10_000), [17, -9, 2])
            .unwrap();
        executor.load_segment(segment(0, 20, [2, -1, 0])).unwrap();
        assert!(matches!(
            executor.poll(DeviceCycle(10_000)).unwrap(),
            MotionPoll::Event { .. }
        ));

        let report = RealtimeMotionReport::from_executor(&executor).unwrap();
        assert_eq!(report.state, ExecutorState::Segment);
        assert_eq!(report.axis_count, 3);
        assert_eq!(report.epoch, DeviceCycle(10_000));
        assert_eq!(report.next_tick, StreamTick(0));
        assert_eq!(report.next_deadline, Some(DeviceCycle(10_005)));
        assert_eq!(report.position, [17, -9, 2, 0, 0, 0, 0, 0]);
        assert_eq!(report.enabled.bits(), 0b011);
        assert_eq!(report.direction_known.bits(), 0b011);
        assert_eq!(report.direction_positive.bits(), 0b001);

        let encoded = report.encode().unwrap();
        assert_eq!(encoded.len(), REALTIME_MOTION_REPORT_BYTES);
        assert_eq!(RealtimeMotionReport::decode(&encoded), Ok(report));
    }

    #[test]
    fn realtime_report_rejects_noncanonical_reserved_position_and_masks() {
        let mut executor = StepperExecutor::new(timing::<2>(0)).unwrap();
        executor.start_job(DeviceCycle(100), [4, -3]).unwrap();
        executor.load_segment(segment(0, 20, [1, 1])).unwrap();
        let encoded = RealtimeMotionReport::from_executor(&executor)
            .unwrap()
            .encode()
            .unwrap();

        let mut reserved = encoded;
        reserved[14] = 1;
        assert_eq!(
            RealtimeMotionReport::decode(&reserved),
            Err(MotionReportError::Reserved)
        );

        let mut unused_position = encoded;
        unused_position[80] = 1;
        assert_eq!(
            RealtimeMotionReport::decode(&unused_position),
            Err(MotionReportError::Position)
        );

        let mut out_of_width_mask = encoded;
        out_of_width_mask[56] |= 0b100;
        assert_eq!(
            RealtimeMotionReport::decode(&out_of_width_mask),
            Err(MotionReportError::Mask)
        );

        let mut impossible_state = encoded;
        impossible_state[10] = ExecutorState::Ready as u8;
        assert_eq!(
            RealtimeMotionReport::decode(&impossible_state),
            Err(MotionReportError::State)
        );
    }

    #[test]
    fn normal_finish_enforces_enable_hold_and_disables_exactly_once() {
        let mut policy = timing::<1>(0);
        policy.axes[0].enable_hold_cycles = 5;
        let mut executor = StepperExecutor::new(policy).unwrap();
        executor.start_job(DeviceCycle(100), [0]).unwrap();
        executor.load_segment(segment(0, 10, [1])).unwrap();
        let (_, completion) = drain_segment(&mut executor);
        assert_eq!(completion.at, DeviceCycle(110));
        assert_eq!(
            executor.finish_job(DeviceCycle(110)),
            Err(MotionError::EnableHold { axis: 0 })
        );
        let disable = executor.finish_job(DeviceCycle(111)).unwrap();
        assert_eq!(disable.disable.bits(), 1);
        assert_eq!(executor.status().state, ExecutorState::Complete);
        assert_eq!(
            executor.finish_job(DeviceCycle(113)),
            Err(MotionError::State)
        );
    }

    #[test]
    fn invalid_width_timing_order_and_epoch_fail_closed() {
        assert!(matches!(
            StepperExecutor::new(StepperTiming::<0> {
                axes: [],
                device_cycle_hz: 1_000_000,
                output_quantum_cycles: 1,
                maximum_lateness_cycles: 0,
            }),
            Err(MotionError::AxisCount)
        ));
        let mut invalid_timing = timing::<1>(0);
        invalid_timing.axes[0].pulse_high_cycles = 0;
        assert!(matches!(
            StepperExecutor::new(invalid_timing),
            Err(MotionError::Timing)
        ));

        let mut executor = StepperExecutor::new(timing::<1>(0)).unwrap();
        executor.start_job(DeviceCycle(u64::MAX - 5), [0]).unwrap();
        assert_eq!(
            executor.load_segment(segment(1, 10, [0])),
            Err(MotionError::SegmentOrder)
        );
        assert_eq!(
            executor.load_segment(segment(0, 10, [0])),
            Err(MotionError::EpochOverflow)
        );
        assert_eq!(executor.status().state, ExecutorState::Ready);
    }
}
