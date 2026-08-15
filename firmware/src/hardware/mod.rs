use esp_hal::peripherals::{CPU_CTRL, SW_INTERRUPT, TIMG0};

mod safety_inputs;

/// Runtime-only tokens consumed before either domain executor starts.
pub struct RuntimeResources {
    pub timer_group0: TIMG0<'static>,
    pub cpu_control: CPU_CTRL<'static>,
    pub software_interrupt: SW_INTERRUPT<'static>,
}

#[cfg(feature = "board-mks-esp32-foc-v1")]
pub mod mks_esp32_foc_v1;
#[cfg(feature = "board-mks-esp32-foc-v1")]
pub use mks_esp32_foc_v1 as selected;

#[cfg(any(feature = "board-mks-tinybee", feature = "board-mks-tinybee-4mb"))]
pub mod mks_tinybee;
#[cfg(all(
    any(feature = "board-mks-tinybee", feature = "board-mks-tinybee-4mb"),
    not(feature = "hil-mks-tinybee-pcm-short-safe")
))]
pub use mks_tinybee as selected;

#[cfg(feature = "board-t-deck-pro")]
pub mod t_deck_pro;
#[cfg(feature = "board-t-deck-pro")]
pub use t_deck_pro as selected;

#[cfg(feature = "board-t-lora-pager")]
pub mod t_lora_pager;
#[cfg(feature = "board-t-lora-pager")]
pub use t_lora_pager as selected;
