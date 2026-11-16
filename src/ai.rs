//! Bot opponents: a coarse navigation grid over the map, A* paths through it,
//! and a small brain per bot that hunts the nearest enemy, drives the path,
//! aims with a little lead and a little error, and fires when lined up.
//!
//! Bots drive through the same `Command` as the player; they get no physics
//! shortcuts.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use glam::{Vec2, Vec3};

use crate::map::Map;
use crate::rng::Rng;
use crate::sim::{Command, Tank, World, wrap_angle, yaw_of};

/// Nav cell size, m. Finer than the city grid so narrow gaps between cover show up.
const NAV_CELL: f32 = 2.0;
/// How far obstacles are grown so paths keep a hull's width off walls.
const CLEARANCE: f32 = 2.2;
/// Bots don't see further than this.
pub const SIGHT: f32 = 130.0;
/// Bots open fire inside this range.
const FIRE_RANGE: f32 = 110.0;
/// Preferred fighting distance: bots stop closing in here.
const STANDOFF: f32 = 24.0;

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
            if o.y1 - o.y0 < 0.6 || o.y0 > map.ground_height(o.center.x, o.center.y) + 1.0 {
                continue;
            }
            let reach = o.half.length() + CLEARANCE;
            let (lo, hi) = (o.center - Vec2::splat(reach), o.center + Vec2::splat(reach));
            let c0 = ((lo / NAV_CELL).floor().max(Vec2::ZERO)).as_uvec2();
            let c1 = (hi / NAV_CELL)
                .ceil()
                .as_uvec2()
                .min(glam::UVec2::splat(n as u32));
            for iz in c0.y as usize..c1.y as usize {
                for ix in c0.x as usize..c1.x as usize {
                    let c = Vec2::new(ix as f32 + 0.5, iz as f32 + 0.5) * NAV_CELL;
                    if o.distance(c) <= CLEARANCE {
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
    /// Aim offset re-rolled after every shot, radians.
    aim_err: f32,
    stuck_for: f32,
    reverse_for: f32,
    reverse_steer: f32,
    /// Side-to-side jink while fighting.
    jink: f32,
    jink_in: f32,
    roam: Option<Vec2>,
}

impl Bot {
    pub fn new(seed: u64) -> Bot {
        let mut rng = Rng::new(seed);
        let jink = if rng.chance(0.5) { 1.0 } else { -1.0 };
        Bot {
            rng,
            path: Vec::new(),
            replan_in: 0.0,
            target: None,
            retarget_in: 0.0,
            seen_for: 0.0,
            last_seen: None,
            aim_err: 0.0,
            stuck_for: 0.0,
            reverse_for: 0.0,
            reverse_steer: 1.0,
            jink,
            jink_in: 2.0,
            roam: None,
        }
    }

    /// Forget everything (after dying).
    pub fn reset(&mut self) {
        self.path.clear();
        self.target = None;
        self.last_seen = None;
        self.seen_for = 0.0;
        self.stuck_for = 0.0;
        self.reverse_for = 0.0;
        self.roam = None;
    }

    /// Decide this step's controls for tank `me`. Returns (command, wants to fire).
    pub fn think(
        &mut self,
        me: usize,
        world: &World,
        contacts: &[Contact],
        nav: &NavGrid,
        dt: f32,
    ) -> (Command, bool) {
        let t = &world.tanks[me];
        let g = crate::combat::gun(t.turret_kind);
        // Short-range guns (the flamethrower) have to close in before they stop to fight.
        let standoff = STANDOFF.min(g.range() * 0.6);
        let my2 = flat(t.pos);
        let eye = t.pos + Vec3::Y * t.gun_height();

        // Target: the nearest enemy, strongly preferring ones in sight. Sticky for a moment.
        self.retarget_in -= dt;
        let stale = self.target.is_some_and(|j| !contacts[j].targetable);
        if self.retarget_in <= 0.0 || stale {
            self.retarget_in = 0.4;
            let mut best: Option<(usize, f32)> = None;
            for (j, o) in world.tanks.iter().enumerate() {
                if j == me || !contacts[j].targetable {
                    continue;
                }
                let d = t.pos.distance(o.pos);
                let mut score = d;
                if !can_see(world, eye, o) {
                    score += 120.0;
                }
                if Some(j) == self.target {
                    score -= 15.0;
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

        // Where to drive: at the target (or where it was last seen), else roam.
        let goal = match (target, visible) {
            (Some(o), _) => flat(o.pos),
            (None, _) => *self.roam.get_or_insert_with(|| {
                world.map.open_spots[self.rng.index(world.map.open_spots.len())]
            }),
        };
        if self.roam.is_some_and(|r| r.distance(my2) < 8.0) {
            self.roam = None;
        }

        self.replan_in -= dt;
        if self.replan_in <= 0.0 || self.path.is_empty() {
            self.replan_in = 1.0 + self.rng.f32() * 0.5;
            self.path = nav.path(my2, goal);
        }
        while self.path.len() > 1 && self.path[0].distance(my2) < 4.0 {
            self.path.remove(0);
        }

        // Driving.
        let mut cmd = Command::default();
        let dist = goal.distance(my2);
        let close = visible && dist < standoff;
        if let Some(&wp) = self.path.first() {
            let err = wrap_angle(yaw_of(wp - my2) - t.yaw());
            cmd.steer = (err * 2.5).clamp(-1.0, 1.0);
            cmd.throttle = if err.abs() > 1.1 {
                0.15
            } else {
                1.0 - err.abs() * 0.5
            };
        }
        if close {
            // In a fight at close range: stop advancing and jink back and forth.
            self.jink_in -= dt;
            if self.jink_in <= 0.0 {
                self.jink = -self.jink;
                self.jink_in = self.rng.range(1.2, 2.6);
            }
            cmd.throttle = self.jink * 0.7;
            cmd.steer *= 0.3;
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

        // Aiming: lead the target a little, with a per-shot error.
        let mut fire = false;
        match target {
            Some(o) if visible => {
                let d = t.pos.distance(o.pos);
                let lead = flat(o.vel) * (d / g.speed()) * 0.8;
                let want = yaw_of(flat(o.pos) + lead - my2) + self.aim_err;
                let err = wrap_angle(want - t.turret_yaw());
                cmd.turret = (err * 3.0).clamp(-1.0, 1.0);
                // Tolerance scales with distance so a hull-width miss still counts as lined up.
                let tol = (1.4 / d.max(1.0)).max(0.02);
                if err.abs() < tol && d < FIRE_RANGE.min(g.range()) && self.seen_for > 0.45 {
                    fire = true;
                }
            }
            _ => cmd.center_turret = true,
        }
        (cmd, fire)
    }

    /// Called when this bot's shot actually leaves the barrel.
    pub fn fired(&mut self) {
        self.aim_err = self.rng.range(-0.04, 0.04);
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
    fn paths_avoid_buildings_on_a_city_map() {
        let map = Map::generate(21);
        let nav = NavGrid::build(&map);
        let (a, b) = (map.open_spots[0], *map.open_spots.last().unwrap());
        let path = nav.path(a, b);
        assert!(!path.is_empty(), "no path across the city");
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
        let map = Map::generate(7);
        let nav = NavGrid::build(&map);
        // Spawns sit on road cells; there must be open nav ground right next to each.
        for &p in &map.open_spots {
            let c = nav.nearest_open(p).expect("spawn with no open ground");
            assert!(
                nav.center(c).distance(p) < 2.5,
                "spawn {p} is {} m from open ground",
                nav.center(c).distance(p)
            );
        }
    }

    /// A bot with nobody to fight drives somewhere: it roams and actually moves.
    #[test]
    fn lone_bot_roams() {
        let mut w = World::new(Map::generate(21), 0);
        let nav = NavGrid::build(&w.map);
        let mut bot = Bot::new(1);
        let start = w.tanks[0].pos;
        let contacts = [Contact { targetable: false }];
        for _ in 0..(8.0 / DT) as usize {
            let (cmd, _) = bot.think(0, &w, &contacts, &nav, DT);
            w.step(&[cmd]);
        }
        assert!(
            w.tanks[0].pos.distance(start) > 15.0,
            "bot only moved {:.1} m",
            w.tanks[0].pos.distance(start)
        );
    }
}
