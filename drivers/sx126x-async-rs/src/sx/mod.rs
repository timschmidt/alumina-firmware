//! The core implementation of the SX126x driver.

pub(crate) mod err;

use core::convert::TryInto;
use embedded_hal::digital::InputPin;
use embedded_hal::digital::OutputPin;
use embedded_hal_async::digital::Wait;
use embedded_hal_async::spi::Operation;
use embedded_hal_async::spi::SpiDevice;
use err::SpiError;

use crate::conf::Config;
use crate::op::*;
use crate::reg::*;

use self::err::{PinError, SxError};

type Pins<TNRST, TBUSY, TANT, TDIO1> = (TNRST, TBUSY, TANT, TDIO1);

const NOP: u8 = 0x00;

/// Calculates the `rf_freq` value that should be passed to `SX126x::set_rf_frequency`
/// based on the desired RF frequency and the XTAL frequency.
///
/// # Example
///
/// For an 868 MHz RF frequency and a 32 MHz crystal:
/// `calc_rf_freq(868_000_000.0, 32_000_000.0)`
pub fn calc_rf_freq(rf_frequency: f32, f_xtal: f32) -> u32 {
    (rf_frequency * (33554432. / f_xtal)) as u32
}

/// A wrapper around a Semtech SX1261/62 LoRa modem.
pub struct SX126x<TSPI: SpiDevice, TNRST, TBUSY, TANT, TDIO1> {
    spi: TSPI,
    nrst_pin: TNRST,
    busy_pin: TBUSY,
    ant_pin: TANT,
    dio1_pin: TDIO1,
}

impl<TSPI, TNRST, TBUSY, TANT, TDIO1, TSPIERR, TPINERR> SX126x<TSPI, TNRST, TBUSY, TANT, TDIO1>
where
    TPINERR: core::fmt::Debug,
    TSPI: SpiDevice<Error = TSPIERR>,
    TNRST: OutputPin<Error = TPINERR>,
    TBUSY: InputPin<Error = TPINERR> + Wait,
    TANT: OutputPin<Error = TPINERR>,
    TDIO1: InputPin<Error = TPINERR> + Wait,
{
    /// Creates a new `SX126x` driver instance.
    ///
    /// # Arguments
    ///
    /// * `spi` - An asynchronous SPI peripheral.
    /// * `pins` - GPIO pins in `(NRESET, BUSY, ANT_SW, DIO1)` order.
    pub fn new(spi: TSPI, pins: Pins<TNRST, TBUSY, TANT, TDIO1>) -> Self {
        let (nrst_pin, busy_pin, ant_pin, dio1_pin) = pins;
        Self {
            spi,
            nrst_pin,
            busy_pin,
            ant_pin,
            dio1_pin,
        }
    }

    /// Initializes and configures the SX126x modem.
    ///
    /// This method performs the configuration sequence described by `conf`.
    pub async fn init(&mut self, conf: Config) -> Result<(), SxError<TSPIERR, TPINERR>> {
        log::trace!("lora::init start");
        self.reset().await?;
        log::trace!(
            "lora::init reset done. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        self.set_standby(crate::op::StandbyConfig::StbyRc).await?;
        log::trace!(
            "lora::init standby set. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        self.set_packet_type(conf.packet_type).await?;
        log::trace!(
            "lora::init packet type set. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        self.set_rf_frequency(conf.rf_freq).await?;
        log::trace!(
            "lora::init rf frequency set. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        if let Some((tcxo_voltage, tcxo_delay)) = conf.tcxo_opts {
            self.set_dio3_as_tcxo_ctrl(tcxo_voltage, tcxo_delay).await?;
            log::trace!(
                "lora::init tcxo ctrl set. busy: {}, dio1: {}",
                self.is_busy(),
                self.is_dio1_high()
            );
        }

        self.calibrate(conf.calib_param).await?;
        log::trace!(
            "lora::init calibrated. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );
        self.calibrate_image(CalibImageFreq::from_rf_frequency(conf.rf_frequency))
            .await?;
        log::trace!(
            "lora::init image calibrated. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        self.set_pa_config(conf.pa_config).await?;
        log::trace!(
            "lora::init pa config set. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        self.set_tx_params(conf.tx_params).await?;
        log::trace!(
            "lora::init tx params set. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        self.set_buffer_base_address(0x00, 0x00).await?;
        log::trace!(
            "lora::init buffer base address set. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        self.set_mod_params(conf.mod_params).await?;
        log::trace!(
            "lora::init mod params set. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        if let Some(packet_params) = conf.packet_params {
            self.set_packet_params(packet_params).await?;
            log::trace!(
                "lora::init packet params set. busy: {}, dio1: {}",
                self.is_busy(),
                self.is_dio1_high()
            );
        }

        self.set_dio_irq_params(
            conf.dio1_irq_mask,
            conf.dio1_irq_mask,
            conf.dio2_irq_mask,
            conf.dio3_irq_mask,
        )
        .await?;
        log::trace!(
            "lora::init dio irq params set. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );
        self.set_dio2_as_rf_switch_ctrl(true).await?;
        log::trace!(
            "lora::init dio2 as rf switch set. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        self.set_sync_word(conf.sync_word).await?;
        log::trace!(
            "lora::init sync word set. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        log::trace!("lora::init done");
        Ok(())
    }

    /// Sets the LoRa sync word (`0x3444` public or `0x1424` private).
    pub async fn set_sync_word(&mut self, sync_word: u16) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.write_register(Register::LoRaSyncWordMsb, &sync_word.to_be_bytes())
            .await
    }

    /// Sets the modem packet type (LoRa or GFSK).
    /// Note: GFSK is not fully supported by this crate.
    pub async fn set_packet_type(
        &mut self,
        packet_type: PacketType,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        self.spi
            .write(&[0x8A, packet_type as u8])
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Puts the modem in a specified standby mode.
    pub async fn set_standby(
        &mut self,
        standby_config: StandbyConfig,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        self.spi
            .write(&[0x80, standby_config as u8])
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Gets the current status of the modem.
    pub async fn get_status(&mut self) -> Result<Status, SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let mut result = [0xC0, NOP];
        self.spi
            .transfer_in_place(&mut result)
            .await
            .map_err(SpiError::Transfer)?;
        log::trace!("lora::get_status raw response: {result:?}");
        Ok(result[1].into())
    }

    /// Sets the modem to frequency synthesis mode.
    pub async fn set_fs(&mut self) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        self.spi.write(&[0xC1]).await.map_err(SpiError::Write)?;
        Ok(())
    }

    /// Gets statistics from the modem (e.g., packets received, CRC errors).
    pub async fn get_stats(&mut self) -> Result<Stats, SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let mut result = [0x10, NOP, NOP, NOP, NOP, NOP, NOP, NOP];
        self.spi
            .transfer_in_place(&mut result)
            .await
            .map_err(SpiError::Transfer)?;

        Ok(TryInto::<[u8; 7]>::try_into(&result[1..]).unwrap().into())
    }

    /// Calibrates the image rejection filter for a given frequency range.
    pub async fn calibrate_image(
        &mut self,
        freq: CalibImageFreq,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let freq: [u8; 2] = freq.into();
        let mut ops = [Operation::Write(&[0x98]), Operation::Write(&freq)];
        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Calibrates the modem with the specified parameters.
    pub async fn calibrate(
        &mut self,
        calib_param: CalibParam,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        self.spi
            .write(&[0x89, calib_param.into()])
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Writes data to a specific register.
    pub async fn write_register(
        &mut self,
        register: Register,
        data: &[u8],
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let start_addr = (register as u16).to_be_bytes();
        let mut ops = [
            Operation::Write(&[0x0D]),
            Operation::Write(&start_addr),
            Operation::Write(data),
        ];

        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Write)?;
        Ok(())
    }

    /// Reads data from a specific register.
    pub async fn read_register(
        &mut self,
        start_addr: u16,
        result: &mut [u8],
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        debug_assert!(!result.is_empty());
        let start_addr = start_addr.to_be_bytes();

        let mut ops = [
            Operation::Write(&[0x1D]),
            Operation::Write(&start_addr),
            Operation::Read(result),
        ];

        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Transfer)?;
        Ok(())
    }

    /// Writes data to the modem's buffer at a given offset.
    pub async fn write_buffer(
        &mut self,
        offset: u8,
        data: &[u8],
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let header = [0x0E, offset];
        let mut ops = [Operation::Write(&header), Operation::Write(data)];
        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Reads data from the modem's buffer at a given offset.
    pub async fn read_buffer(
        &mut self,
        offset: u8,
        result: &mut [u8],
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let header = [0x1E, offset, NOP];
        let mut ops = [Operation::Write(&header), Operation::Read(result)];
        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Transfer)
            .map_err(Into::into)
    }

    /// Configures the DIO2 pin as an RF control switch.
    pub async fn set_dio2_as_rf_switch_ctrl(
        &mut self,
        enable: bool,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        self.spi
            .write(&[0x9D, enable as u8])
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Gets the status of the last received packet (RSSI, SNR).
    pub async fn get_packet_status(&mut self) -> Result<PacketStatus, SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let header = [0x14, NOP];
        let mut result = [NOP; 3];
        let mut ops = [Operation::Write(&header), Operation::Read(&mut result)];
        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Transfer)?;

        Ok(result.into())
    }

    /// Configures the DIO3 pin as a TCXO control switch.
    pub async fn set_dio3_as_tcxo_ctrl(
        &mut self,
        tcxo_voltage: TcxoVoltage,
        tcxo_delay: TcxoDelay,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let header = [0x97, tcxo_voltage as u8];
        let tcxo_delay: [u8; 3] = tcxo_delay.into();
        let mut ops = [Operation::Write(&header), Operation::Write(&tcxo_delay)];
        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Clears the device error register.
    pub async fn clear_device_errors(&mut self) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        self.spi
            .write(&[0x07, NOP, NOP])
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Gets the current device errors.
    pub async fn get_device_errors(&mut self) -> Result<DeviceErrors, SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let mut result = [0x17, NOP, NOP, NOP];
        self.spi
            .transfer_in_place(&mut result)
            .await
            .map_err(SpiError::Transfer)?;
        Ok(DeviceErrors::from(u16::from_le_bytes(
            result[2..].try_into().unwrap(),
        )))
    }

    /// Resets the device by pulling the NRESET pin low.
    pub async fn reset(&mut self) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        self.nrst_pin.set_low().map_err(PinError::Output)?;
        // Hold NRESET low for twice the datasheet's typical 100 µs pulse.
        self.spi
            .transaction(&mut [Operation::DelayNs(200_000)])
            .await
            .map_err(SpiError::Write)?;
        self.nrst_pin
            .set_high()
            .map_err(PinError::Output)
            .map_err(Into::into)
    }

    /// Enables or disables the antenna switch.
    pub fn set_ant_enabled(&mut self, enabled: bool) -> Result<(), TPINERR> {
        if enabled {
            self.ant_pin.set_high()
        } else {
            self.ant_pin.set_low()
        }
    }

    /// Configures the interrupt (IRQ) masks for the DIO pins.
    pub async fn set_dio_irq_params(
        &mut self,
        irq_mask: IrqMask,
        dio1_mask: IrqMask,
        dio2_mask: IrqMask,
        dio3_mask: IrqMask,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let irq = (Into::<u16>::into(irq_mask)).to_be_bytes();
        let dio1 = (Into::<u16>::into(dio1_mask)).to_be_bytes();
        let dio2 = (Into::<u16>::into(dio2_mask)).to_be_bytes();
        let dio3 = (Into::<u16>::into(dio3_mask)).to_be_bytes();
        let mut ops = [
            Operation::Write(&[0x08]),
            Operation::Write(&irq),
            Operation::Write(&dio1),
            Operation::Write(&dio2),
            Operation::Write(&dio3),
        ];
        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Transfer)
            .map_err(Into::into)
    }

    /// Gets the current IRQ status.
    pub async fn get_irq_status(&mut self) -> Result<IrqStatus, SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let mut status = [NOP, NOP, NOP];
        let mut ops = [Operation::Write(&[0x12]), Operation::Read(&mut status)];
        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Transfer)?;
        let irq_status: [u8; 2] = [status[1], status[2]];
        Ok(u16::from_be_bytes(irq_status).into())
    }

    /// Clears the specified IRQ status flags.
    pub async fn clear_irq_status(
        &mut self,
        mask: IrqMask,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let mask = Into::<u16>::into(mask).to_be_bytes();
        let mut ops = [Operation::Write(&[0x02]), Operation::Write(&mask)];
        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Puts the device in TX mode.
    pub async fn set_tx(
        &mut self,
        timeout: RxTxTimeout,
    ) -> Result<Status, SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let mut buf = [0x83u8; 4];
        let timeout: [u8; 3] = timeout.into();
        buf[1..].copy_from_slice(&timeout);

        self.spi
            .transfer_in_place(&mut buf)
            .await
            .map_err(SpiError::Transfer)?;
        Ok(buf[0].into())
    }

    /// Puts the device in RX mode.
    pub async fn set_rx(
        &mut self,
        timeout: RxTxTimeout,
    ) -> Result<Status, SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let mut buf = [0x82u8; 4];
        let timeout: [u8; 3] = timeout.into();
        buf[1..].copy_from_slice(&timeout);

        self.spi
            .transfer_in_place(&mut buf)
            .await
            .map_err(SpiError::Transfer)?;
        Ok(buf[0].into())
    }

    /// Sets the packet parameters.
    pub async fn set_packet_params(
        &mut self,
        params: PacketParams,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let params: [u8; 9] = params.into();
        let mut ops = [Operation::Write(&[0x8C]), Operation::Write(&params)];
        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Sets the modulation parameters.
    pub async fn set_mod_params(
        &mut self,
        params: ModParams,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let params: [u8; 8] = params.into();
        let mut ops = [Operation::Write(&[0x8B]), Operation::Write(&params)];
        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Sets the TX parameters (output power, ramp time).
    pub async fn set_tx_params(
        &mut self,
        params: TxParams,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let params: [u8; 2] = params.into();
        let mut ops = [Operation::Write(&[0x8E]), Operation::Write(&params)];
        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Sets the Over Current Protection (OCP) level in milliamps.
    pub async fn set_ocp(&mut self, ocp_ma: u8) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        // OCP uses a five-bit value in 2.5 mA steps.
        let ocp_val = (ocp_ma as f32 / 2.5) as u8;
        self.write_register(Register::OcpConfiguration, &[ocp_val])
            .await
    }

    /// Sets the RF frequency.
    pub async fn set_rf_frequency(
        &mut self,
        rf_freq: u32,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let rf_freq = rf_freq.to_be_bytes();
        let mut ops = [Operation::Write(&[0x86]), Operation::Write(&rf_freq)];
        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Sets the Power Amplifier (PA) configuration.
    pub async fn set_pa_config(
        &mut self,
        pa_config: PaConfig,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let pa_config: [u8; 4] = pa_config.into();
        let mut ops = [Operation::Write(&[0x95]), Operation::Write(&pa_config)];
        self.spi
            .transaction(&mut ops)
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// Applies a fix for sensitivity issues on some modules.
    pub async fn fix_sensitivity(&mut self) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        self.write_register(Register::RxGain, &[0x96]).await
    }

    /// Configures the base addresses for the TX and RX buffers.
    pub async fn set_buffer_base_address(
        &mut self,
        tx_base_addr: u8,
        rx_base_addr: u8,
    ) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        self.spi
            .write(&[0x8F, tx_base_addr, rx_base_addr])
            .await
            .map_err(SpiError::Write)
            .map_err(Into::into)
    }

    /// A high-level method to send a message.
    ///
    /// This method writes the data to the buffer, puts the device in TX mode,
    /// and waits until the transmission is complete or a timeout occurs.
    pub async fn write_bytes(
        &mut self,
        data: &[u8],
        timeout: RxTxTimeout,
        preamble_len: u16,
        crc_type: packet::LoRaCrcType,
    ) -> Result<Status, SxError<TSPIERR, TPINERR>> {
        use packet::LoRaPacketParams;
        log::trace!("lora::write_bytes {data:?}");

        self.clear_device_errors().await?;
        self.clear_irq_status(IrqMask::all()).await?;

        self.write_buffer(0x00, data).await?;
        log::trace!(
            "lora::write_bytes buffer written. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        let params = LoRaPacketParams::default()
            .set_header_type(LoRaHeaderType::VarLen)
            .set_preamble_len(preamble_len)
            .set_payload_len(data.len() as u8)
            .set_crc_type(crc_type)
            .into();

        self.set_packet_params(params).await?;
        log::trace!(
            "lora::write_bytes packet params set. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        self.fix_sensitivity().await?;
        log::trace!(
            "lora::write_bytes sensitivity fixed. busy: {}, dio1: {}",
            self.is_busy(),
            self.is_dio1_high()
        );

        let errors = self.get_device_errors().await?;
        log::trace!("lora::write_bytes device errors: {errors:?}");

        let status = self.set_tx(timeout).await?;
        log::trace!(
            "lora::write_bytes tx mode set, status: {:?}. busy: {}, dio1: {}",
            status,
            self.is_busy(),
            self.is_dio1_high()
        );

        log::trace!("lora::write_bytes waiting on DIO1");
        self.wait_on_dio1().await?;
        log::trace!("lora::write_bytes DIO1 went high");

        self.clear_irq_status(IrqMask::all()).await?;
        log::trace!("lora::write_bytes IRQ cleared");

        Ok(status)
    }

    /// Gets the length and buffer start pointer of the last received packet.
    pub async fn get_rx_buffer_status(
        &mut self,
    ) -> Result<RxBufferStatus, SxError<TSPIERR, TPINERR>> {
        self.wait_on_busy().await?;
        let mut result = [0x13, NOP, NOP, NOP];
        self.spi
            .transfer_in_place(&mut result)
            .await
            .map_err(SpiError::Transfer)?;
        log::trace!("lora::get_rx_buffer_status raw response: {result:?}");
        Ok(TryInto::<[u8; 2]>::try_into(&result[2..]).unwrap().into())
    }

    /// Checks if the modem's BUSY pin is high.
    pub fn is_busy(&mut self) -> bool {
        self.busy_pin.is_high().unwrap_or(true)
    }

    /// Checks if the modem's DIO1 pin is high.
    pub fn is_dio1_high(&mut self) -> bool {
        self.dio1_pin.is_high().unwrap_or(false)
    }

    /// Waits until the BUSY pin goes low.
    pub async fn wait_on_busy(&mut self) -> Result<(), SxError<TSPIERR, TPINERR>> {
        self.spi
            .transaction(&mut [Operation::DelayNs(1000)])
            .await
            .map_err(SpiError::Transfer)?;
        self.busy_pin
            .wait_for_low()
            .await
            .map_err(PinError::Input)?;
        Ok(())
    }

    /// Waits until the DIO1 pin goes high.
    pub async fn wait_on_dio1(&mut self) -> Result<(), PinError<TPINERR>> {
        self.dio1_pin
            .wait_for_high()
            .await
            .map_err(PinError::Input)?;
        Ok(())
    }
}
