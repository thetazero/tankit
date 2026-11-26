//! What a faction's style adds to a works' parts: none of it solid, all of
//! it in the part's own frame (so it stands whichever way the part's turned).

use glam::Vec3;

use super::parts::{Panel, Site};
use crate::render::material::Material;

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
    /// Girders, cranes, pipes.
    pub steel: Vec3,
    pub steel_mat: Material,
    /// A raw casting.
    pub casting: Vec3,
    /// What runs out of the furnaces, and lights the place.
    pub molten: Vec3,
    pub wagon: Vec3,
}

pub trait Works {
    fn palette(&self) -> Palette;
    /// A whole wall panel, the `k`th of its wall.
    fn panel(&self, s: &mut Site, w: &Panel, k: usize);
    /// One bay of a roof, its slab's top at `y`.
    fn bay(&self, s: &mut Site, x: f32, z: f32, half: (f32, f32), y: f32);
    /// A furnace, its mouth toward +x.
    fn furnace(&self, s: &mut Site, x: f32, z: f32, r: f32, h: f32);
    fn chimney(&self, s: &mut Site, x: f32, z: f32, r: f32, y: (f32, f32));
    /// A crane's or portal's beam, `half` about the middle.
    fn girder(&self, s: &mut Site, half: (f32, f32), y: (f32, f32));
    /// A stack of what they cast from.
    fn billets(&self, s: &mut Site, w: f32, l: f32);
}
