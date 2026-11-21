//! The Uthenn beam: its buzz while it fires (a loop on the tank), the burn
//! where it lands (a loop of its own), and the moments it lights, lets go,
//! strikes a hull and locks out hot.

use std::f32::consts::TAU;

use glam::Vec3;

use super::Play;
use crate::audio::dsp::{Noise, OnePole, Panner, Quick, Smooth, Svf};
use crate::audio::mixer::{Coefs, OneShot};
use crate::audio::tank::{Loop, TankParams};
use crate::audio::voice::{Filt, L, Layer, Src, noise, ring, sine};
use crate::audio::{Listener, OWN_HULL, REF_DIST};
use crate::combat::{Burn, Match, muzzle};

/// How much louder a firing beam is at `d` m than the rest of its tank: it
/// falls off at 6 dB per doubling, half as fast as everything else, so a beam
/// is heard across a fight, not only up close.
pub fn carry(d: f32) -> f32 {
    // Never louder than close up: gain × carry stays under 1.
    (1.0 + (d / REF_DIST).powi(2)).sqrt()
}

/// A beam locked out hot.
pub fn overheat(tank: usize, at: Vec3, jitter: f32) -> Play {
    let g = if tank == 0 { 1.0 } else { 0.6 };
    Play::at(OVERHEAT, at, g, jitter)
}

/// An overheated beam cooled right down and ready again. Only your own: it
/// tells you when you can fire, and a bot's would just be noise.
pub fn ready(tank: usize, at: Vec3) -> Option<Play> {
    (tank == 0).then(|| Play::at(READY, at, OWN_HULL, 1.0))
}

/// Per tank, as of last frame: whether its beam was on, and what it burned.
#[derive(Default)]
pub struct Moments {
    beaming: Vec<bool>,
    burning: Vec<Burn>,
}

impl Moments {
    /// Beams that lit, let go, or struck a hull since last frame.
    pub fn update(&mut self, game: &Match, ears: &Listener, noise: &mut Noise) -> Vec<OneShot> {
        let n = game.world.tanks.len();
        self.beaming.resize(n, false);
        self.burning.resize(n, Burn::Air);
        let mut out = Vec::new();
        let mut play = |layers, at: Vec3, gain: f32, pitch: f32| {
            out.push(ears.place(at).shot(layers, gain, pitch));
        };
        for (i, (t, f)) in game.world.tanks.iter().zip(&game.fighters).enumerate() {
            let on = f.beam.is_some();
            let jitter = 0.94 + 0.12 * noise.unit();
            let gun = muzzle(t);
            if on && !self.beaming[i] {
                play(BEAM_ON, gun, 1.0, jitter);
            }
            // Locking out hot has its own hiss; a wreck goes quiet.
            if !on && self.beaming[i] && f.alive && !f.overheated {
                play(BEAM_OFF, gun, 0.9, jitter);
            }
            if let (Burn::Tank(j), Some(end)) = (f.burning, f.beam)
                && self.burning[i] != f.burning
            {
                let g = if j == 0 { OWN_HULL } else { 1.0 };
                play(SCORCH, end, g, jitter);
            }
            self.beaming[i] = on;
            self.burning[i] = f.burning;
        }
        out
    }
}

/// The beam lights: a zap that leaps up in pitch out of a crack of static
/// and a thump of the emitter charging.
pub const BEAM_ON: &[Layer] = &[
    sine(260.0, 1500.0, 0.04, 0.1, 0.3),
    sine(520.0, 3000.0, 0.04, 0.05, 0.1),
    noise(Filt::High, 2600.0, 2600.0, 0.1, 0.02, 0.6),
    Layer {
        src: Src::Crackle(900.0),
        filt: Filt::Band,
        cutoff: (3500.0, 2200.0),
        sweep: 0.08,
        q: 1.2,
        decay: 0.09,
        gain: 0.45,
        ..L
    },
    sine(95.0, 55.0, 0.05, 0.08, 0.4),
];

/// Trigger let go: the beam winds down, a falling zap and a fizz.
pub const BEAM_OFF: &[Layer] = &[
    sine(1300.0, 200.0, 0.06, 0.14, 0.22),
    Layer {
        q: 1.5,
        ..noise(Filt::Band, 3000.0, 700.0, 0.08, 0.1, 0.3)
    },
];

/// A beam strikes a hull: hit as hard as a shell (the blow, a crunch), then
/// the plate sizzles and pings as it takes the heat.
pub const SCORCH: &[Layer] = &[
    sine(120.0, 50.0, 0.03, 0.06, 0.5),
    noise(Filt::Low, 2200.0, 300.0, 0.04, 0.09, 1.0),
    Layer {
        src: Src::Crackle(1800.0),
        filt: Filt::Band,
        cutoff: (2400.0, 1200.0),
        sweep: 0.1,
        q: 0.9,
        decay: 0.12,
        gain: 0.35,
        ..L
    },
    ring(1470.0, 0.3, 0.12),
    ring(2290.0, 0.22, 0.08),
    ring(3720.0, 0.15, 0.05),
    Layer {
        attack: 0.015,
        ..noise(Filt::High, 3500.0, 3500.0, 0.1, 0.25, 0.25)
    },
];

/// The beam locks out: a steam hiss as the emitter powers down.
pub const OVERHEAT: &[Layer] = &[
    Layer {
        attack: 0.01,
        ..noise(Filt::High, 1500.0, 4500.0, 0.2, 0.5, 0.3)
    },
    sine(320.0, 110.0, 0.2, 0.25, 0.22),
];

/// Cooled and ready: the emitter spinning back up in a rising whine, then
/// two bright notes stepping up (a fifth), clean where the overheat hissed.
pub const READY: &[Layer] = &[
    Layer {
        attack: 0.08,
        ..sine(300.0, 900.0, 0.08, 0.06, 0.12)
    },
    Layer {
        delay: 0.1,
        ..sine(1046.0, 1046.0, 0.1, 0.09, 0.22)
    },
    Layer {
        delay: 0.1,
        ..sine(2092.0, 2092.0, 0.1, 0.05, 0.06)
    },
    Layer {
        delay: 0.19,
        ..sine(1568.0, 1568.0, 0.1, 0.18, 0.24)
    },
    Layer {
        delay: 0.19,
        ..sine(3136.0, 3136.0, 0.1, 0.09, 0.07)
    },
];

/// Laser beam: a harsh electric buzz pulsing a few times a second, fizzing
/// with arcs, climbing as it heats. Its weight is in the 0.6-4 kHz mids,
/// where laptop speakers play: a buzz that was mostly its 90 Hz fundamental
/// went unheard on them.
#[derive(Default)]
pub struct Buzz {
    level: Smooth,
    heat: Smooth,
    saw_a: f32,
    saw_b: f32,
    throb: f32,
    body: Svf,
    nasal: Svf,
    bite: Svf,
    arc: Svf,
    hiss: Svf,
}

impl Loop for Buzz {
    fn set(&mut self, p: &TankParams) {
        self.level.target = p.beam;
        self.heat.target = p.heat;
    }

    fn next(&mut self, n: f32, noise: &mut Noise, sr: f32, k: &Coefs) -> f32 {
        let beam = self.level.next(k.snap);
        let heat = self.heat.next(k.fast);
        if beam <= 0.001 {
            return 0.0;
        }
        let f = 120.0 + 80.0 * heat;
        self.saw_a = (self.saw_a + f / sr).fract_q();
        self.saw_b = (self.saw_b + f * 1.006 / sr).fract_q();
        let saws = self.saw_a + self.saw_b - 1.0;
        // Two formants make the buzz nasal and biting rather than a drone.
        let nasal = self.nasal.run(saws, 900.0 + 500.0 * heat, 2.5, sr).band;
        let bite = self.bite.run(saws, 2400.0 + 900.0 * heat, 3.0, sr).band;
        let body = self.body.run(saws, 350.0, 0.7, sr).low;
        self.throb = (self.throb + (6.0 + 6.0 * heat) / sr).fract_q();
        let throb = 0.75 + 0.25 * (self.throb * TAU).sin_q();
        // Arcing: snaps of static, more of them hot.
        let spark = if noise.unit() < (40.0 + 80.0 * heat) / sr {
            noise.next() * 3.0
        } else {
            0.0
        };
        let arc = self.arc.run(spark, 3600.0, 1.2, sr).band;
        let hiss = self.hiss.run(n, 5500.0, 0.9, sr).band;
        ((nasal * 0.55 + bite * 0.35) * throb + body * 0.25 + arc * 0.6 + hiss * 0.12) * beam * 0.9
    }
}

/// Where one tank's beam lands this frame, from the game thread.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BurnParams {
    /// Distance gain at the burn (0 when the beam touches nothing).
    pub gain: f32,
    pub pan: f32,
    pub air: f32,
    /// Burning into a hull (metal ringing) rather than stone and sand.
    pub metal: bool,
}

/// Where each tank's beam is burning: at its far end, louder on your own hull.
pub fn burns(game: &Match, ears: &Listener) -> Vec<BurnParams> {
    game.fighters
        .iter()
        .map(|f| match (f.beam, f.burning) {
            (Some(end), Burn::Surface | Burn::Tank(_)) => {
                let p = ears.place(end);
                let own = f.burning == Burn::Tank(0);
                BurnParams {
                    gain: p.gain * if own { OWN_HULL } else { 1.0 },
                    pan: p.pan,
                    air: p.air,
                    metal: matches!(f.burning, Burn::Tank(_)),
                }
            }
            _ => BurnParams::default(),
        })
        .collect()
}

/// The burn where a beam lands: a frying sizzle with pops of boiling
/// material, a low roar of vapour, and on a hull the metal ringing as it heats.
pub struct BurnVoice {
    gain: Smooth,
    pan: Smooth,
    air: Smooth,
    metal: Smooth,
    noise: Noise,
    flutter: OnePole,
    fry: Svf,
    pop: Svf,
    roar: Svf,
    ring_a: Svf,
    ring_b: Svf,
    ring_c: Svf,
    air_lp: OnePole,
    panner: Panner,
}

impl BurnVoice {
    pub fn new(seed: u32) -> BurnVoice {
        BurnVoice {
            gain: Smooth::new(0.0),
            pan: Smooth::new(0.0),
            air: Smooth::new(8000.0),
            metal: Smooth::new(0.0),
            noise: Noise::new(seed.wrapping_mul(0x85EB_CA6B).max(1)),
            flutter: OnePole::default(),
            fry: Svf::default(),
            pop: Svf::default(),
            roar: Svf::default(),
            ring_a: Svf::default(),
            ring_b: Svf::default(),
            ring_c: Svf::default(),
            air_lp: OnePole::default(),
            panner: Panner::default(),
        }
    }

    pub fn set(&mut self, p: &BurnParams) {
        self.gain.target = p.gain;
        // Where it lands jumps as the beam sweeps: keep the old place until
        // it has faded in at the new one.
        if self.gain.value < 1e-3 {
            self.pan.value = p.pan;
            self.air.value = p.air;
        }
        self.pan.target = p.pan;
        self.air.target = p.air;
        self.metal.target = p.metal as u8 as f32;
    }

    pub fn silent(&self) -> bool {
        self.gain.value < 1e-4 && self.gain.target < 1e-4
    }

    /// Add the burn into `out` (stereo frames).
    pub fn render(&mut self, out: &mut [f32], sr: f32, k: &Coefs) {
        for frame in out.chunks_exact_mut(2) {
            let gain = self.gain.next(k.snap);
            let pan = self.pan.next(k.fast);
            let air = self.air.next(k.fast);
            let metal = self.metal.next(k.fast);
            let n = self.noise.next();
            // Frying: bright noise whose level flutters like fat in a pan.
            let flutter = self.flutter.run(self.noise.next(), k.wobble * 4.0).abs() * 6.0;
            let fry = self.fry.run(n, 3800.0, 0.7, sr).high * (0.4 + flutter.min(1.4));
            // Pops of boiling stone or steel.
            let rate = 90.0 + 60.0 * metal;
            let pop = if self.noise.unit() < rate / sr {
                self.noise.next() * 5.0
            } else {
                0.0
            };
            let pops = self.pop.run(pop, 1600.0 + 900.0 * metal, 1.3, sr).band;
            let roar = self.roar.run(n, 420.0, 0.8, sr).low;
            // A hot hull rings: inharmonic plate partials, struck by the pops.
            let x = pop * 0.25 + n * 0.02;
            let ring = self.ring_a.run(x, 1470.0, 14.0, sr).band
                + self.ring_b.run(x, 2290.0, 16.0, sr).band * 0.7
                + self.ring_c.run(x, 3720.0, 18.0, sr).band * 0.4;
            let mix = fry * 0.28 + pops * 0.5 + roar * (0.9 - 0.4 * metal) + ring * 0.12 * metal;
            let x = self.air_lp.run_at(mix, air, sr) * gain;
            let (l, r) = self.panner.gains(pan);
            frame[0] += x * l;
            frame[1] += x * r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::mixer::Mixer;
    use crate::audio::testkit::{alone, driving, ears, mids, sane};
    use crate::audio::{Audio, WIND, wav};
    use crate::combat::Sound;
    use crate::profile::Settings;

    #[test]
    fn every_sound_is_sane() {
        sane(&[
            ("beam_on", BEAM_ON),
            ("beam_off", BEAM_OFF),
            ("scorch", SCORCH),
            ("overheat", OVERHEAT),
            ("ready", READY),
        ]);
    }

    /// By value: a `const` slice has no one address (debug builds copy it).
    fn has(shots: &[OneShot], l: &'static [Layer]) -> bool {
        shots.iter().any(|s| s.layers == l)
    }

    /// Issue #114: a beam is heard lighting, striking a hull (once, not
    /// every frame it burns) and letting go; locking out hot has its own hiss.
    #[test]
    fn a_beam_lights_strikes_and_lets_go() {
        let mut game = Match::new(crate::map::Map::saltmarket(), 0, 3, 7);
        let mut beams = Moments::default();
        let mut noise = Noise::new(1);
        let mut moments = |game: &Match| beams.update(game, &ears(), &mut noise);
        let end = game.world.tanks[1].pos;
        assert!(moments(&game).is_empty());

        game.fighters[0].beam = Some(end);
        game.fighters[0].burning = Burn::Surface;
        let lit = moments(&game);
        assert!(has(&lit, BEAM_ON) && !has(&lit, SCORCH));

        game.fighters[0].burning = Burn::Tank(1);
        let struck = moments(&game);
        assert!(has(&struck, SCORCH) && !has(&struck, BEAM_ON));
        assert!(moments(&game).is_empty(), "struck again");

        game.fighters[0].beam = None;
        game.fighters[0].burning = Burn::Air;
        assert!(has(&moments(&game), BEAM_OFF));

        game.fighters[0].beam = Some(end);
        moments(&game);
        game.fighters[0].beam = None;
        game.fighters[0].overheated = true;
        assert!(moments(&game).is_empty(), "locked out");
    }

    /// Your beam cooled and ready is heard, over an idling engine; a bot's
    /// says nothing.
    #[test]
    fn your_beam_ready_is_heard() {
        assert!(ready(3, Vec3::ZERO).is_none());
        let mut a = Audio::offline(24_000);
        let e = ears();
        let at = Vec3::new(0.0, 0.0, -10.0);
        let own = a.one_shot(&Sound::BeamReady { tank: 0, at }, &e).unwrap();
        let hit = a.one_shot(&Sound::Hit { at, victim: 1 }, &e).unwrap();
        assert!(
            own.gain > 0.8 * hit.gain,
            "ready {}, hit {}",
            own.gain,
            hit.gain
        );
        assert!(a.one_shot(&Sound::BeamReady { tank: 2, at }, &e).is_none());
    }

    /// Writes `docs/audio/beam-ready.wav`: your beam held until it locks
    /// out, the wait while it cools (shortened), the ready chime, and firing
    /// again: `cargo test record_beam_ready -- --ignored`.
    #[test]
    #[ignore]
    fn record_beam_ready() {
        const SR: u32 = 48_000;
        let e = ears();
        let v = Settings::default().volume();
        let mut m = Mixer::new(SR as f32);
        m.set_master(v * v);
        m.set_world(1.0);
        m.set_wind(WIND);
        let mut out = Vec::new();
        let mut run = |m: &mut Mixer, secs: f32| {
            let start = out.len();
            out.resize(start + 2 * (secs * SR as f32) as usize, 0.0);
            m.render(&mut out[start..]);
        };
        let own = Vec3::new(0.0, 0.0, -10.0);
        let p = e.place(own);
        let tank = |beam: f32, heat: f32| TankParams {
            gain: p.gain,
            pan: p.pan,
            air: p.air,
            rpm: 0.2,
            beam: beam * carry(own.length()),
            heat,
            heavy: true,
            ..TankParams::default()
        };
        let play = |m: &mut Mixer, s: Play| m.play(p.shot(s.layers, s.gain, s.pitch));
        m.set_tanks(&[tank(0.0, 0.0)]);
        run(&mut m, 0.5);
        play(&mut m, Play::at(BEAM_ON, own, 1.0, 1.0));
        for k in 0..40 {
            m.set_tanks(&[tank(1.0, k as f32 / 40.0)]);
            run(&mut m, 0.05);
        }
        m.set_tanks(&[tank(0.0, 1.0)]);
        play(&mut m, overheat(0, own, 1.0));
        run(&mut m, 2.5);
        play(&mut m, ready(0, own).unwrap());
        run(&mut m, 0.8);
        play(&mut m, Play::at(BEAM_ON, own, 1.0, 1.0));
        m.set_tanks(&[tank(1.0, 0.1)]);
        run(&mut m, 1.0);
        m.set_tanks(&[tank(0.0, 0.1)]);
        play(&mut m, Play::at(BEAM_OFF, own, 0.9, 1.0));
        run(&mut m, 0.8);

        let peak = out.iter().fold(0.0f32, |a, x| a.max(x.abs()));
        assert!(peak < 0.99, "peak {peak}");
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/audio/beam-ready.wav");
        std::fs::write(path, wav(&out, SR)).unwrap();
    }

    /// Being burned is as loud as being shot: your own hull, louder than
    /// someone else's, and the strike as loud as a shell's hit.
    #[test]
    fn a_beam_on_your_hull_is_loud() {
        let mut game = Match::new(crate::map::Map::saltmarket(), 0, 3, 7);
        let e = ears();
        let end = Vec3::new(0.0, 0.0, -10.0);
        game.fighters[1].beam = Some(end);
        game.fighters[1].burning = Burn::Tank(0);
        game.fighters[2].beam = Some(end);
        game.fighters[2].burning = Burn::Tank(3);
        game.fighters[3].beam = Some(end);
        game.fighters[3].burning = Burn::Air;
        let b = burns(&game, &e);
        assert!(b[1].metal && b[2].metal);
        assert!(b[1].gain > 1.5 * b[2].gain);
        assert_eq!(b[3].gain, 0.0, "a beam into the sky burns nothing");

        let shots = Moments::default().update(&game, &e, &mut Noise::new(1));
        let scorch = shots
            .iter()
            .filter(|s| s.layers == SCORCH)
            .map(|s| s.gain)
            .fold(0.0, f32::max);
        let hit = Audio::offline(24_000)
            .one_shot(&Sound::Hit { at: end, victim: 0 }, &e)
            .unwrap()
            .gain;
        assert!(scorch >= hit * 0.99, "scorch {scorch}, hit {hit}");
    }

    /// Writes `docs/audio/beam.wav`, the beam's sounds in turn (run with
    /// `cargo test write_beam_demo -- --ignored`): your beam into a wall, into
    /// a hull until it locks out hot, an enemy's burning you, and one across the fight.
    #[test]
    #[ignore]
    fn write_beam_demo() {
        const SR: u32 = 48_000;
        let e = ears();
        let mut m = Mixer::new(SR as f32);
        m.set_master(Settings::default().volume().powi(2));
        m.set_world(1.0);
        m.set_wind(WIND);
        let mut out = Vec::new();
        let mut run = |m: &mut Mixer, secs: f32| {
            let start = out.len();
            out.resize(start + (secs * SR as f32) as usize * 2, 0.0);
            for chunk in out[start..].chunks_mut(960) {
                m.render(chunk);
            }
        };
        let own = Vec3::new(0.0, 0.0, -10.0);
        let shot = |layers, at: Vec3, g: f32| e.place(at).shot(layers, g, 1.0);
        let tank = |at: Vec3, beam: f32, heat: f32| {
            let p = e.place(at);
            TankParams {
                gain: p.gain,
                pan: p.pan,
                air: p.air,
                rpm: 0.2,
                beam: beam * carry(at.length()),
                heat,
                heavy: true,
                ..TankParams::default()
            }
        };
        let burn = |at: Vec3, metal: bool, g: f32| {
            let p = e.place(at);
            BurnParams {
                gain: p.gain * g,
                pan: p.pan,
                air: p.air,
                metal,
            }
        };
        let idle = |m: &mut Mixer| {
            m.set_tanks(&[tank(own, 0.0, 0.0)]);
            m.set_burns(&[BurnParams::default()]);
        };
        // A beam from `from` burning `to`, held for `secs` while heating
        // from `h0` to `h1`. Your own tank is always the first.
        let hold = |m: &mut Mixer,
                    run: &mut dyn FnMut(&mut Mixer, f32),
                    (from, to, metal, g): (Vec3, Vec3, bool, f32),
                    (h0, h1): (f32, f32),
                    secs: f32| {
            let steps = (secs * 20.0) as usize;
            for k in 0..steps {
                let h = h0 + (h1 - h0) * k as f32 / steps as f32;
                let (beamer, at) = (tank(from, 1.0, h), burn(to, metal, g));
                if from == own {
                    m.set_tanks(&[beamer]);
                    m.set_burns(&[at]);
                } else {
                    m.set_tanks(&[tank(own, 0.0, 0.0), beamer]);
                    m.set_burns(&[BurnParams::default(), at]);
                }
                run(m, 0.05);
            }
        };

        idle(&mut m);
        run(&mut m, 0.6);
        // 1. Your beam into a wall 35 m off.
        let wall = Vec3::new(8.0, 0.0, -45.0);
        m.play(shot(BEAM_ON, own, 1.0));
        hold(&mut m, &mut run, (own, wall, false, 1.0), (0.0, 0.4), 2.5);
        idle(&mut m);
        m.play(shot(BEAM_OFF, own, 0.9));
        run(&mut m, 1.0);
        // 2. Your beam onto an enemy hull 30 m off, held until it locks out.
        let enemy = Vec3::new(-6.0, 0.0, -40.0);
        m.play(shot(BEAM_ON, own, 1.0));
        hold(&mut m, &mut run, (own, wall, false, 1.0), (0.0, 0.1), 0.4);
        m.play(shot(SCORCH, enemy, 1.0));
        hold(&mut m, &mut run, (own, enemy, true, 1.0), (0.1, 1.0), 3.0);
        idle(&mut m);
        m.play(shot(OVERHEAT, own, 1.0));
        run(&mut m, 1.4);
        // 3. An enemy 40 m off burns your hull.
        let foe = Vec3::new(25.0, 0.0, -40.0);
        m.play(shot(BEAM_ON, foe, 1.0));
        m.play(shot(SCORCH, own, OWN_HULL));
        hold(
            &mut m,
            &mut run,
            (foe, own, true, OWN_HULL),
            (0.0, 0.6),
            2.5,
        );
        idle(&mut m);
        m.play(shot(BEAM_OFF, foe, 0.9));
        run(&mut m, 1.2);
        // 4. A beam 70 m off, across the fight, into the ground.
        let far = Vec3::new(-45.0, 0.0, -55.0);
        m.play(shot(BEAM_ON, far, 1.0));
        let ground = far + Vec3::new(10.0, 0.0, -20.0);
        hold(&mut m, &mut run, (far, ground, false, 1.0), (0.0, 0.4), 2.5);
        idle(&mut m);
        m.play(shot(BEAM_OFF, far, 0.9));
        run(&mut m, 1.0);

        let peak = out.iter().fold(0.0f32, |a, x| a.max(x.abs()));
        assert!(peak < 0.99, "peak {peak}");
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/audio/beam.wav");
        std::fs::write(path, wav(&out, SR)).unwrap();
    }

    /// A beam across the fight is still heard: it fades half as fast (in
    /// dB) as a gun or an engine.
    #[test]
    fn a_beam_carries() {
        let e = ears();
        let db = |d: f32| {
            let g = e.place(Vec3::new(0.0, 0.0, -d)).gain;
            (20.0 * g.log10(), 20.0 * (g * carry(d)).log10())
        };
        let (gun10, beam10) = db(10.0);
        assert!((beam10 - gun10) < 3.0, "close up it's hardly louder");
        let ((gun40, beam40), (gun160, beam160)) = (db(40.0), db(160.0));
        let (fall_gun, fall_beam) = (gun40 - gun160, beam40 - beam160);
        assert!(
            fall_beam < 0.6 * fall_gun,
            "{fall_beam:.1} vs {fall_gun:.1} dB"
        );
    }

    /// Issue #114: the beam's buzz used to be mostly its 90 Hz fundamental,
    /// unheard on laptop speakers. It should cut through a tank driving flat
    /// out where they play, cold or hot.
    #[test]
    fn the_beam_cuts_through_on_small_speakers() {
        let engine = mids(&alone(
            TankParams {
                heavy: true,
                turret: 0.5,
                ..driving(1.0)
            },
            BurnParams::default(),
        ));
        for heat in [0.0, 1.0] {
            let beam = mids(&alone(
                TankParams {
                    gain: 1.0,
                    air: 16_000.0,
                    beam: 1.0,
                    heat,
                    ..TankParams::default()
                },
                BurnParams::default(),
            ));
            assert!(
                beam > 1.5 * engine,
                "heat {heat}: beam {beam}, engine {engine}"
            );
        }
    }

    /// Where the beam lands is heard, on stone and on a hull, and only while it burns.
    #[test]
    fn the_burn_is_heard() {
        let burn = |gain, metal| {
            mids(&alone(
                TankParams::default(),
                BurnParams {
                    gain,
                    pan: 0.0,
                    air: 16_000.0,
                    metal,
                },
            ))
        };
        assert!(burn(1.0, false) > 0.02, "stone {}", burn(1.0, false));
        assert!(burn(1.0, true) > 0.02, "hull {}", burn(1.0, true));
        assert!(burn(0.0, true) < 1e-6);
    }
}
