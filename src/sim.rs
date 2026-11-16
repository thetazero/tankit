//! Game simulation, stepped at a fixed rate. Rendering only reads this.
//!
//! Tanks are rapier rigid bodies. The physics is honest (real gravity, real
//! contacts); the tank-specific part is how tracks interact with the world:
//!
//! * **Suspension:** one spring ray per road wheel carries the tank's weight.
//! * **Track traction:** every point where a track touches something (a road
//!   wheel's ray, or the hull collider's track faces, including the swept-up
//!   ends) pushes along the track's direction of travel at that point, up to
//!   friction times that contact's load. That is what lets a tank climb a ledge
//!   as high as its sprocket: the sloped track run grips the edge and pulls.
//! * **Skid steering:** each side's track runs at its own speed; turning
//!   (including pivoting in place) comes from the speed difference.
//! * The rest of the hull is frictionless, so glancing off walls slides.

use std::f32::consts::{PI, TAU};

use glam::{Quat, Vec2, Vec3};
use rapier3d::prelude::*;

use crate::game::DT;
use crate::map::{Map, Wedge};

pub const GRAVITY: f32 = 9.81;

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

impl TurretKind {
    /// Velocity change one shot kicks into the hull, m/s. It's applied at gun
    /// height, so most of it shows up as the hull rocking back on its springs.
    pub fn recoil(self) -> f32 {
        match self {
            // Long rail cannon: one big slam.
            TurretKind::Lance => 2.2,
            // Twin short guns: smaller kicks, more often.
            TurretKind::Maul => 1.3,
        }
    }
}

/// Running gear and body geometry. Body frame = art frame: x across (+x is the
/// left track), y up with the ground at 0 when resting, z forward.
#[derive(Debug)]
pub struct Chassis {
    pub mass: f32,
    pub track_x: f32,
    pub track_width: f32,
    pub road_z: &'static [f32],
    /// Road wheel center height and radius at rest.
    pub road_y: f32,
    #[allow(dead_code)] // Checked against the art in tests; the suspension uses road_y.
    pub road_r: f32,
    /// End wheels as (z, y, radius).
    pub front: (f32, f32, f32),
    pub rear: (f32, f32, f32),
    /// Hull body box above the tracks.
    pub hull_half_x: f32,
    pub hull_top: f32,
    pub hull_z: (f32, f32),
    pub center_of_mass: Vec3,
}

impl Chassis {
    pub const LANCER: Chassis = Chassis {
        mass: 18_000.0,
        track_x: 1.33,
        track_width: 0.8,
        road_z: &[-1.27, -0.42, 0.42, 1.27],
        road_y: 0.38,
        road_r: 0.3,
        front: (1.8, 0.56, 0.34),
        rear: (-1.97, 0.62, 0.32),
        hull_half_x: 1.7,
        hull_top: 1.42,
        hull_z: (-2.4, 2.8),
        center_of_mass: Vec3::new(0.0, 0.65, -0.1),
    };
    pub const BASTION: Chassis = Chassis {
        mass: 28_000.0,
        track_x: 1.42,
        track_width: 0.92,
        road_z: &[-1.5, -0.75, 0.0, 0.75, 1.5],
        road_y: 0.4,
        road_r: 0.32,
        front: (1.86, 0.58, 0.32),
        rear: (-2.0, 0.7, 0.38),
        hull_half_x: 1.85,
        hull_top: 1.6,
        hull_z: (-2.5, 2.95),
        center_of_mass: Vec3::new(0.0, 0.7, -0.1),
    };

    pub fn of(kind: HullKind) -> &'static Chassis {
        match kind {
            HullKind::Lancer => &Chassis::LANCER,
            HullKind::Bastion => &Chassis::BASTION,
        }
    }

    pub fn wheel_count(&self) -> usize {
        self.road_z.len() * 2
    }

    /// Suspension mount for wheel `i` (left side first, then right).
    pub fn mount(&self, i: usize) -> Vec3 {
        let n = self.road_z.len();
        let x = if i < n { self.track_x } else { -self.track_x };
        Vec3::new(x, self.road_y + SUSPENSION_TRAVEL_UP, self.road_z[i % n])
    }

    /// Ray length from mount to ground at rest under load.
    pub fn rest_len(&self) -> f32 {
        self.road_y + SUSPENSION_TRAVEL_UP
    }

    /// Ray length with the spring fully extended.
    pub fn free_len(&self) -> f32 {
        self.rest_len() + SUSPENSION_SAG
    }

    /// Convex collider: belly above the road wheels' travel, ends swept up around the
    /// sprocket and idler like the real track run, hull box on top.
    pub fn collider_points(&self) -> Vec<Vec3> {
        let mut pts = Vec::new();
        let x = self.track_x + self.track_width * 0.5;
        // The road wheels (suspension rays) handle anything up to their travel.
        // The collider's bottom sits at the top of that travel, so kerbs and
        // small bumps go through the springs instead of popping off a hull corner.
        let belly = self.road_y - self.road_r + SUSPENSION_TRAVEL_UP;
        let (z_lo, z_hi) = (self.road_z[0], self.road_z[self.road_z.len() - 1]);
        for sx in [-x, x] {
            pts.push(Vec3::new(sx, belly, z_lo));
            pts.push(Vec3::new(sx, belly, z_hi));
            for k in 0..=6 {
                let a = -PI / 2.0 + PI * k as f32 / 6.0;
                let (fz, fy, fr) = self.front;
                pts.push(Vec3::new(sx, fy + fr * a.sin(), fz + fr * a.cos()));
                let (rz, ry, rr) = self.rear;
                pts.push(Vec3::new(sx, ry + rr * a.sin(), rz - rr * a.cos()));
            }
        }
        for sx in [-self.hull_half_x, self.hull_half_x] {
            for z in [self.hull_z.0, self.hull_z.1] {
                pts.push(Vec3::new(sx, self.hull_top, z));
            }
        }
        pts
    }

    fn inertia(&self) -> Vec3 {
        let s = Vec3::new(self.hull_half_x * 2.0, self.hull_top, self.hull_z.1 - self.hull_z.0);
        Vec3::new(s.y * s.y + s.z * s.z, s.x * s.x + s.z * s.z, s.x * s.x + s.y * s.y) * self.mass / 12.0
    }
}

/// How far a road wheel can rise above its resting position.
pub const SUSPENSION_TRAVEL_UP: f32 = 0.25;
/// Static compression under the tank's own weight (sets spring stiffness).
pub const SUSPENSION_SAG: f32 = 0.09;
const SUSPENSION_DAMPING_RATIO: f32 = 0.5;
/// Fraction of the drive force's lever arm removed by suspension geometry.
const ANTI_SQUAT: f32 = 0.6;
/// How far the tracks' commanded speed may lead the hull (m/s, rad/s).
const DRIVE_SLIP: f32 = 2.0;
const TURN_SLIP: f32 = 1.2;
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

/// Drive tuning. Grip numbers are friction coefficients (load-limited).
#[derive(Clone, Copy, Debug)]
pub struct HullStats {
    pub max_speed: f32,
    pub reverse_speed: f32,
    /// Peak acceleration from a standstill, m/s².
    pub accel: f32,
    /// Above this speed the engine is power-limited: accel falls off as 1/v.
    pub power_knee: f32,
    pub brake: f32,
    /// Deceleration with no throttle (tracks drag the tank to a stop).
    pub coast: f32,
    /// Pivot / turning rate, rad/s.
    pub turn_rate: f32,
    pub turn_accel: f32,
    /// Track grip along and across the direction of travel.
    pub grip_long: f32,
    pub grip_lat: f32,
    /// Sideways grip while steering (tracks scrub to let the hull swing).
    pub grip_lat_turning: f32,
    /// Max turret rotation speed relative to the hull, rad/s.
    pub turret_speed: f32,
    /// How fast the turret spins up to full speed, rad/s².
    pub turret_accel: f32,
}

impl HullStats {
    pub const LANCER: Self = Self {
        max_speed: 15.0,
        reverse_speed: 9.0,
        accel: 12.0,
        power_knee: 9.0,
        brake: 22.0,
        coast: 10.0,
        // Wider tracks (art pass) scrub more in a pivot; 2.45 keeps the old pivot speed.
        turn_rate: 2.45,
        turn_accel: 20.0,
        grip_long: 1.6,
        grip_lat: 1.3,
        grip_lat_turning: 0.7,
        turret_speed: 2.0,
        turret_accel: 12.0,
    };
    pub const BASTION: Self = Self {
        max_speed: 11.5,
        reverse_speed: 7.0,
        accel: 9.0,
        power_knee: 6.5,
        brake: 18.0,
        coast: 8.0,
        turn_rate: 1.7,
        turn_accel: 9.0,
        grip_long: 1.6,
        grip_lat: 1.4,
        grip_lat_turning: 0.85,
        turret_speed: 1.5,
        turret_accel: 8.0,
    };

    pub fn of(kind: HullKind) -> HullStats {
        match kind {
            HullKind::Lancer => HullStats::LANCER,
            HullKind::Bastion => HullStats::BASTION,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tank {
    /// Team color; tints the light armor plates.
    pub color: Vec3,
    pub hull_kind: HullKind,
    pub turret_kind: TurretKind,
    pub chassis: &'static Chassis,
    pub hull: HullStats,
    pub body: RigidBodyHandle,
    pub collider: ColliderHandle,

    // Mirrored from the rigid body after every step, for rendering and AI.
    pub pos: Vec3,
    pub rot: Quat,
    pub vel: Vec3,
    /// Road wheel displacement above its rest position, per wheel (left side first).
    pub wheel_lift: Vec<f32>,
    pub wheels_down: usize,
    /// Track contacts from the hull collider this step (sweep faces, belly).
    pub face_contacts: usize,
    /// Commanded track speeds [left, right], m/s.
    pub track_speed: [f32; 2],

    /// Turret angle relative to the hull: the turret is mounted, so it turns with the hull.
    pub turret_rel: f32,
    pub turret_rate: f32,
    /// Gun elevation in radians (positive = up).
    pub gun_pitch: f32,
    /// Visual-only gun stabilizer: it fights changes in hull pitch but lags
    /// and overshoots, so the elevation ram is always working a little.
    pub gun_sway: f32,
    pub gun_sway_vel: f32,
    /// Slow-following hull pitch: the stabilizer reacts to changes, not to
    /// standing on a slope.
    pitch_slow: f32,
    /// Distance each tread has travelled: [left (+x), right (-x)].
    pub track_dist: [f32; 2],
    /// Seconds since the gun last fired (drives recoil and muzzle flash).
    pub since_fire: f32,
    /// Which barrel fired last (twin guns alternate).
    pub fire_barrel: usize,
    pub flipped_time: f32,
    /// State at the start of the last step, for render interpolation.
    prev_pos: Vec3,
    prev_rot: Quat,
    prev_turret_rel: f32,
    prev_track_dist: [f32; 2],
    /// Speed the engine is running the tracks at (before the turning split).
    drive_speed: f32,
    drive_turn: f32,
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

    pub fn grounded(&self) -> bool {
        self.wheels_down + self.face_contacts > 0
    }

    fn snapshot(&mut self) {
        self.prev_pos = self.pos;
        self.prev_rot = self.rot;
        self.prev_turret_rel = self.turret_rel;
        self.prev_track_dist = self.track_dist;
    }

    /// The tank as it should be drawn `alpha` (0..1) of the way from the previous
    /// sim step to the current one. Rendering at a different rate than the 120 Hz
    /// sim without this shows up as a rhythmic judder.
    pub fn interpolated(&self, alpha: f32) -> Tank {
        let mut t = self.clone();
        t.pos = self.prev_pos.lerp(self.pos, alpha);
        t.rot = self.prev_rot.slerp(self.rot, alpha);
        t.turret_rel = self.prev_turret_rel + wrap_angle(self.turret_rel - self.prev_turret_rel) * alpha;
        for s in 0..2 {
            t.track_dist[s] = self.prev_track_dist[s] + (self.track_dist[s] - self.prev_track_dist[s]) * alpha;
        }
        t
    }
}

pub struct World {
    pub map: Map,
    pub tanks: Vec<Tank>,
    pub physics: PhysicsWorld,
    pub time: f32,
}

impl World {
    /// Map with the player's tank at spawn point `spawn`.
    pub fn new(map: Map, spawn: usize) -> World {
        let spot = map.open_spots[spawn % map.open_spots.len()];
        let yaw = map.spawn_yaw.unwrap_or_else(|| yaw_of(map.center() - spot));
        let mut w = World::empty(map);
        w.spawn_tank(Vec3::new(0.92, 0.93, 0.95), spot, yaw, HullKind::Lancer, TurretKind::Lance);
        w
    }

    /// Map with static colliders and no tanks.
    pub fn empty(map: Map) -> World {
        let mut physics = PhysicsWorld::default();
        physics.gravity = Vec3::new(0.0, -GRAVITY, 0.0);
        physics.integration_parameters.dt = DT;
        add_static_colliders(&mut physics, &map);
        // Initialize the broad phase so the first suspension raycasts see the ground.
        physics.step();
        World { map, tanks: Vec::new(), physics, time: 0.0 }
    }

    pub fn spawn_tank(&mut self, color: Vec3, at: Vec2, yaw: f32, hull: HullKind, turret: TurretKind) -> usize {
        let y = self.map.ground_height(at.x, at.y) + 0.15;
        let pose = Pose::from_parts(Vec3::new(at.x, y, at.y), Quat::from_rotation_y(yaw));
        let (body, collider) = insert_tank_body(&mut self.physics, Chassis::of(hull), pose);
        let chassis = Chassis::of(hull);
        let mut t = Tank {
            color,
            hull_kind: hull,
            turret_kind: turret,
            chassis,
            hull: HullStats::of(hull),
            body,
            collider,
            pos: Vec3::ZERO,
            rot: Quat::IDENTITY,
            vel: Vec3::ZERO,
            wheel_lift: vec![0.0; chassis.wheel_count()],
            wheels_down: 0,
            face_contacts: 0,
            track_speed: [0.0; 2],
            turret_rel: 0.0,
            turret_rate: 0.0,
            gun_pitch: 0.0,
            gun_sway: 0.0,
            gun_sway_vel: 0.0,
            pitch_slow: 0.0,
            track_dist: [0.0; 2],
            since_fire: 10.0,
            fire_barrel: 0,
            flipped_time: 0.0,
            prev_pos: Vec3::ZERO,
            prev_rot: Quat::IDENTITY,
            prev_turret_rel: 0.0,
            prev_track_dist: [0.0; 2],
            drive_speed: 0.0,
            drive_turn: 0.0,
        };
        sync_from_body(&mut t, &self.physics);
        t.snapshot();
        self.tanks.push(t);
        self.tanks.len() - 1
    }

    /// Swap a tank's hull in place (new body and running gear, same pose and speed).
    pub fn set_hull(&mut self, i: usize, kind: HullKind) {
        let t = &mut self.tanks[i];
        if t.hull_kind == kind {
            return;
        }
        let old = self.physics.remove_body_with_colliders(t.body, true).expect("tank body");
        let pose = Pose::from_parts(old.translation() + Vec3::Y * 0.3, *old.rotation());
        let chassis = Chassis::of(kind);
        let (body, collider) = insert_tank_body(&mut self.physics, chassis, pose);
        self.physics.bodies[body].set_linvel(old.linvel(), true);
        t.body = body;
        t.collider = collider;
        t.hull_kind = kind;
        t.chassis = chassis;
        t.hull = HullStats::of(kind);
        t.wheel_lift = vec![0.0; chassis.wheel_count()];
        sync_from_body(t, &self.physics);
        t.snapshot();
    }

    /// Fire tank `i`'s gun: recoil impulse on the hull plus the visual timers.
    pub fn fire(&mut self, i: usize) {
        let t = &mut self.tanks[i];
        t.since_fire = 0.0;
        t.fire_barrel += 1;
        t.gun_sway_vel += 1.4;
        let b = &mut self.physics.bodies[t.body];
        let muzzle_height = t.rot * Vec3::Y * (t.chassis.hull_top + 0.4);
        let at = b.translation() + muzzle_height;
        b.apply_impulse_at_point(-t.turret_dir() * t.turret_kind.recoil() * t.chassis.mass, at, true);
    }

    pub fn step(&mut self, cmds: &[Command]) {
        self.time += DT;
        for t in &mut self.tanks {
            t.snapshot();
        }
        for i in 0..self.tanks.len() {
            let cmd = cmds.get(i).copied().unwrap_or_default();
            let t = &mut self.tanks[i];
            drive(t, &cmd, &mut self.physics);
            turret(t, &cmd, DT);
            t.since_fire += DT;
        }
        self.physics.step();
        for t in &mut self.tanks {
            sync_from_body(t, &self.physics);
            recover_if_flipped(t, &mut self.physics);
            stabilizer(t, DT);
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

fn insert_tank_body(physics: &mut PhysicsWorld, c: &Chassis, pose: Pose) -> (RigidBodyHandle, ColliderHandle) {
    let body = RigidBodyBuilder::dynamic().pose(pose).angular_damping(0.3).ccd_enabled(true).build();
    let h = physics.insert_body(body);
    // Frictionless and bounceless: all grip comes from the track model.
    let collider = ColliderBuilder::convex_hull(&c.collider_points())
        .expect("tank hull shape")
        .mass_properties(MassProperties::new(c.center_of_mass, c.mass, c.inertia()))
        .friction(0.0)
        .friction_combine_rule(CoefficientCombineRule::Min)
        .restitution(0.0);
    let ch = physics.insert_collider(collider, Some(h));
    (h, ch)
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

/// Four rects surrounding the terrain rect [a, b], reaching well beyond the arena.
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

/// Visual gun stabilizer: a lagging spring that counters changes in hull pitch.
fn stabilizer(t: &mut Tank, dt: f32) {
    let pitch = t.forward().y.clamp(-1.0, 1.0).asin();
    t.pitch_slow += (pitch - t.pitch_slow) * (1.0 - (-dt * 2.0).exp());
    let target = -(pitch - t.pitch_slow) * 0.8;
    t.gun_sway_vel += ((target - t.gun_sway) * 70.0 - t.gun_sway_vel * 7.0) * dt;
    t.gun_sway = (t.gun_sway + t.gun_sway_vel * dt).clamp(-0.35, 0.35);
}

fn sync_from_body(t: &mut Tank, physics: &PhysicsWorld) {
    let b = &physics.bodies[t.body];
    t.pos = b.translation();
    t.rot = *b.rotation();
    t.vel = b.linvel();
}

/// A point where a track touches the world this step.
struct TrackContact {
    point: Vec3,
    normal: Vec3,
    /// Normal force, N.
    load: f32,
    /// 0 = left (+x), 1 = right.
    side: usize,
    /// Direction the track pushes the hull at this point when driving forward (world).
    push: Vec3,
    /// Friction coefficient at this contact.
    grip: f32,
}

impl Chassis {
    /// Direction the ground pushes the hull, at a point on the track belt, when the
    /// track drives forward (body frame). The belt runs backward along the bottom,
    /// up around the rear wheel, forward along the top and down around the front,
    /// so the push is forward on the bottom run, upward at the front of the front
    /// wheel and downward at the back of the rear wheel.
    pub fn belt_push(&self, local: Vec3) -> Vec3 {
        let (fz, fy, _) = self.front;
        let (rz, ry, _) = self.rear;
        let center = if local.z > fz {
            Vec2::new(fz, fy)
        } else if local.z < rz {
            Vec2::new(rz, ry)
        } else {
            return Vec3::Z;
        };
        // Angle around the wheel in the (z, y) plane.
        let phi = (local.y - center.y).atan2(local.z - center.x);
        Vec3::new(0.0, phi.cos(), -phi.sin())
    }
}

fn drive(t: &mut Tank, cmd: &Command, physics: &mut PhysicsWorld) {
    let c = t.chassis;
    let (pose, mass) = {
        let b = &physics.bodies[t.body];
        (*b.position(), b.mass())
    };
    let up = pose.rotation * Vec3::Y;
    let fwd = pose.rotation * Vec3::Z;
    let n_wheels = c.wheel_count();

    // 1. Suspension rays (one per road wheel).
    let mut ray_hits: Vec<Option<(Vec3, Vec3, f32)>> = vec![None; n_wheels];
    {
        let filter = QueryFilter::default().exclude_rigid_body(t.body);
        let qp = physics.query_pipeline_with_filter(filter);
        for (i, hit) in ray_hits.iter_mut().enumerate() {
            let origin = pose.transform_point(c.mount(i));
            if let Some((_, h)) = qp.cast_ray_and_get_normal(&Ray::new(origin, -up), c.free_len(), true) {
                *hit = Some((origin, h.normal, h.time_of_impact));
            }
        }
    }

    // 2. Hull contacts on the track faces, with their loads from the last solve.
    let mut contacts: Vec<TrackContact> = Vec::new();
    let track_inner = c.track_x - c.track_width * 0.5 - 0.05;
    for pair in physics.contact_pairs_with(t.collider) {
        let PairContacts::Rigid(rc) = &pair.contacts else { continue };
        let ours_first = pair.collider1 == t.collider;
        for m in rc.solver_manifolds() {
            // Manifold normal points from collider1 to collider2; we want it pointing into the tank.
            let normal = if ours_first { -m.data.normal } else { m.data.normal };
            // Only the belt surface (bottom run, the curves around the end wheels,
            // the top run) can drive. The outer side of a track is a plain slab:
            // touching a wall with it must not drive the tank up the wall.
            let normal_local = pose.rotation.inverse() * normal;
            if normal_local.x.abs() > 0.5 {
                continue;
            }
            for p in &m.points {
                let local = if ours_first { p.local_p1 } else { p.local_p2 };
                let on_track = local.x.abs() >= track_inner && local.y < 1.1;
                if !on_track || p.data.impulse <= 0.0 {
                    continue;
                }
                contacts.push(TrackContact {
                    point: pose.transform_point(local),
                    normal,
                    // A collision makes a huge, brief contact force. That's the
                    // physics engine's job; traction only ever sees weight-scale
                    // loads, or one hard bump would fling the tank with the belt.
                    load: (p.data.impulse / DT).min(mass * GRAVITY),
                    side: if local.x > 0.0 { 0 } else { 1 },
                    push: pose.rotation * c.belt_push(local),
                    grip: t.hull.grip_long,
                });
            }
        }
    }
    t.face_contacts = contacts.len();

    // Spring forces; ray contacts join the traction list with the spring load.
    let k = mass * GRAVITY / (n_wheels as f32 * SUSPENSION_SAG);
    let damp = 2.0 * (k * mass / n_wheels as f32).sqrt() * SUSPENSION_DAMPING_RATIO;
    let b = &mut physics.bodies[t.body];
    t.wheels_down = 0;
    for (i, hit) in ray_hits.iter().enumerate() {
        match hit {
            Some((origin, n, toi)) => {
                t.wheels_down += 1;
                t.wheel_lift[i] = (c.rest_len() - toi).clamp(-SUSPENSION_SAG, SUSPENSION_TRAVEL_UP);
                let compression = c.free_len() - toi;
                // Bump stop: much stiffer once the wheel runs out of travel.
                let over = (compression - SUSPENSION_SAG - SUSPENSION_TRAVEL_UP).max(0.0);
                let v_along = b.velocity_at_point(*origin).dot(up);
                let force = (k * compression + k * 8.0 * over - damp * v_along).max(0.0);
                b.apply_impulse_at_point(up * force * DT, *origin, true);
                contacts.push(TrackContact {
                    point: *origin - up * *toi,
                    normal: *n,
                    load: force * n.dot(up).max(0.0),
                    side: if i < n_wheels / 2 { 0 } else { 1 },
                    push: fwd,
                    grip: t.hull.grip_long,
                });
            }
            None => t.wheel_lift[i] = -SUSPENSION_SAG,
        }
    }

    // A wall or edge can only push back on the track faces as hard as the tracks
    // on the ground drive the tank into it (or the tank's weight, if airborne).
    // Without this, pushing up an edge pitches the nose into it, which raises the
    // contact load, which allows more push: a runaway that walks tanks up walls.
    let ground_load: f32 = contacts.iter().skip(t.face_contacts).map(|c| c.load).sum();
    let face_budget = if ground_load > 0.0 { t.hull.grip_long * ground_load } else { mass * GRAVITY };
    let face_load: f32 = contacts.iter().take(t.face_contacts).map(|c| c.load).sum();
    if face_load > face_budget {
        let scale = face_budget / face_load;
        for ct in contacts.iter_mut().take(t.face_contacts) {
            ct.load *= scale;
        }
    }

    // 3. What the tracks are trying to do: forward speed plus a speed split for turning.
    let h = t.hull;
    let throttle = cmd.throttle.clamp(-1.0, 1.0);
    let v_now = b.linvel().dot(fwd);
    let target = if throttle >= 0.0 { throttle * h.max_speed } else { throttle * h.reverse_speed };
    let rate = if throttle.abs() < 0.05 {
        h.coast
    } else if v_now * target < 0.0 {
        h.brake
    } else {
        // Power-limited: full pull from a standstill, tapering toward top speed.
        h.accel * (h.power_knee / v_now.abs().max(h.power_knee))
    };
    // The tracks have their own speed, driven by the engine. They may run ahead of
    // the hull (that slip is what pushes), but only within a window, so a blocked
    // or airborne tank doesn't wind up a huge target.
    t.drive_speed = approach(t.drive_speed, target, rate * DT).clamp(v_now - DRIVE_SLIP, v_now + DRIVE_SLIP);
    // Tank steering: A/D always rotate the hull the same way, reversing or not.
    let steer = cmd.steer;
    let speed_frac = (v_now.abs() / h.max_speed).min(1.0);
    let turn_target = steer.clamp(-1.0, 1.0) * h.turn_rate * (1.0 - 0.25 * speed_frac);
    let yaw_now = b.angvel().dot(up);
    t.drive_turn = approach(t.drive_turn, turn_target, h.turn_accel * DT).clamp(yaw_now - TURN_SLIP, yaw_now + TURN_SLIP);
    // Left track (+x) runs slower to turn left (counter-clockwise).
    let half_span = c.track_x;
    t.track_speed = [t.drive_speed - t.drive_turn * half_span, t.drive_speed + t.drive_turn * half_span];
    t.track_dist[0] += t.track_speed[0] * DT;
    t.track_dist[1] += t.track_speed[1] * DT;

    if contacts.is_empty() {
        return;
    }

    // 4. Traction: each contact pushes its patch toward the track speed along the
    // track's direction at that point, and resists sliding sideways, each limited
    // by friction times the contact's load.
    let turning = turn_target.abs() > 0.05;
    let grip_lat = if turning { h.grip_lat_turning } else { h.grip_lat };
    // Velocities are sampled before any impulse is applied, so every contact
    // sees the same slip and together they deliver exactly the requested change.
    let share = mass / contacts.len() as f32;
    let impulses: Vec<(Vec3, Vec3)> = contacts
        .iter()
        .filter_map(|ct| {
            // Track push direction at the contact, kept in the contact surface.
            let tangent = (ct.push - ct.normal * ct.push.dot(ct.normal)).normalize_or_zero();
            if tangent == Vec3::ZERO {
                return None;
            }
            let lateral = ct.normal.cross(tangent).normalize_or_zero();
            let v = b.velocity_at_point(ct.point);
            let max_long = ct.grip * ct.load * DT;
            let max_lat = grip_lat * ct.load * DT;
            let j_long = (share * (t.track_speed[ct.side] - v.dot(tangent))).clamp(-max_long, max_long);
            let j_lat = (-share * v.dot(lateral)).clamp(-max_lat, max_lat);
            Some((tangent * j_long + lateral * j_lat, ct.point))
        })
        .collect();
    let com = b.center_of_mass();
    // Anti-squat: track and suspension geometry carries part of the drive force
    // up toward the center of mass, so hard acceleration squats the hull instead
    // of standing it on its tail. 0 = force at the ground, 1 = at the CoM.
    for (j, p) in impulses {
        let lift = (com - p).dot(up).max(0.0) * ANTI_SQUAT;
        b.apply_impulse_at_point(j, p + up * lift, true);
    }
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
        let pos = b.translation() + Vec3::Y * 2.0;
        b.set_position(Pose::from_parts(pos, Quat::from_rotation_y(t.yaw())), true);
        b.set_linvel(Vec3::ZERO, true);
        b.set_angvel(Vec3::ZERO, true);
        sync_from_body(t, physics);
        t.snapshot();
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
pub(crate) mod tests_support {
    use super::*;

    pub fn flat_world() -> World {
        let mut map = Map::proving_ground();
        map.terrain = None;
        map.wedges.clear();
        map.solids.retain(|s| s.max.x < 1.0 || s.min.x > 199.0 || s.max.z < 1.0 || s.min.z > 199.0);
        map.boxes.clear();
        map.open_spots = vec![Vec2::new(100.0, 100.0)];
        World::new(map, 0)
    }

    pub fn run(w: &mut World, cmd: Command, seconds: f32) {
        for _ in 0..(seconds / DT) as usize {
            w.step(&[cmd]);
        }
    }

    pub fn place(w: &mut World, pos: Vec3, rot: Quat) {
        let b = &mut w.physics.bodies[w.tanks[0].body];
        b.set_position(Pose::from_parts(pos, rot), true);
        b.set_linvel(Vec3::ZERO, true);
        b.set_angvel(Vec3::ZERO, true);
    }

    /// Flat world plus one block whose near face is at z=110.
    pub fn ledge_world(height: f32) -> World {
        let mut w = flat_world();
        let (min, max) = (Vec3::new(85.0, 0.0, 110.0), Vec3::new(115.0, height, 140.0));
        let half = (max - min) * 0.5;
        w.physics.insert_collider(ColliderBuilder::cuboid(half.x, half.y, half.z).translation(min + half), None);
        w.physics.step();
        w
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::*;
    use super::*;
    use crate::map::{Rise, Terrain};




    fn throttle(x: f32) -> Command {
        Command { throttle: x, ..Default::default() }
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
    fn settles_level_on_its_wheels() {
        for hull in [HullKind::Lancer, HullKind::Bastion] {
            let mut w = flat_world();
            w.set_hull(0, hull);
            run(&mut w, Command::default(), 3.0);
            let t = &w.tanks[0];
            assert_eq!(t.wheels_down, t.chassis.wheel_count(), "{hull:?}");
            // Body origin is the ground plane at rest: treads on the ground.
            assert!(t.pos.y.abs() < 0.03, "{hull:?} ride height {}", t.pos.y);
            assert!(t.up().y > 0.999, "{hull:?} not level: {:?}", t.up());
            let p0 = t.pos;
            run(&mut w, Command::default(), 1.0);
            assert!(w.tanks[0].pos.distance(p0) < 0.005, "{hull:?} still moving");
        }
    }

    #[test]
    fn accelerates_hard_then_tapers_to_top_speed() {
        let mut w = flat_world();
        run(&mut w, Command::default(), 1.0);
        run(&mut w, throttle(1.0), 0.5);
        let early = w.tanks[0].speed();
        assert!(early > 5.0, "sluggish start: {early} m/s after 0.5 s");
        assert!(w.tanks[0].up().y > 0.98, "wheelie: {:?}", w.tanks[0].up());
        run(&mut w, throttle(1.0), 2.5);
        let s = w.tanks[0].speed();
        assert!((s - HullStats::LANCER.max_speed).abs() < 0.5, "top speed {s}");
    }

    #[test]
    fn stops_quickly() {
        let mut w = flat_world();
        run(&mut w, Command::default(), 0.5);
        run(&mut w, throttle(1.0), 3.0);
        let z0 = w.tanks[0].pos.z;
        let mut furthest = z0;
        for _ in 0..(2.0 / DT) as usize {
            w.step(&[throttle(-1.0)]);
            furthest = furthest.max(w.tanks[0].pos.z);
        }
        let stop_dist = furthest - z0;
        assert!(stop_dist < 8.0, "braking from top speed took {stop_dist} m");
    }

    #[test]
    fn pivots_in_place() {
        let mut w = flat_world();
        run(&mut w, Command::default(), 1.0);
        let (p0, yaw0) = (w.tanks[0].pos, w.tanks[0].yaw());
        let mut turned = 0.0;
        let mut last = yaw0;
        for _ in 0..(1.6 / DT) as usize {
            w.step(&[Command { steer: 1.0, ..Default::default() }]);
            let y = w.tanks[0].yaw();
            turned += wrap_angle(y - last);
            last = y;
        }
        assert!(turned > PI, "only turned {:.0} degrees in 1.6 s", turned.to_degrees());
        let drift = Vec2::new(w.tanks[0].pos.x - p0.x, w.tanks[0].pos.z - p0.z).length();
        assert!(drift < 0.6, "pivot wandered {drift} m");
    }




    #[test]
    fn holds_position_on_slope() {
        let mut map = Map::proving_ground();
        // A uniform 20 degree slope rising along +x; tank parked across it.
        let grade = 20f32.to_radians().tan();
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
        assert!(drift < 0.2, "slid {drift} m down the slope");
    }

    #[test]
    fn jumps_and_lands_upright() {
        let mut w = flat_world();
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
        place(&mut w, Vec3::new(100.0, 0.2, 70.0), Quat::IDENTITY);
        run(&mut w, Command::default(), 1.0);

        let mut airborne = 0.0f32;
        let mut max_height = 0.0f32;
        for _ in 0..(6.0 / DT) as usize {
            w.step(&[throttle(1.0)]);
            let t = &w.tanks[0];
            if !t.grounded() {
                airborne += DT;
            }
            max_height = max_height.max(t.pos.y);
        }
        assert!(airborne > 0.3, "barely left the ground: {airborne}s");
        assert!(max_height > 2.0, "jump too low: {max_height}");
        run(&mut w, Command::default(), 2.0);
        let t = &w.tanks[0];
        assert!(t.up().y > 0.95, "landed badly: up = {:?}", t.up());
        assert!(t.pos.y.abs() < 0.3, "not resting on the ground: {}", t.pos.y);
    }

    #[test]
    fn every_kicker_lane_is_clearable_at_full_speed() {
        for lane in 0..3 {
            let mut w = World::new(Map::proving_ground(), lane);
            run(&mut w, throttle(1.0), 6.0);
            let t = &w.tanks[0];
            assert!(t.pos.z > 80.0, "lane {lane}: stopped at z={} (crashed into the landing ramp?)", t.pos.z);
            assert!(t.up().y > 0.95, "lane {lane}: not upright");
        }
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
            place(&mut w, Vec3::new(spot.x, 0.2, spot.y), Quat::IDENTITY);
            run(&mut w, Command::default(), 1.0);
            run(&mut w, throttle(1.0), 4.0);
            let t = &w.tanks[0];
            assert!(t.pos.z > spot.y + 30.0, "stuck at z={} starting from {spot}", t.pos.z);
            assert!(t.up().y > 0.8, "tipped over at {spot}");
        }
    }


    /// Drive at a ledge from `start` (rotated by `yaw`); true if the tank ends up on top.
    fn climbs(height: f32, start: Vec2, yaw: f32, hull: HullKind) -> bool {
        let mut w = ledge_world(height);
        w.set_hull(0, hull);
        place(&mut w, Vec3::new(start.x, 0.2, start.y), Quat::from_rotation_y(yaw));
        run(&mut w, Command::default(), 0.5);
        // Success: at some point the tank is sitting upright on top of the block.
        (0..(6.0 / DT) as usize).any(|_| {
            w.step(&[throttle(1.0)]);
            let t = &w.tanks[0];
            t.pos.y > height - 0.2 && t.pos.z > 112.0 && t.up().y > 0.9 && t.wheels_down > 0
        })
    }

    /// Kerbs, steps and low ledges never stop a tank, from a standstill or at speed.
    #[test]
    fn low_ledges_never_stop_you() {
        for hull in [HullKind::Lancer, HullKind::Bastion] {
            for h in [0.25, 0.5, 0.8] {
                assert!(climbs(h, Vec2::new(100.0, 106.5), 0.0, hull), "{hull:?} can't climb {h} m from a standstill");
                assert!(climbs(h, Vec2::new(100.0, 80.0), 0.0, hull), "{hull:?} can't climb {h} m with a run-up");
            }
            // Hitting a kerb or step at an angle.
            assert!(climbs(0.4, Vec2::new(90.0, 100.0), 0.6, hull), "{hull:?} stuck on a diagonal step");
        }
    }

    /// Something clearly wall-like (taller than the hull) still blocks.
    #[test]
    fn walls_still_block() {
        assert!(!climbs(2.0, Vec2::new(100.0, 80.0), 0.0, HullKind::Lancer), "drove up a wall");
    }

    #[test]
    fn crosses_stairs_side_on() {
        // The stairs block from the proving ground, approached from its side.
        let mut w = World::new(Map::proving_ground(), 0);
        // At z=121.5 the side face is the first two steps: 0.25 m, then 0.5 m.
        place(&mut w, Vec3::new(20.0, 0.2, 121.5), Quat::from_rotation_y(PI / 2.0));
        run(&mut w, Command::default(), 0.5);
        run(&mut w, throttle(1.0), 5.0);
        let t = &w.tanks[0];
        assert!(t.pos.x > 34.0, "stuck at x={} climbing the stairs side-on", t.pos.x);
        assert!(t.up().y > 0.8, "tipped over");
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
    fn interpolation_blends_between_steps() {
        let mut w = flat_world();
        run(&mut w, throttle(1.0), 1.0);
        let t = &w.tanks[0];
        let (a, b) = (t.interpolated(0.0), t.interpolated(1.0));
        assert_eq!(a.pos, t.prev_pos);
        assert_eq!(b.pos, t.pos);
        let mid = t.interpolated(0.5).pos;
        assert!((mid - (t.prev_pos + t.pos) * 0.5).length() < 1e-5);
        assert!(t.pos.distance(t.prev_pos) > 0.01, "moving tank should differ between steps");
    }

    /// A always rotates the hull counter-clockwise, D clockwise, whether driving
    /// forward, reversing, or standing still.
    #[test]
    fn steering_direction_ignores_throttle() {
        for throttle in [1.0, 0.0, -1.0] {
            let mut w = flat_world();
            run(&mut w, Command::default(), 0.5);
            run(&mut w, Command { throttle, ..Default::default() }, 1.0);
            let yaw0 = w.tanks[0].yaw();
            run(&mut w, Command { throttle, steer: 1.0, ..Default::default() }, 0.5);
            let turned = wrap_angle(w.tanks[0].yaw() - yaw0);
            assert!(turned > 0.2, "steer left with throttle {throttle} turned {turned} rad");
        }
    }

    #[test]
    fn rights_itself_when_flipped() {
        let mut w = flat_world();
        place(&mut w, Vec3::new(100.0, 2.5, 100.0), Quat::from_rotation_z(PI));
        run(&mut w, Command::default(), 1.0);
        assert!(w.tanks[0].up().y < 0.0, "should be upside down first");
        run(&mut w, Command::default(), 4.0);
        assert!(w.tanks[0].up().y > 0.95, "did not recover: {:?}", w.tanks[0].up());
    }

    #[test]
    fn firing_kicks_the_hull_back() {
        let mut w = flat_world();
        run(&mut w, Command::default(), 1.0);
        w.fire(0);
        let mut pitch = 0.0f32;
        for _ in 0..60 {
            w.step(&[Command::default()]);
            pitch = pitch.max(w.tanks[0].forward().y.asin().to_degrees());
        }
        // Lance fires forward: the nose kicks up as the hull rocks back.
        assert!(pitch > 5.0, "weak recoil: hull only pitched {pitch} deg");
    }
}








