//! Deterministic affine clocks and Wi-Fi heartbeat exchanges for multi-MCU jobs.

use alumina_clock::{BootId, ClockFlags, ClockHeartbeatResponse, ClockObservation, ClockSource};
use alumina_protocol::DeviceCycle;

const PPM_SCALE: i64 = 1_000_000;
const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// Arithmetic or parameter rejection from the deterministic clock model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AffineClockError {
    Frequency,
    Rate,
    Time,
    Arithmetic,
}

/// One deterministic device clock with an explicit boot, offset, and rate.
///
/// The simulated counter is the floor of the exact positive affine mapping.
/// This deliberately differs from the browser estimator, which sees only a
/// declared nominal frequency, a drift envelope, and causal Wi-Fi timestamps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AffineDeviceClock {
    pub boot_id: BootId,
    pub nominal_frequency_hz: u64,
    pub rate_adjustment_ppm: i32,
    pub offset_cycles: u64,
    pub minimum_lead_cycles: u64,
    pub maximum_schedule_horizon_cycles: u64,
}

impl AffineDeviceClock {
    /// Validates the positive clock rate and exported scheduling bounds.
    pub fn validate(self) -> Result<(), AffineClockError> {
        if self.nominal_frequency_hz == 0 {
            return Err(AffineClockError::Frequency);
        }
        if i64::from(self.rate_adjustment_ppm).unsigned_abs() >= PPM_SCALE as u64 {
            return Err(AffineClockError::Rate);
        }
        if self.minimum_lead_cycles == 0
            || self.maximum_schedule_horizon_cycles < self.minimum_lead_cycles
        {
            return Err(AffineClockError::Time);
        }
        Ok(())
    }

    /// Samples the actual integer counter at one browser-worker time.
    pub fn cycle_at_ui_ns(self, ui_ns: u64) -> Result<DeviceCycle, AffineClockError> {
        self.validate()?;
        let numerator = self.rate_numerator()?;
        let denominator = NANOS_PER_SECOND
            .checked_mul(PPM_SCALE as u128)
            .ok_or(AffineClockError::Arithmetic)?;
        let elapsed = u128::from(ui_ns)
            .checked_mul(numerator)
            .ok_or(AffineClockError::Arithmetic)?
            / denominator;
        let elapsed = u64::try_from(elapsed).map_err(|_| AffineClockError::Arithmetic)?;
        Ok(DeviceCycle(
            self.offset_cycles
                .checked_add(elapsed)
                .ok_or(AffineClockError::Arithmetic)?,
        ))
    }

    /// Earliest browser-worker nanosecond at which the counter reaches a cycle.
    pub fn ui_ns_at_or_after_cycle(self, cycle: DeviceCycle) -> Result<u64, AffineClockError> {
        self.validate()?;
        let elapsed = cycle
            .0
            .checked_sub(self.offset_cycles)
            .ok_or(AffineClockError::Time)?;
        let denominator = NANOS_PER_SECOND
            .checked_mul(PPM_SCALE as u128)
            .ok_or(AffineClockError::Arithmetic)?;
        let scaled = u128::from(elapsed)
            .checked_mul(denominator)
            .ok_or(AffineClockError::Arithmetic)?;
        let numerator = self.rate_numerator()?;
        let ui_ns = scaled
            .checked_add(numerator - 1)
            .ok_or(AffineClockError::Arithmetic)?
            / numerator;
        u64::try_from(ui_ns).map_err(|_| AffineClockError::Arithmetic)
    }

    /// Produces one causal four-timestamp heartbeat over asymmetric Wi-Fi.
    pub fn heartbeat(
        self,
        probe_id: u64,
        ui_send_ns: u64,
        request_delay_ns: u64,
        device_processing_ns: u64,
        response_delay_ns: u64,
    ) -> Result<ClockObservation, AffineClockError> {
        let receive_ui_ns = ui_send_ns
            .checked_add(request_delay_ns)
            .ok_or(AffineClockError::Arithmetic)?;
        let transmit_ui_ns = receive_ui_ns
            .checked_add(device_processing_ns)
            .ok_or(AffineClockError::Arithmetic)?;
        let ui_receive_ns = transmit_ui_ns
            .checked_add(response_delay_ns)
            .ok_or(AffineClockError::Arithmetic)?;
        Ok(ClockObservation {
            response: ClockHeartbeatResponse {
                flags: ClockFlags(
                    ClockFlags::MONOTONIC
                        | ClockFlags::SHARED_BETWEEN_CORES
                        | ClockFlags::DEADLINE_HEALTHY,
                ),
                counter_bits: 64,
                source: ClockSource::EmbassyMonotonic,
                probe_id,
                ui_send_ns,
                boot_id: self.boot_id,
                receive_cycle: self.cycle_at_ui_ns(receive_ui_ns)?,
                transmit_cycle: self.cycle_at_ui_ns(transmit_ui_ns)?,
                frequency_hz: self.nominal_frequency_hz,
                minimum_lead_cycles: self.minimum_lead_cycles,
                maximum_schedule_horizon_cycles: self.maximum_schedule_horizon_cycles,
                queue_horizon_cycles: self.minimum_lead_cycles,
                maximum_lateness_cycles: 0,
                missed_deadlines: 0,
                command_queue_free: 4,
                work_queue_depth: 1,
            },
            ui_receive_ns,
        })
    }

    fn rate_numerator(self) -> Result<u128, AffineClockError> {
        let factor = PPM_SCALE
            .checked_add(i64::from(self.rate_adjustment_ppm))
            .ok_or(AffineClockError::Arithmetic)?;
        let factor = u128::try_from(factor).map_err(|_| AffineClockError::Rate)?;
        u128::from(self.nominal_frequency_hz)
            .checked_mul(factor)
            .ok_or(AffineClockError::Arithmetic)
    }
}

#[cfg(test)]
mod tests {
    use alumina_clock::{BOOT_ID_BYTES, ClockEstimationPolicy, ClockEstimator, ClockPrediction};
    use alumina_job::{
        JOB_COMMIT_ID_BYTES, JobCommitId, JobCommitRequest, JobDescriptor, JobNetworkPolicy,
        JobScheduleAction, JobScheduleAdmission, JobScheduleError, JobScheduleReference,
        JobScheduleReferenceAction, JobScheduleState, PreparedJobSchedule,
    };
    use alumina_machine_ir::{
        BlockValidationLimits, EXECUTION_BLOCK_BYTES, StreamId, StreamTick, ValidationLimits,
    };
    use alumina_protocol::Digest;
    use alumina_storage::{ContentId, DigestAlgorithm, ObjectKind, PublishedObject, StoredObject};

    use super::*;

    const TARGET_UI_NS: u64 = 5_000_000_000;
    const OBSERVATION_NOW_UI_NS: u64 = 3_001_000_000;

    fn boot(byte: u8) -> BootId {
        BootId::new([byte; BOOT_ID_BYTES]).unwrap()
    }

    fn clock(boot_byte: u8, offset_cycles: u64, rate_adjustment_ppm: i32) -> AffineDeviceClock {
        AffineDeviceClock {
            boot_id: boot(boot_byte),
            nominal_frequency_hz: 1_000_000,
            rate_adjustment_ppm,
            offset_cycles,
            minimum_lead_cycles: 100_000,
            maximum_schedule_horizon_cycles: 10_000_000,
        }
    }

    fn estimator(clock: AffineDeviceClock, flip_delays: bool) -> ClockEstimator {
        let mut estimator = ClockEstimator::new(ClockEstimationPolicy {
            maximum_round_trip_ns: 3_000_000,
            maximum_device_processing_cycles: 1_000,
            maximum_drift_ppm: 100,
            maximum_sample_age_ns: 100_000_000,
            maximum_schedule_horizon_ns: 10_000_000_000,
            minimum_schedule_lead_ns: 100_000_000,
            minimum_samples: 3,
        })
        .unwrap();
        let delays = if flip_delays {
            [
                (700_000, 40_000, 120_000),
                (100_000, 60_000, 700_000),
                (500_000, 50_000, 400_000),
            ]
        } else {
            [
                (120_000, 50_000, 700_000),
                (650_000, 60_000, 120_000),
                (300_000, 40_000, 600_000),
            ]
        };
        for (index, (up, processing, down)) in delays.into_iter().enumerate() {
            let send = u64::try_from(index + 1).unwrap() * 1_000_000_000;
            estimator
                .observe(
                    clock
                        .heartbeat(
                            u64::try_from(index + 1).unwrap(),
                            send,
                            up,
                            processing,
                            down,
                        )
                        .unwrap(),
                )
                .unwrap();
        }
        estimator
    }

    fn descriptor(partition_byte: u8) -> JobDescriptor {
        JobDescriptor {
            prepare_id: 41,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId {
                        algorithm: DigestAlgorithm::Sha256,
                        digest: Digest([partition_byte; 32]),
                    },
                    byte_len: EXECUTION_BLOCK_BYTES as u64,
                },
                manifest: ContentId {
                    algorithm: DigestAlgorithm::Sha256,
                    digest: Digest([partition_byte.wrapping_add(1); 32]),
                },
            },
            stream_id: StreamId::new([partition_byte; 16]).unwrap(),
            capability_digest: Digest([0x71; 32]),
            config_digest: Digest([0x72; 32]),
            axis_count: 3,
            block_count: 1,
            first_tick: StreamTick(0),
            initial_position: [0; alumina_machine_ir::MAX_EXECUTION_AXES],
            limits: BlockValidationLimits {
                maximum_block_ticks: 1_000_000,
                segment: ValidationLimits {
                    maximum_segment_ticks: 100_000,
                    maximum_steps_per_segment: 100_000,
                },
            },
        }
    }

    fn prediction(clock: AffineDeviceClock, flipped: bool) -> ClockPrediction {
        estimator(clock, flipped)
            .predict(OBSERVATION_NOW_UI_NS, TARGET_UI_NS, 2_000)
            .unwrap()
    }

    fn commit(
        clock: AffineDeviceClock,
        descriptor: JobDescriptor,
        prediction: ClockPrediction,
        commit_byte: u8,
    ) -> JobCommitRequest {
        let schedule = PreparedJobSchedule::prepare::<3>(clock.boot_id, descriptor).unwrap();
        JobCommitRequest {
            policy: JobNetworkPolicy::NetworkAttended,
            prepare_id: descriptor.prepare_id,
            boot_id: clock.boot_id,
            global_job_digest: Digest([0xa1; 32]),
            participant_set_digest: Digest([0xa2; 32]),
            prepared_token: schedule.prepared_token(),
            partition_digest: descriptor.partition.object.content.digest,
            local_start_cycle: prediction.scheduled_cycle,
            confirm_deadline_cycle: DeviceCycle(prediction.scheduled_cycle.0 - 400_000),
            abort_guard_cycle: DeviceCycle(prediction.scheduled_cycle.0 - 200_000),
            lease_expiry_cycle: DeviceCycle(prediction.scheduled_cycle.0 + 5_000_000),
            clock_probe_id: prediction.latest_probe_id,
            clock_uncertainty_cycles: prediction.uncertainty_cycles,
            required_sync_tolerance_cycles: 2_000,
            commit_id: JobCommitId::new([commit_byte; JOB_COMMIT_ID_BYTES]).unwrap(),
        }
    }

    fn admission(clock: AffineDeviceClock, descriptor: JobDescriptor) -> JobScheduleAdmission {
        JobScheduleAdmission {
            now: clock.cycle_at_ui_ns(OBSERVATION_NOW_UI_NS).unwrap(),
            active_config: descriptor.config_digest,
            minimum_lead_cycles: clock.minimum_lead_cycles,
            maximum_start_horizon_cycles: clock.maximum_schedule_horizon_cycles,
            maximum_lease_cycles: 10_000_000,
            maximum_sync_tolerance_cycles: 2_000,
            cache_ready: true,
            safety_ready: true,
            autonomous_allowed: false,
        }
    }

    fn pair() -> (
        AffineDeviceClock,
        AffineDeviceClock,
        JobDescriptor,
        JobDescriptor,
        JobCommitRequest,
        JobCommitRequest,
    ) {
        let clock_a = clock(0x11, 75_000, 40);
        let clock_b = clock(0x22, 325_000, -35);
        let descriptor_a = descriptor(0x31);
        let descriptor_b = descriptor(0x41);
        let commit_a = commit(clock_a, descriptor_a, prediction(clock_a, false), 0x51);
        let commit_b = commit(clock_b, descriptor_b, prediction(clock_b, true), 0x61);
        (
            clock_a,
            clock_b,
            descriptor_a,
            descriptor_b,
            commit_a,
            commit_b,
        )
    }

    #[test]
    fn skewed_devices_install_confirm_and_start_from_one_ui_epoch() {
        let (clock_a, clock_b, descriptor_a, descriptor_b, commit_a, commit_b) = pair();
        let exact_a = clock_a.cycle_at_ui_ns(TARGET_UI_NS).unwrap().0;
        let exact_b = clock_b.cycle_at_ui_ns(TARGET_UI_NS).unwrap().0;
        assert!(exact_a.abs_diff(commit_a.local_start_cycle.0) <= 2_000);
        assert!(exact_b.abs_diff(commit_b.local_start_cycle.0) <= 2_000);

        let mut schedule_a =
            PreparedJobSchedule::prepare::<3>(clock_a.boot_id, descriptor_a).unwrap();
        let mut schedule_b =
            PreparedJobSchedule::prepare::<3>(clock_b.boot_id, descriptor_b).unwrap();
        schedule_a
            .install(commit_a, admission(clock_a, descriptor_a))
            .unwrap();
        schedule_b
            .install(commit_b, admission(clock_b, descriptor_b))
            .unwrap();
        let confirm_a =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit_a)
                .unwrap();
        let confirm_b =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit_b)
                .unwrap();
        schedule_a
            .confirm(
                confirm_a,
                DeviceCycle(commit_a.confirm_deadline_cycle.0 - 1),
            )
            .unwrap();
        schedule_b
            .confirm(
                confirm_b,
                DeviceCycle(commit_b.confirm_deadline_cycle.0 - 1),
            )
            .unwrap();
        assert!(matches!(
            schedule_a.advance(commit_a.local_start_cycle),
            JobScheduleAction::Start { .. }
        ));
        assert!(matches!(
            schedule_b.advance(commit_b.local_start_cycle),
            JobScheduleAction::Start { .. }
        ));

        let actual_start_a = clock_a
            .ui_ns_at_or_after_cycle(commit_a.local_start_cycle)
            .unwrap();
        let actual_start_b = clock_b
            .ui_ns_at_or_after_cycle(commit_b.local_start_cycle)
            .unwrap();
        assert!(actual_start_a.abs_diff(TARGET_UI_NS) <= 2_000_000);
        assert!(actual_start_b.abs_diff(TARGET_UI_NS) <= 2_000_000);
        assert!(actual_start_a.abs_diff(actual_start_b) <= 4_000_000);
    }

    #[test]
    fn missing_install_or_confirmation_is_reconciled_before_the_abort_guard() {
        let (clock_a, clock_b, descriptor_a, descriptor_b, commit_a, commit_b) = pair();
        let mut installed =
            PreparedJobSchedule::prepare::<3>(clock_a.boot_id, descriptor_a).unwrap();
        let untouched = PreparedJobSchedule::prepare::<3>(clock_b.boot_id, descriptor_b).unwrap();
        installed
            .install(commit_a, admission(clock_a, descriptor_a))
            .unwrap();
        let abort_a =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Abort, commit_a).unwrap();
        installed
            .abort(abort_a, DeviceCycle(commit_a.confirm_deadline_cycle.0))
            .unwrap();
        assert_eq!(installed.report().state, JobScheduleState::Aborted);
        assert_eq!(untouched.report().state, JobScheduleState::Prepared);

        let mut confirmed =
            PreparedJobSchedule::prepare::<3>(clock_a.boot_id, descriptor_a).unwrap();
        let mut unconfirmed =
            PreparedJobSchedule::prepare::<3>(clock_b.boot_id, descriptor_b).unwrap();
        confirmed
            .install(commit_a, admission(clock_a, descriptor_a))
            .unwrap();
        unconfirmed
            .install(commit_b, admission(clock_b, descriptor_b))
            .unwrap();
        confirmed
            .confirm(
                JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit_a)
                    .unwrap(),
                DeviceCycle(commit_a.confirm_deadline_cycle.0 - 1),
            )
            .unwrap();
        assert_eq!(
            unconfirmed.advance(commit_b.confirm_deadline_cycle),
            JobScheduleAction::AbortUnconfirmed
        );
        confirmed
            .abort(abort_a, DeviceCycle(commit_a.confirm_deadline_cycle.0 + 1))
            .unwrap();
        assert_eq!(confirmed.report().state, JobScheduleState::Aborted);
        assert_eq!(unconfirmed.report().state, JobScheduleState::Expired);
        assert_eq!(
            confirmed.advance(commit_a.local_start_cycle),
            JobScheduleAction::None
        );
        assert_eq!(
            unconfirmed.advance(commit_b.local_start_cycle),
            JobScheduleAction::None
        );
    }

    #[test]
    fn reboot_identity_rejects_stale_commit_and_followup_actions() {
        let (clock_a, _, descriptor_a, _, commit_a, _) = pair();
        let confirm =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit_a)
                .unwrap();
        let reboot = clock(0x77, clock_a.offset_cycles, clock_a.rate_adjustment_ppm);
        let mut rebooted = PreparedJobSchedule::prepare::<3>(reboot.boot_id, descriptor_a).unwrap();
        assert_eq!(
            rebooted.install(commit_a, admission(reboot, descriptor_a)),
            Err(JobScheduleError::Identity)
        );
        assert_eq!(
            rebooted.confirm(confirm, DeviceCycle(commit_a.confirm_deadline_cycle.0 - 1)),
            Err(JobScheduleError::Identity)
        );
        assert_eq!(rebooted.report().state, JobScheduleState::Prepared);
    }
}
