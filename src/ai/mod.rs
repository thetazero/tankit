//! Bot opponents: a coarse navigation grid over the map, A* paths through it,
//! and a small brain per bot. It picks a target (near, in sight, already
//! hurt, in reach of its gun), drives the path to it, and fights the way its
//! gun wants: ranged guns hold their distance and circle, the flamethrower
//! drives straight in and stays nose-on, a broken Vigil spends its last
//! seconds going for the throat. It leads its shots by where the target
//! will be (its own motion included), lays the turret against the hull's
//! turn, waits for a rocking gun to come level, keeps a spin-up gun turning,
//! rings a bell up with the hull in line so the toll can't roll it over, and
//! goes for a repair crate when it's hurt. Given an objective (the Hold
//! site), bots make for it, fight whoever comes near it, and patrol inside
//! the ring.
//!
//! Bots drive through the same `Command` as the player; they get no physics
//! shortcuts, and see what a player would: tanks, crates, their own gun.
//!
//! `baseline` keeps the bots as they were before, to measure against
//! (`--arena`, `--duel`).

pub mod baseline;

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use glam::{Vec2, Vec3};

use crate::map::Map;
use crate::rng::Rng;
use crate::sim::{Command, Tank, TurretKind, World, wrap_angle, yaw_of};

/// Nav cell size, m. Fine enough that narrow gaps between cover show up.
const NAV_CELL: f32 = 2.0;
/// How far obstacles are grown so paths keep a hull's width off walls.
const CLEARANCE: f32 = 2.2;
/// Bots don't see further than this.
pub const SIGHT: f32 = 130.0;
/// Bots open fire inside this range.
const FIRE_RANGE: f32 = 110.0;
/// Guns shorter-ranged than this brawl: they charge in to fight.
const BRAWL_RANGE: f32 = 40.0;
/// Below this share of health, ranged bots fight from further out.
const WOUNDED: f32 = 0.35;
/// With an objective, bots leave it to chase an enemy only this close.
const GUARD_CHASE: f32 = 40.0;
/// Below this share of health, bots go for a repair crate within reach...
const MEND_BELOW: f32 = 0.65;
/// ...this far off, m.
const MEND_REACH: f32 = 70.0;
/// A bouncing gun banks a shot off a wall at a target out of sight (its
/// health bar shows through walls, to a player too) at most this far off, m.
const BANK_REACH: f32 = 60.0;

/// Steepest ground (rise over run) bots will path across.
const MAX_SLOPE: f32 = 0.7;
/// On a mesa, bots keep this far back from the edge, m.
const EDGE_MARGIN: f32 = 7.0;

pub struct NavGrid {
    nx: usize,
    nz: usize,
    blocked: Vec<bool>,
}

impl NavGrid {
    pub fn build(map: &Map) -> NavGrid {
        let n = (map.size / NAV_CELL).ceil() as usize;
        let mut blocked = vec![false; n * n];
        for s in &map.solids {
            // Anything a tank can't drive over: tall, and starting near the ground.
            if s.max.y - s.min.y < 0.6 || s.min.y > 1.0 {
                continue;
            }
            let (lo, hi) = (s.min2() - CLEARANCE, s.max2() + CLEARANCE);
            let c0 = ((lo / NAV_CELL).floor().max(Vec2::ZERO)).as_uvec2();
            let c1 = (hi / NAV_CELL)
                .ceil()
                .as_uvec2()
                .min(glam::UVec2::splat(n as u32));
            for iz in c0.y as usize..c1.y as usize {
                for ix in c0.x as usize..c1.x as usize {
                    let c = Vec2::new(ix as f32 + 0.5, iz as f32 + 0.5) * NAV_CELL;
                    if c.cmpge(lo).all() && c.cmple(hi).all() {
                        blocked[iz * n + ix] = true;
                    }
                }
            }
        }
        for o in &map.oboxes {
            // Debris and drivable ramps don't block; neither does anything low or overhead.
            // (A tall leaning spire is tilted like a ramp, but it's a wall.)
            // Measured as the piece actually stands (tilted, rolled): a
            // standing post is tall and narrow, not a beam lying flat.
            let (y0, y1) = o.y_range();
            if !o.solid
                || (o.is_ramp() && o.y1 - o.y0 < 1.5)
                || y1 - y0 < 0.6
                || y0 > map.ground_height(o.center.x, o.center.y) + 1.0
            {
                continue;
            }
            let reach = o.half.length().max((y1 - y0) * 0.5) + CLEARANCE;
            // What's under a tank's height from the piece's lowest point.
            const TANK_HEIGHT: f32 = 3.0;
            let (lo, hi) = (o.center - Vec2::splat(reach), o.center + Vec2::splat(reach));
            let c0 = ((lo / NAV_CELL).floor().max(Vec2::ZERO)).as_uvec2();
            let c1 = (hi / NAV_CELL)
                .ceil()
                .as_uvec2()
                .min(glam::UVec2::splat(n as u32));
            for iz in c0.y as usize..c1.y as usize {
                for ix in c0.x as usize..c1.x as usize {
                    let c = Vec2::new(ix as f32 + 0.5, iz as f32 + 0.5) * NAV_CELL;
                    if o.base_distance(c, TANK_HEIGHT) <= CLEARANCE {
                        blocked[iz * n + ix] = true;
                    }
                }
            }
        }
        // Crystals: wherever they stand low enough to hit.
        for c in map.crystals.iter().filter(|c| c.solid) {
            let g = map.ground_height(c.base.x, c.base.z);
            for (p, r) in c.footprint(g, 3.0) {
                let reach = r + CLEARANCE;
                let c0 = (((p - Vec2::splat(reach)) / NAV_CELL)
                    .floor()
                    .max(Vec2::ZERO))
                .as_uvec2();
                let c1 = ((p + Vec2::splat(reach)) / NAV_CELL)
                    .ceil()
                    .as_uvec2()
                    .min(glam::UVec2::splat(n as u32));
                for iz in c0.y as usize..c1.y as usize {
                    for ix in c0.x as usize..c1.x as usize {
                        let cc = Vec2::new(ix as f32 + 0.5, iz as f32 + 0.5) * NAV_CELL;
                        if cc.distance(p) <= reach {
                            blocked[iz * n + ix] = true;
                        }
                    }
                }
            }
        }
        // Ground too steep to drive up (cliffs, steep banks).
        if let Some(t) = &map.terrain {
            for iz in 0..n {
                for ix in 0..n {
                    let c = Vec2::new(ix as f32 + 0.5, iz as f32 + 0.5) * NAV_CELL;
                    let h = |d: Vec2| t.height_at(c.x + d.x, c.y + d.y);
                    let e = NAV_CELL * 0.5;
                    let gx = (h(Vec2::new(e, 0.0)) - h(Vec2::new(-e, 0.0))) / (2.0 * e);
                    let gz = (h(Vec2::new(0.0, e)) - h(Vec2::new(0.0, -e))) / (2.0 * e);
                    if Vec2::new(gx, gz).length() > MAX_SLOPE {
                        blocked[iz * n + ix] = true;
                    }
                }
            }
        }
        // A mesa: keep well back from the edge, wherever the ground drops
        // away within reach.
        if let Some(fall_y) = map.fall_y {
            let drop = fall_y * 0.3;
            for iz in 0..n {
                for ix in 0..n {
                    let c = Vec2::new(ix as f32 + 0.5, iz as f32 + 0.5) * NAV_CELL;
                    let near_edge = (0..12).any(|k| {
                        let a = k as f32 / 12.0 * std::f32::consts::TAU;
                        let p = c + Vec2::new(a.cos(), a.sin()) * EDGE_MARGIN;
                        map.ground_height(p.x, p.y) < drop
                    });
                    if near_edge || map.ground_height(c.x, c.y) < drop {
                        blocked[iz * n + ix] = true;
                    }
                }
            }
        }
        NavGrid {
            nx: n,
            nz: n,
            blocked,
        }
    }

    fn cell(&self, p: Vec2) -> (usize, usize) {
        let c = (p / NAV_CELL).floor();
        (
            c.x.clamp(0.0, self.nx as f32 - 1.0) as usize,
            c.y.clamp(0.0, self.nz as f32 - 1.0) as usize,
        )
    }

    fn center(&self, (ix, iz): (usize, usize)) -> Vec2 {
        Vec2::new(ix as f32 + 0.5, iz as f32 + 0.5) * NAV_CELL
    }

    pub fn is_blocked(&self, p: Vec2) -> bool {
        let (ix, iz) = self.cell(p);
        self.blocked[iz * self.nx + ix]
    }

    /// Nearest open cell to `p` (search in growing rings).
    fn nearest_open(&self, p: Vec2) -> Option<(usize, usize)> {
        let (cx, cz) = self.cell(p);
        for r in 0..12i32 {
            let mut best: Option<((usize, usize), f32)> = None;
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dz.abs() != r {
                        continue;
                    }
                    let (x, z) = (cx as i32 + dx, cz as i32 + dz);
                    if x < 0 || z < 0 || x >= self.nx as i32 || z >= self.nz as i32 {
                        continue;
                    }
                    let c = (x as usize, z as usize);
                    if !self.blocked[c.1 * self.nx + c.0] {
                        let d = self.center(c).distance_squared(p);
                        if best.is_none_or(|(_, bd)| d < bd) {
                            best = Some((c, d));
                        }
                    }
                }
            }
            if let Some((c, _)) = best {
                return Some(c);
            }
        }
        None
    }

    /// Straight drive from a to b stays in open cells.
    pub fn clear_line(&self, a: Vec2, b: Vec2) -> bool {
        let steps = (a.distance(b) / (NAV_CELL * 0.5)).ceil().max(1.0) as usize;
        (0..=steps).all(|i| !self.is_blocked(a.lerp(b, i as f32 / steps as f32)))
    }

    /// Waypoints from `from` to `to` (excluding the start), string-pulled so
    /// bots cut straight across open ground. Empty if unreachable.
    pub fn path(&self, from: Vec2, to: Vec2) -> Vec<Vec2> {
        let (Some(start), Some(goal)) = (self.nearest_open(from), self.nearest_open(to)) else {
            return Vec::new();
        };
        let idx = |c: (usize, usize)| c.1 * self.nx + c.0;
        let h = |c: (usize, usize)| {
            let (dx, dz) = (
                (c.0 as f32 - goal.0 as f32).abs(),
                (c.1 as f32 - goal.1 as f32).abs(),
            );
            dx.max(dz) + (std::f32::consts::SQRT_2 - 1.0) * dx.min(dz)
        };
        let mut g = vec![f32::INFINITY; self.nx * self.nz];
        let mut came = vec![usize::MAX; self.nx * self.nz];
        let mut open = BinaryHeap::new();
        g[idx(start)] = 0.0;
        open.push(Node {
            f: h(start),
            cell: start,
        });
        let mut found = false;
        while let Some(Node { cell, f }) = open.pop() {
            if cell == goal {
                found = true;
                break;
            }
            let gc = g[idx(cell)];
            if f > gc + h(cell) + 1e-3 {
                continue;
            }
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
                let (x, z) = (cell.0 as i32 + dx, cell.1 as i32 + dz);
                if x < 0 || z < 0 || x >= self.nx as i32 || z >= self.nz as i32 {
                    continue;
                }
                let n = (x as usize, z as usize);
                if self.blocked[idx(n)] {
                    continue;
                }
                // No cutting corners past a blocked cell.
                if dx != 0 && dz != 0 {
                    let a = (cell.0 as i32 + dx) as usize + cell.1 * self.nx;
                    let b = cell.0 + (cell.1 as i32 + dz) as usize * self.nx;
                    if self.blocked[a] || self.blocked[b] {
                        continue;
                    }
                }
                let cost = gc
                    + if dx != 0 && dz != 0 {
                        std::f32::consts::SQRT_2
                    } else {
                        1.0
                    };
                if cost < g[idx(n)] {
                    g[idx(n)] = cost;
                    came[idx(n)] = idx(cell);
                    open.push(Node {
                        f: cost + h(n),
                        cell: n,
                    });
                }
            }
        }
        if !found {
            return Vec::new();
        }
        let mut cells = vec![goal];
        let mut at = idx(goal);
        while at != idx(start) {
            at = came[at];
            cells.push((at % self.nx, at / self.nx));
        }
        cells.reverse();
        // String pulling: from each kept point, skip ahead as far as a straight line stays open.
        let pts: Vec<Vec2> = cells.iter().map(|&c| self.center(c)).collect();
        let mut out = Vec::new();
        let mut i = 0;
        let mut from_pt = from;
        while i + 1 < pts.len() {
            let mut j = pts.len() - 1;
            while j > i + 1 && !self.clear_line(from_pt, pts[j]) {
                j -= 1;
            }
            out.push(pts[j]);
            from_pt = pts[j];
            i = j;
        }
        if out.is_empty() {
            out.push(self.center(goal));
        }
        out
    }
}

#[derive(PartialEq)]
struct Node {
    f: f32,
    cell: (usize, usize),
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

/// What a bot may know about every tank in the match.
#[derive(Clone, Copy)]
pub struct Contact {
    /// Alive and not spawn-protected: worth shooting at.
    pub targetable: bool,
    /// Which side it's on; bots only hunt the other one.
    pub team: usize,
    /// Share of full health left, 0..1.
    pub health: f32,
}

/// The distance a gun is best fought at, m: where it hits hard and reliably
/// but the enemy isn't on top of it.
pub fn ideal_range(kind: TurretKind) -> f32 {
    match kind {
        // Rail cannon: lands instantly at any range; keep well back.
        TurretKind::Kontos => 75.0,
        // Light, fast twin guns: middle distance.
        TurretKind::Pelekys => 45.0,
        // Slow orbs: close enough that they're hard to sidestep (from much
        // further a strafing tank is long gone by the time they get there).
        TurretKind::Ammeth => 20.0,
        // Spraying darts: they fly slow enough to dodge from far off too.
        TurretKind::Ossrin => 20.0,
        // The beam is at full strength out to 40% of its range.
        TurretKind::Lumen => 55.0,
        // Flamethrower: right up against them.
        TurretKind::Hymn => 8.0,
        // Siege bell: far enough back that it isn't a sitting duck while it
        // rings up; its toll still carries.
        TurretKind::Knell => 85.0,
    }
}

/// A bot's brain: the current one, or the frozen baseline it's measured against.
pub enum Brain {
    Bot(Bot),
    Baseline(baseline::Bot),
}

impl Brain {
    #[allow(clippy::too_many_arguments)]
    pub fn think(
        &mut self,
        me: usize,
        world: &World,
        contacts: &[Contact],
        objective: Option<(Vec2, f32)>,
        crates: &[Vec2],
        nav: &NavGrid,
        dt: f32,
    ) -> (Command, bool) {
        match self {
            Brain::Bot(b) => b.think(me, world, contacts, objective, crates, nav, dt),
            Brain::Baseline(b) => b.think(me, world, contacts, objective, nav, dt),
        }
    }

    pub fn reset(&mut self) {
        match self {
            Brain::Bot(b) => b.reset(),
            Brain::Baseline(b) => b.reset(),
        }
    }

    pub fn fired(&mut self) {
        match self {
            Brain::Bot(b) => b.fired(),
            Brain::Baseline(b) => b.fired(),
        }
    }
}

/// One bot's memory between steps.
pub struct Bot {
    rng: Rng,
    path: Vec<Vec2>,
    replan_in: f32,
    target: Option<usize>,
    retarget_in: f32,
    /// How long the current target has been in sight (reaction time before firing).
    seen_for: f32,
    last_seen: Option<Vec2>,
    /// A bouncing gun's bank shot at a target out of sight: the heading to
    /// fire along, off a wall, and when to look for one again.
    bank: Option<f32>,
    bank_in: f32,
    /// Aim offset re-rolled after every shot, radians.
    aim_err: f32,
    stuck_for: f32,
    reverse_for: f32,
    reverse_steer: f32,
    /// Side-to-side jink while fighting.
    jink: f32,
    jink_in: f32,
    /// Holding at range and circling the target (ranged guns), rather than closing in.
    engaged: bool,
    /// Which way round the target it circles, ±1.
    strafe: f32,
    strafe_in: f32,
    /// Driving forwards (1) or backwards (-1) while circling.
    drive_sign: f32,
    /// Angle off the straight line to the target it approaches along, so
    /// bots hunting one enemy come at it from different sides.
    flank: f32,
    roam: Option<Vec2>,
    /// Patrol point inside the objective's ring.
    post: Option<Vec2>,
    /// Path searches run so far (each one can cost a whole-grid search).
    plans: u32,
    /// Hull heading last step and how fast it's turning, rad/s: the turret
    /// rides on the hull, so aiming has to take its turn out.
    last_yaw: Option<f32>,
    hull_rate: f32,
}

impl Bot {
    pub fn new(seed: u64) -> Bot {
        let mut rng = Rng::new(seed);
        let jink = if rng.chance(0.5) { 1.0 } else { -1.0 };
        let strafe = if rng.chance(0.5) { 1.0 } else { -1.0 };
        let flank = rng.range(-0.9, 0.9);
        Bot {
            rng,
            path: Vec::new(),
            replan_in: 0.0,
            target: None,
            retarget_in: 0.0,
            seen_for: 0.0,
            last_seen: None,
            bank: None,
            bank_in: 0.0,
            aim_err: 0.0,
            stuck_for: 0.0,
            reverse_for: 0.0,
            reverse_steer: 1.0,
            jink,
            jink_in: 2.0,
            engaged: false,
            strafe,
            strafe_in: 2.0,
            drive_sign: 1.0,
            flank,
            roam: None,
            post: None,
            plans: 0,
            last_yaw: None,
            hull_rate: 0.0,
        }
    }

    /// Forget everything (after dying).
    pub fn reset(&mut self) {
        self.path.clear();
        self.target = None;
        self.last_seen = None;
        self.bank = None;
        self.seen_for = 0.0;
        self.stuck_for = 0.0;
        self.reverse_for = 0.0;
        self.roam = None;
        self.post = None;
        self.engaged = false;
        self.last_yaw = None;
        self.hull_rate = 0.0;
        self.flank = self.rng.range(-0.9, 0.9);
    }

    /// Decide this step's controls for tank `me`, with an optional objective
    /// ring (center, radius) to hold and the repair crates in sight (falling
    /// or down). Returns (command, wants to fire).
    #[allow(clippy::too_many_arguments)]
    pub fn think(
        &mut self,
        me: usize,
        world: &World,
        contacts: &[Contact],
        objective: Option<(Vec2, f32)>,
        crates: &[Vec2],
        nav: &NavGrid,
        dt: f32,
    ) -> (Command, bool) {
        let t = &world.tanks[me];
        let g = crate::combat::gun(t.turret_kind);
        // A broken Vigil has a few seconds left whatever it does: it spends
        // them going for the throat.
        let husk = t.husk.is_some();
        // Short-range guns (the flamethrower) have to close in before they
        // stop to fight. Everything else picks a distance that suits it and
        // fights from there, further out when hurt.
        let brawler = g.range() < BRAWL_RANGE || husk;
        let reach = FIRE_RANGE.min(g.range());
        let mut ideal = ideal_range(t.turret_kind).min(reach * 0.8);
        if !brawler && contacts[me].health < WOUNDED {
            ideal = (ideal * 1.35).min(reach * 0.9);
        }
        let standoff = if husk {
            g.range() * 0.5
        } else {
            ideal.max(g.range() * 0.6).min(reach)
        };
        let my2 = flat(t.pos);
        let eye = t.pos + Vec3::Y * t.gun_height();
        let banks = matches!(g.action, crate::combat::Action::Shell(s) if s.bounces > 0);

        // How fast the hull is swinging round (the turret rides on it).
        let yaw = t.yaw();
        if let Some(y) = self.last_yaw {
            self.hull_rate = wrap_angle(yaw - y) / dt.max(1e-4);
        }
        self.last_yaw = Some(yaw);

        // Target: the enemy it can do the most about, the nearest in sight,
        // weighed towards ones already hurt (finish them) and ones its gun
        // reaches. Sticky for a moment.
        self.retarget_in -= dt;
        let stale = self.target.is_some_and(|j| !contacts[j].targetable);
        if self.retarget_in <= 0.0 || stale {
            self.retarget_in = 0.4;
            let mut best: Option<(usize, f32)> = None;
            for (j, o) in world.tanks.iter().enumerate() {
                if j == me || !contacts[j].targetable || contacts[j].team == contacts[me].team {
                    continue;
                }
                let d = t.pos.distance(o.pos);
                let mut score = d;
                if !can_see(world, eye, o) {
                    // Out of sight is out of reach, unless the gun can
                    // bank a shot round to it.
                    score += if banks && d < BANK_REACH { 25.0 } else { 120.0 };
                }
                if Some(j) == self.target {
                    score -= 15.0;
                }
                score += contacts[j].health * 30.0;
                score += (d - reach).max(0.0) * 0.5;
                // Holding: the enemies that matter are the ones on or near the site.
                if let Some((c, r)) = objective {
                    score += (flat(o.pos).distance(c) - r).max(0.0) * 0.6;
                }
                if best.is_none_or(|(_, s)| score < s) {
                    best = Some((j, score));
                }
            }
            let new = best.map(|(j, _)| j);
            if new != self.target {
                self.seen_for = 0.0;
                self.replan_in = 0.0;
            }
            self.target = new;
        }

        let target = self.target.map(|j| &world.tanks[j]);
        let visible =
            target.is_some_and(|o| t.pos.distance(o.pos) < SIGHT && can_see(world, eye, o));
        if visible {
            self.seen_for += dt;
            self.last_seen = target.map(|o| flat(o.pos));
        } else {
            self.seen_for = 0.0;
        }

        // In a fight at range: stop closing once inside the gun's band, and
        // only give it up if the target gets well out of it or out of sight.
        let target_d = target.map_or(f32::INFINITY, |o| flat(o.pos).distance(my2));
        self.engaged = !brawler
            && visible
            && target_d < reach
            && target_d < ideal * if self.engaged { 1.5 } else { 1.2 };

        // Hurt and not in the thick of it: a repair crate nearby is worth
        // the drive.
        let mend = (!husk && contacts[me].health < MEND_BELOW && !(visible && target_d < 25.0))
            .then(|| {
                crates
                    .iter()
                    .copied()
                    .filter(|c| c.distance(my2) < MEND_REACH)
                    .min_by(|a, b| a.distance(my2).total_cmp(&b.distance(my2)))
            })
            .flatten();
        if mend.is_some() {
            self.engaged = false;
        }

        // Where a ranged bot goes to fight `o` from: at its ideal range, on
        // its own side of the target, swung round by its flank angle. Once
        // there with no sight of it, straight at it to find it.
        let flank = self.flank;
        let approach = |o: Vec2| -> Vec2 {
            if brawler {
                return o;
            }
            let back = (my2 - o).try_normalize().unwrap_or(Vec2::X);
            let spot = o + Vec2::from_angle(flank).rotate(back) * ideal * 0.9;
            if !visible && spot.distance(my2) < 10.0 {
                o
            } else {
                spot
            }
        };

        // Where to drive: to fight the target (or where it was last seen),
        // else roam. With an objective: to the site, leaving it only for
        // enemies close by or on it.
        let goal = match (mend, objective, target) {
            (Some(c), _, _) => c,
            (None, Some(_), Some(o)) if husk => flat(o.pos),
            (None, Some((c, r)), t) => {
                let chase = t.map(|o| flat(o.pos)).filter(|&p| {
                    visible && (p.distance(my2) < GUARD_CHASE || p.distance(c) < r + 8.0)
                });
                match chase {
                    Some(p) => approach(p),
                    None => {
                        if self.post.is_some_and(|p| p.distance(my2) < 4.0) {
                            self.post = None;
                        }
                        *self.post.get_or_insert_with(|| {
                            let mut p = c;
                            for _ in 0..8 {
                                let a = self.rng.range(0.0, std::f32::consts::TAU);
                                let d = self.rng.range(0.0, r * 0.6);
                                p = c + Vec2::new(a.cos(), a.sin()) * d;
                                if !nav.is_blocked(p) {
                                    break;
                                }
                            }
                            p
                        })
                    }
                }
            }
            (None, None, Some(o)) => approach(flat(o.pos)),
            (None, None, None) => *self.roam.get_or_insert_with(|| {
                world.map.open_spots[self.rng.index(world.map.open_spots.len())]
            }),
        };
        if self.roam.is_some_and(|r| r.distance(my2) < 8.0) {
            self.roam = None;
        }

        // Replan on a timer. A failed plan (goal cut off, e.g. up on a roof)
        // waits for the timer too: each try searches the whole grid.
        self.replan_in -= dt;
        if self.replan_in <= 0.0 {
            self.replan_in = 1.0 + self.rng.f32() * 0.5;
            self.path = nav.path(my2, goal);
            self.plans += 1;
            if self.path.is_empty() {
                // Unreachable: give up on that roam spot or patrol point; head
                // straight for a target or the objective.
                self.roam = None;
                self.post = None;
                if target.is_some() || objective.is_some() || mend.is_some() {
                    self.path.push(goal);
                }
            }
        }
        while self.path.len() > 1 && self.path[0].distance(my2) < 4.0 {
            self.path.remove(0);
        }

        // Driving.
        let mut cmd = Command::default();
        let close = brawler && visible && target_d < standoff && mend.is_none();
        if let Some(&wp) = self.path.first() {
            let err = wrap_angle(yaw_of(wp - my2) - t.yaw());
            cmd.steer = (err * 2.5).clamp(-1.0, 1.0);
            cmd.throttle = if err.abs() > 1.1 {
                0.15
            } else {
                1.0 - err.abs() * 0.5
            };
        }
        // A close-range gun with its target in sight and nothing in between
        // drives straight for it (where it's going, a moment ahead), not
        // along a path planned to where it was.
        if let Some(o) = target.filter(|_| brawler && visible && !close && mend.is_none()) {
            let ahead = flat(o.pos) + flat(o.vel) * (target_d / 25.0).min(1.0);
            if nav.clear_line(my2, ahead) {
                let err = wrap_angle(yaw_of(ahead - my2) - t.yaw());
                cmd.steer = (err * 2.5).clamp(-1.0, 1.0);
                cmd.throttle = if err.abs() > 1.1 { 0.3 } else { 1.0 };
            }
        }
        if close {
            if husk || t.turret_kind == TurretKind::Hymn {
                // Point blank: stay on it, nose to it, so the flame (or the
                // last of a husk's fight) lands; ram if it comes to that.
                if let Some(o) = target {
                    let err = wrap_angle(yaw_of(flat(o.pos) - my2) - t.yaw());
                    cmd.steer = (err * 2.5).clamp(-1.0, 1.0);
                    cmd.throttle = if target_d > 6.0 { 1.0 } else { 0.3 };
                    if err.abs() > 1.2 {
                        cmd.throttle = 0.2;
                    }
                }
            } else {
                // In a fight at close range: stop advancing and jink back and forth.
                self.jink_in -= dt;
                if self.jink_in <= 0.0 {
                    self.jink = -self.jink;
                    self.jink_in = self.rng.range(1.2, 2.6);
                }
                cmd.throttle = self.jink * 0.7;
                cmd.steer *= 0.3;
            }
        }
        if let (true, Some(o)) = (self.engaged, target) {
            cmd = self.circle(t, flat(o.pos), ideal, objective, nav, dt);
        }

        // Stuck against something: back off with the wheel turned, then replan.
        if self.reverse_for > 0.0 {
            self.reverse_for -= dt;
            cmd.throttle = -1.0;
            cmd.steer = self.reverse_steer;
            if self.reverse_for <= 0.0 {
                self.replan_in = 0.0;
            }
        } else if cmd.throttle.abs() > 0.5 && t.speed().abs() < 0.8 {
            self.stuck_for += dt;
            if self.stuck_for > 1.0 {
                self.stuck_for = 0.0;
                self.reverse_for = self.rng.range(0.8, 1.4);
                self.reverse_steer = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
                if cmd.throttle < 0.0 {
                    // Was backing up into something: go forward instead.
                    self.reverse_for = -self.reverse_for;
                }
            }
        } else {
            self.stuck_for = 0.0;
        }
        if self.reverse_for < 0.0 {
            self.reverse_for += dt;
            cmd.throttle = 1.0;
            cmd.steer = self.reverse_steer;
            if self.reverse_for >= 0.0 {
                self.reverse_for = 0.0;
            }
        }

        // An enemy Knell ringing up with its bell on us: get out of its line.
        let tolling = world.tanks.iter().enumerate().any(|(j, o)| {
            if j == me || contacts[j].team == contacts[me].team || o.charge < 0.35 {
                return false;
            }
            let to = my2 - flat(o.pos);
            let d = to.length();
            d < crate::combat::gun(o.turret_kind).range()
                && wrap_angle(yaw_of(to) - o.turret_yaw()).abs() < (3.0 / d.max(1.0)).max(0.06)
                && can_see(world, o.pos + Vec3::Y * o.gun_height(), t)
        });
        if tolling && !husk {
            cmd.throttle = 1.0;
            cmd.steer = self.jink;
        }

        // On a mesa: never drive at the edge, whatever else says to (jinking,
        // backing off something). Brake, and back away from it.
        if let Some(fall_y) = world.map.fall_y {
            let dir = flat(t.forward()).normalize_or_zero() * cmd.throttle.signum();
            let ahead = my2 + dir * (4.0 + t.speed().abs() * 0.8);
            if cmd.throttle != 0.0 && world.map.ground_height(ahead.x, ahead.y) < fall_y * 0.3 {
                cmd.throttle = -cmd.throttle.signum() * 0.6;
                self.reverse_for = 0.0;
            }
        }

        // A gun whose shots skip off walls can reach a target that's just
        // ducked out of sight: off a wall beside it. Seen a moment ago and
        // still close, look for a wall to bank off; with one, stop (the
        // shot carries the tank's own motion) and take it from cover.
        let bounces = match g.action {
            crate::combat::Action::Shell(s) => s.bounces,
            _ => 0,
        };
        self.bank_in -= dt;
        match target {
            Some(o) if bounces > 0 && !visible && target_d < BANK_REACH => {
                if self.bank_in <= 0.0 {
                    self.bank_in = 0.25;
                    self.bank = bank_shot(world, t, o, g.speed());
                }
            }
            _ => self.bank = None,
        }
        if self.bank.is_some() && self.reverse_for == 0.0 {
            // Brake (a hover hull coasts on if just let go).
            cmd.throttle = (-t.speed() * 0.5).clamp(-1.0, 1.0);
            cmd.steer = 0.0;
        }

        // Aiming: lead the target by where it'll be when the shot gets there
        // (a shell flies off with the gun's own motion added), with a
        // per-shot error. A charge gun leads by its charge left as well:
        // that's when it goes.
        let mut fire = false;
        match target {
            Some(o) if visible => {
                let d = t.pos.distance(o.pos);
                let carried = if g.speed().is_finite() {
                    flat(t.vel)
                } else {
                    Vec2::ZERO
                };
                let rel = flat(o.vel) - carried;
                let wait = if g.charge > 0.0 {
                    (1.0 - t.charge) * g.charge
                } else {
                    0.0
                };
                let mut aim = flat(o.pos);
                for _ in 0..3 {
                    let flight = aim.distance(my2) / g.speed();
                    aim = flat(o.pos) + flat(o.vel) * wait + rel * flight;
                }
                let want = yaw_of(aim - my2) + self.aim_err;
                let err = wrap_angle(want - t.turret_yaw());
                cmd.turret = self.lay(t, err);
                // Tolerance scales with distance so a hull-width miss still counts as lined up.
                let tol = (1.4 / d.max(1.0)).max(0.02);
                // And up and down: a hull rocking on its springs (or
                // pitching on a slope) can have the gun at the sky or the
                // ground; the aim assist only levels it onto a tank close to
                // the line. Wait for it to come level rather than waste the shot.
                let gun = t.gun_dir();
                let drop = 0.5 * g.gravity() * (d / g.speed()).powi(2);
                let rise = (o.pos.y + o.chassis.hull_top * 0.6 + drop - t.muzzle().y) / d.max(1.0);
                // Anywhere on the tank will do; a shot that skips off the
                // ground (orbs) can still go on to land, so only high ones count.
                let off_y = gun.y / Vec2::new(gun.x, gun.z).length().max(1e-3) - rise;
                let skips = matches!(g.action, crate::combat::Action::Shell(s) if s.bounces > 0);
                let level = !g.speed().is_finite()
                    || t.turret_kind == TurretKind::Hymn
                    || if skips { off_y } else { off_y.abs() } < (1.6 / d.max(1.0)).max(0.03);
                if err.abs() < tol && level && d < reach && self.seen_for > 0.45 {
                    fire = true;
                }
                // A spin-up gun only fires fast once it's spun up, and runs
                // down when let go: keep it turning (and firing) while the
                // target's in reach and roughly in line.
                if g.spool > 0.0 && d < reach && err.abs() < tol * 3.0 + 0.05 && self.seen_for > 0.3
                {
                    fire = true;
                }
                // A charge gun has to be held through its charge: start
                // ringing it up as the gun comes onto the target, and keep
                // holding while it's in sight; the lead above puts the gun
                // where the target will be when it tolls.
                if g.charge > 0.0 {
                    let starting = t.charge == 0.0;
                    fire = if starting {
                        err.abs() < 0.3 && d < reach * 0.9 && self.seen_for > 0.3
                    } else {
                        true
                    };
                    // The toll throws its own hull back. Fired across the
                    // hull it rolls a narrow tank right over, so while it
                    // rings up the hull swings in line with the shot (either
                    // end) and holds still.
                    if fire {
                        let ahead = wrap_angle(want - t.yaw());
                        let behind = wrap_angle(ahead + std::f32::consts::PI);
                        let e = if ahead.abs() < behind.abs() {
                            ahead
                        } else {
                            behind
                        };
                        cmd.steer = (e * 2.5).clamp(-1.0, 1.0);
                        cmd.throttle = 0.0;
                    }
                }
            }
            _ if self.bank.is_some() => {
                // The heading the shot has to travel along; it flies off
                // with the tank's own motion added, so fire a little
                // across that to make it good.
                let path = self.bank.unwrap_or_default();
                let want = Vec2::new(path.sin(), path.cos());
                let v = flat(t.vel);
                let s = g.speed();
                let along = v.dot(want);
                let k = along + (along * along - v.length_squared() + s * s).max(0.0).sqrt();
                let yaw = yaw_of(want * k - v);
                let err = wrap_angle(yaw - t.turret_yaw());
                cmd.turret = self.lay(t, err);
                // Fired level, steady, and dead on the line: a bank shot
                // has no aim assist to fix it up.
                let gun = t.gun_dir();
                let level = (gun.y / Vec2::new(gun.x, gun.z).length().max(1e-3)).abs() < 0.03;
                fire = err.abs() < 0.015 && level && t.vel.length() < 4.0;
            }
            _ => {
                // Nothing in sight: point the gun where the target was last
                // seen, ready, rather than back over the hull.
                match self
                    .last_seen
                    .filter(|p| p.distance(my2) < SIGHT && target.is_some())
                {
                    Some(p) => {
                        let err = wrap_angle(yaw_of(p - my2) - t.turret_yaw());
                        cmd.turret = self.lay(t, err);
                    }
                    None => cmd.center_turret = true,
                }
            }
        }
        (cmd, fire)
    }

    /// Turret command to swing the gun through `err` radians and stop
    /// there: as fast as it can still brake in time, minus the hull's own
    /// turn (the turret rides on it).
    fn lay(&self, t: &Tank, err: f32) -> f32 {
        let h = t.hull;
        let rate = err.signum() * (2.0 * h.turret_brake * err.abs()).sqrt() * 0.8 + err * 4.0;
        let want = rate.clamp(-h.turret_speed, h.turret_speed) - self.hull_rate;
        let c = want / h.turret_speed;
        // The motor ignores the faintest touch: nudge past that when there's
        // still an angle to close.
        if err.abs() > 0.004 && c.abs() < 0.06 {
            0.06 * c.signum()
        } else {
            c.clamp(-1.0, 1.0)
        }
    }

    /// Controls for holding a ranged fight with a target at `tp`: slide
    /// sideways round it (hard to hit, never parked), backing off when it
    /// gets too close and easing in when it drifts too far. With an
    /// objective, it leans back towards the site when it strays off it.
    fn circle(
        &mut self,
        t: &Tank,
        tp: Vec2,
        ideal: f32,
        objective: Option<(Vec2, f32)>,
        nav: &NavGrid,
        dt: f32,
    ) -> Command {
        let my2 = flat(t.pos);
        let to = tp - my2;
        let d = to.length();
        let to = to / d.max(1e-3);
        let radial = if d < ideal * 0.7 {
            -1.2
        } else if d > ideal * 1.1 {
            0.6
        } else {
            0.0
        };
        let home = objective
            .filter(|&(c, r)| c.distance(my2) > r * 0.8)
            .map_or(Vec2::ZERO, |(c, _)| (c - my2).normalize_or_zero() * 0.8);

        self.strafe_in -= dt;
        if self.strafe_in <= 0.0 {
            self.strafe = -self.strafe;
            self.strafe_in = self.rng.range(1.8, 4.0);
        }
        let dir_for = |s: f32| (to.perp() * s + to * radial + home).normalize_or_zero();
        let open = |dir: Vec2| dir != Vec2::ZERO && nav.clear_line(my2, my2 + dir * 7.0);
        let mut dir = dir_for(self.strafe);
        if !open(dir) {
            // Wall that way: slide the other way round instead.
            if open(dir_for(-self.strafe)) {
                self.strafe = -self.strafe;
                self.strafe_in = self.rng.range(1.8, 4.0);
                dir = dir_for(self.strafe);
            } else if open(-to) && d < ideal {
                dir = -to;
            } else {
                // Boxed in: sit and shoot.
                return Command::default();
            }
        }

        // Tracks drive either way: take whichever end of the hull is nearer
        // the way to go, with some slack so it doesn't dither between them.
        let ahead = wrap_angle(yaw_of(dir) - t.yaw());
        let behind = wrap_angle(ahead + std::f32::consts::PI);
        let current = if self.drive_sign > 0.0 { ahead } else { behind };
        if current.abs() > std::f32::consts::FRAC_PI_2 + 0.4 {
            self.drive_sign = -self.drive_sign;
        }
        let err = if self.drive_sign > 0.0 { ahead } else { behind };
        Command {
            steer: (err * 2.5).clamp(-1.0, 1.0),
            throttle: self.drive_sign
                * if err.abs() > 1.0 {
                    0.2
                } else {
                    strafe_speed(t) - err.abs() * 0.4
                },
            ..Command::default()
        }
    }

    /// Called when this bot's shot actually leaves the barrel.
    pub fn fired(&mut self) {
        self.aim_err = self.rng.range(-0.04, 0.04);
    }
}

/// The heading for a shot from `t` at `o` off one wall, if there's a clean
/// one: out to the wall, mirrored off it, on to the tank, with nothing in
/// the way on either leg. Shells fly level (no assist lifts a bank shot),
/// so the target has to stand about as high as the gun. Of those it finds,
/// the shortest flight.
fn bank_shot(world: &World, t: &Tank, o: &Tank, speed: f32) -> Option<f32> {
    bank_from(world, t.breech(), o, speed)
}

/// [`bank_shot`] from a gun at `eye`.
fn bank_from(world: &World, eye: Vec3, o: &Tank, speed: f32) -> Option<f32> {
    let my2 = flat(eye);
    // Fired level from the gun, it crosses the target at gun height: its
    // turret, or its hull if it stands lower.
    let aim = o.pos + o.up() * (o.chassis.hull_top + 0.4);
    if (aim.y - eye.y).abs() > 0.8 {
        return None;
    }
    let bearing = yaw_of(flat(aim) - my2);
    let mut best: Option<(f32, f32)> = None;
    for k in 0..40 {
        let a = bearing + (k as f32 / 39.0 - 0.5) * 3.0;
        let dir = Vec3::new(a.sin(), 0.0, a.cos());
        let Some((h, n)) = world.raycast_normal(eye, dir, BANK_REACH) else {
            continue;
        };
        let n2 = flat(n);
        if n.y.abs() > 0.35 || h < 2.0 {
            continue;
        }
        let n2 = n2.normalize();
        let wall = my2 + flat(dir) * h;
        // Where it'll be by the time the shot gets round (a guess at the
        // flight from the straight line out and back).
        let est = (h + flat(aim).distance(wall)) / speed;
        let tp = flat(aim) + flat(o.vel) * est;
        if (tp - wall).dot(n2) < 0.5 || (my2 - wall).dot(n2) < 0.5 {
            continue;
        }
        let image = tp - 2.0 * (tp - wall).dot(n2) * n2;
        let to = image - my2;
        let d = to.length();
        let d2 = to / d;
        let shot = Vec3::new(d2.x, 0.0, d2.y);
        // The real wall where that line meets it: the same face, short of
        // the image.
        let Some((h2, m)) = world.raycast_normal(eye, shot, d) else {
            continue;
        };
        let m2 = flat(m).normalize_or_zero();
        if m.y.abs() > 0.35 || m2.dot(n2) < 0.97 || h2 < 2.0 {
            continue;
        }
        let q = eye + shot * h2;
        let r = d2 - 2.0 * d2.dot(m2) * m2;
        let leg = flat(q).distance(tp);
        let back = Vec3::new(r.x, 0.0, r.y);
        let from = q + Vec3::new(m2.x, 0.0, m2.y) * 0.1;
        if world.raycast(from, back, leg) < leg - 1.0 {
            continue;
        }
        // And it does pass over the tank, not beside it.
        let miss = (tp - flat(q)).perp_dot(r).abs();
        let total = h2 + leg;
        if miss > 0.5 || total > BANK_REACH * 1.4 {
            continue;
        }
        if best.is_none_or(|(b, _)| total < b) {
            best = Some((total, yaw_of(d2)));
        }
    }
    best.map(|(_, yaw)| yaw)
}

/// How hard a bot drives while circling a target. A hover hull rocks on
/// its cushion when it pushes hard, and its gun with it: it eases off.
fn strafe_speed(t: &Tank) -> f32 {
    if t.hull_kind == crate::sim::HullKind::Sethuun {
        0.35
    } else {
        1.0
    }
}

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

/// Line of sight from an eye point to a tank's upper hull, against static geometry.
pub fn can_see(world: &World, eye: Vec3, o: &Tank) -> bool {
    let to = o.pos + o.up() * o.chassis.hull_top - eye;
    let len = to.length();
    len < 1e-3 || world.raycast(eye, to / len, len) >= len - 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::DT;

    #[test]
    fn paths_avoid_buildings() {
        let map = Map::saltmarket();
        let nav = NavGrid::build(&map);
        let (a, b) = (map.open_spots[0], *map.open_spots.last().unwrap());
        let path = nav.path(a, b);
        assert!(!path.is_empty(), "no path across the town");
        assert!(path.last().unwrap().distance(b) < 3.0);
        let mut prev = a;
        for &p in &path {
            assert!(
                nav.clear_line(prev, p) || prev == a,
                "path cuts through a block"
            );
            prev = p;
        }
    }

    #[test]
    fn spawn_spots_are_open() {
        for map in [Map::saltmarket(), Map::sanctum(), Map::kiln()] {
            let nav = NavGrid::build(&map);
            // There must be open nav ground right next to each spawn.
            for &p in &map.open_spots {
                let c = nav.nearest_open(p).expect("spawn with no open ground");
                assert!(
                    nav.center(c).distance(p) < 2.5,
                    "spawn {p} is {} m from open ground",
                    nav.center(c).distance(p)
                );
            }
        }
    }

    /// Walled into a pocket, every roam spot is out of reach, and each failed
    /// search covers the whole grid: the bot must wait for its replan timer
    /// instead of searching again every step (that made big matches crawl).
    #[test]
    fn unreachable_goals_dont_replan_every_step() {
        let mut map = Map::empty(100.0);
        map.open_spots = vec![Vec2::new(20.0, 20.0), Vec2::new(30.0, 80.0)];
        // A sealed 16 m pen around (70, 70).
        for (min, max) in [
            (Vec2::new(60.0, 60.0), Vec2::new(80.0, 62.0)),
            (Vec2::new(60.0, 78.0), Vec2::new(80.0, 80.0)),
            (Vec2::new(60.0, 60.0), Vec2::new(62.0, 80.0)),
            (Vec2::new(78.0, 60.0), Vec2::new(80.0, 80.0)),
        ] {
            map.solids.push(crate::map::Solid {
                min: Vec3::new(min.x, 0.0, min.y),
                max: Vec3::new(max.x, 4.0, max.y),
            });
        }
        let nav = NavGrid::build(&map);
        let mut w = World::empty(map);
        w.spawn_tank(
            Vec3::ONE,
            Vec2::new(70.0, 70.0),
            0.0,
            crate::sim::HullKind::Kataphrakt,
            crate::sim::TurretKind::Kontos,
        );
        let mut bot = Bot::new(3);
        let contacts = [Contact {
            targetable: false,
            team: 0,
            health: 1.0,
        }];
        for _ in 0..(3.0 / DT) as usize {
            let (cmd, _) = bot.think(0, &w, &contacts, None, &[], &nav, DT);
            w.step(&[cmd]);
        }
        assert!(bot.plans <= 3, "{} path searches in 3 s", bot.plans);
    }

    /// Bot 0 with `turret` against a parked enemy `start` m away on open
    /// ground, for `secs`: the distance between them and the bot's position,
    /// sampled every step.
    fn standoff_track(turret: TurretKind, start: f32, secs: f32) -> Vec<(f32, Vec2)> {
        let map = Map::empty(400.0);
        let nav = NavGrid::build(&map);
        let mut w = World::empty(map);
        let hull = crate::sim::HullKind::Kataphrakt;
        w.spawn_tank(
            Vec3::ONE,
            Vec2::new(200.0, 200.0 - start),
            0.0,
            hull,
            turret,
        );
        w.spawn_tank(
            Vec3::ONE,
            Vec2::new(200.0, 200.0),
            0.0,
            hull,
            TurretKind::Kontos,
        );
        let mut bot = Bot::new(7);
        let contacts = [0, 1].map(|team| Contact {
            targetable: true,
            team,
            health: 1.0,
        });
        let mut out = Vec::new();
        for _ in 0..(secs / DT) as usize {
            let (cmd, _) = bot.think(0, &w, &contacts, None, &[], &nav, DT);
            w.step(&[cmd, Command::default()]);
            let p = flat(w.tanks[0].pos);
            out.push((p.distance(flat(w.tanks[1].pos)), p));
        }
        out
    }

    fn standoff(turret: TurretKind, start: f32, secs: f32) -> Vec<f32> {
        standoff_track(turret, start, secs)
            .into_iter()
            .map(|(d, _)| d)
            .collect()
    }

    /// Ranged guns don't charge: a rail cannon closes from long range to its
    /// band and holds it, never driving into the enemy's face.
    #[test]
    fn ranged_bots_hold_their_distance() {
        let d = standoff(TurretKind::Kontos, 120.0, 20.0);
        let (min, end) = (
            d.iter().cloned().fold(f32::MAX, f32::min),
            *d.last().unwrap(),
        );
        assert!(min > 45.0, "rail cannon closed to {min:.1} m");
        assert!(
            end < 100.0,
            "rail cannon never closed in: still {end:.1} m off"
        );
    }

    /// Started on top of the enemy, a ranged bot backs out to fighting range.
    #[test]
    fn ranged_bots_back_off_when_crowded() {
        let d = standoff(TurretKind::Pelekys, 12.0, 15.0);
        let end = *d.last().unwrap();
        assert!(end > 25.0, "twin guns still crowded at {end:.1} m");
    }

    /// Ranged bots keep moving while they fight, so they're hard to hit.
    #[test]
    fn ranged_bots_dont_park() {
        let track = standoff_track(TurretKind::Lumen, 55.0, 10.0);
        assert!(track.iter().all(|&(d, _)| d > 30.0), "laser closed in");
        let driven: f32 = track.windows(2).map(|w| w[0].1.distance(w[1].1)).sum();
        assert!(driven > 25.0, "laser only drove {driven:.1} m in 10 s");
    }

    /// The flamethrower still has to get right up close.
    #[test]
    fn flamethrowers_close_in() {
        let d = standoff(TurretKind::Hymn, 80.0, 20.0);
        let end = *d.last().unwrap();
        assert!(end < 16.0, "flamethrower stayed {end:.1} m off");
    }

    /// A bot with nobody to fight drives somewhere: it roams and actually moves.
    #[test]
    fn lone_bot_roams() {
        let mut w = World::new(Map::saltmarket(), 0);
        let nav = NavGrid::build(&w.map);
        let mut bot = Bot::new(1);
        let start = w.tanks[0].pos;
        let contacts = [Contact {
            targetable: false,
            team: 0,
            health: 1.0,
        }];
        for _ in 0..(8.0 / DT) as usize {
            let (cmd, _) = bot.think(0, &w, &contacts, None, &[], &nav, DT);
            w.step(&[cmd]);
        }
        assert!(
            w.tanks[0].pos.distance(start) > 15.0,
            "bot only moved {:.1} m",
            w.tanks[0].pos.distance(start)
        );
    }
}
