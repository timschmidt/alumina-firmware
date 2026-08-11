//! Compile-only TinyBee PCM-short circular-DMA composition.
//!
//! This module deliberately has no caller in the boot/runtime path. It proves
//! the selected permissive HAL surface and the ownership handoff compile, but
//! it cannot establish WS/data phase, physical latch observation, underrun
//! behavior, or a qualified safe stop.

use alumina_shift_register::PcmShortMonoFrame;
use esp_hal::Blocking;
use esp_hal::dma::DmaTransferTxCircular;
use esp_hal::i2s::master::{Channels, Config, DataFormat, Error as I2sError, I2s, I2sTx};
use esp_hal::time::Rate;

use super::{EstablishedRealtimeResources, SafetyInputBank, tinybee_safe_image};

/// Model-only capture rate used to compile the initial HAL composition.
/// This is not a selected or measured TinyBee frequency.
pub const UNQUALIFIED_PCM_SHORT_FRAME_RATE_HZ: u32 = 250_000;
/// Safe frames retained by the compile-only internal-SRAM circular buffer.
pub const UNQUALIFIED_PCM_SHORT_DMA_FRAMES: usize = 256;
const DMA_BYTES: usize = UNQUALIFIED_PCM_SHORT_DMA_FRAMES * size_of::<u32>();

/// Failure while constructing or exercising the unqualified HAL surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnqualifiedPcmShortError {
    /// The repository's fixed complete safe image was invalid.
    SafeImage,
    /// The pinned HAL rejected the fixed PCM-short configuration.
    Configuration,
    /// The pinned HAL or DMA engine rejected a transfer operation.
    Transfer,
}

/// Consumed TinyBee RT resources after an explicit static-to-I²S ownership
/// handoff, with the circular transfer deliberately not started.
///
/// Keeping the TX object and buffer separate avoids a self-referential owner.
/// A future core-1 loop can borrow both for a circular transfer, stop that
/// transfer, and then issue a complete safe one-shot sample through the same
/// routed peripheral. None of those transitions is qualified yet.
#[allow(
    dead_code,
    reason = "compile-only HIL surface remains unreachable until waveform qualification"
)]
pub struct UnqualifiedPcmShortResources {
    tx: I2sTx<'static, Blocking>,
    tx_buffer: &'static mut [u8; DMA_BYTES],
    timer_group1: esp_hal::peripherals::TIMG1<'static>,
    probe_servo: esp_hal::gpio::Input<'static>,
    safety_inputs: SafetyInputBank<4>,
    thermistor_1_sd_detect: esp_hal::peripherals::GPIO34<'static>,
    thermistor_0: esp_hal::peripherals::GPIO36<'static>,
    thermistor_bed: esp_hal::peripherals::GPIO39<'static>,
    adc1: esp_hal::peripherals::ADC1<'static>,
}

impl EstablishedRealtimeResources {
    /// Consumes the proven static-safe owner and composes the pinned HAL's
    /// original-ESP32 PCM-short TX path with a safe-prefilled circular buffer.
    ///
    /// This method is intentionally unused. Calling it would relinquish the
    /// qualified static GPIO writer before the I²S phase and stop path have HIL
    /// evidence, so no production or arm path may reference it.
    #[allow(
        dead_code,
        reason = "compile-only HIL surface remains unreachable until waveform qualification"
    )]
    pub fn into_unqualified_pcm_short(
        self,
    ) -> Result<UnqualifiedPcmShortResources, UnqualifiedPcmShortError> {
        let EstablishedRealtimeResources {
            timer_group1,
            i2s0,
            i2s_dma,
            safe_shift,
            probe_servo,
            safety_inputs,
            thermistor_1_sd_detect,
            thermistor_0,
            thermistor_bed,
            adc1,
        } = self;
        let (clock, data, latch, _delay) = safe_shift.into_parts();
        let safe = PcmShortMonoFrame::new(tinybee_safe_image())
            .map_err(|_| UnqualifiedPcmShortError::SafeImage)?;
        let (_, _, tx_buffer, tx_descriptors) = esp_hal::dma_circular_buffers!(0, DMA_BYTES);
        for word in tx_buffer.chunks_exact_mut(size_of::<u32>()) {
            word.copy_from_slice(&safe.dma_bytes_le());
        }

        let i2s = I2s::new(
            i2s0,
            i2s_dma,
            Config::new_tdm_pcm_short()
                .with_sample_rate(Rate::from_hz(UNQUALIFIED_PCM_SHORT_FRAME_RATE_HZ))
                .with_data_format(DataFormat::Data32Channel32)
                .with_channels(Channels::MONO),
        )
        .map_err(|_| UnqualifiedPcmShortError::Configuration)?;
        let tx = i2s
            .i2s_tx
            .with_bclk(clock)
            .with_ws(latch)
            .with_dout(data)
            .build(tx_descriptors);

        Ok(UnqualifiedPcmShortResources {
            tx,
            tx_buffer,
            timer_group1,
            probe_servo,
            safety_inputs,
            thermistor_1_sd_detect,
            thermistor_0,
            thermistor_bed,
            adc1,
        })
    }
}

#[allow(
    dead_code,
    reason = "compile-only HIL surface remains unreachable until waveform qualification"
)]
impl UnqualifiedPcmShortResources {
    /// Starts a safe-prefilled circular transfer borrowed from the retained
    /// owner. Dropping or stopping the returned guard stops the peripheral.
    pub fn start_safe_capture(
        &mut self,
    ) -> Result<DmaTransferTxCircular<'_, I2sTx<'static, Blocking>>, UnqualifiedPcmShortError> {
        self.tx
            .write_dma_circular(&self.tx_buffer)
            .map_err(map_i2s_error)
    }

    /// Sends two safe frames after a circular guard has stopped so the modeled
    /// one-frame pipeline contains a following latch boundary. Its physical
    /// phase and stop safety remain deliberately unclaimed.
    pub fn rewrite_safe_pipeline(&mut self) -> Result<(), UnqualifiedPcmShortError> {
        let sample = PcmShortMonoFrame::new(tinybee_safe_image())
            .map_err(|_| UnqualifiedPcmShortError::SafeImage)?
            .sample_word();
        self.tx
            .write_words(&[sample, sample])
            .map_err(map_i2s_error)
    }

    /// Exact safe-prefilled byte capacity exposed for HIL harness assertions.
    pub const fn dma_bytes(&self) -> usize {
        DMA_BYTES
    }
}

const fn map_i2s_error(_error: I2sError) -> UnqualifiedPcmShortError {
    UnqualifiedPcmShortError::Transfer
}
