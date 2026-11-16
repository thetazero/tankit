//! Sound: every effect is synthesized at runtime, with no sample files.
//!
//! Each tank carries continuous loops driven by its physics: an engine whose
//! revs follow the belts and throttle, track clatter at the rate links meet
//! the sprockets, a turret ring motor, and the laser's hum or the flamer's
//! roar while they fire. Discrete moments (shots, impacts, armour hits, kills,
//! repairs) come out of the match as [`Sound`] events and play as one-shots.
//! Everything is placed relative to the camera: quieter and duller with
//! distance, panned left or right.
//!
//! The game thread sets targets once a frame; `mixer` eases toward them and
//! renders on the audio thread. With no audio device the game runs silent.

mod dsp;
mod mixer;
mod patch;

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use glam::Vec3;

use crate::combat::{Match, Sound};
use crate::render::Camera;
use crate::sim::{HullKind, TurretKind};

use dsp::Noise;
pub use mixer::{Mixer, OneShot, TankParams};

/// Distance at which a sound is at half its close-up loudness, m.
const REF_DIST: f32 = 8.0;
/// Past this nothing is heard, m.
const EARSHOT: f32 = 400.0;
/// How far a sound can swing to one side (1 would be hard-panned).
const PAN_WIDTH: f32 = 0.8;
/// Wind level under everything.
const WIND: f32 = 0.05;
/// How long a flamer keeps roaring after its last puff, s.
const FLAME_HOLD: f32 = 0.12;

/// UI sounds the app can trigger directly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ui {
    Move,
    Select,
    Back,
}

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
        let gain = REF_DIST / (REF_DIST + d) * (1.0 - d / EARSHOT).clamp(0.0, 1.0);
        let pan = if d > 0.5 {
            to.dot(self.right) / d * PAN_WIDTH
        } else {
            0.0
        };
        let air = (16_000.0 / (1.0 + d / 35.0)).max(900.0);
        Placed { gain, pan, air }
    }
}

pub struct Audio {
    mixer: Option<Arc<Mutex<Mixer>>>,
    _stream: Option<cpal::Stream>,
    noise: Noise,
    time: f32,
    /// Per tank: until when its flamer is pouring.
    flame_until: Vec<f32>,
}

impl Audio {
    /// Open the default output device; silent (but harmless) if there is none.
    pub fn new() -> Audio {
        let (mixer, stream) = match open() {
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
            flame_until: Vec::new(),
        }
    }

    /// No device: render by hand with [`Audio::render`] (the `--wav` recorder, tests).
    pub fn offline(sample_rate: u32) -> Audio {
        Audio {
            mixer: Some(Arc::new(Mutex::new(Mixer::new(sample_rate as f32)))),
            _stream: None,
            noise: Noise::new(0xA0D10),
            time: 0.0,
            flame_until: Vec::new(),
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
        let (layers, pitch) = match sound {
            Ui::Move => (patch::TICK, 1.0),
            Ui::Select => (patch::SELECT, 1.0),
            Ui::Back => (patch::SELECT, 0.7),
        };
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
        let n = game.world.tanks.len();
        self.flame_until.resize(n, 0.0);
        let mut shots = Vec::new();
        for s in sounds {
            if let Some(one) = self.one_shot(s, &ears) {
                shots.push(one);
            }
        }
        let params = self.tank_params(game, &ears, live);
        self.with(|m| {
            m.set_world(if live { 1.0 } else { 0.0 });
            m.set_wind(WIND);
            m.set_tanks(&params);
            for s in shots {
                m.play(s);
            }
        });
    }

    fn one_shot(&mut self, s: &Sound, ears: &Listener) -> Option<OneShot> {
        let jitter = 0.94 + 0.12 * self.noise.unit();
        let (layers, at, gain, pitch): (&'static [patch::Layer], Option<Vec3>, f32, f32) = match *s
        {
            Sound::Shot { tank, turret, at } => {
                let layers = match turret {
                    TurretKind::Kontos => patch::KONTOS,
                    TurretKind::Pelekys => patch::PELEKYS,
                    TurretKind::Ammeth => patch::WISP,
                    TurretKind::Ossrin => patch::TRIAD,
                    TurretKind::Hymn => {
                        if let Some(t) = self.flame_until.get_mut(tank) {
                            *t = self.time + FLAME_HOLD;
                        }
                        return None;
                    }
                    TurretKind::Lumen => return None,
                };
                (layers, Some(at), 1.0, jitter)
            }
            Sound::Impact { at, plasma } => {
                let layers = if plasma { patch::SPLASH } else { patch::IMPACT };
                (layers, Some(at), 0.8, jitter)
            }
            Sound::Bounce { at } => (patch::BOUNCE, Some(at), 0.8, jitter),
            // Your own hull ringing is louder: you're inside it.
            Sound::Hit { at, victim } => {
                let g = if victim == 0 { 1.6 } else { 1.0 };
                (patch::HIT, Some(at), g, jitter)
            }
            Sound::Kill { at, .. } => (patch::KILL, Some(at), 1.3, jitter),
            Sound::Repair { at, .. } => (patch::REPAIR, Some(at), 1.0, 1.0),
            Sound::Overheat { tank, at } => {
                let g = if tank == 0 { 1.0 } else { 0.6 };
                (patch::OVERHEAT, Some(at), g, jitter)
            }
            Sound::Confirm => (patch::CONFIRM, None, 1.0, 1.0),
            Sound::Link => (patch::LINK, None, 1.0, 1.0),
        };
        let placed = match at {
            Some(p) => ears.place(p),
            None => Placed {
                gain: 0.7,
                pan: 0.0,
                air: 16_000.0,
            },
        };
        Some(OneShot {
            layers,
            pitch,
            gain: placed.gain * gain,
            pan: placed.pan,
            air: placed.air,
        })
    }

    fn tank_params(&self, game: &Match, ears: &Listener, live: bool) -> Vec<TankParams> {
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
                let flaming = live && self.flame_until.get(i).is_some_and(|&u| u > self.time);
                TankParams {
                    gain: p.gain,
                    pan: p.pan,
                    air: p.air,
                    rpm: revs.max(t.drive * 0.35),
                    load: t.drive,
                    tread,
                    turret,
                    beam: f.is_some_and(|f| f.beam.is_some()) as u8 as f32,
                    heat: f.map_or(0.0, |f| f.heat),
                    flame: flaming as u8 as f32,
                    heavy: t.hull_kind == HullKind::Kastron,
                    hover: t.chassis.hover,
                }
            })
            .collect()
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

/// Start an output stream on the default device, rendering from a shared mixer.
fn open() -> anyhow::Result<(Arc<Mutex<Mixer>>, cpal::Stream)> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("no output device"))?;
    let supported = device.default_output_config()?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let channels = config.channels as usize;
    let mixer = Arc::new(Mutex::new(Mixer::new(config.sample_rate as f32)));
    let err = |e| log::warn!("audio stream: {e}");
    let stream = match format {
        cpal::SampleFormat::F32 => stream::<f32>(&device, config, mixer.clone(), channels, err)?,
        cpal::SampleFormat::I16 => stream::<i16>(&device, config, mixer.clone(), channels, err)?,
        cpal::SampleFormat::U16 => stream::<u16>(&device, config, mixer.clone(), channels, err)?,
        cpal::SampleFormat::I32 => stream::<i32>(&device, config, mixer.clone(), channels, err)?,
        other => anyhow::bail!("unsupported sample format {other:?}"),
    };
    stream.play()?;
    Ok((mixer, stream))
}

fn stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mixer: Arc<Mutex<Mixer>>,
    channels: usize,
    err: impl FnMut(cpal::Error) + Send + 'static,
) -> anyhow::Result<cpal::Stream>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    let mut stereo = Vec::new();
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let frames = data.len() / channels.max(1);
            stereo.resize(frames * 2, 0.0);
            match mixer.lock() {
                Ok(mut m) => m.render(&mut stereo),
                Err(_) => stereo.fill(0.0),
            }
            for (out, lr) in data.chunks_mut(channels).zip(stereo.chunks_exact(2)) {
                for (c, s) in out.iter_mut().enumerate() {
                    // Mono gets both sides; extra channels (surround) stay quiet.
                    let v = match (channels, c) {
                        (1, _) => (lr[0] + lr[1]) * 0.5,
                        (_, 0) => lr[0],
                        (_, 1) => lr[1],
                        _ => 0.0,
                    };
                    *s = T::from_sample(v);
                }
            }
        },
        err,
        None,
    )?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ears() -> Listener {
        Listener::of(&Camera {
            pos: Vec3::ZERO,
            target: Vec3::NEG_Z,
            fov_y: 1.0,
        })
    }

    #[test]
    fn farther_is_quieter_and_duller() {
        let e = ears();
        let near = e.place(Vec3::new(0.0, 0.0, -5.0));
        let far = e.place(Vec3::new(0.0, 0.0, -120.0));
        assert!(near.gain > 3.0 * far.gain);
        assert!(near.air > far.air);
        assert_eq!(e.place(Vec3::new(0.0, 0.0, -EARSHOT - 1.0)).gain, 0.0);
    }

    /// A real bot battle, recorded: loud enough to hear, never clipping hard.
    #[test]
    fn a_battle_sounds_sane() {
        let map = crate::make_map(7, crate::MapKind::City);
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
