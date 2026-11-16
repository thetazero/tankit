//! Saltmarket's edge: what boxes the town in. The reservoir was dammed in a
//! dry valley, so sandstone cliffs close the north, east and south sides; the
//! west, where the valley opens and the main road ran out, was walled off in
//! the war with a barricade of stacked shipping containers.
//!
//! Every piece is solid and too tall to climb, and they overlap without gaps,
//! so a tank driving at any edge stops against something it can see. The
//! arena's own boundary still stands, hidden inside them, as a backstop.

use glam::{Vec2, Vec3};

use super::{Map, OBox};
use crate::render::material::Material;
use crate::rng::Rng;

/// How far (m) the cliffs' and barricade's inner faces reach into the arena
/// at most. Bases line up 6 m from the north and south edges.
pub(super) const MAX_INSET: f32 = 2.0;

/// Sandstone strata, darkest at the foot.
const STRATA: [Vec3; 4] = [
    Vec3::new(0.58, 0.42, 0.3),
    Vec3::new(0.68, 0.5, 0.35),
    Vec3::new(0.74, 0.58, 0.41),
    Vec3::new(0.66, 0.47, 0.33),
];

/// Faded container paint.
const CONTAINERS: [Vec3; 5] = [
    Vec3::new(0.42, 0.2, 0.14),
    Vec3::new(0.24, 0.31, 0.38),
    Vec3::new(0.3, 0.35, 0.25),
    Vec3::new(0.52, 0.4, 0.22),
    Vec3::new(0.46, 0.45, 0.42),
];

/// One side of the arena: its edge line runs from `origin` along `along`;
/// `inward` points into the arena.
struct Side {
    origin: Vec2,
    along: Vec2,
    inward: Vec2,
}

impl Side {
    fn yaw(&self) -> f32 {
        // OBox local +x is (cos yaw, -sin yaw) on the ground.
        (-self.along.y).atan2(self.along.x)
    }

    /// How far a box reaches past the edge line into the arena.
    fn reach(&self, o: &OBox) -> f32 {
        o.corners()
            .iter()
            .map(|c| (*c - self.origin).dot(self.inward))
            .fold(f32::MIN, f32::max)
    }

    /// Slides the box across the edge so its inner face is `inset` inside.
    fn seat(&self, o: &mut OBox, inset: f32) {
        let r = self.reach(o);
        o.center += self.inward * (inset - r);
    }

    /// Slides the box across the edge so even the shallower end of its
    /// (slightly turned) inner face is `inset` inside: it covers the edge
    /// line all along its length.
    fn seat_whole(&self, o: &mut OBox, inset: f32) {
        let mut r: Vec<f32> = o
            .corners()
            .iter()
            .map(|c| (*c - self.origin).dot(self.inward))
            .collect();
        r.sort_by(f32::total_cmp);
        o.center += self.inward * (inset - r[2]);
    }
}

impl Map {
    /// Cliffs on the north, east and south; the container barricade on the
    /// west. `ground` is the play ground's height.
    pub(super) fn valley_edge(&mut self, rng: &mut Rng, ground: fn(f32, f32) -> f32) {
        let s = self.size;
        let sides = [
            Side {
                origin: Vec2::ZERO,
                along: Vec2::X,
                inward: Vec2::Y,
            },
            Side {
                origin: Vec2::new(s, 0.0),
                along: Vec2::Y,
                inward: Vec2::NEG_X,
            },
            Side {
                origin: Vec2::new(0.0, s),
                along: Vec2::X,
                inward: Vec2::NEG_Y,
            },
        ];
        for (k, side) in sides.iter().enumerate() {
            // Only the east cliff sheds boulders: the north and south keep
            // their foot clear for the teams lined up along them.
            self.cliff(rng, side, ground, k == 1);
        }
        self.barricade(rng, ground);
    }

    /// A sandstone cliff along one side: columns of stepped tiers, each
    /// narrower, set back and taller than the one below, turned a little so
    /// the face is ragged. Runs past both ends to close the corners.
    fn cliff(&mut self, rng: &mut Rng, side: &Side, ground: fn(f32, f32) -> f32, scree: bool) {
        let s = self.size;
        let mut t = -14.0;
        while t < s + 14.0 {
            let step = rng.range(10.0, 17.0);
            let mid = t + step * 0.5;
            t += step;
            let at = side.origin + side.along * mid;
            let inside = at.clamp(Vec2::ZERO, Vec2::splat(s));
            let g = ground(inside.x, inside.y);
            // The foot: a sheer wall nothing climbs (the dam's top is 5 m).
            let mut inset = rng.range(0.3, 0.6);
            let mut top = g + rng.range(9.0, 12.0);
            let mut len = step + rng.range(5.0, 9.0);
            let mut depth = rng.range(14.0, 18.0);
            let mut bottom = g - 2.0;
            let tiers = if rng.chance(0.6) { 3 } else { 2 };
            for tier in 0..tiers {
                let mut o = OBox {
                    center: at + side.along * rng.range(-1.5, 1.5) - side.inward * depth * 0.5,
                    // The foot turns least: it has to cover the edge end to end.
                    yaw: side.yaw() + rng.range(-0.12, 0.12) * if tier == 0 { 0.4 } else { 1.0 },
                    half: Vec2::new(len * 0.5, depth * 0.5),
                    y0: bottom,
                    y1: top,
                    color: STRATA[(tier + rng.index(2)) % STRATA.len()] * rng.range(0.92, 1.05),
                    side_mat: Material::Rubble,
                    top_mat: Material::Rubble,
                    chip: 1.0,
                    ..OBox::default()
                };
                if tier == 0 {
                    side.seat_whole(&mut o, inset);
                    inset = side.reach(&o);
                } else {
                    side.seat(&mut o, inset);
                }
                self.oboxes.push(o);
                // The next tier up: set back, shorter along the face, taller.
                bottom = top - 1.0;
                inset -= rng.range(1.0, 3.0);
                top += rng.range(3.0, 6.0);
                len *= rng.range(0.8, 1.05);
                depth *= rng.range(0.75, 0.95);
            }
            // A boulder fallen from the face.
            if scree && rng.chance(0.35) {
                let r = rng.range(1.2, 2.4);
                let mut o = OBox {
                    center: at + side.along * rng.range(-2.0, 2.0),
                    yaw: rng.range(0.0, 3.0),
                    half: Vec2::new(r, r * rng.range(0.7, 1.0)),
                    y0: g - 0.6,
                    y1: g + r * rng.range(0.9, 1.4),
                    color: STRATA[rng.index(STRATA.len())] * 0.95,
                    side_mat: Material::Rubble,
                    top_mat: Material::Rubble,
                    chip: 0.9,
                    ..OBox::default()
                };
                side.seat(&mut o, MAX_INSET + rng.range(1.0, 3.0));
                self.oboxes.push(o);
            }
        }
    }

    /// The west barricade across the valley mouth: 40-foot shipping
    /// containers end to end along the edge, stacked two high and three in
    /// places, a few shoved askew; steel tank traps where the roads ran out.
    fn barricade(&mut self, rng: &mut Rng, ground: fn(f32, f32) -> f32) {
        let s = self.size;
        let side = Side {
            origin: Vec2::ZERO,
            along: Vec2::Y,
            inward: Vec2::X,
        };
        let (wide, tall) = (2.44f32, 2.6f32);
        // Lowest and highest ground under a container.
        let foot = |z0: f32, z1: f32| {
            (0..=4)
                .map(|k| ground(0.0, (z0 + (z1 - z0) * k as f32 / 4.0).clamp(0.0, s)))
                .fold((f32::MAX, f32::MIN), |(lo, hi), g| (lo.min(g), hi.max(g)))
        };
        // Sun-faded paint, or rusted through to bare steel.
        let paint = |rng: &mut Rng| {
            if rng.chance(0.3) {
                (
                    Vec3::new(0.42, 0.27, 0.17) * rng.range(0.8, 1.1),
                    Material::Rust,
                )
            } else {
                let c = CONTAINERS[rng.index(CONTAINERS.len())] * rng.range(0.8, 1.05);
                (c, Material::Painted)
            }
        };
        let mut z = -6.0;
        while z < s + 4.0 {
            // Bottom row: tight end to end, sunk a little into the sand.
            // 40-foot containers, or 20-foot ones where the ground climbs
            // too fast under a long one (up onto the north ridge).
            let mut len = 12.2;
            let (mut g, mut high) = foot(z, z + len);
            if high - g > 0.6 {
                len = 6.06;
                (g, high) = foot(z, z + len);
            }
            let (color, side_mat) = paint(rng);
            let mut base = OBox {
                center: Vec2::new(0.0, z + len * 0.5),
                yaw: side.yaw() + rng.range(-0.03, 0.03),
                half: Vec2::new(len * 0.5, wide * 0.5),
                y0: g - 0.15,
                y1: g - 0.15 + tall,
                color,
                side_mat,
                top_mat: Material::Rust,
                ..OBox::default()
            };
            side.seat_whole(&mut base, rng.range(0.5, 1.0));
            self.oboxes.push(base);
            // A second one stacked on top (always, so the wall is too tall
            // to climb), often shifted along and turned a touch.
            let (color, side_mat) = paint(rng);
            let mut upper = OBox {
                center: base.center + Vec2::new(0.0, rng.range(-0.4, 0.4)),
                yaw: base.yaw + rng.range(-0.08, 0.08),
                y0: base.y1,
                y1: base.y1 + tall,
                color,
                side_mat,
                ..base
            };
            side.seat_whole(&mut upper, rng.range(0.2, 0.8));
            self.oboxes.push(upper);
            // A third here and there, and always where the ground rises
            // under the stack (toward the north ridge) and two would leave it
            // low on the uphill end.
            let steep = high - g > 0.3;
            if steep || rng.chance(0.3) {
                let (color, side_mat) = paint(rng);
                let shift = if steep { 0.4 } else { len * 0.25 };
                let mut third = OBox {
                    center: upper.center + Vec2::new(-0.3, rng.range(-shift, shift)),
                    yaw: upper.yaw + rng.range(-0.15, 0.15) * shift / 3.0,
                    y0: upper.y1,
                    y1: upper.y1 + tall,
                    color,
                    side_mat,
                    ..upper
                };
                if steep {
                    side.seat_whole(&mut third, rng.range(0.0, 0.5));
                } else {
                    side.seat(&mut third, rng.range(-0.5, 0.8));
                }
                self.oboxes.push(third);
            }
            // Jammed end to end: no gap a shell (or a sand-drift climb) gets through.
            z += len - rng.range(0.1, 0.3);
        }
        // Tank traps (three crossed girders) where the roads met the wall.
        for (cz, n) in [(108.0f32, 3), (56.0, 2), (22.0, 2)] {
            for k in 0..n {
                let c = Vec2::new(
                    rng.range(4.0, 6.0),
                    cz + (k as f32 - (n - 1) as f32 * 0.5) * 4.5,
                );
                self.hedgehog(rng, c, ground(c.x, c.y));
            }
        }
    }

    /// A Czech hedgehog: three steel girders crossed at their middles.
    fn hedgehog(&mut self, rng: &mut Rng, at: Vec2, g: f32) {
        let steel = Vec3::new(0.2, 0.16, 0.13);
        let mid = Vec3::new(at.x, g + 0.75, at.y);
        let turn = rng.range(0.0, std::f32::consts::TAU);
        let axes = [
            Vec3::new(1.0, 0.75, 0.0),
            Vec3::new(-0.5, 0.75, 0.87),
            Vec3::new(-0.5, 0.75, -0.87),
        ];
        for a in axes {
            let a = glam::Quat::from_rotation_y(turn) * a.normalize() * 1.1;
            self.segment(mid - a, mid + a, 0.12, steel, Material::Rust, true);
        }
    }
}
