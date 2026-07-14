//! MKS TinyBee pin map selected by `device_mks_tinybee`.

use crate::devices::Device;

pub mod pins {
    // PCF8575-backed outputs use the firmware's virtual-pin range 128–149.
    // Heater outputs.
    pub const H_BED: i32 = 144;
    pub const H_E0: i32 = 145;
    pub const H_E1: i32 = 146;

    // PWM fan outputs.
    pub const FAN1: i32 = 147;
    pub const FAN2: i32 = 148;

    // Stepper-driver outputs.
    pub const X_ENABLE: i32 = 128;
    pub const X_STEP: i32 = 129;
    pub const X_DIR: i32 = 130;

    pub const Y_ENABLE: i32 = 131;
    pub const Y_STEP: i32 = 132;
    pub const Y_DIR: i32 = 133;

    pub const Z_ENABLE: i32 = 134;
    pub const Z_STEP: i32 = 135;
    pub const Z_DIR: i32 = 136;

    pub const E0_ENABLE: i32 = 137;
    pub const E0_STEP: i32 = 138;
    pub const E0_DIR: i32 = 139;

    pub const E1_ENABLE: i32 = 140;
    pub const E1_STEP: i32 = 141;
    pub const E1_DIR: i32 = 142;

    // Endstop and material-detection inputs.
    pub const X_STOP: i32 = 33;
    pub const Y_STOP: i32 = 32;
    pub const Z_STOP: i32 = 22;

    pub const MT_DET: i32 = 35;

    // Servo and probe header.
    pub const TOUCH_3D: i32 = 2;

    // Thermistor inputs. TH2 requires the corresponding board jumper.
    pub const TH1: i32 = 36;
    pub const TH2: i32 = 34;
    pub const TB: i32 = 39;

    // EXP1 display, button, and beeper header.
    pub const BEEPER: i32 = 149;
    pub const BTN_ENC: i32 = 13;
    pub const LCD_EN: i32 = 21;
    pub const LCD_RS: i32 = 4;
    pub const LCD_D4: i32 = 0;
    pub const LCD_D5: i32 = 16;
    pub const LCD_D6: i32 = 15;
    pub const LCD_D7: i32 = 17;

    // EXP2 external SD-card header.
    pub const SD_MISO: i32 = 19;
    pub const SD_SCK: i32 = 18;
    pub const BTN_EN1: i32 = 14;
    pub const SD_CS: i32 = 5;
    pub const BTN_EN2: i32 = 12;
    pub const SD_MOSI: i32 = 23;
    pub const SD_DET: i32 = 34;

    // Onboard TF-card interface.
    pub const TF_CS: i32 = 5;
    pub const TF_SCK: i32 = 18;
    pub const TF_MISO: i32 = 19;
    pub const TF_MOSI: i32 = 23;
    pub const TF_DET: i32 = 34;

    // UART2; the board routes its USB serial connection to UART0 separately.
    pub const UART2_TXD: i32 = 17;
    pub const UART2_RXD: i32 = 16;
}

impl Device {
    pub const NAME: &'static str = "mks_tinybee";
    pub const DISPLAY_NAME: &'static str = "MKS TinyBee";
    pub const IMAGE_BYTES: &'static [u8] =
        include_bytes!("../../docs/device_images/mks_tinybee.png");
    pub const IMAGE_MIME: &'static str = "image/png";
}
