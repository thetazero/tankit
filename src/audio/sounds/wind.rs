//! Desert wind under everything: two drifting bands of noise, one per ear.

use crate::audio::dsp::{Noise, Smooth, Svf};
use crate::audio::mixer::Coefs;

pub struct Wind {
    level: Smooth,
    left: Svf,
    right: Svf,
    /// Where the gusts are drifting to, 0..1, and when they turn next.
    gust: Smooth,
    timer: f32,
    noise: Noise,
}

impl Wind {
    pub fn new() -> Wind {
        Wind {
            level: Smooth::new(0.0),
            left: Svf::default(),
            right: Svf::default(),
            gust: Smooth::new(0.5),
            timer: 0.0,
            noise: Noise::new(12345),
        }
    }

    pub fn set(&mut self, gain: f32) {
        self.level.target = gain;
    }

    /// Add the wind into `out` (stereo frames).
    pub fn render(&mut self, out: &mut [f32], sr: f32, k: &Coefs) {
        if self.level.value < 1e-4 && self.level.target < 1e-4 {
            return;
        }
        let drift = Smooth::coef(1.5, sr);
        for frame in out.chunks_exact_mut(2) {
            self.timer -= 1.0 / sr;
            if self.timer <= 0.0 {
                self.timer = 1.5 + 3.0 * self.noise.unit();
                self.gust.target = self.noise.unit();
            }
            let m = self.gust.next(drift);
            let g = self.level.next(k.slow) * (0.5 + m);
            let cutoff = 220.0 + 500.0 * m;
            let l = self.left.run(self.noise.next(), cutoff, 1.4, sr).band;
            let r = self
                .right
                .run(self.noise.next(), cutoff * 1.1, 1.4, sr)
                .band;
            frame[0] += l * g;
            frame[1] += r * g;
        }
    }
}
