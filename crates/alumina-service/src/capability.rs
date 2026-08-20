//! Stateless authenticated access to immutable board capability and visual bytes.

use alumina_board::BoardPackage;
use alumina_capability::{
    CAPABILITY_READ_RESPONSE_PREFIX_BYTES, CapabilityError, CapabilityReadRequest,
    CapabilityReadResponse, MAX_CAPABILITY_CHUNK_BYTES, MAX_VISUAL_ASSET_CHUNK_BYTES,
    VISUAL_ASSET_READ_RESPONSE_PREFIX_BYTES, VisualAssetIdentity, VisualAssetReadRequest,
    VisualAssetReadResponse, read_verified_range,
};
use alumina_protocol::{
    DeviceCycle, Digest, FrameHeader, FrameKind, MessageHeader, Operation, StatusCode,
};
use alumina_storage::sha256;

use crate::{
    MAX_SERVICE_RESPONSE_BYTES, NativeRequest, ServiceRequest, ServiceRequestKind, ServiceResponse,
};

const MAX_CAPABILITY_RESPONSE_BODY: usize =
    CAPABILITY_READ_RESPONSE_PREFIX_BYTES + MAX_CAPABILITY_CHUNK_BYTES;
const MAX_VISUAL_ASSET_RESPONSE_BODY: usize =
    VISUAL_ASSET_READ_RESPONSE_PREFIX_BYTES + MAX_VISUAL_ASSET_CHUNK_BYTES;

/// One immutable visual blob compiled into a board or simulation image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityVisualAsset<'a> {
    /// SHA-256 declared by exactly one visual in the canonical capability.
    pub digest: Digest,
    /// Exact immutable bytes returned by the range service.
    pub bytes: &'a [u8],
}

/// A complete immutable asset catalog verified against one canonical package.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedCapabilityVisualAssets<'a> {
    capability_digest: Digest,
    assets: &'a [CapabilityVisualAsset<'a>],
}

impl<'a> VerifiedCapabilityVisualAssets<'a> {
    /// Verifies exact declaration coverage and every asset's complete SHA-256.
    ///
    /// The result can be retained after boot and reused for every range. No
    /// asset bytes are allocated or copied.
    pub fn try_new(
        package: &BoardPackage<'_>,
        assets: &'a [CapabilityVisualAsset<'a>],
    ) -> Result<Self, CapabilityVisualCatalogError> {
        let capability = alumina_capability::verify_declared_identity(package)
            .map_err(CapabilityVisualCatalogError::Capability)?;
        for (index, asset) in assets.iter().enumerate() {
            if asset.digest.is_zero() || asset.bytes.is_empty() {
                return Err(CapabilityVisualCatalogError::Incomplete);
            }
            if u32::try_from(asset.bytes.len()).is_err() {
                return Err(CapabilityVisualCatalogError::Length);
            }
            if assets[..index]
                .iter()
                .any(|prior| prior.digest == asset.digest)
            {
                return Err(CapabilityVisualCatalogError::Duplicate(asset.digest));
            }
            if !package
                .visuals
                .iter()
                .any(|visual| visual.asset_digest == asset.digest)
            {
                return Err(CapabilityVisualCatalogError::Undeclared(asset.digest));
            }
            let calculated = sha256(asset.bytes).digest;
            if calculated != asset.digest {
                return Err(CapabilityVisualCatalogError::Digest {
                    declared: asset.digest,
                    calculated,
                });
            }
        }
        for visual in package.visuals {
            if !assets
                .iter()
                .any(|asset| asset.digest == visual.asset_digest)
            {
                return Err(CapabilityVisualCatalogError::Missing(visual.asset_digest));
            }
        }
        Ok(Self {
            capability_digest: capability.digest,
            assets,
        })
    }

    fn find(self, digest: Digest) -> Option<CapabilityVisualAsset<'a>> {
        self.assets
            .iter()
            .copied()
            .find(|asset| asset.digest == digest)
    }
}

/// Static visual-catalog composition failure detected before range serving.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityVisualCatalogError {
    /// The declaring board package did not match its canonical identity.
    Capability(CapabilityError),
    /// An asset had a zero identity or empty byte string.
    Incomplete,
    /// An asset byte length did not fit the canonical wire identity.
    Length,
    /// Two catalog entries used one digest.
    Duplicate(Digest),
    /// A catalog entry was absent from the capability document.
    Undeclared(Digest),
    /// A capability visual had no immutable byte entry.
    Missing(Digest),
    /// Immutable bytes did not match their declared SHA-256.
    Digest {
        /// Digest declared by the canonical package and catalog.
        declared: Digest,
        /// Digest calculated over the complete immutable bytes.
        calculated: Digest,
    },
}

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
        let visual_assets = VerifiedCapabilityVisualAssets {
            capability_digest: package.board.capability_digest,
            assets: &[],
        };
        Self::dispatch_with_visual_assets(package, visual_assets, request, now)
    }

    /// Returns one verified capability or visual range from an explicit,
    /// immutable asset catalog.
    pub fn dispatch_with_visual_assets(
        package: &BoardPackage<'_>,
        visual_assets: VerifiedCapabilityVisualAssets<'_>,
        request: &ServiceRequest,
        now: DeviceCycle,
    ) -> ServiceResponse {
        let Ok(native) = NativeRequest::decode(request.bytes()) else {
            return ServiceResponse::invalid_native();
        };
        if native.frame.kind != FrameKind::Capabilities {
            return ServiceResponse::invalid_native();
        }
        if !native.frame.config_digest.is_zero() {
            return respond(native, now, StatusCode::InvalidRequest, &[]);
        }
        match native.message.operation {
            Operation::CapabilitiesGet => dispatch_capability(package, native, now),
            Operation::CapabilityVisualGet => dispatch_visual(package, visual_assets, native, now),
            _ => respond(native, now, StatusCode::InvalidRequest, &[]),
        }
    }
}

fn dispatch_capability(
    package: &BoardPackage<'_>,
    native: NativeRequest<'_>,
    now: DeviceCycle,
) -> ServiceResponse {
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

fn dispatch_visual(
    package: &BoardPackage<'_>,
    visual_assets: VerifiedCapabilityVisualAssets<'_>,
    native: NativeRequest<'_>,
    now: DeviceCycle,
) -> ServiceResponse {
    let request = match VisualAssetReadRequest::decode(native.body) {
        Ok(request) => request,
        Err(_) => return respond(native, now, StatusCode::InvalidRequest, &[]),
    };
    if request.capability_digest != package.board.capability_digest {
        return respond(native, now, StatusCode::Conflict, &[]);
    }
    if visual_assets.capability_digest != package.board.capability_digest {
        return respond(native, now, StatusCode::Internal, &[]);
    }
    if package.visuals.is_empty() {
        return respond(native, now, StatusCode::Unsupported, &[]);
    }
    if !package
        .visuals
        .iter()
        .any(|visual| visual.asset_digest == request.asset_digest)
    {
        return respond(native, now, StatusCode::InvalidRequest, &[]);
    }
    let Some(asset) = visual_assets.find(request.asset_digest) else {
        return respond(native, now, StatusCode::Unsupported, &[]);
    };
    let Ok(asset_len) = u32::try_from(asset.bytes.len()) else {
        return respond(native, now, StatusCode::Internal, &[]);
    };
    let Ok(start) = usize::try_from(request.offset) else {
        return respond(native, now, StatusCode::InvalidRequest, &[]);
    };
    if start >= asset.bytes.len() {
        return respond(native, now, StatusCode::InvalidRequest, &[]);
    }
    let end = start
        .saturating_add(usize::from(request.maximum_bytes))
        .min(asset.bytes.len());
    let chunk = &asset.bytes[start..end];
    let Ok(chunk_len) = u16::try_from(chunk.len()) else {
        return respond(native, now, StatusCode::Internal, &[]);
    };
    let prefix = VisualAssetReadResponse {
        capability_digest: request.capability_digest,
        asset: VisualAssetIdentity {
            byte_len: asset_len,
            digest: asset.digest,
        },
        offset: request.offset,
        chunk_len,
        complete: end == asset.bytes.len(),
    };
    let Ok(prefix) = prefix.encode() else {
        return respond(native, now, StatusCode::Internal, &[]);
    };
    let mut body = [0_u8; MAX_VISUAL_ASSET_RESPONSE_BODY];
    body[..VISUAL_ASSET_READ_RESPONSE_PREFIX_BYTES].copy_from_slice(&prefix);
    body[VISUAL_ASSET_READ_RESPONSE_PREFIX_BYTES
        ..VISUAL_ASSET_READ_RESPONSE_PREFIX_BYTES + chunk.len()]
        .copy_from_slice(chunk);
    respond(
        native,
        now,
        StatusCode::Ok,
        &body[..VISUAL_ASSET_READ_RESPONSE_PREFIX_BYTES + chunk.len()],
    )
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
const _: () = assert!(
    MAX_VISUAL_ASSET_RESPONSE_BODY
        <= MAX_SERVICE_RESPONSE_BYTES - FrameHeader::WIRE_LEN - MessageHeader::WIRE_LEN
);
