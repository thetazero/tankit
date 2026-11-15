//! Fixed-timestep driver. The simulation always advances in DT steps no
//! matter the display refresh rate; rendering just reads the latest state.

pub const DT: f32 = 1.0 / 120.0;
/// Never simulate more than this much wall time per frame (avoids a spiral of death after a stall).
const MAX_FRAME: f32 = 0.25;

#[derive(Default)]
pub struct Clock {
    accumulator: f32,
}

impl Clock {
    /// Returns how many fixed steps to run for this frame.
    pub fn advance(&mut self, frame_dt: f32) -> u32 {
        self.accumulator += frame_dt.clamp(0.0, MAX_FRAME);
        let steps = (self.accumulator / DT) as u32;
        self.accumulator -= steps as f32 * DT;
        steps
    }
}

#[derive(Default)]
pub struct Game {
    pub ticks: u64,
}

impl Game {
    pub fn step(&mut self) {
        self.ticks += 1;
    }

    pub fn time(&self) -> f32 {
        self.ticks as f32 * DT
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
        assert_eq!(c.advance(5.0), (MAX_FRAME / DT) as u32);
    }
}
