#![doc = "Deterministic host models for Alumina storage and service/RT boundaries."]

use core::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;

use alumina_storage::media::{AsyncBlockDevice, MEDIA_BLOCK_BYTES, MediaBlock};
use alumina_storage::{
    CacheLimits, ContentHasher, ContentId, ManifestHasher, MutationContext, PublishedObject,
    StoredObject, UploadCheckpoint, UploadCoordinator, UploadId, UploadPhase, UploadPlan,
    UploadProgress, sha256,
};

/// Deterministic failure from the shared in-memory block-device model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockDeviceError {
    /// A read or write addressed a block outside the image.
    OutsideDevice,
    /// The simulated device remains unpowered after an injected cut.
    PoweredOff,
    /// Power was removed during this numbered write or synchronization operation.
    InjectedPowerLoss {
        /// Zero-based persistent-operation counter.
        operation: u64,
    },
}

#[derive(Debug)]
struct SimBlockState {
    blocks: Vec<MediaBlock>,
}

/// Cloneable control plane for fault injection and inspection while a media
/// backend owns the corresponding [`SimBlockDevice`].
#[derive(Clone, Debug)]
pub struct SimBlockControl {
    state: Rc<RefCell<SimBlockState>>,
    powered: Rc<Cell<bool>>,
    operations: Rc<Cell<u64>>,
    cut_at: Rc<Cell<Option<u64>>>,
    torn_write_bytes: Rc<Cell<usize>>,
}

impl SimBlockControl {
    /// Arms a one-shot power cut relative to the next write or sync operation.
    ///
    /// A value of zero cuts the next persistent operation. If that operation
    /// is a write, exactly `torn_write_bytes` leading bytes reach the image
    /// before power disappears. The prefix is clamped to one complete block.
    pub fn arm_power_cut(&self, operations_from_now: u64, torn_write_bytes: usize) {
        self.cut_at.set(Some(
            self.operations.get().saturating_add(operations_from_now),
        ));
        self.torn_write_bytes
            .set(torn_write_bytes.min(MEDIA_BLOCK_BYTES));
    }

    /// Cancels an armed cut without changing power state or operation count.
    pub fn disarm_power_cut(&self) {
        self.cut_at.set(None);
    }

    /// Restores power after a cut and cancels any still-armed fault.
    pub fn restore_power(&self) {
        self.powered.set(true);
        self.cut_at.set(None);
    }

    /// Whether the simulated block device currently has power.
    pub fn is_powered(&self) -> bool {
        self.powered.get()
    }

    /// Number of write and sync operations attempted since image creation.
    pub fn operation_count(&self) -> u64 {
        self.operations.get()
    }

    /// Copies the exact persistent image for deterministic reboot branching.
    pub fn snapshot(&self) -> Vec<MediaBlock> {
        self.state.borrow().blocks.clone()
    }

    /// Corrupts one persistent byte without changing any neighboring state.
    pub fn flip_byte(&self, block: u64, byte: usize) -> Result<(), BlockDeviceError> {
        let block = usize::try_from(block).map_err(|_| BlockDeviceError::OutsideDevice)?;
        let mut state = self.state.borrow_mut();
        let target = state
            .blocks
            .get_mut(block)
            .and_then(|sector| sector.get_mut(byte))
            .ok_or(BlockDeviceError::OutsideDevice)?;
        *target ^= 0x80;
        Ok(())
    }
}

/// Shared, byte-exact 512-byte block device for host simulation.
///
/// Successful writes update the persistent image immediately. This is one
/// conservative behavior permitted before `sync`; an injected sync failure may
/// therefore leave every prior write present. An injected write failure can
/// instead leave a configurable torn prefix. Together these modes exercise the
/// cache format's old-or-new recovery contract without simulating a filesystem.
#[derive(Debug)]
pub struct SimBlockDevice {
    control: SimBlockControl,
}

impl SimBlockDevice {
    /// Creates an erased image and an independent cloneable control plane.
    pub fn erased(block_count: usize) -> (Self, SimBlockControl) {
        Self::from_snapshot(vec![[0xff; MEDIA_BLOCK_BYTES]; block_count])
    }

    /// Boots a new device instance over an exact persistent image snapshot.
    pub fn from_snapshot(blocks: Vec<MediaBlock>) -> (Self, SimBlockControl) {
        let control = SimBlockControl {
            state: Rc::new(RefCell::new(SimBlockState { blocks })),
            powered: Rc::new(Cell::new(true)),
            operations: Rc::new(Cell::new(0)),
            cut_at: Rc::new(Cell::new(None)),
            torn_write_bytes: Rc::new(Cell::new(0)),
        };
        (
            Self {
                control: control.clone(),
            },
            control,
        )
    }

    fn require_power(&self) -> Result<(), BlockDeviceError> {
        if self.control.powered.get() {
            Ok(())
        } else {
            Err(BlockDeviceError::PoweredOff)
        }
    }

    fn begin_persistent_operation(&self) -> Result<u64, BlockDeviceError> {
        self.require_power()?;
        let operation = self.control.operations.get();
        self.control.operations.set(operation.saturating_add(1));
        if self.control.cut_at.get() == Some(operation) {
            self.control.cut_at.set(None);
            self.control.powered.set(false);
            Err(BlockDeviceError::InjectedPowerLoss { operation })
        } else {
            Ok(operation)
        }
    }
}

impl AsyncBlockDevice for SimBlockDevice {
    type Error = BlockDeviceError;

    fn block_count(&self) -> u64 {
        u64::try_from(self.control.state.borrow().blocks.len()).unwrap_or(u64::MAX)
    }

    async fn read_block(&mut self, block: u64, output: &mut MediaBlock) -> Result<(), Self::Error> {
        self.require_power()?;
        let block = usize::try_from(block).map_err(|_| BlockDeviceError::OutsideDevice)?;
        let state = self.control.state.borrow();
        let source = state
            .blocks
            .get(block)
            .ok_or(BlockDeviceError::OutsideDevice)?;
        output.copy_from_slice(source);
        Ok(())
    }

    async fn write_block(&mut self, block: u64, data: &MediaBlock) -> Result<(), Self::Error> {
        let block = usize::try_from(block).map_err(|_| BlockDeviceError::OutsideDevice)?;
        if block >= self.control.state.borrow().blocks.len() {
            return Err(BlockDeviceError::OutsideDevice);
        }
        if let Err(error) = self.begin_persistent_operation() {
            if matches!(error, BlockDeviceError::InjectedPowerLoss { .. }) {
                let prefix = self.control.torn_write_bytes.get();
                let mut state = self.control.state.borrow_mut();
                let target = state
                    .blocks
                    .get_mut(block)
                    .ok_or(BlockDeviceError::OutsideDevice)?;
                target[..prefix].copy_from_slice(&data[..prefix]);
            }
            return Err(error);
        }
        let mut state = self.control.state.borrow_mut();
        let target = state
            .blocks
            .get_mut(block)
            .ok_or(BlockDeviceError::OutsideDevice)?;
        target.copy_from_slice(data);
        Ok(())
    }

    async fn sync(&mut self) -> Result<(), Self::Error> {
        self.begin_persistent_operation().map(|_| ())
    }
}

/// One immutable ordered chunk journal record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkRecord {
    /// Sequential index in the canonical manifest.
    pub index: u32,
    /// Content-addressed blob identity.
    pub content: ContentId,
    /// Exact bytes in this chunk.
    pub byte_len: u32,
}

/// Atomically visible object manifest in the simulated durable directory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedRecord {
    /// Immutable object facts.
    pub object: StoredObject,
    /// Canonical ordered manifest identity.
    pub manifest: ContentId,
    /// Canonical fixed chunk size used by the manifest.
    pub chunk_bytes: u32,
    /// Ordered content-addressed chunk records.
    pub chunks: Vec<ChunkRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DurableUpload {
    checkpoint: UploadCheckpoint,
    chunks: Vec<ChunkRecord>,
}

/// Cloneable durable SD image used to model resets at exact transaction boundaries.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DurableCache {
    blobs: BTreeMap<ContentId, Vec<u8>>,
    upload: Option<DurableUpload>,
    staging: Option<PublishedRecord>,
    published: BTreeMap<ContentId, PublishedRecord>,
}

/// Injected power-loss points around durable and atomic publication boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerCut {
    /// Content-addressed chunk is durable but its ordered journal entry is not.
    AfterChunkBlob,
    /// Ordered chunk entry is durable but checkpoint counters are not.
    AfterChunkJournal,
    /// Complete verification checkpoint is durable but no staging manifest exists.
    AfterPublishPending,
    /// Staging manifest exists but has not become atomically visible.
    AfterStagingManifest,
    /// Manifest is atomically visible but the upload journal is not cleared.
    AfterAtomicPublish,
}

/// Host simulation/cache failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Portable transaction model rejected the operation.
    Storage(alumina_storage::Error),
    /// Operation requires a powered service instance.
    PoweredOff,
    /// Injected power loss occurred at the named boundary.
    PowerLoss(PowerCut),
    /// Durable journal/checkpoint relationship is impossible.
    CorruptJournal,
    /// A referenced content-addressed blob is absent or corrupt.
    Integrity,
    /// An immutable object identity is already published with another manifest.
    PublicationConflict,
}

impl From<alumina_storage::Error> for Error {
    fn from(error: alumina_storage::Error) -> Self {
        Self::Storage(error)
    }
}

/// Rebootable core-0 cache service model. It does not model a filesystem API.
#[derive(Clone, Debug)]
pub struct CacheService {
    limits: CacheLimits,
    durable: DurableCache,
    coordinator: UploadCoordinator,
    powered: bool,
}

impl CacheService {
    /// Boots an empty durable image.
    pub fn empty(limits: CacheLimits) -> Result<Self, Error> {
        Self::from_durable(DurableCache::default(), limits)
    }

    /// Boots a cloned durable image and reconciles recoverable transaction state.
    pub fn from_durable(durable: DurableCache, limits: CacheLimits) -> Result<Self, Error> {
        let mut service = Self {
            limits,
            durable,
            coordinator: UploadCoordinator::new(),
            powered: true,
        };
        service.reconcile_after_boot()?;
        Ok(service)
    }

    /// Snapshot equivalent to bytes that survived an immediate power cut.
    pub fn durable_snapshot(&self) -> DurableCache {
        self.durable.clone()
    }

    /// Whether simulated service power is currently present.
    pub const fn is_powered(&self) -> bool {
        self.powered
    }

    /// Reboots from only durable state; volatile coordinator state is discarded.
    pub fn reboot(&mut self) -> Result<(), Error> {
        self.powered = true;
        self.coordinator = UploadCoordinator::new();
        self.reconcile_after_boot()
    }

    /// Begins or idempotently resumes one upload and durably checkpoints it.
    pub fn begin_upload(
        &mut self,
        plan: UploadPlan,
        context: MutationContext,
    ) -> Result<UploadProgress, Error> {
        self.require_powered()?;
        let progress = self.coordinator.begin(plan, self.limits, context)?;
        let checkpoint = self.coordinator.checkpoint().ok_or(Error::CorruptJournal)?;
        match &mut self.durable.upload {
            Some(upload) if upload.checkpoint.plan == plan => {
                upload.checkpoint = checkpoint;
            }
            Some(_) => return Err(Error::PublicationConflict),
            None => {
                self.durable.upload = Some(DurableUpload {
                    checkpoint,
                    chunks: Vec::new(),
                });
            }
        }
        Ok(progress)
    }

    /// Verifies and durably records exactly the next chunk with optional power loss.
    pub fn put_chunk(
        &mut self,
        upload_id: UploadId,
        index: u32,
        claimed_content: ContentId,
        bytes: &[u8],
        context: MutationContext,
        cut: Option<PowerCut>,
    ) -> Result<UploadProgress, Error> {
        self.require_powered()?;
        context.validate()?;
        let token = self
            .coordinator
            .verify_chunk(upload_id, index, claimed_content, bytes)?;
        match self.durable.blobs.get(&token.content()) {
            Some(existing) if existing.as_slice() != bytes => return Err(Error::Integrity),
            Some(_) => {}
            None => {
                self.durable.blobs.insert(token.content(), bytes.to_vec());
            }
        }
        self.maybe_cut(cut, PowerCut::AfterChunkBlob)?;

        {
            let upload = self.durable.upload.as_mut().ok_or(Error::CorruptJournal)?;
            if upload.chunks.len() != usize::try_from(index).map_err(|_| Error::CorruptJournal)? {
                return Err(Error::CorruptJournal);
            }
            upload.chunks.push(ChunkRecord {
                index,
                content: token.content(),
                byte_len: token.byte_len(),
            });
        }
        self.maybe_cut(cut, PowerCut::AfterChunkJournal)?;

        let progress = self.coordinator.record_chunk(token, context)?;
        self.durable
            .upload
            .as_mut()
            .ok_or(Error::CorruptJournal)?
            .checkpoint = self.coordinator.checkpoint().ok_or(Error::CorruptJournal)?;
        Ok(progress)
    }

    /// Re-hashes durable blobs and manifest, then performs atomic publication.
    pub fn finalize_upload(
        &mut self,
        upload_id: UploadId,
        context: MutationContext,
        cut: Option<PowerCut>,
    ) -> Result<PublishedObject, Error> {
        self.require_powered()?;
        let durable_upload = self.durable.upload.as_ref().ok_or(Error::CorruptJournal)?;
        if durable_upload.checkpoint.plan.upload_id != upload_id {
            return Err(alumina_storage::Error::UploadIdMismatch {
                received: upload_id,
                expected: durable_upload.checkpoint.plan.upload_id,
            }
            .into());
        }
        let plan = durable_upload.checkpoint.plan;
        if durable_upload.chunks.len()
            != usize::try_from(plan.chunk_count).map_err(|_| Error::CorruptJournal)?
        {
            return Err(alumina_storage::Error::Incomplete {
                received: u32::try_from(durable_upload.chunks.len()).unwrap_or(u32::MAX),
                expected: plan.chunk_count,
            }
            .into());
        }

        let mut object_hasher = ContentHasher::new();
        let mut manifest_hasher =
            ManifestHasher::new(plan.object, plan.chunk_bytes, plan.chunk_count, self.limits)?;
        for record in &durable_upload.chunks {
            let bytes = self
                .durable
                .blobs
                .get(&record.content)
                .ok_or(Error::Integrity)?;
            if sha256(bytes) != record.content
                || bytes.len()
                    != usize::try_from(record.byte_len).map_err(|_| Error::CorruptJournal)?
            {
                return Err(Error::Integrity);
            }
            object_hasher.update(bytes);
            manifest_hasher.push(record.index, record.content, record.byte_len)?;
        }
        let observed_object = object_hasher.finalize();
        let observed_manifest = manifest_hasher.finalize()?;
        let publish =
            self.coordinator
                .finalize(upload_id, observed_object, observed_manifest, context)?;
        self.durable
            .upload
            .as_mut()
            .ok_or(Error::CorruptJournal)?
            .checkpoint = self.coordinator.checkpoint().ok_or(Error::CorruptJournal)?;
        self.maybe_cut(cut, PowerCut::AfterPublishPending)?;

        let record = PublishedRecord {
            object: publish.object(),
            manifest: publish.manifest(),
            chunk_bytes: plan.chunk_bytes,
            chunks: self
                .durable
                .upload
                .as_ref()
                .ok_or(Error::CorruptJournal)?
                .chunks
                .clone(),
        };
        self.durable.staging = Some(record.clone());
        self.maybe_cut(cut, PowerCut::AfterStagingManifest)?;

        if let Some(existing) = self.durable.published.get(&record.object.content)
            && existing != &record
        {
            return Err(Error::PublicationConflict);
        }
        self.durable.published.insert(record.object.content, record);
        self.durable.staging = None;
        self.maybe_cut(cut, PowerCut::AfterAtomicPublish)?;

        let published = self.coordinator.record_published(publish, context)?;
        self.durable.upload = None;
        Ok(published)
    }

    /// Reads and verifies one published object in canonical chunk order.
    pub fn read_published(&self, content: ContentId) -> Result<Vec<u8>, Error> {
        self.require_powered()?;
        let record = self
            .durable
            .published
            .get(&content)
            .ok_or(Error::Integrity)?;
        let mut bytes = Vec::with_capacity(
            usize::try_from(record.object.byte_len).map_err(|_| Error::Integrity)?,
        );
        for chunk in &record.chunks {
            let blob = self
                .durable
                .blobs
                .get(&chunk.content)
                .ok_or(Error::Integrity)?;
            if sha256(blob) != chunk.content {
                return Err(Error::Integrity);
            }
            bytes.extend_from_slice(blob);
        }
        if bytes.len() != usize::try_from(record.object.byte_len).map_err(|_| Error::Integrity)?
            || sha256(&bytes) != record.object.content
        {
            return Err(Error::Integrity);
        }
        Ok(bytes)
    }

    /// Re-verifies every visible object, chunk, and canonical manifest.
    pub fn scrub(&self) -> Result<(), Error> {
        self.require_powered()?;
        for record in self.durable.published.values() {
            let bytes = self.read_published(record.object.content)?;
            if sha256(&bytes) != record.object.content {
                return Err(Error::Integrity);
            }
            let mut manifest = ManifestHasher::new(
                record.object,
                record.chunk_bytes,
                u32::try_from(record.chunks.len()).map_err(|_| Error::Integrity)?,
                self.limits,
            )?;
            for chunk in &record.chunks {
                manifest.push(chunk.index, chunk.content, chunk.byte_len)?;
            }
            if manifest.finalize()? != record.manifest {
                return Err(Error::Integrity);
            }
        }
        Ok(())
    }

    /// Number of durable blobs referenced by no active or published manifest.
    pub fn orphan_blob_count(&self) -> usize {
        let mut referenced = BTreeSet::new();
        if let Some(upload) = &self.durable.upload {
            for chunk in &upload.chunks {
                referenced.insert(chunk.content);
            }
        }
        for record in self.durable.published.values() {
            for chunk in &record.chunks {
                referenced.insert(chunk.content);
            }
        }
        self.durable
            .blobs
            .keys()
            .filter(|content| !referenced.contains(content))
            .count()
    }

    /// Fault injection: changes one durable blob byte without changing its name.
    pub fn corrupt_blob(&mut self, content: ContentId, offset: usize) -> Result<(), Error> {
        let blob = self
            .durable
            .blobs
            .get_mut(&content)
            .ok_or(Error::Integrity)?;
        let byte = blob.get_mut(offset).ok_or(Error::Integrity)?;
        *byte ^= 0x80;
        Ok(())
    }

    /// Whether one blob exists and still hashes to its content-addressed name.
    pub fn blob_is_valid(&self, content: ContentId) -> bool {
        self.powered
            && self
                .durable
                .blobs
                .get(&content)
                .is_some_and(|bytes| sha256(bytes) == content)
    }

    fn reconcile_after_boot(&mut self) -> Result<(), Error> {
        self.durable.staging = None;
        let Some(upload) = &mut self.durable.upload else {
            self.coordinator = UploadCoordinator::new();
            return Ok(());
        };
        let journaled =
            usize::try_from(upload.checkpoint.next_chunk).map_err(|_| Error::CorruptJournal)?;
        if upload.chunks.len() < journaled {
            return Err(Error::CorruptJournal);
        }
        upload.chunks.truncate(journaled);
        self.coordinator = UploadCoordinator::restore(upload.checkpoint, self.limits)?;

        if upload.checkpoint.phase == UploadPhase::PublishPending
            && let Some(record) = self
                .durable
                .published
                .get(&upload.checkpoint.plan.object.content)
            && record.manifest == upload.checkpoint.plan.manifest
            && record.object == upload.checkpoint.plan.object
        {
            let publish = self.coordinator.finalize(
                upload.checkpoint.plan.upload_id,
                upload.checkpoint.plan.object.content,
                upload.checkpoint.plan.manifest,
                MutationContext::DISARMED_IDLE,
            )?;
            self.coordinator
                .record_published(publish, MutationContext::DISARMED_IDLE)?;
            self.durable.upload = None;
        }
        Ok(())
    }

    fn require_powered(&self) -> Result<(), Error> {
        if self.powered {
            Ok(())
        } else {
            Err(Error::PoweredOff)
        }
    }

    fn maybe_cut(&mut self, requested: Option<PowerCut>, point: PowerCut) -> Result<(), Error> {
        if requested == Some(point) {
            self.powered = false;
            Err(Error::PowerLoss(point))
        } else {
            Ok(())
        }
    }
}

/// One immutable block required by core 1 at an exact local-cycle interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledBlock {
    /// Strictly contiguous stream sequence.
    pub sequence: u32,
    /// Inclusive certified execution cycle.
    pub start_cycle: u64,
    /// Exclusive certified execution cycle.
    pub end_cycle: u64,
    /// Blob core 0 must verify before queue handoff.
    pub content: ContentId,
}

/// Interval during which core-0 storage/network service makes no progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceStall {
    /// Inclusive stall start.
    pub start_cycle: u64,
    /// Exclusive stall end.
    pub end_cycle: u64,
}

/// Fixed simulator service/queue budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamConfig {
    /// Maximum verified blocks in internal-SRAM handoff.
    pub queue_capacity: usize,
    /// Unstalled service cycles required to verify/prefetch one block.
    pub service_cycles_per_block: u64,
}

/// Fail-closed runtime result from a simulated cached stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamFault {
    /// Required block was not verified in the queue at its start cycle.
    Starvation {
        /// Missing block.
        sequence: u32,
        /// Local cycle at which execution refused to extend output.
        cycle: u64,
    },
    /// Core 0 discovered a missing/corrupt content-addressed block.
    Integrity {
        /// Rejected block.
        sequence: u32,
        /// Verification completion cycle.
        cycle: u64,
    },
}

/// Deterministic queue and terminal-state evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StreamReport {
    /// Blocks whose exact start was admitted.
    pub executed_blocks: usize,
    /// Largest internal-SRAM queue occupancy.
    pub maximum_queue_depth: usize,
    /// Last simulated local cycle (terminal end or fail-closed decision).
    pub final_cycle: u64,
    /// Runtime fault, if any.
    pub fault: Option<StreamFault>,
}

/// Static schedule/configuration rejection before simulation begins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamPlanError {
    /// Queue or service budget was zero.
    InvalidConfig,
    /// Block times/sequences were empty, reversed, or noncontiguous.
    InvalidBlock {
        /// Rejected table index.
        index: usize,
    },
    /// Stall intervals were empty, overlapping, or unsorted.
    InvalidStall {
        /// Rejected table index.
        index: usize,
    },
    /// Checked virtual-cycle arithmetic overflowed.
    Arithmetic,
}

/// Simulates concurrent core-0 prefetch and core-1 exact-cycle consumption.
pub fn simulate_stream(
    cache: &CacheService,
    blocks: &[ScheduledBlock],
    stalls: &[ServiceStall],
    config: StreamConfig,
) -> Result<StreamReport, StreamPlanError> {
    validate_stream_plan(blocks, stalls, config)?;
    let mut queue = VecDeque::with_capacity(config.queue_capacity);
    let mut next_load = 0_usize;
    let mut service_cursor = 0_u64;
    let mut inflight: Option<(usize, u64)> = None;
    let mut maximum_queue_depth = 0_usize;
    let mut executed_blocks = 0_usize;
    let mut final_cycle = 0_u64;

    for (execute_index, block) in blocks.iter().enumerate() {
        loop {
            if let Some((load_index, completion)) = inflight {
                if completion > block.start_cycle {
                    break;
                }
                let loaded = blocks[load_index];
                if !cache.blob_is_valid(loaded.content) {
                    return Ok(StreamReport {
                        executed_blocks,
                        maximum_queue_depth,
                        final_cycle: completion,
                        fault: Some(StreamFault::Integrity {
                            sequence: loaded.sequence,
                            cycle: completion,
                        }),
                    });
                }
                queue.push_back(load_index);
                maximum_queue_depth = maximum_queue_depth.max(queue.len());
                next_load = next_load
                    .checked_add(1)
                    .ok_or(StreamPlanError::Arithmetic)?;
                service_cursor = completion;
                inflight = None;
                continue;
            }
            if next_load >= blocks.len() || queue.len() >= config.queue_capacity {
                break;
            }
            let completion =
                finish_service_work(service_cursor, config.service_cycles_per_block, stalls)?;
            inflight = Some((next_load, completion));
        }

        if queue.front().copied() != Some(execute_index) {
            return Ok(StreamReport {
                executed_blocks,
                maximum_queue_depth,
                final_cycle: block.start_cycle,
                fault: Some(StreamFault::Starvation {
                    sequence: block.sequence,
                    cycle: block.start_cycle,
                }),
            });
        }
        queue.pop_front();
        executed_blocks += 1;
        final_cycle = block.end_cycle;
        if inflight.is_none() {
            service_cursor = service_cursor.max(block.start_cycle);
        }
    }

    Ok(StreamReport {
        executed_blocks,
        maximum_queue_depth,
        final_cycle,
        fault: None,
    })
}

fn validate_stream_plan(
    blocks: &[ScheduledBlock],
    stalls: &[ServiceStall],
    config: StreamConfig,
) -> Result<(), StreamPlanError> {
    if config.queue_capacity == 0 || config.service_cycles_per_block == 0 {
        return Err(StreamPlanError::InvalidConfig);
    }
    if blocks.is_empty() {
        return Err(StreamPlanError::InvalidBlock { index: 0 });
    }
    for (index, block) in blocks.iter().enumerate() {
        let invalid_sequence =
            index != 0 && blocks[index - 1].sequence.checked_add(1) != Some(block.sequence);
        let invalid_time = block.end_cycle <= block.start_cycle
            || (index != 0 && block.start_cycle != blocks[index - 1].end_cycle);
        if !block.content.is_valid() || invalid_sequence || invalid_time {
            return Err(StreamPlanError::InvalidBlock { index });
        }
    }
    for (index, stall) in stalls.iter().enumerate() {
        if stall.end_cycle <= stall.start_cycle
            || (index != 0 && stall.start_cycle < stalls[index - 1].end_cycle)
        {
            return Err(StreamPlanError::InvalidStall { index });
        }
    }
    Ok(())
}

fn finish_service_work(
    start: u64,
    work: u64,
    stalls: &[ServiceStall],
) -> Result<u64, StreamPlanError> {
    let mut cursor = start;
    let mut remaining = work;
    for stall in stalls {
        if stall.end_cycle <= cursor {
            continue;
        }
        if stall.start_cycle > cursor {
            let available = stall.start_cycle - cursor;
            if remaining <= available {
                return cursor
                    .checked_add(remaining)
                    .ok_or(StreamPlanError::Arithmetic);
            }
            remaining -= available;
        }
        cursor = cursor.max(stall.end_cycle);
    }
    cursor
        .checked_add(remaining)
        .ok_or(StreamPlanError::Arithmetic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alumina_job::{
        JobDescriptor, PrefetchYield, RealtimeJob, RealtimeJobState, RealtimePoll, ServiceJobState,
        ServicePrefetch,
    };
    use alumina_machine_ir::{
        BlockError, BlockValidationLimits, ExecutionBlock, ExecutionSegment, PartitionAssembler,
        StreamId, StreamTick, ValidationLimits,
    };
    use alumina_protocol::Digest;
    use alumina_runtime::IntercoreBoundary;
    use alumina_storage::media::{CacheMedia, MediaAvailability, MediaId, MediaRegion};
    use alumina_storage::provisioning::{CacheProvisionRequest, ProvisionedCache};
    use alumina_storage::{ChunkUploadHeader, FinalizeUploadRequest, ObjectKind, UploadId};
    use embassy_futures::block_on;

    const LIMITS: CacheLimits = CacheLimits {
        maximum_object_bytes: 64 * 1_024,
        maximum_chunk_bytes: 1_024,
        maximum_chunks: 4_096,
    };
    const MEDIA_REGION: MediaRegion = MediaRegion {
        start_block: 8,
        block_count: 120,
    };
    const PROVISIONED_DEVICE_BLOCKS: usize = 2_300;
    const PROVISIONED_REGION: MediaRegion = MediaRegion {
        start_block: 2_048,
        block_count: 200,
    };

    fn plan_for(bytes: &[u8], chunk_bytes: u32, upload_id: u64) -> (UploadPlan, Vec<&[u8]>) {
        let chunk_size = usize::try_from(chunk_bytes).unwrap();
        let chunks: Vec<_> = bytes.chunks(chunk_size).collect();
        let object = StoredObject {
            kind: ObjectKind::MachineJobPartition,
            content: sha256(bytes),
            byte_len: u64::try_from(bytes.len()).unwrap(),
        };
        let mut manifest = ManifestHasher::new(
            object,
            chunk_bytes,
            u32::try_from(chunks.len()).unwrap(),
            LIMITS,
        )
        .unwrap();
        for (index, chunk) in chunks.iter().enumerate() {
            manifest
                .push(
                    u32::try_from(index).unwrap(),
                    sha256(chunk),
                    u32::try_from(chunk.len()).unwrap(),
                )
                .unwrap();
        }
        (
            UploadPlan {
                upload_id: UploadId(upload_id),
                object,
                manifest: manifest.finalize().unwrap(),
                chunk_bytes,
                chunk_count: u32::try_from(chunks.len()).unwrap(),
            },
            chunks,
        )
    }

    fn upload_all(
        service: &mut CacheService,
        plan: UploadPlan,
        chunks: &[&[u8]],
    ) -> PublishedObject {
        service
            .begin_upload(plan, MutationContext::DISARMED_IDLE)
            .unwrap();
        for (index, chunk) in chunks.iter().enumerate() {
            service
                .put_chunk(
                    plan.upload_id,
                    u32::try_from(index).unwrap(),
                    sha256(chunk),
                    chunk,
                    MutationContext::DISARMED_IDLE,
                    None,
                )
                .unwrap();
        }
        service
            .finalize_upload(plan.upload_id, MutationContext::DISARMED_IDLE, None)
            .unwrap()
    }

    fn upload_media(
        media: &mut CacheMedia<SimBlockDevice>,
        plan: UploadPlan,
        chunks: &[&[u8]],
    ) -> PublishedObject {
        block_on(media.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        for (index, chunk) in chunks.iter().enumerate() {
            block_on(media.put_chunk(
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
        block_on(media.finalize_upload(
            FinalizeUploadRequest {
                upload_id: plan.upload_id,
            },
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap()
    }

    fn upload_provisioned(
        cache: &mut ProvisionedCache<SimBlockDevice>,
        plan: UploadPlan,
        chunks: &[&[u8]],
    ) -> PublishedObject {
        block_on(cache.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        for (index, chunk) in chunks.iter().enumerate() {
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
        block_on(cache.finalize_upload(
            FinalizeUploadRequest {
                upload_id: plan.upload_id,
            },
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap()
    }

    fn block_limits() -> BlockValidationLimits {
        BlockValidationLimits {
            maximum_block_ticks: 1_000,
            segment: ValidationLimits {
                maximum_segment_ticks: 1_000,
                maximum_steps_per_segment: 1_000,
            },
        }
    }

    #[test]
    fn public_block_simulator_recovers_every_torn_chunk_append() {
        let bytes = vec![0xa5; 1_024];
        let (plan, chunks) = plan_for(&bytes, 1_024, 11);
        let (device, control) = SimBlockDevice::erased(160);
        let mut media = CacheMedia::new(device, MEDIA_REGION, LIMITS);
        block_on(media.format(MediaId::new([0x5a; 16]).unwrap())).unwrap();
        block_on(media.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        let baseline = control.snapshot();

        // Nine persistent operations cover header, three payload blocks,
        // barriers, commit, and anchor; the tenth iteration proves success.
        for operation_from_now in 0..=9 {
            let (device, control) = SimBlockDevice::from_snapshot(baseline.clone());
            let mut media = CacheMedia::new(device, MEDIA_REGION, LIMITS);
            block_on(media.mount()).unwrap();
            control.arm_power_cut(operation_from_now, 137);
            let chunk = chunks[0];
            let result = block_on(media.put_chunk(
                ChunkUploadHeader {
                    upload_id: plan.upload_id,
                    index: 0,
                    byte_len: u32::try_from(chunk.len()).unwrap(),
                    content: sha256(chunk),
                },
                chunk,
                MutationContext::DISARMED_IDLE,
            ));
            control.restore_power();

            let status = block_on(media.mount()).unwrap();
            assert_eq!(status.availability, MediaAvailability::Ready);
            let next_chunk = status.upload.unwrap().next_chunk;
            assert!(next_chunk == 0 || next_chunk == 1);
            if result.is_ok() {
                assert_eq!(next_chunk, 1);
            }
        }
    }

    #[test]
    fn real_cache_chunks_feed_independently_validated_owned_work_blocks() {
        let mut object = Vec::new();
        let mut previous_digest = Digest::ZERO;
        for sequence in 0_u32..3 {
            let start = u64::from(sequence) * 100;
            let block = ExecutionBlock::encode_motion(
                StreamId::new([0x33; 16]).unwrap(),
                Digest([0x44; 32]),
                Digest([0x55; 32]),
                sequence,
                previous_digest,
                &[ExecutionSegment {
                    start_tick: StreamTick(start),
                    end_tick: StreamTick(start + 100),
                    delta_steps: [i64::from(sequence) + 1, -1, 0],
                    flags: 0,
                }],
            )
            .unwrap();
            previous_digest = block.header().block_digest;
            object.extend_from_slice(block.as_bytes());
        }
        let (plan, chunks) = plan_for(&object, 700, 0x3344);
        let (device, _) = SimBlockDevice::erased(PROVISIONED_DEVICE_BLOCKS);
        let mut cache = ProvisionedCache::new(device, LIMITS);
        block_on(cache.discover()).unwrap();
        let request = CacheProvisionRequest::new(
            u64::try_from(PROVISIONED_DEVICE_BLOCKS).unwrap(),
            0,
            None,
            PROVISIONED_REGION,
            MediaId::new([0x5a; 16]).unwrap(),
            false,
        )
        .unwrap();
        block_on(cache.provision(request, MutationContext::DISARMED_IDLE)).unwrap();
        let published = upload_provisioned(&mut cache, plan, &chunks);
        let descriptor = JobDescriptor {
            prepare_id: 0x7788,
            partition: published,
            stream_id: StreamId::new([0x33; 16]).unwrap(),
            capability_digest: Digest([0x44; 32]),
            config_digest: Digest([0x55; 32]),
            axis_count: 3,
            block_count: 3,
            first_tick: StreamTick(0),
            limits: block_limits(),
        };
        let mut prefetch = block_on(ServicePrefetch::<3>::open(&mut cache, descriptor)).unwrap();
        let mut realtime_job = RealtimeJob::<3>::prepare(descriptor).unwrap();
        type Boundary = IntercoreBoundary<1, 1, 4, 4, 2>;
        let boundary = Box::leak(Box::new(Boundary::new()));
        let (mut service, mut realtime) = boundary.split();

        assert_eq!(
            block_on(prefetch.step(&mut cache, &mut service))
                .unwrap()
                .yielded,
            PrefetchYield::Progress
        );
        assert_eq!(
            block_on(prefetch.step(&mut cache, &mut service))
                .unwrap()
                .yielded,
            PrefetchYield::Progress
        );
        assert_eq!(service.work_free_capacity(), 0);
        assert_eq!(
            block_on(prefetch.step(&mut cache, &mut service))
                .unwrap()
                .yielded,
            PrefetchYield::Backpressured
        );

        for _ in 0..8 {
            if realtime_job.status().state != RealtimeJobState::Complete {
                loop {
                    match realtime_job.poll(&mut realtime).unwrap() {
                        RealtimePoll::Block(admitted) => {
                            let _segments: Vec<_> = admitted.segments().unwrap().collect();
                            let complete = admitted.progress().complete;
                            realtime_job.acknowledge(admitted).unwrap();
                            if complete {
                                break;
                            }
                        }
                        RealtimePoll::Empty => break,
                        RealtimePoll::Outstanding => {
                            panic!("test acknowledges every admitted block")
                        }
                    }
                }
            }
            if prefetch.status().state != ServiceJobState::Complete {
                block_on(prefetch.step(&mut cache, &mut service)).unwrap();
            }
            if realtime_job.status().state == RealtimeJobState::Complete
                && prefetch.status().state == ServiceJobState::Complete
            {
                break;
            }
        }

        let completion = block_on(prefetch.step(&mut cache, &mut service)).unwrap();
        assert_eq!(completion.yielded, PrefetchYield::Complete);
        assert_eq!(completion.status.state, ServiceJobState::Complete);
        let service_progress = completion.status.final_progress.unwrap();
        let realtime_status = realtime_job.status();
        assert_eq!(realtime_status.state, RealtimeJobState::Complete);
        let realtime_progress = realtime_status.completed_progress.unwrap();
        assert_eq!(service_progress, realtime_progress);
        assert_eq!(service_progress.position, [6, -3, 0]);
        assert_eq!(service_progress.end_tick, StreamTick(300));
        assert_eq!(completion.status.storage_chunks_read, 3);
        assert_eq!(service.work_free_capacity(), 2);
    }

    #[test]
    fn storage_valid_but_machine_ir_corrupt_bytes_never_enter_work_queue() {
        let block = ExecutionBlock::encode_motion(
            StreamId::new([0x33; 16]).unwrap(),
            Digest([0x44; 32]),
            Digest([0x55; 32]),
            0,
            Digest::ZERO,
            &[ExecutionSegment {
                start_tick: StreamTick(1_000),
                end_tick: StreamTick(1_100),
                delta_steps: [1, 0, 0],
                flags: 0,
            }],
        )
        .unwrap();
        let mut corrupt = block.into_bytes();
        corrupt[180] ^= 1;
        let (plan, chunks) = plan_for(&corrupt, 512, 0x5566);
        let (device, _) = SimBlockDevice::erased(160);
        let mut media = CacheMedia::new(device, MEDIA_REGION, LIMITS);
        block_on(media.format(MediaId::new([0x5a; 16]).unwrap())).unwrap();
        let published = upload_media(&mut media, plan, &chunks);
        let mut reader = block_on(media.open_published(published)).unwrap();
        let mut storage = [0_u8; alumina_storage::media::MAX_MEDIA_CHUNK_BYTES];
        let chunk = block_on(media.read_next_published(&mut reader, &mut storage))
            .unwrap()
            .unwrap();
        assert!(reader.is_complete());
        let mut assembler = PartitionAssembler::new(plan.object.byte_len).unwrap();
        assert!(matches!(
            assembler.push(&storage[..usize::try_from(chunk.byte_len).unwrap()]),
            Err(BlockError::BlockDigest)
        ));
    }

    #[test]
    fn upload_publish_read_and_scrub_preserve_exact_bytes() {
        let bytes = b"a synthetic cached integer machine job";
        let (plan, chunks) = plan_for(bytes, 8, 7);
        let mut service = CacheService::empty(LIMITS).unwrap();
        assert_eq!(upload_all(&mut service, plan, &chunks).object, plan.object);
        assert_eq!(service.read_published(plan.object.content).unwrap(), bytes);
        assert_eq!(service.scrub(), Ok(()));
        assert_eq!(service.orphan_blob_count(), 0);
    }

    #[test]
    fn chunk_power_cuts_leave_resumable_state_or_unreferenced_blob() {
        for cut in [PowerCut::AfterChunkBlob, PowerCut::AfterChunkJournal] {
            let (plan, chunks) = plan_for(b"abcdefghij", 4, 7);
            let mut service = CacheService::empty(LIMITS).unwrap();
            service
                .begin_upload(plan, MutationContext::DISARMED_IDLE)
                .unwrap();
            assert_eq!(
                service.put_chunk(
                    plan.upload_id,
                    0,
                    sha256(chunks[0]),
                    chunks[0],
                    MutationContext::DISARMED_IDLE,
                    Some(cut),
                ),
                Err(Error::PowerLoss(cut))
            );
            assert!(!service.is_powered());
            service.reboot().unwrap();
            assert_eq!(service.coordinator.checkpoint().unwrap().next_chunk, 0);
            service
                .put_chunk(
                    plan.upload_id,
                    0,
                    sha256(chunks[0]),
                    chunks[0],
                    MutationContext::DISARMED_IDLE,
                    None,
                )
                .unwrap();
        }
    }

    #[test]
    fn every_publish_power_cut_recovers_without_visible_corruption() {
        for cut in [
            PowerCut::AfterPublishPending,
            PowerCut::AfterStagingManifest,
            PowerCut::AfterAtomicPublish,
        ] {
            let bytes = b"abcdefghij";
            let (plan, chunks) = plan_for(bytes, 4, 7);
            let mut service = CacheService::empty(LIMITS).unwrap();
            service
                .begin_upload(plan, MutationContext::DISARMED_IDLE)
                .unwrap();
            for (index, chunk) in chunks.iter().enumerate() {
                service
                    .put_chunk(
                        plan.upload_id,
                        u32::try_from(index).unwrap(),
                        sha256(chunk),
                        chunk,
                        MutationContext::DISARMED_IDLE,
                        None,
                    )
                    .unwrap();
            }
            assert_eq!(
                service.finalize_upload(plan.upload_id, MutationContext::DISARMED_IDLE, Some(cut),),
                Err(Error::PowerLoss(cut))
            );
            service.reboot().unwrap();
            if service.read_published(plan.object.content).is_err() {
                service
                    .finalize_upload(plan.upload_id, MutationContext::DISARMED_IDLE, None)
                    .unwrap();
            }
            assert_eq!(service.read_published(plan.object.content).unwrap(), bytes);
            assert_eq!(service.scrub(), Ok(()));
        }
    }

    #[test]
    fn scrub_and_prefetch_detect_content_address_corruption() {
        let (plan, chunks) = plan_for(b"abcdefghij", 4, 7);
        let mut service = CacheService::empty(LIMITS).unwrap();
        upload_all(&mut service, plan, &chunks);
        let first = sha256(chunks[0]);
        service.corrupt_blob(first, 0).unwrap();
        assert_eq!(service.scrub(), Err(Error::Integrity));
        assert!(!service.blob_is_valid(first));
    }

    #[test]
    fn armed_upload_attempt_cannot_write_even_an_orphan_blob() {
        let (plan, chunks) = plan_for(b"abcdefghij", 4, 7);
        let mut service = CacheService::empty(LIMITS).unwrap();
        service
            .begin_upload(plan, MutationContext::DISARMED_IDLE)
            .unwrap();
        let armed = MutationContext {
            armed_or_energized: true,
            realtime_job_active: false,
        };
        assert_eq!(
            service.put_chunk(plan.upload_id, 0, sha256(chunks[0]), chunks[0], armed, None,),
            Err(Error::Storage(alumina_storage::Error::MutationForbidden))
        );
        assert_eq!(service.orphan_blob_count(), 0);
        assert_eq!(service.coordinator.checkpoint().unwrap().next_chunk, 0);
    }

    #[test]
    fn bounded_prefetch_streams_a_long_cached_job() {
        let bytes: Vec<u8> = (0_u16..4_096).map(|value| value as u8).collect();
        let (plan, chunks) = plan_for(&bytes, 4, 7);
        let mut service = CacheService::empty(LIMITS).unwrap();
        upload_all(&mut service, plan, &chunks);
        let blocks: Vec<_> = chunks
            .iter()
            .enumerate()
            .map(|(index, chunk)| ScheduledBlock {
                sequence: u32::try_from(index).unwrap(),
                start_cycle: 10 + u64::try_from(index).unwrap() * 5,
                end_cycle: 15 + u64::try_from(index).unwrap() * 5,
                content: sha256(chunk),
            })
            .collect();
        let report = simulate_stream(
            &service,
            &blocks,
            &[],
            StreamConfig {
                queue_capacity: 4,
                service_cycles_per_block: 1,
            },
        )
        .unwrap();
        assert_eq!(report.executed_blocks, chunks.len());
        assert_eq!(report.fault, None);
        assert!(report.maximum_queue_depth <= 4);
    }

    #[test]
    fn service_stall_faults_before_extending_an_empty_queue() {
        let (plan, chunks) = plan_for(b"abcdefghijkl", 4, 7);
        let mut service = CacheService::empty(LIMITS).unwrap();
        upload_all(&mut service, plan, &chunks);
        let blocks = [
            ScheduledBlock {
                sequence: 0,
                start_cycle: 10,
                end_cycle: 20,
                content: sha256(chunks[0]),
            },
            ScheduledBlock {
                sequence: 1,
                start_cycle: 20,
                end_cycle: 30,
                content: sha256(chunks[1]),
            },
            ScheduledBlock {
                sequence: 2,
                start_cycle: 30,
                end_cycle: 40,
                content: sha256(chunks[2]),
            },
        ];
        let report = simulate_stream(
            &service,
            &blocks,
            &[ServiceStall {
                start_cycle: 10,
                end_cycle: 35,
            }],
            StreamConfig {
                queue_capacity: 2,
                service_cycles_per_block: 2,
            },
        )
        .unwrap();
        assert_eq!(report.executed_blocks, 2);
        assert_eq!(
            report.fault,
            Some(StreamFault::Starvation {
                sequence: 2,
                cycle: 30,
            })
        );
        assert_eq!(report.final_cycle, 30);
    }

    #[test]
    fn empty_or_wrapped_streams_reject_before_execution() {
        let service = CacheService::empty(LIMITS).unwrap();
        let config = StreamConfig {
            queue_capacity: 1,
            service_cycles_per_block: 1,
        };
        assert_eq!(
            simulate_stream(&service, &[], &[], config),
            Err(StreamPlanError::InvalidBlock { index: 0 })
        );
        let content = sha256(b"block");
        let blocks = [
            ScheduledBlock {
                sequence: u32::MAX,
                start_cycle: 1,
                end_cycle: 2,
                content,
            },
            ScheduledBlock {
                sequence: 0,
                start_cycle: 2,
                end_cycle: 3,
                content,
            },
        ];
        assert_eq!(
            simulate_stream(&service, &blocks, &[], config),
            Err(StreamPlanError::InvalidBlock { index: 1 })
        );
    }
}
