//! Each tank's continuous loops: what the game thread asks of them
//! ([`TankParams`]) and the voice that plays them all ([`TankVoice`]).
//! The loops themselves live with their families in `sounds/`. A new loop
//! is a [`Loop`] there, plus a line in [`TankParams`], in [`params`], and in
//! [`Loops`] and its `each`.

use crate::combat::{Match, SPAWN_SHIELD};
use crate::sim::HullKind;

use super::Listener;
use super::dsp::{Noise, OnePole, Panner, Smooth};
use super::mixer::Coefs;
use super::sounds::{beam, engine, guns, knocks, plasma, shield, tracks, turret, vigil};

/// What one tank sounds like this frame, from the game thread.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TankParams {
    /// Distance gain (0 for a wreck or out of earshot).
    pub gain: f32,
    /// -1 left .. 1 right of the listener.
    pub pan: f32,
    /// Air-absorption lowpass cutoff, Hz.
    pub air: f32,
    /// Engine speed, 0 idle .. 1 flat out.
    pub rpm: f32,
    /// Throttle / steering effort, 0..1.
    pub load: f32,
    /// Track belt speed, m/s.
    pub tread: f32,
    /// Turret traverse, 0..1 of full speed.
    pub turret: f32,
    /// Beam level (0 off; about 1 up close, more far off, where it carries)
    /// and its heat 0..1.
    pub beam: f32,
    pub heat: f32,
    /// Flamethrower pouring (0/1).
    pub flame: f32,
    /// Burning in a plasma river (0/1; more in your own tank).
    pub plasma: f32,
    /// A Knell ringing up: how far its charge has got, 0..1.
    pub charge: f32,
    /// A broken Vigil running on: how much light still pours out, 0..1.
    pub husk: f32,
    /// Spawn shield up: its loudness (0 when down), and how much of it is
    /// left, 1 fresh .. 0 about to drop.
    pub shield: f32,
    pub shield_left: f32,
    /// Falling: how hard the air rushes past, 0..1.
    pub fall: f32,
    /// Heavy hull: a deeper, slower engine.
    pub heavy: bool,
    /// Rides a plasma cushion: a field hum instead of an engine and tracks.
    pub hover: bool,
}

impl TankParams {
    /// Close up, flat out, with every loop going at once.
    #[cfg(test)]
    pub fn loudest() -> TankParams {
        TankParams {
            gain: 1.0,
            air: 16_000.0,
            rpm: 1.0,
            load: 1.0,
            tread: 10.0,
            turret: 1.0,
            beam: 1.0,
            heat: 1.0,
            flame: 1.0,
            plasma: plasma::level(0),
            charge: 1.0,
            husk: 1.0,
            shield: shield::level(0),
            shield_left: 1.0,
            fall: 1.0,
            heavy: true,
            ..TankParams::default()
        }
    }
}

/// Every tank's loops this frame, as heard by `ears`. `live` is false while
/// the world is frozen.
pub fn params(
    game: &Match,
    ears: &Listener,
    flames: &guns::Flames,
    time: f32,
    live: bool,
) -> Vec<TankParams> {
    game.world
        .tanks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let f = game.fighters.get(i);
            if !f.is_none_or(|f| f.alive) {
                return TankParams::default();
            }
            let p = ears.place(t.pos + t.up());
            let tread = (t.belt[0].abs() + t.belt[1].abs()) * 0.5;
            // A hover hull has no belts: its field works with ground speed.
            let pace = if t.chassis.hover { t.speed() } else { tread };
            let revs = (pace / t.hull.max_speed).min(1.0);
            let turret = (t.turret_rate.abs() / t.hull.turret_speed).min(1.0);
            let flaming = live && flames.pouring(i, time);
            TankParams {
                gain: p.gain,
                pan: p.pan,
                air: p.air,
                rpm: revs.max(t.drive * 0.35),
                load: t.drive,
                tread,
                turret,
                beam: if f.is_some_and(|f| f.beam.is_some()) {
                    beam::carry((t.pos - ears.pos).length())
                } else {
                    0.0
                },
                heat: f.map_or(0.0, |f| f.heat),
                flame: flaming as u8 as f32,
                plasma: f
                    .filter(|f| f.in_plasma.is_some())
                    .map_or(0.0, |_| plasma::level(i)),
                charge: t.charge,
                husk: f.filter(|f| f.is_husk()).map_or(0.0, |f| f.strength()),
                shield: f
                    .filter(|f| f.shield > 0.0)
                    .map_or(0.0, |_| shield::level(i)),
                shield_left: f.map_or(0.0, |f| (f.shield / SPAWN_SHIELD).clamp(0.0, 1.0)),
                fall: knocks::rush(t.grounded(), -t.vel.y),
                heavy: t.hull_kind == HullKind::Kastron,
                hover: t.chassis.hover,
            }
        })
        .collect()
}

/// One continuous sound a tank makes, fed from [`TankParams`] each frame.
pub trait Loop {
    /// Take this frame's targets.
    fn set(&mut self, p: &TankParams);
    /// The next sample. `n` is white noise the tank's loops share; draw more
    /// from `noise`.
    fn next(&mut self, n: f32, noise: &mut Noise, sr: f32, k: &Coefs) -> f32;
}

/// Every loop a tank has.
#[derive(Default)]
struct Loops {
    engine: engine::Engine,
    tracks: tracks::Tracks,
    traverse: turret::Traverse,
    buzz: beam::Buzz,
    flame: guns::Flame,
    plasma: plasma::Sizzle,
    hum: guns::Hum,
    husk: vigil::Husk,
    ward: shield::Ward,
    rush: knocks::Rush,
}

impl Loops {
    /// Each loop in turn, always in this order (they share one noise stream).
    fn each(&mut self, mut f: impl FnMut(&mut dyn Loop)) {
        f(&mut self.engine);
        f(&mut self.tracks);
        f(&mut self.traverse);
        f(&mut self.buzz);
        f(&mut self.flame);
        f(&mut self.plasma);
        f(&mut self.hum);
        f(&mut self.husk);
        f(&mut self.ward);
        f(&mut self.rush);
    }
}

/// One tank's loops, placed and mixed.
pub struct TankVoice {
    gain: Smooth,
    pan: Smooth,
    air: Smooth,
    noise: Noise,
    loops: Loops,
    air_lp: OnePole,
    panner: Panner,
}

impl TankVoice {
    pub fn new(seed: u32) -> TankVoice {
        TankVoice {
            gain: Smooth::new(0.0),
            pan: Smooth::new(0.0),
            air: Smooth::new(8000.0),
            noise: Noise::new(seed.wrapping_mul(2654435761).max(1)),
            loops: Loops::default(),
            air_lp: OnePole::default(),
            panner: Panner::default(),
        }
    }

    pub fn set(&mut self, p: &TankParams) {
        self.gain.target = p.gain;
        self.pan.target = p.pan;
        self.air.target = p.air;
        self.loops.each(|l| l.set(p));
    }

    pub fn silent(&self) -> bool {
        self.gain.value < 1e-4 && self.gain.target < 1e-4
    }

    /// Add this tank's loops into `out` (stereo frames).
    pub fn render(&mut self, out: &mut [f32], sr: f32, k: &Coefs) {
        for frame in out.chunks_exact_mut(2) {
            let gain = self.gain.next(k.slow);
            let pan = self.pan.next(k.slow);
            let air = self.air.next(k.slow);
            let n = self.noise.next();
            let noise = &mut self.noise;
            // -0.0 so the first loop's sample passes through exactly.
            let mut mix = -0.0;
            self.loops.each(|l| mix += l.next(n, noise, sr, k));
            let x = self.air_lp.run_at(mix, air, sr) * gain;
            let (l, r) = self.panner.gains(pan);
            frame[0] += x * l;
            frame[1] += x * r;
        }
    }
}
