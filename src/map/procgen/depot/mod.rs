//! The rail depot biome: a freight yard on a desert world. Parallel sidings
//! with lanes to fight down between rows of standing cars, gaps through the
//! rows, gantries over them, sheds and water towers round them (`plan`).
//! It's built of a foundry's parts, in its builder's style.

mod plan;
#[cfg(test)]
mod tests;

use glam::Vec3;

use super::Spec;
use super::foundry::{self, Ground};
use super::skeleton::Skeleton;
use crate::map::Map;
use crate::rng::Rng;

/// The depot `spec` lays out: the first, from its seed on, whose greybox is
/// playable.
pub fn skeleton(spec: &Spec) -> Skeleton {
    let mut last = None;
    for attempt in 0..16u64 {
        let mut rng =
            Rng::new(spec.seed.wrapping_mul(0x9E37_79B9).wrapping_add(attempt) ^ 0x000D_E907);
        let sk = plan::skeleton(&mut rng);
        match super::check::playable(&sk.greybox(&spec.sides)) {
            Ok(()) => return sk,
            Err(why) => log::info!("depot seed {} attempt {attempt}: {why}", spec.seed),
        }
        last = Some(sk);
    }
    last.expect("no depot laid out")
}

/// `spec`'s depot undressed.
pub fn greybox(spec: &Spec) -> Map {
    let sk = skeleton(spec);
    let mut m = sk.greybox(&spec.sides);
    if let Some(t) = &mut m.terrain {
        sk.paint(t, Vec3::splat(0.4));
    }
    m.raid_stands = sk.stands.clone();
    m.atmosphere = Ground::Desert.atmosphere(&mut Rng::new(spec.seed ^ 0x5EED_D2E5));
    m
}

/// Builds `spec`'s depot.
pub fn build(spec: &Spec) -> Map {
    let sk = skeleton(spec);
    let mut m = foundry::dressed(spec, Ground::Desert, &sk);
    m.hold_name = "RAIL YARD";
    m.raid_stands = sk.stands.clone();
    m
}
