//! Transactional composition of one configured encoder/servo/current/PWM axis.
//!
//! This module owns only portable deterministic state. A target must still
//! acquire truthful encoder/current timing evidence, stage the returned complete
//! PWM image, observe its physical timer-zero commit, and apply the qualified
//! shutdown transaction on any rejection.

use alumina_protocol::{DeviceCycle, Digest};

use crate::{
    AlphaBeta, CascadedServoController, CurrentSample, DqControlUpdate, DqCurrentController,
    DqInterval, DqPoint, FocCurrentCommand, FocError, FocParameterSnapshot, ModulationResult,
    PowerStageCommit, PwmAdcSampleStamp, PwmCommitBankCompletion, PwmCompareContract,
    PwmCompareError, PwmCompareImage, PwmCompareLatch, PwmCompareLatchError, PwmCompareLatchOwner,
    Q30Interval, RotationPrecision, RotorCalibration, RotorSample, ServoCascadeConfig,
    ServoCascadeError, ServoCascadeProfileError, ServoCascadeUpdate, ServoEncoderError,
    ServoEncoderEstimate, ServoEncoderEstimator, ServoEncoderObservation, ServoEncoderProfile,
    ServoEncoderProfileError, ServoEncoderSeed, ServoLoopGrid, ServoLoopGridError, ServoSetpoint,
    ValidatedTwoShuntCurrentCalibration, inverse_park, park, space_vector_modulate,
};

const INITIAL_DUTY_TOKEN: u32 = 1;
const INITIAL_CURRENT_COMMAND_ID: u32 = 1;

/// Host-verified upper bound for one live complete-axis controller value.
pub const MAX_SERVO_FOC_AXIS_CONTROLLER_BYTES: usize = 2_048;
/// Host-verified upper bound for one neutral-image activation candidate.
pub const MAX_PREPARED_SERVO_FOC_AXIS_ACTIVATION_BYTES: usize = 2_048;
/// Host-verified upper bound for one uncommitted complete-axis transition.
pub const MAX_PREPARED_SERVO_FOC_AXIS_TRANSITION_BYTES: usize = 2_304;
/// Host-verified upper bound for the replayable calculation result alone.
pub const MAX_SERVO_FOC_AXIS_PREPARED_UPDATE_BYTES: usize = 1_024;
/// Maximum complete axes joined by one simultaneous portable FOC owner.
pub const MAX_SERVO_FOC_BANK_AXES: usize = 4;
/// Portable upper bound for one complete four-axis activation candidate.
pub const MAX_PREPARED_SERVO_FOC_BANK_ACTIVATION_BYTES: usize =
    MAX_SERVO_FOC_BANK_AXES * MAX_PREPARED_SERVO_FOC_AXIS_ACTIVATION_BYTES + 256;

/// Immutable, fully normalized inputs for one portable servo/FOC owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoFocAxisProfile {
    pub parameters: FocParameterSnapshot,
    pub rotor: RotorCalibration,
    pub rotation_precision: RotationPrecision,
    pub current: ValidatedTwoShuntCurrentCalibration,
    pub pwm_compare: PwmCompareContract,
    pub servo: ServoCascadeConfig,
    pub encoder: ServoEncoderProfile,
}

impl ServoFocAxisProfile {
    /// Replays every portable identity, clock, calibration, and loop-grid seam.
    ///
    /// The returned grid is rebased onto the boot-local activation boundary;
    /// its periods remain exactly those selected by immutable configuration.
    pub fn validate_at(
        self,
        activation_boundary: DeviceCycle,
    ) -> Result<ServoLoopGrid, ServoFocAxisError> {
        self.parameters.validate()?;
        let current = self.current.snapshot();
        current.validate()?;
        self.pwm_compare
            .validate_for(&self.parameters, current.synchronization)?;
        self.encoder
            .validate()
            .map_err(ServoFocAxisError::EncoderProfile)?;

        let grid = ServoLoopGrid::new(
            activation_boundary,
            self.encoder.device_cycle_hz,
            self.parameters.timing,
        )
        .map_err(ServoFocAxisError::Grid)?;
        self.servo
            .validate_for(grid, &self.parameters)
            .map_err(ServoFocAxisError::ServoProfile)?;

        self.rotor.observe(
            self.rotor.count_at_reference,
            activation_boundary,
            self.rotation_precision,
        )?;

        let digest = self.parameters.configuration_digest;
        if digest.is_zero()
            || self.rotor.configuration_digest != digest
            || current.configuration_digest != digest
            || self.pwm_compare.configuration_digest() != digest
            || self.servo.configuration_digest != digest
            || self.encoder.configuration_digest != digest
            || self.rotor.pole_pairs != self.parameters.pole_pairs
            || current.maximum_phase_current != self.parameters.maximum_phase_current
            || self.parameters.timing.current_loop_hz != self.parameters.timing.pwm_hz
            || grid.current_period_cycles() != u64::from(current.synchronization.pwm_period_cycles)
            || self.encoder.device_cycle_hz != current.synchronization.device_cycle_hz
            || self.encoder.sample_period_cycles != grid.velocity_period_cycles()
            || self.encoder.counts_per_mechanical_turn != self.rotor.counts_per_mechanical_turn
            || self.encoder.count_at_reference != self.rotor.count_at_reference
            || self.encoder.direction != self.rotor.direction
            || self.encoder.maximum_count_error != self.rotor.maximum_count_error
            || self.encoder.maximum_admitted_velocity != self.servo.maximum_velocity
            || self.servo.maximum_sample_age_cycles
                < self.encoder.maximum_observation_latency_cycles
        {
            return Err(ServoFocAxisError::Profile);
        }
        Ok(grid)
    }

    /// Complete immutable configuration identity shared by every layer.
    pub const fn configuration_digest(self) -> Digest {
        self.parameters.configuration_digest
    }
}

/// One current-period input with independently produced physical observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoFocAxisPeriodInput {
    /// Exact start of the active PWM/current period.
    pub at: DeviceCycle,
    /// Present exactly on position-loop boundaries.
    pub setpoint: Option<ServoSetpoint>,
    /// Present exactly on velocity-loop boundaries.
    pub encoder_observation: Option<ServoEncoderObservation>,
    /// Two raw codes covered by `current_stamp`.
    pub raw_current_counts: [u16; 2],
    /// Truthful PWM-correlated acquisition witness for the active image.
    pub current_stamp: PwmAdcSampleStamp,
    /// Raw absolute count used for electrical-angle observation this period.
    pub raw_rotor_count: u32,
}

/// Complete result calculated before the next PWM image is physically committed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoFocAxisPreparedUpdate {
    pub activation_id: u64,
    pub at: DeviceCycle,
    pub period_sequence: u32,
    pub active_image: PwmCompareImage,
    pub encoder: Option<ServoEncoderEstimate>,
    pub cascade: ServoCascadeUpdate,
    pub current_command: FocCurrentCommand,
    pub current: CurrentSample,
    pub rotor: RotorSample,
    pub measured_interval: DqInterval,
    pub measured: DqPoint,
    pub current_control: DqControlUpdate,
    pub modulation: ModulationResult,
    pub staged_image: PwmCompareImage,
}

/// Opaque candidate state awaiting one exact physical PWM commit.
#[derive(Debug, Eq, PartialEq)]
pub struct PreparedServoFocAxisTransition {
    update: ServoFocAxisPreparedUpdate,
    base_period_started_at: DeviceCycle,
    base_period_sequence: u32,
    base_next_duty_token: u32,
    base_next_current_command_id: u32,
    encoder: ServoEncoderEstimator,
    cascade: CascadedServoController,
    current_controller: DqCurrentController,
    compare_owner: PwmCompareLatchOwner,
    next_duty_token: u32,
    next_current_command_id: u32,
}

impl PreparedServoFocAxisTransition {
    /// Pure calculation result and complete image the target must stage.
    pub const fn update(&self) -> ServoFocAxisPreparedUpdate {
        self.update
    }

    /// Complete future image whose physical commit completes this transition.
    pub const fn staged_image(&self) -> PwmCompareImage {
        self.update.staged_image
    }
}

/// Opaque validated activation awaiting the initial neutral-image commit.
#[derive(Debug, Eq, PartialEq)]
pub struct PreparedServoFocAxisActivation {
    profile: ServoFocAxisProfile,
    activation_id: u64,
    encoder: ServoEncoderEstimator,
    cascade: CascadedServoController,
    current_controller: DqCurrentController,
    compare_owner: PwmCompareLatchOwner,
    first_boundary: DeviceCycle,
    initial_image: PwmCompareImage,
}

impl PreparedServoFocAxisActivation {
    /// Nonzero boot-local activation identity retained by the candidate.
    pub const fn activation_id(&self) -> u64 {
        self.activation_id
    }

    /// First exact timer-zero boundary selected for this activation.
    pub const fn first_boundary(&self) -> DeviceCycle {
        self.first_boundary
    }

    /// Complete neutral image the target must stage before activation.
    pub const fn initial_image(&self) -> PwmCompareImage {
        self.initial_image
    }
}

/// Opaque complete-axis bank awaiting one simultaneous neutral-image commit.
///
/// Every axis profile, seed, and boot-local identity is validated before this
/// value is returned. No live controller exists until [`ServoFocBank::activate`]
/// consumes the complete physical acknowledgement vector.
#[derive(Debug, Eq, PartialEq)]
pub struct PreparedServoFocBankActivation<const AXES: usize> {
    configuration_digest: Digest,
    first_boundary: DeviceCycle,
    pwm_period_cycles: u32,
    prepared: [Option<PreparedServoFocAxisActivation>; AXES],
}

impl<const AXES: usize> PreparedServoFocBankActivation<AXES> {
    /// Immutable configuration identity shared by every prepared axis.
    pub const fn configuration_digest(&self) -> Digest {
        self.configuration_digest
    }

    /// Sole first physical timer-zero accepted by this activation.
    pub const fn first_boundary(&self) -> DeviceCycle {
        self.first_boundary
    }

    /// Common exact PWM period in device cycles.
    pub const fn pwm_period_cycles(&self) -> u32 {
        self.pwm_period_cycles
    }

    /// One initial neutral image in physical-axis order.
    pub fn initial_image(&self, axis: usize) -> Option<PwmCompareImage> {
        self.prepared
            .get(axis)
            .and_then(Option::as_ref)
            .map(PreparedServoFocAxisActivation::initial_image)
    }

    /// Complete initial neutral-image vector in physical-axis order.
    pub fn initial_images(&self) -> [PwmCompareImage; AXES] {
        core::array::from_fn(|axis| {
            self.prepared[axis]
                .as_ref()
                .expect("complete prepared FOC bank activation")
                .initial_image()
        })
    }
}

/// One atomically accepted complete-axis transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoFocAxisUpdate {
    pub prepared: ServoFocAxisPreparedUpdate,
    pub power_stage_commit: PowerStageCommit,
    pub latch: PwmCompareLatch,
}

/// One fixed-capacity multi-axis transition awaiting physical PWM commits.
///
/// Every contained axis candidate is calculated before the target stages any
/// complete-image set. The bank consumes this value only after the target has
/// observed the corresponding timer-zero commits for all axes.
#[derive(Debug, Eq, PartialEq)]
pub struct PreparedServoFocBankTransition<const AXES: usize> {
    base_period_started_at: DeviceCycle,
    base_period_sequence: u32,
    prepared: [Option<PreparedServoFocAxisTransition>; AXES],
}

impl<const AXES: usize> PreparedServoFocBankTransition<AXES> {
    /// Exact common current-loop boundary represented by every candidate.
    pub const fn period_started_at(&self) -> DeviceCycle {
        self.base_period_started_at
    }

    /// Pure axis result and complete future image, before physical acceptance.
    pub fn axis_update(&self, axis: usize) -> Option<ServoFocAxisPreparedUpdate> {
        self.prepared
            .get(axis)
            .and_then(Option::as_ref)
            .map(PreparedServoFocAxisTransition::update)
    }
}

/// Bank-level profile, schedule, axis, or physical-commit rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoFocBankError {
    /// A bank must contain between one and the fixed portable maximum axes.
    AxisCount,
    /// Axis identities, loop grids, or activation identities cannot form one bank.
    Profile,
    /// An input or prepared transition did not represent the common bank boundary.
    Schedule,
    /// One indexed complete-axis actor rejected before bank state installation.
    Axis {
        axis: usize,
        error: ServoFocAxisError,
    },
    /// The enclosing owner made every power stage safe and invalidated the bank.
    SafetyInvalidated,
    /// A prior bank rejection terminally closed all axes.
    FaultLatched,
}

/// Allocation-free simultaneous owner for a fixed set of complete FOC axes.
///
/// Preparation mutates no successful axis state. Commit first derives and
/// validates every complete next controller value, then replaces the full axis
/// array in one bank transaction. A commit failure retains the prior live
/// array; a preparation failure may retain its axis-local first cause but
/// advances no estimator, controller, or image prefix. Either latches the bank
/// closed so an enclosing target can perform its qualified all-stage shutdown
/// transaction.
#[derive(Debug, Eq, PartialEq)]
pub struct ServoFocBank<const AXES: usize> {
    axes: [ServoFocAxisController; AXES],
    configuration_digest: Digest,
    period_started_at: DeviceCycle,
    period_sequence: u32,
    fault: Option<ServoFocBankError>,
}

/// Allocation-free owner joining estimator, outer loops, current loop, and PWM.
#[derive(Debug, Eq, PartialEq)]
pub struct ServoFocAxisController {
    profile: ServoFocAxisProfile,
    activation_id: u64,
    encoder: ServoEncoderEstimator,
    cascade: CascadedServoController,
    current_controller: DqCurrentController,
    compare_owner: PwmCompareLatchOwner,
    period_started_at: DeviceCycle,
    period_sequence: u32,
    next_duty_token: u32,
    next_current_command_id: u32,
    fault: Option<ServoFocAxisError>,
}

impl ServoFocAxisController {
    /// Validates all immutable and boot-local state and returns the complete
    /// neutral image which must be staged before an actor can become active.
    pub fn prepare_activation(
        profile: ServoFocAxisProfile,
        activation_id: u64,
        seed: ServoEncoderSeed,
        first_boundary: DeviceCycle,
    ) -> Result<PreparedServoFocAxisActivation, ServoFocAxisError> {
        if activation_id == 0 {
            return Err(ServoFocAxisError::ActivationIdentity);
        }
        let grid = profile.validate_at(first_boundary)?;
        let first_sampled_at = seed
            .observation
            .sampled_at
            .0
            .checked_add(profile.encoder.sample_period_cycles)
            .ok_or(ServoFocAxisError::CounterOverflow)?;
        if seed.observation.available_at.0 > first_boundary.0
            || first_sampled_at > first_boundary.0
            || first_boundary.0 - first_sampled_at > profile.servo.maximum_sample_age_cycles
        {
            return Err(ServoFocAxisError::SeedSchedule);
        }
        let encoder = ServoEncoderEstimator::new(profile.encoder, seed)
            .map_err(ServoFocAxisError::Encoder)?;
        let cascade = CascadedServoController::new(grid, profile.servo, &profile.parameters)
            .map_err(ServoFocAxisError::ServoProfile)?;
        let current_controller = DqCurrentController::from_snapshot(&profile.parameters)?;

        let synchronization = profile.current.snapshot().synchronization;
        let mut compare_owner = PwmCompareLatchOwner::new(profile.pwm_compare, first_boundary)?;
        let neutral = space_vector_modulate(
            AlphaBeta {
                alpha: Q30Interval::ZERO,
                beta: Q30Interval::ZERO,
            },
            profile.parameters.maximum_phase_voltage,
        )?;
        let initial_image = profile.pwm_compare.lower(
            &profile.parameters,
            synchronization,
            INITIAL_DUTY_TOKEN,
            first_boundary,
            neutral.duties,
        )?;
        compare_owner.stage(initial_image, &profile.parameters, synchronization)?;
        Ok(PreparedServoFocAxisActivation {
            profile,
            activation_id,
            encoder,
            cascade,
            current_controller,
            compare_owner,
            first_boundary,
            initial_image,
        })
    }

    /// Creates the live owner only after the exact prepared neutral image has
    /// been acknowledged at its selected physical timer-zero boundary.
    pub fn activate(
        prepared: PreparedServoFocAxisActivation,
        initial_commit: PowerStageCommit,
    ) -> Result<Self, ServoFocAxisError> {
        validate_power_stage_commit(initial_commit, prepared.initial_image)?;
        let mut compare_owner = prepared.compare_owner;
        let initial_latch = compare_owner
            .observe_timer_zero(initial_commit.observed_at)?
            .ok_or(ServoFocAxisError::PowerStageCommit)?;
        if initial_latch.image != prepared.initial_image {
            return Err(ServoFocAxisError::PowerStageCommit);
        }
        let period_sequence = u32::try_from(initial_latch.period_sequence)
            .map_err(|_| ServoFocAxisError::CounterOverflow)?;

        Ok(Self {
            profile: prepared.profile,
            activation_id: prepared.activation_id,
            encoder: prepared.encoder,
            cascade: prepared.cascade,
            current_controller: prepared.current_controller,
            compare_owner,
            period_started_at: prepared.first_boundary,
            period_sequence,
            next_duty_token: INITIAL_DUTY_TOKEN + 1,
            next_current_command_id: INITIAL_CURRENT_COMMAND_ID,
            fault: None,
        })
    }

    /// Immutable complete profile retained by this actor.
    pub const fn profile(&self) -> ServoFocAxisProfile {
        self.profile
    }

    /// Nonzero boot-local activation identity joining prepared transitions.
    pub const fn activation_id(&self) -> u64 {
        self.activation_id
    }

    /// Exact start of the active PWM/current period.
    pub const fn period_started_at(&self) -> DeviceCycle {
        self.period_started_at
    }

    /// Active PWM-period sequence correlated with acquisition witnesses.
    pub const fn period_sequence(&self) -> u32 {
        self.period_sequence
    }

    /// Complete active PWM image, present after successful construction.
    pub const fn active_image(&self) -> Option<PwmCompareImage> {
        self.compare_owner.active_image()
    }

    /// Retained absolute-count estimator state.
    pub const fn encoder(&self) -> ServoEncoderEstimator {
        self.encoder
    }

    /// Retained cascaded position/velocity controller state.
    pub const fn cascade(&self) -> CascadedServoController {
        self.cascade
    }

    /// Exact first failure retained by the complete-axis owner.
    pub const fn fault(&self) -> Option<ServoFocAxisError> {
        self.fault
    }

    /// Calculates one complete next-period candidate without advancing live
    /// controller state or claiming a hardware write.
    ///
    /// Any rejection latches the complete-axis owner. Success returns an opaque
    /// transition which must be consumed by [`Self::commit`] after the target
    /// stages the exposed image and observes its exact timer-zero activation.
    pub fn prepare(
        &mut self,
        input: ServoFocAxisPeriodInput,
    ) -> Result<PreparedServoFocAxisTransition, ServoFocAxisError> {
        if self.fault.is_some() {
            return Err(ServoFocAxisError::FaultLatched);
        }
        match self.prepare_inner(input) {
            Ok(prepared) => Ok(prepared),
            Err(error) => {
                self.fault = Some(error);
                Err(error)
            }
        }
    }

    fn prepare_inner(
        &self,
        input: ServoFocAxisPeriodInput,
    ) -> Result<PreparedServoFocAxisTransition, ServoFocAxisError> {
        if input.at != self.period_started_at {
            return Err(ServoFocAxisError::Period {
                expected: self.period_started_at,
                received: input.at,
            });
        }
        let active_image = self
            .compare_owner
            .active_image()
            .ok_or(ServoFocAxisError::CurrentWitness)?;
        let stamp = input.current_stamp;
        if stamp.duty_token != active_image.token()
            || stamp.period_sequence != self.period_sequence
            || stamp.period_started_at != self.period_started_at
        {
            return Err(ServoFocAxisError::CurrentWitness);
        }

        let tick = self
            .cascade
            .grid()
            .tick(input.at)
            .map_err(ServoFocAxisError::Grid)?;
        if input.encoder_observation.is_some() != tick.velocity_due {
            return Err(ServoFocAxisError::EncoderPresence {
                required: tick.velocity_due,
                received: input.encoder_observation.is_some(),
            });
        }

        let mut next_encoder = self.encoder;
        let encoder = input
            .encoder_observation
            .map(|observation| next_encoder.observe(observation))
            .transpose()
            .map_err(ServoFocAxisError::Encoder)?;

        let mut next_cascade = self.cascade;
        let cascade = next_cascade
            .service(
                input.at,
                input.setpoint,
                encoder.map(|estimate| estimate.sample),
            )
            .map_err(ServoFocAxisError::Cascade)?;

        let parameters = self.profile.parameters;
        let synchronization = self.profile.current.snapshot().synchronization;
        active_image.validate_for(self.profile.pwm_compare, &parameters, synchronization)?;
        let current = self
            .profile
            .current
            .observe(input.raw_current_counts, input.current_stamp)?;
        current.validate_for(&parameters, self.profile.current)?;
        let rotor = self.profile.rotor.observe(
            input.raw_rotor_count,
            input.current_stamp.channel0_sampled_at,
            self.profile.rotation_precision,
        )?;
        rotor.validate_for(&parameters, self.profile.rotation_precision)?;
        let measured_interval = park(current.stationary()?, rotor.rotation)?;
        let measured = DqPoint {
            d: measured_interval.d.midpoint(),
            q: measured_interval.q.midpoint(),
        };

        let current_command = FocCurrentCommand {
            command_id: self.next_current_command_id,
            scheduled_at: input.at,
            configuration_digest: parameters.configuration_digest,
            target: cascade.current_target,
            voltage_feed_forward: DqPoint::default(),
        };
        current_command.validate(&parameters)?;
        let mut next_current_controller = self.current_controller;
        let current_control = next_current_controller.update(
            current_command.target,
            measured,
            current_command.voltage_feed_forward,
        )?;
        let stationary_voltage = inverse_park(
            DqInterval {
                d: Q30Interval::point(current_control.voltage.d),
                q: Q30Interval::point(current_control.voltage.q),
            },
            rotor.rotation,
        )?;
        let modulation =
            space_vector_modulate(stationary_voltage, parameters.maximum_phase_voltage)?;

        let scheduled_at = self.compare_owner.next_boundary();
        let expected_boundary = input
            .at
            .0
            .checked_add(u64::from(synchronization.pwm_period_cycles))
            .map(DeviceCycle)
            .ok_or(ServoFocAxisError::CounterOverflow)?;
        if scheduled_at != expected_boundary {
            return Err(ServoFocAxisError::CurrentWitness);
        }
        let staged_image = self.profile.pwm_compare.lower(
            &parameters,
            synchronization,
            self.next_duty_token,
            scheduled_at,
            modulation.duties,
        )?;
        let mut next_compare_owner = self.compare_owner;
        next_compare_owner.stage(staged_image, &parameters, synchronization)?;

        let next_duty_token = self
            .next_duty_token
            .checked_add(1)
            .ok_or(ServoFocAxisError::CounterOverflow)?;
        let next_current_command_id = self
            .next_current_command_id
            .checked_add(1)
            .ok_or(ServoFocAxisError::CounterOverflow)?;
        let update = ServoFocAxisPreparedUpdate {
            activation_id: self.activation_id,
            at: input.at,
            period_sequence: self.period_sequence,
            active_image,
            encoder,
            cascade,
            current_command,
            current,
            rotor,
            measured_interval,
            measured,
            current_control,
            modulation,
            staged_image,
        };
        Ok(PreparedServoFocAxisTransition {
            update,
            base_period_started_at: self.period_started_at,
            base_period_sequence: self.period_sequence,
            base_next_duty_token: self.next_duty_token,
            base_next_current_command_id: self.next_current_command_id,
            encoder: next_encoder,
            cascade: next_cascade,
            current_controller: next_current_controller,
            compare_owner: next_compare_owner,
            next_duty_token,
            next_current_command_id,
        })
    }

    /// Commits one prepared candidate only after an exact physical image
    /// acknowledgement. Any mismatch leaves prior controller state unchanged,
    /// latches the first cause, and requires the target's shutdown path.
    pub fn commit(
        &mut self,
        prepared: PreparedServoFocAxisTransition,
        power_stage_commit: PowerStageCommit,
    ) -> Result<ServoFocAxisUpdate, ServoFocAxisError> {
        if self.fault.is_some() {
            return Err(ServoFocAxisError::FaultLatched);
        }
        match self.committed_candidate(prepared, power_stage_commit) {
            Ok((next, update)) => {
                *self = next;
                Ok(update)
            }
            Err(error) => {
                self.fault = Some(error);
                Err(error)
            }
        }
    }

    fn committed_candidate(
        &self,
        prepared: PreparedServoFocAxisTransition,
        power_stage_commit: PowerStageCommit,
    ) -> Result<(Self, ServoFocAxisUpdate), ServoFocAxisError> {
        if prepared.update.activation_id != self.activation_id
            || prepared.base_period_started_at != self.period_started_at
            || prepared.base_period_sequence != self.period_sequence
            || prepared.base_next_duty_token != self.next_duty_token
            || prepared.base_next_current_command_id != self.next_current_command_id
        {
            return Err(ServoFocAxisError::PreparedState);
        }
        validate_power_stage_commit(power_stage_commit, prepared.update.staged_image)?;
        let mut next_compare_owner = prepared.compare_owner;
        let latch = next_compare_owner
            .observe_timer_zero(power_stage_commit.observed_at)?
            .ok_or(ServoFocAxisError::PowerStageCommit)?;
        if latch.image != prepared.update.staged_image
            || latch.period_sequence
                != u64::from(self.period_sequence)
                    .checked_add(1)
                    .ok_or(ServoFocAxisError::CounterOverflow)?
        {
            return Err(ServoFocAxisError::PowerStageCommit);
        }
        let period_sequence =
            u32::try_from(latch.period_sequence).map_err(|_| ServoFocAxisError::CounterOverflow)?;

        let update = ServoFocAxisUpdate {
            prepared: prepared.update,
            power_stage_commit,
            latch,
        };
        let next = Self {
            profile: self.profile,
            activation_id: self.activation_id,
            encoder: prepared.encoder,
            cascade: prepared.cascade,
            current_controller: prepared.current_controller,
            compare_owner: next_compare_owner,
            period_started_at: latch.observed_at,
            period_sequence,
            next_duty_token: prepared.next_duty_token,
            next_current_command_id: prepared.next_current_command_id,
            fault: None,
        };
        Ok((next, update))
    }
}

impl<const AXES: usize> ServoFocBank<AXES> {
    /// Validates a complete bank before any initial neutral image is staged.
    pub fn prepare_activation(
        profiles: [ServoFocAxisProfile; AXES],
        activation_ids: [u64; AXES],
        seeds: [ServoEncoderSeed; AXES],
        first_boundary: DeviceCycle,
    ) -> Result<PreparedServoFocBankActivation<AXES>, ServoFocBankError> {
        if AXES == 0 || AXES > MAX_SERVO_FOC_BANK_AXES {
            return Err(ServoFocBankError::AxisCount);
        }
        for axis in 0..AXES {
            if activation_ids[axis] == 0 {
                return Err(ServoFocBankError::Axis {
                    axis,
                    error: ServoFocAxisError::ActivationIdentity,
                });
            }
            if activation_ids
                .iter()
                .take(axis)
                .any(|prior| *prior == activation_ids[axis])
            {
                return Err(ServoFocBankError::Profile);
            }
        }

        let mut prepared = core::array::from_fn(|_| None);
        for axis in 0..AXES {
            let candidate = ServoFocAxisController::prepare_activation(
                profiles[axis],
                activation_ids[axis],
                seeds[axis],
                first_boundary,
            )
            .map_err(|error| ServoFocBankError::Axis { axis, error })?;
            prepared[axis] = Some(candidate);
        }

        let first = prepared[0]
            .as_ref()
            .expect("nonempty prepared FOC bank activation");
        let configuration_digest = first.profile.configuration_digest();
        let grid = first.cascade.grid();
        let pwm_period_cycles = first.profile.pwm_compare.pwm_period_device_cycles();
        for candidate in prepared
            .iter()
            .map(|candidate| candidate.as_ref().expect("complete FOC bank activation"))
        {
            if candidate.profile.configuration_digest() != configuration_digest
                || candidate.first_boundary != first_boundary
                || candidate.cascade.grid() != grid
                || candidate.profile.pwm_compare.pwm_period_device_cycles() != pwm_period_cycles
                || candidate.initial_image.configuration_digest() != configuration_digest
                || candidate.initial_image.scheduled_at() != first_boundary
            {
                return Err(ServoFocBankError::Profile);
            }
        }

        Ok(PreparedServoFocBankActivation {
            configuration_digest,
            first_boundary,
            pwm_period_cycles,
            prepared,
        })
    }

    /// Creates one live bank only after every prepared neutral image committed.
    pub fn activate(
        mut prepared: PreparedServoFocBankActivation<AXES>,
        completion: PwmCommitBankCompletion<AXES>,
    ) -> Result<Self, ServoFocBankError> {
        if completion.configuration_digest() != prepared.configuration_digest {
            return Err(ServoFocBankError::Profile);
        }
        if completion.period_sequence() != 0 || completion.observed_at() != prepared.first_boundary
        {
            return Err(ServoFocBankError::Schedule);
        }
        let commits = completion.commits();
        for (axis, commit) in commits.iter().copied().enumerate() {
            let image = prepared
                .prepared
                .get(axis)
                .and_then(Option::as_ref)
                .ok_or(ServoFocBankError::Schedule)?
                .initial_image();
            validate_power_stage_commit(commit, image)
                .map_err(|error| ServoFocBankError::Axis { axis, error })?;
        }

        let mut axes = core::array::from_fn(|_| None);
        for (axis, commit) in commits.iter().copied().enumerate() {
            let axis_prepared = prepared.prepared[axis]
                .take()
                .ok_or(ServoFocBankError::Schedule)?;
            let controller = ServoFocAxisController::activate(axis_prepared, commit)
                .map_err(|error| ServoFocBankError::Axis { axis, error })?;
            axes[axis] = Some(controller);
        }
        Self::from_activated_axes(axes.map(|axis| axis.expect("complete activated FOC bank")))
    }

    /// Internal final validation after the complete activation commit succeeds.
    fn from_activated_axes(
        axes: [ServoFocAxisController; AXES],
    ) -> Result<Self, ServoFocBankError> {
        if AXES == 0 || AXES > MAX_SERVO_FOC_BANK_AXES {
            return Err(ServoFocBankError::AxisCount);
        }
        let first = &axes[0];
        let configuration_digest = first.profile().configuration_digest();
        let period_started_at = first.period_started_at();
        let period_sequence = first.period_sequence();
        let grid = first.cascade().grid();
        if first.fault().is_some() {
            return Err(ServoFocBankError::Profile);
        }
        for axis in 0..AXES {
            let candidate = &axes[axis];
            if candidate.fault().is_some()
                || candidate.profile().configuration_digest() != configuration_digest
                || candidate.period_started_at() != period_started_at
                || candidate.period_sequence() != period_sequence
                || candidate.cascade().grid() != grid
                || candidate
                    .active_image()
                    .is_none_or(|image| image.scheduled_at() != period_started_at)
            {
                return Err(ServoFocBankError::Profile);
            }
            for prior in axes.iter().take(axis) {
                if prior.activation_id() == candidate.activation_id() {
                    return Err(ServoFocBankError::Profile);
                }
            }
        }
        Ok(Self {
            axes,
            configuration_digest,
            period_started_at,
            period_sequence,
            fault: None,
        })
    }

    /// Complete immutable configuration identity shared by every axis.
    pub const fn configuration_digest(&self) -> Digest {
        self.configuration_digest
    }

    /// Exact common current-loop boundary owned by the bank.
    pub const fn period_started_at(&self) -> DeviceCycle {
        self.period_started_at
    }

    /// Common physical PWM-period sequence accepted across every axis.
    pub const fn period_sequence(&self) -> u32 {
        self.period_sequence
    }

    /// Read-only access to one complete live axis controller.
    pub fn axis(&self, axis: usize) -> Option<&ServoFocAxisController> {
        self.axes.get(axis)
    }

    /// First bank-level cause which invalidated all subsequent transitions.
    pub const fn fault(&self) -> Option<ServoFocBankError> {
        self.fault
    }

    /// Calculates every complete axis transition without advancing any
    /// successfully prepared controller state.
    pub fn prepare(
        &mut self,
        inputs: [ServoFocAxisPeriodInput; AXES],
    ) -> Result<PreparedServoFocBankTransition<AXES>, ServoFocBankError> {
        if self.fault.is_some() {
            return Err(ServoFocBankError::FaultLatched);
        }
        if inputs
            .iter()
            .any(|input| input.at != self.period_started_at)
        {
            return self.latch(ServoFocBankError::Schedule);
        }

        let mut prepared = core::array::from_fn(|_| None);
        for axis in 0..AXES {
            let candidate = match self.axes[axis].prepare(inputs[axis]) {
                Ok(candidate) => candidate,
                Err(error) => return self.latch(ServoFocBankError::Axis { axis, error }),
            };
            prepared[axis] = Some(candidate);
        }
        Ok(PreparedServoFocBankTransition {
            base_period_started_at: self.period_started_at,
            base_period_sequence: self.period_sequence,
            prepared,
        })
    }

    /// Validates every exact physical image acknowledgement before installing
    /// any candidate axis controller state.
    pub fn commit(
        &mut self,
        mut prepared: PreparedServoFocBankTransition<AXES>,
        completion: PwmCommitBankCompletion<AXES>,
    ) -> Result<[ServoFocAxisUpdate; AXES], ServoFocBankError> {
        if self.fault.is_some() {
            return Err(ServoFocBankError::FaultLatched);
        }
        if prepared.base_period_started_at != self.period_started_at
            || prepared.base_period_sequence != self.period_sequence
        {
            return self.latch(ServoFocBankError::Schedule);
        }
        let expected_sequence = u64::from(self.period_sequence) + 1;
        if completion.configuration_digest() != self.configuration_digest {
            return self.latch(ServoFocBankError::Profile);
        }
        if completion.period_sequence() != expected_sequence {
            return self.latch(ServoFocBankError::Schedule);
        }
        let commits = completion.commits();

        let mut next_axes = core::array::from_fn(|_| None);
        let mut updates = core::array::from_fn(|_| None);
        for axis in 0..AXES {
            let Some(axis_prepared) = prepared.prepared[axis].take() else {
                return self.latch(ServoFocBankError::Schedule);
            };
            let (next, update) =
                match self.axes[axis].committed_candidate(axis_prepared, commits[axis]) {
                    Ok(candidate) => candidate,
                    Err(error) => return self.latch(ServoFocBankError::Axis { axis, error }),
                };
            next_axes[axis] = Some(next);
            updates[axis] = Some(update);
        }

        let Some(first) = next_axes[0].as_ref() else {
            return self.latch(ServoFocBankError::Schedule);
        };
        let next_period_started_at = first.period_started_at();
        let next_period_sequence = first.period_sequence();
        if next_axes.iter().any(|axis| {
            axis.as_ref().is_none_or(|axis| {
                axis.period_started_at() != next_period_started_at
                    || axis.period_sequence() != next_period_sequence
            })
        }) {
            return self.latch(ServoFocBankError::Schedule);
        }

        self.axes = next_axes.map(|axis| axis.expect("complete FOC bank candidate"));
        self.period_started_at = next_period_started_at;
        self.period_sequence = next_period_sequence;
        Ok(updates.map(|update| update.expect("complete FOC bank update")))
    }

    /// Closes the logical owner after the enclosing target has synchronously
    /// completed its separately qualified all-power-stage safe transaction.
    pub fn invalidate_after_safe(&mut self) {
        if self.fault.is_none() {
            self.fault = Some(ServoFocBankError::SafetyInvalidated);
        }
    }

    fn latch<T>(&mut self, error: ServoFocBankError) -> Result<T, ServoFocBankError> {
        self.fault = Some(error);
        Err(error)
    }
}

fn validate_power_stage_commit(
    commit: PowerStageCommit,
    image: PwmCompareImage,
) -> Result<(), ServoFocAxisError> {
    if commit.token != image.token()
        || commit.scheduled_at != image.scheduled_at()
        || commit.observed_at != image.scheduled_at()
    {
        return Err(ServoFocAxisError::PowerStageCommit);
    }
    Ok(())
}

/// First-cause profile, observation, control, staging, or commit rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoFocAxisError {
    /// A nonzero boot-local activation identity is required.
    ActivationIdentity,
    /// The seed was unavailable or could not yield a fresh first grid sample.
    SeedSchedule,
    /// Immutable profiles disagreed across identity, clocks, or geometry.
    Profile,
    /// The configured integer loop grid rejected.
    Grid(ServoLoopGridError),
    /// The outer-loop profile rejected against the inner snapshot/grid.
    ServoProfile(ServoCascadeProfileError),
    /// The absolute-encoder profile rejected.
    EncoderProfile(ServoEncoderProfileError),
    /// Seed or runtime absolute-count estimation rejected.
    Encoder(ServoEncoderError),
    /// Encoder observation presence disagreed with the velocity grid.
    EncoderPresence { required: bool, received: bool },
    /// The cascaded position/velocity owner rejected the boundary.
    Cascade(ServoCascadeError),
    /// FOC/current/angle/control arithmetic or evidence rejected.
    Foc(FocError),
    /// Duty-to-integer-compare lowering or replay rejected.
    Compare(PwmCompareError),
    /// Complete-image staging or timer-zero sequencing rejected.
    Latch(PwmCompareLatchError),
    /// A caller supplied another active period.
    Period {
        expected: DeviceCycle,
        received: DeviceCycle,
    },
    /// Current acquisition did not name the exact active image/period.
    CurrentWitness,
    /// A prepared transition did not originate at the actor's current prefix.
    PreparedState,
    /// The physical commit did not name the exact staged image and boundary.
    PowerStageCommit,
    /// A device-cycle, token, command, or period counter overflowed.
    CounterOverflow,
    /// The exact first cause is already retained.
    FaultLatched,
}

impl From<FocError> for ServoFocAxisError {
    fn from(error: FocError) -> Self {
        Self::Foc(error)
    }
}

impl From<PwmCompareError> for ServoFocAxisError {
    fn from(error: PwmCompareError) -> Self {
        Self::Compare(error)
    }
}

impl From<PwmCompareLatchError> for ServoFocAxisError {
    fn from(error: PwmCompareLatchError) -> Self {
        Self::Latch(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CountUncertainty, CurrentChannelCalibration, CurrentPolarity, ElectricalPhase,
        FocTimingProfile, PiConfig, PwmAdcSynchronization, PwmCommitBankBarrier, Q30,
        RotorCountDirection, ServoEncoderScale, ServoPosition, TwoShuntCurrentCalibration,
        TwoShuntPhasePair,
    };

    const CONFIGURATION: Digest = Digest([0x6a; 32]);
    const ACTIVATION: u64 = 0x1020_3040_5060_7080;
    const FIRST_BOUNDARY: DeviceCycle = DeviceCycle(1_000);

    fn point(numerator: i64, denominator: i64) -> Q30 {
        Q30Interval::from_ratio(numerator, denominator)
            .unwrap()
            .midpoint()
    }

    fn pi(limit: Q30) -> PiConfig {
        PiConfig {
            proportional_gain: point(1, 4),
            integral_gain_per_update: point(1, 64),
            integral_minimum: limit.checked_neg().unwrap(),
            integral_maximum: limit,
            output_minimum: limit.checked_neg().unwrap(),
            output_maximum: limit,
        }
    }

    fn profile() -> ServoFocAxisProfile {
        profile_with_digest(CONFIGURATION)
    }

    fn profile_with_digest(configuration_digest: Digest) -> ServoFocAxisProfile {
        let parameters = FocParameterSnapshot {
            configuration_digest,
            pole_pairs: 7,
            timing: FocTimingProfile {
                pwm_hz: 10_000,
                current_loop_hz: 10_000,
                velocity_loop_divider: 2,
                position_loop_divider: 2,
            },
            maximum_phase_current: Q30::ONE,
            maximum_phase_voltage: Q30::ONE,
            d_current: pi(Q30::HALF),
            q_current: pi(Q30::HALF),
        };
        let synchronization = PwmAdcSynchronization {
            configuration_digest,
            device_cycle_hz: 1_000_000,
            pwm_period_cycles: 100,
            nominal_acquisition_offset_cycles: 50,
            maximum_trigger_jitter_cycles: 0,
            maximum_acquisition_cycles: 2,
            maximum_channel_skew_cycles: 0,
            maximum_conversion_cycles: 2,
            minimum_switching_guard_cycles: 10,
        };
        let channel = CurrentChannelCalibration {
            adc_maximum_count: 4_095,
            valid_count_minimum: 1_000,
            valid_count_maximum: 3_000,
            count_at_zero: 2_000,
            polarity: CurrentPolarity::Increasing,
            normalized_current_per_count: Q30Interval::point(Q30::from_bits(1 << 15)),
            maximum_additive_error: Q30::from_bits(1 << 14),
            maximum_interval_width_ulps: 1 << 17,
        };
        let current = TwoShuntCurrentCalibration {
            configuration_digest,
            phase_pair: TwoShuntPhasePair::Ab,
            channel0: channel,
            channel1: channel,
            synchronization,
            maximum_normalized_current_slew_per_cycle: Q30::ZERO,
            maximum_interchannel_skew_error: Q30::ZERO,
            maximum_phase_current: Q30::ONE,
            maximum_phase_interval_width_ulps: 1 << 20,
        }
        .validated()
        .unwrap();
        let rotor = RotorCalibration {
            configuration_digest,
            counts_per_mechanical_turn: 4_096,
            count_at_reference: 0,
            electrical_phase_at_reference: ElectricalPhase::ZERO,
            pole_pairs: 7,
            direction: RotorCountDirection::Increasing,
            maximum_alignment_error_bits: 1_024,
            maximum_count_error: CountUncertainty::new(1, 2).unwrap(),
        };
        let rotation_precision = RotationPrecision {
            maximum_component_width_ulps: 12_000_000,
            maximum_norm_error_ulps: 12_000_000,
        };
        let pwm_compare = PwmCompareContract::new(
            configuration_digest,
            1_000_000,
            100,
            1_000_000,
            50,
            2,
            u32::MAX,
        )
        .unwrap();
        let servo = ServoCascadeConfig {
            configuration_digest,
            position_proportional_gain: Q30::ONE,
            velocity_controller: pi(Q30::HALF),
            maximum_velocity: Q30::ONE,
            maximum_current: Q30::HALF,
            direct_current_target: Q30::ZERO,
            maximum_following_error_bits: 1_u64 << 34,
            maximum_sample_age_cycles: 200,
        };
        let encoder = ServoEncoderProfile {
            configuration_digest,
            counts_per_mechanical_turn: 4_096,
            count_at_reference: 0,
            direction: RotorCountDirection::Increasing,
            maximum_count_error: CountUncertainty::new(1, 2).unwrap(),
            position_at_reference: ServoPosition::ZERO,
            scale: ServoEncoderScale::new(1_u64 << 32, 1, 40_960, 1).unwrap(),
            device_cycle_hz: 1_000_000,
            sample_period_cycles: 200,
            maximum_observation_latency_cycles: 20,
            maximum_trackable_velocity: Q30::ONE,
            maximum_admitted_velocity: Q30::ONE,
            maximum_velocity_estimation_error: Q30::from_bits(1 << 20),
            maximum_position_interval_width_ulps: 1 << 21,
            maximum_velocity_interval_width_ulps: 400_000_000,
        };
        ServoFocAxisProfile {
            parameters,
            rotor,
            rotation_precision,
            current,
            pwm_compare,
            servo,
            encoder,
        }
    }

    fn seed() -> ServoEncoderSeed {
        seed_with_digest(CONFIGURATION)
    }

    fn seed_with_digest(configuration_digest: Digest) -> ServoEncoderSeed {
        ServoEncoderSeed {
            observation: ServoEncoderObservation {
                configuration_digest,
                raw_count: 0,
                sampled_at: DeviceCycle(800),
                available_at: DeviceCycle(800),
            },
            turn_index: 0,
        }
    }

    fn initial_commit() -> PowerStageCommit {
        PowerStageCommit {
            token: INITIAL_DUTY_TOKEN,
            scheduled_at: FIRST_BOUNDARY,
            observed_at: FIRST_BOUNDARY,
        }
    }

    fn bank_completion<const AXES: usize>(
        images: [PwmCompareImage; AXES],
        pwm_period_cycles: u32,
        period_sequence: u64,
    ) -> PwmCommitBankCompletion<AXES> {
        let boundary = images[0].scheduled_at();
        let mut barrier = PwmCommitBankBarrier::new(
            images[0].configuration_digest(),
            pwm_period_cycles,
            boundary,
            period_sequence,
        )
        .unwrap();
        barrier.stage(images).unwrap();
        for (axis, image) in images.iter().copied().enumerate() {
            barrier
                .record_latch(
                    axis,
                    PowerStageCommit {
                        token: image.token(),
                        scheduled_at: image.scheduled_at(),
                        observed_at: boundary,
                    },
                )
                .unwrap();
        }
        barrier.finish_boundary(boundary).unwrap()
    }

    fn controller() -> ServoFocAxisController {
        controller_with_activation(ACTIVATION)
    }

    fn controller_with_activation(activation_id: u64) -> ServoFocAxisController {
        let prepared = ServoFocAxisController::prepare_activation(
            profile(),
            activation_id,
            seed(),
            FIRST_BOUNDARY,
        )
        .unwrap();
        assert_eq!(prepared.initial_image().token(), INITIAL_DUTY_TOKEN);
        ServoFocAxisController::activate(prepared, initial_commit()).unwrap()
    }

    fn stamp(controller: &ServoFocAxisController) -> PwmAdcSampleStamp {
        let image = controller.active_image().unwrap();
        let at = controller.period_started_at();
        let mut previous = None;
        let mut next = None;
        for comparison in image.comparisons() {
            for edge in [
                comparison.first_switching_edge_ticks(),
                comparison.second_switching_edge_ticks(),
            ] {
                let edge = u64::from(edge);
                if edge <= 50 {
                    previous = Some(previous.map_or(edge, |prior: u64| prior.max(edge)));
                }
                if edge >= 50 {
                    next = Some(next.map_or(edge, |following: u64| following.min(edge)));
                }
            }
        }
        let add = |offset: u64| DeviceCycle(at.0 + offset);
        PwmAdcSampleStamp {
            configuration_digest: CONFIGURATION,
            duty_token: image.token(),
            period_sequence: controller.period_sequence(),
            period_started_at: at,
            previous_switching_edge_at: add(previous.unwrap()),
            acquisition_started_at: add(50),
            channel0_sampled_at: add(50),
            channel1_sampled_at: add(50),
            conversion_completed_at: add(51),
            next_switching_edge_at: add(next.unwrap()),
        }
    }

    fn setpoint(command_id: u32, at: DeviceCycle, position: ServoPosition) -> ServoSetpoint {
        ServoSetpoint {
            command_id,
            scheduled_at: at,
            configuration_digest: CONFIGURATION,
            position,
            velocity_feed_forward: Q30::ZERO,
            quadrature_current_feed_forward: Q30::ZERO,
        }
    }

    fn encoder_observation(at: DeviceCycle, raw_count: u32) -> ServoEncoderObservation {
        ServoEncoderObservation {
            configuration_digest: CONFIGURATION,
            raw_count,
            sampled_at: at,
            available_at: at,
        }
    }

    fn input(
        controller: &ServoFocAxisController,
        setpoint: Option<ServoSetpoint>,
        encoder_observation: Option<ServoEncoderObservation>,
    ) -> ServoFocAxisPeriodInput {
        ServoFocAxisPeriodInput {
            at: controller.period_started_at(),
            setpoint,
            encoder_observation,
            raw_current_counts: [2_000, 2_000],
            current_stamp: stamp(controller),
            raw_rotor_count: 0,
        }
    }

    fn exact_commit(prepared: &PreparedServoFocAxisTransition) -> PowerStageCommit {
        let image = prepared.staged_image();
        PowerStageCommit {
            token: image.token(),
            scheduled_at: image.scheduled_at(),
            observed_at: image.scheduled_at(),
        }
    }

    #[test]
    fn profile_replays_every_identity_geometry_and_clock_seam() {
        assert!(profile().validate_at(FIRST_BOUNDARY).is_ok());

        let mut foreign_encoder = profile();
        foreign_encoder.encoder.configuration_digest = Digest([0x55; 32]);
        assert_eq!(
            foreign_encoder.validate_at(FIRST_BOUNDARY),
            Err(ServoFocAxisError::Profile)
        );

        let mut wrong_modulus = profile();
        wrong_modulus.encoder.counts_per_mechanical_turn = 8_192;
        assert_eq!(
            wrong_modulus.validate_at(FIRST_BOUNDARY),
            Err(ServoFocAxisError::Profile)
        );

        let mut fractional_rate = profile();
        fractional_rate.parameters.timing.current_loop_hz = 8_000;
        assert!(matches!(
            fractional_rate.validate_at(FIRST_BOUNDARY),
            Err(ServoFocAxisError::Grid(_))
                | Err(ServoFocAxisError::Profile)
                | Err(ServoFocAxisError::Compare(_))
                | Err(ServoFocAxisError::Foc(FocError::Profile))
        ));

        assert_eq!(
            ServoFocAxisController::prepare_activation(profile(), 0, seed(), FIRST_BOUNDARY),
            Err(ServoFocAxisError::ActivationIdentity)
        );
        let mut late_seed = seed();
        late_seed.observation.sampled_at = DeviceCycle(950);
        assert_eq!(
            ServoFocAxisController::prepare_activation(
                profile(),
                ACTIVATION,
                late_seed,
                FIRST_BOUNDARY
            ),
            Err(ServoFocAxisError::SeedSchedule)
        );
    }

    #[test]
    fn activation_exposes_neutral_image_before_accepting_physical_state() {
        let prepared = ServoFocAxisController::prepare_activation(
            profile(),
            ACTIVATION,
            seed(),
            FIRST_BOUNDARY,
        )
        .unwrap();
        assert_eq!(prepared.activation_id(), ACTIVATION);
        assert_eq!(prepared.first_boundary(), FIRST_BOUNDARY);
        assert_eq!(prepared.initial_image().token(), INITIAL_DUTY_TOKEN);
        assert!(
            prepared
                .initial_image()
                .comparisons()
                .iter()
                .all(|comparison| comparison.compare_ticks() == 25)
        );

        let mut wrong = initial_commit();
        wrong.observed_at = DeviceCycle(FIRST_BOUNDARY.0 + 1);
        assert_eq!(
            ServoFocAxisController::activate(prepared, wrong),
            Err(ServoFocAxisError::PowerStageCommit)
        );
    }

    #[test]
    fn complete_bank_activation_publishes_no_live_partial_owner() {
        let prepared = ServoFocBank::<2>::prepare_activation(
            [profile(), profile()],
            [ACTIVATION, ACTIVATION + 1],
            [seed(), seed()],
            FIRST_BOUNDARY,
        )
        .unwrap();
        assert_eq!(prepared.configuration_digest(), CONFIGURATION);
        assert_eq!(prepared.first_boundary(), FIRST_BOUNDARY);
        assert_eq!(prepared.pwm_period_cycles(), 100);
        let images = prepared.initial_images();
        assert_eq!(prepared.initial_image(2), None);
        assert!(images.iter().all(|image| {
            image.configuration_digest() == CONFIGURATION
                && image.scheduled_at() == FIRST_BOUNDARY
                && image.token() == INITIAL_DUTY_TOKEN
                && image
                    .comparisons()
                    .iter()
                    .all(|comparison| comparison.compare_ticks() == 25)
        }));

        let mut late = PwmCommitBankBarrier::new(CONFIGURATION, 100, FIRST_BOUNDARY, 0).unwrap();
        late.stage(images).unwrap();
        late.record_latch(0, initial_commit()).unwrap();
        assert_eq!(
            late.record_latch(
                1,
                PowerStageCommit {
                    observed_at: DeviceCycle(FIRST_BOUNDARY.0 + 1),
                    ..initial_commit()
                }
            ),
            Err(crate::PwmCommitBarrierError::Observation { axis: 1 })
        );
        let wrong_sequence = bank_completion(images, 100, 1);
        assert_eq!(
            ServoFocBank::activate(prepared, wrong_sequence),
            Err(ServoFocBankError::Schedule)
        );

        let prepared = ServoFocBank::<2>::prepare_activation(
            [profile(), profile()],
            [ACTIVATION, ACTIVATION + 1],
            [seed(), seed()],
            FIRST_BOUNDARY,
        )
        .unwrap();
        let foreign = Digest([0x6b; 32]);
        let foreign_prepared = ServoFocBank::<2>::prepare_activation(
            [profile_with_digest(foreign); 2],
            [ACTIVATION + 2, ACTIVATION + 3],
            [seed_with_digest(foreign); 2],
            FIRST_BOUNDARY,
        )
        .unwrap();
        let foreign_completion = bank_completion(foreign_prepared.initial_images(), 100, 0);
        assert_eq!(
            ServoFocBank::activate(prepared, foreign_completion),
            Err(ServoFocBankError::Profile)
        );

        let prepared = ServoFocBank::<2>::prepare_activation(
            [profile(), profile()],
            [ACTIVATION, ACTIVATION + 1],
            [seed(), seed()],
            FIRST_BOUNDARY,
        )
        .unwrap();
        let completion = bank_completion(prepared.initial_images(), 100, 0);
        let bank = ServoFocBank::activate(prepared, completion).unwrap();
        assert_eq!(bank.configuration_digest(), CONFIGURATION);
        assert_eq!(bank.period_started_at(), FIRST_BOUNDARY);
        assert_eq!(bank.period_sequence(), 0);
        for (axis, image) in images.iter().copied().enumerate() {
            let controller = bank.axis(axis).unwrap();
            assert_eq!(controller.activation_id(), ACTIVATION + axis as u64);
            assert_eq!(controller.active_image(), Some(image));
        }
    }

    #[test]
    fn bank_activation_rejects_width_identity_and_axis_substitution_before_staging() {
        assert_eq!(
            ServoFocBank::<0>::prepare_activation([], [], [], FIRST_BOUNDARY),
            Err(ServoFocBankError::AxisCount)
        );
        assert_eq!(
            ServoFocBank::<5>::prepare_activation(
                [profile(); 5],
                core::array::from_fn(|axis| ACTIVATION + axis as u64),
                [seed(); 5],
                FIRST_BOUNDARY,
            ),
            Err(ServoFocBankError::AxisCount)
        );
        assert_eq!(
            ServoFocBank::<2>::prepare_activation(
                [profile(), profile()],
                [ACTIVATION, ACTIVATION],
                [seed(), seed()],
                FIRST_BOUNDARY,
            ),
            Err(ServoFocBankError::Profile)
        );
        assert_eq!(
            ServoFocBank::<2>::prepare_activation(
                [profile(), profile()],
                [0, ACTIVATION + 1],
                [seed(), seed()],
                FIRST_BOUNDARY,
            ),
            Err(ServoFocBankError::Axis {
                axis: 0,
                error: ServoFocAxisError::ActivationIdentity,
            })
        );

        let foreign = Digest([0x6b; 32]);
        assert_eq!(
            ServoFocBank::<2>::prepare_activation(
                [profile(), profile_with_digest(foreign)],
                [ACTIVATION, ACTIVATION + 1],
                [seed(), seed_with_digest(foreign)],
                FIRST_BOUNDARY,
            ),
            Err(ServoFocBankError::Profile)
        );
    }

    #[test]
    fn exact_nested_axis_transition_advances_only_after_physical_commit() {
        let mut controller = controller();
        let first_input = input(
            &controller,
            Some(setpoint(1, FIRST_BOUNDARY, ServoPosition::ZERO)),
            Some(encoder_observation(FIRST_BOUNDARY, 0)),
        );
        let prepared = controller.prepare(first_input).unwrap();
        let first = prepared.update();
        assert_eq!(first.period_sequence, 0);
        assert_eq!(first.encoder.unwrap().sample.sequence, 1);
        assert!(first.cascade.position_updated);
        assert!(first.cascade.velocity_updated);
        assert_eq!(first.current_command.command_id, 1);
        assert_eq!(first.staged_image.token(), 2);
        assert_eq!(first.staged_image.scheduled_at(), DeviceCycle(1_100));

        assert_eq!(controller.period_started_at(), FIRST_BOUNDARY);
        assert_eq!(controller.encoder().estimate_sequence(), 0);
        assert_eq!(controller.cascade().last_current_index(), None);

        let commit = exact_commit(&prepared);
        let accepted = controller.commit(prepared, commit).unwrap();
        assert_eq!(accepted.latch.period_sequence, 1);
        assert_eq!(controller.period_started_at(), DeviceCycle(1_100));
        assert_eq!(controller.encoder().estimate_sequence(), 1);
        assert_eq!(controller.cascade().last_current_index(), Some(0));

        let mut setpoint_id = 1;
        let mut encoder_sample = FIRST_BOUNDARY.0;
        for expected_index in 1_u64..=4 {
            let at = controller.period_started_at();
            let velocity_due = expected_index.is_multiple_of(2);
            let position_due = expected_index.is_multiple_of(4);
            let observation = if velocity_due {
                encoder_sample += 200;
                Some(encoder_observation(DeviceCycle(encoder_sample), 0))
            } else {
                None
            };
            let next_setpoint = if position_due {
                setpoint_id += 1;
                Some(setpoint(setpoint_id, at, ServoPosition::ZERO))
            } else {
                None
            };
            let prepared = controller
                .prepare(input(&controller, next_setpoint, observation))
                .unwrap();
            assert_eq!(
                prepared.update().current_command.command_id,
                u32::try_from(expected_index + 1).unwrap()
            );
            let commit = exact_commit(&prepared);
            controller.commit(prepared, commit).unwrap();
        }

        assert_eq!(controller.cascade().last_current_index(), Some(4));
        assert_eq!(controller.cascade().position_updates(), 2);
        assert_eq!(controller.cascade().velocity_updates(), 3);
        assert_eq!(controller.encoder().estimate_sequence(), 3);
        assert_eq!(controller.period_started_at(), DeviceCycle(1_500));
        assert_eq!(controller.period_sequence(), 5);
        assert_eq!(controller.fault(), None);
    }

    #[test]
    fn two_axis_bank_installs_only_a_complete_commit_set() {
        fn bank() -> ServoFocBank<2> {
            ServoFocBank::from_activated_axes([
                controller_with_activation(ACTIVATION),
                controller_with_activation(ACTIVATION + 1),
            ])
            .unwrap()
        }

        fn first_inputs(bank: &ServoFocBank<2>) -> [ServoFocAxisPeriodInput; 2] {
            core::array::from_fn(|axis| {
                let controller = bank.axis(axis).unwrap();
                input(
                    controller,
                    Some(setpoint(
                        1,
                        FIRST_BOUNDARY,
                        ServoPosition::from_bits(i64::try_from(axis).unwrap() << 28),
                    )),
                    Some(encoder_observation(FIRST_BOUNDARY, 0)),
                )
            })
        }

        fn completion(
            prepared: &PreparedServoFocBankTransition<2>,
            period_sequence: u64,
        ) -> PwmCommitBankCompletion<2> {
            let images =
                core::array::from_fn(|axis| prepared.axis_update(axis).unwrap().staged_image);
            bank_completion(images, 100, period_sequence)
        }

        let mut rejected = bank();
        let prepared = rejected.prepare(first_inputs(&rejected)).unwrap();
        assert_eq!(prepared.period_started_at(), FIRST_BOUNDARY);
        let wrong_sequence = completion(&prepared, 2);
        assert_eq!(
            rejected.commit(prepared, wrong_sequence),
            Err(ServoFocBankError::Schedule)
        );
        assert_eq!(rejected.period_started_at(), FIRST_BOUNDARY);
        assert_eq!(rejected.period_sequence(), 0);
        for axis in 0..2 {
            let controller = rejected.axis(axis).unwrap();
            assert_eq!(controller.period_started_at(), FIRST_BOUNDARY);
            assert_eq!(controller.period_sequence(), 0);
            assert_eq!(controller.encoder().estimate_sequence(), 0);
            assert_eq!(controller.cascade().last_current_index(), None);
        }
        assert_eq!(rejected.fault(), Some(ServoFocBankError::Schedule));
        assert_eq!(
            rejected.prepare(first_inputs(&rejected)),
            Err(ServoFocBankError::FaultLatched)
        );

        let mut accepted = bank();
        let prepared = accepted.prepare(first_inputs(&accepted)).unwrap();
        let completion = completion(&prepared, 1);
        let updates = accepted.commit(prepared, completion).unwrap();
        assert_eq!(accepted.period_started_at(), DeviceCycle(1_100));
        assert_eq!(accepted.period_sequence(), 1);
        for (axis, update) in updates.iter().enumerate() {
            assert_eq!(update.prepared.activation_id, ACTIVATION + axis as u64);
            assert_eq!(update.prepared.cascade.setpoint_id, 1);
            assert_eq!(update.latch.period_sequence, 1);
            assert_eq!(
                accepted.axis(axis).unwrap().period_started_at(),
                DeviceCycle(1_100)
            );
        }

        accepted.invalidate_after_safe();
        assert_eq!(accepted.fault(), Some(ServoFocBankError::SafetyInvalidated));
    }

    #[test]
    fn bank_rejects_empty_and_duplicate_activation_ownership() {
        assert_eq!(
            ServoFocBank::<0>::from_activated_axes([]),
            Err(ServoFocBankError::AxisCount)
        );
        assert_eq!(
            ServoFocBank::from_activated_axes([controller(), controller()]),
            Err(ServoFocBankError::Profile)
        );
    }

    #[test]
    fn late_inner_failure_does_not_advance_estimator_or_outer_controller() {
        let mut controller = controller();
        let mut invalid = input(
            &controller,
            Some(setpoint(1, FIRST_BOUNDARY, ServoPosition::ZERO)),
            Some(encoder_observation(FIRST_BOUNDARY, 0)),
        );
        invalid.raw_current_counts[0] = 999;
        assert_eq!(
            controller.prepare(invalid),
            Err(ServoFocAxisError::Foc(FocError::CurrentRawSample))
        );
        assert_eq!(controller.period_started_at(), FIRST_BOUNDARY);
        assert_eq!(controller.encoder().estimate_sequence(), 0);
        assert_eq!(controller.cascade().last_current_index(), None);
        assert_eq!(
            controller.fault(),
            Some(ServoFocAxisError::Foc(FocError::CurrentRawSample))
        );
        assert_eq!(
            controller.prepare(invalid),
            Err(ServoFocAxisError::FaultLatched)
        );
    }

    #[test]
    fn commit_mismatch_faults_without_accepting_candidate_state() {
        let mut controller = controller();
        let prepared = controller
            .prepare(input(
                &controller,
                Some(setpoint(1, FIRST_BOUNDARY, ServoPosition::ZERO)),
                Some(encoder_observation(FIRST_BOUNDARY, 0)),
            ))
            .unwrap();
        let mut wrong_commit = exact_commit(&prepared);
        wrong_commit.token += 1;
        assert_eq!(
            controller.commit(prepared, wrong_commit),
            Err(ServoFocAxisError::PowerStageCommit)
        );
        assert_eq!(controller.period_started_at(), FIRST_BOUNDARY);
        assert_eq!(controller.encoder().estimate_sequence(), 0);
        assert_eq!(controller.cascade().last_current_index(), None);
        assert_eq!(
            controller.fault(),
            Some(ServoFocAxisError::PowerStageCommit)
        );
    }

    #[test]
    fn prepared_transition_is_activation_and_prefix_bound() {
        let mut first = controller();
        let mut second = controller_with_activation(ACTIVATION + 1);
        let prepared = first
            .prepare(input(
                &first,
                Some(setpoint(1, FIRST_BOUNDARY, ServoPosition::ZERO)),
                Some(encoder_observation(FIRST_BOUNDARY, 0)),
            ))
            .unwrap();
        let commit = exact_commit(&prepared);
        assert_eq!(
            second.commit(prepared, commit),
            Err(ServoFocAxisError::PreparedState)
        );
        assert_eq!(second.period_started_at(), FIRST_BOUNDARY);
        assert_eq!(second.encoder().estimate_sequence(), 0);
        assert_eq!(second.fault(), Some(ServoFocAxisError::PreparedState));
        assert_eq!(first.fault(), None);
    }

    #[test]
    fn repeated_preparation_is_identical_but_a_consumed_prefix_cannot_replay() {
        let mut controller = controller();
        let period = input(
            &controller,
            Some(setpoint(1, FIRST_BOUNDARY, ServoPosition::ZERO)),
            Some(encoder_observation(FIRST_BOUNDARY, 0)),
        );
        let first = controller.prepare(period).unwrap();
        let stale = controller.prepare(period).unwrap();
        assert_eq!(first, stale);

        let first_commit = exact_commit(&first);
        controller.commit(first, first_commit).unwrap();
        assert_eq!(controller.period_started_at(), DeviceCycle(1_100));
        assert_eq!(controller.encoder().estimate_sequence(), 1);

        let stale_commit = exact_commit(&stale);
        assert_eq!(
            controller.commit(stale, stale_commit),
            Err(ServoFocAxisError::PreparedState)
        );
        assert_eq!(controller.period_started_at(), DeviceCycle(1_100));
        assert_eq!(controller.encoder().estimate_sequence(), 1);
        assert_eq!(controller.fault(), Some(ServoFocAxisError::PreparedState));
    }

    #[test]
    fn encoder_presence_and_current_witness_are_exact() {
        let mut wrong_witness = controller();
        let mut first = input(
            &wrong_witness,
            Some(setpoint(1, FIRST_BOUNDARY, ServoPosition::ZERO)),
            Some(encoder_observation(FIRST_BOUNDARY, 0)),
        );
        first.current_stamp.duty_token += 1;
        assert_eq!(
            wrong_witness.prepare(first),
            Err(ServoFocAxisError::CurrentWitness)
        );

        let mut wrong_presence = controller();
        let prepared = wrong_presence
            .prepare(input(
                &wrong_presence,
                Some(setpoint(1, FIRST_BOUNDARY, ServoPosition::ZERO)),
                Some(encoder_observation(FIRST_BOUNDARY, 0)),
            ))
            .unwrap();
        let commit = exact_commit(&prepared);
        wrong_presence.commit(prepared, commit).unwrap();
        let at = wrong_presence.period_started_at();
        assert_eq!(
            wrong_presence.prepare(input(
                &wrong_presence,
                None,
                Some(encoder_observation(DeviceCycle(1_200), 0)),
            )),
            Err(ServoFocAxisError::EncoderPresence {
                required: false,
                received: true,
            })
        );
        assert_eq!(wrong_presence.period_started_at(), at);
        assert_eq!(wrong_presence.encoder().estimate_sequence(), 1);
    }

    #[test]
    fn complete_actor_and_prepared_transition_have_fixed_small_bounds() {
        let actor = core::mem::size_of::<ServoFocAxisController>();
        let activation = core::mem::size_of::<PreparedServoFocAxisActivation>();
        let transition = core::mem::size_of::<PreparedServoFocAxisTransition>();
        let update = core::mem::size_of::<ServoFocAxisPreparedUpdate>();
        assert!(
            actor <= MAX_SERVO_FOC_AXIS_CONTROLLER_BYTES,
            "actor bytes: {actor}"
        );
        assert!(
            activation <= MAX_PREPARED_SERVO_FOC_AXIS_ACTIVATION_BYTES,
            "activation bytes: {activation}"
        );
        assert!(
            transition <= MAX_PREPARED_SERVO_FOC_AXIS_TRANSITION_BYTES,
            "transition bytes: {transition}"
        );
        assert!(
            update <= MAX_SERVO_FOC_AXIS_PREPARED_UPDATE_BYTES,
            "update bytes: {update}"
        );
        let two_axis_bank = core::mem::size_of::<ServoFocBank<2>>();
        let four_axis_activation = ServoFocBank::<MAX_SERVO_FOC_BANK_AXES>::prepare_activation(
            [profile(); MAX_SERVO_FOC_BANK_AXES],
            core::array::from_fn(|axis| ACTIVATION + axis as u64),
            [seed(); MAX_SERVO_FOC_BANK_AXES],
            FIRST_BOUNDARY,
        )
        .unwrap();
        assert_eq!(
            four_axis_activation.initial_images().len(),
            MAX_SERVO_FOC_BANK_AXES
        );
        let four_axis_activation = core::mem::size_of_val(&four_axis_activation);
        let two_axis_transition = core::mem::size_of::<PreparedServoFocBankTransition<2>>();
        assert!(
            two_axis_bank <= 2 * MAX_SERVO_FOC_AXIS_CONTROLLER_BYTES + 128,
            "two-axis bank bytes: {two_axis_bank}"
        );
        assert!(
            two_axis_transition <= 2 * MAX_PREPARED_SERVO_FOC_AXIS_TRANSITION_BYTES + 32,
            "two-axis transition bytes: {two_axis_transition}"
        );
        assert!(
            four_axis_activation <= MAX_PREPARED_SERVO_FOC_BANK_ACTIVATION_BYTES,
            "four-axis activation bytes: {four_axis_activation}"
        );
    }
}
