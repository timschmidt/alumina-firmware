#![no_std]
#![doc = "Allocation-free fixed-point FOC mathematics and control contracts for Alumina."]

use alumina_protocol::{DeviceCycle, Digest};

mod angle;
mod current;
mod pwm;
mod servo;

pub use angle::{
    CountUncertainty, ElectricalPhase, ElectricalPhaseEstimate, HALF_TURN_BITS,
    MAXIMUM_OBSERVATION_ERROR_BITS, PHASE_POINTS_PER_TURN, QUARTER_TURN_BITS, RotationPrecision,
    RotorCalibration, RotorCountDirection, rotation_from_estimate, rotation_from_phase,
};
pub use current::{
    CurrentChannelCalibration, CurrentPolarity, CurrentSample, PwmAdcSampleStamp,
    PwmAdcSynchronization, SequentialAdcAcquisition, SequentialAdcAcquisitionError,
    SequentialAdcChannel, SequentialAdcPair, SequentialAdcRequest, TwoShuntCurrentCalibration,
    TwoShuntPhasePair, ValidatedTwoShuntCurrentCalibration,
};
pub use pwm::{
    PwmCompareContract, PwmCompareError, PwmCompareImage, PwmCompareLatch, PwmCompareLatchError,
    PwmCompareLatchOwner, PwmCompareValue,
};
pub use servo::{
    CascadedServoController, SERVO_POSITION_FRACTION_BITS, SERVO_POSITION_SCALE,
    ServoCascadeConfig, ServoCascadeError, ServoCascadeProfileError, ServoCascadeUpdate,
    ServoKinematicSample, ServoLoopGrid, ServoLoopGridError, ServoLoopTick, ServoPosition,
    ServoPositionErrorInterval, ServoPositionInterval, ServoPositionIntervalError,
    ServoQ30ErrorInterval, ServoSetpoint,
};

/// Fractional bits in the signed Q2.30 real-time representation.
pub const Q30_FRACTION_BITS: u32 = 30;
/// Exact denominator of every [`Q30`] value.
pub const Q30_SCALE: i64 = 1_i64 << Q30_FRACTION_BITS;

/// Exact signed rational whose value is `bits / 2^30`.
///
/// The type performs no implicit saturation. Checked operations reject values
/// outside the representable `[-2, 2)` interval so a control-law overflow
/// cannot silently become a hardware command.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Q30(i32);

impl Q30 {
    /// Exact zero.
    pub const ZERO: Self = Self(0);
    /// Exact positive one.
    pub const ONE: Self = Self(1_i32 << Q30_FRACTION_BITS);
    /// Exact negative one.
    pub const NEG_ONE: Self = Self(-(1_i32 << Q30_FRACTION_BITS));
    /// Exact one half.
    pub const HALF: Self = Self(1_i32 << (Q30_FRACTION_BITS - 1));

    /// Constructs the exact represented rational from its canonical bits.
    pub const fn from_bits(bits: i32) -> Self {
        Self(bits)
    }

    /// Returns the canonical signed numerator over [`Q30_SCALE`].
    pub const fn bits(self) -> i32 {
        self.0
    }

    /// Checked exact addition.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Range`] when the exact sum is not representable.
    pub fn checked_add(self, other: Self) -> Result<Self, FocError> {
        self.0.checked_add(other.0).map(Self).ok_or(FocError::Range)
    }

    /// Checked exact subtraction.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Range`] when the exact difference is not representable.
    pub fn checked_sub(self, other: Self) -> Result<Self, FocError> {
        self.0.checked_sub(other.0).map(Self).ok_or(FocError::Range)
    }

    /// Checked exact negation.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Range`] for the sole asymmetric minimum value.
    pub fn checked_neg(self) -> Result<Self, FocError> {
        self.0.checked_neg().map(Self).ok_or(FocError::Range)
    }

    /// Deterministic multiplication rounded to nearest, ties to even.
    ///
    /// Interval paths use outward rounding instead. This operation is intended
    /// for the selected nominal value in a fixed-rate controller.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Range`] when the rounded product is not representable.
    pub fn checked_mul(self, other: Self) -> Result<Self, FocError> {
        let product = i128::from(self.0) * i128::from(other.0);
        q30_from_i128(round_ties_even(product, i128::from(Q30_SCALE)))
    }

    /// Deterministic division rounded to nearest, ties to even.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::DivideByZero`] for zero divisor and
    /// [`FocError::Range`] when the rounded quotient is not representable.
    pub fn checked_div(self, other: Self) -> Result<Self, FocError> {
        if other.0 == 0 {
            return Err(FocError::DivideByZero);
        }
        let numerator = i128::from(self.0) * i128::from(Q30_SCALE);
        q30_from_i128(round_ties_even(numerator, i128::from(other.0)))
    }

    /// Inclusive clamp in the same exact rational domain.
    pub const fn clamp(self, lower: Self, upper: Self) -> Self {
        if self.0 < lower.0 {
            lower
        } else if self.0 > upper.0 {
            upper
        } else {
            self
        }
    }

    /// Absolute magnitude as a widened integer numerator.
    pub const fn unsigned_abs_bits(self) -> u64 {
        self.0.unsigned_abs() as u64
    }
}

/// Conservative closed interval over exact Q2.30 rationals.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Q30Interval {
    lower: Q30,
    upper: Q30,
}

impl Q30Interval {
    /// Exact zero interval.
    pub const ZERO: Self = Self::point(Q30::ZERO);
    /// Exact unit interval containing only one.
    pub const ONE: Self = Self::point(Q30::ONE);
    /// Exact interval containing only one half.
    pub const HALF: Self = Self::point(Q30::HALF);

    const fn from_raw_bits(lower: i32, upper: i32) -> Self {
        Self {
            lower: Q30::from_bits(lower),
            upper: Q30::from_bits(upper),
        }
    }

    /// Constructs a point interval.
    pub const fn point(value: Q30) -> Self {
        Self {
            lower: value,
            upper: value,
        }
    }

    /// Constructs an ordered closed interval.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::IntervalOrder`] when `upper < lower`.
    pub const fn new(lower: Q30, upper: Q30) -> Result<Self, FocError> {
        if upper.bits() < lower.bits() {
            Err(FocError::IntervalOrder)
        } else {
            Ok(Self { lower, upper })
        }
    }

    /// Smallest Q2.30 interval containing the exact rational `numerator / denominator`.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::DivideByZero`] for zero denominator and
    /// [`FocError::Range`] when either outward-rounded endpoint is unrepresentable.
    pub fn from_ratio(numerator: i64, denominator: i64) -> Result<Self, FocError> {
        if denominator == 0 {
            return Err(FocError::DivideByZero);
        }
        let (numerator, denominator) = if denominator < 0 {
            (-i128::from(numerator), -i128::from(denominator))
        } else {
            (i128::from(numerator), i128::from(denominator))
        };
        let scaled = numerator
            .checked_mul(i128::from(Q30_SCALE))
            .ok_or(FocError::Range)?;
        Self::new(
            q30_from_i128(div_floor(scaled, denominator))?,
            q30_from_i128(div_ceil(scaled, denominator))?,
        )
    }

    /// Inclusive lower endpoint.
    pub const fn lower(self) -> Q30 {
        self.lower
    }

    /// Inclusive upper endpoint.
    pub const fn upper(self) -> Q30 {
        self.upper
    }

    /// Integer midpoint, rounded toward the lower endpoint.
    pub fn midpoint(self) -> Q30 {
        let width = i64::from(self.upper.0) - i64::from(self.lower.0);
        Q30::from_bits((i64::from(self.lower.0) + width / 2) as i32)
    }

    /// Width in exact Q2.30 lattice units.
    pub const fn width_ulps(self) -> u64 {
        (self.upper.0 as i64 - self.lower.0 as i64) as u64
    }

    /// Whether this interval includes one exact Q2.30 point.
    pub const fn contains(self, value: Q30) -> bool {
        self.lower.0 <= value.0 && value.0 <= self.upper.0
    }

    /// Exact interval addition.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Range`] when an endpoint is not representable.
    pub fn checked_add(self, other: Self) -> Result<Self, FocError> {
        Self::new(
            q30_from_i64(i64::from(self.lower.0) + i64::from(other.lower.0))?,
            q30_from_i64(i64::from(self.upper.0) + i64::from(other.upper.0))?,
        )
    }

    /// Exact interval subtraction.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Range`] when an endpoint is not representable.
    pub fn checked_sub(self, other: Self) -> Result<Self, FocError> {
        Self::new(
            q30_from_i64(i64::from(self.lower.0) - i64::from(other.upper.0))?,
            q30_from_i64(i64::from(self.upper.0) - i64::from(other.lower.0))?,
        )
    }

    /// Exact interval negation.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Range`] for an unrepresentable endpoint.
    pub fn checked_neg(self) -> Result<Self, FocError> {
        Self::new(self.upper.checked_neg()?, self.lower.checked_neg()?)
    }

    /// Outward-rounded interval multiplication.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Range`] when an endpoint is not representable.
    pub fn checked_mul(self, other: Self) -> Result<Self, FocError> {
        let products = [
            i128::from(self.lower.0) * i128::from(other.lower.0),
            i128::from(self.lower.0) * i128::from(other.upper.0),
            i128::from(self.upper.0) * i128::from(other.lower.0),
            i128::from(self.upper.0) * i128::from(other.upper.0),
        ];
        let minimum = products.into_iter().min().ok_or(FocError::Range)?;
        let maximum = products.into_iter().max().ok_or(FocError::Range)?;
        Self::new(
            q30_from_i128(div_floor(minimum, i128::from(Q30_SCALE)))?,
            q30_from_i128(div_ceil(maximum, i128::from(Q30_SCALE)))?,
        )
    }

    /// Outward-rounded interval division.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::DivideByZero`] when the denominator includes zero and
    /// [`FocError::Range`] when an endpoint is not representable.
    pub fn checked_div(self, other: Self) -> Result<Self, FocError> {
        if other.contains(Q30::ZERO) {
            return Err(FocError::DivideByZero);
        }
        let ratios = [
            ratio_bounds(self.lower.0, other.lower.0),
            ratio_bounds(self.lower.0, other.upper.0),
            ratio_bounds(self.upper.0, other.lower.0),
            ratio_bounds(self.upper.0, other.upper.0),
        ];
        let minimum = ratios
            .iter()
            .map(|(lower, _)| *lower)
            .min()
            .ok_or(FocError::Range)?;
        let maximum = ratios
            .iter()
            .map(|(_, upper)| *upper)
            .max()
            .ok_or(FocError::Range)?;
        Self::new(q30_from_i128(minimum)?, q30_from_i128(maximum)?)
    }

    /// Tight outward-rounded square interval.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Range`] when an endpoint is not representable.
    pub fn checked_square(self) -> Result<Self, FocError> {
        let lower_squared = i128::from(self.lower.0) * i128::from(self.lower.0);
        let upper_squared = i128::from(self.upper.0) * i128::from(self.upper.0);
        let minimum = if self.contains(Q30::ZERO) {
            0
        } else {
            lower_squared.min(upper_squared)
        };
        let maximum = lower_squared.max(upper_squared);
        Self::new(
            q30_from_i128(div_floor(minimum, i128::from(Q30_SCALE)))?,
            q30_from_i128(div_ceil(maximum, i128::from(Q30_SCALE)))?,
        )
    }
}

/// Adjacent Q2.30 values enclosing `2/3`.
pub const TWO_THIRDS: Q30Interval = Q30Interval {
    lower: Q30::from_bits(715_827_882),
    upper: Q30::from_bits(715_827_883),
};
/// Adjacent Q2.30 values enclosing `1/sqrt(3)`.
pub const INV_SQRT_3: Q30Interval = Q30Interval {
    lower: Q30::from_bits(619_925_131),
    upper: Q30::from_bits(619_925_132),
};
/// Adjacent Q2.30 values enclosing `sqrt(3)/2`.
pub const SQRT_3_OVER_2: Q30Interval = Q30Interval {
    lower: Q30::from_bits(929_887_696),
    upper: Q30::from_bits(929_887_697),
};

/// Three phase-domain closed intervals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Phase3 {
    pub a: Q30Interval,
    pub b: Q30Interval,
    pub c: Q30Interval,
}

/// Stationary orthogonal alpha/beta intervals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AlphaBeta {
    pub alpha: Q30Interval,
    pub beta: Q30Interval,
}

/// Rotating orthogonal direct/quadrature intervals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DqInterval {
    pub d: Q30Interval,
    pub q: Q30Interval,
}

/// Nominal direct/quadrature point used by the deterministic controller.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DqPoint {
    pub d: Q30,
    pub q: Q30,
}

/// Conservative sine/cosine evidence for one electrical angle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rotation {
    sine: Q30Interval,
    cosine: Q30Interval,
    norm: Q30Interval,
}

impl Rotation {
    /// Validates a bounded unit vector against an explicit maximum norm error.
    ///
    /// `maximum_norm_error` is a nonnegative Q2.30 distance from one. The
    /// computed squared-norm interval must contain exact one and remain within
    /// that distance on both sides.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Rotation`] for an invalid component or norm
    /// certificate, or an arithmetic error from interval evaluation.
    pub fn new(
        sine: Q30Interval,
        cosine: Q30Interval,
        maximum_norm_error: Q30,
    ) -> Result<Self, FocError> {
        if sine.lower < Q30::NEG_ONE
            || sine.upper > Q30::ONE
            || cosine.lower < Q30::NEG_ONE
            || cosine.upper > Q30::ONE
            || maximum_norm_error < Q30::ZERO
        {
            return Err(FocError::Rotation);
        }
        let norm = sine
            .checked_square()?
            .checked_add(cosine.checked_square()?)?;
        if !norm.contains(Q30::ONE)
            || Q30::ONE.bits() as i64 - norm.lower.bits() as i64
                > i64::from(maximum_norm_error.bits())
            || norm.upper.bits() as i64 - Q30::ONE.bits() as i64
                > i64::from(maximum_norm_error.bits())
        {
            return Err(FocError::Rotation);
        }
        Ok(Self { sine, cosine, norm })
    }

    /// Exact zero-angle rotation.
    pub const fn zero() -> Self {
        Self {
            sine: Q30Interval::ZERO,
            cosine: Q30Interval::ONE,
            norm: Q30Interval::ONE,
        }
    }

    /// Conservative sine interval.
    pub const fn sine(self) -> Q30Interval {
        self.sine
    }

    /// Conservative cosine interval.
    pub const fn cosine(self) -> Q30Interval {
        self.cosine
    }

    /// Evaluated squared-norm interval.
    pub const fn norm(self) -> Q30Interval {
        self.norm
    }
}

/// Clarke transform with an explicit admissible zero-sequence residual.
///
/// # Errors
///
/// Returns [`FocError::UnbalancedPhases`] if `a + b + c` exceeds the supplied
/// nonnegative bound, or an arithmetic error when the result is unrepresentable.
pub fn clarke(phases: Phase3, maximum_zero_sequence: Q30) -> Result<AlphaBeta, FocError> {
    if maximum_zero_sequence < Q30::ZERO {
        return Err(FocError::UnbalancedPhases);
    }
    let residual_lower = i64::from(phases.a.lower.bits())
        + i64::from(phases.b.lower.bits())
        + i64::from(phases.c.lower.bits());
    let residual_upper = i64::from(phases.a.upper.bits())
        + i64::from(phases.b.upper.bits())
        + i64::from(phases.c.upper.bits());
    if residual_lower
        .unsigned_abs()
        .max(residual_upper.unsigned_abs())
        > maximum_zero_sequence.unsigned_abs_bits()
    {
        return Err(FocError::UnbalancedPhases);
    }
    let alpha_lower_numerator = 2_i64 * i64::from(phases.a.lower.bits())
        - i64::from(phases.b.upper.bits())
        - i64::from(phases.c.upper.bits());
    let alpha_upper_numerator = 2_i64 * i64::from(phases.a.upper.bits())
        - i64::from(phases.b.lower.bits())
        - i64::from(phases.c.lower.bits());
    let beta_difference_lower = i64::from(phases.b.lower.bits()) - i64::from(phases.c.upper.bits());
    let beta_difference_upper = i64::from(phases.b.upper.bits()) - i64::from(phases.c.lower.bits());
    Ok(AlphaBeta {
        alpha: Q30Interval::new(
            q30_from_i64(div_floor_i64(alpha_lower_numerator, 3))?,
            q30_from_i64(div_ceil_i64(alpha_upper_numerator, 3))?,
        )?,
        beta: scale_wide_interval(beta_difference_lower, beta_difference_upper, INV_SQRT_3)?,
    })
}

/// Inverse Clarke transform into three balanced phase intervals.
///
/// # Errors
///
/// Returns an arithmetic error when an outward-rounded endpoint is unrepresentable.
pub fn inverse_clarke(value: AlphaBeta) -> Result<Phase3, FocError> {
    let negative_half_alpha = Q30Interval::HALF.checked_mul(value.alpha)?.checked_neg()?;
    let beta_term = SQRT_3_OVER_2.checked_mul(value.beta)?;
    Ok(Phase3 {
        a: value.alpha,
        b: negative_half_alpha.checked_add(beta_term)?,
        c: negative_half_alpha.checked_sub(beta_term)?,
    })
}

/// Park transform from the stationary to rotating frame.
///
/// # Errors
///
/// Returns an arithmetic error when an outward-rounded endpoint is unrepresentable.
pub fn park(value: AlphaBeta, rotation: Rotation) -> Result<DqInterval, FocError> {
    Ok(DqInterval {
        d: value
            .alpha
            .checked_mul(rotation.cosine)?
            .checked_add(value.beta.checked_mul(rotation.sine)?)?,
        q: value
            .beta
            .checked_mul(rotation.cosine)?
            .checked_sub(value.alpha.checked_mul(rotation.sine)?)?,
    })
}

/// Inverse Park transform from rotating to stationary frame.
///
/// # Errors
///
/// Returns an arithmetic error when an outward-rounded endpoint is unrepresentable.
pub fn inverse_park(value: DqInterval, rotation: Rotation) -> Result<AlphaBeta, FocError> {
    Ok(AlphaBeta {
        alpha: value
            .d
            .checked_mul(rotation.cosine)?
            .checked_sub(value.q.checked_mul(rotation.sine)?)?,
        beta: value
            .d
            .checked_mul(rotation.sine)?
            .checked_add(value.q.checked_mul(rotation.cosine)?)?,
    })
}

/// Three conservative normalized PWM duty intervals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Duty3 {
    pub a: Q30Interval,
    pub b: Q30Interval,
    pub c: Q30Interval,
}

/// Complete min/max common-mode space-vector modulation result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModulationResult {
    /// Conservative normalized `[0, 1]` duty intervals.
    pub duties: Duty3,
    /// Injected phase common-mode interval.
    pub common_mode: Q30Interval,
    /// Centered normalized phase voltages before duty conversion.
    pub centered_phases: Phase3,
}

/// Applies min/max common-mode injection and rejects overmodulation.
///
/// `phase_limit` is the exact positive normalized half-bus limit. The function
/// never clips an overrange phase; callers must explicitly choose a separate
/// saturation policy before hardware commitment.
///
/// # Errors
///
/// Returns [`FocError::ModulationRange`] for a nonpositive limit, overrange
/// phase, or duty outside `[0, 1]`, and arithmetic errors for unrepresentable
/// interval operations.
pub fn space_vector_modulate(
    value: AlphaBeta,
    phase_limit: Q30,
) -> Result<ModulationResult, FocError> {
    if phase_limit <= Q30::ZERO {
        return Err(FocError::ModulationRange);
    }
    let phases = inverse_clarke(value)?;
    let maximum = interval_max(phases.a, interval_max(phases.b, phases.c));
    let minimum = interval_min(phases.a, interval_min(phases.b, phases.c));
    let common_mode = Q30Interval::HALF
        .checked_mul(maximum.checked_add(minimum)?)?
        .checked_neg()?;
    let centered_phases = Phase3 {
        a: phases.a.checked_add(common_mode)?,
        b: phases.b.checked_add(common_mode)?,
        c: phases.c.checked_add(common_mode)?,
    };
    let negative_limit = phase_limit.checked_neg()?;
    if [centered_phases.a, centered_phases.b, centered_phases.c]
        .iter()
        .any(|phase| phase.lower < negative_limit || phase.upper > phase_limit)
    {
        return Err(FocError::ModulationRange);
    }
    let duties = Duty3 {
        a: phase_to_duty(centered_phases.a, phase_limit)?,
        b: phase_to_duty(centered_phases.b, phase_limit)?,
        c: phase_to_duty(centered_phases.c, phase_limit)?,
    };
    if [duties.a, duties.b, duties.c]
        .iter()
        .any(|duty| duty.lower < Q30::ZERO || duty.upper > Q30::ONE)
    {
        return Err(FocError::ModulationRange);
    }
    Ok(ModulationResult {
        duties,
        common_mode,
        centered_phases,
    })
}

/// Fixed-rate PI controller parameters in normalized Q2.30 units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PiConfig {
    pub proportional_gain: Q30,
    /// Integral gain already multiplied by the exact fixed update period.
    pub integral_gain_per_update: Q30,
    pub integral_minimum: Q30,
    pub integral_maximum: Q30,
    pub output_minimum: Q30,
    pub output_maximum: Q30,
}

impl PiConfig {
    /// Checks gain signs and ordered finite bounds.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Controller`] for a negative gain or invalid bound.
    pub const fn validate(self) -> Result<(), FocError> {
        if self.proportional_gain.bits() < 0
            || self.integral_gain_per_update.bits() < 0
            || self.integral_minimum.bits() > self.integral_maximum.bits()
            || self.output_minimum.bits() > self.output_maximum.bits()
        {
            Err(FocError::Controller)
        } else {
            Ok(())
        }
    }
}

/// Stateful deterministic PI controller with conditional-integration anti-windup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PiController {
    config: PiConfig,
    integral: Q30,
}

/// One controller update and retained state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PiUpdate {
    pub output: Q30,
    pub integral: Q30,
    pub saturated: bool,
}

/// Two-axis current-loop output and the independently retained PI states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DqControlUpdate {
    /// Selected normalized voltage request in the rotating frame.
    pub voltage: DqPoint,
    /// Direct-axis controller result.
    pub direct: PiUpdate,
    /// Quadrature-axis controller result.
    pub quadrature: PiUpdate,
}

/// Coupled ownership wrapper for the independent direct/quadrature PI loops.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DqCurrentController {
    direct: PiController,
    quadrature: PiController,
}

impl PiController {
    /// Constructs a zero-integral controller from a complete validated snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Controller`] for an invalid configuration.
    pub const fn new(config: PiConfig) -> Result<Self, FocError> {
        match config.validate() {
            Ok(()) => Ok(Self {
                config,
                integral: Q30::ZERO,
            }),
            Err(error) => Err(error),
        }
    }

    /// Current retained integral contribution.
    pub const fn integral(self) -> Q30 {
        self.integral
    }

    /// Clears retained integral state without changing parameters.
    pub fn reset(&mut self) {
        self.integral = Q30::ZERO;
    }

    /// Executes one fixed-period update.
    ///
    /// Integration is withheld only when the candidate output is saturated and
    /// the error would drive it farther into the same limit. Feed-forward is an
    /// explicit caller value rather than hidden plant knowledge.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Range`] if a gain product is unrepresentable.
    pub fn update(&mut self, error: Q30, feed_forward: Q30) -> Result<PiUpdate, FocError> {
        self.update_error_bits(i64::from(error.bits()), feed_forward)
    }

    fn update_error_bits(
        &mut self,
        error_bits: i64,
        feed_forward: Q30,
    ) -> Result<PiUpdate, FocError> {
        let proportional_bits = multiply_q30_by_wide(self.config.proportional_gain, error_bits)?;
        let increment_bits =
            multiply_q30_by_wide(self.config.integral_gain_per_update, error_bits)?;
        let candidate_integral = Q30::from_bits(clamp_i64_to_i32_bounds(
            i64::from(self.integral.bits())
                .checked_add(increment_bits)
                .ok_or(FocError::Range)?,
            self.config.integral_minimum,
            self.config.integral_maximum,
        ));
        let candidate_sum = proportional_bits
            + i64::from(candidate_integral.bits())
            + i64::from(feed_forward.bits());
        let drives_high =
            candidate_sum > i64::from(self.config.output_maximum.bits()) && error_bits > 0;
        let drives_low =
            candidate_sum < i64::from(self.config.output_minimum.bits()) && error_bits < 0;
        if !drives_high && !drives_low {
            self.integral = candidate_integral;
        }
        let output_sum =
            proportional_bits + i64::from(self.integral.bits()) + i64::from(feed_forward.bits());
        let output_bits = clamp_i64_to_i32_bounds(
            output_sum,
            self.config.output_minimum,
            self.config.output_maximum,
        );
        Ok(PiUpdate {
            output: Q30::from_bits(output_bits),
            integral: self.integral,
            saturated: i64::from(output_bits) != output_sum,
        })
    }
}

impl DqCurrentController {
    /// Constructs both loops from individually validated controller parameters.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Controller`] when either controller configuration is
    /// invalid.
    pub const fn new(direct: PiConfig, quadrature: PiConfig) -> Result<Self, FocError> {
        let direct = match PiController::new(direct) {
            Ok(controller) => controller,
            Err(error) => return Err(error),
        };
        let quadrature = match PiController::new(quadrature) {
            Ok(controller) => controller,
            Err(error) => return Err(error),
        };
        Ok(Self { direct, quadrature })
    }

    /// Constructs both loops from a complete validated real-time snapshot.
    ///
    /// This is the production construction path: snapshot validation also
    /// proves the complete rectangular PI output domain fits inside the
    /// declared circular normalized voltage limit.
    ///
    /// # Errors
    ///
    /// Propagates any snapshot or controller validation error.
    pub fn from_snapshot(snapshot: &FocParameterSnapshot) -> Result<Self, FocError> {
        snapshot.validate()?;
        Self::new(snapshot.d_current, snapshot.q_current)
    }

    /// Clears both retained integral states at the same control boundary.
    pub fn reset(&mut self) {
        self.direct.reset();
        self.quadrature.reset();
    }

    /// Executes both current loops from exact Q2.30 lattice points.
    ///
    /// Target-minus-measurement is evaluated in a widened integer domain. This
    /// preserves the valid `+2` endpoint produced by `+1 - (-1)`, which Q2.30
    /// itself cannot encode, until the controller output is explicitly clamped.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Range`] for an unrepresentable intermediate product.
    pub fn update(
        &mut self,
        target: DqPoint,
        measured: DqPoint,
        feed_forward: DqPoint,
    ) -> Result<DqControlUpdate, FocError> {
        let mut next_direct = self.direct;
        let mut next_quadrature = self.quadrature;
        let direct = next_direct.update_error_bits(
            i64::from(target.d.bits()) - i64::from(measured.d.bits()),
            feed_forward.d,
        )?;
        let quadrature = next_quadrature.update_error_bits(
            i64::from(target.q.bits()) - i64::from(measured.q.bits()),
            feed_forward.q,
        )?;
        self.direct = next_direct;
        self.quadrature = next_quadrature;
        Ok(DqControlUpdate {
            voltage: DqPoint {
                d: direct.output,
                q: quadrature.output,
            },
            direct,
            quadrature,
        })
    }
}

/// Fixed timing relationship for one FOC axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocTimingProfile {
    pub pwm_hz: u32,
    pub current_loop_hz: u32,
    pub velocity_loop_divider: u16,
    pub position_loop_divider: u16,
}

impl FocTimingProfile {
    /// Requires integer nested clock domains rooted at PWM.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Profile`] for zero or nonintegral rates/dividers.
    pub const fn validate(self) -> Result<(), FocError> {
        if self.pwm_hz == 0
            || self.current_loop_hz == 0
            || !self.pwm_hz.is_multiple_of(self.current_loop_hz)
            || self.velocity_loop_divider == 0
            || self.position_loop_divider == 0
        {
            Err(FocError::Profile)
        } else {
            Ok(())
        }
    }
}

/// Complete immutable normalized inner-loop parameter snapshot.
///
/// Physical units and their exact scales remain in the machine configuration;
/// normalized values here are the bounded core-1 representation selected from
/// that authoritative configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocParameterSnapshot {
    pub configuration_digest: Digest,
    pub pole_pairs: u16,
    pub timing: FocTimingProfile,
    pub maximum_phase_current: Q30,
    pub maximum_phase_voltage: Q30,
    pub d_current: PiConfig,
    pub q_current: PiConfig,
}

impl FocParameterSnapshot {
    /// Validates the complete snapshot before an atomic real-time swap.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Profile`] for missing identity, zero motor facts, or
    /// nonpositive/out-of-unit normalized limits, a controller output rectangle
    /// outside the circular voltage limit, or a propagated timing/PI error.
    pub fn validate(self) -> Result<(), FocError> {
        if self.configuration_digest.is_zero()
            || self.pole_pairs == 0
            || self.maximum_phase_current.bits() <= 0
            || self.maximum_phase_current.bits() > Q30::ONE.bits()
            || self.maximum_phase_voltage.bits() <= 0
            || self.maximum_phase_voltage.bits() > Q30::ONE.bits()
        {
            return Err(FocError::Profile);
        }
        match self.timing.validate() {
            Ok(()) => {}
            Err(error) => return Err(error),
        }
        match self.d_current.validate() {
            Ok(()) => {}
            Err(error) => return Err(error),
        }
        self.q_current.validate()?;
        if !controller_rectangle_within_limit(
            self.d_current,
            self.q_current,
            self.maximum_phase_voltage,
        ) {
            return Err(FocError::Profile);
        }
        Ok(())
    }
}

/// Fixed-size scheduled dq-current command consumed by the real-time queue.
///
/// The browser compiler derives these normalized lattice points from exact
/// machine configuration. The firmware accepts no raw geometry and binds every
/// command to the complete parameter snapshot that gave the points meaning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocCurrentCommand {
    /// Monotonic identifier within the enclosing immutable command stream.
    pub command_id: u32,
    /// Device cycle at which this setpoint becomes active.
    pub scheduled_at: DeviceCycle,
    /// Identity of the exact configuration and reduced parameter snapshot.
    pub configuration_digest: Digest,
    /// Requested normalized direct/quadrature phase-current vector.
    pub target: DqPoint,
    /// Optional normalized direct/quadrature voltage feed-forward vector.
    pub voltage_feed_forward: DqPoint,
}

impl FocCurrentCommand {
    /// Checks identity and exact circular current/voltage bounds.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Configuration`] when the command and snapshot do not
    /// share an established digest, [`FocError::CommandRange`] when either
    /// vector exceeds its normalized circular limit, or a snapshot validation
    /// error.
    pub fn validate(self, snapshot: &FocParameterSnapshot) -> Result<(), FocError> {
        snapshot.validate()?;
        if self.configuration_digest.is_zero()
            || self.configuration_digest != snapshot.configuration_digest
        {
            return Err(FocError::Configuration);
        }
        if !vector_within_limit(self.target, snapshot.maximum_phase_current)
            || !vector_within_limit(self.voltage_feed_forward, snapshot.maximum_phase_voltage)
        {
            return Err(FocError::CommandRange);
        }
        Ok(())
    }
}

/// Boot-local bounded rotor observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RotorSample {
    pub configuration_digest: Digest,
    pub pole_pairs: u16,
    pub observed_at: DeviceCycle,
    pub electrical_phase: ElectricalPhase,
    /// Symmetric circular error bound in binary phase lattice points.
    pub maximum_phase_error_bits: u32,
    pub rotation: Rotation,
}

impl RotorSample {
    /// Replays phase generation and binds the sample to one parameter snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Configuration`] for a foreign configuration,
    /// [`FocError::RotorSample`] if the retained rotation is not the canonical
    /// result for its phase/error pair, or a snapshot/phase/precision error.
    pub fn validate_for(
        self,
        snapshot: &FocParameterSnapshot,
        precision: RotationPrecision,
    ) -> Result<(), FocError> {
        snapshot.validate()?;
        if self.configuration_digest.is_zero()
            || self.configuration_digest != snapshot.configuration_digest
            || self.pole_pairs == 0
            || self.pole_pairs != snapshot.pole_pairs
        {
            return Err(FocError::Configuration);
        }
        let estimate =
            ElectricalPhaseEstimate::new(self.electrical_phase, self.maximum_phase_error_bits)?;
        if rotation_from_estimate(estimate, precision)? != self.rotation {
            return Err(FocError::RotorSample);
        }
        Ok(())
    }
}

/// Backend observation of one committed three-phase duty image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PowerStageCommit {
    pub token: u32,
    pub scheduled_at: DeviceCycle,
    pub observed_at: DeviceCycle,
}

/// Allocation-free rotor feedback boundary.
pub trait RotorSensor {
    type Error;

    /// Samples rotor position without performing motion control.
    fn sample(&mut self, at: DeviceCycle) -> Result<RotorSample, Self::Error>;
}

/// Allocation-free phase-current acquisition boundary.
pub trait CurrentSense {
    type Error;

    /// Takes one completed, PWM-correlated sample when the backend has one.
    ///
    /// A backend configured from a validated immutable calibration returns
    /// `None` until a complete ADC conversion and synchronization stamp exist.
    fn take_synchronized_sample(&mut self) -> Result<Option<CurrentSample>, Self::Error>;
}

/// Sole owner of qualified inverter shutdown and PWM commitment.
pub trait PowerStage {
    type Error;

    /// Atomically commits one complete future duty image.
    fn commit(
        &mut self,
        token: u32,
        scheduled_at: DeviceCycle,
        duties: Duty3,
    ) -> Result<PowerStageCommit, Self::Error>;

    /// Applies the independently qualified disabled/off transaction.
    fn disable(&mut self) -> Result<(), Self::Error>;
}

/// FOC arithmetic, certificate, controller, or parameter rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocError {
    Range,
    IntervalOrder,
    DivideByZero,
    Rotation,
    Phase,
    Precision,
    RotorCalibration,
    RotorSample,
    CurrentCalibration,
    CurrentRawSample,
    CurrentSynchronization,
    CurrentSample,
    UnbalancedPhases,
    ModulationRange,
    Controller,
    Profile,
    Configuration,
    CommandRange,
}

fn interval_max(left: Q30Interval, right: Q30Interval) -> Q30Interval {
    Q30Interval {
        lower: left.lower.max(right.lower),
        upper: left.upper.max(right.upper),
    }
}

fn interval_min(left: Q30Interval, right: Q30Interval) -> Q30Interval {
    Q30Interval {
        lower: left.lower.min(right.lower),
        upper: left.upper.min(right.upper),
    }
}

fn vector_within_limit(value: DqPoint, limit: Q30) -> bool {
    let direct = i128::from(value.d.bits());
    let quadrature = i128::from(value.q.bits());
    let limit = i128::from(limit.bits());
    direct * direct + quadrature * quadrature <= limit * limit
}

fn controller_rectangle_within_limit(direct: PiConfig, quadrature: PiConfig, limit: Q30) -> bool {
    let direct_maximum =
        maximum_absolute_i32(direct.output_minimum.bits(), direct.output_maximum.bits());
    let quadrature_maximum = maximum_absolute_i32(
        quadrature.output_minimum.bits(),
        quadrature.output_maximum.bits(),
    );
    let limit = i128::from(limit.bits());
    direct_maximum * direct_maximum + quadrature_maximum * quadrature_maximum <= limit * limit
}

fn maximum_absolute_i32(left: i32, right: i32) -> i128 {
    i128::from(left).abs().max(i128::from(right).abs())
}

fn phase_to_duty(phase: Q30Interval, positive_limit: Q30) -> Result<Q30Interval, FocError> {
    let limit_bits = i128::from(positive_limit.bits());
    let denominator = 2 * limit_bits;
    let scaled_lower = i128::from(Q30_SCALE) * (limit_bits + i128::from(phase.lower().bits()));
    let scaled_upper = i128::from(Q30_SCALE) * (limit_bits + i128::from(phase.upper().bits()));
    Q30Interval::new(
        q30_from_i128(div_floor(scaled_lower, denominator))?,
        q30_from_i128(div_ceil(scaled_upper, denominator))?,
    )
}

fn scale_wide_interval(
    lower_bits: i64,
    upper_bits: i64,
    scale: Q30Interval,
) -> Result<Q30Interval, FocError> {
    let products = [
        i128::from(lower_bits) * i128::from(scale.lower().bits()),
        i128::from(lower_bits) * i128::from(scale.upper().bits()),
        i128::from(upper_bits) * i128::from(scale.lower().bits()),
        i128::from(upper_bits) * i128::from(scale.upper().bits()),
    ];
    let minimum = products.into_iter().min().ok_or(FocError::Range)?;
    let maximum = products.into_iter().max().ok_or(FocError::Range)?;
    Q30Interval::new(
        q30_from_i128(div_floor(minimum, i128::from(Q30_SCALE)))?,
        q30_from_i128(div_ceil(maximum, i128::from(Q30_SCALE)))?,
    )
}

fn ratio_bounds(numerator: i32, denominator: i32) -> (i128, i128) {
    let numerator = i128::from(numerator) * i128::from(Q30_SCALE);
    let denominator = i128::from(denominator);
    (
        div_floor(numerator, denominator),
        div_ceil(numerator, denominator),
    )
}

fn q30_from_i64(value: i64) -> Result<Q30, FocError> {
    i32::try_from(value)
        .map(Q30::from_bits)
        .map_err(|_| FocError::Range)
}

fn q30_from_i128(value: i128) -> Result<Q30, FocError> {
    i32::try_from(value)
        .map(Q30::from_bits)
        .map_err(|_| FocError::Range)
}

fn multiply_q30_by_wide(factor: Q30, value_bits: i64) -> Result<i64, FocError> {
    let product = i128::from(factor.bits())
        .checked_mul(i128::from(value_bits))
        .ok_or(FocError::Range)?;
    i64::try_from(round_ties_even(product, i128::from(Q30_SCALE))).map_err(|_| FocError::Range)
}

fn div_floor(numerator: i128, denominator: i128) -> i128 {
    if denominator < 0 {
        (-numerator).div_euclid(-denominator)
    } else {
        numerator.div_euclid(denominator)
    }
}

fn div_ceil(numerator: i128, denominator: i128) -> i128 {
    -div_floor(-numerator, denominator)
}

fn div_floor_i64(numerator: i64, denominator: i64) -> i64 {
    if denominator < 0 {
        (-numerator).div_euclid(-denominator)
    } else {
        numerator.div_euclid(denominator)
    }
}

fn div_ceil_i64(numerator: i64, denominator: i64) -> i64 {
    -div_floor_i64(-numerator, denominator)
}

fn round_ties_even(numerator: i128, denominator: i128) -> i128 {
    let (numerator, denominator) = if denominator < 0 {
        (-numerator, -denominator)
    } else {
        (numerator, denominator)
    };
    let floor = numerator.div_euclid(denominator);
    let remainder = numerator.rem_euclid(denominator);
    match (remainder * 2).cmp(&denominator) {
        core::cmp::Ordering::Less => floor,
        core::cmp::Ordering::Greater => floor + 1,
        core::cmp::Ordering::Equal if floor & 1 == 0 => floor,
        core::cmp::Ordering::Equal => floor + 1,
    }
}

fn clamp_i64_to_i32_bounds(value: i64, lower: Q30, upper: Q30) -> i32 {
    value.clamp(i64::from(lower.bits()), i64::from(upper.bits())) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(numerator: i64, denominator: i64) -> Q30Interval {
        Q30Interval::from_ratio(numerator, denominator).unwrap()
    }

    fn point(numerator: i64, denominator: i64) -> Q30 {
        q(numerator, denominator).midpoint()
    }

    #[test]
    fn interval_arithmetic_is_outward_and_division_rejects_zero() {
        let one_third = q(1, 3);
        assert_eq!(one_third.width_ulps(), 1);
        let product = one_third.checked_mul(q(3, 2)).unwrap();
        assert!(product.contains(Q30::HALF));
        assert_eq!(q(1, -2), Q30Interval::point(point(-1, 2)));
        assert_eq!(q(-1, -2), Q30Interval::point(Q30::HALF));
        assert_eq!(
            Q30Interval::ONE.checked_div(Q30Interval::new(point(-1, 2), point(1, 2)).unwrap()),
            Err(FocError::DivideByZero)
        );
        assert_eq!(Q30::from_bits(i32::MIN).checked_neg(), Err(FocError::Range));
    }

    #[test]
    fn exact_ratio_and_point_operations_enclose_independent_integer_facts() {
        for numerator in -17_i64..=17 {
            for denominator in (-17_i64..=17).filter(|value| *value != 0) {
                let (normalized_numerator, normalized_denominator) = if denominator < 0 {
                    (-numerator, -denominator)
                } else {
                    (numerator, denominator)
                };
                if normalized_numerator < -2 * normalized_denominator
                    || normalized_numerator >= 2 * normalized_denominator
                {
                    continue;
                }
                let interval = Q30Interval::from_ratio(numerator, denominator).unwrap();
                let scaled_numerator = i128::from(normalized_numerator) * i128::from(Q30_SCALE);
                assert!(
                    i128::from(interval.lower().bits()) * i128::from(normalized_denominator)
                        <= scaled_numerator
                );
                assert!(
                    scaled_numerator
                        <= i128::from(interval.upper().bits()) * i128::from(normalized_denominator)
                );
                assert!(interval.width_ulps() <= 1);
            }
        }

        let points = [
            Q30::NEG_ONE.bits(),
            -Q30::HALF.bits(),
            -1,
            0,
            1,
            Q30::HALF.bits(),
            Q30::ONE.bits(),
        ];
        for left in points {
            for right in points {
                let product = Q30Interval::point(Q30::from_bits(left))
                    .checked_mul(Q30Interval::point(Q30::from_bits(right)))
                    .unwrap();
                let exact_product = i128::from(left) * i128::from(right);
                assert!(
                    i128::from(product.lower().bits()) * i128::from(Q30_SCALE) <= exact_product
                );
                assert!(
                    exact_product <= i128::from(product.upper().bits()) * i128::from(Q30_SCALE)
                );

                if right != 0 && i64::from(left).abs() <= i64::from(right).abs() {
                    let quotient = Q30Interval::point(Q30::from_bits(left))
                        .checked_div(Q30Interval::point(Q30::from_bits(right)))
                        .unwrap();
                    let (scaled_numerator, positive_denominator) = if right < 0 {
                        (
                            -i128::from(left) * i128::from(Q30_SCALE),
                            -i128::from(right),
                        )
                    } else {
                        (i128::from(left) * i128::from(Q30_SCALE), i128::from(right))
                    };
                    assert!(
                        i128::from(quotient.lower().bits()) * positive_denominator
                            <= scaled_numerator
                    );
                    assert!(
                        scaled_numerator
                            <= i128::from(quotient.upper().bits()) * positive_denominator
                    );
                }
            }
        }
    }

    #[test]
    fn nominal_multiplication_rounds_halfway_cases_to_even() {
        assert_eq!(Q30::from_bits(1).checked_mul(Q30::HALF).unwrap(), Q30::ZERO);
        assert_eq!(
            Q30::from_bits(3).checked_mul(Q30::HALF).unwrap(),
            Q30::from_bits(2)
        );
        assert_eq!(
            Q30::from_bits(-1).checked_mul(Q30::HALF).unwrap(),
            Q30::ZERO
        );
        assert_eq!(
            Q30::from_bits(-3).checked_mul(Q30::HALF).unwrap(),
            Q30::from_bits(-2)
        );
    }

    #[test]
    fn transform_constant_intervals_have_integer_certificates() {
        let scale = i128::from(Q30_SCALE);
        assert!(3 * i128::from(TWO_THIRDS.lower().bits()) <= 2 * scale);
        assert!(2 * scale <= 3 * i128::from(TWO_THIRDS.upper().bits()));

        let inverse_lower = i128::from(INV_SQRT_3.lower().bits());
        let inverse_upper = i128::from(INV_SQRT_3.upper().bits());
        assert!(3 * inverse_lower * inverse_lower <= scale * scale);
        assert!(scale * scale <= 3 * inverse_upper * inverse_upper);

        let root_lower = i128::from(SQRT_3_OVER_2.lower().bits());
        let root_upper = i128::from(SQRT_3_OVER_2.upper().bits());
        assert!(4 * root_lower * root_lower <= 3 * scale * scale);
        assert!(3 * scale * scale <= 4 * root_upper * root_upper);
    }

    #[test]
    fn clarke_park_round_trip_contains_balanced_known_vectors() {
        let phases = Phase3 {
            a: q(1, 1),
            b: q(-1, 2),
            c: q(-1, 2),
        };
        let stationary = clarke(phases, Q30::ZERO).unwrap();
        assert!(stationary.alpha.contains(Q30::ONE));
        assert!(stationary.beta.contains(Q30::ZERO));
        let rotating = park(stationary, Rotation::zero()).unwrap();
        assert!(rotating.d.contains(Q30::ONE));
        assert!(rotating.q.contains(Q30::ZERO));
        let recovered = inverse_clarke(inverse_park(rotating, Rotation::zero()).unwrap()).unwrap();
        assert!(recovered.a.contains(Q30::ONE));
        assert!(recovered.b.contains(point(-1, 2)));
        assert!(recovered.c.contains(point(-1, 2)));

        let unbalanced = Phase3 {
            c: q(-1, 4),
            ..phases
        };
        assert_eq!(
            clarke(unbalanced, point(1, 10)),
            Err(FocError::UnbalancedPhases)
        );

        let wide_alpha_intermediate = clarke(
            Phase3 {
                a: q(3, 2),
                b: q(-1, 1),
                c: q(-1, 2),
            },
            Q30::ZERO,
        )
        .unwrap();
        assert!(wide_alpha_intermediate.alpha.contains(point(3, 2)));

        let wide_beta_intermediate = clarke(
            Phase3 {
                a: Q30Interval::ZERO,
                b: q(3, 2),
                c: q(-3, 2),
            },
            Q30::ZERO,
        )
        .unwrap();
        assert!(wide_beta_intermediate.beta.lower() > point(17, 10));
        assert!(wide_beta_intermediate.beta.upper() < point(9, 5));
    }

    #[test]
    fn rotation_requires_a_unit_vector_certificate() {
        assert_eq!(
            Rotation::new(q(1, 2), q(1, 2), point(1, 100)),
            Err(FocError::Rotation)
        );
        let sine = Q30Interval::new(Q30::from_bits(-1), Q30::from_bits(1)).unwrap();
        let rotation = Rotation::new(sine, Q30Interval::ONE, Q30::from_bits(1)).unwrap();
        assert!(rotation.norm().contains(Q30::ONE));
    }

    #[test]
    fn min_max_modulation_contains_known_duties_and_rejects_overrange() {
        let result = space_vector_modulate(
            AlphaBeta {
                alpha: q(1, 2),
                beta: Q30Interval::ZERO,
            },
            Q30::ONE,
        )
        .unwrap();
        assert!(result.duties.a.contains(point(11, 16)));
        assert!(result.duties.b.contains(point(5, 16)));
        assert!(result.duties.c.contains(point(5, 16)));
        assert_eq!(
            phase_to_duty(Q30Interval::point(Q30::NEG_ONE), Q30::ONE).unwrap(),
            Q30Interval::ZERO
        );
        assert_eq!(
            phase_to_duty(Q30Interval::ONE, Q30::ONE).unwrap(),
            Q30Interval::ONE
        );
        assert_eq!(
            space_vector_modulate(
                AlphaBeta {
                    alpha: q(3, 2),
                    beta: Q30Interval::ZERO,
                },
                Q30::ONE,
            ),
            Err(FocError::ModulationRange)
        );
    }

    #[test]
    fn pi_controller_is_deterministic_and_withholds_windup() {
        let config = PiConfig {
            proportional_gain: point(1, 2),
            integral_gain_per_update: point(1, 4),
            integral_minimum: Q30::NEG_ONE,
            integral_maximum: Q30::ONE,
            output_minimum: point(-1, 2),
            output_maximum: point(1, 2),
        };
        let mut controller = PiController::new(config).unwrap();
        let first = controller.update(point(1, 2), Q30::ZERO).unwrap();
        assert_eq!(first.output, point(3, 8));
        assert_eq!(first.integral, point(1, 8));
        let saturated = controller.update(Q30::ONE, Q30::ZERO).unwrap();
        assert_eq!(saturated.output, point(1, 2));
        assert_eq!(saturated.integral, point(1, 8));
        assert!(saturated.saturated);
        let recovery = controller.update(point(-1, 2), Q30::ZERO).unwrap();
        assert!(recovery.output < saturated.output);
        assert!(recovery.integral < saturated.integral);
    }

    #[test]
    fn dq_controller_preserves_widened_endpoint_errors_until_output_clamp() {
        let config = PiConfig {
            proportional_gain: Q30::HALF,
            integral_gain_per_update: Q30::ZERO,
            integral_minimum: Q30::NEG_ONE,
            integral_maximum: Q30::ONE,
            output_minimum: Q30::NEG_ONE,
            output_maximum: Q30::ONE,
        };
        let mut controller = DqCurrentController::new(config, config).unwrap();
        let update = controller
            .update(
                DqPoint {
                    d: Q30::ONE,
                    q: Q30::NEG_ONE,
                },
                DqPoint {
                    d: Q30::NEG_ONE,
                    q: Q30::ONE,
                },
                DqPoint::default(),
            )
            .unwrap();
        assert_eq!(update.voltage.d, Q30::ONE);
        assert_eq!(update.voltage.q, Q30::NEG_ONE);
    }

    #[test]
    fn complete_parameter_snapshot_rejects_invalid_identity_and_clock_ratios() {
        let pi = PiConfig {
            proportional_gain: point(1, 2),
            integral_gain_per_update: point(1, 100),
            integral_minimum: Q30::NEG_ONE,
            integral_maximum: Q30::ONE,
            output_minimum: Q30::from_bits(-Q30::HALF.bits()),
            output_maximum: Q30::HALF,
        };
        let mut snapshot = FocParameterSnapshot {
            configuration_digest: Digest([0x51; 32]),
            pole_pairs: 7,
            timing: FocTimingProfile {
                pwm_hz: 20_000,
                current_loop_hz: 20_000,
                velocity_loop_divider: 20,
                position_loop_divider: 10,
            },
            maximum_phase_current: Q30::ONE,
            maximum_phase_voltage: point(9, 10),
            d_current: pi,
            q_current: pi,
        };
        assert_eq!(snapshot.validate(), Ok(()));
        assert!(DqCurrentController::from_snapshot(&snapshot).is_ok());
        snapshot.d_current.output_minimum = Q30::NEG_ONE;
        snapshot.d_current.output_maximum = Q30::ONE;
        snapshot.q_current.output_minimum = Q30::NEG_ONE;
        snapshot.q_current.output_maximum = Q30::ONE;
        assert_eq!(snapshot.validate(), Err(FocError::Profile));
        snapshot.d_current = pi;
        snapshot.q_current = pi;
        snapshot.timing.current_loop_hz = 12_000;
        assert_eq!(snapshot.validate(), Err(FocError::Profile));
        snapshot.timing.current_loop_hz = 20_000;
        snapshot.configuration_digest = Digest::ZERO;
        assert_eq!(snapshot.validate(), Err(FocError::Profile));
    }

    #[test]
    fn scheduled_current_command_binds_snapshot_and_circular_limits() {
        let pi = PiConfig {
            proportional_gain: Q30::HALF,
            integral_gain_per_update: point(1, 100),
            integral_minimum: Q30::NEG_ONE,
            integral_maximum: Q30::ONE,
            output_minimum: Q30::from_bits(-Q30::HALF.bits()),
            output_maximum: Q30::HALF,
        };
        let snapshot = FocParameterSnapshot {
            configuration_digest: Digest([0x51; 32]),
            pole_pairs: 7,
            timing: FocTimingProfile {
                pwm_hz: 20_000,
                current_loop_hz: 20_000,
                velocity_loop_divider: 20,
                position_loop_divider: 10,
            },
            maximum_phase_current: Q30::ONE,
            maximum_phase_voltage: Q30::ONE,
            d_current: pi,
            q_current: pi,
        };
        let mut command = FocCurrentCommand {
            command_id: 9,
            scheduled_at: DeviceCycle(50_000),
            configuration_digest: snapshot.configuration_digest,
            target: DqPoint {
                d: Q30::HALF,
                q: Q30::HALF,
            },
            voltage_feed_forward: DqPoint::default(),
        };
        assert_eq!(command.validate(&snapshot), Ok(()));
        command.target = DqPoint {
            d: Q30::ONE,
            q: Q30::ONE,
        };
        assert_eq!(command.validate(&snapshot), Err(FocError::CommandRange));
        command.target = DqPoint::default();
        command.configuration_digest = Digest([0x52; 32]);
        assert_eq!(command.validate(&snapshot), Err(FocError::Configuration));
    }
}
