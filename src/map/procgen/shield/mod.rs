//! Shield generators: each side has its own standard model, put up the
//! same on every world it holds, whoever built the place round it. A city
//! the Imperium built can have an Ithessar generator in it: the city was
//! taken, and the new owners installed their own.
//!
//! Every model keeps to the skeleton's landmark, as the style's monument
//! would: a ring round it (a wall of some kind) open on each door, the
//! generator in the middle on a plinth no wider than `PLINTH_R`, its plant
//! between the doors inside the ring and out on the precinct. The side that
//! fell in (war damage) is shelled: broken down and heaped with rubble.
//!
//! - Imperium (`imperium::shield`): a steel mast in a concrete blast wall,
//!   banded in blue, a crown of gold vanes; capacitor banks, transformers,
//!   pylons strung with cable.
//! - Resonant (`resonant`): a crystal held in a gilt reliquary spire, ivory
//!   plates floating round it on light; banks of organ pipes, bells in
//!   gilt frames, shrines of cut crystal.
//! - Uthenn (`uthenn`): a twisting stack of tungsten cubes, a glowing cube
//!   balanced on its corner over it, fed by green lasers from emitter
//!   stacks; a wall of cubes, stockpiles.
//! - Ithessar (`ithessar`): great crystals of their dark glass leaning in
//!   to hold a ball of plasma high over a dais; plasma running in channels
//!   in the floor, crystal obelisks round the precinct.

mod ithessar;
mod resonant;
mod ruin;
mod uthenn;

pub use ruin::Condition;
pub(super) use ruin::{Mark, ruin};

use std::f32::consts::{PI, TAU};

use glam::{Vec2, Vec3};

use super::skeleton::*;
use crate::faction::Faction;
use crate::map::ruins::Drift;
use crate::map::{GlowPath, Map, OBox, rocks};
use crate::render::material::Material;
use crate::rng::Rng;

/// The plinth under every model: nothing at the middle wider than this.
pub const PLINTH_R: f32 = 6.5;

/// The generator `owner` puts up, in the landmark's place.
pub(super) fn build(map: &mut Map, sk: &Skeleton, rng: &mut Rng, owner: Faction) -> Vec<Drift> {
    let lm = &sk.landmarks[0];
    build_at(map, lm, sk.ground(lm.center), rng, owner)
}

/// The generator `owner` puts up on `lm`, its floor at `base`: in a city's
/// landmark, or on any biome's slot shaped as one (the desert's).
pub(super) fn build_at(
    map: &mut Map,
    lm: &Landmark,
    base: f32,
    rng: &mut Rng,
    owner: Faction,
) -> Vec<Drift> {
    match owner {
        // The Company holds no ground; a generator it fights over is
        // someone else's, and the Imperium's is the commonest.
        Faction::Imperium | Faction::Company => map.imperium_generator(rng, lm, base),
        Faction::Resonant => map.resonant_generator(rng, lm, base),
        Faction::Uthenn => map.uthenn_generator(rng, lm, base),
        Faction::Ithessar => map.ithessar_generator(rng, lm, base),
    }
}

/// The angle of a direction on the ground plane.
pub(super) fn angle(d: Vec2) -> f32 {
    d.y.atan2(d.x)
}

/// How far round from `a` to `b`, radians, either way (0..PI).
pub(super) fn apart(a: f32, b: f32) -> f32 {
    ((a - b + PI).rem_euclid(TAU) - PI).abs()
}

/// The direction at angle `a`.
pub(super) fn dir(a: f32) -> Vec2 {
    Vec2::new(a.cos(), a.sin())
}

/// Is angle `a` round the middle on the side that was shelled?
pub(super) fn shelled(lm: &Landmark, a: f32) -> bool {
    lm.fallen.is_some_and(|f| apart(a, angle(f)) < 0.6)
}

/// Is angle `a` round the middle in a doorway (the ring stays open there)?
pub(super) fn in_door(lm: &Landmark, a: f32) -> bool {
    lm.doors.iter().any(|&d| apart(a, angle(d)) < 0.24)
}

/// The angles halfway between each door and the next round the middle:
/// where each model puts its plant.
pub(super) fn between_doors(lm: &Landmark) -> Vec<f32> {
    let mut doors: Vec<f32> = lm.doors.iter().map(|&d| angle(d).rem_euclid(TAU)).collect();
    doors.sort_by(f32::total_cmp);
    (0..doors.len())
        .map(|k| {
            let a0 = doors[k];
            let mut a1 = doors[(k + 1) % doors.len()];
            if a1 <= a0 {
                a1 += TAU;
            }
            (a0 + a1) * 0.5
        })
        .collect()
}

/// The ring's pieces: (angle, half its width along the ring) for each of
/// `segs` round `radius`, skipping the doors.
pub(super) fn ring(lm: &Landmark, radius: f32, segs: usize) -> Vec<(f32, f32)> {
    let half = radius * PI / segs as f32 * 1.02;
    (0..segs)
        .map(|k| (k as f32 + 0.5) / segs as f32 * TAU)
        .filter(|&a| !in_door(lm, a))
        .map(|a| (a, half))
        .collect()
}

/// Spots round the precinct for its smaller pieces (pylons, obelisks):
/// `n` round, bar those near a door or on the shelled side.
pub(super) fn precinct_spots(lm: &Landmark, n: usize) -> Vec<Option<f32>> {
    (0..n)
        .map(|k| (k as f32 + 0.5) / n as f32 * TAU)
        .map(|a| {
            let near_door = lm.doors.iter().any(|&d| apart(a, angle(d)) < 0.3);
            (!near_door && !shelled(lm, a)).then_some(a)
        })
        .collect()
}

/// A glowing channel in the floor along the ray at angle `a`, from `r0` out
/// to `r1` from the middle.
pub(super) fn channel(
    lm: &Landmark,
    a: f32,
    r0: f32,
    r1: f32,
    y: f32,
    w: f32,
    color: Vec3,
) -> GlowPath {
    let n = (((r1 - r0).abs() / 4.0).ceil() as usize).max(1);
    GlowPath {
        points: (0..=n)
            .map(|k| {
                let q = lm.center + dir(a) * (r0 + (r1 - r0) * k as f32 / n as f32);
                Vec3::new(q.x, y + 0.04, q.y)
            })
            .collect(),
        widths: vec![w; n + 1],
        color,
    }
}

impl Map {
    /// A box standing at `c`, `half` across, from `y0` to `y1`, turned
    /// `yaw` (its local x along the direction at angle `-yaw`).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn block(
        &mut self,
        c: Vec2,
        half: Vec2,
        yaw: f32,
        (y0, y1): (f32, f32),
        color: Vec3,
        material: Material,
        solid: bool,
    ) -> &mut OBox {
        self.oboxes.push(OBox {
            center: c,
            yaw,
            half,
            y0,
            y1,
            color,
            side_mat: material,
            top_mat: material,
            solid,
            ..OBox::default()
        });
        self.oboxes.last_mut().unwrap()
    }

    /// Where it was shelled: rubble banked against the broken ring and
    /// chunks of it, `color`, strewn about inside and out.
    pub(super) fn generator_rubble(
        &mut self,
        rng: &mut Rng,
        lm: &Landmark,
        base: f32,
        color: Vec3,
    ) -> Vec<Drift> {
        let Some(fall) = lm.fallen else {
            return Vec::new();
        };
        let mut drifts = Vec::new();
        for (side, sign) in [(-1.0f32, 1.0f32), (1.0, -1.0)] {
            drifts.push(Drift {
                b: OBox {
                    center: lm.center + fall * (lm.radius + side * 3.5),
                    yaw: -angle(fall),
                    half: Vec2::new(9.0, 2.0),
                    y0: 0.0,
                    y1: 1.0,
                    ..OBox::default()
                },
                axis: 1,
                sign,
                height: 1.3,
                run: 7.0,
            });
        }
        for _ in 0..24 {
            let a = angle(fall) + rng.range(-0.6, 0.6);
            let d = rng.range(PLINTH_R + 3.0, lm.radius + 12.0);
            let p = lm.center + dir(a) * d;
            let s = rng.range(0.7, 2.2);
            let half = Vec2::new(s, s * rng.range(0.4, 0.9));
            let top = base + s * rng.range(0.3, 0.7);
            // None that blocks lands in a doorway.
            let in_way = lm.doors.iter().any(|&d| {
                let q = p - lm.center;
                q.dot(d) > 0.0 && q.perp_dot(d).abs() < s + 2.5
            });
            self.oboxes.push(OBox {
                tilt: rng.range(-0.4, 0.4),
                roll: rng.range(-0.3, 0.3),
                solid: s > 1.5 && !in_way,
                ..rocks::rubble(
                    p,
                    rng.range(0.0, TAU),
                    half,
                    base - 0.3,
                    top,
                    color * rng.range(0.85, 1.0),
                )
            });
        }
        drifts
    }
}
