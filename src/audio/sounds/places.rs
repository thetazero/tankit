//! The maps' own sounds, beyond the wind: what the places make, heard as you
//! get near them. The Kiln's lit crystals sing and fizz, its cores thrum and
//! crackle and the plasma river burbles; the Foundry's crucible still roars
//! and its wrecked presses hum, hiss and clank; the Sanctum's altar crystal
//! is a choir, its crystal clusters ring like rubbed glass and the wind
//! stirs the bell; the imperial city's banners flap and its broken dome moans;
//! Saltmarket's cables sing, its stall cloths snap and its wrecks creak.
//!
//! Where each one is comes from the map ([`crate::map::Map::sounds`]). Each
//! frame [`params`] places them round the listener and keeps only the
//! nearest few voiced; the mixer plays those as [`PlaceVoice`]s on their own
//! bus, which ducks under the match's one-shots so a fight always comes
//! through.

use std::f32::consts::TAU;

use crate::audio::Listener;
use crate::audio::dsp::{Noise, OnePole, Panner, Quick, Smooth, Svf};
use crate::audio::mixer::Coefs;
use crate::map::{Ambient, Map};

/// Most places voiced at once (the nearest, loudest ones).
pub const VOICED: usize = 8;

/// How a kind of place carries: its level up close, how far off it fades
/// out entirely (m), and how big it is (m: a big source doesn't swing hard
/// to one side as you pass). Levels put each kind's close-up loudness at
/// about a third to a half of your own tank's from the chase camera: the
/// centerpieces (a core, the crucible, the altar, the dome) the loudest.
struct Spec {
    level: f32,
    reach: f32,
    size: f32,
}

fn spec(kind: Ambient) -> Spec {
    let s = |level, reach, size| Spec { level, reach, size };
    match kind {
        Ambient::Crystal => s(0.17, 22.0, 2.0),
        Ambient::Core => s(0.19, 75.0, 6.0),
        Ambient::River => s(0.11, 30.0, 6.0),
        Ambient::Furnace => s(0.47, 70.0, 9.0),
        Ambient::Machine => s(0.41, 40.0, 3.0),
        Ambient::Altar => s(0.33, 60.0, 6.0),
        Ambient::Shrine => s(0.3, 28.0, 2.0),
        Ambient::Bell => s(0.33, 110.0, 3.0),
        Ambient::Banner => s(0.9, 24.0, 2.0),
        Ambient::Dome => s(1.06, 70.0, 20.0),
        Ambient::Cable => s(0.5, 22.0, 4.0),
        Ambient::Cloth => s(0.8, 20.0, 2.0),
        Ambient::Hull => s(1.0, 40.0, 5.0),
    }
}

/// One place's targets this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaceParams {
    pub kind: Ambient,
    pub gain: f32,
    pub pan: f32,
    pub air: f32,
}

/// Every place on `map` as heard by `ears`: all but the [`VOICED`] loudest
/// silent.
pub fn params(map: &Map, ears: &Listener) -> Vec<PlaceParams> {
    let mut out: Vec<PlaceParams> = map
        .sounds
        .iter()
        .map(|e| {
            let s = spec(e.kind);
            let d = e.at.distance(ears.pos);
            let near = (1.0 - d / s.reach).clamp(0.0, 1.0);
            let p = ears.place(e.at);
            PlaceParams {
                kind: e.kind,
                gain: s.level * near * near * (3.0 - 2.0 * near),
                pan: p.pan * d / (d + s.size),
                air: p.air,
            }
        })
        .collect();
    if out.len() > VOICED {
        let mut order: Vec<usize> = (0..out.len()).collect();
        order.sort_by(|&a, &b| out[b].gain.total_cmp(&out[a].gain));
        for &i in &order[VOICED..] {
            out[i].gain = 0.0;
        }
    }
    out
}

/// One place's sound, placed: whatever its kind makes.
pub struct PlaceVoice {
    kind: Ambient,
    gain: Smooth,
    pan: Smooth,
    air: Smooth,
    air_lp: OnePole,
    panner: Panner,
    s: State,
}

impl PlaceVoice {
    pub fn new(kind: Ambient, seed: u32) -> PlaceVoice {
        let mut noise = Noise::new(seed.wrapping_mul(0x2545_F491).max(1));
        let vary = noise.unit();
        let mut s = State {
            noise,
            vary,
            ph: [0.0; 6],
            f: [Svf::default(); 4],
            env: [0.0; 6],
            timer: [0.0; 2],
            gust: Smooth::new(0.5),
            wander: Smooth::new(0.5),
            t: 0.0,
            pitch: 0.0,
        };
        // Spread the first events out so a row of them doesn't start together.
        s.timer = [s.noise.unit() * 4.0, s.noise.unit() * 2.0];
        PlaceVoice {
            kind,
            gain: Smooth::new(0.0),
            pan: Smooth::new(0.0),
            air: Smooth::new(8000.0),
            air_lp: OnePole::default(),
            panner: Panner::default(),
            s,
        }
    }

    pub fn kind(&self) -> Ambient {
        self.kind
    }

    pub fn set(&mut self, p: &PlaceParams) {
        self.gain.target = p.gain;
        self.pan.target = p.pan;
        self.air.target = p.air;
    }

    pub fn silent(&self) -> bool {
        self.gain.value < 1e-4 && self.gain.target < 1e-4
    }

    /// Add this place into `out` (stereo frames).
    pub fn render(&mut self, out: &mut [f32], sr: f32, k: &Coefs) {
        for frame in out.chunks_exact_mut(2) {
            let gain = self.gain.next(k.slow);
            let pan = self.pan.next(k.slow);
            let air = self.air.next(k.slow);
            let x = self.s.next(self.kind, sr);
            let x = self.air_lp.run_at(x, air, sr) * gain;
            let (l, r) = self.panner.gains(pan);
            frame[0] += x * l;
            frame[1] += x * r;
        }
    }
}

/// What every kind's synth keeps between samples; each uses what it needs.
struct State {
    noise: Noise,
    /// 0..1, fixed per place: its pitch and pace, so two of a kind differ.
    vary: f32,
    /// Oscillator phases, 0..1.
    ph: [f32; 6],
    f: [Svf; 4],
    /// Decaying envelopes of events (an arc, a bubble, a clank, a bell).
    env: [f32; 6],
    /// Seconds to the next event.
    timer: [f32; 2],
    /// The wind on this place, 0..1, drifting.
    gust: Smooth,
    /// A faster drift (a river's babble, a choir's vowel).
    wander: Smooth,
    /// Seconds since it started.
    t: f32,
    /// A swept pitch (a bubble rising), Hz.
    pitch: f32,
}

/// Phase `ph` advanced by `f` Hz; the sine at the new phase.
fn osc(ph: &mut f32, f: f32, sr: f32) -> f32 {
    *ph = (*ph + f / sr).fract_q();
    (*ph * TAU).sin_q()
}

/// Seconds-per-sample decay factor for time constant `tau`.
fn decay(tau: f32, sr: f32) -> f32 {
    (-1.0 / (tau * sr)).exp()
}

impl State {
    /// Count timer `i` down; true (and rearmed to `lo..hi` s) when it fires.
    fn every(&mut self, i: usize, lo: f32, hi: f32, sr: f32) -> bool {
        self.timer[i] -= 1.0 / sr;
        if self.timer[i] <= 0.0 {
            self.timer[i] = lo + (hi - lo) * self.noise.unit();
            true
        } else {
            false
        }
    }

    /// Drift the gust toward a new target every few seconds; its value.
    fn gusting(&mut self, sr: f32, lo: f32, hi: f32) -> f32 {
        if self.every(1, lo, hi, sr) {
            self.gust.target = self.noise.unit();
        }
        self.gust.next(Smooth::coef(lo * 0.6, sr))
    }

    fn next(&mut self, kind: Ambient, sr: f32) -> f32 {
        self.t += 1.0 / sr;
        let n = self.noise.next();
        match kind {
            Ambient::Crystal => self.crystal(n, sr),
            Ambient::Core => self.core(n, sr),
            Ambient::River => self.river(n, sr),
            Ambient::Furnace => self.furnace(n, sr),
            Ambient::Machine => self.machine(n, sr),
            Ambient::Altar => self.altar(n, sr),
            Ambient::Shrine => self.shrine(sr),
            Ambient::Bell => self.bell(sr),
            Ambient::Banner => self.flap(n, sr, 2.2, 900.0),
            Ambient::Cloth => self.flap(n, sr, 4.5, 1700.0),
            Ambient::Dome => self.dome(n, sr),
            Ambient::Cable => self.cable(n, sr),
            Ambient::Hull => self.hull(n, sr),
        }
    }

    /// Glass singing at one of a pentatonic set of pitches, swelling slowly,
    /// with a beating twin and a glassy overtone; plasma fizzing up the seam.
    fn crystal(&mut self, n: f32, sr: f32) -> f32 {
        const NOTES: [f32; 5] = [660.0, 742.5, 880.0, 990.0, 1113.75];
        let f = NOTES[(self.vary * 4.99) as usize];
        let swell = 0.55 + 0.45 * (TAU * (0.11 + 0.1 * self.vary) * self.t).sin_q();
        let tone = osc(&mut self.ph[0], f, sr) * 0.5
            + osc(&mut self.ph[1], f * 1.004, sr) * 0.4
            + osc(&mut self.ph[2], f * 2.76, sr) * 0.1;
        let crackle = n * n * n;
        let fizz = self.f[0].run(crackle, 4800.0, 1.5, sr).band;
        tone * swell * 0.25 + fizz * 0.25
    }

    /// A deep thrum that wobbles, a roar of plasma under it, and arcs
    /// snapping off it now and then.
    fn core(&mut self, n: f32, sr: f32) -> f32 {
        let f = 52.0 + 6.0 * self.vary;
        let wobble = 0.75 + 0.25 * (TAU * 0.37 * self.t).sin_q();
        let thrum = osc(&mut self.ph[0], f, sr)
            + osc(&mut self.ph[1], f * 2.0, sr) * 0.6
            + osc(&mut self.ph[2], f * 3.01, sr) * 0.3;
        let roar = self.f[0].run(n, 320.0, 0.8, sr).low;
        if self.every(0, 0.08, 0.7, sr) {
            self.env[0] = 0.4 + 0.6 * self.noise.unit();
        }
        self.env[0] *= decay(0.025, sr);
        let arc = self.f[1].run(n, 2600.0, 1.2, sr).band * self.env[0];
        thrum * wobble * 0.22 + roar * 0.35 + arc * 0.9
    }

    /// Plasma running along the ground: a liquid babble (a resonance
    /// skipping about), a low hum, a fizz on top.
    fn river(&mut self, n: f32, sr: f32) -> f32 {
        if self.every(0, 0.03, 0.12, sr) {
            self.wander.target = self.noise.unit();
        }
        let w = self.wander.next(Smooth::coef(0.025, sr));
        let babble = self.f[0].run(n, 280.0 + 700.0 * w, 5.0, sr).band;
        let hum = osc(&mut self.ph[0], 82.0 + 10.0 * self.vary, sr);
        let fizz = self.f[1].run(n * n * n, 5200.0, 1.0, sr).band;
        babble * 0.5 + hum * 0.08 + fizz * 0.2
    }

    /// The crucible: a low roar breathing slowly, flame through it, and
    /// molten metal bubbling (a pitch rising fast under a quick decay).
    fn furnace(&mut self, n: f32, sr: f32) -> f32 {
        let breathe = 0.75 + 0.25 * (TAU * 0.13 * self.t).sin_q();
        let roar = self.f[0].run(n, 170.0, 0.8, sr).low;
        let flame = self.f[1].run(n, 650.0, 1.0, sr).band;
        if self.every(0, 0.12, 0.55, sr) {
            self.env[0] = 0.5 + 0.5 * self.noise.unit();
            self.pitch = 70.0 + 60.0 * self.noise.unit();
        }
        self.env[0] *= decay(0.06, sr);
        self.pitch *= 1.0 + 9.0 / sr;
        let bubble = osc(&mut self.ph[0], self.pitch, sr) * self.env[0];
        (roar * 0.8 + flame * 0.15) * breathe + bubble * 0.3
    }

    /// A wrecked press: mains hum with a flicker in it, and every few
    /// seconds its hydraulics sigh and the head lands with a metal clank.
    fn machine(&mut self, n: f32, sr: f32) -> f32 {
        let hum = osc(&mut self.ph[0], 100.0, sr) * 0.6
            + osc(&mut self.ph[1], 200.0, sr) * 0.3
            + osc(&mut self.ph[2], 300.0, sr) * 0.2;
        let flicker = if (TAU * 7.0 * self.t).sin_q() > 0.97 {
            0.4
        } else {
            1.0
        };
        if self.every(0, 4.0 + 2.0 * self.vary, 9.0, sr) {
            self.env[0] = 1.0;
            // The clank, this long after the hiss starts.
            self.env[5] = 0.45 + 0.2 * self.noise.unit();
        }
        self.env[0] *= decay(0.35, sr);
        let hiss = self.f[0].run(n, 3200.0, 0.8, sr).band * self.env[0];
        if self.env[5] > 0.0 {
            self.env[5] -= 1.0 / sr;
            if self.env[5] <= 0.0 {
                self.env[1] = 1.0;
            }
        }
        self.env[1] *= decay(0.18, sr);
        let clank = (osc(&mut self.ph[3], 420.0, sr)
            + osc(&mut self.ph[4], 1130.0, sr) * 0.7
            + osc(&mut self.ph[5], 1870.0, sr) * 0.5)
            * self.env[1];
        hum * flicker * 0.06 + hiss * 0.5 + clank * 0.35
    }

    /// The altar crystal: a choir on one open chord (detuned buzzing
    /// voices through an "ah" that drifts toward "oh"), swelling slowly.
    fn altar(&mut self, n: f32, sr: f32) -> f32 {
        const VOICES: [f32; 4] = [220.0, 220.9, 329.6, 440.6];
        let mut buzz = 0.0;
        for (j, &f) in VOICES.iter().enumerate() {
            // A little vibrato each, out of step.
            let vib = 1.0 + 0.004 * (TAU * (4.6 + j as f32 * 0.3) * self.t).sin_q();
            self.ph[j] = (self.ph[j] + f * vib / sr).fract_q();
            buzz += self.ph[j] * 2.0 - 1.0;
        }
        buzz += n * 0.15;
        if self.every(0, 3.0, 6.0, sr) {
            self.wander.target = self.noise.unit();
        }
        let w = self.wander.next(Smooth::coef(2.0, sr));
        let f1 = 750.0 - 250.0 * w;
        let f2 = 1150.0 - 300.0 * w;
        let a = self.f[0].run(buzz, f1, 5.0, sr).band;
        let b = self.f[1].run(buzz, f2, 6.0, sr).band;
        let swell = 0.65 + 0.35 * (TAU * 0.07 * self.t).sin_q();
        let shimmer =
            osc(&mut self.ph[4], 1760.0, sr) * (0.5 + 0.5 * (TAU * 0.23 * self.t).sin_q());
        (a * 0.06 + b * 0.04) * swell + shimmer * 0.03
    }

    /// A cluster of Resonant crystals: a soft chord of pure tones, each
    /// swelling on its own, like a wet finger round a glass.
    fn shrine(&mut self, sr: f32) -> f32 {
        const ROOTS: [f32; 4] = [523.3, 587.3, 659.3, 784.0];
        let f = ROOTS[(self.vary * 3.99) as usize];
        let mut out = 0.0;
        for (j, (mult, g)) in [(1.0f32, 0.5f32), (1.5, 0.3), (2.0, 0.2)]
            .into_iter()
            .enumerate()
        {
            let swell =
                0.5 + 0.5 * (TAU * (0.09 + 0.05 * j as f32) * self.t + j as f32 * 2.0).sin_q();
            out += osc(&mut self.ph[j], f * mult, sr) * g * swell;
        }
        out * 0.3
    }

    /// The bell, stirred by the wind every so often: never struck hard, so a
    /// soft stroke and a long hum (a bell's minor-third partials).
    fn bell(&mut self, sr: f32) -> f32 {
        const ROOT: f32 = 196.0;
        // (partial, level, decay s)
        const PARTIALS: [(f32, f32, f32); 6] = [
            (0.5, 0.5, 6.0),
            (1.0, 0.6, 4.0),
            (1.2, 0.35, 3.0),
            (1.5, 0.25, 2.2),
            (2.0, 0.3, 1.6),
            (2.66, 0.15, 0.9),
        ];
        if self.every(0, 6.0, 14.0, sr) {
            let s = 0.3 + 0.7 * self.noise.unit();
            for e in &mut self.env {
                *e = s;
            }
        }
        let mut out = 0.0;
        for (j, &(m, g, d)) in PARTIALS.iter().enumerate() {
            if self.env[j] > 1e-4 {
                self.env[j] *= decay(d, sr);
                out += osc(&mut self.ph[j], ROOT * m, sr) * g * self.env[j];
            }
        }
        out * 0.3
    }

    /// Cloth in the wind: flapping at about `rate` Hz (faster in a gust),
    /// each flap a snap of noise around `bright` Hz and a soft whump.
    fn flap(&mut self, n: f32, sr: f32, rate: f32, bright: f32) -> f32 {
        let g = self.gusting(sr, 1.0, 3.0);
        let r = rate * (0.8 + 0.4 * self.vary) * (0.6 + 0.8 * g);
        let p = osc(&mut self.ph[0], r, sr).max(0.0).powi(6);
        let snap = self.f[0].run(n, bright * (0.7 + 0.6 * g), 0.9, sr).band;
        let whump = self.f[1].run(n, 160.0, 0.7, sr).low;
        (snap * (0.15 + p * 1.2) + whump * p * 0.8) * (0.3 + 0.7 * g) * 0.6
    }

    /// Wind in a broken dome: noise ringing in its low hollow modes, swelling
    /// with the gusts, and a thin whistle across the hole in it.
    fn dome(&mut self, n: f32, sr: f32) -> f32 {
        let g = self.gusting(sr, 2.5, 6.0);
        let f0 = 98.0 * (1.0 + 0.04 * self.vary);
        let modes = self.f[0].run(n, f0, 20.0, sr).band
            + self.f[1].run(n, f0 * 1.5, 20.0, sr).band * 0.7
            + self.f[2].run(n, f0 * 2.0, 22.0, sr).band * 0.5;
        let whistle = self.f[3].run(n, 880.0 + 120.0 * g, 40.0, sr).band;
        modes * 0.09 * (0.3 + g) + whistle * 0.02 * g * g
    }

    /// A power cable singing in the wind: a thin aeolian tone that drifts in
    /// pitch and comes and goes with the gusts, and its octave.
    fn cable(&mut self, n: f32, sr: f32) -> f32 {
        let g = self.gusting(sr, 1.5, 4.0);
        let f = (650.0 + 450.0 * self.vary) * (0.9 + 0.2 * g);
        let a = self.f[0].run(n, f, 40.0, sr).band;
        let b = self.f[1].run(n, f * 2.0, 40.0, sr).band;
        (a + b * 0.4) * 0.09 * g * g
    }

    /// A wreck in the heat: every few seconds a creak (stick-slip clicks
    /// through a wooden or steel body, swelling and fading), and the odd
    /// metal tick between.
    fn hull(&mut self, n: f32, sr: f32) -> f32 {
        if self.every(0, 3.0 + 2.0 * self.vary, 9.0, sr) {
            // The creak's length, and its click rate.
            self.env[0] = 0.4 + 0.8 * self.noise.unit();
            self.env[1] = self.env[0];
            self.pitch = 25.0 + 35.0 * self.noise.unit();
        }
        let mut click = 0.0;
        if self.env[0] > 0.0 {
            self.env[0] -= 1.0 / sr;
            let along = 1.0 - self.env[0].max(0.0) / self.env[1];
            let shape = (along * std::f32::consts::PI).sin_q();
            self.pitch *= 1.0 + 0.4 / sr;
            let before = self.ph[0];
            self.ph[0] = (self.ph[0] + self.pitch / sr).fract_q();
            if self.ph[0] < before {
                click = shape * (0.6 + 0.4 * self.noise.unit());
            }
        }
        let body = self.f[0]
            .run(click, 420.0 + 300.0 * self.vary, 8.0, sr)
            .band;
        if self.every(1, 0.6, 3.5, sr) {
            self.env[2] = 0.3 + 0.7 * self.noise.unit();
        }
        self.env[2] *= decay(0.004, sr);
        let tick = self.f[1].run(n * self.env[2], 2600.0, 6.0, sr).band;
        body * 1.6 + tick * 1.2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::mixer::Mixer;
    use crate::audio::testkit::{driving, ears, rms};

    const ALL: [Ambient; 13] = [
        Ambient::Crystal,
        Ambient::Core,
        Ambient::River,
        Ambient::Furnace,
        Ambient::Machine,
        Ambient::Altar,
        Ambient::Shrine,
        Ambient::Bell,
        Ambient::Banner,
        Ambient::Dome,
        Ambient::Cable,
        Ambient::Cloth,
        Ambient::Hull,
    ];

    /// `seconds` of one place of `kind` at `gain`, alone in a 48 kHz mix.
    fn alone(kind: Ambient, gain: f32, seconds: f32) -> Vec<f32> {
        let mut m = Mixer::new(48_000.0);
        m.set_master(1.0);
        m.set_world(1.0);
        m.set_places(&[PlaceParams {
            kind,
            gain,
            pan: 0.0,
            air: 16_000.0,
        }]);
        let mut out = vec![0.0; 2 * (48_000.0 * seconds) as usize];
        m.render(&mut out);
        out
    }

    /// Every kind makes some sound over half a minute, finite and bounded,
    /// and up close stays well under a tank driving past the camera.
    #[test]
    fn every_place_is_heard_but_under_a_tank() {
        let mut m = Mixer::new(48_000.0);
        m.set_master(1.0);
        m.set_world(1.0);
        m.set_tanks(&[driving(0.5)]);
        let tank = rms(&crate::audio::testkit::settle(&mut m));
        for kind in ALL {
            let buf = alone(kind, spec(kind).level, 30.0);
            assert!(
                buf.iter().all(|x| x.is_finite() && x.abs() < 1.0),
                "{kind:?}"
            );
            let level = rms(&buf);
            assert!(level > 0.004, "{kind:?} too quiet: {level}");
            assert!(level < tank * 0.6, "{kind:?} {level} against a tank {tank}");
        }
    }

    /// Nearer is louder, and past its reach a place is silent.
    #[test]
    fn nearer_is_louder() {
        let mut map = Map::empty(100.0);
        map.sound(Ambient::Furnace, glam::Vec3::new(0.0, 0.0, -10.0));
        map.sound(Ambient::Furnace, glam::Vec3::new(0.0, 0.0, -40.0));
        map.sound(Ambient::Cloth, glam::Vec3::new(0.0, 0.0, -60.0));
        let p = params(&map, &ears());
        assert!(p[0].gain > p[1].gain && p[1].gain > 0.0);
        assert_eq!(p[2].gain, 0.0);
    }

    /// In a crowd only the loudest few play.
    #[test]
    fn only_the_nearest_few_are_voiced() {
        let mut map = Map::empty(100.0);
        for i in 0..30 {
            map.sound(
                Ambient::Cable,
                glam::Vec3::new(0.0, 0.0, -1.0 - i as f32 * 0.5),
            );
        }
        let p = params(&map, &ears());
        assert_eq!(p.iter().filter(|p| p.gain > 0.0).count(), VOICED);
        assert!(p[..VOICED].iter().all(|p| p.gain > 0.0));
    }

    /// Every map the issue names sounds somewhere, with what it should.
    #[test]
    fn every_map_has_its_places() {
        use Ambient::*;
        for (name, map, want) in [
            ("kiln", Map::kiln(), &[Crystal, Core, River][..]),
            ("foundry", Map::foundry(), &[Furnace, Machine]),
            ("sanctum", Map::sanctum(), &[Altar, Shrine, Bell]),
            ("city", Map::city(0), &[Banner, Dome]),
            ("saltmarket", Map::saltmarket(), &[Cable, Cloth, Hull]),
        ] {
            for k in want {
                assert!(map.sounds.iter().any(|e| e.kind == *k), "{name}: no {k:?}");
            }
            for e in &map.sounds {
                assert!(e.at.is_finite(), "{name}: {e:?}");
            }
        }
    }

    /// A blast ducks the places, and they come back after.
    #[test]
    fn a_fight_ducks_them() {
        let mut m = Mixer::new(48_000.0);
        m.set_master(1.0);
        m.set_world(1.0);
        m.set_places(&[PlaceParams {
            kind: Ambient::Furnace,
            gain: 1.0,
            pan: 0.0,
            air: 16_000.0,
        }]);
        let mut buf = vec![0.0; 2 * 24_000];
        m.render(&mut buf);
        assert!(m.duck() > 0.95, "{}", m.duck());
        m.play(crate::audio::mixer::OneShot {
            layers: super::super::hits::KILL,
            pitch: 1.0,
            gain: 1.0,
            pan: 0.0,
            air: 16_000.0,
        });
        let mut short = vec![0.0; 2 * 2400];
        m.render(&mut short);
        assert!(m.duck() < 0.6, "{}", m.duck());
        for _ in 0..8 {
            m.render(&mut buf);
        }
        assert!(m.duck() > 0.9, "{}", m.duck());
    }

    /// Writes `docs/audio/places-<map>.wav`: a slow drive past each kind of
    /// place on each map, as heard in the game (wind and all, no tanks):
    /// `cargo test record_places -- --ignored`.
    #[test]
    #[ignore]
    fn record_places() {
        use crate::audio::wav;
        use glam::Vec3;
        const RATE: u32 = 48_000;
        let volume = crate::profile::Settings::default().volume();
        for (name, map) in [
            ("kiln", Map::kiln()),
            ("foundry", Map::foundry()),
            ("sanctum", Map::sanctum()),
            ("city", Map::city(0)),
            ("saltmarket", Map::saltmarket()),
        ] {
            let mut m = Mixer::new(RATE as f32);
            m.set_master(volume * volume);
            m.set_world(1.0);
            m.set_wind(crate::audio::WIND);
            let mut out = Vec::new();
            let frame = (RATE / 60) as usize * 2;
            // Each kind on the map once: the one nearest the middle.
            let mut seen = Vec::new();
            let mid = Vec3::new(map.size * 0.5, 0.0, map.size * 0.5);
            let mut stops: Vec<_> = map.sounds.clone();
            stops.sort_by(|a, b| a.at.distance(mid).total_cmp(&b.at.distance(mid)));
            stops.retain(|e| {
                let new = !seen.contains(&e.kind);
                seen.push(e.kind);
                new
            });
            for e in stops {
                // Drive in from 35 m off to 4 m over 5 s, sit 2 s, then on.
                let from = e.at + Vec3::new(35.0, 0.0, 0.0);
                let to = e.at + Vec3::new(4.0, 0.0, 2.0);
                for i in 0..(8 * 60) {
                    let t = (i as f32 / (5.0 * 60.0)).min(1.0);
                    let pos = from.lerp(to, t) + Vec3::Y * 2.5;
                    let ears = Listener::of(&crate::render::Camera {
                        pos,
                        target: pos + Vec3::NEG_X,
                        fov_y: 1.0,
                    });
                    m.set_places(&params(&map, &ears));
                    let start = out.len();
                    out.resize(start + frame, 0.0);
                    m.render(&mut out[start..]);
                }
            }
            std::fs::write(format!("docs/audio/places-{name}.wav"), wav(&out, RATE)).unwrap();
        }
    }
}
