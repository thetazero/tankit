//! Where a tank can drive, for the bots: a grid of 2 m cells over the map.
//! Each cell has the floor a tank drives on seen from above (the ground, or
//! a deck or cave roof laid over it), and, where that deck or roof stands
//! high enough over the ground for a tank to drive under it, the floor
//! beneath as well. A route steps from a floor to a floor in the next cell
//! only at about the same height, so a bot on a bridge's deck plans along
//! the deck, one under it plans along the pit floor, and the two only meet
//! where the levels do (a ramp, a road, a cave mouth).
//!
//! On a map with nothing stacked every cell has just the one floor, and the
//! grid is the flat one it always was.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use glam::{Vec2, Vec3};

use crate::map::{Map, OBox, Terrain};

/// Nav cell size, m. Fine enough that narrow gaps between cover show up.
const NAV_CELL: f32 = 2.0;
/// How far obstacles are grown so paths keep a hull's width off walls.
const CLEARANCE: f32 = 2.2;
/// On a deck, a low rail along its edge (that a tank can see and shoot
/// over) is only kept this far off: it's there to keep tanks on.
const RAIL_CLEARANCE: f32 = 1.0;
/// Steepest ground (rise over run) bots will path across.
const MAX_SLOPE: f32 = 0.7;
/// On a mesa, bots keep this far back from the edge, m.
const EDGE_MARGIN: f32 = 7.0;
/// Room a tank needs to drive under a deck or a roof, m.
const HEADROOM: f32 = 3.2;
/// Most a floor rises or falls from a cell to the next one along, m (the
/// steepest slope a route takes, with some slack).
const STEP: f32 = 1.6;
/// A tank's height, for what it would hit.
const TANK_HEIGHT: f32 = 3.0;
/// On the ground, routes keep this far back from a drop (a cliff's lip,
/// the edge of a road cut into a pit's wall), m: a hover hull slides on
/// through a turn. A deck's rails keep tanks on it instead.
const DROP_MARGIN: f32 = 3.0;
/// What counts as a drop there, m.
const DROP: f32 = 2.0;
/// Floors further than this from a height aren't "at" it, m.
const LEVEL_TOLERANCE: f32 = 2.5;

/// The top floor (always there) and the one under it (NaN where none).
const TOP: usize = 0;
const UNDER: usize = 1;

pub struct NavGrid {
    n: usize,
    floor: Vec<[f32; 2]>,
    blocked: Vec<[bool; 2]>,
    terrain: Option<Terrain>,
    /// Flat drivable slabs (decks, roofs): floors over the ground.
    slabs: Vec<OBox>,
}

/// A floor of a cell: (cell index, TOP or UNDER).
type Spot = (usize, usize);

impl NavGrid {
    pub fn build(map: &Map) -> NavGrid {
        let n = (map.size / NAV_CELL).ceil() as usize;
        let slabs = map
            .oboxes
            .iter()
            .filter(|o| o.solid && o.ramp && !o.is_turned())
            .cloned()
            .collect();
        let mut nav = NavGrid {
            n,
            floor: Vec::new(),
            blocked: vec![[false; 2]; n * n],
            terrain: map.terrain.clone(),
            slabs,
        };
        nav.floor = (0..n * n).map(|i| nav.floors_at(nav.center(i))).collect();

        for s in &map.solids {
            // Anything a tank can't drive over: tall, and standing on the floor.
            if s.max.y - s.min.y < 0.6 {
                continue;
            }
            let (lo, hi) = (s.min2() - CLEARANCE, s.max2() + CLEARANCE);
            nav.mark(lo, hi, |c, f, _| {
                c.cmpge(lo).all() && c.cmple(hi).all() && s.min.y < f + 1.0 && s.max.y > f + 0.6
            });
        }
        for o in &map.oboxes {
            // Debris and drivable ramps don't block; neither does anything low.
            // (A tall leaning spire is tilted like a ramp, but it's a wall.)
            // Measured as the piece actually stands (tilted, rolled): a
            // standing post is tall and narrow, not a beam lying flat.
            let (y0, y1) = o.y_range();
            if !o.solid || (o.is_ramp() && o.y1 - o.y0 < 1.5) || y1 - y0 < 0.6 {
                continue;
            }
            let reach = o.half.length().max((y1 - y0) * 0.5) + CLEARANCE;
            let (lo, hi) = (o.center - Vec2::splat(reach), o.center + Vec2::splat(reach));
            nav.mark(lo, hi, |c, f, on_slab| {
                // Only what stands on this floor, up into a tank's way.
                if y0 >= f + 1.0 || y1 <= f + 0.6 {
                    return false;
                }
                let keep = if on_slab && y1 < f + 1.2 {
                    RAIL_CLEARANCE
                } else {
                    CLEARANCE
                };
                o.base_distance(c, TANK_HEIGHT) <= keep
            });
        }
        // Crystals: wherever they stand low enough over a floor to hit.
        for c in map.crystals.iter().filter(|c| c.solid) {
            for i in 0..=4 {
                let t = i as f32 / 4.0 * 0.6;
                let p = c.base + c.axis * t;
                let (q, r) = (Vec2::new(p.x, p.z), c.radius * (1.0 - t * 0.3));
                let reach = r + CLEARANCE;
                nav.mark(
                    q - Vec2::splat(reach),
                    q + Vec2::splat(reach),
                    |cc, f, _| p.y < f + 3.0 && p.y > f - 1.5 && cc.distance(q) <= reach,
                );
            }
        }
        // Floors too steep to drive up (cliffs, steep banks, a deck's edge).
        for i in 0..n * n {
            let c = nav.center(i);
            for l in [TOP, UNDER] {
                let h = nav.floor[i][l];
                if h.is_nan() {
                    continue;
                }
                let s = |d: Vec2| nav.surface_near(c + d, h);
                let e = NAV_CELL * 0.5;
                let gx = (s(Vec2::new(e, 0.0)) - s(Vec2::new(-e, 0.0))) / (2.0 * e);
                let gz = (s(Vec2::new(0.0, e)) - s(Vec2::new(0.0, -e))) / (2.0 * e);
                if Vec2::new(gx, gz).length() > MAX_SLOPE {
                    nav.blocked[i][l] = true;
                }
            }
        }
        // Back from drops.
        for i in 0..n * n {
            let c = nav.center(i);
            for l in [TOP, UNDER] {
                let h = nav.floor[i][l];
                if h.is_nan() || nav.blocked[i][l] || (l == TOP && h > nav.ground(c) + 0.1) {
                    continue;
                }
                let near_drop = (0..16).any(|k| {
                    let a = k as f32 / 16.0 * std::f32::consts::TAU;
                    let d = Vec2::new(a.cos(), a.sin());
                    [DROP_MARGIN * 0.5, DROP_MARGIN]
                        .iter()
                        .any(|&r| nav.surface_near(c + d * r, h) < h - DROP)
                });
                if near_drop {
                    nav.blocked[i][l] = true;
                }
            }
        }
        // A mesa: keep well back from the edge, wherever the ground drops
        // away within reach.
        if let Some(fall_y) = map.fall_y {
            let drop = fall_y * 0.3;
            for i in 0..n * n {
                let c = nav.center(i);
                let near_edge = (0..12).any(|k| {
                    let a = k as f32 / 12.0 * std::f32::consts::TAU;
                    let p = c + Vec2::new(a.cos(), a.sin()) * EDGE_MARGIN;
                    map.ground_height(p.x, p.y) < drop
                });
                if near_edge || map.ground_height(c.x, c.y) < drop {
                    nav.blocked[i][TOP] = true;
                }
            }
        }
        nav
    }

    /// Run `hits(cell center, floor, floor is a slab)` over every floor of
    /// every cell between `lo` and `hi`, blocking it where it says so.
    fn mark(&mut self, lo: Vec2, hi: Vec2, hits: impl Fn(Vec2, f32, bool) -> bool) {
        let n = self.n;
        let c0 = (lo / NAV_CELL).floor().max(Vec2::ZERO).as_uvec2();
        let c1 = (hi / NAV_CELL)
            .ceil()
            .as_uvec2()
            .min(glam::UVec2::splat(n as u32));
        for iz in c0.y as usize..c1.y as usize {
            for ix in c0.x as usize..c1.x as usize {
                let i = iz * n + ix;
                let c = self.center(i);
                for l in [TOP, UNDER] {
                    let f = self.floor[i][l];
                    if f.is_nan() || self.blocked[i][l] {
                        continue;
                    }
                    let on_slab = l == TOP && f > self.ground(c) + 0.1;
                    if hits(c, f, on_slab) {
                        self.blocked[i][l] = true;
                    }
                }
            }
        }
    }

    fn ground(&self, p: Vec2) -> f32 {
        self.terrain.as_ref().map_or(0.0, |t| t.height_at(p.x, p.y))
    }

    /// The floors at a point: the top one, and the one under it (NaN where
    /// nothing stands high enough over the ground to drive under).
    fn floors_at(&self, p: Vec2) -> [f32; 2] {
        let g = self.ground(p);
        let (mut top, mut under) = (g, f32::NAN);
        for s in &self.slabs {
            let l = s.local_point(p);
            if l.x.abs() > s.half.x || l.y.abs() > s.half.y {
                continue;
            }
            if s.y0 >= g + HEADROOM {
                under = g;
            }
            top = top.max(s.y1);
        }
        [top, under]
    }

    /// The floor at a point nearest height `h`.
    fn surface_near(&self, p: Vec2, h: f32) -> f32 {
        let [top, under] = self.floors_at(p);
        if !under.is_nan() && (under - h).abs() < (top - h).abs() {
            under
        } else {
            top
        }
    }

    fn cell(&self, p: Vec2) -> usize {
        let c = (p / NAV_CELL).floor();
        let ix = c.x.clamp(0.0, self.n as f32 - 1.0) as usize;
        let iz = c.y.clamp(0.0, self.n as f32 - 1.0) as usize;
        iz * self.n + ix
    }

    fn center(&self, i: usize) -> Vec2 {
        Vec2::new((i % self.n) as f32 + 0.5, (i / self.n) as f32 + 0.5) * NAV_CELL
    }

    fn point(&self, (i, l): Spot) -> Vec3 {
        let c = self.center(i);
        Vec3::new(c.x, self.floor[i][l], c.y)
    }

    /// The floor of cell `i` nearest height `y` (the top one for an
    /// infinite `y`), open or not.
    fn level(&self, i: usize, y: f32) -> usize {
        let [top, under] = self.floor[i];
        if !under.is_nan() && y.is_finite() && (under - y).abs() < (top - y).abs() {
            UNDER
        } else {
            TOP
        }
    }

    /// The open floor of cell `i` within `step` of height `h`, nearest it.
    fn step_to(&self, i: usize, h: f32, step: f32) -> Option<usize> {
        [TOP, UNDER]
            .into_iter()
            .filter(|&l| {
                let f = self.floor[i][l];
                !f.is_nan() && !self.blocked[i][l] && (f - h).abs() <= step
            })
            .min_by(|&a, &b| {
                (self.floor[i][a] - h)
                    .abs()
                    .total_cmp(&(self.floor[i][b] - h).abs())
            })
    }

    /// Top floor blocked at `p` (what's driven on seen from above).
    pub fn is_blocked(&self, p: Vec2) -> bool {
        self.blocked[self.cell(p)][TOP]
    }

    /// Nearest open floor to `p` (search in growing rings): at about `p`'s
    /// height if there's one near, else at any. An infinite `p.y` means the
    /// top floor.
    fn nearest_open(&self, p: Vec3) -> Option<Spot> {
        let flat = Vec2::new(p.x, p.z);
        let i0 = self.cell(flat);
        let (cx, cz) = ((i0 % self.n) as i32, (i0 / self.n) as i32);
        let fits = |i: usize, l: usize, strict: bool| {
            let f = self.floor[i][l];
            if f.is_nan() || self.blocked[i][l] {
                return false;
            }
            if !strict {
                return true;
            }
            if p.y.is_finite() {
                (f - p.y).abs() < LEVEL_TOLERANCE
            } else {
                l == TOP
            }
        };
        for strict in [true, false] {
            for r in 0..12i32 {
                let mut best: Option<(Spot, f32)> = None;
                for dz in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() != r && dz.abs() != r {
                            continue;
                        }
                        let (x, z) = (cx + dx, cz + dz);
                        if x < 0 || z < 0 || x >= self.n as i32 || z >= self.n as i32 {
                            continue;
                        }
                        let i = z as usize * self.n + x as usize;
                        for l in [TOP, UNDER] {
                            if !fits(i, l, strict) {
                                continue;
                            }
                            let d = self.center(i).distance_squared(flat);
                            // Same cell, two floors: the nearer in height.
                            let d = d + if p.y.is_finite() {
                                (self.floor[i][l] - p.y).abs() * 1e-3
                            } else {
                                l as f32 * 1e-3
                            };
                            if best.is_none_or(|(_, bd)| d < bd) {
                                best = Some(((i, l), d));
                            }
                        }
                    }
                }
                if let Some((s, _)) = best {
                    return Some(s);
                }
            }
        }
        None
    }

    /// The open floor at `p` about its height, if there's one (tests).
    #[cfg(test)]
    pub fn floor_at(&self, p: Vec3) -> Option<f32> {
        let i = self.cell(Vec2::new(p.x, p.z));
        [TOP, UNDER]
            .into_iter()
            .map(|l| (l, self.floor[i][l]))
            .find(|&(l, f)| !f.is_nan() && !self.blocked[i][l] && (f - p.y).abs() < LEVEL_TOLERANCE)
            .map(|(_, f)| f)
    }

    /// Open ground right by `p` on its top floor (spawn checks).
    #[cfg(test)]
    pub fn nearest_open_point(&self, p: Vec2) -> Option<Vec2> {
        self.nearest_open(Vec3::new(p.x, f32::INFINITY, p.y))
            .map(|(i, _)| self.center(i))
    }

    /// Driving straight from `a` (on the floor nearest its height) to `b`,
    /// following the floor, stays on open floors: the height it ends at,
    /// if so.
    fn walk(&self, a: Vec3, b: Vec2) -> Option<f32> {
        let a2 = Vec2::new(a.x, a.z);
        let mut i = self.cell(a2);
        let mut l = self.level(i, a.y);
        if self.blocked[i][l] {
            return None;
        }
        let mut h = self.floor[i][l];
        let steps = (a2.distance(b) / (NAV_CELL * 0.5)).ceil().max(1.0) as usize;
        for k in 1..=steps {
            let j = self.cell(a2.lerp(b, k as f32 / steps as f32));
            if j == i {
                continue;
            }
            l = self.step_to(j, h, STEP * 1.5)?;
            i = j;
            h = self.floor[i][l];
        }
        Some(h)
    }

    /// Straight drive from `a` to `b` stays on open top floors.
    pub fn clear_line(&self, a: Vec2, b: Vec2) -> bool {
        self.walk(Vec3::new(a.x, f32::INFINITY, a.y), b).is_some()
    }

    /// Straight drive from `a` (on the floor at its height) to `b` stays on
    /// open floors, on the level it starts on.
    pub fn clear_from(&self, a: Vec3, b: Vec2) -> bool {
        self.walk(a, b).is_some()
    }

    /// Driving from `a` (a tank's position) towards `b` goes off a drop: the
    /// floor falls away steeper than one in one somewhere (a cliff, a pit's
    /// wall, a deck's side), far steeper than any route goes.
    pub fn ledge(&self, a: Vec3, b: Vec2) -> bool {
        let a2 = Vec2::new(a.x, a.z);
        let steps = (a2.distance(b) / 0.5).ceil().max(1.0) as usize;
        let mut h = self.surface_near(a2, a.y - 0.5);
        for k in 1..=steps {
            let s = self.surface_near(a2.lerp(b, k as f32 / steps as f32), h);
            if h - s > 0.5 {
                return true;
            }
            h = s;
        }
        false
    }

    /// Waypoints from `from` to `to` on the top floors (excluding the
    /// start), string-pulled. Empty if unreachable.
    pub fn path(&self, from: Vec2, to: Vec2) -> Vec<Vec2> {
        let top = |p: Vec2| Vec3::new(p.x, f32::INFINITY, p.y);
        self.route(top(from), top(to))
            .into_iter()
            .map(|p| Vec2::new(p.x, p.z))
            .collect()
    }

    /// Waypoints, each on its floor, from `from` to `to` (excluding the
    /// start; each end on the floor nearest its height, the top one for an
    /// infinite height), string-pulled so bots cut straight across open
    /// ground. Empty if unreachable.
    pub fn route(&self, from: Vec3, to: Vec3) -> Vec<Vec3> {
        let (Some(start), Some(goal)) = (self.nearest_open(from), self.nearest_open(to)) else {
            return Vec::new();
        };
        let n = self.n;
        let id = |(i, l): Spot| i * 2 + l;
        let spot = |k: usize| (k / 2, k % 2);
        let (gx, gz) = ((goal.0 % n) as f32, (goal.0 / n) as f32);
        let h = |s: Spot| {
            let (dx, dz) = (((s.0 % n) as f32 - gx).abs(), ((s.0 / n) as f32 - gz).abs());
            dx.max(dz) + (std::f32::consts::SQRT_2 - 1.0) * dx.min(dz)
        };
        let mut g = vec![f32::INFINITY; n * n * 2];
        let mut came = vec![usize::MAX; n * n * 2];
        let mut open = BinaryHeap::new();
        g[id(start)] = 0.0;
        open.push(Node {
            f: h(start),
            at: id(start),
        });
        let mut found = false;
        while let Some(Node { at, f }) = open.pop() {
            let s = spot(at);
            if s == goal {
                found = true;
                break;
            }
            let gc = g[at];
            if f > gc + h(s) + 1e-3 {
                continue;
            }
            let here = self.floor[s.0][s.1];
            let (x0, z0) = ((s.0 % n) as i32, (s.0 / n) as i32);
            for (dx, dz) in [
                (1i32, 0i32),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
            ] {
                let (x, z) = (x0 + dx, z0 + dz);
                if x < 0 || z < 0 || x >= n as i32 || z >= n as i32 {
                    continue;
                }
                let j = z as usize * n + x as usize;
                let diag = dx != 0 && dz != 0;
                let reach = if diag { STEP * 1.4 } else { STEP };
                let Some(l) = self.step_to(j, here, reach) else {
                    continue;
                };
                // No cutting corners past a blocked cell.
                if diag {
                    let a = z0 as usize * n + x as usize;
                    let b = z as usize * n + x0 as usize;
                    if self.step_to(a, here, reach).is_none()
                        || self.step_to(b, here, reach).is_none()
                    {
                        continue;
                    }
                }
                let next = id((j, l));
                let cost = gc + if diag { std::f32::consts::SQRT_2 } else { 1.0 };
                if cost < g[next] {
                    g[next] = cost;
                    came[next] = at;
                    open.push(Node {
                        f: cost + h((j, l)),
                        at: next,
                    });
                }
            }
        }
        if !found {
            return Vec::new();
        }
        let mut spots = vec![goal];
        let mut at = id(goal);
        while at != id(start) {
            at = came[at];
            spots.push(spot(at));
        }
        spots.reverse();
        // String pulling: from each kept point, skip ahead as far as a
        // straight line stays open and on the level.
        let pts: Vec<Vec3> = spots.iter().map(|&s| self.point(s)).collect();
        let reaches = |a: Vec3, b: Vec3| {
            self.walk(a, Vec2::new(b.x, b.z))
                .is_some_and(|h| (h - b.y).abs() < 0.5)
        };
        let mut out = Vec::new();
        let mut i = 0;
        let mut from_pt = Vec3::new(from.x, pts[0].y, from.z);
        while i + 1 < pts.len() {
            let mut j = pts.len() - 1;
            while j > i + 1 && !reaches(from_pt, pts[j]) {
                j -= 1;
            }
            out.push(pts[j]);
            from_pt = pts[j];
            i = j;
        }
        if out.is_empty() {
            out.push(self.point(goal));
        }
        out
    }
}

#[derive(PartialEq)]
struct Node {
    f: f32,
    at: usize,
}
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.partial_cmp(&self.f).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
