use anyhow::{Result, anyhow};
use embedded_svc::{
    http::Method,
    io::Write,
    wifi::{AccessPointConfiguration, AuthMethod, Configuration as WifiConfiguration},
};
use esp_idf_hal::{gpio::PinDriver, modem::Modem, peripherals::Peripherals};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    http::server::{Configuration, EspHttpServer},
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, EspWifi},
};
use esp_idf_sys::esp_timer_get_time;
use std::{
    sync::{Arc, Mutex},
    thread::sleep,
    time::Duration,
};

// Importing the crate activates the startup symbols supplied by its `binstart` feature.
use esp_idf_sys as _;

pub mod commandbuffer;
pub mod devices;
pub mod interrupts;
pub mod peripherals;
pub mod planner;
pub mod serial;
pub mod wifi;

use crate::planner::Planner;

const BLOCK_BUFFER_SIZE: usize = 20;
const WIFI_SSID: &str = "Alumina";
const WIFI_PSK: &str = "";

const UI_INDEX: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../alumina-interface/dist/index.html"
));
const UI_JAVASCRIPT: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../alumina-interface/dist/alumina-ui.js.gz"
));
const UI_WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../alumina-interface/dist/alumina-ui_bg.wasm.br"
));
const UI_FAVICON: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../alumina-interface/dist/favicon.ico"
));

/// Starts an ESP32 access point and waits for its network interface to become ready.
fn start_access_point(
    ssid: &str,
    password: &str,
    modem: Modem,
    system_event_loop: EspSystemEventLoop,
) -> Result<BlockingWifi<EspWifi<'static>>> {
    let nvs = EspDefaultNvsPartition::take()?;
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, system_event_loop.clone(), Some(nvs))?,
        system_event_loop,
    )?;

    let ssid = ssid
        .try_into()
        .map_err(|_| anyhow!("SSID exceeds the 32-byte limit"))?;
    let ap_password = password
        .try_into()
        .map_err(|_| anyhow!("Wi-Fi password exceeds the 64-byte limit"))?;
    let auth_method = if password.is_empty() {
        AuthMethod::None
    } else {
        if password.len() < 8 {
            anyhow::bail!("WPA2 passwords must contain at least eight bytes");
        }
        AuthMethod::WPA2Personal
    };

    wifi.set_configuration(&WifiConfiguration::AccessPoint(AccessPointConfiguration {
        ssid,
        password: ap_password,
        auth_method,
        ssid_hidden: false,
        channel: 11,
        max_connections: 4,
        ..Default::default()
    }))?;
    wifi.start()?;
    wifi.wait_netif_up()?;

    log::info!(
        "Alumina access point is available at {:?}",
        wifi.wifi().ap_netif().get_ip_info()?
    );
    Ok(wifi)
}

fn main() -> Result<()> {
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let system_event_loop = EspSystemEventLoop::take()?;
    let _wifi = start_access_point(WIFI_SSID, WIFI_PSK, peripherals.modem, system_event_loop)?;

    let planner = Arc::new(Mutex::new(Planner::new(BLOCK_BUFFER_SIZE)));

    // These logical names are the pin labels currently exposed by the web interface.
    let d0_main = Arc::new(Mutex::new(PinDriver::output(peripherals.pins.gpio2)?));
    let d1_main = Arc::new(Mutex::new(PinDriver::output(peripherals.pins.gpio23)?));
    let d3_main = Arc::new(Mutex::new(PinDriver::output(peripherals.pins.gpio4)?));
    let d4_main = Arc::new(Mutex::new(PinDriver::output(peripherals.pins.gpio5)?));
    let d5_main = Arc::new(Mutex::new(PinDriver::output(peripherals.pins.gpio16)?));
    let d6_main = Arc::new(Mutex::new(PinDriver::output(peripherals.pins.gpio17)?));
    let d7_main = Arc::new(Mutex::new(PinDriver::output(peripherals.pins.gpio18)?));
    let d12_main = Arc::new(Mutex::new(PinDriver::output(peripherals.pins.gpio19)?));

    let mut server = EspHttpServer::new(&Configuration::default())?;

    server.fn_handler("/", Method::Get, |request| -> Result<()> {
        let mut response =
            request.into_response(200, Some("OK"), &[("Content-Type", "text/html")])?;
        response.write_all(UI_INDEX)?;
        Ok(())
    })?;

    server.fn_handler("/alumina-ui.js", Method::Get, |request| -> Result<()> {
        let mut response = request.into_response(
            200,
            Some("OK"),
            &[
                ("Content-Type", "text/javascript; charset=utf-8"),
                ("Content-Encoding", "gzip"),
            ],
        )?;
        response.write_all(UI_JAVASCRIPT)?;
        Ok(())
    })?;

    server.fn_handler(
        "/alumina-ui_bg.wasm",
        Method::Get,
        |request| -> Result<()> {
            let mut response = request.into_response(
                200,
                Some("OK"),
                &[
                    ("Content-Type", "application/wasm"),
                    ("Content-Encoding", "br"),
                ],
            )?;
            response.write_all(UI_WASM)?;
            Ok(())
        },
    )?;

    server.fn_handler("/favicon.ico", Method::Get, |request| -> Result<()> {
        let mut response =
            request.into_response(200, Some("OK"), &[("Content-Type", "image/gif")])?;
        response.write_all(UI_FAVICON)?;
        Ok(())
    })?;

    server.fn_handler("/device", Method::Get, |request| -> Result<()> {
        let body = format!(
            r#"{{"name":"{}","display_name":"{}","image_mime":"{}","image_url":"/device/image"}}"#,
            devices::Device::NAME,
            devices::Device::DISPLAY_NAME,
            devices::Device::IMAGE_MIME,
        );
        let mut response =
            request.into_response(200, Some("OK"), &[("Content-Type", "application/json")])?;
        response.write_all(body.as_bytes())?;
        Ok(())
    })?;

    server.fn_handler("/device/image", Method::Get, |request| -> Result<()> {
        let mut response = request.into_response(
            200,
            Some("OK"),
            &[
                ("Content-Type", devices::Device::IMAGE_MIME),
                ("Cache-Control", "public, max-age=86400"),
            ],
        )?;
        response.write_all(devices::Device::IMAGE_BYTES)?;
        Ok(())
    })?;

    server.fn_handler("/time", Method::Get, |request| -> Result<()> {
        // ESP-IDF reports monotonic time in microseconds; the browser consumes milliseconds.
        let milliseconds = (unsafe { esp_timer_get_time() } as u64) / 1_000;
        let body = format!("{milliseconds}\n");
        let mut response =
            request.into_response(200, Some("OK"), &[("Content-Type", "text/plain")])?;
        response.write_all(body.as_bytes())?;
        Ok(())
    })?;

    server.fn_handler("/files", Method::Post, |mut request| -> Result<()> {
        let mut chunk = [0_u8; 2_048];
        let mut received = 0_usize;
        loop {
            let bytes_read = request.read(&mut chunk)?;
            if bytes_read == 0 {
                break;
            }
            received = received.saturating_add(bytes_read);
        }

        let body = format!("received: {received} bytes\n");
        let mut response =
            request.into_response(200, Some("OK"), &[("Content-Type", "text/plain")])?;
        response.write_all(body.as_bytes())?;
        Ok(())
    })?;

    server.fn_handler("/queue", Method::Get, |request| -> Result<()> {
        let mut response =
            request.into_response(200, Some("OK"), &[("Content-Type", "text/plain")])?;
        response.write_all(b"Queue: []\n")?;
        Ok(())
    })?;

    {
        let d0 = Arc::clone(&d0_main);
        let d1 = Arc::clone(&d1_main);
        let d3 = Arc::clone(&d3_main);
        let d4 = Arc::clone(&d4_main);
        let d5 = Arc::clone(&d5_main);
        let d6 = Arc::clone(&d6_main);
        let d7 = Arc::clone(&d7_main);
        let d12 = Arc::clone(&d12_main);

        server.fn_handler("/pins", Method::Get, move |request| -> Result<()> {
            // Output pins expose their latched state even when physical input sampling is unavailable.
            let body = format!(
                r#"{{"D0":{},"D1":{},"D3":{},"D4":{},"D5":{},"D6":{},"D7":{},"D12":{}}}"#,
                d0.lock().expect("D0 lock poisoned").is_set_high() as u8,
                d1.lock().expect("D1 lock poisoned").is_set_high() as u8,
                d3.lock().expect("D3 lock poisoned").is_set_high() as u8,
                d4.lock().expect("D4 lock poisoned").is_set_high() as u8,
                d5.lock().expect("D5 lock poisoned").is_set_high() as u8,
                d6.lock().expect("D6 lock poisoned").is_set_high() as u8,
                d7.lock().expect("D7 lock poisoned").is_set_high() as u8,
                d12.lock().expect("D12 lock poisoned").is_set_high() as u8,
            );
            let mut response = request.into_response(
                200,
                Some("OK"),
                &[
                    ("Content-Type", "application/json"),
                    ("Cache-Control", "no-store"),
                ],
            )?;
            response.write_all(body.as_bytes())?;
            Ok(())
        })?;
    }

    let d0 = Arc::clone(&d0_main);
    let d1 = Arc::clone(&d1_main);
    let d3 = Arc::clone(&d3_main);
    let d4 = Arc::clone(&d4_main);
    let d5 = Arc::clone(&d5_main);
    let d6 = Arc::clone(&d6_main);
    let d7 = Arc::clone(&d7_main);
    let status_led = Arc::clone(&d12_main);
    let relay = Arc::clone(&d1_main);

    server.fn_handler("/queue", Method::Post, move |mut request| -> Result<()> {
        let mut buffer = [0_u8; 1_024];
        let bytes_read = request.read(&mut buffer)?;
        let command = std::str::from_utf8(&buffer[..bytes_read])?.trim();

        macro_rules! respond {
            ($status:expr, $reason:expr, $body:expr) => {{
                let body = $body;
                let mut response = request.into_response(
                    $status,
                    Some($reason),
                    &[("Content-Type", "text/plain")],
                )?;
                response.write_all(body.as_bytes())?;
            }};
        }

        match command {
            "status_on" => {
                status_led
                    .lock()
                    .expect("status LED lock poisoned")
                    .set_high()?;
                respond!(200, "OK", "Status LED on\n");
            }
            "status_off" => {
                status_led
                    .lock()
                    .expect("status LED lock poisoned")
                    .set_low()?;
                respond!(200, "OK", "Status LED off\n");
            }
            "relay_on" => {
                relay.lock().expect("relay lock poisoned").set_low()?;
                respond!(200, "OK", "Relay on\n");
            }
            "relay_off" => {
                relay.lock().expect("relay lock poisoned").set_high()?;
                respond!(200, "OK", "Relay off\n");
            }
            "d0_high" => {
                d0.lock().expect("D0 lock poisoned").set_high()?;
                respond!(200, "OK", "D0 high\n");
            }
            "d0_low" => {
                d0.lock().expect("D0 lock poisoned").set_low()?;
                respond!(200, "OK", "D0 low\n");
            }
            "d1_high" => {
                d1.lock().expect("D1 lock poisoned").set_high()?;
                respond!(200, "OK", "D1 high\n");
            }
            "d1_low" => {
                d1.lock().expect("D1 lock poisoned").set_low()?;
                respond!(200, "OK", "D1 low\n");
            }
            "d3_high" => {
                d3.lock().expect("D3 lock poisoned").set_high()?;
                respond!(200, "OK", "D3 high\n");
            }
            "d3_low" => {
                d3.lock().expect("D3 lock poisoned").set_low()?;
                respond!(200, "OK", "D3 low\n");
            }
            "d4_high" => {
                d4.lock().expect("D4 lock poisoned").set_high()?;
                respond!(200, "OK", "D4 high\n");
            }
            "d4_low" => {
                d4.lock().expect("D4 lock poisoned").set_low()?;
                respond!(200, "OK", "D4 low\n");
            }
            "d5_high" => {
                d5.lock().expect("D5 lock poisoned").set_high()?;
                respond!(200, "OK", "D5 high\n");
            }
            "d5_low" => {
                d5.lock().expect("D5 lock poisoned").set_low()?;
                respond!(200, "OK", "D5 low\n");
            }
            "d6_high" => {
                d6.lock().expect("D6 lock poisoned").set_high()?;
                respond!(200, "OK", "D6 high\n");
            }
            "d6_low" => {
                d6.lock().expect("D6 lock poisoned").set_low()?;
                respond!(200, "OK", "D6 low\n");
            }
            "d7_high" => {
                d7.lock().expect("D7 lock poisoned").set_high()?;
                respond!(200, "OK", "D7 high\n");
            }
            "d7_low" => {
                d7.lock().expect("D7 lock poisoned").set_low()?;
                respond!(200, "OK", "D7 low\n");
            }
            "scan_wifi" | "set_wifi" => {
                respond!(
                    501,
                    "Not Implemented",
                    "Wi-Fi configuration is not implemented\n"
                );
            }
            command if command == "g0" || command.starts_with("g0 ") => {
                let mut x = 10.0;
                let mut y = 0.0;
                let mut z = 0.0;
                let mut feed_rate = 1_500.0;

                for word in command.split_whitespace().skip(1) {
                    let Some((axis, value)) = word.get(..1).zip(word.get(1..)) else {
                        continue;
                    };
                    let Ok(value) = value.parse::<f32>() else {
                        continue;
                    };
                    match axis {
                        "x" | "X" => x = value,
                        "y" | "Y" => y = value,
                        "z" | "Z" => z = value,
                        "f" | "F" => feed_rate = value,
                        _ => {}
                    }
                }

                let mut planner = planner.lock().expect("motion planner lock poisoned");
                if planner.buffer_line(x, y, z, 0.0, feed_rate) {
                    planner.recalculate_trapezoids();
                    respond!(200, "OK", "Queued G0\n");
                } else {
                    respond!(503, "Service Unavailable", "Motion queue full\n");
                }
            }
            unknown => {
                log::warn!("Unknown queue command: {unknown}");
                respond!(400, "Bad Request", "Unknown command\n");
            }
        }

        Ok(())
    })?;

    log::info!("Alumina HTTP server is ready");
    loop {
        sleep(Duration::from_secs(1));
    }
}
