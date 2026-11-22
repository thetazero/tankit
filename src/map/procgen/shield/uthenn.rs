//! The Uthenn's shield generator. They make tungsten cubes, and build with
//! them: a stack of great cubes, each turned a little from the one under
//! it, a green seam lit between each; over the top, a cube balanced on its
//! corner, glowing, held there by green lasers from four emitter stacks
//! round it. Round it, a wall of cubes, open on each door; outside,
//! stockpiles of finished cubes and green guide lines in the floor, and
//! square posts round the precinct carrying a laser strung high between
//! them.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use glam::{Quat, Vec2, Vec3};

use super::*;
use crate::map::Ambient;
use crate::map::landmarks::Rig;
use crate::render::PointLight;

const TUNGSTEN: Vec3 = Vec3::new(0.2, 0.24, 0.3);
const SHELL_DARK: Vec3 = Vec3::new(0.12, 0.15, 0.14);
const GREEN: Vec3 = Vec3::new(0.15, 0.9, 0.35);
/// The wall's cubes, a side.
const WALL_CUBE: f32 = 3.4;
/// Where the floating cube's middle hangs.
const HEART: f32 = 29.0;

impl Map {
    pub(super) fn uthenn_generator(
        &mut self,
        rng: &mut Rng,
        lm: &Landmark,
        base: f32,
    ) -> Vec<Drift> {
        self.cube_wall(rng, lm, base);
        self.cube_stack(lm, base);
        for a in between_doors(lm) {
            let wrecked = shelled(lm, a);
            self.emitter_stack(lm, a, base, wrecked);
            if !wrecked {
                self.stockpile(rng, lm, a, base);
            }
        }
        self.laser_posts(lm, base);
        self.generator_rubble(rng, lm, base, TUNGSTEN * 1.4)
    }

    /// A tungsten cube `s` on a side standing at `c` on `y`, turned `yaw`,
    /// a seam round its middle (lit or dark).
    #[allow(clippy::too_many_arguments)]
    fn tungsten(&mut self, c: Vec2, s: f32, yaw: f32, y: f32, lit: bool, solid: bool) {
        self.block(
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
        self.block(
            c,
            Vec2::splat(s * 0.5 + 0.05),
            yaw,
            (mid - 0.1, mid + 0.1),
            color,
            mat,
            false,
        );
    }

    /// The wall: cubes shoulder to shoulder round the ring, square to it, a
    /// second course on most; knocked down and scattered where shelled.
    fn cube_wall(&mut self, rng: &mut Rng, lm: &Landmark, base: f32) {
        let r = lm.radius - WALL_CUBE * 0.5;
        let segs = (TAU * r / (WALL_CUBE + 0.15)).floor() as usize;
        // Cubes are big against the gaps between them: one skipped by angle
        // alone can leave a door too narrow, so keep each well off a door.
        let in_way = |c: Vec2| {
            lm.doors.iter().any(|&d| {
                let q = c - lm.center;
                q.dot(d) > 0.0 && q.perp_dot(d).abs() < WALL_CUBE * 0.5 + 3.2
            })
        };
        let spots = (0..segs).map(|k| (k as f32 + 0.5) / segs as f32 * TAU);
        for (k, a) in spots
            .filter(|&a| !in_way(lm.center + dir(a) * r))
            .enumerate()
        {
            let c = lm.center + dir(a) * r;
            let yaw = -a;
            if shelled(lm, a) {
                // Knocked off the ring, lying tipped where it fell.
                let off = dir(a + rng.range(-0.1, 0.1)) * rng.range(0.0, 3.0);
                self.block(
                    c + off,
                    Vec2::splat(WALL_CUBE * 0.5),
                    yaw + rng.range(-0.5, 0.5),
                    (base - 0.6, base - 0.6 + WALL_CUBE),
                    TUNGSTEN * 0.9,
                    Material::Metal,
                    true,
                )
                .tilt = rng.range(-0.3, 0.3);
                continue;
            }
            self.tungsten(c, WALL_CUBE, yaw, base, k % 3 == 0, true);
            if k % 5 != 2 {
                self.tungsten(c, WALL_CUBE * 0.9, yaw, base + WALL_CUBE, k % 4 == 1, true);
            }
        }
    }

    /// The stack: a square plinth, then great cubes shrinking upward, each
    /// turned on the last, a lit seam between each; the cube over it all on
    /// its corner, glowing, and the light it throws up.
    fn cube_stack(&mut self, lm: &Landmark, base: f32) {
        let c = lm.center;
        let at = |y: f32| Vec3::new(c.x, base + y, c.y);
        self.sound(Ambient::Machine, at(3.0));
        // Half-diagonal 6.2: inside the plinth's ring.
        self.block(
            c,
            Vec2::splat(4.4),
            0.0,
            (base, base + 1.2),
            SHELL_DARK * 1.6,
            Material::Metal,
            true,
        );
        let mut y = base + 1.2;
        for (k, s) in [7.4f32, 6.0, 4.8, 3.8].into_iter().enumerate() {
            let yaw = k as f32 * 0.26;
            self.block(
                c,
                Vec2::splat(s * 0.5),
                yaw,
                (y, y + s),
                TUNGSTEN,
                Material::Metal,
                true,
            )
            .chip = 0.12;
            // Green seams up its corners.
            for q in 0..4 {
                let corner = Vec2::from_angle(-yaw + q as f32 * FRAC_PI_2 + FRAC_PI_4)
                    * (s * 0.5 * 1.414 + 0.01);
                self.block(
                    c + corner,
                    Vec2::splat(0.12),
                    yaw,
                    (y + 0.4, y + s - 0.4),
                    GREEN,
                    Material::Glow,
                    false,
                );
            }
            y += s;
            self.block(
                c,
                Vec2::splat(s * 0.42),
                yaw + 0.13,
                (y, y + 0.35),
                GREEN,
                Material::Glow,
                false,
            );
            y += 0.35;
        }
        // The cube on its corner: a body diagonal stood upright.
        let rig = Rig {
            pos: at(HEART),
            rot: Quat::from_rotation_arc(Vec3::ONE.normalize(), Vec3::Y),
        };
        rig.part(
            self,
            Vec3::ZERO,
            Vec3::splat(1.7),
            Quat::IDENTITY,
            GREEN,
            Material::Glow,
            false,
            0.0,
        );
        // A tungsten frame along its edges, the glow showing between.
        for (i, axis) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
            for sx in [-1.0f32, 1.0] {
                for sy in [-1.0f32, 1.0] {
                    let (u, v) = match i {
                        0 => (Vec3::Y, Vec3::Z),
                        1 => (Vec3::X, Vec3::Z),
                        _ => (Vec3::X, Vec3::Y),
                    };
                    let local = (u * sx + v * sy) * 2.2;
                    let half = axis * 2.35 + (u + v) * 0.16;
                    rig.part(
                        self,
                        local,
                        half,
                        Quat::IDENTITY,
                        TUNGSTEN * 0.8,
                        Material::Metal,
                        false,
                        0.0,
                    );
                }
            }
        }
        self.pipe(
            at(HEART + 3.2),
            at(HEART + 60.0),
            0.4,
            GREEN * 1.2,
            Material::Glow,
            false,
        );
        self.lights.push(PointLight {
            pos: at(HEART),
            color: GREEN * 12.0,
            radius: 24.0,
        });
    }

    /// An emitter stack at angle `a` inside the wall: three cubes, an
    /// emitter on top, its laser up to the floating cube; a guide line in the
    /// floor to the plinth. A `wrecked` one is one cube, dark.
    fn emitter_stack(&mut self, lm: &Landmark, a: f32, base: f32, wrecked: bool) {
        let p = lm.center + dir(a) * (lm.radius - 8.5);
        let yaw = -a;
        if wrecked {
            self.tungsten(p, 3.0, yaw + 0.3, base, false, true);
            return;
        }
        let mut y = base;
        for (k, s) in [3.0f32, 2.6, 2.2].into_iter().enumerate() {
            self.tungsten(p, s, yaw + k as f32 * 0.2, y, k == 1, true);
            y += s;
        }
        let tip = Vec3::new(p.x, y + 0.6, p.y);
        self.block(
            p,
            Vec2::splat(0.5),
            yaw,
            (y, y + 1.0),
            GREEN,
            Material::Glow,
            false,
        );
        let heart = Vec3::new(lm.center.x, base + HEART, lm.center.y);
        self.segment(
            tip,
            heart - (heart - tip).normalize() * 1.5,
            0.14,
            GREEN * 1.3,
            Material::Glow,
            false,
        );
        self.lights.push(PointLight {
            pos: tip,
            color: GREEN * 3.0,
            radius: 9.0,
        });
        self.glow_paths
            .push(channel(lm, a, lm.radius - 10.3, PLINTH_R, base, 0.3, GREEN));
    }

    /// Finished cubes stacked outside the wall at angle `a`, two courses,
    /// one off the top fallen beside; a guide line to the wall.
    fn stockpile(&mut self, rng: &mut Rng, lm: &Landmark, a: f32, base: f32) {
        let d = dir(a);
        let side = Vec2::new(-d.y, d.x);
        let p = lm.center + d * (lm.radius + 8.5);
        let s = 1.6;
        for i in -1i32..=1 {
            for j in 0..2 {
                let q = p + side * i as f32 * (s + 0.1) + d * (j as f32 - 0.5) * (s + 0.1);
                self.tungsten(q, s, -a, base, false, true);
                if (i + j) % 2 == 0 {
                    self.tungsten(q, s, -a, base + s, false, false);
                }
            }
        }
        let q = p + side * 3.6;
        self.block(
            q,
            Vec2::splat(s * 0.5),
            -a + rng.range(0.2, 0.6),
            (base, base + s),
            TUNGSTEN,
            Material::Metal,
            true,
        );
        self.glow_paths.push(channel(
            lm,
            a,
            lm.radius + 6.5,
            lm.radius + 0.15,
            base,
            0.25,
            GREEN * 0.8,
        ));
    }

    /// Square posts round the precinct, a laser strung between each and the
    /// next, high over the way.
    fn laser_posts(&mut self, lm: &Landmark, base: f32) {
        let h = 11.0;
        let spots = precinct_spots(lm, 14);
        let tops: Vec<Option<Vec3>> = spots
            .iter()
            .map(|s| {
                s.map(|a| {
                    let p = lm.center + dir(a) * lm.precinct;
                    self.block(
                        p,
                        Vec2::splat(0.7),
                        -a,
                        (base, base + h),
                        TUNGSTEN,
                        Material::Metal,
                        true,
                    )
                    .chip = 0.04;
                    self.block(
                        p,
                        Vec2::splat(0.75),
                        -a,
                        (base + h - 1.2, base + h - 0.9),
                        GREEN,
                        Material::Glow,
                        false,
                    );
                    Vec3::new(p.x, base + h - 1.05, p.y)
                })
            })
            .collect();
        for k in 0..tops.len() {
            if let (Some(a), Some(b)) = (tops[k], tops[(k + 1) % tops.len()]) {
                let d = (b - a).normalize();
                self.segment(
                    a + d * 0.8,
                    b - d * 0.8,
                    0.06,
                    GREEN * 1.2,
                    Material::Glow,
                    false,
                );
            }
        }
    }
}
