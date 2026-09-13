#![no_std]
#![doc = "Audited same-core target access for conservative Xtensa stack watermarks."]
#![deny(missing_docs)]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]

use core::ptr::{read_volatile, with_exposed_provenance_mut, write_volatile};

use alumina_protocol::DeviceCycle;
use alumina_runtime::stack::{
    STACK_WORD_BYTES, StackDomain, StackWatermarkError, StackWatermarkSnapshot,
    StackWatermarkTracker, stack_canary_word,
};
use esp_hal::interrupt::software::SoftwareInterrupt;
use esp_hal::peripherals::CPU_CTRL;
use esp_hal::system::Stack;
use esp_hal::xtensa_lx;

/// Low stack bytes excluded from canary access.
///
/// ESP-HAL 1.0 currently places its guard at byte 60. Keeping a 256-byte
/// exclusion both preserves that word and makes this instrument independent of
/// small guard-offset changes.
pub const STACK_LOW_EXCLUSION_BYTES: usize = 256;
/// Space below the sampled stack pointer never painted or scanned.
///
/// This covers the probe's own call chain and asynchronous exception entry.
/// It is deliberately charged as used in every report.
const CURRENT_STACK_RESERVE_BYTES: usize = 2 * 1_024;

/// Maximum words read per 1 ms real-time management pass.
pub const REALTIME_SCAN_WORDS: usize = 16;
/// Maximum words read per 10 ms service pass.
pub const SERVICE_SCAN_WORDS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StackBounds {
    bottom: usize,
    top: usize,
}

impl StackBounds {
    fn new(bottom: usize, top: usize) -> Result<Self, TargetStackWatermarkError> {
        if !bottom.is_multiple_of(STACK_WORD_BYTES)
            || !top.is_multiple_of(STACK_WORD_BYTES)
            || top <= bottom
            || top - bottom <= STACK_LOW_EXCLUSION_BYTES + CURRENT_STACK_RESERVE_BYTES
        {
            return Err(TargetStackWatermarkError::Bounds);
        }
        Ok(Self { bottom, top })
    }
}

/// One-shot initializer delivered only inside the application-core entry function.
///
/// It is intentionally neither `Copy` nor `Clone`. Safe callers cannot detach
/// raw bounds from the permanent stack passed to ESP-RTOS, nor initialize the
/// probe before the second core is executing on that stack.
pub struct AppStackInitializer(Result<StackBounds, TargetStackWatermarkError>);

impl AppStackInitializer {
    /// Initializes the application-core epoch on the current core and stack.
    pub fn initialize_current(
        self,
        epoch_cycle: DeviceCycle,
    ) -> Result<TargetStackWatermark, TargetStackWatermarkError> {
        let bounds = self.0?;
        // SAFETY: this initializer is constructed only by
        // `start_second_core_with_watermark` from the same permanent HAL stack
        // used to enter this closure. The implementation also rejects a live
        // stack pointer outside the captured range.
        unsafe { TargetStackWatermark::initialize(StackDomain::RealtimeCore, bounds, epoch_cycle) }
    }
}

/// Starts core 1 and delivers its stack initializer only inside its entry function.
///
/// Bounds setup failure does not prevent the core from starting: the delivered
/// initializer reports the failure and firmware can disable passive measurement.
pub fn start_second_core_with_watermark<const BYTES: usize>(
    cpu_control: CPU_CTRL,
    interrupt1: SoftwareInterrupt<'static, 1>,
    stack: &'static mut Stack<BYTES>,
    entry: impl FnOnce(AppStackInitializer) + Send + 'static,
) {
    let bottom = stack.bottom().expose_provenance();
    let bounds = bottom
        .checked_add(BYTES)
        .ok_or(TargetStackWatermarkError::Bounds)
        .and_then(|top| StackBounds::new(bottom, top));
    esp_rtos::start_second_core(cpu_control, interrupt1, stack, move || {
        entry(AppStackInitializer(bounds))
    });
}

/// Initializes the linker-owned service-core stack measurement epoch.
///
/// Live stack-pointer validation and a fixed reserve make this safe to call
/// after the RTOS timer is started. Startup before this call remains explicitly
/// outside the epoch and is charged as unknown/used.
pub fn initialize_service_current(
    epoch_cycle: DeviceCycle,
) -> Result<TargetStackWatermark, TargetStackWatermarkError> {
    unsafe extern "C" {
        static _stack_end_cpu0: u8;
        static _stack_start_cpu0: u8;
    }
    let bottom = (&raw const _stack_end_cpu0).expose_provenance();
    let top = (&raw const _stack_start_cpu0).expose_provenance();
    let bounds = StackBounds::new(bottom, top)?;
    // SAFETY: these are the selected image's linker-owned CPU0 stack symbols.
    // The implementation additionally verifies the live stack pointer before
    // touching only the unused prefix below a fixed reserve.
    unsafe { TargetStackWatermark::initialize(StackDomain::ServiceCore, bounds, epoch_cycle) }
}

/// Same-core target probe. It never lends or retains a Rust reference to stack memory.
pub struct TargetStackWatermark {
    bounds: StackBounds,
    monitored_start: usize,
    owner_processor_id: u32,
    tracker: StackWatermarkTracker,
}

impl TargetStackWatermark {
    /// Paints only the unused prefix below the current core's stack pointer.
    ///
    /// # Safety
    ///
    /// `bounds` must describe the complete downward-growing stack currently in
    /// use by the calling processor. No other processor or DMA engine may read
    /// or write the range. This must run once for the stack's measurement epoch.
    unsafe fn initialize(
        domain: StackDomain,
        bounds: StackBounds,
        epoch_cycle: DeviceCycle,
    ) -> Result<Self, TargetStackWatermarkError> {
        xtensa_lx::interrupt::free(|| {
            let owner_processor_id = xtensa_lx::get_processor_id();
            let stack_pointer = xtensa_lx::get_stack_pointer().expose_provenance();
            if stack_pointer <= bounds.bottom || stack_pointer > bounds.top {
                return Err(TargetStackWatermarkError::StackPointer);
            }
            let monitored_start = align_up(
                bounds
                    .bottom
                    .checked_add(STACK_LOW_EXCLUSION_BYTES)
                    .ok_or(TargetStackWatermarkError::Bounds)?,
                STACK_WORD_BYTES,
            );
            let paint_end = align_down(
                stack_pointer
                    .saturating_sub(CURRENT_STACK_RESERVE_BYTES)
                    .min(bounds.top),
                STACK_WORD_BYTES,
            );
            if paint_end <= monitored_start {
                return Err(TargetStackWatermarkError::StackPointer);
            }

            let mut address = monitored_start;
            while address < paint_end {
                // SAFETY: the caller establishes exclusive ownership of the
                // current core's unused stack prefix. Interrupts are masked and
                // the fixed current-stack reserve keeps this call chain out of
                // the painted range.
                unsafe {
                    write_volatile(
                        with_exposed_provenance_mut::<u32>(address),
                        stack_canary_word(address),
                    );
                }
                address += STACK_WORD_BYTES;
            }

            let tracker = StackWatermarkTracker::new(
                domain,
                bounds.top - bounds.bottom,
                monitored_start - bounds.bottom,
                paint_end - monitored_start,
                epoch_cycle,
            )
            .map_err(TargetStackWatermarkError::Tracker)?;
            Ok(Self {
                bounds,
                monitored_start,
                owner_processor_id,
                tracker,
            })
        })
    }

    /// Reads one bounded same-core canary window and updates the low-water mark.
    pub fn sample_current(
        &mut self,
        maximum_scan_words: usize,
        sampled_at: DeviceCycle,
    ) -> Result<StackWatermarkSnapshot, TargetStackWatermarkError> {
        xtensa_lx::interrupt::free(|| {
            if xtensa_lx::get_processor_id() != self.owner_processor_id {
                return Err(TargetStackWatermarkError::WrongCore);
            }
            let stack_pointer = xtensa_lx::get_stack_pointer().expose_provenance();
            if stack_pointer <= self.bounds.bottom || stack_pointer > self.bounds.top {
                return Err(TargetStackWatermarkError::StackPointer);
            }
            let safe_ceiling = align_down(
                stack_pointer
                    .saturating_sub(CURRENT_STACK_RESERVE_BYTES)
                    .min(self.bounds.top),
                STACK_WORD_BYTES,
            );
            let available = safe_ceiling.saturating_sub(self.monitored_start);
            self.tracker
                .sample(available, maximum_scan_words, sampled_at, |word| {
                    let address = self.monitored_start + word * STACK_WORD_BYTES;
                    // SAFETY: construction established this as the current
                    // processor's canary prefix. Interrupts are masked, and the
                    // tracker never requests a word above `safe_ceiling`.
                    unsafe {
                        read_volatile(with_exposed_provenance_mut::<u32>(address))
                            == stack_canary_word(address)
                    }
                })
                .map_err(TargetStackWatermarkError::Tracker)
        })
    }

    /// Latest conservative report without touching stack memory.
    pub const fn snapshot(&self) -> StackWatermarkSnapshot {
        self.tracker.snapshot()
    }
}

/// Target stack setup or sampling failure. Measurement failure is passive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetStackWatermarkError {
    /// Stack bounds are unaligned, empty, or too small for the policy margins.
    Bounds,
    /// The live stack pointer is outside the expected range or has no paintable prefix.
    StackPointer,
    /// A probe was moved away from its initializing processor.
    WrongCore,
    /// Portable tracker construction or sampling failed.
    Tracker(StackWatermarkError),
}

const fn align_up(value: usize, alignment: usize) -> usize {
    value.saturating_add(alignment - 1) / alignment * alignment
}

const fn align_down(value: usize, alignment: usize) -> usize {
    value / alignment * alignment
}
