//! The audio thread's side: continuous per-tank loops (engine, tracks, turret
//! motor, beam, flame), the burn where each beam lands, the desert wind, and
//! one-shot voices, mixed to stereo.
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
    /// Beam level (0 off; about 1 up close, more far off, where it carries)
    /// and its heat 0..1.
    pub beam: f32,
    pub heat: f32,
    /// Flamethrower pouring (0/1).
    pub flame: f32,
    /// Heavy hull: a deeper, slower engine.
    pub heavy: bool,
    /// Rides a plasma cushion: a field hum instead of an engine and tracks.
    pub hover: bool,
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
    hover: bool,
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
    /// Ring-gear tooth phase, the traverse drive's hum phase.
    motor: f32,
    motor_hum: f32,
    motor_lp: Svf,
    motor_a: Svf,
    motor_b: Svf,
    beam_a: f32,
    beam_b: f32,
    beam_throb: f32,
    beam_lp: Svf,
    beam_nasal: Svf,
    beam_bite: Svf,
    beam_arc: Svf,
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
            hover: false,
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
            motor_hum: 0.0,
            motor_lp: Svf::default(),
            motor_a: Svf::default(),
            motor_b: Svf::default(),
            beam_a: 0.0,
            beam_b: 0.0,
            beam_throb: 0.0,
            beam_lp: Svf::default(),
            beam_nasal: Svf::default(),
            beam_bite: Svf::default(),
            beam_arc: Svf::default(),
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
        self.hover = p.hover;
    }

    fn silent(&self) -> bool {
        self.gain.value < 1e-4 && self.gain.target < 1e-4
    }

    /// Add this tank's loops into `out` (stereo frames).
    fn render(&mut self, out: &mut [f32], sr: f32, k: &Coefs) {
        let base = match (self.hover, self.heavy) {
            (true, _) => 58.0,
            (_, true) => 22.0,
            _ => 29.0,
        };
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
            let engine = if self.hover {
                // Field hum: a steady tone and its fifth, rising with effort, over hiss.
                let hum = (self.cycle * TAU).sin() * 0.7 + (self.cycle * TAU * 3.0).sin() * 0.25;
                let air = self.engine_lp.run(n, 500.0 + 1200.0 * rpm, 1.5, sr).band;
                (hum + air * 0.8) * (0.15 + 0.2 * rpm + 0.15 * load)
            } else {
                self.dc.run(self.engine_lp.run(raw, cutoff, 0.9, sr).low)
                    * (0.3 + 0.4 * rpm + 0.3 * load)
            };

            // Tracks: a clank per link meeting a sprocket, and ground rumble.
            let mut tracks = 0.0;
            if tread > 0.05 && !self.hover {
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

            // Turret traverse: a heavy ring gear grinding round. Teeth meet tens
            // of times a second, so they're heard as a ratchet, not a pitch; under
            // them a low grind and a near-steady drive hum. Nothing tonal in the
            // voice range (a pitched whine that rose with speed sounded like a scream).
            let mut motor = 0.0;
            if turret > 0.01 {
                self.motor += (14.0 + 26.0 * turret) / sr;
                let mut tooth = 0.0;
                if self.motor >= 1.0 {
                    self.motor -= 1.0;
                    tooth = 0.6 + 0.8 * self.noise.unit();
                }
                let x = tooth + n * 0.02;
                let clack = self.motor_a.run(x, 520.0, 4.0, sr).band * 0.5
                    + self.motor_b.run(x, 1350.0, 6.0, sr).band * 0.3;
                let grind = self.motor_lp.run(n, 260.0 + 120.0 * turret, 0.8, sr).low
                    * (0.7 + 0.6 * (1.0 - self.motor));
                self.motor_hum = (self.motor_hum + (46.0 + 8.0 * turret) / sr).fract();
                let hum =
                    (self.motor_hum * TAU).sin() * 0.5 + (self.motor_hum * TAU * 2.0).sin() * 0.25;
                motor = (clack * 1.6 + grind * 2.4 + hum * 0.25) * turret.sqrt() * 0.3;
            }

            // Laser beam: a harsh electric buzz pulsing a few times a second,
            // fizzing with arcs, climbing as it heats. Its weight is in the
            // 0.6-4 kHz mids, where laptop speakers play: a buzz that was
            // mostly its 90 Hz fundamental went unheard on them.
            let mut buzz = 0.0;
            if beam > 0.001 {
                let f = 120.0 + 80.0 * heat;
                self.beam_a = (self.beam_a + f / sr).fract();
                self.beam_b = (self.beam_b + f * 1.006 / sr).fract();
                let saws = self.beam_a + self.beam_b - 1.0;
                // Two formants make the buzz nasal and biting rather than a drone.
                let nasal = self
                    .beam_nasal
                    .run(saws, 900.0 + 500.0 * heat, 2.5, sr)
                    .band;
                let bite = self
                    .beam_bite
                    .run(saws, 2400.0 + 900.0 * heat, 3.0, sr)
                    .band;
                let body = self.beam_lp.run(saws, 350.0, 0.7, sr).low;
                self.beam_throb = (self.beam_throb + (6.0 + 6.0 * heat) / sr).fract();
                let throb = 0.75 + 0.25 * (self.beam_throb * TAU).sin();
                // Arcing: snaps of static, more of them hot.
                let spark = if self.noise.unit() < (40.0 + 80.0 * heat) / sr {
                    self.noise.next() * 3.0
                } else {
                    0.0
                };
                let arc = self.beam_arc.run(spark, 3600.0, 1.2, sr).band;
                let hiss = self.beam_hiss.run(n, 5500.0, 0.9, sr).band;
                buzz =
                    ((nasal * 0.55 + bite * 0.35) * throb + body * 0.25 + arc * 0.6 + hiss * 0.12)
                        * beam
                        * 0.9;
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

/// The burn where a beam lands: a frying sizzle with pops of boiling
/// material, a low roar of vapour, and on a hull the metal ringing as it heats.
struct BurnVoice {
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
}

impl BurnVoice {
    fn new(seed: u32) -> BurnVoice {
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
        }
    }

    fn set(&mut self, p: &BurnParams) {
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

    fn silent(&self) -> bool {
        self.gain.value < 1e-4 && self.gain.target < 1e-4
    }

    fn render(&mut self, out: &mut [f32], sr: f32, k: &Coefs) {
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
    burns: Vec<BurnVoice>,
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
            burns: Vec::new(),
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
        for b in &mut self.burns {
            if !b.silent() {
                b.render(&mut world, sr, &k);
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

    /// Level above 600 Hz, which small speakers actually play.
    fn mids(buf: &[f32]) -> f32 {
        let mut f = Svf::default();
        let hp: Vec<f32> = buf
            .chunks_exact(2)
            .map(|lr| f.run(lr[0] + lr[1], 600.0, 0.7, 48_000.0).high)
            .collect();
        rms(&hp[hp.len() / 2..])
    }

    fn alone(tank: TankParams, burn: BurnParams) -> Vec<f32> {
        let mut m = Mixer::new(48_000.0);
        m.set_master(1.0);
        m.set_world(1.0);
        m.set_tanks(&[tank]);
        m.set_burns(&[burn]);
        settle(&mut m)
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
