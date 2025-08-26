//! Board selection (compile-time).
//!
//! Pick a board using a Cargo feature, e.g.:
//!     cargo build --features board-mks_tinybee
//!
//! Then, in code, you can do:
//!     use crate::boards::active::pins::*;
//!     let x_step_pin = X_STEP;

#[cfg(feature="board-mks_tinybee")]
pub mod mks_tinybee;

pub trait BoardInfo {
    #[doc=" Human-readable board name."]
    fn name() -> &'static str;

    #[doc=" Raw bytes of the board image (e.g., pinout)."]
    fn image_bytes() -> &'static [u8];

    #[doc=" MIME type for the image bytes."]
    fn image_mime() -> &'static str;
}

#[doc=" The currently selected board module, re-exported as `active`."] 
#[cfg(feature="board-mks_tinybee")] pub use mks_tinybee as active;

#[cfg(feature="board-mks_tinybee")]
impl BoardInfo for active::MksTinyBee {
    fn name() -> &'static str { active::MksTinyBee::NAME }
    fn image_bytes() -> &'static [u8] { active::MksTinyBee::IMAGE_BYTES }
    fn image_mime() -> &'static str { active::MksTinyBee::IMAGE_MIME }
}

#[cfg(not(any(feature="board-mks_tinybee",)))]
compile_error!("No board selected. Enable one of the board features, e.g. `--features board-mks_tinybee`.");

