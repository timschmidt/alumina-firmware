//! Deterministic configuration, immutable-cache, and cached-job HTTP fixture state.

use alumina_clock::BootId;
use alumina_config::{
    CONFIGURATION_COORDINATOR_STATUS_BYTES, ConfigurationCoordinatorFault,
    ConfigurationCoordinatorFlags, ConfigurationCoordinatorPhase, ConfigurationCoordinatorStatus,
    ConfigurationDocumentView, ConfigurationError, ConfigurationFaultCode,
    MAX_CONFIGURATION_RECORDS, RealtimeConfigurationReport, RealtimeConfigurationState,
};
use alumina_job::{
    DecodedMachineJobManifest, JobCancelRequest, JobCommitRequest, JobDescriptor,
    JobScheduleAction, JobScheduleAdmission, JobScheduleError, JobScheduleReference,
    JobScheduleState, JobStartObservation, JobStartObservationSource, JobStatusReport,
    PreparedJobSchedule, RealtimeJobReport, RealtimeJobState, ServiceJobReport, ServiceJobState,
};
use alumina_machine_ir::{
    BlockExpectation, EXECUTION_BLOCK_BYTES, ExecutionBlock, ExecutionKind, StreamTick,
};
use alumina_protocol::{DeviceCycle, DeviceId, Digest, FrameKind, Operation, StatusCode};
use alumina_service::{CACHE_LIMITS, NativeRequest, ServiceResponse};
use alumina_storage::{
    ChunkUploadHeader, ContentId, Error as StorageError, FinalizeUploadRequest, MutationContext,
    ObjectKind, PublishedObject, UploadPlan, sha256,
};

use crate::capability;
use crate::configuration::representative_tinybee_configuration_bytes;
use crate::{CacheService, Error as CacheError};

const JOB_AXES: usize = 2;
const ACTIVE_CONFIGURATION_TRANSACTION_ID: u64 = 1;
const SIMULATED_OUTPUT_TOKEN: u32 = 1;

/// Simulator-owned active configuration and exact encoded status.
pub(crate) struct SimulatedActiveConfiguration {
    digest: Digest,
    status: [u8; CONFIGURATION_COORDINATOR_STATUS_BYTES],
}

impl SimulatedActiveConfiguration {
    pub(crate) fn new() -> Result<Self, ConfigurationError> {
        let package = capability::package();
        let bytes = representative_tinybee_configuration_bytes(package.board.capability_digest)?;
        let digest = sha256(&bytes).digest;
        let view = ConfigurationDocumentView::decode::<MAX_CONFIGURATION_RECORDS>(
            &package, &bytes, digest,
        )?;
        let identity = view.identity();
        let realtime = RealtimeConfigurationReport {
            state: RealtimeConfigurationState::Active,
            transaction_id: ACTIVE_CONFIGURATION_TRANSACTION_ID,
            digest,
            total_bytes: identity.byte_len,
            consumed_bytes: identity.byte_len,
            summary: Some(identity.summary),
            fault: ConfigurationFaultCode::None,
            active_digest: digest,
            active_bytes: identity.byte_len,
            active_authorized: true,
        };
        let status = ConfigurationCoordinatorStatus {
            phase: ConfigurationCoordinatorPhase::Active,
            flags: ConfigurationCoordinatorFlags(
                ConfigurationCoordinatorFlags::JOBS_AUTHORIZED
                    | ConfigurationCoordinatorFlags::CORE0_VALID,
            ),
            fault: ConfigurationCoordinatorFault::None,
            operation_transaction_id: 0,
            operation_digest: Digest::ZERO,
            operation_bytes: 0,
            validated_bytes: 0,
            storage_chunks_read: 0,
            active_transaction_id: ACTIVE_CONFIGURATION_TRANSACTION_ID,
            active_digest: digest,
            active_bytes: identity.byte_len,
            summary: Some(identity.summary),
            realtime,
        }
        .encode()
        .expect("independently validated simulator configuration forms canonical active status");
        Ok(Self { digest, status })
    }

    pub(crate) const fn digest(&self) -> Digest {
        self.digest
    }

    pub(crate) const fn status(&self) -> &[u8; CONFIGURATION_COORDINATOR_STATUS_BYTES] {
        &self.status
    }
}

/// Host-only state behind authenticated storage and job frames.
pub(crate) struct SimulatedCachedJobService {
    device_id: DeviceId,
    boot_id: BootId,
    active_config: Digest,
    capability_digest: Digest,
    cache: CacheService,
    job: Option<SimulatedJob>,
}

impl SimulatedCachedJobService {
    pub(crate) fn new(device_id: DeviceId, boot_id: BootId, active_config: Digest) -> Self {
        Self {
            device_id,
            boot_id,
            active_config,
            capability_digest: capability::CAPABILITY_DIGEST,
            cache: CacheService::empty(CACHE_LIMITS)
                .expect("firmware cache limits are a valid simulator policy"),
            job: None,
        }
    }

    pub(crate) fn reboot(&mut self, boot_id: BootId) -> Result<(), CacheError> {
        self.boot_id = boot_id;
        self.job = None;
        self.cache.reboot()
    }

    pub(crate) fn dispatch(
        &mut self,
        native: NativeRequest<'_>,
        now: DeviceCycle,
        frequency_hz: u64,
        minimum_lead_cycles: u64,
        maximum_schedule_horizon_cycles: u64,
    ) -> Option<ServiceResponse> {
        match native.frame.kind {
            FrameKind::Storage => Some(self.dispatch_storage(native, now)),
            FrameKind::Job => Some(self.dispatch_job(
                native,
                now,
                frequency_hz,
                minimum_lead_cycles,
                maximum_schedule_horizon_cycles,
            )),
            _ => None,
        }
    }

    fn dispatch_storage(&mut self, native: NativeRequest<'_>, now: DeviceCycle) -> ServiceResponse {
        if native.frame.config_digest != Digest::ZERO {
            return native_response(native, now, StatusCode::InvalidRequest, &[]);
        }
        let mutation_forbidden = self
            .job
            .as_ref()
            .is_some_and(|job| !job.storage_mutation_safe());
        let (status, body) = match native.message.operation {
            Operation::StorageInspect => match PublishedObject::decode(native.body, CACHE_LIMITS) {
                Ok(expected) => match self.cache.inspect_published(expected) {
                    Ok(Some(publication)) => (StatusCode::Ok, publication.encode().to_vec()),
                    Ok(None) => (StatusCode::NotFound, Vec::new()),
                    Err(error) => (cache_error_status(error), Vec::new()),
                },
                Err(_) => (StatusCode::InvalidRequest, Vec::new()),
            },
            Operation::StorageBeginUpload if mutation_forbidden => {
                (StatusCode::ForbiddenState, Vec::new())
            }
            Operation::StorageBeginUpload => match UploadPlan::decode(native.body, CACHE_LIMITS) {
                Ok(plan) => match self
                    .cache
                    .begin_upload(plan, MutationContext::DISARMED_IDLE)
                {
                    Ok(progress) => (StatusCode::Ok, progress.encode().to_vec()),
                    Err(error) => (cache_error_status(error), Vec::new()),
                },
                Err(_) => (StatusCode::InvalidRequest, Vec::new()),
            },
            Operation::StoragePutChunk if mutation_forbidden => {
                (StatusCode::ForbiddenState, Vec::new())
            }
            Operation::StoragePutChunk => match decode_chunk(native) {
                Ok((header, bytes)) => match self.cache.put_chunk(
                    header.upload_id,
                    header.index,
                    header.content,
                    bytes,
                    MutationContext::DISARMED_IDLE,
                    None,
                ) {
                    Ok(progress) => (StatusCode::Ok, progress.encode().to_vec()),
                    Err(error) => (cache_error_status(error), Vec::new()),
                },
                Err(status) => (status, Vec::new()),
            },
            Operation::StorageFinalize if mutation_forbidden => {
                (StatusCode::ForbiddenState, Vec::new())
            }
            Operation::StorageFinalize => match FinalizeUploadRequest::decode(native.body) {
                Ok(request) => match self.cache.finalize_upload(
                    request.upload_id,
                    MutationContext::DISARMED_IDLE,
                    None,
                ) {
                    Ok(_) => (StatusCode::Ok, Vec::new()),
                    Err(error) => (cache_error_status(error), Vec::new()),
                },
                Err(_) => (StatusCode::InvalidRequest, Vec::new()),
            },
            _ => (StatusCode::Unsupported, Vec::new()),
        };
        native_response(native, now, status, &body)
    }

    fn dispatch_job(
        &mut self,
        native: NativeRequest<'_>,
        now: DeviceCycle,
        frequency_hz: u64,
        minimum_lead_cycles: u64,
        maximum_schedule_horizon_cycles: u64,
    ) -> ServiceResponse {
        if native.frame.config_digest != self.active_config {
            return native_response(native, now, StatusCode::Integrity, &[]);
        }
        if let Some(job) = self.job.as_mut() {
            job.advance(now);
        }
        let status = match native.message.operation {
            Operation::JobPrepare => self.prepare(native.body),
            Operation::JobStatus if native.body.is_empty() => Ok(()),
            Operation::JobCommit => self.install(
                native.body,
                now,
                frequency_hz,
                minimum_lead_cycles,
                maximum_schedule_horizon_cycles,
            ),
            Operation::JobConfirm => self.confirm(native.body, now),
            Operation::JobAbort => self.abort(native.body, now),
            Operation::JobCancel => self.cancel(native.body),
            _ => Err(StatusCode::Unsupported),
        };
        let body = self
            .job
            .as_ref()
            .map_or_else(JobStatusReport::default, SimulatedJob::report)
            .encode()
            .expect("simulator job state always forms one canonical status report");
        match status {
            Ok(()) => native_response(native, now, StatusCode::Ok, &body),
            Err(status) if status == StatusCode::Unsupported => {
                native_response(native, now, status, &[])
            }
            Err(status) => native_response(native, now, status, &body),
        }
    }

    fn prepare(&mut self, body: &[u8]) -> Result<(), StatusCode> {
        let descriptor =
            JobDescriptor::decode::<JOB_AXES>(body).map_err(|_| StatusCode::InvalidRequest)?;
        if descriptor.capability_digest != self.capability_digest
            || descriptor.config_digest != self.active_config
        {
            return Err(StatusCode::Integrity);
        }
        if let Some(job) = &self.job {
            return if job.descriptor == descriptor && !job.cancelled {
                Ok(())
            } else {
                Err(StatusCode::Conflict)
            };
        }
        let publication = self
            .cache
            .inspect_published(descriptor.partition)
            .map_err(cache_error_status)?
            .ok_or(StatusCode::NotFound)?;
        if publication != descriptor.partition {
            return Err(StatusCode::Integrity);
        }
        let bytes = self
            .cache
            .read_published(descriptor.partition.object.content)
            .map_err(cache_error_status)?;
        let validation = validate_partition(descriptor, &bytes)?;
        let schedule = PreparedJobSchedule::prepare::<JOB_AXES>(self.boot_id, descriptor)
            .map_err(|_| StatusCode::InvalidRequest)?;
        self.job = Some(SimulatedJob {
            descriptor,
            validation,
            schedule,
            cancelled: false,
        });
        Ok(())
    }

    fn install(
        &mut self,
        body: &[u8],
        now: DeviceCycle,
        frequency_hz: u64,
        minimum_lead_cycles: u64,
        maximum_schedule_horizon_cycles: u64,
    ) -> Result<(), StatusCode> {
        let commit = JobCommitRequest::decode(body).map_err(|_| StatusCode::InvalidRequest)?;
        self.validate_manifest(commit)?;
        let maximum_lease_cycles = frequency_hz
            .checked_mul(3_600)
            .ok_or(StatusCode::Capacity)?;
        let maximum_sync_tolerance_cycles = frequency_hz.max(1);
        let admission = JobScheduleAdmission {
            now,
            active_config: self.active_config,
            minimum_lead_cycles,
            maximum_start_horizon_cycles: maximum_schedule_horizon_cycles,
            maximum_lease_cycles,
            maximum_sync_tolerance_cycles,
            minimum_prime_lead_cycles: minimum_lead_cycles,
            cache_ready: true,
            safety_ready: true,
            autonomous_allowed: true,
        };
        self.job
            .as_mut()
            .ok_or(StatusCode::NotFound)?
            .schedule
            .install(commit, admission)
            .map(|_| ())
            .map_err(job_schedule_error_status)
    }

    fn confirm(&mut self, body: &[u8], now: DeviceCycle) -> Result<(), StatusCode> {
        let reference =
            JobScheduleReference::decode(body).map_err(|_| StatusCode::InvalidRequest)?;
        self.job
            .as_mut()
            .ok_or(StatusCode::NotFound)?
            .schedule
            .confirm(reference, now)
            .map(|_| ())
            .map_err(job_schedule_error_status)
    }

    fn abort(&mut self, body: &[u8], now: DeviceCycle) -> Result<(), StatusCode> {
        let reference =
            JobScheduleReference::decode(body).map_err(|_| StatusCode::InvalidRequest)?;
        self.job
            .as_mut()
            .ok_or(StatusCode::NotFound)?
            .schedule
            .abort(reference, now)
            .map(|_| ())
            .map_err(job_schedule_error_status)
    }

    fn cancel(&mut self, body: &[u8]) -> Result<(), StatusCode> {
        let request = JobCancelRequest::decode(body).map_err(|_| StatusCode::InvalidRequest)?;
        let job = self.job.as_mut().ok_or(StatusCode::NotFound)?;
        if request.prepare_id != job.descriptor.prepare_id {
            return Err(StatusCode::Conflict);
        }
        if job.schedule.report().state != JobScheduleState::Prepared && !job.cancelled {
            return Err(StatusCode::ForbiddenState);
        }
        job.cancelled = true;
        Ok(())
    }

    fn validate_manifest(&self, commit: JobCommitRequest) -> Result<(), StatusCode> {
        let job = self.job.as_ref().ok_or(StatusCode::NotFound)?;
        if job.cancelled {
            return Err(StatusCode::ForbiddenState);
        }
        let content = ContentId::from_sha256(commit.global_job_digest);
        let publication = self
            .cache
            .published_object(content)
            .map_err(cache_error_status)?
            .ok_or(StatusCode::NotFound)?;
        if publication.object.kind != ObjectKind::MachineJobManifest {
            return Err(StatusCode::Integrity);
        }
        let bytes = self
            .cache
            .read_published(publication.object.content)
            .map_err(cache_error_status)?;
        let manifest =
            DecodedMachineJobManifest::decode(&bytes).map_err(|_| StatusCode::Integrity)?;
        if manifest.global_job_digest() != commit.global_job_digest
            || manifest.participant_set_digest() != commit.participant_set_digest
            || manifest.global().network_policy != commit.policy
        {
            return Err(StatusCode::Integrity);
        }
        let descriptor = job.descriptor;
        let mut matching = None;
        for index in 0..manifest.participant_count() {
            let participant = manifest
                .participant(index)
                .map_err(|_| StatusCode::Integrity)?
                .ok_or(StatusCode::Integrity)?;
            if participant.device_id == self.device_id {
                matching = Some(participant);
                break;
            }
        }
        let participant = matching.ok_or(StatusCode::Integrity)?;
        if participant.stream_id != descriptor.stream_id
            || participant.capability_digest != descriptor.capability_digest
            || participant.config_digest != descriptor.config_digest
            || participant.partition_digest != descriptor.partition.object.content.digest
            || participant.partition_manifest_digest != descriptor.partition.manifest.digest
            || participant.partition_byte_len != descriptor.partition.object.byte_len
            || participant.block_count != descriptor.block_count
            || participant.axis_count != descriptor.axis_count
            || participant.execution_kind != descriptor.execution_kind
            || participant.maximum_dense_updates != descriptor.maximum_dense_updates
            || participant.dense_update_period_ticks != descriptor.dense_update_period_ticks
            || participant.terminal_block_digest != job.validation.final_digest
        {
            return Err(StatusCode::Integrity);
        }
        Ok(())
    }
}

struct SimulatedJob {
    descriptor: JobDescriptor,
    validation: ValidatedPartition,
    schedule: PreparedJobSchedule,
    cancelled: bool,
}

impl SimulatedJob {
    fn storage_mutation_safe(&self) -> bool {
        self.cancelled
            || matches!(
                self.schedule.report().state,
                JobScheduleState::Aborted
                    | JobScheduleState::Expired
                    | JobScheduleState::Complete
                    | JobScheduleState::Faulted
            )
    }

    fn advance(&mut self, now: DeviceCycle) {
        if self.cancelled {
            return;
        }
        match self.schedule.advance(now) {
            JobScheduleAction::PrimeHardware { .. } => {
                let _ = self.schedule.mark_primed(now);
            }
            JobScheduleAction::Start {
                scheduled_cycle, ..
            } => {
                let observation = JobStartObservation {
                    source: JobStartObservationSource::SimulatedLatch,
                    output_token: SIMULATED_OUTPUT_TOKEN,
                    scheduled_cycle,
                    earliest_cycle: scheduled_cycle,
                    latest_cycle: scheduled_cycle,
                };
                if self.schedule.record_start_observation(observation).is_ok() {
                    let _ = self.schedule.complete(now);
                }
            }
            JobScheduleAction::None
            | JobScheduleAction::AbortUnconfirmed
            | JobScheduleAction::MissedStart
            | JobScheduleAction::LeaseExpired => {}
        }
    }

    fn report(&self) -> JobStatusReport {
        if self.cancelled {
            return JobStatusReport {
                service: Some(ServiceJobReport {
                    prepare_id: self.descriptor.prepare_id,
                    state: ServiceJobState::Cancelled,
                    axis_count: self.descriptor.axis_count,
                    validated_blocks: self.descriptor.block_count,
                    sent_blocks: self.descriptor.block_count,
                    total_blocks: self.descriptor.block_count,
                    storage_chunks_read: self.validation.storage_chunks_read,
                    queue_free: 0,
                    queue_depth: 0,
                    final_progress: None,
                }),
                realtime: Some(RealtimeJobReport {
                    prepare_id: self.descriptor.prepare_id,
                    state: RealtimeJobState::Cancelled,
                    admitted_blocks: 1,
                    completed_blocks: 0,
                    total_blocks: self.descriptor.block_count,
                    queue_depth: 0,
                    admitted_progress: None,
                    completed_progress: None,
                    outstanding: false,
                }),
                schedule: None,
            };
        }
        let schedule = self.schedule.report();
        let complete = schedule.state == JobScheduleState::Complete;
        JobStatusReport {
            service: Some(ServiceJobReport {
                prepare_id: self.descriptor.prepare_id,
                state: ServiceJobState::Complete,
                axis_count: self.descriptor.axis_count,
                validated_blocks: self.descriptor.block_count,
                sent_blocks: self.descriptor.block_count,
                total_blocks: self.descriptor.block_count,
                storage_chunks_read: self.validation.storage_chunks_read,
                queue_free: 0,
                queue_depth: 0,
                final_progress: Some((self.validation.final_tick, self.validation.final_digest)),
            }),
            realtime: Some(if complete {
                RealtimeJobReport {
                    prepare_id: self.descriptor.prepare_id,
                    state: RealtimeJobState::Complete,
                    admitted_blocks: self.descriptor.block_count,
                    completed_blocks: self.descriptor.block_count,
                    total_blocks: self.descriptor.block_count,
                    queue_depth: 0,
                    admitted_progress: None,
                    completed_progress: Some((
                        self.validation.final_tick,
                        self.validation.final_digest,
                    )),
                    outstanding: false,
                }
            } else {
                RealtimeJobReport {
                    prepare_id: self.descriptor.prepare_id,
                    state: RealtimeJobState::Admitted,
                    admitted_blocks: 1,
                    completed_blocks: 0,
                    total_blocks: self.descriptor.block_count,
                    queue_depth: 0,
                    admitted_progress: Some((
                        self.validation.first_tick,
                        self.validation.first_digest,
                    )),
                    completed_progress: None,
                    outstanding: true,
                }
            }),
            schedule: Some(schedule),
        }
    }
}

#[derive(Clone, Copy)]
struct ValidatedPartition {
    first_tick: StreamTick,
    first_digest: Digest,
    final_tick: StreamTick,
    final_digest: Digest,
    storage_chunks_read: u32,
}

fn validate_partition(
    descriptor: JobDescriptor,
    bytes: &[u8],
) -> Result<ValidatedPartition, StatusCode> {
    if descriptor.execution_kind != ExecutionKind::Motion {
        return Err(StatusCode::Unsupported);
    }
    let expected_bytes = usize::try_from(descriptor.block_count)
        .ok()
        .and_then(|blocks| blocks.checked_mul(EXECUTION_BLOCK_BYTES))
        .ok_or(StatusCode::Capacity)?;
    if bytes.len() != expected_bytes {
        return Err(StatusCode::Integrity);
    }
    let mut expectation = descriptor.first_expectation();
    let mut first = None;
    let mut final_progress = None;
    for raw in bytes.chunks_exact(EXECUTION_BLOCK_BYTES) {
        let block = ExecutionBlock::decode(raw.try_into().map_err(|_| StatusCode::Integrity)?)
            .map_err(|_| StatusCode::Integrity)?;
        let summary = block
            .validate_motion::<JOB_AXES>(expectation, descriptor.limits)
            .map_err(|_| StatusCode::Integrity)?;
        let progress = (summary.end_tick, summary.block_digest);
        first.get_or_insert(progress);
        final_progress = Some(progress);
        expectation = BlockExpectation {
            stream_id: descriptor.stream_id,
            capability_digest: descriptor.capability_digest,
            config_digest: descriptor.config_digest,
            sequence: summary
                .sequence
                .checked_add(1)
                .ok_or(StatusCode::Capacity)?,
            start_tick: summary.end_tick,
            previous_digest: summary.block_digest,
        };
    }
    let (first_tick, first_digest) = first.ok_or(StatusCode::Integrity)?;
    let (final_tick, final_digest) = final_progress.ok_or(StatusCode::Integrity)?;
    let storage_chunks_read =
        u32::try_from(bytes.len().div_ceil(1_024)).map_err(|_| StatusCode::Capacity)?;
    Ok(ValidatedPartition {
        first_tick,
        first_digest,
        final_tick,
        final_digest,
        storage_chunks_read,
    })
}

fn decode_chunk(native: NativeRequest<'_>) -> Result<(ChunkUploadHeader, &[u8]), StatusCode> {
    let prefix = native
        .body
        .get(..ChunkUploadHeader::WIRE_LEN)
        .ok_or(StatusCode::InvalidRequest)?;
    let header = ChunkUploadHeader::decode(prefix).map_err(|_| StatusCode::InvalidRequest)?;
    header
        .validate_body_len(native.message.body_len)
        .map_err(|_| StatusCode::InvalidRequest)?;
    let bytes = &native.body[ChunkUploadHeader::WIRE_LEN..];
    if sha256(bytes) != header.content {
        return Err(StatusCode::Integrity);
    }
    Ok((header, bytes))
}

fn native_response(
    native: NativeRequest<'_>,
    now: DeviceCycle,
    status: StatusCode,
    body: &[u8],
) -> ServiceResponse {
    ServiceResponse::native(native, now, status, body)
        .unwrap_or_else(|_| ServiceResponse::invalid_native())
}

const fn job_schedule_error_status(error: JobScheduleError) -> StatusCode {
    match error {
        JobScheduleError::Wire(_) => StatusCode::InvalidRequest,
        JobScheduleError::Identity => StatusCode::Integrity,
        JobScheduleError::Conflict | JobScheduleError::State => StatusCode::Conflict,
        JobScheduleError::Readiness | JobScheduleError::Policy => StatusCode::ForbiddenState,
        JobScheduleError::Deadline => StatusCode::Deadline,
    }
}

const fn cache_error_status(error: CacheError) -> StatusCode {
    match error {
        CacheError::Storage(
            StorageError::MissingDigest { .. }
            | StorageError::ChunkDigest { .. }
            | StorageError::ObjectDigest
            | StorageError::ManifestDigest,
        )
        | CacheError::Integrity
        | CacheError::CorruptJournal => StatusCode::Integrity,
        CacheError::Storage(
            StorageError::UploadConflict
            | StorageError::UploadIdMismatch { .. }
            | StorageError::UnexpectedChunk { .. }
            | StorageError::VerifiedChunkMismatch
            | StorageError::PublishTokenMismatch
            | StorageError::ConfigurationTransition
            | StorageError::GraphTransition,
        )
        | CacheError::PublicationConflict => StatusCode::Conflict,
        CacheError::Storage(
            StorageError::ObjectTooLarge { .. } | StorageError::ChunkCount { .. },
        ) => StatusCode::Capacity,
        CacheError::Storage(StorageError::MutationForbidden) => StatusCode::ForbiddenState,
        CacheError::Storage(StorageError::PublishPending | StorageError::NotReadyToPublish) => {
            StatusCode::Busy
        }
        CacheError::Storage(StorageError::NoUpload) => StatusCode::NotFound,
        CacheError::Storage(_) => StatusCode::InvalidRequest,
        CacheError::PoweredOff | CacheError::PowerLoss(_) => StatusCode::Internal,
    }
}

#[cfg(test)]
mod tests {
    use alumina_job::{
        JOB_COMMIT_ID_BYTES, JobCommitId, JobNetworkPolicy, JobScheduleReferenceAction,
        MachineJobGlobalFacts, MachineJobManifest, MachineJobParticipant,
    };
    use alumina_machine_ir::{
        BlockValidationLimits, ExecutionSegment, MAX_EXECUTION_AXES, StreamId, ValidationLimits,
    };
    use alumina_protocol::{FrameHeader, MessageHeader};
    use alumina_service::ResponseMedia;
    use alumina_storage::{
        CacheLimits, ChunkUploadHeader, DigestAlgorithm, ManifestHasher, StoredObject, UploadId,
    };

    use super::*;

    #[test]
    fn active_configuration_status_is_canonical_and_authorized() {
        let active = SimulatedActiveConfiguration::new().unwrap();
        let status = ConfigurationCoordinatorStatus::decode(active.status()).unwrap();
        assert_eq!(status.phase, ConfigurationCoordinatorPhase::Active);
        assert_eq!(status.active_digest, active.digest());
        assert_ne!(status.active_bytes, 0);
        assert!(
            status
                .flags
                .contains(ConfigurationCoordinatorFlags::JOBS_AUTHORIZED)
        );
    }

    #[test]
    fn immutable_cache_prepare_and_future_start_complete_through_native_frames() {
        let device_id = DeviceId(*b"SIM-JOB-MCU-0001");
        let boot_id = BootId::new([0x31; 16]).unwrap();
        let active = SimulatedActiveConfiguration::new().unwrap();
        let mut service = SimulatedCachedJobService::new(device_id, boot_id, active.digest());
        let stream_id = StreamId::new(*b"sim-job-stream01").unwrap();
        let block = ExecutionBlock::encode_motion(
            stream_id,
            capability::CAPABILITY_DIGEST,
            active.digest(),
            0,
            Digest::ZERO,
            &[ExecutionSegment {
                start_tick: StreamTick(0),
                end_tick: StreamTick(100),
                delta_steps: [10, -5],
                flags: 0,
            }],
        )
        .unwrap();
        let partition_bytes = block.as_bytes().to_vec();
        let partition_plan = upload_plan(
            UploadId(11),
            ObjectKind::MachineJobPartition,
            &partition_bytes,
        );
        upload(&mut service, partition_plan, &partition_bytes, 10);

        let descriptor = JobDescriptor {
            prepare_id: 41,
            partition: PublishedObject {
                object: partition_plan.object,
                manifest: partition_plan.manifest,
            },
            stream_id,
            capability_digest: capability::CAPABILITY_DIGEST,
            config_digest: active.digest(),
            axis_count: 2,
            execution_kind: ExecutionKind::Motion,
            maximum_dense_updates: 0,
            dense_update_period_ticks: 0,
            block_count: 1,
            first_tick: StreamTick(0),
            initial_position: [0; MAX_EXECUTION_AXES],
            limits: BlockValidationLimits {
                maximum_block_ticks: 1_000,
                segment: ValidationLimits {
                    maximum_segment_ticks: 1_000,
                    maximum_steps_per_segment: 1_000,
                },
            },
        };
        let mut final_position = [0_i64; MAX_EXECUTION_AXES];
        final_position[0] = 10;
        final_position[1] = -5;
        let participant = MachineJobParticipant {
            device_id,
            stream_id,
            board_package_digest: Digest([0x40; 32]),
            capability_digest: capability::CAPABILITY_DIGEST,
            config_digest: active.digest(),
            partition_digest: partition_plan.object.content.digest,
            partition_manifest_digest: partition_plan.manifest.digest,
            terminal_block_digest: block.header().block_digest,
            resource_set_digest: Digest([0x41; 32]),
            error_evidence_digest: Digest([0x42; 32]),
            safety_envelope_digest: Digest([0x43; 32]),
            partition_byte_len: partition_plan.object.byte_len,
            block_count: 1,
            axis_count: 2,
            execution_kind: ExecutionKind::Motion,
            dense_update_period_ticks: 0,
            maximum_dense_updates: 0,
            local_timer_hz: 1_000_000,
            first_tick: StreamTick(0),
            end_tick: StreamTick(100),
            initial_position: [0; MAX_EXECUTION_AXES],
            final_position,
        };
        let global = MachineJobGlobalFacts {
            network_policy: JobNetworkPolicy::CachedAutonomous,
            global_timebase_hz: 1_000_000,
            duration_ticks: 100,
            source_digest: Digest([0x51; 32]),
            compiler_digest: Digest([0x52; 32]),
            interface_digest: Digest([0x53; 32]),
            policy_digest: Digest([0x54; 32]),
            machine_digest: Digest([0x55; 32]),
            coordinate_epoch_digest: Digest([0x56; 32]),
            safety_policy_digest: Digest([0x57; 32]),
            synchronization_digest: Digest([0x58; 32]),
        };
        let participants = [participant];
        let manifest = MachineJobManifest::new(global, &participants).unwrap();
        let mut manifest_bytes = vec![0_u8; manifest.wire_len().unwrap()];
        manifest.encode_into(&mut manifest_bytes).unwrap();
        let manifest_plan = upload_plan(
            UploadId(12),
            ObjectKind::MachineJobManifest,
            &manifest_bytes,
        );
        upload(&mut service, manifest_plan, &manifest_bytes, 20);

        let (status, body) = dispatch(
            &mut service,
            FrameKind::Job,
            Operation::JobPrepare,
            active.digest(),
            &descriptor.encode::<2>().unwrap(),
            DeviceCycle(1_000_000),
            30,
        );
        assert_eq!(status, StatusCode::Ok);
        let prepared = JobStatusReport::decode(&body).unwrap();
        assert_eq!(prepared.schedule.unwrap().state, JobScheduleState::Prepared);
        let prepared_token = prepared.schedule.unwrap().prepared_token.unwrap();
        let commit = JobCommitRequest {
            policy: JobNetworkPolicy::CachedAutonomous,
            prepare_id: descriptor.prepare_id,
            boot_id,
            global_job_digest: manifest_plan.object.content.digest,
            participant_set_digest: manifest.participant_set_digest(),
            prepared_token,
            partition_digest: descriptor.partition.object.content.digest,
            local_start_cycle: DeviceCycle(5_000_000),
            confirm_deadline_cycle: DeviceCycle(2_000_000),
            abort_guard_cycle: DeviceCycle(4_000_000),
            lease_expiry_cycle: DeviceCycle(6_000_000),
            clock_probe_id: 9,
            clock_uncertainty_cycles: 100,
            required_sync_tolerance_cycles: 1_000,
            commit_id: JobCommitId::new([0x61; JOB_COMMIT_ID_BYTES]).unwrap(),
        };
        let (status, body) = dispatch(
            &mut service,
            FrameKind::Job,
            Operation::JobCommit,
            active.digest(),
            &commit.encode().unwrap(),
            DeviceCycle(1_100_000),
            31,
        );
        assert_eq!(status, StatusCode::Ok);
        assert_eq!(
            JobStatusReport::decode(&body)
                .unwrap()
                .schedule
                .unwrap()
                .state,
            JobScheduleState::Installed
        );
        let confirm =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit).unwrap();
        let (status, body) = dispatch(
            &mut service,
            FrameKind::Job,
            Operation::JobConfirm,
            active.digest(),
            &confirm.encode().unwrap(),
            DeviceCycle(1_200_000),
            32,
        );
        assert_eq!(status, StatusCode::Ok);
        assert_eq!(
            JobStatusReport::decode(&body)
                .unwrap()
                .schedule
                .unwrap()
                .state,
            JobScheduleState::Confirmed
        );

        let (_, body) = dispatch(
            &mut service,
            FrameKind::Job,
            Operation::JobStatus,
            active.digest(),
            &[],
            DeviceCycle(4_000_000),
            33,
        );
        assert_eq!(
            JobStatusReport::decode(&body)
                .unwrap()
                .schedule
                .unwrap()
                .state,
            JobScheduleState::Primed
        );
        let (status, body) = dispatch(
            &mut service,
            FrameKind::Job,
            Operation::JobStatus,
            active.digest(),
            &[],
            DeviceCycle(5_000_000),
            34,
        );
        assert_eq!(status, StatusCode::Ok);
        let complete = JobStatusReport::decode(&body).unwrap();
        assert_eq!(complete.schedule.unwrap().state, JobScheduleState::Complete);
        assert_eq!(
            complete.schedule.unwrap().start_observation.unwrap().source,
            JobStartObservationSource::SimulatedLatch
        );
        assert_eq!(complete.realtime.unwrap().state, RealtimeJobState::Complete);
    }

    fn upload_plan(upload_id: UploadId, kind: ObjectKind, bytes: &[u8]) -> UploadPlan {
        const CHUNK_BYTES: u32 = 1_024;
        let object = StoredObject {
            kind,
            content: sha256(bytes),
            byte_len: u64::try_from(bytes.len()).unwrap(),
        };
        let chunk_count = u32::try_from(bytes.len().div_ceil(CHUNK_BYTES as usize)).unwrap();
        let mut manifest = ManifestHasher::new(
            object,
            CHUNK_BYTES,
            chunk_count,
            CacheLimits {
                maximum_object_bytes: 4 * 1_024 * 1_024,
                maximum_chunk_bytes: CHUNK_BYTES,
                maximum_chunks: 10_000,
            },
        )
        .unwrap();
        for (index, chunk) in bytes.chunks(CHUNK_BYTES as usize).enumerate() {
            manifest
                .push(
                    u32::try_from(index).unwrap(),
                    sha256(chunk),
                    u32::try_from(chunk.len()).unwrap(),
                )
                .unwrap();
        }
        UploadPlan {
            upload_id,
            object,
            manifest: manifest.finalize().unwrap(),
            chunk_bytes: CHUNK_BYTES,
            chunk_count,
        }
    }

    fn upload(
        service: &mut SimulatedCachedJobService,
        plan: UploadPlan,
        bytes: &[u8],
        correlation: u32,
    ) {
        let publication = PublishedObject {
            object: plan.object,
            manifest: plan.manifest,
        };
        assert_eq!(
            dispatch(
                service,
                FrameKind::Storage,
                Operation::StorageInspect,
                Digest::ZERO,
                &publication.encode(),
                DeviceCycle(1),
                correlation,
            )
            .0,
            StatusCode::NotFound
        );
        assert_eq!(
            dispatch(
                service,
                FrameKind::Storage,
                Operation::StorageBeginUpload,
                Digest::ZERO,
                &plan.encode(),
                DeviceCycle(2),
                correlation + 1,
            )
            .0,
            StatusCode::Ok
        );
        for (index, chunk) in bytes.chunks(plan.chunk_bytes as usize).enumerate() {
            let index = u32::try_from(index).unwrap();
            let header = ChunkUploadHeader {
                upload_id: plan.upload_id,
                index,
                byte_len: u32::try_from(chunk.len()).unwrap(),
                content: ContentId {
                    algorithm: DigestAlgorithm::Sha256,
                    digest: sha256(chunk).digest,
                },
            };
            let mut body = header.encode().to_vec();
            body.extend_from_slice(chunk);
            assert_eq!(
                dispatch(
                    service,
                    FrameKind::Storage,
                    Operation::StoragePutChunk,
                    Digest::ZERO,
                    &body,
                    DeviceCycle(3 + u64::from(index)),
                    correlation + 2 + index,
                )
                .0,
                StatusCode::Ok
            );
        }
        assert_eq!(
            dispatch(
                service,
                FrameKind::Storage,
                Operation::StorageFinalize,
                Digest::ZERO,
                &FinalizeUploadRequest {
                    upload_id: plan.upload_id,
                }
                .encode(),
                DeviceCycle(9),
                correlation + 9,
            )
            .0,
            StatusCode::Ok
        );
    }

    fn dispatch(
        service: &mut SimulatedCachedJobService,
        kind: FrameKind,
        operation: Operation,
        config_digest: Digest,
        body: &[u8],
        now: DeviceCycle,
        correlation: u32,
    ) -> (StatusCode, Vec<u8>) {
        let message =
            MessageHeader::request(operation, correlation, u32::try_from(body.len()).unwrap());
        let payload_len = MessageHeader::WIRE_LEN + body.len();
        let frame = FrameHeader::new(
            kind,
            u32::try_from(payload_len).unwrap(),
            correlation,
            DeviceCycle(0),
            config_digest,
        );
        let mut request = frame.encode().to_vec();
        request.extend_from_slice(&message.encode());
        request.extend_from_slice(body);
        let native = NativeRequest::decode(&request).unwrap();
        let response = service
            .dispatch(native, now, 1_000_000, 100_000, 60_000_000)
            .unwrap();
        assert_eq!(response.media, ResponseMedia::NativeFrame);
        let bytes = response.bytes();
        let response_frame = FrameHeader::decode(
            &bytes[..FrameHeader::WIRE_LEN],
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        let message_start = FrameHeader::WIRE_LEN;
        let message_end = message_start + MessageHeader::WIRE_LEN;
        let response_message = MessageHeader::decode_and_validate(
            &bytes[message_start..message_end],
            response_frame.kind,
            response_frame.payload_len,
        )
        .unwrap();
        (response_message.status, bytes[message_end..].to_vec())
    }
}
