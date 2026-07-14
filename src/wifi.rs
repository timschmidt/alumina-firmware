//! Mixed station/access-point Wi-Fi mode.

use anyhow::{Result, anyhow, bail};
use core::convert::TryInto;
use embedded_svc::wifi::{
    AccessPointConfiguration, AuthMethod, ClientConfiguration, Configuration,
};
use esp_idf_hal::peripheral;
use esp_idf_svc::{eventloop::EspSystemEventLoop, wifi::EspWifi};
use log::info;
use std::{net::Ipv4Addr, time::Duration, time::Instant};

/// Starts mixed station/access-point mode and waits up to 20 seconds for station DHCP.
///
/// The access point mirrors the requested station credentials. An empty password selects an open
/// network; a non-empty password must satisfy WPA2's eight-byte minimum.
pub fn wifi(
    ssid: &str,
    pass: &str,
    modem: impl peripheral::Peripheral<P = esp_idf_hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
) -> Result<Box<EspWifi<'static>>> {
    if ssid.is_empty() {
        bail!("Wi-Fi SSID is empty")
    }
    let auth_method = if pass.is_empty() {
        info!("Wi-Fi password is empty; configuring an open network");
        AuthMethod::None
    } else {
        if pass.len() < 8 {
            bail!("WPA2 passwords must contain at least eight bytes");
        }
        AuthMethod::WPA2Personal
    };

    let mut wifi = Box::new(EspWifi::new(modem, sysloop.clone(), None)?);
    info!("Scanning for the configured Wi-Fi network");

    let ap_infos = wifi.scan()?;
    let ours = ap_infos.into_iter().find(|a| a.ssid.as_str() == ssid);
    let channel = if let Some(ours) = ours {
        info!(
            "Found configured access point {} on channel {}",
            ssid, ours.channel
        );
        Some(ours.channel)
    } else {
        info!(
            "Configured access point {} was not found; connecting without a channel hint",
            ssid
        );
        None
    };

    wifi.set_configuration(&Configuration::Mixed(
        ClientConfiguration {
            ssid: ssid
                .try_into()
                .map_err(|_| anyhow!("SSID exceeds the 32-byte limit"))?,
            password: pass
                .try_into()
                .map_err(|_| anyhow!("Wi-Fi password exceeds the 64-byte limit"))?,
            channel,
            auth_method,
            ..Default::default()
        },
        AccessPointConfiguration {
            ssid: ssid
                .try_into()
                .map_err(|_| anyhow!("SSID exceeds the 32-byte limit"))?,
            password: pass
                .try_into()
                .map_err(|_| anyhow!("Wi-Fi password exceeds the 64-byte limit"))?,
            auth_method: if pass.is_empty() {
                AuthMethod::None
            } else {
                AuthMethod::WPA2Personal
            },
            ssid_hidden: false,
            channel: channel.unwrap_or(1),
            max_connections: 4,
            ..Default::default()
        },
    ))?;

    wifi.start()?;
    info!("Connecting to Wi-Fi");
    wifi.connect()?;

    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if wifi.is_connected()?
            && let Ok(ip) = wifi.sta_netif().get_ip_info()
            && ip.ip != Ipv4Addr::UNSPECIFIED
        {
            break;
        }
        if Instant::now() >= deadline {
            bail!("Wi-Fi did not connect or receive a DHCP lease");
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let ip_info = wifi.sta_netif().get_ip_info()?;
    info!("Wi-Fi DHCP information: {:?}", ip_info);
    Ok(wifi)
}
