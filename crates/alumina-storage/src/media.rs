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
    UploadProgress,
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
const ANCHOR_HASH_OFFSET: usize = MEDIA_BLOCK_BYTES - 32;
const PUBLICATION_WIRE_LEN: usize = UploadPlan::WIRE_LEN + 8;
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

#[derive(Clone, Copy)]
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
}

impl RecordKind {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Begin),
            2 => Some(Self::Chunk),
            3 => Some(Self::Publish),
            4 => Some(Self::Abort),
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
    degraded_anchor: bool,
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
            degraded_anchor,
        });
        Ok(self.status())
    }

    /// Begins or resumes one declaration only after its begin record is durable.
    pub async fn begin_upload(
        &mut self,
        plan: UploadPlan,
        context: MutationContext,
    ) -> Result<UploadProgress, MediaError<D::Error>> {
        let (mut uploads, existing) = {
            let ready = self.ready()?;
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
}

impl ReplayState {
    const fn new() -> Self {
        Self {
            uploads: UploadCoordinator::new(),
            hashes: None,
            published_objects: 0,
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
        }
        Ok(())
    }
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
    }

    struct RamBlockDevice {
        blocks: Rc<RefCell<Vec<MediaBlock>>>,
        control: DeviceControl,
    }

    impl RamBlockDevice {
        fn erased(blocks: usize) -> (Self, DeviceControl) {
            let control = DeviceControl {
                cut_at: Rc::new(Cell::new(None)),
                operations: Rc::new(Cell::new(0)),
            };
            (
                Self {
                    blocks: Rc::new(RefCell::new(vec![[0xff; MEDIA_BLOCK_BYTES]; blocks])),
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
            let control = DeviceControl {
                cut_at: Rc::new(Cell::new(None)),
                operations: Rc::new(Cell::new(0)),
            };
            (
                Self {
                    blocks: Rc::new(RefCell::new(blocks)),
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
        assert!(!bytes.is_empty());
        let chunk_count = bytes.len().div_ceil(chunk_bytes);
        let object = StoredObject {
            kind: ObjectKind::MachineJobPartition,
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
