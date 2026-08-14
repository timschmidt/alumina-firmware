#![no_std]
#![doc = "Canonical bounded integer work accepted by an Alumina real-time executor."]

use core::fmt;

use alumina_protocol::{DeviceCycle, Digest};
use sha2::{Digest as _, Sha256};

mod servo;

pub use servo::*;

/// Magic identifying a per-MCU Alumina machine-IR partition.
pub const JOB_MAGIC: [u8; 4] = *b"AJOB";

/// Exact machine-IR schema implemented here.
pub const MACHINE_IR_VERSION: u16 = 3;

/// Exact bytes in one independently owned core-0-to-core-1 work block.
pub const EXECUTION_BLOCK_BYTES: usize = 512;
/// Fixed header bytes preceding motion records.
pub const EXECUTION_BLOCK_HEADER_BYTES: usize = 160;
/// Final SHA-256 digest offset and usable end of the padded payload.
pub const EXECUTION_BLOCK_DIGEST_OFFSET: usize = EXECUTION_BLOCK_BYTES - 32;
/// Maximum canonical record bytes after the header and before the digest.
pub const EXECUTION_BLOCK_PAYLOAD_BYTES: usize =
    EXECUTION_BLOCK_DIGEST_OFFSET - EXECUTION_BLOCK_HEADER_BYTES;
/// Largest axis vector admitted by execution-block schema V3.
pub const MAX_EXECUTION_AXES: usize = 8;

const BLOCK_MAGIC: [u8; 8] = *b"ALMBLK03";
const MOTION_RECORD_PREFIX_BYTES: usize = 16;
const FINITE_DIFFERENCE_RECORD_PREFIX_BYTES: usize = 16;
const FINITE_DIFFERENCE_AXIS_BYTES: usize = 32;

/// A fixed job-partition prefix validated before arming.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct JobHeader {
    /// Constant [`JOB_MAGIC`].
    pub magic: [u8; 4],
    /// Must equal [`MACHINE_IR_VERSION`].
    pub version: u16,
    /// Number of coordinate axes encoded by each segment.
    pub axis_count: u8,
    /// Reserved flags; the current schema requires zero.
    pub flags: u8,
    /// Number of following [`Segment`] values.
    pub segment_count: u32,
    /// Exact board capability set used by the compiler.
    pub capability_digest: Digest,
    /// Exact active machine configuration used by the compiler.
    pub config_digest: Digest,
    /// Digest of the immutable per-MCU partition bytes.
    pub partition_digest: Digest,
}

impl JobHeader {
    /// Constructs a current-schema header. A serializer fills
    /// `partition_digest` after encoding.
    pub const fn new(axis_count: u8, segment_count: u32) -> Self {
        Self {
            magic: JOB_MAGIC,
            version: MACHINE_IR_VERSION,
            axis_count,
            flags: 0,
            segment_count,
            capability_digest: Digest::ZERO,
            config_digest: Digest::ZERO,
            partition_digest: Digest::ZERO,
        }
    }
}

/// Smallest auditable coordinated integer-motion segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Segment<const AXES: usize> {
    /// Inclusive local-device start tick.
    pub start_cycle: DeviceCycle,
    /// Exclusive local-device end tick.
    pub end_cycle: DeviceCycle,
    /// Signed commanded lattice displacement for each axis.
    pub delta_steps: [i64; AXES],
    /// Reserved segment flags; must be zero.
    pub flags: u32,
}

/// Board/config-derived limits used for bounded firmware validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidationLimits {
    /// Longest accepted segment duration in device ticks.
    pub maximum_segment_ticks: u64,
    /// Largest absolute step displacement in one segment on any axis.
    pub maximum_steps_per_segment: u64,
}

/// Borrowed machine partition after core-0 decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Job<'a, const AXES: usize> {
    /// Canonical partition header.
    pub header: JobHeader,
    /// Fixed-axis coordinated segments.
    pub segments: &'a [Segment<AXES>],
}

impl<const AXES: usize> Job<'_, AXES> {
    /// Validates structural, identity, time, displacement, and overflow invariants.
    pub fn validate(&self, limits: ValidationLimits) -> Result<JobSummary<AXES>, Error> {
        if self.header.magic != JOB_MAGIC {
            return Err(Error::Magic);
        }
        if self.header.version != MACHINE_IR_VERSION {
            return Err(Error::Version {
                received: self.header.version,
            });
        }
        if usize::from(self.header.axis_count) != AXES || AXES == 0 {
            return Err(Error::AxisCount {
                encoded: self.header.axis_count,
                expected: AXES,
            });
        }
        if self.header.flags != 0 {
            return Err(Error::HeaderFlags(self.header.flags));
        }
        if usize::try_from(self.header.segment_count).ok() != Some(self.segments.len()) {
            return Err(Error::SegmentCount {
                encoded: self.header.segment_count,
                actual: self.segments.len(),
            });
        }
        if self.header.capability_digest.is_zero() {
            return Err(Error::MissingCapabilityDigest);
        }
        if self.header.config_digest.is_zero() {
            return Err(Error::MissingConfigDigest);
        }
        if self.header.partition_digest.is_zero() {
            return Err(Error::MissingPartitionDigest);
        }

        let mut end_cycle = DeviceCycle(0);
        let mut final_steps = [0_i64; AXES];
        let mut index = 0;
        while index < self.segments.len() {
            let segment = self.segments[index];
            if segment.flags != 0 {
                return Err(Error::SegmentFlags {
                    index,
                    flags: segment.flags,
                });
            }
            if segment.end_cycle.0 <= segment.start_cycle.0 {
                return Err(Error::EmptyOrReversedTime { index });
            }
            if index != 0 && segment.start_cycle != end_cycle {
                return Err(Error::NonContiguousTime { index });
            }
            let duration = segment.end_cycle.0 - segment.start_cycle.0;
            if duration > limits.maximum_segment_ticks {
                return Err(Error::SegmentTooLong { index, duration });
            }

            let mut axis = 0;
            while axis < AXES {
                let delta = segment.delta_steps[axis];
                let magnitude = delta.unsigned_abs();
                if magnitude > limits.maximum_steps_per_segment {
                    return Err(Error::TooManySteps {
                        index,
                        axis,
                        magnitude,
                    });
                }
                final_steps[axis] = final_steps[axis]
                    .checked_add(delta)
                    .ok_or(Error::PositionOverflow { index, axis })?;
                axis += 1;
            }
            end_cycle = segment.end_cycle;
            index += 1;
        }

        Ok(JobSummary {
            end_cycle,
            final_steps,
        })
    }
}

/// Facts established by validation and useful for arming diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobSummary<const AXES: usize> {
    /// Exclusive end tick, or zero for an empty job.
    pub end_cycle: DeviceCycle,
    /// Final relative lattice displacement on every axis.
    pub final_steps: [i64; AXES],
}

/// Nonzero identity shared by every block in one per-MCU execution stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct StreamId(pub [u8; 16]);

impl StreamId {
    /// Creates an identity while rejecting the all-zero unbound sentinel.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, BlockError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(BlockError::MissingStreamId)
    }

    const fn is_valid(self) -> bool {
        let mut index = 0;
        while index < self.0.len() {
            if self.0[index] != 0 {
                return true;
            }
            index += 1;
        }
        false
    }
}

/// Stream-relative tick offset in the target device clock's declared units.
///
/// A later deterministic commit supplies the absolute [`DeviceCycle`] epoch.
/// Keeping the types distinct prevents cached bytes from arming themselves.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct StreamTick(pub u64);

impl StreamTick {
    /// Adds this relative tick to a committed absolute device epoch.
    pub fn at_epoch(self, epoch: DeviceCycle) -> Result<DeviceCycle, BlockError> {
        epoch
            .0
            .checked_add(self.0)
            .map(DeviceCycle)
            .ok_or(BlockError::EpochOverflow)
    }
}

/// One exact cached motion segment in stream-relative clock units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionSegment<const AXES: usize> {
    /// Inclusive stream-relative start tick.
    pub start_tick: StreamTick,
    /// Exclusive stream-relative end tick.
    pub end_tick: StreamTick,
    /// Signed commanded lattice displacement for each axis.
    pub delta_steps: [i64; AXES],
    /// Reserved flags; must be zero.
    pub flags: u32,
}

/// Fractional precision of one canonical third-order step-coordinate value.
///
/// A signed Q31.32 value covers more than two billion relative command steps
/// while keeping every live finite-difference update in one checked `i64`.
/// Browser-side exact lowering must separately prove its approximation error;
/// firmware treats these integer coefficients as the complete execution input.
pub const FINITE_DIFFERENCE_FRACTIONAL_BITS: u32 = 32;
/// One whole command step in the canonical Q31.32 finite-difference lattice.
pub const FINITE_DIFFERENCE_ONE_STEP: i64 = 1_i64 << FINITE_DIFFERENCE_FRACTIONAL_BITS;

/// One axis of a cubic position sequence in Newton-forward form.
///
/// For update index `k`, starting at zero, the relative step coordinate is
///
/// `p(k) = p0 + k*d1 + C(k,2)*d2 + C(k,3)*d3`.
///
/// Equivalently, a dense executor performs `p += d1; d1 += d2; d2 += d3`
/// once per update period. All fields use the common Q31.32 step lattice.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FiniteDifferenceAxis {
    /// Exact relative Q31.32 coordinate at update zero.
    pub initial_position: i64,
    /// First forward difference at update zero.
    pub first_difference: i64,
    /// Second forward difference at update zero.
    pub second_difference: i64,
    /// Constant third forward difference.
    pub third_difference: i64,
}

/// One bounded third-order finite-difference step segment.
///
/// The segment is stream-relative and cannot arm itself. `end_tick` is
/// redundant by design: independent validators require it to equal
/// `start_tick + update_period_ticks * update_count` exactly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteDifferenceSegment<const AXES: usize> {
    /// Inclusive stream-relative start tick.
    pub start_tick: StreamTick,
    /// Exclusive stream-relative end tick.
    pub end_tick: StreamTick,
    /// Exact device ticks between adjacent forward-difference updates.
    pub update_period_ticks: u32,
    /// Number of updates in this segment.
    pub update_count: u32,
    /// Per-axis Q31.32 Newton-forward state at the segment start.
    pub axes: [FiniteDifferenceAxis; AXES],
    /// Reserved flags; the first schema requires zero.
    pub flags: u32,
}

/// Caller-owned finite-difference admission limits derived from configuration
/// and the selected output backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteDifferenceValidationLimits<const AXES: usize> {
    /// Longest accepted segment duration in device ticks.
    pub maximum_segment_ticks: u64,
    /// Largest accepted dense update count in one segment.
    pub maximum_update_count: u32,
    /// Exact device ticks between dense updates for this prepared stream.
    pub required_update_period_ticks: u32,
    /// Largest absolute command-step displacement on any axis.
    pub maximum_steps_per_segment: u64,
    /// Per-axis absolute first-difference ceiling in Q31.32 steps/update.
    ///
    /// A physical executor derives this from pulse high/low and maximum-rate
    /// spacing. Values below one whole step also prove that a dense update can
    /// cross at most one command-lattice boundary.
    pub maximum_absolute_first_difference: [u64; AXES],
}

/// Exact result of bounded third-order segment validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteDifferenceSegmentSummary<const AXES: usize> {
    /// Exact exclusive terminal stream tick.
    pub end_tick: StreamTick,
    /// Rounded relative command coordinates at update zero.
    pub start_steps: [i64; AXES],
    /// Rounded relative command coordinates at the terminal update.
    pub end_steps: [i64; AXES],
    /// Exact integer step displacement implied by those endpoints.
    pub delta_steps: [i64; AXES],
    /// Exact Q31.32 terminal coordinate carried into the next segment.
    pub terminal_position: [i64; AXES],
    /// Minimum first difference reached at a discrete update.
    pub minimum_first_difference: [i64; AXES],
    /// Maximum first difference reached at a discrete update.
    pub maximum_first_difference: [i64; AXES],
}

impl<const AXES: usize> FiniteDifferenceSegment<AXES> {
    /// Evaluate one exact Q31.32 axis coordinate at a bounded update index.
    ///
    /// This closed-form evaluator is used by allocation-free admission and
    /// sparse event searches. A dense backend may instead apply the equivalent
    /// three checked forward additions per update.
    pub fn position_at(&self, axis: usize, update: u32) -> Result<i64, FiniteDifferenceError> {
        if axis >= AXES {
            return Err(FiniteDifferenceError::AxisCount);
        }
        if update > self.update_count {
            return Err(FiniteDifferenceError::UpdatePolicy);
        }
        finite_difference_position(self.axes[axis], update)
            .and_then(i64_from_finite_difference)
            .map_err(|_| FiniteDifferenceError::CoefficientOverflow { axis })
    }

    /// Evaluate and round one exact finite-difference coordinate onto the
    /// integer command lattice.
    pub fn step_at(&self, axis: usize, update: u32) -> Result<i64, FiniteDifferenceError> {
        self.position_at(axis, update)
            .map(round_finite_difference_position)
    }

    /// Independently validates timing, coefficient range, exact continuity,
    /// monotonic direction, bounded lattice displacement, and the maximum
    /// per-update progress needed by an electrical preflight.
    ///
    /// Runtime is bounded by `AXES * log2(update_count)` and never iterates the
    /// dense frame count. `expected_initial_position` is the exact Q31.32 state
    /// retained from the preceding segment, or all zeroes at stream origin.
    pub fn validate(
        &self,
        expected_start_tick: StreamTick,
        expected_initial_position: [i64; AXES],
        limits: FiniteDifferenceValidationLimits<AXES>,
    ) -> Result<FiniteDifferenceSegmentSummary<AXES>, FiniteDifferenceError> {
        validate_finite_difference_limits(limits)?;
        if self.flags != 0 {
            return Err(FiniteDifferenceError::Flags(self.flags));
        }
        if self.start_tick != expected_start_tick {
            return Err(FiniteDifferenceError::StartTick {
                received: self.start_tick,
                expected: expected_start_tick,
            });
        }
        if self.update_period_ticks != limits.required_update_period_ticks
            || self.update_count == 0
            || self.update_count > limits.maximum_update_count
        {
            return Err(FiniteDifferenceError::UpdatePolicy);
        }
        let duration = u64::from(self.update_period_ticks)
            .checked_mul(u64::from(self.update_count))
            .ok_or(FiniteDifferenceError::Arithmetic)?;
        if duration > limits.maximum_segment_ticks
            || self.start_tick.0.checked_add(duration) != Some(self.end_tick.0)
        {
            return Err(FiniteDifferenceError::Time);
        }

        let mut start_steps = [0_i64; AXES];
        let mut end_steps = [0_i64; AXES];
        let mut delta_steps = [0_i64; AXES];
        let mut terminal_position = [0_i64; AXES];
        let mut minimum_first_difference = [0_i64; AXES];
        let mut maximum_first_difference = [0_i64; AXES];

        let mut axis = 0;
        while axis < AXES {
            let coefficients = self.axes[axis];
            if coefficients.initial_position != expected_initial_position[axis] {
                return Err(FiniteDifferenceError::PositionContinuity { axis });
            }
            let terminal = finite_difference_position(coefficients, self.update_count)
                .and_then(i64_from_finite_difference)
                .map_err(|_| FiniteDifferenceError::CoefficientOverflow { axis })?;
            finite_difference_state_fits(coefficients, self.update_count)
                .map_err(|_| FiniteDifferenceError::CoefficientOverflow { axis })?;
            let (minimum, maximum) =
                finite_difference_first_bounds(coefficients, self.update_count)
                    .and_then(|(minimum, maximum)| {
                        Ok((
                            i64_from_finite_difference(minimum)?,
                            i64_from_finite_difference(maximum)?,
                        ))
                    })
                    .map_err(|_| FiniteDifferenceError::CoefficientOverflow { axis })?;

            let initial = coefficients.initial_position;
            match terminal.cmp(&initial) {
                core::cmp::Ordering::Greater if minimum < 0 => {
                    return Err(FiniteDifferenceError::DirectionReversal { axis });
                }
                core::cmp::Ordering::Less if maximum > 0 => {
                    return Err(FiniteDifferenceError::DirectionReversal { axis });
                }
                core::cmp::Ordering::Equal if minimum != 0 || maximum != 0 => {
                    return Err(FiniteDifferenceError::DirectionReversal { axis });
                }
                _ => {}
            }

            let maximum_magnitude = minimum.unsigned_abs().max(maximum.unsigned_abs());
            if maximum_magnitude > limits.maximum_absolute_first_difference[axis] {
                return Err(FiniteDifferenceError::UpdateRate {
                    axis,
                    magnitude: maximum_magnitude,
                    maximum: limits.maximum_absolute_first_difference[axis],
                });
            }

            let start = round_finite_difference_position(initial);
            let end = round_finite_difference_position(terminal);
            let delta = end
                .checked_sub(start)
                .ok_or(FiniteDifferenceError::CoefficientOverflow { axis })?;
            let magnitude = delta.unsigned_abs();
            if magnitude > limits.maximum_steps_per_segment {
                return Err(FiniteDifferenceError::TooManySteps {
                    axis,
                    magnitude,
                    maximum: limits.maximum_steps_per_segment,
                });
            }
            start_steps[axis] = start;
            end_steps[axis] = end;
            delta_steps[axis] = delta;
            terminal_position[axis] = terminal;
            minimum_first_difference[axis] = minimum;
            maximum_first_difference[axis] = maximum;
            axis += 1;
        }

        Ok(FiniteDifferenceSegmentSummary {
            end_tick: self.end_tick,
            start_steps,
            end_steps,
            delta_steps,
            terminal_position,
            minimum_first_difference,
            maximum_first_difference,
        })
    }
}

/// Round one signed Q31.32 coordinate to the nearest integer step, with exact
/// ties to even. This is the sole command-lattice projection used by the
/// finite-difference IR and is symmetric for negative coordinates.
pub fn round_finite_difference_position(position: i64) -> i64 {
    let scale = i128::from(FINITE_DIFFERENCE_ONE_STEP);
    let value = i128::from(position);
    let quotient = value.div_euclid(scale);
    let remainder = value.rem_euclid(scale);
    let half = scale / 2;
    let rounded = if remainder < half || (remainder == half && quotient % 2 == 0) {
        quotient
    } else {
        quotient + 1
    };
    i64::try_from(rounded).expect("an i64 Q31.32 coordinate rounds inside i64")
}

/// Finite-difference structural or bounded-admission failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FiniteDifferenceError {
    /// Compile-time axis width is unsupported.
    AxisCount,
    /// Caller limits were zero or could permit more than one crossing/update.
    Limits,
    /// Reserved segment flags were nonzero.
    Flags(u32),
    /// Segment start did not equal the retained stream horizon.
    StartTick {
        /// Received start tick.
        received: StreamTick,
        /// Required start tick.
        expected: StreamTick,
    },
    /// Update period/count were zero or exceeded the bounded count.
    UpdatePolicy,
    /// End tick, duration, or maximum segment horizon disagreed.
    Time,
    /// The initial Q31.32 coordinate did not continue the preceding segment.
    PositionContinuity {
        /// Axis index.
        axis: usize,
    },
    /// A closed-form coordinate or forward-difference value exceeded `i64`.
    CoefficientOverflow {
        /// Axis index.
        axis: usize,
    },
    /// First differences reverse direction inside one segment.
    DirectionReversal {
        /// Axis index.
        axis: usize,
    },
    /// Per-update progress exceeded the caller's electrical/crossing bound.
    UpdateRate {
        /// Axis index.
        axis: usize,
        /// Received absolute Q31.32 steps/update.
        magnitude: u64,
        /// Maximum admitted absolute Q31.32 steps/update.
        maximum: u64,
    },
    /// Rounded terminal displacement exceeded the configured segment bound.
    TooManySteps {
        /// Axis index.
        axis: usize,
        /// Received absolute step count.
        magnitude: u64,
        /// Maximum admitted absolute step count.
        maximum: u64,
    },
    /// Checked integer arithmetic failed.
    Arithmetic,
}

fn validate_finite_difference_limits<const AXES: usize>(
    limits: FiniteDifferenceValidationLimits<AXES>,
) -> Result<(), FiniteDifferenceError> {
    if AXES == 0
        || AXES > MAX_EXECUTION_AXES
        || limits.maximum_segment_ticks == 0
        || limits.maximum_update_count == 0
        || limits.required_update_period_ticks == 0
        || limits.maximum_steps_per_segment == 0
        || limits
            .maximum_absolute_first_difference
            .iter()
            .any(|maximum| *maximum == 0 || *maximum >= FINITE_DIFFERENCE_ONE_STEP.unsigned_abs())
    {
        return Err(if AXES == 0 || AXES > MAX_EXECUTION_AXES {
            FiniteDifferenceError::AxisCount
        } else {
            FiniteDifferenceError::Limits
        });
    }
    Ok(())
}

fn finite_difference_position(
    coefficients: FiniteDifferenceAxis,
    update: u32,
) -> Result<i128, FiniteDifferenceError> {
    let k = i128::from(update);
    let choose_two = k
        .checked_mul(k - 1)
        .and_then(|value| value.checked_div(2))
        .ok_or(FiniteDifferenceError::Arithmetic)?;
    let choose_three = choose_two
        .checked_mul(k - 2)
        .and_then(|value| value.checked_div(3))
        .ok_or(FiniteDifferenceError::Arithmetic)?;
    i128::from(coefficients.initial_position)
        .checked_add(
            k.checked_mul(i128::from(coefficients.first_difference))
                .ok_or(FiniteDifferenceError::Arithmetic)?,
        )
        .and_then(|value| {
            value.checked_add(choose_two.checked_mul(i128::from(coefficients.second_difference))?)
        })
        .and_then(|value| {
            value.checked_add(choose_three.checked_mul(i128::from(coefficients.third_difference))?)
        })
        .ok_or(FiniteDifferenceError::Arithmetic)
}

fn finite_difference_first(
    coefficients: FiniteDifferenceAxis,
    update: u32,
) -> Result<i128, FiniteDifferenceError> {
    let k = i128::from(update);
    let choose_two = k
        .checked_mul(k - 1)
        .and_then(|value| value.checked_div(2))
        .ok_or(FiniteDifferenceError::Arithmetic)?;
    i128::from(coefficients.first_difference)
        .checked_add(
            k.checked_mul(i128::from(coefficients.second_difference))
                .ok_or(FiniteDifferenceError::Arithmetic)?,
        )
        .and_then(|value| {
            value.checked_add(choose_two.checked_mul(i128::from(coefficients.third_difference))?)
        })
        .ok_or(FiniteDifferenceError::Arithmetic)
}

fn finite_difference_second(
    coefficients: FiniteDifferenceAxis,
    update: u32,
) -> Result<i128, FiniteDifferenceError> {
    i128::from(coefficients.second_difference)
        .checked_add(
            i128::from(update)
                .checked_mul(i128::from(coefficients.third_difference))
                .ok_or(FiniteDifferenceError::Arithmetic)?,
        )
        .ok_or(FiniteDifferenceError::Arithmetic)
}

fn finite_difference_state_fits(
    coefficients: FiniteDifferenceAxis,
    update_count: u32,
) -> Result<(), FiniteDifferenceError> {
    i64_from_finite_difference(finite_difference_position(coefficients, update_count)?)?;
    i64_from_finite_difference(finite_difference_first(coefficients, update_count)?)?;
    i64_from_finite_difference(finite_difference_second(coefficients, update_count)?)?;
    Ok(())
}

fn finite_difference_first_bounds(
    coefficients: FiniteDifferenceAxis,
    update_count: u32,
) -> Result<(i128, i128), FiniteDifferenceError> {
    let final_update = update_count - 1;
    let first = finite_difference_first(coefficients, 0)?;
    let last = finite_difference_first(coefficients, final_update)?;
    let mut minimum = first.min(last);
    let mut maximum = first.max(last);

    if update_count > 1 && coefficients.third_difference != 0 {
        let second_at_zero = i128::from(coefficients.second_difference);
        let second_at_last_change = finite_difference_second(coefficients, update_count - 2)?;
        let increasing_curvature = coefficients.third_difference > 0;
        let crosses = if increasing_curvature {
            second_at_zero < 0 && second_at_last_change >= 0
        } else {
            second_at_zero > 0 && second_at_last_change <= 0
        };
        if crosses {
            let mut low = 0_u32;
            let mut high = update_count - 2;
            while low < high {
                let middle = low + (high - low) / 2;
                let second = finite_difference_second(coefficients, middle)?;
                let reached = if increasing_curvature {
                    second >= 0
                } else {
                    second <= 0
                };
                if reached {
                    high = middle;
                } else {
                    low = middle + 1;
                }
            }
            for update in [low, low.saturating_add(1).min(final_update)] {
                let value = finite_difference_first(coefficients, update)?;
                minimum = minimum.min(value);
                maximum = maximum.max(value);
            }
        }
    }
    Ok((minimum, maximum))
}

fn i64_from_finite_difference(value: i128) -> Result<i64, FiniteDifferenceError> {
    i64::try_from(value).map_err(|_| FiniteDifferenceError::Arithmetic)
}

/// Exact execution payload family admitted by the machine-block schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ExecutionKind {
    /// Coordinated integer lattice displacement segments.
    Motion = 1,
    /// Direct Q31.32 third-order finite-difference step segments.
    FiniteDifference = 2,
    /// Fixed-cadence Q31.32/Q2.30 servo-setpoint recurrences.
    ServoFiniteDifference = 3,
}

impl ExecutionKind {
    /// Decode one exact machine-block execution-kind discriminant.
    pub const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Motion),
            2 => Some(Self::FiniteDifference),
            3 => Some(Self::ServoFiniteDifference),
            _ => None,
        }
    }
}

/// Decoded immutable metadata repeated in every independently checked block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionBlockHeader {
    /// Exact payload family.
    pub kind: ExecutionKind,
    /// Axis width of each motion record.
    pub axis_count: u8,
    /// Strictly contiguous stream sequence, beginning at zero.
    pub sequence: u32,
    /// Number of canonical records in this block.
    pub segment_count: u32,
    /// Exact initialized bytes in the padded record area.
    pub payload_len: u32,
    /// Inclusive stream-relative execution tick.
    pub start_tick: StreamTick,
    /// Exclusive stream-relative execution tick.
    pub end_tick: StreamTick,
    /// Per-partition identity installed during job preparation.
    pub stream_id: StreamId,
    /// Board capability set against which the stream was compiled.
    pub capability_digest: Digest,
    /// Runtime machine configuration against which the stream was compiled.
    pub config_digest: Digest,
    /// Digest of sequence `n - 1`, or zero only for sequence zero.
    pub previous_digest: Digest,
    /// SHA-256 over the exact 480-byte header and padded payload prefix.
    pub block_digest: Digest,
}

/// Caller-provided facts core 0 and core 1 must independently require.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockExpectation {
    /// Prepared stream identity.
    pub stream_id: StreamId,
    /// Active board capability digest.
    pub capability_digest: Digest,
    /// Active machine configuration digest.
    pub config_digest: Digest,
    /// Next required stream sequence.
    pub sequence: u32,
    /// Required contiguous stream-relative start tick.
    pub start_tick: StreamTick,
    /// Previously admitted block digest, zero for the first block.
    pub previous_digest: Digest,
}

/// Board/config-derived bounds for one independently consumable block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockValidationLimits {
    /// Longest admitted time horizon carried by one work-block ownership unit.
    pub maximum_block_ticks: u64,
    /// Per-segment bounds already used by whole-job validation.
    pub segment: ValidationLimits,
}

/// Validated terminal facts used to advance the next-block expectation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionBlockSummary<const AXES: usize> {
    /// Validated block sequence.
    pub sequence: u32,
    /// Exclusive terminal stream-relative tick.
    pub end_tick: StreamTick,
    /// Relative displacement accumulated within this block.
    pub final_steps: [i64; AXES],
    /// Exact digest required in the following block.
    pub block_digest: Digest,
    /// Number of decoded motion segments.
    pub segment_count: u32,
}

/// Board/config-derived bounds for one direct finite-difference work block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteDifferenceBlockValidationLimits<const AXES: usize> {
    /// Longest admitted block ownership horizon in device ticks.
    pub maximum_block_ticks: u64,
    /// Per-record third-order and step-crossing bounds.
    pub segment: FiniteDifferenceValidationLimits<AXES>,
}

/// Validated terminal facts for one direct finite-difference work block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteDifferenceExecutionBlockSummary<const AXES: usize> {
    /// Validated block sequence.
    pub sequence: u32,
    /// Exclusive terminal stream-relative tick.
    pub end_tick: StreamTick,
    /// Relative integer displacement accumulated within this block.
    pub final_steps: [i64; AXES],
    /// Exact terminal Q31.32 coordinate required by the following block.
    pub terminal_finite_position: [i64; AXES],
    /// Exact digest required in the following block.
    pub block_digest: Digest,
    /// Number of decoded finite-difference records.
    pub segment_count: u32,
    /// Total dense updates represented by this block.
    pub update_count: u64,
}

impl<const AXES: usize> FiniteDifferenceExecutionBlockSummary<AXES> {
    /// Derive the only valid identity/chain/tick expectation for the next
    /// direct finite-difference block.
    pub fn next_expectation(
        self,
        stream_id: StreamId,
        capability_digest: Digest,
        config_digest: Digest,
    ) -> Result<BlockExpectation, BlockError> {
        Ok(BlockExpectation {
            stream_id,
            capability_digest,
            config_digest,
            sequence: self
                .sequence
                .checked_add(1)
                .ok_or(BlockError::SequenceOverflow)?,
            start_tick: self.end_tick,
            previous_digest: self.block_digest,
        })
    }
}

impl<const AXES: usize> ExecutionBlockSummary<AXES> {
    /// Derives the only valid expectation for the next contiguous block.
    pub fn next_expectation(
        self,
        stream_id: StreamId,
        capability_digest: Digest,
        config_digest: Digest,
    ) -> Result<BlockExpectation, BlockError> {
        Ok(BlockExpectation {
            stream_id,
            capability_digest,
            config_digest,
            sequence: self
                .sequence
                .checked_add(1)
                .ok_or(BlockError::SequenceOverflow)?,
            start_tick: self.end_tick,
            previous_digest: self.block_digest,
        })
    }
}

/// Stateful independent validator for one complete fixed-block motion stream.
pub struct MotionStreamValidator<const AXES: usize> {
    expectation: BlockExpectation,
    limits: BlockValidationLimits,
    expected_blocks: u32,
    accepted_blocks: u32,
    position: [i64; AXES],
}

impl<const AXES: usize> MotionStreamValidator<AXES> {
    /// Starts at sequence zero with the exact prepared identities and cycle.
    pub fn new(
        expected_blocks: u32,
        expectation: BlockExpectation,
        limits: BlockValidationLimits,
    ) -> Result<Self, BlockError> {
        validate_axis_count::<AXES>()?;
        if expected_blocks == 0 {
            return Err(BlockError::PartitionLength);
        }
        if expectation.sequence != 0 || !expectation.previous_digest.is_zero() {
            return Err(BlockError::ChainOrigin);
        }
        if !expectation.stream_id.is_valid() {
            return Err(BlockError::MissingStreamId);
        }
        if expectation.capability_digest.is_zero() {
            return Err(BlockError::MissingCapabilityDigest);
        }
        if expectation.config_digest.is_zero() {
            return Err(BlockError::MissingConfigDigest);
        }
        Ok(Self {
            expectation,
            limits,
            expected_blocks,
            accepted_blocks: 0,
            position: [0; AXES],
        })
    }

    /// Independently admits exactly the next block and advances no state on error.
    pub fn accept(
        &mut self,
        block: &ExecutionBlock,
    ) -> Result<MotionStreamProgress<AXES>, BlockError> {
        if self.accepted_blocks >= self.expected_blocks {
            return Err(BlockError::StreamComplete);
        }
        let summary = block.validate_motion::<AXES>(self.expectation, self.limits)?;
        let mut position = self.position;
        for (axis, delta) in summary.final_steps.iter().copied().enumerate() {
            position[axis] = position[axis]
                .checked_add(delta)
                .ok_or(BlockError::StreamPositionOverflow { axis })?;
        }
        let accepted_blocks = self
            .accepted_blocks
            .checked_add(1)
            .ok_or(BlockError::Arithmetic)?;
        let next = summary.next_expectation(
            self.expectation.stream_id,
            self.expectation.capability_digest,
            self.expectation.config_digest,
        );
        if accepted_blocks < self.expected_blocks && next.is_err() {
            return Err(BlockError::SequenceOverflow);
        }
        self.accepted_blocks = accepted_blocks;
        self.position = position;
        if let Ok(next) = next {
            self.expectation = next;
        }
        Ok(MotionStreamProgress {
            accepted_blocks,
            expected_blocks: self.expected_blocks,
            end_tick: summary.end_tick,
            position,
            block_digest: summary.block_digest,
            complete: accepted_blocks == self.expected_blocks,
        })
    }

    /// Requires that the exact object-derived number of blocks was accepted.
    pub fn finish(&self) -> Result<MotionStreamProgress<AXES>, BlockError> {
        if self.accepted_blocks != self.expected_blocks {
            return Err(BlockError::StreamIncomplete {
                received: self.accepted_blocks,
                expected: self.expected_blocks,
            });
        }
        Ok(MotionStreamProgress {
            accepted_blocks: self.accepted_blocks,
            expected_blocks: self.expected_blocks,
            end_tick: self.expectation.start_tick,
            position: self.position,
            block_digest: self.expectation.previous_digest,
            complete: true,
        })
    }

    /// Exact next expected block facts, useful for bounded diagnostics.
    pub const fn expectation(&self) -> BlockExpectation {
        self.expectation
    }
}

/// Cumulative facts after one successful stream-block ownership admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MotionStreamProgress<const AXES: usize> {
    /// Blocks independently admitted so far.
    pub accepted_blocks: u32,
    /// Exact block count derived from immutable object length.
    pub expected_blocks: u32,
    /// Exclusive stream-relative end tick of the accepted horizon.
    pub end_tick: StreamTick,
    /// Cumulative relative lattice displacement.
    pub position: [i64; AXES],
    /// Digest required by the next block, or terminal chain digest.
    pub block_digest: Digest,
    /// Whether the complete declared stream was admitted.
    pub complete: bool,
}

/// Stateful independent validator for one complete direct finite-difference
/// cache stream. It retains exact Q31.32 continuity across block ownership
/// boundaries in addition to the existing digest/tick/step facts.
pub struct FiniteDifferenceStreamValidator<const AXES: usize> {
    expectation: BlockExpectation,
    limits: FiniteDifferenceBlockValidationLimits<AXES>,
    expected_blocks: u32,
    accepted_blocks: u32,
    position: [i64; AXES],
    finite_position: [i64; AXES],
    update_count: u64,
}

impl<const AXES: usize> FiniteDifferenceStreamValidator<AXES> {
    /// Starts one relative stream at an exact zero tick, integer displacement,
    /// and Q31.32 coordinate. The later scheduled commit still owns the
    /// absolute device epoch and machine position.
    pub fn new(
        expected_blocks: u32,
        expectation: BlockExpectation,
        limits: FiniteDifferenceBlockValidationLimits<AXES>,
    ) -> Result<Self, BlockError> {
        validate_axis_count::<AXES>()?;
        if expected_blocks == 0 {
            return Err(BlockError::PartitionLength);
        }
        if expectation.sequence != 0
            || !expectation.previous_digest.is_zero()
            || expectation.start_tick != StreamTick(0)
        {
            return Err(BlockError::ChainOrigin);
        }
        if !expectation.stream_id.is_valid() {
            return Err(BlockError::MissingStreamId);
        }
        if expectation.capability_digest.is_zero() {
            return Err(BlockError::MissingCapabilityDigest);
        }
        if expectation.config_digest.is_zero() {
            return Err(BlockError::MissingConfigDigest);
        }
        Ok(Self {
            expectation,
            limits,
            expected_blocks,
            accepted_blocks: 0,
            position: [0; AXES],
            finite_position: [0; AXES],
            update_count: 0,
        })
    }

    /// Independently admit exactly the next block without advancing state on
    /// any identity, chain, coefficient, continuity, or arithmetic failure.
    pub fn accept(
        &mut self,
        block: &ExecutionBlock,
    ) -> Result<FiniteDifferenceStreamProgress<AXES>, BlockError> {
        if self.accepted_blocks >= self.expected_blocks {
            return Err(BlockError::StreamComplete);
        }
        let summary = block.validate_finite_difference::<AXES>(
            self.expectation,
            self.finite_position,
            self.limits,
        )?;
        let mut position = self.position;
        for (axis, delta) in summary.final_steps.iter().copied().enumerate() {
            position[axis] = position[axis]
                .checked_add(delta)
                .ok_or(BlockError::StreamPositionOverflow { axis })?;
        }
        let accepted_blocks = self
            .accepted_blocks
            .checked_add(1)
            .ok_or(BlockError::Arithmetic)?;
        let update_count = self
            .update_count
            .checked_add(summary.update_count)
            .ok_or(BlockError::Arithmetic)?;
        let next = summary.next_expectation(
            self.expectation.stream_id,
            self.expectation.capability_digest,
            self.expectation.config_digest,
        );
        if accepted_blocks < self.expected_blocks && next.is_err() {
            return Err(BlockError::SequenceOverflow);
        }
        self.accepted_blocks = accepted_blocks;
        self.position = position;
        self.finite_position = summary.terminal_finite_position;
        self.update_count = update_count;
        if let Ok(next) = next {
            self.expectation = next;
        }
        Ok(self.progress())
    }

    /// Require the immutable object-derived number of blocks.
    pub fn finish(&self) -> Result<FiniteDifferenceStreamProgress<AXES>, BlockError> {
        if self.accepted_blocks != self.expected_blocks {
            return Err(BlockError::StreamIncomplete {
                received: self.accepted_blocks,
                expected: self.expected_blocks,
            });
        }
        Ok(self.progress())
    }

    /// Exact next required block facts.
    pub const fn expectation(&self) -> BlockExpectation {
        self.expectation
    }

    const fn progress(&self) -> FiniteDifferenceStreamProgress<AXES> {
        FiniteDifferenceStreamProgress {
            accepted_blocks: self.accepted_blocks,
            expected_blocks: self.expected_blocks,
            end_tick: self.expectation.start_tick,
            position: self.position,
            finite_position: self.finite_position,
            update_count: self.update_count,
            block_digest: self.expectation.previous_digest,
            complete: self.accepted_blocks == self.expected_blocks,
        }
    }
}

/// Cumulative direct finite-difference stream admission facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteDifferenceStreamProgress<const AXES: usize> {
    /// Blocks independently admitted so far.
    pub accepted_blocks: u32,
    /// Exact block count derived from immutable object length.
    pub expected_blocks: u32,
    /// Exclusive terminal stream-relative tick.
    pub end_tick: StreamTick,
    /// Cumulative rounded relative lattice displacement.
    pub position: [i64; AXES],
    /// Exact terminal Q31.32 coordinate.
    pub finite_position: [i64; AXES],
    /// Total dense updates represented by admitted blocks.
    pub update_count: u64,
    /// Digest required by the next block, or terminal chain digest.
    pub block_digest: Digest,
    /// Whether the complete declared stream was admitted.
    pub complete: bool,
}

/// One canonical owned work unit. It deliberately has no `Copy` or `Clone`.
#[repr(transparent)]
pub struct ExecutionBlock {
    bytes: [u8; EXECUTION_BLOCK_BYTES],
}

impl ExecutionBlock {
    /// Encodes one exact motion block, including zero padding and digest chain.
    pub fn encode_motion<const AXES: usize>(
        stream_id: StreamId,
        capability_digest: Digest,
        config_digest: Digest,
        sequence: u32,
        previous_digest: Digest,
        segments: &[ExecutionSegment<AXES>],
    ) -> Result<Self, BlockError> {
        validate_axis_count::<AXES>()?;
        if !stream_id.is_valid() {
            return Err(BlockError::MissingStreamId);
        }
        if capability_digest.is_zero() {
            return Err(BlockError::MissingCapabilityDigest);
        }
        if config_digest.is_zero() {
            return Err(BlockError::MissingConfigDigest);
        }
        validate_chain_origin(sequence, previous_digest)?;
        if segments.is_empty() {
            return Err(BlockError::SegmentCount);
        }
        let record_bytes = motion_record_bytes::<AXES>()?;
        let payload_len = segments
            .len()
            .checked_mul(record_bytes)
            .ok_or(BlockError::Arithmetic)?;
        if payload_len > EXECUTION_BLOCK_PAYLOAD_BYTES {
            return Err(BlockError::PayloadLength);
        }
        let segment_count = u32::try_from(segments.len()).map_err(|_| BlockError::SegmentCount)?;
        let payload_len_wire = u32::try_from(payload_len).map_err(|_| BlockError::PayloadLength)?;
        let first = segments.first().ok_or(BlockError::SegmentCount)?;
        let last = segments.last().ok_or(BlockError::SegmentCount)?;
        if first.end_tick.0 <= first.start_tick.0 {
            return Err(BlockError::SegmentTime { index: 0 });
        }

        let mut bytes = [0_u8; EXECUTION_BLOCK_BYTES];
        bytes[0..8].copy_from_slice(&BLOCK_MAGIC);
        bytes[8..10].copy_from_slice(&MACHINE_IR_VERSION.to_le_bytes());
        bytes[10] = ExecutionKind::Motion as u8;
        bytes[11] = u8::try_from(AXES).map_err(|_| BlockError::AxisCount {
            encoded: u8::MAX,
            expected: AXES,
        })?;
        bytes[12..16].copy_from_slice(&sequence.to_le_bytes());
        bytes[16..20].copy_from_slice(&segment_count.to_le_bytes());
        bytes[20..24].copy_from_slice(&payload_len_wire.to_le_bytes());
        bytes[24..32].copy_from_slice(&first.start_tick.0.to_le_bytes());
        bytes[32..40].copy_from_slice(&last.end_tick.0.to_le_bytes());
        bytes[40..56].copy_from_slice(&stream_id.0);
        bytes[56..88].copy_from_slice(&capability_digest.0);
        bytes[88..120].copy_from_slice(&config_digest.0);
        bytes[120..152].copy_from_slice(&previous_digest.0);

        let mut expected_start = first.start_tick;
        for (index, segment) in segments.iter().enumerate() {
            if segment.flags != 0 {
                return Err(BlockError::SegmentFlags {
                    index,
                    flags: segment.flags,
                });
            }
            if segment.start_tick != expected_start || segment.end_tick.0 <= segment.start_tick.0 {
                return Err(BlockError::SegmentTime { index });
            }
            let duration = segment.end_tick.0 - segment.start_tick.0;
            let offset = EXECUTION_BLOCK_HEADER_BYTES
                .checked_add(
                    index
                        .checked_mul(record_bytes)
                        .ok_or(BlockError::Arithmetic)?,
                )
                .ok_or(BlockError::Arithmetic)?;
            bytes[offset..offset + 8].copy_from_slice(&duration.to_le_bytes());
            bytes[offset + 8..offset + 12].copy_from_slice(&segment.flags.to_le_bytes());
            for (axis, delta) in segment.delta_steps.iter().enumerate() {
                let axis_offset = offset
                    .checked_add(MOTION_RECORD_PREFIX_BYTES)
                    .and_then(|base| base.checked_add(axis.checked_mul(8)?))
                    .ok_or(BlockError::Arithmetic)?;
                bytes[axis_offset..axis_offset + 8].copy_from_slice(&delta.to_le_bytes());
            }
            expected_start = segment.end_tick;
        }
        let digest = hash_block_prefix(&bytes);
        if digest.is_zero() {
            return Err(BlockError::BlockDigest);
        }
        bytes[EXECUTION_BLOCK_DIGEST_OFFSET..].copy_from_slice(&digest.0);
        Self::decode(bytes)
    }

    /// Encodes one direct third-order finite-difference block with canonical
    /// zero padding and the same immutable digest chain as ordinary motion.
    pub fn encode_finite_difference<const AXES: usize>(
        stream_id: StreamId,
        capability_digest: Digest,
        config_digest: Digest,
        sequence: u32,
        previous_digest: Digest,
        segments: &[FiniteDifferenceSegment<AXES>],
    ) -> Result<Self, BlockError> {
        validate_axis_count::<AXES>()?;
        if !stream_id.is_valid() {
            return Err(BlockError::MissingStreamId);
        }
        if capability_digest.is_zero() {
            return Err(BlockError::MissingCapabilityDigest);
        }
        if config_digest.is_zero() {
            return Err(BlockError::MissingConfigDigest);
        }
        validate_chain_origin(sequence, previous_digest)?;
        if segments.is_empty() {
            return Err(BlockError::SegmentCount);
        }
        let record_bytes = finite_difference_record_bytes::<AXES>()?;
        let payload_len = segments
            .len()
            .checked_mul(record_bytes)
            .ok_or(BlockError::Arithmetic)?;
        if payload_len > EXECUTION_BLOCK_PAYLOAD_BYTES {
            return Err(BlockError::PayloadLength);
        }
        let segment_count = u32::try_from(segments.len()).map_err(|_| BlockError::SegmentCount)?;
        let payload_len_wire = u32::try_from(payload_len).map_err(|_| BlockError::PayloadLength)?;
        let first = segments.first().ok_or(BlockError::SegmentCount)?;
        let last = segments.last().ok_or(BlockError::SegmentCount)?;

        let mut bytes = [0_u8; EXECUTION_BLOCK_BYTES];
        bytes[0..8].copy_from_slice(&BLOCK_MAGIC);
        bytes[8..10].copy_from_slice(&MACHINE_IR_VERSION.to_le_bytes());
        bytes[10] = ExecutionKind::FiniteDifference as u8;
        bytes[11] = u8::try_from(AXES).map_err(|_| BlockError::AxisCount {
            encoded: u8::MAX,
            expected: AXES,
        })?;
        bytes[12..16].copy_from_slice(&sequence.to_le_bytes());
        bytes[16..20].copy_from_slice(&segment_count.to_le_bytes());
        bytes[20..24].copy_from_slice(&payload_len_wire.to_le_bytes());
        bytes[24..32].copy_from_slice(&first.start_tick.0.to_le_bytes());
        bytes[32..40].copy_from_slice(&last.end_tick.0.to_le_bytes());
        bytes[40..56].copy_from_slice(&stream_id.0);
        bytes[56..88].copy_from_slice(&capability_digest.0);
        bytes[88..120].copy_from_slice(&config_digest.0);
        bytes[120..152].copy_from_slice(&previous_digest.0);

        let mut expected_start = first.start_tick;
        for (index, segment) in segments.iter().enumerate() {
            if segment.flags != 0 {
                return Err(BlockError::SegmentFlags {
                    index,
                    flags: segment.flags,
                });
            }
            let duration = u64::from(segment.update_period_ticks)
                .checked_mul(u64::from(segment.update_count))
                .ok_or(BlockError::SegmentTime { index })?;
            if segment.update_period_ticks == 0
                || segment.update_count == 0
                || segment.start_tick != expected_start
                || segment.start_tick.0.checked_add(duration) != Some(segment.end_tick.0)
            {
                return Err(BlockError::SegmentTime { index });
            }
            let offset = EXECUTION_BLOCK_HEADER_BYTES
                .checked_add(
                    index
                        .checked_mul(record_bytes)
                        .ok_or(BlockError::Arithmetic)?,
                )
                .ok_or(BlockError::Arithmetic)?;
            bytes[offset..offset + 4].copy_from_slice(&segment.update_period_ticks.to_le_bytes());
            bytes[offset + 4..offset + 8].copy_from_slice(&segment.update_count.to_le_bytes());
            bytes[offset + 8..offset + 12].copy_from_slice(&segment.flags.to_le_bytes());
            for (axis, coefficients) in segment.axes.iter().enumerate() {
                let axis_offset = offset
                    .checked_add(FINITE_DIFFERENCE_RECORD_PREFIX_BYTES)
                    .and_then(|base| {
                        base.checked_add(axis.checked_mul(FINITE_DIFFERENCE_AXIS_BYTES)?)
                    })
                    .ok_or(BlockError::Arithmetic)?;
                for (field, value) in [
                    coefficients.initial_position,
                    coefficients.first_difference,
                    coefficients.second_difference,
                    coefficients.third_difference,
                ]
                .iter()
                .copied()
                .enumerate()
                {
                    let field_offset = axis_offset
                        .checked_add(field.checked_mul(8).ok_or(BlockError::Arithmetic)?)
                        .ok_or(BlockError::Arithmetic)?;
                    bytes[field_offset..field_offset + 8].copy_from_slice(&value.to_le_bytes());
                }
            }
            expected_start = segment.end_tick;
        }
        let digest = hash_block_prefix(&bytes);
        if digest.is_zero() {
            return Err(BlockError::BlockDigest);
        }
        bytes[EXECUTION_BLOCK_DIGEST_OFFSET..].copy_from_slice(&digest.0);
        Self::decode(bytes)
    }

    /// Takes ownership of one exact 512-byte block after structural validation.
    pub fn decode(bytes: [u8; EXECUTION_BLOCK_BYTES]) -> Result<Self, BlockError> {
        let header = decode_block_header(&bytes)?;
        validate_block_structure(&bytes, header)?;
        Ok(Self { bytes })
    }

    /// Canonical bytes suitable for hashing, storage, or an ownership channel.
    pub const fn as_bytes(&self) -> &[u8; EXECUTION_BLOCK_BYTES] {
        &self.bytes
    }

    /// Returns canonical bytes while consuming this ownership unit.
    pub const fn into_bytes(self) -> [u8; EXECUTION_BLOCK_BYTES] {
        self.bytes
    }

    /// Structurally validated immutable metadata.
    pub fn header(&self) -> ExecutionBlockHeader {
        decode_block_header(&self.bytes).expect("ExecutionBlock constructors validate headers")
    }

    /// Revalidates identity, chain, timing, and configured limits independently.
    pub fn validate_motion<const AXES: usize>(
        &self,
        expected: BlockExpectation,
        limits: BlockValidationLimits,
    ) -> Result<ExecutionBlockSummary<AXES>, BlockError> {
        validate_axis_count::<AXES>()?;
        let header = decode_block_header(&self.bytes)?;
        validate_block_structure(&self.bytes, header)?;
        if header.kind != ExecutionKind::Motion {
            return Err(BlockError::Kind {
                received: header.kind as u8,
            });
        }
        if usize::from(header.axis_count) != AXES {
            return Err(BlockError::AxisCount {
                encoded: header.axis_count,
                expected: AXES,
            });
        }
        if header.stream_id != expected.stream_id {
            return Err(BlockError::StreamIdentity);
        }
        if header.capability_digest != expected.capability_digest {
            return Err(BlockError::CapabilityIdentity);
        }
        if header.config_digest != expected.config_digest {
            return Err(BlockError::ConfigurationIdentity);
        }
        if header.sequence != expected.sequence {
            return Err(BlockError::Sequence {
                received: header.sequence,
                expected: expected.sequence,
            });
        }
        if header.start_tick != expected.start_tick {
            return Err(BlockError::StartTick {
                received: header.start_tick,
                expected: expected.start_tick,
            });
        }
        if header.previous_digest != expected.previous_digest {
            return Err(BlockError::PreviousDigest);
        }
        let block_ticks = header.end_tick.0 - header.start_tick.0;
        if limits.maximum_block_ticks == 0 || block_ticks > limits.maximum_block_ticks {
            return Err(BlockError::BlockTooLong {
                duration: block_ticks,
                maximum: limits.maximum_block_ticks,
            });
        }

        let mut final_steps = [0_i64; AXES];
        let mut segments = self.motion_segments::<AXES>()?;
        for (index, segment) in (&mut segments).enumerate() {
            let duration = segment.end_tick.0 - segment.start_tick.0;
            if duration > limits.segment.maximum_segment_ticks {
                return Err(BlockError::SegmentTooLong {
                    index,
                    duration,
                    maximum: limits.segment.maximum_segment_ticks,
                });
            }
            for (axis, delta) in segment.delta_steps.iter().copied().enumerate() {
                let magnitude = delta.unsigned_abs();
                if magnitude > limits.segment.maximum_steps_per_segment {
                    return Err(BlockError::TooManySteps {
                        index,
                        axis,
                        magnitude,
                        maximum: limits.segment.maximum_steps_per_segment,
                    });
                }
                final_steps[axis] = final_steps[axis]
                    .checked_add(delta)
                    .ok_or(BlockError::PositionOverflow { index, axis })?;
            }
        }
        Ok(ExecutionBlockSummary {
            sequence: header.sequence,
            end_tick: header.end_tick,
            final_steps,
            block_digest: header.block_digest,
            segment_count: header.segment_count,
        })
    }

    /// Revalidates one direct finite-difference block against exact identity,
    /// chain, incoming Q31.32 state, and caller-owned execution bounds.
    pub fn validate_finite_difference<const AXES: usize>(
        &self,
        expected: BlockExpectation,
        expected_initial_position: [i64; AXES],
        limits: FiniteDifferenceBlockValidationLimits<AXES>,
    ) -> Result<FiniteDifferenceExecutionBlockSummary<AXES>, BlockError> {
        validate_axis_count::<AXES>()?;
        let header = decode_block_header(&self.bytes)?;
        validate_block_structure(&self.bytes, header)?;
        if header.kind != ExecutionKind::FiniteDifference {
            return Err(BlockError::Kind {
                received: header.kind as u8,
            });
        }
        if usize::from(header.axis_count) != AXES {
            return Err(BlockError::AxisCount {
                encoded: header.axis_count,
                expected: AXES,
            });
        }
        validate_block_expectation(header, expected)?;
        let block_ticks = header.end_tick.0 - header.start_tick.0;
        if limits.maximum_block_ticks == 0 || block_ticks > limits.maximum_block_ticks {
            return Err(BlockError::BlockTooLong {
                duration: block_ticks,
                maximum: limits.maximum_block_ticks,
            });
        }

        let mut finite_position = expected_initial_position;
        let mut start_steps = finite_position.map(round_finite_difference_position);
        let mut final_steps = [0_i64; AXES];
        let mut update_count = 0_u64;
        let mut next_tick = expected.start_tick;
        let mut segments = self.finite_difference_segments::<AXES>()?;
        for (index, segment) in (&mut segments).enumerate() {
            let summary = segment
                .validate(next_tick, finite_position, limits.segment)
                .map_err(|error| BlockError::FiniteDifference { index, error })?;
            let mut axis = 0;
            while axis < AXES {
                if summary.start_steps[axis] != start_steps[axis] {
                    return Err(BlockError::FiniteDifference {
                        index,
                        error: FiniteDifferenceError::PositionContinuity { axis },
                    });
                }
                final_steps[axis] = final_steps[axis]
                    .checked_add(summary.delta_steps[axis])
                    .ok_or(BlockError::PositionOverflow { index, axis })?;
                axis += 1;
            }
            update_count = update_count
                .checked_add(u64::from(segment.update_count))
                .ok_or(BlockError::Arithmetic)?;
            finite_position = summary.terminal_position;
            start_steps = summary.end_steps;
            next_tick = summary.end_tick;
        }
        if next_tick != header.end_tick {
            return Err(BlockError::BlockTime);
        }
        Ok(FiniteDifferenceExecutionBlockSummary {
            sequence: header.sequence,
            end_tick: header.end_tick,
            final_steps,
            terminal_finite_position: finite_position,
            block_digest: header.block_digest,
            segment_count: header.segment_count,
            update_count,
        })
    }

    /// Iterates decoded segments without allocation or exposing backing bytes.
    pub fn motion_segments<const AXES: usize>(
        &self,
    ) -> Result<MotionSegments<'_, AXES>, BlockError> {
        validate_axis_count::<AXES>()?;
        let header = self.header();
        if header.kind != ExecutionKind::Motion {
            return Err(BlockError::Kind {
                received: header.kind as u8,
            });
        }
        if usize::from(header.axis_count) != AXES {
            return Err(BlockError::AxisCount {
                encoded: header.axis_count,
                expected: AXES,
            });
        }
        Ok(MotionSegments {
            bytes: &self.bytes,
            next: 0,
            count: header.segment_count,
            tick: header.start_tick,
        })
    }

    /// Iterates direct finite-difference records without allocation or exposing
    /// mutable access to their canonical backing bytes.
    pub fn finite_difference_segments<const AXES: usize>(
        &self,
    ) -> Result<FiniteDifferenceSegments<'_, AXES>, BlockError> {
        validate_axis_count::<AXES>()?;
        let header = self.header();
        if header.kind != ExecutionKind::FiniteDifference {
            return Err(BlockError::Kind {
                received: header.kind as u8,
            });
        }
        if usize::from(header.axis_count) != AXES {
            return Err(BlockError::AxisCount {
                encoded: header.axis_count,
                expected: AXES,
            });
        }
        Ok(FiniteDifferenceSegments {
            bytes: &self.bytes,
            next: 0,
            count: header.segment_count,
            tick: header.start_tick,
        })
    }
}

/// Returns the exact number of coordinated motion records that fit in one
/// canonical execution block for `AXES`.
///
/// Authoritative compilers use this query instead of duplicating the motion-record
/// prefix or axis-field layout. Zero and over-wide axis vectors fail with the
/// same error as block construction.
pub fn maximum_motion_segments_per_block<const AXES: usize>() -> Result<usize, BlockError> {
    let record_bytes = motion_record_bytes::<AXES>()?;
    Ok(EXECUTION_BLOCK_PAYLOAD_BYTES / record_bytes)
}

/// Exact number of direct finite-difference records fitting one block.
pub fn maximum_finite_difference_segments_per_block<const AXES: usize>() -> Result<usize, BlockError>
{
    let record_bytes = finite_difference_record_bytes::<AXES>()?;
    Ok(EXECUTION_BLOCK_PAYLOAD_BYTES / record_bytes)
}

impl fmt::Debug for ExecutionBlock {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionBlock")
            .field("header", &self.header())
            .finish_non_exhaustive()
    }
}

impl PartialEq for ExecutionBlock {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Eq for ExecutionBlock {}

/// Allocation-free iterator rebuilding absolute cycles from stored durations.
pub struct MotionSegments<'a, const AXES: usize> {
    bytes: &'a [u8; EXECUTION_BLOCK_BYTES],
    next: u32,
    count: u32,
    tick: StreamTick,
}

impl<const AXES: usize> Iterator for MotionSegments<'_, AXES> {
    type Item = ExecutionSegment<AXES>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.count {
            return None;
        }
        let record_bytes = MOTION_RECORD_PREFIX_BYTES + AXES * 8;
        let offset = EXECUTION_BLOCK_HEADER_BYTES + usize::try_from(self.next).ok()? * record_bytes;
        let duration = read_u64(&self.bytes[..], offset);
        let end_tick = StreamTick(self.tick.0.checked_add(duration)?);
        let mut delta_steps = [0_i64; AXES];
        let mut axis = 0;
        while axis < AXES {
            delta_steps[axis] = read_i64(
                &self.bytes[..],
                offset + MOTION_RECORD_PREFIX_BYTES + axis * 8,
            );
            axis += 1;
        }
        let segment = ExecutionSegment {
            start_tick: self.tick,
            end_tick,
            delta_steps,
            flags: read_u32(&self.bytes[..], offset + 8),
        };
        self.tick = end_tick;
        self.next += 1;
        Some(segment)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = usize::try_from(self.count - self.next).unwrap_or(usize::MAX);
        (remaining, Some(remaining))
    }
}

impl<const AXES: usize> ExactSizeIterator for MotionSegments<'_, AXES> {}

/// Allocation-free iterator rebuilding direct finite-difference records from
/// canonical durations and Q31.32 coefficient fields.
pub struct FiniteDifferenceSegments<'a, const AXES: usize> {
    bytes: &'a [u8; EXECUTION_BLOCK_BYTES],
    next: u32,
    count: u32,
    tick: StreamTick,
}

impl<const AXES: usize> Iterator for FiniteDifferenceSegments<'_, AXES> {
    type Item = FiniteDifferenceSegment<AXES>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.count {
            return None;
        }
        let record_bytes =
            FINITE_DIFFERENCE_RECORD_PREFIX_BYTES + AXES * FINITE_DIFFERENCE_AXIS_BYTES;
        let offset = EXECUTION_BLOCK_HEADER_BYTES + usize::try_from(self.next).ok()? * record_bytes;
        let update_period_ticks = read_u32(&self.bytes[..], offset);
        let update_count = read_u32(&self.bytes[..], offset + 4);
        let duration = u64::from(update_period_ticks).checked_mul(u64::from(update_count))?;
        let end_tick = StreamTick(self.tick.0.checked_add(duration)?);
        let mut axes = [FiniteDifferenceAxis::default(); AXES];
        let mut axis = 0;
        while axis < AXES {
            let axis_offset = offset
                + FINITE_DIFFERENCE_RECORD_PREFIX_BYTES
                + axis * FINITE_DIFFERENCE_AXIS_BYTES;
            axes[axis] = FiniteDifferenceAxis {
                initial_position: read_i64(&self.bytes[..], axis_offset),
                first_difference: read_i64(&self.bytes[..], axis_offset + 8),
                second_difference: read_i64(&self.bytes[..], axis_offset + 16),
                third_difference: read_i64(&self.bytes[..], axis_offset + 24),
            };
            axis += 1;
        }
        let segment = FiniteDifferenceSegment {
            start_tick: self.tick,
            end_tick,
            update_period_ticks,
            update_count,
            axes,
            flags: read_u32(&self.bytes[..], offset + 8),
        };
        self.tick = end_tick;
        self.next += 1;
        Some(segment)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = usize::try_from(self.count - self.next).unwrap_or(usize::MAX);
        (remaining, Some(remaining))
    }
}

impl<const AXES: usize> ExactSizeIterator for FiniteDifferenceSegments<'_, AXES> {}

/// Incremental bridge from arbitrary storage chunks to exact owned blocks.
pub struct PartitionAssembler {
    bytes: [u8; EXECUTION_BLOCK_BYTES],
    filled: usize,
    produced: u32,
    expected: u32,
}

impl PartitionAssembler {
    /// Requires one or more exact 512-byte blocks in the published object.
    pub fn new(object_bytes: u64) -> Result<Self, BlockError> {
        let block_bytes = u64::try_from(EXECUTION_BLOCK_BYTES).expect("block bytes fit u64");
        if object_bytes == 0 || !object_bytes.is_multiple_of(block_bytes) {
            return Err(BlockError::PartitionLength);
        }
        let expected =
            u32::try_from(object_bytes / block_bytes).map_err(|_| BlockError::PartitionLength)?;
        Ok(Self {
            bytes: [0; EXECUTION_BLOCK_BYTES],
            filled: 0,
            produced: 0,
            expected,
        })
    }

    /// Copies a bounded prefix and yields at most one complete ownership unit.
    pub fn push(&mut self, input: &[u8]) -> Result<AssembleOutcome, BlockError> {
        if self.produced >= self.expected {
            return Err(BlockError::PartitionLength);
        }
        let remaining = EXECUTION_BLOCK_BYTES - self.filled;
        let consumed = remaining.min(input.len());
        self.bytes[self.filled..self.filled + consumed].copy_from_slice(&input[..consumed]);
        self.filled += consumed;
        if self.filled != EXECUTION_BLOCK_BYTES {
            return Ok(AssembleOutcome::NeedMore { consumed });
        }
        let bytes = core::mem::replace(&mut self.bytes, [0; EXECUTION_BLOCK_BYTES]);
        self.filled = 0;
        self.produced = self.produced.checked_add(1).ok_or(BlockError::Arithmetic)?;
        Ok(AssembleOutcome::Block {
            consumed,
            block: ExecutionBlock::decode(bytes)?,
        })
    }

    /// Confirms the storage object ended on the exact declared block boundary.
    pub const fn finish(&self) -> Result<u32, BlockError> {
        if self.filled != 0 || self.produced != self.expected {
            return Err(BlockError::PartitionIncomplete {
                received: self.produced,
                expected: self.expected,
            });
        }
        Ok(self.produced)
    }

    /// Complete blocks already emitted to the caller.
    pub const fn produced(&self) -> u32 {
        self.produced
    }

    /// Total complete blocks committed by the published object length.
    pub const fn expected(&self) -> u32 {
        self.expected
    }
}

/// Result of one bounded assembler call.
#[derive(Debug, Eq, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "the completed block transfers inline ownership; boxing would violate no-allocation RT preparation"
)]
pub enum AssembleOutcome {
    /// Input ended before one complete block was owned.
    NeedMore {
        /// Prefix bytes consumed from this caller slice.
        consumed: usize,
    },
    /// One structurally verified block is ready for semantic admission.
    Block {
        /// Prefix bytes consumed from this caller slice.
        consumed: usize,
        /// Newly owned canonical work unit.
        block: ExecutionBlock,
    },
}

/// Canonical block construction, decode, or independent-admission failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockError {
    /// Fixed block magic did not match schema V3.
    Magic,
    /// Block schema version did not match exactly.
    Version {
        /// Received version.
        received: u16,
    },
    /// Execution payload family was not assigned.
    Kind {
        /// Received discriminant.
        received: u8,
    },
    /// Axis width was zero, above the schema maximum, or did not match the decoder.
    AxisCount {
        /// Encoded axis count.
        encoded: u8,
        /// Required generic width.
        expected: usize,
    },
    /// Stream ID used the all-zero sentinel.
    MissingStreamId,
    /// Capability identity used the all-zero sentinel.
    MissingCapabilityDigest,
    /// Configuration identity used the all-zero sentinel.
    MissingConfigDigest,
    /// Sequence zero/nonzero did not agree with the previous-digest sentinel.
    ChainOrigin,
    /// Segment count was empty or unrepresentable.
    SegmentCount,
    /// Payload length was noncanonical or exceeded the fixed block.
    PayloadLength,
    /// A reserved or unused padding byte was nonzero.
    Reserved,
    /// Block digest was zero or failed SHA-256 verification.
    BlockDigest,
    /// One segment set flags not assigned by schema V3.
    SegmentFlags {
        /// Segment index.
        index: usize,
        /// Received flags.
        flags: u32,
    },
    /// One segment was empty, reversed, noncontiguous, or overflowed time.
    SegmentTime {
        /// Segment index.
        index: usize,
    },
    /// Direct finite-difference semantic validation failed at one record.
    FiniteDifference {
        /// Record index.
        index: usize,
        /// Exact coefficient/continuity/bound failure.
        error: FiniteDifferenceError,
    },
    /// Servo finite-difference semantic validation failed at one record.
    ServoFiniteDifference {
        /// Record index.
        index: usize,
        /// Exact cadence, coefficient, continuity, or configured-limit failure.
        error: ServoFiniteDifferenceError,
    },
    /// A complete servo stream did not return both feed-forward channels to zero.
    ServoTerminalFeedForward,
    /// Servo execution was requested without an exact configuration-derived profile.
    ServoProfileRequired,
    /// Servo setpoint count could not retain contiguous nonzero `u32` command IDs.
    ServoCommandCountOverflow,
    /// Block end did not equal the exact sum of segment durations.
    BlockTime,
    /// Prepared stream identity did not match.
    StreamIdentity,
    /// Active capability identity did not match.
    CapabilityIdentity,
    /// Active configuration identity did not match.
    ConfigurationIdentity,
    /// Stream sequence was not exactly next.
    Sequence {
        /// Received value.
        received: u32,
        /// Required value.
        expected: u32,
    },
    /// Sequence could not advance without wrap.
    SequenceOverflow,
    /// Block start tick was not contiguous with the admitted horizon.
    StartTick {
        /// Received value.
        received: StreamTick,
        /// Required value.
        expected: StreamTick,
    },
    /// Previous block digest did not match the admitted chain.
    PreviousDigest,
    /// Block duration exceeded its fixed ownership-horizon limit.
    BlockTooLong {
        /// Received ticks.
        duration: u64,
        /// Maximum admitted ticks.
        maximum: u64,
    },
    /// Segment duration exceeded the configured limit.
    SegmentTooLong {
        /// Segment index.
        index: usize,
        /// Received ticks.
        duration: u64,
        /// Maximum admitted ticks.
        maximum: u64,
    },
    /// One displacement exceeded the configured lattice bound.
    TooManySteps {
        /// Segment index.
        index: usize,
        /// Axis index.
        axis: usize,
        /// Received magnitude.
        magnitude: u64,
        /// Maximum admitted magnitude.
        maximum: u64,
    },
    /// Accumulated block displacement overflowed `i64`.
    PositionOverflow {
        /// Segment index.
        index: usize,
        /// Axis index.
        axis: usize,
    },
    /// Published object length was not a nonempty exact block multiple.
    PartitionLength,
    /// Storage ended before every declared block was emitted.
    PartitionIncomplete {
        /// Complete blocks emitted.
        received: u32,
        /// Complete blocks required.
        expected: u32,
    },
    /// A caller attempted to append beyond the immutable declared block count.
    StreamComplete,
    /// A validator was finalized before the immutable block count was reached.
    StreamIncomplete {
        /// Blocks accepted.
        received: u32,
        /// Blocks required.
        expected: u32,
    },
    /// Cumulative displacement across blocks overflowed one axis.
    StreamPositionOverflow {
        /// Axis index.
        axis: usize,
    },
    /// Adding a relative stream tick to the committed device epoch overflowed.
    EpochOverflow,
    /// Checked size, time, or counter arithmetic overflowed.
    Arithmetic,
}

fn validate_axis_count<const AXES: usize>() -> Result<(), BlockError> {
    if AXES == 0 || AXES > MAX_EXECUTION_AXES {
        return Err(BlockError::AxisCount {
            encoded: u8::try_from(AXES).unwrap_or(u8::MAX),
            expected: AXES,
        });
    }
    Ok(())
}

fn motion_record_bytes<const AXES: usize>() -> Result<usize, BlockError> {
    validate_axis_count::<AXES>()?;
    AXES.checked_mul(8)
        .and_then(|axes| axes.checked_add(MOTION_RECORD_PREFIX_BYTES))
        .ok_or(BlockError::Arithmetic)
}

fn finite_difference_record_bytes<const AXES: usize>() -> Result<usize, BlockError> {
    validate_axis_count::<AXES>()?;
    AXES.checked_mul(FINITE_DIFFERENCE_AXIS_BYTES)
        .and_then(|axes| axes.checked_add(FINITE_DIFFERENCE_RECORD_PREFIX_BYTES))
        .ok_or(BlockError::Arithmetic)
}

pub(crate) fn servo_finite_difference_record_bytes<const AXES: usize>() -> Result<usize, BlockError>
{
    servo::validate_servo_axis_count::<AXES>().map_err(|_| BlockError::AxisCount {
        encoded: u8::try_from(AXES).unwrap_or(u8::MAX),
        expected: MAX_SERVO_EXECUTION_AXES,
    })?;
    AXES.checked_mul(SERVO_FINITE_DIFFERENCE_AXIS_BYTES)
        .and_then(|axes| axes.checked_add(SERVO_FINITE_DIFFERENCE_RECORD_PREFIX_BYTES))
        .ok_or(BlockError::Arithmetic)
}

fn validate_chain_origin(sequence: u32, previous_digest: Digest) -> Result<(), BlockError> {
    if (sequence == 0) != previous_digest.is_zero() {
        return Err(BlockError::ChainOrigin);
    }
    Ok(())
}

fn validate_block_expectation(
    header: ExecutionBlockHeader,
    expected: BlockExpectation,
) -> Result<(), BlockError> {
    if header.stream_id != expected.stream_id {
        return Err(BlockError::StreamIdentity);
    }
    if header.capability_digest != expected.capability_digest {
        return Err(BlockError::CapabilityIdentity);
    }
    if header.config_digest != expected.config_digest {
        return Err(BlockError::ConfigurationIdentity);
    }
    if header.sequence != expected.sequence {
        return Err(BlockError::Sequence {
            received: header.sequence,
            expected: expected.sequence,
        });
    }
    if header.start_tick != expected.start_tick {
        return Err(BlockError::StartTick {
            received: header.start_tick,
            expected: expected.start_tick,
        });
    }
    if header.previous_digest != expected.previous_digest {
        return Err(BlockError::PreviousDigest);
    }
    Ok(())
}

fn decode_block_header(
    bytes: &[u8; EXECUTION_BLOCK_BYTES],
) -> Result<ExecutionBlockHeader, BlockError> {
    if bytes[0..8] != BLOCK_MAGIC {
        return Err(BlockError::Magic);
    }
    let version = read_u16(bytes, 8);
    if version != MACHINE_IR_VERSION {
        return Err(BlockError::Version { received: version });
    }
    let kind = ExecutionKind::from_wire(bytes[10]).ok_or(BlockError::Kind {
        received: bytes[10],
    })?;
    let mut stream_id = [0_u8; 16];
    stream_id.copy_from_slice(&bytes[40..56]);
    let mut capability_digest = [0_u8; 32];
    capability_digest.copy_from_slice(&bytes[56..88]);
    let mut config_digest = [0_u8; 32];
    config_digest.copy_from_slice(&bytes[88..120]);
    let mut previous_digest = [0_u8; 32];
    previous_digest.copy_from_slice(&bytes[120..152]);
    let mut block_digest = [0_u8; 32];
    block_digest.copy_from_slice(&bytes[EXECUTION_BLOCK_DIGEST_OFFSET..]);
    Ok(ExecutionBlockHeader {
        kind,
        axis_count: bytes[11],
        sequence: read_u32(bytes, 12),
        segment_count: read_u32(bytes, 16),
        payload_len: read_u32(bytes, 20),
        start_tick: StreamTick(read_u64(bytes, 24)),
        end_tick: StreamTick(read_u64(bytes, 32)),
        stream_id: StreamId(stream_id),
        capability_digest: Digest(capability_digest),
        config_digest: Digest(config_digest),
        previous_digest: Digest(previous_digest),
        block_digest: Digest(block_digest),
    })
}

fn validate_block_structure(
    bytes: &[u8; EXECUTION_BLOCK_BYTES],
    header: ExecutionBlockHeader,
) -> Result<(), BlockError> {
    let axes = usize::from(header.axis_count);
    if axes == 0 || axes > MAX_EXECUTION_AXES {
        return Err(BlockError::AxisCount {
            encoded: header.axis_count,
            expected: MAX_EXECUTION_AXES,
        });
    }
    if !header.stream_id.is_valid() {
        return Err(BlockError::MissingStreamId);
    }
    if header.capability_digest.is_zero() {
        return Err(BlockError::MissingCapabilityDigest);
    }
    if header.config_digest.is_zero() {
        return Err(BlockError::MissingConfigDigest);
    }
    validate_chain_origin(header.sequence, header.previous_digest)?;
    if header.segment_count == 0 {
        return Err(BlockError::SegmentCount);
    }
    let (record_prefix_bytes, axis_bytes, maximum_axes) = match header.kind {
        ExecutionKind::Motion => (MOTION_RECORD_PREFIX_BYTES, 8, MAX_EXECUTION_AXES),
        ExecutionKind::FiniteDifference => (
            FINITE_DIFFERENCE_RECORD_PREFIX_BYTES,
            FINITE_DIFFERENCE_AXIS_BYTES,
            MAX_EXECUTION_AXES,
        ),
        ExecutionKind::ServoFiniteDifference => (
            SERVO_FINITE_DIFFERENCE_RECORD_PREFIX_BYTES,
            SERVO_FINITE_DIFFERENCE_AXIS_BYTES,
            MAX_SERVO_EXECUTION_AXES,
        ),
    };
    if axes > maximum_axes {
        return Err(BlockError::AxisCount {
            encoded: header.axis_count,
            expected: maximum_axes,
        });
    }
    let record_bytes = axes
        .checked_mul(axis_bytes)
        .and_then(|bytes| bytes.checked_add(record_prefix_bytes))
        .ok_or(BlockError::Arithmetic)?;
    let expected_payload = usize::try_from(header.segment_count)
        .ok()
        .and_then(|count| count.checked_mul(record_bytes))
        .ok_or(BlockError::PayloadLength)?;
    if usize::try_from(header.payload_len).ok() != Some(expected_payload)
        || expected_payload > EXECUTION_BLOCK_PAYLOAD_BYTES
    {
        return Err(BlockError::PayloadLength);
    }
    if bytes[152..EXECUTION_BLOCK_HEADER_BYTES]
        .iter()
        .any(|byte| *byte != 0)
        || bytes[EXECUTION_BLOCK_HEADER_BYTES + expected_payload..EXECUTION_BLOCK_DIGEST_OFFSET]
            .iter()
            .any(|byte| *byte != 0)
    {
        return Err(BlockError::Reserved);
    }
    if header.end_tick.0 <= header.start_tick.0 {
        return Err(BlockError::BlockTime);
    }
    let mut total_duration = 0_u64;
    for index in 0..usize::try_from(header.segment_count).map_err(|_| BlockError::SegmentCount)? {
        let offset = EXECUTION_BLOCK_HEADER_BYTES
            .checked_add(
                index
                    .checked_mul(record_bytes)
                    .ok_or(BlockError::Arithmetic)?,
            )
            .ok_or(BlockError::Arithmetic)?;
        let duration = match header.kind {
            ExecutionKind::Motion => read_u64(bytes, offset),
            ExecutionKind::FiniteDifference | ExecutionKind::ServoFiniteDifference => {
                let update_period = read_u32(bytes, offset);
                let update_count = read_u32(bytes, offset + 4);
                if update_period == 0 || update_count == 0 {
                    return Err(BlockError::SegmentTime { index });
                }
                u64::from(update_period)
                    .checked_mul(u64::from(update_count))
                    .ok_or(BlockError::SegmentTime { index })?
            }
        };
        let flags = read_u32(bytes, offset + 8);
        if flags != 0 {
            return Err(BlockError::SegmentFlags { index, flags });
        }
        if bytes[offset + 12..offset + record_prefix_bytes]
            .iter()
            .any(|byte| *byte != 0)
        {
            return Err(BlockError::Reserved);
        }
        if duration == 0 {
            return Err(BlockError::SegmentTime { index });
        }
        total_duration = total_duration
            .checked_add(duration)
            .ok_or(BlockError::SegmentTime { index })?;
    }
    if header.start_tick.0.checked_add(total_duration) != Some(header.end_tick.0) {
        return Err(BlockError::BlockTime);
    }
    if header.block_digest.is_zero() || hash_block_prefix(bytes) != header.block_digest {
        return Err(BlockError::BlockDigest);
    }
    Ok(())
}

fn hash_block_prefix(bytes: &[u8; EXECUTION_BLOCK_BYTES]) -> Digest {
    let mut hasher = Sha256::new();
    hasher.update(&bytes[..EXECUTION_BLOCK_DIGEST_OFFSET]);
    let result = hasher.finalize();
    let mut digest = [0_u8; 32];
    digest.copy_from_slice(&result);
    Digest(digest)
}

const fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

const fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

const fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

const fn read_i64(bytes: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

/// Rejection reason carrying the exact segment/axis where possible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Partition magic mismatch.
    Magic,
    /// Exact machine-IR version mismatch.
    Version {
        /// Version found in the partition.
        received: u16,
    },
    /// Compile-time axis width did not match the encoded width.
    AxisCount {
        /// Encoded axis count.
        encoded: u8,
        /// Validator axis count.
        expected: usize,
    },
    /// Unknown header flags were set.
    HeaderFlags(u8),
    /// Header count did not match the decoded slice.
    SegmentCount {
        /// Encoded count.
        encoded: u32,
        /// Decoded count.
        actual: usize,
    },
    /// Capability identity was not bound.
    MissingCapabilityDigest,
    /// Machine configuration identity was not bound.
    MissingConfigDigest,
    /// Immutable partition identity was not bound.
    MissingPartitionDigest,
    /// Unknown segment flags were set.
    SegmentFlags {
        /// Segment index.
        index: usize,
        /// Unknown bits.
        flags: u32,
    },
    /// Segment had zero duration or reversed time.
    EmptyOrReversedTime {
        /// Segment index.
        index: usize,
    },
    /// A segment did not start exactly where its predecessor ended.
    NonContiguousTime {
        /// Segment index.
        index: usize,
    },
    /// Segment exceeded the board/config duration bound.
    SegmentTooLong {
        /// Segment index.
        index: usize,
        /// Encoded duration.
        duration: u64,
    },
    /// Axis displacement exceeded its per-segment bound.
    TooManySteps {
        /// Segment index.
        index: usize,
        /// Axis index.
        axis: usize,
        /// Absolute displacement.
        magnitude: u64,
    },
    /// Cumulative relative position exceeded `i64`.
    PositionOverflow {
        /// Segment index.
        index: usize,
        /// Axis index.
        axis: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream_id() -> StreamId {
        StreamId::new([0x11; 16]).unwrap()
    }

    fn block_limits() -> BlockValidationLimits {
        BlockValidationLimits {
            maximum_block_ticks: 1_000,
            segment: limits(),
        }
    }

    fn expected(sequence: u32, start_tick: u64, previous_digest: Digest) -> BlockExpectation {
        BlockExpectation {
            stream_id: stream_id(),
            capability_digest: Digest([1; 32]),
            config_digest: Digest([2; 32]),
            sequence,
            start_tick: StreamTick(start_tick),
            previous_digest,
        }
    }

    fn block(
        sequence: u32,
        previous_digest: Digest,
        segments: &[ExecutionSegment<3>],
    ) -> ExecutionBlock {
        ExecutionBlock::encode_motion(
            stream_id(),
            Digest([1; 32]),
            Digest([2; 32]),
            sequence,
            previous_digest,
            segments,
        )
        .unwrap()
    }

    fn reseal(bytes: &mut [u8; EXECUTION_BLOCK_BYTES]) {
        let digest = hash_block_prefix(bytes);
        bytes[EXECUTION_BLOCK_DIGEST_OFFSET..].copy_from_slice(&digest.0);
    }

    fn header(segment_count: u32) -> JobHeader {
        JobHeader {
            capability_digest: Digest([1; 32]),
            config_digest: Digest([2; 32]),
            partition_digest: Digest([3; 32]),
            ..JobHeader::new(3, segment_count)
        }
    }

    fn limits() -> ValidationLimits {
        ValidationLimits {
            maximum_segment_ticks: 1_000,
            maximum_steps_per_segment: 100,
        }
    }

    #[test]
    fn contiguous_integer_job_returns_terminal_facts() {
        let segments = [
            Segment {
                start_cycle: DeviceCycle(100),
                end_cycle: DeviceCycle(200),
                delta_steps: [10, -4, 0],
                flags: 0,
            },
            Segment {
                start_cycle: DeviceCycle(200),
                end_cycle: DeviceCycle(350),
                delta_steps: [5, 4, 2],
                flags: 0,
            },
        ];
        let job = Job {
            header: header(2),
            segments: &segments,
        };

        assert_eq!(
            job.validate(limits()),
            Ok(JobSummary {
                end_cycle: DeviceCycle(350),
                final_steps: [15, 0, 2]
            })
        );
    }

    #[test]
    fn time_gap_is_rejected_at_exact_segment() {
        let segments = [
            Segment {
                start_cycle: DeviceCycle(10),
                end_cycle: DeviceCycle(20),
                delta_steps: [1, 0, 0],
                flags: 0,
            },
            Segment {
                start_cycle: DeviceCycle(21),
                end_cycle: DeviceCycle(30),
                delta_steps: [1, 0, 0],
                flags: 0,
            },
        ];
        let job = Job {
            header: header(2),
            segments: &segments,
        };

        assert_eq!(
            job.validate(limits()),
            Err(Error::NonContiguousTime { index: 1 })
        );
    }

    #[test]
    fn missing_identity_never_arms() {
        let job = Job::<3> {
            header: JobHeader::new(3, 0),
            segments: &[],
        };
        assert_eq!(job.validate(limits()), Err(Error::MissingCapabilityDigest));
    }

    #[test]
    fn extreme_delta_is_checked_without_signed_abs_overflow() {
        let segments = [Segment {
            start_cycle: DeviceCycle(0),
            end_cycle: DeviceCycle(1),
            delta_steps: [i64::MIN, 0, 0],
            flags: 0,
        }];
        let job = Job {
            header: header(1),
            segments: &segments,
        };
        assert_eq!(
            job.validate(limits()),
            Err(Error::TooManySteps {
                index: 0,
                axis: 0,
                magnitude: 1_u64 << 63
            })
        );
    }

    #[test]
    fn owned_motion_block_has_canonical_bytes_and_independent_summary() {
        let segments = [
            ExecutionSegment {
                start_tick: StreamTick(100),
                end_tick: StreamTick(220),
                delta_steps: [10, -4, 0],
                flags: 0,
            },
            ExecutionSegment {
                start_tick: StreamTick(220),
                end_tick: StreamTick(350),
                delta_steps: [5, 4, 2],
                flags: 0,
            },
        ];
        let block = block(0, Digest::ZERO, &segments);
        let header = block.header();
        assert_eq!(
            core::mem::size_of::<ExecutionBlock>(),
            EXECUTION_BLOCK_BYTES
        );
        assert_eq!(header.kind, ExecutionKind::Motion);
        assert_eq!(header.axis_count, 3);
        assert_eq!(header.sequence, 0);
        assert_eq!(header.segment_count, 2);
        assert_eq!(header.payload_len, 80);
        assert_eq!(header.start_tick, StreamTick(100));
        assert_eq!(header.end_tick, StreamTick(350));
        assert_eq!(
            header.block_digest.0,
            [
                237, 68, 7, 124, 40, 87, 219, 249, 130, 203, 84, 169, 51, 95, 197, 150, 206, 124,
                144, 31, 183, 233, 47, 56, 24, 126, 244, 212, 81, 24, 232, 172,
            ]
        );
        assert_eq!(&block.as_bytes()[152..160], &[0; 8]);
        assert!(
            block.as_bytes()[240..EXECUTION_BLOCK_DIGEST_OFFSET]
                .iter()
                .all(|byte| *byte == 0)
        );
        let mut decoded = block.motion_segments::<3>().unwrap();
        assert_eq!(decoded.next(), Some(segments[0]));
        assert_eq!(decoded.next(), Some(segments[1]));
        assert_eq!(decoded.next(), None);

        let summary = block
            .validate_motion::<3>(expected(0, 100, Digest::ZERO), block_limits())
            .unwrap();
        assert_eq!(summary.end_tick, StreamTick(350));
        assert_eq!(
            summary.end_tick.at_epoch(DeviceCycle(10_000)),
            Ok(DeviceCycle(10_350))
        );
        assert_eq!(
            StreamTick(1).at_epoch(DeviceCycle(u64::MAX)),
            Err(BlockError::EpochOverflow)
        );
        assert_eq!(summary.final_steps, [15, 0, 2]);
        assert_eq!(summary.segment_count, 2);
        assert_eq!(summary.block_digest, header.block_digest);
        assert_eq!(
            summary
                .next_expectation(stream_id(), Digest([1; 32]), Digest([2; 32]))
                .unwrap(),
            expected(1, 350, header.block_digest)
        );
    }

    #[test]
    fn envelope_digest_padding_and_chain_fail_closed() {
        let segments = [ExecutionSegment {
            start_tick: StreamTick(10),
            end_tick: StreamTick(20),
            delta_steps: [1, 2, 3],
            flags: 0,
        }];
        let canonical = block(0, Digest::ZERO, &segments).into_bytes();

        let mut tampered = canonical;
        tampered[EXECUTION_BLOCK_HEADER_BYTES + MOTION_RECORD_PREFIX_BYTES] ^= 1;
        assert_eq!(
            ExecutionBlock::decode(tampered),
            Err(BlockError::BlockDigest)
        );

        let mut reserved = canonical;
        reserved[159] = 1;
        reseal(&mut reserved);
        assert_eq!(ExecutionBlock::decode(reserved), Err(BlockError::Reserved));

        let block = ExecutionBlock::decode(canonical).unwrap();
        let mut wrong = expected(1, 10, Digest([9; 32]));
        assert_eq!(
            block.validate_motion::<3>(wrong, block_limits()),
            Err(BlockError::Sequence {
                received: 0,
                expected: 1
            })
        );
        wrong.sequence = 0;
        assert_eq!(
            block.validate_motion::<3>(wrong, block_limits()),
            Err(BlockError::PreviousDigest)
        );
        wrong.previous_digest = Digest::ZERO;
        wrong.config_digest = Digest([8; 32]);
        assert_eq!(
            block.validate_motion::<3>(wrong, block_limits()),
            Err(BlockError::ConfigurationIdentity)
        );
    }

    #[test]
    fn fixed_payload_capacity_is_exact_for_three_and_eight_axes() {
        assert_eq!(maximum_motion_segments_per_block::<2>(), Ok(10));
        assert_eq!(maximum_motion_segments_per_block::<3>(), Ok(8));
        assert_eq!(maximum_motion_segments_per_block::<8>(), Ok(4));
        assert!(matches!(
            maximum_motion_segments_per_block::<0>(),
            Err(BlockError::AxisCount { .. })
        ));
        assert!(matches!(
            maximum_motion_segments_per_block::<9>(),
            Err(BlockError::AxisCount { .. })
        ));

        let segment3 = ExecutionSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(1),
            delta_steps: [0; 3],
            flags: 0,
        };
        let mut eight3 = [segment3; 8];
        for (index, segment) in eight3.iter_mut().enumerate() {
            segment.start_tick = StreamTick(u64::try_from(index).unwrap());
            segment.end_tick = StreamTick(u64::try_from(index + 1).unwrap());
        }
        assert!(block(0, Digest::ZERO, &eight3).header().payload_len == 320);
        let mut nine3 = [segment3; 9];
        for (index, segment) in nine3.iter_mut().enumerate() {
            segment.start_tick = StreamTick(u64::try_from(index).unwrap());
            segment.end_tick = StreamTick(u64::try_from(index + 1).unwrap());
        }
        assert!(matches!(
            ExecutionBlock::encode_motion(
                stream_id(),
                Digest([1; 32]),
                Digest([2; 32]),
                0,
                Digest::ZERO,
                &nine3
            ),
            Err(BlockError::PayloadLength)
        ));

        let segment8 = ExecutionSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(1),
            delta_steps: [0; 8],
            flags: 0,
        };
        let mut four8 = [segment8; 4];
        for (index, segment) in four8.iter_mut().enumerate() {
            segment.start_tick = StreamTick(u64::try_from(index).unwrap());
            segment.end_tick = StreamTick(u64::try_from(index + 1).unwrap());
        }
        let block = ExecutionBlock::encode_motion(
            stream_id(),
            Digest([1; 32]),
            Digest([2; 32]),
            0,
            Digest::ZERO,
            &four8,
        )
        .unwrap();
        assert_eq!(block.header().payload_len, 320);
    }

    #[test]
    fn arbitrary_storage_splits_assemble_two_owned_blocks() {
        let first_segments = [ExecutionSegment {
            start_tick: StreamTick(100),
            end_tick: StreamTick(200),
            delta_steps: [1, 0, 0],
            flags: 0,
        }];
        let first = block(0, Digest::ZERO, &first_segments);
        let second_segments = [ExecutionSegment {
            start_tick: StreamTick(200),
            end_tick: StreamTick(300),
            delta_steps: [0, 1, 0],
            flags: 0,
        }];
        let second = block(1, first.header().block_digest, &second_segments);
        let mut object = [0_u8; EXECUTION_BLOCK_BYTES * 2];
        object[..EXECUTION_BLOCK_BYTES].copy_from_slice(first.as_bytes());
        object[EXECUTION_BLOCK_BYTES..].copy_from_slice(second.as_bytes());

        let mut assembler = PartitionAssembler::new(1_024).unwrap();
        assert_eq!(
            assembler.push(&object[..300]).unwrap(),
            AssembleOutcome::NeedMore { consumed: 300 }
        );
        let first = match assembler.push(&object[300..]).unwrap() {
            AssembleOutcome::Block { consumed, block } => {
                assert_eq!(consumed, 212);
                block
            }
            AssembleOutcome::NeedMore { .. } => panic!("first block must complete"),
        };
        let second = match assembler.push(&object[512..]).unwrap() {
            AssembleOutcome::Block { consumed, block } => {
                assert_eq!(consumed, 512);
                block
            }
            AssembleOutcome::NeedMore { .. } => panic!("second block must complete"),
        };
        assert_eq!(assembler.finish(), Ok(2));

        let first_summary = first
            .validate_motion::<3>(expected(0, 100, Digest::ZERO), block_limits())
            .unwrap();
        second
            .validate_motion::<3>(expected(1, 200, first_summary.block_digest), block_limits())
            .unwrap();

        let mut stream =
            MotionStreamValidator::<3>::new(2, expected(0, 100, Digest::ZERO), block_limits())
                .unwrap();
        assert!(matches!(
            stream.accept(&second),
            Err(BlockError::Sequence {
                received: 1,
                expected: 0
            })
        ));
        assert_eq!(stream.expectation(), expected(0, 100, Digest::ZERO));
        assert!(!stream.accept(&first).unwrap().complete);
        assert!(matches!(
            stream.accept(&first),
            Err(BlockError::Sequence {
                received: 0,
                expected: 1
            })
        ));
        let progress = stream.accept(&second).unwrap();
        assert!(progress.complete);
        assert_eq!(progress.position, [1, 1, 0]);
        assert_eq!(stream.finish(), Ok(progress));
        assert_eq!(stream.accept(&second), Err(BlockError::StreamComplete));
        assert!(matches!(
            PartitionAssembler::new(513),
            Err(BlockError::PartitionLength)
        ));
    }

    #[test]
    fn segment_and_block_limits_are_checked_after_structural_decode() {
        let segments = [ExecutionSegment {
            start_tick: StreamTick(10),
            end_tick: StreamTick(111),
            delta_steps: [1, 0, 0],
            flags: 0,
        }];
        let block = block(0, Digest::ZERO, &segments);
        let limits = BlockValidationLimits {
            maximum_block_ticks: 2_000,
            segment: ValidationLimits {
                maximum_segment_ticks: 100,
                maximum_steps_per_segment: 100,
            },
        };
        assert_eq!(
            block.validate_motion::<3>(expected(0, 10, Digest::ZERO), limits),
            Err(BlockError::SegmentTooLong {
                index: 0,
                duration: 101,
                maximum: 100
            })
        );
    }

    fn finite_difference_limits<const AXES: usize>(
        required_update_period_ticks: u32,
    ) -> FiniteDifferenceValidationLimits<AXES> {
        FiniteDifferenceValidationLimits {
            maximum_segment_ticks: 10_000,
            maximum_update_count: 1_000,
            required_update_period_ticks,
            maximum_steps_per_segment: 1_000,
            maximum_absolute_first_difference: [FINITE_DIFFERENCE_ONE_STEP.unsigned_abs() - 1;
                AXES],
        }
    }

    #[test]
    fn finite_difference_rounding_is_symmetric_and_ties_to_even() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        assert_eq!(round_finite_difference_position(0), 0);
        assert_eq!(round_finite_difference_position(step / 2), 0);
        assert_eq!(round_finite_difference_position(step), 1);
        assert_eq!(round_finite_difference_position(step + step / 2), 2);
        assert_eq!(round_finite_difference_position(-(step / 2)), 0);
        assert_eq!(round_finite_difference_position(-step), -1);
        assert_eq!(round_finite_difference_position(-(step + step / 2)), -2);
    }

    #[test]
    fn third_order_segment_replays_newton_forward_state_and_exact_endpoint() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        let axis = FiniteDifferenceAxis {
            initial_position: 0,
            first_difference: step / 16,
            second_difference: 0,
            third_difference: step / 128,
        };
        let segment = FiniteDifferenceSegment {
            start_tick: StreamTick(20),
            end_tick: StreamTick(100),
            update_period_ticks: 10,
            update_count: 8,
            axes: [axis, FiniteDifferenceAxis::default()],
            flags: 0,
        };
        let summary = segment
            .validate(StreamTick(20), [0; 2], finite_difference_limits(10))
            .unwrap();
        assert_eq!(summary.end_tick, StreamTick(100));
        assert_eq!(summary.start_steps, [0, 0]);
        assert_eq!(summary.end_steps, [1, 0]);
        assert_eq!(summary.delta_steps, [1, 0]);
        assert_eq!(summary.terminal_position, [step * 15 / 16, 0]);

        let mut position = axis.initial_position;
        let mut first = axis.first_difference;
        let mut second = axis.second_difference;
        for update in 0..=segment.update_count {
            assert_eq!(
                i128::from(position),
                finite_difference_position(axis, update).unwrap()
            );
            if update != segment.update_count {
                position = position.checked_add(first).unwrap();
                first = first.checked_add(second).unwrap();
                second = second.checked_add(axis.third_difference).unwrap();
            }
        }
    }

    #[test]
    fn finite_difference_bounds_find_the_discrete_velocity_vertex() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        let axis = FiniteDifferenceAxis {
            initial_position: 0,
            first_difference: step / 4,
            second_difference: -(step / 16),
            third_difference: step / 64,
        };
        let segment = FiniteDifferenceSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(8),
            update_period_ticks: 1,
            update_count: 8,
            axes: [axis],
            flags: 0,
        };
        let summary = segment
            .validate(StreamTick(0), [0], finite_difference_limits(1))
            .unwrap();
        assert_eq!(summary.minimum_first_difference, [step * 3 / 32]);
        assert_eq!(summary.maximum_first_difference, [step / 4]);
        assert_eq!(summary.terminal_position, [step * 9 / 8]);
        assert_eq!(summary.delta_steps, [1]);
    }

    #[test]
    fn finite_difference_validation_rejects_reversal_rate_and_discontinuity() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        let reversing = FiniteDifferenceSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(4),
            update_period_ticks: 1,
            update_count: 4,
            axes: [FiniteDifferenceAxis {
                initial_position: 0,
                first_difference: step / 4,
                second_difference: -(step / 4),
                third_difference: 0,
            }],
            flags: 0,
        };
        assert_eq!(
            reversing.validate(StreamTick(0), [0], finite_difference_limits(1)),
            Err(FiniteDifferenceError::DirectionReversal { axis: 0 })
        );

        let constant = FiniteDifferenceSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(8),
            update_period_ticks: 1,
            update_count: 8,
            axes: [FiniteDifferenceAxis {
                initial_position: 0,
                first_difference: step / 4,
                second_difference: 0,
                third_difference: 0,
            }],
            flags: 0,
        };
        let mut limits = finite_difference_limits(1);
        limits.maximum_absolute_first_difference = [(step / 4 - 1).unsigned_abs()];
        assert_eq!(
            constant.validate(StreamTick(0), [0], limits),
            Err(FiniteDifferenceError::UpdateRate {
                axis: 0,
                magnitude: (step / 4).unsigned_abs(),
                maximum: (step / 4 - 1).unsigned_abs(),
            })
        );
        assert_eq!(
            constant.validate(StreamTick(0), [1], finite_difference_limits(1)),
            Err(FiniteDifferenceError::PositionContinuity { axis: 0 })
        );
    }

    #[test]
    fn finite_difference_overflow_and_noncanonical_time_fail_before_summary() {
        let overflowing = FiniteDifferenceSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(1),
            update_period_ticks: 1,
            update_count: 1,
            axes: [FiniteDifferenceAxis {
                initial_position: i64::MAX - 1,
                first_difference: 2,
                second_difference: 0,
                third_difference: 0,
            }],
            flags: 0,
        };
        assert_eq!(
            overflowing.validate(StreamTick(0), [i64::MAX - 1], finite_difference_limits(1),),
            Err(FiniteDifferenceError::CoefficientOverflow { axis: 0 })
        );

        let wrong_end = FiniteDifferenceSegment {
            end_tick: StreamTick(2),
            ..overflowing
        };
        assert_eq!(
            wrong_end.validate(StreamTick(0), [i64::MAX - 1], finite_difference_limits(1),),
            Err(FiniteDifferenceError::Time)
        );
    }

    fn finite_block_limits<const AXES: usize>() -> FiniteDifferenceBlockValidationLimits<AXES> {
        FiniteDifferenceBlockValidationLimits {
            maximum_block_ticks: 10_000,
            segment: finite_difference_limits(1),
        }
    }

    fn finite_block_segment(
        start_tick: u64,
        initial_position: [i64; 2],
    ) -> FiniteDifferenceSegment<2> {
        let first_difference = (FINITE_DIFFERENCE_ONE_STEP - 1) / 4;
        FiniteDifferenceSegment {
            start_tick: StreamTick(start_tick),
            end_tick: StreamTick(start_tick + 16),
            update_period_ticks: 1,
            update_count: 16,
            axes: [
                FiniteDifferenceAxis {
                    initial_position: initial_position[0],
                    first_difference,
                    second_difference: 0,
                    third_difference: 0,
                },
                FiniteDifferenceAxis {
                    initial_position: initial_position[1],
                    first_difference: 0,
                    second_difference: 0,
                    third_difference: 0,
                },
            ],
            flags: 0,
        }
    }

    #[test]
    fn finite_difference_block_is_canonical_bounded_and_kind_separated() {
        let first = finite_block_segment(0, [0, 0]);
        let second = finite_block_segment(16, [first.position_at(0, 16).unwrap(), 0]);
        let block = ExecutionBlock::encode_finite_difference(
            stream_id(),
            Digest([1; 32]),
            Digest([2; 32]),
            0,
            Digest::ZERO,
            &[first, second],
        )
        .unwrap();
        let repeated = ExecutionBlock::encode_finite_difference(
            stream_id(),
            Digest([1; 32]),
            Digest([2; 32]),
            0,
            Digest::ZERO,
            &[first, second],
        )
        .unwrap();
        assert_eq!(block, repeated);
        assert_eq!(&block.as_bytes()[0..8], b"ALMBLK03");
        assert_eq!(&block.as_bytes()[8..10], &3_u16.to_le_bytes());
        assert_eq!(block.header().kind, ExecutionKind::FiniteDifference);
        assert_eq!(block.header().payload_len, 160);
        assert_eq!(maximum_finite_difference_segments_per_block::<2>(), Ok(4));
        assert_eq!(maximum_finite_difference_segments_per_block::<3>(), Ok(2));
        assert_eq!(maximum_finite_difference_segments_per_block::<8>(), Ok(1));
        assert!(matches!(
            block.motion_segments::<2>(),
            Err(BlockError::Kind { received: 2 })
        ));
        assert!(matches!(
            block.validate_motion::<2>(expected(0, 0, Digest::ZERO), block_limits()),
            Err(BlockError::Kind { received: 2 })
        ));

        let mut decoded = block.finite_difference_segments::<2>().unwrap();
        assert_eq!(decoded.next(), Some(first));
        assert_eq!(decoded.next(), Some(second));
        assert_eq!(decoded.next(), None);
        let summary = block
            .validate_finite_difference::<2>(
                expected(0, 0, Digest::ZERO),
                [0, 0],
                finite_block_limits(),
            )
            .unwrap();
        assert_eq!(summary.end_tick, StreamTick(32));
        assert_eq!(summary.final_steps, [8, 0]);
        assert_eq!(summary.update_count, 32);
        assert_eq!(
            summary.terminal_finite_position,
            [second.position_at(0, 16).unwrap(), 0]
        );
    }

    #[test]
    fn finite_difference_stream_retains_q31_32_state_across_block_chain() {
        let first_segment = finite_block_segment(0, [0, 0]);
        let first = ExecutionBlock::encode_finite_difference(
            stream_id(),
            Digest([1; 32]),
            Digest([2; 32]),
            0,
            Digest::ZERO,
            &[first_segment],
        )
        .unwrap();
        let terminal = first_segment.position_at(0, 16).unwrap();
        let second_segment = finite_block_segment(16, [terminal, 0]);
        let second = ExecutionBlock::encode_finite_difference(
            stream_id(),
            Digest([1; 32]),
            Digest([2; 32]),
            1,
            first.header().block_digest,
            &[second_segment],
        )
        .unwrap();
        let mut stream = FiniteDifferenceStreamValidator::<2>::new(
            2,
            expected(0, 0, Digest::ZERO),
            finite_block_limits(),
        )
        .unwrap();
        assert!(matches!(
            stream.accept(&second),
            Err(BlockError::Sequence {
                received: 1,
                expected: 0,
            })
        ));
        assert!(!stream.accept(&first).unwrap().complete);

        let wrong_segment = finite_block_segment(16, [terminal + 1, 0]);
        let wrong = ExecutionBlock::encode_finite_difference(
            stream_id(),
            Digest([1; 32]),
            Digest([2; 32]),
            1,
            first.header().block_digest,
            &[wrong_segment],
        )
        .unwrap();
        assert_eq!(
            stream.accept(&wrong),
            Err(BlockError::FiniteDifference {
                index: 0,
                error: FiniteDifferenceError::PositionContinuity { axis: 0 },
            })
        );
        assert_eq!(stream.expectation().sequence, 1);

        let progress = stream.accept(&second).unwrap();
        assert!(progress.complete);
        assert_eq!(progress.end_tick, StreamTick(32));
        assert_eq!(progress.position, [8, 0]);
        assert_eq!(progress.update_count, 32);
        assert_eq!(
            progress.finite_position,
            [second_segment.position_at(0, 16).unwrap(), 0]
        );
        assert_eq!(stream.finish(), Ok(progress));
    }

    #[test]
    fn execution_block_v3_rejects_the_retired_v2_wire_identity() {
        let segments = [ExecutionSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(10),
            delta_steps: [0],
            flags: 0,
        }];
        let block = ExecutionBlock::encode_motion(
            stream_id(),
            Digest([1; 32]),
            Digest([2; 32]),
            0,
            Digest::ZERO,
            &segments,
        )
        .unwrap();
        let mut retired = block.into_bytes();
        retired[0..8].copy_from_slice(b"ALMBLK02");
        assert!(matches!(
            ExecutionBlock::decode(retired),
            Err(BlockError::Magic)
        ));
    }
}
