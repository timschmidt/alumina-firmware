//! Deterministic dimensionless plant models for portable FOC control tests.
//!
//! These models establish arithmetic, state, saturation, and replay behavior.
//! They are intentionally not motor identification, electrical simulation, or
//! evidence that a physical power stage is safe to energize.

use alumina_foc::{
    CascadedServoController, DqControlUpdate, DqCurrentController, DqPoint, FocError, Q30,
    Q30Interval, RotorCountDirection, ServoCascadeError, ServoCascadeUpdate, ServoEncoderError,
    ServoEncoderEstimate, ServoEncoderEstimator, ServoKinematicSample, ServoPosition,
    ServoPositionInterval, ServoSetpoint,
};
use alumina_protocol::DeviceCycle;

/// Plant configuration or fixed-point execution rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DqPlantError {
    /// The per-update response was outside the exact `(0, 1]` interval.
    Response,
    /// A state or drive vector exceeded the normalized unit circle.
    VectorRange,
    /// The requested trace length did not fit the deterministic update counter.
    TraceLength,
    /// The portable controller or plant encountered an arithmetic rejection.
    Arithmetic(FocError),
}

/// Dimensionless servo-plant or cascaded-control rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoPlantError {
    /// Per-update velocity response was outside `(0, 1]`.
    VelocityResponse,
    /// Position integration scale was outside `(0, 1]`.
    PositionScale,
    /// A supplied dq-current vector exceeded the normalized unit circle.
    CurrentRange,
    /// A fixed trace index, cycle, position, or identity overflowed.
    Arithmetic,
    /// Portable FOC fixed-point arithmetic rejected one plant operation.
    Foc(FocError),
    /// The portable outer-loop owner rejected one exact service boundary.
    Cascade(ServoCascadeError),
}

impl From<FocError> for ServoPlantError {
    fn from(error: FocError) -> Self {
        Self::Foc(error)
    }
}

impl From<ServoCascadeError> for ServoPlantError {
    fn from(error: ServoCascadeError) -> Self {
        Self::Cascade(error)
    }
}

/// Deterministic absolute-encoder truth replay rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncoderReplayError {
    /// A trace index, count, or device-cycle calculation overflowed.
    Arithmetic,
    /// The portable estimator rejected one generated observation.
    Estimator(ServoEncoderError),
    /// The estimator's retained multi-turn count differed from simulator truth.
    TruthMismatch { expected: i64, received: i64 },
}

impl From<ServoEncoderError> for EncoderReplayError {
    fn from(error: ServoEncoderError) -> Self {
        Self::Estimator(error)
    }
}

impl From<FocError> for DqPlantError {
    fn from(error: FocError) -> Self {
        Self::Arithmetic(error)
    }
}

/// One deterministic dimensionless first-order dq-current plant.
///
/// Each update applies the exact-lattice convex recurrence
/// `current' = (1 - response) * current + response * voltage`. This isolates
/// controller and numerical behavior from any unmeasured resistance,
/// inductance, back-EMF, inertia, load, or inverter fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FirstOrderDqPlant {
    response_per_update: Q30,
    current: DqPoint,
    updates: u64,
}

/// One complete controller/plant state transition retained for replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DqLoopSample {
    /// Zero-based update index.
    pub index: u64,
    /// Plant current supplied to the controller.
    pub measured_before: DqPoint,
    /// Complete two-axis controller result.
    pub control: DqControlUpdate,
    /// Plant current after applying the selected voltage.
    pub measured_after: DqPoint,
}

/// Deterministic dimensionless single-axis mechanical response.
///
/// Velocity follows q current through the exact convex recurrence
/// `velocity' = (1 - response) * velocity + response * q_current`. Position
/// then integrates `velocity' * position_scale` on the Q31.32 lattice. These
/// coefficients are synthetic numerical fixtures, not motor parameters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FirstOrderServoPlant {
    velocity_response_per_update: Q30,
    position_scale_per_update: Q30,
    position: ServoPosition,
    velocity: Q30,
    updates: u64,
}

/// One complete cascaded-controller and mechanical-plant replay record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoLoopSample {
    /// Zero-based current-loop update index.
    pub index: u64,
    /// Exact device-cycle service boundary.
    pub at: DeviceCycle,
    /// Plant position supplied to any due outer loop.
    pub position_before: ServoPosition,
    /// Plant velocity supplied to any due outer loop.
    pub velocity_before: Q30,
    /// Complete portable cascaded-controller result.
    pub control: ServoCascadeUpdate,
    /// Plant position after applying the held q-current target.
    pub position_after: ServoPosition,
    /// Plant velocity after applying the held q-current target.
    pub velocity_after: Q30,
}

/// One independent wrapping-count truth and estimator result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncoderReplaySample {
    /// Zero-based replay index.
    pub index: u64,
    /// Known directed multi-turn count after the requested truth delta.
    pub true_unwrapped_count: i64,
    /// Independently wrapped raw count supplied to the estimator.
    pub raw_count: u32,
    /// Complete portable estimator result.
    pub estimate: ServoEncoderEstimate,
}

impl FirstOrderDqPlant {
    /// Constructs a plant at zero current.
    ///
    /// # Errors
    ///
    /// Returns [`DqPlantError::Response`] unless `response_per_update` is in the
    /// exact interval `(0, 1]`.
    pub const fn new(response_per_update: Q30) -> Result<Self, DqPlantError> {
        Self::with_current(
            response_per_update,
            DqPoint {
                d: Q30::ZERO,
                q: Q30::ZERO,
            },
        )
    }

    /// Constructs a plant at an explicit normalized current.
    ///
    /// # Errors
    ///
    /// Returns [`DqPlantError::Response`] for an invalid response and
    /// [`DqPlantError::VectorRange`] when `current` exceeds the unit circle.
    pub const fn with_current(
        response_per_update: Q30,
        current: DqPoint,
    ) -> Result<Self, DqPlantError> {
        if response_per_update.bits() <= 0 || response_per_update.bits() > Q30::ONE.bits() {
            return Err(DqPlantError::Response);
        }
        if !within_unit_circle(current) {
            return Err(DqPlantError::VectorRange);
        }
        Ok(Self {
            response_per_update,
            current,
            updates: 0,
        })
    }

    /// Current exact dq lattice state.
    pub const fn current(self) -> DqPoint {
        self.current
    }

    /// Number of state transitions completed since construction.
    pub const fn updates(self) -> u64 {
        self.updates
    }

    /// Applies one bounded normalized voltage vector.
    ///
    /// # Errors
    ///
    /// Returns [`DqPlantError::VectorRange`] when `voltage` exceeds the unit
    /// circle and propagates any fixed-point arithmetic rejection.
    pub fn update(&mut self, voltage: DqPoint) -> Result<DqPoint, DqPlantError> {
        if !within_unit_circle(voltage) {
            return Err(DqPlantError::VectorRange);
        }
        let retained = Q30::ONE.checked_sub(self.response_per_update)?;
        self.current = DqPoint {
            d: convex_update(
                self.current.d,
                voltage.d,
                retained,
                self.response_per_update,
            )?,
            q: convex_update(
                self.current.q,
                voltage.q,
                retained,
                self.response_per_update,
            )?,
        };
        self.updates = self
            .updates
            .checked_add(1)
            .ok_or(DqPlantError::TraceLength)?;
        Ok(self.current)
    }
}

impl FirstOrderServoPlant {
    /// Constructs a zero-position, zero-velocity fixture.
    pub const fn new(
        velocity_response_per_update: Q30,
        position_scale_per_update: Q30,
    ) -> Result<Self, ServoPlantError> {
        Self::with_state(
            velocity_response_per_update,
            position_scale_per_update,
            ServoPosition::ZERO,
            Q30::ZERO,
        )
    }

    /// Constructs a fixture from an explicit exact state.
    pub const fn with_state(
        velocity_response_per_update: Q30,
        position_scale_per_update: Q30,
        position: ServoPosition,
        velocity: Q30,
    ) -> Result<Self, ServoPlantError> {
        if velocity_response_per_update.bits() <= 0
            || velocity_response_per_update.bits() > Q30::ONE.bits()
        {
            return Err(ServoPlantError::VelocityResponse);
        }
        if position_scale_per_update.bits() <= 0
            || position_scale_per_update.bits() > Q30::ONE.bits()
        {
            return Err(ServoPlantError::PositionScale);
        }
        Ok(Self {
            velocity_response_per_update,
            position_scale_per_update,
            position,
            velocity,
            updates: 0,
        })
    }

    /// Current exact Q31.32 position state.
    pub const fn position(self) -> ServoPosition {
        self.position
    }

    /// Current exact normalized velocity state.
    pub const fn velocity(self) -> Q30 {
        self.velocity
    }

    /// Number of complete plant updates.
    pub const fn updates(self) -> u64 {
        self.updates
    }

    /// Applies one complete normalized dq-current target transactionally.
    pub fn update(&mut self, current: DqPoint) -> Result<(ServoPosition, Q30), ServoPlantError> {
        if !within_unit_circle(current) {
            return Err(ServoPlantError::CurrentRange);
        }
        let retained = Q30::ONE.checked_sub(self.velocity_response_per_update)?;
        let velocity = self
            .velocity
            .checked_mul(retained)?
            .checked_add(current.q.checked_mul(self.velocity_response_per_update)?)?;
        let position_delta = velocity.checked_mul(self.position_scale_per_update)?;
        let position_delta_bits = i64::from(position_delta.bits())
            .checked_mul(4)
            .ok_or(ServoPlantError::Arithmetic)?;
        let position = self
            .position
            .checked_add_bits(position_delta_bits)
            .map_err(|_| ServoPlantError::Arithmetic)?;
        let updates = self
            .updates
            .checked_add(1)
            .ok_or(ServoPlantError::Arithmetic)?;
        self.position = position;
        self.velocity = velocity;
        self.updates = updates;
        Ok((position, velocity))
    }
}

/// Runs a fixed count of portable dq-current control and plant updates.
///
/// The returned trace contains every pre-state, selected voltage, PI state, and
/// post-state needed for byte-for-byte deterministic replay comparison.
///
/// # Errors
///
/// Returns [`DqPlantError::TraceLength`] if `updates` cannot fit in the plant's
/// `u64` counter and propagates controller or plant rejection.
pub fn simulate_dq_current_loop(
    mut plant: FirstOrderDqPlant,
    mut controller: DqCurrentController,
    target: DqPoint,
    feed_forward: DqPoint,
    updates: usize,
) -> Result<Vec<DqLoopSample>, DqPlantError> {
    let updates_u64 = u64::try_from(updates).map_err(|_| DqPlantError::TraceLength)?;
    plant
        .updates
        .checked_add(updates_u64)
        .ok_or(DqPlantError::TraceLength)?;
    let mut trace = Vec::with_capacity(updates);
    for _ in 0..updates {
        let measured_before = plant.current();
        let control = controller.update(target, measured_before, feed_forward)?;
        let measured_after = plant.update(control.voltage)?;
        trace.push(DqLoopSample {
            index: plant.updates() - 1,
            measured_before,
            control,
            measured_after,
        });
    }
    Ok(trace)
}

/// Replays a fixed target through every exact current-loop boundary.
///
/// Fresh point observations are produced only when the controller's velocity
/// domain is due, and contiguous setpoints are produced only when its position
/// domain is due. The returned trace retains every held current command and
/// exact plant transition.
pub fn simulate_cascaded_servo(
    mut plant: FirstOrderServoPlant,
    mut controller: CascadedServoController,
    target_position: ServoPosition,
    updates: usize,
) -> Result<Vec<ServoLoopSample>, ServoPlantError> {
    let updates_u64 = u64::try_from(updates).map_err(|_| ServoPlantError::Arithmetic)?;
    plant
        .updates
        .checked_add(updates_u64)
        .ok_or(ServoPlantError::Arithmetic)?;
    let grid = controller.grid();
    let config = controller.config();
    let mut setpoint_id = 0_u32;
    let mut sample_sequence = 0_u32;
    let mut trace = Vec::with_capacity(updates);
    for index in 0..updates_u64 {
        let offset = index
            .checked_mul(grid.current_period_cycles())
            .ok_or(ServoPlantError::Arithmetic)?;
        let at = DeviceCycle(
            grid.epoch()
                .0
                .checked_add(offset)
                .ok_or(ServoPlantError::Arithmetic)?,
        );
        let tick = grid.tick(at).map_err(ServoCascadeError::Grid)?;
        let setpoint = if tick.position_due {
            setpoint_id = setpoint_id
                .checked_add(1)
                .ok_or(ServoPlantError::Arithmetic)?;
            Some(ServoSetpoint {
                command_id: setpoint_id,
                scheduled_at: at,
                configuration_digest: config.configuration_digest,
                position: target_position,
                velocity_feed_forward: Q30::ZERO,
                quadrature_current_feed_forward: Q30::ZERO,
            })
        } else {
            None
        };
        let position_before = plant.position();
        let velocity_before = plant.velocity();
        let sample = if tick.velocity_due {
            sample_sequence = sample_sequence
                .checked_add(1)
                .ok_or(ServoPlantError::Arithmetic)?;
            Some(ServoKinematicSample {
                sequence: sample_sequence,
                sampled_at: at,
                available_at: at,
                configuration_digest: config.configuration_digest,
                position: ServoPositionInterval::point(position_before),
                velocity: Q30Interval::point(velocity_before),
            })
        } else {
            None
        };
        let control = controller.service(at, setpoint, sample)?;
        let (position_after, velocity_after) = plant.update(control.current_target)?;
        trace.push(ServoLoopSample {
            index,
            at,
            position_before,
            velocity_before,
            control,
            position_after,
            velocity_after,
        });
    }
    Ok(trace)
}

/// Replays directed truth deltas through an independently wrapped raw sensor.
///
/// The simulator inverts the configured reference and direction itself, emits
/// observations at the exact configured cadence, and requires the estimator to
/// recover the known multi-turn count after every step.
pub fn simulate_servo_encoder(
    mut estimator: ServoEncoderEstimator,
    truth_deltas: &[i32],
    availability_latency_cycles: u64,
) -> Result<Vec<EncoderReplaySample>, EncoderReplayError> {
    let profile = estimator.profile();
    let mut true_unwrapped_count = estimator.seed().unwrapped_count;
    let mut trace = Vec::with_capacity(truth_deltas.len());
    for (index, delta) in truth_deltas.iter().copied().enumerate() {
        true_unwrapped_count = true_unwrapped_count
            .checked_add(i64::from(delta))
            .ok_or(EncoderReplayError::Arithmetic)?;
        let index = u64::try_from(index).map_err(|_| EncoderReplayError::Arithmetic)?;
        let sample_number = index.checked_add(1).ok_or(EncoderReplayError::Arithmetic)?;
        let sampled_at = estimator
            .seed()
            .sampled_at
            .0
            .checked_add(
                sample_number
                    .checked_mul(profile.sample_period_cycles)
                    .ok_or(EncoderReplayError::Arithmetic)?,
            )
            .ok_or(EncoderReplayError::Arithmetic)?;
        let available_at = sampled_at
            .checked_add(availability_latency_cycles)
            .ok_or(EncoderReplayError::Arithmetic)?;
        let raw_count = raw_count_from_directed_truth(
            true_unwrapped_count,
            profile.counts_per_mechanical_turn,
            profile.count_at_reference,
            profile.direction,
        )?;
        let estimate = estimator.observe(alumina_foc::ServoEncoderObservation {
            configuration_digest: profile.configuration_digest,
            raw_count,
            sampled_at: DeviceCycle(sampled_at),
            available_at: DeviceCycle(available_at),
        })?;
        if estimate.unwrapped_count != true_unwrapped_count {
            return Err(EncoderReplayError::TruthMismatch {
                expected: true_unwrapped_count,
                received: estimate.unwrapped_count,
            });
        }
        trace.push(EncoderReplaySample {
            index,
            true_unwrapped_count,
            raw_count,
            estimate,
        });
    }
    Ok(trace)
}

fn raw_count_from_directed_truth(
    unwrapped_count: i64,
    modulus: u32,
    count_at_reference: u32,
    direction: RotorCountDirection,
) -> Result<u32, EncoderReplayError> {
    let modulus_i64 = i64::from(modulus);
    if modulus_i64 < 2 {
        return Err(EncoderReplayError::Arithmetic);
    }
    let directed = unwrapped_count.rem_euclid(modulus_i64);
    let increasing = match direction {
        RotorCountDirection::Increasing => directed,
        RotorCountDirection::Decreasing => (modulus_i64 - directed) % modulus_i64,
    };
    let raw = (i64::from(count_at_reference) + increasing) % modulus_i64;
    u32::try_from(raw).map_err(|_| EncoderReplayError::Arithmetic)
}

fn convex_update(current: Q30, drive: Q30, retained: Q30, response: Q30) -> Result<Q30, FocError> {
    current
        .checked_mul(retained)?
        .checked_add(drive.checked_mul(response)?)
}

const fn within_unit_circle(value: DqPoint) -> bool {
    let direct = value.d.bits() as i128;
    let quadrature = value.q.bits() as i128;
    let limit = Q30::ONE.bits() as i128;
    direct * direct + quadrature * quadrature <= limit * limit
}

#[cfg(test)]
mod tests {
    use alumina_foc::{
        CountUncertainty, FocParameterSnapshot, FocTimingProfile, PiConfig, ServoCascadeConfig,
        ServoEncoderObservation, ServoEncoderProfile, ServoEncoderScale, ServoEncoderSeed,
        ServoLoopGrid,
    };
    use alumina_protocol::Digest;

    use super::*;

    fn point(numerator: i64, denominator: i64) -> Q30 {
        Q30Interval::from_ratio(numerator, denominator)
            .unwrap()
            .midpoint()
    }

    fn controller(output_limit: Q30) -> DqCurrentController {
        let config = PiConfig {
            proportional_gain: Q30::HALF,
            integral_gain_per_update: point(1, 32),
            integral_minimum: output_limit.checked_neg().unwrap(),
            integral_maximum: output_limit,
            output_minimum: output_limit.checked_neg().unwrap(),
            output_maximum: output_limit,
        };
        DqCurrentController::new(config, config).unwrap()
    }

    fn servo_fixture() -> (FirstOrderServoPlant, CascadedServoController) {
        let digest = Digest([0x6b; 32]);
        let timing = FocTimingProfile {
            pwm_hz: 20_000,
            current_loop_hz: 10_000,
            velocity_loop_divider: 10,
            position_loop_divider: 5,
        };
        let current_pi = PiConfig {
            proportional_gain: Q30::HALF,
            integral_gain_per_update: point(1, 32),
            integral_minimum: point(-1, 2),
            integral_maximum: Q30::HALF,
            output_minimum: point(-1, 2),
            output_maximum: Q30::HALF,
        };
        let snapshot = FocParameterSnapshot {
            configuration_digest: digest,
            pole_pairs: 7,
            timing,
            maximum_phase_current: point(3, 4),
            maximum_phase_voltage: point(3, 4),
            d_current: current_pi,
            q_current: current_pi,
        };
        let grid = ServoLoopGrid::new(DeviceCycle(10_000), 1_000_000, timing).unwrap();
        let outer = ServoCascadeConfig {
            configuration_digest: digest,
            position_proportional_gain: point(3, 4),
            velocity_controller: PiConfig {
                proportional_gain: point(3, 4),
                integral_gain_per_update: point(1, 64),
                integral_minimum: point(-1, 2),
                integral_maximum: Q30::HALF,
                output_minimum: point(-1, 2),
                output_maximum: Q30::HALF,
            },
            maximum_velocity: Q30::HALF,
            maximum_current: point(3, 4),
            direct_current_target: Q30::ZERO,
            maximum_following_error_bits: 1_u64 << 32,
            maximum_sample_age_cycles: 100,
        };
        (
            FirstOrderServoPlant::new(point(1, 16), point(1, 2_048)).unwrap(),
            CascadedServoController::new(grid, outer, &snapshot).unwrap(),
        )
    }

    fn encoder_fixture(direction: RotorCountDirection) -> ServoEncoderEstimator {
        let digest = Digest([0x72; 32]);
        let profile = ServoEncoderProfile {
            configuration_digest: digest,
            counts_per_mechanical_turn: 4_096,
            count_at_reference: 0,
            direction,
            maximum_count_error: CountUncertainty::new(1, 2).unwrap(),
            position_at_reference: ServoPosition::ZERO,
            scale: ServoEncoderScale::new(1_u64 << 32, 1, 8_192, 1).unwrap(),
            device_cycle_hz: 1_000_000,
            sample_period_cycles: 1_000,
            maximum_observation_latency_cycles: 100,
            maximum_trackable_velocity: Q30::ONE,
            maximum_admitted_velocity: Q30::ONE,
            maximum_velocity_estimation_error: Q30::from_bits(1 << 20),
            maximum_position_interval_width_ulps: 1_u64 << 20,
            maximum_velocity_interval_width_ulps: 264_241_152,
        };
        let raw_count = match direction {
            RotorCountDirection::Increasing => 4_090,
            RotorCountDirection::Decreasing => 6,
        };
        ServoEncoderEstimator::new(
            profile,
            ServoEncoderSeed {
                observation: ServoEncoderObservation {
                    configuration_digest: digest,
                    raw_count,
                    sampled_at: DeviceCycle(10_000),
                    available_at: DeviceCycle(10_050),
                },
                turn_index: 0,
            },
        )
        .unwrap()
    }

    #[test]
    fn fixed_trace_is_repeatable_and_converges_without_cross_axis_drift() {
        let plant = FirstOrderDqPlant::new(point(1, 8)).unwrap();
        let target = DqPoint {
            d: Q30::ZERO,
            q: Q30::HALF,
        };
        let first = simulate_dq_current_loop(
            plant,
            controller(point(3, 4)),
            target,
            DqPoint::default(),
            800,
        )
        .unwrap();
        let replay = simulate_dq_current_loop(
            plant,
            controller(point(3, 4)),
            target,
            DqPoint::default(),
            800,
        )
        .unwrap();
        assert_eq!(first, replay);
        let final_sample = first.last().unwrap();
        assert_eq!(final_sample.measured_after.d, Q30::ZERO);
        let error_bits =
            (i64::from(final_sample.measured_after.q.bits()) - i64::from(target.q.bits())).abs();
        assert!(
            error_bits <= i64::from(point(1, 10_000).bits()),
            "final q error was {error_bits} lattice units"
        );
    }

    #[test]
    fn output_saturation_is_visible_and_plant_remains_bounded() {
        let trace = simulate_dq_current_loop(
            FirstOrderDqPlant::new(Q30::HALF).unwrap(),
            controller(point(1, 4)),
            DqPoint {
                d: Q30::ZERO,
                q: Q30::ONE,
            },
            DqPoint::default(),
            8,
        )
        .unwrap();
        assert!(
            trace
                .iter()
                .any(|sample| sample.control.quadrature.saturated)
        );
        assert!(
            trace
                .iter()
                .all(|sample| within_unit_circle(sample.measured_after))
        );
        assert!(trace.last().unwrap().measured_after.q < Q30::HALF);
    }

    #[test]
    fn invalid_response_and_drive_are_rejected_before_state_changes() {
        assert_eq!(
            FirstOrderDqPlant::new(Q30::ZERO),
            Err(DqPlantError::Response)
        );
        assert_eq!(
            FirstOrderDqPlant::new(point(3, 2)),
            Err(DqPlantError::Response)
        );
        let mut plant = FirstOrderDqPlant::new(Q30::HALF).unwrap();
        assert_eq!(
            plant.update(DqPoint {
                d: Q30::ONE,
                q: Q30::ONE,
            }),
            Err(DqPlantError::VectorRange)
        );
        assert_eq!(plant.current(), DqPoint::default());
        assert_eq!(plant.updates(), 0);
    }

    #[test]
    fn cascaded_servo_trace_is_repeatable_and_converges_on_exact_grids() {
        let (plant, controller) = servo_fixture();
        let target = ServoPosition::from_bits(1_i64 << 29);
        let first = simulate_cascaded_servo(plant, controller, target, 12_000).unwrap();
        let replay = simulate_cascaded_servo(plant, controller, target, 12_000).unwrap();
        assert_eq!(first, replay);
        assert_eq!(first.len(), 12_000);
        assert_eq!(
            first
                .iter()
                .filter(|sample| sample.control.position_updated)
                .count(),
            240
        );
        assert_eq!(
            first
                .iter()
                .filter(|sample| sample.control.velocity_updated)
                .count(),
            1_200
        );
        let final_sample = first.last().unwrap();
        let position_error =
            i128::from(target.bits()) - i128::from(final_sample.position_after.bits());
        assert!(position_error.unsigned_abs() < 1_u128 << 22);
        assert!(i64::from(final_sample.velocity_after.bits()).unsigned_abs() < 1_u64 << 20);
        assert!(
            first
                .iter()
                .all(|sample| sample.control.current_target.d == Q30::ZERO)
        );
    }

    #[test]
    fn servo_plant_rejects_invalid_coefficients_and_drive_transactionally() {
        assert_eq!(
            FirstOrderServoPlant::new(Q30::ZERO, Q30::HALF),
            Err(ServoPlantError::VelocityResponse)
        );
        assert_eq!(
            FirstOrderServoPlant::new(Q30::HALF, Q30::ZERO),
            Err(ServoPlantError::PositionScale)
        );
        let mut plant = FirstOrderServoPlant::new(Q30::HALF, Q30::HALF).unwrap();
        assert_eq!(
            plant.update(DqPoint {
                d: Q30::ONE,
                q: Q30::ONE,
            }),
            Err(ServoPlantError::CurrentRange)
        );
        assert_eq!(plant.position(), ServoPosition::ZERO);
        assert_eq!(plant.velocity(), Q30::ZERO);
        assert_eq!(plant.updates(), 0);
    }

    #[test]
    fn wrapping_encoder_truth_replays_deterministically_in_both_directions() {
        let mut deltas = vec![7_i32; 700];
        deltas.extend(vec![-7_i32; 900]);
        for direction in [
            RotorCountDirection::Increasing,
            RotorCountDirection::Decreasing,
        ] {
            let first = simulate_servo_encoder(encoder_fixture(direction), &deltas, 50).unwrap();
            let replay = simulate_servo_encoder(encoder_fixture(direction), &deltas, 50).unwrap();
            assert_eq!(first, replay);
            assert_eq!(first.len(), 1_600);
            assert_eq!(first.last().unwrap().true_unwrapped_count, 2_690);
            assert!(first.iter().all(|sample| {
                sample.estimate.unwrapped_count == sample.true_unwrapped_count
                    && sample.estimate.sample.sequence == (sample.index + 1) as u32
            }));
            assert!(first.windows(2).any(|pair| {
                let left = pair[0].raw_count;
                let right = pair[1].raw_count;
                left.abs_diff(right) > 4_000
            }));
        }
    }

    #[test]
    fn encoder_truth_outside_the_proven_wrap_window_is_visible() {
        assert!(matches!(
            simulate_servo_encoder(encoder_fixture(RotorCountDirection::Increasing), &[11], 50,),
            Err(EncoderReplayError::Estimator(
                ServoEncoderError::CountDelta { .. }
            ))
        ));
    }
}
