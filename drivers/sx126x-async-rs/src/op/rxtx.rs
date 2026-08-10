//! Transmit (TX) and Receive (RX) operation parameters.

/// A timeout value for RX and TX operations.
#[derive(Copy, Clone)]
pub struct RxTxTimeout {
    inner: [u8; 3],
}

impl From<RxTxTimeout> for [u8; 3] {
    fn from(val: RxTxTimeout) -> Self {
        val.inner
    }
}

impl RxTxTimeout {
    /// Creates a timeout from a duration in milliseconds.
    /// The value is `ms * 64 / 1000`.
    pub const fn from_ms(ms: u32) -> Self {
        let inner = ms << 6;
        let inner = inner.to_le_bytes();
        let inner = [inner[2], inner[1], inner[0]];
        Self { inner }
    }

    /// A special value representing continuous receive mode.
    pub const fn continuous_rx() -> Self {
        Self {
            inner: [0xFF, 0xFF, 0xFF],
        }
    }
}

impl From<u32> for RxTxTimeout {
    fn from(val: u32) -> Self {
        let bytes = val.to_be_bytes();
        Self {
            inner: [bytes[0], bytes[1], bytes[2]],
        }
    }
}

/// Power amplifier ramp time.
#[repr(u8)]
#[derive(Copy, Clone)]
pub enum RampTime {
    /// 10 µs
    Ramp10u = 0x00,
    /// 20 µs
    Ramp20u = 0x01,
    /// 40 µs
    Ramp40u = 0x02,
    /// 80 µs
    Ramp80u = 0x03,
    /// 200 µs
    Ramp200u = 0x04,
    /// 800 µs
    Ramp800u = 0x05,
    /// 1700 µs
    Ramp1700u = 0x06,
    /// 3400 µs
    Ramp3400u = 0x07,
}

/// A builder for transmit (TX) parameters.
pub struct TxParams {
    power_dbm: i8,
    ramp_time: RampTime,
}

impl Default for TxParams {
    fn default() -> Self {
        Self {
            power_dbm: 0,
            ramp_time: RampTime::Ramp10u,
        }
    }
}

impl From<TxParams> for [u8; 2] {
    fn from(val: TxParams) -> Self {
        [val.power_dbm as u8, val.ramp_time as u8]
    }
}

impl TxParams {
    /// Sets the output power in dBm.
    ///
    /// The valid range depends on the selected power amplifier (PA):
    /// - Low power PA: -17 to +14 dBm
    /// - High power PA: -9 to +22 dBm
    pub fn set_power_dbm(mut self, power_dbm: i8) -> Self {
        debug_assert!(power_dbm >= -17);
        debug_assert!(power_dbm <= 22);
        self.power_dbm = power_dbm;
        self
    }

    /// Sets the power amplifier ramp time.
    pub fn set_ramp_time(mut self, ramp_time: RampTime) -> Self {
        self.ramp_time = ramp_time;
        self
    }
}

/// The selected device type for the power amplifier.
#[repr(u8)]
#[derive(Copy, Clone)]
pub enum DeviceSel {
    /// For SX1262.
    SX1262 = 0x00,
    /// For SX1261.
    SX1261 = 0x01,
}

/// A builder for power amplifier (PA) configuration.
pub struct PaConfig {
    pa_duty_cycle: u8,
    hp_max: u8,
    device_sel: DeviceSel,
}

impl From<PaConfig> for [u8; 4] {
    fn from(val: PaConfig) -> Self {
        [val.pa_duty_cycle, val.hp_max, val.device_sel as u8, 0x01]
    }
}

impl Default for PaConfig {
    fn default() -> Self {
        Self {
            pa_duty_cycle: 0x00,
            hp_max: 0x00,
            device_sel: DeviceSel::SX1262,
        }
    }
}

impl PaConfig {
    /// Sets the PA duty cycle.
    pub fn set_pa_duty_cycle(mut self, pa_duty_cycle: u8) -> Self {
        self.pa_duty_cycle = pa_duty_cycle;
        self
    }

    /// Sets the maximum output power for the high-power PA.
    pub fn set_hp_max(mut self, hp_max: u8) -> Self {
        self.hp_max = hp_max;
        self
    }

    /// Sets the device type (SX1261 or SX1262).
    pub fn set_device_sel(mut self, device_sel: DeviceSel) -> Self {
        self.device_sel = device_sel;
        self
    }
}

/// The status of the receive (RX) buffer.
#[derive(Debug)]
pub struct RxBufferStatus {
    payload_length_rx: u8,
    rx_start_buffer_pointer: u8,
}

impl From<[u8; 2]> for RxBufferStatus {
    fn from(raw: [u8; 2]) -> Self {
        Self {
            payload_length_rx: raw[0],
            rx_start_buffer_pointer: raw[1],
        }
    }
}

impl RxBufferStatus {
    /// Returns the length of the received payload.
    pub fn payload_length_rx(&self) -> u8 {
        self.payload_length_rx
    }

    /// Returns the starting address of the payload in the buffer.
    pub fn rx_start_buffer_pointer(&self) -> u8 {
        self.rx_start_buffer_pointer
    }
}
