//! CNC xPro V5 pin map selected by `device_xprov5`.

use crate::devices::Device;

pub mod pins {
    // General-purpose inputs and outputs.
    pub const MIST: i32 = 21;
    pub const DOOR: i32 = 16;
    pub const MACRO1: i32 = 13;
    pub const MACRO2: i32 = 0;

    // Endstop and material-detection inputs.
    pub const X_STOP: i32 = 35;
    pub const Y_STOP: i32 = 34;
    pub const AY2_STOP: i32 = 36;
    pub const Z_STOP: i32 = 39;
    pub const MT_DET: i32 = 22;

    // Onboard TF-card interface.
    pub const TF_CS: i32 = 5;
    pub const TF_SCK: i32 = 18;
    pub const TF_MISO: i32 = 19;
    pub const TF_MOSI: i32 = 23;
    pub const TF_DET: i32 = 34;

    pub const MOTOR_DRIVER_CS: i32 = 17;
    // Positions in the motor-driver SPI daisy chain.
    pub const MOTOR_X: i32 = 1;
    pub const MOTOR_Y: i32 = 2;
    pub const MOTOR_AY2: i32 = 4;
    pub const MOTOR_Z: i32 = 3;

    // Multiplexed spindle-control and RS-485 signals.
    pub const SPINDLE_EN: i32 = 4;
    pub const SPINDLE_PWM: i32 = 25;

    // UART2; the board routes its USB serial connection to UART0 separately.
    pub const UART2_TXD: i32 = 17;
    pub const UART2_RXD: i32 = 16;
}

impl Device {
    pub const NAME: &'static str = "xprov5";
    pub const DISPLAY_NAME: &'static str = "CNC xPro V5";
    pub const IMAGE_BYTES: &'static [u8] = include_bytes!("../../docs/device_images/xprov5.png");
    pub const IMAGE_MIME: &'static str = "image/png";
}
