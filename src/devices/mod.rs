//! Device selection (compile-time).
//!
//! Pick a device using a Cargo feature, e.g.:
//!     cargo build --features device_mks_tinybee
//!
//! Then, in code, you can do:
//!     use crate::devices::active::pins::*;
//!     let x_step_pin = X_STEP;

#[cfg(feature="device_mks_tinybee")]
pub mod mks_tinybee;

#[cfg(feature="device_esp32drive")]
pub mod esp32drive;

#[cfg(feature="device_esp32cam")]
pub mod esp32cam;

#[doc=" The currently selected device, re-exported as `active`."] 
#[cfg(feature="device_mks_tinybee")] pub use mks_tinybee as active;
#[cfg(feature="device_esp32drive")] pub use esp32drive as active;
#[cfg(feature="device_esp32cam")] pub use esp32cam as active;

#[cfg(not(any(feature="device_mks_tinybee",feature="device_esp32drive",feature="device_esp32cam",)))]
compile_error!("No board selected. Enable one of the device features, e.g. `--features device_mks_tinybee`.");

