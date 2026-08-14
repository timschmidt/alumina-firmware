//! Fail-closed ownership of an aggregate physical PWM commit backend.

use core::convert::Infallible;

use alumina_protocol::{DeviceCycle, Digest};

use crate::{
    PowerStageCommit, PwmCommitBankBarrier, PwmCommitBankCompletion, PwmCommitBarrierError,
    PwmCompareImage,
};

/// Aggregate target contract for one complete physical PWM stage bank.
///
/// A backend may contain heterogeneous peripheral types. `stage_images` must
/// accept the entire vector into inactive/shadow state or return an error; an
/// error makes physical state uncertain. `take_latch` must report only truthful
/// hardware status. `force_safe` owns the independently qualified all-stage
/// safe transaction and must not rely on a later logical publication.
pub trait PwmCommitBankHardware<const AXES: usize> {
    type Error;

    /// Stages one complete vector without claiming that its boundary occurred.
    fn stage_images(&mut self, images: &[PwmCompareImage; AXES]) -> Result<(), Self::Error>;

    /// Takes at most one truthful physical latch witness for an axis.
    fn take_latch(&mut self, axis: usize) -> Result<Option<PowerStageCommit>, Self::Error>;

    /// Applies the complete independently qualified all-stage safe transaction.
    fn force_safe(&mut self) -> Result<(), Self::Error>;
}

/// Coarse hardware operation retained when a backend call fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PwmCommitBankHardwareOperation {
    Stage,
    Observe { axis: usize },
}

/// First terminal cause retained independently of backend error ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PwmCommitBankOwnerFault {
    Barrier(PwmCommitBarrierError),
    Hardware(PwmCommitBankHardwareOperation),
    Publication,
    ExternalSafety,
}

/// Closed ownership state of the aggregate target seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PwmCommitBankOwnerState {
    /// No image bank is staged; exactly the barrier's next boundary may begin.
    Ready,
    /// One complete image vector awaits physical latch reports/publication.
    AwaitingLatches,
    /// The first fault is retained and the all-stage safe transaction succeeded.
    SafeFault,
    /// The first fault is retained but the all-stage safe transaction failed.
    UnsafeFault,
}

/// Construction rejection which returns unique aggregate hardware ownership.
#[derive(Debug, Eq, PartialEq)]
pub struct PwmCommitBankOwnerBuildError<Hardware> {
    hardware: Hardware,
    error: PwmCommitBarrierError,
}

impl<Hardware> PwmCommitBankOwnerBuildError<Hardware> {
    /// Portable barrier construction cause.
    pub const fn error(&self) -> PwmCommitBarrierError {
        self.error
    }

    /// Returns the never-staged hardware value and exact rejection.
    pub fn into_parts(self) -> (Hardware, PwmCommitBarrierError) {
        (self.hardware, self.error)
    }
}

/// Stage, observation, barrier, or logical-publication rejection.
///
/// `safe_error` is present only when the automatic all-stage safe transaction
/// also failed. In that case the owner enters [`PwmCommitBankOwnerState::UnsafeFault`]
/// and [`PwmCommitBankTargetOwner::force_safe`] is the sole retry operation.
#[derive(Debug, Eq, PartialEq)]
pub enum PwmCommitBankOwnerError<HardwareError, PublicationError = Infallible> {
    /// The owner was already terminally closed.
    Closed { state: PwmCommitBankOwnerState },
    /// Exact barrier admission/correlation failed.
    Barrier {
        error: PwmCommitBarrierError,
        safe_error: Option<HardwareError>,
    },
    /// Aggregate backend staging or observation failed.
    Hardware {
        operation: PwmCommitBankHardwareOperation,
        error: HardwareError,
        safe_error: Option<HardwareError>,
    },
    /// Physical completion succeeded but transactional logical publication failed.
    Publication {
        error: PublicationError,
        safe_error: Option<HardwareError>,
    },
}

/// Sole fail-closed owner joining a physical PWM bank to its commit barrier.
///
/// The hardware value is private and no mutable escape hatch exists. Every
/// terminal failure automatically attempts `force_safe`; the only nonterminal
/// rejection is staging while the existing complete vector is still pending.
#[derive(Debug, Eq, PartialEq)]
pub struct PwmCommitBankTargetOwner<Hardware, const AXES: usize>
where
    Hardware: PwmCommitBankHardware<AXES>,
{
    hardware: Hardware,
    barrier: PwmCommitBankBarrier<AXES>,
    state: PwmCommitBankOwnerState,
    fault: Option<PwmCommitBankOwnerFault>,
}

impl<Hardware, const AXES: usize> PwmCommitBankTargetOwner<Hardware, AXES>
where
    Hardware: PwmCommitBankHardware<AXES>,
{
    /// Retains unique hardware only after the portable barrier validates.
    pub fn new(
        hardware: Hardware,
        configuration_digest: Digest,
        pwm_period_cycles: u32,
        next_boundary: DeviceCycle,
        next_period_sequence: u64,
    ) -> Result<Self, PwmCommitBankOwnerBuildError<Hardware>> {
        let barrier = match PwmCommitBankBarrier::new(
            configuration_digest,
            pwm_period_cycles,
            next_boundary,
            next_period_sequence,
        ) {
            Ok(barrier) => barrier,
            Err(error) => return Err(PwmCommitBankOwnerBuildError { hardware, error }),
        };
        Ok(Self {
            hardware,
            barrier,
            state: PwmCommitBankOwnerState::Ready,
            fault: None,
        })
    }

    /// Validates then stages one complete image vector through the aggregate backend.
    pub fn stage(
        &mut self,
        images: [PwmCompareImage; AXES],
    ) -> Result<(), PwmCommitBankOwnerError<Hardware::Error>> {
        match self.state {
            PwmCommitBankOwnerState::Ready => {}
            PwmCommitBankOwnerState::AwaitingLatches => {
                return Err(PwmCommitBankOwnerError::Barrier {
                    error: PwmCommitBarrierError::Busy,
                    safe_error: None,
                });
            }
            state => return Err(PwmCommitBankOwnerError::Closed { state }),
        }
        if let Err(error) = self.barrier.stage(images) {
            let safe_error = self.fail_safe(PwmCommitBankOwnerFault::Barrier(error));
            return Err(PwmCommitBankOwnerError::Barrier { error, safe_error });
        }
        self.state = PwmCommitBankOwnerState::AwaitingLatches;
        if let Err(error) = self.hardware.stage_images(&images) {
            let operation = PwmCommitBankHardwareOperation::Stage;
            let safe_error = self.fail_safe(PwmCommitBankOwnerFault::Hardware(operation));
            return Err(PwmCommitBankOwnerError::Hardware {
                operation,
                error,
                safe_error,
            });
        }
        Ok(())
    }

    /// Polls one physical slot and records its first available truthful report.
    pub fn poll_latch(
        &mut self,
        axis: usize,
    ) -> Result<bool, PwmCommitBankOwnerError<Hardware::Error>> {
        match self.state {
            PwmCommitBankOwnerState::AwaitingLatches => {}
            PwmCommitBankOwnerState::Ready => {
                let error = PwmCommitBarrierError::Sequence;
                let safe_error = self.fail_safe(PwmCommitBankOwnerFault::Barrier(error));
                return Err(PwmCommitBankOwnerError::Barrier { error, safe_error });
            }
            state => return Err(PwmCommitBankOwnerError::Closed { state }),
        }
        if axis >= AXES {
            let error = PwmCommitBarrierError::AxisRange { axis };
            self.barrier.invalidate_after_hardware_fault();
            let safe_error = self.fail_safe(PwmCommitBankOwnerFault::Barrier(error));
            return Err(PwmCommitBankOwnerError::Barrier { error, safe_error });
        }
        let report = match self.hardware.take_latch(axis) {
            Ok(report) => report,
            Err(error) => {
                let operation = PwmCommitBankHardwareOperation::Observe { axis };
                let safe_error = self.fail_safe(PwmCommitBankOwnerFault::Hardware(operation));
                return Err(PwmCommitBankOwnerError::Hardware {
                    operation,
                    error,
                    safe_error,
                });
            }
        };
        let Some(report) = report else {
            return Ok(false);
        };
        if let Err(error) = self.barrier.record_latch(axis, report) {
            let safe_error = self.fail_safe(PwmCommitBankOwnerFault::Barrier(error));
            return Err(PwmCommitBankOwnerError::Barrier { error, safe_error });
        }
        Ok(true)
    }

    /// Closes the boundary and transactionally publishes the sealed completion.
    ///
    /// `publish` must provide all-or-none logical state installation. A
    /// publication rejection occurs after reported physical completion and
    /// therefore immediately invokes the all-stage safe transaction. The
    /// closure must install logical state atomically and must not panic.
    pub fn finish_and_publish<Value, PublicationError, Publish>(
        &mut self,
        observed_at: DeviceCycle,
        publish: Publish,
    ) -> Result<Value, PwmCommitBankOwnerError<Hardware::Error, PublicationError>>
    where
        Publish: FnOnce(PwmCommitBankCompletion<AXES>) -> Result<Value, PublicationError>,
    {
        match self.state {
            PwmCommitBankOwnerState::Ready | PwmCommitBankOwnerState::AwaitingLatches => {}
            state => return Err(PwmCommitBankOwnerError::Closed { state }),
        }
        let completion = match self.barrier.finish_boundary(observed_at) {
            Ok(completion) => completion,
            Err(error) => {
                let safe_error = self.fail_safe(PwmCommitBankOwnerFault::Barrier(error));
                return Err(PwmCommitBankOwnerError::Barrier { error, safe_error });
            }
        };
        match publish(completion) {
            Ok(value) => {
                self.state = PwmCommitBankOwnerState::Ready;
                Ok(value)
            }
            Err(error) => {
                let safe_error = self.fail_safe(PwmCommitBankOwnerFault::Publication);
                Err(PwmCommitBankOwnerError::Publication { error, safe_error })
            }
        }
    }

    /// Applies or retries the all-stage safe transaction and permanently closes.
    pub fn force_safe(&mut self) -> Result<(), Hardware::Error> {
        if self.state == PwmCommitBankOwnerState::SafeFault {
            return Ok(());
        }
        if self.fault.is_none() {
            self.fault = Some(PwmCommitBankOwnerFault::ExternalSafety);
        }
        match self.hardware.force_safe() {
            Ok(()) => {
                self.barrier.invalidate_after_safe();
                self.state = PwmCommitBankOwnerState::SafeFault;
                Ok(())
            }
            Err(error) => {
                self.barrier.invalidate_after_hardware_fault();
                self.state = PwmCommitBankOwnerState::UnsafeFault;
                Err(error)
            }
        }
    }

    /// Current closed ownership state.
    pub const fn state(&self) -> PwmCommitBankOwnerState {
        self.state
    }

    /// First terminal target-owner cause.
    pub const fn fault(&self) -> Option<PwmCommitBankOwnerFault> {
        self.fault
    }

    /// Whether every physical stage report exists for the pending boundary.
    pub fn ready_to_publish(&self) -> bool {
        self.state == PwmCommitBankOwnerState::AwaitingLatches && self.barrier.ready()
    }

    /// Sole next boundary admitted by the retained barrier.
    pub const fn next_boundary(&self) -> DeviceCycle {
        self.barrier.next_boundary()
    }

    /// Physical-period sequence assigned to the next complete set.
    pub const fn next_period_sequence(&self) -> u64 {
        self.barrier.next_period_sequence()
    }

    /// Read-only aggregate backend access for bounded diagnostics.
    pub const fn hardware(&self) -> &Hardware {
        &self.hardware
    }

    fn fail_safe(&mut self, fault: PwmCommitBankOwnerFault) -> Option<Hardware::Error> {
        if self.fault.is_none() {
            self.fault = Some(fault);
        }
        match self.hardware.force_safe() {
            Ok(()) => {
                self.barrier.invalidate_after_safe();
                self.state = PwmCommitBankOwnerState::SafeFault;
                None
            }
            Err(error) => {
                self.barrier.invalidate_after_hardware_fault();
                self.state = PwmCommitBankOwnerState::UnsafeFault;
                Some(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Duty3, FocParameterSnapshot, FocTimingProfile, PiConfig, PwmAdcSynchronization,
        PwmCompareContract, Q30, Q30Interval,
    };

    const DIGEST: Digest = Digest([0x79; 32]);
    const FIRST_BOUNDARY: DeviceCycle = DeviceCycle(8_000);
    const PWM_PERIOD_CYCLES: u32 = 4_000;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum BackendError {
        Stage,
        Observe,
        Safe,
    }

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    struct Behavior {
        stage_error: bool,
        observe_error_axis: Option<usize>,
        missing_axis: Option<usize>,
        late_axis: Option<usize>,
        substituted_axis: Option<usize>,
        repeated_axis: Option<usize>,
        safe_failures: u8,
    }

    #[derive(Debug, Eq, PartialEq)]
    struct ScriptedHardware<const AXES: usize> {
        behavior: Behavior,
        staged: Option<[PwmCompareImage; AXES]>,
        reported: [bool; AXES],
        stage_calls: u32,
        observation_calls: u32,
        safe_calls: u32,
        safe: bool,
    }

    impl<const AXES: usize> ScriptedHardware<AXES> {
        fn new(behavior: Behavior) -> Self {
            Self {
                behavior,
                staged: None,
                reported: [false; AXES],
                stage_calls: 0,
                observation_calls: 0,
                safe_calls: 0,
                safe: false,
            }
        }
    }

    impl<const AXES: usize> PwmCommitBankHardware<AXES> for ScriptedHardware<AXES> {
        type Error = BackendError;

        fn stage_images(&mut self, images: &[PwmCompareImage; AXES]) -> Result<(), Self::Error> {
            self.stage_calls += 1;
            self.staged = Some(*images);
            self.reported = [false; AXES];
            self.safe = false;
            if self.behavior.stage_error {
                Err(BackendError::Stage)
            } else {
                Ok(())
            }
        }

        fn take_latch(&mut self, axis: usize) -> Result<Option<PowerStageCommit>, Self::Error> {
            self.observation_calls += 1;
            if self.behavior.observe_error_axis == Some(axis) {
                return Err(BackendError::Observe);
            }
            if self.behavior.missing_axis == Some(axis) {
                return Ok(None);
            }
            let Some(image) = self
                .staged
                .as_ref()
                .and_then(|images| images.get(axis))
                .copied()
            else {
                return Ok(None);
            };
            if self.reported[axis] && self.behavior.repeated_axis != Some(axis) {
                return Ok(None);
            }
            self.reported[axis] = true;
            Ok(Some(PowerStageCommit {
                token: image.token() + u32::from(self.behavior.substituted_axis == Some(axis)),
                scheduled_at: image.scheduled_at(),
                observed_at: DeviceCycle(
                    image.scheduled_at().0 + u64::from(self.behavior.late_axis == Some(axis)),
                ),
            }))
        }

        fn force_safe(&mut self) -> Result<(), Self::Error> {
            self.safe_calls += 1;
            if self.behavior.safe_failures != 0 {
                self.behavior.safe_failures -= 1;
                return Err(BackendError::Safe);
            }
            self.staged = None;
            self.safe = true;
            Ok(())
        }
    }

    fn parameters() -> FocParameterSnapshot {
        let controller = PiConfig {
            proportional_gain: Q30::ZERO,
            integral_gain_per_update: Q30::ZERO,
            integral_minimum: Q30::NEG_ONE,
            integral_maximum: Q30::ONE,
            output_minimum: Q30::from_bits(-Q30::HALF.bits()),
            output_maximum: Q30::HALF,
        };
        FocParameterSnapshot {
            configuration_digest: DIGEST,
            pole_pairs: 7,
            timing: FocTimingProfile {
                pwm_hz: 20_000,
                current_loop_hz: 20_000,
                velocity_loop_divider: 20,
                position_loop_divider: 10,
            },
            maximum_phase_current: Q30::ONE,
            maximum_phase_voltage: Q30::ONE,
            d_current: controller,
            q_current: controller,
        }
    }

    fn synchronization() -> PwmAdcSynchronization {
        PwmAdcSynchronization {
            configuration_digest: DIGEST,
            device_cycle_hz: 80_000_000,
            pwm_period_cycles: PWM_PERIOD_CYCLES,
            nominal_acquisition_offset_cycles: 2_000,
            maximum_trigger_jitter_cycles: 2,
            maximum_acquisition_cycles: 40,
            maximum_channel_skew_cycles: 20,
            maximum_conversion_cycles: 80,
            minimum_switching_guard_cycles: 100,
        }
    }

    fn images(at: DeviceCycle) -> [PwmCompareImage; 2] {
        let contract = PwmCompareContract::new(
            DIGEST,
            80_000_000,
            PWM_PERIOD_CYCLES,
            160_000_000,
            4_000,
            8,
            u32::MAX,
        )
        .unwrap();
        [5_u32, 6_u32].map(|token| {
            contract
                .lower(
                    &parameters(),
                    synchronization(),
                    token,
                    at,
                    Duty3 {
                        a: Q30Interval::HALF,
                        b: Q30Interval::HALF,
                        c: Q30Interval::HALF,
                    },
                )
                .unwrap()
        })
    }

    fn owner(behavior: Behavior) -> PwmCommitBankTargetOwner<ScriptedHardware<2>, 2> {
        PwmCommitBankTargetOwner::new(
            ScriptedHardware::new(behavior),
            DIGEST,
            PWM_PERIOD_CYCLES,
            FIRST_BOUNDARY,
            7,
        )
        .unwrap()
    }

    #[test]
    fn construction_returns_never_staged_hardware_on_rejection() {
        let rejected = PwmCommitBankTargetOwner::<_, 2>::new(
            ScriptedHardware::new(Behavior::default()),
            Digest::ZERO,
            PWM_PERIOD_CYCLES,
            FIRST_BOUNDARY,
            7,
        )
        .unwrap_err();
        assert_eq!(rejected.error(), PwmCommitBarrierError::Configuration);
        let (hardware, error) = rejected.into_parts();
        assert_eq!(error, PwmCommitBarrierError::Configuration);
        assert_eq!(hardware.stage_calls, 0);
        assert_eq!(hardware.safe_calls, 0);
    }

    #[test]
    fn complete_out_of_order_hardware_set_publishes_once() {
        let mut owner = owner(Behavior::default());
        let staged = images(FIRST_BOUNDARY);
        owner.stage(staged).unwrap();
        assert_eq!(owner.state(), PwmCommitBankOwnerState::AwaitingLatches);
        assert_eq!(owner.hardware().stage_calls, 1);
        assert_eq!(
            owner.stage(staged),
            Err(PwmCommitBankOwnerError::Barrier {
                error: PwmCommitBarrierError::Busy,
                safe_error: None,
            })
        );
        assert_eq!(owner.hardware().safe_calls, 0);

        assert!(owner.poll_latch(1).unwrap());
        assert!(!owner.ready_to_publish());
        assert!(owner.poll_latch(0).unwrap());
        assert!(owner.ready_to_publish());
        let published = owner
            .finish_and_publish(FIRST_BOUNDARY, |completion| {
                assert_eq!(completion.configuration_digest(), DIGEST);
                assert_eq!(completion.period_sequence(), 7);
                assert_eq!(completion.observed_at(), FIRST_BOUNDARY);
                Ok::<_, Infallible>(completion.commits().map(|commit| commit.token))
            })
            .unwrap();
        assert_eq!(published, [5, 6]);
        assert_eq!(owner.state(), PwmCommitBankOwnerState::Ready);
        assert_eq!(owner.fault(), None);
        assert_eq!(owner.next_boundary(), DeviceCycle(12_000));
        assert_eq!(owner.next_period_sequence(), 8);
        assert_eq!(owner.hardware().safe_calls, 0);
    }

    #[test]
    fn stage_failure_attempts_safe_and_supports_only_safe_retry() {
        let mut owner = owner(Behavior {
            stage_error: true,
            safe_failures: 1,
            ..Behavior::default()
        });
        assert_eq!(
            owner.stage(images(FIRST_BOUNDARY)),
            Err(PwmCommitBankOwnerError::Hardware {
                operation: PwmCommitBankHardwareOperation::Stage,
                error: BackendError::Stage,
                safe_error: Some(BackendError::Safe),
            })
        );
        assert_eq!(owner.state(), PwmCommitBankOwnerState::UnsafeFault);
        assert_eq!(
            owner.fault(),
            Some(PwmCommitBankOwnerFault::Hardware(
                PwmCommitBankHardwareOperation::Stage
            ))
        );
        assert_eq!(owner.hardware().safe_calls, 1);
        assert_eq!(owner.force_safe(), Ok(()));
        assert_eq!(owner.state(), PwmCommitBankOwnerState::SafeFault);
        assert!(owner.hardware().safe);
        assert_eq!(owner.hardware().safe_calls, 2);
        assert_eq!(
            owner.stage(images(FIRST_BOUNDARY)),
            Err(PwmCommitBankOwnerError::Closed {
                state: PwmCommitBankOwnerState::SafeFault
            })
        );
    }

    #[test]
    fn observation_and_report_mismatches_force_the_complete_safe_path() {
        let mut peripheral = owner(Behavior {
            observe_error_axis: Some(1),
            ..Behavior::default()
        });
        peripheral.stage(images(FIRST_BOUNDARY)).unwrap();
        assert_eq!(
            peripheral.poll_latch(1),
            Err(PwmCommitBankOwnerError::Hardware {
                operation: PwmCommitBankHardwareOperation::Observe { axis: 1 },
                error: BackendError::Observe,
                safe_error: None,
            })
        );
        assert_eq!(peripheral.state(), PwmCommitBankOwnerState::SafeFault);
        assert!(peripheral.hardware().safe);

        for (behavior, expected) in [
            (
                Behavior {
                    late_axis: Some(1),
                    ..Behavior::default()
                },
                PwmCommitBarrierError::Observation { axis: 1 },
            ),
            (
                Behavior {
                    substituted_axis: Some(1),
                    ..Behavior::default()
                },
                PwmCommitBarrierError::Observation { axis: 1 },
            ),
        ] {
            let mut owner = owner(behavior);
            owner.stage(images(FIRST_BOUNDARY)).unwrap();
            assert_eq!(
                owner.poll_latch(1),
                Err(PwmCommitBankOwnerError::Barrier {
                    error: expected,
                    safe_error: None,
                })
            );
            assert_eq!(owner.state(), PwmCommitBankOwnerState::SafeFault);
            assert_eq!(
                owner.fault(),
                Some(PwmCommitBankOwnerFault::Barrier(expected))
            );
            assert!(owner.hardware().safe);
        }

        let mut duplicate = owner(Behavior {
            repeated_axis: Some(0),
            ..Behavior::default()
        });
        duplicate.stage(images(FIRST_BOUNDARY)).unwrap();
        assert!(duplicate.poll_latch(0).unwrap());
        assert_eq!(
            duplicate.poll_latch(0),
            Err(PwmCommitBankOwnerError::Barrier {
                error: PwmCommitBarrierError::DuplicateObservation { axis: 0 },
                safe_error: None,
            })
        );
        assert!(duplicate.hardware().safe);
    }

    #[test]
    fn barrier_stage_sequence_and_boundary_failures_are_terminal_and_safe() {
        let mut wrong_schedule = owner(Behavior::default());
        assert_eq!(
            wrong_schedule.stage(images(DeviceCycle(12_000))),
            Err(PwmCommitBankOwnerError::Barrier {
                error: PwmCommitBarrierError::Schedule { axis: 0 },
                safe_error: None,
            })
        );
        assert_eq!(
            wrong_schedule.fault(),
            Some(PwmCommitBankOwnerFault::Barrier(
                PwmCommitBarrierError::Schedule { axis: 0 }
            ))
        );
        assert_eq!(wrong_schedule.state(), PwmCommitBankOwnerState::SafeFault);
        assert_eq!(wrong_schedule.hardware().safe_calls, 1);

        let mut no_stage = owner(Behavior::default());
        assert_eq!(
            no_stage.poll_latch(0),
            Err(PwmCommitBankOwnerError::Barrier {
                error: PwmCommitBarrierError::Sequence,
                safe_error: None,
            })
        );
        assert_eq!(no_stage.state(), PwmCommitBankOwnerState::SafeFault);
        assert_eq!(no_stage.hardware().observation_calls, 0);
        assert_eq!(no_stage.hardware().safe_calls, 1);

        let mut wrong_boundary = owner(Behavior::default());
        wrong_boundary.stage(images(FIRST_BOUNDARY)).unwrap();
        assert!(wrong_boundary.poll_latch(0).unwrap());
        assert!(wrong_boundary.poll_latch(1).unwrap());
        let mut published = false;
        assert_eq!(
            wrong_boundary.finish_and_publish(DeviceCycle(8_001), |_| {
                published = true;
                Ok::<_, Infallible>(())
            }),
            Err(PwmCommitBankOwnerError::Barrier {
                error: PwmCommitBarrierError::Boundary,
                safe_error: None,
            })
        );
        assert!(!published);
        assert_eq!(
            wrong_boundary.fault(),
            Some(PwmCommitBankOwnerFault::Barrier(
                PwmCommitBarrierError::Boundary
            ))
        );
        assert_eq!(wrong_boundary.state(), PwmCommitBankOwnerState::SafeFault);
        assert_eq!(wrong_boundary.hardware().safe_calls, 1);
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum PublishError {
        Rejected,
    }

    #[test]
    fn missing_boundary_and_publication_failure_never_escape_safe_handling() {
        let mut missing = owner(Behavior {
            missing_axis: Some(1),
            ..Behavior::default()
        });
        missing.stage(images(FIRST_BOUNDARY)).unwrap();
        assert!(missing.poll_latch(0).unwrap());
        assert!(!missing.poll_latch(1).unwrap());
        assert_eq!(
            missing.finish_and_publish(FIRST_BOUNDARY, |_| Ok::<_, Infallible>(())),
            Err(PwmCommitBankOwnerError::Barrier {
                error: PwmCommitBarrierError::MissingObservation { axis: 1 },
                safe_error: None,
            })
        );
        assert_eq!(missing.state(), PwmCommitBankOwnerState::SafeFault);
        assert!(missing.hardware().safe);

        let mut publication = owner(Behavior::default());
        publication.stage(images(FIRST_BOUNDARY)).unwrap();
        assert!(publication.poll_latch(0).unwrap());
        assert!(publication.poll_latch(1).unwrap());
        assert_eq!(
            publication
                .finish_and_publish(FIRST_BOUNDARY, |_| { Err::<(), _>(PublishError::Rejected) }),
            Err(PwmCommitBankOwnerError::Publication {
                error: PublishError::Rejected,
                safe_error: None,
            })
        );
        assert_eq!(publication.state(), PwmCommitBankOwnerState::SafeFault);
        assert_eq!(
            publication.fault(),
            Some(PwmCommitBankOwnerFault::Publication)
        );
        assert!(publication.hardware().safe);
        assert_eq!(publication.next_boundary(), DeviceCycle(12_000));
    }

    #[test]
    fn unsolicited_axis_and_external_stop_are_terminal_and_safe() {
        let mut invalid_axis = owner(Behavior::default());
        invalid_axis.stage(images(FIRST_BOUNDARY)).unwrap();
        assert_eq!(
            invalid_axis.poll_latch(2),
            Err(PwmCommitBankOwnerError::Barrier {
                error: PwmCommitBarrierError::AxisRange { axis: 2 },
                safe_error: None,
            })
        );
        assert!(invalid_axis.hardware().safe);

        let mut stopped = owner(Behavior::default());
        stopped.stage(images(FIRST_BOUNDARY)).unwrap();
        assert_eq!(stopped.force_safe(), Ok(()));
        assert_eq!(stopped.state(), PwmCommitBankOwnerState::SafeFault);
        assert_eq!(
            stopped.fault(),
            Some(PwmCommitBankOwnerFault::ExternalSafety)
        );
        assert!(stopped.hardware().safe);
        assert_eq!(stopped.force_safe(), Ok(()));
        assert_eq!(stopped.hardware().safe_calls, 1);
    }
}
