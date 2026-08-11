//! Core-1 coupling between cached exact motion and the selected board output.

use alumina_config::RealtimeConfiguration;
use alumina_job::{AdmittedBlock, JobDescriptor};
use alumina_motion::{
    ExecutorState, ScheduledShiftPlan, ScheduledShiftedStepper, StepperExecutionProfile,
};
use alumina_protocol::{DeviceCycle, Digest};
use embassy_time::TICK_HZ;

use crate::hardware::selected;

type Runner =
    ScheduledShiftedStepper<{ selected::JOB_AXES }, { selected::MOTION_OUTPUT_RING_IMAGES }>;
type OwnedBlock = AdmittedBlock<{ selected::JOB_AXES }>;

/// One bounded result from servicing the exact motion owner.
#[allow(
    clippy::large_enum_variant,
    reason = "completion returns the unique inline block to its job actor"
)]
pub enum MotionAction {
    Idle,
    Future { at: DeviceCycle },
    WaitingForHardware,
    OutputCommitted,
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
    runner: Option<Runner>,
    configuration_digest: Digest,
    primed_epoch: Option<DeviceCycle>,
    running: bool,
    remaining_blocks: u32,
    finish_requested: bool,
    finish_preplanned: bool,
    finish_committed: bool,
}

impl MotionService {
    /// Starts without an executable output mapping.
    pub const fn new() -> Self {
        Self {
            runner: None,
            configuration_digest: Digest::ZERO,
            primed_epoch: None,
            running: false,
            remaining_blocks: 0,
            finish_requested: false,
            finish_preplanned: false,
            finish_committed: false,
        }
    }

    /// Replaces the inert mapping from one independently validated active
    /// configuration. FOC-only configurations remain accepted but do not
    /// fabricate a step/dir backend.
    pub fn configure(
        &mut self,
        configuration: &RealtimeConfiguration,
    ) -> Result<(), MotionServiceError> {
        self.runner = None;
        self.configuration_digest = Digest::ZERO;
        self.primed_epoch = None;
        self.running = false;
        self.remaining_blocks = 0;
        self.finish_requested = false;
        self.finish_preplanned = false;
        self.finish_committed = false;
        let summary = configuration.identity.summary;
        if summary.stepper_axes == 0 {
            self.configuration_digest = configuration.identity.digest;
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
            &configuration.identity,
            &configuration.profile,
            TICK_HZ,
            selected::MOTION_OUTPUT_QUANTUM_CYCLES,
            selected::MOTION_MAXIMUM_COMMIT_LATENESS_CYCLES,
        )
        .map_err(|_| MotionServiceError::Configuration)?;
        self.runner = Some(
            ScheduledShiftedStepper::new(profile, contract)
                .map_err(|_| MotionServiceError::Configuration)?,
        );
        self.configuration_digest = configuration.identity.digest;
        Ok(())
    }

    /// Removes executable mapping only after the caller made outputs safe.
    pub fn clear(&mut self) {
        self.runner = None;
        self.configuration_digest = Digest::ZERO;
        self.primed_epoch = None;
        self.running = false;
        self.remaining_blocks = 0;
        self.finish_requested = false;
        self.finish_preplanned = false;
        self.finish_committed = false;
    }

    /// Whether the selected package and active mapping can enter arm authority.
    pub fn ready_to_arm(&self) -> bool {
        selected::PACKAGE.armable && selected::MOTION_OUTPUT_QUALIFIED && self.runner.is_some()
    }

    /// After the abort guard closes, transfers the first admitted block and
    /// constructs the initial immutable hardware horizon before local start.
    pub fn prime(
        &mut self,
        resources: &mut selected::EstablishedRealtimeResources,
        descriptor: JobDescriptor,
        epoch: DeviceCycle,
        admitted: OwnedBlock,
        observed: DeviceCycle,
    ) -> Result<(), MotionServiceError> {
        if descriptor.config_digest != self.configuration_digest
            || self.primed_epoch.is_some()
            || self.running
            || observed >= epoch
        {
            return Err(MotionServiceError::Configuration);
        }
        let initial = descriptor
            .initial_position_for::<{ selected::JOB_AXES }>()
            .map_err(|_| MotionServiceError::Configuration)?;
        let runner = self
            .runner
            .as_mut()
            .ok_or(MotionServiceError::Unsupported)?;
        runner
            .start_job(epoch, initial)
            .map_err(|_| MotionServiceError::State)?;
        runner
            .admit_block(admitted)
            .map_err(|_| MotionServiceError::State)?;
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
        let plan = Self::plan_and_stage(runner, resources, required_horizon)?;
        let mut seal_through = Self::covered_through(plan, required_horizon)?;
        let mut finish_preplanned = false;
        if matches!(plan, ScheduledShiftPlan::BlockPlanned { .. }) && descriptor.block_count == 1 {
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
        self.remaining_blocks = descriptor.block_count;
        self.finish_requested = false;
        self.finish_preplanned = finish_preplanned;
        self.finish_committed = false;
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

    /// Moves the exact outstanding block token into the generated-output owner.
    #[allow(
        clippy::result_large_err,
        reason = "rejection must return unique inline block ownership"
    )]
    pub fn admit(&mut self, admitted: OwnedBlock) -> Result<(), OwnedBlock> {
        if self.remaining_blocks == 0 || self.finish_preplanned {
            return Err(admitted);
        }
        match self.runner.as_mut() {
            Some(runner) => runner
                .admit_block(admitted)
                .map_err(|rejected| rejected.into_block()),
            None => Err(admitted),
        }
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
            self.running = false;
            self.primed_epoch = None;
            self.remaining_blocks = 0;
            self.finish_requested = false;
            self.finish_preplanned = false;
            self.finish_committed = false;
            return Ok(MotionAction::JobComplete);
        }
        if let Some((token, committed_at)) = resources
            .take_motion_commit()
            .map_err(|_| MotionServiceError::Output)?
        {
            runner
                .commit_output(token, committed_at)
                .map_err(|_| MotionServiceError::Commit)?;
            if runner.take_job_complete() {
                self.finish_committed = true;
            }
            return Ok(MotionAction::OutputCommitted);
        }
        let mut known_writable_horizon = None;
        if self.remaining_blocks == 1
            && !self.finish_preplanned
            && runner.block_completion_planned()
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
        let plan = Self::plan_and_stage(runner, resources, writable_horizon)?;
        let mut covered = Self::covered_through(plan, writable_horizon)?;
        let mut finish_preplanned = false;
        if matches!(plan, ScheduledShiftPlan::BlockPlanned { .. }) && self.remaining_blocks == 1 {
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
        match plan {
            ScheduledShiftPlan::Idle => Ok(MotionAction::Idle),
            ScheduledShiftPlan::Future { at } => Ok(MotionAction::Future { at }),
            ScheduledShiftPlan::HorizonFull { .. } | ScheduledShiftPlan::BlockPlanned { .. } => {
                Ok(MotionAction::WaitingForHardware)
            }
        }
    }

    /// Latest boundary whose complete-image value is known from one planning
    /// result. A full sparse ring cannot certify the as-yet ungenerated event
    /// at `next_at`, so coverage ends one exact output quantum earlier.
    fn covered_through(
        plan: ScheduledShiftPlan,
        requested: DeviceCycle,
    ) -> Result<DeviceCycle, MotionServiceError> {
        match plan {
            ScheduledShiftPlan::Idle => Err(MotionServiceError::State),
            ScheduledShiftPlan::Future { at } => {
                if at <= requested {
                    Err(MotionServiceError::State)
                } else {
                    Ok(requested)
                }
            }
            ScheduledShiftPlan::HorizonFull { next_at, .. } => next_at
                .0
                .checked_sub(u64::from(selected::MOTION_OUTPUT_QUANTUM_CYCLES))
                .map(DeviceCycle)
                .ok_or(MotionServiceError::State),
            ScheduledShiftPlan::BlockPlanned { completion_at } => Ok(completion_at),
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
        self.remaining_blocks = 0;
        self.finish_requested = false;
        self.finish_preplanned = false;
        self.finish_committed = false;
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

    fn plan_and_stage(
        runner: &mut Runner,
        resources: &mut selected::EstablishedRealtimeResources,
        through: DeviceCycle,
    ) -> Result<ScheduledShiftPlan, MotionServiceError> {
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
