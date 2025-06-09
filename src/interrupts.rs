use esp_idf_hal::timer::{Timer, TimerConfig};
use crate::commandbuffer::Block;

pub struct Stepper {
    current_block: Option<Block>,
    step_count: i32,
    acceleration_time: i32,
    deceleration_time: i32,
}

impl Stepper {
    pub fn new() -> Self {
        Self {
            current_block: None,
            step_count: 0,
            acceleration_time: 0,
            deceleration_time: 0,
        }
    }

    pub fn execute_block(&mut self, block: Block) {
        self.current_block = Some(block);
        self.step_count = 0;
        // Setup hardware timer to execute steps based on the block’s profile
        //_event: TimerEvent) {
    }

    pub fn step_interrupt_handler(&mut self) {
        // Called by a hardware timer interrupt to execute a step
        if let Some(block) = &self.current_block {
            self.step_count += 1;
            if self.step_count >= block.steps.step_event_count {
                self.current_block = None; // Block complete
            }
        }
    }
}
