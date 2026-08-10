//! Modulation parameters for LoRa and GFSK.

/// A container for modulation parameters.
pub struct ModParams {
    inner: [u8; 8],
}

impl From<ModParams> for [u8; 8] {
    fn from(val: ModParams) -> Self {
        val.inner
    }
}

pub use lora::*;

mod lora {
    use super::ModParams;

    /// LoRa spreading factor.
    #[derive(Copy, Clone, Debug)]
    #[repr(u8)]
    pub enum LoRaSpreadFactor {
        /// Spreading Factor 5.
        SF5 = 0x05,
        /// Spreading Factor 6.
        SF6 = 0x06,
        /// Spreading Factor 7.
        SF7 = 0x07,
        /// Spreading Factor 8.
        SF8 = 0x08,
        /// Spreading Factor 9.
        SF9 = 0x09,
        /// Spreading Factor 10.
        SF10 = 0x0A,
        /// Spreading Factor 11.
        SF11 = 0x0B,
        /// Spreading Factor 12.
        SF12 = 0x0C,
    }

    impl From<u8> for LoRaSpreadFactor {
        fn from(value: u8) -> Self {
            match value {
                0x05 => Self::SF5,
                0x06 => Self::SF6,
                0x07 => Self::SF7,
                0x08 => Self::SF8,
                0x09 => Self::SF9,
                0x0A => Self::SF10,
                0x0B => Self::SF11,
                0x0C => Self::SF12,
                _ => panic!("Invalid LoRa spread factor"),
            }
        }
    }

    /// LoRa bandwidth.
    #[derive(Copy, Clone, Debug)]
    #[repr(u8)]
    pub enum LoRaBandWidth {
        /// 7.81 kHz
        BW7 = 0x00,
        /// 10.42 kHz
        BW10 = 0x08,
        /// 15.63 kHz
        BW15 = 0x01,
        /// 20.83 kHz
        BW20 = 0x09,
        /// 31.25 kHz
        BW31 = 0x02,
        /// 41.67 kHz
        BW41 = 0x0A,
        /// 62.50 kHz
        BW62 = 0x03,
        /// 125 kHz
        BW125 = 0x04,
        /// 250 kHz
        BW250 = 0x05,
        /// 500 kHz
        BW500 = 0x06,
    }

    /// LoRa coding rate.
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    #[repr(u8)]
    pub enum LoraCodingRate {
        /// 4/5
        CR4_5 = 0x01,
        /// 4/6
        CR4_6 = 0x02,
        /// 4/7
        CR4_7 = 0x03,
        /// 4/8
        CR4_8 = 0x04,
    }

    /// A builder for LoRa modulation parameters.
    pub struct LoraModParams {
        spread_factor: LoRaSpreadFactor,
        bandwidth: LoRaBandWidth,
        coding_rate: LoraCodingRate,
        /// Low Data Rate Optimize.
        low_dr_opt: bool,
    }

    impl Default for LoraModParams {
        fn default() -> Self {
            Self {
                spread_factor: LoRaSpreadFactor::SF7,
                bandwidth: LoRaBandWidth::BW125,
                coding_rate: LoraCodingRate::CR4_5,
                low_dr_opt: false,
            }
        }
    }

    impl LoraModParams {
        /// Sets the spreading factor.
        pub fn set_spread_factor(mut self, spread_factor: LoRaSpreadFactor) -> Self {
            self.spread_factor = spread_factor;
            self
        }
        /// Sets the bandwidth.
        pub fn set_bandwidth(mut self, bandwidth: LoRaBandWidth) -> Self {
            self.bandwidth = bandwidth;
            self
        }
        /// Sets the coding rate.
        pub fn set_coding_rate(mut self, coding_rate: LoraCodingRate) -> Self {
            self.coding_rate = coding_rate;
            self
        }
        /// Enables or disables the low data rate optimization.
        pub fn set_low_dr_opt(mut self, low_dr_opt: bool) -> Self {
            self.low_dr_opt = low_dr_opt;
            self
        }
    }

    impl From<LoraModParams> for ModParams {
        fn from(val: LoraModParams) -> Self {
            ModParams {
                inner: [
                    val.spread_factor as u8,
                    val.bandwidth as u8,
                    val.coding_rate as u8,
                    val.low_dr_opt as u8,
                    0x00,
                    0x00,
                    0x00,
                    0x00,
                ],
            }
        }
    }
}
