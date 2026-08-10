//! The core LoRa radio driver implementation.

use embassy_time::{Duration, Timer};
use embedded_hal_async::spi::SpiDevice;
use esp_hal::gpio::{Input, Output};
use sx126x_async::SX126x as Device;
use sx126x_async::calc_rf_freq;
use sx126x_async::op::*;

/// Configuration for the LoRa radio.
#[derive(Debug, Clone)]
pub struct LoraConfig {
    /// LoRa spreading factor.
    pub spreading_factor: LoRaSpreadFactor,
    /// LoRa signal bandwidth.
    pub bandwidth: LoRaBandWidth,
    /// LoRa coding rate.
    pub coding_rate: LoraCodingRate,
    /// SX1262 PA duty-cycle setting.
    pub pa_duty_cycle: u8,
    /// SX1262 high-power amplifier maximum setting.
    pub hp_max: u8,
    /// LoRa sync word.
    pub sync_word: u16,
}

impl Default for LoraConfig {
    fn default() -> Self {
        Self {
            spreading_factor: LoRaSpreadFactor::SF10,
            bandwidth: LoRaBandWidth::BW125,
            coding_rate: LoraCodingRate::CR4_6,
            pa_duty_cycle: 0x04,
            hp_max: 0x07,
            sync_word: 0x1424,
        }
    }
}

type Error = ();

/// A high-level interface for the SX1262 LoRa radio.
///
/// This struct encapsulates the `sx126x_async` device and provides methods
/// for initialization, sending, and receiving data.
pub struct LoraRadio<'a, SPI>
where
    SPI: SpiDevice,
{
    /// The underlying `sx126x_async` device instance.
    pub device: sx126x_async::SX126x<SPI, Output<'a>, Input<'a>, Output<'a>, Input<'a>>,
}

impl<'a, SPI> LoraRadio<'a, SPI>
where
    SPI: SpiDevice,
{
    /// Creates a new `LoraRadio`.
    ///
    /// Pins are supplied in board wiring order: RESET, DIO1, BUSY, and enable.
    pub fn new(
        lora_spi: SPI,
        rst: Output<'a>,
        dio1: Input<'a>,
        busy: Input<'a>,
        en: Output<'a>,
    ) -> Self {
        let device = Device::new(lora_spi, (rst, busy, en, dio1));
        Self { device }
    }

    /// Resets the LoRa module.
    pub async fn reset(&mut self) -> Result<(), Error> {
        self.device.reset().await.map_err(|err| {
            log::warn!("Error resetting device: {err:?}");
        })
    }

    /// Initializes the radio from `config` and fixed board-level RF settings.
    ///
    /// Frequency, TX power, preamble, CRC, and TCXO settings are currently
    /// fixed by this wrapper.
    pub async fn init(&mut self, config: &LoraConfig) -> Result<(), Error> {
        let lora_modparam = LoraModParams::default()
            .set_spread_factor(config.spreading_factor)
            .set_bandwidth(config.bandwidth)
            .set_coding_rate(config.coding_rate);

        let pa_config = PaConfig::default()
            .set_pa_duty_cycle(config.pa_duty_cycle)
            .set_hp_max(config.hp_max)
            .set_device_sel(DeviceSel::SX1262);

        let dio1_irq_mask = IrqMask::none()
            .combine(IrqMaskBit::TxDone)
            .combine(IrqMaskBit::RxDone)
            .combine(IrqMaskBit::Timeout);

        let conf = sx126x_async::conf::Config {
            packet_type: PacketType::LoRa,
            sync_word: config.sync_word,
            calib_param: CalibParam::all(),
            mod_params: lora_modparam.into(),
            pa_config,
            packet_params: Some(
                LoRaPacketParams {
                    preamble_len: 15,
                    header_type: LoRaHeaderType::VarLen,
                    payload_len: 0xFF,
                    crc_type: LoRaCrcType::CrcOff,
                    invert_iq: LoRaInvertIq::Standard,
                }
                .into(),
            ),
            tx_params: TxParams::default()
                .set_power_dbm(22i8)
                .set_ramp_time(RampTime::Ramp200u),
            dio1_irq_mask,
            dio2_irq_mask: IrqMask::none(),
            dio3_irq_mask: IrqMask::none(),
            rf_freq: calc_rf_freq(868_000_000.0, 32_000_000.0),
            rf_frequency: 868_000_000,
            tcxo_opts: Some((TcxoVoltage::Volt2_4, TcxoDelay::from_ms(5))),
        };
        log::trace!(
            "lora::init busy: {}, dio1: {}",
            self.device.is_busy(),
            self.device.is_dio1_high()
        );
        let init_result = self.device.init(conf).await.map_err(|err| {
            log::warn!("Error initializing device: {err:?}");
        });
        log::trace!(
            "lora::init done. busy: {}, dio1: {}",
            self.device.is_busy(),
            self.device.is_dio1_high()
        );

        log::debug!("LoRa radio status: {:?}", self.device.get_status().await);

        self.device.set_ocp(140).await.map_err(|err| {
            log::warn!("Error setting ocp: {err:?}");
        })?;

        init_result
    }

    /// Sends a data packet using the LoRa radio.
    ///
    /// `data` must contain at most 255 bytes.
    pub async fn send(&mut self, data: &[u8]) -> Result<(), Error> {
        self.device
            .write_bytes(data, RxTxTimeout::from_ms(2000), 15, LoRaCrcType::CrcOff)
            .await
            .map_err(|err| {
                log::warn!("Error sending lora message: {err:?}");
            })?;
        Ok(())
    }

    /// Waits to receive a data packet from the LoRa radio.
    ///
    /// This function puts the radio into receive mode and waits for the DIO1
    /// interrupt pin to signal a received packet. It handles timeouts and
    /// other interrupt flags.
    ///
    /// # Arguments
    ///
    /// * `buffer` - A mutable byte slice to store the received data.
    ///
    /// # Returns
    ///
    /// The number of bytes received, or an error.
    pub async fn receive(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        log::trace!("lora::receive waiting for message");
        self.device
            .clear_irq_status(IrqMask::all())
            .await
            .map_err(|_| ())?;
        self.device
            .set_rx(RxTxTimeout::from_ms(5000))
            .await
            .map_err(|err| {
                log::warn!("Error setting rx mode: {err:?}");
            })?;
        log::trace!(
            "lora::receive in rx mode. busy: {}, dio1: {}",
            self.device.is_busy(),
            self.device.is_dio1_high()
        );

        self.device.wait_on_dio1().await.map_err(|err| {
            log::warn!("Error waiting for dio1: {err:?}");
        })?;
        log::trace!(
            "lora::receive DIO1 went high. busy: {}, dio1: {}",
            self.device.is_busy(),
            self.device.is_dio1_high()
        );

        // Allow the IRQ status registers to settle after DIO1 rises.
        Timer::after(Duration::from_millis(1)).await;
        log::trace!(
            "lora::receive post-delay. busy: {}, dio1: {}",
            self.device.is_busy(),
            self.device.is_dio1_high()
        );

        let irq_status = self.device.get_irq_status().await.map_err(|err| {
            log::warn!("Error getting irq status: {err:?}");
        })?;
        log::trace!("lora::receive irq status: {irq_status:?}");

        // Timeout is an expected empty receive; all other unexpected IRQs fail.
        if irq_status.timeout() {
            log::trace!("lora::receive timeout");
            self.device
                .clear_irq_status(IrqMask::all())
                .await
                .map_err(|_| ())?;
            return Ok(0);
        }

        if !irq_status.rx_done() {
            log::warn!("lora::receive unexpected interrupt: {irq_status:?}");
            self.device
                .clear_irq_status(IrqMask::all())
                .await
                .map_err(|_| ())?;
            return Err(());
        }

        let rx_status = self.device.get_rx_buffer_status().await.map_err(|err| {
            log::warn!("Error getting rx buffer status: {err:?}");
        })?;
        let len = rx_status.payload_length_rx() as usize;
        let offset = rx_status.rx_start_buffer_pointer();
        log::trace!("lora::receive rx status: len={len}, offset={offset}");

        if len > buffer.len() {
            log::warn!("lora::receive received payload larger than buffer");
            self.device
                .clear_irq_status(IrqMask::all())
                .await
                .map_err(|_| ())?;
            return Err(());
        }

        self.device
            .read_buffer(offset, &mut buffer[..len])
            .await
            .map_err(|err| {
                log::warn!("Error reading buffer: {err:?}");
            })?;
        log::trace!("lora::receive buffer read");

        self.device
            .clear_irq_status(IrqMask::all())
            .await
            .map_err(|err| {
                log::warn!("Error clearing irq status: {err:?}");
            })?;
        log::trace!("lora::receive IRQ cleared");

        Ok(len)
    }
}
