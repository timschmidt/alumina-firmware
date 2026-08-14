//! Deterministic cached-motion replay through the production core-1 owners.

use alumina_job::{
    JobDescriptor, MachineStreamProgress, RealtimeJob, RealtimeJobState, RealtimePoll, WorkSource,
};
use alumina_machine_ir::{BlockError, EXECUTION_BLOCK_BYTES, ExecutionBlock, StreamTick};
use alumina_motion::{
    CachedFiniteDifferenceError, CachedFiniteDifferenceExecutor, CachedFiniteDifferencePoll,
    CachedMotionError, CachedMotionPoll, CachedStepperExecutor, FiniteDifferenceExecutionLimits,
    FiniteDifferencePreflightError, MotionError, StepperTiming,
};
use alumina_protocol::{DeviceCycle, Digest};
use alumina_storage::{ObjectKind, sha256};

/// Successful byte-to-event replay of one immutable cached partition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CachedStepperReplayReport<const AXES: usize> {
    /// Number of independently decoded, admitted, executed, and acknowledged blocks.
    pub block_count: u32,
    /// Number of canonical constant-velocity segments executed.
    pub segment_count: u32,
    /// Rising step edges observed on each logical axis.
    pub rising_edges: [u64; AXES],
    /// Absolute terminal step-lattice position.
    pub terminal_position: [i64; AXES],
    /// Exclusive terminal stream tick.
    pub terminal_tick: StreamTick,
    /// Digest of the final independently admitted block.
    pub terminal_block_digest: Digest,
    /// Earliest exact device cycle at which normal driver disable was legal.
    pub finish_cycle: DeviceCycle,
    /// Total nonempty logical output transactions emitted by the executor.
    pub output_transactions: u64,
}

/// Successful byte-to-dense-update replay of one immutable direct-motion
/// partition through the production job and finite-difference owners.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CachedFiniteDifferenceReplayReport<const AXES: usize> {
    /// Independently decoded, admitted, executed, and acknowledged blocks.
    pub block_count: u32,
    /// Canonical direct finite-difference records consumed.
    pub segment_count: u32,
    /// Dense recurrence frames consumed, including frames with no output edge.
    pub update_count: u64,
    /// Rising step edges observed on each logical axis.
    pub rising_edges: [u64; AXES],
    /// Absolute terminal integer command-lattice position.
    pub terminal_position: [i64; AXES],
    /// Exact terminal stream-relative Q31.32 coordinate.
    pub terminal_finite_position: [i64; AXES],
    /// Exclusive terminal stream tick.
    pub terminal_tick: StreamTick,
    /// Digest of the final independently admitted block.
    pub terminal_block_digest: Digest,
    /// Earliest exact normal-disable cycle.
    pub finish_cycle: DeviceCycle,
    /// Nonempty logical output transactions emitted by the direct executor.
    pub output_transactions: u64,
}

/// Fail-closed stage of a deterministic cached-stepper replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CachedStepperReplayError {
    /// Partition byte length did not exactly match the descriptor's block count.
    PartitionLength,
    /// Complete bytes did not match the descriptor's immutable object identity.
    PartitionIdentity,
    /// A canonical block failed decoding or structural validation.
    Block(BlockError),
    /// Descriptor axis layout or initial position was invalid.
    Descriptor,
    /// Core-1 job preparation, admission, or acknowledgement failed.
    Job,
    /// The single-owner work source did not produce exactly one admitted block.
    AdmissionOwnership,
    /// An admitted block unexpectedly exposed no next execution deadline.
    MissingDeadline,
    /// An admitted block unexpectedly returned to an idle executor.
    UnexpectedExecutorIdle,
    /// The cached executor rejected a block after independent core-1 admission.
    CachedMotion(CachedMotionError),
    /// The stepper executor rejected electrical timing or final disable.
    Motion(MotionError),
    /// Job and executor terminal progress did not agree exactly.
    TerminalMismatch,
    /// An event or block counter overflowed.
    Arithmetic,
}

/// Fail-closed stage of a deterministic cached direct-motion replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CachedFiniteDifferenceReplayError {
    /// Partition byte length did not exactly match the descriptor.
    PartitionLength,
    /// Complete bytes did not match the immutable object identity.
    PartitionIdentity,
    /// A canonical block failed structural decoding.
    Block(BlockError),
    /// Descriptor axis layout, direct limits, or initial position was invalid.
    Descriptor,
    /// Core-1 job preparation, admission, or acknowledgement failed.
    Job,
    /// The single-owner source did not produce exactly one admitted block.
    AdmissionOwnership,
    /// An admitted block unexpectedly exposed no next execution deadline.
    MissingDeadline,
    /// An admitted block unexpectedly returned to an idle direct executor.
    UnexpectedExecutorIdle,
    /// Sparse or dense direct execution failed.
    FiniteDifference(CachedFiniteDifferenceError),
    /// Construction or final disable failed before block execution.
    DirectMotion(FiniteDifferencePreflightError),
    /// Job and direct executor terminal progress did not agree exactly.
    TerminalMismatch,
    /// An event, frame, or block counter overflowed.
    Arithmetic,
}

/// Decode immutable partition bytes, independently admit every block through
/// [`RealtimeJob`], execute every logical edge through
/// [`CachedStepperExecutor`], acknowledge ownership in order, and compare both
/// terminal progress records.
///
/// The simulation invokes no board backend and cannot energize hardware. Its
/// loop advances directly to each exact executor deadline, so the result is
/// deterministic and independent of host wall-clock scheduling.
pub fn replay_cached_stepper_partition<const AXES: usize>(
    partition_bytes: &[u8],
    descriptor: JobDescriptor,
    timing: StepperTiming<AXES>,
) -> Result<CachedStepperReplayReport<AXES>, CachedStepperReplayError> {
    let expected_bytes = usize::try_from(descriptor.block_count)
        .ok()
        .and_then(|count| count.checked_mul(EXECUTION_BLOCK_BYTES))
        .ok_or(CachedStepperReplayError::PartitionLength)?;
    if descriptor.block_count == 0 || partition_bytes.len() != expected_bytes {
        return Err(CachedStepperReplayError::PartitionLength);
    }
    let partition_byte_len = u64::try_from(partition_bytes.len())
        .map_err(|_| CachedStepperReplayError::PartitionLength)?;
    if descriptor.partition.object.kind != ObjectKind::MachineJobPartition
        || descriptor.partition.object.byte_len != partition_byte_len
        || descriptor.partition.object.content != sha256(partition_bytes)
    {
        return Err(CachedStepperReplayError::PartitionIdentity);
    }
    let initial_position = descriptor
        .initial_position_for::<AXES>()
        .map_err(|_| CachedStepperReplayError::Descriptor)?;
    let mut job =
        RealtimeJob::<AXES>::prepare(descriptor).map_err(|_| CachedStepperReplayError::Job)?;
    let mut executor =
        CachedStepperExecutor::new(timing).map_err(CachedStepperReplayError::Motion)?;
    executor
        .start_job(DeviceCycle(0), initial_position)
        .map_err(CachedStepperReplayError::Motion)?;

    let mut rising_edges = [0_u64; AXES];
    let mut output_transactions = 0_u64;
    let mut replayed_blocks = 0_u32;
    for encoded in partition_bytes.chunks_exact(EXECUTION_BLOCK_BYTES) {
        let mut owned = [0_u8; EXECUTION_BLOCK_BYTES];
        owned.copy_from_slice(encoded);
        let block = ExecutionBlock::decode(owned).map_err(CachedStepperReplayError::Block)?;
        let mut source = SingleBlock(Some(block));
        let admitted = match job
            .poll(&mut source)
            .map_err(|_| CachedStepperReplayError::Job)?
        {
            RealtimePoll::Block(admitted) if source.0.is_none() => admitted,
            RealtimePoll::Block(_) | RealtimePoll::Empty | RealtimePoll::Outstanding => {
                return Err(CachedStepperReplayError::AdmissionOwnership);
            }
        };
        executor
            .admit_block(admitted)
            .map_err(|rejected| CachedStepperReplayError::CachedMotion(rejected.error()))?;

        let completed = loop {
            let deadline = executor
                .next_deadline()
                .ok_or(CachedStepperReplayError::MissingDeadline)?;
            match executor
                .poll(deadline)
                .map_err(CachedStepperReplayError::CachedMotion)?
            {
                CachedMotionPoll::Event { event, .. } => {
                    output_transactions = output_transactions
                        .checked_add(1)
                        .ok_or(CachedStepperReplayError::Arithmetic)?;
                    for (axis, count) in rising_edges.iter_mut().enumerate() {
                        if event.step_high.contains(axis) {
                            *count = count
                                .checked_add(1)
                                .ok_or(CachedStepperReplayError::Arithmetic)?;
                        }
                    }
                }
                CachedMotionPoll::BlockComplete { admitted, .. } => break admitted,
                CachedMotionPoll::Future { .. } => {}
                CachedMotionPoll::Idle => {
                    return Err(CachedStepperReplayError::UnexpectedExecutorIdle);
                }
            }
        };
        job.acknowledge(completed)
            .map_err(|_| CachedStepperReplayError::Job)?;
        replayed_blocks = replayed_blocks
            .checked_add(1)
            .ok_or(CachedStepperReplayError::Arithmetic)?;
    }

    let job_status = job.status();
    let progress = match (job_status.state, job_status.completed_progress) {
        (RealtimeJobState::Complete, Some(progress)) => progress,
        _ => return Err(CachedStepperReplayError::TerminalMismatch),
    };
    let executor_status = executor.status();
    let mut expected_terminal_position = initial_position;
    for (position, displacement) in expected_terminal_position.iter_mut().zip(progress.position) {
        *position = position
            .checked_add(displacement)
            .ok_or(CachedStepperReplayError::Arithmetic)?;
    }
    if replayed_blocks != descriptor.block_count
        || job_status.completed_blocks != descriptor.block_count
        || executor_status.position != expected_terminal_position
        || executor_status.next_tick != progress.end_tick
        || executor_status.emitted_steps != rising_edges
    {
        return Err(CachedStepperReplayError::TerminalMismatch);
    }
    let finish_cycle = executor
        .earliest_finish_cycle()
        .map_err(CachedStepperReplayError::Motion)?;
    executor
        .finish_job(finish_cycle)
        .map_err(CachedStepperReplayError::Motion)?;

    Ok(report_from_progress(
        replayed_blocks,
        executor_status.completed_segments,
        rising_edges,
        executor_status.position,
        progress,
        finish_cycle,
        output_transactions,
    ))
}

/// Decode immutable direct-motion bytes, independently admit them through
/// [`RealtimeJob`], consume every dense recurrence frame through
/// [`CachedFiniteDifferenceExecutor`], acknowledge block ownership in order,
/// and compare integer plus Q31.32 terminal facts.
///
/// This deterministic host path owns no board backend and cannot energize
/// hardware. It advances directly to each exact recurrence/output deadline.
pub fn replay_cached_finite_difference_partition<const AXES: usize>(
    partition_bytes: &[u8],
    descriptor: JobDescriptor,
    timing: StepperTiming<AXES>,
) -> Result<CachedFiniteDifferenceReplayReport<AXES>, CachedFiniteDifferenceReplayError> {
    let expected_bytes = usize::try_from(descriptor.block_count)
        .ok()
        .and_then(|count| count.checked_mul(EXECUTION_BLOCK_BYTES))
        .ok_or(CachedFiniteDifferenceReplayError::PartitionLength)?;
    if descriptor.block_count == 0 || partition_bytes.len() != expected_bytes {
        return Err(CachedFiniteDifferenceReplayError::PartitionLength);
    }
    let partition_byte_len = u64::try_from(partition_bytes.len())
        .map_err(|_| CachedFiniteDifferenceReplayError::PartitionLength)?;
    if descriptor.partition.object.kind != ObjectKind::MachineJobPartition
        || descriptor.partition.object.byte_len != partition_byte_len
        || descriptor.partition.object.content != sha256(partition_bytes)
    {
        return Err(CachedFiniteDifferenceReplayError::PartitionIdentity);
    }
    let initial_position = descriptor
        .initial_position_for::<AXES>()
        .map_err(|_| CachedFiniteDifferenceReplayError::Descriptor)?;
    let limits = FiniteDifferenceExecutionLimits {
        maximum_segment_ticks: descriptor.limits.segment.maximum_segment_ticks,
        maximum_update_count: descriptor.maximum_finite_difference_updates,
        maximum_steps_per_segment: descriptor.limits.segment.maximum_steps_per_segment,
    };
    let mut job = RealtimeJob::<AXES>::prepare(descriptor)
        .map_err(|_| CachedFiniteDifferenceReplayError::Job)?;
    let mut executor = CachedFiniteDifferenceExecutor::new(timing, limits)
        .map_err(CachedFiniteDifferenceReplayError::DirectMotion)?;
    executor
        .start_job(DeviceCycle(0), initial_position)
        .map_err(CachedFiniteDifferenceReplayError::DirectMotion)?;

    let mut rising_edges = [0_u64; AXES];
    let mut output_transactions = 0_u64;
    let mut replayed_blocks = 0_u32;
    let mut segment_count = 0_u32;
    let mut update_count = 0_u64;
    for encoded in partition_bytes.chunks_exact(EXECUTION_BLOCK_BYTES) {
        let mut owned = [0_u8; EXECUTION_BLOCK_BYTES];
        owned.copy_from_slice(encoded);
        let block =
            ExecutionBlock::decode(owned).map_err(CachedFiniteDifferenceReplayError::Block)?;
        segment_count = segment_count
            .checked_add(block.header().segment_count)
            .ok_or(CachedFiniteDifferenceReplayError::Arithmetic)?;
        let mut source = SingleBlock(Some(block));
        let admitted = match job
            .poll(&mut source)
            .map_err(|_| CachedFiniteDifferenceReplayError::Job)?
        {
            RealtimePoll::Block(admitted) if source.0.is_none() => admitted,
            RealtimePoll::Block(_) | RealtimePoll::Empty | RealtimePoll::Outstanding => {
                return Err(CachedFiniteDifferenceReplayError::AdmissionOwnership);
            }
        };
        executor.admit_block(admitted).map_err(|rejected| {
            CachedFiniteDifferenceReplayError::FiniteDifference(rejected.error())
        })?;

        let completed = loop {
            let deadline = executor
                .next_deadline()
                .ok_or(CachedFiniteDifferenceReplayError::MissingDeadline)?;
            match executor
                .poll(deadline)
                .map_err(CachedFiniteDifferenceReplayError::FiniteDifference)?
            {
                CachedFiniteDifferencePoll::Update { event, .. } => {
                    update_count = update_count
                        .checked_add(1)
                        .ok_or(CachedFiniteDifferenceReplayError::Arithmetic)?;
                    if let Some(event) = event {
                        record_direct_event(event, &mut rising_edges, &mut output_transactions)?;
                    }
                }
                CachedFiniteDifferencePoll::Event { event, .. } => {
                    record_direct_event(event, &mut rising_edges, &mut output_transactions)?;
                }
                CachedFiniteDifferencePoll::BlockComplete { admitted, .. } => break admitted,
                CachedFiniteDifferencePoll::Future { .. } => {}
                CachedFiniteDifferencePoll::Idle => {
                    return Err(CachedFiniteDifferenceReplayError::UnexpectedExecutorIdle);
                }
            }
        };
        job.acknowledge(completed)
            .map_err(|_| CachedFiniteDifferenceReplayError::Job)?;
        replayed_blocks = replayed_blocks
            .checked_add(1)
            .ok_or(CachedFiniteDifferenceReplayError::Arithmetic)?;
    }

    let job_status = job.status();
    let progress = match (job_status.state, job_status.completed_progress) {
        (RealtimeJobState::Complete, Some(progress)) => progress,
        _ => return Err(CachedFiniteDifferenceReplayError::TerminalMismatch),
    };
    let executor_status = executor.status();
    let mut expected_terminal_position = initial_position;
    for (position, displacement) in expected_terminal_position.iter_mut().zip(progress.position) {
        *position = position
            .checked_add(displacement)
            .ok_or(CachedFiniteDifferenceReplayError::Arithmetic)?;
    }
    if replayed_blocks != descriptor.block_count
        || job_status.completed_blocks != descriptor.block_count
        || executor_status.position != expected_terminal_position
        || executor_status.next_tick != progress.end_tick
        || executor_status.emitted_steps != rising_edges
        || executor_status.completed_segments != segment_count
    {
        return Err(CachedFiniteDifferenceReplayError::TerminalMismatch);
    }
    let terminal_finite_position = executor.finite_position();
    let finish_cycle = executor
        .earliest_finish_cycle()
        .map_err(CachedFiniteDifferenceReplayError::DirectMotion)?;
    executor
        .finish_job(finish_cycle)
        .map_err(CachedFiniteDifferenceReplayError::DirectMotion)?;

    Ok(CachedFiniteDifferenceReplayReport {
        block_count: replayed_blocks,
        segment_count,
        update_count,
        rising_edges,
        terminal_position: executor_status.position,
        terminal_finite_position,
        terminal_tick: progress.end_tick,
        terminal_block_digest: progress.block_digest,
        finish_cycle,
        output_transactions,
    })
}

fn record_direct_event<const AXES: usize>(
    event: alumina_motion::StepperEvent,
    rising_edges: &mut [u64; AXES],
    output_transactions: &mut u64,
) -> Result<(), CachedFiniteDifferenceReplayError> {
    *output_transactions = output_transactions
        .checked_add(1)
        .ok_or(CachedFiniteDifferenceReplayError::Arithmetic)?;
    for (axis, count) in rising_edges.iter_mut().enumerate() {
        if event.step_high.contains(axis) {
            *count = count
                .checked_add(1)
                .ok_or(CachedFiniteDifferenceReplayError::Arithmetic)?;
        }
    }
    Ok(())
}

fn report_from_progress<const AXES: usize>(
    block_count: u32,
    segment_count: u32,
    rising_edges: [u64; AXES],
    terminal_position: [i64; AXES],
    progress: MachineStreamProgress<AXES>,
    finish_cycle: DeviceCycle,
    output_transactions: u64,
) -> CachedStepperReplayReport<AXES> {
    CachedStepperReplayReport {
        block_count,
        segment_count,
        rising_edges,
        terminal_position,
        terminal_tick: progress.end_tick,
        terminal_block_digest: progress.block_digest,
        finish_cycle,
        output_transactions,
    }
}

struct SingleBlock(Option<ExecutionBlock>);

impl WorkSource for SingleBlock {
    fn try_receive(&mut self) -> Option<ExecutionBlock> {
        self.0.take()
    }

    fn depth(&self) -> usize {
        usize::from(self.0.is_some())
    }
}

#[cfg(test)]
mod tests {
    use alumina_machine_ir::{
        BlockValidationLimits, ExecutionKind, FINITE_DIFFERENCE_ONE_STEP, FiniteDifferenceAxis,
        FiniteDifferenceSegment, StreamId, ValidationLimits,
    };
    use alumina_motion::AxisTiming;
    use alumina_storage::{ContentId, PublishedObject, StoredObject};

    use super::*;

    fn timing() -> StepperTiming<2> {
        StepperTiming {
            axes: [AxisTiming {
                pulse_high_cycles: 1,
                pulse_low_cycles: 1,
                direction_setup_cycles: 2,
                direction_hold_cycles: 2,
                enable_setup_cycles: 2,
                enable_hold_cycles: 2,
                maximum_step_frequency_hz: 500_000,
            }; 2],
            device_cycle_hz: 1_000_000,
            output_quantum_cycles: 1,
            maximum_lateness_cycles: 0,
        }
    }

    #[test]
    fn immutable_direct_partition_replays_every_dense_update_and_token() {
        let step = FINITE_DIFFERENCE_ONE_STEP;
        let first = FiniteDifferenceSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(8),
            update_period_ticks: 1,
            update_count: 8,
            axes: [
                FiniteDifferenceAxis {
                    initial_position: 0,
                    first_difference: step / 16,
                    second_difference: 0,
                    third_difference: step / 128,
                },
                FiniteDifferenceAxis {
                    initial_position: 0,
                    first_difference: -(step / 16),
                    second_difference: 0,
                    third_difference: -(step / 128),
                },
            ],
            flags: 0,
        };
        let terminal = [
            first.position_at(0, 8).unwrap(),
            first.position_at(1, 8).unwrap(),
        ];
        let second = FiniteDifferenceSegment {
            start_tick: StreamTick(8),
            end_tick: StreamTick(16),
            update_period_ticks: 1,
            update_count: 8,
            axes: [
                FiniteDifferenceAxis {
                    initial_position: terminal[0],
                    first_difference: (step - 1) / 4,
                    second_difference: 0,
                    third_difference: 0,
                },
                FiniteDifferenceAxis {
                    initial_position: terminal[1],
                    first_difference: -((step - 1) / 4),
                    second_difference: 0,
                    third_difference: 0,
                },
            ],
            flags: 0,
        };
        let stream_id = StreamId::new([0x71; 16]).unwrap();
        let capability_digest = Digest([0x72; 32]);
        let config_digest = Digest([0x73; 32]);
        let block = ExecutionBlock::encode_finite_difference(
            stream_id,
            capability_digest,
            config_digest,
            0,
            Digest::ZERO,
            &[first, second],
        )
        .unwrap();
        let bytes = block.as_bytes().to_vec();
        let descriptor = JobDescriptor {
            prepare_id: 23,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: sha256(&bytes),
                    byte_len: EXECUTION_BLOCK_BYTES as u64,
                },
                manifest: ContentId::from_sha256(Digest([0x74; 32])),
            },
            stream_id,
            capability_digest,
            config_digest,
            axis_count: 2,
            execution_kind: ExecutionKind::FiniteDifference,
            maximum_finite_difference_updates: 100,
            block_count: 1,
            first_tick: StreamTick(0),
            initial_position: [20, -20, 0, 0, 0, 0, 0, 0],
            limits: BlockValidationLimits {
                maximum_block_ticks: 1_000,
                segment: ValidationLimits {
                    maximum_segment_ticks: 1_000,
                    maximum_steps_per_segment: 100,
                },
            },
        };
        let report =
            replay_cached_finite_difference_partition(&bytes, descriptor, timing()).unwrap();
        assert_eq!(report.block_count, 1);
        assert_eq!(report.segment_count, 2);
        assert_eq!(report.update_count, 16);
        assert_eq!(report.rising_edges, [3, 3]);
        assert_eq!(report.terminal_position, [23, -23]);
        assert_eq!(report.terminal_tick, StreamTick(16));
        assert_eq!(report.terminal_block_digest, block.header().block_digest);
        assert_eq!(report.finish_cycle, DeviceCycle(18));
        assert_eq!(report.output_transactions, 7);

        let mut corrupt = bytes;
        corrupt[200] ^= 1;
        assert_eq!(
            replay_cached_finite_difference_partition(&corrupt, descriptor, timing()),
            Err(CachedFiniteDifferenceReplayError::PartitionIdentity)
        );
    }
}
