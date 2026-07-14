//! ESP32Drive pin map selected by `device_esp32drive`.

use crate::devices::Device;
pub mod pins {
    // General-purpose and UART pins.
    pub const TX0: i32 = 1;
    pub const RX0: i32 = 3;
    pub const GPIO17: i32 = 17;
    pub const GPIO21: i32 = 21;
    pub const GPIO22: i32 = 22;

    // Sensor header.
    pub const GPIO5: i32 = 5;
    pub const GPIO18: i32 = 18;
    pub const GPIO19: i32 = 19;
    pub const GPIO23: i32 = 23;
}

impl Device {
    pub const NAME: &'static str = "esp32drive";
    pub const DISPLAY_NAME: &'static str = "ESP32Drive";
    pub const IMAGE_BYTES: &'static [u8] =
        include_bytes!("../../docs/device_images/esp32drive.png");
    pub const IMAGE_MIME: &'static str = "image/png";
}
