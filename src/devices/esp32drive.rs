
//! esp32drive device pin map (compile-time selected via `device_esp32drive` feature).
//! All values are the device’s labeled IO numbers from the provided reference.

use crate::devices::Device;

#![allow(non_upper_case_globals)]
pub mod pins {
	// IO pins
    pub const TX0: i32 = 1;
    pub const RX0: i32 = 3;
    pub const GPIO17: i32 = 17;
    pub const GPIO21: i32 = 21;
    pub const GPIO22: i32 = 22;
    
    // Sensor
    pub const GPIO5: i32 = 5;
    pub const GPIO18: i32 = 18;
    pub const GPIO19: i32 = 19;
    pub const GPIO23: i32 = 23;

	

}

impl Device {
    pub const NAME: &'static str = "esp32drive";
    pub const DISPLAY_NAME: &'static str = "ESP32Drive";
    
	// NEW: compile-time embed of the board image so it’s available to the web server
    pub const IMAGE_BYTES: &'static [u8] = include_bytes!("../../docs/device_images/esp32drive.png");
    pub const IMAGE_MIME: &'static str = "image/png";
}

