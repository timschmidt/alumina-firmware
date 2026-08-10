#![no_std]
#![doc = "Bounded cached-job preparation and dual-core admission for Alumina."]

use alumina_machine_ir::{
    AssembleOutcome, BlockError, BlockExpectation, BlockValidationLimits, EXECUTION_BLOCK_BYTES,
    ExecutionBlock, ExecutionBlockHeader, MAX_EXECUTION_AXES, MotionSegments, MotionStreamProgress,
    MotionStreamValidator, PartitionAssembler, StreamId, StreamTick,
};
use alumina_runtime::{RealtimeEndpoint, ServiceEndpoint};
use alumina_storage::media::{AsyncBlockDevice, MAX_MEDIA_CHUNK_BYTES, PublishedReader};
use alumina_storage::provisioning::{ProvisionedCache, ProvisionedCacheError};
use alumina_storage::{ObjectKind, PublishedObject};
use embassy_sync::channel::TrySendError;

/// Complete immutable facts required before core 0 may read executable bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobDescriptor {
    /// Boot-local nonzero lifecycle correlation; not part of cached bytes.
    pub prepare_id: u64,
    /// Exact typed storage publication selected by the controller.
    pub partition: PublishedObject,
    /// Identity repeated by every execution block.
    pub stream_id: StreamId,
    /// Board capabilities used by the authoritative compiler.
    pub capability_digest: alumina_protocol::Digest,
    /// Active machine configuration used by the authoritative compiler.
    pub config_digest: alumina_protocol::Digest,
    /// Exact axis width selected for this per-MCU stream.
    pub axis_count: u8,
    /// Exact count implied by partition bytes; repeated for conflict detection.
    pub block_count: u32,
    /// First relative stream tick; V1 full partitions begin at zero.
    pub first_tick: StreamTick,
    /// Board/config-derived block and segment admission limits.
    pub limits: BlockValidationLimits,
}

impl JobDescriptor {
    /// Validates typed identity, size/count agreement, and all nonzero bounds.
    pub fn validate<const AXES: usize>(self) -> Result<(), DescriptorError> {
        if self.prepare_id == 0 {
            return Err(DescriptorError::PrepareId);
        }
        if self.partition.object.kind != ObjectKind::MachineJobPartition {
            return Err(DescriptorError::ObjectKind);
        }
        if !self.partition.object.content.is_valid() || !self.partition.manifest.is_valid() {
            return Err(DescriptorError::ContentIdentity);
        }
        if AXES == 0 || AXES > MAX_EXECUTION_AXES || usize::from(self.axis_count) != AXES {
            return Err(DescriptorError::AxisCount {
                encoded: self.axis_count,
                expected: AXES,
            });
        }
        StreamId::new(self.stream_id.0).map_err(|_| DescriptorError::StreamIdentity)?;
        if self.capability_digest.is_zero() {
            return Err(DescriptorError::CapabilityIdentity);
        }
        if self.config_digest.is_zero() {
            return Err(DescriptorError::ConfigurationIdentity);
        }
        if self.first_tick != StreamTick(0) {
            return Err(DescriptorError::FirstTick);
        }
        if self.limits.maximum_block_ticks == 0
            || self.limits.segment.maximum_segment_ticks == 0
            || self.limits.segment.maximum_steps_per_segment == 0
        {
            return Err(DescriptorError::Limits);
        }
        let block_bytes =
            u64::try_from(EXECUTION_BLOCK_BYTES).map_err(|_| DescriptorError::PartitionLayout)?;
        let bytes = self.partition.object.byte_len;
        if bytes == 0 || !bytes.is_multiple_of(block_bytes) {
            return Err(DescriptorError::PartitionLayout);
        }
        let derived =
            u32::try_from(bytes / block_bytes).map_err(|_| DescriptorError::PartitionLayout)?;
        if self.block_count == 0 || self.block_count != derived {
            return Err(DescriptorError::BlockCount {
                encoded: self.block_count,
                derived,
            });
        }
        Ok(())
    }

    /// First independently required block facts.
    pub const fn first_expectation(self) -> BlockExpectation {
        BlockExpectation {
            stream_id: self.stream_id,
            capability_digest: self.capability_digest,
            config_digest: self.config_digest,
            sequence: 0,
            start_tick: self.first_tick,
            previous_digest: alumina_protocol::Digest::ZERO,
        }
    }
}

/// Descriptor rejection before storage or a queue is touched.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorError {
    /// Boot-local preparation correlation was zero.
    PrepareId,
    /// Selected object was not executable machine-job data.
    ObjectKind,
    /// Object or manifest identity was unbound.
    ContentIdentity,
    /// Descriptor and compile-time executor axis widths diverged.
    AxisCount {
        /// Descriptor width.
        encoded: u8,
        /// Executor width.
        expected: usize,
    },
    /// Stream identity used the zero sentinel.
    StreamIdentity,
    /// Capability identity used the zero sentinel.
    CapabilityIdentity,
    /// Configuration identity used the zero sentinel.
    ConfigurationIdentity,
    /// V1 full partitions must begin at relative tick zero.
    FirstTick,
    /// A required motion/block bound was zero.
    Limits,
    /// Object length was not a representable nonempty block multiple.
    PartitionLayout,
    /// Explicit count did not equal the count derived from object length.
    BlockCount {
        /// Descriptor count.
        encoded: u32,
        /// Object-derived count.
        derived: u32,
    },
}

/// Minimal fixed-credit sink used by the core-0 prefetch state machine.
pub trait WorkSink {
    /// Available inline ownership slots.
    fn free_capacity(&self) -> usize;

    /// Transfers ownership or returns the unchanged block when no credit exists.
    #[allow(
        clippy::result_large_err,
        reason = "the no-allocation producer must retain the complete inline block on backpressure"
    )]
    fn try_send(&mut self, block: ExecutionBlock) -> Result<(), ExecutionBlock>;
}

impl<
    const COMMANDS: usize,
    const TELEMETRY: usize,
    const COMMAND_PAYLOAD: usize,
    const TELEMETRY_PAYLOAD: usize,
    const WORK_BLOCKS: usize,
> WorkSink
    for ServiceEndpoint<'_, COMMANDS, TELEMETRY, COMMAND_PAYLOAD, TELEMETRY_PAYLOAD, WORK_BLOCKS>
{
    fn free_capacity(&self) -> usize {
        self.work_free_capacity()
    }

    fn try_send(&mut self, block: ExecutionBlock) -> Result<(), ExecutionBlock> {
        self.try_send_work(block).map_err(|error| match error {
            TrySendError::Full(block) => block,
        })
    }
}

/// Minimal fixed-credit source used by the core-1 admission state machine.
pub trait WorkSource {
    /// Takes the next inline-owned block, or `None` when the ring is empty.
    fn try_receive(&mut self) -> Option<ExecutionBlock>;

    /// Blocks still owned by the ring.
    fn depth(&self) -> usize;
}

impl<
    const COMMANDS: usize,
    const TELEMETRY: usize,
    const COMMAND_PAYLOAD: usize,
    const TELEMETRY_PAYLOAD: usize,
    const WORK_BLOCKS: usize,
> WorkSource
    for RealtimeEndpoint<'_, COMMANDS, TELEMETRY, COMMAND_PAYLOAD, TELEMETRY_PAYLOAD, WORK_BLOCKS>
{
    fn try_receive(&mut self) -> Option<ExecutionBlock> {
        self.try_receive_work().ok()
    }

    fn depth(&self) -> usize {
        self.work_depth()
    }
}

/// Core-0 immutable read/assembly/validation lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceJobState {
    /// Publication is open and more blocks remain to validate or transfer.
    Prefetching,
    /// Every declared block was validated and transferred into queue ownership.
    Complete,
    /// Local cancellation discarded unread and pending service-side bytes.
    Cancelled,
    /// Media, machine IR, or an internal invariant failed closed.
    Faulted,
}

/// One bounded service-core prefetch observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceJobStatus<const AXES: usize> {
    /// Boot-local preparation correlation.
    pub prepare_id: u64,
    /// Current service-side lifecycle.
    pub state: ServiceJobState,
    /// Structurally and semantically validated blocks.
    pub validated_blocks: u32,
    /// Blocks whose ownership reached the cross-core ring.
    pub sent_blocks: u32,
    /// Exact total declared by immutable object length.
    pub total_blocks: u32,
    /// Storage chunks read through the verified publication cursor.
    pub storage_chunks_read: u32,
    /// Complete service-side stream facts, only at terminal prefetch.
    pub final_progress: Option<MotionStreamProgress<AXES>>,
}

/// Why one prefetch invocation yielded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrefetchYield {
    /// At least one storage or queue transition advanced.
    Progress,
    /// No queue ownership credit was available.
    Backpressured,
    /// Every immutable block was validated and transferred.
    Complete,
}

/// Status plus the bounded reason for returning to the executor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrefetchObservation<const AXES: usize> {
    /// Yield reason.
    pub yielded: PrefetchYield,
    /// Current exact actor status.
    pub status: ServiceJobStatus<AXES>,
}

/// Sole core-0 owner of one open job cursor and its partial buffers.
pub struct ServicePrefetch<const AXES: usize> {
    descriptor: JobDescriptor,
    reader: PublishedReader,
    assembler: PartitionAssembler,
    validator: MotionStreamValidator<AXES>,
    storage: [u8; MAX_MEDIA_CHUNK_BYTES],
    storage_offset: usize,
    storage_len: usize,
    pending: Option<ExecutionBlock>,
    state: ServiceJobState,
    validated_blocks: u32,
    sent_blocks: u32,
    storage_chunks_read: u32,
    final_progress: Option<MotionStreamProgress<AXES>>,
}

impl<const AXES: usize> ServicePrefetch<AXES> {
    /// Opens the exact publication and allocates only fixed inline state.
    pub async fn open<D>(
        cache: &mut ProvisionedCache<D>,
        descriptor: JobDescriptor,
    ) -> Result<Self, JobError<D::Error>>
    where
        D: AsyncBlockDevice,
    {
        descriptor
            .validate::<AXES>()
            .map_err(JobError::Descriptor)?;
        let reader = cache
            .open_published(descriptor.partition)
            .await
            .map_err(JobError::Storage)?;
        let assembler = PartitionAssembler::new(descriptor.partition.object.byte_len)
            .map_err(JobError::Machine)?;
        let validator = MotionStreamValidator::new(
            descriptor.block_count,
            descriptor.first_expectation(),
            descriptor.limits,
        )
        .map_err(JobError::Machine)?;
        Ok(Self {
            descriptor,
            reader,
            assembler,
            validator,
            storage: [0; MAX_MEDIA_CHUNK_BYTES],
            storage_offset: 0,
            storage_len: 0,
            pending: None,
            state: ServiceJobState::Prefetching,
            validated_blocks: 0,
            sent_blocks: 0,
            storage_chunks_read: 0,
            final_progress: None,
        })
    }

    /// Performs at most one SD chunk read and yields on queue backpressure.
    pub async fn step<D, S>(
        &mut self,
        cache: &mut ProvisionedCache<D>,
        sink: &mut S,
    ) -> Result<PrefetchObservation<AXES>, JobError<D::Error>>
    where
        D: AsyncBlockDevice,
        S: WorkSink,
    {
        match self.state {
            ServiceJobState::Complete => {
                return Ok(self.observation(PrefetchYield::Complete));
            }
            ServiceJobState::Cancelled | ServiceJobState::Faulted => {
                return Err(JobError::State);
            }
            ServiceJobState::Prefetching => {}
        }
        let result = self.step_inner(cache, sink).await;
        if result.is_err() {
            self.state = ServiceJobState::Faulted;
            self.pending = None;
            self.storage.fill(0);
            self.storage_offset = 0;
            self.storage_len = 0;
        }
        result
    }

    async fn step_inner<D, S>(
        &mut self,
        cache: &mut ProvisionedCache<D>,
        sink: &mut S,
    ) -> Result<PrefetchObservation<AXES>, JobError<D::Error>>
    where
        D: AsyncBlockDevice,
        S: WorkSink,
    {
        let mut read_performed = false;
        let mut advanced = false;
        loop {
            if let Some(block) = self.pending.take() {
                match sink.try_send(block) {
                    Ok(()) => {
                        self.sent_blocks = self
                            .sent_blocks
                            .checked_add(1)
                            .ok_or(JobError::Machine(BlockError::Arithmetic))?;
                        advanced = true;
                    }
                    Err(block) => {
                        self.pending = Some(block);
                        return Ok(self.observation(PrefetchYield::Backpressured));
                    }
                }
            }

            if self.storage_offset < self.storage_len {
                let outcome = self
                    .assembler
                    .push(&self.storage[self.storage_offset..self.storage_len])
                    .map_err(JobError::Machine)?;
                let consumed = match &outcome {
                    AssembleOutcome::NeedMore { consumed }
                    | AssembleOutcome::Block { consumed, .. } => *consumed,
                };
                if consumed == 0 {
                    return Err(JobError::Machine(BlockError::Arithmetic));
                }
                self.storage_offset = self
                    .storage_offset
                    .checked_add(consumed)
                    .ok_or(JobError::Machine(BlockError::Arithmetic))?;
                advanced = true;
                if let AssembleOutcome::Block { block, .. } = outcome {
                    let progress = self.validator.accept(&block).map_err(JobError::Machine)?;
                    self.validated_blocks = progress.accepted_blocks;
                    self.pending = Some(block);
                }
                continue;
            }

            self.storage.fill(0);
            self.storage_offset = 0;
            self.storage_len = 0;
            if self.reader.is_complete() {
                self.assembler.finish().map_err(JobError::Machine)?;
                let final_progress = self.validator.finish().map_err(JobError::Machine)?;
                if self.pending.is_some()
                    || self.sent_blocks != self.descriptor.block_count
                    || self.validated_blocks != self.descriptor.block_count
                {
                    return Err(JobError::Machine(BlockError::StreamIncomplete {
                        received: self.sent_blocks,
                        expected: self.descriptor.block_count,
                    }));
                }
                self.final_progress = Some(final_progress);
                self.state = ServiceJobState::Complete;
                return Ok(self.observation(PrefetchYield::Complete));
            }
            if read_performed {
                return Ok(self.observation(if advanced {
                    PrefetchYield::Progress
                } else {
                    PrefetchYield::Backpressured
                }));
            }
            if sink.free_capacity() == 0 {
                return Ok(self.observation(PrefetchYield::Backpressured));
            }
            let chunk = cache
                .read_next_published(&mut self.reader, &mut self.storage)
                .await
                .map_err(JobError::Storage)?
                .ok_or(JobError::Machine(BlockError::PartitionIncomplete {
                    received: self.assembler.produced(),
                    expected: self.assembler.expected(),
                }))?;
            self.storage_len = usize::try_from(chunk.byte_len)
                .map_err(|_| JobError::Machine(BlockError::Arithmetic))?;
            if self.storage_len == 0 || self.storage_len > self.storage.len() {
                return Err(JobError::Machine(BlockError::PartitionLength));
            }
            self.storage_chunks_read = self
                .storage_chunks_read
                .checked_add(1)
                .ok_or(JobError::Machine(BlockError::Arithmetic))?;
            read_performed = true;
            advanced = true;
        }
    }

    /// Cancels unread and pending service-side data; core 1 must drain its ring.
    pub fn cancel(&mut self) {
        if self.state == ServiceJobState::Prefetching {
            self.state = ServiceJobState::Cancelled;
            self.pending = None;
            self.storage.fill(0);
            self.storage_offset = 0;
            self.storage_len = 0;
        }
    }

    /// Exact non-I/O status.
    pub const fn status(&self) -> ServiceJobStatus<AXES> {
        ServiceJobStatus {
            prepare_id: self.descriptor.prepare_id,
            state: self.state,
            validated_blocks: self.validated_blocks,
            sent_blocks: self.sent_blocks,
            total_blocks: self.descriptor.block_count,
            storage_chunks_read: self.storage_chunks_read,
            final_progress: self.final_progress,
        }
    }

    fn observation(&self, yielded: PrefetchYield) -> PrefetchObservation<AXES> {
        PrefetchObservation {
            yielded,
            status: self.status(),
        }
    }
}

/// Core-1 job lifecycle independent of storage and network state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealtimeJobState {
    /// Descriptor installed; no block is currently owned by the executor.
    Prepared,
    /// One independently validated block is owned by the executor.
    Admitted,
    /// Every block was acknowledged consumed in exact order.
    Complete,
    /// Local cancellation invalidated all outstanding work.
    Cancelled,
    /// Validation or token mismatch faulted the stream.
    Faulted,
}

#[derive(Clone, Copy)]
struct Outstanding {
    prepare_id: u64,
    sequence: u32,
    block_digest: alumina_protocol::Digest,
}

/// Core-1 status safe to publish as bounded telemetry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeJobStatus<const AXES: usize> {
    /// Boot-local preparation correlation.
    pub prepare_id: u64,
    /// Current local lifecycle.
    pub state: RealtimeJobState,
    /// Blocks that passed independent core-1 validation.
    pub admitted_blocks: u32,
    /// Blocks acknowledged complete by the future execution engine.
    pub completed_blocks: u32,
    /// Exact immutable total.
    pub total_blocks: u32,
    /// Terminal facts for the completed prefix.
    pub completed_progress: Option<MotionStreamProgress<AXES>>,
    /// Whether one admitted block is currently owned outside this state machine.
    pub outstanding: bool,
}

/// One independently validated block owned by the future RT execution engine.
pub struct AdmittedBlock<const AXES: usize> {
    prepare_id: u64,
    block: ExecutionBlock,
    progress: MotionStreamProgress<AXES>,
}

impl<const AXES: usize> AdmittedBlock<AXES> {
    /// Structurally and semantically validated immutable header.
    pub fn header(&self) -> ExecutionBlockHeader {
        self.block.header()
    }

    /// Iterates exact relative-time segments without copying or allocation.
    pub fn segments(&self) -> Result<MotionSegments<'_, AXES>, BlockError> {
        self.block.motion_segments()
    }

    /// Cumulative independently validated stream facts through this block.
    pub const fn progress(&self) -> MotionStreamProgress<AXES> {
        self.progress
    }
}

/// Result of one nonblocking core-1 queue poll.
#[allow(
    clippy::large_enum_variant,
    reason = "the admitted variant transfers the complete inline block without allocation or aliasing"
)]
pub enum RealtimePoll<const AXES: usize> {
    /// Ring was empty.
    Empty,
    /// A prior admitted block must be completed or cancelled first.
    Outstanding,
    /// Newly owned and independently validated block.
    Block(AdmittedBlock<AXES>),
}

/// Core-1 independent stream validator and one-block ownership gate.
pub struct RealtimeJob<const AXES: usize> {
    descriptor: JobDescriptor,
    validator: MotionStreamValidator<AXES>,
    state: RealtimeJobState,
    outstanding: Option<Outstanding>,
    admitted_blocks: u32,
    completed_blocks: u32,
    completed_progress: Option<MotionStreamProgress<AXES>>,
}

impl<const AXES: usize> RealtimeJob<AXES> {
    /// Installs an independently validated descriptor with no storage authority.
    pub fn prepare(descriptor: JobDescriptor) -> Result<Self, JobError<core::convert::Infallible>> {
        descriptor
            .validate::<AXES>()
            .map_err(JobError::Descriptor)?;
        let validator = MotionStreamValidator::new(
            descriptor.block_count,
            descriptor.first_expectation(),
            descriptor.limits,
        )
        .map_err(JobError::Machine)?;
        Ok(Self {
            descriptor,
            validator,
            state: RealtimeJobState::Prepared,
            outstanding: None,
            admitted_blocks: 0,
            completed_blocks: 0,
            completed_progress: None,
        })
    }

    /// Takes and validates at most one work block without waiting on core 0.
    pub fn poll<S: WorkSource>(
        &mut self,
        source: &mut S,
    ) -> Result<RealtimePoll<AXES>, JobError<core::convert::Infallible>> {
        match self.state {
            RealtimeJobState::Prepared | RealtimeJobState::Admitted => {}
            RealtimeJobState::Complete
            | RealtimeJobState::Cancelled
            | RealtimeJobState::Faulted => return Err(JobError::State),
        }
        if self.outstanding.is_some() {
            return Ok(RealtimePoll::Outstanding);
        }
        let Some(block) = source.try_receive() else {
            return Ok(RealtimePoll::Empty);
        };
        let progress = match self.validator.accept(&block) {
            Ok(progress) => progress,
            Err(error) => {
                self.state = RealtimeJobState::Faulted;
                return Err(JobError::Machine(error));
            }
        };
        let header = block.header();
        self.outstanding = Some(Outstanding {
            prepare_id: self.descriptor.prepare_id,
            sequence: header.sequence,
            block_digest: header.block_digest,
        });
        self.admitted_blocks = progress.accepted_blocks;
        self.state = RealtimeJobState::Admitted;
        Ok(RealtimePoll::Block(AdmittedBlock {
            prepare_id: self.descriptor.prepare_id,
            block,
            progress,
        }))
    }

    /// Acknowledges that the future hardware engine consumed this exact block.
    pub fn acknowledge(
        &mut self,
        admitted: AdmittedBlock<AXES>,
    ) -> Result<RealtimeJobStatus<AXES>, JobError<core::convert::Infallible>> {
        if self.state != RealtimeJobState::Admitted {
            return Err(JobError::State);
        }
        let expected = self.outstanding.ok_or(JobError::AdmissionToken)?;
        let header = admitted.block.header();
        if admitted.prepare_id != expected.prepare_id
            || admitted.prepare_id != self.descriptor.prepare_id
            || header.sequence != expected.sequence
            || header.block_digest != expected.block_digest
            || admitted.progress.accepted_blocks != self.admitted_blocks
        {
            self.state = RealtimeJobState::Faulted;
            self.outstanding = None;
            return Err(JobError::AdmissionToken);
        }
        self.outstanding = None;
        self.completed_blocks = admitted.progress.accepted_blocks;
        self.completed_progress = Some(admitted.progress);
        if admitted.progress.complete {
            self.validator.finish().map_err(JobError::Machine)?;
            self.state = RealtimeJobState::Complete;
        } else {
            self.state = RealtimeJobState::Prepared;
        }
        Ok(self.status())
    }

    /// Invalidates the current token. The safety owner must stop output first.
    pub fn cancel(&mut self) {
        if !matches!(
            self.state,
            RealtimeJobState::Complete | RealtimeJobState::Faulted
        ) {
            self.state = RealtimeJobState::Cancelled;
            self.outstanding = None;
        }
    }

    /// Drops queued blocks after cancellation/fault before another prepare.
    pub fn drain<S: WorkSource>(
        &self,
        source: &mut S,
    ) -> Result<u32, JobError<core::convert::Infallible>> {
        if !matches!(
            self.state,
            RealtimeJobState::Cancelled | RealtimeJobState::Faulted
        ) {
            return Err(JobError::State);
        }
        let mut drained = 0_u32;
        while source.try_receive().is_some() {
            drained = drained
                .checked_add(1)
                .ok_or(JobError::Machine(BlockError::Arithmetic))?;
        }
        Ok(drained)
    }

    /// Exact nonblocking status.
    pub const fn status(&self) -> RealtimeJobStatus<AXES> {
        RealtimeJobStatus {
            prepare_id: self.descriptor.prepare_id,
            state: self.state,
            admitted_blocks: self.admitted_blocks,
            completed_blocks: self.completed_blocks,
            total_blocks: self.descriptor.block_count,
            completed_progress: self.completed_progress,
            outstanding: self.outstanding.is_some(),
        }
    }
}

/// Cached-job lifecycle failure retaining a concrete media-device error.
#[derive(Debug)]
pub enum JobError<E> {
    /// Descriptor was invalid before state mutation.
    Descriptor(DescriptorError),
    /// Published media lookup or readback failed.
    Storage(ProvisionedCacheError<E>),
    /// Canonical block, stream, or fixed-buffer invariant failed.
    Machine(BlockError),
    /// Operation was invalid for the current terminal lifecycle.
    State,
    /// An admitted block did not match the outstanding private token.
    AdmissionToken,
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::boxed::Box;

    use alumina_machine_ir::{ExecutionSegment, ValidationLimits};
    use alumina_protocol::{DeviceCycle, Digest};
    use alumina_runtime::IntercoreBoundary;
    use alumina_storage::{ContentId, DigestAlgorithm, StoredObject};

    use super::*;

    fn descriptor(blocks: u32) -> JobDescriptor {
        JobDescriptor {
            prepare_id: 7,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId {
                        algorithm: DigestAlgorithm::Sha256,
                        digest: Digest([0xaa; 32]),
                    },
                    byte_len: u64::from(blocks) * 512,
                },
                manifest: ContentId {
                    algorithm: DigestAlgorithm::Sha256,
                    digest: Digest([0xbb; 32]),
                },
            },
            stream_id: StreamId([0x11; 16]),
            capability_digest: Digest([0x22; 32]),
            config_digest: Digest([0x33; 32]),
            axis_count: 3,
            block_count: blocks,
            first_tick: StreamTick(0),
            limits: BlockValidationLimits {
                maximum_block_ticks: 1_000,
                segment: ValidationLimits {
                    maximum_segment_ticks: 1_000,
                    maximum_steps_per_segment: 100,
                },
            },
        }
    }

    fn block(sequence: u32, previous: Digest) -> ExecutionBlock {
        ExecutionBlock::encode_motion(
            StreamId([0x11; 16]),
            Digest([0x22; 32]),
            Digest([0x33; 32]),
            sequence,
            previous,
            &[ExecutionSegment {
                start_tick: StreamTick(u64::from(sequence) * 100),
                end_tick: StreamTick((u64::from(sequence) + 1) * 100),
                delta_steps: [1, -1, 0],
                flags: 0,
            }],
        )
        .unwrap()
    }

    #[test]
    fn descriptor_rejects_kind_axis_count_layout_and_nonzero_first_tick() {
        assert_eq!(descriptor(2).validate::<3>(), Ok(()));
        let mut invalid = descriptor(2);
        invalid.partition.object.kind = ObjectKind::OpaqueData;
        assert_eq!(invalid.validate::<3>(), Err(DescriptorError::ObjectKind));
        invalid = descriptor(2);
        invalid.axis_count = 4;
        assert_eq!(
            invalid.validate::<3>(),
            Err(DescriptorError::AxisCount {
                encoded: 4,
                expected: 3
            })
        );
        invalid = descriptor(2);
        invalid.partition.object.byte_len = 513;
        assert_eq!(
            invalid.validate::<3>(),
            Err(DescriptorError::PartitionLayout)
        );
        invalid = descriptor(2);
        invalid.first_tick = StreamTick(1);
        assert_eq!(invalid.validate::<3>(), Err(DescriptorError::FirstTick));
    }

    #[test]
    fn realtime_admission_owns_one_block_and_rejects_wrong_order() {
        type Boundary = IntercoreBoundary<1, 1, 4, 4, 2>;
        let boundary = Box::leak(Box::new(Boundary::new()));
        let (mut service, mut realtime) = boundary.split();
        let first = block(0, Digest::ZERO);
        let first_digest = first.header().block_digest;
        service.try_send_work(first).unwrap();
        service.try_send_work(block(1, first_digest)).unwrap();

        let mut job = RealtimeJob::<3>::prepare(descriptor(2)).unwrap();
        let admitted = match job.poll(&mut realtime).unwrap() {
            RealtimePoll::Block(block) => block,
            _ => panic!("first block must be available"),
        };
        assert_eq!(admitted.header().sequence, 0);
        assert_eq!(admitted.segments().unwrap().len(), 1);
        assert!(matches!(
            job.poll(&mut realtime).unwrap(),
            RealtimePoll::Outstanding
        ));
        let status = job.acknowledge(admitted).unwrap();
        assert_eq!(status.completed_blocks, 1);
        assert_eq!(status.state, RealtimeJobState::Prepared);

        let admitted = match job.poll(&mut realtime).unwrap() {
            RealtimePoll::Block(block) => block,
            _ => panic!("second block must be available"),
        };
        let status = job.acknowledge(admitted).unwrap();
        assert_eq!(status.state, RealtimeJobState::Complete);
        assert_eq!(status.completed_blocks, 2);
        assert_eq!(status.completed_progress.unwrap().position, [2, -2, 0]);
    }

    #[test]
    fn wrong_block_faults_and_cancelled_job_drains_ring() {
        type Boundary = IntercoreBoundary<1, 1, 4, 4, 2>;
        let boundary = Box::leak(Box::new(Boundary::new()));
        let (mut service, mut realtime) = boundary.split();
        let wrong = ExecutionBlock::encode_motion(
            StreamId([0x11; 16]),
            Digest([0x22; 32]),
            Digest([0x44; 32]),
            0,
            Digest::ZERO,
            &[ExecutionSegment {
                start_tick: StreamTick(0),
                end_tick: StreamTick(100),
                delta_steps: [0; 3],
                flags: 0,
            }],
        )
        .unwrap();
        service.try_send_work(wrong).unwrap();
        let mut job = RealtimeJob::<3>::prepare(descriptor(1)).unwrap();
        assert!(matches!(
            job.poll(&mut realtime),
            Err(JobError::Machine(BlockError::ConfigurationIdentity))
        ));
        assert_eq!(job.status().state, RealtimeJobState::Faulted);

        service.try_send_work(block(0, Digest::ZERO)).unwrap();
        assert_eq!(job.drain(&mut realtime).unwrap(), 1);
        assert_eq!(realtime.work_depth(), 0);
    }

    #[test]
    fn stream_ticks_require_checked_commit_epoch_mapping() {
        assert_eq!(
            StreamTick(250).at_epoch(DeviceCycle(10_000)),
            Ok(DeviceCycle(10_250))
        );
    }
}
