//! The Ithessar's staging court: a wall of turned stones that curves round
//! the corner, three blades behind the curve leaning in to hold a lens of
//! their side's light over the court; low walls on the open sides, a gate
//! of blades in each. Plasma in its side's color runs in the floor.

use super::*;
use crate::render::PointLight;

/// The middle of the back wall, in from the edge, and how it curves.
const IN: f32 = 0.95;
const BEND: f32 = 12.0;

struct Court<'a> {
    map: &'a mut Map,
    rng: &'a mut Rng,
    b: &'a Base,
    /// Its side's light.
    light: Vec3,
}

pub(super) fn build(map: &mut Map, rng: &mut Rng, side: usize, b: &Base) {
    // (Its color by hue, as bright as another's: crimson's dark.)
    let paint = map.factions[side].paint();
    let light = paint * (0.6 / paint.max_element());
    let mut c = Court { map, rng, b, light };
    c.floor();
    c.back_wall();
    c.lens();
    c.fronts();
}

impl Court<'_> {
    fn at3(&self, u: f32, v: f32, y: f32) -> Vec3 {
        let p = self.b.at(u, v);
        Vec3::new(p.x, y, p.y)
    }

    /// A box with its middle at (u, v), its local x along `along` (u, v).
    #[allow(clippy::too_many_arguments)]
    fn piece(
        &mut self,
        at: Vec2,
        along: Vec2,
        half: Vec2,
        y: (f32, f32),
        color: Vec3,
        mat: Material,
        solid: bool,
    ) {
        let yaw = -angle(along * self.b.inward);
        self.map
            .block(self.b.at(at.x, at.y), half, yaw, y, color, mat, solid)
            .chip = 0.01;
    }

    fn glow(&mut self, points: Vec<Vec3>, width: f32) {
        self.map.glow_paths.push(GlowPath {
            widths: vec![width; points.len()],
            points,
            color: self.light,
        });
    }

    /// Paving in rings round the middle, flush with the ground, and its
    /// side's light in channels: round the middle, to the corner and out of
    /// both gates.
    fn floor(&mut self) {
        let m = self.b.size * 0.5;
        let mid = Vec2::splat(m);
        let (mut r, mut k) = (2.6, 0);
        while r + 2.5 < m - 2.0 {
            // (Wide enough only for its inner edge: the stones never meet.)
            let n = (r * 1.9).round().max(6.0) as i32;
            let inner = (r - 1.2) * (PI / n as f32).tan() - 0.04;
            for j in 0..n {
                let d = dir((j as f32 + 0.5 * (k % 2) as f32) / n as f32 * TAU);
                let shade = 1.15 + 0.12 * (((j * 3 + k * 5) % 4) as f32);
                self.piece(
                    mid + d * r,
                    Vec2::new(-d.y, d.x),
                    Vec2::new(inner.max(0.2), 1.15),
                    (-0.1, 0.02),
                    STONE * shade,
                    C,
                    false,
                );
            }
            r += 2.5;
            k += 1;
        }
        let ring = |c: &Self, r: f32| -> Vec<Vec3> {
            (0..=40)
                .map(|k| {
                    let p = mid + dir(k as f32 / 40.0 * TAU) * r;
                    c.at3(p.x, p.y, 0.05)
                })
                .collect()
        };
        let pts = ring(self, 6.35);
        self.glow(pts, 0.16);
        let corner = IN + BEND * (1.0 - FRAC_PI_4.cos()) + 1.0;
        let line = mid.x - 6.35 * FRAC_PI_4.cos();
        let out = self.b.size + 1.0;
        for (a, z) in [
            (Vec2::splat(line), Vec2::splat(corner)),
            (Vec2::new(m + 6.35, m), Vec2::new(out, m)),
            (Vec2::new(m, m + 6.35), Vec2::new(m, out)),
        ] {
            let n = 10;
            let pts = (0..=n)
                .map(|k| {
                    let p = a.lerp(z, k as f32 / n as f32);
                    self.at3(p.x, p.y, 0.06)
                })
                .collect();
            self.glow(pts, 0.16);
        }
    }

    /// The wall on the two sides at its back: each stone turned a little,
    /// and higher the nearer the corner.
    fn back_wall(&mut self) {
        let court = self.b.size;
        let straight = court + 1.2 - IN - BEND;
        let arc = BEND * FRAC_PI_2;
        let total = straight * 2.0 + arc;
        // Along it, in from one end: where, and which way it runs.
        let path = |s: f32| -> (Vec2, Vec2) {
            if s < straight {
                (Vec2::new(IN, court + 1.2 - s), Vec2::new(0.0, -1.0))
            } else if s < straight + arc {
                let a = PI + (s - straight) / BEND;
                (
                    Vec2::splat(IN + BEND) + dir(a) * BEND,
                    Vec2::new(-a.sin(), a.cos()),
                )
            } else {
                (
                    Vec2::new(IN + BEND + s - straight - arc, IN),
                    Vec2::new(1.0, 0.0),
                )
            }
        };
        let n = (total / 2.8).round() as i32;
        let step = total / n as f32;
        for k in 0..n {
            let (p, along) = path((k as f32 + 0.5) * step);
            // 0 at the ends, 1 in the corner.
            let near = 1.0 - ((k as f32 + 0.5) / n as f32 * 2.0 - 1.0).abs();
            // Turned the other way past the corner: the two halves mirror.
            let turn = if k * 2 < n { 0.16f32 } else { -0.16 };
            let along = Vec2::from_angle(turn).rotate(along);
            let h = 4.6 + 4.4 * near * near + self.rng.range(-0.15, 0.15) + 0.5 * (k % 2) as f32;
            let half = Vec2::new(step * 0.53, 0.6);
            let color = WALLS[(k % 2) as usize] * self.rng.range(0.98, 1.08);
            self.piece(p, along, half, (-0.5, h), color, C, true);
            self.piece(
                p,
                along,
                half + Vec2::new(0.03, 0.12),
                (h - 0.02, h + 0.4),
                GLASS * 1.6,
                V,
                false,
            );
            // The court's face of it: a panel of glass, its side's light
            // up the middle.
            let face = Vec2::new(-along.y, along.x);
            if k % 2 == 1 {
                self.piece(
                    p + face * 0.6,
                    along,
                    Vec2::new(step * 0.3, 0.06),
                    (0.9, h - 0.7),
                    GLASS * 1.3,
                    V,
                    false,
                );
                self.piece(
                    p + face * 0.66,
                    along,
                    Vec2::new(0.2, 0.07),
                    (1.2, h - 1.0),
                    self.light,
                    G,
                    false,
                );
            }
        }
    }

    /// Behind the wall's curve: blades leaning in over it, holding a lens of
    /// its side's light, seen from the streets round.
    fn lens(&mut self) {
        let heart = Vec2::splat(6.6);
        let hy = 14.0;
        for (k, (u, v)) in [(2.2, 2.2), (7.4, 1.1), (1.1, 7.4)].into_iter().enumerate() {
            let foot = Vec2::new(u, v);
            let to = heart + (foot - heart).normalize() * if k == 0 { 1.2 } else { 2.1 };
            let top = if k == 0 { hy + 5.5 } else { hy - 0.6 };
            let a = self.at3(u, v, -0.6);
            let axis = self.at3(to.x, to.y, top) - a;
            let r = if k == 0 { 1.15 } else { 0.8 };
            crystal(self.map, self.rng, a, axis, r, false, false);
        }
        let heart = self.at3(heart.x, heart.y, hy);
        // It faces over the court.
        let n = Vec3::new(self.b.inward.x, 0.0, self.b.inward.y).normalize();
        self.disc_lens(heart, n, 2.3);
        self.map.lights.push(PointLight {
            pos: heart,
            color: self.light * 8.0,
            radius: 16.0,
        });
        self.map.sound(Ambient::Crystal, heart);
    }

    /// A lens of its side's light in a rim of glass, `n` the way it faces.
    fn disc_lens(&mut self, c: Vec3, n: Vec3, r: f32) {
        let light = self.light;
        self.map.disc(c, n * r * 0.1, r, GLASS * 1.6, V);
        self.map.disc(c, n * r * 0.16, r * 0.84, light, G);
        self.map.disc(c, n * r * 0.24, r * 0.6, light, G);
        self.map.disc(c, n * r * 0.3, r * 0.34, light * 1.4, G);
    }

    /// The open sides: low walls, cover to fight from, and in each a wide
    /// gate, blades leaning over it to hold a thread of light.
    fn fronts(&mut self) {
        let court = self.b.size;
        for flip in [false, true] {
            let uv = |t: f32, across: f32| {
                let p = Vec2::new(t, court + across);
                if flip { Vec2::new(p.y, p.x) } else { p }
            };
            let along = uv(1.0, 0.0) - uv(0.0, 0.0);
            let (g0, g1) = (court * 0.35, court * 0.65);
            // (The far one's end is the other front's: one stops short.)
            let end = if flip { court - 0.1 } else { court + 1.4 };
            for (t0, t1) in [(2.0, g0), (g1, end)] {
                let n = ((t1 - t0) / 2.6).round() as i32;
                let step = (t1 - t0) / n as f32;
                for k in 0..n {
                    let turn = if k % 2 == 0 { 0.09f32 } else { -0.09 };
                    let a = Vec2::from_angle(turn).rotate(along);
                    let h = 1.55 + self.rng.range(0.0, 0.15);
                    let p = uv(t0 + (k as f32 + 0.5) * step, 0.7);
                    let half = Vec2::new(step * 0.48, 0.5);
                    let color = WALLS[(k % 2) as usize] * 1.05;
                    self.piece(p, a, half, (-0.4, h), color, C, true);
                    self.piece(
                        p,
                        a,
                        half + Vec2::new(0.02, 0.06),
                        (h - 0.02, h + 0.2),
                        GLASS * 1.6,
                        V,
                        false,
                    );
                    // A seam of light along the court's face.
                    let face = if flip {
                        Vec2::new(-1.0, 0.0)
                    } else {
                        Vec2::new(0.0, -1.0)
                    };
                    let face = Vec2::from_angle(turn).rotate(face);
                    self.piece(
                        p + face * 0.5,
                        a,
                        Vec2::new(step * 0.34, 0.04),
                        (0.95, 1.12),
                        self.light,
                        G,
                        false,
                    );
                }
            }
            let mut tips = Vec::new();
            for (t, s) in [(g0 - 1.0, 1.0f32), (g1 + 1.0, -1.0)] {
                let p = uv(t, 0.7);
                let half = Vec2::new(1.0, 0.68);
                self.piece(p, along, half, (-0.5, 2.7), STONE * 1.2, C, true);
                self.piece(
                    p,
                    along,
                    half - Vec2::splat(0.12),
                    (2.68, 3.2),
                    GLASS * 1.6,
                    V,
                    false,
                );
                let tip = uv(t + s * 5.2, 0.7);
                let (a, z) = (self.at3(p.x, p.y, 2.4), self.at3(tip.x, tip.y, 11.5));
                crystal(self.map, self.rng, a, z - a, 0.6, false, false);
                tips.push(z);
            }
            let (a, z) = (tips[0], tips[1]);
            let (light, down) = (self.light, Vec3::Y * 0.5);
            self.map.segment(a - down, z - down, 0.12, light, G, false);
            // A small lens hanging under the thread, facing out.
            let out = along.perp() * self.b.inward;
            let c = (a + z) * 0.5 - down - Vec3::Y * 1.15;
            self.disc_lens(c, Vec3::new(out.x, 0.0, out.y), 0.95);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::map::procgen::{Look, map, skeleton};

    /// A court keeps to its ground, and takes none of its spawns.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn courts_leave_every_spawn() {
        for seed in 0..2 {
            let spec = crate::city_spec(seed, Look::Ithessar);
            let (sk, m) = (skeleton(&spec), map(&spec));
            for (k, b) in sk.bases.iter().enumerate() {
                assert_eq!(m.bases[k], b.spawn_points(), "seed {seed} side {k}");
            }
        }
    }

    #[test]
    fn a_court_keeps_to_its_ground() {
        use super::*;
        let sk = skeleton(&crate::city_spec(0, Look::Imperium));
        let mut rng = Rng::new(0);
        for b in &sk.bases {
            let mut m = sk.blank(&crate::map::DEFAULT_SIDES);
            let built = m.oboxes.len();
            super::build(&mut m, &mut rng, 0, b);
            for o in m.oboxes[built..].iter().filter(|o| o.solid) {
                for p in o.base_shadow(3.0) {
                    let q = (p - b.corner) * b.inward;
                    assert!(
                        q.min_element() > -0.1 && q.max_element() < b.size + 1.5,
                        "{q} off the court: {o:?}"
                    );
                }
            }
        }
    }
}
