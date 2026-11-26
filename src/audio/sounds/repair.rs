//! Repair crates: a canopy cracking open high overhead as one releases, a
//! wooden thump and a two-note call as it lands, and the chime of a hull
//! mended when it's picked up. All placed at the crate, so a drop across the
//! map is faint and one overhead is plain.

use glam::Vec3;

use super::Play;
use crate::audio::voice::{Filt, L, Layer, Src, noise, ring, sine};

/// A crate released: it's 40 m up, so it has to carry.
pub const DROP_GAIN: f32 = 4.0;
/// A crate landing.
pub const LANDED_GAIN: f32 = 1.3;

/// A repair crate released over its spot (`at` is the crate, high up).
pub fn drop(at: Vec3) -> Play {
    Play::at(DROP, at, DROP_GAIN, 1.0)
}

/// A repair crate touched down at `at`, ready to pick up.
pub fn landed(at: Vec3) -> Play {
    Play::at(LANDED, at, LANDED_GAIN, 1.0)
}

/// A hull mended at a repair crate.
pub fn repair(at: Vec3) -> Play {
    Play::at(REPAIR, at, 1.0, 1.0)
}

/// Hull mended at a repair crate.
pub const REPAIR: &[Layer] = &[
    sine(660.0, 660.0, 1.0, 0.25, 0.22),
    Layer {
        delay: 0.09,
        ..sine(990.0, 990.0, 1.0, 0.35, 0.22)
    },
    Layer {
        delay: 0.18,
        ..sine(1320.0, 1320.0, 1.0, 0.5, 0.18)
    },
    ring(140.0, 0.12, 0.3),
];

/// A crate released: the canopy cracks open with a snap and a whump of
/// caught air, then flaps and rustles as it fills, with a breath of wind
/// through the cords; under it, the repair chime's top two notes falling
/// softly — mending on its way down.
pub const DROP: &[Layer] = &[
    // Snap of cloth pulling taut.
    Layer {
        q: 1.2,
        ..noise(Filt::Band, 2400.0, 700.0, 0.04, 0.05, 0.7)
    },
    // Whump of the canopy filling.
    Layer {
        attack: 0.01,
        ..noise(Filt::Low, 700.0, 160.0, 0.08, 0.16, 0.6)
    },
    sine(95.0, 55.0, 0.1, 0.14, 0.3),
    // Flapping as it settles, quick then slower and softer.
    Layer {
        src: Src::Crackle(32.0),
        filt: Filt::Band,
        cutoff: (900.0, 500.0),
        sweep: 0.8,
        q: 1.6,
        attack: 0.1,
        decay: 0.7,
        gain: 0.55,
        delay: 0.06,
        ..L
    },
    Layer {
        src: Src::Crackle(14.0),
        filt: Filt::Band,
        cutoff: (420.0, 300.0),
        sweep: 1.0,
        q: 1.4,
        attack: 0.3,
        decay: 1.1,
        gain: 0.45,
        delay: 0.2,
        ..L
    },
    // Wind through the cords.
    Layer {
        attack: 0.35,
        q: 1.5,
        ..noise(Filt::Band, 1100.0, 650.0, 1.2, 0.9, 0.12)
    },
    // The call: repair's high notes, falling.
    Layer {
        delay: 0.3,
        attack: 0.01,
        ..sine(1320.0, 1320.0, 1.0, 0.3, 0.18)
    },
    Layer {
        delay: 0.48,
        attack: 0.01,
        ..sine(990.0, 990.0, 1.0, 0.45, 0.18)
    },
];

/// A crate landing: a heavy wooden thump and a knock of the box, a rattle of
/// what's packed inside, the canopy sighing down onto the ground, then the
/// repair chime's first two notes rising — here, come and get it.
pub const LANDED: &[Layer] = &[
    // Thump into the dirt.
    sine(110.0, 48.0, 0.06, 0.16, 0.6),
    Layer {
        attack: 0.003,
        ..noise(Filt::Low, 900.0, 180.0, 0.05, 0.12, 0.5)
    },
    // The box: hollow wood partials and a dry knock.
    sine(230.0, 225.0, 0.4, 0.07, 0.25),
    sine(370.0, 362.0, 0.4, 0.05, 0.18),
    Layer {
        q: 2.0,
        ..noise(Filt::Band, 1400.0, 900.0, 0.05, 0.04, 0.45)
    },
    // Contents settling.
    Layer {
        src: Src::Crackle(60.0),
        filt: Filt::Band,
        cutoff: (2600.0, 1800.0),
        sweep: 0.2,
        q: 2.0,
        delay: 0.04,
        decay: 0.12,
        gain: 0.35,
        ..L
    },
    // Canopy sagging down.
    Layer {
        delay: 0.12,
        attack: 0.25,
        ..noise(Filt::Low, 600.0, 250.0, 0.6, 0.4, 0.14)
    },
    // The call: repair's low notes, rising.
    Layer {
        delay: 0.35,
        attack: 0.01,
        ..sine(660.0, 660.0, 1.0, 0.22, 0.26)
    },
    Layer {
        delay: 0.5,
        attack: 0.01,
        ..sine(990.0, 990.0, 1.0, 0.4, 0.26)
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::testkit::{ears, sane};
    use crate::combat::{Match, Sound};

    #[test]
    fn every_sound_is_sane() {
        sane(&[("repair", REPAIR), ("drop", DROP), ("landed", LANDED)]);
    }

    /// Heard near, faint far, gone past earshot.
    #[test]
    fn a_drop_fades_with_distance() {
        let e = ears();
        let up = |d: f32| e.place(Vec3::new(0.0, 40.0, -d)).gain * DROP_GAIN;
        let down = |d: f32| e.place(Vec3::new(0.0, 0.0, -d)).gain * LANDED_GAIN;
        // Overhead or a short drive off, about as loud as a nearby repair.
        assert!(up(0.0) > 0.2 && up(30.0) > 0.1, "{} {}", up(0.0), up(30.0));
        assert!(down(15.0) > 0.4, "{}", down(15.0));
        // Across the map, faint; out of earshot, silent.
        assert!(up(200.0) < 0.05 && down(200.0) < 0.02);
        assert_eq!(down(500.0), 0.0);
    }

    /// In a real match every spot's first crate is heard releasing and then
    /// landing, in that order, about a fall's time apart.
    #[test]
    fn a_match_drop_is_heard_twice() {
        use crate::repair::{DROP_HEIGHT, FALL_SPEED, FIRST_DROP};
        let mut game = Match::new(crate::map::Map::reservoir(0), 0, 5, 7);
        game.start();
        let dt = crate::game::DT;
        let mut heard = Vec::new();
        let secs = FIRST_DROP + DROP_HEIGHT / FALL_SPEED + 1.0;
        for step in 0..(secs / dt) as u32 {
            game.step(crate::game::autopilot(game.world.time), true);
            for s in std::mem::take(&mut game.sounds) {
                match s {
                    Sound::RepairDrop { .. } => heard.push(("drop", step as f32 * dt)),
                    Sound::RepairLanded { .. } => heard.push(("landed", step as f32 * dt)),
                    _ => {}
                }
            }
        }
        assert!(heard.len() >= 2, "{heard:?}");
        assert_eq!(heard[0].0, "drop");
        assert!((heard[0].1 - FIRST_DROP).abs() < 0.1, "{heard:?}");
        let land = heard.iter().find(|h| h.0 == "landed").unwrap().1;
        let fall = DROP_HEIGHT / FALL_SPEED;
        assert!((land - heard[0].1 - fall).abs() < 0.1, "{heard:?}");
    }

    /// Writes `docs/audio/repair-drop.wav`, over your own idling engine: a
    /// crate released 40 m up and landing 15 m off, then the same 60 m off,
    /// then 150 m off. The fall (about 9 s) is cut to 2.5 s.
    /// `cargo test record_repair_drop -- --ignored`.
    #[test]
    #[ignore]
    fn record_repair_drop() {
        use crate::audio::mixer::Mixer;
        use crate::audio::tank::TankParams;
        use crate::audio::wav;
        const RATE: u32 = 48_000;
        let volume = crate::profile::Settings::default().volume();
        let mut m = Mixer::new(RATE as f32);
        m.set_master(volume * volume);
        m.set_world(1.0);
        m.set_tanks(&[TankParams {
            gain: 0.6,
            air: 16_000.0,
            ..TankParams::default()
        }]);
        let e = ears();
        let mut cues = Vec::new();
        for (k, d) in [15.0, 60.0, 150.0].into_iter().enumerate() {
            let t = 0.3 + k as f32 * 5.5;
            cues.push((t, drop(Vec3::new(d * 0.6, 40.0, -d * 0.8))));
            cues.push((t + 2.5, landed(Vec3::new(d * 0.6, 0.0, -d * 0.8))));
        }
        let frame = (RATE / 60) as usize * 2;
        let mut out = Vec::new();
        for i in 0..(17.0 * 60.0) as usize {
            let t = i as f32 / 60.0;
            for (at, p) in &cues {
                if (t - at).abs() < 0.5 / 60.0 {
                    let place = e.place(p.at.unwrap());
                    m.play(crate::audio::mixer::OneShot {
                        layers: p.layers,
                        pitch: p.pitch,
                        gain: place.gain * p.gain,
                        pan: place.pan,
                        air: place.air,
                    });
                }
            }
            let start = out.len();
            out.resize(start + frame, 0.0);
            m.render(&mut out[start..]);
        }
        std::fs::write("docs/audio/repair-drop.wav", wav(&out, RATE)).unwrap();
    }
}
