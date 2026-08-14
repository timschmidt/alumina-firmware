//! Canonical third-order servo-setpoint recurrences.
//!
//! These records are intentionally distinct from direct step-crossing finite
//! differences. Position is Q31.32 in the configured servo-axis lattice;
//! velocity and quadrature-current feed-forward are Q2.30. A segment emits its
//! state at update indices `0..update_count` and retains the state at
//! `update_count` solely as the exact continuation into the next segment.

use super::{
    BLOCK_MAGIC, BlockError, BlockExpectation, EXECUTION_BLOCK_BYTES,
    EXECUTION_BLOCK_DIGEST_OFFSET, EXECUTION_BLOCK_HEADER_BYTES, EXECUTION_BLOCK_PAYLOAD_BYTES,
    ExecutionBlock, ExecutionKind, FiniteDifferenceAxis, FiniteDifferenceError, MACHINE_IR_VERSION,
    StreamId, StreamTick, decode_block_header, finite_difference_first_bounds,
    finite_difference_position, finite_difference_state_fits, hash_block_prefix,
    i64_from_finite_difference, read_i64, read_u32, servo_finite_difference_record_bytes,
    validate_block_expectation, validate_block_structure, validate_chain_origin,
};
use alumina_protocol::Digest;

/// Largest homogeneous FOC-axis vector represented by one servo record.
pub const MAX_SERVO_EXECUTION_AXES: usize = 4;
/// Fractional bits in every servo-position coefficient.
pub const SERVO_FINITE_DIFFERENCE_POSITION_FRACTION_BITS: u32 = 32;
/// Fractional bits in every normalized feed-forward coefficient.
pub const SERVO_FINITE_DIFFERENCE_Q30_FRACTION_BITS: u32 = 30;
/// Exact normalized magnitude one in the Q2.30 wire lattice.
pub const SERVO_FINITE_DIFFERENCE_Q30_ONE_BITS: u32 = 1_u32 << 30;
pub(crate) const SERVO_FINITE_DIFFERENCE_RECORD_PREFIX_BYTES: usize = 16;
pub(crate) const SERVO_FINITE_DIFFERENCE_AXIS_BYTES: usize = 64;

/// One cubic Q2.30 sequence in Newton-forward form.
///
/// At update `k`, the value is
/// `p0 + k*d1 + C(k,2)*d2 + C(k,3)*d3` in raw Q2.30 bits.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ServoQ30FiniteDifferenceAxis {
    pub initial_value: i32,
    pub first_difference: i32,
    pub second_difference: i32,
    pub third_difference: i32,
}

impl ServoQ30FiniteDifferenceAxis {
    const fn widened(self) -> FiniteDifferenceAxis {
        FiniteDifferenceAxis {
            initial_position: self.initial_value as i64,
            first_difference: self.first_difference as i64,
            second_difference: self.second_difference as i64,
            third_difference: self.third_difference as i64,
        }
    }
}

/// Position plus both feed-forward sequences for one servo axis.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ServoFiniteDifferenceAxis {
    /// Absolute Q31.32 configured-axis position.
    pub position: FiniteDifferenceAxis,
    /// Normalized Q2.30 velocity feed-forward.
    pub velocity_feed_forward: ServoQ30FiniteDifferenceAxis,
    /// Normalized Q2.30 quadrature-current feed-forward.
    pub quadrature_current_feed_forward: ServoQ30FiniteDifferenceAxis,
}

/// Exact continuation state shared by adjacent records and blocks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoFiniteDifferenceState<const AXES: usize> {
    pub position: [i64; AXES],
    pub velocity_feed_forward: [i32; AXES],
    pub quadrature_current_feed_forward: [i32; AXES],
}

impl<const AXES: usize> ServoFiniteDifferenceState<AXES> {
    /// Starts a stream at exact absolute positions and zero feed-forward.
    pub const fn at_rest(position: [i64; AXES]) -> Self {
        Self {
            position,
            velocity_feed_forward: [0; AXES],
            quadrature_current_feed_forward: [0; AXES],
        }
    }

    /// Whether every feed-forward channel is exactly zero.
    pub fn feed_forwards_are_zero(self) -> bool {
        self.velocity_feed_forward
            .iter()
            .chain(self.quadrature_current_feed_forward.iter())
            .all(|value| *value == 0)
    }
}

/// One half-open, fixed-cadence servo setpoint segment.
///
/// Setpoints are emitted at `start_tick + k * update_period_ticks` for
/// `k = 0..update_count`. The terminal recurrence state at `end_tick` is not
/// emitted by this record; it must be the next record's initial state or the
/// final at-rest state of the complete stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoFiniteDifferenceSegment<const AXES: usize> {
    pub start_tick: StreamTick,
    pub end_tick: StreamTick,
    pub update_period_ticks: u32,
    pub update_count: u32,
    pub axes: [ServoFiniteDifferenceAxis; AXES],
    /// Reserved; schema V3 requires zero.
    pub flags: u32,
}

impl<const AXES: usize> ServoFiniteDifferenceSegment<AXES> {
    /// Evaluate the complete exact setpoint state at an admitted update index.
    ///
    /// `update_count` itself is accepted so validators and executors can replay
    /// the continuation state, although that terminal state is not emitted by
    /// this record.
    pub fn state_at(
        &self,
        update: u32,
    ) -> Result<ServoFiniteDifferenceState<AXES>, ServoFiniteDifferenceError> {
        validate_servo_axis_count::<AXES>()?;
        if update > self.update_count {
            return Err(ServoFiniteDifferenceError::UpdatePolicy);
        }
        let mut state = ServoFiniteDifferenceState::at_rest([0; AXES]);
        let mut axis = 0;
        while axis < AXES {
            state.position[axis] = evaluate_i64(self.axes[axis].position, update, axis)?;
            state.velocity_feed_forward[axis] = evaluate_i32(
                self.axes[axis].velocity_feed_forward,
                update,
                axis,
                ServoSignal::VelocityFeedForward,
            )?;
            state.quadrature_current_feed_forward[axis] = evaluate_i32(
                self.axes[axis].quadrature_current_feed_forward,
                update,
                axis,
                ServoSignal::QuadratureCurrentFeedForward,
            )?;
            axis += 1;
        }
        Ok(state)
    }

    /// Validate exact cadence, continuation, recurrence bounds, and limits.
    ///
    /// Every sequence must be monotonic within one record. An authoritative
    /// compiler splits at an extremum; this makes endpoint range proofs exact
    /// and keeps admission logarithmic in `update_count`.
    pub fn validate(
        &self,
        expected_start_tick: StreamTick,
        expected_state: ServoFiniteDifferenceState<AXES>,
        limits: ServoFiniteDifferenceValidationLimits<AXES>,
    ) -> Result<ServoFiniteDifferenceSegmentSummary<AXES>, ServoFiniteDifferenceError> {
        validate_limits(limits)?;
        if self.flags != 0 {
            return Err(ServoFiniteDifferenceError::Flags(self.flags));
        }
        if self.start_tick != expected_start_tick {
            return Err(ServoFiniteDifferenceError::StartTick {
                received: self.start_tick,
                expected: expected_start_tick,
            });
        }
        if self.update_period_ticks != limits.required_update_period_ticks
            || self.update_count == 0
            || self.update_count > limits.maximum_update_count
        {
            return Err(ServoFiniteDifferenceError::UpdatePolicy);
        }
        let duration = u64::from(self.update_period_ticks)
            .checked_mul(u64::from(self.update_count))
            .ok_or(ServoFiniteDifferenceError::Arithmetic)?;
        if duration > limits.maximum_segment_ticks
            || self.start_tick.0.checked_add(duration) != Some(self.end_tick.0)
        {
            return Err(ServoFiniteDifferenceError::Time);
        }

        let mut terminal_state = expected_state;
        let mut minimum_position_first_difference = [0_i64; AXES];
        let mut maximum_position_first_difference = [0_i64; AXES];
        let mut minimum_velocity_feed_forward = [0_i32; AXES];
        let mut maximum_velocity_feed_forward = [0_i32; AXES];
        let mut minimum_quadrature_current_feed_forward = [0_i32; AXES];
        let mut maximum_quadrature_current_feed_forward = [0_i32; AXES];

        let mut axis = 0;
        while axis < AXES {
            let record = self.axes[axis];
            if record.position.initial_position != expected_state.position[axis] {
                return Err(ServoFiniteDifferenceError::Continuity {
                    axis,
                    signal: ServoSignal::Position,
                });
            }
            if record.velocity_feed_forward.initial_value
                != expected_state.velocity_feed_forward[axis]
            {
                return Err(ServoFiniteDifferenceError::Continuity {
                    axis,
                    signal: ServoSignal::VelocityFeedForward,
                });
            }
            if record.quadrature_current_feed_forward.initial_value
                != expected_state.quadrature_current_feed_forward[axis]
            {
                return Err(ServoFiniteDifferenceError::Continuity {
                    axis,
                    signal: ServoSignal::QuadratureCurrentFeedForward,
                });
            }

            let position = validate_position_axis(
                record.position,
                self.update_count,
                limits.maximum_position_delta_bits[axis],
                limits.maximum_absolute_position_first_difference_bits[axis],
                axis,
            )?;
            terminal_state.position[axis] = position.terminal;
            minimum_position_first_difference[axis] = position.minimum_first_difference;
            maximum_position_first_difference[axis] = position.maximum_first_difference;

            let velocity = validate_q30_axis(
                record.velocity_feed_forward,
                self.update_count,
                limits.maximum_absolute_velocity_feed_forward_bits[axis],
                axis,
                ServoSignal::VelocityFeedForward,
            )?;
            terminal_state.velocity_feed_forward[axis] = velocity.terminal;
            minimum_velocity_feed_forward[axis] = velocity.minimum;
            maximum_velocity_feed_forward[axis] = velocity.maximum;

            let current = validate_q30_axis(
                record.quadrature_current_feed_forward,
                self.update_count,
                limits.maximum_absolute_quadrature_current_feed_forward_bits[axis],
                axis,
                ServoSignal::QuadratureCurrentFeedForward,
            )?;
            terminal_state.quadrature_current_feed_forward[axis] = current.terminal;
            minimum_quadrature_current_feed_forward[axis] = current.minimum;
            maximum_quadrature_current_feed_forward[axis] = current.maximum;
            axis += 1;
        }

        Ok(ServoFiniteDifferenceSegmentSummary {
            end_tick: self.end_tick,
            terminal_state,
            minimum_position_first_difference,
            maximum_position_first_difference,
            minimum_velocity_feed_forward,
            maximum_velocity_feed_forward,
            minimum_quadrature_current_feed_forward,
            maximum_quadrature_current_feed_forward,
        })
    }
}

/// Exact runtime/configuration limits for one servo recurrence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoFiniteDifferenceValidationLimits<const AXES: usize> {
    pub maximum_segment_ticks: u64,
    pub maximum_update_count: u32,
    pub required_update_period_ticks: u32,
    pub maximum_position_delta_bits: [u64; AXES],
    pub maximum_absolute_position_first_difference_bits: [u64; AXES],
    pub maximum_absolute_velocity_feed_forward_bits: [u32; AXES],
    pub maximum_absolute_quadrature_current_feed_forward_bits: [u32; AXES],
}

/// Complete sparse validation result for one servo segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoFiniteDifferenceSegmentSummary<const AXES: usize> {
    pub end_tick: StreamTick,
    pub terminal_state: ServoFiniteDifferenceState<AXES>,
    pub minimum_position_first_difference: [i64; AXES],
    pub maximum_position_first_difference: [i64; AXES],
    pub minimum_velocity_feed_forward: [i32; AXES],
    pub maximum_velocity_feed_forward: [i32; AXES],
    pub minimum_quadrature_current_feed_forward: [i32; AXES],
    pub maximum_quadrature_current_feed_forward: [i32; AXES],
}

/// Signal identifying a per-axis recurrence rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoSignal {
    Position,
    VelocityFeedForward,
    QuadratureCurrentFeedForward,
}

/// Canonical servo recurrence rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoFiniteDifferenceError {
    AxisCount,
    Limits,
    Flags(u32),
    StartTick {
        received: StreamTick,
        expected: StreamTick,
    },
    UpdatePolicy,
    Time,
    Continuity {
        axis: usize,
        signal: ServoSignal,
    },
    CoefficientOverflow {
        axis: usize,
        signal: ServoSignal,
    },
    DirectionReversal {
        axis: usize,
        signal: ServoSignal,
    },
    PositionDelta {
        axis: usize,
        magnitude: u64,
        maximum: u64,
    },
    PositionUpdateRate {
        axis: usize,
        magnitude: u64,
        maximum: u64,
    },
    FeedForwardRange {
        axis: usize,
        signal: ServoSignal,
        magnitude: u32,
        maximum: u32,
    },
    Arithmetic,
}

#[derive(Clone, Copy)]
struct PositionValidation {
    terminal: i64,
    minimum_first_difference: i64,
    maximum_first_difference: i64,
}

#[derive(Clone, Copy)]
struct Q30Validation {
    terminal: i32,
    minimum: i32,
    maximum: i32,
}

fn validate_limits<const AXES: usize>(
    limits: ServoFiniteDifferenceValidationLimits<AXES>,
) -> Result<(), ServoFiniteDifferenceError> {
    validate_servo_axis_count::<AXES>()?;
    if limits.maximum_segment_ticks == 0
        || limits.maximum_update_count == 0
        || limits.required_update_period_ticks == 0
        || limits.maximum_position_delta_bits.contains(&0)
        || limits
            .maximum_absolute_position_first_difference_bits
            .contains(&0)
        || limits
            .maximum_absolute_velocity_feed_forward_bits
            .iter()
            .any(|value| *value == 0 || *value > SERVO_FINITE_DIFFERENCE_Q30_ONE_BITS)
        || limits
            .maximum_absolute_quadrature_current_feed_forward_bits
            .iter()
            .any(|value| *value > SERVO_FINITE_DIFFERENCE_Q30_ONE_BITS)
    {
        return Err(ServoFiniteDifferenceError::Limits);
    }
    Ok(())
}

pub(crate) fn validate_servo_axis_count<const AXES: usize>()
-> Result<(), ServoFiniteDifferenceError> {
    if AXES == 0 || AXES > MAX_SERVO_EXECUTION_AXES {
        Err(ServoFiniteDifferenceError::AxisCount)
    } else {
        Ok(())
    }
}

fn validate_position_axis(
    coefficients: FiniteDifferenceAxis,
    update_count: u32,
    maximum_delta: u64,
    maximum_first_difference: u64,
    axis: usize,
) -> Result<PositionValidation, ServoFiniteDifferenceError> {
    finite_difference_state_fits(coefficients, update_count)
        .map_err(|_| overflow(axis, ServoSignal::Position))?;
    let terminal = evaluate_i64(coefficients, update_count, axis)?;
    let (minimum, maximum) = finite_difference_first_bounds(coefficients, update_count)
        .and_then(|(minimum, maximum)| {
            Ok((
                i64_from_finite_difference(minimum)?,
                i64_from_finite_difference(maximum)?,
            ))
        })
        .map_err(|_| overflow(axis, ServoSignal::Position))?;
    validate_direction(
        coefficients.initial_position,
        terminal,
        minimum,
        maximum,
        axis,
        ServoSignal::Position,
    )?;
    let delta = i128::from(terminal) - i128::from(coefficients.initial_position);
    let magnitude =
        u64::try_from(delta.unsigned_abs()).map_err(|_| ServoFiniteDifferenceError::Arithmetic)?;
    if magnitude > maximum_delta {
        return Err(ServoFiniteDifferenceError::PositionDelta {
            axis,
            magnitude,
            maximum: maximum_delta,
        });
    }
    let rate = minimum.unsigned_abs().max(maximum.unsigned_abs());
    if rate > maximum_first_difference {
        return Err(ServoFiniteDifferenceError::PositionUpdateRate {
            axis,
            magnitude: rate,
            maximum: maximum_first_difference,
        });
    }
    Ok(PositionValidation {
        terminal,
        minimum_first_difference: minimum,
        maximum_first_difference: maximum,
    })
}

fn validate_q30_axis(
    coefficients: ServoQ30FiniteDifferenceAxis,
    update_count: u32,
    maximum_magnitude: u32,
    axis: usize,
    signal: ServoSignal,
) -> Result<Q30Validation, ServoFiniteDifferenceError> {
    let widened = coefficients.widened();
    finite_difference_state_fits(widened, update_count).map_err(|_| overflow(axis, signal))?;
    let terminal = evaluate_i32(coefficients, update_count, axis, signal)?;
    let (minimum_first, maximum_first) = finite_difference_first_bounds(widened, update_count)
        .and_then(|(minimum, maximum)| {
            Ok((
                i64_from_finite_difference(minimum)?,
                i64_from_finite_difference(maximum)?,
            ))
        })
        .map_err(|_| overflow(axis, signal))?;
    validate_direction(
        i64::from(coefficients.initial_value),
        i64::from(terminal),
        minimum_first,
        maximum_first,
        axis,
        signal,
    )?;
    let minimum = coefficients.initial_value.min(terminal);
    let maximum = coefficients.initial_value.max(terminal);
    let magnitude = absolute_i32(minimum).max(absolute_i32(maximum));
    if magnitude > maximum_magnitude {
        return Err(ServoFiniteDifferenceError::FeedForwardRange {
            axis,
            signal,
            magnitude,
            maximum: maximum_magnitude,
        });
    }
    Ok(Q30Validation {
        terminal,
        minimum,
        maximum,
    })
}

fn validate_direction(
    initial: i64,
    terminal: i64,
    minimum_first: i64,
    maximum_first: i64,
    axis: usize,
    signal: ServoSignal,
) -> Result<(), ServoFiniteDifferenceError> {
    let reverses = match terminal.cmp(&initial) {
        core::cmp::Ordering::Greater => minimum_first < 0,
        core::cmp::Ordering::Less => maximum_first > 0,
        core::cmp::Ordering::Equal => minimum_first != 0 || maximum_first != 0,
    };
    if reverses {
        Err(ServoFiniteDifferenceError::DirectionReversal { axis, signal })
    } else {
        Ok(())
    }
}

fn evaluate_i64(
    coefficients: FiniteDifferenceAxis,
    update: u32,
    axis: usize,
) -> Result<i64, ServoFiniteDifferenceError> {
    finite_difference_position(coefficients, update)
        .and_then(i64_from_finite_difference)
        .map_err(|_| overflow(axis, ServoSignal::Position))
}

fn evaluate_i32(
    coefficients: ServoQ30FiniteDifferenceAxis,
    update: u32,
    axis: usize,
    signal: ServoSignal,
) -> Result<i32, ServoFiniteDifferenceError> {
    let value = finite_difference_position(coefficients.widened(), update)
        .map_err(|_| overflow(axis, signal))?;
    i32::try_from(value).map_err(|_| overflow(axis, signal))
}

const fn overflow(axis: usize, signal: ServoSignal) -> ServoFiniteDifferenceError {
    ServoFiniteDifferenceError::CoefficientOverflow { axis, signal }
}

const fn absolute_i32(value: i32) -> u32 {
    value.unsigned_abs()
}

impl From<FiniteDifferenceError> for ServoFiniteDifferenceError {
    fn from(_: FiniteDifferenceError) -> Self {
        Self::Arithmetic
    }
}

/// Board/config-derived bounds for one independently admitted servo block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoFiniteDifferenceBlockValidationLimits<const AXES: usize> {
    /// Longest admitted block ownership horizon in device ticks.
    pub maximum_block_ticks: u64,
    /// Per-record recurrence, cadence, and configured-axis bounds.
    pub segment: ServoFiniteDifferenceValidationLimits<AXES>,
}

/// Validated continuation facts for one servo finite-difference block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoFiniteDifferenceExecutionBlockSummary<const AXES: usize> {
    /// Validated block sequence.
    pub sequence: u32,
    /// Exclusive terminal stream-relative tick.
    pub end_tick: StreamTick,
    /// Exact position and feed-forward continuation state.
    pub terminal_state: ServoFiniteDifferenceState<AXES>,
    /// Exact digest required in the following block.
    pub block_digest: Digest,
    /// Number of decoded recurrence records.
    pub segment_count: u32,
    /// Total dense setpoints emitted by this block.
    pub update_count: u64,
}

impl<const AXES: usize> ServoFiniteDifferenceExecutionBlockSummary<AXES> {
    /// Derive the only valid identity, chain, and tick expectation for the next block.
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

impl ExecutionBlock {
    /// Encode one canonical fixed-cadence servo-setpoint block.
    pub fn encode_servo_finite_difference<const AXES: usize>(
        stream_id: StreamId,
        capability_digest: Digest,
        config_digest: Digest,
        sequence: u32,
        previous_digest: Digest,
        segments: &[ServoFiniteDifferenceSegment<AXES>],
    ) -> Result<Self, BlockError> {
        let record_bytes = servo_finite_difference_record_bytes::<AXES>()?;
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
        bytes[10] = ExecutionKind::ServoFiniteDifference as u8;
        bytes[11] = u8::try_from(AXES).map_err(|_| BlockError::AxisCount {
            encoded: u8::MAX,
            expected: MAX_SERVO_EXECUTION_AXES,
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
                    .checked_add(SERVO_FINITE_DIFFERENCE_RECORD_PREFIX_BYTES)
                    .and_then(|base| {
                        base.checked_add(axis.checked_mul(SERVO_FINITE_DIFFERENCE_AXIS_BYTES)?)
                    })
                    .ok_or(BlockError::Arithmetic)?;
                write_i64_coefficients(
                    &mut bytes,
                    axis_offset,
                    [
                        coefficients.position.initial_position,
                        coefficients.position.first_difference,
                        coefficients.position.second_difference,
                        coefficients.position.third_difference,
                    ],
                )?;
                write_i32_coefficients(
                    &mut bytes,
                    axis_offset + 32,
                    coefficients.velocity_feed_forward,
                )?;
                write_i32_coefficients(
                    &mut bytes,
                    axis_offset + 48,
                    coefficients.quadrature_current_feed_forward,
                )?;
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

    /// Revalidate one servo block against identity, exact continuation, and limits.
    pub fn validate_servo_finite_difference<const AXES: usize>(
        &self,
        expected: BlockExpectation,
        expected_initial_state: ServoFiniteDifferenceState<AXES>,
        limits: ServoFiniteDifferenceBlockValidationLimits<AXES>,
    ) -> Result<ServoFiniteDifferenceExecutionBlockSummary<AXES>, BlockError> {
        servo_finite_difference_record_bytes::<AXES>()?;
        let header = decode_block_header(&self.bytes)?;
        validate_block_structure(&self.bytes, header)?;
        if header.kind != ExecutionKind::ServoFiniteDifference {
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

        let mut terminal_state = expected_initial_state;
        let mut update_count = 0_u64;
        let mut next_tick = expected.start_tick;
        let mut decoded_count = 0_u32;
        let mut segments = self.servo_finite_difference_segments::<AXES>()?;
        for (index, segment) in (&mut segments).enumerate() {
            let summary = segment
                .validate(next_tick, terminal_state, limits.segment)
                .map_err(|error| BlockError::ServoFiniteDifference { index, error })?;
            terminal_state = summary.terminal_state;
            next_tick = summary.end_tick;
            update_count = update_count
                .checked_add(u64::from(segment.update_count))
                .ok_or(BlockError::Arithmetic)?;
            decoded_count = decoded_count.checked_add(1).ok_or(BlockError::Arithmetic)?;
        }
        if decoded_count != header.segment_count || next_tick != header.end_tick {
            return Err(BlockError::BlockTime);
        }
        Ok(ServoFiniteDifferenceExecutionBlockSummary {
            sequence: header.sequence,
            end_tick: header.end_tick,
            terminal_state,
            block_digest: header.block_digest,
            segment_count: header.segment_count,
            update_count,
        })
    }

    /// Iterate decoded servo recurrence records without allocation.
    pub fn servo_finite_difference_segments<const AXES: usize>(
        &self,
    ) -> Result<ServoFiniteDifferenceSegments<'_, AXES>, BlockError> {
        servo_finite_difference_record_bytes::<AXES>()?;
        let header = self.header();
        if header.kind != ExecutionKind::ServoFiniteDifference {
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
        Ok(ServoFiniteDifferenceSegments {
            bytes: &self.bytes,
            next: 0,
            count: header.segment_count,
            tick: header.start_tick,
        })
    }
}

/// Exact servo-recurrence record capacity of one canonical block.
pub fn maximum_servo_finite_difference_segments_per_block<const AXES: usize>()
-> Result<usize, BlockError> {
    Ok(EXECUTION_BLOCK_PAYLOAD_BYTES / servo_finite_difference_record_bytes::<AXES>()?)
}

/// Allocation-free iterator over canonical servo recurrence records.
pub struct ServoFiniteDifferenceSegments<'a, const AXES: usize> {
    bytes: &'a [u8; EXECUTION_BLOCK_BYTES],
    next: u32,
    count: u32,
    tick: StreamTick,
}

impl<const AXES: usize> Iterator for ServoFiniteDifferenceSegments<'_, AXES> {
    type Item = ServoFiniteDifferenceSegment<AXES>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.count {
            return None;
        }
        let record_bytes =
            SERVO_FINITE_DIFFERENCE_RECORD_PREFIX_BYTES + AXES * SERVO_FINITE_DIFFERENCE_AXIS_BYTES;
        let offset = EXECUTION_BLOCK_HEADER_BYTES
            + usize::try_from(self.next).ok()?.checked_mul(record_bytes)?;
        let update_period_ticks = read_u32(&self.bytes[..], offset);
        let update_count = read_u32(&self.bytes[..], offset + 4);
        let duration = u64::from(update_period_ticks).checked_mul(u64::from(update_count))?;
        let end_tick = StreamTick(self.tick.0.checked_add(duration)?);
        let mut axes = [ServoFiniteDifferenceAxis::default(); AXES];
        let mut axis = 0;
        while axis < AXES {
            let axis_offset = offset
                + SERVO_FINITE_DIFFERENCE_RECORD_PREFIX_BYTES
                + axis * SERVO_FINITE_DIFFERENCE_AXIS_BYTES;
            axes[axis] = ServoFiniteDifferenceAxis {
                position: FiniteDifferenceAxis {
                    initial_position: read_i64(&self.bytes[..], axis_offset),
                    first_difference: read_i64(&self.bytes[..], axis_offset + 8),
                    second_difference: read_i64(&self.bytes[..], axis_offset + 16),
                    third_difference: read_i64(&self.bytes[..], axis_offset + 24),
                },
                velocity_feed_forward: read_i32_coefficients(&self.bytes[..], axis_offset + 32),
                quadrature_current_feed_forward: read_i32_coefficients(
                    &self.bytes[..],
                    axis_offset + 48,
                ),
            };
            axis += 1;
        }
        let segment = ServoFiniteDifferenceSegment {
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

impl<const AXES: usize> ExactSizeIterator for ServoFiniteDifferenceSegments<'_, AXES> {}

/// Stateful validator for one complete homogeneous servo cache stream.
pub struct ServoFiniteDifferenceStreamValidator<const AXES: usize> {
    expectation: BlockExpectation,
    limits: ServoFiniteDifferenceBlockValidationLimits<AXES>,
    expected_blocks: u32,
    accepted_blocks: u32,
    state: ServoFiniteDifferenceState<AXES>,
    update_count: u64,
}

impl<const AXES: usize> ServoFiniteDifferenceStreamValidator<AXES> {
    /// Start at relative tick zero and exact configured-axis positions at rest.
    pub fn new(
        expected_blocks: u32,
        expectation: BlockExpectation,
        initial_position: [i64; AXES],
        limits: ServoFiniteDifferenceBlockValidationLimits<AXES>,
    ) -> Result<Self, BlockError> {
        servo_finite_difference_record_bytes::<AXES>()?;
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
        validate_limits(limits.segment)
            .map_err(|error| BlockError::ServoFiniteDifference { index: 0, error })?;
        if limits.maximum_block_ticks == 0 {
            return Err(BlockError::BlockTooLong {
                duration: 0,
                maximum: 0,
            });
        }
        Ok(Self {
            expectation,
            limits,
            expected_blocks,
            accepted_blocks: 0,
            state: ServoFiniteDifferenceState::at_rest(initial_position),
            update_count: 0,
        })
    }

    /// Independently admit exactly the next block, advancing no state on failure.
    pub fn accept(
        &mut self,
        block: &ExecutionBlock,
    ) -> Result<ServoFiniteDifferenceStreamProgress<AXES>, BlockError> {
        if self.accepted_blocks >= self.expected_blocks {
            return Err(BlockError::StreamComplete);
        }
        let summary = block.validate_servo_finite_difference::<AXES>(
            self.expectation,
            self.state,
            self.limits,
        )?;
        let accepted_blocks = self
            .accepted_blocks
            .checked_add(1)
            .ok_or(BlockError::Arithmetic)?;
        if accepted_blocks == self.expected_blocks
            && !summary.terminal_state.feed_forwards_are_zero()
        {
            return Err(BlockError::ServoTerminalFeedForward);
        }
        let update_count = self
            .update_count
            .checked_add(summary.update_count)
            .ok_or(BlockError::Arithmetic)?;
        // Command identity zero is reserved and the executor emits one
        // additional terminal at-rest hold after the half-open dense stream.
        // Therefore the recurrence updates themselves must stop strictly
        // below `u32::MAX`.
        if update_count >= u64::from(u32::MAX) {
            return Err(BlockError::ServoCommandCountOverflow);
        }
        let next = summary.next_expectation(
            self.expectation.stream_id,
            self.expectation.capability_digest,
            self.expectation.config_digest,
        )?;
        self.accepted_blocks = accepted_blocks;
        self.state = summary.terminal_state;
        self.update_count = update_count;
        self.expectation = next;
        Ok(self.progress())
    }

    /// Require the immutable object-derived block count and terminal at-rest state.
    pub fn finish(&self) -> Result<ServoFiniteDifferenceStreamProgress<AXES>, BlockError> {
        if self.accepted_blocks != self.expected_blocks {
            return Err(BlockError::StreamIncomplete {
                received: self.accepted_blocks,
                expected: self.expected_blocks,
            });
        }
        if !self.state.feed_forwards_are_zero() {
            return Err(BlockError::ServoTerminalFeedForward);
        }
        Ok(self.progress())
    }

    /// Exact next required block identity, chain, sequence, and start tick.
    pub const fn expectation(&self) -> BlockExpectation {
        self.expectation
    }

    const fn progress(&self) -> ServoFiniteDifferenceStreamProgress<AXES> {
        ServoFiniteDifferenceStreamProgress {
            accepted_blocks: self.accepted_blocks,
            expected_blocks: self.expected_blocks,
            end_tick: self.expectation.start_tick,
            terminal_state: self.state,
            update_count: self.update_count,
            block_digest: self.expectation.previous_digest,
            complete: self.accepted_blocks == self.expected_blocks,
        }
    }
}

/// Cumulative exact admission facts for a homogeneous servo stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoFiniteDifferenceStreamProgress<const AXES: usize> {
    /// Blocks independently admitted so far.
    pub accepted_blocks: u32,
    /// Exact block count derived from immutable object length.
    pub expected_blocks: u32,
    /// Exclusive terminal stream-relative tick.
    pub end_tick: StreamTick,
    /// Exact terminal position and feed-forward recurrence state.
    pub terminal_state: ServoFiniteDifferenceState<AXES>,
    /// Total dense setpoints represented by admitted blocks.
    pub update_count: u64,
    /// Digest required by the next block, or terminal chain digest.
    pub block_digest: Digest,
    /// Whether the complete declared stream was admitted.
    pub complete: bool,
}

fn write_i64_coefficients(
    bytes: &mut [u8; EXECUTION_BLOCK_BYTES],
    offset: usize,
    coefficients: [i64; 4],
) -> Result<(), BlockError> {
    for (field, value) in coefficients.iter().copied().enumerate() {
        let field_offset = offset
            .checked_add(field.checked_mul(8).ok_or(BlockError::Arithmetic)?)
            .ok_or(BlockError::Arithmetic)?;
        bytes[field_offset..field_offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn write_i32_coefficients(
    bytes: &mut [u8; EXECUTION_BLOCK_BYTES],
    offset: usize,
    coefficients: ServoQ30FiniteDifferenceAxis,
) -> Result<(), BlockError> {
    for (field, value) in [
        coefficients.initial_value,
        coefficients.first_difference,
        coefficients.second_difference,
        coefficients.third_difference,
    ]
    .iter()
    .copied()
    .enumerate()
    {
        let field_offset = offset
            .checked_add(field.checked_mul(4).ok_or(BlockError::Arithmetic)?)
            .ok_or(BlockError::Arithmetic)?;
        bytes[field_offset..field_offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

const fn read_i32_coefficients(bytes: &[u8], offset: usize) -> ServoQ30FiniteDifferenceAxis {
    ServoQ30FiniteDifferenceAxis {
        initial_value: read_i32(bytes, offset),
        first_difference: read_i32(bytes, offset + 4),
        second_difference: read_i32(bytes, offset + 8),
        third_difference: read_i32(bytes, offset + 12),
    }
}

const fn read_i32(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FINITE_DIFFERENCE_ONE_STEP;

    const POSITION_ONE: i64 = FINITE_DIFFERENCE_ONE_STEP;

    fn identity(byte: u8) -> Digest {
        Digest([byte; 32])
    }

    fn stream_id() -> StreamId {
        StreamId([0x53; 16])
    }

    fn position(initial: i64, first: i64) -> FiniteDifferenceAxis {
        FiniteDifferenceAxis {
            initial_position: initial,
            first_difference: first,
            second_difference: 0,
            third_difference: 0,
        }
    }

    fn q30(initial: i32, first: i32) -> ServoQ30FiniteDifferenceAxis {
        ServoQ30FiniteDifferenceAxis {
            initial_value: initial,
            first_difference: first,
            second_difference: 0,
            third_difference: 0,
        }
    }

    fn limits() -> ServoFiniteDifferenceBlockValidationLimits<2> {
        ServoFiniteDifferenceBlockValidationLimits {
            maximum_block_ticks: 1_000,
            segment: ServoFiniteDifferenceValidationLimits {
                maximum_segment_ticks: 1_000,
                maximum_update_count: 100,
                required_update_period_ticks: 10,
                maximum_position_delta_bits: [10 * POSITION_ONE as u64; 2],
                maximum_absolute_position_first_difference_bits: [POSITION_ONE as u64 / 2; 2],
                maximum_absolute_velocity_feed_forward_bits: [1_000; 2],
                maximum_absolute_quadrature_current_feed_forward_bits: [1_000; 2],
            },
        }
    }

    fn first_segment() -> ServoFiniteDifferenceSegment<2> {
        ServoFiniteDifferenceSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(40),
            update_period_ticks: 10,
            update_count: 4,
            axes: [
                ServoFiniteDifferenceAxis {
                    position: position(0, POSITION_ONE / 4),
                    velocity_feed_forward: q30(0, 100),
                    quadrature_current_feed_forward: q30(0, 50),
                },
                ServoFiniteDifferenceAxis {
                    position: position(10 * POSITION_ONE, -POSITION_ONE / 8),
                    velocity_feed_forward: q30(0, -75),
                    quadrature_current_feed_forward: q30(0, 25),
                },
            ],
            flags: 0,
        }
    }

    fn second_segment() -> ServoFiniteDifferenceSegment<2> {
        ServoFiniteDifferenceSegment {
            start_tick: StreamTick(40),
            end_tick: StreamTick(80),
            update_period_ticks: 10,
            update_count: 4,
            axes: [
                ServoFiniteDifferenceAxis {
                    position: position(POSITION_ONE, POSITION_ONE / 4),
                    velocity_feed_forward: q30(400, -100),
                    quadrature_current_feed_forward: q30(200, -50),
                },
                ServoFiniteDifferenceAxis {
                    position: position(10 * POSITION_ONE - POSITION_ONE / 2, -POSITION_ONE / 8),
                    velocity_feed_forward: q30(-300, 75),
                    quadrature_current_feed_forward: q30(100, -25),
                },
            ],
            flags: 0,
        }
    }

    #[test]
    fn canonical_capacity_matches_fixed_axis_layout() {
        assert_eq!(
            maximum_servo_finite_difference_segments_per_block::<1>(),
            Ok(4)
        );
        assert_eq!(
            maximum_servo_finite_difference_segments_per_block::<2>(),
            Ok(2)
        );
        assert_eq!(
            maximum_servo_finite_difference_segments_per_block::<3>(),
            Ok(1)
        );
        assert_eq!(
            maximum_servo_finite_difference_segments_per_block::<4>(),
            Ok(1)
        );
        assert!(maximum_servo_finite_difference_segments_per_block::<0>().is_err());
        assert!(maximum_servo_finite_difference_segments_per_block::<5>().is_err());
    }

    #[test]
    fn block_round_trip_preserves_all_three_recurrences() {
        let segments = [first_segment(), second_segment()];
        let block = ExecutionBlock::encode_servo_finite_difference(
            stream_id(),
            identity(0xCA),
            identity(0xCF),
            0,
            Digest::ZERO,
            &segments,
        )
        .unwrap();
        assert_eq!(block.header().kind, ExecutionKind::ServoFiniteDifference);
        assert_eq!(block.header().payload_len, 288);
        let mut decoded = block.servo_finite_difference_segments::<2>().unwrap();
        assert_eq!(decoded.next(), Some(segments[0]));
        assert_eq!(decoded.next(), Some(segments[1]));
        assert_eq!(decoded.next(), None);
        let summary = block
            .validate_servo_finite_difference(
                BlockExpectation {
                    stream_id: stream_id(),
                    capability_digest: identity(0xCA),
                    config_digest: identity(0xCF),
                    sequence: 0,
                    start_tick: StreamTick(0),
                    previous_digest: Digest::ZERO,
                },
                ServoFiniteDifferenceState::at_rest([0, 10 * POSITION_ONE]),
                limits(),
            )
            .unwrap();
        assert_eq!(summary.end_tick, StreamTick(80));
        assert_eq!(summary.update_count, 8);
        assert_eq!(
            summary.terminal_state.position,
            [2 * POSITION_ONE, 9 * POSITION_ONE]
        );
        assert!(summary.terminal_state.feed_forwards_are_zero());
    }

    #[test]
    fn two_block_stream_retains_exact_state_and_digest_chain() {
        let first = ExecutionBlock::encode_servo_finite_difference(
            stream_id(),
            identity(0xCA),
            identity(0xCF),
            0,
            Digest::ZERO,
            &[first_segment()],
        )
        .unwrap();
        let second = ExecutionBlock::encode_servo_finite_difference(
            stream_id(),
            identity(0xCA),
            identity(0xCF),
            1,
            first.header().block_digest,
            &[second_segment()],
        )
        .unwrap();
        let mut validator = ServoFiniteDifferenceStreamValidator::new(
            2,
            BlockExpectation {
                stream_id: stream_id(),
                capability_digest: identity(0xCA),
                config_digest: identity(0xCF),
                sequence: 0,
                start_tick: StreamTick(0),
                previous_digest: Digest::ZERO,
            },
            [0, 10 * POSITION_ONE],
            limits(),
        )
        .unwrap();
        let progress = validator.accept(&first).unwrap();
        assert_eq!(progress.update_count, 4);
        assert!(!progress.complete);
        let progress = validator.accept(&second).unwrap();
        assert!(progress.complete);
        assert_eq!(progress.end_tick, StreamTick(80));
        assert_eq!(progress.block_digest, second.header().block_digest);
        assert_eq!(validator.finish().unwrap(), progress);
    }

    #[test]
    fn final_feed_forward_is_rejected_without_advancing_stream() {
        let block = ExecutionBlock::encode_servo_finite_difference(
            stream_id(),
            identity(0xCA),
            identity(0xCF),
            0,
            Digest::ZERO,
            &[first_segment()],
        )
        .unwrap();
        let expectation = BlockExpectation {
            stream_id: stream_id(),
            capability_digest: identity(0xCA),
            config_digest: identity(0xCF),
            sequence: 0,
            start_tick: StreamTick(0),
            previous_digest: Digest::ZERO,
        };
        let mut validator = ServoFiniteDifferenceStreamValidator::new(
            1,
            expectation,
            [0, 10 * POSITION_ONE],
            limits(),
        )
        .unwrap();
        assert_eq!(
            validator.accept(&block),
            Err(BlockError::ServoTerminalFeedForward)
        );
        assert_eq!(validator.expectation(), expectation);
        assert_eq!(
            validator.finish(),
            Err(BlockError::StreamIncomplete {
                received: 0,
                expected: 1
            })
        );
    }

    #[test]
    fn dense_updates_reserve_the_terminal_nonzero_command_identity() {
        let segment = ServoFiniteDifferenceSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(10),
            update_period_ticks: 10,
            update_count: 1,
            axes: [
                ServoFiniteDifferenceAxis {
                    position: position(0, 0),
                    velocity_feed_forward: q30(0, 0),
                    quadrature_current_feed_forward: q30(0, 0),
                },
                ServoFiniteDifferenceAxis {
                    position: position(10 * POSITION_ONE, 0),
                    velocity_feed_forward: q30(0, 0),
                    quadrature_current_feed_forward: q30(0, 0),
                },
            ],
            flags: 0,
        };
        let block = ExecutionBlock::encode_servo_finite_difference(
            stream_id(),
            identity(0xCA),
            identity(0xCF),
            0,
            Digest::ZERO,
            &[segment],
        )
        .unwrap();
        let expectation = BlockExpectation {
            stream_id: stream_id(),
            capability_digest: identity(0xCA),
            config_digest: identity(0xCF),
            sequence: 0,
            start_tick: StreamTick(0),
            previous_digest: Digest::ZERO,
        };
        let mut validator = ServoFiniteDifferenceStreamValidator::new(
            2,
            expectation,
            [0, 10 * POSITION_ONE],
            limits(),
        )
        .unwrap();
        validator.update_count = u64::from(u32::MAX - 1);
        assert_eq!(
            validator.accept(&block),
            Err(BlockError::ServoCommandCountOverflow)
        );
        assert_eq!(validator.expectation(), expectation);
        assert_eq!(validator.update_count, u64::from(u32::MAX - 1));
    }

    #[test]
    fn direction_reversal_and_feed_forward_limit_are_sparse_failures() {
        let mut segment = first_segment();
        segment.axes[0].position = FiniteDifferenceAxis {
            initial_position: 0,
            first_difference: POSITION_ONE / 4,
            second_difference: -POSITION_ONE / 4,
            third_difference: 0,
        };
        assert_eq!(
            segment.validate(
                StreamTick(0),
                ServoFiniteDifferenceState::at_rest([0, 10 * POSITION_ONE]),
                limits().segment,
            ),
            Err(ServoFiniteDifferenceError::DirectionReversal {
                axis: 0,
                signal: ServoSignal::Position
            })
        );

        let mut segment = first_segment();
        segment.axes[0].velocity_feed_forward.first_difference = 251;
        assert_eq!(
            segment.validate(
                StreamTick(0),
                ServoFiniteDifferenceState::at_rest([0, 10 * POSITION_ONE]),
                limits().segment,
            ),
            Err(ServoFiniteDifferenceError::FeedForwardRange {
                axis: 0,
                signal: ServoSignal::VelocityFeedForward,
                magnitude: 1_004,
                maximum: 1_000
            })
        );
    }
}
