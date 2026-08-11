//! Core-0 ownership and authenticated admission for cached machine jobs.

use alumina_clock::BootId;
use alumina_job::{
    AdmittedBlock, CoreJobCommand, JobCancelRequest, JobCommitRequest, JobDescriptor, JobError,
    JobNetworkPolicy, JobScheduleAction, JobScheduleAdmission, JobScheduleReference,
    JobScheduleReferenceAction, JobScheduleReport, JobScheduleState, JobStatusReport, RealtimeJob,
    RealtimeJobReport, RealtimeJobState, RealtimePoll, ServiceJobReport, ServiceJobState,
    ServicePrefetch,
};
use alumina_protocol::{DeviceCycle, Digest, FrameKind, Operation, StatusCode};
use alumina_runtime::{DefaultRealtimeEndpoint, DefaultServiceEndpoint, IntercoreFrame};
use alumina_safety::SafetyState;
use alumina_service::{NativeRequest, ServiceRequest, ServiceRequestKind, ServiceResponse};
use alumina_storage::media::MediaError;
use alumina_storage::provisioning::ProvisionedCacheError;

use crate::clock::{ClockJobFacts, MAXIMUM_START_HORIZON_CYCLES, MINIMUM_START_LEAD_CYCLES};
use crate::hardware::selected;

const MAXIMUM_JOB_LEASE_CYCLES: u64 = embassy_time::Duration::from_secs(60 * 60).as_ticks();
const MAXIMUM_SYNC_TOLERANCE_CYCLES: u64 = embassy_time::Duration::from_millis(5).as_ticks();

/// Sole service-core owner of a publication cursor and latest RT observation.
pub struct JobService {
    boot_id: BootId,
    descriptor: Option<JobDescriptor>,
    prefetch: Option<ServicePrefetch<{ selected::JOB_AXES }>>,
    realtime: Option<RealtimeJobReport>,
    schedule: Option<JobScheduleReport>,
    commit: Option<JobCommitRequest>,
    command_sequence: u32,
    active_config: Digest,
    configuration_transition: bool,
}

impl JobService {
    /// Starts with no prepared job and no storage cursor.
    pub const fn new(boot_id: BootId) -> Self {
        Self {
            boot_id,
            descriptor: None,
            prefetch: None,
            realtime: None,
            schedule: None,
            commit: None,
            command_sequence: 0,
            active_config: Digest::ZERO,
            configuration_transition: false,
        }
    }

    /// Whether a valid universal request selects the native job family.
    pub fn handles(request: &ServiceRequest) -> bool {
        request.kind() == ServiceRequestKind::NativeFrame
            && NativeRequest::decode(request.bytes())
                .is_ok_and(|request| request.frame.kind == FrameKind::Job)
    }

    /// Dispatches one authenticated job request and returns a correlated frame.
    pub async fn dispatch(
        &mut self,
        cache: &mut selected::StorageBackend,
        endpoint: &mut DefaultServiceEndpoint,
        request: &ServiceRequest,
        now: DeviceCycle,
        latest_clock_probe_id: Option<u64>,
        safety_state: SafetyState,
    ) -> ServiceResponse {
        let Ok(native) = NativeRequest::decode(request.bytes()) else {
            return ServiceResponse::invalid_native();
        };
        if native.frame.kind != FrameKind::Job {
            return ServiceResponse::invalid_native();
        }
        match native.message.operation {
            Operation::JobPrepare => {
                self.prepare(cache, endpoint, native, now, safety_state)
                    .await
            }
            Operation::JobCommit => {
                self.commit(endpoint, native, now, latest_clock_probe_id, safety_state)
            }
            Operation::JobConfirm => self.confirm(endpoint, native, now, safety_state),
            Operation::JobAbort => self.abort(endpoint, native, now),
            Operation::JobCancel => self.cancel(endpoint, native, now),
            Operation::JobStatus if native.body.is_empty() => {
                self.respond(endpoint, native, now, StatusCode::Ok, true)
            }
            _ => self.respond(endpoint, native, now, StatusCode::Unsupported, false),
        }
    }

    async fn prepare(
        &mut self,
        cache: &mut selected::StorageBackend,
        endpoint: &mut DefaultServiceEndpoint,
        native: NativeRequest<'_>,
        now: DeviceCycle,
        safety_state: SafetyState,
    ) -> ServiceResponse {
        if self.configuration_transition {
            return self.respond(endpoint, native, now, StatusCode::Busy, false);
        }
        if safety_state != SafetyState::Configured {
            return self.respond(endpoint, native, now, StatusCode::ForbiddenState, false);
        }
        let descriptor = match JobDescriptor::decode::<{ selected::JOB_AXES }>(native.body) {
            Ok(descriptor) => descriptor,
            Err(_) => {
                return self.respond(endpoint, native, now, StatusCode::InvalidRequest, false);
            }
        };
        if native.frame.config_digest != descriptor.config_digest {
            return self.respond(endpoint, native, now, StatusCode::Conflict, false);
        }
        let expected_capability = selected::PACKAGE.board.capability_digest;
        if !selected::PACKAGE.armable
            || !selected::MOTION_OUTPUT_QUALIFIED
            || expected_capability.is_zero()
            || self.active_config.is_zero()
        {
            return self.respond(endpoint, native, now, StatusCode::Unsupported, false);
        }
        if descriptor.capability_digest != expected_capability {
            return self.respond(endpoint, native, now, StatusCode::Conflict, false);
        }
        if descriptor.config_digest != self.active_config {
            return self.respond(endpoint, native, now, StatusCode::Conflict, false);
        }

        if self.descriptor == Some(descriptor) {
            return self.respond(endpoint, native, now, StatusCode::Ok, true);
        }
        if !self.replaceable(endpoint) {
            return self.respond(endpoint, native, now, StatusCode::Busy, false);
        }

        let actor = match ServicePrefetch::open(cache, descriptor).await {
            Ok(actor) => actor,
            Err(error) => {
                return self.respond(endpoint, native, now, job_error_status(error), false);
            }
        };
        let command = match (CoreJobCommand::Prepare {
            boot_id: self.boot_id,
            descriptor,
        })
        .encode::<{ selected::JOB_AXES }>()
        {
            Ok(command) => command,
            Err(_) => {
                return self.respond(endpoint, native, now, StatusCode::InvalidRequest, false);
            }
        };
        let sequence = next_nonzero(self.command_sequence);
        let frame = match IntercoreFrame::new(
            FrameKind::Job,
            sequence,
            now,
            descriptor.config_digest,
            &command,
        ) {
            Ok(frame) => frame,
            Err(_) => {
                return self.respond(endpoint, native, now, StatusCode::Internal, false);
            }
        };
        if endpoint.try_send_command(frame).is_err() {
            return self.respond(endpoint, native, now, StatusCode::Capacity, false);
        }

        self.command_sequence = sequence;
        self.descriptor = Some(descriptor);
        self.prefetch = Some(actor);
        self.realtime = None;
        self.schedule = None;
        self.commit = None;
        self.respond(endpoint, native, now, StatusCode::Ok, true)
    }

    fn commit(
        &mut self,
        endpoint: &mut DefaultServiceEndpoint,
        native: NativeRequest<'_>,
        now: DeviceCycle,
        latest_clock_probe_id: Option<u64>,
        safety_state: SafetyState,
    ) -> ServiceResponse {
        let request = match JobCommitRequest::decode(native.body) {
            Ok(request) => request,
            Err(_) => {
                return self.respond(endpoint, native, now, StatusCode::InvalidRequest, false);
            }
        };
        let Some(descriptor) = self.descriptor else {
            return self.respond(endpoint, native, now, StatusCode::NotFound, false);
        };
        if !matches!(safety_state, SafetyState::Configured | SafetyState::Armed) {
            return self.respond(endpoint, native, now, StatusCode::ForbiddenState, false);
        }
        if native.frame.config_digest != descriptor.config_digest
            || request.boot_id != self.boot_id
            || request.prepare_id != descriptor.prepare_id
            || request.partition_digest != descriptor.partition.object.content.digest
            || request.clock_probe_id != latest_clock_probe_id.unwrap_or(0)
        {
            return self.respond(endpoint, native, now, StatusCode::Conflict, false);
        }
        if request.policy != JobNetworkPolicy::NetworkAttended {
            return self.respond(endpoint, native, now, StatusCode::Unsupported, false);
        }
        if request.required_sync_tolerance_cycles > MAXIMUM_SYNC_TOLERANCE_CYCLES {
            return self.respond(endpoint, native, now, StatusCode::InvalidRequest, false);
        }
        let lead = request.local_start_cycle.0.checked_sub(now.0);
        let lease = request
            .lease_expiry_cycle
            .0
            .checked_sub(request.local_start_cycle.0);
        let prime_lead = request
            .local_start_cycle
            .0
            .checked_sub(request.abort_guard_cycle.0);
        if request.confirm_deadline_cycle.0 <= now.0
            || !lead.is_some_and(|lead| {
                (MINIMUM_START_LEAD_CYCLES..=MAXIMUM_START_HORIZON_CYCLES).contains(&lead)
            })
            || lease.is_none_or(|lease| lease > MAXIMUM_JOB_LEASE_CYCLES)
            || prime_lead.is_none_or(|lead| lead < selected::MOTION_MINIMUM_PRIME_LEAD_CYCLES)
        {
            return self.respond(endpoint, native, now, StatusCode::Deadline, false);
        }
        if let Some(installed) = self.commit {
            let status = if installed == request {
                StatusCode::Ok
            } else {
                StatusCode::Conflict
            };
            return self.respond(endpoint, native, now, status, status == StatusCode::Ok);
        }
        let prepared = self.schedule.is_some_and(|report| {
            report.state == JobScheduleState::Prepared
                && report.prepared_token == Some(request.prepared_token)
        });
        let preadmitted = self
            .realtime
            .is_some_and(|report| report.state == RealtimeJobState::Admitted && report.outstanding);
        let service_ready = self.prefetch.as_ref().is_some_and(|actor| {
            matches!(
                actor.status().state,
                ServiceJobState::Prefetching | ServiceJobState::Complete
            )
        });
        if !prepared || !preadmitted || !service_ready {
            return self.respond(endpoint, native, now, StatusCode::Busy, true);
        }
        if let Err(status) =
            self.enqueue(endpoint, descriptor, CoreJobCommand::Commit(request), now)
        {
            return self.respond(endpoint, native, now, status, false);
        }
        self.commit = Some(request);
        self.respond(endpoint, native, now, StatusCode::Ok, true)
    }

    fn confirm(
        &mut self,
        endpoint: &mut DefaultServiceEndpoint,
        native: NativeRequest<'_>,
        now: DeviceCycle,
        safety_state: SafetyState,
    ) -> ServiceResponse {
        let reference = match JobScheduleReference::decode(native.body) {
            Ok(reference) if reference.action == JobScheduleReferenceAction::Confirm => reference,
            _ => {
                return self.respond(endpoint, native, now, StatusCode::InvalidRequest, false);
            }
        };
        let Some(descriptor) = self.descriptor else {
            return self.respond(endpoint, native, now, StatusCode::NotFound, false);
        };
        if safety_state != SafetyState::Armed {
            return self.respond(endpoint, native, now, StatusCode::ForbiddenState, true);
        }
        let Some(commit) = self.commit else {
            return self.respond(endpoint, native, now, StatusCode::NotFound, false);
        };
        let expected =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit);
        if native.frame.config_digest != descriptor.config_digest || expected != Ok(reference) {
            return self.respond(endpoint, native, now, StatusCode::Conflict, false);
        }
        let Some(schedule) = self.schedule else {
            return self.respond(endpoint, native, now, StatusCode::Busy, true);
        };
        if matches!(
            schedule.state,
            JobScheduleState::Confirmed
                | JobScheduleState::Priming
                | JobScheduleState::Primed
                | JobScheduleState::Running
                | JobScheduleState::Complete
        ) {
            return self.respond(endpoint, native, now, StatusCode::Ok, true);
        }
        if schedule.state != JobScheduleState::Installed {
            return self.respond(endpoint, native, now, StatusCode::Busy, true);
        }
        if now.0 >= commit.confirm_deadline_cycle.0 {
            return self.respond(endpoint, native, now, StatusCode::Deadline, true);
        }
        if let Err(status) = self.enqueue(
            endpoint,
            descriptor,
            CoreJobCommand::Confirm(reference),
            now,
        ) {
            return self.respond(endpoint, native, now, status, false);
        }
        self.respond(endpoint, native, now, StatusCode::Ok, true)
    }

    fn abort(
        &mut self,
        endpoint: &mut DefaultServiceEndpoint,
        native: NativeRequest<'_>,
        now: DeviceCycle,
    ) -> ServiceResponse {
        let reference = match JobScheduleReference::decode(native.body) {
            Ok(reference) if reference.action == JobScheduleReferenceAction::Abort => reference,
            _ => {
                return self.respond(endpoint, native, now, StatusCode::InvalidRequest, false);
            }
        };
        let Some(descriptor) = self.descriptor else {
            return self.respond(endpoint, native, now, StatusCode::NotFound, false);
        };
        let Some(commit) = self.commit else {
            return self.respond(endpoint, native, now, StatusCode::NotFound, false);
        };
        let expected = JobScheduleReference::for_commit(JobScheduleReferenceAction::Abort, commit);
        if native.frame.config_digest != descriptor.config_digest || expected != Ok(reference) {
            return self.respond(endpoint, native, now, StatusCode::Conflict, false);
        }
        if self.schedule.is_some_and(|schedule| {
            matches!(
                schedule.state,
                JobScheduleState::Aborted | JobScheduleState::Expired
            )
        }) {
            return self.respond(endpoint, native, now, StatusCode::Ok, true);
        }
        if now.0 >= commit.abort_guard_cycle.0 {
            return self.respond(endpoint, native, now, StatusCode::Deadline, true);
        }
        if self.schedule.is_some_and(|schedule| {
            matches!(
                schedule.state,
                JobScheduleState::Priming
                    | JobScheduleState::Primed
                    | JobScheduleState::Running
                    | JobScheduleState::Complete
                    | JobScheduleState::Faulted
            )
        }) {
            return self.respond(endpoint, native, now, StatusCode::ForbiddenState, true);
        }
        if let Err(status) =
            self.enqueue(endpoint, descriptor, CoreJobCommand::Abort(reference), now)
        {
            return self.respond(endpoint, native, now, status, false);
        }
        self.respond(endpoint, native, now, StatusCode::Ok, true)
    }

    fn cancel(
        &mut self,
        endpoint: &mut DefaultServiceEndpoint,
        native: NativeRequest<'_>,
        now: DeviceCycle,
    ) -> ServiceResponse {
        let cancel = match JobCancelRequest::decode(native.body) {
            Ok(cancel) => cancel,
            Err(_) => {
                return self.respond(endpoint, native, now, StatusCode::InvalidRequest, false);
            }
        };
        let Some(descriptor) = self.descriptor else {
            return self.respond(endpoint, native, now, StatusCode::NotFound, false);
        };
        if native.frame.config_digest != descriptor.config_digest {
            return self.respond(endpoint, native, now, StatusCode::Conflict, false);
        }
        if descriptor.prepare_id != cancel.prepare_id {
            return self.respond(endpoint, native, now, StatusCode::Conflict, false);
        }
        if self.commit.is_some()
            && !self.schedule.is_some_and(|schedule| {
                matches!(
                    schedule.state,
                    JobScheduleState::Aborted | JobScheduleState::Expired
                )
            })
        {
            return self.respond(endpoint, native, now, StatusCode::ForbiddenState, true);
        }
        if self
            .realtime
            .is_some_and(|report| report.state == RealtimeJobState::Cancelled)
            && self
                .prefetch
                .as_ref()
                .is_some_and(|actor| actor.status().state == ServiceJobState::Cancelled)
        {
            return self.respond(endpoint, native, now, StatusCode::Ok, true);
        }
        let command = match (CoreJobCommand::Cancel {
            prepare_id: cancel.prepare_id,
        })
        .encode::<{ selected::JOB_AXES }>()
        {
            Ok(command) => command,
            Err(_) => return self.respond(endpoint, native, now, StatusCode::Internal, false),
        };
        let sequence = next_nonzero(self.command_sequence);
        let frame = match IntercoreFrame::new(
            FrameKind::Job,
            sequence,
            now,
            descriptor.config_digest,
            &command,
        ) {
            Ok(frame) => frame,
            Err(_) => return self.respond(endpoint, native, now, StatusCode::Internal, false),
        };
        if endpoint.try_send_command(frame).is_err() {
            return self.respond(endpoint, native, now, StatusCode::Capacity, false);
        }
        self.command_sequence = sequence;
        if let Some(actor) = self.prefetch.as_mut() {
            actor.cancel();
        }
        self.commit = None;
        self.schedule = None;
        self.respond(endpoint, native, now, StatusCode::Ok, true)
    }

    /// Advances storage by at most one verified chunk and never waits for credit.
    pub async fn prefetch_step(
        &mut self,
        cache: &mut selected::StorageBackend,
        endpoint: &mut DefaultServiceEndpoint,
    ) -> Result<(), ()> {
        let Some(actor) = self.prefetch.as_mut() else {
            return Ok(());
        };
        if actor.status().state != ServiceJobState::Prefetching {
            return Ok(());
        }
        actor
            .step(cache, endpoint)
            .await
            .map(|_| ())
            .map_err(|_| ())
    }

    /// Installs only a correlated, internally consistent report from core 1.
    pub fn observe_realtime(
        &mut self,
        config_digest: Digest,
        report: RealtimeJobReport,
    ) -> Result<(), ()> {
        let descriptor = self.descriptor.ok_or(())?;
        if config_digest != descriptor.config_digest
            || report.prepare_id != descriptor.prepare_id
            || report.total_blocks != descriptor.block_count
        {
            return Err(());
        }
        if matches!(
            report.state,
            RealtimeJobState::Cancelled | RealtimeJobState::Faulted
        ) && let Some(prefetch) = self.prefetch.as_mut()
        {
            prefetch.cancel();
        }
        self.realtime = Some(report);
        Ok(())
    }

    /// Installs only a report matching this boot, descriptor, and exact commit.
    pub fn observe_schedule(
        &mut self,
        config_digest: Digest,
        report: JobScheduleReport,
    ) -> Result<(), ()> {
        let descriptor = self.descriptor.ok_or(())?;
        if config_digest != descriptor.config_digest {
            return Err(());
        }
        if let Some(commit) = self.commit {
            if report.state == JobScheduleState::Prepared {
                let expected = alumina_job::PreparedJobToken::derive::<{ selected::JOB_AXES }>(
                    self.boot_id,
                    descriptor,
                )
                .map_err(|_| ())?;
                if report.prepared_token != Some(expected) {
                    return Err(());
                }
            } else if !schedule_matches_commit(report, commit) {
                return Err(());
            }
        } else {
            let expected = alumina_job::PreparedJobToken::derive::<{ selected::JOB_AXES }>(
                self.boot_id,
                descriptor,
            )
            .map_err(|_| ())?;
            if !matches!(
                (report.state, report.fault),
                (
                    JobScheduleState::Prepared,
                    alumina_job::JobScheduleFault::None
                ) | (
                    JobScheduleState::Faulted,
                    alumina_job::JobScheduleFault::SafetyStop
                )
            ) || report.prepared_token != Some(expected)
            {
                return Err(());
            }
        }
        if let Some(previous) = self.schedule
            && !schedule_report_advances(previous, report)
        {
            return Err(());
        }
        self.schedule = Some(report);
        Ok(())
    }

    /// Bounded state exported by the heartbeat without leaking actor internals.
    pub fn clock_facts(&self, now: DeviceCycle) -> ClockJobFacts {
        let Some(schedule) = self.schedule else {
            return ClockJobFacts::default();
        };
        let prepared = matches!(
            schedule.state,
            JobScheduleState::Prepared
                | JobScheduleState::Installed
                | JobScheduleState::Confirmed
                | JobScheduleState::Priming
                | JobScheduleState::Primed
                | JobScheduleState::Running
        );
        let committed = matches!(
            schedule.state,
            JobScheduleState::Installed
                | JobScheduleState::Confirmed
                | JobScheduleState::Priming
                | JobScheduleState::Primed
                | JobScheduleState::Running
        );
        ClockJobFacts {
            prepared,
            committed,
            running: schedule.state == JobScheduleState::Running,
            queue_horizon_cycles: schedule.local_start_cycle.0.saturating_sub(now.0),
        }
    }

    /// Immediate local veto while any nonquiescent job owns cache/work state.
    pub fn excludes_storage_mutation(&self, endpoint: &DefaultServiceEndpoint) -> bool {
        self.descriptor.is_some() && !self.replaceable(endpoint)
    }

    /// Installs the sole durably authorized configuration identity used for
    /// subsequent job admission. Zero explicitly revokes admission.
    pub fn set_active_config(&mut self, digest: Digest) {
        self.active_config = digest;
    }

    /// Prevents a new job prepare while configuration ownership is changing.
    pub fn set_configuration_transition(&mut self, active: bool) {
        self.configuration_transition = active;
    }

    fn replaceable(&self, endpoint: &DefaultServiceEndpoint) -> bool {
        let Some(service) = self.prefetch.as_ref().map(ServicePrefetch::status) else {
            return true;
        };
        let service_terminal = matches!(
            service.state,
            ServiceJobState::Cancelled | ServiceJobState::Faulted | ServiceJobState::Complete
        );
        let realtime_terminal = self.realtime.is_some_and(|report| {
            matches!(
                report.state,
                RealtimeJobState::Cancelled
                    | RealtimeJobState::Faulted
                    | RealtimeJobState::Complete
            ) && !report.outstanding
        });
        let schedule_terminal = self.schedule.is_none_or(|report| {
            matches!(
                report.state,
                JobScheduleState::Aborted
                    | JobScheduleState::Expired
                    | JobScheduleState::Complete
                    | JobScheduleState::Faulted
            )
        });
        service_terminal
            && realtime_terminal
            && schedule_terminal
            && endpoint.work_depth() == 0
            && self.realtime.is_some_and(|report| report.queue_depth == 0)
    }

    fn respond(
        &self,
        endpoint: &DefaultServiceEndpoint,
        native: NativeRequest<'_>,
        now: DeviceCycle,
        status: StatusCode,
        include_report: bool,
    ) -> ServiceResponse {
        if !include_report {
            return ServiceResponse::native(native, now, status, &[])
                .unwrap_or_else(|_| ServiceResponse::invalid_native());
        }
        let service = match self.prefetch.as_ref() {
            Some(actor) => match ServiceJobReport::from_status(
                actor.status(),
                endpoint.work_free_capacity(),
                endpoint.work_depth(),
            ) {
                Ok(report) => Some(report),
                Err(_) => {
                    return ServiceResponse::native(native, now, StatusCode::Internal, &[])
                        .unwrap_or_else(|_| ServiceResponse::invalid_native());
                }
            },
            None => None,
        };
        let report = JobStatusReport {
            service,
            realtime: self.realtime,
            schedule: if service.is_some() && self.realtime.is_some() {
                self.schedule
            } else {
                None
            },
        };
        let body = match report.encode() {
            Ok(body) => body,
            Err(_) => {
                return ServiceResponse::native(native, now, StatusCode::Internal, &[])
                    .unwrap_or_else(|_| ServiceResponse::invalid_native());
            }
        };
        ServiceResponse::native(native, now, status, &body)
            .unwrap_or_else(|_| ServiceResponse::invalid_native())
    }

    fn enqueue(
        &mut self,
        endpoint: &mut DefaultServiceEndpoint,
        descriptor: JobDescriptor,
        command: CoreJobCommand,
        now: DeviceCycle,
    ) -> Result<(), StatusCode> {
        let command = command
            .encode::<{ selected::JOB_AXES }>()
            .map_err(|_| StatusCode::Internal)?;
        let sequence = next_nonzero(self.command_sequence);
        let frame = IntercoreFrame::new(
            FrameKind::Job,
            sequence,
            now,
            descriptor.config_digest,
            &command,
        )
        .map_err(|_| StatusCode::Internal)?;
        endpoint
            .try_send_command(frame)
            .map_err(|_| StatusCode::Capacity)?;
        self.command_sequence = sequence;
        Ok(())
    }
}

/// Sole core-1 owner of independent validation and the next pre-admitted block.
pub struct RealtimeJobService {
    descriptor: Option<JobDescriptor>,
    job: Option<RealtimeJob<{ selected::JOB_AXES }>>,
    admitted: Option<AdmittedBlock<{ selected::JOB_AXES }>>,
    lookahead: Option<AdmittedBlock<{ selected::JOB_AXES }>>,
    schedule: Option<alumina_job::PreparedJobSchedule>,
    report_sequence: u32,
    active_config: Digest,
}

impl RealtimeJobService {
    /// Starts with no job authority and no owned work.
    pub const fn new() -> Self {
        Self {
            descriptor: None,
            job: None,
            admitted: None,
            lookahead: None,
            schedule: None,
            report_sequence: 0,
            active_config: Digest::ZERO,
        }
    }

    /// Installs only the configuration identity authorized after durable
    /// service-core commit; zero revokes all new job admission.
    pub fn set_active_config(&mut self, digest: Digest) {
        self.active_config = digest;
    }

    /// Applies one exact ordered prepare/install/confirm/abort/cancel command.
    pub fn apply_command(
        &mut self,
        endpoint: &mut DefaultRealtimeEndpoint,
        frame: &IntercoreFrame<{ alumina_runtime::COMMAND_PAYLOAD_BYTES }>,
        now: DeviceCycle,
        safety_state: SafetyState,
        deadline_healthy: bool,
    ) -> Result<(), ()> {
        frame.validate(FrameKind::Job).map_err(|_| ())?;
        let payload = frame.payload().map_err(|_| ())?;
        let command = CoreJobCommand::decode::<{ selected::JOB_AXES }>(payload).map_err(|_| ())?;
        match command {
            CoreJobCommand::Prepare {
                boot_id,
                descriptor,
            } => {
                if frame.header().config_digest != descriptor.config_digest
                    || descriptor.capability_digest != selected::PACKAGE.board.capability_digest
                    || descriptor.capability_digest.is_zero()
                    || !selected::PACKAGE.armable
                    || !selected::MOTION_OUTPUT_QUALIFIED
                    || self.active_config.is_zero()
                    || descriptor.config_digest != self.active_config
                {
                    return Err(());
                }
                if self.descriptor == Some(descriptor) {
                    if !self
                        .schedule
                        .is_some_and(|schedule| schedule.boot_id() == boot_id)
                    {
                        return Err(());
                    }
                    return self.publish_report(endpoint, now);
                }
                if self.job.as_ref().is_some_and(|job| {
                    matches!(
                        job.status().state,
                        RealtimeJobState::Cancelled
                            | RealtimeJobState::Faulted
                            | RealtimeJobState::Complete
                    )
                }) && self.admitted.is_none()
                    && self.lookahead.is_none()
                    && endpoint.work_depth() == 0
                {
                    self.job = None;
                    self.descriptor = None;
                    self.schedule = None;
                }
                if self.job.is_some() || self.admitted.is_some() || self.lookahead.is_some() {
                    return Err(());
                }
                self.job = Some(RealtimeJob::prepare(descriptor).map_err(|_| ())?);
                self.schedule = Some(
                    alumina_job::PreparedJobSchedule::prepare::<{ selected::JOB_AXES }>(
                        boot_id, descriptor,
                    )
                    .map_err(|_| ())?,
                );
                self.descriptor = Some(descriptor);
            }
            CoreJobCommand::Cancel { prepare_id } => {
                let descriptor = self.descriptor.ok_or(())?;
                if descriptor.prepare_id != prepare_id {
                    return Err(());
                }
                let job = self.job.as_mut().ok_or(())?;
                if !matches!(
                    job.status().state,
                    RealtimeJobState::Cancelled | RealtimeJobState::Faulted
                ) {
                    job.cancel();
                    self.admitted = None;
                    self.lookahead = None;
                    job.drain(endpoint).map_err(|_| ())?;
                }
                self.schedule = None;
            }
            CoreJobCommand::Commit(commit) => {
                let descriptor = self.descriptor.ok_or(())?;
                if frame.header().config_digest != descriptor.config_digest {
                    return Err(());
                }
                let required = if descriptor.block_count == 1 { 1 } else { 2 };
                let cache_ready = self.preadmitted_count() >= required
                    && self
                        .job
                        .as_ref()
                        .is_some_and(|job| job.status().state == RealtimeJobState::Admitted);
                let admission = JobScheduleAdmission {
                    now,
                    active_config: self.active_config,
                    minimum_lead_cycles: MINIMUM_START_LEAD_CYCLES,
                    maximum_start_horizon_cycles: MAXIMUM_START_HORIZON_CYCLES,
                    maximum_lease_cycles: MAXIMUM_JOB_LEASE_CYCLES,
                    maximum_sync_tolerance_cycles: MAXIMUM_SYNC_TOLERANCE_CYCLES,
                    minimum_prime_lead_cycles: selected::MOTION_MINIMUM_PRIME_LEAD_CYCLES,
                    cache_ready,
                    safety_ready: matches!(
                        safety_state,
                        SafetyState::Configured | SafetyState::Armed
                    ) && deadline_healthy,
                    autonomous_allowed: false,
                };
                self.schedule
                    .as_mut()
                    .ok_or(())?
                    .install(commit, admission)
                    .map_err(|_| ())?;
            }
            CoreJobCommand::Confirm(reference) => {
                if frame.header().config_digest != self.descriptor.ok_or(())?.config_digest {
                    return Err(());
                }
                if safety_state != SafetyState::Armed || !deadline_healthy {
                    return Err(());
                }
                self.schedule
                    .as_mut()
                    .ok_or(())?
                    .confirm(reference, now)
                    .map_err(|_| ())?;
            }
            CoreJobCommand::Abort(reference) => {
                if frame.header().config_digest != self.descriptor.ok_or(())?.config_digest {
                    return Err(());
                }
                self.schedule
                    .as_mut()
                    .ok_or(())?
                    .abort(reference, now)
                    .map_err(|_| ())?;
            }
        }
        self.publish_report(endpoint, now)
    }

    /// Independently validates and retains the next block while the bounded
    /// execution window still has ownership capacity.
    pub fn preadmit(
        &mut self,
        endpoint: &mut DefaultRealtimeEndpoint,
        now: DeviceCycle,
    ) -> Result<(), ()> {
        if self.lookahead.is_some() {
            return Ok(());
        }
        let Some(job) = self.job.as_mut() else {
            return Ok(());
        };
        if !matches!(
            job.status().state,
            RealtimeJobState::Prepared | RealtimeJobState::Admitted
        ) {
            return Ok(());
        }
        let mut changed = false;
        loop {
            match job.poll(endpoint).map_err(|_| ())? {
                RealtimePoll::Empty | RealtimePoll::Outstanding => break,
                RealtimePoll::Block(admitted) => {
                    if self.admitted.is_none() {
                        self.admitted = Some(admitted);
                    } else if self.lookahead.is_none() {
                        self.lookahead = Some(admitted);
                    } else {
                        return Err(());
                    }
                    changed = true;
                    if self.lookahead.is_some() {
                        break;
                    }
                }
            }
        }
        if changed {
            self.publish_report(endpoint, now)
        } else {
            Ok(())
        }
    }

    /// Exact immutable descriptor retained for the current boot-local job.
    pub const fn descriptor(&self) -> Option<JobDescriptor> {
        self.descriptor
    }

    /// Whether an installed schedule and its required one- or two-block initial
    /// planning window are present for the local arm transition.
    pub fn ready_to_arm(&self) -> bool {
        let required = self.descriptor.map_or(
            0,
            |descriptor| if descriptor.block_count == 1 { 1 } else { 2 },
        );
        self.preadmitted_count() >= required
            && self
                .job
                .as_ref()
                .is_some_and(|job| job.status().state == RealtimeJobState::Admitted)
            && self
                .schedule
                .is_some_and(|schedule| schedule.report().state == JobScheduleState::Installed)
    }

    /// Transfers the next pre-admitted block into the physical motion owner.
    /// The job actor retains every ordered token until the same block is
    /// returned by its exact, independent physical-commit barrier.
    pub fn take_admitted(&mut self) -> Option<AdmittedBlock<{ selected::JOB_AXES }>> {
        let admitted = self.admitted.take();
        self.admitted = self.lookahead.take();
        admitted
    }

    fn preadmitted_count(&self) -> usize {
        usize::from(self.admitted.is_some()) + usize::from(self.lookahead.is_some())
    }

    /// Completes the oldest outstanding token only after the motion owner
    /// returns that exact block following its physical output prefix.
    pub fn acknowledge_executed(
        &mut self,
        endpoint: &mut DefaultRealtimeEndpoint,
        now: DeviceCycle,
        admitted: AdmittedBlock<{ selected::JOB_AXES }>,
    ) -> Result<RealtimeJobState, ()> {
        let state = self
            .job
            .as_mut()
            .ok_or(())?
            .acknowledge(admitted)
            .map_err(|_| ())?
            .state;
        self.publish_report(endpoint, now)?;
        Ok(state)
    }

    /// Marks the committed local schedule complete only after the normal
    /// terminal output image has been physically acknowledged.
    pub fn complete_schedule(
        &mut self,
        endpoint: &mut DefaultRealtimeEndpoint,
        now: DeviceCycle,
    ) -> Result<(), ()> {
        self.schedule
            .as_mut()
            .ok_or(())?
            .complete(now)
            .map_err(|_| ())?;
        self.publish_report(endpoint, now)
    }

    /// Records that the sole physical owner accepted the required future
    /// horizon after the distributed abort guard closed.
    pub fn mark_hardware_primed(
        &mut self,
        endpoint: &mut DefaultRealtimeEndpoint,
        now: DeviceCycle,
    ) -> Result<(), ()> {
        self.schedule
            .as_mut()
            .ok_or(())?
            .mark_primed(now)
            .map_err(|_| ())?;
        self.publish_report(endpoint, now)
    }

    /// Current local schedule lifecycle for safety-state reconciliation.
    pub fn schedule_state(&self) -> Option<JobScheduleState> {
        self.schedule.map(|schedule| schedule.report().state)
    }

    /// Next exact schedule-only wake cycle, independent of motion edges.
    pub fn next_schedule_deadline(&self) -> Option<DeviceCycle> {
        let report = self.schedule?.report();
        match report.state {
            JobScheduleState::Installed => Some(report.confirm_deadline_cycle),
            JobScheduleState::Confirmed => Some(report.abort_guard_cycle),
            JobScheduleState::Priming | JobScheduleState::Primed => Some(report.local_start_cycle),
            JobScheduleState::Running => Some(report.lease_expiry_cycle),
            JobScheduleState::Prepared
            | JobScheduleState::Aborted
            | JobScheduleState::Expired
            | JobScheduleState::Complete
            | JobScheduleState::Faulted => None,
        }
    }

    /// Re-publishes the latest exact report; ordinary queue pressure may drop it.
    pub fn publish_report(
        &mut self,
        endpoint: &mut DefaultRealtimeEndpoint,
        now: DeviceCycle,
    ) -> Result<(), ()> {
        let Some(job) = self.job.as_ref() else {
            return Ok(());
        };
        let descriptor = self.descriptor.ok_or(())?;
        let report =
            RealtimeJobReport::from_status(job.status(), endpoint.work_depth()).map_err(|_| ())?;
        let payload = report.encode().map_err(|_| ())?;
        self.publish_payload(endpoint, now, descriptor.config_digest, &payload)?;
        if let Some(schedule) = self.schedule {
            let payload = schedule.report().encode().map_err(|_| ())?;
            self.publish_payload(endpoint, now, descriptor.config_digest, &payload)?;
        }
        Ok(())
    }

    /// Advances clock-only schedule transitions on core 1.
    pub fn advance_schedule(
        &mut self,
        endpoint: &mut DefaultRealtimeEndpoint,
        now: DeviceCycle,
    ) -> Result<JobScheduleAction, ()> {
        let Some(schedule) = self.schedule.as_mut() else {
            return Ok(JobScheduleAction::None);
        };
        let action = schedule.advance(now);
        if action != JobScheduleAction::None {
            self.publish_report(endpoint, now)?;
        }
        Ok(action)
    }

    /// Invalidates all core-1 job ownership after the hardware owner has
    /// synchronously applied its board-safe transaction.
    pub fn local_safety_fault(
        &mut self,
        endpoint: &mut DefaultRealtimeEndpoint,
        now: DeviceCycle,
    ) -> Result<(), ()> {
        if let Some(job) = self.job.as_mut() {
            job.fault();
        }
        self.admitted = None;
        self.lookahead = None;
        if let Some(job) = self.job.as_ref()
            && matches!(
                job.status().state,
                RealtimeJobState::Cancelled | RealtimeJobState::Faulted
            )
        {
            job.drain(endpoint).map_err(|_| ())?;
        }
        if let Some(schedule) = self.schedule.as_mut()
            && !matches!(
                schedule.report().state,
                JobScheduleState::Aborted
                    | JobScheduleState::Expired
                    | JobScheduleState::Complete
                    | JobScheduleState::Faulted
            )
        {
            schedule.fault_safety_stop().map_err(|_| ())?;
        }
        self.publish_report(endpoint, now)
    }

    /// Cancels all core-1 job ownership after an operator-requested safe stop.
    pub fn local_stop(
        &mut self,
        endpoint: &mut DefaultRealtimeEndpoint,
        now: DeviceCycle,
    ) -> Result<(), ()> {
        if let Some(job) = self.job.as_mut() {
            job.cancel();
        }
        self.admitted = None;
        self.lookahead = None;
        if let Some(job) = self.job.as_ref()
            && matches!(
                job.status().state,
                RealtimeJobState::Cancelled | RealtimeJobState::Faulted
            )
        {
            job.drain(endpoint).map_err(|_| ())?;
        }
        if let Some(schedule) = self.schedule.as_mut()
            && !matches!(
                schedule.report().state,
                JobScheduleState::Aborted
                    | JobScheduleState::Expired
                    | JobScheduleState::Complete
                    | JobScheduleState::Faulted
            )
        {
            schedule.fault_safety_stop().map_err(|_| ())?;
        }
        self.publish_report(endpoint, now)
    }

    fn publish_payload(
        &mut self,
        endpoint: &mut DefaultRealtimeEndpoint,
        now: DeviceCycle,
        config_digest: Digest,
        payload: &[u8],
    ) -> Result<(), ()> {
        let sequence = next_nonzero(self.report_sequence);
        let frame = IntercoreFrame::new(FrameKind::Job, sequence, now, config_digest, payload)
            .map_err(|_| ())?;
        if endpoint.try_publish_telemetry(frame).is_ok() {
            self.report_sequence = sequence;
        }
        Ok(())
    }

    /// Whether prepared or admitted ownership must be reported to safety policy.
    pub fn active(&self) -> bool {
        let stream_active = self.job.as_ref().is_some_and(|job| {
            matches!(
                job.status().state,
                RealtimeJobState::Prepared | RealtimeJobState::Admitted
            )
        });
        let schedule_active = self.schedule.is_some_and(|schedule| {
            matches!(
                schedule.report().state,
                JobScheduleState::Prepared
                    | JobScheduleState::Installed
                    | JobScheduleState::Confirmed
                    | JobScheduleState::Priming
                    | JobScheduleState::Primed
                    | JobScheduleState::Running
            )
        });
        stream_active || schedule_active
    }

    /// Whether any terminal or active job remains available for status replay.
    pub const fn has_job(&self) -> bool {
        self.job.is_some()
    }
}

impl Default for RealtimeJobService {
    fn default() -> Self {
        Self::new()
    }
}

fn schedule_matches_commit(report: JobScheduleReport, commit: JobCommitRequest) -> bool {
    report.policy == Some(commit.policy)
        && report.prepared_token.is_none()
        && report.local_start_cycle == commit.local_start_cycle
        && report.confirm_deadline_cycle == commit.confirm_deadline_cycle
        && report.abort_guard_cycle == commit.abort_guard_cycle
        && report.lease_expiry_cycle == commit.lease_expiry_cycle
        && report.commit_id == commit.commit_id.as_bytes()
}

fn schedule_report_advances(previous: JobScheduleReport, next: JobScheduleReport) -> bool {
    if previous == next {
        return true;
    }
    match previous.state {
        JobScheduleState::Prepared => next.state != JobScheduleState::Prepared,
        JobScheduleState::Installed => matches!(
            next.state,
            JobScheduleState::Confirmed
                | JobScheduleState::Priming
                | JobScheduleState::Primed
                | JobScheduleState::Running
                | JobScheduleState::Aborted
                | JobScheduleState::Expired
                | JobScheduleState::Faulted
        ),
        JobScheduleState::Confirmed => matches!(
            next.state,
            JobScheduleState::Priming
                | JobScheduleState::Primed
                | JobScheduleState::Running
                | JobScheduleState::Aborted
                | JobScheduleState::Faulted
        ),
        JobScheduleState::Priming => matches!(
            next.state,
            JobScheduleState::Primed | JobScheduleState::Running | JobScheduleState::Faulted
        ),
        JobScheduleState::Primed => matches!(
            next.state,
            JobScheduleState::Running | JobScheduleState::Faulted
        ),
        JobScheduleState::Running => matches!(
            next.state,
            JobScheduleState::Complete | JobScheduleState::Faulted
        ),
        JobScheduleState::Aborted
        | JobScheduleState::Expired
        | JobScheduleState::Complete
        | JobScheduleState::Faulted => false,
    }
}

fn job_error_status<E>(error: JobError<E>) -> StatusCode {
    match error {
        JobError::Descriptor(_) => StatusCode::InvalidRequest,
        JobError::Machine(_) => StatusCode::Integrity,
        JobError::State => StatusCode::Busy,
        JobError::AdmissionToken => StatusCode::Internal,
        JobError::Storage(error) => match error {
            ProvisionedCacheError::Media(MediaError::PublishedNotFound) => StatusCode::NotFound,
            ProvisionedCacheError::Media(MediaError::Corrupt(_))
            | ProvisionedCacheError::Locator(_)
            | ProvisionedCacheError::MediaIdentity => StatusCode::Integrity,
            ProvisionedCacheError::NotDiscovered | ProvisionedCacheError::NotMounted => {
                StatusCode::Unsupported
            }
            ProvisionedCacheError::Conflict | ProvisionedCacheError::RecoveryIntent => {
                StatusCode::Conflict
            }
            ProvisionedCacheError::Geometry | ProvisionedCacheError::Mutation(_) => {
                StatusCode::InvalidRequest
            }
            ProvisionedCacheError::Arithmetic => StatusCode::Capacity,
            ProvisionedCacheError::Device(_)
            | ProvisionedCacheError::DeviceStateUnavailable
            | ProvisionedCacheError::Media(_) => StatusCode::Internal,
        },
    }
}

const fn next_nonzero(value: u32) -> u32 {
    let next = value.wrapping_add(1);
    if next == 0 { 1 } else { next }
}
