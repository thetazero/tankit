//! The Resonant's foundry: ivory halls under steep slate, lancets lit blue;
//! the furnaces are reliquaries, a crystal burning in each behind a rank of
//! organ pipes.

use std::f32::consts::{FRAC_PI_4, PI};

use glam::{Vec2, Vec3};

use super::parts::{Panel, Site};
use super::style::*;
use crate::map::{Ambient, CRYSTAL_LIGHT, Crystal};
use crate::render::material::Material;

const GILT: Vec3 = Vec3::new(0.62, 0.47, 0.22);
const BRONZE: Vec3 = Vec3::new(0.5, 0.35, 0.18);
const IVORY: Vec3 = Vec3::new(0.68, 0.63, 0.53);
const GLASS: Vec3 = Vec3::new(0.05, 0.1, 0.22);
const CLOTH: Vec3 = Vec3::new(0.12, 0.26, 0.7);

pub struct Resonant;

impl Works for Resonant {
    fn palette(&self) -> Palette {
        Palette {
            wall: IVORY,
            wall_mat: Material::Plaster,
            dark: Vec3::new(0.42, 0.38, 0.32),
            trim: GILT,
            floor: Vec3::new(0.42, 0.39, 0.34),
            roof: Vec3::new(0.17, 0.2, 0.28),
            roof_mat: Material::Roof,
            steel: BRONZE * 0.75,
            steel_mat: Material::Metal,
            casting: Vec3::new(0.46, 0.4, 0.3),
            molten: CRYSTAL_LIGHT,
            wagon: Vec3::new(0.15, 0.22, 0.42),
        }
    }

    fn panel(&self, s: &mut Site, w: &Panel, k: usize) {
        let h = w.top;
        let (c, m) = if k % 3 != 1 {
            (CRYSTAL_LIGHT * 0.55, Material::Glow)
        } else {
            (GLASS, Material::Metal)
        };
        let half = (w.reach * 0.26).min(0.9);
        let (x, z) = w.spot(0.0, 0.0);
        s.deck(x, z, w.half(half, 0.61), (h * 0.35, h * 0.72), c, m);
        for (k, f) in [0.72f32, 0.44, 0.2].into_iter().enumerate() {
            let y = h * 0.72 + k as f32 * 0.55;
            s.deck(x, z, w.half(half * f, 0.61), (y, y + 0.55), c, m);
        }
        let sill = (h * 0.35 - 0.2, h * 0.35);
        s.deck(x, z, w.half(half + 0.25, 0.72), sill, GILT, Material::Metal);
        // A pinnacle on the pilaster.
        let (x, z) = w.spot(-w.reach + 0.35, 0.0);
        let o = s.deck(
            x,
            z,
            (0.5, 0.5),
            (h, h + 1.4),
            IVORY * 0.95,
            Material::Plaster,
        );
        o.yaw += FRAC_PI_4;
        s.deck(
            x,
            z,
            (0.15, 0.15),
            (h + 1.3, h + 2.7),
            GILT,
            Material::Metal,
        );
    }

    fn bay(&self, s: &mut Site, x: f32, z: f32, half: (f32, f32), y: f32) {
        // A steep pitch: stepped in courses, its gable ends closed by them.
        let p = self.palette();
        let rise = half.0 * 0.6;
        let n = 9;
        for k in 0..n {
            let u = k as f32 / n as f32;
            let shade = 0.94 + 0.06 * (k % 2) as f32;
            s.deck(
                x,
                z,
                (half.0 * (1.0 - u) + 0.2, half.1 - 0.02 * (k % 3) as f32),
                (y + rise * u, y + rise * (u + 1.0 / n as f32)),
                p.roof * shade,
                p.roof_mat,
            );
        }
        let ridge = (y + rise - 0.05, y + rise + 0.5);
        s.deck(x, z, (0.16, half.1), ridge, GILT, Material::Metal);
    }

    fn furnace(&self, s: &mut Site, x: f32, z: f32, r: f32, h: f32) {
        let (pl, gm) = (Material::Plaster, Material::Metal);
        s.pipe((x, -0.2, z), (x, 1.4, z), r, IVORY * 0.9, pl);
        s.disc((x, 1.38, z), r * 0.96, GILT, gm);
        for (dx, dz, rr, hh) in [
            (0.0, 0.0, 1.5, h - 1.4),
            (0.6, 1.5, 0.8, 4.2),
            (0.8, -1.4, 0.7, 3.4),
        ] {
            let lean = s.dir(dx * 0.15, dz * 0.15);
            let base = s.at(x + dx, 1.0, z + dz);
            let seed = s.dice.next_u64() as u32;
            s.map.crystals.push(Crystal {
                base,
                axis: Vec3::new(lean.x, 1.0, lean.y).normalize() * hh,
                radius: rr,
                sides: 6,
                seed,
                color: CRYSTAL_LIGHT,
                lit: true,
                solid: false,
                raw: false,
            });
        }
        for k in 0..11 {
            let t = (k as f32 - 5.0) / 5.0;
            let d = Vec2::from_angle(PI + t * 1.9) * (r - 0.6);
            let top = h * (1.0 - t.abs() * 0.45);
            let c = if k % 2 == 0 { GILT } else { BRONZE };
            s.pipe((x + d.x, 1.3, z + d.y), (x + d.x, top, z + d.y), 0.5, c, gm);
        }
        let face = (0.1, 1.1);
        s.deck(
            x + r - 0.1,
            z,
            (0.2, 1.0),
            face,
            CRYSTAL_LIGHT,
            Material::Glow,
        );
        let p = s.at(x, 4.0, z);
        s.map.sound(Ambient::Shrine, p);
    }

    fn chimney(&self, s: &mut Site, x: f32, z: f32, r: f32, (a, b): (f32, f32)) {
        // A bell tower of a chimney: ivory, a lit lantern, a slate cone.
        let top = b - 5.0;
        let (pl, gm) = (Material::Plaster, Material::Metal);
        for turn in [0.0, FRAC_PI_4] {
            let h = r * 0.72;
            s.deck(x, z, (h, h), (a - 0.2, top), IVORY * 0.95, pl).yaw += turn;
        }
        for y in [a + (top - a) * 0.5, top - 0.4] {
            s.deck(x, z, (r * 0.86, r * 0.86), (y, y + 0.5), GILT, gm);
        }
        let h = r * 0.35;
        s.deck(
            x,
            z,
            (h, h),
            (top, top + 2.2),
            CRYSTAL_LIGHT * 0.8,
            Material::Glow,
        );
        for (dx, dz) in [(-0.6f32, -0.6f32), (0.6, -0.6), (0.6, 0.6), (-0.6, 0.6)] {
            let (u, v) = (x + dx * r, z + dz * r);
            s.beam((u, top, v), (u, top + 2.4, v), 0.14, GILT, gm);
        }
        let p = self.palette();
        for k in 0..5 {
            let u = k as f32 / 5.0;
            let h = r * 0.95 * (1.0 - u);
            let y = top + 2.3;
            s.deck(
                x,
                z,
                (h, h),
                (y + u * 3.5, y + (u + 0.2) * 3.5),
                p.roof,
                p.roof_mat,
            );
        }
    }

    fn girder(&self, s: &mut Site, (w, l): (f32, f32), (a, b): (f32, f32)) {
        let gm = Material::Metal;
        s.deck(
            0.0,
            0.0,
            (w + 0.05, l + 0.06),
            (a + 0.5, a + 0.8),
            IVORY,
            Material::Plaster,
        );
        // A pointed arch over it, a bell in the arch.
        for e in [-1.0f32, 1.0] {
            s.beam(
                (e * w * 0.7, b - 0.1, 0.0),
                (0.0, b + 4.2, 0.0),
                0.22,
                GILT,
                gm,
            );
            let top = s.at(e * w * 0.5, a + 0.4, l + 0.1);
            let out = s.dir(0.0, 1.0);
            s.map.side_banner(top, out, 1.4, 3.2, CLOTH, false);
        }
        s.pipe((0.0, b + 2.4, 0.0), (0.0, b + 3.3, 0.0), 0.5, BRONZE, gm);
        s.pipe((0.0, b + 1.7, 0.0), (0.0, b + 2.4, 0.0), 0.8, BRONZE, gm);
        let p = s.at(0.0, b + 2.0, 0.0);
        s.map.sound(Ambient::Bell, p);
    }

    fn billets(&self, s: &mut Site, w: f32, l: f32) {
        // Bell metal in round pigs, in a pyramid.
        let r = w / 3.0;
        for row in 0..3 {
            for i in 0..3 - row {
                let x = (i as f32 - (2 - row) as f32 * 0.5) * r * 2.0;
                let y = r + row as f32 * r * 1.73;
                // (Cast octagonal: two bars, an eighth turned.)
                let h = r * 0.9;
                s.beam((x, y, -l), (x, y, l), h * 0.92, BRONZE, Material::Metal);
                s.beam(
                    (x, y, -l + 0.06),
                    (x, y, l - 0.06),
                    h,
                    BRONZE,
                    Material::Metal,
                );
                if let Some(o) = s.map.oboxes.last_mut() {
                    o.roll = std::f32::consts::FRAC_PI_4;
                }
            }
        }
    }
}
