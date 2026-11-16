//! The menus, and the HUD's own sounds: a kill confirmed, the link back.

use super::Play;
use crate::audio::voice::{Filt, Layer, noise, sine};

/// UI sounds the app can trigger directly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ui {
    Move,
    Select,
    Back,
}

/// A menu sound's patch and pitch.
pub fn menu(sound: Ui) -> (&'static [Layer], f32) {
    match sound {
        Ui::Move => (TICK, 1.0),
        Ui::Select => (SELECT, 1.0),
        Ui::Back => (SELECT, 0.7),
    }
}

/// Your shot made a kill.
pub fn confirm() -> Play {
    Play::everywhere(CONFIRM)
}

/// Your link came back in a fresh chassis.
pub fn link() -> Play {
    Play::everywhere(LINK)
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::testkit::sane;

    #[test]
    fn every_sound_is_sane() {
        sane(&[
            ("tick", TICK),
            ("select", SELECT),
            ("confirm", CONFIRM),
            ("link", LINK),
        ]);
    }
}
