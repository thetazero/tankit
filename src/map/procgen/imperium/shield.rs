//! The Imperium's shield generator (see `procgen::shield`): the standard
//! model they put up on every world they hold, the same from one city to
//! the next.
//!
//! It stands in the landmark's place and keeps to its envelope: a round
//! blast wall, lower than the rotunda's drum, open on each door; inside, a
//! steel mast on a stepped plinth, its capacitor rings glowing the
//! shield's pale blue, crowned with gold emitter vanes and the beam it
//! throws up to the sky. Round it, the plant that feeds it: capacitor
//! banks inside the wall with conduits up to the mast, transformer houses
//! outside, and a ring of pylons strung with cable on the precinct. The
//! side it fell on (war damage) is shelled: the wall broken to stumps, the
//! bank there wrecked, rubble heaped round it.

use super::*;
use crate::map::GlowPath;
use crate::map::procgen::shield::{between_doors, in_door, shelled};
use crate::render::PointLight;

/// The shield's light: a cold, pale blue.
const SHIELD: Vec3 = Vec3::new(0.35, 0.72, 1.0);
/// Dark steel: the mast and the machinery.
const STEEL: Vec3 = Vec3::new(0.24, 0.25, 0.27);

/// The blast wall's height.
const WALL_H: f32 = 7.0;
/// The mast's top (above the plinth's foot), under the crown.
const MAST_TOP: f32 = 36.0;
/// The mast's radius.
const MAST_R: f32 = 2.6;
/// The plinth's foot: radius, and its top.
const PLINTH_R: f32 = 6.5;
const PLINTH_TOP: f32 = 1.1;

impl Map {
    /// The shield generator in the landmark's place (see the module).
    pub(in crate::map::procgen) fn imperium_generator(
        &mut self,
        rng: &mut Rng,
        lm: &Landmark,
        base: f32,
    ) -> Vec<Drift> {
        let banks = between_doors(lm);
        self.blast_wall(rng, lm, base);
        self.generator_mast(lm, base);
        for &a in &banks {
            let wrecked = shelled(lm, a);
            self.capacitor_bank(rng, lm, a, base, wrecked);
            self.transformer(lm, a, base, wrecked);
        }
        self.pylons(lm, base);
        self.generator_rubble(rng, lm, base, CONCRETE)
    }

    /// The blast wall: a ring of concrete, open on each door, a stone
    /// coping on top, buttresses, banners outside, and a pair of gate posts
    /// flanking each door. Broken down to stumps where it was shelled.
    fn blast_wall(&mut self, rng: &mut Rng, lm: &Landmark, base: f32) {
        let (center, radius) = (lm.center, lm.radius);
        let segs = 40;
        let r_mid = radius - DRUM_T * 0.5;
        let seg_half = radius * PI / segs as f32 * 1.02;
        for k in 0..segs {
            let a = (k as f32 + 0.5) / segs as f32 * TAU;
            if in_door(lm, a) {
                continue;
            }
            let dir = Vec2::new(a.cos(), a.sin());
            let c = center + dir * r_mid;
            let yaw = -a + FRAC_PI_2;
            let wall = |y0: f32, y1: f32| {
                (
                    Vec3::new(c.x - seg_half, base + y0, c.y - DRUM_T * 0.5),
                    Vec3::new(c.x + seg_half, base + y1, c.y + DRUM_T * 0.5),
                )
            };
            if shelled(lm, a) {
                let (l, u) = wall(0.0, rng.range(0.9, 3.8));
                let shade = rng.range(0.88, 1.0);
                self.slab(l, u, yaw, CONCRETE * shade, Material::Concrete, true);
                self.oboxes.last_mut().unwrap().broken = true;
                continue;
            }
            let (l, u) = wall(0.0, WALL_H);
            self.slab(l, u, yaw, CONCRETE * 0.96, Material::Concrete, true);
            self.oboxes.last_mut().unwrap().chip = 0.01;
            // The coping, overhanging both faces.
            let p = center + dir * (r_mid + 0.05);
            self.slab(
                Vec3::new(
                    p.x - seg_half * 1.05,
                    base + WALL_H,
                    p.y - DRUM_T * 0.5 - 0.35,
                ),
                Vec3::new(
                    p.x + seg_half * 1.05,
                    base + WALL_H + 0.7,
                    p.y + DRUM_T * 0.5 + 0.35,
                ),
                yaw,
                STONE * 0.9,
                Material::Concrete,
                false,
            );
            if k % 4 == 1 {
                let p = center + dir * (radius + 0.6);
                self.slab(
                    Vec3::new(p.x - 1.0, base, p.y - 0.9),
                    Vec3::new(p.x + 1.0, base + WALL_H + 1.0, p.y + 0.9),
                    yaw,
                    CONCRETE * 0.9,
                    Material::Concrete,
                    true,
                );
            }
            if k % 4 == 3 {
                let p = center + dir * (radius + 0.05);
                self.banner(Vec3::new(p.x, base + WALL_H - 0.3, p.y), dir, 2.4, 5.6);
            }
        }
        // Gate posts either side of each door, a gold disc on each, a lamp
        // in the shield's blue on its inner face.
        for &d in &lm.doors {
            let side = Vec2::new(-d.y, d.x);
            for s in [-1.0f32, 1.0] {
                let p = center + d * (radius + 1.6) + side * s * 7.6;
                if shelled(lm, angle(p - center)) {
                    continue;
                }
                let yaw = -angle(d) + FRAC_PI_2;
                self.slab(
                    Vec3::new(p.x - 1.4, base, p.y - 1.4),
                    Vec3::new(p.x + 1.4, base + WALL_H + 3.0, p.y + 1.4),
                    yaw,
                    CONCRETE * 0.9,
                    Material::Concrete,
                    true,
                );
                self.slab(
                    Vec3::new(p.x - 1.7, base + WALL_H + 3.0, p.y - 1.7),
                    Vec3::new(p.x + 1.7, base + WALL_H + 3.6, p.y + 1.7),
                    yaw,
                    STONE * 0.9,
                    Material::Concrete,
                    false,
                );
                let out = p + d * 1.45;
                self.disc(
                    Vec3::new(out.x, base + WALL_H + 1.2, out.y),
                    Vec3::new(d.x, 0.0, d.y) * 0.06,
                    0.9,
                    GOLD,
                    Material::Metal,
                );
                let face = p - side * s * 1.45;
                self.slab(
                    Vec3::new(face.x - 0.12, base + 4.0, face.y - 0.5),
                    Vec3::new(face.x + 0.12, base + 5.4, face.y + 0.5),
                    -angle(side),
                    SHIELD,
                    Material::Glow,
                    false,
                );
            }
        }
    }

    /// The mast: a stepped plinth, a steel column braced by four concrete
    /// fins, ringed with glowing capacitor bands, the emblem facing each
    /// door; on top, the crown of gold vanes round the emitter, and the
    /// beam it throws up to the sky.
    fn generator_mast(&mut self, lm: &Landmark, base: f32) {
        let c = lm.center;
        let at = |y: f32| Vec3::new(c.x, base + y, c.y);
        self.sound(Ambient::Machine, at(3.0));
        self.drum(at(0.0), at(0.55), PLINTH_R, STONE * 0.9);
        // Down inside the foot: as two low steps, with the mast standing over
        // them, bots would take it for ground they can drive on.
        self.drum(at(0.1), at(PLINTH_TOP), PLINTH_R - 1.4, CONCRETE);
        self.pipe(
            at(PLINTH_TOP),
            at(MAST_TOP),
            MAST_R,
            STEEL,
            Material::Metal,
            true,
        );
        // The fins, on the diagonals.
        for k in 0..4 {
            let a = (k as f32 + 0.5) * FRAC_PI_2;
            let dir = Vec2::new(a.cos(), a.sin());
            for (r0, r1, y0, y1) in [
                (MAST_R - 0.3, PLINTH_R - 1.6, PLINTH_TOP, 9.0),
                (MAST_R - 0.3, MAST_R + 1.5, 9.0, 21.0),
            ] {
                let p = c + dir * (r0 + r1) * 0.5;
                let h = (r1 - r0) * 0.5;
                self.slab(
                    Vec3::new(p.x - h, base + y0, p.y - 0.45),
                    Vec3::new(p.x + h, base + y1, p.y + 0.45),
                    -a,
                    CONCRETE * 0.92,
                    Material::Concrete,
                    true,
                );
                self.oboxes.last_mut().unwrap().chip = 0.01;
            }
        }
        // Capacitor bands, each between two steel collars.
        for i in 0..8 {
            let y = 6.0 + i as f32 * 3.8;
            self.pipe(
                at(y),
                at(y + 0.5),
                MAST_R + 0.15,
                SHIELD,
                Material::Glow,
                false,
            );
            for (y0, y1) in [(y - 0.25, y), (y + 0.5, y + 0.75)] {
                self.pipe(
                    at(y0),
                    at(y1),
                    MAST_R + 0.3,
                    STEEL * 1.3,
                    Material::Metal,
                    false,
                );
            }
        }
        // The emblem, one to greet each door.
        for &d in &lm.doors {
            let p = c + d * (MAST_R + 0.2);
            self.standing_emblem(Vec3::new(p.x, base + 11.87, p.y), d, 0.9);
        }

        // The crown: a steel collar, capped; gold vanes splayed round the
        // emitter; the emitter itself, and its beam.
        let top = MAST_TOP;
        self.pipe(
            at(top - 0.6),
            at(top + 0.4),
            MAST_R + 0.9,
            STEEL,
            Material::Metal,
            false,
        );
        self.disc(
            at(top + 0.48),
            Vec3::Y * 0.05,
            MAST_R + 0.8,
            STEEL * 0.8,
            Material::Metal,
        );
        let vanes = 8;
        for k in 0..vanes {
            let a = (k as f32 + 0.5) / vanes as f32 * TAU;
            let radial = Vec3::new(a.cos(), 0.0, a.sin());
            let tangent = Vec3::new(-a.sin(), 0.0, a.cos());
            let up = (Vec3::Y * 0.87 + radial * 0.5).normalize();
            let rig = Rig {
                pos: at(top + 3.1) + radial * 3.4,
                rot: frame(tangent, up),
            };
            rig.part(
                self,
                Vec3::ZERO,
                Vec3::new(1.0, 2.9, 0.12),
                Quat::IDENTITY,
                GOLD,
                Material::Metal,
                false,
                0.0,
            );
        }
        self.pipe(
            at(top + 0.6),
            at(top + 4.6),
            1.3,
            SHIELD * 1.2,
            Material::Glow,
            false,
        );
        self.disc(
            at(top + 4.7),
            Vec3::Y * 0.08,
            1.25,
            SHIELD * 1.2,
            Material::Glow,
        );
        self.pipe(
            at(top + 4.8),
            at(top + 70.0),
            0.45,
            SHIELD * 1.4,
            Material::Glow,
            false,
        );
        self.lights.push(PointLight {
            pos: at(top + 3.0),
            color: SHIELD * 12.0,
            radius: 26.0,
        });
        self.lights.push(PointLight {
            pos: at(12.0),
            color: SHIELD * 5.0,
            radius: 14.0,
        });
    }

    /// A capacitor bank inside the wall, at angle `a` round the middle:
    /// three steel cells on a concrete plinth, banded in blue, a conduit
    /// from each up to the mast and a glowing channel in the floor. A
    /// `wrecked` one is shells of cells, dark.
    fn capacitor_bank(&mut self, rng: &mut Rng, lm: &Landmark, a: f32, base: f32, wrecked: bool) {
        let dir = Vec2::new(a.cos(), a.sin());
        let side = Vec2::new(-dir.y, dir.x);
        let rb = lm.radius - 8.5;
        let p = lm.center + dir * rb;
        self.slab(
            Vec3::new(p.x - 2.0, base, p.y - 3.4),
            Vec3::new(p.x + 2.0, base + 1.0, p.y + 3.4),
            -a,
            CONCRETE_DARK,
            Material::Concrete,
            true,
        );
        for s in [-2.2f32, 0.0, 2.2] {
            let q = p + side * s;
            let at = |y: f32| Vec3::new(q.x, base + y, q.y);
            let top = if wrecked { rng.range(2.0, 4.0) } else { 6.5 };
            self.pipe(at(1.0), at(top), 0.85, STEEL, Material::Metal, true);
            if wrecked {
                continue;
            }
            self.pipe(at(4.7), at(5.2), 0.92, SHIELD, Material::Glow, false);
            self.disc(at(top + 0.1), Vec3::Y * 0.06, 0.75, GOLD, Material::Metal);
            // The conduit, up over the floor to the mast.
            let m = lm.center + (q - lm.center).normalize() * (MAST_R + 0.1);
            self.pipe(
                at(top - 0.6),
                Vec3::new(m.x, base + 8.4, m.y),
                0.28,
                STEEL * 1.2,
                Material::Metal,
                false,
            );
        }
        if wrecked {
            return;
        }
        self.sound(Ambient::Machine, Vec3::new(p.x, base + 3.0, p.y));
        let (from, to) = (rb - 2.2, PLINTH_R + 0.3);
        self.glow_paths.push(GlowPath {
            points: [from, (from + to) * 0.5, to]
                .iter()
                .map(|&r| {
                    let q = lm.center + dir * r;
                    Vec3::new(q.x, base + 0.04, q.y)
                })
                .collect(),
            widths: vec![0.45; 3],
            color: SHIELD,
        });
        self.lights.push(PointLight {
            pos: Vec3::new(p.x, base + 5.0, p.y),
            color: SHIELD * 3.0,
            radius: 9.0,
        });
    }

    /// A transformer house outside the wall, at angle `a`: a concrete box
    /// with steel cooling fins down its flanks and a hazard lamp on top,
    /// a glowing channel from it to the wall's foot.
    fn transformer(&mut self, lm: &Landmark, a: f32, base: f32, wrecked: bool) {
        let dir = Vec2::new(a.cos(), a.sin());
        let side = Vec2::new(-dir.y, dir.x);
        let rt = lm.radius + 8.5;
        let p = lm.center + dir * rt;
        let h = if wrecked { 1.8 } else { 3.4 };
        self.slab(
            Vec3::new(p.x - 2.2, base, p.y - 3.0),
            Vec3::new(p.x + 2.2, base + h, p.y + 3.0),
            -a,
            CONCRETE * 0.85,
            Material::Concrete,
            true,
        );
        if wrecked {
            self.oboxes.last_mut().unwrap().broken = true;
            return;
        }
        // Fins, on both flanks.
        for s in [-1.0f32, 1.0] {
            for k in 0..5 {
                let q = p + side * s * 3.2 + dir * (k as f32 - 2.0) * 0.8;
                self.slab(
                    Vec3::new(q.x - 0.06, base + 0.4, q.y - 0.25),
                    Vec3::new(q.x + 0.06, base + h - 0.4, q.y + 0.25),
                    -a,
                    STEEL * 1.2,
                    Material::Metal,
                    false,
                );
            }
        }
        // Insulators on the roof, and the lamp.
        for k in [-1.0f32, 1.0] {
            let q = p + side * k * 1.6;
            self.pipe(
                Vec3::new(q.x, base + h, q.y),
                Vec3::new(q.x, base + h + 1.4, q.y),
                0.22,
                STONE * 0.8,
                Material::Metal,
                false,
            );
        }
        self.slab(
            Vec3::new(p.x - 0.35, base + h, p.y - 0.35),
            Vec3::new(p.x + 0.35, base + h + 0.5, p.y + 0.35),
            -a,
            SHIELD,
            Material::Glow,
            false,
        );
        let (from, to) = (rt - 2.4, lm.radius + 0.15);
        self.glow_paths.push(GlowPath {
            points: [from, to]
                .iter()
                .map(|&r| {
                    let q = lm.center + dir * r;
                    Vec3::new(q.x, base + 0.04, q.y)
                })
                .collect(),
            widths: vec![0.35; 2],
            color: SHIELD * 0.8,
        });
    }

    /// Pylons round the precinct, open where the avenues come in: a
    /// concrete foot, a steel mast, a crossarm; cable strung, sagging, from
    /// each to the next and down to the wall's top.
    fn pylons(&mut self, lm: &Landmark, base: f32) {
        let n = 14;
        let h = 13.0;
        let mut arms: Vec<Option<[Vec3; 2]>> = Vec::new();
        for k in 0..n {
            let a = (k as f32 + 0.5) / n as f32 * TAU;
            let near_door = lm.doors.iter().any(|&d| apart(a, angle(d)) < 0.3);
            if near_door || shelled(lm, a) {
                arms.push(None);
                continue;
            }
            let dir = Vec2::new(a.cos(), a.sin());
            let tangent = Vec2::new(-dir.y, dir.x);
            let p = lm.center + dir * lm.precinct;
            let yaw = -a + FRAC_PI_2;
            self.slab(
                Vec3::new(p.x - 1.0, base, p.y - 1.0),
                Vec3::new(p.x + 1.0, base + 1.0, p.y + 1.0),
                yaw,
                CONCRETE * 0.9,
                Material::Concrete,
                true,
            );
            self.pipe(
                Vec3::new(p.x, base + 0.9, p.y),
                Vec3::new(p.x, base + h + 0.5, p.y),
                0.32,
                STEEL,
                Material::Metal,
                true,
            );
            self.slab(
                Vec3::new(p.x - 2.4, base + h - 0.4, p.y - 0.15),
                Vec3::new(p.x + 2.4, base + h, p.y + 0.15),
                yaw,
                STEEL,
                Material::Metal,
                false,
            );
            let ends = [-1.0f32, 1.0].map(|s| {
                let q = p + tangent * s * 2.1;
                Vec3::new(q.x, base + h - 0.75, q.y)
            });
            for e in ends {
                self.slab(
                    e + Vec3::new(-0.12, 0.0, -0.12),
                    e + Vec3::new(0.12, 0.33, 0.12),
                    yaw,
                    STONE * 0.8,
                    Material::Metal,
                    false,
                );
            }
            // Down to the wall's coping.
            let w = lm.center + dir * (lm.radius - DRUM_T * 0.5);
            self.cable(
                Vec3::new(p.x, base + h - 0.75, p.y),
                Vec3::new(w.x, base + WALL_H + 0.7, w.y),
                0.8,
            );
            arms.push(Some(ends));
        }
        for k in 0..n {
            if let (Some(a), Some(b)) = (arms[k], arms[(k + 1) % n]) {
                for (u, v) in [(a[1], b[0]), (a[0], b[1])] {
                    self.cable(u, v, 1.6);
                }
            }
        }
    }
}
