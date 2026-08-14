//! Core-1 coupling between cached exact motion and the selected board output.

use alumina_config::RealtimeConfiguration;
use alumina_job::{AdmittedBlock, JobDescriptor};
use alumina_motion::{
    CommittedShiftOutput, ExecutorState, ScheduledBlockBoundary, ScheduledExecutionPlan,
    ScheduledShiftedExecution, ShiftImageContract, ShiftImageMapper, StepperExecutionProfile,
    scheduled_execution_mode_from_descriptor,
};
use alumina_protocol::{DeviceCycle, Digest};
use embassy_time::TICK_HZ;

use crate::hardware::selected;

type Runner =
    ScheduledShiftedExecution<{ selected::JOB_AXES }, { selected::MOTION_OUTPUT_RING_IMAGES }>;
type OwnedBlock = AdmittedBlock<{ selected::JOB_AXES }>;

#[derive(Clone, Copy)]
struct ConfiguredOutput {
    profile: StepperExecutionProfile<{ selected::JOB_AXES }>,
    contract: ShiftImageContract,
}

struct PipelinePlan {
    plan: ScheduledExecutionPlan,
    final_block_boundary_reached: bool,
    final_block_planned: bool,
}

/// One bounded result from servicing the exact motion owner.
#[allow(
    clippy::large_enum_variant,
    reason = "completion returns the unique inline block to its job actor"
)]
pub enum MotionAction {
    Idle,
    Future {
        at: DeviceCycle,
    },
    WaitingForHardware,
    OutputCommitted,
    /// First job-owned complete image physically committed at the start epoch.
    StartOutputCommitted(CommittedShiftOutput),
    BlockComplete(OwnedBlock),
    JobComplete,
}

/// Target composition, mapping, or physical output failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionServiceError {
    Configuration,
    Unsupported,
    State,
    Generator,
    Output,
    Commit,
    SafeImage,
}

/// Sole core-1 owner of the generated-versus-committed motion boundary.
pub struct MotionService {
    configured_output: Option<ConfiguredOutput>,
    runner: Option<Runner>,
    configuration_digest: Digest,
    primed_epoch: Option<DeviceCycle>,
    running: bool,
    total_blocks: u32,
    remaining_blocks: u32,
    lookahead: Option<OwnedBlock>,
    final_block_boundary_reached: bool,
    final_block_planned: bool,
    finish_requested: bool,
    finish_preplanned: bool,
    finish_committed: bool,
    start_observation_recorded: bool,
}

impl MotionService {
    /// Starts without an executable output mapping.
    pub const fn new() -> Self {
        Self {
            configured_output: None,
            runner: None,
            configuration_digest: Digest::ZERO,
            primed_epoch: None,
            running: false,
            total_blocks: 0,
            remaining_blocks: 0,
            lookahead: None,
            final_block_boundary_reached: false,
            final_block_planned: false,
            finish_requested: false,
            finish_preplanned: false,
            finish_committed: false,
            start_observation_recorded: false,
        }
    }

    /// Replaces the inert mapping from one independently validated active
    /// configuration. FOC-only configurations remain accepted but do not
    /// fabricate a step/dir backend.
    pub fn configure(
        &mut self,
        configuration: &RealtimeConfiguration,
    ) -> Result<(), MotionServiceError> {
        self.configured_output = None;
        self.runner = None;
        self.configuration_digest = Digest::ZERO;
        self.primed_epoch = None;
        self.running = false;
        self.total_blocks = 0;
        self.remaining_blocks = 0;
        self.lookahead = None;
        self.final_block_boundary_reached = false;
        self.final_block_planned = false;
        self.finish_requested = false;
        self.finish_preplanned = false;
        self.finish_committed = false;
        self.start_observation_recorded = false;
        let identity = configuration.identity();
        let summary = identity.summary;
        if summary.stepper_axes == 0 {
            self.configuration_digest = identity.digest;
            return Ok(());
        }
        if usize::from(summary.stepper_axes) != selected::JOB_AXES || summary.foc_axes != 0 {
            return Err(MotionServiceError::Configuration);
        }
        if !selected::MOTION_OUTPUT_IMPLEMENTED {
            return Err(MotionServiceError::Unsupported);
        }
        let contract = selected::motion_shift_contract().ok_or(MotionServiceError::Unsupported)?;
        let profile = StepperExecutionProfile::<{ selected::JOB_AXES }>::from_configuration(
            &identity,
            configuration.profile(),
            TICK_HZ,
            selected::MOTION_OUTPUT_QUANTUM_CYCLES,
            selected::MOTION_MAXIMUM_COMMIT_LATENESS_CYCLES,
        )
        .map_err(|_| MotionServiceError::Configuration)?;
        if selected::MOTION_OUTPUT_RING_IMAGES == 0 {
            return Err(MotionServiceError::Configuration);
        }
        let _mapper = ShiftImageMapper::new(&profile, contract)
            .map_err(|_| MotionServiceError::Configuration)?;
        self.configured_output = Some(ConfiguredOutput { profile, contract });
        self.configuration_digest = identity.digest;
        Ok(())
    }

    /// Removes executable mapping only after the caller made outputs safe.
    pub fn clear(&mut self) {
        self.configured_output = None;
        self.runner = None;
        self.configuration_digest = Digest::ZERO;
        self.primed_epoch = None;
        self.running = false;
        self.total_blocks = 0;
        self.remaining_blocks = 0;
        self.lookahead = None;
        self.final_block_boundary_reached = false;
        self.final_block_planned = false;
        self.finish_requested = false;
        self.finish_preplanned = false;
        self.finish_committed = false;
        self.start_observation_recorded = false;
    }

    /// Whether the selected package and active mapping can enter arm authority.
    pub fn ready_to_arm(&self) -> bool {
        selected::PACKAGE.armable
            && selected::MOTION_OUTPUT_QUALIFIED
            && self.configured_output.is_some()
    }

    /// After the abort guard closes, transfers the first admitted block and
    /// constructs the initial immutable hardware horizon before local start.
    pub fn prime(
        &mut self,
        resources: &mut selected::EstablishedRealtimeResources,
        descriptor: JobDescriptor,
        epoch: DeviceCycle,
        admitted: OwnedBlock,
        lookahead: Option<OwnedBlock>,
        observed: DeviceCycle,
    ) -> Result<(), MotionServiceError> {
        if descriptor.config_digest != self.configuration_digest
            || self.primed_epoch.is_some()
            || self.running
            || self.runner.is_some()
            || observed >= epoch
            || (descriptor.block_count == 1) != lookahead.is_none()
            || admitted.header().sequence != 0
            || lookahead
                .as_ref()
                .is_some_and(|block| block.header().sequence != 1)
        {
            return Err(MotionServiceError::Configuration);
        }
        let mode = scheduled_execution_mode_from_descriptor::<{ selected::JOB_AXES }>(descriptor)
            .map_err(|_| MotionServiceError::Configuration)?;
        let initial = descriptor
            .initial_position_for::<{ selected::JOB_AXES }>()
            .map_err(|_| MotionServiceError::Configuration)?;
        let configured = self
            .configured_output
            .ok_or(MotionServiceError::Unsupported)?;
        let mut runner = Runner::new(mode, configured.profile, configured.contract)
            .map_err(|_| MotionServiceError::Configuration)?;
        runner
            .start_job(epoch, initial)
            .map_err(|_| MotionServiceError::State)?;
        runner
            .admit_block(admitted)
            .map_err(|_| MotionServiceError::State)?;
        self.runner = Some(runner);
        let runner = self.runner.as_mut().ok_or(MotionServiceError::State)?;
        let mut lookahead = lookahead;
        let required_horizon = DeviceCycle(
            epoch
                .0
                .checked_add(selected::MOTION_PRIME_HORIZON_CYCLES)
                .ok_or(MotionServiceError::State)?,
        );
        let writable_horizon = resources
            .motion_output_writable_horizon(observed)
            .map_err(|_| MotionServiceError::Unsupported)?;
        if writable_horizon < required_horizon {
            return Err(MotionServiceError::Output);
        }
        let outcome = Self::plan_pipeline(
            runner,
            resources,
            required_horizon,
            descriptor.block_count,
            &mut lookahead,
        )?;
        let mut seal_through = Self::covered_through(outcome.plan, required_horizon)?;
        let mut finish_preplanned = false;
        if outcome.final_block_planned {
            let finish_at = runner
                .planned_finish_cycle()
                .map_err(|_| MotionServiceError::State)?;
            if writable_horizon < finish_at {
                return Err(MotionServiceError::Output);
            }
            Self::schedule_and_stage_planned_finish(runner, resources, finish_at)?;
            finish_preplanned = true;
            seal_through = required_horizon.max(finish_at);
        } else if seal_through < required_horizon {
            return Err(MotionServiceError::Output);
        }
        let sealed = resources
            .seal_motion_output_horizon(seal_through)
            .map_err(|_| MotionServiceError::Output)?;
        if sealed < seal_through {
            return Err(MotionServiceError::Output);
        }
        self.primed_epoch = Some(epoch);
        self.total_blocks = descriptor.block_count;
        self.remaining_blocks = descriptor.block_count;
        self.lookahead = lookahead;
        self.final_block_boundary_reached = outcome.final_block_boundary_reached;
        self.final_block_planned = outcome.final_block_planned;
        self.finish_requested = false;
        self.finish_preplanned = finish_preplanned;
        self.finish_committed = false;
        self.start_observation_recorded = false;
        Ok(())
    }

    /// Releases a previously hardware-primed job at the exact committed epoch.
    pub fn start(&mut self, epoch: DeviceCycle) -> Result<(), MotionServiceError> {
        if self.primed_epoch != Some(epoch) || self.running {
            return Err(MotionServiceError::State);
        }
        self.running = true;
        Ok(())
    }

    /// Retains the next exact outstanding block for admission as soon as the
    /// current logical block reaches its planning boundary.
    #[allow(
        clippy::result_large_err,
        reason = "rejection must return unique inline block ownership"
    )]
    pub fn admit(&mut self, admitted: OwnedBlock) -> Result<(), OwnedBlock> {
        if !self.running
            || self.remaining_blocks == 0
            || self.final_block_boundary_reached
            || self.final_block_planned
            || self.finish_preplanned
            || self.lookahead.is_some()
            || admitted.header().sequence >= self.total_blocks
        {
            return Err(admitted);
        }
        if self.runner.is_none() {
            return Err(admitted);
        }
        self.lookahead = Some(admitted);
        Ok(())
    }

    /// Advances hardware observations and extends the immutable future horizon.
    pub fn poll(
        &mut self,
        resources: &mut selected::EstablishedRealtimeResources,
        observed: DeviceCycle,
    ) -> Result<MotionAction, MotionServiceError> {
        let runner = self
            .runner
            .as_mut()
            .ok_or(MotionServiceError::Unsupported)?;
        if !self.running {
            return Err(MotionServiceError::State);
        }
        if self.finish_requested && self.finish_committed {
            self.runner = None;
            self.running = false;
            self.primed_epoch = None;
            self.total_blocks = 0;
            self.remaining_blocks = 0;
            self.lookahead = None;
            self.final_block_boundary_reached = false;
            self.final_block_planned = false;
            self.finish_requested = false;
            self.finish_preplanned = false;
            self.finish_committed = false;
            self.start_observation_recorded = false;
            return Ok(MotionAction::JobComplete);
        }
        if let Some((token, committed_at)) = resources
            .take_motion_commit()
            .map_err(|_| MotionServiceError::Output)?
        {
            let committed = runner
                .commit_output(token, committed_at)
                .map_err(|_| MotionServiceError::Commit)?;
            let action = if !self.start_observation_recorded {
                let epoch = self.primed_epoch.ok_or(MotionServiceError::State)?;
                if committed.update.at != epoch {
                    return Err(MotionServiceError::Commit);
                }
                self.start_observation_recorded = true;
                MotionAction::StartOutputCommitted(committed)
            } else {
                MotionAction::OutputCommitted
            };
            if runner.take_job_complete() {
                self.finish_committed = true;
            }
            return Ok(action);
        }
        let mut known_writable_horizon = None;
        if self.final_block_boundary_reached && !self.final_block_planned {
            let writable_horizon = resources
                .motion_output_writable_horizon(observed)
                .map_err(|_| MotionServiceError::Output)?;
            known_writable_horizon = Some(writable_horizon);
            let plan = Self::plan_and_stage(runner, resources, writable_horizon)?;
            if matches!(
                plan,
                ScheduledExecutionPlan::Idle | ScheduledExecutionPlan::BlockPlanned { .. }
            ) {
                return Err(MotionServiceError::State);
            }
            let covered = Self::covered_through(plan, writable_horizon)?;
            self.final_block_planned =
                matches!(plan, ScheduledExecutionPlan::OwnerTailComplete { .. });
            let sealed = resources
                .seal_motion_output_horizon(covered)
                .map_err(|_| MotionServiceError::Output)?;
            if sealed < covered {
                return Err(MotionServiceError::Output);
            }
            if !self.final_block_planned {
                return Self::action_for_plan(plan);
            }
        }
        if self.final_block_planned && !self.finish_preplanned && runner.block_completion_planned()
        {
            let finish_at = runner
                .planned_finish_cycle()
                .map_err(|_| MotionServiceError::State)?;
            if observed >= finish_at {
                return Err(MotionServiceError::Output);
            }
            let writable_horizon = resources
                .motion_output_writable_horizon(observed)
                .map_err(|_| MotionServiceError::Output)?;
            known_writable_horizon = Some(writable_horizon);
            if writable_horizon >= finish_at {
                Self::schedule_and_stage_planned_finish(runner, resources, finish_at)?;
                self.finish_preplanned = true;
            }
            let sealed = resources
                .seal_motion_output_horizon(writable_horizon)
                .map_err(|_| MotionServiceError::Output)?;
            if sealed < writable_horizon {
                return Err(MotionServiceError::Output);
            }
            if !self.finish_preplanned {
                return Ok(MotionAction::WaitingForHardware);
            }
        }
        if let Some(completed) = runner.take_completed_block(observed) {
            if self.remaining_blocks == 0 || (self.remaining_blocks == 1 && !self.finish_preplanned)
            {
                return Err(MotionServiceError::State);
            }
            self.remaining_blocks -= 1;
            return Ok(MotionAction::BlockComplete(completed.into_block()));
        }
        let writable_horizon = match known_writable_horizon {
            Some(horizon) => horizon,
            None => resources
                .motion_output_writable_horizon(observed)
                .map_err(|_| MotionServiceError::Output)?,
        };
        if self.finish_preplanned {
            let sealed = resources
                .seal_motion_output_horizon(writable_horizon)
                .map_err(|_| MotionServiceError::Output)?;
            if sealed < writable_horizon {
                return Err(MotionServiceError::Output);
            }
            return Ok(MotionAction::WaitingForHardware);
        }
        let outcome = Self::plan_pipeline(
            runner,
            resources,
            writable_horizon,
            self.total_blocks,
            &mut self.lookahead,
        )?;
        let mut covered = Self::covered_through(outcome.plan, writable_horizon)?;
        let mut finish_preplanned = false;
        self.final_block_boundary_reached |= outcome.final_block_boundary_reached;
        if outcome.final_block_planned {
            self.final_block_planned = true;
            let finish_at = runner
                .planned_finish_cycle()
                .map_err(|_| MotionServiceError::State)?;
            if writable_horizon >= finish_at {
                Self::schedule_and_stage_planned_finish(runner, resources, finish_at)?;
                covered = writable_horizon;
                finish_preplanned = true;
            }
        }
        let sealed = resources
            .seal_motion_output_horizon(covered)
            .map_err(|_| MotionServiceError::Output)?;
        if sealed < covered {
            return Err(MotionServiceError::Output);
        }
        self.finish_preplanned = finish_preplanned;
        Self::action_for_plan(outcome.plan)
    }

    /// Latest boundary whose complete-image value is known from one planning
    /// result. A full sparse ring cannot certify the as-yet ungenerated event
    /// at `next_at`, so coverage ends one exact output quantum earlier.
    fn covered_through(
        plan: ScheduledExecutionPlan,
        requested: DeviceCycle,
    ) -> Result<DeviceCycle, MotionServiceError> {
        plan.covered_through(requested, selected::MOTION_OUTPUT_QUANTUM_CYCLES)
            .map_err(|_| MotionServiceError::State)
    }

    fn action_for_plan(plan: ScheduledExecutionPlan) -> Result<MotionAction, MotionServiceError> {
        match plan {
            ScheduledExecutionPlan::Idle => Ok(MotionAction::Idle),
            ScheduledExecutionPlan::Future { at } => Ok(MotionAction::Future { at }),
            ScheduledExecutionPlan::HorizonFull { .. }
            | ScheduledExecutionPlan::BlockPlanned { .. }
            | ScheduledExecutionPlan::OwnerTailComplete { .. } => {
                Ok(MotionAction::WaitingForHardware)
            }
        }
    }

    /// Releases job-complete reporting only after the final block token has
    /// returned and its preplanned normal disable has physically committed.
    pub fn request_finish(&mut self) -> Result<(), MotionServiceError> {
        let runner = self
            .runner
            .as_ref()
            .ok_or(MotionServiceError::Unsupported)?;
        if self.remaining_blocks != 0
            || !self.finish_preplanned
            || runner.planned_status().state != ExecutorState::Complete
        {
            return Err(MotionServiceError::State);
        }
        self.finish_requested = true;
        Ok(())
    }

    /// Invalidates every pending token after the caller synchronously applied
    /// the board-safe transaction. Any retained block is deliberately dropped,
    /// never returned to the job actor for acknowledgement.
    pub fn fault(&mut self, at: DeviceCycle) -> Result<(), MotionServiceError> {
        let Some(runner) = self.runner.as_mut() else {
            return Ok(());
        };
        let update = runner.fault(at);
        self.primed_epoch = None;
        self.running = false;
        self.total_blocks = 0;
        self.remaining_blocks = 0;
        self.lookahead = None;
        self.final_block_boundary_reached = false;
        self.final_block_planned = false;
        self.finish_requested = false;
        self.finish_preplanned = false;
        self.finish_committed = false;
        self.start_observation_recorded = false;
        if selected::motion_shift_contract().map(|contract| contract.safe_image)
            != Some(update.image)
        {
            return Err(MotionServiceError::SafeImage);
        }
        let _unacknowledgeable = runner.take_faulted_block();
        Ok(())
    }

    /// Next exact logical generation deadline. Target refill/commit wakeups are
    /// an independent hardware responsibility.
    pub fn next_deadline(&self) -> Option<DeviceCycle> {
        self.runner.as_ref()?.next_deadline()
    }

    /// Whether this owner has entered the job lifecycle.
    pub fn started(&self) -> bool {
        self.running
    }

    /// Extends one immutable hardware horizon across at most the bounded
    /// successor window. A logical block boundary is not treated as a physical
    /// output boundary: if its successor is already owned, planning continues
    /// at the same exact stream tick while the prior commit barrier remains.
    fn plan_pipeline(
        runner: &mut Runner,
        resources: &mut selected::EstablishedRealtimeResources,
        through: DeviceCycle,
        total_blocks: u32,
        lookahead: &mut Option<OwnedBlock>,
    ) -> Result<PipelinePlan, MotionServiceError> {
        loop {
            let plan = Self::plan_and_stage(runner, resources, through)?;
            let ScheduledExecutionPlan::BlockPlanned {
                sequence, boundary, ..
            } = plan
            else {
                if matches!(plan, ScheduledExecutionPlan::OwnerTailComplete { .. }) {
                    return Err(MotionServiceError::State);
                }
                return Ok(PipelinePlan {
                    plan,
                    final_block_boundary_reached: false,
                    final_block_planned: false,
                });
            };
            let successor = sequence.checked_add(1).ok_or(MotionServiceError::State)?;
            if successor > total_blocks {
                return Err(MotionServiceError::State);
            }
            if successor == total_blocks {
                if lookahead.is_some() {
                    return Err(MotionServiceError::State);
                }
                if boundary == ScheduledBlockBoundary::ContinuationOpen {
                    let tail_plan = Self::plan_owner_tail_and_stage(runner, resources, through)?;
                    if matches!(
                        tail_plan,
                        ScheduledExecutionPlan::Idle | ScheduledExecutionPlan::BlockPlanned { .. }
                    ) {
                        return Err(MotionServiceError::State);
                    }
                    return Ok(PipelinePlan {
                        plan: tail_plan,
                        final_block_boundary_reached: true,
                        final_block_planned: matches!(
                            tail_plan,
                            ScheduledExecutionPlan::OwnerTailComplete { .. }
                        ),
                    });
                }
                return Ok(PipelinePlan {
                    plan,
                    final_block_boundary_reached: true,
                    final_block_planned: true,
                });
            }
            let Some(next) = lookahead.take() else {
                return Ok(PipelinePlan {
                    plan,
                    final_block_boundary_reached: false,
                    final_block_planned: false,
                });
            };
            if next.header().sequence != successor {
                *lookahead = Some(next);
                return Err(MotionServiceError::State);
            }
            if let Err(rejected) = runner.admit_block(next) {
                *lookahead = Some(rejected.into_block());
                return Err(MotionServiceError::State);
            }
        }
    }

    fn plan_and_stage(
        runner: &mut Runner,
        resources: &mut selected::EstablishedRealtimeResources,
        through: DeviceCycle,
    ) -> Result<ScheduledExecutionPlan, MotionServiceError> {
        let plan = runner
            .plan_through(through)
            .map_err(|_| MotionServiceError::Generator)?;
        while let Some(output) = runner.next_unstaged_output() {
            resources
                .stage_motion_output(output)
                .map_err(|_| MotionServiceError::Output)?;
            runner
                .stage_output(output)
                .map_err(|_| MotionServiceError::Commit)?;
        }
        Ok(plan)
    }

    fn plan_owner_tail_and_stage(
        runner: &mut Runner,
        resources: &mut selected::EstablishedRealtimeResources,
        through: DeviceCycle,
    ) -> Result<ScheduledExecutionPlan, MotionServiceError> {
        let plan = runner
            .plan_owner_tail_through(through)
            .map_err(|_| MotionServiceError::Generator)?;
        while let Some(output) = runner.next_unstaged_output() {
            resources
                .stage_motion_output(output)
                .map_err(|_| MotionServiceError::Output)?;
            runner
                .stage_output(output)
                .map_err(|_| MotionServiceError::Commit)?;
        }
        Ok(plan)
    }

    fn schedule_and_stage_planned_finish(
        runner: &mut Runner,
        resources: &mut selected::EstablishedRealtimeResources,
        at: DeviceCycle,
    ) -> Result<(), MotionServiceError> {
        let output = runner
            .schedule_planned_finish(at)
            .map_err(|_| MotionServiceError::Generator)?;
        resources
            .stage_motion_output(output)
            .map_err(|_| MotionServiceError::Output)?;
        runner
            .stage_output(output)
            .map_err(|_| MotionServiceError::Commit)?;
        Ok(())
    }
}
