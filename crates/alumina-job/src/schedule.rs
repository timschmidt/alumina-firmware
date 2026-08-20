//! Boot-bound cached-job install, confirmation, abort, and local start authority.

use alumina_clock::{BOOT_ID_BYTES, BootId};
use alumina_protocol::{DeviceCycle, Digest};
use alumina_storage::sha256;

use crate::{JOB_DESCRIPTOR_WIRE_BYTES, JobDescriptor, JobDescriptorWireError};

/// Exact authenticated `JobCommit` request body.
pub const JOB_COMMIT_WIRE_BYTES: usize = 240;
/// Exact authenticated `JobConfirm`/`JobAbort` reference body.
pub const JOB_SCHEDULE_REFERENCE_WIRE_BYTES: usize = 88;
/// Exact authenticated attended-lease renewal body.
pub const JOB_LEASE_RENEW_WIRE_BYTES: usize = 96;
/// Exact schedule section appended to combined job status.
pub const JOB_SCHEDULE_REPORT_WIRE_BYTES: usize = 128;
/// Nonzero UI-selected commit identity bytes.
pub const JOB_COMMIT_ID_BYTES: usize = 16;

const COMMIT_MAGIC: [u8; 8] = *b"ALMJCOM2";
const REFERENCE_MAGIC: [u8; 8] = *b"ALMJREF2";
const LEASE_RENEW_MAGIC: [u8; 8] = *b"ALMJREN1";
const REPORT_MAGIC: [u8; 8] = *b"ALMJSCH5";
const COMMIT_VERSION: u16 = 2;
const REFERENCE_VERSION: u16 = 2;
const LEASE_RENEW_VERSION: u16 = 1;
const REPORT_VERSION: u16 = 5;
const PREPARED_TOKEN_DOMAIN: [u8; 16] = *b"ALM-PREPARED-V2\0";
const REFERENCE_CONFIRM: u8 = 1;
const REFERENCE_ABORT: u8 = 2;
const REPORT_FLAG_COMMIT: u16 = 1 << 0;
const REPORT_FLAG_ATTENDED: u16 = 1 << 1;
const REPORT_FLAG_AUTONOMOUS: u16 = 1 << 2;
const REPORT_FLAG_START_EMITTED: u16 = 1 << 3;
const REPORT_KNOWN_FLAGS: u16 =
    REPORT_FLAG_COMMIT | REPORT_FLAG_ATTENDED | REPORT_FLAG_AUTONOMOUS | REPORT_FLAG_START_EMITTED;

/// Browser-selected unique identity for one installed local schedule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobCommitId([u8; JOB_COMMIT_ID_BYTES]);

impl JobCommitId {
    /// Rejects the all-zero absent sentinel.
    pub const fn new(bytes: [u8; JOB_COMMIT_ID_BYTES]) -> Result<Self, JobScheduleWireError> {
        if bytes_nonzero(&bytes) {
            Ok(Self(bytes))
        } else {
            Err(JobScheduleWireError::CommitId)
        }
    }

    /// Exact canonical bytes.
    pub const fn as_bytes(self) -> [u8; JOB_COMMIT_ID_BYTES] {
        self.0
    }
}

/// SHA-256 binding a preparation to this boot and complete local descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedJobToken(pub Digest);

impl PreparedJobToken {
    /// Derives the token without secret material. Authentication and `BootId`
    /// provide replay protection; this digest provides exact state correlation.
    pub fn derive<const AXES: usize>(
        boot_id: BootId,
        descriptor: JobDescriptor,
    ) -> Result<Self, JobScheduleWireError> {
        let descriptor = descriptor
            .encode::<AXES>()
            .map_err(JobScheduleWireError::Descriptor)?;
        let mut transcript =
            [0_u8; PREPARED_TOKEN_DOMAIN.len() + BOOT_ID_BYTES + JOB_DESCRIPTOR_WIRE_BYTES];
        transcript[..PREPARED_TOKEN_DOMAIN.len()].copy_from_slice(&PREPARED_TOKEN_DOMAIN);
        let boot_start = PREPARED_TOKEN_DOMAIN.len();
        transcript[boot_start..boot_start + BOOT_ID_BYTES].copy_from_slice(&boot_id.as_bytes());
        transcript[boot_start + BOOT_ID_BYTES..].copy_from_slice(&descriptor);
        Ok(Self(sha256(&transcript).digest))
    }

    fn validate(self) -> Result<(), JobScheduleWireError> {
        if self.0.is_zero() {
            Err(JobScheduleWireError::PreparedToken)
        } else {
            Ok(())
        }
    }
}

/// Network-loss policy compiled into a local job commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum JobNetworkPolicy {
    /// Local execution requires an unexpired browser communication lease.
    NetworkAttended = 1,
    /// Complete cached work may finish without the browser under local policy.
    CachedAutonomous = 2,
}

impl JobNetworkPolicy {
    pub(crate) const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::NetworkAttended),
            2 => Some(Self::CachedAutonomous),
            _ => None,
        }
    }
}

/// Complete participant-set-bound local schedule installed before confirmation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobCommitRequest {
    pub policy: JobNetworkPolicy,
    pub prepare_id: u64,
    pub boot_id: BootId,
    pub global_job_digest: Digest,
    pub participant_set_digest: Digest,
    pub prepared_token: PreparedJobToken,
    pub partition_digest: Digest,
    pub local_start_cycle: DeviceCycle,
    pub confirm_deadline_cycle: DeviceCycle,
    pub abort_guard_cycle: DeviceCycle,
    pub lease_expiry_cycle: DeviceCycle,
    pub clock_probe_id: u64,
    pub clock_uncertainty_cycles: u64,
    pub required_sync_tolerance_cycles: u64,
    pub commit_id: JobCommitId,
}

impl JobCommitRequest {
    /// Encodes one canonical fixed commit body.
    pub fn encode(self) -> Result<[u8; JOB_COMMIT_WIRE_BYTES], JobScheduleWireError> {
        self.validate()?;
        let mut encoded = [0_u8; JOB_COMMIT_WIRE_BYTES];
        encoded[0..8].copy_from_slice(&COMMIT_MAGIC);
        encoded[8..10].copy_from_slice(&COMMIT_VERSION.to_le_bytes());
        encoded[10] = self.policy as u8;
        // Bytes 11..16 are reserved zero.
        encoded[16..24].copy_from_slice(&self.prepare_id.to_le_bytes());
        encoded[24..40].copy_from_slice(&self.boot_id.as_bytes());
        encoded[40..72].copy_from_slice(&self.global_job_digest.0);
        encoded[72..104].copy_from_slice(&self.participant_set_digest.0);
        encoded[104..136].copy_from_slice(&self.prepared_token.0.0);
        encoded[136..168].copy_from_slice(&self.partition_digest.0);
        encoded[168..176].copy_from_slice(&self.local_start_cycle.0.to_le_bytes());
        encoded[176..184].copy_from_slice(&self.confirm_deadline_cycle.0.to_le_bytes());
        encoded[184..192].copy_from_slice(&self.abort_guard_cycle.0.to_le_bytes());
        encoded[192..200].copy_from_slice(&self.lease_expiry_cycle.0.to_le_bytes());
        encoded[200..208].copy_from_slice(&self.clock_probe_id.to_le_bytes());
        encoded[208..216].copy_from_slice(&self.clock_uncertainty_cycles.to_le_bytes());
        encoded[216..224].copy_from_slice(&self.required_sync_tolerance_cycles.to_le_bytes());
        encoded[224..240].copy_from_slice(&self.commit_id.as_bytes());
        Ok(encoded)
    }

    /// Decodes and re-encodes to enforce the unique V1 representation.
    pub fn decode(encoded: &[u8]) -> Result<Self, JobScheduleWireError> {
        if encoded.len() != JOB_COMMIT_WIRE_BYTES {
            return Err(JobScheduleWireError::Length);
        }
        if encoded[0..8] != COMMIT_MAGIC {
            return Err(JobScheduleWireError::Magic);
        }
        if read_u16(encoded, 8) != COMMIT_VERSION {
            return Err(JobScheduleWireError::Version);
        }
        if encoded[11..16].iter().any(|byte| *byte != 0) {
            return Err(JobScheduleWireError::Reserved);
        }
        let mut boot_id = [0_u8; BOOT_ID_BYTES];
        boot_id.copy_from_slice(&encoded[24..40]);
        let mut global_job_digest = [0_u8; 32];
        global_job_digest.copy_from_slice(&encoded[40..72]);
        let mut participant_set_digest = [0_u8; 32];
        participant_set_digest.copy_from_slice(&encoded[72..104]);
        let mut prepared_token = [0_u8; 32];
        prepared_token.copy_from_slice(&encoded[104..136]);
        let mut partition_digest = [0_u8; 32];
        partition_digest.copy_from_slice(&encoded[136..168]);
        let mut commit_id = [0_u8; JOB_COMMIT_ID_BYTES];
        commit_id.copy_from_slice(&encoded[224..240]);
        let request = Self {
            policy: JobNetworkPolicy::from_wire(encoded[10]).ok_or(JobScheduleWireError::Policy)?,
            prepare_id: read_u64(encoded, 16),
            boot_id: BootId::new(boot_id).map_err(|_| JobScheduleWireError::BootId)?,
            global_job_digest: Digest(global_job_digest),
            participant_set_digest: Digest(participant_set_digest),
            prepared_token: PreparedJobToken(Digest(prepared_token)),
            partition_digest: Digest(partition_digest),
            local_start_cycle: DeviceCycle(read_u64(encoded, 168)),
            confirm_deadline_cycle: DeviceCycle(read_u64(encoded, 176)),
            abort_guard_cycle: DeviceCycle(read_u64(encoded, 184)),
            lease_expiry_cycle: DeviceCycle(read_u64(encoded, 192)),
            clock_probe_id: read_u64(encoded, 200),
            clock_uncertainty_cycles: read_u64(encoded, 208),
            required_sync_tolerance_cycles: read_u64(encoded, 216),
            commit_id: JobCommitId::new(commit_id)?,
        };
        request.validate()?;
        if request.encode()? != encoded {
            return Err(JobScheduleWireError::Noncanonical);
        }
        Ok(request)
    }

    /// Content identity echoed by confirm/abort without repeating the schedule.
    pub fn identity(self) -> Result<Digest, JobScheduleWireError> {
        Ok(sha256(&self.encode()?).digest)
    }

    fn validate(self) -> Result<(), JobScheduleWireError> {
        if self.prepare_id == 0 {
            return Err(JobScheduleWireError::PrepareId);
        }
        if self.global_job_digest.is_zero()
            || self.participant_set_digest.is_zero()
            || self.partition_digest.is_zero()
        {
            return Err(JobScheduleWireError::Digest);
        }
        self.prepared_token.validate()?;
        JobCommitId::new(self.commit_id.as_bytes())?;
        if self.clock_probe_id == 0 {
            return Err(JobScheduleWireError::ClockProbe);
        }
        if self.required_sync_tolerance_cycles == 0
            || self.clock_uncertainty_cycles > self.required_sync_tolerance_cycles
        {
            return Err(JobScheduleWireError::Uncertainty);
        }
        if self.confirm_deadline_cycle.0 >= self.abort_guard_cycle.0
            || self.abort_guard_cycle.0 >= self.local_start_cycle.0
            || self.local_start_cycle.0 >= self.lease_expiry_cycle.0
        {
            return Err(JobScheduleWireError::CycleOrder);
        }
        Ok(())
    }
}

/// Follow-up action bound to the complete installed commit identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum JobScheduleReferenceAction {
    Confirm = REFERENCE_CONFIRM,
    Abort = REFERENCE_ABORT,
}

impl JobScheduleReferenceAction {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            REFERENCE_CONFIRM => Some(Self::Confirm),
            REFERENCE_ABORT => Some(Self::Abort),
            _ => None,
        }
    }
}

/// Exact small body for confirmation or pre-guard abort.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobScheduleReference {
    pub action: JobScheduleReferenceAction,
    pub prepare_id: u64,
    pub boot_id: BootId,
    pub commit_id: JobCommitId,
    pub commit_digest: Digest,
}

impl JobScheduleReference {
    /// Constructs the exact reference for a commit.
    pub fn for_commit(
        action: JobScheduleReferenceAction,
        commit: JobCommitRequest,
    ) -> Result<Self, JobScheduleWireError> {
        Ok(Self {
            action,
            prepare_id: commit.prepare_id,
            boot_id: commit.boot_id,
            commit_id: commit.commit_id,
            commit_digest: commit.identity()?,
        })
    }

    pub fn encode(self) -> Result<[u8; JOB_SCHEDULE_REFERENCE_WIRE_BYTES], JobScheduleWireError> {
        self.validate()?;
        let mut encoded = [0_u8; JOB_SCHEDULE_REFERENCE_WIRE_BYTES];
        encoded[0..8].copy_from_slice(&REFERENCE_MAGIC);
        encoded[8..10].copy_from_slice(&REFERENCE_VERSION.to_le_bytes());
        encoded[10] = self.action as u8;
        // Bytes 11..16 are reserved zero.
        encoded[16..24].copy_from_slice(&self.prepare_id.to_le_bytes());
        encoded[24..40].copy_from_slice(&self.boot_id.as_bytes());
        encoded[40..56].copy_from_slice(&self.commit_id.as_bytes());
        encoded[56..88].copy_from_slice(&self.commit_digest.0);
        Ok(encoded)
    }

    pub fn decode(encoded: &[u8]) -> Result<Self, JobScheduleWireError> {
        if encoded.len() != JOB_SCHEDULE_REFERENCE_WIRE_BYTES {
            return Err(JobScheduleWireError::Length);
        }
        if encoded[0..8] != REFERENCE_MAGIC {
            return Err(JobScheduleWireError::Magic);
        }
        if read_u16(encoded, 8) != REFERENCE_VERSION {
            return Err(JobScheduleWireError::Version);
        }
        if encoded[11..16].iter().any(|byte| *byte != 0) {
            return Err(JobScheduleWireError::Reserved);
        }
        let mut boot_id = [0_u8; BOOT_ID_BYTES];
        boot_id.copy_from_slice(&encoded[24..40]);
        let mut commit_id = [0_u8; JOB_COMMIT_ID_BYTES];
        commit_id.copy_from_slice(&encoded[40..56]);
        let mut commit_digest = [0_u8; 32];
        commit_digest.copy_from_slice(&encoded[56..88]);
        let reference = Self {
            action: JobScheduleReferenceAction::from_wire(encoded[10])
                .ok_or(JobScheduleWireError::Action)?,
            prepare_id: read_u64(encoded, 16),
            boot_id: BootId::new(boot_id).map_err(|_| JobScheduleWireError::BootId)?,
            commit_id: JobCommitId::new(commit_id)?,
            commit_digest: Digest(commit_digest),
        };
        reference.validate()?;
        if reference.encode()? != encoded {
            return Err(JobScheduleWireError::Noncanonical);
        }
        Ok(reference)
    }

    fn validate(self) -> Result<(), JobScheduleWireError> {
        if self.prepare_id == 0 {
            return Err(JobScheduleWireError::PrepareId);
        }
        JobCommitId::new(self.commit_id.as_bytes())?;
        if self.commit_digest.is_zero() {
            return Err(JobScheduleWireError::Digest);
        }
        Ok(())
    }
}

/// Exact idempotent extension of one installed network-attended lease.
///
/// The immutable commit remains unchanged. The request binds its full digest
/// and chooses one absolute device-cycle expiry, so retrying identical bytes
/// cannot extend authority a second time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobLeaseRenewRequest {
    pub prepare_id: u64,
    pub boot_id: BootId,
    pub commit_id: JobCommitId,
    pub commit_digest: Digest,
    pub lease_expiry_cycle: DeviceCycle,
}

impl JobLeaseRenewRequest {
    /// Constructs an exact renewal for one immutable attended commit.
    pub fn for_commit(
        commit: JobCommitRequest,
        lease_expiry_cycle: DeviceCycle,
    ) -> Result<Self, JobScheduleWireError> {
        if commit.policy != JobNetworkPolicy::NetworkAttended {
            return Err(JobScheduleWireError::Policy);
        }
        if lease_expiry_cycle.0 <= commit.lease_expiry_cycle.0 {
            return Err(JobScheduleWireError::CycleOrder);
        }
        let request = Self {
            prepare_id: commit.prepare_id,
            boot_id: commit.boot_id,
            commit_id: commit.commit_id,
            commit_digest: commit.identity()?,
            lease_expiry_cycle,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn encode(self) -> Result<[u8; JOB_LEASE_RENEW_WIRE_BYTES], JobScheduleWireError> {
        self.validate()?;
        let mut encoded = [0_u8; JOB_LEASE_RENEW_WIRE_BYTES];
        encoded[0..8].copy_from_slice(&LEASE_RENEW_MAGIC);
        encoded[8..10].copy_from_slice(&LEASE_RENEW_VERSION.to_le_bytes());
        // Bytes 10..16 are reserved zero.
        encoded[16..24].copy_from_slice(&self.prepare_id.to_le_bytes());
        encoded[24..40].copy_from_slice(&self.boot_id.as_bytes());
        encoded[40..56].copy_from_slice(&self.commit_id.as_bytes());
        encoded[56..88].copy_from_slice(&self.commit_digest.0);
        encoded[88..96].copy_from_slice(&self.lease_expiry_cycle.0.to_le_bytes());
        Ok(encoded)
    }

    pub fn decode(encoded: &[u8]) -> Result<Self, JobScheduleWireError> {
        if encoded.len() != JOB_LEASE_RENEW_WIRE_BYTES {
            return Err(JobScheduleWireError::Length);
        }
        if encoded[0..8] != LEASE_RENEW_MAGIC {
            return Err(JobScheduleWireError::Magic);
        }
        if read_u16(encoded, 8) != LEASE_RENEW_VERSION {
            return Err(JobScheduleWireError::Version);
        }
        if encoded[10..16].iter().any(|byte| *byte != 0) {
            return Err(JobScheduleWireError::Reserved);
        }
        let mut boot_id = [0_u8; BOOT_ID_BYTES];
        boot_id.copy_from_slice(&encoded[24..40]);
        let mut commit_id = [0_u8; JOB_COMMIT_ID_BYTES];
        commit_id.copy_from_slice(&encoded[40..56]);
        let mut commit_digest = [0_u8; 32];
        commit_digest.copy_from_slice(&encoded[56..88]);
        let request = Self {
            prepare_id: read_u64(encoded, 16),
            boot_id: BootId::new(boot_id).map_err(|_| JobScheduleWireError::BootId)?,
            commit_id: JobCommitId::new(commit_id)?,
            commit_digest: Digest(commit_digest),
            lease_expiry_cycle: DeviceCycle(read_u64(encoded, 88)),
        };
        request.validate()?;
        if request.encode()? != encoded {
            return Err(JobScheduleWireError::Noncanonical);
        }
        Ok(request)
    }

    fn validate(self) -> Result<(), JobScheduleWireError> {
        if self.prepare_id == 0 {
            return Err(JobScheduleWireError::PrepareId);
        }
        JobCommitId::new(self.commit_id.as_bytes())?;
        if self.commit_digest.is_zero() {
            return Err(JobScheduleWireError::Digest);
        }
        if self.lease_expiry_cycle.0 == 0 {
            return Err(JobScheduleWireError::CycleOrder);
        }
        Ok(())
    }
}

/// Local schedule lifecycle, separate from stream-prefetch/ownership state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum JobScheduleState {
    Prepared = 1,
    Installed = 2,
    Confirmed = 3,
    /// Remote abort is closed and the hardware owner must build its horizon.
    Priming = 4,
    /// The hardware owner proved the required future start horizon is staged.
    Primed = 5,
    Running = 6,
    Aborted = 7,
    Expired = 8,
    Complete = 9,
    Faulted = 10,
}

impl JobScheduleState {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Prepared),
            2 => Some(Self::Installed),
            3 => Some(Self::Confirmed),
            4 => Some(Self::Priming),
            5 => Some(Self::Primed),
            6 => Some(Self::Running),
            7 => Some(Self::Aborted),
            8 => Some(Self::Expired),
            9 => Some(Self::Complete),
            10 => Some(Self::Faulted),
            _ => None,
        }
    }
}

/// Stable schedule fault family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum JobScheduleFault {
    None = 0,
    MissedStart = 1,
    LeaseExpired = 2,
    Execution = 3,
    SafetyStop = 4,
    /// The first backend-observed output edge conflicted with its start certificate.
    StartObservation = 5,
}

impl JobScheduleFault {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::MissedStart),
            2 => Some(Self::LeaseExpired),
            3 => Some(Self::Execution),
            4 => Some(Self::SafetyStop),
            5 => Some(Self::StartObservation),
            _ => None,
        }
    }
}

/// Authority that produced one bounded start-edge observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum JobStartObservationSource {
    /// Deterministic bit-level simulator latch; never physical evidence.
    SimulatedLatch = 1,
    /// Board backend observation of the physical output latch domain.
    PeripheralLatch = 2,
    /// Software bracketing around a peripheral launch call; not a latch claim.
    SoftwareBracket = 3,
}

impl JobStartObservationSource {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::SimulatedLatch),
            2 => Some(Self::PeripheralLatch),
            3 => Some(Self::SoftwareBracket),
            _ => None,
        }
    }
}

/// Canonical evidence for the first job-owned complete-image output at start.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobStartObservation {
    /// Explicit simulator, peripheral, or software authority.
    pub source: JobStartObservationSource,
    /// Nonzero output-owner correlation token.
    pub output_token: u32,
    /// Exact cycle at which the first output was scheduled to latch.
    pub scheduled_cycle: DeviceCycle,
    /// Conservative lower cycle bound reported by the observation source.
    pub earliest_cycle: DeviceCycle,
    /// Conservative upper cycle bound reported by the observation source.
    pub latest_cycle: DeviceCycle,
}

impl JobStartObservation {
    fn validate(self) -> Result<(), JobScheduleWireError> {
        if self.output_token == 0
            || self.earliest_cycle < self.scheduled_cycle
            || self.latest_cycle < self.earliest_cycle
        {
            return Err(JobScheduleWireError::Observation);
        }
        if matches!(
            self.source,
            JobStartObservationSource::SimulatedLatch | JobStartObservationSource::PeripheralLatch
        ) && self.earliest_cycle != self.latest_cycle
        {
            return Err(JobScheduleWireError::Observation);
        }
        Ok(())
    }
}

/// Local admission facts sampled immediately before schedule installation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobScheduleAdmission {
    pub now: DeviceCycle,
    pub active_config: Digest,
    pub minimum_lead_cycles: u64,
    pub maximum_start_horizon_cycles: u64,
    pub maximum_lease_cycles: u64,
    pub maximum_sync_tolerance_cycles: u64,
    /// Minimum irrevocable interval from abort guard through local start in
    /// which the hardware owner can construct and verify its output horizon.
    pub minimum_prime_lead_cycles: u64,
    pub cache_ready: bool,
    pub safety_ready: bool,
    pub autonomous_allowed: bool,
}

/// Local facts bounding one authenticated network-attended lease extension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobLeaseRenewalAdmission {
    /// Device cycle at which core 1 applies the request.
    pub now: DeviceCycle,
    /// Maximum new expiry displacement from `now`.
    pub maximum_extension_cycles: u64,
    /// Maximum absolute expiry displacement from the installed start cycle.
    pub maximum_total_lease_cycles: u64,
    /// Fresh local safety/deadline state required to extend energy authority.
    pub safety_ready: bool,
}

/// One action the real-time timer/execution owner must perform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobScheduleAction {
    None,
    AbortUnconfirmed,
    /// Remote abort is no longer legal; construct the immutable hardware
    /// horizon and acknowledge it with [`PreparedJobSchedule::mark_primed`].
    PrimeHardware {
        scheduled_cycle: DeviceCycle,
        lease_expiry_cycle: DeviceCycle,
    },
    Start {
        scheduled_cycle: DeviceCycle,
        lease_expiry_cycle: DeviceCycle,
    },
    MissedStart,
    LeaseExpired,
}

/// Core-local boot-bound schedule authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedJobSchedule {
    prepare_id: u64,
    boot_id: BootId,
    prepared_token: PreparedJobToken,
    partition_digest: Digest,
    config_digest: Digest,
    state: JobScheduleState,
    fault: JobScheduleFault,
    commit: Option<JobCommitRequest>,
    lease_expiry_cycle: DeviceCycle,
    start_emitted: bool,
    start_observation: Option<JobStartObservation>,
}

impl PreparedJobSchedule {
    /// Creates schedule authority only from the complete validated descriptor.
    pub fn prepare<const AXES: usize>(
        boot_id: BootId,
        descriptor: JobDescriptor,
    ) -> Result<Self, JobScheduleWireError> {
        Ok(Self {
            prepare_id: descriptor.prepare_id,
            boot_id,
            prepared_token: PreparedJobToken::derive::<AXES>(boot_id, descriptor)?,
            partition_digest: descriptor.partition.object.content.digest,
            config_digest: descriptor.config_digest,
            state: JobScheduleState::Prepared,
            fault: JobScheduleFault::None,
            commit: None,
            lease_expiry_cycle: DeviceCycle(0),
            start_emitted: false,
            start_observation: None,
        })
    }

    /// Installs one exact future schedule without granting start authority.
    pub fn install(
        &mut self,
        request: JobCommitRequest,
        admission: JobScheduleAdmission,
    ) -> Result<JobScheduleReport, JobScheduleError> {
        request.validate().map_err(JobScheduleError::Wire)?;
        if let Some(commit) = self.commit {
            if commit == request
                && matches!(
                    self.state,
                    JobScheduleState::Installed
                        | JobScheduleState::Confirmed
                        | JobScheduleState::Priming
                        | JobScheduleState::Primed
                )
            {
                return Ok(self.report());
            }
            return Err(JobScheduleError::Conflict);
        }
        if self.state != JobScheduleState::Prepared {
            return Err(JobScheduleError::State);
        }
        if request.prepare_id != self.prepare_id
            || request.boot_id != self.boot_id
            || request.prepared_token != self.prepared_token
            || request.partition_digest != self.partition_digest
            || admission.active_config != self.config_digest
        {
            return Err(JobScheduleError::Identity);
        }
        if !admission.cache_ready || !admission.safety_ready {
            return Err(JobScheduleError::Readiness);
        }
        if request.policy == JobNetworkPolicy::CachedAutonomous && !admission.autonomous_allowed {
            return Err(JobScheduleError::Policy);
        }
        if admission.minimum_lead_cycles == 0
            || admission.maximum_start_horizon_cycles < admission.minimum_lead_cycles
            || admission.maximum_lease_cycles == 0
            || admission.maximum_sync_tolerance_cycles == 0
            || admission.minimum_prime_lead_cycles == 0
            || request.required_sync_tolerance_cycles > admission.maximum_sync_tolerance_cycles
        {
            return Err(JobScheduleError::Policy);
        }
        let lead = request
            .local_start_cycle
            .0
            .checked_sub(admission.now.0)
            .ok_or(JobScheduleError::Deadline)?;
        let lease = request
            .lease_expiry_cycle
            .0
            .checked_sub(request.local_start_cycle.0)
            .ok_or(JobScheduleError::Deadline)?;
        let prime_lead = request
            .local_start_cycle
            .0
            .checked_sub(request.abort_guard_cycle.0)
            .ok_or(JobScheduleError::Deadline)?;
        if request.confirm_deadline_cycle.0 <= admission.now.0
            || lead < admission.minimum_lead_cycles
            || lead > admission.maximum_start_horizon_cycles
            || lease > admission.maximum_lease_cycles
            || prime_lead < admission.minimum_prime_lead_cycles
        {
            return Err(JobScheduleError::Deadline);
        }
        self.commit = Some(request);
        self.lease_expiry_cycle = request.lease_expiry_cycle;
        self.state = JobScheduleState::Installed;
        Ok(self.report())
    }

    /// Idempotently extends only a network-attended lease under a fresh local cap.
    pub fn renew_lease(
        &mut self,
        request: JobLeaseRenewRequest,
        admission: JobLeaseRenewalAdmission,
    ) -> Result<JobScheduleReport, JobScheduleError> {
        request.validate().map_err(JobScheduleError::Wire)?;
        let commit = self.commit.ok_or(JobScheduleError::State)?;
        if commit.policy != JobNetworkPolicy::NetworkAttended {
            return Err(JobScheduleError::Policy);
        }
        if request.prepare_id != self.prepare_id
            || request.boot_id != self.boot_id
            || request.commit_id != commit.commit_id
            || request.commit_digest != commit.identity().map_err(JobScheduleError::Wire)?
        {
            return Err(JobScheduleError::Identity);
        }
        if !matches!(
            self.state,
            JobScheduleState::Confirmed
                | JobScheduleState::Priming
                | JobScheduleState::Primed
                | JobScheduleState::Running
        ) {
            return Err(JobScheduleError::State);
        }
        if !admission.safety_ready
            || admission.maximum_extension_cycles == 0
            || admission.maximum_total_lease_cycles == 0
        {
            return Err(JobScheduleError::Readiness);
        }
        if admission.now.0 >= self.lease_expiry_cycle.0 {
            return Err(JobScheduleError::Deadline);
        }
        if request.lease_expiry_cycle == self.lease_expiry_cycle {
            return Ok(self.report());
        }
        let extension = request
            .lease_expiry_cycle
            .0
            .checked_sub(admission.now.0)
            .ok_or(JobScheduleError::Deadline)?;
        let total = request
            .lease_expiry_cycle
            .0
            .checked_sub(commit.local_start_cycle.0)
            .ok_or(JobScheduleError::Deadline)?;
        if request.lease_expiry_cycle.0 <= self.lease_expiry_cycle.0
            || extension > admission.maximum_extension_cycles
            || total > admission.maximum_total_lease_cycles
        {
            return Err(JobScheduleError::Deadline);
        }
        self.lease_expiry_cycle = request.lease_expiry_cycle;
        Ok(self.report())
    }

    /// Grants start authority only before both confirmation and abort guards.
    pub fn confirm(
        &mut self,
        reference: JobScheduleReference,
        now: DeviceCycle,
    ) -> Result<JobScheduleReport, JobScheduleError> {
        self.match_reference(reference, JobScheduleReferenceAction::Confirm)?;
        if matches!(
            self.state,
            JobScheduleState::Confirmed | JobScheduleState::Priming | JobScheduleState::Primed
        ) {
            return Ok(self.report());
        }
        if self.state != JobScheduleState::Installed {
            return Err(JobScheduleError::State);
        }
        let commit = self.commit.ok_or(JobScheduleError::State)?;
        if now.0 >= commit.confirm_deadline_cycle.0 || now.0 >= commit.abort_guard_cycle.0 {
            return Err(JobScheduleError::Deadline);
        }
        self.state = JobScheduleState::Confirmed;
        Ok(self.report())
    }

    /// Idempotently revokes an installed/confirmed start before the guard.
    pub fn abort(
        &mut self,
        reference: JobScheduleReference,
        now: DeviceCycle,
    ) -> Result<JobScheduleReport, JobScheduleError> {
        self.match_reference(reference, JobScheduleReferenceAction::Abort)?;
        if matches!(
            self.state,
            JobScheduleState::Aborted | JobScheduleState::Expired
        ) {
            return Ok(self.report());
        }
        if !matches!(
            self.state,
            JobScheduleState::Installed | JobScheduleState::Confirmed
        ) {
            return Err(JobScheduleError::State);
        }
        let commit = self.commit.ok_or(JobScheduleError::State)?;
        if now.0 >= commit.abort_guard_cycle.0 {
            return Err(JobScheduleError::Deadline);
        }
        self.state = JobScheduleState::Aborted;
        Ok(self.report())
    }

    /// Advances only clock-derived transitions and emits each hazardous action
    /// at most once.
    pub fn advance(&mut self, now: DeviceCycle) -> JobScheduleAction {
        let Some(commit) = self.commit else {
            return JobScheduleAction::None;
        };
        if self.state == JobScheduleState::Installed && now.0 >= commit.confirm_deadline_cycle.0 {
            self.state = JobScheduleState::Expired;
            return JobScheduleAction::AbortUnconfirmed;
        }
        if self.state == JobScheduleState::Confirmed && now.0 >= commit.local_start_cycle.0 {
            self.state = JobScheduleState::Faulted;
            self.fault = JobScheduleFault::MissedStart;
            return JobScheduleAction::MissedStart;
        }
        if self.state == JobScheduleState::Confirmed && now.0 >= commit.abort_guard_cycle.0 {
            self.state = JobScheduleState::Priming;
            return JobScheduleAction::PrimeHardware {
                scheduled_cycle: commit.local_start_cycle,
                lease_expiry_cycle: self.lease_expiry_cycle,
            };
        }
        if self.state == JobScheduleState::Priming && now.0 >= commit.local_start_cycle.0 {
            self.state = JobScheduleState::Faulted;
            self.fault = JobScheduleFault::MissedStart;
            return JobScheduleAction::MissedStart;
        }
        if self.state == JobScheduleState::Primed && now.0 >= commit.local_start_cycle.0 {
            let lateness = now.0 - commit.local_start_cycle.0;
            if lateness > commit.required_sync_tolerance_cycles {
                self.state = JobScheduleState::Faulted;
                self.fault = JobScheduleFault::MissedStart;
                return JobScheduleAction::MissedStart;
            }
            self.state = JobScheduleState::Running;
            self.start_emitted = true;
            return JobScheduleAction::Start {
                scheduled_cycle: commit.local_start_cycle,
                lease_expiry_cycle: self.lease_expiry_cycle,
            };
        }
        if self.state == JobScheduleState::Running && now.0 >= self.lease_expiry_cycle.0 {
            self.state = JobScheduleState::Faulted;
            self.fault = JobScheduleFault::LeaseExpired;
            return JobScheduleAction::LeaseExpired;
        }
        JobScheduleAction::None
    }

    /// Acknowledges that the sole local hardware owner has staged and checked
    /// the required future output horizon after the abort guard closed.
    pub fn mark_primed(&mut self, now: DeviceCycle) -> Result<JobScheduleReport, JobScheduleError> {
        if self.state == JobScheduleState::Primed {
            return Ok(self.report());
        }
        if self.state != JobScheduleState::Priming {
            return Err(JobScheduleError::State);
        }
        let commit = self.commit.ok_or(JobScheduleError::State)?;
        if now.0 >= commit.local_start_cycle.0 {
            self.state = JobScheduleState::Faulted;
            self.fault = JobScheduleFault::MissedStart;
            return Err(JobScheduleError::Deadline);
        }
        self.state = JobScheduleState::Primed;
        Ok(self.report())
    }

    /// Records the first job-owned output latch after start authority emitted.
    ///
    /// A valid observation is retained even when its latest bound exceeds the
    /// installed synchronization tolerance. That violation changes the
    /// schedule to `Faulted/StartObservation` so telemetry preserves the
    /// evidence that caused the fail-closed outcome.
    pub fn record_start_observation(
        &mut self,
        observation: JobStartObservation,
    ) -> Result<JobScheduleReport, JobScheduleError> {
        if let Some(existing) = self.start_observation {
            return if existing == observation {
                Ok(self.report())
            } else {
                Err(JobScheduleError::Conflict)
            };
        }
        if self.state != JobScheduleState::Running || !self.start_emitted {
            return Err(JobScheduleError::State);
        }
        observation.validate().map_err(JobScheduleError::Wire)?;
        let commit = self.commit.ok_or(JobScheduleError::State)?;
        if observation.scheduled_cycle != commit.local_start_cycle {
            return Err(JobScheduleError::Identity);
        }
        self.start_observation = Some(observation);
        let lateness = observation.latest_cycle.0 - observation.scheduled_cycle.0;
        if lateness > commit.required_sync_tolerance_cycles {
            self.state = JobScheduleState::Faulted;
            self.fault = JobScheduleFault::StartObservation;
        }
        Ok(self.report())
    }

    /// Marks exact terminal execution before the finite lease expires.
    pub fn complete(&mut self, now: DeviceCycle) -> Result<JobScheduleReport, JobScheduleError> {
        if self.state != JobScheduleState::Running || self.start_observation.is_none() {
            return Err(JobScheduleError::State);
        }
        self.commit.ok_or(JobScheduleError::State)?;
        if now.0 >= self.lease_expiry_cycle.0 {
            self.state = JobScheduleState::Faulted;
            self.fault = JobScheduleFault::LeaseExpired;
            return Err(JobScheduleError::Deadline);
        }
        self.state = JobScheduleState::Complete;
        Ok(self.report())
    }

    /// Latches an execution-engine failure after a start action was emitted.
    pub fn fault_execution(&mut self) -> Result<JobScheduleReport, JobScheduleError> {
        if self.state != JobScheduleState::Running || !self.start_emitted {
            return Err(JobScheduleError::State);
        }
        self.state = JobScheduleState::Faulted;
        self.fault = JobScheduleFault::Execution;
        Ok(self.report())
    }

    /// Latches a local safety stop before or after the scheduled start. The
    /// caller must apply the board-safe output transaction first.
    pub fn fault_safety_stop(&mut self) -> Result<JobScheduleReport, JobScheduleError> {
        if matches!(
            self.state,
            JobScheduleState::Aborted
                | JobScheduleState::Expired
                | JobScheduleState::Complete
                | JobScheduleState::Faulted
        ) {
            return Err(JobScheduleError::State);
        }
        self.state = JobScheduleState::Faulted;
        self.fault = JobScheduleFault::SafetyStop;
        Ok(self.report())
    }

    /// Fixed status suitable for the combined authenticated job response.
    pub fn report(&self) -> JobScheduleReport {
        let (policy, prepared_token, start, confirm, abort, lease, commit_id) = self.commit.map_or(
            (
                None,
                Some(self.prepared_token),
                0,
                0,
                0,
                0,
                [0; JOB_COMMIT_ID_BYTES],
            ),
            |commit| {
                (
                    Some(commit.policy),
                    None,
                    commit.local_start_cycle.0,
                    commit.confirm_deadline_cycle.0,
                    commit.abort_guard_cycle.0,
                    self.lease_expiry_cycle.0,
                    commit.commit_id.as_bytes(),
                )
            },
        );
        JobScheduleReport {
            state: self.state,
            fault: self.fault,
            policy,
            descriptor_token: self.prepared_token,
            prepared_token,
            start_emitted: self.start_emitted,
            start_observation: self.start_observation,
            local_start_cycle: DeviceCycle(start),
            confirm_deadline_cycle: DeviceCycle(confirm),
            abort_guard_cycle: DeviceCycle(abort),
            lease_expiry_cycle: DeviceCycle(lease),
            commit_id,
        }
    }

    pub const fn prepared_token(&self) -> PreparedJobToken {
        self.prepared_token
    }

    pub const fn boot_id(&self) -> BootId {
        self.boot_id
    }

    fn match_reference(
        &self,
        reference: JobScheduleReference,
        expected: JobScheduleReferenceAction,
    ) -> Result<(), JobScheduleError> {
        reference.validate().map_err(JobScheduleError::Wire)?;
        if reference.action != expected
            || reference.prepare_id != self.prepare_id
            || reference.boot_id != self.boot_id
        {
            return Err(JobScheduleError::Identity);
        }
        let commit = self.commit.ok_or(JobScheduleError::State)?;
        if reference.commit_id != commit.commit_id
            || reference.commit_digest != commit.identity().map_err(JobScheduleError::Wire)?
        {
            return Err(JobScheduleError::Identity);
        }
        Ok(())
    }
}

/// Compact schedule section fitting the existing fixed service envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobScheduleReport {
    pub state: JobScheduleState,
    pub fault: JobScheduleFault,
    pub policy: Option<JobNetworkPolicy>,
    /// Exact boot-and-descriptor identity retained before and after commit.
    pub descriptor_token: PreparedJobToken,
    /// Present only before commit; this is the browser's boot-bound prepare receipt.
    pub prepared_token: Option<PreparedJobToken>,
    pub start_emitted: bool,
    /// First job-owned output latch evidence, absent until the backend reports it.
    pub start_observation: Option<JobStartObservation>,
    pub local_start_cycle: DeviceCycle,
    pub confirm_deadline_cycle: DeviceCycle,
    pub abort_guard_cycle: DeviceCycle,
    pub lease_expiry_cycle: DeviceCycle,
    pub commit_id: [u8; JOB_COMMIT_ID_BYTES],
}

impl JobScheduleReport {
    pub fn encode(self) -> Result<[u8; JOB_SCHEDULE_REPORT_WIRE_BYTES], JobScheduleWireError> {
        self.validate()?;
        let mut encoded = [0_u8; JOB_SCHEDULE_REPORT_WIRE_BYTES];
        encoded[0..8].copy_from_slice(&REPORT_MAGIC);
        encoded[8..10].copy_from_slice(&REPORT_VERSION.to_le_bytes());
        encoded[10] = self.state as u8;
        encoded[11] = self.fault as u8;
        let mut flags = 0_u16;
        if let Some(policy) = self.policy {
            flags |= REPORT_FLAG_COMMIT;
            flags |= match policy {
                JobNetworkPolicy::NetworkAttended => REPORT_FLAG_ATTENDED,
                JobNetworkPolicy::CachedAutonomous => REPORT_FLAG_AUTONOMOUS,
            };
        }
        if self.start_emitted {
            flags |= REPORT_FLAG_START_EMITTED;
        }
        encoded[12..14].copy_from_slice(&flags.to_le_bytes());
        // Bytes 14..16 are reserved zero.
        if let Some(prepared_token) = self.prepared_token {
            encoded[16..48].copy_from_slice(&prepared_token.0.0);
            // Bytes 48..64 are the zero-filled uncommitted union tail.
        } else {
            encoded[16..24].copy_from_slice(&self.local_start_cycle.0.to_le_bytes());
            encoded[24..32].copy_from_slice(&self.confirm_deadline_cycle.0.to_le_bytes());
            encoded[32..40].copy_from_slice(&self.abort_guard_cycle.0.to_le_bytes());
            encoded[40..48].copy_from_slice(&self.lease_expiry_cycle.0.to_le_bytes());
            encoded[48..64].copy_from_slice(&self.commit_id);
        }
        if let Some(observation) = self.start_observation {
            observation.validate()?;
            encoded[64] = observation.source as u8;
            // Bytes 65..68 remain reserved zero.
            encoded[68..72].copy_from_slice(&observation.output_token.to_le_bytes());
            encoded[72..80].copy_from_slice(&observation.scheduled_cycle.0.to_le_bytes());
            encoded[80..88].copy_from_slice(&observation.earliest_cycle.0.to_le_bytes());
            encoded[88..96].copy_from_slice(&observation.latest_cycle.0.to_le_bytes());
        }
        encoded[96..128].copy_from_slice(&self.descriptor_token.0.0);
        Ok(encoded)
    }

    pub fn decode(encoded: &[u8]) -> Result<Self, JobScheduleWireError> {
        if encoded.len() != JOB_SCHEDULE_REPORT_WIRE_BYTES {
            return Err(JobScheduleWireError::Length);
        }
        if encoded[0..8] != REPORT_MAGIC {
            return Err(JobScheduleWireError::Magic);
        }
        if read_u16(encoded, 8) != REPORT_VERSION {
            return Err(JobScheduleWireError::Version);
        }
        if encoded[14..16].iter().any(|byte| *byte != 0) {
            return Err(JobScheduleWireError::Reserved);
        }
        let flags = read_u16(encoded, 12);
        if flags & !REPORT_KNOWN_FLAGS != 0 {
            return Err(JobScheduleWireError::Reserved);
        }
        let policy = match (
            flags & REPORT_FLAG_ATTENDED != 0,
            flags & REPORT_FLAG_AUTONOMOUS != 0,
        ) {
            (true, false) => Some(JobNetworkPolicy::NetworkAttended),
            (false, true) => Some(JobNetworkPolicy::CachedAutonomous),
            (false, false) => None,
            (true, true) => return Err(JobScheduleWireError::Policy),
        };
        if policy.is_some() != (flags & REPORT_FLAG_COMMIT != 0) {
            return Err(JobScheduleWireError::Policy);
        }
        let committed = policy.is_some();
        let (
            prepared_token,
            local_start_cycle,
            confirm_deadline_cycle,
            abort_guard_cycle,
            lease_expiry_cycle,
            commit_id,
        ) = if committed {
            let mut commit_id = [0_u8; JOB_COMMIT_ID_BYTES];
            commit_id.copy_from_slice(&encoded[48..64]);
            (
                None,
                DeviceCycle(read_u64(encoded, 16)),
                DeviceCycle(read_u64(encoded, 24)),
                DeviceCycle(read_u64(encoded, 32)),
                DeviceCycle(read_u64(encoded, 40)),
                commit_id,
            )
        } else {
            if encoded[48..64].iter().any(|byte| *byte != 0) {
                return Err(JobScheduleWireError::Reserved);
            }
            let mut token = [0_u8; 32];
            token.copy_from_slice(&encoded[16..48]);
            (
                Some(PreparedJobToken(Digest(token))),
                DeviceCycle(0),
                DeviceCycle(0),
                DeviceCycle(0),
                DeviceCycle(0),
                [0; JOB_COMMIT_ID_BYTES],
            )
        };
        let report = Self {
            state: JobScheduleState::from_wire(encoded[10]).ok_or(JobScheduleWireError::State)?,
            fault: JobScheduleFault::from_wire(encoded[11]).ok_or(JobScheduleWireError::Fault)?,
            policy,
            descriptor_token: {
                let mut token = [0_u8; 32];
                token.copy_from_slice(&encoded[96..128]);
                PreparedJobToken(Digest(token))
            },
            prepared_token,
            start_emitted: flags & REPORT_FLAG_START_EMITTED != 0,
            start_observation: if encoded[64] == 0 {
                if encoded[65..96].iter().any(|byte| *byte != 0) {
                    return Err(JobScheduleWireError::Reserved);
                }
                None
            } else {
                if encoded[65..68].iter().any(|byte| *byte != 0) {
                    return Err(JobScheduleWireError::Reserved);
                }
                Some(JobStartObservation {
                    source: JobStartObservationSource::from_wire(encoded[64])
                        .ok_or(JobScheduleWireError::Observation)?,
                    output_token: read_u32(encoded, 68),
                    scheduled_cycle: DeviceCycle(read_u64(encoded, 72)),
                    earliest_cycle: DeviceCycle(read_u64(encoded, 80)),
                    latest_cycle: DeviceCycle(read_u64(encoded, 88)),
                })
            },
            local_start_cycle,
            confirm_deadline_cycle,
            abort_guard_cycle,
            lease_expiry_cycle,
            commit_id,
        };
        report.validate()?;
        if report.encode()? != encoded {
            return Err(JobScheduleWireError::Noncanonical);
        }
        Ok(report)
    }

    fn validate(self) -> Result<(), JobScheduleWireError> {
        self.descriptor_token.validate()?;
        let committed = self.policy.is_some();
        if !committed {
            let valid_state = matches!(
                (self.state, self.fault),
                (JobScheduleState::Prepared, JobScheduleFault::None)
                    | (JobScheduleState::Faulted, JobScheduleFault::SafetyStop)
            );
            if !valid_state
                || self.prepared_token != Some(self.descriptor_token)
                || self.start_emitted
                || self.start_observation.is_some()
                || self.local_start_cycle.0 != 0
                || self.confirm_deadline_cycle.0 != 0
                || self.abort_guard_cycle.0 != 0
                || self.lease_expiry_cycle.0 != 0
                || bytes_nonzero(&self.commit_id)
            {
                return Err(JobScheduleWireError::StateShape);
            }
            return Ok(());
        }
        if self.prepared_token.is_some() {
            return Err(JobScheduleWireError::StateShape);
        }
        if let Some(observation) = self.start_observation {
            observation.validate()?;
            if !self.start_emitted || observation.scheduled_cycle != self.local_start_cycle {
                return Err(JobScheduleWireError::StateShape);
            }
        }
        if !matches!(
            self.state,
            JobScheduleState::Installed
                | JobScheduleState::Confirmed
                | JobScheduleState::Priming
                | JobScheduleState::Primed
                | JobScheduleState::Running
                | JobScheduleState::Aborted
                | JobScheduleState::Expired
                | JobScheduleState::Complete
                | JobScheduleState::Faulted
        ) {
            return Err(JobScheduleWireError::StateShape);
        }
        JobCommitId::new(self.commit_id)?;
        if self.confirm_deadline_cycle.0 >= self.abort_guard_cycle.0
            || self.abort_guard_cycle.0 >= self.local_start_cycle.0
            || self.local_start_cycle.0 >= self.lease_expiry_cycle.0
        {
            return Err(JobScheduleWireError::CycleOrder);
        }
        let start_required = matches!(
            self.state,
            JobScheduleState::Running | JobScheduleState::Complete
        ) || self.state == JobScheduleState::Faulted
            && matches!(
                self.fault,
                JobScheduleFault::LeaseExpired
                    | JobScheduleFault::Execution
                    | JobScheduleFault::StartObservation
            );
        if self.fault != JobScheduleFault::SafetyStop && self.start_emitted != start_required {
            return Err(JobScheduleWireError::StateShape);
        }
        if (self.state == JobScheduleState::Faulted) != (self.fault != JobScheduleFault::None) {
            return Err(JobScheduleWireError::StateShape);
        }
        if self.state != JobScheduleState::Faulted && self.fault != JobScheduleFault::None {
            return Err(JobScheduleWireError::StateShape);
        }
        if self.state == JobScheduleState::Complete && self.start_observation.is_none()
            || self.fault == JobScheduleFault::StartObservation && self.start_observation.is_none()
        {
            return Err(JobScheduleWireError::StateShape);
        }
        Ok(())
    }
}

/// Fixed wire-format rejection for schedule bodies and reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobScheduleWireError {
    Length,
    Magic,
    Version,
    Reserved,
    Noncanonical,
    Action,
    Policy,
    PrepareId,
    BootId,
    CommitId,
    PreparedToken,
    Digest,
    ClockProbe,
    Uncertainty,
    CycleOrder,
    State,
    Fault,
    StateShape,
    Observation,
    Descriptor(JobDescriptorWireError),
}

/// State/admission rejection that never partially installs a schedule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobScheduleError {
    Wire(JobScheduleWireError),
    Identity,
    Conflict,
    State,
    Readiness,
    Policy,
    Deadline,
}

const fn bytes_nonzero<const N: usize>(bytes: &[u8; N]) -> bool {
    let mut index = 0;
    while index < N {
        if bytes[index] != 0 {
            return true;
        }
        index += 1;
    }
    false
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

#[cfg(test)]
mod tests {
    use alumina_machine_ir::{
        BlockValidationLimits, EXECUTION_BLOCK_BYTES, StreamId, StreamTick, ValidationLimits,
    };
    use alumina_storage::{ContentId, DigestAlgorithm, ObjectKind, PublishedObject, StoredObject};

    use super::*;

    fn boot(byte: u8) -> BootId {
        BootId::new([byte; BOOT_ID_BYTES]).unwrap()
    }

    fn descriptor() -> JobDescriptor {
        JobDescriptor {
            prepare_id: 7,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId {
                        algorithm: DigestAlgorithm::Sha256,
                        digest: Digest([0x11; 32]),
                    },
                    byte_len: u64::try_from(EXECUTION_BLOCK_BYTES).unwrap(),
                },
                manifest: ContentId {
                    algorithm: DigestAlgorithm::Sha256,
                    digest: Digest([0x22; 32]),
                },
            },
            stream_id: StreamId([0x33; 16]),
            capability_digest: Digest([0x44; 32]),
            config_digest: Digest([0x55; 32]),
            axis_count: 3,
            execution_kind: alumina_machine_ir::ExecutionKind::Motion,
            maximum_dense_updates: 0,
            dense_update_period_ticks: 0,
            block_count: 1,
            first_tick: StreamTick(0),
            initial_position: [0; alumina_machine_ir::MAX_EXECUTION_AXES],
            limits: BlockValidationLimits {
                maximum_block_ticks: 10_000,
                segment: ValidationLimits {
                    maximum_segment_ticks: 10_000,
                    maximum_steps_per_segment: 1_000,
                },
            },
        }
    }

    fn commit() -> JobCommitRequest {
        let descriptor = descriptor();
        JobCommitRequest {
            policy: JobNetworkPolicy::NetworkAttended,
            prepare_id: descriptor.prepare_id,
            boot_id: boot(0x66),
            global_job_digest: Digest([0x77; 32]),
            participant_set_digest: Digest([0x88; 32]),
            prepared_token: PreparedJobToken::derive::<3>(boot(0x66), descriptor).unwrap(),
            partition_digest: descriptor.partition.object.content.digest,
            local_start_cycle: DeviceCycle(10_000),
            confirm_deadline_cycle: DeviceCycle(8_000),
            abort_guard_cycle: DeviceCycle(9_000),
            lease_expiry_cycle: DeviceCycle(20_000),
            clock_probe_id: 99,
            clock_uncertainty_cycles: 10,
            required_sync_tolerance_cycles: 25,
            commit_id: JobCommitId::new([0x99; JOB_COMMIT_ID_BYTES]).unwrap(),
        }
    }

    fn admission(now: u64) -> JobScheduleAdmission {
        JobScheduleAdmission {
            now: DeviceCycle(now),
            active_config: descriptor().config_digest,
            minimum_lead_cycles: 1_000,
            maximum_start_horizon_cycles: 20_000,
            maximum_lease_cycles: 20_000,
            maximum_sync_tolerance_cycles: 100,
            minimum_prime_lead_cycles: 500,
            cache_ready: true,
            safety_ready: true,
            autonomous_allowed: false,
        }
    }

    fn renewal_admission(now: u64) -> JobLeaseRenewalAdmission {
        JobLeaseRenewalAdmission {
            now: DeviceCycle(now),
            maximum_extension_cycles: 20_000,
            maximum_total_lease_cycles: 30_000,
            safety_ready: true,
        }
    }

    fn start_observation(lateness: u64) -> JobStartObservation {
        JobStartObservation {
            source: JobStartObservationSource::SimulatedLatch,
            output_token: 17,
            scheduled_cycle: DeviceCycle(10_000),
            earliest_cycle: DeviceCycle(10_000 + lateness),
            latest_cycle: DeviceCycle(10_000 + lateness),
        }
    }

    #[test]
    fn commit_reference_token_and_report_are_canonical() {
        let commit = commit();
        let encoded = commit.encode().unwrap();
        assert_eq!(encoded.len(), JOB_COMMIT_WIRE_BYTES);
        assert_eq!(&encoded[..8], b"ALMJCOM2");
        assert_eq!(&encoded[8..10], &2_u16.to_le_bytes());
        assert_eq!(JobCommitRequest::decode(&encoded), Ok(commit));
        assert!(!commit.identity().unwrap().is_zero());
        let mut legacy = encoded;
        legacy[..8].copy_from_slice(b"ALMJCOM1");
        legacy[8..10].copy_from_slice(&1_u16.to_le_bytes());
        assert_eq!(
            JobCommitRequest::decode(&legacy),
            Err(JobScheduleWireError::Magic)
        );

        let confirm =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit).unwrap();
        let encoded = confirm.encode().unwrap();
        assert_eq!(encoded.len(), JOB_SCHEDULE_REFERENCE_WIRE_BYTES);
        assert_eq!(&encoded[..8], b"ALMJREF2");
        assert_eq!(&encoded[8..10], &2_u16.to_le_bytes());
        assert_eq!(JobScheduleReference::decode(&encoded), Ok(confirm));
        let mut legacy = encoded;
        legacy[..8].copy_from_slice(b"ALMJREF1");
        legacy[8..10].copy_from_slice(&1_u16.to_le_bytes());
        assert_eq!(
            JobScheduleReference::decode(&legacy),
            Err(JobScheduleWireError::Magic)
        );

        let renewal = JobLeaseRenewRequest::for_commit(commit, DeviceCycle(25_000)).unwrap();
        let encoded = renewal.encode().unwrap();
        assert_eq!(encoded.len(), JOB_LEASE_RENEW_WIRE_BYTES);
        assert_eq!(&encoded[..8], b"ALMJREN1");
        assert_eq!(&encoded[8..10], &1_u16.to_le_bytes());
        assert_eq!(JobLeaseRenewRequest::decode(&encoded), Ok(renewal));

        let schedule = PreparedJobSchedule::prepare::<3>(boot(0x66), descriptor()).unwrap();
        let report = schedule.report();
        let encoded = report.encode().unwrap();
        assert_eq!(encoded.len(), JOB_SCHEDULE_REPORT_WIRE_BYTES);
        assert_eq!(&encoded[..8], b"ALMJSCH5");
        assert_eq!(&encoded[8..10], &5_u16.to_le_bytes());
        assert_eq!(JobScheduleReport::decode(&encoded), Ok(report));
        let mut legacy = encoded;
        legacy[..8].copy_from_slice(b"ALMJSCH1");
        legacy[8..10].copy_from_slice(&1_u16.to_le_bytes());
        assert_eq!(
            JobScheduleReport::decode(&legacy),
            Err(JobScheduleWireError::Magic)
        );
    }

    #[test]
    fn attended_lease_renewal_is_absolute_idempotent_and_locally_bounded() {
        let commit = commit();
        let mut schedule = PreparedJobSchedule::prepare::<3>(boot(0x66), descriptor()).unwrap();
        schedule.install(commit, admission(1_000)).unwrap();
        let confirm =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit).unwrap();
        schedule.confirm(confirm, DeviceCycle(7_000)).unwrap();

        let renewal = JobLeaseRenewRequest::for_commit(commit, DeviceCycle(25_000)).unwrap();
        let report = schedule
            .renew_lease(renewal, renewal_admission(7_500))
            .unwrap();
        assert_eq!(report.lease_expiry_cycle, DeviceCycle(25_000));
        assert_eq!(
            schedule.renew_lease(renewal, renewal_admission(8_000)),
            Ok(report)
        );

        let shorter = JobLeaseRenewRequest::for_commit(commit, DeviceCycle(24_000)).unwrap();
        assert_eq!(
            schedule.renew_lease(shorter, renewal_admission(8_100)),
            Err(JobScheduleError::Deadline)
        );
        let too_far = JobLeaseRenewRequest::for_commit(commit, DeviceCycle(40_001)).unwrap();
        assert_eq!(
            schedule.renew_lease(too_far, renewal_admission(8_200)),
            Err(JobScheduleError::Deadline)
        );

        let mut autonomous = commit;
        autonomous.policy = JobNetworkPolicy::CachedAutonomous;
        assert_eq!(
            JobLeaseRenewRequest::for_commit(autonomous, DeviceCycle(25_000)),
            Err(JobScheduleWireError::Policy)
        );
    }

    #[test]
    fn install_confirm_prime_start_and_complete_are_strictly_ordered() {
        let commit = commit();
        let mut schedule = PreparedJobSchedule::prepare::<3>(boot(0x66), descriptor()).unwrap();
        assert_eq!(schedule.prepared_token(), commit.prepared_token);
        let installed = schedule.install(commit, admission(1_000)).unwrap();
        assert_eq!(installed.state, JobScheduleState::Installed);
        assert_eq!(installed.descriptor_token, commit.prepared_token);
        assert_eq!(installed.prepared_token, None);
        assert_eq!(
            JobScheduleReport::decode(&installed.encode().unwrap()),
            Ok(installed)
        );
        assert_eq!(schedule.install(commit, admission(1_001)), Ok(installed));

        let confirm =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit).unwrap();
        assert_eq!(
            schedule.confirm(confirm, DeviceCycle(7_000)).unwrap().state,
            JobScheduleState::Confirmed
        );
        assert_eq!(
            schedule.advance(DeviceCycle(8_999)),
            JobScheduleAction::None
        );
        assert_eq!(
            schedule.advance(DeviceCycle(9_000)),
            JobScheduleAction::PrimeHardware {
                scheduled_cycle: DeviceCycle(10_000),
                lease_expiry_cycle: DeviceCycle(20_000),
            }
        );
        assert_eq!(schedule.report().state, JobScheduleState::Priming);
        assert_eq!(
            schedule.mark_primed(DeviceCycle(9_001)).unwrap().state,
            JobScheduleState::Primed
        );
        assert_eq!(
            schedule.mark_primed(DeviceCycle(9_002)).unwrap().state,
            JobScheduleState::Primed
        );
        assert_eq!(
            schedule.advance(DeviceCycle(10_000)),
            JobScheduleAction::Start {
                scheduled_cycle: DeviceCycle(10_000),
                lease_expiry_cycle: DeviceCycle(20_000),
            }
        );
        assert_eq!(
            schedule.advance(DeviceCycle(10_001)),
            JobScheduleAction::None
        );
        let mut faulted = schedule;
        assert_eq!(
            faulted.fault_execution().unwrap().fault,
            JobScheduleFault::Execution
        );
        assert_eq!(
            schedule
                .record_start_observation(start_observation(3))
                .unwrap()
                .start_observation,
            Some(start_observation(3))
        );
        assert_eq!(
            schedule.complete(DeviceCycle(15_000)).unwrap().state,
            JobScheduleState::Complete
        );
        let encoded = schedule.report().encode().unwrap();
        assert_eq!(
            JobScheduleReport::decode(&encoded)
                .unwrap()
                .start_observation,
            Some(start_observation(3))
        );
    }

    #[test]
    fn start_observation_is_correlated_and_tolerance_fault_is_retained() {
        let commit = commit();
        let mut schedule = PreparedJobSchedule::prepare::<3>(boot(0x66), descriptor()).unwrap();
        schedule.install(commit, admission(1_000)).unwrap();
        let confirm =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit).unwrap();
        schedule.confirm(confirm, DeviceCycle(7_000)).unwrap();
        assert!(matches!(
            schedule.advance(commit.abort_guard_cycle),
            JobScheduleAction::PrimeHardware { .. }
        ));
        schedule
            .mark_primed(DeviceCycle(commit.abort_guard_cycle.0 + 1))
            .unwrap();
        assert!(matches!(
            schedule.advance(commit.local_start_cycle),
            JobScheduleAction::Start { .. }
        ));

        let late = start_observation(commit.required_sync_tolerance_cycles + 1);
        let report = schedule.record_start_observation(late).unwrap();
        assert_eq!(report.state, JobScheduleState::Faulted);
        assert_eq!(report.fault, JobScheduleFault::StartObservation);
        assert_eq!(report.start_observation, Some(late));
        assert_eq!(
            JobScheduleReport::decode(&report.encode().unwrap()),
            Ok(report)
        );
        assert_eq!(schedule.record_start_observation(late), Ok(report));

        let mut conflicting = late;
        conflicting.output_token += 1;
        assert_eq!(
            schedule.record_start_observation(conflicting),
            Err(JobScheduleError::Conflict)
        );
    }

    #[test]
    fn local_safety_stop_is_canonical_before_and_after_start() {
        let mut prepared = PreparedJobSchedule::prepare::<3>(boot(0x66), descriptor()).unwrap();
        let report = prepared.fault_safety_stop().unwrap();
        assert_eq!(report.state, JobScheduleState::Faulted);
        assert_eq!(report.fault, JobScheduleFault::SafetyStop);
        assert!(!report.start_emitted);
        assert_eq!(
            JobScheduleReport::decode(&report.encode().unwrap()),
            Ok(report)
        );
        assert_eq!(prepared.fault_safety_stop(), Err(JobScheduleError::State));

        let commit = commit();
        let mut running = PreparedJobSchedule::prepare::<3>(boot(0x66), descriptor()).unwrap();
        running.install(commit, admission(1_000)).unwrap();
        let confirm =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit).unwrap();
        running.confirm(confirm, DeviceCycle(7_000)).unwrap();
        assert!(matches!(
            running.advance(commit.abort_guard_cycle),
            JobScheduleAction::PrimeHardware { .. }
        ));
        running
            .mark_primed(DeviceCycle(commit.abort_guard_cycle.0 + 1))
            .unwrap();
        assert!(matches!(
            running.advance(commit.local_start_cycle),
            JobScheduleAction::Start { .. }
        ));
        let report = running.fault_safety_stop().unwrap();
        assert_eq!(report.fault, JobScheduleFault::SafetyStop);
        assert!(report.start_emitted);
        assert_eq!(
            JobScheduleReport::decode(&report.encode().unwrap()),
            Ok(report)
        );
    }

    #[test]
    fn missing_confirm_and_guarded_abort_fail_closed() {
        let commit = commit();
        let mut expired = PreparedJobSchedule::prepare::<3>(boot(0x66), descriptor()).unwrap();
        expired.install(commit, admission(1_000)).unwrap();
        assert_eq!(
            expired.advance(commit.confirm_deadline_cycle),
            JobScheduleAction::AbortUnconfirmed
        );
        assert_eq!(expired.report().state, JobScheduleState::Expired);

        let abort =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Abort, commit).unwrap();
        assert_eq!(
            expired.abort(abort, DeviceCycle(8_500)).unwrap().state,
            JobScheduleState::Expired
        );

        let mut guarded = PreparedJobSchedule::prepare::<3>(boot(0x66), descriptor()).unwrap();
        guarded.install(commit, admission(1_000)).unwrap();
        assert_eq!(
            guarded.abort(abort, commit.abort_guard_cycle),
            Err(JobScheduleError::Deadline)
        );
        assert_eq!(guarded.report().state, JobScheduleState::Installed);
    }

    #[test]
    fn missed_start_and_expired_lease_have_distinct_canonical_faults() {
        let commit = commit();
        let confirm =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit).unwrap();
        let mut missed = PreparedJobSchedule::prepare::<3>(boot(0x66), descriptor()).unwrap();
        missed.install(commit, admission(1_000)).unwrap();
        missed.confirm(confirm, DeviceCycle(7_000)).unwrap();
        assert_eq!(
            missed.advance(DeviceCycle(
                commit.local_start_cycle.0 + commit.required_sync_tolerance_cycles + 1
            )),
            JobScheduleAction::MissedStart
        );
        let missed_report = missed.report();
        assert!(!missed_report.start_emitted);
        assert_eq!(
            JobScheduleReport::decode(&missed_report.encode().unwrap()),
            Ok(missed_report)
        );

        let mut leased = PreparedJobSchedule::prepare::<3>(boot(0x66), descriptor()).unwrap();
        leased.install(commit, admission(1_000)).unwrap();
        leased.confirm(confirm, DeviceCycle(7_000)).unwrap();
        assert!(matches!(
            leased.advance(commit.abort_guard_cycle),
            JobScheduleAction::PrimeHardware { .. }
        ));
        leased
            .mark_primed(DeviceCycle(commit.abort_guard_cycle.0 + 1))
            .unwrap();
        assert!(matches!(
            leased.advance(commit.local_start_cycle),
            JobScheduleAction::Start { .. }
        ));
        assert_eq!(
            leased.advance(commit.lease_expiry_cycle),
            JobScheduleAction::LeaseExpired
        );
        let lease_report = leased.report();
        assert!(lease_report.start_emitted);
        assert_eq!(
            JobScheduleReport::decode(&lease_report.encode().unwrap()),
            Ok(lease_report)
        );
    }

    #[test]
    fn hardware_priming_is_irrevocable_bounded_and_required_before_start() {
        let commit = commit();
        let confirm =
            JobScheduleReference::for_commit(JobScheduleReferenceAction::Confirm, commit).unwrap();
        let mut schedule = PreparedJobSchedule::prepare::<3>(boot(0x66), descriptor()).unwrap();
        schedule.install(commit, admission(1_000)).unwrap();
        assert_eq!(
            schedule.mark_primed(DeviceCycle(8_999)),
            Err(JobScheduleError::State)
        );
        schedule.confirm(confirm, DeviceCycle(7_000)).unwrap();
        assert_eq!(
            schedule.advance(commit.abort_guard_cycle),
            JobScheduleAction::PrimeHardware {
                scheduled_cycle: commit.local_start_cycle,
                lease_expiry_cycle: commit.lease_expiry_cycle,
            }
        );

        let mut late_acknowledgement = schedule;
        assert_eq!(
            late_acknowledgement.mark_primed(commit.local_start_cycle),
            Err(JobScheduleError::Deadline)
        );
        assert_eq!(
            late_acknowledgement.report().fault,
            JobScheduleFault::MissedStart
        );
        assert_eq!(
            schedule.advance(commit.local_start_cycle),
            JobScheduleAction::MissedStart
        );
        assert!(!schedule.report().start_emitted);
    }

    #[test]
    fn identity_readiness_policy_and_deadline_reject_without_installing() {
        let base = PreparedJobSchedule::prepare::<3>(boot(0x66), descriptor()).unwrap();
        let mut schedule = base;
        let mut wrong = commit();
        wrong.boot_id = boot(0x67);
        assert_eq!(
            schedule.install(wrong, admission(1_000)),
            Err(JobScheduleError::Identity)
        );
        assert_eq!(schedule, base);

        let mut not_ready = admission(1_000);
        not_ready.cache_ready = false;
        assert_eq!(
            schedule.install(commit(), not_ready),
            Err(JobScheduleError::Readiness)
        );
        assert_eq!(schedule, base);

        let mut autonomous = commit();
        autonomous.policy = JobNetworkPolicy::CachedAutonomous;
        assert_eq!(
            schedule.install(autonomous, admission(1_000)),
            Err(JobScheduleError::Policy)
        );
        assert_eq!(schedule, base);

        let mut insufficient_prime_lead = admission(1_000);
        insufficient_prime_lead.minimum_prime_lead_cycles = 1_001;
        assert_eq!(
            schedule.install(commit(), insufficient_prime_lead),
            Err(JobScheduleError::Deadline)
        );
        assert_eq!(schedule, base);

        assert_eq!(
            schedule.install(commit(), admission(9_500)),
            Err(JobScheduleError::Deadline)
        );
        assert_eq!(schedule, base);
    }
}
