//! Interrupt (IRQ) configuration and status structures.

/// A bitmask for individual interrupt flags.
#[repr(u16)]
#[derive(Copy, Clone)]
pub enum IrqMaskBit {
    /// No interrupt.
    None = 0x0000,
    /// Transmit operation done.
    TxDone = 1 << 0,
    /// Receive operation done.
    RxDone = 1 << 1,
    /// Preamble detected.
    PreambleDetected = 1 << 2,
    /// Sync word valid.
    SyncwordValid = 1 << 3,
    /// Header valid.
    HeaderValid = 1 << 4,
    /// Header error.
    HeaderError = 1 << 5,
    /// CRC error.
    CrcErr = 1 << 6,
    /// Channel Activity Detection (CAD) done.
    CadDone = 1 << 7,
    /// Channel Activity Detected.
    CadDetected = 1 << 8,
    /// Operation timeout.
    Timeout = 1 << 9,
    /// All interrupts.
    All = 0xFFFF,
}

/// A builder for creating an interrupt mask.
#[derive(Copy, Clone)]
pub struct IrqMask {
    inner: u16,
}

impl IrqMask {
    /// Creates a new, empty `IrqMask`.
    pub const fn none() -> Self {
        Self {
            inner: IrqMaskBit::None as u16,
        }
    }

    /// Creates a new `IrqMask` with all interrupts enabled.
    pub const fn all() -> Self {
        Self {
            inner: IrqMaskBit::All as u16,
        }
    }

    /// Adds an interrupt flag to the mask.
    pub const fn combine(self, bit: IrqMaskBit) -> Self {
        let inner = self.inner | bit as u16;
        Self { inner }
    }
}

impl From<IrqMask> for u16 {
    fn from(val: IrqMask) -> Self {
        val.inner
    }
}

impl From<u16> for IrqMask {
    fn from(mask: u16) -> Self {
        Self { inner: mask }
    }
}

impl Default for IrqMask {
    fn default() -> Self {
        Self::none()
    }
}

/// Represents the interrupt status flags read from the device.
#[derive(Copy, Clone)]
pub struct IrqStatus {
    inner: u16,
}

impl From<u16> for IrqStatus {
    fn from(status: u16) -> Self {
        Self { inner: status }
    }
}

impl core::fmt::Debug for IrqStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("IrqStatus")
            .field("tx_done", &self.tx_done())
            .field("rx_done", &self.rx_done())
            .field("preamble_detected", &self.preamble_detected())
            .field("syncword_valid", &self.syncword_valid())
            .field("header_valid", &self.header_valid())
            .field("header_error", &self.header_error())
            .field("crc_err", &self.crc_err())
            .field("cad_done", &self.cad_done())
            .field("cad_detected", &self.cad_detected())
            .field("timeout", &self.timeout())
            .finish()
    }
}

impl IrqStatus {
    /// Returns `true` if the `TxDone` interrupt is active.
    pub fn tx_done(self) -> bool {
        (self.inner & IrqMaskBit::TxDone as u16) > 0
    }

    /// Returns `true` if the `RxDone` interrupt is active.
    pub fn rx_done(self) -> bool {
        (self.inner & IrqMaskBit::RxDone as u16) > 0
    }

    /// Returns `true` if the `PreambleDetected` interrupt is active.
    pub fn preamble_detected(self) -> bool {
        (self.inner & IrqMaskBit::PreambleDetected as u16) > 0
    }

    /// Returns `true` if the `SyncwordValid` interrupt is active.
    pub fn syncword_valid(self) -> bool {
        (self.inner & IrqMaskBit::SyncwordValid as u16) > 0
    }

    /// Returns `true` if the `HeaderValid` interrupt is active.
    pub fn header_valid(self) -> bool {
        (self.inner & IrqMaskBit::HeaderValid as u16) > 0
    }

    /// Returns `true` if the `HeaderError` interrupt is active.
    pub fn header_error(self) -> bool {
        (self.inner & IrqMaskBit::HeaderError as u16) > 0
    }

    /// Returns `true` if the `CrcErr` interrupt is active.
    pub fn crc_err(self) -> bool {
        (self.inner & IrqMaskBit::CrcErr as u16) > 0
    }

    /// Returns `true` if the `CadDone` interrupt is active.
    pub fn cad_done(self) -> bool {
        (self.inner & IrqMaskBit::CadDone as u16) > 0
    }

    /// Returns `true` if the `CadDetected` interrupt is active.
    pub fn cad_detected(self) -> bool {
        (self.inner & IrqMaskBit::CadDetected as u16) > 0
    }

    /// Returns `true` if the `Timeout` interrupt is active.
    pub fn timeout(self) -> bool {
        (self.inner & IrqMaskBit::Timeout as u16) > 0
    }
}
