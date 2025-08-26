use anyhow::{anyhow, bail, Result};
use core::convert::TryInto;
use embedded_svc::wifi::{
    AccessPointConfiguration, AuthMethod, ClientConfiguration, Configuration, Wifi,
};
use esp_idf_hal::peripheral;
use esp_idf_svc::{eventloop::EspSystemEventLoop, netif::EspNetif, wifi::EspWifi};
use log::info;
use std::{net::Ipv4Addr, time::Duration};
use std::time::Instant;

pub fn wifi(
    ssid: &str,
    pass: &str,
    modem: impl peripheral::Peripheral<P = esp_idf_hal::modem::Modem> + 'static,
    sysloop: EspSystemEventLoop,
) -> Result<Box<EspWifi<'static>>> {
    let mut auth_method = AuthMethod::WPA2Personal;

    if ssid.is_empty() {
        bail!("Missing WiFi name")
    }
    if pass.is_empty() {
        auth_method = AuthMethod::None;
        info!("Wifi password is empty");
    }

    let mut wifi = Box::new(EspWifi::new(modem, sysloop.clone(), None)?);
    info!("Wifi created, about to scan");

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
            "Configured access point {} not found during scanning, will go with unknown channel",
            ssid
        );
        None
    };

    wifi.set_configuration(&Configuration::Mixed(
        ClientConfiguration {
            // heapless::String<32/64>: convert with try_into (fails if too long)
            ssid: ssid
                .try_into()
                .map_err(|_| anyhow!("SSID too long (max 32)"))?,
            password: pass
                .try_into()
                .map_err(|_| anyhow!("password too long (max 64)"))?,
            channel,
            auth_method,
            ..Default::default()
        },
        AccessPointConfiguration {
            ssid: ssid
                .try_into()
                .map_err(|_| anyhow!("SSID too long (max 32)"))?,
            password: pass
                .try_into()
                .map_err(|_| anyhow!("password too long (max 64)"))?,
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
    info!("Starting wifi...");

    info!("Connecting wifi...");
    wifi.connect()?;

    // Wait up to 20s for connected + DHCP
	let deadline = Instant::now() + Duration::from_secs(20);
	loop {
	 if wifi.is_connected()? {
		 if let Ok(ip) = wifi.sta_netif().get_ip_info() {
			 if ip.ip != Ipv4Addr::new(0, 0, 0, 0) {
				 break;
			 }
		 }
	 }
	 if Instant::now() >= deadline {
		 bail!("Wifi did not connect or did not receive a DHCP lease");
	 }
	 std::thread::sleep(Duration::from_millis(100));
	}

    let ip_info = wifi.sta_netif().get_ip_info()?;
    info!("Wifi DHCP info: {:?}", ip_info);
    Ok(wifi)
}
