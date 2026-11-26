//! The Imperium's crystal mine: iron and crimson, banded in gold. Caged
//! lamps burning amber, a thermal lance on a crawler's chassis in the bay,
//! slab-sided cars behind a diesel, their planet under its halo on the
//! warehouse, a concrete pylon hung with banners by their spawns.

use glam::{Vec2, Vec3};

use super::dig::{Dig, STEEL};
use super::kit::*;
use super::*;
use crate::map::landmarks::Rig;
use crate::map::{Ambient, OBox};
use crate::render::material::Material;
use crate::render::mesh::MeshData;
use crate::tank::model::{self, GILT, Skin};

const IRON: Vec3 = Vec3::new(0.16, 0.14, 0.13);
const CRIMSON: Vec3 = Vec3::new(0.3, 0.045, 0.045);
const GOLD: Vec3 = Vec3::new(0.48, 0.33, 0.14);
const CONCRETE: Vec3 = Vec3::new(0.5, 0.47, 0.43);
const AMBER: Vec3 = Vec3::new(1.0, 0.5, 0.14);
const AMBER_HOT: Vec3 = Vec3::new(1.0, 0.86, 0.6);

pub struct Imperium;

fn iron() -> Skin {
    skin(IRON, Material::Metal)
}
fn crimson() -> Skin {
    skin(CRIMSON, Material::Painted)
}
fn gold() -> Skin {
    skin(GOLD, Material::Metal)
}
fn amber() -> Skin {
    skin(AMBER, Material::Glow)
}

const FREIGHT: Freight = Freight {
    bogie,
    tub,
    loco,
    load: |_| 1.45,
};

impl MineStyle for Imperium {
    fn hold_name(&self) -> &'static str {
        "LODE"
    }

    fn wall_lamp(&self, d: &mut Dig, at: Vec3, out: Vec2, size: f32) {
        let end = d.bracket(at, out, size);
        self.hanging_lamp(d, end, 0.0, size);
    }

    fn hanging_lamp(&self, d: &mut Dig, at: Vec3, drop: f32, size: f32) {
        let light = AMBER.lerp(AMBER_HOT, 0.45) * LAMP_BRIGHT;
        d.lamp_on(at, drop, size, light, |m| {
            // A bell of a shade, the bulb, four bars of a cage, a gold foot.
            post(
                m,
                Vec2::ZERO,
                &[(-0.34, 0.32), (-0.2, 0.2), (0.0, 0.08)],
                iron(),
            );
            post(
                m,
                Vec2::ZERO,
                &[(-0.8, 0.12), (-0.72, 0.16), (-0.36, 0.14)],
                amber(),
            );
            for (x, z) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
                let a = Vec3::new(x * 0.17, -0.32, z * 0.17);
                let b = Vec3::new(x * 0.13, -0.86, z * 0.13);
                bar(m, a, b, 0.016, iron());
            }
            post(m, Vec2::ZERO, &[(-0.94, 0.1), (-0.84, 0.21)], gold());
        });
    }

    fn borer(&self, d: &mut Dig, beam: (Vec3, Vec3, f32), _: &mut Rng) {
        let rig = d.borer_stand(beam, AMBER, AMBER_HOT);
        let y = beam.0.y;
        let m = &mut MeshData::default();
        // Crawler tracks either side, their rollers showing.
        for s in [-1.0f32, 1.0] {
            let side = [
                Vec2::new(-5.0, 0.55),
                Vec2::new(-4.4, 0.05),
                Vec2::new(3.2, 0.05),
                Vec2::new(3.9, 0.55),
                Vec2::new(3.4, 1.15),
                Vec2::new(-4.5, 1.15),
            ];
            model::solid(
                m,
                &[model::at_x(&side, s * 1.75), model::at_x(&side, s * 2.75)],
                0.06,
                model::TREAD,
            );
            for k in 0..6 {
                let c = Vec2::new(-3.9 + k as f32 * 1.35, 0.6);
                let ring = |r: f32, x: f32| model::at_x(&model::ngon(10, r, c), s * x);
                model::round(m, &[ring(0.4, 2.73), ring(0.4, 2.84)], 0.03, iron());
                model::round(m, &[ring(0.14, 2.83), ring(0.14, 2.9)], 0.01, gold());
            }
        }
        // The chassis, the engine house on it, its stripe and its grilles.
        slab(
            m,
            Vec3::new(0.0, 1.0, -0.6),
            Vec3::new(1.7, 0.45, 4.2),
            iron(),
        );
        slab(
            m,
            Vec3::new(0.0, 2.2, -2.2),
            Vec3::new(2.2, 0.78, 2.6),
            crimson(),
        );
        slab(
            m,
            Vec3::new(0.0, 2.2, -2.2),
            Vec3::new(2.24, 0.09, 2.64),
            gold(),
        );
        for k in 0..5 {
            let z = -4.0 + k as f32 * 0.8;
            slab(
                m,
                Vec3::new(0.0, 3.05, z),
                Vec3::new(1.5, 0.12, 0.22),
                iron(),
            );
        }
        // The operator's cab, lit.
        slab(
            m,
            Vec3::new(-1.6, 2.5, 1.3),
            Vec3::new(0.75, 1.0, 0.8),
            crimson(),
        );
        slab(
            m,
            Vec3::new(-1.6, 2.95, 1.4),
            Vec3::new(0.6, 0.3, 0.75),
            amber(),
        );
        slab(
            m,
            Vec3::new(-1.6, 3.55, 1.3),
            Vec3::new(0.85, 0.07, 0.95),
            iron(),
        );
        // Stacks, smoking up past the house.
        for x in [0.9f32, 1.7] {
            post(m, Vec2::new(x, -4.1), &[(2.9, 0.2), (4.7, 0.2)], iron());
            post(m, Vec2::new(x, -4.1), &[(4.6, 0.26), (4.85, 0.26)], gold());
        }
        // The lance: a breech, a ribbed barrel, a flared nozzle.
        turned(m, Vec2::new(0.0, y), &[(0.3, 1.05), (1.9, 1.05)], iron());
        turned(m, Vec2::new(0.0, y), &[(1.85, 0.7), (5.0, 0.6)], iron());
        for k in 0..4 {
            let z = 2.2 + k as f32 * 0.75;
            turned(m, Vec2::new(0.0, y), &[(z, 0.84), (z + 0.22, 0.84)], gold());
        }
        turned(
            m,
            Vec2::new(0.0, y),
            &[(4.9, 0.66), (5.6, 1.0), (5.75, 1.0)],
            iron(),
        );
        turned(m, Vec2::new(0.0, y), &[(5.7, 0.8), (5.79, 0.8)], amber());
        // Jacks braced to the floor, out past the tracks.
        for (x, z) in [(-2.2f32, 4.1f32), (2.2, 4.1)] {
            bar(
                m,
                Vec3::new(x * 0.4, 1.2, 3.2),
                Vec3::new(x, 0.1, z),
                0.14,
                gold(),
            );
            slab(m, Vec3::new(x, 0.06, z), Vec3::new(0.4, 0.06, 0.4), iron());
        }
        // Fuel behind, in two banded bottles, piped in.
        for x in [-1.7f32, 1.7] {
            let c = Vec2::new(x, -6.2);
            post(
                m,
                c,
                &[(-0.1, 0.55), (4.6, 0.55), (5.1, 0.4), (5.4, 0.14)],
                crimson(),
            );
            for y in [0.7, 2.6, 4.2] {
                post(m, c, &[(y, 0.59), (y + 0.2, 0.59)], gold());
            }
            let (a, b) = (Vec3::new(x * 0.8, 1.6, -5.8), Vec3::new(x * 0.6, 1.8, -4.7));
            bar(m, a, b, 0.1, iron());
        }
        d.sculpt(&rig, m);
        d.borer_posts(&rig);
    }

    fn strike_sound(&self) -> Ambient {
        Ambient::Furnace
    }

    fn rail_car(&self, d: &mut Dig, at: Vec2, yaw: f32, roll: f32, loco: bool) {
        d.car(at, yaw, roll, loco, &FREIGHT);
    }

    fn emblem(&self, d: &mut Dig, rig: &Rig, at: Vec3, r: f32) {
        let m = &mut MeshData::default();
        model::emblem(m, at, Vec3::Y, -Vec3::Z, r * 1.3, GILT);
        d.sculpt(rig, m);
    }

    fn base(&self, d: &mut Dig, side: usize, at: Vec2, _: &mut Rng) {
        if d.factions.get(side) != Some(&crate::faction::Faction::Imperium) {
            return;
        }
        // A pylon on a plinth, stepped in, gold between its courses.
        let g = d.ground_height(at.x, at.y);
        let mut course = |half: f32, y: (f32, f32), color, mat| {
            d.oboxes.push(OBox {
                center: at,
                half: Vec2::splat(half),
                y0: g + y.0,
                y1: g + y.1,
                color,
                side_mat: mat,
                top_mat: mat,
                ..OBox::default()
            });
        };
        course(1.8, (-1.0, 1.4), CONCRETE * 0.8, Material::Concrete);
        course(1.25, (1.2, 9.0), CONCRETE, Material::Concrete);
        course(1.4, (8.9, 9.4), GOLD, Material::Metal);
        course(0.95, (9.3, 11.0), CONCRETE, Material::Concrete);
        for out in [Vec2::X, Vec2::Y, -Vec2::X, -Vec2::Y] {
            let p = at + out * 1.25;
            d.banner(Vec3::new(p.x, g + 8.6, p.y), out, 1.8, 5.4);
        }
        let rig = Rig {
            pos: Vec3::new(at.x, g + 12.2, at.y),
            rot: glam::Quat::IDENTITY,
        };
        let m = &mut MeshData::default();
        for s in [-1.0f32, 1.0] {
            model::emblem(m, Vec3::Z * 0.03 * s, Vec3::Y, Vec3::Z * s, 1.3, GILT);
        }
        bar(m, Vec3::Y * -1.3, Vec3::Y * -0.7, 0.08, gold());
        d.sculpt(&rig, m);
    }
}

fn bogie(m: &mut MeshData, z: f32) {
    wheels(m, z, iron(), gold());
    for x in [-1.42f32, 1.42] {
        slab(m, Vec3::new(x, 0.72, z), Vec3::new(0.06, 0.16, 1.5), iron());
    }
    slab(
        m,
        Vec3::new(0.0, 0.95, z),
        Vec3::new(1.2, 0.16, 0.45),
        skin(STEEL, Material::Metal),
    );
}

/// An ore car: a slab-sided tub in crimson, ribbed, its rim gilt.
fn tub(m: &mut MeshData, loaded: bool) {
    slab(
        m,
        Vec3::new(0.0, 1.25, 0.0),
        Vec3::new(1.3, 0.16, 4.25),
        iron(),
    );
    let c = crimson();
    for s in [-1.0f32, 1.0] {
        slab(
            m,
            Vec3::new(s * 1.6, 2.4, 0.0),
            Vec3::new(0.08, 1.0, 4.1),
            c,
        );
        slab(
            m,
            Vec3::new(0.0, 2.4, s * 4.1),
            Vec3::new(1.52, 0.97, 0.08),
            c,
        );
        slab(
            m,
            Vec3::new(s * 1.6, 3.45, 0.0),
            Vec3::new(0.14, 0.09, 4.26),
            gold(),
        );
        slab(
            m,
            Vec3::new(0.0, 3.45, s * 4.1),
            Vec3::new(1.46, 0.07, 0.14),
            gold(),
        );
        for k in 0..6 {
            let z = -3.5 + k as f32 * 1.4;
            slab(
                m,
                Vec3::new(s * 1.7, 2.35, z),
                Vec3::new(0.05, 0.97, 0.1),
                iron(),
            );
        }
        slab(
            m,
            Vec3::new(0.0, 1.2, s * 4.4),
            Vec3::new(0.25, 0.2, 0.16),
            iron(),
        );
    }
    slab(
        m,
        Vec3::new(0.0, 1.48, 0.0),
        Vec3::new(1.54, 0.1, 4.04),
        iron(),
    );
    if loaded {
        bed(m, Vec2::new(1.48, 4.0));
    }
}

/// The locomotive: a long hood over its engine, a crimson cab behind.
fn loco(m: &mut MeshData) {
    let c = crimson();
    slab(
        m,
        Vec3::new(0.0, 1.3, -0.3),
        Vec3::new(1.45, 0.2, 6.1),
        iron(),
    );
    // The hood, its stripe, and the grilles down it.
    let hood = model::trap(1.2, 0.95, 1.45, 3.3, 0.3);
    model::solid(
        m,
        &[model::at_z(&hood, -0.8), model::at_z(&hood, 5.2)],
        0.12,
        c,
    );
    slab(
        m,
        Vec3::new(0.0, 2.0, 2.2),
        Vec3::new(1.26, 0.08, 3.06),
        gold(),
    );
    for s in [-1.0f32, 1.0] {
        for k in 0..5 {
            let z = 0.4 + k as f32 * 0.9;
            slab(
                m,
                Vec3::new(s * 1.1, 2.6, z),
                Vec3::new(0.08, 0.3, 0.3),
                iron(),
            );
        }
        // Handrails along the walks.
        bar(
            m,
            Vec3::new(s * 1.6, 2.3, -0.6),
            Vec3::new(s * 1.6, 2.3, 5.4),
            0.035,
            gold(),
        );
        for k in 0..4 {
            let p = Vec3::new(s * 1.6, 1.3, -0.4 + k as f32 * 1.9);
            bar(m, p, p + Vec3::Y, 0.03, gold());
        }
        slab(
            m,
            Vec3::new(s * 1.35, 1.46, 2.3),
            Vec3::new(0.38, 0.05, 3.2),
            iron(),
        );
    }
    // Stacks, headlamp, emblem on the nose, buffer beam.
    for z in [1.2f32, 2.6] {
        post(m, Vec2::new(0.0, z), &[(3.2, 0.24), (3.85, 0.24)], iron());
    }
    turned(m, Vec2::new(0.0, 3.5), &[(4.5, 0.26), (5.1, 0.32)], iron());
    turned(
        m,
        Vec2::new(0.0, 3.5),
        &[(5.05, 0.25), (5.14, 0.25)],
        amber(),
    );
    model::emblem(m, Vec3::new(0.0, 2.6, 5.2), Vec3::Y, Vec3::Z, 0.5, GILT);
    slab(
        m,
        Vec3::new(0.0, 1.0, 5.7),
        Vec3::new(1.6, 0.35, 0.15),
        gold(),
    );
    // The cab, windows lit all round, and the lantern roof.
    slab(m, Vec3::new(0.0, 2.6, -3.5), Vec3::new(1.65, 1.2, 2.65), c);
    slab(
        m,
        Vec3::new(0.0, 3.15, -3.5),
        Vec3::new(1.68, 0.3, 1.9),
        amber(),
    );
    slab(
        m,
        Vec3::new(0.0, 3.15, -3.5),
        Vec3::new(1.3, 0.3, 2.69),
        amber(),
    );
    for z in [-4.6f32, -3.5, -2.4] {
        slab(m, Vec3::new(0.0, 3.15, z), Vec3::new(1.71, 0.36, 0.1), c);
    }
    slab(
        m,
        Vec3::new(0.0, 3.88, -3.5),
        Vec3::new(1.74, 0.1, 2.75),
        iron(),
    );
    slab(
        m,
        Vec3::new(0.0, 4.35, -2.5),
        Vec3::new(0.95, 0.42, 1.9),
        iron(),
    );
    slab(
        m,
        Vec3::new(0.0, 4.8, -2.5),
        Vec3::new(1.0, 0.08, 2.0),
        gold(),
    );
}
