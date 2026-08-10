//! Core implementation of the CST328 touch controller driver.

use embassy_time::{Duration, Timer, with_timeout};
use embedded_hal_async::i2c::{I2c, SevenBitAddress};
use esp_hal::gpio::{Input, Output};
use heapless::Vec;

const I2C_ADDRESS: u8 = 0x1A;

// CST328 registers exercised by the reference implementations.
const REG_TOUCH_DATA: u16 = 0xD000;
const REG_DEBUG_INFO_MODE: u16 = 0xD101;
const REG_NORMAL_MODE: u16 = 0xD109;

/// Represents a touch event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchEvent {
    /// A finger touched the screen.
    Down,
    /// A finger was lifted from the screen.
    Up,
    /// A finger is moving on the screen.
    Contact,
    /// No touch event.
    None,
}

/// Represents a single touch point with its coordinates and state.
#[derive(Debug, Clone, Copy)]
pub struct TouchPoint {
    /// The ID of the touch point (0-4).
    pub id: u8,
    /// The type of touch event.
    pub event: TouchEvent,
    /// The x-coordinate of the touch point.
    pub x: u16,
    /// The y-coordinate of the touch point.
    pub y: u16,
}

/// A controller for the CST328 touch interface.
pub struct TouchController<
    'a,
    I2cType: I2c<SevenBitAddress, Error = ErrorType>,
    ErrorType: embedded_hal_async::i2c::Error,
> {
    i2c: I2cType,
    int: Input<'a>,
    rst: Option<Output<'a>>,
}

impl<
    'a,
    I2cType: I2c<SevenBitAddress, Error = ErrorType>,
    ErrorType: embedded_hal_async::i2c::Error,
> TouchController<'a, I2cType, ErrorType>
{
    /// Creates a new `TouchController`.
    ///
    /// # Arguments
    ///
    /// * `i2c` - An I2C peripheral that implements `embedded-hal-async::i2c::I2c`.
    /// * `int` - The interrupt input pin from the touch controller.
    /// * `rst` - An optional output pin for resetting the controller.
    pub fn new(i2c: I2cType, int: Input<'a>, rst: Option<Output<'a>>) -> Self {
        Self { i2c, int, rst }
    }

    /// Returns whether the active-low interrupt line is asserted.
    pub fn is_touch_active(&self) -> bool {
        self.int.is_low()
    }

    /// Initializes the touch controller.
    ///
    /// This function resets the CST328 and puts it into the normal operating mode.
    pub async fn init(&mut self) -> Result<(), ()> {
        if let Some(rst) = &mut self.rst {
            rst.set_low();
            Timer::after(Duration::from_millis(10)).await;
            rst.set_high();
            Timer::after(Duration::from_millis(300)).await;
        }

        // Debug-information mode is part of the controller wake sequence.
        self.i2c
            .write(I2C_ADDRESS, &REG_DEBUG_INFO_MODE.to_be_bytes())
            .await
            .map_err(|err| {
                log::warn!("Error writing REG_DEBUG_INFO_MODE: {err:?}");
            })?;
        Timer::after(Duration::from_millis(10)).await;

        // Return to normal touch-reporting mode.
        self.i2c
            .write(I2C_ADDRESS, &REG_NORMAL_MODE.to_be_bytes())
            .await
            .map_err(|err| {
                log::warn!("Error writing REG_NORMAL_MODE: {err:?}");
            })?;
        Timer::after(Duration::from_millis(10)).await;

        Ok(())
    }

    /// Reads the current touch points from the controller.
    ///
    /// This function waits for a touch interrupt, reads the raw touch data,
    /// and returns five optional hardware slots.
    pub async fn read_touches(&mut self) -> Result<Vec<Option<TouchPoint>, 5>, ()> {
        let _ = with_timeout(Duration::from_secs(1), self.int.wait_for_low()).await;
        Timer::after(Duration::from_millis(20)).await;
        let mut data_buf = [0u8; 27];
        self.i2c
            .write_read(I2C_ADDRESS, &REG_TOUCH_DATA.to_be_bytes(), &mut data_buf)
            .await
            .map_err(|err| {
                log::warn!("Error writing/reading REG_TOUCH_DATA: {err:?}");
            })?;

        let mut result: Vec<Option<TouchPoint>, 5> = Vec::new();

        let parse_touch_point = |point_data: &[u8], finger_index: u8| -> Option<TouchPoint> {
            let state = point_data[0] & 0x0F;

            if state == 0x06 {
                // State 0x06 is the pressed/contact report used by this panel.
                let id = finger_index;
                let event = TouchEvent::Down;
                let x = ((point_data[1] as u16) << 4) | (((point_data[3] >> 4) & 0x0F) as u16);
                let y = ((point_data[2] as u16) << 4) | ((point_data[3] & 0x0F) as u16);
                Some(TouchPoint { id, event, x, y })
            } else {
                None
            }
        };

        result.push(parse_touch_point(&data_buf[0..5], 0)).unwrap();
        result.push(parse_touch_point(&data_buf[7..12], 1)).unwrap();
        result
            .push(parse_touch_point(&data_buf[12..17], 2))
            .unwrap();
        result
            .push(parse_touch_point(&data_buf[17..22], 3))
            .unwrap();
        result
            .push(parse_touch_point(&data_buf[22..27], 4))
            .unwrap();

        // Available references do not document an explicit status-clear command;
        // wait for the controller to release its interrupt as a best-effort ack.
        if with_timeout(Duration::from_secs(1), self.int.wait_for_high())
            .await
            .is_err()
        {
            log::warn!("Timeout waiting for high.");
        }
        Ok(result)
    }
}
