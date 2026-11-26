//! The Uthenn's foundry: pale shell on a dark plinth, everything square, a
//! green seam lit along it; furnaces that are stacks of great cubes. They
//! cast from tungsten cubes.

use glam::{Vec2, Vec3};

use super::skeleton::*;
use super::style::*;
use crate::map::{Ambient, Map};
use crate::render::material::Material;
use crate::rng::Rng;

const TUNGSTEN: Vec3 = Vec3::new(0.2, 0.24, 0.3);
const SHELL: Vec3 = Vec3::new(0.44, 0.49, 0.46);
const SHELL_DARK: Vec3 = Vec3::new(0.12, 0.15, 0.14);
const GREEN: Vec3 = Vec3::new(0.15, 0.9, 0.35);

pub struct Uthenn;

/// A cube `s` on a side on `y`, a seam round its middle.
fn cube(map: &mut Map, c: Vec2, s: f32, yaw: f32, y: f32, lit: bool, solid: bool) {
    map.block(
        c,
        Vec2::splat(s * 0.5),
        yaw,
        (y, y + s),
        TUNGSTEN,
        Material::Metal,
        solid,
    )
    .chip = s * 0.02;
    let (color, mat) = if lit {
        (GREEN, Material::Glow)
    } else {
        (SHELL_DARK, Material::Concrete)
    };
    let mid = y + s * 0.5;
    map.block(
        c,
        Vec2::splat(s * 0.5 + 0.05),
        yaw,
        (mid - s * 0.035, mid + s * 0.035),
        color,
        mat,
        false,
    );
}

impl Works for Uthenn {
    fn palette(&self) -> Palette {
        Palette {
            wall: SHELL,
            wall_mat: Material::Painted,
            dark: SHELL_DARK * 1.3,
            trim: Vec3::new(0.16, 0.3, 0.22),
            floor: Vec3::new(0.33, 0.35, 0.34),
            roof: SHELL * 0.8,
            roof_mat: Material::Painted,
            steel: TUNGSTEN * 0.9,
            steel_mat: Material::Metal,
            casting: TUNGSTEN * 1.2,
            molten: GREEN,
            wagon: SHELL_DARK * 2.0,
        }
    }

    fn panel(&self, map: &mut Map, w: &Panel, k: usize) {
        // The seam over the plinth and another near the top, lit in runs; a
        // cube let into every third panel.
        for (y, lit) in [(1.35, k % 4 < 2), (w.top * 0.74, k % 5 == 0)] {
            let (c, m) = if lit {
                (GREEN, Material::Glow)
            } else {
                (SHELL_DARK, Material::Concrete)
            };
            on_faces(map, w, 0.0, w.reach() - 0.05, (y, y + 0.14), 0.07, c, m);
        }
        if k % 3 == 0 && w.reach() > 1.6 {
            let y = w.top * 0.4;
            on_faces(
                map,
                w,
                0.0,
                1.1,
                (y, y + 2.2),
                0.25,
                TUNGSTEN,
                Material::Metal,
            );
        }
        if k.is_multiple_of(2) {
            let post = w.center - w.along() * (w.reach() - 0.5);
            cube(map, post, 1.0, 0.0, w.top + 0.1, k.is_multiple_of(4), false);
        }
    }

    fn bay(&self, map: &mut Map, hall: &Hall, x0: f32, x1: f32) {
        let pal = self.palette();
        let c = Vec2::new((x0 + x1) * 0.5, hall.center.y);
        let half = Vec2::new((x1 - x0) * 0.5 - 0.15, hall.half.y - 0.1);
        slab(
            map,
            c,
            half,
            (hall.h, hall.h + 0.4),
            pal.roof,
            pal.roof_mat,
            true,
        );
        for s in [-1.0f32, 1.0] {
            let p = c + Vec2::Y * s * hall.half.y * 0.45;
            cube(map, p, 1.8, 0.0, hall.h + 0.4, s > 0.0, false);
        }
    }

    fn furnace(&self, map: &mut Map, f: &Round, _rng: &mut Rng) {
        // Half the diagonal inside the furnace's reach.
        let s = f.r * 1.3;
        let mut y = -0.1;
        for (k, (size, yaw)) in [(s, 0.0), (s * 0.8, 0.3), (s * 0.6, 0.6)]
            .into_iter()
            .enumerate()
        {
            let size = if k == 2 { f.h - y } else { size };
            let half = [s, s * 0.8, s * 0.6][k] * 0.5;
            let tall = if k == 2 { size } else { half * 1.7 };
            map.block(
                f.at,
                Vec2::splat(half),
                yaw,
                (y, y + tall),
                TUNGSTEN,
                Material::Metal,
                false,
            )
            .chip = 0.1;
            map.block(
                f.at,
                Vec2::splat(half * 0.9),
                yaw + 0.15,
                (y + tall, y + tall + 0.3),
                GREEN,
                Material::Glow,
                false,
            );
            y += tall + 0.3;
        }
        let mouth = f.at - Vec2::Y * (s * 0.5);
        slab(
            map,
            mouth,
            Vec2::new(1.1, 0.12),
            (0.2, 2.4),
            GREEN,
            Material::Glow,
            false,
        );
        slab(
            map,
            mouth,
            Vec2::new(1.5, 0.08),
            (-0.1, 2.8),
            SHELL_DARK,
            Material::Metal,
            false,
        );
        // Four emitters at its corners, burning into it.
        for (sx, sz) in [(-1.0f32, 1.0f32), (1.0, 1.0)] {
            let p = f.at + Vec2::new(sx, sz) * f.r * 0.62;
            cube(map, p, 0.9, 0.0, f.h * 0.62, true, false);
            map.segment(
                at(p, f.h * 0.62),
                at(p, 0.0),
                0.2,
                SHELL_DARK,
                Material::Metal,
                false,
            );
        }
        map.sound(Ambient::Machine, at(f.at, 5.0));
    }

    fn chimney(&self, map: &mut Map, c: &Round) {
        let half = c.r * std::f32::consts::FRAC_1_SQRT_2;
        let n = (c.h / 5.0).round().max(1.0) as usize;
        let seg = c.h / n as f32;
        for k in 0..n {
            let y = k as f32 * seg;
            let w = half * (1.0 - 0.25 * k as f32 / n as f32);
            let y0 = if k == 0 { -0.3 } else { y + 0.25 };
            slab(
                map,
                c.at,
                Vec2::splat(w),
                (y0, y + seg),
                SHELL * (0.95 - 0.06 * (k % 2) as f32),
                Material::Painted,
                false,
            );
            let (col, m) = if k % 2 == 1 {
                (GREEN, Material::Glow)
            } else {
                (SHELL_DARK, Material::Concrete)
            };
            slab(
                map,
                c.at,
                Vec2::splat(w * 0.92),
                (y + seg - 0.05, y + seg + 0.3),
                col,
                m,
                false,
            );
        }
    }

    fn gantry(&self, map: &mut Map, g: &Gantry) {
        slab(
            map,
            g.at,
            Vec2::new(1.3, g.span + 1.6),
            (g.h, g.h + 1.6),
            SHELL,
            Material::Painted,
            true,
        );
        slab(
            map,
            g.at,
            Vec2::new(1.36, g.span + 1.5),
            (g.h + 0.7, g.h + 0.9),
            GREEN,
            Material::Glow,
            false,
        );
        for s in [-1.0f32, 1.0] {
            let end = g.at + Vec2::Y * s * g.span;
            cube(map, end, 2.4, 0.0, g.h + 1.6, true, false);
        }
        // What it's carrying.
        let hung = g.at + Vec2::Y * g.span * g.hoist;
        cube(map, hung, 1.8, 0.3, g.h * 0.55 - 2.4, true, false);
    }

    fn fence(&self, map: &mut Map, c: Vec2, inward: Vec2, top: f32, k: usize) {
        let half = inward.perp().abs() * 2.4 + inward.abs() * (FENCE_T + 0.06);
        let (col, m) = if k.is_multiple_of(3) {
            (GREEN, Material::Glow)
        } else {
            (SHELL_DARK, Material::Concrete)
        };
        slab(map, c, half, (top - 1.4, top - 1.25), col, m, false);
        if k.is_multiple_of(4) {
            cube(map, c, 1.3, 0.0, top + 0.25, false, false);
        }
    }

    fn billets(&self, map: &mut Map, s: &Stock, g: f32, rng: &mut Rng) {
        let a = s.half.y - 0.04;
        for i in 0..2 {
            let p = s.at + Vec2::X * (i as f32 * 2.0 - 1.0) * (a + 0.05);
            let size = a * 1.72;
            cube(map, p, size, 0.0, g - 0.2, rng.chance(0.4), true);
        }
        let top = g - 0.2 + a * 1.72;
        cube(map, s.at, a * 1.2, rng.range(0.0, 0.7), top, false, false);
    }
}
