//! ESP HAL adapter for the portable SD SPI transport.

use alumina_sd_spi::ReconfigurableSpiBus;
use embedded_hal::spi::ErrorType;
use embedded_hal_async::spi::SpiBus;
use esp_hal::Async;
use esp_hal::gpio::Output;
use esp_hal::spi::master::{Config as SpiConfig, ConfigError, Spi};
use esp_hal::spi::{Error, Mode};
use esp_hal::time::Rate;

/// Local newtype permits portable frequency changes without coupling the SD
/// driver to `esp-hal`. Guard outputs keep every other shared-bus chip select
/// inactive until its actor is implemented.
pub struct EspSdSpiBus {
    spi: Spi<'static, Async>,
    _guard_outputs: [Option<Output<'static>>; 3],
}

impl EspSdSpiBus {
    pub fn new(spi: Spi<'static, Async>, guard_outputs: [Option<Output<'static>>; 3]) -> Self {
        Self {
            spi,
            _guard_outputs: guard_outputs,
        }
    }
}

impl ErrorType for EspSdSpiBus {
    type Error = Error;
}

impl SpiBus<u8> for EspSdSpiBus {
    async fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        SpiBus::read(&mut self.spi, words).await
    }

    async fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
        SpiBus::write(&mut self.spi, words).await
    }

    async fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
        SpiBus::transfer(&mut self.spi, read, write).await
    }

    async fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
        SpiBus::transfer_in_place(&mut self.spi, words).await
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        SpiBus::flush(&mut self.spi).await
    }
}

impl ReconfigurableSpiBus for EspSdSpiBus {
    type ConfigError = ConfigError;

    fn set_frequency_hz(&mut self, frequency_hz: u32) -> Result<(), Self::ConfigError> {
        self.spi.apply_config(
            &SpiConfig::default()
                .with_frequency(Rate::from_hz(frequency_hz))
                .with_mode(Mode::_0),
        )
    }
}
