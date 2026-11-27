//! The minimap, top right in battle (M: the whole map, big): the map's layout (`Map::skeleton`), so
//! only generated maps have one. The land shaded by height, then streets,
//! washes, buildings, digs and railroads; over it where each side spawns, your
//! side, what you're fighting over and you. Enemies aren't on it.

use std::sync::Arc;

use glam::Vec2;

use crate::combat::{Match, Phase};
use crate::faction::Faction;
use crate::map::Map;
use crate::map::procgen::skeleton::{LotKind, Rank, Skeleton};
use crate::sim::Tank;
use crate::ui::{Align, Painter, Rgba, design, emblem, measure, theme, with_alpha};

/// Its side, in logical px.
pub const SIZE: f32 = 190.0;
/// The whole map's side, of the screen's height.
pub const BIG: f32 = 0.7;
/// How many cells across its land's shaded in, small and big.
const CELLS: usize = 48;
const BIG_CELLS: usize = 128;

/// A map drawn once: polygons on the unit square, up the screen +z.
pub struct Chart {
    of: Arc<Skeleton>,
    /// The land, small and big.
    land: [Vec<Poly>; 2],
    /// What's on it.
    polys: Vec<Poly>,
}

type Poly = (Vec<Vec2>, Rgba);

/// A point of a map `size` across, on the unit square. (Seen from above
/// with +z up the screen, +x is to the left.)
fn unit(p: Vec2, size: f32) -> Vec2 {
    Vec2::ONE - p / size
}

/// `poly` (convex) cut to the unit square.
fn clipped(poly: &[Vec2]) -> Vec<Vec2> {
    let mut out = poly.to_vec();
    for (axis, at, sign) in [(0, 0.0, 1.0), (0, 1.0, -1.0), (1, 0.0, 1.0), (1, 1.0, -1.0)] {
        let inside = |p: Vec2| (p[axis] - at) * sign;
        let from = std::mem::take(&mut out);
        for (k, &a) in from.iter().enumerate() {
            let b = from[(k + 1) % from.len()];
            let (da, db) = (inside(a), inside(b));
            if da >= 0.0 {
                out.push(a);
            }
            if (da >= 0.0) != (db >= 0.0) {
                out.push(a.lerp(b, da / (da - db)));
            }
        }
    }
    out
}

impl Chart {
    /// `map`'s, if it was generated.
    pub fn of(map: &Map) -> Option<Chart> {
        let sk = map.skeleton.clone()?;
        let mut c = Chart {
            of: sk.clone(),
            land: [CELLS, BIG_CELLS].map(|n| land(map, &sk, n)),
            polys: Vec::new(),
        };
        let grey = |v: f32, a: f32| -> Rgba { [v, v * 0.98, v * 0.94, a] };
        for w in &sk.land.washes {
            c.line(&w.line, w.floor + 2.0, grey(0.02, 0.5));
        }
        for s in &sk.streets {
            let v = if s.rank == Rank::Avenue { 0.4 } else { 0.3 };
            if s.rank != Rank::Beyond {
                c.line(&[s.from, s.to], s.half, grey(v, 0.75));
            }
        }
        for l in &sk.lots {
            let shade = match l.kind {
                LotKind::Distant => continue,
                LotKind::Rubble
                | LotKind::Spires
                | LotKind::Fins
                | LotKind::Boulders
                | LotKind::Outcrop
                | LotKind::Cut => grey(0.16, 0.55),
                _ => grey(0.035, 0.95),
            };
            c.poly(&l.footprint.corners(), shade);
        }
        for l in &sk.landmarks {
            let n = 20;
            let ring: Vec<Vec2> = (0..n)
                .map(|k| Vec2::from_angle(k as f32 / n as f32 * std::f32::consts::TAU))
                .map(|d| l.center + d * l.radius)
                .collect();
            c.poly(&ring, grey(0.07, 0.8));
        }
        // Digs, seen through the rock over them.
        for d in &sk.land.digs {
            for r in &d.rooms {
                let q = [
                    r.min,
                    Vec2::new(r.max.x, r.min.y),
                    r.max,
                    Vec2::new(r.min.x, r.max.y),
                ];
                c.poly(&q.map(|p| d.to_map(p)), grey(0.02, 0.6));
            }
        }
        for t in &sk.tracks {
            c.line(t, 1.1, grey(0.012, 0.9));
        }
        for x in &sk.crossings {
            c.line(&[x.from, x.to], x.half, grey(0.5, 0.9));
        }
        Some(c)
    }

    fn poly(&mut self, pts: &[Vec2], color: Rgba) {
        let size = self.of.size;
        let on: Vec<Vec2> = pts.iter().map(|&p| unit(p, size)).collect();
        let cut = clipped(&on);
        if cut.len() >= 3 {
            self.polys.push((cut, color));
        }
    }

    fn line(&mut self, pts: &[Vec2], half: f32, color: Rgba) {
        for w in pts.windows(2) {
            let d = (w[1] - w[0]).normalize_or_zero();
            let side = d.perp() * half;
            // (A little over its ends, so bends don't gape.)
            let (a, b) = (w[0] - d * half * 0.3, w[1] + d * half * 0.3);
            self.poly(&[a - side, b - side, b + side, a + side], color);
        }
    }

    /// Whether it's `map`'s.
    fn is_of(&self, map: &Map) -> bool {
        map.skeleton
            .as_ref()
            .is_some_and(|s| Arc::ptr_eq(s, &self.of))
    }
}

/// The land: lighter the higher, slopes lit from one corner.
fn land(map: &Map, sk: &Skeleton, cells: usize) -> Vec<Poly> {
    let mut out = Vec::new();
    let cell = sk.size / cells as f32;
    let at = |i: usize, j: usize| (Vec2::new(i as f32, j as f32) + 0.5) * cell;
    let h: Vec<f32> = (0..cells * cells)
        .map(|k| sk.ground(at(k % cells, k / cells)))
        .collect();
    let (lo, hi) = h
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    let span = (hi - lo).max(8.0);
    for j in 0..cells {
        for i in 0..cells {
            let p = at(i, j);
            let g = h[j * cells + i];
            if map.is_out(glam::Vec3::new(p.x, g, p.y)) {
                continue;
            }
            let next =
                |di: usize, dj: usize| h[(j + dj).min(cells - 1) * cells + (i + di).min(cells - 1)];
            let slope = (next(1, 0) - g + next(0, 1) - g) / cell;
            let v = 0.08 + 0.16 * (g - lo) / span + (slope * 0.06).clamp(-0.06, 0.06);
            let wet = sk
                .land
                .channel
                .as_ref()
                .is_some_and(|c| c.local(p).1.abs() < c.half);
            let color = if wet {
                [0.5, 0.12, 0.42, 0.9]
            } else {
                [v * 1.04, v, v * 0.9, 0.9]
            };
            let a = Vec2::new(i as f32, j as f32) * cell;
            let quad = [
                a,
                a + Vec2::new(cell, 0.0),
                a + Vec2::splat(cell),
                a + Vec2::new(0.0, cell),
            ];
            out.push((quad.map(|p| unit(p, sk.size)).to_vec(), color));
        }
    }
    out
}

/// Keeps `chart` to the match's map.
pub fn keep(chart: &mut Option<Chart>, map: &Map) {
    let stale = match chart {
        Some(c) => !c.is_of(map),
        None => map.skeleton.is_some(),
    };
    if stale {
        *chart = Chart::of(map);
    }
}

/// Draws it `side` px across, its top left corner at `min`: big, with a
/// compass.
pub fn draw(p: &mut Painter, chart: &Chart, m: &Match, tanks: &[Tank], min: Vec2, side: f32) {
    let big = side > SIZE * 1.5;
    let pad = Vec2::splat(design::UNIT * if big { 1.5 } else { 0.5 });
    let plate = with_alpha(design::GROUND, if big { 0.94 } else { 0.88 });
    p.rect(min - pad, Vec2::splat(side) + pad * 2.0, plate);
    for (poly, color) in chart.land[big as usize].iter().chain(&chart.polys) {
        let pts: Vec<Vec2> = poly.iter().map(|&q| min + q * side).collect();
        p.poly(&pts, *color);
    }
    // Marks, bigger on the big one.
    let k = if big { 2.0 } else { 1.0 };
    let size = chart.of.size;
    let on = |q: Vec2| min + unit(q, size).clamp(Vec2::ZERO, Vec2::ONE) * side;
    let flat = |v: glam::Vec3| Vec2::new(v.x, v.z);
    let me = m.player();
    let ring = |p: &mut Painter, at: Vec2, r: f32, color: Rgba| {
        p.arc(at, r * k, 0.0, std::f32::consts::TAU, 1.6, color);
    };
    let stands: Vec<Vec2> = m
        .raid
        .iter()
        .flat_map(|r| r.stands.clone())
        .map(|s| on(flat(s)))
        .collect();
    for (team, spots) in m.world.map.bases.iter().enumerate() {
        if let Some(area) = spawn_area(spots) {
            let area = area.map(|q| on(q));
            let yours = team == me.team;
            spawn(p, m.factions[team], yours, area, &stands, (min, side), big);
        }
    }
    if let Some(h) = &m.hold {
        for s in &h.sites {
            ring(p, on(s.at), 6.0, theme::FG);
        }
    }
    if let Some(r) = m.raid.as_ref().filter(|r| !r.done()) {
        ring(p, on(flat(r.capture_point())), 5.0, theme::DIM);
        let at = on(flat(r.flag_at(tanks)));
        let color = m.factions[r.defenders()].ui();
        p.poly(
            &[
                at + Vec2::new(0.0, -5.0) * k,
                at + Vec2::new(4.0, 0.0) * k,
                at + Vec2::new(0.0, 5.0) * k,
                at + Vec2::new(-4.0, 0.0) * k,
            ],
            color,
        );
    }
    for (t, f) in tanks.iter().zip(&m.fighters).skip(1) {
        if f.alive && f.team == me.team {
            let color = m.factions[f.team].ui();
            let half = Vec2::splat(2.2 * k);
            p.rect(on(flat(t.pos)) - half, half * 2.0, color);
        }
    }
    if me.alive && m.phase == Phase::Playing {
        let t = &tanks[0];
        let (s, c) = t.yaw().sin_cos();
        let fwd = -Vec2::new(s, c);
        let at = on(flat(t.pos));
        let wing = fwd.perp() * 4.2 * k;
        p.poly(
            &[
                at + fwd * 7.5 * k,
                at - fwd * 4.5 * k + wing,
                at - fwd * 4.5 * k - wing,
            ],
            theme::HOT,
        );
    }
    if big {
        compass(p, min + Vec2::new(side - 44.0, 52.0));
    }
}

/// The corners of the box round a side's spawn `spots`, a tank's room over.
fn spawn_area(spots: &[Vec2]) -> Option<[Vec2; 2]> {
    let first = *spots.first()?;
    let (lo, hi) = spots
        .iter()
        .fold((first, first), |(a, b), &q| (a.min(q), b.max(q)));
    Some([lo - Vec2::splat(5.0), hi + Vec2::splat(5.0)])
}

/// Where side `f` spawns: `area` (opposite corners, on screen) in its colour
/// and its emblem on a badge, rimmed white if it's yours. The badge steps off
/// the raid `stands` so it hides no flag, and stays on the chart (`min`, its
/// side); the big chart's says what it is.
fn spawn(
    p: &mut Painter,
    f: Faction,
    yours: bool,
    area: [Vec2; 2],
    stands: &[Vec2],
    (min, side): (Vec2, f32),
    big: bool,
) {
    let color = f.ui();
    let (lo, hi) = (area[0].min(area[1]), area[0].max(area[1]));
    p.rect(lo, hi - lo, with_alpha(color, 0.3));
    p.rect_outline(lo, hi - lo, if big { 2.0 } else { 1.2 }, color);
    let r = if big { 17.0 } else { 10.0 };
    let gap = r + if big { 12.0 } else { 7.0 };
    let mut at = (lo + hi) * 0.5;
    for &s in stands {
        if at.distance(s) < gap {
            // Toward the middle of the chart if it's dead on.
            let middle = min + Vec2::splat(side * 0.5);
            let off = (at - s).try_normalize();
            at = s + off.unwrap_or((middle - s).normalize_or(Vec2::Y)) * gap;
        }
    }
    let at = at.clamp(min + Vec2::splat(r), min + Vec2::splat(side - r));
    let rim = if yours { theme::FG } else { color };
    p.disc(at, r, rim, rim);
    p.disc(
        at,
        r - if big { 2.5 } else { 1.6 },
        design::GROUND,
        design::GROUND,
    );
    emblem::emblem(p, f, at, r * 1.2, if big { 2.0 } else { 1.3 }, color);
    if big {
        let text = if yours { "YOUR SPAWN" } else { "ENEMY SPAWN" };
        let style = design::SMALL.colored(rim);
        let w = measure(text, &style) * 0.5 + 8.0;
        let x = at.x.clamp(min.x + w, min.x + side - w);
        let below = hi.y.max(at.y + r) + 4.0;
        let h = style.size + 8.0;
        let y = if below + h > min.y + side {
            lo.y.min(at.y - r) - 4.0 - h
        } else {
            below
        };
        let plate = with_alpha(design::GROUND, 0.8);
        p.rect(Vec2::new(x - w, y), Vec2::new(w * 2.0, h), plate);
        p.text(Vec2::new(x, y + 4.0), text, &style, Align::Center);
    }
}

/// An arrow north (+z, up the map) with its N.
fn compass(p: &mut Painter, at: Vec2) {
    let (up, across) = (Vec2::NEG_Y, Vec2::X);
    p.disc(
        at,
        30.0,
        with_alpha(design::GROUND, 0.8),
        with_alpha(design::GROUND, 0.8),
    );
    let tip = at + up * 14.0;
    let tail = at - up * 18.0;
    let waist = at - up * 2.0;
    p.poly(
        &[tip, waist + across * 6.0, waist - across * 6.0],
        theme::HOT,
    );
    p.poly(
        &[tail, waist - across * 6.0, waist + across * 6.0],
        theme::DIM,
    );
    let mut n = design::BODY;
    n.color = theme::FG;
    p.text(tip + up * 13.0, "N", &n, Align::Center);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::procgen::Look;

    /// Generated maps have one, all of it on the square; built by hand,
    /// none.
    #[test]
    fn generated_maps_have_a_minimap() {
        for spec in [
            crate::city_spec(1, Look::Imperium),
            crate::mine_spec(
                1,
                Look::Ithessar,
                crate::map::procgen::foundry::Ground::Desert,
            ),
            crate::desert_spec(1),
        ] {
            let m = crate::map::procgen::map(&spec);
            let c = Chart::of(&m).expect("a minimap");
            assert!(c.land.iter().all(|l| l.len() > 1000) && !c.polys.is_empty());
            let all = c.land.iter().flatten().chain(&c.polys);
            for p in all.flat_map(|p| &p.0) {
                assert!(
                    p.min_element() > -1e-4 && p.max_element() < 1.0 + 1e-4,
                    "{p}"
                );
            }
        }
        assert!(Chart::of(&Map::sanctum()).is_none());
    }

    /// Every spawn's inside its side's marked area, and the areas don't overlap.
    #[test]
    fn spawns_are_marked() {
        assert!(spawn_area(&[]).is_none());
        let m = crate::map::procgen::map(&crate::city_spec(1, Look::Imperium));
        let areas: Vec<[Vec2; 2]> = m.bases.iter().map(|b| spawn_area(b).unwrap()).collect();
        for (b, [lo, hi]) in m.bases.iter().zip(&areas) {
            assert!(b.iter().all(|q| q.cmpge(*lo).all() && q.cmple(*hi).all()));
        }
        let ([alo, ahi], [blo, bhi]) = (areas[0], areas[1]);
        assert!(ahi.cmplt(blo).any() || bhi.cmplt(alo).any());
    }

    /// Ahead of a tank facing +z is up, and its left is left.
    #[test]
    fn it_isnt_mirrored() {
        let at = unit(Vec2::splat(50.0), 100.0);
        assert!(unit(Vec2::new(50.0, 60.0), 100.0).y < at.y);
        let right = glam::Vec3::Z.cross(glam::Vec3::Y);
        let r = unit(
            Vec2::new(50.0, 50.0) + Vec2::new(right.x, right.z) * 10.0,
            100.0,
        );
        assert!(r.x > at.x);
    }
}
