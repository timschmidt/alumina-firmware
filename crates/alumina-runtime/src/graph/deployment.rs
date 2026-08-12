//! Independent fixed-memory graph-package admission and core-1 lifecycle.

use alumina_graph_ir::{
    CoreGraphAction, CoreGraphCommand, GRAPH_IR_PACKAGE_BYTES, GraphIrSummary, GraphPublication,
    MAX_CORE_GRAPH_DATA_BYTES, graph_ir_content_digest,
};
use alumina_protocol::{DeviceId, Digest};
use alumina_storage::media::{AsyncBlockDevice, MAX_MEDIA_CHUNK_BYTES, PublishedReader};
use alumina_storage::provisioning::{ProvisionedCache, ProvisionedCacheError};

use super::{
    GraphExecutionReport, GraphRuntimeAuthority, GraphRuntimeError, GraphRuntimeIdentity,
    GraphRuntimeLimits, GraphRuntimeUsage, admit_package,
};

/// Exact core-1 graph lifecycle report bytes.
pub const REALTIME_GRAPH_REPORT_BYTES: usize = 128;
/// Exact authenticated GraphGet and lifecycle response body.
pub const GRAPH_COORDINATOR_REPORT_BYTES: usize = 312;

const REALTIME_GRAPH_REPORT_MAGIC: [u8; 4] = *b"ALGR";
const GRAPH_COORDINATOR_REPORT_MAGIC: [u8; 4] = *b"ALGS";
const REALTIME_GRAPH_REPORT_VERSION: u16 = 1;
const GRAPH_COORDINATOR_REPORT_VERSION: u16 = 2;
const REPORT_FLAG_SUMMARY: u8 = 1 << 0;
const REPORT_FLAG_ACTIVE_AUTHORIZED: u8 = 1 << 1;
const REPORT_KNOWN_FLAGS: u8 = REPORT_FLAG_SUMMARY | REPORT_FLAG_ACTIVE_AUTHORIZED;

/// Sole core-0 graph coordinator phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GraphCoordinatorPhase {
    /// No candidate or active publication is retained.
    Empty = 0,
    /// Published bytes are being read, independently validated, and transferred.
    Validating = 1,
    /// Both cores independently admitted the exact candidate.
    CandidateValid = 2,
    /// Candidate selection command is crossing to core 1.
    Activating = 3,
    /// Service agreement is crossing after core-1 selection.
    Authorizing = 4,
    /// One exact graph package is selected and authorized.
    Active = 5,
    /// Active selection is being cleared.
    Clearing = 6,
    /// Candidate receipt or validation is being discarded.
    Aborting = 7,
    /// The latest service or realtime lifecycle operation failed closed.
    Rejected = 8,
    /// Core 0 primed tick zero and is awaiting core-1 run admission.
    Starting = 9,
    /// Both permanent actors admitted the exact run and may release.
    Running = 10,
    /// Both actors are reconciling stop acknowledgements for the exact run.
    Stopping = 11,
    /// A shared first-cause execution fault is retained until exact stop.
    ExecutionFaulted = 12,
    /// A committed graph selection is being reopened and independently admitted at boot.
    Recovering = 13,
    /// Activation intent is being made durable before core-1 selection.
    Preparing = 14,
    /// Core-1 selected the graph and the prepared durable selection is committing.
    Committing = 15,
}

impl GraphCoordinatorPhase {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Empty),
            1 => Some(Self::Validating),
            2 => Some(Self::CandidateValid),
            3 => Some(Self::Activating),
            4 => Some(Self::Authorizing),
            5 => Some(Self::Active),
            6 => Some(Self::Clearing),
            7 => Some(Self::Aborting),
            8 => Some(Self::Rejected),
            9 => Some(Self::Starting),
            10 => Some(Self::Running),
            11 => Some(Self::Stopping),
            12 => Some(Self::ExecutionFaulted),
            13 => Some(Self::Recovering),
            14 => Some(Self::Preparing),
            15 => Some(Self::Committing),
            _ => None,
        }
    }
}

/// Stable service-core graph lifecycle fault.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GraphCoordinatorFault {
    /// No fault is present.
    None = 0,
    /// Authenticated operation body or lifecycle selection was invalid.
    Request = 1,
    /// Published object lookup/readback failed.
    Storage = 2,
    /// Core-0 independent graph admission failed.
    ServiceValidation = 3,
    /// Inter-core framing or command encoding failed.
    Protocol = 4,
    /// Core-1 independently rejected the operation.
    Realtime = 5,
    /// Safety/job/configuration authority forbade mutation.
    ForbiddenState = 6,
    /// Fixed coordinator state contradicted itself.
    Internal = 7,
    /// A permanent graph actor latched a first-cause execution failure.
    Execution = 8,
    /// The durable graph-selection journal could not prepare, commit, abort, or replay.
    Durability = 9,
}

impl GraphCoordinatorFault {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Request),
            2 => Some(Self::Storage),
            3 => Some(Self::ServiceValidation),
            4 => Some(Self::Protocol),
            5 => Some(Self::Realtime),
            6 => Some(Self::ForbiddenState),
            7 => Some(Self::Internal),
            8 => Some(Self::Execution),
            9 => Some(Self::Durability),
            _ => None,
        }
    }
}

/// Canonical combined service/core-1 graph lifecycle observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphCoordinatorReport {
    /// Sole service-core phase.
    pub phase: GraphCoordinatorPhase,
    /// Service-core failure family.
    pub fault: GraphCoordinatorFault,
    /// Current candidate/operation transaction, or active transaction.
    pub transaction_id: u64,
    /// Current candidate/operation full content identity, or active identity.
    pub content_digest: Digest,
    /// Current candidate/operation embedded digest, or active identity.
    pub package_digest: Digest,
    /// Bytes independently validated by core 0.
    pub validated_bytes: u32,
    /// Complete published-storage chunks replayed by core 0.
    pub storage_chunks_read: u32,
    /// Active full content identity retained by core 0.
    pub active_content_digest: Digest,
    /// Latest independently decoded core-1 report.
    pub realtime: RealtimeGraphReport,
    /// Combined permanent-actor execution observation.
    pub execution: GraphExecutionReport,
}

impl GraphCoordinatorReport {
    /// Canonical boot state.
    pub const fn empty() -> Self {
        Self {
            phase: GraphCoordinatorPhase::Empty,
            fault: GraphCoordinatorFault::None,
            transaction_id: 0,
            content_digest: Digest::ZERO,
            package_digest: Digest::ZERO,
            validated_bytes: 0,
            storage_chunks_read: 0,
            active_content_digest: Digest::ZERO,
            realtime: RealtimeGraphReport::empty(),
            execution: GraphExecutionReport::empty(),
        }
    }

    /// Encode one exact fixed authenticated response body.
    pub fn encode(
        self,
    ) -> Result<[u8; GRAPH_COORDINATOR_REPORT_BYTES], GraphCoordinatorReportError> {
        self.validate()?;
        let mut encoded = [0_u8; GRAPH_COORDINATOR_REPORT_BYTES];
        encoded[..4].copy_from_slice(&GRAPH_COORDINATOR_REPORT_MAGIC);
        encoded[4..6].copy_from_slice(&GRAPH_COORDINATOR_REPORT_VERSION.to_le_bytes());
        encoded[6] = self.phase as u8;
        encoded[7] = self.fault as u8;
        encoded[8..16].copy_from_slice(&self.transaction_id.to_le_bytes());
        encoded[16..48].copy_from_slice(&self.content_digest.0);
        encoded[48..80].copy_from_slice(&self.package_digest.0);
        encoded[80..84].copy_from_slice(&self.validated_bytes.to_le_bytes());
        encoded[84..88].copy_from_slice(&self.storage_chunks_read.to_le_bytes());
        encoded[88..120].copy_from_slice(&self.active_content_digest.0);
        encoded[120..248].copy_from_slice(
            &self
                .realtime
                .encode()
                .map_err(GraphCoordinatorReportError::Realtime)?,
        );
        encoded[248..312].copy_from_slice(
            &self
                .execution
                .encode()
                .map_err(GraphCoordinatorReportError::Execution)?,
        );
        Ok(encoded)
    }

    /// Decode only the canonical fixed response representation.
    pub fn decode(encoded: &[u8]) -> Result<Self, GraphCoordinatorReportError> {
        if encoded.len() != GRAPH_COORDINATOR_REPORT_BYTES {
            return Err(GraphCoordinatorReportError::Length);
        }
        if encoded[..4] != GRAPH_COORDINATOR_REPORT_MAGIC {
            return Err(GraphCoordinatorReportError::Magic);
        }
        if read_u16(encoded, 4) != GRAPH_COORDINATOR_REPORT_VERSION {
            return Err(GraphCoordinatorReportError::Version);
        }
        let report = Self {
            phase: GraphCoordinatorPhase::from_wire(encoded[6])
                .ok_or(GraphCoordinatorReportError::Phase)?,
            fault: GraphCoordinatorFault::from_wire(encoded[7])
                .ok_or(GraphCoordinatorReportError::Fault)?,
            transaction_id: read_u64(encoded, 8),
            content_digest: Digest(array::<32>(encoded, 16)),
            package_digest: Digest(array::<32>(encoded, 48)),
            validated_bytes: read_u32(encoded, 80),
            storage_chunks_read: read_u32(encoded, 84),
            active_content_digest: Digest(array::<32>(encoded, 88)),
            realtime: RealtimeGraphReport::decode(&encoded[120..248])
                .map_err(GraphCoordinatorReportError::Realtime)?,
            execution: GraphExecutionReport::decode(&encoded[248..312])
                .map_err(GraphCoordinatorReportError::Execution)?,
        };
        report.validate()?;
        if report.encode()? != encoded {
            return Err(GraphCoordinatorReportError::Noncanonical);
        }
        Ok(report)
    }

    fn validate(self) -> Result<(), GraphCoordinatorReportError> {
        if self.validated_bytes > GRAPH_IR_PACKAGE_BYTES as u32 {
            return Err(GraphCoordinatorReportError::ValidatedBytes);
        }
        let operation_empty = self.transaction_id == 0
            && self.content_digest.is_zero()
            && self.package_digest.is_zero()
            && self.validated_bytes == 0
            && self.storage_chunks_read == 0;
        let operation_valid = self.transaction_id != 0
            && !self.content_digest.is_zero()
            && !self.package_digest.is_zero();
        if !operation_empty && !operation_valid {
            return Err(GraphCoordinatorReportError::Identity);
        }
        match self.phase {
            GraphCoordinatorPhase::Empty
                if operation_empty
                    && self.active_content_digest.is_zero()
                    && self.fault == GraphCoordinatorFault::None
                    && self.execution == GraphExecutionReport::empty() =>
            {
                Ok(())
            }
            GraphCoordinatorPhase::Active
                if operation_valid
                    && self.validated_bytes == GRAPH_IR_PACKAGE_BYTES as u32
                    && self.active_content_digest == self.content_digest
                    && self.fault == GraphCoordinatorFault::None
                    && self.realtime.state == RealtimeGraphState::Active
                    && self.realtime.active_authorized
                    && self.realtime.active_content_digest == self.active_content_digest =>
            {
                Ok(())
            }
            GraphCoordinatorPhase::Starting
            | GraphCoordinatorPhase::Running
            | GraphCoordinatorPhase::Stopping
                if operation_valid
                    && self.validated_bytes == GRAPH_IR_PACKAGE_BYTES as u32
                    && self.active_content_digest == self.content_digest
                    && self.fault == GraphCoordinatorFault::None
                    && self.realtime.state == RealtimeGraphState::Active
                    && self.realtime.active_authorized
                    && self.realtime.active_content_digest == self.active_content_digest
                    && self.execution.run_id != 0 =>
            {
                Ok(())
            }
            GraphCoordinatorPhase::ExecutionFaulted
                if operation_valid
                    && self.validated_bytes == GRAPH_IR_PACKAGE_BYTES as u32
                    && self.active_content_digest == self.content_digest
                    && self.fault != GraphCoordinatorFault::None
                    && self.realtime.state == RealtimeGraphState::Active
                    && self.realtime.active_authorized
                    && self.realtime.active_content_digest == self.active_content_digest
                    && self.execution.run_id != 0
                    && (self.fault != GraphCoordinatorFault::Execution
                        || self.execution.fault.is_some()) =>
            {
                Ok(())
            }
            GraphCoordinatorPhase::Rejected
                if (operation_valid || operation_empty)
                    && self.fault != GraphCoordinatorFault::None
                    && (operation_valid
                        || self.active_content_digest.is_zero()
                            && self.execution == GraphExecutionReport::empty()) =>
            {
                Ok(())
            }
            _ if self.phase != GraphCoordinatorPhase::Empty
                && self.phase != GraphCoordinatorPhase::Active
                && self.phase != GraphCoordinatorPhase::Rejected
                && self.phase != GraphCoordinatorPhase::Starting
                && self.phase != GraphCoordinatorPhase::Running
                && self.phase != GraphCoordinatorPhase::Stopping
                && self.phase != GraphCoordinatorPhase::ExecutionFaulted
                && operation_valid
                && self.fault == GraphCoordinatorFault::None =>
            {
                Ok(())
            }
            _ => Err(GraphCoordinatorReportError::StateShape),
        }
    }
}

/// Canonical combined graph status rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphCoordinatorReportError {
    /// Fixed response length differed.
    Length,
    /// Response magic differed.
    Magic,
    /// Response version differed.
    Version,
    /// Reserved bytes were nonzero.
    Reserved,
    /// Decode followed by encode changed the bytes.
    Noncanonical,
    /// Coordinator phase byte was unknown.
    Phase,
    /// Coordinator fault byte was unknown.
    Fault,
    /// Current operation identity was partial.
    Identity,
    /// Service validation exceeded the package.
    ValidatedBytes,
    /// Fields did not match the declared phase.
    StateShape,
    /// Nested core-1 report was invalid.
    Realtime(RealtimeGraphReportError),
    /// Nested split-core execution report was invalid.
    Execution(super::GraphExecutionReportError),
}

/// Core-1 graph candidate/active lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RealtimeGraphState {
    /// No candidate or active package exists.
    Empty = 0,
    /// Exact package bytes are arriving in order.
    Receiving = 1,
    /// Both graph syntax and concrete firmware admission passed.
    CandidateValid = 2,
    /// One package is selected, with execution separately gated by authorization.
    Active = 3,
    /// The latest operation failed closed; a prior active package may remain retained.
    Rejected = 4,
    /// The selected active package was explicitly cleared.
    Cleared = 5,
}

impl RealtimeGraphState {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Empty),
            1 => Some(Self::Receiving),
            2 => Some(Self::CandidateValid),
            3 => Some(Self::Active),
            4 => Some(Self::Rejected),
            5 => Some(Self::Cleared),
            _ => None,
        }
    }
}

/// Stable graph admission failure suitable for telemetry and HIL assertions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GraphDeploymentFault {
    /// No failure is present.
    None = 0,
    /// Transaction, action, or byte offset arrived out of order.
    Sequence = 1,
    /// Canonical package syntax or fixed executor shape was invalid.
    Syntax = 2,
    /// Full stored-object SHA-256 differed from the requested publication.
    ContentDigest = 3,
    /// Embedded package digest differed from the authenticated request.
    PackageDigest = 4,
    /// Package targeted a different physical MCU.
    Device = 5,
    /// Package targeted a different board capability ledger.
    Capability = 6,
    /// Package targeted a different active machine configuration.
    Configuration = 7,
    /// Package selected a different implementation-registry identity.
    Implementation = 8,
    /// One fixed arena exceeded the image's compile-time reservation.
    Capacity = 9,
    /// Current safety, job, or queue ownership forbids mutation.
    ForbiddenState = 10,
    /// Checked package or arena arithmetic failed.
    Arithmetic = 11,
}

impl GraphDeploymentFault {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Sequence),
            2 => Some(Self::Syntax),
            3 => Some(Self::ContentDigest),
            4 => Some(Self::PackageDigest),
            5 => Some(Self::Device),
            6 => Some(Self::Capability),
            7 => Some(Self::Configuration),
            8 => Some(Self::Implementation),
            9 => Some(Self::Capacity),
            10 => Some(Self::ForbiddenState),
            11 => Some(Self::Arithmetic),
            _ => None,
        }
    }

    pub(super) const fn from_runtime(error: GraphRuntimeError) -> Self {
        match error {
            GraphRuntimeError::MutationForbidden | GraphRuntimeError::ExecutionForbidden => {
                Self::ForbiddenState
            }
            GraphRuntimeError::MissingExpectedDigest | GraphRuntimeError::Package(_) => {
                Self::Syntax
            }
            GraphRuntimeError::PackageDigest { .. } => Self::PackageDigest,
            GraphRuntimeError::Identity(GraphRuntimeIdentity::Device) => Self::Device,
            GraphRuntimeError::Identity(GraphRuntimeIdentity::Capability) => Self::Capability,
            GraphRuntimeError::Identity(GraphRuntimeIdentity::Configuration) => Self::Configuration,
            GraphRuntimeError::Identity(GraphRuntimeIdentity::Implementation) => {
                Self::Implementation
            }
            GraphRuntimeError::OpcodeCapability { .. }
            | GraphRuntimeError::ResourceCapability { .. } => Self::Capability,
            GraphRuntimeError::Capacity { .. } => Self::Capacity,
            GraphRuntimeError::Arithmetic => Self::Arithmetic,
            GraphRuntimeError::Phase { .. }
            | GraphRuntimeError::RuntimeShape
            | GraphRuntimeError::Execution(_) => Self::Syntax,
        }
    }
}

/// Small exact identity retained for a validated candidate or active package.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphDeploymentIdentity {
    /// Boot-local install transaction.
    pub transaction_id: u64,
    /// Full stored-object SHA-256 identity.
    pub content_digest: Digest,
    /// Embedded canonical package digest.
    pub package_digest: Digest,
    /// Graph-bound implementation-registry identity.
    pub implementation_digest: Digest,
    /// Source graph document identity.
    pub graph_digest: Digest,
    /// Independently reconstructed node/channel totals.
    pub summary: GraphIrSummary,
    /// Exact selected arena use.
    pub usage: GraphRuntimeUsage,
}

/// Independently decode one complete package and enforce exact runtime authority
/// and compile-time arena limits without retaining or executing it.
pub fn validate_graph_package(
    bytes: &[u8],
    transaction_id: u64,
    content_digest: Digest,
    package_digest: Digest,
    authority: GraphRuntimeAuthority,
    limits: GraphRuntimeLimits,
) -> Result<GraphDeploymentIdentity, GraphDeploymentFault> {
    if transaction_id == 0 || content_digest.is_zero() {
        return Err(GraphDeploymentFault::Sequence);
    }
    if graph_ir_content_digest(bytes) != content_digest {
        return Err(GraphDeploymentFault::ContentDigest);
    }
    let (package, _, usage) = admit_package(bytes, package_digest, authority, limits)
        .map_err(GraphDeploymentFault::from_runtime)?;
    Ok(GraphDeploymentIdentity {
        transaction_id,
        content_digest,
        package_digest,
        implementation_digest: authority.implementation_digest,
        graph_digest: package.header().graph_digest,
        summary: package.summary(),
        usage,
    })
}

/// Core-0 state of one immutable graph publication transfer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ServiceGraphValidationState {
    /// Begin has not yet crossed the inter-core queue.
    Begin = 1,
    /// Published bytes are being read, validated, and copied into commands.
    Streaming = 2,
    /// Independent core-0 validation passed; Finish remains to be sent.
    Finish = 3,
    /// Every package byte and lifecycle command was produced.
    Complete = 4,
    /// Media, syntax, identity, capacity, or sequencing failed closed.
    Faulted = 5,
}

/// Fixed progress safe to expose from the sole service-core reader.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceGraphValidationStatus {
    /// Current transfer phase.
    pub state: ServiceGraphValidationState,
    /// Authenticated install transaction.
    pub transaction_id: u64,
    /// Full stored-object SHA-256.
    pub content_digest: Digest,
    /// Embedded package digest.
    pub package_digest: Digest,
    /// Package bytes independently validated on core 0.
    pub validated_bytes: u32,
    /// Complete storage chunks read through the publication cursor.
    pub storage_chunks_read: u32,
    /// Independently admitted candidate identity after all bytes validate.
    pub identity: Option<GraphDeploymentIdentity>,
}

/// Sole core-0 owner of a verified publication cursor and fixed package buffer.
pub struct ServiceGraphValidation {
    publication: GraphPublication,
    authority: GraphRuntimeAuthority,
    limits: GraphRuntimeLimits,
    reader: PublishedReader,
    storage: [u8; MAX_MEDIA_CHUNK_BYTES],
    storage_offset: usize,
    storage_len: usize,
    package: [u8; GRAPH_IR_PACKAGE_BYTES],
    package_offset: u32,
    storage_chunks_read: u32,
    state: ServiceGraphValidationState,
    identity: Option<GraphDeploymentIdentity>,
}

impl ServiceGraphValidation {
    /// Open the exact typed immutable publication without emitting a command.
    pub async fn open<D>(
        cache: &mut ProvisionedCache<D>,
        publication: GraphPublication,
        authority: GraphRuntimeAuthority,
        limits: GraphRuntimeLimits,
    ) -> Result<Self, ServiceGraphTransferError<D::Error>>
    where
        D: AsyncBlockDevice,
    {
        publication
            .validate()
            .map_err(ServiceGraphTransferError::Request)?;
        if publication.implementation_digest != authority.implementation_digest {
            return Err(ServiceGraphTransferError::Validation(
                GraphDeploymentFault::Implementation,
            ));
        }
        let reader = cache
            .open_published(publication.publication)
            .await
            .map_err(ServiceGraphTransferError::Storage)?;
        Ok(Self {
            publication,
            authority,
            limits,
            reader,
            storage: [0; MAX_MEDIA_CHUNK_BYTES],
            storage_offset: 0,
            storage_len: 0,
            package: [0; GRAPH_IR_PACKAGE_BYTES],
            package_offset: 0,
            storage_chunks_read: 0,
            state: ServiceGraphValidationState::Begin,
            identity: None,
        })
    }

    /// Advance at most one SD read and return at most one ordered core command.
    pub async fn next<D>(
        &mut self,
        cache: &mut ProvisionedCache<D>,
    ) -> Result<Option<CoreGraphCommand>, ServiceGraphTransferError<D::Error>>
    where
        D: AsyncBlockDevice,
    {
        let result = self.next_inner(cache).await;
        if result.is_err() {
            self.state = ServiceGraphValidationState::Faulted;
            self.identity = None;
            self.storage.fill(0);
            self.package.fill(0);
            self.storage_offset = 0;
            self.storage_len = 0;
        }
        result
    }

    /// Current progress without media I/O.
    pub const fn status(&self) -> ServiceGraphValidationStatus {
        ServiceGraphValidationStatus {
            state: self.state,
            transaction_id: self.publication.transaction_id,
            content_digest: self.publication.publication.object.content.digest,
            package_digest: self.publication.package_digest,
            validated_bytes: self.package_offset,
            storage_chunks_read: self.storage_chunks_read,
            identity: self.identity,
        }
    }

    /// Exact independently validated package bytes after the complete transfer
    /// command has been produced.
    ///
    /// These bytes remain core-0-owned and are suitable for a second admission
    /// into its permanent executor actor. Their presence alone grants no start
    /// or execution authority.
    pub fn validated_package_bytes(&self) -> Option<&[u8; GRAPH_IR_PACKAGE_BYTES]> {
        if self.state == ServiceGraphValidationState::Complete && self.identity.is_some() {
            Some(&self.package)
        } else {
            None
        }
    }

    async fn next_inner<D>(
        &mut self,
        cache: &mut ProvisionedCache<D>,
    ) -> Result<Option<CoreGraphCommand>, ServiceGraphTransferError<D::Error>>
    where
        D: AsyncBlockDevice,
    {
        loop {
            match self.state {
                ServiceGraphValidationState::Begin => {
                    self.state = ServiceGraphValidationState::Streaming;
                    return CoreGraphCommand::begin(self.publication)
                        .map(Some)
                        .map_err(ServiceGraphTransferError::Request);
                }
                ServiceGraphValidationState::Streaming => {
                    if self.storage_offset < self.storage_len {
                        let count =
                            (self.storage_len - self.storage_offset).min(MAX_CORE_GRAPH_DATA_BYTES);
                        let storage_end = self
                            .storage_offset
                            .checked_add(count)
                            .ok_or(ServiceGraphTransferError::State)?;
                        let package_start = usize::try_from(self.package_offset)
                            .map_err(|_| ServiceGraphTransferError::State)?;
                        let package_end = package_start
                            .checked_add(count)
                            .ok_or(ServiceGraphTransferError::State)?;
                        let bytes = &self.storage[self.storage_offset..storage_end];
                        self.package
                            .get_mut(package_start..package_end)
                            .ok_or(ServiceGraphTransferError::State)?
                            .copy_from_slice(bytes);
                        let command =
                            CoreGraphCommand::data(self.publication, self.package_offset, bytes)
                                .map_err(ServiceGraphTransferError::Request)?;
                        self.storage_offset = storage_end;
                        self.package_offset = u32::try_from(package_end)
                            .map_err(|_| ServiceGraphTransferError::State)?;
                        return Ok(Some(command));
                    }

                    self.storage.fill(0);
                    self.storage_offset = 0;
                    self.storage_len = 0;
                    if self.reader.is_complete() {
                        if self.package_offset != GRAPH_IR_PACKAGE_BYTES as u32 {
                            return Err(ServiceGraphTransferError::State);
                        }
                        self.identity = Some(
                            validate_graph_package(
                                &self.package,
                                self.publication.transaction_id,
                                self.publication.content_digest(),
                                self.publication.package_digest,
                                self.authority,
                                self.limits,
                            )
                            .map_err(ServiceGraphTransferError::Validation)?,
                        );
                        self.state = ServiceGraphValidationState::Finish;
                        continue;
                    }
                    let chunk = cache
                        .read_next_published(&mut self.reader, &mut self.storage)
                        .await
                        .map_err(ServiceGraphTransferError::Storage)?
                        .ok_or(ServiceGraphTransferError::State)?;
                    self.storage_len = usize::try_from(chunk.byte_len)
                        .map_err(|_| ServiceGraphTransferError::State)?;
                    if self.storage_len == 0 || self.storage_len > self.storage.len() {
                        return Err(ServiceGraphTransferError::State);
                    }
                    self.storage_chunks_read = self
                        .storage_chunks_read
                        .checked_add(1)
                        .ok_or(ServiceGraphTransferError::State)?;
                }
                ServiceGraphValidationState::Finish => {
                    self.state = ServiceGraphValidationState::Complete;
                    return CoreGraphCommand::finish(self.publication)
                        .map(Some)
                        .map_err(ServiceGraphTransferError::Request);
                }
                ServiceGraphValidationState::Complete => return Ok(None),
                ServiceGraphValidationState::Faulted => {
                    return Err(ServiceGraphTransferError::State);
                }
            }
        }
    }
}

/// Published-media, canonical request, independent admission, or state failure.
#[derive(Debug)]
pub enum ServiceGraphTransferError<E> {
    /// Authenticated request or inter-core command was malformed.
    Request(alumina_graph_ir::GraphDeploymentWireError),
    /// Core-0 independent graph/runtime admission failed.
    Validation(GraphDeploymentFault),
    /// Verified publication lookup or read failed.
    Storage(ProvisionedCacheError<E>),
    /// Internal byte counts or lifecycle state contradicted the fixed package.
    State,
}

/// Canonical core-1 status returned periodically and after each command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeGraphReport {
    /// Current receiver/selection state.
    pub state: RealtimeGraphState,
    /// Latest operation or selected active transaction.
    pub transaction_id: u64,
    /// Latest operation or selected active full content identity.
    pub content_digest: Digest,
    /// Latest operation or selected active embedded package identity.
    pub package_digest: Digest,
    /// Contiguous bytes consumed for the latest operation.
    pub consumed_bytes: u32,
    /// Independently reconstructed totals when a candidate/active package is valid.
    pub summary: Option<GraphIrSummary>,
    /// Stable fail-closed reason.
    pub fault: GraphDeploymentFault,
    /// Retained active full content identity, including during a rejected replacement.
    pub active_content_digest: Digest,
    /// Execution may be exposed only after service-core agreement.
    pub active_authorized: bool,
}

impl RealtimeGraphReport {
    /// Canonical boot state.
    pub const fn empty() -> Self {
        Self {
            state: RealtimeGraphState::Empty,
            transaction_id: 0,
            content_digest: Digest::ZERO,
            package_digest: Digest::ZERO,
            consumed_bytes: 0,
            summary: None,
            fault: GraphDeploymentFault::None,
            active_content_digest: Digest::ZERO,
            active_authorized: false,
        }
    }

    /// Encode one exact fixed telemetry payload.
    pub fn encode(self) -> Result<[u8; REALTIME_GRAPH_REPORT_BYTES], RealtimeGraphReportError> {
        self.validate()?;
        let mut encoded = [0_u8; REALTIME_GRAPH_REPORT_BYTES];
        encoded[..4].copy_from_slice(&REALTIME_GRAPH_REPORT_MAGIC);
        encoded[4..6].copy_from_slice(&REALTIME_GRAPH_REPORT_VERSION.to_le_bytes());
        encoded[6] = self.state as u8;
        encoded[7] = (u8::from(self.summary.is_some()) * REPORT_FLAG_SUMMARY)
            | (u8::from(self.active_authorized) * REPORT_FLAG_ACTIVE_AUTHORIZED);
        encoded[8..16].copy_from_slice(&self.transaction_id.to_le_bytes());
        encoded[16..48].copy_from_slice(&self.content_digest.0);
        encoded[48..80].copy_from_slice(&self.package_digest.0);
        encoded[80..84].copy_from_slice(&self.consumed_bytes.to_le_bytes());
        if let Some(summary) = self.summary {
            encoded[84..86].copy_from_slice(&summary.node_count.to_le_bytes());
            encoded[86..88].copy_from_slice(&summary.channel_count.to_le_bytes());
            encoded[88..90].copy_from_slice(&summary.bridge_count.to_le_bytes());
        }
        encoded[90] = self.fault as u8;
        // Byte 91 and bytes 124..128 are reserved zero.
        encoded[92..124].copy_from_slice(&self.active_content_digest.0);
        Ok(encoded)
    }

    /// Decode only the canonical fixed telemetry representation.
    pub fn decode(encoded: &[u8]) -> Result<Self, RealtimeGraphReportError> {
        if encoded.len() != REALTIME_GRAPH_REPORT_BYTES {
            return Err(RealtimeGraphReportError::Length);
        }
        if encoded[..4] != REALTIME_GRAPH_REPORT_MAGIC {
            return Err(RealtimeGraphReportError::Magic);
        }
        if read_u16(encoded, 4) != REALTIME_GRAPH_REPORT_VERSION {
            return Err(RealtimeGraphReportError::Version);
        }
        let flags = encoded[7];
        if flags & !REPORT_KNOWN_FLAGS != 0
            || encoded[91] != 0
            || encoded[124..].iter().any(|byte| *byte != 0)
        {
            return Err(RealtimeGraphReportError::Reserved);
        }
        let summary = if flags & REPORT_FLAG_SUMMARY != 0 {
            Some(GraphIrSummary {
                node_count: read_u16(encoded, 84),
                channel_count: read_u16(encoded, 86),
                bridge_count: read_u16(encoded, 88),
                service_state_bytes: 0,
                realtime_state_bytes: 0,
                channel_storage_bytes: 0,
                bridge_storage_bytes: 0,
            })
        } else {
            if encoded[84..90].iter().any(|byte| *byte != 0) {
                return Err(RealtimeGraphReportError::Reserved);
            }
            None
        };
        let report = Self {
            state: RealtimeGraphState::from_wire(encoded[6])
                .ok_or(RealtimeGraphReportError::State)?,
            transaction_id: read_u64(encoded, 8),
            content_digest: Digest(array::<32>(encoded, 16)),
            package_digest: Digest(array::<32>(encoded, 48)),
            consumed_bytes: read_u32(encoded, 80),
            summary,
            fault: GraphDeploymentFault::from_wire(encoded[90])
                .ok_or(RealtimeGraphReportError::Fault)?,
            active_content_digest: Digest(array::<32>(encoded, 92)),
            active_authorized: flags & REPORT_FLAG_ACTIVE_AUTHORIZED != 0,
        };
        report.validate()?;
        if report.encode()? != encoded {
            return Err(RealtimeGraphReportError::Noncanonical);
        }
        Ok(report)
    }

    fn validate(self) -> Result<(), RealtimeGraphReportError> {
        if self.consumed_bytes > GRAPH_IR_PACKAGE_BYTES as u32 {
            return Err(RealtimeGraphReportError::ConsumedBytes);
        }
        if self.summary.is_some_and(|summary| {
            summary.node_count == 0
                || summary.bridge_count > summary.channel_count
                || summary.service_state_bytes != 0
                || summary.realtime_state_bytes != 0
                || summary.channel_storage_bytes != 0
                || summary.bridge_storage_bytes != 0
        }) {
            return Err(RealtimeGraphReportError::Summary);
        }
        let operation_empty = self.transaction_id == 0
            && self.content_digest.is_zero()
            && self.package_digest.is_zero()
            && self.consumed_bytes == 0;
        let operation_valid = self.transaction_id != 0
            && !self.content_digest.is_zero()
            && !self.package_digest.is_zero();
        if !operation_empty && !operation_valid {
            return Err(RealtimeGraphReportError::Identity);
        }
        if self.active_authorized && self.active_content_digest.is_zero() {
            return Err(RealtimeGraphReportError::Authorization);
        }
        match self.state {
            RealtimeGraphState::Empty
                if operation_empty
                    && self.summary.is_none()
                    && self.fault == GraphDeploymentFault::None
                    && self.active_content_digest.is_zero()
                    && !self.active_authorized =>
            {
                Ok(())
            }
            RealtimeGraphState::Receiving
                if operation_valid
                    && self.summary.is_none()
                    && self.fault == GraphDeploymentFault::None =>
            {
                Ok(())
            }
            RealtimeGraphState::CandidateValid
                if operation_valid
                    && self.consumed_bytes == GRAPH_IR_PACKAGE_BYTES as u32
                    && self.summary.is_some()
                    && self.fault == GraphDeploymentFault::None =>
            {
                Ok(())
            }
            RealtimeGraphState::Active
                if operation_valid
                    && self.consumed_bytes == GRAPH_IR_PACKAGE_BYTES as u32
                    && self.summary.is_some()
                    && self.fault == GraphDeploymentFault::None
                    && self.active_content_digest == self.content_digest =>
            {
                Ok(())
            }
            RealtimeGraphState::Rejected
                if operation_valid
                    && self.summary.is_none()
                    && self.fault != GraphDeploymentFault::None =>
            {
                Ok(())
            }
            RealtimeGraphState::Cleared
                if operation_valid
                    && self.consumed_bytes == GRAPH_IR_PACKAGE_BYTES as u32
                    && self.summary.is_none()
                    && self.fault == GraphDeploymentFault::None
                    && self.active_content_digest.is_zero()
                    && !self.active_authorized =>
            {
                Ok(())
            }
            _ => Err(RealtimeGraphReportError::StateShape),
        }
    }
}

/// Canonical realtime graph report rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealtimeGraphReportError {
    /// Fixed report length differed.
    Length,
    /// Report magic differed.
    Magic,
    /// Report version differed.
    Version,
    /// Unknown flag or reserved byte was nonzero.
    Reserved,
    /// Decode followed by encode changed the bytes.
    Noncanonical,
    /// State byte was unknown.
    State,
    /// Fault byte was unknown.
    Fault,
    /// Operation identity was partial.
    Identity,
    /// Consumed bytes exceeded the package.
    ConsumedBytes,
    /// Summary counters were impossible or contained non-transmitted fields.
    Summary,
    /// Authorization was asserted without an active identity.
    Authorization,
    /// Fields did not match the declared lifecycle state.
    StateShape,
}

/// Core-1 owner of package staging, independent admission, and active bytes.
///
/// Candidate staging and active storage are distinct fixed arrays. Receiving a
/// replacement therefore cannot mutate the exact active image.
pub struct RealtimeGraphDeployment {
    device_id: DeviceId,
    capability_digest: Digest,
    limits: GraphRuntimeLimits,
    receiving: bool,
    transaction_id: u64,
    content_digest: Digest,
    package_digest: Digest,
    implementation_digest: Digest,
    consumed_bytes: u32,
    staging: [u8; GRAPH_IR_PACKAGE_BYTES],
    candidate: Option<GraphDeploymentIdentity>,
    active_bytes: [u8; GRAPH_IR_PACKAGE_BYTES],
    active: Option<GraphDeploymentIdentity>,
    active_authorized: bool,
    cleared: bool,
    last_fault: GraphDeploymentFault,
}

impl RealtimeGraphDeployment {
    /// Create empty fixed storage bound to this physical target and arena budget.
    pub const fn new(
        device_id: DeviceId,
        capability_digest: Digest,
        limits: GraphRuntimeLimits,
    ) -> Self {
        Self {
            device_id,
            capability_digest,
            limits,
            receiving: false,
            transaction_id: 0,
            content_digest: Digest::ZERO,
            package_digest: Digest::ZERO,
            implementation_digest: Digest::ZERO,
            consumed_bytes: 0,
            staging: [0; GRAPH_IR_PACKAGE_BYTES],
            candidate: None,
            active_bytes: [0; GRAPH_IR_PACKAGE_BYTES],
            active: None,
            active_authorized: false,
            cleared: false,
            last_fault: GraphDeploymentFault::None,
        }
    }

    /// Stable target MCU identity bound at construction.
    pub const fn device_id(&self) -> DeviceId {
        self.device_id
    }

    /// Apply one ordered command using independently supplied current authority.
    pub fn apply(
        &mut self,
        command: CoreGraphCommand,
        authority: GraphRuntimeAuthority,
        mutation_allowed: bool,
    ) -> RealtimeGraphReport {
        if !mutation_allowed {
            return self.reject(command, GraphDeploymentFault::ForbiddenState);
        }
        if let Err(fault) = self.validate_authority(command, authority) {
            return self.reject(command, fault);
        }
        match command.action {
            CoreGraphAction::Begin => {
                self.staging.fill(0);
                self.set_operation(command);
                self.consumed_bytes = 0;
                self.receiving = true;
                self.candidate = None;
                self.cleared = false;
                self.last_fault = GraphDeploymentFault::None;
                self.report()
            }
            CoreGraphAction::Data => {
                if !self.receiving
                    || !self.matches(command)
                    || command.offset != self.consumed_bytes
                {
                    return self.reject(command, GraphDeploymentFault::Sequence);
                }
                let start = match usize::try_from(command.offset) {
                    Ok(start) => start,
                    Err(_) => return self.reject(command, GraphDeploymentFault::Arithmetic),
                };
                let end = match start.checked_add(command.data_bytes().len()) {
                    Some(end) => end,
                    None => return self.reject(command, GraphDeploymentFault::Arithmetic),
                };
                let Some(destination) = self.staging.get_mut(start..end) else {
                    return self.reject(command, GraphDeploymentFault::Sequence);
                };
                destination.copy_from_slice(command.data_bytes());
                self.consumed_bytes = match u32::try_from(end) {
                    Ok(consumed) => consumed,
                    Err(_) => return self.reject(command, GraphDeploymentFault::Arithmetic),
                };
                self.report()
            }
            CoreGraphAction::Finish => {
                if !self.receiving
                    || !self.matches(command)
                    || self.consumed_bytes != GRAPH_IR_PACKAGE_BYTES as u32
                {
                    return self.reject(command, GraphDeploymentFault::Sequence);
                }
                let candidate = validate_graph_package(
                    &self.staging,
                    command.transaction_id,
                    command.content_digest,
                    command.package_digest,
                    authority,
                    self.limits,
                );
                let candidate = match candidate {
                    Ok(candidate) => candidate,
                    Err(fault) => {
                        return self.reject(command, fault);
                    }
                };
                self.receiving = false;
                self.candidate = Some(candidate);
                self.last_fault = GraphDeploymentFault::None;
                self.report()
            }
            CoreGraphAction::Activate => {
                if !self.matches(command)
                    || self
                        .candidate
                        .is_none_or(|candidate| !identity_matches(candidate, command))
                {
                    return self.reject(command, GraphDeploymentFault::Sequence);
                }
                self.active_bytes.copy_from_slice(&self.staging);
                self.active = self.candidate.take();
                self.active_authorized = false;
                self.receiving = false;
                self.cleared = false;
                self.last_fault = GraphDeploymentFault::None;
                self.report()
            }
            CoreGraphAction::Clear => {
                if self
                    .active
                    .is_none_or(|active| !identity_matches(active, command))
                {
                    return self.reject(command, GraphDeploymentFault::Sequence);
                }
                self.set_operation(command);
                self.consumed_bytes = GRAPH_IR_PACKAGE_BYTES as u32;
                self.receiving = false;
                self.candidate = None;
                self.active_bytes.fill(0);
                self.active = None;
                self.active_authorized = false;
                self.cleared = true;
                self.last_fault = GraphDeploymentFault::None;
                self.report()
            }
            CoreGraphAction::Abort => {
                if !self.matches(command)
                    || (!self.receiving
                        && self.candidate.is_none()
                        && self.last_fault == GraphDeploymentFault::None)
                {
                    return self.reject(command, GraphDeploymentFault::Sequence);
                }
                self.receiving = false;
                self.candidate = None;
                self.staging.fill(0);
                self.cleared = false;
                self.last_fault = GraphDeploymentFault::None;
                if let Some(active) = self.active {
                    self.set_identity(active);
                    self.consumed_bytes = GRAPH_IR_PACKAGE_BYTES as u32;
                } else {
                    self.clear_operation();
                }
                self.report()
            }
            CoreGraphAction::Authorize => {
                if self
                    .active
                    .is_none_or(|active| !identity_matches(active, command))
                    || self.receiving
                    || self.candidate.is_some()
                {
                    return self.reject(command, GraphDeploymentFault::Sequence);
                }
                self.active_authorized = true;
                self.last_fault = GraphDeploymentFault::None;
                self.report()
            }
        }
    }

    /// Latest state suitable for periodic replay after lossy telemetry.
    pub fn report(&self) -> RealtimeGraphReport {
        if let Some(candidate) = self.candidate {
            return report_for_identity(
                RealtimeGraphState::CandidateValid,
                candidate,
                self.active,
                self.active_authorized,
            );
        }
        if self.receiving {
            return RealtimeGraphReport {
                state: RealtimeGraphState::Receiving,
                transaction_id: self.transaction_id,
                content_digest: self.content_digest,
                package_digest: self.package_digest,
                consumed_bytes: self.consumed_bytes,
                summary: None,
                fault: GraphDeploymentFault::None,
                active_content_digest: active_content(self.active),
                active_authorized: self.active_authorized,
            };
        }
        if self.last_fault != GraphDeploymentFault::None {
            return RealtimeGraphReport {
                state: RealtimeGraphState::Rejected,
                transaction_id: self.transaction_id,
                content_digest: self.content_digest,
                package_digest: self.package_digest,
                consumed_bytes: self.consumed_bytes.min(GRAPH_IR_PACKAGE_BYTES as u32),
                summary: None,
                fault: self.last_fault,
                active_content_digest: active_content(self.active),
                active_authorized: self.active_authorized,
            };
        }
        if self.cleared {
            return RealtimeGraphReport {
                state: RealtimeGraphState::Cleared,
                transaction_id: self.transaction_id,
                content_digest: self.content_digest,
                package_digest: self.package_digest,
                consumed_bytes: GRAPH_IR_PACKAGE_BYTES as u32,
                summary: None,
                fault: GraphDeploymentFault::None,
                active_content_digest: Digest::ZERO,
                active_authorized: false,
            };
        }
        if let Some(active) = self.active {
            return report_for_identity(
                RealtimeGraphState::Active,
                active,
                Some(active),
                self.active_authorized,
            );
        }
        RealtimeGraphReport::empty()
    }

    /// Independently validated inactive candidate identity.
    pub const fn candidate_identity(&self) -> Option<GraphDeploymentIdentity> {
        self.candidate
    }

    /// Independently selected active identity, whether or not execution is authorized.
    pub const fn active_identity(&self) -> Option<GraphDeploymentIdentity> {
        self.active
    }

    /// Exact selected active bytes for independent permanent-actor admission.
    ///
    /// Selection is not execution authority. Callers must still require the
    /// explicit dual-core authorization transition before any run is prepared.
    pub fn selected_package_bytes(&self) -> Option<&[u8; GRAPH_IR_PACKAGE_BYTES]> {
        self.active.as_ref().map(|_| &self.active_bytes)
    }

    /// Exact active bytes only after dual-core authorization.
    pub fn authorized_package_bytes(&self) -> Option<&[u8; GRAPH_IR_PACKAGE_BYTES]> {
        if self.active_authorized {
            Some(&self.active_bytes)
        } else {
            None
        }
    }

    fn validate_authority(
        &self,
        command: CoreGraphCommand,
        authority: GraphRuntimeAuthority,
    ) -> Result<(), GraphDeploymentFault> {
        if authority.device_id != self.device_id {
            return Err(GraphDeploymentFault::Device);
        }
        if authority.capability_digest != self.capability_digest {
            return Err(GraphDeploymentFault::Capability);
        }
        if authority.config_digest.is_zero() {
            return Err(GraphDeploymentFault::Configuration);
        }
        if authority.implementation_digest != command.implementation_digest {
            return Err(GraphDeploymentFault::Implementation);
        }
        Ok(())
    }

    fn matches(&self, command: CoreGraphCommand) -> bool {
        self.transaction_id == command.transaction_id
            && self.content_digest == command.content_digest
            && self.package_digest == command.package_digest
            && self.implementation_digest == command.implementation_digest
    }

    fn set_operation(&mut self, command: CoreGraphCommand) {
        self.transaction_id = command.transaction_id;
        self.content_digest = command.content_digest;
        self.package_digest = command.package_digest;
        self.implementation_digest = command.implementation_digest;
    }

    fn set_identity(&mut self, identity: GraphDeploymentIdentity) {
        self.transaction_id = identity.transaction_id;
        self.content_digest = identity.content_digest;
        self.package_digest = identity.package_digest;
        self.implementation_digest = identity.implementation_digest;
    }

    fn clear_operation(&mut self) {
        self.transaction_id = 0;
        self.content_digest = Digest::ZERO;
        self.package_digest = Digest::ZERO;
        self.implementation_digest = Digest::ZERO;
        self.consumed_bytes = 0;
    }

    fn reject(
        &mut self,
        command: CoreGraphCommand,
        fault: GraphDeploymentFault,
    ) -> RealtimeGraphReport {
        self.set_operation(command);
        self.receiving = false;
        self.candidate = None;
        self.cleared = false;
        self.last_fault = fault;
        self.report()
    }
}

fn identity_matches(identity: GraphDeploymentIdentity, command: CoreGraphCommand) -> bool {
    identity.transaction_id == command.transaction_id
        && identity.content_digest == command.content_digest
        && identity.package_digest == command.package_digest
        && identity.implementation_digest == command.implementation_digest
}

fn report_for_identity(
    state: RealtimeGraphState,
    identity: GraphDeploymentIdentity,
    active: Option<GraphDeploymentIdentity>,
    active_authorized: bool,
) -> RealtimeGraphReport {
    RealtimeGraphReport {
        state,
        transaction_id: identity.transaction_id,
        content_digest: identity.content_digest,
        package_digest: identity.package_digest,
        consumed_bytes: GRAPH_IR_PACKAGE_BYTES as u32,
        summary: Some(GraphIrSummary {
            node_count: identity.summary.node_count,
            channel_count: identity.summary.channel_count,
            bridge_count: identity.summary.bridge_count,
            service_state_bytes: 0,
            realtime_state_bytes: 0,
            channel_storage_bytes: 0,
            bridge_storage_bytes: 0,
        }),
        fault: GraphDeploymentFault::None,
        active_content_digest: active_content(active),
        active_authorized,
    }
}

const fn active_content(active: Option<GraphDeploymentIdentity>) -> Digest {
    match active {
        Some(active) => active.content_digest,
        None => Digest::ZERO,
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

fn array<const N: usize>(bytes: &[u8], offset: usize) -> [u8; N] {
    let mut result = [0_u8; N];
    result.copy_from_slice(&bytes[offset..offset + N]);
    result
}

#[cfg(test)]
mod tests {
    extern crate std;

    use core::cell::RefCell;
    use std::rc::Rc;
    use std::vec;
    use std::vec::Vec;

    use alumina_graph_ir::{
        BOOLEAN_LATEST_STATE_BYTES, BOOLEAN_STREAM_ITEM_BYTES, CoreGraphCommand, GraphIrChannel,
        GraphIrChannelOwner, GraphIrDomain, GraphIrFullPolicy, GraphIrHeader, GraphIrNode,
        GraphIrOpcode, GraphIrPackage, GraphIrSchedule, GraphPublication,
    };
    use alumina_protocol::DeviceCycle;
    use alumina_storage::media::{
        DurableGraphSelection, GraphTransition, MEDIA_BLOCK_BYTES, MediaBlock, MediaId, MediaRegion,
    };
    use alumina_storage::provisioning::CacheProvisionRequest;
    use alumina_storage::{
        CacheLimits, ChunkUploadHeader, ContentId, FinalizeUploadRequest, ManifestHasher,
        MutationContext, ObjectKind, PublishedObject, StoredObject, UploadId, UploadPlan, sha256,
    };
    use embassy_futures::block_on;

    use super::*;
    use crate::graph::{
        FixedGraphRealtimeActor, FixedGraphServiceActor, GraphActorPhase, GraphBridgePhase,
        GraphRunIdentity, ReloadableGraphBridge,
    };

    const TEST_DEVICE_BLOCKS: usize = 2_300;
    const TEST_REGION: MediaRegion = MediaRegion {
        start_block: 2_048,
        block_count: 200,
    };
    const TEST_CACHE_LIMITS: CacheLimits = CacheLimits {
        maximum_object_bytes: 64 * 1_024,
        maximum_chunk_bytes: 1_024,
        maximum_chunks: 128,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestDeviceError {
        OutsideDevice,
    }

    struct RamBlockDevice {
        blocks: Rc<RefCell<Vec<MediaBlock>>>,
    }

    impl RamBlockDevice {
        fn erased() -> Self {
            Self {
                blocks: Rc::new(RefCell::new(vec![
                    [0xff; MEDIA_BLOCK_BYTES];
                    TEST_DEVICE_BLOCKS
                ])),
            }
        }
    }

    impl AsyncBlockDevice for RamBlockDevice {
        type Error = TestDeviceError;

        fn block_count(&self) -> u64 {
            u64::try_from(self.blocks.borrow().len()).unwrap()
        }

        async fn read_block(
            &mut self,
            block: u64,
            output: &mut MediaBlock,
        ) -> Result<(), Self::Error> {
            let index = usize::try_from(block).map_err(|_| TestDeviceError::OutsideDevice)?;
            let blocks = self.blocks.borrow();
            let source = blocks.get(index).ok_or(TestDeviceError::OutsideDevice)?;
            output.copy_from_slice(source);
            Ok(())
        }

        async fn write_block(&mut self, block: u64, data: &MediaBlock) -> Result<(), Self::Error> {
            let index = usize::try_from(block).map_err(|_| TestDeviceError::OutsideDevice)?;
            let mut blocks = self.blocks.borrow_mut();
            let target = blocks
                .get_mut(index)
                .ok_or(TestDeviceError::OutsideDevice)?;
            target.copy_from_slice(data);
            Ok(())
        }

        async fn sync(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    fn digest(byte: u8) -> Digest {
        Digest([byte; 32])
    }

    fn package() -> GraphIrPackage {
        GraphIrPackage::encode(
            GraphIrHeader {
                device_id: DeviceId([1; 16]),
                graph_digest: digest(2),
                implementation_digest: digest(3),
                capability_digest: digest(4),
                config_digest: digest(5),
                service_schedule: GraphIrSchedule {
                    clock_id: 10,
                    period_cycles: 1_000,
                    total_wcet_cycles: 20,
                    executor_reserve_cycles: 100,
                    node_count: 1,
                },
                realtime_schedule: GraphIrSchedule {
                    clock_id: 20,
                    period_cycles: 2_000,
                    total_wcet_cycles: 40,
                    executor_reserve_cycles: 100,
                    node_count: 2,
                },
                total_state_bytes: BOOLEAN_LATEST_STATE_BYTES,
                service_state_bytes: 0,
                realtime_state_bytes: BOOLEAN_LATEST_STATE_BYTES,
                channel_storage_bytes: 3 * BOOLEAN_STREAM_ITEM_BYTES,
                bridge_storage_bytes: 2 * BOOLEAN_STREAM_ITEM_BYTES,
            },
            &[
                GraphIrNode {
                    graph_node_id: 1,
                    domain: GraphIrDomain::Service,
                    opcode: GraphIrOpcode::BooleanStreamConstant,
                    schedule_clock_id: 10,
                    period_cycles: 1_000,
                    wcet_cycles: 20,
                    state_offset: 0,
                    state_bytes: 0,
                    parameter: 1,
                },
                GraphIrNode {
                    graph_node_id: 2,
                    domain: GraphIrDomain::Realtime,
                    opcode: GraphIrOpcode::BooleanLatest,
                    schedule_clock_id: 20,
                    period_cycles: 2_000,
                    wcet_cycles: 20,
                    state_offset: 0,
                    state_bytes: BOOLEAN_LATEST_STATE_BYTES,
                    parameter: 0,
                },
                GraphIrNode {
                    graph_node_id: 3,
                    domain: GraphIrDomain::Realtime,
                    opcode: GraphIrOpcode::BooleanStreamSink,
                    schedule_clock_id: 20,
                    period_cycles: 2_000,
                    wcet_cycles: 20,
                    state_offset: BOOLEAN_LATEST_STATE_BYTES,
                    state_bytes: 0,
                    parameter: 0,
                },
            ],
            &[
                GraphIrChannel {
                    graph_wire_id: 1,
                    source_node: 0,
                    target_node: 1,
                    owner: GraphIrChannelOwner::ServiceToRealtime,
                    full_policy: GraphIrFullPolicy::Fault,
                    capacity: 2,
                    item_bytes: BOOLEAN_STREAM_ITEM_BYTES,
                    storage_offset: 0,
                    storage_bytes: 2 * BOOLEAN_STREAM_ITEM_BYTES,
                },
                GraphIrChannel {
                    graph_wire_id: 2,
                    source_node: 1,
                    target_node: 2,
                    owner: GraphIrChannelOwner::Realtime,
                    full_policy: GraphIrFullPolicy::Fault,
                    capacity: 1,
                    item_bytes: BOOLEAN_STREAM_ITEM_BYTES,
                    storage_offset: 0,
                    storage_bytes: BOOLEAN_STREAM_ITEM_BYTES,
                },
            ],
        )
        .unwrap()
    }

    fn publication(package: &GraphIrPackage, transaction_id: u64) -> GraphPublication {
        GraphPublication {
            transaction_id,
            publication: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::DeployedGraph,
                    content: ContentId::from_sha256(graph_ir_content_digest(package.bytes())),
                    byte_len: GRAPH_IR_PACKAGE_BYTES as u64,
                },
                manifest: ContentId::from_sha256(digest(9)),
            },
            package_digest: package.digest(),
            implementation_digest: package.header().implementation_digest,
        }
    }

    fn durable(publication: GraphPublication) -> DurableGraphSelection {
        DurableGraphSelection::new(
            publication.transaction_id,
            publication.publication,
            publication.package_digest,
            publication.implementation_digest,
        )
        .unwrap()
    }

    fn provisioned_graph(
        package: &GraphIrPackage,
        transaction_id: u64,
        chunk_bytes: usize,
    ) -> (ProvisionedCache<RamBlockDevice>, GraphPublication) {
        let bytes = package.bytes();
        let mut cache = ProvisionedCache::new(RamBlockDevice::erased(), TEST_CACHE_LIMITS);
        block_on(cache.discover()).unwrap();
        let request = CacheProvisionRequest::new(
            u64::try_from(TEST_DEVICE_BLOCKS).unwrap(),
            0,
            None,
            TEST_REGION,
            MediaId::new([0x5a; 16]).unwrap(),
            false,
        )
        .unwrap();
        block_on(cache.provision(request, MutationContext::DISARMED_IDLE)).unwrap();

        let object = StoredObject {
            kind: ObjectKind::DeployedGraph,
            content: sha256(bytes),
            byte_len: GRAPH_IR_PACKAGE_BYTES as u64,
        };
        let chunk_count = bytes.len().div_ceil(chunk_bytes);
        let mut manifest = ManifestHasher::new(
            object,
            u32::try_from(chunk_bytes).unwrap(),
            u32::try_from(chunk_count).unwrap(),
            TEST_CACHE_LIMITS,
        )
        .unwrap();
        for (index, chunk) in bytes.chunks(chunk_bytes).enumerate() {
            manifest
                .push(
                    u32::try_from(index).unwrap(),
                    sha256(chunk),
                    u32::try_from(chunk.len()).unwrap(),
                )
                .unwrap();
        }
        let plan = UploadPlan {
            upload_id: UploadId(0x1234_9876),
            object,
            manifest: manifest.finalize().unwrap(),
            chunk_bytes: u32::try_from(chunk_bytes).unwrap(),
            chunk_count: u32::try_from(chunk_count).unwrap(),
        };
        block_on(cache.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        for (index, chunk) in bytes.chunks(chunk_bytes).enumerate() {
            block_on(cache.put_chunk(
                ChunkUploadHeader {
                    upload_id: plan.upload_id,
                    index: u32::try_from(index).unwrap(),
                    byte_len: u32::try_from(chunk.len()).unwrap(),
                    content: sha256(chunk),
                },
                chunk,
                MutationContext::DISARMED_IDLE,
            ))
            .unwrap();
        }
        let published = block_on(cache.finalize_upload(
            FinalizeUploadRequest {
                upload_id: plan.upload_id,
            },
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap();
        (
            cache,
            GraphPublication {
                transaction_id,
                publication: published,
                package_digest: package.digest(),
                implementation_digest: package.header().implementation_digest,
            },
        )
    }

    fn authority() -> GraphRuntimeAuthority {
        GraphRuntimeAuthority {
            device_id: DeviceId([1; 16]),
            capability_digest: digest(4),
            config_digest: digest(5),
            implementation_digest: digest(3),
        }
    }

    fn limits() -> GraphRuntimeLimits {
        GraphRuntimeLimits {
            service_state_bytes: 0,
            realtime_state_bytes: 5,
            service_channel_bytes: 0,
            realtime_channel_bytes: 21,
            bridge_channel_bytes: 42,
            opcodes: crate::graph::RESOURCE_FREE_GRAPH_OPCODES,
            resources: &[],
        }
    }

    fn transfer(
        deployment: &mut RealtimeGraphDeployment,
        publication: GraphPublication,
        bytes: &[u8],
    ) -> RealtimeGraphReport {
        transfer_with_authority(deployment, publication, bytes, authority())
    }

    fn transfer_with_authority(
        deployment: &mut RealtimeGraphDeployment,
        publication: GraphPublication,
        bytes: &[u8],
        authority: GraphRuntimeAuthority,
    ) -> RealtimeGraphReport {
        deployment.apply(
            CoreGraphCommand::begin(publication).unwrap(),
            authority,
            true,
        );
        let mut offset = 0;
        for chunk in bytes.chunks(alumina_graph_ir::MAX_CORE_GRAPH_DATA_BYTES) {
            let report = deployment.apply(
                CoreGraphCommand::data(publication, offset, chunk).unwrap(),
                authority,
                true,
            );
            offset += u32::try_from(chunk.len()).unwrap();
            if offset == GRAPH_IR_PACKAGE_BYTES as u32 {
                assert_eq!(report.state, RealtimeGraphState::Receiving);
                assert_eq!(report.consumed_bytes, GRAPH_IR_PACKAGE_BYTES as u32);
                assert_eq!(
                    RealtimeGraphReport::decode(&report.encode().unwrap()),
                    Ok(report)
                );
            }
        }
        deployment.apply(
            CoreGraphCommand::finish(publication).unwrap(),
            authority,
            true,
        )
    }

    #[test]
    fn complete_transfer_activate_and_authorize_preserve_exact_active_bytes() {
        let package = package();
        let publication = publication(&package, 41);
        let mut deployment = RealtimeGraphDeployment::new(
            authority().device_id,
            authority().capability_digest,
            limits(),
        );
        let candidate = transfer(&mut deployment, publication, package.bytes());
        assert_eq!(candidate.state, RealtimeGraphState::CandidateValid);
        assert_eq!(candidate.summary.unwrap().node_count, 3);
        assert_eq!(
            RealtimeGraphReport::decode(&candidate.encode().unwrap()),
            Ok(candidate)
        );
        let recovering = GraphCoordinatorReport {
            phase: GraphCoordinatorPhase::Recovering,
            fault: GraphCoordinatorFault::None,
            transaction_id: publication.transaction_id,
            content_digest: publication.content_digest(),
            package_digest: publication.package_digest,
            validated_bytes: 0,
            storage_chunks_read: 0,
            active_content_digest: Digest::ZERO,
            realtime: RealtimeGraphReport::empty(),
            execution: GraphExecutionReport::empty(),
        };
        assert_eq!(
            GraphCoordinatorReport::decode(&recovering.encode().unwrap()),
            Ok(recovering)
        );
        let preparing = GraphCoordinatorReport {
            phase: GraphCoordinatorPhase::Preparing,
            validated_bytes: GRAPH_IR_PACKAGE_BYTES as u32,
            storage_chunks_read: 4,
            realtime: candidate,
            ..recovering
        };
        assert_eq!(
            GraphCoordinatorReport::decode(&preparing.encode().unwrap()),
            Ok(preparing)
        );

        let active = deployment.apply(
            CoreGraphCommand::activate(publication).unwrap(),
            authority(),
            true,
        );
        assert_eq!(active.state, RealtimeGraphState::Active);
        assert!(!active.active_authorized);
        assert!(deployment.authorized_package_bytes().is_none());
        let committing = GraphCoordinatorReport {
            phase: GraphCoordinatorPhase::Committing,
            realtime: active,
            ..preparing
        };
        assert_eq!(
            GraphCoordinatorReport::decode(&committing.encode().unwrap()),
            Ok(committing)
        );
        let durability_rejection = GraphCoordinatorReport {
            phase: GraphCoordinatorPhase::Rejected,
            fault: GraphCoordinatorFault::Durability,
            ..committing
        };
        assert_eq!(
            GraphCoordinatorReport::decode(&durability_rejection.encode().unwrap()),
            Ok(durability_rejection)
        );
        let selector_replay_failure = GraphCoordinatorReport {
            phase: GraphCoordinatorPhase::Rejected,
            fault: GraphCoordinatorFault::Storage,
            ..GraphCoordinatorReport::empty()
        };
        assert_eq!(
            GraphCoordinatorReport::decode(&selector_replay_failure.encode().unwrap()),
            Ok(selector_replay_failure)
        );

        let authorized = deployment.apply(
            CoreGraphCommand::authorize(publication).unwrap(),
            authority(),
            true,
        );
        assert!(authorized.active_authorized);
        assert_eq!(deployment.authorized_package_bytes(), Some(package.bytes()));
        let coordinator = GraphCoordinatorReport {
            phase: GraphCoordinatorPhase::Active,
            fault: GraphCoordinatorFault::None,
            transaction_id: publication.transaction_id,
            content_digest: publication.content_digest(),
            package_digest: publication.package_digest,
            validated_bytes: GRAPH_IR_PACKAGE_BYTES as u32,
            storage_chunks_read: 4,
            active_content_digest: publication.content_digest(),
            realtime: authorized,
            execution: GraphExecutionReport::empty(),
        };
        assert_eq!(
            GraphCoordinatorReport::decode(&coordinator.encode().unwrap()),
            Ok(coordinator)
        );

        let handshake_fault = GraphCoordinatorReport {
            phase: GraphCoordinatorPhase::ExecutionFaulted,
            fault: GraphCoordinatorFault::Protocol,
            execution: GraphExecutionReport {
                service_phase: GraphActorPhase::Prepared,
                realtime_phase: GraphActorPhase::Installed,
                bridge_phase: GraphBridgePhase::Primed,
                run_id: 9,
                start_cycle: DeviceCycle(1_000),
                service_next_cycle: Some(DeviceCycle(2_000)),
                realtime_next_cycle: None,
                service_last_tick: Some(0),
                realtime_last_tick: None,
                fault: None,
            },
            ..coordinator
        };
        assert_eq!(
            GraphCoordinatorReport::decode(&handshake_fault.encode().unwrap()),
            Ok(handshake_fault)
        );
    }

    #[test]
    fn durable_activation_orphan_recovery_and_clear_preserve_authorization_order() {
        let package = package();
        let (mut cache, publication) = provisioned_graph(&package, 81, 173);
        let selection = durable(publication);
        let activation = GraphTransition::activate(selection);

        let prepared =
            block_on(cache.prepare_graph_transition(activation, MutationContext::DISARMED_IDLE))
                .unwrap();
        assert_eq!(prepared.active, None);
        assert_eq!(prepared.pending, Some(activation));

        let mut realtime = RealtimeGraphDeployment::new(
            authority().device_id,
            authority().capability_digest,
            limits(),
        );
        assert_eq!(
            transfer(&mut realtime, publication, package.bytes()).state,
            RealtimeGraphState::CandidateValid
        );
        let selected = realtime.apply(
            CoreGraphCommand::activate(publication).unwrap(),
            authority(),
            true,
        );
        assert_eq!(selected.state, RealtimeGraphState::Active);
        assert!(!selected.active_authorized);
        assert!(realtime.authorized_package_bytes().is_none());

        let committed =
            block_on(cache.commit_graph_transition(activation, MutationContext::DISARMED_IDLE))
                .unwrap();
        assert_eq!(committed.active, Some(selection));
        assert_eq!(committed.pending, None);
        assert!(
            realtime
                .apply(
                    CoreGraphCommand::authorize(publication).unwrap(),
                    authority(),
                    true,
                )
                .active_authorized
        );

        // A replacement prepare is durable but inert. Reboot must retain the
        // old complete selection, expose the orphan, and abort the orphan
        // before reopening any graph bytes.
        let replacement = GraphPublication {
            transaction_id: publication.transaction_id + 1,
            ..publication
        };
        let orphan = GraphTransition::activate(durable(replacement));
        block_on(cache.prepare_graph_transition(orphan, MutationContext::DISARMED_IDLE)).unwrap();
        let device = cache.into_device();
        let mut cache = ProvisionedCache::new(device, TEST_CACHE_LIMITS);
        block_on(cache.discover()).unwrap();
        assert_eq!(
            cache.graph_journal().unwrap(),
            alumina_storage::media::GraphJournal {
                active: Some(selection),
                pending: Some(orphan),
            }
        );
        let recovered =
            block_on(cache.abort_graph_transition(orphan, MutationContext::DISARMED_IDLE)).unwrap();
        assert_eq!(recovered.active, Some(selection));
        assert_eq!(recovered.pending, None);

        let mut service = block_on(ServiceGraphValidation::open(
            &mut cache,
            publication,
            authority(),
            limits(),
        ))
        .unwrap();
        let mut rebooted_realtime = RealtimeGraphDeployment::new(
            authority().device_id,
            authority().capability_digest,
            limits(),
        );
        while let Some(command) = block_on(service.next(&mut cache)).unwrap() {
            assert_ne!(
                rebooted_realtime.apply(command, authority(), true).state,
                RealtimeGraphState::Rejected
            );
        }
        assert_eq!(
            service.status().identity,
            rebooted_realtime.candidate_identity()
        );
        assert!(
            !rebooted_realtime
                .apply(
                    CoreGraphCommand::activate(publication).unwrap(),
                    authority(),
                    true,
                )
                .active_authorized
        );
        assert_eq!(cache.graph_journal().unwrap().active, Some(selection));
        assert!(
            rebooted_realtime
                .apply(
                    CoreGraphCommand::authorize(publication).unwrap(),
                    authority(),
                    true,
                )
                .active_authorized
        );

        let clear = GraphTransition::clear(selection);
        let clear_prepared =
            block_on(cache.prepare_graph_transition(clear, MutationContext::DISARMED_IDLE))
                .unwrap();
        assert_eq!(clear_prepared.active, Some(selection));
        assert_eq!(clear_prepared.pending, Some(clear));
        assert_eq!(
            rebooted_realtime
                .apply(
                    CoreGraphCommand::clear(publication).unwrap(),
                    authority(),
                    true,
                )
                .state,
            RealtimeGraphState::Cleared
        );
        let cleared =
            block_on(cache.commit_graph_transition(clear, MutationContext::DISARMED_IDLE)).unwrap();
        assert_eq!(cleared.active, None);
        assert_eq!(cleared.pending, None);
    }

    #[test]
    fn published_bytes_install_into_both_permanent_actors_and_run() {
        let package = package();
        let (mut cache, publication) = provisioned_graph(&package, 71, 173);
        let mut service = block_on(ServiceGraphValidation::open(
            &mut cache,
            publication,
            authority(),
            limits(),
        ))
        .unwrap();
        let mut realtime = RealtimeGraphDeployment::new(
            authority().device_id,
            authority().capability_digest,
            limits(),
        );
        let mut commands = 0_u32;
        while let Some(command) = block_on(service.next(&mut cache)).unwrap() {
            commands += 1;
            let report = realtime.apply(command, authority(), true);
            assert_ne!(report.state, RealtimeGraphState::Rejected);
            assert_eq!(
                RealtimeGraphReport::decode(&report.encode().unwrap()),
                Ok(report)
            );
        }
        let service_status = service.status();
        assert_eq!(service_status.state, ServiceGraphValidationState::Complete);
        assert_eq!(
            service_status.validated_bytes,
            GRAPH_IR_PACKAGE_BYTES as u32
        );
        assert_eq!(service_status.storage_chunks_read, 24);
        assert!(commands > service_status.storage_chunks_read);
        assert_eq!(
            service_status.identity,
            realtime.candidate_identity(),
            "core 0 and core 1 must reconstruct the same package authority"
        );
        assert_eq!(
            service_status.identity.unwrap().package_digest,
            package.digest()
        );

        let selected = realtime.apply(
            CoreGraphCommand::activate(publication).unwrap(),
            authority(),
            true,
        );
        assert_eq!(selected.state, RealtimeGraphState::Active);
        assert!(!selected.active_authorized);
        assert_eq!(realtime.selected_package_bytes(), Some(package.bytes()));

        let bridge = ReloadableGraphBridge::<42>::new();
        let mut realtime_actor = FixedGraphRealtimeActor::<5, 21, 42>::new(&bridge);
        let identity = realtime.active_identity().unwrap();
        realtime_actor
            .install(
                realtime.selected_package_bytes().unwrap(),
                identity.transaction_id,
                identity.content_digest,
                identity.package_digest,
                authority(),
                limits(),
                true,
            )
            .unwrap();
        let authorized = realtime.apply(
            CoreGraphCommand::authorize(publication).unwrap(),
            authority(),
            true,
        );
        assert!(authorized.active_authorized);

        let mut service_actor = FixedGraphServiceActor::<0, 0, 42>::new(&bridge);
        service_actor
            .install(
                service.validated_package_bytes().unwrap(),
                identity.transaction_id,
                identity.content_digest,
                identity.package_digest,
                authority(),
                limits(),
                true,
            )
            .unwrap();
        let run = GraphRunIdentity {
            transaction_id: identity.transaction_id,
            run_id: 1,
            content_digest: identity.content_digest,
            package_digest: identity.package_digest,
            start_cycle: DeviceCycle(10_000),
        };
        service_actor.prepare_start(run, true).unwrap();
        realtime_actor.prepare_start(run, true).unwrap();
        realtime_actor.activate(run).unwrap();
        service_actor.observe_realtime_started(run).unwrap();
        assert_eq!(
            realtime_actor
                .release(DeviceCycle(10_000), true, |_| None)
                .unwrap()
                .last_sink_value,
            Some(true)
        );
        assert!(!service_actor.stop(run).unwrap().bridge_empty);
        assert!(realtime_actor.stop(run).unwrap().bridge_empty);
        realtime_actor.clear(true).unwrap();
        let cleared = realtime.apply(
            CoreGraphCommand::clear(publication).unwrap(),
            authority(),
            true,
        );
        assert_eq!(cleared.state, RealtimeGraphState::Cleared);
        service_actor.clear(true).unwrap();
    }

    #[test]
    fn replacement_staging_cannot_mutate_authorized_active_bytes() {
        let first = package();
        let first_publication = publication(&first, 51);
        let mut deployment = RealtimeGraphDeployment::new(
            authority().device_id,
            authority().capability_digest,
            limits(),
        );
        transfer(&mut deployment, first_publication, first.bytes());
        deployment.apply(
            CoreGraphCommand::activate(first_publication).unwrap(),
            authority(),
            true,
        );
        deployment.apply(
            CoreGraphCommand::authorize(first_publication).unwrap(),
            authority(),
            true,
        );

        let second_publication = GraphPublication {
            transaction_id: 52,
            ..first_publication
        };
        deployment.apply(
            CoreGraphCommand::begin(second_publication).unwrap(),
            authority(),
            true,
        );
        deployment.apply(
            CoreGraphCommand::data(second_publication, 0, &[0xaa; 32]).unwrap(),
            authority(),
            true,
        );
        assert_eq!(deployment.authorized_package_bytes(), Some(first.bytes()));
        let report = deployment.apply(
            CoreGraphCommand::abort(second_publication).unwrap(),
            authority(),
            true,
        );
        assert_eq!(report.state, RealtimeGraphState::Active);
        assert_eq!(deployment.authorized_package_bytes(), Some(first.bytes()));
    }

    #[test]
    fn content_authority_capacity_and_sequence_fail_closed() {
        let package = package();
        let publication = publication(&package, 61);
        let mut deployment = RealtimeGraphDeployment::new(
            authority().device_id,
            authority().capability_digest,
            limits(),
        );
        let mut corrupted = *package.bytes();
        corrupted[0] ^= 1;
        let report = transfer(&mut deployment, publication, &corrupted);
        assert_eq!(report.fault, GraphDeploymentFault::ContentDigest);
        let report = deployment.apply(
            CoreGraphCommand::abort(publication).unwrap(),
            authority(),
            true,
        );
        assert_eq!(report, RealtimeGraphReport::empty());

        let mut wrong_authority = authority();
        wrong_authority.config_digest = digest(7);
        let report = transfer_with_authority(
            &mut deployment,
            publication,
            package.bytes(),
            wrong_authority,
        );
        assert_eq!(report.fault, GraphDeploymentFault::Configuration);

        let report = deployment.apply(
            CoreGraphCommand::begin(publication).unwrap(),
            authority(),
            false,
        );
        assert_eq!(report.fault, GraphDeploymentFault::ForbiddenState);

        let mut too_small = RealtimeGraphDeployment::new(
            authority().device_id,
            authority().capability_digest,
            GraphRuntimeLimits {
                bridge_channel_bytes: 41,
                ..limits()
            },
        );
        let report = transfer(&mut too_small, publication, package.bytes());
        assert_eq!(report.fault, GraphDeploymentFault::Capacity);

        let mut out_of_order = RealtimeGraphDeployment::new(
            authority().device_id,
            authority().capability_digest,
            limits(),
        );
        out_of_order.apply(
            CoreGraphCommand::begin(publication).unwrap(),
            authority(),
            true,
        );
        let report = out_of_order.apply(
            CoreGraphCommand::data(publication, 1, &package.bytes()[..8]).unwrap(),
            authority(),
            true,
        );
        assert_eq!(report.fault, GraphDeploymentFault::Sequence);
    }
}
