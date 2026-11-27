//! What a layout says about getting round a map: the ways worth taking to a
//! fight (streets, over bridges and under them, through tunnels and digs,
//! up ramps, along canyons and washes) and the places worth being. The same
//! for every biome: it's all read off the `Skeleton`. Then what a bot can't
//! drive, as built, is left out.

use glam::{Vec2, Vec3};

use super::crossing::{Crossing, Kind as Over};
use super::mine::{line_at, line_len};
use super::skeleton::{Rank, Skeleton};
use crate::ai::nav::NavGrid;
use crate::map::Map;

/// How far apart a way's points are, m.
const STEP: f32 = 14.0;
/// A way's no use shorter than this, m.
const SHORTEST: f32 = 26.0;
/// How far in from the arena's edge they keep, m.
const EDGE: f32 = 12.0;

/// What a way runs along.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Street,
    Bridge,
    Tunnel,
    Dig,
    Ramp,
    Canyon,
    Wash,
}

#[derive(Clone, Debug)]
pub struct Way {
    pub kind: Kind,
    /// Its points, each about on its floor.
    pub line: Vec<Vec3>,
}

/// The points `STEP` or so apart along `line`, its ends too.
fn along(line: &[Vec2]) -> Vec<Vec2> {
    if line.len() < 2 {
        return Vec::new();
    }
    let total = line_len(line, line.len() - 1);
    let n = (total / STEP).ceil().max(1.0) as usize;
    (0..=n)
        .map(|k| line_at(line, total * k as f32 / n as f32).0)
        .collect()
}

impl Crossing {
    /// The height of the way over (or through) it.
    fn level(&self) -> f32 {
        match self.kind {
            Over::Bridge => self.top,
            Over::Tunnel => self.floor,
        }
    }

    fn carries(&self, p: Vec2) -> bool {
        let t = (p - self.from).dot(self.dir());
        (-1.0..self.len() + 1.0).contains(&t) && self.distance(p) < self.half
    }
}

impl Skeleton {
    /// The height of a road at `p`: a bridge's deck or a tunnel's floor on
    /// one, else the ground's.
    fn road_level(&self, p: Vec2) -> f32 {
        self.crossings
            .iter()
            .find(|x| x.carries(p))
            .map_or_else(|| self.ground(p), Crossing::level)
    }

    /// Every way the layout has.
    pub fn ways(&self) -> Vec<Way> {
        let mut out = Vec::new();
        let mut put = |kind, line: Vec<Vec3>| out.push(Way { kind, line });
        let ground = |p: Vec2| Vec3::new(p.x, self.ground(p), p.y);
        // Streets, bends and bridges and all.
        let mut roads: Vec<Vec<Vec2>> = Vec::new();
        for s in &self.streets {
            if matches!(s.rank, Rank::Beyond | Rank::Rail) {
                continue;
            }
            match roads.last_mut() {
                Some(r) if r.last().is_some_and(|p| p.distance(s.from) < 0.5) => r.push(s.to),
                _ => roads.push(vec![s.from, s.to]),
            }
        }
        for r in &roads {
            let line = along(r)
                .into_iter()
                .map(|p| Vec3::new(p.x, self.road_level(p), p.y))
                .collect();
            put(Kind::Street, line);
        }
        for x in &self.crossings {
            let kind = match x.kind {
                Over::Bridge => Kind::Bridge,
                Over::Tunnel => Kind::Tunnel,
            };
            let run = 6.0 / x.len();
            let line = [-run, 0.2, 0.5, 0.8, 1.0 + run]
                .map(|t| x.from.lerp(x.to, t))
                .map(|p| Vec3::new(p.x, self.road_level(p), p.y));
            put(kind, line.to_vec());
        }
        for d in &self.land.digs {
            // In at one mouth, through the hall, out at another.
            let line: Vec<Vec3> = along(&d.through)
                .into_iter()
                .map(|p| d.to_map(p))
                .map(|p| Vec3::new(p.x, d.floor, p.y))
                .collect();
            put(Kind::Dig, line);
        }
        for r in &self.land.high.ramps {
            let dir = (r.top - r.foot).normalize();
            let line = [
                r.foot - dir * 4.0,
                r.foot.lerp(r.top, 0.5),
                r.top + dir * 6.0,
            ];
            put(Kind::Ramp, line.map(ground).to_vec());
        }
        for c in &self.land.high.channels {
            put(
                Kind::Canyon,
                along(&c.line).into_iter().map(ground).collect(),
            );
        }
        for w in &self.land.washes {
            put(Kind::Wash, along(&w.line).into_iter().map(ground).collect());
        }
        out
    }

    /// Places worth being: the great buildings, squares, halls, bridges,
    /// the tops of ramps.
    pub fn posts(&self) -> Vec<Vec3> {
        let ground = |p: Vec2| Vec3::new(p.x, self.ground(p), p.y);
        let mut out: Vec<Vec3> = Vec::new();
        out.push(ground(self.heart));
        for l in &self.landmarks {
            out.extend(
                l.doors
                    .iter()
                    .map(|&d| ground(l.center + d * (l.radius + 8.0))),
            );
        }
        out.extend(self.squares.iter().map(|s| ground(s.center)));
        for x in &self.crossings {
            let p = (x.from + x.to) * 0.5;
            out.push(Vec3::new(p.x, x.level(), p.y));
        }
        for d in &self.land.digs {
            if let Some(hall) = d.rooms.get(1) {
                let y = hall.min.y + (hall.max.y - hall.min.y) * 0.25;
                let p = d.to_map(Vec2::new((hall.min.x + hall.max.x) * 0.5, y));
                out.push(Vec3::new(p.x, d.floor, p.y));
            }
        }
        for r in &self.land.high.ramps {
            out.push(ground(r.top + (r.top - r.foot).normalize() * 8.0));
        }
        out
    }
}

/// `p` on the floor a bot can drive nearest it, if there's one within
/// `reach`.
fn settle(nav: &NavGrid, p: Vec3, reach: f32) -> Option<Vec3> {
    let rings = (reach / 2.0) as usize;
    (0..=rings).find_map(|r| {
        let n = if r == 0 { 1 } else { 8 };
        (0..n).find_map(|k| {
            let a = k as f32 / 8.0 * std::f32::consts::TAU;
            let q = p + Vec3::new(a.cos(), 0.0, a.sin()) * r as f32 * 2.0;
            nav.floor_at(q).map(|y| Vec3::new(q.x, y, q.z))
        })
    })
}

/// Gives a map built from a layout the layout's ways and posts: those a bot
/// can drive, as built. Added to any ways it has already.
pub fn lay(map: &mut Map) {
    let Some(sk) = map.skeleton.clone() else {
        return;
    };
    let nav = NavGrid::build(map);
    let size = sk.size;
    let inside = |p: &Vec3| p.x.min(p.z) > EDGE && p.x.max(p.z) < size - EDGE && !map.is_out(*p);
    let mut found: Vec<Vec<Vec3>> = Vec::new();
    for way in sk.ways() {
        // Each stretch of it that's open, driven from point to point.
        let mut run: Vec<Vec3> = Vec::new();
        let mut runs = Vec::new();
        for p in way.line {
            let Some(q) = settle(&nav, p, 4.0).filter(inside) else {
                runs.push(std::mem::take(&mut run));
                continue;
            };
            // (Not if it's a long way round to the next.)
            let near = run.last().is_none_or(|&last: &Vec3| {
                if nav.clear_from(last, Vec2::new(q.x, q.z)) {
                    return true;
                }
                let mut at = last;
                let mut len = 0.0;
                let legs = nav.route(last, q);
                for &l in &legs {
                    len += at.distance(l);
                    at = l;
                }
                !legs.is_empty() && len < last.distance(q) * 1.6 + 6.0
            });
            if !near {
                runs.push(std::mem::take(&mut run));
            }
            run.push(q);
        }
        runs.push(run);
        found.extend(runs.into_iter().filter(|r| {
            let len: f32 = r.windows(2).map(|w| w[0].distance(w[1])).sum();
            r.len() >= 2 && len >= SHORTEST
        }));
    }
    let from = map.bases.iter().flatten().next().copied();
    // Places one of the sides can get to.
    let posts = sk
        .posts()
        .into_iter()
        .filter_map(|p| settle(&nav, p, 10.0))
        .filter(inside)
        .filter(|&p| {
            from.is_none_or(|b| {
                let b = Vec3::new(b.x, f32::INFINITY, b.y);
                !nav.route(b, p).is_empty()
            })
        })
        .collect();
    // (Those it had may have been built over since.)
    for r in &mut map.routes {
        r.retain(|&p| nav.floor_at(p).is_some());
    }
    map.routes.retain(|r| r.len() >= 2);
    map.routes.extend(found);
    map.posts = posts;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::procgen::Look;

    /// Every kind of generated map keeps its layout, and has ways and posts
    /// read off it, every point of them on a floor a bot can drive.
    #[test]
    fn every_map_has_its_layouts_ways() {
        use crate::map::procgen::foundry::Ground;
        for (name, spec, kind) in [
            ("city", crate::city_spec(1, Look::Imperium), Kind::Street),
            ("mine", crate::mine_spec(1, Look::Ithessar), Kind::Dig),
            ("desert", crate::desert_spec(1), Kind::Canyon),
            (
                "foundry",
                crate::foundry_spec(0, Look::Imperium, Ground::Desert),
                Kind::Street,
            ),
        ] {
            let m = crate::map::procgen::map(&spec);
            let sk = m.skeleton.as_ref().expect("its layout");
            assert!(
                sk.ways().iter().any(|w| w.kind == kind),
                "{name}: no {kind:?}"
            );
            assert!(m.routes.len() >= 3, "{name}: {} ways", m.routes.len());
            assert!(m.posts.len() >= 2, "{name}: {} posts", m.posts.len());
            let nav = NavGrid::build(&m);
            for p in m.routes.iter().flatten().chain(&m.posts) {
                assert!(nav.floor_at(*p).is_some(), "{name}: {p} isn't drivable");
            }
        }
        assert!(Map::sanctum().skeleton.is_none());
    }
}
