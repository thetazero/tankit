//! The tungsten hoard biome: where the Uthenn stash their cubes, on desert,
//! tundra or grassland (a seed's the same hoard on any). Stacks of cubes on
//! a strict grid, lanes between them, round a raised counting floor
//! (`plan`). It's the Uthenn's whoever holds it now, built of a foundry's
//! parts in their style.

mod plan;
#[cfg(test)]
mod tests;

use glam::Vec3;

use super::Spec;
use super::foundry::{self, Ground};
use super::memo::{Memo, SKELETONS};
use super::skeleton::Skeleton;
use crate::map::Map;
use crate::rng::Rng;

/// The hoard `spec` lays out: the first, from its seed on, whose greybox is
/// playable. Worked out only the first time it's asked for (the last few are kept).
pub fn skeleton(spec: &Spec) -> Skeleton {
    static KEPT: Memo<Spec, Skeleton, SKELETONS> = Memo::new();
    KEPT.get(spec, || lay_out(spec))
}

fn lay_out(spec: &Spec) -> Skeleton {
    let mut last = None;
    for attempt in 0..16u64 {
        let mut rng =
            Rng::new(spec.seed.wrapping_mul(0x9E37_79B9).wrapping_add(attempt) ^ 0x0040_A2D5);
        let sk = plan::skeleton(&mut rng);
        match super::check::playable(&sk.greybox(&spec.sides)) {
            Ok(()) => return sk,
            Err(why) => log::info!("hoard seed {} attempt {attempt}: {why}", spec.seed),
        }
        last = Some(sk);
    }
    last.expect("no hoard laid out")
}

/// `spec`'s hoard undressed.
pub fn greybox(spec: &Spec, ground: Ground) -> Map {
    let sk = skeleton(spec);
    let mut m = sk.greybox(&spec.sides);
    if let Some(t) = &mut m.terrain {
        sk.paint(t, Vec3::splat(0.4));
    }
    m.atmosphere = ground.atmosphere(&mut Rng::new(spec.seed ^ 0x5EED_D2E5));
    m
}

/// Builds `spec`'s hoard.
pub fn build(spec: &Spec, ground: Ground) -> Map {
    let mut m = foundry::dressed(spec, ground, &skeleton(spec));
    m.hold_name = "COUNTING FLOOR";
    m
}
