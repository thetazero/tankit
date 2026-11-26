//! The Ithessar's foundry: squared dark stone slit with plasma, crystal
//! standing on it; the furnaces are balls of plasma held between crystals,
//! and the halls are mostly open to the sky under ribs of their glass.

use std::f32::consts::TAU;

use glam::{Vec2, Vec3};

use super::skeleton::*;
use super::style::*;
use crate::map::{Ambient, Core, Crystal, GlowPath, Map};
use crate::render::material::Material;
use crate::rng::Rng;
use crate::tank::ithessar::PLASMA;

const GLASS: Vec3 = Vec3::new(0.02, 0.065, 0.17);
const STONE: Vec3 = Vec3::new(0.18, 0.19, 0.23);
const BRONZE: Vec3 = Vec3::new(0.28, 0.2, 0.14);

pub struct Ithessar;

/// One of their crystals, from `base` along `axis`. Never solid: the
/// skeleton's own boxes stop tanks.
fn crystal(map: &mut Map, base: Vec3, axis: Vec3, radius: f32, lit: bool) {
    let seed = (base.x * 37.0 + base.z * 91.0 + base.y * 13.0).to_bits();
    map.crystals.push(Crystal {
        base,
        axis,
        radius,
        sides: 5 + seed % 3,
        seed,
        color: GLASS,
        lit,
        solid: false,
        raw: false,
    });
}

impl Works for Ithessar {
    fn palette(&self) -> Palette {
        Palette {
            wall: STONE * 1.35,
            wall_mat: Material::Concrete,
            dark: STONE * 0.75,
            trim: BRONZE,
            floor: STONE * 1.15,
            roof: GLASS * 1.4,
            roof_mat: Material::Metal,
            steel: BRONZE * 0.9,
            steel_mat: Material::Metal,
            casting: Vec3::new(0.07, 0.1, 0.17),
            molten: PLASMA,
            wagon: STONE * 1.1,
        }
    }

    fn panel(&self, map: &mut Map, w: &Panel, k: usize) {
        if k.is_multiple_of(2) {
            on_faces(
                map,
                w,
                0.0,
                0.16,
                (1.8, w.top - 1.2),
                0.06,
                PLASMA,
                Material::Glow,
            );
        }
        // Stepped: a higher course on every other one, crystals between.
        if k % 2 == 1 {
            slab(
                map,
                w.center,
                w.half - w.along() * 0.4,
                (w.top + 0.05, w.top + 1.2),
                STONE * 1.2,
                Material::Concrete,
                true,
            );
        } else if w.reach() > 1.5 {
            let up = Vec3::new(w.out.x * 0.25, 1.0, w.out.y * 0.25);
            crystal(
                map,
                at(w.center, w.top - 0.4),
                up * 3.4,
                0.5,
                k.is_multiple_of(4),
            );
        }
    }

    fn bay(&self, map: &mut Map, hall: &Hall, x0: f32, x1: f32) {
        // Ribs leaning in from both walls, not quite meeting.
        let x = (x0 + x1) * 0.5;
        for s in [-1.0f32, 1.0] {
            let foot = Vec2::new(x, hall.center.y + s * (hall.half.y - 0.6));
            let axis = Vec3::new(0.0, 6.0, -s * (hall.half.y - 2.4));
            crystal(map, at(foot, hall.h - 0.4), axis, 1.0, (x as i32) % 3 == 0);
        }
        // Panes of their glass, low along each wall.
        for s in [-1.0f32, 1.0] {
            slab(
                map,
                Vec2::new(x, hall.center.y + s * (hall.half.y - 3.2)),
                Vec2::new((x1 - x0) * 0.5 - 0.3, 2.6),
                (hall.h + 0.1, hall.h + 0.32),
                self.palette().roof,
                Material::Metal,
                true,
            );
        }
    }

    fn furnace(&self, map: &mut Map, f: &Round, _rng: &mut Rng) {
        map.pipe(
            at(f.at, -0.2),
            at(f.at, 1.0),
            f.r,
            STONE * 1.2,
            Material::Concrete,
            false,
        );
        map.disc(
            at(f.at, 0.98),
            Vec3::Y * 0.05,
            f.r * 0.97,
            STONE,
            Material::Concrete,
        );
        let heart = at(f.at, f.h * 0.66);
        let mut prongs = Vec::new();
        for k in 0..5 {
            let d = Vec2::from_angle(std::f32::consts::FRAC_PI_2 + k as f32 * TAU / 5.0);
            let foot = at(f.at + d * (f.r - 0.9), 0.6);
            let tip = heart + Vec3::new(d.x, 0.35, d.y) * 1.9;
            crystal(map, foot, tip - foot, 0.95, k % 2 == 0);
            prongs.push(tip);
        }
        map.cores.push(Core {
            at: heart,
            radius: 1.5,
            prongs,
        });
        let n = 24;
        map.glow_paths.push(GlowPath {
            points: (0..=n)
                .map(|k| {
                    let q = f.at + Vec2::from_angle(k as f32 / n as f32 * TAU) * (f.r + 0.3);
                    at(q, 0.09)
                })
                .collect(),
            widths: vec![0.25; n + 1],
            color: PLASMA,
        });
        map.sound(Ambient::Core, heart);
    }

    fn chimney(&self, map: &mut Map, c: &Round) {
        // A pylon of stone, a long crystal standing in its top: what comes
        // up the flue is light.
        let top = c.h * 0.6;
        for yaw in [0.0, std::f32::consts::FRAC_PI_4] {
            map.block(
                c.at,
                Vec2::splat(c.r * 0.7),
                yaw,
                (-0.3, top),
                STONE * 1.25,
                Material::Concrete,
                false,
            );
        }
        for k in 0..4 {
            let d = Vec2::from_angle(k as f32 * TAU / 4.0);
            slab(
                map,
                c.at + d * c.r * 0.7,
                Vec2::splat(0.13),
                (1.5, top - 1.0),
                PLASMA,
                Material::Glow,
                false,
            );
        }
        crystal(
            map,
            at(c.at, top - 1.0),
            Vec3::Y * (c.h - top + 1.0),
            c.r * 0.75,
            true,
        );
    }

    fn gantry(&self, map: &mut Map, g: &Gantry) {
        slab(
            map,
            g.at,
            Vec2::new(1.1, g.span + 1.4),
            (g.h, g.h + 1.6),
            STONE * 1.3,
            Material::Concrete,
            true,
        );
        slab(
            map,
            g.at,
            Vec2::new(1.16, g.span + 0.8),
            (g.h + 0.35, g.h + 0.55),
            PLASMA,
            Material::Glow,
            false,
        );
        for s in [-1.0f32, 1.0] {
            let end = g.at + Vec2::Y * s * g.span;
            crystal(
                map,
                at(end, g.h + 1.0),
                Vec3::new(0.0, 5.0, s * 1.6),
                0.8,
                s > 0.0,
            );
        }
        map.sound(Ambient::Crystal, at(g.at, g.h));
    }

    fn fence(&self, map: &mut Map, c: Vec2, inward: Vec2, top: f32, k: usize) {
        if k.is_multiple_of(3) {
            let half = inward.perp().abs() * 0.15 + inward.abs() * (FENCE_T + 0.06);
            slab(
                map,
                c,
                half,
                (top - 3.6, top - 0.6),
                PLASMA,
                Material::Glow,
                false,
            );
        }
        if k % 4 == 1 {
            crystal(map, at(c, top - 0.3), Vec3::Y * 2.6, 0.55, false);
        }
    }

    fn billets(&self, map: &mut Map, s: &Stock, g: f32, rng: &mut Rng) {
        // Crystals cut to length, lying in a rack.
        slab(
            map,
            s.at,
            s.half,
            (g - 0.6, g + 1.9),
            Vec3::ZERO,
            Material::Plain,
            true,
        )
        .hidden = true;
        for x in [-1.5f32, 1.5] {
            for z in [-1.0f32, 1.0] {
                slab(
                    map,
                    s.at + Vec2::new(x, z * (s.half.y - 0.12)),
                    Vec2::splat(0.12),
                    (g - 0.6, g + 1.9),
                    BRONZE,
                    Material::Metal,
                    false,
                );
            }
        }
        for (z, y) in [(-0.6, 0.5), (0.6, 0.5), (-0.5, 1.35), (0.55, 1.4)] {
            crystal(
                map,
                at(s.at + Vec2::new(-s.half.x + 0.1, z), g + y),
                Vec3::X * (s.half.x * 2.0 - 0.2),
                0.5,
                rng.chance(0.3),
            );
        }
    }
}
