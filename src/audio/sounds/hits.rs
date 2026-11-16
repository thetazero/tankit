//! Rounds landing: on walls and the ground, off walls, on armour, and the
//! tank that blows up.

use glam::Vec3;

use super::Play;
use crate::audio::OWN_HULL;
use crate::audio::voice::{Filt, L, Layer, Src, noise, sine};

/// A round spent on a wall or the ground.
pub fn impact(at: Vec3, plasma: bool, jitter: f32) -> Play {
    let layers = if plasma { SPLASH } else { IMPACT };
    Play::at(layers, at, 0.8, jitter)
}

/// An orb skipping off a wall.
pub fn bounce(at: Vec3, jitter: f32) -> Play {
    Play::at(BOUNCE, at, 0.8, jitter)
}

/// A round striking a tank. Your own hull ringing is louder: you're inside it.
pub fn hit(at: Vec3, victim: usize, jitter: f32) -> Play {
    let g = if victim == 0 { OWN_HULL } else { 1.0 };
    Play::at(HIT, at, g, jitter)
}

pub fn kill(at: Vec3, jitter: f32) -> Play {
    Play::at(KILL, at, 1.3, jitter)
}

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

/// A round striking armour: one crunch. All noise, no pitched ring: any
/// tone that hangs on, however low, reads as a ping off a can.
pub const HIT: &[Layer] = &[
    // A thump too short to have a pitch.
    sine(110.0, 45.0, 0.03, 0.06, 0.6),
    // The blow: low-passed noise that closes fast.
    noise(Filt::Low, 1800.0, 250.0, 0.04, 0.1, 1.3),
    Layer {
        q: 0.6,
        ..noise(Filt::Band, 500.0, 250.0, 0.08, 0.12, 0.8)
    },
    // The crunch: a dense burst of grinding clicks.
    Layer {
        src: Src::Crackle(2500.0),
        filt: Filt::Band,
        cutoff: (1200.0, 600.0),
        sweep: 0.08,
        q: 0.8,
        decay: 0.09,
        gain: 0.5,
        ..L
    },
    // Grit settling after it.
    Layer {
        src: Src::Crackle(600.0),
        filt: Filt::Band,
        cutoff: (2000.0, 900.0),
        q: 1.0,
        decay: 0.18,
        gain: 0.3,
        delay: 0.01,
        ..L
    },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::testkit::{band, sane};
    use crate::audio::voice::Voice;

    #[test]
    fn every_sound_is_sane() {
        sane(&[
            ("impact", IMPACT),
            ("splash", SPLASH),
            ("bounce", BOUNCE),
            ("hit", HIT),
            ("kill", KILL),
        ]);
    }

    /// Getting hit sounded like a tin can: long bright partials above 1 kHz.
    /// A round hitting thick armour is a low crunch that dies quickly.
    #[test]
    fn hits_crunch_not_ping() {
        let sr = 48_000.0;
        let mut v = Voice::new(HIT, 1.0, 1.0, (1.0, 0.0), 1.0, 7);
        let mut buf = vec![0.0; 2 * (crate::audio::voice::length(HIT) * sr) as usize];
        v.render(&mut buf, sr);
        let x: Vec<f32> = buf.iter().step_by(2).copied().collect();
        let body = band(&x, 100.0, 900.0, sr);
        let ping = band(&x, 1000.0, 6000.0, sr);
        assert!(body > 3.0 * ping, "body {body:.2}, ping {ping:.2}");
        // No bright ring hanging on after the strike.
        let tail = band(&x[(0.15 * sr) as usize..], 1000.0, 6000.0, sr);
        assert!(tail < 0.02 * ping, "rings on: {:.3}", tail / ping);
        // A crunch, not a ping: no pitched layer lasts long enough to hear
        // as a note.
        for l in HIT {
            if l.src == Src::Sine {
                assert!(l.decay <= 0.06, "a tone rings for {} s", l.decay);
            }
        }
    }
}
