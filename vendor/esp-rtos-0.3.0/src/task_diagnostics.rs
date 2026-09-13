//! Bounded diagnostics for initial Xtensa compatibility-task contexts.

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

const RECORD_CAPACITY: usize = 8;
const NAME_BYTES: usize = 12;

struct CreationSlot {
    published: AtomicU32,
    name_0: AtomicU32,
    name_1: AtomicU32,
    name_2: AtomicU32,
    name_len: AtomicU32,
    entry: AtomicU32,
    parameter: AtomicU32,
    stack_bottom: AtomicU32,
    stack_top: AtomicU32,
    context_pc: AtomicU32,
    context_a6: AtomicU32,
    context_a7: AtomicU32,
    priority: AtomicU32,
    pinned_core: AtomicU32,
}

impl CreationSlot {
    const fn new() -> Self {
        Self {
            published: AtomicU32::new(0),
            name_0: AtomicU32::new(0),
            name_1: AtomicU32::new(0),
            name_2: AtomicU32::new(0),
            name_len: AtomicU32::new(0),
            entry: AtomicU32::new(0),
            parameter: AtomicU32::new(0),
            stack_bottom: AtomicU32::new(0),
            stack_top: AtomicU32::new(0),
            context_pc: AtomicU32::new(0),
            context_a6: AtomicU32::new(0),
            context_a7: AtomicU32::new(0),
            priority: AtomicU32::new(0),
            pinned_core: AtomicU32::new(0),
        }
    }
}

struct WrapperSlot {
    published: AtomicU32,
    entry: AtomicU32,
    parameter: AtomicU32,
}

impl WrapperSlot {
    const fn new() -> Self {
        Self {
            published: AtomicU32::new(0),
            entry: AtomicU32::new(0),
            parameter: AtomicU32::new(0),
        }
    }
}

static CREATION_NEXT: AtomicUsize = AtomicUsize::new(0);
static CREATIONS: [CreationSlot; RECORD_CAPACITY] =
    [const { CreationSlot::new() }; RECORD_CAPACITY];
static WRAPPER_NEXT: AtomicUsize = AtomicUsize::new(0);
static WRAPPERS: [WrapperSlot; RECORD_CAPACITY] =
    [const { WrapperSlot::new() }; RECORD_CAPACITY];

/// One task's immutable initial context facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskCreationDiagnostic {
    /// Truncated UTF-8 task name.
    pub name: [u8; NAME_BYTES],
    /// Valid leading bytes in `name`.
    pub name_len: u8,
    /// Compatibility-layer task entry address.
    pub entry: u32,
    /// Compatibility-layer task parameter address.
    pub parameter: u32,
    /// Inclusive stack allocation base.
    pub stack_bottom: u32,
    /// Exclusive aligned initial stack pointer.
    pub stack_top: u32,
    /// Initial program counter stored in the saved context.
    pub context_pc: u32,
    /// Initial physical A6 value, mapped to wrapper argument A2.
    pub context_a6: u32,
    /// Initial physical A7 value, mapped to wrapper argument A3.
    pub context_a7: u32,
    /// Scheduler priority after compatibility-layer clamping.
    pub priority: u32,
    /// Pinned core, or `u32::MAX` when unpinned.
    pub pinned_core: u32,
}

impl TaskCreationDiagnostic {
    /// Empty value suitable for fixed snapshot storage.
    pub const EMPTY: Self = Self {
        name: [0; NAME_BYTES],
        name_len: 0,
        entry: 0,
        parameter: 0,
        stack_bottom: 0,
        stack_top: 0,
        context_pc: 0,
        context_a6: 0,
        context_a7: 0,
        priority: 0,
        pinned_core: u32::MAX,
    };
}

/// Arguments observed on entry to one task wrapper.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskWrapperDiagnostic {
    /// Task entry address received by the wrapper ABI.
    pub entry: u32,
    /// Task parameter address received by the wrapper ABI.
    pub parameter: u32,
}

impl TaskWrapperDiagnostic {
    /// Empty value suitable for fixed snapshot storage.
    pub const EMPTY: Self = Self {
        entry: 0,
        parameter: 0,
    };
}

pub(crate) fn record_task_creation(
    name: &str,
    entry: u32,
    parameter: u32,
    stack_bottom: u32,
    stack_top: u32,
    context_pc: u32,
    context_a6: u32,
    context_a7: u32,
    priority: u32,
    pinned_core: u32,
) {
    let index = CREATION_NEXT.fetch_add(1, Ordering::Relaxed);
    let Some(slot) = CREATIONS.get(index) else {
        return;
    };
    let mut name_bytes = [0_u8; NAME_BYTES];
    let name_len = name.len().min(NAME_BYTES);
    name_bytes[..name_len].copy_from_slice(&name.as_bytes()[..name_len]);
    slot.name_0.store(
        u32::from_le_bytes(name_bytes[0..4].try_into().unwrap()),
        Ordering::Relaxed,
    );
    slot.name_1.store(
        u32::from_le_bytes(name_bytes[4..8].try_into().unwrap()),
        Ordering::Relaxed,
    );
    slot.name_2.store(
        u32::from_le_bytes(name_bytes[8..12].try_into().unwrap()),
        Ordering::Relaxed,
    );
    slot.name_len.store(name_len as u32, Ordering::Relaxed);
    slot.entry.store(entry, Ordering::Relaxed);
    slot.parameter.store(parameter, Ordering::Relaxed);
    slot.stack_bottom.store(stack_bottom, Ordering::Relaxed);
    slot.stack_top.store(stack_top, Ordering::Relaxed);
    slot.context_pc.store(context_pc, Ordering::Relaxed);
    slot.context_a6.store(context_a6, Ordering::Relaxed);
    slot.context_a7.store(context_a7, Ordering::Relaxed);
    slot.priority.store(priority, Ordering::Relaxed);
    slot.pinned_core.store(pinned_core, Ordering::Relaxed);
    slot.published.store(index as u32 + 1, Ordering::Release);
}

pub(crate) fn record_task_wrapper(entry: u32, parameter: u32) {
    let index = WRAPPER_NEXT.fetch_add(1, Ordering::Relaxed);
    let Some(slot) = WRAPPERS.get(index) else {
        return;
    };
    slot.entry.store(entry, Ordering::Relaxed);
    slot.parameter.store(parameter, Ordering::Relaxed);
    slot.published.store(index as u32 + 1, Ordering::Release);
}

/// Copies all completely published task-creation records into `destination`.
pub fn task_creation_diagnostics(destination: &mut [TaskCreationDiagnostic]) -> usize {
    let mut written = 0;
    for (index, slot) in CREATIONS.iter().enumerate() {
        if written == destination.len()
            || slot.published.load(Ordering::Acquire) != index as u32 + 1
        {
            break;
        }
        let mut name = [0_u8; NAME_BYTES];
        name[0..4].copy_from_slice(&slot.name_0.load(Ordering::Relaxed).to_le_bytes());
        name[4..8].copy_from_slice(&slot.name_1.load(Ordering::Relaxed).to_le_bytes());
        name[8..12].copy_from_slice(&slot.name_2.load(Ordering::Relaxed).to_le_bytes());
        destination[written] = TaskCreationDiagnostic {
            name,
            name_len: slot.name_len.load(Ordering::Relaxed) as u8,
            entry: slot.entry.load(Ordering::Relaxed),
            parameter: slot.parameter.load(Ordering::Relaxed),
            stack_bottom: slot.stack_bottom.load(Ordering::Relaxed),
            stack_top: slot.stack_top.load(Ordering::Relaxed),
            context_pc: slot.context_pc.load(Ordering::Relaxed),
            context_a6: slot.context_a6.load(Ordering::Relaxed),
            context_a7: slot.context_a7.load(Ordering::Relaxed),
            priority: slot.priority.load(Ordering::Relaxed),
            pinned_core: slot.pinned_core.load(Ordering::Relaxed),
        };
        written += 1;
    }
    written
}

/// Copies all completely published wrapper-entry records into `destination`.
pub fn task_wrapper_diagnostics(destination: &mut [TaskWrapperDiagnostic]) -> usize {
    let mut written = 0;
    for (index, slot) in WRAPPERS.iter().enumerate() {
        if written == destination.len()
            || slot.published.load(Ordering::Acquire) != index as u32 + 1
        {
            break;
        }
        destination[written] = TaskWrapperDiagnostic {
            entry: slot.entry.load(Ordering::Relaxed),
            parameter: slot.parameter.load(Ordering::Relaxed),
        };
        written += 1;
    }
    written
}
