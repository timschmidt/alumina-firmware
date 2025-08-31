//! Board selection (compile-time).
//!
//! Pick a board using a Cargo feature, e.g.:
//!     cargo build --features board_mks_tinybee
//!
//! Then, in code, you can do:
//!     use crate::boards::active::pins::*;
//!     let x_step_pin = X_STEP;

#[cfg(feature="board_mks_tinybee")]
pub mod mks_tinybee;

#[cfg(feature="board_esp32drive")]
pub mod esp32drive;

#[cfg(feature="board_esp32cam")]
pub mod esp32cam;

#[doc=" The currently selected device, re-exported as `active`."] 
#[cfg(feature="board_mks_tinybee")] pub use mks_tinybee as active;
#[cfg(feature="board_esp32drive")] pub use esp32drive as active;
#[cfg(feature="board_esp32cam")] pub use esp32cam as active;

#[cfg(not(any(feature="board_mks_tinybee",feature="board_esp32drive",feature="board_esp32cam",)))]
compile_error!("No board selected. Enable one of the board features, e.g. `--features board_mks_tinybee`.");

