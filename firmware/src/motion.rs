//! Core-1 coupling between cached exact motion and the selected board output.

use alumina_config::RealtimeConfiguration;
use alumina_job::{AdmittedBlock, JobDescriptor};
use alumina_motion::{
    ExecutorState, ShiftedCachedStepper, ShiftedMotionPoll, StepperExecutionProfile,
};
use alumina_protocol::{DeviceCycle, Digest};
use embassy_time::{Instant, TICK_HZ};

use crate::hardware::selected;

type Runner = ShiftedCachedStepper<{ selected::JOB_AXES }>;
type OwnedBlock = AdmittedBlock<{ selected::JOB_AXES }>;

/// One bounded result from servicing the exact motion owner.
#[allow(
    clippy::large_enum_variant,
    reason = "completion returns the unique inline block to its job actor"
)]
pub enum MotionAction {
    Idle,
    Future { at: DeviceCycle },
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
    finish_requested: bool,
}

impl MotionService {
    /// Starts without an executable output mapping.
    pub const fn new() -> Self {
        Self {
            runner: None,
            configuration_digest: Digest::ZERO,
            finish_requested: false,
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
        self.finish_requested = false;
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
            selected::MOTION_MAXIMUM_COMMIT_LATENESS_CYCLES,
        )
        .map_err(|_| MotionServiceError::Configuration)?;
        self.runner = Some(
            ShiftedCachedStepper::new(profile, contract)
                .map_err(|_| MotionServiceError::Configuration)?,
        );
        self.configuration_digest = configuration.identity.digest;
        Ok(())
    }

    /// Removes executable mapping only after the caller made outputs safe.
    pub fn clear(&mut self) {
        self.runner = None;
        self.configuration_digest = Digest::ZERO;
        self.finish_requested = false;
    }

    /// Whether the selected package and active mapping can enter arm authority.
    pub fn ready_to_arm(&self) -> bool {
        selected::PACKAGE.armable && selected::MOTION_OUTPUT_QUALIFIED && self.runner.is_some()
    }

    /// Starts one exact job at its committed local epoch and descriptor-bound
    /// absolute machine-lattice origin.
    pub fn start(
        &mut self,
        descriptor: JobDescriptor,
        epoch: DeviceCycle,
    ) -> Result<(), MotionServiceError> {
        if descriptor.config_digest != self.configuration_digest {
            return Err(MotionServiceError::Configuration);
        }
        let initial = descriptor
            .initial_position_for::<{ selected::JOB_AXES }>()
            .map_err(|_| MotionServiceError::Configuration)?;
        self.runner
            .as_mut()
            .ok_or(MotionServiceError::Unsupported)?
            .start_job(epoch, initial)
            .map_err(|_| MotionServiceError::State)?;
        self.finish_requested = false;
        Ok(())
    }

    /// Moves the exact outstanding block token into the generated-output owner.
    #[allow(
        clippy::result_large_err,
        reason = "rejection must return unique inline block ownership"
    )]
    pub fn admit(&mut self, admitted: OwnedBlock) -> Result<(), OwnedBlock> {
        match self.runner.as_mut() {
            Some(runner) => runner
                .admit_block(admitted)
                .map_err(|rejected| rejected.into_block()),
            None => Err(admitted),
        }
    }

    /// Generates and synchronously commits at most one complete image.
    pub fn poll(
        &mut self,
        resources: &mut selected::EstablishedRealtimeResources,
        observed: DeviceCycle,
    ) -> Result<MotionAction, MotionServiceError> {
        let runner = self
            .runner
            .as_mut()
            .ok_or(MotionServiceError::Unsupported)?;
        if self.finish_requested {
            let at = runner
                .earliest_finish_cycle()
                .map_err(|_| MotionServiceError::State)?;
            if observed < at {
                return Ok(MotionAction::Future { at });
            }
            let pending = runner
                .finish_job(at)
                .map_err(|_| MotionServiceError::Generator)?;
            resources
                .apply_motion_image(pending.update)
                .map_err(|_| MotionServiceError::Output)?;
            runner
                .commit_output(pending.token, DeviceCycle(Instant::now().as_ticks()))
                .map_err(|_| MotionServiceError::Commit)?;
            self.finish_requested = false;
            return Ok(MotionAction::JobComplete);
        }
        match runner
            .poll(observed)
            .map_err(|_| MotionServiceError::Generator)?
        {
            ShiftedMotionPoll::Idle => Ok(MotionAction::Idle),
            ShiftedMotionPoll::Future { at } => Ok(MotionAction::Future { at }),
            ShiftedMotionPoll::AwaitingCommit(_) => Err(MotionServiceError::State),
            ShiftedMotionPoll::Output(pending) => {
                resources
                    .apply_motion_image(pending.update)
                    .map_err(|_| MotionServiceError::Output)?;
                let committed_at = DeviceCycle(Instant::now().as_ticks());
                runner
                    .commit_output(pending.token, committed_at)
                    .map_err(|_| MotionServiceError::Commit)?;
                Ok(MotionAction::OutputCommitted)
            }
            ShiftedMotionPoll::BlockComplete { admitted, .. } => {
                Ok(MotionAction::BlockComplete(admitted))
            }
        }
    }

    /// Requests normal terminal disable after the exact job actor reports its
    /// final block complete. [`Self::poll`] waits for the enable-hold deadline.
    pub fn request_finish(&mut self) -> Result<(), MotionServiceError> {
        let runner = self
            .runner
            .as_ref()
            .ok_or(MotionServiceError::Unsupported)?;
        if runner.status().state != ExecutorState::Ready {
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
        self.finish_requested = false;
        if selected::motion_shift_contract().map(|contract| contract.safe_image)
            != Some(update.image)
        {
            return Err(MotionServiceError::SafeImage);
        }
        let _unacknowledgeable = runner.take_faulted_block();
        Ok(())
    }

    /// Next exact edge/horizon deadline, if execution has started.
    pub fn next_deadline(&self) -> Option<DeviceCycle> {
        let runner = self.runner.as_ref()?;
        if self.finish_requested {
            runner.earliest_finish_cycle().ok()
        } else {
            runner.next_deadline()
        }
    }

    /// Whether this owner has entered the job lifecycle.
    pub fn started(&self) -> bool {
        self.runner.as_ref().is_some_and(|runner| {
            matches!(
                runner.status().state,
                ExecutorState::Ready | ExecutorState::Segment | ExecutorState::Complete
            )
        })
    }
}
