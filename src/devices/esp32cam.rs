//! esp32cam device pin map (compile-time selected via `device_esp32cam` feature).
//! All values are the device’s labeled IO numbers from the provided reference.

use crate::devices::Device;

#![allow(non_upper_case_globals)]
pub mod pins {
	// Camera module
	pub const D0: i32 = 5;
	pub const D1: i32 = 18;
	pub const D2: i32 = 19;
	pub const D3: i32 = 21;
	pub const D4: i32 = 36;
	pub const D5: i32 = 39;
	pub const D6: i32 = 34;
	pub const D7: i32 = 35;
	pub const XCLK: i32 = 0;
	pub const PCLK: i32 = 22;
	pub const VSYNC: i32 = 25;
	pub const HREF: i32 = 23;
	pub const SDA: i32 = 26;
	pub const SCL: i32 = 27;
	pub const POWER: i32 = 32;
	
	// SD
	pub const CLK: i32 = 14;
	pub const CMD: i32 = 15;
	pub const DATA0: i32 = 2;
	pub const DATA1: i32 = 4;
	pub const DATA2: i32 = 12;
	pub const DATA3: i32 = 13;
	
	// Header pins
	pub const GPIO0: i32 = 0; // CSI MCLK / XCLK
	pub const GPIO1: i32 = 1; // uart0 tx
	pub const GPIO2: i32 = 2; // SD DATA0
	pub const GPIO3: i32 = 3; // uart0 rx
	pub const GPIO4: i32 = 4; // SD DATA1
	pub const GPIO12: i32 = 12; // SD DATA2
	pub const GPIO13: i32 = 13; // SD DATA3
	pub const GPIO14: i32 = 14; // SD CLK
	pub const GPIO15: i32 = 15; // SD CMD
	pub const GPIO16: i32 = 16; // uart2 rx
}

impl Device {
    pub const NAME: &'static str = "esp32cam";
    pub const DISPLAY_NAME: &'static str = "ESP32-CAM";
    
	// NEW: compile-time embed of the board image so it’s available to the web server
    pub const IMAGE_BYTES: &'static [u8] = include_bytes!("../../docs/device_images/esp32cam.jpg");
    pub const IMAGE_MIME: &'static str = "image/jpg";
}

