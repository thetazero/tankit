//! The Vigil: its crystal shell shattering, the broken husk's light singing
//! out of its cracks (a loop) and its cracked bell tolling as it runs on,
//! and the husk going out.

use std::f32::consts::TAU;

use glam::Vec3;

use super::Play;
use crate::audio::dsp::{Noise, Quick, Smooth, Svf};
use crate::audio::mixer::Coefs;
use crate::audio::tank::{Loop, TankParams};
use crate::audio::voice::{Filt, L, Layer, Src, noise, ring, sine};

/// One bell throughout: no pitch jitter.
pub fn shatter(at: Vec3) -> Play {
    Play::at(SHATTER, at, 1.3, 1.0)
}

/// Each toll weaker and a little flatter than the last.
pub fn toll(at: Vec3, strength: f32) -> Play {
    let s = strength.clamp(0.0, 1.0);
    Play::at(TOLL, at, 0.8 + 0.7 * s, 0.94 + 0.06 * s)
}

pub fn gutter(at: Vec3) -> Play {
    Play::at(GUTTER, at, 1.2, 1.0)
}

/// The Vigil's bell, a tone under the arrival's: hum, prime, minor third,
/// fifth, nominal, twelfth, Hz.
const VIGIL_BELL: [f32; 6] = [147.0, 294.0, 353.0, 441.0, 588.0, 882.0];

/// A Vigil breaking: the crystal shell cracks (a sharp snap, a shower of
/// glass, a few high crystal rings), the light rushes out with a rising
/// breath, and the bell is struck hard and left to ring.
pub const SHATTER: &[Layer] = &[
    // The snap, and the blow that cracked it.
    noise(Filt::High, 3200.0, 3200.0, 0.1, 0.012, 0.7),
    sine(95.0, 42.0, 0.08, 0.2, 0.55),
    noise(Filt::Low, 1600.0, 220.0, 0.06, 0.18, 0.5),
    // Glass: a shower of tinkling shards and the crystal singing as it splits.
    Layer {
        src: Src::Crackle(900.0),
        filt: Filt::Band,
        cutoff: (5200.0, 3400.0),
        sweep: 0.3,
        q: 2.5,
        decay: 0.35,
        gain: 0.4,
        delay: 0.01,
        ..L
    },
    sine(2870.0, 2850.0, 0.5, 0.3, 0.07),
    sine(3910.0, 3880.0, 0.5, 0.22, 0.05),
    sine(5230.0, 5190.0, 0.5, 0.15, 0.04),
    // The light pouring out: a breath of air rising through the cracks.
    Layer {
        attack: 0.08,
        q: 3.0,
        ..noise(Filt::Band, 1800.0, 6500.0, 0.4, 0.45, 0.22)
    },
    // The bell, struck hard.
    Layer {
        delay: 0.03,
        ..ring(VIGIL_BELL[0], 1.5, 0.32)
    },
    Layer {
        delay: 0.03,
        ..ring(VIGIL_BELL[1], 1.3, 0.32)
    },
    Layer {
        delay: 0.03,
        ..ring(VIGIL_BELL[2], 1.0, 0.2)
    },
    Layer {
        delay: 0.03,
        ..ring(VIGIL_BELL[3], 0.8, 0.15)
    },
    Layer {
        delay: 0.03,
        ..ring(VIGIL_BELL[4], 0.6, 0.13)
    },
    Layer {
        delay: 0.03,
        ..ring(VIGIL_BELL[5], 0.35, 0.08)
    },
];

/// A husk's bell tolling while it runs on. The shell is cracked, so the bell
/// is too: its prime has a split twin a few Hz off, and the two beat.
pub const TOLL: &[Layer] = &[
    ring(VIGIL_BELL[0], 1.1, 0.24),
    ring(VIGIL_BELL[1], 0.9, 0.2),
    ring(VIGIL_BELL[1] + 3.5, 0.9, 0.13),
    ring(VIGIL_BELL[2], 0.7, 0.13),
    ring(VIGIL_BELL[3], 0.55, 0.1),
    ring(VIGIL_BELL[4], 0.4, 0.08),
    // The clapper, and a glint of crystal answering it.
    noise(Filt::High, 3000.0, 3000.0, 0.1, 0.008, 0.18),
    sine(2870.0, 2850.0, 0.5, 0.25, 0.03),
];

/// A husk going out: one last toll that chokes off, its partials sagging, as
/// the light drains away in a falling shimmer and the crystal falls in on
/// itself. No explosion: it doesn't burn, it goes dark.
pub const GUTTER: &[Layer] = &[
    // The choked toll.
    sine(VIGIL_BELL[0], VIGIL_BELL[0] * 0.75, 0.7, 0.8, 0.26),
    sine(VIGIL_BELL[1], VIGIL_BELL[1] * 0.75, 0.6, 0.55, 0.22),
    sine(VIGIL_BELL[2], VIGIL_BELL[2] * 0.75, 0.5, 0.35, 0.12),
    sine(VIGIL_BELL[3], VIGIL_BELL[3] * 0.75, 0.4, 0.25, 0.08),
    noise(Filt::High, 3000.0, 3000.0, 0.1, 0.008, 0.15),
    // The light draining: a shimmer falling from bright to nothing.
    Layer {
        attack: 0.05,
        ..sine(3400.0, 380.0, 0.45, 0.5, 0.07)
    },
    Layer {
        attack: 0.05,
        ..sine(4450.0, 500.0, 0.45, 0.45, 0.045)
    },
    Layer {
        attack: 0.05,
        q: 4.0,
        ..noise(Filt::Band, 6000.0, 900.0, 0.4, 0.5, 0.12)
    },
    // The crystal falling in, and the dead hull settling.
    Layer {
        src: Src::Crackle(500.0),
        filt: Filt::Band,
        cutoff: (4200.0, 1500.0),
        sweep: 0.4,
        q: 2.0,
        decay: 0.4,
        gain: 0.35,
        delay: 0.08,
        ..L
    },
    Layer {
        delay: 0.1,
        ..sine(70.0, 34.0, 0.12, 0.3, 0.5)
    },
    Layer {
        delay: 0.1,
        ..noise(Filt::Low, 900.0, 150.0, 0.2, 0.35, 0.4)
    },
];

/// A husk's crystal partials (Hz, inharmonic like struck glass), their
/// levels and how fast each swells and ebbs (Hz).
const HUSK_PARTIALS: [(f32, f32, f32); 4] = [
    (1870.0, 0.3, 0.7),
    (2350.0, 0.25, 1.1),
    (2960.0, 0.2, 1.6),
    (3720.0, 0.15, 2.3),
];

/// A husk: blue light pouring out of its cracks, heard as glass singing (a
/// few crystal partials, each swelling and ebbing), a breath of air, the
/// faint hum of its bell, and the tick of the cracks still spreading. It
/// sags flat as the light fades.
pub struct Husk {
    level: Smooth,
    tone: [f32; 4],
    swell: [f32; 4],
    hum: f32,
    air: Svf,
    tick: Svf,
}

impl Default for Husk {
    fn default() -> Husk {
        Husk {
            level: Smooth::new(0.0),
            tone: [0.0; 4],
            swell: [0.0, 0.25, 0.5, 0.75],
            hum: 0.0,
            air: Svf::default(),
            tick: Svf::default(),
        }
    }
}

impl Loop for Husk {
    fn set(&mut self, p: &TankParams) {
        self.level.target = p.husk;
    }

    fn next(&mut self, n: f32, noise: &mut Noise, sr: f32, k: &Coefs) -> f32 {
        let husk = self.level.next(k.slow);
        if husk <= 0.001 {
            return 0.0;
        }
        let sag = 0.9 + 0.1 * husk;
        let mut glass = 0.0;
        for (j, &(f, g, rate)) in HUSK_PARTIALS.iter().enumerate() {
            self.tone[j] = (self.tone[j] + f * sag / sr).fract_q();
            self.swell[j] = (self.swell[j] + rate / sr).fract_q();
            let swell = 0.5 + 0.5 * (self.swell[j] * TAU).sin_q();
            glass += (self.tone[j] * TAU).sin_q() * g * swell * swell;
        }
        let breath = 0.5 + 0.5 * (self.swell[0] * TAU * 0.5).cos_q();
        let pour = self.air.run(n, 3800.0 + 2400.0 * breath, 2.5, sr).band;
        self.hum = (self.hum + VIGIL_BELL[0] * sag / sr).fract_q();
        let hum = (self.hum * TAU).sin_q();
        let crack = if noise.unit() < 18.0 * husk / sr {
            0.6 + noise.unit()
        } else {
            0.0
        };
        let tick = self.tick.run(crack, 5600.0, 9.0, sr).band;
        (glass * 0.35 + pour * 0.4 + hum * 0.08 + tick * 0.5) * husk
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::mixer::Mixer;
    use crate::audio::testkit::{rms, sane, settle};

    #[test]
    fn every_sound_is_sane() {
        sane(&[("shatter", SHATTER), ("toll", TOLL), ("gutter", GUTTER)]);
    }

    /// A husk's light sings: bright and audible at full strength, fading
    /// with it.
    #[test]
    fn a_husk_sings_and_fades() {
        let level = |husk: f32| {
            let mut m = Mixer::new(48_000.0);
            m.set_master(1.0);
            m.set_world(1.0);
            m.set_tanks(&[TankParams {
                gain: 1.0,
                air: 16_000.0,
                husk,
                ..TankParams::default()
            }]);
            rms(&settle(&mut m))
        };
        // Over the idling engine, which runs on underneath.
        let idle = level(0.0);
        let (full, faint) = (level(1.0) - idle, level(0.2) - idle);
        assert!(full > 0.02, "full {full}, idle {idle}");
        assert!(
            faint < full * 0.4 && faint > 0.0,
            "full {full}, faint {faint}"
        );
    }
}
