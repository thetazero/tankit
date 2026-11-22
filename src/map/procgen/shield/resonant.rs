//! The Resonant's shield generator. They don't build machines, they house
//! a crystal: this is a reliquary, a great cut crystal held in a gilt spire
//! of pointed arches over an ivory plinth, worn ivory plates floating clear
//! of it on shafts of its light. Round it, behind a screen wall set with
//! crystal glass, banks of organ pipes lit blue at the mouths; outside,
//! bells hung in gilt frames, and shrines of cut crystal round the precinct.
//! Nobody can say whether it works by the rite or by something else.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::{Mat3, Quat, Vec2, Vec3};

use super::*;
use crate::map::landmarks::Rig;
use crate::map::{Ambient, CRYSTAL_LIGHT, Crystal};
use crate::render::PointLight;

const IVORY: Vec3 = Vec3::new(0.8, 0.76, 0.66);
const GILT: Vec3 = Vec3::new(0.62, 0.47, 0.22);
const BRONZE: Vec3 = Vec3::new(0.5, 0.35, 0.18);
const CLOTH: Vec3 = Vec3::new(0.12, 0.26, 0.7);
/// The screen wall's height.
const WALL_H: f32 = 5.5;
/// Where the crystal floats, over the plinth's foot.
const HEART: f32 = 10.0;
/// The spire's tip.
const SPIRE: f32 = 27.0;

fn frame(x: Vec3, y: Vec3) -> Quat {
    Quat::from_mat3(&Mat3::from_cols(x, y, x.cross(y)))
}

impl Map {
    pub(super) fn resonant_generator(
        &mut self,
        rng: &mut Rng,
        lm: &Landmark,
        base: f32,
    ) -> Vec<Drift> {
        self.screen_wall(rng, lm, base);
        self.reliquary(rng, lm, base);
        for a in between_doors(lm) {
            let wrecked = shelled(lm, a);
            self.organ(lm, a, base, wrecked);
            if !wrecked {
                self.bell_frame(lm, a, base);
            }
        }
        for a in precinct_spots(lm, 12).into_iter().flatten() {
            self.shrine(rng, lm, a, base);
        }
        self.generator_rubble(rng, lm, base, IVORY * 0.9)
    }

    /// The screen wall: ivory, open at each door, a pinnacle capped in gilt
    /// on every third piece, a panel of crystal glass lit in the others.
    fn screen_wall(&mut self, rng: &mut Rng, lm: &Landmark, base: f32) {
        let t = 1.6;
        for (k, (a, half)) in ring(lm, lm.radius, 40).into_iter().enumerate() {
            let d = dir(a);
            let c = lm.center + d * (lm.radius - t * 0.5);
            let yaw = -a + FRAC_PI_2;
            if shelled(lm, a) {
                let h = rng.range(0.8, 3.0);
                self.block(
                    c,
                    Vec2::new(half, t * 0.5),
                    yaw,
                    (base, base + h),
                    IVORY * 0.9,
                    Material::Plaster,
                    true,
                )
                .broken = true;
                continue;
            }
            self.block(
                c,
                Vec2::new(half, t * 0.5),
                yaw,
                (base, base + WALL_H),
                IVORY,
                Material::Plaster,
                true,
            )
            .chip = 0.01;
            self.block(
                c,
                Vec2::new(half * 1.04, t * 0.5 + 0.2),
                yaw,
                (base + WALL_H, base + WALL_H + 0.4),
                GILT,
                Material::Metal,
                false,
            );
            if k % 3 == 0 {
                let p = lm.center + d * (lm.radius + 0.3);
                self.block(
                    p,
                    Vec2::splat(0.6),
                    yaw,
                    (base, base + WALL_H + 3.5),
                    IVORY * 0.95,
                    Material::Plaster,
                    true,
                );
                self.block(
                    p,
                    Vec2::splat(0.35),
                    yaw + 0.785,
                    (base + WALL_H + 3.5, base + WALL_H + 4.6),
                    GILT,
                    Material::Metal,
                    false,
                );
            } else {
                let p = lm.center + d * (lm.radius + 0.06);
                self.block(
                    p,
                    Vec2::new(half * 0.55, 0.1),
                    yaw,
                    (base + 1.6, base + WALL_H - 0.9),
                    CRYSTAL_LIGHT * 0.8,
                    Material::Glow,
                    false,
                );
            }
        }
    }

    /// The reliquary: an ivory plinth, eight gilt posts rising round it and
    /// leaning in to a spire, the crystal floating in the middle, plates
    /// floating round it, and its light thrown up to the sky.
    fn reliquary(&mut self, rng: &mut Rng, lm: &Landmark, base: f32) {
        let c = lm.center;
        let at = |y: f32| Vec3::new(c.x, base + y, c.y);
        self.drum_at(at(0.0), at(0.6), PLINTH_R, IVORY * 0.92);
        self.drum_at(at(0.6), at(1.4), PLINTH_R - 1.5, IVORY);
        let posts = 8;
        let (r, bend) = (3.6, 18.0);
        for k in 0..posts {
            let a = (k as f32 + 0.5) / posts as f32 * TAU;
            let p = c + dir(a) * r;
            let foot = Vec3::new(p.x, base + 1.4, p.y);
            let knee = Vec3::new(p.x, base + bend, p.y);
            self.pipe(foot, knee, 0.35, GILT, Material::Metal, true);
            self.pipe(knee, at(SPIRE), 0.28, GILT, Material::Metal, false);
            // A pointed arch between this post and the next, at the knee.
            let q = c + dir(a + TAU / posts as f32) * r;
            let mid = (p + q) * 0.5 + (p + q - c * 2.0).normalize() * 0.15;
            let apex = Vec3::new(mid.x, base + bend - 1.6, mid.y);
            for e in [p, q] {
                self.segment(
                    Vec3::new(e.x, base + bend - 4.0, e.y),
                    apex,
                    0.12,
                    GILT,
                    Material::Metal,
                    false,
                );
            }
        }
        self.disc(at(SPIRE + 0.1), Vec3::Y * 0.1, 0.5, GILT, Material::Metal);
        // The crystal, cut, point up and point down.
        for (len, up) in [(7.0f32, 1.0f32), (4.5, -1.0)] {
            self.crystals.push(Crystal {
                base: at(HEART),
                axis: Vec3::Y * len * up,
                radius: 1.5,
                sides: 6,
                seed: rng.next_u64() as u32,
                color: CRYSTAL_LIGHT,
                lit: true,
                solid: false,
                raw: false,
            });
        }
        self.sound(Ambient::Altar, at(HEART));
        // The plates, and the light holding each.
        for k in 0..6 {
            let a = k as f32 / 6.0 * TAU + 0.3;
            let radial = Vec3::new(a.cos(), 0.0, a.sin());
            let tangent = Vec3::new(-a.sin(), 0.0, a.cos());
            let y = HEART + if k % 2 == 0 { 2.2 } else { -0.8 };
            let rig = Rig {
                pos: at(y) + radial * 2.7,
                rot: frame(tangent, Vec3::Y),
            };
            rig.part(
                self,
                Vec3::ZERO,
                Vec3::new(0.8, 1.5, 0.09),
                Quat::IDENTITY,
                IVORY,
                Material::Plaster,
                false,
                0.0,
            );
            self.segment(
                at(y) + radial * 1.0,
                at(y) + radial * 2.6,
                0.06,
                CRYSTAL_LIGHT,
                Material::Glow,
                false,
            );
        }
        self.pipe(
            at(SPIRE + 0.3),
            at(SPIRE + 60.0),
            0.4,
            CRYSTAL_LIGHT * 1.3,
            Material::Glow,
            false,
        );
        self.lights.push(PointLight {
            pos: at(HEART),
            color: CRYSTAL_LIGHT * 14.0,
            radius: 24.0,
        });
    }

    /// A short round drum, solid, from `a` up to `b`.
    fn drum_at(&mut self, a: Vec3, b: Vec3, r: f32, color: Vec3) {
        self.pipe(a, b, r, color, Material::Plaster, true);
        self.disc(
            b - Vec3::Y * 0.08,
            Vec3::Y * 0.03,
            r - 0.08,
            color * 0.95,
            Material::Plaster,
        );
    }

    /// A bank of organ pipes at angle `a` inside the wall: an ivory chest,
    /// gilt pipes rising to the middle one, their mouths lit, and light
    /// running from it in the floor to the plinth. A `wrecked` one's pipes
    /// are snapped off and dark.
    fn organ(&mut self, lm: &Landmark, a: f32, base: f32, wrecked: bool) {
        let d = dir(a);
        let side = Vec2::new(-d.y, d.x);
        let rb = lm.radius - 7.5;
        let p = lm.center + d * rb;
        self.block(
            p,
            Vec2::new(1.5, 4.2),
            -a,
            (base, base + 1.6),
            IVORY * 0.9,
            Material::Plaster,
            true,
        );
        for i in -3i32..=3 {
            let q = p + side * i as f32 * 1.1;
            let h = if wrecked {
                2.5 + (i.abs() as f32) * 0.4
            } else {
                12.0 - i.abs() as f32 * 1.8
            };
            self.pipe(
                Vec3::new(q.x, base + 1.6, q.y),
                Vec3::new(q.x, base + 1.6 + h, q.y),
                0.42,
                GILT,
                Material::Metal,
                false,
            );
            if !wrecked {
                let m = q + d * -0.4;
                self.block(
                    m,
                    Vec2::new(0.06, 0.22),
                    -a,
                    (base + 2.6, base + 3.2),
                    CRYSTAL_LIGHT,
                    Material::Glow,
                    false,
                );
            }
        }
        if wrecked {
            return;
        }
        self.sound(Ambient::Shrine, Vec3::new(p.x, base + 4.0, p.y));
        self.glow_paths.push(channel(
            lm,
            a,
            rb - 1.7,
            PLINTH_R + 0.3,
            base,
            0.4,
            CRYSTAL_LIGHT,
        ));
    }

    /// Outside the wall at angle `a`: a bronze bell hung in a gilt frame.
    fn bell_frame(&mut self, lm: &Landmark, a: f32, base: f32) {
        let d = dir(a);
        let side = Vec2::new(-d.y, d.x);
        let p = lm.center + d * (lm.radius + 8.5);
        let top = base + 8.0;
        for s in [-1.0f32, 1.0] {
            let q = p + side * s * 2.6;
            self.block(
                q,
                Vec2::splat(0.6),
                -a,
                (base, base + 0.8),
                IVORY * 0.9,
                Material::Plaster,
                true,
            );
            self.pipe(
                Vec3::new(q.x, base + 0.7, q.y),
                Vec3::new(q.x, top, q.y),
                0.25,
                GILT,
                Material::Metal,
                true,
            );
        }
        let (l, r) = (p - side * 2.6, p + side * 2.6);
        self.segment(
            Vec3::new(l.x, top, l.y),
            Vec3::new(r.x, top, r.y),
            0.22,
            GILT,
            Material::Metal,
            false,
        );
        // The bell: its crown, waist and flared lip.
        let at = |y: f32| Vec3::new(p.x, y, p.y);
        self.segment(at(top), at(top - 0.6), 0.08, GILT, Material::Metal, false);
        self.pipe(
            at(top - 1.2),
            at(top - 0.6),
            0.55,
            BRONZE,
            Material::Metal,
            false,
        );
        self.pipe(
            at(top - 2.4),
            at(top - 1.2),
            0.9,
            BRONZE,
            Material::Metal,
            false,
        );
        self.pipe(
            at(top - 3.1),
            at(top - 2.4),
            1.25,
            BRONZE * 0.9,
            Material::Metal,
            false,
        );
        self.disc(at(top - 0.62), Vec3::Y * 0.03, 0.5, BRONZE, Material::Metal);
        self.sound(Ambient::Bell, at(top - 1.5));
    }

    /// A shrine on the precinct at angle `a`: cut crystals in a cluster, a
    /// tall ivory post over them hung with a blue banner.
    fn shrine(&mut self, rng: &mut Rng, lm: &Landmark, a: f32, base: f32) {
        let d = dir(a);
        let p = lm.center + d * lm.precinct;
        for k in 0..3 {
            let off = dir(a + k as f32 * 2.1 + 0.4) * 1.3;
            let lean = rng.range(0.05, 0.25);
            let h = rng.range(1.6, 3.0);
            self.crystals.push(Crystal {
                base: Vec3::new(p.x + off.x, base - 0.6, p.y + off.y),
                axis: Vec3::new(off.x * lean, 1.0, off.y * lean).normalize() * (h + 0.6),
                radius: rng.range(0.35, 0.6),
                sides: 6,
                seed: rng.next_u64() as u32,
                color: CRYSTAL_LIGHT,
                lit: k == 0,
                solid: true,
                raw: false,
            });
        }
        let q = p - d * 1.9;
        self.block(
            q,
            Vec2::splat(0.35),
            -a,
            (base, base + 9.0),
            IVORY * 0.95,
            Material::Plaster,
            true,
        );
        self.block(
            q - d * 0.4,
            Vec2::new(0.04, 0.8),
            -a,
            (base + 4.4, base + 8.6),
            CLOTH,
            Material::Cloth,
            false,
        );
        self.block(
            q,
            Vec2::splat(0.5),
            -a,
            (base + 9.0, base + 9.3),
            GILT,
            Material::Metal,
            false,
        );
    }
}
