//! Motion blocks and their trapezoidal step-rate profiles.

use core::cmp;

/// One buffered move expressed as per-axis steps and a step-rate profile.
#[derive(Default, Clone)]
pub struct Block {
    pub steps: Steps,
    /// Requested linear feed rate in millimetres per minute.
    pub feed_rate: f32,
    /// Linear acceleration in millimetres per second squared.
    pub acceleration: f32,
    /// Linear deceleration in millimetres per second squared.
    pub deceleration: f32,
    /// Plateau rate in step events per second.
    pub nominal_rate: f32,
    /// Rate at the first step event.
    pub entry_rate: f32,
    /// Rate at the final step event.
    pub exit_rate: f32,
    /// Step-event index at which acceleration ends.
    pub accel_until: i32,
    /// Step-event index at which deceleration begins.
    pub decel_after: i32,
}

const DEFAULT_ACCELERATION: f32 = 1_200.0;
const DEFAULT_DECELERATION: f32 = 1_200.0;

impl Block {
    /// Creates a move with the default acceleration limits.
    pub fn new(target: Target, feed_rate: f32) -> Self {
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

    /// Computes this move's trapezoidal step-rate profile.
    ///
    /// `previous_exit_rate` carries velocity continuity from the preceding block. The current
    /// prototype always ends each block at rest; junction look-ahead is not implemented yet.
    pub fn calculate_trapezoid(&mut self, previous_exit_rate: f32) {
        let steps_total = self.steps.step_event_count as f32;
        if steps_total <= 0.0 {
            self.nominal_rate = 0.0;
            self.entry_rate = 0.0;
            self.exit_rate = 0.0;
            self.accel_until = 0;
            self.decel_after = 0;
            return;
        }

        let dominant_axis_steps = cmp::max(
            cmp::max(self.steps.x.abs(), self.steps.y.abs()),
            cmp::max(self.steps.z.abs(), self.steps.e.abs()),
        ) as f32;
        let target_rate = (self.feed_rate / 60.0) * dominant_axis_steps / steps_total;

        self.nominal_rate = target_rate;

        let acceleration = self.acceleration * dominant_axis_steps / steps_total;
        let deceleration = self.deceleration * dominant_axis_steps / steps_total;

        self.entry_rate = previous_exit_rate.min(target_rate);
        self.exit_rate = 0.0;

        let accel_steps = ((target_rate.powi(2) - self.entry_rate.powi(2)) / (2.0 * acceleration))
            .ceil()
            .max(0.0);

        let decel_steps = ((target_rate.powi(2) - self.exit_rate.powi(2)) / (2.0 * deceleration))
            .ceil()
            .max(0.0);

        let plateau_steps = steps_total - accel_steps - decel_steps;
        let (accel_steps, decel_steps) = if plateau_steps < 0.0 {
            // Overlapping ramps form a triangular profile, so scale them to meet once.
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

/// A move target measured in whole steps from the planner's origin.
#[derive(Clone)]
pub struct Target {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub e: i32,
}

/// Per-axis step counts and the number of synchronized step events they require.
#[derive(Default, Clone)]
pub struct Steps {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub e: i32,
    pub step_event_count: i32,
}

impl Steps {
    /// Converts a target into synchronized per-axis step counts.
    pub fn new(target: Target) -> Self {
        let step_event_count = cmp::max(
            cmp::max(target.x.abs(), target.y.abs()),
            cmp::max(target.z.abs(), target.e.abs()),
        );
        Self {
            x: target.x,
            y: target.y,
            z: target.z,
            e: target.e,
            step_event_count,
        }
    }
}
