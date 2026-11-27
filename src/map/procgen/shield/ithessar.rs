//! The Ithessar's shield generator. They shape plasma with crystal, and
//! this is the same craft as the Kiln's sun, raised for a shield: great
//! crystals of their worked glass, near black, stand on the ring between
//! the doors and lean in over a dais to hold a ball of plasma high between
//! their tips, a seam of it lit up each. Plasma runs in channels in the
//! floor from each crystal to the dais and out to pools beyond the wall,
//! each kept by a ring of smaller crystals. Round it, a ring wall of
//! squared dark stone slit with plasma light, and slender crystal obelisks
//! round the precinct.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::{Vec2, Vec3};

use super::*;
use crate::map::{Ambient, Core, Crystal};
use crate::render::PointLight;
use crate::tank::ithessar::{PLASMA, PLASMA_HOT};

const GLASS: Vec3 = Vec3::new(0.02, 0.065, 0.17);
const STONE: Vec3 = Vec3::new(0.18, 0.19, 0.23);
/// The ring wall's height.
const WALL_H: f32 = 6.0;
/// Where the plasma hangs, over the dais.
const HEART: f32 = 21.0;

impl Map {
    pub(super) fn ithessar_generator(
        &mut self,
        rng: &mut Rng,
        lm: &Landmark,
        base: f32,
    ) -> Vec<Drift> {
        self.slit_wall(rng, lm, base);
        let c = lm.center;
        // The dais, and a ring of plasma round its foot.
        self.pipe(
            Vec3::new(c.x, base, c.y),
            Vec3::new(c.x, base + 0.8, c.y),
            PLINTH_R,
            STONE * 1.2,
            Material::Concrete,
            true,
        );
        self.disc(
            Vec3::new(c.x, base + 0.72, c.y),
            Vec3::Y * 0.03,
            PLINTH_R - 0.1,
            STONE,
            Material::Concrete,
        );
        let n = 32;
        self.glow_paths.push(GlowPath {
            points: (0..=n)
                .map(|k| {
                    let q = c + dir(k as f32 / n as f32 * TAU) * (PLINTH_R + 0.6);
                    Vec3::new(q.x, base + 0.04, q.y)
                })
                .collect(),
            widths: vec![0.35; n + 1],
            color: PLASMA,
        });
        // The great crystals, holding the core.
        let mut prongs = Vec::new();
        for a in between_doors(lm) {
            let p = c + dir(a) * 12.0;
            if shelled(lm, a) {
                // Snapped off: a stump, dark, its top lying beside it.
                self.great_crystal(rng, p, base - 1.2, Vec3::new(0.0, 7.2, 0.0), 2.3, false);
                let fell = dir(a + 0.9);
                self.great_crystal(
                    rng,
                    p + fell * 3.0,
                    base - 0.4,
                    Vec3::new(fell.x * 12.0, 1.2, fell.y * 12.0),
                    1.8,
                    false,
                );
                continue;
            }
            let lean = 0.34f32;
            let inward = -dir(a);
            let axis = Vec3::new(inward.x * lean.sin(), lean.cos(), inward.y * lean.sin()) * 23.7;
            self.great_crystal(rng, p, base - 1.2, axis, 2.3, true);
            let tip = Vec3::new(p.x, base - 1.2, p.y) + axis * 0.95;
            prongs.push(tip);
            self.glow_paths
                .push(channel(lm, a, 9.4, PLINTH_R + 1.0, base, 0.4, PLASMA));
            self.plasma_pool(rng, lm, a, base);
        }
        let heart = Vec3::new(c.x, base + HEART, c.y);
        self.cores.push(Core {
            at: heart,
            radius: 3.0,
            prongs,
        });
        self.sound(Ambient::Core, heart);
        self.pipe(
            heart + Vec3::Y * 3.4,
            heart + Vec3::Y * 60.0,
            0.4,
            PLASMA * 1.2,
            Material::Glow,
            false,
        );
        self.lights.push(PointLight {
            pos: heart,
            color: PLASMA.lerp(PLASMA_HOT, 0.5) * 12.0,
            radius: 26.0,
        });
        for a in precinct_spots(lm, 12).into_iter().flatten() {
            let p = c + dir(a) * lm.precinct;
            let lit = rng.chance(0.5);
            let h = rng.range(7.0, 9.5);
            self.great_crystal(rng, p, base - 0.8, Vec3::new(0.0, h, 0.0), 0.7, lit);
            if lit {
                self.sound(Ambient::Crystal, Vec3::new(p.x, base + 4.0, p.y));
            }
        }
        self.generator_rubble(rng, lm, base, STONE * 1.6)
    }

    /// One of their crystals, worked glass, standing from `foot` (at `y`)
    /// along `axis`, `r` thick; `lit`, a seam of plasma up it.
    fn great_crystal(&mut self, rng: &mut Rng, foot: Vec2, y: f32, axis: Vec3, r: f32, lit: bool) {
        self.crystals.push(Crystal {
            base: Vec3::new(foot.x, y, foot.y),
            axis,
            radius: r,
            sides: 5 + (rng.next_u64() % 3) as u32,
            seed: rng.next_u64() as u32,
            color: GLASS * rng.range(0.85, 1.2),
            lit,
            solid: true,
            raw: false,
        });
    }

    /// The ring wall: squared dark stone, open at each door, a slit of
    /// plasma light down every fourth piece; broken down where shelled.
    fn slit_wall(&mut self, rng: &mut Rng, lm: &Landmark, base: f32) {
        let t = 2.2;
        for (k, (a, half)) in ring(lm, lm.radius, 40).into_iter().enumerate() {
            let d = dir(a);
            let c = lm.center + d * (lm.radius - t * 0.5);
            let yaw = -a + FRAC_PI_2;
            if shelled(lm, a) {
                let h = rng.range(0.8, 3.2);
                self.block(
                    c,
                    Vec2::new(half, t * 0.5),
                    yaw,
                    (base, base + h),
                    STONE * 1.1,
                    Material::Concrete,
                    true,
                )
                .broken = true;
                continue;
            }
            // Stepped: a higher course every other piece.
            let h = if k % 2 == 0 { WALL_H } else { WALL_H - 1.2 };
            self.block(
                c,
                Vec2::new(half, t * 0.5),
                yaw,
                (base, base + h),
                STONE * 1.15,
                Material::Concrete,
                true,
            )
            .chip = 0.01;
            if k % 4 == 1 {
                let p = lm.center + d * (lm.radius + 0.05);
                self.block(
                    p,
                    Vec2::new(0.18, 0.12),
                    yaw,
                    (base + 0.8, base + h - 0.8),
                    PLASMA,
                    Material::Glow,
                    false,
                );
            }
        }
    }

    /// Outside the wall at angle `a`: a pool of plasma kept by a ring of
    /// small crystals leaning out, a channel from it to the wall's foot.
    fn plasma_pool(&mut self, rng: &mut Rng, lm: &Landmark, a: f32, base: f32) {
        let p = lm.center + dir(a) * (lm.radius + 8.5);
        let n = 12;
        self.glow_paths.push(GlowPath {
            points: (0..=n)
                .map(|k| {
                    let q = p + dir(k as f32 / n as f32 * TAU) * 1.4;
                    Vec3::new(q.x, base + 0.04, q.y)
                })
                .collect(),
            widths: vec![1.0; n + 1],
            color: PLASMA,
        });
        for k in 0..5 {
            let out = dir(a + k as f32 / 5.0 * TAU + 0.3);
            let lean = rng.range(0.25, 0.45);
            let h = rng.range(2.2, 3.6);
            let r = rng.range(0.4, 0.65);
            self.great_crystal(
                rng,
                p + out * 3.0,
                base - 0.6,
                Vec3::new(out.x * lean.sin(), lean.cos(), out.y * lean.sin()) * (h + 0.6),
                r,
                k % 2 == 0,
            );
        }
        self.sound(Ambient::River, Vec3::new(p.x, base + 0.5, p.y));
        self.lights.push(PointLight {
            pos: Vec3::new(p.x, base + 1.5, p.y),
            color: PLASMA * 3.0,
            radius: 8.0,
        });
        self.glow_paths.push(channel(
            lm,
            a,
            // (From the edge of the pool.)
            lm.radius + 6.55,
            lm.radius + 0.15,
            base,
            0.3,
            PLASMA,
        ));
    }
}
