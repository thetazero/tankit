//! The Ithessar's foundry: squared dark stone slit with plasma, crystal
//! standing on it; the furnaces are balls of plasma held between crystals.

use std::f32::consts::{FRAC_PI_4, TAU};

use glam::{Vec2, Vec3};

use super::parts::{Panel, Site};
use super::style::*;
use crate::map::{Ambient, Core, Crystal};
use crate::render::material::Material;
use crate::tank::ithessar::PLASMA;

const GLASS: Vec3 = Vec3::new(0.02, 0.065, 0.17);
const STONE: Vec3 = Vec3::new(0.18, 0.19, 0.23);
const BRONZE: Vec3 = Vec3::new(0.28, 0.2, 0.14);

pub struct Ithessar;

/// One of their crystals, from (x, y, z) along `axis` in the site's frame.
/// Never solid: the part's own boxes stop tanks.
fn crystal(s: &mut Site, at: (f32, f32, f32), axis: Vec3, radius: f32, lit: bool) {
    let d = s.dir(axis.x, axis.z);
    let base = s.at(at.0, at.1, at.2);
    let seed = s.dice.next_u64() as u32;
    s.map.crystals.push(Crystal {
        base,
        axis: Vec3::new(d.x, axis.y, d.y),
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

    fn panel(&self, s: &mut Site, w: &Panel, k: usize) {
        let (x, z) = w.spot(0.0, 0.0);
        if k.is_multiple_of(2) {
            let y = (1.8, (w.top - 1.2).max(2.2));
            s.deck(x, z, w.half(0.16, 0.62), y, PLASMA, Material::Glow);
            if w.reach > 1.5 {
                let (ox, oz) = w.out();
                let up = Vec3::new(ox * 0.25, 1.0, oz * 0.25) * 3.2;
                crystal(s, (x, w.top - 0.4, z), up, 0.5, k.is_multiple_of(4));
            }
        } else {
            let y = (w.top + 0.1, w.top + 1.2);
            let half = w.half(w.reach - 0.4, 0.55);
            s.deck(x, z, half, y, STONE * 1.2, Material::Concrete);
        }
    }

    fn bay(&self, s: &mut Site, x: f32, z: f32, half: (f32, f32), y: f32) {
        // Ribs of crystal leaning in from both sides.
        for e in [-1.0f32, 1.0] {
            let axis = Vec3::new(-e * (half.0 - 1.6), 5.0, 0.0);
            let lit = s.dice.chance(0.4);
            crystal(s, (x + e * (half.0 - 0.5), y - 0.3, z), axis, 0.9, lit);
        }
    }

    fn furnace(&self, s: &mut Site, x: f32, z: f32, r: f32, h: f32) {
        let cast = Material::Concrete;
        s.pipe((x, -0.2, z), (x, 1.0, z), r, STONE * 1.2, cast);
        s.disc((x, 0.98, z), r * 0.96, STONE, cast);
        let y = h * 0.66;
        let mut prongs = Vec::new();
        for k in 0..5 {
            let d = Vec2::from_angle(std::f32::consts::PI + k as f32 * TAU / 5.0);
            let foot = (x + d.x * (r - 0.9), 0.6, z + d.y * (r - 0.9));
            let tip = (x + d.x * 1.9, y + 0.7, z + d.y * 1.9);
            let axis = Vec3::new(tip.0 - foot.0, tip.1 - foot.1, tip.2 - foot.2);
            crystal(s, foot, axis, 0.95, k % 2 == 0);
            prongs.push(s.at(tip.0, tip.1, tip.2));
        }
        let at = s.at(x, y, z);
        s.map.cores.push(Core {
            at,
            radius: 1.5,
            prongs,
        });
        s.map.sound(Ambient::Core, at);
    }

    fn chimney(&self, s: &mut Site, x: f32, z: f32, r: f32, (a, b): (f32, f32)) {
        // A pylon of stone, a long crystal standing in its top.
        let top = a + (b - a) * 0.55;
        for turn in [0.0, FRAC_PI_4] {
            let h = r * 0.7;
            let c = STONE * 1.25;
            s.deck(x, z, (h, h), (a - 0.2, top), c, Material::Concrete)
                .yaw += turn;
        }
        for (dx, dz) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let h = r * 0.92;
            let y = (a + 0.8, top - 0.8);
            s.deck(
                x + dx * h,
                z + dz * h,
                (0.13, 0.13),
                y,
                PLASMA,
                Material::Glow,
            );
        }
        let axis = Vec3::Y * (b - top + 1.0);
        crystal(s, (x, top - 1.0, z), axis, r * 0.75, true);
    }

    fn girder(&self, s: &mut Site, (w, l): (f32, f32), (a, b): (f32, f32)) {
        let y = a + 0.35;
        s.deck(
            0.0,
            0.0,
            (w - 0.6, l + 0.06),
            (y, y + 0.2),
            PLASMA,
            Material::Glow,
        );
        for e in [-1.0f32, 1.0] {
            let axis = Vec3::new(e * 1.2, 3.6, 0.0);
            crystal(s, (e * w * 0.9, b - 0.4, 0.0), axis, 0.6, e > 0.0);
        }
        let p = s.at(0.0, b, 0.0);
        s.map.sound(Ambient::Crystal, p);
    }

    fn billets(&self, s: &mut Site, w: f32, l: f32) {
        // Crystals cut to length, lying in a rack.
        for z in [-0.6f32, 0.6] {
            for x in [-1.0f32, 1.0] {
                let y = (-0.6, 1.9);
                let at = (x * (w - 0.12), z * l);
                s.deck(at.0, at.1, (0.12, 0.12), y, BRONZE, Material::Metal);
            }
        }
        for (x, y) in [(-0.5, 0.5), (0.5, 0.5), (-0.4, 1.35), (0.45, 1.4)] {
            let lit = s.dice.chance(0.3);
            let axis = Vec3::Z * (l * 2.0 - 0.2);
            crystal(s, (x * w, y, -l + 0.1), axis, 0.5, lit);
        }
    }
}
