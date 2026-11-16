//! A tank's hull meeting the world: ramming a wall or another tank, coming
//! down hard after a drop, going over onto its side, and the air rushing
//! past as it falls. Every knock is scaled by how hard it was (the speed it
//! lost): a nudge is a dull thud, a full-speed ram a crash. Like a round
//! striking armour, these are low and dead, never a ring: a tank weighs
//! tens of tonnes.

use super::Play;
use crate::audio::OWN_HULL;
use crate::audio::dsp::{Noise, Smooth, Svf};
use crate::audio::mixer::Coefs;
use crate::audio::tank::{Loop, TankParams};
use crate::audio::voice::{Filt, L, Layer, Src, noise, sine};
use crate::sim::{Knock, KnockKind};

/// A knock this hard (speed lost, m/s) is full size: a ram at top speed, or
/// a drop of about five metres.
const FULL: f32 = 10.0;
/// Past this much of `FULL`, a wall or tank knock is a crash, not a bump.
const CRASH: f32 = 0.45;

/// How hard a knock was, 0 (just heard) .. 1 (full size) and a little past.
pub fn size(strength: f32) -> f32 {
    (strength / FULL).clamp(0.0, 1.4)
}

/// The one-shot for a knock: louder and deeper the harder it was. Your own
/// hull is the loud one: you're inside it.
pub fn knock(k: Knock, jitter: f32) -> Play {
    let s = size(k.strength);
    let hard = s > CRASH;
    let layers = match k.kind {
        KnockKind::Wall if hard => WALL_CRASH,
        KnockKind::Wall => WALL_BUMP,
        KnockKind::Tank if hard => RAM_CRASH,
        KnockKind::Tank => RAM_BUMP,
        KnockKind::Land => LAND,
        KnockKind::Tip => TIP,
    };
    let own = if k.tank == 0 { OWN_HULL } else { 1.0 };
    let gain = own * (0.2 + 0.8 * s.min(1.0)) * (1.0 + 0.3 * (s - 1.0).max(0.0));
    let pitch = jitter * (1.12 - 0.22 * s.min(1.0));
    Play::at(layers, k.at, gain, pitch)
}

/// A nudge into a wall: a dead thud, a scuff.
pub const WALL_BUMP: &[Layer] = &[
    sine(85.0, 42.0, 0.04, 0.09, 0.7),
    noise(Filt::Low, 1300.0, 220.0, 0.05, 0.1, 0.9),
    Layer {
        q: 0.7,
        ..noise(Filt::Band, 420.0, 260.0, 0.08, 0.13, 0.5)
    },
    Layer {
        src: Src::Crackle(900.0),
        filt: Filt::Band,
        cutoff: (1800.0, 900.0),
        q: 1.0,
        decay: 0.07,
        gain: 0.3,
        ..L
    },
];

/// Ramming a wall at speed: a heavy blow, armour crumpling against stone,
/// grit and rubble coming down after it.
pub const WALL_CRASH: &[Layer] = &[
    sine(72.0, 30.0, 0.08, 0.22, 1.0),
    noise(Filt::Low, 2600.0, 160.0, 0.08, 0.22, 1.2),
    Layer {
        q: 0.6,
        ..noise(Filt::Band, 600.0, 250.0, 0.12, 0.2, 0.7)
    },
    // Plate crumpling: a dense grinding burst.
    Layer {
        src: Src::Crackle(2200.0),
        filt: Filt::Band,
        cutoff: (1500.0, 650.0),
        sweep: 0.12,
        q: 0.8,
        decay: 0.14,
        gain: 0.6,
        ..L
    },
    // Rubble coming down off the wall.
    Layer {
        src: Src::Crackle(260.0),
        filt: Filt::Band,
        cutoff: (2600.0, 1400.0),
        sweep: 0.3,
        q: 1.2,
        decay: 0.4,
        gain: 0.35,
        delay: 0.04,
        ..L
    },
    // The weight of it settling.
    Layer {
        attack: 0.02,
        ..noise(Filt::Low, 300.0, 90.0, 0.3, 0.45, 0.45)
    },
];

/// Two hulls bumping: a hollow, dull bonk (steel on steel, but thick
/// enough that it doesn't ring) and a scrape.
pub const RAM_BUMP: &[Layer] = &[
    sine(150.0, 95.0, 0.05, 0.1, 0.6),
    sine(232.0, 160.0, 0.05, 0.07, 0.3),
    noise(Filt::Low, 1500.0, 300.0, 0.05, 0.09, 0.8),
    Layer {
        attack: 0.01,
        q: 2.0,
        ..noise(Filt::Band, 1900.0, 1300.0, 0.15, 0.14, 0.25)
    },
];

/// Ramming a tank at speed: the bonk of two hulls meeting hard, a long
/// screech of armour grinding past armour, and loose kit rattling.
pub const RAM_CRASH: &[Layer] = &[
    sine(130.0, 70.0, 0.06, 0.16, 0.9),
    sine(205.0, 130.0, 0.06, 0.1, 0.4),
    sine(60.0, 32.0, 0.1, 0.22, 0.8),
    noise(Filt::Low, 2400.0, 220.0, 0.07, 0.18, 1.1),
    // The plates grinding past each other.
    Layer {
        attack: 0.015,
        q: 2.5,
        sweep: 0.25,
        ..noise(Filt::Band, 2400.0, 1100.0, 0.25, 0.3, 0.35)
    },
    Layer {
        src: Src::Crackle(1800.0),
        filt: Filt::Band,
        cutoff: (1300.0, 700.0),
        sweep: 0.1,
        q: 0.9,
        decay: 0.12,
        gain: 0.5,
        ..L
    },
    // Kit on the hulls rattling.
    Layer {
        src: Src::Crackle(320.0),
        filt: Filt::Band,
        cutoff: (3200.0, 2000.0),
        q: 1.5,
        decay: 0.3,
        gain: 0.3,
        delay: 0.03,
        ..L
    },
];

/// Coming down hard: the suspension bottoms out (a deep thump and the clank
/// of the stops), the links slap the wheels, and it bounces once on its
/// springs.
pub const LAND: &[Layer] = &[
    sine(62.0, 30.0, 0.1, 0.26, 1.0),
    noise(Filt::Low, 900.0, 120.0, 0.08, 0.2, 1.0),
    // The bump stops.
    Layer {
        src: Src::Crackle(2600.0),
        filt: Filt::Band,
        cutoff: (900.0, 500.0),
        q: 0.9,
        decay: 0.05,
        gain: 0.55,
        ..L
    },
    // Track links slapping.
    Layer {
        src: Src::Crackle(420.0),
        filt: Filt::Band,
        cutoff: (2400.0, 1600.0),
        q: 1.3,
        decay: 0.25,
        gain: 0.3,
        delay: 0.03,
        ..L
    },
    // Dust kicked up.
    Layer {
        attack: 0.02,
        ..noise(Filt::Low, 1800.0, 400.0, 0.3, 0.35, 0.25)
    },
    // The rebound.
    Layer {
        delay: 0.2,
        ..sine(70.0, 40.0, 0.06, 0.12, 0.35)
    },
    Layer {
        delay: 0.2,
        ..noise(Filt::Low, 700.0, 160.0, 0.06, 0.1, 0.3)
    },
];

/// Going over: the hull groans as its weight leaves the tracks, then it
/// crashes down on its side and everything loose on it rattles.
pub const TIP: &[Layer] = &[
    // The groan.
    Layer {
        attack: 0.12,
        q: 3.0,
        sweep: 0.3,
        ..noise(Filt::Band, 520.0, 300.0, 0.3, 0.25, 0.45)
    },
    Layer {
        attack: 0.1,
        ..sine(118.0, 88.0, 0.3, 0.2, 0.15)
    },
    // The crash, as it lands.
    Layer {
        delay: 0.28,
        ..sine(70.0, 28.0, 0.1, 0.3, 1.0)
    },
    Layer {
        delay: 0.28,
        ..noise(Filt::Low, 2200.0, 140.0, 0.1, 0.25, 1.2)
    },
    Layer {
        src: Src::Crackle(2000.0),
        filt: Filt::Band,
        cutoff: (1400.0, 600.0),
        sweep: 0.12,
        q: 0.8,
        decay: 0.15,
        gain: 0.55,
        delay: 0.28,
        ..L
    },
    // Loose kit rattling on and on.
    Layer {
        src: Src::Crackle(240.0),
        filt: Filt::Band,
        cutoff: (2800.0, 1500.0),
        sweep: 0.5,
        q: 1.3,
        decay: 0.55,
        gain: 0.35,
        delay: 0.32,
        ..L
    },
];

/// Falling faster than this, m/s, the air starts to rush; at `RUSH_FULL`
/// it's at full roar.
const RUSH_FROM: f32 = 4.0;
const RUSH_FULL: f32 = 20.0;

/// The fall loop's level for a tank sinking at `sink` m/s, off the ground.
pub fn rush(grounded: bool, sink: f32) -> f32 {
    if grounded {
        return 0.0;
    }
    ((sink - RUSH_FROM) / (RUSH_FULL - RUSH_FROM)).clamp(0.0, 1.0)
}

/// Air rushing past a falling tank: a broad roar that rises in pitch and
/// level as it speeds up, with a whistle of wind through the running gear.
pub struct Rush {
    level: Smooth,
    roar: Svf,
    whistle: Svf,
}

impl Default for Rush {
    fn default() -> Rush {
        Rush {
            level: Smooth::new(0.0),
            roar: Svf::default(),
            whistle: Svf::default(),
        }
    }
}

impl Loop for Rush {
    fn set(&mut self, p: &TankParams) {
        self.level.target = p.fall;
    }

    fn next(&mut self, n: f32, noise: &mut Noise, sr: f32, k: &Coefs) -> f32 {
        let fall = self.level.next(k.slow);
        if fall <= 0.001 {
            return 0.0;
        }
        let roar = self.roar.run(n, 300.0 + 1400.0 * fall, 0.8, sr).band;
        let whistle = self
            .whistle
            .run(noise.next(), 1800.0 + 2600.0 * fall, 6.0, sr)
            .band;
        (roar * 2.0 + whistle * 0.5) * fall * fall.sqrt()
    }
}

/// A knock on tank `tank`, at the origin, for the tests and demo.
#[cfg(test)]
fn at(tank: usize, kind: KnockKind, strength: f32) -> Knock {
    Knock {
        tank,
        kind,
        at: glam::Vec3::ZERO,
        strength,
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;
    use crate::audio::mixer::{Mixer, OneShot};
    use crate::audio::testkit::{rms, sane, settle};
    use crate::audio::wav;
    use crate::combat::{Match, Sound};
    use crate::sim::World;

    #[test]
    fn every_sound_is_sane() {
        sane(&[
            ("wall bump", WALL_BUMP),
            ("wall crash", WALL_CRASH),
            ("ram bump", RAM_BUMP),
            ("ram crash", RAM_CRASH),
            ("land", LAND),
            ("tip", TIP),
        ]);
    }

    /// Harder is louder and deeper, a full-size knock a crash; your own
    /// hull is the loud one.
    #[test]
    fn harder_is_louder() {
        for kind in [
            KnockKind::Wall,
            KnockKind::Tank,
            KnockKind::Land,
            KnockKind::Tip,
        ] {
            let soft = knock(at(1, kind, 2.0), 1.0);
            let hard = knock(at(1, kind, 10.0), 1.0);
            let own = knock(at(0, kind, 10.0), 1.0);
            assert!(hard.gain > 2.5 * soft.gain, "{kind:?}");
            assert!(hard.pitch < soft.pitch, "{kind:?}");
            assert!(own.gain > hard.gain, "{kind:?}");
        }
        assert!(knock(at(1, KnockKind::Wall, 2.0), 1.0).layers == WALL_BUMP);
        assert!(knock(at(1, KnockKind::Wall, 9.0), 1.0).layers == WALL_CRASH);
    }

    fn rushing(fall: f32) -> f32 {
        let mut m = Mixer::new(48_000.0);
        m.set_master(1.0);
        m.set_world(1.0);
        m.set_tanks(&[TankParams {
            gain: 1.0,
            air: 16_000.0,
            fall,
            ..TankParams::default()
        }]);
        rms(&settle(&mut m))
    }

    /// A falling tank roars, louder the faster it falls; on the ground it's
    /// just the idling engine.
    #[test]
    fn a_fall_rushes() {
        let idle = rushing(0.0);
        let slow = rushing(rush(false, 8.0)) - idle;
        let fast = rushing(rush(false, 20.0)) - idle;
        assert_eq!(rush(true, 30.0), 0.0);
        assert_eq!(rush(false, 2.0), 0.0);
        assert!(fast > 0.05, "fast {fast}, idle {idle}");
        assert!(slow > 0.0 && slow < fast * 0.5, "slow {slow}, fast {fast}");
    }

    fn knocks(w: &mut World, cmds: &[crate::sim::Command], seconds: f32) -> Vec<Knock> {
        let mut out = Vec::new();
        for _ in 0..(seconds / crate::game::DT) as usize {
            w.step(cmds);
            out.extend(&w.knocks);
        }
        out
    }

    fn full(throttle: f32) -> crate::sim::Command {
        crate::sim::Command {
            throttle,
            ..Default::default()
        }
    }

    /// Driving flat out into the yard wall is one hard wall knock, and
    /// sitting still or cruising is none.
    #[test]
    fn a_wall_ram_knocks_once() {
        use crate::sim::tests_support::*;
        let mut w = flat_world();
        assert!(knocks(&mut w, &[full(0.0)], 1.0).is_empty());
        let ks = knocks(&mut w, &[full(1.0)], 25.0);
        assert_eq!(ks.len(), 1, "{ks:?}");
        assert_eq!(ks[0].kind, KnockKind::Wall);
        assert!(size(ks[0].strength) > CRASH, "{ks:?}");
    }

    /// Two tanks meeting head on both feel it, against a tank.
    #[test]
    fn a_ram_knocks_both() {
        use crate::sim::tests_support::*;
        use crate::sim::{HullKind, TurretKind};
        let mut w = flat_world();
        w.spawn_tank(
            Vec3::ONE,
            glam::Vec2::new(100.0, 130.0),
            std::f32::consts::PI,
            HullKind::Kataphrakt,
            TurretKind::Kontos,
        );
        let ks = knocks(&mut w, &[full(1.0), full(1.0)], 2.5);
        for tank in [0, 1] {
            assert!(
                ks.iter()
                    .any(|k| k.tank == tank && k.kind == KnockKind::Tank),
                "{ks:?}"
            );
        }
    }

    /// A drop lands once, harder the higher it was.
    #[test]
    fn higher_drops_land_harder() {
        use crate::sim::tests_support::*;
        let drop = |h: f32| {
            let mut w = flat_world();
            place(&mut w, Vec3::new(100.0, h, 100.0), glam::Quat::IDENTITY);
            let ks = knocks(&mut w, &[full(0.0)], 4.0);
            let lands: Vec<_> = ks.iter().filter(|k| k.kind == KnockKind::Land).collect();
            assert_eq!(lands.len(), 1, "{h} m: {ks:?}");
            lands[0].strength
        };
        let (low, high) = (drop(1.5), drop(8.0));
        assert!(low > 1.5 && high > 2.0 * low, "low {low}, high {high}");
    }

    /// Rolled hard, a tank goes over: one tip, however long it lies there.
    #[test]
    fn a_tank_tips_over() {
        use crate::sim::tests_support::*;
        let mut w = flat_world();
        place(
            &mut w,
            Vec3::new(100.0, 1.5, 100.0),
            glam::Quat::from_rotation_z(0.6),
        );
        let body = w.tanks[0].body;
        w.physics.bodies[body].set_angvel(Vec3::Z * 3.0, true);
        let ks = knocks(&mut w, &[full(0.0)], 4.0);
        assert_eq!(
            ks.iter().filter(|k| k.kind == KnockKind::Tip).count(),
            1,
            "{ks:?}"
        );
    }

    /// Bots driving round real maps don't clatter: knocks are moments, not
    /// a constant noise.
    #[test]
    fn battles_knock_now_and_then() {
        for map in [crate::map::Map::saltmarket(), crate::map::Map::kiln()] {
            let mut game = Match::new(map, 0, 5, 7);
            let mut n = 0;
            for _ in 0..(20.0 / crate::game::DT) as u32 {
                game.step(crate::game::autopilot(game.world.time), true);
                for s in std::mem::take(&mut game.sounds) {
                    n += matches!(s, Sound::Knock(_)) as usize;
                }
            }
            let per_tank_second = n as f32 / game.fighters.len() as f32 / 20.0;
            assert!(per_tank_second < 0.6, "{n} knocks");
        }
    }

    /// Writes `docs/audio/knocks.wav`: wall bumps and rams soft to hard,
    /// two tanks meeting soft and hard, landings from low to high, a tank
    /// going over, and a fall off a height with the rush of air and the
    /// landing at the bottom: `cargo test record_knocks -- --ignored`.
    #[test]
    #[ignore]
    fn record_knocks() {
        const RATE: u32 = 48_000;
        let volume = crate::profile::Settings::default().volume();
        let mut m = Mixer::new(RATE as f32);
        m.set_master(volume * volume);
        m.set_world(1.0);
        // (time, knock), and the fall's rush between two times.
        let mut cues = Vec::new();
        let mut t = 0.3;
        for &(kind, strengths) in &[
            (KnockKind::Wall, &[2.0, 5.0, 11.0][..]),
            (KnockKind::Tank, &[2.5, 11.0][..]),
            (KnockKind::Land, &[2.0, 5.0, 10.0][..]),
            (KnockKind::Tip, &[3.0][..]),
        ] {
            for &s in strengths {
                cues.push((t, at(0, kind, s)));
                t += 1.1;
            }
            t += 0.4;
        }
        let (fall_from, fall_to) = (t, t + 2.0);
        cues.push((fall_to, at(0, KnockKind::Land, 19.0)));
        let end = fall_to + 1.5;
        let frame = (RATE / 60) as usize * 2;
        let mut out = Vec::new();
        for i in 0..(end * 60.0) as usize {
            let now = i as f32 / 60.0;
            let falling = now >= fall_from && now < fall_to;
            let sink = crate::sim::GRAVITY * (now - fall_from);
            m.set_tanks(&[TankParams {
                gain: 0.6,
                air: 16_000.0,
                fall: if falling { rush(false, sink) } else { 0.0 },
                ..TankParams::default()
            }]);
            for (c, k) in &cues {
                if (now - c).abs() < 0.5 / 60.0 {
                    let p = knock(*k, 1.0);
                    m.play(OneShot {
                        layers: p.layers,
                        pitch: p.pitch,
                        gain: 0.6 * p.gain,
                        pan: 0.0,
                        air: 16_000.0,
                    });
                }
            }
            let start = out.len();
            out.resize(start + frame, 0.0);
            m.render(&mut out[start..]);
        }
        std::fs::write("docs/audio/knocks.wav", wav(&out, RATE)).unwrap();
    }
}
