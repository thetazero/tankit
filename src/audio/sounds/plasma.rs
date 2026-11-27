//! A tank burning in a plasma river: it sizzles and spits, over a low roar
//! of fire, for as long as it's in there. And what splashes as it goes in.

use glam::Vec3;

use super::Play;
use crate::audio::dsp::{Noise, OnePole, Smooth, Svf};
use crate::audio::mixer::Coefs;
use crate::audio::tank::{Loop, TankParams};
use crate::audio::voice::{Filt, L, Layer, Src, noise, sine};

/// Something about `size` m across going into a plasma river: a tank or its
/// wreck, or a shell. The bigger, the louder and lower.
pub fn splash(at: Vec3, size: f32, jitter: f32) -> Play {
    if size > 2.0 {
        Play::at(PLUNGE, at, 1.2, jitter * (3.0 / size).powf(0.5))
    } else {
        let gain = 0.4 + 0.4 * size;
        Play::at(PLOP, at, gain, jitter * size.max(0.3).powf(-0.4))
    }
}

/// A shell going in: a gulp, and a spatter of plasma.
pub const PLOP: &[Layer] = &[
    sine(130.0, 330.0, 0.09, 0.1, 0.4),
    Layer {
        q: 0.8,
        ..noise(Filt::Band, 2400.0, 700.0, 0.12, 0.16, 0.6)
    },
    Layer {
        src: Src::Crackle(320.0),
        filt: Filt::Band,
        cutoff: (3600.0, 2200.0),
        q: 1.4,
        decay: 0.25,
        gain: 0.35,
        delay: 0.06,
        ..L
    },
];

/// A tank going in: a heavy slap, the plasma closing over it, what it threw
/// up hissing back down.
pub const PLUNGE: &[Layer] = &[
    sine(80.0, 38.0, 0.15, 0.3, 0.8),
    noise(Filt::Low, 3200.0, 260.0, 0.22, 0.4, 0.9),
    Layer {
        attack: 0.03,
        delay: 0.18,
        ..sine(70.0, 170.0, 0.3, 0.28, 0.45)
    },
    Layer {
        attack: 0.12,
        delay: 0.1,
        q: 0.7,
        ..noise(Filt::Band, 2600.0, 900.0, 0.8, 0.6, 0.5)
    },
    Layer {
        src: Src::Crackle(260.0),
        filt: Filt::Band,
        cutoff: (3400.0, 1800.0),
        sweep: 0.6,
        q: 1.2,
        attack: 0.1,
        decay: 0.7,
        gain: 0.55,
        delay: 0.3,
        ..L
    },
];

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
    use crate::audio::testkit::{alone, driving, mids, sane, settle};

    fn burning(plasma: f32) -> TankParams {
        TankParams {
            gain: 1.0,
            air: 16_000.0,
            plasma,
            ..TankParams::default()
        }
    }

    #[test]
    fn every_sound_is_sane() {
        sane(&[("plop", PLOP), ("plunge", PLUNGE)]);
    }

    #[test]
    fn bigger_splashes_are_louder_and_lower() {
        let [bolt, shell, tank, heavy] =
            [0.6, 1.2, 3.0, 4.2].map(|size| splash(Vec3::ZERO, size, 1.0));
        assert!(bolt.gain < shell.gain && shell.gain < tank.gain);
        assert!(bolt.pitch > shell.pitch && tank.pitch > heavy.pitch);
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
