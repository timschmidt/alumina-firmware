//! Deterministic dimensionless plant models for portable FOC control tests.
//!
//! These models establish arithmetic, state, saturation, and replay behavior.
//! They are intentionally not motor identification, electrical simulation, or
//! evidence that a physical power stage is safe to energize.

use alumina_foc::{DqControlUpdate, DqCurrentController, DqPoint, FocError, Q30};

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
    use alumina_foc::{PiConfig, Q30Interval};

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
}
