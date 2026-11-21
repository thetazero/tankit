//! The spawn shield: a glassy ward humming round a fresh tank while it's up
//! (a loop), fluttering faster as it runs out, and a falling chime when it
//! drops. Your own shield is the loud one; a crowd of others spawning at
//! once is a faint shimmer.

use std::f32::consts::TAU;

use glam::Vec3;

use super::Play;
use crate::audio::OWN_HULL;
use crate::audio::dsp::{Noise, Quick, Smooth, Svf};
use crate::audio::mixer::Coefs;
use crate::audio::tank::{Loop, TankParams};
use crate::audio::voice::{Filt, Layer, noise, sine};

/// How loud a shield is on someone else's tank, against your own.
pub const OTHERS: f32 = 0.35;

/// Loop level for tank `tank` while its shield is up.
pub fn level(tank: usize) -> f32 {
    if tank == 0 { OWN_HULL } else { OTHERS }
}

/// Tank `tank`'s spawn shield just dropped: it can be hurt now.
pub fn down(tank: usize, at: Vec3) -> Play {
    Play::at(DOWN, at, 1.1 * level(tank), 1.0)
}

/// The ward's chord, Hz: a fifth with a twin a few Hz off the root, so the
/// two beat slowly.
const WARD: [(f32, f32); 4] = [(392.0, 0.3), (394.6, 0.22), (587.0, 0.16), (784.0, 0.1)];

/// A spawn shield up: a soft glassy chord that beats and shimmers, with a
/// breath of air through it, pulsing slowly at first and fluttering ever
/// faster as the protection runs out.
pub struct Ward {
    level: Smooth,
    left: f32,
    tone: [f32; 4],
    pulse: f32,
    air: Svf,
}

impl Default for Ward {
    fn default() -> Ward {
        Ward {
            level: Smooth::new(0.0),
            left: 0.0,
            tone: [0.0, 0.25, 0.5, 0.75],
            pulse: 0.0,
            air: Svf::default(),
        }
    }
}

impl Loop for Ward {
    fn set(&mut self, p: &TankParams) {
        self.level.target = p.shield;
        self.left = p.shield_left;
    }

    fn next(&mut self, n: f32, _noise: &mut Noise, sr: f32, k: &Coefs) -> f32 {
        let level = self.level.next(k.slow);
        if level <= 0.001 {
            return 0.0;
        }
        let mut chord = 0.0;
        for (j, &(f, g)) in WARD.iter().enumerate() {
            self.tone[j] = (self.tone[j] + f / sr).fract_q();
            chord += (self.tone[j] * TAU).sin_q() * g;
        }
        // 3 Hz with the shield fresh, 11 Hz as it's about to go.
        let rate = 3.0 + 8.0 * (1.0 - self.left).clamp(0.0, 1.0);
        self.pulse = (self.pulse + rate / sr).fract_q();
        let pulse = 0.65 + 0.35 * (self.pulse * TAU).sin_q();
        let shimmer = self.air.run(n, 3600.0, 3.0, sr).band;
        (chord * 0.3 + shimmer * 0.25) * pulse * level
    }
}

/// A shield dropping: the ward's chord falling away in a glassy sweep, a
/// fizz as the field lets go, and a soft thump — the hull is bare.
pub const DOWN: &[Layer] = &[
    sine(784.0, 196.0, 0.12, 0.3, 0.2),
    sine(1176.0, 294.0, 0.12, 0.25, 0.12),
    sine(1568.0, 392.0, 0.1, 0.18, 0.08),
    Layer {
        q: 2.0,
        ..noise(Filt::Band, 5200.0, 700.0, 0.12, 0.18, 0.25)
    },
    Layer {
        delay: 0.05,
        ..sine(140.0, 60.0, 0.08, 0.12, 0.3)
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::mixer::Mixer;
    use crate::audio::testkit::{mids, sane, settle};
    use crate::audio::wav;
    use crate::combat::{Match, SPAWN_SHIELD, Sound};

    #[test]
    fn every_sound_is_sane() {
        sane(&[("shield_down", DOWN)]);
    }

    /// A tank alone, its shield at `shield` with `left` of it remaining.
    fn ward(shield: f32, left: f32) -> Vec<f32> {
        let mut m = Mixer::new(48_000.0);
        m.set_master(1.0);
        m.set_world(1.0);
        m.set_tanks(&[TankParams {
            gain: 1.0,
            air: 16_000.0,
            shield,
            shield_left: left,
            ..TankParams::default()
        }]);
        settle(&mut m)
    }

    /// You can hear a shield over the idling engine, your own most of all.
    #[test]
    fn a_shield_is_heard() {
        let idle = mids(&ward(0.0, 0.0));
        let own = mids(&ward(level(0), 1.0));
        let other = mids(&ward(level(3), 1.0));
        assert!(own > 3.0 * idle && own > 0.02, "own {own}, idle {idle}");
        assert!(
            other < own * 0.5 && other > idle,
            "other {other}, own {own}"
        );
    }

    /// Every shield in a real match drops exactly once, when it runs out, and
    /// only while the tank is alive.
    #[test]
    fn each_shield_drops_once() {
        let mut game = Match::new(crate::map::Map::saltmarket(), 0, 5, 7);
        let n = game.fighters.len();
        let mut drops = vec![0; n];
        let mut first = None;
        for step in 0..((SPAWN_SHIELD + 0.5) / crate::game::DT) as u32 {
            game.step(crate::game::autopilot(game.world.time), true);
            for s in std::mem::take(&mut game.sounds) {
                if let Sound::ShieldDown { tank, .. } = s {
                    drops[tank] += 1;
                    first.get_or_insert(step as f32 * crate::game::DT);
                }
            }
        }
        assert!(drops.iter().all(|&d| d == 1), "{drops:?}");
        let t = first.unwrap();
        assert!((t - SPAWN_SHIELD).abs() < 0.1, "dropped at {t}");
    }

    /// Writes `docs/audio/shield.wav` (your shield up for its two seconds,
    /// then dropping, alone) and `docs/audio/shield-match.wav` (the start of
    /// a real match from the chase camera, everyone's shields up and down):
    /// `cargo test record_shield -- --ignored`.
    #[test]
    #[ignore]
    fn record_shield() {
        const RATE: u32 = 48_000;
        let volume = crate::profile::Settings::default().volume();
        let mut m = Mixer::new(RATE as f32);
        m.set_master(volume * volume);
        m.set_world(1.0);
        let frame = (RATE / 60) as usize * 2;
        let mut out = Vec::new();
        let frames = (3.2 * 60.0) as usize;
        for i in 0..frames {
            let t = i as f32 / 60.0;
            let up = t < SPAWN_SHIELD;
            m.set_tanks(&[TankParams {
                gain: 0.6,
                air: 16_000.0,
                shield: if up { level(0) } else { 0.0 },
                shield_left: 1.0 - t / SPAWN_SHIELD,
                ..TankParams::default()
            }]);
            if (t - SPAWN_SHIELD).abs() < 0.5 / 60.0 {
                let p = down(0, Vec3::ZERO);
                m.play(crate::audio::mixer::OneShot {
                    layers: p.layers,
                    pitch: 1.0,
                    gain: 0.6 * p.gain,
                    pan: 0.0,
                    air: 16_000.0,
                });
            }
            let start = out.len();
            out.resize(start + frame, 0.0);
            m.render(&mut out[start..]);
        }
        std::fs::write("docs/audio/shield.wav", wav(&out, RATE)).unwrap();

        let mut game = Match::new(crate::map::Map::saltmarket(), 0, 5, 7);
        let buf = crate::audio::record(&mut game, 3.5, RATE, volume, |w| {
            crate::game::autopilot(w.time)
        });
        std::fs::write("docs/audio/shield-match.wav", wav(&buf, RATE)).unwrap();
    }
}
