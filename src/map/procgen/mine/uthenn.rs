//! The Uthenn's crystal mine: everything a cube. Cubes of tungsten with a
//! seam of green light round them for lamps, a Lumen's beam out of a stack
//! of them in the bay, cars and locomotive cut square, a cube on the
//! warehouse, three by their spawns. Nobody's in any of it.

use std::f32::consts::{FRAC_PI_4, FRAC_PI_6};

use glam::{Quat, Vec2, Vec3};

use super::dig::Dig;
use super::kit::*;
use super::*;
use crate::map::landmarks::Rig;
use crate::map::{Ambient, OBox};
use crate::render::material::Material;
use crate::render::mesh::MeshData;
use crate::tank::model::Skin;

const TUNGSTEN: Vec3 = Vec3::new(0.18, 0.21, 0.26);
const SHELL: Vec3 = Vec3::new(0.42, 0.47, 0.44);
const DARK: Vec3 = Vec3::new(0.06, 0.075, 0.07);
const GREEN: Vec3 = Vec3::new(0.15, 0.9, 0.35);
const GREEN_HOT: Vec3 = Vec3::new(0.75, 1.0, 0.8);

pub struct Uthenn;

fn tungsten() -> Skin {
    skin(TUNGSTEN, Material::Metal)
}
fn shell() -> Skin {
    skin(SHELL, Material::Painted)
}
fn dark() -> Skin {
    skin(DARK, Material::Metal)
}
fn green() -> Skin {
    skin(GREEN, Material::Glow)
}

/// A block split by a lit seam round its middle.
fn seamed(m: &mut MeshData, c: Vec3, h: Vec3, body: Skin) {
    let gap = (h.y * 0.07).max(0.02);
    let half = Vec3::new(h.x, (h.y - gap) * 0.5, h.z);
    let off = Vec3::Y * (h.y + gap) * 0.5;
    slab(m, c - off, half, body);
    slab(m, c + off, half, body);
    slab(m, c, Vec3::new(h.x * 0.94, gap * 1.3, h.z * 0.94), green());
}

const FREIGHT: Freight = Freight {
    bogie,
    tub,
    loco,
    load: |_| 1.45,
};

impl MineStyle for Uthenn {
    fn hold_name(&self) -> &'static str {
        "VEIN"
    }

    fn wall_lamp(&self, d: &mut Dig, at: Vec3, out: Vec2, size: f32) {
        let end = d.bracket(at, out, size);
        self.hanging_lamp(d, end, 0.0, size);
    }

    fn hanging_lamp(&self, d: &mut Dig, at: Vec3, drop: f32, size: f32) {
        let light = GREEN.lerp(GREEN_HOT, 0.5) * LAMP_BRIGHT;
        d.lamp_on(at, drop, size, light, |m| {
            slab(m, Vec3::Y * -0.13, Vec3::new(0.05, 0.14, 0.05), dark());
            slab(m, Vec3::Y * -0.36, Vec3::new(0.28, 0.12, 0.28), tungsten());
            slab(m, Vec3::Y * -0.55, Vec3::new(0.24, 0.1, 0.24), green());
            slab(m, Vec3::Y * -0.74, Vec3::new(0.28, 0.12, 0.28), tungsten());
        });
    }

    fn borer(&self, d: &mut Dig, beam: (Vec3, Vec3, f32), _: &mut Rng) {
        let rig = d.borer_stand(beam, GREEN, GREEN_HOT);
        let y = beam.0.y;
        let m = &mut MeshData::default();
        // Skids; the body over them in three blocks, each smaller.
        for s in [-1.0f32, 1.0] {
            slab(
                m,
                Vec3::new(s * 2.1, 0.35, -0.6),
                Vec3::new(0.6, 0.35, 4.5),
                dark(),
            );
        }
        seamed(
            m,
            Vec3::new(0.0, 1.75, -2.6),
            Vec3::new(2.4, 1.2, 2.4),
            tungsten(),
        );
        seamed(
            m,
            Vec3::new(0.0, 3.8, -2.6),
            Vec3::new(1.5, 0.8, 1.5),
            tungsten(),
        );
        seamed(
            m,
            Vec3::new(0.0, 5.05, -2.6),
            Vec3::new(0.75, 0.42, 0.75),
            tungsten(),
        );
        for s in [-1.0f32, 1.0] {
            seamed(
                m,
                Vec3::new(s * 1.8, 1.4, 1.4),
                Vec3::new(0.7, 0.75, 1.3),
                shell(),
            );
        }
        // The barrel: square, in lengths with light between them, the
        // aperture a lit square in a frame.
        for k in 0..4 {
            let z = 0.5 + k as f32 * 1.4;
            let h = 0.95 - k as f32 * 0.06;
            slab(m, Vec3::new(0.0, y, z), Vec3::new(h, h, 0.6), shell());
            slab(
                m,
                Vec3::new(0.0, y, z + 0.7),
                Vec3::new(h * 0.8, h * 0.8, 0.14),
                green(),
            );
        }
        for (x, v) in [(0.95f32, 0.0f32), (-0.95, 0.0), (0.0, 0.95), (0.0, -0.95)] {
            let h = if v == 0.0 {
                Vec3::new(0.2, 0.74, 0.5)
            } else {
                Vec3::new(1.15, 0.2, 0.52)
            };
            slab(m, Vec3::new(x, y + v, 5.6), h, tungsten());
        }
        // Its feed, two pillars of cubes behind it.
        for x in [-1.7f32, 1.7] {
            for k in 0..5 {
                let c = Vec3::new(x, 0.45 + k as f32 * 1.06, -6.2);
                seamed(m, c, Vec3::new(0.45, 0.5, 0.45), tungsten());
            }
            bar(
                m,
                Vec3::new(x, 1.9, -5.8),
                Vec3::new(x, 1.9, -4.9),
                0.14,
                dark(),
            );
        }
        d.sculpt(&rig, m);
        d.borer_posts(&rig);
    }

    fn strike_sound(&self) -> Ambient {
        Ambient::Cable
    }

    fn rail_car(&self, d: &mut Dig, at: Vec2, yaw: f32, roll: f32, loco: bool) {
        d.car(at, yaw, roll, loco, &FREIGHT);
    }

    fn emblem(&self, d: &mut Dig, rig: &Rig, at: Vec3, r: f32) {
        // A cube, seen corner on, half out of the wall.
        let m = &mut MeshData::default();
        let h = r * 0.6;
        seamed(m, Vec3::ZERO, Vec3::splat(h), tungsten());
        let cube = Rig {
            pos: rig.point(at),
            rot: rig.rot * Quat::from_rotation_x(-FRAC_PI_6) * Quat::from_rotation_y(FRAC_PI_4),
        };
        d.sculpt(&cube, m);
    }

    fn base(&self, d: &mut Dig, side: usize, at: Vec2, rng: &mut Rng) {
        if d.factions.get(side) != Some(&crate::faction::Faction::Uthenn) {
            return;
        }
        // Great cubes, one on another, each turned.
        let g = d.ground_height(at.x, at.y);
        let turn = rng.range(0.0, 0.8);
        for (y, a, yaw) in [
            (-0.6f32, 1.75f32, 0.0f32),
            (2.9, 1.4, 0.35),
            (5.7, 1.0, 0.8),
        ] {
            let rig = Rig {
                pos: Vec3::new(at.x, g + y + a, at.y),
                rot: Quat::from_rotation_y(turn + yaw),
            };
            let m = &mut MeshData::default();
            seamed(m, Vec3::ZERO, Vec3::splat(a), tungsten());
            d.sculpt(&rig, m);
            d.oboxes.push(OBox {
                center: at,
                yaw: turn + yaw,
                half: Vec2::splat(a),
                y0: g + y,
                y1: g + y + a * 2.0,
                hidden: true,
                ..OBox::default()
            });
        }
        d.sound(Ambient::Machine, Vec3::new(at.x, g + 2.0, at.y));
    }
}

fn bogie(m: &mut MeshData, z: f32) {
    wheels(m, z, dark(), green());
    for x in [-1.42f32, 1.42] {
        slab(
            m,
            Vec3::new(x, 0.75, z),
            Vec3::new(0.07, 0.3, 1.5),
            tungsten(),
        );
    }
    slab(
        m,
        Vec3::new(0.0, 0.95, z),
        Vec3::new(1.2, 0.16, 0.45),
        dark(),
    );
}

/// An ore car: two square bins on one frame, a seam of light round them.
fn tub(m: &mut MeshData, loaded: bool) {
    slab(
        m,
        Vec3::new(0.0, 1.25, 0.0),
        Vec3::new(1.3, 0.16, 4.28),
        dark(),
    );
    for s in [-1.0f32, 1.0] {
        for (y, h) in [(1.9f32, 0.5f32), (3.03, 0.5)] {
            slab(
                m,
                Vec3::new(s * 1.6, y, -2.1),
                Vec3::new(0.1, h, 2.05),
                shell(),
            );
            slab(
                m,
                Vec3::new(s * 1.6, y, 2.1),
                Vec3::new(0.1, h, 2.05),
                shell(),
            );
            slab(
                m,
                Vec3::new(0.0, y, s * 4.05),
                Vec3::new(1.49, h - 0.02, 0.1),
                shell(),
            );
        }
        slab(
            m,
            Vec3::new(s * 1.58, 2.465, 0.0),
            Vec3::new(0.07, 0.08, 4.1),
            green(),
        );
        slab(
            m,
            Vec3::new(0.0, 2.465, s * 4.03),
            Vec3::new(1.5, 0.07, 0.07),
            green(),
        );
        for z in [-4.0f32, 0.0, 4.0] {
            slab(
                m,
                Vec3::new(s * 1.62, 3.55, z),
                Vec3::new(0.13, 0.13, 0.13),
                tungsten(),
            );
        }
        slab(
            m,
            Vec3::new(0.0, 1.2, s * 4.4),
            Vec3::new(0.25, 0.2, 0.16),
            dark(),
        );
    }
    slab(
        m,
        Vec3::new(0.0, 1.48, 0.0),
        Vec3::new(1.48, 0.1, 3.94),
        dark(),
    );
    if loaded {
        bed(m, Vec2::new(1.46, 3.93));
    }
}

/// The locomotive: blocks coupled end to end, no cab, an eye in front.
fn loco(m: &mut MeshData) {
    slab(
        m,
        Vec3::new(0.0, 1.25, -0.3),
        Vec3::new(1.4, 0.2, 6.15),
        dark(),
    );
    seamed(
        m,
        Vec3::new(0.0, 2.7, -3.7),
        Vec3::new(1.7, 1.22, 2.6),
        tungsten(),
    );
    seamed(
        m,
        Vec3::new(0.0, 4.42, -2.5),
        Vec3::new(0.95, 0.44, 1.9),
        shell(),
    );
    seamed(
        m,
        Vec3::new(0.0, 2.4, 0.8),
        Vec3::new(1.55, 0.92, 1.6),
        shell(),
    );
    seamed(
        m,
        Vec3::new(0.0, 2.15, 4.2),
        Vec3::new(1.4, 0.68, 1.6),
        tungsten(),
    );
    // The couplings between, lit.
    for (z, h) in [(-0.95f32, 0.7f32), (2.5, 0.55)] {
        slab(m, Vec3::new(0.0, 2.2, z), Vec3::new(1.0, h, 0.25), green());
    }
    slab(
        m,
        Vec3::new(0.0, 2.35, 5.82),
        Vec3::new(0.5, 0.2, 0.06),
        dark(),
    );
    slab(
        m,
        Vec3::new(0.0, 2.35, 5.87),
        Vec3::new(0.32, 0.1, 0.04),
        green(),
    );
    for k in 0..4 {
        let z = -5.4 + k as f32 * 0.6;
        slab(
            m,
            Vec3::new(0.0, 3.97, z),
            Vec3::new(1.2, 0.08, 0.16),
            dark(),
        );
    }
}
