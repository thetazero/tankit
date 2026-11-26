//! The Imperium's foundry: raw concrete halls under sawtooth roofs, hung
//! with crimson; drum furnaces banded in gold.

use glam::{Vec2, Vec3};

use super::dress::ALONG_X;
use super::skeleton::*;
use super::style::*;
use crate::map::Map;
use crate::render::material::Material;
use crate::rng::Rng;

const GOLD: Vec3 = Vec3::new(0.62, 0.46, 0.16);
const IRON: Vec3 = Vec3::new(0.24, 0.2, 0.18);
const MOLTEN: Vec3 = Vec3::new(1.0, 0.4, 0.08);

pub struct Imperium;

impl Works for Imperium {
    fn palette(&self) -> Palette {
        Palette {
            wall: Vec3::new(0.52, 0.49, 0.45),
            wall_mat: Material::Concrete,
            dark: Vec3::new(0.3, 0.28, 0.26),
            trim: Vec3::new(0.46, 0.09, 0.08),
            floor: Vec3::new(0.36, 0.34, 0.32),
            roof: Vec3::new(0.3, 0.27, 0.25),
            roof_mat: Material::Rust,
            steel: Vec3::new(0.34, 0.25, 0.2),
            steel_mat: Material::Rust,
            casting: Vec3::new(0.32, 0.31, 0.31),
            molten: MOLTEN,
            wagon: Vec3::new(0.4, 0.14, 0.11),
        }
    }

    fn panel(&self, map: &mut Map, w: &Panel, k: usize) {
        // A gilt string course, high windows; a banner on every fourth.
        let h = w.top;
        on_faces(
            map,
            w,
            0.0,
            w.reach() - 0.06,
            (h * 0.62, h * 0.62 + 0.22),
            0.1,
            GOLD,
            Material::Metal,
        );
        if k % 4 == 1 && w.reach() > 2.0 {
            map.banner(face(w, 0.0, h * 0.95), w.out, 2.4, h * 0.6);
            return;
        }
        let n = (w.reach() / 1.0) as i32;
        for i in 0..n {
            let t = (i as f32 - (n - 1) as f32 * 0.5) * 1.5;
            let lit = (k as i32 * 3 + i) % 5 == 0;
            let (c, m) = if lit {
                (MOLTEN * 0.5, Material::Glow)
            } else {
                (Vec3::new(0.08, 0.08, 0.09), Material::Metal)
            };
            on_faces(map, w, t, 0.42, (h * 0.69, h * 0.9), 0.05, c, m);
        }
    }

    fn bay(&self, map: &mut Map, hall: &Hall, x0: f32, x1: f32) {
        let pal = self.palette();
        // A sawtooth: a slope up, glass down its steep side.
        let rise = 2.2f32;
        let (w, z) = (x1 - x0, hall.half.y - 0.2);
        let len = w.hypot(rise);
        let y = hall.h + 0.3 + rise * 0.5;
        map.block(
            Vec2::new((x0 + x1) * 0.5, hall.center.y),
            Vec2::new(z, len * 0.5),
            ALONG_X,
            (y - 0.15, y + 0.15),
            pal.roof,
            pal.roof_mat,
            true,
        )
        .tilt = -(rise / w).atan();
        slab(
            map,
            Vec2::new(x1 - 0.25, hall.center.y),
            Vec2::new(0.06, z - 0.3),
            (hall.h + 0.1, hall.h + rise),
            Vec3::new(0.12, 0.13, 0.14),
            Material::Metal,
            false,
        );
    }

    fn furnace(&self, map: &mut Map, f: &Round, _rng: &mut Rng) {
        let y = |t: f32| at(f.at, f.h * t);
        let m = Material::Rust;
        map.pipe(at(f.at, -0.2), y(0.42), f.r, IRON, m, false);
        map.pipe(y(0.42), y(0.75), f.r * 0.86, IRON * 1.1, m, false);
        map.pipe(y(0.75), y(1.0), f.r * 0.66, IRON, m, false);
        for (t, r) in [(0.42, 1.0), (0.75, 0.86), (1.0, 0.66)] {
            map.disc(y(t), Vec3::Y * 0.12, f.r * r * 0.97, IRON * 0.9, m);
            map.pipe(
                at(f.at, f.h * t - 0.5),
                at(f.at, f.h * t - 0.15),
                f.r * r + 0.08,
                GOLD,
                Material::Metal,
                false,
            );
        }
        // The mouth, under a hood.
        let mouth = f.at - Vec2::Y * (f.r - 0.12);
        slab(
            map,
            mouth,
            Vec2::new(1.1, 0.25),
            (0.1, 2.2),
            MOLTEN,
            Material::Glow,
            false,
        );
        slab(
            map,
            mouth - Vec2::Y * 0.2,
            Vec2::new(1.6, 0.6),
            (2.25, 2.7),
            IRON * 0.7,
            m,
            false,
        );
        map.standing_disc(
            at(mouth - Vec2::Y * 0.05, f.h * 0.33),
            Vec2::NEG_Y,
            0.9,
            GOLD,
        );
    }

    fn chimney(&self, map: &mut Map, c: &Round) {
        let pal = self.palette();
        let top = |t: f32| at(c.at, c.h * t);
        map.pipe(
            at(c.at, -0.3),
            top(0.5),
            c.r,
            pal.wall * 0.9,
            Material::Concrete,
            false,
        );
        map.pipe(
            top(0.5),
            top(1.0),
            c.r * 0.85,
            pal.wall * 0.8,
            Material::Concrete,
            false,
        );
        for t in [0.5, 0.98] {
            map.pipe(
                top(t),
                at(c.at, c.h * t + 0.5),
                c.r * 1.06,
                GOLD * 0.8,
                Material::Metal,
                false,
            );
        }
        map.pipe(
            top(0.78),
            top(0.86),
            c.r * 0.9,
            pal.trim,
            Material::Painted,
            false,
        );
    }

    fn gantry(&self, map: &mut Map, g: &Gantry) {
        let pal = self.palette();
        for s in [-1.0f32, 1.0] {
            slab(
                map,
                g.at + Vec2::X * s * 1.1,
                Vec2::new(0.3, g.span + 2.0),
                (g.h, g.h + 1.6),
                pal.trim,
                Material::Painted,
                true,
            );
            slab(
                map,
                g.at + Vec2::Y * s * (g.span + 1.8),
                Vec2::new(1.5, 0.25),
                (g.h + 0.1, g.h + 1.5),
                pal.steel,
                pal.steel_mat,
                false,
            );
            let end = g.at + Vec2::Y * s * g.span * 0.75 - Vec2::X * 1.42;
            map.banner(at(end, g.h + 0.5), Vec2::NEG_X, 1.6, 4.0);
        }
    }

    fn fence(&self, map: &mut Map, c: Vec2, inward: Vec2, top: f32, k: usize) {
        if k.is_multiple_of(7) {
            map.banner(at(c + inward * FENCE_T, top - 0.2), inward, 2.0, 3.4);
        }
    }

    fn billets(&self, map: &mut Map, s: &Stock, g: f32, _rng: &mut Rng) {
        // Bars, crosswise course by course.
        let pal = self.palette();
        for k in 0..4 {
            let y = g + k as f32 * 0.5;
            let y0 = if k == 0 { g - 0.6 } else { y };
            for i in 0..4 {
                let t = (i as f32 - 1.5) / 2.0;
                let (off, half) = if k % 2 == 0 {
                    (Vec2::Y * t * s.half.y, Vec2::new(s.half.x, s.half.y * 0.22))
                } else {
                    (Vec2::X * t * s.half.x, Vec2::new(s.half.x * 0.22, s.half.y))
                };
                let shade = 0.85 + 0.1 * ((i * 2 + k) % 3) as f32;
                slab(
                    map,
                    s.at + off,
                    half,
                    (y0, y + 0.48),
                    pal.casting * shade,
                    Material::Rust,
                    true,
                );
            }
        }
    }
}

impl Map {
    /// A disc standing against a wall that faces `out`.
    fn standing_disc(&mut self, c: Vec3, out: Vec2, r: f32, color: Vec3) {
        let n = Vec3::new(out.x, 0.0, out.y);
        self.disc(c, n * 0.08, r, color, Material::Metal);
    }
}
