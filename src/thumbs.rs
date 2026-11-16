//! Rendered thumbnails of every part for the garage's cards: each hull
//! wearing the equipped turret, each turret on the equipped hull, shot
//! against the map's sky. They live in one atlas texture the UI samples,
//! drawn a few per frame and redrawn when the equipped kit or the card size
//! changes.

use glam::{Quat, Vec2, Vec3};

use crate::economy::{PARTS, Part};
use crate::garage::{self, Loadout};
use crate::render::{Camera, Renderer};
use crate::sim::{Tank, World};
use crate::tank::TankArt;

const COLS: u32 = 4;
/// How far above the map the studio tank floats, clear of the terrain.
const STUDIO_LIFT: f32 = 300.0;

#[derive(Clone, Copy, PartialEq)]
struct Key {
    kit: Loadout,
    cell: (u32, u32),
}

#[derive(Default)]
pub struct Thumbs {
    key: Option<Key>,
    /// Which of `PARTS` are drawn into the atlas.
    ready: Vec<bool>,
}

impl Thumbs {
    pub fn new() -> Thumbs {
        Thumbs::default()
    }

    /// Where `part`'s thumbnail sits in the atlas (UV min, max), once drawn.
    pub fn uv(&self, part: Part) -> Option<(Vec2, Vec2)> {
        let key = self.key?;
        let i = PARTS.iter().position(|&p| p == part)?;
        if !self.ready.get(i).copied().unwrap_or(false) {
            return None;
        }
        let size = atlas_size(key.cell);
        let (x, y) = cell_origin(i, key.cell);
        let min = Vec2::new(x as f32 / size.0 as f32, y as f32 / size.1 as f32);
        let span = Vec2::new(
            key.cell.0 as f32 / size.0 as f32,
            key.cell.1 as f32 / size.1 as f32,
        );
        Some((min, min + span))
    }

    /// Draw up to `budget` missing thumbnails at `cell` px each, for `kit`
    /// equipped. `tank` is the player's (its paint and place in the world).
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        r: &mut Renderer,
        art: &TankArt,
        world: &World,
        tank: &Tank,
        kit: Loadout,
        cell: (u32, u32),
        budget: usize,
    ) {
        let cell = (cell.0.max(8), cell.1.max(8));
        let key = Key { kit, cell };
        let remade = r.ensure_thumb_atlas(atlas_size(cell));
        if remade || self.key != Some(key) {
            self.key = Some(key);
            self.ready = vec![false; PARTS.len()];
        }
        let mut left = budget;
        for i in 0..PARTS.len() {
            if left == 0 {
                break;
            }
            if self.ready[i] {
                continue;
            }
            let scene = studio(PARTS[i], kit, world, tank, art, cell);
            r.render_thumb(&scene, cell_origin(i, cell), cell);
            self.ready[i] = true;
            left -= 1;
        }
    }
}

fn atlas_size(cell: (u32, u32)) -> (u32, u32) {
    let rows = (PARTS.len() as u32).div_ceil(COLS);
    (cell.0 * COLS, cell.1 * rows)
}

fn cell_origin(i: usize, cell: (u32, u32)) -> (u32, u32) {
    let i = i as u32;
    ((i % COLS) * cell.0, (i / COLS) * cell.1)
}

/// `part` on a tank floating high above the map, framed for its card: a
/// hull whole from the front quarter, a turret close up from the side.
fn studio(
    part: Part,
    kit: Loadout,
    world: &World,
    tank: &Tank,
    art: &TankArt,
    cell: (u32, u32),
) -> crate::render::Scene {
    let mut t = tank.clone();
    garage::dress(&mut t, garage::with_part(kit, part));
    t.pos = Vec3::new(tank.pos.x, STUDIO_LIFT, tank.pos.z);
    t.rot = Quat::IDENTITY;
    t.vel = Vec3::ZERO;
    t.turret_rel = 0.0;
    t.gun_pitch = 0.0;
    t.since_fire = 100.0;
    t.wheel_lift.iter_mut().for_each(|l| *l = 0.0);

    let aspect = cell.0 as f32 / cell.1 as f32;
    let (focus_y, dist, yaw, height) = match part {
        Part::Hull(_) => (1.0, 9.6, 0.75, 2.6),
        Part::Turret(_) => (1.9, 8.0, 1.4, 1.0),
    };
    let f = t.forward();
    let a = f.x.atan2(f.z) + yaw;
    let focus = t.pos + Vec3::Y * focus_y;
    // Wide cards: pull in a little so the tank fills the width.
    let dist = dist * (2.0 / aspect).clamp(0.8, 1.2);
    let camera = Camera {
        pos: focus + Vec3::new(a.sin() * dist, height, a.cos() * dist),
        target: focus,
        fov_y: 32f32.to_radians(),
    };
    let mut scene = crate::scene::build(world, std::slice::from_ref(&t), art, camera);
    scene.world = false;
    scene.shadow_center = t.pos;
    scene.shadow_radius = 6.0;
    scene
}
