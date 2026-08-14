//! Exact-lattice outer-loop contracts for cascaded position/velocity servo control.
//!
//! This module is portable control software. It does not unwrap a physical
//! encoder, identify a motor, sample ADC hardware, commit PWM, or establish a
//! safe energizing path. A later configuration checkpoint must give the
//! position lattice physical units and lower a complete validated profile.

use alumina_protocol::{DeviceCycle, Digest};

use crate::{
    DqPoint, FocError, FocParameterSnapshot, FocTimingProfile, PiConfig, PiController, PiUpdate,
    Q30, Q30Interval, div_ceil, div_floor, round_ties_even,
};

/// Fractional bits in the signed Q31.32 servo-position lattice.
pub const SERVO_POSITION_FRACTION_BITS: u32 = 32;
/// Exact denominator of every [`ServoPosition`] value.
pub const SERVO_POSITION_SCALE: i128 = 1_i128 << SERVO_POSITION_FRACTION_BITS;

/// Exact signed Q31.32 position in configuration-defined axis units.
///
/// The lattice deliberately does not claim that its unit is a motor turn,
/// output turn, radian, or linear unit. The authoritative machine
/// configuration must establish that scale before target integration.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct ServoPosition(i64);

impl ServoPosition {
    /// Exact zero position.
    pub const ZERO: Self = Self(0);

    /// Constructs one exact Q31.32 lattice point.
    pub const fn from_bits(bits: i64) -> Self {
        Self(bits)
    }

    /// Returns the signed numerator over [`SERVO_POSITION_SCALE`].
    pub const fn bits(self) -> i64 {
        self.0
    }

    /// Checked addition in the exact position lattice.
    pub fn checked_add_bits(self, delta_bits: i64) -> Result<Self, ServoPositionIntervalError> {
        self.0
            .checked_add(delta_bits)
            .map(Self)
            .ok_or(ServoPositionIntervalError::Arithmetic)
    }
}

/// Conservative closed interval over Q31.32 axis position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoPositionInterval {
    lower: ServoPosition,
    upper: ServoPosition,
}

impl ServoPositionInterval {
    /// Constructs an exact point observation.
    pub const fn point(position: ServoPosition) -> Self {
        Self {
            lower: position,
            upper: position,
        }
    }

    /// Constructs one ordered closed position interval.
    pub const fn new(
        lower: ServoPosition,
        upper: ServoPosition,
    ) -> Result<Self, ServoPositionIntervalError> {
        if upper.0 < lower.0 {
            Err(ServoPositionIntervalError::Order)
        } else {
            Ok(Self { lower, upper })
        }
    }

    /// Inclusive lower endpoint.
    pub const fn lower(self) -> ServoPosition {
        self.lower
    }

    /// Inclusive upper endpoint.
    pub const fn upper(self) -> ServoPosition {
        self.upper
    }

    /// Deterministic midpoint rounded toward the lower endpoint.
    pub fn midpoint(self) -> ServoPosition {
        let lower = i128::from(self.lower.0);
        let width = i128::from(self.upper.0) - lower;
        ServoPosition((lower + width / 2) as i64)
    }

    /// Exact interval width in Q31.32 lattice units.
    pub fn width_ulps(self) -> u64 {
        (i128::from(self.upper.0) - i128::from(self.lower.0)) as u64
    }
}

/// Position-lattice construction or arithmetic failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoPositionIntervalError {
    /// The supplied upper endpoint preceded the lower endpoint.
    Order,
    /// A checked exact position operation overflowed Q31.32 storage.
    Arithmetic,
}

/// Wide closed position-error interval in Q31.32 lattice units.
///
/// An `i128` endpoint preserves the exact difference between any two `i64`
/// position endpoints without overflow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoPositionErrorInterval {
    lower_bits: i128,
    upper_bits: i128,
}

impl ServoPositionErrorInterval {
    fn from_target_and_observation(target: ServoPosition, observed: ServoPositionInterval) -> Self {
        Self {
            lower_bits: i128::from(target.0) - i128::from(observed.upper.0),
            upper_bits: i128::from(target.0) - i128::from(observed.lower.0),
        }
    }

    /// Inclusive lower endpoint in Q31.32 lattice units.
    pub const fn lower_bits(self) -> i128 {
        self.lower_bits
    }

    /// Inclusive upper endpoint in Q31.32 lattice units.
    pub const fn upper_bits(self) -> i128 {
        self.upper_bits
    }

    /// Largest absolute endpoint in exact Q31.32 lattice units.
    pub const fn maximum_absolute_bits(self) -> u128 {
        let lower = self.lower_bits.unsigned_abs();
        let upper = self.upper_bits.unsigned_abs();
        if lower > upper { lower } else { upper }
    }
}

/// Wide closed velocity-error interval whose endpoints retain Q2.30 units.
///
/// The widened `i64` endpoints preserve the valid `+2` and `-2` differences
/// that cannot themselves be stored in [`Q30`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoQ30ErrorInterval {
    lower_bits: i64,
    upper_bits: i64,
}

impl ServoQ30ErrorInterval {
    fn from_target_and_observation(target: Q30, observed: Q30Interval) -> Self {
        Self {
            lower_bits: i64::from(target.bits()) - i64::from(observed.upper().bits()),
            upper_bits: i64::from(target.bits()) - i64::from(observed.lower().bits()),
        }
    }

    /// Inclusive lower endpoint retaining the Q2.30 denominator.
    pub const fn lower_bits(self) -> i64 {
        self.lower_bits
    }

    /// Inclusive upper endpoint retaining the Q2.30 denominator.
    pub const fn upper_bits(self) -> i64 {
        self.upper_bits
    }
}

/// Integer device-cycle grid for nested current, velocity, and position loops.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoLoopGrid {
    epoch: DeviceCycle,
    device_cycle_hz: u32,
    timing: FocTimingProfile,
    current_period_cycles: u64,
    velocity_period_cycles: u64,
    position_period_cycles: u64,
}

impl ServoLoopGrid {
    /// Builds an exact nested grid rooted at one current-loop boundary.
    ///
    /// The velocity loop runs every `velocity_loop_divider` current updates.
    /// The position loop runs every `position_loop_divider` velocity updates.
    /// No fractional device-cycle relationship is rounded into existence.
    pub fn new(
        epoch: DeviceCycle,
        device_cycle_hz: u32,
        timing: FocTimingProfile,
    ) -> Result<Self, ServoLoopGridError> {
        timing.validate().map_err(ServoLoopGridError::Timing)?;
        if device_cycle_hz == 0 {
            return Err(ServoLoopGridError::ZeroDeviceClock);
        }
        if !device_cycle_hz.is_multiple_of(timing.current_loop_hz) {
            return Err(ServoLoopGridError::NonIntegralCurrentPeriod {
                device_cycle_hz,
                current_loop_hz: timing.current_loop_hz,
            });
        }
        let current_period_cycles = u64::from(device_cycle_hz / timing.current_loop_hz);
        let velocity_period_cycles = current_period_cycles
            .checked_mul(u64::from(timing.velocity_loop_divider))
            .ok_or(ServoLoopGridError::Arithmetic)?;
        let position_period_cycles = velocity_period_cycles
            .checked_mul(u64::from(timing.position_loop_divider))
            .ok_or(ServoLoopGridError::Arithmetic)?;
        Ok(Self {
            epoch,
            device_cycle_hz,
            timing,
            current_period_cycles,
            velocity_period_cycles,
            position_period_cycles,
        })
    }

    /// Classifies one exact current-loop boundary on this grid.
    pub fn tick(self, at: DeviceCycle) -> Result<ServoLoopTick, ServoLoopGridError> {
        let delta =
            at.0.checked_sub(self.epoch.0)
                .ok_or(ServoLoopGridError::BeforeEpoch {
                    epoch: self.epoch,
                    received: at,
                })?;
        if !delta.is_multiple_of(self.current_period_cycles) {
            return Err(ServoLoopGridError::OffGrid {
                epoch: self.epoch,
                received: at,
                current_period_cycles: self.current_period_cycles,
            });
        }
        let current_index = delta / self.current_period_cycles;
        Ok(ServoLoopTick {
            at,
            current_index,
            velocity_due: delta.is_multiple_of(self.velocity_period_cycles),
            position_due: delta.is_multiple_of(self.position_period_cycles),
        })
    }

    /// Root epoch shared by all three nested loops.
    pub const fn epoch(self) -> DeviceCycle {
        self.epoch
    }

    /// Exact device-cycle counter frequency.
    pub const fn device_cycle_hz(self) -> u32 {
        self.device_cycle_hz
    }

    /// Immutable FOC loop-divider facts.
    pub const fn timing(self) -> FocTimingProfile {
        self.timing
    }

    /// Exact current-loop period in device cycles.
    pub const fn current_period_cycles(self) -> u64 {
        self.current_period_cycles
    }

    /// Exact velocity-loop period in device cycles.
    pub const fn velocity_period_cycles(self) -> u64 {
        self.velocity_period_cycles
    }

    /// Exact position-loop period in device cycles.
    pub const fn position_period_cycles(self) -> u64 {
        self.position_period_cycles
    }
}

/// Exact loop work due at one current-loop boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoLoopTick {
    /// Exact device-cycle boundary.
    pub at: DeviceCycle,
    /// Zero-based current-loop index from the grid epoch.
    pub current_index: u64,
    /// Whether a new velocity observation and controller update are required.
    pub velocity_due: bool,
    /// Whether a new scheduled position setpoint is required.
    pub position_due: bool,
}

/// Servo loop-grid construction or boundary rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoLoopGridError {
    /// The nested FOC timing profile itself was invalid.
    Timing(FocError),
    /// Device-cycle frequency was zero.
    ZeroDeviceClock,
    /// One current-loop period required fractional device cycles.
    NonIntegralCurrentPeriod {
        device_cycle_hz: u32,
        current_loop_hz: u32,
    },
    /// A nested period multiplication overflowed.
    Arithmetic,
    /// The supplied boundary preceded the immutable epoch.
    BeforeEpoch {
        epoch: DeviceCycle,
        received: DeviceCycle,
    },
    /// The supplied boundary was not on the current-loop lattice.
    OffGrid {
        epoch: DeviceCycle,
        received: DeviceCycle,
        current_period_cycles: u64,
    },
}

/// Complete portable outer-loop profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoCascadeConfig {
    /// Identity shared with the complete inner-loop snapshot.
    pub configuration_digest: Digest,
    /// Position-error to normalized-velocity proportional gain.
    pub position_proportional_gain: Q30,
    /// Velocity-error to normalized q-current PI controller.
    pub velocity_controller: PiConfig,
    /// Symmetric normalized velocity limit.
    pub maximum_velocity: Q30,
    /// Symmetric normalized current-vector limit for the outer loop.
    pub maximum_current: Q30,
    /// Constant normalized d-current target.
    pub direct_current_target: Q30,
    /// Maximum worst-case Q31.32 position error, including observation width.
    pub maximum_following_error_bits: u64,
    /// Maximum age of a velocity-loop observation at its service boundary.
    pub maximum_sample_age_cycles: u64,
}

impl ServoCascadeConfig {
    /// Validates this profile against the complete immutable inner-loop snapshot.
    pub fn validate_for(
        self,
        grid: ServoLoopGrid,
        snapshot: &FocParameterSnapshot,
    ) -> Result<(), ServoCascadeProfileError> {
        snapshot
            .validate()
            .map_err(ServoCascadeProfileError::InnerSnapshot)?;
        if self.configuration_digest.is_zero() {
            return Err(ServoCascadeProfileError::MissingConfiguration);
        }
        if self.configuration_digest != snapshot.configuration_digest {
            return Err(ServoCascadeProfileError::ConfigurationMismatch {
                expected: snapshot.configuration_digest,
                received: self.configuration_digest,
            });
        }
        if grid.timing != snapshot.timing {
            return Err(ServoCascadeProfileError::TimingMismatch);
        }
        if self.position_proportional_gain.bits() < 0 {
            return Err(ServoCascadeProfileError::PositionGain);
        }
        if self.maximum_velocity.bits() <= 0 || self.maximum_velocity.bits() > Q30::ONE.bits() {
            return Err(ServoCascadeProfileError::VelocityLimit);
        }
        if self.maximum_current.bits() <= 0
            || self.maximum_current.bits() > snapshot.maximum_phase_current.bits()
        {
            return Err(ServoCascadeProfileError::CurrentLimit);
        }
        if self.maximum_following_error_bits == 0 {
            return Err(ServoCascadeProfileError::FollowingLimit);
        }
        if self.maximum_sample_age_cycles > grid.velocity_period_cycles {
            return Err(ServoCascadeProfileError::SampleAge {
                maximum_sample_age_cycles: self.maximum_sample_age_cycles,
                velocity_period_cycles: grid.velocity_period_cycles,
            });
        }
        self.velocity_controller
            .validate()
            .map_err(ServoCascadeProfileError::VelocityController)?;
        for quadrature in [
            self.velocity_controller.output_minimum,
            self.velocity_controller.output_maximum,
        ] {
            if !within_current_circle(self.direct_current_target, quadrature, self.maximum_current)
            {
                return Err(ServoCascadeProfileError::CurrentEnvelope);
            }
        }
        Ok(())
    }
}

/// Rejected portable outer-loop profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoCascadeProfileError {
    /// The complete immutable inner-loop snapshot was invalid.
    InnerSnapshot(FocError),
    /// No nonzero configuration identity was supplied.
    MissingConfiguration,
    /// Outer and inner loops named different immutable configurations.
    ConfigurationMismatch { expected: Digest, received: Digest },
    /// The loop grid did not retain the snapshot's exact timing facts.
    TimingMismatch,
    /// Position proportional gain was negative.
    PositionGain,
    /// Normalized velocity limit was not in `(0, 1]`.
    VelocityLimit,
    /// Outer-loop current limit was not positive or exceeded inner authority.
    CurrentLimit,
    /// Following-error limit was zero.
    FollowingLimit,
    /// Observations could remain older than one complete velocity period.
    SampleAge {
        maximum_sample_age_cycles: u64,
        velocity_period_cycles: u64,
    },
    /// Velocity PI parameters were invalid.
    VelocityController(FocError),
    /// A velocity-controller endpoint plus d-current exceeded the current circle.
    CurrentEnvelope,
}

/// One immutable scheduled outer-loop setpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoSetpoint {
    /// Contiguous nonzero identifier in the immutable setpoint stream.
    pub command_id: u32,
    /// Exact position-loop boundary at which this setpoint becomes active.
    pub scheduled_at: DeviceCycle,
    /// Complete immutable configuration identity.
    pub configuration_digest: Digest,
    /// Exact target in the configuration-defined Q31.32 position lattice.
    pub position: ServoPosition,
    /// Normalized velocity feed-forward point.
    pub velocity_feed_forward: Q30,
    /// Normalized q-current feed-forward point.
    pub quadrature_current_feed_forward: Q30,
}

/// One bounded mechanical observation supplied at a velocity-loop boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoKinematicSample {
    /// Contiguous nonzero sample identity.
    pub sequence: u32,
    /// Device cycle represented by the observation.
    pub sampled_at: DeviceCycle,
    /// Device cycle at which the complete observation became available.
    pub available_at: DeviceCycle,
    /// Complete immutable configuration identity.
    pub configuration_digest: Digest,
    /// Conservative Q31.32 position enclosure.
    pub position: ServoPositionInterval,
    /// Conservative normalized velocity enclosure.
    pub velocity: Q30Interval,
}

/// One complete cascaded-service result retained for replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoCascadeUpdate {
    /// Exact current-loop boundary serviced.
    pub at: DeviceCycle,
    /// Zero-based current-loop index.
    pub current_index: u64,
    /// Active setpoint identifier.
    pub setpoint_id: u32,
    /// Latest accepted kinematic sample sequence.
    pub sample_sequence: u32,
    /// Whether this boundary recomputed the position loop.
    pub position_updated: bool,
    /// Whether this boundary recomputed the velocity loop.
    pub velocity_updated: bool,
    /// New wide position-error enclosure when the position loop ran.
    pub position_error: Option<ServoPositionErrorInterval>,
    /// New wide velocity-error enclosure when the velocity loop ran.
    pub velocity_error: Option<ServoQ30ErrorInterval>,
    /// Selected normalized velocity target held for subsequent current ticks.
    pub velocity_target: Q30,
    /// Conservative target enclosure induced by position observation uncertainty.
    pub velocity_target_interval: Q30Interval,
    /// New PI result when the velocity loop ran.
    pub velocity_control: Option<PiUpdate>,
    /// Complete normalized dq-current target for the inner loop.
    pub current_target: DqPoint,
}

/// Allocation-free, fail-closed position/velocity cascade.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CascadedServoController {
    grid: ServoLoopGrid,
    config: ServoCascadeConfig,
    velocity_controller: PiController,
    last_current_index: Option<u64>,
    last_setpoint_id: Option<u32>,
    last_sample_sequence: Option<u32>,
    last_sampled_at: Option<DeviceCycle>,
    held_velocity_target: Q30,
    held_velocity_target_interval: Q30Interval,
    held_quadrature_feed_forward: Q30,
    held_current_target: DqPoint,
    position_updates: u64,
    velocity_updates: u64,
    fault: Option<ServoCascadeError>,
}

impl CascadedServoController {
    /// Constructs a zero-state cascade from complete outer and inner profiles.
    pub fn new(
        grid: ServoLoopGrid,
        config: ServoCascadeConfig,
        snapshot: &FocParameterSnapshot,
    ) -> Result<Self, ServoCascadeProfileError> {
        config.validate_for(grid, snapshot)?;
        let velocity_controller = PiController::new(config.velocity_controller)
            .map_err(ServoCascadeProfileError::VelocityController)?;
        Ok(Self {
            grid,
            config,
            velocity_controller,
            last_current_index: None,
            last_setpoint_id: None,
            last_sample_sequence: None,
            last_sampled_at: None,
            held_velocity_target: Q30::ZERO,
            held_velocity_target_interval: Q30Interval::ZERO,
            held_quadrature_feed_forward: Q30::ZERO,
            held_current_target: DqPoint::default(),
            position_updates: 0,
            velocity_updates: 0,
            fault: None,
        })
    }

    /// Services exactly one contiguous current-loop boundary transactionally.
    ///
    /// A setpoint is required only at a position boundary, and a complete
    /// kinematic sample is required only at a velocity boundary. Supplying or
    /// omitting either at another shape is a terminal schedule error. On any
    /// error, controller state and accepted identities remain unchanged while
    /// the exact first cause is latched.
    pub fn service(
        &mut self,
        at: DeviceCycle,
        setpoint: Option<ServoSetpoint>,
        sample: Option<ServoKinematicSample>,
    ) -> Result<ServoCascadeUpdate, ServoCascadeError> {
        if self.fault.is_some() {
            return Err(ServoCascadeError::FaultLatched);
        }
        let mut next = *self;
        match next.service_inner(at, setpoint, sample) {
            Ok(update) => {
                *self = next;
                Ok(update)
            }
            Err(error) => {
                self.fault = Some(error);
                Err(error)
            }
        }
    }

    /// Immutable exact loop grid.
    pub const fn grid(&self) -> ServoLoopGrid {
        self.grid
    }

    /// Immutable validated outer-loop profile.
    pub const fn config(&self) -> ServoCascadeConfig {
        self.config
    }

    /// Last contiguous current-loop index accepted.
    pub const fn last_current_index(&self) -> Option<u64> {
        self.last_current_index
    }

    /// Complete position-loop updates accepted.
    pub const fn position_updates(&self) -> u64 {
        self.position_updates
    }

    /// Complete velocity-loop updates accepted.
    pub const fn velocity_updates(&self) -> u64 {
        self.velocity_updates
    }

    /// Current held inner-loop dq target.
    pub const fn current_target(&self) -> DqPoint {
        self.held_current_target
    }

    /// First retained cascade fault, if any.
    pub const fn fault(&self) -> Option<ServoCascadeError> {
        self.fault
    }

    fn service_inner(
        &mut self,
        at: DeviceCycle,
        setpoint: Option<ServoSetpoint>,
        sample: Option<ServoKinematicSample>,
    ) -> Result<ServoCascadeUpdate, ServoCascadeError> {
        let tick = self.grid.tick(at).map_err(ServoCascadeError::Grid)?;
        let expected_index = match self.last_current_index {
            Some(index) => index
                .checked_add(1)
                .ok_or(ServoCascadeError::CounterOverflow)?,
            None => 0,
        };
        if tick.current_index != expected_index {
            return Err(ServoCascadeError::TickSequence {
                expected: expected_index,
                received: tick.current_index,
            });
        }
        if setpoint.is_some() != tick.position_due {
            return Err(ServoCascadeError::SetpointPresence {
                required: tick.position_due,
                received: setpoint.is_some(),
            });
        }
        if sample.is_some() != tick.velocity_due {
            return Err(ServoCascadeError::SamplePresence {
                required: tick.velocity_due,
                received: sample.is_some(),
            });
        }

        let mut position_error = None;
        let mut velocity_error = None;
        let mut velocity_control = None;

        if let Some(setpoint) = setpoint {
            self.validate_setpoint(setpoint, at)?;
            self.last_setpoint_id = Some(setpoint.command_id);
            self.held_quadrature_feed_forward = setpoint.quadrature_current_feed_forward;
        }

        if let Some(sample) = sample {
            self.validate_sample(sample, at)?;
            self.last_sample_sequence = Some(sample.sequence);
            self.last_sampled_at = Some(sample.sampled_at);

            if tick.position_due {
                let setpoint = setpoint.ok_or(ServoCascadeError::InternalState)?;
                let error = ServoPositionErrorInterval::from_target_and_observation(
                    setpoint.position,
                    sample.position,
                );
                if error.maximum_absolute_bits()
                    > u128::from(self.config.maximum_following_error_bits)
                {
                    return Err(ServoCascadeError::FollowingError {
                        maximum_observed_bits: error.maximum_absolute_bits(),
                        maximum_allowed_bits: self.config.maximum_following_error_bits,
                    });
                }
                let (target, interval) = position_velocity_target(
                    self.config.position_proportional_gain,
                    error,
                    i128::from(setpoint.position.bits())
                        - i128::from(sample.position.midpoint().bits()),
                    setpoint.velocity_feed_forward,
                    self.config.maximum_velocity,
                )?;
                self.held_velocity_target = target;
                self.held_velocity_target_interval = interval;
                self.position_updates = self
                    .position_updates
                    .checked_add(1)
                    .ok_or(ServoCascadeError::CounterOverflow)?;
                position_error = Some(error);
            }

            let error = ServoQ30ErrorInterval::from_target_and_observation(
                self.held_velocity_target,
                sample.velocity,
            );
            let selected_error = i64::from(self.held_velocity_target.bits())
                - i64::from(sample.velocity.midpoint().bits());
            let control = self
                .velocity_controller
                .update_error_bits(selected_error, self.held_quadrature_feed_forward)
                .map_err(ServoCascadeError::Arithmetic)?;
            self.held_current_target = DqPoint {
                d: self.config.direct_current_target,
                q: control.output,
            };
            self.velocity_updates = self
                .velocity_updates
                .checked_add(1)
                .ok_or(ServoCascadeError::CounterOverflow)?;
            velocity_error = Some(error);
            velocity_control = Some(control);
        }

        self.last_current_index = Some(tick.current_index);
        Ok(ServoCascadeUpdate {
            at,
            current_index: tick.current_index,
            setpoint_id: self
                .last_setpoint_id
                .ok_or(ServoCascadeError::InternalState)?,
            sample_sequence: self
                .last_sample_sequence
                .ok_or(ServoCascadeError::InternalState)?,
            position_updated: tick.position_due,
            velocity_updated: tick.velocity_due,
            position_error,
            velocity_error,
            velocity_target: self.held_velocity_target,
            velocity_target_interval: self.held_velocity_target_interval,
            velocity_control,
            current_target: self.held_current_target,
        })
    }

    fn validate_setpoint(
        &self,
        setpoint: ServoSetpoint,
        at: DeviceCycle,
    ) -> Result<(), ServoCascadeError> {
        if setpoint.configuration_digest != self.config.configuration_digest {
            return Err(ServoCascadeError::Configuration {
                expected: self.config.configuration_digest,
                received: setpoint.configuration_digest,
            });
        }
        if setpoint.scheduled_at != at {
            return Err(ServoCascadeError::SetpointCycle {
                expected: at,
                received: setpoint.scheduled_at,
            });
        }
        let expected = match self.last_setpoint_id {
            Some(id) => id
                .checked_add(1)
                .ok_or(ServoCascadeError::CounterOverflow)?,
            None => 1,
        };
        if setpoint.command_id != expected {
            return Err(ServoCascadeError::SetpointSequence {
                expected,
                received: setpoint.command_id,
            });
        }
        if absolute_q30_bits(setpoint.velocity_feed_forward)
            > i64::from(self.config.maximum_velocity.bits())
            || !within_current_circle(
                self.config.direct_current_target,
                setpoint.quadrature_current_feed_forward,
                self.config.maximum_current,
            )
        {
            return Err(ServoCascadeError::SetpointRange);
        }
        Ok(())
    }

    fn validate_sample(
        &self,
        sample: ServoKinematicSample,
        at: DeviceCycle,
    ) -> Result<(), ServoCascadeError> {
        if sample.configuration_digest != self.config.configuration_digest {
            return Err(ServoCascadeError::Configuration {
                expected: self.config.configuration_digest,
                received: sample.configuration_digest,
            });
        }
        let expected = match self.last_sample_sequence {
            Some(sequence) => sequence
                .checked_add(1)
                .ok_or(ServoCascadeError::CounterOverflow)?,
            None => 1,
        };
        if sample.sequence != expected {
            return Err(ServoCascadeError::SampleSequence {
                expected,
                received: sample.sequence,
            });
        }
        if let Some(prior) = self.last_sampled_at
            && sample.sampled_at.0 <= prior.0
        {
            return Err(ServoCascadeError::SampleTimeOrder {
                prior,
                received: sample.sampled_at,
            });
        }
        if sample.available_at.0 < sample.sampled_at.0 {
            return Err(ServoCascadeError::SampleWindowOrder {
                sampled_at: sample.sampled_at,
                available_at: sample.available_at,
            });
        }
        if sample.available_at.0 > at.0 {
            return Err(ServoCascadeError::SampleAvailableAfterService {
                service_at: at,
                available_at: sample.available_at,
            });
        }
        let age = at.0 - sample.sampled_at.0;
        if age > self.config.maximum_sample_age_cycles {
            return Err(ServoCascadeError::SampleStale {
                age_cycles: age,
                maximum_age_cycles: self.config.maximum_sample_age_cycles,
            });
        }
        let maximum_velocity = i64::from(self.config.maximum_velocity.bits());
        if absolute_q30_bits(sample.velocity.lower()) > maximum_velocity
            || absolute_q30_bits(sample.velocity.upper()) > maximum_velocity
        {
            return Err(ServoCascadeError::SampleVelocityRange {
                lower: sample.velocity.lower(),
                upper: sample.velocity.upper(),
                maximum: self.config.maximum_velocity,
            });
        }
        Ok(())
    }
}

/// First-cause runtime rejection from the portable servo cascade.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoCascadeError {
    /// A device-cycle boundary did not belong to the immutable loop grid.
    Grid(ServoLoopGridError),
    /// Current-loop service skipped, repeated, or reordered a grid index.
    TickSequence { expected: u64, received: u64 },
    /// Setpoint presence did not match the position-loop schedule.
    SetpointPresence { required: bool, received: bool },
    /// Sample presence did not match the velocity-loop schedule.
    SamplePresence { required: bool, received: bool },
    /// A setpoint or sample named another immutable configuration.
    Configuration { expected: Digest, received: Digest },
    /// A setpoint named another position-loop boundary.
    SetpointCycle {
        expected: DeviceCycle,
        received: DeviceCycle,
    },
    /// Setpoint identifiers were not contiguous from one.
    SetpointSequence { expected: u32, received: u32 },
    /// Sample identifiers were not contiguous from one.
    SampleSequence { expected: u32, received: u32 },
    /// A new sample did not represent a strictly later physical instant.
    SampleTimeOrder {
        prior: DeviceCycle,
        received: DeviceCycle,
    },
    /// A sample became available before the instant it represented.
    SampleWindowOrder {
        sampled_at: DeviceCycle,
        available_at: DeviceCycle,
    },
    /// A sample was not complete at the service boundary.
    SampleAvailableAfterService {
        service_at: DeviceCycle,
        available_at: DeviceCycle,
    },
    /// A complete sample exceeded the configured exact age.
    SampleStale {
        age_cycles: u64,
        maximum_age_cycles: u64,
    },
    /// Setpoint velocity/current feed-forward exceeded the validated profile.
    SetpointRange,
    /// The conservative velocity observation exceeded the normalized limit.
    SampleVelocityRange {
        lower: Q30,
        upper: Q30,
        maximum: Q30,
    },
    /// Worst-case position error, including uncertainty, exceeded policy.
    FollowingError {
        maximum_observed_bits: u128,
        maximum_allowed_bits: u64,
    },
    /// Fixed-point control arithmetic rejected an intermediate.
    Arithmetic(FocError),
    /// An exact sequence or update counter overflowed.
    CounterOverflow,
    /// Internally retained schedule state was incomplete.
    InternalState,
    /// A prior first cause remains latched.
    FaultLatched,
}

fn position_velocity_target(
    gain: Q30,
    error: ServoPositionErrorInterval,
    selected_error_bits: i128,
    feed_forward: Q30,
    maximum_velocity: Q30,
) -> Result<(Q30, Q30Interval), ServoCascadeError> {
    let gain_bits = i128::from(gain.bits());
    let lower_product = error
        .lower_bits
        .checked_mul(gain_bits)
        .ok_or(ServoCascadeError::Arithmetic(FocError::Range))?;
    let upper_product = error
        .upper_bits
        .checked_mul(gain_bits)
        .ok_or(ServoCascadeError::Arithmetic(FocError::Range))?;
    let lower = div_floor(lower_product, SERVO_POSITION_SCALE)
        .checked_add(i128::from(feed_forward.bits()))
        .ok_or(ServoCascadeError::Arithmetic(FocError::Range))?;
    let upper = div_ceil(upper_product, SERVO_POSITION_SCALE)
        .checked_add(i128::from(feed_forward.bits()))
        .ok_or(ServoCascadeError::Arithmetic(FocError::Range))?;
    let minimum = -i128::from(maximum_velocity.bits());
    let maximum = i128::from(maximum_velocity.bits());
    let lower = i32::try_from(lower.clamp(minimum, maximum))
        .map_err(|_| ServoCascadeError::Arithmetic(FocError::Range))?;
    let upper = i32::try_from(upper.clamp(minimum, maximum))
        .map_err(|_| ServoCascadeError::Arithmetic(FocError::Range))?;
    let interval = Q30Interval::new(Q30::from_bits(lower), Q30::from_bits(upper))
        .map_err(ServoCascadeError::Arithmetic)?;

    let selected_product = selected_error_bits
        .checked_mul(gain_bits)
        .ok_or(ServoCascadeError::Arithmetic(FocError::Range))?;
    let selected = round_ties_even(selected_product, SERVO_POSITION_SCALE)
        .checked_add(i128::from(feed_forward.bits()))
        .ok_or(ServoCascadeError::Arithmetic(FocError::Range))?
        .clamp(minimum, maximum);
    let selected = i32::try_from(selected)
        .map(Q30::from_bits)
        .map_err(|_| ServoCascadeError::Arithmetic(FocError::Range))?;
    Ok((selected, interval))
}

const fn absolute_q30_bits(value: Q30) -> i64 {
    if value.bits() < 0 {
        -(value.bits() as i64)
    } else {
        value.bits() as i64
    }
}

const fn within_current_circle(direct: Q30, quadrature: Q30, limit: Q30) -> bool {
    let direct = direct.bits() as i128;
    let quadrature = quadrature.bits() as i128;
    let limit = limit.bits() as i128;
    direct * direct + quadrature * quadrature <= limit * limit
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: Digest = Digest([0x5a; 32]);

    fn point(numerator: i64, denominator: i64) -> Q30 {
        Q30Interval::from_ratio(numerator, denominator)
            .unwrap()
            .midpoint()
    }

    fn timing() -> FocTimingProfile {
        FocTimingProfile {
            pwm_hz: 20_000,
            current_loop_hz: 10_000,
            velocity_loop_divider: 10,
            position_loop_divider: 5,
        }
    }

    fn pi(output: Q30) -> PiConfig {
        PiConfig {
            proportional_gain: Q30::HALF,
            integral_gain_per_update: point(1, 16),
            integral_minimum: output.checked_neg().unwrap(),
            integral_maximum: output,
            output_minimum: output.checked_neg().unwrap(),
            output_maximum: output,
        }
    }

    fn snapshot() -> FocParameterSnapshot {
        FocParameterSnapshot {
            configuration_digest: DIGEST,
            pole_pairs: 7,
            timing: timing(),
            maximum_phase_current: point(3, 4),
            maximum_phase_voltage: point(3, 4),
            d_current: pi(Q30::HALF),
            q_current: pi(Q30::HALF),
        }
    }

    fn grid() -> ServoLoopGrid {
        ServoLoopGrid::new(DeviceCycle(1_000), 1_000_000, timing()).unwrap()
    }

    fn config() -> ServoCascadeConfig {
        ServoCascadeConfig {
            configuration_digest: DIGEST,
            position_proportional_gain: Q30::ONE,
            velocity_controller: pi(Q30::HALF),
            maximum_velocity: Q30::HALF,
            maximum_current: point(3, 4),
            direct_current_target: Q30::ZERO,
            maximum_following_error_bits: 1_u64 << 32,
            maximum_sample_age_cycles: 100,
        }
    }

    fn setpoint(id: u32, at: u64, position_bits: i64) -> ServoSetpoint {
        ServoSetpoint {
            command_id: id,
            scheduled_at: DeviceCycle(at),
            configuration_digest: DIGEST,
            position: ServoPosition::from_bits(position_bits),
            velocity_feed_forward: Q30::ZERO,
            quadrature_current_feed_forward: Q30::ZERO,
        }
    }

    fn sample(sequence: u32, at: u64, position_bits: i64, velocity: Q30) -> ServoKinematicSample {
        ServoKinematicSample {
            sequence,
            sampled_at: DeviceCycle(at),
            available_at: DeviceCycle(at),
            configuration_digest: DIGEST,
            position: ServoPositionInterval::point(ServoPosition::from_bits(position_bits)),
            velocity: Q30Interval::point(velocity),
        }
    }

    #[test]
    fn q31_32_intervals_preserve_extreme_width_and_midpoint() {
        let interval = ServoPositionInterval::new(
            ServoPosition::from_bits(i64::MIN),
            ServoPosition::from_bits(i64::MAX),
        )
        .unwrap();
        assert_eq!(interval.width_ulps(), u64::MAX);
        assert_eq!(interval.midpoint(), ServoPosition::from_bits(-1));
        assert_eq!(
            ServoPositionInterval::new(ServoPosition::from_bits(1), ServoPosition::from_bits(0)),
            Err(ServoPositionIntervalError::Order)
        );
        assert_eq!(
            ServoPosition::from_bits(i64::MAX).checked_add_bits(1),
            Err(ServoPositionIntervalError::Arithmetic)
        );
    }

    #[test]
    fn nested_loop_grid_is_integer_exact_and_classifies_boundaries() {
        let grid = grid();
        assert_eq!(grid.current_period_cycles(), 100);
        assert_eq!(grid.velocity_period_cycles(), 1_000);
        assert_eq!(grid.position_period_cycles(), 5_000);
        assert_eq!(
            grid.tick(DeviceCycle(1_000)).unwrap(),
            ServoLoopTick {
                at: DeviceCycle(1_000),
                current_index: 0,
                velocity_due: true,
                position_due: true,
            }
        );
        assert!(!grid.tick(DeviceCycle(1_100)).unwrap().velocity_due);
        assert!(grid.tick(DeviceCycle(2_000)).unwrap().velocity_due);
        assert!(!grid.tick(DeviceCycle(2_000)).unwrap().position_due);
        assert!(grid.tick(DeviceCycle(6_000)).unwrap().position_due);
        assert!(matches!(
            grid.tick(DeviceCycle(1_001)),
            Err(ServoLoopGridError::OffGrid { .. })
        ));
        assert!(matches!(
            grid.tick(DeviceCycle(999)),
            Err(ServoLoopGridError::BeforeEpoch { .. })
        ));
        assert!(matches!(
            ServoLoopGrid::new(DeviceCycle(0), 1_000_001, timing()),
            Err(ServoLoopGridError::NonIntegralCurrentPeriod { .. })
        ));
    }

    #[test]
    fn profile_binds_snapshot_timing_limits_and_current_circle() {
        config().validate_for(grid(), &snapshot()).unwrap();

        let mut invalid = config();
        invalid.position_proportional_gain = Q30::NEG_ONE;
        assert_eq!(
            invalid.validate_for(grid(), &snapshot()),
            Err(ServoCascadeProfileError::PositionGain)
        );
        invalid = config();
        invalid.maximum_sample_age_cycles = 1_001;
        assert_eq!(
            invalid.validate_for(grid(), &snapshot()),
            Err(ServoCascadeProfileError::SampleAge {
                maximum_sample_age_cycles: 1_001,
                velocity_period_cycles: 1_000,
            })
        );
        invalid = config();
        invalid.direct_current_target = Q30::HALF;
        invalid.velocity_controller = pi(point(2, 3));
        assert_eq!(
            invalid.validate_for(grid(), &snapshot()),
            Err(ServoCascadeProfileError::CurrentEnvelope)
        );
    }

    #[test]
    fn cascade_updates_only_on_exact_nested_boundaries_and_holds_current() {
        let mut controller = CascadedServoController::new(grid(), config(), &snapshot()).unwrap();
        let first = controller
            .service(
                DeviceCycle(1_000),
                Some(setpoint(1, 1_000, 1_i64 << 29)),
                Some(sample(1, 1_000, 0, Q30::ZERO)),
            )
            .unwrap();
        assert!(first.position_updated);
        assert!(first.velocity_updated);
        assert_eq!(first.velocity_target, point(1, 8));
        assert_eq!(
            first.velocity_target_interval,
            Q30Interval::point(point(1, 8))
        );
        assert_eq!(
            first.velocity_error.unwrap().lower_bits(),
            i64::from(point(1, 8).bits())
        );
        assert_eq!(first.current_target.d, Q30::ZERO);
        assert!(first.current_target.q > Q30::ZERO);
        let held = first.current_target;

        for index in 1..10 {
            let at = 1_000 + index * 100;
            let update = controller.service(DeviceCycle(at), None, None).unwrap();
            assert!(!update.position_updated);
            assert!(!update.velocity_updated);
            assert_eq!(update.current_target, held);
        }
        let velocity = controller
            .service(
                DeviceCycle(2_000),
                None,
                Some(sample(2, 2_000, 0, point(1, 16))),
            )
            .unwrap();
        assert!(!velocity.position_updated);
        assert!(velocity.velocity_updated);
        assert_eq!(controller.position_updates(), 1);
        assert_eq!(controller.velocity_updates(), 2);
        assert_eq!(controller.last_current_index(), Some(10));
        assert_eq!(controller.fault(), None);
    }

    #[test]
    fn position_uncertainty_is_outward_and_wide_velocity_error_is_retained() {
        let mut profile = config();
        profile.maximum_velocity = Q30::ONE;
        let mut controller = CascadedServoController::new(grid(), profile, &snapshot()).unwrap();
        let observed =
            ServoPositionInterval::new(ServoPosition::from_bits(-1), ServoPosition::from_bits(1))
                .unwrap();
        let update = controller
            .service(
                DeviceCycle(1_000),
                Some(ServoSetpoint {
                    position: ServoPosition::from_bits(1_i64 << 31),
                    ..setpoint(1, 1_000, 0)
                }),
                Some(ServoKinematicSample {
                    position: observed,
                    velocity: Q30Interval::point(Q30::NEG_ONE),
                    ..sample(1, 1_000, 0, Q30::ZERO)
                }),
            )
            .unwrap();
        let position_error = update.position_error.unwrap();
        assert_eq!(position_error.lower_bits(), (1_i128 << 31) - 1);
        assert_eq!(position_error.upper_bits(), (1_i128 << 31) + 1);
        assert_eq!(update.velocity_target, Q30::HALF);
        assert!(update.velocity_target_interval.contains(Q30::HALF));
        let velocity_error = update.velocity_error.unwrap();
        assert_eq!(
            velocity_error.lower_bits(),
            i64::from(Q30::HALF.bits()) - i64::from(Q30::NEG_ONE.bits())
        );
        assert_eq!(velocity_error.lower_bits(), velocity_error.upper_bits());
    }

    #[test]
    fn timing_and_input_substitution_latch_without_partial_state() {
        let mut controller = CascadedServoController::new(grid(), config(), &snapshot()).unwrap();
        let error = controller
            .service(DeviceCycle(1_000), Some(setpoint(1, 1_000, 0)), None)
            .unwrap_err();
        assert_eq!(
            error,
            ServoCascadeError::SamplePresence {
                required: true,
                received: false,
            }
        );
        assert_eq!(controller.last_current_index(), None);
        assert_eq!(controller.position_updates(), 0);
        assert_eq!(controller.velocity_updates(), 0);
        assert_eq!(controller.current_target(), DqPoint::default());
        assert_eq!(controller.fault(), Some(error));
        assert_eq!(
            controller.service(
                DeviceCycle(1_000),
                Some(setpoint(1, 1_000, 0)),
                Some(sample(1, 1_000, 0, Q30::ZERO)),
            ),
            Err(ServoCascadeError::FaultLatched)
        );
    }

    #[test]
    fn stale_overspeed_and_following_observations_fault_before_control() {
        let cases = [
            (
                ServoKinematicSample {
                    sampled_at: DeviceCycle(899),
                    available_at: DeviceCycle(1_000),
                    ..sample(1, 1_000, 0, Q30::ZERO)
                },
                setpoint(1, 1_000, 0),
                ServoCascadeError::SampleStale {
                    age_cycles: 101,
                    maximum_age_cycles: 100,
                },
            ),
            (
                sample(1, 1_000, 0, point(3, 4)),
                setpoint(1, 1_000, 0),
                ServoCascadeError::SampleVelocityRange {
                    lower: point(3, 4),
                    upper: point(3, 4),
                    maximum: Q30::HALF,
                },
            ),
            (
                sample(1, 1_000, 0, Q30::ZERO),
                setpoint(1, 1_000, (1_i64 << 32) + 1),
                ServoCascadeError::FollowingError {
                    maximum_observed_bits: (1_u128 << 32) + 1,
                    maximum_allowed_bits: 1_u64 << 32,
                },
            ),
        ];
        for (sample, setpoint, expected) in cases {
            let mut controller =
                CascadedServoController::new(grid(), config(), &snapshot()).unwrap();
            assert_eq!(
                controller.service(DeviceCycle(1_000), Some(setpoint), Some(sample)),
                Err(expected)
            );
            assert_eq!(controller.current_target(), DqPoint::default());
            assert_eq!(controller.position_updates(), 0);
            assert_eq!(controller.velocity_updates(), 0);
        }
    }

    #[test]
    fn skipped_tick_and_noncontiguous_identities_are_terminal() {
        let mut skipped = CascadedServoController::new(grid(), config(), &snapshot()).unwrap();
        assert_eq!(
            skipped.service(DeviceCycle(1_100), None, None),
            Err(ServoCascadeError::TickSequence {
                expected: 0,
                received: 1,
            })
        );

        let mut wrong_id = CascadedServoController::new(grid(), config(), &snapshot()).unwrap();
        assert_eq!(
            wrong_id.service(
                DeviceCycle(1_000),
                Some(setpoint(2, 1_000, 0)),
                Some(sample(1, 1_000, 0, Q30::ZERO)),
            ),
            Err(ServoCascadeError::SetpointSequence {
                expected: 1,
                received: 2,
            })
        );
    }

    #[test]
    fn repeated_physical_sample_time_cannot_advance_the_velocity_loop() {
        let mut profile = config();
        profile.maximum_sample_age_cycles = 1_000;
        let mut controller = CascadedServoController::new(grid(), profile, &snapshot()).unwrap();
        let first = controller
            .service(
                DeviceCycle(1_000),
                Some(setpoint(1, 1_000, 0)),
                Some(sample(1, 1_000, 0, Q30::ZERO)),
            )
            .unwrap();
        for index in 1..10 {
            controller
                .service(DeviceCycle(1_000 + index * 100), None, None)
                .unwrap();
        }
        assert_eq!(
            controller.service(
                DeviceCycle(2_000),
                None,
                Some(ServoKinematicSample {
                    sequence: 2,
                    sampled_at: DeviceCycle(1_000),
                    available_at: DeviceCycle(2_000),
                    configuration_digest: DIGEST,
                    position: ServoPositionInterval::point(ServoPosition::ZERO),
                    velocity: Q30Interval::ZERO,
                }),
            ),
            Err(ServoCascadeError::SampleTimeOrder {
                prior: DeviceCycle(1_000),
                received: DeviceCycle(1_000),
            })
        );
        assert_eq!(controller.position_updates(), 1);
        assert_eq!(controller.velocity_updates(), 1);
        assert_eq!(controller.current_target(), first.current_target);
    }
}
