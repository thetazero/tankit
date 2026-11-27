//! The crystal mine biome: digs in a cliff wandering across a desert, at
//! whatever angle it and they lie. Laid out (`plan`) on the one `Skeleton`
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
pub mod plan;
mod resonant;
mod uthenn;
mod works;

use std::f32::consts::PI;

use glam::{Vec2, Vec3};

use super::{Look, Spec};
use crate::map::Map;
use crate::rng::Rng;

pub use super::skeleton::Skeleton;
pub(in crate::map) use dig::Dig;

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

/// Builds `spec`'s mine.
pub fn build(spec: &Spec) -> Map {
    dress::build(spec).1
}

/// `spec`'s mine undressed.
pub fn greybox(spec: &Spec) -> Map {
    let mut m = plan::skeleton(spec.seed, &spec.sides).greybox(&spec.sides);
    m.atmosphere = atmosphere(&mut Rng::new(spec.seed ^ 0x5EED_D2E5));
    m
}

/// The sky over the mine: hot late afternoon, a low gold sun from a
/// different quarter each seed (but never straight down the Face), a hazy
/// sky going white at the horizon, sand blowing off the mesa.
pub fn atmosphere(rng: &mut Rng) -> crate::render::Atmosphere {
    // The Seam's sun came from the south-east, low.
    let az = -0.53 + rng.range(-1.1, 1.1);
    let up = rng.range(0.5, 0.62);
    let flat = 0.81 * (1.0 - up * up).sqrt();
    crate::render::Atmosphere {
        sun_dir: Vec3::new(az.cos() * flat, up * 0.81, az.sin() * flat),
        sun_color: Vec3::new(2.2, 1.85, 1.45),
        sky_zenith: Vec3::new(0.2, 0.36, 0.62),
        sky_horizon: Vec3::new(0.7, 0.6, 0.5),
        ground_color: Vec3::new(0.42, 0.3, 0.2),
        fog_density: rng.range(0.0011, 0.0015),
        wind: Vec2::new(rng.range(1.5, 3.5), rng.range(-1.5, 1.5)),
        blowing_sand: 0.3,
        dust: Vec3::new(0.72, 0.55, 0.38),
        settle: crate::render::SAND_SETTLE,
        ambient: 1.0,
        stars: 0.0,
    }
}

/// A mine world from orbit: red desert, dark basins, little sea, thin
/// cloud.
pub const PLANET: crate::planet::Look = crate::planet::Look {
    land: Vec3::new(0.34, 0.2, 0.12),
    basin: Vec3::new(0.17, 0.1, 0.07),
    sea: Vec3::new(0.03, 0.06, 0.1),
    cloud: Vec3::new(0.45, 0.42, 0.4),
    cloudiness: 0.35,
};
