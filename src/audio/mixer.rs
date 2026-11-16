//! The audio thread's side: continuous per-tank loops (engine, tracks, turret
//! motor, beam, flame), the desert wind, and one-shot voices, mixed to stereo.
//! The game thread only sets targets; everything eases toward them here.

use std::f32::consts::TAU;

use super::dsp::{DcBlock, Noise, OnePole, Smooth, Svf, pan_gains};
use super::patch::{Layer, Voice};

/// Mix level into the soft clipper: a busy battle peaks just under full scale.
const MASTER: f32 = 1.3;
/// Most one-shots at once; past this the quietest is dropped.
const MAX_VOICES: usize = 48;

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
    /// Beam firing (0/1) and its heat 0..1.
    pub beam: f32,
    pub heat: f32,
    /// Flamethrower pouring (0/1).
    pub flame: f32,
    /// Heavy hull: a deeper, slower engine.
    pub heavy: bool,
}

/// A one-shot to start.
pub struct OneShot {
    pub layers: &'static [Layer],
    pub pitch: f32,
    pub gain: f32,
    pub pan: f32,
    pub air: f32,
}

struct TankVoice {
    gain: Smooth,
    pan: Smooth,
    air: Smooth,
    rpm: Smooth,
    load: Smooth,
    tread: Smooth,
    turret: Smooth,
    beam: Smooth,
    heat: Smooth,
    flame: Smooth,
    heavy: bool,
    noise: Noise,
    // Engine.
    cycle: f32,
    cylinder: f32,
    engine_lp: Svf,
    dc: DcBlock,
    // Tracks.
    link: f32,
    clank_a: Svf,
    clank_b: Svf,
    rumble: Svf,
    // Turret motor, beam, flame.
    motor: f32,
    motor_lp: Svf,
    beam_a: f32,
    beam_b: f32,
    beam_tone: f32,
    beam_lp: Svf,
    beam_hiss: Svf,
    flame_lp: Svf,
    flame_crackle: Svf,
    flame_hiss: Svf,
    flame_wobble: OnePole,
    air_lp: OnePole,
}

impl TankVoice {
    fn new(seed: u32) -> TankVoice {
        TankVoice {
            gain: Smooth::new(0.0),
            pan: Smooth::new(0.0),
            air: Smooth::new(8000.0),
            rpm: Smooth::new(0.0),
            load: Smooth::new(0.0),
            tread: Smooth::new(0.0),
            turret: Smooth::new(0.0),
            beam: Smooth::new(0.0),
            heat: Smooth::new(0.0),
            flame: Smooth::new(0.0),
            heavy: false,
            noise: Noise::new(seed.wrapping_mul(2654435761).max(1)),
            cycle: 0.0,
            cylinder: 1.0,
            engine_lp: Svf::default(),
            dc: DcBlock::default(),
            link: 0.0,
            clank_a: Svf::default(),
            clank_b: Svf::default(),
            rumble: Svf::default(),
            motor: 0.0,
            motor_lp: Svf::default(),
            beam_a: 0.0,
            beam_b: 0.0,
            beam_tone: 0.0,
            beam_lp: Svf::default(),
            beam_hiss: Svf::default(),
            flame_lp: Svf::default(),
            flame_crackle: Svf::default(),
            flame_hiss: Svf::default(),
            flame_wobble: OnePole::default(),
            air_lp: OnePole::default(),
        }
    }

    fn set(&mut self, p: &TankParams) {
        self.gain.target = p.gain;
        self.pan.target = p.pan;
        self.air.target = p.air;
        self.rpm.target = p.rpm;
        self.load.target = p.load;
        self.tread.target = p.tread;
        self.turret.target = p.turret;
        self.beam.target = p.beam;
        self.heat.target = p.heat;
        self.flame.target = p.flame;
        self.heavy = p.heavy;
    }

    fn silent(&self) -> bool {
        self.gain.value < 1e-4 && self.gain.target < 1e-4
    }

    /// Add this tank's loops into `out` (stereo frames).
    fn render(&mut self, out: &mut [f32], sr: f32, k: &Coefs) {
        let base = if self.heavy { 22.0 } else { 29.0 };
        for frame in out.chunks_exact_mut(2) {
            let gain = self.gain.next(k.slow);
            let pan = self.pan.next(k.slow);
            let air = self.air.next(k.slow);
            let rpm = self.rpm.next(k.rpm);
            let load = self.load.next(k.slow);
            let tread = self.tread.next(k.slow);
            let turret = self.turret.next(k.fast);
            let beam = self.beam.next(k.snap);
            let heat = self.heat.next(k.fast);
            let flame = self.flame.next(k.fast);
            let n = self.noise.next();

            // Engine: a pulse per firing stroke, each cylinder a little different.
            let f0 = base * (1.0 + 1.9 * rpm);
            self.cycle += f0 / sr;
            if self.cycle >= 1.0 {
                self.cycle -= 1.0;
                self.cylinder = 0.75 + 0.5 * self.noise.unit();
            }
            let pulse = (-self.cycle * 7.0).exp() * self.cylinder;
            let sub = (self.cycle * TAU * 0.5).sin();
            let raw = pulse + sub * 0.3 + n * 0.15 * pulse * (0.4 + load);
            let cutoff = 220.0 + 1000.0 * load + 600.0 * rpm;
            let engine = self.dc.run(self.engine_lp.run(raw, cutoff, 0.9, sr).low)
                * (0.3 + 0.4 * rpm + 0.3 * load);

            // Tracks: a clank per link meeting a sprocket, and ground rumble.
            let mut tracks = 0.0;
            if tread > 0.05 {
                self.link += tread / 0.3 / sr;
                let mut hit = 0.0;
                if self.link >= 1.0 {
                    self.link -= 1.0;
                    hit = 0.5 + self.noise.unit();
                }
                let x = hit + n * 0.04;
                let a = self.clank_a.run(x, 1100.0, 6.0, sr).band;
                let b = self.clank_b.run(x, 2700.0, 9.0, sr).band;
                let rumble = self.rumble.run(n, 170.0, 0.7, sr).low;
                tracks = (a * 0.6 + b * 0.35) * (tread / 5.0).min(1.0) * 0.5
                    + rumble * (tread / 8.0).min(1.0) * 0.9;
            }

            // Turret ring motor: a geared whine.
            let mut motor = 0.0;
            if turret > 0.01 {
                self.motor = (self.motor + (150.0 + 260.0 * turret) / sr).fract();
                let saw = self.motor * 2.0 - 1.0;
                let whine = self.motor_lp.run(saw, 1400.0, 1.2, sr).low;
                motor = (whine * 0.6 + (self.motor * TAU * 4.0).sin() * 0.15) * turret * 0.22;
            }

            // Laser beam: a detuned buzz with a warbling top that climbs as it heats.
            let mut buzz = 0.0;
            if beam > 0.001 {
                let f = 90.0 + 70.0 * heat;
                self.beam_a = (self.beam_a + f / sr).fract();
                self.beam_b = (self.beam_b + f * 1.007 / sr).fract();
                self.beam_tone = (self.beam_tone + (2200.0 + 1500.0 * heat) / sr).fract();
                let saws = self.beam_a + self.beam_b - 1.0;
                let body = self.beam_lp.run(saws, 1200.0 + 2000.0 * heat, 2.0, sr).low;
                let warble =
                    (self.beam_tone * TAU).sin() * (0.5 + 0.5 * (self.beam_a * TAU * 0.3).sin());
                let hiss = self.beam_hiss.run(n, 3200.0, 1.0, sr).band;
                buzz = (body * 0.6 + warble * 0.15 + hiss * 0.25) * beam * 0.45;
            }

            // Flamethrower: a roar with crackle in it.
            let mut fire = 0.0;
            if flame > 0.001 {
                let wobble = self.flame_wobble.run(self.noise.next(), k.wobble);
                let roar = self
                    .flame_lp
                    .run(n, 900.0 + 2500.0 * wobble.abs(), 0.8, sr)
                    .low;
                let hiss = self.flame_hiss.run(n, 4500.0, 0.7, sr).band;
                let spark = if self.noise.unit() < 70.0 / sr {
                    self.noise.next() * 2.5
                } else {
                    0.0
                };
                let crackle = self.flame_crackle.run(spark, 2200.0, 1.2, sr).band;
                fire = (roar * 1.1 + hiss * 0.35 + crackle * 0.6) * flame * 0.6;
            }

            let mix = engine * 0.55 + tracks + motor + buzz + fire;
            let x = self.air_lp.run(mix, OnePole::coef(air, sr)) * gain;
            let (l, r) = pan_gains(pan);
            frame[0] += x * l;
            frame[1] += x * r;
        }
    }
}

/// Smoothing coefficients for one render call.
struct Coefs {
    /// Spatial and load changes (frame-rate updates, no zipper noise).
    slow: f32,
    /// Engine revs (an engine takes a moment to spin up).
    rpm: f32,
    fast: f32,
    /// Trigger on/off.
    snap: f32,
    /// Flame flicker lowpass.
    wobble: f32,
}

pub struct Mixer {
    sr: f32,
    tanks: Vec<TankVoice>,
    voices: Vec<Voice>,
    master: Smooth,
    /// World sounds (loops and wind) fade out while paused.
    world: Smooth,
    wind: Smooth,
    wind_l: Svf,
    wind_r: Svf,
    wind_mod: Smooth,
    wind_timer: f32,
    noise: Noise,
    seed: u32,
    /// The world loops' buffer, kept so the audio thread doesn't allocate.
    scratch: Vec<f32>,
}

impl Mixer {
    pub fn new(sr: f32) -> Mixer {
        Mixer {
            sr,
            tanks: Vec::new(),
            voices: Vec::new(),
            master: Smooth::new(0.0),
            world: Smooth::new(0.0),
            wind: Smooth::new(0.0),
            wind_l: Svf::default(),
            wind_r: Svf::default(),
            wind_mod: Smooth::new(0.5),
            wind_timer: 0.0,
            noise: Noise::new(12345),
            seed: 1,
            scratch: Vec::new(),
        }
    }

    pub fn set_master(&mut self, gain: f32) {
        self.master.target = gain;
    }

    pub fn set_world(&mut self, gain: f32) {
        self.world.target = gain;
    }

    pub fn set_wind(&mut self, gain: f32) {
        self.wind.target = gain;
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
        ));
    }

    #[cfg(test)]
    pub fn voices(&self) -> usize {
        self.voices.len()
    }

    /// Fill `out` with interleaved stereo frames.
    pub fn render(&mut self, out: &mut [f32]) {
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
        self.render_wind(&mut world, &k);
        let world_coef = Smooth::coef(0.15, sr);
        for (o, w) in out.chunks_exact_mut(2).zip(world.chunks_exact(2)) {
            let g = self.world.next(world_coef);
            o[0] = w[0] * g;
            o[1] = w[1] * g;
        }
        self.scratch = world;

        for v in &mut self.voices {
            v.render(out, sr);
        }
        self.voices.retain(|v| !v.done());

        let master_coef = Smooth::coef(0.05, sr);
        for frame in out.chunks_exact_mut(2) {
            let g = self.master.next(master_coef);
            for s in frame {
                // Soft clip: loud moments squash instead of crackling.
                *s = (*s * g * MASTER).tanh();
            }
        }
    }

    /// Desert wind: two drifting bands of noise, one per ear.
    fn render_wind(&mut self, out: &mut [f32], k: &Coefs) {
        if self.wind.value < 1e-4 && self.wind.target < 1e-4 {
            return;
        }
        let drift = Smooth::coef(1.5, self.sr);
        for frame in out.chunks_exact_mut(2) {
            self.wind_timer -= 1.0 / self.sr;
            if self.wind_timer <= 0.0 {
                self.wind_timer = 1.5 + 3.0 * self.noise.unit();
                self.wind_mod.target = self.noise.unit();
            }
            let m = self.wind_mod.next(drift);
            let g = self.wind.next(k.slow) * (0.5 + m);
            let cutoff = 220.0 + 500.0 * m;
            let l = self
                .wind_l
                .run(self.noise.next(), cutoff, 1.4, self.sr)
                .band;
            let r = self
                .wind_r
                .run(self.noise.next(), cutoff * 1.1, 1.4, self.sr)
                .band;
            frame[0] += l * g;
            frame[1] += r * g;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::patch;
    use super::*;

    fn rms(buf: &[f32]) -> f32 {
        (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt()
    }

    fn driving(rpm: f32) -> TankParams {
        TankParams {
            gain: 1.0,
            air: 16_000.0,
            rpm,
            load: rpm,
            tread: rpm * 10.0,
            ..TankParams::default()
        }
    }

    fn settle(m: &mut Mixer) -> Vec<f32> {
        let mut buf = vec![0.0; 2 * 4800];
        for _ in 0..10 {
            m.render(&mut buf);
        }
        buf
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
    fn engine_gets_louder_flat_out() {
        let idle = {
            let mut m = Mixer::new(48_000.0);
            m.set_master(1.0);
            m.set_world(1.0);
            m.set_tanks(&[driving(0.0)]);
            rms(&settle(&mut m))
        };
        let flat_out = {
            let mut m = Mixer::new(48_000.0);
            m.set_master(1.0);
            m.set_world(1.0);
            m.set_tanks(&[driving(1.0)]);
            rms(&settle(&mut m))
        };
        assert!(idle > 0.005, "idle {idle}");
        assert!(flat_out > idle * 1.5, "idle {idle}, flat out {flat_out}");
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
            layers: patch::SELECT,
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
        let loud = TankParams {
            turret: 1.0,
            beam: 1.0,
            heat: 1.0,
            flame: 1.0,
            heavy: true,
            ..driving(1.0)
        };
        m.set_tanks(&[loud; 12]);
        for _ in 0..60 {
            m.play(OneShot {
                layers: patch::KILL,
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
