#![no_std]
#![doc = "Bounded service-core request admission shared by ESP firmware and simulation."]

use core::fmt::Write as _;

use alumina_net::MAX_AUTHENTICATED_BODY_BYTES;
use alumina_protocol::{
    DeviceCycle, FrameHeader, FrameKind, MessageDirection, MessageHeader, Operation, StatusCode,
};
use alumina_safety::SafetyState;
use alumina_storage::media::{
    AsyncBlockDevice, CacheMedia, MediaAvailability, MediaError, MediaStatus,
};
use alumina_storage::{
    CacheLimits, ChunkUploadHeader, Error as StorageError, FinalizeUploadRequest, MutationContext,
    PublishedObject, UploadPlan, UploadProgress, sha256,
};
use heapless::String as FixedString;

/// Initial bounded protocol/cache policy; free SD capacity remains a runtime limit.
pub const CACHE_LIMITS: CacheLimits = CacheLimits {
    maximum_object_bytes: 64 * 1_024 * 1_024,
    maximum_chunk_bytes: 1_024,
    maximum_chunks: 65_536,
};

/// Largest service response, including a native frame or bounded JSON status.
pub const MAX_SERVICE_RESPONSE_BYTES: usize = 192;
/// Exact native V1 storage-status body length.
pub const STORAGE_BACKEND_STATUS_WIRE_BYTES: usize = 72;

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
    /// A write/sync failure requires remount before more work.
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
}

impl StorageBackendStatus {
    /// Exact fixed native representation, with zero-filled absent upload fields.
    pub fn encode(self) -> [u8; STORAGE_BACKEND_STATUS_WIRE_BYTES] {
        let mut encoded = [0_u8; STORAGE_BACKEND_STATUS_WIRE_BYTES];
        encoded[0] = self.availability as u8;
        encoded[1] = u8::from(self.mutation_available);
        encoded[2] = u8::from(self.degraded_anchor);
        encoded[3] = u8::from(self.upload.is_some());
        encoded[8..16].copy_from_slice(&self.total_blocks.to_le_bytes());
        encoded[16..24].copy_from_slice(&self.free_blocks.to_le_bytes());
        encoded[24..32].copy_from_slice(&self.last_sequence.to_le_bytes());
        encoded[32..36].copy_from_slice(&self.published_objects.to_le_bytes());
        if let Some(progress) = self.upload {
            encoded[40..72].copy_from_slice(&progress.encode());
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
            total_blocks: 0,
            free_blocks: 0,
            last_sequence: 0,
            published_objects: 0,
            upload: None,
            degraded_anchor: false,
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
        media_status(self.status())
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

fn media_status(status: MediaStatus) -> StorageBackendStatus {
    StorageBackendStatus {
        availability: match status.availability {
            MediaAvailability::Detached => StorageBackendAvailability::Detached,
            MediaAvailability::Ready => StorageBackendAvailability::Ready,
            MediaAvailability::Faulted => StorageBackendAvailability::Faulted,
        },
        mutation_available: false,
        total_blocks: status.total_blocks,
        free_blocks: status.free_blocks,
        last_sequence: status.last_sequence,
        published_objects: status.published_objects,
        upload: status.upload,
        degraded_anchor: status.degraded_anchor,
    }
}

fn media_error_status<E>(error: MediaError<E>) -> StatusCode {
    match error {
        MediaError::Unformatted | MediaError::NotMounted => StatusCode::Unsupported,
        MediaError::Faulted | MediaError::Device(_) => StatusCode::Internal,
        MediaError::Corrupt(_) => StatusCode::Integrity,
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
        | StorageError::PublishTokenMismatch => StatusCode::Conflict,
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
    safety_state: SafetyState,
    realtime_job_active: bool,
}

impl StorageServiceState {
    /// Starts in boot state, where every mutation is forbidden.
    pub const fn new() -> Self {
        Self {
            safety_state: SafetyState::Boot,
            realtime_job_active: false,
        }
    }

    /// Dispatches one already authenticated request on the sole service owner.
    pub async fn dispatch<B: StorageBackend>(
        &mut self,
        backend: &mut B,
        request: &ServiceRequest,
        now: DeviceCycle,
    ) -> ServiceResponse {
        match request.kind() {
            ServiceRequestKind::StorageStatus => self.human_status(backend),
            ServiceRequestKind::NativeFrame => {
                self.dispatch_native(backend, request.bytes(), now).await
            }
        }
    }

    /// Updates the last core-1-authoritative safety state observation.
    pub fn observe_safety_state(&mut self, state: SafetyState) {
        self.safety_state = state;
    }

    /// Updates whether deterministic execution/prefetch currently owns the cache.
    pub fn observe_realtime_job_active(&mut self, active: bool) {
        self.realtime_job_active = active;
    }

    async fn dispatch_native<B: StorageBackend>(
        &mut self,
        backend: &mut B,
        bytes: &[u8],
        now: DeviceCycle,
    ) -> ServiceResponse {
        let Some(frame_bytes) = bytes.get(..FrameHeader::WIRE_LEN) else {
            return invalid_native_frame();
        };
        let Ok(frame) = FrameHeader::decode(
            frame_bytes,
            u32::try_from(MAX_NATIVE_PAYLOAD_BYTES).expect("native payload bound fits u32"),
        ) else {
            return invalid_native_frame();
        };
        let Ok(payload_len) = usize::try_from(frame.payload_len) else {
            return invalid_native_frame();
        };
        if frame.kind != FrameKind::Storage
            || FrameHeader::WIRE_LEN.checked_add(payload_len) != Some(bytes.len())
        {
            return invalid_native_frame();
        }
        let message_start = FrameHeader::WIRE_LEN;
        let message_end = message_start + MessageHeader::WIRE_LEN;
        let Some(message_bytes) = bytes.get(message_start..message_end) else {
            return invalid_native_frame();
        };
        let Ok(message) =
            MessageHeader::decode_and_validate(message_bytes, frame.kind, frame.payload_len)
        else {
            return invalid_native_frame();
        };
        if message.direction != MessageDirection::Request {
            return invalid_native_frame();
        }
        let body = &bytes[message_end..];
        let mut response_body = [0_u8; STORAGE_BACKEND_STATUS_WIRE_BYTES];
        let (status, response_len) = match message.operation {
            Operation::StorageStatus if body.is_empty() => {
                let status = self.effective_status(backend);
                response_body.copy_from_slice(&status.encode());
                (StatusCode::Ok, STORAGE_BACKEND_STATUS_WIRE_BYTES)
            }
            Operation::StorageBeginUpload => match UploadPlan::decode(body, CACHE_LIMITS) {
                Ok(plan) => match backend.begin_upload(plan, self.mutation_context()).await {
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
                        .put_chunk(header, chunk, self.mutation_context())
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
                    .finalize_upload(request, self.mutation_context())
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
    }

    fn human_status<B: StorageBackend>(&self, backend: &B) -> ServiceResponse {
        let status = self.effective_status(backend);
        let mut json = FixedString::<MAX_SERVICE_RESPONSE_BYTES>::new();
        write!(
            &mut json,
            "{{\"backend\":\"{}\",\"mutation_available\":{},\"free_blocks\":{},\"published_objects\":{},\"upload_next_chunk\":",
            status.availability.label(),
            status.mutation_available,
            status.free_blocks,
            status.published_objects,
        )
        .expect("bounded storage status prefix fits response");
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

    fn effective_status<B: StorageBackend>(&self, backend: &B) -> StorageBackendStatus {
        let mut status = backend.status();
        status.mutation_available = status.availability == StorageBackendAvailability::Ready
            && self.mutation_context().validate().is_ok();
        status
    }

    fn mutation_context(&self) -> MutationContext {
        let safe_for_mutation = matches!(
            self.safety_state,
            SafetyState::Safe | SafetyState::Configured
        );
        MutationContext {
            armed_or_energized: !safe_for_mutation,
            realtime_job_active: self.realtime_job_active,
        }
    }
}

impl Default for StorageServiceState {
    fn default() -> Self {
        Self::new()
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
) -> ServiceResponse {
    let payload_len = u32::try_from(MessageHeader::WIRE_LEN + body.len())
        .expect("bounded response payload fits u32");
    let frame = FrameHeader::new(
        FrameKind::Storage,
        payload_len,
        request_frame.sequence,
        now,
        request_frame.config_digest,
    );
    let message = MessageHeader::response(
        request_message.operation,
        request_message.correlation_id,
        status,
        u32::try_from(body.len()).expect("bounded response body fits u32"),
    );
    let mut bytes = [0_u8; MAX_SERVICE_RESPONSE_BYTES];
    let response_len = FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN + body.len();
    bytes[..FrameHeader::WIRE_LEN].copy_from_slice(&frame.encode());
    bytes[FrameHeader::WIRE_LEN..FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN]
        .copy_from_slice(&message.encode());
    bytes[FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN..response_len].copy_from_slice(body);
    ServiceResponse::from_bytes(200, ResponseMedia::NativeFrame, &bytes[..response_len])
}

#[cfg(test)]
mod tests {
    use super::*;
    use alumina_protocol::{Digest, MessageDirection};
    use alumina_storage::{
        ContentId, DigestAlgorithm, ObjectKind, StoredObject, UploadId, UploadPhase,
    };
    use embassy_futures::block_on;

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
            FrameKind::Storage,
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

    struct ReadyBackend {
        upload: Option<UploadProgress>,
        calls: u8,
    }

    impl ReadyBackend {
        const fn new() -> Self {
            Self {
                upload: None,
                calls: 0,
            }
        }
    }

    impl StorageBackend for ReadyBackend {
        fn status(&self) -> StorageBackendStatus {
            StorageBackendStatus {
                availability: StorageBackendAvailability::Ready,
                mutation_available: false,
                total_blocks: 1_000,
                free_blocks: 900,
                last_sequence: 3,
                published_objects: 2,
                upload: self.upload,
                degraded_anchor: false,
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
            Ok(PublishedObject {
                object: plan().object,
                manifest: plan().manifest,
            })
        }
    }

    #[test]
    fn status_is_bounded_and_does_not_claim_a_backend() {
        let mut service = StorageServiceState::new();
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
            b"{\"backend\":\"unavailable\",\"mutation_available\":false,\"free_blocks\":0,\"published_objects\":0,\"upload_next_chunk\":null}"
        );
    }

    #[test]
    fn structurally_valid_upload_is_explicitly_unsupported_without_backend() {
        let mut service = StorageServiceState::new();
        let mut backend = UnavailableStorageBackend;
        let response = block_on(service.dispatch(
            &mut backend,
            &request(Operation::StorageBeginUpload, &plan().encode()),
            DeviceCycle(10),
        ));
        assert_eq!(response_status(&response), StatusCode::Unsupported);
    }

    #[test]
    fn invalid_storage_body_is_rejected_before_backend_dispatch() {
        let mut service = StorageServiceState::new();
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
        let mut service = StorageServiceState::new();
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
        let mut service = StorageServiceState::new();
        let mut backend = ReadyBackend::new();
        let response = block_on(service.dispatch(&mut backend, &native, DeviceCycle(10)));
        assert_eq!(response.http_status, 400);
        assert_eq!(response.media, ResponseMedia::Json);
        assert_eq!(backend.calls, 0);
    }

    #[test]
    fn backend_mutation_remains_fail_closed_until_safe_state_is_observed() {
        let mut service = StorageServiceState::new();
        let mut backend = ReadyBackend::new();
        let begin = request(Operation::StorageBeginUpload, &plan().encode());
        let response = block_on(service.dispatch(&mut backend, &begin, DeviceCycle(10)));
        assert_eq!(response_status(&response), StatusCode::ForbiddenState);
        assert_eq!(backend.calls, 1);
        service.observe_safety_state(SafetyState::Safe);
        let response = block_on(service.dispatch(&mut backend, &begin, DeviceCycle(11)));
        let (status, body) = response_parts(&response);
        assert_eq!(status, StatusCode::Ok);
        assert_eq!(UploadProgress::decode(body).unwrap().next_chunk, 0);
    }

    #[test]
    fn native_status_reports_exact_backend_and_effective_mutation_state() {
        let mut service = StorageServiceState::new();
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
        assert_eq!(u64::from_le_bytes(body[16..24].try_into().unwrap()), 900);

        service.observe_safety_state(SafetyState::Configured);
        let response = block_on(service.dispatch(
            &mut backend,
            &request(Operation::StorageStatus, &[]),
            DeviceCycle(11),
        ));
        assert_eq!(response_parts(&response).1[1], 1);
    }
}
