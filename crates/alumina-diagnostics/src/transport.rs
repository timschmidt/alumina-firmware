//! Canonical, bounded transport bodies for diagnostic subscriptions and captures.
//!
//! These records sit inside authenticated Alumina protocol frames. They bind
//! every mutable session to a complete device/boot/capability/configuration
//! context and use exact digests for idempotent retry reconciliation. Decoders
//! borrow caller-owned bytes and allocate no memory.

use core::fmt;

use alumina_board::ResourceId;
use alumina_capability::{decode_resource_id, encode_resource_id};
use alumina_protocol::{DeviceCycle, Digest};
use sha2::{Digest as _, Sha256};

use super::{
    CaptureId, CaptureQualityFlags, DiagnosticContext, DiagnosticError, DiagnosticLimits,
    DigitalTriggerCondition, ResourceOverviewView, decode_context, decode_digital_capture,
    decode_resource_overview, encode_context, read_u16, read_u32, read_u64,
    resource_overview_encoded_len, validate_context,
};

/// Exact telemetry-subscription request magic.
pub const TELEMETRY_SUBSCRIBE_MAGIC: [u8; 8] = *b"ALMTLS01";
/// Exact telemetry session-reference magic.
pub const TELEMETRY_SESSION_MAGIC: [u8; 8] = *b"ALMTLR01";
/// Exact telemetry status magic.
pub const TELEMETRY_STATUS_MAGIC: [u8; 8] = *b"ALMTST01";
/// Exact telemetry event-envelope magic.
pub const TELEMETRY_EVENT_MAGIC: [u8; 8] = *b"ALMTEV01";
/// Exact waveform configure-request magic.
pub const WAVEFORM_CONFIGURE_MAGIC: [u8; 8] = *b"ALMWCF01";
/// Exact waveform session-reference magic.
pub const WAVEFORM_SESSION_MAGIC: [u8; 8] = *b"ALMWRF01";
/// Exact waveform status magic.
pub const WAVEFORM_STATUS_MAGIC: [u8; 8] = *b"ALMWST01";
/// Exact waveform range-read request magic.
pub const WAVEFORM_READ_MAGIC: [u8; 8] = *b"ALMWRD01";
/// Exact waveform chunk-envelope magic.
pub const WAVEFORM_CHUNK_MAGIC: [u8; 8] = *b"ALMWCH01";

/// Exact transport-body schema version.
pub const DIAGNOSTIC_TRANSPORT_VERSION: u16 = 1;
/// Bytes before telemetry resource selectors.
pub const TELEMETRY_SUBSCRIBE_HEADER_BYTES: usize = 160;
/// Exact telemetry session-reference length.
pub const TELEMETRY_SESSION_WIRE_BYTES: usize = 56;
/// Exact telemetry status length.
pub const TELEMETRY_STATUS_WIRE_BYTES: usize = 120;
/// Bytes before an embedded complete resource overview.
pub const TELEMETRY_EVENT_HEADER_BYTES: usize = 112;
/// Bytes before waveform channel selectors.
pub const WAVEFORM_CONFIGURE_HEADER_BYTES: usize = 192;
/// Exact waveform session-reference length.
pub const WAVEFORM_SESSION_WIRE_BYTES: usize = 64;
/// Exact waveform status length.
pub const WAVEFORM_STATUS_WIRE_BYTES: usize = 144;
/// Exact waveform range-read request length.
pub const WAVEFORM_READ_WIRE_BYTES: usize = 112;
/// Bytes before waveform chunk payload bytes.
pub const WAVEFORM_CHUNK_HEADER_BYTES: usize = 144;
/// Hard format maximum for telemetry resource selectors.
pub const MAX_TELEMETRY_RESOURCES: usize = 256;

const TELEMETRY_SUBSCRIBE_FLAG_LATEST_ONLY: u16 = 1 << 0;
const TELEMETRY_STATUS_FLAG_PENDING: u8 = 1 << 0;
const WAVEFORM_CONFIGURE_FLAG_EDGE_TIMESTAMPS: u16 = 1 << 0;
const WAVEFORM_CONFIGURE_FLAG_ALLOW_SOFTWARE: u16 = 1 << 1;
const WAVEFORM_STATUS_FLAG_TRIGGERED: u8 = 1 << 0;
const WAVEFORM_STATUS_FLAG_RETAINED_RECORD: u8 = 1 << 1;
const WAVEFORM_CHUNK_FLAG_FINAL: u16 = 1 << 0;
const NO_TRIGGER_CHANNEL: u16 = u16::MAX;

/// Caller-selected transport admission ceilings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticTransportLimits {
    /// Largest complete telemetry event envelope.
    pub maximum_telemetry_event_bytes: u32,
    /// Largest admitted telemetry selector table.
    pub maximum_telemetry_resources: u16,
    /// Largest admitted waveform channel table.
    pub maximum_waveform_channels: u16,
    /// Largest requested hardware transition capacity.
    pub maximum_waveform_transitions: u32,
    /// Largest retained complete canonical capture.
    pub maximum_waveform_record_bytes: u32,
    /// Largest range payload in one waveform chunk envelope.
    pub maximum_waveform_chunk_bytes: u32,
}

impl DiagnosticTransportLimits {
    /// Conservative native-control policy; a 144-byte chunk header plus 168
    /// payload bytes exactly fills the current 312-byte service body budget.
    pub const fn native_control() -> Self {
        Self {
            maximum_telemetry_event_bytes: 64 * 1_024,
            // 160-byte header + 229 selectors = 1076-byte native body.
            maximum_telemetry_resources: 229,
            // 192-byte header + 221 selectors = 1076-byte native body.
            maximum_waveform_channels: 221,
            maximum_waveform_transitions: 65_536,
            maximum_waveform_record_bytes: 4 * 1_024 * 1_024,
            maximum_waveform_chunk_bytes: 168,
        }
    }
}

impl Default for DiagnosticTransportLimits {
    fn default() -> Self {
        Self::native_control()
    }
}

/// Canonical diagnostic transport rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticTransportError {
    /// Complete byte length or caller buffer length is invalid.
    Length,
    /// Magic does not identify the requested transport body.
    Magic,
    /// Schema version is unsupported.
    Version,
    /// Unknown or missing flags were observed.
    Flags,
    /// Reserved bytes were nonzero.
    Reserved,
    /// A nonzero session identity or digest was absent.
    Identity,
    /// Device/boot/capability/configuration/clock context did not match.
    Context,
    /// Resource selectors were malformed, duplicated, or out of order.
    Resource,
    /// A caller-selected count or byte ceiling was exceeded.
    Limit(&'static str),
    /// Requested timing or acquisition window is invalid.
    Window,
    /// Lifecycle phase or status fields contradict each other.
    State,
    /// A canonical request, record, or chunk digest did not match.
    Integrity,
    /// Requested byte range is empty, overflowing, or outside the record.
    Range,
    /// Embedded canonical diagnostic evidence was rejected.
    Record(DiagnosticError),
}

impl fmt::Display for DiagnosticTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "diagnostic transport rejected: {self:?}")
    }
}

/// Stable nonzero identity of one telemetry subscription.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct SubscriptionId(u64);

impl SubscriptionId {
    /// Constructs a subscription identity, rejecting zero.
    pub const fn new(value: u64) -> Result<Self, DiagnosticTransportError> {
        if value == 0 {
            Err(DiagnosticTransportError::Identity)
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the exact wire value.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Telemetry subscription behavior flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct TelemetrySubscribeFlags(pub u16);

impl TelemetrySubscribeFlags {
    /// Retain only the newest unpublished snapshot and count replacements.
    pub const LATEST_ONLY: u16 = TELEMETRY_SUBSCRIBE_FLAG_LATEST_ONLY;

    /// Whether one behavior bit is present.
    pub const fn contains(self, flag: u16) -> bool {
        self.0 & flag != 0
    }
}

/// Borrowed canonical telemetry subscription supplied to the encoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetrySubscribeRequest<'a> {
    /// Nonzero boot-local subscription identity.
    pub subscription_id: SubscriptionId,
    /// Complete evidence context expected on every event.
    pub context: DiagnosticContext,
    /// Subscription behavior; V1 requires latest-only replacement.
    pub flags: TelemetrySubscribeFlags,
    /// Minimum cycles between produced snapshots.
    pub minimum_period_cycles: u64,
    /// Maximum complete event-envelope bytes accepted by the caller.
    pub maximum_event_bytes: u32,
    /// Strictly increasing resources included in every overview.
    pub resources: &'a [ResourceId],
}

/// Allocation-free validated telemetry-subscription view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetrySubscribeView<'a> {
    subscription_id: SubscriptionId,
    context: DiagnosticContext,
    flags: TelemetrySubscribeFlags,
    minimum_period_cycles: u64,
    maximum_event_bytes: u32,
    resource_records: &'a [u8],
    resource_count: usize,
    digest: Digest,
}

impl<'a> TelemetrySubscribeView<'a> {
    /// Nonzero boot-local subscription identity.
    pub const fn subscription_id(self) -> SubscriptionId {
        self.subscription_id
    }

    /// Complete required diagnostic context.
    pub const fn context(self) -> DiagnosticContext {
        self.context
    }

    /// Subscription behavior.
    pub const fn flags(self) -> TelemetrySubscribeFlags {
        self.flags
    }

    /// Minimum period between produced snapshots.
    pub const fn minimum_period_cycles(self) -> u64 {
        self.minimum_period_cycles
    }

    /// Maximum complete event-envelope bytes accepted by the caller.
    pub const fn maximum_event_bytes(self) -> u32 {
        self.maximum_event_bytes
    }

    /// Number of exact selected resources.
    pub const fn resource_count(self) -> usize {
        self.resource_count
    }

    /// SHA-256 of the complete canonical subscription request.
    pub const fn digest(self) -> Digest {
        self.digest
    }

    /// Iterates validated resource selectors without allocation.
    pub fn resources(self) -> ResourceSelectorIter<'a> {
        ResourceSelectorIter {
            records: self.resource_records,
            offset: 0,
        }
    }
}

/// Iterator over validated four-byte resource selectors.
#[derive(Clone, Debug)]
pub struct ResourceSelectorIter<'a> {
    records: &'a [u8],
    offset: usize,
}

impl Iterator for ResourceSelectorIter<'_> {
    type Item = ResourceId;

    fn next(&mut self) -> Option<Self::Item> {
        let end = self.offset.checked_add(4)?;
        let record = self.records.get(self.offset..end)?;
        self.offset = end;
        Some(decode_resource_id(record).expect("validated resource selector remains valid"))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.records.len() - self.offset) / 4;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for ResourceSelectorIter<'_> {}

/// Returns the exact telemetry subscription length for a selector count.
pub fn telemetry_subscribe_encoded_len(
    resource_count: usize,
) -> Result<usize, DiagnosticTransportError> {
    TELEMETRY_SUBSCRIBE_HEADER_BYTES
        .checked_add(
            resource_count
                .checked_mul(4)
                .ok_or(DiagnosticTransportError::Length)?,
        )
        .ok_or(DiagnosticTransportError::Length)
}

/// Encodes one canonical telemetry subscription into caller-owned memory.
pub fn encode_telemetry_subscribe(
    request: &TelemetrySubscribeRequest<'_>,
    output: &mut [u8],
    limits: DiagnosticTransportLimits,
) -> Result<usize, DiagnosticTransportError> {
    validate_context(request.context).map_err(DiagnosticTransportError::Record)?;
    validate_telemetry_flags(request.flags)?;
    validate_resource_slice(request.resources, limits.maximum_telemetry_resources)?;
    if request.minimum_period_cycles == 0 {
        return Err(DiagnosticTransportError::Window);
    }
    let required_overview = resource_overview_encoded_len(request.resources.len())
        .map_err(DiagnosticTransportError::Record)?;
    let required_event = TELEMETRY_EVENT_HEADER_BYTES
        .checked_add(required_overview)
        .ok_or(DiagnosticTransportError::Length)?;
    validate_event_budget(request.maximum_event_bytes, required_event, limits)?;
    let total = telemetry_subscribe_encoded_len(request.resources.len())?;
    if output.len() < total {
        return Err(DiagnosticTransportError::Length);
    }
    let encoded = &mut output[..total];
    encoded.fill(0);
    encoded[0..8].copy_from_slice(&TELEMETRY_SUBSCRIBE_MAGIC);
    encoded[8..10].copy_from_slice(&DIAGNOSTIC_TRANSPORT_VERSION.to_le_bytes());
    encoded[10..12].copy_from_slice(&request.flags.0.to_le_bytes());
    encoded[12..16].copy_from_slice(&to_u32(total)?.to_le_bytes());
    encoded[16..24].copy_from_slice(&request.subscription_id.get().to_le_bytes());
    encoded[24..32].copy_from_slice(&request.minimum_period_cycles.to_le_bytes());
    encoded[32..36].copy_from_slice(&request.maximum_event_bytes.to_le_bytes());
    encoded[36..38].copy_from_slice(&to_u16(request.resources.len())?.to_le_bytes());
    encode_context(request.context, &mut encoded[40..152]);
    for (index, resource) in request.resources.iter().copied().enumerate() {
        let start = TELEMETRY_SUBSCRIBE_HEADER_BYTES + index * 4;
        encoded[start..start + 4].copy_from_slice(&encode_resource_id(resource));
    }
    Ok(total)
}

/// Decodes and independently validates a complete telemetry subscription.
pub fn decode_telemetry_subscribe(
    encoded: &[u8],
    limits: DiagnosticTransportLimits,
) -> Result<TelemetrySubscribeView<'_>, DiagnosticTransportError> {
    validate_prefix(
        encoded,
        TELEMETRY_SUBSCRIBE_MAGIC,
        TELEMETRY_SUBSCRIBE_HEADER_BYTES,
    )?;
    if encoded[38..40].iter().any(|byte| *byte != 0)
        || encoded[152..160].iter().any(|byte| *byte != 0)
    {
        return Err(DiagnosticTransportError::Reserved);
    }
    let flags = TelemetrySubscribeFlags(read_u16(encoded, 10));
    validate_telemetry_flags(flags)?;
    let resource_count = usize::from(read_u16(encoded, 36));
    if resource_count == 0
        || resource_count > MAX_TELEMETRY_RESOURCES
        || resource_count > usize::from(limits.maximum_telemetry_resources)
    {
        return Err(DiagnosticTransportError::Limit("telemetry resources"));
    }
    if encoded.len() != telemetry_subscribe_encoded_len(resource_count)? {
        return Err(DiagnosticTransportError::Length);
    }
    let resource_records = &encoded[TELEMETRY_SUBSCRIBE_HEADER_BYTES..];
    validate_resource_records(resource_records)?;
    let minimum_period_cycles = read_u64(encoded, 24);
    if minimum_period_cycles == 0 {
        return Err(DiagnosticTransportError::Window);
    }
    let required_overview =
        resource_overview_encoded_len(resource_count).map_err(DiagnosticTransportError::Record)?;
    let required_event = TELEMETRY_EVENT_HEADER_BYTES
        .checked_add(required_overview)
        .ok_or(DiagnosticTransportError::Length)?;
    let maximum_event_bytes = read_u32(encoded, 32);
    validate_event_budget(maximum_event_bytes, required_event, limits)?;
    Ok(TelemetrySubscribeView {
        subscription_id: SubscriptionId::new(read_u64(encoded, 16))?,
        context: decode_context(&encoded[40..152]).map_err(DiagnosticTransportError::Record)?,
        flags,
        minimum_period_cycles,
        maximum_event_bytes,
        resource_records,
        resource_count,
        digest: sha256(encoded),
    })
}

fn validate_telemetry_flags(
    flags: TelemetrySubscribeFlags,
) -> Result<(), DiagnosticTransportError> {
    if flags.0 != TELEMETRY_SUBSCRIBE_FLAG_LATEST_ONLY {
        Err(DiagnosticTransportError::Flags)
    } else {
        Ok(())
    }
}

fn validate_event_budget(
    maximum_event_bytes: u32,
    required: usize,
    limits: DiagnosticTransportLimits,
) -> Result<(), DiagnosticTransportError> {
    let required = to_u32(required)?;
    if maximum_event_bytes < required {
        return Err(DiagnosticTransportError::Limit("telemetry event budget"));
    }
    if maximum_event_bytes > limits.maximum_telemetry_event_bytes {
        return Err(DiagnosticTransportError::Limit("telemetry event bytes"));
    }
    Ok(())
}

fn validate_resource_slice(
    resources: &[ResourceId],
    caller_maximum: u16,
) -> Result<(), DiagnosticTransportError> {
    if resources.is_empty()
        || resources.len() > MAX_TELEMETRY_RESOURCES
        || resources.len() > usize::from(caller_maximum)
    {
        return Err(DiagnosticTransportError::Limit("resource selectors"));
    }
    let mut previous = None;
    for resource in resources {
        if previous.is_some_and(|value| value >= *resource) {
            return Err(DiagnosticTransportError::Resource);
        }
        previous = Some(*resource);
    }
    Ok(())
}

fn validate_resource_records(records: &[u8]) -> Result<(), DiagnosticTransportError> {
    if !records.len().is_multiple_of(4) {
        return Err(DiagnosticTransportError::Length);
    }
    let mut previous = None;
    for record in records.chunks_exact(4) {
        let resource =
            decode_resource_id(record).map_err(|_| DiagnosticTransportError::Resource)?;
        if previous.is_some_and(|value| value >= resource) {
            return Err(DiagnosticTransportError::Resource);
        }
        previous = Some(resource);
    }
    Ok(())
}

/// Exact reference to one canonical telemetry subscription request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetrySessionRequest {
    /// Subscription identity.
    pub subscription_id: SubscriptionId,
    /// SHA-256 of the complete canonical subscribe request.
    pub subscription_digest: Digest,
}

impl TelemetrySessionRequest {
    /// Encodes the exact fixed session reference.
    pub fn encode(self) -> Result<[u8; TELEMETRY_SESSION_WIRE_BYTES], DiagnosticTransportError> {
        validate_nonzero_digest(self.subscription_digest)?;
        let mut encoded = [0_u8; TELEMETRY_SESSION_WIRE_BYTES];
        encoded[0..8].copy_from_slice(&TELEMETRY_SESSION_MAGIC);
        encoded[8..10].copy_from_slice(&DIAGNOSTIC_TRANSPORT_VERSION.to_le_bytes());
        encoded[12..16].copy_from_slice(&(TELEMETRY_SESSION_WIRE_BYTES as u32).to_le_bytes());
        encoded[16..24].copy_from_slice(&self.subscription_id.get().to_le_bytes());
        encoded[24..56].copy_from_slice(&self.subscription_digest.0);
        Ok(encoded)
    }

    /// Decodes a fixed telemetry status/unsubscribe reference.
    pub fn decode(encoded: &[u8]) -> Result<Self, DiagnosticTransportError> {
        validate_prefix(
            encoded,
            TELEMETRY_SESSION_MAGIC,
            TELEMETRY_SESSION_WIRE_BYTES,
        )?;
        if encoded[10..12].iter().any(|byte| *byte != 0) {
            return Err(DiagnosticTransportError::Reserved);
        }
        let request = Self {
            subscription_id: SubscriptionId::new(read_u64(encoded, 16))?,
            subscription_digest: read_digest(encoded, 24),
        };
        validate_nonzero_digest(request.subscription_digest)?;
        if request.encode()? != encoded {
            return Err(DiagnosticTransportError::Reserved);
        }
        Ok(request)
    }
}

/// Lifecycle of one retained telemetry subscription.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum TelemetryPhase {
    /// Subscription is admitted and may publish snapshots.
    Active = 1,
    /// Subscription was explicitly removed; counters are retained for audit.
    Unsubscribed = 2,
}

impl TelemetryPhase {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Active),
            2 => Some(Self::Unsubscribed),
            _ => None,
        }
    }
}

/// Exact telemetry subscription status returned for reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetrySubscriptionStatus {
    /// Lifecycle phase.
    pub phase: TelemetryPhase,
    /// Whether an unpublished latest-only event is retained.
    pub pending: bool,
    /// Subscription identity.
    pub subscription_id: SubscriptionId,
    /// Exact canonical subscribe-request digest.
    pub subscription_digest: Digest,
    /// Admitted minimum period.
    pub minimum_period_cycles: u64,
    /// Admitted maximum event envelope length.
    pub maximum_event_bytes: u32,
    /// Admitted resource count.
    pub resource_count: u16,
    /// Sequence assigned to the next produced overview.
    pub next_event_sequence: u64,
    /// Number of events successfully taken by the transport owner.
    pub published_events: u64,
    /// Number of pending events replaced before publication.
    pub dropped_events: u64,
    /// Assembly cycle of the newest produced event, or zero before any event.
    pub last_event_cycle: DeviceCycle,
    /// Sequence of the pending event, or zero when none is retained.
    pub pending_event_sequence: u64,
    /// Complete pending event-envelope bytes, or zero when none is retained.
    pub pending_event_bytes: u32,
}

impl TelemetrySubscriptionStatus {
    /// Encodes the exact fixed status record.
    pub fn encode(self) -> Result<[u8; TELEMETRY_STATUS_WIRE_BYTES], DiagnosticTransportError> {
        self.validate()?;
        let mut encoded = [0_u8; TELEMETRY_STATUS_WIRE_BYTES];
        encoded[0..8].copy_from_slice(&TELEMETRY_STATUS_MAGIC);
        encoded[8..10].copy_from_slice(&DIAGNOSTIC_TRANSPORT_VERSION.to_le_bytes());
        encoded[10] = self.phase as u8;
        encoded[11] = if self.pending {
            TELEMETRY_STATUS_FLAG_PENDING
        } else {
            0
        };
        encoded[12..16].copy_from_slice(&(TELEMETRY_STATUS_WIRE_BYTES as u32).to_le_bytes());
        encoded[16..24].copy_from_slice(&self.subscription_id.get().to_le_bytes());
        encoded[24..56].copy_from_slice(&self.subscription_digest.0);
        encoded[56..64].copy_from_slice(&self.minimum_period_cycles.to_le_bytes());
        encoded[64..68].copy_from_slice(&self.maximum_event_bytes.to_le_bytes());
        encoded[68..70].copy_from_slice(&self.resource_count.to_le_bytes());
        encoded[72..80].copy_from_slice(&self.next_event_sequence.to_le_bytes());
        encoded[80..88].copy_from_slice(&self.published_events.to_le_bytes());
        encoded[88..96].copy_from_slice(&self.dropped_events.to_le_bytes());
        encoded[96..104].copy_from_slice(&self.last_event_cycle.0.to_le_bytes());
        encoded[104..112].copy_from_slice(&self.pending_event_sequence.to_le_bytes());
        encoded[112..116].copy_from_slice(&self.pending_event_bytes.to_le_bytes());
        Ok(encoded)
    }

    /// Decodes and independently validates an exact fixed status record.
    pub fn decode(encoded: &[u8]) -> Result<Self, DiagnosticTransportError> {
        validate_prefix(encoded, TELEMETRY_STATUS_MAGIC, TELEMETRY_STATUS_WIRE_BYTES)?;
        if encoded[70..72].iter().any(|byte| *byte != 0)
            || encoded[116..120].iter().any(|byte| *byte != 0)
        {
            return Err(DiagnosticTransportError::Reserved);
        }
        if encoded[11] & !TELEMETRY_STATUS_FLAG_PENDING != 0 {
            return Err(DiagnosticTransportError::Flags);
        }
        let status = Self {
            phase: TelemetryPhase::from_wire(encoded[10]).ok_or(DiagnosticTransportError::State)?,
            pending: encoded[11] & TELEMETRY_STATUS_FLAG_PENDING != 0,
            subscription_id: SubscriptionId::new(read_u64(encoded, 16))?,
            subscription_digest: read_digest(encoded, 24),
            minimum_period_cycles: read_u64(encoded, 56),
            maximum_event_bytes: read_u32(encoded, 64),
            resource_count: read_u16(encoded, 68),
            next_event_sequence: read_u64(encoded, 72),
            published_events: read_u64(encoded, 80),
            dropped_events: read_u64(encoded, 88),
            last_event_cycle: DeviceCycle(read_u64(encoded, 96)),
            pending_event_sequence: read_u64(encoded, 104),
            pending_event_bytes: read_u32(encoded, 112),
        };
        status.validate()?;
        if status.encode()? != encoded {
            return Err(DiagnosticTransportError::Reserved);
        }
        Ok(status)
    }

    fn validate(self) -> Result<(), DiagnosticTransportError> {
        validate_nonzero_digest(self.subscription_digest)?;
        if self.minimum_period_cycles == 0
            || self.maximum_event_bytes == 0
            || self.resource_count == 0
            || usize::from(self.resource_count) > MAX_TELEMETRY_RESOURCES
            || self.next_event_sequence == 0
        {
            return Err(DiagnosticTransportError::State);
        }
        if self.pending != (self.pending_event_sequence != 0 && self.pending_event_bytes != 0)
            || self.pending_event_sequence >= self.next_event_sequence
        {
            return Err(DiagnosticTransportError::State);
        }
        if self.phase == TelemetryPhase::Unsubscribed && self.pending {
            return Err(DiagnosticTransportError::State);
        }
        let accounted = self
            .published_events
            .checked_add(self.dropped_events)
            .and_then(|events| events.checked_add(u64::from(self.pending)))
            .ok_or(DiagnosticTransportError::State)?;
        if accounted != self.next_event_sequence - 1 {
            return Err(DiagnosticTransportError::State);
        }
        Ok(())
    }
}

/// Borrowed telemetry event supplied to the canonical encoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetryEvent<'a> {
    /// Subscription identity.
    pub subscription_id: SubscriptionId,
    /// Exact canonical subscribe-request digest.
    pub subscription_digest: Digest,
    /// Monotonic event sequence, also required on the embedded overview.
    pub event_sequence: u64,
    /// Cumulative pending-event replacements before this event.
    pub dropped_events: u64,
    /// Complete canonical resource-overview bytes.
    pub overview: &'a [u8],
}

/// Allocation-free validated telemetry event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TelemetryEventView<'a> {
    subscription_id: SubscriptionId,
    subscription_digest: Digest,
    event_sequence: u64,
    dropped_events: u64,
    overview_digest: Digest,
    overview: ResourceOverviewView<'a>,
}

impl<'a> TelemetryEventView<'a> {
    /// Subscription identity.
    pub const fn subscription_id(self) -> SubscriptionId {
        self.subscription_id
    }

    /// Canonical subscribe-request digest.
    pub const fn subscription_digest(self) -> Digest {
        self.subscription_digest
    }

    /// Monotonic event sequence.
    pub const fn event_sequence(self) -> u64 {
        self.event_sequence
    }

    /// Cumulative latest-only replacement count.
    pub const fn dropped_events(self) -> u64 {
        self.dropped_events
    }

    /// SHA-256 of the complete embedded canonical overview.
    pub const fn overview_digest(self) -> Digest {
        self.overview_digest
    }

    /// Independently validated complete overview.
    pub const fn overview(self) -> ResourceOverviewView<'a> {
        self.overview
    }
}

/// Returns the exact telemetry event length for a complete overview length.
pub fn telemetry_event_encoded_len(
    overview_bytes: usize,
) -> Result<usize, DiagnosticTransportError> {
    TELEMETRY_EVENT_HEADER_BYTES
        .checked_add(overview_bytes)
        .ok_or(DiagnosticTransportError::Length)
}

/// Encodes an event after binding its overview to the exact subscription.
pub fn encode_telemetry_event(
    event: &TelemetryEvent<'_>,
    subscription: TelemetrySubscribeView<'_>,
    output: &mut [u8],
    record_limits: DiagnosticLimits,
) -> Result<usize, DiagnosticTransportError> {
    if event.subscription_id != subscription.subscription_id()
        || event.subscription_digest != subscription.digest()
        || event.event_sequence == 0
    {
        return Err(DiagnosticTransportError::Identity);
    }
    let overview = decode_resource_overview(event.overview, record_limits)
        .map_err(DiagnosticTransportError::Record)?;
    validate_overview_binding(overview, subscription, event.event_sequence)?;
    let total = telemetry_event_encoded_len(event.overview.len())?;
    if total
        > usize::try_from(subscription.maximum_event_bytes())
            .map_err(|_| DiagnosticTransportError::Length)?
        || output.len() < total
    {
        return Err(DiagnosticTransportError::Limit("telemetry event bytes"));
    }
    let encoded = &mut output[..total];
    encoded.fill(0);
    encoded[0..8].copy_from_slice(&TELEMETRY_EVENT_MAGIC);
    encoded[8..10].copy_from_slice(&DIAGNOSTIC_TRANSPORT_VERSION.to_le_bytes());
    encoded[12..16].copy_from_slice(&to_u32(total)?.to_le_bytes());
    encoded[16..24].copy_from_slice(&event.subscription_id.get().to_le_bytes());
    encoded[24..56].copy_from_slice(&event.subscription_digest.0);
    encoded[56..64].copy_from_slice(&event.event_sequence.to_le_bytes());
    encoded[64..72].copy_from_slice(&event.dropped_events.to_le_bytes());
    encoded[72..76].copy_from_slice(&to_u32(event.overview.len())?.to_le_bytes());
    encoded[80..112].copy_from_slice(&sha256(event.overview).0);
    encoded[112..].copy_from_slice(event.overview);
    Ok(total)
}

/// Decodes and independently binds an event to the exact subscription.
pub fn decode_telemetry_event<'a>(
    encoded: &'a [u8],
    subscription: TelemetrySubscribeView<'_>,
    record_limits: DiagnosticLimits,
) -> Result<TelemetryEventView<'a>, DiagnosticTransportError> {
    validate_prefix(encoded, TELEMETRY_EVENT_MAGIC, TELEMETRY_EVENT_HEADER_BYTES)?;
    if encoded[10..12].iter().any(|byte| *byte != 0)
        || encoded[76..80].iter().any(|byte| *byte != 0)
    {
        return Err(DiagnosticTransportError::Reserved);
    }
    if encoded.len()
        > usize::try_from(subscription.maximum_event_bytes())
            .map_err(|_| DiagnosticTransportError::Length)?
    {
        return Err(DiagnosticTransportError::Limit("telemetry event bytes"));
    }
    let overview_len =
        usize::try_from(read_u32(encoded, 72)).map_err(|_| DiagnosticTransportError::Length)?;
    if encoded.len() != telemetry_event_encoded_len(overview_len)? {
        return Err(DiagnosticTransportError::Length);
    }
    let subscription_id = SubscriptionId::new(read_u64(encoded, 16))?;
    let subscription_digest = read_digest(encoded, 24);
    if subscription_id != subscription.subscription_id()
        || subscription_digest != subscription.digest()
    {
        return Err(DiagnosticTransportError::Identity);
    }
    let event_sequence = read_u64(encoded, 56);
    if event_sequence == 0 {
        return Err(DiagnosticTransportError::State);
    }
    let overview_bytes = &encoded[TELEMETRY_EVENT_HEADER_BYTES..];
    let overview_digest = read_digest(encoded, 80);
    if sha256(overview_bytes) != overview_digest {
        return Err(DiagnosticTransportError::Integrity);
    }
    let overview = decode_resource_overview(overview_bytes, record_limits)
        .map_err(DiagnosticTransportError::Record)?;
    validate_overview_binding(overview, subscription, event_sequence)?;
    Ok(TelemetryEventView {
        subscription_id,
        subscription_digest,
        event_sequence,
        dropped_events: read_u64(encoded, 64),
        overview_digest,
        overview,
    })
}

fn validate_overview_binding(
    overview: ResourceOverviewView<'_>,
    subscription: TelemetrySubscribeView<'_>,
    event_sequence: u64,
) -> Result<(), DiagnosticTransportError> {
    if overview.context() != subscription.context() {
        return Err(DiagnosticTransportError::Context);
    }
    if overview.sequence() != event_sequence
        || overview.sample_count() != subscription.resource_count()
    {
        return Err(DiagnosticTransportError::State);
    }
    if !overview
        .samples()
        .map(|sample| sample.resource)
        .eq(subscription.resources())
    {
        return Err(DiagnosticTransportError::Resource);
    }
    Ok(())
}

/// Waveform acquisition behavior flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct WaveformConfigureFlags(pub u16);

impl WaveformConfigureFlags {
    /// Require acquisition timestamps from an edge-aware peripheral path.
    pub const EDGE_TIMESTAMPS: u16 = WAVEFORM_CONFIGURE_FLAG_EDGE_TIMESTAMPS;
    /// Explicitly permit qualified bounded software sampling.
    pub const ALLOW_SOFTWARE: u16 = WAVEFORM_CONFIGURE_FLAG_ALLOW_SOFTWARE;

    /// Whether one behavior bit is present.
    pub const fn contains(self, flag: u16) -> bool {
        self.0 & flag != 0
    }
}

/// Borrowed canonical waveform configuration supplied to the encoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaveformConfigureRequest<'a> {
    /// Nonzero capture attempt identity.
    pub capture_id: CaptureId,
    /// Complete evidence context expected on the retained record.
    pub context: DiagnosticContext,
    /// Acquisition behavior and permitted fallback paths.
    pub flags: WaveformConfigureFlags,
    /// Requested cycles retained before a channel trigger.
    pub requested_pretrigger_cycles: u64,
    /// Requested cycles retained after the trigger.
    pub requested_posttrigger_cycles: u64,
    /// Earliest device cycle at which an arm may fire.
    pub earliest_trigger_cycle: DeviceCycle,
    /// Latest device cycle at which an arm may fire.
    pub latest_trigger_cycle: DeviceCycle,
    /// Fixed maximum transition records admitted for acquisition.
    pub transition_capacity: u32,
    /// Maximum range payload accepted in one chunk.
    pub maximum_chunk_bytes: u32,
    /// Trigger channel index, or `u16::MAX` for immediate capture.
    pub trigger_channel_index: u16,
    /// Trigger predicate.
    pub trigger_condition: DigitalTriggerCondition,
    /// Strictly increasing capture resources.
    pub channels: &'a [ResourceId],
}

/// Allocation-free validated waveform configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaveformConfigureView<'a> {
    capture_id: CaptureId,
    context: DiagnosticContext,
    flags: WaveformConfigureFlags,
    requested_pretrigger_cycles: u64,
    requested_posttrigger_cycles: u64,
    earliest_trigger_cycle: DeviceCycle,
    latest_trigger_cycle: DeviceCycle,
    transition_capacity: u32,
    maximum_chunk_bytes: u32,
    trigger_channel_index: u16,
    trigger_condition: DigitalTriggerCondition,
    channel_records: &'a [u8],
    channel_count: usize,
    digest: Digest,
}

impl<'a> WaveformConfigureView<'a> {
    /// Capture attempt identity.
    pub const fn capture_id(self) -> CaptureId {
        self.capture_id
    }

    /// Complete required diagnostic context.
    pub const fn context(self) -> DiagnosticContext {
        self.context
    }

    /// Acquisition behavior.
    pub const fn flags(self) -> WaveformConfigureFlags {
        self.flags
    }

    /// Requested `(pretrigger, posttrigger)` intervals.
    pub const fn requested_window_cycles(self) -> (u64, u64) {
        (
            self.requested_pretrigger_cycles,
            self.requested_posttrigger_cycles,
        )
    }

    /// Earliest and latest permitted trigger cycles, inclusive.
    pub const fn trigger_deadline_window(self) -> (DeviceCycle, DeviceCycle) {
        (self.earliest_trigger_cycle, self.latest_trigger_cycle)
    }

    /// Admitted fixed transition capacity.
    pub const fn transition_capacity(self) -> u32 {
        self.transition_capacity
    }

    /// Maximum range payload accepted in one chunk.
    pub const fn maximum_chunk_bytes(self) -> u32 {
        self.maximum_chunk_bytes
    }

    /// Trigger channel and predicate.
    pub const fn trigger(self) -> (u16, DigitalTriggerCondition) {
        (self.trigger_channel_index, self.trigger_condition)
    }

    /// Number of exact capture resources.
    pub const fn channel_count(self) -> usize {
        self.channel_count
    }

    /// Iterates validated channel resource selectors without allocation.
    pub fn channels(self) -> ResourceSelectorIter<'a> {
        ResourceSelectorIter {
            records: self.channel_records,
            offset: 0,
        }
    }

    /// SHA-256 of the complete canonical configure request.
    pub const fn digest(self) -> Digest {
        self.digest
    }
}

/// Returns the exact waveform configure length for a channel count.
pub fn waveform_configure_encoded_len(
    channel_count: usize,
) -> Result<usize, DiagnosticTransportError> {
    WAVEFORM_CONFIGURE_HEADER_BYTES
        .checked_add(
            channel_count
                .checked_mul(4)
                .ok_or(DiagnosticTransportError::Length)?,
        )
        .ok_or(DiagnosticTransportError::Length)
}

/// Encodes one canonical waveform configuration into caller-owned memory.
pub fn encode_waveform_configure(
    request: &WaveformConfigureRequest<'_>,
    output: &mut [u8],
    limits: DiagnosticTransportLimits,
) -> Result<usize, DiagnosticTransportError> {
    validate_context(request.context).map_err(DiagnosticTransportError::Record)?;
    validate_waveform_flags(request.flags)?;
    validate_waveform_fields(
        request.requested_pretrigger_cycles,
        request.requested_posttrigger_cycles,
        request.earliest_trigger_cycle,
        request.latest_trigger_cycle,
        request.transition_capacity,
        request.maximum_chunk_bytes,
        request.trigger_channel_index,
        request.trigger_condition,
        request.channels.len(),
        limits,
    )?;
    validate_resource_slice(request.channels, limits.maximum_waveform_channels)?;
    let total = waveform_configure_encoded_len(request.channels.len())?;
    if output.len() < total {
        return Err(DiagnosticTransportError::Length);
    }
    let encoded = &mut output[..total];
    encoded.fill(0);
    encoded[0..8].copy_from_slice(&WAVEFORM_CONFIGURE_MAGIC);
    encoded[8..10].copy_from_slice(&DIAGNOSTIC_TRANSPORT_VERSION.to_le_bytes());
    encoded[10..12].copy_from_slice(&request.flags.0.to_le_bytes());
    encoded[12..16].copy_from_slice(&to_u32(total)?.to_le_bytes());
    encoded[16..32].copy_from_slice(&request.capture_id.as_bytes());
    encode_context(request.context, &mut encoded[32..144]);
    encoded[144..152].copy_from_slice(&request.requested_pretrigger_cycles.to_le_bytes());
    encoded[152..160].copy_from_slice(&request.requested_posttrigger_cycles.to_le_bytes());
    encoded[160..168].copy_from_slice(&request.earliest_trigger_cycle.0.to_le_bytes());
    encoded[168..176].copy_from_slice(&request.latest_trigger_cycle.0.to_le_bytes());
    encoded[176..180].copy_from_slice(&request.transition_capacity.to_le_bytes());
    encoded[180..184].copy_from_slice(&request.maximum_chunk_bytes.to_le_bytes());
    encoded[184..186].copy_from_slice(&request.trigger_channel_index.to_le_bytes());
    encoded[186] = request.trigger_condition as u8;
    encoded[188..190].copy_from_slice(&to_u16(request.channels.len())?.to_le_bytes());
    for (index, resource) in request.channels.iter().copied().enumerate() {
        let start = WAVEFORM_CONFIGURE_HEADER_BYTES + index * 4;
        encoded[start..start + 4].copy_from_slice(&encode_resource_id(resource));
    }
    Ok(total)
}

/// Decodes and independently validates a complete waveform configuration.
pub fn decode_waveform_configure(
    encoded: &[u8],
    limits: DiagnosticTransportLimits,
) -> Result<WaveformConfigureView<'_>, DiagnosticTransportError> {
    validate_prefix(
        encoded,
        WAVEFORM_CONFIGURE_MAGIC,
        WAVEFORM_CONFIGURE_HEADER_BYTES,
    )?;
    if encoded[187] != 0 || encoded[190..192].iter().any(|byte| *byte != 0) {
        return Err(DiagnosticTransportError::Reserved);
    }
    let flags = WaveformConfigureFlags(read_u16(encoded, 10));
    validate_waveform_flags(flags)?;
    let channel_count = usize::from(read_u16(encoded, 188));
    if encoded.len() != waveform_configure_encoded_len(channel_count)? {
        return Err(DiagnosticTransportError::Length);
    }
    let trigger_condition =
        DigitalTriggerCondition::from_wire(encoded[186]).ok_or(DiagnosticTransportError::Window)?;
    validate_waveform_fields(
        read_u64(encoded, 144),
        read_u64(encoded, 152),
        DeviceCycle(read_u64(encoded, 160)),
        DeviceCycle(read_u64(encoded, 168)),
        read_u32(encoded, 176),
        read_u32(encoded, 180),
        read_u16(encoded, 184),
        trigger_condition,
        channel_count,
        limits,
    )?;
    let channel_records = &encoded[WAVEFORM_CONFIGURE_HEADER_BYTES..];
    validate_resource_records(channel_records)?;
    Ok(WaveformConfigureView {
        capture_id: read_capture_id(encoded, 16)?,
        context: decode_context(&encoded[32..144]).map_err(DiagnosticTransportError::Record)?,
        flags,
        requested_pretrigger_cycles: read_u64(encoded, 144),
        requested_posttrigger_cycles: read_u64(encoded, 152),
        earliest_trigger_cycle: DeviceCycle(read_u64(encoded, 160)),
        latest_trigger_cycle: DeviceCycle(read_u64(encoded, 168)),
        transition_capacity: read_u32(encoded, 176),
        maximum_chunk_bytes: read_u32(encoded, 180),
        trigger_channel_index: read_u16(encoded, 184),
        trigger_condition,
        channel_records,
        channel_count,
        digest: sha256(encoded),
    })
}

fn validate_waveform_flags(flags: WaveformConfigureFlags) -> Result<(), DiagnosticTransportError> {
    let known = WAVEFORM_CONFIGURE_FLAG_EDGE_TIMESTAMPS | WAVEFORM_CONFIGURE_FLAG_ALLOW_SOFTWARE;
    if flags.0 & !known != 0 || !flags.contains(WaveformConfigureFlags::EDGE_TIMESTAMPS) {
        Err(DiagnosticTransportError::Flags)
    } else {
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_waveform_fields(
    requested_pretrigger_cycles: u64,
    requested_posttrigger_cycles: u64,
    earliest_trigger_cycle: DeviceCycle,
    latest_trigger_cycle: DeviceCycle,
    transition_capacity: u32,
    maximum_chunk_bytes: u32,
    trigger_channel_index: u16,
    trigger_condition: DigitalTriggerCondition,
    channel_count: usize,
    limits: DiagnosticTransportLimits,
) -> Result<(), DiagnosticTransportError> {
    if requested_posttrigger_cycles == 0
        || earliest_trigger_cycle > latest_trigger_cycle
        || transition_capacity == 0
        || transition_capacity > limits.maximum_waveform_transitions
        || maximum_chunk_bytes == 0
        || maximum_chunk_bytes > limits.maximum_waveform_chunk_bytes
    {
        return Err(DiagnosticTransportError::Limit(
            "waveform acquisition budget",
        ));
    }
    if channel_count == 0
        || channel_count > super::MAX_DIGITAL_CAPTURE_CHANNELS
        || channel_count > usize::from(limits.maximum_waveform_channels)
    {
        return Err(DiagnosticTransportError::Limit("waveform channels"));
    }
    match trigger_condition {
        DigitalTriggerCondition::Immediate
            if trigger_channel_index == NO_TRIGGER_CHANNEL && requested_pretrigger_cycles == 0 => {}
        DigitalTriggerCondition::Immediate => return Err(DiagnosticTransportError::Window),
        _ if usize::from(trigger_channel_index) < channel_count => {}
        _ => return Err(DiagnosticTransportError::Window),
    }
    Ok(())
}

/// Exact reference to one canonical waveform configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaveformSessionRequest {
    /// Capture attempt identity.
    pub capture_id: CaptureId,
    /// SHA-256 of the complete canonical configure request.
    pub configure_digest: Digest,
}

impl WaveformSessionRequest {
    /// Encodes the exact fixed session reference.
    pub fn encode(self) -> Result<[u8; WAVEFORM_SESSION_WIRE_BYTES], DiagnosticTransportError> {
        validate_nonzero_digest(self.configure_digest)?;
        let mut encoded = [0_u8; WAVEFORM_SESSION_WIRE_BYTES];
        encoded[0..8].copy_from_slice(&WAVEFORM_SESSION_MAGIC);
        encoded[8..10].copy_from_slice(&DIAGNOSTIC_TRANSPORT_VERSION.to_le_bytes());
        encoded[12..16].copy_from_slice(&(WAVEFORM_SESSION_WIRE_BYTES as u32).to_le_bytes());
        encoded[16..32].copy_from_slice(&self.capture_id.as_bytes());
        encoded[32..64].copy_from_slice(&self.configure_digest.0);
        Ok(encoded)
    }

    /// Decodes an arm, stop, or status reference.
    pub fn decode(encoded: &[u8]) -> Result<Self, DiagnosticTransportError> {
        validate_prefix(encoded, WAVEFORM_SESSION_MAGIC, WAVEFORM_SESSION_WIRE_BYTES)?;
        if encoded[10..12].iter().any(|byte| *byte != 0) {
            return Err(DiagnosticTransportError::Reserved);
        }
        let request = Self {
            capture_id: read_capture_id(encoded, 16)?,
            configure_digest: read_digest(encoded, 32),
        };
        validate_nonzero_digest(request.configure_digest)?;
        if request.encode()? != encoded {
            return Err(DiagnosticTransportError::Reserved);
        }
        Ok(request)
    }
}

/// Lifecycle of one bounded waveform capture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum WaveformPhase {
    /// Configuration is admitted but not armed.
    Configured = 1,
    /// Acquisition is armed and waiting or retaining posttrigger data.
    Armed = 2,
    /// A complete canonical record is retained for range reads.
    Complete = 3,
    /// Acquisition was stopped and no record is retained.
    Stopped = 4,
    /// Acquisition failed; quality flags retain bounded public detail.
    Faulted = 5,
}

impl WaveformPhase {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Configured),
            2 => Some(Self::Armed),
            3 => Some(Self::Complete),
            4 => Some(Self::Stopped),
            5 => Some(Self::Faulted),
            _ => None,
        }
    }
}

/// Exact waveform lifecycle and retained-record status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaveformStatus {
    /// Lifecycle phase.
    pub phase: WaveformPhase,
    /// Whether the configured trigger fired.
    pub triggered: bool,
    /// Capture attempt identity.
    pub capture_id: CaptureId,
    /// Exact canonical configure-request digest.
    pub configure_digest: Digest,
    /// Monotonic generation assigned when the capture was armed.
    pub generation: u64,
    /// Exact cycle at which acquisition armed, or zero while configured.
    pub arm_cycle: DeviceCycle,
    /// Exact trigger cycle, or zero if no trigger fired.
    pub trigger_cycle: DeviceCycle,
    /// Complete retained capture length, or zero without a record.
    pub record_bytes: u32,
    /// Bytes passed by the live-delivery cursor, including acknowledged and
    /// explicitly dropped ranges; authoritative reads remain available.
    pub published_bytes: u32,
    /// SHA-256 of the complete retained canonical capture.
    pub record_digest: Digest,
    /// Chunks dropped before live delivery; range reads remain authoritative.
    pub dropped_chunks: u64,
    /// Capture-wide quality flags available before downloading the record.
    pub quality_flags: CaptureQualityFlags,
}

impl WaveformStatus {
    /// Whether a complete record is retained for exact range reads.
    pub const fn has_retained_record(self) -> bool {
        self.record_bytes != 0
    }

    /// Encodes the exact fixed status record.
    pub fn encode(self) -> Result<[u8; WAVEFORM_STATUS_WIRE_BYTES], DiagnosticTransportError> {
        self.validate()?;
        let mut encoded = [0_u8; WAVEFORM_STATUS_WIRE_BYTES];
        encoded[0..8].copy_from_slice(&WAVEFORM_STATUS_MAGIC);
        encoded[8..10].copy_from_slice(&DIAGNOSTIC_TRANSPORT_VERSION.to_le_bytes());
        encoded[10] = self.phase as u8;
        encoded[11] = (if self.triggered {
            WAVEFORM_STATUS_FLAG_TRIGGERED
        } else {
            0
        }) | (if self.has_retained_record() {
            WAVEFORM_STATUS_FLAG_RETAINED_RECORD
        } else {
            0
        });
        encoded[12..16].copy_from_slice(&(WAVEFORM_STATUS_WIRE_BYTES as u32).to_le_bytes());
        encoded[16..32].copy_from_slice(&self.capture_id.as_bytes());
        encoded[32..64].copy_from_slice(&self.configure_digest.0);
        encoded[64..72].copy_from_slice(&self.generation.to_le_bytes());
        encoded[72..80].copy_from_slice(&self.arm_cycle.0.to_le_bytes());
        encoded[80..88].copy_from_slice(&self.trigger_cycle.0.to_le_bytes());
        encoded[88..92].copy_from_slice(&self.record_bytes.to_le_bytes());
        encoded[92..96].copy_from_slice(&self.published_bytes.to_le_bytes());
        encoded[96..128].copy_from_slice(&self.record_digest.0);
        encoded[128..136].copy_from_slice(&self.dropped_chunks.to_le_bytes());
        encoded[136..140].copy_from_slice(&self.quality_flags.0.to_le_bytes());
        Ok(encoded)
    }

    /// Decodes and independently validates an exact fixed status record.
    pub fn decode(encoded: &[u8]) -> Result<Self, DiagnosticTransportError> {
        validate_prefix(encoded, WAVEFORM_STATUS_MAGIC, WAVEFORM_STATUS_WIRE_BYTES)?;
        let known_flags = WAVEFORM_STATUS_FLAG_TRIGGERED | WAVEFORM_STATUS_FLAG_RETAINED_RECORD;
        if encoded[11] & !known_flags != 0 || encoded[140..144].iter().any(|byte| *byte != 0) {
            return Err(DiagnosticTransportError::Flags);
        }
        let status = Self {
            phase: WaveformPhase::from_wire(encoded[10]).ok_or(DiagnosticTransportError::State)?,
            triggered: encoded[11] & WAVEFORM_STATUS_FLAG_TRIGGERED != 0,
            capture_id: read_capture_id(encoded, 16)?,
            configure_digest: read_digest(encoded, 32),
            generation: read_u64(encoded, 64),
            arm_cycle: DeviceCycle(read_u64(encoded, 72)),
            trigger_cycle: DeviceCycle(read_u64(encoded, 80)),
            record_bytes: read_u32(encoded, 88),
            published_bytes: read_u32(encoded, 92),
            record_digest: read_digest(encoded, 96),
            dropped_chunks: read_u64(encoded, 128),
            quality_flags: CaptureQualityFlags(read_u32(encoded, 136)),
        };
        if (encoded[11] & WAVEFORM_STATUS_FLAG_RETAINED_RECORD != 0) != status.has_retained_record()
        {
            return Err(DiagnosticTransportError::State);
        }
        status.validate()?;
        if status.encode()? != encoded {
            return Err(DiagnosticTransportError::Reserved);
        }
        Ok(status)
    }

    fn validate(self) -> Result<(), DiagnosticTransportError> {
        validate_nonzero_digest(self.configure_digest)?;
        let has_record = self.record_bytes != 0;
        if self.published_bytes > self.record_bytes
            || has_record == self.record_digest.is_zero()
            || self.triggered != (self.trigger_cycle.0 != 0)
        {
            return Err(DiagnosticTransportError::State);
        }
        match self.phase {
            WaveformPhase::Configured
                if self.generation == 0
                    && self.arm_cycle.0 == 0
                    && !self.triggered
                    && !has_record
                    && self.published_bytes == 0
                    && self.dropped_chunks == 0
                    && self.quality_flags.0 == 0 => {}
            WaveformPhase::Armed
                if self.generation != 0
                    && self.arm_cycle.0 != 0
                    && !has_record
                    && self.published_bytes == 0 => {}
            WaveformPhase::Complete
                if self.generation != 0 && self.arm_cycle.0 != 0 && has_record => {}
            WaveformPhase::Stopped
                if !has_record && self.published_bytes == 0 && self.record_digest.is_zero() => {}
            WaveformPhase::Faulted if !has_record => {}
            _ => return Err(DiagnosticTransportError::State),
        }
        if self.triggered && self.trigger_cycle < self.arm_cycle {
            return Err(DiagnosticTransportError::Window);
        }
        Ok(())
    }
}

/// Exact range request for a complete retained waveform record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaveformReadRequest {
    /// Capture attempt identity.
    pub capture_id: CaptureId,
    /// Exact canonical configure-request digest.
    pub configure_digest: Digest,
    /// Expected complete retained-record digest from status.
    pub record_digest: Digest,
    /// First requested byte in the complete canonical capture.
    pub offset: u32,
    /// Maximum requested payload bytes.
    pub maximum_bytes: u32,
}

impl WaveformReadRequest {
    /// Encodes the exact fixed range request.
    pub fn encode(
        self,
        limits: DiagnosticTransportLimits,
    ) -> Result<[u8; WAVEFORM_READ_WIRE_BYTES], DiagnosticTransportError> {
        self.validate(limits)?;
        let mut encoded = [0_u8; WAVEFORM_READ_WIRE_BYTES];
        encoded[0..8].copy_from_slice(&WAVEFORM_READ_MAGIC);
        encoded[8..10].copy_from_slice(&DIAGNOSTIC_TRANSPORT_VERSION.to_le_bytes());
        encoded[12..16].copy_from_slice(&(WAVEFORM_READ_WIRE_BYTES as u32).to_le_bytes());
        encoded[16..32].copy_from_slice(&self.capture_id.as_bytes());
        encoded[32..64].copy_from_slice(&self.configure_digest.0);
        encoded[64..96].copy_from_slice(&self.record_digest.0);
        encoded[96..100].copy_from_slice(&self.offset.to_le_bytes());
        encoded[100..104].copy_from_slice(&self.maximum_bytes.to_le_bytes());
        Ok(encoded)
    }

    /// Decodes and validates an exact fixed range request.
    pub fn decode(
        encoded: &[u8],
        limits: DiagnosticTransportLimits,
    ) -> Result<Self, DiagnosticTransportError> {
        validate_prefix(encoded, WAVEFORM_READ_MAGIC, WAVEFORM_READ_WIRE_BYTES)?;
        if encoded[10..12].iter().any(|byte| *byte != 0)
            || encoded[104..112].iter().any(|byte| *byte != 0)
        {
            return Err(DiagnosticTransportError::Reserved);
        }
        let request = Self {
            capture_id: read_capture_id(encoded, 16)?,
            configure_digest: read_digest(encoded, 32),
            record_digest: read_digest(encoded, 64),
            offset: read_u32(encoded, 96),
            maximum_bytes: read_u32(encoded, 100),
        };
        request.validate(limits)?;
        if request.encode(limits)? != encoded {
            return Err(DiagnosticTransportError::Reserved);
        }
        Ok(request)
    }

    fn validate(self, limits: DiagnosticTransportLimits) -> Result<(), DiagnosticTransportError> {
        validate_nonzero_digest(self.configure_digest)?;
        validate_nonzero_digest(self.record_digest)?;
        if self.maximum_bytes == 0 || self.maximum_bytes > limits.maximum_waveform_chunk_bytes {
            return Err(DiagnosticTransportError::Limit("waveform chunk bytes"));
        }
        self.offset
            .checked_add(self.maximum_bytes)
            .ok_or(DiagnosticTransportError::Range)?;
        Ok(())
    }
}

/// Borrowed waveform chunk supplied to the canonical encoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaveformChunk<'a> {
    /// Capture attempt identity.
    pub capture_id: CaptureId,
    /// Exact canonical configure-request digest.
    pub configure_digest: Digest,
    /// SHA-256 of the complete retained canonical capture.
    pub record_digest: Digest,
    /// Complete retained record length.
    pub record_bytes: u32,
    /// First byte represented by this chunk.
    pub offset: u32,
    /// Exact borrowed range bytes.
    pub bytes: &'a [u8],
}

/// Allocation-free validated waveform chunk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaveformChunkView<'a> {
    capture_id: CaptureId,
    configure_digest: Digest,
    record_digest: Digest,
    record_bytes: u32,
    offset: u32,
    chunk_digest: Digest,
    bytes: &'a [u8],
}

impl<'a> WaveformChunkView<'a> {
    /// Capture attempt identity.
    pub const fn capture_id(self) -> CaptureId {
        self.capture_id
    }

    /// Canonical configure-request digest.
    pub const fn configure_digest(self) -> Digest {
        self.configure_digest
    }

    /// Complete retained-record digest.
    pub const fn record_digest(self) -> Digest {
        self.record_digest
    }

    /// Complete retained-record length.
    pub const fn record_bytes(self) -> u32 {
        self.record_bytes
    }

    /// First represented byte.
    pub const fn offset(self) -> u32 {
        self.offset
    }

    /// SHA-256 of just this chunk's range bytes.
    pub const fn chunk_digest(self) -> Digest {
        self.chunk_digest
    }

    /// Exact borrowed range bytes.
    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// First byte after this chunk.
    pub fn end_offset(self) -> u32 {
        self.offset + u32::try_from(self.bytes.len()).expect("validated chunk length fits u32")
    }

    /// Whether this is the final contiguous range of the record.
    pub fn is_final(self) -> bool {
        self.end_offset() == self.record_bytes
    }
}

/// Returns the exact waveform chunk-envelope length for a range payload.
pub fn waveform_chunk_encoded_len(chunk_bytes: usize) -> Result<usize, DiagnosticTransportError> {
    WAVEFORM_CHUNK_HEADER_BYTES
        .checked_add(chunk_bytes)
        .ok_or(DiagnosticTransportError::Length)
}

/// Encodes one canonical waveform range chunk.
pub fn encode_waveform_chunk(
    chunk: &WaveformChunk<'_>,
    output: &mut [u8],
    limits: DiagnosticTransportLimits,
) -> Result<usize, DiagnosticTransportError> {
    validate_nonzero_digest(chunk.configure_digest)?;
    validate_nonzero_digest(chunk.record_digest)?;
    validate_chunk_range(chunk.record_bytes, chunk.offset, chunk.bytes.len(), limits)?;
    let total = waveform_chunk_encoded_len(chunk.bytes.len())?;
    if output.len() < total {
        return Err(DiagnosticTransportError::Length);
    }
    let end = chunk
        .offset
        .checked_add(to_u32(chunk.bytes.len())?)
        .ok_or(DiagnosticTransportError::Range)?;
    let encoded = &mut output[..total];
    encoded.fill(0);
    encoded[0..8].copy_from_slice(&WAVEFORM_CHUNK_MAGIC);
    encoded[8..10].copy_from_slice(&DIAGNOSTIC_TRANSPORT_VERSION.to_le_bytes());
    let flags = if end == chunk.record_bytes {
        WAVEFORM_CHUNK_FLAG_FINAL
    } else {
        0
    };
    encoded[10..12].copy_from_slice(&flags.to_le_bytes());
    encoded[12..16].copy_from_slice(&to_u32(total)?.to_le_bytes());
    encoded[16..32].copy_from_slice(&chunk.capture_id.as_bytes());
    encoded[32..64].copy_from_slice(&chunk.configure_digest.0);
    encoded[64..96].copy_from_slice(&chunk.record_digest.0);
    encoded[96..100].copy_from_slice(&chunk.record_bytes.to_le_bytes());
    encoded[100..104].copy_from_slice(&chunk.offset.to_le_bytes());
    encoded[104..108].copy_from_slice(&to_u32(chunk.bytes.len())?.to_le_bytes());
    encoded[112..144].copy_from_slice(&sha256(chunk.bytes).0);
    encoded[144..].copy_from_slice(chunk.bytes);
    Ok(total)
}

/// Decodes and independently validates one waveform range chunk.
pub fn decode_waveform_chunk(
    encoded: &[u8],
    expected: WaveformSessionRequest,
    expected_record_digest: Digest,
    limits: DiagnosticTransportLimits,
) -> Result<WaveformChunkView<'_>, DiagnosticTransportError> {
    validate_prefix(encoded, WAVEFORM_CHUNK_MAGIC, WAVEFORM_CHUNK_HEADER_BYTES)?;
    let flags = read_u16(encoded, 10);
    if flags & !WAVEFORM_CHUNK_FLAG_FINAL != 0 || encoded[108..112].iter().any(|byte| *byte != 0) {
        return Err(DiagnosticTransportError::Flags);
    }
    let chunk_len =
        usize::try_from(read_u32(encoded, 104)).map_err(|_| DiagnosticTransportError::Length)?;
    if encoded.len() != waveform_chunk_encoded_len(chunk_len)? {
        return Err(DiagnosticTransportError::Length);
    }
    let capture_id = read_capture_id(encoded, 16)?;
    let configure_digest = read_digest(encoded, 32);
    let record_digest = read_digest(encoded, 64);
    if capture_id != expected.capture_id
        || configure_digest != expected.configure_digest
        || record_digest != expected_record_digest
    {
        return Err(DiagnosticTransportError::Identity);
    }
    let record_bytes = read_u32(encoded, 96);
    let offset = read_u32(encoded, 100);
    let bytes = &encoded[WAVEFORM_CHUNK_HEADER_BYTES..];
    validate_chunk_range(record_bytes, offset, bytes.len(), limits)?;
    let final_chunk = offset
        .checked_add(to_u32(bytes.len())?)
        .ok_or(DiagnosticTransportError::Range)?
        == record_bytes;
    if (flags & WAVEFORM_CHUNK_FLAG_FINAL != 0) != final_chunk {
        return Err(DiagnosticTransportError::Range);
    }
    let chunk_digest = read_digest(encoded, 112);
    if sha256(bytes) != chunk_digest {
        return Err(DiagnosticTransportError::Integrity);
    }
    Ok(WaveformChunkView {
        capture_id,
        configure_digest,
        record_digest,
        record_bytes,
        offset,
        chunk_digest,
        bytes,
    })
}

/// Validates a complete retained digital capture against its configuration and
/// returns the record digest used by status/read/chunk reconciliation.
pub fn validate_retained_capture(
    encoded: &[u8],
    configuration: WaveformConfigureView<'_>,
    record_limits: DiagnosticLimits,
    transport_limits: DiagnosticTransportLimits,
) -> Result<Digest, DiagnosticTransportError> {
    if encoded.len()
        > usize::try_from(transport_limits.maximum_waveform_record_bytes)
            .map_err(|_| DiagnosticTransportError::Length)?
    {
        return Err(DiagnosticTransportError::Limit("waveform record bytes"));
    }
    let capture =
        decode_digital_capture(encoded, record_limits).map_err(DiagnosticTransportError::Record)?;
    if capture.context() != configuration.context()
        || capture.capture_id() != configuration.capture_id()
    {
        return Err(DiagnosticTransportError::Context);
    }
    if capture.requested_window_cycles() != configuration.requested_window_cycles()
        || capture.retention().0 != configuration.transition_capacity()
        || capture.channel_count() != configuration.channel_count()
    {
        return Err(DiagnosticTransportError::Window);
    }
    let (trigger_cycle, trigger_channel, trigger_condition, _) = capture.trigger();
    if trigger_condition != configuration.trigger().1
        || trigger_channel != configuration.trigger().0
        || !capture
            .channels()
            .map(|channel| channel.resource)
            .eq(configuration.channels())
    {
        return Err(DiagnosticTransportError::Resource);
    }
    if trigger_cycle.0 != 0 {
        let (earliest, latest) = configuration.trigger_deadline_window();
        if trigger_cycle < earliest || trigger_cycle > latest {
            return Err(DiagnosticTransportError::Window);
        }
    }
    let used_software = capture
        .channels()
        .any(|channel| channel.source == super::DigitalAcquisitionSource::Software);
    if used_software
        && !configuration
            .flags()
            .contains(WaveformConfigureFlags::ALLOW_SOFTWARE)
    {
        return Err(DiagnosticTransportError::State);
    }
    Ok(sha256(encoded))
}

fn validate_chunk_range(
    record_bytes: u32,
    offset: u32,
    chunk_len: usize,
    limits: DiagnosticTransportLimits,
) -> Result<(), DiagnosticTransportError> {
    let chunk_len = to_u32(chunk_len)?;
    if record_bytes == 0
        || record_bytes > limits.maximum_waveform_record_bytes
        || chunk_len == 0
        || chunk_len > limits.maximum_waveform_chunk_bytes
        || offset >= record_bytes
        || offset
            .checked_add(chunk_len)
            .is_none_or(|end| end > record_bytes)
    {
        return Err(DiagnosticTransportError::Range);
    }
    Ok(())
}

fn validate_prefix(
    encoded: &[u8],
    magic: [u8; 8],
    minimum_len: usize,
) -> Result<(), DiagnosticTransportError> {
    if encoded.len() < minimum_len {
        return Err(DiagnosticTransportError::Length);
    }
    if encoded[0..8] != magic {
        return Err(DiagnosticTransportError::Magic);
    }
    if read_u16(encoded, 8) != DIAGNOSTIC_TRANSPORT_VERSION {
        return Err(DiagnosticTransportError::Version);
    }
    if read_u32(encoded, 12) != to_u32(encoded.len())? {
        return Err(DiagnosticTransportError::Length);
    }
    Ok(())
}

fn validate_nonzero_digest(digest: Digest) -> Result<(), DiagnosticTransportError> {
    if digest.is_zero() {
        Err(DiagnosticTransportError::Identity)
    } else {
        Ok(())
    }
}

fn read_digest(encoded: &[u8], offset: usize) -> Digest {
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(&encoded[offset..offset + 32]);
    Digest(bytes)
}

fn read_capture_id(encoded: &[u8], offset: usize) -> Result<CaptureId, DiagnosticTransportError> {
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&encoded[offset..offset + 16]);
    CaptureId::new(bytes).map_err(DiagnosticTransportError::Record)
}

fn sha256(bytes: &[u8]) -> Digest {
    let digest = Sha256::digest(bytes);
    let mut result = [0_u8; 32];
    result.copy_from_slice(&digest);
    Digest(result)
}

fn to_u16(value: usize) -> Result<u16, DiagnosticTransportError> {
    u16::try_from(value).map_err(|_| DiagnosticTransportError::Length)
}

fn to_u32(value: usize) -> Result<u32, DiagnosticTransportError> {
    u32::try_from(value).map_err(|_| DiagnosticTransportError::Length)
}

#[cfg(test)]
mod tests {
    use alumina_capability::CapabilityIdentity;
    use alumina_clock::{BOOT_ID_BYTES, BootId};
    use alumina_protocol::DeviceId;

    use super::*;
    use crate::{
        CaptureQualityFlags, DigitalAcquisitionSource, DigitalCaptureChannel,
        DigitalCaptureChannel as Channel, DigitalCaptureDocument, DigitalCaptureFlags,
        DigitalCaptureState, DigitalChannelFlags, DigitalLevel, DigitalTransition,
        DigitalTransitionFlags, OverviewFlags, ResourceOverviewDocument, ResourceOverviewSample,
        ResourceValue, SampleProvenance, SampleQuality, SampleQualityFlags, encode_digital_capture,
        encode_resource_overview,
    };

    const RESOURCES: [ResourceId; 4] = [
        ResourceId::Gpio(22),
        ResourceId::Gpio(32),
        ResourceId::Gpio(33),
        ResourceId::Gpio(35),
    ];

    fn context() -> DiagnosticContext {
        DiagnosticContext {
            device_id: DeviceId(*b"ALUM-SIM:TINYBEE"),
            boot_id: BootId::new([0x53; BOOT_ID_BYTES]).unwrap(),
            capability: CapabilityIdentity {
                byte_len: 3_435,
                digest: Digest([0x71; 32]),
            },
            config_digest: Digest([0x72; 32]),
            clock_frequency_hz: 1_000_000,
        }
    }

    fn subscription_request() -> TelemetrySubscribeRequest<'static> {
        TelemetrySubscribeRequest {
            subscription_id: SubscriptionId::new(7).unwrap(),
            context: context(),
            flags: TelemetrySubscribeFlags(TelemetrySubscribeFlags::LATEST_ONLY),
            minimum_period_cycles: 10_000,
            maximum_event_bytes: 432,
            resources: &RESOURCES,
        }
    }

    fn encode_subscription() -> [u8; 176] {
        let mut encoded = [0_u8; 176];
        assert_eq!(
            encode_telemetry_subscribe(
                &subscription_request(),
                &mut encoded,
                DiagnosticTransportLimits::native_control(),
            ),
            Ok(encoded.len())
        );
        encoded
    }

    fn overview_samples() -> [ResourceOverviewSample; 4] {
        RESOURCES.map(|resource| ResourceOverviewSample {
            resource,
            provenance: SampleProvenance::Simulated,
            quality: SampleQuality::Valid,
            quality_flags: SampleQualityFlags(SampleQualityFlags::DEBOUNCED),
            captured_cycle: DeviceCycle(1_900),
            value: ResourceValue::Boolean(resource == ResourceId::Gpio(32)),
        })
    }

    fn encode_overview(sequence: u64) -> [u8; 320] {
        let samples = overview_samples();
        let mut encoded = [0_u8; 320];
        encode_resource_overview(
            &ResourceOverviewDocument {
                context: context(),
                flags: OverviewFlags(OverviewFlags::SIMULATED),
                snapshot_cycle: DeviceCycle(2_000),
                sequence,
                samples: &samples,
            },
            &mut encoded,
        )
        .unwrap();
        encoded
    }

    fn channels() -> [Channel; 4] {
        RESOURCES.map(|resource| DigitalCaptureChannel {
            resource,
            initial_level: DigitalLevel::Low,
            source: DigitalAcquisitionSource::Simulated,
            flags: DigitalChannelFlags(0),
        })
    }

    fn transitions() -> [DigitalTransition; 6] {
        [
            (100, 0, DigitalLevel::High),
            (200, 0, DigitalLevel::Low),
            (300, 1, DigitalLevel::High),
            (400, 1, DigitalLevel::Low),
            (500, 2, DigitalLevel::High),
            (600, 2, DigitalLevel::Low),
        ]
        .map(|(offset_cycles, channel_index, level)| DigitalTransition {
            offset_cycles,
            channel_index,
            level,
            flags: DigitalTransitionFlags(0),
        })
    }

    fn waveform_request() -> WaveformConfigureRequest<'static> {
        WaveformConfigureRequest {
            capture_id: CaptureId::new(*b"TINYBEE-SIM-0001").unwrap(),
            context: context(),
            flags: WaveformConfigureFlags(WaveformConfigureFlags::EDGE_TIMESTAMPS),
            requested_pretrigger_cycles: 500,
            requested_posttrigger_cycles: 1_500,
            earliest_trigger_cycle: DeviceCycle(2_400),
            latest_trigger_cycle: DeviceCycle(2_700),
            transition_capacity: 64,
            maximum_chunk_bytes: 168,
            trigger_channel_index: 2,
            trigger_condition: DigitalTriggerCondition::Rising,
            channels: &RESOURCES,
        }
    }

    fn encode_configuration() -> [u8; 208] {
        let mut encoded = [0_u8; 208];
        assert_eq!(
            encode_waveform_configure(
                &waveform_request(),
                &mut encoded,
                DiagnosticTransportLimits::native_control(),
            ),
            Ok(encoded.len())
        );
        encoded
    }

    fn encode_capture() -> [u8; 384] {
        let channels = channels();
        let transitions = transitions();
        let mut encoded = [0_u8; 384];
        encode_digital_capture(
            &DigitalCaptureDocument {
                context: context(),
                flags: DigitalCaptureFlags(DigitalCaptureFlags::SIMULATED),
                capture_id: waveform_request().capture_id,
                start_cycle: DeviceCycle(2_000),
                end_cycle_exclusive: DeviceCycle(4_000),
                requested_pretrigger_cycles: 500,
                requested_posttrigger_cycles: 1_500,
                trigger_cycle: DeviceCycle(2_500),
                trigger_channel_index: 2,
                trigger_condition: DigitalTriggerCondition::Rising,
                state: DigitalCaptureState::TriggeredComplete,
                trigger_transition_index: 4,
                transition_capacity: 64,
                retained_event_stride: 1,
                quality_flags: CaptureQualityFlags(CaptureQualityFlags::CLOCK_UNQUALIFIED),
                channels: &channels,
                transitions: &transitions,
            },
            &mut encoded,
        )
        .unwrap();
        encoded
    }

    #[test]
    fn telemetry_subscription_event_and_status_round_trip_with_exact_binding() {
        let request_bytes = encode_subscription();
        let subscription =
            decode_telemetry_subscribe(&request_bytes, DiagnosticTransportLimits::native_control())
                .unwrap();
        assert!(subscription.resources().eq(RESOURCES));
        assert_eq!(subscription.digest(), sha256(&request_bytes));

        let overview = encode_overview(9);
        let event = TelemetryEvent {
            subscription_id: subscription.subscription_id(),
            subscription_digest: subscription.digest(),
            event_sequence: 9,
            dropped_events: 2,
            overview: &overview,
        };
        let mut event_bytes = [0_u8; 432];
        assert_eq!(
            encode_telemetry_event(
                &event,
                subscription,
                &mut event_bytes,
                DiagnosticLimits::interactive(),
            ),
            Ok(event_bytes.len())
        );
        let decoded =
            decode_telemetry_event(&event_bytes, subscription, DiagnosticLimits::interactive())
                .unwrap();
        assert_eq!(decoded.event_sequence(), 9);
        assert_eq!(decoded.dropped_events(), 2);
        assert_eq!(decoded.overview().context(), context());

        let status = TelemetrySubscriptionStatus {
            phase: TelemetryPhase::Active,
            pending: true,
            subscription_id: subscription.subscription_id(),
            subscription_digest: subscription.digest(),
            minimum_period_cycles: subscription.minimum_period_cycles(),
            maximum_event_bytes: subscription.maximum_event_bytes(),
            resource_count: u16::try_from(subscription.resource_count()).unwrap(),
            next_event_sequence: 10,
            published_events: 6,
            dropped_events: 2,
            last_event_cycle: DeviceCycle(2_000),
            pending_event_sequence: 9,
            pending_event_bytes: 432,
        };
        assert_eq!(
            TelemetrySubscriptionStatus::decode(&status.encode().unwrap()),
            Ok(status)
        );
        let reference = TelemetrySessionRequest {
            subscription_id: subscription.subscription_id(),
            subscription_digest: subscription.digest(),
        };
        assert_eq!(
            TelemetrySessionRequest::decode(&reference.encode().unwrap()),
            Ok(reference)
        );
    }

    #[test]
    fn telemetry_rejects_substitution_tampering_and_unbounded_selectors() {
        let mut request_bytes = encode_subscription();
        request_bytes[160..164].copy_from_slice(&encode_resource_id(ResourceId::Gpio(35)));
        assert_eq!(
            decode_telemetry_subscribe(&request_bytes, DiagnosticTransportLimits::native_control()),
            Err(DiagnosticTransportError::Resource)
        );

        let request_bytes = encode_subscription();
        let subscription =
            decode_telemetry_subscribe(&request_bytes, DiagnosticTransportLimits::native_control())
                .unwrap();
        let overview = encode_overview(9);
        let mut event_bytes = [0_u8; 432];
        encode_telemetry_event(
            &TelemetryEvent {
                subscription_id: subscription.subscription_id(),
                subscription_digest: subscription.digest(),
                event_sequence: 9,
                dropped_events: 0,
                overview: &overview,
            },
            subscription,
            &mut event_bytes,
            DiagnosticLimits::interactive(),
        )
        .unwrap();
        event_bytes[200] ^= 1;
        assert_eq!(
            decode_telemetry_event(&event_bytes, subscription, DiagnosticLimits::interactive()),
            Err(DiagnosticTransportError::Integrity)
        );

        let mut too_many = [0_u8; TELEMETRY_SUBSCRIBE_HEADER_BYTES + 257 * 4];
        too_many[0..8].copy_from_slice(&TELEMETRY_SUBSCRIBE_MAGIC);
        too_many[8..10].copy_from_slice(&DIAGNOSTIC_TRANSPORT_VERSION.to_le_bytes());
        too_many[10..12].copy_from_slice(&TelemetrySubscribeFlags::LATEST_ONLY.to_le_bytes());
        let too_many_len = u32::try_from(too_many.len()).unwrap();
        too_many[12..16].copy_from_slice(&too_many_len.to_le_bytes());
        too_many[36..38].copy_from_slice(&257_u16.to_le_bytes());
        assert_eq!(
            decode_telemetry_subscribe(&too_many, DiagnosticTransportLimits::native_control()),
            Err(DiagnosticTransportError::Limit("telemetry resources"))
        );
    }

    #[test]
    fn waveform_configuration_record_status_and_ranges_round_trip() {
        let configuration_bytes = encode_configuration();
        let configuration = decode_waveform_configure(
            &configuration_bytes,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        assert!(configuration.channels().eq(RESOURCES));
        assert_eq!(configuration.digest(), sha256(&configuration_bytes));

        let capture = encode_capture();
        let record_digest = validate_retained_capture(
            &capture,
            configuration,
            DiagnosticLimits::interactive(),
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        assert_eq!(record_digest, sha256(&capture));

        let reference = WaveformSessionRequest {
            capture_id: configuration.capture_id(),
            configure_digest: configuration.digest(),
        };
        assert_eq!(
            WaveformSessionRequest::decode(&reference.encode().unwrap()),
            Ok(reference)
        );
        let status = WaveformStatus {
            phase: WaveformPhase::Complete,
            triggered: true,
            capture_id: configuration.capture_id(),
            configure_digest: configuration.digest(),
            generation: 1,
            arm_cycle: DeviceCycle(2_400),
            trigger_cycle: DeviceCycle(2_500),
            record_bytes: u32::try_from(capture.len()).unwrap(),
            published_bytes: 168,
            record_digest,
            dropped_chunks: 1,
            quality_flags: CaptureQualityFlags(CaptureQualityFlags::CLOCK_UNQUALIFIED),
        };
        assert_eq!(
            WaveformStatus::decode(&status.encode().unwrap()),
            Ok(status)
        );

        let read = WaveformReadRequest {
            capture_id: reference.capture_id,
            configure_digest: reference.configure_digest,
            record_digest,
            offset: 336,
            maximum_bytes: 48,
        };
        assert_eq!(
            WaveformReadRequest::decode(
                &read
                    .encode(DiagnosticTransportLimits::native_control())
                    .unwrap(),
                DiagnosticTransportLimits::native_control(),
            ),
            Ok(read)
        );

        let mut chunk_bytes = [0_u8; 192];
        assert_eq!(
            encode_waveform_chunk(
                &WaveformChunk {
                    capture_id: reference.capture_id,
                    configure_digest: reference.configure_digest,
                    record_digest,
                    record_bytes: u32::try_from(capture.len()).unwrap(),
                    offset: 336,
                    bytes: &capture[336..],
                },
                &mut chunk_bytes,
                DiagnosticTransportLimits::native_control(),
            ),
            Ok(chunk_bytes.len())
        );
        let chunk = decode_waveform_chunk(
            &chunk_bytes,
            reference,
            record_digest,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        assert!(chunk.is_final());
        assert_eq!(chunk.offset(), 336);
        assert_eq!(chunk.bytes(), &capture[336..]);
    }

    #[test]
    fn waveform_rejects_invalid_trigger_record_and_chunk_integrity() {
        let mut immediate = waveform_request();
        immediate.trigger_condition = DigitalTriggerCondition::Immediate;
        immediate.trigger_channel_index = NO_TRIGGER_CHANNEL;
        let mut output = [0_u8; 208];
        assert_eq!(
            encode_waveform_configure(
                &immediate,
                &mut output,
                DiagnosticTransportLimits::native_control()
            ),
            Err(DiagnosticTransportError::Window)
        );

        let configuration_bytes = encode_configuration();
        let configuration = decode_waveform_configure(
            &configuration_bytes,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        let mut capture = encode_capture();
        capture[40] ^= 1;
        assert!(matches!(
            validate_retained_capture(
                &capture,
                configuration,
                DiagnosticLimits::interactive(),
                DiagnosticTransportLimits::native_control(),
            ),
            Err(DiagnosticTransportError::Context) | Err(DiagnosticTransportError::Record(_))
        ));

        let capture = encode_capture();
        let record_digest = sha256(&capture);
        let reference = WaveformSessionRequest {
            capture_id: configuration.capture_id(),
            configure_digest: configuration.digest(),
        };
        let mut chunk_bytes = [0_u8; 312];
        encode_waveform_chunk(
            &WaveformChunk {
                capture_id: reference.capture_id,
                configure_digest: reference.configure_digest,
                record_digest,
                record_bytes: u32::try_from(capture.len()).unwrap(),
                offset: 0,
                bytes: &capture[..168],
            },
            &mut chunk_bytes,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        chunk_bytes[200] ^= 1;
        assert_eq!(
            decode_waveform_chunk(
                &chunk_bytes,
                reference,
                record_digest,
                DiagnosticTransportLimits::native_control()
            ),
            Err(DiagnosticTransportError::Integrity)
        );
    }

    #[test]
    fn lifecycle_statuses_reject_ambiguous_or_noncanonical_state() {
        let request_bytes = encode_subscription();
        let subscription =
            decode_telemetry_subscribe(&request_bytes, DiagnosticTransportLimits::native_control())
                .unwrap();
        let invalid = TelemetrySubscriptionStatus {
            phase: TelemetryPhase::Unsubscribed,
            pending: true,
            subscription_id: subscription.subscription_id(),
            subscription_digest: subscription.digest(),
            minimum_period_cycles: 1,
            maximum_event_bytes: 432,
            resource_count: 4,
            next_event_sequence: 2,
            published_events: 0,
            dropped_events: 0,
            last_event_cycle: DeviceCycle(1),
            pending_event_sequence: 1,
            pending_event_bytes: 432,
        };
        assert_eq!(invalid.encode(), Err(DiagnosticTransportError::State));

        let configuration_bytes = encode_configuration();
        let configuration = decode_waveform_configure(
            &configuration_bytes,
            DiagnosticTransportLimits::native_control(),
        )
        .unwrap();
        let invalid = WaveformStatus {
            phase: WaveformPhase::Configured,
            triggered: false,
            capture_id: configuration.capture_id(),
            configure_digest: configuration.digest(),
            generation: 1,
            arm_cycle: DeviceCycle(0),
            trigger_cycle: DeviceCycle(0),
            record_bytes: 0,
            published_bytes: 0,
            record_digest: Digest::ZERO,
            dropped_chunks: 0,
            quality_flags: CaptureQualityFlags(0),
        };
        assert_eq!(invalid.encode(), Err(DiagnosticTransportError::State));
    }
}
