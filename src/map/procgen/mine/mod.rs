//! The crystal mine biome: digs in a cliff wandering across a desert, tundra or
//! grassland (`Ground`), at whatever angle it and they lie. Laid out (`plan`) on the one `Skeleton`
//! and its solid land, dressed (`dress`) outside as a desert is and inside
//! each dig in the dig's own frame. The mining gear any mine has (`dig`)
//! and its railroads (`works`) are built wherever they're put.
//!
//! Nothing here is anyone's: whose mine it is (their lamps, their machine
//! in the bay, their freight, their marker by their spawns) is the style's
//! (`MineStyle`: `imperium`, `resonant`, `uthenn`, `ithessar`; `kit` is what
//! they share).

mod dig;
pub mod dress;
mod imperium;
mod ithessar;
mod kit;
pub(in crate::map::procgen) mod land;
pub mod plan;
mod resonant;
mod uthenn;
mod works;

use std::f32::consts::PI;

use glam::{Vec2, Vec3};

use super::foundry::Ground;
use super::{Look, Spec};
use crate::map::Map;
use crate::rng::Rng;

pub use super::skeleton::Skeleton;
pub(in crate::map) use dig::Dig;
pub(in crate::map::procgen) use dig::cutting_rig;

pub const DECK_HALF: f32 = 3.6;
pub const DECK_TOP: f32 = 0.25;
const DECK_T: f32 = 1.0;
/// Track gauge (rail to rail, m): broad, for big ore cars.
const GAUGE: f32 = 2.2;
/// The warehouse's spur: it leaves the main line back the way the trains
/// came, swings off on a curve of this radius through this angle, and runs straight in
/// at the warehouse's rail door this far on.
const BRANCH_R: f32 = 14.0;
const BRANCH_TURN: f32 = 85.0 * PI / 180.0;
const SHED_LEAD: f32 = 4.0;
/// The warehouse: half its width (across the line) and length (along it),
/// its eaves and ridge.
pub const SHED_HALF: Vec2 = Vec2::new(13.0, 18.0);
const SHED_EAVES: f32 = 8.0;
const SHED_RIDGE: f32 = 11.3;
/// Where the branch ends, inside, at its buffer stop (along from the middle).
const SHED_BUFFER: f32 = 13.0;

pub(crate) fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Nearest point on segment a-b to p: (distance, 0..1 along).
pub(crate) fn seg(p: Vec2, a: Vec2, b: Vec2) -> (f32, f32) {
    let (d2, t) = seg_squared(p, a, b);
    (d2.sqrt(), t)
}

/// The same, the distance squared.
pub(crate) fn seg_squared(p: Vec2, a: Vec2, b: Vec2) -> (f32, f32) {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    (p.distance_squared(a + ab * t), t)
}

/// Position and heading `s` m along a line.
pub(crate) fn line_at(line: &[Vec2], s: f32) -> (Vec2, Vec2) {
    let mut left = s;
    for w in line.windows(2) {
        let len = w[0].distance(w[1]);
        let dir = (w[1] - w[0]) / len;
        if left <= len {
            return (w[0] + dir * left, dir);
        }
        left -= len;
    }
    let n = line.len();
    (line[n - 1], (line[n - 1] - line[n - 2]).normalize())
}

/// Length of a line up to its point `k`.
pub(crate) fn line_len(line: &[Vec2], k: usize) -> f32 {
    line[..=k].windows(2).map(|w| w[0].distance(w[1])).sum()
}

pub(crate) fn line_distance(line: &[Vec2], p: Vec2) -> f32 {
    // (Squared, and one root at the end: the same least distance.)
    line.windows(2)
        .map(|w| seg_squared(p, w[0], w[1]).0)
        .fold(f32::INFINITY, f32::min)
        .sqrt()
}

/// How far along `line` its nearest point to `p` is.
pub(crate) fn along(line: &[Vec2], p: Vec2) -> f32 {
    let mut gone = 0.0;
    let mut best = (f32::INFINITY, 0.0);
    for w in line.windows(2) {
        let (d, t) = seg(p, w[0], w[1]);
        let len = w[0].distance(w[1]);
        if d < best.0 {
            best = (d, gone + t * len);
        }
        gone += len;
    }
    best.1
}

/// Yaw that turns a box's local +z along `dir`.
pub(crate) fn yaw_along(dir: Vec2) -> f32 {
    dir.x.atan2(dir.y)
}

/// What a mine's owner puts in it: their lamps, the machine in the borer's
/// bay, their freight on the rails, their mark on the warehouse, and what
/// stands by their spawns. Any mine laid out by this biome can be dressed
/// by any style: an Ithessar crystal mine and an Imperium one are the same
/// ground, someone else's.
pub(in crate::map) trait MineStyle {
    /// The Hold site's name on the HUD.
    fn hold_name(&self) -> &'static str;
    /// A lamp on a wall at `at`, its bracket standing out along `out`,
    /// `size` its scale (1.5 in the tunnels).
    fn wall_lamp(&self, d: &mut Dig, at: Vec3, out: Vec2, size: f32);
    /// A lamp hanging `drop` m from `at`.
    fn hanging_lamp(&self, d: &mut Dig, at: Vec3, drop: f32, size: f32);
    /// The machine cutting the bore in a borer's bay, along `beam` (from
    /// its emitter to the bore's face, and how thick).
    fn borer(&self, d: &mut Dig, beam: (Vec3, Vec3, f32), rng: &mut Rng);
    /// What its beam sounds like where it strikes the rock.
    fn strike_sound(&self) -> crate::map::Ambient;
    /// A rail car centered at `at`, its length along `yaw`, rolled `roll`
    /// onto its side: a freight car, or the locomotive (`loco`).
    fn rail_car(&self, d: &mut Dig, at: Vec2, yaw: f32, roll: f32, loco: bool);
    /// Their mark, small, on a building at `at` in `rig`'s frame.
    fn emblem(&self, d: &mut Dig, rig: &crate::map::landmarks::Rig, at: Vec3, r: f32);
    /// What stands by side `side`'s spawns at `at`, if that's theirs.
    fn base(&self, d: &mut Dig, side: usize, at: Vec2, rng: &mut Rng);
}

fn style(look: Look) -> &'static dyn MineStyle {
    match look {
        Look::Ithessar => &ithessar::Ithessar,
        Look::Imperium => &imperium::Imperium,
        Look::Resonant => &resonant::Resonant,
        Look::Uthenn => &uthenn::Uthenn,
    }
}

/// Builds `spec`'s mine on `ground`.
pub fn build(spec: &Spec, ground: Ground) -> Map {
    dress::build(spec, ground).1
}

/// `spec`'s mine undressed.
pub fn greybox(spec: &Spec, ground: Ground) -> Map {
    let mut m = plan::skeleton(spec.seed, &spec.sides).greybox(&spec.sides);
    m.atmosphere = land::atmosphere(ground, &mut Rng::new(spec.seed ^ 0x5EED_D2E5));
    m
}

pub use land::planet;
