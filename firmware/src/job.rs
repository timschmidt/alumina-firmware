//! Core-0 ownership and authenticated admission for cached machine jobs.

use alumina_job::{
    AdmittedBlock, CoreJobCommand, JobCancelRequest, JobDescriptor, JobError, JobStatusReport,
    RealtimeJob, RealtimeJobReport, RealtimeJobState, RealtimePoll, ServiceJobReport,
    ServiceJobState, ServicePrefetch,
};
use alumina_protocol::{DeviceCycle, FrameKind, Operation, StatusCode};
use alumina_runtime::{DefaultRealtimeEndpoint, DefaultServiceEndpoint, IntercoreFrame};
use alumina_service::{NativeRequest, ServiceRequest, ServiceRequestKind, ServiceResponse};
use alumina_storage::media::MediaError;
use alumina_storage::provisioning::ProvisionedCacheError;

use crate::hardware::selected;

/// Sole service-core owner of a publication cursor and latest RT observation.
pub struct JobService {
    descriptor: Option<JobDescriptor>,
    prefetch: Option<ServicePrefetch<{ selected::JOB_AXES }>>,
    realtime: Option<RealtimeJobReport>,
    command_sequence: u32,
}

impl JobService {
    /// Starts with no prepared job and no storage cursor.
    pub const fn new() -> Self {
        Self {
            descriptor: None,
            prefetch: None,
            realtime: None,
            command_sequence: 0,
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
    ) -> ServiceResponse {
        let Ok(native) = NativeRequest::decode(request.bytes()) else {
            return ServiceResponse::invalid_native();
        };
        if native.frame.kind != FrameKind::Job {
            return ServiceResponse::invalid_native();
        }
        match native.message.operation {
            Operation::JobPrepare => self.prepare(cache, endpoint, native, now).await,
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
    ) -> ServiceResponse {
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
        if expected_capability.is_zero() {
            return self.respond(endpoint, native, now, StatusCode::Unsupported, false);
        }
        if descriptor.capability_digest != expected_capability {
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
        let command = match CoreJobCommand::Prepare(descriptor).encode::<{ selected::JOB_AXES }>() {
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
        if descriptor.prepare_id != cancel.prepare_id {
            return self.respond(endpoint, native, now, StatusCode::Conflict, false);
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
    pub fn observe_realtime(&mut self, report: RealtimeJobReport) -> Result<(), ()> {
        let descriptor = self.descriptor.ok_or(())?;
        if report.prepare_id != descriptor.prepare_id
            || report.total_blocks != descriptor.block_count
        {
            return Err(());
        }
        self.realtime = Some(report);
        Ok(())
    }

    /// Immediate local veto while any nonquiescent job owns cache/work state.
    pub fn excludes_storage_mutation(&self, endpoint: &DefaultServiceEndpoint) -> bool {
        self.descriptor.is_some() && !self.replaceable(endpoint)
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
        service_terminal
            && realtime_terminal
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
}

impl Default for JobService {
    fn default() -> Self {
        Self::new()
    }
}

/// Sole core-1 owner of independent validation and the pre-admitted block.
pub struct RealtimeJobService {
    descriptor: Option<JobDescriptor>,
    job: Option<RealtimeJob<{ selected::JOB_AXES }>>,
    admitted: Option<AdmittedBlock<{ selected::JOB_AXES }>>,
    report_sequence: u32,
}

impl RealtimeJobService {
    /// Starts with no job authority and no owned work.
    pub const fn new() -> Self {
        Self {
            descriptor: None,
            job: None,
            admitted: None,
            report_sequence: 0,
        }
    }

    /// Applies one exact ordered prepare/cancel command.
    pub fn apply_command(
        &mut self,
        endpoint: &mut DefaultRealtimeEndpoint,
        frame: &IntercoreFrame<{ alumina_runtime::COMMAND_PAYLOAD_BYTES }>,
        now: DeviceCycle,
    ) -> Result<(), ()> {
        frame.validate(FrameKind::Job).map_err(|_| ())?;
        let payload = frame.payload().map_err(|_| ())?;
        let command = CoreJobCommand::decode::<{ selected::JOB_AXES }>(payload).map_err(|_| ())?;
        match command {
            CoreJobCommand::Prepare(descriptor) => {
                if frame.header().config_digest != descriptor.config_digest
                    || descriptor.capability_digest != selected::PACKAGE.board.capability_digest
                    || descriptor.capability_digest.is_zero()
                {
                    return Err(());
                }
                if self.descriptor == Some(descriptor) {
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
                    && endpoint.work_depth() == 0
                {
                    self.job = None;
                    self.descriptor = None;
                }
                if self.job.is_some() || self.admitted.is_some() {
                    return Err(());
                }
                self.job = Some(RealtimeJob::prepare(descriptor).map_err(|_| ())?);
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
                    job.drain(endpoint).map_err(|_| ())?;
                }
            }
        }
        self.publish_report(endpoint, now)
    }

    /// Independently validates and retains at most the first unexecuted block.
    pub fn preadmit(
        &mut self,
        endpoint: &mut DefaultRealtimeEndpoint,
        now: DeviceCycle,
    ) -> Result<(), ()> {
        if self.admitted.is_some() {
            return Ok(());
        }
        let Some(job) = self.job.as_mut() else {
            return Ok(());
        };
        if job.status().state != RealtimeJobState::Prepared {
            return Ok(());
        }
        match job.poll(endpoint).map_err(|_| ())? {
            RealtimePoll::Empty | RealtimePoll::Outstanding => Ok(()),
            RealtimePoll::Block(admitted) => {
                self.admitted = Some(admitted);
                self.publish_report(endpoint, now)
            }
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
        let sequence = next_nonzero(self.report_sequence);
        let frame = IntercoreFrame::new(
            FrameKind::Job,
            sequence,
            now,
            descriptor.config_digest,
            &payload,
        )
        .map_err(|_| ())?;
        if endpoint.try_publish_telemetry(frame).is_ok() {
            self.report_sequence = sequence;
        }
        Ok(())
    }

    /// Whether prepared or admitted ownership must be reported to safety policy.
    pub fn active(&self) -> bool {
        self.job.as_ref().is_some_and(|job| {
            matches!(
                job.status().state,
                RealtimeJobState::Prepared | RealtimeJobState::Admitted
            )
        })
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
