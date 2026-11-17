//! Fixed-timestep driver. The simulation always advances in DT steps no
//! matter the display refresh rate; rendering just reads the latest state.

pub const DT: f32 = 1.0 / 120.0;
/// Most steps one frame catches up on (50 ms of play: real time down to
/// 20 fps). Time past that is dropped and the game slows down instead. If
/// steps cost more than DT, every frame owes more steps than the last, so the
/// frame rate collapses to however long this many steps take: a cap of a
/// quarter second (30 steps) pinned a heavy fight at ~5 fps.
const MAX_STEPS: u32 = 6;

#[derive(Default)]
pub struct Clock {
    accumulator: f32,
}

impl Clock {
    /// Returns how many fixed steps to run for this frame.
    pub fn advance(&mut self, frame_dt: f32) -> u32 {
        self.accumulator += frame_dt.max(0.0);
        let steps = ((self.accumulator / DT) as u32).min(MAX_STEPS);
        self.accumulator -= steps as f32 * DT;
        // What's owed past the cap is dropped, not carried into the next frame.
        self.accumulator = self.accumulator.min(DT);
        steps
    }

    /// How far (0..1) real time has run past the last sim step, for interpolation.
    pub fn alpha(&self) -> f32 {
        (self.accumulator / DT).clamp(0.0, 1.0)
    }
}

/// Scripted input for headless runs: drive around the block so screenshots
/// show motion, suspension tilt, and turret lag.
pub fn autopilot(time: f32) -> crate::sim::Command {
    crate::sim::Command {
        throttle: if (time % 6.0) < 5.0 { 1.0 } else { -0.6 },
        steer: (time * 0.7).sin() * 0.8,
        // Sweep the turret back and forth.
        turret: (time * 0.5).sin(),
        center_turret: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_is_rate_independent() {
        let mut fast = Clock::default();
        let fast_steps: u32 = (0..144).map(|_| fast.advance(1.0 / 144.0)).sum();
        let mut slow = Clock::default();
        let slow_steps: u32 = (0..30).map(|_| slow.advance(1.0 / 30.0)).sum();
        assert!((fast_steps as i32 - 120).abs() <= 1);
        assert!((slow_steps as i32 - 120).abs() <= 1);
    }

    #[test]
    fn clock_caps_long_stalls() {
        let mut c = Clock::default();
        assert_eq!(c.advance(5.0), MAX_STEPS);
        // The stall isn't paid back over the frames after it.
        assert!(c.advance(1.0 / 60.0) <= 3);
    }

    /// Steps that cost more than DT (a heavy fight on a slow machine) slow
    /// the game down rather than collapsing the frame rate.
    #[test]
    fn slow_steps_dont_spiral() {
        let (step_cost, draw_cost) = (0.012, 0.015);
        let mut c = Clock::default();
        let mut dt = 1.0 / 60.0;
        for _ in 0..600 {
            dt = c.advance(dt) as f32 * step_cost + draw_cost;
        }
        assert!(1.0 / dt > 10.0, "{:.1} fps", 1.0 / dt);
    }
}
