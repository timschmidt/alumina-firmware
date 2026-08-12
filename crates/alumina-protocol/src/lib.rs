#![no_std]
#![doc = "Bounded wire-level identities shared by Alumina firmware, simulator, and UI."]

use core::fmt;

/// Magic bytes at the beginning of every native Alumina frame.
pub const FRAME_MAGIC: [u8; 4] = *b"ALUM";

/// The exact protocol version implemented by this source tree.
pub const PROTOCOL_VERSION: u16 = 1;

/// A canonical protocol V1 SHA-256 content or configuration digest.
#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Digest(pub [u8; 32]);

impl Digest {
    /// The all-zero digest, reserved for "not established" fields.
    pub const ZERO: Self = Self([0; 32]);

    /// Returns true when no digest has been established.
    pub const fn is_zero(self) -> bool {
        let mut index = 0;
        while index < self.0.len() {
            if self.0[index] != 0 {
                return false;
            }
            index += 1;
        }
        true
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Digest(")?;
        for byte in &self.0[..4] {
            write!(formatter, "{byte:02x}")?;
        }
        formatter.write_str("…)")
    }
}

/// Stable identity provisioned into one physical MCU.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct DeviceId(pub [u8; 16]);

impl DeviceId {
    /// Canonical device identity derived from an ESP factory base MAC address.
    ///
    /// The ten-byte namespace prevents the six public MAC bytes from being
    /// confused with another device-identity scheme. The result is stable
    /// across firmware updates and contains no credential material.
    pub const fn from_esp_base_mac(mac: [u8; 6]) -> Self {
        let mut bytes = *b"ALUM-ESP1:\0\0\0\0\0\0";
        let mut index = 0;
        while index < mac.len() {
            bytes[10 + index] = mac[index];
            index += 1;
        }
        Self(bytes)
    }

    /// Whether no stable identity has been established.
    pub const fn is_zero(self) -> bool {
        let mut index = 0;
        while index < self.0.len() {
            if self.0[index] != 0 {
                return false;
            }
            index += 1;
        }
        true
    }
}

/// Boot-scoped identity. It changes on every restart and invalidates prepared work.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct BootId(pub u64);

/// An unwrapped tick in a device's declared monotonic clock domain.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct DeviceCycle(pub u64);

/// Native bounded frame families.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FrameKind {
    /// Identity and boot information.
    Identity = 1,
    /// Board and machine capability information.
    Capabilities = 2,
    /// A timestamped clock heartbeat.
    ClockSample = 3,
    /// Transactional configuration traffic.
    Configuration = 4,
    /// Immutable job upload and control traffic.
    Job = 5,
    /// Bounded real-time or service command traffic.
    Command = 6,
    /// Subscriptions and bounded sample/event telemetry.
    Telemetry = 7,
    /// Wi-Fi AP/STA discovery and transactional network configuration.
    Network = 8,
    /// Content-addressed storage and resumable upload traffic.
    Storage = 9,
    /// Device health and bounded resource-state snapshots.
    Health = 10,
    /// Latched fault events and physical-reset flow.
    Fault = 11,
    /// Triggered digital/analog acquisition traffic.
    Waveform = 12,
    /// Coupled firmware and interface update traffic.
    Update = 13,
    /// Published fixed graph package installation and lifecycle traffic.
    Graph = 14,
}

impl FrameKind {
    /// Returns the exact byte used by protocol V1.
    pub const fn wire_value(self) -> u8 {
        self as u8
    }

    /// Decodes one exact protocol V1 frame-family byte.
    pub const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Identity),
            2 => Some(Self::Capabilities),
            3 => Some(Self::ClockSample),
            4 => Some(Self::Configuration),
            5 => Some(Self::Job),
            6 => Some(Self::Command),
            7 => Some(Self::Telemetry),
            8 => Some(Self::Network),
            9 => Some(Self::Storage),
            10 => Some(Self::Health),
            11 => Some(Self::Fault),
            12 => Some(Self::Waveform),
            13 => Some(Self::Update),
            14 => Some(Self::Graph),
            _ => None,
        }
    }
}

/// Fixed prefix validated before any frame payload is decoded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct FrameHeader {
    /// Constant [`FRAME_MAGIC`].
    pub magic: [u8; 4],
    /// Must equal [`PROTOCOL_VERSION`]; no compatibility negotiation is performed.
    pub version: u16,
    /// Frame family.
    pub kind: FrameKind,
    /// Kind-specific flags. Unknown set bits are rejected by the payload decoder.
    pub flags: u8,
    /// Encoded payload size after this header.
    pub payload_len: u32,
    /// Monotonic sequence within the relevant stream.
    pub sequence: u32,
    /// Device-cycle timestamp or requested execution cycle.
    pub cycle: DeviceCycle,
    /// Configuration this frame was compiled or sampled against.
    pub config_digest: Digest,
}

impl FrameHeader {
    /// Exact encoded V1 prefix length. Rust layout is never used on the wire.
    pub const WIRE_LEN: usize = 56;

    /// Constructs a header for this exact protocol version.
    pub const fn new(
        kind: FrameKind,
        payload_len: u32,
        sequence: u32,
        cycle: DeviceCycle,
        config_digest: Digest,
    ) -> Self {
        Self {
            magic: FRAME_MAGIC,
            version: PROTOCOL_VERSION,
            kind,
            flags: 0,
            payload_len,
            sequence,
            cycle,
            config_digest,
        }
    }

    /// Validates the universal bounded prefix.
    pub fn validate(&self, maximum_payload_len: u32) -> Result<(), HeaderError> {
        if self.magic != FRAME_MAGIC {
            return Err(HeaderError::Magic);
        }
        if self.version != PROTOCOL_VERSION {
            return Err(HeaderError::Version {
                received: self.version,
            });
        }
        if self.flags != 0 {
            return Err(HeaderError::Flags {
                received: self.flags,
            });
        }
        if self.payload_len > maximum_payload_len {
            return Err(HeaderError::PayloadTooLarge {
                received: self.payload_len,
                maximum: maximum_payload_len,
            });
        }
        Ok(())
    }

    /// Encodes the header using explicit little-endian protocol V1 fields.
    pub fn encode(&self) -> [u8; Self::WIRE_LEN] {
        let mut encoded = [0_u8; Self::WIRE_LEN];
        encoded[0..4].copy_from_slice(&self.magic);
        encoded[4..6].copy_from_slice(&self.version.to_le_bytes());
        encoded[6] = self.kind.wire_value();
        encoded[7] = self.flags;
        encoded[8..12].copy_from_slice(&self.payload_len.to_le_bytes());
        encoded[12..16].copy_from_slice(&self.sequence.to_le_bytes());
        encoded[16..24].copy_from_slice(&self.cycle.0.to_le_bytes());
        encoded[24..56].copy_from_slice(&self.config_digest.0);
        encoded
    }

    /// Decodes and validates one exact-length protocol V1 header.
    pub fn decode(encoded: &[u8], maximum_payload_len: u32) -> Result<Self, HeaderError> {
        if encoded.len() != Self::WIRE_LEN {
            return Err(HeaderError::EncodedLength {
                received: encoded.len(),
                expected: Self::WIRE_LEN,
            });
        }
        let kind = FrameKind::from_wire(encoded[6]).ok_or(HeaderError::Kind {
            received: encoded[6],
        })?;
        let mut magic = [0_u8; 4];
        magic.copy_from_slice(&encoded[0..4]);
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&encoded[24..56]);
        let header = Self {
            magic,
            version: read_u16(encoded, 4),
            kind,
            flags: encoded[7],
            payload_len: read_u32(encoded, 8),
            sequence: read_u32(encoded, 12),
            cycle: DeviceCycle(read_u64(encoded, 16)),
            config_digest: Digest(digest),
        };
        header.validate(maximum_payload_len)?;
        Ok(header)
    }
}

/// Universal frame-prefix validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeaderError {
    /// The caller did not provide exactly one encoded header.
    EncodedLength {
        /// Bytes supplied.
        received: usize,
        /// Required V1 length.
        expected: usize,
    },
    /// Magic did not identify the native Alumina protocol.
    Magic,
    /// The frame belongs to another exact schema version.
    Version {
        /// Version found in the frame.
        received: u16,
    },
    /// Frame-family byte is not assigned in this exact protocol version.
    Kind {
        /// Unknown family byte.
        received: u8,
    },
    /// Universal V1 header flags must be zero.
    Flags {
        /// Unsupported flag bits.
        received: u8,
    },
    /// Payload length exceeded the receiving endpoint's fixed budget.
    PayloadTooLarge {
        /// Length found in the frame.
        received: u32,
        /// Maximum admitted by this endpoint.
        maximum: u32,
    },
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

/// Exact request/response/event operation carried at the start of a frame payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum Operation {
    /// Fetch stable and boot-scoped device identity.
    IdentityGet = 0x0101,
    /// Fetch the canonical board capability document.
    CapabilitiesGet = 0x0201,
    /// Exchange timestamped local-cycle samples.
    ClockHeartbeat = 0x0301,
    /// Fetch active and candidate configuration identity.
    ConfigurationGet = 0x0401,
    /// Validate a complete candidate configuration without committing it.
    ConfigurationValidate = 0x0402,
    /// Atomically activate a previously validated configuration.
    ConfigurationCommit = 0x0403,
    /// Discard a candidate or select the prior committed configuration.
    ConfigurationRollback = 0x0404,
    /// Inspect an immutable cached job partition.
    JobInspect = 0x0501,
    /// Preflight one cached partition and create a boot-bound prepared lease.
    JobPrepare = 0x0502,
    /// Install a nonce/digest-bound future local start cycle.
    JobCommit = 0x0503,
    /// Confirm an installed start only after every participant acknowledged.
    JobConfirm = 0x0509,
    /// Idempotently invalidate a prepared or abortable committed job.
    JobAbort = 0x0504,
    /// Request a bounded controlled hold.
    JobHold = 0x0505,
    /// Resume a held job from its certified continuation.
    JobResume = 0x0506,
    /// Cancel a job and enter its declared local safe outcome.
    JobCancel = 0x0507,
    /// Fetch cached/prepared/running/terminal job state.
    JobStatus = 0x0508,
    /// Submit one fixed binary scheduled command batch.
    CommandBatch = 0x0601,
    /// Acquire a bounded disarmed diagnostic-output lease.
    CommandDiagnosticLease = 0x0602,
    /// Release a diagnostic-output lease and restore safe states.
    CommandDiagnosticRelease = 0x0603,
    /// Subscribe to bounded telemetry families and rates.
    TelemetrySubscribe = 0x0701,
    /// Remove a telemetry subscription.
    TelemetryUnsubscribe = 0x0702,
    /// Device-originated bounded telemetry event or sample batch.
    TelemetryEvent = 0x0703,
    /// Fetch current AP/STA and credential-transaction state.
    NetworkStatus = 0x0801,
    /// Scan visible infrastructure WLANs.
    NetworkScan = 0x0802,
    /// Transactionally join one infrastructure WLAN.
    NetworkJoin = 0x0803,
    /// Leave infrastructure mode without losing AP recovery.
    NetworkLeave = 0x0804,
    /// Restore the protected device AP recovery path.
    NetworkRecoverAp = 0x0805,
    /// Fetch storage capacity, health, and mutation state.
    StorageStatus = 0x0901,
    /// List bounded manifest/blob records.
    StorageList = 0x0902,
    /// Create or resume one content-addressed upload session.
    StorageBeginUpload = 0x0903,
    /// Store one fixed-size content-addressed chunk.
    StoragePutChunk = 0x0904,
    /// Verify all chunks and atomically publish an immutable manifest.
    StorageFinalize = 0x0905,
    /// Read a bounded manifest/blob range for execution or audit.
    StorageRead = 0x0906,
    /// Delete one unreferenced object while disarmed and idle.
    StorageDelete = 0x0907,
    /// Verify cache integrity and report orphan/corrupt records.
    StorageScrub = 0x0908,
    /// Explicitly format and persist one exact raw cache region.
    StorageProvision = 0x0909,
    /// Inspect one exact typed object/manifest publication for retry reconciliation.
    StorageInspect = 0x090a,
    /// Fetch bounded executor, queue, memory, temperature, and radio health.
    HealthSnapshot = 0x0a01,
    /// Device-originated latched fault record.
    FaultEvent = 0x0b01,
    /// Begin a reset flow without clearing a physical latch.
    FaultResetRequest = 0x0b02,
    /// Confirm the local physical and policy reset conditions.
    FaultResetConfirm = 0x0b03,
    /// Configure a bounded digital/analog trigger and capture budget.
    WaveformConfigure = 0x0c01,
    /// Arm a validated waveform trigger.
    WaveformArm = 0x0c02,
    /// Device-originated waveform data chunk.
    WaveformChunk = 0x0c03,
    /// Stop acquisition and release its buffers/resources.
    WaveformStop = 0x0c04,
    /// Fetch coupled firmware/interface update slots and versions.
    UpdateInspect = 0x0d01,
    /// Begin a signed coupled update upload.
    UpdateBegin = 0x0d02,
    /// Store one update chunk.
    UpdatePutChunk = 0x0d03,
    /// Verify signature, manifest, component digests, and compatibility.
    UpdateFinalize = 0x0d04,
    /// Select a verified update for recoverable reboot.
    UpdateCommit = 0x0d05,
    /// Select the prior verified firmware/interface pair.
    UpdateRollback = 0x0d06,
    /// Fetch candidate and active deployed-graph identities.
    GraphGet = 0x0e01,
    /// Independently validate one already-published graph package on both cores.
    GraphInstall = 0x0e02,
    /// Activate one independently validated graph package.
    GraphActivate = 0x0e03,
    /// Discard a candidate or clear the selected active graph.
    GraphClear = 0x0e04,
}

impl Operation {
    /// Exact V1 operation number.
    pub const fn wire_value(self) -> u16 {
        self as u16
    }

    /// Frame family required for this operation.
    pub const fn frame_kind(self) -> FrameKind {
        match self {
            Self::IdentityGet => FrameKind::Identity,
            Self::CapabilitiesGet => FrameKind::Capabilities,
            Self::ClockHeartbeat => FrameKind::ClockSample,
            Self::ConfigurationGet
            | Self::ConfigurationValidate
            | Self::ConfigurationCommit
            | Self::ConfigurationRollback => FrameKind::Configuration,
            Self::JobInspect
            | Self::JobPrepare
            | Self::JobCommit
            | Self::JobConfirm
            | Self::JobAbort
            | Self::JobHold
            | Self::JobResume
            | Self::JobCancel
            | Self::JobStatus => FrameKind::Job,
            Self::CommandBatch | Self::CommandDiagnosticLease | Self::CommandDiagnosticRelease => {
                FrameKind::Command
            }
            Self::TelemetrySubscribe | Self::TelemetryUnsubscribe | Self::TelemetryEvent => {
                FrameKind::Telemetry
            }
            Self::NetworkStatus
            | Self::NetworkScan
            | Self::NetworkJoin
            | Self::NetworkLeave
            | Self::NetworkRecoverAp => FrameKind::Network,
            Self::StorageStatus
            | Self::StorageList
            | Self::StorageBeginUpload
            | Self::StoragePutChunk
            | Self::StorageFinalize
            | Self::StorageRead
            | Self::StorageDelete
            | Self::StorageScrub
            | Self::StorageProvision
            | Self::StorageInspect => FrameKind::Storage,
            Self::HealthSnapshot => FrameKind::Health,
            Self::FaultEvent | Self::FaultResetRequest | Self::FaultResetConfirm => {
                FrameKind::Fault
            }
            Self::WaveformConfigure
            | Self::WaveformArm
            | Self::WaveformChunk
            | Self::WaveformStop => FrameKind::Waveform,
            Self::UpdateInspect
            | Self::UpdateBegin
            | Self::UpdatePutChunk
            | Self::UpdateFinalize
            | Self::UpdateCommit
            | Self::UpdateRollback => FrameKind::Update,
            Self::GraphGet | Self::GraphInstall | Self::GraphActivate | Self::GraphClear => {
                FrameKind::Graph
            }
        }
    }

    /// Decodes one assigned protocol V1 operation number.
    pub const fn from_wire(value: u16) -> Option<Self> {
        match value {
            0x0101 => Some(Self::IdentityGet),
            0x0201 => Some(Self::CapabilitiesGet),
            0x0301 => Some(Self::ClockHeartbeat),
            0x0401 => Some(Self::ConfigurationGet),
            0x0402 => Some(Self::ConfigurationValidate),
            0x0403 => Some(Self::ConfigurationCommit),
            0x0404 => Some(Self::ConfigurationRollback),
            0x0501 => Some(Self::JobInspect),
            0x0502 => Some(Self::JobPrepare),
            0x0503 => Some(Self::JobCommit),
            0x0504 => Some(Self::JobAbort),
            0x0505 => Some(Self::JobHold),
            0x0506 => Some(Self::JobResume),
            0x0507 => Some(Self::JobCancel),
            0x0508 => Some(Self::JobStatus),
            0x0509 => Some(Self::JobConfirm),
            0x0601 => Some(Self::CommandBatch),
            0x0602 => Some(Self::CommandDiagnosticLease),
            0x0603 => Some(Self::CommandDiagnosticRelease),
            0x0701 => Some(Self::TelemetrySubscribe),
            0x0702 => Some(Self::TelemetryUnsubscribe),
            0x0703 => Some(Self::TelemetryEvent),
            0x0801 => Some(Self::NetworkStatus),
            0x0802 => Some(Self::NetworkScan),
            0x0803 => Some(Self::NetworkJoin),
            0x0804 => Some(Self::NetworkLeave),
            0x0805 => Some(Self::NetworkRecoverAp),
            0x0901 => Some(Self::StorageStatus),
            0x0902 => Some(Self::StorageList),
            0x0903 => Some(Self::StorageBeginUpload),
            0x0904 => Some(Self::StoragePutChunk),
            0x0905 => Some(Self::StorageFinalize),
            0x0906 => Some(Self::StorageRead),
            0x0907 => Some(Self::StorageDelete),
            0x0908 => Some(Self::StorageScrub),
            0x0909 => Some(Self::StorageProvision),
            0x090a => Some(Self::StorageInspect),
            0x0a01 => Some(Self::HealthSnapshot),
            0x0b01 => Some(Self::FaultEvent),
            0x0b02 => Some(Self::FaultResetRequest),
            0x0b03 => Some(Self::FaultResetConfirm),
            0x0c01 => Some(Self::WaveformConfigure),
            0x0c02 => Some(Self::WaveformArm),
            0x0c03 => Some(Self::WaveformChunk),
            0x0c04 => Some(Self::WaveformStop),
            0x0d01 => Some(Self::UpdateInspect),
            0x0d02 => Some(Self::UpdateBegin),
            0x0d03 => Some(Self::UpdatePutChunk),
            0x0d04 => Some(Self::UpdateFinalize),
            0x0d05 => Some(Self::UpdateCommit),
            0x0d06 => Some(Self::UpdateRollback),
            0x0e01 => Some(Self::GraphGet),
            0x0e02 => Some(Self::GraphInstall),
            0x0e03 => Some(Self::GraphActivate),
            0x0e04 => Some(Self::GraphClear),
            _ => None,
        }
    }

    /// Whether V1 admits this operation in the requested message direction.
    pub const fn allows_direction(self, direction: MessageDirection) -> bool {
        match self {
            Self::TelemetryEvent | Self::FaultEvent | Self::WaveformChunk => {
                matches!(direction, MessageDirection::Event)
            }
            _ => matches!(
                direction,
                MessageDirection::Request | MessageDirection::Response
            ),
        }
    }
}

/// Payload message direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MessageDirection {
    /// Caller-to-device operation.
    Request = 0,
    /// Correlated response to a request.
    Response = 1,
    /// Uncorrelated device-originated event.
    Event = 2,
}

impl MessageDirection {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Request),
            1 => Some(Self::Response),
            2 => Some(Self::Event),
            _ => None,
        }
    }
}

/// Bounded protocol response status; detail remains a typed operation body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum StatusCode {
    /// Request succeeded or a request/event carries no response status.
    Ok = 0,
    /// Body, flags, range, or operation state was invalid.
    InvalidRequest = 1,
    /// Exact protocol/schema/component version did not match.
    VersionMismatch = 2,
    /// Device credentials were missing or invalid.
    Unauthorized = 3,
    /// Current safety/executor state forbids the operation.
    ForbiddenState = 4,
    /// Named object was not present.
    NotFound = 5,
    /// Existing identity/state conflicts with the requested mutation.
    Conflict = 6,
    /// Fixed memory, storage, queue, or rate budget was exhausted.
    Capacity = 7,
    /// Digest, signature, manifest, or stored bytes failed validation.
    Integrity = 8,
    /// A bounded operation is already in progress.
    Busy = 9,
    /// Requested execution/lease deadline cannot be admitted.
    Deadline = 10,
    /// Capability is not implemented or qualified on this package.
    Unsupported = 11,
    /// Internal failure with no more specific safe public status.
    Internal = 12,
}

impl StatusCode {
    const fn from_wire(value: u16) -> Option<Self> {
        match value {
            0 => Some(Self::Ok),
            1 => Some(Self::InvalidRequest),
            2 => Some(Self::VersionMismatch),
            3 => Some(Self::Unauthorized),
            4 => Some(Self::ForbiddenState),
            5 => Some(Self::NotFound),
            6 => Some(Self::Conflict),
            7 => Some(Self::Capacity),
            8 => Some(Self::Integrity),
            9 => Some(Self::Busy),
            10 => Some(Self::Deadline),
            11 => Some(Self::Unsupported),
            12 => Some(Self::Internal),
            _ => None,
        }
    }
}

/// Fixed operation prefix at the start of every nonempty V1 frame payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MessageHeader {
    /// Exact operation and frame-family binding.
    pub operation: Operation,
    /// Request, response, or device-originated event.
    pub direction: MessageDirection,
    /// Reserved V1 operation flags; must be zero.
    pub flags: u8,
    /// `Ok` for requests/events; typed result for responses.
    pub status: StatusCode,
    /// Nonzero for requests/responses and zero for events.
    pub correlation_id: u32,
    /// Encoded operation-specific bytes following this prefix.
    pub body_len: u32,
}

impl MessageHeader {
    /// Exact encoded V1 message-prefix length.
    pub const WIRE_LEN: usize = 16;

    /// Constructs a correlated request.
    pub const fn request(operation: Operation, correlation_id: u32, body_len: u32) -> Self {
        Self {
            operation,
            direction: MessageDirection::Request,
            flags: 0,
            status: StatusCode::Ok,
            correlation_id,
            body_len,
        }
    }

    /// Constructs a correlated response.
    pub const fn response(
        operation: Operation,
        correlation_id: u32,
        status: StatusCode,
        body_len: u32,
    ) -> Self {
        Self {
            operation,
            direction: MessageDirection::Response,
            flags: 0,
            status,
            correlation_id,
            body_len,
        }
    }

    /// Constructs an uncorrelated device event.
    pub const fn event(operation: Operation, body_len: u32) -> Self {
        Self {
            operation,
            direction: MessageDirection::Event,
            flags: 0,
            status: StatusCode::Ok,
            correlation_id: 0,
            body_len,
        }
    }

    /// Validates operation family, direction semantics, and outer payload length.
    pub fn validate(
        &self,
        outer_kind: FrameKind,
        outer_payload_len: u32,
    ) -> Result<(), MessageError> {
        if self.operation.frame_kind() != outer_kind {
            return Err(MessageError::FrameKind {
                operation: self.operation,
                received: outer_kind,
            });
        }
        if !self.operation.allows_direction(self.direction) {
            return Err(MessageError::DirectionForOperation {
                operation: self.operation,
                direction: self.direction,
            });
        }
        if self.flags != 0 {
            return Err(MessageError::Flags {
                received: self.flags,
            });
        }
        match self.direction {
            MessageDirection::Request | MessageDirection::Response if self.correlation_id == 0 => {
                return Err(MessageError::Correlation);
            }
            MessageDirection::Event if self.correlation_id != 0 => {
                return Err(MessageError::Correlation);
            }
            _ => {}
        }
        if self.direction != MessageDirection::Response && self.status != StatusCode::Ok {
            return Err(MessageError::StatusDirection);
        }
        let expected = u32::try_from(Self::WIRE_LEN)
            .expect("message header length fits u32")
            .checked_add(self.body_len)
            .ok_or(MessageError::PayloadLength {
                received: outer_payload_len,
                expected: None,
            })?;
        if outer_payload_len != expected {
            return Err(MessageError::PayloadLength {
                received: outer_payload_len,
                expected: Some(expected),
            });
        }
        Ok(())
    }

    /// Encodes this prefix using explicit little-endian V1 fields.
    pub fn encode(&self) -> [u8; Self::WIRE_LEN] {
        let mut encoded = [0_u8; Self::WIRE_LEN];
        encoded[0..2].copy_from_slice(&self.operation.wire_value().to_le_bytes());
        encoded[2] = self.direction as u8;
        encoded[3] = self.flags;
        encoded[4..6].copy_from_slice(&(self.status as u16).to_le_bytes());
        // Bytes 6..8 are reserved zero in V1.
        encoded[8..12].copy_from_slice(&self.correlation_id.to_le_bytes());
        encoded[12..16].copy_from_slice(&self.body_len.to_le_bytes());
        encoded
    }

    /// Decodes an exact-length operation prefix and rejects reserved values.
    pub fn decode(encoded: &[u8]) -> Result<Self, MessageError> {
        if encoded.len() != Self::WIRE_LEN {
            return Err(MessageError::EncodedLength {
                received: encoded.len(),
                expected: Self::WIRE_LEN,
            });
        }
        let operation_value = read_u16(encoded, 0);
        let operation = Operation::from_wire(operation_value).ok_or(MessageError::Operation {
            received: operation_value,
        })?;
        let direction = MessageDirection::from_wire(encoded[2]).ok_or(MessageError::Direction {
            received: encoded[2],
        })?;
        let status_value = read_u16(encoded, 4);
        let status = StatusCode::from_wire(status_value).ok_or(MessageError::Status {
            received: status_value,
        })?;
        let reserved = read_u16(encoded, 6);
        if reserved != 0 {
            return Err(MessageError::Reserved { received: reserved });
        }
        Ok(Self {
            operation,
            direction,
            flags: encoded[3],
            status,
            correlation_id: read_u32(encoded, 8),
            body_len: read_u32(encoded, 12),
        })
    }

    /// Decodes and immediately applies all outer-family/length semantics.
    pub fn decode_and_validate(
        encoded: &[u8],
        outer_kind: FrameKind,
        outer_payload_len: u32,
    ) -> Result<Self, MessageError> {
        let message = Self::decode(encoded)?;
        message.validate(outer_kind, outer_payload_len)?;
        Ok(message)
    }
}

/// Operation-prefix decode or semantic rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageError {
    /// Caller did not provide exactly one encoded prefix.
    EncodedLength {
        /// Bytes supplied.
        received: usize,
        /// Required V1 length.
        expected: usize,
    },
    /// Operation number is unassigned in V1.
    Operation {
        /// Unknown operation number.
        received: u16,
    },
    /// Direction byte is unassigned in V1.
    Direction {
        /// Unknown direction byte.
        received: u8,
    },
    /// Status number is unassigned in V1.
    Status {
        /// Unknown status number.
        received: u16,
    },
    /// Reserved prefix bytes were nonzero.
    Reserved {
        /// Unsupported reserved value.
        received: u16,
    },
    /// Operation belongs to another outer frame family.
    FrameKind {
        /// Decoded operation.
        operation: Operation,
        /// Outer family supplied by the frame.
        received: FrameKind,
    },
    /// V1 operation flags must be zero.
    Flags {
        /// Unsupported flag bits.
        received: u8,
    },
    /// Request/response/event direction is not admitted for this operation.
    DirectionForOperation {
        /// Exact operation.
        operation: Operation,
        /// Rejected direction.
        direction: MessageDirection,
    },
    /// Correlation ID did not match request/response/event semantics.
    Correlation,
    /// Requests/events cannot carry response failure status.
    StatusDirection,
    /// Message body length did not exactly consume the outer payload.
    PayloadLength {
        /// Outer payload bytes.
        received: u32,
        /// Exact expected length, or `None` after arithmetic overflow.
        expected: Option<u32>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_header_validates_at_exact_limit() {
        let header = FrameHeader::new(FrameKind::Command, 128, 7, DeviceCycle(42), Digest([3; 32]));

        assert_eq!(header.validate(128), Ok(()));
    }

    #[test]
    fn version_mismatch_is_not_negotiated() {
        let mut header = FrameHeader::new(FrameKind::Identity, 0, 0, DeviceCycle(0), Digest::ZERO);
        header.version += 1;

        assert_eq!(
            header.validate(0),
            Err(HeaderError::Version {
                received: PROTOCOL_VERSION + 1
            })
        );
    }

    #[test]
    fn oversized_payload_is_rejected_before_decode() {
        let header = FrameHeader::new(FrameKind::Telemetry, 65, 1, DeviceCycle(3), Digest::ZERO);

        assert_eq!(
            header.validate(64),
            Err(HeaderError::PayloadTooLarge {
                received: 65,
                maximum: 64
            })
        );
    }

    #[test]
    fn frame_header_has_an_explicit_golden_wire_image() {
        let header = FrameHeader {
            magic: FRAME_MAGIC,
            version: PROTOCOL_VERSION,
            kind: FrameKind::Storage,
            flags: 0,
            payload_len: 0x1122_3344,
            sequence: 0x5566_7788,
            cycle: DeviceCycle(0x0102_0304_0506_0708),
            config_digest: Digest([0xaa; 32]),
        };
        let mut expected = [0xaa; FrameHeader::WIRE_LEN];
        expected[0..24].copy_from_slice(&[
            0x41, 0x4c, 0x55, 0x4d, 0x01, 0x00, 0x09, 0x00, 0x44, 0x33, 0x22, 0x11, 0x88, 0x77,
            0x66, 0x55, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01,
        ]);

        let encoded = header.encode();
        assert_eq!(encoded, expected);
        assert_eq!(FrameHeader::decode(&encoded, 0x1122_3344), Ok(header));
    }

    #[test]
    fn esp_factory_mac_has_one_namespaced_stable_device_identity() {
        let device = DeviceId::from_esp_base_mac([0x24, 0x6f, 0x28, 0xaa, 0xbb, 0xcc]);
        assert_eq!(
            device.0,
            [
                b'A', b'L', b'U', b'M', b'-', b'E', b'S', b'P', b'1', b':', 0x24, 0x6f, 0x28, 0xaa,
                0xbb, 0xcc,
            ]
        );
        assert!(!device.is_zero());
        assert!(DeviceId::default().is_zero());
    }

    #[test]
    fn frame_decode_rejects_unknown_kind_flags_and_length() {
        let header = FrameHeader::new(FrameKind::Identity, 0, 1, DeviceCycle(0), Digest::ZERO);
        let mut encoded = header.encode();
        encoded[6] = 0xff;
        assert_eq!(
            FrameHeader::decode(&encoded, 0),
            Err(HeaderError::Kind { received: 0xff })
        );

        encoded = header.encode();
        encoded[7] = 0x80;
        assert_eq!(
            FrameHeader::decode(&encoded, 0),
            Err(HeaderError::Flags { received: 0x80 })
        );
        assert_eq!(
            FrameHeader::decode(&encoded[..55], 0),
            Err(HeaderError::EncodedLength {
                received: 55,
                expected: 56,
            })
        );
    }

    #[test]
    fn every_service_family_has_an_exact_operation_binding() {
        let representatives = [
            (Operation::IdentityGet, FrameKind::Identity),
            (Operation::CapabilitiesGet, FrameKind::Capabilities),
            (Operation::ClockHeartbeat, FrameKind::ClockSample),
            (Operation::ConfigurationCommit, FrameKind::Configuration),
            (Operation::JobPrepare, FrameKind::Job),
            (Operation::CommandBatch, FrameKind::Command),
            (Operation::TelemetryEvent, FrameKind::Telemetry),
            (Operation::NetworkJoin, FrameKind::Network),
            (Operation::StoragePutChunk, FrameKind::Storage),
            (Operation::HealthSnapshot, FrameKind::Health),
            (Operation::FaultEvent, FrameKind::Fault),
            (Operation::WaveformChunk, FrameKind::Waveform),
            (Operation::UpdateCommit, FrameKind::Update),
            (Operation::GraphInstall, FrameKind::Graph),
        ];
        for (operation, kind) in representatives {
            assert_eq!(operation.frame_kind(), kind);
            assert_eq!(
                Operation::from_wire(operation.wire_value()),
                Some(operation)
            );
        }
        assert_eq!(
            Operation::from_wire(0x0909),
            Some(Operation::StorageProvision)
        );
        assert_eq!(Operation::from_wire(0x0509), Some(Operation::JobConfirm));
        assert_eq!(
            Operation::from_wire(0x090a),
            Some(Operation::StorageInspect)
        );
        assert_eq!(Operation::from_wire(0x090b), None);
        assert_eq!(Operation::from_wire(0x0e04), Some(Operation::GraphClear));
        assert_eq!(Operation::from_wire(0x0e05), None);
    }

    #[test]
    fn message_header_has_golden_bytes_and_exact_outer_length() {
        let message = MessageHeader::request(Operation::StoragePutChunk, 0x1122_3344, 0x5566_7788);
        let expected = [
            0x04, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x44, 0x33, 0x22, 0x11, 0x88, 0x77,
            0x66, 0x55,
        ];
        assert_eq!(message.encode(), expected);
        assert_eq!(MessageHeader::decode(&expected), Ok(message));
        assert_eq!(
            MessageHeader::decode_and_validate(&expected, FrameKind::Storage, 16 + 0x5566_7788,),
            Ok(message)
        );
        assert_eq!(
            message.validate(FrameKind::Storage, 16 + 0x5566_7788),
            Ok(())
        );
        assert_eq!(
            message.validate(FrameKind::Job, 16 + 0x5566_7788),
            Err(MessageError::FrameKind {
                operation: Operation::StoragePutChunk,
                received: FrameKind::Job,
            })
        );
    }

    #[test]
    fn message_semantics_reject_ambiguous_or_malformed_prefixes() {
        let zero_correlation = MessageHeader::request(Operation::IdentityGet, 0, 0);
        assert_eq!(
            zero_correlation.validate(FrameKind::Identity, 16),
            Err(MessageError::Correlation)
        );

        let failed_event = MessageHeader {
            status: StatusCode::Internal,
            ..MessageHeader::event(Operation::FaultEvent, 2)
        };
        assert_eq!(
            failed_event.validate(FrameKind::Fault, 18),
            Err(MessageError::StatusDirection)
        );

        let request = MessageHeader::request(Operation::NetworkScan, 7, 4);
        assert_eq!(
            request.validate(FrameKind::Network, 19),
            Err(MessageError::PayloadLength {
                received: 19,
                expected: Some(20),
            })
        );

        let mut encoded = request.encode();
        encoded[6] = 1;
        assert_eq!(
            MessageHeader::decode(&encoded),
            Err(MessageError::Reserved { received: 1 })
        );

        let requested_event = MessageHeader::request(Operation::FaultEvent, 3, 0);
        assert_eq!(
            requested_event.validate(FrameKind::Fault, 16),
            Err(MessageError::DirectionForOperation {
                operation: Operation::FaultEvent,
                direction: MessageDirection::Request,
            })
        );
        let unsolicited_request = MessageHeader::event(Operation::NetworkScan, 0);
        assert_eq!(
            unsolicited_request.validate(FrameKind::Network, 16),
            Err(MessageError::DirectionForOperation {
                operation: Operation::NetworkScan,
                direction: MessageDirection::Event,
            })
        );
    }
}
