use crate::commandbuffer::Block;
use crate::commandbuffer::Target;

const X_AXIS_STEPS_PER_UNIT: f32 = 10.0;
const Y_AXIS_STEPS_PER_UNIT: f32 = 10.0;
const Z_AXIS_STEPS_PER_UNIT: f32 = 10.0;
const E_AXIS_STEPS_PER_UNIT: f32 = 10.0;

pub struct Planner {
    block_buffer: Vec<Block>,  // Motion instructions buffer
    head: usize,               // Index of the next block to be pushed
    tail: usize,               // Index of the block to process
}

impl Planner {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            block_buffer: vec![Block::default(); buffer_size],
            head: 0,
            tail: 0,
        }
    }

    // Adds a new movement to the buffer
    pub fn buffer_line(&mut self, x: f32, y: f32, z: f32, e: f32, feed_rate: f32, extruder: u8) {
        let next_head = (self.head + 1) % self.block_buffer.len();
        if next_head == self.tail {
            // Buffer is full
            return;
        }

        let target = Target {
            x: (x * X_AXIS_STEPS_PER_UNIT) as i32,
            y: (y * Y_AXIS_STEPS_PER_UNIT) as i32,
            z: (z * Z_AXIS_STEPS_PER_UNIT) as i32,
            e: (e * E_AXIS_STEPS_PER_UNIT) as i32,
        };

        let block = Block::new(target, feed_rate, extruder);
        self.block_buffer[self.head] = block;
        self.head = next_head;
    }

    pub fn recalculate_trapezoids(&mut self) {
		let mut block_index = self.tail;
		let mut prev_exit_rate = 0.0;

		while block_index != self.head {
			let block = &mut self.block_buffer[block_index];
			block.calculate_trapezoid(prev_exit_rate);
			// for the next one this becomes the “previous” speed
			prev_exit_rate = block.exit_rate;

			block_index = (block_index + 1) % self.block_buffer.len();
		}
	}
}
