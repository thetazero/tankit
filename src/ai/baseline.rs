//! The bots as they were before they learned to fight (issue #186), kept
//! unchanged as the yardstick: `--arena` pits the current bots against
//! these to measure whether a change makes them win more.

use glam::{Vec2, Vec3};

use super::{Contact, NavGrid, can_see};
use crate::rng::Rng;
use crate::sim::{Command, Tank, TurretKind, World, wrap_angle, yaw_of};

const SIGHT: f32 = super::SIGHT;
const FIRE_RANGE: f32 = 110.0;
const BRAWL_RANGE: f32 = 40.0;
const WOUNDED: f32 = 0.35;
const GUARD_CHASE: f32 = 40.0;

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
    #[allow(dead_code)]
    plans: u32,
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
        self.post = None;
        self.engaged = false;
        self.flank = self.rng.range(-0.9, 0.9);
    }

    /// Decide this step's controls for tank `me`, with an optional objective
    /// ring (center, radius) to hold. Returns (command, wants to fire).
    pub fn think(
        &mut self,
        me: usize,
        world: &World,
        contacts: &[Contact],
        objective: Option<(Vec2, f32)>,
        nav: &NavGrid,
        dt: f32,
    ) -> (Command, bool) {
        let t = &world.tanks[me];
        let g = crate::combat::gun(t.turret_kind);
        // Short-range guns (the flamethrower) have to close in before they
        // stop to fight. Everything else picks a distance that suits it and
        // fights from there, further out when hurt.
        let brawler = g.range() < BRAWL_RANGE;
        let reach = FIRE_RANGE.min(g.range());
        let mut ideal = ideal_range(t.turret_kind).min(reach * 0.8);
        if !brawler && contacts[me].health < WOUNDED {
            ideal = (ideal * 1.35).min(reach * 0.9);
        }
        let standoff = ideal.max(g.range() * 0.6).min(reach);
        let my2 = flat(t.pos);
        let eye = t.pos + Vec3::Y * t.gun_height();

        // Target: the nearest enemy, strongly preferring ones in sight. Sticky for a moment.
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
                    score += 120.0;
                }
                if Some(j) == self.target {
                    score -= 15.0;
                }
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
        let goal = match (objective, target) {
            (Some((c, r)), t) => {
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
            (None, Some(o)) => approach(flat(o.pos)),
            (None, None) => *self.roam.get_or_insert_with(|| {
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
                if target.is_some() || objective.is_some() {
                    self.path.push(goal);
                }
            }
        }
        while self.path.len() > 1 && self.path[0].distance(my2) < 4.0 {
            self.path.remove(0);
        }

        // Driving.
        let mut cmd = Command::default();
        let close = brawler && visible && target_d < standoff;
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
        if tolling {
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
                // A charge gun has to be held through its charge: once it's
                // ringing up, keep holding while the target stays roughly
                // in line, rather than letting go and wasting it.
                if g.charge > 0.0 && t.charge > 0.0 && err.abs() < tol * 4.0 + 0.05 {
                    fire = true;
                }
            }
            _ => cmd.center_turret = true,
        }
        (cmd, fire)
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
                    0.85 - err.abs() * 0.4
                },
            ..Command::default()
        }
    }

    /// Called when this bot's shot actually leaves the barrel.
    pub fn fired(&mut self) {
        self.aim_err = self.rng.range(-0.04, 0.04);
    }
}

fn flat(v: Vec3) -> Vec2 {
    Vec2::new(v.x, v.z)
}

/// The distance a gun is best fought at, m: where it hits hard and reliably
/// but the enemy isn't on top of it.
fn ideal_range(kind: TurretKind) -> f32 {
    match kind {
        // Rail cannon: lands instantly at any range; keep well back.
        TurretKind::Kontos => 75.0,
        // Light, fast twin guns: middle distance.
        TurretKind::Pelekys => 45.0,
        // Slow orbs: close enough that they're hard to sidestep.
        TurretKind::Ammeth => 50.0,
        // Spraying darts: their spread wants it fairly close.
        TurretKind::Ossrin => 42.0,
        // The beam is at full strength out to 40% of its range.
        TurretKind::Lumen => 55.0,
        // Flamethrower: right up against them.
        TurretKind::Hymn => 8.0,
        // Siege bell: near enough that the slow toll is hard to sidestep.
        TurretKind::Knell => 40.0,
    }
}
