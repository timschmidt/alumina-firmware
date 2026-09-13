//! Core-0-only ESP radio, static IPv4, DHCP, and bounded HTTP adapter.

extern crate alloc;

#[cfg(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-mks-tinybee",
    feature = "board-mks-tinybee-4mb"
))]
use alloc::boxed::Box;
use alloc::string::String;
use core::cell::RefCell;
use core::fmt::Write as _;
use core::fmt::{Debug, Display};
use core::net::{IpAddr, Ipv4Addr, SocketAddr};

use alumina_capability::{CapabilityIdentity, verify_declared_identity};
use alumina_net::captive_dns::{
    CAPTIVE_DNS_PACKET_BYTES, CAPTIVE_DNS_PORT, build_captive_dns_reply,
};
use alumina_net::provisioning::{
    MAX_NETWORK_SCAN_RESULTS, NetworkAuthentication, NetworkFailure, NetworkJoinRequest,
    NetworkMutationRequest, NetworkScanEntry, NetworkScanRequest, NetworkScanResult, NetworkStatus,
    NetworkStatusFlags, StationLinkState,
};
use alumina_net::{
    AUTH_COUNTER_HEADER, AUTH_DISCOVERY_BODY_BYTES, AUTH_DISCOVERY_CONTENT_LENGTH,
    AUTH_NONCE_BYTES, AUTH_RESPONSE_HEADER, AUTH_TAG_HEX_BYTES, AccessPointProfile, AuthError,
    AuthHeaderAccumulator, AuthRateLimit, AuthenticatedMedia, AuthenticatedRequestMetadata,
    AuthenticationState, BootNonce, CORS_ALLOW_ORIGIN_HEADER, CORS_ALLOW_PRIVATE_NETWORK_HEADER,
    CORS_ORIGIN_HEADER, CorsOrigin, CorsPreflight, CorsPreflightAccumulator, CredentialSource,
    DHCP_RANGE_END, DHCP_RANGE_START, HttpAdmissionError, HttpMethod, MAX_AUTHENTICATED_BODY_BYTES,
    NetworkSupervisor, PROVISIONING_ADDRESS, PROVISIONING_PREFIX, Route, WebLimits, classify_route,
    sign_response, write_lower_hex,
};
use alumina_protocol::{DeviceCycle, DeviceId, Digest, FrameKind, Operation, StatusCode};
use alumina_service::{NativeRequest, ResponseMedia, ServiceRequest, ServiceResponse};
use alumina_storage::sha256;
use alumina_web_assets::{
    EmbeddedWebAsset, INTERFACE_COMMIT, INTERFACE_CONTENT_SECURITY_POLICY, WEB_BUNDLE_DIGEST,
    WEB_BUNDLE_FORMAT, web_asset,
};
use defmt::{error, info, warn};
use edge_dhcp::io::{DEFAULT_SERVER_PORT, server as dhcp_io};
use edge_dhcp::server::{Server as DhcpServer, ServerOptions};
use edge_http::io::Error as HttpError;
use edge_http::io::server::{Connection, Handler, handle_connection};
use edge_http::{Method, RequestHeaders};
use edge_nal::{
    TcpAccept, TcpBind, TcpSplit, UdpBind, UdpReceive, UdpSend, WithTimeout, WithTimeoutError,
    with_timeout,
};
use edge_nal_embassy::{Tcp, TcpBuffers, Udp, UdpBuffers};
use embassy_executor::Spawner;
use embassy_net::{
    Config as NetworkConfig, ConfigV4, Ipv4Address, Ipv4Cidr, Runner, Stack, StackResources,
    StaticConfigV4,
};
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::{Duration, Instant, Timer};
use embedded_io_async_v06::{Read, Write};
use esp_hal::peripherals::WIFI;
use esp_hal::rng::Rng;
use esp_hal::system::Cpu;
use esp_radio::wifi::{
    AuthenticationMethod, Config as WifiConfig, ControllerConfig, Interface, WifiController,
    ap::AccessPointConfig, scan::ScanConfig, sta::StationConfig,
};
use heapless::{String as FixedString, Vec as FixedVec};
#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
use static_cell::ConstStaticCell;
use static_cell::StaticCell;

use crate::hardware::selected;
use crate::poll_boundary::poll_boundary;
use crate::service::ServiceBridge;

const AP_SSID: &str = concat!("Alumina-", env!("ALUMINA_BOARD_ID"));
const DEVELOPMENT_PASSPHRASE: &str = "alumina-development";
const AP_PASSPHRASE: &str = match option_env!("ALUMINA_AP_PASSWORD") {
    Some(value) => value,
    None => DEVELOPMENT_PASSPHRASE,
};
const CREDENTIAL_SOURCE: CredentialSource = match option_env!("ALUMINA_AP_PASSWORD") {
    Some(_) => CredentialSource::BuildProvisioned,
    None => CredentialSource::DevelopmentFallback,
};

// Classic ESP32 has no PSRAM on the first MKS targets and must share its
// reclaimed DRAM between the vendor radio and all permanent service actors.
// TinyBee reserves two admissions for the browser bootstrap. The FOC target
// retains one until its own hardware memory profile is qualified.
#[cfg(any(feature = "board-mks-tinybee", feature = "board-mks-tinybee-4mb"))]
const HTTP_CONNECTIONS: usize = 2;
#[cfg(feature = "board-mks-esp32-foc-v1")]
const HTTP_CONNECTIONS: usize = 1;
#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
const HTTP_CONNECTIONS: usize = WebLimits::INITIAL.connections;
const HTTP_HEADER_BYTES: usize = WebLimits::INITIAL.header_bytes;
const HTTP_HEADER_COUNT: usize = WebLimits::INITIAL.header_count;
#[cfg(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-mks-tinybee",
    feature = "board-mks-tinybee-4mb"
))]
const HTTP_SOCKET_BYTES: usize = 1_536;
#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
const HTTP_SOCKET_BYTES: usize = WebLimits::INITIAL.socket_bytes;
const STACK_SOCKETS: usize = HTTP_CONNECTIONS + 2;
const STATION_HTTP_CONNECTIONS: usize = 1;
const STATION_STACK_SOCKETS: usize = STATION_HTTP_CONNECTIONS + 1;
const NETWORK_SCAN_TIMEOUT_MS: u32 = 3_000;
const NETWORK_ASSOCIATION_TIMEOUT_MS: u32 = 12_000;
const NETWORK_DHCP_TIMEOUT_MS: u64 = 12_000;
const ACCESS_POINT_START_TIMEOUT_MS: u64 = 3_000;
#[cfg(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-mks-tinybee",
    feature = "board-mks-tinybee-4mb"
))]
const DHCP_PACKET_BYTES: usize = 576;
#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
const DHCP_PACKET_BYTES: usize = 1_500;
const DHCP_METADATA_SLOTS: usize = 2;
const DHCP_LEASES: usize = 4;
const DNS_METADATA_SLOTS: usize = 2;
const CONNECTION_CLOSE_HEADER: (&str, &str) = ("Connection", "close");

const AP_IP: Ipv4Addr = Ipv4Addr::new(
    PROVISIONING_ADDRESS[0],
    PROVISIONING_ADDRESS[1],
    PROVISIONING_ADDRESS[2],
    PROVISIONING_ADDRESS[3],
);
const DHCP_FIRST: Ipv4Addr = Ipv4Addr::new(
    DHCP_RANGE_START[0],
    DHCP_RANGE_START[1],
    DHCP_RANGE_START[2],
    DHCP_RANGE_START[3],
);
const DHCP_LAST: Ipv4Addr = Ipv4Addr::new(
    DHCP_RANGE_END[0],
    DHCP_RANGE_END[1],
    DHCP_RANGE_END[2],
    DHCP_RANGE_END[3],
);

type ApRunner = Runner<'static, Interface<'static>>;
type StationRunner = Runner<'static, Interface<'static>>;
type AuthState = BlockingMutex<NoopRawMutex, RefCell<AuthenticationState>>;

#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
static AP_STACK_RESOURCES: StaticCell<StackResources<STACK_SOCKETS>> = StaticCell::new();
#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
static STATION_STACK_RESOURCES: StaticCell<StackResources<STATION_STACK_SOCKETS>> =
    StaticCell::new();
#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
static HTTP_TCP_BUFFERS: StaticCell<
    TcpBuffers<HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>,
> = StaticCell::new();
#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
static STATION_HTTP_TCP_BUFFERS: StaticCell<
    TcpBuffers<STATION_HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>,
> = StaticCell::new();
#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
static HTTP_HEADER_BUFFERS: ConstStaticCell<[u8; HTTP_HEADER_BYTES * HTTP_CONNECTIONS]> =
    ConstStaticCell::new([0; HTTP_HEADER_BYTES * HTTP_CONNECTIONS]);
#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
static STATION_HTTP_HEADER_BUFFER: ConstStaticCell<[u8; HTTP_HEADER_BYTES]> =
    ConstStaticCell::new([0; HTTP_HEADER_BYTES]);
#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
static DHCP_UDP_BUFFERS: StaticCell<
    UdpBuffers<1, DHCP_PACKET_BYTES, DHCP_PACKET_BYTES, DHCP_METADATA_SLOTS>,
> = StaticCell::new();
#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
static DHCP_WORK_BUFFER: StaticCell<[u8; DHCP_PACKET_BYTES]> = StaticCell::new();
#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
static DNS_UDP_BUFFERS: StaticCell<
    UdpBuffers<1, CAPTIVE_DNS_PACKET_BYTES, CAPTIVE_DNS_PACKET_BYTES, DNS_METADATA_SLOTS>,
> = StaticCell::new();
#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
static DNS_WORK_BUFFER: StaticCell<[u8; CAPTIVE_DNS_PACKET_BYTES]> = StaticCell::new();
static AUTH_STATE: StaticCell<AuthState> = StaticCell::new();

struct NetworkResources {
    stack: &'static mut StackResources<STACK_SOCKETS>,
    tcp: &'static TcpBuffers<HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>,
    headers: &'static mut [u8],
    station_stack: &'static mut StackResources<STATION_STACK_SOCKETS>,
    station_tcp:
        &'static TcpBuffers<STATION_HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>,
    station_header: &'static mut [u8; HTTP_HEADER_BYTES],
    dhcp: &'static UdpBuffers<1, DHCP_PACKET_BYTES, DHCP_PACKET_BYTES, DHCP_METADATA_SLOTS>,
    dhcp_work: &'static mut [u8],
    dns: &'static UdpBuffers<
        1,
        CAPTIVE_DNS_PACKET_BYTES,
        CAPTIVE_DNS_PACKET_BYTES,
        DNS_METADATA_SLOTS,
    >,
    dns_work: &'static mut [u8; CAPTIVE_DNS_PACKET_BYTES],
}

#[cfg(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-mks-tinybee",
    feature = "board-mks-tinybee-4mb"
))]
#[inline(never)]
fn initialize_network_resources() -> NetworkResources {
    let stack = Box::leak(Box::write(
        Box::<StackResources<STACK_SOCKETS>>::new_uninit(),
        StackResources::new(),
    ));
    trace_classic_network_heap("resources-ap-stack");
    let tcp = Box::leak(Box::write(
        Box::<TcpBuffers<HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>>::new_uninit(),
        TcpBuffers::new(),
    ));
    trace_classic_network_heap("resources-ap-tcp");
    // A flat byte vector is initialized without a value-sized array temporary.
    // Even constructing one 2 KiB worker array on the core-0 startup stack is
    // enough to make this path sensitive to unrelated diagnostic codegen.
    let headers =
        Box::leak(alloc::vec![0_u8; HTTP_HEADER_BYTES * HTTP_CONNECTIONS].into_boxed_slice());
    trace_classic_network_heap("resources-ap-headers");
    let station_stack = Box::leak(Box::write(
        Box::<StackResources<STATION_STACK_SOCKETS>>::new_uninit(),
        StackResources::new(),
    ));
    trace_classic_network_heap("resources-station-stack");
    let station_tcp = Box::leak(Box::write(
        Box::<
            TcpBuffers<STATION_HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>,
        >::new_uninit(),
        TcpBuffers::new(),
    ));
    trace_classic_network_heap("resources-station-tcp");
    let station_header = Box::leak(Box::write(
        Box::<[u8; HTTP_HEADER_BYTES]>::new_uninit(),
        [0; HTTP_HEADER_BYTES],
    ));
    trace_classic_network_heap("resources-station-header");
    let dhcp = Box::leak(Box::write(
        Box::<UdpBuffers<1, DHCP_PACKET_BYTES, DHCP_PACKET_BYTES, DHCP_METADATA_SLOTS>>::new_uninit(
        ),
        UdpBuffers::new(),
    ));
    trace_classic_network_heap("resources-dhcp");
    let dhcp_work = Box::leak(alloc::vec![0_u8; DHCP_PACKET_BYTES].into_boxed_slice());
    trace_classic_network_heap("resources-dhcp-work");
    let dns =
        Box::leak(Box::write(
            Box::<
                UdpBuffers<
                    1,
                    CAPTIVE_DNS_PACKET_BYTES,
                    CAPTIVE_DNS_PACKET_BYTES,
                    DNS_METADATA_SLOTS,
                >,
            >::new_uninit(),
            UdpBuffers::new(),
        ));
    trace_classic_network_heap("resources-dns");
    let dns_work = Box::leak(Box::write(
        Box::<[u8; CAPTIVE_DNS_PACKET_BYTES]>::new_uninit(),
        [0; CAPTIVE_DNS_PACKET_BYTES],
    ));
    trace_classic_network_heap("resources-dns-work");
    NetworkResources {
        stack,
        tcp,
        headers,
        station_stack,
        station_tcp,
        station_header,
        dhcp,
        dhcp_work,
        dns,
        dns_work,
    }
}

#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
fn initialize_network_resources() -> NetworkResources {
    NetworkResources {
        stack: AP_STACK_RESOURCES.init(StackResources::new()),
        tcp: HTTP_TCP_BUFFERS.init(TcpBuffers::new()),
        headers: HTTP_HEADER_BUFFERS.take(),
        station_stack: STATION_STACK_RESOURCES.init(StackResources::new()),
        station_tcp: STATION_HTTP_TCP_BUFFERS.init(TcpBuffers::new()),
        station_header: STATION_HTTP_HEADER_BUFFER.take(),
        dhcp: DHCP_UDP_BUFFERS.init(UdpBuffers::new()),
        dhcp_work: DHCP_WORK_BUFFER.init([0; DHCP_PACKET_BYTES]),
        dns: DNS_UDP_BUFFERS.init(UdpBuffers::new()),
        dns_work: DNS_WORK_BUFFER.init([0; CAPTIVE_DNS_PACKET_BYTES]),
    }
}

/// Controller state retained by the sole core-0 provisioning owner.
pub struct NetworkControl {
    controller: WifiController<'static>,
    access_point_stack: Stack<'static>,
    access_point: AccessPointConfig,
    station_stack: Stack<'static>,
    supervisor: NetworkSupervisor,
    status: NetworkStatus,
    last_scan: Option<NetworkScanResult>,
    last_mutation_operation: Option<Operation>,
    last_mutation_digest: Digest,
    credential_source: CredentialSource,
    boot_nonce: BootNonce,
    device_id: DeviceId,
    capability_identity: CapabilityIdentity,
}

impl NetworkControl {
    pub fn access_point_link_up(&self) -> bool {
        self.access_point_stack.is_link_up()
    }

    /// Current portable supervision state.
    pub const fn supervisor(&self) -> NetworkSupervisor {
        self.supervisor
    }

    /// Credential provenance without credential material.
    pub const fn credential_source(&self) -> CredentialSource {
        self.credential_source
    }

    /// Public boot identity shared by authentication, clock models, and jobs.
    pub const fn boot_nonce(&self) -> BootNonce {
        self.boot_nonce
    }

    /// Stable non-secret physical MCU identity used by targeted work.
    pub const fn device_id(&self) -> DeviceId {
        self.device_id
    }

    /// Verified compile-time board capability identity used by authenticated services.
    pub const fn capability_identity(&self) -> CapabilityIdentity {
        self.capability_identity
    }

    /// Whether this sole service owner handles the request family.
    pub fn handles(request: &ServiceRequest) -> bool {
        NativeRequest::decode(request.bytes())
            .is_ok_and(|request| request.frame.kind == FrameKind::Network)
    }

    /// Dispatch one authenticated canonical network operation.
    pub async fn dispatch(
        &mut self,
        request: &ServiceRequest,
        now: DeviceCycle,
    ) -> ServiceResponse {
        let Ok(native) = NativeRequest::decode(request.bytes()) else {
            return ServiceResponse::invalid_native();
        };
        if native.frame.kind != FrameKind::Network {
            return ServiceResponse::invalid_native();
        }
        match native.message.operation {
            Operation::NetworkStatus if native.body.is_empty() => {
                self.refresh_station_status();
                network_response(native, now, StatusCode::Ok, &self.status.encode())
            }
            Operation::NetworkScan => {
                let Ok(scan) = NetworkScanRequest::decode(native.body) else {
                    return network_response(native, now, StatusCode::InvalidRequest, &[]);
                };
                match self.scan(scan, now).await {
                    Ok(result) => network_response(native, now, StatusCode::Ok, &result.encode()),
                    Err(status) => network_response(native, now, status, &[]),
                }
            }
            Operation::NetworkJoin => {
                let Ok(join) = NetworkJoinRequest::decode(native.body) else {
                    return network_response(native, now, StatusCode::InvalidRequest, &[]);
                };
                let status = self.join(&join).await;
                network_response(native, now, status, &self.status.encode())
            }
            Operation::NetworkLeave => {
                let Ok(mutation) = NetworkMutationRequest::decode(native.body) else {
                    return network_response(native, now, StatusCode::InvalidRequest, &[]);
                };
                let status = self.leave(mutation, native.body).await;
                network_response(native, now, status, &self.status.encode())
            }
            Operation::NetworkRecoverAp => {
                let Ok(mutation) = NetworkMutationRequest::decode(native.body) else {
                    return network_response(native, now, StatusCode::InvalidRequest, &[]);
                };
                let status = self.recover(mutation, native.body).await;
                network_response(native, now, status, &self.status.encode())
            }
            Operation::NetworkStatus
            | Operation::IdentityGet
            | Operation::CapabilitiesGet
            | Operation::CapabilityVisualGet
            | Operation::ClockHeartbeat
            | Operation::ConfigurationGet
            | Operation::ConfigurationValidate
            | Operation::ConfigurationCommit
            | Operation::ConfigurationRollback
            | Operation::JobInspect
            | Operation::JobPrepare
            | Operation::JobCommit
            | Operation::JobConfirm
            | Operation::JobLeaseRenew
            | Operation::JobAbort
            | Operation::JobHold
            | Operation::JobResume
            | Operation::JobCancel
            | Operation::JobStatus
            | Operation::CommandBatch
            | Operation::CommandDiagnosticLease
            | Operation::CommandDiagnosticRelease
            | Operation::TelemetrySubscribe
            | Operation::TelemetryUnsubscribe
            | Operation::TelemetryEvent
            | Operation::TelemetryStatus
            | Operation::TelemetryPoll
            | Operation::StorageStatus
            | Operation::StorageList
            | Operation::StorageBeginUpload
            | Operation::StoragePutChunk
            | Operation::StorageFinalize
            | Operation::StorageRead
            | Operation::StorageDelete
            | Operation::StorageScrub
            | Operation::StorageProvision
            | Operation::StorageInspect
            | Operation::HealthSnapshot
            | Operation::FaultEvent
            | Operation::FaultResetRequest
            | Operation::FaultResetConfirm
            | Operation::WaveformConfigure
            | Operation::WaveformArm
            | Operation::WaveformChunk
            | Operation::WaveformStop
            | Operation::WaveformStatus
            | Operation::WaveformRead
            | Operation::UpdateInspect
            | Operation::UpdateBegin
            | Operation::UpdatePutChunk
            | Operation::UpdateFinalize
            | Operation::UpdateCommit
            | Operation::UpdateRollback
            | Operation::GraphGet
            | Operation::GraphInstall
            | Operation::GraphActivate
            | Operation::GraphClear
            | Operation::GraphStart
            | Operation::GraphStop => ServiceResponse::invalid_native(),
        }
    }

    async fn scan(
        &mut self,
        request: NetworkScanRequest,
        now: DeviceCycle,
    ) -> Result<NetworkScanResult, StatusCode> {
        if let Some(previous) = self.last_scan {
            if previous.transaction_id == request.transaction_id {
                return Ok(previous);
            }
            if request.transaction_id < previous.transaction_id {
                return Err(StatusCode::Conflict);
            }
        }
        let config = ScanConfig::default().with_max(MAX_NETWORK_SCAN_RESULTS + 1);
        let scanned = match with_timeout(
            NETWORK_SCAN_TIMEOUT_MS,
            self.controller.scan_async(&config),
        )
        .await
        {
            Ok(scanned) => scanned,
            Err(WithTimeoutError::Timeout) => {
                self.retain_failure(NetworkFailure::Timeout);
                return Err(StatusCode::Deadline);
            }
            Err(WithTimeoutError::Error(_)) => {
                self.retain_failure(NetworkFailure::Scan);
                return Err(StatusCode::Internal);
            }
        };
        let mut entries = FixedVec::<NetworkScanEntry, MAX_NETWORK_SCAN_RESULTS>::new();
        let mut truncated = scanned.len() > MAX_NETWORK_SCAN_RESULTS;
        for access_point in scanned {
            let authentication = network_authentication(access_point.auth_method);
            let Ok(entry) = NetworkScanEntry::try_new(
                access_point.ssid.as_str(),
                authentication,
                access_point.channel,
                access_point.signal_strength,
                access_point.bssid,
            ) else {
                continue;
            };
            if entries.iter().any(|known| known.bssid == entry.bssid) {
                continue;
            }
            if entries.push(entry).is_err() {
                truncated = true;
            }
        }
        let scan_generation = self.status.scan_generation.wrapping_add(1).max(1);
        let result = NetworkScanResult::try_new(
            request.transaction_id,
            self.supervisor.generation(),
            scan_generation,
            now.0,
            entries.as_slice(),
            truncated,
        )
        .map_err(|_| StatusCode::Internal)?;
        self.last_scan = Some(result);
        self.status.scan_generation = scan_generation;
        self.retain_failure(NetworkFailure::None);
        Ok(result)
    }

    async fn join(&mut self, request: &NetworkJoinRequest) -> StatusCode {
        let body = request.encode();
        if let Some(replay) =
            self.reconcile_mutation(Operation::NetworkJoin, request.transaction_id, &body)
        {
            return replay;
        }
        if request.expected_generation != self.supervisor.generation() {
            return StatusCode::Conflict;
        }
        self.remember_mutation(Operation::NetworkJoin, request.transaction_id, &body);
        if self.supervisor.begin_station_join().is_err() {
            self.retain_failure_for(
                request,
                NetworkFailure::Interrupted,
                StationLinkState::Disconnected,
            );
            return StatusCode::Conflict;
        }
        self.retain_join_state(request, StationLinkState::Associating, NetworkFailure::None);

        if self.controller.is_connected() {
            let _ = with_timeout(2_000, self.controller.disconnect_async()).await;
        }
        let mut client = StationConfig::default()
            .with_ssid(String::from(request.ssid()))
            .with_password(String::from(request.passphrase()))
            .with_auth_method(radio_authentication(request.authentication()));
        if let Some(bssid) = request.bssid() {
            client = client.with_bssid(bssid);
        }
        if let Some(channel) = request.channel() {
            client = client.with_channel(channel);
        }
        if self
            .controller
            .set_config(&WifiConfig::AccessPointStation(
                client,
                self.access_point.clone(),
            ))
            .is_err()
        {
            let _ = self.supervisor.station_join_failed();
            self.retain_failure_for(
                request,
                NetworkFailure::Driver,
                StationLinkState::Disconnected,
            );
            return StatusCode::Internal;
        }
        match with_timeout(
            NETWORK_ASSOCIATION_TIMEOUT_MS,
            self.controller.connect_async(),
        )
        .await
        {
            Ok(_) => {}
            Err(WithTimeoutError::Timeout) => {
                let _ = self.supervisor.station_join_failed();
                self.retain_failure_for(
                    request,
                    NetworkFailure::Timeout,
                    StationLinkState::Disconnected,
                );
                return StatusCode::Deadline;
            }
            Err(WithTimeoutError::Error(_)) => {
                let _ = self.supervisor.station_join_failed();
                self.retain_failure_for(
                    request,
                    NetworkFailure::Association,
                    StationLinkState::Disconnected,
                );
                return StatusCode::Unauthorized;
            }
        }
        if self.supervisor.station_joined().is_err() {
            self.supervisor.begin_recovery();
            self.retain_failure_for(
                request,
                NetworkFailure::Driver,
                StationLinkState::Associated,
            );
            return StatusCode::Internal;
        }
        self.retain_join_state(request, StationLinkState::Associated, NetworkFailure::None);
        if embassy_time::with_timeout(
            Duration::from_millis(NETWORK_DHCP_TIMEOUT_MS),
            self.station_stack.wait_config_up(),
        )
        .await
        .is_err()
        {
            self.retain_failure_for(request, NetworkFailure::Dhcp, StationLinkState::Associated);
            return StatusCode::Deadline;
        }
        self.refresh_station_status();
        if self.status.station_link == StationLinkState::Addressed {
            StatusCode::Ok
        } else {
            self.retain_failure_for(request, NetworkFailure::Dhcp, StationLinkState::Associated);
            StatusCode::Deadline
        }
    }

    async fn leave(&mut self, request: NetworkMutationRequest, body: &[u8]) -> StatusCode {
        if let Some(replay) =
            self.reconcile_mutation(Operation::NetworkLeave, request.transaction_id, body)
        {
            return replay;
        }
        if request.expected_generation != self.supervisor.generation() {
            return StatusCode::Conflict;
        }
        self.remember_mutation(Operation::NetworkLeave, request.transaction_id, body);
        let mut committed_supervisor = self.supervisor;
        if committed_supervisor.leave_station().is_err() {
            self.retain_failure(NetworkFailure::Interrupted);
            return StatusCode::Conflict;
        }
        if self.controller.is_connected()
            && with_timeout(2_000, self.controller.disconnect_async())
                .await
                .is_err()
        {
            self.supervisor.begin_recovery();
            self.retain_failure(NetworkFailure::Driver);
            return StatusCode::Internal;
        }
        if self
            .controller
            .set_config(&WifiConfig::AccessPointStation(
                StationConfig::default(),
                self.access_point.clone(),
            ))
            .is_err()
        {
            self.supervisor.begin_recovery();
            self.retain_failure(NetworkFailure::Driver);
            return StatusCode::Internal;
        }
        self.supervisor = committed_supervisor;
        self.station_stack
            .set_config_v4(ConfigV4::Dhcp(Default::default()));
        self.status = NetworkStatus::try_new(
            self.supervisor.phase(),
            self.supervisor.generation(),
            self.status.scan_generation,
            request.transaction_id,
            StationLinkState::Disconnected,
            NetworkAuthentication::Unsupported,
            self.base_status_flags(NetworkFailure::None, false, StationLinkState::Disconnected),
            0,
            i8::MIN,
            0,
            [0; 4],
            [0; 4],
            [0; 6],
            NetworkFailure::None,
            None,
        )
        .expect("leave status is internally valid");
        StatusCode::Ok
    }

    async fn recover(&mut self, request: NetworkMutationRequest, body: &[u8]) -> StatusCode {
        if let Some(replay) =
            self.reconcile_mutation(Operation::NetworkRecoverAp, request.transaction_id, body)
        {
            return replay;
        }
        if request.expected_generation != self.supervisor.generation() {
            return StatusCode::Conflict;
        }
        self.remember_mutation(Operation::NetworkRecoverAp, request.transaction_id, body);
        self.supervisor.begin_recovery();
        if self.controller.is_connected() {
            let _ = with_timeout(2_000, self.controller.disconnect_async()).await;
        }
        if self
            .controller
            .set_config(&WifiConfig::AccessPointStation(
                StationConfig::default(),
                self.access_point.clone(),
            ))
            .is_err()
            || self.supervisor.begin_access_point().is_err()
        {
            self.supervisor.begin_recovery();
            self.retain_failure(NetworkFailure::Driver);
            return StatusCode::Internal;
        }
        if self.supervisor.access_point_ready().is_err() {
            self.supervisor.begin_recovery();
            self.retain_failure(NetworkFailure::Driver);
            return StatusCode::Internal;
        }
        self.station_stack
            .set_config_v4(ConfigV4::Dhcp(Default::default()));
        self.status = NetworkStatus::try_new(
            self.supervisor.phase(),
            self.supervisor.generation(),
            self.status.scan_generation,
            request.transaction_id,
            StationLinkState::Disconnected,
            NetworkAuthentication::Unsupported,
            self.base_status_flags(NetworkFailure::None, false, StationLinkState::Disconnected),
            0,
            i8::MIN,
            0,
            [0; 4],
            [0; 4],
            [0; 6],
            NetworkFailure::None,
            None,
        )
        .expect("recovery status is internally valid");
        StatusCode::Ok
    }

    fn reconcile_mutation(
        &self,
        operation: Operation,
        transaction_id: u64,
        body: &[u8],
    ) -> Option<StatusCode> {
        let previous = self.status.last_transaction_id;
        if transaction_id < previous {
            return Some(StatusCode::Conflict);
        }
        if transaction_id != previous || previous == 0 {
            return None;
        }
        let digest = sha256(body).digest;
        Some(
            if self.last_mutation_operation == Some(operation)
                && self.last_mutation_digest == digest
            {
                if self
                    .status
                    .flags
                    .contains(NetworkStatusFlags::LAST_OPERATION_FAILED)
                {
                    network_failure_status(self.status.last_failure)
                } else {
                    StatusCode::Ok
                }
            } else {
                StatusCode::Conflict
            },
        )
    }

    fn remember_mutation(&mut self, operation: Operation, transaction_id: u64, body: &[u8]) {
        self.last_mutation_operation = Some(operation);
        self.last_mutation_digest = sha256(body).digest;
        self.status.last_transaction_id = transaction_id;
    }

    fn retain_join_state(
        &mut self,
        request: &NetworkJoinRequest,
        link: StationLinkState,
        failure: NetworkFailure,
    ) {
        let (prefix, address, gateway) = self.station_ipv4(link);
        let signal = if matches!(
            link,
            StationLinkState::Associated | StationLinkState::Addressed
        ) {
            self.controller
                .rssi()
                .ok()
                .and_then(|value| i8::try_from(value).ok())
                .unwrap_or(i8::MIN)
        } else {
            i8::MIN
        };
        self.status = NetworkStatus::try_new(
            self.supervisor.phase(),
            self.supervisor.generation(),
            self.status.scan_generation,
            request.transaction_id,
            link,
            request.authentication(),
            self.base_status_flags(failure, true, link),
            request.channel().unwrap_or(0),
            signal,
            prefix,
            address,
            gateway,
            request.bssid().unwrap_or([0; 6]),
            failure,
            Some(request.ssid()),
        )
        .expect("join status is internally valid");
    }

    fn retain_failure_for(
        &mut self,
        request: &NetworkJoinRequest,
        failure: NetworkFailure,
        link: StationLinkState,
    ) {
        self.retain_join_state(request, link, failure);
    }

    fn retain_failure(&mut self, failure: NetworkFailure) {
        let ssid = self.status.ssid().map(String::from);
        let configured = ssid.is_some();
        let flags = self.base_status_flags(failure, configured, self.status.station_link);
        self.status = NetworkStatus::try_new(
            self.supervisor.phase(),
            self.supervisor.generation(),
            self.status.scan_generation,
            self.status.last_transaction_id,
            self.status.station_link,
            self.status.authentication,
            flags,
            self.status.channel,
            self.status.signal_dbm,
            self.status.ipv4_prefix,
            self.status.ipv4_address,
            self.status.ipv4_gateway,
            self.status.bssid,
            failure,
            ssid.as_deref(),
        )
        .expect("retained network failure is internally valid");
    }

    fn refresh_station_status(&mut self) {
        if !matches!(
            self.status.station_link,
            StationLinkState::Associated | StationLinkState::Addressed
        ) {
            return;
        }
        let Some(ssid) = self.status.ssid().map(String::from) else {
            return;
        };
        let link = if self.station_stack.config_v4().is_some() {
            StationLinkState::Addressed
        } else {
            StationLinkState::Associated
        };
        let failure = if link == StationLinkState::Addressed
            && self.status.last_failure == NetworkFailure::Dhcp
        {
            NetworkFailure::None
        } else {
            self.status.last_failure
        };
        let (prefix, address, gateway) = self.station_ipv4(link);
        let signal = self
            .controller
            .rssi()
            .ok()
            .and_then(|value| i8::try_from(value).ok())
            .unwrap_or(i8::MIN);
        self.status = NetworkStatus::try_new(
            self.supervisor.phase(),
            self.supervisor.generation(),
            self.status.scan_generation,
            self.status.last_transaction_id,
            link,
            self.status.authentication,
            self.base_status_flags(failure, true, link),
            self.status.channel,
            signal,
            prefix,
            address,
            gateway,
            self.status.bssid,
            failure,
            Some(ssid.as_str()),
        )
        .expect("refreshed station status is internally valid");
    }

    fn station_ipv4(&self, link: StationLinkState) -> (u8, [u8; 4], [u8; 4]) {
        if link != StationLinkState::Addressed {
            return (0, [0; 4], [0; 4]);
        }
        let Some(config) = self.station_stack.config_v4() else {
            return (0, [0; 4], [0; 4]);
        };
        let mut address = [0_u8; 4];
        address.copy_from_slice(&config.address.address().octets());
        let mut gateway = [0_u8; 4];
        if let Some(value) = config.gateway {
            gateway.copy_from_slice(&value.octets());
        }
        (config.address.prefix_len(), address, gateway)
    }

    fn base_status_flags(
        &self,
        failure: NetworkFailure,
        configured: bool,
        link: StationLinkState,
    ) -> NetworkStatusFlags {
        let mut flags = 0_u8;
        let associated = matches!(
            link,
            StationLinkState::Associated | StationLinkState::Addressed
        );
        if self.supervisor.access_point_expected() {
            flags |= NetworkStatusFlags::AP_EXPECTED;
        }
        if configured {
            flags |= NetworkStatusFlags::STATION_CONFIGURED;
        }
        if associated {
            flags |= NetworkStatusFlags::STATION_ASSOCIATED;
        }
        if link == StationLinkState::Addressed {
            flags |= NetworkStatusFlags::STATION_IPV4_READY;
        }
        if self.last_scan.is_some_and(|scan| scan.truncated()) {
            flags |= NetworkStatusFlags::SCAN_TRUNCATED;
        }
        if failure != NetworkFailure::None {
            flags |= NetworkStatusFlags::LAST_OPERATION_FAILED;
        }
        if !self.supervisor.access_point_expected() {
            flags |= NetworkStatusFlags::RECOVERY_REQUIRED;
        }
        NetworkStatusFlags(flags)
    }
}

fn network_response(
    request: NativeRequest<'_>,
    now: DeviceCycle,
    status: StatusCode,
    body: &[u8],
) -> ServiceResponse {
    ServiceResponse::native(request, now, status, body)
        .unwrap_or_else(|_| ServiceResponse::invalid_native())
}

fn network_authentication(authentication: Option<AuthenticationMethod>) -> NetworkAuthentication {
    match authentication {
        Some(AuthenticationMethod::None) => NetworkAuthentication::Open,
        Some(AuthenticationMethod::Wpa2Personal) => NetworkAuthentication::Wpa2Personal,
        Some(AuthenticationMethod::Wpa3Personal) => NetworkAuthentication::Wpa3Personal,
        Some(AuthenticationMethod::Wpa2Wpa3Personal) => NetworkAuthentication::Wpa2Wpa3Personal,
        _ => NetworkAuthentication::Unsupported,
    }
}

fn radio_authentication(authentication: NetworkAuthentication) -> AuthenticationMethod {
    match authentication {
        NetworkAuthentication::Open => AuthenticationMethod::None,
        NetworkAuthentication::Wpa2Personal => AuthenticationMethod::Wpa2Personal,
        NetworkAuthentication::Wpa3Personal => AuthenticationMethod::Wpa3Personal,
        NetworkAuthentication::Wpa2Wpa3Personal => AuthenticationMethod::Wpa2Wpa3Personal,
        NetworkAuthentication::Unsupported => AuthenticationMethod::Wpa2Personal,
    }
}

fn network_failure_status(failure: NetworkFailure) -> StatusCode {
    match failure {
        NetworkFailure::None => StatusCode::Ok,
        NetworkFailure::Association => StatusCode::Unauthorized,
        NetworkFailure::Timeout | NetworkFailure::Dhcp => StatusCode::Deadline,
        NetworkFailure::Interrupted => StatusCode::Conflict,
        NetworkFailure::Scan | NetworkFailure::Driver | NetworkFailure::Storage => {
            StatusCode::Internal
        }
    }
}

/// Initializes the radio on core 0 and starts all AP service tasks on its executor.
pub async fn start(
    spawner: Spawner,
    wifi: WIFI<'static>,
    service_bridge: &'static ServiceBridge,
    device_id: DeviceId,
) -> NetworkControl {
    assert_service_core("network initialization");
    trace_classic_network_layout();
    let NetworkResources {
        stack: stack_resources,
        tcp: http_tcp_buffers,
        headers: http_header_buffers,
        station_stack: station_stack_resources,
        station_tcp: station_http_tcp_buffers,
        station_header: station_http_header_buffer,
        dhcp: dhcp_buffers,
        dhcp_work,
        dns: dns_buffers,
        dns_work,
    } = initialize_network_resources();
    trace_classic_network_heap("resources");
    let capability_identity = verify_declared_identity(selected::PACKAGE)
        .unwrap_or_else(|_| panic!("selected board capability identity is invalid"));

    let profile = AccessPointProfile::new(AP_SSID, AP_PASSPHRASE, CREDENTIAL_SOURCE);
    #[cfg(any(
        feature = "board-mks-esp32-foc-v1",
        feature = "board-mks-tinybee",
        feature = "board-mks-tinybee-4mb"
    ))]
    let profile = {
        // One associated browser/controller may use the target's bounded set
        // of concurrent TCP admissions.
        let mut profile = profile;
        profile.maximum_clients = 1;
        profile
    };
    if profile.validate().is_err() || WebLimits::INITIAL.validate().is_err() {
        panic!("invalid compile-time Wi-Fi/web policy");
    }
    if CREDENTIAL_SOURCE == CredentialSource::DevelopmentFallback {
        warn!("development AP credential active; image is not production-armable");
    }

    let access_point = AccessPointConfig::default()
        .with_ssid(String::from(profile.ssid))
        .with_password(String::from(profile.passphrase))
        .with_auth_method(AuthenticationMethod::Wpa2Personal)
        .with_channel(profile.channel)
        .with_max_connections(u16::from(profile.maximum_clients));
    let initial_config =
        WifiConfig::AccessPointStation(StationConfig::default(), access_point.clone());
    let radio_config = target_radio_config().with_initial_config(initial_config);
    let (controller, interfaces) = match esp_radio::wifi::new(wifi, radio_config) {
        Ok(parts) => parts,
        Err(_) => panic!("Wi-Fi controller initialization failed"),
    };
    #[cfg(feature = "radio-driver-diagnostics")]
    esp_radio::wifi_set_log_verbose();
    trace_classic_network_heap("wifi-new");
    trace_classic_ap_config("configured");
    trace_classic_network_heap("ap-config");

    let mut supervisor = NetworkSupervisor::new();
    if supervisor.begin_access_point().is_err() {
        panic!("invalid AP supervision transition");
    }
    trace_classic_network_heap("ap-starting");
    trace_classic_ap_config("started");
    trace_classic_network_heap("ap-started");

    let ip = Ipv4Address::new(
        PROVISIONING_ADDRESS[0],
        PROVISIONING_ADDRESS[1],
        PROVISIONING_ADDRESS[2],
        PROVISIONING_ADDRESS[3],
    );
    let stack_config = NetworkConfig::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(ip, PROVISIONING_PREFIX),
        gateway: None,
        dns_servers: Default::default(),
    });
    let rng = Rng::new();
    let mut nonce_bytes = [0_u8; AUTH_NONCE_BYTES];
    for chunk in nonce_bytes.chunks_exact_mut(4) {
        chunk.copy_from_slice(&rng.random().to_le_bytes());
    }
    let auth_nonce = match BootNonce::new(nonce_bytes) {
        Ok(nonce) => nonce,
        Err(_) => panic!("hardware RNG returned an invalid authentication nonce"),
    };
    let auth_state = match AuthenticationState::new(auth_nonce, AuthRateLimit::INITIAL) {
        Ok(state) => AUTH_STATE.init(BlockingMutex::new(RefCell::new(state))),
        Err(_) => panic!("invalid request-authentication policy"),
    };
    let seed = (u64::from(rng.random()) << 32) | u64::from(rng.random());
    let (stack, runner) =
        embassy_net::new(interfaces.access_point, stack_config, stack_resources, seed);
    let station_seed = seed ^ 0xa17e_51a7_10c0_0001;
    let (station_stack, station_runner) = embassy_net::new(
        interfaces.station,
        NetworkConfig::dhcpv4(Default::default()),
        station_stack_resources,
        station_seed,
    );

    spawner.spawn(ap_runner_task(runner).expect("failed to allocate AP network runner"));
    spawner.spawn(
        station_runner_task(station_runner).expect("failed to allocate station network runner"),
    );
    if embassy_time::with_timeout(
        Duration::from_millis(ACCESS_POINT_START_TIMEOUT_MS),
        stack.wait_link_up(),
    )
    .await
    .is_err()
    {
        panic!("Wi-Fi AP did not reach link-up before the startup deadline");
    }
    let handler = AluminaHttpHandler {
        credential_source: CREDENTIAL_SOURCE,
        auth_nonce,
        auth_state,
        service_bridge,
        capability_identity,
        device_id,
    };
    spawn_ap_http_workers(
        spawner,
        stack,
        handler,
        http_tcp_buffers,
        http_header_buffers,
    );
    spawner.spawn(
        station_http_worker_task(
            station_stack,
            station_http_tcp_buffers,
            station_http_header_buffer,
            handler,
        )
        .expect("failed to allocate station HTTP worker"),
    );
    spawner
        .spawn(dhcp_task(stack, dhcp_buffers, dhcp_work).expect("failed to allocate DHCP server"));
    spawner.spawn(
        captive_dns_task(stack, dns_buffers, dns_work)
            .expect("failed to allocate captive DNS server"),
    );
    if supervisor.access_point_ready().is_err() {
        panic!("invalid AP supervision transition");
    }

    info!("protected Alumina AP started: {}", AP_SSID);
    let status = NetworkStatus::provisioning(supervisor);
    NetworkControl {
        controller,
        access_point_stack: stack,
        access_point,
        station_stack,
        supervisor,
        status,
        last_scan: None,
        last_mutation_operation: None,
        last_mutation_digest: Digest::ZERO,
        credential_source: CREDENTIAL_SOURCE,
        boot_nonce: auth_nonce,
        device_id,
        capability_identity,
    }
}

#[cfg(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-mks-tinybee",
    feature = "board-mks-tinybee-4mb"
))]
fn target_radio_config() -> ControllerConfig {
    ControllerConfig::default()
}

#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
fn target_radio_config() -> ControllerConfig {
    ControllerConfig::default()
}

#[cfg(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-mks-tinybee",
    feature = "board-mks-tinybee-4mb"
))]
fn trace_classic_network_heap(stage: &str) {
    let heap = esp_alloc::HEAP.stats();
    esp_println_uart::println!(
        "alumina: network heap stage={} used={} free={}",
        stage,
        heap.current_usage,
        esp_alloc::HEAP.free()
    );
}

#[cfg(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-mks-tinybee",
    feature = "board-mks-tinybee-4mb"
))]
fn trace_classic_ap_config(stage: &str) {
    match alumina_esp_radio_audit::read_access_point_config() {
        Ok(ap) => {
            let ssid_length = usize::from(ap.ssid_length).min(ap.ssid.len());
            let ssid = core::str::from_utf8(&ap.ssid[..ssid_length]).unwrap_or("<invalid>");
            esp_println_uart::println!(
                "alumina: Wi-Fi AP readback stage={} ssid={} ssid-len={} hidden={} auth={} mode={} struct-size={} beacon-offset={} csa-offset={} dtim-offset={} beacon={} csa={} dtim={} configured-channel={} active-channel={} secondary={} max-clients={} mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} protocols=0x{:02x} bandwidth={} power-save={}",
                stage,
                ssid,
                ap.ssid_length,
                ap.ssid_hidden,
                ap.authentication_mode,
                ap.radio_mode,
                ap.structure_bytes,
                ap.beacon_interval_offset,
                ap.csa_count_offset,
                ap.dtim_period_offset,
                ap.beacon_interval,
                ap.csa_count,
                ap.dtim_period,
                ap.channel,
                ap.active_primary_channel,
                ap.active_secondary_channel,
                ap.maximum_connections,
                ap.mac_address[0],
                ap.mac_address[1],
                ap.mac_address[2],
                ap.mac_address[3],
                ap.mac_address[4],
                ap.mac_address[5],
                ap.protocol_bitmap,
                ap.bandwidth,
                ap.power_save_mode
            );
        }
        Err(error) => esp_println_uart::println!(
            "alumina: Wi-Fi AP readback stage={} error={:?}",
            stage,
            error
        ),
    }
}

#[cfg(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-mks-tinybee",
    feature = "board-mks-tinybee-4mb"
))]
fn trace_classic_network_layout() {
    let ap_stack = core::mem::size_of::<StackResources<STACK_SOCKETS>>();
    let ap_tcp =
        core::mem::size_of::<TcpBuffers<HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>>();
    let ap_headers = core::mem::size_of::<[[u8; HTTP_HEADER_BYTES]; HTTP_CONNECTIONS]>();
    let station_stack = core::mem::size_of::<StackResources<STATION_STACK_SOCKETS>>();
    let station_tcp = core::mem::size_of::<
        TcpBuffers<STATION_HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>,
    >();
    let station_header = core::mem::size_of::<[u8; HTTP_HEADER_BYTES]>();
    let dhcp = core::mem::size_of::<
        UdpBuffers<1, DHCP_PACKET_BYTES, DHCP_PACKET_BYTES, DHCP_METADATA_SLOTS>,
    >();
    let dns = core::mem::size_of::<
        UdpBuffers<1, CAPTIVE_DNS_PACKET_BYTES, CAPTIVE_DNS_PACKET_BYTES, DNS_METADATA_SLOTS>,
    >();
    let total = ap_stack
        + ap_tcp
        + ap_headers
        + station_stack
        + station_tcp
        + station_header
        + dhcp
        + DHCP_PACKET_BYTES
        + dns
        + CAPTIVE_DNS_PACKET_BYTES;
    esp_println_uart::println!(
        "alumina: network layout ap_stack={} ap_tcp={} ap_headers={} station_stack={} station_tcp={} station_header={} dhcp={} dhcp_work={} dns={} dns_work={} total={}",
        ap_stack,
        ap_tcp,
        ap_headers,
        station_stack,
        station_tcp,
        station_header,
        dhcp,
        DHCP_PACKET_BYTES,
        dns,
        CAPTIVE_DNS_PACKET_BYTES,
        total
    );
}

#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
fn trace_classic_network_heap(_stage: &str) {}

#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
fn trace_classic_ap_config(_stage: &str) {}

#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
fn trace_classic_network_layout() {}

#[embassy_executor::task]
async fn ap_runner_task(mut runner: ApRunner) -> ! {
    assert_service_core("AP network runner");
    runner.run().await
}

#[embassy_executor::task]
async fn station_runner_task(mut runner: StationRunner) -> ! {
    assert_service_core("station network runner");
    runner.run().await
}

/// Places each HTTP future directly in its own task slot. Keeping the bounded
/// admissions outside the async network initializer also prevents their
/// spawn arguments from inflating its compiler stack frame.
#[inline(never)]
fn spawn_ap_http_workers(
    spawner: Spawner,
    stack: Stack<'static>,
    handler: AluminaHttpHandler,
    buffers: &'static TcpBuffers<HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>,
    headers: &'static mut [u8],
) {
    for (task_id, header) in headers.chunks_exact_mut(HTTP_HEADER_BYTES).enumerate() {
        spawner.spawn(
            http_worker_task(task_id, stack, buffers, header, handler)
                .expect("failed to allocate AP HTTP worker"),
        );
    }
}

/// The FOC target retains one connection until its hardware memory profile is
/// measured. TinyBee publishes two workers; S3 targets retain three.
#[cfg(feature = "board-mks-esp32-foc-v1")]
#[embassy_executor::task]
async fn http_worker_task(
    task_id: usize,
    stack: Stack<'static>,
    buffers: &'static TcpBuffers<HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>,
    header_buffer: &'static mut [u8],
    handler: AluminaHttpHandler,
) -> ! {
    http_worker_loop(task_id, stack, buffers, header_buffer, handler).await
}

#[cfg(any(feature = "board-mks-tinybee", feature = "board-mks-tinybee-4mb"))]
#[embassy_executor::task(pool_size = 2)]
async fn http_worker_task(
    task_id: usize,
    stack: Stack<'static>,
    buffers: &'static TcpBuffers<HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>,
    header_buffer: &'static mut [u8],
    handler: AluminaHttpHandler,
) -> ! {
    http_worker_loop(task_id, stack, buffers, header_buffer, handler).await
}

#[cfg(any(feature = "board-t-deck-pro", feature = "board-t-lora-pager"))]
#[embassy_executor::task(pool_size = 3)]
async fn http_worker_task(
    task_id: usize,
    stack: Stack<'static>,
    buffers: &'static TcpBuffers<HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>,
    header_buffer: &'static mut [u8],
    handler: AluminaHttpHandler,
) -> ! {
    http_worker_loop(task_id, stack, buffers, header_buffer, handler).await
}

/// One independently stored HTTP worker. Keeping each socket in its own task
/// avoids retaining connection futures in a parent polling frame.
async fn http_worker_loop<const CONNECTIONS: usize>(
    task_id: usize,
    stack: Stack<'static>,
    buffers: &'static TcpBuffers<CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>,
    header_buffer: &'static mut [u8],
    handler: AluminaHttpHandler,
) -> ! {
    assert_service_core("HTTP service");
    poll_boundary(stack.wait_config_up()).await;

    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 80);
    let tcp = Tcp::new(stack, buffers);
    let acceptor = match poll_boundary(tcp.bind(address)).await {
        Ok(acceptor) => acceptor,
        Err(_) => panic!("HTTP worker failed to bind"),
    };
    let acceptor = WithTimeout::new(WebLimits::INITIAL.io_timeout_ms, acceptor);
    let handler = RouteAwareTimeout::new(
        WebLimits::INITIAL.request_timeout_ms,
        WebLimits::INITIAL.asset_transfer_timeout_ms,
        handler,
    );

    loop {
        let socket = match poll_boundary(acceptor.accept()).await {
            Ok((_, socket)) => socket,
            Err(WithTimeoutError::Timeout) => continue,
            Err(_) => {
                error!("HTTP accept failed");
                Timer::after(Duration::from_millis(250)).await;
                continue;
            }
        };
        poll_boundary(handle_connection::<_, _, HTTP_HEADER_COUNT>(
            socket,
            header_buffer,
            Some(WebLimits::INITIAL.keepalive_timeout_ms),
            task_id,
            &handler,
        ))
        .await;
    }
}

/// One infrastructure-side API listener. The browser application is loaded
/// through the two-listener recovery AP before association, then can verify and
/// use the station address without transferring the large bundle again.
#[embassy_executor::task]
async fn station_http_worker_task(
    stack: Stack<'static>,
    buffers: &'static TcpBuffers<STATION_HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>,
    header_buffer: &'static mut [u8; HTTP_HEADER_BYTES],
    handler: AluminaHttpHandler,
) -> ! {
    http_worker_loop(HTTP_CONNECTIONS, stack, buffers, header_buffer, handler).await
}

/// Retains the short API deadline while permitting bounded forward progress
/// for the content-addressed browser bundle. Socket operations retain their
/// independent, much shorter I/O timeout, so a stalled transfer still releases
/// its connection promptly.
struct RouteAwareTimeout<H> {
    request_timeout_ms: u32,
    asset_transfer_timeout_ms: u32,
    handler: H,
}

impl<H> RouteAwareTimeout<H> {
    const fn new(request_timeout_ms: u32, asset_transfer_timeout_ms: u32, handler: H) -> Self {
        Self {
            request_timeout_ms,
            asset_transfer_timeout_ms,
            handler,
        }
    }
}

impl<H> Handler for RouteAwareTimeout<H>
where
    H: Handler,
{
    type Error<E>
        = WithTimeoutError<H::Error<E>>
    where
        E: Debug;

    async fn handle<T, const N: usize>(
        &self,
        task_id: impl Display + Copy,
        connection: &mut Connection<'_, T, N>,
    ) -> Result<(), Self::Error<T::Error>>
    where
        T: Read + Write + TcpSplit,
    {
        let is_asset = connection
            .headers()
            .ok()
            .and_then(|headers| web_asset(headers.path))
            .is_some();
        let timeout_ms = if is_asset {
            self.asset_transfer_timeout_ms
        } else {
            self.request_timeout_ms
        };
        with_timeout(
            timeout_ms,
            poll_boundary(self.handler.handle(task_id, connection)),
        )
        .await
    }
}

#[embassy_executor::task]
async fn dhcp_task(
    stack: Stack<'static>,
    buffers: &'static UdpBuffers<1, DHCP_PACKET_BYTES, DHCP_PACKET_BYTES, DHCP_METADATA_SLOTS>,
    work: &'static mut [u8],
) -> ! {
    assert_service_core("DHCP service");
    stack.wait_config_up().await;

    let udp = Udp::new(stack, buffers);
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_SERVER_PORT);
    let mut socket = match udp.bind(address).await {
        Ok(socket) => socket,
        Err(_) => panic!("DHCP socket bind failed"),
    };
    let mut gateway = [Ipv4Addr::UNSPECIFIED];
    let dns = [AP_IP];
    let mut options = ServerOptions::new(AP_IP, Some(&mut gateway));
    options.dns = &dns;
    options.captive_url = Some("http://192.168.4.1/");
    let mut server = DhcpServer::<_, DHCP_LEASES>::new(|| Instant::now().as_secs(), AP_IP);
    server.range_start = DHCP_FIRST;
    server.range_end = DHCP_LAST;

    loop {
        if dhcp_io::run(&mut server, &options, &mut socket, work)
            .await
            .is_err()
        {
            error!("DHCP service exited; restarting");
        }
        Timer::after(Duration::from_millis(250)).await;
    }
}

/// Wildcard IPv4 DNS on the isolated AP. It neither forwards nor recurses;
/// every valid A/IN question points at the zero-TTL recovery page.
#[embassy_executor::task]
async fn captive_dns_task(
    stack: Stack<'static>,
    buffers: &'static UdpBuffers<
        1,
        CAPTIVE_DNS_PACKET_BYTES,
        CAPTIVE_DNS_PACKET_BYTES,
        DNS_METADATA_SLOTS,
    >,
    work: &'static mut [u8; CAPTIVE_DNS_PACKET_BYTES],
) -> ! {
    assert_service_core("captive DNS service");
    stack.wait_config_up().await;

    let udp = Udp::new(stack, buffers);
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), CAPTIVE_DNS_PORT);
    let mut socket = match udp.bind(address).await {
        Ok(socket) => socket,
        Err(_) => panic!("captive DNS socket bind failed"),
    };

    loop {
        match socket.receive(work).await {
            Ok((received, remote)) => {
                if let Ok(response_len) =
                    build_captive_dns_reply(work, received, PROVISIONING_ADDRESS)
                    && socket.send(remote, &work[..response_len]).await.is_err()
                {
                    warn!("captive DNS response failed");
                }
            }
            Err(_) => {
                error!("captive DNS receive failed");
                Timer::after(Duration::from_millis(250)).await;
            }
        }
    }
}

#[derive(Clone, Copy)]
struct AluminaHttpHandler {
    credential_source: CredentialSource,
    auth_nonce: BootNonce,
    auth_state: &'static AuthState,
    service_bridge: &'static ServiceBridge,
    capability_identity: CapabilityIdentity,
    device_id: DeviceId,
}

impl Handler for AluminaHttpHandler {
    type Error<E>
        = HttpError<E>
    where
        E: Debug;

    async fn handle<T, const N: usize>(
        &self,
        _task_id: impl Display + Copy,
        connection: &mut Connection<'_, T, N>,
    ) -> Result<(), Self::Error<T::Error>>
    where
        T: Read + Write + TcpSplit,
    {
        let (method, route, asset_request) = {
            let headers = connection.headers()?;
            let method = match headers.method {
                Method::Get => HttpMethod::Get,
                Method::Post => HttpMethod::Post,
                Method::Put => HttpMethod::Put,
                Method::Delete => HttpMethod::Delete,
                Method::Options => HttpMethod::Options,
                _ => HttpMethod::Other,
            };
            (
                method,
                classify_route(method, headers.path),
                embedded_asset_request(headers, method),
            )
        };

        if let Some(asset_request) = asset_request {
            match asset_request {
                EmbeddedAssetRequest::Serve(asset) => {
                    write_embedded_asset(connection, asset).await?;
                }
                EmbeddedAssetRequest::MethodNotAllowed => {
                    asset_method_not_allowed(connection).await?;
                }
            }
            return Ok(());
        }

        if route == Route::CorsPreflight {
            let preflight = match cors_preflight_metadata(connection.headers()?) {
                Ok(preflight) => preflight,
                Err(rejection) => {
                    reject_request(connection, rejection, None).await?;
                    return Ok(());
                }
            };
            write_cors_preflight(connection, preflight).await?;
            return Ok(());
        }

        let request_origin = match request_origin(connection.headers()?) {
            Ok(origin) => origin,
            Err(rejection) => {
                reject_request(connection, rejection, None).await?;
                return Ok(());
            }
        };

        if route.requires_authentication() {
            handle_authenticated_route(self, connection, request_origin, method, route).await?;
            return Ok(());
        }

        match route {
            Route::Bootstrap => {
                connection
                    .initiate_response(
                        200,
                        Some("OK"),
                        &[
                            CONNECTION_CLOSE_HEADER,
                            ("Content-Type", "text/html; charset=utf-8"),
                            ("Cache-Control", "no-store"),
                            ("X-Content-Type-Options", "nosniff"),
                            (
                                "Content-Security-Policy",
                                "default-src 'none'; style-src 'unsafe-inline'",
                            ),
                        ],
                    )
                    .await?;
                connection
                    .write_all(
                        b"<!doctype html><meta charset=utf-8><title>Alumina</title>\
                          <h1>Alumina firmware</h1><p>Bounded network bootstrap is active.</p>\
                          <p><a href=/api/v1/identity>Device identity</a></p>",
                    )
                    .await?;
            }
            Route::Identity => {
                json_response(connection, request_origin).await?;
                connection
                    .write_all(b"{\"protocol_version\":1,\"board_id\":\"")
                    .await?;
                connection
                    .write_all(env!("ALUMINA_BOARD_ID").as_bytes())
                    .await?;
                connection.write_all(b"\",\"credential_source\":\"").await?;
                connection
                    .write_all(self.credential_source.label().as_bytes())
                    .await?;
                connection.write_all(b"\",\"production_armable\":").await?;
                connection
                    .write_all(if self.credential_source.production_armable() {
                        b"true"
                    } else {
                        b"false"
                    })
                    .await?;
                connection.write_all(b",\"device_id\":\"").await?;
                let mut device_id = [0_u8; 32];
                if write_lower_hex(&self.device_id.0, &mut device_id).is_err() {
                    panic!("device identity encoding failed");
                }
                connection.write_all(&device_id).await?;
                connection.write_all(b"\",\"capability_digest\":\"").await?;
                let mut digest = [0_u8; 64];
                if write_lower_hex(&self.capability_identity.digest.0, &mut digest).is_err() {
                    panic!("capability digest encoding failed");
                }
                connection.write_all(&digest).await?;
                let mut bundle_digest = [0_u8; 64];
                if write_lower_hex(&WEB_BUNDLE_DIGEST.0, &mut bundle_digest).is_err() {
                    panic!("interface bundle digest encoding failed");
                }
                let bundle_digest =
                    core::str::from_utf8(&bundle_digest).expect("lowercase hex is UTF-8");
                let mut tail = FixedString::<256>::new();
                write!(
                    tail,
                    "\",\"capability_document_bytes\":{},\"interface_bundle_format\":\"{}\",\"interface_commit\":\"{}\",\"interface_bundle_sha256\":\"{}\"}}",
                    self.capability_identity.byte_len,
                    WEB_BUNDLE_FORMAT,
                    INTERFACE_COMMIT,
                    bundle_digest,
                )
                .unwrap_or_else(|_| panic!("identity response exceeded its fixed suffix"));
                connection.write_all(tail.as_bytes()).await?;
            }
            Route::Health => {
                json_response(connection, request_origin).await?;
                connection
                    .write_all(b"{\"service_core\":0,\"network\":\"ap\",\"state\":\"boot\"}")
                    .await?;
            }
            Route::Network => {
                json_response(connection, request_origin).await?;
                connection
                    .write_all(
                        b"{\"mode\":\"access-point-plus-station\",\"address\":\"192.168.4.1\",\
                          \"scan_join_available\":true,\"control\":\"authenticated-native\",\
                          \"ap_preserved\":true,\"credentials_durable\":false}",
                    )
                    .await?;
            }
            Route::Authentication => {
                let nonce = self.auth_nonce.as_bytes();
                let mut encoded = [0_u8; AUTH_NONCE_BYTES * 2];
                if write_lower_hex(&nonce, &mut encoded).is_err() {
                    panic!("authentication nonce encoding failed");
                }
                let encoded = core::str::from_utf8(&encoded).expect("lowercase hex is UTF-8");
                let mut body = FixedString::<AUTH_DISCOVERY_BODY_BYTES>::new();
                write!(
                    &mut body,
                    "{{\"scheme\":\"hmac-sha256-v2\",\"origin_bound\":true,\"boot_nonce\":\"{encoded}\",\"counter_window\":64,\"rate_burst\":32,\"rate_per_second\":50,\"request_proof_header\":\"X-Alumina-Authorization\",\"response_proof_header\":\"X-Alumina-Response-Authorization\"}}"
                )
                .unwrap_or_else(|_| panic!("authentication response exceeded its fixed body"));
                if body.len() != AUTH_DISCOVERY_BODY_BYTES {
                    panic!("authentication response length changed without protocol update");
                }
                exact_json_response(connection, request_origin, body.as_bytes()).await?;
            }
            Route::StorageStatus | Route::ControlCommand | Route::CorsPreflight => unreachable!(),
            Route::MethodNotAllowed => {
                connection
                    .initiate_response(
                        405,
                        Some("Method Not Allowed"),
                        &[
                            CONNECTION_CLOSE_HEADER,
                            ("Allow", "GET"),
                            ("Content-Type", "text/plain"),
                        ],
                    )
                    .await?;
                connection.write_all(b"method not allowed\n").await?;
            }
            Route::NotFound => {
                connection
                    .initiate_response(
                        404,
                        Some("Not Found"),
                        &[
                            CONNECTION_CLOSE_HEADER,
                            ("Content-Type", "text/plain"),
                            ("Cache-Control", "no-store"),
                        ],
                    )
                    .await?;
                connection.write_all(b"not found\n").await?;
            }
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EmbeddedAssetRequest {
    Serve(EmbeddedWebAsset),
    MethodNotAllowed,
}

fn embedded_asset_request<const N: usize>(
    headers: &RequestHeaders<'_, N>,
    method: HttpMethod,
) -> Option<EmbeddedAssetRequest> {
    let asset = web_asset(headers.path)?;
    if method != HttpMethod::Get {
        return Some(EmbeddedAssetRequest::MethodNotAllowed);
    }
    Some(EmbeddedAssetRequest::Serve(asset))
}

async fn write_embedded_asset<T, const N: usize>(
    connection: &mut Connection<'_, T, N>,
    asset: EmbeddedWebAsset,
) -> Result<(), HttpError<T::Error>>
where
    T: Read + Write,
{
    let mut length = FixedString::<10>::new();
    write!(&mut length, "{}", asset.wire_bytes())
        .unwrap_or_else(|_| panic!("embedded interface asset length exceeded u32 text"));
    let mut wire_digest = [0_u8; 64];
    if write_lower_hex(&asset.wire_digest().0, &mut wire_digest).is_err() {
        panic!("embedded interface wire digest encoding failed");
    }
    let wire_digest = core::str::from_utf8(&wire_digest).expect("lowercase hex is UTF-8");
    let mut source_digest = [0_u8; 64];
    if write_lower_hex(&asset.source_digest().0, &mut source_digest).is_err() {
        panic!("embedded interface source digest encoding failed");
    }
    let source_digest = core::str::from_utf8(&source_digest).expect("lowercase hex is UTF-8");
    let mut bundle_digest = [0_u8; 64];
    if write_lower_hex(&WEB_BUNDLE_DIGEST.0, &mut bundle_digest).is_err() {
        panic!("embedded interface bundle digest encoding failed");
    }
    let bundle_digest = core::str::from_utf8(&bundle_digest).expect("lowercase hex is UTF-8");
    let stored_representation = asset.stored_representation().label();

    match asset.path() {
        "/" => {
            connection
                .initiate_response(
                    200,
                    Some("OK"),
                    &[
                        CONNECTION_CLOSE_HEADER,
                        ("Content-Type", asset.media_type()),
                        ("Content-Length", length.as_str()),
                        ("Cache-Control", "no-store"),
                        ("X-Content-Type-Options", "nosniff"),
                        ("Content-Security-Policy", INTERFACE_CONTENT_SECURITY_POLICY),
                        ("Cross-Origin-Resource-Policy", "same-origin"),
                        ("X-Alumina-Interface-Commit", INTERFACE_COMMIT),
                        ("X-Alumina-Bundle-SHA256", bundle_digest),
                        ("X-Alumina-Source-SHA256", source_digest),
                        ("X-Alumina-Wire-SHA256", wire_digest),
                        ("X-Alumina-Stored-Representation", stored_representation),
                    ],
                )
                .await?;
        }
        _ => {
            connection
                .initiate_response(
                    200,
                    Some("OK"),
                    &[
                        CONNECTION_CLOSE_HEADER,
                        ("Content-Type", asset.media_type()),
                        ("Content-Length", length.as_str()),
                        ("Cache-Control", "no-store"),
                        ("X-Content-Type-Options", "nosniff"),
                        ("Cross-Origin-Resource-Policy", "same-origin"),
                        ("X-Alumina-Interface-Commit", INTERFACE_COMMIT),
                        ("X-Alumina-Bundle-SHA256", bundle_digest),
                        ("X-Alumina-Source-SHA256", source_digest),
                        ("X-Alumina-Wire-SHA256", wire_digest),
                        ("X-Alumina-Stored-Representation", stored_representation),
                    ],
                )
                .await?;
        }
    }
    connection.write_all(asset.bytes()).await
}

async fn asset_method_not_allowed<T, const N: usize>(
    connection: &mut Connection<'_, T, N>,
) -> Result<(), HttpError<T::Error>>
where
    T: Read + Write,
{
    connection
        .initiate_response(
            405,
            Some("Method Not Allowed"),
            &[
                CONNECTION_CLOSE_HEADER,
                ("Allow", "GET"),
                ("Content-Type", "text/plain"),
            ],
        )
        .await?;
    connection.write_all(b"method not allowed\n").await
}

type AuthenticatedRequestAdmission =
    Result<(AuthenticatedRequestMetadata, ServiceRequest), RequestRejection>;

/// Complete an authenticated route without retaining its body scratch space
/// during the later inter-core transaction.
async fn handle_authenticated_route<T, const N: usize>(
    handler: &AluminaHttpHandler,
    connection: &mut Connection<'_, T, N>,
    request_origin: Option<CorsOrigin>,
    method: HttpMethod,
    route: Route,
) -> Result<(), HttpError<T::Error>>
where
    T: Read + Write,
{
    let (metadata, request) =
        match read_and_authorize_request(handler, connection, method, route).await? {
            Ok(admitted) => admitted,
            Err(rejection) => {
                reject_request(connection, rejection, request_origin).await?;
                return Ok(());
            }
        };
    let response = handler.service_bridge.transact(request).await;
    write_authenticated_response(
        connection,
        AP_PASSPHRASE.as_bytes(),
        handler.auth_nonce,
        metadata.proof.counter,
        metadata.origin,
        &response,
    )
    .await
}

/// Read, authenticate, and decode one bounded request as a complete phase.
///
/// Returning the owned service request ends the body-array lifetime before the
/// bridge can await. Embassy can therefore reuse this phase's fixed storage
/// while preserving the exact route and authentication limits.
async fn read_and_authorize_request<T, const N: usize>(
    handler: &AluminaHttpHandler,
    connection: &mut Connection<'_, T, N>,
    method: HttpMethod,
    route: Route,
) -> Result<AuthenticatedRequestAdmission, HttpError<T::Error>>
where
    T: Read + Write,
{
    let metadata = match authenticated_metadata(connection.headers()?, method, route) {
        Ok(metadata) => metadata,
        Err(rejection) => return Ok(Err(rejection)),
    };
    let mut body = [0_u8; MAX_AUTHENTICATED_BODY_BYTES];
    read_body_exact(connection, &mut body[..metadata.body_len]).await?;
    let body = &body[..metadata.body_len];
    let auth_result = handler.auth_state.lock(|state| {
        state.borrow_mut().authorize(
            AP_PASSPHRASE.as_bytes(),
            metadata,
            body,
            Instant::now().as_millis(),
        )
    });
    if let Err(error) = auth_result {
        return Ok(Err(if error == AuthError::RateLimited {
            RequestRejection::RateLimited
        } else {
            RequestRejection::Unauthorized
        }));
    }

    let request = match route {
        Route::StorageStatus => ServiceRequest::storage_status(),
        Route::ControlCommand => match ServiceRequest::native(body) {
            Ok(request) => request,
            Err(_) => return Ok(Err(RequestRejection::BodyTooLarge)),
        },
        _ => unreachable!(),
    };
    Ok(Ok((metadata, request)))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestRejection {
    BadRequest,
    Unauthorized,
    BodyTooLarge,
    UnsupportedMedia,
    RateLimited,
}

fn authenticated_metadata<const N: usize>(
    headers: &RequestHeaders<'_, N>,
    method: HttpMethod,
    route: Route,
) -> Result<AuthenticatedRequestMetadata, RequestRejection> {
    let mut accumulator = AuthHeaderAccumulator::new();
    for (candidate, value) in headers.headers.iter_raw() {
        accumulator
            .observe(candidate.as_bytes(), value)
            .map_err(map_http_admission_error)?;
    }
    accumulator
        .finish(method, route)
        .map_err(map_http_admission_error)
}

fn request_origin<const N: usize>(
    headers: &RequestHeaders<'_, N>,
) -> Result<Option<CorsOrigin>, RequestRejection> {
    let mut origin = None;
    for (candidate, value) in headers.headers.iter_raw() {
        if !candidate
            .as_bytes()
            .eq_ignore_ascii_case(CORS_ORIGIN_HEADER.as_bytes())
        {
            continue;
        }
        if origin.is_some() {
            return Err(RequestRejection::BadRequest);
        }
        let value = core::str::from_utf8(value).map_err(|_| RequestRejection::BadRequest)?;
        origin = Some(CorsOrigin::parse(value).map_err(|_| RequestRejection::BadRequest)?);
    }
    Ok(origin)
}

fn cors_preflight_metadata<const N: usize>(
    headers: &RequestHeaders<'_, N>,
) -> Result<CorsPreflight, RequestRejection> {
    let mut accumulator = CorsPreflightAccumulator::new();
    for (candidate, value) in headers.headers.iter_raw() {
        accumulator
            .observe(candidate.as_bytes(), value)
            .map_err(map_http_admission_error)?;
    }
    accumulator
        .finish(headers.path)
        .map_err(map_http_admission_error)
}

async fn write_cors_preflight<T, const N: usize>(
    connection: &mut Connection<'_, T, N>,
    preflight: CorsPreflight,
) -> Result<(), HttpError<T::Error>>
where
    T: Read + Write,
{
    let method = match preflight.target_method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        _ => panic!("preflight admitted an unsupported target method"),
    };
    let vary = "Origin, Access-Control-Request-Method, Access-Control-Request-Headers, Access-Control-Request-Private-Network";
    if preflight.private_network {
        connection
            .initiate_response(
                204,
                Some("No Content"),
                &[
                    CONNECTION_CLOSE_HEADER,
                    (CORS_ALLOW_ORIGIN_HEADER, preflight.origin.as_str()),
                    ("Access-Control-Allow-Methods", method),
                    (
                        "Access-Control-Allow-Headers",
                        "Content-Type, X-Alumina-Counter, X-Alumina-Authorization",
                    ),
                    ("Access-Control-Max-Age", "600"),
                    (CORS_ALLOW_PRIVATE_NETWORK_HEADER, "true"),
                    ("Vary", vary),
                    ("Content-Length", "0"),
                ],
            )
            .await
    } else {
        connection
            .initiate_response(
                204,
                Some("No Content"),
                &[
                    CONNECTION_CLOSE_HEADER,
                    (CORS_ALLOW_ORIGIN_HEADER, preflight.origin.as_str()),
                    ("Access-Control-Allow-Methods", method),
                    (
                        "Access-Control-Allow-Headers",
                        "Content-Type, X-Alumina-Counter, X-Alumina-Authorization",
                    ),
                    ("Access-Control-Max-Age", "600"),
                    ("Vary", vary),
                    ("Content-Length", "0"),
                ],
            )
            .await
    }
}

const fn map_http_admission_error(error: HttpAdmissionError) -> RequestRejection {
    match error {
        HttpAdmissionError::BodyTooLarge => RequestRejection::BodyTooLarge,
        HttpAdmissionError::ContentType => RequestRejection::UnsupportedMedia,
        HttpAdmissionError::Credentials => RequestRejection::Unauthorized,
        HttpAdmissionError::Origin => RequestRejection::BadRequest,
        HttpAdmissionError::CorsMethod
        | HttpAdmissionError::CorsHeaders
        | HttpAdmissionError::CorsPrivateNetwork => RequestRejection::BadRequest,
        HttpAdmissionError::DuplicateHeader
        | HttpAdmissionError::TransferEncoding
        | HttpAdmissionError::ContentLength
        | HttpAdmissionError::BodyRequired
        | HttpAdmissionError::Route => RequestRejection::BadRequest,
    }
}

async fn read_body_exact<T, const N: usize>(
    connection: &mut Connection<'_, T, N>,
    body: &mut [u8],
) -> Result<(), HttpError<T::Error>>
where
    T: Read + Write,
{
    let mut read = 0;
    while read < body.len() {
        let received = connection.read(&mut body[read..]).await?;
        if received == 0 {
            return Err(HttpError::IncompleteBody);
        }
        read += received;
    }
    Ok(())
}

async fn reject_request<T, const N: usize>(
    connection: &mut Connection<'_, T, N>,
    rejection: RequestRejection,
    origin: Option<CorsOrigin>,
) -> Result<(), HttpError<T::Error>>
where
    T: Read + Write,
{
    let (status, reason, body) = match rejection {
        RequestRejection::BadRequest => (400, "Bad Request", b"bad request\n".as_slice()),
        RequestRejection::Unauthorized => (401, "Unauthorized", b"unauthorized\n".as_slice()),
        RequestRejection::BodyTooLarge => {
            (413, "Content Too Large", b"body too large\n".as_slice())
        }
        RequestRejection::UnsupportedMedia => (
            415,
            "Unsupported Media Type",
            b"unsupported media type\n".as_slice(),
        ),
        RequestRejection::RateLimited => (429, "Too Many Requests", b"rate limited\n".as_slice()),
    };
    if let (Some(origin), RequestRejection::Unauthorized) = (origin, rejection) {
        connection
            .initiate_response(
                status,
                Some(reason),
                &[
                    CONNECTION_CLOSE_HEADER,
                    ("Content-Type", "text/plain"),
                    ("Cache-Control", "no-store"),
                    (CORS_ALLOW_ORIGIN_HEADER, origin.as_str()),
                    ("Vary", CORS_ORIGIN_HEADER),
                    ("WWW-Authenticate", "Alumina-HMAC-SHA256-V2"),
                ],
            )
            .await?;
    } else if let (Some(origin), RequestRejection::RateLimited) = (origin, rejection) {
        connection
            .initiate_response(
                status,
                Some(reason),
                &[
                    CONNECTION_CLOSE_HEADER,
                    ("Content-Type", "text/plain"),
                    ("Cache-Control", "no-store"),
                    (CORS_ALLOW_ORIGIN_HEADER, origin.as_str()),
                    ("Vary", CORS_ORIGIN_HEADER),
                    ("Retry-After", "1"),
                ],
            )
            .await?;
    } else if let Some(origin) = origin {
        connection
            .initiate_response(
                status,
                Some(reason),
                &[
                    CONNECTION_CLOSE_HEADER,
                    ("Content-Type", "text/plain"),
                    ("Cache-Control", "no-store"),
                    (CORS_ALLOW_ORIGIN_HEADER, origin.as_str()),
                    ("Vary", CORS_ORIGIN_HEADER),
                ],
            )
            .await?;
    } else if rejection == RequestRejection::Unauthorized {
        connection
            .initiate_response(
                status,
                Some(reason),
                &[
                    CONNECTION_CLOSE_HEADER,
                    ("Content-Type", "text/plain"),
                    ("Cache-Control", "no-store"),
                    ("WWW-Authenticate", "Alumina-HMAC-SHA256-V2"),
                ],
            )
            .await?;
    } else if rejection == RequestRejection::RateLimited {
        connection
            .initiate_response(
                status,
                Some(reason),
                &[
                    CONNECTION_CLOSE_HEADER,
                    ("Content-Type", "text/plain"),
                    ("Cache-Control", "no-store"),
                    ("Retry-After", "1"),
                ],
            )
            .await?;
    } else {
        connection
            .initiate_response(
                status,
                Some(reason),
                &[
                    CONNECTION_CLOSE_HEADER,
                    ("Content-Type", "text/plain"),
                    ("Cache-Control", "no-store"),
                ],
            )
            .await?;
    }
    connection.write_all(body).await
}

async fn write_authenticated_response<T, const N: usize>(
    connection: &mut Connection<'_, T, N>,
    secret: &[u8],
    nonce: BootNonce,
    counter: u64,
    origin: CorsOrigin,
    response: &ServiceResponse,
) -> Result<(), HttpError<T::Error>>
where
    T: Read + Write,
{
    let (media, content_type) = match response.media {
        ResponseMedia::Json => (AuthenticatedMedia::Json, "application/json"),
        ResponseMedia::NativeFrame => (
            AuthenticatedMedia::NativeFrame,
            "application/vnd.alumina.frame",
        ),
    };
    let proof = match sign_response(
        secret,
        nonce,
        counter,
        response.http_status,
        media,
        origin,
        response.bytes(),
    ) {
        Ok(proof) => proof,
        Err(_) => panic!("authenticated response signing failed"),
    };
    let mut tag = [0_u8; AUTH_TAG_HEX_BYTES];
    if write_lower_hex(&proof.tag, &mut tag).is_err() {
        panic!("response proof encoding failed");
    }
    let tag = core::str::from_utf8(&tag).expect("lowercase hex is UTF-8");
    let mut counter_text = FixedString::<20>::new();
    write!(&mut counter_text, "{}", counter).expect("counter text capacity is exact");
    let mut length_text = FixedString::<20>::new();
    write!(&mut length_text, "{}", response.bytes().len())
        .expect("content length text capacity is exact");
    let reason = match response.http_status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Service Response",
    };
    connection
        .initiate_response(
            response.http_status,
            Some(reason),
            &[
                CONNECTION_CLOSE_HEADER,
                ("Content-Type", content_type),
                ("Content-Length", length_text.as_str()),
                ("Cache-Control", "no-store"),
                ("X-Content-Type-Options", "nosniff"),
                (AUTH_COUNTER_HEADER, counter_text.as_str()),
                (AUTH_RESPONSE_HEADER, tag),
                (CORS_ALLOW_ORIGIN_HEADER, origin.as_str()),
                (
                    "Access-Control-Expose-Headers",
                    "X-Alumina-Counter, X-Alumina-Response-Authorization",
                ),
                ("Vary", CORS_ORIGIN_HEADER),
            ],
        )
        .await?;
    connection.write_all(response.bytes()).await
}

async fn json_response<T, const N: usize>(
    connection: &mut Connection<'_, T, N>,
    origin: Option<CorsOrigin>,
) -> Result<(), HttpError<T::Error>>
where
    T: Read + Write,
{
    if let Some(origin) = origin {
        connection
            .initiate_response(
                200,
                Some("OK"),
                &[
                    CONNECTION_CLOSE_HEADER,
                    ("Content-Type", "application/json"),
                    ("Cache-Control", "no-store"),
                    ("X-Content-Type-Options", "nosniff"),
                    (CORS_ALLOW_ORIGIN_HEADER, origin.as_str()),
                    ("Vary", CORS_ORIGIN_HEADER),
                ],
            )
            .await
    } else {
        connection
            .initiate_response(
                200,
                Some("OK"),
                &[
                    CONNECTION_CLOSE_HEADER,
                    ("Content-Type", "application/json"),
                    ("Cache-Control", "no-store"),
                    ("X-Content-Type-Options", "nosniff"),
                ],
            )
            .await
    }
}

async fn exact_json_response<T, const N: usize>(
    connection: &mut Connection<'_, T, N>,
    origin: Option<CorsOrigin>,
    body: &[u8],
) -> Result<(), HttpError<T::Error>>
where
    T: Read + Write,
{
    let mut length = FixedString::<20>::new();
    write!(&mut length, "{}", body.len()).expect("content length text capacity is exact");
    if body.len() == AUTH_DISCOVERY_BODY_BYTES && length.as_str() != AUTH_DISCOVERY_CONTENT_LENGTH {
        panic!("authentication content length constant is inconsistent");
    }
    if let Some(origin) = origin {
        connection
            .initiate_response(
                200,
                Some("OK"),
                &[
                    CONNECTION_CLOSE_HEADER,
                    ("Content-Type", "application/json"),
                    ("Content-Length", length.as_str()),
                    ("Cache-Control", "no-store"),
                    ("X-Content-Type-Options", "nosniff"),
                    (CORS_ALLOW_ORIGIN_HEADER, origin.as_str()),
                    ("Vary", CORS_ORIGIN_HEADER),
                ],
            )
            .await?;
    } else {
        connection
            .initiate_response(
                200,
                Some("OK"),
                &[
                    CONNECTION_CLOSE_HEADER,
                    ("Content-Type", "application/json"),
                    ("Content-Length", length.as_str()),
                    ("Cache-Control", "no-store"),
                    ("X-Content-Type-Options", "nosniff"),
                ],
            )
            .await?;
    }
    connection.write_all(body).await
}

fn assert_service_core(component: &str) {
    if Cpu::current() != Cpu::ProCpu {
        error!("{} started outside service core", component);
        panic!("network component started on the wrong core");
    }
}
