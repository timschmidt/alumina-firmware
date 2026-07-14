//! AI-Thinker ESP32-CAM pin map selected by `device_esp32cam`.

use crate::devices::Device;

pub mod pins {
    // Camera signals.
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

    // SD-card signals.
    pub const CLK: i32 = 14;
    pub const CMD: i32 = 15;
    pub const DATA0: i32 = 2;
    pub const DATA1: i32 = 4;
    pub const DATA2: i32 = 12;
    pub const DATA3: i32 = 13;

    // Header pins. Several are shared with the camera or SD card.
    pub const GPIO0: i32 = 0;
    pub const GPIO1: i32 = 1;
    pub const GPIO2: i32 = 2;
    pub const GPIO3: i32 = 3;
    pub const GPIO4: i32 = 4;
    pub const GPIO12: i32 = 12;
    pub const GPIO13: i32 = 13;
    pub const GPIO14: i32 = 14;
    pub const GPIO15: i32 = 15;
    pub const GPIO16: i32 = 16;
}

impl Device {
    pub const NAME: &'static str = "esp32cam";
    pub const DISPLAY_NAME: &'static str = "ESP32-CAM";
    pub const IMAGE_BYTES: &'static [u8] = include_bytes!("../../docs/device_images/esp32cam.jpg");
    pub const IMAGE_MIME: &'static str = "image/jpeg";
}
