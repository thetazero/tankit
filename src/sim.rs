//! Game simulation, stepped at a fixed rate. Rendering only reads this.
//!
//! Tanks are rapier rigid bodies held up by raycast suspension: 8 rays per
//! tank act as springs against whatever is below (terrain, ramps, roofs,
//! other tanks). Drive and steering forces only apply through rays that touch
//! something, so tanks fly ballistically off jumps, tilt on slopes, and can
//! land badly or flip over.

use std::f32::consts::{PI, TAU};

use glam::{Quat, Vec2, Vec3};
use rapier3d::prelude::*;

use crate::game::DT;
use crate::map::{Map, Wedge};

pub const GRAVITY: f32 = 20.0;
pub const TANK_MASS: f32 = 1500.0;
/// Hull collider half extents in the body frame.
pub const HULL_HALF: Vec3 = Vec3::new(1.35, 0.42, 2.25);

/// Suspension: 4 rays per side, mounted just inside the hull bottom.
pub const WHEEL_COUNT: usize = 8;
pub const WHEELS: [Vec3; WHEEL_COUNT] = [
    Vec3::new(-1.15, -0.3, -1.8),
    Vec3::new(-1.15, -0.3, -0.6),
    Vec3::new(-1.15, -0.3, 0.6),
    Vec3::new(-1.15, -0.3, 1.8),
    Vec3::new(1.15, -0.3, -1.8),
    Vec3::new(1.15, -0.3, -0.6),
    Vec3::new(1.15, -0.3, 0.6),
    Vec3::new(1.15, -0.3, 1.8),
];
/// Ray length at full extension.
pub const SUSPENSION_REST: f32 = 0.8;
/// Static compression under the tank's own weight.
const SUSPENSION_SAG: f32 = 0.15;
const SUSPENSION_DAMPING_RATIO: f32 = 0.45;
/// Seconds upside down (and slow) before the tank rights itself.
const FLIP_RECOVER_TIME: f32 = 2.5;

#[derive(Clone, Copy, Default, Debug)]
pub struct Command {
    /// -1 reverse .. 1 forward.
    pub throttle: f32,
    /// +1 turns left (counter-clockwise seen from above).
    pub steer: f32,
    /// Turret rotation input, -1..1 (+1 turns left). Tanki-style: Z/X.
    pub turret: f32,
    /// Swing the turret back to face forward over the hull (C).
    pub center_turret: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct HullStats {
    pub max_speed: f32,
    pub reverse_speed: f32,
    pub accel: f32,
    pub brake: f32,
    /// Deceleration with no throttle. Also what holds a parked tank on a slope.
    pub coast: f32,
    pub turn_rate: f32,
    pub turn_accel: f32,
    pub grip: f32,
    /// Max turret rotation speed relative to the hull, rad/s.
    pub turret_speed: f32,
    /// How fast the turret spins up to full speed, rad/s².
    pub turret_accel: f32,
}

impl HullStats {
    /// Light, fast hull: the default feel we tune everything else against.
    pub const SCOUT: Self = Self {
        max_speed: 15.0,
        reverse_speed: 9.0,
        accel: 22.0,
        brake: 45.0,
        coast: 12.0,
        turn_rate: 2.5,
        turn_accel: 16.0,
        grip: 38.0,
        turret_speed: 2.0,
        turret_accel: 12.0,
    };
}

/// Body frame: +z forward, +y up (yaw 0 faces world +z).
#[derive(Clone, Debug)]
pub struct Tank {
    pub color: Vec3,
    pub hull: HullStats,
    pub body: RigidBodyHandle,

    // Mirrored from the rigid body after every step, for rendering and AI.
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    /// Current suspension ray length per wheel (SUSPENSION_REST when in the air).
    pub wheel_len: [f32; WHEEL_COUNT],
    pub wheels_down: usize,

    /// Turret angle relative to the hull: the turret is mounted, so it turns with the hull.
    pub turret_rel: f32,
    pub turret_rate: f32,
    pub track_phase: f32,
    pub flipped_time: f32,
}

impl Tank {
    pub fn forward(&self) -> Vec3 {
        self.rot * Vec3::Z
    }

    pub fn up(&self) -> Vec3 {
        self.rot * Vec3::Y
    }

    /// Heading on the ground plane.
    pub fn yaw(&self) -> f32 {
        let f = self.forward();
        yaw_of(Vec2::new(f.x, f.z))
    }

    /// World-space turret direction (3D, follows hull tilt).
    pub fn turret_dir(&self) -> Vec3 {
        self.rot * (Quat::from_rotation_y(self.turret_rel) * Vec3::Z)
    }

    /// Turret heading on the ground plane.
    pub fn turret_yaw(&self) -> f32 {
        let d = self.turret_dir();
        if Vec2::new(d.x, d.z).length_squared() < 1e-4 { self.yaw() } else { yaw_of(Vec2::new(d.x, d.z)) }
    }

    /// Signed speed along the hull's forward axis.
    pub fn speed(&self) -> f32 {
        self.vel.dot(self.forward())
    }
}

pub struct World {
    pub map: Map,
    pub tanks: Vec<Tank>,
    pub physics: PhysicsWorld,
    pub time: f32,
}

impl World {
    pub fn new(map: Map, spawn: usize) -> World {
        let mut physics = PhysicsWorld::default();
        physics.gravity = Vec3::new(0.0, -GRAVITY, 0.0);
        physics.integration_parameters.dt = DT;
        add_static_colliders(&mut physics, &map);

        let spot = map.open_spots[spawn % map.open_spots.len()];
        let yaw = map.spawn_yaw.unwrap_or_else(|| yaw_of(map.center() - spot));
        let mut w = World { map, tanks: Vec::new(), physics, time: 0.0 };
        w.spawn_tank(Vec3::new(0.30, 0.62, 0.30), spot, yaw);
        // Initialize the broad phase so the first suspension raycasts see the ground.
        w.physics.step();
        w
    }

    pub fn spawn_tank(&mut self, color: Vec3, at: Vec2, yaw: f32) -> usize {
        let y = self.map.ground_height(at.x, at.y) + 1.1;
        let body = RigidBodyBuilder::dynamic()
            .pose(Pose::from_parts(Vec3::new(at.x, y, at.y), Quat::from_rotation_y(yaw)))
            .linear_damping(0.05)
            .angular_damping(0.6)
            .ccd_enabled(true)
            .build();
        let h = self.physics.insert_body(body);
        // Lowered center of mass keeps the tank planted; the inertia is a slightly
        // inflated box so it doesn't tumble too eagerly off jumps.
        let s = HULL_HALF * 2.0;
        let inertia = Vec3::new(s.y * s.y + s.z * s.z, s.x * s.x + s.z * s.z, s.x * s.x + s.y * s.y) * TANK_MASS / 12.0 * 1.3;
        let collider = ColliderBuilder::cuboid(HULL_HALF.x, HULL_HALF.y, HULL_HALF.z)
            .mass_properties(MassProperties::new(Vec3::new(0.0, -0.25, 0.0), TANK_MASS, inertia))
            .friction(0.3);
        self.physics.insert_collider(collider, Some(h));
        let mut t = Tank {
            color,
            hull: HullStats::SCOUT,
            body: h,
            pos: Vec3::ZERO,
            rot: Quat::IDENTITY,
            vel: Vec3::ZERO,
            wheel_len: [SUSPENSION_REST; WHEEL_COUNT],
            wheels_down: 0,
            turret_rel: 0.0,
            turret_rate: 0.0,
            track_phase: 0.0,
            flipped_time: 0.0,
        };
        sync_from_body(&mut t, &self.physics);
        self.tanks.push(t);
        self.tanks.len() - 1
    }

    pub fn step(&mut self, cmds: &[Command]) {
        self.time += DT;
        for i in 0..self.tanks.len() {
            let cmd = cmds.get(i).copied().unwrap_or_default();
            let t = &mut self.tanks[i];
            suspension_and_drive(t, &cmd, &mut self.physics);
            turret(t, &cmd, DT);
        }
        self.physics.step();
        for t in &mut self.tanks {
            sync_from_body(t, &self.physics);
            recover_if_flipped(t, &mut self.physics);
        }
    }

    /// Distance along a ray until it hits static geometry, capped at `max`.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> f32 {
        let ray = Ray::new(origin, dir);
        self.physics
            .cast_ray(&ray, max, true, QueryFilter::only_fixed())
            .map_or(max, |(_, toi)| toi)
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

fn add_static_colliders(physics: &mut PhysicsWorld, map: &Map) {
    let ground_friction = 0.7;
    match &map.terrain {
        Some(t) => {
            // Column-major, rows advance along z and columns along x.
            let mut data = Vec::with_capacity(t.nx * t.nz);
            for ix in 0..t.nx {
                for iz in 0..t.nz {
                    data.push(t.at(ix, iz));
                }
            }
            let size = t.size();
            let heights = rapier3d::parry::utils::Array2::new(t.nz, t.nx, data);
            physics.insert_collider(
                ColliderBuilder::heightfield(heights, Vec3::new(size.x, 1.0, size.y))
                    .translation(Vec3::new(t.origin.x + size.x * 0.5, 0.0, t.origin.y + size.y * 0.5))
                    .friction(ground_friction),
                None,
            );
            // Flat apron around (not under) the terrain.
            for (min, max) in apron_rects(t.origin, t.origin + size, map.size) {
                let half = Vec3::new((max.x - min.x) * 0.5, 1.0, (max.y - min.y) * 0.5);
                physics.insert_collider(
                    ColliderBuilder::cuboid(half.x, half.y, half.z)
                        .translation(Vec3::new(min.x + half.x, -1.0, min.y + half.z))
                        .friction(ground_friction),
                    None,
                );
            }
        }
        None => {
            physics.insert_collider(
                ColliderBuilder::cuboid(map.size * 2.0, 1.0, map.size * 2.0)
                    .translation(Vec3::new(map.size * 0.5, -1.0, map.size * 0.5))
                    .friction(ground_friction),
                None,
            );
        }
    }
    for s in &map.solids {
        let half = (s.max - s.min) * 0.5;
        physics.insert_collider(
            ColliderBuilder::cuboid(half.x, half.y, half.z).translation(s.min + half).friction(0.4),
            None,
        );
    }
    // Low visual slabs (sidewalks, plazas, lawns) are real curbs to roll over.
    for b in &map.boxes {
        let h = b.max.y - b.min.y;
        if b.min.y.abs() < 1e-3 && h > 0.0 && h <= 0.3 {
            let half = (b.max - b.min) * 0.5;
            physics.insert_collider(
                ColliderBuilder::cuboid(half.x, half.y, half.z).translation(b.min + half).friction(ground_friction),
                None,
            );
        }
    }
    for w in &map.wedges {
        if let Some(c) = wedge_collider(w) {
            physics.insert_collider(c.friction(ground_friction), None);
        }
    }
}

/// Four rects surrounding the terrain rect [a, b], reaching `pad` beyond the arena.
pub fn apron_rects(a: Vec2, b: Vec2, size: f32) -> [(Vec2, Vec2); 4] {
    let (lo, hi) = (-size, size * 2.0);
    [
        (Vec2::new(lo, lo), Vec2::new(hi, a.y)),
        (Vec2::new(lo, b.y), Vec2::new(hi, hi)),
        (Vec2::new(lo, a.y), Vec2::new(a.x, b.y)),
        (Vec2::new(b.x, a.y), Vec2::new(hi, b.y)),
    ]
}

fn wedge_collider(w: &Wedge) -> Option<ColliderBuilder> {
    ColliderBuilder::convex_hull(&w.points())
}

fn sync_from_body(t: &mut Tank, physics: &PhysicsWorld) {
    let b = &physics.bodies[t.body];
    t.pos = b.translation();
    t.rot = *b.rotation();
    t.vel = b.linvel();
}

/// Spring constant per wheel so the tank sags SUSPENSION_SAG at rest.
fn spring_k() -> f32 {
    TANK_MASS * GRAVITY / (WHEEL_COUNT as f32 * SUSPENSION_SAG)
}

fn spring_c() -> f32 {
    2.0 * (spring_k() * TANK_MASS / WHEEL_COUNT as f32).sqrt() * SUSPENSION_DAMPING_RATIO
}

fn suspension_and_drive(t: &mut Tank, cmd: &Command, physics: &mut PhysicsWorld) {
    let (pose, com) = {
        let b = &physics.bodies[t.body];
        (*b.position(), b.center_of_mass())
    };
    let up = pose.rotation * Vec3::Y;
    let fwd = pose.rotation * Vec3::Z;

    // Cast every suspension ray first (immutable borrow), then apply forces.
    let mut hits: [Option<(Vec3, Vec3, f32)>; WHEEL_COUNT] = [None; WHEEL_COUNT];
    {
        let filter = QueryFilter::default().exclude_rigid_body(t.body);
        let qp = physics.query_pipeline_with_filter(filter);
        for (i, m) in WHEELS.iter().enumerate() {
            let origin = pose.transform_point(*m);
            let ray = Ray::new(origin, -up);
            if let Some((_, hit)) = qp.cast_ray_and_get_normal(&ray, SUSPENSION_REST, true) {
                hits[i] = Some((origin, hit.normal, hit.time_of_impact));
            }
        }
    }

    let (k, c) = (spring_k(), spring_c());
    let b = &mut physics.bodies[t.body];
    let mut normal_sum = Vec3::ZERO;
    t.wheels_down = 0;
    for (i, hit) in hits.iter().enumerate() {
        match hit {
            Some((origin, n, toi)) => {
                t.wheel_len[i] = *toi;
                t.wheels_down += 1;
                normal_sum += *n;
                let compression = SUSPENSION_REST - toi;
                let v_along = b.velocity_at_point(*origin).dot(up);
                let force = (k * compression - c * v_along).max(0.0);
                b.apply_impulse_at_point(up * force * DT, *origin, true);
            }
            None => t.wheel_len[i] = SUSPENSION_REST,
        }
    }

    if t.wheels_down == 0 {
        return;
    }
    let h = t.hull;
    let traction = t.wheels_down as f32 / WHEEL_COUNT as f32;
    let n = normal_sum.normalize_or(up);

    // Track directions projected onto the ground.
    let fwd_g = (fwd - n * fwd.dot(n)).normalize_or(fwd);
    let left_g = n.cross(fwd_g).normalize_or(pose.rotation * Vec3::X);
    let v = b.linvel();
    let v_long = v.dot(fwd_g);
    let v_lat = v.dot(left_g);

    let throttle = cmd.throttle.clamp(-1.0, 1.0);
    let target = if throttle >= 0.0 { throttle * h.max_speed } else { throttle * h.reverse_speed };
    let rate = if throttle.abs() < 0.05 {
        h.coast
    } else if v_long * target < 0.0 {
        h.brake
    } else {
        h.accel
    };
    let new_long = approach(v_long, target, rate * DT);
    let new_lat = approach(v_lat, 0.0, h.grip * DT);
    let dv = (fwd_g * (new_long - v_long) + left_g * (new_lat - v_lat)) * traction;
    // Push slightly below the center of mass: accelerating lifts the nose, braking dips it.
    b.apply_impulse_at_point(dv * TANK_MASS, com - up * 0.35, true);
    t.track_phase += new_long * DT;

    // Steering: drive the yaw rate around the hull's up axis directly.
    let steer = if v_long < -0.5 && throttle < 0.0 { -cmd.steer } else { cmd.steer };
    let speed_frac = (new_long.abs() / h.max_speed).min(1.0);
    let target_rate = steer.clamp(-1.0, 1.0) * h.turn_rate * (1.0 - 0.25 * speed_frac);
    let w = b.angvel();
    let yaw_rate = w.dot(up);
    let new_rate = approach(yaw_rate, target_rate, h.turn_accel * DT);
    b.set_angvel(w + up * (new_rate - yaw_rate) * traction, true);
}

fn turret(t: &mut Tank, cmd: &Command, dt: f32) {
    let h = t.hull;
    // Manual rotation relative to the hull, with a short spin-up.
    // Centering drives it back to 0 and stops exactly there.
    let manual = cmd.turret.clamp(-1.0, 1.0);
    let target_rate = if manual.abs() > 0.05 {
        manual * h.turret_speed
    } else if cmd.center_turret {
        // Full speed toward center until it's time to brake.
        let stop_dist = t.turret_rate * t.turret_rate / (2.0 * h.turret_accel);
        let err = -t.turret_rel;
        if err.abs() <= stop_dist { 0.0 } else { err.signum() * h.turret_speed }
    } else {
        0.0
    };
    t.turret_rate = approach(t.turret_rate, target_rate, h.turret_accel * dt);
    let before = t.turret_rel;
    t.turret_rel = wrap_angle(t.turret_rel + t.turret_rate * dt);
    if cmd.center_turret && manual.abs() <= 0.05 {
        let crossed = before.signum() != t.turret_rel.signum() && before.abs() < 0.5;
        let settled = t.turret_rel.abs() < 0.02 && t.turret_rate.abs() < 0.5;
        if crossed || settled {
            // Snap and stop exactly forward instead of creeping or oscillating.
            t.turret_rel = 0.0;
            t.turret_rate = 0.0;
        }
    }
}

/// Upside down or on its side for a while: put it back on its tracks.
fn recover_if_flipped(t: &mut Tank, physics: &mut PhysicsWorld) {
    let tipped = t.up().y < 0.3;
    if tipped && t.vel.length() < 3.0 {
        t.flipped_time += DT;
    } else {
        t.flipped_time = 0.0;
    }
    if t.flipped_time > FLIP_RECOVER_TIME {
        t.flipped_time = 0.0;
        let b = &mut physics.bodies[t.body];
        let pos = b.translation() + Vec3::Y * 1.5;
        b.set_position(Pose::from_parts(pos, Quat::from_rotation_y(t.yaw())), true);
        b.set_linvel(Vec3::ZERO, true);
        b.set_angvel(Vec3::ZERO, true);
        sync_from_body(t, physics);
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
    use crate::map::{Rise, Terrain};

    fn flat_world() -> World {
        let mut map = Map::proving_ground();
        map.terrain = None;
        map.wedges.clear();
        map.solids.retain(|s| s.max.x < 1.0 || s.min.x > 199.0 || s.max.z < 1.0 || s.min.z > 199.0);
        map.boxes.clear();
        map.open_spots = vec![Vec2::new(100.0, 100.0)];
        World::new(map, 0)
    }

    fn run(w: &mut World, cmd: Command, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            w.step(&[cmd]);
        }
    }

    fn place(w: &mut World, pos: Vec3, rot: Quat) {
        let b = &mut w.physics.bodies[w.tanks[0].body];
        b.set_position(Pose::from_parts(pos, rot), true);
        b.set_linvel(Vec3::ZERO, true);
        b.set_angvel(Vec3::ZERO, true);
    }

    #[test]
    fn map_is_deterministic() {
        let (a, b) = (Map::generate(42), Map::generate(42));
        assert_eq!(a.solids.len(), b.solids.len());
        assert_eq!(a.boxes.len(), b.boxes.len());
        assert!(a.solids.len() > 20, "expected a dense map, got {} solids", a.solids.len());
    }

    #[test]
    fn open_spots_are_drivable() {
        for seed in 0..20 {
            let m = Map::generate(seed);
            assert!(m.open_spots.len() > 100);
            for p in &m.open_spots {
                for s in &m.solids {
                    let cp = p.clamp(s.min2(), s.max2());
                    assert!(cp.distance(*p) >= 2.3, "seed {seed}: open spot {p} overlaps a solid");
                }
            }
        }
    }

    #[test]
    fn settles_level_at_rest_height() {
        let mut w = flat_world();
        run(&mut w, Command::default(), 3.0);
        let t = &w.tanks[0];
        assert_eq!(t.wheels_down, WHEEL_COUNT);
        let expected = 0.3 + SUSPENSION_REST - SUSPENSION_SAG;
        assert!((t.pos.y - expected).abs() < 0.08, "ride height {} vs {expected}", t.pos.y);
        assert!(t.up().y > 0.999, "not level: {:?}", t.up());
        // Velocity read after a step includes one tick of gravity, so check drift instead.
        let p0 = t.pos;
        run(&mut w, Command::default(), 1.0);
        assert!(w.tanks[0].pos.distance(p0) < 0.005, "still moving: {:?}", w.tanks[0].pos - p0);
    }

    #[test]
    fn reaches_top_speed_on_flat() {
        let mut w = flat_world();
        run(&mut w, Command::default(), 1.0);
        run(&mut w, Command { throttle: 1.0, ..Default::default() }, 2.5);
        let s = w.tanks[0].speed();
        assert!((s - HullStats::SCOUT.max_speed).abs() < 0.5, "speed {s}");
    }

    #[test]
    fn holds_position_on_slope() {
        let mut map = Map::proving_ground();
        // A uniform 15 degree slope rising along +x.
        let grade = 15f32.to_radians().tan();
        map.terrain = Some(Terrain::from_fn(Vec2::ZERO, 2.0, 101, 101, move |x, _| x * grade));
        map.wedges.clear();
        map.solids.clear();
        map.boxes.clear();
        map.open_spots = vec![Vec2::new(100.0, 100.0)];
        let mut w = World::new(map, 0);
        run(&mut w, Command::default(), 2.0);
        let p0 = w.tanks[0].pos;
        run(&mut w, Command::default(), 3.0);
        let drift = w.tanks[0].pos.distance(p0);
        assert!(drift < 0.3, "slid {drift} m down the slope");
        assert_eq!(w.tanks[0].wheels_down, WHEEL_COUNT);
    }

    #[test]
    fn jumps_and_lands_upright() {
        let mut w = flat_world();
        // A 22 degree kicker straight ahead along +z.
        let kicker = Wedge {
            min: Vec2::new(95.0, 120.0),
            max: Vec2::new(105.0, 124.0),
            base_y: 0.0,
            height: 1.6,
            rise: Rise::PosZ,
            color: Vec3::ONE,
        };
        w.physics.insert_collider(wedge_collider(&kicker).unwrap(), None);
        w.map.wedges.push(kicker);
        place(&mut w, Vec3::new(100.0, 1.0, 80.0), Quat::IDENTITY);
        run(&mut w, Command::default(), 1.0);

        let mut airborne = 0.0f32;
        let mut max_height = 0.0f32;
        for _ in 0..(6.0 / DT) as usize {
            w.step(&[Command { throttle: 1.0, ..Default::default() }]);
            let t = &w.tanks[0];
            if t.wheels_down == 0 {
                airborne += DT;
            }
            max_height = max_height.max(t.pos.y);
        }
        assert!(airborne > 0.3, "barely left the ground: {airborne}s");
        assert!(max_height > 2.5, "jump too low: {max_height}");
        run(&mut w, Command::default(), 2.0);
        let t = &w.tanks[0];
        assert!(t.up().y > 0.95, "landed badly: up = {:?}", t.up());
        assert!(t.pos.y > 0.5 && t.pos.y < 1.5, "not resting on the ground: {}", t.pos.y);
    }

    #[test]
    fn heightfield_matches_terrain_heights() {
        let mut w = World::new(Map::proving_ground(), 0);
        w.physics.step();
        let t = w.map.terrain.clone().unwrap();
        for (x, z) in [(130.0, 40.0), (160.0, 50.0), (155.0, 150.0), (170.0, 120.0)] {
            let top = Vec3::new(x, 50.0, z);
            let hit = top.y - w.raycast(top, Vec3::NEG_Y, 100.0);
            let expect = t.height_at(x, z);
            assert!((hit - expect).abs() < 0.3, "at ({x},{z}) collider {hit} vs terrain {expect}");
        }
    }

    #[test]
    fn rolls_over_speed_bumps_and_stairs() {
        let mut w = World::new(Map::proving_ground(), 0);
        for spot in [Vec2::new(12.0, 105.0), Vec2::new(34.0, 105.0)] {
            place(&mut w, Vec3::new(spot.x, 1.0, spot.y), Quat::IDENTITY);
            run(&mut w, Command::default(), 1.0);
            run(&mut w, Command { throttle: 1.0, ..Default::default() }, 3.5);
            let t = &w.tanks[0];
            assert!(t.pos.z > spot.y + 30.0, "stuck at z={} starting from {spot}", t.pos.z);
            assert!(t.up().y > 0.8, "tipped over at {spot}");
        }
    }

    #[test]
    fn every_kicker_lane_is_clearable_at_full_speed() {
        for lane in 0..3 {
            let mut w = World::new(Map::proving_ground(), lane);
            run(&mut w, Command { throttle: 1.0, ..Default::default() }, 5.0);
            let t = &w.tanks[0];
            assert!(t.pos.z > 75.0, "lane {lane}: stopped at z={} (crashed into the landing ramp?)", t.pos.z);
            assert!(t.up().y > 0.95, "lane {lane}: not upright");
        }
    }

    #[test]
    fn turret_turns_with_hull_and_centers() {
        let mut w = flat_world();
        run(&mut w, Command { turret: 1.0, ..Default::default() }, 0.5);
        assert!(w.tanks[0].turret_rel > 0.5);
        run(&mut w, Command::default(), 0.25);
        let rel = w.tanks[0].turret_rel;
        run(&mut w, Command { steer: 1.0, ..Default::default() }, 1.0);
        assert!((w.tanks[0].turret_rel - rel).abs() < 1e-4);
        run(&mut w, Command { center_turret: true, ..Default::default() }, 2.0);
        assert!(w.tanks[0].turret_rel.abs() < 0.01);
        assert!(w.tanks[0].turret_rate.abs() < 0.01);
    }

    #[test]
    fn rights_itself_when_flipped() {
        let mut w = flat_world();
        place(&mut w, Vec3::new(100.0, 2.0, 100.0), Quat::from_rotation_z(PI));
        run(&mut w, Command::default(), 1.0);
        assert!(w.tanks[0].up().y < 0.0, "should be upside down first");
        run(&mut w, Command::default(), 4.0);
        assert!(w.tanks[0].up().y > 0.95, "did not recover: {:?}", w.tanks[0].up());
    }
}

