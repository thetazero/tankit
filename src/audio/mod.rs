//! Sound: every effect is synthesized at runtime, with no sample files.
//!
//! Each tank carries continuous loops driven by its physics: an engine whose
//! revs follow the belts and throttle, track clatter at the rate links meet
//! the sprockets, a turret ring motor, and the laser's buzz or the flamer's
//! roar while they fire; where a beam lands it sizzles, and a broken Vigil's
//! light sings out of its cracks, and a spawn shield hums while it's up. Discrete moments (shots, impacts, armour
//! hits, kills, repairs) come out of the match as [`Sound`] events and play
//! as one-shots; a beam lighting, letting go and striking a hull are spotted
//! here, frame to frame.
//! Everything is placed relative to the camera: quieter and duller with
//! distance, panned left or right.
//!
//! The game thread sets targets once a frame; `mixer` eases toward them and
//! renders on the audio thread. With no audio device the game runs silent.
//!
//! **Adding a sound?** Each sound lives with its family in `sounds/`; see
//! the note at the top of `sounds/mod.rs` for where each part goes.

mod device;
mod dsp;
mod mixer;
mod sounds;
mod tank;
#[cfg(test)]
mod testkit;
mod voice;

use std::sync::{Arc, Mutex};

use glam::Vec3;

use crate::combat::{Match, Sound};
use crate::render::Camera;

use dsp::Noise;
use mixer::{Mixer, OneShot};
pub use sounds::ui::Ui;
use sounds::{Play, arrival, beam, guns, hits, hold, repair, shield, ui, vigil};
use voice::Layer;

/// Distance at which a sound is at half its close-up loudness, m. Past it
/// loudness falls with the square of distance (12 dB per doubling): the chase
/// camera's own tank (about 10 m off) is about 5 dB down, a fight 100 m away about
/// 30 dB under it, and one across the map faint.
const REF_DIST: f32 = 12.0;
/// Past this nothing is heard, m.
const EARSHOT: f32 = 400.0;
/// How far a sound can swing to one side (1 would be hard-panned).
const PAN_WIDTH: f32 = 0.8;
/// Wind level under everything.
const WIND: f32 = 0.05;
/// A blow to your own hull, as against someone else's: you're inside it.
const OWN_HULL: f32 = 1.6;

/// Where the ears are.
#[derive(Clone, Copy)]
pub struct Listener {
    pub pos: Vec3,
    /// Unit vector to the listener's right.
    pub right: Vec3,
}

/// Loudness, pan and air-absorption cutoff of a source at some point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub gain: f32,
    pub pan: f32,
    pub air: f32,
}

impl Listener {
    pub fn of(cam: &Camera) -> Listener {
        let fwd = (cam.target - cam.pos).normalize_or(Vec3::NEG_Z);
        Listener {
            pos: cam.pos,
            right: fwd.cross(Vec3::Y).normalize_or(Vec3::X),
        }
    }

    pub fn place(&self, at: Vec3) -> Placed {
        let to = at - self.pos;
        let d = to.length();
        let fade = (1.0 - d / EARSHOT).clamp(0.0, 1.0);
        let gain = 1.0 / (1.0 + (d / REF_DIST).powi(2)) * fade;
        let pan = if d > 0.5 {
            to.dot(self.right) / d * PAN_WIDTH
        } else {
            0.0
        };
        let air = (16_000.0 / (1.0 + d / 20.0)).max(600.0);
        Placed { gain, pan, air }
    }
}

impl Placed {
    /// Heard the same wherever you are, like the HUD.
    const EVERYWHERE: Placed = Placed {
        gain: 0.7,
        pan: 0.0,
        air: 16_000.0,
    };

    /// `layers` played here, `gain` times as loud.
    fn shot(self, layers: &'static [Layer], gain: f32, pitch: f32) -> OneShot {
        OneShot {
            layers,
            pitch,
            gain: self.gain * gain,
            pan: self.pan,
            air: self.air,
        }
    }
}

pub struct Audio {
    mixer: Option<Arc<Mutex<Mixer>>>,
    _stream: Option<cpal::Stream>,
    noise: Noise,
    time: f32,
    flames: guns::Flames,
    beams: beam::Moments,
}

impl Audio {
    /// Open the default output device; silent (but harmless) if there is none.
    pub fn new() -> Audio {
        let (mixer, stream) = match device::open() {
            Ok((m, s)) => (Some(m), Some(s)),
            Err(e) => {
                log::warn!("no sound: {e:#}");
                (None, None)
            }
        };
        Audio {
            mixer,
            _stream: stream,
            noise: Noise::new(0xA0D10),
            time: 0.0,
            flames: Default::default(),
            beams: Default::default(),
        }
    }

    /// No device: render by hand with [`Audio::render`] (the `--wav` recorder, tests).
    pub fn offline(sample_rate: u32) -> Audio {
        Audio {
            mixer: Some(Arc::new(Mutex::new(Mixer::new(sample_rate as f32)))),
            _stream: None,
            noise: Noise::new(0xA0D10),
            time: 0.0,
            flames: Default::default(),
            beams: Default::default(),
        }
    }

    /// Fill `out` with interleaved stereo frames (offline only; a device pulls its own).
    pub fn render(&self, out: &mut [f32]) {
        self.with(|m| m.render(out));
    }

    fn with(&self, f: impl FnOnce(&mut Mixer)) {
        if let Some(Ok(mut m)) = self.mixer.as_ref().map(|m| m.lock()) {
            f(&mut m);
        }
    }

    /// Master volume, 0..1.
    pub fn set_volume(&self, v: f32) {
        // Perceived loudness is roughly logarithmic: square the slider.
        self.with(|m| m.set_master(v * v));
    }

    pub fn ui(&mut self, sound: Ui) {
        let (layers, pitch) = ui::menu(sound);
        self.with(|m| {
            m.play(OneShot {
                layers,
                pitch,
                gain: 0.8,
                pan: 0.0,
                air: 16_000.0,
            })
        });
    }

    /// Once a frame: play what happened in the match since last frame and set
    /// every tank's loops. `live` is false while the world is frozen (paused).
    pub fn update(&mut self, game: &Match, sounds: &[Sound], cam: &Camera, live: bool, dt: f32) {
        if self.mixer.is_none() {
            return;
        }
        if live {
            self.time += dt;
        }
        let ears = Listener::of(cam);
        self.flames.resize(game.world.tanks.len());
        let mut shots = Vec::new();
        for s in sounds {
            if let Some(one) = self.one_shot(s, &ears) {
                shots.push(one);
            }
        }
        if live {
            shots.extend(self.beams.update(game, &ears, &mut self.noise));
        }
        let params = tank::params(game, &ears, &self.flames, self.time, live);
        let burns = beam::burns(game, &ears);
        self.with(|m| {
            m.set_world(if live { 1.0 } else { 0.0 });
            m.set_wind(WIND);
            m.set_tanks(&params);
            m.set_burns(&burns);
            for s in shots {
                m.play(s);
            }
        });
    }

    /// The one-shot a match event plays, if any. One arm per event, grouped
    /// by the family in `sounds/` that voices it.
    fn one_shot(&mut self, s: &Sound, ears: &Listener) -> Option<OneShot> {
        let jitter = 0.94 + 0.12 * self.noise.unit();
        let play: Play = match *s {
            Sound::Arrival { tank, faction, at } => arrival::arrival(tank, faction, at, jitter),

            Sound::Overheat { tank, at } => beam::overheat(tank, at, jitter),

            Sound::Shot { tank, turret, at } => {
                self.flames.shot(tank, turret, at, self.time, jitter)?
            }

            Sound::Impact { at, plasma } => hits::impact(at, plasma, jitter),
            Sound::Bounce { at } => hits::bounce(at, jitter),
            Sound::Hit { at, victim } => hits::hit(at, victim, jitter),
            Sound::Kill { at } => hits::kill(at, jitter),

            Sound::Hold(cue) => hold::cue(cue),

            Sound::Repair { at } => repair::repair(at),

            Sound::ShieldDown { tank, at } => shield::down(tank, at),

            Sound::Confirm => ui::confirm(),
            Sound::Link => ui::link(),

            Sound::Shatter { at } => vigil::shatter(at),
            Sound::Toll { at, strength } => vigil::toll(at, strength),
            Sound::Gutter { at } => vigil::gutter(at),
        };
        let placed = play.at.map_or(Placed::EVERYWHERE, |p| ears.place(p));
        Some(placed.shot(play.layers, play.gain, play.pitch))
    }
}

/// Record `seconds` of `game` (driven by `drive`, trigger held) from the chase
/// camera, as interleaved stereo at `sample_rate`.
pub fn record(
    game: &mut Match,
    seconds: f32,
    sample_rate: u32,
    volume: f32,
    drive: impl Fn(&crate::sim::World) -> crate::sim::Command,
) -> Vec<f32> {
    use crate::game::DT;
    let mut audio = Audio::offline(sample_rate);
    audio.set_volume(volume);
    let mut cam = crate::scene::ChaseCam::new();
    let mut out = Vec::new();
    let mut owed = 0.0;
    for _ in 0..(seconds / DT).round() as u32 {
        game.step(drive(&game.world), true);
        let camera = cam.update(&game.world, &game.world.tanks[0], DT);
        let sounds = std::mem::take(&mut game.sounds);
        audio.update(game, &sounds, &camera, true, DT);
        owed += sample_rate as f32 * DT;
        let frames = owed as usize;
        owed -= frames as f32;
        let start = out.len();
        out.resize(start + frames * 2, 0.0);
        audio.render(&mut out[start..]);
    }
    out
}

/// 16-bit stereo PCM WAV bytes.
pub fn wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&sample_rate.to_le_bytes());
    b.extend_from_slice(&(sample_rate * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use testkit::ears;

    #[test]
    fn farther_is_quieter_and_duller() {
        let e = ears();
        let near = e.place(Vec3::new(0.0, 0.0, -5.0));
        let far = e.place(Vec3::new(0.0, 0.0, -120.0));
        assert!(near.gain > 3.0 * far.gain);
        assert!(near.air > far.air);
        assert_eq!(e.place(Vec3::new(0.0, 0.0, -EARSHOT - 1.0)).gain, 0.0);
    }

    /// Issue #77: distant fights were too loud. Your own tank sits about 10 m
    /// from the chase camera; a fight 100 m off should be ~30 dB under it and
    /// one across the map (200 m) fainter still.
    #[test]
    fn distant_fights_are_faint() {
        let e = ears();
        let db = |d: f32| 20.0 * e.place(Vec3::new(0.0, 0.0, -d)).gain.log10();
        let own = db(10.0);
        assert!(own > -6.0, "own tank {own:.1} dB");
        assert!(own - db(30.0) > 10.0);
        assert!(
            own - db(100.0) > 28.0,
            "100 m only {:.1} dB down",
            own - db(100.0)
        );
        assert!(
            own - db(200.0) > 40.0,
            "200 m only {:.1} dB down",
            own - db(200.0)
        );
    }

    /// A real bot battle, recorded: loud enough to hear, never clipping hard.
    #[test]
    fn a_battle_sounds_sane() {
        let map = crate::map::Map::saltmarket();
        let mut game = Match::new(map, 0, 5, 7);
        let buf = record(&mut game, 6.0, 24_000, 0.8, |w| {
            crate::game::autopilot(w.time)
        });
        assert_eq!(buf.len(), 2 * 6 * 24_000);
        assert!(buf.iter().all(|x| x.is_finite()));
        let rms = (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt();
        let hot = buf.iter().filter(|x| x.abs() > 0.95).count() as f32 / buf.len() as f32;
        assert!(rms > 0.01, "rms {rms}");
        assert!(hot < 0.01, "{:.2}% of samples near full scale", hot * 100.0);
    }

    #[test]
    fn wav_header_is_right() {
        let b = wav(&[0.0, 1.0, -1.0, 0.5], 48_000);
        assert_eq!(b.len(), 44 + 8);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(b[40..44].try_into().unwrap()), 8);
    }

    #[test]
    fn right_is_right() {
        let e = ears();
        assert!(e.place(Vec3::new(10.0, 0.0, 0.0)).pan > 0.7);
        assert!(e.place(Vec3::new(-10.0, 0.0, 0.0)).pan < -0.7);
        assert!(e.place(Vec3::new(0.0, 0.0, -10.0)).pan.abs() < 1e-5);
    }
}
