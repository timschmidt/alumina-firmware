#![no_std]
#![deny(missing_docs)]

//! Asynchronous framebuffer driver for the T-Deck Pro e-paper display.
//!
//! [`EInkDisplay`] implements `embedded-graphics`'s [`DrawTarget`] and provides
//! full and partial asynchronous refresh methods. See the package README and
//! `simple_example` for the complete pin and SPI setup.

use embassy_futures::yield_now;
use embassy_time::{Duration, Timer, with_timeout};
use embedded_graphics::{pixelcolor::BinaryColor, prelude::*, primitives::Rectangle};
use embedded_hal_async::spi::SpiDevice;
use esp_hal::gpio::{Input, Output};
use log::{info, trace, warn};

/// Buffered T-Deck Pro e-paper display.
///
/// This struct owns the control pins and fixed buffers for the next and
/// previously displayed frames.
pub struct EInkDisplay<'d> {
    dc: Output<'d>,
    busy: Input<'d>,
    rst: Option<Output<'d>>,
    buffer: [u8; 240 * 320 / 8],
    old_buffer: [u8; 240 * 320 / 8],
    width: u32,
    height: u32,
    use_fast_full_update: bool,
    power_is_on: bool,
    init_display_done: bool,
}

// Waveform lookup tables used by the fast full-update mode.
const LUT_WW: [u8; 43] = [
    0x01, 0x0A, 0x00, 0x00, 0x00, 0x01, 0x60, 0x14, 0x14, 0x00, 0x00, 0x01, 0x00, 0x14, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x14, 0x14, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const LUT_KW: [u8; 43] = [
    0x01, 0x8A, 0x00, 0x00, 0x00, 0x01, 0xA0, 0x14, 0x14, 0x00, 0x00, 0x01, 0x00, 0x14, 0x00, 0x00,
    0x00, 0x01, 0x90, 0x14, 0x14, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const LUT_WK: [u8; 43] = [
    0x02, 0x0A, 0x00, 0x00, 0x00, 0x01, 0x60, 0x14, 0x14, 0x00, 0x00, 0x01, 0x00, 0x14, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x14, 0x14, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const LUT_KK: [u8; 43] = [
    0x01, 0x0A, 0x00, 0x00, 0x00, 0x01, 0x60, 0x14, 0x14, 0x00, 0x00, 0x01, 0x00, 0x14, 0x00, 0x00,
    0x00, 0x01, 0x00, 0x14, 0x14, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

impl<'d> EInkDisplay<'d> {
    /// Creates a new `EInkDisplay` instance.
    ///
    /// Framebuffer storage is fixed at 240 × 320; callers should pass those
    /// dimensions. Set `use_fast_full_update` to enable the shorter waveform.
    pub fn new(
        dc: Output<'d>,
        busy: Input<'d>,
        rst: Option<Output<'d>>,
        width: u32,
        height: u32,
        use_fast_full_update: bool,
    ) -> Self {
        Self {
            dc,
            busy,
            rst,
            buffer: [0xFF; 240 * 320 / 8],
            old_buffer: [0xFF; 240 * 320 / 8],
            width,
            height,
            use_fast_full_update,
            power_is_on: false,
            init_display_done: false,
        }
    }

    /// Resets the display.
    pub async fn reset(&mut self) {
        info!("Resetting display...");
        if let Some(rst) = &mut self.rst {
            rst.set_low();
            Timer::after(Duration::from_millis(20)).await;
            rst.set_high();
            // The controller requires at least 1 ms after reset deassertion.
            Timer::after(Duration::from_millis(10)).await;
        }
        self.power_is_on = false;
        self.init_display_done = false;
        info!("Reset complete.");
    }

    /// Waits up to one second for the active-low BUSY pin to deassert.
    ///
    /// A timeout is logged and then returned as if the controller were idle.
    pub async fn wait_for_idle(&mut self) {
        trace!("Waiting for display to become idle...");
        if with_timeout(Duration::from_secs(1), self.busy.wait_for_high())
            .await
            .is_err()
        {
            warn!("Timeout waiting for display to become idle");
        } else {
            trace!("Display is idle.");
        }
    }

    /// Sends a command to the display.
    ///
    /// # Panics
    ///
    /// Panics if the SPI write fails.
    pub async fn send_command<SPI: SpiDevice<u8>>(&mut self, spi: &mut SPI, command: u8) {
        self.dc.set_low();
        trace!("Sending command: {command:#04x}");
        spi.write(&[command]).await.unwrap();
    }

    /// Sends data to the display.
    ///
    /// # Panics
    ///
    /// Panics if the SPI write fails.
    pub async fn send_data<SPI: SpiDevice<u8>>(&mut self, spi: &mut SPI, data: &[u8]) {
        self.dc.set_high();
        trace!("Sending data: {data:?}");
        spi.write(data).await.unwrap();
    }

    /// Powers on the display.
    pub async fn power_on<SPI: SpiDevice<u8>>(&mut self, spi: &mut SPI) -> Result<(), ()> {
        if !self.power_is_on {
            self.send_command(spi, 0x04).await; // PON
            if with_timeout(Duration::from_secs(1), self.busy.wait_for_high())
                .await
                .is_err()
            {
                warn!("Timeout waiting for display to become idle after power on");
                return Err(());
            } else {
                trace!("Display became idle after power on.");
            }
        }
        self.power_is_on = true;
        Ok(())
    }

    /// Powers off the display.
    pub async fn power_off<SPI: SpiDevice<u8>>(&mut self, spi: &mut SPI) -> Result<(), ()> {
        if self.power_is_on {
            self.send_command(spi, 0x02).await; // POF
            if with_timeout(Duration::from_secs(1), self.busy.wait_for_high())
                .await
                .is_err()
            {
                warn!("Timeout waiting for display to become idle after power off");
                return Err(());
            } else {
                trace!("Display became idle after power off.");
            }
        }
        self.power_is_on = false;
        Ok(())
    }

    /// Initializes the display controller.
    ///
    /// This method sends the necessary command sequence to configure the display
    /// for drawing.
    pub async fn init<SPI: SpiDevice<u8>>(&mut self, spi: &mut SPI) -> Result<(), ()> {
        if self.init_display_done {
            return Ok(());
        }

        self.reset().await;
        self.wait_for_idle().await;

        self.send_command(spi, 0x00).await; // PANEL SETTING
        self.send_data(spi, &[0x1e]).await; // soft reset
        self.send_data(spi, &[0x0d]).await;
        Timer::after(Duration::from_millis(1)).await;

        self.send_command(spi, 0x00).await; // PANEL SETTING
        self.send_data(spi, &[0x1f]).await;
        self.send_data(spi, &[0x0d]).await;

        // Load all four transition waveform tables.
        self.send_command(spi, 0x21).await; // LUTWW
        self.send_data(spi, &LUT_WW).await;
        self.send_command(spi, 0x22).await; // LUTKW
        self.send_data(spi, &LUT_KW).await;
        self.send_command(spi, 0x23).await; // LUTWK
        self.send_data(spi, &LUT_WK).await;
        self.send_command(spi, 0x24).await; // LUTKK
        self.send_data(spi, &LUT_KK).await;

        self.init_display_done = true;
        Ok(())
    }

    /// Writes the internal buffer to the display and refreshes the screen.
    ///
    /// This method sends the old and new buffer data to the display,
    /// sends the refresh command, and then powers the display off.
    pub async fn refresh_display<SPI: SpiDevice<u8>>(&mut self, spi: &mut SPI) -> Result<(), ()> {
        if !self.init_display_done {
            self.init(spi).await?;
        }

        self.power_on(spi).await?;

        self.send_command(spi, 0x10).await; // DTM1
        self.dc.set_high();
        trace!("Sending old buffer");
        spi.write(&self.old_buffer).await.unwrap();

        self.send_command(spi, 0x13).await; // DTM2
        self.dc.set_high();
        trace!("Sending new buffer");
        spi.write(&self.buffer).await.unwrap();

        // The protocol doesn't require waiting for idle after DTM2, only after DRF.

        if self.use_fast_full_update {
            self.send_command(spi, 0xE0).await; // Cascade Setting
            self.send_data(spi, &[0x02]).await;
            self.send_command(spi, 0xE5).await; // Force Temperature
            self.send_data(spi, &[0x5A]).await;
        }

        self.send_command(spi, 0x50).await; // VCOM AND DATA INTERVAL SETTING
        self.send_data(spi, &[0x97]).await;

        self.send_command(spi, 0x12).await; // DISPLAY REFRESH (DRF)
        if with_timeout(Duration::from_secs(30), self.busy.wait_for_high())
            .await
            .is_err()
        {
            trace!("Timeout waiting for display to become idle after display refresh");
            // Reset after a stalled refresh to recover a known controller state.
            self.reset().await;
            return Err(());
        } else {
            info!("Display became idle after display refresh.");
        }

        self.power_off(spi).await?;

        for (old_chunk, new_chunk) in self
            .old_buffer
            .chunks_mut(self.width as usize / 8)
            .zip(self.buffer.chunks(self.width as usize / 8))
        {
            old_chunk.copy_from_slice(new_chunk);
            yield_now().await;
        }

        // According to the manual, a refresh command may reset the controller,
        // so we mark it as needing re-initialization for the next draw operation.
        self.init_display_done = false;
        Ok(())
    }

    /// Updates a partial area of the display.
    ///
    /// This method sends the old and new buffer data for a specific rectangular
    /// region to the display and then refreshes that region.
    pub async fn refresh_partial_display<SPI: SpiDevice<u8>>(
        &mut self,
        spi: &mut SPI,
        rect: Rectangle,
    ) -> Result<(), ()> {
        self.power_on(spi).await?;

        // Define the partial window before entering partial mode.
        self.set_partial_ram_area(
            spi,
            rect.top_left.x as u16,
            rect.top_left.y as u16,
            rect.size.width as u16,
            rect.size.height as u16,
        )
        .await;

        self.send_command(spi, 0x91).await; // PTIN

        // Send the previously displayed bytes for the window.
        self.send_command(spi, 0x10).await; // DTM1
        self.dc.set_high();
        for y in rect.top_left.y..rect.top_left.y + rect.size.height as i32 {
            let start_byte =
                (y as usize * self.width as usize / 8) + (rect.top_left.x as usize / 8);
            let end_byte = start_byte + (rect.size.width as usize / 8);
            spi.write(&self.old_buffer[start_byte..end_byte])
                .await
                .unwrap();
            yield_now().await;
        }

        // Send the new framebuffer bytes for the window.
        self.send_command(spi, 0x13).await; // DTM2
        self.dc.set_high();
        for y in rect.top_left.y..rect.top_left.y + rect.size.height as i32 {
            let start_byte =
                (y as usize * self.width as usize / 8) + (rect.top_left.x as usize / 8);
            let end_byte = start_byte + (rect.size.width as usize / 8);
            spi.write(&self.buffer[start_byte..end_byte]).await.unwrap();
            yield_now().await;
        }

        self.send_command(spi, 0x12).await; // DRF
        if with_timeout(Duration::from_secs(30), self.busy.wait_for_high())
            .await
            .is_err()
        {
            warn!("Timeout waiting for display to become idle after partial refresh");
            self.reset().await;
            return Err(());
        }

        self.send_command(spi, 0x92).await; // PTOUT
        self.power_off(spi).await?;

        // The retained old frame must match the panel after the refresh.
        for y in rect.top_left.y..rect.top_left.y + rect.size.height as i32 {
            let start_byte =
                (y as usize * self.width as usize / 8) + (rect.top_left.x as usize / 8);
            let end_byte = start_byte + (rect.size.width as usize / 8);
            self.old_buffer[start_byte..end_byte]
                .copy_from_slice(&self.buffer[start_byte..end_byte]);
            yield_now().await;
        }

        Ok(())
    }

    async fn set_partial_ram_area<SPI: SpiDevice<u8>>(
        &mut self,
        spi: &mut SPI,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
    ) {
        let xe = (x + w - 1) | 0x0007;
        let ye = y + h - 1;
        let x = x & 0xFFF8;
        self.send_command(spi, 0x90).await; // partial window
        let mut data = [0u8; 7];
        data[0] = (x & 0xFF) as u8;
        data[1] = (xe & 0xFF) as u8;
        data[2] = (y >> 8) as u8;
        data[3] = (y & 0xFF) as u8;
        data[4] = (ye >> 8) as u8;
        data[5] = (ye & 0xFF) as u8;
        data[6] = 0x01;
        self.send_data(spi, &data).await;
    }
}

impl<'d> DrawTarget for EInkDisplay<'d> {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels.into_iter() {
            if let (x @ 0..=239_i32, y @ 0..=319_i32) = coord.into() {
                let byte_index = (y as usize * (self.width as usize / 8)) + (x as usize / 8);
                let bit_index = 7 - (x % 8);
                if byte_index < self.buffer.len() {
                    match color {
                        BinaryColor::On => self.buffer[byte_index] &= !(1 << bit_index), // Black
                        BinaryColor::Off => self.buffer[byte_index] |= 1 << bit_index,   // White
                    }
                }
            }
        }
        Ok(())
    }
}

impl<'d> OriginDimensions for EInkDisplay<'d> {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}
