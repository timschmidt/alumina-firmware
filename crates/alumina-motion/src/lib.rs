#![no_std]
#![doc = "Allocation-free exact integer event execution for Alumina motion streams."]

use alumina_board::ResourceId;
use alumina_config::{
    AxisDriverControl, ConfigurationIdentity, RealtimeConfigurationProfile, SignalPolarity,
    StepperAxisProfile,
};
use alumina_machine_ir::{ExecutionSegment, MAX_EXECUTION_AXES, StreamTick};
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
    /// Maximum time after a scheduled edge at which the backend may still
    /// apply it. Exceeding this value faults before returning that edge.
    pub maximum_lateness_cycles: u32,
}

impl<const AXES: usize> StepperTiming<AXES> {
    /// Validates the fixed axis width and every electrical timing bound.
    pub fn validate(self) -> Result<(), MotionError> {
        validate_axis_count::<AXES>()?;
        if self.device_cycle_hz == 0 {
            return Err(MotionError::Timing);
        }
        for timing in self.axes {
            timing.validate()?;
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
        maximum_lateness_cycles: u32,
    ) -> Result<Self, MotionError> {
        validate_axis_count::<AXES>()?;
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
    pub maximum_half_tick_error: u8,
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
            let first_offset = centered_step_offset(duration, count, 0)?;
            let first = DeviceCycle(
                start
                    .0
                    .checked_add(first_offset)
                    .ok_or(MotionError::EpochOverflow)?,
            );
            let last_offset = centered_step_offset(duration, count, count - 1)?;
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
                    let offset = centered_step_offset(duration, active.steps[axis], next_index)?;
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
            maximum_half_tick_error: u8::from(active.steps.iter().any(|steps| *steps != 0)),
        }))
    }
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
    fn complete_shift_image_maps_enable_direction_and_steps_without_losing_other_bits() {
        let profile = shifted_profile();
        let contract = ShiftImageContract {
            engine: 0,
            width: 24,
            defined_mask: 0x00ff_ffff,
            safe_image: 0x0000_1249,
        };
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
