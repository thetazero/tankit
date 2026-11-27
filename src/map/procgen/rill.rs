//! Side channels off a plasma river, feeding the houses: shallow runnels set
//! in the paving. Each is drawn up the river's bank, crosses the quay and
//! goes up one side of a street, a spur off it into each house it passes;
//! houses on the quay have theirs straight from the river. They're only for
//! show, too shallow to burn anything: tanks drive over them.

use glam::{Vec2, Vec3};

use super::channel::{BANK, Channel, LINING};
use super::skeleton::*;
use crate::map::Map;
use crate::render::material::Material;
use crate::render::mesh::MeshData;

/// Half a main's width, and a spur's, and how wide their kerbs are.
const MAIN: f32 = 0.5;
const SPUR: f32 = 0.28;
const KERB: f32 = 0.2;
/// How far over the ground the kerbs and the plasma are.
const TOP: f32 = 0.17;
const LEVEL: f32 = 0.1;
/// The furthest house a spur goes to.
const REACH: f32 = 11.0;
/// Intakes are at least this far apart along the river.
const APART: f32 = 9.0;
/// How far an intake's middle is off a lamp post's, at least.
const LAMP: f32 = 1.7;
/// How long a main's worth laying.
const LEAST: f32 = 30.0;

#[derive(Clone, Debug)]
pub struct Rill {
    /// Down its middle, about a meter apart.
    pub points: Vec<Vec2>,
    pub half: f32,
    /// Where along the river it's drawn from, and from which bank (a spur's
    /// off another).
    pub intake: Option<(f32, f32)>,
    /// It ends in a house.
    #[cfg_attr(not(test), allow(dead_code))]
    pub feeds: bool,
}

impl Rill {
    fn distance(&self, p: Vec2) -> f32 {
        self.points
            .windows(2)
            .map(|w| to_segment(p, w[0], w[1]))
            .fold(f32::MAX, f32::min)
    }
}

fn to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.length_squared().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + d * t)
}

/// `line` in pieces about a meter long.
fn fine(line: &[Vec2]) -> Vec<Vec2> {
    let mut pts = vec![line[0]];
    for w in line.windows(2) {
        let n = w[0].distance(w[1]).ceil().max(1.0) as i32;
        pts.extend((1..=n).map(|k| w[0].lerp(w[1], k as f32 / n as f32)));
    }
    pts
}

fn house(l: &Lot) -> bool {
    matches!(l.kind, LotKind::Big | LotKind::Small)
}

/// Can one run through `p`? (`into`: the house it's going to.)
fn open(sk: &Skeleton, c: &Channel, p: Vec2, into: Option<usize>) -> bool {
    sk.out(p) < 2.0
        && sk.off_plazas(p) > 0.5
        && sk.bases.iter().all(|b| !b.contains(p, 2.0))
        && c.bridges
            .iter()
            .all(|b| to_segment(p, b.ends[0], b.ends[1]) > b.half + 1.5)
        && sk
            .lots
            .iter()
            .enumerate()
            .all(|(k, l)| Some(k) == into || l.footprint.distance(p) > 0.8)
        && sk
            .decos
            .iter()
            .all(|d| d.at.distance(p) > d.kind.reach() * 0.75 + 0.8)
        && sk.repair_spots.iter().all(|r| r.distance(p) > 4.0)
        && sk
            .spans
            .iter()
            .all(|s| (s.at.distance(p) - s.opening).abs() > 2.0)
        && sk
            .conduits
            .iter()
            .all(|o| o.legs().iter().all(|l| l.distance(p) > 1.8))
}

/// Keeps clear of every one laid already.
fn apart(sk: &Skeleton, p: Vec2) -> bool {
    sk.rills.iter().all(|r| r.distance(p) > r.half + 1.4)
}

/// Is there room for an intake `t` along, on the `side` bank?
fn room(sk: &Skeleton, t: f32, side: f32) -> bool {
    sk.rills
        .iter()
        .filter_map(|r| r.intake)
        .all(|(o, s)| s != side || (o - t).abs() > APART)
        && sk.conduits.iter().all(|o| (o.t - t).abs() > 4.0)
}

/// Lays them all out, the city already whole.
pub fn plan(sk: &mut Skeleton) {
    let Some(c) = sk.land.channel.clone() else {
        return;
    };
    mains(sk, &c);
    quayside(sk, &c);
}

/// How far off the river's middle one starts: just in the plasma.
fn source(c: &Channel) -> f32 {
    c.half - 0.6
}

/// Streets off the quays, each as one line away from the river (and half
/// its width, and whether it's an avenue).
fn streets(sk: &Skeleton, c: &Channel) -> Vec<(Vec<Vec2>, f32, bool)> {
    let on_quay = |p: Vec2| c.local(p).1.abs() < c.half + BANK + 13.0;
    let mut lines: Vec<(Vec<Vec2>, Street)> = Vec::new();
    for s in &sk.streets {
        if !matches!(s.rank, Rank::Lane | Rank::Avenue) {
            continue;
        }
        match lines.last_mut() {
            Some((l, o)) if l.last() == Some(&s.from) && o.half == s.half && o.rank == s.rank => {
                l.push(s.to);
            }
            _ => lines.push((vec![s.from, s.to], *s)),
        }
    }
    let mut out = Vec::new();
    for (mut line, s) in lines {
        // (Not the quays' own roads.)
        if line.iter().all(|&p| on_quay(p)) {
            continue;
        }
        for _ in 0..2 {
            if on_quay(line[0]) {
                out.push((line.clone(), s.half, s.rank == Rank::Avenue));
            }
            line.reverse();
        }
    }
    out
}

fn mains(sk: &mut Skeleton, c: &Channel) {
    for (line, half, avenue) in streets(sk, c) {
        let line = fine(&line);
        // Down both sides of an avenue, one of anything else.
        for e in [1.0f32, -1.0] {
            let aside = e * (half - if avenue { 2.2 } else { 1.0 });
            let n = line.len();
            let run: Vec<Vec2> = (0..n)
                .map(|k| {
                    let d = line[(k + 2).min(n - 1)] - line[k.saturating_sub(2)];
                    line[k] + d.normalize_or(Vec2::X).perp() * aside
                })
                // Off the quay's road.
                .skip_while(|&p| c.local(p).1.abs() < c.half + BANK + 11.0)
                .collect();
            let Some(&first) = run.first() else {
                continue;
            };
            let (t, off) = c.local(first);
            let (t, side) = (c.off_lamps(t, LAMP), off.signum());
            if off.abs() > c.half + BANK + 18.0 || !room(sk, t, side) {
                continue;
            }
            let mut pts = vec![c.point(t, side * source(c))];
            pts.extend(run);
            let mut pts = fine(&pts);
            let end = pts
                .iter()
                .position(|&p| !open(sk, c, p, None) || !apart(sk, p))
                .unwrap_or(pts.len());
            // (Back down the river it won't go.)
            let end = (4..end)
                .find(|&k| c.local(pts[k]).1.abs() < c.local(pts[k - 4]).1.abs() - 2.0)
                .unwrap_or(end);
            pts.truncate(end);
            if (pts.len() as f32) < LEAST {
                continue;
            }
            sk.rills.push(Rill {
                points: pts.clone(),
                half: MAIN,
                intake: Some((t, side)),
                feeds: false,
            });
            let before = sk.rills.len();
            spurs(sk, c, &pts);
            if !avenue && sk.rills.len() == before {
                // It fed nothing.
                sk.rills.pop();
                continue;
            }
            if !avenue {
                // No further than the last house it feeds.
                let main = &sk.rills[before - 1].points;
                let last = sk.rills[before..]
                    .iter()
                    .filter_map(|r| main.iter().position(|p| *p == r.points[0]))
                    .max()
                    .unwrap_or(main.len() - 1);
                sk.rills[before - 1].points.truncate(last + 1);
                break;
            }
        }
    }
}

/// From where a house's fed, in through its wall to its middle.
fn into(l: &Lot, from: Vec2) -> Vec<Vec2> {
    fine(&[from, l.footprint.center])
}

fn spurs(sk: &mut Skeleton, c: &Channel, main: &[Vec2]) {
    for k in 0..sk.lots.len() {
        let l = sk.lots[k];
        if !house(&l) || fed(sk, &l) {
            continue;
        }
        // (None off it on the bank.)
        let Some(&from) = main.iter().filter(|&&p| !c.wet(p, 2.0)).min_by(|a, b| {
            l.footprint
                .distance(**a)
                .total_cmp(&l.footprint.distance(**b))
        }) else {
            continue;
        };
        if l.footprint.distance(from) > REACH {
            continue;
        }
        let pts = into(&l, from);
        let own = sk.rills.iter().rposition(|r| r.intake.is_some());
        let clear = pts.iter().skip(2).all(|&p| {
            open(sk, c, p, Some(k))
                && sk
                    .rills
                    .iter()
                    .enumerate()
                    .all(|(i, r)| Some(i) == own || r.distance(p) > r.half + 1.4)
        });
        // Away from its main, never along it.
        let off = pts.len() < 4
            || main
                .iter()
                .all(|m| m.distance(pts[3]) > MAIN + SPUR + KERB * 2.0 + 0.3);
        if clear && off {
            sk.rills.push(Rill {
                points: pts,
                half: SPUR,
                intake: None,
                feeds: true,
            });
        }
    }
}

fn fed(sk: &Skeleton, l: &Lot) -> bool {
    sk.rills
        .iter()
        .any(|r| r.feeds && r.points.last() == Some(&l.footprint.center))
}

/// Houses on the quays.
fn quayside(sk: &mut Skeleton, c: &Channel) {
    for k in 0..sk.lots.len() {
        let l = sk.lots[k];
        let (t, off) = c.local(l.footprint.center);
        let (t, side) = (c.off_lamps(t, LAMP), off.signum());
        if !house(&l) || fed(sk, &l) || !room(sk, t, side) {
            continue;
        }
        let from = c.point(t, side * source(c));
        if l.footprint.distance(from) > BANK + 18.0 {
            continue;
        }
        let pts = into(&l, from);
        if pts.iter().all(|&p| open(sk, c, p, Some(k)) && apart(sk, p)) {
            sk.rills.push(Rill {
                points: pts,
                half: SPUR,
                intake: Some((t, side)),
                feeds: true,
            });
        }
    }
}

/// Builds them.
pub fn build(map: &mut Map, sk: &Skeleton) {
    let Some(c) = &sk.land.channel else {
        return;
    };
    // (The banks are lined.)
    let ground = |p: Vec2| sk.ground(p) + if c.wet(p, 1.0) { LINING } else { 0.0 };
    let mut m = MeshData::default();
    for (i, r) in sk.rills.iter().enumerate() {
        let near: Vec<&Rill> = sk
            .rills
            .iter()
            .enumerate()
            .filter(|&(k, o)| {
                k != i
                    && (r.points.first().is_some_and(|&p| o.distance(p) < 2.0)
                        || o.points.first().is_some_and(|&p| r.distance(p) < 2.0))
            })
            .map(|(_, o)| o)
            .collect();
        kerbs(&mut m, r, &near, &ground);
        plasma(&mut m, r, &ground);
    }
    let base = map.sculpted.verts.len() as u32;
    map.sculpted.verts.extend(m.verts);
    map.sculpted
        .indices
        .extend(m.indices.iter().map(|i| i + base));
}

/// Which way's across it at point `k`.
fn across(r: &Rill, k: usize) -> Vec2 {
    let n = r.points.len();
    (r.points[(k + 1).min(n - 1)] - r.points[k.saturating_sub(1)])
        .normalize_or(Vec2::X)
        .perp()
}

fn kerbs(m: &mut MeshData, r: &Rill, near: &[&Rill], ground: &impl Fn(Vec2) -> f32) {
    let glass = Vec3::new(0.09, 0.11, 0.18).extend(Material::Vhaal.id());
    let at = |p: Vec2, y: f32| Vec3::new(p.x, y, p.y);
    for (k, w) in r.points.windows(2).enumerate() {
        let sides = [across(r, k), across(r, k + 1)];
        let h = [ground(w[0]), ground(w[1])];
        for e in [-1.0f32, 1.0] {
            let edge = |off: f32, u: f32| {
                let a = w[0] + sides[0] * e * off;
                a.lerp(w[1] + sides[1] * e * off, u)
            };
            let (inner, outer) = (r.half, r.half + KERB);
            // In pieces where another meets it, so there's a way through.
            let meets = near
                .iter()
                .any(|o| o.distance(edge(inner, 0.5)) < o.half + 2.0);
            let n = if meets { 6 } else { 1 };
            let mut kept = Vec::new();
            for j in 0..n {
                let u = [j as f32 / n as f32, (j + 1) as f32 / n as f32];
                let mid = edge((inner + outer) * 0.5, (u[0] + u[1]) * 0.5);
                if near.iter().all(|o| o.distance(mid) > o.half) {
                    kept.push(u);
                }
            }
            for u in kept {
                let y = u.map(|u| h[0] + (h[1] - h[0]) * u);
                let [a0, b0] = [edge(inner, u[0]), edge(outer, u[0])];
                let [a1, b1] = [edge(inner, u[1]), edge(outer, u[1])];
                let out = (b0 - a0).normalize_or(Vec2::X);
                let out = Vec3::new(out.x, 0.0, out.y);
                let mut quad = |p: [Vec3; 4], n: Vec3| {
                    let facing = (p[1] - p[0]).cross(p[2] - p[0]).dot(n) > 0.0;
                    let p = if facing { p } else { [p[0], p[3], p[2], p[1]] };
                    m.quad(p, n, glass);
                };
                let (top, foot) = (TOP, -0.3);
                quad(
                    [
                        at(a0, y[0] + top),
                        at(a1, y[1] + top),
                        at(b1, y[1] + top),
                        at(b0, y[0] + top),
                    ],
                    Vec3::Y,
                );
                for (p, q, n) in [(a0, a1, -out), (b0, b1, out)] {
                    quad(
                        [
                            at(p, y[0] + foot),
                            at(q, y[1] + foot),
                            at(q, y[1] + top),
                            at(p, y[0] + top),
                        ],
                        n,
                    );
                }
                let d = (a1 - a0).normalize_or(Vec2::X);
                let d = Vec3::new(d.x, 0.0, d.y);
                for (p, q, y, n) in [(a0, b0, y[0], -d), (a1, b1, y[1], d)] {
                    // (Only where it's cut: elsewhere the next goes on.)
                    if meets {
                        quad(
                            [
                                at(p, y + foot),
                                at(q, y + foot),
                                at(q, y + top),
                                at(p, y + top),
                            ],
                            n,
                        );
                    }
                }
            }
        }
    }
}

fn plasma(m: &mut MeshData, r: &Rill, ground: &impl Fn(Vec2) -> f32) {
    // A spur's starts at its main's edge.
    let spur = r.intake.is_none();
    let mut down = 0.0;
    let mut last: Option<[u32; 3]> = None;
    for (k, &p) in r.points.iter().enumerate() {
        let mut p = p;
        if spur && k == 0 {
            p = p.move_towards(r.points[1], MAIN - 0.02);
        }
        if k > 0 {
            down += p.distance(r.points[k - 1]);
        }
        let s = across(r, k) * (r.half + 0.05);
        // (A little lower, where it runs in under the main's.)
        let y = ground(p) + LEVEL - if spur { 0.01 } else { 0.0 };
        let row = [-1.0f32, 0.0, 1.0].map(|u| {
            let q = p + s * u;
            let c = Vec3::new(down / 100.0, u * 0.5 + 0.5, r.half / 10.0);
            m.vert(
                Vec3::new(q.x, y, q.y),
                Vec3::Y,
                c.extend(Material::Plasma.id()),
            )
        });
        if let Some(l) = last {
            for j in 0..2 {
                let (a, b, c, d) = (l[j], l[j + 1], row[j + 1], row[j]);
                m.indices.extend_from_slice(&[a, c, b, a, d, c]);
            }
        }
        last = Some(row);
    }
}
