//! Error types for the SX126x driver.

use core::fmt::{self, Debug};

/// An error related to SPI communication.
pub enum SpiError<TSPIERR> {
    /// An error occurred during a `write` operation.
    Write(TSPIERR),
    /// An error occurred during a `transfer` operation.
    Transfer(TSPIERR),
}

impl<TSPIERR: Debug> Debug for SpiError<TSPIERR> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Write(err) => write!(f, "Write({err:?})"),
            Self::Transfer(err) => write!(f, "Transfer({err:?})"),
        }
    }
}

/// An error related to GPIO pin operations.
pub enum PinError<TPINERR> {
    /// An error occurred on an output pin.
    Output(TPINERR),
    /// An error occurred on an input pin.
    Input(TPINERR),
}

impl<TPINERR: Debug> Debug for PinError<TPINERR> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Output(err) => write!(f, "Output({err:?})"),
            Self::Input(err) => write!(f, "Input({err:?})"),
        }
    }
}

/// The main error type for the SX126x driver.
pub enum SxError<TSPIERR, TPINERR> {
    /// An SPI-related error.
    Spi(SpiError<TSPIERR>),
    /// A pin-related error.
    Pin(PinError<TPINERR>),
}

impl<TSPIERR: Debug, TPINERR: Debug> Debug for SxError<TSPIERR, TPINERR> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spi(err) => write!(f, "Spi({err:?})"),
            Self::Pin(err) => write!(f, "Pin({err:?})"),
        }
    }
}

impl<TSPIERR, TPINERR> From<SpiError<TSPIERR>> for SxError<TSPIERR, TPINERR> {
    fn from(spi_err: SpiError<TSPIERR>) -> Self {
        SxError::Spi(spi_err)
    }
}

impl<TSPIERR, TPINERR> From<PinError<TPINERR>> for SxError<TSPIERR, TPINERR> {
    fn from(spi_err: PinError<TPINERR>) -> Self {
        SxError::Pin(spi_err)
    }
}
