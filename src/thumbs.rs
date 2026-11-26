//! Rendered thumbnails of every part for the garage's rows: each hull and
//! each turret alone, against the dark. They live in one atlas texture the UI samples,
//! drawn a few per frame and redrawn when the row size changes.

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::economy::{PARTS, Part};
use crate::garage::{self, Loadout};
use crate::render::mesh::{Instance, MeshId};
use crate::render::{Atmosphere, Camera, Renderer};
use crate::sim::{HullKind, Tank, TurretKind, World};
use crate::tank::TankArt;

const COLS: u32 = 4;
/// How far above the map the studio tank floats, clear of the terrain.
const STUDIO_LIFT: f32 = 300.0;
/// What a part's put on to be shot (only the part's drawn).
const STAND: Loadout = (HullKind::Kataphrakt, TurretKind::Kontos);

#[derive(Clone, Copy, PartialEq)]
struct Key {
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

    /// Draw up to `budget` missing thumbnails at `cell` px each. `tank` is the player's (its paint and place in the world).
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        r: &mut Renderer,
        art: &TankArt,
        world: &World,
        tank: &Tank,
        cell: (u32, u32),
        budget: usize,
    ) {
        let cell = (cell.0.max(8), cell.1.max(8));
        let key = Key { cell };
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
            let scene = studio(PARTS[i], world, tank, art, cell);
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

/// `part` alone, floating high above the map in the dark: a hull from the
/// front quarter, a turret from the side.
fn studio(
    part: Part,
    world: &World,
    tank: &Tank,
    art: &TankArt,
    cell: (u32, u32),
) -> crate::render::Scene {
    let mut t = tank.clone();
    garage::dress(&mut t, garage::with_part(STAND, part));
    t.pos = Vec3::new(tank.pos.x, STUDIO_LIFT, tank.pos.z);
    t.rot = Quat::IDENTITY;
    t.vel = Vec3::ZERO;
    t.turret_rel = 0.0;
    t.gun_pitch = 0.0;
    t.since_fire = 100.0;
    t.wheel_lift.iter_mut().for_each(|l| *l = 0.0);

    let aspect = cell.0 as f32 / cell.1 as f32;
    // What to look at (ahead of the tank's middle, and up), from how far,
    // which way round and how high.
    let (ahead, up, dist, yaw, height) = match part {
        Part::Hull(HullKind::Gravis) => (0.0, 1.1, 10.2, 0.85, 2.6),
        Part::Hull(_) => (0.0, 0.75, 6.6, 0.85, 1.9),
        Part::Turret(TurretKind::Knell) => (0.2, 2.3, 5.6, 1.1, 0.8),
        Part::Turret(TurretKind::Kontos | TurretKind::Lumen) => (1.25, 1.75, 6.9, 1.05, 1.0),
        Part::Turret(_) => (0.8, 1.75, 5.0, 1.05, 0.8),
    };
    let f = t.forward();
    let a = f.x.atan2(f.z) + yaw;
    let focus = t.pos + f * ahead + Vec3::Y * up;
    // Wide pictures: pull in a little so the part fills the width.
    let dist = dist * (2.0 / aspect).clamp(0.8, 1.2);
    let camera = Camera {
        pos: focus + Vec3::new(a.sin() * dist, height, a.cos() * dist),
        target: focus,
        fov_y: 32f32.to_radians(),
    };
    let mut scene = crate::scene::build(world, &[], art, camera);
    match part {
        Part::Hull(_) => art.hull_instances(&t, &mut scene.instances),
        Part::Turret(_) => {
            art.turret_instances(&t, &mut scene.instances, &mut scene.particles);
        }
    }
    // The dark, self-lit, behind it.
    let dark = Mat4::from_scale_rotation_translation(Vec3::splat(200.0), Quat::IDENTITY, focus);
    let grey = Vec4::new(0.0455, 0.047, 0.0475, 1.0);
    scene
        .instances
        .push((MeshId::SPHERE, Instance::new(dark, grey)));
    scene.atmosphere = Atmosphere {
        sun_dir: Vec3::new(0.7, 0.8, 0.5).normalize(),
        sun_color: Vec3::splat(2.6),
        sky_zenith: Vec3::splat(0.4),
        sky_horizon: Vec3::splat(0.25),
        ground_color: Vec3::splat(0.12),
        fog_density: 0.0,
        blowing_sand: 0.0,
        ..Atmosphere::default()
    };
    scene.sun_dir = scene.atmosphere.sun_dir;
    scene.lights.clear();
    scene.world = false;
    scene.shadow_center = t.pos;
    scene.shadow_radius = 6.0;
    scene
}
