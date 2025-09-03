//! Device selection (compile-time).
//!
//! Pick a device using a Cargo feature, e.g.:
//!     cargo build --features device_mks_tinybee
//!
//! Then, in code, you can do:
//!     use crate::devices::active::pins::*;
//!     let x_step_pin = X_STEP;

pub struct Device;

#[cfg(feature="device_mks_tinybee")]
pub mod mks_tinybee;

#[cfg(feature="device_esp32drive")]
pub mod esp32drive;

#[cfg(feature="device_esp32cam")]
pub mod esp32cam;

#[cfg(feature="device_xprov5")]
pub mod xprov5;

#[cfg(not(any(feature="device_mks_tinybee",feature="device_esp32drive",feature="device_esp32cam",feature="device_xprov5",)))]
compile_error!("No device selected. Enable one of the device features, e.g. `--features device_mks_tinybee`.");

