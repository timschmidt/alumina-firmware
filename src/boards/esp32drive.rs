
//! esp32drive board pin map (compile-time selected via `board_esp32drive` feature).
//! All values are the board’s labeled IO numbers from the provided reference.

#![allow(non_upper_case_globals)]
pub mod pins {

}

pub struct Device;

impl Device {
    pub const NAME: &'static str = "esp32drive";
    
	// NEW: compile-time embed of the board image so it’s available to the web server
    pub const IMAGE_BYTES: &'static [u8] = include_bytes!("../../docs/board_images/esp32drive.png");
    pub const IMAGE_MIME: &'static str = "image/png";
}

