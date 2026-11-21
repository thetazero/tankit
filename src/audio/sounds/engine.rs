//! A tank's engine: revs following the belts and throttle, or a hover hull's
//! field hum.

use std::f32::consts::TAU;

use crate::audio::dsp::{DcBlock, Noise, Quick, Smooth, Svf};
use crate::audio::mixer::Coefs;
use crate::audio::tank::{Loop, TankParams};

pub struct Engine {
    rpm: Smooth,
    load: Smooth,
    heavy: bool,
    hover: bool,
    /// Phase through one firing stroke, and how hard this stroke fires.
    cycle: f32,
    cylinder: f32,
    lp: Svf,
    dc: DcBlock,
}

impl Default for Engine {
    fn default() -> Engine {
        Engine {
            rpm: Smooth::new(0.0),
            load: Smooth::new(0.0),
            heavy: false,
            hover: false,
            cycle: 0.0,
            cylinder: 1.0,
            lp: Svf::default(),
            dc: DcBlock::default(),
        }
    }
}

impl Loop for Engine {
    fn set(&mut self, p: &TankParams) {
        self.rpm.target = p.rpm;
        self.load.target = p.load;
        self.heavy = p.heavy;
        self.hover = p.hover;
    }

    fn next(&mut self, n: f32, noise: &mut Noise, sr: f32, k: &Coefs) -> f32 {
        let rpm = self.rpm.next(k.rpm);
        let load = self.load.next(k.slow);
        let base = match (self.hover, self.heavy) {
            (true, _) => 58.0,
            (_, true) => 22.0,
            _ => 29.0,
        };
        // A pulse per firing stroke, each cylinder a little different.
        let f0 = base * (1.0 + 1.9 * rpm);
        self.cycle += f0 / sr;
        if self.cycle >= 1.0 {
            self.cycle -= 1.0;
            self.cylinder = 0.75 + 0.5 * noise.unit();
        }
        let pulse = (-self.cycle * 7.0).exp() * self.cylinder;
        let sub = (self.cycle * TAU * 0.5).sin_q();
        let raw = pulse + sub * 0.3 + n * 0.15 * pulse * (0.4 + load);
        let cutoff = 220.0 + 1000.0 * load + 600.0 * rpm;
        let engine = if self.hover {
            // Field hum: a steady tone and its fifth, rising with effort, over hiss.
            let hum = (self.cycle * TAU).sin_q() * 0.7 + (self.cycle * TAU * 3.0).sin_q() * 0.25;
            let air = self.lp.run(n, 500.0 + 1200.0 * rpm, 1.5, sr).band;
            (hum + air * 0.8) * (0.15 + 0.2 * rpm + 0.15 * load)
        } else {
            self.dc.run(self.lp.run(raw, cutoff, 0.9, sr).low) * (0.3 + 0.4 * rpm + 0.3 * load)
        };
        engine * 0.55
    }
}

#[cfg(test)]
mod tests {
    use crate::audio::mixer::Mixer;
    use crate::audio::testkit::{driving, rms, settle};

    #[test]
    fn engine_gets_louder_flat_out() {
        let level = |rpm| {
            let mut m = Mixer::new(48_000.0);
            m.set_master(1.0);
            m.set_world(1.0);
            m.set_tanks(&[driving(rpm)]);
            rms(&settle(&mut m))
        };
        let (idle, flat_out) = (level(0.0), level(1.0));
        assert!(idle > 0.005, "idle {idle}");
        assert!(flat_out > idle * 1.5, "idle {idle}, flat out {flat_out}");
    }
}
