//! Plasma rivers: an Ithessar city's power grid. A channel cut deep and
//! lined, plasma running down it. Hover hulls ride on its surface; anything
//! else sinks in it and burns (see `combat`).

use std::f32::consts::TAU;

use glam::{Vec2, Vec3};

use super::OBox;
use crate::render::material::Material;
use crate::render::mesh::MeshData;

/// A tracked tank in it burns down from full health in this long, s.
pub const BURN: f32 = 3.2;

#[derive(Clone, Debug, Default)]
pub struct River {
    /// The middle of its surface, down its course.
    pub points: Vec<Vec3>,
    /// Half its width.
    pub half: f32,
}

impl River {
    /// Its surface's height at `p`, if `p` is over it (within `margin` m).
    pub fn surface(&self, p: Vec2, margin: f32) -> Option<f32> {
        let reach = self.half + margin;
        let mut best: Option<(f32, f32)> = None;
        for w in self.points.windows(2) {
            let (a, b) = (Vec2::new(w[0].x, w[0].z), Vec2::new(w[1].x, w[1].z));
            let d = b - a;
            let t = ((p - a).dot(d) / d.length_squared().max(1e-6)).clamp(0.0, 1.0);
            let off = p.distance_squared(a + d * t);
            if off < reach * reach && best.is_none_or(|(o, _)| off < o) {
                best = Some((off, w[0].y + (w[1].y - w[0].y) * t));
            }
        }
        best.map(|(_, y)| y)
    }

    /// Its surface: each vertex's color is how far down it is, how far
    /// across (0 to 1) and how wide it is (tens of meters); the shader
    /// makes the plasma of that.
    pub fn mesh(&self, m: &mut MeshData) {
        let n = self.points.len();
        if n < 2 {
            return;
        }
        let side = |i: usize| {
            let a = self.points[i.saturating_sub(1)];
            let b = self.points[(i + 1).min(n - 1)];
            let d = Vec2::new(b.x - a.x, b.z - a.z).normalize_or_zero();
            Vec3::new(-d.y, 0.0, d.x)
        };
        // Into the banks a little, so no gap shows at its edge.
        let half = self.half + 0.6;
        const ACROSS: usize = 8;
        let mut down = 0.0;
        let mut rows: Vec<Vec<u32>> = Vec::with_capacity(n);
        for i in 0..n {
            if i > 0 {
                down += self.points[i].distance(self.points[i - 1]);
            }
            let s = side(i);
            rows.push(
                (0..=ACROSS)
                    .map(|k| {
                        let u = k as f32 / ACROSS as f32;
                        let p = self.points[i] + s * half * (u * 2.0 - 1.0);
                        let c = Vec3::new(down / 100.0, u, self.half / 10.0);
                        m.vert(p, Vec3::Y, c.extend(Material::Plasma.id()))
                    })
                    .collect(),
            );
        }
        for w in rows.windows(2) {
            for k in 0..ACROSS {
                let (a, b, c, d) = (w[0][k], w[0][k + 1], w[1][k + 1], w[1][k]);
                m.indices.extend_from_slice(&[a, c, b, a, d, c]);
            }
        }
    }

    /// Posts of dark glass down both banks, every `step` m, and a lip
    /// between them: what keeps the plasma in.
    pub fn lining(&self, ground: impl Fn(Vec2) -> f32, skip: impl Fn(Vec2) -> bool) -> Vec<OBox> {
        let glass = Vec3::new(0.035, 0.05, 0.09);
        let mut out = Vec::new();
        for (i, w) in self.points.windows(2).enumerate() {
            let (a, b) = (Vec2::new(w[0].x, w[0].z), Vec2::new(w[1].x, w[1].z));
            let d = (b - a).normalize_or(Vec2::X);
            let side = Vec2::new(-d.y, d.x);
            for e in [-1.0f32, 1.0] {
                let mid = (a + b) * 0.5 + side * e * (self.half + 3.4);
                if skip(mid) {
                    continue;
                }
                let g = ground(mid);
                let yaw = d.x.atan2(d.y);
                // A kerb a tank rolls over.
                out.push(OBox {
                    center: mid,
                    yaw,
                    half: Vec2::new(0.5, a.distance(b) * 0.5 + 0.1),
                    y0: g - 0.8,
                    y1: g + 0.22,
                    color: glass * 1.6,
                    side_mat: Material::Vhaal,
                    top_mat: Material::Vhaal,
                    solid: false,
                    ..OBox::default()
                });
                if i % 4 == 0 {
                    out.push(OBox {
                        center: mid + side * e * 0.3,
                        yaw: yaw + TAU / 8.0,
                        half: Vec2::splat(0.55),
                        y0: g - 0.8,
                        y1: g + 2.6,
                        color: glass,
                        side_mat: Material::Vhaal,
                        top_mat: Material::Vhaal,
                        ..OBox::default()
                    });
                    out.push(OBox {
                        center: mid + side * e * 0.3,
                        yaw,
                        half: Vec2::splat(0.22),
                        y0: g + 2.55,
                        y1: g + 3.1,
                        color: Vec3::new(1.0, 0.16, 0.52),
                        side_mat: Material::Glow,
                        top_mat: Material::Glow,
                        solid: false,
                        ..OBox::default()
                    });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_surface_is_where_the_river_is() {
        let r = River {
            points: vec![Vec3::new(0.0, 1.0, 0.0), Vec3::new(100.0, 3.0, 0.0)],
            half: 8.0,
        };
        assert_eq!(r.surface(Vec2::new(50.0, 5.0), 0.0), Some(2.0));
        assert_eq!(r.surface(Vec2::new(50.0, 9.0), 0.0), None);
        assert_eq!(r.surface(Vec2::new(50.0, 9.0), 2.0), Some(2.0));
    }
}
