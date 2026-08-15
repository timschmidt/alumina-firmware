//! Deterministic replay of the exact configured FOC hardware boundary.
//!
//! This simulator converts integer compare edges into the device-cycle domain,
//! constructs synchronized two-shunt samples, executes the portable current
//! controller, and commits the next complete compare image. It is not an
//! electrical motor model and makes no physical timing or energization claim.

use core::{cell::Cell, convert::Infallible};

use alumina_config::{ConfigurationError, LoweredFocAxisConfiguration, RealtimeConfiguration};
use alumina_foc::{
    AlphaBeta, CurrentSample, DqControlUpdate, DqCurrentController, DqInterval, DqPoint,
    FocCurrentCommand, FocError, ModulationResult, PowerStageCommit,
    PreparedServoFocBankActivation, PwmAdcSampleStamp, PwmAdcSynchronization,
    PwmCommitBankHardware, PwmCommitBankOwnerError, PwmCommitBankOwnerFault,
    PwmCommitBankOwnerState, PwmCommitBankTargetOwner, PwmCommitBarrierError, PwmCompareError,
    PwmCompareImage, PwmCompareLatch, PwmCompareLatchError, PwmCompareLatchOwner, Q30Interval,
    RotorSample, ServoEncoderObservation, ServoEncoderSeed, ServoFocAxisController,
    ServoFocAxisError, ServoFocAxisPeriodInput, ServoFocAxisProfile, ServoFocAxisUpdate,
    ServoFocBank, ServoFocBankError, ServoSetpoint, inverse_park, park, space_vector_modulate,
};
use alumina_motion::{PreparedServoSetpoints, ServoSetpointOutput, ServoSetpointOutputCommit};
use alumina_protocol::DeviceCycle;

const INITIAL_DUTY_TOKEN: u32 = 1;
const INITIAL_COMMAND_ID: u32 = 1;

/// Configuration, timing, acquisition, or replay rejection in the virtual loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocHardwareLoopError {
    Configuration(ConfigurationError),
    Foc(FocError),
    Compare(PwmCompareError),
    Latch(PwmCompareLatchError),
    /// The complete estimator/servo/current/PWM actor rejected.
    Axis(ServoFocAxisError),
    /// The simultaneous complete-axis bank rejected before state publication.
    Bank(ServoFocBankError),
    /// The physical multi-stage commit barrier rejected before controller publication.
    CommitBarrier(PwmCommitBarrierError),
    /// The first simulator supports one current update per PWM period.
    Rate,
    /// Command identity, time, token, or sequence was not the sole expected value.
    Sequence,
    /// A counter edge did not map to an integer device cycle.
    ClockDomain,
    /// Device-cycle or sequence arithmetic overflowed.
    Overflow,
    /// A prior rejection terminally closed this virtual owner.
    Faulted,
}

impl From<ConfigurationError> for FocHardwareLoopError {
    fn from(error: ConfigurationError) -> Self {
        Self::Configuration(error)
    }
}

impl From<FocError> for FocHardwareLoopError {
    fn from(error: FocError) -> Self {
        Self::Foc(error)
    }
}

impl From<PwmCompareError> for FocHardwareLoopError {
    fn from(error: PwmCompareError) -> Self {
        Self::Compare(error)
    }
}

impl From<PwmCompareLatchError> for FocHardwareLoopError {
    fn from(error: PwmCompareLatchError) -> Self {
        Self::Latch(error)
    }
}

impl From<ServoFocAxisError> for FocHardwareLoopError {
    fn from(error: ServoFocAxisError) -> Self {
        Self::Axis(error)
    }
}

impl From<ServoFocBankError> for FocHardwareLoopError {
    fn from(error: ServoFocBankError) -> Self {
        Self::Bank(error)
    }
}

impl From<PwmCommitBarrierError> for FocHardwareLoopError {
    fn from(error: PwmCommitBarrierError) -> Self {
        Self::CommitBarrier(error)
    }
}

/// One exact command plus the integer sensor observations for its PWM period.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocHardwareLoopInput {
    pub command: FocCurrentCommand,
    pub raw_current_counts: [u16; 2],
    pub raw_rotor_count: u32,
}

/// Complete replayable result of one virtual hardware/current-loop transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocHardwareLoopSample {
    pub input: FocHardwareLoopInput,
    pub active_image: PwmCompareImage,
    pub current: CurrentSample,
    pub rotor: RotorSample,
    pub measured_interval: DqInterval,
    pub measured: DqPoint,
    pub control: DqControlUpdate,
    pub modulation: ModulationResult,
    pub committed: PwmCompareLatch,
}

/// One exact outer-loop command/observation plus raw inner-loop observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocServoHardwareLoopInput {
    pub setpoint: Option<ServoSetpoint>,
    pub encoder_observation: Option<ServoEncoderObservation>,
    pub raw_current_counts: [u16; 2],
    pub raw_rotor_count: u32,
}

/// Configuration-derived virtual owner for the complete servo/FOC chain.
#[derive(Debug, Eq, PartialEq)]
pub struct ConfiguredServoFocHardwareLoop {
    controller: ServoFocAxisController,
}

impl ConfiguredServoFocHardwareLoop {
    /// Constructs through the private independently validated configuration.
    pub fn from_configuration(
        configuration: &RealtimeConfiguration,
        slot: usize,
        activation_id: u64,
        seed: ServoEncoderSeed,
        first_boundary: DeviceCycle,
    ) -> Result<Self, FocHardwareLoopError> {
        let lowered = configuration.lower_foc_axis(slot)?;
        Self::from_lowered(lowered, activation_id, seed, first_boundary)
    }

    fn from_lowered(
        lowered: LoweredFocAxisConfiguration,
        activation_id: u64,
        seed: ServoEncoderSeed,
        first_boundary: DeviceCycle,
    ) -> Result<Self, FocHardwareLoopError> {
        let profile = lowered.servo_foc_axis_profile()?;
        let prepared = ServoFocAxisController::prepare_activation(
            profile,
            activation_id,
            seed,
            first_boundary,
        )?;
        let initial_image = prepared.initial_image();
        let controller = ServoFocAxisController::activate(
            prepared,
            PowerStageCommit {
                token: initial_image.token(),
                scheduled_at: initial_image.scheduled_at(),
                observed_at: initial_image.scheduled_at(),
            },
        )?;
        Ok(Self { controller })
    }

    /// Complete portable actor state retained by this virtual hardware owner.
    pub const fn controller(&self) -> &ServoFocAxisController {
        &self.controller
    }

    /// Replays one complete current period and exact next timer-zero commit.
    pub fn step(
        &mut self,
        input: FocServoHardwareLoopInput,
    ) -> Result<ServoFocAxisUpdate, FocHardwareLoopError> {
        let observed = DeviceCycle(
            self.controller
                .period_started_at()
                .0
                .checked_add(u64::from(
                    self.controller
                        .profile()
                        .current
                        .snapshot()
                        .synchronization
                        .pwm_period_cycles,
                ))
                .ok_or(FocHardwareLoopError::Overflow)?,
        );
        self.step_with_timer_zero(input, observed)
    }

    /// Replays one period with an explicit physical-boundary fault injection.
    pub fn step_with_timer_zero(
        &mut self,
        input: FocServoHardwareLoopInput,
        observed_timer_zero: DeviceCycle,
    ) -> Result<ServoFocAxisUpdate, FocHardwareLoopError> {
        let profile = self.controller.profile();
        let active_image = self
            .controller
            .active_image()
            .ok_or(FocHardwareLoopError::Sequence)?;
        let at = self.controller.period_started_at();
        let stamp = exact_sample_stamp(
            profile.pwm_compare,
            &profile.parameters,
            profile.current.snapshot().synchronization,
            active_image,
            self.controller.period_sequence(),
            at,
        )?;
        let prepared = self.controller.prepare(ServoFocAxisPeriodInput {
            at,
            setpoint: input.setpoint,
            encoder_observation: input.encoder_observation,
            raw_current_counts: input.raw_current_counts,
            current_stamp: stamp,
            raw_rotor_count: input.raw_rotor_count,
        })?;
        let staged = prepared.staged_image();
        self.controller
            .commit(
                prepared,
                PowerStageCommit {
                    token: staged.token(),
                    scheduled_at: staged.scheduled_at(),
                    observed_at: observed_timer_zero,
                },
            )
            .map_err(FocHardwareLoopError::Axis)
    }
}

/// Deterministic aggregate PWM backend used only by the host simulator.
///
/// Supplied timer-zero values become modeled reports; they are not peripheral
/// observations. The backend still exercises the same aggregate stage/poll/safe
/// trait that a future target owner must implement.
#[derive(Debug, Eq, PartialEq)]
struct ModeledPwmCommitHardware<const AXES: usize> {
    staged: Option<[PwmCompareImage; AXES]>,
    observed_timer_zero: Cell<[DeviceCycle; AXES]>,
    reported: [bool; AXES],
    safe: bool,
    safe_transactions: u64,
}

impl<const AXES: usize> ModeledPwmCommitHardware<AXES> {
    fn new(observed_timer_zero: [DeviceCycle; AXES]) -> Self {
        Self {
            staged: None,
            observed_timer_zero: Cell::new(observed_timer_zero),
            reported: [false; AXES],
            safe: false,
            safe_transactions: 0,
        }
    }

    fn set_observed_timer_zero(&self, observed_timer_zero: [DeviceCycle; AXES]) {
        self.observed_timer_zero.set(observed_timer_zero);
    }
}

impl<const AXES: usize> PwmCommitBankHardware<AXES> for ModeledPwmCommitHardware<AXES> {
    type Error = Infallible;

    fn stage_images(&mut self, images: &[PwmCompareImage; AXES]) -> Result<(), Self::Error> {
        self.staged = Some(*images);
        self.reported = [false; AXES];
        self.safe = false;
        Ok(())
    }

    fn take_latch(&mut self, axis: usize) -> Result<Option<PowerStageCommit>, Self::Error> {
        let Some(image) = self
            .staged
            .as_ref()
            .and_then(|images| images.get(axis))
            .copied()
        else {
            return Ok(None);
        };
        if self.reported[axis] {
            return Ok(None);
        }
        self.reported[axis] = true;
        let commit = PowerStageCommit {
            token: image.token(),
            scheduled_at: image.scheduled_at(),
            observed_at: self.observed_timer_zero.get()[axis],
        };
        if self.reported.iter().all(|reported| *reported) {
            self.staged = None;
        }
        Ok(Some(commit))
    }

    fn force_safe(&mut self) -> Result<(), Self::Error> {
        self.staged = None;
        self.reported = [false; AXES];
        self.safe = true;
        self.safe_transactions = self
            .safe_transactions
            .checked_add(1)
            .expect("modeled safe-transaction count");
        Ok(())
    }
}

type ModeledPwmCommitOwner<const AXES: usize> =
    PwmCommitBankTargetOwner<ModeledPwmCommitHardware<AXES>, AXES>;

fn map_modeled_owner_error(error: PwmCommitBankOwnerError<Infallible>) -> FocHardwareLoopError {
    match error {
        PwmCommitBankOwnerError::Closed { .. } => FocHardwareLoopError::Faulted,
        PwmCommitBankOwnerError::Barrier { error, .. } => {
            FocHardwareLoopError::CommitBarrier(error)
        }
        PwmCommitBankOwnerError::Hardware { error, .. }
        | PwmCommitBankOwnerError::Publication { error, .. } => match error {},
    }
}

fn map_modeled_publication_error(
    error: PwmCommitBankOwnerError<Infallible, ServoFocBankError>,
) -> FocHardwareLoopError {
    match error {
        PwmCommitBankOwnerError::Closed { .. } => FocHardwareLoopError::Faulted,
        PwmCommitBankOwnerError::Barrier { error, .. } => {
            FocHardwareLoopError::CommitBarrier(error)
        }
        PwmCommitBankOwnerError::Hardware { error, .. } => match error {},
        PwmCommitBankOwnerError::Publication { error, .. } => FocHardwareLoopError::Bank(error),
    }
}

/// Fixed-capacity virtual owner for simultaneous complete servo/FOC axes.
///
/// The portable bank calculates every encoder/cascade/current/angle/SVPWM
/// transition first and installs no live axis state until every exact PWM
/// commit has validated. This models the software transaction required around
/// synchronized physical timer-zero observations; it does not claim that any
/// target peripheral supplies those observations.
#[derive(Debug, Eq, PartialEq)]
pub struct ConfiguredServoFocHardwareBank<const AXES: usize> {
    controller: ServoFocBank<AXES>,
    commit_owner: ModeledPwmCommitOwner<AXES>,
}

impl<const AXES: usize> ConfiguredServoFocHardwareBank<AXES> {
    /// Constructs all axes through one independently validated configuration.
    pub fn from_configuration(
        configuration: &RealtimeConfiguration,
        slots: [usize; AXES],
        activation_ids: [u64; AXES],
        seeds: [ServoEncoderSeed; AXES],
        first_boundary: DeviceCycle,
    ) -> Result<Self, FocHardwareLoopError> {
        let mut profiles = core::array::from_fn(|_| None);
        for axis in 0..AXES {
            if slots.iter().take(axis).any(|slot| *slot == slots[axis]) {
                return Err(FocHardwareLoopError::Bank(ServoFocBankError::Profile));
            }
            profiles[axis] = Some(
                configuration
                    .lower_foc_axis(slots[axis])?
                    .servo_foc_axis_profile()?,
            );
        }
        let profiles = profiles.map(|profile| profile.expect("complete configured FOC bank"));
        Self::from_profiles(profiles, activation_ids, seeds, first_boundary)
    }

    #[cfg(test)]
    fn from_lowered(
        lowered: [LoweredFocAxisConfiguration; AXES],
        activation_ids: [u64; AXES],
        seeds: [ServoEncoderSeed; AXES],
        first_boundary: DeviceCycle,
    ) -> Result<Self, FocHardwareLoopError> {
        let mut profiles = core::array::from_fn(|_| None);
        for axis in 0..AXES {
            if lowered
                .iter()
                .take(axis)
                .any(|candidate| candidate.instance == lowered[axis].instance)
            {
                return Err(FocHardwareLoopError::Bank(ServoFocBankError::Profile));
            }
            profiles[axis] = Some(lowered[axis].servo_foc_axis_profile()?);
        }
        let profiles = profiles.map(|profile| profile.expect("complete lowered FOC bank"));
        Self::from_profiles(profiles, activation_ids, seeds, first_boundary)
    }

    fn from_profiles(
        profiles: [ServoFocAxisProfile; AXES],
        activation_ids: [u64; AXES],
        seeds: [ServoEncoderSeed; AXES],
        first_boundary: DeviceCycle,
    ) -> Result<Self, FocHardwareLoopError> {
        let prepared =
            ServoFocBank::prepare_activation(profiles, activation_ids, seeds, first_boundary)?;
        Self::activate_prepared(prepared, [first_boundary; AXES])
    }

    fn activate_prepared(
        prepared: PreparedServoFocBankActivation<AXES>,
        observed_timer_zero: [DeviceCycle; AXES],
    ) -> Result<Self, FocHardwareLoopError> {
        let configuration_digest = prepared.configuration_digest();
        let pwm_period_cycles = prepared.pwm_period_cycles();
        let first_boundary = prepared.first_boundary();
        let images = prepared.initial_images();
        let mut commit_owner = PwmCommitBankTargetOwner::new(
            ModeledPwmCommitHardware::new(observed_timer_zero),
            configuration_digest,
            pwm_period_cycles,
            first_boundary,
            0,
        )
        .map_err(|rejected| FocHardwareLoopError::CommitBarrier(rejected.error()))?;
        commit_owner
            .stage(images)
            .map_err(map_modeled_owner_error)?;
        for axis in 0..AXES {
            if !commit_owner
                .poll_latch(axis)
                .map_err(map_modeled_owner_error)?
            {
                match commit_owner.force_safe() {
                    Ok(()) => {}
                    Err(error) => match error {},
                }
                return Err(FocHardwareLoopError::Sequence);
            }
        }
        let controller = commit_owner
            .finish_and_publish(first_boundary, |completion| {
                ServoFocBank::activate(prepared, completion)
            })
            .map_err(map_modeled_publication_error)?;
        Ok(Self {
            controller,
            commit_owner,
        })
    }

    /// Complete portable simultaneous controller state retained by the owner.
    pub const fn controller(&self) -> &ServoFocBank<AXES> {
        &self.controller
    }

    /// Aggregate modeled-hardware owner state.
    pub const fn commit_owner_state(&self) -> PwmCommitBankOwnerState {
        self.commit_owner.state()
    }

    /// First aggregate modeled-hardware fault.
    pub const fn commit_owner_fault(&self) -> Option<PwmCommitBankOwnerFault> {
        self.commit_owner.fault()
    }

    /// Sole next physical boundary accepted by the aggregate owner.
    pub const fn next_commit_boundary(&self) -> DeviceCycle {
        self.commit_owner.next_boundary()
    }

    /// Sequence assigned to the next aggregate physical completion.
    pub const fn next_commit_sequence(&self) -> u64 {
        self.commit_owner.next_period_sequence()
    }

    /// Number of modeled all-stage safe transactions invoked by the owner.
    pub const fn safe_transaction_count(&self) -> u64 {
        self.commit_owner.hardware().safe_transactions
    }

    /// Executes one simultaneous current/PWM period at each exact next boundary.
    pub fn step(
        &mut self,
        inputs: [FocServoHardwareLoopInput; AXES],
    ) -> Result<[ServoFocAxisUpdate; AXES], FocHardwareLoopError> {
        let observed = [self.commit_owner.next_boundary(); AXES];
        self.step_with_timer_zero(inputs, observed)
    }

    /// Executes one bank period with per-axis physical-boundary fault injection.
    pub fn step_with_timer_zero(
        &mut self,
        inputs: [FocServoHardwareLoopInput; AXES],
        observed_timer_zero: [DeviceCycle; AXES],
    ) -> Result<[ServoFocAxisUpdate; AXES], FocHardwareLoopError> {
        if matches!(
            self.commit_owner.state(),
            PwmCommitBankOwnerState::SafeFault | PwmCommitBankOwnerState::UnsafeFault
        ) {
            return Err(FocHardwareLoopError::Faulted);
        }
        let prepared = match (|| {
            let mut period_inputs = core::array::from_fn(|_| None);
            for axis in 0..AXES {
                let controller = self
                    .controller
                    .axis(axis)
                    .ok_or(FocHardwareLoopError::Sequence)?;
                let profile = controller.profile();
                let active_image = controller
                    .active_image()
                    .ok_or(FocHardwareLoopError::Sequence)?;
                let at = controller.period_started_at();
                let stamp = exact_sample_stamp(
                    profile.pwm_compare,
                    &profile.parameters,
                    profile.current.snapshot().synchronization,
                    active_image,
                    controller.period_sequence(),
                    at,
                )?;
                period_inputs[axis] = Some(ServoFocAxisPeriodInput {
                    at,
                    setpoint: inputs[axis].setpoint,
                    encoder_observation: inputs[axis].encoder_observation,
                    raw_current_counts: inputs[axis].raw_current_counts,
                    current_stamp: stamp,
                    raw_rotor_count: inputs[axis].raw_rotor_count,
                });
            }
            let period_inputs =
                period_inputs.map(|input| input.expect("complete FOC bank period input"));
            self.controller
                .prepare(period_inputs)
                .map_err(FocHardwareLoopError::Bank)
        })() {
            Ok(prepared) => prepared,
            Err(error) => {
                self.force_safe_and_invalidate();
                return Err(error);
            }
        };
        let images = core::array::from_fn(|axis| {
            prepared
                .axis_update(axis)
                .expect("complete prepared FOC bank")
                .staged_image
        });
        self.commit_owner
            .hardware()
            .set_observed_timer_zero(observed_timer_zero);
        if let Err(error) = self.commit_owner.stage(images) {
            self.invalidate_controller_after_owner_fault();
            return Err(map_modeled_owner_error(error));
        }
        for axis in 0..AXES {
            match self.commit_owner.poll_latch(axis) {
                Ok(true) => {}
                Ok(false) => {
                    self.force_safe_and_invalidate();
                    return Err(FocHardwareLoopError::Sequence);
                }
                Err(error) => {
                    self.invalidate_controller_after_owner_fault();
                    return Err(map_modeled_owner_error(error));
                }
            }
        }
        let result = {
            let controller = &mut self.controller;
            self.commit_owner
                .finish_and_publish(images[0].scheduled_at(), |completion| {
                    controller.commit(prepared, completion)
                })
        };
        match result {
            Ok(updates) => Ok(updates),
            Err(error) => {
                self.invalidate_controller_after_owner_fault();
                Err(map_modeled_publication_error(error))
            }
        }
    }

    /// Invalidates the logical bank only after a separately modeled safe-output action.
    pub fn invalidate_after_safe(&mut self) {
        self.force_safe_and_invalidate();
    }

    fn force_safe_and_invalidate(&mut self) {
        match self.commit_owner.force_safe() {
            Ok(()) => self.controller.invalidate_after_safe(),
            Err(error) => match error {},
        }
    }

    fn invalidate_controller_after_owner_fault(&mut self) {
        if self.commit_owner.state() == PwmCommitBankOwnerState::SafeFault {
            self.controller.invalidate_after_safe();
        }
    }
}

/// Cached simultaneous-setpoint mailbox joined to a complete virtual FOC bank.
#[derive(Debug, Eq, PartialEq)]
pub struct ScheduledServoFocHardwareBank<const AXES: usize> {
    hardware: ConfiguredServoFocHardwareBank<AXES>,
    staged: Option<PreparedServoSetpoints<AXES>>,
    commit: Option<ServoSetpointOutputCommit>,
}

impl<const AXES: usize> ScheduledServoFocHardwareBank<AXES> {
    /// Constructs all axes through one independently validated configuration.
    pub fn from_configuration(
        configuration: &RealtimeConfiguration,
        slots: [usize; AXES],
        activation_ids: [u64; AXES],
        seeds: [ServoEncoderSeed; AXES],
        first_boundary: DeviceCycle,
    ) -> Result<Self, FocHardwareLoopError> {
        Ok(Self {
            hardware: ConfiguredServoFocHardwareBank::from_configuration(
                configuration,
                slots,
                activation_ids,
                seeds,
                first_boundary,
            )?,
            staged: None,
            commit: None,
        })
    }

    #[cfg(test)]
    fn from_lowered(
        lowered: [LoweredFocAxisConfiguration; AXES],
        activation_ids: [u64; AXES],
        seeds: [ServoEncoderSeed; AXES],
        first_boundary: DeviceCycle,
    ) -> Result<Self, FocHardwareLoopError> {
        Ok(Self {
            hardware: ConfiguredServoFocHardwareBank::from_lowered(
                lowered,
                activation_ids,
                seeds,
                first_boundary,
            )?,
            staged: None,
            commit: None,
        })
    }

    /// Complete underlying simultaneous encoder/cascade/current/PWM owner.
    pub const fn hardware(&self) -> &ConfiguredServoFocHardwareBank<AXES> {
        &self.hardware
    }

    /// Whether one future simultaneous cached batch remains staged.
    pub const fn has_staged_setpoints(&self) -> bool {
        self.staged.is_some()
    }

    /// Executes one bank current period and consumes a due simultaneous batch.
    pub fn step(
        &mut self,
        mut inputs: [FocServoHardwareLoopInput; AXES],
    ) -> Result<[ServoFocAxisUpdate; AXES], FocHardwareLoopError> {
        if inputs.iter().any(|input| input.setpoint.is_some()) || self.commit.is_some() {
            return Err(FocHardwareLoopError::Sequence);
        }
        let at = self.hardware.controller().period_started_at();
        let due = match self.staged {
            Some(prepared) if prepared.scheduled_at() < at => {
                return Err(FocHardwareLoopError::Sequence);
            }
            Some(prepared) if prepared.scheduled_at() == at => Some(prepared),
            Some(_) | None => None,
        };
        if let Some(prepared) = due {
            for (axis, input) in inputs.iter_mut().enumerate() {
                input.setpoint = Some(prepared.setpoints()[axis]);
            }
        }
        let updates = self.hardware.step(inputs)?;
        if let Some(prepared) = due {
            self.staged = None;
            self.commit = Some(ServoSetpointOutputCommit::new(
                prepared.token(),
                prepared.scheduled_at(),
            ));
        }
        Ok(updates)
    }

    /// Clears mailbox ownership after an enclosing all-stage safe transaction.
    pub fn invalidate_after_safe(&mut self) {
        self.staged = None;
        self.commit = None;
        self.hardware.invalidate_after_safe();
    }
}

impl<const AXES: usize> ServoSetpointOutput<AXES> for ScheduledServoFocHardwareBank<AXES> {
    type Error = FocHardwareLoopError;

    fn stage_servo_setpoints(
        &mut self,
        prepared: PreparedServoSetpoints<AXES>,
    ) -> Result<(), Self::Error> {
        if self.staged.is_some()
            || self.commit.is_some()
            || prepared.scheduled_at() < self.hardware.controller().period_started_at()
            || self.hardware.controller().fault().is_some()
        {
            return Err(FocHardwareLoopError::Sequence);
        }
        self.staged = Some(prepared);
        Ok(())
    }

    fn take_servo_setpoint_commit(
        &mut self,
    ) -> Result<Option<ServoSetpointOutputCommit>, Self::Error> {
        Ok(self.commit.take())
    }
}

/// One-axis scheduled-setpoint mailbox joined to the complete virtual FOC owner.
///
/// A staged cached command becomes a controller input only at its exact
/// position-loop boundary. The corresponding commit is published only after
/// the complete candidate controller state and next PWM compare image commit
/// successfully; a failed period cannot advance cached-stream ownership.
#[derive(Debug, Eq, PartialEq)]
pub struct ScheduledServoFocHardwareLoop {
    hardware: ConfiguredServoFocHardwareLoop,
    staged: Option<PreparedServoSetpoints<1>>,
    commit: Option<ServoSetpointOutputCommit>,
}

impl ScheduledServoFocHardwareLoop {
    /// Constructs through one independently validated canonical configuration.
    pub fn from_configuration(
        configuration: &RealtimeConfiguration,
        slot: usize,
        activation_id: u64,
        seed: ServoEncoderSeed,
        first_boundary: DeviceCycle,
    ) -> Result<Self, FocHardwareLoopError> {
        let hardware = ConfiguredServoFocHardwareLoop::from_configuration(
            configuration,
            slot,
            activation_id,
            seed,
            first_boundary,
        )?;
        Ok(Self {
            hardware,
            staged: None,
            commit: None,
        })
    }

    #[cfg(test)]
    fn from_lowered(
        lowered: LoweredFocAxisConfiguration,
        activation_id: u64,
        seed: ServoEncoderSeed,
        first_boundary: DeviceCycle,
    ) -> Result<Self, FocHardwareLoopError> {
        let hardware = ConfiguredServoFocHardwareLoop::from_lowered(
            lowered,
            activation_id,
            seed,
            first_boundary,
        )?;
        Ok(Self {
            hardware,
            staged: None,
            commit: None,
        })
    }

    /// Complete underlying encoder/cascade/current/PWM simulation owner.
    pub const fn hardware(&self) -> &ConfiguredServoFocHardwareLoop {
        &self.hardware
    }

    /// Executes one current/PWM period and consumes a due staged setpoint.
    pub fn step(
        &mut self,
        mut input: FocServoHardwareLoopInput,
    ) -> Result<ServoFocAxisUpdate, FocHardwareLoopError> {
        if input.setpoint.is_some() || self.commit.is_some() {
            return Err(FocHardwareLoopError::Sequence);
        }
        let at = self.hardware.controller().period_started_at();
        let due = match self.staged {
            Some(prepared) if prepared.scheduled_at() < at => {
                return Err(FocHardwareLoopError::Sequence);
            }
            Some(prepared) if prepared.scheduled_at() == at => Some(prepared),
            Some(_) | None => None,
        };
        if let Some(prepared) = due {
            input.setpoint = Some(prepared.setpoints()[0]);
        }
        let update = self.hardware.step(input)?;
        if let Some(prepared) = due {
            self.staged = None;
            self.commit = Some(ServoSetpointOutputCommit::new(
                prepared.token(),
                prepared.scheduled_at(),
            ));
        }
        Ok(update)
    }
}

impl ServoSetpointOutput<1> for ScheduledServoFocHardwareLoop {
    type Error = FocHardwareLoopError;

    fn stage_servo_setpoints(
        &mut self,
        prepared: PreparedServoSetpoints<1>,
    ) -> Result<(), Self::Error> {
        if self.staged.is_some()
            || self.commit.is_some()
            || prepared.scheduled_at() < self.hardware.controller().period_started_at()
        {
            return Err(FocHardwareLoopError::Sequence);
        }
        self.staged = Some(prepared);
        Ok(())
    }

    fn take_servo_setpoint_commit(
        &mut self,
    ) -> Result<Option<ServoSetpointOutputCommit>, Self::Error> {
        Ok(self.commit.take())
    }
}

/// Allocation-free virtual owner for one configuration-derived FOC axis.
#[derive(Debug, Eq, PartialEq)]
pub struct ConfiguredFocHardwareLoop {
    lowered: LoweredFocAxisConfiguration,
    controller: DqCurrentController,
    compare_owner: PwmCompareLatchOwner,
    period_started_at: DeviceCycle,
    period_sequence: u32,
    next_duty_token: u32,
    next_command_id: u32,
    faulted: bool,
}

impl ConfiguredFocHardwareLoop {
    /// Constructs only from the private, independently validated configuration container.
    pub fn from_configuration(
        configuration: &RealtimeConfiguration,
        slot: usize,
        first_boundary: DeviceCycle,
    ) -> Result<Self, FocHardwareLoopError> {
        let lowered = configuration.lower_foc_axis(slot)?;
        Self::from_lowered(lowered, first_boundary)
    }

    fn from_lowered(
        lowered: LoweredFocAxisConfiguration,
        first_boundary: DeviceCycle,
    ) -> Result<Self, FocHardwareLoopError> {
        lowered.validate()?;
        let parameters = lowered.parameters;
        let synchronization = lowered.current.snapshot().synchronization;
        if parameters.timing.current_loop_hz != parameters.timing.pwm_hz {
            return Err(FocHardwareLoopError::Rate);
        }
        let _next_boundary = first_boundary
            .0
            .checked_add(u64::from(synchronization.pwm_period_cycles))
            .ok_or(FocHardwareLoopError::Overflow)?;

        let mut compare_owner = PwmCompareLatchOwner::new(lowered.pwm_compare, first_boundary)?;
        let neutral = space_vector_modulate(
            AlphaBeta {
                alpha: Q30Interval::ZERO,
                beta: Q30Interval::ZERO,
            },
            parameters.maximum_phase_voltage,
        )?;
        let image = lowered.pwm_compare.lower(
            &parameters,
            synchronization,
            INITIAL_DUTY_TOKEN,
            first_boundary,
            neutral.duties,
        )?;
        compare_owner.stage(image, &parameters, synchronization)?;
        let initial = compare_owner
            .observe_timer_zero(first_boundary)?
            .ok_or(FocHardwareLoopError::Sequence)?;
        let period_sequence =
            u32::try_from(initial.period_sequence).map_err(|_| FocHardwareLoopError::Overflow)?;

        Ok(Self {
            lowered,
            controller: DqCurrentController::from_snapshot(&parameters)?,
            compare_owner,
            period_started_at: first_boundary,
            period_sequence,
            next_duty_token: INITIAL_DUTY_TOKEN + 1,
            next_command_id: INITIAL_COMMAND_ID,
            faulted: false,
        })
    }

    /// Exact configuration-derived facts retained by this virtual owner.
    pub const fn lowered(&self) -> LoweredFocAxisConfiguration {
        self.lowered
    }

    /// Start of the active PWM period and required command schedule.
    pub const fn period_started_at(&self) -> DeviceCycle {
        self.period_started_at
    }

    /// Whether any prior loop, edge, timing, or sequence rejection latched closed.
    pub const fn is_faulted(&self) -> bool {
        self.faulted
    }

    /// Executes one period and observes the exact next timer-zero boundary.
    pub fn step(
        &mut self,
        input: FocHardwareLoopInput,
    ) -> Result<FocHardwareLoopSample, FocHardwareLoopError> {
        let observed = self.compare_owner.next_boundary();
        self.step_with_timer_zero(input, observed)
    }

    /// Executes one period with an explicit timer-zero observation for fault injection.
    pub fn step_with_timer_zero(
        &mut self,
        input: FocHardwareLoopInput,
        observed_timer_zero: DeviceCycle,
    ) -> Result<FocHardwareLoopSample, FocHardwareLoopError> {
        if self.faulted {
            return Err(FocHardwareLoopError::Faulted);
        }
        let result = self.step_inner(input, observed_timer_zero);
        if result.is_err() {
            self.faulted = true;
        }
        result
    }

    fn step_inner(
        &mut self,
        input: FocHardwareLoopInput,
        observed_timer_zero: DeviceCycle,
    ) -> Result<FocHardwareLoopSample, FocHardwareLoopError> {
        let parameters = self.lowered.parameters;
        let synchronization = self.lowered.current.snapshot().synchronization;
        if input.command.command_id != self.next_command_id
            || input.command.scheduled_at != self.period_started_at
        {
            return Err(FocHardwareLoopError::Sequence);
        }
        input.command.validate(&parameters)?;
        let next_command_id = self
            .next_command_id
            .checked_add(1)
            .ok_or(FocHardwareLoopError::Overflow)?;
        let next_duty_token = self
            .next_duty_token
            .checked_add(1)
            .ok_or(FocHardwareLoopError::Overflow)?;
        let active_image = self
            .compare_owner
            .active_image()
            .ok_or(FocHardwareLoopError::Sequence)?;
        let stamp = exact_sample_stamp(
            self.lowered.pwm_compare,
            &parameters,
            synchronization,
            active_image,
            self.period_sequence,
            self.period_started_at,
        )?;
        let current = self
            .lowered
            .current
            .observe(input.raw_current_counts, stamp)?;
        current.validate_for(&parameters, self.lowered.current)?;
        let rotor = self.lowered.rotor.observe(
            input.raw_rotor_count,
            stamp.channel0_sampled_at,
            self.lowered.rotation_precision,
        )?;
        rotor.validate_for(&parameters, self.lowered.rotation_precision)?;
        let measured_interval = park(current.stationary()?, rotor.rotation)?;
        let measured = DqPoint {
            d: measured_interval.d.midpoint(),
            q: measured_interval.q.midpoint(),
        };
        let mut next_controller = self.controller;
        let control = next_controller.update(
            input.command.target,
            measured,
            input.command.voltage_feed_forward,
        )?;
        let stationary_voltage = inverse_park(
            DqInterval {
                d: Q30Interval::point(control.voltage.d),
                q: Q30Interval::point(control.voltage.q),
            },
            rotor.rotation,
        )?;
        let modulation =
            space_vector_modulate(stationary_voltage, parameters.maximum_phase_voltage)?;
        let scheduled_at = self.compare_owner.next_boundary();
        let image = self.lowered.pwm_compare.lower(
            &parameters,
            synchronization,
            self.next_duty_token,
            scheduled_at,
            modulation.duties,
        )?;
        self.compare_owner
            .stage(image, &parameters, synchronization)?;
        let committed = self
            .compare_owner
            .observe_timer_zero(observed_timer_zero)?
            .ok_or(FocHardwareLoopError::Sequence)?;
        let period_sequence =
            u32::try_from(committed.period_sequence).map_err(|_| FocHardwareLoopError::Overflow)?;

        self.controller = next_controller;
        self.period_started_at = committed.observed_at;
        self.period_sequence = period_sequence;
        self.next_duty_token = next_duty_token;
        self.next_command_id = next_command_id;
        Ok(FocHardwareLoopSample {
            input,
            active_image,
            current,
            rotor,
            measured_interval,
            measured,
            control,
            modulation,
            committed,
        })
    }
}

fn exact_sample_stamp(
    contract: alumina_foc::PwmCompareContract,
    parameters: &alumina_foc::FocParameterSnapshot,
    synchronization: PwmAdcSynchronization,
    active_image: PwmCompareImage,
    period_sequence: u32,
    period_started_at: DeviceCycle,
) -> Result<PwmAdcSampleStamp, FocHardwareLoopError> {
    active_image.validate_for(contract, parameters, synchronization)?;
    let acquisition_offset = u64::from(synchronization.nominal_acquisition_offset_cycles);
    let channel_skew = u64::from(synchronization.maximum_channel_skew_cycles != 0);
    let channel0_offset = acquisition_offset;
    let channel1_offset = acquisition_offset
        .checked_add(channel_skew)
        .ok_or(FocHardwareLoopError::Overflow)?;
    let last_sample_offset = channel0_offset.max(channel1_offset);
    let conversion_offset = last_sample_offset
        .checked_add(1)
        .ok_or(FocHardwareLoopError::Overflow)?;

    let mut previous_offset = None;
    let mut next_offset = None;
    for comparison in active_image.comparisons() {
        for edge_ticks in [
            comparison.first_switching_edge_ticks(),
            comparison.second_switching_edge_ticks(),
        ] {
            let offset = counter_ticks_to_device_cycles(contract, edge_ticks)?;
            if offset <= acquisition_offset {
                previous_offset =
                    Some(previous_offset.map_or(offset, |prior: u64| prior.max(offset)));
            }
            if offset >= last_sample_offset {
                next_offset = Some(next_offset.map_or(offset, |next: u64| next.min(offset)));
            }
        }
    }
    let previous_offset = previous_offset.ok_or(FocHardwareLoopError::ClockDomain)?;
    let next_offset = next_offset.ok_or(FocHardwareLoopError::ClockDomain)?;
    let stamp = PwmAdcSampleStamp {
        configuration_digest: contract.configuration_digest(),
        duty_token: active_image.token(),
        period_sequence,
        period_started_at,
        previous_switching_edge_at: add_cycles(period_started_at, previous_offset)?,
        acquisition_started_at: add_cycles(period_started_at, acquisition_offset)?,
        channel0_sampled_at: add_cycles(period_started_at, channel0_offset)?,
        channel1_sampled_at: add_cycles(period_started_at, channel1_offset)?,
        conversion_completed_at: add_cycles(period_started_at, conversion_offset)?,
        next_switching_edge_at: add_cycles(period_started_at, next_offset)?,
    };
    synchronization.validate_stamp(stamp)?;
    Ok(stamp)
}

fn counter_ticks_to_device_cycles(
    contract: alumina_foc::PwmCompareContract,
    edge_ticks: u32,
) -> Result<u64, FocHardwareLoopError> {
    let numerator = u64::from(edge_ticks)
        .checked_mul(u64::from(contract.device_cycle_hz()))
        .ok_or(FocHardwareLoopError::Overflow)?;
    let divisor = u64::from(contract.counter_clock_hz());
    if !numerator.is_multiple_of(divisor) {
        return Err(FocHardwareLoopError::ClockDomain);
    }
    Ok(numerator / divisor)
}

fn add_cycles(base: DeviceCycle, offset: u64) -> Result<DeviceCycle, FocHardwareLoopError> {
    base.0
        .checked_add(offset)
        .map(DeviceCycle)
        .ok_or(FocHardwareLoopError::Overflow)
}

#[cfg(test)]
mod tests {
    use alumina_board::{BoardPackage, OwnerDomain, ResourceId, SupportLevel};
    use alumina_capability::calculate_identity;
    use alumina_config::{
        BindingFlags, BindingRole, ConfigurationFlags, ConfigurationHeader, ConfigurationRecord,
        ConfigurationStreamValidator, ExactScalar, FactEvidence, FocAdcAttenuation,
        FocAdcFrontendParameters, FocControllerAxis, FocControllerParameters, FocCurrentChannel,
        FocCurrentChannelParameters, FocEncoderPolicyParameters, FocEncoderScaleParameters,
        FocPwmAdcTimingParameters, FocPwmHardwareParameters, FocRotorParameters,
        FocRuntimeParameters, FocServoParameters, FocShutdownContract, FocShutdownStrategy,
        Rational, RealtimeConfiguration, ResourceBinding, ScalarFact, SignalPolarity,
    };
    use alumina_foc::{
        CountUncertainty, CurrentChannelCalibration, CurrentPolarity, ElectricalPhase,
        FocParameterSnapshot, FocTimingProfile, PiConfig, PwmAdcSynchronization,
        PwmCommitBankBarrier, PwmCompareContract, Q30, RotationPrecision, RotorCalibration,
        RotorCountDirection, ServoCascadeConfig, ServoEncoderProfile, ServoEncoderScale,
        ServoLoopGrid, ServoPosition, TwoShuntCurrentCalibration, TwoShuntPhasePair,
    };
    use alumina_job::{JobDescriptor, RealtimeJob, RealtimeJobState, RealtimePoll, WorkSource};
    use alumina_machine_ir::{
        BlockValidationLimits, ExecutionBlock, ExecutionKind, FiniteDifferenceAxis,
        ServoFiniteDifferenceAxis, ServoFiniteDifferenceSegment, ServoQ30FiniteDifferenceAxis,
        StreamId, StreamTick, ValidationLimits,
    };
    use alumina_motion::{
        CachedServoConfiguration, CachedServoStreamPolicy, ScheduledServoAction,
        ScheduledServoExecution, ServoSetpointAxisAdmissionProfile, cached_servo_admission_profile,
    };
    use alumina_protocol::Digest;
    use alumina_storage::{ContentId, ObjectKind, PublishedObject, StoredObject, sha256};

    use super::*;

    const DIGEST: Digest = Digest([0x94; 32]);
    const ACCEPTED_COMPARE_ERROR_ULPS: u32 = 1_200_000;
    const CANONICAL_DUAL_MKS_CONFIGURATION_DIGEST: Digest = Digest([
        0x4a, 0x21, 0x13, 0x2e, 0xca, 0x68, 0xdf, 0x83, 0x0c, 0x27, 0xf8, 0xa0, 0x0c, 0xca, 0xfd,
        0xeb, 0x26, 0xe9, 0x75, 0x4a, 0xec, 0x88, 0x3c, 0x1f, 0x50, 0x26, 0xf7, 0x2f, 0x2a, 0x8e,
        0x85, 0x35,
    ]);

    fn exact_rational(numerator: i64, denominator: u64) -> Rational {
        Rational::new(numerator, denominator).unwrap()
    }

    fn configured_binding(
        instance: u16,
        role: BindingRole,
        resource: ResourceId,
        polarity: SignalPolarity,
        timed: bool,
    ) -> ConfigurationRecord {
        let required_interlock = matches!(
            role,
            BindingRole::EmergencyStop | BindingRole::SafetyInterlock
        );
        ConfigurationRecord::Binding(ResourceBinding {
            instance,
            role,
            resource,
            owner: OwnerDomain::Realtime,
            polarity,
            flags: BindingFlags(if required_interlock {
                BindingFlags::REQUIRED_INTERLOCK
            } else {
                0
            }),
            minimum_active_cycles: u32::from(timed) * 48,
            minimum_inactive_cycles: u32::from(timed) * 48,
            maximum_frequency_hz: u32::from(timed) * 100_000,
            watchdog_cycles: 240_000,
        })
    }

    fn configured_sample_binding(
        instance: u16,
        role: BindingRole,
        resource: ResourceId,
        maximum_frequency_hz: u32,
    ) -> ConfigurationRecord {
        ConfigurationRecord::Binding(ResourceBinding {
            instance,
            role,
            resource,
            owner: OwnerDomain::Realtime,
            polarity: SignalPolarity::NotApplicable,
            flags: BindingFlags::default(),
            minimum_active_cycles: 0,
            minimum_inactive_cycles: 0,
            maximum_frequency_hz,
            watchdog_cycles: 240_000,
        })
    }

    fn configured_scalar(instance: u16, fact: ScalarFact, value: Rational) -> ConfigurationRecord {
        ConfigurationRecord::Scalar(ExactScalar {
            instance,
            fact,
            value,
            uncertainty: exact_rational(0, 1),
            evidence: FactEvidence::Declared,
        })
    }

    fn canonical_mks_axis_records(
        instance: u16,
        pwm_engine: u8,
        adc_channels: [u8; 2],
        power_stage: u16,
        encoder: u16,
        include_estop: bool,
        include_timer_tick: bool,
    ) -> Vec<ConfigurationRecord> {
        let controller = PiConfig {
            proportional_gain: Q30::ZERO,
            integral_gain_per_update: Q30::ZERO,
            integral_minimum: Q30::NEG_ONE,
            integral_maximum: Q30::ONE,
            output_minimum: Q30::from_bits(-Q30::HALF.bits()),
            output_maximum: Q30::HALF,
        };
        let current_channel = CurrentChannelCalibration {
            adc_maximum_count: 4_095,
            valid_count_minimum: 1_200,
            valid_count_maximum: 2_800,
            count_at_zero: 2_000,
            polarity: CurrentPolarity::Increasing,
            normalized_current_per_count: Q30Interval::point(Q30::from_bits(1 << 19)),
            maximum_additive_error: Q30::from_bits(1 << 18),
            maximum_interval_width_ulps: 1 << 19,
        };
        let mut records = Vec::from([
            configured_binding(
                instance,
                BindingRole::FocPhaseU,
                ResourceId::TimedOutput {
                    engine: pwm_engine,
                    channel: 0,
                },
                SignalPolarity::ActiveHigh,
                true,
            ),
            configured_binding(
                instance,
                BindingRole::FocPhaseV,
                ResourceId::TimedOutput {
                    engine: pwm_engine,
                    channel: 1,
                },
                SignalPolarity::ActiveHigh,
                true,
            ),
            configured_binding(
                instance,
                BindingRole::FocPhaseW,
                ResourceId::TimedOutput {
                    engine: pwm_engine,
                    channel: 2,
                },
                SignalPolarity::ActiveHigh,
                true,
            ),
            configured_sample_binding(
                instance,
                BindingRole::FocCurrentA,
                ResourceId::Adc {
                    unit: 1,
                    channel: adc_channels[0],
                },
                20_000,
            ),
            configured_sample_binding(
                instance,
                BindingRole::FocCurrentB,
                ResourceId::Adc {
                    unit: 1,
                    channel: adc_channels[1],
                },
                20_000,
            ),
            configured_sample_binding(
                instance,
                BindingRole::FocEncoder,
                ResourceId::Device(encoder),
                1_000,
            ),
            ConfigurationRecord::FocShutdown(FocShutdownContract {
                instance,
                strategy: FocShutdownStrategy::PhaseHighImpedance,
                power_stage: ResourceId::Device(power_stage),
                control: None,
                control_polarity: SignalPolarity::NotApplicable,
                maximum_transition_cycles: 2_400,
                evidence: FactEvidence::Qualified,
            }),
        ]);
        if include_estop {
            records.push(configured_binding(
                0,
                BindingRole::EmergencyStop,
                ResourceId::Gpio(15),
                SignalPolarity::ActiveLow,
                false,
            ));
        }

        for (fact, value) in [
            (
                ScalarFact::AxisMotorTurnsPerOutputTurn,
                exact_rational(1, 1),
            ),
            (
                ScalarFact::AxisTravelMetresPerOutputTurn,
                exact_rational(1, 100),
            ),
            (ScalarFact::AxisCalibrationScale, exact_rational(1, 1)),
            (ScalarFact::AxisPositionMinimumMetres, exact_rational(-1, 1)),
            (ScalarFact::AxisPositionMaximumMetres, exact_rational(1, 1)),
            (
                ScalarFact::AxisVelocityLimitMetresPerSecond,
                exact_rational(1, 10),
            ),
            (
                ScalarFact::AxisAccelerationLimitMetresPerSecondSquared,
                exact_rational(1, 1),
            ),
            (
                ScalarFact::AxisJerkLimitMetresPerSecondCubed,
                exact_rational(10, 1),
            ),
            (
                ScalarFact::AxisFollowingErrorMetres,
                exact_rational(1, 1_000),
            ),
            (
                ScalarFact::AxisEncoderCountsPerTurn,
                exact_rational(4_096, 1),
            ),
            (ScalarFact::MotorPolePairs, exact_rational(7, 1)),
            (ScalarFact::MotorCurrentLimitAmperes, exact_rational(1, 2)),
            (ScalarFact::MotorVoltageLimitVolts, exact_rational(6, 1)),
            (ScalarFact::PwmCarrierHertz, exact_rational(20_000, 1)),
            (
                ScalarFact::PwmDeadTimeSeconds,
                exact_rational(1, 10_000_000),
            ),
            (ScalarFact::ControlRateHertz, exact_rational(20_000, 1)),
            (ScalarFact::CurrentSenseOhms, exact_rational(1, 100)),
            (ScalarFact::CurrentSenseVoltsPerAmpere, exact_rational(1, 1)),
        ] {
            records.push(configured_scalar(instance, fact, value));
        }
        if include_timer_tick {
            records.push(configured_scalar(
                instance,
                ScalarFact::TimerTickHertz,
                exact_rational(1_000_000, 1),
            ));
        }

        records.extend([
            ConfigurationRecord::FocRuntime(FocRuntimeParameters {
                instance,
                pole_pairs: 7,
                timing: FocTimingProfile {
                    pwm_hz: 20_000,
                    current_loop_hz: 20_000,
                    velocity_loop_divider: 20,
                    position_loop_divider: 10,
                },
                maximum_phase_current: Q30::ONE,
                maximum_phase_voltage: Q30::ONE,
            }),
            ConfigurationRecord::FocController(FocControllerParameters {
                instance,
                axis: FocControllerAxis::Direct,
                parameters: controller,
            }),
            ConfigurationRecord::FocController(FocControllerParameters {
                instance,
                axis: FocControllerAxis::Quadrature,
                parameters: controller,
            }),
            ConfigurationRecord::FocRotor(FocRotorParameters {
                instance,
                counts_per_mechanical_turn: 4_096,
                count_at_reference: 0,
                electrical_phase_at_reference: ElectricalPhase::ZERO,
                direction: RotorCountDirection::Increasing,
                maximum_alignment_error_bits: 1_024,
                maximum_count_error: CountUncertainty::new(1, 2).unwrap(),
                rotation_precision: RotationPrecision {
                    maximum_component_width_ulps: 12_000_000,
                    maximum_norm_error_ulps: 12_000_000,
                },
                evidence: FactEvidence::Measured,
            }),
            ConfigurationRecord::FocCurrentChannel(FocCurrentChannelParameters {
                instance,
                channel: FocCurrentChannel::Channel0,
                calibration: current_channel,
                evidence: FactEvidence::Qualified,
            }),
            ConfigurationRecord::FocCurrentChannel(FocCurrentChannelParameters {
                instance,
                channel: FocCurrentChannel::Channel1,
                calibration: current_channel,
                evidence: FactEvidence::Qualified,
            }),
            ConfigurationRecord::FocPwmAdcTiming(FocPwmAdcTimingParameters {
                instance,
                phase_pair: TwoShuntPhasePair::Ab,
                device_cycle_hz: 80_000_000,
                pwm_period_cycles: 4_000,
                nominal_acquisition_offset_cycles: 2_000,
                maximum_trigger_jitter_cycles: 2,
                maximum_acquisition_cycles: 20,
                maximum_channel_skew_cycles: 8,
                maximum_conversion_cycles: 40,
                minimum_switching_guard_cycles: 50,
                maximum_normalized_current_slew_per_cycle: Q30::from_bits(1 << 14),
                maximum_interchannel_skew_error: Q30::from_bits(1 << 17),
                maximum_phase_current: Q30::ONE,
                maximum_phase_interval_width_ulps: 1 << 22,
                pwm_dead_time_cycles: 8,
                evidence: FactEvidence::Qualified,
            }),
            ConfigurationRecord::FocAdcFrontend(FocAdcFrontendParameters {
                instance,
                channel: FocCurrentChannel::Channel0,
                attenuation: FocAdcAttenuation::Db11,
                evidence: FactEvidence::Qualified,
            }),
            ConfigurationRecord::FocAdcFrontend(FocAdcFrontendParameters {
                instance,
                channel: FocCurrentChannel::Channel1,
                attenuation: FocAdcAttenuation::Db11,
                evidence: FactEvidence::Qualified,
            }),
            ConfigurationRecord::FocPwmHardware(FocPwmHardwareParameters {
                instance,
                peripheral_source_clock_hz: 160_000_000,
                counter_clock_hz: 80_000_000,
                timer_peak_ticks: 2_000,
                minimum_active_ticks: 8,
                maximum_quantization_error_ulps: 1_000_000,
                peripheral_prescaler: 1,
                timer_prescaler: 0,
                evidence: FactEvidence::Qualified,
            }),
            ConfigurationRecord::FocServo(FocServoParameters {
                instance,
                position_proportional_gain: Q30::ONE,
                velocity_controller: controller,
                maximum_velocity: Q30::ONE,
                maximum_current: Q30::ONE,
                direct_current_target: Q30::ZERO,
                maximum_following_error_bits: 4_294_967,
                maximum_sample_age_cycles: 80_000,
            }),
            ConfigurationRecord::FocEncoderScale(FocEncoderScaleParameters {
                instance,
                position_at_reference: ServoPosition::ZERO,
                scale: ServoEncoderScale::new(1_073_741_824, 25, 40_960, 1).unwrap(),
                evidence: FactEvidence::Measured,
            }),
            ConfigurationRecord::FocEncoderPolicy(FocEncoderPolicyParameters {
                instance,
                device_cycle_hz: 80_000_000,
                sample_period_cycles: 80_000,
                maximum_observation_latency_cycles: 20_000,
                maximum_trackable_velocity: Q30::ONE,
                maximum_admitted_velocity: Q30::ONE,
                maximum_velocity_estimation_error: Q30::from_bits(1 << 24),
                maximum_position_interval_width_ulps: 20_000,
                maximum_velocity_interval_width_ulps: 100_000_000,
                evidence: FactEvidence::Qualified,
            }),
        ]);
        records
    }

    fn canonical_dual_mks_configuration() -> RealtimeConfiguration {
        let mut devices = Vec::from(board_mks_esp32_foc_v1::PACKAGE.devices);
        for device in &mut devices[..2] {
            device.support = SupportLevel::Qualified;
        }
        let mut package = BoardPackage {
            devices: &devices,
            ..board_mks_esp32_foc_v1::PACKAGE
        };
        package.board.capability_digest = Digest([1; 32]);
        package.board.capability_digest = calculate_identity(&package).unwrap().digest;

        let mut records = canonical_mks_axis_records(
            0,
            0,
            [3, 0],
            board_mks_esp32_foc_v1::device::POWER_STAGE_0,
            board_mks_esp32_foc_v1::device::ENCODER_0,
            true,
            true,
        );
        records.extend(canonical_mks_axis_records(
            1,
            1,
            [7, 6],
            board_mks_esp32_foc_v1::device::POWER_STAGE_1,
            board_mks_esp32_foc_v1::device::ENCODER_1,
            false,
            false,
        ));
        records.sort_by_key(|record| record.canonical_order_key());
        let realtime_record_count = records
            .iter()
            .filter(|record| record.realtime_relevant())
            .count();
        let header = ConfigurationHeader {
            capability_digest: package.board.capability_digest,
            record_count: u16::try_from(records.len()).unwrap(),
            realtime_record_count: u16::try_from(realtime_record_count).unwrap(),
            flags: ConfigurationFlags(
                ConfigurationFlags::MOTION | ConfigurationFlags::FIELD_ORIENTED_CONTROL,
            ),
        };
        let mut bytes = Vec::from(header.encode().unwrap());
        for record in records {
            bytes.extend_from_slice(&record.encode().unwrap());
        }
        let digest = sha256(&bytes).digest;
        assert_eq!(digest, CANONICAL_DUAL_MKS_CONFIGURATION_DIGEST);
        let mut validator = ConfigurationStreamValidator::<64>::new(
            &package,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        for chunk in bytes.chunks(173) {
            validator.push(chunk).unwrap();
        }
        validator.finish_configuration().unwrap()
    }

    fn point(numerator: i64, denominator: i64) -> Q30 {
        Q30Interval::from_ratio(numerator, denominator)
            .unwrap()
            .midpoint()
    }

    fn current_channel() -> CurrentChannelCalibration {
        CurrentChannelCalibration {
            adc_maximum_count: 4_095,
            valid_count_minimum: 1_200,
            valid_count_maximum: 2_800,
            count_at_zero: 2_000,
            polarity: CurrentPolarity::Increasing,
            normalized_current_per_count: Q30Interval::point(Q30::from_bits(1 << 19)),
            maximum_additive_error: Q30::from_bits(1 << 18),
            maximum_interval_width_ulps: 1 << 19,
        }
    }

    fn lowered_fixture() -> LoweredFocAxisConfiguration {
        lowered_with_lattices(
            80_000_000,
            4_000,
            80_000_000,
            2_000,
            8,
            8,
            ACCEPTED_COMPARE_ERROR_ULPS,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the test fixture exposes each independent timer and precision fact"
    )]
    fn lowered_with_lattices(
        device_cycle_hz: u32,
        pwm_period_cycles: u32,
        counter_clock_hz: u32,
        timer_peak_ticks: u16,
        minimum_active_ticks: u16,
        dead_time_cycles: u32,
        maximum_quantization_error_ulps: u32,
    ) -> LoweredFocAxisConfiguration {
        let pwm_hz = device_cycle_hz / pwm_period_cycles;
        let controller = PiConfig {
            proportional_gain: Q30::HALF,
            integral_gain_per_update: point(1, 32),
            integral_minimum: Q30::from_bits(-Q30::HALF.bits()),
            integral_maximum: Q30::HALF,
            output_minimum: Q30::from_bits(-Q30::HALF.bits()),
            output_maximum: Q30::HALF,
        };
        let parameters = FocParameterSnapshot {
            configuration_digest: DIGEST,
            pole_pairs: 7,
            timing: FocTimingProfile {
                pwm_hz,
                current_loop_hz: pwm_hz,
                velocity_loop_divider: 20,
                position_loop_divider: 10,
            },
            maximum_phase_current: Q30::ONE,
            maximum_phase_voltage: Q30::ONE,
            d_current: controller,
            q_current: controller,
        };
        let rotor = RotorCalibration {
            configuration_digest: DIGEST,
            counts_per_mechanical_turn: 4_096,
            count_at_reference: 0,
            electrical_phase_at_reference: ElectricalPhase::ZERO,
            pole_pairs: 7,
            direction: RotorCountDirection::Increasing,
            maximum_alignment_error_bits: 1_024,
            maximum_count_error: CountUncertainty::new(1, 2).unwrap(),
        };
        let synchronization = if pwm_period_cycles == 4_000 {
            PwmAdcSynchronization {
                configuration_digest: DIGEST,
                device_cycle_hz,
                pwm_period_cycles,
                nominal_acquisition_offset_cycles: 2_000,
                maximum_trigger_jitter_cycles: 2,
                maximum_acquisition_cycles: 20,
                maximum_channel_skew_cycles: 8,
                maximum_conversion_cycles: 40,
                minimum_switching_guard_cycles: 50,
            }
        } else {
            PwmAdcSynchronization {
                configuration_digest: DIGEST,
                device_cycle_hz,
                pwm_period_cycles,
                nominal_acquisition_offset_cycles: pwm_period_cycles / 2,
                maximum_trigger_jitter_cycles: 0,
                maximum_acquisition_cycles: 1,
                maximum_channel_skew_cycles: 0,
                maximum_conversion_cycles: 1,
                minimum_switching_guard_cycles: 1,
            }
        };
        let current = TwoShuntCurrentCalibration {
            configuration_digest: DIGEST,
            phase_pair: TwoShuntPhasePair::Ab,
            channel0: current_channel(),
            channel1: current_channel(),
            synchronization,
            maximum_normalized_current_slew_per_cycle: Q30::from_bits(1 << 14),
            maximum_interchannel_skew_error: if synchronization.maximum_channel_skew_cycles == 0 {
                Q30::ZERO
            } else {
                Q30::from_bits(1 << 17)
            },
            maximum_phase_current: Q30::ONE,
            maximum_phase_interval_width_ulps: 1 << 22,
        }
        .validated()
        .unwrap();
        let pwm_hardware = FocPwmHardwareParameters {
            instance: 0,
            peripheral_source_clock_hz: counter_clock_hz,
            counter_clock_hz,
            timer_peak_ticks,
            minimum_active_ticks,
            maximum_quantization_error_ulps,
            peripheral_prescaler: 0,
            timer_prescaler: 0,
            evidence: FactEvidence::Qualified,
        };
        let pwm_compare = PwmCompareContract::new(
            DIGEST,
            device_cycle_hz,
            pwm_period_cycles,
            counter_clock_hz,
            timer_peak_ticks,
            minimum_active_ticks,
            maximum_quantization_error_ulps,
        )
        .unwrap();
        let servo_grid =
            ServoLoopGrid::new(DeviceCycle(0), device_cycle_hz, parameters.timing).unwrap();
        let servo_parameters = FocServoParameters {
            instance: 0,
            position_proportional_gain: Q30::ZERO,
            velocity_controller: controller,
            maximum_velocity: Q30::ONE,
            maximum_current: Q30::ONE,
            direct_current_target: Q30::ZERO,
            maximum_following_error_bits: u64::MAX,
            maximum_sample_age_cycles: servo_grid.velocity_period_cycles(),
        };
        let servo = ServoCascadeConfig {
            configuration_digest: DIGEST,
            position_proportional_gain: servo_parameters.position_proportional_gain,
            velocity_controller: servo_parameters.velocity_controller,
            maximum_velocity: servo_parameters.maximum_velocity,
            maximum_current: servo_parameters.maximum_current,
            direct_current_target: servo_parameters.direct_current_target,
            maximum_following_error_bits: servo_parameters.maximum_following_error_bits,
            maximum_sample_age_cycles: servo_parameters.maximum_sample_age_cycles,
        };
        let encoder_scale_parameters = FocEncoderScaleParameters {
            instance: 0,
            position_at_reference: ServoPosition::ZERO,
            scale: ServoEncoderScale::new(1, 1, 1, 1).unwrap(),
            evidence: FactEvidence::Measured,
        };
        let encoder_policy_parameters = FocEncoderPolicyParameters {
            instance: 0,
            device_cycle_hz,
            sample_period_cycles: servo_grid.velocity_period_cycles(),
            maximum_observation_latency_cycles: 0,
            maximum_trackable_velocity: Q30::ONE,
            maximum_admitted_velocity: Q30::ONE,
            maximum_velocity_estimation_error: Q30::from_bits(1),
            maximum_position_interval_width_ulps: u64::MAX,
            maximum_velocity_interval_width_ulps: u32::MAX,
            evidence: FactEvidence::Qualified,
        };
        let encoder = ServoEncoderProfile {
            configuration_digest: DIGEST,
            counts_per_mechanical_turn: rotor.counts_per_mechanical_turn,
            count_at_reference: rotor.count_at_reference,
            direction: rotor.direction,
            maximum_count_error: rotor.maximum_count_error,
            position_at_reference: encoder_scale_parameters.position_at_reference,
            scale: encoder_scale_parameters.scale,
            device_cycle_hz: encoder_policy_parameters.device_cycle_hz,
            sample_period_cycles: encoder_policy_parameters.sample_period_cycles,
            maximum_observation_latency_cycles: encoder_policy_parameters
                .maximum_observation_latency_cycles,
            maximum_trackable_velocity: encoder_policy_parameters.maximum_trackable_velocity,
            maximum_admitted_velocity: encoder_policy_parameters.maximum_admitted_velocity,
            maximum_velocity_estimation_error: encoder_policy_parameters
                .maximum_velocity_estimation_error,
            maximum_position_interval_width_ulps: encoder_policy_parameters
                .maximum_position_interval_width_ulps,
            maximum_velocity_interval_width_ulps: encoder_policy_parameters
                .maximum_velocity_interval_width_ulps,
        };
        let lowered = LoweredFocAxisConfiguration {
            instance: 0,
            parameters,
            rotor,
            rotation_precision: RotationPrecision {
                maximum_component_width_ulps: 12_000_000,
                maximum_norm_error_ulps: 12_000_000,
            },
            current,
            pwm_dead_time_cycles: dead_time_cycles,
            adc_channel0: FocAdcFrontendParameters {
                instance: 0,
                channel: FocCurrentChannel::Channel0,
                attenuation: FocAdcAttenuation::Db11,
                evidence: FactEvidence::Qualified,
            },
            adc_channel1: FocAdcFrontendParameters {
                instance: 0,
                channel: FocCurrentChannel::Channel1,
                attenuation: FocAdcAttenuation::Db11,
                evidence: FactEvidence::Qualified,
            },
            pwm_hardware,
            pwm_compare,
            servo_parameters,
            encoder_scale_parameters,
            encoder_policy_parameters,
            servo_grid,
            servo,
            encoder,
        };
        lowered.validate().unwrap();
        lowered
    }

    fn input(period: DeviceCycle, command_id: u32) -> FocHardwareLoopInput {
        FocHardwareLoopInput {
            command: FocCurrentCommand {
                command_id,
                scheduled_at: period,
                configuration_digest: DIGEST,
                target: DqPoint {
                    d: Q30::ZERO,
                    q: point(1, 4),
                },
                voltage_feed_forward: DqPoint::default(),
            },
            raw_current_counts: [2_000, 2_000],
            raw_rotor_count: 0,
        }
    }

    fn servo_lowered_fixture() -> LoweredFocAxisConfiguration {
        servo_lowered_fixture_for(0)
    }

    fn servo_lowered_fixture_for(instance: u16) -> LoweredFocAxisConfiguration {
        let mut lowered = lowered_fixture();
        let scale = ServoEncoderScale::new(1, 1, 4_096, 1).unwrap();
        lowered.encoder_scale_parameters.scale = scale;
        lowered.encoder.scale = scale;
        lowered.servo_parameters.position_proportional_gain = Q30::HALF;
        lowered.servo.position_proportional_gain = Q30::HALF;
        lowered.pwm_hardware.maximum_quantization_error_ulps = u32::MAX;
        lowered.pwm_compare = PwmCompareContract::new(
            DIGEST,
            lowered.pwm_compare.device_cycle_hz(),
            lowered.pwm_compare.pwm_period_device_cycles(),
            lowered.pwm_compare.counter_clock_hz(),
            lowered.pwm_compare.timer_peak_ticks(),
            lowered.pwm_compare.minimum_active_ticks(),
            u32::MAX,
        )
        .unwrap();
        lowered.instance = instance;
        lowered.adc_channel0.instance = instance;
        lowered.adc_channel1.instance = instance;
        lowered.pwm_hardware.instance = instance;
        lowered.servo_parameters.instance = instance;
        lowered.encoder_scale_parameters.instance = instance;
        lowered.encoder_policy_parameters.instance = instance;
        lowered.validate().unwrap();
        lowered
    }

    fn servo_lowered_bank_fixture() -> [LoweredFocAxisConfiguration; 2] {
        [servo_lowered_fixture_for(0), servo_lowered_fixture_for(1)]
    }

    fn servo_seed() -> ServoEncoderSeed {
        ServoEncoderSeed {
            observation: ServoEncoderObservation {
                configuration_digest: DIGEST,
                raw_count: 0,
                sampled_at: DeviceCycle(0),
                available_at: DeviceCycle(0),
            },
            turn_index: 0,
        }
    }

    fn servo_input(
        loop_owner: &ConfiguredServoFocHardwareLoop,
        current_index: u64,
    ) -> FocServoHardwareLoopInput {
        let at = loop_owner.controller().period_started_at();
        let velocity_due = current_index.is_multiple_of(20);
        let position_due = current_index.is_multiple_of(200);
        let encoder_observation = velocity_due.then_some(ServoEncoderObservation {
            configuration_digest: DIGEST,
            raw_count: 0,
            sampled_at: DeviceCycle(80_000 + current_index / 20 * 80_000),
            available_at: at,
        });
        let setpoint = position_due.then_some(ServoSetpoint {
            command_id: u32::try_from(current_index / 200 + 1).unwrap(),
            scheduled_at: at,
            configuration_digest: DIGEST,
            position: ServoPosition::from_bits(1 << 28),
            velocity_feed_forward: Q30::ZERO,
            quadrature_current_feed_forward: Q30::ZERO,
        });
        FocServoHardwareLoopInput {
            setpoint,
            encoder_observation,
            raw_current_counts: [2_000, 2_000],
            raw_rotor_count: 0,
        }
    }

    fn servo_bank_input<const AXES: usize>(
        bank: &ConfiguredServoFocHardwareBank<AXES>,
        axis: usize,
        current_index: u64,
    ) -> FocServoHardwareLoopInput {
        let controller = bank.controller().axis(axis).unwrap();
        let at = controller.period_started_at();
        let velocity_due = current_index.is_multiple_of(20);
        let position_due = current_index.is_multiple_of(200);
        let encoder_observation = velocity_due.then_some(ServoEncoderObservation {
            configuration_digest: DIGEST,
            raw_count: 0,
            sampled_at: DeviceCycle(80_000 + current_index / 20 * 80_000),
            available_at: at,
        });
        let target = if axis.is_multiple_of(2) {
            1_i64 << 28
        } else {
            -(1_i64 << 27)
        };
        let setpoint = position_due.then_some(ServoSetpoint {
            command_id: u32::try_from(current_index / 200 + 1).unwrap(),
            scheduled_at: at,
            configuration_digest: DIGEST,
            position: ServoPosition::from_bits(target),
            velocity_feed_forward: Q30::ZERO,
            quadrature_current_feed_forward: Q30::ZERO,
        });
        FocServoHardwareLoopInput {
            setpoint,
            encoder_observation,
            raw_current_counts: [2_000, 2_000],
            raw_rotor_count: 0,
        }
    }

    fn two_axis_cached_fixture() -> (
        CachedServoConfiguration<2>,
        alumina_motion::CachedServoAdmissionProfile<2>,
        JobDescriptor,
        ExecutionBlock,
        ExecutionBlock,
    ) {
        let lowered = servo_lowered_fixture();
        let axis = ServoSetpointAxisAdmissionProfile::from_foc_axis(
            lowered.servo_foc_axis_profile().unwrap(),
        )
        .unwrap();
        let configuration = CachedServoConfiguration::from_axes([axis; 2]).unwrap();
        let admission = cached_servo_admission_profile(
            [axis; 2],
            CachedServoStreamPolicy {
                maximum_block_ticks: 800_000,
                maximum_segment_ticks: 800_000,
                maximum_update_count: 1,
            },
        )
        .unwrap();
        let positive = ServoFiniteDifferenceAxis {
            position: FiniteDifferenceAxis {
                initial_position: 1 << 28,
                first_difference: 0,
                second_difference: 0,
                third_difference: 0,
            },
            velocity_feed_forward: ServoQ30FiniteDifferenceAxis::default(),
            quadrature_current_feed_forward: ServoQ30FiniteDifferenceAxis::default(),
        };
        let negative = ServoFiniteDifferenceAxis {
            position: FiniteDifferenceAxis {
                initial_position: -(1 << 27),
                ..positive.position
            },
            ..positive
        };
        let first_segment = ServoFiniteDifferenceSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(800_000),
            update_period_ticks: 800_000,
            update_count: 1,
            axes: [positive, negative],
            flags: 0,
        };
        let second_segment = ServoFiniteDifferenceSegment {
            start_tick: StreamTick(800_000),
            end_tick: StreamTick(1_600_000),
            ..first_segment
        };
        let stream_id = StreamId::new([0x91; 16]).unwrap();
        let capability_digest = Digest([0x92; 32]);
        let first = ExecutionBlock::encode_servo_finite_difference(
            stream_id,
            capability_digest,
            DIGEST,
            0,
            Digest::ZERO,
            &[first_segment],
        )
        .unwrap();
        let second = ExecutionBlock::encode_servo_finite_difference(
            stream_id,
            capability_digest,
            DIGEST,
            1,
            first.header().block_digest,
            &[second_segment],
        )
        .unwrap();
        let maximum_position_delta = admission
            .limits
            .segment
            .maximum_position_delta_bits
            .iter()
            .copied()
            .max()
            .unwrap();
        let descriptor = JobDescriptor {
            prepare_id: 72,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId::from_sha256(Digest([0x93; 32])),
                    byte_len: 1_024,
                },
                manifest: ContentId::from_sha256(Digest([0x95; 32])),
            },
            stream_id,
            capability_digest,
            config_digest: DIGEST,
            axis_count: 2,
            execution_kind: ExecutionKind::ServoFiniteDifference,
            maximum_dense_updates: 1,
            dense_update_period_ticks: 800_000,
            block_count: 2,
            first_tick: StreamTick(0),
            initial_position: [1 << 28, -(1 << 27), 0, 0, 0, 0, 0, 0],
            limits: BlockValidationLimits {
                maximum_block_ticks: admission.limits.maximum_block_ticks,
                segment: ValidationLimits {
                    maximum_segment_ticks: admission.limits.segment.maximum_segment_ticks,
                    maximum_steps_per_segment: maximum_position_delta,
                },
            },
        };
        (configuration, admission, descriptor, first, second)
    }

    #[test]
    fn configuration_derived_complete_axis_replays_every_nested_loop_and_commit() {
        let lowered = servo_lowered_fixture();
        let mut first = ConfiguredServoFocHardwareLoop::from_lowered(
            lowered,
            0x55aa,
            servo_seed(),
            DeviceCycle(80_000),
        )
        .unwrap();
        let mut replay = ConfiguredServoFocHardwareLoop::from_lowered(
            lowered,
            0x55aa,
            servo_seed(),
            DeviceCycle(80_000),
        )
        .unwrap();
        let mut nonneutral_images = 0_u32;

        for current_index in 0_u64..=400 {
            let input = servo_input(&first, current_index);
            let update = first.step(input).unwrap();
            let replayed = replay.step(input).unwrap();
            assert_eq!(update, replayed);
            assert_eq!(update.prepared.cascade.current_index, current_index);
            assert_eq!(
                update.prepared.current_command.command_id,
                u32::try_from(current_index + 1).unwrap()
            );
            assert_eq!(
                update.latch.period_sequence,
                current_index.checked_add(1).unwrap()
            );
            assert_eq!(
                update.prepared.encoder.is_some(),
                current_index.is_multiple_of(20)
            );
            assert_eq!(
                update.prepared.cascade.position_updated,
                current_index.is_multiple_of(200)
            );
            if update
                .prepared
                .staged_image
                .comparisons()
                .iter()
                .any(|comparison| comparison.compare_ticks() != 1_000)
            {
                nonneutral_images += 1;
            }
        }

        assert_eq!(first.controller().cascade().last_current_index(), Some(400));
        assert_eq!(first.controller().cascade().position_updates(), 3);
        assert_eq!(first.controller().cascade().velocity_updates(), 21);
        assert_eq!(first.controller().encoder().estimate_sequence(), 21);
        assert_eq!(first.controller().period_sequence(), 401);
        assert!(nonneutral_images > 0);
        assert_eq!(first, replay);
    }

    #[test]
    fn cached_two_block_stream_drives_the_complete_axis_for_401_periods() {
        struct Blocks {
            first: Option<ExecutionBlock>,
            second: Option<ExecutionBlock>,
        }

        impl WorkSource for Blocks {
            fn try_receive(&mut self) -> Option<ExecutionBlock> {
                self.first.take().or_else(|| self.second.take())
            }

            fn depth(&self) -> usize {
                usize::from(self.first.is_some()) + usize::from(self.second.is_some())
            }
        }

        let lowered = servo_lowered_fixture();
        let foc_profile = lowered.servo_foc_axis_profile().unwrap();
        let axis = ServoSetpointAxisAdmissionProfile::from_foc_axis(foc_profile).unwrap();
        let configuration = CachedServoConfiguration::from_axes([axis]).unwrap();
        let admission = cached_servo_admission_profile(
            [axis],
            CachedServoStreamPolicy {
                maximum_block_ticks: 800_000,
                maximum_segment_ticks: 800_000,
                maximum_update_count: 1,
            },
        )
        .unwrap();
        assert_eq!(admission.setpoints.update_period_ticks, 800_000);

        let stationary = ServoFiniteDifferenceAxis {
            position: FiniteDifferenceAxis {
                initial_position: 1 << 28,
                first_difference: 0,
                second_difference: 0,
                third_difference: 0,
            },
            velocity_feed_forward: ServoQ30FiniteDifferenceAxis::default(),
            quadrature_current_feed_forward: ServoQ30FiniteDifferenceAxis::default(),
        };
        let first_segment = ServoFiniteDifferenceSegment {
            start_tick: StreamTick(0),
            end_tick: StreamTick(800_000),
            update_period_ticks: 800_000,
            update_count: 1,
            axes: [stationary],
            flags: 0,
        };
        let second_segment = ServoFiniteDifferenceSegment {
            start_tick: StreamTick(800_000),
            end_tick: StreamTick(1_600_000),
            ..first_segment
        };
        let stream_id = StreamId::new([0x81; 16]).unwrap();
        let capability_digest = Digest([0x82; 32]);
        let first = ExecutionBlock::encode_servo_finite_difference(
            stream_id,
            capability_digest,
            DIGEST,
            0,
            Digest::ZERO,
            &[first_segment],
        )
        .unwrap();
        let second = ExecutionBlock::encode_servo_finite_difference(
            stream_id,
            capability_digest,
            DIGEST,
            1,
            first.header().block_digest,
            &[second_segment],
        )
        .unwrap();
        let descriptor = JobDescriptor {
            prepare_id: 71,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId::from_sha256(Digest([0x83; 32])),
                    byte_len: 1_024,
                },
                manifest: ContentId::from_sha256(Digest([0x84; 32])),
            },
            stream_id,
            capability_digest,
            config_digest: DIGEST,
            axis_count: 1,
            execution_kind: ExecutionKind::ServoFiniteDifference,
            maximum_dense_updates: 1,
            dense_update_period_ticks: 800_000,
            block_count: 2,
            first_tick: StreamTick(0),
            initial_position: [1 << 28, 0, 0, 0, 0, 0, 0, 0],
            limits: BlockValidationLimits {
                maximum_block_ticks: admission.limits.maximum_block_ticks,
                segment: ValidationLimits {
                    maximum_segment_ticks: admission.limits.segment.maximum_segment_ticks,
                    maximum_steps_per_segment: admission.limits.segment.maximum_position_delta_bits
                        [0],
                },
            },
        };
        let mut source = Blocks {
            first: Some(first),
            second: Some(second),
        };
        let mut job = RealtimeJob::<1>::prepare_servo(descriptor, admission.limits).unwrap();
        let first = match job.poll(&mut source).unwrap() {
            RealtimePoll::Block(block) => block,
            RealtimePoll::Empty | RealtimePoll::Outstanding => panic!("first block"),
        };
        let second = match job.poll(&mut source).unwrap() {
            RealtimePoll::Block(block) => block,
            RealtimePoll::Empty | RealtimePoll::Outstanding => panic!("second block"),
        };
        assert_eq!(
            configuration.bind_descriptor(descriptor).unwrap(),
            admission
        );
        let mut hardware = ScheduledServoFocHardwareLoop::from_lowered(
            lowered,
            0x55aa,
            servo_seed(),
            DeviceCycle(80_000),
        )
        .unwrap();
        let mut execution = ScheduledServoExecution::new();
        execution.configure(configuration, 4_000).unwrap();
        execution
            .prime(
                &mut hardware,
                descriptor,
                DeviceCycle(80_000),
                first,
                Some(second),
                DeviceCycle(79_999),
            )
            .unwrap();
        execution.start(DeviceCycle(80_000)).unwrap();
        let mut command_ids = [0_u32; 3];
        let mut command_count = 0_usize;

        for current_index in 0_u64..=400 {
            let mut input = servo_input(hardware.hardware(), current_index);
            input.setpoint = None;
            hardware.step(input).unwrap();
            let observed = hardware.hardware().controller().period_started_at();
            match execution.poll(&mut hardware, observed).unwrap() {
                ScheduledServoAction::StartSetpointsCommitted(committed)
                | ScheduledServoAction::SetpointsCommitted(committed) => {
                    command_ids[command_count] = committed.command_id;
                    command_count += 1;
                }
                ScheduledServoAction::BlockComplete(completed) => {
                    command_ids[command_count] = if completed.header().sequence == 0 {
                        2
                    } else {
                        3
                    };
                    command_count += 1;
                    job.acknowledge(completed).unwrap();
                }
                ScheduledServoAction::Future { .. } | ScheduledServoAction::WaitingForHardware => {}
                ScheduledServoAction::Idle
                | ScheduledServoAction::NeedBlock { .. }
                | ScheduledServoAction::JobComplete => panic!("unexpected scheduled action"),
            }
        }

        assert_eq!(command_ids, [1, 2, 3]);
        execution.request_finish().unwrap();
        let final_observed = hardware.hardware().controller().period_started_at();
        assert!(matches!(
            execution.poll(&mut hardware, final_observed),
            Ok(ScheduledServoAction::JobComplete)
        ));
        assert_eq!(hardware.hardware().controller().period_sequence(), 401);
        assert_eq!(
            hardware
                .hardware()
                .controller()
                .cascade()
                .position_updates(),
            3
        );
        assert_eq!(job.status().state, RealtimeJobState::Complete);
    }

    #[test]
    fn canonical_dual_mks_document_drives_cached_admission_and_complete_bank() {
        let configuration = canonical_dual_mks_configuration();
        assert_eq!(
            configuration.identity().digest,
            CANONICAL_DUAL_MKS_CONFIGURATION_DIGEST
        );
        let cached = CachedServoConfiguration::<2>::from_configuration(&configuration).unwrap();
        assert_eq!(
            cached.configuration_digest(),
            CANONICAL_DUAL_MKS_CONFIGURATION_DIGEST
        );
        assert_eq!(cached.update_period_ticks(), 800_000);

        let seed = ServoEncoderSeed {
            observation: ServoEncoderObservation {
                configuration_digest: CANONICAL_DUAL_MKS_CONFIGURATION_DIGEST,
                raw_count: 0,
                sampled_at: DeviceCycle(0),
                available_at: DeviceCycle(0),
            },
            turn_index: 0,
        };
        let mut hardware = ConfiguredServoFocHardwareBank::from_configuration(
            &configuration,
            [0, 1],
            [0x6d_00, 0x6d_01],
            [seed; 2],
            DeviceCycle(80_000),
        )
        .unwrap();
        assert_eq!(
            hardware.controller().period_started_at(),
            DeviceCycle(80_000)
        );
        assert_eq!(hardware.controller().period_sequence(), 0);
        assert_eq!(
            hardware.commit_owner_state(),
            PwmCommitBankOwnerState::Ready
        );
        assert_eq!(hardware.next_commit_boundary(), DeviceCycle(84_000));
        assert_eq!(hardware.next_commit_sequence(), 1);
        assert_eq!(hardware.commit_owner_fault(), None);
        assert_eq!(hardware.safe_transaction_count(), 0);
        for axis in 0..2 {
            let controller = hardware.controller().axis(axis).unwrap();
            assert_eq!(
                controller.activation_id(),
                0x6d_00 + u64::try_from(axis).unwrap()
            );
            assert_eq!(
                controller.profile().parameters.configuration_digest,
                CANONICAL_DUAL_MKS_CONFIGURATION_DIGEST
            );
            assert_eq!(controller.period_started_at(), DeviceCycle(80_000));
        }

        let inputs = core::array::from_fn(|_| FocServoHardwareLoopInput {
            setpoint: Some(ServoSetpoint {
                command_id: 1,
                scheduled_at: DeviceCycle(80_000),
                configuration_digest: CANONICAL_DUAL_MKS_CONFIGURATION_DIGEST,
                position: ServoPosition::ZERO,
                velocity_feed_forward: Q30::ZERO,
                quadrature_current_feed_forward: Q30::ZERO,
            }),
            encoder_observation: Some(ServoEncoderObservation {
                configuration_digest: CANONICAL_DUAL_MKS_CONFIGURATION_DIGEST,
                raw_count: 0,
                sampled_at: DeviceCycle(80_000),
                available_at: DeviceCycle(80_000),
            }),
            raw_current_counts: [2_000, 2_000],
            raw_rotor_count: 0,
        });
        let updates = hardware.step(inputs).unwrap();
        assert_eq!(updates[0].prepared.at, DeviceCycle(80_000));
        assert_eq!(updates[0].prepared.at, updates[1].prepared.at);
        assert_eq!(updates[0].latch.observed_at, DeviceCycle(84_000));
        assert_eq!(updates[0].latch.observed_at, updates[1].latch.observed_at);
        for axis in 0..2 {
            let controller = hardware.controller().axis(axis).unwrap();
            assert_eq!(controller.period_started_at(), DeviceCycle(84_000));
            assert_eq!(controller.period_sequence(), 1);
        }
        assert_eq!(
            hardware.commit_owner_state(),
            PwmCommitBankOwnerState::Ready
        );
        assert_eq!(hardware.next_commit_boundary(), DeviceCycle(88_000));
        assert_eq!(hardware.next_commit_sequence(), 2);
        assert_eq!(hardware.commit_owner_fault(), None);
        assert_eq!(hardware.safe_transaction_count(), 0);
    }

    #[test]
    fn complete_bank_activation_requires_every_exact_initial_latch() {
        let lowered = servo_lowered_bank_fixture();
        let profiles = lowered.map(|axis| axis.servo_foc_axis_profile().unwrap());
        let prepared = ServoFocBank::<2>::prepare_activation(
            profiles,
            [0x55ca, 0x55cb],
            [servo_seed(); 2],
            DeviceCycle(80_000),
        )
        .unwrap();
        let images = prepared.initial_images();
        let mut missing =
            PwmCommitBankBarrier::<2>::new(DIGEST, 4_000, DeviceCycle(80_000), 0).unwrap();
        missing.stage(images).unwrap();
        missing
            .record_latch(
                0,
                PowerStageCommit {
                    token: images[0].token(),
                    scheduled_at: images[0].scheduled_at(),
                    observed_at: DeviceCycle(80_000),
                },
            )
            .unwrap();
        assert_eq!(
            missing.finish_boundary(DeviceCycle(80_000)),
            Err(PwmCommitBarrierError::MissingObservation { axis: 1 })
        );
        assert_eq!(missing.staged_images(), None);

        let prepared = ServoFocBank::<2>::prepare_activation(
            profiles,
            [0x55da, 0x55db],
            [servo_seed(); 2],
            DeviceCycle(80_000),
        )
        .unwrap();
        assert_eq!(
            ConfiguredServoFocHardwareBank::activate_prepared(
                prepared,
                [DeviceCycle(80_000), DeviceCycle(80_001)],
            ),
            Err(FocHardwareLoopError::CommitBarrier(
                PwmCommitBarrierError::Observation { axis: 1 }
            ))
        );

        let prepared = ServoFocBank::<2>::prepare_activation(
            profiles,
            [0x55ea, 0x55eb],
            [servo_seed(); 2],
            DeviceCycle(80_000),
        )
        .unwrap();
        let images = prepared.initial_images();
        let mut duplicate =
            PwmCommitBankBarrier::<2>::new(DIGEST, 4_000, DeviceCycle(80_000), 0).unwrap();
        duplicate.stage(images).unwrap();
        let axis0 = PowerStageCommit {
            token: images[0].token(),
            scheduled_at: images[0].scheduled_at(),
            observed_at: DeviceCycle(80_000),
        };
        duplicate.record_latch(0, axis0).unwrap();
        assert_eq!(
            duplicate.record_latch(0, axis0),
            Err(PwmCommitBarrierError::DuplicateObservation { axis: 0 })
        );
        assert_eq!(duplicate.staged_images(), None);

        let prepared = ServoFocBank::<2>::prepare_activation(
            profiles,
            [0x55fa, 0x55fb],
            [servo_seed(); 2],
            DeviceCycle(80_000),
        )
        .unwrap();
        let images = prepared.initial_images();
        let mut substituted =
            PwmCommitBankBarrier::<2>::new(DIGEST, 4_000, DeviceCycle(80_000), 0).unwrap();
        substituted.stage(images).unwrap();
        assert_eq!(
            substituted.record_latch(
                1,
                PowerStageCommit {
                    token: images[1].token() + 1,
                    scheduled_at: images[1].scheduled_at(),
                    observed_at: DeviceCycle(80_000),
                },
            ),
            Err(PwmCommitBarrierError::Observation { axis: 1 })
        );
    }

    #[test]
    fn cached_two_axis_stream_commits_only_complete_foc_bank_boundaries() {
        struct Blocks {
            first: Option<ExecutionBlock>,
            second: Option<ExecutionBlock>,
        }

        impl WorkSource for Blocks {
            fn try_receive(&mut self) -> Option<ExecutionBlock> {
                self.first.take().or_else(|| self.second.take())
            }

            fn depth(&self) -> usize {
                usize::from(self.first.is_some()) + usize::from(self.second.is_some())
            }
        }

        let (configuration, admission, descriptor, first, second) = two_axis_cached_fixture();
        assert_eq!(
            configuration.bind_descriptor(descriptor).unwrap(),
            admission
        );
        let mut source = Blocks {
            first: Some(first),
            second: Some(second),
        };
        let mut job = RealtimeJob::<2>::prepare_servo(descriptor, admission.limits).unwrap();
        let first = match job.poll(&mut source).unwrap() {
            RealtimePoll::Block(block) => block,
            RealtimePoll::Empty | RealtimePoll::Outstanding => panic!("first block"),
        };
        let second = match job.poll(&mut source).unwrap() {
            RealtimePoll::Block(block) => block,
            RealtimePoll::Empty | RealtimePoll::Outstanding => panic!("second block"),
        };
        let mut hardware = ScheduledServoFocHardwareBank::from_lowered(
            servo_lowered_bank_fixture(),
            [0x55aa, 0x55ab],
            [servo_seed(); 2],
            DeviceCycle(80_000),
        )
        .unwrap();
        let mut execution = ScheduledServoExecution::new();
        execution.configure(configuration, 4_000).unwrap();
        execution
            .prime(
                &mut hardware,
                descriptor,
                DeviceCycle(80_000),
                first,
                Some(second),
                DeviceCycle(79_999),
            )
            .unwrap();
        execution.start(DeviceCycle(80_000)).unwrap();

        let mut command_ids = [0_u32; 3];
        let mut command_count = 0_usize;
        let mut simultaneous_position_updates = 0_u32;
        for current_index in 0_u64..=400 {
            let mut inputs = core::array::from_fn(|axis| {
                servo_bank_input(hardware.hardware(), axis, current_index)
            });
            for input in &mut inputs {
                input.setpoint = None;
            }
            let updates = hardware.step(inputs).unwrap();
            assert_eq!(updates[0].prepared.at, updates[1].prepared.at);
            assert_eq!(updates[0].latch.observed_at, updates[1].latch.observed_at);
            assert_eq!(
                updates[0].prepared.cascade.position_updated,
                updates[1].prepared.cascade.position_updated
            );
            if updates[0].prepared.cascade.position_updated {
                simultaneous_position_updates += 1;
                assert_eq!(
                    updates[0].prepared.cascade.setpoint_id,
                    updates[1].prepared.cascade.setpoint_id
                );
                assert!(updates[0].prepared.cascade.velocity_target.bits() > 0);
                assert!(updates[1].prepared.cascade.velocity_target.bits() < 0);
            }

            let observed = hardware.hardware().controller().period_started_at();
            match execution.poll(&mut hardware, observed).unwrap() {
                ScheduledServoAction::StartSetpointsCommitted(committed)
                | ScheduledServoAction::SetpointsCommitted(committed) => {
                    command_ids[command_count] = committed.command_id;
                    command_count += 1;
                }
                ScheduledServoAction::BlockComplete(completed) => {
                    command_ids[command_count] = if completed.header().sequence == 0 {
                        2
                    } else {
                        3
                    };
                    command_count += 1;
                    job.acknowledge(completed).unwrap();
                }
                ScheduledServoAction::Future { .. } | ScheduledServoAction::WaitingForHardware => {}
                ScheduledServoAction::Idle
                | ScheduledServoAction::NeedBlock { .. }
                | ScheduledServoAction::JobComplete => panic!("unexpected scheduled action"),
            }
        }

        assert_eq!(command_ids, [1, 2, 3]);
        assert_eq!(simultaneous_position_updates, 3);
        execution.request_finish().unwrap();
        let final_observed = hardware.hardware().controller().period_started_at();
        assert!(matches!(
            execution.poll(&mut hardware, final_observed),
            Ok(ScheduledServoAction::JobComplete)
        ));
        assert_eq!(job.status().state, RealtimeJobState::Complete);
        assert_eq!(
            hardware.hardware().next_commit_boundary(),
            DeviceCycle(1_688_000)
        );
        assert_eq!(hardware.hardware().next_commit_sequence(), 402);
        assert_eq!(hardware.hardware().commit_owner_fault(), None);
        assert_eq!(hardware.hardware().safe_transaction_count(), 0);
        for axis in 0..2 {
            let controller = hardware.hardware().controller().axis(axis).unwrap();
            assert_eq!(controller.period_sequence(), 401);
            assert_eq!(controller.cascade().position_updates(), 3);
            assert_eq!(controller.cascade().velocity_updates(), 21);
            assert_eq!(controller.encoder().estimate_sequence(), 21);
        }
    }

    #[test]
    fn complete_foc_bank_rejects_late_or_missing_axis_without_partial_advance() {
        assert_eq!(
            ConfiguredServoFocHardwareBank::from_lowered(
                [servo_lowered_fixture(); 2],
                [0x55a8, 0x55a9],
                [servo_seed(); 2],
                DeviceCycle(80_000),
            ),
            Err(FocHardwareLoopError::Bank(ServoFocBankError::Profile))
        );
        let mut late = ConfiguredServoFocHardwareBank::from_lowered(
            servo_lowered_bank_fixture(),
            [0x55aa, 0x55ab],
            [servo_seed(); 2],
            DeviceCycle(80_000),
        )
        .unwrap();
        let inputs = core::array::from_fn(|axis| servo_bank_input(&late, axis, 0));
        assert_eq!(
            late.step_with_timer_zero(inputs, [DeviceCycle(84_000), DeviceCycle(84_001)]),
            Err(FocHardwareLoopError::CommitBarrier(
                PwmCommitBarrierError::Observation { axis: 1 }
            ))
        );
        assert_eq!(
            late.commit_owner_fault(),
            Some(PwmCommitBankOwnerFault::Barrier(
                PwmCommitBarrierError::Observation { axis: 1 }
            ))
        );
        assert_eq!(
            late.commit_owner_state(),
            PwmCommitBankOwnerState::SafeFault
        );
        assert_eq!(late.safe_transaction_count(), 1);
        assert_eq!(late.next_commit_boundary(), DeviceCycle(84_000));
        assert_eq!(late.controller().period_started_at(), DeviceCycle(80_000));
        assert_eq!(late.controller().period_sequence(), 0);
        assert_eq!(
            late.controller().fault(),
            Some(ServoFocBankError::SafetyInvalidated)
        );
        for axis in 0..2 {
            let controller = late.controller().axis(axis).unwrap();
            assert_eq!(controller.period_started_at(), DeviceCycle(80_000));
            assert_eq!(controller.period_sequence(), 0);
            assert_eq!(controller.encoder().estimate_sequence(), 0);
            assert_eq!(controller.cascade().last_current_index(), None);
        }

        let mut missing = ConfiguredServoFocHardwareBank::from_lowered(
            servo_lowered_bank_fixture(),
            [0x55ba, 0x55bb],
            [servo_seed(); 2],
            DeviceCycle(80_000),
        )
        .unwrap();
        let mut inputs = core::array::from_fn(|axis| servo_bank_input(&missing, axis, 0));
        inputs[1].encoder_observation = None;
        assert_eq!(
            missing.step(inputs),
            Err(FocHardwareLoopError::Bank(ServoFocBankError::Axis {
                axis: 1,
                error: ServoFocAxisError::EncoderPresence {
                    required: true,
                    received: false,
                },
            }))
        );
        assert_eq!(
            missing.controller().axis(0).unwrap().period_started_at(),
            DeviceCycle(80_000)
        );
        assert_eq!(
            missing
                .controller()
                .axis(0)
                .unwrap()
                .encoder()
                .estimate_sequence(),
            0
        );
        assert_eq!(
            missing.commit_owner_state(),
            PwmCommitBankOwnerState::SafeFault
        );
        assert_eq!(
            missing.commit_owner_fault(),
            Some(PwmCommitBankOwnerFault::ExternalSafety)
        );
        assert_eq!(missing.safe_transaction_count(), 1);
        assert_eq!(
            missing.controller().fault(),
            Some(ServoFocBankError::Axis {
                axis: 1,
                error: ServoFocAxisError::EncoderPresence {
                    required: true,
                    received: false,
                },
            })
        );
    }

    #[test]
    fn safe_invalidation_clears_two_axis_cached_mailbox_before_job_fault() {
        struct Blocks {
            first: Option<ExecutionBlock>,
            second: Option<ExecutionBlock>,
        }

        impl WorkSource for Blocks {
            fn try_receive(&mut self) -> Option<ExecutionBlock> {
                self.first.take().or_else(|| self.second.take())
            }

            fn depth(&self) -> usize {
                usize::from(self.first.is_some()) + usize::from(self.second.is_some())
            }
        }

        let (configuration, admission, descriptor, first, second) = two_axis_cached_fixture();
        let mut source = Blocks {
            first: Some(first),
            second: Some(second),
        };
        let mut job = RealtimeJob::<2>::prepare_servo(descriptor, admission.limits).unwrap();
        let first = match job.poll(&mut source).unwrap() {
            RealtimePoll::Block(block) => block,
            RealtimePoll::Empty | RealtimePoll::Outstanding => panic!("first block"),
        };
        let second = match job.poll(&mut source).unwrap() {
            RealtimePoll::Block(block) => block,
            RealtimePoll::Empty | RealtimePoll::Outstanding => panic!("second block"),
        };
        let mut hardware = ScheduledServoFocHardwareBank::from_lowered(
            servo_lowered_bank_fixture(),
            [0x55ca, 0x55cb],
            [servo_seed(); 2],
            DeviceCycle(80_000),
        )
        .unwrap();
        let mut execution = ScheduledServoExecution::new();
        execution.configure(configuration, 4_000).unwrap();
        execution
            .prime(
                &mut hardware,
                descriptor,
                DeviceCycle(80_000),
                first,
                Some(second),
                DeviceCycle(79_999),
            )
            .unwrap();
        execution.start(DeviceCycle(80_000)).unwrap();
        assert!(hardware.has_staged_setpoints());
        assert!(execution.started());

        hardware.invalidate_after_safe();
        execution.fault();
        assert!(!hardware.has_staged_setpoints());
        assert!(!execution.started());
        assert_eq!(
            hardware.hardware().controller().fault(),
            Some(ServoFocBankError::SafetyInvalidated)
        );
        assert_eq!(
            hardware.hardware().commit_owner_fault(),
            Some(PwmCommitBankOwnerFault::ExternalSafety)
        );
        assert_eq!(
            hardware.hardware().commit_owner_state(),
            PwmCommitBankOwnerState::SafeFault
        );
        assert_eq!(hardware.hardware().safe_transaction_count(), 1);
        assert_eq!(hardware.take_servo_setpoint_commit().unwrap(), None);
        let mut inputs =
            core::array::from_fn(|axis| servo_bank_input(hardware.hardware(), axis, 0));
        for input in &mut inputs {
            input.setpoint = None;
        }
        assert_eq!(hardware.step(inputs), Err(FocHardwareLoopError::Faulted));
    }

    #[test]
    fn complete_axis_commit_and_encoder_schedule_fail_without_partial_advance() {
        let lowered = servo_lowered_fixture();
        let mut late_commit = ConfiguredServoFocHardwareLoop::from_lowered(
            lowered,
            0x55aa,
            servo_seed(),
            DeviceCycle(80_000),
        )
        .unwrap();
        let input = servo_input(&late_commit, 0);
        assert_eq!(
            late_commit.step_with_timer_zero(input, DeviceCycle(84_001)),
            Err(FocHardwareLoopError::Axis(
                ServoFocAxisError::PowerStageCommit
            ))
        );
        assert_eq!(
            late_commit.controller().period_started_at(),
            DeviceCycle(80_000)
        );
        assert_eq!(late_commit.controller().encoder().estimate_sequence(), 0);
        assert_eq!(
            late_commit.controller().cascade().last_current_index(),
            None
        );
        assert_eq!(
            late_commit.step(input),
            Err(FocHardwareLoopError::Axis(ServoFocAxisError::FaultLatched))
        );

        let mut missing_encoder = ConfiguredServoFocHardwareLoop::from_lowered(
            lowered,
            0x55ab,
            servo_seed(),
            DeviceCycle(80_000),
        )
        .unwrap();
        let mut input = servo_input(&missing_encoder, 0);
        input.encoder_observation = None;
        assert_eq!(
            missing_encoder.step(input),
            Err(FocHardwareLoopError::Axis(
                ServoFocAxisError::EncoderPresence {
                    required: true,
                    received: false,
                }
            ))
        );
        assert_eq!(
            missing_encoder.controller().encoder().estimate_sequence(),
            0
        );
        assert_eq!(
            missing_encoder.controller().cascade().last_current_index(),
            None
        );
    }

    #[test]
    fn configured_integer_loop_replays_current_angle_control_and_compare_images() {
        let lowered = lowered_fixture();
        let start = DeviceCycle(8_000);
        let mut first = ConfiguredFocHardwareLoop::from_lowered(lowered, start).unwrap();
        let mut replay = ConfiguredFocHardwareLoop::from_lowered(lowered, start).unwrap();
        let mut observed_maximum_error = 0;

        for command_id in 1..=8 {
            let command = input(first.period_started_at(), command_id);
            let sample = first.step(command).unwrap();
            let replayed = replay.step(command).unwrap();
            assert_eq!(sample, replayed);
            assert_eq!(sample.active_image.token(), command_id);
            assert_eq!(sample.committed.image.token(), command_id + 1);
            assert_eq!(
                sample.committed.observed_at.0,
                sample.current.synchronization.period_started_at.0 + 4_000
            );
            assert_eq!(sample.current.synchronization.duty_token, command_id);
            sample
                .current
                .validate_for(&lowered.parameters, lowered.current)
                .unwrap();
            sample
                .rotor
                .validate_for(&lowered.parameters, lowered.rotation_precision)
                .unwrap();
            sample
                .committed
                .image
                .validate_for(
                    lowered.pwm_compare,
                    &lowered.parameters,
                    lowered.current.snapshot().synchronization,
                )
                .unwrap();
            observed_maximum_error = observed_maximum_error.max(
                sample
                    .committed
                    .image
                    .comparisons()
                    .map(|value| value.maximum_error_ulps())
                    .into_iter()
                    .max()
                    .unwrap(),
            );
        }
        assert!(observed_maximum_error > 1_000_000);
        assert!(observed_maximum_error <= ACCEPTED_COMPARE_ERROR_ULPS);
        assert!(!first.is_faulted());
        assert_eq!(first.period_started_at(), DeviceCycle(40_000));
    }

    #[test]
    fn configured_precision_policy_rejects_an_overwide_control_result() {
        let lowered = lowered_with_lattices(80_000_000, 4_000, 80_000_000, 2_000, 8, 8, 1_000_000);
        let start = DeviceCycle(8_000);
        let mut simulator = ConfiguredFocHardwareLoop::from_lowered(lowered, start).unwrap();
        simulator.step(input(start, 1)).unwrap();
        let next = simulator.period_started_at();
        assert_eq!(
            simulator.step(input(next, 2)),
            Err(FocHardwareLoopError::Compare(PwmCompareError::Quantization))
        );
        assert!(simulator.is_faulted());
        assert_eq!(
            simulator.step(input(next, 2)),
            Err(FocHardwareLoopError::Faulted)
        );
    }

    #[test]
    fn timer_zero_and_raw_sample_faults_latch_before_reuse() {
        let lowered = lowered_fixture();
        let start = DeviceCycle(8_000);
        let mut boundary = ConfiguredFocHardwareLoop::from_lowered(lowered, start).unwrap();
        assert_eq!(
            boundary.step_with_timer_zero(input(start, 1), DeviceCycle(12_001)),
            Err(FocHardwareLoopError::Latch(PwmCompareLatchError::Boundary))
        );
        assert!(boundary.is_faulted());
        assert_eq!(
            boundary.step(input(start, 1)),
            Err(FocHardwareLoopError::Faulted)
        );

        let mut raw = ConfiguredFocHardwareLoop::from_lowered(lowered, start).unwrap();
        let mut invalid = input(start, 1);
        invalid.raw_current_counts[0] = 0;
        assert_eq!(
            raw.step(invalid),
            Err(FocHardwareLoopError::Foc(FocError::CurrentRawSample))
        );
        assert!(raw.is_faulted());
        assert_eq!(raw.period_started_at(), start);
    }

    #[test]
    fn stale_bundle_rate_and_fractional_edge_cycle_are_rejected_exactly() {
        let mut stale = lowered_fixture();
        stale.pwm_hardware.timer_peak_ticks = 1_999;
        assert_eq!(
            ConfiguredFocHardwareLoop::from_lowered(stale, DeviceCycle(0)),
            Err(FocHardwareLoopError::Configuration(
                ConfigurationError::FocHardware
            ))
        );

        let mut slower = lowered_fixture();
        slower.parameters.timing.current_loop_hz = 10_000;
        slower.servo_grid = ServoLoopGrid::new(
            DeviceCycle(0),
            slower.encoder_policy_parameters.device_cycle_hz,
            slower.parameters.timing,
        )
        .unwrap();
        slower.encoder_policy_parameters.sample_period_cycles =
            slower.servo_grid.velocity_period_cycles();
        slower.encoder.sample_period_cycles = slower.servo_grid.velocity_period_cycles();
        slower.validate().unwrap();
        assert_eq!(
            ConfiguredFocHardwareLoop::from_lowered(slower, DeviceCycle(0)),
            Err(FocHardwareLoopError::Rate)
        );

        let fractional = lowered_with_lattices(100, 10, 80, 4, 1, 1, u32::MAX);
        let mut simulator =
            ConfiguredFocHardwareLoop::from_lowered(fractional, DeviceCycle(100)).unwrap();
        assert_eq!(
            simulator.step(input(DeviceCycle(100), 1)),
            Err(FocHardwareLoopError::ClockDomain)
        );
        assert!(simulator.is_faulted());
    }
}
