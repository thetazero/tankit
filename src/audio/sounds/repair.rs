//! Repairs.

use glam::Vec3;

use super::Play;
use crate::audio::voice::{Layer, ring, sine};

/// A hull mended at a repair crate.
pub fn repair(at: Vec3) -> Play {
    Play::at(REPAIR, at, 1.0, 1.0)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::testkit::sane;

    #[test]
    fn every_sound_is_sane() {
        sane(&[("repair", REPAIR)]);
    }
}
