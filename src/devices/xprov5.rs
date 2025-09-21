//! xProV5 device pin map (compile-time selected via `device_xprov5` feature).
//! All values are the device’s labeled IO numbers from the provided reference.
#![allow(non_upper_case_globals)]

use crate::devices::Device;

pub mod pins {
    // IO
    pub const MIST: i32 = 21;
    pub const DOOR: i32 = 16;
    pub const MACRO1: i32 = 13;
    pub const MACRO2: i32 = 0;

    // Endstops & Detection
    pub const X_STOP: i32 = 35;
    pub const Y_STOP: i32 = 34;
    pub const AY2_STOP: i32 = 36;
    pub const Z_STOP: i32 = 39;
    pub const MT_DET: i32 = 22; // Motor/Material detect (per ref)

    // TF Card (onboard)
    pub const TF_CS:   i32 = 5;
    pub const TF_SCK:  i32 = 18;
    pub const TF_MISO: i32 = 19;
    pub const TF_MOSI: i32 = 23;
    pub const TF_DET:  i32 = 34; // default for SD_DET
    
    pub const MOTOR_DRIVER_CS: i32 = 17;
    pub const MOTOR_X: i32 = 1; // SPI daisy chain index
    pub const MOTOR_Y: i32 = 2; // SPI daisy chain index
    pub const MOTOR_AY2: i32 = 4; // SPI daisy chain index
    pub const MOTOR_Z: i32 = 3; // SPI daisy chain index

	// RS-485
	pub const SPINDLE_EN: i32 = 4; // Spindle Enable or RS485RX
	pub const SPINDLE_PWM: i32 = 25; // Spindle PWM Output or RS485TX

    // UART
    pub const UART2_TXD: i32 = 17;
    pub const UART2_RXD: i32 = 16;
    // USB-to-serial is USART0 (pins depend on SoC/board routing; not exposed here)

    // Convenience aliases
    pub const STATUS_LED: i32 = SD_MISO; // Example alias if you repurpose; adjust as needed
}

impl Device {
    pub const NAME: &'static str = "xprov5";
    pub const DISPLAY_NAME: &'static str = "CNC xPro V5";
    
	// NEW: compile-time embed of the board image so it’s available to the web server
    pub const IMAGE_BYTES: &'static [u8] = include_bytes!("../../docs/device_images/xprov5.png");
    pub const IMAGE_MIME: &'static str = "image/png";
}

