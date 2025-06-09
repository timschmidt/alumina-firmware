const DEFAULT_ACCELERATION: f32 = 0.0;
const DEFAULT_DECELERATION: f32 = 0.0;

#[derive(Default, Clone)]
pub struct Block {
    pub steps: Steps,
    pub feed_rate: f32,
    pub acceleration: f32,
    pub deceleration: f32,
    // More fields for jerk control, etc.
}

impl Block {
    pub fn new(target: Target, feed_rate: f32, extruder: u8) -> Self {
        let steps = Steps::new(target);
        Self {
            steps,
            feed_rate,
            acceleration: DEFAULT_ACCELERATION,
            deceleration: DEFAULT_DECELERATION,
        }
    }

    pub fn calculate_trapezoid(&mut self) {
        // Implement the trapezoidal calculation
    }
}

#[derive(Clone)]
pub struct Target {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub e: i32,
}

#[derive(Default, Clone)]
pub struct Steps {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub e: i32,
    pub step_event_count: i32,
}

impl Steps {
    pub fn new(target: Target) -> Self {
        let step_event_count = i32::max(target.x.abs(), i32::max(target.y.abs(), target.z.abs()));
        Self {
            x: target.x,
            y: target.y,
            z: target.z,
            e: target.e,
            step_event_count,
        }
    }
}
