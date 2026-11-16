//! Track clatter: a clank each time a link meets a sprocket, and the ground
//! rumbling under the belts.

use crate::audio::dsp::{Noise, Smooth, Svf};
use crate::audio::mixer::Coefs;
use crate::audio::tank::{Loop, TankParams};

#[derive(Default)]
pub struct Tracks {
    tread: Smooth,
    hover: bool,
    /// Phase to the next link.
    link: f32,
    clank_a: Svf,
    clank_b: Svf,
    rumble: Svf,
}

impl Loop for Tracks {
    fn set(&mut self, p: &TankParams) {
        self.tread.target = p.tread;
        self.hover = p.hover;
    }

    fn next(&mut self, n: f32, noise: &mut Noise, sr: f32, k: &Coefs) -> f32 {
        let tread = self.tread.next(k.slow);
        if tread <= 0.05 || self.hover {
            return 0.0;
        }
        self.link += tread / 0.3 / sr;
        let mut hit = 0.0;
        if self.link >= 1.0 {
            self.link -= 1.0;
            hit = 0.5 + noise.unit();
        }
        let x = hit + n * 0.04;
        let a = self.clank_a.run(x, 1100.0, 6.0, sr).band;
        let b = self.clank_b.run(x, 2700.0, 9.0, sr).band;
        let rumble = self.rumble.run(n, 170.0, 0.7, sr).low;
        (a * 0.6 + b * 0.35) * (tread / 5.0).min(1.0) * 0.5 + rumble * (tread / 8.0).min(1.0) * 0.9
    }
}
