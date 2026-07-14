//! Compile-time device selection and metadata.
//!
//! Enable exactly one `device_*` Cargo feature. The selected module supplies its pin map, while
//! [`Device`] exposes metadata embedded in the HTTP diagnostic interface.

/// Namespace for metadata associated with the selected controller board.
pub struct Device;

#[cfg(feature = "device_mks_tinybee")]
pub mod mks_tinybee;

#[cfg(feature = "device_esp32drive")]
pub mod esp32drive;

#[cfg(feature = "device_esp32cam")]
pub mod esp32cam;

#[cfg(feature = "device_xprov5")]
pub mod xprov5;

#[cfg(not(any(
    feature = "device_mks_tinybee",
    feature = "device_esp32drive",
    feature = "device_esp32cam",
    feature = "device_xprov5",
)))]
compile_error!(
    "no device selected; enable exactly one device feature, such as `device_mks_tinybee`"
);

#[cfg(any(
    all(feature = "device_mks_tinybee", feature = "device_esp32drive"),
    all(feature = "device_mks_tinybee", feature = "device_esp32cam"),
    all(feature = "device_mks_tinybee", feature = "device_xprov5"),
    all(feature = "device_esp32drive", feature = "device_esp32cam"),
    all(feature = "device_esp32drive", feature = "device_xprov5"),
    all(feature = "device_esp32cam", feature = "device_xprov5"),
))]
compile_error!("device features are mutually exclusive; enable exactly one");
