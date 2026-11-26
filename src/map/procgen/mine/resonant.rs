//! The Resonant's crystal mine: ivory, slate and gilt round the crystal's
//! own blue. Lanterns with a shard in each, a reliquary in the bay whose
//! choir of pipes sings the rock apart, freight in shrines on wheels, their
//! window over the warehouse door, a shrine by their spawns.

use std::f32::consts::FRAC_PI_4;

use glam::{Quat, Vec2, Vec3};

use super::dig::Dig;
use super::kit::*;
use super::*;
use crate::map::landmarks::Rig;
use crate::map::{Ambient, CRYSTAL_LIGHT, Crystal, OBox};
use crate::render::material::Material;
use crate::render::mesh::MeshData;
use crate::tank::model::{self, Skin};

const GILT: Vec3 = Vec3::new(0.62, 0.47, 0.22);
const BRONZE: Vec3 = Vec3::new(0.42, 0.29, 0.15);
const IVORY: Vec3 = Vec3::new(0.66, 0.61, 0.51);
const SLATE: Vec3 = Vec3::new(0.15, 0.18, 0.26);
const BLUE_HOT: Vec3 = Vec3::new(0.75, 0.9, 1.0);

pub struct Resonant;

fn gilt() -> Skin {
    skin(GILT, Material::Metal)
}
fn bronze() -> Skin {
    skin(BRONZE, Material::Metal)
}
fn ivory() -> Skin {
    skin(IVORY, Material::Plaster)
}
fn slate() -> Skin {
    skin(SLATE, Material::Roof)
}
fn blue() -> Skin {
    skin(CRYSTAL_LIGHT, Material::Glow)
}

/// A crystal of light, pointed at both ends, from `a` to `b`.
fn shard(m: &mut MeshData, a: Vec3, b: Vec3, r: f32) {
    let d = (b - a).normalize();
    let side = d.any_orthonormal_vector();
    let up = d.cross(side);
    let ring = |t: f32, r: f32| {
        (0..6)
            .map(|k| {
                let w = Vec2::from_angle(k as f32 * FRAC_PI_4 * 4.0 / 3.0) * r;
                a.lerp(b, t) + side * w.x + up * w.y
            })
            .collect::<Vec<_>>()
    };
    model::round(
        m,
        &[
            ring(0.0, 0.01),
            ring(0.3, r),
            ring(0.7, r * 0.9),
            ring(1.0, 0.01),
        ],
        0.0,
        blue(),
    );
}

/// A lancet's outline, `r` half as wide, from its foot up: (across, up).
fn lancet(r: f32) -> [Vec2; 7] {
    [
        Vec2::new(-1.0, -1.3),
        Vec2::new(-1.0, 0.3),
        Vec2::new(-0.62, 1.05),
        Vec2::new(0.0, 1.6),
        Vec2::new(0.62, 1.05),
        Vec2::new(1.0, 0.3),
        Vec2::new(1.0, -1.3),
    ]
    .map(|p| p * r)
}

/// A lit lancet framed in gilt, at `c`, across along `u`, facing `out`.
fn window(m: &mut MeshData, c: Vec3, u: Vec3, out: Vec3, r: f32) {
    let pts = lancet(r);
    let at = |p: Vec2, o: f32| c + u * p.x + Vec3::Y * p.y + out * o;
    let ring = |o: f32| pts.iter().map(|&p| at(p, o)).collect::<Vec<_>>();
    model::round(m, &[ring(-0.02), ring(0.04)], 0.0, blue());
    for k in 0..7 {
        let (a, b) = (pts[k], pts[(k + 1) % 7]);
        bar(m, at(a, 0.04), at(b, 0.04), 0.06 + 0.05 * r, gilt());
    }
}

const FREIGHT: Freight = Freight {
    bogie,
    tub,
    loco,
    load: |_| 1.45,
};

impl MineStyle for Resonant {
    fn hold_name(&self) -> &'static str {
        "HOLY SEAM"
    }

    fn wall_lamp(&self, d: &mut Dig, at: Vec3, out: Vec2, size: f32) {
        let end = d.bracket(at, out, size);
        self.hanging_lamp(d, end, 0.0, size);
    }

    fn hanging_lamp(&self, d: &mut Dig, at: Vec3, drop: f32, size: f32) {
        let light = CRYSTAL_LIGHT.lerp(BLUE_HOT, 0.45) * LAMP_BRIGHT;
        d.lamp_on(at, drop, size, light, |m| {
            // A spired cap, the shard, its frame, a pointed foot.
            post(
                m,
                Vec2::ZERO,
                &[(-0.3, 0.26), (-0.22, 0.22), (0.0, 0.03)],
                gilt(),
            );
            shard(m, Vec3::Y * -0.28, Vec3::Y * -0.84, 0.13);
            for k in 0..4 {
                let w = Vec2::from_angle(k as f32 * FRAC_PI_4 * 2.0 + FRAC_PI_4) * 0.21;
                let a = Vec3::new(w.x, -0.28, w.y);
                bar(m, a, a - Vec3::Y * 0.56, 0.018, gilt());
            }
            post(
                m,
                Vec2::ZERO,
                &[(-1.02, 0.02), (-0.88, 0.22), (-0.8, 0.25)],
                gilt(),
            );
        });
    }

    fn borer(&self, d: &mut Dig, beam: (Vec3, Vec3, f32), rng: &mut Rng) {
        let rig = d.borer_stand(beam, CRYSTAL_LIGHT, BLUE_HOT);
        let y = beam.0.y;
        let m = &mut MeshData::default();
        // A carriage on iron-shod wheels.
        for z in [-3.6f32, 0.0, 2.6] {
            for s in [-1.0f32, 1.0] {
                let c = Vec2::new(z, 0.75);
                let ring = |r: f32, x: f32| model::at_x(&model::ngon(14, r, c), s * x);
                model::round(m, &[ring(0.75, 2.2), ring(0.75, 2.55)], 0.04, bronze());
                model::round(m, &[ring(0.5, 2.53), ring(0.5, 2.6)], 0.02, ivory());
                model::round(m, &[ring(0.18, 2.59), ring(0.18, 2.72)], 0.02, gilt());
            }
        }
        slab(
            m,
            Vec3::new(0.0, 1.0, -0.8),
            Vec3::new(2.0, 0.35, 4.4),
            bronze(),
        );
        // The reliquary: an ivory house, gilt sills, lancets down its sides,
        // a slate roof with a ridge.
        slab(
            m,
            Vec3::new(0.0, 2.0, -1.8),
            Vec3::new(1.7, 0.75, 3.0),
            ivory(),
        );
        for (y, h) in [(1.4f32, 0.1f32), (2.72, 0.12)] {
            slab(m, Vec3::new(0.0, y, -1.8), Vec3::new(1.84, h, 3.12), gilt());
        }
        for k in 0..4 {
            let z = -4.0 + k as f32 * 1.45;
            for s in [-1.0f32, 1.0] {
                window(m, Vec3::new(s * 1.7, 2.02, z), Vec3::Z, Vec3::X * s, 0.36);
            }
        }
        let roof = [
            Vec2::new(-1.75, 2.82),
            Vec2::new(1.75, 2.82),
            Vec2::new(0.0, 4.3),
        ];
        model::round(
            m,
            &[model::at_z(&roof, -4.7), model::at_z(&roof, 1.1)],
            0.02,
            slate(),
        );
        bar(
            m,
            Vec3::new(0.0, 4.32, -4.75),
            Vec3::new(0.0, 4.32, 1.15),
            0.08,
            gilt(),
        );
        // The choir: pipes ranked down either side of the mouth, which is a
        // bell, the crystal's tip in it.
        for k in 0..4 {
            for s in [-1.0f32, 1.0] {
                let (x, z) = (s * (1.2 + 0.3 * k as f32), 1.6);
                let top = 4.9 - 0.45 * k as f32;
                post(
                    m,
                    Vec2::new(x, z),
                    &[(1.3, 0.06), (1.8, 0.15), (top, 0.15)],
                    gilt(),
                );
                post(
                    m,
                    Vec2::new(x, z),
                    &[(top - 0.02, 0.1), (top + 0.04, 0.02)],
                    blue(),
                );
            }
        }
        let c = Vec2::new(0.0, y);
        turned(m, c, &[(1.0, 0.75), (2.6, 0.75)], bronze());
        turned(
            m,
            c,
            &[(2.5, 0.62), (4.3, 0.7), (5.2, 1.0), (5.5, 1.3), (5.6, 1.3)],
            bronze(),
        );
        for (z, r) in [(2.4f32, 0.88f32), (3.4, 0.78), (4.4, 0.84)] {
            turned(m, c, &[(z, r), (z + 0.16, r)], gilt());
        }
        turned(m, c, &[(5.55, 1.1), (5.63, 1.1)], blue());
        shard(m, Vec3::new(0.0, y, 5.4), Vec3::new(0.0, y, 6.9), 0.4);
        // Braces from the bell down to the floor.
        for s in [-1.0f32, 1.0] {
            let foot = Vec3::new(s * 2.2, 0.1, 4.2);
            bar(m, Vec3::new(s * 0.7, y, 3.6), foot, 0.12, gilt());
            slab(m, foot, Vec3::new(0.4, 0.1, 0.4), bronze());
        }
        // Behind it, the crystal it draws on, between two gilt pillars.
        for x in [-1.7f32, 1.7] {
            let c = Vec2::new(x, -6.2);
            post(
                m,
                c,
                &[
                    (-0.1, 0.52),
                    (0.7, 0.45),
                    (0.8, 0.3),
                    (4.5, 0.3),
                    (4.6, 0.45),
                    (5.4, 0.03),
                ],
                gilt(),
            );
        }
        d.sculpt(&rig, m);
        d.borer_posts(&rig);
        d.crystals.push(Crystal {
            base: rig.point(Vec3::new(0.0, 0.0, -6.2)),
            axis: Vec3::Y * 4.4,
            radius: 0.6,
            sides: 6,
            seed: rng.next_u64() as u32,
            color: CRYSTAL_LIGHT,
            lit: true,
            solid: false,
            raw: false,
        });
    }

    fn strike_sound(&self) -> Ambient {
        Ambient::Altar
    }

    fn rail_car(&self, d: &mut Dig, at: Vec2, yaw: f32, roll: f32, loco: bool) {
        d.car(at, yaw, roll, loco, &FREIGHT);
    }

    fn emblem(&self, d: &mut Dig, rig: &Rig, at: Vec3, r: f32) {
        let m = &mut MeshData::default();
        window(m, at, Vec3::X, -Vec3::Z, r * 0.7);
        d.sculpt(rig, m);
    }

    fn base(&self, d: &mut Dig, side: usize, at: Vec2, rng: &mut Rng) {
        if d.factions.get(side) != Some(&crate::faction::Faction::Resonant) {
            return;
        }
        // A shrine: a stepped plinth, a gilt canopy on four posts, the
        // crystal under it.
        let g = d.ground_height(at.x, at.y);
        for (half, y0, y1) in [(1.85f32, -1.0f32, 0.9f32), (1.65, 0.8, 1.6)] {
            d.oboxes.push(OBox {
                center: at,
                half: Vec2::splat(half),
                y0: g + y0,
                y1: g + y1,
                color: IVORY,
                side_mat: Material::Plaster,
                top_mat: Material::Plaster,
                ..OBox::default()
            });
        }
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::IDENTITY,
        };
        let m = &mut MeshData::default();
        for (x, z) in [(1.0f32, 1.0f32), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
            let c = Vec2::new(x, z) * 1.35;
            post(
                m,
                c,
                &[
                    (1.5, 0.35),
                    (2.0, 0.22),
                    (7.0, 0.22),
                    (7.1, 0.4),
                    (8.6, 0.02),
                ],
                gilt(),
            );
            let spring = Vec3::new(c.x, 6.4, c.y);
            bar(m, spring, Vec3::new(0.0, 9.0, 0.0), 0.16, gilt());
        }
        post(
            m,
            Vec2::ZERO,
            &[(8.6, 0.5), (9.2, 0.4), (11.6, 0.03)],
            slate(),
        );
        d.sculpt(&rig, m);
        for (off, h, r) in [
            (Vec2::ZERO, 5.4f32, 0.65f32),
            (Vec2::new(0.6, 0.3), 3.0, 0.4),
            (Vec2::new(-0.4, -0.6), 2.5, 0.4),
        ] {
            let p = at + off;
            d.crystals.push(Crystal {
                base: Vec3::new(p.x, g + 1.2, p.y),
                axis: Vec3::new(off.x * 0.25, 1.0, off.y * 0.25).normalize() * h,
                radius: r,
                sides: 6,
                seed: rng.next_u64() as u32,
                color: CRYSTAL_LIGHT,
                lit: true,
                solid: false,
                raw: false,
            });
        }
        d.sound(Ambient::Shrine, Vec3::new(at.x, g + 4.0, at.y));
    }
}

fn bogie(m: &mut MeshData, z: f32) {
    wheels(m, z, bronze(), gilt());
    for x in [-1.42f32, 1.42] {
        slab(
            m,
            Vec3::new(x, 0.72, z),
            Vec3::new(0.06, 0.14, 1.4),
            bronze(),
        );
    }
    slab(
        m,
        Vec3::new(0.0, 0.95, z),
        Vec3::new(1.2, 0.16, 0.4),
        bronze(),
    );
}

/// An ore car: ivory panels between gilt ribs, a lit lancet in each.
fn tub(m: &mut MeshData, loaded: bool) {
    slab(
        m,
        Vec3::new(0.0, 1.25, 0.0),
        Vec3::new(1.3, 0.16, 4.25),
        bronze(),
    );
    for s in [-1.0f32, 1.0] {
        slab(
            m,
            Vec3::new(s * 1.57, 2.4, 0.0),
            Vec3::new(0.08, 1.0, 4.05),
            ivory(),
        );
        slab(
            m,
            Vec3::new(0.0, 2.4, s * 4.05),
            Vec3::new(1.49, 0.97, 0.08),
            ivory(),
        );
        bar(
            m,
            Vec3::new(s * 1.57, 3.45, -4.2),
            Vec3::new(s * 1.57, 3.45, 4.2),
            0.11,
            gilt(),
        );
        bar(
            m,
            Vec3::new(-1.47, 3.44, s * 4.05),
            Vec3::new(1.47, 3.44, s * 4.05),
            0.09,
            gilt(),
        );
        for k in 0..4 {
            let z = -3.0 + k as f32 * 2.0;
            window(m, Vec3::new(s * 1.65, 2.3, z), Vec3::Z, Vec3::X * s, 0.38);
        }
        for k in 0..5 {
            let z = -4.0 + k as f32 * 2.0;
            post(
                m,
                Vec2::new(s * 1.64, z),
                &[(1.33, 0.1), (3.5, 0.1), (3.8, 0.01)],
                gilt(),
            );
        }
        slab(
            m,
            Vec3::new(0.0, 1.2, s * 4.4),
            Vec3::new(0.25, 0.2, 0.16),
            bronze(),
        );
    }
    slab(
        m,
        Vec3::new(0.0, 1.48, 0.0),
        Vec3::new(1.51, 0.1, 3.99),
        bronze(),
    );
    if loaded {
        bed(m, Vec2::new(1.45, 3.95));
    }
}

/// The locomotive: a chapel on wheels, its nave behind, its choir of pipes
/// at the prow with the crystal burning between them.
fn loco(m: &mut MeshData) {
    slab(
        m,
        Vec3::new(0.0, 1.3, -0.3),
        Vec3::new(1.5, 0.2, 6.1),
        bronze(),
    );
    slab(
        m,
        Vec3::new(0.0, 2.45, -2.6),
        Vec3::new(1.5, 1.0, 3.6),
        ivory(),
    );
    slab(
        m,
        Vec3::new(0.0, 3.5, -2.6),
        Vec3::new(1.66, 0.1, 3.72),
        gilt(),
    );
    let roof = [
        Vec2::new(-1.6, 3.58),
        Vec2::new(1.6, 3.58),
        Vec2::new(0.95, 4.15),
        Vec2::new(0.0, 4.85),
        Vec2::new(-0.95, 4.15),
    ];
    model::round(
        m,
        &[model::at_z(&roof, -6.1), model::at_z(&roof, 0.9)],
        0.02,
        slate(),
    );
    bar(
        m,
        Vec3::new(0.0, 4.86, -6.2),
        Vec3::new(0.0, 4.86, 1.0),
        0.07,
        gilt(),
    );
    for s in [-1.0f32, 1.0] {
        for k in 0..4 {
            let z = -5.1 + k as f32 * 1.65;
            window(m, Vec3::new(s * 1.5, 2.45, z), Vec3::Z, Vec3::X * s, 0.45);
        }
        for k in 0..5 {
            let z = -5.95 + k as f32 * 1.65;
            post(
                m,
                Vec2::new(s * 1.55, z),
                &[(1.36, 0.13), (3.45, 0.11)],
                gilt(),
            );
        }
    }
    window(m, Vec3::new(0.0, 2.5, -6.2), Vec3::X, -Vec3::Z, 0.6);
    // The prow: a narrowing ivory pulpit.
    let base = model::trap(1.35, 1.2, 1.45, 2.5, 0.1);
    let nose = model::trap(0.5, 0.4, 1.45, 2.1, 0.08);
    model::solid(
        m,
        &[
            model::at_z(&base, 0.9),
            model::at_z(&base, 3.4),
            model::at_z(&nose, 5.7),
        ],
        0.05,
        ivory(),
    );
    slab(
        m,
        Vec3::new(0.0, 2.52, 2.1),
        Vec3::new(1.28, 0.06, 1.35),
        gilt(),
    );
    for k in 0..4 {
        for s in [-1.0f32, 1.0] {
            let z = 1.3 + 0.55 * k as f32;
            let top = 4.2 - 0.35 * k as f32;
            post(
                m,
                Vec2::new(s * 0.85, z),
                &[(2.5, 0.07), (2.8, 0.17), (top, 0.17)],
                gilt(),
            );
            post(
                m,
                Vec2::new(s * 0.85, z),
                &[(top - 0.02, 0.12), (top + 0.03, 0.03)],
                blue(),
            );
        }
    }
    shard(m, Vec3::new(0.0, 2.4, 2.1), Vec3::new(0.0, 4.4, 2.1), 0.35);
    slab(
        m,
        Vec3::new(0.0, 1.0, 5.7),
        Vec3::new(1.5, 0.3, 0.15),
        gilt(),
    );
}
