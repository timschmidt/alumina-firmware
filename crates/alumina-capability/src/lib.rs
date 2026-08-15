#![no_std]
#![doc = "Canonical byte-addressable board capability documents for Alumina."]

use alumina_board::{
    AliasDescriptor, BoardError, BoardPackage, BusKind, Chip, ClockDomain, ClockSource,
    DeviceRoute, DiagnosticObservationKind, DiagnosticOverviewDescriptor,
    DiagnosticResourceDescriptor, DigitalCaptureConfigureFlags, DigitalCaptureDescriptor,
    DigitalCaptureResourceDescriptor, DigitalCaptureSourceKind, DigitalCaptureTriggerSet,
    ElectricalConstraintKind, FlashRegionKind, GraphExecutorDescriptor, GraphOpcodeDescriptor,
    GraphResourceAccess, GraphResourceClass, GraphResourceDescriptor, HilKind, InterruptTrigger,
    NormalizedPoint, OwnerDomain, Qualification, ResourceDescriptor, ResourceId, SafeValue,
    SupportLevel,
};
use alumina_protocol::Digest;
use sha2::{Digest as ShaDigest, Sha256};

/// Exact capability-document schema version.
pub const CAPABILITY_DOCUMENT_VERSION: u16 = 4;
/// Bytes in the fixed canonical document header.
pub const CAPABILITY_DOCUMENT_HEADER_BYTES: usize = 16;
/// Exact `CapabilitiesGet` range-request body length.
pub const CAPABILITY_READ_REQUEST_BYTES: usize = 56;
/// Exact response prefix before bounded document bytes.
pub const CAPABILITY_READ_RESPONSE_PREFIX_BYTES: usize = 64;
/// Largest capability range returned in one native response.
pub const MAX_CAPABILITY_CHUNK_BYTES: usize = 240;
/// Bytes in the fixed graph-executor prefix before opcode/resource records.
pub const GRAPH_EXECUTOR_HEADER_BYTES: usize = 72;
/// Bytes in one graph opcode-capability record.
pub const GRAPH_OPCODE_CAPABILITY_BYTES: usize = 12;
/// Bytes in one graph resource-capability record.
pub const GRAPH_RESOURCE_CAPABILITY_BYTES: usize = 12;
/// Bytes in the passive diagnostic-overview prefix before resource records.
pub const DIAGNOSTIC_OVERVIEW_HEADER_BYTES: usize = 48;
/// Bytes in one passive diagnostic resource-capability record.
pub const DIAGNOSTIC_RESOURCE_CAPABILITY_BYTES: usize = 12;
/// Bytes in the digital-capture prefix before resource records.
pub const DIGITAL_CAPTURE_HEADER_BYTES: usize = 64;
/// Bytes in one digital-capture resource-capability record.
pub const DIGITAL_CAPTURE_RESOURCE_CAPABILITY_BYTES: usize = 12;

const DOCUMENT_MAGIC: [u8; 8] = *b"ALMCAP04";
const REQUEST_MAGIC: [u8; 8] = *b"ALMCPQ04";
const RESPONSE_MAGIC: [u8; 8] = *b"ALMCPR04";
const GRAPH_EXECUTOR_MAGIC: [u8; 8] = *b"ALMGRC02";
const DIAGNOSTIC_OVERVIEW_MAGIC: [u8; 8] = *b"ALMDOV01";
const DIGITAL_CAPTURE_MAGIC: [u8; 8] = *b"ALMDCP01";
const RESPONSE_FLAG_COMPLETE: u8 = 1 << 0;

/// Exact identity of one canonical immutable capability document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityIdentity {
    /// Complete document length including its 16-byte header.
    pub byte_len: u32,
    /// SHA-256 over every document byte.
    pub digest: Digest,
}

/// Caller-selected work and memory limits for independent board-document
/// inspection.
///
/// These limits bound hostile range-assembled documents before any UI model is
/// constructed. A zero count limit intentionally rejects a nonempty matching
/// section while still permitting an empty one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardCapabilityLimits {
    /// Largest complete document accepted.
    pub maximum_document_bytes: u32,
    /// Largest individual UTF-8 string accepted.
    pub maximum_string_bytes: u32,
    /// Largest diagnostic-resource, descriptive-resource, alias, bus, device,
    /// flash, clock, constraint, interrupt, safe-image, or HIL-requirement
    /// table accepted.
    pub maximum_records_per_section: u32,
    /// Largest visual table accepted.
    pub maximum_visuals: u32,
    /// Largest hotspot table accepted for one visual.
    pub maximum_hotspots_per_visual: u32,
    /// Largest polygon accepted for one hotspot.
    pub maximum_points_per_hotspot: u32,
}

impl BoardCapabilityLimits {
    /// Bounded browser/native inspection policy.
    pub const fn interactive() -> Self {
        Self {
            maximum_document_bytes: 4 * 1_024 * 1_024,
            maximum_string_bytes: 64 * 1_024,
            maximum_records_per_section: 4_096,
            maximum_visuals: 32,
            maximum_hotspots_per_visual: 4_096,
            maximum_points_per_hotspot: 4_096,
        }
    }
}

impl Default for BoardCapabilityLimits {
    fn default() -> Self {
        Self::interactive()
    }
}

/// Independently validated, allocation-free view of one complete canonical V4
/// board capability document.
///
/// The view exposes the board summary and the descriptive tables needed by a
/// board explorer while preserving the graph executor as a separate authority.
/// All intervening V4 sections are structurally and canonically validated even
/// when they are represented here only by counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardCapabilityView<'a> {
    identity: CapabilityIdentity,
    board_id: &'a str,
    revision: &'a str,
    chip: Chip,
    application_cores: u8,
    qualification: Qualification,
    armable: bool,
    flash_bytes: u64,
    internal_sram_bytes: u64,
    psram_bytes: u64,
    realtime_psram_allowed: bool,
    service_core: u8,
    realtime_core: u8,
    graph: GraphExecutionCapability<'a>,
    diagnostic_overview: DiagnosticOverviewCapability<'a>,
    digital_capture: DigitalCaptureCapability<'a>,
    resource_records: &'a [u8],
    alias_records: &'a [u8],
    alias_count: usize,
    bus_count: usize,
    device_count: usize,
    flash_region_count: usize,
    clock_count: usize,
    electrical_constraint_count: usize,
    interrupt_count: usize,
    safe_output_image_count: usize,
    visual_records: &'a [u8],
    visual_count: usize,
    hil_requirement_count: usize,
}

impl<'a> BoardCapabilityView<'a> {
    /// SHA-256 identity of every byte in the complete document.
    pub const fn identity(self) -> CapabilityIdentity {
        self.identity
    }

    /// Stable board/revision identifier.
    pub const fn board_id(self) -> &'a str {
        self.board_id
    }

    /// Human-readable revision evidence string.
    pub const fn revision(self) -> &'a str {
        self.revision
    }

    /// Application MCU family.
    pub const fn chip(self) -> Chip {
        self.chip
    }

    /// Number of application cores published by the exact image.
    pub const fn application_cores(self) -> u8 {
        self.application_cores
    }

    /// Evidence level of the exact board package.
    pub const fn qualification(self) -> Qualification {
        self.qualification
    }

    /// Whether this exact package claims that arming may be admitted.
    pub const fn armable(self) -> bool {
        self.armable
    }

    /// Fitted flash capacity in bytes.
    pub const fn flash_bytes(self) -> u64 {
        self.flash_bytes
    }

    /// Internal SRAM capacity in bytes before runtime reservations.
    pub const fn internal_sram_bytes(self) -> u64 {
        self.internal_sram_bytes
    }

    /// Fitted external PSRAM capacity in bytes.
    pub const fn psram_bytes(self) -> u64 {
        self.psram_bytes
    }

    /// Whether deterministic active state may occupy PSRAM. V4 board-package
    /// validation currently requires this to be false.
    pub const fn realtime_psram_allowed(self) -> bool {
        self.realtime_psram_allowed
    }

    /// Fixed service-core index.
    pub const fn service_core(self) -> u8 {
        self.service_core
    }

    /// Fixed real-time-core index.
    pub const fn realtime_core(self) -> u8 {
        self.realtime_core
    }

    /// Independently decoded graph executor and its narrower access palette.
    pub const fn graph(self) -> GraphExecutionCapability<'a> {
        self.graph
    }

    /// Passive diagnostic observations, separate from graph authority.
    pub const fn diagnostic_overview(self) -> DiagnosticOverviewCapability<'a> {
        self.diagnostic_overview
    }

    /// Device-produced digital capture, separate from graph and overview authority.
    pub const fn digital_capture(self) -> DigitalCaptureCapability<'a> {
        self.digital_capture
    }

    /// Number of descriptive resource records.
    pub const fn resource_count(self) -> usize {
        self.resource_records.len() / 8
    }

    /// Iterate descriptive resources in canonical board-package order.
    pub fn resources(self) -> impl ExactSizeIterator<Item = ResourceDescriptor> + 'a {
        self.resource_records
            .chunks_exact(8)
            .map(decode_resource_descriptor_unchecked)
    }

    /// Number of aliases.
    pub const fn alias_count(self) -> usize {
        self.alias_count
    }

    /// Iterate aliases without allocating.
    pub const fn aliases(self) -> CapabilityAliasIter<'a> {
        CapabilityAliasIter {
            records: self.alias_records,
            cursor: 0,
            remaining: self.alias_count,
        }
    }

    /// Number of routed bus records validated in the complete document.
    pub const fn bus_count(self) -> usize {
        self.bus_count
    }

    /// Number of fitted or routed device records validated in the document.
    pub const fn device_count(self) -> usize {
        self.device_count
    }

    /// Number of flash-region records validated in the document.
    pub const fn flash_region_count(self) -> usize {
        self.flash_region_count
    }

    /// Number of clock records validated in the document.
    pub const fn clock_count(self) -> usize {
        self.clock_count
    }

    /// Number of electrical-constraint records validated in the document.
    pub const fn electrical_constraint_count(self) -> usize {
        self.electrical_constraint_count
    }

    /// Number of interrupt records validated in the document.
    pub const fn interrupt_count(self) -> usize {
        self.interrupt_count
    }

    /// Number of complete shifted-output safe images validated in the document.
    pub const fn safe_output_image_count(self) -> usize {
        self.safe_output_image_count
    }

    /// Number of licensed board visuals.
    pub const fn visual_count(self) -> usize {
        self.visual_count
    }

    /// Iterate visual records and their normalized hotspot views.
    pub const fn visuals(self) -> CapabilityVisualIter<'a> {
        CapabilityVisualIter {
            records: self.visual_records,
            cursor: 0,
            remaining: self.visual_count,
        }
    }

    /// Number of HIL evidence requirements validated in the document.
    pub const fn hil_requirement_count(self) -> usize {
        self.hil_requirement_count
    }
}

/// Allocation-free iterator over canonical alias records.
#[derive(Clone, Debug)]
pub struct CapabilityAliasIter<'a> {
    records: &'a [u8],
    cursor: usize,
    remaining: usize,
}

impl<'a> Iterator for CapabilityAliasIter<'a> {
    type Item = AliasDescriptor<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let name = decode_validated_string(self.records, &mut self.cursor);
        let resource = decode_validated_resource(self.records, &mut self.cursor);
        self.remaining -= 1;
        Some(AliasDescriptor { name, resource })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for CapabilityAliasIter<'_> {}

/// One independently decoded licensed visual and its hotspot byte range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityVisual<'a> {
    id: &'a str,
    asset_path: &'a str,
    media_type: &'a str,
    pixel_width: u32,
    pixel_height: u32,
    asset_digest: Digest,
    license: &'a str,
    attribution: &'a str,
    hotspot_records: &'a [u8],
    hotspot_count: usize,
}

impl<'a> CapabilityVisual<'a> {
    /// Stable visual ID.
    pub const fn id(self) -> &'a str {
        self.id
    }

    /// Repository-relative raster path bound by `asset_digest`.
    pub const fn asset_path(self) -> &'a str {
        self.asset_path
    }

    /// Exact raster MIME type.
    pub const fn media_type(self) -> &'a str {
        self.media_type
    }

    /// Reviewed raster width in pixels.
    pub const fn pixel_width(self) -> u32 {
        self.pixel_width
    }

    /// Reviewed raster height in pixels.
    pub const fn pixel_height(self) -> u32 {
        self.pixel_height
    }

    /// SHA-256 of the raster bytes.
    pub const fn asset_digest(self) -> Digest {
        self.asset_digest
    }

    /// SPDX expression for the raster asset.
    pub const fn license(self) -> &'a str {
        self.license
    }

    /// Required source/photographer attribution.
    pub const fn attribution(self) -> &'a str {
        self.attribution
    }

    /// Number of reviewed resource polygons.
    pub const fn hotspot_count(self) -> usize {
        self.hotspot_count
    }

    /// Iterate reviewed hotspot polygons without allocating.
    pub const fn hotspots(self) -> CapabilityHotspotIter<'a> {
        CapabilityHotspotIter {
            records: self.hotspot_records,
            cursor: 0,
            remaining: self.hotspot_count,
        }
    }
}

/// Allocation-free iterator over canonical visual records.
#[derive(Clone, Debug)]
pub struct CapabilityVisualIter<'a> {
    records: &'a [u8],
    cursor: usize,
    remaining: usize,
}

impl<'a> Iterator for CapabilityVisualIter<'a> {
    type Item = CapabilityVisual<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let visual = decode_validated_visual(self.records, &mut self.cursor);
        self.remaining -= 1;
        Some(visual)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for CapabilityVisualIter<'_> {}

/// One typed resource polygon in normalized visual coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityHotspot<'a> {
    id: &'a str,
    resource: ResourceId,
    point_records: &'a [u8],
}

impl<'a> CapabilityHotspot<'a> {
    /// Stable hotspot ID within one visual.
    pub const fn id(self) -> &'a str {
        self.id
    }

    /// Typed resource represented by the polygon.
    pub const fn resource(self) -> ResourceId {
        self.resource
    }

    /// Number of normalized polygon points.
    pub const fn point_count(self) -> usize {
        self.point_records.len() / 4
    }

    /// Iterate normalized polygon points.
    pub fn points(self) -> impl ExactSizeIterator<Item = NormalizedPoint> + 'a {
        self.point_records
            .chunks_exact(4)
            .map(|point| NormalizedPoint {
                x: read_u16(point, 0),
                y: read_u16(point, 2),
            })
    }
}

/// Allocation-free iterator over canonical hotspot records.
#[derive(Clone, Debug)]
pub struct CapabilityHotspotIter<'a> {
    records: &'a [u8],
    cursor: usize,
    remaining: usize,
}

impl<'a> Iterator for CapabilityHotspotIter<'a> {
    type Item = CapabilityHotspot<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let hotspot = decode_validated_hotspot(self.records, &mut self.cursor);
        self.remaining -= 1;
        Some(hotspot)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for CapabilityHotspotIter<'_> {}

/// Independently decoded fixed V2 graph-executor section of one complete V4
/// capability document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphExecutionCapability<'a> {
    identity: CapabilityIdentity,
    ir_version: u16,
    package_bytes: u32,
    maximum_nodes: u16,
    maximum_channels: u16,
    maximum_queue_items: u32,
    service_state_bytes: u32,
    realtime_state_bytes: u32,
    service_channel_bytes: u32,
    realtime_channel_bytes: u32,
    bridge_channel_bytes: u32,
    support: SupportLevel,
    opcode_records: &'a [u8],
    resource_records: &'a [u8],
}

impl<'a> GraphExecutionCapability<'a> {
    /// Complete capability-document identity containing this graph section.
    pub const fn identity(self) -> CapabilityIdentity {
        self.identity
    }

    /// Exact deployed graph-IR version understood by the image.
    pub const fn ir_version(self) -> u16 {
        self.ir_version
    }

    /// Exact fixed package length.
    pub const fn package_bytes(self) -> u32 {
        self.package_bytes
    }

    /// Maximum admitted node records.
    pub const fn maximum_nodes(self) -> u16 {
        self.maximum_nodes
    }

    /// Maximum admitted channel records.
    pub const fn maximum_channels(self) -> u16 {
        self.maximum_channels
    }

    /// Maximum queue capacity in items.
    pub const fn maximum_queue_items(self) -> u32 {
        self.maximum_queue_items
    }

    /// Permanently reserved Service state bytes.
    pub const fn service_state_bytes(self) -> u32 {
        self.service_state_bytes
    }

    /// Permanently reserved Realtime state bytes.
    pub const fn realtime_state_bytes(self) -> u32 {
        self.realtime_state_bytes
    }

    /// Permanently reserved Service-local channel bytes.
    pub const fn service_channel_bytes(self) -> u32 {
        self.service_channel_bytes
    }

    /// Permanently reserved Realtime-local channel bytes.
    pub const fn realtime_channel_bytes(self) -> u32 {
        self.realtime_channel_bytes
    }

    /// Permanently reserved Service-to-Realtime bridge bytes.
    pub const fn bridge_channel_bytes(self) -> u32 {
        self.bridge_channel_bytes
    }

    /// Evidence level for the fixed executor and its declared timing.
    pub const fn support(self) -> SupportLevel {
        self.support
    }

    /// Number of opcode records in the exact capability palette.
    pub const fn opcode_count(self) -> usize {
        self.opcode_records.len() / GRAPH_OPCODE_CAPABILITY_BYTES
    }

    /// Number of explicitly graph-addressable resources.
    pub const fn resource_count(self) -> usize {
        self.resource_records.len() / GRAPH_RESOURCE_CAPABILITY_BYTES
    }

    /// Iterate independently decoded opcode records in canonical order.
    pub fn opcodes(self) -> impl ExactSizeIterator<Item = GraphOpcodeDescriptor> + 'a {
        self.opcode_records
            .chunks_exact(GRAPH_OPCODE_CAPABILITY_BYTES)
            .map(decode_graph_opcode_unchecked)
    }

    /// Iterate independently decoded resource records in canonical order.
    pub fn resources(self) -> impl ExactSizeIterator<Item = GraphResourceDescriptor> + 'a {
        self.resource_records
            .chunks_exact(GRAPH_RESOURCE_CAPABILITY_BYTES)
            .map(decode_graph_resource_unchecked)
    }

    /// Whether this exact class/selector/access tuple is published.
    pub fn admits_resource(
        self,
        class: GraphResourceClass,
        resource: ResourceId,
        access: GraphResourceAccess,
    ) -> bool {
        self.resources().any(|candidate| {
            candidate.class == class
                && candidate.resource == resource
                && candidate.access == access
                && candidate.support >= SupportLevel::Compiles
        })
    }
}

/// Independently decoded passive diagnostic-overview section of one complete
/// V4 capability document.
///
/// This catalog authorizes observation only. It does not imply graph access,
/// raw electrical acquisition, a GPIO lease, or permission to change pin mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticOverviewCapability<'a> {
    identity: CapabilityIdentity,
    schema_version: u16,
    support: Option<SupportLevel>,
    maximum_resources: u16,
    telemetry_request_bytes: u32,
    telemetry_event_bytes: u32,
    nominal_period_micros: u32,
    maximum_age_micros: u32,
    resource_records: &'a [u8],
}

impl<'a> DiagnosticOverviewCapability<'a> {
    /// Complete capability-document identity containing this section.
    pub const fn identity(self) -> CapabilityIdentity {
        self.identity
    }

    /// Exact `ALMOVW` record schema emitted by the provider, or zero if absent.
    pub const fn schema_version(self) -> u16 {
        self.schema_version
    }

    /// Whole-provider evidence floor, absent when no provider is composed.
    pub const fn support(self) -> Option<SupportLevel> {
        self.support
    }

    /// Whether this exact image has at least a compiling overview provider.
    pub const fn is_implemented(self) -> bool {
        matches!(
            self.support,
            Some(SupportLevel::Compiles | SupportLevel::Bench | SupportLevel::Qualified)
        )
    }

    /// Largest resource selection admitted by one subscription.
    pub const fn maximum_resources(self) -> u16 {
        self.maximum_resources
    }

    /// Permanently reserved request and event byte budgets.
    pub const fn telemetry_bytes(self) -> (u32, u32) {
        (self.telemetry_request_bytes, self.telemetry_event_bytes)
    }

    /// Nominal publication period and maximum fresh-sample age in microseconds.
    pub const fn timing_micros(self) -> (u32, u32) {
        (self.nominal_period_micros, self.maximum_age_micros)
    }

    /// Number of passive semantic resources published by this exact image.
    pub const fn resource_count(self) -> usize {
        self.resource_records.len() / DIAGNOSTIC_RESOURCE_CAPABILITY_BYTES
    }

    /// Iterate independently decoded passive observation records.
    pub fn resources(self) -> impl ExactSizeIterator<Item = DiagnosticResourceDescriptor> + 'a {
        self.resource_records
            .chunks_exact(DIAGNOSTIC_RESOURCE_CAPABILITY_BYTES)
            .map(decode_diagnostic_resource_unchecked)
    }

    /// Whether this exact resource/observation pair has an implemented path.
    pub fn admits(self, resource: ResourceId, observation: DiagnosticObservationKind) -> bool {
        self.is_implemented()
            && self.resources().any(|candidate| {
                candidate.resource == resource
                    && candidate.observation == observation
                    && candidate.support >= SupportLevel::Compiles
            })
    }
}

/// Independently decoded device-produced digital-capture section of one
/// complete V4 capability document.
///
/// This catalog authorizes only bounded evidence acquisition over exact
/// channels. It does not imply graph access, raw access to other pins, output
/// control, arming, or safety authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DigitalCaptureCapability<'a> {
    identity: CapabilityIdentity,
    schema_version: u16,
    support: Option<SupportLevel>,
    configure_flags: DigitalCaptureConfigureFlags,
    trigger_kinds: DigitalCaptureTriggerSet,
    maximum_channels: u16,
    maximum_transitions: u32,
    configure_bytes: u32,
    record_bytes: u32,
    maximum_chunk_bytes: u32,
    maximum_pretrigger_micros: u32,
    maximum_duration_micros: u32,
    arm_horizon_micros: u32,
    resource_records: &'a [u8],
}

impl<'a> DigitalCaptureCapability<'a> {
    /// Complete capability-document identity containing this section.
    pub const fn identity(self) -> CapabilityIdentity {
        self.identity
    }

    /// Exact `ALMDIG` record schema emitted by the provider, or zero if absent.
    pub const fn schema_version(self) -> u16 {
        self.schema_version
    }

    /// Whole-provider evidence floor, absent when no provider is declared.
    pub const fn support(self) -> Option<SupportLevel> {
        self.support
    }

    /// Whether this exact image has at least a compiling capture provider.
    pub const fn is_implemented(self) -> bool {
        matches!(
            self.support,
            Some(SupportLevel::Compiles | SupportLevel::Bench | SupportLevel::Qualified)
        )
    }

    /// Exact configure flags and trigger predicates accepted by the provider.
    pub const fn configure_policy(
        self,
    ) -> (DigitalCaptureConfigureFlags, DigitalCaptureTriggerSet) {
        (self.configure_flags, self.trigger_kinds)
    }

    /// Maximum selected channels and retained transitions.
    pub const fn shape_limits(self) -> (u16, u32) {
        (self.maximum_channels, self.maximum_transitions)
    }

    /// Fixed configure, retained-record, and range/chunk byte budgets.
    pub const fn byte_limits(self) -> (u32, u32, u32) {
        (
            self.configure_bytes,
            self.record_bytes,
            self.maximum_chunk_bytes,
        )
    }

    /// Maximum pretrigger, complete duration, and arm horizon in microseconds.
    pub const fn timing_micros(self) -> (u32, u32, u32) {
        (
            self.maximum_pretrigger_micros,
            self.maximum_duration_micros,
            self.arm_horizon_micros,
        )
    }

    /// Number of channel resources published by this exact image.
    pub const fn resource_count(self) -> usize {
        self.resource_records.len() / DIGITAL_CAPTURE_RESOURCE_CAPABILITY_BYTES
    }

    /// Iterate independently decoded channel records.
    pub fn resources(self) -> impl ExactSizeIterator<Item = DigitalCaptureResourceDescriptor> + 'a {
        self.resource_records
            .chunks_exact(DIGITAL_CAPTURE_RESOURCE_CAPABILITY_BYTES)
            .map(decode_digital_capture_resource_unchecked)
    }

    /// Whether this exact resource/source pair has an implemented capture path.
    pub fn admits(self, resource: ResourceId, source: DigitalCaptureSourceKind) -> bool {
        self.is_implemented()
            && self.resources().any(|candidate| {
                candidate.resource == resource
                    && candidate.source == source
                    && candidate.support >= SupportLevel::Compiles
            })
    }
}

/// Failure while independently locating and decoding the fixed graph section
/// of an untrusted complete capability document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityDocumentError {
    /// Document or a length-prefixed prefix field was truncated.
    Length,
    /// Document, graph, or diagnostic-section magic was not exact.
    Magic,
    /// Document version was not exactly V4.
    Version,
    /// Reserved bytes or a Boolean were noncanonical.
    Reserved,
    /// A caller-selected document, string, table, hotspot, or polygon bound was
    /// exceeded.
    Limit,
    /// Fixed board, memory, or core facts were internally invalid.
    Board,
    /// A non-graph section contained an invalid enum, zero-required value,
    /// malformed route, or other noncanonical local record.
    Section,
    /// Graph capacities, counts, class, access, or support were invalid.
    Graph,
    /// Passive diagnostic support, budgets, timing, or resource records were invalid.
    Diagnostic,
    /// Digital-capture support, budgets, timing, or resource records were invalid.
    Capture,
    /// One graph resource identifier was malformed.
    Resource(ResourceWireError),
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
    /// A count, string, address, or total length exceeded V4 integer bounds.
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

/// Independently validates and exposes one complete canonical V4 board
/// capability document within caller-selected bounds.
///
/// This function hashes the complete byte string and validates every V4
/// section, including sections not directly exposed by the returned view. The
/// resulting digest is content identity only: the caller must compare it with
/// an identity obtained from its authenticated device/session before treating
/// any fact as belonging to that device.
#[allow(
    clippy::too_many_lines,
    reason = "the canonical V4 section order remains one linear, auditable decoder"
)]
pub fn decode_board_capability(
    document: &[u8],
    limits: BoardCapabilityLimits,
) -> Result<BoardCapabilityView<'_>, CapabilityDocumentError> {
    let document_len = u32::try_from(document.len()).map_err(|_| CapabilityDocumentError::Limit)?;
    if document_len > limits.maximum_document_bytes {
        return Err(CapabilityDocumentError::Limit);
    }
    validate_document_header(document)?;
    let graph = decode_graph_execution(document)?;
    let diagnostic_overview = decode_diagnostic_overview(document)?;
    let digital_capture = decode_digital_capture_capability(document)?;
    if diagnostic_overview.resource_count()
        > usize::try_from(limits.maximum_records_per_section)
            .map_err(|_| CapabilityDocumentError::Limit)?
    {
        return Err(CapabilityDocumentError::Limit);
    }
    if digital_capture.resource_count()
        > usize::try_from(limits.maximum_records_per_section)
            .map_err(|_| CapabilityDocumentError::Limit)?
    {
        return Err(CapabilityDocumentError::Limit);
    }
    let mut cursor = DocumentCursor::new(document, CAPABILITY_DOCUMENT_HEADER_BYTES, limits);
    let board_id = cursor.string()?;
    let revision = cursor.string()?;
    let fixed = cursor.take(33)?;
    if fixed[31..33].iter().any(|byte| *byte != 0) {
        return Err(CapabilityDocumentError::Reserved);
    }
    let chip = chip_from_wire(fixed[0]).ok_or(CapabilityDocumentError::Board)?;
    let application_cores = fixed[1];
    let qualification = qualification_from_wire(fixed[2]).ok_or(CapabilityDocumentError::Board)?;
    let armable = bool_from_wire(fixed[3]).ok_or(CapabilityDocumentError::Reserved)?;
    let flash_bytes = read_u64(fixed, 4);
    let internal_sram_bytes = read_u64(fixed, 12);
    let psram_bytes = read_u64(fixed, 20);
    let realtime_psram_allowed =
        bool_from_wire(fixed[28]).ok_or(CapabilityDocumentError::Reserved)?;
    let service_core = fixed[29];
    let realtime_core = fixed[30];
    if application_cores < 2
        || flash_bytes == 0
        || internal_sram_bytes == 0
        || realtime_psram_allowed
        || service_core == realtime_core
        || service_core >= application_cores
        || realtime_core >= application_cores
    {
        return Err(CapabilityDocumentError::Board);
    }

    let graph_bytes = GRAPH_EXECUTOR_HEADER_BYTES
        .checked_add(
            graph
                .opcode_count()
                .checked_mul(GRAPH_OPCODE_CAPABILITY_BYTES)
                .ok_or(CapabilityDocumentError::Length)?,
        )
        .and_then(|bytes| {
            graph
                .resource_count()
                .checked_mul(GRAPH_RESOURCE_CAPABILITY_BYTES)
                .and_then(|resources| bytes.checked_add(resources))
        })
        .ok_or(CapabilityDocumentError::Length)?;
    let _ = cursor.take(graph_bytes)?;

    let diagnostic_bytes = DIAGNOSTIC_OVERVIEW_HEADER_BYTES
        .checked_add(
            diagnostic_overview
                .resource_count()
                .checked_mul(DIAGNOSTIC_RESOURCE_CAPABILITY_BYTES)
                .ok_or(CapabilityDocumentError::Length)?,
        )
        .ok_or(CapabilityDocumentError::Length)?;
    let _ = cursor.take(diagnostic_bytes)?;

    let capture_bytes = DIGITAL_CAPTURE_HEADER_BYTES
        .checked_add(
            digital_capture
                .resource_count()
                .checked_mul(DIGITAL_CAPTURE_RESOURCE_CAPABILITY_BYTES)
                .ok_or(CapabilityDocumentError::Length)?,
        )
        .ok_or(CapabilityDocumentError::Length)?;
    let _ = cursor.take(capture_bytes)?;

    let resource_count = cursor.count(limits.maximum_records_per_section)?;
    let resource_start = cursor.position();
    for _ in 0..resource_count {
        let record = cursor.take(8)?;
        let descriptor = decode_resource_descriptor(record)?;
        if descriptor.hazardous_output
            && (descriptor.owner != OwnerDomain::Realtime
                || descriptor.safe_value == SafeValue::NotApplicable)
        {
            return Err(CapabilityDocumentError::Section);
        }
    }
    let resource_records = cursor.slice_from(resource_start)?;
    validate_unique_resources(resource_records)?;
    for resource in graph.resources() {
        let descriptor = find_resource_descriptor(resource_records, resource.resource)
            .ok_or(CapabilityDocumentError::Section)?;
        if descriptor.owner != OwnerDomain::Realtime {
            return Err(CapabilityDocumentError::Section);
        }
    }
    for resource in diagnostic_overview.resources() {
        let descriptor = find_resource_descriptor(resource_records, resource.resource)
            .ok_or(CapabilityDocumentError::Diagnostic)?;
        if descriptor.owner != OwnerDomain::Realtime
            || descriptor.hazardous_output
            || descriptor.safe_value != SafeValue::HighImpedance
            || !matches!(
                descriptor.id,
                ResourceId::Gpio(_) | ResourceId::SafetyInput(_)
            )
            || resource.observation != DiagnosticObservationKind::StableBooleanInput
        {
            return Err(CapabilityDocumentError::Diagnostic);
        }
    }
    for resource in digital_capture.resources() {
        let descriptor = find_resource_descriptor(resource_records, resource.resource)
            .ok_or(CapabilityDocumentError::Capture)?;
        if descriptor.owner != OwnerDomain::Realtime
            || descriptor.hazardous_output
            || descriptor.safe_value != SafeValue::HighImpedance
            || !matches!(
                descriptor.id,
                ResourceId::Gpio(_) | ResourceId::SafetyInput(_)
            )
        {
            return Err(CapabilityDocumentError::Capture);
        }
    }

    let alias_count = cursor.count(limits.maximum_records_per_section)?;
    let alias_start = cursor.position();
    for _ in 0..alias_count {
        let _ = cursor.string()?;
        let resource = cursor.resource()?;
        require_resource(resource_records, resource)?;
    }
    let alias_records = cursor.slice_from(alias_start)?;
    validate_unique_aliases(alias_records, alias_count)?;

    let bus_count = cursor.count(limits.maximum_records_per_section)?;
    for _ in 0..bus_count {
        let bus = cursor.resource()?;
        require_resource(resource_records, bus)?;
        let metadata = cursor.take(4)?;
        let kind = bus_kind_from_wire(metadata[0]).ok_or(CapabilityDocumentError::Section)?;
        let bus_owner = owner_from_wire(metadata[1]).ok_or(CapabilityDocumentError::Section)?;
        if metadata[2..4].iter().any(|byte| *byte != 0)
            || find_resource_descriptor(resource_records, bus)
                .is_none_or(|descriptor| descriptor.owner != bus_owner)
        {
            return Err(CapabilityDocumentError::Section);
        }
        if !bus_kind_matches_resource(kind, bus) || cursor.u32()? == 0 {
            return Err(CapabilityDocumentError::Section);
        }
        let pin_count = cursor.count(limits.maximum_records_per_section)?;
        if pin_count == 0 {
            return Err(CapabilityDocumentError::Section);
        }
        for _ in 0..pin_count {
            let pin = cursor.resource()?;
            require_resource(resource_records, pin)?;
            if !matches!(pin, ResourceId::Gpio(_))
                || find_resource_descriptor(resource_records, pin)
                    .is_none_or(|descriptor| descriptor.owner != bus_owner)
            {
                return Err(CapabilityDocumentError::Section);
            }
        }
    }

    let device_count = cursor.count(limits.maximum_records_per_section)?;
    for _ in 0..device_count {
        let device = cursor.resource()?;
        require_resource(resource_records, device)?;
        if !matches!(device, ResourceId::Device(_) | ResourceId::Storage(_)) {
            return Err(CapabilityDocumentError::Section);
        }
        let metadata = cursor.take(4)?;
        let device_owner = owner_from_wire(metadata[0]).ok_or(CapabilityDocumentError::Section)?;
        if support_from_wire(metadata[1]).is_none()
            || metadata[2..4].iter().any(|byte| *byte != 0)
            || find_resource_descriptor(resource_records, device)
                .is_none_or(|descriptor| descriptor.owner != device_owner)
        {
            return Err(CapabilityDocumentError::Section);
        }
        let bus = decode_optional_resource(cursor.take(8)?)?;
        if let Some(bus) = bus {
            require_resource(resource_records, bus)?;
            if find_resource_descriptor(resource_records, bus)
                .is_none_or(|descriptor| descriptor.owner != device_owner)
            {
                return Err(CapabilityDocumentError::Section);
            }
        }
        let (route_kind, route_resource) = decode_device_route(cursor.take(8)?)?;
        if !device_route_matches_bus(route_kind, bus) {
            return Err(CapabilityDocumentError::Section);
        }
        if let Some(resource) = route_resource {
            require_resource(resource_records, resource)?;
            if find_resource_descriptor(resource_records, resource)
                .is_none_or(|descriptor| descriptor.owner != device_owner)
            {
                return Err(CapabilityDocumentError::Section);
            }
        }
        let auxiliary_count = cursor.count(limits.maximum_records_per_section)?;
        for _ in 0..auxiliary_count {
            let resource = cursor.resource()?;
            require_resource(resource_records, resource)?;
        }
    }

    let flash_region_count = cursor.count(limits.maximum_records_per_section)?;
    for _ in 0..flash_region_count {
        let _ = cursor.string()?;
        let offset = cursor.u32()?;
        let length = cursor.u32()?;
        let metadata = cursor.take(4)?;
        let writable_while_armed =
            bool_from_wire(metadata[1]).ok_or(CapabilityDocumentError::Section)?;
        if length == 0
            || offset
                .checked_add(length)
                .is_none_or(|end| u64::from(end) > flash_bytes)
            || flash_kind_from_wire(metadata[0]).is_none()
            || support_from_wire(metadata[2]).is_none()
            || metadata[3] != 0
            || armable && writable_while_armed
        {
            return Err(CapabilityDocumentError::Section);
        }
    }

    let clock_count = cursor.count(limits.maximum_records_per_section)?;
    for _ in 0..clock_count {
        let _ = cursor.string()?;
        let metadata = cursor.take(4)?;
        let has_error = bool_from_wire(metadata[3]).ok_or(CapabilityDocumentError::Section)?;
        if clock_source_from_wire(metadata[0]).is_none()
            || clock_domain_from_wire(metadata[1]).is_none()
            || support_from_wire(metadata[2]).is_none()
        {
            return Err(CapabilityDocumentError::Section);
        }
        let nominal_hz = cursor.u64()?;
        let error_ppm = cursor.u32()?;
        if nominal_hz == 0 || !has_error && error_ppm != 0 {
            return Err(CapabilityDocumentError::Section);
        }
    }

    let electrical_constraint_count = cursor.count(limits.maximum_records_per_section)?;
    for _ in 0..electrical_constraint_count {
        let _ = cursor.string()?;
        let metadata = cursor.take(4)?;
        if electrical_kind_from_wire(metadata[0]).is_none()
            || support_from_wire(metadata[1]).is_none()
            || metadata[2..4].iter().any(|byte| *byte != 0)
        {
            return Err(CapabilityDocumentError::Section);
        }
        let governed_count = cursor.count(limits.maximum_records_per_section)?;
        if governed_count == 0 {
            return Err(CapabilityDocumentError::Section);
        }
        for _ in 0..governed_count {
            let resource = cursor.resource()?;
            require_resource(resource_records, resource)?;
        }
        let _ = cursor.string()?;
    }

    let interrupt_count = cursor.count(limits.maximum_records_per_section)?;
    let interrupt_start = cursor.position();
    for interrupt_index in 0..interrupt_count {
        let source = cursor.resource()?;
        require_resource(resource_records, source)?;
        let metadata = cursor.take(4)?;
        let has_latency = bool_from_wire(metadata[3]).ok_or(CapabilityDocumentError::Section)?;
        let interrupt_owner =
            owner_from_wire(metadata[0]).ok_or(CapabilityDocumentError::Section)?;
        if interrupt_trigger_from_wire(metadata[1]).is_none()
            || support_from_wire(metadata[2]).is_none()
            || find_resource_descriptor(resource_records, source)
                .is_none_or(|descriptor| descriptor.owner != interrupt_owner)
        {
            return Err(CapabilityDocumentError::Section);
        }
        let latency = cursor.u64()?;
        if has_latency != (latency != 0) {
            return Err(CapabilityDocumentError::Section);
        }
        if document[interrupt_start..interrupt_start + interrupt_index * 16]
            .chunks_exact(16)
            .any(|record| decode_validated_resource_at(record, 0) == source)
        {
            return Err(CapabilityDocumentError::Section);
        }
    }

    let safe_output_image_count = cursor.count(limits.maximum_records_per_section)?;
    let safe_image_start = cursor.position();
    for image_index in 0..safe_output_image_count {
        let metadata = cursor.take(4)?;
        let bench_verified = bool_from_wire(metadata[1]).ok_or(CapabilityDocumentError::Section)?;
        if metadata[2..4].iter().any(|byte| *byte != 0)
            || find_resource_descriptor(resource_records, ResourceId::I2s(metadata[0]))
                .is_none_or(|descriptor| descriptor.owner != OwnerDomain::Realtime)
            || armable && !bench_verified
            || document[safe_image_start..safe_image_start + image_index * 12]
                .chunks_exact(12)
                .any(|record| record[0] == metadata[0])
        {
            return Err(CapabilityDocumentError::Section);
        }
        let defined_mask = cursor.u32()?;
        let safe_bits = cursor.u32()?;
        if safe_bits & !defined_mask != 0 {
            return Err(CapabilityDocumentError::Section);
        }
    }
    let safe_image_records = cursor.slice_from(safe_image_start)?;
    for descriptor in resource_records
        .chunks_exact(8)
        .map(decode_resource_descriptor_unchecked)
    {
        let ResourceId::I2sOut { engine, bit } = descriptor.id else {
            continue;
        };
        if !descriptor.hazardous_output || descriptor.safe_value != SafeValue::EngineImage {
            continue;
        }
        if bit >= 32
            || !safe_image_records
                .chunks_exact(12)
                .any(|record| record[0] == engine && read_u32(record, 4) & (1_u32 << bit) != 0)
        {
            return Err(CapabilityDocumentError::Section);
        }
    }

    let visual_count = cursor.count(limits.maximum_visuals)?;
    let visual_start = cursor.position();
    for _ in 0..visual_count {
        validate_visual_record(&mut cursor, resource_records)?;
    }
    let visual_records = cursor.slice_from(visual_start)?;
    validate_unique_visuals(visual_records, visual_count)?;

    let hil_requirement_count = cursor.count(limits.maximum_records_per_section)?;
    for _ in 0..hil_requirement_count {
        let _ = cursor.string()?;
        let metadata = cursor.take(4)?;
        if hil_kind_from_wire(metadata[0]).is_none()
            || qualification_from_wire(metadata[1]).is_none()
            || metadata[2..4].iter().any(|byte| *byte != 0)
        {
            return Err(CapabilityDocumentError::Section);
        }
        let resource_count = cursor.count(limits.maximum_records_per_section)?;
        for _ in 0..resource_count {
            let resource = cursor.resource()?;
            require_resource(resource_records, resource)?;
        }
    }
    if cursor.position() != document.len() {
        return Err(CapabilityDocumentError::Section);
    }

    Ok(BoardCapabilityView {
        identity: graph.identity(),
        board_id,
        revision,
        chip,
        application_cores,
        qualification,
        armable,
        flash_bytes,
        internal_sram_bytes,
        psram_bytes,
        realtime_psram_allowed,
        service_core,
        realtime_core,
        graph,
        diagnostic_overview,
        digital_capture,
        resource_records,
        alias_records,
        alias_count,
        bus_count,
        device_count,
        flash_region_count,
        clock_count,
        electrical_constraint_count,
        interrupt_count,
        safe_output_image_count,
        visual_records,
        visual_count,
        hil_requirement_count,
    })
}

/// Locates and independently decodes the graph-executor section of a complete
/// canonical V4 capability document. The graph subsection retains its exact V2
/// encoding because that subsection did not change.
///
/// The caller must still compare [`GraphExecutionCapability::identity`] with
/// the device identity it authenticated. This function hashes all supplied
/// bytes and requires the document's own total length, but a digest is content
/// identity rather than authorization.
pub fn decode_graph_execution(
    document: &[u8],
) -> Result<GraphExecutionCapability<'_>, CapabilityDocumentError> {
    if document.len() < CAPABILITY_DOCUMENT_HEADER_BYTES {
        return Err(CapabilityDocumentError::Length);
    }
    if document[..8] != DOCUMENT_MAGIC {
        return Err(CapabilityDocumentError::Magic);
    }
    if read_u16(document, 8) != CAPABILITY_DOCUMENT_VERSION {
        return Err(CapabilityDocumentError::Version);
    }
    if document[10..12].iter().any(|byte| *byte != 0)
        || usize::try_from(read_u32(document, 12)).ok() != Some(document.len())
    {
        return Err(CapabilityDocumentError::Reserved);
    }

    let mut cursor = CAPABILITY_DOCUMENT_HEADER_BYTES;
    cursor = skip_capability_string(document, cursor)?;
    cursor = skip_capability_string(document, cursor)?;
    let fixed_end = cursor
        .checked_add(33)
        .ok_or(CapabilityDocumentError::Length)?;
    let fixed = document
        .get(cursor..fixed_end)
        .ok_or(CapabilityDocumentError::Length)?;
    if !matches!(fixed[0], 1 | 2)
        || fixed[1] < 2
        || !matches!(fixed[2], 1..=5)
        || fixed[3] > 1
        || fixed[28] > 1
        || fixed[31..33].iter().any(|byte| *byte != 0)
    {
        return Err(CapabilityDocumentError::Reserved);
    }
    cursor = fixed_end;

    let graph_end = cursor
        .checked_add(GRAPH_EXECUTOR_HEADER_BYTES)
        .ok_or(CapabilityDocumentError::Length)?;
    let header = document
        .get(cursor..graph_end)
        .ok_or(CapabilityDocumentError::Length)?;
    if header[..8] != GRAPH_EXECUTOR_MAGIC {
        return Err(CapabilityDocumentError::Magic);
    }
    if header[11] != 0 || header[48..72].iter().any(|byte| *byte != 0) {
        return Err(CapabilityDocumentError::Reserved);
    }
    let support = support_from_wire(header[10]).ok_or(CapabilityDocumentError::Graph)?;
    let ir_version = read_u16(header, 8);
    let package_bytes = read_u32(header, 12);
    let maximum_nodes = read_u16(header, 16);
    let maximum_channels = read_u16(header, 18);
    let maximum_queue_items = read_u32(header, 20);
    let service_state_bytes = read_u32(header, 24);
    let realtime_state_bytes = read_u32(header, 28);
    let service_channel_bytes = read_u32(header, 32);
    let realtime_channel_bytes = read_u32(header, 36);
    let bridge_channel_bytes = read_u32(header, 40);
    let opcode_count = usize::from(read_u16(header, 44));
    let resource_count = usize::from(read_u16(header, 46));
    if ir_version == 0
        || package_bytes == 0
        || maximum_nodes == 0
        || maximum_channels == 0
        || maximum_queue_items == 0
        || service_state_bytes == 0
        || realtime_state_bytes == 0
        || service_channel_bytes == 0
        || realtime_channel_bytes == 0
        || bridge_channel_bytes == 0
        || opcode_count == 0
    {
        return Err(CapabilityDocumentError::Graph);
    }

    cursor = graph_end;
    let opcode_bytes = opcode_count
        .checked_mul(GRAPH_OPCODE_CAPABILITY_BYTES)
        .ok_or(CapabilityDocumentError::Length)?;
    let opcode_end = cursor
        .checked_add(opcode_bytes)
        .ok_or(CapabilityDocumentError::Length)?;
    let opcode_records = document
        .get(cursor..opcode_end)
        .ok_or(CapabilityDocumentError::Length)?;
    validate_graph_opcode_records(opcode_records)?;
    cursor = opcode_end;
    let resource_bytes = resource_count
        .checked_mul(GRAPH_RESOURCE_CAPABILITY_BYTES)
        .ok_or(CapabilityDocumentError::Length)?;
    let resource_end = cursor
        .checked_add(resource_bytes)
        .ok_or(CapabilityDocumentError::Length)?;
    let resource_records = document
        .get(cursor..resource_end)
        .ok_or(CapabilityDocumentError::Length)?;
    validate_graph_resource_records(opcode_records, resource_records)?;

    let digest = Sha256::digest(document);
    let mut digest_bytes = [0_u8; 32];
    digest_bytes.copy_from_slice(&digest);
    Ok(GraphExecutionCapability {
        identity: CapabilityIdentity {
            byte_len: u32::try_from(document.len()).map_err(|_| CapabilityDocumentError::Length)?,
            digest: Digest(digest_bytes),
        },
        ir_version,
        package_bytes,
        maximum_nodes,
        maximum_channels,
        maximum_queue_items,
        service_state_bytes,
        realtime_state_bytes,
        service_channel_bytes,
        realtime_channel_bytes,
        bridge_channel_bytes,
        support,
        opcode_records,
        resource_records,
    })
}

/// Locates and independently decodes the passive diagnostic-overview section
/// of a complete canonical V4 capability document.
///
/// The returned palette is observation authority only. Full board decoding
/// additionally reconciles every entry with the descriptive resource table.
pub fn decode_diagnostic_overview(
    document: &[u8],
) -> Result<DiagnosticOverviewCapability<'_>, CapabilityDocumentError> {
    validate_document_header(document)?;
    let graph = decode_graph_execution(document)?;

    let mut cursor = CAPABILITY_DOCUMENT_HEADER_BYTES;
    cursor = skip_capability_string(document, cursor)?;
    cursor = skip_capability_string(document, cursor)?;
    cursor = cursor
        .checked_add(33)
        .ok_or(CapabilityDocumentError::Length)?;
    cursor = cursor
        .checked_add(GRAPH_EXECUTOR_HEADER_BYTES)
        .and_then(|offset| {
            graph
                .opcode_count()
                .checked_mul(GRAPH_OPCODE_CAPABILITY_BYTES)
                .and_then(|bytes| offset.checked_add(bytes))
        })
        .and_then(|offset| {
            graph
                .resource_count()
                .checked_mul(GRAPH_RESOURCE_CAPABILITY_BYTES)
                .and_then(|bytes| offset.checked_add(bytes))
        })
        .ok_or(CapabilityDocumentError::Length)?;

    let header_end = cursor
        .checked_add(DIAGNOSTIC_OVERVIEW_HEADER_BYTES)
        .ok_or(CapabilityDocumentError::Length)?;
    let header = document
        .get(cursor..header_end)
        .ok_or(CapabilityDocumentError::Length)?;
    if header[..8] != DIAGNOSTIC_OVERVIEW_MAGIC {
        return Err(CapabilityDocumentError::Magic);
    }
    if header[11] != 0 || header[32..48].iter().any(|byte| *byte != 0) {
        return Err(CapabilityDocumentError::Reserved);
    }
    let schema_version = read_u16(header, 8);
    let support = match header[10] {
        0 => None,
        value => Some(support_from_wire(value).ok_or(CapabilityDocumentError::Diagnostic)?),
    };
    let maximum_resources = read_u16(header, 12);
    let resource_count = usize::from(read_u16(header, 14));
    let telemetry_request_bytes = read_u32(header, 16);
    let telemetry_event_bytes = read_u32(header, 20);
    let nominal_period_micros = read_u32(header, 24);
    let maximum_age_micros = read_u32(header, 28);
    match support {
        None if schema_version != 0
            || maximum_resources != 0
            || resource_count != 0
            || telemetry_request_bytes != 0
            || telemetry_event_bytes != 0
            || nominal_period_micros != 0
            || maximum_age_micros != 0 =>
        {
            return Err(CapabilityDocumentError::Diagnostic);
        }
        Some(_)
            if schema_version == 0
                || maximum_resources == 0
                || resource_count == 0
                || resource_count > usize::from(maximum_resources)
                || telemetry_request_bytes == 0
                || telemetry_event_bytes == 0
                || nominal_period_micros == 0
                || maximum_age_micros < nominal_period_micros =>
        {
            return Err(CapabilityDocumentError::Diagnostic);
        }
        _ => {}
    }

    let record_bytes = resource_count
        .checked_mul(DIAGNOSTIC_RESOURCE_CAPABILITY_BYTES)
        .ok_or(CapabilityDocumentError::Length)?;
    let record_end = header_end
        .checked_add(record_bytes)
        .ok_or(CapabilityDocumentError::Length)?;
    let resource_records = document
        .get(header_end..record_end)
        .ok_or(CapabilityDocumentError::Length)?;
    let mut previous = None;
    for record in resource_records.chunks_exact(DIAGNOSTIC_RESOURCE_CAPABILITY_BYTES) {
        let resource = decode_diagnostic_resource(record)?;
        if support.is_none_or(|floor| resource.support < floor)
            || previous.is_some_and(|prior| prior >= resource.resource)
        {
            return Err(CapabilityDocumentError::Diagnostic);
        }
        previous = Some(resource.resource);
    }

    Ok(DiagnosticOverviewCapability {
        identity: graph.identity(),
        schema_version,
        support,
        maximum_resources,
        telemetry_request_bytes,
        telemetry_event_bytes,
        nominal_period_micros,
        maximum_age_micros,
        resource_records,
    })
}

/// Locates and independently decodes the device-produced digital-capture
/// section of a complete canonical V4 capability document.
///
/// Full board decoding additionally reconciles every channel with the
/// descriptive resource table. This narrower view remains useful for bounded
/// preflight before a UI allocates channel state.
pub fn decode_digital_capture_capability(
    document: &[u8],
) -> Result<DigitalCaptureCapability<'_>, CapabilityDocumentError> {
    validate_document_header(document)?;
    let graph = decode_graph_execution(document)?;
    let overview = decode_diagnostic_overview(document)?;

    let mut cursor = CAPABILITY_DOCUMENT_HEADER_BYTES;
    cursor = skip_capability_string(document, cursor)?;
    cursor = skip_capability_string(document, cursor)?;
    cursor = cursor
        .checked_add(33)
        .ok_or(CapabilityDocumentError::Length)?;
    cursor = cursor
        .checked_add(GRAPH_EXECUTOR_HEADER_BYTES)
        .and_then(|offset| {
            graph
                .opcode_count()
                .checked_mul(GRAPH_OPCODE_CAPABILITY_BYTES)
                .and_then(|bytes| offset.checked_add(bytes))
        })
        .and_then(|offset| {
            graph
                .resource_count()
                .checked_mul(GRAPH_RESOURCE_CAPABILITY_BYTES)
                .and_then(|bytes| offset.checked_add(bytes))
        })
        .and_then(|offset| offset.checked_add(DIAGNOSTIC_OVERVIEW_HEADER_BYTES))
        .and_then(|offset| {
            overview
                .resource_count()
                .checked_mul(DIAGNOSTIC_RESOURCE_CAPABILITY_BYTES)
                .and_then(|bytes| offset.checked_add(bytes))
        })
        .ok_or(CapabilityDocumentError::Length)?;

    let header_end = cursor
        .checked_add(DIGITAL_CAPTURE_HEADER_BYTES)
        .ok_or(CapabilityDocumentError::Length)?;
    let header = document
        .get(cursor..header_end)
        .ok_or(CapabilityDocumentError::Length)?;
    if header[..8] != DIGITAL_CAPTURE_MAGIC {
        return Err(CapabilityDocumentError::Magic);
    }
    if header[18..20].iter().any(|byte| *byte != 0) || header[48..64].iter().any(|byte| *byte != 0)
    {
        return Err(CapabilityDocumentError::Reserved);
    }
    let schema_version = read_u16(header, 8);
    let support = match header[10] {
        0 => None,
        value => Some(support_from_wire(value).ok_or(CapabilityDocumentError::Capture)?),
    };
    let trigger_kinds = DigitalCaptureTriggerSet(header[11]);
    let configure_flags = DigitalCaptureConfigureFlags(read_u16(header, 12));
    let maximum_channels = read_u16(header, 14);
    let resource_count = usize::from(read_u16(header, 16));
    let maximum_transitions = read_u32(header, 20);
    let configure_bytes = read_u32(header, 24);
    let record_bytes = read_u32(header, 28);
    let maximum_chunk_bytes = read_u32(header, 32);
    let maximum_pretrigger_micros = read_u32(header, 36);
    let maximum_duration_micros = read_u32(header, 40);
    let arm_horizon_micros = read_u32(header, 44);
    match support {
        None if schema_version != 0
            || trigger_kinds.0 != 0
            || configure_flags.0 != 0
            || maximum_channels != 0
            || resource_count != 0
            || maximum_transitions != 0
            || configure_bytes != 0
            || record_bytes != 0
            || maximum_chunk_bytes != 0
            || maximum_pretrigger_micros != 0
            || maximum_duration_micros != 0
            || arm_horizon_micros != 0 =>
        {
            return Err(CapabilityDocumentError::Capture);
        }
        Some(_)
            if schema_version == 0
                || configure_flags.0 & !DigitalCaptureConfigureFlags::KNOWN != 0
                || !configure_flags.contains(DigitalCaptureConfigureFlags::EDGE_TIMESTAMPS)
                || trigger_kinds.0 == 0
                || trigger_kinds.0 & !DigitalCaptureTriggerSet::KNOWN != 0
                || maximum_channels == 0
                || resource_count == 0
                || resource_count > usize::from(maximum_channels)
                || maximum_transitions == 0
                || configure_bytes == 0
                || record_bytes == 0
                || maximum_chunk_bytes == 0
                || maximum_chunk_bytes > record_bytes
                || maximum_pretrigger_micros > maximum_duration_micros
                || maximum_duration_micros == 0
                || arm_horizon_micros == 0 =>
        {
            return Err(CapabilityDocumentError::Capture);
        }
        _ => {}
    }

    let records_bytes = resource_count
        .checked_mul(DIGITAL_CAPTURE_RESOURCE_CAPABILITY_BYTES)
        .ok_or(CapabilityDocumentError::Length)?;
    let records_end = header_end
        .checked_add(records_bytes)
        .ok_or(CapabilityDocumentError::Length)?;
    let resource_records = document
        .get(header_end..records_end)
        .ok_or(CapabilityDocumentError::Length)?;
    let mut previous = None;
    let mut has_software_source = false;
    for record in resource_records.chunks_exact(DIGITAL_CAPTURE_RESOURCE_CAPABILITY_BYTES) {
        let resource = decode_digital_capture_resource(record)?;
        has_software_source |= resource.source == DigitalCaptureSourceKind::Software;
        if support.is_none_or(|floor| resource.support < floor)
            || previous.is_some_and(|prior| prior >= resource.resource)
        {
            return Err(CapabilityDocumentError::Capture);
        }
        previous = Some(resource.resource);
    }
    if configure_flags.contains(DigitalCaptureConfigureFlags::ALLOW_SOFTWARE) != has_software_source
    {
        return Err(CapabilityDocumentError::Capture);
    }

    Ok(DigitalCaptureCapability {
        identity: graph.identity(),
        schema_version,
        support,
        configure_flags,
        trigger_kinds,
        maximum_channels,
        maximum_transitions,
        configure_bytes,
        record_bytes,
        maximum_chunk_bytes,
        maximum_pretrigger_micros,
        maximum_duration_micros,
        arm_horizon_micros,
        resource_records,
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

    /// Decodes only the exact V4 representation.
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
    /// Version was not exactly V4.
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
    write_graph_executor(sink, package.graph)?;
    write_diagnostic_overview(sink, package.diagnostic_overview)?;
    write_digital_capture(sink, package.digital_capture)?;

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

fn write_graph_executor<S: ByteSink>(
    sink: &mut S,
    graph: GraphExecutorDescriptor<'_>,
) -> Result<(), CapabilityError> {
    let opcode_count = u16::try_from(graph.opcodes.len()).map_err(|_| CapabilityError::Length)?;
    let resource_count =
        u16::try_from(graph.resources.len()).map_err(|_| CapabilityError::Length)?;
    let mut header = [0_u8; GRAPH_EXECUTOR_HEADER_BYTES];
    header[..8].copy_from_slice(&GRAPH_EXECUTOR_MAGIC);
    header[8..10].copy_from_slice(&graph.ir_version.to_le_bytes());
    header[10] = support(graph.support);
    // Byte 11 and bytes 48..72 are reserved zero.
    header[12..16].copy_from_slice(&graph.package_bytes.to_le_bytes());
    header[16..18].copy_from_slice(&graph.maximum_nodes.to_le_bytes());
    header[18..20].copy_from_slice(&graph.maximum_channels.to_le_bytes());
    header[20..24].copy_from_slice(&graph.maximum_queue_items.to_le_bytes());
    header[24..28].copy_from_slice(&graph.service_state_bytes.to_le_bytes());
    header[28..32].copy_from_slice(&graph.realtime_state_bytes.to_le_bytes());
    header[32..36].copy_from_slice(&graph.service_channel_bytes.to_le_bytes());
    header[36..40].copy_from_slice(&graph.realtime_channel_bytes.to_le_bytes());
    header[40..44].copy_from_slice(&graph.bridge_channel_bytes.to_le_bytes());
    header[44..46].copy_from_slice(&opcode_count.to_le_bytes());
    header[46..48].copy_from_slice(&resource_count.to_le_bytes());
    sink.write(&header)?;
    for opcode in graph.opcodes {
        let mut encoded = [0_u8; GRAPH_OPCODE_CAPABILITY_BYTES];
        encoded[0] = opcode.opcode;
        encoded[1] = owner(opcode.domain);
        encoded[2] = support(opcode.support);
        encoded[3] = opcode.resource_access.map_or(0, graph_resource_access);
        encoded[4..8].copy_from_slice(
            &opcode
                .resource_class
                .map_or(0, GraphResourceClass::get)
                .to_le_bytes(),
        );
        // Bytes 8..12 are reserved zero.
        sink.write(&encoded)?;
    }
    for resource in graph.resources {
        let mut encoded = [0_u8; GRAPH_RESOURCE_CAPABILITY_BYTES];
        encoded[..4].copy_from_slice(&encode_resource_id(resource.resource));
        encoded[4] = graph_resource_access(resource.access);
        encoded[5] = support(resource.support);
        // Bytes 6..8 are reserved zero.
        encoded[8..12].copy_from_slice(&resource.class.get().to_le_bytes());
        sink.write(&encoded)?;
    }
    Ok(())
}

fn write_diagnostic_overview<S: ByteSink>(
    sink: &mut S,
    overview: DiagnosticOverviewDescriptor<'_>,
) -> Result<(), CapabilityError> {
    let resource_count =
        u16::try_from(overview.resources.len()).map_err(|_| CapabilityError::Length)?;
    let mut header = [0_u8; DIAGNOSTIC_OVERVIEW_HEADER_BYTES];
    header[..8].copy_from_slice(&DIAGNOSTIC_OVERVIEW_MAGIC);
    header[8..10].copy_from_slice(&overview.schema_version.to_le_bytes());
    header[10] = overview.support.map_or(0, support);
    // Byte 11 and bytes 32..48 are reserved zero.
    header[12..14].copy_from_slice(&overview.maximum_resources.to_le_bytes());
    header[14..16].copy_from_slice(&resource_count.to_le_bytes());
    header[16..20].copy_from_slice(&overview.telemetry_request_bytes.to_le_bytes());
    header[20..24].copy_from_slice(&overview.telemetry_event_bytes.to_le_bytes());
    header[24..28].copy_from_slice(&overview.nominal_period_micros.to_le_bytes());
    header[28..32].copy_from_slice(&overview.maximum_age_micros.to_le_bytes());
    sink.write(&header)?;
    for resource in overview.resources {
        let mut encoded = [0_u8; DIAGNOSTIC_RESOURCE_CAPABILITY_BYTES];
        encoded[..4].copy_from_slice(&encode_resource_id(resource.resource));
        encoded[4] = diagnostic_observation_kind(resource.observation);
        encoded[5] = support(resource.support);
        // Bytes 6..12 are reserved zero.
        sink.write(&encoded)?;
    }
    Ok(())
}

fn write_digital_capture<S: ByteSink>(
    sink: &mut S,
    capture: DigitalCaptureDescriptor<'_>,
) -> Result<(), CapabilityError> {
    let resource_count =
        u16::try_from(capture.resources.len()).map_err(|_| CapabilityError::Length)?;
    let mut header = [0_u8; DIGITAL_CAPTURE_HEADER_BYTES];
    header[..8].copy_from_slice(&DIGITAL_CAPTURE_MAGIC);
    header[8..10].copy_from_slice(&capture.schema_version.to_le_bytes());
    header[10] = capture.support.map_or(0, support);
    header[11] = capture.trigger_kinds.0;
    header[12..14].copy_from_slice(&capture.configure_flags.0.to_le_bytes());
    header[14..16].copy_from_slice(&capture.maximum_channels.to_le_bytes());
    header[16..18].copy_from_slice(&resource_count.to_le_bytes());
    // Bytes 18..20 and 48..64 are reserved zero.
    header[20..24].copy_from_slice(&capture.maximum_transitions.to_le_bytes());
    header[24..28].copy_from_slice(&capture.configure_bytes.to_le_bytes());
    header[28..32].copy_from_slice(&capture.record_bytes.to_le_bytes());
    header[32..36].copy_from_slice(&capture.maximum_chunk_bytes.to_le_bytes());
    header[36..40].copy_from_slice(&capture.maximum_pretrigger_micros.to_le_bytes());
    header[40..44].copy_from_slice(&capture.maximum_duration_micros.to_le_bytes());
    header[44..48].copy_from_slice(&capture.arm_horizon_micros.to_le_bytes());
    sink.write(&header)?;
    for resource in capture.resources {
        let mut encoded = [0_u8; DIGITAL_CAPTURE_RESOURCE_CAPABILITY_BYTES];
        encoded[..4].copy_from_slice(&encode_resource_id(resource.resource));
        encoded[4] = digital_capture_source(resource.source);
        encoded[5] = support(resource.support);
        // Bytes 6..12 are reserved zero.
        sink.write(&encoded)?;
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
    sink.write(&encode_resource_id(resource))
}

/// Encodes the shared canonical four-byte resource identifier.
pub const fn encode_resource_id(resource: ResourceId) -> [u8; 4] {
    let (kind, first, second) = match resource {
        ResourceId::Gpio(index) => (1, 0, index as u16),
        ResourceId::I2sOut { engine, bit } => (2, engine, bit as u16),
        ResourceId::Adc { unit, channel } => (3, unit, channel as u16),
        ResourceId::Timer { group, index } => (4, group, index as u16),
        ResourceId::I2s(index) => (5, 0, index as u16),
        ResourceId::Rmt(index) => (6, 0, index as u16),
        ResourceId::TimedOutput { engine, channel } => (7, engine, channel as u16),
        ResourceId::I2c(index) => (8, 0, index as u16),
        ResourceId::Spi(index) => (9, 0, index as u16),
        ResourceId::Uart(index) => (10, 0, index as u16),
        ResourceId::Pcnt(index) => (11, 0, index as u16),
        ResourceId::Dma(index) => (12, 0, index as u16),
        ResourceId::Twai(index) => (13, 0, index as u16),
        ResourceId::Storage(index) => (14, 0, index as u16),
        ResourceId::Radio(index) => (15, 0, index as u16),
        ResourceId::SafetyInput(index) => (16, 0, index as u16),
        ResourceId::Device(index) => (17, 0, index),
    };
    [kind, first, second as u8, (second >> 8) as u8]
}

/// Decodes the shared canonical four-byte resource identifier.
pub fn decode_resource_id(encoded: &[u8]) -> Result<ResourceId, ResourceWireError> {
    if encoded.len() != 4 {
        return Err(ResourceWireError::Length);
    }
    let first = encoded[1];
    let second = u16::from_le_bytes([encoded[2], encoded[3]]);
    let single = u8::try_from(second).ok();
    let resource = match encoded[0] {
        1 if first == 0 => ResourceId::Gpio(single.ok_or(ResourceWireError::Index)?),
        2 => ResourceId::I2sOut {
            engine: first,
            bit: single.ok_or(ResourceWireError::Index)?,
        },
        3 => ResourceId::Adc {
            unit: first,
            channel: single.ok_or(ResourceWireError::Index)?,
        },
        4 => ResourceId::Timer {
            group: first,
            index: single.ok_or(ResourceWireError::Index)?,
        },
        5 if first == 0 => ResourceId::I2s(single.ok_or(ResourceWireError::Index)?),
        6 if first == 0 => ResourceId::Rmt(single.ok_or(ResourceWireError::Index)?),
        7 => ResourceId::TimedOutput {
            engine: first,
            channel: single.ok_or(ResourceWireError::Index)?,
        },
        8 if first == 0 => ResourceId::I2c(single.ok_or(ResourceWireError::Index)?),
        9 if first == 0 => ResourceId::Spi(single.ok_or(ResourceWireError::Index)?),
        10 if first == 0 => ResourceId::Uart(single.ok_or(ResourceWireError::Index)?),
        11 if first == 0 => ResourceId::Pcnt(single.ok_or(ResourceWireError::Index)?),
        12 if first == 0 => ResourceId::Dma(single.ok_or(ResourceWireError::Index)?),
        13 if first == 0 => ResourceId::Twai(single.ok_or(ResourceWireError::Index)?),
        14 if first == 0 => ResourceId::Storage(single.ok_or(ResourceWireError::Index)?),
        15 if first == 0 => ResourceId::Radio(single.ok_or(ResourceWireError::Index)?),
        16 if first == 0 => ResourceId::SafetyInput(single.ok_or(ResourceWireError::Index)?),
        17 if first == 0 => ResourceId::Device(second),
        1..=17 => return Err(ResourceWireError::Reserved),
        value => return Err(ResourceWireError::Kind(value)),
    };
    if encode_resource_id(resource) != encoded {
        return Err(ResourceWireError::Noncanonical);
    }
    Ok(resource)
}

/// Canonical resource-ID decoding rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceWireError {
    /// Resource IDs are exactly four bytes.
    Length,
    /// The resource-kind byte is unknown.
    Kind(u8),
    /// A one-byte resource index did not fit.
    Index,
    /// A reserved coordinate was nonzero.
    Reserved,
    /// A valid value had a noncanonical representation.
    Noncanonical,
}

#[derive(Clone, Copy)]
struct DocumentCursor<'a> {
    document: &'a [u8],
    offset: usize,
    limits: BoardCapabilityLimits,
}

impl<'a> DocumentCursor<'a> {
    const fn new(document: &'a [u8], offset: usize, limits: BoardCapabilityLimits) -> Self {
        Self {
            document,
            offset,
            limits,
        }
    }

    const fn position(self) -> usize {
        self.offset
    }

    fn take(&mut self, byte_len: usize) -> Result<&'a [u8], CapabilityDocumentError> {
        let end = self
            .offset
            .checked_add(byte_len)
            .ok_or(CapabilityDocumentError::Length)?;
        let value = self
            .document
            .get(self.offset..end)
            .ok_or(CapabilityDocumentError::Length)?;
        self.offset = end;
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32, CapabilityDocumentError> {
        Ok(read_u32(self.take(4)?, 0))
    }

    fn u64(&mut self) -> Result<u64, CapabilityDocumentError> {
        Ok(read_u64(self.take(8)?, 0))
    }

    fn count(&mut self, maximum: u32) -> Result<usize, CapabilityDocumentError> {
        let count = self.u32()?;
        if count > maximum {
            return Err(CapabilityDocumentError::Limit);
        }
        usize::try_from(count).map_err(|_| CapabilityDocumentError::Limit)
    }

    fn string(&mut self) -> Result<&'a str, CapabilityDocumentError> {
        let byte_len = self.count(self.limits.maximum_string_bytes)?;
        let value = self.take(byte_len)?;
        if value.is_empty() {
            return Err(CapabilityDocumentError::Section);
        }
        core::str::from_utf8(value).map_err(|_| CapabilityDocumentError::Section)
    }

    fn resource(&mut self) -> Result<ResourceId, CapabilityDocumentError> {
        decode_resource_id(self.take(4)?).map_err(CapabilityDocumentError::Resource)
    }

    fn slice_from(self, start: usize) -> Result<&'a [u8], CapabilityDocumentError> {
        self.document
            .get(start..self.offset)
            .ok_or(CapabilityDocumentError::Length)
    }
}

fn validate_document_header(document: &[u8]) -> Result<(), CapabilityDocumentError> {
    if document.len() < CAPABILITY_DOCUMENT_HEADER_BYTES {
        return Err(CapabilityDocumentError::Length);
    }
    if document[..8] != DOCUMENT_MAGIC {
        return Err(CapabilityDocumentError::Magic);
    }
    if read_u16(document, 8) != CAPABILITY_DOCUMENT_VERSION {
        return Err(CapabilityDocumentError::Version);
    }
    if document[10..12].iter().any(|byte| *byte != 0)
        || usize::try_from(read_u32(document, 12)).ok() != Some(document.len())
    {
        return Err(CapabilityDocumentError::Reserved);
    }
    Ok(())
}

fn decode_resource_descriptor(
    record: &[u8],
) -> Result<ResourceDescriptor, CapabilityDocumentError> {
    if record.len() != 8 || record[7] != 0 {
        return Err(CapabilityDocumentError::Reserved);
    }
    Ok(ResourceDescriptor {
        id: decode_resource_id(&record[..4]).map_err(CapabilityDocumentError::Resource)?,
        owner: owner_from_wire(record[4]).ok_or(CapabilityDocumentError::Section)?,
        safe_value: safe_value_from_wire(record[5]).ok_or(CapabilityDocumentError::Section)?,
        hazardous_output: bool_from_wire(record[6]).ok_or(CapabilityDocumentError::Section)?,
    })
}

fn decode_resource_descriptor_unchecked(record: &[u8]) -> ResourceDescriptor {
    decode_resource_descriptor(record)
        .expect("board capability resource was independently validated")
}

fn decode_diagnostic_resource(
    record: &[u8],
) -> Result<DiagnosticResourceDescriptor, CapabilityDocumentError> {
    if record.len() != DIAGNOSTIC_RESOURCE_CAPABILITY_BYTES
        || record[6..12].iter().any(|byte| *byte != 0)
    {
        return Err(CapabilityDocumentError::Reserved);
    }
    Ok(DiagnosticResourceDescriptor {
        resource: decode_resource_id(&record[..4]).map_err(CapabilityDocumentError::Resource)?,
        observation: diagnostic_observation_kind_from_wire(record[4])
            .ok_or(CapabilityDocumentError::Diagnostic)?,
        support: support_from_wire(record[5]).ok_or(CapabilityDocumentError::Diagnostic)?,
    })
}

fn decode_diagnostic_resource_unchecked(record: &[u8]) -> DiagnosticResourceDescriptor {
    decode_diagnostic_resource(record)
        .expect("diagnostic capability resource was independently validated")
}

fn decode_digital_capture_resource(
    record: &[u8],
) -> Result<DigitalCaptureResourceDescriptor, CapabilityDocumentError> {
    if record.len() != DIGITAL_CAPTURE_RESOURCE_CAPABILITY_BYTES
        || record[6..12].iter().any(|byte| *byte != 0)
    {
        return Err(CapabilityDocumentError::Reserved);
    }
    Ok(DigitalCaptureResourceDescriptor {
        resource: decode_resource_id(&record[..4]).map_err(CapabilityDocumentError::Resource)?,
        source: digital_capture_source_from_wire(record[4])
            .ok_or(CapabilityDocumentError::Capture)?,
        support: support_from_wire(record[5]).ok_or(CapabilityDocumentError::Capture)?,
    })
}

fn decode_digital_capture_resource_unchecked(record: &[u8]) -> DigitalCaptureResourceDescriptor {
    decode_digital_capture_resource(record)
        .expect("digital-capture capability resource was independently validated")
}

fn resource_records_contain(records: &[u8], resource: ResourceId) -> bool {
    find_resource_descriptor(records, resource).is_some()
}

fn find_resource_descriptor(records: &[u8], resource: ResourceId) -> Option<ResourceDescriptor> {
    records
        .chunks_exact(8)
        .map(decode_resource_descriptor_unchecked)
        .find(|candidate| candidate.id == resource)
}

fn require_resource(records: &[u8], resource: ResourceId) -> Result<(), CapabilityDocumentError> {
    if !resource_records_contain(records, resource) {
        return Err(CapabilityDocumentError::Section);
    }
    Ok(())
}

fn validate_unique_resources(records: &[u8]) -> Result<(), CapabilityDocumentError> {
    for (index, record) in records.chunks_exact(8).enumerate() {
        let resource = decode_resource_descriptor_unchecked(record).id;
        if records[..index * 8]
            .chunks_exact(8)
            .map(decode_resource_descriptor_unchecked)
            .any(|previous| previous.id == resource)
        {
            return Err(CapabilityDocumentError::Section);
        }
    }
    Ok(())
}

fn validate_unique_aliases(records: &[u8], count: usize) -> Result<(), CapabilityDocumentError> {
    let aliases = CapabilityAliasIter {
        records,
        cursor: 0,
        remaining: count,
    };
    for (index, alias) in aliases.enumerate() {
        if (CapabilityAliasIter {
            records,
            cursor: 0,
            remaining: count,
        })
        .take(index)
        .any(|previous| previous.name == alias.name)
        {
            return Err(CapabilityDocumentError::Section);
        }
    }
    Ok(())
}

fn decode_optional_resource(encoded: &[u8]) -> Result<Option<ResourceId>, CapabilityDocumentError> {
    if encoded.len() != 8 {
        return Err(CapabilityDocumentError::Length);
    }
    match encoded[0] {
        0 if encoded.iter().all(|byte| *byte == 0) => Ok(None),
        1 if encoded[1..4].iter().all(|byte| *byte == 0) => decode_resource_id(&encoded[4..8])
            .map(Some)
            .map_err(CapabilityDocumentError::Resource),
        _ => Err(CapabilityDocumentError::Section),
    }
}

fn decode_device_route(
    encoded: &[u8],
) -> Result<(u8, Option<ResourceId>), CapabilityDocumentError> {
    if encoded.len() != 8 || encoded[1..4].iter().any(|byte| *byte != 0) {
        return Err(CapabilityDocumentError::Section);
    }
    match encoded[0] {
        1 | 4 if encoded[4..8].iter().all(|byte| *byte == 0) => Ok((encoded[0], None)),
        2 if encoded[4] <= 0x7f && encoded[5..8].iter().all(|byte| *byte == 0) => {
            Ok((encoded[0], None))
        }
        3 => decode_resource_id(&encoded[4..8])
            .map(|resource| (encoded[0], Some(resource)))
            .map_err(CapabilityDocumentError::Resource),
        _ => Err(CapabilityDocumentError::Section),
    }
}

const fn device_route_matches_bus(route_kind: u8, bus: Option<ResourceId>) -> bool {
    matches!(
        (route_kind, bus),
        (1, None)
            | (2, Some(ResourceId::I2c(_)))
            | (3, Some(ResourceId::Spi(_)))
            | (4, Some(ResourceId::Uart(_)))
    )
}

fn decode_validated_resource_at(records: &[u8], offset: usize) -> ResourceId {
    decode_resource_id(&records[offset..offset + 4])
        .expect("validated capability resource is canonical")
}

fn validate_visual_record(
    cursor: &mut DocumentCursor<'_>,
    resource_records: &[u8],
) -> Result<(), CapabilityDocumentError> {
    let _ = cursor.string()?;
    let _ = cursor.string()?;
    let _ = cursor.string()?;
    if cursor.u32()? == 0 || cursor.u32()? == 0 {
        return Err(CapabilityDocumentError::Section);
    }
    if cursor.take(32)?.iter().all(|byte| *byte == 0) {
        return Err(CapabilityDocumentError::Section);
    }
    let _ = cursor.string()?;
    let _ = cursor.string()?;
    let hotspot_count = cursor.count(cursor.limits.maximum_hotspots_per_visual)?;
    if hotspot_count == 0 {
        return Err(CapabilityDocumentError::Section);
    }
    let hotspot_start = cursor.position();
    for _ in 0..hotspot_count {
        let _ = cursor.string()?;
        let resource = cursor.resource()?;
        require_resource(resource_records, resource)?;
        let point_count = cursor.count(cursor.limits.maximum_points_per_hotspot)?;
        if point_count < 3 {
            return Err(CapabilityDocumentError::Section);
        }
        for _ in 0..point_count {
            let point = cursor.take(4)?;
            if read_u16(point, 0) > 10_000 || read_u16(point, 2) > 10_000 {
                return Err(CapabilityDocumentError::Section);
            }
        }
    }
    validate_unique_hotspots(cursor.slice_from(hotspot_start)?, hotspot_count)
}

fn validate_unique_hotspots(records: &[u8], count: usize) -> Result<(), CapabilityDocumentError> {
    let hotspots = CapabilityHotspotIter {
        records,
        cursor: 0,
        remaining: count,
    };
    for (index, hotspot) in hotspots.enumerate() {
        if (CapabilityHotspotIter {
            records,
            cursor: 0,
            remaining: count,
        })
        .take(index)
        .any(|previous| previous.id() == hotspot.id())
        {
            return Err(CapabilityDocumentError::Section);
        }
    }
    Ok(())
}

fn validate_unique_visuals(records: &[u8], count: usize) -> Result<(), CapabilityDocumentError> {
    let visuals = CapabilityVisualIter {
        records,
        cursor: 0,
        remaining: count,
    };
    for (index, visual) in visuals.enumerate() {
        if (CapabilityVisualIter {
            records,
            cursor: 0,
            remaining: count,
        })
        .take(index)
        .any(|previous| previous.id() == visual.id())
        {
            return Err(CapabilityDocumentError::Section);
        }
    }
    Ok(())
}

fn decode_validated_string<'a>(records: &'a [u8], cursor: &mut usize) -> &'a str {
    let length = usize::try_from(read_u32(records, *cursor))
        .expect("validated capability string length fits usize");
    *cursor += 4;
    let end = *cursor + length;
    let value =
        core::str::from_utf8(&records[*cursor..end]).expect("validated capability string is UTF-8");
    *cursor = end;
    value
}

fn decode_validated_resource(records: &[u8], cursor: &mut usize) -> ResourceId {
    let end = *cursor + 4;
    let resource = decode_resource_id(&records[*cursor..end])
        .expect("validated capability resource is canonical");
    *cursor = end;
    resource
}

fn decode_validated_visual<'a>(records: &'a [u8], cursor: &mut usize) -> CapabilityVisual<'a> {
    let id = decode_validated_string(records, cursor);
    let asset_path = decode_validated_string(records, cursor);
    let media_type = decode_validated_string(records, cursor);
    let pixel_width = read_u32(records, *cursor);
    *cursor += 4;
    let pixel_height = read_u32(records, *cursor);
    *cursor += 4;
    let mut asset_digest = [0_u8; 32];
    asset_digest.copy_from_slice(&records[*cursor..*cursor + 32]);
    *cursor += 32;
    let license = decode_validated_string(records, cursor);
    let attribution = decode_validated_string(records, cursor);
    let hotspot_count =
        usize::try_from(read_u32(records, *cursor)).expect("validated hotspot count fits usize");
    *cursor += 4;
    let hotspot_start = *cursor;
    for _ in 0..hotspot_count {
        let _ = decode_validated_hotspot(records, cursor);
    }
    CapabilityVisual {
        id,
        asset_path,
        media_type,
        pixel_width,
        pixel_height,
        asset_digest: Digest(asset_digest),
        license,
        attribution,
        hotspot_records: &records[hotspot_start..*cursor],
        hotspot_count,
    }
}

fn decode_validated_hotspot<'a>(records: &'a [u8], cursor: &mut usize) -> CapabilityHotspot<'a> {
    let id = decode_validated_string(records, cursor);
    let resource = decode_validated_resource(records, cursor);
    let point_count =
        usize::try_from(read_u32(records, *cursor)).expect("validated polygon count fits usize");
    *cursor += 4;
    let point_bytes = point_count * 4;
    let end = *cursor + point_bytes;
    let point_records = &records[*cursor..end];
    *cursor = end;
    CapabilityHotspot {
        id,
        resource,
        point_records,
    }
}

fn skip_capability_string(
    document: &[u8],
    offset: usize,
) -> Result<usize, CapabilityDocumentError> {
    let length_end = offset
        .checked_add(4)
        .ok_or(CapabilityDocumentError::Length)?;
    let length_prefix = document
        .get(offset..length_end)
        .ok_or(CapabilityDocumentError::Length)?;
    let length =
        usize::try_from(read_u32(length_prefix, 0)).map_err(|_| CapabilityDocumentError::Length)?;
    let end = length_end
        .checked_add(length)
        .ok_or(CapabilityDocumentError::Length)?;
    let value = document
        .get(length_end..end)
        .ok_or(CapabilityDocumentError::Length)?;
    if value.is_empty() || core::str::from_utf8(value).is_err() {
        return Err(CapabilityDocumentError::Reserved);
    }
    Ok(end)
}

fn validate_graph_opcode_records(records: &[u8]) -> Result<(), CapabilityDocumentError> {
    let mut previous = 0_u8;
    for record in records.chunks_exact(GRAPH_OPCODE_CAPABILITY_BYTES) {
        let opcode = decode_graph_opcode(record)?;
        if opcode.opcode <= previous {
            return Err(CapabilityDocumentError::Graph);
        }
        previous = opcode.opcode;
    }
    Ok(())
}

fn validate_graph_resource_records(
    opcodes: &[u8],
    resources: &[u8],
) -> Result<(), CapabilityDocumentError> {
    for (index, record) in resources
        .chunks_exact(GRAPH_RESOURCE_CAPABILITY_BYTES)
        .enumerate()
    {
        let resource = decode_graph_resource(record)?;
        if resource.class.get() == 0
            || !opcodes
                .chunks_exact(GRAPH_OPCODE_CAPABILITY_BYTES)
                .map(decode_graph_opcode_unchecked)
                .any(|opcode| {
                    opcode.domain == OwnerDomain::Realtime
                        && opcode.resource_class == Some(resource.class)
                        && opcode.resource_access == Some(resource.access)
                })
        {
            return Err(CapabilityDocumentError::Graph);
        }
        for previous in resources[..index * GRAPH_RESOURCE_CAPABILITY_BYTES]
            .chunks_exact(GRAPH_RESOURCE_CAPABILITY_BYTES)
        {
            let previous = decode_graph_resource_unchecked(previous);
            if previous.class == resource.class && previous.resource == resource.resource {
                return Err(CapabilityDocumentError::Graph);
            }
        }
    }
    Ok(())
}

fn decode_graph_opcode(encoded: &[u8]) -> Result<GraphOpcodeDescriptor, CapabilityDocumentError> {
    if encoded.len() != GRAPH_OPCODE_CAPABILITY_BYTES
        || encoded[8..12].iter().any(|byte| *byte != 0)
    {
        return Err(CapabilityDocumentError::Reserved);
    }
    let resource_class = read_u32(encoded, 4);
    let resource_access = graph_resource_access_from_wire(encoded[3]);
    if encoded[0] == 0
        || resource_access.is_none() != (resource_class == 0)
        || encoded[3] != 0 && resource_access.is_none()
    {
        return Err(CapabilityDocumentError::Graph);
    }
    Ok(GraphOpcodeDescriptor {
        opcode: encoded[0],
        domain: owner_from_wire(encoded[1]).ok_or(CapabilityDocumentError::Graph)?,
        support: support_from_wire(encoded[2]).ok_or(CapabilityDocumentError::Graph)?,
        resource_class: (resource_class != 0).then_some(GraphResourceClass::new(resource_class)),
        resource_access,
    })
}

fn decode_graph_opcode_unchecked(encoded: &[u8]) -> GraphOpcodeDescriptor {
    decode_graph_opcode(encoded).expect("graph capability view was independently validated")
}

fn decode_graph_resource(
    encoded: &[u8],
) -> Result<GraphResourceDescriptor, CapabilityDocumentError> {
    if encoded.len() != GRAPH_RESOURCE_CAPABILITY_BYTES
        || encoded[6..8].iter().any(|byte| *byte != 0)
    {
        return Err(CapabilityDocumentError::Reserved);
    }
    Ok(GraphResourceDescriptor {
        resource: decode_resource_id(&encoded[..4]).map_err(CapabilityDocumentError::Resource)?,
        class: GraphResourceClass::new(read_u32(encoded, 8)),
        access: graph_resource_access_from_wire(encoded[4])
            .ok_or(CapabilityDocumentError::Graph)?,
        support: support_from_wire(encoded[5]).ok_or(CapabilityDocumentError::Graph)?,
    })
}

fn decode_graph_resource_unchecked(encoded: &[u8]) -> GraphResourceDescriptor {
    decode_graph_resource(encoded).expect("graph capability view was independently validated")
}

const fn chip(value: Chip) -> u8 {
    match value {
        Chip::Esp32 => 1,
        Chip::Esp32S3 => 2,
    }
}

const fn chip_from_wire(value: u8) -> Option<Chip> {
    match value {
        1 => Some(Chip::Esp32),
        2 => Some(Chip::Esp32S3),
        _ => None,
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

const fn qualification_from_wire(value: u8) -> Option<Qualification> {
    match value {
        1 => Some(Qualification::Described),
        2 => Some(Qualification::Compiles),
        3 => Some(Qualification::Bench),
        4 => Some(Qualification::MotionQualified),
        5 => Some(Qualification::ProductionQualified),
        _ => None,
    }
}

const fn bool_from_wire(value: u8) -> Option<bool> {
    match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

const fn owner(value: OwnerDomain) -> u8 {
    match value {
        OwnerDomain::Service => 1,
        OwnerDomain::Realtime => 2,
    }
}

const fn owner_from_wire(value: u8) -> Option<OwnerDomain> {
    match value {
        1 => Some(OwnerDomain::Service),
        2 => Some(OwnerDomain::Realtime),
        _ => None,
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

const fn safe_value_from_wire(value: u8) -> Option<SafeValue> {
    match value {
        1 => Some(SafeValue::NotApplicable),
        2 => Some(SafeValue::HighImpedance),
        3 => Some(SafeValue::Low),
        4 => Some(SafeValue::High),
        5 => Some(SafeValue::EngineImage),
        _ => None,
    }
}

const fn bus_kind(value: BusKind) -> u8 {
    match value {
        BusKind::I2c => 1,
        BusKind::Spi => 2,
        BusKind::Uart => 3,
    }
}

const fn bus_kind_from_wire(value: u8) -> Option<BusKind> {
    match value {
        1 => Some(BusKind::I2c),
        2 => Some(BusKind::Spi),
        3 => Some(BusKind::Uart),
        _ => None,
    }
}

const fn bus_kind_matches_resource(kind: BusKind, resource: ResourceId) -> bool {
    matches!(
        (kind, resource),
        (BusKind::I2c, ResourceId::I2c(_))
            | (BusKind::Spi, ResourceId::Spi(_))
            | (BusKind::Uart, ResourceId::Uart(_))
    )
}

const fn support(value: SupportLevel) -> u8 {
    match value {
        SupportLevel::Described => 1,
        SupportLevel::Compiles => 2,
        SupportLevel::Bench => 3,
        SupportLevel::Qualified => 4,
    }
}

const fn support_from_wire(value: u8) -> Option<SupportLevel> {
    match value {
        1 => Some(SupportLevel::Described),
        2 => Some(SupportLevel::Compiles),
        3 => Some(SupportLevel::Bench),
        4 => Some(SupportLevel::Qualified),
        _ => None,
    }
}

const fn graph_resource_access(value: GraphResourceAccess) -> u8 {
    match value {
        GraphResourceAccess::StableBooleanInput => 1,
    }
}

const fn graph_resource_access_from_wire(value: u8) -> Option<GraphResourceAccess> {
    match value {
        0 => None,
        1 => Some(GraphResourceAccess::StableBooleanInput),
        _ => None,
    }
}

const fn diagnostic_observation_kind(value: DiagnosticObservationKind) -> u8 {
    match value {
        DiagnosticObservationKind::StableBooleanInput => 1,
    }
}

const fn diagnostic_observation_kind_from_wire(value: u8) -> Option<DiagnosticObservationKind> {
    match value {
        1 => Some(DiagnosticObservationKind::StableBooleanInput),
        _ => None,
    }
}

const fn digital_capture_source(value: DigitalCaptureSourceKind) -> u8 {
    match value {
        DigitalCaptureSourceKind::Simulated => 1,
        DigitalCaptureSourceKind::Rmt => 2,
        DigitalCaptureSourceKind::Pcnt => 3,
        DigitalCaptureSourceKind::Dma => 4,
        DigitalCaptureSourceKind::Software => 5,
    }
}

const fn digital_capture_source_from_wire(value: u8) -> Option<DigitalCaptureSourceKind> {
    match value {
        1 => Some(DigitalCaptureSourceKind::Simulated),
        2 => Some(DigitalCaptureSourceKind::Rmt),
        3 => Some(DigitalCaptureSourceKind::Pcnt),
        4 => Some(DigitalCaptureSourceKind::Dma),
        5 => Some(DigitalCaptureSourceKind::Software),
        _ => None,
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

const fn flash_kind_from_wire(value: u8) -> Option<FlashRegionKind> {
    match value {
        1 => Some(FlashRegionKind::Bootloader),
        2 => Some(FlashRegionKind::PartitionTable),
        3 => Some(FlashRegionKind::Application),
        4 => Some(FlashRegionKind::Configuration),
        5 => Some(FlashRegionKind::WebBundle),
        6 => Some(FlashRegionKind::UpdateSlot),
        7 => Some(FlashRegionKind::CrashLog),
        _ => None,
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

const fn clock_source_from_wire(value: u8) -> Option<ClockSource> {
    match value {
        1 => Some(ClockSource::Crystal),
        2 => Some(ClockSource::Pll),
        3 => Some(ClockSource::PeripheralBus),
        4 => Some(ClockSource::Rtc),
        5 => Some(ClockSource::External),
        _ => None,
    }
}

const fn clock_domain(value: ClockDomain) -> u8 {
    match value {
        ClockDomain::Chip => 1,
        ClockDomain::Service => 2,
        ClockDomain::Realtime => 3,
    }
}

const fn clock_domain_from_wire(value: u8) -> Option<ClockDomain> {
    match value {
        1 => Some(ClockDomain::Chip),
        2 => Some(ClockDomain::Service),
        3 => Some(ClockDomain::Realtime),
        _ => None,
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

const fn electrical_kind_from_wire(value: u8) -> Option<ElectricalConstraintKind> {
    match value {
        1 => Some(ElectricalConstraintKind::InputOnly),
        2 => Some(ElectricalConstraintKind::OutputOnly),
        3 => Some(ElectricalConstraintKind::BootStrap),
        4 => Some(ElectricalConstraintKind::SharedRoute),
        5 => Some(ElectricalConstraintKind::ActiveHigh),
        6 => Some(ElectricalConstraintKind::ActiveLow),
        7 => Some(ElectricalConstraintKind::NotPwm),
        8 => Some(ElectricalConstraintKind::Logic3v3),
        9 => Some(ElectricalConstraintKind::ResetStateUnverified),
        _ => None,
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

const fn interrupt_trigger_from_wire(value: u8) -> Option<InterruptTrigger> {
    match value {
        1 => Some(InterruptTrigger::Rising),
        2 => Some(InterruptTrigger::Falling),
        3 => Some(InterruptTrigger::AnyEdge),
        4 => Some(InterruptTrigger::LowLevel),
        5 => Some(InterruptTrigger::HighLevel),
        6 => Some(InterruptTrigger::Configurable),
        _ => None,
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

const fn hil_kind_from_wire(value: u8) -> Option<HilKind> {
    match value {
        1 => Some(HilKind::BoardIdentity),
        2 => Some(HilKind::SafeState),
        3 => Some(HilKind::PeripheralSmoke),
        4 => Some(HilKind::CoreIsolation),
        5 => Some(HilKind::Timing),
        6 => Some(HilKind::FaultInjection),
        7 => Some(HilKind::VisualReconciliation),
        _ => None,
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
    extern crate alloc;

    use super::*;
    use alloc::vec;

    fn complete_document(package: &BoardPackage<'_>) -> alloc::vec::Vec<u8> {
        let identity = calculate_identity(package).unwrap();
        let mut document = vec![0_u8; usize::try_from(identity.byte_len).unwrap()];
        let mut sink = RangeSink::new(0, &mut document);
        sink.write(&document_header(identity.byte_len)).unwrap();
        encode_payload(package, &mut sink).unwrap();
        assert_eq!(sink.finish(), Ok(document.len()));
        document
    }

    fn graph_section_offset(package: &BoardPackage<'_>) -> usize {
        CAPABILITY_DOCUMENT_HEADER_BYTES
            + 4
            + package.board.id.len()
            + 4
            + package.board.revision.len()
            + 33
    }

    fn diagnostic_section_offset(package: &BoardPackage<'_>) -> usize {
        graph_section_offset(package)
            + GRAPH_EXECUTOR_HEADER_BYTES
            + package.graph.opcodes.len() * GRAPH_OPCODE_CAPABILITY_BYTES
            + package.graph.resources.len() * GRAPH_RESOURCE_CAPABILITY_BYTES
    }

    fn resource_section_offset(package: &BoardPackage<'_>) -> usize {
        capture_section_offset(package)
            + DIGITAL_CAPTURE_HEADER_BYTES
            + package.digital_capture.resources.len() * DIGITAL_CAPTURE_RESOURCE_CAPABILITY_BYTES
    }

    fn capture_section_offset(package: &BoardPackage<'_>) -> usize {
        diagnostic_section_offset(package)
            + DIAGNOSTIC_OVERVIEW_HEADER_BYTES
            + package.diagnostic_overview.resources.len() * DIAGNOSTIC_RESOURCE_CAPABILITY_BYTES
    }

    #[test]
    fn request_and_response_prefixes_are_exact_and_canonical() {
        for resource in [
            ResourceId::Gpio(45),
            ResourceId::I2sOut { engine: 2, bit: 31 },
            ResourceId::Device(0x1234),
        ] {
            assert_eq!(
                decode_resource_id(&encode_resource_id(resource)),
                Ok(resource)
            );
        }
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
        for package in [
            &board_mks_tinybee::PACKAGE,
            &board_mks_tinybee::PACKAGE_4_MIB,
            &board_t_deck_pro::PACKAGE,
            &board_mks_esp32_foc_v1::PACKAGE,
        ] {
            let identity = calculate_identity(package).unwrap();
            assert!(!identity.digest.is_zero());
            let document = complete_document(package);
            assert_eq!(&document[..8], b"ALMCAP04");
            assert_eq!(read_u32(&document, 12), identity.byte_len);
            let mut hasher = Sha256::new();
            hasher.update(&document);
            let mut digest = [0_u8; 32];
            digest.copy_from_slice(&hasher.finalize());
            assert_eq!(Digest(digest), identity.digest);
            assert_eq!(calculate_identity(package).unwrap(), identity);
            let graph = decode_graph_execution(&document).unwrap();
            assert_eq!(graph.identity(), identity);
            assert_eq!(graph.ir_version(), package.graph.ir_version);
            assert_eq!(graph.package_bytes(), package.graph.package_bytes);
            assert_eq!(graph.maximum_nodes(), package.graph.maximum_nodes);
            assert_eq!(graph.maximum_channels(), package.graph.maximum_channels);
            assert_eq!(
                graph.maximum_queue_items(),
                package.graph.maximum_queue_items
            );
            assert_eq!(
                graph.service_state_bytes(),
                package.graph.service_state_bytes
            );
            assert_eq!(
                graph.realtime_state_bytes(),
                package.graph.realtime_state_bytes
            );
            assert_eq!(
                graph.service_channel_bytes(),
                package.graph.service_channel_bytes
            );
            assert_eq!(
                graph.realtime_channel_bytes(),
                package.graph.realtime_channel_bytes
            );
            assert_eq!(
                graph.bridge_channel_bytes(),
                package.graph.bridge_channel_bytes
            );
            assert_eq!(graph.support(), package.graph.support);
            assert!(graph.opcodes().eq(package.graph.opcodes.iter().copied()));
            assert!(
                graph
                    .resources()
                    .eq(package.graph.resources.iter().copied())
            );
            let diagnostic_overview = decode_diagnostic_overview(&document).unwrap();
            assert_eq!(diagnostic_overview.identity(), identity);
            assert_eq!(
                diagnostic_overview.schema_version(),
                package.diagnostic_overview.schema_version
            );
            assert_eq!(
                diagnostic_overview.support(),
                package.diagnostic_overview.support
            );
            assert_eq!(
                diagnostic_overview.maximum_resources(),
                package.diagnostic_overview.maximum_resources
            );
            assert_eq!(
                diagnostic_overview.telemetry_bytes(),
                (
                    package.diagnostic_overview.telemetry_request_bytes,
                    package.diagnostic_overview.telemetry_event_bytes,
                )
            );
            assert_eq!(
                diagnostic_overview.timing_micros(),
                (
                    package.diagnostic_overview.nominal_period_micros,
                    package.diagnostic_overview.maximum_age_micros,
                )
            );
            assert!(
                diagnostic_overview.resources().eq(package
                    .diagnostic_overview
                    .resources
                    .iter()
                    .copied())
            );
            let digital_capture = decode_digital_capture_capability(&document).unwrap();
            assert_eq!(digital_capture.identity(), identity);
            assert_eq!(
                digital_capture.schema_version(),
                package.digital_capture.schema_version
            );
            assert_eq!(digital_capture.support(), package.digital_capture.support);
            assert_eq!(
                digital_capture.configure_policy(),
                (
                    package.digital_capture.configure_flags,
                    package.digital_capture.trigger_kinds,
                )
            );
            assert_eq!(
                digital_capture.shape_limits(),
                (
                    package.digital_capture.maximum_channels,
                    package.digital_capture.maximum_transitions,
                )
            );
            assert_eq!(
                digital_capture.byte_limits(),
                (
                    package.digital_capture.configure_bytes,
                    package.digital_capture.record_bytes,
                    package.digital_capture.maximum_chunk_bytes,
                )
            );
            assert_eq!(
                digital_capture.timing_micros(),
                (
                    package.digital_capture.maximum_pretrigger_micros,
                    package.digital_capture.maximum_duration_micros,
                    package.digital_capture.arm_horizon_micros,
                )
            );
            assert!(
                digital_capture
                    .resources()
                    .eq(package.digital_capture.resources.iter().copied())
            );
            let board =
                decode_board_capability(&document, BoardCapabilityLimits::interactive()).unwrap();
            assert_eq!(board.identity(), identity);
            assert_eq!(board.board_id(), package.board.id);
            assert_eq!(board.revision(), package.board.revision);
            assert_eq!(board.chip(), package.board.chip);
            assert_eq!(board.application_cores(), package.board.application_cores);
            assert_eq!(board.qualification(), package.board.qualification);
            assert_eq!(board.armable(), package.armable);
            assert_eq!(board.flash_bytes(), package.memory.flash_bytes as u64);
            assert_eq!(
                board.internal_sram_bytes(),
                package.memory.internal_sram_bytes as u64
            );
            assert_eq!(board.psram_bytes(), package.memory.psram_bytes as u64);
            assert_eq!(
                board.realtime_psram_allowed(),
                package.memory.realtime_psram_allowed
            );
            assert_eq!(board.service_core(), package.cores.service_core);
            assert_eq!(board.realtime_core(), package.cores.realtime_core);
            assert_eq!(board.diagnostic_overview(), diagnostic_overview);
            assert_eq!(board.digital_capture(), digital_capture);
            assert!(
                board
                    .resources()
                    .eq(package.board.resources.iter().copied())
            );
            assert!(board.aliases().eq(package.aliases.iter().copied()));
            assert_eq!(board.bus_count(), package.buses.len());
            assert_eq!(board.device_count(), package.devices.len());
            assert_eq!(board.flash_region_count(), package.flash_regions.len());
            assert_eq!(board.clock_count(), package.clocks.len());
            assert_eq!(
                board.electrical_constraint_count(),
                package.electrical_constraints.len()
            );
            assert_eq!(board.interrupt_count(), package.interrupts.len());
            assert_eq!(
                board.safe_output_image_count(),
                package.safe_output_images.len()
            );
            assert_eq!(board.visual_count(), package.visuals.len());
            assert_eq!(
                board.hil_requirement_count(),
                package.hil_requirements.len()
            );
        }
    }

    #[test]
    fn licensed_visuals_and_normalized_hotspots_replay_without_allocation() {
        let polygon = [
            NormalizedPoint { x: 100, y: 200 },
            NormalizedPoint { x: 300, y: 200 },
            NormalizedPoint { x: 200, y: 450 },
        ];
        let hotspots = [alumina_board::HotspotDescriptor {
            id: "limit-x-negative-pad",
            resource: ResourceId::Gpio(33),
            polygon: &polygon,
        }];
        let visuals = [alumina_board::BoardVisualDescriptor {
            id: "top",
            asset_path: "boards/mks-tinybee/assets/tinybee-v1-top.png",
            media_type: "image/png",
            pixel_width: 1_600,
            pixel_height: 1_200,
            asset_digest: Digest([0x7a; 32]),
            license: "CC0-1.0",
            attribution: "operator-owned orthographic fixture photograph",
            hotspots: &hotspots,
        }];
        let mut package = board_mks_tinybee::PACKAGE;
        package.visuals = &visuals;
        let document = complete_document(&package);
        let board =
            decode_board_capability(&document, BoardCapabilityLimits::interactive()).unwrap();
        let visual = board.visuals().next().unwrap();
        assert_eq!(visual.id(), "top");
        assert_eq!(
            visual.asset_path(),
            "boards/mks-tinybee/assets/tinybee-v1-top.png"
        );
        assert_eq!(visual.media_type(), "image/png");
        assert_eq!(
            (visual.pixel_width(), visual.pixel_height()),
            (1_600, 1_200)
        );
        assert_eq!(visual.asset_digest(), Digest([0x7a; 32]));
        assert_eq!(visual.license(), "CC0-1.0");
        assert_eq!(visual.hotspot_count(), 1);
        let hotspot = visual.hotspots().next().unwrap();
        assert_eq!(hotspot.id(), "limit-x-negative-pad");
        assert_eq!(hotspot.resource(), ResourceId::Gpio(33));
        assert_eq!(hotspot.point_count(), 3);
        assert!(hotspot.points().eq(polygon));

        let mut tight = BoardCapabilityLimits::interactive();
        tight.maximum_points_per_hotspot = 2;
        assert_eq!(
            decode_board_capability(&document, tight),
            Err(CapabilityDocumentError::Limit)
        );
    }

    #[test]
    fn board_explorer_decoder_fails_closed_on_bounds_and_trailing_or_bad_records() {
        let package = &board_mks_tinybee::PACKAGE;
        let document = complete_document(package);
        let mut tight = BoardCapabilityLimits::interactive();
        tight.maximum_document_bytes = u32::try_from(document.len() - 1).unwrap();
        assert_eq!(
            decode_board_capability(&document, tight),
            Err(CapabilityDocumentError::Limit)
        );
        tight = BoardCapabilityLimits::interactive();
        tight.maximum_string_bytes = 1;
        assert_eq!(
            decode_board_capability(&document, tight),
            Err(CapabilityDocumentError::Limit)
        );
        tight = BoardCapabilityLimits::interactive();
        tight.maximum_records_per_section = 1;
        assert_eq!(
            decode_board_capability(&document, tight),
            Err(CapabilityDocumentError::Limit)
        );

        let first_resource = resource_section_offset(package) + 4;
        let mut bad_boolean = document.clone();
        bad_boolean[first_resource + 6] = 2;
        assert_eq!(
            decode_board_capability(&bad_boolean, BoardCapabilityLimits::interactive()),
            Err(CapabilityDocumentError::Section)
        );

        let mut trailing = document;
        trailing.push(0);
        let new_len = u32::try_from(trailing.len()).unwrap();
        trailing[12..16].copy_from_slice(&new_len.to_le_bytes());
        assert_eq!(
            decode_board_capability(&trailing, BoardCapabilityLimits::interactive()),
            Err(CapabilityDocumentError::Section)
        );
    }

    #[test]
    fn diagnostic_overview_tamper_fails_closed_and_is_not_graph_authority() {
        let package = &board_mks_tinybee::PACKAGE;
        let document = complete_document(package);
        let offset = diagnostic_section_offset(package);
        let overview = decode_diagnostic_overview(&document).unwrap();
        assert_eq!(overview.schema_version(), 1);
        assert_eq!(overview.support(), Some(SupportLevel::Compiles));
        assert_eq!(overview.maximum_resources(), 4);
        assert_eq!(overview.telemetry_bytes(), (176, 432));
        assert_eq!(overview.timing_micros(), (100_000, 500_000));
        assert!(overview.resources().eq([
            DiagnosticResourceDescriptor {
                resource: ResourceId::Gpio(22),
                observation: DiagnosticObservationKind::StableBooleanInput,
                support: SupportLevel::Compiles,
            },
            DiagnosticResourceDescriptor {
                resource: ResourceId::Gpio(32),
                observation: DiagnosticObservationKind::StableBooleanInput,
                support: SupportLevel::Compiles,
            },
            DiagnosticResourceDescriptor {
                resource: ResourceId::Gpio(33),
                observation: DiagnosticObservationKind::StableBooleanInput,
                support: SupportLevel::Compiles,
            },
            DiagnosticResourceDescriptor {
                resource: ResourceId::Gpio(35),
                observation: DiagnosticObservationKind::StableBooleanInput,
                support: SupportLevel::Compiles,
            },
        ]));
        assert!(overview.admits(
            ResourceId::Gpio(22),
            DiagnosticObservationKind::StableBooleanInput
        ));
        assert!(!overview.admits(
            ResourceId::Gpio(25),
            DiagnosticObservationKind::StableBooleanInput
        ));

        let mut described_only = document.clone();
        described_only[offset + 10] = support(SupportLevel::Described);
        let described_only = decode_diagnostic_overview(&described_only).unwrap();
        assert_eq!(described_only.support(), Some(SupportLevel::Described));
        assert!(!described_only.is_implemented());
        assert!(!described_only.admits(
            ResourceId::Gpio(22),
            DiagnosticObservationKind::StableBooleanInput
        ));

        let unsupported = complete_document(&board_t_deck_pro::PACKAGE);
        let unsupported = decode_diagnostic_overview(&unsupported).unwrap();
        assert_eq!(unsupported.support(), None);
        assert_eq!(unsupported.resource_count(), 0);

        let mut reserved = document.clone();
        reserved[offset + 32] = 1;
        assert_eq!(
            decode_diagnostic_overview(&reserved),
            Err(CapabilityDocumentError::Reserved)
        );

        let mut absent_with_facts = document.clone();
        absent_with_facts[offset + 10] = 0;
        assert_eq!(
            decode_diagnostic_overview(&absent_with_facts),
            Err(CapabilityDocumentError::Diagnostic)
        );

        let record_offset = offset + DIAGNOSTIC_OVERVIEW_HEADER_BYTES;
        let mut unknown_observation = document.clone();
        unknown_observation[record_offset + 4] = 0xff;
        assert_eq!(
            decode_diagnostic_overview(&unknown_observation),
            Err(CapabilityDocumentError::Diagnostic)
        );

        let mut duplicate = document;
        let first =
            duplicate[record_offset..record_offset + DIAGNOSTIC_RESOURCE_CAPABILITY_BYTES].to_vec();
        duplicate[record_offset + DIAGNOSTIC_RESOURCE_CAPABILITY_BYTES
            ..record_offset + 2 * DIAGNOSTIC_RESOURCE_CAPABILITY_BYTES]
            .copy_from_slice(&first);
        assert_eq!(
            decode_diagnostic_overview(&duplicate),
            Err(CapabilityDocumentError::Diagnostic)
        );
    }

    #[test]
    fn digital_capture_tamper_fails_closed_and_is_not_graph_authority() {
        let channels = [22_u8, 32, 33, 35].map(|gpio| DigitalCaptureResourceDescriptor {
            resource: ResourceId::Gpio(gpio),
            source: DigitalCaptureSourceKind::Simulated,
            support: SupportLevel::Compiles,
        });
        let mut package = board_mks_tinybee::PACKAGE;
        package.digital_capture = DigitalCaptureDescriptor {
            schema_version: 1,
            support: Some(SupportLevel::Compiles),
            configure_flags: DigitalCaptureConfigureFlags(
                DigitalCaptureConfigureFlags::EDGE_TIMESTAMPS,
            ),
            trigger_kinds: DigitalCaptureTriggerSet(DigitalCaptureTriggerSet::IMMEDIATE),
            maximum_channels: 4,
            maximum_transitions: 64,
            configure_bytes: 208,
            record_bytes: 2_048,
            maximum_chunk_bytes: 168,
            maximum_pretrigger_micros: 0,
            maximum_duration_micros: 2_000_000,
            arm_horizon_micros: 30_000_000,
            resources: &channels,
        };
        let document = complete_document(&package);
        let offset = capture_section_offset(&package);
        let capture = decode_digital_capture_capability(&document).unwrap();
        assert_eq!(capture.schema_version(), 1);
        assert_eq!(capture.support(), Some(SupportLevel::Compiles));
        assert_eq!(
            capture.configure_policy(),
            (
                DigitalCaptureConfigureFlags(DigitalCaptureConfigureFlags::EDGE_TIMESTAMPS),
                DigitalCaptureTriggerSet(DigitalCaptureTriggerSet::IMMEDIATE),
            )
        );
        assert_eq!(capture.shape_limits(), (4, 64));
        assert_eq!(capture.byte_limits(), (208, 2_048, 168));
        assert_eq!(capture.timing_micros(), (0, 2_000_000, 30_000_000));
        assert!(capture.resources().eq(channels));
        assert!(capture.admits(ResourceId::Gpio(22), DigitalCaptureSourceKind::Simulated));
        assert!(!capture.admits(ResourceId::Gpio(25), DigitalCaptureSourceKind::Simulated));

        let absent = complete_document(&board_t_deck_pro::PACKAGE);
        let absent = decode_digital_capture_capability(&absent).unwrap();
        assert_eq!(absent.support(), None);
        assert_eq!(absent.resource_count(), 0);

        let mut reserved = document.clone();
        reserved[offset + 48] = 1;
        assert_eq!(
            decode_digital_capture_capability(&reserved),
            Err(CapabilityDocumentError::Reserved)
        );

        let mut absent_with_facts = document.clone();
        absent_with_facts[offset + 10] = 0;
        assert_eq!(
            decode_digital_capture_capability(&absent_with_facts),
            Err(CapabilityDocumentError::Capture)
        );

        let record_offset = offset + DIGITAL_CAPTURE_HEADER_BYTES;
        let mut unknown_source = document.clone();
        unknown_source[record_offset + 4] = 0xff;
        assert_eq!(
            decode_digital_capture_capability(&unknown_source),
            Err(CapabilityDocumentError::Capture)
        );

        let mut unsafe_resource = document.clone();
        unsafe_resource[record_offset..record_offset + 4]
            .copy_from_slice(&encode_resource_id(ResourceId::Gpio(25)));
        assert!(decode_digital_capture_capability(&unsafe_resource).is_ok());
        assert_eq!(
            decode_board_capability(&unsafe_resource, BoardCapabilityLimits::interactive()),
            Err(CapabilityDocumentError::Capture)
        );

        let mut duplicate = document;
        let first = duplicate
            [record_offset..record_offset + DIGITAL_CAPTURE_RESOURCE_CAPABILITY_BYTES]
            .to_vec();
        duplicate[record_offset + DIGITAL_CAPTURE_RESOURCE_CAPABILITY_BYTES
            ..record_offset + 2 * DIGITAL_CAPTURE_RESOURCE_CAPABILITY_BYTES]
            .copy_from_slice(&first);
        assert_eq!(
            decode_digital_capture_capability(&duplicate),
            Err(CapabilityDocumentError::Capture)
        );
    }

    #[test]
    fn graph_capability_tamper_fails_closed_and_palettes_are_exact() {
        let package = &board_mks_tinybee::PACKAGE;
        let document = complete_document(package);
        let graph_offset = graph_section_offset(package);
        let graph = decode_graph_execution(&document).unwrap();
        assert_eq!(graph.opcode_count(), 4);
        assert_eq!(graph.resource_count(), 4);
        assert!(graph.resources().eq([
            GraphResourceDescriptor {
                resource: ResourceId::Gpio(33),
                class: GraphResourceClass::new(1),
                access: GraphResourceAccess::StableBooleanInput,
                support: SupportLevel::Compiles,
            },
            GraphResourceDescriptor {
                resource: ResourceId::Gpio(32),
                class: GraphResourceClass::new(1),
                access: GraphResourceAccess::StableBooleanInput,
                support: SupportLevel::Compiles,
            },
            GraphResourceDescriptor {
                resource: ResourceId::Gpio(22),
                class: GraphResourceClass::new(1),
                access: GraphResourceAccess::StableBooleanInput,
                support: SupportLevel::Compiles,
            },
            GraphResourceDescriptor {
                resource: ResourceId::Gpio(35),
                class: GraphResourceClass::new(1),
                access: GraphResourceAccess::StableBooleanInput,
                support: SupportLevel::Compiles,
            },
        ]));
        let t_deck = complete_document(&board_t_deck_pro::PACKAGE);
        assert_eq!(decode_graph_execution(&t_deck).unwrap().resource_count(), 0);

        let mut reserved = document.clone();
        reserved[graph_offset + 48] = 1;
        assert_eq!(
            decode_graph_execution(&reserved),
            Err(CapabilityDocumentError::Reserved)
        );

        let opcode_offset = graph_offset + GRAPH_EXECUTOR_HEADER_BYTES;
        let mut unknown_access = document.clone();
        unknown_access[opcode_offset + 3 * GRAPH_OPCODE_CAPABILITY_BYTES + 3] = 0xff;
        assert_eq!(
            decode_graph_execution(&unknown_access),
            Err(CapabilityDocumentError::Graph)
        );

        let resource_offset =
            opcode_offset + package.graph.opcodes.len() * GRAPH_OPCODE_CAPABILITY_BYTES;
        let mut malformed_resource = document.clone();
        malformed_resource[resource_offset] = 0xff;
        assert_eq!(
            decode_graph_execution(&malformed_resource),
            Err(CapabilityDocumentError::Resource(ResourceWireError::Kind(
                0xff
            )))
        );

        let mut duplicate_resource = document;
        let first = duplicate_resource
            [resource_offset..resource_offset + GRAPH_RESOURCE_CAPABILITY_BYTES]
            .to_vec();
        duplicate_resource[resource_offset + GRAPH_RESOURCE_CAPABILITY_BYTES
            ..resource_offset + 2 * GRAPH_RESOURCE_CAPABILITY_BYTES]
            .copy_from_slice(&first);
        assert_eq!(
            decode_graph_execution(&duplicate_resource),
            Err(CapabilityDocumentError::Graph)
        );
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
        assert_eq!(&chunk[..8], b"ALMCAP04");
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
