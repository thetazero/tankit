//! The audio thread's side: every tank's loops, the burn where each beam
//! lands, the wind, the places on the map, and one-shot voices, mixed to
//! stereo. The places duck under the one-shots, so a fight comes through.
//! The game thread only sets targets, through an [`Inbox`]; everything
//! eases toward them here.
//! What each loop sounds like lives with its family in `sounds/`.

use std::sync::Mutex;

use super::dsp::{FlushDenormals, OnePole, Smooth, pan_gains};
use super::sounds::beam::{BurnParams, BurnVoice};
use super::sounds::garage::Turntable;
use super::sounds::places::{PlaceParams, PlaceVoice};
use super::sounds::wind::Wind;
use super::tank::{TankParams, TankVoice};
use super::voice::{Layer, Voice};

/// Mix level into the soft clipper: a busy battle peaks just under full scale.
const MASTER: f32 = 1.3;
/// Most one-shots at once; past this the quietest is dropped.
const MAX_VOICES: usize = 48;
/// How hard the one-shots duck the places: at a one-shot level of `x` the
/// places play at `1 / (1 + DUCK * x)`.
const DUCK: f32 = 12.0;

/// One-shots waiting in an [`Inbox`] past this are dropped (the audio
/// thread has stopped coming for them: the device stalled).
const MAX_WAITING: usize = 4 * MAX_VOICES;

/// What the game thread has asked of the mixer since the audio thread last
/// looked. The game thread never touches the [`Mixer`] itself: rendering a
/// buffer can take milliseconds, and the game would wait on all of it. It
/// writes here instead, and the audio thread collects everything before each
/// buffer, holding the lock only to swap it out. Targets are overwritten
/// (only the latest counts); one-shots queue up.
#[derive(Default)]
pub struct Inbox {
    master: Option<f32>,
    world: Option<f32>,
    wind: Option<f32>,
    turntable: Option<f32>,
    tanks: Option<Vec<TankParams>>,
    burns: Option<Vec<BurnParams>>,
    places: Option<Vec<PlaceParams>>,
    shots: Vec<OneShot>,
}

impl Inbox {
    pub fn set_master(&mut self, gain: f32) {
        self.master = Some(gain);
    }

    pub fn set_world(&mut self, gain: f32) {
        self.world = Some(gain);
    }

    pub fn set_wind(&mut self, gain: f32) {
        self.wind = Some(gain);
    }

    pub fn set_turntable(&mut self, gain: f32) {
        self.turntable = Some(gain);
    }

    pub fn set_tanks(&mut self, params: Vec<TankParams>) {
        self.tanks = Some(params);
    }

    pub fn set_burns(&mut self, params: Vec<BurnParams>) {
        self.burns = Some(params);
    }

    pub fn set_places(&mut self, params: Vec<PlaceParams>) {
        self.places = Some(params);
    }

    pub fn play(&mut self, s: OneShot) {
        if self.shots.len() < MAX_WAITING {
            self.shots.push(s);
        }
    }
}

/// A one-shot to start.
pub struct OneShot {
    pub layers: &'static [Layer],
    pub pitch: f32,
    pub gain: f32,
    pub pan: f32,
    pub air: f32,
}

/// Smoothing coefficients for one render call.
pub struct Coefs {
    /// Spatial and load changes (frame-rate updates, no zipper noise).
    pub slow: f32,
    /// Engine revs (an engine takes a moment to spin up).
    pub rpm: f32,
    pub fast: f32,
    /// Trigger on/off.
    pub snap: f32,
    /// Flame flicker lowpass.
    pub wobble: f32,
}

pub struct Mixer {
    sr: f32,
    tanks: Vec<TankVoice>,
    burns: Vec<BurnVoice>,
    places: Vec<PlaceVoice>,
    voices: Vec<Voice>,
    master: Smooth,
    /// World sounds (loops and wind) fade out while paused.
    world: Smooth,
    wind: Wind,
    /// The garage bay's turntable, while the bay is shown.
    turntable: Turntable,
    seed: u32,
    /// The world loops' buffer, kept so the audio thread doesn't allocate.
    scratch: Vec<f32>,
    /// The places' buffer, likewise.
    scratch_places: Vec<f32>,
    /// How loud the one-shots are lately (follows fast, lets go slowly).
    sidechain: f32,
}

impl Mixer {
    pub fn new(sr: f32) -> Mixer {
        Mixer {
            sr,
            tanks: Vec::new(),
            burns: Vec::new(),
            places: Vec::new(),
            voices: Vec::new(),
            master: Smooth::new(0.0),
            world: Smooth::new(0.0),
            wind: Wind::new(),
            turntable: Turntable::new(),
            seed: 1,
            scratch: Vec::new(),
            scratch_places: Vec::new(),
            sidechain: 0.0,
        }
    }

    /// Take in what the game thread has asked for since last time. Never
    /// waits: if the game thread is writing right now, it's picked up before
    /// the next buffer instead.
    pub fn collect(&mut self, inbox: &Mutex<Inbox>) {
        let Ok(mut held) = inbox.try_lock() else {
            return;
        };
        let new = std::mem::take(&mut *held);
        drop(held);
        self.apply(new);
    }

    /// Do what an inbox asked, in the order the game thread used to.
    pub fn apply(&mut self, inbox: Inbox) {
        let Inbox {
            master,
            world,
            wind,
            turntable,
            tanks,
            burns,
            places,
            shots,
        } = inbox;
        if let Some(g) = master {
            self.set_master(g);
        }
        if let Some(g) = turntable {
            self.set_turntable(g);
        }
        if let Some(g) = world {
            self.set_world(g);
        }
        if let Some(g) = wind {
            self.set_wind(g);
        }
        if let Some(p) = tanks {
            self.set_tanks(&p);
        }
        if let Some(p) = burns {
            self.set_burns(&p);
        }
        if let Some(p) = places {
            self.set_places(&p);
        }
        for s in shots {
            self.play(s);
        }
    }

    pub fn set_master(&mut self, gain: f32) {
        self.master.target = gain;
    }

    pub fn set_world(&mut self, gain: f32) {
        self.world.target = gain;
    }

    pub fn set_wind(&mut self, gain: f32) {
        self.wind.set(gain);
    }

    /// The bay turntable's loudness: 0 off .. 1 (it eases in and out).
    pub fn set_turntable(&mut self, gain: f32) {
        self.turntable.set(gain);
    }

    pub fn set_tanks(&mut self, params: &[TankParams]) {
        while self.tanks.len() < params.len() {
            self.tanks.push(TankVoice::new(self.tanks.len() as u32 + 1));
        }
        self.tanks.truncate(params.len());
        for (v, p) in self.tanks.iter_mut().zip(params) {
            v.set(p);
        }
    }

    /// Where each tank's beam is burning, by tank.
    pub fn set_burns(&mut self, params: &[BurnParams]) {
        while self.burns.len() < params.len() {
            self.burns.push(BurnVoice::new(self.burns.len() as u32 + 1));
        }
        self.burns.truncate(params.len());
        for (v, p) in self.burns.iter_mut().zip(params) {
            v.set(p);
        }
    }

    /// Every place on the map, in the map's order (a new map, new voices).
    pub fn set_places(&mut self, params: &[PlaceParams]) {
        let same = self.places.len() == params.len()
            && self
                .places
                .iter()
                .zip(params)
                .all(|(v, p)| v.kind() == p.kind);
        if !same {
            self.places = params
                .iter()
                .enumerate()
                .map(|(i, p)| PlaceVoice::new(p.kind, i as u32 + 1))
                .collect();
        }
        for (v, p) in self.places.iter_mut().zip(params) {
            v.set(p);
        }
    }

    /// How far the places are ducked right now (1 = not at all).
    #[cfg(test)]
    pub fn duck(&self) -> f32 {
        1.0 / (1.0 + DUCK * self.sidechain)
    }

    pub fn play(&mut self, s: OneShot) {
        if s.gain < 1e-3 {
            return;
        }
        if self.voices.len() >= MAX_VOICES {
            let quietest = (0..self.voices.len())
                .min_by(|&a, &b| self.voices[a].level().total_cmp(&self.voices[b].level()))
                .unwrap();
            if self.voices[quietest].level() > s.gain {
                return;
            }
            self.voices.swap_remove(quietest);
        }
        self.seed = self.seed.wrapping_add(0x9E37_79B9);
        self.voices.push(Voice::new(
            s.layers,
            s.pitch,
            s.gain,
            pan_gains(s.pan),
            OnePole::coef(s.air, self.sr),
            self.seed,
            self.sr,
        ));
    }

    #[cfg(test)]
    pub fn voices(&self) -> usize {
        self.voices.len()
    }

    /// What's sounding: one-shots, and tank loops, beam burns and places
    /// that aren't silent.
    #[cfg(test)]
    pub fn busy(&self) -> [usize; 4] {
        [
            self.voices.len(),
            self.tanks.iter().filter(|t| !t.silent()).count(),
            self.burns.iter().filter(|b| !b.silent()).count(),
            self.places.iter().filter(|p| !p.silent()).count(),
        ]
    }

    /// Fill `out` with interleaved stereo frames.
    pub fn render(&mut self, out: &mut [f32]) {
        let _flush = FlushDenormals::new();
        out.fill(0.0);
        let sr = self.sr;
        let k = Coefs {
            slow: Smooth::coef(0.08, sr),
            rpm: Smooth::coef(0.25, sr),
            fast: Smooth::coef(0.04, sr),
            snap: Smooth::coef(0.012, sr),
            wobble: OnePole::coef(9.0, sr),
        };

        // World loops go into a scratch buffer so pause can fade them as one.
        let mut world = std::mem::take(&mut self.scratch);
        world.clear();
        world.resize(out.len(), 0.0);
        for t in &mut self.tanks {
            if !t.silent() {
                t.render(&mut world, sr, &k);
            }
        }
        for b in &mut self.burns {
            if !b.silent() {
                b.render(&mut world, sr, &k);
            }
        }
        self.wind.render(&mut world, sr, &k);
        self.turntable.render(&mut world, sr, &k);
        let mut places = std::mem::take(&mut self.scratch_places);
        places.clear();
        places.resize(out.len(), 0.0);
        for p in &mut self.places {
            if !p.silent() {
                p.render(&mut places, sr, &k);
            }
        }

        // The one-shots first, alone in `out`, so they can duck the places.
        for v in &mut self.voices {
            v.render(out);
        }
        self.voices.retain(|v| !v.done());

        let world_coef = Smooth::coef(0.15, sr);
        let (attack, release) = (Smooth::coef(0.01, sr), Smooth::coef(0.5, sr));
        for ((o, w), p) in out
            .chunks_exact_mut(2)
            .zip(world.chunks_exact(2))
            .zip(places.chunks_exact(2))
        {
            let g = self.world.next(world_coef);
            let level = o[0].abs().max(o[1].abs());
            let k = if level > self.sidechain {
                attack
            } else {
                release
            };
            self.sidechain += (level - self.sidechain) * k;
            let duck = g / (1.0 + DUCK * self.sidechain);
            o[0] += w[0] * g + p[0] * duck;
            o[1] += w[1] * g + p[1] * duck;
        }
        self.scratch = world;
        self.scratch_places = places;

        let master_coef = Smooth::coef(0.05, sr);
        for frame in out.chunks_exact_mut(2) {
            let g = self.master.next(master_coef);
            for s in frame {
                // Soft clip: loud moments squash instead of crackling.
                *s = (*s * g * MASTER).tanh();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::sounds::{hits, ui};
    use super::super::testkit::{driving, rms, settle};
    use super::*;

    /// The audio thread never waits on the game thread either: if the inbox
    /// is held, it renders with what it has and picks the news up next time.
    #[test]
    fn the_inbox_never_blocks() {
        let inbox = Mutex::new(Inbox::default());
        let mut m = Mixer::new(48_000.0);
        inbox.lock().unwrap().set_master(1.0);
        inbox.lock().unwrap().set_world(1.0);
        inbox.lock().unwrap().set_tanks(vec![driving(0.8)]);
        {
            let _game = inbox.lock().unwrap();
            m.collect(&inbox);
            assert!(rms(&settle(&mut m)) < 1e-6);
        }
        m.collect(&inbox);
        assert!(rms(&settle(&mut m)) > 1e-3);
        // Collected once: the targets stay, nothing is played twice.
        inbox.lock().unwrap().play(OneShot {
            layers: hits::KILL,
            pitch: 1.0,
            gain: 1.0,
            pan: 0.0,
            air: 16_000.0,
        });
        m.collect(&inbox);
        m.collect(&inbox);
        assert_eq!(m.voices(), 1);
    }

    #[test]
    fn silent_until_asked() {
        let mut m = Mixer::new(48_000.0);
        m.set_master(1.0);
        m.set_world(1.0);
        m.set_tanks(&[TankParams::default()]);
        assert!(rms(&settle(&mut m)) < 1e-6);
    }

    #[test]
    fn pan_puts_a_tank_in_one_ear() {
        let mut m = Mixer::new(48_000.0);
        m.set_master(1.0);
        m.set_world(1.0);
        m.set_tanks(&[TankParams {
            pan: -1.0,
            ..driving(0.6)
        }]);
        let buf = settle(&mut m);
        let l: Vec<f32> = buf.iter().step_by(2).copied().collect();
        let r: Vec<f32> = buf.iter().skip(1).step_by(2).copied().collect();
        assert!(rms(&l) > 10.0 * rms(&r));
    }

    #[test]
    fn pause_fades_loops_but_not_one_shots() {
        let mut m = Mixer::new(48_000.0);
        m.set_master(1.0);
        m.set_world(1.0);
        m.set_tanks(&[driving(0.8)]);
        let running = rms(&settle(&mut m));
        m.set_world(0.0);
        let paused = rms(&settle(&mut m));
        assert!(
            paused < running * 0.01,
            "running {running}, paused {paused}"
        );
        m.play(OneShot {
            layers: ui::SELECT,
            pitch: 1.0,
            gain: 1.0,
            pan: 0.0,
            air: 16_000.0,
        });
        let mut buf = vec![0.0; 2 * 2400];
        m.render(&mut buf);
        assert!(rms(&buf) > 1e-3);
    }

    #[test]
    fn every_loop_at_once_stays_bounded() {
        let mut m = Mixer::new(48_000.0);
        m.set_master(1.0);
        m.set_world(1.0);
        m.set_wind(0.1);
        m.set_tanks(&[TankParams::loudest(); 12]);
        m.set_burns(
            &[BurnParams {
                gain: 1.0,
                pan: 0.0,
                air: 16_000.0,
                metal: true,
            }; 12],
        );
        for _ in 0..60 {
            m.play(OneShot {
                layers: hits::KILL,
                pitch: 1.0,
                gain: 1.0,
                pan: 0.3,
                air: 16_000.0,
            });
        }
        assert_eq!(m.voices(), MAX_VOICES);
        let buf = settle(&mut m);
        assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
    }
}
