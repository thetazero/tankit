//! A ball swept along a line against the static world (the terrain
//! heightfield and the map's compound of boxes and hulls), solved in closed
//! form for triangles and boxes.
//!
//! Every road wheel sweeps a ball down from its mount each step. parry's
//! general shape cast runs an iterative GJK sweep for each triangle or box
//! the ball might touch, and that was most of the sim's time. The same sweep
//! in closed form (faces, then edges, then corners) gives the same contact,
//! up to float rounding, many times faster. Rocks, ramps and crystals (convex
//! hulls) still go through parry's cast; there are few of them.

use glam::Vec3;
use rapier3d::parry::bounding_volume::Aabb;
use rapier3d::parry::math::Pose;
use rapier3d::parry::query::{ShapeCastOptions, cast_shapes};
use rapier3d::parry::shape::{Ball, Compound, Cuboid, HeightField};

/// A first touch: how far the ball travelled, and the surface's outward
/// normal there, pointing at the ball.
pub type Hit = (f32, Vec3);

/// Keeps the earlier of two hits.
fn earliest(best: &mut Option<Hit>, hit: Option<Hit>) {
    if let Some(h) = hit {
        if best.is_none_or(|b| h.0 < b.0) {
            *best = Some(h);
        }
    }
}

/// The swept ball's bounds, in the frame its center and direction are in.
fn swept_aabb(c: Vec3, d: Vec3, r: f32, max: f32) -> Aabb {
    let end = c + d * max;
    Aabb::new(c.min(end) - Vec3::splat(r), c.max(end) + Vec3::splat(r))
}

/// Where a ball of radius `r` at `center`, moving along the unit vector `dir`,
/// first touches the heightfield (placed at `pose`) within `max` of travel
/// (world space). A ball that starts touching it hits at 0.
pub fn ball_heightfield(
    hf: &HeightField,
    pose: &Pose,
    center: Vec3,
    dir: Vec3,
    r: f32,
    max: f32,
) -> Option<Hit> {
    let c = pose.inverse_transform_point(center);
    let d = pose.rotation.inverse() * dir;
    let mut best: Option<Hit> = None;
    hf.map_elements_in_local_aabb(&swept_aabb(c, d, r, max), &mut |_, tri| {
        let limit = best.map_or(max, |b| b.0);
        earliest(&mut best, ball_triangle(c, d, r, limit, tri.a, tri.b, tri.c));
    });
    best.map(|(t, n)| (t, pose.rotation * n))
}

/// Like [`ball_heightfield`], against a compound (placed at `pose`): boxes in
/// closed form, any other part through parry's cast.
pub fn ball_compound(
    compound: &Compound,
    pose: &Pose,
    center: Vec3,
    dir: Vec3,
    r: f32,
    max: f32,
) -> Option<Hit> {
    let c = pose.inverse_transform_point(center);
    let d = pose.rotation.inverse() * dir;
    let aabb = swept_aabb(c, d, r, max);
    let parts = compound.shapes();
    let mut best: Option<Hit> = None;
    for i in compound.bvh().intersect_aabb(&aabb) {
        let (part_pose, shape) = &parts[i as usize];
        let limit = best.map_or(max, |b| b.0);
        let hit = if let Some(cuboid) = shape.as_cuboid() {
            let pc = part_pose.inverse_transform_point(c);
            let pd = part_pose.rotation.inverse() * d;
            ball_cuboid(cuboid, pc, pd, r, limit).map(|(t, n)| (t, part_pose.rotation * n))
        } else {
            let options = ShapeCastOptions {
                max_time_of_impact: limit,
                target_distance: 0.0,
                stop_at_penetration: true,
                compute_impact_geometry_on_penetration: true,
            };
            let at = Pose::from_translation(c);
            cast_shapes(part_pose, Vec3::ZERO, &**shape, &at, d, &Ball::new(r), options)
                .ok()
                .flatten()
                // parry gives the normal in the part's own frame.
                .map(|h| (h.time_of_impact, part_pose.rotation * h.normal1.normalize_or_zero()))
        };
        earliest(&mut best, hit);
    }
    best.map(|(t, n)| (t, pose.rotation * n))
}

/// A ball (center `c`, radius `r`, moving along unit `d` up to `max`)
/// against a box centered on the origin, in the box's frame.
fn ball_cuboid(cuboid: &Cuboid, c: Vec3, d: Vec3, r: f32, max: f32) -> Option<Hit> {
    let h = cuboid.half_extents;
    // Already touching.
    let q = c.clamp(-h, h);
    let off = c - q;
    let d2 = off.length_squared();
    if d2 < r * r {
        if d2 > 1e-12 {
            return Some((0.0, off / d2.sqrt()));
        }
        // The center is inside the box: out through the nearest face.
        let gap = h - c.abs();
        let axis = if gap.x <= gap.y && gap.x <= gap.z {
            Vec3::X
        } else if gap.y <= gap.z {
            Vec3::Y
        } else {
            Vec3::Z
        };
        return Some((0.0, axis * c.dot(axis).signum()));
    }

    let mut best: Option<Hit> = None;
    // The faces: the center reaches r off the face, over the face.
    for axis in 0..3 {
        for sign in [-1.0f32, 1.0] {
            let (ci, di, hi) = (c[axis] * sign, d[axis] * sign, h[axis]);
            if di >= 0.0 || ci < hi + r {
                continue;
            }
            let t = (ci - hi - r) / -di;
            if t > max {
                continue;
            }
            let p = c + d * t;
            let (j, k) = ((axis + 1) % 3, (axis + 2) % 3);
            if p[j].abs() <= h[j] && p[k].abs() <= h[k] {
                let mut n = Vec3::ZERO;
                n[axis] = sign;
                earliest(&mut best, Some((t, n)));
            }
        }
    }
    // A face hit is the earliest there can be.
    if best.is_some() {
        return best;
    }
    let corner = |i: u32| {
        Vec3::new(
            if i & 1 == 0 { -h.x } else { h.x },
            if i & 2 == 0 { -h.y } else { h.y },
            if i & 4 == 0 { -h.z } else { h.z },
        )
    };
    // The twelve edges join corners that differ in one coordinate.
    for i in 0..8u32 {
        for bit in [1u32, 2, 4] {
            if i & bit == 0 {
                earliest(&mut best, ball_segment(c, d, r, max, corner(i), corner(i | bit)));
            }
        }
    }
    for i in 0..8 {
        earliest(&mut best, ball_point(c, d, r, max, corner(i)));
    }
    best
}

/// The first touch, at a distance up to `max`, of a ball of radius `r` moving
/// from `c` along the unit vector `d`, with triangle (a, b, e).
fn ball_triangle(c: Vec3, d: Vec3, r: f32, max: f32, a: Vec3, b: Vec3, e: Vec3) -> Option<Hit> {
    let n = (b - a).cross(e - a);
    let area2 = n.length();
    if area2 < 1e-12 {
        return None;
    }
    let mut n = n / area2;
    let mut s = (c - a).dot(n);
    if s < 0.0 {
        n = -n;
        s = -s;
    }

    // Already touching.
    let q = closest_on_triangle(c, a, b, e);
    let off = c - q;
    let d2 = off.length_squared();
    if d2 < r * r {
        let normal = if d2 > 1e-12 { off / d2.sqrt() } else { n };
        return Some((0.0, normal));
    }

    // Too far to reach, even straight on.
    if s - r > max {
        return None;
    }

    // The face: the ball's lowest point reaches the plane inside the triangle.
    // That's the earliest there can be: edges and corners only touch later,
    // or where the face misses.
    let dn = d.dot(n);
    if dn < 0.0 {
        let t = (s - r) / -dn;
        let p = c + d * t - n * r;
        if t <= max && inside(p, a, b, e, n) {
            return Some((t, n));
        }
    }

    let mut best: Option<Hit> = None;
    for (p0, p1) in [(a, b), (b, e), (e, a)] {
        earliest(&mut best, ball_segment(c, d, r, max, p0, p1));
    }
    for v in [a, b, e] {
        earliest(&mut best, ball_point(c, d, r, max, v));
    }
    best
}

/// The ball's center reaching distance `r` from segment p0-p1, along its
/// length (its ends are corners, tested on their own). Assumes the ball
/// doesn't start touching it.
fn ball_segment(c: Vec3, d: Vec3, r: f32, max: f32, p0: Vec3, p1: Vec3) -> Option<Hit> {
    let edge = p1 - p0;
    let ee = edge.dot(edge);
    let m = c - p0;
    let (md, nd) = (m.dot(edge), d.dot(edge));
    let m_perp = m - edge * (md / ee);
    let d_perp = d - edge * (nd / ee);
    let qa = d_perp.dot(d_perp);
    if qa < 1e-12 {
        return None;
    }
    let qb = m_perp.dot(d_perp);
    let qc = m_perp.dot(m_perp) - r * r;
    let disc = qb * qb - qa * qc;
    if disc < 0.0 {
        return None;
    }
    let t = (-qb - disc.sqrt()) / qa;
    let along = (md + nd * t) / ee;
    if !(0.0..=max).contains(&t) || !(0.0..=1.0).contains(&along) {
        return None;
    }
    Some((t, (c + d * t - (p0 + edge * along)) / r))
}

/// The ball's center reaching distance `r` from point `v`. Assumes the ball
/// doesn't start touching it.
fn ball_point(c: Vec3, d: Vec3, r: f32, max: f32, v: Vec3) -> Option<Hit> {
    let m = c - v;
    let qb = m.dot(d);
    let qc = m.dot(m) - r * r;
    let disc = qb * qb - qc;
    if disc < 0.0 {
        return None;
    }
    let t = -qb - disc.sqrt();
    (0.0..=max)
        .contains(&t)
        .then(|| (t, (c + d * t - v) / r))
}

/// Whether `p`, on the plane of triangle (a, b, e) with unit normal `n`, lies
/// inside it (edges included), whichever way it winds.
fn inside(p: Vec3, a: Vec3, b: Vec3, e: Vec3, n: Vec3) -> bool {
    let s = [
        (b - a).cross(p - a).dot(n),
        (e - b).cross(p - b).dot(n),
        (a - e).cross(p - e).dot(n),
    ];
    s.iter().all(|&x| x >= 0.0) || s.iter().all(|&x| x <= 0.0)
}

/// The point of triangle (a, b, c) closest to `p` (Ericson, Real-Time
/// Collision Detection, 5.1.5).
fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier3d::parry::shape::{Shape, SharedShape};
    use rapier3d::parry::utils::Array2;

    /// Casts many random wheel-like balls with both `ours` and parry's own
    /// cast against `shape` (at `pose`), and checks they agree.
    fn agrees(
        shape: &dyn Shape,
        pose: &Pose,
        span: f32,
        ours: impl Fn(Vec3, Vec3, f32, f32) -> Option<Hit>,
    ) {
        let mut rng = crate::rng::Rng::new(7);
        let mut hits = 0;
        for _ in 0..4000 {
            let center = Vec3::new(
                rng.range(-span, span),
                rng.range(0.5, 3.5),
                rng.range(-span, span),
            );
            // Mostly down, tilted as a tank on a slope or rolling over is.
            let dir = Vec3::new(rng.range(-0.6, 0.6), -1.0, rng.range(-0.6, 0.6)).normalize();
            let r = rng.range(0.2, 0.5);
            let max = rng.range(0.5, 2.5);
            let options = ShapeCastOptions {
                max_time_of_impact: max,
                target_distance: 0.0,
                stop_at_penetration: true,
                compute_impact_geometry_on_penetration: true,
            };
            let at = Pose::from_translation(center);
            let want = cast_shapes(pose, Vec3::ZERO, shape, &at, dir, &Ball::new(r), options)
                .unwrap();
            let got = ours(center, dir, r, max);
            match (want, got) {
                (None, None) => {}
                (Some(w), Some((t, n))) => {
                    hits += 1;
                    // parry's GJK sweep stops within its tolerance of the
                    // touch, a few mm in; ours is exact.
                    assert!(
                        (w.time_of_impact - t).abs() < 1e-2,
                        "toi {} vs {t}",
                        w.time_of_impact
                    );
                    if t > 0.0 {
                        let wn = w.normal1.normalize();
                        assert!(wn.dot(n) > 0.995, "normal {wn} vs {n}");
                    }
                }
                (w, g) => {
                    // Grazing within rounding of the reach is fine either way.
                    let t = w.map(|w| w.time_of_impact).or(g.map(|g| g.0)).unwrap();
                    assert!((t - max).abs() < 1e-2, "{w:?} vs {g:?} (max {max})");
                }
            }
        }
        assert!(hits > 500, "only {hits} hits: the test isn't testing much");
    }

    #[test]
    fn heightfield_matches_parry() {
        let n = 24;
        let mut data = Vec::new();
        for ix in 0..n {
            for iz in 0..n {
                let (x, z) = (ix as f32, iz as f32);
                let jag = ((ix * 7 + iz * 3) % 5) as f32 * 0.15;
                data.push((x * 0.7).sin() * 0.8 + (z * 0.45).cos() * 0.6 + jag);
            }
        }
        let hf = HeightField::new(Array2::new(n, n, data), Vec3::new(46.0, 1.0, 46.0));
        let pose = Pose::from_translation(Vec3::new(3.0, -0.5, -2.0));
        agrees(&hf, &pose, 20.0, |c, d, r, max| {
            ball_heightfield(&hf, &pose, c, d, r, max)
        });
    }

    #[test]
    fn compound_matches_parry() {
        // Kerbs, a tilted slab, a tall wall and a rock, scattered about.
        let mut rng = crate::rng::Rng::new(3);
        let mut parts = Vec::new();
        for i in 0..40 {
            let at = Vec3::new(rng.range(-8.0, 8.0), rng.range(-0.3, 0.8), rng.range(-8.0, 8.0));
            let rot = glam::Quat::from_rotation_y(rng.range(0.0, 3.0))
                * glam::Quat::from_rotation_x(rng.range(-0.3, 0.3));
            let shape = if i % 5 == 0 {
                let pts: Vec<Vec3> = (0..12)
                    .map(|_| {
                        Vec3::new(rng.range(-0.8, 0.8), rng.range(-0.5, 0.5), rng.range(-0.8, 0.8))
                    })
                    .collect();
                SharedShape::convex_hull(&pts).unwrap()
            } else {
                SharedShape::cuboid(rng.range(0.1, 1.5), rng.range(0.05, 0.6), rng.range(0.1, 1.5))
            };
            parts.push((Pose::from_parts(at, rot), shape));
        }
        let compound = SharedShape::compound(parts);
        let pose = Pose::from_translation(Vec3::new(0.5, 0.0, -0.25));
        let c = compound.as_compound().unwrap();
        agrees(c, &pose, 9.0, |ctr, d, r, max| ball_compound(c, &pose, ctr, d, r, max));
    }
}
