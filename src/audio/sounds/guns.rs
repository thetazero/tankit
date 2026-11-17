//! The guns: each turret's report as it fires, and the flamer's roar (a
//! loop, as it pours rather than fires shots).

use glam::Vec3;

use super::Play;
use crate::audio::dsp::{Noise, OnePole, Smooth, Svf};
use crate::audio::mixer::Coefs;
use crate::audio::tank::{Loop, TankParams};
use crate::audio::voice::{Filt, Layer, noise, ring, sine};
use crate::sim::TurretKind;

/// How long a flamer keeps roaring after its last puff, s.
const FLAME_HOLD: f32 = 0.12;

/// Per tank: until when its flamer is pouring.
#[derive(Default)]
pub struct Flames {
    until: Vec<f32>,
}

impl Flames {
    /// Track `n` tanks.
    pub fn resize(&mut self, n: usize) {
        self.until.resize(n, 0.0);
    }

    /// `tank` fired its `turret` at `time`: a report, or (for the flamer
    /// and the beam, which are loops) nothing.
    pub fn shot(
        &mut self,
        tank: usize,
        turret: TurretKind,
        at: Vec3,
        time: f32,
        jitter: f32,
    ) -> Option<Play> {
        let layers = match turret {
            TurretKind::Kontos => KONTOS,
            TurretKind::Pelekys => PELEKYS,
            TurretKind::Ammeth => WISP,
            TurretKind::Ossrin => TRIAD,
            TurretKind::Knell => KNELL,
            TurretKind::Hymn => {
                if let Some(t) = self.until.get_mut(tank) {
                    *t = time + FLAME_HOLD;
                }
                return None;
            }
            TurretKind::Lumen => return None,
        };
        Some(Play::at(layers, at, 1.0, jitter))
    }

    /// Whether tank `i`'s flamer is still pouring at `time`.
    pub fn pouring(&self, i: usize, time: f32) -> bool {
        self.until.get(i).is_some_and(|&u| u > time)
    }
}

/// Imperium rail cannon: a heavy gun's report. A falling slam (with its
/// overtones, so small speakers still hear the drop), a whump of blast that
/// closes fast, only milliseconds of crack, then a long rolling rumble and its
/// echo. Kept out of the 1-5 kHz band where a snare drum's wires live.
pub const KONTOS: &[Layer] = &[
    sine(110.0, 38.0, 0.07, 0.3, 0.5),
    sine(220.0, 76.0, 0.07, 0.18, 0.35),
    sine(330.0, 114.0, 0.07, 0.1, 0.2),
    noise(Filt::Low, 3000.0, 380.0, 0.04, 0.3, 1.3),
    Layer {
        q: 0.6,
        ..noise(Filt::Band, 700.0, 300.0, 0.15, 0.35, 0.9)
    },
    noise(Filt::High, 1800.0, 1800.0, 0.1, 0.006, 0.35),
    Layer {
        attack: 0.02,
        ..noise(Filt::Low, 650.0, 180.0, 0.5, 0.9, 0.8)
    },
    Layer {
        delay: 0.18,
        attack: 0.03,
        ..noise(Filt::Low, 380.0, 120.0, 0.4, 0.7, 0.25)
    },
    sine(2600.0, 800.0, 0.08, 0.08, 0.05),
];

/// Imperium twin autocannon: the same report, shorter and punchier.
pub const PELEKYS: &[Layer] = &[
    sine(130.0, 48.0, 0.05, 0.14, 0.5),
    sine(260.0, 96.0, 0.05, 0.08, 0.3),
    noise(Filt::Low, 3200.0, 420.0, 0.03, 0.15, 1.2),
    Layer {
        q: 0.6,
        ..noise(Filt::Band, 800.0, 350.0, 0.08, 0.16, 0.7)
    },
    noise(Filt::High, 2200.0, 2200.0, 0.1, 0.005, 0.3),
    Layer {
        attack: 0.01,
        ..noise(Filt::Low, 600.0, 170.0, 0.3, 0.4, 0.5)
    },
];

/// Sorn orb launcher: a falling, hollow "thoom".
pub const WISP: &[Layer] = &[
    sine(900.0, 170.0, 0.12, 0.25, 0.42),
    sine(1350.0, 255.0, 0.12, 0.18, 0.18),
    Layer {
        q: 3.0,
        ..noise(Filt::Band, 1800.0, 500.0, 0.1, 0.17, 0.3)
    },
    sine(75.0, 60.0, 0.1, 0.15, 0.4),
];

/// Sorn three-barrel: quick plasma darts.
pub const TRIAD: &[Layer] = &[
    sine(1400.0, 520.0, 0.05, 0.07, 0.28),
    Layer {
        q: 4.0,
        ..noise(Filt::Band, 2600.0, 900.0, 0.05, 0.05, 0.18)
    },
    sine(110.0, 80.0, 0.05, 0.05, 0.2),
];

/// The Knell's bell, far under the Vigil's: hum, prime, minor third,
/// fifth, nominal, twelfth, Hz.
const KNELL_BELL: [f32; 6] = [55.0, 110.0, 132.0, 165.0, 220.0, 330.0];

/// Resonant siege bell tolling: a blow like a building falling, the great
/// bell struck and left to ring, and a long rolling wash of air after it.
pub const KNELL: &[Layer] = &[
    // The blow.
    sine(90.0, 34.0, 0.1, 0.35, 0.7),
    noise(Filt::Low, 2400.0, 160.0, 0.08, 0.4, 0.9),
    Layer {
        q: 0.7,
        ..noise(Filt::Band, 600.0, 220.0, 0.2, 0.5, 0.6)
    },
    // The bell.
    ring(KNELL_BELL[0], 1.6, 0.45),
    ring(KNELL_BELL[1], 1.4, 0.4),
    ring(KNELL_BELL[2], 1.1, 0.22),
    ring(KNELL_BELL[3], 0.9, 0.18),
    ring(KNELL_BELL[4], 0.7, 0.16),
    ring(KNELL_BELL[5], 0.45, 0.1),
    // Crystal singing over it.
    sine(1760.0, 1745.0, 0.5, 0.8, 0.04),
    // The wash rolling out.
    Layer {
        delay: 0.12,
        attack: 0.1,
        ..noise(Filt::Low, 500.0, 90.0, 1.0, 1.0, 0.45)
    },
];

/// A Knell let go early: the pipes' breath sighing out, falling, louder
/// the further the charge had got.
pub fn exhale(at: Vec3, charge: f32) -> Play {
    Play::at(EXHALE, at, 0.4 + 0.8 * charge, 0.7 + 0.4 * charge)
}

const EXHALE: &[Layer] = &[
    Layer {
        attack: 0.02,
        q: 1.5,
        ..noise(Filt::Band, 1400.0, 300.0, 0.25, 0.3, 0.45)
    },
    sine(220.0, 110.0, 0.2, 0.3, 0.2),
    sine(330.0, 150.0, 0.2, 0.25, 0.12),
];

/// A Knell ringing up: an organ chord breathed through the pipes, rising
/// and swelling as the charge builds, with the crystal's shimmer over it.
#[derive(Default)]
pub struct Hum {
    level: Smooth,
    phase: [f32; 4],
    breath: Svf,
}

impl Loop for Hum {
    fn set(&mut self, p: &TankParams) {
        self.level.target = p.charge;
    }

    fn next(&mut self, n: f32, _noise: &mut Noise, sr: f32, k: &Coefs) -> f32 {
        let c = self.level.next(k.fast);
        if c <= 0.001 {
            return 0.0;
        }
        // Root, fifth, octave and a shimmering twelfth, all climbing an
        // octave over the charge.
        let root = 55.0 * (1.0 + c);
        let mut tone = 0.0;
        for (i, (mul, gain)) in [(1.0, 0.5), (1.5, 0.3), (2.0, 0.25), (3.0 + 0.004, 0.12)]
            .into_iter()
            .enumerate()
        {
            self.phase[i] = (self.phase[i] + root * mul / sr).fract();
            tone += (self.phase[i] * std::f32::consts::TAU).sin() * gain;
        }
        let breath = self.breath.run(n, 700.0 + 1800.0 * c, 1.2, sr).band;
        (tone + breath * 0.3) * c.powf(1.5) * 0.8
    }
}

/// Flamethrower: a roar with crackle in it.
#[derive(Default)]
pub struct Flame {
    level: Smooth,
    roar: Svf,
    crackle: Svf,
    hiss: Svf,
    wobble: OnePole,
}

impl Loop for Flame {
    fn set(&mut self, p: &TankParams) {
        self.level.target = p.flame;
    }

    fn next(&mut self, n: f32, noise: &mut Noise, sr: f32, k: &Coefs) -> f32 {
        let flame = self.level.next(k.fast);
        if flame <= 0.001 {
            return 0.0;
        }
        let wobble = self.wobble.run(noise.next(), k.wobble);
        let roar = self.roar.run(n, 900.0 + 2500.0 * wobble.abs(), 0.8, sr).low;
        let hiss = self.hiss.run(n, 4500.0, 0.7, sr).band;
        let spark = if noise.unit() < 70.0 / sr {
            noise.next() * 2.5
        } else {
            0.0
        };
        let crackle = self.crackle.run(spark, 2200.0, 1.2, sr).band;
        (roar * 1.1 + hiss * 0.35 + crackle * 0.6) * flame * 0.6
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::testkit::{band, sane};
    use crate::audio::voice::{Voice, length};

    #[test]
    fn every_sound_is_sane() {
        sane(&[
            ("kontos", KONTOS),
            ("pelekys", PELEKYS),
            ("wisp", WISP),
            ("triad", TRIAD),
            ("knell", KNELL),
            ("exhale", EXHALE),
        ]);
    }

    /// Issue #77: the Imperium guns sounded like a snare drum. Their slam sat
    /// below what small speakers play, leaving a burst of 1-5 kHz noise. A
    /// cannon's audible body is low-mid, and its report rolls on.
    #[test]
    fn imperium_guns_boom_not_snap() {
        let sr = 48_000.0;
        let render = |layers: &'static [Layer]| {
            let mut v = Voice::new(layers, 1.0, 1.0, (1.0, 0.0), 1.0, 7);
            let mut buf = vec![0.0; 2 * (length(layers) * sr) as usize];
            v.render(&mut buf, sr);
            buf.iter().step_by(2).copied().collect::<Vec<f32>>()
        };
        for (name, layers) in [("kontos", KONTOS), ("pelekys", PELEKYS)] {
            let x = render(layers);
            let body = band(&x, 150.0, 900.0, sr);
            let snare = band(&x, 1500.0, 6000.0, sr);
            // The old patches scored 0.9 and 2.7 here.
            assert!(
                body > 4.0 * snare,
                "{name}: body {body:.2}, snare {snare:.2}"
            );
        }
        // The main gun's report rolls on (the old one was 0.05 here).
        let x = render(KONTOS);
        let tail = band(&x[(0.25 * sr) as usize..], 150.0, 900.0, sr);
        let body = band(&x, 150.0, 900.0, sr);
        assert!(tail > 0.08 * body, "kontos stops dead: {:.3}", tail / body);
    }
}
