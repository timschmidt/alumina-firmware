//! Prototype step executor intended to be driven by a hardware timer interrupt.

use crate::commandbuffer::Block;

/// Tracks the block currently being emitted by the step generator.
#[derive(Default)]
pub struct Stepper {
    current_block: Option<Block>,
    step_count: i32,
}

impl Stepper {
    /// Creates an idle step executor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the active block and resets its emitted-step count.
    pub fn execute_block(&mut self, block: Block) {
        self.current_block = Some(block);
        self.step_count = 0;
    }

    /// Advances the software execution state by one step event.
    ///
    /// Hardware pulse generation and timer scheduling are not implemented yet.
    pub fn step_interrupt_handler(&mut self) {
        if let Some(block) = &self.current_block {
            self.step_count += 1;
            if self.step_count >= block.steps.step_event_count {
                self.current_block = None;
            }
        }
    }
}
