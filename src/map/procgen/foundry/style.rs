//! What a faction's style fills in on a foundry. Each filler keeps its solid
//! parts at tank height to the skeleton's envelope: what drives on the
//! greybox drives on the dressed map.

use glam::{Vec2, Vec3};

use super::skeleton::*;
use crate::map::{Map, OBox};
use crate::render::material::Material;
use crate::rng::Rng;

/// The colours and stuff a works is built of.
pub struct Palette {
    pub wall: Vec3,
    pub wall_mat: Material,
    /// Plinths, copings, posts.
    pub dark: Vec3,
    pub trim: Vec3,
    pub floor: Vec3,
    pub roof: Vec3,
    pub roof_mat: Material,
    /// Girders, cranes, wagons.
    pub steel: Vec3,
    pub steel_mat: Material,
    /// A raw casting.
    pub casting: Vec3,
    /// What runs out of the furnaces, and lights the place.
    pub molten: Vec3,
    /// Wagons' paint.
    pub wagon: Vec3,
}

pub trait Works {
    fn palette(&self) -> Palette;
    /// What dresses a whole wall panel of a hall, the `k`th, `h` tall.
    fn panel(&self, map: &mut Map, w: &Panel, k: usize);
    /// One bay of a hall's roof, from `x0` to `x1`.
    fn bay(&self, map: &mut Map, hall: &Hall, x0: f32, x1: f32);
    /// What closes a hall's roof at its ends, if it needs any.
    fn gables(&self, _map: &mut Map, _hall: &Hall) {}
    /// A furnace, its mouth to the south.
    fn furnace(&self, map: &mut Map, f: &Round, rng: &mut Rng);
    fn chimney(&self, map: &mut Map, c: &Round);
    /// A gantry's beam and what's on it (its legs and hoist are dressed
    /// already).
    fn gantry(&self, map: &mut Map, g: &Gantry);
    /// What marks a gatepost or a fence panel as theirs.
    fn fence(&self, map: &mut Map, c: Vec2, inward: Vec2, top: f32, k: usize);
    /// A stack of what they cast hulls from.
    fn billets(&self, map: &mut Map, s: &Stock, g: f32, rng: &mut Rng);
}

/// A plain box, square to the map.
pub fn slab(
    map: &mut Map,
    c: Vec2,
    half: Vec2,
    y: (f32, f32),
    color: Vec3,
    mat: Material,
    solid: bool,
) -> &mut OBox {
    map.block(c, half, 0.0, y, color, mat, solid)
}

/// What stops a tank at something round: two boxes, an eighth apart.
pub fn round_solid(map: &mut Map, c: Vec2, r: f32, y: (f32, f32)) {
    for yaw in [0.0, std::f32::consts::FRAC_PI_4] {
        let half = Vec2::splat(r * std::f32::consts::FRAC_1_SQRT_2);
        map.block(c, half, yaw, y, Vec3::ZERO, Material::Plain, true)
            .hidden = true;
    }
}

pub fn at(p: Vec2, y: f32) -> Vec3 {
    Vec3::new(p.x, y, p.y)
}

/// A flat thing on a panel's outer and inner faces: `w` wide, from `y0` to
/// `y1`, `proud` of it, `t` along it from its middle.
pub fn on_faces(
    map: &mut Map,
    w: &Panel,
    t: f32,
    half_w: f32,
    y: (f32, f32),
    proud: f32,
    color: Vec3,
    mat: Material,
) {
    let along = w.along();
    let half = along * half_w + w.out.abs() * (WALL_T * 0.5 + proud);
    slab(map, w.center + along * t, half, y, color, mat, false);
}

/// Where a panel's outer face is, `t` along it, at height `y`.
pub fn face(w: &Panel, t: f32, y: f32) -> Vec3 {
    at(w.center + w.along() * t + w.out * (WALL_T * 0.5), y)
}
