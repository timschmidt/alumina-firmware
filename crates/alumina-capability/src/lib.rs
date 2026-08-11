#![no_std]
#![doc = "Canonical byte-addressable board capability documents for Alumina."]

use alumina_board::{
    BoardError, BoardPackage, BusKind, Chip, ClockDomain, ClockSource, DeviceRoute,
    ElectricalConstraintKind, FlashRegionKind, HilKind, InterruptTrigger, OwnerDomain,
    Qualification, ResourceId, SafeValue, SupportLevel,
};
use alumina_protocol::Digest;
use sha2::{Digest as ShaDigest, Sha256};

/// Exact capability-document schema version.
pub const CAPABILITY_DOCUMENT_VERSION: u16 = 1;
/// Bytes in the fixed canonical document header.
pub const CAPABILITY_DOCUMENT_HEADER_BYTES: usize = 16;
/// Exact `CapabilitiesGet` range-request body length.
pub const CAPABILITY_READ_REQUEST_BYTES: usize = 56;
/// Exact response prefix before bounded document bytes.
pub const CAPABILITY_READ_RESPONSE_PREFIX_BYTES: usize = 64;
/// Largest capability range returned in one native response.
pub const MAX_CAPABILITY_CHUNK_BYTES: usize = 240;

const DOCUMENT_MAGIC: [u8; 8] = *b"ALMCAP01";
const REQUEST_MAGIC: [u8; 8] = *b"ALMCPQ01";
const RESPONSE_MAGIC: [u8; 8] = *b"ALMCPR01";
const RESPONSE_FLAG_COMPLETE: u8 = 1 << 0;

/// Exact identity of one canonical immutable capability document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityIdentity {
    /// Complete document length including its 16-byte header.
    pub byte_len: u32,
    /// SHA-256 over every document byte.
    pub digest: Digest,
}

/// Result of one caller-buffer range read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityRead {
    /// Verified document identity.
    pub identity: CapabilityIdentity,
    /// Requested byte offset.
    pub offset: u32,
    /// Bytes initialized in the caller buffer.
    pub byte_len: u16,
    /// True only when this range reaches the exact document end.
    pub complete: bool,
}

/// Canonical capability serialization, identity, or range rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityError {
    /// The board package failed its structural validation.
    Board(BoardError),
    /// A count, string, address, or total length exceeded V1 integer bounds.
    Length,
    /// The requested offset was beyond the exact document end.
    Range,
    /// The package still used the zero digest sentinel.
    MissingDeclaredDigest,
    /// The package's compiled digest did not match its canonical bytes.
    DeclaredDigestMismatch {
        /// Digest compiled into the board package.
        declared: Digest,
        /// Digest derived from the canonical document.
        calculated: Digest,
    },
}

/// Computes length and SHA-256 without allocating or trusting the declared digest.
pub fn calculate_identity(
    package: &BoardPackage<'_>,
) -> Result<CapabilityIdentity, CapabilityError> {
    // The declared digest is deliberately excluded from its own document. Use
    // a validation-only sentinel so a newly promoted armable package can
    // calculate its first identity before that identity is compiled back in.
    let mut structural = *package;
    if structural.armable && structural.board.capability_digest.is_zero() {
        structural.board.capability_digest = Digest([0xff; 32]);
    }
    structural.validate().map_err(CapabilityError::Board)?;
    let mut body_counter = CountingSink::default();
    encode_payload(package, &mut body_counter)?;
    let total = u32::try_from(CAPABILITY_DOCUMENT_HEADER_BYTES)
        .ok()
        .and_then(|header| header.checked_add(body_counter.byte_len))
        .ok_or(CapabilityError::Length)?;
    let header = document_header(total);
    let mut hasher = HashingSink::default();
    hasher.write(&header)?;
    encode_payload(package, &mut hasher)?;
    if hasher.byte_len != total {
        return Err(CapabilityError::Length);
    }
    Ok(CapabilityIdentity {
        byte_len: total,
        digest: hasher.finish(),
    })
}

/// Requires the package's compiled identity to match the canonical document.
pub fn verify_declared_identity(
    package: &BoardPackage<'_>,
) -> Result<CapabilityIdentity, CapabilityError> {
    let calculated = calculate_identity(package)?;
    let declared = package.board.capability_digest;
    if declared.is_zero() {
        return Err(CapabilityError::MissingDeclaredDigest);
    }
    if declared != calculated.digest {
        return Err(CapabilityError::DeclaredDigestMismatch {
            declared,
            calculated: calculated.digest,
        });
    }
    Ok(calculated)
}

/// Re-encodes only the requested verified range into caller-owned memory.
pub fn read_verified_range(
    package: &BoardPackage<'_>,
    offset: u32,
    output: &mut [u8],
) -> Result<CapabilityRead, CapabilityError> {
    let identity = verify_declared_identity(package)?;
    if offset > identity.byte_len {
        return Err(CapabilityError::Range);
    }
    let maximum = output.len().min(MAX_CAPABILITY_CHUNK_BYTES);
    let maximum = u32::try_from(maximum).map_err(|_| CapabilityError::Length)?;
    let remaining = identity.byte_len - offset;
    let requested = remaining.min(maximum);
    let requested_usize = usize::try_from(requested).map_err(|_| CapabilityError::Length)?;
    let mut sink = RangeSink::new(offset, &mut output[..requested_usize]);
    sink.write(&document_header(identity.byte_len))?;
    encode_payload(package, &mut sink)?;
    let written = sink.finish()?;
    if written != requested_usize {
        return Err(CapabilityError::Length);
    }
    let byte_len = u16::try_from(written).map_err(|_| CapabilityError::Length)?;
    Ok(CapabilityRead {
        identity,
        offset,
        byte_len,
        complete: offset
            .checked_add(u32::from(byte_len))
            .is_some_and(|end| end == identity.byte_len),
    })
}

/// Exact authenticated range request for one immutable capability identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityReadRequest {
    /// Zero for first discovery, otherwise the identity the caller already has.
    pub expected_digest: Digest,
    /// Exact document byte offset.
    pub offset: u32,
    /// Nonzero response-data budget, capped at 240 bytes.
    pub maximum_bytes: u16,
}

impl CapabilityReadRequest {
    /// Encodes one canonical request body.
    pub fn encode(self) -> Result<[u8; CAPABILITY_READ_REQUEST_BYTES], CapabilityWireError> {
        self.validate()?;
        let mut encoded = [0_u8; CAPABILITY_READ_REQUEST_BYTES];
        encoded[0..8].copy_from_slice(&REQUEST_MAGIC);
        encoded[8..10].copy_from_slice(&CAPABILITY_DOCUMENT_VERSION.to_le_bytes());
        // Bytes 10..12 and 18..24 are reserved zero.
        encoded[12..16].copy_from_slice(&self.offset.to_le_bytes());
        encoded[16..18].copy_from_slice(&self.maximum_bytes.to_le_bytes());
        encoded[24..56].copy_from_slice(&self.expected_digest.0);
        Ok(encoded)
    }

    /// Decodes only the exact V1 representation.
    pub fn decode(encoded: &[u8]) -> Result<Self, CapabilityWireError> {
        if encoded.len() != CAPABILITY_READ_REQUEST_BYTES {
            return Err(CapabilityWireError::Length);
        }
        if encoded[0..8] != REQUEST_MAGIC {
            return Err(CapabilityWireError::Magic);
        }
        if read_u16(encoded, 8) != CAPABILITY_DOCUMENT_VERSION {
            return Err(CapabilityWireError::Version);
        }
        if encoded[10..12].iter().any(|byte| *byte != 0)
            || encoded[18..24].iter().any(|byte| *byte != 0)
        {
            return Err(CapabilityWireError::Reserved);
        }
        let mut expected_digest = [0_u8; 32];
        expected_digest.copy_from_slice(&encoded[24..56]);
        let request = Self {
            expected_digest: Digest(expected_digest),
            offset: read_u32(encoded, 12),
            maximum_bytes: read_u16(encoded, 16),
        };
        request.validate()?;
        if request.encode()? != encoded {
            return Err(CapabilityWireError::Noncanonical);
        }
        Ok(request)
    }

    fn validate(self) -> Result<(), CapabilityWireError> {
        if self.maximum_bytes == 0 || usize::from(self.maximum_bytes) > MAX_CAPABILITY_CHUNK_BYTES {
            return Err(CapabilityWireError::ChunkLength);
        }
        Ok(())
    }
}

/// Fixed response metadata preceding exactly `chunk_len` document bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityReadResponse {
    /// Complete immutable document identity.
    pub identity: CapabilityIdentity,
    /// Offset of the following bytes.
    pub offset: u32,
    /// Exact following document bytes.
    pub chunk_len: u16,
    /// True only for the range ending at `identity.byte_len`.
    pub complete: bool,
}

impl CapabilityReadResponse {
    /// Encodes the canonical fixed response prefix.
    pub fn encode(
        self,
    ) -> Result<[u8; CAPABILITY_READ_RESPONSE_PREFIX_BYTES], CapabilityWireError> {
        self.validate()?;
        let mut encoded = [0_u8; CAPABILITY_READ_RESPONSE_PREFIX_BYTES];
        encoded[0..8].copy_from_slice(&RESPONSE_MAGIC);
        encoded[8..10].copy_from_slice(&CAPABILITY_DOCUMENT_VERSION.to_le_bytes());
        encoded[10] = u8::from(self.complete) * RESPONSE_FLAG_COMPLETE;
        // Bytes 11..16 and 26..32 are reserved zero.
        encoded[16..20].copy_from_slice(&self.identity.byte_len.to_le_bytes());
        encoded[20..24].copy_from_slice(&self.offset.to_le_bytes());
        encoded[24..26].copy_from_slice(&self.chunk_len.to_le_bytes());
        encoded[32..64].copy_from_slice(&self.identity.digest.0);
        Ok(encoded)
    }

    /// Decodes the exact prefix; the caller still binds the following body length.
    pub fn decode(encoded: &[u8]) -> Result<Self, CapabilityWireError> {
        if encoded.len() != CAPABILITY_READ_RESPONSE_PREFIX_BYTES {
            return Err(CapabilityWireError::Length);
        }
        if encoded[0..8] != RESPONSE_MAGIC {
            return Err(CapabilityWireError::Magic);
        }
        if read_u16(encoded, 8) != CAPABILITY_DOCUMENT_VERSION {
            return Err(CapabilityWireError::Version);
        }
        if encoded[10] & !RESPONSE_FLAG_COMPLETE != 0
            || encoded[11..16].iter().any(|byte| *byte != 0)
            || encoded[26..32].iter().any(|byte| *byte != 0)
        {
            return Err(CapabilityWireError::Reserved);
        }
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&encoded[32..64]);
        let response = Self {
            identity: CapabilityIdentity {
                byte_len: read_u32(encoded, 16),
                digest: Digest(digest),
            },
            offset: read_u32(encoded, 20),
            chunk_len: read_u16(encoded, 24),
            complete: encoded[10] & RESPONSE_FLAG_COMPLETE != 0,
        };
        response.validate()?;
        if response.encode()? != encoded {
            return Err(CapabilityWireError::Noncanonical);
        }
        Ok(response)
    }

    /// Decodes a complete response body and binds its exact following range.
    pub fn decode_body(encoded: &[u8]) -> Result<(Self, &[u8]), CapabilityWireError> {
        let prefix = encoded
            .get(..CAPABILITY_READ_RESPONSE_PREFIX_BYTES)
            .ok_or(CapabilityWireError::Length)?;
        let response = Self::decode(prefix)?;
        let expected = CAPABILITY_READ_RESPONSE_PREFIX_BYTES
            .checked_add(usize::from(response.chunk_len))
            .ok_or(CapabilityWireError::Length)?;
        if encoded.len() != expected {
            return Err(CapabilityWireError::Length);
        }
        Ok((response, &encoded[CAPABILITY_READ_RESPONSE_PREFIX_BYTES..]))
    }

    fn validate(self) -> Result<(), CapabilityWireError> {
        if self.identity.byte_len
            < u32::try_from(CAPABILITY_DOCUMENT_HEADER_BYTES).unwrap_or(u32::MAX)
            || self.identity.digest.is_zero()
            || self.chunk_len == 0
            || usize::from(self.chunk_len) > MAX_CAPABILITY_CHUNK_BYTES
        {
            return Err(CapabilityWireError::ChunkLength);
        }
        let end = self
            .offset
            .checked_add(u32::from(self.chunk_len))
            .ok_or(CapabilityWireError::Range)?;
        if end > self.identity.byte_len || self.complete != (end == self.identity.byte_len) {
            return Err(CapabilityWireError::Range);
        }
        Ok(())
    }
}

/// Canonical capability request/response rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityWireError {
    /// Body or fixed prefix length was not exact.
    Length,
    /// Magic did not select the expected schema.
    Magic,
    /// Version was not exactly V1.
    Version,
    /// Flags or reserved bytes were nonzero.
    Reserved,
    /// Requested or returned chunk length was outside the fixed budget.
    ChunkLength,
    /// Offset, length, completion, and total document length disagreed.
    Range,
    /// Valid fields used a noncanonical representation.
    Noncanonical,
}

trait ByteSink {
    fn write(&mut self, bytes: &[u8]) -> Result<(), CapabilityError>;
}

#[derive(Default)]
struct CountingSink {
    byte_len: u32,
}

impl ByteSink for CountingSink {
    fn write(&mut self, bytes: &[u8]) -> Result<(), CapabilityError> {
        self.byte_len = self
            .byte_len
            .checked_add(u32::try_from(bytes.len()).map_err(|_| CapabilityError::Length)?)
            .ok_or(CapabilityError::Length)?;
        Ok(())
    }
}

struct HashingSink {
    hasher: Sha256,
    byte_len: u32,
}

impl Default for HashingSink {
    fn default() -> Self {
        Self {
            hasher: Sha256::new(),
            byte_len: 0,
        }
    }
}

impl HashingSink {
    fn finish(self) -> Digest {
        let digest = self.hasher.finalize();
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(&digest);
        Digest(bytes)
    }
}

impl ByteSink for HashingSink {
    fn write(&mut self, bytes: &[u8]) -> Result<(), CapabilityError> {
        self.byte_len = self
            .byte_len
            .checked_add(u32::try_from(bytes.len()).map_err(|_| CapabilityError::Length)?)
            .ok_or(CapabilityError::Length)?;
        self.hasher.update(bytes);
        Ok(())
    }
}

struct RangeSink<'a> {
    offset: u32,
    cursor: u32,
    output: &'a mut [u8],
    written: usize,
}

impl<'a> RangeSink<'a> {
    const fn new(offset: u32, output: &'a mut [u8]) -> Self {
        Self {
            offset,
            cursor: 0,
            output,
            written: 0,
        }
    }

    fn finish(self) -> Result<usize, CapabilityError> {
        if self.written != self.output.len() {
            return Err(CapabilityError::Length);
        }
        Ok(self.written)
    }
}

impl ByteSink for RangeSink<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), CapabilityError> {
        let byte_len = u32::try_from(bytes.len()).map_err(|_| CapabilityError::Length)?;
        let end = self
            .cursor
            .checked_add(byte_len)
            .ok_or(CapabilityError::Length)?;
        let output_end = self
            .offset
            .checked_add(u32::try_from(self.output.len()).map_err(|_| CapabilityError::Length)?)
            .ok_or(CapabilityError::Length)?;
        let overlap_start = self.cursor.max(self.offset);
        let overlap_end = end.min(output_end);
        if overlap_start < overlap_end {
            let source_start = usize::try_from(overlap_start - self.cursor)
                .map_err(|_| CapabilityError::Length)?;
            let count = usize::try_from(overlap_end - overlap_start)
                .map_err(|_| CapabilityError::Length)?;
            let destination_end = self
                .written
                .checked_add(count)
                .ok_or(CapabilityError::Length)?;
            self.output[self.written..destination_end]
                .copy_from_slice(&bytes[source_start..source_start + count]);
            self.written = destination_end;
        }
        self.cursor = end;
        Ok(())
    }
}

fn document_header(total: u32) -> [u8; CAPABILITY_DOCUMENT_HEADER_BYTES] {
    let mut header = [0_u8; CAPABILITY_DOCUMENT_HEADER_BYTES];
    header[0..8].copy_from_slice(&DOCUMENT_MAGIC);
    header[8..10].copy_from_slice(&CAPABILITY_DOCUMENT_VERSION.to_le_bytes());
    // Bytes 10..12 are reserved zero.
    header[12..16].copy_from_slice(&total.to_le_bytes());
    header
}

fn encode_payload<S: ByteSink>(
    package: &BoardPackage<'_>,
    sink: &mut S,
) -> Result<(), CapabilityError> {
    write_str(sink, package.board.id)?;
    write_str(sink, package.board.revision)?;
    write_u8(sink, chip(package.board.chip))?;
    write_u8(sink, package.board.application_cores)?;
    write_u8(sink, qualification(package.board.qualification))?;
    write_bool(sink, package.armable)?;
    write_usize(sink, package.memory.flash_bytes)?;
    write_usize(sink, package.memory.internal_sram_bytes)?;
    write_usize(sink, package.memory.psram_bytes)?;
    write_bool(sink, package.memory.realtime_psram_allowed)?;
    sink.write(&[
        package.cores.service_core,
        package.cores.realtime_core,
        0,
        0,
    ])?;

    write_count(sink, package.board.resources.len())?;
    for resource in package.board.resources {
        write_resource(sink, resource.id)?;
        sink.write(&[
            owner(resource.owner),
            safe_value(resource.safe_value),
            u8::from(resource.hazardous_output),
            0,
        ])?;
    }

    write_count(sink, package.aliases.len())?;
    for alias in package.aliases {
        write_str(sink, alias.name)?;
        write_resource(sink, alias.resource)?;
    }

    write_count(sink, package.buses.len())?;
    for bus in package.buses {
        write_resource(sink, bus.resource)?;
        sink.write(&[bus_kind(bus.kind), owner(bus.owner), 0, 0])?;
        write_u32(sink, bus.maximum_frequency_hz)?;
        write_count(sink, bus.pins.len())?;
        for pin in bus.pins {
            write_resource(sink, *pin)?;
        }
    }

    write_count(sink, package.devices.len())?;
    for device in package.devices {
        write_resource(sink, device.resource)?;
        sink.write(&[owner(device.owner), support(device.support), 0, 0])?;
        write_optional_resource(sink, device.bus)?;
        write_device_route(sink, device.route)?;
        write_count(sink, device.auxiliary_resources.len())?;
        for resource in device.auxiliary_resources {
            write_resource(sink, *resource)?;
        }
    }

    write_count(sink, package.flash_regions.len())?;
    for region in package.flash_regions {
        write_str(sink, region.name)?;
        write_u32(sink, region.offset)?;
        write_u32(sink, region.length)?;
        sink.write(&[
            flash_kind(region.kind),
            u8::from(region.writable_while_armed),
            support(region.support),
            0,
        ])?;
    }

    write_count(sink, package.clocks.len())?;
    for clock in package.clocks {
        write_str(sink, clock.name)?;
        sink.write(&[
            clock_source(clock.source),
            clock_domain(clock.domain),
            support(clock.support),
            u8::from(clock.maximum_error_ppm.is_some()),
        ])?;
        write_u64(sink, clock.nominal_hz)?;
        write_u32(sink, clock.maximum_error_ppm.unwrap_or(0))?;
    }

    write_count(sink, package.electrical_constraints.len())?;
    for constraint in package.electrical_constraints {
        write_str(sink, constraint.id)?;
        sink.write(&[
            electrical_kind(constraint.kind),
            support(constraint.support),
            0,
            0,
        ])?;
        write_count(sink, constraint.resources.len())?;
        for resource in constraint.resources {
            write_resource(sink, *resource)?;
        }
        write_str(sink, constraint.note)?;
    }

    write_count(sink, package.interrupts.len())?;
    for interrupt in package.interrupts {
        write_resource(sink, interrupt.source)?;
        sink.write(&[
            owner(interrupt.owner),
            interrupt_trigger(interrupt.trigger),
            support(interrupt.support),
            u8::from(interrupt.maximum_latency_cycles.is_some()),
        ])?;
        write_u64(sink, interrupt.maximum_latency_cycles.unwrap_or(0))?;
    }

    write_count(sink, package.safe_output_images.len())?;
    for image in package.safe_output_images {
        sink.write(&[image.engine, u8::from(image.bench_verified), 0, 0])?;
        write_u32(sink, image.defined_mask)?;
        write_u32(sink, image.safe_bits)?;
    }

    write_count(sink, package.visuals.len())?;
    for visual in package.visuals {
        write_str(sink, visual.id)?;
        write_str(sink, visual.asset_path)?;
        write_str(sink, visual.media_type)?;
        write_u32(sink, visual.pixel_width)?;
        write_u32(sink, visual.pixel_height)?;
        sink.write(&visual.asset_digest.0)?;
        write_str(sink, visual.license)?;
        write_str(sink, visual.attribution)?;
        write_count(sink, visual.hotspots.len())?;
        for hotspot in visual.hotspots {
            write_str(sink, hotspot.id)?;
            write_resource(sink, hotspot.resource)?;
            write_count(sink, hotspot.polygon.len())?;
            for point in hotspot.polygon {
                write_u16(sink, point.x)?;
                write_u16(sink, point.y)?;
            }
        }
    }

    write_count(sink, package.hil_requirements.len())?;
    for requirement in package.hil_requirements {
        write_str(sink, requirement.id)?;
        sink.write(&[
            hil_kind(requirement.kind),
            qualification(requirement.required_for),
            0,
            0,
        ])?;
        write_count(sink, requirement.resources.len())?;
        for resource in requirement.resources {
            write_resource(sink, *resource)?;
        }
    }
    Ok(())
}

fn write_str<S: ByteSink>(sink: &mut S, value: &str) -> Result<(), CapabilityError> {
    write_count(sink, value.len())?;
    sink.write(value.as_bytes())
}

fn write_count<S: ByteSink>(sink: &mut S, value: usize) -> Result<(), CapabilityError> {
    write_u32(
        sink,
        u32::try_from(value).map_err(|_| CapabilityError::Length)?,
    )
}

fn write_usize<S: ByteSink>(sink: &mut S, value: usize) -> Result<(), CapabilityError> {
    write_u64(
        sink,
        u64::try_from(value).map_err(|_| CapabilityError::Length)?,
    )
}

fn write_bool<S: ByteSink>(sink: &mut S, value: bool) -> Result<(), CapabilityError> {
    write_u8(sink, u8::from(value))
}

fn write_u8<S: ByteSink>(sink: &mut S, value: u8) -> Result<(), CapabilityError> {
    sink.write(&[value])
}

fn write_u16<S: ByteSink>(sink: &mut S, value: u16) -> Result<(), CapabilityError> {
    sink.write(&value.to_le_bytes())
}

fn write_u32<S: ByteSink>(sink: &mut S, value: u32) -> Result<(), CapabilityError> {
    sink.write(&value.to_le_bytes())
}

fn write_u64<S: ByteSink>(sink: &mut S, value: u64) -> Result<(), CapabilityError> {
    sink.write(&value.to_le_bytes())
}

fn write_optional_resource<S: ByteSink>(
    sink: &mut S,
    resource: Option<ResourceId>,
) -> Result<(), CapabilityError> {
    match resource {
        Some(resource) => {
            sink.write(&[1, 0, 0, 0])?;
            write_resource(sink, resource)
        }
        None => sink.write(&[0; 8]),
    }
}

fn write_device_route<S: ByteSink>(
    sink: &mut S,
    route: DeviceRoute,
) -> Result<(), CapabilityError> {
    match route {
        DeviceRoute::Dedicated => sink.write(&[1, 0, 0, 0, 0, 0, 0, 0]),
        DeviceRoute::I2cAddress(address) => sink.write(&[2, 0, 0, 0, address, 0, 0, 0]),
        DeviceRoute::SpiChipSelect(resource) => {
            sink.write(&[3, 0, 0, 0])?;
            write_resource(sink, resource)
        }
        DeviceRoute::Uart => sink.write(&[4, 0, 0, 0, 0, 0, 0, 0]),
    }
}

fn write_resource<S: ByteSink>(sink: &mut S, resource: ResourceId) -> Result<(), CapabilityError> {
    let (kind, first, second) = match resource {
        ResourceId::Gpio(index) => (1, 0, u16::from(index)),
        ResourceId::I2sOut { engine, bit } => (2, engine, u16::from(bit)),
        ResourceId::Adc { unit, channel } => (3, unit, u16::from(channel)),
        ResourceId::Timer { group, index } => (4, group, u16::from(index)),
        ResourceId::I2s(index) => (5, 0, u16::from(index)),
        ResourceId::Rmt(index) => (6, 0, u16::from(index)),
        ResourceId::TimedOutput { engine, channel } => (7, engine, u16::from(channel)),
        ResourceId::I2c(index) => (8, 0, u16::from(index)),
        ResourceId::Spi(index) => (9, 0, u16::from(index)),
        ResourceId::Uart(index) => (10, 0, u16::from(index)),
        ResourceId::Pcnt(index) => (11, 0, u16::from(index)),
        ResourceId::Dma(index) => (12, 0, u16::from(index)),
        ResourceId::Twai(index) => (13, 0, u16::from(index)),
        ResourceId::Storage(index) => (14, 0, u16::from(index)),
        ResourceId::Radio(index) => (15, 0, u16::from(index)),
        ResourceId::SafetyInput(index) => (16, 0, u16::from(index)),
        ResourceId::Device(index) => (17, 0, index),
    };
    sink.write(&[kind, first, second as u8, (second >> 8) as u8])
}

const fn chip(value: Chip) -> u8 {
    match value {
        Chip::Esp32 => 1,
        Chip::Esp32S3 => 2,
    }
}

const fn qualification(value: Qualification) -> u8 {
    match value {
        Qualification::Described => 1,
        Qualification::Compiles => 2,
        Qualification::Bench => 3,
        Qualification::MotionQualified => 4,
        Qualification::ProductionQualified => 5,
    }
}

const fn owner(value: OwnerDomain) -> u8 {
    match value {
        OwnerDomain::Service => 1,
        OwnerDomain::Realtime => 2,
    }
}

const fn safe_value(value: SafeValue) -> u8 {
    match value {
        SafeValue::NotApplicable => 1,
        SafeValue::HighImpedance => 2,
        SafeValue::Low => 3,
        SafeValue::High => 4,
        SafeValue::EngineImage => 5,
    }
}

const fn bus_kind(value: BusKind) -> u8 {
    match value {
        BusKind::I2c => 1,
        BusKind::Spi => 2,
        BusKind::Uart => 3,
    }
}

const fn support(value: SupportLevel) -> u8 {
    match value {
        SupportLevel::Described => 1,
        SupportLevel::Compiles => 2,
        SupportLevel::Bench => 3,
        SupportLevel::Qualified => 4,
    }
}

const fn flash_kind(value: FlashRegionKind) -> u8 {
    match value {
        FlashRegionKind::Bootloader => 1,
        FlashRegionKind::PartitionTable => 2,
        FlashRegionKind::Application => 3,
        FlashRegionKind::Configuration => 4,
        FlashRegionKind::WebBundle => 5,
        FlashRegionKind::UpdateSlot => 6,
        FlashRegionKind::CrashLog => 7,
    }
}

const fn clock_source(value: ClockSource) -> u8 {
    match value {
        ClockSource::Crystal => 1,
        ClockSource::Pll => 2,
        ClockSource::PeripheralBus => 3,
        ClockSource::Rtc => 4,
        ClockSource::External => 5,
    }
}

const fn clock_domain(value: ClockDomain) -> u8 {
    match value {
        ClockDomain::Chip => 1,
        ClockDomain::Service => 2,
        ClockDomain::Realtime => 3,
    }
}

const fn electrical_kind(value: ElectricalConstraintKind) -> u8 {
    match value {
        ElectricalConstraintKind::InputOnly => 1,
        ElectricalConstraintKind::OutputOnly => 2,
        ElectricalConstraintKind::BootStrap => 3,
        ElectricalConstraintKind::SharedRoute => 4,
        ElectricalConstraintKind::ActiveHigh => 5,
        ElectricalConstraintKind::ActiveLow => 6,
        ElectricalConstraintKind::NotPwm => 7,
        ElectricalConstraintKind::Logic3v3 => 8,
        ElectricalConstraintKind::ResetStateUnverified => 9,
    }
}

const fn interrupt_trigger(value: InterruptTrigger) -> u8 {
    match value {
        InterruptTrigger::Rising => 1,
        InterruptTrigger::Falling => 2,
        InterruptTrigger::AnyEdge => 3,
        InterruptTrigger::LowLevel => 4,
        InterruptTrigger::HighLevel => 5,
        InterruptTrigger::Configurable => 6,
    }
}

const fn hil_kind(value: HilKind) -> u8 {
    match value {
        HilKind::BoardIdentity => 1,
        HilKind::SafeState => 2,
        HilKind::PeripheralSmoke => 3,
        HilKind::CoreIsolation => 4,
        HilKind::Timing => 5,
        HilKind::FaultInjection => 6,
        HilKind::VisualReconciliation => 7,
    }
}

const fn read_u16(encoded: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([encoded[offset], encoded[offset + 1]])
}

const fn read_u32(encoded: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        encoded[offset],
        encoded[offset + 1],
        encoded[offset + 2],
        encoded[offset + 3],
    ])
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::vec;

    #[test]
    fn request_and_response_prefixes_are_exact_and_canonical() {
        let request = CapabilityReadRequest {
            expected_digest: Digest([0x55; 32]),
            offset: 240,
            maximum_bytes: 240,
        };
        assert_eq!(
            CapabilityReadRequest::decode(&request.encode().unwrap()),
            Ok(request)
        );
        let response = CapabilityReadResponse {
            identity: CapabilityIdentity {
                byte_len: 480,
                digest: Digest([0x66; 32]),
            },
            offset: 240,
            chunk_len: 240,
            complete: true,
        };
        assert_eq!(
            CapabilityReadResponse::decode(&response.encode().unwrap()),
            Ok(response)
        );
        let mut body = vec![0_u8; CAPABILITY_READ_RESPONSE_PREFIX_BYTES + 240];
        body[..CAPABILITY_READ_RESPONSE_PREFIX_BYTES].copy_from_slice(&response.encode().unwrap());
        body[CAPABILITY_READ_RESPONSE_PREFIX_BYTES..].fill(0xa5);
        let (decoded, chunk) = CapabilityReadResponse::decode_body(&body).unwrap();
        assert_eq!(decoded, response);
        assert_eq!(chunk, &[0xa5; 240]);
        body.push(0);
        assert_eq!(
            CapabilityReadResponse::decode_body(&body),
            Err(CapabilityWireError::Length)
        );

        let mut reserved = request.encode().unwrap();
        reserved[20] = 1;
        assert_eq!(
            CapabilityReadRequest::decode(&reserved),
            Err(CapabilityWireError::Reserved)
        );
    }

    #[test]
    fn real_board_documents_are_repeatable_and_byte_addressable() {
        for package in [&board_mks_tinybee::PACKAGE, &board_t_deck_pro::PACKAGE] {
            let identity = calculate_identity(package).unwrap();
            assert!(!identity.digest.is_zero());
            let mut document = vec![0_u8; usize::try_from(identity.byte_len).unwrap()];
            let mut offset = 0_u32;
            while offset < identity.byte_len {
                let start = usize::try_from(offset).unwrap();
                let mut chunk = [0_u8; 73];
                let maximum = chunk
                    .len()
                    .min(usize::try_from(identity.byte_len - offset).unwrap());
                let mut sink = RangeSink::new(offset, &mut chunk[..maximum]);
                sink.write(&document_header(identity.byte_len)).unwrap();
                encode_payload(package, &mut sink).unwrap();
                let written = sink.finish().unwrap();
                document[start..start + written].copy_from_slice(&chunk[..written]);
                offset += u32::try_from(written).unwrap();
            }
            assert_eq!(&document[..8], b"ALMCAP01");
            assert_eq!(read_u32(&document, 12), identity.byte_len);
            let mut hasher = Sha256::new();
            hasher.update(&document);
            let mut digest = [0_u8; 32];
            digest.copy_from_slice(&hasher.finalize());
            assert_eq!(Digest(digest), identity.digest);
            assert_eq!(calculate_identity(package).unwrap(), identity);
        }
    }

    #[test]
    fn verified_ranges_require_the_compiled_digest() {
        let package = &board_mks_tinybee::PACKAGE;
        let mut missing = *package;
        missing.board.capability_digest = Digest::ZERO;
        assert_eq!(
            read_verified_range(&missing, 0, &mut [0_u8; 16]),
            Err(CapabilityError::MissingDeclaredDigest)
        );
        let identity = calculate_identity(package).unwrap();
        assert_eq!(verify_declared_identity(package), Ok(identity));
        let mut divergent = *package;
        divergent.board.capability_digest = Digest([0x11; 32]);
        assert_eq!(
            verify_declared_identity(&divergent),
            Err(CapabilityError::DeclaredDigestMismatch {
                declared: Digest([0x11; 32]),
                calculated: identity.digest,
            })
        );
        let mut chunk = [0_u8; 32];
        let read = read_verified_range(package, 0, &mut chunk).unwrap();
        assert_eq!(read.identity, identity);
        assert_eq!(read.byte_len, 32);
        assert!(!read.complete);
        assert_eq!(&chunk[..8], b"ALMCAP01");
    }

    #[test]
    fn no_output_at_exact_end_is_a_complete_range() {
        let package = &board_t_deck_pro::PACKAGE;
        let identity = calculate_identity(package).unwrap();
        assert_eq!(verify_declared_identity(package), Ok(identity));
        let read = read_verified_range(package, identity.byte_len, &mut []).unwrap();
        assert_eq!(read.byte_len, 0);
        assert!(read.complete);
    }
}
