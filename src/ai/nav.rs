//! Where a tank can drive, for the bots: a grid of 2 m cells over the map.
//! Each cell has the floor a tank drives on seen from above (the ground, or
//! a deck or cave roof laid over it), and, where that deck or roof stands
//! high enough over the ground for a tank to drive under it, the floor
//! beneath as well. A route steps from a floor to a floor in the next cell
//! only at about the same height, so a bot on a bridge's deck plans along
//! the deck, one under it along the ground beneath, and the two only meet
//! where the levels do (a ramp, a road, a cave mouth).
//!
//! On a map with nothing stacked every cell has just the one floor, and the
//! grid is the flat one it always was.
//!
//! Tracked hulls keep out of a plasma river; hover hulls ride on it, so they
//! have a grid of their own where there's one (`build_hover`), its surface
//! a floor like any other.

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
/// Where a deck's wide enough, a route keeps this far in from its rails (a
/// hull's half-width and some), m...
const DECK_MARGIN: f32 = 2.0;
/// ...and a cell by its edge costs this many times a cell more.
const EDGE_COST: f32 = 1.5;
/// Down the sides of a ramp up onto a deck, further than this is an edge, m.
const KERB: f32 = 0.35;
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
/// How far past a plasma river's edge it's still drawn, m.
const RIVER_MARGIN: f32 = 0.6;
/// How far back from the top of its banks tracked hulls keep, m.
const BANK_MARGIN: f32 = 1.0;
/// Floors further than this from a height aren't "at" it, m.
const LEVEL_TOLERANCE: f32 = 2.5;

/// The top floor (always there); the ones under it follow, the highest
/// first (NaN where none).
const TOP: usize = 0;
/// The most floors there may be over one another.
const LEVELS: usize = 4;

pub struct NavGrid {
    n: usize,
    floor: Vec<[f32; LEVELS]>,
    blocked: Vec<[bool; LEVELS]>,
    terrain: Option<Terrain>,
    solid: Option<std::sync::Arc<crate::map::solid::Surface>>,
    /// Drivable slabs: floors over the ground. Level ones (decks, roofs),
    /// and the ramps up onto those from the ground at their ends.
    slabs: Vec<OBox>,
    /// For a hover hull: the rivers it rides on, and the one each cell's by,
    /// with the stretch of it that's nearest (looking one up is slow).
    rivers: std::sync::Arc<Vec<crate::map::river::River>>,
    wet: Vec<Option<(u32, u32)>>,
}

/// Which of `floors` is nearest height `h` (the higher, if two are), and
/// its height.
fn nearest(floors: &[f32; LEVELS], h: f32) -> (usize, f32) {
    let mut best = (TOP, floors[TOP]);
    for (l, &f) in floors.iter().enumerate().skip(1) {
        if !f.is_nan() && (f - h).abs() < (best.1 - h).abs() {
            best = (l, f);
        }
    }
    best
}

/// The top of a slab `z` along it (its local +z tips down by its tilt).
fn slab_top(s: &OBox, z: f32) -> f32 {
    let thick = (s.y1 - s.y0) * 0.5;
    (s.y0 + s.y1) * 0.5 + thick / s.tilt.cos() - z * s.tilt.tan()
}

/// Whether a ramp's high end meets one of the level `decks`.
fn meets_deck(ramp: &OBox, decks: &[OBox]) -> bool {
    let (_, az) = ramp.axes();
    let end = -ramp.tilt.signum() * ramp.half.y * ramp.tilt.cos();
    let top = slab_top(ramp, end);
    let past = ramp.center + az * (end + 0.5 * end.signum());
    decks.iter().any(|d| {
        let l = d.local_point(past);
        l.x.abs() <= d.half.x && l.y.abs() <= d.half.y && (d.y1 - top).abs() < 0.3
    })
}

/// A floor of a cell: (cell index, which floor of it).
type Spot = (usize, usize);

impl NavGrid {
    /// Where a tracked tank can drive.
    pub fn build(map: &Map) -> NavGrid {
        NavGrid::build_for(map, false)
    }

    /// Where a hover hull can: over plasma rivers, too. (Where there are
    /// none it's `build`'s.)
    pub fn build_hover(map: &Map) -> NavGrid {
        NavGrid::build_for(map, true)
    }

    fn build_for(map: &Map, hover: bool) -> NavGrid {
        let n = (map.size / NAV_CELL).ceil() as usize;
        let drivable = || map.oboxes.iter().filter(|o| o.solid && o.ramp);
        let mut slabs: Vec<OBox> = drivable().filter(|o| !o.is_turned()).copied().collect();
        let ramps: Vec<OBox> = drivable()
            .filter(|o| o.tilt != 0.0 && o.roll == 0.0 && meets_deck(o, &slabs))
            .copied()
            .collect();
        slabs.extend(&ramps);
        let mut nav = NavGrid {
            n,
            floor: Vec::new(),
            blocked: vec![[false; LEVELS]; n * n],
            terrain: map.terrain.clone(),
            solid: map.solid.clone(),
            slabs,
            rivers: Default::default(),
            wet: Vec::new(),
        };
        if hover && !map.rivers.is_empty() {
            nav.wet = (0..n * n)
                .map(|i| {
                    let (near, c) = (RIVER_MARGIN + NAV_CELL, nav.center(i));
                    map.rivers.iter().enumerate().find_map(|(k, r)| {
                        let (_, seg) = r.surface_in(c, near, 0..r.points.len())?;
                        Some((k as u32, seg as u32))
                    })
                })
                .collect();
            nav.rivers = map.rivers.clone();
        }
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
            // A rail standing on a deck: kept only a little off from either
            // side, so where a deck meets the ground at its end a rail's
            // end doesn't close the way on.
            // What a tank on the ground runs into, worked out once (a rock's
            // is its whole surface's hull: not cheap).
            let foot = o.base_shadow(TANK_HEIGHT);
            let deck = nav.floors_at(o.center)[TOP];
            let on_deck = deck > nav.ground(o.center) + 0.1 && (o.y0 - deck).abs() < 0.1;
            nav.mark(lo, hi, |c, f, on_slab| {
                // Only what stands on this floor, up into a tank's way.
                if y0 >= f + 1.0 || y1 <= f + 0.6 {
                    return false;
                }
                let keep = if (on_slab || on_deck) && y1 < f + 1.2 {
                    RAIL_CLEARANCE
                } else {
                    CLEARANCE
                };
                crate::map::polygon_distance(&foot, c) <= keep
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
        // Beams (a borer's): nowhere near one, wherever it runs low enough
        // over a floor to touch a tank.
        for b in &map.beams {
            let steps = (b.from.distance(b.to) / 0.5).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let p = b.from.lerp(b.to, k as f32 / steps as f32);
                let q = Vec2::new(p.x, p.z);
                let reach = b.radius + CLEARANCE + 1.5;
                nav.mark(
                    q - Vec2::splat(reach),
                    q + Vec2::splat(reach),
                    |cc, f, _| p.y < f + 4.0 && p.y > f - 1.5 && cc.distance(q) <= reach,
                );
            }
        }
        // Past the edge of an arena that isn't the whole square.
        if let Some(b) = &map.band {
            for i in 0..n * n {
                if b.out(nav.center(i)) > 4.0 {
                    nav.blocked[i] = [true; LEVELS];
                }
            }
        }
        // A plasma river's bed and its banks, which a tracked hull would slide
        // down (its bridges are slabs over them).
        if !hover && !map.rivers.is_empty() {
            for i in 0..n * n {
                let c = nav.center(i);
                let Some(y) = map
                    .rivers
                    .iter()
                    .find_map(|r| r.surface(c, r.bank + BANK_MARGIN))
                else {
                    continue;
                };
                let ground = nav.ground(c) + 0.1;
                for l in 0..LEVELS {
                    if nav.floor[i][l] < ground.max(y + 1.0) {
                        nav.blocked[i][l] = true;
                    }
                }
            }
        }
        // Off the sides of the ramps up onto decks: one's driven up from its
        // foot, not over the edge with a track hanging off it.
        for r in &ramps {
            let reach = Vec2::splat(r.half.length() + RAIL_CLEARANCE);
            let (_, az) = r.axes();
            let side = az.perp();
            nav.mark(r.center - reach, r.center + reach, |c, f, _| {
                let l = r.local_point(c);
                if l.y.abs() > r.half.y * r.tilt.cos()
                    || (l.x.abs() - r.half.x).abs() > RAIL_CLEARANCE
                {
                    return false;
                }
                let edge = |x: f32| r.center + side * x + az * l.y;
                // Which way across `side` goes from the ramp's middle.
                let e = if (c - r.center).dot(side) < 0.0 {
                    -1.0
                } else {
                    1.0
                };
                let on = slab_top(r, l.y);
                let p = edge(e * (r.half.x + 0.3));
                let off = map.ground_height(p.x, p.y);
                on - off > KERB && (f - on).abs() < STEP
            });
        }
        // Floors too steep to drive up (cliffs, steep banks, a deck's edge).
        for i in 0..n * n {
            let c = nav.center(i);
            for l in 0..LEVELS {
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
        // Where it's plainly level all round, there's nothing to keep back
        // from.
        let plain: Vec<bool> = (0..n * n)
            .map(|i| nav.solid.is_some() && nav.level_round(i))
            .collect();
        // Back from rock standing up out of a floor: a cliff, a dig's walls.
        if nav.solid.is_some() {
            for i in 0..n * n {
                if plain[i] {
                    continue;
                }
                let c = nav.center(i);
                for l in 0..LEVELS {
                    let h = nav.floor[i][l];
                    if h.is_nan() || nav.blocked[i][l] || (l == TOP && h > nav.ground(c) + 0.1) {
                        continue;
                    }
                    nav.blocked[i][l] = (0..8).any(|k| {
                        let d = Vec2::from_angle(k as f32 * std::f32::consts::TAU / 8.0);
                        nav.surface_near(c + d * CLEARANCE, h) > h + CLEARANCE * MAX_SLOPE + 1.0
                    });
                }
            }
        }
        // Back from drops.
        for i in 0..n * n {
            if plain[i] {
                continue;
            }
            let c = nav.center(i);
            for l in 0..LEVELS {
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
                for l in 0..LEVELS {
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

    /// Whether the cells round cell `i` are all one floor, at about one
    /// height.
    fn level_round(&self, i: usize) -> bool {
        let n = self.n;
        let (x, z) = (i % n, i / n);
        let r = 2;
        if x < r || z < r || x + r >= n || z + r >= n {
            return false;
        }
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for z in z - r..=z + r {
            for f in &self.floor[z * n + x - r..=z * n + x + r] {
                if !f[TOP + 1].is_nan() {
                    return false;
                }
                (lo, hi) = (lo.min(f[TOP]), hi.max(f[TOP]));
            }
        }
        hi - lo < 0.4
    }

    fn ground(&self, p: Vec2) -> f32 {
        let solid = self.solid.as_ref().and_then(|s| s.top(p));
        let g =
            solid.unwrap_or_else(|| self.terrain.as_ref().map_or(0.0, |t| t.height_at(p.x, p.y)));
        if self.rivers.is_empty() {
            return g;
        }
        let Some(&Some((k, seg))) = self.wet.get(self.cell(p)) else {
            return g;
        };
        // (Only the stretch by this cell.)
        let (r, seg) = (&self.rivers[k as usize], seg as usize);
        let by = seg.saturating_sub(3)..seg + 5;
        r.surface_in(p, RIVER_MARGIN, by)
            .map_or(g, |(y, _)| g.max(y))
    }

    /// The floors at a point, the highest first (NaN past the last): those
    /// with room over them to drive.
    fn floors_at(&self, p: Vec2) -> [f32; LEVELS] {
        let mut out = [f32::NAN; LEVELS];
        let mut n = 0;
        let mut put = |f: f32| {
            if n < LEVELS {
                out[n] = f;
                n += 1;
            }
        };
        // The ground's own floors, the highest first.
        let mut dug = [f32::NAN; 2 * LEVELS];
        let mut m = 0;
        if let Some(solid) = &self.solid {
            for f in solid.over(p) {
                let k = dug[..m].iter().position(|&d| f > d - 1e-3).unwrap_or(m);
                if k < m && f <= dug[k] + 1e-3 {
                    continue;
                }
                if m < dug.len() {
                    m += 1;
                }
                if k < m {
                    dug.copy_within(k..m - 1, k + 1);
                    dug[k] = f;
                }
            }
        }
        let g = if m > 0 { dug[0] } else { self.ground(p) };
        // Decks and roofs laid over the ground.
        let (mut top, mut under) = (g, false);
        for s in &self.slabs {
            let l = s.local_point(p);
            if l.x.abs() > s.half.x || l.y.abs() > s.half.y {
                continue;
            }
            under |= !s.is_turned() && s.y0 >= g + HEADROOM;
            top = top.max(slab_top(s, l.y));
        }
        put(top);
        if under && top > g {
            put(g);
        }
        // What's dug out under the ground.
        let mut over = g;
        for &f in &dug[1.min(m)..m] {
            if over - f >= HEADROOM {
                put(f);
                over = f;
            }
        }
        out
    }

    /// The floor at a point nearest height `h`.
    fn surface_near(&self, p: Vec2, h: f32) -> f32 {
        nearest(&self.floors_at(p), h).1
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
        if y.is_finite() {
            nearest(&self.floor[i], y).0
        } else {
            TOP
        }
    }

    /// The open floor of cell `i` within `step` of height `h`, nearest it.
    fn step_to(&self, i: usize, h: f32, step: f32) -> Option<usize> {
        (0..LEVELS)
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
                        for l in 0..LEVELS {
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

    /// The open floor at `p` about its height, if there's one (tests, and
    /// generated maps settling their ways round onto it).
    pub fn floor_at(&self, p: Vec3) -> Option<f32> {
        let i = self.cell(Vec2::new(p.x, p.z));
        (0..LEVELS)
            .map(|l| (l, self.floor[i][l]))
            .find(|&(l, f)| !f.is_nan() && !self.blocked[i][l] && (f - p.y).abs() < LEVEL_TOLERANCE)
            .map(|(_, f)| f)
    }

    /// Open ground right by `p` on its top floor (spawn checks).
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

    /// Is this floor a deck's (or one of its ramps')?
    fn on_deck(&self, (i, l): Spot) -> bool {
        l == TOP && self.floor[i][l] > self.ground(self.center(i)) + 0.1
    }

    /// Is `p` (a tank's position) over a deck rather than the ground?
    pub fn over_deck(&self, p: Vec3) -> bool {
        let i = self.cell(Vec2::new(p.x, p.z));
        self.on_deck((i, self.level(i, p.y)))
    }

    /// A deck's cell with one beside it that can't be driven.
    fn by_decks_edge(&self, s: Spot) -> bool {
        let n = self.n as i32;
        let (x0, z0) = ((s.0 % self.n) as i32, (s.0 / self.n) as i32);
        let h = self.floor[s.0][s.1];
        self.on_deck(s)
            && (-1..=1).any(|dz| {
                (-1..=1).any(|dx| {
                    let (x, z) = (x0 + dx, z0 + dz);
                    let inside = (0..n).contains(&x) && (0..n).contains(&z);
                    !inside || self.step_to((z * n + x) as usize, h, STEP * 1.5).is_none()
                })
            })
    }

    /// Driving straight from `a` to `b`, wherever that's on a deck it keeps
    /// `DECK_MARGIN` in from what can't be driven along the deck's edges.
    fn off_rails(&self, a: Vec3, b: Vec2) -> bool {
        let a2 = Vec2::new(a.x, a.z);
        let Some(side) = (b - a2).try_normalize().map(|d| d.perp() * DECK_MARGIN) else {
            return true;
        };
        let steps = (a2.distance(b) / (NAV_CELL * 0.5)).ceil().max(1.0) as usize;
        let mut h = a.y;
        (0..=steps).all(|k| {
            let p = a2.lerp(b, k as f32 / steps as f32);
            let i = self.cell(p);
            let l = self.level(i, h);
            h = self.floor[i][l];
            !self.on_deck((i, l))
                || [-1.0, 1.0].iter().all(|&e| {
                    self.step_to(self.cell(p + side * e), h, STEP * 1.5)
                        .is_some()
                })
        })
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
        let id = |(i, l): Spot| i * LEVELS + l;
        let spot = |k: usize| (k / LEVELS, k % LEVELS);
        let (gx, gz) = ((goal.0 % n) as f32, (goal.0 / n) as f32);
        let h = |s: Spot| {
            let (dx, dz) = (((s.0 % n) as f32 - gx).abs(), ((s.0 / n) as f32 - gz).abs());
            dx.max(dz) + (std::f32::consts::SQRT_2 - 1.0) * dx.min(dz)
        };
        let mut g = vec![f32::INFINITY; n * n * LEVELS];
        let mut came = vec![usize::MAX; n * n * LEVELS];
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
                let edge = if self.by_decks_edge((j, l)) {
                    EDGE_COST
                } else {
                    0.0
                };
                let cost = gc + edge + if diag { std::f32::consts::SQRT_2 } else { 1.0 };
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
            let to = Vec2::new(b.x, b.z);
            self.walk(a, to).is_some_and(|h| (h - b.y).abs() < 0.5) && self.off_rails(a, to)
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

#[cfg(test)]
mod dig_tests {
    use super::*;
    use crate::map::procgen::land::{Dig, Hollow, Land, Slab};

    /// A bot drives into a dig, through it and out of its other mouth, and
    /// another drives over the top of it.
    #[test]
    fn bots_drive_through_a_dig_and_over_it() {
        let line = vec![Vec2::new(20.0, 40.0), Vec2::new(160.0, 90.0)];
        let into = (line[1] - line[0]).normalize().perp();
        let room = |x0: f32, y0: f32, x1: f32, y1: f32| Hollow {
            min: Vec2::new(x0, y0),
            max: Vec2::new(x1, y1),
            floor: 0.0,
            roof: 6.0,
        };
        let mut land = Land::default();
        land.high.height = 12.0;
        land.high.slabs = vec![Slab::Behind { line: line.clone() }];
        land.digs = vec![Dig {
            at: line[0].lerp(line[1], 0.5),
            facing: into,
            floor: 0.0,
            // Two ways in, joined by a hall.
            rooms: vec![
                room(-25.0, -6.0, -15.0, 34.0),
                room(15.0, -6.0, 25.0, 34.0),
                room(-25.0, 20.0, 25.0, 34.0),
            ],
            pillars: Vec::new(),
            beam: None,
            mouths: Vec::new(),
            through: Vec::new(),
        }];
        let mut map = Map::empty(180.0);
        map.terrain = Some(Terrain::from_fn(Vec2::ZERO, 2.0, 91, 91, |_, _| 0.0));
        map.solid = Some(land.surface(180.0, 1.0, |_| 0.0).into());
        let nav = NavGrid::build(&map);
        let at = |x: f32, y: f32, h: f32| {
            let p = land.digs[0].to_map(Vec2::new(x, y));
            Vec3::new(p.x, h, p.y)
        };
        let through = nav.route(at(-20.0, -12.0, 0.0), at(0.0, 27.0, 0.0));
        assert!(!through.is_empty(), "no way into the hall");
        assert!(through.iter().all(|p| p.y < 1.0), "{through:?}");
        let out = nav.route(at(0.0, 27.0, 0.0), at(20.0, -12.0, 0.0));
        assert!(!out.is_empty() && out.iter().all(|p| p.y < 1.0), "{out:?}");
        let over = nav.route(at(-20.0, 27.0, 12.0), at(20.0, 27.0, 12.0));
        assert!(
            !over.is_empty() && over.iter().all(|p| p.y > 11.0),
            "{over:?}"
        );
        // Not up the cliff, nor through the rock.
        assert!(
            nav.route(at(0.0, 27.0, 0.0), at(0.0, 50.0, 12.0))
                .is_empty()
        );
        assert!(!nav.clear_from(
            at(-20.0, 10.0, 0.0),
            land.digs[0].to_map(Vec2::new(20.0, 10.0))
        ));
    }
}
