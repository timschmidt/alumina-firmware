//! Power-cut-safe append-only cache media over an asynchronous block device.
//!
//! The media region is deliberately not a filesystem. Two alternating hashed
//! anchors identify the exact committed tail of a hash-chained record log. A
//! record becomes visible only after its data, commit block, and replacement
//! anchor have each crossed an explicit device synchronization barrier.

use alumina_protocol::Digest;

use crate::{
    CacheLimits, ChunkUploadHeader, ContentHasher, Error as StorageError, FinalizeUploadRequest,
    ManifestHasher, MutationContext, PublishedObject, UploadCoordinator, UploadId, UploadPlan,
    UploadProgress, sha256,
};

/// Sector size required by the V1 cache-media schema and SD block adapter.
pub const MEDIA_BLOCK_BYTES: usize = 512;
/// Alternating anchor sectors at relative blocks zero and one.
pub const MEDIA_ANCHOR_BLOCKS: u64 = 2;
/// Largest chunk admitted by the V1 on-media and HTTP service boundary.
pub const MAX_MEDIA_CHUNK_BYTES: usize = 1_024;
/// Largest record payload: one chunk prefix followed by the largest chunk.
pub const MAX_MEDIA_RECORD_PAYLOAD_BYTES: usize =
    ChunkUploadHeader::WIRE_LEN + MAX_MEDIA_CHUNK_BYTES;
/// Current exact cache-media schema.
pub const CACHE_MEDIA_VERSION: u16 = 1;

const ANCHOR_MAGIC: [u8; 8] = *b"ALMCACH1";
const RECORD_MAGIC: [u8; 8] = *b"ALMREC01";
const COMMIT_MAGIC: [u8; 8] = *b"ALMCOM01";
const CONFIGURATION_SELECTION_MAGIC: [u8; 8] = *b"ALMCAS01";
const ANCHOR_HASH_OFFSET: usize = MEDIA_BLOCK_BYTES - 32;
const PUBLICATION_WIRE_LEN: usize = UploadPlan::WIRE_LEN + 8;
const CONFIGURATION_SELECTION_WIRE_LEN: usize = 96;
const MINIMUM_REGION_BLOCKS: u64 = MEDIA_ANCHOR_BLOCKS + 3;

/// One exact block transferred to or from an SD-class device.
pub type MediaBlock = [u8; MEDIA_BLOCK_BYTES];

/// Asynchronous, ordered block-device contract used only by core 0.
///
/// `sync` must not complete until every preceding successful write is no longer
/// busy and is ordered before a subsequent write. Drivers may provide stronger
/// guarantees. The log never treats a successful `write_block` alone as durable.
#[allow(
    async_fn_in_trait,
    reason = "the single-core no_std storage actor deliberately does not require Send futures"
)]
pub trait AsyncBlockDevice {
    /// Transport/media failure reported by the concrete card adapter.
    type Error;

    /// Number of addressable 512-byte blocks currently exposed by the device.
    fn block_count(&self) -> u64;

    /// Reads exactly one complete block.
    async fn read_block(&mut self, block: u64, output: &mut MediaBlock) -> Result<(), Self::Error>;

    /// Writes exactly one complete block.
    async fn write_block(&mut self, block: u64, data: &MediaBlock) -> Result<(), Self::Error>;

    /// Establishes a durability/order barrier for preceding writes.
    async fn sync(&mut self) -> Result<(), Self::Error>;
}

/// Explicit device subregion reserved for the Alumina cache schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaRegion {
    /// Absolute first 512-byte device block.
    pub start_block: u64,
    /// Exact number of blocks owned by this cache region.
    pub block_count: u64,
}

impl MediaRegion {
    pub(crate) fn validate(self, device_blocks: u64) -> Result<(), MediaGeometryError> {
        if self.block_count < MINIMUM_REGION_BLOCKS {
            return Err(MediaGeometryError::RegionTooSmall);
        }
        let end = self
            .start_block
            .checked_add(self.block_count)
            .ok_or(MediaGeometryError::AddressOverflow)?;
        if end > device_blocks {
            return Err(MediaGeometryError::OutsideDevice);
        }
        Ok(())
    }

    fn absolute(self, relative: u64) -> Result<u64, MediaGeometryError> {
        if relative >= self.block_count {
            return Err(MediaGeometryError::OutsideRegion);
        }
        self.start_block
            .checked_add(relative)
            .ok_or(MediaGeometryError::AddressOverflow)
    }
}

/// Nonzero format identity generated when a region is explicitly initialized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaId(pub [u8; 16]);

impl MediaId {
    /// Rejects the zero sentinel used by erased/unformatted media.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, MediaGeometryError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(MediaGeometryError::ZeroMediaId)
    }
}

/// Coarse backend state safe to expose in authenticated status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaAvailability {
    /// Device exists but no valid region has been mounted.
    Detached,
    /// A valid V1 anchor and its complete committed log were replayed.
    Ready,
    /// A write/sync failure made the in-memory tail untrustworthy; remount first.
    Faulted,
}

/// Bounded, allocation-free cache status derived from the selected anchor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaStatus {
    /// Current backend state.
    pub availability: MediaAvailability,
    /// Stable identity selected at explicit formatting time, when mounted.
    pub media_id: Option<MediaId>,
    /// Total blocks owned by the configured region.
    pub total_blocks: u64,
    /// First uncommitted relative block, including the two anchors.
    pub committed_blocks: u64,
    /// Blocks still available without compaction.
    pub free_blocks: u64,
    /// Last committed record sequence.
    pub last_sequence: u64,
    /// Number of publication records replayed from this log generation.
    pub published_objects: u32,
    /// Active resumable upload, if any.
    pub upload: Option<UploadProgress>,
    /// True when mount recovered through one valid anchor while its peer was torn.
    pub degraded_anchor: bool,
}

/// Metadata for one independently verified chunk copied into a caller buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishedChunk {
    /// Sequential chunk index from the canonical publication manifest.
    pub index: u32,
    /// Exact initialized prefix length in the caller's chunk buffer.
    pub byte_len: u32,
    /// SHA-256 identity verified over exactly those initialized bytes.
    pub content: crate::ContentId,
}

/// Exact immutable configuration publication named by a durable activation.
///
/// The operation identity is retained for audit and idempotence. Boot recovery
/// must still reopen and independently revalidate `publication` on both cores
/// before any resource described by it can become active.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DurableConfigurationSelection {
    transaction_id: u64,
    publication: PublishedObject,
}

impl DurableConfigurationSelection {
    /// Constructs a selection only for a nonempty, SHA-256-addressed machine
    /// configuration and a nonzero operation identity.
    pub fn new(transaction_id: u64, publication: PublishedObject) -> Result<Self, StorageError> {
        let selection = Self {
            transaction_id,
            publication,
        };
        selection.validate()?;
        Ok(selection)
    }

    /// Operation identity that prepared and committed this selection.
    pub const fn transaction_id(self) -> u64 {
        self.transaction_id
    }

    /// Exact typed immutable object and manifest that must be reopened at boot.
    pub const fn publication(self) -> PublishedObject {
        self.publication
    }

    fn validate(self) -> Result<(), StorageError> {
        if self.transaction_id == 0
            || self.publication.object.kind != crate::ObjectKind::MachineConfiguration
            || self.publication.object.byte_len == 0
            || !self.publication.object.content.is_valid()
            || !self.publication.manifest.is_valid()
        {
            return Err(StorageError::ConfigurationTransition);
        }
        Ok(())
    }
}

/// Safety-relevant durable configuration transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ConfigurationTransitionAction {
    /// Replace the active selection after both cores accepted the candidate.
    Activate = 1,
    /// Remove the exact currently active selection after core 1 deactivated it.
    Clear = 2,
}

impl ConfigurationTransitionAction {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Activate),
            2 => Some(Self::Clear),
            _ => None,
        }
    }
}

/// One prepare/commit operation in the configuration-selection journal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigurationTransition {
    action: ConfigurationTransitionAction,
    selection: DurableConfigurationSelection,
}

impl ConfigurationTransition {
    /// Prepares selection of an independently validated publication.
    pub fn activate(selection: DurableConfigurationSelection) -> Self {
        Self {
            action: ConfigurationTransitionAction::Activate,
            selection,
        }
    }

    /// Prepares removal of the exact active publication. The selection's
    /// transaction identifies the clear operation, not the earlier activation.
    pub fn clear(selection: DurableConfigurationSelection) -> Self {
        Self {
            action: ConfigurationTransitionAction::Clear,
            selection,
        }
    }

    /// Requested transition action.
    pub const fn action(self) -> ConfigurationTransitionAction {
        self.action
    }

    /// Exact publication and operation identity bound into the transition.
    pub const fn selection(self) -> DurableConfigurationSelection {
        self.selection
    }

    fn validate(self) -> Result<(), StorageError> {
        self.selection.validate()
    }
}

/// Replayed configuration selector state from the chosen committed anchor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigurationJournal {
    /// Last completely committed selection, if any.
    pub active: Option<DurableConfigurationSelection>,
    /// A durable prepare without its matching commit. This never changes
    /// `active` and is safe to supersede after boot.
    pub pending: Option<ConfigurationTransition>,
}

/// Linear, allocation-free cursor over one exact immutable publication.
///
/// Fields are private so callers cannot skip records, substitute media, or
/// claim aggregate verification before the publication record is reached.
pub struct PublishedReader {
    plan: UploadPlan,
    media_id: MediaId,
    region: MediaRegion,
    opened_generation: u64,
    opened_tail: u64,
    begin_block: u64,
    publish_block: u64,
    publish_header: RecordHeader,
    publish_digest: Digest,
    next_record_block: u64,
    next_sequence: u64,
    previous_digest: Digest,
    next_chunk: u32,
    accepted_bytes: u64,
    object: ContentHasher,
    manifest: ManifestHasher,
    complete: bool,
}

impl PublishedReader {
    /// Exact declaration recovered from the selected publication record.
    pub const fn plan(&self) -> UploadPlan {
        self.plan
    }

    /// Typed immutable identity selected when this cursor was opened.
    pub const fn published(&self) -> PublishedObject {
        PublishedObject {
            object: self.plan.object,
            manifest: self.plan.manifest,
        }
    }

    /// Next chunk index that has not yet passed record and content verification.
    pub const fn next_chunk(&self) -> u32 {
        self.next_chunk
    }

    /// Exact number of object bytes already released to the caller.
    pub const fn accepted_bytes(&self) -> u64 {
        self.accepted_bytes
    }

    /// True only after the final chunk and its bound publication record passed.
    pub const fn is_complete(&self) -> bool {
        self.complete
    }
}

/// Geometry/configuration rejection before any log record is trusted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaGeometryError {
    /// Region cannot hold two anchors plus one smallest record.
    RegionTooSmall,
    /// Region end or absolute address overflowed `u64`.
    AddressOverflow,
    /// Configured region extends beyond the detected device.
    OutsideDevice,
    /// Relative address escaped the owned region.
    OutsideRegion,
    /// Explicit format identity used the erased zero sentinel.
    ZeroMediaId,
    /// Chunk policy exceeds the fixed V1 record workspace.
    ChunkLimit,
}

/// Committed-media structural failure. No operation may arm from this state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaCorruption {
    /// Neither anchor was blank or structurally recoverable.
    Anchor,
    /// Two valid anchors describe different formatted media.
    ConflictingAnchors,
    /// Anchor tail/sequence/digest facts are internally inconsistent.
    AnchorState,
    /// Record header was malformed or noncanonical.
    RecordHeader,
    /// Record payload padding was nonzero.
    RecordPadding,
    /// Commit block did not bind the preceding record.
    RecordCommit,
    /// Sequence or previous-digest chain diverged.
    RecordChain,
    /// Log replay violated upload/storage invariants.
    Replay,
    /// Selected anchor did not match the exact replayed tail.
    Tail,
    /// A published-object cursor diverged from its bound media or record stream.
    PublishedReader,
}

/// Cache backend failure, preserving the concrete device error without boxing.
#[derive(Debug)]
pub enum MediaError<E> {
    /// Card/bus adapter failed.
    Device(E),
    /// Region or compile-time policy was impossible.
    Geometry(MediaGeometryError),
    /// Both anchors are erased; explicit formatting is required.
    Unformatted,
    /// Hash/chain/schema validation failed for committed state.
    Corrupt(MediaCorruption),
    /// Operation requires a successfully mounted region.
    NotMounted,
    /// A preceding write failed; remount before retrying.
    Faulted,
    /// Append-only region lacks room for the next complete record.
    Full {
        /// Complete record blocks required.
        required: u64,
        /// Blocks following the committed tail.
        available: u64,
    },
    /// No committed publication exactly matched the requested typed identity.
    PublishedNotFound,
    /// Existing upload/safety/content invariant rejected the operation.
    Storage(StorageError),
    /// Sequence, generation, or publication counter overflowed.
    Arithmetic,
}

impl<E> From<StorageError> for MediaError<E> {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl<E> From<MediaGeometryError> for MediaError<E> {
    fn from(error: MediaGeometryError) -> Self {
        Self::Geometry(error)
    }
}

#[derive(Clone, Copy)]
struct Anchor {
    slot: u8,
    generation: u64,
    committed_tail: u64,
    last_sequence: u64,
    region_blocks: u64,
    media_id: MediaId,
    limits: CacheLimits,
    last_record_digest: Digest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RecordHeader {
    kind: RecordKind,
    sequence: u64,
    payload_len: usize,
    record_blocks: u64,
    upload_id: UploadId,
    previous_digest: Digest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum RecordKind {
    Begin = 1,
    Chunk = 2,
    Publish = 3,
    Abort = 4,
    ConfigurationPrepare = 5,
    ConfigurationCommit = 6,
}

impl RecordKind {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Begin),
            2 => Some(Self::Chunk),
            3 => Some(Self::Publish),
            4 => Some(Self::Abort),
            5 => Some(Self::ConfigurationPrepare),
            6 => Some(Self::ConfigurationCommit),
            _ => None,
        }
    }
}

#[derive(Clone)]
struct ActiveHashes {
    plan: UploadPlan,
    begin_block: u64,
    object: ContentHasher,
    manifest: ManifestHasher,
}

struct ReadyState {
    anchor: Anchor,
    uploads: UploadCoordinator,
    hashes: Option<ActiveHashes>,
    published_objects: u32,
    configuration: ConfigurationJournal,
    degraded_anchor: bool,
}

#[derive(Clone, Copy)]
struct PublishedLocation {
    plan: UploadPlan,
    begin_block: u64,
    publish_block: u64,
    publish_header: RecordHeader,
    publish_digest: Digest,
}

#[allow(
    clippy::large_enum_variant,
    reason = "fixed no-alloc state is intentional; boxing would move safety state onto the heap"
)]
enum MountState {
    Detached,
    Ready(ReadyState),
    Faulted,
}

/// Core-0-owned append-only cache over a concrete async block device.
pub struct CacheMedia<D> {
    device: D,
    region: MediaRegion,
    limits: CacheLimits,
    state: MountState,
}

impl<D> CacheMedia<D>
where
    D: AsyncBlockDevice,
{
    /// Creates a detached backend without reading or mutating the device.
    pub const fn new(device: D, region: MediaRegion, limits: CacheLimits) -> Self {
        Self {
            device,
            region,
            limits,
            state: MountState::Detached,
        }
    }

    /// Returns the owned device, including after a failed mount/write.
    pub fn into_device(self) -> D {
        self.device
    }

    /// Exact physical device capacity currently visible through the adapter.
    pub fn device_block_count(&self) -> u64 {
        self.device.block_count()
    }

    /// Exact configured raw-media interval.
    pub const fn region(&self) -> MediaRegion {
        self.region
    }

    /// Current state without performing I/O.
    pub fn status(&self) -> MediaStatus {
        match &self.state {
            MountState::Detached => MediaStatus {
                availability: MediaAvailability::Detached,
                media_id: None,
                total_blocks: self.region.block_count,
                committed_blocks: 0,
                free_blocks: self.region.block_count,
                last_sequence: 0,
                published_objects: 0,
                upload: None,
                degraded_anchor: false,
            },
            MountState::Faulted => MediaStatus {
                availability: MediaAvailability::Faulted,
                media_id: None,
                total_blocks: self.region.block_count,
                committed_blocks: 0,
                free_blocks: 0,
                last_sequence: 0,
                published_objects: 0,
                upload: None,
                degraded_anchor: false,
            },
            MountState::Ready(ready) => MediaStatus {
                availability: MediaAvailability::Ready,
                media_id: Some(ready.anchor.media_id),
                total_blocks: self.region.block_count,
                committed_blocks: ready.anchor.committed_tail,
                free_blocks: self
                    .region
                    .block_count
                    .saturating_sub(ready.anchor.committed_tail),
                last_sequence: ready.anchor.last_sequence,
                published_objects: ready.published_objects,
                upload: ready.uploads.checkpoint().map(checkpoint_progress),
                degraded_anchor: ready.degraded_anchor,
            },
        }
    }

    /// Explicitly initializes this exact region, invalidating prior anchors.
    ///
    /// This is destructive and is intentionally not called by `mount` or boot.
    pub async fn format(&mut self, media_id: MediaId) -> Result<(), MediaError<D::Error>> {
        self.validate_configuration()?;
        self.state = MountState::Detached;
        let blank = [0_u8; MEDIA_BLOCK_BYTES];
        self.write_absolute(self.region.absolute(0)?, &blank)
            .await?;
        self.write_absolute(self.region.absolute(1)?, &blank)
            .await?;
        self.sync_device().await?;

        let anchor = Anchor {
            slot: 0,
            generation: 1,
            committed_tail: MEDIA_ANCHOR_BLOCKS,
            last_sequence: 0,
            region_blocks: self.region.block_count,
            media_id,
            limits: self.limits,
            last_record_digest: Digest::ZERO,
        };
        let encoded = encode_anchor(anchor);
        self.write_absolute(self.region.absolute(0)?, &encoded)
            .await?;
        self.sync_device().await?;
        self.state = MountState::Ready(ReadyState {
            anchor,
            uploads: UploadCoordinator::new(),
            hashes: None,
            published_objects: 0,
            configuration: ConfigurationJournal {
                active: None,
                pending: None,
            },
            degraded_anchor: false,
        });
        Ok(())
    }

    /// Selects the newest valid anchor and replays its exact committed chain.
    pub async fn mount(&mut self) -> Result<MediaStatus, MediaError<D::Error>> {
        self.validate_configuration()?;
        self.state = MountState::Detached;

        let mut first = [0_u8; MEDIA_BLOCK_BYTES];
        let mut second = [0_u8; MEDIA_BLOCK_BYTES];
        self.read_absolute(self.region.absolute(0)?, &mut first)
            .await?;
        self.read_absolute(self.region.absolute(1)?, &mut second)
            .await?;

        let decoded_first = decode_anchor(&first, 0, self.region, self.limits);
        let decoded_second = decode_anchor(&second, 1, self.region, self.limits);
        let (anchor, degraded_anchor) =
            select_anchor(decoded_first, decoded_second).map_err(MediaError::Corrupt)?;
        let Some(anchor) = anchor else {
            return Err(MediaError::Unformatted);
        };

        let mut replay = ReplayState::new();
        let mut cursor = MEDIA_ANCHOR_BLOCKS;
        let mut expected_sequence = 1_u64;
        let mut previous_digest = Digest::ZERO;
        let mut header_block = [0_u8; MEDIA_BLOCK_BYTES];
        let mut payload = [0_u8; MAX_MEDIA_RECORD_PAYLOAD_BYTES];
        while cursor < anchor.committed_tail {
            self.read_relative(cursor, &mut header_block).await?;
            let header = decode_record_header(&header_block).map_err(MediaError::Corrupt)?;
            if header.sequence != expected_sequence
                || header.previous_digest != previous_digest
                || header
                    .record_blocks
                    .checked_add(cursor)
                    .is_none_or(|end| end > anchor.committed_tail)
            {
                return Err(MediaError::Corrupt(MediaCorruption::RecordChain));
            }
            payload.fill(0);
            let digest = self
                .read_and_verify_record(cursor, &header_block, header, &mut payload)
                .await?;
            replay
                .apply(header, cursor, &payload[..header.payload_len], self.limits)
                .map_err(MediaError::Corrupt)?;
            cursor = cursor
                .checked_add(header.record_blocks)
                .ok_or(MediaError::Arithmetic)?;
            expected_sequence = expected_sequence
                .checked_add(1)
                .ok_or(MediaError::Arithmetic)?;
            previous_digest = digest;
        }
        if cursor != anchor.committed_tail
            || expected_sequence.wrapping_sub(1) != anchor.last_sequence
            || previous_digest != anchor.last_record_digest
        {
            return Err(MediaError::Corrupt(MediaCorruption::Tail));
        }

        self.state = MountState::Ready(ReadyState {
            anchor,
            uploads: replay.uploads,
            hashes: replay.hashes,
            published_objects: replay.published_objects,
            configuration: replay.configuration,
            degraded_anchor,
        });
        Ok(self.status())
    }

    /// Returns the exact committed and prepared configuration selector state.
    ///
    /// A caller must treat `pending` as inert. Only `active` is a boot recovery
    /// candidate, and even it requires reopening and independent validation.
    pub fn configuration_journal(&self) -> Result<ConfigurationJournal, MediaError<D::Error>> {
        Ok(self.ready()?.configuration)
    }

    /// Durably records intent to activate or clear one exact configuration.
    ///
    /// Preparing never changes the committed active selection. An exact retry
    /// is idempotent; another valid prepare safely supersedes an orphaned one.
    pub async fn prepare_configuration_transition(
        &mut self,
        transition: ConfigurationTransition,
        context: MutationContext,
    ) -> Result<ConfigurationJournal, MediaError<D::Error>> {
        context.validate()?;
        transition.validate()?;
        let ready = self.ready()?;
        if ready.uploads.checkpoint().is_some() {
            return Err(StorageError::ConfigurationTransition.into());
        }
        let current = ready.configuration;
        validate_configuration_transition(current.active, transition)?;
        if current.pending == Some(transition) {
            return Ok(current);
        }

        if transition.action == ConfigurationTransitionAction::Activate {
            // Resolve the exact typed publication before durable intent can be
            // recorded. Complete byte validation remains the responsibility of
            // the two configuration validators immediately before this call.
            let _ = self
                .open_published(transition.selection.publication)
                .await?;
        }
        let payload = encode_configuration_transition(transition);
        self.append_record(
            RecordKind::ConfigurationPrepare,
            UploadId(transition.selection.transaction_id),
            &payload,
        )
        .await?;
        self.ready_mut()?.configuration.pending = Some(transition);
        Ok(self.ready()?.configuration)
    }

    /// Commits exactly the prepared configuration transition.
    ///
    /// The matching commit record is the only event that changes replayed
    /// active state, making every power cut resolve to the complete old or new
    /// selection.
    pub async fn commit_configuration_transition(
        &mut self,
        transition: ConfigurationTransition,
        context: MutationContext,
    ) -> Result<ConfigurationJournal, MediaError<D::Error>> {
        context.validate()?;
        transition.validate()?;
        let ready = self.ready()?;
        if ready.uploads.checkpoint().is_some() {
            return Err(StorageError::ConfigurationTransition.into());
        }
        let current = ready.configuration;
        if current.pending != Some(transition) {
            return Err(StorageError::ConfigurationTransition.into());
        }
        let payload = encode_configuration_transition(transition);
        self.append_record(
            RecordKind::ConfigurationCommit,
            UploadId(transition.selection.transaction_id),
            &payload,
        )
        .await?;
        let ready = self.ready_mut()?;
        apply_configuration_commit(&mut ready.configuration, transition);
        Ok(ready.configuration)
    }

    /// Locates the newest publication with this exact typed object and manifest.
    ///
    /// Lookup revalidates the complete committed chain without allocating a
    /// directory. A miss is benign. Device or integrity failures latch this
    /// media instance faulted so no later operation can consume uncertain data.
    pub async fn open_published(
        &mut self,
        expected: PublishedObject,
    ) -> Result<PublishedReader, MediaError<D::Error>> {
        let result = self.open_published_inner(expected).await;
        if matches!(&result, Err(MediaError::Device(_) | MediaError::Corrupt(_))) {
            self.state = MountState::Faulted;
        }
        result
    }

    /// Copies and verifies the next chunk of a publication into fixed caller RAM.
    ///
    /// Only `chunk.byte_len` bytes are object data. The remainder is zeroed on
    /// success. The final chunk is not released until the aggregate object hash,
    /// canonical manifest, and original publication record all match.
    pub async fn read_next_published(
        &mut self,
        reader: &mut PublishedReader,
        output: &mut [u8; MAX_MEDIA_CHUNK_BYTES],
    ) -> Result<Option<PublishedChunk>, MediaError<D::Error>> {
        let result = self.read_next_published_inner(reader, output).await;
        if matches!(&result, Err(MediaError::Device(_) | MediaError::Corrupt(_))) {
            self.state = MountState::Faulted;
        }
        result
    }

    async fn open_published_inner(
        &mut self,
        expected: PublishedObject,
    ) -> Result<PublishedReader, MediaError<D::Error>> {
        let anchor = self.ready()?.anchor;
        let mut replay = ReplayState::new();
        let mut selected = None;
        let mut cursor = MEDIA_ANCHOR_BLOCKS;
        let mut expected_sequence = 1_u64;
        let mut previous_digest = Digest::ZERO;
        let mut header_block = [0_u8; MEDIA_BLOCK_BYTES];
        let mut payload = [0_u8; MAX_MEDIA_RECORD_PAYLOAD_BYTES];

        while cursor < anchor.committed_tail {
            self.read_relative(cursor, &mut header_block).await?;
            let header = decode_record_header(&header_block).map_err(MediaError::Corrupt)?;
            if header.sequence != expected_sequence
                || header.previous_digest != previous_digest
                || header
                    .record_blocks
                    .checked_add(cursor)
                    .is_none_or(|end| end > anchor.committed_tail)
            {
                return Err(MediaError::Corrupt(MediaCorruption::RecordChain));
            }
            payload.fill(0);
            let digest = self
                .read_and_verify_record(cursor, &header_block, header, &mut payload)
                .await?;
            replay
                .apply(header, cursor, &payload[..header.payload_len], self.limits)
                .map_err(MediaError::Corrupt)?;
            if header.kind == RecordKind::Publish {
                let (plan, begin_block) =
                    decode_publication(&payload[..header.payload_len], self.limits)
                        .map_err(MediaError::Corrupt)?;
                if plan.object == expected.object && plan.manifest == expected.manifest {
                    selected = Some(PublishedLocation {
                        plan,
                        begin_block,
                        publish_block: cursor,
                        publish_header: header,
                        publish_digest: digest,
                    });
                }
            }
            cursor = cursor
                .checked_add(header.record_blocks)
                .ok_or(MediaError::Corrupt(MediaCorruption::RecordChain))?;
            expected_sequence = expected_sequence
                .checked_add(1)
                .ok_or(MediaError::Corrupt(MediaCorruption::RecordChain))?;
            previous_digest = digest;
        }
        if cursor != anchor.committed_tail
            || expected_sequence.wrapping_sub(1) != anchor.last_sequence
            || previous_digest != anchor.last_record_digest
        {
            return Err(MediaError::Corrupt(MediaCorruption::Tail));
        }

        let selected = selected.ok_or(MediaError::PublishedNotFound)?;
        if selected.begin_block < MEDIA_ANCHOR_BLOCKS
            || selected.begin_block >= selected.publish_block
        {
            return Err(MediaError::Corrupt(MediaCorruption::PublishedReader));
        }
        self.read_relative(selected.begin_block, &mut header_block)
            .await?;
        let begin_header = decode_record_header(&header_block).map_err(MediaError::Corrupt)?;
        if begin_header.kind != RecordKind::Begin
            || begin_header.upload_id != selected.plan.upload_id
            || begin_header
                .record_blocks
                .checked_add(selected.begin_block)
                .is_none_or(|end| end > selected.publish_block)
        {
            return Err(MediaError::Corrupt(MediaCorruption::PublishedReader));
        }
        payload.fill(0);
        let begin_digest = self
            .read_and_verify_record(
                selected.begin_block,
                &header_block,
                begin_header,
                &mut payload,
            )
            .await?;
        let begin_plan = UploadPlan::decode(&payload[..begin_header.payload_len], self.limits)
            .map_err(|_| MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        if begin_plan != selected.plan {
            return Err(MediaError::Corrupt(MediaCorruption::PublishedReader));
        }
        let next_record_block = selected
            .begin_block
            .checked_add(begin_header.record_blocks)
            .ok_or(MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        let next_sequence = begin_header
            .sequence
            .checked_add(1)
            .ok_or(MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        let manifest = ManifestHasher::new(
            selected.plan.object,
            selected.plan.chunk_bytes,
            selected.plan.chunk_count,
            self.limits,
        )
        .map_err(|_| MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        Ok(PublishedReader {
            plan: selected.plan,
            media_id: anchor.media_id,
            region: self.region,
            opened_generation: anchor.generation,
            opened_tail: anchor.committed_tail,
            begin_block: selected.begin_block,
            publish_block: selected.publish_block,
            publish_header: selected.publish_header,
            publish_digest: selected.publish_digest,
            next_record_block,
            next_sequence,
            previous_digest: begin_digest,
            next_chunk: 0,
            accepted_bytes: 0,
            object: ContentHasher::new(),
            manifest,
            complete: false,
        })
    }

    async fn read_next_published_inner(
        &mut self,
        reader: &mut PublishedReader,
        output: &mut [u8; MAX_MEDIA_CHUNK_BYTES],
    ) -> Result<Option<PublishedChunk>, MediaError<D::Error>> {
        let anchor = self.ready()?.anchor;
        if anchor.media_id != reader.media_id
            || self.region != reader.region
            || anchor.generation < reader.opened_generation
            || anchor.committed_tail < reader.opened_tail
            || reader.next_chunk > reader.plan.chunk_count
        {
            return Err(MediaError::Corrupt(MediaCorruption::PublishedReader));
        }
        if reader.complete {
            return Ok(None);
        }
        if reader.next_chunk >= reader.plan.chunk_count
            || reader.next_record_block >= reader.publish_block
        {
            return Err(MediaError::Corrupt(MediaCorruption::PublishedReader));
        }

        let mut header_block = [0_u8; MEDIA_BLOCK_BYTES];
        self.read_relative(reader.next_record_block, &mut header_block)
            .await?;
        let header = decode_record_header(&header_block).map_err(MediaError::Corrupt)?;
        if header.kind != RecordKind::Chunk
            || header.upload_id != reader.plan.upload_id
            || header.sequence != reader.next_sequence
            || header.previous_digest != reader.previous_digest
            || header
                .record_blocks
                .checked_add(reader.next_record_block)
                .is_none_or(|end| end > reader.publish_block)
        {
            return Err(MediaError::Corrupt(MediaCorruption::PublishedReader));
        }
        let mut payload = [0_u8; MAX_MEDIA_RECORD_PAYLOAD_BYTES];
        let digest = self
            .read_and_verify_record(
                reader.next_record_block,
                &header_block,
                header,
                &mut payload,
            )
            .await?;
        let prefix = payload
            .get(..ChunkUploadHeader::WIRE_LEN)
            .ok_or(MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        let chunk = ChunkUploadHeader::decode(prefix)
            .map_err(|_| MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        chunk
            .validate_body_len(
                u32::try_from(header.payload_len)
                    .map_err(|_| MediaError::Corrupt(MediaCorruption::PublishedReader))?,
            )
            .map_err(|_| MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        let bytes = &payload[ChunkUploadHeader::WIRE_LEN..header.payload_len];
        let expected_len = reader
            .plan
            .expected_chunk_len(reader.next_chunk)
            .ok_or(MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        let byte_len = u32::try_from(bytes.len())
            .map_err(|_| MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        if chunk.upload_id != reader.plan.upload_id
            || chunk.index != reader.next_chunk
            || chunk.byte_len != expected_len
            || byte_len != expected_len
            || sha256(bytes) != chunk.content
        {
            return Err(MediaError::Corrupt(MediaCorruption::PublishedReader));
        }

        let mut object = reader.object.clone();
        object.update(bytes);
        let mut manifest = reader.manifest.clone();
        manifest
            .push(chunk.index, chunk.content, chunk.byte_len)
            .map_err(|_| MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        let next_record_block = reader
            .next_record_block
            .checked_add(header.record_blocks)
            .ok_or(MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        let next_sequence = reader
            .next_sequence
            .checked_add(1)
            .ok_or(MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        let next_chunk = reader
            .next_chunk
            .checked_add(1)
            .ok_or(MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        let accepted_bytes = reader
            .accepted_bytes
            .checked_add(u64::from(chunk.byte_len))
            .ok_or(MediaError::Corrupt(MediaCorruption::PublishedReader))?;
        let final_chunk = next_chunk == reader.plan.chunk_count;

        let (committed_record_block, committed_sequence, committed_digest) = if final_chunk {
            if next_record_block != reader.publish_block {
                return Err(MediaError::Corrupt(MediaCorruption::PublishedReader));
            }
            self.read_relative(reader.publish_block, &mut header_block)
                .await?;
            let publish_header =
                decode_record_header(&header_block).map_err(MediaError::Corrupt)?;
            if publish_header != reader.publish_header
                || publish_header.kind != RecordKind::Publish
                || publish_header.upload_id != reader.plan.upload_id
                || publish_header.sequence != next_sequence
                || publish_header.previous_digest != digest
                || publish_header
                    .record_blocks
                    .checked_add(reader.publish_block)
                    .is_none_or(|end| end > reader.opened_tail)
            {
                return Err(MediaError::Corrupt(MediaCorruption::PublishedReader));
            }
            let mut publication_payload = [0_u8; MAX_MEDIA_RECORD_PAYLOAD_BYTES];
            let publish_digest = self
                .read_and_verify_record(
                    reader.publish_block,
                    &header_block,
                    publish_header,
                    &mut publication_payload,
                )
                .await?;
            let (published_plan, begin_block) = decode_publication(
                &publication_payload[..publish_header.payload_len],
                self.limits,
            )
            .map_err(MediaError::Corrupt)?;
            if publish_digest != reader.publish_digest
                || published_plan != reader.plan
                || begin_block != reader.begin_block
                || object.clone().finalize() != reader.plan.object.content
                || manifest
                    .clone()
                    .finalize()
                    .map_err(|_| MediaError::Corrupt(MediaCorruption::PublishedReader))?
                    != reader.plan.manifest
                || accepted_bytes != reader.plan.object.byte_len
            {
                return Err(MediaError::Corrupt(MediaCorruption::PublishedReader));
            }
            (
                reader
                    .publish_block
                    .checked_add(publish_header.record_blocks)
                    .ok_or(MediaError::Corrupt(MediaCorruption::PublishedReader))?,
                next_sequence
                    .checked_add(1)
                    .ok_or(MediaError::Corrupt(MediaCorruption::PublishedReader))?,
                publish_digest,
            )
        } else {
            if next_record_block >= reader.publish_block {
                return Err(MediaError::Corrupt(MediaCorruption::PublishedReader));
            }
            (next_record_block, next_sequence, digest)
        };

        output.fill(0);
        output[..bytes.len()].copy_from_slice(bytes);
        reader.next_record_block = committed_record_block;
        reader.next_sequence = committed_sequence;
        reader.previous_digest = committed_digest;
        reader.next_chunk = next_chunk;
        reader.accepted_bytes = accepted_bytes;
        reader.object = object;
        reader.manifest = manifest;
        reader.complete = final_chunk;
        Ok(Some(PublishedChunk {
            index: chunk.index,
            byte_len: chunk.byte_len,
            content: chunk.content,
        }))
    }

    /// Begins or resumes one declaration only after its begin record is durable.
    pub async fn begin_upload(
        &mut self,
        plan: UploadPlan,
        context: MutationContext,
    ) -> Result<UploadProgress, MediaError<D::Error>> {
        let (mut uploads, existing) = {
            let ready = self.ready()?;
            if ready.configuration.pending.is_some() {
                return Err(StorageError::ConfigurationTransition.into());
            }
            (ready.uploads, ready.uploads.checkpoint())
        };
        let progress = uploads.begin(plan, self.limits, context)?;
        if existing.is_some_and(|checkpoint| checkpoint.plan == plan) {
            return Ok(progress);
        }
        let begin_block = self
            .append_record(RecordKind::Begin, plan.upload_id, &plan.encode())
            .await?;
        let hashes = ActiveHashes {
            plan,
            begin_block,
            object: ContentHasher::new(),
            manifest: ManifestHasher::new(
                plan.object,
                plan.chunk_bytes,
                plan.chunk_count,
                self.limits,
            )?,
        };
        let ready = self.ready_mut()?;
        ready.uploads = uploads;
        ready.hashes = Some(hashes);
        Ok(progress)
    }

    /// Verifies and durably appends exactly the next sequential chunk.
    pub async fn put_chunk(
        &mut self,
        header: ChunkUploadHeader,
        bytes: &[u8],
        context: MutationContext,
    ) -> Result<UploadProgress, MediaError<D::Error>> {
        context.validate()?;
        if bytes.len() > MAX_MEDIA_CHUNK_BYTES {
            return Err(MediaError::Geometry(MediaGeometryError::ChunkLimit));
        }
        let (mut uploads, mut hashes) = {
            let ready = self.ready()?;
            let hashes = ready
                .hashes
                .as_ref()
                .ok_or(MediaError::Corrupt(MediaCorruption::Replay))?
                .clone();
            (ready.uploads, hashes)
        };
        let token = uploads.verify_chunk(header.upload_id, header.index, header.content, bytes)?;
        let body_len = u32::try_from(ChunkUploadHeader::WIRE_LEN + bytes.len())
            .map_err(|_| MediaError::Arithmetic)?;
        header
            .validate_body_len(body_len)
            .map_err(|_| MediaError::Corrupt(MediaCorruption::Replay))?;
        hashes.object.update(bytes);
        hashes
            .manifest
            .push(token.index(), token.content(), token.byte_len())?;
        let progress = uploads.record_chunk(token, context)?;

        let mut payload = [0_u8; MAX_MEDIA_RECORD_PAYLOAD_BYTES];
        payload[..ChunkUploadHeader::WIRE_LEN].copy_from_slice(&header.encode());
        payload[ChunkUploadHeader::WIRE_LEN..ChunkUploadHeader::WIRE_LEN + bytes.len()]
            .copy_from_slice(bytes);
        self.append_record(
            RecordKind::Chunk,
            header.upload_id,
            &payload[..ChunkUploadHeader::WIRE_LEN + bytes.len()],
        )
        .await?;
        let ready = self.ready_mut()?;
        ready.uploads = uploads;
        ready.hashes = Some(hashes);
        Ok(progress)
    }

    /// Replays complete hashes, commits one atomic publication record, and clears upload state.
    pub async fn finalize_upload(
        &mut self,
        request: FinalizeUploadRequest,
        context: MutationContext,
    ) -> Result<PublishedObject, MediaError<D::Error>> {
        context.validate()?;
        let (mut uploads, hashes) = {
            let ready = self.ready()?;
            let hashes = ready
                .hashes
                .as_ref()
                .ok_or(MediaError::Corrupt(MediaCorruption::Replay))?
                .clone();
            (ready.uploads, hashes)
        };
        if hashes.plan.upload_id != request.upload_id {
            return Err(StorageError::UploadIdMismatch {
                received: request.upload_id,
                expected: hashes.plan.upload_id,
            }
            .into());
        }
        let observed_object = hashes.object.clone().finalize();
        let observed_manifest = hashes.manifest.clone().finalize()?;
        let token = uploads.finalize(
            request.upload_id,
            observed_object,
            observed_manifest,
            context,
        )?;
        let mut publication = [0_u8; PUBLICATION_WIRE_LEN];
        publication[..UploadPlan::WIRE_LEN].copy_from_slice(&hashes.plan.encode());
        publication[UploadPlan::WIRE_LEN..].copy_from_slice(&hashes.begin_block.to_le_bytes());
        self.append_record(RecordKind::Publish, request.upload_id, &publication)
            .await?;
        let published = uploads.record_published(token, context)?;
        let ready = self.ready_mut()?;
        ready.uploads = uploads;
        ready.hashes = None;
        ready.published_objects = ready
            .published_objects
            .checked_add(1)
            .ok_or(MediaError::Arithmetic)?;
        Ok(published)
    }

    /// Durably abandons the active transaction; existing chunk records become orphans.
    pub async fn abort_upload(
        &mut self,
        upload_id: UploadId,
        context: MutationContext,
    ) -> Result<bool, MediaError<D::Error>> {
        let mut uploads = self.ready()?.uploads;
        let removed = uploads.abort(upload_id, context)?;
        if removed.is_none() {
            return Ok(false);
        }
        self.append_record(RecordKind::Abort, upload_id, &upload_id.0.to_le_bytes())
            .await?;
        let ready = self.ready_mut()?;
        ready.uploads = uploads;
        ready.hashes = None;
        Ok(true)
    }

    fn validate_configuration(&self) -> Result<(), MediaError<D::Error>> {
        self.region.validate(self.device.block_count())?;
        self.limits.validate()?;
        if usize::try_from(self.limits.maximum_chunk_bytes)
            .ok()
            .is_none_or(|limit| limit > MAX_MEDIA_CHUNK_BYTES)
        {
            return Err(MediaGeometryError::ChunkLimit.into());
        }
        Ok(())
    }

    fn ready(&self) -> Result<&ReadyState, MediaError<D::Error>> {
        match &self.state {
            MountState::Ready(ready) => Ok(ready),
            MountState::Detached => Err(MediaError::NotMounted),
            MountState::Faulted => Err(MediaError::Faulted),
        }
    }

    fn ready_mut(&mut self) -> Result<&mut ReadyState, MediaError<D::Error>> {
        match &mut self.state {
            MountState::Ready(ready) => Ok(ready),
            MountState::Detached => Err(MediaError::NotMounted),
            MountState::Faulted => Err(MediaError::Faulted),
        }
    }

    async fn append_record(
        &mut self,
        kind: RecordKind,
        upload_id: UploadId,
        payload: &[u8],
    ) -> Result<u64, MediaError<D::Error>> {
        let anchor = self.ready()?.anchor;
        let payload_len = payload.len();
        if payload_len == 0 || payload_len > MAX_MEDIA_RECORD_PAYLOAD_BYTES {
            return Err(MediaError::Corrupt(MediaCorruption::RecordHeader));
        }
        let payload_blocks = div_ceil_u64(
            u64::try_from(payload_len).map_err(|_| MediaError::Arithmetic)?,
            u64::try_from(MEDIA_BLOCK_BYTES).expect("block bytes fit u64"),
        );
        let record_blocks = payload_blocks
            .checked_add(2)
            .ok_or(MediaError::Arithmetic)?;
        let available = self
            .region
            .block_count
            .saturating_sub(anchor.committed_tail);
        if record_blocks > available {
            return Err(MediaError::Full {
                required: record_blocks,
                available,
            });
        }
        let sequence = anchor
            .last_sequence
            .checked_add(1)
            .ok_or(MediaError::Arithmetic)?;
        let start = anchor.committed_tail;
        let header = RecordHeader {
            kind,
            sequence,
            payload_len,
            record_blocks,
            upload_id,
            previous_digest: anchor.last_record_digest,
        };
        let header_block = encode_record_header(header);
        let mut hasher = ContentHasher::new();
        hasher.update(&header_block);
        self.write_relative(start, &header_block).await?;

        let mut block = [0_u8; MEDIA_BLOCK_BYTES];
        let mut offset = 0_usize;
        for relative in 0..payload_blocks {
            block.fill(0);
            let remaining = payload.len() - offset;
            let copied = remaining.min(MEDIA_BLOCK_BYTES);
            block[..copied].copy_from_slice(&payload[offset..offset + copied]);
            hasher.update(&block);
            self.write_relative(start + 1 + relative, &block).await?;
            offset += copied;
        }
        self.sync_device().await?;

        let record_digest = hasher.finalize().digest;
        let commit = encode_record_commit(start, header, record_digest);
        self.write_relative(start + record_blocks - 1, &commit)
            .await?;
        self.sync_device().await?;

        let next_slot = anchor.slot ^ 1;
        let next_anchor = Anchor {
            slot: next_slot,
            generation: anchor
                .generation
                .checked_add(1)
                .ok_or(MediaError::Arithmetic)?,
            committed_tail: start
                .checked_add(record_blocks)
                .ok_or(MediaError::Arithmetic)?,
            last_sequence: sequence,
            region_blocks: anchor.region_blocks,
            media_id: anchor.media_id,
            limits: anchor.limits,
            last_record_digest: record_digest,
        };
        let encoded_anchor = encode_anchor(next_anchor);
        self.write_relative(u64::from(next_slot), &encoded_anchor)
            .await?;
        self.sync_device().await?;
        self.ready_mut()?.anchor = next_anchor;
        Ok(start)
    }

    async fn read_and_verify_record(
        &mut self,
        start: u64,
        header_block: &MediaBlock,
        header: RecordHeader,
        payload: &mut [u8; MAX_MEDIA_RECORD_PAYLOAD_BYTES],
    ) -> Result<Digest, MediaError<D::Error>> {
        let payload_blocks = header.record_blocks - 2;
        let mut remaining = header.payload_len;
        let mut offset = 0_usize;
        let mut block = [0_u8; MEDIA_BLOCK_BYTES];
        let mut hasher = ContentHasher::new();
        hasher.update(header_block);
        for relative in 0..payload_blocks {
            self.read_relative(start + 1 + relative, &mut block).await?;
            hasher.update(&block);
            let copied = remaining.min(MEDIA_BLOCK_BYTES);
            payload[offset..offset + copied].copy_from_slice(&block[..copied]);
            if block[copied..].iter().any(|byte| *byte != 0) {
                return Err(MediaError::Corrupt(MediaCorruption::RecordPadding));
            }
            remaining -= copied;
            offset += copied;
        }
        if remaining != 0 || offset != header.payload_len {
            return Err(MediaError::Corrupt(MediaCorruption::RecordHeader));
        }
        let digest = hasher.finalize().digest;
        self.read_relative(start + header.record_blocks - 1, &mut block)
            .await?;
        validate_record_commit(&block, start, header, digest).map_err(MediaError::Corrupt)?;
        Ok(digest)
    }

    async fn read_relative(
        &mut self,
        relative: u64,
        output: &mut MediaBlock,
    ) -> Result<(), MediaError<D::Error>> {
        let absolute = self.region.absolute(relative)?;
        self.read_absolute(absolute, output).await
    }

    async fn write_relative(
        &mut self,
        relative: u64,
        data: &MediaBlock,
    ) -> Result<(), MediaError<D::Error>> {
        let absolute = self.region.absolute(relative)?;
        self.write_absolute(absolute, data).await
    }

    async fn read_absolute(
        &mut self,
        absolute: u64,
        output: &mut MediaBlock,
    ) -> Result<(), MediaError<D::Error>> {
        self.device
            .read_block(absolute, output)
            .await
            .map_err(MediaError::Device)
    }

    async fn write_absolute(
        &mut self,
        absolute: u64,
        data: &MediaBlock,
    ) -> Result<(), MediaError<D::Error>> {
        match self.device.write_block(absolute, data).await {
            Ok(()) => Ok(()),
            Err(error) => {
                self.state = MountState::Faulted;
                Err(MediaError::Device(error))
            }
        }
    }

    async fn sync_device(&mut self) -> Result<(), MediaError<D::Error>> {
        match self.device.sync().await {
            Ok(()) => Ok(()),
            Err(error) => {
                self.state = MountState::Faulted;
                Err(MediaError::Device(error))
            }
        }
    }
}

struct ReplayState {
    uploads: UploadCoordinator,
    hashes: Option<ActiveHashes>,
    published_objects: u32,
    configuration: ConfigurationJournal,
}

impl ReplayState {
    const fn new() -> Self {
        Self {
            uploads: UploadCoordinator::new(),
            hashes: None,
            published_objects: 0,
            configuration: ConfigurationJournal {
                active: None,
                pending: None,
            },
        }
    }

    fn apply(
        &mut self,
        header: RecordHeader,
        start: u64,
        payload: &[u8],
        limits: CacheLimits,
    ) -> Result<(), MediaCorruption> {
        match header.kind {
            RecordKind::Begin => {
                if self.configuration.pending.is_some() {
                    return Err(MediaCorruption::Replay);
                }
                let plan =
                    UploadPlan::decode(payload, limits).map_err(|_| MediaCorruption::Replay)?;
                if plan.upload_id != header.upload_id || self.hashes.is_some() {
                    return Err(MediaCorruption::Replay);
                }
                self.uploads
                    .begin(plan, limits, MutationContext::DISARMED_IDLE)
                    .map_err(|_| MediaCorruption::Replay)?;
                self.hashes = Some(ActiveHashes {
                    plan,
                    begin_block: start,
                    object: ContentHasher::new(),
                    manifest: ManifestHasher::new(
                        plan.object,
                        plan.chunk_bytes,
                        plan.chunk_count,
                        limits,
                    )
                    .map_err(|_| MediaCorruption::Replay)?,
                });
            }
            RecordKind::Chunk => {
                let prefix = payload
                    .get(..ChunkUploadHeader::WIRE_LEN)
                    .ok_or(MediaCorruption::Replay)?;
                let chunk =
                    ChunkUploadHeader::decode(prefix).map_err(|_| MediaCorruption::Replay)?;
                if chunk.upload_id != header.upload_id {
                    return Err(MediaCorruption::Replay);
                }
                chunk
                    .validate_body_len(
                        u32::try_from(payload.len()).map_err(|_| MediaCorruption::Replay)?,
                    )
                    .map_err(|_| MediaCorruption::Replay)?;
                let bytes = &payload[ChunkUploadHeader::WIRE_LEN..];
                let token = self
                    .uploads
                    .verify_chunk(chunk.upload_id, chunk.index, chunk.content, bytes)
                    .map_err(|_| MediaCorruption::Replay)?;
                let hashes = self.hashes.as_mut().ok_or(MediaCorruption::Replay)?;
                hashes.object.update(bytes);
                hashes
                    .manifest
                    .push(token.index(), token.content(), token.byte_len())
                    .map_err(|_| MediaCorruption::Replay)?;
                self.uploads
                    .record_chunk(token, MutationContext::DISARMED_IDLE)
                    .map_err(|_| MediaCorruption::Replay)?;
            }
            RecordKind::Publish => {
                if payload.len() != PUBLICATION_WIRE_LEN {
                    return Err(MediaCorruption::Replay);
                }
                let plan = UploadPlan::decode(&payload[..UploadPlan::WIRE_LEN], limits)
                    .map_err(|_| MediaCorruption::Replay)?;
                let begin_block = read_u64(payload, UploadPlan::WIRE_LEN);
                let hashes = self.hashes.as_ref().ok_or(MediaCorruption::Replay)?;
                if plan != hashes.plan
                    || plan.upload_id != header.upload_id
                    || begin_block != hashes.begin_block
                {
                    return Err(MediaCorruption::Replay);
                }
                let observed_object = hashes.object.clone().finalize();
                let observed_manifest = hashes
                    .manifest
                    .clone()
                    .finalize()
                    .map_err(|_| MediaCorruption::Replay)?;
                let token = self
                    .uploads
                    .finalize(
                        plan.upload_id,
                        observed_object,
                        observed_manifest,
                        MutationContext::DISARMED_IDLE,
                    )
                    .map_err(|_| MediaCorruption::Replay)?;
                self.uploads
                    .record_published(token, MutationContext::DISARMED_IDLE)
                    .map_err(|_| MediaCorruption::Replay)?;
                self.hashes = None;
                self.published_objects = self
                    .published_objects
                    .checked_add(1)
                    .ok_or(MediaCorruption::Replay)?;
            }
            RecordKind::Abort => {
                if payload.len() != 8 {
                    return Err(MediaCorruption::Replay);
                }
                let upload_id = UploadId(read_u64(payload, 0));
                if upload_id != header.upload_id {
                    return Err(MediaCorruption::Replay);
                }
                let removed = self
                    .uploads
                    .abort(upload_id, MutationContext::DISARMED_IDLE)
                    .map_err(|_| MediaCorruption::Replay)?;
                if removed.is_none() {
                    return Err(MediaCorruption::Replay);
                }
                self.hashes = None;
            }
            RecordKind::ConfigurationPrepare => {
                if self.hashes.is_some() {
                    return Err(MediaCorruption::Replay);
                }
                let transition = decode_configuration_transition(payload)?;
                if transition.selection.transaction_id != header.upload_id.0 {
                    return Err(MediaCorruption::Replay);
                }
                validate_configuration_transition(self.configuration.active, transition)
                    .map_err(|_| MediaCorruption::Replay)?;
                self.configuration.pending = Some(transition);
            }
            RecordKind::ConfigurationCommit => {
                if self.hashes.is_some() {
                    return Err(MediaCorruption::Replay);
                }
                let transition = decode_configuration_transition(payload)?;
                if transition.selection.transaction_id != header.upload_id.0
                    || self.configuration.pending != Some(transition)
                {
                    return Err(MediaCorruption::Replay);
                }
                apply_configuration_commit(&mut self.configuration, transition);
            }
        }
        Ok(())
    }
}

fn validate_configuration_transition(
    active: Option<DurableConfigurationSelection>,
    transition: ConfigurationTransition,
) -> Result<(), StorageError> {
    transition.validate()?;
    match transition.action {
        ConfigurationTransitionAction::Activate => Ok(()),
        ConfigurationTransitionAction::Clear
            if active
                .is_some_and(|active| active.publication == transition.selection.publication) =>
        {
            Ok(())
        }
        ConfigurationTransitionAction::Clear => Err(StorageError::ConfigurationTransition),
    }
}

fn apply_configuration_commit(
    journal: &mut ConfigurationJournal,
    transition: ConfigurationTransition,
) {
    journal.active = match transition.action {
        ConfigurationTransitionAction::Activate => Some(transition.selection),
        ConfigurationTransitionAction::Clear => None,
    };
    journal.pending = None;
}

fn encode_configuration_transition(
    transition: ConfigurationTransition,
) -> [u8; CONFIGURATION_SELECTION_WIRE_LEN] {
    let mut encoded = [0_u8; CONFIGURATION_SELECTION_WIRE_LEN];
    encoded[0..8].copy_from_slice(&CONFIGURATION_SELECTION_MAGIC);
    encoded[8..10].copy_from_slice(&CACHE_MEDIA_VERSION.to_le_bytes());
    encoded[10] = transition.action as u8;
    encoded[11] = transition.selection.publication.object.kind as u8;
    encoded[12] = transition.selection.publication.object.content.algorithm as u8;
    encoded[13] = transition.selection.publication.manifest.algorithm as u8;
    // Bytes 14..16 are reserved zero.
    encoded[16..24].copy_from_slice(&transition.selection.transaction_id.to_le_bytes());
    encoded[24..32].copy_from_slice(
        &transition
            .selection
            .publication
            .object
            .byte_len
            .to_le_bytes(),
    );
    encoded[32..64].copy_from_slice(&transition.selection.publication.object.content.digest.0);
    encoded[64..96].copy_from_slice(&transition.selection.publication.manifest.digest.0);
    encoded
}

fn decode_configuration_transition(
    encoded: &[u8],
) -> Result<ConfigurationTransition, MediaCorruption> {
    if encoded.len() != CONFIGURATION_SELECTION_WIRE_LEN
        || encoded[0..8] != CONFIGURATION_SELECTION_MAGIC
        || read_u16(encoded, 8) != CACHE_MEDIA_VERSION
        || encoded[11] != crate::ObjectKind::MachineConfiguration as u8
        || encoded[12] != crate::DigestAlgorithm::Sha256 as u8
        || encoded[13] != crate::DigestAlgorithm::Sha256 as u8
        || encoded[14..16].iter().any(|byte| *byte != 0)
    {
        return Err(MediaCorruption::Replay);
    }
    let action =
        ConfigurationTransitionAction::from_wire(encoded[10]).ok_or(MediaCorruption::Replay)?;
    let mut object_digest = [0_u8; 32];
    object_digest.copy_from_slice(&encoded[32..64]);
    let mut manifest_digest = [0_u8; 32];
    manifest_digest.copy_from_slice(&encoded[64..96]);
    let selection = DurableConfigurationSelection::new(
        read_u64(encoded, 16),
        PublishedObject {
            object: crate::StoredObject {
                kind: crate::ObjectKind::MachineConfiguration,
                content: crate::ContentId::from_sha256(Digest(object_digest)),
                byte_len: read_u64(encoded, 24),
            },
            manifest: crate::ContentId::from_sha256(Digest(manifest_digest)),
        },
    )
    .map_err(|_| MediaCorruption::Replay)?;
    let transition = ConfigurationTransition { action, selection };
    if encode_configuration_transition(transition) != encoded {
        return Err(MediaCorruption::Replay);
    }
    Ok(transition)
}

fn decode_publication(
    payload: &[u8],
    limits: CacheLimits,
) -> Result<(UploadPlan, u64), MediaCorruption> {
    if payload.len() != PUBLICATION_WIRE_LEN {
        return Err(MediaCorruption::PublishedReader);
    }
    let plan = UploadPlan::decode(&payload[..UploadPlan::WIRE_LEN], limits)
        .map_err(|_| MediaCorruption::PublishedReader)?;
    Ok((plan, read_u64(payload, UploadPlan::WIRE_LEN)))
}

fn checkpoint_progress(checkpoint: crate::UploadCheckpoint) -> UploadProgress {
    UploadProgress {
        upload_id: checkpoint.plan.upload_id,
        phase: checkpoint.phase,
        next_chunk: checkpoint.next_chunk,
        accepted_bytes: checkpoint.accepted_bytes,
        total_bytes: checkpoint.plan.object.byte_len,
    }
}

fn encode_anchor(anchor: Anchor) -> MediaBlock {
    let mut block = [0_u8; MEDIA_BLOCK_BYTES];
    block[0..8].copy_from_slice(&ANCHOR_MAGIC);
    block[8..10].copy_from_slice(&CACHE_MEDIA_VERSION.to_le_bytes());
    block[10] = anchor.slot;
    block[12..16].copy_from_slice(
        &u32::try_from(MEDIA_BLOCK_BYTES)
            .expect("media block bytes fit u32")
            .to_le_bytes(),
    );
    block[16..24].copy_from_slice(&anchor.generation.to_le_bytes());
    block[24..32].copy_from_slice(&anchor.committed_tail.to_le_bytes());
    block[32..40].copy_from_slice(&anchor.last_sequence.to_le_bytes());
    block[40..48].copy_from_slice(&anchor.region_blocks.to_le_bytes());
    block[48..64].copy_from_slice(&anchor.media_id.0);
    block[64..72].copy_from_slice(&anchor.limits.maximum_object_bytes.to_le_bytes());
    block[72..76].copy_from_slice(&anchor.limits.maximum_chunk_bytes.to_le_bytes());
    block[76..80].copy_from_slice(&anchor.limits.maximum_chunks.to_le_bytes());
    block[80..112].copy_from_slice(&anchor.last_record_digest.0);
    let digest = crate::sha256(&block[..ANCHOR_HASH_OFFSET]);
    block[ANCHOR_HASH_OFFSET..].copy_from_slice(&digest.digest.0);
    block
}

fn decode_anchor(
    block: &MediaBlock,
    expected_slot: u8,
    region: MediaRegion,
    limits: CacheLimits,
) -> Result<Option<Anchor>, MediaCorruption> {
    if block.iter().all(|byte| *byte == 0) || block.iter().all(|byte| *byte == 0xff) {
        return Ok(None);
    }
    if block[0..8] != ANCHOR_MAGIC
        || read_u16(block, 8) != CACHE_MEDIA_VERSION
        || block[10] != expected_slot
        || block[11] != 0
        || read_u32(block, 12) != u32::try_from(MEDIA_BLOCK_BYTES).unwrap_or(u32::MAX)
        || block[112..ANCHOR_HASH_OFFSET].iter().any(|byte| *byte != 0)
    {
        return Err(MediaCorruption::Anchor);
    }
    let expected_digest = crate::sha256(&block[..ANCHOR_HASH_OFFSET]);
    if block[ANCHOR_HASH_OFFSET..] != expected_digest.digest.0 {
        return Err(MediaCorruption::Anchor);
    }
    let mut media_id = [0_u8; 16];
    media_id.copy_from_slice(&block[48..64]);
    let media_id = MediaId::new(media_id).map_err(|_| MediaCorruption::Anchor)?;
    let decoded_limits = CacheLimits {
        maximum_object_bytes: read_u64(block, 64),
        maximum_chunk_bytes: read_u32(block, 72),
        maximum_chunks: read_u32(block, 76),
    };
    let mut last_digest = [0_u8; 32];
    last_digest.copy_from_slice(&block[80..112]);
    let anchor = Anchor {
        slot: expected_slot,
        generation: read_u64(block, 16),
        committed_tail: read_u64(block, 24),
        last_sequence: read_u64(block, 32),
        region_blocks: read_u64(block, 40),
        media_id,
        limits: decoded_limits,
        last_record_digest: Digest(last_digest),
    };
    if anchor.generation == 0
        || anchor.region_blocks != region.block_count
        || anchor.limits != limits
        || anchor.committed_tail < MEDIA_ANCHOR_BLOCKS
        || anchor.committed_tail > region.block_count
        || (anchor.last_sequence == 0
            && (anchor.committed_tail != MEDIA_ANCHOR_BLOCKS
                || !anchor.last_record_digest.is_zero()))
        || (anchor.last_sequence != 0
            && (anchor.committed_tail == MEDIA_ANCHOR_BLOCKS
                || anchor.last_record_digest.is_zero()))
    {
        return Err(MediaCorruption::AnchorState);
    }
    Ok(Some(anchor))
}

fn select_anchor(
    first: Result<Option<Anchor>, MediaCorruption>,
    second: Result<Option<Anchor>, MediaCorruption>,
) -> Result<(Option<Anchor>, bool), MediaCorruption> {
    match (first, second) {
        (Ok(None), Ok(None)) => Ok((None, false)),
        (Ok(Some(anchor)), Ok(None)) | (Ok(None), Ok(Some(anchor))) => Ok((Some(anchor), false)),
        (Ok(Some(first)), Ok(Some(second))) => {
            if first.media_id != second.media_id || first.generation == second.generation {
                return Err(MediaCorruption::ConflictingAnchors);
            }
            Ok((
                Some(if first.generation > second.generation {
                    first
                } else {
                    second
                }),
                false,
            ))
        }
        (Ok(Some(anchor)), Err(_)) | (Err(_), Ok(Some(anchor))) => Ok((Some(anchor), true)),
        (Ok(None), Err(error)) | (Err(error), Ok(None)) => Err(error),
        (Err(_), Err(_)) => Err(MediaCorruption::Anchor),
    }
}

fn encode_record_header(header: RecordHeader) -> MediaBlock {
    let mut block = [0_u8; MEDIA_BLOCK_BYTES];
    block[0..8].copy_from_slice(&RECORD_MAGIC);
    block[8..10].copy_from_slice(&CACHE_MEDIA_VERSION.to_le_bytes());
    block[10] = header.kind as u8;
    block[12..16].copy_from_slice(
        &u32::try_from(MEDIA_BLOCK_BYTES)
            .expect("media block bytes fit u32")
            .to_le_bytes(),
    );
    block[16..24].copy_from_slice(&header.sequence.to_le_bytes());
    block[24..32].copy_from_slice(
        &u64::try_from(header.payload_len)
            .expect("record payload fits u64")
            .to_le_bytes(),
    );
    block[32..40].copy_from_slice(&header.record_blocks.to_le_bytes());
    block[40..48].copy_from_slice(&header.upload_id.0.to_le_bytes());
    block[48..80].copy_from_slice(&header.previous_digest.0);
    block
}

fn decode_record_header(block: &MediaBlock) -> Result<RecordHeader, MediaCorruption> {
    let payload_len =
        usize::try_from(read_u64(block, 24)).map_err(|_| MediaCorruption::RecordHeader)?;
    let record_blocks = read_u64(block, 32);
    let expected_blocks = div_ceil_u64(
        u64::try_from(payload_len).map_err(|_| MediaCorruption::RecordHeader)?,
        u64::try_from(MEDIA_BLOCK_BYTES).expect("block bytes fit u64"),
    )
    .checked_add(2)
    .ok_or(MediaCorruption::RecordHeader)?;
    let kind = RecordKind::from_wire(block[10]).ok_or(MediaCorruption::RecordHeader)?;
    let mut previous_digest = [0_u8; 32];
    previous_digest.copy_from_slice(&block[48..80]);
    let header = RecordHeader {
        kind,
        sequence: read_u64(block, 16),
        payload_len,
        record_blocks,
        upload_id: UploadId(read_u64(block, 40)),
        previous_digest: Digest(previous_digest),
    };
    if block[0..8] != RECORD_MAGIC
        || read_u16(block, 8) != CACHE_MEDIA_VERSION
        || block[11] != 0
        || read_u32(block, 12) != u32::try_from(MEDIA_BLOCK_BYTES).unwrap_or(u32::MAX)
        || header.sequence == 0
        || header.payload_len == 0
        || header.payload_len > MAX_MEDIA_RECORD_PAYLOAD_BYTES
        || header.record_blocks != expected_blocks
        || header.upload_id.0 == 0
        || block[80..].iter().any(|byte| *byte != 0)
    {
        return Err(MediaCorruption::RecordHeader);
    }
    Ok(header)
}

fn encode_record_commit(start: u64, header: RecordHeader, digest: Digest) -> MediaBlock {
    let mut block = [0_u8; MEDIA_BLOCK_BYTES];
    block[0..8].copy_from_slice(&COMMIT_MAGIC);
    block[8..10].copy_from_slice(&CACHE_MEDIA_VERSION.to_le_bytes());
    block[10] = header.kind as u8;
    block[12..16].copy_from_slice(
        &u32::try_from(MEDIA_BLOCK_BYTES)
            .expect("media block bytes fit u32")
            .to_le_bytes(),
    );
    block[16..24].copy_from_slice(&header.sequence.to_le_bytes());
    block[24..32].copy_from_slice(&start.to_le_bytes());
    block[32..40].copy_from_slice(&header.record_blocks.to_le_bytes());
    block[40..48].copy_from_slice(
        &u64::try_from(header.payload_len)
            .expect("record payload fits u64")
            .to_le_bytes(),
    );
    block[48..80].copy_from_slice(&digest.0);
    block
}

fn validate_record_commit(
    block: &MediaBlock,
    start: u64,
    header: RecordHeader,
    digest: Digest,
) -> Result<(), MediaCorruption> {
    if block[0..8] != COMMIT_MAGIC
        || read_u16(block, 8) != CACHE_MEDIA_VERSION
        || block[10] != header.kind as u8
        || block[11] != 0
        || read_u32(block, 12) != u32::try_from(MEDIA_BLOCK_BYTES).unwrap_or(u32::MAX)
        || read_u64(block, 16) != header.sequence
        || read_u64(block, 24) != start
        || read_u64(block, 32) != header.record_blocks
        || read_u64(block, 40) != u64::try_from(header.payload_len).unwrap_or(u64::MAX)
        || block[48..80] != digest.0
        || block[80..].iter().any(|byte| *byte != 0)
    {
        return Err(MediaCorruption::RecordCommit);
    }
    Ok(())
}

const fn div_ceil_u64(value: u64, divisor: u64) -> u64 {
    value / divisor + if value.is_multiple_of(divisor) { 0 } else { 1 }
}

const fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

const fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

const fn read_u64(bytes: &[u8], offset: usize) -> u64 {
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

#[cfg(test)]
#[allow(
    clippy::std_instead_of_core,
    reason = "the deterministic media fault model is intentionally host-only"
)]
mod tests {
    extern crate std;

    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::vec;
    use std::vec::Vec;

    use alumina_protocol::Digest;
    use embassy_futures::block_on;

    use super::*;
    use crate::{ContentId, DigestAlgorithm, ObjectKind, StoredObject, UploadPhase, sha256};

    const TEST_LIMITS: CacheLimits = CacheLimits {
        maximum_object_bytes: 1_024 * 1_024,
        maximum_chunk_bytes: 1_024,
        maximum_chunks: 1_024,
    };
    const REGION: MediaRegion = MediaRegion {
        start_block: 8,
        block_count: 120,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum DeviceError {
        InjectedCut,
        OutsideDevice,
    }

    #[derive(Clone)]
    struct DeviceControl {
        blocks: Rc<RefCell<Vec<MediaBlock>>>,
        cut_at: Rc<Cell<Option<usize>>>,
        operations: Rc<Cell<usize>>,
    }

    impl DeviceControl {
        fn arm_relative(&self, relative: usize) {
            self.cut_at
                .set(Some(self.operations.get().saturating_add(relative)));
        }

        fn disarm(&self) {
            self.cut_at.set(None);
        }

        fn flip(&self, block: u64, byte: usize) {
            let block = usize::try_from(block).unwrap();
            self.blocks.borrow_mut()[block][byte] ^= 0x80;
        }
    }

    struct RamBlockDevice {
        blocks: Rc<RefCell<Vec<MediaBlock>>>,
        control: DeviceControl,
    }

    impl RamBlockDevice {
        fn erased(blocks: usize) -> (Self, DeviceControl) {
            let blocks = Rc::new(RefCell::new(vec![[0xff; MEDIA_BLOCK_BYTES]; blocks]));
            let control = DeviceControl {
                blocks: blocks.clone(),
                cut_at: Rc::new(Cell::new(None)),
                operations: Rc::new(Cell::new(0)),
            };
            (
                Self {
                    blocks,
                    control: control.clone(),
                },
                control,
            )
        }

        fn flip(&self, block: u64, byte: usize) {
            let block = usize::try_from(block).unwrap();
            self.blocks.borrow_mut()[block][byte] ^= 0x80;
        }

        fn snapshot(&self) -> Vec<MediaBlock> {
            self.blocks.borrow().clone()
        }

        fn from_snapshot(blocks: Vec<MediaBlock>) -> (Self, DeviceControl) {
            let blocks = Rc::new(RefCell::new(blocks));
            let control = DeviceControl {
                blocks: blocks.clone(),
                cut_at: Rc::new(Cell::new(None)),
                operations: Rc::new(Cell::new(0)),
            };
            (
                Self {
                    blocks,
                    control: control.clone(),
                },
                control,
            )
        }

        fn before_operation(&self) -> bool {
            let operation = self.control.operations.get();
            self.control.operations.set(operation.saturating_add(1));
            self.control.cut_at.get() == Some(operation)
        }
    }

    impl AsyncBlockDevice for RamBlockDevice {
        type Error = DeviceError;

        fn block_count(&self) -> u64 {
            u64::try_from(self.blocks.borrow().len()).unwrap()
        }

        async fn read_block(
            &mut self,
            block: u64,
            output: &mut MediaBlock,
        ) -> Result<(), Self::Error> {
            let block = usize::try_from(block).map_err(|_| DeviceError::OutsideDevice)?;
            let blocks = self.blocks.borrow();
            let source = blocks.get(block).ok_or(DeviceError::OutsideDevice)?;
            output.copy_from_slice(source);
            Ok(())
        }

        async fn write_block(&mut self, block: u64, data: &MediaBlock) -> Result<(), Self::Error> {
            let block = usize::try_from(block).map_err(|_| DeviceError::OutsideDevice)?;
            let mut blocks = self.blocks.borrow_mut();
            let target = blocks.get_mut(block).ok_or(DeviceError::OutsideDevice)?;
            if self.before_operation() {
                target[..137].copy_from_slice(&data[..137]);
                return Err(DeviceError::InjectedCut);
            }
            target.copy_from_slice(data);
            Ok(())
        }

        async fn sync(&mut self) -> Result<(), Self::Error> {
            if self.before_operation() {
                Err(DeviceError::InjectedCut)
            } else {
                Ok(())
            }
        }
    }

    fn media_id() -> MediaId {
        MediaId::new([0x5a; 16]).unwrap()
    }

    fn plan(bytes: &[u8], chunk_bytes: usize) -> UploadPlan {
        plan_for_kind(bytes, chunk_bytes, ObjectKind::MachineJobPartition)
    }

    fn plan_for_kind(bytes: &[u8], chunk_bytes: usize, kind: ObjectKind) -> UploadPlan {
        assert!(!bytes.is_empty());
        let chunk_count = bytes.len().div_ceil(chunk_bytes);
        let object = StoredObject {
            kind,
            content: sha256(bytes),
            byte_len: u64::try_from(bytes.len()).unwrap(),
        };
        let mut manifest = ManifestHasher::new(
            object,
            u32::try_from(chunk_bytes).unwrap(),
            u32::try_from(chunk_count).unwrap(),
            TEST_LIMITS,
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
        UploadPlan {
            upload_id: UploadId(0x1020_3040_5060_7080),
            object,
            manifest: manifest.finalize().unwrap(),
            chunk_bytes: u32::try_from(chunk_bytes).unwrap(),
            chunk_count: u32::try_from(chunk_count).unwrap(),
        }
    }

    fn chunk_header(plan: UploadPlan, index: u32, bytes: &[u8]) -> ChunkUploadHeader {
        ChunkUploadHeader {
            upload_id: plan.upload_id,
            index,
            byte_len: u32::try_from(bytes.len()).unwrap(),
            content: sha256(bytes),
        }
    }

    fn formatted() -> (CacheMedia<RamBlockDevice>, DeviceControl) {
        let (device, control) = RamBlockDevice::erased(160);
        let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
        block_on(media.format(media_id())).unwrap();
        (media, control)
    }

    fn publish(
        media: &mut CacheMedia<RamBlockDevice>,
        bytes: &[u8],
        chunk_bytes: usize,
    ) -> (UploadPlan, PublishedObject) {
        publish_kind(media, bytes, chunk_bytes, ObjectKind::MachineJobPartition)
    }

    fn publish_kind(
        media: &mut CacheMedia<RamBlockDevice>,
        bytes: &[u8],
        chunk_bytes: usize,
        kind: ObjectKind,
    ) -> (UploadPlan, PublishedObject) {
        let plan = plan_for_kind(bytes, chunk_bytes, kind);
        block_on(media.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        for (index, chunk) in bytes.chunks(chunk_bytes).enumerate() {
            block_on(media.put_chunk(
                chunk_header(plan, u32::try_from(index).unwrap(), chunk),
                chunk,
                MutationContext::DISARMED_IDLE,
            ))
            .unwrap();
        }
        let published = block_on(media.finalize_upload(
            FinalizeUploadRequest {
                upload_id: plan.upload_id,
            },
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap();
        (plan, published)
    }

    fn configuration_selection(
        transaction_id: u64,
        publication: PublishedObject,
    ) -> DurableConfigurationSelection {
        DurableConfigurationSelection::new(transaction_id, publication).unwrap()
    }

    fn collect_published(
        media: &mut CacheMedia<RamBlockDevice>,
        published: PublishedObject,
    ) -> Vec<u8> {
        let mut reader = block_on(media.open_published(published)).unwrap();
        assert_eq!(reader.published(), published);
        let mut collected = Vec::new();
        let mut output = [0xa5; MAX_MEDIA_CHUNK_BYTES];
        while let Some(chunk) =
            block_on(media.read_next_published(&mut reader, &mut output)).unwrap()
        {
            let byte_len = usize::try_from(chunk.byte_len).unwrap();
            assert_eq!(chunk.content, sha256(&output[..byte_len]));
            assert!(output[byte_len..].iter().all(|byte| *byte == 0));
            collected.extend_from_slice(&output[..byte_len]);
            output.fill(0xa5);
        }
        assert!(reader.is_complete());
        assert_eq!(reader.next_chunk(), reader.plan().chunk_count);
        assert_eq!(reader.accepted_bytes(), reader.plan().object.byte_len);
        collected
    }

    #[test]
    fn explicit_format_and_mount_preserve_region_boundaries() {
        let (device, _) = RamBlockDevice::erased(160);
        let before = device.snapshot();
        let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
        block_on(media.format(media_id())).unwrap();
        let status = media.status();
        assert_eq!(status.availability, MediaAvailability::Ready);
        assert_eq!(status.media_id, Some(media_id()));
        assert_eq!(status.committed_blocks, MEDIA_ANCHOR_BLOCKS);
        assert_eq!(status.free_blocks, REGION.block_count - MEDIA_ANCHOR_BLOCKS);

        let device = media.into_device();
        let after = device.snapshot();
        assert_eq!(
            &after[..usize::try_from(REGION.start_block).unwrap()],
            &before[..usize::try_from(REGION.start_block).unwrap()]
        );
        let region_end = usize::try_from(REGION.start_block + REGION.block_count).unwrap();
        assert_eq!(&after[region_end..], &before[region_end..]);

        let mut remounted = CacheMedia::new(device, REGION, TEST_LIMITS);
        let status = block_on(remounted.mount()).unwrap();
        assert_eq!(status.media_id, Some(media_id()));
        assert!(!status.degraded_anchor);
    }

    #[test]
    fn unformatted_invalid_geometry_and_oversized_policy_fail_closed() {
        assert_eq!(MediaId::new([0; 16]), Err(MediaGeometryError::ZeroMediaId));
        let (device, _) = RamBlockDevice::erased(32);
        let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
        assert!(matches!(
            block_on(media.mount()),
            Err(MediaError::Geometry(MediaGeometryError::OutsideDevice))
        ));

        let (device, _) = RamBlockDevice::erased(160);
        let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
        assert!(matches!(
            block_on(media.mount()),
            Err(MediaError::Unformatted)
        ));

        let (device, _) = RamBlockDevice::erased(160);
        let oversized = CacheLimits {
            maximum_chunk_bytes: 1_025,
            ..TEST_LIMITS
        };
        let mut media = CacheMedia::new(device, REGION, oversized);
        assert!(matches!(
            block_on(media.format(media_id())),
            Err(MediaError::Geometry(MediaGeometryError::ChunkLimit))
        ));
    }

    #[test]
    fn upload_replays_after_every_record_and_publication_is_atomic() {
        let bytes = b"an exact machine stream";
        let plan = plan(bytes, 8);
        let (mut media, _) = formatted();
        let progress = block_on(media.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        assert_eq!(progress.next_chunk, 0);

        for (index, chunk) in bytes.chunks(8).enumerate() {
            let progress = block_on(media.put_chunk(
                chunk_header(plan, u32::try_from(index).unwrap(), chunk),
                chunk,
                MutationContext::DISARMED_IDLE,
            ))
            .unwrap();
            assert_eq!(progress.next_chunk, u32::try_from(index + 1).unwrap());

            let device = media.into_device();
            media = CacheMedia::new(device, REGION, TEST_LIMITS);
            let mounted = block_on(media.mount()).unwrap();
            assert_eq!(mounted.upload, Some(progress));
        }

        let published = block_on(media.finalize_upload(
            FinalizeUploadRequest {
                upload_id: plan.upload_id,
            },
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap();
        assert_eq!(published.object, plan.object);
        assert_eq!(published.manifest, plan.manifest);
        assert_eq!(media.status().published_objects, 1);
        assert_eq!(media.status().upload, None);

        let device = media.into_device();
        let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
        let mounted = block_on(media.mount()).unwrap();
        assert_eq!(mounted.published_objects, 1);
        assert_eq!(mounted.upload, None);
        assert_eq!(mounted.last_sequence, u64::from(plan.chunk_count) + 2);
    }

    #[test]
    fn published_reader_streams_exact_bytes_before_and_after_remount() {
        let bytes = b"a bounded exact machine stream crossing several records";
        let (mut media, _) = formatted();
        let (plan, published) = publish(&mut media, bytes, 9);

        let mut reader = block_on(media.open_published(published)).unwrap();
        assert_eq!(reader.plan(), plan);
        assert_eq!(reader.next_chunk(), 0);
        assert_eq!(reader.accepted_bytes(), 0);
        assert!(!reader.is_complete());
        let mut output = [0_u8; MAX_MEDIA_CHUNK_BYTES];
        let first = block_on(media.read_next_published(&mut reader, &mut output))
            .unwrap()
            .unwrap();
        assert_eq!(first.index, 0);
        assert_eq!(
            &output[..usize::try_from(first.byte_len).unwrap()],
            &bytes[..9]
        );

        let mut collected = bytes[..9].to_vec();
        while let Some(chunk) =
            block_on(media.read_next_published(&mut reader, &mut output)).unwrap()
        {
            collected.extend_from_slice(&output[..usize::try_from(chunk.byte_len).unwrap()]);
        }
        assert_eq!(collected, bytes);
        assert!(reader.is_complete());
        assert!(
            block_on(media.read_next_published(&mut reader, &mut output))
                .unwrap()
                .is_none()
        );

        let device = media.into_device();
        let mut remounted = CacheMedia::new(device, REGION, TEST_LIMITS);
        block_on(remounted.mount()).unwrap();
        assert_eq!(collect_published(&mut remounted, published), bytes);
    }

    #[test]
    fn published_lookup_is_typed_and_a_miss_does_not_fault_media() {
        let bytes = b"typed immutable work";
        let (mut media, _) = formatted();
        let (_, published) = publish(&mut media, bytes, 7);
        let wrong_kind = PublishedObject {
            object: StoredObject {
                kind: ObjectKind::OpaqueData,
                ..published.object
            },
            manifest: published.manifest,
        };
        assert!(matches!(
            block_on(media.open_published(wrong_kind)),
            Err(MediaError::PublishedNotFound)
        ));
        assert_eq!(media.status().availability, MediaAvailability::Ready);
        assert_eq!(collect_published(&mut media, published), bytes);
    }

    #[test]
    fn reader_allows_later_appends_but_faults_on_post_open_corruption() {
        let bytes = b"abcdefgh";
        let (mut media, control) = formatted();
        let (_, published) = publish(&mut media, bytes, 4);
        let mut reader = block_on(media.open_published(published)).unwrap();

        let later = plan(b"later", 5);
        block_on(media.begin_upload(later, MutationContext::DISARMED_IDLE)).unwrap();
        let mut output = [0_u8; MAX_MEDIA_CHUNK_BYTES];
        let chunk = block_on(media.read_next_published(&mut reader, &mut output))
            .unwrap()
            .unwrap();
        assert_eq!(chunk.index, 0);
        assert_eq!(&output[..4], b"abcd");

        // Begin is 2..5 and chunk zero is 5..8; chunk one's payload is block 9.
        control.flip(REGION.start_block + 9, ChunkUploadHeader::WIRE_LEN + 1);
        assert!(matches!(
            block_on(media.read_next_published(&mut reader, &mut output)),
            Err(MediaError::Corrupt(MediaCorruption::RecordCommit))
                | Err(MediaError::Corrupt(MediaCorruption::PublishedReader))
        ));
        assert_eq!(media.status().availability, MediaAvailability::Faulted);
    }

    #[test]
    fn mutation_context_and_chunk_identity_reject_before_media_append() {
        let bytes = b"abcdefgh";
        let plan = plan(bytes, 4);
        let (mut media, _) = formatted();
        let before = media.status();
        assert!(matches!(
            block_on(media.begin_upload(
                plan,
                MutationContext {
                    armed_or_energized: true,
                    realtime_job_active: false,
                }
            )),
            Err(MediaError::Storage(StorageError::MutationForbidden))
        ));
        assert_eq!(media.status(), before);

        block_on(media.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        let before = media.status();
        let mut wrong = chunk_header(plan, 0, &bytes[..4]);
        wrong.content = ContentId {
            algorithm: DigestAlgorithm::Sha256,
            digest: Digest([0x33; 32]),
        };
        assert!(matches!(
            block_on(media.put_chunk(wrong, &bytes[..4], MutationContext::DISARMED_IDLE)),
            Err(MediaError::Storage(StorageError::ChunkDigest { index: 0 }))
        ));
        assert_eq!(media.status(), before);
    }

    #[test]
    fn abort_is_durable_and_frees_the_single_active_transaction() {
        let plan = plan(b"abcdefgh", 4);
        let (mut media, _) = formatted();
        block_on(media.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        assert!(
            block_on(media.abort_upload(plan.upload_id, MutationContext::DISARMED_IDLE)).unwrap()
        );
        assert_eq!(media.status().upload, None);
        let device = media.into_device();
        let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
        assert_eq!(block_on(media.mount()).unwrap().upload, None);
    }

    #[test]
    fn configuration_selection_requires_publication_and_matching_two_phase_commit() {
        let (mut media, _) = formatted();
        let (_, published) = publish_kind(
            &mut media,
            b"canonical machine configuration",
            9,
            ObjectKind::MachineConfiguration,
        );
        let selection = configuration_selection(0x4455, published);
        let activation = ConfigurationTransition::activate(selection);

        let journal = block_on(
            media.prepare_configuration_transition(activation, MutationContext::DISARMED_IDLE),
        )
        .unwrap();
        assert_eq!(journal.active, None);
        assert_eq!(journal.pending, Some(activation));
        // An exact prepare retry does not consume another record.
        let sequence = media.status().last_sequence;
        assert_eq!(
            block_on(
                media.prepare_configuration_transition(activation, MutationContext::DISARMED_IDLE,)
            )
            .unwrap(),
            journal
        );
        assert_eq!(media.status().last_sequence, sequence);

        let device = media.into_device();
        let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
        block_on(media.mount()).unwrap();
        assert_eq!(media.configuration_journal().unwrap(), journal);
        let committed = block_on(
            media.commit_configuration_transition(activation, MutationContext::DISARMED_IDLE),
        )
        .unwrap();
        assert_eq!(committed.active, Some(selection));
        assert_eq!(committed.pending, None);

        let clear_selection = configuration_selection(0x4456, published);
        let clear = ConfigurationTransition::clear(clear_selection);
        let prepared_clear =
            block_on(media.prepare_configuration_transition(clear, MutationContext::DISARMED_IDLE))
                .unwrap();
        assert_eq!(prepared_clear.active, Some(selection));
        assert_eq!(prepared_clear.pending, Some(clear));
        let cleared =
            block_on(media.commit_configuration_transition(clear, MutationContext::DISARMED_IDLE))
                .unwrap();
        assert_eq!(cleared.active, None);
        assert_eq!(cleared.pending, None);

        let device = media.into_device();
        let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
        block_on(media.mount()).unwrap();
        assert_eq!(media.configuration_journal().unwrap(), cleared);
    }

    #[test]
    fn invalid_configuration_transitions_reject_before_append() {
        let (mut media, _) = formatted();
        let (_, job) = publish(&mut media, b"not a configuration", 8);
        assert_eq!(
            DurableConfigurationSelection::new(1, job),
            Err(StorageError::ConfigurationTransition)
        );

        let missing = PublishedObject {
            object: StoredObject {
                kind: ObjectKind::MachineConfiguration,
                content: sha256(b"missing configuration"),
                byte_len: 21,
            },
            manifest: sha256(b"missing manifest"),
        };
        let missing = ConfigurationTransition::activate(configuration_selection(2, missing));
        let sequence = media.status().last_sequence;
        assert!(matches!(
            block_on(
                media.prepare_configuration_transition(missing, MutationContext::DISARMED_IDLE,)
            ),
            Err(MediaError::PublishedNotFound)
        ));
        assert_eq!(media.status().last_sequence, sequence);

        let (_, published) = publish_kind(
            &mut media,
            b"present configuration",
            8,
            ObjectKind::MachineConfiguration,
        );
        let activation = ConfigurationTransition::activate(configuration_selection(3, published));
        let sequence = media.status().last_sequence;
        assert!(matches!(
            block_on(media.prepare_configuration_transition(
                activation,
                MutationContext {
                    armed_or_energized: true,
                    realtime_job_active: false,
                },
            )),
            Err(MediaError::Storage(StorageError::MutationForbidden))
        ));
        assert_eq!(media.status().last_sequence, sequence);
        assert!(matches!(
            block_on(
                media.commit_configuration_transition(activation, MutationContext::DISARMED_IDLE,)
            ),
            Err(MediaError::Storage(StorageError::ConfigurationTransition))
        ));

        let clear = ConfigurationTransition::clear(configuration_selection(4, published));
        assert!(matches!(
            block_on(
                media.prepare_configuration_transition(clear, MutationContext::DISARMED_IDLE,)
            ),
            Err(MediaError::Storage(StorageError::ConfigurationTransition))
        ));

        let upload = plan(b"serialized upload", 8);
        block_on(media.begin_upload(upload, MutationContext::DISARMED_IDLE)).unwrap();
        assert!(matches!(
            block_on(
                media.prepare_configuration_transition(activation, MutationContext::DISARMED_IDLE,)
            ),
            Err(MediaError::Storage(StorageError::ConfigurationTransition))
        ));
        block_on(media.abort_upload(upload.upload_id, MutationContext::DISARMED_IDLE)).unwrap();
        block_on(
            media.prepare_configuration_transition(activation, MutationContext::DISARMED_IDLE),
        )
        .unwrap();
        assert!(matches!(
            block_on(media.begin_upload(upload, MutationContext::DISARMED_IDLE)),
            Err(MediaError::Storage(StorageError::ConfigurationTransition))
        ));
    }

    #[test]
    fn every_activation_cut_replays_complete_old_or_new_selection() {
        let (mut baseline, _) = formatted();
        let (_, old_publication) = publish_kind(
            &mut baseline,
            b"old exact configuration",
            8,
            ObjectKind::MachineConfiguration,
        );
        let old = configuration_selection(0x1001, old_publication);
        let old_transition = ConfigurationTransition::activate(old);
        block_on(
            baseline
                .prepare_configuration_transition(old_transition, MutationContext::DISARMED_IDLE),
        )
        .unwrap();
        block_on(
            baseline
                .commit_configuration_transition(old_transition, MutationContext::DISARMED_IDLE),
        )
        .unwrap();
        let (_, new_publication) = publish_kind(
            &mut baseline,
            b"new exact configuration",
            8,
            ObjectKind::MachineConfiguration,
        );
        let new = configuration_selection(0x1002, new_publication);
        let transition = ConfigurationTransition::activate(new);
        let before_prepare = baseline.into_device().snapshot();

        for cut in 0..=7 {
            let (device, control) = RamBlockDevice::from_snapshot(before_prepare.clone());
            let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
            block_on(media.mount()).unwrap();
            control.arm_relative(cut);
            let result = block_on(
                media.prepare_configuration_transition(transition, MutationContext::DISARMED_IDLE),
            );
            control.disarm();
            block_on(media.mount()).unwrap();
            let journal = media.configuration_journal().unwrap();
            assert_eq!(journal.active, Some(old));
            assert!(journal.pending.is_none() || journal.pending == Some(transition));
            if result.is_ok() {
                assert_eq!(journal.pending, Some(transition));
            }
        }

        let (device, _) = RamBlockDevice::from_snapshot(before_prepare);
        let mut prepared = CacheMedia::new(device, REGION, TEST_LIMITS);
        block_on(prepared.mount()).unwrap();
        block_on(
            prepared.prepare_configuration_transition(transition, MutationContext::DISARMED_IDLE),
        )
        .unwrap();
        let before_commit = prepared.into_device().snapshot();
        for cut in 0..=7 {
            let (device, control) = RamBlockDevice::from_snapshot(before_commit.clone());
            let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
            block_on(media.mount()).unwrap();
            control.arm_relative(cut);
            let result = block_on(
                media.commit_configuration_transition(transition, MutationContext::DISARMED_IDLE),
            );
            control.disarm();
            block_on(media.mount()).unwrap();
            let journal = media.configuration_journal().unwrap();
            assert!(journal.active == Some(old) || journal.active == Some(new));
            if journal.active == Some(old) {
                assert_eq!(journal.pending, Some(transition));
            } else {
                assert_eq!(journal.pending, None);
            }
            if result.is_ok() {
                assert_eq!(journal.active, Some(new));
            }
        }
    }

    #[test]
    fn every_clear_commit_cut_replays_selected_or_cleared() {
        let (mut baseline, _) = formatted();
        let (_, publication) = publish_kind(
            &mut baseline,
            b"configuration selected for clear",
            8,
            ObjectKind::MachineConfiguration,
        );
        let active = configuration_selection(0x2001, publication);
        let activation = ConfigurationTransition::activate(active);
        block_on(
            baseline.prepare_configuration_transition(activation, MutationContext::DISARMED_IDLE),
        )
        .unwrap();
        block_on(
            baseline.commit_configuration_transition(activation, MutationContext::DISARMED_IDLE),
        )
        .unwrap();
        let clear = ConfigurationTransition::clear(configuration_selection(0x2002, publication));
        block_on(baseline.prepare_configuration_transition(clear, MutationContext::DISARMED_IDLE))
            .unwrap();
        let snapshot = baseline.into_device().snapshot();

        for cut in 0..=7 {
            let (device, control) = RamBlockDevice::from_snapshot(snapshot.clone());
            let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
            block_on(media.mount()).unwrap();
            control.arm_relative(cut);
            let result = block_on(
                media.commit_configuration_transition(clear, MutationContext::DISARMED_IDLE),
            );
            control.disarm();
            block_on(media.mount()).unwrap();
            let journal = media.configuration_journal().unwrap();
            assert!(journal.active.is_none() || journal.active == Some(active));
            if journal.active.is_some() {
                assert_eq!(journal.pending, Some(clear));
            } else {
                assert_eq!(journal.pending, None);
            }
            if result.is_ok() {
                assert_eq!(journal.active, None);
            }
        }
    }

    #[test]
    fn committed_payload_corruption_is_detected_during_mount() {
        let plan = plan(b"abcdefgh", 4);
        let (mut media, _) = formatted();
        block_on(media.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        block_on(media.put_chunk(
            chunk_header(plan, 0, b"abcd"),
            b"abcd",
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap();
        let device = media.into_device();
        // Begin occupies relative 2..5; the first chunk payload is relative block 6.
        device.flip(REGION.start_block + 6, 53);
        let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
        assert!(matches!(
            block_on(media.mount()),
            Err(MediaError::Corrupt(MediaCorruption::RecordCommit))
                | Err(MediaError::Corrupt(MediaCorruption::Replay))
        ));
    }

    #[test]
    fn one_torn_anchor_recovers_the_previous_complete_commit() {
        let plan = plan(b"abcdefgh", 4);
        let (mut media, _) = formatted();
        block_on(media.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        block_on(media.put_chunk(
            chunk_header(plan, 0, b"abcd"),
            b"abcd",
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap();
        let device = media.into_device();
        // Generation three is in slot zero; corrupt it so slot one/gen two wins.
        device.flip(REGION.start_block, ANCHOR_HASH_OFFSET);
        let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
        let status = block_on(media.mount()).unwrap();
        assert!(status.degraded_anchor);
        assert_eq!(status.last_sequence, 1);
        assert_eq!(status.upload.unwrap().next_chunk, 0);
    }

    #[test]
    fn power_cut_at_every_begin_barrier_recovers_old_or_new_state() {
        let plan = plan(b"abcdefgh", 4);
        let (baseline, _) = formatted();
        let snapshot = baseline.into_device().snapshot();
        for cut in 0..=7 {
            let (device, control) = RamBlockDevice::from_snapshot(snapshot.clone());
            let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
            block_on(media.mount()).unwrap();
            control.arm_relative(cut);
            let result = block_on(media.begin_upload(plan, MutationContext::DISARMED_IDLE));
            control.disarm();
            let mounted = block_on(media.mount()).unwrap();
            assert!(
                mounted.upload.is_none()
                    || mounted
                        .upload
                        .is_some_and(|progress| progress.next_chunk == 0)
            );
            if result.is_ok() {
                assert!(mounted.upload.is_some());
            }
        }
    }

    #[test]
    fn power_cut_at_every_chunk_barrier_recovers_exact_progress() {
        let bytes = b"abcdefgh";
        let plan = plan(bytes, 4);
        let (mut baseline, _) = formatted();
        block_on(baseline.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        let snapshot = baseline.into_device().snapshot();
        for cut in 0..=7 {
            let (device, control) = RamBlockDevice::from_snapshot(snapshot.clone());
            let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
            block_on(media.mount()).unwrap();
            control.arm_relative(cut);
            let result = block_on(media.put_chunk(
                chunk_header(plan, 0, &bytes[..4]),
                &bytes[..4],
                MutationContext::DISARMED_IDLE,
            ));
            control.disarm();
            let mounted = block_on(media.mount()).unwrap();
            let next = mounted.upload.unwrap().next_chunk;
            assert!(next == 0 || next == 1);
            if result.is_ok() {
                assert_eq!(next, 1);
            }
        }
    }

    #[test]
    fn power_cut_at_every_publish_barrier_never_exposes_partial_publication() {
        let bytes = b"abcdefgh";
        let plan = plan(bytes, 4);
        let (mut baseline, _) = formatted();
        block_on(baseline.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        for (index, chunk) in bytes.chunks(4).enumerate() {
            block_on(baseline.put_chunk(
                chunk_header(plan, u32::try_from(index).unwrap(), chunk),
                chunk,
                MutationContext::DISARMED_IDLE,
            ))
            .unwrap();
        }
        let snapshot = baseline.into_device().snapshot();
        for cut in 0..=7 {
            let (device, control) = RamBlockDevice::from_snapshot(snapshot.clone());
            let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
            block_on(media.mount()).unwrap();
            control.arm_relative(cut);
            let result = block_on(media.finalize_upload(
                FinalizeUploadRequest {
                    upload_id: plan.upload_id,
                },
                MutationContext::DISARMED_IDLE,
            ));
            control.disarm();
            let mounted = block_on(media.mount()).unwrap();
            assert!(
                (mounted.published_objects == 0
                    && mounted
                        .upload
                        .is_some_and(|progress| progress.phase == UploadPhase::Receiving))
                    || (mounted.published_objects == 1 && mounted.upload.is_none())
            );
            if result.is_ok() {
                assert_eq!(mounted.published_objects, 1);
            }
        }
    }

    #[test]
    fn interrupted_format_is_always_recoverable_by_explicit_retry() {
        let (baseline, _) = formatted();
        let snapshot = baseline.into_device().snapshot();
        for cut in 0..=5 {
            let (device, control) = RamBlockDevice::from_snapshot(snapshot.clone());
            let mut media = CacheMedia::new(device, REGION, TEST_LIMITS);
            control.arm_relative(cut);
            let _ = block_on(media.format(MediaId::new([0xa5; 16]).unwrap()));
            control.disarm();
            block_on(media.format(MediaId::new([0xa5; 16]).unwrap())).unwrap();
            let status = block_on(media.mount()).unwrap();
            assert_eq!(status.last_sequence, 0);
            assert_eq!(status.media_id, Some(MediaId([0xa5; 16])));
        }
    }
}
