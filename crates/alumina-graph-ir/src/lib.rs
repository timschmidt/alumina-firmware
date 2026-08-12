#![no_std]
#![doc = "Canonical fixed-memory graph work admitted by Alumina firmware."]

use core::fmt;

use alumina_protocol::{DeviceId, Digest};
use alumina_storage::{CacheLimits, ObjectKind, PublishedObject};
use sha2::{Digest as _, Sha256};

/// Exact bytes in one independently validated deployed-graph package.
pub const GRAPH_IR_PACKAGE_BYTES: usize = 4_096;
/// Fixed canonical header bytes before node and channel records.
pub const GRAPH_IR_HEADER_BYTES: usize = 256;
/// Final SHA-256 field offset and usable end of the zero-padded body.
pub const GRAPH_IR_DIGEST_OFFSET: usize = GRAPH_IR_PACKAGE_BYTES - 32;
/// Exact deployed-node record bytes.
pub const GRAPH_IR_NODE_BYTES: usize = 48;
/// Exact deployed-channel record bytes.
pub const GRAPH_IR_CHANNEL_BYTES: usize = 32;
/// Largest node count admitted by graph-IR V1 independent of package fit.
pub const MAX_GRAPH_IR_NODES: usize = 32;
/// Largest channel count admitted by graph-IR V1 independent of package fit.
pub const MAX_GRAPH_IR_CHANNELS: usize = 64;
/// Largest queue capacity admitted by graph-IR V1.
pub const MAX_GRAPH_IR_QUEUE_ITEMS: u32 = 4_096;
/// Canonical Boolean typed sample plus `u64` tick and `u64` sequence.
pub const BOOLEAN_STREAM_ITEM_BYTES: u32 = 21;
/// Canonical retained Boolean typed value for latest-at-or-before.
pub const BOOLEAN_LATEST_STATE_BYTES: u32 = 5;
/// Deployment-local type tag in every graph-IR Boolean value.
///
/// This is deliberately not an `ALGR` document-local type ID. The reviewed
/// implementation digest binds the source schema; deployed runtime bytes use
/// this fixed tag so firmware never needs the arbitrary-precision schema.
pub const GRAPH_IR_BOOLEAN_TYPE_TAG: u32 = 1;
/// Exact graph-IR schema implemented by this crate.
pub const GRAPH_IR_VERSION: u16 = 1;
/// Exact authenticated `GraphInstall` request bytes.
pub const GRAPH_PUBLICATION_BYTES: usize = 168;
/// Exact authenticated `GraphActivate` and `GraphClear` request bytes.
pub const GRAPH_SELECTION_BYTES: usize = 88;
/// Fixed inter-core graph command prefix before initialized package bytes.
pub const CORE_GRAPH_COMMAND_PREFIX_BYTES: usize = 128;
/// Maximum package bytes carried by one default 336-byte inter-core command.
pub const MAX_CORE_GRAPH_DATA_BYTES: usize = 208;
/// Complete fixed command capacity, equal to the default runtime payload.
pub const CORE_GRAPH_COMMAND_CAPACITY: usize =
    CORE_GRAPH_COMMAND_PREFIX_BYTES + MAX_CORE_GRAPH_DATA_BYTES;

/// Magic bytes at the beginning of every graph-IR V1 package.
pub const GRAPH_IR_MAGIC: [u8; 8] = *b"ALGRIR01";
const GRAPH_IR_FLAGS: u16 = 0;
const GRAPH_PUBLICATION_MAGIC: [u8; 8] = *b"ALGRPQ01";
const GRAPH_SELECTION_MAGIC: [u8; 8] = *b"ALGRPS01";
const CORE_GRAPH_COMMAND_MAGIC: [u8; 4] = *b"ALGC";
const CORE_GRAPH_WIRE_VERSION: u16 = 1;
const GRAPH_OBJECT_LIMITS: CacheLimits = CacheLimits {
    maximum_object_bytes: GRAPH_IR_PACKAGE_BYTES as u64,
    maximum_chunk_bytes: GRAPH_IR_PACKAGE_BYTES as u32,
    maximum_chunks: GRAPH_IR_PACKAGE_BYTES as u32,
};

/// SHA-256 identity over every stored package byte, including its embedded digest.
///
/// This is intentionally distinct from [`GraphIrPackage::digest`], which is
/// the package-internal digest over the padded prefix before the final field.
pub fn graph_ir_content_digest(bytes: &[u8]) -> Digest {
    sha256(bytes)
}

/// One immutable graph package selected for independent dual-core admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphPublication {
    /// Nonzero boot-local transaction joining requests and both core reports.
    pub transaction_id: u64,
    /// Exact typed content and manifest already published on cache media.
    pub publication: PublishedObject,
    /// Package-internal digest over the canonical padded prefix.
    pub package_digest: Digest,
    /// Graph-bound fixed implementation-registry identity expected by the caller.
    pub implementation_digest: Digest,
}

impl GraphPublication {
    /// Encode one canonical fixed GraphInstall body.
    pub fn encode(self) -> Result<[u8; GRAPH_PUBLICATION_BYTES], GraphDeploymentWireError> {
        self.validate()?;
        let mut encoded = [0_u8; GRAPH_PUBLICATION_BYTES];
        encoded[..8].copy_from_slice(&GRAPH_PUBLICATION_MAGIC);
        encoded[8..10].copy_from_slice(&GRAPH_IR_VERSION.to_le_bytes());
        // Bytes 10..16 are reserved zero.
        encoded[16..24].copy_from_slice(&self.transaction_id.to_le_bytes());
        encoded[24..104].copy_from_slice(&self.publication.encode());
        encoded[104..136].copy_from_slice(&self.package_digest.0);
        encoded[136..168].copy_from_slice(&self.implementation_digest.0);
        Ok(encoded)
    }

    /// Decode only the exact canonical GraphInstall representation.
    pub fn decode(encoded: &[u8]) -> Result<Self, GraphDeploymentWireError> {
        if encoded.len() != GRAPH_PUBLICATION_BYTES {
            return Err(GraphDeploymentWireError::Length);
        }
        if encoded[..8] != GRAPH_PUBLICATION_MAGIC {
            return Err(GraphDeploymentWireError::Magic);
        }
        if get_u16(encoded, 8) != GRAPH_IR_VERSION {
            return Err(GraphDeploymentWireError::Version);
        }
        if encoded[10..16].iter().any(|byte| *byte != 0) {
            return Err(GraphDeploymentWireError::Reserved);
        }
        let publication = PublishedObject::decode(&encoded[24..104], GRAPH_OBJECT_LIMITS)
            .map_err(|_| GraphDeploymentWireError::Publication)?;
        let request = Self {
            transaction_id: get_u64(encoded, 16),
            publication,
            package_digest: Digest(array::<32>(encoded, 104)),
            implementation_digest: Digest(array::<32>(encoded, 136)),
        };
        request.validate()?;
        if request.encode()? != encoded {
            return Err(GraphDeploymentWireError::Noncanonical);
        }
        Ok(request)
    }

    /// Full stored-object SHA-256 identity.
    pub const fn content_digest(self) -> Digest {
        self.publication.object.content.digest
    }

    /// Validate the typed immutable publication and every required identity.
    pub fn validate(self) -> Result<(), GraphDeploymentWireError> {
        if self.transaction_id == 0
            || self.publication.object.kind != ObjectKind::DeployedGraph
            || self.publication.object.byte_len != GRAPH_IR_PACKAGE_BYTES as u64
            || self.content_digest().is_zero()
            || self.package_digest.is_zero()
            || self.implementation_digest.is_zero()
        {
            return Err(GraphDeploymentWireError::Identity);
        }
        self.publication
            .validate(GRAPH_OBJECT_LIMITS)
            .map_err(|_| GraphDeploymentWireError::Publication)
    }
}

/// Exact identity selected by GraphActivate or GraphClear.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphSelection {
    /// Transaction originally used to install the candidate.
    pub transaction_id: u64,
    /// Full stored-object SHA-256 identity.
    pub content_digest: Digest,
    /// Package-internal canonical digest.
    pub package_digest: Digest,
}

impl GraphSelection {
    /// Encode one canonical lifecycle selection.
    pub fn encode(self) -> Result<[u8; GRAPH_SELECTION_BYTES], GraphDeploymentWireError> {
        self.validate()?;
        let mut encoded = [0_u8; GRAPH_SELECTION_BYTES];
        encoded[..8].copy_from_slice(&GRAPH_SELECTION_MAGIC);
        encoded[8..10].copy_from_slice(&GRAPH_IR_VERSION.to_le_bytes());
        // Bytes 10..16 are reserved zero.
        encoded[16..24].copy_from_slice(&self.transaction_id.to_le_bytes());
        encoded[24..56].copy_from_slice(&self.content_digest.0);
        encoded[56..88].copy_from_slice(&self.package_digest.0);
        Ok(encoded)
    }

    /// Decode only the exact canonical lifecycle selection.
    pub fn decode(encoded: &[u8]) -> Result<Self, GraphDeploymentWireError> {
        if encoded.len() != GRAPH_SELECTION_BYTES {
            return Err(GraphDeploymentWireError::Length);
        }
        if encoded[..8] != GRAPH_SELECTION_MAGIC {
            return Err(GraphDeploymentWireError::Magic);
        }
        if get_u16(encoded, 8) != GRAPH_IR_VERSION {
            return Err(GraphDeploymentWireError::Version);
        }
        if encoded[10..16].iter().any(|byte| *byte != 0) {
            return Err(GraphDeploymentWireError::Reserved);
        }
        let selection = Self {
            transaction_id: get_u64(encoded, 16),
            content_digest: Digest(array::<32>(encoded, 24)),
            package_digest: Digest(array::<32>(encoded, 56)),
        };
        selection.validate()?;
        if selection.encode()? != encoded {
            return Err(GraphDeploymentWireError::Noncanonical);
        }
        Ok(selection)
    }

    /// Validate the complete lifecycle selection identity.
    pub fn validate(self) -> Result<(), GraphDeploymentWireError> {
        if self.transaction_id == 0
            || self.content_digest.is_zero()
            || self.package_digest.is_zero()
        {
            Err(GraphDeploymentWireError::Identity)
        } else {
            Ok(())
        }
    }
}

/// Ordered action transferring or changing one fixed graph package on core 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CoreGraphAction {
    /// Start receiving exactly one package.
    Begin = 1,
    /// Append one contiguous initialized byte range.
    Data = 2,
    /// Independently decode and validate all received bytes.
    Finish = 3,
    /// Select the validated candidate without authorizing execution.
    Activate = 4,
    /// Clear the selected active package.
    Clear = 5,
    /// Discard an incomplete or validated candidate.
    Abort = 6,
    /// Confirm that the service-core package and active configuration agree.
    Authorize = 7,
}

impl CoreGraphAction {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Begin),
            2 => Some(Self::Data),
            3 => Some(Self::Finish),
            4 => Some(Self::Activate),
            5 => Some(Self::Clear),
            6 => Some(Self::Abort),
            7 => Some(Self::Authorize),
            _ => None,
        }
    }
}

/// Owned bounded core-0 to core-1 graph package command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoreGraphCommand {
    /// Ordered lifecycle action.
    pub action: CoreGraphAction,
    /// Boot-local transaction identity.
    pub transaction_id: u64,
    /// Full stored-object SHA-256 identity.
    pub content_digest: Digest,
    /// Package-internal canonical digest.
    pub package_digest: Digest,
    /// Authenticated graph-bound implementation-registry identity.
    pub implementation_digest: Digest,
    /// Contiguous package offset for Data, or the complete size for Finish.
    pub offset: u32,
    data_len: u16,
    data: [u8; MAX_CORE_GRAPH_DATA_BYTES],
}

impl CoreGraphCommand {
    /// Begin one exact package transfer.
    pub fn begin(publication: GraphPublication) -> Result<Self, GraphDeploymentWireError> {
        Self::without_data(CoreGraphAction::Begin, publication, 0)
    }

    /// Carry one nonempty contiguous package range.
    pub fn data(
        publication: GraphPublication,
        offset: u32,
        data: &[u8],
    ) -> Result<Self, GraphDeploymentWireError> {
        let data_len =
            u16::try_from(data.len()).map_err(|_| GraphDeploymentWireError::DataLength)?;
        let mut command = Self::from_publication(CoreGraphAction::Data, publication, offset);
        command.data_len = data_len;
        command
            .data
            .get_mut(..data.len())
            .ok_or(GraphDeploymentWireError::DataLength)?
            .copy_from_slice(data);
        command.validate()?;
        Ok(command)
    }

    /// Finish and validate a complete package transfer.
    pub fn finish(publication: GraphPublication) -> Result<Self, GraphDeploymentWireError> {
        Self::without_data(
            CoreGraphAction::Finish,
            publication,
            GRAPH_IR_PACKAGE_BYTES as u32,
        )
    }

    /// Select the exact candidate while retaining execution gating.
    pub fn activate(publication: GraphPublication) -> Result<Self, GraphDeploymentWireError> {
        Self::without_data(CoreGraphAction::Activate, publication, 0)
    }

    /// Clear the exact active package.
    pub fn clear(publication: GraphPublication) -> Result<Self, GraphDeploymentWireError> {
        Self::without_data(CoreGraphAction::Clear, publication, 0)
    }

    /// Discard the exact candidate transfer.
    pub fn abort(publication: GraphPublication) -> Result<Self, GraphDeploymentWireError> {
        Self::without_data(CoreGraphAction::Abort, publication, 0)
    }

    /// Authorize the exact active package after service-core agreement.
    pub fn authorize(publication: GraphPublication) -> Result<Self, GraphDeploymentWireError> {
        Self::without_data(CoreGraphAction::Authorize, publication, 0)
    }

    /// Only initialized graph bytes carried by a Data action.
    pub fn data_bytes(&self) -> &[u8] {
        &self.data[..usize::from(self.data_len)]
    }

    /// Encode only the initialized prefix and Data bytes.
    pub fn encode(self) -> Result<EncodedCoreGraphCommand, GraphDeploymentWireError> {
        self.validate()?;
        let mut bytes = [0_u8; CORE_GRAPH_COMMAND_CAPACITY];
        bytes[..4].copy_from_slice(&CORE_GRAPH_COMMAND_MAGIC);
        bytes[4..6].copy_from_slice(&CORE_GRAPH_WIRE_VERSION.to_le_bytes());
        bytes[6] = self.action as u8;
        // Byte 7 and bytes 118..128 are reserved zero.
        bytes[8..16].copy_from_slice(&self.transaction_id.to_le_bytes());
        bytes[16..48].copy_from_slice(&self.content_digest.0);
        bytes[48..80].copy_from_slice(&self.package_digest.0);
        bytes[80..112].copy_from_slice(&self.implementation_digest.0);
        bytes[112..116].copy_from_slice(&self.offset.to_le_bytes());
        bytes[116..118].copy_from_slice(&self.data_len.to_le_bytes());
        let data_len = usize::from(self.data_len);
        bytes[CORE_GRAPH_COMMAND_PREFIX_BYTES..CORE_GRAPH_COMMAND_PREFIX_BYTES + data_len]
            .copy_from_slice(&self.data[..data_len]);
        Ok(EncodedCoreGraphCommand {
            byte_len: u16::try_from(CORE_GRAPH_COMMAND_PREFIX_BYTES + data_len)
                .map_err(|_| GraphDeploymentWireError::Length)?,
            bytes,
        })
    }

    /// Decode one exact initialized inter-core payload.
    pub fn decode(encoded: &[u8]) -> Result<Self, GraphDeploymentWireError> {
        if !(CORE_GRAPH_COMMAND_PREFIX_BYTES..=CORE_GRAPH_COMMAND_CAPACITY).contains(&encoded.len())
        {
            return Err(GraphDeploymentWireError::Length);
        }
        if encoded[..4] != CORE_GRAPH_COMMAND_MAGIC {
            return Err(GraphDeploymentWireError::Magic);
        }
        if get_u16(encoded, 4) != CORE_GRAPH_WIRE_VERSION {
            return Err(GraphDeploymentWireError::Version);
        }
        if encoded[7] != 0 || encoded[118..128].iter().any(|byte| *byte != 0) {
            return Err(GraphDeploymentWireError::Reserved);
        }
        let data_len = get_u16(encoded, 116);
        if encoded.len() != CORE_GRAPH_COMMAND_PREFIX_BYTES + usize::from(data_len) {
            return Err(GraphDeploymentWireError::Length);
        }
        let mut data = [0_u8; MAX_CORE_GRAPH_DATA_BYTES];
        let source = &encoded[CORE_GRAPH_COMMAND_PREFIX_BYTES..];
        data.get_mut(..source.len())
            .ok_or(GraphDeploymentWireError::DataLength)?
            .copy_from_slice(source);
        let command = Self {
            action: CoreGraphAction::from_wire(encoded[6])
                .ok_or(GraphDeploymentWireError::Action)?,
            transaction_id: get_u64(encoded, 8),
            content_digest: Digest(array::<32>(encoded, 16)),
            package_digest: Digest(array::<32>(encoded, 48)),
            implementation_digest: Digest(array::<32>(encoded, 80)),
            offset: get_u32(encoded, 112),
            data_len,
            data,
        };
        command.validate()?;
        if command.encode()?.as_bytes() != encoded {
            return Err(GraphDeploymentWireError::Noncanonical);
        }
        Ok(command)
    }

    fn without_data(
        action: CoreGraphAction,
        publication: GraphPublication,
        offset: u32,
    ) -> Result<Self, GraphDeploymentWireError> {
        let command = Self::from_publication(action, publication, offset);
        command.validate()?;
        Ok(command)
    }

    fn from_publication(
        action: CoreGraphAction,
        publication: GraphPublication,
        offset: u32,
    ) -> Self {
        Self {
            action,
            transaction_id: publication.transaction_id,
            content_digest: publication.content_digest(),
            package_digest: publication.package_digest,
            implementation_digest: publication.implementation_digest,
            offset,
            data_len: 0,
            data: [0; MAX_CORE_GRAPH_DATA_BYTES],
        }
    }

    fn validate(self) -> Result<(), GraphDeploymentWireError> {
        if self.transaction_id == 0
            || self.content_digest.is_zero()
            || self.package_digest.is_zero()
            || self.implementation_digest.is_zero()
        {
            return Err(GraphDeploymentWireError::Identity);
        }
        match self.action {
            CoreGraphAction::Begin
            | CoreGraphAction::Activate
            | CoreGraphAction::Clear
            | CoreGraphAction::Abort
            | CoreGraphAction::Authorize
                if self.offset == 0 && self.data_len == 0 =>
            {
                Ok(())
            }
            CoreGraphAction::Finish
                if self.offset == GRAPH_IR_PACKAGE_BYTES as u32 && self.data_len == 0 =>
            {
                Ok(())
            }
            CoreGraphAction::Data
                if self.data_len != 0
                    && usize::from(self.data_len) <= MAX_CORE_GRAPH_DATA_BYTES
                    && self
                        .offset
                        .checked_add(u32::from(self.data_len))
                        .is_some_and(|end| end <= GRAPH_IR_PACKAGE_BYTES as u32) =>
            {
                Ok(())
            }
            CoreGraphAction::Data => Err(GraphDeploymentWireError::DataLength),
            _ => Err(GraphDeploymentWireError::ActionState),
        }
    }
}

/// Exact initialized representation of one inter-core graph command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodedCoreGraphCommand {
    byte_len: u16,
    bytes: [u8; CORE_GRAPH_COMMAND_CAPACITY],
}

impl EncodedCoreGraphCommand {
    /// Borrow only the canonical initialized prefix.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.byte_len)]
    }
}

/// Canonical graph deployment request or inter-core wire rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphDeploymentWireError {
    /// Exact fixed or initialized length differed.
    Length,
    /// Family magic differed.
    Magic,
    /// Schema or command version differed.
    Version,
    /// A reserved byte was nonzero.
    Reserved,
    /// Decode followed by encode did not reproduce the bytes.
    Noncanonical,
    /// Published typed object or manifest was invalid.
    Publication,
    /// A transaction or digest identity was absent or inconsistent.
    Identity,
    /// Inter-core action byte was unknown.
    Action,
    /// Action, offset, and data shape disagreed.
    ActionState,
    /// Data was empty, oversized, or exceeded the package.
    DataLength,
}

/// Fixed firmware execution domain admitted by graph-IR V1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GraphIrDomain {
    /// Bounded core-0 service actor.
    Service = 1,
    /// Statically scheduled core-1 actor.
    Realtime = 2,
}

impl GraphIrDomain {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Service),
            2 => Some(Self::Realtime),
            _ => None,
        }
    }
}

/// Whitelisted fixed implementation opcode in graph-IR V1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GraphIrOpcode {
    /// Emit one retained Boolean parameter at every Service schedule tick.
    BooleanStreamConstant = 1,
    /// Consume due Boolean samples and retain/emit the latest on core 1.
    BooleanLatest = 2,
    /// Consume one Boolean Stream without a modeled side effect.
    BooleanStreamSink = 3,
}

impl GraphIrOpcode {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::BooleanStreamConstant),
            2 => Some(Self::BooleanLatest),
            3 => Some(Self::BooleanStreamSink),
            _ => None,
        }
    }

    const fn has_input(self) -> bool {
        !matches!(self, Self::BooleanStreamConstant)
    }

    const fn has_output(self) -> bool {
        !matches!(self, Self::BooleanStreamSink)
    }
}

/// Fixed owner of one preallocated channel arena.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GraphIrChannelOwner {
    /// Both endpoints execute on Service/core 0.
    Service = 1,
    /// Both endpoints execute on Realtime/core 1.
    Realtime = 2,
    /// A one-way Service-to-Realtime inter-core queue.
    ServiceToRealtime = 3,
}

impl GraphIrChannelOwner {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Service),
            2 => Some(Self::Realtime),
            3 => Some(Self::ServiceToRealtime),
            _ => None,
        }
    }

    const fn index(self) -> usize {
        self as usize - 1
    }
}

/// Queue-full behavior retained in deployed IR.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GraphIrFullPolicy {
    /// Producer waits until storage is available.
    Backpressure = 0,
    /// Overflow is a latched execution fault.
    Fault = 1,
    /// Reject the newly arriving sample.
    DropNewest = 2,
    /// Evict the oldest queued sample.
    DropOldest = 3,
}

impl GraphIrFullPolicy {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Backpressure),
            1 => Some(Self::Fault),
            2 => Some(Self::DropNewest),
            3 => Some(Self::DropOldest),
            _ => None,
        }
    }
}

/// One domain's single-clock cyclic-executive proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphIrSchedule {
    /// Graph-local clock identity retained for correlation.
    pub clock_id: u32,
    /// Exact integer device cycles between releases.
    pub period_cycles: u64,
    /// Sum of admitted per-node WCET cycles in this domain.
    pub total_wcet_cycles: u64,
    /// Statically reserved dispatch/queue/executor cycles per release.
    pub executor_reserve_cycles: u64,
    /// Number of nodes covered by this schedule.
    pub node_count: u16,
}

impl GraphIrSchedule {
    /// Canonical absent-domain schedule.
    pub const EMPTY: Self = Self {
        clock_id: 0,
        period_cycles: 0,
        total_wcet_cycles: 0,
        executor_reserve_cycles: 0,
        node_count: 0,
    };
}

/// Identities, totals, and schedules encoded in one package header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphIrHeader {
    /// Exact target MCU.
    pub device_id: DeviceId,
    /// Canonical source graph identity.
    pub graph_digest: Digest,
    /// Reviewed fixed implementation-registry identity.
    pub implementation_digest: Digest,
    /// Target capability ledger identity.
    pub capability_digest: Digest,
    /// Active stored configuration identity.
    pub config_digest: Digest,
    /// Service cyclic-executive proof, or [`GraphIrSchedule::EMPTY`].
    pub service_schedule: GraphIrSchedule,
    /// Realtime cyclic-executive proof, or [`GraphIrSchedule::EMPTY`].
    pub realtime_schedule: GraphIrSchedule,
    /// Sum of every fixed node-state arena.
    pub total_state_bytes: u32,
    /// Service-owned node state.
    pub service_state_bytes: u32,
    /// Realtime-owned node state.
    pub realtime_state_bytes: u32,
    /// Sum of all channel arenas.
    pub channel_storage_bytes: u32,
    /// Service-to-Realtime subset of channel storage.
    pub bridge_storage_bytes: u32,
}

/// One topologically ordered fixed implementation record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphIrNode {
    /// Original graph-local node identity for diagnostics.
    pub graph_node_id: u32,
    /// Fixed execution domain.
    pub domain: GraphIrDomain,
    /// Whitelisted behavior.
    pub opcode: GraphIrOpcode,
    /// Schedule clock, equal to the corresponding header schedule.
    pub schedule_clock_id: u32,
    /// Exact integer device-cycle release period.
    pub period_cycles: u64,
    /// Reviewed worst-case execution cycles per release.
    pub wcet_cycles: u64,
    /// Offset within this domain's node-state arena.
    pub state_offset: u32,
    /// Fixed runtime state bytes.
    pub state_bytes: u32,
    /// Opcode-specific canonical immediate; only Boolean constant uses `0`/`1`.
    pub parameter: u64,
}

/// One preallocated Stream queue between topologically ordered nodes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphIrChannel {
    /// Original graph-local wire identity for diagnostics.
    pub graph_wire_id: u32,
    /// Source index in the package node records.
    pub source_node: u16,
    /// Target index in the package node records.
    pub target_node: u16,
    /// Static storage/ownership arena.
    pub owner: GraphIrChannelOwner,
    /// Explicit overflow behavior.
    pub full_policy: GraphIrFullPolicy,
    /// Maximum queued samples.
    pub capacity: u32,
    /// Fixed bytes per timestamped canonical sample.
    pub item_bytes: u32,
    /// Offset within the selected channel arena.
    pub storage_offset: u32,
    /// Exact preallocated bytes for this queue.
    pub storage_bytes: u32,
}

/// Canonical deployment-local Boolean value retained in node state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphIrBooleanValue {
    /// Exact Boolean payload.
    pub value: bool,
}

impl GraphIrBooleanValue {
    /// Encode the fixed deployment tag followed by one canonical Boolean byte.
    pub fn encode(self) -> [u8; BOOLEAN_LATEST_STATE_BYTES as usize] {
        let mut bytes = [0_u8; BOOLEAN_LATEST_STATE_BYTES as usize];
        put_u32(&mut bytes, 0, GRAPH_IR_BOOLEAN_TYPE_TAG);
        bytes[4] = u8::from(self.value);
        bytes
    }

    /// Decode one exact deployment-local Boolean value.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, GraphIrValueError> {
        if bytes.len() != BOOLEAN_LATEST_STATE_BYTES as usize {
            return Err(GraphIrValueError::Length);
        }
        let tag = get_u32(bytes, 0);
        if tag != GRAPH_IR_BOOLEAN_TYPE_TAG {
            return Err(GraphIrValueError::TypeTag(tag));
        }
        let value = match bytes[4] {
            0 => false,
            1 => true,
            value => return Err(GraphIrValueError::Boolean(value)),
        };
        Ok(Self { value })
    }
}

/// One timestamped canonical Boolean queue item used by every V1 opcode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphIrBooleanStreamItem {
    /// Deployment-local Boolean payload.
    pub value: bool,
    /// Release tick in the source node's package schedule.
    pub source_tick: u64,
    /// Monotonic source sequence; fixed V1 opcodes emit their release tick.
    pub sequence: u64,
}

impl GraphIrBooleanStreamItem {
    /// Encode the five-byte Boolean value, source tick, and sequence.
    pub fn encode(self) -> [u8; BOOLEAN_STREAM_ITEM_BYTES as usize] {
        let mut bytes = [0_u8; BOOLEAN_STREAM_ITEM_BYTES as usize];
        bytes[..BOOLEAN_LATEST_STATE_BYTES as usize]
            .copy_from_slice(&GraphIrBooleanValue { value: self.value }.encode());
        put_u64(
            &mut bytes,
            BOOLEAN_LATEST_STATE_BYTES as usize,
            self.source_tick,
        );
        put_u64(&mut bytes, 13, self.sequence);
        bytes
    }

    /// Decode one exact queue item and reject noncanonical tags or Booleans.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, GraphIrValueError> {
        if bytes.len() != BOOLEAN_STREAM_ITEM_BYTES as usize {
            return Err(GraphIrValueError::Length);
        }
        let value = GraphIrBooleanValue::from_slice(&bytes[..BOOLEAN_LATEST_STATE_BYTES as usize])?;
        Ok(Self {
            value: value.value,
            source_tick: get_u64(bytes, BOOLEAN_LATEST_STATE_BYTES as usize),
            sequence: get_u64(bytes, 13),
        })
    }
}

/// Canonical deployed-value rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphIrValueError {
    /// Input has the wrong fixed width.
    Length,
    /// The deployment-local type tag is not Boolean.
    TypeTag(u32),
    /// A Boolean byte was neither zero nor one.
    Boolean(u8),
}

/// Totals independently reconstructed while decoding one package.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphIrSummary {
    /// Complete node records.
    pub node_count: u16,
    /// Complete channel records.
    pub channel_count: u16,
    /// One-way Service-to-Realtime bridge queues.
    pub bridge_count: u16,
    /// Reconstructed Service state bytes.
    pub service_state_bytes: u32,
    /// Reconstructed Realtime state bytes.
    pub realtime_state_bytes: u32,
    /// Reconstructed all-channel storage bytes.
    pub channel_storage_bytes: u32,
    /// Reconstructed Service-to-Realtime storage bytes.
    pub bridge_storage_bytes: u32,
}

/// Canonical fixed-size package paired with decoded authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GraphIrPackage {
    bytes: [u8; GRAPH_IR_PACKAGE_BYTES],
    header: GraphIrHeader,
    summary: GraphIrSummary,
    digest: Digest,
}

impl GraphIrPackage {
    /// Encode, pad, hash, and independently decode one canonical package.
    pub fn encode(
        header: GraphIrHeader,
        nodes: &[GraphIrNode],
        channels: &[GraphIrChannel],
    ) -> Result<Self, GraphIrError> {
        validate_counts(nodes.len(), channels.len())?;
        let initialized_len = initialized_len(nodes.len(), channels.len())?;
        let mut bytes = [0_u8; GRAPH_IR_PACKAGE_BYTES];
        bytes[..8].copy_from_slice(&GRAPH_IR_MAGIC);
        put_u16(&mut bytes, 8, GRAPH_IR_VERSION);
        put_u16(&mut bytes, 10, GRAPH_IR_FLAGS);
        put_u32(
            &mut bytes,
            12,
            u32::try_from(initialized_len).map_err(|_| GraphIrError::Arithmetic)?,
        );
        put_u16(
            &mut bytes,
            16,
            u16::try_from(nodes.len()).map_err(|_| GraphIrError::NodeCount)?,
        );
        put_u16(
            &mut bytes,
            18,
            u16::try_from(channels.len()).map_err(|_| GraphIrError::ChannelCount)?,
        );
        put_u16(&mut bytes, 20, header.service_schedule.node_count);
        put_u16(&mut bytes, 22, header.realtime_schedule.node_count);
        let bridge_count = channels
            .iter()
            .filter(|channel| channel.owner == GraphIrChannelOwner::ServiceToRealtime)
            .count();
        put_u16(
            &mut bytes,
            24,
            u16::try_from(bridge_count).map_err(|_| GraphIrError::ChannelCount)?,
        );
        put_u32(&mut bytes, 28, header.total_state_bytes);
        put_u32(&mut bytes, 32, header.service_state_bytes);
        put_u32(&mut bytes, 36, header.realtime_state_bytes);
        put_u32(&mut bytes, 40, header.channel_storage_bytes);
        put_u32(&mut bytes, 44, header.bridge_storage_bytes);
        put_u32(&mut bytes, 48, header.service_schedule.clock_id);
        put_u32(&mut bytes, 52, header.realtime_schedule.clock_id);
        put_u64(&mut bytes, 56, header.service_schedule.period_cycles);
        put_u64(&mut bytes, 64, header.realtime_schedule.period_cycles);
        put_u64(&mut bytes, 72, header.service_schedule.total_wcet_cycles);
        put_u64(&mut bytes, 80, header.realtime_schedule.total_wcet_cycles);
        bytes[88..104].copy_from_slice(&header.device_id.0);
        bytes[104..136].copy_from_slice(&header.graph_digest.0);
        bytes[136..168].copy_from_slice(&header.implementation_digest.0);
        bytes[168..200].copy_from_slice(&header.capability_digest.0);
        bytes[200..232].copy_from_slice(&header.config_digest.0);
        put_u64(
            &mut bytes,
            232,
            header.service_schedule.executor_reserve_cycles,
        );
        put_u64(
            &mut bytes,
            240,
            header.realtime_schedule.executor_reserve_cycles,
        );

        let mut offset = GRAPH_IR_HEADER_BYTES;
        for node in nodes {
            encode_node(&mut bytes[offset..offset + GRAPH_IR_NODE_BYTES], *node);
            offset += GRAPH_IR_NODE_BYTES;
        }
        for channel in channels {
            encode_channel(
                &mut bytes[offset..offset + GRAPH_IR_CHANNEL_BYTES],
                *channel,
            );
            offset += GRAPH_IR_CHANNEL_BYTES;
        }
        let digest = sha256(&bytes[..GRAPH_IR_DIGEST_OFFSET]);
        bytes[GRAPH_IR_DIGEST_OFFSET..].copy_from_slice(&digest.0);
        Self::decode(bytes)
    }

    /// Decode and independently validate one exact fixed-size package.
    pub fn decode(bytes: [u8; GRAPH_IR_PACKAGE_BYTES]) -> Result<Self, GraphIrError> {
        let (header, summary, digest) = validate_package(&bytes)?;
        Ok(Self {
            bytes,
            header,
            summary,
            digest,
        })
    }

    /// Copy and decode an exact-size borrowed package.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, GraphIrError> {
        let bytes: [u8; GRAPH_IR_PACKAGE_BYTES] =
            bytes.try_into().map_err(|_| GraphIrError::Length)?;
        Self::decode(bytes)
    }

    /// Borrow all canonical fixed-size bytes, including zero padding and digest.
    pub const fn bytes(&self) -> &[u8; GRAPH_IR_PACKAGE_BYTES] {
        &self.bytes
    }

    /// Return the decoded package header.
    pub const fn header(&self) -> GraphIrHeader {
        self.header
    }

    /// Return independently reconstructed totals.
    pub const fn summary(&self) -> GraphIrSummary {
        self.summary
    }

    /// Return SHA-256 over the padded package prefix.
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    /// Iterate topologically ordered node records without allocation.
    pub fn nodes(&self) -> GraphIrNodes<'_> {
        GraphIrNodes {
            bytes: &self.bytes,
            next: 0,
            count: self.summary.node_count,
        }
    }

    /// Iterate target-ordered channel records without allocation.
    pub fn channels(&self) -> GraphIrChannels<'_> {
        GraphIrChannels {
            bytes: &self.bytes,
            node_count: self.summary.node_count,
            next: 0,
            count: self.summary.channel_count,
        }
    }

    /// Decode one already-admitted node by topological record index.
    pub fn node(&self, index: u16) -> Option<GraphIrNode> {
        if index >= self.summary.node_count {
            return None;
        }
        let offset = GRAPH_IR_HEADER_BYTES + usize::from(index) * GRAPH_IR_NODE_BYTES;
        decode_node(
            &self.bytes[offset..offset + GRAPH_IR_NODE_BYTES],
            usize::from(index),
        )
        .ok()
    }

    /// Decode one already-admitted channel by target-ordered record index.
    pub fn channel(&self, index: u16) -> Option<GraphIrChannel> {
        if index >= self.summary.channel_count {
            return None;
        }
        let offset = GRAPH_IR_HEADER_BYTES
            + usize::from(self.summary.node_count) * GRAPH_IR_NODE_BYTES
            + usize::from(index) * GRAPH_IR_CHANNEL_BYTES;
        decode_channel(
            &self.bytes[offset..offset + GRAPH_IR_CHANNEL_BYTES],
            usize::from(index),
        )
        .ok()
    }
}

/// Allocation-free deployed-node iterator.
pub struct GraphIrNodes<'a> {
    bytes: &'a [u8; GRAPH_IR_PACKAGE_BYTES],
    next: u16,
    count: u16,
}

impl Iterator for GraphIrNodes<'_> {
    type Item = GraphIrNode;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.count {
            return None;
        }
        let index = self.next;
        let offset = GRAPH_IR_HEADER_BYTES + usize::from(index) * GRAPH_IR_NODE_BYTES;
        self.next += 1;
        decode_node(
            &self.bytes[offset..offset + GRAPH_IR_NODE_BYTES],
            usize::from(index),
        )
        .ok()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = usize::from(self.count - self.next);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for GraphIrNodes<'_> {}

/// Allocation-free deployed-channel iterator.
pub struct GraphIrChannels<'a> {
    bytes: &'a [u8; GRAPH_IR_PACKAGE_BYTES],
    node_count: u16,
    next: u16,
    count: u16,
}

impl Iterator for GraphIrChannels<'_> {
    type Item = GraphIrChannel;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.count {
            return None;
        }
        let index = self.next;
        let offset = GRAPH_IR_HEADER_BYTES
            + usize::from(self.node_count) * GRAPH_IR_NODE_BYTES
            + usize::from(index) * GRAPH_IR_CHANNEL_BYTES;
        self.next += 1;
        decode_channel(
            &self.bytes[offset..offset + GRAPH_IR_CHANNEL_BYTES],
            usize::from(index),
        )
        .ok()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = usize::from(self.count - self.next);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for GraphIrChannels<'_> {}

/// Canonical encoding or independent firmware-admission failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphIrError {
    /// Input was not exactly [`GRAPH_IR_PACKAGE_BYTES`].
    Length,
    /// Package magic differed.
    Magic,
    /// Package version is unsupported.
    Version(u16),
    /// Reserved package flags were nonzero.
    Flags(u16),
    /// Initialized-body length disagreed with record counts.
    InitializedLength,
    /// Node count was zero, too large, or did not fit the body.
    NodeCount,
    /// Channel count was too large or did not fit the body.
    ChannelCount,
    /// A required identity was all zero.
    MissingIdentity(&'static str),
    /// Header totals or counts disagreed with records.
    HeaderMismatch(&'static str),
    /// Reserved header/body bytes were nonzero.
    Reserved,
    /// Bytes after initialized records and before the digest were nonzero.
    Padding,
    /// Stored SHA-256 did not match the padded prefix.
    Digest,
    /// A node record contradicted fixed opcode/domain/schedule/state rules.
    Node {
        /// Topological record index.
        index: usize,
        /// Rejected fact.
        aspect: &'static str,
    },
    /// A channel contradicted topology/queue/ownership rules.
    Channel {
        /// Target-ordered record index.
        index: usize,
        /// Rejected fact.
        aspect: &'static str,
    },
    /// Checked integer arithmetic overflowed.
    Arithmetic,
}

impl fmt::Display for GraphIrError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Length => formatter.write_str("graph IR package length is invalid"),
            Self::Magic => formatter.write_str("graph IR magic is invalid"),
            Self::Version(version) => {
                write!(formatter, "graph IR version {version} is unsupported")
            }
            Self::Flags(flags) => write!(formatter, "graph IR flags {flags:#06x} are unsupported"),
            Self::InitializedLength => formatter.write_str("graph IR initialized length differs"),
            Self::NodeCount => formatter.write_str("graph IR node count is invalid"),
            Self::ChannelCount => formatter.write_str("graph IR channel count is invalid"),
            Self::MissingIdentity(name) => write!(formatter, "graph IR {name} identity is missing"),
            Self::HeaderMismatch(name) => write!(formatter, "graph IR {name} header total differs"),
            Self::Reserved => formatter.write_str("graph IR reserved bytes are nonzero"),
            Self::Padding => formatter.write_str("graph IR padding is nonzero"),
            Self::Digest => formatter.write_str("graph IR digest differs"),
            Self::Node { index, aspect } => {
                write!(formatter, "graph IR node {index} has invalid {aspect}")
            }
            Self::Channel { index, aspect } => {
                write!(formatter, "graph IR channel {index} has invalid {aspect}")
            }
            Self::Arithmetic => formatter.write_str("graph IR arithmetic overflowed"),
        }
    }
}

fn validate_package(
    bytes: &[u8; GRAPH_IR_PACKAGE_BYTES],
) -> Result<(GraphIrHeader, GraphIrSummary, Digest), GraphIrError> {
    if bytes[..8] != GRAPH_IR_MAGIC {
        return Err(GraphIrError::Magic);
    }
    let version = get_u16(bytes, 8);
    if version != GRAPH_IR_VERSION {
        return Err(GraphIrError::Version(version));
    }
    let flags = get_u16(bytes, 10);
    if flags != GRAPH_IR_FLAGS {
        return Err(GraphIrError::Flags(flags));
    }
    let node_count = usize::from(get_u16(bytes, 16));
    let channel_count = usize::from(get_u16(bytes, 18));
    validate_counts(node_count, channel_count)?;
    let expected_initialized = initialized_len(node_count, channel_count)?;
    if usize::try_from(get_u32(bytes, 12)).ok() != Some(expected_initialized) {
        return Err(GraphIrError::InitializedLength);
    }
    if bytes[26..28].iter().any(|byte| *byte != 0)
        || bytes[248..GRAPH_IR_HEADER_BYTES]
            .iter()
            .any(|byte| *byte != 0)
    {
        return Err(GraphIrError::Reserved);
    }
    if bytes[expected_initialized..GRAPH_IR_DIGEST_OFFSET]
        .iter()
        .any(|byte| *byte != 0)
    {
        return Err(GraphIrError::Padding);
    }
    let digest = sha256(&bytes[..GRAPH_IR_DIGEST_OFFSET]);
    if bytes[GRAPH_IR_DIGEST_OFFSET..] != digest.0 {
        return Err(GraphIrError::Digest);
    }

    let device_id = DeviceId(array::<16>(bytes, 88));
    let graph_digest = Digest(array::<32>(bytes, 104));
    let implementation_digest = Digest(array::<32>(bytes, 136));
    let capability_digest = Digest(array::<32>(bytes, 168));
    let config_digest = Digest(array::<32>(bytes, 200));
    if device_id_is_zero(device_id) {
        return Err(GraphIrError::MissingIdentity("device"));
    }
    for (name, identity) in [
        ("graph", graph_digest),
        ("implementation", implementation_digest),
        ("capability", capability_digest),
        ("configuration", config_digest),
    ] {
        if identity.is_zero() {
            return Err(GraphIrError::MissingIdentity(name));
        }
    }

    let service_schedule = GraphIrSchedule {
        clock_id: get_u32(bytes, 48),
        period_cycles: get_u64(bytes, 56),
        total_wcet_cycles: get_u64(bytes, 72),
        executor_reserve_cycles: get_u64(bytes, 232),
        node_count: get_u16(bytes, 20),
    };
    let realtime_schedule = GraphIrSchedule {
        clock_id: get_u32(bytes, 52),
        period_cycles: get_u64(bytes, 64),
        total_wcet_cycles: get_u64(bytes, 80),
        executor_reserve_cycles: get_u64(bytes, 240),
        node_count: get_u16(bytes, 22),
    };
    validate_schedule(service_schedule, "service")?;
    validate_schedule(realtime_schedule, "realtime")?;
    if usize::from(service_schedule.node_count) + usize::from(realtime_schedule.node_count)
        != node_count
    {
        return Err(GraphIrError::HeaderMismatch("domain node count"));
    }

    let mut nodes = [None; MAX_GRAPH_IR_NODES];
    let mut service_state = 0_u32;
    let mut realtime_state = 0_u32;
    let mut service_wcet = 0_u64;
    let mut realtime_wcet = 0_u64;
    let mut service_nodes = 0_u16;
    let mut realtime_nodes = 0_u16;
    for index in 0..node_count {
        let offset = GRAPH_IR_HEADER_BYTES + index * GRAPH_IR_NODE_BYTES;
        let node = decode_node(&bytes[offset..offset + GRAPH_IR_NODE_BYTES], index)?;
        validate_node(
            index,
            node,
            service_schedule,
            realtime_schedule,
            &nodes[..index],
        )?;
        match node.domain {
            GraphIrDomain::Service => {
                if node.state_offset != service_state {
                    return Err(node_error(index, "Service state offset"));
                }
                service_state = service_state
                    .checked_add(node.state_bytes)
                    .ok_or(GraphIrError::Arithmetic)?;
                service_wcet = service_wcet
                    .checked_add(node.wcet_cycles)
                    .ok_or(GraphIrError::Arithmetic)?;
                service_nodes = service_nodes
                    .checked_add(1)
                    .ok_or(GraphIrError::Arithmetic)?;
            }
            GraphIrDomain::Realtime => {
                if node.state_offset != realtime_state {
                    return Err(node_error(index, "Realtime state offset"));
                }
                realtime_state = realtime_state
                    .checked_add(node.state_bytes)
                    .ok_or(GraphIrError::Arithmetic)?;
                realtime_wcet = realtime_wcet
                    .checked_add(node.wcet_cycles)
                    .ok_or(GraphIrError::Arithmetic)?;
                realtime_nodes = realtime_nodes
                    .checked_add(1)
                    .ok_or(GraphIrError::Arithmetic)?;
            }
        }
        nodes[index] = Some(node);
    }
    if service_nodes != service_schedule.node_count
        || realtime_nodes != realtime_schedule.node_count
        || service_wcet != service_schedule.total_wcet_cycles
        || realtime_wcet != realtime_schedule.total_wcet_cycles
    {
        return Err(GraphIrError::HeaderMismatch("domain schedule"));
    }

    let mut target_has_input = [false; MAX_GRAPH_IR_NODES];
    let mut source_has_output = [false; MAX_GRAPH_IR_NODES];
    let mut prior_target = None;
    let mut wire_ids = [0_u32; MAX_GRAPH_IR_CHANNELS];
    let mut owner_offsets = [0_u32; 3];
    let mut bridge_count = 0_u16;
    for index in 0..channel_count {
        let offset = GRAPH_IR_HEADER_BYTES
            + node_count * GRAPH_IR_NODE_BYTES
            + index * GRAPH_IR_CHANNEL_BYTES;
        let channel = decode_channel(&bytes[offset..offset + GRAPH_IR_CHANNEL_BYTES], index)?;
        validate_channel(
            index,
            channel,
            &nodes[..node_count],
            &mut target_has_input,
            &mut source_has_output,
            prior_target,
            &wire_ids[..index],
            &mut owner_offsets,
        )?;
        prior_target = Some(channel.target_node);
        wire_ids[index] = channel.graph_wire_id;
        if channel.owner == GraphIrChannelOwner::ServiceToRealtime {
            bridge_count = bridge_count
                .checked_add(1)
                .ok_or(GraphIrError::Arithmetic)?;
        }
    }
    for index in 0..node_count {
        let node = nodes[index].ok_or(GraphIrError::NodeCount)?;
        if node.opcode.has_input() && !target_has_input[index] {
            return Err(node_error(index, "required input"));
        }
        if !node.opcode.has_input() && target_has_input[index] {
            return Err(node_error(index, "unexpected input"));
        }
        if node.opcode.has_output() && !source_has_output[index] {
            return Err(node_error(index, "required output"));
        }
        if !node.opcode.has_output() && source_has_output[index] {
            return Err(node_error(index, "unexpected output"));
        }
    }

    let channel_storage = owner_offsets
        .iter()
        .try_fold(0_u32, |sum, bytes| sum.checked_add(*bytes))
        .ok_or(GraphIrError::Arithmetic)?;
    let bridge_storage = owner_offsets[GraphIrChannelOwner::ServiceToRealtime.index()];
    let total_state = service_state
        .checked_add(realtime_state)
        .ok_or(GraphIrError::Arithmetic)?;
    let header = GraphIrHeader {
        device_id,
        graph_digest,
        implementation_digest,
        capability_digest,
        config_digest,
        service_schedule,
        realtime_schedule,
        total_state_bytes: get_u32(bytes, 28),
        service_state_bytes: get_u32(bytes, 32),
        realtime_state_bytes: get_u32(bytes, 36),
        channel_storage_bytes: get_u32(bytes, 40),
        bridge_storage_bytes: get_u32(bytes, 44),
    };
    if header.total_state_bytes != total_state
        || header.service_state_bytes != service_state
        || header.realtime_state_bytes != realtime_state
    {
        return Err(GraphIrError::HeaderMismatch("state storage"));
    }
    if header.channel_storage_bytes != channel_storage
        || header.bridge_storage_bytes != bridge_storage
    {
        return Err(GraphIrError::HeaderMismatch("channel storage"));
    }
    if get_u16(bytes, 24) != bridge_count {
        return Err(GraphIrError::HeaderMismatch("bridge count"));
    }
    Ok((
        header,
        GraphIrSummary {
            node_count: u16::try_from(node_count).map_err(|_| GraphIrError::NodeCount)?,
            channel_count: u16::try_from(channel_count).map_err(|_| GraphIrError::ChannelCount)?,
            bridge_count,
            service_state_bytes: service_state,
            realtime_state_bytes: realtime_state,
            channel_storage_bytes: channel_storage,
            bridge_storage_bytes: bridge_storage,
        },
        digest,
    ))
}

fn validate_schedule(schedule: GraphIrSchedule, name: &'static str) -> Result<(), GraphIrError> {
    if schedule.node_count == 0 {
        if schedule != GraphIrSchedule::EMPTY {
            return Err(GraphIrError::HeaderMismatch(name));
        }
    } else if schedule.clock_id == 0
        || schedule.period_cycles == 0
        || schedule.total_wcet_cycles == 0
        || schedule.executor_reserve_cycles == 0
        || schedule
            .total_wcet_cycles
            .checked_add(schedule.executor_reserve_cycles)
            .is_none_or(|required| required > schedule.period_cycles)
    {
        return Err(GraphIrError::HeaderMismatch(name));
    }
    Ok(())
}

fn validate_node(
    index: usize,
    node: GraphIrNode,
    service: GraphIrSchedule,
    realtime: GraphIrSchedule,
    prior: &[Option<GraphIrNode>],
) -> Result<(), GraphIrError> {
    if node.graph_node_id == 0
        || prior
            .iter()
            .flatten()
            .any(|other| other.graph_node_id == node.graph_node_id)
    {
        return Err(node_error(index, "graph identity"));
    }
    let schedule = match node.domain {
        GraphIrDomain::Service => service,
        GraphIrDomain::Realtime => realtime,
    };
    if node.schedule_clock_id != schedule.clock_id
        || node.period_cycles != schedule.period_cycles
        || node.wcet_cycles == 0
        || node.wcet_cycles > node.period_cycles
    {
        return Err(node_error(index, "schedule"));
    }
    match (node.domain, node.opcode) {
        (GraphIrDomain::Service, GraphIrOpcode::BooleanStreamConstant)
            if node.state_bytes == 0 && node.parameter <= 1 =>
        {
            Ok(())
        }
        (GraphIrDomain::Realtime, GraphIrOpcode::BooleanLatest)
            if node.state_bytes == BOOLEAN_LATEST_STATE_BYTES && node.parameter == 0 =>
        {
            Ok(())
        }
        (GraphIrDomain::Realtime, GraphIrOpcode::BooleanStreamSink)
            if node.state_bytes == 0 && node.parameter == 0 =>
        {
            Ok(())
        }
        _ => Err(node_error(index, "opcode/domain/state/parameter")),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "independent channel admission keeps every reconstructed arena explicit"
)]
fn validate_channel(
    index: usize,
    channel: GraphIrChannel,
    nodes: &[Option<GraphIrNode>],
    target_has_input: &mut [bool; MAX_GRAPH_IR_NODES],
    source_has_output: &mut [bool; MAX_GRAPH_IR_NODES],
    prior_target: Option<u16>,
    prior_wire_ids: &[u32],
    owner_offsets: &mut [u32; 3],
) -> Result<(), GraphIrError> {
    if channel.graph_wire_id == 0 || prior_wire_ids.contains(&channel.graph_wire_id) {
        return Err(channel_error(index, "graph identity"));
    }
    let source_index = usize::from(channel.source_node);
    let target_index = usize::from(channel.target_node);
    if source_index >= nodes.len() || target_index >= nodes.len() || source_index >= target_index {
        return Err(channel_error(index, "topology"));
    }
    if prior_target.is_some_and(|prior| channel.target_node <= prior) {
        return Err(channel_error(index, "canonical target order"));
    }
    if target_has_input[target_index] {
        return Err(channel_error(index, "target ownership"));
    }
    let source = nodes[source_index].ok_or(GraphIrError::NodeCount)?;
    let target = nodes[target_index].ok_or(GraphIrError::NodeCount)?;
    let required_owner = match (source.domain, target.domain) {
        (GraphIrDomain::Service, GraphIrDomain::Service) => GraphIrChannelOwner::Service,
        (GraphIrDomain::Realtime, GraphIrDomain::Realtime) => GraphIrChannelOwner::Realtime,
        (GraphIrDomain::Service, GraphIrDomain::Realtime) => GraphIrChannelOwner::ServiceToRealtime,
        (GraphIrDomain::Realtime, GraphIrDomain::Service) => {
            return Err(channel_error(index, "Realtime-to-Service direction"));
        }
    };
    if channel.owner != required_owner {
        return Err(channel_error(index, "owner"));
    }
    if !source.opcode.has_output() || !target.opcode.has_input() {
        return Err(channel_error(index, "opcode endpoints"));
    }
    if target.domain == GraphIrDomain::Realtime && channel.full_policy != GraphIrFullPolicy::Fault {
        return Err(channel_error(index, "Realtime full policy"));
    }
    if channel.capacity == 0
        || channel.capacity > MAX_GRAPH_IR_QUEUE_ITEMS
        || channel.item_bytes != BOOLEAN_STREAM_ITEM_BYTES
    {
        return Err(channel_error(index, "queue shape"));
    }
    let required_bytes = channel
        .capacity
        .checked_mul(channel.item_bytes)
        .ok_or(GraphIrError::Arithmetic)?;
    let arena = channel.owner.index();
    if channel.storage_bytes != required_bytes || channel.storage_offset != owner_offsets[arena] {
        return Err(channel_error(index, "storage"));
    }
    owner_offsets[arena] = owner_offsets[arena]
        .checked_add(channel.storage_bytes)
        .ok_or(GraphIrError::Arithmetic)?;
    target_has_input[target_index] = true;
    source_has_output[source_index] = true;
    Ok(())
}

fn validate_counts(nodes: usize, channels: usize) -> Result<(), GraphIrError> {
    if nodes == 0 || nodes > MAX_GRAPH_IR_NODES {
        return Err(GraphIrError::NodeCount);
    }
    if channels > MAX_GRAPH_IR_CHANNELS {
        return Err(GraphIrError::ChannelCount);
    }
    initialized_len(nodes, channels)?;
    Ok(())
}

fn initialized_len(nodes: usize, channels: usize) -> Result<usize, GraphIrError> {
    let nodes = nodes
        .checked_mul(GRAPH_IR_NODE_BYTES)
        .ok_or(GraphIrError::Arithmetic)?;
    let channels = channels
        .checked_mul(GRAPH_IR_CHANNEL_BYTES)
        .ok_or(GraphIrError::Arithmetic)?;
    let length = GRAPH_IR_HEADER_BYTES
        .checked_add(nodes)
        .and_then(|value| value.checked_add(channels))
        .ok_or(GraphIrError::Arithmetic)?;
    if length > GRAPH_IR_DIGEST_OFFSET {
        return Err(GraphIrError::ChannelCount);
    }
    Ok(length)
}

fn encode_node(bytes: &mut [u8], node: GraphIrNode) {
    put_u32(bytes, 0, node.graph_node_id);
    bytes[4] = node.domain as u8;
    bytes[5] = node.opcode as u8;
    put_u32(bytes, 8, node.schedule_clock_id);
    put_u32(bytes, 12, node.state_offset);
    put_u32(bytes, 16, node.state_bytes);
    put_u64(bytes, 24, node.period_cycles);
    put_u64(bytes, 32, node.wcet_cycles);
    put_u64(bytes, 40, node.parameter);
}

fn decode_node(bytes: &[u8], index: usize) -> Result<GraphIrNode, GraphIrError> {
    if bytes[6..8].iter().any(|byte| *byte != 0) || bytes[20..24].iter().any(|byte| *byte != 0) {
        return Err(GraphIrError::Reserved);
    }
    Ok(GraphIrNode {
        graph_node_id: get_u32(bytes, 0),
        domain: GraphIrDomain::from_wire(bytes[4]).ok_or(GraphIrError::Node {
            index,
            aspect: "domain",
        })?,
        opcode: GraphIrOpcode::from_wire(bytes[5]).ok_or(GraphIrError::Node {
            index,
            aspect: "opcode",
        })?,
        schedule_clock_id: get_u32(bytes, 8),
        state_offset: get_u32(bytes, 12),
        state_bytes: get_u32(bytes, 16),
        period_cycles: get_u64(bytes, 24),
        wcet_cycles: get_u64(bytes, 32),
        parameter: get_u64(bytes, 40),
    })
}

fn encode_channel(bytes: &mut [u8], channel: GraphIrChannel) {
    put_u32(bytes, 0, channel.graph_wire_id);
    put_u16(bytes, 4, channel.source_node);
    put_u16(bytes, 6, channel.target_node);
    bytes[8] = channel.owner as u8;
    bytes[9] = channel.full_policy as u8;
    put_u32(bytes, 12, channel.capacity);
    put_u32(bytes, 16, channel.item_bytes);
    put_u32(bytes, 20, channel.storage_offset);
    put_u32(bytes, 24, channel.storage_bytes);
}

fn decode_channel(bytes: &[u8], index: usize) -> Result<GraphIrChannel, GraphIrError> {
    if bytes[10..12].iter().any(|byte| *byte != 0) || bytes[28..32].iter().any(|byte| *byte != 0) {
        return Err(GraphIrError::Reserved);
    }
    Ok(GraphIrChannel {
        graph_wire_id: get_u32(bytes, 0),
        source_node: get_u16(bytes, 4),
        target_node: get_u16(bytes, 6),
        owner: GraphIrChannelOwner::from_wire(bytes[8]).ok_or(GraphIrError::Channel {
            index,
            aspect: "owner",
        })?,
        full_policy: GraphIrFullPolicy::from_wire(bytes[9]).ok_or(GraphIrError::Channel {
            index,
            aspect: "full policy",
        })?,
        capacity: get_u32(bytes, 12),
        item_bytes: get_u32(bytes, 16),
        storage_offset: get_u32(bytes, 20),
        storage_bytes: get_u32(bytes, 24),
    })
}

const fn node_error(index: usize, aspect: &'static str) -> GraphIrError {
    GraphIrError::Node { index, aspect }
}

const fn channel_error(index: usize, aspect: &'static str) -> GraphIrError {
    GraphIrError::Channel { index, aspect }
}

fn sha256(bytes: &[u8]) -> Digest {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Digest(hasher.finalize().into())
}

fn device_id_is_zero(device_id: DeviceId) -> bool {
    device_id.0.iter().all(|byte| *byte == 0)
}

fn array<const N: usize>(bytes: &[u8], offset: usize) -> [u8; N] {
    let mut value = [0_u8; N];
    value.copy_from_slice(&bytes[offset..offset + N]);
    value
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn get_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(array(bytes, offset))
}

fn get_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(array(bytes, offset))
}

fn get_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(array(bytes, offset))
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alumina_storage::{ContentId, StoredObject};

    use super::*;

    fn digest(byte: u8) -> Digest {
        Digest([byte; 32])
    }

    fn fixture() -> (GraphIrHeader, [GraphIrNode; 3], [GraphIrChannel; 2]) {
        let service_schedule = GraphIrSchedule {
            clock_id: 10,
            period_cycles: 1_000,
            total_wcet_cycles: 20,
            executor_reserve_cycles: 100,
            node_count: 1,
        };
        let realtime_schedule = GraphIrSchedule {
            clock_id: 11,
            period_cycles: 2_000,
            total_wcet_cycles: 60,
            executor_reserve_cycles: 100,
            node_count: 2,
        };
        let nodes = [
            GraphIrNode {
                graph_node_id: 100,
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
                graph_node_id: 200,
                domain: GraphIrDomain::Realtime,
                opcode: GraphIrOpcode::BooleanLatest,
                schedule_clock_id: 11,
                period_cycles: 2_000,
                wcet_cycles: 40,
                state_offset: 0,
                state_bytes: BOOLEAN_LATEST_STATE_BYTES,
                parameter: 0,
            },
            GraphIrNode {
                graph_node_id: 300,
                domain: GraphIrDomain::Realtime,
                opcode: GraphIrOpcode::BooleanStreamSink,
                schedule_clock_id: 11,
                period_cycles: 2_000,
                wcet_cycles: 20,
                state_offset: BOOLEAN_LATEST_STATE_BYTES,
                state_bytes: 0,
                parameter: 0,
            },
        ];
        let channels = [
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
        ];
        (
            GraphIrHeader {
                device_id: DeviceId([1; 16]),
                graph_digest: digest(2),
                implementation_digest: digest(3),
                capability_digest: digest(4),
                config_digest: digest(5),
                service_schedule,
                realtime_schedule,
                total_state_bytes: BOOLEAN_LATEST_STATE_BYTES,
                service_state_bytes: 0,
                realtime_state_bytes: BOOLEAN_LATEST_STATE_BYTES,
                channel_storage_bytes: 3 * BOOLEAN_STREAM_ITEM_BYTES,
                bridge_storage_bytes: 2 * BOOLEAN_STREAM_ITEM_BYTES,
            },
            nodes,
            channels,
        )
    }

    #[test]
    fn canonical_package_replays_with_all_fixed_totals() {
        let (header, nodes, channels) = fixture();
        let package = GraphIrPackage::encode(header, &nodes, &channels).unwrap();
        let replay = GraphIrPackage::from_slice(package.bytes()).unwrap();
        assert_eq!(replay, package);
        assert_eq!(package.header(), header);
        assert_eq!(package.nodes().collect::<std::vec::Vec<_>>(), nodes);
        assert_eq!(package.channels().collect::<std::vec::Vec<_>>(), channels);
        assert_eq!(package.node(1), Some(nodes[1]));
        assert_eq!(package.node(3), None);
        assert_eq!(package.channel(0), Some(channels[0]));
        assert_eq!(package.channel(2), None);
        assert_eq!(
            package.summary(),
            GraphIrSummary {
                node_count: 3,
                channel_count: 2,
                bridge_count: 1,
                service_state_bytes: 0,
                realtime_state_bytes: BOOLEAN_LATEST_STATE_BYTES,
                channel_storage_bytes: 3 * BOOLEAN_STREAM_ITEM_BYTES,
                bridge_storage_bytes: 2 * BOOLEAN_STREAM_ITEM_BYTES,
            }
        );
        assert_eq!(
            package.digest(),
            sha256(&package.bytes()[..GRAPH_IR_DIGEST_OFFSET])
        );
        assert_eq!(
            package.digest(),
            Digest([
                0x09, 0xba, 0x7f, 0x44, 0x3c, 0xb6, 0xac, 0xbd, 0x82, 0xc4, 0x36, 0x94, 0x36, 0x53,
                0xfb, 0x55, 0xce, 0x2d, 0x20, 0x99, 0x2f, 0x76, 0x36, 0x32, 0xf8, 0x59, 0xe4, 0xf0,
                0x5f, 0xac, 0x58, 0x76,
            ])
        );
    }

    #[test]
    fn deployment_requests_and_core_commands_bind_both_package_digests() {
        let (header, nodes, channels) = fixture();
        let package = GraphIrPackage::encode(header, &nodes, &channels).unwrap();
        let publication = GraphPublication {
            transaction_id: 0x0102_0304_0506_0708,
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
        };
        let encoded_publication = publication.encode().unwrap();
        assert_eq!(
            GraphPublication::decode(&encoded_publication),
            Ok(publication)
        );

        let selection = GraphSelection {
            transaction_id: publication.transaction_id,
            content_digest: publication.content_digest(),
            package_digest: publication.package_digest,
        };
        assert_eq!(
            GraphSelection::decode(&selection.encode().unwrap()),
            Ok(selection)
        );

        let begin = CoreGraphCommand::begin(publication).unwrap();
        assert_eq!(
            CoreGraphCommand::decode(begin.encode().unwrap().as_bytes()),
            Ok(begin)
        );
        let data = CoreGraphCommand::data(
            publication,
            17,
            &package.bytes()[17..17 + MAX_CORE_GRAPH_DATA_BYTES],
        )
        .unwrap();
        let encoded_data = data.encode().unwrap();
        assert_eq!(encoded_data.as_bytes().len(), CORE_GRAPH_COMMAND_CAPACITY);
        assert_eq!(CoreGraphCommand::decode(encoded_data.as_bytes()), Ok(data));
        for command in [
            CoreGraphCommand::finish(publication).unwrap(),
            CoreGraphCommand::activate(publication).unwrap(),
            CoreGraphCommand::clear(publication).unwrap(),
            CoreGraphCommand::abort(publication).unwrap(),
            CoreGraphCommand::authorize(publication).unwrap(),
        ] {
            assert_eq!(
                CoreGraphCommand::decode(command.encode().unwrap().as_bytes()),
                Ok(command)
            );
        }

        let mut reserved = encoded_publication;
        reserved[10] = 1;
        assert_eq!(
            GraphPublication::decode(&reserved),
            Err(GraphDeploymentWireError::Reserved)
        );
        let mut wrong_kind = publication;
        wrong_kind.publication.object.kind = ObjectKind::OpaqueData;
        assert_eq!(wrong_kind.encode(), Err(GraphDeploymentWireError::Identity));
        assert_eq!(
            CoreGraphCommand::data(publication, 0, &[]),
            Err(GraphDeploymentWireError::DataLength)
        );
    }

    #[test]
    fn boolean_value_and_stream_item_have_one_deployment_local_encoding() {
        let value = GraphIrBooleanValue { value: true };
        assert_eq!(value.encode(), [1, 0, 0, 0, 1]);
        assert_eq!(GraphIrBooleanValue::from_slice(&value.encode()), Ok(value));

        let item = GraphIrBooleanStreamItem {
            value: false,
            source_tick: 0x0102_0304_0506_0708,
            sequence: 0x1112_1314_1516_1718,
        };
        let encoded = item.encode();
        assert_eq!(&encoded[..5], &[1, 0, 0, 0, 0]);
        assert_eq!(&encoded[5..13], &item.source_tick.to_le_bytes());
        assert_eq!(&encoded[13..], &item.sequence.to_le_bytes());
        assert_eq!(GraphIrBooleanStreamItem::from_slice(&encoded), Ok(item));

        let mut wrong_tag = encoded;
        wrong_tag[0] = 2;
        assert_eq!(
            GraphIrBooleanStreamItem::from_slice(&wrong_tag),
            Err(GraphIrValueError::TypeTag(2))
        );
        let mut wrong_boolean = encoded;
        wrong_boolean[4] = 2;
        assert_eq!(
            GraphIrBooleanStreamItem::from_slice(&wrong_boolean),
            Err(GraphIrValueError::Boolean(2))
        );
        assert_eq!(
            GraphIrBooleanStreamItem::from_slice(&encoded[..20]),
            Err(GraphIrValueError::Length)
        );
    }

    #[test]
    fn digest_padding_and_reserved_bytes_fail_independently() {
        let (header, nodes, channels) = fixture();
        let package = GraphIrPackage::encode(header, &nodes, &channels).unwrap();
        let mut changed = *package.bytes();
        changed[GRAPH_IR_HEADER_BYTES + 40] ^= 1;
        assert_eq!(GraphIrPackage::decode(changed), Err(GraphIrError::Digest));

        let mut padding = *package.bytes();
        padding[500] = 1;
        let digest = sha256(&padding[..GRAPH_IR_DIGEST_OFFSET]);
        padding[GRAPH_IR_DIGEST_OFFSET..].copy_from_slice(&digest.0);
        assert_eq!(GraphIrPackage::decode(padding), Err(GraphIrError::Padding));

        let mut reserved = *package.bytes();
        reserved[26] = 1;
        let digest = sha256(&reserved[..GRAPH_IR_DIGEST_OFFSET]);
        reserved[GRAPH_IR_DIGEST_OFFSET..].copy_from_slice(&digest.0);
        assert_eq!(
            GraphIrPackage::decode(reserved),
            Err(GraphIrError::Reserved)
        );
    }

    #[test]
    fn topology_schedule_state_and_realtime_loss_fail_closed() {
        let (header, mut nodes, channels) = fixture();
        nodes[1].state_bytes = 4;
        assert!(matches!(
            GraphIrPackage::encode(header, &nodes, &channels),
            Err(GraphIrError::Node { index: 1, .. })
        ));

        let (mut header, nodes, mut channels) = fixture();
        header.realtime_schedule.total_wcet_cycles = 2_001;
        assert_eq!(
            GraphIrPackage::encode(header, &nodes, &channels),
            Err(GraphIrError::HeaderMismatch("realtime"))
        );

        let (header, nodes, _) = fixture();
        channels[0].full_policy = GraphIrFullPolicy::DropOldest;
        assert!(matches!(
            GraphIrPackage::encode(header, &nodes, &channels),
            Err(GraphIrError::Channel { index: 0, .. })
        ));

        let (header, nodes, mut channels) = fixture();
        channels[1].source_node = 2;
        assert!(matches!(
            GraphIrPackage::encode(header, &nodes, &channels),
            Err(GraphIrError::Channel { index: 1, .. })
        ));
    }

    #[test]
    fn independent_decode_rejects_semantic_tamper_even_with_a_valid_digest() {
        let (header, nodes, channels) = fixture();
        let package = GraphIrPackage::encode(header, &nodes, &channels).unwrap();

        let mut bad_state = *package.bytes();
        let latest_state_bytes = GRAPH_IR_HEADER_BYTES + GRAPH_IR_NODE_BYTES + 16;
        put_u32(&mut bad_state, latest_state_bytes, 4);
        rehash(&mut bad_state);
        assert_eq!(
            GraphIrPackage::decode(bad_state),
            Err(GraphIrError::Node {
                index: 1,
                aspect: "opcode/domain/state/parameter",
            })
        );

        let mut lossy_bridge = *package.bytes();
        let first_channel = GRAPH_IR_HEADER_BYTES + 3 * GRAPH_IR_NODE_BYTES;
        lossy_bridge[first_channel + 9] = GraphIrFullPolicy::DropOldest as u8;
        rehash(&mut lossy_bridge);
        assert_eq!(
            GraphIrPackage::decode(lossy_bridge),
            Err(GraphIrError::Channel {
                index: 0,
                aspect: "Realtime full policy",
            })
        );

        let mut wrong_total = *package.bytes();
        put_u32(&mut wrong_total, 40, 64);
        rehash(&mut wrong_total);
        assert_eq!(
            GraphIrPackage::decode(wrong_total),
            Err(GraphIrError::HeaderMismatch("channel storage"))
        );

        let mut unbound = *package.bytes();
        unbound[200..232].fill(0);
        rehash(&mut unbound);
        assert_eq!(
            GraphIrPackage::decode(unbound),
            Err(GraphIrError::MissingIdentity("configuration"))
        );
    }

    #[test]
    fn every_nonexact_package_length_rejects_before_decode() {
        let (header, nodes, channels) = fixture();
        let package = GraphIrPackage::encode(header, &nodes, &channels).unwrap();
        for length in 0..GRAPH_IR_PACKAGE_BYTES {
            assert_eq!(
                GraphIrPackage::from_slice(&package.bytes()[..length]),
                Err(GraphIrError::Length),
                "prefix {length} was accepted"
            );
        }
        let mut trailing = package.bytes().to_vec();
        trailing.push(0);
        assert_eq!(
            GraphIrPackage::from_slice(&trailing),
            Err(GraphIrError::Length)
        );
    }

    fn rehash(bytes: &mut [u8; GRAPH_IR_PACKAGE_BYTES]) {
        let digest = sha256(&bytes[..GRAPH_IR_DIGEST_OFFSET]);
        bytes[GRAPH_IR_DIGEST_OFFSET..].copy_from_slice(&digest.0);
    }
}
