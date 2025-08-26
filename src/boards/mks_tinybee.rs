//! MKS TinyBee board pin map (compile-time selected via `board-mks_tinybee` feature).
//! All values are the board’s labeled IO numbers from the provided reference.

#![allow(non_upper_case_globals)]
pub mod pins {
    // -------------------------
    // Heaters
    // -------------------------
    pub const H_BED: i32 = 144;
    pub const H_E0:  i32 = 145;
    pub const H_E1:  i32 = 146;

    // -------------------------
    // PWM Fans
    // -------------------------
    pub const FAN1: i32 = 147;
    pub const FAN2: i32 = 148;

    // -------------------------
    // Steppers
    // -------------------------
    pub const X_ENABLE: i32 = 128;
    pub const X_STEP:   i32 = 129;
    pub const X_DIR:    i32 = 130;

    pub const Y_ENABLE: i32 = 131;
    pub const Y_STEP:   i32 = 132;
    pub const Y_DIR:    i32 = 133;

    pub const Z_ENABLE: i32 = 134;
    pub const Z_STEP:   i32 = 135;
    pub const Z_DIR:    i32 = 136;

    pub const E0_ENABLE: i32 = 137;
    pub const E0_STEP:   i32 = 138;
    pub const E0_DIR:    i32 = 139;

    pub const E1_ENABLE: i32 = 140;
    pub const E1_STEP:   i32 = 141;
    pub const E1_DIR:    i32 = 142;

    // -------------------------
    // Endstops & Detection
    // -------------------------
    pub const X_STOP: i32 = 33;
    pub const Y_STOP: i32 = 32;
    pub const Z_STOP: i32 = 22;

    pub const MT_DET: i32 = 35; // Motor/Material detect (per ref)

    // -------------------------
    // Servo / Probe
    // -------------------------
    pub const TOUCH_3D: i32 = 2;

    // -------------------------
    // Thermistors (THM)
    // -------------------------
    pub const TH1: i32 = 36;
    pub const TH2: i32 = 34; // needs jumper selection
    pub const TB:  i32 = 39;

    // -------------------------
    // EXP1 (LCD/Buttons/Beeper)
    // -------------------------
    pub const BEEPER: i32 = 149;
    pub const BTN_ENC: i32 = 13;
    pub const LCD_EN:  i32 = 21;
    pub const LCD_RS:  i32 = 4;
    pub const LCD_D4:  i32 = 0;
    pub const LCD_D5:  i32 = 16;
    pub const LCD_D6:  i32 = 15;
    pub const LCD_D7:  i32 = 17;

    // -------------------------
    // EXP2 / SD (external)
    // -------------------------
    pub const SD_MISO: i32 = 19;
    pub const SD_SCK:  i32 = 18;
    pub const BTN_EN1: i32 = 14;
    pub const SD_CS:   i32 = 5;
    pub const BTN_EN2: i32 = 12;
    pub const SD_MOSI: i32 = 23;
    pub const SD_DET:  i32 = 34; // default for SD_DET

    // -------------------------
    // TF Card (onboard)
    // -------------------------
    pub const TF_CS:   i32 = 5;
    pub const TF_SCK:  i32 = 18;
    pub const TF_MISO: i32 = 19;
    pub const TF_MOSI: i32 = 23;
    pub const TF_DET:  i32 = 34; // default for SD_DET

    // -------------------------
    // UART
    // -------------------------
    pub const UART2_TXD: i32 = 17;
    pub const UART2_RXD: i32 = 16;
    // USB-to-serial is USART0 (pins depend on SoC/board routing; not exposed here)

    // -------------------------
    // Convenience aliases
    // -------------------------
    pub const STATUS_LED: i32 = SD_MISO; // Example alias if you repurpose; adjust as needed
}

// A minimal marker type so higher-level code can refer to the active board by type.
pub struct MksTinyBee;

impl MksTinyBee {
    pub const NAME: &'static str = "mks_tinybee";
    
	// NEW: compile-time embed of the board image so it’s available to the web server
    pub const IMAGE_BYTES: &'static [u8] = include_bytes!("../../docs/board_images/mks_tinybee.png");
    pub const IMAGE_MIME: &'static str = "image/png";
}

