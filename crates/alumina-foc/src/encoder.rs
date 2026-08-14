//! Exact absolute-count unwrapping and bounded servo observations.
//!
//! This module begins after a sensor transport has produced one raw absolute
//! count with truthful device-cycle stamps. It does not read a bus, infer a
//! timestamp, home an axis, establish a physical speed bound, or qualify an
//! encoder. A complete machine configuration must provide those facts and an
//! explicit multi-turn seed before this portable estimator can be selected.

use alumina_protocol::{DeviceCycle, Digest};

use crate::{
    CountUncertainty, Q30, Q30_SCALE, Q30Interval, RotorCountDirection, ServoKinematicSample,
    ServoPosition, ServoPositionInterval, div_ceil, div_floor,
};

/// Two reduced positive rational scales used by the mechanical estimator.
///
/// `position_bits_per_turn` is measured in Q31.32 position-lattice bits per
/// positive mechanical turn. `counts_per_second_at_velocity_one` defines the
/// count rate represented by normalized velocity `+1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoEncoderScale {
    position_bits_per_turn_numerator: u64,
    position_bits_per_turn_denominator: u64,
    counts_per_second_at_velocity_one_numerator: u64,
    counts_per_second_at_velocity_one_denominator: u64,
}

impl ServoEncoderScale {
    /// Constructs two canonical reduced positive ratios.
    pub fn new(
        position_bits_per_turn_numerator: u64,
        position_bits_per_turn_denominator: u64,
        counts_per_second_at_velocity_one_numerator: u64,
        counts_per_second_at_velocity_one_denominator: u64,
    ) -> Result<Self, ServoEncoderScaleError> {
        if position_bits_per_turn_numerator == 0
            || position_bits_per_turn_denominator == 0
            || counts_per_second_at_velocity_one_numerator == 0
            || counts_per_second_at_velocity_one_denominator == 0
        {
            return Err(ServoEncoderScaleError::Zero);
        }
        if gcd_u64(
            position_bits_per_turn_numerator,
            position_bits_per_turn_denominator,
        ) != 1
            || gcd_u64(
                counts_per_second_at_velocity_one_numerator,
                counts_per_second_at_velocity_one_denominator,
            ) != 1
        {
            return Err(ServoEncoderScaleError::NonCanonical);
        }
        Ok(Self {
            position_bits_per_turn_numerator,
            position_bits_per_turn_denominator,
            counts_per_second_at_velocity_one_numerator,
            counts_per_second_at_velocity_one_denominator,
        })
    }

    /// Numerator of Q31.32 position bits per positive mechanical turn.
    pub const fn position_bits_per_turn_numerator(self) -> u64 {
        self.position_bits_per_turn_numerator
    }

    /// Denominator of Q31.32 position bits per positive mechanical turn.
    pub const fn position_bits_per_turn_denominator(self) -> u64 {
        self.position_bits_per_turn_denominator
    }

    /// Numerator of raw counts per second at normalized velocity one.
    pub const fn counts_per_second_at_velocity_one_numerator(self) -> u64 {
        self.counts_per_second_at_velocity_one_numerator
    }

    /// Denominator of raw counts per second at normalized velocity one.
    pub const fn counts_per_second_at_velocity_one_denominator(self) -> u64 {
        self.counts_per_second_at_velocity_one_denominator
    }
}

/// Rejected rational scale construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoEncoderScaleError {
    /// At least one numerator or denominator was zero.
    Zero,
    /// At least one ratio was not reduced to lowest terms.
    NonCanonical,
}

/// Complete portable absolute-encoder estimator profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoEncoderProfile {
    /// Complete immutable machine-configuration identity.
    pub configuration_digest: Digest,
    /// Exact raw-count modulus for one mechanical turn.
    pub counts_per_mechanical_turn: u32,
    /// Raw count corresponding to the position reference, modulo one turn.
    pub count_at_reference: u32,
    /// Relationship between increasing raw counts and positive axis motion.
    pub direction: RotorCountDirection,
    /// Complete symmetric error of each accepted raw count.
    pub maximum_count_error: CountUncertainty,
    /// Exact Q31.32 position at directed unwrapped count zero.
    pub position_at_reference: ServoPosition,
    /// Exact rational position and normalized-rate scales.
    pub scale: ServoEncoderScale,
    /// Exact device-cycle counter frequency.
    pub device_cycle_hz: u32,
    /// Required physical-sample cadence in device cycles.
    pub sample_period_cycles: u64,
    /// Largest accepted sample-to-availability latency.
    pub maximum_observation_latency_cycles: u64,
    /// Largest normalized speed assumed when selecting one wrap candidate.
    pub maximum_trackable_velocity: Q30,
    /// Conservative velocity endpoints admitted to the servo controller.
    pub maximum_admitted_velocity: Q30,
    /// Symmetric endpoint-velocity error beyond two-count uncertainty.
    ///
    /// This must include the secant-to-newest-sample difference from bounded
    /// acceleration plus any other configured estimator/model error. It is
    /// never inferred from the two raw counts.
    pub maximum_velocity_estimation_error: Q30,
    /// Largest admitted Q31.32 position interval width.
    pub maximum_position_interval_width_ulps: u64,
    /// Largest admitted Q2.30 velocity interval width.
    pub maximum_velocity_interval_width_ulps: u32,
}

impl ServoEncoderProfile {
    /// Validates identity, geometry, clocks, bounds, and unique wrap selection.
    pub fn validate(self) -> Result<(), ServoEncoderProfileError> {
        if self.configuration_digest.is_zero() {
            return Err(ServoEncoderProfileError::MissingConfiguration);
        }
        if self.counts_per_mechanical_turn < 2
            || self.count_at_reference >= self.counts_per_mechanical_turn
        {
            return Err(ServoEncoderProfileError::CountGeometry);
        }
        if self.device_cycle_hz == 0 {
            return Err(ServoEncoderProfileError::DeviceClock);
        }
        if self.sample_period_cycles == 0 {
            return Err(ServoEncoderProfileError::SamplePeriod);
        }
        if self.maximum_observation_latency_cycles > self.sample_period_cycles {
            return Err(ServoEncoderProfileError::ObservationLatency);
        }
        if self.maximum_admitted_velocity.bits() <= 0
            || self.maximum_trackable_velocity.bits() < self.maximum_admitted_velocity.bits()
            || self.maximum_trackable_velocity.bits() > Q30::ONE.bits()
            || self.maximum_velocity_estimation_error.bits() <= 0
            || self.maximum_velocity_estimation_error.bits() > self.maximum_admitted_velocity.bits()
        {
            return Err(ServoEncoderProfileError::VelocityPolicy);
        }
        let maximum_observed_delta_counts = self.maximum_observed_delta_counts_inner()?;
        if u64::from(maximum_observed_delta_counts)
            .checked_mul(2)
            .ok_or(ServoEncoderProfileError::Arithmetic)?
            >= u64::from(self.counts_per_mechanical_turn)
        {
            return Err(ServoEncoderProfileError::AmbiguousWrapWindow {
                maximum_observed_delta_counts,
                counts_per_mechanical_turn: self.counts_per_mechanical_turn,
            });
        }
        Ok(())
    }

    /// Maximum integer difference between adjacent observed nominal counts.
    ///
    /// The bound is the configured trackable motion over one exact sample
    /// period plus the two observations' complete count uncertainty, rounded
    /// upward. Validation requires twice this value to be strictly below the
    /// modulus, so no two wrap candidates can both be admitted.
    pub fn maximum_observed_delta_counts(self) -> Result<u32, ServoEncoderProfileError> {
        self.validate()?;
        self.maximum_observed_delta_counts_inner()
    }

    fn maximum_observed_delta_counts_inner(self) -> Result<u32, ServoEncoderProfileError> {
        let error_numerator = u128::from(self.maximum_count_error.numerator());
        let error_denominator = u128::from(self.maximum_count_error.denominator());
        let q30_scale =
            u128::try_from(Q30_SCALE).map_err(|_| ServoEncoderProfileError::Arithmetic)?;
        let velocity_bits = u128::try_from(self.maximum_trackable_velocity.bits())
            .map_err(|_| ServoEncoderProfileError::VelocityPolicy)?;
        let rate_numerator = u128::from(self.scale.counts_per_second_at_velocity_one_numerator);
        let rate_denominator = u128::from(self.scale.counts_per_second_at_velocity_one_denominator);
        let device_cycle_hz = u128::from(self.device_cycle_hz);
        let sample_period_cycles = u128::from(self.sample_period_cycles);

        let motion_numerator = velocity_bits
            .checked_mul(rate_numerator)
            .and_then(|value| value.checked_mul(sample_period_cycles))
            .and_then(|value| value.checked_mul(error_denominator))
            .ok_or(ServoEncoderProfileError::Arithmetic)?;
        let uncertainty_numerator = error_numerator
            .checked_mul(2)
            .and_then(|value| value.checked_mul(q30_scale))
            .and_then(|value| value.checked_mul(rate_denominator))
            .and_then(|value| value.checked_mul(device_cycle_hz))
            .ok_or(ServoEncoderProfileError::Arithmetic)?;
        let denominator = q30_scale
            .checked_mul(rate_denominator)
            .and_then(|value| value.checked_mul(device_cycle_hz))
            .and_then(|value| value.checked_mul(error_denominator))
            .ok_or(ServoEncoderProfileError::Arithmetic)?;
        let numerator = motion_numerator
            .checked_add(uncertainty_numerator)
            .ok_or(ServoEncoderProfileError::Arithmetic)?;
        let bound =
            div_ceil_u128(numerator, denominator).ok_or(ServoEncoderProfileError::Arithmetic)?;
        u32::try_from(bound).map_err(|_| ServoEncoderProfileError::Arithmetic)
    }
}

/// Rejected portable encoder profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoEncoderProfileError {
    /// No nonzero immutable configuration identity was supplied.
    MissingConfiguration,
    /// The modulus or reference count was invalid.
    CountGeometry,
    /// Device-cycle frequency was zero.
    DeviceClock,
    /// Physical sample cadence was zero.
    SamplePeriod,
    /// An observation could become available more than one sample late.
    ObservationLatency,
    /// Admitted and trackable normalized velocities were not ordered and positive.
    VelocityPolicy,
    /// More than one modular count candidate could satisfy the tracking bound.
    AmbiguousWrapWindow {
        /// Motion-plus-observation bound for one exact sample period.
        maximum_observed_delta_counts: u32,
        /// Complete absolute-count modulus.
        counts_per_mechanical_turn: u32,
    },
    /// A checked profile calculation overflowed.
    Arithmetic,
}

/// One raw absolute count with truthful timing and configuration identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoEncoderObservation {
    pub configuration_digest: Digest,
    pub raw_count: u32,
    /// Exact device cycle represented by the raw count.
    pub sampled_at: DeviceCycle,
    /// Exact device cycle at which the complete observation became available.
    pub available_at: DeviceCycle,
}

/// Explicit boot-local multi-turn seed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoEncoderSeed {
    pub observation: ServoEncoderObservation,
    /// Directed mechanical turn containing the seed observation.
    pub turn_index: i64,
}

/// Retained position result for the seed observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoEncoderSeedState {
    pub raw_count: u32,
    pub directed_count: u32,
    pub unwrapped_count: i64,
    pub sampled_at: DeviceCycle,
    pub available_at: DeviceCycle,
    pub position: ServoPositionInterval,
}

/// One complete raw-to-servo estimation result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoEncoderEstimate {
    pub raw_count: u32,
    pub directed_count: u32,
    pub unwrapped_count: i64,
    pub delta_counts: i64,
    pub sample: ServoKinematicSample,
}

/// Allocation-free, fail-closed absolute-count estimator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoEncoderEstimator {
    profile: ServoEncoderProfile,
    maximum_observed_delta_counts: u32,
    seed: ServoEncoderSeedState,
    last_raw_count: u32,
    last_directed_count: u32,
    last_unwrapped_count: i64,
    last_sampled_at: DeviceCycle,
    last_position: ServoPositionInterval,
    estimate_sequence: u32,
    fault: Option<ServoEncoderError>,
}

impl ServoEncoderEstimator {
    /// Validates a profile and establishes an explicit multi-turn seed.
    ///
    /// The seed produces a bounded position but no velocity: two physical
    /// samples are required before a [`ServoKinematicSample`] can exist.
    pub fn new(
        profile: ServoEncoderProfile,
        seed: ServoEncoderSeed,
    ) -> Result<Self, ServoEncoderError> {
        profile.validate().map_err(ServoEncoderError::Profile)?;
        validate_observation(profile, seed.observation)?;
        let directed_count = directed_count(profile, seed.observation.raw_count);
        let unwrapped = i128::from(seed.turn_index)
            .checked_mul(i128::from(profile.counts_per_mechanical_turn))
            .and_then(|value| value.checked_add(i128::from(directed_count)))
            .ok_or(ServoEncoderError::CountOverflow)?;
        let unwrapped_count =
            i64::try_from(unwrapped).map_err(|_| ServoEncoderError::CountOverflow)?;
        let position = position_interval(profile, unwrapped_count)?;
        validate_position_width(profile, position)?;
        let seed_state = ServoEncoderSeedState {
            raw_count: seed.observation.raw_count,
            directed_count,
            unwrapped_count,
            sampled_at: seed.observation.sampled_at,
            available_at: seed.observation.available_at,
            position,
        };
        Ok(Self {
            profile,
            maximum_observed_delta_counts: profile
                .maximum_observed_delta_counts_inner()
                .map_err(ServoEncoderError::Profile)?,
            seed: seed_state,
            last_raw_count: seed_state.raw_count,
            last_directed_count: seed_state.directed_count,
            last_unwrapped_count: seed_state.unwrapped_count,
            last_sampled_at: seed_state.sampled_at,
            last_position: seed_state.position,
            estimate_sequence: 0,
            fault: None,
        })
    }

    /// Immutable validated estimator profile.
    pub const fn profile(&self) -> ServoEncoderProfile {
        self.profile
    }

    /// Exact integer wrap-selection bound retained at construction.
    pub const fn maximum_observed_delta_counts(&self) -> u32 {
        self.maximum_observed_delta_counts
    }

    /// Immutable boot-local seed state.
    pub const fn seed(&self) -> ServoEncoderSeedState {
        self.seed
    }

    /// Last accepted raw absolute count.
    pub const fn last_raw_count(&self) -> u32 {
        self.last_raw_count
    }

    /// Last accepted reference- and direction-adjusted count residue.
    pub const fn last_directed_count(&self) -> u32 {
        self.last_directed_count
    }

    /// Last accepted directed multi-turn nominal count.
    pub const fn last_unwrapped_count(&self) -> i64 {
        self.last_unwrapped_count
    }

    /// Last accepted conservative position enclosure.
    pub const fn last_position(&self) -> ServoPositionInterval {
        self.last_position
    }

    /// Number and identity of the last emitted kinematic sample.
    pub const fn estimate_sequence(&self) -> u32 {
        self.estimate_sequence
    }

    /// Exact retained first cause, if any.
    pub const fn fault(&self) -> Option<ServoEncoderError> {
        self.fault
    }

    /// Accepts one exact-cadence observation transactionally.
    ///
    /// On failure no count, timestamp, sequence, or position state advances;
    /// the exact first cause is retained and later calls return `FaultLatched`.
    pub fn observe(
        &mut self,
        observation: ServoEncoderObservation,
    ) -> Result<ServoEncoderEstimate, ServoEncoderError> {
        if self.fault.is_some() {
            return Err(ServoEncoderError::FaultLatched);
        }
        let mut next = *self;
        match next.observe_inner(observation) {
            Ok(estimate) => {
                *self = next;
                Ok(estimate)
            }
            Err(error) => {
                self.fault = Some(error);
                Err(error)
            }
        }
    }

    fn observe_inner(
        &mut self,
        observation: ServoEncoderObservation,
    ) -> Result<ServoEncoderEstimate, ServoEncoderError> {
        validate_observation(self.profile, observation)?;
        let expected_sampled_at = DeviceCycle(
            self.last_sampled_at
                .0
                .checked_add(self.profile.sample_period_cycles)
                .ok_or(ServoEncoderError::SampleCycleOverflow)?,
        );
        if observation.sampled_at != expected_sampled_at {
            return Err(ServoEncoderError::SampleCycle {
                expected: expected_sampled_at,
                received: observation.sampled_at,
            });
        }
        let directed_count = directed_count(self.profile, observation.raw_count);
        let delta_counts = select_delta(
            self.last_directed_count,
            directed_count,
            self.profile.counts_per_mechanical_turn,
            self.maximum_observed_delta_counts,
        )?;
        let unwrapped_count = self
            .last_unwrapped_count
            .checked_add(delta_counts)
            .ok_or(ServoEncoderError::CountOverflow)?;
        let position = position_interval(self.profile, unwrapped_count)?;
        validate_position_width(self.profile, position)?;
        let velocity = velocity_interval(self.profile, delta_counts)?;
        validate_velocity(self.profile, velocity)?;
        let sequence = self
            .estimate_sequence
            .checked_add(1)
            .ok_or(ServoEncoderError::SequenceOverflow)?;
        let sample = ServoKinematicSample {
            sequence,
            sampled_at: observation.sampled_at,
            available_at: observation.available_at,
            configuration_digest: self.profile.configuration_digest,
            position,
            velocity,
        };

        self.last_raw_count = observation.raw_count;
        self.last_directed_count = directed_count;
        self.last_unwrapped_count = unwrapped_count;
        self.last_sampled_at = observation.sampled_at;
        self.last_position = position;
        self.estimate_sequence = sequence;
        Ok(ServoEncoderEstimate {
            raw_count: observation.raw_count,
            directed_count,
            unwrapped_count,
            delta_counts,
            sample,
        })
    }
}

/// First-cause initialization or runtime rejection from the estimator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoEncoderError {
    /// Construction rejected the immutable profile.
    Profile(ServoEncoderProfileError),
    /// An observation named another immutable configuration.
    Configuration { expected: Digest, received: Digest },
    /// A raw count was outside the profile's absolute-count modulus.
    RawCount {
        received: u32,
        counts_per_mechanical_turn: u32,
    },
    /// Availability preceded the physical instant represented by the count.
    ObservationWindow {
        sampled_at: DeviceCycle,
        available_at: DeviceCycle,
    },
    /// Complete observation latency exceeded policy.
    ObservationLatency {
        received_cycles: u64,
        maximum_cycles: u64,
    },
    /// A new physical sample was not on the exact retained cadence.
    SampleCycle {
        expected: DeviceCycle,
        received: DeviceCycle,
    },
    /// The next required physical sample cycle overflowed.
    SampleCycleOverflow,
    /// Neither modular count difference satisfied the unique tracking bound.
    CountDelta {
        positive_candidate: u32,
        negative_candidate: i64,
        maximum_absolute: u32,
    },
    /// Both modular count differences satisfied a supposedly unique profile.
    CountDeltaAmbiguous,
    /// Directed multi-turn count state overflowed.
    CountOverflow,
    /// Exact position mapping did not fit the Q31.32 storage lattice.
    PositionRange,
    /// A conservative position enclosure exceeded its ULP policy.
    PositionPrecision {
        width_ulps: u64,
        maximum_width_ulps: u64,
    },
    /// A conservative velocity enclosure exceeded the admitted speed.
    VelocityRange {
        lower_bits: i128,
        upper_bits: i128,
        maximum_absolute_bits: i32,
    },
    /// A conservative velocity enclosure exceeded its ULP policy.
    VelocityPrecision {
        width_ulps: u64,
        maximum_width_ulps: u32,
    },
    /// The contiguous kinematic-sample identity overflowed.
    SequenceOverflow,
    /// Checked exact intermediate arithmetic overflowed.
    Arithmetic,
    /// A prior exact first cause remains latched.
    FaultLatched,
}

fn validate_observation(
    profile: ServoEncoderProfile,
    observation: ServoEncoderObservation,
) -> Result<(), ServoEncoderError> {
    if observation.configuration_digest != profile.configuration_digest {
        return Err(ServoEncoderError::Configuration {
            expected: profile.configuration_digest,
            received: observation.configuration_digest,
        });
    }
    if observation.raw_count >= profile.counts_per_mechanical_turn {
        return Err(ServoEncoderError::RawCount {
            received: observation.raw_count,
            counts_per_mechanical_turn: profile.counts_per_mechanical_turn,
        });
    }
    let latency = observation
        .available_at
        .0
        .checked_sub(observation.sampled_at.0)
        .ok_or(ServoEncoderError::ObservationWindow {
            sampled_at: observation.sampled_at,
            available_at: observation.available_at,
        })?;
    if latency > profile.maximum_observation_latency_cycles {
        return Err(ServoEncoderError::ObservationLatency {
            received_cycles: latency,
            maximum_cycles: profile.maximum_observation_latency_cycles,
        });
    }
    Ok(())
}

fn directed_count(profile: ServoEncoderProfile, raw_count: u32) -> u32 {
    let modulus = u64::from(profile.counts_per_mechanical_turn);
    let increasing =
        (u64::from(raw_count) + modulus - u64::from(profile.count_at_reference)) % modulus;
    let directed = match profile.direction {
        RotorCountDirection::Increasing => increasing,
        RotorCountDirection::Decreasing => (modulus - increasing) % modulus,
    };
    directed as u32
}

fn select_delta(
    prior: u32,
    current: u32,
    modulus: u32,
    maximum_absolute: u32,
) -> Result<i64, ServoEncoderError> {
    let modulus_u64 = u64::from(modulus);
    let positive = (u64::from(current) + modulus_u64 - u64::from(prior)) % modulus_u64;
    let positive = i64::try_from(positive).map_err(|_| ServoEncoderError::Arithmetic)?;
    let negative = positive - i64::from(modulus);
    let positive_admitted = positive <= i64::from(maximum_absolute);
    let negative_admitted = negative.unsigned_abs() <= u64::from(maximum_absolute);
    match (positive_admitted, negative_admitted) {
        (true, false) => Ok(positive),
        (false, true) => Ok(negative),
        (false, false) => Err(ServoEncoderError::CountDelta {
            positive_candidate: u32::try_from(positive)
                .map_err(|_| ServoEncoderError::Arithmetic)?,
            negative_candidate: negative,
            maximum_absolute,
        }),
        (true, true) => Err(ServoEncoderError::CountDeltaAmbiguous),
    }
}

fn position_interval(
    profile: ServoEncoderProfile,
    unwrapped_count: i64,
) -> Result<ServoPositionInterval, ServoEncoderError> {
    let error_numerator = i128::from(profile.maximum_count_error.numerator());
    let error_denominator = i128::from(profile.maximum_count_error.denominator());
    let center = i128::from(unwrapped_count)
        .checked_mul(error_denominator)
        .ok_or(ServoEncoderError::Arithmetic)?;
    let lower_count = center
        .checked_sub(error_numerator)
        .ok_or(ServoEncoderError::Arithmetic)?;
    let upper_count = center
        .checked_add(error_numerator)
        .ok_or(ServoEncoderError::Arithmetic)?;
    let scale_numerator = i128::from(profile.scale.position_bits_per_turn_numerator);
    let denominator = error_denominator
        .checked_mul(i128::from(profile.counts_per_mechanical_turn))
        .and_then(|value| {
            value.checked_mul(i128::from(profile.scale.position_bits_per_turn_denominator))
        })
        .ok_or(ServoEncoderError::Arithmetic)?;
    let lower_delta = lower_count
        .checked_mul(scale_numerator)
        .map(|value| div_floor(value, denominator))
        .ok_or(ServoEncoderError::Arithmetic)?;
    let upper_delta = upper_count
        .checked_mul(scale_numerator)
        .map(|value| div_ceil(value, denominator))
        .ok_or(ServoEncoderError::Arithmetic)?;
    let reference = i128::from(profile.position_at_reference.bits());
    let lower = reference
        .checked_add(lower_delta)
        .ok_or(ServoEncoderError::Arithmetic)?;
    let upper = reference
        .checked_add(upper_delta)
        .ok_or(ServoEncoderError::Arithmetic)?;
    let lower = i64::try_from(lower).map_err(|_| ServoEncoderError::PositionRange)?;
    let upper = i64::try_from(upper).map_err(|_| ServoEncoderError::PositionRange)?;
    ServoPositionInterval::new(
        ServoPosition::from_bits(lower),
        ServoPosition::from_bits(upper),
    )
    .map_err(|_| ServoEncoderError::Arithmetic)
}

fn validate_position_width(
    profile: ServoEncoderProfile,
    position: ServoPositionInterval,
) -> Result<(), ServoEncoderError> {
    let width = position.width_ulps();
    if width > profile.maximum_position_interval_width_ulps {
        Err(ServoEncoderError::PositionPrecision {
            width_ulps: width,
            maximum_width_ulps: profile.maximum_position_interval_width_ulps,
        })
    } else {
        Ok(())
    }
}

fn velocity_interval(
    profile: ServoEncoderProfile,
    delta_counts: i64,
) -> Result<Q30Interval, ServoEncoderError> {
    let error_numerator = i128::from(profile.maximum_count_error.numerator());
    let error_denominator = i128::from(profile.maximum_count_error.denominator());
    let center = i128::from(delta_counts)
        .checked_mul(error_denominator)
        .ok_or(ServoEncoderError::Arithmetic)?;
    let complete_error = error_numerator
        .checked_mul(2)
        .ok_or(ServoEncoderError::Arithmetic)?;
    let lower_count = center
        .checked_sub(complete_error)
        .ok_or(ServoEncoderError::Arithmetic)?;
    let upper_count = center
        .checked_add(complete_error)
        .ok_or(ServoEncoderError::Arithmetic)?;
    let numerator_scale = i128::from(profile.device_cycle_hz)
        .checked_mul(i128::from(
            profile.scale.counts_per_second_at_velocity_one_denominator,
        ))
        .and_then(|value| value.checked_mul(i128::from(Q30_SCALE)))
        .ok_or(ServoEncoderError::Arithmetic)?;
    let denominator = error_denominator
        .checked_mul(i128::from(profile.sample_period_cycles))
        .and_then(|value| {
            value.checked_mul(i128::from(
                profile.scale.counts_per_second_at_velocity_one_numerator,
            ))
        })
        .ok_or(ServoEncoderError::Arithmetic)?;
    let lower_bits = lower_count
        .checked_mul(numerator_scale)
        .map(|value| div_floor(value, denominator))
        .ok_or(ServoEncoderError::Arithmetic)?;
    let upper_bits = upper_count
        .checked_mul(numerator_scale)
        .map(|value| div_ceil(value, denominator))
        .ok_or(ServoEncoderError::Arithmetic)?;
    let lower_bits = lower_bits
        .checked_sub(i128::from(profile.maximum_velocity_estimation_error.bits()))
        .ok_or(ServoEncoderError::Arithmetic)?;
    let upper_bits = upper_bits
        .checked_add(i128::from(profile.maximum_velocity_estimation_error.bits()))
        .ok_or(ServoEncoderError::Arithmetic)?;
    let maximum = i128::from(profile.maximum_admitted_velocity.bits());
    if lower_bits < -maximum || upper_bits > maximum {
        return Err(ServoEncoderError::VelocityRange {
            lower_bits,
            upper_bits,
            maximum_absolute_bits: profile.maximum_admitted_velocity.bits(),
        });
    }
    let lower = i32::try_from(lower_bits).map_err(|_| ServoEncoderError::Arithmetic)?;
    let upper = i32::try_from(upper_bits).map_err(|_| ServoEncoderError::Arithmetic)?;
    Q30Interval::new(Q30::from_bits(lower), Q30::from_bits(upper))
        .map_err(|_| ServoEncoderError::Arithmetic)
}

fn validate_velocity(
    profile: ServoEncoderProfile,
    velocity: Q30Interval,
) -> Result<(), ServoEncoderError> {
    let width = velocity.width_ulps();
    if width > u64::from(profile.maximum_velocity_interval_width_ulps) {
        Err(ServoEncoderError::VelocityPrecision {
            width_ulps: width,
            maximum_width_ulps: profile.maximum_velocity_interval_width_ulps,
        })
    } else {
        Ok(())
    }
}

const fn div_ceil_u128(numerator: u128, denominator: u128) -> Option<u128> {
    if denominator == 0 {
        return None;
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    if remainder == 0 {
        Some(quotient)
    } else {
        quotient.checked_add(1)
    }
}

const fn gcd_u64(mut left: u64, mut right: u64) -> u64 {
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

    const DIGEST: Digest = Digest([0x6d; 32]);
    const POSITION_BITS_PER_COUNT: i64 = 1_i64 << 20;
    const POSITION_WIDTH: u64 = 1_u64 << 20;
    const VELOCITY_BITS_PER_COUNT: i32 = 131_072_000;
    const VELOCITY_ESTIMATION_ERROR: i32 = 1 << 20;
    const VELOCITY_WIDTH: u32 = 264_241_152;

    fn scale() -> ServoEncoderScale {
        ServoEncoderScale::new(1_u64 << 32, 1, 8_192, 1).unwrap()
    }

    fn profile(direction: RotorCountDirection) -> ServoEncoderProfile {
        ServoEncoderProfile {
            configuration_digest: DIGEST,
            counts_per_mechanical_turn: 4_096,
            count_at_reference: 0,
            direction,
            maximum_count_error: CountUncertainty::new(1, 2).unwrap(),
            position_at_reference: ServoPosition::ZERO,
            scale: scale(),
            device_cycle_hz: 1_000_000,
            sample_period_cycles: 1_000,
            maximum_observation_latency_cycles: 100,
            maximum_trackable_velocity: Q30::ONE,
            maximum_admitted_velocity: Q30::ONE,
            maximum_velocity_estimation_error: Q30::from_bits(VELOCITY_ESTIMATION_ERROR),
            maximum_position_interval_width_ulps: POSITION_WIDTH,
            maximum_velocity_interval_width_ulps: VELOCITY_WIDTH,
        }
    }

    fn observation(raw_count: u32, sampled_at: u64) -> ServoEncoderObservation {
        ServoEncoderObservation {
            configuration_digest: DIGEST,
            raw_count,
            sampled_at: DeviceCycle(sampled_at),
            available_at: DeviceCycle(sampled_at + 50),
        }
    }

    fn estimator(raw_count: u32) -> ServoEncoderEstimator {
        ServoEncoderEstimator::new(
            profile(RotorCountDirection::Increasing),
            ServoEncoderSeed {
                observation: observation(raw_count, 1_000),
                turn_index: 0,
            },
        )
        .unwrap()
    }

    #[test]
    fn scales_are_positive_and_canonical_and_wrap_window_is_exact() {
        assert_eq!(
            ServoEncoderScale::new(0, 1, 1, 1),
            Err(ServoEncoderScaleError::Zero)
        );
        assert_eq!(
            ServoEncoderScale::new(2, 2, 1, 1),
            Err(ServoEncoderScaleError::NonCanonical)
        );
        let valid = profile(RotorCountDirection::Increasing);
        assert_eq!(valid.maximum_observed_delta_counts(), Ok(10));

        let mut ambiguous = valid;
        ambiguous.scale = ServoEncoderScale::new(1_u64 << 32, 1, 3_000_000, 1).unwrap();
        assert_eq!(
            ambiguous.validate(),
            Err(ServoEncoderProfileError::AmbiguousWrapWindow {
                maximum_observed_delta_counts: 3_001,
                counts_per_mechanical_turn: 4_096,
            })
        );

        let mut bad_velocity = valid;
        bad_velocity.maximum_admitted_velocity = Q30::ZERO;
        assert_eq!(
            bad_velocity.validate(),
            Err(ServoEncoderProfileError::VelocityPolicy)
        );
        bad_velocity.maximum_admitted_velocity = Q30::ONE;
        bad_velocity.maximum_trackable_velocity = Q30::HALF;
        assert_eq!(
            bad_velocity.validate(),
            Err(ServoEncoderProfileError::VelocityPolicy)
        );
        bad_velocity.maximum_trackable_velocity = Q30::ONE;
        bad_velocity.maximum_velocity_estimation_error = Q30::ZERO;
        assert_eq!(
            bad_velocity.validate(),
            Err(ServoEncoderProfileError::VelocityPolicy)
        );
    }

    #[test]
    fn explicit_seed_retains_position_but_withholds_velocity() {
        let estimator = estimator(4_094);
        assert_eq!(estimator.maximum_observed_delta_counts(), 10);
        assert_eq!(estimator.estimate_sequence(), 0);
        assert_eq!(estimator.seed().directed_count, 4_094);
        assert_eq!(estimator.seed().unwrapped_count, 4_094);
        assert_eq!(
            estimator.seed().position,
            ServoPositionInterval::new(
                ServoPosition::from_bits(
                    4_094 * POSITION_BITS_PER_COUNT - POSITION_BITS_PER_COUNT / 2
                ),
                ServoPosition::from_bits(
                    4_094 * POSITION_BITS_PER_COUNT + POSITION_BITS_PER_COUNT / 2
                ),
            )
            .unwrap()
        );
        assert_eq!(estimator.fault(), None);
    }

    #[test]
    fn forward_wrap_produces_outward_position_and_velocity_intervals() {
        let mut estimator = estimator(4_094);
        let estimate = estimator.observe(observation(2, 2_000)).unwrap();
        assert_eq!(estimate.raw_count, 2);
        assert_eq!(estimate.directed_count, 2);
        assert_eq!(estimate.delta_counts, 4);
        assert_eq!(estimate.unwrapped_count, 4_098);
        assert_eq!(estimate.sample.sequence, 1);
        assert_eq!(estimate.sample.sampled_at, DeviceCycle(2_000));
        assert_eq!(
            estimate.sample.position,
            ServoPositionInterval::new(
                ServoPosition::from_bits(
                    4_098 * POSITION_BITS_PER_COUNT - POSITION_BITS_PER_COUNT / 2
                ),
                ServoPosition::from_bits(
                    4_098 * POSITION_BITS_PER_COUNT + POSITION_BITS_PER_COUNT / 2
                ),
            )
            .unwrap()
        );
        assert_eq!(
            estimate.sample.velocity,
            Q30Interval::new(
                Q30::from_bits(3 * VELOCITY_BITS_PER_COUNT - VELOCITY_ESTIMATION_ERROR),
                Q30::from_bits(5 * VELOCITY_BITS_PER_COUNT + VELOCITY_ESTIMATION_ERROR),
            )
            .unwrap()
        );
        assert_eq!(
            estimate.sample.velocity.width_ulps(),
            u64::from(VELOCITY_WIDTH)
        );

        let second = estimator.observe(observation(5, 3_000)).unwrap();
        assert_eq!(second.delta_counts, 3);
        assert_eq!(second.unwrapped_count, 4_101);
        assert_eq!(second.sample.sequence, 2);
    }

    #[test]
    fn decreasing_raw_counts_map_to_positive_axis_motion_across_wrap() {
        let mut estimator = ServoEncoderEstimator::new(
            profile(RotorCountDirection::Decreasing),
            ServoEncoderSeed {
                observation: observation(2, 1_000),
                turn_index: 0,
            },
        )
        .unwrap();
        assert_eq!(estimator.seed().directed_count, 4_094);
        let estimate = estimator.observe(observation(4_094, 2_000)).unwrap();
        assert_eq!(estimate.directed_count, 2);
        assert_eq!(estimate.delta_counts, 4);
        assert_eq!(estimate.unwrapped_count, 4_098);
    }

    #[test]
    fn negative_motion_selects_the_only_admitted_modular_candidate() {
        let mut estimator = estimator(2);
        let estimate = estimator.observe(observation(4_094, 2_000)).unwrap();
        assert_eq!(estimate.delta_counts, -4);
        assert_eq!(estimate.unwrapped_count, -2);
        assert_eq!(
            estimate.sample.velocity,
            Q30Interval::new(
                Q30::from_bits(-5 * VELOCITY_BITS_PER_COUNT - VELOCITY_ESTIMATION_ERROR),
                Q30::from_bits(-3 * VELOCITY_BITS_PER_COUNT + VELOCITY_ESTIMATION_ERROR),
            )
            .unwrap()
        );
    }

    #[test]
    fn non_power_of_two_negative_position_and_velocity_bounds_are_outward() {
        let position_numerator = 5_u64 * (1_u64 << 32);
        let profile = ServoEncoderProfile {
            configuration_digest: DIGEST,
            counts_per_mechanical_turn: 12,
            count_at_reference: 3,
            direction: RotorCountDirection::Increasing,
            maximum_count_error: CountUncertainty::new(1, 3).unwrap(),
            position_at_reference: ServoPosition::from_bits(123),
            scale: ServoEncoderScale::new(position_numerator, 3, 30, 1).unwrap(),
            device_cycle_hz: 1_000,
            sample_period_cycles: 100,
            maximum_observation_latency_cycles: 10,
            maximum_trackable_velocity: Q30::ONE,
            maximum_admitted_velocity: Q30::ONE,
            maximum_velocity_estimation_error: Q30::from_bits(7),
            maximum_position_interval_width_ulps: u64::MAX,
            maximum_velocity_interval_width_ulps: u32::MAX,
        };
        assert_eq!(profile.maximum_observed_delta_counts(), Ok(4));
        let mut estimator = ServoEncoderEstimator::new(
            profile,
            ServoEncoderSeed {
                observation: ServoEncoderObservation {
                    configuration_digest: DIGEST,
                    raw_count: 3,
                    sampled_at: DeviceCycle(100),
                    available_at: DeviceCycle(100),
                },
                turn_index: -1,
            },
        )
        .unwrap();
        let estimate = estimator
            .observe(ServoEncoderObservation {
                configuration_digest: DIGEST,
                raw_count: 4,
                sampled_at: DeviceCycle(200),
                available_at: DeviceCycle(200),
            })
            .unwrap();
        assert_eq!(estimate.unwrapped_count, -11);
        assert_eq!(estimate.delta_counts, 1);

        let position_denominator = 3_i128 * 12 * 3;
        let reference = 123_i128 * position_denominator;
        let lower_position_numerator =
            reference + (-11_i128 * 3 - 1) * i128::from(position_numerator);
        let upper_position_numerator =
            reference + (-11_i128 * 3 + 1) * i128::from(position_numerator);
        let lower_position = i128::from(estimate.sample.position.lower().bits());
        let upper_position = i128::from(estimate.sample.position.upper().bits());
        assert!(lower_position * position_denominator <= lower_position_numerator);
        assert!((lower_position + 1) * position_denominator > lower_position_numerator);
        assert!((upper_position - 1) * position_denominator < upper_position_numerator);
        assert!(upper_position * position_denominator >= upper_position_numerator);

        let velocity_denominator = 3_i128 * 100 * 30;
        let lower_velocity_numerator = 1_000_i128 * i128::from(Q30_SCALE);
        let upper_velocity_numerator = 5_000_i128 * i128::from(Q30_SCALE);
        let lower_velocity = i128::from(estimate.sample.velocity.lower().bits()) + 7;
        let upper_velocity = i128::from(estimate.sample.velocity.upper().bits()) - 7;
        assert!(lower_velocity * velocity_denominator <= lower_velocity_numerator);
        assert!((lower_velocity + 1) * velocity_denominator > lower_velocity_numerator);
        assert!((upper_velocity - 1) * velocity_denominator < upper_velocity_numerator);
        assert!(upper_velocity * velocity_denominator >= upper_velocity_numerator);
    }

    #[test]
    fn missing_wrap_candidate_is_terminal_and_transactional() {
        let mut estimator = estimator(0);
        let before = estimator;
        assert_eq!(
            estimator.observe(observation(100, 2_000)),
            Err(ServoEncoderError::CountDelta {
                positive_candidate: 100,
                negative_candidate: -3_996,
                maximum_absolute: 10,
            })
        );
        assert_eq!(
            estimator.last_unwrapped_count(),
            before.last_unwrapped_count()
        );
        assert_eq!(estimator.estimate_sequence(), 0);
        assert_eq!(
            estimator.fault(),
            Some(ServoEncoderError::CountDelta {
                positive_candidate: 100,
                negative_candidate: -3_996,
                maximum_absolute: 10,
            })
        );
        assert_eq!(
            estimator.observe(observation(1, 2_000)),
            Err(ServoEncoderError::FaultLatched)
        );
    }

    #[test]
    fn identity_window_latency_and_exact_cadence_are_rejected() {
        let mut foreign = observation(0, 1_000);
        foreign.configuration_digest = Digest([0x44; 32]);
        assert_eq!(
            ServoEncoderEstimator::new(
                profile(RotorCountDirection::Increasing),
                ServoEncoderSeed {
                    observation: foreign,
                    turn_index: 0,
                }
            ),
            Err(ServoEncoderError::Configuration {
                expected: DIGEST,
                received: Digest([0x44; 32]),
            })
        );

        assert_eq!(
            ServoEncoderEstimator::new(
                profile(RotorCountDirection::Increasing),
                ServoEncoderSeed {
                    observation: observation(4_096, 1_000),
                    turn_index: 0,
                }
            ),
            Err(ServoEncoderError::RawCount {
                received: 4_096,
                counts_per_mechanical_turn: 4_096,
            })
        );

        let mut reversed = observation(0, 1_000);
        reversed.available_at = DeviceCycle(999);
        assert!(matches!(
            ServoEncoderEstimator::new(
                profile(RotorCountDirection::Increasing),
                ServoEncoderSeed {
                    observation: reversed,
                    turn_index: 0,
                }
            ),
            Err(ServoEncoderError::ObservationWindow { .. })
        ));

        let mut late = observation(0, 1_000);
        late.available_at = DeviceCycle(1_101);
        assert_eq!(
            ServoEncoderEstimator::new(
                profile(RotorCountDirection::Increasing),
                ServoEncoderSeed {
                    observation: late,
                    turn_index: 0,
                }
            ),
            Err(ServoEncoderError::ObservationLatency {
                received_cycles: 101,
                maximum_cycles: 100,
            })
        );

        let mut estimator = estimator(0);
        assert_eq!(
            estimator.observe(observation(1, 2_001)),
            Err(ServoEncoderError::SampleCycle {
                expected: DeviceCycle(2_000),
                received: DeviceCycle(2_001),
            })
        );
        assert_eq!(estimator.last_unwrapped_count(), 0);
    }

    #[test]
    fn precision_and_range_policies_fail_before_state_advances() {
        let mut narrow_position = profile(RotorCountDirection::Increasing);
        narrow_position.maximum_position_interval_width_ulps = POSITION_WIDTH - 1;
        assert_eq!(
            ServoEncoderEstimator::new(
                narrow_position,
                ServoEncoderSeed {
                    observation: observation(0, 1_000),
                    turn_index: 0,
                }
            ),
            Err(ServoEncoderError::PositionPrecision {
                width_ulps: POSITION_WIDTH,
                maximum_width_ulps: POSITION_WIDTH - 1,
            })
        );

        let mut narrow_velocity = profile(RotorCountDirection::Increasing);
        narrow_velocity.maximum_velocity_interval_width_ulps = VELOCITY_WIDTH - 1;
        let mut narrow_estimator = ServoEncoderEstimator::new(
            narrow_velocity,
            ServoEncoderSeed {
                observation: observation(0, 1_000),
                turn_index: 0,
            },
        )
        .unwrap();
        assert_eq!(
            narrow_estimator.observe(observation(4, 2_000)),
            Err(ServoEncoderError::VelocityPrecision {
                width_ulps: u64::from(VELOCITY_WIDTH),
                maximum_width_ulps: VELOCITY_WIDTH - 1,
            })
        );
        assert_eq!(narrow_estimator.estimate_sequence(), 0);

        let mut overspeed = estimator(0);
        assert!(matches!(
            overspeed.observe(observation(8, 2_000)),
            Err(ServoEncoderError::VelocityRange { .. })
        ));
        assert_eq!(overspeed.last_unwrapped_count(), 0);
    }

    #[test]
    fn count_position_cycle_and_sequence_overflow_are_explicit() {
        assert_eq!(
            ServoEncoderEstimator::new(
                profile(RotorCountDirection::Increasing),
                ServoEncoderSeed {
                    observation: observation(0, 1_000),
                    turn_index: i64::MAX,
                }
            ),
            Err(ServoEncoderError::CountOverflow)
        );

        let mut position_overflow = profile(RotorCountDirection::Increasing);
        position_overflow.position_at_reference = ServoPosition::from_bits(i64::MAX);
        assert_eq!(
            ServoEncoderEstimator::new(
                position_overflow,
                ServoEncoderSeed {
                    observation: observation(1, 1_000),
                    turn_index: 0,
                }
            ),
            Err(ServoEncoderError::PositionRange)
        );

        let mut cycle_overflow = ServoEncoderEstimator::new(
            profile(RotorCountDirection::Increasing),
            ServoEncoderSeed {
                observation: ServoEncoderObservation {
                    configuration_digest: DIGEST,
                    raw_count: 0,
                    sampled_at: DeviceCycle(u64::MAX - 500),
                    available_at: DeviceCycle(u64::MAX - 500),
                },
                turn_index: 0,
            },
        )
        .unwrap();
        assert_eq!(
            cycle_overflow.observe(ServoEncoderObservation {
                configuration_digest: DIGEST,
                raw_count: 0,
                sampled_at: DeviceCycle(u64::MAX),
                available_at: DeviceCycle(u64::MAX),
            }),
            Err(ServoEncoderError::SampleCycleOverflow)
        );

        let mut sequence_overflow = estimator(0);
        sequence_overflow.estimate_sequence = u32::MAX;
        assert_eq!(
            sequence_overflow.observe(observation(1, 2_000)),
            Err(ServoEncoderError::SequenceOverflow)
        );
        assert_eq!(sequence_overflow.last_unwrapped_count(), 0);
    }
}
