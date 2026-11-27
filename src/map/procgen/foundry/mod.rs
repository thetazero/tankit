//! The hull foundry biome: a works that casts hulls for the war, on desert,
//! tundra or grassland (`Ground`). Laid out on the shared skeleton and land
//! (`plan`): the land, the big buildings, railway and roads, the yard,
//! cover. Checked for play on its greybox, then dressed (`dress`) in its
//! builder's style: `parts` are what's solid, each style's pieces hang on
//! them.

mod dress;
pub(in crate::map) use self::dress::{growth, snow, snowdrift, tree, tuft};
mod ground;
mod imperium;
mod ithessar;
mod parts;
pub(super) mod plan;
mod resonant;
mod style;
#[cfg(test)]
mod tests;
mod uthenn;

use glam::Vec3;

use super::memo::{Memo, SKELETONS};
use super::skeleton::Skeleton;
use super::{Look, Spec};
use crate::map::Map;
use crate::rng::Rng;
pub use ground::Ground;
pub use parts::{Part, build as build_part};
use style::Works;

pub(super) fn works(look: Look) -> &'static dyn Works {
    match look {
        Look::Imperium => &imperium::Imperium,
        Look::Resonant => &resonant::Resonant,
        Look::Uthenn => &uthenn::Uthenn,
        Look::Ithessar => &ithessar::Ithessar,
    }
}

/// The works `spec` lays out: the first, from its seed on, whose greybox is
/// playable. Worked out only the first time it's asked for (the last few are kept).
pub fn skeleton(spec: &Spec) -> Skeleton {
    static KEPT: Memo<Spec, Skeleton, SKELETONS> = Memo::new();
    KEPT.get(spec, || lay_out(spec))
}

fn lay_out(spec: &Spec) -> Skeleton {
    let mut last = None;
    for attempt in 0..16u64 {
        let mut rng =
            Rng::new(spec.seed.wrapping_mul(0x9E37_79B9).wrapping_add(attempt) ^ 0xF0_0D2E);
        let sk = plan::skeleton(&mut rng, spec.look);
        match super::check::playable(&sk.greybox(&spec.sides)) {
            Ok(()) => return sk,
            Err(why) => log::info!("foundry seed {} attempt {attempt}: {why}", spec.seed),
        }
        last = Some(sk);
    }
    last.expect("no foundry laid out")
}

/// `spec`'s foundry undressed.
pub fn greybox(spec: &Spec, ground: Ground) -> Map {
    let sk = skeleton(spec);
    let mut m = sk.greybox(&spec.sides);
    if let Some(t) = &mut m.terrain {
        sk.paint(t, Vec3::splat(0.4));
    }
    m.atmosphere = ground.atmosphere(&mut Rng::new(spec.seed ^ 0x5EED_D2E5));
    m
}

/// Builds `spec`'s foundry.
pub fn build(spec: &Spec, ground: Ground) -> Map {
    dressed(spec, ground, &skeleton(spec))
}

/// What `sk` lays out, dressed as `spec`'s works.
pub(super) fn dressed(spec: &Spec, ground: Ground, sk: &Skeleton) -> Map {
    let mut rng = Rng::new(spec.seed ^ 0x5EED_D2E5);
    let mut m = sk.blank(&spec.sides);
    m.atmosphere = ground.atmosphere(&mut rng);
    m.planet = Some((spec.seed, ground.planet()));
    m.hold_name = "WORKS YARD";
    // (Its own dice: each ground's sky draws a different number.)
    let mut rng = Rng::new(spec.seed ^ 0x00D2_E55E);
    dress::dress(&mut m, sk, spec.look, ground, &mut rng);
    m
}
