#![no_std]
#![doc = "Audited read-only ESP radio configuration introspection for Alumina."]
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

/// A read-only snapshot of the vendor driver's applied SoftAP state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccessPointSnapshot {
    /// Applied SSID bytes; only the `ssid_length` prefix is meaningful.
    pub ssid: [u8; 32],
    /// Applied SSID byte length.
    pub ssid_length: u8,
    /// Whether the applied SSID is hidden.
    pub ssid_hidden: bool,
    /// Applied authentication-mode discriminator.
    pub authentication_mode: u32,
    /// Size of the Rust FFI SoftAP configuration structure.
    pub structure_bytes: usize,
    /// Byte offset of `beacon_interval` in the Rust FFI structure.
    pub beacon_interval_offset: usize,
    /// Byte offset of `csa_count` in the Rust FFI structure.
    pub csa_count_offset: usize,
    /// Byte offset of `dtim_period` in the Rust FFI structure.
    pub dtim_period_offset: usize,
    /// Applied beacon interval in time units.
    pub beacon_interval: u16,
    /// Applied channel-switch announcement count.
    pub csa_count: u8,
    /// Applied delivery traffic indication message period.
    pub dtim_period: u8,
    /// Applied Wi-Fi channel.
    pub channel: u8,
    /// Applied maximum associated-station count.
    pub maximum_connections: u8,
    /// Active radio-mode discriminator.
    pub radio_mode: u32,
    /// Active primary radio channel reported by the driver.
    pub active_primary_channel: u8,
    /// Active secondary-channel discriminator reported by the driver.
    pub active_secondary_channel: u32,
    /// Applied SoftAP interface MAC address.
    pub mac_address: [u8; 6],
    /// Applied 802.11 protocol bitmap.
    pub protocol_bitmap: u8,
    /// Applied channel-bandwidth discriminator.
    pub bandwidth: u32,
    /// Applied power-save discriminator.
    pub power_save_mode: u32,
}

/// Failure returned by [`read_access_point_config`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RadioAuditError {
    /// No supported ESP radio target feature was selected.
    UnsupportedTarget,
    /// The vendor API rejected the read; the contained value is `esp_err_t`.
    Driver(i32),
}

/// Reads the SoftAP configuration copied into the vendor radio driver.
///
/// This does not mutate radio state and must be called only after the radio has
/// been initialized and AP mode has been configured.
#[cfg(any(feature = "esp32", feature = "esp32s3"))]
pub fn read_access_point_config() -> Result<AccessPointSnapshot, RadioAuditError> {
    use core::mem::{MaybeUninit, offset_of, size_of};
    use esp_wifi_sys::include::{
        esp_wifi_get_bandwidth, esp_wifi_get_channel, esp_wifi_get_config, esp_wifi_get_mac,
        esp_wifi_get_mode, esp_wifi_get_protocol, esp_wifi_get_ps, wifi_ap_config_t, wifi_config_t,
        wifi_interface_t_WIFI_IF_AP,
    };
    #[cfg(feature = "esp32")]
    use esp_wifi_sys_esp32 as esp_wifi_sys;
    #[cfg(feature = "esp32s3")]
    use esp_wifi_sys_esp32s3 as esp_wifi_sys;

    let mut raw = MaybeUninit::<wifi_config_t>::zeroed();
    // SAFETY: `raw` points to writable, correctly aligned storage for the exact
    // chip-selected `wifi_config_t`. The API receives the AP discriminator and
    // is documented to initialize the corresponding union member on success.
    let result = unsafe { esp_wifi_get_config(wifi_interface_t_WIFI_IF_AP, raw.as_mut_ptr()) };
    if result != 0 {
        return Err(RadioAuditError::Driver(result));
    }

    // SAFETY: a successful `esp_wifi_get_config` initialized the AP member. The
    // zeroed backing storage also prevents reads of indeterminate padding.
    let config = unsafe { raw.assume_init() };
    // SAFETY: the AP interface discriminator above selects the `ap` union arm.
    let ap = unsafe { config.ap };

    let mut radio_mode = 0;
    let mut active_primary_channel = 0;
    let mut active_secondary_channel = 0;
    let mut mac_address = [0_u8; 6];
    let mut protocol_bitmap = 0;
    let mut bandwidth = 0;
    let mut power_save_mode = 0;
    // SAFETY: every output points to initialized, writable storage of the exact
    // FFI type, and read-only queries use the AP interface discriminator where
    // required. Each value is observed only after its call succeeds.
    let query_results = unsafe {
        [
            esp_wifi_get_mode(&mut radio_mode),
            esp_wifi_get_channel(&mut active_primary_channel, &mut active_secondary_channel),
            esp_wifi_get_mac(wifi_interface_t_WIFI_IF_AP, mac_address.as_mut_ptr()),
            esp_wifi_get_protocol(wifi_interface_t_WIFI_IF_AP, &mut protocol_bitmap),
            esp_wifi_get_bandwidth(wifi_interface_t_WIFI_IF_AP, &mut bandwidth),
            esp_wifi_get_ps(&mut power_save_mode),
        ]
    };
    if let Some(result) = query_results.into_iter().find(|result| *result != 0) {
        return Err(RadioAuditError::Driver(result));
    }

    Ok(AccessPointSnapshot {
        ssid: ap.ssid,
        ssid_length: ap.ssid_len,
        ssid_hidden: ap.ssid_hidden != 0,
        authentication_mode: ap.authmode,
        structure_bytes: size_of::<wifi_ap_config_t>(),
        beacon_interval_offset: offset_of!(wifi_ap_config_t, beacon_interval),
        csa_count_offset: offset_of!(wifi_ap_config_t, csa_count),
        dtim_period_offset: offset_of!(wifi_ap_config_t, dtim_period),
        beacon_interval: ap.beacon_interval,
        csa_count: ap.csa_count,
        dtim_period: ap.dtim_period,
        channel: ap.channel,
        maximum_connections: ap.max_connection,
        radio_mode,
        active_primary_channel,
        active_secondary_channel,
        mac_address,
        protocol_bitmap,
        bandwidth,
        power_save_mode,
    })
}

/// Returns [`RadioAuditError::UnsupportedTarget`] in portable builds.
#[cfg(not(any(feature = "esp32", feature = "esp32s3")))]
pub fn read_access_point_config() -> Result<AccessPointSnapshot, RadioAuditError> {
    Err(RadioAuditError::UnsupportedTarget)
}
