//! The Imperium's foundry: raw concrete halls under sawtooth roofs, hung
//! with crimson; drum furnaces banded in gold.

use glam::Vec3;

use super::parts::{Panel, Site};
use super::style::*;
use crate::render::material::Material;

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

    fn panel(&self, s: &mut Site, w: &Panel, k: usize) {
        let h = w.top;
        let (x, z) = w.spot(0.0, 0.0);
        let y = h * 0.62;
        let l = w.reach - 0.06;
        s.deck(x, z, w.half(l, 0.66), (y, y + 0.22), GOLD, Material::Metal);
        if k % 4 == 1 && w.reach > 1.8 && h > 6.0 {
            let (x, z) = w.spot(0.0, 0.8);
            let (ox, oz) = w.out();
            let (top, out) = (s.at(x, h * 0.95, z), s.dir(ox, oz));
            s.map.banner(top, out, 2.2, h * 0.55);
            return;
        }
        let n = (w.reach / 1.0) as i32;
        for i in 0..n {
            let t = (i as f32 - (n - 1) as f32 * 0.5) * 1.5;
            let (c, m) = if (k as i32 * 3 + i) % 5 == 0 {
                (MOLTEN * 0.5, Material::Glow)
            } else {
                (Vec3::new(0.08, 0.08, 0.09), Material::Metal)
            };
            let (x, z) = w.spot(t, 0.0);
            s.deck(x, z, w.half(0.42, 0.61), (h * 0.69, h * 0.9), c, m);
        }
    }

    fn bay(&self, s: &mut Site, x: f32, z: f32, half: (f32, f32), y: f32) {
        // A sawtooth: a slope up, glass down its steep side.
        let p = self.palette();
        let rise = 2.0f32;
        let len = (half.1 * 2.0).hypot(rise) * 0.5;
        let o = s.deck(
            x,
            z,
            (half.0, len),
            (y + rise * 0.5 - 0.1, y + rise * 0.5 + 0.12),
            p.roof,
            p.roof_mat,
        );
        o.tilt = -(rise / (half.1 * 2.0)).atan();
        s.deck(
            x,
            z + half.1 - 0.2,
            (half.0 - 0.3, 0.06),
            (y - 0.1, y + rise - 0.15),
            Vec3::new(0.12, 0.13, 0.14),
            Material::Metal,
        );
    }

    fn furnace(&self, s: &mut Site, x: f32, z: f32, r: f32, h: f32) {
        let m = Material::Rust;
        let rings = [(0.0, 0.42, 1.0), (0.42, 0.75, 0.86), (0.75, 1.0, 0.66)];
        for (a, b, k) in rings {
            let a = if a == 0.0 { -0.2 } else { h * a };
            s.pipe((x, a, z), (x, h * b, z), r * k, IRON, m);
            s.disc((x, h * b - 0.08, z), r * k * 0.96, IRON * 0.9, m);
            s.pipe(
                (x, h * b - 0.55, z),
                (x, h * b - 0.2, z),
                r * k + 0.08,
                GOLD,
                Material::Metal,
            );
        }
        s.deck(
            x + r - 0.12,
            z,
            (0.25, 1.1),
            (0.1, 2.2),
            MOLTEN,
            Material::Glow,
        );
        s.deck(x + r + 0.1, z, (0.6, 1.6), (2.25, 2.7), IRON * 0.7, m);
        s.pipe((x, h - 0.1, z), (x, h + 3.0, z), 0.8, IRON, m);
    }

    fn chimney(&self, s: &mut Site, x: f32, z: f32, r: f32, (a, b): (f32, f32)) {
        let p = self.palette();
        let mid = (a + b) * 0.5;
        s.pipe((x, a - 0.2, z), (x, mid, z), r, p.wall * 0.9, p.wall_mat);
        s.pipe((x, mid, z), (x, b, z), r * 0.85, p.wall * 0.8, p.wall_mat);
        for y in [mid, b - 0.4] {
            s.pipe(
                (x, y, z),
                (x, y + 0.5, z),
                r * 1.06,
                GOLD * 0.8,
                Material::Metal,
            );
        }
        let y = b - (b - a) * 0.18;
        s.pipe(
            (x, y, z),
            (x, y + 1.4, z),
            r * 0.9,
            p.trim,
            Material::Painted,
        );
    }

    fn girder(&self, s: &mut Site, (w, l): (f32, f32), (a, b): (f32, f32)) {
        let g = Material::Metal;
        s.deck(
            0.0,
            0.0,
            (w + 0.05, l + 0.06),
            (b - 0.35, b - 0.15),
            GOLD,
            g,
        );
        for e in [-1.0f32, 1.0] {
            let top = s.at(e * w * 0.55, a + 0.4, l + 0.1);
            let out = s.dir(0.0, 1.0);
            s.map.banner(top, out, 1.5, 3.2);
        }
    }

    fn billets(&self, s: &mut Site, w: f32, l: f32) {
        let c = self.palette().casting;
        for k in 0..4 {
            let y = k as f32 * 0.5;
            for i in 0..4 {
                let t = (i as f32 - 1.5) / 2.0;
                let (x, z, half) = if k % 2 == 0 {
                    (t * w, 0.0, (w * 0.22, l))
                } else {
                    (0.0, t * l, (w, l * 0.22))
                };
                let y0 = if k == 0 { -0.5 } else { y };
                let shade = 0.85 + 0.1 * ((i * 2 + k) % 3) as f32;
                s.deck(x, z, half, (y0, y + 0.48), c * shade, Material::Rust);
            }
        }
    }
}
