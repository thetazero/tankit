//! A tank burning in a plasma river: it sizzles and spits, over a low roar
//! of fire, for as long as it's in there.

use crate::audio::dsp::{Noise, OnePole, Smooth, Svf};
use crate::audio::mixer::Coefs;
use crate::audio::tank::{Loop, TankParams};

/// How loud tank `i` burns: your own is all round you.
pub fn level(i: usize) -> f32 {
    if i == 0 { 1.3 } else { 1.0 }
}

#[derive(Default)]
pub struct Sizzle {
    level: Smooth,
    flutter: OnePole,
    fry: Svf,
    spit: Svf,
    roar: Svf,
}

impl Loop for Sizzle {
    fn set(&mut self, p: &TankParams) {
        self.level.target = p.plasma;
    }

    fn next(&mut self, n: f32, noise: &mut Noise, sr: f32, k: &Coefs) -> f32 {
        let level = self.level.next(k.fast);
        if level <= 0.001 {
            return 0.0;
        }
        // Frying, its level fluttering like fat in a pan.
        let flutter = self.flutter.run(noise.next(), k.wobble * 4.0).abs() * 6.0;
        let fry = self.fry.run(n, 3000.0, 0.7, sr).band * (0.35 + flutter.min(1.4));
        // Plate spitting.
        let spit = if noise.unit() < 60.0 / sr {
            noise.next() * 5.0
        } else {
            0.0
        };
        let spit = self.spit.run(spit, 1700.0, 1.3, sr).band;
        let roar = self.roar.run(n, 520.0, 0.8, sr).low;
        (fry * 0.6 + spit * 0.8 + roar * 1.1) * level * 0.9
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::mixer::Mixer;
    use crate::audio::testkit::{alone, driving, mids, settle};

    fn burning(plasma: f32) -> TankParams {
        TankParams {
            gain: 1.0,
            air: 16_000.0,
            plasma,
            ..TankParams::default()
        }
    }

    /// Over the tank's own engine, on small speakers too.
    #[test]
    fn a_burning_tank_is_heard() {
        let none = Default::default();
        let engine = mids(&alone(driving(1.0), none));
        for i in [0, 1] {
            let on = mids(&alone(burning(level(i)), none));
            assert!(on > engine, "tank {i}: burning {on}, engine {engine}");
        }
    }

    #[test]
    fn it_stops_when_its_out() {
        let mut m = Mixer::new(48_000.0);
        m.set_master(1.0);
        m.set_world(1.0);
        m.set_tanks(&[burning(1.0)]);
        assert!(mids(&settle(&mut m)) > 0.02);
        m.set_tanks(&[burning(0.0)]);
        let off = settle(&mut m);
        m.set_tanks(&[TankParams::default()]);
        let dead = settle(&mut m);
        // (What's left is the idling engine.)
        let idle = mids(&alone(burning(0.0), Default::default()));
        assert!(mids(&off) < idle * 1.5 + 1e-3);
        assert!(mids(&dead) < 1e-3);
    }
}
