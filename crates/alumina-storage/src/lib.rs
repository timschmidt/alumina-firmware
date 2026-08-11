#![no_std]
#![doc = "Bounded, resumable, content-addressed storage transactions for Alumina."]

use alumina_protocol::Digest;
use sha2::{Digest as _, Sha256};

pub mod media;
pub mod provisioning;

/// Magic identifying the canonical ordered chunk-manifest hash stream.
pub const MANIFEST_MAGIC: [u8; 4] = *b"ACMF";

/// Exact canonical manifest schema implemented here.
pub const MANIFEST_VERSION: u16 = 1;

/// Content algorithm admitted by storage schema V1.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum DigestAlgorithm {
    /// SHA-256, selected for browser WebCrypto and ESP hardware/software support.
    Sha256 = 1,
}

/// Algorithm-tagged immutable content identity.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ContentId {
    /// Hash algorithm; V1 admits only SHA-256.
    pub algorithm: DigestAlgorithm,
    /// Exact 256-bit result.
    pub digest: Digest,
}

impl ContentId {
    /// Creates a SHA-256 identity from already verified digest bytes.
    pub const fn from_sha256(digest: Digest) -> Self {
        Self {
            algorithm: DigestAlgorithm::Sha256,
            digest,
        }
    }

    /// Whether this identity is established and admitted by V1.
    pub const fn is_valid(self) -> bool {
        matches!(self.algorithm, DigestAlgorithm::Sha256) && !self.digest.is_zero()
    }
}

impl DigestAlgorithm {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Sha256),
            _ => None,
        }
    }
}

/// Computes the canonical V1 content identity for one complete byte slice.
pub fn sha256(bytes: &[u8]) -> ContentId {
    let mut hasher = ContentHasher::new();
    hasher.update(bytes);
    hasher.finalize()
}

/// Incremental SHA-256 verifier used while streaming files from an SD backend.
#[derive(Clone)]
pub struct ContentHasher(Sha256);

impl ContentHasher {
    /// Starts an empty SHA-256 stream.
    pub fn new() -> Self {
        Self(Sha256::new())
    }

    /// Adds the next exact byte range.
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    /// Consumes the stream and returns its canonical content identity.
    pub fn finalize(self) -> ContentId {
        let result = self.0.finalize();
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&result);
        ContentId::from_sha256(Digest(digest))
    }
}

impl Default for ContentHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Executability and retention policy of a stored object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ObjectKind {
    /// Integer/fixed-point work for exactly one MCU.
    MachineJobPartition = 1,
    /// Global/per-MCU immutable job manifest.
    MachineJobManifest = 2,
    /// Exact UI/WASM bundle paired with this firmware.
    InterfaceBundle = 3,
    /// Signed coupled firmware/interface update package.
    UpdateBundle = 4,
    /// Explicitly non-executable user/audit bytes.
    OpaqueData = 5,
    /// Canonical machine/resource configuration, inert until separately activated.
    MachineConfiguration = 6,
}

impl ObjectKind {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::MachineJobPartition),
            2 => Some(Self::MachineJobManifest),
            3 => Some(Self::InterfaceBundle),
            4 => Some(Self::UpdateBundle),
            5 => Some(Self::OpaqueData),
            6 => Some(Self::MachineConfiguration),
            _ => None,
        }
    }
}

/// Immutable object declared before upload begins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoredObject {
    /// Typed use; opaque data can never become machine work implicitly.
    pub kind: ObjectKind,
    /// Digest over exactly `byte_len` object bytes.
    pub content: ContentId,
    /// Exact object byte length.
    pub byte_len: u64,
}

/// Boot-independent upload transaction identity selected by the authenticated client.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct UploadId(pub u64);

/// Complete immutable upload declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadPlan {
    /// Nonzero idempotency/resume identity.
    pub upload_id: UploadId,
    /// Object to publish.
    pub object: StoredObject,
    /// Digest of the canonical ordered chunk manifest.
    pub manifest: ContentId,
    /// Fixed maximum bytes per chunk; only the last may be shorter.
    pub chunk_bytes: u32,
    /// Exact number of sequential chunks.
    pub chunk_count: u32,
}

impl UploadPlan {
    /// Exact `StorageBeginUpload` body length in protocol V1.
    pub const WIRE_LEN: usize = 96;

    /// Validates sizes, counts, and identities against device storage policy.
    pub fn validate(self, limits: CacheLimits) -> Result<(), Error> {
        limits.validate()?;
        if self.upload_id.0 == 0 {
            return Err(Error::InvalidUploadId);
        }
        if !self.manifest.is_valid() {
            return Err(Error::MissingDigest {
                role: DigestRole::Manifest,
            });
        }
        validate_layout(self.object, self.chunk_bytes, self.chunk_count, limits)
    }

    fn expected_chunk_len(self, index: u32) -> Option<u32> {
        expected_chunk_len(self.object, self.chunk_bytes, self.chunk_count, index)
    }

    /// Encodes a canonical fixed-size `StorageBeginUpload` operation body.
    pub fn encode(self) -> [u8; Self::WIRE_LEN] {
        let mut encoded = [0_u8; Self::WIRE_LEN];
        encoded[0..8].copy_from_slice(&self.upload_id.0.to_le_bytes());
        encoded[8] = self.object.kind as u8;
        encoded[9] = self.object.content.algorithm as u8;
        // Bytes 10..12 are reserved zero in V1.
        encoded[12..44].copy_from_slice(&self.object.content.digest.0);
        encoded[44..52].copy_from_slice(&self.object.byte_len.to_le_bytes());
        encoded[52] = self.manifest.algorithm as u8;
        // Bytes 53..56 are reserved zero in V1.
        encoded[56..88].copy_from_slice(&self.manifest.digest.0);
        encoded[88..92].copy_from_slice(&self.chunk_bytes.to_le_bytes());
        encoded[92..96].copy_from_slice(&self.chunk_count.to_le_bytes());
        encoded
    }

    /// Decodes and validates a canonical `StorageBeginUpload` operation body.
    pub fn decode(encoded: &[u8], limits: CacheLimits) -> Result<Self, WireError> {
        require_wire_len(encoded, Self::WIRE_LEN)?;
        let object_kind = ObjectKind::from_wire(encoded[8]).ok_or(WireError::ObjectKind {
            received: encoded[8],
        })?;
        let object_algorithm =
            DigestAlgorithm::from_wire(encoded[9]).ok_or(WireError::Algorithm {
                received: encoded[9],
            })?;
        if encoded[10..12].iter().any(|byte| *byte != 0) {
            return Err(WireError::Reserved);
        }
        let manifest_algorithm =
            DigestAlgorithm::from_wire(encoded[52]).ok_or(WireError::Algorithm {
                received: encoded[52],
            })?;
        if encoded[53..56].iter().any(|byte| *byte != 0) {
            return Err(WireError::Reserved);
        }
        let mut object_digest = [0_u8; 32];
        object_digest.copy_from_slice(&encoded[12..44]);
        let mut manifest_digest = [0_u8; 32];
        manifest_digest.copy_from_slice(&encoded[56..88]);
        let plan = Self {
            upload_id: UploadId(read_u64(encoded, 0)),
            object: StoredObject {
                kind: object_kind,
                content: ContentId {
                    algorithm: object_algorithm,
                    digest: Digest(object_digest),
                },
                byte_len: read_u64(encoded, 44),
            },
            manifest: ContentId {
                algorithm: manifest_algorithm,
                digest: Digest(manifest_digest),
            },
            chunk_bytes: read_u32(encoded, 88),
            chunk_count: read_u32(encoded, 92),
        };
        plan.validate(limits).map_err(WireError::Plan)?;
        Ok(plan)
    }
}

fn validate_layout(
    object: StoredObject,
    chunk_bytes: u32,
    chunk_count: u32,
    limits: CacheLimits,
) -> Result<(), Error> {
    limits.validate()?;
    if !object.content.is_valid() {
        return Err(Error::MissingDigest {
            role: DigestRole::Object,
        });
    }
    if object.byte_len == 0 {
        return Err(Error::EmptyObject);
    }
    if object.byte_len > limits.maximum_object_bytes {
        return Err(Error::ObjectTooLarge {
            received: object.byte_len,
            maximum: limits.maximum_object_bytes,
        });
    }
    if chunk_bytes == 0 || chunk_bytes > limits.maximum_chunk_bytes {
        return Err(Error::ChunkSize {
            received: chunk_bytes,
            maximum: limits.maximum_chunk_bytes,
        });
    }
    let expected = object.byte_len.div_ceil(u64::from(chunk_bytes));
    if expected > u64::from(limits.maximum_chunks) || u64::from(chunk_count) != expected {
        return Err(Error::ChunkCount {
            received: chunk_count,
            expected,
            maximum: limits.maximum_chunks,
        });
    }
    Ok(())
}

fn expected_chunk_len(
    object: StoredObject,
    chunk_bytes: u32,
    chunk_count: u32,
    index: u32,
) -> Option<u32> {
    if index >= chunk_count {
        return None;
    }
    if index + 1 < chunk_count {
        return Some(chunk_bytes);
    }
    let preceding = u64::from(chunk_bytes).checked_mul(u64::from(index))?;
    u32::try_from(object.byte_len.checked_sub(preceding)?).ok()
}

/// Compile-time/runtime storage admission budget for one board configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheLimits {
    /// Largest single immutable object.
    pub maximum_object_bytes: u64,
    /// Largest request/body chunk that may enter bounded service RAM.
    pub maximum_chunk_bytes: u32,
    /// Largest sequential chunk journal for one object.
    pub maximum_chunks: u32,
}

impl CacheLimits {
    fn validate(self) -> Result<(), Error> {
        if self.maximum_object_bytes == 0
            || self.maximum_chunk_bytes == 0
            || self.maximum_chunks == 0
        {
            return Err(Error::InvalidLimits);
        }
        Ok(())
    }
}

/// Streaming encoder/hasher for the canonical V1 ordered chunk manifest.
#[derive(Clone)]
pub struct ManifestHasher {
    object: StoredObject,
    chunk_bytes: u32,
    chunk_count: u32,
    next_chunk: u32,
    accepted_bytes: u64,
    hasher: ContentHasher,
}

impl ManifestHasher {
    /// Starts a manifest after validating the exact object/chunk layout.
    pub fn new(
        object: StoredObject,
        chunk_bytes: u32,
        chunk_count: u32,
        limits: CacheLimits,
    ) -> Result<Self, Error> {
        validate_layout(object, chunk_bytes, chunk_count, limits)?;
        let mut hasher = ContentHasher::new();
        let mut prefix = [0_u8; 56];
        prefix[0..4].copy_from_slice(&MANIFEST_MAGIC);
        prefix[4..6].copy_from_slice(&MANIFEST_VERSION.to_le_bytes());
        prefix[6] = object.kind as u8;
        prefix[7] = object.content.algorithm as u8;
        prefix[8..40].copy_from_slice(&object.content.digest.0);
        prefix[40..48].copy_from_slice(&object.byte_len.to_le_bytes());
        prefix[48..52].copy_from_slice(&chunk_bytes.to_le_bytes());
        prefix[52..56].copy_from_slice(&chunk_count.to_le_bytes());
        hasher.update(&prefix);
        Ok(Self {
            object,
            chunk_bytes,
            chunk_count,
            next_chunk: 0,
            accepted_bytes: 0,
            hasher,
        })
    }

    /// Adds exactly the next ordered, independently verified chunk descriptor.
    pub fn push(&mut self, index: u32, content: ContentId, byte_len: u32) -> Result<(), Error> {
        if index != self.next_chunk {
            return Err(Error::UnexpectedChunk {
                received: index,
                expected: self.next_chunk,
            });
        }
        let expected = expected_chunk_len(self.object, self.chunk_bytes, self.chunk_count, index)
            .ok_or(Error::UnexpectedChunk {
            received: index,
            expected: self.next_chunk,
        })?;
        if byte_len != expected {
            return Err(Error::ChunkLength {
                received: u64::from(byte_len),
                expected,
            });
        }
        if !content.is_valid() {
            return Err(Error::ChunkDigest { index });
        }
        let mut entry = [0_u8; 44];
        entry[0..4].copy_from_slice(&index.to_le_bytes());
        entry[4..8].copy_from_slice(&byte_len.to_le_bytes());
        entry[8] = content.algorithm as u8;
        // Bytes 9..12 are reserved zero in V1.
        entry[12..44].copy_from_slice(&content.digest.0);
        self.hasher.update(&entry);
        self.next_chunk = self.next_chunk.checked_add(1).ok_or(Error::Arithmetic)?;
        self.accepted_bytes = self
            .accepted_bytes
            .checked_add(u64::from(byte_len))
            .ok_or(Error::Arithmetic)?;
        Ok(())
    }

    /// Returns the canonical manifest identity only after exact completion.
    pub fn finalize(self) -> Result<ContentId, Error> {
        if self.next_chunk != self.chunk_count || self.accepted_bytes != self.object.byte_len {
            return Err(Error::Incomplete {
                received: self.next_chunk,
                expected: self.chunk_count,
            });
        }
        Ok(self.hasher.finalize())
    }
}

/// Safety/workload facts required for every durable mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MutationContext {
    /// True for `Armed`, `Running`, `Hold`, or any torque/process-enabled state.
    pub armed_or_energized: bool,
    /// True while a cached stream or another deterministic job owns storage service.
    pub realtime_job_active: bool,
}

impl MutationContext {
    /// Context admitted for upload, publication, deletion, and repair.
    pub const DISARMED_IDLE: Self = Self {
        armed_or_energized: false,
        realtime_job_active: false,
    };

    /// Rejects every durable mutation while energized or serving RT work.
    pub const fn validate(self) -> Result<(), Error> {
        if self.allows_mutation() {
            Ok(())
        } else {
            Err(Error::MutationForbidden)
        }
    }

    const fn allows_mutation(self) -> bool {
        !self.armed_or_energized && !self.realtime_job_active
    }
}

/// Durable upload phase represented in the small journal/checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum UploadPhase {
    /// More sequential chunks may be verified and committed.
    Receiving = 1,
    /// All bytes/digests passed; atomic manifest publication is pending.
    PublishPending = 2,
}

impl UploadPhase {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Receiving),
            2 => Some(Self::PublishPending),
            _ => None,
        }
    }
}

/// Minimal durable checkpoint; chunk bytes and ordered descriptors live on storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadCheckpoint {
    /// Immutable declaration.
    pub plan: UploadPlan,
    /// First chunk index not durably journaled.
    pub next_chunk: u32,
    /// Bytes durably represented by preceding chunk journal entries.
    pub accepted_bytes: u64,
    /// Resume phase.
    pub phase: UploadPhase,
}

/// Public bounded progress returned to the UI and health service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadProgress {
    /// Upload identity.
    pub upload_id: UploadId,
    /// Current durable phase.
    pub phase: UploadPhase,
    /// First required sequential chunk.
    pub next_chunk: u32,
    /// Exact durably journaled bytes.
    pub accepted_bytes: u64,
    /// Declared total bytes.
    pub total_bytes: u64,
}

impl UploadProgress {
    /// Exact V1 response body length.
    pub const WIRE_LEN: usize = 32;

    /// Encodes progress using explicit little-endian fields.
    pub fn encode(self) -> [u8; Self::WIRE_LEN] {
        let mut encoded = [0_u8; Self::WIRE_LEN];
        encoded[0..8].copy_from_slice(&self.upload_id.0.to_le_bytes());
        encoded[8] = self.phase as u8;
        // Bytes 9..12 are reserved zero in V1.
        encoded[12..16].copy_from_slice(&self.next_chunk.to_le_bytes());
        encoded[16..24].copy_from_slice(&self.accepted_bytes.to_le_bytes());
        encoded[24..32].copy_from_slice(&self.total_bytes.to_le_bytes());
        encoded
    }

    /// Decodes a structurally valid V1 progress response.
    pub fn decode(encoded: &[u8]) -> Result<Self, WireError> {
        require_wire_len(encoded, Self::WIRE_LEN)?;
        if encoded[9..12].iter().any(|byte| *byte != 0) {
            return Err(WireError::Reserved);
        }
        let phase = UploadPhase::from_wire(encoded[8]).ok_or(WireError::Phase {
            received: encoded[8],
        })?;
        let progress = Self {
            upload_id: UploadId(read_u64(encoded, 0)),
            phase,
            next_chunk: read_u32(encoded, 12),
            accepted_bytes: read_u64(encoded, 16),
            total_bytes: read_u64(encoded, 24),
        };
        if progress.upload_id.0 == 0
            || progress.total_bytes == 0
            || progress.accepted_bytes > progress.total_bytes
        {
            return Err(WireError::Progress);
        }
        Ok(progress)
    }
}

/// Fixed prefix followed immediately by one chunk's exact bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkUploadHeader {
    /// Active upload identity.
    pub upload_id: UploadId,
    /// Required sequential chunk index.
    pub index: u32,
    /// Exact bytes following this prefix.
    pub byte_len: u32,
    /// Claimed SHA-256 identity verified before durable acknowledgement.
    pub content: ContentId,
}

impl ChunkUploadHeader {
    /// Exact V1 prefix length before chunk bytes.
    pub const WIRE_LEN: usize = 52;

    /// Encodes the fixed chunk prefix.
    pub fn encode(self) -> [u8; Self::WIRE_LEN] {
        let mut encoded = [0_u8; Self::WIRE_LEN];
        encoded[0..8].copy_from_slice(&self.upload_id.0.to_le_bytes());
        encoded[8..12].copy_from_slice(&self.index.to_le_bytes());
        encoded[12..16].copy_from_slice(&self.byte_len.to_le_bytes());
        encoded[16] = self.content.algorithm as u8;
        // Bytes 17..20 are reserved zero in V1.
        encoded[20..52].copy_from_slice(&self.content.digest.0);
        encoded
    }

    /// Decodes and checks one fixed chunk prefix.
    pub fn decode(encoded: &[u8]) -> Result<Self, WireError> {
        require_wire_len(encoded, Self::WIRE_LEN)?;
        let algorithm = DigestAlgorithm::from_wire(encoded[16]).ok_or(WireError::Algorithm {
            received: encoded[16],
        })?;
        if encoded[17..20].iter().any(|byte| *byte != 0) {
            return Err(WireError::Reserved);
        }
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&encoded[20..52]);
        let header = Self {
            upload_id: UploadId(read_u64(encoded, 0)),
            index: read_u32(encoded, 8),
            byte_len: read_u32(encoded, 12),
            content: ContentId {
                algorithm,
                digest: Digest(digest),
            },
        };
        if header.upload_id.0 == 0 || header.byte_len == 0 || !header.content.is_valid() {
            return Err(WireError::ChunkHeader);
        }
        Ok(header)
    }

    /// Checks the enclosing message body consumes exactly prefix plus chunk bytes.
    pub fn validate_body_len(self, body_len: u32) -> Result<(), WireError> {
        let expected = u32::try_from(Self::WIRE_LEN)
            .expect("chunk header length fits u32")
            .checked_add(self.byte_len)
            .ok_or(WireError::BodyLength {
                received: body_len,
                expected: None,
            })?;
        if body_len != expected {
            return Err(WireError::BodyLength {
                received: body_len,
                expected: Some(expected),
            });
        }
        Ok(())
    }
}

/// Fixed `StorageFinalize` request body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinalizeUploadRequest {
    /// Upload to hash and atomically publish.
    pub upload_id: UploadId,
}

impl FinalizeUploadRequest {
    /// Exact V1 body length.
    pub const WIRE_LEN: usize = 8;

    /// Encodes the request.
    pub const fn encode(self) -> [u8; Self::WIRE_LEN] {
        self.upload_id.0.to_le_bytes()
    }

    /// Decodes a nonzero upload identity.
    pub fn decode(encoded: &[u8]) -> Result<Self, WireError> {
        require_wire_len(encoded, Self::WIRE_LEN)?;
        let request = Self {
            upload_id: UploadId(read_u64(encoded, 0)),
        };
        if request.upload_id.0 == 0 {
            return Err(WireError::UploadId);
        }
        Ok(request)
    }
}

/// Token proving bytes passed chunk length and SHA-256 verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedChunk {
    plan: UploadPlan,
    index: u32,
    content: ContentId,
    byte_len: u32,
}

impl VerifiedChunk {
    /// Content-addressed chunk name for the storage backend.
    pub const fn content(self) -> ContentId {
        self.content
    }

    /// Exact byte count the backend must durably write before recording.
    pub const fn byte_len(self) -> u32 {
        self.byte_len
    }

    /// Sequential manifest index.
    pub const fn index(self) -> u32 {
        self.index
    }
}

/// Token passed to the atomic manifest publication backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishToken {
    upload_id: UploadId,
    object: StoredObject,
    manifest: ContentId,
}

impl PublishToken {
    /// Immutable object being made visible.
    pub const fn object(self) -> StoredObject {
        self.object
    }

    /// Verified canonical manifest identity.
    pub const fn manifest(self) -> ContentId {
        self.manifest
    }
}

/// Object and manifest successfully made visible by an atomic backend commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublishedObject {
    /// Published immutable object.
    pub object: StoredObject,
    /// Published ordered manifest identity.
    pub manifest: ContentId,
}

/// Small no-allocation transaction coordinator; it never owns filesystem bytes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UploadCoordinator {
    active: Option<UploadCheckpoint>,
}

impl UploadCoordinator {
    /// Starts with no mutable transaction.
    pub const fn new() -> Self {
        Self { active: None }
    }

    /// Restores a durable journal only after all derived counters validate.
    pub fn restore(checkpoint: UploadCheckpoint, limits: CacheLimits) -> Result<Self, Error> {
        checkpoint.plan.validate(limits)?;
        if checkpoint.next_chunk > checkpoint.plan.chunk_count {
            return Err(Error::CorruptCheckpoint);
        }
        let expected_bytes = if checkpoint.next_chunk == checkpoint.plan.chunk_count {
            checkpoint.plan.object.byte_len
        } else {
            u64::from(checkpoint.plan.chunk_bytes)
                .checked_mul(u64::from(checkpoint.next_chunk))
                .ok_or(Error::Arithmetic)?
        };
        if checkpoint.accepted_bytes != expected_bytes
            || (checkpoint.phase == UploadPhase::PublishPending
                && checkpoint.next_chunk != checkpoint.plan.chunk_count)
        {
            return Err(Error::CorruptCheckpoint);
        }
        Ok(Self {
            active: Some(checkpoint),
        })
    }

    /// Returns the exact journal state to persist after each mutation.
    pub const fn checkpoint(&self) -> Option<UploadCheckpoint> {
        self.active
    }

    /// Begins or idempotently resumes one immutable upload declaration.
    pub fn begin(
        &mut self,
        plan: UploadPlan,
        limits: CacheLimits,
        context: MutationContext,
    ) -> Result<UploadProgress, Error> {
        require_mutation(context)?;
        plan.validate(limits)?;
        match self.active {
            Some(active) if active.plan == plan => Ok(progress(active)),
            Some(_) => Err(Error::UploadConflict),
            None => {
                let checkpoint = UploadCheckpoint {
                    plan,
                    next_chunk: 0,
                    accepted_bytes: 0,
                    phase: UploadPhase::Receiving,
                };
                self.active = Some(checkpoint);
                Ok(progress(checkpoint))
            }
        }
    }

    /// Hashes and validates the next chunk without advancing durable state.
    ///
    /// The service backend writes/fsyncs bytes under `token.content()` and its
    /// ordered journal entry before passing this token to [`Self::record_chunk`].
    pub fn verify_chunk(
        &self,
        upload_id: UploadId,
        index: u32,
        claimed_content: ContentId,
        bytes: &[u8],
    ) -> Result<VerifiedChunk, Error> {
        let active = self.active.ok_or(Error::NoUpload)?;
        check_session(active, upload_id)?;
        if active.phase != UploadPhase::Receiving {
            return Err(Error::PublishPending);
        }
        if index != active.next_chunk {
            return Err(Error::UnexpectedChunk {
                received: index,
                expected: active.next_chunk,
            });
        }
        let expected = active
            .plan
            .expected_chunk_len(index)
            .ok_or(Error::UnexpectedChunk {
                received: index,
                expected: active.next_chunk,
            })?;
        let received = u32::try_from(bytes.len()).map_err(|_| Error::ChunkLength {
            received: u64::MAX,
            expected,
        })?;
        if received != expected {
            return Err(Error::ChunkLength {
                received: u64::from(received),
                expected,
            });
        }
        if !claimed_content.is_valid() || sha256(bytes) != claimed_content {
            return Err(Error::ChunkDigest { index });
        }
        Ok(VerifiedChunk {
            plan: active.plan,
            index,
            content: claimed_content,
            byte_len: received,
        })
    }

    /// Advances the checkpoint after the backend durably stores the verified token.
    pub fn record_chunk(
        &mut self,
        token: VerifiedChunk,
        context: MutationContext,
    ) -> Result<UploadProgress, Error> {
        require_mutation(context)?;
        let mut active = self.active.ok_or(Error::NoUpload)?;
        check_session(active, token.plan.upload_id)?;
        if token.plan != active.plan {
            return Err(Error::VerifiedChunkMismatch);
        }
        if active.phase != UploadPhase::Receiving {
            return Err(Error::PublishPending);
        }
        if token.index != active.next_chunk {
            return Err(Error::UnexpectedChunk {
                received: token.index,
                expected: active.next_chunk,
            });
        }
        let expected =
            active
                .plan
                .expected_chunk_len(token.index)
                .ok_or(Error::UnexpectedChunk {
                    received: token.index,
                    expected: active.next_chunk,
                })?;
        if token.byte_len != expected || !token.content.is_valid() {
            return Err(Error::ChunkLength {
                received: u64::from(token.byte_len),
                expected,
            });
        }
        active.accepted_bytes = active
            .accepted_bytes
            .checked_add(u64::from(token.byte_len))
            .ok_or(Error::Arithmetic)?;
        active.next_chunk = active.next_chunk.checked_add(1).ok_or(Error::Arithmetic)?;
        if active.accepted_bytes > active.plan.object.byte_len {
            return Err(Error::Arithmetic);
        }
        self.active = Some(active);
        Ok(progress(active))
    }

    /// Verifies streamed object/manifest digests and enters atomic-publish pending.
    pub fn finalize(
        &mut self,
        upload_id: UploadId,
        observed_object: ContentId,
        observed_manifest: ContentId,
        context: MutationContext,
    ) -> Result<PublishToken, Error> {
        require_mutation(context)?;
        let mut active = self.active.ok_or(Error::NoUpload)?;
        check_session(active, upload_id)?;
        if active.next_chunk != active.plan.chunk_count
            || active.accepted_bytes != active.plan.object.byte_len
        {
            return Err(Error::Incomplete {
                received: active.next_chunk,
                expected: active.plan.chunk_count,
            });
        }
        if observed_object != active.plan.object.content {
            return Err(Error::ObjectDigest);
        }
        if observed_manifest != active.plan.manifest {
            return Err(Error::ManifestDigest);
        }
        active.phase = UploadPhase::PublishPending;
        self.active = Some(active);
        Ok(publish_token(active))
    }

    /// Clears the upload journal only after atomic backend publication succeeds.
    pub fn record_published(
        &mut self,
        token: PublishToken,
        context: MutationContext,
    ) -> Result<PublishedObject, Error> {
        require_mutation(context)?;
        let active = self.active.ok_or(Error::NoUpload)?;
        if active.phase != UploadPhase::PublishPending {
            return Err(Error::NotReadyToPublish);
        }
        if publish_token(active) != token {
            return Err(Error::PublishTokenMismatch);
        }
        self.active = None;
        Ok(PublishedObject {
            object: token.object,
            manifest: token.manifest,
        })
    }

    /// Idempotently abandons one upload; unreferenced chunks remain GC candidates.
    pub fn abort(
        &mut self,
        upload_id: UploadId,
        context: MutationContext,
    ) -> Result<Option<UploadPlan>, Error> {
        require_mutation(context)?;
        let Some(active) = self.active else {
            return Ok(None);
        };
        check_session(active, upload_id)?;
        self.active = None;
        Ok(Some(active.plan))
    }
}

const fn progress(checkpoint: UploadCheckpoint) -> UploadProgress {
    UploadProgress {
        upload_id: checkpoint.plan.upload_id,
        phase: checkpoint.phase,
        next_chunk: checkpoint.next_chunk,
        accepted_bytes: checkpoint.accepted_bytes,
        total_bytes: checkpoint.plan.object.byte_len,
    }
}

const fn publish_token(checkpoint: UploadCheckpoint) -> PublishToken {
    PublishToken {
        upload_id: checkpoint.plan.upload_id,
        object: checkpoint.plan.object,
        manifest: checkpoint.plan.manifest,
    }
}

fn require_mutation(context: MutationContext) -> Result<(), Error> {
    context.validate()
}

fn check_session(checkpoint: UploadCheckpoint, upload_id: UploadId) -> Result<(), Error> {
    if checkpoint.plan.upload_id == upload_id {
        Ok(())
    } else {
        Err(Error::UploadIdMismatch {
            received: upload_id,
            expected: checkpoint.plan.upload_id,
        })
    }
}

/// Digest field that failed admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DigestRole {
    /// Complete immutable object.
    Object,
    /// Canonical ordered chunk manifest.
    Manifest,
}

/// Fail-closed upload/cache state rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Device storage policy contained a zero bound.
    InvalidLimits,
    /// Upload identity zero is reserved.
    InvalidUploadId,
    /// Required identity is the all-zero sentinel.
    MissingDigest {
        /// Rejected field.
        role: DigestRole,
    },
    /// Empty executable or opaque objects are not stored.
    EmptyObject,
    /// Object exceeds the configured device/cache bound.
    ObjectTooLarge {
        /// Declared bytes.
        received: u64,
        /// Device bound.
        maximum: u64,
    },
    /// Chunk body size is zero or exceeds bounded service RAM.
    ChunkSize {
        /// Declared bytes.
        received: u32,
        /// Device bound.
        maximum: u32,
    },
    /// Chunk count disagrees with object length/fixed chunk size or device limit.
    ChunkCount {
        /// Declared chunks.
        received: u32,
        /// Derived chunks.
        expected: u64,
        /// Device bound.
        maximum: u32,
    },
    /// Upload/publication/repair mutation is forbidden by current RT/safety state.
    MutationForbidden,
    /// Another immutable declaration already owns the one bounded upload slot.
    UploadConflict,
    /// No upload transaction exists.
    NoUpload,
    /// Request referenced another upload identity.
    UploadIdMismatch {
        /// Request identity.
        received: UploadId,
        /// Active identity.
        expected: UploadId,
    },
    /// Upload already awaits atomic publication.
    PublishPending,
    /// Only the first missing sequential chunk is accepted.
    UnexpectedChunk {
        /// Submitted index.
        received: u32,
        /// Required index.
        expected: u32,
    },
    /// Chunk bytes did not match the fixed-size plan (except the final remainder).
    ChunkLength {
        /// Supplied byte length.
        received: u64,
        /// Required byte length.
        expected: u32,
    },
    /// Claimed chunk identity was absent or did not match hashed bytes.
    ChunkDigest {
        /// Rejected chunk index.
        index: u32,
    },
    /// Verified token belongs to another immutable upload declaration.
    VerifiedChunkMismatch,
    /// Finalize was attempted before every chunk was durably journaled.
    Incomplete {
        /// Journaled chunks.
        received: u32,
        /// Declared chunks.
        expected: u32,
    },
    /// Streamed complete object did not match its declared content identity.
    ObjectDigest,
    /// Canonical ordered descriptor stream did not match the declared manifest.
    ManifestDigest,
    /// Restored journal counters/phase did not match the immutable plan.
    CorruptCheckpoint,
    /// Checked counter/size arithmetic overflowed.
    Arithmetic,
    /// Atomic publication was recorded before complete digest verification.
    NotReadyToPublish,
    /// Backend publication acknowledgement did not match the pending object.
    PublishTokenMismatch,
}

/// Fixed storage-operation body decode or length rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireError {
    /// Caller did not provide exactly one fixed body/prefix.
    Length {
        /// Bytes supplied.
        received: usize,
        /// Exact V1 requirement.
        expected: usize,
    },
    /// Digest algorithm byte is unassigned in V1.
    Algorithm {
        /// Unknown value.
        received: u8,
    },
    /// Stored-object kind byte is unassigned in V1.
    ObjectKind {
        /// Unknown value.
        received: u8,
    },
    /// Upload-phase byte is unassigned in V1.
    Phase {
        /// Unknown value.
        received: u8,
    },
    /// Reserved V1 bytes were nonzero.
    Reserved,
    /// Decoded upload plan failed storage admission.
    Plan(Error),
    /// Progress counters/identity were impossible.
    Progress,
    /// Chunk prefix contained zero/invalid identity or length.
    ChunkHeader,
    /// Request used the reserved zero upload identity.
    UploadId,
    /// Variable operation body was not exactly prefix plus declared bytes.
    BodyLength {
        /// Outer operation body length.
        received: u32,
        /// Exact expected length, or `None` after overflow.
        expected: Option<u32>,
    },
}

fn require_wire_len(encoded: &[u8], expected: usize) -> Result<(), WireError> {
    if encoded.len() == expected {
        Ok(())
    } else {
        Err(WireError::Length {
            received: encoded.len(),
            expected,
        })
    }
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
    use super::*;
    use alumina_protocol::{FrameKind, MessageHeader, Operation};

    const LIMITS: CacheLimits = CacheLimits {
        maximum_object_bytes: 1_024,
        maximum_chunk_bytes: 4,
        maximum_chunks: 256,
    };

    fn object() -> StoredObject {
        StoredObject {
            kind: ObjectKind::MachineJobPartition,
            content: sha256(b"abcdefghij"),
            byte_len: 10,
        }
    }

    fn manifest() -> ContentId {
        let mut hasher = ManifestHasher::new(object(), 4, 3, LIMITS).unwrap();
        for (index, bytes) in [(0, &b"abcd"[..]), (1, &b"efgh"[..]), (2, &b"ij"[..])] {
            hasher
                .push(index, sha256(bytes), u32::try_from(bytes.len()).unwrap())
                .unwrap();
        }
        hasher.finalize().unwrap()
    }

    fn plan() -> UploadPlan {
        UploadPlan {
            upload_id: UploadId(7),
            object: object(),
            manifest: manifest(),
            chunk_bytes: 4,
            chunk_count: 3,
        }
    }

    #[test]
    fn sha256_matches_the_nist_abc_vector_and_streaming() {
        let expected = Digest([
            0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae,
            0x22, 0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61,
            0xf2, 0x00, 0x15, 0xad,
        ]);
        assert_eq!(sha256(b"abc"), ContentId::from_sha256(expected));

        let mut streaming = ContentHasher::new();
        streaming.update(b"a");
        streaming.update(b"bc");
        assert_eq!(streaming.finalize(), ContentId::from_sha256(expected));
    }

    #[test]
    fn plan_requires_exact_derived_chunk_count_and_nonzero_identities() {
        assert_eq!(plan().validate(LIMITS), Ok(()));
        let malformed = UploadPlan {
            chunk_count: 2,
            ..plan()
        };
        assert_eq!(
            malformed.validate(LIMITS),
            Err(Error::ChunkCount {
                received: 2,
                expected: 3,
                maximum: 256,
            })
        );
        let missing = UploadPlan {
            manifest: ContentId::from_sha256(Digest::ZERO),
            ..plan()
        };
        assert_eq!(
            missing.validate(LIMITS),
            Err(Error::MissingDigest {
                role: DigestRole::Manifest,
            })
        );
    }

    #[test]
    fn canonical_manifest_requires_order_lengths_and_complete_layout() {
        let mut hasher = ManifestHasher::new(object(), 4, 3, LIMITS).unwrap();
        assert_eq!(
            hasher.push(1, sha256(b"efgh"), 4),
            Err(Error::UnexpectedChunk {
                received: 1,
                expected: 0,
            })
        );
        hasher.push(0, sha256(b"abcd"), 4).unwrap();
        assert_eq!(
            hasher.push(1, sha256(b"efgh"), 3),
            Err(Error::ChunkLength {
                received: 3,
                expected: 4,
            })
        );
        hasher.push(1, sha256(b"efgh"), 4).unwrap();
        assert_eq!(
            hasher.finalize(),
            Err(Error::Incomplete {
                received: 2,
                expected: 3,
            })
        );
        assert_eq!(
            manifest(),
            ContentId::from_sha256(Digest([
                0xb2, 0x1f, 0x48, 0x2b, 0xf0, 0x43, 0x8d, 0x08, 0x3c, 0x17, 0x20, 0xe4, 0x75, 0x32,
                0x3d, 0x23, 0x94, 0x16, 0x46, 0x28, 0xb8, 0x3d, 0xb3, 0x46, 0x50, 0x03, 0x3f, 0x5f,
                0x78, 0xa7, 0xac, 0xaf,
            ]))
        );
    }

    #[test]
    fn storage_operation_bodies_round_trip_and_bind_outer_lengths() {
        let encoded_plan = plan().encode();
        assert_eq!(encoded_plan.len(), UploadPlan::WIRE_LEN);
        assert_eq!(UploadPlan::decode(&encoded_plan, LIMITS), Ok(plan()));
        let begin_message = MessageHeader::request(
            Operation::StorageBeginUpload,
            11,
            u32::try_from(UploadPlan::WIRE_LEN).unwrap(),
        );
        assert_eq!(
            begin_message.validate(
                FrameKind::Storage,
                u32::try_from(MessageHeader::WIRE_LEN + UploadPlan::WIRE_LEN).unwrap(),
            ),
            Ok(())
        );

        let chunk = ChunkUploadHeader {
            upload_id: UploadId(7),
            index: 1,
            byte_len: 4,
            content: sha256(b"efgh"),
        };
        let encoded_chunk = chunk.encode();
        assert_eq!(ChunkUploadHeader::decode(&encoded_chunk), Ok(chunk));
        assert_eq!(chunk.validate_body_len(56), Ok(()));
        assert_eq!(
            chunk.validate_body_len(55),
            Err(WireError::BodyLength {
                received: 55,
                expected: Some(56),
            })
        );

        let progress = UploadProgress {
            upload_id: UploadId(7),
            phase: UploadPhase::Receiving,
            next_chunk: 1,
            accepted_bytes: 4,
            total_bytes: 10,
        };
        assert_eq!(UploadProgress::decode(&progress.encode()), Ok(progress));
        let finalize = FinalizeUploadRequest {
            upload_id: UploadId(7),
        };
        assert_eq!(
            FinalizeUploadRequest::decode(&finalize.encode()),
            Ok(finalize)
        );
    }

    #[test]
    fn storage_wire_decoders_reject_reserved_and_unbounded_plans() {
        let mut encoded = plan().encode();
        encoded[10] = 1;
        assert_eq!(
            UploadPlan::decode(&encoded, LIMITS),
            Err(WireError::Reserved)
        );

        encoded = plan().encode();
        encoded[88..92].copy_from_slice(&5_u32.to_le_bytes());
        assert_eq!(
            UploadPlan::decode(&encoded, LIMITS),
            Err(WireError::Plan(Error::ChunkSize {
                received: 5,
                maximum: 4,
            }))
        );

        let mut chunk = ChunkUploadHeader {
            upload_id: UploadId(7),
            index: 0,
            byte_len: 4,
            content: sha256(b"abcd"),
        }
        .encode();
        chunk[16] = 0xff;
        assert_eq!(
            ChunkUploadHeader::decode(&chunk),
            Err(WireError::Algorithm { received: 0xff })
        );
        assert_eq!(
            FinalizeUploadRequest::decode(&[0; 8]),
            Err(WireError::UploadId)
        );
    }

    #[test]
    fn verified_bytes_advance_only_after_durable_record_acknowledgement() {
        let mut coordinator = UploadCoordinator::new();
        assert_eq!(
            coordinator.begin(plan(), LIMITS, MutationContext::DISARMED_IDLE),
            Ok(UploadProgress {
                upload_id: UploadId(7),
                phase: UploadPhase::Receiving,
                next_chunk: 0,
                accepted_bytes: 0,
                total_bytes: 10,
            })
        );
        let verified = coordinator
            .verify_chunk(UploadId(7), 0, sha256(b"abcd"), b"abcd")
            .unwrap();
        assert_eq!(coordinator.checkpoint().unwrap().next_chunk, 0);
        assert_eq!(
            coordinator.record_chunk(verified, MutationContext::DISARMED_IDLE),
            Ok(UploadProgress {
                upload_id: UploadId(7),
                phase: UploadPhase::Receiving,
                next_chunk: 1,
                accepted_bytes: 4,
                total_bytes: 10,
            })
        );
    }

    #[test]
    fn sequential_chunks_and_final_remainder_are_fail_closed() {
        let mut coordinator = UploadCoordinator::new();
        coordinator
            .begin(plan(), LIMITS, MutationContext::DISARMED_IDLE)
            .unwrap();
        assert_eq!(
            coordinator.verify_chunk(UploadId(7), 1, sha256(b"efgh"), b"efgh"),
            Err(Error::UnexpectedChunk {
                received: 1,
                expected: 0,
            })
        );
        assert_eq!(
            coordinator.verify_chunk(UploadId(7), 0, sha256(b"bad!"), b"abcd"),
            Err(Error::ChunkDigest { index: 0 })
        );

        for (index, bytes) in [(0, &b"abcd"[..]), (1, &b"efgh"[..]), (2, &b"ij"[..])] {
            let token = coordinator
                .verify_chunk(UploadId(7), index, sha256(bytes), bytes)
                .unwrap();
            coordinator
                .record_chunk(token, MutationContext::DISARMED_IDLE)
                .unwrap();
        }
        assert_eq!(
            coordinator.verify_chunk(UploadId(7), 3, sha256(b""), b""),
            Err(Error::UnexpectedChunk {
                received: 3,
                expected: 3,
            })
        );
    }

    #[test]
    fn finalize_restore_and_atomic_publish_are_idempotent() {
        let mut coordinator = UploadCoordinator::new();
        coordinator
            .begin(plan(), LIMITS, MutationContext::DISARMED_IDLE)
            .unwrap();
        for (index, bytes) in [(0, &b"abcd"[..]), (1, &b"efgh"[..]), (2, &b"ij"[..])] {
            let token = coordinator
                .verify_chunk(UploadId(7), index, sha256(bytes), bytes)
                .unwrap();
            coordinator
                .record_chunk(token, MutationContext::DISARMED_IDLE)
                .unwrap();
        }
        assert_eq!(
            coordinator.finalize(
                UploadId(7),
                sha256(b"wrong"),
                plan().manifest,
                MutationContext::DISARMED_IDLE,
            ),
            Err(Error::ObjectDigest)
        );
        let publish = coordinator
            .finalize(
                UploadId(7),
                plan().object.content,
                plan().manifest,
                MutationContext::DISARMED_IDLE,
            )
            .unwrap();
        let checkpoint = coordinator.checkpoint().unwrap();
        assert_eq!(checkpoint.phase, UploadPhase::PublishPending);

        let mut restored = UploadCoordinator::restore(checkpoint, LIMITS).unwrap();
        let retried = restored
            .finalize(
                UploadId(7),
                plan().object.content,
                plan().manifest,
                MutationContext::DISARMED_IDLE,
            )
            .unwrap();
        assert_eq!(retried, publish);
        assert_eq!(
            restored.record_published(retried, MutationContext::DISARMED_IDLE),
            Ok(PublishedObject {
                object: plan().object,
                manifest: plan().manifest,
            })
        );
        assert_eq!(restored.checkpoint(), None);
    }

    #[test]
    fn mutation_is_rejected_while_armed_or_realtime_active() {
        let armed = MutationContext {
            armed_or_energized: true,
            realtime_job_active: false,
        };
        let mut coordinator = UploadCoordinator::new();
        assert_eq!(
            coordinator.begin(plan(), LIMITS, armed),
            Err(Error::MutationForbidden)
        );
        coordinator
            .begin(plan(), LIMITS, MutationContext::DISARMED_IDLE)
            .unwrap();
        let token = coordinator
            .verify_chunk(UploadId(7), 0, sha256(b"abcd"), b"abcd")
            .unwrap();
        assert_eq!(
            coordinator.record_chunk(token, armed),
            Err(Error::MutationForbidden)
        );
        assert_eq!(
            coordinator.abort(UploadId(7), armed),
            Err(Error::MutationForbidden)
        );
    }

    #[test]
    fn checkpoints_and_resume_identity_cannot_silently_diverge() {
        let mut coordinator = UploadCoordinator::new();
        coordinator
            .begin(plan(), LIMITS, MutationContext::DISARMED_IDLE)
            .unwrap();
        assert_eq!(
            coordinator.begin(plan(), LIMITS, MutationContext::DISARMED_IDLE),
            Ok(UploadProgress {
                upload_id: UploadId(7),
                phase: UploadPhase::Receiving,
                next_chunk: 0,
                accepted_bytes: 0,
                total_bytes: 10,
            })
        );
        let other = UploadPlan {
            upload_id: UploadId(8),
            ..plan()
        };
        assert_eq!(
            coordinator.begin(other, LIMITS, MutationContext::DISARMED_IDLE),
            Err(Error::UploadConflict)
        );

        let corrupt = UploadCheckpoint {
            plan: plan(),
            next_chunk: 2,
            accepted_bytes: 7,
            phase: UploadPhase::Receiving,
        };
        assert_eq!(
            UploadCoordinator::restore(corrupt, LIMITS),
            Err(Error::CorruptCheckpoint)
        );
        assert_eq!(
            coordinator.abort(UploadId(7), MutationContext::DISARMED_IDLE),
            Ok(Some(plan()))
        );
        assert_eq!(
            coordinator.abort(UploadId(7), MutationContext::DISARMED_IDLE),
            Ok(None)
        );
    }
}
