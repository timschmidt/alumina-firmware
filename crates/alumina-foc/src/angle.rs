//! Exact binary-turn reduction and certified fixed-point rotor angles.

use alumina_protocol::{DeviceCycle, Digest};

use super::{FocError, Q30, Q30Interval, Rotation, RotorSample};

/// Number of binary phase lattice points in one complete turn.
pub const PHASE_POINTS_PER_TURN: u64 = 1_u64 << 32;
/// Exact quarter turn on the wrapping binary phase lattice.
pub const QUARTER_TURN_BITS: u32 = 1_u32 << 30;
/// Exact half turn on the wrapping binary phase lattice.
pub const HALF_TURN_BITS: u32 = 1_u32 << 31;
/// Largest admitted observation uncertainty: one thirty-second of a turn.
///
/// This is a representation/admission bound, not a claim that such a large
/// error is suitable for a particular motor. A machine profile normally sets a
/// much tighter [`RotationPrecision`] policy.
pub const MAXIMUM_OBSERVATION_ERROR_BITS: u32 = 1_u32 << 27;

const EIGHTH_TURN_BITS: u64 = 1_u64 << 29;

// Adjacent Q2.30 points containing pi/4. The test certificate derives pi/4
// independently with Machin's identity and directed integer arithmetic.
const PI_OVER_4: Q30Interval = Q30Interval::from_raw_bits(843_314_856, 843_314_857);
#[cfg(test)]
const INV_SQRT_2: Q30Interval = Q30Interval::from_raw_bits(759_250_124, 759_250_125);

// Adjacent Q2.30 points containing reciprocal factorials. Keeping these as
// constants removes general integer division from the fixed-rate angle path.
const INV_2_FACTORIAL: Q30Interval = Q30Interval::from_raw_bits(536_870_912, 536_870_912);
const INV_3_FACTORIAL: Q30Interval = Q30Interval::from_raw_bits(178_956_970, 178_956_971);
const INV_4_FACTORIAL: Q30Interval = Q30Interval::from_raw_bits(44_739_242, 44_739_243);
const INV_5_FACTORIAL: Q30Interval = Q30Interval::from_raw_bits(8_947_848, 8_947_849);
const INV_6_FACTORIAL: Q30Interval = Q30Interval::from_raw_bits(1_491_308, 1_491_309);
const INV_7_FACTORIAL: Q30Interval = Q30Interval::from_raw_bits(213_044, 213_045);
const INV_8_FACTORIAL: Q30Interval = Q30Interval::from_raw_bits(26_630, 26_631);
const INV_9_FACTORIAL: Q30Interval = Q30Interval::from_raw_bits(2_958, 2_959);
const INV_10_FACTORIAL: Q30Interval = Q30Interval::from_raw_bits(295, 296);
const INV_11_FACTORIAL: Q30Interval = Q30Interval::from_raw_bits(26, 27);
const INV_12_FACTORIAL: Q30Interval = Q30Interval::from_raw_bits(2, 3);

/// Wrapping electrical angle whose complete `u32` range is exactly one turn.
///
/// This binary-angle representation makes quadrant reduction, pole-pair
/// multiplication, and wrap exact. Radians appear only inside the explicitly
/// bounded trigonometric approximation.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct ElectricalPhase(u32);

impl ElectricalPhase {
    pub const ZERO: Self = Self(0);
    pub const QUARTER_TURN: Self = Self(QUARTER_TURN_BITS);
    pub const HALF_TURN: Self = Self(HALF_TURN_BITS);
    pub const THREE_QUARTER_TURN: Self = Self(QUARTER_TURN_BITS * 3);

    /// Constructs the exact wrapping phase from its canonical binary-turn bits.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns the canonical binary-turn bits.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Exact modular phase addition.
    pub const fn wrapping_add(self, other: Self) -> Self {
        Self(self.0.wrapping_add(other.0))
    }
}

/// Nominal binary phase plus a symmetric circular error bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElectricalPhaseEstimate {
    nominal: ElectricalPhase,
    maximum_error_bits: u32,
}

impl ElectricalPhaseEstimate {
    /// Constructs an estimate only inside the useful single-arc bound.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::Phase`] when the error exceeds one thirty-second of
    /// a turn.
    pub const fn new(nominal: ElectricalPhase, maximum_error_bits: u32) -> Result<Self, FocError> {
        if maximum_error_bits > MAXIMUM_OBSERVATION_ERROR_BITS {
            Err(FocError::Phase)
        } else {
            Ok(Self {
                nominal,
                maximum_error_bits,
            })
        }
    }

    pub const fn nominal(self) -> ElectricalPhase {
        self.nominal
    }

    pub const fn maximum_error_bits(self) -> u32 {
        self.maximum_error_bits
    }
}

/// Exact admission budget for generated sine/cosine evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RotationPrecision {
    /// Largest permitted width of either component interval in Q2.30 ULPs.
    pub maximum_component_width_ulps: u32,
    /// Largest permitted distance of the squared-norm interval from exact one.
    pub maximum_norm_error_ulps: u32,
}

impl RotationPrecision {
    fn norm_error(self) -> Result<Q30, FocError> {
        i32::try_from(self.maximum_norm_error_ulps)
            .map(Q30::from_bits)
            .map_err(|_| FocError::Precision)
    }
}

/// Generates a certified rotation for one exact binary electrical phase.
///
/// # Errors
///
/// Returns [`FocError::Precision`] when the generated component intervals
/// exceed the caller's exact ULP budget, [`FocError::Rotation`] when the
/// squared-unit-norm certificate exceeds its budget, or a checked arithmetic
/// error.
pub fn rotation_from_phase(
    phase: ElectricalPhase,
    precision: RotationPrecision,
) -> Result<Rotation, FocError> {
    rotation_from_estimate(ElectricalPhaseEstimate::new(phase, 0)?, precision)
}

/// Generates a certified rotation containing every phase in an estimate.
///
/// Exact nearest-quadrant reduction leaves an angle in `[-pi/4, pi/4]`. Sine
/// and cosine are evaluated with outward Q2.30 interval arithmetic and
/// alternating Taylor remainder bounds. Observation uncertainty is then added
/// with the global `|sin(x)-sin(y)| <= |x-y|` and
/// `|cos(x)-cos(y)| <= |x-y|` bounds before the unit-norm certificate is checked.
///
/// # Errors
///
/// Returns a phase, precision, rotation, or arithmetic error if the complete
/// conservative result cannot satisfy the supplied policy.
pub fn rotation_from_estimate(
    estimate: ElectricalPhaseEstimate,
    precision: RotationPrecision,
) -> Result<Rotation, FocError> {
    let (mut sine, mut cosine) = reduced_components(estimate.nominal)?;
    if estimate.maximum_error_bits != 0 {
        let ratio_bits = estimate
            .maximum_error_bits
            .checked_mul(2)
            .ok_or(FocError::Range)?;
        let ratio = Q30Interval::point(Q30::from_bits(
            i32::try_from(ratio_bits).map_err(|_| FocError::Range)?,
        ));
        let maximum_radians = ratio.checked_mul(PI_OVER_4)?.upper();
        sine = expand_unit_interval(sine, maximum_radians)?;
        cosine = expand_unit_interval(cosine, maximum_radians)?;
    }
    if sine.width_ulps() > u64::from(precision.maximum_component_width_ulps)
        || cosine.width_ulps() > u64::from(precision.maximum_component_width_ulps)
    {
        return Err(FocError::Precision);
    }
    Rotation::new(sine, cosine, precision.norm_error()?)
}

/// Direction in which increasing sensor counts move mechanical angle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RotorCountDirection {
    Increasing,
    Decreasing,
}

/// Reduced exact absolute sensor uncertainty measured in encoder counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CountUncertainty {
    numerator: u32,
    denominator: u32,
}

impl CountUncertainty {
    /// Constructs a canonical nonnegative rational count error.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::RotorCalibration`] for zero denominator, a
    /// non-reduced fraction, or a noncanonical zero.
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, FocError> {
        if denominator == 0
            || numerator == 0 && denominator != 1
            || gcd_u32(numerator, denominator) != 1
        {
            return Err(FocError::RotorCalibration);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    pub const fn denominator(self) -> u32 {
        self.denominator
    }
}

/// Immutable exact mapping from one absolute-count sensor to electrical phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RotorCalibration {
    pub configuration_digest: Digest,
    pub counts_per_mechanical_turn: u32,
    pub count_at_reference: u32,
    pub electrical_phase_at_reference: ElectricalPhase,
    pub pole_pairs: u16,
    pub direction: RotorCountDirection,
    /// Symmetric electrical offset/alignment error in binary phase points.
    pub maximum_alignment_error_bits: u32,
    /// Complete count-domain error, including sensor quantization.
    pub maximum_count_error: CountUncertainty,
}

impl RotorCalibration {
    /// Validates identity, modular count geometry, and the worst phase-error arc.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::RotorCalibration`] when a fact is absent, out of
    /// range, or could produce an observation error wider than the admitted
    /// single-arc bound.
    pub fn validate(self) -> Result<(), FocError> {
        if self.configuration_digest.is_zero()
            || self.counts_per_mechanical_turn < 2
            || self.count_at_reference >= self.counts_per_mechanical_turn
            || self.pole_pairs == 0
        {
            return Err(FocError::RotorCalibration);
        }
        let worst_error = sensor_phase_error_bits(
            self.maximum_count_error,
            self.pole_pairs,
            self.counts_per_mechanical_turn,
        )?
        .checked_add(u128::from(self.maximum_alignment_error_bits))
        .ok_or(FocError::RotorCalibration)?
        .checked_add(1)
        .ok_or(FocError::RotorCalibration)?;
        if worst_error > u128::from(MAXIMUM_OBSERVATION_ERROR_BITS) {
            return Err(FocError::RotorCalibration);
        }
        Ok(())
    }

    /// Reduces one bounded absolute count to a circular electrical-phase arc.
    ///
    /// The selected nominal phase is nearest on the `u32` lattice with ties to
    /// even. Its at-most-one-ULP reduction error is retained separately from
    /// the exact rational sensor-error projection.
    ///
    /// # Errors
    ///
    /// Returns [`FocError::RotorSample`] for a raw count outside the calibrated
    /// modulus, or a calibration/arithmetic error.
    pub fn phase_estimate(self, raw_count: u32) -> Result<ElectricalPhaseEstimate, FocError> {
        self.validate()?;
        if raw_count >= self.counts_per_mechanical_turn {
            return Err(FocError::RotorSample);
        }
        let modulus = u64::from(self.counts_per_mechanical_turn);
        let increasing =
            (u64::from(raw_count) + modulus - u64::from(self.count_at_reference)) % modulus;
        let directed = match self.direction {
            RotorCountDirection::Increasing => increasing,
            RotorCountDirection::Decreasing => (modulus - increasing) % modulus,
        };
        let electrical_remainder =
            (u128::from(directed) * u128::from(self.pole_pairs)) % u128::from(modulus);
        let scaled = electrical_remainder
            .checked_mul(u128::from(PHASE_POINTS_PER_TURN))
            .ok_or(FocError::Range)?;
        let (phase_bits, reduction_error) = round_phase_ratio(scaled, u128::from(modulus))?;
        let nominal = self
            .electrical_phase_at_reference
            .wrapping_add(ElectricalPhase::from_bits(phase_bits));
        let maximum_error = sensor_phase_error_bits(
            self.maximum_count_error,
            self.pole_pairs,
            self.counts_per_mechanical_turn,
        )?
        .checked_add(u128::from(self.maximum_alignment_error_bits))
        .ok_or(FocError::RotorCalibration)?
        .checked_add(u128::from(reduction_error))
        .ok_or(FocError::RotorCalibration)?;
        ElectricalPhaseEstimate::new(
            nominal,
            u32::try_from(maximum_error).map_err(|_| FocError::RotorCalibration)?,
        )
    }

    /// Produces a digest-bound rotor sample and certified rotation.
    ///
    /// # Errors
    ///
    /// Returns any calibration, sample, phase-generation, or precision error
    /// before a sample can reach the controller.
    pub fn observe(
        self,
        raw_count: u32,
        observed_at: DeviceCycle,
        precision: RotationPrecision,
    ) -> Result<RotorSample, FocError> {
        let estimate = self.phase_estimate(raw_count)?;
        let rotation = rotation_from_estimate(estimate, precision)?;
        Ok(RotorSample {
            configuration_digest: self.configuration_digest,
            pole_pairs: self.pole_pairs,
            observed_at,
            electrical_phase: estimate.nominal(),
            maximum_phase_error_bits: estimate.maximum_error_bits(),
            rotation,
        })
    }
}

fn reduced_components(phase: ElectricalPhase) -> Result<(Q30Interval, Q30Interval), FocError> {
    let quadrant =
        (((u64::from(phase.bits()) + EIGHTH_TURN_BITS) / u64::from(QUARTER_TURN_BITS)) & 3) as u32;
    let center = quadrant.wrapping_mul(QUARTER_TURN_BITS);
    let delta = phase.bits().wrapping_sub(center) as i32;
    let magnitude = delta.unsigned_abs();
    if magnitude == 0 {
        return Ok(match quadrant {
            0 => (Q30Interval::ZERO, Q30Interval::ONE),
            1 => (Q30Interval::ONE, Q30Interval::ZERO),
            2 => (Q30Interval::ZERO, Q30Interval::point(Q30::NEG_ONE)),
            3 => (Q30Interval::point(Q30::NEG_ONE), Q30Interval::ZERO),
            _ => return Err(FocError::Phase),
        });
    }
    if u64::from(magnitude) > EIGHTH_TURN_BITS {
        return Err(FocError::Phase);
    }
    let ratio_bits = magnitude.checked_mul(2).ok_or(FocError::Range)?;
    let ratio = Q30Interval::point(Q30::from_bits(
        i32::try_from(ratio_bits).map_err(|_| FocError::Range)?,
    ));
    let radians = ratio.checked_mul(PI_OVER_4)?;
    let mut sine = sine_first_octant(radians)?;
    let cosine = cosine_first_octant(radians)?;
    if delta < 0 {
        sine = sine.checked_neg()?;
    }
    match quadrant {
        0 => Ok((sine, cosine)),
        1 => Ok((cosine, sine.checked_neg()?)),
        2 => Ok((sine.checked_neg()?, cosine.checked_neg()?)),
        3 => Ok((cosine.checked_neg()?, sine)),
        _ => Err(FocError::Phase),
    }
}

fn sine_first_octant(x: Q30Interval) -> Result<Q30Interval, FocError> {
    let x2 = x.checked_mul(x)?;
    let x3 = x.checked_mul(x2)?;
    let x5 = x3.checked_mul(x2)?;
    let x7 = x5.checked_mul(x2)?;
    let x9 = x7.checked_mul(x2)?;
    let x11 = x9.checked_mul(x2)?;
    let partial = x
        .checked_sub(x3.checked_mul(INV_3_FACTORIAL)?)?
        .checked_add(x5.checked_mul(INV_5_FACTORIAL)?)?
        .checked_sub(x7.checked_mul(INV_7_FACTORIAL)?)?
        .checked_add(x9.checked_mul(INV_9_FACTORIAL)?)?
        .checked_sub(x11.checked_mul(INV_11_FACTORIAL)?)?;
    // The next positive term is x^13/13! < 1/13! < one Q2.30 ULP.
    widen_upper_one_ulp(partial)
}

fn cosine_first_octant(x: Q30Interval) -> Result<Q30Interval, FocError> {
    let x2 = x.checked_mul(x)?;
    let x4 = x2.checked_mul(x2)?;
    let x6 = x4.checked_mul(x2)?;
    let x8 = x6.checked_mul(x2)?;
    let x10 = x8.checked_mul(x2)?;
    let x12 = x10.checked_mul(x2)?;
    let partial = Q30Interval::ONE
        .checked_sub(x2.checked_mul(INV_2_FACTORIAL)?)?
        .checked_add(x4.checked_mul(INV_4_FACTORIAL)?)?
        .checked_sub(x6.checked_mul(INV_6_FACTORIAL)?)?
        .checked_add(x8.checked_mul(INV_8_FACTORIAL)?)?
        .checked_sub(x10.checked_mul(INV_10_FACTORIAL)?)?
        .checked_add(x12.checked_mul(INV_12_FACTORIAL)?)?;
    // The next negative term has magnitude x^14/14! < one Q2.30 ULP.
    widen_lower_one_ulp(partial)
}

fn widen_upper_one_ulp(value: Q30Interval) -> Result<Q30Interval, FocError> {
    Q30Interval::new(
        value.lower(),
        Q30::from_bits(value.upper().bits().checked_add(1).ok_or(FocError::Range)?),
    )
}

fn widen_lower_one_ulp(value: Q30Interval) -> Result<Q30Interval, FocError> {
    Q30Interval::new(
        Q30::from_bits(value.lower().bits().checked_sub(1).ok_or(FocError::Range)?),
        value.upper(),
    )
}

fn expand_unit_interval(value: Q30Interval, maximum_delta: Q30) -> Result<Q30Interval, FocError> {
    if maximum_delta < Q30::ZERO {
        return Err(FocError::Phase);
    }
    let lower = (i64::from(value.lower().bits()) - i64::from(maximum_delta.bits()))
        .max(i64::from(Q30::NEG_ONE.bits()));
    let upper = (i64::from(value.upper().bits()) + i64::from(maximum_delta.bits()))
        .min(i64::from(Q30::ONE.bits()));
    Q30Interval::new(
        Q30::from_bits(i32::try_from(lower).map_err(|_| FocError::Range)?),
        Q30::from_bits(i32::try_from(upper).map_err(|_| FocError::Range)?),
    )
}

fn sensor_phase_error_bits(
    count_error: CountUncertainty,
    pole_pairs: u16,
    counts_per_turn: u32,
) -> Result<u128, FocError> {
    let numerator = u128::from(count_error.numerator)
        .checked_mul(u128::from(pole_pairs))
        .and_then(|value| value.checked_mul(u128::from(PHASE_POINTS_PER_TURN)))
        .ok_or(FocError::RotorCalibration)?;
    let denominator = u128::from(count_error.denominator)
        .checked_mul(u128::from(counts_per_turn))
        .ok_or(FocError::RotorCalibration)?;
    div_ceil_u128(numerator, denominator)
}

fn round_phase_ratio(numerator: u128, denominator: u128) -> Result<(u32, u8), FocError> {
    if denominator == 0 {
        return Err(FocError::DivideByZero);
    }
    let floor = numerator / denominator;
    let remainder = numerator % denominator;
    let doubled = remainder.checked_mul(2).ok_or(FocError::Range)?;
    let rounded = match doubled.cmp(&denominator) {
        core::cmp::Ordering::Less => floor,
        core::cmp::Ordering::Greater => floor.checked_add(1).ok_or(FocError::Range)?,
        core::cmp::Ordering::Equal if floor & 1 == 0 => floor,
        core::cmp::Ordering::Equal => floor.checked_add(1).ok_or(FocError::Range)?,
    };
    let wrapped = rounded % u128::from(PHASE_POINTS_PER_TURN);
    Ok((
        u32::try_from(wrapped).map_err(|_| FocError::Range)?,
        u8::from(remainder != 0),
    ))
}

fn div_ceil_u128(numerator: u128, denominator: u128) -> Result<u128, FocError> {
    if denominator == 0 {
        return Err(FocError::DivideByZero);
    }
    Ok(numerator / denominator + u128::from(!numerator.is_multiple_of(denominator)))
}

fn gcd_u32(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FocParameterSnapshot, FocTimingProfile, PiConfig, Q30_SCALE, SQRT_3_OVER_2};

    const BASE_PRECISION: RotationPrecision = RotationPrecision {
        maximum_component_width_ulps: 128,
        maximum_norm_error_ulps: 256,
    };

    #[test]
    fn pi_and_factorial_intervals_have_integer_certificates() {
        let scale = i128::from(Q30_SCALE);
        for (interval, denominator) in [
            (INV_2_FACTORIAL, 2_i128),
            (INV_3_FACTORIAL, 6),
            (INV_4_FACTORIAL, 24),
            (INV_5_FACTORIAL, 120),
            (INV_6_FACTORIAL, 720),
            (INV_7_FACTORIAL, 5_040),
            (INV_8_FACTORIAL, 40_320),
            (INV_9_FACTORIAL, 362_880),
            (INV_10_FACTORIAL, 3_628_800),
            (INV_11_FACTORIAL, 39_916_800),
            (INV_12_FACTORIAL, 479_001_600),
        ] {
            assert!(i128::from(interval.lower().bits()) * denominator <= scale);
            assert!(scale <= i128::from(interval.upper().bits()) * denominator);
            assert!(interval.width_ulps() <= 1);
        }

        let (atan_fifth_lower, atan_fifth_upper) = atan_reciprocal_bounds(5, 20);
        let (atan_239_lower, atan_239_upper) = atan_reciprocal_bounds(239, 6);
        let machin_lower = 4 * atan_fifth_lower - atan_239_upper;
        let machin_upper = 4 * atan_fifth_upper - atan_239_lower;
        let q30_to_certificate = 1_i128 << (96 - 30);
        assert!(i128::from(PI_OVER_4.lower().bits()) * q30_to_certificate <= machin_lower);
        assert!(machin_upper <= i128::from(PI_OVER_4.upper().bits()) * q30_to_certificate);
        assert_eq!(PI_OVER_4.width_ulps(), 1);

        let root_lower = i128::from(INV_SQRT_2.lower().bits());
        let root_upper = i128::from(INV_SQRT_2.upper().bits());
        assert!(2 * root_lower * root_lower <= scale * scale);
        assert!(scale * scale <= 2 * root_upper * root_upper);
    }

    #[test]
    fn cardinals_are_exact_and_quadrants_preserve_component_symmetry() {
        let zero = rotation_from_phase(ElectricalPhase::ZERO, BASE_PRECISION).unwrap();
        assert_eq!(zero.sine(), Q30Interval::ZERO);
        assert_eq!(zero.cosine(), Q30Interval::ONE);
        let quarter = rotation_from_phase(ElectricalPhase::QUARTER_TURN, BASE_PRECISION).unwrap();
        assert_eq!(quarter.sine(), Q30Interval::ONE);
        assert_eq!(quarter.cosine(), Q30Interval::ZERO);
        let half = rotation_from_phase(ElectricalPhase::HALF_TURN, BASE_PRECISION).unwrap();
        assert_eq!(half.sine(), Q30Interval::ZERO);
        assert_eq!(half.cosine(), Q30Interval::point(Q30::NEG_ONE));

        let phase = ElectricalPhase::from_bits(0x1234_5678);
        let first = rotation_from_phase(phase, BASE_PRECISION).unwrap();
        let next = rotation_from_phase(
            phase.wrapping_add(ElectricalPhase::QUARTER_TURN),
            BASE_PRECISION,
        )
        .unwrap();
        assert_eq!(next.sine(), first.cosine());
        assert_eq!(next.cosine(), first.sine().checked_neg().unwrap());
    }

    #[test]
    fn eighth_turn_contains_independently_certified_square_root() {
        let rotation =
            rotation_from_phase(ElectricalPhase::from_bits(1_u32 << 29), BASE_PRECISION).unwrap();
        assert!(rotation.sine().lower() <= INV_SQRT_2.lower());
        assert!(rotation.sine().upper() >= INV_SQRT_2.upper());
        assert!(rotation.cosine().lower() <= INV_SQRT_2.lower());
        assert!(rotation.cosine().upper() >= INV_SQRT_2.upper());
    }

    #[test]
    fn reduced_generator_grid_stays_inside_the_tested_ulp_budget() {
        let mut maximum_component_width = 0_u64;
        let mut maximum_norm_error = 0_u64;
        for index in 0_u32..4_096 {
            let phase = ElectricalPhase::from_bits(index << 20);
            let rotation = rotation_from_phase(phase, BASE_PRECISION).unwrap();
            maximum_component_width = maximum_component_width
                .max(rotation.sine().width_ulps())
                .max(rotation.cosine().width_ulps());
            maximum_norm_error = maximum_norm_error
                .max(
                    (i64::from(Q30::ONE.bits()) - i64::from(rotation.norm().lower().bits())) as u64,
                )
                .max(
                    (i64::from(rotation.norm().upper().bits()) - i64::from(Q30::ONE.bits())) as u64,
                );
        }
        assert!(maximum_component_width <= u64::from(BASE_PRECISION.maximum_component_width_ulps));
        assert!(maximum_norm_error <= u64::from(BASE_PRECISION.maximum_norm_error_ulps));
        assert_eq!(
            rotation_from_phase(
                ElectricalPhase::from_bits(0x1234_5678),
                RotationPrecision {
                    maximum_component_width_ulps: 1,
                    maximum_norm_error_ulps: 1,
                },
            ),
            Err(FocError::Precision)
        );
    }

    #[test]
    fn exact_count_reduction_retains_rounding_and_sensor_error() {
        let calibration = RotorCalibration {
            configuration_digest: Digest([0x35; 32]),
            counts_per_mechanical_turn: 4_096,
            count_at_reference: 123,
            electrical_phase_at_reference: ElectricalPhase::QUARTER_TURN,
            pole_pairs: 7,
            direction: RotorCountDirection::Increasing,
            maximum_alignment_error_bits: 1_024,
            maximum_count_error: CountUncertainty::new(1, 2).unwrap(),
        };
        calibration.validate().unwrap();
        let reference = calibration.phase_estimate(123).unwrap();
        assert_eq!(reference.nominal(), ElectricalPhase::QUARTER_TURN);
        assert_eq!(reference.maximum_error_bits(), 3_671_040);

        let precision = RotationPrecision {
            maximum_component_width_ulps: 12_000_000,
            maximum_norm_error_ulps: 12_000_000,
        };
        let sample = calibration
            .observe(123, DeviceCycle(55), precision)
            .unwrap();
        assert_eq!(
            sample.configuration_digest,
            calibration.configuration_digest
        );
        assert_eq!(sample.observed_at, DeviceCycle(55));
        assert_eq!(sample.pole_pairs, 7);
        assert_eq!(sample.electrical_phase, ElectricalPhase::QUARTER_TURN);
        assert_eq!(sample.maximum_phase_error_bits, 3_671_040);
        assert!(sample.rotation.sine().contains(Q30::ONE));

        let pi = PiConfig {
            proportional_gain: Q30::ZERO,
            integral_gain_per_update: Q30::ZERO,
            integral_minimum: Q30::NEG_ONE,
            integral_maximum: Q30::ONE,
            output_minimum: Q30::from_bits(-Q30::HALF.bits()),
            output_maximum: Q30::HALF,
        };
        let snapshot = FocParameterSnapshot {
            configuration_digest: calibration.configuration_digest,
            pole_pairs: calibration.pole_pairs,
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
        assert_eq!(sample.validate_for(&snapshot, precision), Ok(()));
        let mut forged = sample;
        forged.rotation = Rotation::zero();
        assert_eq!(
            forged.validate_for(&snapshot, precision),
            Err(FocError::RotorSample)
        );
        forged = sample;
        forged.pole_pairs += 1;
        assert_eq!(
            forged.validate_for(&snapshot, precision),
            Err(FocError::Configuration)
        );

        assert_eq!(
            calibration.phase_estimate(4_096),
            Err(FocError::RotorSample)
        );
    }

    #[test]
    fn count_derived_thirty_degrees_contains_exact_known_components() {
        let calibration = RotorCalibration {
            configuration_digest: Digest([0x37; 32]),
            counts_per_mechanical_turn: 12,
            count_at_reference: 0,
            electrical_phase_at_reference: ElectricalPhase::ZERO,
            pole_pairs: 1,
            direction: RotorCountDirection::Increasing,
            maximum_alignment_error_bits: 0,
            maximum_count_error: CountUncertainty::new(0, 1).unwrap(),
        };
        let estimate = calibration.phase_estimate(1).unwrap();
        assert_eq!(estimate.maximum_error_bits(), 1);
        let rotation = rotation_from_estimate(
            estimate,
            RotationPrecision {
                maximum_component_width_ulps: 128,
                maximum_norm_error_ulps: 256,
            },
        )
        .unwrap();
        assert!(rotation.sine().contains(Q30::HALF));
        assert!(rotation.cosine().lower() <= SQRT_3_OVER_2.lower());
        assert!(rotation.cosine().upper() >= SQRT_3_OVER_2.upper());
    }

    #[test]
    fn non_power_of_two_counts_round_to_nearest_ties_even_and_reverse() {
        let mut calibration = RotorCalibration {
            configuration_digest: Digest([0x36; 32]),
            counts_per_mechanical_turn: 3,
            count_at_reference: 0,
            electrical_phase_at_reference: ElectricalPhase::ZERO,
            pole_pairs: 1,
            direction: RotorCountDirection::Increasing,
            maximum_alignment_error_bits: 0,
            maximum_count_error: CountUncertainty::new(0, 1).unwrap(),
        };
        let one_third = calibration.phase_estimate(1).unwrap();
        assert_eq!(one_third.nominal().bits(), 0x5555_5555);
        assert_eq!(one_third.maximum_error_bits(), 1);
        let two_thirds = calibration.phase_estimate(2).unwrap();
        assert_eq!(two_thirds.nominal().bits(), 0xaaaa_aaab);
        assert_eq!(two_thirds.maximum_error_bits(), 1);

        calibration.direction = RotorCountDirection::Decreasing;
        assert_eq!(
            calibration.phase_estimate(1).unwrap().nominal(),
            two_thirds.nominal()
        );
        calibration.maximum_count_error = CountUncertainty::new(1, 1).unwrap();
        assert_eq!(calibration.validate(), Err(FocError::RotorCalibration));
    }

    #[test]
    fn count_uncertainty_is_canonical() {
        assert_eq!(CountUncertainty::new(0, 2), Err(FocError::RotorCalibration));
        assert_eq!(CountUncertainty::new(2, 4), Err(FocError::RotorCalibration));
        assert_eq!(CountUncertainty::new(1, 0), Err(FocError::RotorCalibration));
        assert_eq!(CountUncertainty::new(1, 2).unwrap().numerator(), 1);
        assert_eq!(CountUncertainty::new(1, 2).unwrap().denominator(), 2);
    }

    #[test]
    fn phase_ratio_rounding_encloses_an_independent_integer_grid() {
        for denominator in 2_u128..=127 {
            for numerator in 0_u128..denominator {
                let exact_scaled = numerator * u128::from(PHASE_POINTS_PER_TURN);
                let (bits, error) = round_phase_ratio(exact_scaled, denominator).unwrap();
                let selected_scaled = u128::from(bits) * denominator;
                let difference = exact_scaled.abs_diff(selected_scaled);
                assert!(difference * 2 <= denominator);
                assert_eq!(error == 0, difference == 0);
            }
        }
    }

    fn atan_reciprocal_bounds(divisor: i128, terms: usize) -> (i128, i128) {
        let scale = 1_i128 << 96;
        let square = divisor * divisor;
        let mut power = divisor;
        let mut lower = 0_i128;
        let mut upper = 0_i128;
        for index in 0..terms {
            let denominator = (2 * index as i128 + 1) * power;
            let term_lower = scale / denominator;
            let term_upper = term_lower + i128::from(scale.rem_euclid(denominator) != 0);
            if index.is_multiple_of(2) {
                lower += term_lower;
                upper += term_upper;
            } else {
                lower -= term_upper;
                upper -= term_lower;
            }
            power *= square;
        }
        let next_denominator = (2 * terms as i128 + 1) * power;
        let next_upper =
            scale / next_denominator + i128::from(scale.rem_euclid(next_denominator) != 0);
        if terms.is_multiple_of(2) {
            upper += next_upper;
        } else {
            lower -= next_upper;
        }
        (lower, upper)
    }
}
