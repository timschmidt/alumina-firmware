//! Portable core-1 ownership and exact replay of cached servo setpoints.

use alumina_foc::{Q30, Q30_SCALE, ServoFocAxisProfile, ServoPosition, ServoSetpoint};
use alumina_job::AdmittedBlock;
use alumina_machine_ir::{
    BlockError, ExecutionKind, MAX_SERVO_EXECUTION_AXES,
    ServoFiniteDifferenceBlockValidationLimits, ServoFiniteDifferenceSegment,
    ServoFiniteDifferenceValidationLimits, StreamTick,
};
use alumina_protocol::{DeviceCycle, Digest};

/// Fixed ownership-horizon policy layered over exact per-axis FOC profiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CachedServoStreamPolicy {
    /// Longest independently owned block horizon in device ticks.
    pub maximum_block_ticks: u64,
    /// Longest recurrence-record horizon in device ticks.
    pub maximum_segment_ticks: u64,
    /// Largest dense update count in one recurrence record.
    pub maximum_update_count: u32,
}

/// Configuration-derived facts sufficient to validate one servo command axis.
///
/// Construction is possible only by replaying a complete [`ServoFocAxisProfile`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServoSetpointAxisAdmissionProfile {
    configuration_digest: Digest,
    position_period_ticks: u32,
    maximum_position_increment_bits: u64,
    maximum_velocity_feed_forward_bits: u32,
    maximum_quadrature_current_feed_forward_bits: u32,
}

impl ServoSetpointAxisAdmissionProfile {
    /// Validate a complete axis and derive exact setpoint-lattice limits.
    pub fn from_foc_axis(
        profile: ServoFocAxisProfile,
    ) -> Result<Self, ServoSetpointAdmissionProfileError> {
        let grid = profile
            .validate_at(DeviceCycle(0))
            .map_err(|_| ServoSetpointAdmissionProfileError::FocProfile)?;
        let position_period_ticks = u32::try_from(grid.position_period_cycles())
            .map_err(|_| ServoSetpointAdmissionProfileError::Arithmetic)?;
        let maximum_position_increment_bits = maximum_position_increment_bits(
            profile.servo.maximum_velocity.bits(),
            profile.encoder.scale,
            profile.encoder.counts_per_mechanical_turn,
            profile.encoder.device_cycle_hz,
            grid.position_period_cycles(),
        )?;
        let maximum_velocity_feed_forward_bits =
            u32::try_from(profile.servo.maximum_velocity.bits())
                .map_err(|_| ServoSetpointAdmissionProfileError::Velocity)?;
        let output_minimum = profile.servo.velocity_controller.output_minimum.bits();
        let output_maximum = profile.servo.velocity_controller.output_maximum.bits();
        if output_minimum > 0 || output_maximum < 0 {
            return Err(ServoSetpointAdmissionProfileError::Current);
        }
        let circle = current_circle_quadrature_limit(
            profile.servo.maximum_current.bits(),
            profile.servo.direct_current_target.bits(),
        )?;
        let negative_authority = output_minimum.unsigned_abs();
        let positive_authority = u32::try_from(output_maximum)
            .map_err(|_| ServoSetpointAdmissionProfileError::Current)?;
        let maximum_quadrature_current_feed_forward_bits =
            circle.min(negative_authority).min(positive_authority);
        Ok(Self {
            configuration_digest: profile.configuration_digest(),
            position_period_ticks,
            maximum_position_increment_bits,
            maximum_velocity_feed_forward_bits,
            maximum_quadrature_current_feed_forward_bits,
        })
    }

    /// Complete immutable configuration identity.
    pub const fn configuration_digest(self) -> Digest {
        self.configuration_digest
    }

    /// Exact position-loop cadence in device ticks.
    pub const fn position_period_ticks(self) -> u32 {
        self.position_period_ticks
    }

    /// Conservative exact Q31.32 progress bound per position update.
    pub const fn maximum_position_increment_bits(self) -> u64 {
        self.maximum_position_increment_bits
    }

    /// Symmetric normalized velocity feed-forward bound.
    pub const fn maximum_velocity_feed_forward_bits(self) -> u32 {
        self.maximum_velocity_feed_forward_bits
    }

    /// Symmetric q-current feed-forward bound inside the configured current circle.
    pub const fn maximum_quadrature_current_feed_forward_bits(self) -> u32 {
        self.maximum_quadrature_current_feed_forward_bits
    }
}

/// Complete common-cadence profile used by both cache validators and replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CachedServoAdmissionProfile<const AXES: usize> {
    /// Exact machine-IR limits for independent block admission.
    pub limits: ServoFiniteDifferenceBlockValidationLimits<AXES>,
    /// Identity and cadence copied into every runtime setpoint.
    pub setpoints: CachedServoSetpointProfile,
}

/// Derive common-cadence machine-IR bounds from complete configured FOC axes.
pub fn cached_servo_admission_profile<const AXES: usize>(
    axes: [ServoSetpointAxisAdmissionProfile; AXES],
    policy: CachedServoStreamPolicy,
) -> Result<CachedServoAdmissionProfile<AXES>, ServoSetpointAdmissionProfileError> {
    if AXES == 0 || AXES > MAX_SERVO_EXECUTION_AXES {
        return Err(ServoSetpointAdmissionProfileError::AxisCount);
    }
    if policy.maximum_block_ticks == 0
        || policy.maximum_segment_ticks == 0
        || policy.maximum_update_count == 0
        || policy.maximum_segment_ticks > policy.maximum_block_ticks
    {
        return Err(ServoSetpointAdmissionProfileError::Policy);
    }
    let configuration_digest = axes[0].configuration_digest;
    let position_period_ticks = axes[0].position_period_ticks;
    if configuration_digest.is_zero()
        || position_period_ticks == 0
        || axes.iter().any(|axis| {
            axis.configuration_digest != configuration_digest
                || axis.position_period_ticks != position_period_ticks
                || axis.maximum_position_increment_bits == 0
                || axis.maximum_velocity_feed_forward_bits == 0
        })
    {
        return Err(ServoSetpointAdmissionProfileError::CommonGrid);
    }
    let represented_ticks = u64::from(position_period_ticks)
        .checked_mul(u64::from(policy.maximum_update_count))
        .ok_or(ServoSetpointAdmissionProfileError::Arithmetic)?;
    if represented_ticks > policy.maximum_segment_ticks {
        return Err(ServoSetpointAdmissionProfileError::Policy);
    }

    let mut maximum_position_delta_bits = [0_u64; AXES];
    let mut maximum_absolute_position_first_difference_bits = [0_u64; AXES];
    let mut maximum_absolute_velocity_feed_forward_bits = [0_u32; AXES];
    let mut maximum_absolute_quadrature_current_feed_forward_bits = [0_u32; AXES];
    let mut axis = 0;
    while axis < AXES {
        let profile = axes[axis];
        maximum_position_delta_bits[axis] = profile
            .maximum_position_increment_bits
            .checked_mul(u64::from(policy.maximum_update_count))
            .ok_or(ServoSetpointAdmissionProfileError::Arithmetic)?;
        if maximum_position_delta_bits[axis] > i64::MAX as u64 {
            return Err(ServoSetpointAdmissionProfileError::Position);
        }
        maximum_absolute_position_first_difference_bits[axis] =
            profile.maximum_position_increment_bits;
        maximum_absolute_velocity_feed_forward_bits[axis] =
            profile.maximum_velocity_feed_forward_bits;
        maximum_absolute_quadrature_current_feed_forward_bits[axis] =
            profile.maximum_quadrature_current_feed_forward_bits;
        axis += 1;
    }
    Ok(CachedServoAdmissionProfile {
        limits: ServoFiniteDifferenceBlockValidationLimits {
            maximum_block_ticks: policy.maximum_block_ticks,
            segment: ServoFiniteDifferenceValidationLimits {
                maximum_segment_ticks: policy.maximum_segment_ticks,
                maximum_update_count: policy.maximum_update_count,
                required_update_period_ticks: position_period_ticks,
                maximum_position_delta_bits,
                maximum_absolute_position_first_difference_bits,
                maximum_absolute_velocity_feed_forward_bits,
                maximum_absolute_quadrature_current_feed_forward_bits,
            },
        },
        setpoints: CachedServoSetpointProfile {
            configuration_digest,
            update_period_ticks: position_period_ticks,
        },
    })
}

/// Rejection while deriving servo command-stream authority from FOC profiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoSetpointAdmissionProfileError {
    /// Compile-time axis width was unsupported.
    AxisCount,
    /// One complete portable FOC axis profile failed validation.
    FocProfile,
    /// Ownership horizon/count policy was zero or internally inconsistent.
    Policy,
    /// Axes selected different configuration identities or position-loop grids.
    CommonGrid,
    /// Normalized velocity authority was invalid.
    Velocity,
    /// Position-scale or per-update progress was invalid.
    Position,
    /// Direct/q-current authority did not define a usable current circle.
    Current,
    /// Checked exact rational or integer arithmetic overflowed.
    Arithmetic,
}

/// Immutable facts shared by every setpoint emitted by one runner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CachedServoSetpointProfile {
    /// Complete active configuration identity.
    pub configuration_digest: Digest,
    /// Exact position-loop cadence in local device ticks.
    pub update_period_ticks: u32,
}

impl CachedServoSetpointProfile {
    /// Reject absent configuration identity, cadence, or unsupported axis width.
    pub fn validate<const AXES: usize>(self) -> Result<(), CachedServoSetpointError> {
        if AXES == 0 || AXES > MAX_SERVO_EXECUTION_AXES {
            return Err(CachedServoSetpointError::AxisCount);
        }
        if self.configuration_digest.is_zero() || self.update_period_ticks == 0 {
            return Err(CachedServoSetpointError::Profile);
        }
        Ok(())
    }
}

/// Opaque one-shot identity for a prepared simultaneous setpoint transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct ServoSetpointCommitToken(u64);

/// One exact simultaneous setpoint batch prepared for all configured axes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedServoSetpoints<const AXES: usize> {
    token: ServoSetpointCommitToken,
    setpoints: [ServoSetpoint; AXES],
}

impl<const AXES: usize> PreparedServoSetpoints<AXES> {
    /// One-shot identity required to commit this exact batch.
    pub const fn token(self) -> ServoSetpointCommitToken {
        self.token
    }

    /// Exact position-loop boundary shared by every axis.
    pub const fn scheduled_at(self) -> DeviceCycle {
        self.setpoints[0].scheduled_at
    }

    /// Contiguous nonzero command identity shared by every axis.
    pub const fn command_id(self) -> u32 {
        self.setpoints[0].command_id
    }

    /// Borrow the complete simultaneous axis vector.
    pub const fn setpoints(&self) -> &[ServoSetpoint; AXES] {
        &self.setpoints
    }
}

/// Result of asking the runner for the next exact position-loop transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoSetpointPlan<const AXES: usize> {
    /// One simultaneous batch is ready for the physical control owner.
    Setpoints(PreparedServoSetpoints<AXES>),
    /// A nonterminal block reached its exact horizon without its successor.
    NeedBlock { scheduled_at: DeviceCycle },
    /// A completed block must return to the job actor before replay continues.
    CompletionPending,
    /// The terminal at-rest hold was committed and no work remains.
    Complete,
}

/// Facts retained after the physical control owner commits one exact batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommittedServoSetpoints {
    /// Contiguous command identity that was applied.
    pub command_id: u32,
    /// Exact position-loop boundary at which it was applied.
    pub applied_at: DeviceCycle,
    /// Whether this batch installed the complete stream's terminal at-rest hold.
    pub terminal_hold: bool,
    /// Whether this batch made an older block available for acknowledgement.
    pub block_completed: bool,
}

/// Portable cached-servo replay failure. The first failure latches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CachedServoSetpointError {
    /// Compile-time axis width was zero or above the servo schema limit.
    AxisCount,
    /// Configuration identity or cadence was absent.
    Profile,
    /// A block selected another execution family.
    Kind,
    /// A block selected another active configuration or cadence.
    Configuration,
    /// Sequence, tick, or digest continuity disagreed with the owned predecessor.
    Chain,
    /// The fixed two-block ownership window was full.
    Capacity,
    /// A canonical record could not be decoded.
    Decode(BlockError),
    /// No operation is legal in the current lifecycle state.
    State,
    /// A commit did not identify the sole prepared transition.
    Token,
    /// A transition was observed before or after its exact position-loop boundary.
    Deadline {
        /// Required local cycle.
        scheduled: DeviceCycle,
        /// Observed local cycle.
        received: DeviceCycle,
    },
    /// Checked tick, command, or token arithmetic overflowed.
    Arithmetic,
    /// A prior failure already latched.
    FaultLatched,
}

/// One rejected block whose unique ownership is returned unchanged.
pub struct RejectedServoSetpointBlock<const AXES: usize> {
    error: CachedServoSetpointError,
    admitted: AdmittedBlock<AXES>,
}

impl<const AXES: usize> RejectedServoSetpointBlock<AXES> {
    /// Exact rejection cause.
    pub const fn error(&self) -> CachedServoSetpointError {
        self.error
    }

    /// Recover the unchanged job-actor token.
    pub fn into_block(self) -> AdmittedBlock<AXES> {
        self.admitted
    }
}

impl<const AXES: usize> core::fmt::Debug for RejectedServoSetpointBlock<AXES> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("RejectedServoSetpointBlock")
            .field("error", &self.error)
            .field("header", &self.admitted.header())
            .finish()
    }
}

struct CurrentServoBlock<const AXES: usize> {
    admitted: AdmittedBlock<AXES>,
    segment_index: u32,
    segment: ServoFiniteDifferenceSegment<AXES>,
    update_index: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PreparedTransition<const AXES: usize> {
    CurrentUpdate,
    LookaheadBoundary {
        first_segment: ServoFiniteDifferenceSegment<AXES>,
    },
    TerminalHold,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RetainedPreparation<const AXES: usize> {
    batch: PreparedServoSetpoints<AXES>,
    transition: PreparedTransition<AXES>,
}

/// Allocation-free two-block servo recurrence owner for one real-time core.
///
/// A record is half-open: updates `0..update_count` are emitted. The next
/// record supplies the shared terminal state at its update zero. The complete
/// stream receives one additional terminal at-rest hold at its exclusive end
/// tick, so the physical controller reaches the exact declared endpoint.
pub struct CachedServoSetpointRunner<const AXES: usize> {
    profile: CachedServoSetpointProfile,
    epoch: DeviceCycle,
    current: Option<CurrentServoBlock<AXES>>,
    lookahead: Option<AdmittedBlock<AXES>>,
    completed: Option<AdmittedBlock<AXES>>,
    prepared: Option<RetainedPreparation<AXES>>,
    next_command_id: u32,
    next_token: u64,
    committed_setpoints: u64,
    last_applied_at: Option<DeviceCycle>,
    finished: bool,
    fault: Option<CachedServoSetpointError>,
}

impl<const AXES: usize> CachedServoSetpointRunner<AXES> {
    /// Construct an inert runner at the committed absolute device epoch.
    pub fn new(
        profile: CachedServoSetpointProfile,
        epoch: DeviceCycle,
    ) -> Result<Self, CachedServoSetpointError> {
        profile.validate::<AXES>()?;
        Ok(Self {
            profile,
            epoch,
            current: None,
            lookahead: None,
            completed: None,
            prepared: None,
            next_command_id: 1,
            next_token: 1,
            committed_setpoints: 0,
            last_applied_at: None,
            finished: false,
            fault: None,
        })
    }

    /// Admit one independently validated block into the fixed two-block window.
    #[allow(
        clippy::result_large_err,
        reason = "rejection must preserve the complete unique inline block"
    )]
    pub fn admit_block(
        &mut self,
        admitted: AdmittedBlock<AXES>,
    ) -> Result<(), RejectedServoSetpointBlock<AXES>> {
        if self.fault.is_some() || self.finished {
            return Err(RejectedServoSetpointBlock {
                error: CachedServoSetpointError::State,
                admitted,
            });
        }
        if let Err(error) = self.validate_block_identity(&admitted) {
            return Err(RejectedServoSetpointBlock { error, admitted });
        }
        if self.current.is_none() {
            match current_from_block(admitted) {
                Ok(current) => {
                    self.current = Some(current);
                    Ok(())
                }
                Err(rejected) => Err(rejected),
            }
        } else if self.lookahead.is_none() && self.completed.is_none() {
            let current = self.current.as_ref().expect("current checked");
            let current_header = current.admitted.header();
            let received = admitted.header();
            let expected_sequence = match current_header.sequence.checked_add(1) {
                Some(sequence) => sequence,
                None => {
                    return Err(RejectedServoSetpointBlock {
                        error: CachedServoSetpointError::Arithmetic,
                        admitted,
                    });
                }
            };
            if received.sequence != expected_sequence
                || received.start_tick != current_header.end_tick
                || received.previous_digest != current_header.block_digest
            {
                return Err(RejectedServoSetpointBlock {
                    error: CachedServoSetpointError::Chain,
                    admitted,
                });
            }
            self.lookahead = Some(admitted);
            Ok(())
        } else {
            Err(RejectedServoSetpointBlock {
                error: CachedServoSetpointError::Capacity,
                admitted,
            })
        }
    }

    /// Prepare or repeat the sole next exact simultaneous setpoint batch.
    pub fn prepare_next(&mut self) -> Result<ServoSetpointPlan<AXES>, CachedServoSetpointError> {
        if self.fault.is_some() {
            return Err(CachedServoSetpointError::FaultLatched);
        }
        if let Some(prepared) = self.prepared {
            return Ok(ServoSetpointPlan::Setpoints(prepared.batch));
        }
        if self.completed.is_some() {
            return Ok(ServoSetpointPlan::CompletionPending);
        }
        if self.finished {
            return Ok(ServoSetpointPlan::Complete);
        }
        let Some(current) = self.current.as_mut() else {
            return Ok(ServoSetpointPlan::NeedBlock {
                scheduled_at: self.epoch,
            });
        };

        while current.update_index == current.segment.update_count
            && current.segment_index + 1 < current.admitted.header().segment_count
        {
            let next_index = current
                .segment_index
                .checked_add(1)
                .ok_or(CachedServoSetpointError::Arithmetic)?;
            current.segment = segment_at(&current.admitted, next_index)
                .map_err(CachedServoSetpointError::Decode)?
                .ok_or(CachedServoSetpointError::State)?;
            current.segment_index = next_index;
            current.update_index = 0;
        }

        let transition;
        let segment;
        let update;
        if current.update_index < current.segment.update_count {
            transition = PreparedTransition::CurrentUpdate;
            segment = current.segment;
            update = current.update_index;
        } else if current.admitted.progress().complete {
            transition = PreparedTransition::TerminalHold;
            segment = current.segment;
            update = current.segment.update_count;
        } else {
            let Some(lookahead) = self.lookahead.as_ref() else {
                let scheduled_at = absolute_tick(self.epoch, current.segment.end_tick)?;
                return Ok(ServoSetpointPlan::NeedBlock { scheduled_at });
            };
            let first_segment = segment_at(lookahead, 0)
                .map_err(CachedServoSetpointError::Decode)?
                .ok_or(CachedServoSetpointError::State)?;
            transition = PreparedTransition::LookaheadBoundary { first_segment };
            segment = first_segment;
            update = 0;
        }

        let state = segment
            .state_at(update)
            .map_err(|_| CachedServoSetpointError::State)?;
        let offset = u64::from(segment.update_period_ticks)
            .checked_mul(u64::from(update))
            .ok_or(CachedServoSetpointError::Arithmetic)?;
        let relative = segment
            .start_tick
            .0
            .checked_add(offset)
            .ok_or(CachedServoSetpointError::Arithmetic)?;
        let scheduled_at = absolute_tick(self.epoch, StreamTick(relative))?;
        let command_id = self.next_command_id;
        let setpoints = core::array::from_fn(|axis| ServoSetpoint {
            command_id,
            scheduled_at,
            configuration_digest: self.profile.configuration_digest,
            position: ServoPosition::from_bits(state.position[axis]),
            velocity_feed_forward: Q30::from_bits(state.velocity_feed_forward[axis]),
            quadrature_current_feed_forward: Q30::from_bits(
                state.quadrature_current_feed_forward[axis],
            ),
        });
        let retained = RetainedPreparation {
            batch: PreparedServoSetpoints {
                token: ServoSetpointCommitToken(self.next_token),
                setpoints,
            },
            transition,
        };
        self.prepared = Some(retained);
        Ok(ServoSetpointPlan::Setpoints(retained.batch))
    }

    /// Commit the prepared batch only at its exact physical position-loop boundary.
    pub fn commit(
        &mut self,
        token: ServoSetpointCommitToken,
        applied_at: DeviceCycle,
    ) -> Result<CommittedServoSetpoints, CachedServoSetpointError> {
        if self.fault.is_some() {
            return Err(CachedServoSetpointError::FaultLatched);
        }
        let Some(prepared) = self.prepared else {
            return self.latch(CachedServoSetpointError::State);
        };
        if token != prepared.batch.token {
            return self.latch(CachedServoSetpointError::Token);
        }
        let scheduled = prepared.batch.scheduled_at();
        if applied_at != scheduled {
            return self.latch(CachedServoSetpointError::Deadline {
                scheduled,
                received: applied_at,
            });
        }
        if self
            .last_applied_at
            .is_some_and(|previous| applied_at <= previous)
        {
            return self.latch(CachedServoSetpointError::State);
        }

        let terminal_hold = prepared.transition == PreparedTransition::TerminalHold;
        let committed_setpoints = match self.committed_setpoints.checked_add(1) {
            Some(count) => count,
            None => return self.latch(CachedServoSetpointError::Arithmetic),
        };
        let (next_command_id, next_token) = if terminal_hold {
            (self.next_command_id, self.next_token)
        } else {
            let Some(command_id) = self.next_command_id.checked_add(1) else {
                return self.latch(CachedServoSetpointError::Arithmetic);
            };
            let Some(token) = self.next_token.checked_add(1) else {
                return self.latch(CachedServoSetpointError::Arithmetic);
            };
            (command_id, token)
        };
        let advanced_update_index = match prepared.transition {
            PreparedTransition::CurrentUpdate => {
                let Some(current) = self.current.as_ref() else {
                    return self.latch(CachedServoSetpointError::State);
                };
                let Some(index) = current.update_index.checked_add(1) else {
                    return self.latch(CachedServoSetpointError::Arithmetic);
                };
                Some(index)
            }
            PreparedTransition::LookaheadBoundary { .. } => {
                if self.current.is_none() || self.lookahead.is_none() || self.completed.is_some() {
                    return self.latch(CachedServoSetpointError::State);
                }
                None
            }
            PreparedTransition::TerminalHold => {
                if self.current.is_none() || self.completed.is_some() {
                    return self.latch(CachedServoSetpointError::State);
                }
                None
            }
        };

        let mut block_completed = false;
        match prepared.transition {
            PreparedTransition::CurrentUpdate => {
                let Some(advanced_update_index) = advanced_update_index else {
                    return self.latch(CachedServoSetpointError::State);
                };
                let Some(current) = self.current.as_mut() else {
                    return self.latch(CachedServoSetpointError::State);
                };
                current.update_index = advanced_update_index;
            }
            PreparedTransition::LookaheadBoundary { first_segment } => {
                let Some(completed) = self.current.take() else {
                    return self.latch(CachedServoSetpointError::State);
                };
                let Some(next) = self.lookahead.take() else {
                    self.current = Some(completed);
                    return self.latch(CachedServoSetpointError::State);
                };
                self.completed = Some(completed.admitted);
                self.current = Some(CurrentServoBlock {
                    admitted: next,
                    segment_index: 0,
                    segment: first_segment,
                    update_index: 1,
                });
                block_completed = true;
            }
            PreparedTransition::TerminalHold => {
                let Some(completed) = self.current.take() else {
                    return self.latch(CachedServoSetpointError::State);
                };
                self.completed = Some(completed.admitted);
                self.finished = true;
                block_completed = true;
            }
        }

        self.prepared = None;
        self.committed_setpoints = committed_setpoints;
        self.last_applied_at = Some(applied_at);
        if !terminal_hold {
            self.next_command_id = next_command_id;
            self.next_token = next_token;
        }
        Ok(CommittedServoSetpoints {
            command_id: prepared.batch.command_id(),
            applied_at,
            terminal_hold,
            block_completed,
        })
    }

    /// Return the oldest block after its continuation/terminal setpoint was applied.
    pub fn take_completed_block(&mut self) -> Option<AdmittedBlock<AXES>> {
        if self.fault.is_none() {
            self.completed.take()
        } else {
            None
        }
    }

    /// Latch a local hardware/control failure and invalidate every prepared batch.
    pub fn fault(&mut self) {
        if self.fault.is_none() {
            self.fault = Some(CachedServoSetpointError::State);
        }
        self.prepared = None;
    }

    /// Recover retained blocks one at a time after the physical owner is safe.
    pub fn take_faulted_block(&mut self) -> Option<AdmittedBlock<AXES>> {
        self.fault?;
        self.completed.take().or_else(|| {
            self.current
                .take()
                .map(|current| current.admitted)
                .or_else(|| self.lookahead.take())
        })
    }

    /// First latched failure, if any.
    pub const fn fault_reason(&self) -> Option<CachedServoSetpointError> {
        self.fault
    }

    /// Number of simultaneous setpoint batches physically applied.
    pub const fn committed_setpoints(&self) -> u64 {
        self.committed_setpoints
    }

    /// Whether the terminal at-rest hold was applied.
    pub const fn is_complete(&self) -> bool {
        self.finished && self.completed.is_none()
    }

    /// Absolute boundary for the next prepared or unprepared setpoint.
    pub fn next_deadline(&self) -> Option<DeviceCycle> {
        if self.fault.is_some() || self.finished {
            return None;
        }
        if let Some(prepared) = self.prepared {
            return Some(prepared.batch.scheduled_at());
        }
        let current = self.current.as_ref()?;
        if current.update_index < current.segment.update_count {
            let offset = u64::from(current.segment.update_period_ticks)
                .checked_mul(u64::from(current.update_index))?;
            absolute_tick(
                self.epoch,
                StreamTick(current.segment.start_tick.0.checked_add(offset)?),
            )
            .ok()
        } else {
            absolute_tick(self.epoch, current.segment.end_tick).ok()
        }
    }

    fn validate_block_identity(
        &self,
        admitted: &AdmittedBlock<AXES>,
    ) -> Result<(), CachedServoSetpointError> {
        let header = admitted.header();
        if header.kind != ExecutionKind::ServoFiniteDifference {
            return Err(CachedServoSetpointError::Kind);
        }
        if header.config_digest != self.profile.configuration_digest {
            return Err(CachedServoSetpointError::Configuration);
        }
        let mut segments = admitted
            .servo_finite_difference_segments()
            .map_err(CachedServoSetpointError::Decode)?;
        if segments.any(|segment| segment.update_period_ticks != self.profile.update_period_ticks) {
            return Err(CachedServoSetpointError::Configuration);
        }
        Ok(())
    }

    fn latch<T>(&mut self, error: CachedServoSetpointError) -> Result<T, CachedServoSetpointError> {
        self.fault = Some(error);
        self.prepared = None;
        Err(error)
    }
}

#[allow(
    clippy::result_large_err,
    reason = "initial decode rejection must preserve the complete unique inline block"
)]
fn current_from_block<const AXES: usize>(
    admitted: AdmittedBlock<AXES>,
) -> Result<CurrentServoBlock<AXES>, RejectedServoSetpointBlock<AXES>> {
    let segment = match segment_at(&admitted, 0) {
        Ok(Some(segment)) => segment,
        Ok(None) => {
            return Err(RejectedServoSetpointBlock {
                error: CachedServoSetpointError::State,
                admitted,
            });
        }
        Err(error) => {
            return Err(RejectedServoSetpointBlock {
                error: CachedServoSetpointError::Decode(error),
                admitted,
            });
        }
    };
    Ok(CurrentServoBlock {
        admitted,
        segment_index: 0,
        segment,
        update_index: 0,
    })
}

fn segment_at<const AXES: usize>(
    admitted: &AdmittedBlock<AXES>,
    index: u32,
) -> Result<Option<ServoFiniteDifferenceSegment<AXES>>, BlockError> {
    let mut segments = admitted.servo_finite_difference_segments()?;
    Ok(segments.nth(usize::try_from(index).map_err(|_| BlockError::Arithmetic)?))
}

fn absolute_tick(
    epoch: DeviceCycle,
    relative: StreamTick,
) -> Result<DeviceCycle, CachedServoSetpointError> {
    epoch
        .0
        .checked_add(relative.0)
        .map(DeviceCycle)
        .ok_or(CachedServoSetpointError::Arithmetic)
}

fn maximum_position_increment_bits(
    maximum_velocity_bits: i32,
    scale: alumina_foc::ServoEncoderScale,
    counts_per_mechanical_turn: u32,
    device_cycle_hz: u32,
    position_period_cycles: u64,
) -> Result<u64, ServoSetpointAdmissionProfileError> {
    let velocity = u128::try_from(maximum_velocity_bits)
        .map_err(|_| ServoSetpointAdmissionProfileError::Velocity)?;
    if velocity == 0 || counts_per_mechanical_turn == 0 || device_cycle_hz == 0 {
        return Err(ServoSetpointAdmissionProfileError::Position);
    }
    let numerator = velocity
        .checked_mul(u128::from(
            scale.counts_per_second_at_velocity_one_numerator(),
        ))
        .and_then(|value| value.checked_mul(u128::from(scale.position_bits_per_turn_numerator())))
        .and_then(|value| value.checked_mul(u128::from(position_period_cycles)))
        .ok_or(ServoSetpointAdmissionProfileError::Arithmetic)?;
    let denominator = u128::try_from(Q30_SCALE)
        .ok()
        .and_then(|value| {
            value.checked_mul(u128::from(
                scale.counts_per_second_at_velocity_one_denominator(),
            ))
        })
        .and_then(|value| value.checked_mul(u128::from(counts_per_mechanical_turn)))
        .and_then(|value| value.checked_mul(u128::from(scale.position_bits_per_turn_denominator())))
        .and_then(|value| value.checked_mul(u128::from(device_cycle_hz)))
        .ok_or(ServoSetpointAdmissionProfileError::Arithmetic)?;
    let increment = div_ceil_u128(numerator, denominator)
        .ok_or(ServoSetpointAdmissionProfileError::Arithmetic)?;
    let increment =
        u64::try_from(increment).map_err(|_| ServoSetpointAdmissionProfileError::Arithmetic)?;
    if increment == 0 || increment > i64::MAX as u64 {
        return Err(ServoSetpointAdmissionProfileError::Position);
    }
    Ok(increment)
}

fn current_circle_quadrature_limit(
    maximum_current_bits: i32,
    direct_current_bits: i32,
) -> Result<u32, ServoSetpointAdmissionProfileError> {
    let maximum = i128::from(maximum_current_bits);
    let direct = i128::from(direct_current_bits);
    if maximum <= 0 || direct.unsigned_abs() > maximum as u128 {
        return Err(ServoSetpointAdmissionProfileError::Current);
    }
    let radicand = maximum
        .checked_mul(maximum)
        .and_then(|square| square.checked_sub(direct.checked_mul(direct)?))
        .and_then(|value| u128::try_from(value).ok())
        .ok_or(ServoSetpointAdmissionProfileError::Arithmetic)?;
    u32::try_from(integer_square_root(radicand))
        .map_err(|_| ServoSetpointAdmissionProfileError::Arithmetic)
}

fn div_ceil_u128(numerator: u128, denominator: u128) -> Option<u128> {
    if denominator == 0 {
        return None;
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    quotient.checked_add(u128::from(remainder != 0))
}

fn integer_square_root(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut low = 1_u128;
    let mut high = 1_u128 << 64;
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if middle <= value / middle {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

#[cfg(test)]
mod tests {
    use super::*;
    use alumina_job::{JobDescriptor, RealtimeJob, RealtimeJobState, RealtimePoll, WorkSource};
    use alumina_machine_ir::{
        BlockValidationLimits, ExecutionBlock, FINITE_DIFFERENCE_ONE_STEP, FiniteDifferenceAxis,
        ServoFiniteDifferenceAxis, ServoFiniteDifferenceBlockValidationLimits,
        ServoFiniteDifferenceSegment, ServoFiniteDifferenceValidationLimits,
        ServoQ30FiniteDifferenceAxis, StreamId, ValidationLimits,
    };
    use alumina_storage::{ContentId, ObjectKind, PublishedObject, StoredObject};

    const POSITION_ONE: i64 = FINITE_DIFFERENCE_ONE_STEP;

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

    fn digest(byte: u8) -> Digest {
        Digest([byte; 32])
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

    fn descriptor() -> JobDescriptor {
        JobDescriptor {
            prepare_id: 41,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId::from_sha256(digest(0x91)),
                    byte_len: 1_024,
                },
                manifest: ContentId::from_sha256(digest(0x92)),
            },
            stream_id: StreamId::new([0x93; 16]).unwrap(),
            capability_digest: digest(0x94),
            config_digest: digest(0x95),
            axis_count: 2,
            execution_kind: ExecutionKind::ServoFiniteDifference,
            maximum_dense_updates: 100,
            dense_update_period_ticks: 10,
            block_count: 2,
            first_tick: StreamTick(0),
            initial_position: [0, 10 * POSITION_ONE, 0, 0, 0, 0, 0, 0],
            limits: BlockValidationLimits {
                maximum_block_ticks: 1_000,
                segment: ValidationLimits {
                    maximum_segment_ticks: 1_000,
                    maximum_steps_per_segment: 10 * POSITION_ONE as u64,
                },
            },
        }
    }

    fn blocks() -> Blocks {
        let descriptor = descriptor();
        let first = ExecutionBlock::encode_servo_finite_difference(
            descriptor.stream_id,
            descriptor.capability_digest,
            descriptor.config_digest,
            0,
            Digest::ZERO,
            &[first_segment()],
        )
        .unwrap();
        let second = ExecutionBlock::encode_servo_finite_difference(
            descriptor.stream_id,
            descriptor.capability_digest,
            descriptor.config_digest,
            1,
            first.header().block_digest,
            &[second_segment()],
        )
        .unwrap();
        Blocks {
            first: Some(first),
            second: Some(second),
        }
    }

    fn poll_block(job: &mut RealtimeJob<2>, source: &mut Blocks) -> AdmittedBlock<2> {
        match job.poll(source).unwrap() {
            RealtimePoll::Block(block) => block,
            RealtimePoll::Empty | RealtimePoll::Outstanding => panic!("block must be available"),
        }
    }

    fn prepared(runner: &mut CachedServoSetpointRunner<2>) -> PreparedServoSetpoints<2> {
        match runner.prepare_next().unwrap() {
            ServoSetpointPlan::Setpoints(prepared) => prepared,
            ServoSetpointPlan::NeedBlock { .. }
            | ServoSetpointPlan::CompletionPending
            | ServoSetpointPlan::Complete => panic!("setpoint must be ready"),
        }
    }

    #[test]
    fn exact_profile_math_rounds_position_outward_and_current_inward() {
        let scale = alumina_foc::ServoEncoderScale::new(4_294_967_296, 1, 4_096, 1).unwrap();
        assert_eq!(
            maximum_position_increment_bits(Q30::ONE.bits(), scale, 4_096, 1_000_000, 1_000,),
            Ok(4_294_968)
        );

        let maximum = Q30::ONE.bits();
        let direct = Q30::from_bits(3 * (1 << 28)).bits();
        let quadrature = current_circle_quadrature_limit(maximum, direct).unwrap();
        let maximum_square = i128::from(maximum) * i128::from(maximum);
        let admitted_square = i128::from(direct) * i128::from(direct)
            + i128::from(quadrature) * i128::from(quadrature);
        assert!(admitted_square <= maximum_square);
        let next = i128::from(quadrature) + 1;
        assert!(i128::from(direct) * i128::from(direct) + next * next > maximum_square);
    }

    #[test]
    fn common_axis_profiles_produce_descriptor_bound_limits() {
        let axis = ServoSetpointAxisAdmissionProfile {
            configuration_digest: digest(0x95),
            position_period_ticks: 10,
            maximum_position_increment_bits: 25,
            maximum_velocity_feed_forward_bits: 500,
            maximum_quadrature_current_feed_forward_bits: 400,
        };
        let policy = CachedServoStreamPolicy {
            maximum_block_ticks: 2_000,
            maximum_segment_ticks: 1_000,
            maximum_update_count: 100,
        };
        let profile = cached_servo_admission_profile([axis, axis], policy).unwrap();
        assert_eq!(profile.setpoints.configuration_digest, digest(0x95));
        assert_eq!(profile.setpoints.update_period_ticks, 10);
        assert_eq!(
            profile.limits.segment.maximum_position_delta_bits,
            [2_500; 2]
        );
        assert_eq!(
            profile
                .limits
                .segment
                .maximum_absolute_position_first_difference_bits,
            [25; 2]
        );

        let mut mismatched = axis;
        mismatched.position_period_ticks = 20;
        assert_eq!(
            cached_servo_admission_profile([axis, mismatched], policy),
            Err(ServoSetpointAdmissionProfileError::CommonGrid)
        );
    }

    #[test]
    fn two_blocks_emit_half_open_updates_and_one_terminal_hold() {
        let descriptor = descriptor();
        let mut source = blocks();
        let mut job = RealtimeJob::<2>::prepare_servo(descriptor, limits()).unwrap();
        let first = poll_block(&mut job, &mut source);
        let mut runner = CachedServoSetpointRunner::new(
            CachedServoSetpointProfile {
                configuration_digest: descriptor.config_digest,
                update_period_ticks: 10,
            },
            DeviceCycle(1_000),
        )
        .unwrap();
        runner.admit_block(first).unwrap();

        for index in 0..4_u32 {
            let prepared = prepared(&mut runner);
            assert_eq!(prepared.command_id(), index + 1);
            assert_eq!(
                prepared.scheduled_at(),
                DeviceCycle(1_000 + u64::from(index) * 10)
            );
            assert_eq!(
                prepared.setpoints()[0].position,
                ServoPosition::from_bits(i64::from(index) * POSITION_ONE / 4)
            );
            assert_eq!(
                prepared.setpoints()[0].velocity_feed_forward,
                Q30::from_bits(i32::try_from(index).unwrap() * 100)
            );
            let committed = runner
                .commit(prepared.token(), prepared.scheduled_at())
                .unwrap();
            assert!(!committed.block_completed);
        }
        assert_eq!(
            runner.prepare_next(),
            Ok(ServoSetpointPlan::NeedBlock {
                scheduled_at: DeviceCycle(1_040)
            })
        );

        let second = poll_block(&mut job, &mut source);
        runner.admit_block(second).unwrap();
        let boundary = prepared(&mut runner);
        assert_eq!(boundary.command_id(), 5);
        assert_eq!(boundary.scheduled_at(), DeviceCycle(1_040));
        assert_eq!(
            boundary.setpoints()[0].position,
            ServoPosition::from_bits(POSITION_ONE)
        );
        assert_eq!(
            boundary.setpoints()[0].velocity_feed_forward,
            Q30::from_bits(400)
        );
        assert!(
            runner
                .commit(boundary.token(), boundary.scheduled_at())
                .unwrap()
                .block_completed
        );
        assert_eq!(
            runner.prepare_next(),
            Ok(ServoSetpointPlan::CompletionPending)
        );
        let completed_first = runner.take_completed_block().unwrap();
        assert_eq!(completed_first.header().sequence, 0);
        job.acknowledge(completed_first).unwrap();

        for index in 1..4_u32 {
            let prepared = prepared(&mut runner);
            assert_eq!(prepared.command_id(), index + 5);
            assert_eq!(
                prepared.scheduled_at(),
                DeviceCycle(1_040 + u64::from(index) * 10)
            );
            runner
                .commit(prepared.token(), prepared.scheduled_at())
                .unwrap();
        }
        let terminal = prepared(&mut runner);
        assert_eq!(terminal.command_id(), 9);
        assert_eq!(terminal.scheduled_at(), DeviceCycle(1_080));
        assert_eq!(
            terminal.setpoints()[0].position,
            ServoPosition::from_bits(2 * POSITION_ONE)
        );
        assert_eq!(terminal.setpoints()[0].velocity_feed_forward, Q30::ZERO);
        assert_eq!(
            terminal.setpoints()[0].quadrature_current_feed_forward,
            Q30::ZERO
        );
        let committed = runner
            .commit(terminal.token(), terminal.scheduled_at())
            .unwrap();
        assert!(committed.terminal_hold);
        assert!(committed.block_completed);
        let completed_second = runner.take_completed_block().unwrap();
        assert_eq!(completed_second.header().sequence, 1);
        assert_eq!(
            job.acknowledge(completed_second).unwrap().state,
            RealtimeJobState::Complete
        );
        assert_eq!(runner.prepare_next(), Ok(ServoSetpointPlan::Complete));
        assert!(runner.is_complete());
        assert_eq!(runner.committed_setpoints(), 9);
    }

    #[test]
    fn wrong_physical_boundary_latches_and_returns_unique_block() {
        let mut descriptor = descriptor();
        descriptor.block_count = 1;
        descriptor.partition.object.byte_len = 512;
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
            descriptor.stream_id,
            descriptor.capability_digest,
            descriptor.config_digest,
            0,
            Digest::ZERO,
            &[segment],
        )
        .unwrap();
        let mut source = Blocks {
            first: Some(block),
            second: None,
        };
        let mut job = RealtimeJob::<2>::prepare_servo(descriptor, limits()).unwrap();
        let admitted = poll_block(&mut job, &mut source);
        let mut runner = CachedServoSetpointRunner::new(
            CachedServoSetpointProfile {
                configuration_digest: descriptor.config_digest,
                update_period_ticks: 10,
            },
            DeviceCycle(500),
        )
        .unwrap();
        runner.admit_block(admitted).unwrap();
        let prepared = prepared(&mut runner);
        assert_eq!(
            runner.commit(prepared.token(), DeviceCycle(501)),
            Err(CachedServoSetpointError::Deadline {
                scheduled: DeviceCycle(500),
                received: DeviceCycle(501)
            })
        );
        assert_eq!(
            runner.prepare_next(),
            Err(CachedServoSetpointError::FaultLatched)
        );
        assert_eq!(runner.take_faulted_block().unwrap().header().sequence, 0);
        assert!(runner.take_faulted_block().is_none());
    }
}
