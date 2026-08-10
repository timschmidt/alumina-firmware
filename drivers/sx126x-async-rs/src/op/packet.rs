//! Packet-related parameters for LoRa and GFSK.

/// The packet type used by the modem.
#[repr(u8)]
#[derive(Copy, Clone)]
pub enum PacketType {
    /// GFSK packet type.
    GFSK = 0x00,
    /// LoRa packet type.
    LoRa = 0x01,
}

/// A container for packet parameters.
pub struct PacketParams {
    inner: [u8; 9],
}

impl From<PacketParams> for [u8; 9] {
    fn from(val: PacketParams) -> Self {
        val.inner
    }
}

pub use lora::*;

mod lora {
    use super::PacketParams;

    /// LoRa header type.
    #[repr(u8)]
    #[derive(Copy, Clone)]
    pub enum LoRaHeaderType {
        /// Variable length packet (explicit header).
        VarLen = 0x00,
        /// Fixed length packet (implicit header).
        FixedLen = 0x01,
    }

    /// LoRa CRC (Cyclic Redundancy Check) type.
    #[repr(u8)]
    #[derive(Copy, Clone)]
    pub enum LoRaCrcType {
        /// CRC disabled.
        CrcOff = 0x00,
        /// CRC enabled.
        CrcOn = 0x01,
    }

    /// LoRa IQ (In-phase/Quadrature) inversion setting.
    #[repr(u8)]
    #[derive(Copy, Clone)]
    pub enum LoRaInvertIq {
        /// Standard IQ setup.
        Standard = 0x00,
        /// Inverted IQ setup.
        Inverted = 0x01,
    }

    /// A builder for LoRa packet parameters.
    pub struct LoRaPacketParams {
        /// Preamble length in number of symbols.
        pub preamble_len: u16,
        /// Header type (variable or fixed length).
        pub header_type: LoRaHeaderType,
        /// Size of the payload in bytes.
        pub payload_len: u8,
        /// CRC type (on or off).
        pub crc_type: LoRaCrcType,
        /// Invert IQ setting.
        pub invert_iq: LoRaInvertIq,
    }

    impl From<LoRaPacketParams> for PacketParams {
        fn from(val: LoRaPacketParams) -> Self {
            let preamble_len = val.preamble_len.to_be_bytes();

            PacketParams {
                inner: [
                    preamble_len[0],
                    preamble_len[1],
                    val.header_type as u8,
                    val.payload_len,
                    val.crc_type as u8,
                    val.invert_iq as u8,
                    0x00,
                    0x00,
                    0x00,
                ],
            }
        }
    }

    impl Default for LoRaPacketParams {
        fn default() -> Self {
            Self {
                preamble_len: 0x0008,
                header_type: LoRaHeaderType::VarLen,
                payload_len: 0xFF,
                crc_type: LoRaCrcType::CrcOff,
                invert_iq: LoRaInvertIq::Standard,
            }
        }
    }

    impl LoRaPacketParams {
        /// Sets the preamble length.
        pub fn set_preamble_len(mut self, preamble_len: u16) -> Self {
            self.preamble_len = preamble_len;
            self
        }

        /// Sets the header type.
        pub fn set_header_type(mut self, header_type: LoRaHeaderType) -> Self {
            self.header_type = header_type;
            self
        }

        /// Sets the payload length.
        pub fn set_payload_len(mut self, payload_len: u8) -> Self {
            self.payload_len = payload_len;
            self
        }

        /// Sets the CRC type.
        pub fn set_crc_type(mut self, crc_type: LoRaCrcType) -> Self {
            self.crc_type = crc_type;
            self
        }

        /// Sets the IQ inversion mode.
        pub fn set_invert_iq(mut self, invert_iq: LoRaInvertIq) -> Self {
            self.invert_iq = invert_iq;
            self
        }
    }
}
