//! Every sound the game makes, one file per family: the thing in the game
//! that makes it (a gun, the beam, a Vigil, the Hold site, the menus...).
//! A family file holds all of its sound: its one-shot patches, how a match
//! event picks one, any loop it runs on a tank, and its tests and demos.
//!
//! # Adding a sound
//!
//! Put it in the family it belongs to, or start a new file here for a new
//! family (and add its `mod` line below, in alphabetical order). Inside the
//! family:
//!
//! - **A one-shot** is a `pub const NAME: &[Layer]` patch (see
//!   [`super::voice`] for the layer helpers), and a `pub fn` that turns the
//!   event's fields into a [`Play`]. Hook it up with one arm in
//!   `Audio::one_shot` (`audio/mod.rs`), next to the family's other arms.
//!   Add the patch to the family's `every_sound_is_sane` test.
//! - **A loop** (a sound that runs while something lasts) is a struct that
//!   implements [`super::tank::Loop`], fed from a field of
//!   [`super::tank::TankParams`]: add the field, set it in `tank::params`,
//!   and add the loop to `tank::Loops` and `Loops::each`. Sounds not tied to
//!   a tank (like [`wind`]) render straight into the mixer's world bus.
//! - **Tests and `--ignored` demos that write `docs/audio/*.wav`** go in the
//!   family's own `tests` module, using `audio::testkit`.
//!
//! The shared places (`one_shot`'s match, `TankParams`, `Loops`, the list
//! below) each take one line per sound; keep those lines with their family
//! (or in alphabetical order) rather than all at the end, so two sound PRs
//! touch different lines and merge cleanly.

pub mod arrival;
pub mod beam;
pub mod engine;
pub mod guns;
pub mod hits;
pub mod hold;
pub mod hull;
pub mod repair;
pub mod shield;
pub mod tracks;
pub mod turret;
pub mod ui;
pub mod vigil;
pub mod wind;

use glam::Vec3;

use super::voice::Layer;

/// A one-shot a match event calls for: its patch, where it sounds (`None`:
/// the same wherever you are, like the HUD), how loud, and its pitch.
pub struct Play {
    pub layers: &'static [Layer],
    pub at: Option<Vec3>,
    pub gain: f32,
    pub pitch: f32,
}

impl Play {
    /// Placed in the world at `at`.
    pub fn at(layers: &'static [Layer], at: Vec3, gain: f32, pitch: f32) -> Play {
        Play {
            layers,
            at: Some(at),
            gain,
            pitch,
        }
    }

    /// Heard the same wherever you are, at full gain and pitch.
    pub fn everywhere(layers: &'static [Layer]) -> Play {
        Play {
            layers,
            at: None,
            gain: 1.0,
            pitch: 1.0,
        }
    }
}
