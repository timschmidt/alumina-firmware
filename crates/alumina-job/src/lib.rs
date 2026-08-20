#![no_std]
#![doc = "Bounded cached-job preparation and dual-core admission for Alumina."]

mod manifest;
mod schedule;

pub use manifest::*;
pub use schedule::*;

use alumina_clock::{BOOT_ID_BYTES, BootId};
use alumina_machine_ir::{
    AssembleOutcome, BlockError, BlockExpectation, BlockValidationLimits, EXECUTION_BLOCK_BYTES,
    ExecutionBlock, ExecutionBlockHeader, ExecutionKind, FINITE_DIFFERENCE_ONE_STEP,
    FiniteDifferenceBlockValidationLimits, FiniteDifferenceSegments,
    FiniteDifferenceStreamProgress, FiniteDifferenceStreamValidator,
    FiniteDifferenceValidationLimits, MAX_EXECUTION_AXES, MAX_SERVO_EXECUTION_AXES, MotionSegments,
    MotionStreamProgress, MotionStreamValidator, PartitionAssembler,
    ServoFiniteDifferenceBlockValidationLimits, ServoFiniteDifferenceSegments,
    ServoFiniteDifferenceStreamProgress, ServoFiniteDifferenceStreamValidator, StreamId,
    StreamTick,
};
use alumina_protocol::Digest;
use alumina_runtime::{RealtimeEndpoint, ServiceEndpoint};
use alumina_storage::media::{AsyncBlockDevice, MAX_MEDIA_CHUNK_BYTES, PublishedReader};
use alumina_storage::provisioning::{ProvisionedCache, ProvisionedCacheError};
use alumina_storage::{
    ContentId, DigestAlgorithm, ObjectKind, PublishedObject, StoredObject, sha256,
};
use embassy_sync::channel::TrySendError;

/// Exact canonical `JobPrepare` body length.
pub const JOB_DESCRIPTOR_WIRE_BYTES: usize = 320;
/// Exact fixed cross-core job-control payload length.
pub const CORE_JOB_COMMAND_WIRE_BYTES: usize = 344;
/// Exact fixed core-1 job report length.
pub const REALTIME_JOB_REPORT_WIRE_BYTES: usize = 128;
/// Exact fixed core-0 prefetch report length.
pub const SERVICE_JOB_REPORT_WIRE_BYTES: usize = 96;
/// Exact combined `JobStatus` response body length.
pub const JOB_STATUS_WIRE_BYTES: usize = 368;
/// Exact `JobCancel` operation body length.
pub const JOB_CANCEL_WIRE_BYTES: usize = 8;
/// Maximum number of independently validated blocks that core 1 may lend to
/// the physical execution pipeline before the oldest token is acknowledged.
///
/// Two is the minimum useful window for continuous hardware output: one block
/// may await its exact physical-commit barrier while the successor is planned.
pub const REALTIME_BLOCK_WINDOW: usize = 2;

const JOB_DESCRIPTOR_MAGIC: [u8; 8] = *b"ALMJOBD4";
const JOB_DESCRIPTOR_VERSION: u16 = 4;
const JOB_DESCRIPTOR_HASH_OFFSET: usize = JOB_DESCRIPTOR_WIRE_BYTES - 32;
const CORE_JOB_COMMAND_MAGIC: [u8; 4] = *b"ALJC";
const CORE_JOB_COMMAND_VERSION: u16 = 3;
const CORE_JOB_PREPARE: u8 = 1;
const CORE_JOB_CANCEL: u8 = 2;
const CORE_JOB_COMMIT: u8 = 3;
const CORE_JOB_CONFIRM: u8 = 4;
const CORE_JOB_ABORT: u8 = 5;
const CORE_JOB_RENEW_LEASE: u8 = 6;
const REALTIME_JOB_REPORT_MAGIC: [u8; 8] = *b"ALMJRT01";
const REALTIME_JOB_REPORT_VERSION: u16 = 1;
const SERVICE_JOB_REPORT_MAGIC: [u8; 8] = *b"ALMJSV01";
const SERVICE_JOB_REPORT_VERSION: u16 = 1;
const SERVICE_REPORT_FLAG_FINAL: u8 = 1 << 0;
const JOB_STATUS_MAGIC: [u8; 8] = *b"ALMJST03";
const JOB_STATUS_VERSION: u16 = 3;
const JOB_STATUS_FLAG_SERVICE: u8 = 1 << 0;
const JOB_STATUS_FLAG_REALTIME: u8 = 1 << 1;
const JOB_STATUS_FLAG_SCHEDULE: u8 = 1 << 2;
const JOB_STATUS_KNOWN_FLAGS: u8 =
    JOB_STATUS_FLAG_SERVICE | JOB_STATUS_FLAG_REALTIME | JOB_STATUS_FLAG_SCHEDULE;
const REPORT_FLAG_OUTSTANDING: u8 = 1 << 0;
const REPORT_FLAG_ADMITTED_PROGRESS: u8 = 1 << 1;
const REPORT_FLAG_COMPLETED_PROGRESS: u8 = 1 << 2;
const REPORT_KNOWN_FLAGS: u8 =
    REPORT_FLAG_OUTSTANDING | REPORT_FLAG_ADMITTED_PROGRESS | REPORT_FLAG_COMPLETED_PROGRESS;

/// Complete immutable facts required before core 0 may read executable bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobDescriptor {
    /// Boot-local nonzero lifecycle correlation; not part of cached bytes.
    pub prepare_id: u64,
    /// Exact typed storage publication selected by the controller.
    pub partition: PublishedObject,
    /// Identity repeated by every execution block.
    pub stream_id: StreamId,
    /// Board capabilities used by the authoritative compiler.
    pub capability_digest: alumina_protocol::Digest,
    /// Active machine configuration used by the authoritative compiler.
    pub config_digest: alumina_protocol::Digest,
    /// Exact axis width selected for this per-MCU stream.
    pub axis_count: u8,
    /// Exact execution-record family required in every partition block.
    pub execution_kind: ExecutionKind,
    /// Maximum dense updates in one recurrence record. Ordinary coordinated
    /// motion streams require zero.
    pub maximum_dense_updates: u32,
    /// Exact device ticks between dense updates. Ordinary coordinated motion
    /// streams require zero.
    pub dense_update_period_ticks: u32,
    /// Exact count implied by partition bytes; repeated for conflict detection.
    pub block_count: u32,
    /// First relative stream tick; current full partitions begin at zero.
    pub first_tick: StreamTick,
    /// Exact absolute machine-lattice position at `first_tick`. Slots at and
    /// above `axis_count` are canonical zero.
    pub initial_position: [i64; MAX_EXECUTION_AXES],
    /// Board/config-derived block and segment admission limits.
    pub limits: BlockValidationLimits,
}

impl JobDescriptor {
    /// Validates typed identity, size/count agreement, and all nonzero bounds.
    pub fn validate<const AXES: usize>(self) -> Result<(), DescriptorError> {
        if self.prepare_id == 0 {
            return Err(DescriptorError::PrepareId);
        }
        if self.partition.object.kind != ObjectKind::MachineJobPartition {
            return Err(DescriptorError::ObjectKind);
        }
        if !self.partition.object.content.is_valid() || !self.partition.manifest.is_valid() {
            return Err(DescriptorError::ContentIdentity);
        }
        if AXES == 0 || AXES > MAX_EXECUTION_AXES || usize::from(self.axis_count) != AXES {
            return Err(DescriptorError::AxisCount {
                encoded: self.axis_count,
                expected: AXES,
            });
        }
        match self.execution_kind {
            ExecutionKind::Motion
                if self.maximum_dense_updates != 0 || self.dense_update_period_ticks != 0 =>
            {
                return Err(DescriptorError::Limits);
            }
            ExecutionKind::FiniteDifference | ExecutionKind::ServoFiniteDifference
                if self.maximum_dense_updates == 0 || self.dense_update_period_ticks == 0 =>
            {
                return Err(DescriptorError::Limits);
            }
            ExecutionKind::ServoFiniteDifference if AXES > MAX_SERVO_EXECUTION_AXES => {
                return Err(DescriptorError::AxisCount {
                    encoded: self.axis_count,
                    expected: MAX_SERVO_EXECUTION_AXES,
                });
            }
            ExecutionKind::Motion
            | ExecutionKind::FiniteDifference
            | ExecutionKind::ServoFiniteDifference => {}
        }
        StreamId::new(self.stream_id.0).map_err(|_| DescriptorError::StreamIdentity)?;
        if self.capability_digest.is_zero() {
            return Err(DescriptorError::CapabilityIdentity);
        }
        if self.config_digest.is_zero() {
            return Err(DescriptorError::ConfigurationIdentity);
        }
        if self.first_tick != StreamTick(0) {
            return Err(DescriptorError::FirstTick);
        }
        if self.initial_position[AXES..]
            .iter()
            .any(|position| *position != 0)
        {
            return Err(DescriptorError::InitialPosition);
        }
        if self.limits.maximum_block_ticks == 0
            || self.limits.segment.maximum_segment_ticks == 0
            || self.limits.segment.maximum_steps_per_segment == 0
        {
            return Err(DescriptorError::Limits);
        }
        let block_bytes =
            u64::try_from(EXECUTION_BLOCK_BYTES).map_err(|_| DescriptorError::PartitionLayout)?;
        let bytes = self.partition.object.byte_len;
        if bytes == 0 || !bytes.is_multiple_of(block_bytes) {
            return Err(DescriptorError::PartitionLayout);
        }
        let derived =
            u32::try_from(bytes / block_bytes).map_err(|_| DescriptorError::PartitionLayout)?;
        if self.block_count == 0 || self.block_count != derived {
            return Err(DescriptorError::BlockCount {
                encoded: self.block_count,
                derived,
            });
        }
        Ok(())
    }

    /// First independently required block facts.
    pub const fn first_expectation(self) -> BlockExpectation {
        BlockExpectation {
            stream_id: self.stream_id,
            capability_digest: self.capability_digest,
            config_digest: self.config_digest,
            sequence: 0,
            start_tick: self.first_tick,
            previous_digest: alumina_protocol::Digest::ZERO,
        }
    }

    /// Dense exact starting lattice position for the selected executor width.
    pub fn initial_position_for<const AXES: usize>(self) -> Result<[i64; AXES], DescriptorError> {
        self.validate::<AXES>()?;
        self.initial_position[..AXES]
            .try_into()
            .map_err(|_| DescriptorError::AxisCount {
                encoded: self.axis_count,
                expected: AXES,
            })
    }

    /// Encodes one canonical, self-hashed prepare body.
    pub fn encode<const AXES: usize>(
        self,
    ) -> Result<[u8; JOB_DESCRIPTOR_WIRE_BYTES], JobDescriptorWireError> {
        self.validate::<AXES>()
            .map_err(JobDescriptorWireError::Descriptor)?;
        let mut encoded = [0_u8; JOB_DESCRIPTOR_WIRE_BYTES];
        encoded[0..8].copy_from_slice(&JOB_DESCRIPTOR_MAGIC);
        encoded[8..10].copy_from_slice(&JOB_DESCRIPTOR_VERSION.to_le_bytes());
        encoded[10] = self.execution_kind as u8;
        // Byte 11 remains reserved.
        encoded[12..16].copy_from_slice(&self.maximum_dense_updates.to_le_bytes());
        encoded[16..20].copy_from_slice(&self.dense_update_period_ticks.to_le_bytes());
        // Bytes 20..24 remain reserved.
        encoded[24..32].copy_from_slice(&self.prepare_id.to_le_bytes());
        encoded[32] = self.partition.object.kind as u8;
        encoded[33] = self.partition.object.content.algorithm as u8;
        encoded[34] = self.partition.manifest.algorithm as u8;
        encoded[35] = self.axis_count;
        encoded[36..40].copy_from_slice(&self.block_count.to_le_bytes());
        encoded[40..48].copy_from_slice(&self.partition.object.byte_len.to_le_bytes());
        encoded[48..56].copy_from_slice(&self.first_tick.0.to_le_bytes());
        encoded[56..64].copy_from_slice(&self.limits.maximum_block_ticks.to_le_bytes());
        encoded[64..72].copy_from_slice(&self.limits.segment.maximum_segment_ticks.to_le_bytes());
        encoded[72..80]
            .copy_from_slice(&self.limits.segment.maximum_steps_per_segment.to_le_bytes());
        encoded[80..96].copy_from_slice(&self.stream_id.0);
        encoded[96..128].copy_from_slice(&self.partition.object.content.digest.0);
        encoded[128..160].copy_from_slice(&self.partition.manifest.digest.0);
        encoded[160..192].copy_from_slice(&self.capability_digest.0);
        encoded[192..224].copy_from_slice(&self.config_digest.0);
        for (axis, position) in self.initial_position.into_iter().enumerate() {
            let offset = 224 + axis * 8;
            encoded[offset..offset + 8].copy_from_slice(&position.to_le_bytes());
        }
        let identity = sha256(&encoded[..JOB_DESCRIPTOR_HASH_OFFSET]);
        encoded[JOB_DESCRIPTOR_HASH_OFFSET..].copy_from_slice(&identity.digest.0);
        Ok(encoded)
    }

    /// Decodes only the exact canonical prepare representation for this executor.
    pub fn decode<const AXES: usize>(encoded: &[u8]) -> Result<Self, JobDescriptorWireError> {
        if encoded.len() != JOB_DESCRIPTOR_WIRE_BYTES {
            return Err(JobDescriptorWireError::WireLength);
        }
        if encoded[0..8] != JOB_DESCRIPTOR_MAGIC {
            return Err(JobDescriptorWireError::Magic);
        }
        if read_u16(encoded, 8) != JOB_DESCRIPTOR_VERSION {
            return Err(JobDescriptorWireError::Version);
        }
        if encoded[11] != 0 || encoded[20..24].iter().any(|byte| *byte != 0) {
            return Err(JobDescriptorWireError::Reserved);
        }
        let execution_kind =
            ExecutionKind::from_wire(encoded[10]).ok_or(JobDescriptorWireError::ExecutionKind)?;
        if encoded[32] != ObjectKind::MachineJobPartition as u8 {
            return Err(JobDescriptorWireError::ObjectKind);
        }
        if encoded[33] != DigestAlgorithm::Sha256 as u8
            || encoded[34] != DigestAlgorithm::Sha256 as u8
        {
            return Err(JobDescriptorWireError::Algorithm);
        }
        let identity = sha256(&encoded[..JOB_DESCRIPTOR_HASH_OFFSET]);
        if encoded[JOB_DESCRIPTOR_HASH_OFFSET..] != identity.digest.0 {
            return Err(JobDescriptorWireError::Integrity);
        }
        let mut stream_id = [0_u8; 16];
        stream_id.copy_from_slice(&encoded[80..96]);
        let mut object_digest = [0_u8; 32];
        object_digest.copy_from_slice(&encoded[96..128]);
        let mut manifest_digest = [0_u8; 32];
        manifest_digest.copy_from_slice(&encoded[128..160]);
        let mut capability_digest = [0_u8; 32];
        capability_digest.copy_from_slice(&encoded[160..192]);
        let mut config_digest = [0_u8; 32];
        config_digest.copy_from_slice(&encoded[192..224]);
        let mut initial_position = [0_i64; MAX_EXECUTION_AXES];
        for (axis, position) in initial_position.iter_mut().enumerate() {
            *position = read_i64(encoded, 224 + axis * 8);
        }
        let descriptor = Self {
            prepare_id: read_u64(encoded, 24),
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId::from_sha256(Digest(object_digest)),
                    byte_len: read_u64(encoded, 40),
                },
                manifest: ContentId::from_sha256(Digest(manifest_digest)),
            },
            stream_id: StreamId(stream_id),
            capability_digest: Digest(capability_digest),
            config_digest: Digest(config_digest),
            axis_count: encoded[35],
            execution_kind,
            maximum_dense_updates: read_u32(encoded, 12),
            dense_update_period_ticks: read_u32(encoded, 16),
            block_count: read_u32(encoded, 36),
            first_tick: StreamTick(read_u64(encoded, 48)),
            initial_position,
            limits: BlockValidationLimits {
                maximum_block_ticks: read_u64(encoded, 56),
                segment: alumina_machine_ir::ValidationLimits {
                    maximum_segment_ticks: read_u64(encoded, 64),
                    maximum_steps_per_segment: read_u64(encoded, 72),
                },
            },
        };
        descriptor
            .validate::<AXES>()
            .map_err(JobDescriptorWireError::Descriptor)?;
        if descriptor.encode::<AXES>()? != encoded {
            return Err(JobDescriptorWireError::Noncanonical);
        }
        Ok(descriptor)
    }
}

/// Canonical descriptor rejection before a job actor is installed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobDescriptorWireError {
    /// Body length was not exactly [`JOB_DESCRIPTOR_WIRE_BYTES`].
    WireLength,
    /// Descriptor magic did not identify this schema.
    Magic,
    /// Descriptor version was unsupported.
    Version,
    /// Flags or reserved bytes were nonzero.
    Reserved,
    /// Execution-record family was not assigned by machine IR V3.
    ExecutionKind,
    /// The object kind was not a per-MCU executable partition.
    ObjectKind,
    /// A content identity did not select SHA-256.
    Algorithm,
    /// The descriptor hash did not match its canonical prefix.
    Integrity,
    /// Decoded semantic facts were inadmissible.
    Descriptor(DescriptorError),
    /// Valid fields used a noncanonical representation.
    Noncanonical,
}

/// Fixed cross-core command; it contains no references or allocator state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "the fixed no-allocation command must own the complete prepare descriptor"
)]
pub enum CoreJobCommand {
    /// Install the exact independently validated prepare descriptor.
    Prepare {
        /// Public boot challenge generated by the authenticated service owner.
        boot_id: BootId,
        descriptor: JobDescriptor,
    },
    /// Invalidate one exact boot-local preparation and drain its work.
    Cancel { prepare_id: u64 },
    /// Install a complete participant-bound future local schedule.
    Commit(JobCommitRequest),
    /// Grant start authority after every participant acknowledged installation.
    Confirm(JobScheduleReference),
    /// Revoke an installed or confirmed schedule before its abort guard.
    Abort(JobScheduleReference),
    /// Extend one exact network-attended lease without changing its commit.
    RenewLease(JobLeaseRenewRequest),
}

impl CoreJobCommand {
    /// Encodes a fixed 344-byte payload fitting the reviewed command ring.
    pub fn encode<const AXES: usize>(
        self,
    ) -> Result<[u8; CORE_JOB_COMMAND_WIRE_BYTES], CoreJobCommandWireError> {
        let mut encoded = [0_u8; CORE_JOB_COMMAND_WIRE_BYTES];
        encoded[0..4].copy_from_slice(&CORE_JOB_COMMAND_MAGIC);
        encoded[4..6].copy_from_slice(&CORE_JOB_COMMAND_VERSION.to_le_bytes());
        match self {
            Self::Prepare {
                boot_id,
                descriptor,
            } => {
                encoded[6] = CORE_JOB_PREPARE;
                encoded[8..8 + BOOT_ID_BYTES].copy_from_slice(&boot_id.as_bytes());
                encoded[8 + BOOT_ID_BYTES..].copy_from_slice(
                    &descriptor
                        .encode::<AXES>()
                        .map_err(CoreJobCommandWireError::Descriptor)?,
                );
            }
            Self::Cancel { prepare_id } => {
                if prepare_id == 0 {
                    return Err(CoreJobCommandWireError::PrepareId);
                }
                encoded[6] = CORE_JOB_CANCEL;
                encoded[8..16].copy_from_slice(&prepare_id.to_le_bytes());
            }
            Self::Commit(commit) => {
                encoded[6] = CORE_JOB_COMMIT;
                encoded[8..8 + JOB_COMMIT_WIRE_BYTES]
                    .copy_from_slice(&commit.encode().map_err(CoreJobCommandWireError::Schedule)?);
            }
            Self::Confirm(reference) => {
                if reference.action != JobScheduleReferenceAction::Confirm {
                    return Err(CoreJobCommandWireError::Schedule(
                        JobScheduleWireError::Action,
                    ));
                }
                encoded[6] = CORE_JOB_CONFIRM;
                encoded[8..8 + JOB_SCHEDULE_REFERENCE_WIRE_BYTES].copy_from_slice(
                    &reference
                        .encode()
                        .map_err(CoreJobCommandWireError::Schedule)?,
                );
            }
            Self::Abort(reference) => {
                if reference.action != JobScheduleReferenceAction::Abort {
                    return Err(CoreJobCommandWireError::Schedule(
                        JobScheduleWireError::Action,
                    ));
                }
                encoded[6] = CORE_JOB_ABORT;
                encoded[8..8 + JOB_SCHEDULE_REFERENCE_WIRE_BYTES].copy_from_slice(
                    &reference
                        .encode()
                        .map_err(CoreJobCommandWireError::Schedule)?,
                );
            }
            Self::RenewLease(request) => {
                encoded[6] = CORE_JOB_RENEW_LEASE;
                encoded[8..8 + JOB_LEASE_RENEW_WIRE_BYTES].copy_from_slice(
                    &request
                        .encode()
                        .map_err(CoreJobCommandWireError::Schedule)?,
                );
            }
        }
        Ok(encoded)
    }

    /// Decodes a fixed payload and checks all unused bytes are zero.
    pub fn decode<const AXES: usize>(encoded: &[u8]) -> Result<Self, CoreJobCommandWireError> {
        if encoded.len() != CORE_JOB_COMMAND_WIRE_BYTES {
            return Err(CoreJobCommandWireError::WireLength);
        }
        if encoded[0..4] != CORE_JOB_COMMAND_MAGIC {
            return Err(CoreJobCommandWireError::Magic);
        }
        if read_u16(encoded, 4) != CORE_JOB_COMMAND_VERSION {
            return Err(CoreJobCommandWireError::Version);
        }
        if encoded[7] != 0 {
            return Err(CoreJobCommandWireError::Reserved);
        }
        match encoded[6] {
            CORE_JOB_PREPARE => {
                let mut boot_id = [0_u8; BOOT_ID_BYTES];
                boot_id.copy_from_slice(&encoded[8..8 + BOOT_ID_BYTES]);
                let boot_id = BootId::new(boot_id).map_err(|_| CoreJobCommandWireError::BootId)?;
                let descriptor = JobDescriptor::decode::<AXES>(&encoded[8 + BOOT_ID_BYTES..])
                    .map_err(CoreJobCommandWireError::Descriptor)?;
                Ok(Self::Prepare {
                    boot_id,
                    descriptor,
                })
            }
            CORE_JOB_CANCEL => {
                if encoded[16..].iter().any(|byte| *byte != 0) {
                    return Err(CoreJobCommandWireError::Reserved);
                }
                let prepare_id = read_u64(encoded, 8);
                if prepare_id == 0 {
                    return Err(CoreJobCommandWireError::PrepareId);
                }
                Ok(Self::Cancel { prepare_id })
            }
            CORE_JOB_COMMIT => {
                if encoded[8 + JOB_COMMIT_WIRE_BYTES..]
                    .iter()
                    .any(|byte| *byte != 0)
                {
                    return Err(CoreJobCommandWireError::Reserved);
                }
                JobCommitRequest::decode(&encoded[8..8 + JOB_COMMIT_WIRE_BYTES])
                    .map(Self::Commit)
                    .map_err(CoreJobCommandWireError::Schedule)
            }
            CORE_JOB_CONFIRM | CORE_JOB_ABORT => {
                if encoded[8 + JOB_SCHEDULE_REFERENCE_WIRE_BYTES..]
                    .iter()
                    .any(|byte| *byte != 0)
                {
                    return Err(CoreJobCommandWireError::Reserved);
                }
                let reference = JobScheduleReference::decode(
                    &encoded[8..8 + JOB_SCHEDULE_REFERENCE_WIRE_BYTES],
                )
                .map_err(CoreJobCommandWireError::Schedule)?;
                match (encoded[6], reference.action) {
                    (CORE_JOB_CONFIRM, JobScheduleReferenceAction::Confirm) => {
                        Ok(Self::Confirm(reference))
                    }
                    (CORE_JOB_ABORT, JobScheduleReferenceAction::Abort) => {
                        Ok(Self::Abort(reference))
                    }
                    _ => Err(CoreJobCommandWireError::Schedule(
                        JobScheduleWireError::Action,
                    )),
                }
            }
            CORE_JOB_RENEW_LEASE => {
                if encoded[8 + JOB_LEASE_RENEW_WIRE_BYTES..]
                    .iter()
                    .any(|byte| *byte != 0)
                {
                    return Err(CoreJobCommandWireError::Reserved);
                }
                JobLeaseRenewRequest::decode(&encoded[8..8 + JOB_LEASE_RENEW_WIRE_BYTES])
                    .map(Self::RenewLease)
                    .map_err(CoreJobCommandWireError::Schedule)
            }
            received => Err(CoreJobCommandWireError::Action { received }),
        }
    }
}

/// Cross-core control rejection before real-time job state changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoreJobCommandWireError {
    /// Payload was not exactly [`CORE_JOB_COMMAND_WIRE_BYTES`].
    WireLength,
    /// Command magic did not match.
    Magic,
    /// Command version was unsupported.
    Version,
    /// Action selector was unknown.
    Action {
        /// Received action value.
        received: u8,
    },
    /// A reserved byte was nonzero.
    Reserved,
    /// Cancellation used the zero prepare sentinel.
    PrepareId,
    /// Preparation carried the absent boot identity sentinel.
    BootId,
    /// Embedded prepare descriptor was invalid.
    Descriptor(JobDescriptorWireError),
    /// Embedded commit/reference body was invalid.
    Schedule(JobScheduleWireError),
}

/// Exact idempotent cancellation request for one boot-local preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobCancelRequest {
    /// Nonzero preparation correlation to invalidate.
    pub prepare_id: u64,
}

impl JobCancelRequest {
    /// Encodes the sole V1 cancellation field.
    pub fn encode(self) -> Result<[u8; JOB_CANCEL_WIRE_BYTES], JobCancelWireError> {
        if self.prepare_id == 0 {
            return Err(JobCancelWireError::PrepareId);
        }
        Ok(self.prepare_id.to_le_bytes())
    }

    /// Decodes an exact nonzero preparation correlation.
    pub fn decode(encoded: &[u8]) -> Result<Self, JobCancelWireError> {
        if encoded.len() != JOB_CANCEL_WIRE_BYTES {
            return Err(JobCancelWireError::WireLength);
        }
        let request = Self {
            prepare_id: read_u64(encoded, 0),
        };
        request.encode()?;
        Ok(request)
    }
}

/// Canonical `JobCancel` body rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobCancelWireError {
    /// Body was not exactly eight bytes.
    WireLength,
    /// Preparation correlation used the zero sentinel.
    PrepareId,
}

/// Descriptor rejection before storage or a queue is touched.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorError {
    /// Boot-local preparation correlation was zero.
    PrepareId,
    /// Selected object was not executable machine-job data.
    ObjectKind,
    /// Object or manifest identity was unbound.
    ContentIdentity,
    /// Descriptor and compile-time executor axis widths diverged.
    AxisCount {
        /// Descriptor width.
        encoded: u8,
        /// Executor width.
        expected: usize,
    },
    /// The selected executor does not implement this valid machine-IR family.
    ExecutionKind,
    /// Stream identity used the zero sentinel.
    StreamIdentity,
    /// Capability identity used the zero sentinel.
    CapabilityIdentity,
    /// Configuration identity used the zero sentinel.
    ConfigurationIdentity,
    /// V3 full partitions must begin at relative tick zero.
    FirstTick,
    /// A fixed position slot above the selected axis width was nonzero.
    InitialPosition,
    /// A required motion/block bound was zero.
    Limits,
    /// Object length was not a representable nonempty block multiple.
    PartitionLayout,
    /// Explicit count did not equal the count derived from object length.
    BlockCount {
        /// Descriptor count.
        encoded: u32,
        /// Object-derived count.
        derived: u32,
    },
}

/// Common bounded progress facts exposed by job actors for every execution
/// record family. Family-specific recurrence state remains private to its
/// independent validator and cannot be forged through this status view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineStreamProgress<const AXES: usize> {
    /// Blocks independently admitted so far.
    pub accepted_blocks: u32,
    /// Exact immutable block count.
    pub expected_blocks: u32,
    /// Exclusive terminal stream-relative tick.
    pub end_tick: StreamTick,
    /// Family-specific terminal position lattice. Motion and direct-step kinds
    /// report relative integer steps; servo recurrence reports absolute Q31.32 bits.
    pub position: [i64; AXES],
    /// Digest required by the next block, or terminal chain digest.
    pub block_digest: Digest,
    /// Whether the complete declared stream was admitted.
    pub complete: bool,
}

impl<const AXES: usize> From<MotionStreamProgress<AXES>> for MachineStreamProgress<AXES> {
    fn from(progress: MotionStreamProgress<AXES>) -> Self {
        Self {
            accepted_blocks: progress.accepted_blocks,
            expected_blocks: progress.expected_blocks,
            end_tick: progress.end_tick,
            position: progress.position,
            block_digest: progress.block_digest,
            complete: progress.complete,
        }
    }
}

impl<const AXES: usize> From<FiniteDifferenceStreamProgress<AXES>> for MachineStreamProgress<AXES> {
    fn from(progress: FiniteDifferenceStreamProgress<AXES>) -> Self {
        Self {
            accepted_blocks: progress.accepted_blocks,
            expected_blocks: progress.expected_blocks,
            end_tick: progress.end_tick,
            position: progress.position,
            block_digest: progress.block_digest,
            complete: progress.complete,
        }
    }
}

impl<const AXES: usize> From<ServoFiniteDifferenceStreamProgress<AXES>>
    for MachineStreamProgress<AXES>
{
    fn from(progress: ServoFiniteDifferenceStreamProgress<AXES>) -> Self {
        Self {
            accepted_blocks: progress.accepted_blocks,
            expected_blocks: progress.expected_blocks,
            end_tick: progress.end_tick,
            position: progress.terminal_state.position,
            block_digest: progress.block_digest,
            complete: progress.complete,
        }
    }
}

enum ExecutionStreamValidator<const AXES: usize> {
    Motion(MotionStreamValidator<AXES>),
    FiniteDifference(FiniteDifferenceStreamValidator<AXES>),
    ServoFiniteDifference(ServoFiniteDifferenceStreamValidator<AXES>),
}

impl<const AXES: usize> ExecutionStreamValidator<AXES> {
    fn new(descriptor: JobDescriptor) -> Result<Self, BlockError> {
        match descriptor.execution_kind {
            ExecutionKind::Motion => Ok(Self::Motion(MotionStreamValidator::new(
                descriptor.block_count,
                descriptor.first_expectation(),
                descriptor.limits,
            )?)),
            ExecutionKind::FiniteDifference => Ok(Self::FiniteDifference(
                FiniteDifferenceStreamValidator::new(
                    descriptor.block_count,
                    descriptor.first_expectation(),
                    FiniteDifferenceBlockValidationLimits {
                        maximum_block_ticks: descriptor.limits.maximum_block_ticks,
                        segment: FiniteDifferenceValidationLimits {
                            maximum_segment_ticks: descriptor.limits.segment.maximum_segment_ticks,
                            maximum_update_count: descriptor.maximum_dense_updates,
                            required_update_period_ticks: descriptor.dense_update_period_ticks,
                            maximum_steps_per_segment: descriptor
                                .limits
                                .segment
                                .maximum_steps_per_segment,
                            maximum_absolute_first_difference: [FINITE_DIFFERENCE_ONE_STEP
                                .unsigned_abs()
                                - 1;
                                AXES],
                        },
                    },
                )?,
            )),
            ExecutionKind::ServoFiniteDifference => Err(BlockError::ServoProfileRequired),
        }
    }

    fn new_servo(
        descriptor: JobDescriptor,
        limits: ServoFiniteDifferenceBlockValidationLimits<AXES>,
    ) -> Result<Self, BlockError> {
        if descriptor.execution_kind != ExecutionKind::ServoFiniteDifference {
            return Err(BlockError::Kind {
                received: descriptor.execution_kind as u8,
            });
        }
        validate_servo_profile_binding(descriptor, limits)?;
        let initial_position = descriptor
            .initial_position_for::<AXES>()
            .map_err(|_| BlockError::ServoProfileRequired)?;
        Ok(Self::ServoFiniteDifference(
            ServoFiniteDifferenceStreamValidator::new(
                descriptor.block_count,
                descriptor.first_expectation(),
                initial_position,
                limits,
            )?,
        ))
    }

    fn accept(
        &mut self,
        block: &ExecutionBlock,
    ) -> Result<MachineStreamProgress<AXES>, BlockError> {
        match self {
            Self::Motion(validator) => validator.accept(block).map(Into::into),
            Self::FiniteDifference(validator) => validator.accept(block).map(Into::into),
            Self::ServoFiniteDifference(validator) => validator.accept(block).map(Into::into),
        }
    }

    fn finish(&self) -> Result<MachineStreamProgress<AXES>, BlockError> {
        match self {
            Self::Motion(validator) => validator.finish().map(Into::into),
            Self::FiniteDifference(validator) => validator.finish().map(Into::into),
            Self::ServoFiniteDifference(validator) => validator.finish().map(Into::into),
        }
    }
}

fn validate_servo_profile_binding<const AXES: usize>(
    descriptor: JobDescriptor,
    limits: ServoFiniteDifferenceBlockValidationLimits<AXES>,
) -> Result<(), BlockError> {
    let maximum_position_delta = limits
        .segment
        .maximum_position_delta_bits
        .iter()
        .copied()
        .max()
        .ok_or(BlockError::ServoProfileRequired)?;
    if descriptor.limits.maximum_block_ticks != limits.maximum_block_ticks
        || descriptor.limits.segment.maximum_segment_ticks != limits.segment.maximum_segment_ticks
        || descriptor.limits.segment.maximum_steps_per_segment != maximum_position_delta
        || descriptor.maximum_dense_updates != limits.segment.maximum_update_count
        || descriptor.dense_update_period_ticks != limits.segment.required_update_period_ticks
    {
        return Err(BlockError::ServoProfileRequired);
    }
    Ok(())
}

/// Minimal fixed-credit sink used by the core-0 prefetch state machine.
pub trait WorkSink {
    /// Available inline ownership slots.
    fn free_capacity(&self) -> usize;

    /// Transfers ownership or returns the unchanged block when no credit exists.
    #[allow(
        clippy::result_large_err,
        reason = "the no-allocation producer must retain the complete inline block on backpressure"
    )]
    fn try_send(&mut self, block: ExecutionBlock) -> Result<(), ExecutionBlock>;
}

impl<
    const COMMANDS: usize,
    const TELEMETRY: usize,
    const COMMAND_PAYLOAD: usize,
    const TELEMETRY_PAYLOAD: usize,
    const WORK_BLOCKS: usize,
> WorkSink
    for ServiceEndpoint<'_, COMMANDS, TELEMETRY, COMMAND_PAYLOAD, TELEMETRY_PAYLOAD, WORK_BLOCKS>
{
    fn free_capacity(&self) -> usize {
        self.work_free_capacity()
    }

    fn try_send(&mut self, block: ExecutionBlock) -> Result<(), ExecutionBlock> {
        self.try_send_work(block).map_err(|error| match error {
            TrySendError::Full(block) => block,
        })
    }
}

/// Minimal fixed-credit source used by the core-1 admission state machine.
pub trait WorkSource {
    /// Takes the next inline-owned block, or `None` when the ring is empty.
    fn try_receive(&mut self) -> Option<ExecutionBlock>;

    /// Blocks still owned by the ring.
    fn depth(&self) -> usize;
}

impl<
    const COMMANDS: usize,
    const TELEMETRY: usize,
    const COMMAND_PAYLOAD: usize,
    const TELEMETRY_PAYLOAD: usize,
    const WORK_BLOCKS: usize,
> WorkSource
    for RealtimeEndpoint<'_, COMMANDS, TELEMETRY, COMMAND_PAYLOAD, TELEMETRY_PAYLOAD, WORK_BLOCKS>
{
    fn try_receive(&mut self) -> Option<ExecutionBlock> {
        self.try_receive_work().ok()
    }

    fn depth(&self) -> usize {
        self.work_depth()
    }
}

/// Core-0 immutable read/assembly/validation lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ServiceJobState {
    /// Publication is open and more blocks remain to validate or transfer.
    Prefetching = 1,
    /// Every declared block was validated and transferred into queue ownership.
    Complete = 2,
    /// Local cancellation discarded unread and pending service-side bytes.
    Cancelled = 3,
    /// Media, machine IR, or an internal invariant failed closed.
    Faulted = 4,
}

impl ServiceJobState {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Prefetching),
            2 => Some(Self::Complete),
            3 => Some(Self::Cancelled),
            4 => Some(Self::Faulted),
            _ => None,
        }
    }
}

/// One bounded service-core prefetch observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceJobStatus<const AXES: usize> {
    /// Boot-local preparation correlation.
    pub prepare_id: u64,
    /// Current service-side lifecycle.
    pub state: ServiceJobState,
    /// Structurally and semantically validated blocks.
    pub validated_blocks: u32,
    /// Blocks whose ownership reached the cross-core ring.
    pub sent_blocks: u32,
    /// Exact total declared by immutable object length.
    pub total_blocks: u32,
    /// Storage chunks read through the verified publication cursor.
    pub storage_chunks_read: u32,
    /// Complete service-side stream facts, only at terminal prefetch.
    pub final_progress: Option<MachineStreamProgress<AXES>>,
}

/// Fixed service-side prefetch summary safe for authenticated status responses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceJobReport {
    /// Exact boot-local preparation correlation.
    pub prepare_id: u64,
    /// Core-0 prefetch lifecycle state.
    pub state: ServiceJobState,
    /// Descriptor axis width.
    pub axis_count: u8,
    /// Blocks independently validated on core 0.
    pub validated_blocks: u32,
    /// Blocks transferred into ring ownership.
    pub sent_blocks: u32,
    /// Immutable partition block count.
    pub total_blocks: u32,
    /// Verified storage chunks read.
    pub storage_chunks_read: u32,
    /// Producer credits currently available.
    pub queue_free: u32,
    /// Blocks currently owned by the ring.
    pub queue_depth: u32,
    /// Terminal stream-relative tick and digest, only when complete.
    pub final_progress: Option<(StreamTick, Digest)>,
}

impl ServiceJobReport {
    /// Reduces a generic actor status to fixed external facts.
    pub fn from_status<const AXES: usize>(
        status: ServiceJobStatus<AXES>,
        queue_free: usize,
        queue_depth: usize,
    ) -> Result<Self, ServiceJobReportWireError> {
        let report = Self {
            prepare_id: status.prepare_id,
            state: status.state,
            axis_count: u8::try_from(AXES).map_err(|_| ServiceJobReportWireError::AxisCount)?,
            validated_blocks: status.validated_blocks,
            sent_blocks: status.sent_blocks,
            total_blocks: status.total_blocks,
            storage_chunks_read: status.storage_chunks_read,
            queue_free: u32::try_from(queue_free)
                .map_err(|_| ServiceJobReportWireError::QueueDepth)?,
            queue_depth: u32::try_from(queue_depth)
                .map_err(|_| ServiceJobReportWireError::QueueDepth)?,
            final_progress: status
                .final_progress
                .map(|progress| (progress.end_tick, progress.block_digest)),
        };
        report.validate()?;
        Ok(report)
    }

    /// Encodes one canonical fixed service report.
    pub fn encode(self) -> Result<[u8; SERVICE_JOB_REPORT_WIRE_BYTES], ServiceJobReportWireError> {
        self.validate()?;
        let mut encoded = [0_u8; SERVICE_JOB_REPORT_WIRE_BYTES];
        encoded[0..8].copy_from_slice(&SERVICE_JOB_REPORT_MAGIC);
        encoded[8..10].copy_from_slice(&SERVICE_JOB_REPORT_VERSION.to_le_bytes());
        encoded[10] = self.state as u8;
        encoded[11] = if self.final_progress.is_some() {
            SERVICE_REPORT_FLAG_FINAL
        } else {
            0
        };
        encoded[12] = self.axis_count;
        // Bytes 13..16 and 88..96 remain reserved zero.
        encoded[16..24].copy_from_slice(&self.prepare_id.to_le_bytes());
        encoded[24..28].copy_from_slice(&self.validated_blocks.to_le_bytes());
        encoded[28..32].copy_from_slice(&self.sent_blocks.to_le_bytes());
        encoded[32..36].copy_from_slice(&self.total_blocks.to_le_bytes());
        encoded[36..40].copy_from_slice(&self.storage_chunks_read.to_le_bytes());
        encoded[40..44].copy_from_slice(&self.queue_free.to_le_bytes());
        encoded[44..48].copy_from_slice(&self.queue_depth.to_le_bytes());
        if let Some((end_tick, digest)) = self.final_progress {
            encoded[48..56].copy_from_slice(&end_tick.0.to_le_bytes());
            encoded[56..88].copy_from_slice(&digest.0);
        }
        Ok(encoded)
    }

    /// Decodes and validates one exact service report.
    pub fn decode(encoded: &[u8]) -> Result<Self, ServiceJobReportWireError> {
        if encoded.len() != SERVICE_JOB_REPORT_WIRE_BYTES {
            return Err(ServiceJobReportWireError::WireLength);
        }
        if encoded[0..8] != SERVICE_JOB_REPORT_MAGIC {
            return Err(ServiceJobReportWireError::Magic);
        }
        if read_u16(encoded, 8) != SERVICE_JOB_REPORT_VERSION {
            return Err(ServiceJobReportWireError::Version);
        }
        let flags = encoded[11];
        if flags & !SERVICE_REPORT_FLAG_FINAL != 0
            || encoded[13..16].iter().any(|byte| *byte != 0)
            || encoded[88..96].iter().any(|byte| *byte != 0)
        {
            return Err(ServiceJobReportWireError::Reserved);
        }
        let mut final_digest = [0_u8; 32];
        final_digest.copy_from_slice(&encoded[56..88]);
        let final_progress = if flags & SERVICE_REPORT_FLAG_FINAL != 0 {
            Some((StreamTick(read_u64(encoded, 48)), Digest(final_digest)))
        } else {
            if encoded[48..88].iter().any(|byte| *byte != 0) {
                return Err(ServiceJobReportWireError::Reserved);
            }
            None
        };
        let report = Self {
            prepare_id: read_u64(encoded, 16),
            state: ServiceJobState::from_wire(encoded[10])
                .ok_or(ServiceJobReportWireError::State)?,
            axis_count: encoded[12],
            validated_blocks: read_u32(encoded, 24),
            sent_blocks: read_u32(encoded, 28),
            total_blocks: read_u32(encoded, 32),
            storage_chunks_read: read_u32(encoded, 36),
            queue_free: read_u32(encoded, 40),
            queue_depth: read_u32(encoded, 44),
            final_progress,
        };
        report.validate()?;
        if report.encode()? != encoded {
            return Err(ServiceJobReportWireError::Noncanonical);
        }
        Ok(report)
    }

    fn validate(self) -> Result<(), ServiceJobReportWireError> {
        if self.prepare_id == 0 {
            return Err(ServiceJobReportWireError::PrepareId);
        }
        if self.axis_count == 0 || usize::from(self.axis_count) > MAX_EXECUTION_AXES {
            return Err(ServiceJobReportWireError::AxisCount);
        }
        if self.total_blocks == 0
            || self.sent_blocks > self.validated_blocks
            || self.validated_blocks > self.total_blocks
        {
            return Err(ServiceJobReportWireError::Counts);
        }
        if self.final_progress.is_some() != (self.state == ServiceJobState::Complete) {
            return Err(ServiceJobReportWireError::Progress);
        }
        if self.state == ServiceJobState::Complete
            && (self.sent_blocks != self.total_blocks || self.validated_blocks != self.total_blocks)
        {
            return Err(ServiceJobReportWireError::Progress);
        }
        if let Some((_, digest)) = self.final_progress
            && digest.is_zero()
        {
            return Err(ServiceJobReportWireError::Progress);
        }
        Ok(())
    }
}

/// Fixed service report decoding or consistency failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceJobReportWireError {
    /// Payload length was not exact.
    WireLength,
    /// Report magic did not match.
    Magic,
    /// Report version was unsupported.
    Version,
    /// State byte was unknown.
    State,
    /// Flags, absent fields, or reserved bytes were nonzero.
    Reserved,
    /// Prepare correlation was zero.
    PrepareId,
    /// Axis width was outside the machine-block schema range.
    AxisCount,
    /// Block counts were empty or out of order.
    Counts,
    /// State and terminal progress disagreed.
    Progress,
    /// Queue counters did not fit their wire fields.
    QueueDepth,
    /// Valid fields used a noncanonical representation.
    Noncanonical,
}

/// Why one prefetch invocation yielded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrefetchYield {
    /// At least one storage or queue transition advanced.
    Progress,
    /// No queue ownership credit was available.
    Backpressured,
    /// Every immutable block was validated and transferred.
    Complete,
}

/// Status plus the bounded reason for returning to the executor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrefetchObservation<const AXES: usize> {
    /// Yield reason.
    pub yielded: PrefetchYield,
    /// Current exact actor status.
    pub status: ServiceJobStatus<AXES>,
}

/// Sole core-0 owner of one open job cursor and its partial buffers.
pub struct ServicePrefetch<const AXES: usize> {
    descriptor: JobDescriptor,
    reader: PublishedReader,
    assembler: PartitionAssembler,
    validator: ExecutionStreamValidator<AXES>,
    storage: [u8; MAX_MEDIA_CHUNK_BYTES],
    storage_offset: usize,
    storage_len: usize,
    pending: Option<ExecutionBlock>,
    state: ServiceJobState,
    validated_blocks: u32,
    sent_blocks: u32,
    storage_chunks_read: u32,
    final_progress: Option<MachineStreamProgress<AXES>>,
}

impl<const AXES: usize> ServicePrefetch<AXES> {
    /// Opens the exact publication and allocates only fixed inline state.
    pub async fn open<D>(
        cache: &mut ProvisionedCache<D>,
        descriptor: JobDescriptor,
    ) -> Result<Self, JobError<D::Error>>
    where
        D: AsyncBlockDevice,
    {
        descriptor
            .validate::<AXES>()
            .map_err(JobError::Descriptor)?;
        let validator = ExecutionStreamValidator::new(descriptor).map_err(JobError::Machine)?;
        Self::open_with_validator(cache, descriptor, validator).await
    }

    /// Opens a servo partition with the exact active configuration-derived profile.
    pub async fn open_servo<D>(
        cache: &mut ProvisionedCache<D>,
        descriptor: JobDescriptor,
        limits: ServoFiniteDifferenceBlockValidationLimits<AXES>,
    ) -> Result<Self, JobError<D::Error>>
    where
        D: AsyncBlockDevice,
    {
        descriptor
            .validate::<AXES>()
            .map_err(JobError::Descriptor)?;
        let validator =
            ExecutionStreamValidator::new_servo(descriptor, limits).map_err(JobError::Machine)?;
        Self::open_with_validator(cache, descriptor, validator).await
    }

    async fn open_with_validator<D>(
        cache: &mut ProvisionedCache<D>,
        descriptor: JobDescriptor,
        validator: ExecutionStreamValidator<AXES>,
    ) -> Result<Self, JobError<D::Error>>
    where
        D: AsyncBlockDevice,
    {
        let reader = cache
            .open_published(descriptor.partition)
            .await
            .map_err(JobError::Storage)?;
        let assembler = PartitionAssembler::new(descriptor.partition.object.byte_len)
            .map_err(JobError::Machine)?;
        Ok(Self {
            descriptor,
            reader,
            assembler,
            validator,
            storage: [0; MAX_MEDIA_CHUNK_BYTES],
            storage_offset: 0,
            storage_len: 0,
            pending: None,
            state: ServiceJobState::Prefetching,
            validated_blocks: 0,
            sent_blocks: 0,
            storage_chunks_read: 0,
            final_progress: None,
        })
    }

    /// Performs at most one SD chunk read and yields on queue backpressure.
    pub async fn step<D, S>(
        &mut self,
        cache: &mut ProvisionedCache<D>,
        sink: &mut S,
    ) -> Result<PrefetchObservation<AXES>, JobError<D::Error>>
    where
        D: AsyncBlockDevice,
        S: WorkSink,
    {
        match self.state {
            ServiceJobState::Complete => {
                return Ok(self.observation(PrefetchYield::Complete));
            }
            ServiceJobState::Cancelled | ServiceJobState::Faulted => {
                return Err(JobError::State);
            }
            ServiceJobState::Prefetching => {}
        }
        let result = self.step_inner(cache, sink).await;
        if result.is_err() {
            self.state = ServiceJobState::Faulted;
            self.pending = None;
            self.storage.fill(0);
            self.storage_offset = 0;
            self.storage_len = 0;
        }
        result
    }

    async fn step_inner<D, S>(
        &mut self,
        cache: &mut ProvisionedCache<D>,
        sink: &mut S,
    ) -> Result<PrefetchObservation<AXES>, JobError<D::Error>>
    where
        D: AsyncBlockDevice,
        S: WorkSink,
    {
        let mut read_performed = false;
        let mut advanced = false;
        loop {
            if let Some(block) = self.pending.take() {
                match sink.try_send(block) {
                    Ok(()) => {
                        self.sent_blocks = self
                            .sent_blocks
                            .checked_add(1)
                            .ok_or(JobError::Machine(BlockError::Arithmetic))?;
                        advanced = true;
                    }
                    Err(block) => {
                        self.pending = Some(block);
                        return Ok(self.observation(PrefetchYield::Backpressured));
                    }
                }
            }

            if self.storage_offset < self.storage_len {
                let outcome = self
                    .assembler
                    .push(&self.storage[self.storage_offset..self.storage_len])
                    .map_err(JobError::Machine)?;
                let consumed = match &outcome {
                    AssembleOutcome::NeedMore { consumed }
                    | AssembleOutcome::Block { consumed, .. } => *consumed,
                };
                if consumed == 0 {
                    return Err(JobError::Machine(BlockError::Arithmetic));
                }
                self.storage_offset = self
                    .storage_offset
                    .checked_add(consumed)
                    .ok_or(JobError::Machine(BlockError::Arithmetic))?;
                advanced = true;
                if let AssembleOutcome::Block { block, .. } = outcome {
                    let progress = self.validator.accept(&block).map_err(JobError::Machine)?;
                    self.validated_blocks = progress.accepted_blocks;
                    self.pending = Some(block);
                }
                continue;
            }

            self.storage.fill(0);
            self.storage_offset = 0;
            self.storage_len = 0;
            if self.reader.is_complete() {
                self.assembler.finish().map_err(JobError::Machine)?;
                let final_progress = self.validator.finish().map_err(JobError::Machine)?;
                if self.pending.is_some()
                    || self.sent_blocks != self.descriptor.block_count
                    || self.validated_blocks != self.descriptor.block_count
                {
                    return Err(JobError::Machine(BlockError::StreamIncomplete {
                        received: self.sent_blocks,
                        expected: self.descriptor.block_count,
                    }));
                }
                self.final_progress = Some(final_progress);
                self.state = ServiceJobState::Complete;
                return Ok(self.observation(PrefetchYield::Complete));
            }
            if read_performed {
                return Ok(self.observation(if advanced {
                    PrefetchYield::Progress
                } else {
                    PrefetchYield::Backpressured
                }));
            }
            if sink.free_capacity() == 0 {
                return Ok(self.observation(PrefetchYield::Backpressured));
            }
            let chunk = cache
                .read_next_published(&mut self.reader, &mut self.storage)
                .await
                .map_err(JobError::Storage)?
                .ok_or(JobError::Machine(BlockError::PartitionIncomplete {
                    received: self.assembler.produced(),
                    expected: self.assembler.expected(),
                }))?;
            self.storage_len = usize::try_from(chunk.byte_len)
                .map_err(|_| JobError::Machine(BlockError::Arithmetic))?;
            if self.storage_len == 0 || self.storage_len > self.storage.len() {
                return Err(JobError::Machine(BlockError::PartitionLength));
            }
            self.storage_chunks_read = self
                .storage_chunks_read
                .checked_add(1)
                .ok_or(JobError::Machine(BlockError::Arithmetic))?;
            read_performed = true;
            advanced = true;
        }
    }

    /// Cancels unread and pending service-side data; core 1 must drain its ring.
    pub fn cancel(&mut self) {
        if self.state == ServiceJobState::Prefetching {
            self.state = ServiceJobState::Cancelled;
            self.pending = None;
            self.storage.fill(0);
            self.storage_offset = 0;
            self.storage_len = 0;
        }
    }

    /// Exact non-I/O status.
    pub const fn status(&self) -> ServiceJobStatus<AXES> {
        ServiceJobStatus {
            prepare_id: self.descriptor.prepare_id,
            state: self.state,
            validated_blocks: self.validated_blocks,
            sent_blocks: self.sent_blocks,
            total_blocks: self.descriptor.block_count,
            storage_chunks_read: self.storage_chunks_read,
            final_progress: self.final_progress,
        }
    }

    fn observation(&self, yielded: PrefetchYield) -> PrefetchObservation<AXES> {
        PrefetchObservation {
            yielded,
            status: self.status(),
        }
    }
}

/// Core-1 job lifecycle independent of storage and network state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RealtimeJobState {
    /// Descriptor installed; no block is currently owned by the executor.
    Prepared = 1,
    /// One or more independently validated blocks are owned by the executor.
    Admitted = 2,
    /// Every block was acknowledged consumed in exact order.
    Complete = 3,
    /// Local cancellation invalidated all outstanding work.
    Cancelled = 4,
    /// Validation or token mismatch faulted the stream.
    Faulted = 5,
}

impl RealtimeJobState {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Prepared),
            2 => Some(Self::Admitted),
            3 => Some(Self::Complete),
            4 => Some(Self::Cancelled),
            5 => Some(Self::Faulted),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct Outstanding<const AXES: usize> {
    prepare_id: u64,
    sequence: u32,
    block_digest: alumina_protocol::Digest,
    progress: MachineStreamProgress<AXES>,
}

/// Core-1 status safe to publish as bounded telemetry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeJobStatus<const AXES: usize> {
    /// Boot-local preparation correlation.
    pub prepare_id: u64,
    /// Current local lifecycle.
    pub state: RealtimeJobState,
    /// Blocks that passed independent core-1 validation.
    pub admitted_blocks: u32,
    /// Blocks acknowledged complete by the future execution engine.
    pub completed_blocks: u32,
    /// Exact immutable total.
    pub total_blocks: u32,
    /// Validated facts through the newest block owned by the executor pipeline.
    pub admitted_progress: Option<MachineStreamProgress<AXES>>,
    /// Terminal facts for the completed prefix.
    pub completed_progress: Option<MachineStreamProgress<AXES>>,
    /// Whether one or more admitted blocks are owned outside this state machine.
    pub outstanding: bool,
}

/// One independently validated block owned by the future RT execution engine.
pub struct AdmittedBlock<const AXES: usize> {
    prepare_id: u64,
    block: ExecutionBlock,
    progress: MachineStreamProgress<AXES>,
}

impl<const AXES: usize> AdmittedBlock<AXES> {
    /// Structurally and semantically validated immutable header.
    pub fn header(&self) -> ExecutionBlockHeader {
        self.block.header()
    }

    /// Iterates exact relative-time segments without copying or allocation.
    pub fn segments(&self) -> Result<MotionSegments<'_, AXES>, BlockError> {
        self.block.motion_segments()
    }

    /// Iterates direct finite-difference records without copying or allocation.
    pub fn finite_difference_segments(
        &self,
    ) -> Result<FiniteDifferenceSegments<'_, AXES>, BlockError> {
        self.block.finite_difference_segments()
    }

    /// Iterates fixed-cadence servo-setpoint records without copying or allocation.
    pub fn servo_finite_difference_segments(
        &self,
    ) -> Result<ServoFiniteDifferenceSegments<'_, AXES>, BlockError> {
        self.block.servo_finite_difference_segments()
    }

    /// Cumulative independently validated stream facts through this block.
    pub const fn progress(&self) -> MachineStreamProgress<AXES> {
        self.progress
    }
}

/// Result of one nonblocking core-1 queue poll.
#[allow(
    clippy::large_enum_variant,
    reason = "the admitted variant transfers the complete inline block without allocation or aliasing"
)]
pub enum RealtimePoll<const AXES: usize> {
    /// Ring was empty.
    Empty,
    /// The bounded admission window is full.
    Outstanding,
    /// Newly owned and independently validated block.
    Block(AdmittedBlock<AXES>),
}

/// Core-1 independent stream validator and bounded ownership gate.
pub struct RealtimeJob<const AXES: usize> {
    descriptor: JobDescriptor,
    validator: ExecutionStreamValidator<AXES>,
    state: RealtimeJobState,
    outstanding: [Option<Outstanding<AXES>>; REALTIME_BLOCK_WINDOW],
    outstanding_head: usize,
    outstanding_len: usize,
    admitted_blocks: u32,
    completed_blocks: u32,
    admitted_progress: Option<MachineStreamProgress<AXES>>,
    completed_progress: Option<MachineStreamProgress<AXES>>,
}

impl<const AXES: usize> RealtimeJob<AXES> {
    /// Installs an independently validated descriptor with no storage authority.
    pub fn prepare(descriptor: JobDescriptor) -> Result<Self, JobError<core::convert::Infallible>> {
        descriptor
            .validate::<AXES>()
            .map_err(JobError::Descriptor)?;
        let validator = ExecutionStreamValidator::new(descriptor).map_err(JobError::Machine)?;
        Ok(Self::prepare_with_validator(descriptor, validator))
    }

    /// Installs a servo descriptor with the independently derived active profile.
    pub fn prepare_servo(
        descriptor: JobDescriptor,
        limits: ServoFiniteDifferenceBlockValidationLimits<AXES>,
    ) -> Result<Self, JobError<core::convert::Infallible>> {
        descriptor
            .validate::<AXES>()
            .map_err(JobError::Descriptor)?;
        let validator =
            ExecutionStreamValidator::new_servo(descriptor, limits).map_err(JobError::Machine)?;
        Ok(Self::prepare_with_validator(descriptor, validator))
    }

    fn prepare_with_validator(
        descriptor: JobDescriptor,
        validator: ExecutionStreamValidator<AXES>,
    ) -> Self {
        Self {
            descriptor,
            validator,
            state: RealtimeJobState::Prepared,
            outstanding: [None; REALTIME_BLOCK_WINDOW],
            outstanding_head: 0,
            outstanding_len: 0,
            admitted_blocks: 0,
            completed_blocks: 0,
            admitted_progress: None,
            completed_progress: None,
        }
    }

    /// Takes and validates at most one work block without waiting on core 0.
    pub fn poll<S: WorkSource>(
        &mut self,
        source: &mut S,
    ) -> Result<RealtimePoll<AXES>, JobError<core::convert::Infallible>> {
        match self.state {
            RealtimeJobState::Prepared | RealtimeJobState::Admitted => {}
            RealtimeJobState::Complete
            | RealtimeJobState::Cancelled
            | RealtimeJobState::Faulted => return Err(JobError::State),
        }
        if self.outstanding_len == REALTIME_BLOCK_WINDOW {
            return Ok(RealtimePoll::Outstanding);
        }
        let Some(block) = source.try_receive() else {
            return Ok(RealtimePoll::Empty);
        };
        let progress = match self.validator.accept(&block) {
            Ok(progress) => progress,
            Err(error) => {
                self.state = RealtimeJobState::Faulted;
                self.clear_outstanding();
                self.admitted_progress = None;
                return Err(JobError::Machine(error));
            }
        };
        let header = block.header();
        let tail = (self.outstanding_head + self.outstanding_len) % REALTIME_BLOCK_WINDOW;
        self.outstanding[tail] = Some(Outstanding {
            prepare_id: self.descriptor.prepare_id,
            sequence: header.sequence,
            block_digest: header.block_digest,
            progress,
        });
        self.outstanding_len += 1;
        self.admitted_blocks = progress.accepted_blocks;
        self.admitted_progress = Some(progress);
        self.state = RealtimeJobState::Admitted;
        Ok(RealtimePoll::Block(AdmittedBlock {
            prepare_id: self.descriptor.prepare_id,
            block,
            progress,
        }))
    }

    /// Acknowledges that the future hardware engine consumed this exact block.
    pub fn acknowledge(
        &mut self,
        admitted: AdmittedBlock<AXES>,
    ) -> Result<RealtimeJobStatus<AXES>, JobError<core::convert::Infallible>> {
        if self.state != RealtimeJobState::Admitted {
            return Err(JobError::State);
        }
        let expected = self.outstanding[self.outstanding_head].ok_or(JobError::AdmissionToken)?;
        let header = admitted.block.header();
        if admitted.prepare_id != expected.prepare_id
            || admitted.prepare_id != self.descriptor.prepare_id
            || header.sequence != expected.sequence
            || header.block_digest != expected.block_digest
            || admitted.progress != expected.progress
        {
            self.state = RealtimeJobState::Faulted;
            self.clear_outstanding();
            self.admitted_progress = None;
            return Err(JobError::AdmissionToken);
        }
        self.outstanding[self.outstanding_head] = None;
        self.outstanding_head = if self.outstanding_len == 1 {
            0
        } else {
            (self.outstanding_head + 1) % REALTIME_BLOCK_WINDOW
        };
        self.outstanding_len -= 1;
        self.completed_blocks = admitted.progress.accepted_blocks;
        self.completed_progress = Some(admitted.progress);
        if admitted.progress.complete {
            if self.outstanding_len != 0 {
                self.state = RealtimeJobState::Faulted;
                self.clear_outstanding();
                self.admitted_progress = None;
                return Err(JobError::AdmissionToken);
            }
            if let Err(error) = self.validator.finish() {
                self.state = RealtimeJobState::Faulted;
                self.clear_outstanding();
                self.admitted_progress = None;
                return Err(JobError::Machine(error));
            }
            self.admitted_progress = None;
            self.state = RealtimeJobState::Complete;
        } else if self.outstanding_len == 0 {
            self.admitted_progress = None;
            self.state = RealtimeJobState::Prepared;
        } else {
            self.state = RealtimeJobState::Admitted;
        }
        Ok(self.status())
    }

    /// Invalidates the current token. The safety owner must stop output first.
    pub fn cancel(&mut self) {
        if !matches!(
            self.state,
            RealtimeJobState::Complete | RealtimeJobState::Faulted
        ) {
            self.state = RealtimeJobState::Cancelled;
            self.clear_outstanding();
            self.admitted_progress = None;
        }
    }

    /// Invalidates the current token after the sole hardware owner has already
    /// applied its board-safe transaction.
    pub fn fault(&mut self) {
        if !matches!(
            self.state,
            RealtimeJobState::Complete | RealtimeJobState::Cancelled | RealtimeJobState::Faulted
        ) {
            self.state = RealtimeJobState::Faulted;
            self.clear_outstanding();
            self.admitted_progress = None;
        }
    }

    /// Drops queued blocks after cancellation/fault before another prepare.
    pub fn drain<S: WorkSource>(
        &self,
        source: &mut S,
    ) -> Result<u32, JobError<core::convert::Infallible>> {
        if !matches!(
            self.state,
            RealtimeJobState::Cancelled | RealtimeJobState::Faulted
        ) {
            return Err(JobError::State);
        }
        let mut drained = 0_u32;
        while source.try_receive().is_some() {
            drained = drained
                .checked_add(1)
                .ok_or(JobError::Machine(BlockError::Arithmetic))?;
        }
        Ok(drained)
    }

    /// Exact nonblocking status.
    pub const fn status(&self) -> RealtimeJobStatus<AXES> {
        RealtimeJobStatus {
            prepare_id: self.descriptor.prepare_id,
            state: self.state,
            admitted_blocks: self.admitted_blocks,
            completed_blocks: self.completed_blocks,
            total_blocks: self.descriptor.block_count,
            admitted_progress: self.admitted_progress,
            completed_progress: self.completed_progress,
            outstanding: self.outstanding_len != 0,
        }
    }

    fn clear_outstanding(&mut self) {
        self.outstanding = [None; REALTIME_BLOCK_WINDOW];
        self.outstanding_head = 0;
        self.outstanding_len = 0;
    }
}

/// Fixed, allocation-free summary sent from core 1 to core 0.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeJobReport {
    /// Exact boot-local preparation correlation.
    pub prepare_id: u64,
    /// Core-1 lifecycle state.
    pub state: RealtimeJobState,
    /// Blocks independently validated on core 1.
    pub admitted_blocks: u32,
    /// Blocks acknowledged by the hardware execution owner.
    pub completed_blocks: u32,
    /// Immutable partition block count.
    pub total_blocks: u32,
    /// Blocks waiting in the cross-core ownership ring.
    pub queue_depth: u32,
    /// Facts through the newest outstanding admitted block, when present.
    pub admitted_progress: Option<(StreamTick, Digest)>,
    /// Facts through the last acknowledged block, when present.
    pub completed_progress: Option<(StreamTick, Digest)>,
    /// Whether the execution pipeline currently holds one or more blocks.
    pub outstanding: bool,
}

impl RealtimeJobReport {
    /// Reduces a generic in-core status to the exact cross-core report facts.
    pub fn from_status<const AXES: usize>(
        status: RealtimeJobStatus<AXES>,
        queue_depth: usize,
    ) -> Result<Self, RealtimeJobReportWireError> {
        let queue_depth =
            u32::try_from(queue_depth).map_err(|_| RealtimeJobReportWireError::QueueDepth)?;
        let report = Self {
            prepare_id: status.prepare_id,
            state: status.state,
            admitted_blocks: status.admitted_blocks,
            completed_blocks: status.completed_blocks,
            total_blocks: status.total_blocks,
            queue_depth,
            admitted_progress: status
                .admitted_progress
                .map(|progress| (progress.end_tick, progress.block_digest)),
            completed_progress: status
                .completed_progress
                .map(|progress| (progress.end_tick, progress.block_digest)),
            outstanding: status.outstanding,
        };
        report.validate()?;
        Ok(report)
    }

    /// Encodes one strict fixed-size telemetry payload.
    pub fn encode(
        self,
    ) -> Result<[u8; REALTIME_JOB_REPORT_WIRE_BYTES], RealtimeJobReportWireError> {
        self.validate()?;
        let mut encoded = [0_u8; REALTIME_JOB_REPORT_WIRE_BYTES];
        encoded[0..8].copy_from_slice(&REALTIME_JOB_REPORT_MAGIC);
        encoded[8..10].copy_from_slice(&REALTIME_JOB_REPORT_VERSION.to_le_bytes());
        encoded[10] = self.state as u8;
        let mut flags = u8::from(self.outstanding) * REPORT_FLAG_OUTSTANDING;
        if self.admitted_progress.is_some() {
            flags |= REPORT_FLAG_ADMITTED_PROGRESS;
        }
        if self.completed_progress.is_some() {
            flags |= REPORT_FLAG_COMPLETED_PROGRESS;
        }
        encoded[11] = flags;
        // Bytes 12..16 and 120..128 remain reserved zero.
        encoded[16..24].copy_from_slice(&self.prepare_id.to_le_bytes());
        encoded[24..28].copy_from_slice(&self.admitted_blocks.to_le_bytes());
        encoded[28..32].copy_from_slice(&self.completed_blocks.to_le_bytes());
        encoded[32..36].copy_from_slice(&self.total_blocks.to_le_bytes());
        encoded[36..40].copy_from_slice(&self.queue_depth.to_le_bytes());
        if let Some((end_tick, digest)) = self.admitted_progress {
            encoded[40..48].copy_from_slice(&end_tick.0.to_le_bytes());
            encoded[48..80].copy_from_slice(&digest.0);
        }
        if let Some((end_tick, digest)) = self.completed_progress {
            encoded[80..88].copy_from_slice(&end_tick.0.to_le_bytes());
            encoded[88..120].copy_from_slice(&digest.0);
        }
        Ok(encoded)
    }

    /// Decodes and validates one exact report from the real-time owner.
    pub fn decode(encoded: &[u8]) -> Result<Self, RealtimeJobReportWireError> {
        if encoded.len() != REALTIME_JOB_REPORT_WIRE_BYTES {
            return Err(RealtimeJobReportWireError::WireLength);
        }
        if encoded[0..8] != REALTIME_JOB_REPORT_MAGIC {
            return Err(RealtimeJobReportWireError::Magic);
        }
        if read_u16(encoded, 8) != REALTIME_JOB_REPORT_VERSION {
            return Err(RealtimeJobReportWireError::Version);
        }
        let flags = encoded[11];
        if flags & !REPORT_KNOWN_FLAGS != 0
            || encoded[12..16].iter().any(|byte| *byte != 0)
            || encoded[120..128].iter().any(|byte| *byte != 0)
        {
            return Err(RealtimeJobReportWireError::Reserved);
        }
        let state =
            RealtimeJobState::from_wire(encoded[10]).ok_or(RealtimeJobReportWireError::State)?;
        let mut admitted_digest = [0_u8; 32];
        admitted_digest.copy_from_slice(&encoded[48..80]);
        let mut completed_digest = [0_u8; 32];
        completed_digest.copy_from_slice(&encoded[88..120]);
        let admitted_progress = if flags & REPORT_FLAG_ADMITTED_PROGRESS != 0 {
            Some((StreamTick(read_u64(encoded, 40)), Digest(admitted_digest)))
        } else {
            if encoded[40..80].iter().any(|byte| *byte != 0) {
                return Err(RealtimeJobReportWireError::Reserved);
            }
            None
        };
        let completed_progress = if flags & REPORT_FLAG_COMPLETED_PROGRESS != 0 {
            Some((StreamTick(read_u64(encoded, 80)), Digest(completed_digest)))
        } else {
            if encoded[80..120].iter().any(|byte| *byte != 0) {
                return Err(RealtimeJobReportWireError::Reserved);
            }
            None
        };
        let report = Self {
            prepare_id: read_u64(encoded, 16),
            state,
            admitted_blocks: read_u32(encoded, 24),
            completed_blocks: read_u32(encoded, 28),
            total_blocks: read_u32(encoded, 32),
            queue_depth: read_u32(encoded, 36),
            admitted_progress,
            completed_progress,
            outstanding: flags & REPORT_FLAG_OUTSTANDING != 0,
        };
        report.validate()?;
        if report.encode()? != encoded {
            return Err(RealtimeJobReportWireError::Noncanonical);
        }
        Ok(report)
    }

    fn validate(self) -> Result<(), RealtimeJobReportWireError> {
        if self.prepare_id == 0 {
            return Err(RealtimeJobReportWireError::PrepareId);
        }
        if self.total_blocks == 0
            || self.completed_blocks > self.admitted_blocks
            || self.admitted_blocks > self.total_blocks
        {
            return Err(RealtimeJobReportWireError::Counts);
        }
        if self.outstanding != (self.state == RealtimeJobState::Admitted)
            || self.admitted_progress.is_some() != self.outstanding
            || self.completed_progress.is_some() != (self.completed_blocks != 0)
        {
            return Err(RealtimeJobReportWireError::Progress);
        }
        if let Some((_, digest)) = self.admitted_progress
            && digest.is_zero()
        {
            return Err(RealtimeJobReportWireError::Progress);
        }
        if let Some((_, digest)) = self.completed_progress
            && digest.is_zero()
        {
            return Err(RealtimeJobReportWireError::Progress);
        }
        match self.state {
            RealtimeJobState::Prepared
                if self.admitted_blocks != self.completed_blocks
                    || self.admitted_progress.is_some() =>
            {
                Err(RealtimeJobReportWireError::Progress)
            }
            RealtimeJobState::Admitted => {
                let in_flight = self.admitted_blocks.saturating_sub(self.completed_blocks);
                let window = u32::try_from(REALTIME_BLOCK_WINDOW)
                    .map_err(|_| RealtimeJobReportWireError::Counts)?;
                if in_flight == 0 || in_flight > window {
                    Err(RealtimeJobReportWireError::Progress)
                } else {
                    Ok(())
                }
            }
            RealtimeJobState::Complete
                if self.completed_blocks != self.total_blocks || self.outstanding =>
            {
                Err(RealtimeJobReportWireError::Progress)
            }
            _ => Ok(()),
        }
    }
}

/// Fixed real-time report decoding or consistency failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealtimeJobReportWireError {
    /// Payload length was not exactly [`REALTIME_JOB_REPORT_WIRE_BYTES`].
    WireLength,
    /// Report magic did not match.
    Magic,
    /// Report version was unsupported.
    Version,
    /// State byte was unknown.
    State,
    /// Flags, absent fields, or reserved bytes were nonzero.
    Reserved,
    /// Prepare correlation was zero.
    PrepareId,
    /// Block counts were empty or out of order.
    Counts,
    /// State, ownership, and progress facts disagreed.
    Progress,
    /// Queue depth did not fit its wire field.
    QueueDepth,
    /// Valid fields used a noncanonical representation.
    Noncanonical,
}

/// One combined service/realtime snapshot returned by `JobStatus`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JobStatusReport {
    /// Current core-0 actor facts, absent when no job is installed.
    pub service: Option<ServiceJobReport>,
    /// Latest core-1 facts, absent until a correlated report arrives.
    pub realtime: Option<RealtimeJobReport>,
    /// Latest boot-bound core-1 scheduling authority for this preparation.
    pub schedule: Option<JobScheduleReport>,
}

impl JobStatusReport {
    /// Encodes one fixed status body with strict zero-filled absent sections.
    pub fn encode(self) -> Result<[u8; JOB_STATUS_WIRE_BYTES], JobStatusReportWireError> {
        self.validate()?;
        let mut encoded = [0_u8; JOB_STATUS_WIRE_BYTES];
        encoded[0..8].copy_from_slice(&JOB_STATUS_MAGIC);
        encoded[8..10].copy_from_slice(&JOB_STATUS_VERSION.to_le_bytes());
        let mut flags = 0_u8;
        if let Some(service) = self.service {
            flags |= JOB_STATUS_FLAG_SERVICE;
            encoded[16..112].copy_from_slice(
                &service
                    .encode()
                    .map_err(JobStatusReportWireError::Service)?,
            );
        }
        if let Some(realtime) = self.realtime {
            flags |= JOB_STATUS_FLAG_REALTIME;
            encoded[112..240].copy_from_slice(
                &realtime
                    .encode()
                    .map_err(JobStatusReportWireError::Realtime)?,
            );
        }
        if let Some(schedule) = self.schedule {
            flags |= JOB_STATUS_FLAG_SCHEDULE;
            encoded[240..368].copy_from_slice(
                &schedule
                    .encode()
                    .map_err(JobStatusReportWireError::Schedule)?,
            );
        }
        encoded[10] = flags;
        // Bytes 11..16 remain reserved zero.
        Ok(encoded)
    }

    /// Decodes one exact combined status body.
    pub fn decode(encoded: &[u8]) -> Result<Self, JobStatusReportWireError> {
        if encoded.len() != JOB_STATUS_WIRE_BYTES {
            return Err(JobStatusReportWireError::WireLength);
        }
        if encoded[0..8] != JOB_STATUS_MAGIC {
            return Err(JobStatusReportWireError::Magic);
        }
        if read_u16(encoded, 8) != JOB_STATUS_VERSION {
            return Err(JobStatusReportWireError::Version);
        }
        let flags = encoded[10];
        if flags & !JOB_STATUS_KNOWN_FLAGS != 0 || encoded[11..16].iter().any(|byte| *byte != 0) {
            return Err(JobStatusReportWireError::Reserved);
        }
        let service = if flags & JOB_STATUS_FLAG_SERVICE != 0 {
            Some(
                ServiceJobReport::decode(&encoded[16..112])
                    .map_err(JobStatusReportWireError::Service)?,
            )
        } else {
            if encoded[16..112].iter().any(|byte| *byte != 0) {
                return Err(JobStatusReportWireError::Reserved);
            }
            None
        };
        let realtime = if flags & JOB_STATUS_FLAG_REALTIME != 0 {
            Some(
                RealtimeJobReport::decode(&encoded[112..240])
                    .map_err(JobStatusReportWireError::Realtime)?,
            )
        } else {
            if encoded[112..240].iter().any(|byte| *byte != 0) {
                return Err(JobStatusReportWireError::Reserved);
            }
            None
        };
        let schedule = if flags & JOB_STATUS_FLAG_SCHEDULE != 0 {
            Some(
                JobScheduleReport::decode(&encoded[240..368])
                    .map_err(JobStatusReportWireError::Schedule)?,
            )
        } else {
            if encoded[240..368].iter().any(|byte| *byte != 0) {
                return Err(JobStatusReportWireError::Reserved);
            }
            None
        };
        let report = Self {
            service,
            realtime,
            schedule,
        };
        report.validate()?;
        if report.encode()? != encoded {
            return Err(JobStatusReportWireError::Noncanonical);
        }
        Ok(report)
    }

    fn validate(self) -> Result<(), JobStatusReportWireError> {
        if self.schedule.is_some() && (self.service.is_none() || self.realtime.is_none()) {
            return Err(JobStatusReportWireError::Correlation);
        }
        if let (Some(service), Some(realtime)) = (self.service, self.realtime)
            && (service.prepare_id != realtime.prepare_id
                || service.total_blocks != realtime.total_blocks)
        {
            return Err(JobStatusReportWireError::Correlation);
        }
        if let (Some(service), Some(realtime)) = (self.service, self.realtime)
            && service.state == ServiceJobState::Complete
            && realtime.state == RealtimeJobState::Complete
            && service.final_progress != realtime.completed_progress
        {
            return Err(JobStatusReportWireError::TerminalDivergence);
        }
        Ok(())
    }
}

/// Combined `JobStatus` body rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobStatusReportWireError {
    /// Body length was not exact.
    WireLength,
    /// Status magic did not match.
    Magic,
    /// Status version was unsupported.
    Version,
    /// Flags, absent sections, or reserved bytes were nonzero.
    Reserved,
    /// Embedded core-0 report failed validation.
    Service(ServiceJobReportWireError),
    /// Embedded core-1 report failed validation.
    Realtime(RealtimeJobReportWireError),
    /// Embedded boot-bound schedule report failed validation.
    Schedule(JobScheduleWireError),
    /// Core reports named different preparations or partition lengths.
    Correlation,
    /// Independently validated terminal tick/digest facts diverged.
    TerminalDivergence,
    /// Valid fields used a noncanonical representation.
    Noncanonical,
}

/// Cached-job lifecycle failure retaining a concrete media-device error.
#[derive(Debug)]
pub enum JobError<E> {
    /// Descriptor was invalid before state mutation.
    Descriptor(DescriptorError),
    /// Published media lookup or readback failed.
    Storage(ProvisionedCacheError<E>),
    /// Canonical block, stream, or fixed-buffer invariant failed.
    Machine(BlockError),
    /// Operation was invalid for the current terminal lifecycle.
    State,
    /// An admitted block did not match the outstanding private token.
    AdmissionToken,
}

const fn read_u16(encoded: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([encoded[offset], encoded[offset + 1]])
}

const fn read_u32(encoded: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        encoded[offset],
        encoded[offset + 1],
        encoded[offset + 2],
        encoded[offset + 3],
    ])
}

const fn read_u64(encoded: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        encoded[offset],
        encoded[offset + 1],
        encoded[offset + 2],
        encoded[offset + 3],
        encoded[offset + 4],
        encoded[offset + 5],
        encoded[offset + 6],
        encoded[offset + 7],
    ])
}

fn read_i64(encoded: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes(
        encoded[offset..offset + 8]
            .try_into()
            .expect("fixed descriptor field"),
    )
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::boxed::Box;

    use alumina_clock::BootId;
    use alumina_machine_ir::{
        ExecutionSegment, FINITE_DIFFERENCE_ONE_STEP, FiniteDifferenceAxis,
        FiniteDifferenceSegment, ValidationLimits,
    };
    use alumina_protocol::{DeviceCycle, Digest};
    use alumina_runtime::IntercoreBoundary;
    use alumina_storage::{ContentId, DigestAlgorithm, StoredObject};

    use super::*;

    fn descriptor(blocks: u32) -> JobDescriptor {
        JobDescriptor {
            prepare_id: 7,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId {
                        algorithm: DigestAlgorithm::Sha256,
                        digest: Digest([0xaa; 32]),
                    },
                    byte_len: u64::from(blocks) * 512,
                },
                manifest: ContentId {
                    algorithm: DigestAlgorithm::Sha256,
                    digest: Digest([0xbb; 32]),
                },
            },
            stream_id: StreamId([0x11; 16]),
            capability_digest: Digest([0x22; 32]),
            config_digest: Digest([0x33; 32]),
            axis_count: 3,
            execution_kind: ExecutionKind::Motion,
            maximum_dense_updates: 0,
            dense_update_period_ticks: 0,
            block_count: blocks,
            first_tick: StreamTick(0),
            initial_position: [10, -20, 30, 0, 0, 0, 0, 0],
            limits: BlockValidationLimits {
                maximum_block_ticks: 1_000,
                segment: ValidationLimits {
                    maximum_segment_ticks: 1_000,
                    maximum_steps_per_segment: 100,
                },
            },
        }
    }

    fn block(sequence: u32, previous: Digest) -> ExecutionBlock {
        ExecutionBlock::encode_motion(
            StreamId([0x11; 16]),
            Digest([0x22; 32]),
            Digest([0x33; 32]),
            sequence,
            previous,
            &[ExecutionSegment {
                start_tick: StreamTick(u64::from(sequence) * 100),
                end_tick: StreamTick((u64::from(sequence) + 1) * 100),
                delta_steps: [1, -1, 0],
                flags: 0,
            }],
        )
        .unwrap()
    }

    fn finite_descriptor(blocks: u32) -> JobDescriptor {
        JobDescriptor {
            execution_kind: ExecutionKind::FiniteDifference,
            maximum_dense_updates: 1_000,
            dense_update_period_ticks: 1,
            ..descriptor(blocks)
        }
    }

    fn finite_block(sequence: u32, previous: Digest, initial_position: [i64; 3]) -> ExecutionBlock {
        ExecutionBlock::encode_finite_difference(
            StreamId([0x11; 16]),
            Digest([0x22; 32]),
            Digest([0x33; 32]),
            sequence,
            previous,
            &[FiniteDifferenceSegment::<3> {
                start_tick: StreamTick(u64::from(sequence) * 16),
                end_tick: StreamTick((u64::from(sequence) + 1) * 16),
                update_period_ticks: 1,
                update_count: 16,
                axes: core::array::from_fn(|axis| FiniteDifferenceAxis {
                    initial_position: initial_position[axis],
                    first_difference: if axis == 0 {
                        (FINITE_DIFFERENCE_ONE_STEP - 1) / 4
                    } else {
                        0
                    },
                    second_difference: 0,
                    third_difference: 0,
                }),
                flags: 0,
            }],
        )
        .unwrap()
    }

    fn commit_request() -> JobCommitRequest {
        let descriptor = descriptor(2);
        let boot_id = BootId::new([0x66; 16]).unwrap();
        JobCommitRequest {
            policy: JobNetworkPolicy::NetworkAttended,
            prepare_id: descriptor.prepare_id,
            boot_id,
            global_job_digest: Digest([0x77; 32]),
            participant_set_digest: Digest([0x88; 32]),
            prepared_token: PreparedJobToken::derive::<3>(boot_id, descriptor).unwrap(),
            partition_digest: descriptor.partition.object.content.digest,
            local_start_cycle: DeviceCycle(10_000),
            confirm_deadline_cycle: DeviceCycle(8_000),
            abort_guard_cycle: DeviceCycle(9_000),
            lease_expiry_cycle: DeviceCycle(20_000),
            clock_probe_id: 9,
            clock_uncertainty_cycles: 5,
            required_sync_tolerance_cycles: 10,
            commit_id: JobCommitId::new([0x99; JOB_COMMIT_ID_BYTES]).unwrap(),
        }
    }

    #[test]
    fn descriptor_rejects_kind_axis_count_layout_and_nonzero_first_tick() {
        assert_eq!(descriptor(2).validate::<3>(), Ok(()));
        let mut invalid = descriptor(2);
        invalid.partition.object.kind = ObjectKind::OpaqueData;
        assert_eq!(invalid.validate::<3>(), Err(DescriptorError::ObjectKind));
        invalid = descriptor(2);
        invalid.axis_count = 4;
        assert_eq!(
            invalid.validate::<3>(),
            Err(DescriptorError::AxisCount {
                encoded: 4,
                expected: 3
            })
        );
        invalid = descriptor(2);
        invalid.partition.object.byte_len = 513;
        assert_eq!(
            invalid.validate::<3>(),
            Err(DescriptorError::PartitionLayout)
        );
        invalid = descriptor(2);
        invalid.first_tick = StreamTick(1);
        assert_eq!(invalid.validate::<3>(), Err(DescriptorError::FirstTick));
        invalid = descriptor(2);
        invalid.initial_position[3] = 1;
        assert_eq!(
            invalid.validate::<3>(),
            Err(DescriptorError::InitialPosition)
        );
        invalid = descriptor(2);
        invalid.maximum_dense_updates = 1;
        assert_eq!(invalid.validate::<3>(), Err(DescriptorError::Limits));
        invalid = finite_descriptor(2);
        invalid.maximum_dense_updates = 0;
        assert_eq!(invalid.validate::<3>(), Err(DescriptorError::Limits));
        assert_eq!(descriptor(2).initial_position_for::<3>(), Ok([10, -20, 30]));
    }

    #[test]
    fn descriptor_and_core_command_have_one_canonical_wire_image() {
        let descriptor = descriptor(2);
        let boot_id = BootId::new([0x66; BOOT_ID_BYTES]).unwrap();
        let encoded = descriptor.encode::<3>().unwrap();
        assert_eq!(encoded.len(), JOB_DESCRIPTOR_WIRE_BYTES);
        assert_eq!(&encoded[0..8], b"ALMJOBD4");
        assert_eq!(encoded[10], ExecutionKind::Motion as u8);
        assert_eq!(&encoded[12..16], &0_u32.to_le_bytes());
        assert_eq!(&encoded[16..20], &0_u32.to_le_bytes());
        assert_eq!(&encoded[224..232], &10_i64.to_le_bytes());
        assert_eq!(&encoded[232..240], &(-20_i64).to_le_bytes());
        assert_eq!(&encoded[240..248], &30_i64.to_le_bytes());
        assert!(encoded[248..288].iter().all(|byte| *byte == 0));
        assert_eq!(JobDescriptor::decode::<3>(&encoded), Ok(descriptor));
        assert_eq!(
            CoreJobCommand::decode::<3>(
                &CoreJobCommand::Prepare {
                    boot_id,
                    descriptor,
                }
                .encode::<3>()
                .unwrap()
            ),
            Ok(CoreJobCommand::Prepare {
                boot_id,
                descriptor,
            })
        );
        assert_eq!(
            CoreJobCommand::decode::<3>(
                &CoreJobCommand::Cancel { prepare_id: 7 }
                    .encode::<3>()
                    .unwrap()
            ),
            Ok(CoreJobCommand::Cancel { prepare_id: 7 })
        );
        let commit = commit_request();
        assert_eq!(
            CoreJobCommand::decode::<3>(&CoreJobCommand::Commit(commit).encode::<3>().unwrap()),
            Ok(CoreJobCommand::Commit(commit))
        );
        let confirm =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit).unwrap();
        assert_eq!(
            CoreJobCommand::decode::<3>(&CoreJobCommand::Confirm(confirm).encode::<3>().unwrap()),
            Ok(CoreJobCommand::Confirm(confirm))
        );
        let renewal = JobLeaseRenewRequest::for_commit(commit, DeviceCycle(30_000)).unwrap();
        assert_eq!(
            CoreJobCommand::decode::<3>(
                &CoreJobCommand::RenewLease(renewal).encode::<3>().unwrap()
            ),
            Ok(CoreJobCommand::RenewLease(renewal))
        );
        let abort =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Abort, commit).unwrap();
        assert_eq!(
            CoreJobCommand::decode::<3>(&CoreJobCommand::Abort(abort).encode::<3>().unwrap()),
            Ok(CoreJobCommand::Abort(abort))
        );
        let cancel = JobCancelRequest { prepare_id: 7 };
        assert_eq!(
            JobCancelRequest::decode(&cancel.encode().unwrap()),
            Ok(cancel)
        );

        let mut corrupt = encoded;
        corrupt[100] ^= 1;
        assert_eq!(
            JobDescriptor::decode::<3>(&corrupt),
            Err(JobDescriptorWireError::Integrity)
        );
        let direct = finite_descriptor(2);
        let direct_encoded = direct.encode::<3>().unwrap();
        assert_eq!(direct_encoded[10], ExecutionKind::FiniteDifference as u8);
        assert_eq!(&direct_encoded[12..16], &1_000_u32.to_le_bytes());
        assert_eq!(&direct_encoded[16..20], &1_u32.to_le_bytes());
        assert_eq!(JobDescriptor::decode::<3>(&direct_encoded), Ok(direct));
        let mut retired = direct_encoded;
        retired[0..8].copy_from_slice(b"ALMJOBD3");
        assert_eq!(
            JobDescriptor::decode::<3>(&retired),
            Err(JobDescriptorWireError::Magic)
        );
        let mut nonzero_reserved = CoreJobCommand::Cancel { prepare_id: 7 }
            .encode::<3>()
            .unwrap();
        nonzero_reserved[200] = 1;
        assert_eq!(
            CoreJobCommand::decode::<3>(&nonzero_reserved),
            Err(CoreJobCommandWireError::Reserved)
        );
    }

    #[test]
    fn combined_status_is_fixed_canonical_and_correlated() {
        let progress = MachineStreamProgress {
            accepted_blocks: 2,
            expected_blocks: 2,
            end_tick: StreamTick(200),
            position: [2, -2, 0],
            block_digest: Digest([0x44; 32]),
            complete: true,
        };
        let service = ServiceJobReport::from_status(
            ServiceJobStatus {
                prepare_id: 7,
                state: ServiceJobState::Complete,
                validated_blocks: 2,
                sent_blocks: 2,
                total_blocks: 2,
                storage_chunks_read: 1,
                final_progress: Some(progress),
            },
            8,
            0,
        )
        .unwrap();
        let realtime = RealtimeJobReport {
            prepare_id: 7,
            state: RealtimeJobState::Complete,
            admitted_blocks: 2,
            completed_blocks: 2,
            total_blocks: 2,
            queue_depth: 0,
            admitted_progress: None,
            completed_progress: Some((StreamTick(200), Digest([0x44; 32]))),
            outstanding: false,
        };
        let status = JobStatusReport {
            service: Some(service),
            realtime: Some(realtime),
            schedule: Some(
                PreparedJobSchedule::prepare::<3>(BootId::new([0x66; 16]).unwrap(), descriptor(2))
                    .unwrap()
                    .report(),
            ),
        };
        let encoded = status.encode().unwrap();
        assert_eq!(encoded.len(), JOB_STATUS_WIRE_BYTES);
        assert_eq!(&encoded[..8], b"ALMJST03");
        assert_eq!(&encoded[8..10], &3_u16.to_le_bytes());
        assert_eq!(JobStatusReport::decode(&encoded), Ok(status));
        assert_eq!(
            JobStatusReport::decode(&JobStatusReport::default().encode().unwrap()),
            Ok(JobStatusReport::default())
        );

        let wrong = JobStatusReport {
            realtime: Some(RealtimeJobReport {
                prepare_id: 8,
                ..realtime
            }),
            ..status
        };
        assert_eq!(wrong.encode(), Err(JobStatusReportWireError::Correlation));

        let divergent = JobStatusReport {
            realtime: Some(RealtimeJobReport {
                completed_progress: Some((StreamTick(200), Digest([0x55; 32]))),
                ..realtime
            }),
            ..status
        };
        assert_eq!(
            divergent.encode(),
            Err(JobStatusReportWireError::TerminalDivergence)
        );
    }

    #[test]
    fn realtime_admission_pipelines_two_blocks_and_rejects_wrong_order() {
        type Boundary = IntercoreBoundary<1, 1, 4, 4, 2>;
        let boundary = Box::leak(Box::new(Boundary::new()));
        let (mut service, mut realtime) = boundary.split();
        let first = block(0, Digest::ZERO);
        let first_digest = first.header().block_digest;
        service.try_send_work(first).unwrap();
        service.try_send_work(block(1, first_digest)).unwrap();

        let mut job = RealtimeJob::<3>::prepare(descriptor(2)).unwrap();
        let admitted = match job.poll(&mut realtime).unwrap() {
            RealtimePoll::Block(block) => block,
            _ => panic!("first block must be available"),
        };
        assert_eq!(admitted.header().sequence, 0);
        assert_eq!(admitted.segments().unwrap().len(), 1);
        let report = RealtimeJobReport::from_status(job.status(), realtime.work_depth()).unwrap();
        assert_eq!(report.state, RealtimeJobState::Admitted);
        assert_eq!(report.admitted_progress.unwrap().0, StreamTick(100));
        assert_eq!(
            RealtimeJobReport::decode(&report.encode().unwrap()),
            Ok(report)
        );
        let second = match job.poll(&mut realtime).unwrap() {
            RealtimePoll::Block(block) => block,
            _ => panic!("second block must enter the bounded lookahead window"),
        };
        assert_eq!(second.header().sequence, 1);
        assert!(matches!(
            job.poll(&mut realtime).unwrap(),
            RealtimePoll::Outstanding
        ));
        let report = RealtimeJobReport::from_status(job.status(), realtime.work_depth()).unwrap();
        assert_eq!(report.admitted_blocks, 2);
        assert_eq!(report.completed_blocks, 0);
        assert_eq!(
            RealtimeJobReport::decode(&report.encode().unwrap()),
            Ok(report)
        );
        let status = job.acknowledge(admitted).unwrap();
        assert_eq!(status.completed_blocks, 1);
        assert_eq!(status.state, RealtimeJobState::Admitted);
        assert_eq!(status.admitted_progress.unwrap().accepted_blocks, 2);

        let status = job.acknowledge(second).unwrap();
        assert_eq!(status.state, RealtimeJobState::Complete);
        assert_eq!(status.completed_blocks, 2);
        assert_eq!(status.completed_progress.unwrap().position, [2, -2, 0]);
        let report = RealtimeJobReport::from_status(status, realtime.work_depth()).unwrap();
        assert_eq!(report.state, RealtimeJobState::Complete);
        assert_eq!(report.completed_progress.unwrap().0, StreamTick(200));
        assert_eq!(
            RealtimeJobReport::decode(&report.encode().unwrap()),
            Ok(report)
        );

        let boundary = Box::leak(Box::new(Boundary::new()));
        let (mut service, mut realtime) = boundary.split();
        let first = block(0, Digest::ZERO);
        let first_digest = first.header().block_digest;
        service.try_send_work(first).unwrap();
        service.try_send_work(block(1, first_digest)).unwrap();
        let mut job = RealtimeJob::<3>::prepare(descriptor(2)).unwrap();
        let _first = match job.poll(&mut realtime).unwrap() {
            RealtimePoll::Block(block) => block,
            _ => panic!("first block must be available"),
        };
        let second = match job.poll(&mut realtime).unwrap() {
            RealtimePoll::Block(block) => block,
            _ => panic!("second block must be available"),
        };
        assert!(matches!(
            job.acknowledge(second),
            Err(JobError::AdmissionToken)
        ));
        assert_eq!(job.status().state, RealtimeJobState::Faulted);
        assert!(!job.status().outstanding);
    }

    #[test]
    fn realtime_job_admits_only_the_declared_finite_difference_family() {
        type Boundary = IntercoreBoundary<1, 1, 4, 4, 2>;
        let boundary = Box::leak(Box::new(Boundary::new()));
        let (mut service, mut realtime) = boundary.split();
        let direct_block = finite_block(0, Digest::ZERO, [0; 3]);
        let digest = direct_block.header().block_digest;
        service.try_send_work(direct_block).unwrap();
        let mut job = RealtimeJob::<3>::prepare(finite_descriptor(1)).unwrap();
        let admitted = match job.poll(&mut realtime).unwrap() {
            RealtimePoll::Block(admitted) => admitted,
            RealtimePoll::Empty | RealtimePoll::Outstanding => {
                panic!("direct block must be admitted")
            }
        };
        assert_eq!(admitted.header().kind, ExecutionKind::FiniteDifference);
        assert_eq!(admitted.finite_difference_segments().unwrap().len(), 1);
        assert!(matches!(
            admitted.segments(),
            Err(BlockError::Kind { received: 2 })
        ));
        let progress = admitted.progress();
        assert_eq!(progress.accepted_blocks, 1);
        assert_eq!(progress.end_tick, StreamTick(16));
        assert_eq!(progress.position, [4, 0, 0]);
        assert_eq!(progress.block_digest, digest);
        assert!(progress.complete);
        let status = job.acknowledge(admitted).unwrap();
        assert_eq!(status.state, RealtimeJobState::Complete);
        assert_eq!(status.completed_progress, Some(progress));

        let boundary = Box::leak(Box::new(Boundary::new()));
        let (mut service, mut realtime) = boundary.split();
        service.try_send_work(block(0, Digest::ZERO)).unwrap();
        let mut job = RealtimeJob::<3>::prepare(finite_descriptor(1)).unwrap();
        assert!(matches!(
            job.poll(&mut realtime),
            Err(JobError::Machine(BlockError::Kind { received: 1 }))
        ));
        let status = job.status();
        assert_eq!(status.state, RealtimeJobState::Faulted);
        assert_eq!(status.admitted_blocks, 0);
        assert_eq!(status.completed_blocks, 0);
        assert!(status.admitted_progress.is_none());
        assert!(!status.outstanding);
    }

    #[test]
    fn wrong_block_faults_and_cancelled_job_drains_ring() {
        type Boundary = IntercoreBoundary<1, 1, 4, 4, 2>;
        let boundary = Box::leak(Box::new(Boundary::new()));
        let (mut service, mut realtime) = boundary.split();
        let wrong = ExecutionBlock::encode_motion(
            StreamId([0x11; 16]),
            Digest([0x22; 32]),
            Digest([0x44; 32]),
            0,
            Digest::ZERO,
            &[ExecutionSegment {
                start_tick: StreamTick(0),
                end_tick: StreamTick(100),
                delta_steps: [0; 3],
                flags: 0,
            }],
        )
        .unwrap();
        service.try_send_work(wrong).unwrap();
        let mut job = RealtimeJob::<3>::prepare(descriptor(1)).unwrap();
        assert!(matches!(
            job.poll(&mut realtime),
            Err(JobError::Machine(BlockError::ConfigurationIdentity))
        ));
        assert_eq!(job.status().state, RealtimeJobState::Faulted);

        service.try_send_work(block(0, Digest::ZERO)).unwrap();
        assert_eq!(job.drain(&mut realtime).unwrap(), 1);
        assert_eq!(realtime.work_depth(), 0);

        let boundary = Box::leak(Box::new(Boundary::new()));
        let (mut service, mut realtime) = boundary.split();
        service.try_send_work(block(0, Digest::ZERO)).unwrap();
        service.try_send_work(block(1, Digest([0x99; 32]))).unwrap();
        let mut job = RealtimeJob::<3>::prepare(descriptor(2)).unwrap();
        let first = match job.poll(&mut realtime).unwrap() {
            RealtimePoll::Block(block) => block,
            _ => panic!("the valid prefix must be admitted"),
        };
        assert!(matches!(
            job.poll(&mut realtime),
            Err(JobError::Machine(BlockError::PreviousDigest))
        ));
        assert_eq!(job.status().state, RealtimeJobState::Faulted);
        assert!(!job.status().outstanding);
        assert!(job.status().admitted_progress.is_none());
        assert!(matches!(job.acknowledge(first), Err(JobError::State)));
    }

    #[test]
    fn local_hardware_fault_invalidates_an_admitted_token_before_drain() {
        type Boundary = IntercoreBoundary<1, 1, 4, 4, 2>;
        let boundary = Box::leak(Box::new(Boundary::new()));
        let (mut service, mut realtime) = boundary.split();
        service.try_send_work(block(0, Digest::ZERO)).unwrap();
        let mut job = RealtimeJob::<3>::prepare(descriptor(1)).unwrap();
        let admitted = match job.poll(&mut realtime).unwrap() {
            RealtimePoll::Block(block) => block,
            _ => panic!("the first block must be admitted"),
        };
        job.fault();
        assert_eq!(job.status().state, RealtimeJobState::Faulted);
        assert!(!job.status().outstanding);
        assert!(matches!(job.acknowledge(admitted), Err(JobError::State)));
        assert_eq!(job.drain(&mut realtime).unwrap(), 0);
    }

    #[test]
    fn stream_ticks_require_checked_commit_epoch_mapping() {
        assert_eq!(
            StreamTick(250).at_epoch(DeviceCycle(10_000)),
            Ok(DeviceCycle(10_250))
        );
    }
}
