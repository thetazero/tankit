//! The one-shot engine: a sound is a [`Layer`] stack (a swept oscillator or
//! filtered noise under a decay envelope each), and a [`Voice`] plays one.
//! The patches themselves live with what makes them, in `sounds/`; the
//! helpers here (`sine`, `noise`, `ring` and the base layer [`L`]) are for
//! writing them.

use std::f32::consts::TAU;

use super::dsp::{Noise, OnePole, Svf};

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

#[derive(Clone, Copy, Default)]
struct LayerState {
    phase: f32,
    svf: Svf,
}

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
}

impl Voice {
    /// `pitch` scales every frequency; `air_coef` is the distance lowpass.
    pub fn new(
        layers: &'static [Layer],
        pitch: f32,
        gain: f32,
        pan: (f32, f32),
        air_coef: f32,
        seed: u32,
    ) -> Voice {
        Voice {
            layers,
            state: vec![LayerState::default(); layers.len()],
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
    pub fn render(&mut self, out: &mut [f32], sr: f32) {
        for frame in out.chunks_exact_mut(2) {
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
    let env = if t < l.attack {
        t / l.attack
    } else {
        (-(t - l.attack) / l.decay).exp()
    };
    let glide = |(a, b): (f32, f32), tau: f32| b + (a - b) * (-t / tau).exp();
    let x = match l.src {
        Src::Sine => {
            s.phase = (s.phase + glide(l.freq, l.glide) * pitch / sr).fract();
            (s.phase * TAU).sin()
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
            let o = s.svf.run(x, glide(l.cutoff, l.sweep) * pitch, l.q, sr);
            match f {
                Filt::Low => o.low,
                Filt::Band => o.band,
                _ => o.high,
            }
        }
    };
    y * env * l.gain
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::sounds::ui::CONFIRM;

    #[test]
    fn delayed_layers_start_silent() {
        let mut v = Voice::new(CONFIRM, 1.0, 1.0, (1.0, 1.0), 1.0, 1);
        let mut buf = vec![0.0; 2 * 10];
        v.render(&mut buf, 48_000.0);
        // Only the first blip has started; the second waits 70 ms.
        assert!(buf.iter().all(|x| x.abs() < 0.2));
    }
}
