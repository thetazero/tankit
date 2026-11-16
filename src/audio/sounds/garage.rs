//! The garage's depot bay: a part bought, a hull or turret swapped on by
//! the gantry crane, a purchase refused, and the turntable the tank stands
//! on, turning. Heard the same wherever the camera is: you're in the bay.

use std::f32::consts::TAU;

use crate::audio::dsp::{Noise, OnePole, Smooth, Svf};
use crate::audio::mixer::Coefs;
use crate::audio::voice::{Filt, Layer, Src, noise, ring, sine};

/// Something done in the garage that has its own sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cue {
    /// A part bought (with crystals) and delivered.
    Bought,
    /// Couldn't buy it: not enough crystals, or its maker won't sell yet.
    Refused,
    /// A different hull swapped in under the turret.
    Hull,
    /// A different turret lowered onto the hull.
    Turret,
}

pub fn cue(cue: Cue) -> &'static [Layer] {
    match cue {
        Cue::Bought => BOUGHT,
        Cue::Refused => REFUSED,
        Cue::Hull => HULL_SWAP,
        Cue::Turret => TURRET_SWAP,
    }
}

/// Bought: the console's two-note "approved" beep, crystals pouring into
/// the till (bright, glassy, inharmonic chimes, quick and falling), then
/// the part's crate set down on the deck with a dull clunk.
pub const BOUGHT: &[Layer] = &[
    // Console: approved, a rising fourth.
    Layer {
        attack: 0.004,
        ..sine(1047.0, 1047.0, 1.0, 0.05, 0.1)
    },
    Layer {
        delay: 0.07,
        attack: 0.004,
        ..sine(1397.0, 1397.0, 1.0, 0.08, 0.1)
    },
    // Crystals: glassy partials (1, 2.76, 5.4: a struck glass rod), in a
    // quick tumble down.
    Layer {
        delay: 0.16,
        ..ring(3520.0, 0.16, 0.07)
    },
    Layer {
        delay: 0.16,
        ..ring(9715.0, 0.06, 0.025)
    },
    Layer {
        delay: 0.21,
        ..ring(2960.0, 0.18, 0.07)
    },
    Layer {
        delay: 0.21,
        ..ring(8170.0, 0.06, 0.02)
    },
    Layer {
        delay: 0.25,
        ..ring(2637.0, 0.22, 0.07)
    },
    Layer {
        delay: 0.25,
        ..ring(7278.0, 0.07, 0.02)
    },
    Layer {
        delay: 0.3,
        ..ring(3136.0, 0.4, 0.08)
    },
    Layer {
        delay: 0.3,
        ..ring(8655.0, 0.1, 0.025)
    },
    // A trickle of grit as they settle.
    Layer {
        src: Src::Crackle(140.0),
        delay: 0.18,
        ..noise(Filt::High, 6000.0, 6000.0, 0.1, 0.12, 0.05)
    },
    // The crate set down: a low thud and a short hollow ring.
    Layer {
        delay: 0.62,
        ..noise(Filt::Low, 500.0, 140.0, 0.05, 0.08, 0.35)
    },
    Layer {
        delay: 0.62,
        ..sine(110.0, 82.0, 0.08, 0.12, 0.25)
    },
    Layer {
        delay: 0.62,
        ..ring(410.0, 0.08, 0.04)
    },
];

/// Refused: the console's low two-note "denied" buzz, falling. Nothing
/// moves on the deck.
pub const REFUSED: &[Layer] = &[
    Layer {
        attack: 0.005,
        ..sine(220.0, 220.0, 1.0, 0.09, 0.14)
    },
    Layer {
        attack: 0.005,
        ..sine(660.0, 660.0, 1.0, 0.07, 0.08)
    },
    Layer {
        delay: 0.13,
        attack: 0.005,
        ..sine(165.0, 160.0, 0.2, 0.14, 0.16)
    },
    Layer {
        delay: 0.13,
        attack: 0.005,
        ..sine(495.0, 480.0, 0.2, 0.1, 0.08)
    },
];

/// Hull swapped: the heavy one. The gantry's hydraulics hiss as the old
/// hull is lifted out, a chain rattles, and the new one comes down on the
/// deck with a deep thud and the clang of the turret ring clamping home.
pub const HULL_SWAP: &[Layer] = &[
    // Hydraulic hiss, opening up.
    Layer {
        attack: 0.12,
        ..noise(Filt::Band, 2000.0, 5000.0, 0.2, 0.18, 0.1)
    },
    // Hoist motor, a low groan winding up.
    Layer {
        attack: 0.1,
        ..sine(55.0, 85.0, 0.25, 0.25, 0.12)
    },
    // Chain links.
    Layer {
        src: Src::Crackle(45.0),
        delay: 0.08,
        attack: 0.05,
        ..noise(Filt::Band, 2600.0, 2600.0, 0.1, 0.22, 0.12)
    },
    // Set down: deep thud and a rumble through the deck.
    Layer {
        delay: 0.55,
        ..noise(Filt::Low, 600.0, 90.0, 0.06, 0.14, 0.5)
    },
    Layer {
        delay: 0.55,
        ..sine(72.0, 46.0, 0.12, 0.22, 0.45)
    },
    // The ring clamps: big struck steel, low and long.
    Layer {
        delay: 0.6,
        ..ring(196.0, 0.35, 0.1)
    },
    Layer {
        delay: 0.6,
        ..ring(523.0, 0.25, 0.1)
    },
    Layer {
        delay: 0.6,
        ..ring(1164.0, 0.12, 0.06)
    },
    // Hydraulics let go.
    Layer {
        delay: 0.7,
        attack: 0.02,
        ..noise(Filt::High, 3500.0, 1500.0, 0.3, 0.22, 0.06)
    },
];

/// Turret swapped: lighter and quicker. A servo whines as the crane lowers
/// it, the ring's ratchet clicks three times as it settles, and the locks
/// snap shut with a bright clank.
pub const TURRET_SWAP: &[Layer] = &[
    // Servo: a whine falling as it lowers.
    Layer {
        attack: 0.06,
        ..sine(880.0, 520.0, 0.2, 0.18, 0.08)
    },
    Layer {
        attack: 0.06,
        ..sine(1760.0, 1040.0, 0.2, 0.14, 0.02)
    },
    // Ratchet: three clicks, closing up.
    noise_click(0.24, 0.12),
    noise_click(0.32, 0.13),
    noise_click(0.38, 0.14),
    // Locked: a short bright clank, and a little weight on the hull.
    Layer {
        delay: 0.46,
        ..ring(740.0, 0.16, 0.12)
    },
    Layer {
        delay: 0.46,
        ..ring(1870.0, 0.1, 0.08)
    },
    Layer {
        delay: 0.46,
        ..ring(3310.0, 0.05, 0.03)
    },
    Layer {
        delay: 0.46,
        ..noise(Filt::Low, 900.0, 200.0, 0.04, 0.06, 0.25)
    },
];

/// A ratchet tooth: a short band of noise.
const fn noise_click(delay: f32, gain: f32) -> Layer {
    Layer {
        delay,
        q: 2.0,
        ..noise(Filt::Band, 3200.0, 2400.0, 0.02, 0.012, gain)
    }
}

/// How many rib-ends pass the roller per turn of the deck (see
/// `bay::deck_mesh`'s 16 ribs), and two rollers half a rib apart: a soft
/// tock every 1.2 s or so, alternating pitch.
const TOCKS_PER_TURN: f32 = 32.0;

/// The turntable turning under the tank: a low geared motor drone with a
/// slight gear whine, rumble from the bearing, and a soft tock as each rib
/// passes a roller. Runs while the bay is on screen.
pub struct Turntable {
    level: Smooth,
    /// Turns of the deck, 0..1 per tock.
    phase: f32,
    motor: f32,
    whine: f32,
    rumble: Svf,
    grit: Svf,
    whine_bend: OnePole,
    /// The current tock: its time since it struck (s) and which roller.
    tock: f32,
    high: bool,
    tock_f: f32,
    tock_phase: f32,
    noise: Noise,
}

impl Turntable {
    pub fn new() -> Turntable {
        Turntable {
            level: Smooth::new(0.0),
            phase: 0.0,
            motor: 0.0,
            whine: 0.0,
            rumble: Svf::default(),
            grit: Svf::default(),
            whine_bend: OnePole::default(),
            tock: 10.0,
            high: false,
            tock_f: 0.0,
            tock_phase: 0.0,
            noise: Noise::new(0x7AB1E),
        }
    }

    /// How loud: 0 (the bay's not shown) .. 1.
    pub fn set(&mut self, gain: f32) {
        self.level.target = gain;
    }

    /// Add the turntable into `out` (stereo frames).
    pub fn render(&mut self, out: &mut [f32], sr: f32, _k: &Coefs) {
        if self.level.value < 1e-4 && self.level.target < 1e-4 {
            self.level.value = 0.0;
            return;
        }
        // Slow in and out: the screen is black around the cut.
        let fade = Smooth::coef(0.35, sr);
        let rate = crate::garage::SPIN_RATE / TAU * TOCKS_PER_TURN;
        let bend = OnePole::coef(3.0, sr);
        for frame in out.chunks_exact_mut(2) {
            let g = self.level.next(fade);
            self.phase += rate / sr;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                self.tock = 0.0;
                self.high = !self.high;
                self.tock_f = if self.high { 150.0 } else { 118.0 };
            }
            // Motor: 50 Hz with a buzz of harmonics; whine wanders a little.
            self.motor = (self.motor + 50.0 / sr).fract();
            let m = self.motor * TAU;
            let drone = m.sin() * 0.5 + (2.0 * m).sin() * 0.35 + (3.0 * m).sin() * 0.12;
            let wander = self.whine_bend.run(self.noise.next(), bend);
            self.whine = (self.whine + (412.0 + 60.0 * wander) / sr).fract();
            let w = self.whine * TAU;
            let whine = w.sin() + 0.4 * (2.0 * w).sin();
            let rumble = self.rumble.run(self.noise.next(), 90.0, 0.9, sr).band;
            // Grit in the bearing: a thin band of noise that swells and sags.
            let grit = self.grit.run(self.noise.next(), 1300.0, 2.5, sr).band * (0.6 + wander);
            // Tock: a damped low knock with a tap of noise on top.
            let mut tock = 0.0;
            if self.tock < 0.4 {
                self.tock_phase = (self.tock_phase + self.tock_f / sr).fract();
                let env = (-self.tock / 0.07).exp();
                let tap = (-self.tock / 0.006).exp() * self.noise.next() * 0.3;
                let p = self.tock_phase * TAU;
                // Low knock, and a hollow wooden partial so small speakers hear it.
                let knock = p.sin() + 0.35 * (4.1 * p).sin() * (-self.tock / 0.03).exp();
                tock = (knock + tap) * env;
                self.tock += 1.0 / sr;
            } else {
                self.tock_phase = 0.0;
            }
            let x = g * (drone * 0.04 + whine * 0.012 + rumble * 0.2 + grit * 0.03 + tock * 0.12);
            frame[0] += x;
            frame[1] += x;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::mixer::{Mixer, OneShot};
    use crate::audio::testkit::{band, mids, rms, sane};
    use crate::audio::wav;

    #[test]
    fn every_sound_is_sane() {
        sane(&[
            ("bought", BOUGHT),
            ("refused", REFUSED),
            ("hull_swap", HULL_SWAP),
            ("turret_swap", TURRET_SWAP),
        ]);
    }

    const RATE: u32 = 48_000;

    /// `secs` of the bay: the turntable at `turning`, and each of `cues` at
    /// its time, as heard (the default volume, played as `Audio::garage` does).
    fn bay(turning: f32, cues: &[(f32, Cue)], secs: f32) -> Vec<f32> {
        let mut m = Mixer::new(RATE as f32);
        let v = crate::profile::Settings::default().volume();
        m.set_master(v * v);
        m.set_world(1.0);
        m.set_turntable(turning);
        let mut out = Vec::new();
        let block = RATE as usize / 100;
        let mut t = 0.0;
        let mut next = 0;
        while t < secs {
            while next < cues.len() && cues[next].0 <= t {
                m.play(OneShot {
                    layers: cue(cues[next].1),
                    pitch: 1.0,
                    gain: crate::audio::GARAGE_GAIN,
                    pan: 0.0,
                    air: 16_000.0,
                });
                next += 1;
            }
            let start = out.len();
            out.resize(start + block * 2, 0.0);
            m.render(&mut out[start..]);
            t += 0.01;
        }
        out
    }

    fn peak(buf: &[f32]) -> f32 {
        buf.iter().fold(0.0f32, |m, x| m.max(x.abs()))
    }

    const CUES: [(Cue, &str); 4] = [
        (Cue::Bought, "bought"),
        (Cue::Refused, "refused"),
        (Cue::Hull, "hull"),
        (Cue::Turret, "turret"),
    ];

    /// Each cue is clearly heard, under clipping, and they're all different.
    #[test]
    fn cues_are_heard_and_tell_apart() {
        for (c, name) in CUES {
            let p = peak(&bay(0.0, &[(0.0, c)], 1.5));
            assert!(p > 0.05 && p < 0.9, "{name} peaks at {p}");
        }
        for (i, (a, _)) in CUES.iter().enumerate() {
            for (b, _) in &CUES[i + 1..] {
                assert!(cue(*a) != cue(*b));
            }
        }
    }

    /// A hull is heavier than a turret: more of it is low.
    #[test]
    fn a_hull_lands_heavier_than_a_turret() {
        let sr = RATE as f32;
        let low = |c| {
            let b = bay(0.0, &[(0.0, c)], 1.5);
            let mono: Vec<f32> = b.chunks_exact(2).map(|f| f[0]).collect();
            band(&mono, 30.0, 150.0, sr) / band(&mono, 600.0, 4000.0, sr)
        };
        assert!(low(Cue::Hull) > 2.0 * low(Cue::Turret));
    }

    /// The turntable is a quiet bed under the cues, and silent when the bay's not shown.
    #[test]
    fn turntable_is_quiet_and_stops() {
        let on = bay(1.0, &[], 4.0);
        let tail = &on[on.len() / 2..];
        let level = rms(tail);
        assert!(level > 0.004, "turntable inaudible: {level}");
        let bought = rms(&bay(0.0, &[(0.0, Cue::Bought)], 1.0));
        assert!(
            level < bought,
            "turntable {level} not under a purchase {bought}"
        );
        assert!(peak(tail) < 0.5);
        assert!(mids(tail) > 0.002, "turntable only rumbles: {}", mids(tail));

        let mut m = Mixer::new(RATE as f32);
        m.set_master(1.0);
        m.set_world(1.0);
        m.set_turntable(1.0);
        let mut buf = vec![0.0; 2 * RATE as usize];
        m.render(&mut buf);
        m.set_turntable(0.0);
        for _ in 0..4 {
            m.render(&mut buf);
        }
        assert!(rms(&buf) < 1e-4, "still turning: {}", rms(&buf));
    }

    /// Writes `docs/audio/garage-*.wav`: `cargo test record_garage -- --ignored`.
    /// One per cue, the turntable alone, and a visit: into the bay, a turret
    /// swapped, a hull bought, a hull swapped, a purchase refused.
    #[test]
    #[ignore]
    fn record_garage() {
        for (c, name) in CUES {
            let path = format!("docs/audio/garage-{name}.wav");
            std::fs::write(&path, wav(&bay(0.0, &[(0.0, c)], 1.6), RATE)).unwrap();
        }
        std::fs::write(
            "docs/audio/garage-turntable.wav",
            wav(&bay(1.0, &[], 8.0), RATE),
        )
        .unwrap();
        let visit = bay(
            1.0,
            &[
                (1.5, Cue::Turret),
                (3.5, Cue::Bought),
                (5.5, Cue::Hull),
                (7.5, Cue::Refused),
            ],
            9.5,
        );
        std::fs::write("docs/audio/garage.wav", wav(&visit, RATE)).unwrap();
    }
}
