//! Track belt geometry: the closed path a tread wraps around its wheels, and
//! where each link sits on it for a given amount of travel.

use glam::Vec2;

/// A wheel in the hull's side profile: center (z forward, y up) and radius.
#[derive(Clone, Copy, Debug)]
pub struct Wheel {
    pub center: Vec2,
    pub radius: f32,
}

/// The belt is the convex hull around all wheels, offset by half the link
/// thickness, sampled as a polyline with cumulative arc length.
#[derive(Clone, Debug)]
pub struct Belt {
    pts: Vec<Vec2>,
    /// Cumulative length at the start of each segment; `cum[pts.len()]` is the full loop.
    cum: Vec<f32>,
    centroid: Vec2,
}

/// Where a link sits: position in the side profile, along-belt tangent and outward normal.
#[derive(Clone, Copy, Debug)]
pub struct BeltPoint {
    pub pos: Vec2,
    pub normal: Vec2,
}

impl Belt {
    pub fn new(wheels: &[Wheel], half_thickness: f32) -> Belt {
        let mut cloud = Vec::new();
        for w in wheels {
            let r = w.radius + half_thickness;
            for i in 0..96 {
                let a = i as f32 / 96.0 * std::f32::consts::TAU;
                cloud.push(w.center + Vec2::new(a.cos(), a.sin()) * r);
            }
        }
        let mut pts = convex_hull(cloud);
        // Clockwise in (z, y), so the bottom run goes backward as s grows.
        pts.reverse();
        // Start at the rearmost point of the bottom run for stable indexing.
        let start = (0..pts.len())
            .min_by(|&a, &b| {
                (pts[a].y, pts[a].x)
                    .partial_cmp(&(pts[b].y, pts[b].x))
                    .unwrap()
            })
            .unwrap();
        pts.rotate_left(start);
        let mut cum = vec![0.0];
        for i in 0..pts.len() {
            let d = pts[i].distance(pts[(i + 1) % pts.len()]);
            cum.push(cum[i] + d);
        }
        let centroid = pts.iter().copied().sum::<Vec2>() / pts.len() as f32;
        Belt { pts, cum, centroid }
    }

    pub fn length(&self) -> f32 {
        *self.cum.last().unwrap()
    }

    /// Sample the belt at arc length `s` (wraps around).
    pub fn at(&self, s: f32) -> BeltPoint {
        let s = s.rem_euclid(self.length());
        let i = match self.cum.binary_search_by(|c| c.partial_cmp(&s).unwrap()) {
            Ok(i) => i.min(self.pts.len() - 1),
            Err(i) => i - 1,
        };
        let (a, b) = (self.pts[i], self.pts[(i + 1) % self.pts.len()]);
        let seg = self.cum[i + 1] - self.cum[i];
        let t = if seg > 1e-6 {
            (s - self.cum[i]) / seg
        } else {
            0.0
        };
        let pos = a.lerp(b, t);
        // Smooth the normal over a short window so links rotate evenly around wheels.
        let ahead = self.raw(s + 0.04);
        let behind = self.raw(s - 0.04);
        let tangent = (ahead - behind).normalize_or_zero();
        let mut normal = Vec2::new(-tangent.y, tangent.x);
        if normal.dot(pos - self.centroid) < 0.0 {
            normal = -normal;
        }
        BeltPoint { pos, normal }
    }

    fn raw(&self, s: f32) -> Vec2 {
        let s = s.rem_euclid(self.length());
        let i = match self.cum.binary_search_by(|c| c.partial_cmp(&s).unwrap()) {
            Ok(i) => i.min(self.pts.len() - 1),
            Err(i) => i - 1,
        };
        let (a, b) = (self.pts[i], self.pts[(i + 1) % self.pts.len()]);
        let seg = self.cum[i + 1] - self.cum[i];
        a.lerp(
            b,
            if seg > 1e-6 {
                (s - self.cum[i]) / seg
            } else {
                0.0
            },
        )
    }
}

/// Andrew's monotone chain; returns the hull counter-clockwise.
fn convex_hull(mut p: Vec<Vec2>) -> Vec<Vec2> {
    p.sort_by(|a, b| (a.x, a.y).partial_cmp(&(b.x, b.y)).unwrap());
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);
    let mut lower: Vec<Vec2> = Vec::new();
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 1e-7 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<Vec2> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 1e-7 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

#[cfg(test)]
mod tests {
    use super::*;

    fn belt() -> Belt {
        let w = |z, y, r| Wheel {
            center: Vec2::new(z, y),
            radius: r,
        };
        Belt::new(
            &[
                w(2.0, 0.6, 0.38),
                w(-2.0, 0.6, 0.34),
                w(1.0, 0.4, 0.32),
                w(0.0, 0.4, 0.32),
                w(-1.0, 0.4, 0.32),
            ],
            0.04,
        )
    }

    #[test]
    fn bottom_run_touches_ground_and_moves_backward() {
        let b = belt();
        let mut checked = 0;
        for i in 0..400 {
            let s = b.length() * i as f32 / 400.0;
            let (p0, p1) = (b.at(s), b.at(s + 0.05));
            // On the bottom run (between the outer road wheels), growing s heads toward -z.
            if p0.pos.y < 0.05 && p0.pos.x.abs() < 0.9 && p1.pos.y < 0.05 {
                assert!(
                    (p0.pos.y - 0.04).abs() < 0.01,
                    "bottom of belt at y={}",
                    p0.pos.y
                );
                assert!(p1.pos.x < p0.pos.x, "bottom run should move backward");
                assert!(p0.normal.y < -0.9, "bottom normal points down");
                checked += 1;
            }
        }
        assert!(checked > 20);
    }

    #[test]
    fn normals_point_outward_everywhere() {
        let b = belt();
        let n = 200;
        for i in 0..n {
            let p = b.at(b.length() * i as f32 / n as f32);
            assert!(p.normal.dot(p.pos - Vec2::new(0.0, 0.5)) > 0.0);
            assert!((p.normal.length() - 1.0).abs() < 1e-3);
        }
    }
}
