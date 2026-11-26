//! An Ithessar city's layout: a strip along a winding plasma river, its
//! arena the strip and not a square (`Map::band`). The river first, its
//! quays and bridges; great buildings on the land outside its bends, a
//! court toward each end; then the city's own passes fill the rest in
//! (`city`): lanes off the quays, houses along them, barricades where a
//! street crosses the strip's edge.

use std::sync::Arc;

use glam::Vec2;

use super::channel::{Channel, WALL};
use super::city::{self, Plan};
use super::land::{Land, Swell};
use super::skeleton::*;
use crate::map::Band;
use crate::rng::Rng;

/// How far the strip reaches off the river's middle.
const REACH: f32 = 72.0;

pub fn skeleton(rng: &mut Rng, sides: usize, plan: &Plan) -> Skeleton {
    let c = Channel::wind(rng);
    // As big a square as holds the strip.
    let course = c.course();
    let lo = course.iter().copied().reduce(Vec2::min).unwrap();
    let hi = course.iter().copied().reduce(Vec2::max).unwrap();
    let size = (((hi - lo).max_element() + REACH * 2.0 + 50.0) / 20.0).ceil() * 20.0;
    let mut c = c.moved(Vec2::splat(size * 0.5) - (lo + hi) * 0.5);
    let mut band = Band {
        line: c.course(),
        reach: REACH,
        bulges: Vec::new(),
    };
    let inside = |p: Vec2, m: f32| p.min_element() > m && p.max_element() < size - m;
    // Off every stretch of the river, not only the one it's by.
    let dry = |c: &Channel, p: Vec2, r: f32| !c.wet(p, r);

    // Great buildings, each beside the river with its doors to the quay
    // and along the bank both ways.
    let want = if rng.chance(0.6) { 2 } else { 1 };
    let which = rng.index(3);
    let mut platforms: Vec<Platform> = Vec::new();
    let mut landmarks: Vec<Landmark> = Vec::new();
    let mut ways: Vec<(Vec2, Vec2, bool)> = Vec::new();
    let mut tries = 0;
    while landmarks.len() < want && tries < 1500 {
        tries += 1;
        let half = rng.range(35.0, 39.0);
        let out = half * 1.15 + 14.0;
        let near = if landmarks.is_empty() { 0.35 } else { 0.8 };
        let t = rng.range(-near, near) * c.play;
        let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
        let p = c.point(t, side * (c.reach() + out - 4.0));
        if !inside(p, out + 2.0)
            || !dry(&c, p, c.reach() - c.half - WALL + out - 12.0)
            || platforms.iter().any(|o| o.center.distance(p) < 190.0)
        {
            continue;
        }
        let quay = c.point(t, side * (c.half + WALL + 10.0));
        let d = (quay - p).normalize();
        let mut doors = vec![d];
        for turn in [1.9f32, -1.9] {
            let e = Vec2::from_angle(turn).rotate(d);
            let to = p + e * (out + 14.0);
            if dry(&c, to, 10.0) && inside(to, 6.0) {
                doors.push(e);
                ways.push((p, to, false));
            }
        }
        ways.push((p, quay, false));
        band.bulges.push((p, out + 10.0));
        platforms.push(Platform {
            center: p,
            half,
            round: rng.range(14.0, 22.0),
            height: 2.0,
            ramp: 14.0,
        });
        let radius = rng.range(20.0, 23.0);
        landmarks.push(Landmark {
            center: p,
            radius,
            precinct: (radius + 16.0).min(half - 5.0),
            fallen: rng.chance(0.7).then(|| -d),
            doors,
            kind: LandmarkKind::Great(((which + landmarks.len()) % 3) as u8),
            clear: 0.0,
            reach: (radius + 16.0).min(half - 5.0),
            facing: Vec2::X,
        });
    }
    let heart = landmarks
        .first()
        .map_or_else(|| c.point(0.0, c.half + WALL + 24.0), |l| l.center);

    // A court for each side, toward an end of the strip each, on whichever
    // bank.
    let court = 42.0;
    let mut bases: Vec<Base> = Vec::new();
    let mut tries = 0;
    while bases.len() < sides && tries < 6000 {
        tries += 1;
        let ease = tries as f32 / 6000.0;
        let end = if bases.len() % 2 == 0 { -1.0 } else { 1.0 };
        let t = end * rng.range(0.5 - 0.4 * ease, 0.97) * c.play;
        let off = (c.reach() + rng.range(10.0, 26.0)) * if rng.chance(0.5) { 1.0 } else { -1.0 };
        let m = c.point(t, off);
        let to = c.point(c.local(m).0, 0.0);
        let inward = Vec2::new(
            if to.x > m.x { 1.0 } else { -1.0 },
            if to.y > m.y { 1.0 } else { -1.0 },
        );
        let clear = inside(m, court * 0.5 + 4.0)
            && band.out(m) < 14.0
            && dry(&c, m, 44.0)
            && platforms
                .iter()
                .all(|p| p.sdf(m) - p.ramp > court * 0.75 + 8.0)
            && bases
                .iter()
                .all(|b| b.middle().distance(m) > 150.0 * (1.0 - 0.4 * ease));
        if clear {
            bases.push(Base {
                corner: m - inward * court * 0.5,
                inward,
                size: court,
                kind: BaseKind::Court,
            });
            band.bulges.push((m, court * 0.75 + 4.0));
        }
    }

    let mut sk = Skeleton {
        size,
        landmarks,
        heart,
        bases,
        ..Skeleton::default()
    };
    let mut land = Land::rolling(rng, size, &[]);
    land.swells.retain(|s| !matches!(s, Swell::Cut(_)));
    c.settle(rng, &land);
    land.platforms = platforms;
    land.channel = Some(c);
    sk.land = land;
    sk.band = Some(Arc::new(band));

    city::civics(&mut sk, rng, plan);
    city::roads(&mut sk, rng, plan, &ways);
    city::gates_and_lines(&mut sk, rng);
    city::plaza_dressing(&mut sk, rng);
    city::frontages(&mut sk, rng, plan);
    city::clear_courts(&mut sk, rng);
    cordon(&mut sk, rng);
    city::repairs(&mut sk, rng);
    city::level(&mut sk);
    sk
}

/// Something across every street where it crosses the strip's edge, and
/// clumps of it wherever there's a way out between buildings.
fn cordon(sk: &mut Skeleton, rng: &mut Rng) {
    let mut edges: Vec<Edge> = Vec::new();
    let mut across = |at: Vec2, d: Vec2, half: f32, kind: EdgeKind| {
        edges.push(Edge {
            start: at,
            along: Vec2::new(-d.y, d.x),
            inward: -d,
            from: -half,
            to: half,
            kind,
        });
    };
    let mut crossed: Vec<Vec2> = Vec::new();
    for s in &sk.streets {
        if s.rank == Rank::Crossing {
            continue;
        }
        let (a, b) = (sk.out(s.from), sk.out(s.to));
        if a * b > 0.0 {
            continue;
        }
        let d = if a < 0.0 { s.dir() } else { -s.dir() };
        let at = s.from.lerp(s.to, a / (a - b));
        if crossed.iter().any(|p| p.distance(at) < 9.0) {
            continue;
        }
        crossed.push(at);
        let wet = sk
            .land
            .channel
            .as_ref()
            .is_some_and(|c| c.wet(at, s.half + 8.0));
        let kind = if rng.chance(0.45) && !wet {
            EdgeKind::Blockade
        } else {
            EdgeKind::Barricade
        };
        across(at, d, s.half + 2.0, kind);
    }
    // Round the rest of the edge.
    let n = (sk.size / 9.0) as i32;
    for i in 0..n {
        for j in 0..n {
            let p = Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * 9.0;
            if sk.out(p).abs() > 4.5 || crossed.iter().any(|q| q.distance(p) < 12.0) {
                continue;
            }
            let g = Vec2::new(
                sk.out(p + Vec2::X) - sk.out(p - Vec2::X),
                sk.out(p + Vec2::Y) - sk.out(p - Vec2::Y),
            )
            .normalize_or(Vec2::X);
            let walled = sk.lots.iter().any(|l| l.footprint.distance(p) < 3.0);
            let wet = sk.land.channel.as_ref().is_some_and(|c| c.wet(p, 3.0));
            if !walled && !wet && rng.chance(0.75) {
                across(p, g, 5.0, EdgeKind::Barricade);
            }
        }
    }
    sk.edges = edges;
}
