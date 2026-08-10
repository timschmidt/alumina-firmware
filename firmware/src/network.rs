//! Core-0-only ESP radio, static IPv4, DHCP, and bounded HTTP adapter.

extern crate alloc;

use alloc::string::String;
use core::fmt::{Debug, Display};
use core::net::{IpAddr, Ipv4Addr, SocketAddr};

use alumina_net::{
    AccessPointProfile, CredentialSource, DHCP_RANGE_END, DHCP_RANGE_START, HttpMethod,
    NetworkSupervisor, PROVISIONING_ADDRESS, PROVISIONING_PREFIX, Route, WebLimits, classify_route,
};
use defmt::{error, info, warn};
use edge_dhcp::io::{DEFAULT_SERVER_PORT, server as dhcp_io};
use edge_dhcp::server::{Server as DhcpServer, ServerOptions};
use edge_http::Method;
use edge_http::io::Error as HttpError;
use edge_http::io::server::{Connection, Handler, Server as HttpServer};
use edge_nal::{TcpBind, TcpSplit, UdpBind, WithTimeout};
use edge_nal_embassy::{Tcp, TcpBuffers, Udp, UdpBuffers};
use embassy_executor::Spawner;
use embassy_net::{
    Config as NetworkConfig, Ipv4Address, Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4,
};
use embassy_time::{Duration, Instant, Timer};
use embedded_io_async_v06::{Read, Write};
use esp_hal::peripherals::WIFI;
use esp_hal::rng::Rng;
use esp_hal::system::Cpu;
use esp_radio::wifi::{
    AccessPointConfig, AuthMethod, Config as RadioConfig, ModeConfig, WifiController, WifiDevice,
};
use static_cell::StaticCell;

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

const HTTP_CONNECTIONS: usize = WebLimits::INITIAL.connections;
const HTTP_HEADER_BYTES: usize = WebLimits::INITIAL.header_bytes;
const HTTP_HEADER_COUNT: usize = WebLimits::INITIAL.header_count;
const HTTP_SOCKET_BYTES: usize = WebLimits::INITIAL.socket_bytes;
const STACK_SOCKETS: usize = HTTP_CONNECTIONS + 2;
const DHCP_PACKET_BYTES: usize = 1_500;
const DHCP_METADATA_SLOTS: usize = 2;
const DHCP_LEASES: usize = 4;

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

type ApRunner = Runner<'static, WifiDevice<'static>>;

static RADIO: StaticCell<esp_radio::Controller<'static>> = StaticCell::new();
static AP_STACK_RESOURCES: StaticCell<StackResources<STACK_SOCKETS>> = StaticCell::new();
static HTTP_TCP_BUFFERS: StaticCell<
    TcpBuffers<HTTP_CONNECTIONS, HTTP_SOCKET_BYTES, HTTP_SOCKET_BYTES>,
> = StaticCell::new();
static DHCP_UDP_BUFFERS: StaticCell<
    UdpBuffers<1, DHCP_PACKET_BYTES, DHCP_PACKET_BYTES, DHCP_METADATA_SLOTS>,
> = StaticCell::new();
static DHCP_WORK_BUFFER: StaticCell<[u8; DHCP_PACKET_BYTES]> = StaticCell::new();

/// Controller state retained by the core-0 service task for later AP/STA changes.
#[allow(
    dead_code,
    reason = "station and controller are retained for the next provisioning slice"
)]
pub struct NetworkControl {
    controller: WifiController<'static>,
    station: WifiDevice<'static>,
    supervisor: NetworkSupervisor,
    credential_source: CredentialSource,
}

impl NetworkControl {
    /// Current portable supervision state.
    pub const fn supervisor(&self) -> NetworkSupervisor {
        self.supervisor
    }

    /// Credential provenance without credential material.
    pub const fn credential_source(&self) -> CredentialSource {
        self.credential_source
    }
}

/// Initializes the radio on core 0 and starts all AP service tasks on its executor.
pub async fn start(spawner: Spawner, wifi: WIFI<'static>) -> NetworkControl {
    assert_service_core("network initialization");

    let profile = AccessPointProfile::new(AP_SSID, AP_PASSPHRASE, CREDENTIAL_SOURCE);
    if profile.validate().is_err() || WebLimits::INITIAL.validate().is_err() {
        panic!("invalid compile-time Wi-Fi/web policy");
    }
    if CREDENTIAL_SOURCE == CredentialSource::DevelopmentFallback {
        warn!("development AP credential active; image is not production-armable");
    }

    let radio = match esp_radio::init() {
        Ok(radio) => RADIO.init(radio),
        Err(_) => panic!("esp-radio initialization failed"),
    };
    let (mut controller, interfaces) =
        match esp_radio::wifi::new(radio, wifi, RadioConfig::default()) {
            Ok(parts) => parts,
            Err(_) => panic!("Wi-Fi controller initialization failed"),
        };

    let access_point = AccessPointConfig::default()
        .with_ssid(String::from(profile.ssid))
        .with_password(String::from(profile.passphrase))
        .with_auth_method(AuthMethod::Wpa2Personal)
        .with_channel(profile.channel)
        .with_max_connections(u16::from(profile.maximum_clients));
    if controller
        .set_config(&ModeConfig::AccessPoint(access_point))
        .is_err()
    {
        panic!("Wi-Fi AP configuration failed");
    }

    let mut supervisor = NetworkSupervisor::new();
    if supervisor.begin_access_point().is_err() || controller.start_async().await.is_err() {
        panic!("Wi-Fi AP failed to start");
    }

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
    let seed = (u64::from(rng.random()) << 32) | u64::from(rng.random());
    let (stack, runner) = embassy_net::new(
        interfaces.ap,
        stack_config,
        AP_STACK_RESOURCES.init(StackResources::new()),
        seed,
    );

    spawner.must_spawn(ap_runner_task(runner));
    spawner.must_spawn(http_task(stack, CREDENTIAL_SOURCE));
    spawner.must_spawn(dhcp_task(stack));
    if supervisor.access_point_ready().is_err() {
        panic!("invalid AP supervision transition");
    }

    info!("protected Alumina AP started: {}", AP_SSID);
    NetworkControl {
        controller,
        station: interfaces.sta,
        supervisor,
        credential_source: CREDENTIAL_SOURCE,
    }
}

#[embassy_executor::task]
async fn ap_runner_task(mut runner: ApRunner) -> ! {
    assert_service_core("AP network runner");
    runner.run().await
}

#[embassy_executor::task]
async fn http_task(stack: Stack<'static>, credential_source: CredentialSource) -> ! {
    assert_service_core("HTTP service");
    stack.wait_config_up().await;

    let buffers = HTTP_TCP_BUFFERS.init(TcpBuffers::new());
    let mut server = HttpServer::<HTTP_CONNECTIONS, HTTP_HEADER_BYTES, HTTP_HEADER_COUNT>::new();
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 80);

    loop {
        let tcp = Tcp::new(stack, buffers);
        let acceptor = match tcp.bind(address).await {
            Ok(acceptor) => acceptor,
            Err(_) => {
                error!("HTTP bind failed");
                Timer::after(Duration::from_millis(250)).await;
                continue;
            }
        };
        let acceptor = WithTimeout::new(WebLimits::INITIAL.io_timeout_ms, acceptor);
        let handler = WithTimeout::new(
            WebLimits::INITIAL.request_timeout_ms,
            AluminaHttpHandler { credential_source },
        );

        if server
            .run(
                Some(WebLimits::INITIAL.keepalive_timeout_ms),
                acceptor,
                handler,
            )
            .await
            .is_err()
        {
            error!("bounded HTTP server exited; restarting");
        }
        Timer::after(Duration::from_millis(250)).await;
    }
}

#[embassy_executor::task]
async fn dhcp_task(stack: Stack<'static>) -> ! {
    assert_service_core("DHCP service");
    stack.wait_config_up().await;

    let buffers = DHCP_UDP_BUFFERS.init(UdpBuffers::new());
    let udp = Udp::new(stack, buffers);
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_SERVER_PORT);
    let mut socket = match udp.bind(address).await {
        Ok(socket) => socket,
        Err(_) => panic!("DHCP socket bind failed"),
    };
    let work = DHCP_WORK_BUFFER.init([0; DHCP_PACKET_BYTES]);
    let mut gateway = [Ipv4Addr::UNSPECIFIED];
    let mut options = ServerOptions::new(AP_IP, Some(&mut gateway));
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

#[derive(Clone, Copy)]
struct AluminaHttpHandler {
    credential_source: CredentialSource,
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
        let headers = connection.headers()?;
        let method = match headers.method {
            Method::Get => HttpMethod::Get,
            Method::Post => HttpMethod::Post,
            Method::Put => HttpMethod::Put,
            _ => HttpMethod::Other,
        };

        match classify_route(method, headers.path) {
            Route::Bootstrap => {
                connection
                    .initiate_response(
                        200,
                        Some("OK"),
                        &[
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
                json_response(connection).await?;
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
                connection
                    .write_all(if self.credential_source.production_armable() {
                        b"\",\"production_armable\":true}"
                    } else {
                        b"\",\"production_armable\":false}"
                    })
                    .await?;
            }
            Route::Health => {
                json_response(connection).await?;
                connection
                    .write_all(b"{\"service_core\":0,\"network\":\"ap\",\"state\":\"boot\"}")
                    .await?;
            }
            Route::Network => {
                json_response(connection).await?;
                connection
                    .write_all(
                        b"{\"mode\":\"access-point\",\"address\":\"192.168.4.1\",\
                          \"scan_join_available\":false}",
                    )
                    .await?;
            }
            Route::MethodNotAllowed => {
                connection
                    .initiate_response(
                        405,
                        Some("Method Not Allowed"),
                        &[("Allow", "GET"), ("Content-Type", "text/plain")],
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

async fn json_response<T, const N: usize>(
    connection: &mut Connection<'_, T, N>,
) -> Result<(), HttpError<T::Error>>
where
    T: Read + Write,
{
    connection
        .initiate_response(
            200,
            Some("OK"),
            &[
                ("Content-Type", "application/json"),
                ("Cache-Control", "no-store"),
                ("X-Content-Type-Options", "nosniff"),
            ],
        )
        .await
}

fn assert_service_core(component: &str) {
    if Cpu::current() != Cpu::ProCpu {
        error!("{} started outside service core", component);
        panic!("network component started on the wrong core");
    }
}
