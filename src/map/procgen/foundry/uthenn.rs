//! The Uthenn's foundry: pale shell on a dark plinth, everything square, a
//! green seam lit along it; furnaces that are stacks of great cubes.

use glam::Vec3;

use super::parts::{Panel, Site};
use super::style::*;
use crate::map::Ambient;
use crate::render::material::Material;

const TUNGSTEN: Vec3 = Vec3::new(0.2, 0.24, 0.3);
const SHELL: Vec3 = Vec3::new(0.44, 0.49, 0.46);
const SHELL_DARK: Vec3 = Vec3::new(0.12, 0.15, 0.14);
const GREEN: Vec3 = Vec3::new(0.15, 0.9, 0.35);

pub struct Uthenn;

fn seam(lit: bool) -> (Vec3, Material) {
    if lit {
        (GREEN, Material::Glow)
    } else {
        (SHELL_DARK, Material::Concrete)
    }
}

/// A cube `a` on a side on `y`, a seam round its middle.
fn cube(s: &mut Site, x: f32, z: f32, a: f32, turn: f32, y: f32, lit: bool) {
    let h = a * 0.5;
    let o = s.deck(x, z, (h, h), (y, y + a), TUNGSTEN, Material::Metal);
    o.yaw += turn;
    o.chip = a * 0.02;
    let (c, m) = seam(lit);
    let mid = y + h;
    let t = (mid - a * 0.035, mid + a * 0.035);
    s.deck(x, z, (h + 0.05, h + 0.05), t, c, m).yaw += turn;
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

    fn panel(&self, s: &mut Site, w: &Panel, k: usize) {
        let (x, z) = w.spot(0.0, 0.0);
        for (y, lit) in [(1.35, k % 4 < 2), (w.top * 0.74, k % 5 == 0)] {
            let (c, m) = seam(lit);
            s.deck(x, z, w.half(w.reach - 0.05, 0.63), (y, y + 0.14), c, m);
        }
        if k.is_multiple_of(3) && w.reach > 1.6 {
            let y = w.top * 0.4;
            s.deck(
                x,
                z,
                w.half(1.1, 0.8),
                (y, y + 2.0),
                TUNGSTEN,
                Material::Metal,
            );
        }
        if k.is_multiple_of(2) {
            let (x, z) = w.spot(-w.reach + 0.5, 0.0);
            cube(s, x, z, 1.0, 0.0, w.top + 0.14, k.is_multiple_of(4));
        }
    }

    fn bay(&self, s: &mut Site, x: f32, z: f32, half: (f32, f32), y: f32) {
        for e in [-1.0f32, 1.0] {
            cube(s, x + e * half.0 * 0.45, z, 1.8, 0.0, y, e > 0.0);
        }
    }

    fn furnace(&self, s: &mut Site, x: f32, z: f32, r: f32, h: f32) {
        let a = r * 1.3;
        let mut y = -0.1;
        for (k, half) in [a * 0.5, a * 0.4, a * 0.3].into_iter().enumerate() {
            let turn = k as f32 * 0.3;
            let tall = if k == 2 { (h - y).max(1.0) } else { half * 1.6 };
            s.deck(x, z, (half, half), (y, y + tall), TUNGSTEN, Material::Metal)
                .yaw += turn;
            let lit = (y + tall, y + tall + 0.3);
            let g = half * 0.9;
            s.deck(x, z, (g, g), lit, GREEN, Material::Glow).yaw += turn + 0.15;
            y += tall + 0.3;
        }
        let m = x + a * 0.5;
        s.deck(m, z, (0.08, 1.5), (-0.1, 2.8), SHELL_DARK, Material::Metal);
        s.deck(m, z, (0.14, 1.1), (0.2, 2.4), GREEN, Material::Glow);
        let p = s.at(x, 5.0, z);
        s.map.sound(Ambient::Machine, p);
    }

    fn chimney(&self, s: &mut Site, x: f32, z: f32, r: f32, (a, b): (f32, f32)) {
        let half = r * 0.8;
        let n = ((b - a) / 5.0).round().max(1.0);
        let seg = (b - a) / n;
        for k in 0..n as i32 {
            let y = a + k as f32 * seg;
            let w = half * (1.0 - 0.25 * k as f32 / n);
            let y0 = if k == 0 { y - 0.2 } else { y + 0.25 };
            let shade = 0.95 - 0.06 * (k % 2) as f32;
            s.deck(
                x,
                z,
                (w, w),
                (y0, y + seg),
                SHELL * shade,
                Material::Painted,
            );
            let (c, m) = seam(k % 2 == 1);
            let t = (y + seg - 0.05, y + seg + 0.3);
            s.deck(x, z, (w * 0.92, w * 0.92), t, c, m);
        }
    }

    fn girder(&self, s: &mut Site, (w, l): (f32, f32), (a, b): (f32, f32)) {
        let y = (a + b) * 0.5;
        s.deck(
            0.0,
            0.0,
            (w + 0.05, l + 0.06),
            (y, y + 0.2),
            GREEN,
            Material::Glow,
        );
        for e in [-1.0f32, 1.0] {
            cube(s, e * w * 0.85, 0.0, 1.8, 0.0, b, true);
        }
    }

    fn billets(&self, s: &mut Site, w: f32, l: f32) {
        let a = w.min(l * 0.5) * 1.8;
        for e in [-1.0f32, 1.0] {
            let lit = s.dice.chance(0.4);
            cube(s, 0.0, e * (a * 0.5 + 0.05), a, 0.0, -0.3, lit);
        }
        let turn = s.dice.range(0.0, 0.7);
        cube(s, 0.0, 0.0, a * 0.6, turn, a - 0.3, false);
    }
}
