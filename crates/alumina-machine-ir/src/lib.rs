#![no_std]
#![doc = "Canonical bounded integer work accepted by an Alumina real-time executor."]

use core::fmt;

use alumina_protocol::{DeviceCycle, Digest};
use sha2::{Digest as _, Sha256};

/// Magic identifying a per-MCU Alumina machine-IR partition.
pub const JOB_MAGIC: [u8; 4] = *b"AJOB";

/// Exact machine-IR schema implemented here.
pub const MACHINE_IR_VERSION: u16 = 1;

/// Exact bytes in one independently owned core-0-to-core-1 work block.
pub const EXECUTION_BLOCK_BYTES: usize = 512;
/// Fixed header bytes preceding motion records.
pub const EXECUTION_BLOCK_HEADER_BYTES: usize = 160;
/// Final SHA-256 digest offset and usable end of the padded payload.
pub const EXECUTION_BLOCK_DIGEST_OFFSET: usize = EXECUTION_BLOCK_BYTES - 32;
/// Maximum canonical record bytes after the header and before the digest.
pub const EXECUTION_BLOCK_PAYLOAD_BYTES: usize =
    EXECUTION_BLOCK_DIGEST_OFFSET - EXECUTION_BLOCK_HEADER_BYTES;
/// Largest axis vector admitted by execution-block schema V1.
pub const MAX_EXECUTION_AXES: usize = 8;

const BLOCK_MAGIC: [u8; 8] = *b"ALMBLK01";
const MOTION_RECORD_PREFIX_BYTES: usize = 16;

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
    /// Reserved flags; V1 requires zero.
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
    /// Constructs a V1 header. A serializer fills `partition_digest` after encoding.
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

/// Smallest auditable V1 coordinated integer-motion segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Segment<const AXES: usize> {
    /// Inclusive local-device start tick.
    pub start_cycle: DeviceCycle,
    /// Exclusive local-device end tick.
    pub end_cycle: DeviceCycle,
    /// Signed commanded lattice displacement for each axis.
    pub delta_steps: [i64; AXES],
    /// Reserved V1 segment flags; must be zero.
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
    /// Reserved V1 flags; must be zero.
    pub flags: u32,
}

/// Exact execution payload family admitted by machine-block schema V1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ExecutionKind {
    /// Coordinated integer lattice displacement segments.
    Motion = 1,
}

impl ExecutionKind {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Motion),
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
    /// Fixed block magic did not match schema V1.
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
    /// Axis width was zero, above V1's maximum, or did not match the decoder.
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
    /// One segment set flags not assigned by V1.
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

fn validate_chain_origin(sequence: u32, previous_digest: Digest) -> Result<(), BlockError> {
    if (sequence == 0) != previous_digest.is_zero() {
        return Err(BlockError::ChainOrigin);
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
    let record_bytes = axes
        .checked_mul(8)
        .and_then(|axis_bytes| axis_bytes.checked_add(MOTION_RECORD_PREFIX_BYTES))
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
        let duration = read_u64(bytes, offset);
        let flags = read_u32(bytes, offset + 8);
        if flags != 0 {
            return Err(BlockError::SegmentFlags { index, flags });
        }
        if bytes[offset + 12..offset + MOTION_RECORD_PREFIX_BYTES]
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
                135, 17, 229, 97, 224, 128, 22, 170, 4, 80, 133, 63, 233, 40, 118, 41, 108, 215,
                174, 69, 81, 95, 48, 179, 41, 133, 246, 75, 107, 232, 214, 65,
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
}
