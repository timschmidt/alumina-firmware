#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "a HIL transfer guard must stop or drop normally"
)]
#![deny(clippy::large_stack_frames)]
#![allow(
    dead_code,
    reason = "the isolated HIL binary reuses the production board split but intentionally leaves service APIs inert"
)]

#[cfg(not(feature = "hil-mks-tinybee-pcm-short-safe"))]
compile_error!("this binary requires `hil-mks-tinybee-pcm-short-safe`");

#[path = "hardware/mod.rs"]
mod hardware;
#[path = "storage.rs"]
mod storage;

use alumina_protocol::DeviceCycle;
use alumina_shift_register::{
    PcmShortDmaRefillBatchError, PcmShortDmaRefillPhase, PcmShortDmaStreamState,
    PcmShortOperationWindow,
};
use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::Output;
use esp_hal::timer::timg::TimerGroup;
use panic_rtt_target as _;

use hardware::mks_tinybee;

/// About 200 ms of steady-state traffic at the model-only 250 kHz frame rate.
const TARGET_REFILLS: u32 = 50_000;
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(2);
const STATIC_CAPTURE_INTERVAL: Duration = Duration::from_millis(50);
const MARKER_REPORT_GAP: Duration = Duration::from_millis(1);
const MARKER_SENTINEL_HIGH: Duration = Duration::from_millis(1);
const MARKER_CODE_HALF_PERIOD: Duration = Duration::from_micros(100);
const MARKER_START_FAILURE: u8 = 32;

esp_bootloader_esp_idf::esp_app_desc!();

#[derive(Clone, Copy, Debug, defmt::Format)]
enum CaptureExit {
    Complete,
    Timeout,
    Availability,
    AvailabilityModel,
    FrameModel,
    TargetPush,
    AcceptanceModel,
}

#[derive(Clone, Copy, Debug, defmt::Format)]
struct CaptureReport {
    exit: CaptureExit,
    accepted_refills: u32,
    sealed_horizon: u64,
}

impl CaptureReport {
    /// Stable numeric outcome used by the marker and machine attestation.
    const fn exit_code(self) -> u8 {
        match self.exit {
            CaptureExit::Complete => 1,
            CaptureExit::Timeout => 2,
            CaptureExit::Availability => 3,
            CaptureExit::AvailabilityModel => 4,
            CaptureExit::FrameModel => 5,
            CaptureExit::TargetPush => 6,
            CaptureExit::AcceptanceModel => 7,
        }
    }

    /// Pulse count between the two long marker sentinels.
    ///
    /// Bits 0–2 identify the exit; bit 3 means stop failed and bit 4 means the
    /// post-stop safe rewrite failed. Code 32 is reserved for start failure.
    const fn marker_code(self, stop_ok: bool, rewrite_ok: bool) -> u8 {
        self.exit_code() | if stop_ok { 0 } else { 1 << 3 } | if rewrite_ok { 0 } else { 1 << 4 }
    }
}

/// Safe-image-only waveform fixture. Its reachable path does not initialize
/// Wi-Fi, storage, motion, the second core, or any process output API.
#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    rtt_target::rtt_init_defmt!();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let split = mks_tinybee::split(peripherals);
    let mks_tinybee::SplitResources {
        runtime,
        service,
        realtime,
    } = split;
    // GPIO4/LCD_RS is a package-declared nonhazardous safe-low output. The HIL
    // procedure requires EXP1 and every display cable to remain disconnected.
    let mut capture_marker = service.into_hil_capture_marker();

    // Establish the complete disabled/off image before starting a timer,
    // logging a capture phase, or configuring I2S. Unused service tokens remain
    // retained and Wi-Fi is never initialized.
    let established = match realtime.establish_safe_outputs() {
        Ok(resources) => resources,
        Err(_) => {
            error!("HIL_ABORT static safe transaction failed");
            halt()
        }
    };
    let timer_group0 = TimerGroup::new(runtime.timer_group0);
    let software_interrupt = SoftwareInterruptControl::new(runtime.software_interrupt);
    esp_rtos::start(timer_group0.timer0, software_interrupt.software_interrupt0);
    let _retained_core_tokens = (runtime.cpu_control, software_interrupt.software_interrupt1);

    if let Err(reason) = mks_tinybee::PACKAGE.validate() {
        error!(
            "HIL_ABORT board package invalid: {:?}",
            defmt::Debug2Format(&reason)
        );
        park().await
    }

    info!(
        "HIL_STATIC_SAFE board={} image=0x{:06x}; DISCONNECT ALL MOTOR AND PROCESS LOADS",
        env!("ALUMINA_BOARD_ID"),
        board_mks_tinybee::DESCRIBED_SAFE_I2S_IMAGE
    );
    Timer::after(STATIC_CAPTURE_INTERVAL).await;

    let mut pcm = match established.into_unqualified_pcm_short() {
        Ok(resources) => resources,
        Err(_) => {
            error!("HIL_ABORT PCM-short configuration failed after static safe image");
            park().await
        }
    };
    let ring_prepared_at = DeviceCycle(Instant::now().as_ticks());

    capture_marker.set_high();
    // The exact hypothesized epoch is the lower bound read immediately before
    // the synchronous HAL start call. Only the analyzer can decide whether
    // physical WS realized this model. Static safe establishment preceded
    // timebase start and is therefore recorded no later than device cycle zero.
    let start_call_before = Instant::now().as_ticks();
    let hypothesized_epoch = DeviceCycle(start_call_before);
    let (mut transfer, start_call_after, mut owner) = match pcm.start_safe_capture() {
        Ok(transfer) => {
            let start_call_after = Instant::now().as_ticks();
            let mut owner = match mks_tinybee::pcm_short::UnqualifiedPcmShortResources::new_owner(
                DeviceCycle(0),
                ring_prepared_at,
                hypothesized_epoch,
            ) {
                Ok(owner) => owner,
                Err(_) => {
                    let _ = transfer.stop();
                    capture_marker.set_low();
                    emit_marker_report(&mut capture_marker, MARKER_START_FAILURE).await;
                    error!("HIL_ABORT portable static/stream owner construction failed");
                    park().await
                }
            };
            if owner
                .record_start_call(
                    PcmShortOperationWindow {
                        began_at: start_call_before,
                        returned_at: start_call_after,
                    },
                    true,
                )
                .is_err()
            {
                let _ = transfer.stop();
                capture_marker.set_low();
                emit_marker_report(&mut capture_marker, MARKER_START_FAILURE).await;
                error!("HIL_ABORT portable owner rejected the HAL start interval");
                park().await
            }
            (transfer, start_call_after, owner)
        }
        Err(_) => {
            let start_call_after = Instant::now().as_ticks();
            let mut owner = match mks_tinybee::pcm_short::UnqualifiedPcmShortResources::new_owner(
                DeviceCycle(0),
                ring_prepared_at,
                hypothesized_epoch,
            ) {
                Ok(owner) => owner,
                Err(_) => {
                    capture_marker.set_low();
                    emit_marker_report(&mut capture_marker, MARKER_START_FAILURE).await;
                    error!("HIL_ABORT portable static/stream owner construction failed");
                    park().await
                }
            };
            let _ = owner.record_start_call(
                PcmShortOperationWindow {
                    began_at: start_call_before,
                    returned_at: start_call_after,
                },
                false,
            );
            capture_marker.set_low();
            emit_marker_report(&mut capture_marker, MARKER_START_FAILURE).await;
            error!("HIL_ABORT circular safe transfer did not start");
            park().await
        }
    };

    // No RTT logging or await point is permitted while the short physical ring
    // is live. Report every start fact only after the transfer has stopped.
    let deadline = Instant::now() + CAPTURE_TIMEOUT;
    let mut accepted_refills = 0_u32;
    let exit = 'capture: loop {
        if accepted_refills >= TARGET_REFILLS {
            break 'capture CaptureExit::Complete;
        }
        if Instant::now() >= deadline {
            break 'capture CaptureExit::Timeout;
        }
        let available =
            match mks_tinybee::pcm_short::UnqualifiedPcmShortResources::available_frame_slots(
                &mut transfer,
            ) {
                Ok(available) => available,
                Err(_) => break 'capture CaptureExit::Availability,
            };
        let remaining = TARGET_REFILLS - accepted_refills;
        let maximum_frames = match usize::try_from(remaining) {
            Ok(maximum_frames) => maximum_frames,
            Err(_) => break 'capture CaptureExit::AcceptanceModel,
        };
        let batch = match owner.refill_batch_with(available, maximum_frames, |frame| {
            mks_tinybee::pcm_short::UnqualifiedPcmShortResources::push_planned_frame(
                &mut transfer,
                frame,
            )
            .is_ok()
        }) {
            Ok(batch) => batch,
            Err(PcmShortDmaRefillBatchError::TargetPush { .. }) => {
                break 'capture CaptureExit::TargetPush;
            }
            Err(PcmShortDmaRefillBatchError::Model { phase, .. }) => {
                break 'capture match phase {
                    PcmShortDmaRefillPhase::Availability => CaptureExit::AvailabilityModel,
                    PcmShortDmaRefillPhase::Preview => CaptureExit::FrameModel,
                    PcmShortDmaRefillPhase::Acceptance | PcmShortDmaRefillPhase::Finalize => {
                        CaptureExit::AcceptanceModel
                    }
                };
            }
        };
        let accepted = match u32::try_from(batch.accepted_frames) {
            Ok(accepted) => accepted,
            Err(_) => break 'capture CaptureExit::AcceptanceModel,
        };
        accepted_refills = match accepted_refills.checked_add(accepted) {
            Some(refills) => refills,
            None => break 'capture CaptureExit::AcceptanceModel,
        };
    };

    let sealed_horizon = owner.sealed_horizon().unwrap_or(0);
    let stop_call_before = Instant::now().as_ticks();
    let hal_stop_ok = transfer.stop().is_ok();
    capture_marker.set_low();
    let stop_call_after = Instant::now().as_ticks();
    let owner_stop_ok = owner
        .record_stop_call(
            PcmShortOperationWindow {
                began_at: stop_call_before,
                returned_at: stop_call_after,
            },
            hal_stop_ok,
        )
        .is_ok();
    let stop_ok = hal_stop_ok && owner_stop_ok;
    let rewrite_call_before = Instant::now().as_ticks();
    let hal_rewrite_ok = pcm.rewrite_safe_pipeline().is_ok();
    let rewrite_call_after = Instant::now().as_ticks();
    let safe_image = owner.safe_image();
    let owner_rewrite_ok = owner
        .record_safe_rewrite(
            PcmShortOperationWindow {
                began_at: rewrite_call_before,
                returned_at: rewrite_call_after,
            },
            safe_image,
            2,
            hal_rewrite_ok,
        )
        .is_ok();
    let rewrite_ok = hal_rewrite_ok
        && owner_rewrite_ok
        && owner.state() == PcmShortDmaStreamState::SafeRewriteIssued
        && owner.fault().is_none();
    let report = CaptureReport {
        exit,
        accepted_refills,
        sealed_horizon,
    };
    let marker_code = report.marker_code(stop_ok, rewrite_ok);
    emit_marker_report(&mut capture_marker, marker_code).await;
    // This ordered numeric suffix is independently parsed from the retained
    // RTT log. Human/debug rendering is deliberately not evidence syntax.
    info!(
        "HIL_PCM_ATTEST_V2 model_epoch={} start_before={} start_after={} frames={} rate_hz={} exit={} accepted_refills={} sealed_horizon={} stop_before={} stop_after={} stop_ok={} rewrite_before={} rewrite_after={} rewrite_ok={} owner_state={} owner_fault={} safe_reclaimed={} marker_code={}",
        hypothesized_epoch.0,
        start_call_before,
        start_call_after,
        mks_tinybee::pcm_short::UNQUALIFIED_PCM_SHORT_DMA_FRAMES,
        mks_tinybee::pcm_short::UNQUALIFIED_PCM_SHORT_FRAME_RATE_HZ,
        report.exit_code(),
        report.accepted_refills,
        report.sealed_horizon,
        stop_call_before,
        stop_call_after,
        u8::from(stop_ok),
        rewrite_call_before,
        rewrite_call_after,
        u8::from(rewrite_ok),
        pcm_owner_state_code(owner.state()),
        u8::from(owner.fault().is_some()),
        u8::from(owner.safe_reclaimed()),
        marker_code
    );
    info!(
        "HIL_PCM_STOPPED model_epoch={} start_before={} start_after={} frames={} rate_hz={} report={} stop_before={} stop_after={} stop_ok={} rewrite_before={} rewrite_after={} safe_rewrite_ok={} owner_state={:?} owner_fault={:?} safe_reclaimed={} marker_code={}",
        hypothesized_epoch.0,
        start_call_before,
        start_call_after,
        mks_tinybee::pcm_short::UNQUALIFIED_PCM_SHORT_DMA_FRAMES,
        mks_tinybee::pcm_short::UNQUALIFIED_PCM_SHORT_FRAME_RATE_HZ,
        report,
        stop_call_before,
        stop_call_after,
        stop_ok,
        rewrite_call_before,
        rewrite_call_after,
        rewrite_ok,
        defmt::Debug2Format(&owner.state()),
        defmt::Debug2Format(&owner.fault()),
        owner.safe_reclaimed(),
        marker_code
    );
    if !matches!(exit, CaptureExit::Complete) || !stop_ok || !rewrite_ok {
        error!("HIL_RESULT failed closed; no hardware qualification granted");
    } else {
        info!("HIL_RESULT capture complete; waveform review is still required");
    }
    Timer::after(STATIC_CAPTURE_INTERVAL).await;
    park().await
}

/// Emits one self-delimiting analyzer-only outcome after all I2S activity.
///
/// A 1 ms high sentinel, 1 ms low separator, `code` 100-us-high/100-us-low
/// pulses, and a final 1 ms high sentinel are distinguishable from the long
/// live-transfer high level. The marker always returns low before parking.
async fn emit_marker_report(marker: &mut Output<'static>, code: u8) {
    marker.set_low();
    Timer::after(MARKER_REPORT_GAP).await;
    marker.set_high();
    Timer::after(MARKER_SENTINEL_HIGH).await;
    marker.set_low();
    Timer::after(MARKER_REPORT_GAP).await;
    let mut emitted = 0;
    while emitted < code {
        marker.set_high();
        Timer::after(MARKER_CODE_HALF_PERIOD).await;
        marker.set_low();
        Timer::after(MARKER_CODE_HALF_PERIOD).await;
        emitted += 1;
    }
    marker.set_high();
    Timer::after(MARKER_SENTINEL_HIGH).await;
    marker.set_low();
}

async fn park() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// Stable attestation codes; these are independent of Rust enum discriminants.
const fn pcm_owner_state_code(state: PcmShortDmaStreamState) -> u8 {
    match state {
        PcmShortDmaStreamState::StaticSafe => 1,
        PcmShortDmaStreamState::PreparedSafe => 2,
        PcmShortDmaStreamState::StartIssued => 3,
        PcmShortDmaStreamState::StreamObserved => 4,
        PcmShortDmaStreamState::StartUncertain => 5,
        PcmShortDmaStreamState::Stopped => 6,
        PcmShortDmaStreamState::StopUncertain => 7,
        PcmShortDmaStreamState::SafeRewriteIssued => 8,
        PcmShortDmaStreamState::SafeRewriteUncertain => 9,
        PcmShortDmaStreamState::PeripheralSafe => 10,
    }
}
