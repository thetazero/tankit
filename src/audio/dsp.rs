//! Small DSP building blocks: noise, filters, parameter smoothing.

use std::f32::consts::PI;

/// White noise from a xorshift generator, in -1..1.
#[derive(Clone, Copy)]
pub struct Noise(u32);

impl Noise {
    pub fn new(seed: u32) -> Noise {
        Noise(seed.max(1))
    }

    pub fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Uniform in 0..1.
    pub fn unit(&mut self) -> f32 {
        self.next() * 0.5 + 0.5
    }
}

/// Outputs of one state-variable filter step.
#[derive(Clone, Copy)]
pub struct SvfOut {
    pub low: f32,
    pub band: f32,
    pub high: f32,
}

/// A trapezoidal state-variable filter (stable under fast cutoff sweeps).
#[derive(Clone, Copy, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    pub fn run(&mut self, x: f32, cutoff: f32, q: f32, sr: f32) -> SvfOut {
        let g = (PI * cutoff.clamp(10.0, sr * 0.45) / sr).tan();
        let k = 1.0 / q.max(0.05);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        SvfOut {
            low: v2,
            band: v1,
            high: x - k * v1 - v2,
        }
    }
}

/// One-pole lowpass with its coefficient precomputed (air absorption).
#[derive(Clone, Copy, Default)]
pub struct OnePole {
    y: f32,
}

impl OnePole {
    /// The coefficient for `cutoff` Hz.
    pub fn coef(cutoff: f32, sr: f32) -> f32 {
        1.0 - (-2.0 * PI * cutoff / sr).exp()
    }

    pub fn run(&mut self, x: f32, coef: f32) -> f32 {
        self.y += (x - self.y) * coef;
        self.y
    }
}

/// Blocks DC (the engine's one-sided pulses would otherwise push the speaker).
#[derive(Clone, Copy, Default)]
pub struct DcBlock {
    x1: f32,
    y1: f32,
}

impl DcBlock {
    pub fn run(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + 0.995 * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// A value easing toward its target (no zipper noise on parameter jumps).
#[derive(Clone, Copy, Default)]
pub struct Smooth {
    pub value: f32,
    pub target: f32,
}

impl Smooth {
    pub fn new(value: f32) -> Smooth {
        Smooth {
            value,
            target: value,
        }
    }

    /// Per-sample coefficient for a time constant of `tau` seconds.
    pub fn coef(tau: f32, sr: f32) -> f32 {
        1.0 - (-1.0 / (tau * sr)).exp()
    }

    pub fn next(&mut self, coef: f32) -> f32 {
        self.value += (self.target - self.value) * coef;
        self.value
    }
}

/// Equal-power stereo gains for `pan` in -1 (left) .. 1 (right), unity in the middle.
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * PI * 0.25;
    (
        a.cos() * std::f32::consts::SQRT_2,
        a.sin() * std::f32::consts::SQRT_2,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_stays_in_range_and_moves() {
        let mut n = Noise::new(7);
        let xs: Vec<f32> = (0..10_000).map(|_| n.next()).collect();
        assert!(xs.iter().all(|x| (-1.0..=1.0).contains(x)));
        let mean = xs.iter().sum::<f32>() / xs.len() as f32;
        assert!(mean.abs() < 0.05, "mean {mean}");
    }

    #[test]
    fn lowpass_passes_dc_and_cuts_highs() {
        let sr = 48_000.0;
        let mut f = Svf::default();
        let mut y = 0.0;
        for _ in 0..4800 {
            y = f.run(1.0, 500.0, 0.7, sr).low;
        }
        assert!((y - 1.0).abs() < 0.01);
        // A tone well above cutoff comes out much quieter.
        let mut f = Svf::default();
        let mut peak: f32 = 0.0;
        for i in 0..4800 {
            let x = (i as f32 * 2.0 * PI * 8000.0 / sr).sin();
            let o = f.run(x, 200.0, 0.7, sr).low;
            if i > 2400 {
                peak = peak.max(o.abs());
            }
        }
        assert!(peak < 0.01, "peak {peak}");
    }

    #[test]
    fn pan_is_equal_power() {
        let (l, r) = pan_gains(0.0);
        assert!((l - 1.0).abs() < 1e-5 && (r - 1.0).abs() < 1e-5);
        let (l, r) = pan_gains(1.0);
        assert!(l.abs() < 1e-5 && r > 1.4);
        for p in [-0.7, -0.2, 0.4, 0.9] {
            let (l, r) = pan_gains(p);
            assert!((l * l + r * r - 2.0).abs() < 1e-4);
        }
    }
}
