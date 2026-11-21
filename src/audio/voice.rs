//! The one-shot engine: a sound is a [`Layer`] stack (a swept oscillator or
//! filtered noise under a decay envelope each), and a [`Voice`] plays one.
//! The patches themselves live with what makes them, in `sounds/`; the
//! helpers here (`sine`, `noise`, `ring` and the base layer [`L`]) are for
//! writing them.

use super::dsp::{Noise, OnePole, Quick, Svf, sin_turns};

#[derive(Clone, Copy, PartialEq)]
pub enum Src {
    Sine,
    Noise,
    /// Random clicks, about this many per second (debris, fire crackle).
    Crackle(f32),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Filt {
    Off,
    Low,
    Band,
    High,
}

/// One layer of a one-shot.
#[derive(Clone, Copy, PartialEq)]
pub struct Layer {
    pub src: Src,
    /// Oscillator pitch, start and end, Hz.
    pub freq: (f32, f32),
    /// Time constant of the pitch glide, seconds.
    pub glide: f32,
    pub filt: Filt,
    /// Filter cutoff, start and end, Hz.
    pub cutoff: (f32, f32),
    /// Time constant of the cutoff sweep, seconds.
    pub sweep: f32,
    pub q: f32,
    /// Linear rise, seconds.
    pub attack: f32,
    /// Exponential decay time constant, seconds.
    pub decay: f32,
    pub gain: f32,
    /// Start offset, seconds.
    pub delay: f32,
}

/// The base layer: patches override what they need with `..L`.
pub const L: Layer = Layer {
    src: Src::Sine,
    freq: (440.0, 440.0),
    glide: 0.1,
    filt: Filt::Off,
    cutoff: (1000.0, 1000.0),
    sweep: 0.1,
    q: 0.7,
    attack: 0.001,
    decay: 0.1,
    gain: 1.0,
    delay: 0.0,
};

pub const fn sine(f0: f32, f1: f32, glide: f32, decay: f32, gain: f32) -> Layer {
    Layer {
        freq: (f0, f1),
        glide,
        decay,
        gain,
        ..L
    }
}

pub const fn noise(filt: Filt, c0: f32, c1: f32, sweep: f32, decay: f32, gain: f32) -> Layer {
    Layer {
        src: Src::Noise,
        filt,
        cutoff: (c0, c1),
        sweep,
        decay,
        gain,
        ..L
    }
}

/// A metal partial (inharmonic sines make a struck plate).
pub const fn ring(f: f32, decay: f32, gain: f32) -> Layer {
    sine(f, f * 0.98, 0.4, decay, gain)
}

/// A layer's running state. Its envelope and glides are exponentials in
/// time; rather than an `exp` each per sample they're stepped by a constant
/// factor, and worked out exactly again every [`RESYNC`] samples so rounding
/// can't creep in.
#[derive(Clone, Copy, Default)]
struct LayerState {
    phase: f32,
    svf: Svf,
    /// exp(-(t - attack) / decay), once past the attack.
    dec: f32,
    /// exp(-t / glide) and exp(-t / sweep), t from the layer's start.
    pitch_glide: f32,
    cutoff_glide: f32,
    /// What each of those is multiplied by per sample.
    dec_step: f32,
    pitch_step: f32,
    cutoff_step: f32,
    /// Whether the envelope and the glides hold exact values for this stretch.
    env_synced: bool,
    glides_synced: bool,
}

/// How many samples the stepped exponentials run before being worked out
/// exactly again.
const RESYNC: usize = 64;
/// A layer this far down (-80 dB) has died away and is skipped.
const DEAD: f32 = 9.2;

/// A one-shot playing.
pub struct Voice {
    layers: &'static [Layer],
    state: Vec<LayerState>,
    noise: Noise,
    /// Samples played.
    n: u32,
    t: f32,
    len: f32,
    pitch: f32,
    pub gain: f32,
    left: f32,
    right: f32,
    air: OnePole,
    air_coef: f32,
    sr: f32,
}

impl Voice {
    /// `pitch` scales every frequency; `air_coef` is the distance lowpass.
    /// It renders at `sr`.
    pub fn new(
        layers: &'static [Layer],
        pitch: f32,
        gain: f32,
        pan: (f32, f32),
        air_coef: f32,
        seed: u32,
        sr: f32,
    ) -> Voice {
        let step = |tau: f32| (-1.0 / (tau * sr)).exp();
        Voice {
            layers,
            state: layers
                .iter()
                .map(|l| LayerState {
                    dec_step: step(l.decay),
                    pitch_step: step(l.glide),
                    cutoff_step: step(l.sweep),
                    ..LayerState::default()
                })
                .collect(),
            noise: Noise::new(seed),
            n: 0,
            t: 0.0,
            len: length(layers),
            pitch,
            gain,
            left: pan.0,
            right: pan.1,
            air: OnePole::default(),
            air_coef,
            sr,
        }
    }

    pub fn done(&self) -> bool {
        self.t >= self.len
    }

    /// How loud it still is, roughly (for dropping the quietest when crowded).
    pub fn level(&self) -> f32 {
        self.gain * (1.0 - self.t / self.len).max(0.0)
    }

    /// Add the next `out.len() / 2` stereo frames into `out`.
    pub fn render(&mut self, out: &mut [f32]) {
        let sr = self.sr;
        for block in out.chunks_mut(2 * RESYNC) {
            for s in &mut self.state {
                s.env_synced = false;
                s.glides_synced = false;
            }
            for frame in block.chunks_exact_mut(2) {
                if self.t >= self.len {
                    return;
                }
                let mut sum = 0.0;
                for (l, s) in self.layers.iter().zip(&mut self.state) {
                    sum += layer(l, s, &mut self.noise, self.t, self.pitch, sr);
                }
                let x = self.air.run(sum, self.air_coef) * self.gain;
                frame[0] += x * self.left;
                frame[1] += x * self.right;
                // Counted, not summed: adding 1/sr drifts over a long tail.
                self.n += 1;
                self.t = self.n as f32 / sr;
            }
        }
    }
}

/// Seconds until every layer has died away (to -60 dB).
pub fn length(layers: &[Layer]) -> f32 {
    layers
        .iter()
        .map(|l| l.delay + l.attack + l.decay * 6.9)
        .fold(0.0, f32::max)
}

fn layer(l: &Layer, s: &mut LayerState, noise: &mut Noise, t: f32, pitch: f32, sr: f32) -> f32 {
    if t < l.delay {
        return 0.0;
    }
    let t = t - l.delay;
    if t > l.attack + l.decay * DEAD {
        // Still draw its noise, so the layers after it hear the same stream.
        match l.src {
            Src::Sine => {}
            Src::Noise => {
                noise.next();
            }
            Src::Crackle(rate) => {
                if noise.unit() < rate / sr {
                    noise.next();
                }
            }
        }
        return 0.0;
    }
    let env = if t < l.attack {
        // Not stepped here: the decay term can be huge before the attack ends.
        s.env_synced = false;
        t / l.attack
    } else {
        if s.env_synced {
            s.dec *= s.dec_step;
        } else {
            s.dec = (-(t - l.attack) / l.decay).exp();
            s.env_synced = true;
        }
        s.dec
    };
    if s.glides_synced {
        s.pitch_glide *= s.pitch_step;
        s.cutoff_glide *= s.cutoff_step;
    } else {
        s.pitch_glide = (-t / l.glide).exp();
        s.cutoff_glide = (-t / l.sweep).exp();
        s.glides_synced = true;
    }
    let glide = |(a, b): (f32, f32), g: f32| b + (a - b) * g;
    let x = match l.src {
        Src::Sine => {
            s.phase += glide(l.freq, s.pitch_glide) * pitch / sr;
            if s.phase >= 1.0 {
                s.phase = s.phase.fract_q();
            }
            sin_turns(s.phase)
        }
        Src::Noise => noise.next(),
        Src::Crackle(rate) => {
            if noise.unit() < rate / sr {
                noise.next() * 3.0
            } else {
                0.0
            }
        }
    };
    let y = match l.filt {
        Filt::Off => x,
        f => {
            let o = s
                .svf
                .run(x, glide(l.cutoff, s.cutoff_glide) * pitch, l.q, sr);
            match f {
                Filt::Low => o.low,
                Filt::Band => o.band,
                _ => o.high,
            }
        }
    };
    y * env * l.gain
}

/// The one-shot engine as first written, with libm's `exp`, `sin` and
/// `tan` worked out every sample: what [`Voice`] must keep sounding like.
/// Mono, at unit gain and with no air filter.
#[cfg(test)]
pub fn reference(layers: &[Layer], seed: u32, sr: f32, frames: usize) -> Vec<f32> {
    use std::f32::consts::{PI, TAU};
    let mut noise = Noise::new(seed);
    let mut phase = vec![0.0f32; layers.len()];
    // The plain trapezoidal filter, its tangent taken every sample.
    let mut ic = vec![(0.0f32, 0.0f32); layers.len()];
    let len = length(layers);
    let mut out = Vec::with_capacity(frames);
    for n in 0..frames {
        let t = n as f32 / sr;
        if t >= len {
            break;
        }
        let mut sum = 0.0;
        for (i, l) in layers.iter().enumerate() {
            if t < l.delay {
                continue;
            }
            let t = t - l.delay;
            let env = if t < l.attack {
                t / l.attack
            } else {
                (-(t - l.attack) / l.decay).exp()
            };
            let glide = |(a, b): (f32, f32), tau: f32| b + (a - b) * (-t / tau).exp();
            let x = match l.src {
                Src::Sine => {
                    phase[i] = (phase[i] + glide(l.freq, l.glide) * 1.0 / sr).fract();
                    (phase[i] * TAU).sin()
                }
                Src::Noise => noise.next(),
                Src::Crackle(rate) => {
                    if noise.unit() < rate / sr {
                        noise.next() * 3.0
                    } else {
                        0.0
                    }
                }
            };
            let y = match l.filt {
                Filt::Off => x,
                f => {
                    let cutoff = glide(l.cutoff, l.sweep);
                    let g = (PI * cutoff.clamp(10.0, sr * 0.45) / sr).tan();
                    let k = 1.0 / l.q.max(0.05);
                    let a1 = 1.0 / (1.0 + g * (g + k));
                    let (a2, a3) = (g * a1, g * g * a1);
                    let (ic1, ic2) = ic[i];
                    let v3 = x - ic2;
                    let v1 = a1 * ic1 + a2 * v3;
                    let v2 = ic2 + a2 * ic1 + a3 * v3;
                    ic[i] = (2.0 * v1 - ic1, 2.0 * v2 - ic2);
                    match f {
                        Filt::Low => v2,
                        Filt::Band => v1,
                        _ => x - k * v1 - v2,
                    }
                }
            };
            sum += y * env * l.gain;
        }
        out.push(sum);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::sounds::ui::CONFIRM;

    #[test]
    fn delayed_layers_start_silent() {
        let mut v = Voice::new(CONFIRM, 1.0, 1.0, (1.0, 1.0), 1.0, 1, 48_000.0);
        let mut buf = vec![0.0; 2 * 10];
        v.render(&mut buf);
        // Only the first blip has started; the second waits 70 ms.
        assert!(buf.iter().all(|x| x.abs() < 0.2));
    }
}
