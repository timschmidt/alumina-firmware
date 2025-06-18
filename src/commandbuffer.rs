use core::cmp;

#[derive(Default, Clone)]
pub struct Block {
    pub steps: Steps,        // already present
    pub feed_rate: f32,      // mm / min
    pub acceleration: f32,   // mm / s ^ 2
    pub deceleration: f32,   // mm / s ^ 2
    pub nominal_rate: f32,   // steps / s at the plateau
    pub entry_rate: f32,     // steps / s at the first step
    pub exit_rate: f32,      // steps / s at the final step
    pub accel_until: i32,    // step index where we finish accelerating
    pub decel_after: i32,    // step index where we start decelerating
}

const DEFAULT_ACCELERATION: f32  = 1200.0; // mm/s^2
const DEFAULT_DECELERATION: f32  = 1200.0; // mm/s^2

impl Block {
    pub fn new(target: Target, feed_rate: f32, extruder: u8) -> Self {
        let steps = Steps::new(target);
        Self {
            steps,
            feed_rate,
            acceleration: DEFAULT_ACCELERATION,
            deceleration: DEFAULT_DECELERATION,
            nominal_rate: 300.0,
			entry_rate: 300.0,
			exit_rate: 300.0,
			accel_until: 128,
			decel_after: 128,
        }
    }

    /// Compute the trapezoidal velocity profile for **this** move.
    /// `prev_exit_rate` must be supplied by the planner so that junction jerk is respected.
    pub fn calculate_trapezoid(&mut self, prev_exit_rate: f32) {
        let steps_total = self.steps.step_event_count as f32;

        // 1. convert the *linear* feed rate (mm/min coming in via G-code)
        //    into *step* rate in steps/s
        //    feed_rate(mm/min) * steps/mm                    / 60 → steps/s
        let target_rate   = (self.feed_rate / 60.0)
            * cmp::max(
                cmp::max(self.steps.x.abs(), self.steps.y.abs()),
                cmp::max(self.steps.z.abs(), self.steps.e.abs()),
            ) as f32
            / steps_total.max(1.0);

        self.nominal_rate = target_rate;

        // 2. acceleration expressed in steps/s²
        let acc_steps_per_s2 = self.acceleration      // mm/s²
            * cmp::max(
                cmp::max(self.steps.x.abs(), self.steps.y.abs()),
                cmp::max(self.steps.z.abs(), self.steps.e.abs()),
            ) as f32
            / steps_total.max(1.0);

        // minimum rate equals the exit rate of the **previous** move
        self.entry_rate = prev_exit_rate.min(target_rate);
        self.exit_rate  = 0.0;                        //  stop at the end for now

        // 3. solve for number of accelerating steps
        let accel_steps = ((target_rate.powi(2) - self.entry_rate.powi(2))
                          / (2.0 * acc_steps_per_s2))
                          .ceil()
                          .max(0.0);

        let decel_steps = ((target_rate.powi(2) - self.exit_rate.powi(2))
                          / (2.0 * acc_steps_per_s2))
                          .ceil()
                          .max(0.0);

        // If they overlap we have a triangle, clip both halves
        let plateau_steps = steps_total - accel_steps - decel_steps;
        let (accel_steps, decel_steps) = if plateau_steps < 0.0 {
            //  Re-scale so they meet in the middle
            let accel = accel_steps / (accel_steps + decel_steps) * steps_total;
            let decel = steps_total - accel;
            (accel, decel)
        } else {
            (accel_steps, decel_steps)
        };

        self.accel_until = accel_steps as i32;
        self.decel_after = (steps_total - decel_steps) as i32;
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
