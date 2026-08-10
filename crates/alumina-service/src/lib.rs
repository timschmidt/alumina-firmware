#![no_std]
#![doc = "Bounded service-core request admission shared by ESP firmware and simulation."]

use alumina_net::MAX_AUTHENTICATED_BODY_BYTES;
use alumina_protocol::{
    DeviceCycle, FrameHeader, FrameKind, MessageDirection, MessageHeader, Operation, StatusCode,
};
use alumina_safety::SafetyState;
use alumina_storage::{
    CacheLimits, ChunkUploadHeader, ContentId, Error as StorageError, FinalizeUploadRequest,
    MutationContext, PublishToken, PublishedObject, UploadCoordinator, UploadId, UploadPlan,
    UploadProgress, VerifiedChunk, sha256,
};

/// Initial bounded protocol/cache policy; free SD capacity remains a runtime limit.
pub const CACHE_LIMITS: CacheLimits = CacheLimits {
    maximum_object_bytes: 64 * 1_024 * 1_024,
    maximum_chunk_bytes: 1_024,
    maximum_chunks: 65_536,
};

/// Largest service response, including a native frame or bounded JSON status.
pub const MAX_SERVICE_RESPONSE_BYTES: usize = 128;

const MAX_NATIVE_PAYLOAD_BYTES: usize = MAX_AUTHENTICATED_BODY_BYTES - FrameHeader::WIRE_LEN;
const EMPTY_STORAGE_STATUS: &[u8] = b"{\"backend_available\":false,\"mutation_available\":false}";
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

/// Storage transaction state intentionally owned by the service-core task future.
pub struct StorageServiceState {
    uploads: UploadCoordinator,
    safety_state: SafetyState,
    realtime_job_active: bool,
}

impl StorageServiceState {
    /// Starts in boot state, where every mutation is forbidden.
    pub const fn new() -> Self {
        Self {
            uploads: UploadCoordinator::new(),
            safety_state: SafetyState::Boot,
            realtime_job_active: false,
        }
    }

    /// Dispatches one already authenticated request on the sole service owner.
    pub fn dispatch(&mut self, request: &ServiceRequest, now: DeviceCycle) -> ServiceResponse {
        match request.kind() {
            ServiceRequestKind::StorageStatus => {
                ServiceResponse::from_bytes(200, ResponseMedia::Json, EMPTY_STORAGE_STATUS)
            }
            ServiceRequestKind::NativeFrame => self.dispatch_native(request.bytes(), now),
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

    /// Starts or resumes one decoded `StorageBeginUpload` declaration.
    pub fn begin_upload(&mut self, plan: UploadPlan) -> Result<UploadProgress, StorageError> {
        self.uploads
            .begin(plan, CACHE_LIMITS, self.mutation_context())
    }

    /// Hashes and validates the next received chunk without mutating durable state.
    pub fn verify_chunk(
        &self,
        upload_id: UploadId,
        index: u32,
        claimed_content: ContentId,
        bytes: &[u8],
    ) -> Result<VerifiedChunk, StorageError> {
        self.uploads
            .verify_chunk(upload_id, index, claimed_content, bytes)
    }

    /// Records a verified chunk only after the async SD backend has made it durable.
    pub fn record_chunk(&mut self, chunk: VerifiedChunk) -> Result<UploadProgress, StorageError> {
        self.uploads.record_chunk(chunk, self.mutation_context())
    }

    /// Enters publish-pending after the backend streams both canonical hashes.
    pub fn finalize_upload(
        &mut self,
        upload_id: UploadId,
        observed_object: ContentId,
        observed_manifest: ContentId,
    ) -> Result<PublishToken, StorageError> {
        self.uploads.finalize(
            upload_id,
            observed_object,
            observed_manifest,
            self.mutation_context(),
        )
    }

    /// Clears the journal only after backend atomic publication succeeds.
    pub fn record_published(
        &mut self,
        token: PublishToken,
    ) -> Result<PublishedObject, StorageError> {
        self.uploads
            .record_published(token, self.mutation_context())
    }

    /// Abandons an upload; durable unreferenced chunks remain scrub/GC candidates.
    pub fn abort_upload(
        &mut self,
        upload_id: UploadId,
    ) -> Result<Option<UploadPlan>, StorageError> {
        self.uploads.abort(upload_id, self.mutation_context())
    }

    /// Returns bounded progress without exposing the coordinator or filesystem.
    pub fn upload_progress(&self) -> Option<UploadProgress> {
        self.uploads.checkpoint().map(|checkpoint| UploadProgress {
            upload_id: checkpoint.plan.upload_id,
            phase: checkpoint.phase,
            next_chunk: checkpoint.next_chunk,
            accepted_bytes: checkpoint.accepted_bytes,
            total_bytes: checkpoint.plan.object.byte_len,
        })
    }

    fn dispatch_native(&mut self, bytes: &[u8], now: DeviceCycle) -> ServiceResponse {
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
        let status = validate_storage_body(message.operation, message.body_len, body);
        native_status_response(frame, message, now, status)
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

fn validate_storage_body(operation: Operation, body_len: u32, body: &[u8]) -> StatusCode {
    let decoded = match operation {
        Operation::StorageBeginUpload => UploadPlan::decode(body, CACHE_LIMITS)
            .map(|_| ())
            .map_err(|_| StorageBodyError::Malformed),
        Operation::StoragePutChunk => validate_chunk_body(body_len, body),
        Operation::StorageFinalize => FinalizeUploadRequest::decode(body)
            .map(|_| ())
            .map_err(|_| StorageBodyError::Malformed),
        Operation::StorageStatus if body.is_empty() => return StatusCode::Unsupported,
        _ => return StatusCode::Unsupported,
    };
    match decoded {
        Ok(()) => StatusCode::Unsupported,
        Err(StorageBodyError::Malformed) => StatusCode::InvalidRequest,
        Err(StorageBodyError::Integrity) => StatusCode::Integrity,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StorageBodyError {
    Malformed,
    Integrity,
}

fn validate_chunk_body(body_len: u32, body: &[u8]) -> Result<(), StorageBodyError> {
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
    Ok(())
}

fn invalid_native_frame() -> ServiceResponse {
    ServiceResponse::from_bytes(400, ResponseMedia::Json, INVALID_NATIVE_FRAME)
}

fn native_status_response(
    request_frame: FrameHeader,
    request_message: MessageHeader,
    now: DeviceCycle,
    status: StatusCode,
) -> ServiceResponse {
    let payload_len = u32::try_from(MessageHeader::WIRE_LEN).expect("message header fits u32");
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
        0,
    );
    let mut bytes = [0_u8; FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN];
    bytes[..FrameHeader::WIRE_LEN].copy_from_slice(&frame.encode());
    bytes[FrameHeader::WIRE_LEN..].copy_from_slice(&message.encode());
    ServiceResponse::from_bytes(200, ResponseMedia::NativeFrame, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alumina_protocol::{Digest, MessageDirection};
    use alumina_storage::{DigestAlgorithm, ObjectKind, StoredObject};

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

    fn response_status(response: &ServiceResponse) -> StatusCode {
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
        message.status
    }

    #[test]
    fn status_is_bounded_and_does_not_claim_a_backend() {
        let mut service = StorageServiceState::new();
        let response = service.dispatch(&ServiceRequest::storage_status(), DeviceCycle(10));
        assert_eq!(response.http_status, 200);
        assert_eq!(response.media, ResponseMedia::Json);
        assert_eq!(response.bytes(), EMPTY_STORAGE_STATUS);
    }

    #[test]
    fn structurally_valid_upload_is_explicitly_unsupported_without_backend() {
        let mut service = StorageServiceState::new();
        let response = service.dispatch(
            &request(Operation::StorageBeginUpload, &plan().encode()),
            DeviceCycle(10),
        );
        assert_eq!(response_status(&response), StatusCode::Unsupported);
    }

    #[test]
    fn invalid_storage_body_is_rejected_before_backend_dispatch() {
        let mut service = StorageServiceState::new();
        let response = service.dispatch(
            &request(Operation::StorageBeginUpload, &[0; UploadPlan::WIRE_LEN]),
            DeviceCycle(10),
        );
        assert_eq!(response_status(&response), StatusCode::InvalidRequest);
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
        let response =
            service.dispatch(&request(Operation::StoragePutChunk, &body), DeviceCycle(10));
        assert_eq!(response_status(&response), StatusCode::Integrity);
    }

    #[test]
    fn wrong_frame_family_never_reaches_storage_dispatch() {
        let mut native = request(Operation::StorageStatus, &[]);
        native.bytes[6] = FrameKind::Health.wire_value();
        let mut service = StorageServiceState::new();
        let response = service.dispatch(&native, DeviceCycle(10));
        assert_eq!(response.http_status, 400);
        assert_eq!(response.media, ResponseMedia::Json);
    }

    #[test]
    fn coordinator_remains_fail_closed_until_safe_state_is_observed() {
        let mut service = StorageServiceState::new();
        assert_eq!(
            service.begin_upload(plan()),
            Err(StorageError::MutationForbidden)
        );
        service.observe_safety_state(SafetyState::Safe);
        assert!(service.begin_upload(plan()).is_ok());
    }
}
