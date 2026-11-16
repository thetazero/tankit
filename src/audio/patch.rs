//! One-shot sounds: each a stack of short synth layers (a swept oscillator or
//! filtered noise under a decay envelope). Shots, impacts, hits, blasts, UI.

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
#[derive(Clone, Copy)]
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

const L: Layer = Layer {
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

const fn sine(f0: f32, f1: f32, glide: f32, decay: f32, gain: f32) -> Layer {
    Layer {
        freq: (f0, f1),
        glide,
        decay,
        gain,
        ..L
    }
}

const fn noise(filt: Filt, c0: f32, c1: f32, sweep: f32, decay: f32, gain: f32) -> Layer {
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
const fn ring(f: f32, decay: f32, gain: f32) -> Layer {
    sine(f, f * 0.98, 0.4, decay, gain)
}

/// Imperium rail cannon: a deep slam, a sharp crack and a rail's zing.
pub const LANCE: &[Layer] = &[
    sine(120.0, 40.0, 0.06, 0.2, 0.6),
    noise(Filt::Low, 6000.0, 500.0, 0.12, 0.24, 0.85),
    Layer {
        q: 1.0,
        ..noise(Filt::Band, 1100.0, 700.0, 0.1, 0.15, 0.6)
    },
    noise(Filt::High, 2500.0, 2500.0, 0.1, 0.02, 0.7),
    sine(3400.0, 900.0, 0.1, 0.14, 0.12),
    Layer {
        attack: 0.01,
        ..noise(Filt::Low, 420.0, 110.0, 0.4, 0.6, 0.4)
    },
];

/// Imperium twin autocannon: shorter, punchier.
pub const MAUL: &[Layer] = &[
    sine(150.0, 55.0, 0.04, 0.1, 0.6),
    noise(Filt::Low, 5000.0, 600.0, 0.06, 0.11, 0.7),
    Layer {
        q: 1.2,
        ..noise(Filt::Band, 1400.0, 900.0, 0.06, 0.08, 0.45)
    },
    noise(Filt::High, 3000.0, 3000.0, 0.1, 0.012, 0.45),
    noise(Filt::Low, 320.0, 160.0, 0.2, 0.28, 0.22),
];

/// Sorn orb launcher: a falling, hollow "thoom".
pub const WISP: &[Layer] = &[
    sine(900.0, 170.0, 0.12, 0.25, 0.42),
    sine(1350.0, 255.0, 0.12, 0.18, 0.18),
    Layer {
        q: 3.0,
        ..noise(Filt::Band, 1800.0, 500.0, 0.1, 0.17, 0.3)
    },
    sine(75.0, 60.0, 0.1, 0.15, 0.4),
];

/// Sorn three-barrel: quick plasma darts.
pub const TRIAD: &[Layer] = &[
    sine(1400.0, 520.0, 0.05, 0.07, 0.28),
    Layer {
        q: 4.0,
        ..noise(Filt::Band, 2600.0, 900.0, 0.05, 0.05, 0.18)
    },
    sine(110.0, 80.0, 0.05, 0.05, 0.2),
];

/// A shell landing on a wall or the ground: thud and debris.
pub const IMPACT: &[Layer] = &[
    sine(95.0, 45.0, 0.05, 0.1, 0.6),
    noise(Filt::Low, 2600.0, 500.0, 0.06, 0.1, 0.55),
    Layer {
        src: Src::Crackle(380.0),
        filt: Filt::Band,
        cutoff: (3000.0, 2200.0),
        q: 1.5,
        decay: 0.22,
        gain: 0.35,
        delay: 0.02,
        ..L
    },
];

/// Plasma splashing on a surface: a fizz.
pub const SPLASH: &[Layer] = &[
    Layer {
        q: 2.0,
        ..noise(Filt::Band, 3200.0, 800.0, 0.08, 0.15, 0.4)
    },
    sine(600.0, 150.0, 0.08, 0.12, 0.28),
];

/// An orb skipping off a wall.
pub const BOUNCE: &[Layer] = &[
    sine(1800.0, 700.0, 0.08, 0.12, 0.24),
    noise(Filt::High, 4000.0, 4000.0, 0.1, 0.02, 0.18),
];

/// A round striking armour: a clang over a thud.
pub const HIT: &[Layer] = &[
    ring(420.0, 0.32, 0.3),
    ring(1088.0, 0.24, 0.24),
    ring(1730.0, 0.17, 0.18),
    ring(2650.0, 0.11, 0.12),
    ring(3900.0, 0.07, 0.08),
    noise(Filt::High, 2000.0, 2000.0, 0.1, 0.015, 0.6),
    sine(130.0, 60.0, 0.05, 0.08, 0.5),
];

/// A tank blowing up.
pub const KILL: &[Layer] = &[
    sine(85.0, 28.0, 0.3, 0.5, 1.0),
    noise(Filt::Low, 6000.0, 200.0, 0.25, 0.55, 0.9),
    Layer {
        src: Src::Crackle(140.0),
        filt: Filt::Band,
        cutoff: (1600.0, 900.0),
        sweep: 0.6,
        q: 1.0,
        decay: 0.8,
        gain: 0.55,
        delay: 0.05,
        ..L
    },
    Layer {
        attack: 0.05,
        ..noise(Filt::Low, 320.0, 80.0, 1.0, 1.3, 0.5)
    },
    Layer {
        delay: 0.08,
        ..sine(230.0, 150.0, 0.6, 0.55, 0.12)
    },
];

/// Hull mended at a repair crate.
pub const REPAIR: &[Layer] = &[
    sine(660.0, 660.0, 1.0, 0.25, 0.22),
    Layer {
        delay: 0.09,
        ..sine(990.0, 990.0, 1.0, 0.35, 0.22)
    },
    Layer {
        delay: 0.18,
        ..sine(1320.0, 1320.0, 1.0, 0.5, 0.18)
    },
    ring(140.0, 0.12, 0.3),
];

/// The beam locks out: a steam hiss as the emitter powers down.
pub const OVERHEAT: &[Layer] = &[
    Layer {
        attack: 0.01,
        ..noise(Filt::High, 1500.0, 4500.0, 0.2, 0.5, 0.3)
    },
    sine(320.0, 110.0, 0.2, 0.25, 0.22),
];

/// Your shot killed someone.
pub const CONFIRM: &[Layer] = &[
    sine(880.0, 880.0, 1.0, 0.06, 0.16),
    Layer {
        delay: 0.07,
        ..sine(1320.0, 1320.0, 1.0, 0.12, 0.16)
    },
];

/// Remote link re-established in a fresh chassis.
pub const LINK: &[Layer] = &[
    Layer {
        attack: 0.05,
        ..sine(220.0, 440.0, 0.15, 0.3, 0.12)
    },
    Layer {
        attack: 0.05,
        delay: 0.05,
        ..sine(330.0, 660.0, 0.15, 0.3, 0.08)
    },
];

/// Menu cursor moved.
pub const TICK: &[Layer] = &[
    sine(1800.0, 1800.0, 1.0, 0.015, 0.09),
    noise(Filt::High, 5000.0, 5000.0, 0.1, 0.004, 0.05),
];

/// Menu item chosen.
pub const SELECT: &[Layer] = &[
    sine(1100.0, 1600.0, 0.03, 0.06, 0.12),
    sine(2200.0, 3200.0, 0.03, 0.03, 0.04),
];

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

    const ALL: [&[Layer]; 15] = [
        LANCE, MAUL, WISP, TRIAD, IMPACT, SPLASH, BOUNCE, HIT, KILL, REPAIR, OVERHEAT, CONFIRM,
        LINK, TICK, SELECT,
    ];

    /// Every sound renders finite, audible, bounded, and actually ends.
    #[test]
    fn every_sound_is_sane() {
        let sr = 48_000.0;
        for (i, layers) in ALL.iter().enumerate() {
            let mut v = Voice::new(layers, 1.0, 1.0, (1.0, 1.0), 1.0, 99);
            let frames = (v.len * sr) as usize + 64;
            let mut buf = vec![0.0; frames * 2];
            v.render(&mut buf, sr);
            assert!(v.done(), "sound {i} never ends");
            let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(buf.iter().all(|x| x.is_finite()), "sound {i} not finite");
            assert!(peak > 0.02, "sound {i} silent: {peak}");
            assert!(peak < 4.0, "sound {i} too hot: {peak}");
            assert!(v.len < 12.0, "sound {i} too long: {}", v.len);
        }
    }

    #[test]
    fn delayed_layers_start_silent() {
        let mut v = Voice::new(CONFIRM, 1.0, 1.0, (1.0, 1.0), 1.0, 1);
        let mut buf = vec![0.0; 2 * 10];
        v.render(&mut buf, 48_000.0);
        // Only the first blip has started; the second waits 70 ms.
        assert!(buf.iter().all(|x| x.abs() < 0.2));
    }
}
