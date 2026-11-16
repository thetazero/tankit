//! Game simulation: plain structs stepped at a fixed rate. Rendering only reads this.

use std::f32::consts::{PI, TAU};

use glam::{Vec2, Vec3};

use crate::game::DT;
use crate::map::Map;
use crate::rng::Rng;

pub const TANK_RADIUS: f32 = 2.3;
/// Distance between the track center lines, for per-side tread speed.
const TRACK_SPAN: f32 = 2.5;

/// Which kit parts a tank is built from (see `tank::model`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HullKind {
    Lancer,
    Bastion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurretKind {
    Lance,
    Maul,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Command {
    /// -1 reverse .. 1 forward.
    pub throttle: f32,
    /// +1 turns left (counter-clockwise seen from above).
    pub steer: f32,
    /// World-space yaw the turret should rotate toward.
    pub aim_yaw: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct HullStats {
    pub max_speed: f32,
    pub reverse_speed: f32,
    pub accel: f32,
    pub brake: f32,
    pub coast: f32,
    pub turn_rate: f32,
    pub turn_accel: f32,
    pub grip: f32,
    pub turret_speed: f32,
}

impl HullStats {
    /// Light, fast hull: the default feel we tune everything else against.
    pub const SCOUT: Self = Self {
        max_speed: 15.0,
        reverse_speed: 9.0,
        accel: 22.0,
        brake: 45.0,
        coast: 10.0,
        turn_rate: 2.5,
        turn_accel: 16.0,
        grip: 38.0,
        turret_speed: 2.6,
    };
}

#[derive(Clone, Debug)]
pub struct Tank {
    /// Team color; tints the light armor plates.
    pub color: Vec3,
    pub hull_kind: HullKind,
    pub turret_kind: TurretKind,
    pub hull: HullStats,
    pub pos: Vec2,
    pub vel: Vec2,
    pub yaw: f32,
    pub yaw_rate: f32,
    pub turret_yaw: f32,
    /// Gun elevation in radians (positive = up).
    pub gun_pitch: f32,

    // Visual-only animation state.
    pub pitch: f32,
    pub pitch_vel: f32,
    pub roll: f32,
    pub roll_vel: f32,
    /// Distance each tread has travelled: [left (+x), right (-x)].
    pub track_dist: [f32; 2],
    /// Seconds since the gun last fired (drives recoil and muzzle flash).
    pub since_fire: f32,
    /// Which barrel fired last (twin guns alternate).
    pub fire_barrel: usize,
}

impl Tank {
    pub fn new(color: Vec3, pos: Vec2, yaw: f32) -> Tank {
        Tank {
            color,
            hull: HullStats::SCOUT,
            pos,
            vel: Vec2::ZERO,
            yaw,
            yaw_rate: 0.0,
            turret_yaw: yaw,
            gun_pitch: 0.0,
            hull_kind: HullKind::Lancer,
            turret_kind: TurretKind::Lance,
            pitch: 0.0,
            pitch_vel: 0.0,
            roll: 0.0,
            roll_vel: 0.0,
            track_dist: [0.0; 2],
            since_fire: 10.0,
            fire_barrel: 0,
        }
    }

    /// Visual side of firing: recoil, muzzle flash, and a kick to the hull.
    pub fn fire_fx(&mut self) {
        self.since_fire = 0.0;
        self.fire_barrel += 1;
        let rel = self.turret_yaw - self.yaw;
        self.pitch_vel -= rel.cos() * 0.9;
        self.roll_vel += rel.sin() * 0.7;
    }

    pub fn forward(&self) -> Vec2 {
        dir2(self.yaw)
    }

    pub fn pos3(&self) -> Vec3 {
        Vec3::new(self.pos.x, 0.0, self.pos.y)
    }

    /// Signed speed along the hull's forward axis.
    pub fn speed(&self) -> f32 {
        self.vel.dot(self.forward())
    }
}

pub struct World {
    pub map: Map,
    pub tanks: Vec<Tank>,
    pub time: f32,
}

impl World {
    pub fn new(seed: u64) -> World {
        let map = Map::generate(seed);
        let mut rng = Rng::new(seed.wrapping_mul(31).wrapping_add(7));
        let spawn = map.open_spots[rng.index(map.open_spots.len())];
        let to_center = map.center() - spawn;
        let player = Tank::new(Vec3::new(0.92, 0.93, 0.95), spawn, yaw_of(to_center));
        World { map, tanks: vec![player], time: 0.0 }
    }

    pub fn step(&mut self, cmds: &[Command]) {
        self.time += DT;
        for (i, t) in self.tanks.iter_mut().enumerate() {
            let cmd = cmds.get(i).copied().unwrap_or_default();
            drive(t, &cmd, DT);
            for s in &self.map.solids {
                resolve_circle_rect(&mut t.pos, &mut t.vel, TANK_RADIUS, s.min2(), s.max2());
            }
        }
    }

    /// Distance along a ray until it hits a solid, capped at `max`.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> f32 {
        let seg = dir * max;
        let mut best = 1.0f32;
        for s in &self.map.solids {
            if let Some(t) = seg_aabb(origin, seg, s.min, s.max) {
                best = best.min(t);
            }
        }
        best * max
    }

    /// Like `raycast`, but also stops at tree canopies (which don't block tanks).
    pub fn camera_raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> f32 {
        let seg = dir * max;
        let mut best = self.raycast(origin, dir, max) / max;
        for tree in &self.map.trees {
            if let Some(t) = seg_sphere(origin, seg, tree.pos, tree.radius * 1.1) {
                best = best.min(t);
            }
        }
        best * max
    }
}

/// Unit direction on the ground plane for a yaw (yaw 0 faces +z, positive turns toward +x).
pub fn dir2(yaw: f32) -> Vec2 {
    Vec2::new(yaw.sin(), yaw.cos())
}

pub fn yaw_of(d: Vec2) -> f32 {
    d.x.atan2(d.y)
}

pub fn wrap_angle(a: f32) -> f32 {
    let mut a = (a + PI) % TAU;
    if a < 0.0 {
        a += TAU;
    }
    a - PI
}

fn approach(v: f32, target: f32, delta: f32) -> f32 {
    if v < target { (v + delta).min(target) } else { (v - delta).max(target) }
}

fn drive(t: &mut Tank, cmd: &Command, dt: f32) {
    let h = t.hull;
    let throttle = cmd.throttle.clamp(-1.0, 1.0);
    let fwd = t.forward();
    let left = Vec2::new(fwd.y, -fwd.x);
    let v_long = t.vel.dot(fwd);
    let v_lat = t.vel.dot(left);

    let target = if throttle >= 0.0 { throttle * h.max_speed } else { throttle * h.reverse_speed };
    let rate = if throttle.abs() < 0.05 {
        h.coast
    } else if v_long * target < 0.0 {
        h.brake
    } else {
        h.accel
    };
    let new_long = approach(v_long, target, rate * dt);
    // Tracks grip sideways hard, but not instantly: collisions and fast turns slide a little.
    let new_lat = approach(v_lat, 0.0, h.grip * dt);

    // Car-style steering when reversing.
    let steer = if v_long < -0.5 && throttle < 0.0 { -cmd.steer } else { cmd.steer };
    // Slightly slower turning at full speed keeps fast hulls controllable.
    let speed_frac = (new_long.abs() / h.max_speed).min(1.0);
    let target_rate = steer.clamp(-1.0, 1.0) * h.turn_rate * (1.0 - 0.25 * speed_frac);
    t.yaw_rate = approach(t.yaw_rate, target_rate, h.turn_accel * dt);
    t.yaw = wrap_angle(t.yaw + t.yaw_rate * dt);

    let fwd2 = t.forward();
    let left2 = Vec2::new(fwd2.y, -fwd2.x);
    t.vel = fwd2 * new_long + left2 * new_lat;
    t.pos += t.vel * dt;
    // A turning tank's treads run at different speeds (the inner one can reverse).
    let half = TRACK_SPAN * 0.5;
    t.track_dist[0] += (new_long - t.yaw_rate * half) * dt;
    t.track_dist[1] += (new_long + t.yaw_rate * half) * dt;
    t.since_fire += dt;

    // Turret slews toward the commanded aim at a limited rate.
    let diff = wrap_angle(cmd.aim_yaw - t.turret_yaw);
    let step = h.turret_speed * dt;
    t.turret_yaw = wrap_angle(t.turret_yaw + diff.clamp(-step, step));

    // Suspension feel: nose lifts when accelerating, hull leans out of turns.
    let long_accel = (new_long - v_long) / dt;
    let pitch_target = (-long_accel * 0.006).clamp(-0.1, 0.1);
    let roll_target = (new_long * t.yaw_rate * 0.012).clamp(-0.08, 0.08);
    spring(&mut t.pitch, &mut t.pitch_vel, pitch_target, 140.0, 11.0, dt);
    spring(&mut t.roll, &mut t.roll_vel, roll_target, 160.0, 12.0, dt);
}

fn spring(x: &mut f32, v: &mut f32, target: f32, k: f32, c: f32, dt: f32) {
    *v += ((target - *x) * k - *v * c) * dt;
    *x = (*x + *v * dt).clamp(-0.35, 0.35);
}

/// Push a circle out of an axis-aligned rect and remove the inbound velocity.
pub fn resolve_circle_rect(pos: &mut Vec2, vel: &mut Vec2, r: f32, min: Vec2, max: Vec2) -> bool {
    let cp = pos.clamp(min, max);
    let d = *pos - cp;
    let l2 = d.length_squared();
    if l2 >= r * r {
        return false;
    }
    let (n, pen) = if l2 > 1e-8 {
        let l = l2.sqrt();
        (d / l, r - l)
    } else {
        // Center inside the rect: leave through the nearest side.
        let opts = [
            (pos.x - min.x, Vec2::NEG_X),
            (max.x - pos.x, Vec2::X),
            (pos.y - min.y, Vec2::NEG_Y),
            (max.y - pos.y, Vec2::Y),
        ];
        let (m, n) = opts.into_iter().fold((f32::INFINITY, Vec2::X), |a, b| if b.0 < a.0 { b } else { a });
        (n, m + r)
    };
    *pos += n * pen;
    let vn = vel.dot(n);
    if vn < 0.0 {
        *vel -= n * vn * 1.1;
    }
    true
}

/// Segment p + d*t, t in [0,1], vs AABB. Returns entry t.
pub fn seg_aabb(p: Vec3, d: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut tmin = 0.0f32;
    let mut tmax = 1.0f32;
    for a in 0..3 {
        let (o, v) = (p[a], d[a]);
        if v.abs() < 1e-9 {
            if o < min[a] || o > max[a] {
                return None;
            }
        } else {
            let inv = 1.0 / v;
            let (mut t1, mut t2) = ((min[a] - o) * inv, (max[a] - o) * inv);
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }
    Some(tmin)
}

/// Segment p + d*t, t in [0,1], vs sphere. Returns entry t (0 if starting inside).
pub fn seg_sphere(p: Vec3, d: Vec3, c: Vec3, r: f32) -> Option<f32> {
    let m = p - c;
    let a = d.dot(d);
    let b = m.dot(d);
    let cc = m.dot(m) - r * r;
    if cc <= 0.0 {
        return Some(0.0);
    }
    let disc = b * b - a * cc;
    if disc < 0.0 || a < 1e-12 {
        return None;
    }
    let t = (-b - disc.sqrt()) / a;
    (0.0..=1.0).contains(&t).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_is_deterministic() {
        let (a, b) = (Map::generate(42), Map::generate(42));
        assert_eq!(a.solids.len(), b.solids.len());
        assert_eq!(a.boxes.len(), b.boxes.len());
        assert!(a.solids.len() > 20, "expected a dense map, got {} solids", a.solids.len());
        assert_ne!(Map::generate(43).boxes.len(), 0);
    }

    #[test]
    fn open_spots_are_drivable() {
        for seed in 0..20 {
            let m = Map::generate(seed);
            assert!(m.open_spots.len() > 100);
            for p in &m.open_spots {
                for s in &m.solids {
                    let cp = p.clamp(s.min2(), s.max2());
                    assert!(cp.distance(*p) >= TANK_RADIUS, "seed {seed}: open spot {p} overlaps a solid");
                }
            }
        }
    }

    #[test]
    fn tank_accelerates_and_stops_at_walls() {
        let mut w = World::new(1);
        w.tanks[0].pos = Vec2::new(5.0, w.map.size * 0.5);
        w.tanks[0].yaw = -std::f32::consts::FRAC_PI_2; // facing -x, toward the perimeter wall
        let cmd = Command { throttle: 1.0, steer: 0.0, aim_yaw: w.tanks[0].yaw };
        for _ in 0..240 {
            w.step(&[cmd]);
        }
        let t = &w.tanks[0];
        assert!(t.pos.x >= TANK_RADIUS - 0.01, "tank went through the wall: {}", t.pos.x);
        assert!(t.speed().abs() < 1.0);
    }

    #[test]
    fn tank_reaches_top_speed() {
        let mut w = World::new(1);
        w.map.solids.clear();
        w.tanks[0].pos = Vec2::splat(80.0);
        let cmd = Command { throttle: 1.0, steer: 0.0, aim_yaw: w.tanks[0].yaw };
        for _ in 0..240 {
            w.step(&[cmd]);
        }
        assert!((w.tanks[0].speed() - HullStats::SCOUT.max_speed).abs() < 0.01);
    }

    #[test]
    fn segment_hits_box() {
        let t = seg_aabb(Vec3::new(-5.0, 1.0, 0.0), Vec3::new(10.0, 0.0, 0.0), Vec3::splat(-1.0), Vec3::splat(1.0));
        assert!((t.unwrap() - 0.4).abs() < 1e-5);
    }
}
