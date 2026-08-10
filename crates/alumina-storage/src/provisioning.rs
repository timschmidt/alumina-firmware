//! Explicit, self-describing cache-region provisioning over raw block media.
//!
//! Blocks 2046 and 2047, immediately before the required 1 MiB raw-cache start,
//! hold alternating hashed locator records. This avoids both the primary
//! partition table and GPT backup-at-end convention. A locator is written only
//! after its exact cache region has been formatted and synchronized. Boot may
//! discover and mount a valid locator, but it never chooses a region or formats
//! arbitrary media. Reprovisioning binds the authenticated request to the
//! observed device size, generation, and media ID.

use crate::media::{
    AsyncBlockDevice, CacheMedia, MAX_MEDIA_CHUNK_BYTES, MEDIA_BLOCK_BYTES, MediaAvailability,
    MediaError, MediaId, MediaRegion, MediaStatus, PublishedChunk, PublishedReader,
};
use crate::{
    CacheLimits, ChunkUploadHeader, Error as StorageError, FinalizeUploadRequest, MutationContext,
    PublishedObject, UploadPlan, UploadProgress, sha256,
};

/// Number of fixed blocks reserved for redundant provisioning locators.
pub const CACHE_LOCATOR_BLOCKS: u64 = 2;
/// First raw block allowed for a V1 cache region, preserving a 1 MiB front guard.
pub const MINIMUM_CACHE_START_BLOCK: u64 = 2_048;
/// First fixed locator block immediately below the cache-start guard.
pub const CACHE_LOCATOR_FIRST_BLOCK: u64 = MINIMUM_CACHE_START_BLOCK - CACHE_LOCATOR_BLOCKS;
/// Exact cache-locator schema emitted by this implementation.
pub const CACHE_LOCATOR_VERSION: u16 = 1;
/// Exact canonical destructive provisioning request body length.
pub const CACHE_PROVISION_REQUEST_WIRE_BYTES: usize = 112;

const LOCATOR_MAGIC: [u8; 8] = *b"ALMLOC01";
const LOCATOR_FOOTER_MAGIC: [u8; 8] = *b"ALMLOCF1";
const PROVISION_MAGIC: [u8; 8] = *b"ALMPRV01";
const LOCATOR_HASH_OFFSET: usize = MEDIA_BLOCK_BYTES - 32;
const LOCATOR_FOOTER_OFFSET: usize = LOCATOR_HASH_OFFSET - 40;
const PROVISION_PREFIX_BYTES: usize = CACHE_PROVISION_REQUEST_WIRE_BYTES - 32;
const FLAG_DESTRUCTIVE_FORMAT: u16 = 1 << 0;
const FLAG_RECOVER_UNTRUSTED_LOCATOR: u16 = 1 << 1;
const KNOWN_PROVISION_FLAGS: u16 = FLAG_DESTRUCTIVE_FORMAT | FLAG_RECOVER_UNTRUSTED_LOCATOR;

/// Exact destructive intent sent by an authenticated controller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheProvisionRequest {
    /// Capacity observed by the UI immediately before confirmation.
    pub expected_device_blocks: u64,
    /// Trusted locator generation, or zero only when no locator is trusted.
    pub expected_locator_generation: u64,
    /// Trusted current media ID, absent exactly when generation is zero.
    pub expected_current_media_id: Option<MediaId>,
    /// Exact raw interval authorized for destructive cache formatting.
    pub region: MediaRegion,
    /// Fresh nonzero identity for the newly formatted cache generation.
    pub new_media_id: MediaId,
    /// Permits replacing damaged Alumina locator bytes when no locator is trusted.
    pub recover_untrusted_locator: bool,
}

impl CacheProvisionRequest {
    /// Constructs and validates a request before its canonical digest is encoded.
    pub fn new(
        expected_device_blocks: u64,
        expected_locator_generation: u64,
        expected_current_media_id: Option<MediaId>,
        region: MediaRegion,
        new_media_id: MediaId,
        recover_untrusted_locator: bool,
    ) -> Result<Self, CacheProvisionRequestError> {
        let request = Self {
            expected_device_blocks,
            expected_locator_generation,
            expected_current_media_id,
            region,
            new_media_id,
            recover_untrusted_locator,
        };
        request.validate()?;
        Ok(request)
    }

    /// Encodes all fields plus a SHA-256 destructive-confirmation digest.
    pub fn encode(self) -> [u8; CACHE_PROVISION_REQUEST_WIRE_BYTES] {
        let mut encoded = [0_u8; CACHE_PROVISION_REQUEST_WIRE_BYTES];
        self.encode_prefix(&mut encoded[..PROVISION_PREFIX_BYTES]);
        let confirmation = sha256(&encoded[..PROVISION_PREFIX_BYTES]);
        encoded[PROVISION_PREFIX_BYTES..].copy_from_slice(&confirmation.digest.0);
        encoded
    }

    /// Decodes only the exact canonical representation and verifies confirmation.
    pub fn decode(bytes: &[u8]) -> Result<Self, CacheProvisionRequestError> {
        if bytes.len() != CACHE_PROVISION_REQUEST_WIRE_BYTES {
            return Err(CacheProvisionRequestError::WireLength);
        }
        if bytes[0..8] != PROVISION_MAGIC {
            return Err(CacheProvisionRequestError::Magic);
        }
        if read_u16(bytes, 8) != CACHE_LOCATOR_VERSION {
            return Err(CacheProvisionRequestError::Version);
        }
        let flags = read_u16(bytes, 10);
        if flags & FLAG_DESTRUCTIVE_FORMAT == 0
            || flags & !KNOWN_PROVISION_FLAGS != 0
            || bytes[12..16].iter().any(|byte| *byte != 0)
        {
            return Err(CacheProvisionRequestError::Flags);
        }
        let confirmation = sha256(&bytes[..PROVISION_PREFIX_BYTES]);
        if bytes[PROVISION_PREFIX_BYTES..] != confirmation.digest.0 {
            return Err(CacheProvisionRequestError::Confirmation);
        }
        let expected_generation = read_u64(bytes, 24);
        let expected_id = read_media_id_or_none(bytes, 32)?;
        let mut new_media_id = [0_u8; 16];
        new_media_id.copy_from_slice(&bytes[64..80]);
        let request = Self {
            expected_device_blocks: read_u64(bytes, 16),
            expected_locator_generation: expected_generation,
            expected_current_media_id: expected_id,
            region: MediaRegion {
                start_block: read_u64(bytes, 48),
                block_count: read_u64(bytes, 56),
            },
            new_media_id: MediaId::new(new_media_id)
                .map_err(|_| CacheProvisionRequestError::NewMediaId)?,
            recover_untrusted_locator: flags & FLAG_RECOVER_UNTRUSTED_LOCATOR != 0,
        };
        request.validate()?;
        if request.encode() != bytes {
            return Err(CacheProvisionRequestError::Noncanonical);
        }
        Ok(request)
    }

    fn validate(self) -> Result<(), CacheProvisionRequestError> {
        if self.expected_device_blocks == 0 {
            return Err(CacheProvisionRequestError::DeviceBlocks);
        }
        MediaId::new(self.new_media_id.0).map_err(|_| CacheProvisionRequestError::NewMediaId)?;
        if let Some(current) = self.expected_current_media_id {
            MediaId::new(current.0).map_err(|_| CacheProvisionRequestError::ExpectedIdentity)?;
        }
        match (
            self.expected_locator_generation,
            self.expected_current_media_id,
        ) {
            (0, None) => {}
            (0, Some(_)) | (_, None) => {
                return Err(CacheProvisionRequestError::ExpectedIdentity);
            }
            (_, Some(current)) if current == self.new_media_id => {
                return Err(CacheProvisionRequestError::NewMediaId);
            }
            (_, Some(_)) => {}
        }
        validate_region(self.expected_device_blocks, self.region)
            .map_err(|_| CacheProvisionRequestError::Region)
    }

    fn encode_prefix(self, encoded: &mut [u8]) {
        encoded.fill(0);
        encoded[0..8].copy_from_slice(&PROVISION_MAGIC);
        encoded[8..10].copy_from_slice(&CACHE_LOCATOR_VERSION.to_le_bytes());
        let mut flags = FLAG_DESTRUCTIVE_FORMAT;
        if self.recover_untrusted_locator {
            flags |= FLAG_RECOVER_UNTRUSTED_LOCATOR;
        }
        encoded[10..12].copy_from_slice(&flags.to_le_bytes());
        encoded[16..24].copy_from_slice(&self.expected_device_blocks.to_le_bytes());
        encoded[24..32].copy_from_slice(&self.expected_locator_generation.to_le_bytes());
        if let Some(media_id) = self.expected_current_media_id {
            encoded[32..48].copy_from_slice(&media_id.0);
        }
        encoded[48..56].copy_from_slice(&self.region.start_block.to_le_bytes());
        encoded[56..64].copy_from_slice(&self.region.block_count.to_le_bytes());
        encoded[64..80].copy_from_slice(&self.new_media_id.0);
    }
}

/// Canonical request rejection before any device mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheProvisionRequestError {
    /// Body was not exactly the fixed V1 length.
    WireLength,
    /// Destructive operation magic was absent.
    Magic,
    /// Schema version is unsupported.
    Version,
    /// Required intent flag, reserved bytes, or flag set was invalid.
    Flags,
    /// Canonical confirmation digest did not match the exact request fields.
    Confirmation,
    /// Expected generation and current identity were inconsistent.
    ExpectedIdentity,
    /// New identity was zero or reused the current identity.
    NewMediaId,
    /// Expected physical capacity was zero.
    DeviceBlocks,
    /// Region violated front-guard, capacity, or locator exclusion policy.
    Region,
    /// A valid but noncanonical byte representation was supplied.
    Noncanonical,
}

/// Stable reason a provisioned-cache manager is not usable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ProvisioningFault {
    /// No current fault.
    None = 0,
    /// Card transport or synchronization failed.
    Device = 1,
    /// Alumina locator bytes were damaged or mutually inconsistent.
    Locator = 2,
    /// Stored/requested geometry was impossible for this device.
    Geometry = 3,
    /// Locator selected a region with no complete cache format.
    Unformatted = 4,
    /// Cache anchors or committed records failed integrity checks.
    MediaIntegrity = 5,
    /// Cache anchor identity did not match the selected locator.
    MediaIdentity = 6,
    /// Cache policy or lifecycle state rejected the operation.
    Policy = 7,
}

impl ProvisioningFault {
    /// Stable human-readable status token.
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Device => "device",
            Self::Locator => "locator",
            Self::Geometry => "geometry",
            Self::Unformatted => "unformatted",
            Self::MediaIntegrity => "media-integrity",
            Self::MediaIdentity => "media-identity",
            Self::Policy => "policy",
        }
    }
}

/// Portable lifecycle independent of the HTTP/service representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProvisionedCacheAvailability {
    /// Card was identified but contains no trusted locator.
    Detached,
    /// Locator and complete cache log were mounted and cross-checked.
    Ready,
    /// Discovery or media operation failed closed.
    Faulted,
}

/// Allocation-free facts exposed by the manager.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProvisionedCacheStatus {
    /// Current mount/provisioning state.
    pub availability: ProvisionedCacheAvailability,
    /// Stable coarse fault reason.
    pub fault: ProvisioningFault,
    /// Exact physical capacity reported by the card adapter.
    pub device_blocks: u64,
    /// Trusted locator generation, zero when absent/untrusted.
    pub locator_generation: u64,
    /// Trusted raw region, if a valid locator was decoded.
    pub region: Option<MediaRegion>,
    /// Trusted media identity from the locator.
    pub media_id: Option<MediaId>,
    /// One expected locator generation was damaged or absent.
    pub degraded_locator: bool,
    /// Mounted cache-log status, only when complete replay succeeded.
    pub media: Option<MediaStatus>,
}

/// Discovery/provisioning failure retaining concrete device errors.
#[derive(Debug)]
pub enum ProvisionedCacheError<E> {
    /// Underlying block adapter failed.
    Device(E),
    /// Locator structure, hash, generation, or pairing was invalid.
    Locator(LocatorError),
    /// Region or device geometry was invalid.
    Geometry,
    /// Cache media mount or mutation failed.
    Media(MediaError<E>),
    /// Locator and mounted cache used different media identities.
    MediaIdentity,
    /// Provisioning was attempted before discovery established current state.
    NotDiscovered,
    /// The retained transport is already faulted and requires reinitialization.
    DeviceStateUnavailable,
    /// Request expected facts did not exactly equal discovered facts.
    Conflict,
    /// Untrusted locator recovery flag was absent or used unexpectedly.
    RecoveryIntent,
    /// Mutation was forbidden by safety/real-time state.
    Mutation(StorageError),
    /// Generation arithmetic overflowed.
    Arithmetic,
    /// Upload operation requires a ready mounted cache.
    NotMounted,
}

/// Locator-specific structural failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocatorError {
    /// An Alumina locator had invalid fields, padding, or digest.
    Corrupt,
    /// Two valid slots did not form adjacent alternating generations.
    Conflicting,
}

#[derive(Clone, Copy)]
struct Locator {
    slot: u8,
    generation: u64,
    device_blocks: u64,
    region: MediaRegion,
    media_id: MediaId,
    limits: CacheLimits,
}

#[derive(Clone, Copy)]
enum LocatorBlock {
    Absent,
    Valid(Locator),
    Corrupt,
}

#[derive(Clone, Copy)]
enum ManagerState {
    Detached,
    Ready,
    Faulted(ProvisioningFault),
}

/// Sole-owner state machine joining physical media, locator, and cache log.
pub struct ProvisionedCache<D> {
    device: Option<D>,
    media: Option<CacheMedia<D>>,
    limits: CacheLimits,
    state: ManagerState,
    locator: Option<Locator>,
    degraded_locator: bool,
    discovered: bool,
}

impl<D> ProvisionedCache<D>
where
    D: AsyncBlockDevice,
{
    /// Creates an unscanned manager. `discover` must complete before mutation.
    pub fn new(device: D, limits: CacheLimits) -> Self {
        Self {
            device: Some(device),
            media: None,
            limits,
            state: ManagerState::Detached,
            locator: None,
            degraded_locator: false,
            discovered: false,
        }
    }

    /// Retains an adapter whose card identification already failed.
    pub fn transport_faulted(device: D, limits: CacheLimits) -> Self {
        Self {
            device: Some(device),
            media: None,
            limits,
            state: ManagerState::Faulted(ProvisioningFault::Device),
            locator: None,
            degraded_locator: false,
            discovered: true,
        }
    }

    /// Reads only the two fixed provisioning locators and a selected cache region.
    pub async fn discover(
        &mut self,
    ) -> Result<ProvisionedCacheStatus, ProvisionedCacheError<D::Error>> {
        self.release_media();
        self.locator = None;
        self.degraded_locator = false;
        self.discovered = true;
        let device_blocks = self.device_block_count();
        if locator_blocks(device_blocks).is_none() {
            self.state = ManagerState::Faulted(ProvisioningFault::Geometry);
            return Err(ProvisionedCacheError::Geometry);
        }
        let (first_address, second_address) = locator_blocks(device_blocks).expect("checked");
        let mut first = [0_u8; MEDIA_BLOCK_BYTES];
        let mut second = [0_u8; MEDIA_BLOCK_BYTES];
        if let Err(error) = self.read_device(first_address, &mut first).await {
            self.state = ManagerState::Faulted(ProvisioningFault::Device);
            return Err(ProvisionedCacheError::Device(error));
        }
        if let Err(error) = self.read_device(second_address, &mut second).await {
            self.state = ManagerState::Faulted(ProvisioningFault::Device);
            return Err(ProvisionedCacheError::Device(error));
        }
        let selected = select_locator(
            decode_locator(&first, 0, device_blocks, self.limits),
            decode_locator(&second, 1, device_blocks, self.limits),
        );
        let (locator, degraded) = match selected {
            Ok(selected) => selected,
            Err(error) => {
                self.state = ManagerState::Faulted(ProvisioningFault::Locator);
                return Err(ProvisionedCacheError::Locator(error));
            }
        };
        let Some(locator) = locator else {
            self.state = ManagerState::Detached;
            return Ok(self.status());
        };
        self.locator = Some(locator);
        self.degraded_locator = degraded;
        self.mount_selected(locator).await?;
        Ok(self.status())
    }

    /// Formats exactly the authenticated interval and then commits its locator.
    pub async fn provision(
        &mut self,
        request: CacheProvisionRequest,
        context: MutationContext,
    ) -> Result<ProvisionedCacheStatus, ProvisionedCacheError<D::Error>> {
        context
            .validate()
            .map_err(ProvisionedCacheError::Mutation)?;
        if !self.discovered {
            return Err(ProvisionedCacheError::NotDiscovered);
        }
        request
            .validate()
            .map_err(|_| ProvisionedCacheError::Geometry)?;
        let device_blocks = self.device_block_count();
        if request.expected_device_blocks != device_blocks {
            return Err(ProvisionedCacheError::Conflict);
        }
        let trusted = self.locator;
        match trusted {
            Some(locator) => {
                if request.recover_untrusted_locator {
                    return Err(ProvisionedCacheError::RecoveryIntent);
                }
                if request.expected_locator_generation != locator.generation
                    || request.expected_current_media_id != Some(locator.media_id)
                {
                    return Err(ProvisionedCacheError::Conflict);
                }
            }
            None => {
                if request.expected_locator_generation != 0
                    || request.expected_current_media_id.is_some()
                {
                    return Err(ProvisionedCacheError::Conflict);
                }
                let locator_fault = matches!(
                    self.state,
                    ManagerState::Faulted(ProvisioningFault::Locator)
                );
                if request.recover_untrusted_locator != locator_fault {
                    return Err(ProvisionedCacheError::RecoveryIntent);
                }
                if matches!(self.state, ManagerState::Faulted(ProvisioningFault::Device)) {
                    return Err(ProvisionedCacheError::DeviceStateUnavailable);
                }
            }
        }
        validate_region(device_blocks, request.region)
            .map_err(|_| ProvisionedCacheError::Geometry)?;
        let next_generation = request
            .expected_locator_generation
            .checked_add(1)
            .ok_or(ProvisionedCacheError::Arithmetic)?;
        let slot = u8::try_from((next_generation - 1) & 1).expect("locator slot fits u8");
        let next = Locator {
            slot,
            generation: next_generation,
            device_blocks,
            region: request.region,
            media_id: request.new_media_id,
            limits: self.limits,
        };

        self.release_media();
        self.state = ManagerState::Faulted(ProvisioningFault::Policy);
        let device = self.device.take().expect("manager always owns one device");
        self.media = Some(CacheMedia::new(device, request.region, self.limits));
        let format_result = self
            .media
            .as_mut()
            .expect("transitional media retains the device")
            .format(request.new_media_id)
            .await;
        if let Err(error) = format_result {
            self.release_media();
            self.state = ManagerState::Faulted(fault_from_media_error(&error));
            return Err(ProvisionedCacheError::Media(error));
        }
        self.release_media();

        if trusted.is_none() {
            let blank = [0_u8; MEDIA_BLOCK_BYTES];
            let (first, second) = locator_blocks(device_blocks).expect("validated geometry");
            self.write_and_fault(first, &blank).await?;
            self.write_and_fault(second, &blank).await?;
            self.sync_and_fault().await?;
        }
        let (first, second) = locator_blocks(device_blocks).expect("validated geometry");
        let address = if slot == 0 { first } else { second };
        let encoded = encode_locator(next);
        self.write_and_fault(address, &encoded).await?;
        self.sync_and_fault().await?;

        let committed = if trusted.is_none() {
            let generation = next_generation
                .checked_add(1)
                .ok_or(ProvisionedCacheError::Arithmetic)?;
            let redundant = Locator {
                slot: u8::try_from((generation - 1) & 1).expect("locator slot fits u8"),
                generation,
                ..next
            };
            self.write_and_fault(second, &encode_locator(redundant))
                .await?;
            self.sync_and_fault().await?;
            redundant
        } else {
            next
        };

        self.locator = Some(committed);
        self.degraded_locator = false;
        self.mount_selected(committed).await?;
        Ok(self.status())
    }

    /// Current state without I/O.
    pub fn status(&self) -> ProvisionedCacheStatus {
        let device_blocks = self.device_block_count();
        let media = self.media.as_ref().map(CacheMedia::status);
        let (availability, fault) = match (self.state, media) {
            (ManagerState::Faulted(fault), _) => (ProvisionedCacheAvailability::Faulted, fault),
            (_, Some(status)) if status.availability == MediaAvailability::Faulted => (
                ProvisionedCacheAvailability::Faulted,
                ProvisioningFault::Device,
            ),
            (ManagerState::Detached, _) => (
                ProvisionedCacheAvailability::Detached,
                ProvisioningFault::None,
            ),
            (ManagerState::Ready, _) => {
                (ProvisionedCacheAvailability::Ready, ProvisioningFault::None)
            }
        };
        ProvisionedCacheStatus {
            availability,
            fault,
            device_blocks,
            locator_generation: self.locator.map_or(0, |locator| locator.generation),
            region: self.locator.map(|locator| locator.region),
            media_id: self.locator.map(|locator| locator.media_id),
            degraded_locator: self.degraded_locator,
            media,
        }
    }

    /// Begins/resumes an upload only after locator and cache replay succeeded.
    pub async fn begin_upload(
        &mut self,
        plan: UploadPlan,
        context: MutationContext,
    ) -> Result<UploadProgress, ProvisionedCacheError<D::Error>> {
        self.media
            .as_mut()
            .ok_or(ProvisionedCacheError::NotMounted)?
            .begin_upload(plan, context)
            .await
            .map_err(ProvisionedCacheError::Media)
    }

    /// Commits the next exact upload chunk to mounted cache media.
    pub async fn put_chunk(
        &mut self,
        header: ChunkUploadHeader,
        bytes: &[u8],
        context: MutationContext,
    ) -> Result<UploadProgress, ProvisionedCacheError<D::Error>> {
        self.media
            .as_mut()
            .ok_or(ProvisionedCacheError::NotMounted)?
            .put_chunk(header, bytes, context)
            .await
            .map_err(ProvisionedCacheError::Media)
    }

    /// Atomically publishes a completely verified object.
    pub async fn finalize_upload(
        &mut self,
        request: FinalizeUploadRequest,
        context: MutationContext,
    ) -> Result<PublishedObject, ProvisionedCacheError<D::Error>> {
        self.media
            .as_mut()
            .ok_or(ProvisionedCacheError::NotMounted)?
            .finalize_upload(request, context)
            .await
            .map_err(ProvisionedCacheError::Media)
    }

    /// Opens the newest exact typed publication through the mounted cache log.
    pub async fn open_published(
        &mut self,
        expected: PublishedObject,
    ) -> Result<PublishedReader, ProvisionedCacheError<D::Error>> {
        let result = self
            .media
            .as_mut()
            .ok_or(ProvisionedCacheError::NotMounted)?
            .open_published(expected)
            .await;
        if let Err(error @ (MediaError::Device(_) | MediaError::Corrupt(_))) = &result {
            self.state = ManagerState::Faulted(fault_from_media_error(error));
        }
        result.map_err(ProvisionedCacheError::Media)
    }

    /// Reads one verified immutable chunk into fixed caller-owned memory.
    pub async fn read_next_published(
        &mut self,
        reader: &mut PublishedReader,
        output: &mut [u8; MAX_MEDIA_CHUNK_BYTES],
    ) -> Result<Option<PublishedChunk>, ProvisionedCacheError<D::Error>> {
        let result = self
            .media
            .as_mut()
            .ok_or(ProvisionedCacheError::NotMounted)?
            .read_next_published(reader, output)
            .await;
        if let Err(error @ (MediaError::Device(_) | MediaError::Corrupt(_))) = &result {
            self.state = ManagerState::Faulted(fault_from_media_error(error));
        }
        result.map_err(ProvisionedCacheError::Media)
    }

    /// Returns the retained physical adapter, including after any failure.
    pub fn into_device(mut self) -> D {
        self.release_media();
        self.device.take().expect("manager always owns one device")
    }

    async fn mount_selected(
        &mut self,
        locator: Locator,
    ) -> Result<(), ProvisionedCacheError<D::Error>> {
        let device = self.device.take().expect("manager always owns one device");
        self.media = Some(CacheMedia::new(device, locator.region, self.limits));
        self.state = ManagerState::Faulted(ProvisioningFault::Policy);
        let mount_result = self
            .media
            .as_mut()
            .expect("transitional media retains the device")
            .mount()
            .await;
        match mount_result {
            Ok(status) if status.media_id == Some(locator.media_id) => {
                self.state = ManagerState::Ready;
                Ok(())
            }
            Ok(_) => {
                self.release_media();
                self.state = ManagerState::Faulted(ProvisioningFault::MediaIdentity);
                Err(ProvisionedCacheError::MediaIdentity)
            }
            Err(error) => {
                self.release_media();
                self.state = ManagerState::Faulted(fault_from_media_error(&error));
                Err(ProvisionedCacheError::Media(error))
            }
        }
    }

    fn release_media(&mut self) {
        if let Some(media) = self.media.take() {
            debug_assert!(self.device.is_none());
            self.device = Some(media.into_device());
        }
    }

    fn device_block_count(&self) -> u64 {
        if let Some(device) = &self.device {
            device.block_count()
        } else {
            self.media
                .as_ref()
                .expect("manager always owns one device")
                .device_block_count()
        }
    }

    async fn read_device(
        &mut self,
        block: u64,
        output: &mut [u8; MEDIA_BLOCK_BYTES],
    ) -> Result<(), D::Error> {
        self.device
            .as_mut()
            .expect("direct I/O requires released media")
            .read_block(block, output)
            .await
    }

    async fn write_and_fault(
        &mut self,
        block: u64,
        bytes: &[u8; MEDIA_BLOCK_BYTES],
    ) -> Result<(), ProvisionedCacheError<D::Error>> {
        match self
            .device
            .as_mut()
            .expect("direct I/O requires released media")
            .write_block(block, bytes)
            .await
        {
            Ok(()) => Ok(()),
            Err(error) => {
                self.state = ManagerState::Faulted(ProvisioningFault::Device);
                Err(ProvisionedCacheError::Device(error))
            }
        }
    }

    async fn sync_and_fault(&mut self) -> Result<(), ProvisionedCacheError<D::Error>> {
        match self
            .device
            .as_mut()
            .expect("direct I/O requires released media")
            .sync()
            .await
        {
            Ok(()) => Ok(()),
            Err(error) => {
                self.state = ManagerState::Faulted(ProvisioningFault::Device);
                Err(ProvisionedCacheError::Device(error))
            }
        }
    }
}

fn validate_region(device_blocks: u64, region: MediaRegion) -> Result<(), ()> {
    region.validate(device_blocks).map_err(|_| ())?;
    if region.start_block < MINIMUM_CACHE_START_BLOCK {
        return Err(());
    }
    Ok(())
}

fn locator_blocks(device_blocks: u64) -> Option<(u64, u64)> {
    if device_blocks <= MINIMUM_CACHE_START_BLOCK {
        return None;
    }
    Some((CACHE_LOCATOR_FIRST_BLOCK, CACHE_LOCATOR_FIRST_BLOCK + 1))
}

fn encode_locator(locator: Locator) -> [u8; MEDIA_BLOCK_BYTES] {
    let mut block = [0_u8; MEDIA_BLOCK_BYTES];
    block[0..8].copy_from_slice(&LOCATOR_MAGIC);
    block[8..10].copy_from_slice(&CACHE_LOCATOR_VERSION.to_le_bytes());
    block[10] = locator.slot;
    block[12..16].copy_from_slice(
        &u32::try_from(MEDIA_BLOCK_BYTES)
            .expect("media block bytes fit u32")
            .to_le_bytes(),
    );
    block[16..24].copy_from_slice(&locator.generation.to_le_bytes());
    block[24..32].copy_from_slice(&locator.device_blocks.to_le_bytes());
    block[32..40].copy_from_slice(&locator.region.start_block.to_le_bytes());
    block[40..48].copy_from_slice(&locator.region.block_count.to_le_bytes());
    block[48..64].copy_from_slice(&locator.media_id.0);
    block[64..72].copy_from_slice(&locator.limits.maximum_object_bytes.to_le_bytes());
    block[72..76].copy_from_slice(&locator.limits.maximum_chunk_bytes.to_le_bytes());
    block[76..80].copy_from_slice(&locator.limits.maximum_chunks.to_le_bytes());
    block[LOCATOR_FOOTER_OFFSET..LOCATOR_FOOTER_OFFSET + 8].copy_from_slice(&LOCATOR_FOOTER_MAGIC);
    let digest = sha256(&block[..LOCATOR_HASH_OFFSET]);
    block[LOCATOR_HASH_OFFSET..].copy_from_slice(&digest.digest.0);
    block
}

fn decode_locator(
    block: &[u8; MEDIA_BLOCK_BYTES],
    expected_slot: u8,
    device_blocks: u64,
    limits: CacheLimits,
) -> LocatorBlock {
    if block[0..8] != LOCATOR_MAGIC
        && block[LOCATOR_FOOTER_OFFSET..LOCATOR_FOOTER_OFFSET + 8] != LOCATOR_FOOTER_MAGIC
    {
        return LocatorBlock::Absent;
    }
    if read_u16(block, 8) != CACHE_LOCATOR_VERSION
        || block[10] != expected_slot
        || block[11] != 0
        || read_u32(block, 12) != u32::try_from(MEDIA_BLOCK_BYTES).unwrap_or(u32::MAX)
        || block[80..LOCATOR_FOOTER_OFFSET]
            .iter()
            .any(|byte| *byte != 0)
        || block[LOCATOR_FOOTER_OFFSET..LOCATOR_FOOTER_OFFSET + 8] != LOCATOR_FOOTER_MAGIC
        || block[LOCATOR_FOOTER_OFFSET + 8..LOCATOR_HASH_OFFSET]
            .iter()
            .any(|byte| *byte != 0)
    {
        return LocatorBlock::Corrupt;
    }
    let expected_digest = sha256(&block[..LOCATOR_HASH_OFFSET]);
    if block[LOCATOR_HASH_OFFSET..] != expected_digest.digest.0 {
        return LocatorBlock::Corrupt;
    }
    let generation = read_u64(block, 16);
    if generation == 0 || u64::from(expected_slot) != (generation - 1) & 1 {
        return LocatorBlock::Corrupt;
    }
    let stored_device_blocks = read_u64(block, 24);
    let region = MediaRegion {
        start_block: read_u64(block, 32),
        block_count: read_u64(block, 40),
    };
    let mut media_id = [0_u8; 16];
    media_id.copy_from_slice(&block[48..64]);
    let Ok(media_id) = MediaId::new(media_id) else {
        return LocatorBlock::Corrupt;
    };
    let stored_limits = CacheLimits {
        maximum_object_bytes: read_u64(block, 64),
        maximum_chunk_bytes: read_u32(block, 72),
        maximum_chunks: read_u32(block, 76),
    };
    if stored_device_blocks != device_blocks
        || stored_limits != limits
        || validate_region(device_blocks, region).is_err()
    {
        return LocatorBlock::Corrupt;
    }
    LocatorBlock::Valid(Locator {
        slot: expected_slot,
        generation,
        device_blocks,
        region,
        media_id,
        limits,
    })
}

fn select_locator(
    first: LocatorBlock,
    second: LocatorBlock,
) -> Result<(Option<Locator>, bool), LocatorError> {
    match (first, second) {
        (LocatorBlock::Absent, LocatorBlock::Absent) => Ok((None, false)),
        (LocatorBlock::Corrupt, LocatorBlock::Absent)
        | (LocatorBlock::Absent, LocatorBlock::Corrupt)
        | (LocatorBlock::Corrupt, LocatorBlock::Corrupt) => Err(LocatorError::Corrupt),
        (LocatorBlock::Valid(locator), LocatorBlock::Absent)
        | (LocatorBlock::Absent, LocatorBlock::Valid(locator)) => Ok((Some(locator), true)),
        (LocatorBlock::Valid(locator), LocatorBlock::Corrupt)
        | (LocatorBlock::Corrupt, LocatorBlock::Valid(locator)) => Ok((Some(locator), true)),
        (LocatorBlock::Valid(first), LocatorBlock::Valid(second)) => {
            let (older, newer) = if first.generation < second.generation {
                (first, second)
            } else {
                (second, first)
            };
            if newer.generation.checked_sub(older.generation) != Some(1) {
                return Err(LocatorError::Conflicting);
            }
            Ok((Some(newer), false))
        }
    }
}

fn fault_from_media_error<E>(error: &MediaError<E>) -> ProvisioningFault {
    match error {
        MediaError::Device(_) | MediaError::Faulted => ProvisioningFault::Device,
        MediaError::Geometry(_) | MediaError::Arithmetic | MediaError::Full { .. } => {
            ProvisioningFault::Geometry
        }
        MediaError::Unformatted | MediaError::NotMounted => ProvisioningFault::Unformatted,
        MediaError::Corrupt(_) => ProvisioningFault::MediaIntegrity,
        MediaError::PublishedNotFound => ProvisioningFault::Policy,
        MediaError::Storage(_) => ProvisioningFault::Policy,
    }
}

fn read_media_id_or_none(
    bytes: &[u8],
    offset: usize,
) -> Result<Option<MediaId>, CacheProvisionRequestError> {
    let mut id = [0_u8; 16];
    id.copy_from_slice(&bytes[offset..offset + 16]);
    if id.iter().all(|byte| *byte == 0) {
        Ok(None)
    } else {
        MediaId::new(id)
            .map(Some)
            .map_err(|_| CacheProvisionRequestError::ExpectedIdentity)
    }
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
    reason = "the deterministic provisioning fault model is intentionally host-only"
)]
mod tests {
    extern crate std;

    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::vec;
    use std::vec::Vec;

    use embassy_futures::block_on;
    use embassy_futures::select::{Either, select};

    use super::*;
    use crate::{ManifestHasher, ObjectKind, StoredObject};

    const DEVICE_BLOCKS: usize = 2_300;
    const REGION: MediaRegion = MediaRegion {
        start_block: 2_048,
        block_count: 200,
    };
    const LIMITS: CacheLimits = CacheLimits {
        maximum_object_bytes: 1_024 * 1_024,
        maximum_chunk_bytes: 1_024,
        maximum_chunks: 1_024,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum DeviceError {
        InjectedCut,
        OutsideDevice,
    }

    #[derive(Clone)]
    struct DeviceControl {
        blocks: Rc<RefCell<Vec<[u8; MEDIA_BLOCK_BYTES]>>>,
        fail_at: Rc<Cell<Option<usize>>>,
        operations: Rc<Cell<usize>>,
    }

    impl DeviceControl {
        fn arm_relative(&self, relative: usize) {
            self.fail_at
                .set(Some(self.operations.get().saturating_add(relative)));
        }

        fn disarm(&self) {
            self.fail_at.set(None);
        }

        fn operations(&self) -> usize {
            self.operations.get()
        }

        fn snapshot(&self) -> Vec<[u8; MEDIA_BLOCK_BYTES]> {
            self.blocks.borrow().clone()
        }

        fn overwrite(&self, block: usize, bytes: [u8; MEDIA_BLOCK_BYTES]) {
            self.blocks.borrow_mut()[block] = bytes;
        }

        fn flip(&self, block: usize, byte: usize) {
            self.blocks.borrow_mut()[block][byte] ^= 0x80;
        }
    }

    struct RamBlockDevice {
        control: DeviceControl,
    }

    impl RamBlockDevice {
        fn erased() -> (Self, DeviceControl) {
            Self::from_snapshot(vec![[0xff; MEDIA_BLOCK_BYTES]; DEVICE_BLOCKS])
        }

        fn from_snapshot(blocks: Vec<[u8; MEDIA_BLOCK_BYTES]>) -> (Self, DeviceControl) {
            let control = DeviceControl {
                blocks: Rc::new(RefCell::new(blocks)),
                fail_at: Rc::new(Cell::new(None)),
                operations: Rc::new(Cell::new(0)),
            };
            (
                Self {
                    control: control.clone(),
                },
                control,
            )
        }

        fn before_mutation(&self) -> bool {
            let operation = self.control.operations.get();
            self.control.operations.set(operation.saturating_add(1));
            self.control.fail_at.get() == Some(operation)
        }
    }

    impl AsyncBlockDevice for RamBlockDevice {
        type Error = DeviceError;

        fn block_count(&self) -> u64 {
            u64::try_from(self.control.blocks.borrow().len()).unwrap()
        }

        async fn read_block(
            &mut self,
            block: u64,
            output: &mut [u8; MEDIA_BLOCK_BYTES],
        ) -> Result<(), Self::Error> {
            let index = usize::try_from(block).map_err(|_| DeviceError::OutsideDevice)?;
            let blocks = self.control.blocks.borrow();
            let source = blocks.get(index).ok_or(DeviceError::OutsideDevice)?;
            output.copy_from_slice(source);
            Ok(())
        }

        async fn write_block(
            &mut self,
            block: u64,
            data: &[u8; MEDIA_BLOCK_BYTES],
        ) -> Result<(), Self::Error> {
            let index = usize::try_from(block).map_err(|_| DeviceError::OutsideDevice)?;
            let mut blocks = self.control.blocks.borrow_mut();
            let target = blocks.get_mut(index).ok_or(DeviceError::OutsideDevice)?;
            if self.before_mutation() {
                target[..137].copy_from_slice(&data[..137]);
                return Err(DeviceError::InjectedCut);
            }
            target.copy_from_slice(data);
            Ok(())
        }

        async fn sync(&mut self) -> Result<(), Self::Error> {
            if self.before_mutation() {
                Err(DeviceError::InjectedCut)
            } else {
                Ok(())
            }
        }
    }

    struct PendingWriteDevice {
        marker: u8,
    }

    impl AsyncBlockDevice for PendingWriteDevice {
        type Error = DeviceError;

        fn block_count(&self) -> u64 {
            u64::try_from(DEVICE_BLOCKS).unwrap()
        }

        async fn read_block(
            &mut self,
            _block: u64,
            output: &mut [u8; MEDIA_BLOCK_BYTES],
        ) -> Result<(), Self::Error> {
            output.fill(0xff);
            Ok(())
        }

        async fn write_block(
            &mut self,
            _block: u64,
            _data: &[u8; MEDIA_BLOCK_BYTES],
        ) -> Result<(), Self::Error> {
            core::future::pending().await
        }

        async fn sync(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    fn media_id(byte: u8) -> MediaId {
        MediaId::new([byte; 16]).unwrap()
    }

    fn initial_request(id: MediaId, recover: bool) -> CacheProvisionRequest {
        CacheProvisionRequest::new(
            u64::try_from(DEVICE_BLOCKS).unwrap(),
            0,
            None,
            REGION,
            id,
            recover,
        )
        .unwrap()
    }

    fn plan(bytes: &[u8], chunk_bytes: usize) -> UploadPlan {
        let object = StoredObject {
            kind: ObjectKind::MachineJobPartition,
            content: sha256(bytes),
            byte_len: u64::try_from(bytes.len()).unwrap(),
        };
        let chunk_count = bytes.len().div_ceil(chunk_bytes);
        let mut manifest = ManifestHasher::new(
            object,
            u32::try_from(chunk_bytes).unwrap(),
            u32::try_from(chunk_count).unwrap(),
            LIMITS,
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
            upload_id: crate::UploadId(0x1234_5678_9abc_def0),
            object,
            manifest: manifest.finalize().unwrap(),
            chunk_bytes: u32::try_from(chunk_bytes).unwrap(),
            chunk_count: u32::try_from(chunk_count).unwrap(),
        }
    }

    fn upload(
        cache: &mut ProvisionedCache<RamBlockDevice>,
        bytes: &[u8],
        chunk_bytes: usize,
    ) -> PublishedObject {
        let plan = plan(bytes, chunk_bytes);
        block_on(cache.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        for (index, chunk) in bytes.chunks(chunk_bytes).enumerate() {
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

    #[test]
    fn canonical_request_binds_every_destructive_field() {
        let request = initial_request(media_id(0x5a), false);
        let encoded = request.encode();
        assert_eq!(CacheProvisionRequest::decode(&encoded), Ok(request));

        let mut tampered = encoded;
        tampered[56] ^= 1;
        assert_eq!(
            CacheProvisionRequest::decode(&tampered),
            Err(CacheProvisionRequestError::Confirmation)
        );
        let mut reserved = encoded;
        reserved[12] = 1;
        let confirmation = sha256(&reserved[..PROVISION_PREFIX_BYTES]);
        reserved[PROVISION_PREFIX_BYTES..].copy_from_slice(&confirmation.digest.0);
        assert_eq!(
            CacheProvisionRequest::decode(&reserved),
            Err(CacheProvisionRequestError::Flags)
        );
        assert_eq!(
            CacheProvisionRequest::new(
                u64::try_from(DEVICE_BLOCKS).unwrap(),
                1,
                None,
                REGION,
                media_id(0x5a),
                false,
            ),
            Err(CacheProvisionRequestError::ExpectedIdentity)
        );
        assert_eq!(
            CacheProvisionRequest::new(
                u64::try_from(DEVICE_BLOCKS).unwrap(),
                0,
                None,
                REGION,
                MediaId([0; 16]),
                false,
            ),
            Err(CacheProvisionRequestError::NewMediaId)
        );
    }

    #[test]
    fn discovery_of_foreign_media_is_read_only_and_detached() {
        let (device, control) = RamBlockDevice::erased();
        let foreign = [0x37; MEDIA_BLOCK_BYTES];
        control.overwrite(2_046, foreign);
        control.overwrite(2_047, foreign);
        let before = control.snapshot();
        let mut cache = ProvisionedCache::new(device, LIMITS);
        let status = block_on(cache.discover()).unwrap();
        assert_eq!(status.availability, ProvisionedCacheAvailability::Detached);
        assert_eq!(status.fault, ProvisioningFault::None);
        assert_eq!(status.locator_generation, 0);
        assert_eq!(control.operations(), 0);
        assert_eq!(control.snapshot(), before);
    }

    #[test]
    fn explicit_provision_preserves_every_unowned_block_and_remounts() {
        let (device, control) = RamBlockDevice::erased();
        let before = control.snapshot();
        let mut cache = ProvisionedCache::new(device, LIMITS);
        block_on(cache.discover()).unwrap();
        let status = block_on(cache.provision(
            initial_request(media_id(0x5a), false),
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap();
        assert_eq!(status.availability, ProvisionedCacheAvailability::Ready);
        assert_eq!(status.locator_generation, 2);
        assert_eq!(status.region, Some(REGION));
        assert_eq!(status.media_id, Some(media_id(0x5a)));
        assert_eq!(status.media.unwrap().free_blocks, REGION.block_count - 2);

        let after = control.snapshot();
        assert_eq!(&after[..2_046], &before[..2_046]);
        assert_eq!(&after[2_248..], &before[2_248..]);

        let device = cache.into_device();
        let mut rebooted = ProvisionedCache::new(device, LIMITS);
        let status = block_on(rebooted.discover()).unwrap();
        assert_eq!(status.availability, ProvisionedCacheAvailability::Ready);
        assert_eq!(status.locator_generation, 2);
        assert!(!status.degraded_locator);
        assert_eq!(status.media.unwrap().media_id, Some(media_id(0x5a)));
    }

    #[test]
    fn provisioned_reader_survives_reboot_and_latches_integrity_faults() {
        let bytes = b"exact cached partition bytes";
        let (device, control) = RamBlockDevice::erased();
        let mut cache = ProvisionedCache::new(device, LIMITS);
        block_on(cache.discover()).unwrap();
        block_on(cache.provision(
            initial_request(media_id(0x5a), false),
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap();
        let published = upload(&mut cache, bytes, 8);

        let device = cache.into_device();
        let mut rebooted = ProvisionedCache::new(device, LIMITS);
        block_on(rebooted.discover()).unwrap();
        let wrong_kind = PublishedObject {
            object: StoredObject {
                kind: ObjectKind::OpaqueData,
                ..published.object
            },
            manifest: published.manifest,
        };
        assert!(matches!(
            block_on(rebooted.open_published(wrong_kind)),
            Err(ProvisionedCacheError::Media(MediaError::PublishedNotFound))
        ));
        assert_eq!(
            rebooted.status().availability,
            ProvisionedCacheAvailability::Ready
        );

        let mut reader = block_on(rebooted.open_published(published)).unwrap();
        let mut output = [0_u8; MAX_MEDIA_CHUNK_BYTES];
        let mut collected = Vec::new();
        while let Some(chunk) =
            block_on(rebooted.read_next_published(&mut reader, &mut output)).unwrap()
        {
            collected.extend_from_slice(&output[..usize::try_from(chunk.byte_len).unwrap()]);
        }
        assert_eq!(collected, bytes);

        let mut corrupted = block_on(rebooted.open_published(published)).unwrap();
        // Begin occupies relative 2..5; first chunk data begins at relative 6.
        control.flip(
            usize::try_from(REGION.start_block + 6).unwrap(),
            ChunkUploadHeader::WIRE_LEN + 1,
        );
        assert!(matches!(
            block_on(rebooted.read_next_published(&mut corrupted, &mut output)),
            Err(ProvisionedCacheError::Media(MediaError::Corrupt(_)))
        ));
        let status = rebooted.status();
        assert_eq!(status.availability, ProvisionedCacheAvailability::Faulted);
        assert_eq!(status.fault, ProvisioningFault::MediaIntegrity);
    }

    #[test]
    fn safety_and_stale_confirmation_reject_before_writes() {
        let (device, control) = RamBlockDevice::erased();
        let mut cache = ProvisionedCache::new(device, LIMITS);
        block_on(cache.discover()).unwrap();
        let request = initial_request(media_id(0x5a), false);
        assert!(matches!(
            block_on(cache.provision(
                request,
                MutationContext {
                    armed_or_energized: true,
                    realtime_job_active: false,
                },
            )),
            Err(ProvisionedCacheError::Mutation(
                StorageError::MutationForbidden
            ))
        ));
        assert_eq!(control.operations(), 0);

        block_on(cache.provision(request, MutationContext::DISARMED_IDLE)).unwrap();
        let before = control.operations();
        assert!(matches!(
            block_on(cache.provision(request, MutationContext::DISARMED_IDLE)),
            Err(ProvisionedCacheError::Conflict)
        ));
        assert_eq!(control.operations(), before);
    }

    #[test]
    fn reprovision_requires_exact_old_identity_and_alternates_locator() {
        let (device, _) = RamBlockDevice::erased();
        let mut cache = ProvisionedCache::new(device, LIMITS);
        block_on(cache.discover()).unwrap();
        block_on(cache.provision(
            initial_request(media_id(0x5a), false),
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap();
        let replacement = CacheProvisionRequest::new(
            u64::try_from(DEVICE_BLOCKS).unwrap(),
            2,
            Some(media_id(0x5a)),
            REGION,
            media_id(0xa5),
            false,
        )
        .unwrap();
        let status =
            block_on(cache.provision(replacement, MutationContext::DISARMED_IDLE)).unwrap();
        assert_eq!(status.locator_generation, 3);
        assert_eq!(status.media_id, Some(media_id(0xa5)));

        let device = cache.into_device();
        let mut rebooted = ProvisionedCache::new(device, LIMITS);
        let status = block_on(rebooted.discover()).unwrap();
        assert_eq!(status.locator_generation, 3);
        assert_eq!(status.media_id, Some(media_id(0xa5)));
        assert!(!status.degraded_locator);
    }

    #[test]
    fn damaged_alumina_locator_requires_explicit_recovery_intent() {
        let (device, control) = RamBlockDevice::erased();
        let mut cache = ProvisionedCache::new(device, LIMITS);
        block_on(cache.discover()).unwrap();
        block_on(cache.provision(
            initial_request(media_id(0x5a), false),
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap();
        let device = cache.into_device();
        control.flip(2_046, 0);
        control.flip(2_047, LOCATOR_HASH_OFFSET);

        let mut damaged = ProvisionedCache::new(device, LIMITS);
        assert!(matches!(
            block_on(damaged.discover()),
            Err(ProvisionedCacheError::Locator(LocatorError::Corrupt))
        ));
        assert_eq!(damaged.status().fault, ProvisioningFault::Locator);
        assert!(matches!(
            block_on(damaged.provision(
                initial_request(media_id(0xa5), false),
                MutationContext::DISARMED_IDLE,
            )),
            Err(ProvisionedCacheError::RecoveryIntent)
        ));
        let status = block_on(damaged.provision(
            initial_request(media_id(0xa5), true),
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap();
        assert_eq!(status.availability, ProvisionedCacheAvailability::Ready);
        assert_eq!(status.locator_generation, 2);
        assert_eq!(status.media_id, Some(media_id(0xa5)));
    }

    #[test]
    fn locator_identity_must_match_formatted_region() {
        let (device, control) = RamBlockDevice::erased();
        let mut media = CacheMedia::new(device, REGION, LIMITS);
        block_on(media.format(media_id(0x5a))).unwrap();
        let mut device = media.into_device();
        let locator = Locator {
            slot: 0,
            generation: 1,
            device_blocks: u64::try_from(DEVICE_BLOCKS).unwrap(),
            region: REGION,
            media_id: media_id(0xa5),
            limits: LIMITS,
        };
        block_on(device.write_block(CACHE_LOCATOR_FIRST_BLOCK, &encode_locator(locator))).unwrap();
        block_on(device.sync()).unwrap();
        assert!(control.operations() > 0);

        let mut cache = ProvisionedCache::new(device, LIMITS);
        assert!(matches!(
            block_on(cache.discover()),
            Err(ProvisionedCacheError::MediaIdentity)
        ));
        let status = cache.status();
        assert_eq!(status.availability, ProvisionedCacheAvailability::Faulted);
        assert_eq!(status.fault, ProvisioningFault::MediaIdentity);
        assert_eq!(status.locator_generation, 1);
    }

    #[test]
    fn every_initial_provision_cut_is_ready_or_explicitly_recoverable() {
        let (_, baseline_control) = RamBlockDevice::erased();
        let baseline = baseline_control.snapshot();
        for cut in 0..=11 {
            let (device, control) = RamBlockDevice::from_snapshot(baseline.clone());
            let mut cache = ProvisionedCache::new(device, LIMITS);
            block_on(cache.discover()).unwrap();
            control.arm_relative(cut);
            let _ = block_on(cache.provision(
                initial_request(media_id(0x5a), false),
                MutationContext::DISARMED_IDLE,
            ));
            control.disarm();
            let snapshot = cache.into_device().control.snapshot();

            let (device, _) = RamBlockDevice::from_snapshot(snapshot);
            let mut rebooted = ProvisionedCache::new(device, LIMITS);
            let _ = block_on(rebooted.discover());
            let observed = rebooted.status();
            if observed.availability == ProvisionedCacheAvailability::Ready {
                assert_eq!(observed.media_id, Some(media_id(0x5a)));
                continue;
            }
            let replacement_id = if observed.media_id == Some(media_id(0x5a)) {
                media_id(0xa5)
            } else {
                media_id(0x5a)
            };
            let retry = CacheProvisionRequest::new(
                u64::try_from(DEVICE_BLOCKS).unwrap(),
                observed.locator_generation,
                observed.media_id,
                REGION,
                replacement_id,
                observed.fault == ProvisioningFault::Locator,
            )
            .unwrap();
            let recovered =
                block_on(rebooted.provision(retry, MutationContext::DISARMED_IDLE)).unwrap();
            assert_eq!(recovered.availability, ProvisionedCacheAvailability::Ready);
            assert_eq!(recovered.media_id, Some(replacement_id));
        }
    }

    #[test]
    fn cancelled_media_transition_retains_the_owned_device() {
        let mut cache = ProvisionedCache::new(PendingWriteDevice { marker: 0x5a }, LIMITS);
        block_on(cache.discover()).unwrap();
        let outcome = block_on(select(
            cache.provision(
                initial_request(media_id(0x5a), false),
                MutationContext::DISARMED_IDLE,
            ),
            core::future::ready(()),
        ));
        assert!(matches!(outcome, Either::Second(())));
        assert_eq!(
            cache.status().availability,
            ProvisionedCacheAvailability::Faulted
        );
        assert_eq!(cache.into_device().marker, 0x5a);
    }
}
