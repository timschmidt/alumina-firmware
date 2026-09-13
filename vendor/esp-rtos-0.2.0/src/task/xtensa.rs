#[cfg(feature = "esp-radio")]
use core::ffi::c_void;
use core::sync::atomic::Ordering;

pub(crate) use esp_hal::trapframe::TrapFrame as CpuContext;
#[cfg(not(esp32))]
use esp_hal::xtensa_lx::interrupt;
use esp_hal::{interrupt::software::SoftwareInterrupt, ram};
#[cfg(multi_core)]
use esp_hal::{
    interrupt::{InterruptHandler, Priority},
    system::Cpu,
};
use portable_atomic::AtomicPtr;

#[cfg(feature = "rtos-trace")]
use crate::TraceEvents;
use crate::{SCHEDULER, task::IdleFn};

static IDLE_HOOK: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

pub(crate) extern "C" fn idle_hook() -> ! {
    loop {
        unsafe { core::arch::asm!("waiti 0") };
    }
}

#[unsafe(naked)]
extern "C" fn idle_entry() -> ! {
    core::arch::naked_asm!("
        .literal idle_hook_fn, {idle_hook_fn}

        l32r   a2, idle_hook_fn
        l32i.n a2, a2, 0
        callx4 a2
    ", idle_hook_fn = sym IDLE_HOOK);
}

pub(crate) fn set_idle_hook_entry(idle_context: &mut CpuContext, hook_fn: IdleFn) {
    IDLE_HOOK.store(hook_fn as *mut (), Ordering::Relaxed);

    // Point idle context PC at the assembly that calls the idle hook. We need a new stack
    // frame for the idle task on the main stack.
    idle_context.PC = idle_entry as usize as u32;
    // Set a valid processor status value that cannot spill registers into the
    // permanent main task's stack when the idle context is restored.
    idle_context.PS = 0x40020;
}

#[cfg(feature = "esp-radio")]
pub(crate) fn new_task_context(
    task_fn: extern "C" fn(*mut c_void),
    param: *mut c_void,
    stack_top: *mut (),
) -> CpuContext {
    // stack must be aligned by 16
    let stack_top = stack_top as u32;
    let stack_top = stack_top - (stack_top % 16);

    unsafe {
        *((stack_top - 4) as *mut u32) = 0;
        *((stack_top - 8) as *mut u32) = 0;
        *((stack_top - 12) as *mut u32) = stack_top;
        *((stack_top - 16) as *mut u32) = 0;
    }

    CpuContext {
        PC: super::task_wrapper as usize as u32,
        A0: 0,
        A1: stack_top,
        A6: task_fn as usize as u32,
        A7: param as usize as u32,

        // For windowed ABI set WOE and CALLINC (pretend task was 'call4'd)
        PS: 0x00040000 | ((1 & 3) << 16),

        ..Default::default()
    }
}

pub(crate) fn task_switch(
    current_context: *mut CpuContext,
    next_context: *mut CpuContext,
    trap_frame: &mut CpuContext,
) {
    if !current_context.is_null() {
        unsafe { core::ptr::copy_nonoverlapping(trap_frame, current_context, 1) };
    }
    unsafe { core::ptr::copy_nonoverlapping(next_context, trap_frame, 1) };
}

// ESP32-S2 and ESP32-S3 use Software0 (priority 1) for same-core task
// switching. Classic ESP32 cannot use it because the Bluetooth driver reserves
// it, so it uses the per-core FROM_CPU interrupts for both local and cross-core
// switching.
#[cfg(not(esp32))]
const SW_INTERRUPT: u32 = 1 << 7;

pub(crate) fn setup_multitasking<const IRQ: u8>(mut irq: SoftwareInterrupt<'static, IRQ>) {
    #[cfg(not(esp32))]
    unsafe {
        interrupt::enable_mask(SW_INTERRUPT);
    }

    #[cfg(multi_core)]
    {
        irq.set_interrupt_handler(InterruptHandler::new(
            unsafe {
                core::mem::transmute::<*const (), extern "C" fn()>(
                    cross_core_yield_handler as *const (),
                )
            },
            Priority::min(),
        ));
    }
}

#[cfg(multi_core)]
pub(crate) fn setup_smp<const IRQ: u8>(irq: SoftwareInterrupt<'static, IRQ>) {
    setup_multitasking(irq);
}

#[allow(non_snake_case)]
#[ram]
#[cfg(not(esp32))]
#[unsafe(export_name = "Software0")]
fn task_switch_interrupt(context: &mut CpuContext) {
    #[cfg(multi_core)]
    if esp_hal::system::Cpu::current() == esp_hal::system::Cpu::AppCpu {
        crate::mark_second_core_start_stage(90);
    }
    unsafe { interrupt::clear(SW_INTERRUPT) };

    trigger_task_switch(context);
    #[cfg(multi_core)]
    if esp_hal::system::Cpu::current() == esp_hal::system::Cpu::AppCpu {
        crate::mark_second_core_start_stage(91);
    }
}

#[inline]
pub(crate) fn yield_task() {
    #[cfg(feature = "rtos-trace")]
    {
        rtos_trace::trace::marker_begin(TraceEvents::YieldTask as u32);
        rtos_trace::trace::marker_end(TraceEvents::YieldTask as u32);
    }

    #[cfg(not(esp32))]
    unsafe {
        interrupt::set(SW_INTERRUPT);
    }

    #[cfg(esp32)]
    {
        match Cpu::current() {
            Cpu::ProCpu => unsafe { SoftwareInterrupt::<'static, 0>::steal() }.raise(),
            Cpu::AppCpu => unsafe { SoftwareInterrupt::<'static, 1>::steal() }.raise(),
        }

        // Allow the peripheral interrupt request to become visible before the
        // caller returns to code that may immediately inspect scheduler state.
        unsafe {
            core::arch::asm!("nop");
            core::arch::asm!("nop");
            core::arch::asm!("nop");
            core::arch::asm!("nop");
        }
    }
}

#[cfg(multi_core)]
#[ram]
extern "C" fn cross_core_yield_handler(context: &mut CpuContext) {
    if Cpu::current() == Cpu::AppCpu {
        crate::mark_second_core_start_stage(80);
    }
    match Cpu::current() {
        Cpu::ProCpu => unsafe { SoftwareInterrupt::<'static, 0>::steal() }.reset(),
        Cpu::AppCpu => unsafe { SoftwareInterrupt::<'static, 1>::steal() }.reset(),
    }

    if Cpu::current() == Cpu::AppCpu {
        crate::mark_second_core_start_stage(81);
    }
    trigger_task_switch(context);
    if Cpu::current() == Cpu::AppCpu {
        crate::mark_second_core_start_stage(82);
    }
}

// Keep one non-inlined switch body in RAM on ESP32-S3. On classic ESP32 and
// ESP32-S2 this stays inline, matching the upstream correction.
#[cfg_attr(esp32s3, ram)]
fn trigger_task_switch(context: &mut CpuContext) {
    SCHEDULER.with(|scheduler| scheduler.switch_task(context));
}
