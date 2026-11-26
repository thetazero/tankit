//! The hull foundry biome: a works that casts hulls for the war, on desert,
//! tundra or grassland (`Ground`): the same works, other land. Furnace halls in a row, a rail yard and a stockyard under
//! gantries on one side of them, slag heaps on the other. Laid out as a
//! skeleton (`skeleton`), checked for play on its greybox (one that fails
//! is laid out again), then dressed (`dress`) in its builder's style
//! (`styles`): each faction has its own.
//!
//! Every one's laid out the same way round and the map turned and mirrored
//! by its seed when it's done.

mod dress;
mod ground;
mod imperium;
mod ithessar;
mod parts;
mod plan;
mod resonant;
mod skeleton;
mod style;
#[cfg(test)]
mod tests;
mod uthenn;

use glam::{Vec2, Vec3};

pub use ground::Ground;
pub use parts::{Part, build as build_part};
pub use skeleton::Skeleton;

use super::{Look, Spec};
use crate::ai::nav::NavGrid;
use crate::faction::Faction;
use crate::map::Map;
use crate::render::material::Material;
use crate::rng::Rng;
use style::Works;

fn works(look: Look) -> &'static dyn Works {
    match look {
        Look::Imperium => &imperium::Imperium,
        Look::Resonant => &resonant::Resonant,
        Look::Uthenn => &uthenn::Uthenn,
        Look::Ithessar => &ithessar::Ithessar,
    }
}

/// The skeleton `spec` lays out: the first, from its seed on, whose greybox
/// is playable, and fair to both sides.
pub fn skeleton(spec: &Spec) -> Skeleton {
    let mut last = None;
    for attempt in 0..16u64 {
        let mut rng =
            Rng::new(spec.seed.wrapping_mul(0x9E37_79B9).wrapping_add(attempt) ^ 0xF0_0D2E);
        let sk = Skeleton::generate(&mut rng);
        let grey = greybox_of(&sk, &spec.sides);
        let nav = NavGrid::build(&grey);
        match super::check::playable_on(&grey, &nav).and_then(|()| fair(&sk, &nav)) {
            Ok(()) => return sk,
            Err(why) => log::debug!("foundry seed {} attempt {attempt}: {why}", spec.seed),
        }
        last = Some(sk);
    }
    last.expect("no foundry laid out")
}

/// Whether neither side's drive to the casting floor is much longer.
fn fair(sk: &Skeleton, nav: &NavGrid) -> Result<(), String> {
    let [a, b] = sk.bases.map(|from| {
        let mut at = from;
        let mut len = 0.0;
        for p in nav.path(from, sk.hold) {
            len += at.distance(p);
            at = p;
        }
        len
    });
    if (a - b).abs() / a.max(b).max(1.0) > 0.15 {
        return Err(format!("unfair: {a:.0} m against {b:.0} m"));
    }
    Ok(())
}

/// A blank map for the skeleton, its spawns laid.
fn blank(sk: &Skeleton, sides: &[Faction]) -> Map {
    let mut map = Map::empty(sk.size);
    map.factions = sides.to_vec();
    map.bases = Map::no_bases(sides);
    for side in 0..sk.bases.len().min(sides.len()) {
        map.bases[side] = sk.spawn_points(side);
    }
    map.open_spots = Vec::new();
    map.spawn_yaw = None;
    map.hold_site = Some(sk.hold);
    map
}

fn greybox_of(sk: &Skeleton, sides: &[Faction]) -> Map {
    let mut map = blank(sk, sides);
    let mut t = sk.terrain();
    t.color = Vec3::splat(0.3);
    t.material = Material::Concrete;
    map.terrain = Some(t);
    sk.boxes(&mut map);
    map.open_spots = map.bases.concat();
    map.open_spots.push(sk.hold);
    map
}

/// The works `spec` lays out: the first, from its seed on, whose greybox is
/// playable.
pub fn works_plan(spec: &Spec) -> super::skeleton::Skeleton {
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
    let sk = works_plan(spec);
    let mut m = sk.greybox(&spec.sides);
    if let Some(t) = &mut m.terrain {
        sk.paint(t, Vec3::splat(0.4));
    }
    m.atmosphere = ground.atmosphere(&mut Rng::new(spec.seed ^ 0x5EED_D2E5));
    m
}

/// Turns a map laid out to the skeleton the way round `seed` has it.
fn orient(seed: u64, m: &mut Map) {
    let mut rng = Rng::new(seed ^ 0x0012_7A4D);
    if rng.chance(0.5) {
        m.mirror_x();
    }
    for _ in 0..rng.index(4) {
        m.turn();
    }
}

/// Builds `spec`'s foundry.
pub fn build(spec: &Spec, ground: Ground) -> Map {
    let mut m = dressed(spec, ground, &skeleton(spec));
    orient(spec.seed, &mut m);
    m
}

/// The foundry `sk` lays out, dressed, the way round it's laid out.
fn dressed(spec: &Spec, ground: Ground, sk: &Skeleton) -> Map {
    let mut rng = Rng::new(spec.seed ^ 0x5EED_D2E5);
    let st = works(spec.look);
    let mut m = blank(sk, &spec.sides);
    m.atmosphere = ground.atmosphere(&mut rng);
    m.planet = Some((spec.seed, ground.planet()));
    m.hold_name = "CASTING FLOOR";
    dress::dress(&mut m, sk, st, ground, &mut rng);
    m
}
