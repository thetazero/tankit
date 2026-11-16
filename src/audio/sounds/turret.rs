//! Turret traverse: a heavy ring gear grinding round. Teeth meet tens of
//! times a second, so they're heard as a ratchet, not a pitch; under them a
//! low grind and a near-steady drive hum. Nothing tonal in the voice range (a
//! pitched whine that rose with speed sounded like a scream).

use std::f32::consts::TAU;

use crate::audio::dsp::{Noise, Smooth, Svf};
use crate::audio::mixer::Coefs;
use crate::audio::tank::{Loop, TankParams};

#[derive(Default)]
pub struct Traverse {
    speed: Smooth,
    /// Ring-gear tooth phase, the drive's hum phase.
    tooth: f32,
    hum: f32,
    grind: Svf,
    clack_a: Svf,
    clack_b: Svf,
}

impl Loop for Traverse {
    fn set(&mut self, p: &TankParams) {
        self.speed.target = p.turret;
    }

    fn next(&mut self, n: f32, noise: &mut Noise, sr: f32, k: &Coefs) -> f32 {
        let turret = self.speed.next(k.fast);
        if turret <= 0.01 {
            return 0.0;
        }
        self.tooth += (14.0 + 26.0 * turret) / sr;
        let mut tooth = 0.0;
        if self.tooth >= 1.0 {
            self.tooth -= 1.0;
            tooth = 0.6 + 0.8 * noise.unit();
        }
        let x = tooth + n * 0.02;
        let clack = self.clack_a.run(x, 520.0, 4.0, sr).band * 0.5
            + self.clack_b.run(x, 1350.0, 6.0, sr).band * 0.3;
        let grind = self.grind.run(n, 260.0 + 120.0 * turret, 0.8, sr).low
            * (0.7 + 0.6 * (1.0 - self.tooth));
        self.hum = (self.hum + (46.0 + 8.0 * turret) / sr).fract();
        let hum = (self.hum * TAU).sin() * 0.5 + (self.hum * TAU * 2.0).sin() * 0.25;
        (clack * 1.6 + grind * 2.4 + hum * 0.25) * turret.sqrt() * 0.3
    }
}
