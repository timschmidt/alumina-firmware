//! Stateless authenticated access to one immutable canonical board document.

use alumina_board::BoardPackage;
use alumina_capability::{
    CAPABILITY_READ_RESPONSE_PREFIX_BYTES, CapabilityError, CapabilityReadRequest,
    CapabilityReadResponse, MAX_CAPABILITY_CHUNK_BYTES, read_verified_range,
};
use alumina_protocol::{DeviceCycle, FrameHeader, FrameKind, MessageHeader, Operation, StatusCode};

use crate::{
    MAX_SERVICE_RESPONSE_BYTES, NativeRequest, ServiceRequest, ServiceRequestKind, ServiceResponse,
};

const MAX_CAPABILITY_RESPONSE_BODY: usize =
    CAPABILITY_READ_RESPONSE_PREFIX_BYTES + MAX_CAPABILITY_CHUNK_BYTES;

/// Stateless dispatcher over a caller-selected, immutable board package.
pub struct CapabilityDocumentService;

impl CapabilityDocumentService {
    /// Whether a valid universal request selects the capability family.
    pub fn handles(request: &ServiceRequest) -> bool {
        request.kind() == ServiceRequestKind::NativeFrame
            && NativeRequest::decode(request.bytes())
                .is_ok_and(|request| request.frame.kind == FrameKind::Capabilities)
    }

    /// Returns one verified range or a correlated fail-closed status.
    pub fn dispatch(
        package: &BoardPackage<'_>,
        request: &ServiceRequest,
        now: DeviceCycle,
    ) -> ServiceResponse {
        let Ok(native) = NativeRequest::decode(request.bytes()) else {
            return ServiceResponse::invalid_native();
        };
        if native.frame.kind != FrameKind::Capabilities {
            return ServiceResponse::invalid_native();
        }
        if native.message.operation != Operation::CapabilitiesGet
            || !native.frame.config_digest.is_zero()
        {
            return respond(native, now, StatusCode::InvalidRequest, &[]);
        }
        let request = match CapabilityReadRequest::decode(native.body) {
            Ok(request) => request,
            Err(_) => return respond(native, now, StatusCode::InvalidRequest, &[]),
        };
        let declared = package.board.capability_digest;
        if !request.expected_digest.is_zero() && request.expected_digest != declared {
            return respond(native, now, StatusCode::Conflict, &[]);
        }

        let mut body = [0_u8; MAX_CAPABILITY_RESPONSE_BODY];
        let chunk_start = CAPABILITY_READ_RESPONSE_PREFIX_BYTES;
        let maximum = usize::from(request.maximum_bytes);
        let read = match read_verified_range(
            package,
            request.offset,
            &mut body[chunk_start..chunk_start + maximum],
        ) {
            Ok(read) if read.byte_len != 0 => read,
            Ok(_) => return respond(native, now, StatusCode::InvalidRequest, &[]),
            Err(error) => return respond(native, now, capability_error_status(error), &[]),
        };
        let prefix = CapabilityReadResponse {
            identity: read.identity,
            offset: read.offset,
            chunk_len: read.byte_len,
            complete: read.complete,
        };
        let prefix = match prefix.encode() {
            Ok(prefix) => prefix,
            Err(_) => return respond(native, now, StatusCode::Internal, &[]),
        };
        body[..chunk_start].copy_from_slice(&prefix);
        let body_len = chunk_start + usize::from(read.byte_len);
        respond(native, now, StatusCode::Ok, &body[..body_len])
    }
}

fn respond(
    native: NativeRequest<'_>,
    now: DeviceCycle,
    status: StatusCode,
    body: &[u8],
) -> ServiceResponse {
    ServiceResponse::native(native, now, status, body)
        .unwrap_or_else(|_| ServiceResponse::invalid_native())
}

const fn capability_error_status(error: CapabilityError) -> StatusCode {
    match error {
        CapabilityError::Range => StatusCode::InvalidRequest,
        CapabilityError::MissingDeclaredDigest => StatusCode::Unsupported,
        CapabilityError::Length
        | CapabilityError::DeclaredDigestMismatch { .. }
        | CapabilityError::Board(_) => StatusCode::Integrity,
    }
}

const _: () = assert!(
    MAX_CAPABILITY_RESPONSE_BODY
        <= MAX_SERVICE_RESPONSE_BYTES - FrameHeader::WIRE_LEN - MessageHeader::WIRE_LEN
);
