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
use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use panic_rtt_target as _;

use hardware::mks_tinybee;

/// About 200 ms of steady-state traffic at the model-only 250 kHz frame rate.
const TARGET_REFILLS: u32 = 50_000;
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(2);
const STATIC_CAPTURE_INTERVAL: Duration = Duration::from_millis(50);

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
    let _retained_service = service;

    let timer_group0 = TimerGroup::new(runtime.timer_group0);
    esp_rtos::start(timer_group0.timer0);
    let _retained_core_tokens = (runtime.cpu_control, runtime.software_interrupt);

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
    let hypothesized_epoch = DeviceCycle(Instant::now().as_ticks());
    let mut horizon = match pcm.new_horizon(hypothesized_epoch) {
        Ok(horizon) => horizon,
        Err(_) => {
            error!("HIL_ABORT portable safe horizon construction failed");
            park().await
        }
    };

    let start_call_before = Instant::now().as_ticks();
    let mut transfer = match pcm.start_safe_capture() {
        Ok(transfer) => transfer,
        Err(_) => {
            error!("HIL_ABORT circular safe transfer did not start");
            park().await
        }
    };
    let start_call_after = Instant::now().as_ticks();

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
        if horizon.synchronize_refill_availability(available).is_err() {
            break 'capture CaptureExit::AvailabilityModel;
        }
        while horizon.refill_credit_frames() != 0 {
            let frame = match horizon.next_refill_frame() {
                Ok(Some(frame)) => frame,
                Ok(None) | Err(_) => break 'capture CaptureExit::FrameModel,
            };
            if mks_tinybee::pcm_short::UnqualifiedPcmShortResources::push_planned_frame(
                &mut transfer,
                frame,
            )
            .is_err()
            {
                break 'capture CaptureExit::TargetPush;
            }
            if horizon.accept_refill(frame).is_err() {
                break 'capture CaptureExit::AcceptanceModel;
            }
            accepted_refills = match accepted_refills.checked_add(1) {
                Some(refills) => refills,
                None => break 'capture CaptureExit::AcceptanceModel,
            };
        }
    };

    let stop_call_before = Instant::now().as_ticks();
    let stop_ok = transfer.stop().is_ok();
    let stop_call_after = Instant::now().as_ticks();
    let rewrite_ok = pcm.rewrite_safe_pipeline().is_ok();
    let sealed_horizon = horizon.sealed_horizon().unwrap_or(0);
    let report = CaptureReport {
        exit,
        accepted_refills,
        sealed_horizon,
    };
    info!(
        "HIL_PCM_STOPPED model_epoch={} start_before={} start_after={} frames={} rate_hz={} report={} stop_before={} stop_after={} stop_ok={} safe_rewrite_ok={}",
        hypothesized_epoch.0,
        start_call_before,
        start_call_after,
        mks_tinybee::pcm_short::UNQUALIFIED_PCM_SHORT_DMA_FRAMES,
        mks_tinybee::pcm_short::UNQUALIFIED_PCM_SHORT_FRAME_RATE_HZ,
        report,
        stop_call_before,
        stop_call_after,
        stop_ok,
        rewrite_ok
    );
    if !matches!(exit, CaptureExit::Complete) || !stop_ok || !rewrite_ok {
        error!("HIL_RESULT failed closed; no hardware qualification granted");
    } else {
        info!("HIL_RESULT capture complete; waveform review is still required");
    }
    Timer::after(STATIC_CAPTURE_INTERVAL).await;
    park().await
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
