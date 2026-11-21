//! A tank arriving (spawn or respawn), each faction its own way.

use glam::Vec3;

use super::Play;
use crate::audio::voice::{Filt, L, Layer, Src, noise, ring, sine};
use crate::faction::Faction;

/// Your own arrival is heard close up, as if from inside the hull.
pub fn arrival(tank: usize, faction: Faction, at: Vec3, jitter: f32) -> Play {
    let layers = match faction {
        Faction::Imperium | Faction::Company => DROP_IMPERIUM,
        Faction::Resonant => KINDLE_RESONANT,
        Faction::Uthenn => BUILD_UTHENN,
        Faction::Ithessar => DESCEND_ITHESSAR,
    };
    let g = if tank == 0 { 1.4 } else { 1.0 };
    Play::at(layers, at, g, jitter)
}

/// Imperium arrival, sent down from orbit: a roar falling out of the sky
/// down the lance (0.4 s), the slam of touchdown, and a brass chord as the
/// emblem burns on the ground.
pub const DROP_IMPERIUM: &[Layer] = &[
    Layer {
        attack: 0.3,
        q: 1.5,
        ..noise(Filt::Band, 4500.0, 600.0, 0.2, 0.08, 0.35)
    },
    Layer {
        attack: 0.35,
        ..sine(1600.0, 300.0, 0.18, 0.05, 0.1)
    },
    Layer {
        delay: 0.4,
        ..sine(90.0, 30.0, 0.12, 0.45, 0.7)
    },
    Layer {
        delay: 0.4,
        ..noise(Filt::Low, 5000.0, 180.0, 0.18, 0.5, 0.55)
    },
    Layer {
        delay: 0.4,
        attack: 0.04,
        ..noise(Filt::Low, 400.0, 120.0, 0.6, 0.8, 0.35)
    },
    // The emblem's chord: a fifth and an octave, brassy with a little edge.
    Layer {
        delay: 0.5,
        attack: 0.08,
        ..sine(220.0, 220.0, 1.0, 0.5, 0.12)
    },
    Layer {
        delay: 0.5,
        attack: 0.08,
        ..sine(330.0, 330.0, 1.0, 0.45, 0.1)
    },
    Layer {
        delay: 0.5,
        attack: 0.08,
        ..sine(440.0, 440.0, 1.0, 0.4, 0.08)
    },
    Layer {
        delay: 0.5,
        attack: 0.08,
        q: 2.0,
        ..noise(Filt::Band, 1300.0, 1300.0, 0.1, 0.35, 0.05)
    },
];

/// Resonant arrival, the vessel kindling: a glassy shimmer gathering as the
/// motes spiral in, then a bell's toll at 0.7 s and its echo 0.3 s later.
pub const KINDLE_RESONANT: &[Layer] = &[
    Layer {
        attack: 0.6,
        ..sine(2400.0, 3600.0, 0.5, 0.06, 0.05)
    },
    Layer {
        attack: 0.6,
        ..sine(3100.0, 4700.0, 0.5, 0.06, 0.035)
    },
    Layer {
        attack: 0.6,
        q: 6.0,
        ..noise(Filt::Band, 5000.0, 7000.0, 0.5, 0.06, 0.06)
    },
    // The toll: a church bell's partials (hum, prime, minor third, fifth, nominal).
    Layer {
        delay: 0.7,
        ..ring(196.0, 1.3, 0.3)
    },
    Layer {
        delay: 0.7,
        ..ring(392.0, 1.0, 0.3)
    },
    Layer {
        delay: 0.7,
        ..ring(470.0, 0.8, 0.18)
    },
    Layer {
        delay: 0.7,
        ..ring(588.0, 0.6, 0.14)
    },
    Layer {
        delay: 0.7,
        ..ring(784.0, 0.45, 0.12)
    },
    Layer {
        delay: 0.7,
        ..ring(1140.0, 0.25, 0.08)
    },
    Layer {
        delay: 0.7,
        ..noise(Filt::High, 3000.0, 3000.0, 0.1, 0.01, 0.25)
    },
    // The echo, softer and duller.
    Layer {
        delay: 1.0,
        ..ring(196.0, 1.0, 0.12)
    },
    Layer {
        delay: 1.0,
        ..ring(392.0, 0.8, 0.12)
    },
    Layer {
        delay: 1.0,
        ..ring(588.0, 0.4, 0.05)
    },
];

/// Uthenn arrival, the hull made: a laser frame's hum, tungsten cubes
/// clunking into the volume, and the scan plane's rising sweep (0.9 s).
pub const BUILD_UTHENN: &[Layer] = &[
    Layer {
        attack: 0.05,
        ..sine(120.0, 120.0, 1.0, 0.5, 0.12)
    },
    Layer {
        attack: 0.05,
        q: 8.0,
        ..noise(Filt::Band, 240.0, 240.0, 1.0, 0.45, 0.12)
    },
    // Cubes landing: dense metal blocks, a dull knock with a short ring.
    Layer {
        delay: 0.1,
        ..sine(180.0, 90.0, 0.03, 0.06, 0.45)
    },
    Layer {
        delay: 0.1,
        ..ring(1900.0, 0.05, 0.08)
    },
    Layer {
        delay: 0.19,
        ..sine(210.0, 100.0, 0.03, 0.05, 0.35)
    },
    Layer {
        delay: 0.19,
        ..ring(2300.0, 0.04, 0.07)
    },
    Layer {
        delay: 0.26,
        ..sine(160.0, 80.0, 0.03, 0.06, 0.4)
    },
    Layer {
        delay: 0.33,
        ..sine(200.0, 95.0, 0.03, 0.05, 0.35)
    },
    Layer {
        delay: 0.33,
        ..ring(2100.0, 0.04, 0.07)
    },
    Layer {
        delay: 0.41,
        ..sine(170.0, 85.0, 0.03, 0.06, 0.4)
    },
    // The scan plane rising up through the hull.
    Layer {
        attack: 0.8,
        ..sine(300.0, 1500.0, 0.6, 0.08, 0.1)
    },
    Layer {
        attack: 0.8,
        ..sine(600.0, 3000.0, 0.6, 0.08, 0.04)
    },
    // Finished: a clean two-note chirp.
    Layer {
        delay: 0.92,
        ..sine(1500.0, 1500.0, 1.0, 0.05, 0.1)
    },
    Layer {
        delay: 1.0,
        ..sine(2000.0, 2000.0, 1.0, 0.08, 0.1)
    },
];

/// Ithessar arrival, lowered on plasma: the orb opening with a rising hum,
/// the stream's fizz as the tank floats down, then the orb's burst at 0.9 s.
pub const DESCEND_ITHESSAR: &[Layer] = &[
    Layer {
        attack: 0.3,
        ..sine(110.0, 220.0, 0.3, 0.5, 0.25)
    },
    Layer {
        attack: 0.3,
        ..sine(330.0, 450.0, 0.3, 0.4, 0.08)
    },
    Layer {
        attack: 0.4,
        q: 2.5,
        ..noise(Filt::Band, 1800.0, 2600.0, 0.5, 0.35, 0.12)
    },
    // The burst: a pop and a splash of droplets.
    Layer {
        delay: 0.9,
        ..sine(700.0, 140.0, 0.06, 0.15, 0.45)
    },
    Layer {
        delay: 0.9,
        q: 1.5,
        ..noise(Filt::Band, 4000.0, 900.0, 0.1, 0.2, 0.45)
    },
    Layer {
        src: Src::Crackle(220.0),
        filt: Filt::Band,
        cutoff: (5000.0, 2500.0),
        sweep: 0.3,
        q: 2.0,
        decay: 0.18,
        gain: 0.3,
        delay: 0.95,
        ..L
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::testkit::sane;

    #[test]
    fn every_sound_is_sane() {
        sane(&[
            ("drop_imperium", DROP_IMPERIUM),
            ("kindle_resonant", KINDLE_RESONANT),
            ("build_uthenn", BUILD_UTHENN),
            ("descend_ithessar", DESCEND_ITHESSAR),
        ]);
    }
}
