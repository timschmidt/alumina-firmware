//! An asynchronous, `no_std` driver for the GPS module on the LilyGo T-Deck.
//!
//! [`Gps`] configures the module with UBX messages and returns parsed NMEA data
//! as [`GpsData`]. See the package README and `simple_gps` example for wiring.
#![no_std]

use chrono::{Datelike, Timelike};
use embassy_time::{Duration, Timer};
use esp_hal::{
    Async,
    gpio::{AnyPin, Output},
    peripherals::UART1,
    uart::{Config, Uart},
};
use heapless::String;
use log::{info, trace};
use nmea::Nmea;

/// Represents the power mode of the GPS module.
#[derive(Debug, Clone, Copy)]
pub enum PowerMode {
    /// Continuous operation with a two-second measurement interval.
    Normal,
    /// Continuous operation with a caller-selected measurement interval.
    Eco { update_rate_ms: u16 },
    /// Software standby mode.
    SoftwareStandby,
}

/// Represents a time of day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Time {
    /// Hour (0-23).
    pub hour: u8,
    /// Minute (0-59).
    pub minute: u8,
    /// Second (0-59).
    pub second: u8,
}

/// Represents a calendar date.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Date {
    /// Year.
    pub year: u16,
    /// Month (1-12).
    pub month: u8,
    /// Day (1-31).
    pub day: u8,
}

/// Represents the type of GPS fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixType {
    /// No fix.
    NoFix,
    /// Standard GPS fix.
    Gps,
    /// Differential GPS fix.
    DGps,
    /// Precise Positioning Service fix.
    Pps,
    /// Real Time Kinematic fix.
    Rtk,
    /// Float Real Time Kinematic fix.
    FloatRtk,
    /// Estimated (dead reckoning) mode.
    Estimated,
    /// Manual input mode.
    Manual,
    /// Simulation mode.
    Simulation,
}

impl From<nmea::sentences::FixType> for FixType {
    fn from(fix_type: nmea::sentences::FixType) -> Self {
        match fix_type {
            nmea::sentences::FixType::Invalid => FixType::NoFix,
            nmea::sentences::FixType::Gps => FixType::Gps,
            nmea::sentences::FixType::DGps => FixType::DGps,
            nmea::sentences::FixType::Pps => FixType::Pps,
            nmea::sentences::FixType::Rtk => FixType::Rtk,
            nmea::sentences::FixType::FloatRtk => FixType::FloatRtk,
            nmea::sentences::FixType::Estimated => FixType::Estimated,
            nmea::sentences::FixType::Manual => FixType::Manual,
            nmea::sentences::FixType::Simulation => FixType::Simulation,
        }
    }
}

/// A structure to hold parsed GPS data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GpsData {
    /// The time of the fix, if available.
    pub fix_time: Option<Time>,
    /// The date of the fix, if available.
    pub fix_date: Option<Date>,
    /// The type of fix, if available.
    pub fix_type: Option<FixType>,
    /// The latitude in decimal degrees, if available.
    pub latitude: Option<f64>,
    /// The longitude in decimal degrees, if available.
    pub longitude: Option<f64>,
    /// The speed over ground in knots, if available.
    pub speed_over_ground: Option<f32>,
}

impl GpsData {
    /// Returns `true` if a valid fix has been acquired.
    #[must_use]
    pub fn has_fix(&self) -> bool {
        self.fix_type.is_some() && self.fix_type != Some(FixType::NoFix)
    }
}

/// A driver for the T-Deck GPS module.
pub struct Gps<'d> {
    uart: Uart<'d, Async>,
    _enable_pin: Output<'d>,
    nmea: Nmea,
}

impl<'d> Gps<'d> {
    /// Creates a new `Gps` driver instance.
    ///
    /// This initializes the UART peripheral for communication with the GPS module
    /// and ensures the module is powered on via the enable pin.
    ///
    /// Configures UART1 for 38,400 baud and asserts `enable_pin`.
    pub fn new(
        uart1: UART1<'d>,
        tx: AnyPin<'d>,
        rx: AnyPin<'d>,
        mut enable_pin: Output<'d>,
    ) -> Self {
        let config = Config::default().with_baudrate(38400);
        let uart = Uart::new(uart1, config).unwrap().with_tx(tx).with_rx(rx);

        enable_pin.set_high();

        Self {
            uart: uart.into_async(),
            _enable_pin: enable_pin,
            nmea: Nmea::default(),
        }
    }

    /// Sets the power mode of the GPS module by sending a UBX message.
    ///
    pub async fn set_power_mode(&mut self, mode: PowerMode) -> Result<(), ()> {
        const UNLOCK_CONFIG: &[u8] = &[
            0x01, 0x01, 0x00, 0x00, // Version, Layers (RAM only), Reserved
            0x03, 0x00, 0xE1, 0x20, // Key: CFG-SEC-CFG_LOCK
            0x00, // Value: false
        ];

        match mode {
            PowerMode::SoftwareStandby => {
                let (class, id, payload) = (
                    0x02,
                    0x41,
                    &[0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00],
                );
                self.send_ubx_message(class, id, payload).await?;
                Timer::after(Duration::from_millis(20)).await;
                Ok(())
            }
            _ => {
                if self
                    .send_ubx_message(
                        0x06, // Class: CFG
                        0x8A, // ID: VALSET
                        UNLOCK_CONFIG,
                    )
                    .await
                    .is_err()
                {
                    log::warn!("Failed sending unlock command");
                }
                log::trace!("Unlock request ack: {:?}", self.wait_for_ack().await);

                let (class, id, payload): (u8, u8, &[u8]) = match mode {
                    PowerMode::Normal => (
                        0x06,
                        0x8A,
                        &[
                            0x01, 0x01, 0x00, 0x00, // Version, RAM layer, reserved
                            0x01, 0x00, 0x21, 0x30, // Key: CFG-RATE-MEAS
                            0xD0, 0x07, // Value: 2000 ms (little-endian 0x07D0)
                        ],
                    ),
                    PowerMode::Eco { update_rate_ms } => {
                        let rate = to_little_endian_bytes(update_rate_ms);
                        (
                            0x06,
                            0x8A,
                            &[
                                0x01, 0x01, 0x00, 0x00, // Version, RAM layer, reserved
                                0x01, 0x00, 0x21, 0x30, // Key: CFG-RATE-MEAS
                                rate[0], rate[1],
                            ],
                        )
                    }
                    PowerMode::SoftwareStandby => unreachable!(),
                };
                self.send_ubx_message(class, id, payload).await?;
                self.wait_for_ack().await
            }
        }
    }

    /// Performs a cold start of the GPS to recover from an unresponsive state.
    pub async fn recovery(&mut self) -> Result<(), ()> {
        // UBX-CFG-RST with a cold-start reset mode.
        let payload = [0x00, 0x00, 0x01, 0x00];
        self.send_ubx_message(0x06, 0x04, &payload).await?;
        self.wait_for_ack().await
    }

    /// Waits for an ACK/NACK message from the GPS module.
    async fn wait_for_ack(&mut self) -> Result<(), ()> {
        let mut buffer = [0u8; 10]; // UBX ACK/NAK messages are 10 bytes long
        let mut pos = 0;

        loop {
            match self.uart.read_async(&mut buffer[pos..pos + 1]).await {
                Ok(1) => {
                    if pos == 0 && buffer[0] != 0xB5 {
                        continue;
                    }
                    if pos == 1 && buffer[1] != 0x62 {
                        pos = 0;
                        continue;
                    }

                    pos += 1;

                    if pos == buffer.len() {
                        // UBX acknowledgements are fixed ten-byte messages.
                        if buffer[2] == 0x05 {
                            if buffer[3] == 0x01 {
                                info!("Received ACK from GPS");
                                return Ok(());
                            } else if buffer[3] == 0x00 {
                                info!("Received NACK from GPS");
                                return Err(());
                            }
                        }
                        // Discard unrelated traffic and resume synchronization.
                        pos = 0;
                    }
                }
                Ok(_) => {} // Should not happen with a read of size 1
                Err(_) => return Err(()),
            }
        }
    }

    /// Sends a UBX protocol message to the GPS module.
    async fn send_ubx_message(&mut self, class: u8, id: u8, payload: &[u8]) -> Result<(), ()> {
        const HEADER_LEN: usize = 6;
        const CHECKSUM_LEN: usize = 2;
        const MAX_PAYLOAD_LEN: usize = 256 - HEADER_LEN - CHECKSUM_LEN;

        // Reject payloads that cannot fit beside the UBX header and checksum.
        if payload.len() > MAX_PAYLOAD_LEN {
            log::error!("UBX payload too large: {} bytes", payload.len());
            return Err(());
        }

        let mut message = [0u8; 256];
        message[0] = 0xB5;
        message[1] = 0x62;
        message[2] = class;
        message[3] = id;
        message[4] = (payload.len() & 0xFF) as u8;
        message[5] = (payload.len() >> 8) as u8;

        let payload_end = HEADER_LEN + payload.len();
        message[HEADER_LEN..payload_end].copy_from_slice(payload);

        let checksum = self.ubx_checksum(&message[2..payload_end]);
        message[payload_end] = checksum.0;
        message[payload_end + 1] = checksum.1;

        let total_len = payload_end + CHECKSUM_LEN;

        log::info!("Sending {:x?}", &message[..total_len]);

        self.uart
            .write_async(&message[..total_len])
            .await
            .map_err(|_| ())
            .map(|_| {})
    }

    /// Calculates the 8-bit Fletcher checksum used in UBX messages.
    fn ubx_checksum(&self, data: &[u8]) -> (u8, u8) {
        log::info!("Calculating checksum of {:x?}", &data);
        let mut ck_a: u8 = 0;
        let mut ck_b: u8 = 0;
        for byte in data {
            ck_a = ck_a.wrapping_add(*byte);
            ck_b = ck_b.wrapping_add(ck_a);
        }
        (ck_a, ck_b)
    }

    /// Reads and parses a complete NMEA message from the GPS module.
    ///
    /// This function continuously reads from the UART, assembling messages line by line.
    /// When a complete sentence is received that results in a valid fix, it returns
    /// a `GpsData` struct.
    pub async fn read_message(&mut self) -> Result<GpsData, ()> {
        let mut buffer = [0u8; 1024];
        let mut message_buffer = [0u8; 1024];
        let mut message_len = 0;
        loop {
            let result = self.uart.read_async(&mut buffer).await;
            match result {
                Ok(len) => {
                    for &byte in &buffer[..len] {
                        if byte == b'\n' {
                            if message_len > 0 {
                                match String::<1024>::from_utf8(
                                    heapless::Vec::from_slice(&message_buffer[..message_len])
                                        .unwrap(),
                                ) {
                                    Ok(sentence) => {
                                        trace!("Received raw GPS message: {sentence}");
                                        match self.nmea.parse(&sentence) {
                                            Ok(_) => {
                                                message_len = 0;
                                                if self.nmea.fix_time.is_some()
                                                    && self.nmea.fix_date.is_some()
                                                    && self.nmea.fix_type.is_some()
                                                    && self.nmea.latitude.is_some()
                                                    && self.nmea.longitude.is_some()
                                                {
                                                    let fix_time =
                                                        self.nmea.fix_time.map(|t| Time {
                                                            hour: t.hour() as u8,
                                                            minute: t.minute() as u8,
                                                            second: t.second() as u8,
                                                        });
                                                    let fix_date =
                                                        self.nmea.fix_date.map(|d| Date {
                                                            year: d.year() as u16,
                                                            month: d.month() as u8,
                                                            day: d.day() as u8,
                                                        });
                                                    let fix_type =
                                                        self.nmea.fix_type.map(|f| f.into());

                                                    return Ok(GpsData {
                                                        fix_time,
                                                        fix_date,
                                                        fix_type,
                                                        latitude: self.nmea.latitude,
                                                        longitude: self.nmea.longitude,
                                                        speed_over_ground: self
                                                            .nmea
                                                            .speed_over_ground,
                                                    });
                                                }
                                            }
                                            Err(e) => {
                                                trace!("Error parsing NMEA sentence: {e:?}");
                                                message_len = 0;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        trace!("Error converting to string: {e:?}");
                                        message_len = 0;
                                    }
                                }
                            }
                        } else if message_len < message_buffer.len() {
                            message_buffer[message_len] = byte;
                            message_len += 1;
                        }
                    }
                }
                Err(_) => return Err(()),
            }
        }
    }
}
/// Converts a u16 value into a 2-byte array in little-endian order.
fn to_little_endian_bytes(value: u16) -> [u8; 2] {
    value.to_le_bytes()
}
