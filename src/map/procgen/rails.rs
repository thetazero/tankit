//! How a railway's rails lie on the ground: in straight pieces, more of them
//! where the ground bends, so none goes under it.

use glam::{Vec2, Vec3};

use crate::map::Map;
use crate::render::material::Material;

/// How far apart the ground's looked at along a rail, m.
const STEP: f32 = 0.25;
/// A rail's half-thickness, and its middle's height over the ground.
const HALF: f32 = 0.08;
const UP: f32 = 0.16;
/// How far the ground may rise over a piece, or fall away under it, between
/// its ends (a rail's top is `UP + HALF` over it).
const OVER: f32 = 0.05;
const UNDER: f32 = 0.2;

/// The ground a railway's laid on at `p`: past the arena's edge, what's drawn
/// there.
pub fn ground(m: &Map, p: Vec2) -> f32 {
    let play = m.ground_height(p.x, p.y);
    if m.out(p) <= 0.0 {
        return play;
    }
    let beyond = m.ground_beyond(p.x, p.y);
    // The land beyond drops away under the arena, from its last corners
    // outside: across the dip that leaves, level with the edge.
    let dips = m.scenery_terrain.as_ref().is_some_and(|t| {
        let lo = t.origin + ((p - t.origin) / t.cell).floor() * t.cell;
        [Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::ONE]
            .iter()
            .any(|&c| m.out(lo + c * t.cell) < -0.01)
    });
    if dips { beyond.max(play) } else { beyond }
}

/// The ends of the pieces a rail `len` long is laid in, over `ground` (its
/// height so far along): how far along each is, and the ground there.
fn knots(len: f32, ground: impl Fn(f32) -> f32) -> Vec<(f32, f32)> {
    let n = (len / STEP).ceil().max(1.0) as usize;
    let at: Vec<(f32, f32)> = (0..=n)
        .map(|k| len * k as f32 / n as f32)
        .map(|s| (s, ground(s)))
        .collect();
    let mut out = vec![at[0]];
    let mut todo = vec![(0, n)];
    while let Some((a, b)) = todo.pop() {
        // Where it's furthest off a straight piece, against what it may be.
        let off = |k: usize| {
            let t = (k - a) as f32 / (b - a) as f32;
            let d = at[k].1 - (at[a].1 + (at[b].1 - at[a].1) * t);
            (d / OVER).max(-d / UNDER)
        };
        let worst = (a + 1..b).max_by(|&j, &k| off(j).total_cmp(&off(k)));
        match worst.filter(|&k| off(k) > 1.0) {
            Some(k) => todo.extend([(k, b), (a, k)]),
            None => out.push(at[b]),
        }
    }
    out
}

/// Lays two rails `gauge` apart from `a` to `b`, level with each other, over
/// `ground`.
pub fn lay(
    map: &mut Map,
    (a, b): (Vec2, Vec2),
    gauge: f32,
    color: Vec3,
    ground: impl Fn(&Map, Vec2) -> f32,
) {
    let len = a.distance(b);
    let d = (b - a) / len.max(1e-3);
    let side = d.perp() * gauge * 0.5;
    // (Over whatever's highest of what's under either rail's two edges.)
    let edge = d.perp() * HALF;
    let under = [
        side - edge,
        side + edge,
        -side - edge,
        -side + edge,
        Vec2::ZERO,
    ];
    let knots = knots(len, |s| {
        let p = a + d * s;
        let over = under.iter().map(|&o| ground(map, p + o));
        over.fold(f32::MIN, f32::max)
    });
    for w in knots.windows(2) {
        let end = |(s, y): (f32, f32), off: Vec2| {
            let p = a + d * s + off;
            Vec3::new(p.x, y + UP, p.y)
        };
        for off in [side, -side] {
            map.segment(
                end(w[0], off),
                end(w[1], off),
                HALF,
                color,
                Material::Metal,
                false,
            );
        }
    }
}

/// Panics if the ground's over any of `m`'s rails (which are `color`).
#[cfg(test)]
pub(super) fn assert_none_buried(m: &Map, color: Vec3, what: &str) {
    // (`separate_coplanar_faces` resizes them by millimetres.)
    let mut rails = 0;
    for o in &m.oboxes {
        if o.solid
            || (o.half.x - HALF).abs() > 0.02
            || o.color != color
            || o.side_mat != Material::Metal
        {
            continue;
        }
        rails += 1;
        let along = Vec2::from_angle(std::f32::consts::FRAC_PI_2 - o.yaw);
        let reach = o.half.y * o.tilt.cos();
        let n = (reach * 2.0 / 0.2).ceil().max(1.0) as i32;
        for k in 0..=n {
            let u = reach * (2.0 * k as f32 / n as f32 - 1.0);
            let p = o.center + along * u;
            let y = (o.y0 + o.y1) * 0.5 - u * o.tilt.tan();
            let ground = if m.out(p) > 0.0 {
                ground(m, p)
            } else {
                m.ground_near(p.x, y, p.y)
            };
            assert!(
                ground < y + HALF,
                "{what}: a rail {:.2} m under at {p}",
                ground - y - HALF
            );
        }
    }
    let tracks = &m.skeleton.as_ref().unwrap().tracks;
    assert!(rails > 0 || tracks.is_empty(), "{what}: no rails");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::procgen::foundry::Ground;
    use crate::map::procgen::{Look, map};

    #[test]
    fn pieces_follow_the_ground() {
        // Flat or an even slope: one piece. A rise between: over it.
        assert_eq!(knots(100.0, |s| s * 0.1).len(), 2);
        let hill = |s: f32| 3.0 * (-((s - 50.0) / 12.0).powi(2)).exp();
        let k = knots(100.0, hill);
        for w in k.windows(2) {
            for j in 0..=20 {
                let t = j as f32 / 20.0;
                let (s, y) = (
                    w[0].0 + (w[1].0 - w[0].0) * t,
                    w[0].1 + (w[1].1 - w[0].1) * t,
                );
                assert!(hill(s) < y + OVER + 0.02, "under at {s}");
                assert!(hill(s) > y - UNDER - 0.02, "over at {s}");
            }
        }
    }

    #[test]
    fn no_rail_is_buried() {
        let g = Ground::Desert;
        for seed in [1, 4] {
            let m = map(&crate::depot_spec(seed, Look::Imperium, g));
            assert_none_buried(
                &m,
                crate::map::procgen::skeleton::RAIL,
                &format!("depot {seed}"),
            );
        }
        for seed in [0, 2] {
            let m = map(&crate::foundry_spec(seed, Look::Imperium, g));
            assert_none_buried(
                &m,
                crate::map::procgen::skeleton::RAIL,
                &format!("foundry {seed}"),
            );
        }
        let m = map(&crate::mine_spec(0, Look::Imperium, g));
        assert_none_buried(&m, crate::map::procgen::mine::RAIL, "mine 0");
    }
}
