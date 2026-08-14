#![no_std]
#![doc = "Bounded service-core request admission shared by ESP firmware and simulation."]

use core::fmt::Write as _;

use alumina_net::MAX_AUTHENTICATED_BODY_BYTES;
use alumina_protocol::{
    DeviceCycle, FrameHeader, FrameKind, MessageDirection, MessageHeader, Operation, StatusCode,
};
use alumina_safety::{
    EffectiveSafety, ObservationError, SafetyContractId, SafetyObservationPolicy, SafetyObserver,
    SafetyState,
};
use alumina_storage::media::{
    AsyncBlockDevice, CacheMedia, MediaAvailability, MediaError, MediaId, MediaRegion, MediaStatus,
};
use alumina_storage::provisioning::{
    CacheProvisionRequest, CacheProvisionRequestError, ProvisionedCache,
    ProvisionedCacheAvailability, ProvisionedCacheError, ProvisionedCacheStatus, ProvisioningFault,
};
use alumina_storage::{
    CacheLimits, ChunkUploadHeader, Error as StorageError, FinalizeUploadRequest, MutationContext,
    PublishedObject, UploadPlan, UploadProgress, sha256,
};
use heapless::String as FixedString;

/// Authenticated bounded reads from one immutable canonical board document.
pub mod capability;
/// Bounded telemetry and waveform session ownership for the service core.
pub mod diagnostics;
/// Passive runtime-health observation and authenticated snapshot service.
pub mod health;

/// Initial bounded protocol/cache policy; free SD capacity remains a runtime limit.
pub const CACHE_LIMITS: CacheLimits = CacheLimits {
    maximum_object_bytes: 64 * 1_024 * 1_024,
    maximum_chunk_bytes: 1_024,
    maximum_chunks: 65_536,
};

/// Largest service response, including a native frame or bounded JSON status.
pub const MAX_SERVICE_RESPONSE_BYTES: usize = 384;
/// Exact native V1 storage-status body length.
pub const STORAGE_BACKEND_STATUS_WIRE_BYTES: usize = 112;

const MAX_NATIVE_PAYLOAD_BYTES: usize = MAX_AUTHENTICATED_BODY_BYTES - FrameHeader::WIRE_LEN;
const INVALID_NATIVE_FRAME: &[u8] = b"{\"error\":\"invalid-native-frame\"}";

/// Request family crossing from the HTTP adapter into the sole service owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceRequestKind {
    /// Bounded human-facing cache/backend status.
    StorageStatus,
    /// One complete authenticated native Alumina frame.
    NativeFrame,
}

/// Fixed-memory request copied into the one-slot core-0 service queue.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ServiceRequest {
    kind: ServiceRequestKind,
    len: u16,
    bytes: [u8; MAX_AUTHENTICATED_BODY_BYTES],
}

impl ServiceRequest {
    /// Creates a bodyless storage-status request.
    pub const fn storage_status() -> Self {
        Self {
            kind: ServiceRequestKind::StorageStatus,
            len: 0,
            bytes: [0; MAX_AUTHENTICATED_BODY_BYTES],
        }
    }

    /// Copies exactly one bounded native request and zero-fills unused capacity.
    pub fn native(bytes: &[u8]) -> Result<Self, ServiceAdmissionError> {
        let len = u16::try_from(bytes.len()).map_err(|_| ServiceAdmissionError::RequestTooLarge)?;
        if bytes.is_empty() || bytes.len() > MAX_AUTHENTICATED_BODY_BYTES {
            return Err(ServiceAdmissionError::RequestTooLarge);
        }
        let mut owned = [0_u8; MAX_AUTHENTICATED_BODY_BYTES];
        owned[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            kind: ServiceRequestKind::NativeFrame,
            len,
            bytes: owned,
        })
    }

    /// Request family without exposing unused buffer bytes.
    pub const fn kind(&self) -> ServiceRequestKind {
        self.kind
    }

    /// Exact initialized request bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

impl core::fmt::Debug for ServiceRequest {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ServiceRequest")
            .field("kind", &self.kind)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

/// Bounded media types emitted by service admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseMedia {
    /// Small discovery/status or rejection document.
    Json,
    /// Exact protocol V1 frame.
    NativeFrame,
}

/// Fixed-memory response returned to the authenticated HTTP handler.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ServiceResponse {
    /// HTTP transport status; native application errors normally remain HTTP 200.
    pub http_status: u16,
    /// Response representation.
    pub media: ResponseMedia,
    len: u16,
    bytes: [u8; MAX_SERVICE_RESPONSE_BYTES],
}

impl ServiceResponse {
    fn from_bytes(http_status: u16, media: ResponseMedia, bytes: &[u8]) -> Self {
        assert!(bytes.len() <= MAX_SERVICE_RESPONSE_BYTES);
        let mut owned = [0_u8; MAX_SERVICE_RESPONSE_BYTES];
        owned[..bytes.len()].copy_from_slice(bytes);
        Self {
            http_status,
            media,
            len: u16::try_from(bytes.len()).expect("service response bound fits u16"),
            bytes: owned,
        }
    }

    /// Exact initialized response bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }

    /// Builds a bounded native response for one already decoded request.
    pub fn native(
        request: NativeRequest<'_>,
        now: DeviceCycle,
        status: StatusCode,
        body: &[u8],
    ) -> Result<Self, NativeResponseError> {
        native_response(request.frame, request.message, now, status, body)
    }

    /// Stable malformed-frame response shared by all native service families.
    pub fn invalid_native() -> Self {
        invalid_native_frame()
    }
}

impl core::fmt::Debug for ServiceResponse {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ServiceResponse")
            .field("http_status", &self.http_status)
            .field("media", &self.media)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

/// HTTP-to-service queue admission error before native decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceAdmissionError {
    /// Body was empty or exceeded the reviewed fixed request buffer.
    RequestTooLarge,
}

/// One exact request frame split into universal, operation, and typed body parts.
#[derive(Clone, Copy, Debug)]
pub struct NativeRequest<'a> {
    /// Validated universal frame header.
    pub frame: FrameHeader,
    /// Validated operation header bound to `frame.kind`.
    pub message: MessageHeader,
    /// Exact operation-specific bytes.
    pub body: &'a [u8],
}

impl<'a> NativeRequest<'a> {
    /// Decodes one complete bounded request without interpreting its body.
    pub fn decode(bytes: &'a [u8]) -> Result<Self, NativeRequestError> {
        let frame_bytes = bytes
            .get(..FrameHeader::WIRE_LEN)
            .ok_or(NativeRequestError::Frame)?;
        let frame = FrameHeader::decode(
            frame_bytes,
            u32::try_from(MAX_NATIVE_PAYLOAD_BYTES).expect("native payload bound fits u32"),
        )
        .map_err(|_| NativeRequestError::Frame)?;
        let payload_len =
            usize::try_from(frame.payload_len).map_err(|_| NativeRequestError::Frame)?;
        if FrameHeader::WIRE_LEN.checked_add(payload_len) != Some(bytes.len()) {
            return Err(NativeRequestError::Frame);
        }
        let message_start = FrameHeader::WIRE_LEN;
        let message_end = message_start + MessageHeader::WIRE_LEN;
        let message_bytes = bytes
            .get(message_start..message_end)
            .ok_or(NativeRequestError::Message)?;
        let message =
            MessageHeader::decode_and_validate(message_bytes, frame.kind, frame.payload_len)
                .map_err(|_| NativeRequestError::Message)?;
        if message.direction != MessageDirection::Request {
            return Err(NativeRequestError::Direction);
        }
        Ok(Self {
            frame,
            message,
            body: &bytes[message_end..],
        })
    }
}

/// Universal native request rejection before family-specific body decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeRequestError {
    /// Universal frame prefix, length, or outer framing was invalid.
    Frame,
    /// Operation prefix or family binding was invalid.
    Message,
    /// Only request-direction messages enter service dispatch.
    Direction,
}

/// A native response exceeded the reviewed fixed response buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeResponseError {
    /// Frame, operation prefix, and typed body did not fit.
    TooLarge,
}

/// Stable backend availability returned by both JSON and native status routes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StorageBackendAvailability {
    /// No card/adapter is present for this board composition.
    Unavailable = 0,
    /// Adapter exists but no valid cache region is mounted.
    Detached = 1,
    /// Complete committed media state was replayed successfully.
    Ready = 2,
    /// Identification, write, sync, or replay failed and requires recovery.
    Faulted = 3,
}

impl StorageBackendAvailability {
    const fn label(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Detached => "detached",
            Self::Ready => "ready",
            Self::Faulted => "faulted",
        }
    }
}

/// Allocation-free status shared by real, simulated, and unavailable backends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageBackendStatus {
    /// Mount/backend state.
    pub availability: StorageBackendAvailability,
    /// Service and safety state currently admit a durable mutation.
    pub mutation_available: bool,
    /// Safety and backend state currently admit explicit destructive provisioning.
    pub provision_available: bool,
    /// Coarse reason the backend is faulted.
    pub fault: ProvisioningFault,
    /// Complete physical device capacity, independent of selected region.
    pub device_blocks: u64,
    /// Monotonic trusted locator generation, zero when no locator is trusted.
    pub locator_generation: u64,
    /// Exact raw region start when a locator/configuration is present.
    pub region_start_block: Option<u64>,
    /// Stable media identity when a locator or mounted anchor is present.
    pub media_id: Option<MediaId>,
    /// Total blocks in the explicit cache region.
    pub total_blocks: u64,
    /// Append capacity remaining before compaction.
    pub free_blocks: u64,
    /// Last committed media record sequence.
    pub last_sequence: u64,
    /// Publication records replayed from the current media generation.
    pub published_objects: u32,
    /// Active resumable upload progress.
    pub upload: Option<UploadProgress>,
    /// One anchor was torn and the prior complete anchor was selected.
    pub degraded_anchor: bool,
    /// One expected fixed provisioning locator is absent or corrupt.
    pub degraded_locator: bool,
}

impl StorageBackendStatus {
    /// Exact fixed native representation, with zero-filled absent upload fields.
    pub fn encode(self) -> [u8; STORAGE_BACKEND_STATUS_WIRE_BYTES] {
        let mut encoded = [0_u8; STORAGE_BACKEND_STATUS_WIRE_BYTES];
        encoded[0] = self.availability as u8;
        encoded[1] = u8::from(self.mutation_available);
        encoded[2] = u8::from(self.degraded_anchor);
        encoded[3] = u8::from(self.upload.is_some());
        encoded[4] = u8::from(self.degraded_locator);
        encoded[5] = u8::from(self.provision_available);
        encoded[6] = self.fault as u8;
        encoded[7] =
            u8::from(self.region_start_block.is_some()) | (u8::from(self.media_id.is_some()) << 1);
        encoded[8..16].copy_from_slice(&self.device_blocks.to_le_bytes());
        if let Some(region_start) = self.region_start_block {
            encoded[16..24].copy_from_slice(&region_start.to_le_bytes());
        }
        encoded[24..32].copy_from_slice(&self.total_blocks.to_le_bytes());
        encoded[32..40].copy_from_slice(&self.free_blocks.to_le_bytes());
        encoded[40..48].copy_from_slice(&self.last_sequence.to_le_bytes());
        encoded[48..56].copy_from_slice(&self.locator_generation.to_le_bytes());
        encoded[56..60].copy_from_slice(&self.published_objects.to_le_bytes());
        if let Some(media_id) = self.media_id {
            encoded[64..80].copy_from_slice(&media_id.0);
        }
        if let Some(progress) = self.upload {
            encoded[80..112].copy_from_slice(&progress.encode());
        }
        encoded
    }
}

/// Async durable operations owned by the sole core-0 service task.
#[allow(
    async_fn_in_trait,
    reason = "backends execute on one no_std Embassy core and intentionally do not require Send"
)]
pub trait StorageBackend {
    /// Current media facts without performing I/O.
    fn status(&self) -> StorageBackendStatus;

    /// Explicitly formats and persists one exact cache region.
    async fn provision(
        &mut self,
        _request: CacheProvisionRequest,
        _context: MutationContext,
    ) -> Result<(), StatusCode> {
        Err(StatusCode::Unsupported)
    }

    /// Revalidates and returns one exact typed publication for retry/cache reconciliation.
    async fn inspect_published(
        &mut self,
        _expected: PublishedObject,
    ) -> Result<PublishedObject, StatusCode> {
        Err(StatusCode::Unsupported)
    }

    /// Begins or idempotently resumes one durable declaration.
    async fn begin_upload(
        &mut self,
        plan: UploadPlan,
        context: MutationContext,
    ) -> Result<UploadProgress, StatusCode>;

    /// Durably commits exactly the next independently hashed chunk.
    async fn put_chunk(
        &mut self,
        header: ChunkUploadHeader,
        bytes: &[u8],
        context: MutationContext,
    ) -> Result<UploadProgress, StatusCode>;

    /// Verifies aggregate identities and atomically publishes one object.
    async fn finalize_upload(
        &mut self,
        request: FinalizeUploadRequest,
        context: MutationContext,
    ) -> Result<PublishedObject, StatusCode>;
}

/// Explicit backend for board images whose physical SD adapter is not present.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableStorageBackend;

impl StorageBackend for UnavailableStorageBackend {
    fn status(&self) -> StorageBackendStatus {
        StorageBackendStatus {
            availability: StorageBackendAvailability::Unavailable,
            mutation_available: false,
            provision_available: false,
            fault: ProvisioningFault::None,
            device_blocks: 0,
            locator_generation: 0,
            region_start_block: None,
            media_id: None,
            total_blocks: 0,
            free_blocks: 0,
            last_sequence: 0,
            published_objects: 0,
            upload: None,
            degraded_anchor: false,
            degraded_locator: false,
        }
    }

    async fn begin_upload(
        &mut self,
        _plan: UploadPlan,
        _context: MutationContext,
    ) -> Result<UploadProgress, StatusCode> {
        Err(StatusCode::Unsupported)
    }

    async fn put_chunk(
        &mut self,
        _header: ChunkUploadHeader,
        _bytes: &[u8],
        _context: MutationContext,
    ) -> Result<UploadProgress, StatusCode> {
        Err(StatusCode::Unsupported)
    }

    async fn finalize_upload(
        &mut self,
        _request: FinalizeUploadRequest,
        _context: MutationContext,
    ) -> Result<PublishedObject, StatusCode> {
        Err(StatusCode::Unsupported)
    }
}

impl<D> StorageBackend for CacheMedia<D>
where
    D: AsyncBlockDevice,
{
    fn status(&self) -> StorageBackendStatus {
        media_status(
            self.status(),
            self.device_block_count(),
            self.region(),
            0,
            false,
        )
    }

    async fn inspect_published(
        &mut self,
        expected: PublishedObject,
    ) -> Result<PublishedObject, StatusCode> {
        CacheMedia::open_published(self, expected)
            .await
            .map(|reader| reader.published())
            .map_err(media_error_status)
    }

    async fn begin_upload(
        &mut self,
        plan: UploadPlan,
        context: MutationContext,
    ) -> Result<UploadProgress, StatusCode> {
        CacheMedia::begin_upload(self, plan, context)
            .await
            .map_err(media_error_status)
    }

    async fn put_chunk(
        &mut self,
        header: ChunkUploadHeader,
        bytes: &[u8],
        context: MutationContext,
    ) -> Result<UploadProgress, StatusCode> {
        CacheMedia::put_chunk(self, header, bytes, context)
            .await
            .map_err(media_error_status)
    }

    async fn finalize_upload(
        &mut self,
        request: FinalizeUploadRequest,
        context: MutationContext,
    ) -> Result<PublishedObject, StatusCode> {
        CacheMedia::finalize_upload(self, request, context)
            .await
            .map_err(media_error_status)
    }
}

impl<D> StorageBackend for ProvisionedCache<D>
where
    D: AsyncBlockDevice,
{
    fn status(&self) -> StorageBackendStatus {
        provisioned_cache_status(self.status())
    }

    async fn provision(
        &mut self,
        request: CacheProvisionRequest,
        context: MutationContext,
    ) -> Result<(), StatusCode> {
        ProvisionedCache::provision(self, request, context)
            .await
            .map(|_| ())
            .map_err(provisioned_cache_error_status)
    }

    async fn inspect_published(
        &mut self,
        expected: PublishedObject,
    ) -> Result<PublishedObject, StatusCode> {
        ProvisionedCache::open_published(self, expected)
            .await
            .map(|reader| reader.published())
            .map_err(provisioned_cache_error_status)
    }

    async fn begin_upload(
        &mut self,
        plan: UploadPlan,
        context: MutationContext,
    ) -> Result<UploadProgress, StatusCode> {
        ProvisionedCache::begin_upload(self, plan, context)
            .await
            .map_err(provisioned_cache_error_status)
    }

    async fn put_chunk(
        &mut self,
        header: ChunkUploadHeader,
        bytes: &[u8],
        context: MutationContext,
    ) -> Result<UploadProgress, StatusCode> {
        ProvisionedCache::put_chunk(self, header, bytes, context)
            .await
            .map_err(provisioned_cache_error_status)
    }

    async fn finalize_upload(
        &mut self,
        request: FinalizeUploadRequest,
        context: MutationContext,
    ) -> Result<PublishedObject, StatusCode> {
        ProvisionedCache::finalize_upload(self, request, context)
            .await
            .map_err(provisioned_cache_error_status)
    }
}

fn media_status(
    status: MediaStatus,
    device_blocks: u64,
    region: MediaRegion,
    locator_generation: u64,
    degraded_locator: bool,
) -> StorageBackendStatus {
    StorageBackendStatus {
        availability: match status.availability {
            MediaAvailability::Detached => StorageBackendAvailability::Detached,
            MediaAvailability::Ready => StorageBackendAvailability::Ready,
            MediaAvailability::Faulted => StorageBackendAvailability::Faulted,
        },
        mutation_available: false,
        provision_available: true,
        fault: if status.availability == MediaAvailability::Faulted {
            ProvisioningFault::Device
        } else {
            ProvisioningFault::None
        },
        device_blocks,
        locator_generation,
        region_start_block: Some(region.start_block),
        media_id: status.media_id,
        total_blocks: status.total_blocks,
        free_blocks: status.free_blocks,
        last_sequence: status.last_sequence,
        published_objects: status.published_objects,
        upload: status.upload,
        degraded_anchor: status.degraded_anchor,
        degraded_locator,
    }
}

fn provisioned_cache_status(status: ProvisionedCacheStatus) -> StorageBackendStatus {
    let region = status.region;
    let media = status.media;
    StorageBackendStatus {
        availability: match status.availability {
            ProvisionedCacheAvailability::Detached => StorageBackendAvailability::Detached,
            ProvisionedCacheAvailability::Ready => StorageBackendAvailability::Ready,
            ProvisionedCacheAvailability::Faulted => StorageBackendAvailability::Faulted,
        },
        mutation_available: false,
        provision_available: status.fault != ProvisioningFault::Device
            && status.fault != ProvisioningFault::Geometry
            && status.fault != ProvisioningFault::Policy,
        fault: status.fault,
        device_blocks: status.device_blocks,
        locator_generation: status.locator_generation,
        region_start_block: region.map(|region| region.start_block),
        media_id: status.media_id,
        total_blocks: region.map_or(0, |region| region.block_count),
        free_blocks: media.map_or(0, |media| media.free_blocks),
        last_sequence: media.map_or(0, |media| media.last_sequence),
        published_objects: media.map_or(0, |media| media.published_objects),
        upload: media.and_then(|media| media.upload),
        degraded_anchor: media.is_some_and(|media| media.degraded_anchor),
        degraded_locator: status.degraded_locator,
    }
}

fn provisioned_cache_error_status<E>(error: ProvisionedCacheError<E>) -> StatusCode {
    match error {
        ProvisionedCacheError::Device(_) | ProvisionedCacheError::DeviceStateUnavailable => {
            StatusCode::Internal
        }
        ProvisionedCacheError::Locator(_) | ProvisionedCacheError::MediaIdentity => {
            StatusCode::Integrity
        }
        ProvisionedCacheError::Geometry => StatusCode::InvalidRequest,
        ProvisionedCacheError::Media(error) => media_error_status(error),
        ProvisionedCacheError::NotDiscovered | ProvisionedCacheError::NotMounted => {
            StatusCode::Unsupported
        }
        ProvisionedCacheError::Conflict | ProvisionedCacheError::RecoveryIntent => {
            StatusCode::Conflict
        }
        ProvisionedCacheError::Mutation(error) => storage_error_status(error),
        ProvisionedCacheError::Arithmetic => StatusCode::Capacity,
    }
}

fn media_error_status<E>(error: MediaError<E>) -> StatusCode {
    match error {
        MediaError::Unformatted | MediaError::NotMounted => StatusCode::Unsupported,
        MediaError::Faulted | MediaError::Device(_) => StatusCode::Internal,
        MediaError::Corrupt(_) => StatusCode::Integrity,
        MediaError::PublishedNotFound => StatusCode::NotFound,
        MediaError::Full { .. } | MediaError::Arithmetic => StatusCode::Capacity,
        MediaError::Geometry(_) => StatusCode::InvalidRequest,
        MediaError::Storage(error) => storage_error_status(error),
    }
}

const fn storage_error_status(error: StorageError) -> StatusCode {
    match error {
        StorageError::MutationForbidden => StatusCode::ForbiddenState,
        StorageError::MissingDigest { .. }
        | StorageError::ChunkDigest { .. }
        | StorageError::ObjectDigest
        | StorageError::ManifestDigest => StatusCode::Integrity,
        StorageError::UploadConflict
        | StorageError::UploadIdMismatch { .. }
        | StorageError::UnexpectedChunk { .. }
        | StorageError::VerifiedChunkMismatch
        | StorageError::PublishTokenMismatch
        | StorageError::ConfigurationTransition
        | StorageError::GraphTransition => StatusCode::Conflict,
        StorageError::PublishPending | StorageError::NotReadyToPublish => StatusCode::Busy,
        StorageError::ObjectTooLarge { .. } | StorageError::ChunkCount { .. } => {
            StatusCode::Capacity
        }
        StorageError::NoUpload => StatusCode::NotFound,
        StorageError::InvalidLimits
        | StorageError::InvalidUploadId
        | StorageError::EmptyObject
        | StorageError::ChunkSize { .. }
        | StorageError::ChunkLength { .. }
        | StorageError::Incomplete { .. }
        | StorageError::CorruptCheckpoint
        | StorageError::Arithmetic => StatusCode::InvalidRequest,
    }
}

/// Storage admission/safety state intentionally owned by the service-core task.
pub struct StorageServiceState {
    safety: SafetyObserver,
    service_job_active: bool,
    configuration_active: bool,
    configuration_transaction_active: bool,
}

impl StorageServiceState {
    /// Starts in boot state, where every mutation is forbidden.
    pub const fn new(
        safe_output_contract: SafetyContractId,
        maximum_safety_age_cycles: u64,
    ) -> Self {
        Self {
            safety: SafetyObserver::new(SafetyObservationPolicy {
                expected_contract: safe_output_contract,
                maximum_age_cycles: maximum_safety_age_cycles,
            }),
            service_job_active: false,
            configuration_active: false,
            configuration_transaction_active: false,
        }
    }

    /// Applies the immediate core-0 cache-ownership veto before RT telemetry catches up.
    pub fn set_service_job_active(&mut self, active: bool) {
        self.service_job_active = active;
    }

    /// Prevents destructive cache reprovisioning while a committed machine
    /// configuration still depends on this exact media log.
    pub fn set_configuration_active(&mut self, active: bool) {
        self.configuration_active = active;
    }

    /// Serializes external storage mutations against the configuration reader
    /// and selector without preventing that coordinator's own journal writes.
    pub fn set_configuration_transaction_active(&mut self, active: bool) {
        self.configuration_transaction_active = active;
    }

    /// Dispatches one already authenticated request on the sole service owner.
    pub async fn dispatch<B: StorageBackend>(
        &mut self,
        backend: &mut B,
        request: &ServiceRequest,
        now: DeviceCycle,
    ) -> ServiceResponse {
        match request.kind() {
            ServiceRequestKind::StorageStatus => self.human_status(backend, now),
            ServiceRequestKind::NativeFrame => {
                self.dispatch_native(backend, request.bytes(), now).await
            }
        }
    }

    /// Validates one complete core-1 safety payload in the shared cycle domain.
    ///
    /// Any rejection revokes the previously accepted state, so callers cannot
    /// accidentally continue mutating storage after malformed telemetry.
    pub fn observe_safety_snapshot(
        &mut self,
        frame_sequence: u32,
        produced_at: DeviceCycle,
        observed_at: DeviceCycle,
        encoded: &[u8],
    ) -> Result<(), ObservationError> {
        self.safety
            .observe_encoded(frame_sequence, produced_at.0, observed_at.0, encoded)
    }

    /// Revokes service-side safety authority after the independent fault path fires.
    pub fn invalidate_safety_observation(&mut self) {
        self.safety.invalidate();
    }

    /// Latest freshness-checked core-1 safety facts for other core-0 services.
    pub fn effective_safety(&self, now: DeviceCycle) -> EffectiveSafety {
        self.safety.effective(now.0)
    }

    async fn dispatch_native<B: StorageBackend>(
        &mut self,
        backend: &mut B,
        bytes: &[u8],
        now: DeviceCycle,
    ) -> ServiceResponse {
        let Ok(request) = NativeRequest::decode(bytes) else {
            return invalid_native_frame();
        };
        let frame = request.frame;
        let message = request.message;
        let body = request.body;
        if frame.kind != FrameKind::Storage {
            return invalid_native_frame();
        }
        let mut response_body = [0_u8; STORAGE_BACKEND_STATUS_WIRE_BYTES];
        let (status, response_len) = match message.operation {
            Operation::StorageStatus if body.is_empty() => {
                let status = self.effective_status(backend, now);
                response_body.copy_from_slice(&status.encode());
                (StatusCode::Ok, STORAGE_BACKEND_STATUS_WIRE_BYTES)
            }
            Operation::StorageProvision if self.configuration_active => {
                (StatusCode::ForbiddenState, 0)
            }
            Operation::StorageProvision => match CacheProvisionRequest::decode(body) {
                Ok(request) => match backend.provision(request, self.mutation_context(now)).await {
                    Ok(()) => {
                        let status = self.effective_status(backend, now);
                        response_body.copy_from_slice(&status.encode());
                        (StatusCode::Ok, STORAGE_BACKEND_STATUS_WIRE_BYTES)
                    }
                    Err(status) => (status, 0),
                },
                Err(CacheProvisionRequestError::Confirmation) => (StatusCode::Integrity, 0),
                Err(_) => (StatusCode::InvalidRequest, 0),
            },
            Operation::StorageInspect => match PublishedObject::decode(body, CACHE_LIMITS) {
                Ok(expected) => match backend.inspect_published(expected).await {
                    Ok(publication) if publication == expected => {
                        response_body[..PublishedObject::WIRE_LEN]
                            .copy_from_slice(&publication.encode());
                        (StatusCode::Ok, PublishedObject::WIRE_LEN)
                    }
                    Ok(_) => (StatusCode::Integrity, 0),
                    Err(status) => (status, 0),
                },
                Err(_) => (StatusCode::InvalidRequest, 0),
            },
            Operation::StorageBeginUpload => match UploadPlan::decode(body, CACHE_LIMITS) {
                Ok(plan) => match backend.begin_upload(plan, self.mutation_context(now)).await {
                    Ok(progress) => {
                        response_body[..UploadProgress::WIRE_LEN]
                            .copy_from_slice(&progress.encode());
                        (StatusCode::Ok, UploadProgress::WIRE_LEN)
                    }
                    Err(status) => (status, 0),
                },
                Err(_) => (StatusCode::InvalidRequest, 0),
            },
            Operation::StoragePutChunk => match validate_chunk_body(message.body_len, body) {
                Ok((header, chunk)) => {
                    match backend
                        .put_chunk(header, chunk, self.mutation_context(now))
                        .await
                    {
                        Ok(progress) => {
                            response_body[..UploadProgress::WIRE_LEN]
                                .copy_from_slice(&progress.encode());
                            (StatusCode::Ok, UploadProgress::WIRE_LEN)
                        }
                        Err(status) => (status, 0),
                    }
                }
                Err(StorageBodyError::Malformed) => (StatusCode::InvalidRequest, 0),
                Err(StorageBodyError::Integrity) => (StatusCode::Integrity, 0),
            },
            Operation::StorageFinalize => match FinalizeUploadRequest::decode(body) {
                Ok(request) => match backend
                    .finalize_upload(request, self.mutation_context(now))
                    .await
                {
                    Ok(_) => (StatusCode::Ok, 0),
                    Err(status) => (status, 0),
                },
                Err(_) => (StatusCode::InvalidRequest, 0),
            },
            _ => (StatusCode::Unsupported, 0),
        };
        native_response(frame, message, now, status, &response_body[..response_len])
            .expect("storage response is bounded by the fixed response body")
    }

    fn human_status<B: StorageBackend>(&self, backend: &B, now: DeviceCycle) -> ServiceResponse {
        let status = self.effective_status(backend, now);
        let mut json = FixedString::<MAX_SERVICE_RESPONSE_BYTES>::new();
        write!(
            &mut json,
            "{{\"backend\":\"{}\",\"fault\":\"{}\",\"mutation_available\":{},\"provision_available\":{},\"device_blocks\":{},\"region_start\":",
            status.availability.label(),
            status.fault.label(),
            status.mutation_available,
            status.provision_available,
            status.device_blocks,
        )
        .expect("bounded storage status prefix fits response");
        if let Some(region_start) = status.region_start_block {
            write!(&mut json, "{region_start}").expect("bounded region start fits response");
        } else {
            json.push_str("null")
                .expect("bounded absent region fits response");
        }
        write!(
            &mut json,
            ",\"region_blocks\":{},\"free_blocks\":{},\"locator_generation\":{},\"published_objects\":{},\"upload_next_chunk\":",
            status.total_blocks,
            status.free_blocks,
            status.locator_generation,
            status.published_objects,
        )
        .expect("bounded storage status facts fit response");
        if let Some(progress) = status.upload {
            write!(&mut json, "{}", progress.next_chunk)
                .expect("bounded upload counter fits response");
        } else {
            json.push_str("null")
                .expect("bounded null status fits response");
        }
        json.push('}')
            .expect("bounded storage status terminator fits response");
        ServiceResponse::from_bytes(200, ResponseMedia::Json, json.as_bytes())
    }

    fn effective_status<B: StorageBackend>(
        &self,
        backend: &B,
        now: DeviceCycle,
    ) -> StorageBackendStatus {
        let mut status = backend.status();
        status.mutation_available = status.availability == StorageBackendAvailability::Ready
            && self.mutation_context(now).validate().is_ok();
        status.provision_available = status.provision_available
            && !self.configuration_active
            && self.mutation_context(now).validate().is_ok();
        status
    }

    /// Current fail-closed durable-mutation context shared with configuration
    /// and future update coordinators on the same service task.
    pub fn mutation_context(&self, now: DeviceCycle) -> MutationContext {
        let mut context = self.configuration_mutation_context(now);
        context.realtime_job_active |= self.configuration_transaction_active;
        context
    }

    /// Context for the sole configuration coordinator. It includes safety and
    /// job ownership but deliberately excludes the coordinator's own storage
    /// serialization flag.
    pub fn configuration_mutation_context(&self, now: DeviceCycle) -> MutationContext {
        let safety = self.safety.effective(now.0);
        let safe_for_mutation = matches!(safety.state, SafetyState::Safe | SafetyState::Configured);
        MutationContext {
            armed_or_energized: !safe_for_mutation,
            realtime_job_active: safety.realtime_job_active || self.service_job_active,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StorageBodyError {
    Malformed,
    Integrity,
}

fn validate_chunk_body(
    body_len: u32,
    body: &[u8],
) -> Result<(ChunkUploadHeader, &[u8]), StorageBodyError> {
    let prefix = body
        .get(..ChunkUploadHeader::WIRE_LEN)
        .ok_or(StorageBodyError::Malformed)?;
    let header = ChunkUploadHeader::decode(prefix).map_err(|_| StorageBodyError::Malformed)?;
    header
        .validate_body_len(body_len)
        .map_err(|_| StorageBodyError::Malformed)?;
    if header.byte_len > CACHE_LIMITS.maximum_chunk_bytes {
        return Err(StorageBodyError::Malformed);
    }
    let chunk = &body[ChunkUploadHeader::WIRE_LEN..];
    if sha256(chunk) != header.content {
        return Err(StorageBodyError::Integrity);
    }
    Ok((header, chunk))
}

fn invalid_native_frame() -> ServiceResponse {
    ServiceResponse::from_bytes(400, ResponseMedia::Json, INVALID_NATIVE_FRAME)
}

fn native_response(
    request_frame: FrameHeader,
    request_message: MessageHeader,
    now: DeviceCycle,
    status: StatusCode,
    body: &[u8],
) -> Result<ServiceResponse, NativeResponseError> {
    let response_len = FrameHeader::WIRE_LEN
        .checked_add(MessageHeader::WIRE_LEN)
        .and_then(|prefix| prefix.checked_add(body.len()))
        .ok_or(NativeResponseError::TooLarge)?;
    if response_len > MAX_SERVICE_RESPONSE_BYTES {
        return Err(NativeResponseError::TooLarge);
    }
    let payload_len = u32::try_from(MessageHeader::WIRE_LEN + body.len())
        .map_err(|_| NativeResponseError::TooLarge)?;
    let frame = FrameHeader::new(
        request_frame.kind,
        payload_len,
        request_frame.sequence,
        now,
        request_frame.config_digest,
    );
    let message = MessageHeader::response(
        request_message.operation,
        request_message.correlation_id,
        status,
        u32::try_from(body.len()).map_err(|_| NativeResponseError::TooLarge)?,
    );
    let mut bytes = [0_u8; MAX_SERVICE_RESPONSE_BYTES];
    bytes[..FrameHeader::WIRE_LEN].copy_from_slice(&frame.encode());
    bytes[FrameHeader::WIRE_LEN..FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN]
        .copy_from_slice(&message.encode());
    bytes[FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN..response_len].copy_from_slice(body);
    Ok(ServiceResponse::from_bytes(
        200,
        ResponseMedia::NativeFrame,
        &bytes[..response_len],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alumina_protocol::{Digest, MessageDirection};
    use alumina_safety::{
        SNAPSHOT_FLAG_REALTIME_JOB_ACTIVE, SNAPSHOT_FLAG_SAFE_OUTPUTS_ESTABLISHED, SafetySnapshot,
    };
    use alumina_storage::{
        ContentId, DigestAlgorithm, ObjectKind, StoredObject, UploadId, UploadPhase,
    };
    use embassy_futures::block_on;

    const TEST_CONTRACT: SafetyContractId = SafetyContractId(*b"service-safe-v01");

    fn service() -> StorageServiceState {
        StorageServiceState::new(TEST_CONTRACT, 100)
    }

    fn observe(
        service: &mut StorageServiceState,
        state: SafetyState,
        frame_sequence: u32,
        at: u64,
        realtime_job_active: bool,
    ) {
        let mut flags = SNAPSHOT_FLAG_SAFE_OUTPUTS_ESTABLISHED;
        if realtime_job_active {
            flags |= SNAPSHOT_FLAG_REALTIME_JOB_ACTIVE;
        }
        let snapshot = SafetySnapshot {
            state,
            fault: None,
            flags,
            transition_generation: frame_sequence,
            safe_output_contract: TEST_CONTRACT,
            maximum_lateness_cycles: 0,
            safety_inputs: alumina_safety::SafetyInputStatus::unconfigured(),
        };
        service
            .observe_safety_snapshot(
                frame_sequence,
                DeviceCycle(at),
                DeviceCycle(at),
                &snapshot.encode().unwrap(),
            )
            .unwrap();
    }

    fn plan() -> UploadPlan {
        UploadPlan {
            upload_id: UploadId(7),
            object: StoredObject {
                kind: ObjectKind::OpaqueData,
                content: sha256(b"abcd"),
                byte_len: 4,
            },
            manifest: ContentId {
                algorithm: DigestAlgorithm::Sha256,
                digest: Digest([9; 32]),
            },
            chunk_bytes: 4,
            chunk_count: 1,
        }
    }

    fn request(operation: Operation, body: &[u8]) -> ServiceRequest {
        let payload_len = MessageHeader::WIRE_LEN + body.len();
        let frame = FrameHeader::new(
            operation.frame_kind(),
            u32::try_from(payload_len).unwrap(),
            3,
            DeviceCycle(5),
            Digest::ZERO,
        );
        let message = MessageHeader::request(operation, 11, u32::try_from(body.len()).unwrap());
        let mut bytes = [0_u8; MAX_AUTHENTICATED_BODY_BYTES];
        bytes[..FrameHeader::WIRE_LEN].copy_from_slice(&frame.encode());
        bytes[FrameHeader::WIRE_LEN..FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN]
            .copy_from_slice(&message.encode());
        bytes[FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN..FrameHeader::WIRE_LEN + payload_len]
            .copy_from_slice(body);
        ServiceRequest::native(&bytes[..FrameHeader::WIRE_LEN + payload_len]).unwrap()
    }

    fn response_parts(response: &ServiceResponse) -> (StatusCode, &[u8]) {
        assert_eq!(response.media, ResponseMedia::NativeFrame);
        let frame = FrameHeader::decode(
            &response.bytes()[..FrameHeader::WIRE_LEN],
            u32::try_from(MAX_NATIVE_PAYLOAD_BYTES).unwrap(),
        )
        .unwrap();
        let message = MessageHeader::decode_and_validate(
            &response.bytes()
                [FrameHeader::WIRE_LEN..FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN],
            frame.kind,
            frame.payload_len,
        )
        .unwrap();
        assert_eq!(message.direction, MessageDirection::Response);
        let body = &response.bytes()
            [FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN..response.bytes().len()];
        assert_eq!(usize::try_from(message.body_len).unwrap(), body.len());
        (message.status, body)
    }

    fn response_status(response: &ServiceResponse) -> StatusCode {
        response_parts(response).0
    }

    struct DetachedBackend(u8);

    impl DetachedBackend {
        const fn into_device(self) -> u8 {
            self.0
        }
    }

    impl StorageBackend for DetachedBackend {
        fn status(&self) -> StorageBackendStatus {
            StorageBackendStatus {
                availability: StorageBackendAvailability::Detached,
                mutation_available: false,
                provision_available: true,
                fault: ProvisioningFault::None,
                device_blocks: 4_096,
                locator_generation: 0,
                region_start_block: None,
                media_id: None,
                total_blocks: 0,
                free_blocks: 0,
                last_sequence: 0,
                published_objects: 0,
                upload: None,
                degraded_anchor: false,
                degraded_locator: false,
            }
        }

        async fn begin_upload(
            &mut self,
            _plan: UploadPlan,
            _context: MutationContext,
        ) -> Result<UploadProgress, StatusCode> {
            Err(StatusCode::Unsupported)
        }

        async fn put_chunk(
            &mut self,
            _header: ChunkUploadHeader,
            _bytes: &[u8],
            _context: MutationContext,
        ) -> Result<UploadProgress, StatusCode> {
            Err(StatusCode::Unsupported)
        }

        async fn finalize_upload(
            &mut self,
            _request: FinalizeUploadRequest,
            _context: MutationContext,
        ) -> Result<PublishedObject, StatusCode> {
            Err(StatusCode::Unsupported)
        }
    }

    struct MaximumStatusBackend;

    impl StorageBackend for MaximumStatusBackend {
        fn status(&self) -> StorageBackendStatus {
            StorageBackendStatus {
                availability: StorageBackendAvailability::Unavailable,
                mutation_available: false,
                provision_available: false,
                fault: ProvisioningFault::MediaIntegrity,
                device_blocks: u64::MAX,
                locator_generation: u64::MAX,
                region_start_block: Some(u64::MAX),
                media_id: Some(MediaId([0xff; 16])),
                total_blocks: u64::MAX,
                free_blocks: u64::MAX,
                last_sequence: u64::MAX,
                published_objects: u32::MAX,
                upload: Some(UploadProgress {
                    upload_id: UploadId(u64::MAX),
                    phase: UploadPhase::Receiving,
                    next_chunk: u32::MAX,
                    accepted_bytes: u64::MAX,
                    total_bytes: u64::MAX,
                }),
                degraded_anchor: true,
                degraded_locator: true,
            }
        }

        async fn begin_upload(
            &mut self,
            _plan: UploadPlan,
            _context: MutationContext,
        ) -> Result<UploadProgress, StatusCode> {
            Err(StatusCode::Unsupported)
        }

        async fn put_chunk(
            &mut self,
            _header: ChunkUploadHeader,
            _bytes: &[u8],
            _context: MutationContext,
        ) -> Result<UploadProgress, StatusCode> {
            Err(StatusCode::Unsupported)
        }

        async fn finalize_upload(
            &mut self,
            _request: FinalizeUploadRequest,
            _context: MutationContext,
        ) -> Result<PublishedObject, StatusCode> {
            Err(StatusCode::Unsupported)
        }
    }

    struct ReadyBackend {
        upload: Option<UploadProgress>,
        published: Option<PublishedObject>,
        provisioned: Option<CacheProvisionRequest>,
        calls: u8,
    }

    impl ReadyBackend {
        const fn new() -> Self {
            Self {
                upload: None,
                published: None,
                provisioned: None,
                calls: 0,
            }
        }
    }

    impl StorageBackend for ReadyBackend {
        fn status(&self) -> StorageBackendStatus {
            StorageBackendStatus {
                availability: StorageBackendAvailability::Ready,
                mutation_available: false,
                provision_available: true,
                fault: ProvisioningFault::None,
                device_blocks: 2_300,
                locator_generation: 1,
                region_start_block: Some(2_048),
                media_id: Some(MediaId([0x5a; 16])),
                total_blocks: 1_000,
                free_blocks: 900,
                last_sequence: 3,
                published_objects: 2,
                upload: self.upload,
                degraded_anchor: false,
                degraded_locator: false,
            }
        }

        async fn provision(
            &mut self,
            request: CacheProvisionRequest,
            context: MutationContext,
        ) -> Result<(), StatusCode> {
            self.calls = self.calls.saturating_add(1);
            context.validate().map_err(storage_error_status)?;
            self.provisioned = Some(request);
            Ok(())
        }

        async fn inspect_published(
            &mut self,
            expected: PublishedObject,
        ) -> Result<PublishedObject, StatusCode> {
            self.calls = self.calls.saturating_add(1);
            match self.published {
                Some(publication) if publication == expected => Ok(publication),
                Some(_) => Err(StatusCode::Integrity),
                None => Err(StatusCode::NotFound),
            }
        }

        async fn begin_upload(
            &mut self,
            plan: UploadPlan,
            context: MutationContext,
        ) -> Result<UploadProgress, StatusCode> {
            self.calls = self.calls.saturating_add(1);
            context.validate().map_err(storage_error_status)?;
            let progress = UploadProgress {
                upload_id: plan.upload_id,
                phase: UploadPhase::Receiving,
                next_chunk: 0,
                accepted_bytes: 0,
                total_bytes: plan.object.byte_len,
            };
            self.upload = Some(progress);
            Ok(progress)
        }

        async fn put_chunk(
            &mut self,
            header: ChunkUploadHeader,
            bytes: &[u8],
            context: MutationContext,
        ) -> Result<UploadProgress, StatusCode> {
            self.calls = self.calls.saturating_add(1);
            context.validate().map_err(storage_error_status)?;
            let Some(mut progress) = self.upload else {
                return Err(StatusCode::NotFound);
            };
            progress.next_chunk = header.index.saturating_add(1);
            progress.accepted_bytes = progress
                .accepted_bytes
                .saturating_add(u64::try_from(bytes.len()).unwrap());
            self.upload = Some(progress);
            Ok(progress)
        }

        async fn finalize_upload(
            &mut self,
            request: FinalizeUploadRequest,
            context: MutationContext,
        ) -> Result<PublishedObject, StatusCode> {
            self.calls = self.calls.saturating_add(1);
            context.validate().map_err(storage_error_status)?;
            let Some(progress) = self.upload else {
                return Err(StatusCode::NotFound);
            };
            if progress.upload_id != request.upload_id {
                return Err(StatusCode::Conflict);
            }
            self.upload = None;
            let publication = PublishedObject {
                object: plan().object,
                manifest: plan().manifest,
            };
            self.published = Some(publication);
            Ok(publication)
        }
    }

    #[test]
    fn status_is_bounded_and_does_not_claim_a_backend() {
        let mut service = service();
        let mut backend = UnavailableStorageBackend;
        let response = block_on(service.dispatch(
            &mut backend,
            &ServiceRequest::storage_status(),
            DeviceCycle(10),
        ));
        assert_eq!(response.http_status, 200);
        assert_eq!(response.media, ResponseMedia::Json);
        assert_eq!(
            response.bytes(),
            b"{\"backend\":\"unavailable\",\"fault\":\"none\",\"mutation_available\":false,\"provision_available\":false,\"device_blocks\":0,\"region_start\":null,\"region_blocks\":0,\"free_blocks\":0,\"locator_generation\":0,\"published_objects\":0,\"upload_next_chunk\":null}"
        );
    }

    #[test]
    fn human_status_fits_at_every_numeric_wire_maximum() {
        let service = service();
        let response = service.human_status(&MaximumStatusBackend, DeviceCycle(10));
        assert_eq!(response.http_status, 200);
        assert_eq!(response.media, ResponseMedia::Json);
        assert_eq!(response.bytes().len(), 355);
        assert!(response.bytes().ends_with(b"4294967295}"));
    }

    #[test]
    fn identified_card_remains_detached_until_explicit_region_provisioning() {
        let mut service = service();
        observe(&mut service, SafetyState::Safe, 1, 10, false);
        let mut backend = DetachedBackend(17);
        let response = block_on(service.dispatch(
            &mut backend,
            &ServiceRequest::storage_status(),
            DeviceCycle(10),
        ));
        assert_eq!(
            response.bytes(),
            b"{\"backend\":\"detached\",\"fault\":\"none\",\"mutation_available\":false,\"provision_available\":true,\"device_blocks\":4096,\"region_start\":null,\"region_blocks\":0,\"free_blocks\":0,\"locator_generation\":0,\"published_objects\":0,\"upload_next_chunk\":null}"
        );
        let response = block_on(service.dispatch(
            &mut backend,
            &request(Operation::StorageStatus, &[]),
            DeviceCycle(11),
        ));
        let (status, body) = response_parts(&response);
        assert_eq!(status, StatusCode::Ok);
        assert_eq!(body[0], StorageBackendAvailability::Detached as u8);
        assert_eq!(u64::from_le_bytes(body[8..16].try_into().unwrap()), 4_096);
        assert_eq!(u64::from_le_bytes(body[24..32].try_into().unwrap()), 0);
        assert_eq!(body[1], 0);
        assert_eq!(body[5], 1);
        assert_eq!(backend.into_device(), 17);
    }

    #[test]
    fn structurally_valid_upload_is_explicitly_unsupported_without_backend() {
        let mut service = service();
        let mut backend = UnavailableStorageBackend;
        let response = block_on(service.dispatch(
            &mut backend,
            &request(Operation::StorageBeginUpload, &plan().encode()),
            DeviceCycle(10),
        ));
        assert_eq!(response_status(&response), StatusCode::Unsupported);
    }

    #[test]
    fn exact_publication_inspection_is_read_only_and_echoes_identity() {
        let publication = PublishedObject {
            object: plan().object,
            manifest: plan().manifest,
        };
        let native = request(Operation::StorageInspect, &publication.encode());
        let mut service = service();
        let mut backend = ReadyBackend::new();

        let response = block_on(service.dispatch(&mut backend, &native, DeviceCycle(10)));
        assert_eq!(response_status(&response), StatusCode::NotFound);
        assert_eq!(backend.calls, 1);

        backend.published = Some(publication);
        let response = block_on(service.dispatch(&mut backend, &native, DeviceCycle(11)));
        let (status, body) = response_parts(&response);
        assert_eq!(status, StatusCode::Ok);
        assert_eq!(PublishedObject::decode(body, CACHE_LIMITS), Ok(publication));
        assert_eq!(backend.calls, 2);

        let response = block_on(service.dispatch(
            &mut backend,
            &request(Operation::StorageInspect, &[0; PublishedObject::WIRE_LEN]),
            DeviceCycle(12),
        ));
        assert_eq!(response_status(&response), StatusCode::InvalidRequest);
        assert_eq!(backend.calls, 2);
    }

    #[test]
    fn invalid_storage_body_is_rejected_before_backend_dispatch() {
        let mut service = service();
        let mut backend = ReadyBackend::new();
        let response = block_on(service.dispatch(
            &mut backend,
            &request(Operation::StorageBeginUpload, &[0; UploadPlan::WIRE_LEN]),
            DeviceCycle(10),
        ));
        assert_eq!(response_status(&response), StatusCode::InvalidRequest);
        assert_eq!(backend.calls, 0);
    }

    #[test]
    fn chunk_digest_failure_is_reported_as_integrity_not_syntax() {
        let header = ChunkUploadHeader {
            upload_id: UploadId(7),
            index: 0,
            byte_len: 4,
            content: sha256(b"wrong"),
        };
        let mut body = [0_u8; ChunkUploadHeader::WIRE_LEN + 4];
        body[..ChunkUploadHeader::WIRE_LEN].copy_from_slice(&header.encode());
        body[ChunkUploadHeader::WIRE_LEN..].copy_from_slice(b"abcd");
        let mut service = service();
        let mut backend = ReadyBackend::new();
        let response = block_on(service.dispatch(
            &mut backend,
            &request(Operation::StoragePutChunk, &body),
            DeviceCycle(10),
        ));
        assert_eq!(response_status(&response), StatusCode::Integrity);
        assert_eq!(backend.calls, 0);
    }

    #[test]
    fn wrong_frame_family_never_reaches_storage_dispatch() {
        let mut native = request(Operation::StorageStatus, &[]);
        native.bytes[6] = FrameKind::Health.wire_value();
        let mut service = service();
        let mut backend = ReadyBackend::new();
        let response = block_on(service.dispatch(&mut backend, &native, DeviceCycle(10)));
        assert_eq!(response.http_status, 400);
        assert_eq!(response.media, ResponseMedia::Json);
        assert_eq!(backend.calls, 0);
    }

    #[test]
    fn universal_native_envelope_preserves_nonstorage_family_and_correlation() {
        let request = request(Operation::JobStatus, &[]);
        let native = NativeRequest::decode(request.bytes()).unwrap();
        assert_eq!(native.frame.kind, FrameKind::Job);
        assert_eq!(native.message.operation, Operation::JobStatus);
        assert!(native.body.is_empty());
        let response = ServiceResponse::native(
            native,
            DeviceCycle(17),
            StatusCode::Unsupported,
            b"job-status",
        )
        .unwrap();
        let (status, body) = response_parts(&response);
        assert_eq!(status, StatusCode::Unsupported);
        assert_eq!(body, b"job-status");
    }

    #[test]
    fn backend_mutation_remains_fail_closed_until_safe_state_is_observed() {
        let mut service = service();
        let mut backend = ReadyBackend::new();
        let begin = request(Operation::StorageBeginUpload, &plan().encode());
        let response = block_on(service.dispatch(&mut backend, &begin, DeviceCycle(10)));
        assert_eq!(response_status(&response), StatusCode::ForbiddenState);
        assert_eq!(backend.calls, 1);
        observe(&mut service, SafetyState::Safe, 1, 11, false);
        let response = block_on(service.dispatch(&mut backend, &begin, DeviceCycle(11)));
        let (status, body) = response_parts(&response);
        assert_eq!(status, StatusCode::Ok);
        assert_eq!(UploadProgress::decode(body).unwrap().next_chunk, 0);
    }

    #[test]
    fn storage_mutation_expires_and_job_ownership_blocks_it() {
        let begin = request(Operation::StorageBeginUpload, &plan().encode());

        let mut stale_service = service();
        let mut stale_backend = ReadyBackend::new();
        observe(&mut stale_service, SafetyState::Safe, 1, 10, false);
        let response =
            block_on(stale_service.dispatch(&mut stale_backend, &begin, DeviceCycle(110)));
        assert_eq!(response_status(&response), StatusCode::Ok);
        let response =
            block_on(stale_service.dispatch(&mut stale_backend, &begin, DeviceCycle(111)));
        assert_eq!(response_status(&response), StatusCode::ForbiddenState);

        let mut busy_service = service();
        let mut busy_backend = ReadyBackend::new();
        observe(&mut busy_service, SafetyState::Safe, 1, 10, true);
        let response = block_on(busy_service.dispatch(&mut busy_backend, &begin, DeviceCycle(10)));
        assert_eq!(response_status(&response), StatusCode::ForbiddenState);

        let mut local_service = service();
        let mut local_backend = ReadyBackend::new();
        observe(&mut local_service, SafetyState::Safe, 1, 10, false);
        local_service.set_service_job_active(true);
        let response =
            block_on(local_service.dispatch(&mut local_backend, &begin, DeviceCycle(10)));
        assert_eq!(response_status(&response), StatusCode::ForbiddenState);
        local_service.set_service_job_active(false);
        let response =
            block_on(local_service.dispatch(&mut local_backend, &begin, DeviceCycle(10)));
        assert_eq!(response_status(&response), StatusCode::Ok);

        let mut configuration_service = service();
        let mut configuration_backend = ReadyBackend::new();
        observe(&mut configuration_service, SafetyState::Safe, 1, 10, false);
        configuration_service.set_configuration_transaction_active(true);
        assert!(
            configuration_service
                .configuration_mutation_context(DeviceCycle(10))
                .validate()
                .is_ok()
        );
        let response = block_on(configuration_service.dispatch(
            &mut configuration_backend,
            &begin,
            DeviceCycle(10),
        ));
        assert_eq!(response_status(&response), StatusCode::ForbiddenState);
    }

    #[test]
    fn rejected_or_fault_signaled_snapshot_revokes_storage_authority() {
        let mut service = service();
        let mut backend = ReadyBackend::new();
        let begin = request(Operation::StorageBeginUpload, &plan().encode());
        observe(&mut service, SafetyState::Safe, 1, 10, false);

        assert!(
            service
                .observe_safety_snapshot(2, DeviceCycle(11), DeviceCycle(11), &[0; 3])
                .is_err()
        );
        let response = block_on(service.dispatch(&mut backend, &begin, DeviceCycle(11)));
        assert_eq!(response_status(&response), StatusCode::ForbiddenState);

        observe(&mut service, SafetyState::Safe, 3, 12, false);
        service.invalidate_safety_observation();
        let response = block_on(service.dispatch(&mut backend, &begin, DeviceCycle(12)));
        assert_eq!(response_status(&response), StatusCode::ForbiddenState);
    }

    #[test]
    fn native_status_reports_exact_backend_and_effective_mutation_state() {
        let mut service = service();
        let mut backend = ReadyBackend::new();
        let response = block_on(service.dispatch(
            &mut backend,
            &request(Operation::StorageStatus, &[]),
            DeviceCycle(10),
        ));
        let (status, body) = response_parts(&response);
        assert_eq!(status, StatusCode::Ok);
        assert_eq!(body.len(), STORAGE_BACKEND_STATUS_WIRE_BYTES);
        assert_eq!(body[0], StorageBackendAvailability::Ready as u8);
        assert_eq!(body[1], 0);
        assert_eq!(u64::from_le_bytes(body[32..40].try_into().unwrap()), 900);

        observe(&mut service, SafetyState::Configured, 1, 11, false);
        let response = block_on(service.dispatch(
            &mut backend,
            &request(Operation::StorageStatus, &[]),
            DeviceCycle(11),
        ));
        assert_eq!(response_parts(&response).1[1], 1);
    }

    #[test]
    fn destructive_provision_requires_canonical_body_and_safe_state() {
        let provision = CacheProvisionRequest::new(
            2_300,
            1,
            Some(MediaId([0x5a; 16])),
            MediaRegion {
                start_block: 2_048,
                block_count: 200,
            },
            MediaId([0xa5; 16]),
            false,
        )
        .unwrap();
        let native = request(Operation::StorageProvision, &provision.encode());
        let mut service = service();
        let mut backend = ReadyBackend::new();
        let response = block_on(service.dispatch(&mut backend, &native, DeviceCycle(10)));
        assert_eq!(response_status(&response), StatusCode::ForbiddenState);
        assert_eq!(backend.provisioned, None);

        observe(&mut service, SafetyState::Safe, 1, 11, false);
        let response = block_on(service.dispatch(&mut backend, &native, DeviceCycle(11)));
        let (status, body) = response_parts(&response);
        assert_eq!(status, StatusCode::Ok);
        assert_eq!(body.len(), STORAGE_BACKEND_STATUS_WIRE_BYTES);
        assert_eq!(backend.provisioned, Some(provision));

        service.set_configuration_active(true);
        let calls = backend.calls;
        let response = block_on(service.dispatch(&mut backend, &native, DeviceCycle(11)));
        assert_eq!(response_status(&response), StatusCode::ForbiddenState);
        assert_eq!(backend.calls, calls);
        service.set_configuration_active(false);

        let mut tampered = provision.encode();
        tampered[56] ^= 1;
        let calls = backend.calls;
        let response = block_on(service.dispatch(
            &mut backend,
            &request(Operation::StorageProvision, &tampered),
            DeviceCycle(12),
        ));
        assert_eq!(response_status(&response), StatusCode::Integrity);
        assert_eq!(backend.calls, calls);
    }
}
