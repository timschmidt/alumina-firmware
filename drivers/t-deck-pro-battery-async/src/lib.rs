//! An asynchronous, `no_std` driver for the BQ25896 battery charger IC used in the T-Deck.
//!
//! [`BatteryService`] controls ADC conversion and returns decoded [`BatteryData`].
//! See the package README and `simple_battery` example for integration details.

#![no_std]

use embassy_time::{Duration, Timer};
use embedded_hal_async::i2c::{I2c, SevenBitAddress};
use log::error;

// Register addresses.
const BQ25896_I2C_ADDR: u8 = 0x6B;
const ADC_CTRL_REG: u8 = 0x02;
const SYS_STATUS_REG: u8 = 0x0B;
const FAULT_STATUS_REG: u8 = 0x0C;
const VBUS_ADC_REG: u8 = 0x11;
const CHARGE_CURRENT_ADC_REG: u8 = 0x12;
const BATTERY_TEMP_ADC_REG: u8 = 0x10;
const VOLTAGE_READ_REG: u8 = 0x0E;

// ADC control values.
const ADC_ENABLE_VALUE: u8 = 0xC0;
const ADC_DISABLE_VALUE: u8 = 0x40;

// REG0B system-status fields.
const VBUS_STATUS_MASK: u8 = 0b1110_0000;
const VBUS_STATUS_SHIFT: u8 = 5;
const CHARGE_STATUS_MASK: u8 = 0b0001_1000;
const CHARGE_STATUS_SHIFT: u8 = 3;
const POWER_GOOD_MASK: u8 = 0b0000_0100;

// REG0C fault-status fields.
const WATCHDOG_FAULT_MASK: u8 = 0b1000_0000;
const BOOST_FAULT_MASK: u8 = 0b0100_0000;
const CHRG_FAULT_MASK: u8 = 0b0011_0000;
const CHRG_FAULT_SHIFT: u8 = 4;
const BAT_FAULT_MASK: u8 = 0b0000_1000;
const NTC_FAULT_MASK: u8 = 0b0000_0111;

/// Represents the charging status of the battery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargingStatus {
    /// Not charging.
    NotCharging,
    /// Pre-charge phase.
    PreCharge,
    /// Fast charge phase.
    FastCharge,
    /// Charging is complete.
    ChargeDone,
}

/// Represents the status of the VBUS (input power).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VbusStatus {
    /// No input power.
    NoInput,
    /// USB host is the power source.
    UsbHost,
    /// An adapter is the power source.
    Adapter,
    /// On-The-Go (OTG) mode.
    Otg,
    /// The power source is unknown.
    Unknown,
}

/// Represents a fault condition related to charging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeFault {
    /// No fault.
    Normal,
    /// Input fault (VBUS OVP or bad source).
    InputFault,
    /// Thermal shutdown.
    ThermalShutdown,
    /// Charge timer expiration.
    TimerExpiration,
}

/// Represents a fault condition related to the NTC thermistor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtcFault {
    /// No fault.
    Normal,
    /// Temperature is too cold.
    Cold,
    /// Temperature is cool.
    Cool,
    /// Temperature is warm.
    Warm,
    /// Temperature is too hot.
    Hot,
}

/// Represents the fault conditions reported by the IC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaultStatus {
    /// Watchdog timer expired.
    pub watchdog_fault: bool,
    /// Boost mode fault.
    pub boost_fault: bool,
    /// Charging-related fault.
    pub charge_fault: ChargeFault,
    /// Battery over-voltage fault.
    pub battery_ovp_fault: bool,
    /// NTC thermistor fault.
    pub ntc_fault: NtcFault,
}

impl Default for FaultStatus {
    fn default() -> Self {
        Self {
            watchdog_fault: false,
            boost_fault: false,
            charge_fault: ChargeFault::Normal,
            battery_ovp_fault: false,
            ntc_fault: NtcFault::Normal,
        }
    }
}

/// Holds a comprehensive set of data read from the BQ25896.
#[derive(Debug, Clone, Copy)]
pub struct BatteryData {
    /// Measured battery voltage in volts.
    pub voltage: f32,
    /// Measured VBUS input voltage in volts.
    pub vbus_voltage: f32,
    /// Measured charge current in amperes.
    pub charge_current: f32,
    /// The measured battery temperature as a percentage of VREGN.
    pub battery_temp_percent: f32,
    /// The current charging status.
    pub charging_status: ChargingStatus,
    /// The status of the VBUS input.
    pub vbus_status: VbusStatus,
    /// Whether the input power source is valid.
    pub power_good: bool,
    /// Reports any fault conditions.
    pub faults: FaultStatus,
}

/// A service for interacting with the BQ25896 battery charger IC.
pub struct BatteryService<
    I2cType: I2c<SevenBitAddress, Error = ErrorType>,
    ErrorType: embedded_hal_async::i2c::Error,
> {
    i2c: I2cType,
    adc_enabled: bool,
}

impl<I2cType: I2c<SevenBitAddress, Error = ErrorType>, ErrorType: embedded_hal_async::i2c::Error>
    BatteryService<I2cType, ErrorType>
{
    /// Creates a new `BatteryService`.
    ///
    /// `i2c` may be a direct bus or a compatible shared-bus handle.
    pub fn new(i2c: I2cType) -> Self {
        Self {
            i2c,
            adc_enabled: false,
        }
    }

    /// Enables the BQ25896's ADC for measurement.
    ///
    /// The ADC is automatically enabled by the `measure` function if needed,
    /// but this method can be used to enable it manually beforehand.
    pub async fn enable_adc(&mut self) -> Result<(), ()> {
        if self.adc_enabled {
            return Ok(());
        }
        self.i2c
            .write(BQ25896_I2C_ADDR, &[ADC_CTRL_REG, ADC_ENABLE_VALUE])
            .await
            .map_err(|e| error!("Failed to enable ADC: {e:?}"))?;
        self.adc_enabled = true;
        Ok(())
    }

    /// Disables the BQ25896's ADC to save power.
    ///
    /// Call this after a sampling burst when continuous conversion is unnecessary.
    pub async fn disable_adc(&mut self) -> Result<(), ()> {
        if !self.adc_enabled {
            return Ok(());
        }
        self.i2c
            .write(BQ25896_I2C_ADDR, &[ADC_CTRL_REG, ADC_DISABLE_VALUE])
            .await
            .map_err(|e| error!("Failed to disable ADC: {e:?}"))?;
        self.adc_enabled = false;
        Ok(())
    }

    /// Reads and returns all available data from the BQ25896.
    ///
    /// This function will automatically enable the ADC if it is not already enabled.
    /// It reads all relevant registers from the IC and parses them into a `BatteryData` struct.
    pub async fn measure(&mut self) -> Result<BatteryData, ()> {
        if !self.adc_enabled {
            self.enable_adc().await?;
            Timer::after(Duration::from_secs(1)).await;
        }

        let mut vbat_buf = [0u8; 1];
        self.i2c
            .write_read(BQ25896_I2C_ADDR, &[VOLTAGE_READ_REG], &mut vbat_buf)
            .await
            .map_err(|e| error!("I2C Error: {e:?}"))?;

        let mut vbus_buf = [0u8; 1];
        self.i2c
            .write_read(BQ25896_I2C_ADDR, &[VBUS_ADC_REG], &mut vbus_buf)
            .await
            .map_err(|e| error!("I2C Error: {e:?}"))?;

        let mut ichg_buf = [0u8; 1];
        self.i2c
            .write_read(BQ25896_I2C_ADDR, &[CHARGE_CURRENT_ADC_REG], &mut ichg_buf)
            .await
            .map_err(|e| error!("I2C Error: {e:?}"))?;

        let mut ts_buf = [0u8; 1];
        self.i2c
            .write_read(BQ25896_I2C_ADDR, &[BATTERY_TEMP_ADC_REG], &mut ts_buf)
            .await
            .map_err(|e| error!("I2C Error: {e:?}"))?;

        let mut status_buf = [0u8; 1];
        self.i2c
            .write_read(BQ25896_I2C_ADDR, &[SYS_STATUS_REG], &mut status_buf)
            .await
            .map_err(|e| error!("I2C Error: {e:?}"))?;

        let mut fault_buf = [0u8; 1];
        self.i2c
            .write_read(BQ25896_I2C_ADDR, &[FAULT_STATUS_REG], &mut fault_buf)
            .await
            .map_err(|e| error!("I2C Error: {e:?}"))?;

        let status_byte = status_buf[0];
        let fault_byte = fault_buf[0];

        let charging_status = match (status_byte & CHARGE_STATUS_MASK) >> CHARGE_STATUS_SHIFT {
            0b00 => ChargingStatus::NotCharging,
            0b01 => ChargingStatus::PreCharge,
            0b10 => ChargingStatus::FastCharge,
            _ => ChargingStatus::ChargeDone,
        };

        let vbus_status = match (status_byte & VBUS_STATUS_MASK) >> VBUS_STATUS_SHIFT {
            0b000 => VbusStatus::NoInput,
            0b001 => VbusStatus::UsbHost,
            0b010 => VbusStatus::Adapter,
            0b111 => VbusStatus::Otg,
            _ => VbusStatus::Unknown,
        };

        let power_good = (status_byte & POWER_GOOD_MASK) != 0;

        let charge_fault = match (fault_byte & CHRG_FAULT_MASK) >> CHRG_FAULT_SHIFT {
            0b01 => ChargeFault::InputFault,
            0b10 => ChargeFault::ThermalShutdown,
            0b11 => ChargeFault::TimerExpiration,
            _ => ChargeFault::Normal,
        };

        let ntc_fault = match fault_byte & NTC_FAULT_MASK {
            0b001 => NtcFault::Cold,
            0b010 => NtcFault::Cool,
            0b101 => NtcFault::Warm,
            0b110 => NtcFault::Hot,
            _ => NtcFault::Normal,
        };

        let faults = FaultStatus {
            watchdog_fault: (fault_byte & WATCHDOG_FAULT_MASK) != 0,
            boost_fault: (fault_byte & BOOST_FAULT_MASK) != 0,
            charge_fault,
            battery_ovp_fault: (fault_byte & BAT_FAULT_MASK) != 0,
            ntc_fault,
        };

        // Scale ADC codes according to the BQ25896 register definitions.
        let voltage = 2.304 + ((vbat_buf[0] & 0x7F) as f32 * 0.020);
        let vbus_voltage = 2.6 + ((vbus_buf[0] & 0x7F) as f32 * 0.100);
        let charge_current = (ichg_buf[0] & 0x7F) as f32 * 50.0 / 1000.0;
        let battery_temp_percent = 21.0 + ((ts_buf[0] & 0x7F) as f32 * 0.465);

        Ok(BatteryData {
            voltage,
            vbus_voltage,
            charge_current,
            battery_temp_percent,
            charging_status,
            vbus_status,
            power_good,
            faults,
        })
    }
}
