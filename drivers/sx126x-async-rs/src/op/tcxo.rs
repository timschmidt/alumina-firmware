//! TCXO (Temperature-Compensated Crystal Oscillator) control parameters.

/// TCXO voltage settings.
#[repr(u8)]
#[derive(Copy, Clone)]
pub enum TcxoVoltage {
    /// 1.6V
    Volt1_6 = 0x00,
    /// 1.7V
    Volt1_7 = 0x01,
    /// 1.8V
    Volt1_8 = 0x02,
    /// 2.2V
    Volt2_2 = 0x03,
    /// 2.4V
    Volt2_4 = 0x04,
    /// 2.7V
    Volt2_7 = 0x05,
    /// 3.0V
    Volt3_0 = 0x06,
    /// 3.3V
    Volt3_3 = 0x07,
}

/// TCXO startup delay.
#[derive(Copy, Clone)]
pub struct TcxoDelay {
    inner: [u8; 3],
}

impl From<TcxoDelay> for [u8; 3] {
    fn from(val: TcxoDelay) -> Self {
        val.inner
    }
}

impl From<[u8; 3]> for TcxoDelay {
    fn from(b: [u8; 3]) -> Self {
        Self { inner: b }
    }
}

impl TcxoDelay {
    /// Creates a TCXO delay from a duration in milliseconds.
    /// The value is `ms * 64 / 1000`.
    pub const fn from_ms(ms: u32) -> Self {
        let inner = ms << 6;
        let inner = inner.to_le_bytes();
        let inner = [inner[2], inner[1], inner[0]];
        Self { inner }
    }
}
