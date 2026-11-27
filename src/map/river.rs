//! Plasma rivers: an Ithessar city's power grid. A channel cut deep and
//! lined, plasma running down it. Hover hulls ride on its surface; anything
//! else sinks in it and burns (see `combat`).

use glam::{Vec2, Vec3};

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

    /// Where something going straight from `a` to `b` goes down through its
    /// surface, if it does.
    pub fn entry(&self, a: Vec3, b: Vec3) -> Option<Vec3> {
        let step = b - a;
        let flat = |p: Vec3| Vec2::new(p.x, p.z);
        let mut first: Option<f32> = None;
        for w in self.points.windows(2) {
            let d = flat(w[1] - w[0]);
            let l2 = d.length_squared().max(1e-6);
            // How far above this piece's own plane a point is.
            let above = |p: Vec3| {
                let t = flat(p - w[0]).dot(d) / l2;
                p.y - (w[0].y + (w[1].y - w[0].y) * t)
            };
            let (from, to) = (above(a), above(b));
            if from <= 0.0 || to > 0.0 {
                continue;
            }
            let s = from / (from - to);
            if first.is_some_and(|f| f <= s) {
                continue;
            }
            // Only if it's this piece that's there.
            let p = a + step * s;
            if self
                .surface(flat(p), 0.0)
                .is_some_and(|y| (y - p.y).abs() < 0.2)
            {
                first = Some(s);
            }
        }
        first.map(|s| a + step * s)
    }

    /// The stretch of it nearest `at`: `n` points, every `step`th of its own,
    /// each with how far down it that is.
    pub fn stretch(&self, at: Vec3, n: usize, step: usize) -> Vec<glam::Vec4> {
        let mut down = 0.0;
        let all: Vec<glam::Vec4> = (0..self.points.len())
            .map(|i| {
                if i > 0 {
                    down += self.points[i].distance(self.points[i - 1]);
                }
                self.points[i].extend(down)
            })
            .step_by(step)
            .collect();
        let near = (0..all.len())
            .min_by(|&a, &b| {
                let d = |i: usize| all[i].truncate().distance_squared(at);
                d(a).total_cmp(&d(b))
            })
            .unwrap_or(0);
        let from = near.saturating_sub(n / 2).min(all.len().saturating_sub(n));
        all[from..(from + n).min(all.len())].to_vec()
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

    #[test]
    fn a_shot_goes_in_where_it_crosses_the_surface() {
        // Bending, so the far pieces' planes are crossed elsewhere too.
        let r = River {
            points: vec![
                Vec3::new(0.0, 1.0, 0.0),
                Vec3::new(100.0, 3.0, 0.0),
                Vec3::new(100.0, 4.0, 100.0),
            ],
            half: 8.0,
        };
        let into = r
            .entry(Vec3::new(50.0, 12.0, -20.0), Vec3::new(50.0, -8.0, 20.0))
            .unwrap();
        assert!(into.distance(Vec3::new(50.0, 2.0, 0.0)) < 0.01, "{into}");
        // Up out of it, short of it, and down beside it: no.
        let up = r.entry(Vec3::new(50.0, 0.0, 0.0), Vec3::new(50.0, 9.0, 0.0));
        let short = r.entry(Vec3::new(50.0, 9.0, 0.0), Vec3::new(50.0, 2.5, 0.0));
        let beside = r.entry(Vec3::new(50.0, 9.0, 12.0), Vec3::new(50.0, 0.0, 12.0));
        assert_eq!((up, short, beside), (None, None, None));
    }
}
