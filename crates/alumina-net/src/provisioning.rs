//! Canonical fixed-memory AP/STA provisioning bodies.
//!
//! Password bytes exist only in [`NetworkJoinRequest`]. Every status and scan
//! representation is credential-free, fixed-size, and independently
//! decodable by firmware, simulation, and the browser client.

use core::cmp::Ordering;
use core::fmt;

use super::{MAX_PASSPHRASE_BYTES, MAX_SSID_BYTES, MIN_PASSPHRASE_BYTES, NetworkPhase};

/// Canonical network-status body magic.
pub const NETWORK_STATUS_MAGIC: [u8; 8] = *b"ALMNST01";
/// Canonical network-scan request magic.
pub const NETWORK_SCAN_REQUEST_MAGIC: [u8; 8] = *b"ALMNSQ01";
/// Canonical network-scan result magic.
pub const NETWORK_SCAN_RESULT_MAGIC: [u8; 8] = *b"ALMNSR01";
/// Canonical network-join request magic.
pub const NETWORK_JOIN_REQUEST_MAGIC: [u8; 8] = *b"ALMNJN01";
/// Canonical leave/recovery mutation request magic.
pub const NETWORK_MUTATION_REQUEST_MAGIC: [u8; 8] = *b"ALMNMT01";
/// Maximum scan entries returned by one fixed service response.
pub const MAX_NETWORK_SCAN_RESULTS: usize = 8;
/// Exact encoded bytes in one network status.
pub const NETWORK_STATUS_WIRE_BYTES: usize = 80;
/// Exact encoded bytes in one network scan request.
pub const NETWORK_SCAN_REQUEST_WIRE_BYTES: usize = 16;
/// Exact encoded bytes in one network scan entry.
pub const NETWORK_SCAN_ENTRY_WIRE_BYTES: usize = 48;
/// Bytes before the scan-entry table.
pub const NETWORK_SCAN_RESULT_HEADER_BYTES: usize = 40;
/// Exact encoded bytes in one complete bounded scan result.
pub const NETWORK_SCAN_RESULT_WIRE_BYTES: usize =
    NETWORK_SCAN_RESULT_HEADER_BYTES + MAX_NETWORK_SCAN_RESULTS * NETWORK_SCAN_ENTRY_WIRE_BYTES;
/// Exact encoded bytes in one station-join request.
pub const NETWORK_JOIN_REQUEST_WIRE_BYTES: usize = 128;
/// Exact encoded bytes in one leave/recovery mutation request.
pub const NETWORK_MUTATION_REQUEST_WIRE_BYTES: usize = 24;

const JOIN_FLAG_BSSID: u8 = 1 << 0;
const JOIN_FLAG_CHANNEL: u8 = 1 << 1;
const JOIN_FLAGS_KNOWN: u8 = JOIN_FLAG_BSSID | JOIN_FLAG_CHANNEL;
const SCAN_FLAG_TRUNCATED: u8 = 1 << 0;
const SCAN_FLAGS_KNOWN: u8 = SCAN_FLAG_TRUNCATED;

/// Credential-free network-status flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct NetworkStatusFlags(pub u8);

impl NetworkStatusFlags {
    /// The protected recovery AP is expected to remain available.
    pub const AP_EXPECTED: u8 = 1 << 0;
    /// A station profile is selected in volatile state.
    pub const STATION_CONFIGURED: u8 = 1 << 1;
    /// The station associated at layer two.
    pub const STATION_ASSOCIATED: u8 = 1 << 2;
    /// DHCP supplied a usable IPv4 configuration.
    pub const STATION_IPV4_READY: u8 = 1 << 3;
    /// The selected station profile is durably committed.
    pub const CREDENTIALS_DURABLE: u8 = 1 << 4;
    /// The latest scan exceeded the fixed returned-entry budget.
    pub const SCAN_TRUNCATED: u8 = 1 << 5;
    /// The most recent network operation failed.
    pub const LAST_OPERATION_FAILED: u8 = 1 << 6;
    /// Radio recovery is required before provisioning can proceed.
    pub const RECOVERY_REQUIRED: u8 = 1 << 7;

    /// Whether one status bit is present.
    pub const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

/// Authentication advertised by a scanned or selected WLAN.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum NetworkAuthentication {
    /// Open WLAN with no passphrase.
    Open = 1,
    /// WPA2-Personal.
    Wpa2Personal = 2,
    /// WPA3-Personal.
    Wpa3Personal = 3,
    /// WPA2/WPA3 transition mode.
    Wpa2Wpa3Personal = 4,
    /// Visible but deliberately unsupported authentication.
    Unsupported = 255,
}

impl NetworkAuthentication {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Open),
            2 => Some(Self::Wpa2Personal),
            3 => Some(Self::Wpa3Personal),
            4 => Some(Self::Wpa2Wpa3Personal),
            255 => Some(Self::Unsupported),
            _ => None,
        }
    }

    /// Whether this release permits an association attempt.
    pub const fn joinable(self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

/// Station link and address progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StationLinkState {
    /// Station mode is inactive.
    Disabled = 0,
    /// Station mode is available but not associated.
    Disconnected = 1,
    /// An association transaction is in progress.
    Associating = 2,
    /// Layer-two association succeeded; no usable address is proven.
    Associated = 3,
    /// Association and DHCP IPv4 configuration both succeeded.
    Addressed = 4,
}

impl StationLinkState {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Disabled),
            1 => Some(Self::Disconnected),
            2 => Some(Self::Associating),
            3 => Some(Self::Associated),
            4 => Some(Self::Addressed),
            _ => None,
        }
    }
}

/// Stable, non-secret failure class retained for reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum NetworkFailure {
    /// No failure is retained.
    None = 0,
    /// Radio scan failed.
    Scan = 1,
    /// Association was rejected or disconnected.
    Association = 2,
    /// DHCP did not establish an address before its deadline.
    Dhcp = 3,
    /// The radio driver rejected a configuration or operation.
    Driver = 4,
    /// A bounded operation exceeded its deadline.
    Timeout = 5,
    /// Durable credential storage failed.
    Storage = 6,
    /// A transaction was superseded or interrupted.
    Interrupted = 7,
}

impl NetworkFailure {
    const fn from_wire(value: u16) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Scan),
            2 => Some(Self::Association),
            3 => Some(Self::Dhcp),
            4 => Some(Self::Driver),
            5 => Some(Self::Timeout),
            6 => Some(Self::Storage),
            7 => Some(Self::Interrupted),
            _ => None,
        }
    }
}

/// One bounded, credential-free visible WLAN record.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct NetworkScanEntry {
    ssid_len: u8,
    ssid: [u8; MAX_SSID_BYTES],
    /// Advertised authentication family.
    pub authentication: NetworkAuthentication,
    /// Primary 2.4 GHz channel.
    pub channel: u8,
    /// Received signal strength in dBm.
    pub signal_dbm: i8,
    /// Exact basic-service-set MAC address.
    pub bssid: [u8; 6],
}

impl NetworkScanEntry {
    const EMPTY: Self = Self {
        ssid_len: 0,
        ssid: [0; MAX_SSID_BYTES],
        authentication: NetworkAuthentication::Unsupported,
        channel: 0,
        signal_dbm: i8::MIN,
        bssid: [0; 6],
    };

    /// Validate and construct one visible WLAN entry.
    pub fn try_new(
        ssid: &str,
        authentication: NetworkAuthentication,
        channel: u8,
        signal_dbm: i8,
        bssid: [u8; 6],
    ) -> Result<Self, NetworkWireError> {
        let ssid_len = ssid.len();
        let ssid = validate_ssid(ssid)?;
        if !(1..=14).contains(&channel) || bssid == [0; 6] {
            return Err(NetworkWireError::Entry);
        }
        Ok(Self {
            ssid_len: u8::try_from(ssid_len).map_err(|_| NetworkWireError::Ssid)?,
            ssid,
            authentication,
            channel,
            signal_dbm,
            bssid,
        })
    }

    /// Borrow the validated UTF-8 SSID.
    pub fn ssid(&self) -> &str {
        core::str::from_utf8(&self.ssid[..usize::from(self.ssid_len)])
            .expect("validated network SSID is UTF-8")
    }

    fn encode_into(self, encoded: &mut [u8]) {
        encoded.fill(0);
        encoded[0] = self.ssid_len;
        encoded[1] = self.authentication as u8;
        encoded[2] = self.channel;
        encoded[4] = self.signal_dbm.to_le_bytes()[0];
        encoded[8..14].copy_from_slice(&self.bssid);
        encoded[16..48].copy_from_slice(&self.ssid);
    }

    fn decode(encoded: &[u8]) -> Result<Self, NetworkWireError> {
        if encoded.len() != NETWORK_SCAN_ENTRY_WIRE_BYTES
            || encoded[3] != 0
            || encoded[5..8] != [0; 3]
            || encoded[14..16] != [0; 2]
        {
            return Err(NetworkWireError::Reserved);
        }
        let ssid_len = usize::from(encoded[0]);
        if ssid_len == 0
            || ssid_len > MAX_SSID_BYTES
            || encoded[16 + ssid_len..48] != [0; MAX_SSID_BYTES][ssid_len..]
        {
            return Err(NetworkWireError::Ssid);
        }
        let ssid = core::str::from_utf8(&encoded[16..16 + ssid_len])
            .map_err(|_| NetworkWireError::Ssid)?;
        let mut bssid = [0_u8; 6];
        bssid.copy_from_slice(&encoded[8..14]);
        Self::try_new(
            ssid,
            NetworkAuthentication::from_wire(encoded[1]).ok_or(NetworkWireError::Authentication)?,
            encoded[2],
            i8::from_le_bytes([encoded[4]]),
            bssid,
        )
    }
}

impl fmt::Debug for NetworkScanEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NetworkScanEntry")
            .field("ssid", &self.ssid())
            .field("authentication", &self.authentication)
            .field("channel", &self.channel)
            .field("signal_dbm", &self.signal_dbm)
            .field("bssid", &self.bssid)
            .finish()
    }
}

/// Idempotence identity for one bounded scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkScanRequest {
    /// Caller-selected monotonically increasing nonzero transaction identity.
    pub transaction_id: u64,
}

impl NetworkScanRequest {
    /// Encode one exact request.
    pub fn encode(self) -> Result<[u8; NETWORK_SCAN_REQUEST_WIRE_BYTES], NetworkWireError> {
        if self.transaction_id == 0 {
            return Err(NetworkWireError::Transaction);
        }
        let mut encoded = [0_u8; NETWORK_SCAN_REQUEST_WIRE_BYTES];
        encoded[..8].copy_from_slice(&NETWORK_SCAN_REQUEST_MAGIC);
        encoded[8..16].copy_from_slice(&self.transaction_id.to_le_bytes());
        Ok(encoded)
    }

    /// Decode one exact request.
    pub fn decode(encoded: &[u8]) -> Result<Self, NetworkWireError> {
        require_magic_len(
            encoded,
            NETWORK_SCAN_REQUEST_MAGIC,
            NETWORK_SCAN_REQUEST_WIRE_BYTES,
        )?;
        let request = Self {
            transaction_id: read_u64(encoded, 8),
        };
        request.encode()?;
        Ok(request)
    }
}

/// Complete bounded result of one visible-WLAN scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkScanResult {
    /// Exact transaction requested by the caller.
    pub transaction_id: u64,
    /// Network-state generation observed after the scan.
    pub network_generation: u32,
    /// Monotonic scan generation.
    pub scan_generation: u32,
    /// Device cycle at which results were retained.
    pub captured_at_cycle: u64,
    count: u8,
    truncated: bool,
    entries: [NetworkScanEntry; MAX_NETWORK_SCAN_RESULTS],
}

impl NetworkScanResult {
    /// Validate, canonicalize, and retain up to eight scan entries.
    pub fn try_new(
        transaction_id: u64,
        network_generation: u32,
        scan_generation: u32,
        captured_at_cycle: u64,
        entries: &[NetworkScanEntry],
        truncated: bool,
    ) -> Result<Self, NetworkWireError> {
        if transaction_id == 0 || scan_generation == 0 || entries.len() > MAX_NETWORK_SCAN_RESULTS {
            return Err(NetworkWireError::ScanCount);
        }
        let mut retained = [NetworkScanEntry::EMPTY; MAX_NETWORK_SCAN_RESULTS];
        retained[..entries.len()].copy_from_slice(entries);
        retained[..entries.len()].sort_unstable_by(compare_scan_entries);
        for left in 0..entries.len() {
            for right in left + 1..entries.len() {
                if retained[left].bssid == retained[right].bssid {
                    return Err(NetworkWireError::DuplicateBssid);
                }
            }
        }
        Ok(Self {
            transaction_id,
            network_generation,
            scan_generation,
            captured_at_cycle,
            count: u8::try_from(entries.len()).map_err(|_| NetworkWireError::ScanCount)?,
            truncated,
            entries: retained,
        })
    }

    /// Canonically ordered returned entries.
    pub fn entries(&self) -> &[NetworkScanEntry] {
        &self.entries[..usize::from(self.count)]
    }

    /// Whether more visible entries existed than this response could retain.
    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    /// Encode the complete fixed-size scan result.
    pub fn encode(self) -> [u8; NETWORK_SCAN_RESULT_WIRE_BYTES] {
        let mut encoded = [0_u8; NETWORK_SCAN_RESULT_WIRE_BYTES];
        encoded[..8].copy_from_slice(&NETWORK_SCAN_RESULT_MAGIC);
        encoded[8..16].copy_from_slice(&self.transaction_id.to_le_bytes());
        encoded[16..20].copy_from_slice(&self.network_generation.to_le_bytes());
        encoded[20..24].copy_from_slice(&self.scan_generation.to_le_bytes());
        encoded[24..32].copy_from_slice(&self.captured_at_cycle.to_le_bytes());
        encoded[32] = self.count;
        encoded[33] = u8::from(self.truncated) * SCAN_FLAG_TRUNCATED;
        for (index, entry) in self.entries().iter().copied().enumerate() {
            let start = NETWORK_SCAN_RESULT_HEADER_BYTES + index * NETWORK_SCAN_ENTRY_WIRE_BYTES;
            entry.encode_into(&mut encoded[start..start + NETWORK_SCAN_ENTRY_WIRE_BYTES]);
        }
        encoded
    }

    /// Decode and independently re-canonicalize a scan result.
    pub fn decode(encoded: &[u8]) -> Result<Self, NetworkWireError> {
        require_magic_len(
            encoded,
            NETWORK_SCAN_RESULT_MAGIC,
            NETWORK_SCAN_RESULT_WIRE_BYTES,
        )?;
        if encoded[33] & !SCAN_FLAGS_KNOWN != 0 || encoded[34..40] != [0; 6] {
            return Err(NetworkWireError::Reserved);
        }
        let count = usize::from(encoded[32]);
        if count > MAX_NETWORK_SCAN_RESULTS {
            return Err(NetworkWireError::ScanCount);
        }
        let mut entries = [NetworkScanEntry::EMPTY; MAX_NETWORK_SCAN_RESULTS];
        for (index, slot) in entries[..count].iter_mut().enumerate() {
            let start = NETWORK_SCAN_RESULT_HEADER_BYTES + index * NETWORK_SCAN_ENTRY_WIRE_BYTES;
            *slot =
                NetworkScanEntry::decode(&encoded[start..start + NETWORK_SCAN_ENTRY_WIRE_BYTES])?;
        }
        let used = NETWORK_SCAN_RESULT_HEADER_BYTES + count * NETWORK_SCAN_ENTRY_WIRE_BYTES;
        if encoded[used..].iter().any(|byte| *byte != 0) {
            return Err(NetworkWireError::Reserved);
        }
        let decoded = Self::try_new(
            read_u64(encoded, 8),
            read_u32(encoded, 16),
            read_u32(encoded, 20),
            read_u64(encoded, 24),
            &entries[..count],
            encoded[33] & SCAN_FLAG_TRUNCATED != 0,
        )?;
        if decoded.encode().as_slice() != encoded {
            return Err(NetworkWireError::NonCanonical);
        }
        Ok(decoded)
    }
}

/// Authenticated station association request with redacted diagnostics.
#[derive(Clone, Eq, PartialEq)]
pub struct NetworkJoinRequest {
    /// Caller-selected monotonically increasing nonzero transaction identity.
    pub transaction_id: u64,
    /// Exact network generation the user reviewed before mutation.
    pub expected_generation: u32,
    authentication: NetworkAuthentication,
    ssid_len: u8,
    ssid: [u8; MAX_SSID_BYTES],
    passphrase_len: u8,
    passphrase: [u8; MAX_PASSPHRASE_BYTES],
    bssid: Option<[u8; 6]>,
    channel: Option<u8>,
}

impl NetworkJoinRequest {
    /// Validate and construct one bounded association request.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        transaction_id: u64,
        expected_generation: u32,
        ssid: &str,
        authentication: NetworkAuthentication,
        passphrase: &str,
        bssid: Option<[u8; 6]>,
        channel: Option<u8>,
    ) -> Result<Self, NetworkWireError> {
        if transaction_id == 0 {
            return Err(NetworkWireError::Transaction);
        }
        if !authentication.joinable() {
            return Err(NetworkWireError::Authentication);
        }
        let ssid_len = ssid.len();
        let passphrase_len = passphrase.len();
        let ssid = validate_ssid(ssid)?;
        let passphrase = validate_passphrase(authentication, passphrase)?;
        if bssid == Some([0; 6]) || channel.is_some_and(|value| !(1..=14).contains(&value)) {
            return Err(NetworkWireError::Entry);
        }
        Ok(Self {
            transaction_id,
            expected_generation,
            authentication,
            ssid_len: u8::try_from(ssid_len).map_err(|_| NetworkWireError::Ssid)?,
            ssid,
            passphrase_len: u8::try_from(passphrase_len)
                .map_err(|_| NetworkWireError::Passphrase)?,
            passphrase,
            bssid,
            channel,
        })
    }

    /// Selected authentication family.
    pub const fn authentication(&self) -> NetworkAuthentication {
        self.authentication
    }

    /// Borrow the exact selected SSID.
    pub fn ssid(&self) -> &str {
        core::str::from_utf8(&self.ssid[..usize::from(self.ssid_len)])
            .expect("validated station SSID is UTF-8")
    }

    /// Borrow the passphrase for the sole radio/storage owner.
    pub fn passphrase(&self) -> &str {
        core::str::from_utf8(&self.passphrase[..usize::from(self.passphrase_len)])
            .expect("validated station passphrase is UTF-8")
    }

    /// Optional exact BSSID selected from a scan.
    pub const fn bssid(&self) -> Option<[u8; 6]> {
        self.bssid
    }

    /// Optional exact channel selected from a scan.
    pub const fn channel(&self) -> Option<u8> {
        self.channel
    }

    /// Encode one exact fixed-size request.
    pub fn encode(&self) -> [u8; NETWORK_JOIN_REQUEST_WIRE_BYTES] {
        let mut encoded = [0_u8; NETWORK_JOIN_REQUEST_WIRE_BYTES];
        encoded[..8].copy_from_slice(&NETWORK_JOIN_REQUEST_MAGIC);
        encoded[8..16].copy_from_slice(&self.transaction_id.to_le_bytes());
        encoded[16..20].copy_from_slice(&self.expected_generation.to_le_bytes());
        encoded[20] = self.authentication as u8;
        encoded[21] = (u8::from(self.bssid.is_some()) * JOIN_FLAG_BSSID)
            | (u8::from(self.channel.is_some()) * JOIN_FLAG_CHANNEL);
        encoded[22] = self.ssid_len;
        encoded[23] = self.passphrase_len;
        encoded[24] = self.channel.unwrap_or(0);
        encoded[25..31].copy_from_slice(&self.bssid.unwrap_or([0; 6]));
        encoded[32..64].copy_from_slice(&self.ssid);
        encoded[64..127].copy_from_slice(&self.passphrase);
        encoded
    }

    /// Decode and independently validate one request.
    pub fn decode(encoded: &[u8]) -> Result<Self, NetworkWireError> {
        require_magic_len(
            encoded,
            NETWORK_JOIN_REQUEST_MAGIC,
            NETWORK_JOIN_REQUEST_WIRE_BYTES,
        )?;
        if encoded[21] & !JOIN_FLAGS_KNOWN != 0 || encoded[31] != 0 || encoded[127] != 0 {
            return Err(NetworkWireError::Reserved);
        }
        let ssid_len = usize::from(encoded[22]);
        let passphrase_len = usize::from(encoded[23]);
        if ssid_len == 0
            || ssid_len > MAX_SSID_BYTES
            || passphrase_len > MAX_PASSPHRASE_BYTES
            || encoded[32 + ssid_len..64].iter().any(|byte| *byte != 0)
            || encoded[64 + passphrase_len..127]
                .iter()
                .any(|byte| *byte != 0)
        {
            return Err(NetworkWireError::NonCanonical);
        }
        let ssid = core::str::from_utf8(&encoded[32..32 + ssid_len])
            .map_err(|_| NetworkWireError::Ssid)?;
        let passphrase = core::str::from_utf8(&encoded[64..64 + passphrase_len])
            .map_err(|_| NetworkWireError::Passphrase)?;
        let mut bssid = [0_u8; 6];
        bssid.copy_from_slice(&encoded[25..31]);
        let bssid = if encoded[21] & JOIN_FLAG_BSSID != 0 {
            Some(bssid)
        } else {
            if bssid != [0; 6] {
                return Err(NetworkWireError::NonCanonical);
            }
            None
        };
        let channel = if encoded[21] & JOIN_FLAG_CHANNEL != 0 {
            Some(encoded[24])
        } else {
            if encoded[24] != 0 {
                return Err(NetworkWireError::NonCanonical);
            }
            None
        };
        let decoded = Self::try_new(
            read_u64(encoded, 8),
            read_u32(encoded, 16),
            ssid,
            NetworkAuthentication::from_wire(encoded[20])
                .ok_or(NetworkWireError::Authentication)?,
            passphrase,
            bssid,
            channel,
        )?;
        if decoded.encode().as_slice() != encoded {
            return Err(NetworkWireError::NonCanonical);
        }
        Ok(decoded)
    }
}

impl fmt::Debug for NetworkJoinRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NetworkJoinRequest")
            .field("transaction_id", &self.transaction_id)
            .field("expected_generation", &self.expected_generation)
            .field("authentication", &self.authentication)
            .field("ssid", &self.ssid())
            .field("passphrase", &"[redacted]")
            .field("bssid", &self.bssid)
            .field("channel", &self.channel)
            .finish()
    }
}

/// Idempotent leave or recovery request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkMutationRequest {
    /// Caller-selected monotonically increasing nonzero transaction identity.
    pub transaction_id: u64,
    /// Exact network generation the caller reconciled before mutation.
    pub expected_generation: u32,
}

impl NetworkMutationRequest {
    /// Encode one exact request.
    pub fn encode(self) -> Result<[u8; NETWORK_MUTATION_REQUEST_WIRE_BYTES], NetworkWireError> {
        if self.transaction_id == 0 {
            return Err(NetworkWireError::Transaction);
        }
        let mut encoded = [0_u8; NETWORK_MUTATION_REQUEST_WIRE_BYTES];
        encoded[..8].copy_from_slice(&NETWORK_MUTATION_REQUEST_MAGIC);
        encoded[8..16].copy_from_slice(&self.transaction_id.to_le_bytes());
        encoded[16..20].copy_from_slice(&self.expected_generation.to_le_bytes());
        Ok(encoded)
    }

    /// Decode one exact request.
    pub fn decode(encoded: &[u8]) -> Result<Self, NetworkWireError> {
        require_magic_len(
            encoded,
            NETWORK_MUTATION_REQUEST_MAGIC,
            NETWORK_MUTATION_REQUEST_WIRE_BYTES,
        )?;
        if encoded[20..24] != [0; 4] {
            return Err(NetworkWireError::Reserved);
        }
        let request = Self {
            transaction_id: read_u64(encoded, 8),
            expected_generation: read_u32(encoded, 16),
        };
        request.encode()?;
        Ok(request)
    }
}

/// Complete credential-free AP/STA state returned after every mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkStatus {
    /// Portable supervision phase.
    pub phase: NetworkPhase,
    /// Monotonic network-state generation.
    pub generation: u32,
    /// Latest complete scan generation, or zero before a scan.
    pub scan_generation: u32,
    /// Last mutation transaction applied or failed, or zero before mutation.
    pub last_transaction_id: u64,
    /// Association/address progress.
    pub station_link: StationLinkState,
    /// Selected or associated authentication family.
    pub authentication: NetworkAuthentication,
    /// Orthogonal non-secret state facts.
    pub flags: NetworkStatusFlags,
    /// Associated channel, or zero when absent.
    pub channel: u8,
    /// Last associated RSSI, or `i8::MIN` when absent.
    pub signal_dbm: i8,
    /// DHCP IPv4 prefix, or zero when absent.
    pub ipv4_prefix: u8,
    /// DHCP IPv4 address, or all zero when absent.
    pub ipv4_address: [u8; 4],
    /// DHCP gateway, or all zero when absent.
    pub ipv4_gateway: [u8; 4],
    /// Selected or associated BSSID, or all zero when absent.
    pub bssid: [u8; 6],
    /// Stable most-recent failure class.
    pub last_failure: NetworkFailure,
    ssid_len: u8,
    ssid: [u8; MAX_SSID_BYTES],
}

impl NetworkStatus {
    /// Construct and validate one credential-free status.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        phase: NetworkPhase,
        generation: u32,
        scan_generation: u32,
        last_transaction_id: u64,
        station_link: StationLinkState,
        authentication: NetworkAuthentication,
        flags: NetworkStatusFlags,
        channel: u8,
        signal_dbm: i8,
        ipv4_prefix: u8,
        ipv4_address: [u8; 4],
        ipv4_gateway: [u8; 4],
        bssid: [u8; 6],
        last_failure: NetworkFailure,
        ssid: Option<&str>,
    ) -> Result<Self, NetworkWireError> {
        let (ssid_len, ssid) = match ssid {
            Some(value) => {
                let value_len = value.len();
                let value = validate_ssid(value)?;
                (
                    u8::try_from(value_len).map_err(|_| NetworkWireError::Ssid)?,
                    value,
                )
            }
            None => (0, [0; MAX_SSID_BYTES]),
        };
        let addressed = station_link == StationLinkState::Addressed;
        if channel > 14
            || ipv4_prefix > 32
            || (addressed != (ipv4_address != [0; 4]))
            || (addressed != flags.contains(NetworkStatusFlags::STATION_IPV4_READY))
            || (flags.contains(NetworkStatusFlags::STATION_ASSOCIATED)
                != matches!(
                    station_link,
                    StationLinkState::Associated | StationLinkState::Addressed
                ))
            || (ssid_len == 0) == flags.contains(NetworkStatusFlags::STATION_CONFIGURED)
            || (last_failure == NetworkFailure::None)
                == flags.contains(NetworkStatusFlags::LAST_OPERATION_FAILED)
            || (phase.access_point_expected() != flags.contains(NetworkStatusFlags::AP_EXPECTED))
        {
            return Err(NetworkWireError::Status);
        }
        if !addressed && (ipv4_prefix != 0 || ipv4_gateway != [0; 4]) {
            return Err(NetworkWireError::Status);
        }
        Ok(Self {
            phase,
            generation,
            scan_generation,
            last_transaction_id,
            station_link,
            authentication,
            flags,
            channel,
            signal_dbm,
            ipv4_prefix,
            ipv4_address,
            ipv4_gateway,
            bssid,
            last_failure,
            ssid_len,
            ssid,
        })
    }

    /// Borrow the selected SSID, if any.
    pub fn ssid(&self) -> Option<&str> {
        (self.ssid_len != 0).then(|| {
            core::str::from_utf8(&self.ssid[..usize::from(self.ssid_len)])
                .expect("validated status SSID is UTF-8")
        })
    }

    /// Initial AP-only provisioning status.
    pub fn provisioning(supervisor: super::NetworkSupervisor) -> Self {
        Self::try_new(
            supervisor.phase(),
            supervisor.generation(),
            0,
            0,
            StationLinkState::Disconnected,
            NetworkAuthentication::Unsupported,
            NetworkStatusFlags(NetworkStatusFlags::AP_EXPECTED),
            0,
            i8::MIN,
            0,
            [0; 4],
            [0; 4],
            [0; 6],
            NetworkFailure::None,
            None,
        )
        .expect("provisioning status is internally valid")
    }

    /// Encode one exact status.
    pub fn encode(self) -> [u8; NETWORK_STATUS_WIRE_BYTES] {
        let mut encoded = [0_u8; NETWORK_STATUS_WIRE_BYTES];
        encoded[..8].copy_from_slice(&NETWORK_STATUS_MAGIC);
        encoded[8..12].copy_from_slice(&self.generation.to_le_bytes());
        encoded[12..16].copy_from_slice(&self.scan_generation.to_le_bytes());
        encoded[16..24].copy_from_slice(&self.last_transaction_id.to_le_bytes());
        encoded[24] = phase_to_wire(self.phase);
        encoded[25] = self.station_link as u8;
        encoded[26] = self.authentication as u8;
        encoded[27] = self.flags.0;
        encoded[28] = self.channel;
        encoded[29] = self.signal_dbm.to_le_bytes()[0];
        encoded[30] = self.ipv4_prefix;
        encoded[31] = self.ssid_len;
        encoded[32..36].copy_from_slice(&self.ipv4_address);
        encoded[36..40].copy_from_slice(&self.ipv4_gateway);
        encoded[40..46].copy_from_slice(&self.bssid);
        encoded[46..48].copy_from_slice(&(self.last_failure as u16).to_le_bytes());
        encoded[48..80].copy_from_slice(&self.ssid);
        encoded
    }

    /// Decode and independently validate one exact status.
    pub fn decode(encoded: &[u8]) -> Result<Self, NetworkWireError> {
        require_magic_len(encoded, NETWORK_STATUS_MAGIC, NETWORK_STATUS_WIRE_BYTES)?;
        let ssid_len = usize::from(encoded[31]);
        if ssid_len > MAX_SSID_BYTES || encoded[48 + ssid_len..80].iter().any(|byte| *byte != 0) {
            return Err(NetworkWireError::NonCanonical);
        }
        let ssid = if ssid_len == 0 {
            None
        } else {
            Some(
                core::str::from_utf8(&encoded[48..48 + ssid_len])
                    .map_err(|_| NetworkWireError::Ssid)?,
            )
        };
        let mut address = [0_u8; 4];
        address.copy_from_slice(&encoded[32..36]);
        let mut gateway = [0_u8; 4];
        gateway.copy_from_slice(&encoded[36..40]);
        let mut bssid = [0_u8; 6];
        bssid.copy_from_slice(&encoded[40..46]);
        let decoded = Self::try_new(
            phase_from_wire(encoded[24]).ok_or(NetworkWireError::Phase)?,
            read_u32(encoded, 8),
            read_u32(encoded, 12),
            read_u64(encoded, 16),
            StationLinkState::from_wire(encoded[25]).ok_or(NetworkWireError::Link)?,
            NetworkAuthentication::from_wire(encoded[26])
                .ok_or(NetworkWireError::Authentication)?,
            NetworkStatusFlags(encoded[27]),
            encoded[28],
            i8::from_le_bytes([encoded[29]]),
            encoded[30],
            address,
            gateway,
            bssid,
            NetworkFailure::from_wire(read_u16(encoded, 46)).ok_or(NetworkWireError::Failure)?,
            ssid,
        )?;
        if decoded.encode().as_slice() != encoded {
            return Err(NetworkWireError::NonCanonical);
        }
        Ok(decoded)
    }
}

/// Canonical provisioning body rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkWireError {
    /// Exact body size differed from the selected schema.
    Length,
    /// Body magic identified another schema.
    Magic,
    /// Reserved bytes or flag bits were nonzero.
    Reserved,
    /// SSID was empty, malformed, or over length.
    Ssid,
    /// Passphrase contradicted the authentication policy.
    Passphrase,
    /// Authentication code was unknown or unjoinable.
    Authentication,
    /// Scan entry channel or BSSID was invalid.
    Entry,
    /// Scan entry count or generation was invalid.
    ScanCount,
    /// Two scan entries named one BSSID.
    DuplicateBssid,
    /// Transaction identity was zero.
    Transaction,
    /// Supervision phase was unknown.
    Phase,
    /// Station link state was unknown.
    Link,
    /// Failure code was unknown.
    Failure,
    /// Status fields contradicted one another.
    Status,
    /// Unused bytes or ordering made an otherwise decodable body noncanonical.
    NonCanonical,
}

fn validate_ssid(value: &str) -> Result<[u8; MAX_SSID_BYTES], NetworkWireError> {
    if value.is_empty() || value.len() > MAX_SSID_BYTES || value.as_bytes().contains(&0) {
        return Err(NetworkWireError::Ssid);
    }
    let mut retained = [0_u8; MAX_SSID_BYTES];
    retained[..value.len()].copy_from_slice(value.as_bytes());
    Ok(retained)
}

fn validate_passphrase(
    authentication: NetworkAuthentication,
    value: &str,
) -> Result<[u8; MAX_PASSPHRASE_BYTES], NetworkWireError> {
    let valid_length = match authentication {
        NetworkAuthentication::Open => value.is_empty(),
        NetworkAuthentication::Wpa2Personal
        | NetworkAuthentication::Wpa3Personal
        | NetworkAuthentication::Wpa2Wpa3Personal => {
            (MIN_PASSPHRASE_BYTES..=MAX_PASSPHRASE_BYTES).contains(&value.len())
        }
        NetworkAuthentication::Unsupported => false,
    };
    if !valid_length || value.as_bytes().contains(&0) {
        return Err(NetworkWireError::Passphrase);
    }
    let mut retained = [0_u8; MAX_PASSPHRASE_BYTES];
    retained[..value.len()].copy_from_slice(value.as_bytes());
    Ok(retained)
}

fn compare_scan_entries(left: &NetworkScanEntry, right: &NetworkScanEntry) -> Ordering {
    right
        .signal_dbm
        .cmp(&left.signal_dbm)
        .then_with(|| left.ssid().as_bytes().cmp(right.ssid().as_bytes()))
        .then_with(|| left.bssid.cmp(&right.bssid))
}

fn phase_to_wire(phase: NetworkPhase) -> u8 {
    match phase {
        NetworkPhase::Reset => 0,
        NetworkPhase::AccessPointStarting => 1,
        NetworkPhase::Provisioning => 2,
        NetworkPhase::StationJoining => 3,
        NetworkPhase::AccessPointAndStation => 4,
        NetworkPhase::Recovering => 5,
    }
}

fn phase_from_wire(value: u8) -> Option<NetworkPhase> {
    match value {
        0 => Some(NetworkPhase::Reset),
        1 => Some(NetworkPhase::AccessPointStarting),
        2 => Some(NetworkPhase::Provisioning),
        3 => Some(NetworkPhase::StationJoining),
        4 => Some(NetworkPhase::AccessPointAndStation),
        5 => Some(NetworkPhase::Recovering),
        _ => None,
    }
}

fn require_magic_len(
    encoded: &[u8],
    magic: [u8; 8],
    expected: usize,
) -> Result<(), NetworkWireError> {
    if encoded.len() != expected {
        return Err(NetworkWireError::Length);
    }
    if encoded[..8] != magic {
        return Err(NetworkWireError::Magic);
    }
    Ok(())
}

fn read_u16(encoded: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([encoded[offset], encoded[offset + 1]])
}

fn read_u32(encoded: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        encoded[offset],
        encoded[offset + 1],
        encoded[offset + 2],
        encoded[offset + 3],
    ])
}

fn read_u64(encoded: &[u8], offset: usize) -> u64 {
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

trait NetworkPhaseFacts {
    fn access_point_expected(self) -> bool;
}

impl NetworkPhaseFacts for NetworkPhase {
    fn access_point_expected(self) -> bool {
        matches!(
            self,
            NetworkPhase::AccessPointStarting
                | NetworkPhase::Provisioning
                | NetworkPhase::StationJoining
                | NetworkPhase::AccessPointAndStation
        )
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::NetworkSupervisor;
    use std::format;

    fn entry(ssid: &str, signal_dbm: i8, bssid_tail: u8) -> NetworkScanEntry {
        NetworkScanEntry::try_new(
            ssid,
            NetworkAuthentication::Wpa2Personal,
            6,
            signal_dbm,
            [2, 3, 4, 5, 6, bssid_tail],
        )
        .unwrap()
    }

    #[test]
    fn join_round_trip_is_canonical_and_debug_redacts_secret() {
        let request = NetworkJoinRequest::try_new(
            7,
            2,
            "machine-lan",
            NetworkAuthentication::Wpa2Personal,
            "never-print-this",
            Some([2, 3, 4, 5, 6, 7]),
            Some(11),
        )
        .unwrap();
        let encoded = request.encode();
        assert_eq!(NetworkJoinRequest::decode(&encoded).unwrap(), request);
        let debug = format!("{request:?}");
        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("never-print-this"));

        let mut noncanonical = encoded;
        noncanonical[127] = 1;
        assert_eq!(
            NetworkJoinRequest::decode(&noncanonical),
            Err(NetworkWireError::Reserved)
        );
    }

    #[test]
    fn authentication_controls_passphrase_policy() {
        assert!(
            NetworkJoinRequest::try_new(1, 2, "open", NetworkAuthentication::Open, "", None, None,)
                .is_ok()
        );
        assert_eq!(
            NetworkJoinRequest::try_new(
                1,
                2,
                "secure",
                NetworkAuthentication::Wpa2Personal,
                "short",
                None,
                None,
            ),
            Err(NetworkWireError::Passphrase)
        );
        assert_eq!(
            NetworkJoinRequest::try_new(
                1,
                2,
                "legacy",
                NetworkAuthentication::Unsupported,
                "long-enough",
                None,
                None,
            ),
            Err(NetworkWireError::Authentication)
        );
    }

    #[test]
    fn scan_round_trip_sorts_strongest_then_identity() {
        let result = NetworkScanResult::try_new(
            9,
            2,
            1,
            50,
            &[entry("weak", -80, 2), entry("strong", -30, 1)],
            true,
        )
        .unwrap();
        assert_eq!(result.entries()[0].ssid(), "strong");
        assert!(result.truncated());
        let encoded = result.encode();
        assert_eq!(NetworkScanResult::decode(&encoded).unwrap(), result);

        let duplicate = entry("other", -20, 1);
        assert_eq!(
            NetworkScanResult::try_new(9, 2, 1, 50, &[entry("strong", -30, 1), duplicate], false,),
            Err(NetworkWireError::DuplicateBssid)
        );
    }

    #[test]
    fn status_round_trip_separates_association_and_address() {
        let mut supervisor = NetworkSupervisor::new();
        supervisor.begin_access_point().unwrap();
        supervisor.access_point_ready().unwrap();
        let initial = NetworkStatus::provisioning(supervisor);
        assert_eq!(NetworkStatus::decode(&initial.encode()).unwrap(), initial);
        assert_eq!(initial.ipv4_address, [0; 4]);
        assert_eq!(crate::PROVISIONING_ADDRESS, [192, 168, 4, 1]);

        supervisor.begin_station_join().unwrap();
        supervisor.station_joined().unwrap();
        let addressed = NetworkStatus::try_new(
            supervisor.phase(),
            supervisor.generation(),
            3,
            17,
            StationLinkState::Addressed,
            NetworkAuthentication::Wpa2Personal,
            NetworkStatusFlags(
                NetworkStatusFlags::AP_EXPECTED
                    | NetworkStatusFlags::STATION_CONFIGURED
                    | NetworkStatusFlags::STATION_ASSOCIATED
                    | NetworkStatusFlags::STATION_IPV4_READY,
            ),
            6,
            -42,
            24,
            [192, 168, 1, 44],
            [192, 168, 1, 1],
            [2, 3, 4, 5, 6, 7],
            NetworkFailure::None,
            Some("machine-lan"),
        )
        .unwrap();
        assert_eq!(
            NetworkStatus::decode(&addressed.encode()).unwrap(),
            addressed
        );

        let inconsistent = NetworkStatus::try_new(
            supervisor.phase(),
            supervisor.generation(),
            3,
            17,
            StationLinkState::Associated,
            NetworkAuthentication::Wpa2Personal,
            NetworkStatusFlags(
                NetworkStatusFlags::AP_EXPECTED | NetworkStatusFlags::STATION_CONFIGURED,
            ),
            6,
            -42,
            0,
            [0; 4],
            [0; 4],
            [2, 3, 4, 5, 6, 7],
            NetworkFailure::None,
            Some("machine-lan"),
        );
        assert_eq!(inconsistent, Err(NetworkWireError::Status));
    }

    #[test]
    fn mutation_and_scan_requests_reject_zero_and_reserved_bytes() {
        let mutation = NetworkMutationRequest {
            transaction_id: 3,
            expected_generation: 2,
        };
        let encoded = mutation.encode().unwrap();
        assert_eq!(NetworkMutationRequest::decode(&encoded).unwrap(), mutation);
        let mut bad = encoded;
        bad[23] = 1;
        assert_eq!(
            NetworkMutationRequest::decode(&bad),
            Err(NetworkWireError::Reserved)
        );
        assert_eq!(
            NetworkScanRequest { transaction_id: 0 }.encode(),
            Err(NetworkWireError::Transaction)
        );
    }
}
