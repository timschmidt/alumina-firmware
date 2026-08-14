//! Compile-only TinyBee PCM-short circular-DMA composition.
//!
//! This module deliberately has no caller in the boot/runtime path. It proves
//! the selected permissive HAL surface and the ownership handoff compile, but
//! it cannot establish WS/data phase, physical latch observation, underrun
//! behavior, or a qualified safe stop.

use alumina_motion::{OutputCommitToken, ScheduledShiftOutput};
use alumina_protocol::DeviceCycle;
use alumina_shift_register::{
    CompleteImage, PcmShortDmaStreamOwner, PcmShortFrameGrid, PcmShortMonoFrame,
    PlannedPcmShortFrame, TaggedScheduledCompleteImage,
};
use embassy_time::TICK_HZ;
use esp_hal::Blocking;
use esp_hal::dma::{DmaError, DmaTransferTxCircular};
use esp_hal::i2s::master::{Channels, Config, DataFormat, Error as I2sError, I2s, I2sTx};
use esp_hal::time::Rate;

use super::{EstablishedRealtimeResources, SafetyInputBank, tinybee_safe_image};

/// Model-only capture rate used to compile the initial HAL composition.
/// This is not a selected or measured TinyBee frequency.
pub const UNQUALIFIED_PCM_SHORT_FRAME_RATE_HZ: u32 = 250_000;
/// Safe frames retained by the compile-only internal-SRAM circular buffer.
pub const UNQUALIFIED_PCM_SHORT_DMA_FRAMES: usize = 256;
/// Sparse complete-image capacity paired with the compile-only DMA owner.
pub const UNQUALIFIED_PCM_SHORT_UPDATES: usize = 64;
const DMA_BYTES: usize = UNQUALIFIED_PCM_SHORT_DMA_FRAMES * size_of::<u32>();

/// Portable static/stream/reclaim owner used by the disconnected-load harness.
pub type UnqualifiedPcmShortOwner = PcmShortDmaStreamOwner<
    OutputCommitToken,
    UNQUALIFIED_PCM_SHORT_UPDATES,
    UNQUALIFIED_PCM_SHORT_DMA_FRAMES,
>;

/// Failure while constructing or exercising the unqualified HAL surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnqualifiedPcmShortError {
    /// The repository's fixed complete safe image was invalid.
    SafeImage,
    /// The pinned HAL rejected the fixed PCM-short configuration.
    Configuration,
    /// The portable static/stream/reclaim owner rejected a transition.
    Ownership,
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
        // One descriptor per complete four-byte frame makes released refill
        // capacity frame-exact. It still does not prove the later FIFO/WS phase.
        let (_, _, tx_buffer, tx_descriptors) =
            esp_hal::dma_circular_buffers_chunk_size!(0, DMA_BYTES, size_of::<u32>());
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
    /// Constructs the portable ownership model from the earlier static-safe
    /// transaction, the completed safe DMA prefill, and one explicit
    /// hypothesized stream epoch. Only capture may qualify that epoch against
    /// physical WS.
    pub fn new_owner(
        static_safe_at: DeviceCycle,
        ring_prepared_at: DeviceCycle,
        stream_epoch: DeviceCycle,
    ) -> Result<UnqualifiedPcmShortOwner, UnqualifiedPcmShortError> {
        let grid =
            PcmShortFrameGrid::new(stream_epoch.0, TICK_HZ, UNQUALIFIED_PCM_SHORT_FRAME_RATE_HZ)
                .map_err(|_| UnqualifiedPcmShortError::Configuration)?;
        let mut owner = PcmShortDmaStreamOwner::new(tinybee_safe_image(), static_safe_at.0)
            .map_err(|_| UnqualifiedPcmShortError::Ownership)?;
        owner
            .prepare_safe_ring(
                grid,
                tinybee_safe_image(),
                UNQUALIFIED_PCM_SHORT_DMA_FRAMES,
                ring_prepared_at.0,
            )
            .map_err(|_| UnqualifiedPcmShortError::Ownership)?;
        Ok(owner)
    }

    /// Transfers one generated motion token into the portable target-owned
    /// sparse plan without manufacturing a replacement token. The portable
    /// owner rejects this until an independent first-safe-latch observation
    /// establishes physical stream authority.
    pub fn stage_planned_output(
        owner: &mut UnqualifiedPcmShortOwner,
        output: ScheduledShiftOutput,
    ) -> Result<(), UnqualifiedPcmShortError> {
        owner
            .stage(TaggedScheduledCompleteImage {
                tag: output.token,
                commit_cycle: output.update.at.0,
                image: CompleteImage {
                    bits: output.update.image,
                    ..tinybee_safe_image()
                },
            })
            .map_err(|_| UnqualifiedPcmShortError::Ownership)
    }

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

    /// Reads the HAL's exact whole-frame refill capacity. This is descriptor
    /// ownership evidence, not a physical latch observation.
    pub fn available_frame_slots(
        transfer: &mut DmaTransferTxCircular<'_, I2sTx<'static, Blocking>>,
    ) -> Result<usize, UnqualifiedPcmShortError> {
        let bytes = transfer.available().map_err(map_dma_error)?;
        if !bytes.is_multiple_of(size_of::<u32>()) {
            return Err(UnqualifiedPcmShortError::Transfer);
        }
        Ok(bytes / size_of::<u32>())
    }

    /// Pushes exactly one already planned dense frame into one released DMA
    /// descriptor. The portable horizon must acknowledge it only after this
    /// method succeeds.
    pub fn push_planned_frame(
        transfer: &mut DmaTransferTxCircular<'_, I2sTx<'static, Blocking>>,
        frame: PlannedPcmShortFrame,
    ) -> Result<(), UnqualifiedPcmShortError> {
        let bytes = frame.frame.dma_bytes_le();
        if transfer.push(&bytes).map_err(map_dma_error)? != bytes.len() {
            return Err(UnqualifiedPcmShortError::Transfer);
        }
        Ok(())
    }

    /// Exact safe-prefilled byte capacity exposed for HIL harness assertions.
    pub const fn dma_bytes(&self) -> usize {
        DMA_BYTES
    }
}

const fn map_i2s_error(_error: I2sError) -> UnqualifiedPcmShortError {
    UnqualifiedPcmShortError::Transfer
}

const fn map_dma_error(_error: DmaError) -> UnqualifiedPcmShortError {
    UnqualifiedPcmShortError::Transfer
}
