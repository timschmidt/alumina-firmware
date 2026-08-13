//! Deterministic cached-motion replay through the production core-1 owners.

use alumina_job::{JobDescriptor, RealtimeJob, RealtimeJobState, RealtimePoll, WorkSource};
use alumina_machine_ir::{
    BlockError, EXECUTION_BLOCK_BYTES, ExecutionBlock, MotionStreamProgress, StreamTick,
};
use alumina_motion::{
    CachedMotionError, CachedMotionPoll, CachedStepperExecutor, MotionError, StepperTiming,
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

fn report_from_progress<const AXES: usize>(
    block_count: u32,
    segment_count: u32,
    rising_edges: [u64; AXES],
    terminal_position: [i64; AXES],
    progress: MotionStreamProgress<AXES>,
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
