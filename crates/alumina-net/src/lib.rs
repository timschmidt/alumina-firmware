#![no_std]
#![doc = "Bounded, portable network policy shared by firmware and simulation."]

/// Maximum IEEE 802.11 SSID size in bytes.
pub const MAX_SSID_BYTES: usize = 32;
/// Minimum WPA2/WPA3 passphrase size in bytes.
pub const MIN_PASSPHRASE_BYTES: usize = 8;
/// Maximum textual WPA2/WPA3 passphrase size in bytes.
pub const MAX_PASSPHRASE_BYTES: usize = 63;
/// Default Alumina provisioning subnet gateway.
pub const PROVISIONING_ADDRESS: [u8; 4] = [192, 168, 4, 1];
/// Default prefix length for the provisioning subnet.
pub const PROVISIONING_PREFIX: u8 = 24;
/// First address offered by the bounded DHCP lease table.
pub const DHCP_RANGE_START: [u8; 4] = [192, 168, 4, 100];
/// Last address offered by the bounded DHCP lease table.
pub const DHCP_RANGE_END: [u8; 4] = [192, 168, 4, 103];

/// Provenance of the password protecting the device access point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialSource {
    /// Repository-wide development credential; never production-armable.
    DevelopmentFallback,
    /// Secret supplied to the image build outside source control.
    BuildProvisioned,
    /// Unique credential loaded from transactional device storage.
    DeviceStored,
}

impl CredentialSource {
    /// Stable human-facing label used by discovery responses.
    pub const fn label(self) -> &'static str {
        match self {
            Self::DevelopmentFallback => "development-fallback",
            Self::BuildProvisioned => "build-provisioned",
            Self::DeviceStored => "device-stored",
        }
    }

    /// Only a unique stored credential is accepted for production arming.
    pub const fn production_armable(self) -> bool {
        matches!(self, Self::DeviceStored)
    }
}

/// Complete bounded policy used to construct a protected provisioning AP.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccessPointProfile<'a> {
    /// Network name advertised by the device.
    pub ssid: &'a str,
    /// WPA2/WPA3 passphrase. It is never returned by discovery APIs.
    pub passphrase: &'a str,
    /// Credential provenance reported without disclosing the credential.
    pub credential_source: CredentialSource,
    /// Regulatory channel selected by board/application policy.
    pub channel: u8,
    /// Maximum associated clients admitted by both Wi-Fi and DHCP policy.
    pub maximum_clients: u8,
}

impl<'a> AccessPointProfile<'a> {
    /// Creates the initial four-client, channel-six provisioning profile.
    pub const fn new(
        ssid: &'a str,
        passphrase: &'a str,
        credential_source: CredentialSource,
    ) -> Self {
        Self {
            ssid,
            passphrase,
            credential_source,
            channel: 6,
            maximum_clients: 4,
        }
    }

    /// Rejects configurations the radio would truncate or expose insecurely.
    pub fn validate(self) -> Result<(), ProfileError> {
        let ssid_len = self.ssid.len();
        if ssid_len == 0 || ssid_len > MAX_SSID_BYTES {
            return Err(ProfileError::SsidLength(ssid_len));
        }
        if self.ssid.as_bytes().contains(&0) {
            return Err(ProfileError::EmbeddedNul);
        }

        let passphrase_len = self.passphrase.len();
        if !(MIN_PASSPHRASE_BYTES..=MAX_PASSPHRASE_BYTES).contains(&passphrase_len) {
            return Err(ProfileError::PassphraseLength(passphrase_len));
        }
        if self.passphrase.as_bytes().contains(&0) {
            return Err(ProfileError::EmbeddedNul);
        }
        if !(1..=13).contains(&self.channel) {
            return Err(ProfileError::Channel(self.channel));
        }
        if !(1..=4).contains(&self.maximum_clients) {
            return Err(ProfileError::MaximumClients(self.maximum_clients));
        }
        Ok(())
    }
}

/// Invalid provisioning policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileError {
    /// SSID is empty or exceeds the radio limit.
    SsidLength(usize),
    /// Passphrase is outside the textual WPA limit.
    PassphraseLength(usize),
    /// A string contains a NUL that would be ambiguous at the vendor boundary.
    EmbeddedNul,
    /// Channel is outside the initial 2.4 GHz policy.
    Channel(u8),
    /// Client count is zero or exceeds the fixed DHCP table.
    MaximumClients(u8),
}

/// Fixed HTTP and socket resource policy for the initial service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebLimits {
    /// Simultaneous TCP handlers.
    pub connections: usize,
    /// Header workspace per connection.
    pub header_bytes: usize,
    /// Parsed headers per request.
    pub header_count: usize,
    /// RX and TX bytes reserved per connection and direction.
    pub socket_bytes: usize,
    /// Timeout on each socket operation.
    pub io_timeout_ms: u32,
    /// Timeout for an idle persistent connection.
    pub keepalive_timeout_ms: u32,
    /// Timeout for one complete handler invocation.
    pub request_timeout_ms: u32,
}

impl WebLimits {
    /// Reviewed initial limits used by the ESP adapter.
    pub const INITIAL: Self = Self {
        connections: 2,
        header_bytes: 1_024,
        header_count: 12,
        socket_bytes: 2_048,
        io_timeout_ms: 2_000,
        keepalive_timeout_ms: 5_000,
        request_timeout_ms: 3_000,
    };

    /// Rejects zero/unbounded-looking policy and inconsistent timeouts.
    pub const fn validate(self) -> Result<(), LimitError> {
        if self.connections == 0 || self.connections > 4 {
            return Err(LimitError::Connections);
        }
        if self.header_bytes < 256 || self.header_bytes > 4_096 {
            return Err(LimitError::HeaderBytes);
        }
        if self.header_count == 0 || self.header_count > 32 {
            return Err(LimitError::HeaderCount);
        }
        if self.socket_bytes < 512 || self.socket_bytes > 8_192 {
            return Err(LimitError::SocketBytes);
        }
        if self.io_timeout_ms == 0 || self.keepalive_timeout_ms == 0 || self.request_timeout_ms == 0
        {
            return Err(LimitError::Timeout);
        }
        Ok(())
    }
}

/// Invalid fixed web-service resource policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitError {
    /// Invalid concurrent connection count.
    Connections,
    /// Invalid per-connection header buffer.
    HeaderBytes,
    /// Invalid parsed-header count.
    HeaderCount,
    /// Invalid per-direction socket buffer.
    SocketBytes,
    /// A timeout is zero.
    Timeout,
}

/// HTTP methods needed by the portable route classifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    /// Safe read.
    Get,
    /// Bounded mutation request.
    Post,
    /// Idempotent replacement request.
    Put,
    /// Any method not admitted by protocol V1.
    Other,
}

/// Routes present in the network foundation image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    /// Minimal embedded bootstrap page.
    Bootstrap,
    /// Protocol and board identity.
    Identity,
    /// Service liveness without mutable machine state.
    Health,
    /// Network status/provisioning family, not yet enabled for mutation.
    Network,
    /// Known resource addressed with a forbidden method.
    MethodNotAllowed,
    /// Unknown path.
    NotFound,
}

/// Classifies exact greenfield paths without legacy aliases or prefix matching.
pub fn classify_route(method: HttpMethod, path: &str) -> Route {
    let known = match path {
        "/" => Some(Route::Bootstrap),
        "/api/v1/identity" => Some(Route::Identity),
        "/api/v1/health" => Some(Route::Health),
        "/api/v1/network" => Some(Route::Network),
        _ => None,
    };

    match (method, known) {
        (HttpMethod::Get, Some(route)) => route,
        (_, Some(_)) => Route::MethodNotAllowed,
        (_, None) => Route::NotFound,
    }
}

/// Recoverable AP/STA supervision phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkPhase {
    /// Radio has not been initialized.
    Reset,
    /// Protected AP configuration has been accepted and is starting.
    AccessPointStarting,
    /// AP is usable; no infrastructure association is active.
    Provisioning,
    /// AP remains active while station association is attempted.
    StationJoining,
    /// AP and infrastructure station are both active.
    AccessPointAndStation,
    /// AP is being recreated after an explicit recovery request or fault.
    Recovering,
}

/// Small deterministic state machine used by firmware and network simulation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkSupervisor {
    phase: NetworkPhase,
    generation: u32,
}

impl NetworkSupervisor {
    /// Starts with no radio state assumed.
    pub const fn new() -> Self {
        Self {
            phase: NetworkPhase::Reset,
            generation: 0,
        }
    }

    /// Current phase.
    pub const fn phase(self) -> NetworkPhase {
        self.phase
    }

    /// Monotonic wrapping transition generation for status polling.
    pub const fn generation(self) -> u32 {
        self.generation
    }

    /// True while the state machine requires the recovery AP to remain usable.
    pub const fn access_point_expected(self) -> bool {
        matches!(
            self.phase,
            NetworkPhase::AccessPointStarting
                | NetworkPhase::Provisioning
                | NetworkPhase::StationJoining
                | NetworkPhase::AccessPointAndStation
        )
    }

    /// Begins initial AP bring-up or a recovery restart.
    pub fn begin_access_point(&mut self) -> Result<(), TransitionError> {
        match self.phase {
            NetworkPhase::Reset | NetworkPhase::Recovering => {
                self.set_phase(NetworkPhase::AccessPointStarting);
                Ok(())
            }
            phase => Err(TransitionError::InvalidPhase(phase)),
        }
    }

    /// Records that the protected AP and its static network stack are live.
    pub fn access_point_ready(&mut self) -> Result<(), TransitionError> {
        self.require(NetworkPhase::AccessPointStarting)?;
        self.set_phase(NetworkPhase::Provisioning);
        Ok(())
    }

    /// Begins a station join without removing the recovery AP.
    pub fn begin_station_join(&mut self) -> Result<(), TransitionError> {
        match self.phase {
            NetworkPhase::Provisioning | NetworkPhase::AccessPointAndStation => {
                self.set_phase(NetworkPhase::StationJoining);
                Ok(())
            }
            phase => Err(TransitionError::InvalidPhase(phase)),
        }
    }

    /// Commits successful infrastructure association.
    pub fn station_joined(&mut self) -> Result<(), TransitionError> {
        self.require(NetworkPhase::StationJoining)?;
        self.set_phase(NetworkPhase::AccessPointAndStation);
        Ok(())
    }

    /// Returns to AP provisioning after wrong or unreachable credentials.
    pub fn station_join_failed(&mut self) -> Result<(), TransitionError> {
        self.require(NetworkPhase::StationJoining)?;
        self.set_phase(NetworkPhase::Provisioning);
        Ok(())
    }

    /// Leaves infrastructure Wi-Fi while preserving the device AP.
    pub fn leave_station(&mut self) -> Result<(), TransitionError> {
        match self.phase {
            NetworkPhase::StationJoining | NetworkPhase::AccessPointAndStation => {
                self.set_phase(NetworkPhase::Provisioning);
                Ok(())
            }
            phase => Err(TransitionError::InvalidPhase(phase)),
        }
    }

    /// Invalidates radio state so recovery must recreate the protected AP.
    pub fn begin_recovery(&mut self) {
        self.set_phase(NetworkPhase::Recovering);
    }

    fn require(&self, required: NetworkPhase) -> Result<(), TransitionError> {
        if self.phase == required {
            Ok(())
        } else {
            Err(TransitionError::InvalidPhase(self.phase))
        }
    }

    fn set_phase(&mut self, phase: NetworkPhase) {
        self.phase = phase;
        self.generation = self.generation.wrapping_add(1);
    }
}

impl Default for NetworkSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

/// A supervision event was applied in an impossible phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionError {
    /// Actual phase did not admit the requested transition.
    InvalidPhase(NetworkPhase),
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVELOPMENT: AccessPointProfile<'static> = AccessPointProfile::new(
        "Alumina-mks-tinybee-v1",
        "alumina-development",
        CredentialSource::DevelopmentFallback,
    );

    #[test]
    fn initial_profile_and_web_limits_are_bounded() {
        assert_eq!(DEVELOPMENT.validate(), Ok(()));
        assert_eq!(WebLimits::INITIAL.validate(), Ok(()));
        assert!(!DEVELOPMENT.credential_source.production_armable());
    }

    #[test]
    fn invalid_radio_strings_and_resources_are_rejected() {
        let mut invalid = DEVELOPMENT;
        invalid.passphrase = "short";
        assert_eq!(invalid.validate(), Err(ProfileError::PassphraseLength(5)));

        invalid = DEVELOPMENT;
        invalid.ssid = "bad\0ssid";
        assert_eq!(invalid.validate(), Err(ProfileError::EmbeddedNul));

        invalid = DEVELOPMENT;
        invalid.maximum_clients = 5;
        assert_eq!(invalid.validate(), Err(ProfileError::MaximumClients(5)));
    }

    #[test]
    fn exact_routes_have_no_legacy_or_prefix_aliases() {
        assert_eq!(
            classify_route(HttpMethod::Get, "/api/v1/identity"),
            Route::Identity
        );
        assert_eq!(
            classify_route(HttpMethod::Post, "/api/v1/identity"),
            Route::MethodNotAllowed
        );
        assert_eq!(classify_route(HttpMethod::Get, "/device"), Route::NotFound);
        assert_eq!(
            classify_route(HttpMethod::Get, "/api/v1/identity/extra"),
            Route::NotFound
        );
    }

    #[test]
    fn failed_join_never_removes_recovery_ap() {
        let mut state = NetworkSupervisor::new();
        state.begin_access_point().unwrap();
        state.access_point_ready().unwrap();
        state.begin_station_join().unwrap();
        assert!(state.access_point_expected());
        state.station_join_failed().unwrap();
        assert_eq!(state.phase(), NetworkPhase::Provisioning);
        assert!(state.access_point_expected());
        assert_eq!(state.generation(), 4);
    }

    #[test]
    fn successful_join_can_leave_back_to_provisioning() {
        let mut state = NetworkSupervisor::new();
        state.begin_access_point().unwrap();
        state.access_point_ready().unwrap();
        state.begin_station_join().unwrap();
        state.station_joined().unwrap();
        assert_eq!(state.phase(), NetworkPhase::AccessPointAndStation);
        state.leave_station().unwrap();
        assert_eq!(state.phase(), NetworkPhase::Provisioning);
    }

    #[test]
    fn recovery_requires_access_point_restart() {
        let mut state = NetworkSupervisor::new();
        state.begin_access_point().unwrap();
        state.access_point_ready().unwrap();
        state.begin_recovery();
        assert_eq!(state.phase(), NetworkPhase::Recovering);
        assert!(!state.access_point_expected());
        state.begin_access_point().unwrap();
        assert_eq!(state.phase(), NetworkPhase::AccessPointStarting);
    }
}
