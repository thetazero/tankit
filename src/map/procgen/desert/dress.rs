//! Dressing a desert's skeleton: the ground painted, the cliffs sculpted,
//! each placeholder filled from the library of natural pieces (spires
//! stacked from the rocks toolkit, fins, boulder fields, the landmarks: a
//! ring of needles, an arch, a crater's rim), then cover wherever the open
//! ground's wide, the edge walled with boulders, the horizon. And the
//! gameplay: spawns, the places bots head for, repair drops, the menu
//! vistas.

use std::f32::consts::TAU;

use glam::{Vec2, Vec3};

use super::Palette;
use super::skeleton::*;
use crate::ai::nav::NavGrid;
use crate::faction::Faction;
use crate::map::noise::noise2;
use crate::map::procgen::Centerpiece;
use crate::map::procgen::mine::{line_at, smoothstep};
use crate::map::ruins::Drift;
use crate::map::{Map, OBox, Terrain, rocks};
use crate::render::material::Material;
use crate::rng::Rng;

mod camp;
mod town;
#[cfg(test)]
pub use camp::site;

/// The stone the rock faces are sculpted in (see `crag`), for tinting
/// them to a desert's own.
const RED_SANDSTONE: Vec3 = Vec3::new(0.58, 0.34, 0.22);

/// Clearance a spawn needs from anything solid.
const TANK_CLEAR: f32 = 4.6;
/// The most ways round a desert lays out for the bots (see `Sand::routes`).
const MAX_ROUTES: usize = 6;

/// What a filler built for.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum Claim {
    Landmark,
    Lot(usize),
}

/// The play ground, painted: crests pale, troughs and the washes darker,
/// the high ground's top weathered rock with sand lying on it.
pub fn terrain(sk: &Skeleton, pal: Palette) -> Terrain {
    terrain_of(sk, pal, None, &[])
}

/// `terrain`, with a generator's camp (`camp`) on it if there's one.
fn terrain_of(sk: &Skeleton, pal: Palette, camp: Option<&camp::Camp>, drifts: &[Drift]) -> Terrain {
    let n = (sk.size / 2.0).round() as usize + 1;
    let mut t = Terrain::from_fn(Vec2::ZERO, 2.0, n, n, |x, z| {
        sk.ground(Vec2::new(x, z))
            + camp.map_or(0.0, |c| c.heap(Vec2::new(x, z)))
            + drifts
                .iter()
                .map(|d| d.height_at(Vec2::new(x, z)))
                .fold(0.0, f32::max)
    });
    t.color = Vec3::ONE;
    t.material = Material::Sand;
    let high = sk.land.high.height;
    t.paint(|x, z, h, slope| {
        let p = Vec2::new(x, z);
        let mottle =
            0.93 + 0.1 * noise2(x / 7.0, z / 7.0, 90) + 0.04 * noise2(x / 1.7, z / 1.7, 91);
        let rel = if h > high * 0.6 { h - high } else { h };
        let mut c = pal
            .low
            .lerp(pal.sand, smoothstep(-0.3, sk.dunes().height * 0.8, rel));
        let cut = sk.land.wash_cut(p) / WASH_DEPTH;
        c = c.lerp(
            pal.low * 0.85 + pal.stone_dark * 0.15,
            smoothstep(0.5, 0.95, cut),
        );
        // The high ground: rock showing through the sand.
        let top =
            smoothstep(high * 0.6, high * 0.95, h) * (0.45 + 0.4 * noise2(x / 9.0, z / 9.0, 92));
        let rock = crate::map::crag::TOP.lerp(pal.stone, 0.6);
        c = c.lerp(rock, top);
        c = c.lerp(pal.stone_dark, smoothstep(0.45, 0.9, slope) * 0.6);
        c = town::paint(sk, p, c, &pal);
        if let Some(camp) = camp {
            c = camp.paint(p, c, &pal);
        }
        c * mottle
    });
    t
}

/// Dresses `map` (its ground already laid) to `sk`. Returns what each
/// placeholder's filler claimed of `map.oboxes`.
pub fn dress(
    map: &mut Map,
    sk: &Skeleton,
    pal: Palette,
    seed: u64,
    piece: Centerpiece,
) -> Vec<(Claim, std::ops::Range<usize>)> {
    let dice = |k: u64| Rng::new(seed.wrapping_mul(0x2545_F491).wrapping_add(k));
    let mut s = Sand { map, sk, pal };
    let mut claims = Vec::new();
    s.map.terrain = Some(terrain(sk, pal));
    if sk.houses().next().is_some() {
        let drifts = s.houses(&mut dice(4));
        s.map.terrain = Some(terrain_of(sk, pal, None, &drifts));
        s.map.settle();
    }
    s.cliffs(&mut dice(1));
    let start = s.map.oboxes.len();
    match sk.landmarks[0].kind {
        LandmarkKind::Needles => s.needles(&mut dice(2)),
        LandmarkKind::Crater => s.crater(&mut dice(2)),
        LandmarkKind::Arch => s.arch(&mut dice(2)),
        LandmarkKind::Dam => s.dam(&mut dice(2)),
        LandmarkKind::Well => s.well(&mut dice(2)),
        LandmarkKind::Generator | LandmarkKind::Great(_) => {
            let (owner, condition) = match piece {
                Centerpiece::ShieldGenerator(owner, condition) => (owner, condition),
                Centerpiece::Monument => (Faction::Imperium, Default::default()),
            };
            s.generator(&mut dice(2), seed, owner, condition)
        }
    }
    claims.push((Claim::Landmark, start..s.map.oboxes.len()));
    let mut rng = dice(3);
    for (k, lot) in sk.lots.iter().enumerate() {
        let start = s.map.oboxes.len();
        match lot.kind {
            LotKind::Spires => s.spires(&mut rng, lot),
            LotKind::Fins => s.fins(&mut rng, lot),
            LotKind::Boulders => s.boulder_field(&mut rng, lot),
            // (Houses are up already.)
            _ => {}
        }
        claims.push((Claim::Lot(k), start..s.map.oboxes.len()));
    }
    s.town(&mut dice(10));
    s.wash_stones(&mut dice(5));
    s.scatter(&mut dice(6));
    s.fill_cover(&mut dice(7));
    s.edge_rocks(&mut dice(8));
    s.spawns();
    s.horizon(&mut dice(9));
    s.map.vistas = s.vistas();
    claims
}

/// A desert being dressed: the map, the skeleton it's dressed to.
struct Sand<'a> {
    map: &'a mut Map,
    sk: &'a Skeleton,
    pal: Palette,
}

impl Sand<'_> {
    fn g(&self, p: Vec2) -> f32 {
        self.map.ground_height(p.x, p.y)
    }

    fn shade(&self, rng: &mut Rng) -> Vec3 {
        let p = self.pal;
        [p.stone, p.stone_dark, p.stone_pale][rng.index(3)] * rng.range(0.88, 1.06)
    }

    /// A boulder about `size` across at `at`.
    fn boulder(&mut self, rng: &mut Rng, at: Vec2, size: f32, solid: bool) {
        let g = self.g(at);
        let yaw = rng.range(0.0, TAU);
        let half = Vec2::new(size * rng.range(0.8, 1.1), size * rng.range(0.6, 0.9));
        let top = g + size * rng.range(0.9, 1.4);
        let color = self.shade(rng);
        self.map.oboxes.push(OBox {
            solid,
            ..rocks::boulder(at, yaw, half, g - size * 0.4, top, color)
        });
    }

    /// Room for a rock of half-width `s` at `p` with `gap` of clear ground
    /// round it (`except`: a piece it may lean on): in the arena, off
    /// what's kept clear and the cliffs' feet, on fairly level ground,
    /// that far from anything else.
    fn room(&self, p: Vec2, s: f32, gap: f32, except: Option<usize>) -> bool {
        let r = s * 1.1;
        let size = self.sk.size;
        if p.min_element() < 5.0 || p.max_element() > size - 5.0 {
            return false;
        }
        let cliff = self.sk.land.plateau(p);
        if cliff > -4.0 - r && cliff < FOOT + 7.0 + r {
            return false;
        }
        if (0..9).any(|k| {
            let q = if k == 0 {
                p
            } else {
                p + Vec2::from_angle(k as f32 * TAU / 8.0) * (r + 2.0)
            };
            self.sk.keep_clear(q)
        }) {
            return false;
        }
        let g = self.g(p);
        let flat = (0..8).all(|k| {
            let q = p + Vec2::from_angle(k as f32 * TAU / 8.0) * (r + 1.5);
            (self.g(q) - g).abs() < 1.2
        });
        if !flat {
            return false;
        }
        let near = r + gap;
        !self.map.oboxes.iter().enumerate().any(|(i, o)| {
            if Some(i) == except {
                return false;
            }
            let (lo, hi) = o.y_range();
            hi > g - 1.0 && lo < g + 6.0 && o.distance(p) < near
        })
    }

    /// The cliffs: sculpted sandstone along every foot line, blocks fallen
    /// off them lying out on the floor.
    fn cliffs(&mut self, rng: &mut Rng) {
        let sk = self.sk;
        let lines = sk.land.cliffs(sk.size);
        let from = self.map.sculpted.verts.len();
        for (k, line) in lines.iter().enumerate() {
            self.map
                .rock_face(line, sk.land.high.height, false, k as f32 * 977.0);
        }
        // The rock faces are sculpted in red sandstone: turned to this
        // desert's stone.
        let tint = self.pal.stone / RED_SANDSTONE;
        for v in &mut self.map.sculpted.verts[from..] {
            let c = Vec3::from_slice(&v.color[..3]) * tint;
            v.color[..3].copy_from_slice(&c.to_array());
        }
        for line in &lines {
            let mut walked = 0.0;
            for w in line.windows(2) {
                walked += w[0].distance(w[1]);
                if walked < 9.0 {
                    continue;
                }
                walked = 0.0;
                if !rng.chance(0.45) {
                    continue;
                }
                let dir = (w[1] - w[0]).normalize_or(Vec2::X);
                let out = Vec2::new(-dir.y, dir.x);
                let s = rng.range(1.1, 2.4);
                let p = w[0] + out * (FOOT + 7.5 + s * 1.2 + rng.range(0.0, 4.0));
                if self.room(p, s, 1.5, None) {
                    self.boulder(rng, p, s, s > 1.2);
                }
            }
        }
    }

    /// A spire of stacked sandstone `h` high, `w` thick: a broad foot, a
    /// waist worn thin, a harder cap.
    fn spire(&mut self, rng: &mut Rng, p: Vec2, h: f32, w: f32) {
        let g = self.g(p);
        let lean = Vec2::new(rng.range(-0.2, 0.2), rng.range(-0.2, 0.2)) * (h / 10.0);
        let pal = self.pal;
        // (fraction up from, to, girth, lean, colour)
        let parts = [
            (0.0, 0.34, 1.2, 0.0, pal.stone),
            (0.3, 0.62, 0.95, 0.4, pal.stone_pale),
            (0.58, 0.86, 0.75, 0.8, pal.stone),
            (0.82, 1.0, 0.95, 1.0, pal.stone_dark),
        ];
        for (k, &(a, b, girth, bend, color)) in parts.iter().enumerate() {
            let y0 = if k == 0 { g - 1.2 } else { g + h * a };
            let half = Vec2::new(w * girth, w * girth * rng.range(0.8, 1.0));
            let color = color * rng.range(0.9, 1.05);
            let yaw = rng.range(0.0, TAU);
            let mk = if k == 0 || k == 2 {
                rocks::block
            } else {
                rocks::boulder
            };
            self.map
                .oboxes
                .push(mk(p + lean * bend, yaw, half, y0, g + h * b, color));
        }
    }

    /// Small stones strewn over the landmark's open ground: nothing to stop
    /// a tank.
    fn strew(&mut self, rng: &mut Rng) {
        let lm = &self.sk.landmarks[0];
        for _ in 0..24 {
            let q = lm.center + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(2.0, lm.clear);
            let s = rng.range(0.25, 0.5);
            self.boulder(rng, q, s, false);
        }
    }

    /// The directions out of the landmark's ground toward each side's
    /// spawns, kept open.
    fn ways_in(&self) -> Vec<Vec2> {
        let lm = self.sk.landmarks[0].center;
        self.sk
            .base_spots()
            .iter()
            .map(|&b| (b - lm).normalize_or(Vec2::X))
            .collect()
    }

    /// The Needles: a ring of spires round the open ground, open toward
    /// each side's spawns; what's fallen off them.
    fn needles(&mut self, rng: &mut Rng) {
        let lm = &self.sk.landmarks[0];
        let ways = self.ways_in();
        let ring = lm.clear + rng.range(5.0, 8.0);
        let count = rng.int(7, 9);
        let b0 = rng.range(0.0, TAU);
        let mut spires = Vec::new();
        for j in 0..count {
            let dir = Vec2::from_angle(b0 + j as f32 * TAU / count as f32 + rng.range(-0.12, 0.12));
            if ways.iter().any(|&w| dir.dot(w) > 0.92) {
                continue;
            }
            let w = rng.range(2.2, 3.0);
            let p = lm.center + dir * (ring + rng.range(-1.5, 2.0));
            let h = rng.range(10.0, 18.0);
            self.spire(rng, p, h, w);
            spires.push((p, w));
        }
        for (p, w) in spires {
            for _ in 0..rng.int(1, 3) {
                let q = p + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(w + 3.0, w + 5.0);
                let s = rng.range(0.9, 1.6);
                if q.distance(lm.center) + s * 1.2 < lm.reach
                    && q.distance(lm.center) > lm.clear + s * 1.2
                {
                    self.boulder(rng, q, s, s > 1.2);
                }
            }
        }
        self.strew(rng);
    }

    /// An old crater: its rim (the ground's, see `Landmark`) heaped with
    /// broken blocks thrown out of it, gaps in it toward each side.
    fn crater(&mut self, rng: &mut Rng) {
        let lm = &self.sk.landmarks[0];
        let ways = self.ways_in();
        let rim = lm.clear + 9.0;
        let n = 34;
        for k in 0..n {
            let dir = Vec2::from_angle(k as f32 * TAU / n as f32 + rng.range(-0.05, 0.05));
            if ways.iter().any(|&w| dir.dot(w) > 0.94) || rng.chance(0.25) {
                continue;
            }
            let p = lm.center + dir * (rim + rng.range(-1.5, 2.5));
            let s = rng.range(1.4, 2.6);
            let g = self.g(p);
            let color = self.shade(rng);
            let half = Vec2::new(s, s * rng.range(0.6, 0.9));
            let mut o = rocks::block(
                p,
                rng.range(0.0, TAU),
                half,
                g - 0.8,
                g + s * rng.range(0.9, 1.5),
                color,
            );
            o.tilt = rng.range(-0.25, 0.25);
            o.broken = true;
            self.map.oboxes.push(o);
        }
        // Ejecta: blocks flung out past the rim.
        for _ in 0..12 {
            let dir = Vec2::from_angle(rng.range(0.0, TAU));
            let p = lm.center + dir * rng.range(rim + 4.0, lm.reach - 2.5);
            let s = rng.range(0.8, 1.8);
            if p.distance(lm.center) + s * 1.2 < lm.reach {
                self.boulder(rng, p, s, s > 1.1);
            }
        }
        self.strew(rng);
    }

    /// A great arch: two legs either side of the open ground, the span
    /// over it high enough for a tank to pass under, rubble round the feet.
    fn arch(&mut self, rng: &mut Rng) {
        let lm = &self.sk.landmarks[0];
        let across = lm.facing;
        let half = lm.clear + 3.0;
        let pal = self.pal;
        let legs = [lm.center - across * half, lm.center + across * half];
        let foot = legs.map(|p| self.g(p));
        let spring = 11.0 + rng.range(0.0, 2.0);
        for (k, &p) in legs.iter().enumerate() {
            let g = foot[k];
            // The leg: blocks stacked, wider at the foot.
            let mut y = g - 1.2;
            let mut w = 3.6;
            while y < g + spring {
                let top = (y + rng.range(2.6, 3.6)).min(g + spring + 0.8);
                let color = [pal.stone, pal.stone_pale][rng.index(2)] * rng.range(0.9, 1.05);
                let half = Vec2::new(w, w * rng.range(0.75, 0.9));
                let at = p + across * rng.range(-0.2, 0.2) * if k == 0 { 1.0 } else { -1.0 };
                self.map.oboxes.push(rocks::block(
                    at,
                    across.x.atan2(across.y) + rng.range(-0.15, 0.15),
                    half,
                    y,
                    top,
                    color,
                ));
                y = top - 0.4;
                w *= 0.9;
            }
        }
        // The span: rock blocks along an arc from leg to leg.
        let lift = 4.0 + rng.range(0.0, 2.0);
        let n = 9;
        let base = (foot[0] + foot[1]) * 0.5 + spring;
        let pts: Vec<(Vec2, f32)> = (0..=n)
            .map(|i| {
                let t = i as f32 / n as f32;
                let x = t * 2.0 - 1.0;
                (legs[0].lerp(legs[1], t), base + lift * (1.0 - x * x))
            })
            .collect();
        for w in pts.windows(2) {
            let ((a, ya), (b, yb)) = (w[0], w[1]);
            let len = a.distance(b);
            let mid = (a + b) * 0.5;
            let y = (ya + yb) * 0.5;
            let thick = 1.7;
            let color = pal.stone * rng.range(0.88, 1.04);
            let o = rocks::block(
                mid,
                across.x.atan2(across.y),
                Vec2::new(2.4, len * 0.5 + 0.6),
                y - thick,
                y + thick,
                color,
            );
            self.map.oboxes.push(OBox {
                tilt: -((yb - ya) / len).atan(),
                ..o
            });
        }
        // Blocks fallen from it, round its feet.
        for &p in &legs {
            for _ in 0..rng.int(2, 4) {
                let q = p + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(5.0, 8.0);
                let s = rng.range(0.9, 1.7);
                if q.distance(lm.center) > lm.clear + s * 1.2
                    && q.distance(lm.center) + s * 1.2 < lm.reach
                {
                    self.boulder(rng, q, s, s > 1.2);
                }
            }
        }
        self.strew(rng);
    }

    /// A lot of spires: two to five hoodoos, a few fallen blocks.
    fn spires(&mut self, rng: &mut Rng, lot: &Lot) {
        let want = rng.int(2, 5);
        let mut placed: Vec<Vec2> = Vec::new();
        for _ in 0..40 {
            if placed.len() >= want as usize {
                break;
            }
            let w = rng.range(1.8, 2.5);
            let p = lot.at()
                + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(0.0, lot.reach() - w * 1.6);
            if placed.iter().any(|q| q.distance(p) < 8.0) || self.sk.keep_clear(p) {
                continue;
            }
            let h = rng.range(5.0, 10.0);
            self.spire(rng, p, h, w);
            placed.push(p);
        }
        for _ in 0..3 {
            let p = lot.at()
                + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(0.0, lot.reach() - 2.0);
            let s = rng.range(0.8, 1.4);
            if self.room(p, s, 1.0, None) {
                self.boulder(rng, p, s, s > 1.1);
            }
        }
    }

    /// A lot of fins: low walls of rock standing out of the sand, blocks
    /// fallen off them.
    fn fins(&mut self, rng: &mut Rng, lot: &Lot) {
        let count = rng.int(1, 2);
        let along = Vec2::from_angle(rng.range(0.0, TAU));
        for k in 0..count {
            let off = along.perp()
                * if count == 1 {
                    0.0
                } else {
                    (k as f32 - 0.5) * 9.0
                };
            let half = (lot.reach() * lot.reach() - off.length_squared()).sqrt() - 2.0;
            let len = rng.range(0.6, 0.95) * 2.0 * half;
            let dir = Vec2::from_angle(rng.range(-0.25, 0.25)).rotate(along);
            let a = lot.at() + off - dir * len * 0.5;
            let mut t = 0.0;
            while t < len - 1.0 {
                let piece = rng.range(2.2, 3.4).min(len - t);
                let p = a + dir * (t + piece * 0.5);
                let g = self.g(p);
                let color = self.shade(rng);
                let mut o = rocks::block(
                    p,
                    dir.x.atan2(dir.y) + rng.range(-0.1, 0.1),
                    Vec2::new(rng.range(0.9, 1.3), piece * 0.5 + 0.15),
                    g - 0.8,
                    g + rng.range(1.8, 3.4),
                    color,
                );
                o.broken = true;
                self.map.oboxes.push(o);
                t += piece;
            }
        }
    }

    /// A lot of boulders: a dense field, a tank's width between them.
    fn boulder_field(&mut self, rng: &mut Rng, lot: &Lot) {
        for _ in 0..60 {
            let s = rng.range(1.4, 2.6);
            let p = lot.at()
                + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(0.0, lot.reach() - s * 1.3);
            if self.room(p, s, 5.0, None) {
                self.boulder(rng, p, s, true);
            }
        }
        for _ in 0..14 {
            let p = lot.at()
                + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(0.0, lot.reach() - 1.0);
            let s = rng.range(0.3, 0.7);
            self.boulder(rng, p, s, false);
        }
    }

    /// Stones washed down the washes and left on their floors.
    fn wash_stones(&mut self, rng: &mut Rng) {
        let sk = self.sk;
        for wash in &sk.land.washes {
            let total: f32 = wash.line.windows(2).map(|w| w[0].distance(w[1])).sum();
            let mut t = 0.0;
            while t < total {
                let (p, dir) = line_at(&wash.line, t);
                t += rng.range(6.0, 14.0);
                if p.min_element() < 6.0 || p.max_element() > sk.size - 6.0 {
                    continue;
                }
                let q = p + dir.perp() * rng.range(-WASH_FLOOR, WASH_FLOOR);
                if rng.chance(0.3) {
                    let s = rng.range(1.5, 2.2);
                    if self.room(q, s, 4.0, None) {
                        self.boulder(rng, q, s, true);
                    }
                } else if !sk.keep_clear(q) {
                    let s = rng.range(0.3, 0.7);
                    self.boulder(rng, q, s, false);
                }
            }
        }
    }

    /// Boulders scattered thinly over the rest, on both levels.
    fn scatter(&mut self, rng: &mut Rng) {
        const STEP: f32 = 12.0;
        let size = self.sk.size;
        let mut z = STEP * 0.5;
        while z < size {
            let mut x = STEP * 0.5;
            while x < size {
                let p = Vec2::new(x, z) + Vec2::new(rng.range(-4.0, 4.0), rng.range(-4.0, 4.0));
                let s = rng.range(1.6, 2.6);
                let thick = noise2(x / 60.0, z / 60.0, 95);
                if rng.chance(smoothstep(0.4, 0.8, thick) * 0.6) && self.room(p, s, 7.0, None) {
                    self.boulder(rng, p, s, true);
                }
                x += STEP;
            }
            z += STEP;
        }
    }

    /// Something to hide behind near `p`: a rock tank-high, or the ground
    /// rising.
    fn cover_near(&self, p: Vec2, r: f32) -> bool {
        let g = self.g(p);
        self.map.oboxes.iter().any(|o| {
            if !o.solid || o.distance(p) > r {
                return false;
            }
            let (lo, hi) = o.y_range();
            let go = self.g(o.center);
            hi > go + 1.4 && lo < go + 2.0 && (go - g).abs() < 4.5
        }) || (0..8).any(|k| {
            let q = p + Vec2::from_angle(k as f32 * TAU / 8.0) * (r * 0.5);
            self.g(q) > g + 2.0
        })
    }

    /// Wherever open ground is more than `REACH` m from anything to hide
    /// behind, a pile of rock.
    fn fill_cover(&mut self, rng: &mut Rng) {
        const REACH: f32 = 14.0;
        let size = self.sk.size;
        let mut pts = Vec::new();
        let mut z = 8.0;
        while z < size - 8.0 {
            let mut x = 8.0;
            while x < size - 8.0 {
                pts.push(Vec2::new(x, z));
                x += 5.0;
            }
            z += 5.0;
        }
        for i in (1..pts.len()).rev() {
            pts.swap(i, rng.index(i + 1));
        }
        for p in pts {
            if self.sk.keep_clear(p) || self.cover_near(p, REACH) {
                continue;
            }
            let s = rng.range(1.5, 2.2);
            let Some(q) = (0..10)
                .map(|k| {
                    let r = 2.0 + k as f32;
                    p + Vec2::new(rng.range(-r, r), rng.range(-r, r))
                })
                .find(|&q| self.room(q, s, 4.0, None))
            else {
                continue;
            };
            self.boulder(rng, q, s, true);
            if rng.chance(0.5) {
                let r = q + Vec2::from_angle(rng.range(0.0, TAU)) * s * rng.range(1.0, 1.3);
                let t = s * rng.range(0.55, 0.75);
                let first = self.map.oboxes.len() - 1;
                if self.room(r, t, 4.0, Some(first)) {
                    self.boulder(rng, r, t, true);
                }
            }
            if rng.chance(0.6) {
                let r = q + Vec2::from_angle(rng.range(0.0, TAU)) * rng.range(s + 1.0, s + 3.0);
                let t = rng.range(0.4, 0.7);
                self.boulder(rng, r, t, false);
            }
        }
    }

    /// The edge: a wall of boulders all the way round, on whatever level.
    fn edge_rocks(&mut self, rng: &mut Rng) {
        let size = self.sk.size;
        for (p, inward) in self.map.edge_points(3.2, 2.0) {
            let at = p + inward * rng.range(0.4, 1.0);
            let q = at.clamp(Vec2::splat(0.5), Vec2::splat(size - 0.5));
            let g = self.g(q);
            let s = rng.range(3.2, 4.2);
            let low = (0..8)
                .map(|k| {
                    let r = at + Vec2::from_angle(k as f32 * TAU / 8.0) * s * 1.4;
                    self.g(r)
                })
                .fold(g, f32::min);
            let half = Vec2::new(s, s * rng.range(0.85, 1.0));
            let color = self.shade(rng);
            let yaw = rng.range(0.0, TAU);
            let tall = rng.range(5.0, 8.0);
            self.map.oboxes.push(rocks::boulder(
                at,
                yaw,
                half,
                low.min(g - 1.5),
                g + tall,
                color,
            ));
        }
    }

    /// Nothing solid within `r` of a ground point, at the ground's height.
    fn clear(&self, p: Vec2, r: f32) -> bool {
        let g = self.g(p);
        !self.map.oboxes.iter().any(|o| {
            let (lo, hi) = o.y_range();
            o.solid && lo < g + 3.0 && hi > g + 0.3 && o.distance(p) < r
        })
    }

    /// Each side's spawns, the places bots head for (each a drive from the
    /// first side's spawns), and where repair drops come down.
    fn spawns(&mut self) {
        let sk = self.sk;
        for side in 0..self.map.bases.len().min(sk.bases.len()) {
            let pts = sk.bases[side].spawn_points();
            self.map.bases[side] = pts
                .into_iter()
                .filter(|&p| self.clear(p, TANK_CLEAR))
                .collect();
        }
        let mut spots = Vec::new();
        let mut z = 8.0;
        while z < sk.size - 8.0 {
            let mut x = 8.0;
            while x < sk.size - 8.0 {
                let p = Vec2::new(x, z);
                if self.clear(p, TANK_CLEAR) && sk.land.plateau(p).abs() > 3.0 {
                    spots.push(p);
                }
                x += 10.0;
            }
            z += 10.0;
        }
        let nav = NavGrid::build(self.map);
        let from = sk.base_spots()[0];
        self.map.open_spots = spots
            .into_iter()
            .filter(|&q| {
                nav.nearest_open_point(q)
                    .is_some_and(|c| c.distance(q) < 2.5)
                    && !nav.path(from, q).is_empty()
            })
            .collect();
        self.routes(&nav);
        // Repair drops: either side of the landmark, and partway out to
        // each side's spawns.
        let lm = sk.landmarks[0].center;
        let across = (sk.base_spots()[0] - lm).normalize_or(Vec2::X).perp();
        let mut want = vec![
            lm + across * (sk.landmarks[0].reach + 10.0),
            lm - across * (sk.landmarks[0].reach + 10.0),
        ];
        for &b in &sk.base_spots() {
            want.push(lm.lerp(b, 0.55) + (b - lm).normalize_or(Vec2::X).perp() * 14.0);
        }
        self.map.repair_spots = want
            .into_iter()
            .filter_map(|p| {
                (0..120).find_map(|k| {
                    let q = p + Vec2::from_angle(k as f32 * 2.4) * (k as f32 * 0.35);
                    let g = self.g(q);
                    let level = (0..12).all(|k| {
                        let r = q + Vec2::from_angle(k as f32 * TAU / 12.0) * 5.0;
                        (self.g(r) - g).abs() < 0.6
                    });
                    let sky = !self
                        .map
                        .oboxes
                        .iter()
                        .any(|o| o.ray_hit(Vec3::new(q.x, g + 0.5, q.y), Vec3::Y, 80.0));
                    let inside = q.min_element() > 12.0 && q.max_element() < sk.size - 12.0;
                    let ok = inside
                        && level
                        && sky
                        && sk.land.plateau(q).abs() > 6.0
                        && self.clear(q, TANK_CLEAR)
                        && !nav.path(from, q).is_empty();
                    ok.then_some(q)
                })
            })
            .take(4)
            .collect();
    }

    /// Ways between the two sides' spawns for the bots to spread out over
    /// (see `Map::routes`): drives by way of somewhere off the straight
    /// line, some on the high ground where there is any and some below,
    /// each a good part of its length well away from the others.
    fn routes(&mut self, nav: &NavGrid) {
        let sk = self.sk;
        let &[a, b] = sk.base_spots().as_slice() else {
            return;
        };
        let top = |p: Vec2| Vec3::new(p.x, f32::INFINITY, p.y);
        let flat = |p: Vec3| Vec2::new(p.x, p.z);
        let direct = a.distance(b);
        // The whole line, a point every 12 m or so on its floor.
        let walk = |legs: &[Vec3]| -> Vec<Vec3> {
            let mut out: Vec<Vec3> = Vec::new();
            for w in legs.windows(2) {
                let n = (w[0].distance(w[1]) / 12.0).ceil().max(1.0) as usize;
                for k in 0..n {
                    let p = flat(w[0].lerp(w[1], k as f32 / n as f32));
                    if !nav.is_blocked(p) {
                        out.push(Vec3::new(p.x, self.g(p), p.y));
                    }
                }
            }
            out
        };
        let mut found: Vec<(f32, f32, Vec<Vec3>)> = Vec::new();
        for &via in self.map.open_spots.iter().step_by(2) {
            // Not by either side's spawns.
            if via.distance(a).min(via.distance(b)) < direct * 0.25 {
                continue;
            }
            let (out, back) = (nav.route(top(a), top(via)), nav.route(top(via), top(b)));
            if out.is_empty() || back.is_empty() {
                continue;
            }
            let mut legs = vec![Vec3::new(a.x, self.g(a), a.y)];
            legs.extend(out);
            legs.extend(back);
            let len: f32 = legs
                .windows(2)
                .map(|w| flat(w[0]).distance(flat(w[1])))
                .sum();
            let pts = walk(&legs);
            // No longer than a bot will take (see `ai::ways_toward`), and
            // not out to the spot and back the way it came.
            let doubles = pts.iter().enumerate().any(|(i, p)| {
                pts[(i + 5).min(pts.len())..]
                    .iter()
                    .any(|q| p.distance(*q) < 10.0)
            });
            if len > direct * 1.5 + 30.0 || doubles || pts.len() < 4 {
                continue;
            }
            // Not through the spawns themselves: every way shares those.
            let pts: Vec<Vec3> = pts
                .into_iter()
                .filter(|p| flat(*p).distance(a).min(flat(*p).distance(b)) > sk.pad + 10.0)
                .collect();
            if pts.len() < 2 {
                continue;
            }
            let high = pts.iter().filter(|p| sk.land.on_high(flat(**p))).count();
            found.push((high as f32 / pts.len() as f32, len / direct, pts));
        }
        // By turns the highest way left and the lowest (shorter first), so
        // both levels get used.
        found.sort_by(|x, y| (y.0 - 0.5 * y.1).total_cmp(&(x.0 - 0.5 * x.1)));
        let mut low: Vec<usize> = (0..found.len()).collect();
        low.sort_by(|&x, &y| {
            let cost = |k: usize| found[k].0 + 0.5 * found[k].1;
            cost(x).total_cmp(&cost(y))
        });
        let mut ways: Vec<Vec<Vec3>> = Vec::new();
        let distinct = |ways: &[Vec<Vec3>], pts: &[Vec3]| {
            // How much of it is its own ground.
            ways.iter().all(|w| {
                let off = |p: &&Vec3| w.iter().all(|q| p.distance(*q) > 22.0);
                pts.iter().filter(off).count() as f32 / pts.len() as f32 > 0.3
            })
        };
        let mut turns = [(0..found.len()).collect::<Vec<_>>(), low].map(Vec::into_iter);
        for side in (0..MAX_ROUTES).map(|k| k % 2) {
            if let Some(k) = turns[side].find(|&k| distinct(&ways, &found[k].2)) {
                ways.push(found[k].2.clone());
            }
        }
        self.map.routes = ways;
    }

    /// Beyond the edge: the desert going on into hills and big dunes,
    /// buttes standing out of it.
    fn horizon(&mut self, rng: &mut Rng) {
        let sk = self.sk;
        let cell = 6.0f32;
        let n = ((sk.size + 2.0 * 520.0) / cell).round() as usize + 1;
        let reach = ((n - 1) as f32 * cell - sk.size) * 0.5;
        let mut t = Terrain::from_fn(Vec2::splat(-reach), cell, n, n, |x, z| {
            sk.scenery_ground(x, z)
        });
        t.color = self.pal.sand * 0.95;
        t.material = Material::Sand;
        self.map.scenery_terrain = Some(t);
        for _ in 0..36 {
            let dir = Vec2::from_angle(rng.range(0.0, TAU));
            let w = rng.range(10.0, 30.0);
            let out = rng.range(40.0 + w, 320.0);
            let edge = sk.size * 0.5 / dir.x.abs().max(dir.y.abs());
            let at = Vec2::splat(sk.size * 0.5) + dir * (edge + out);
            let g = sk.scenery_ground(at.x, at.y) - 2.0;
            let h = rng.range(12.0, 40.0);
            let half = Vec2::new(w, w * rng.range(0.5, 0.9));
            let color = self.shade(rng);
            let yaw = rng.range(0.0, TAU);
            self.map.oboxes.push(OBox {
                solid: false,
                broken: true,
                ..rocks::boulder(at, yaw, half, g, g + h, color)
            });
        }
    }

    /// Where the menus show each side's tank: by its spawns, in the sun,
    /// the camera looking past it toward the landmark.
    fn vistas(&self) -> Vec<(Vec2, f32)> {
        let sk = self.sk;
        let m = &*self.map;
        let sun = m.atmosphere.sun_dir.normalize();
        let lm = sk.landmarks[0].center;
        let view = |p: Vec2| {
            let d = lm - p;
            (-d.x).atan2(-d.y)
        };
        sk.base_spots()
            .iter()
            .map(|&b| {
                let want = b.lerp(lm, 0.12);
                let mut spots = Vec::new();
                let mut u = -30.0;
                while u <= 30.0 {
                    let mut v = -30.0;
                    while v <= 30.0 {
                        spots.push(want + Vec2::new(u, v));
                        v += 3.0;
                    }
                    u += 3.0;
                }
                spots.sort_by(|a, c| a.distance(want).total_cmp(&c.distance(want)));
                let good = |p: Vec2| {
                    let g = m.ground_height(p.x, p.y);
                    let at = Vec3::new(p.x, g + 1.9, p.y);
                    let blocked =
                        |dir: Vec3, len: f32| m.oboxes.iter().any(|o| o.ray_hit(at, dir, len));
                    let under = m
                        .oboxes
                        .iter()
                        .any(|o| o.distance(p) < 3.4 && o.y1 > g + 0.05 && o.y0 < g + 3.0);
                    let v = view(p);
                    let back = Vec2::new(v.sin(), v.cos());
                    let level = (0..=14).all(|k| {
                        let q = p + back * k as f32;
                        (m.ground_height(q.x, q.y) - g).abs() < 0.5
                    });
                    level && !under && !blocked(sun, 300.0) && !blocked(Vec3::Y, 60.0)
                };
                let p = spots.into_iter().find(|&p| good(p)).unwrap_or(b);
                (p, view(p))
            })
            .collect()
    }
}
