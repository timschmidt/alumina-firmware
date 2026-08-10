use esp_hal::peripherals::{CPU_CTRL, SW_INTERRUPT, TIMG0};

/// Runtime-only tokens consumed before either domain executor starts.
pub struct RuntimeResources {
    pub timer_group0: TIMG0<'static>,
    pub cpu_control: CPU_CTRL<'static>,
    pub software_interrupt: SW_INTERRUPT<'static>,
}

#[cfg(feature = "board-mks-tinybee")]
pub mod mks_tinybee;
#[cfg(feature = "board-mks-tinybee")]
pub use mks_tinybee as selected;

#[cfg(feature = "board-t-deck-pro")]
pub mod t_deck_pro;
#[cfg(feature = "board-t-deck-pro")]
pub use t_deck_pro as selected;
