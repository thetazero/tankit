//! The Resonant's foundry: ivory halls under steep slate, buttressed like a
//! church, lancets lit blue; the furnaces are reliquaries, a crystal burning
//! in each behind a rank of organ pipes.

use std::f32::consts::{FRAC_PI_4, TAU};

use glam::{Vec2, Vec3};

use super::skeleton::*;
use super::style::*;
use crate::map::{Ambient, CRYSTAL_LIGHT, Crystal, Map};
use crate::render::material::Material;
use crate::rng::Rng;

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

    fn panel(&self, map: &mut Map, w: &Panel, k: usize) {
        let h = w.top;
        // A lancet between gilt mouldings; a pinnacle on the pilaster.
        let lit = k % 3 != 1;
        let (c, m) = if lit {
            (CRYSTAL_LIGHT * 0.55, Material::Glow)
        } else {
            (GLASS, Material::Metal)
        };
        let half = (w.reach() * 0.26).min(0.9);
        on_faces(map, w, 0.0, half, (h * 0.35, h * 0.72), 0.05, c, m);
        for (k, f) in [0.72f32, 0.44, 0.2].into_iter().enumerate() {
            let y = h * 0.72 + k as f32 * 0.55;
            on_faces(map, w, 0.0, half * f, (y, y + 0.55), 0.05, c, m);
        }
        on_faces(
            map,
            w,
            0.0,
            half + 0.25,
            (h * 0.35 - 0.2, h * 0.35),
            0.16,
            GILT,
            Material::Metal,
        );
        let post = w.center - w.along() * (w.reach() - 0.4);
        map.block(
            post,
            Vec2::splat(0.5),
            FRAC_PI_4,
            (h - 0.2, h + 1.3),
            IVORY * 0.95,
            Material::Plaster,
            false,
        );
        map.block(
            post,
            Vec2::splat(0.16),
            0.0,
            (h + 1.2, h + 2.6),
            GILT,
            Material::Metal,
            false,
        );
    }

    fn bay(&self, map: &mut Map, hall: &Hall, x0: f32, x1: f32) {
        let pal = self.palette();
        let (rise, z) = (pitch(hall), hall.half.y);
        let slope = (rise / z).atan();
        let len = z.hypot(rise) * 0.5 + 0.3;
        for s in [-1.0f32, 1.0] {
            let y = hall.h + 0.2 + rise * 0.5;
            let o = map.block(
                Vec2::new((x0 + x1) * 0.5, hall.center.y + s * z * 0.5),
                Vec2::new((x1 - x0) * 0.5 - if s > 0.0 { 0.04 } else { 0.0 }, len),
                0.0,
                (y - 0.16, y + 0.16),
                pal.roof * if s > 0.0 { 1.0 } else { 0.94 },
                pal.roof_mat,
                true,
            );
            o.tilt = s * slope;
        }
        slab(
            map,
            Vec2::new((x0 + x1) * 0.5, hall.center.y),
            Vec2::new((x1 - x0) * 0.5, 0.2),
            (hall.h + rise + 0.1, hall.h + rise + 0.75),
            GILT,
            Material::Metal,
            false,
        );
    }

    fn gables(&self, map: &mut Map, hall: &Hall) {
        let rise = pitch(hall);
        for s in [-1.0f32, 1.0] {
            let x = hall.center.x + s * (hall.half.x - WALL_T * 0.5);
            let n = 12;
            for k in 0..n {
                let u = k as f32 / n as f32;
                slab(
                    map,
                    Vec2::new(x, hall.center.y),
                    Vec2::new(0.45, hall.half.y * (1.0 - u) - 0.5),
                    (hall.h + rise * u, hall.h + rise * (u + 1.0 / n as f32)),
                    IVORY * 0.97,
                    Material::Plaster,
                    true,
                );
            }
            // A round window in it.
            map.disc(
                at(Vec2::new(x, hall.center.y), hall.h + rise * 0.4),
                Vec3::X * 0.55,
                rise * 0.2,
                CRYSTAL_LIGHT * 0.6,
                Material::Glow,
            );
            map.segment(
                at(Vec2::new(x, hall.center.y), hall.h + rise - 0.2),
                at(Vec2::new(x, hall.center.y), hall.h + rise + 3.0),
                0.14,
                GILT,
                Material::Metal,
                false,
            );
        }
    }

    fn furnace(&self, map: &mut Map, f: &Round, rng: &mut Rng) {
        // A drum of ivory open in front, the crystal standing in it.
        map.pipe(
            at(f.at, -0.2),
            at(f.at, 1.4),
            f.r,
            IVORY * 0.9,
            Material::Plaster,
            false,
        );
        map.disc(
            at(f.at, 1.38),
            Vec3::Y * 0.06,
            f.r * 0.97,
            GILT,
            Material::Metal,
        );
        for (off, r, h) in [
            (Vec2::ZERO, 1.5, f.h - 1.4),
            (Vec2::new(-1.5, -0.6), 0.8, 4.2),
            (Vec2::new(1.4, -0.8), 0.7, 3.4),
        ] {
            map.crystals.push(Crystal {
                base: at(f.at + off, 1.0),
                axis: Vec3::new(off.x * 0.15, 1.0, off.y * 0.15).normalize() * h,
                radius: r,
                sides: 6,
                seed: rng.next_u64() as u32,
                color: CRYSTAL_LIGHT,
                lit: true,
                solid: false,
                raw: false,
            });
        }
        // The pipes, a rank round its back, tallest in the middle.
        for k in 0..11 {
            let t = (k as f32 - 5.0) / 5.0;
            let a = std::f32::consts::FRAC_PI_2 + t * 1.9;
            let p = f.at + Vec2::from_angle(a) * (f.r - 0.6);
            let top = f.h * (1.0 - t.abs() * 0.45);
            map.pipe(
                at(p, 1.3),
                at(p, top),
                0.5,
                if k % 2 == 0 { GILT } else { BRONZE },
                Material::Metal,
                false,
            );
        }
        // An altar front, lit.
        let mouth = f.at - Vec2::Y * (f.r - 0.1);
        slab(
            map,
            mouth,
            Vec2::new(1.0, 0.2),
            (0.1, 1.1),
            CRYSTAL_LIGHT,
            Material::Glow,
            false,
        );
        map.sound(Ambient::Shrine, at(f.at, 4.0));
    }

    fn chimney(&self, map: &mut Map, c: &Round) {
        // A bell tower of a chimney: ivory, a gilt lantern, a slate cone.
        let top = c.h - 5.0;
        for (k, (y0, y1, r)) in [(-0.3, top * 0.55, 1.0), (top * 0.55, top, 0.86)]
            .into_iter()
            .enumerate()
        {
            for yaw in [0.0, FRAC_PI_4] {
                map.block(
                    c.at,
                    Vec2::splat(c.r * r * 0.72),
                    yaw,
                    (y0, y1),
                    IVORY * (0.95 - 0.05 * k as f32),
                    Material::Plaster,
                    false,
                );
            }
            map.block(
                c.at,
                Vec2::splat(c.r * r * 0.86),
                0.0,
                (y1 - 0.4, y1 + 0.1),
                GILT,
                Material::Metal,
                false,
            );
        }
        for k in 0..4 {
            let p = c.at + Vec2::from_angle(FRAC_PI_4 + k as f32 * TAU / 4.0) * c.r * 0.8;
            map.segment(
                at(p, top),
                at(p, top + 2.4),
                0.16,
                GILT,
                Material::Metal,
                false,
            );
        }
        map.block(
            c.at,
            Vec2::splat(c.r * 0.35),
            FRAC_PI_4,
            (top, top + 2.2),
            CRYSTAL_LIGHT * 0.8,
            Material::Glow,
            false,
        );
        for k in 0..5 {
            let u = k as f32 / 5.0;
            map.block(
                c.at,
                Vec2::splat(c.r * 0.95 * (1.0 - u)),
                0.0,
                (top + 2.3 + u * 3.5, top + 2.3 + (u + 0.2) * 3.5),
                self.palette().roof,
                Material::Roof,
                false,
            );
        }
    }

    fn gantry(&self, map: &mut Map, g: &Gantry) {
        // A pointed arch over the beam, a bell in it.
        slab(
            map,
            g.at,
            Vec2::new(0.9, g.span + 1.6),
            (g.h, g.h + 1.6),
            IVORY,
            Material::Plaster,
            true,
        );
        for s in [-1.0f32, 1.0] {
            slab(
                map,
                g.at + Vec2::X * s * 0.95,
                Vec2::new(0.1, g.span + 1.7),
                (g.h + 1.2, g.h + 1.5),
                GILT,
                Material::Metal,
                false,
            );
            let foot = g.at + Vec2::Y * s * g.span;
            map.segment(
                at(foot, g.h + 1.4),
                at(g.at, g.h + 6.4),
                0.28,
                GILT,
                Material::Metal,
                false,
            );
            map.side_banner(
                at(foot - Vec2::Y * s * 2.4 - Vec2::X * 1.05, g.h + 0.9),
                Vec2::NEG_X,
                1.5,
                4.0,
                CLOTH,
                false,
            );
        }
        let bell = |y: f32| at(g.at, g.h + y);
        map.segment(bell(5.4), bell(6.4), 0.1, GILT, Material::Metal, false);
        map.pipe(bell(4.3), bell(5.4), 0.6, BRONZE, Material::Metal, false);
        map.pipe(bell(3.5), bell(4.3), 0.95, BRONZE, Material::Metal, false);
        map.disc(bell(5.36), Vec3::Y * 0.04, 0.55, BRONZE, Material::Metal);
        map.sound(Ambient::Bell, bell(4.0));
    }

    fn fence(&self, map: &mut Map, c: Vec2, inward: Vec2, top: f32, k: usize) {
        if k.is_multiple_of(3) {
            let half = inward.perp().abs() * 0.5 + inward.abs() * (FENCE_T + 0.06);
            slab(
                map,
                c,
                half,
                (top - 3.2, top - 1.0),
                GLASS,
                Material::Metal,
                false,
            );
            let sill = half + Vec2::splat(0.1);
            slab(
                map,
                c,
                sill,
                (top - 3.4, top - 3.2),
                GILT,
                Material::Metal,
                false,
            );
        }
    }

    fn billets(&self, map: &mut Map, s: &Stock, g: f32, _rng: &mut Rng) {
        // Bell metal in round pigs, stacked in a pyramid.
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
        let r = s.half.y / 3.0;
        for row in 0..3 {
            for i in 0..3 - row {
                let z = (i as f32 - (2 - row) as f32 * 0.5) * r * 2.0;
                let y = g + r + row as f32 * r * 1.73;
                let p = s.at + Vec2::Y * z;
                let (a, b) = (p - Vec2::X * s.half.x, p + Vec2::X * s.half.x);
                map.pipe(at(a, y), at(b, y), r, BRONZE, Material::Metal, false);
                for e in [a, b] {
                    map.disc(
                        at(e, y),
                        Vec3::X * 0.05,
                        r * 0.9,
                        BRONZE * 0.8,
                        Material::Metal,
                    );
                }
            }
        }
    }
}

/// How high a hall's ridge stands over its walls.
fn pitch(hall: &Hall) -> f32 {
    hall.half.y * 0.55
}
