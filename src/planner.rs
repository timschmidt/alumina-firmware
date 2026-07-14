//! Fixed-capacity motion planning queue.

use crate::commandbuffer::{Block, Target};

const X_AXIS_STEPS_PER_UNIT: f32 = 10.0;
const Y_AXIS_STEPS_PER_UNIT: f32 = 10.0;
const Z_AXIS_STEPS_PER_UNIT: f32 = 10.0;
const E_AXIS_STEPS_PER_UNIT: f32 = 10.0;

/// Buffers moves and derives their trapezoidal step-rate profiles.
pub struct Planner {
    block_buffer: Vec<Block>,
    head: usize,
    tail: usize,
}

impl Planner {
    /// Creates an empty ring buffer with room for `buffer_size - 1` moves.
    ///
    /// # Panics
    ///
    /// Panics if `buffer_size` is less than two.
    pub fn new(buffer_size: usize) -> Self {
        assert!(
            buffer_size >= 2,
            "planner buffer must contain at least two slots"
        );
        Self {
            block_buffer: vec![Block::default(); buffer_size],
            head: 0,
            tail: 0,
        }
    }

    /// Adds a linear move, returning `false` when the queue is full.
    pub fn buffer_line(&mut self, x: f32, y: f32, z: f32, e: f32, feed_rate: f32) -> bool {
        let next_head = (self.head + 1) % self.block_buffer.len();
        if next_head == self.tail {
            return false;
        }

        let target = Target {
            x: (x * X_AXIS_STEPS_PER_UNIT) as i32,
            y: (y * Y_AXIS_STEPS_PER_UNIT) as i32,
            z: (z * Z_AXIS_STEPS_PER_UNIT) as i32,
            e: (e * E_AXIS_STEPS_PER_UNIT) as i32,
        };

        let block = Block::new(target, feed_rate);
        self.block_buffer[self.head] = block;
        self.head = next_head;
        true
    }

    /// Recomputes profiles for all queued blocks in execution order.
    pub fn recalculate_trapezoids(&mut self) {
        let mut block_index = self.tail;
        let mut previous_exit_rate = 0.0;

        while block_index != self.head {
            let block = &mut self.block_buffer[block_index];
            block.calculate_trapezoid(previous_exit_rate);
            previous_exit_rate = block.exit_rate;
            block_index = (block_index + 1) % self.block_buffer.len();
        }
    }
}
