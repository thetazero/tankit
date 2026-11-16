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
    /// Ithessar: rides a plasma cushion instead of tracks.
    Sethuun,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurretKind {
    /// Imperium rail cannon.
    Lance,
    /// Imperium twin autocannon.
    Maul,
    /// Ithessar plasma orb that bounces off walls.
    Ammeth,
    /// Ithessar three-barrel rapid plasma.
    Ossrin,
    /// Machine laser beam; overheats.
    Lumen,
    /// Resonant short-range crystal flamethrower.
    Hymn,
}

impl HullKind {
    pub fn maker(self) -> crate::faction::Faction {
        use crate::faction::Faction;
        match self {
            HullKind::Lancer | HullKind::Bastion => Faction::Imperium,
            HullKind::Sethuun => Faction::Ithessar,
        }
    }
}

impl TurretKind {
    /// Every turret, grouped by maker. Also the kit's art order.
    pub const ALL: [TurretKind; 6] = [
        TurretKind::Lance,
        TurretKind::Maul,
        TurretKind::Ammeth,
        TurretKind::Ossrin,
        TurretKind::Lumen,
        TurretKind::Hymn,
    ];

    pub fn maker(self) -> crate::faction::Faction {
        use crate::faction::Faction;
        match self {
            TurretKind::Lance | TurretKind::Maul => Faction::Imperium,
            TurretKind::Ammeth | TurretKind::Ossrin => Faction::Ithessar,
            TurretKind::Lumen => Faction::Machines,
            TurretKind::Hymn => Faction::Resonant,
        }
    }

    /// Velocity change one shot kicks into the hull, m/s. It's applied at gun
    /// height, so most of it shows up as the hull rocking back on its springs.
    pub fn recoil(self) -> f32 {
        match self {
            // Long rail cannon: one big slam.
            TurretKind::Lance => 2.2,
            // Twin short guns: smaller kicks, more often.
            TurretKind::Maul => 1.3,
            // Plasma has little mass: a soft push.
            TurretKind::Ammeth => 0.8,
            TurretKind::Ossrin => 0.35,
            // Beam and flame don't kick.
            TurretKind::Lumen | TurretKind::Hymn => 0.0,
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
    /// Floats on plasma emitters instead of running on tracks. The emitters sit
    /// where road wheels would (`track_x` x `road_z`), `road_y` above the ground.
    pub hover: bool,
}

impl Chassis {
    pub const LANCER: Chassis = Chassis {
        mass: 18_000.0,
        track_x: 1.22,
        track_width: 0.56,
        road_z: &[-1.35, -0.45, 0.45, 1.35],
        road_y: 0.38,
        road_r: 0.3,
        front: (2.05, 0.66, 0.36),
        rear: (-2.1, 0.62, 0.32),
        hull_half_x: 1.45,
        hull_top: 1.42,
        hull_z: (-2.3, 2.2),
        center_of_mass: Vec3::new(0.0, 0.65, -0.1),
        hover: false,
    };
    pub const BASTION: Chassis = Chassis {
        mass: 28_000.0,
        track_x: 1.28,
        track_width: 0.64,
        road_z: &[-1.6, -0.8, 0.0, 0.8, 1.6],
        road_y: 0.4,
        road_r: 0.32,
        front: (2.15, 0.68, 0.33),
        rear: (-2.15, 0.7, 0.38),
        hull_half_x: 1.55,
        hull_top: 1.62,
        hull_z: (-2.5, 2.4),
        center_of_mass: Vec3::new(0.0, 0.7, -0.1),
        hover: false,
    };
    /// Ithessar hover hull: light and long, its belly riding 0.7 m up on six emitters.
    /// No tracks: `front`/`rear` are only the collider's prow and tail.
    pub const SETHUUN: Chassis = Chassis {
        mass: 13_000.0,
        track_x: 1.0,
        track_width: 0.5,
        road_z: &[-1.75, 0.05, 1.7],
        road_y: 1.1,
        road_r: 0.0,
        front: (2.75, 1.25, 0.0),
        rear: (-2.55, 1.3, 0.0),
        hull_half_x: 1.4,
        hull_top: 1.7,
        hull_z: (-2.3, 2.1),
        center_of_mass: Vec3::new(0.0, 1.25, 0.0),
        hover: true,
    };

    pub fn of(kind: HullKind) -> &'static Chassis {
        match kind {
            HullKind::Lancer => &Chassis::LANCER,
            HullKind::Bastion => &Chassis::BASTION,
            HullKind::Sethuun => &Chassis::SETHUUN,
        }
    }

    pub fn wheel_count(&self) -> usize {
        self.road_z.len() * 2
    }

    /// Suspension mount for wheel `i` (left side first, then right).
    pub fn mount(&self, i: usize) -> Vec3 {
        let n = self.road_z.len();
        let x = if i < n { self.track_x } else { -self.track_x };
        Vec3::new(x, self.rest_len(), self.road_z[i % n])
    }

    /// Ray length from mount to ground at rest under load.
    pub fn rest_len(&self) -> f32 {
        if self.hover {
            self.road_y
        } else {
            self.road_y + SUSPENSION_TRAVEL_UP
        }
    }

    /// Ray length with the spring fully extended.
    pub fn free_len(&self) -> f32 {
        self.rest_len()
            + if self.hover {
                HOVER_SAG
            } else {
                SUSPENSION_SAG
            }
    }

    /// Convex collider: belly above the road wheels' travel, ends swept up around the
    /// sprocket and idler like the real track run, hull box on top.
    pub fn collider_points(&self) -> Vec<Vec3> {
        if self.hover {
            return self.hover_collider_points();
        }
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

    /// A sled: flat belly just above the emitters' working range, a long
    /// raked prow (so ledges and wrecks push it up instead of stopping it
    /// dead) and a shorter rake at the tail.
    fn hover_collider_points(&self) -> Vec<Vec3> {
        let belly = self.road_y - HOVER_BELLY_GAP;
        let (fz, fy, _) = self.front;
        let (rz, ry, _) = self.rear;
        let (z_lo, z_hi) = (
            self.road_z[0] - 0.3,
            self.road_z[self.road_z.len() - 1] + 0.1,
        );
        let mut pts = Vec::new();
        for sx in [-1.0, 1.0] {
            let w = self.hull_half_x;
            pts.push(Vec3::new(sx * w * 0.7, belly, z_lo));
            pts.push(Vec3::new(sx * w * 0.7, belly, z_hi));
            pts.push(Vec3::new(sx * w, belly + 0.25, z_hi - 0.4));
            pts.push(Vec3::new(sx * w, belly + 0.25, z_lo + 0.2));
            pts.push(Vec3::new(sx * 0.35, fy, fz));
            pts.push(Vec3::new(sx * w * 0.6, ry, rz));
            for z in [self.hull_z.0, self.hull_z.1] {
                pts.push(Vec3::new(sx * w * 0.8, self.hull_top, z));
            }
        }
        pts
    }

    fn inertia(&self) -> Vec3 {
        let s = Vec3::new(
            self.hull_half_x * 2.0,
            self.hull_top,
            self.hull_z.1 - self.hull_z.0,
        );
        Vec3::new(
            s.y * s.y + s.z * s.z,
            s.x * s.x + s.z * s.z,
            s.x * s.x + s.y * s.y,
        ) * self.mass
            / 12.0
    }
}

/// How far a road wheel can rise above its resting position.
pub const SUSPENSION_TRAVEL_UP: f32 = 0.25;
/// Static compression under the tank's own weight (sets spring stiffness).
pub const SUSPENSION_SAG: f32 = 0.09;
const SUSPENSION_DAMPING_RATIO: f32 = 0.5;
/// Fraction of the drive force's lever arm removed by suspension geometry.
const ANTI_SQUAT: f32 = 0.6;
/// Drivetrain inertia seen at each belt, as a fraction of the tank's mass.
const BELT_MASS_FRAC: f32 = 0.08;
/// Below this speed with no input, the parking brake holds the tank.
const PARK_SPEED: f32 = 0.3;
/// Hover emitters: static compression under the hull's weight (soft: it bobs).
pub const HOVER_SAG: f32 = 0.16;
const HOVER_DAMPING_RATIO: f32 = 0.45;
/// How far the belly sits above the emitters' rest line: their working range
/// before the hull itself touches.
const HOVER_BELLY_GAP: f32 = 0.42;
/// Beyond full extension the cushion still steers and thrusts, fading out
/// over this much more height (cresting a dune doesn't cut the drive).
const HOVER_REACH: f32 = 0.9;
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
    /// Start swinging the turret back to face forward over the hull (C). A tap is
    /// enough: the swing keeps going on its own until it lands or Z/X cancels it.
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
    /// Brake force, as a deceleration (m/s²).
    pub brake: f32,
    /// Rolling resistance and engine drag while coasting, as a deceleration (m/s²).
    pub rolling: f32,
    /// Turn rate the steering drive aims for, rad/s.
    pub turn_rate: f32,
    /// Steering drive force, as an acceleration of the belts' speed difference (m/s²).
    pub steer_accel: f32,
    /// Track friction coefficients: along the belt (traction) and across it
    /// (sideways sliding resistance). Both share one limit per contact.
    pub grip_long: f32,
    pub grip_lat: f32,
    /// Max turret rotation speed relative to the hull, rad/s.
    pub turret_speed: f32,
    /// How fast the turret spins up to full speed, rad/s².
    pub turret_accel: f32,
    /// How fast it stops when you let go (or reverse), rad/s². Quicker than the
    /// spin-up so the turret has weight but still stops where you aim.
    pub turret_brake: f32,
}

impl HullStats {
    pub const LANCER: Self = Self {
        max_speed: 13.0,
        reverse_speed: 7.5,
        accel: 11.0,
        power_knee: 8.0,
        brake: 24.0,
        rolling: 1.4,
        turn_rate: 3.0,
        steer_accel: 20.0,
        grip_long: 1.9,
        grip_lat: 1.7,
        turret_speed: 1.3,
        turret_accel: 2.6,
        turret_brake: 10.0,
    };
    pub const BASTION: Self = Self {
        max_speed: 10.0,
        reverse_speed: 6.0,
        accel: 7.5,
        power_knee: 5.5,
        brake: 20.0,
        rolling: 1.4,
        turn_rate: 2.1,
        steer_accel: 14.0,
        grip_long: 1.9,
        grip_lat: 1.8,
        turret_speed: 0.95,
        turret_accel: 1.6,
        turret_brake: 7.0,
    };

    /// Hover: fast and light, but nothing grips the ground. Thrust and
    /// braking are accelerations; `grip_lat` is how hard the cushion can stop
    /// a sideways slide (m/s²), so it drifts through turns; `steer_accel` is
    /// how fast it spins up a turn (rad/s²). `grip_long` is unused.
    pub const SETHUUN: Self = Self {
        max_speed: 16.5,
        reverse_speed: 10.0,
        accel: 10.0,
        power_knee: 9.0,
        brake: 13.0,
        rolling: 0.9,
        turn_rate: 2.5,
        steer_accel: 11.0,
        grip_long: 0.0,
        grip_lat: 8.5,
        turret_speed: 1.6,
        turret_accel: 4.0,
        turret_brake: 12.0,
    };

    pub fn of(kind: HullKind) -> HullStats {
        match kind {
            HullKind::Lancer => HullStats::LANCER,
            HullKind::Bastion => HullStats::BASTION,
            HullKind::Sethuun => HullStats::SETHUUN,
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
    /// Belt speed of each track relative to the hull [left, right], m/s.
    pub belt: [f32; 2],
    /// Parking brake engaged (holds the belts until there's input).
    pub parked: bool,
    /// How hard the driver is working the engine, 0..1 (last step's throttle or steering).
    pub drive: f32,

    /// Turret angle relative to the hull: the turret is mounted, so it turns with the hull.
    pub turret_rel: f32,
    pub turret_rate: f32,
    /// An automatic recenter (C) is under way.
    pub turret_centering: bool,
    /// Gun elevation in radians (positive = up).
    pub gun_pitch: f32,
    /// Distance each tread has travelled: [left (+x), right (-x)].
    pub track_dist: [f32; 2],
    /// Seconds since the gun last fired (drives recoil and muzzle flash).
    pub since_fire: f32,
    /// Which barrel fired last (twin guns alternate).
    pub fire_barrel: usize,
    pub flipped_time: f32,
    /// Seconds since the tank was spawned (drives idle animation).
    pub age: f32,
    /// Destroyed: a hover hull's field is out and it falls.
    pub dead: bool,
    /// Hover hulls bank into turns (radians, + rolls right side down). Visual.
    pub lean: f32,
    /// Spin-up guns: how far spun up, 0..1, and the cluster's angle.
    pub spool: f32,
    pub spin: f32,
    /// State at the start of the last step, for render interpolation.
    prev_pos: Vec3,
    prev_rot: Quat,
    prev_turret_rel: f32,
    prev_track_dist: [f32; 2],
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

    /// Where the gun points in the world, including elevation.
    pub fn gun_dir(&self) -> Vec3 {
        self.rot
            * (Quat::from_rotation_y(self.turret_rel)
                * Quat::from_rotation_x(-self.gun_pitch)
                * Vec3::Z)
    }

    /// Height of the gun's pivot above the hull origin.
    pub fn gun_height(&self) -> f32 {
        self.chassis.hull_top + 0.4
    }

    /// Turret heading on the ground plane.
    pub fn turret_yaw(&self) -> f32 {
        let d = self.turret_dir();
        if Vec2::new(d.x, d.z).length_squared() < 1e-4 {
            self.yaw()
        } else {
            yaw_of(Vec2::new(d.x, d.z))
        }
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
        t.turret_rel =
            self.prev_turret_rel + wrap_angle(self.turret_rel - self.prev_turret_rel) * alpha;
        for s in 0..2 {
            t.track_dist[s] =
                self.prev_track_dist[s] + (self.track_dist[s] - self.prev_track_dist[s]) * alpha;
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
        w.spawn_tank(
            Vec3::new(0.92, 0.93, 0.95),
            spot,
            yaw,
            HullKind::Lancer,
            TurretKind::Lance,
        );
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
        World {
            map,
            tanks: Vec::new(),
            physics,
            time: 0.0,
        }
    }

    pub fn spawn_tank(
        &mut self,
        color: Vec3,
        at: Vec2,
        yaw: f32,
        hull: HullKind,
        turret: TurretKind,
    ) -> usize {
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
            belt: [0.0; 2],
            drive: 0.0,
            parked: true,
            turret_rel: 0.0,
            turret_rate: 0.0,
            turret_centering: false,
            gun_pitch: 0.0,
            track_dist: [0.0; 2],
            since_fire: 10.0,
            fire_barrel: 0,
            flipped_time: 0.0,
            age: 0.0,
            dead: false,
            lean: 0.0,
            spool: 0.0,
            spin: 0.0,
            prev_pos: Vec3::ZERO,
            prev_rot: Quat::IDENTITY,
            prev_turret_rel: 0.0,
            prev_track_dist: [0.0; 2],
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
        let old = self
            .physics
            .remove_body_with_colliders(t.body, true)
            .expect("tank body");
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
        let b = &mut self.physics.bodies[t.body];
        let muzzle_height = t.rot * Vec3::Y * (t.chassis.hull_top + 0.4);
        let at = b.translation() + muzzle_height;
        b.apply_impulse_at_point(
            -t.turret_dir() * t.turret_kind.recoil() * t.chassis.mass,
            at,
            true,
        );
    }

    /// Put tank `i` down at `at` facing `yaw`, at rest, turret forward (respawns).
    pub fn teleport(&mut self, i: usize, at: Vec2, yaw: f32) {
        let y = self.map.ground_height(at.x, at.y) + 0.15;
        let t = &mut self.tanks[i];
        let b = &mut self.physics.bodies[t.body];
        b.set_position(
            Pose::from_parts(Vec3::new(at.x, y, at.y), Quat::from_rotation_y(yaw)),
            true,
        );
        b.set_linvel(Vec3::ZERO, true);
        b.set_angvel(Vec3::ZERO, true);
        t.turret_rel = 0.0;
        t.turret_rate = 0.0;
        t.turret_centering = false;
        t.gun_pitch = 0.0;
        t.belt = [0.0; 2];
        t.parked = true;
        t.flipped_time = 0.0;
        sync_from_body(t, &self.physics);
        t.snapshot();
    }

    /// Shove tank `i` with an impulse (N·s) at a world point (shell hits, blasts).
    pub fn push(&mut self, i: usize, impulse: Vec3, at: Vec3) {
        let b = &mut self.physics.bodies[self.tanks[i].body];
        b.apply_impulse_at_point(impulse, at, true);
    }

    /// Spin tank `i` (angular impulse, N·m·s): wrecks tumbling after a kill.
    pub fn spin(&mut self, i: usize, torque: Vec3) {
        let b = &mut self.physics.bodies[self.tanks[i].body];
        b.apply_torque_impulse(torque, true);
    }

    /// Which tank owns a collider, if any.
    pub fn tank_of(&self, c: ColliderHandle) -> Option<usize> {
        self.tanks.iter().position(|t| t.collider == c)
    }

    pub fn step(&mut self, cmds: &[Command]) {
        self.time += DT;
        for t in &mut self.tanks {
            t.snapshot();
        }
        for i in 0..self.tanks.len() {
            let cmd = cmds.get(i).copied().unwrap_or_default();
            let t = &mut self.tanks[i];
            t.drive = cmd.throttle.abs().max(cmd.steer.abs()).min(1.0);
            drive(t, &cmd, &mut self.physics);
            turret(t, &cmd, DT);
            t.since_fire += DT;
            t.age += DT;
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

fn insert_tank_body(
    physics: &mut PhysicsWorld,
    c: &Chassis,
    pose: Pose,
) -> (RigidBodyHandle, ColliderHandle) {
    let body = RigidBodyBuilder::dynamic()
        .pose(pose)
        .angular_damping(0.3)
        .ccd_enabled(true)
        .build();
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
                    .translation(Vec3::new(
                        t.origin.x + size.x * 0.5,
                        0.0,
                        t.origin.y + size.y * 0.5,
                    ))
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
            ColliderBuilder::cuboid(half.x, half.y, half.z)
                .translation(s.min + half)
                .friction(0.4),
            None,
        );
    }
    for o in map.oboxes.iter().filter(|o| o.solid) {
        let hy = (o.y1 - o.y0) * 0.5;
        // Ramps (collapsed slabs) grip like ground; walls are slick-ish.
        let friction = if o.is_ramp() { ground_friction } else { 0.4 };
        physics.insert_collider(
            ColliderBuilder::cuboid(o.half.x, hy, o.half.y)
                .translation(o.center3())
                .rotation(o.rotation().to_scaled_axis())
                .friction(friction),
            None,
        );
    }
    // Low visual slabs (sidewalks, plazas, lawns) are real curbs to roll over.
    for b in &map.boxes {
        let h = b.max.y - b.min.y;
        if b.min.y.abs() < 1e-3 && h > 0.0 && h <= 0.3 {
            let half = (b.max - b.min) * 0.5;
            physics.insert_collider(
                ColliderBuilder::cuboid(half.x, half.y, half.z)
                    .translation(b.min + half)
                    .friction(ground_friction),
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
    if t.chassis.hover {
        hover(t, cmd, physics);
        return;
    }
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
            if let Some((_, h)) =
                qp.cast_ray_and_get_normal(&Ray::new(origin, -up), c.free_len(), true)
            {
                *hit = Some((origin, h.normal, h.time_of_impact));
            }
        }
    }

    // 2. Hull contacts on the track faces, with their loads from the last solve.
    let mut contacts: Vec<TrackContact> = Vec::new();
    let track_inner = c.track_x - c.track_width * 0.5 - 0.05;
    for pair in physics.contact_pairs_with(t.collider) {
        let PairContacts::Rigid(rc) = &pair.contacts else {
            continue;
        };
        let ours_first = pair.collider1 == t.collider;
        for m in rc.solver_manifolds() {
            // Manifold normal points from collider1 to collider2; we want it pointing into the tank.
            let normal = if ours_first {
                -m.data.normal
            } else {
                m.data.normal
            };
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
    let face_budget = if ground_load > 0.0 {
        t.hull.grip_long * ground_load
    } else {
        mass * GRAVITY
    };
    let face_load: f32 = contacts.iter().take(t.face_contacts).map(|c| c.load).sum();
    if face_load > face_budget {
        let scale = face_budget / face_load;
        for ct in contacts.iter_mut().take(t.face_contacts) {
            ct.load *= scale;
        }
    }

    // 3. Drivetrain. Each side's belt speed is a real state (with the drivetrain's
    // inertia), pushed by the engine, the steering drive and the brakes. The
    // ground never sets it directly: it pushes back through slip, below.
    let h = t.hull;
    let throttle = cmd.throttle.clamp(-1.0, 1.0);
    let steer = cmd.steer.clamp(-1.0, 1.0);
    let m_belt = mass * BELT_MASS_FRAC;
    let dv = |force: f32| force / m_belt * DT;
    let u_avg = (t.belt[0] + t.belt[1]) * 0.5;
    let v_now = b.linvel().dot(fwd);
    // Parking brake: engages once stopped with no input, and stays on (even if
    // something shoves the tank) until there's throttle or steering again.
    let idle = throttle.abs() < 0.05 && steer.abs() < 0.05;
    if !idle {
        t.parked = false;
    } else if u_avg.abs() < PARK_SPEED && v_now.abs() < PARK_SPEED {
        t.parked = true;
    }
    if throttle.abs() > 0.05 && u_avg * throttle >= -0.5 {
        // Engine: full force from a standstill, power-limited above the knee,
        // and governed off as the belts reach top speed.
        let limit = if throttle > 0.0 {
            h.max_speed
        } else {
            h.reverse_speed
        };
        let governor = (limit - u_avg * throttle.signum()).clamp(0.0, 1.0);
        let force =
            throttle * mass * h.accel * (h.power_knee / u_avg.abs().max(h.power_knee)) * governor;
        t.belt[0] += dv(force * 0.5);
        t.belt[1] += dv(force * 0.5);
    } else if throttle.abs() > 0.05 {
        // Throttle against the motion: brakes.
        let step = dv(mass * h.brake * 0.5);
        t.belt = t.belt.map(|u| approach(u, 0.0, step));
    } else if t.parked {
        // The parking brake holds the belts until the driver touches the controls.
        t.belt = [0.0; 2];
        t.parked = true;
    } else {
        // Coasting: only rolling resistance and engine drag slow the belts.
        let step = dv(mass * h.rolling * 0.5);
        t.belt = t.belt.map(|u| approach(u, 0.0, step));
    }
    // Steering drive: pushes the belts' speed difference toward the turn asked
    // for, with limited force. Left track (+x) slower turns left (counter-clockwise).
    let want_diff = steer * h.turn_rate * 2.0 * c.track_x;
    let limit = dv(mass * h.steer_accel);
    let corr = (want_diff - (t.belt[1] - t.belt[0])).clamp(-limit, limit);
    t.belt[0] -= corr * 0.5;
    t.belt[1] += corr * 0.5;

    if !contacts.is_empty() {
        // 4. Ground contacts. Force comes from slip between belt and ground:
        // along the track the belt can roll, so with no drivetrain torque there's
        // no slip and no force (a coasting tank rolls on); sideways the belt can't
        // move at all, so sideways motion is pure slip and meets full friction.
        // Both directions share one friction limit per contact. Whatever the
        // ground pushes on the hull, it pushes back on the belt.
        let share = mass / contacts.len() as f32;
        let mut per_side = [0usize; 2];
        for ct in &contacts {
            per_side[ct.side] += 1;
        }
        let mut belt_reaction = [0.0f32; 2];
        let impulses: Vec<(Vec3, Vec3)> = contacts
            .iter()
            .filter_map(|ct| {
                // Belt direction at the contact, kept in the contact surface.
                let tangent = (ct.push - ct.normal * ct.push.dot(ct.normal)).normalize_or_zero();
                if tangent == Vec3::ZERO {
                    return None;
                }
                let lateral = ct.normal.cross(tangent).normalize_or_zero();
                // Velocities are sampled before any impulse is applied, so every
                // contact sees the same state.
                let v = b.velocity_at_point(ct.point);
                let slip = t.belt[ct.side] - v.dot(tangent);
                // Zeroing the slip moves both this patch of hull and this side's belt.
                let belt_share = m_belt / per_side[ct.side] as f32;
                let m_eff = 1.0 / (1.0 / share + 1.0 / belt_share);
                let mut j_long = m_eff * slip;
                let mut j_lat = -share * v.dot(lateral);
                let max_long = ct.grip * ct.load * DT;
                let max_lat = h.grip_lat * ct.load * DT;
                let usage = ((j_long / max_long.max(1e-6)).powi(2)
                    + (j_lat / max_lat.max(1e-6)).powi(2))
                .sqrt();
                if usage > 1.0 {
                    // Sliding: friction opposes the direction the contact is
                    // actually slipping (belt slip along, hull motion across),
                    // at the friction limit. When pivoting, the middle of each
                    // track slips lengthwise and drives the turn, while the ends
                    // scrub sideways and resist it, as real skid steering does.
                    let lat_slip = -v.dot(lateral);
                    let len = (slip * slip + lat_slip * lat_slip).sqrt().max(1e-6);
                    j_long = max_long * slip / len;
                    j_lat = max_lat * lat_slip / len;
                }
                belt_reaction[ct.side] -= j_long;
                Some((tangent * j_long + lateral * j_lat, ct.point))
            })
            .collect();
        for s in 0..2 {
            t.belt[s] += belt_reaction[s] / m_belt;
        }
        let com = b.center_of_mass();
        // Anti-squat: track and suspension geometry carries part of the drive force
        // up toward the center of mass, so hard acceleration squats the hull instead
        // of standing it on its tail. 0 = force at the ground, 1 = at the CoM.
        for (j, p) in impulses {
            let lift = (com - p).dot(up).max(0.0) * ANTI_SQUAT;
            b.apply_impulse_at_point(j, p + up * lift, true);
        }
    }
    t.track_dist[0] += t.belt[0] * DT;
    t.track_dist[1] += t.belt[1] * DT;
}

/// Hover drive: each emitter is a spring of plasma against whatever is below
/// it. Thrust, braking, sideways grip and turning all push through the
/// cushion, so they fade as the hull lifts off and stop in the air.
fn hover(t: &mut Tank, cmd: &Command, physics: &mut PhysicsWorld) {
    let c = t.chassis;
    let (pose, mass) = {
        let b = &physics.bodies[t.body];
        (*b.position(), b.mass())
    };
    let up = pose.rotation * Vec3::Y;
    let fwd = pose.rotation * Vec3::Z;
    let n = c.wheel_count();
    let reach = c.free_len() + HOVER_REACH;

    let mut hits: Vec<Option<(Vec3, Vec3, f32)>> = vec![None; n];
    {
        let filter = QueryFilter::default().exclude_rigid_body(t.body);
        let qp = physics.query_pipeline_with_filter(filter);
        for (i, hit) in hits.iter_mut().enumerate() {
            let origin = pose.transform_point(c.mount(i));
            if let Some((_, h)) = qp.cast_ray_and_get_normal(&Ray::new(origin, -up), reach, true) {
                *hit = Some((origin, h.normal, h.time_of_impact));
            }
        }
    }

    if t.dead {
        t.wheels_down = 0;
        t.wheel_lift.fill(-HOVER_SAG);
        return;
    }
    let k = mass * GRAVITY / (n as f32 * HOVER_SAG);
    let damp = 2.0 * (k * mass / n as f32).sqrt() * HOVER_DAMPING_RATIO;
    let b = &mut physics.bodies[t.body];
    // How much of the cushion is working (1 = every emitter in range), and the
    // average ground normal under it.
    let mut grip = 0.0;
    let mut ground_n = Vec3::ZERO;
    t.wheels_down = 0;
    for (i, hit) in hits.iter().enumerate() {
        let Some((origin, nrm, toi)) = *hit else {
            t.wheel_lift[i] = -HOVER_SAG - HOVER_REACH;
            continue;
        };
        t.wheel_lift[i] = c.rest_len() - toi;
        let compression = c.free_len() - toi;
        let w = (1.0 - (toi - c.free_len()).max(0.0) / HOVER_REACH).clamp(0.0, 1.0);
        grip += w / n as f32;
        ground_n += nrm * w;
        if compression > 0.0 {
            t.wheels_down += 1;
            // Past the belly gap the hull is about to scrape: stiffen hard.
            let over = (compression - HOVER_SAG - HOVER_BELLY_GAP * 0.8).max(0.0);
            let v_along = b.velocity_at_point(origin).dot(up);
            let force = (k * compression + k * 6.0 * over - damp * v_along).max(0.0);
            b.apply_impulse_at_point(up * force * DT, origin, true);
        }
    }
    t.face_contacts = 0;

    let h = t.hull;
    let throttle = cmd.throttle.clamp(-1.0, 1.0);
    let steer = cmd.steer.clamp(-1.0, 1.0);
    let v = b.linvel();
    let idle = throttle.abs() < 0.05 && steer.abs() < 0.05;
    if !idle {
        t.parked = false;
    } else if v.length() < PARK_SPEED {
        t.parked = true;
    }

    // Drive in the plane of the ground under the cushion.
    let gn = ground_n.normalize_or(up);
    let f = (fwd - gn * fwd.dot(gn)).normalize_or(fwd);
    let side = gn.cross(f);
    let (v_f, v_s) = (v.dot(f), v.dot(side));
    let a_long = if throttle.abs() > 0.05 {
        let target = throttle
            * if throttle > 0.0 {
                h.max_speed
            } else {
                h.reverse_speed
            };
        let thrust = h.accel * (h.power_knee / v_f.abs().max(h.power_knee));
        let limit = if v_f * throttle < -0.5 {
            h.brake
        } else {
            thrust
        };
        ((target - v_f) / DT).clamp(-limit, limit)
    } else if t.parked {
        // Station-keeping: hold position, even on a slope.
        (-v_f / DT).clamp(-h.brake, h.brake)
    } else {
        // Coasting: it glides a long way.
        (-v_f / DT).clamp(-h.rolling, h.rolling)
    };
    let lat_limit = if t.parked { h.brake } else { h.grip_lat };
    let a_side = (-v_s / DT).clamp(-lat_limit, lat_limit);
    // On a slope the cushion also has to hold the hull against gravity's pull
    // down it, or it would slide off every hill.
    let downhill = Vec3::NEG_Y - gn * Vec3::NEG_Y.dot(gn);
    let hold = -downhill * GRAVITY;
    b.apply_impulse((f * a_long + side * a_side + hold) * mass * DT * grip, true);

    // Yaw: spin up toward the turn asked for. Steering left is +y.
    let w = b.angvel().dot(up);
    let want = steer * h.turn_rate;
    let dw = (want - w).clamp(-h.steer_accel * DT, h.steer_accel * DT) * grip;
    let inertia = c.inertia().y;
    b.apply_torque_impulse(up * inertia * dw, true);

    // Bank into the turn, more the faster it goes.
    let bank = (w * v_f * 0.014).clamp(-0.14, 0.14);
    t.lean += (bank - t.lean) * (1.0 - (-DT * 5.0).exp());
    t.belt = [v_f; 2];
    t.track_dist[0] += v_f * DT;
    t.track_dist[1] += v_f * DT;
}

fn turret(t: &mut Tank, cmd: &Command, dt: f32) {
    let h = t.hull;
    // Manual rotation relative to the hull: a slow spin-up, a quicker stop.
    // Centering (latched by a C tap) drives it back to 0 and stops exactly there;
    // any manual input takes over at once.
    let manual = cmd.turret.clamp(-1.0, 1.0);
    if manual.abs() > 0.05 {
        t.turret_centering = false;
    } else if cmd.center_turret {
        t.turret_centering = true;
    }
    let target_rate = if manual.abs() > 0.05 {
        manual * h.turret_speed
    } else if t.turret_centering {
        // Full speed toward center until it's time to brake.
        let stop_dist = t.turret_rate * t.turret_rate / (2.0 * h.turret_brake);
        let err = -t.turret_rel;
        if err.abs() <= stop_dist {
            0.0
        } else {
            err.signum() * h.turret_speed
        }
    } else {
        0.0
    };
    // Speeding up in the current direction uses the motor; anything else is braking.
    let spinning_up = target_rate * t.turret_rate >= 0.0 && target_rate.abs() > t.turret_rate.abs();
    let rate = if spinning_up {
        h.turret_accel
    } else {
        h.turret_brake
    };
    t.turret_rate = approach(t.turret_rate, target_rate, rate * dt);
    let before = t.turret_rel;
    t.turret_rel = wrap_angle(t.turret_rel + t.turret_rate * dt);
    if t.turret_centering {
        let crossed = before.signum() != t.turret_rel.signum() && before.abs() < 0.5;
        let settled = t.turret_rel.abs() < 0.02 && t.turret_rate.abs() < 0.5;
        if crossed || settled {
            // Snap and stop exactly forward instead of creeping or oscillating.
            t.turret_rel = 0.0;
            t.turret_rate = 0.0;
            t.turret_centering = false;
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
    if v < target {
        (v + delta).min(target)
    } else {
        (v - delta).max(target)
    }
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
        map.solids
            .retain(|s| s.max.x < 1.0 || s.min.x > 199.0 || s.max.z < 1.0 || s.min.z > 199.0);
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
        w.physics.insert_collider(
            ColliderBuilder::cuboid(half.x, half.y, half.z).translation(min + half),
            None,
        );
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
        Command {
            throttle: x,
            ..Default::default()
        }
    }

    #[test]
    fn map_is_deterministic() {
        let (a, b) = (Map::generate(42), Map::generate(42));
        assert_eq!(a.solids.len(), b.solids.len());
        assert_eq!(a.boxes.len(), b.boxes.len());
        assert!(
            a.solids.len() > 20,
            "expected a dense map, got {} solids",
            a.solids.len()
        );
    }

    #[test]
    fn open_spots_are_drivable() {
        for seed in 0..20 {
            let m = Map::generate(seed);
            assert!(m.open_spots.len() > 100);
            for p in &m.open_spots {
                for s in &m.solids {
                    let cp = p.clamp(s.min2(), s.max2());
                    assert!(
                        cp.distance(*p) >= 2.3,
                        "seed {seed}: open spot {p} overlaps a solid"
                    );
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
        assert!(
            early > 0.33 * HullStats::LANCER.max_speed,
            "sluggish start: {early} m/s after 0.5 s"
        );
        assert!(w.tanks[0].up().y > 0.98, "wheelie: {:?}", w.tanks[0].up());
        run(&mut w, throttle(1.0), 2.5);
        let s = w.tanks[0].speed();
        assert!(
            (s - HullStats::LANCER.max_speed).abs() < 0.5,
            "top speed {s}"
        );
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
            w.step(&[Command {
                steer: 1.0,
                ..Default::default()
            }]);
            let y = w.tanks[0].yaw();
            turned += wrap_angle(y - last);
            last = y;
        }
        assert!(
            turned > 0.8 * PI,
            "only turned {:.0} degrees in 1.6 s",
            turned.to_degrees()
        );
        let drift = Vec2::new(w.tanks[0].pos.x - p0.x, w.tanks[0].pos.z - p0.z).length();
        assert!(drift < 0.6, "pivot wandered {drift} m");
    }

    #[test]
    fn holds_position_on_slope() {
        let mut map = Map::proving_ground();
        // A uniform 20 degree slope rising along +x; tank parked across it.
        let grade = 20f32.to_radians().tan();
        map.terrain = Some(Terrain::from_fn(Vec2::ZERO, 2.0, 101, 101, move |x, _| {
            x * grade
        }));
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
            material: None,
            min: Vec2::new(95.0, 120.0),
            max: Vec2::new(105.0, 124.0),
            base_y: 0.0,
            height: 1.6,
            rise: Rise::PosZ,
            color: Vec3::ONE,
        };
        w.physics
            .insert_collider(wedge_collider(&kicker).unwrap(), None);
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
        assert!(
            t.pos.y.abs() < 0.3,
            "not resting on the ground: {}",
            t.pos.y
        );
    }

    #[test]
    fn every_kicker_lane_is_clearable_at_full_speed() {
        for lane in 0..3 {
            let mut w = World::new(Map::proving_ground(), lane);
            run(&mut w, throttle(1.0), 7.0);
            let t = &w.tanks[0];
            assert!(
                t.pos.z > 80.0,
                "lane {lane}: stopped at z={} (crashed into the landing ramp?)",
                t.pos.z
            );
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
            assert!(
                (hit - expect).abs() < 0.3,
                "at ({x},{z}) collider {hit} vs terrain {expect}"
            );
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
            assert!(
                t.pos.z > spot.y + 30.0,
                "stuck at z={} starting from {spot}",
                t.pos.z
            );
            assert!(t.up().y > 0.8, "tipped over at {spot}");
        }
    }

    /// Drive at a ledge from `start` (rotated by `yaw`); true if the tank ends up on top.
    fn climbs(height: f32, start: Vec2, yaw: f32, hull: HullKind) -> bool {
        let mut w = ledge_world(height);
        w.set_hull(0, hull);
        place(
            &mut w,
            Vec3::new(start.x, 0.2, start.y),
            Quat::from_rotation_y(yaw),
        );
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
                assert!(
                    climbs(h, Vec2::new(100.0, 106.5), 0.0, hull),
                    "{hull:?} can't climb {h} m from a standstill"
                );
                assert!(
                    climbs(h, Vec2::new(100.0, 80.0), 0.0, hull),
                    "{hull:?} can't climb {h} m with a run-up"
                );
            }
            // Hitting a kerb or step at an angle.
            assert!(
                climbs(0.4, Vec2::new(90.0, 100.0), 0.6, hull),
                "{hull:?} stuck on a diagonal step"
            );
        }
    }

    /// Something clearly wall-like (taller than the hull) still blocks.
    #[test]
    fn walls_still_block() {
        assert!(
            !climbs(2.0, Vec2::new(100.0, 80.0), 0.0, HullKind::Lancer),
            "drove up a wall"
        );
    }

    #[test]
    fn crosses_stairs_side_on() {
        // The stairs block from the proving ground, approached from its side.
        let mut w = World::new(Map::proving_ground(), 0);
        // At z=121.5 the side face is the first two steps: 0.25 m, then 0.5 m.
        place(
            &mut w,
            Vec3::new(20.0, 0.2, 121.5),
            Quat::from_rotation_y(PI / 2.0),
        );
        run(&mut w, Command::default(), 0.5);
        run(&mut w, throttle(1.0), 5.0);
        let t = &w.tanks[0];
        assert!(
            t.pos.x > 34.0,
            "stuck at x={} climbing the stairs side-on",
            t.pos.x
        );
        assert!(t.up().y > 0.8, "tipped over");
    }

    /// The turret winds up over about half a second but stops within a few degrees of release.
    #[test]
    fn turret_spins_up_slowly_and_stops_quickly() {
        let mut w = flat_world();
        let h = HullStats::LANCER;
        run(
            &mut w,
            Command {
                turret: 1.0,
                ..Default::default()
            },
            0.2,
        );
        let rate = w.tanks[0].turret_rate;
        assert!(
            rate > 0.1 && rate < 0.6 * h.turret_speed,
            "after 0.2 s already at {rate} rad/s"
        );
        run(
            &mut w,
            Command {
                turret: 1.0,
                ..Default::default()
            },
            0.6,
        );
        assert!(
            (w.tanks[0].turret_rate - h.turret_speed).abs() < 1e-3,
            "not at full speed after 0.8 s"
        );
        let released = w.tanks[0].turret_rel;
        run(&mut w, Command::default(), 0.5);
        let coast = (w.tanks[0].turret_rel - released).abs();
        assert!(
            w.tanks[0].turret_rate == 0.0 && coast < 10f32.to_radians(),
            "coasted {:.1} deg after release",
            coast.to_degrees()
        );
    }

    #[test]
    fn turret_turns_with_hull_and_centers() {
        let mut w = flat_world();
        run(
            &mut w,
            Command {
                turret: 1.0,
                ..Default::default()
            },
            1.0,
        );
        assert!(w.tanks[0].turret_rel > 0.5);
        run(&mut w, Command::default(), 0.25);
        let rel = w.tanks[0].turret_rel;
        run(
            &mut w,
            Command {
                steer: 1.0,
                ..Default::default()
            },
            1.0,
        );
        assert!((w.tanks[0].turret_rel - rel).abs() < 1e-4);
        // A single tap of C is enough: the swing finishes on its own.
        run(
            &mut w,
            Command {
                center_turret: true,
                ..Default::default()
            },
            DT,
        );
        run(&mut w, Command::default(), 3.0);
        assert!(w.tanks[0].turret_rel.abs() < 0.01);
        assert!(w.tanks[0].turret_rate.abs() < 0.01);
    }

    /// Pressing Z/X mid-swing cancels the recenter; the turret then just stops.
    #[test]
    fn turret_input_cancels_centering() {
        let mut w = flat_world();
        let turn = |t: f32| Command {
            turret: t,
            ..Default::default()
        };
        run(&mut w, turn(1.0), 1.5);
        run(&mut w, Command::default(), 0.5);
        let start = w.tanks[0].turret_rel;
        run(
            &mut w,
            Command {
                center_turret: true,
                ..Default::default()
            },
            DT,
        );
        run(&mut w, Command::default(), 0.3);
        assert!(w.tanks[0].turret_centering);
        assert!(w.tanks[0].turret_rel < start, "not swinging back");
        run(&mut w, turn(1.0), DT);
        assert!(!w.tanks[0].turret_centering);
        run(&mut w, Command::default(), 1.0);
        let stopped = w.tanks[0].turret_rel;
        assert!(
            stopped.abs() > 0.3,
            "kept centering after cancel ({stopped})"
        );
        run(&mut w, Command::default(), 0.5);
        assert_eq!(w.tanks[0].turret_rel, stopped);
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
        assert!(
            t.pos.distance(t.prev_pos) > 0.01,
            "moving tank should differ between steps"
        );
    }

    /// A always rotates the hull counter-clockwise, D clockwise, whether driving
    /// forward, reversing, or standing still.
    #[test]
    fn steering_direction_ignores_throttle() {
        for throttle in [1.0, 0.0, -1.0] {
            let mut w = flat_world();
            run(&mut w, Command::default(), 0.5);
            run(
                &mut w,
                Command {
                    throttle,
                    ..Default::default()
                },
                1.0,
            );
            let yaw0 = w.tanks[0].yaw();
            run(
                &mut w,
                Command {
                    throttle,
                    steer: 1.0,
                    ..Default::default()
                },
                0.5,
            );
            let turned = wrap_angle(w.tanks[0].yaw() - yaw0);
            assert!(
                turned > 0.2,
                "steer left with throttle {throttle} turned {turned} rad"
            );
        }
    }

    /// Tracks roll freely along their length and grip hard sideways: a coasting
    /// tank rolls on, a sliding one stops almost at once.
    #[test]
    fn rolls_forward_freely_but_resists_sliding_sideways() {
        // Slide: shove a parked tank sideways.
        let distance = |vel: Vec3, belt: f32| {
            let mut w = flat_world();
            run(&mut w, Command::default(), 0.5);
            w.tanks[0].belt = [belt; 2];
            w.physics.bodies[w.tanks[0].body].set_linvel(vel, true);
            let p0 = w.tanks[0].pos;
            run(&mut w, Command::default(), 8.0);
            w.tanks[0].pos.distance(p0)
        };
        // Coast: drive up to about 6 m/s, then let go.
        let coast = {
            let mut w = flat_world();
            run(&mut w, Command::default(), 0.5);
            while w.tanks[0].speed() < 6.0 {
                w.step(&[throttle(1.0)]);
            }
            let p0 = w.tanks[0].pos;
            run(&mut w, Command::default(), 8.0);
            w.tanks[0].pos.distance(p0)
        };
        let slide = distance(Vec3::new(6.0, 0.0, 0.0), 0.0);
        assert!(coast > 12.0, "coasting from 6 m/s only rolled {coast} m");
        assert!(slide < 3.0, "sliding sideways from 6 m/s went {slide} m");
    }

    /// Nudged while parked (no input), the brakes hold.
    #[test]
    fn parking_brake_holds() {
        let mut w = flat_world();
        run(&mut w, Command::default(), 1.0);
        let p0 = w.tanks[0].pos;
        w.fire(0);
        run(&mut w, Command::default(), 2.0);
        assert!(
            w.tanks[0].pos.distance(p0) < 0.3,
            "rolled {} m after firing while parked",
            w.tanks[0].pos.distance(p0)
        );
    }

    #[test]
    fn rights_itself_when_flipped() {
        let mut w = flat_world();
        place(
            &mut w,
            Vec3::new(100.0, 2.5, 100.0),
            Quat::from_rotation_z(PI),
        );
        run(&mut w, Command::default(), 1.0);
        assert!(w.tanks[0].up().y < 0.0, "should be upside down first");
        run(&mut w, Command::default(), 4.0);
        assert!(
            w.tanks[0].up().y > 0.95,
            "did not recover: {:?}",
            w.tanks[0].up()
        );
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

#[cfg(test)]
mod hover_tests {
    use super::tests_support::*;
    use super::*;

    fn hover_world() -> World {
        let mut w = flat_world();
        w.set_hull(0, HullKind::Sethuun);
        run(&mut w, Command::default(), 2.0);
        w
    }

    fn cmd(throttle: f32, steer: f32) -> Command {
        Command {
            throttle,
            steer,
            ..Default::default()
        }
    }

    #[test]
    fn floats_at_ride_height_and_holds_still() {
        let mut w = hover_world();
        let start = w.tanks[0].pos;
        run(&mut w, Command::default(), 3.0);
        let t = &w.tanks[0];
        // Belly clear of the ground, hull level, not drifting.
        assert!((t.pos.y - 0.0).abs() < 0.2, "rides at {}", t.pos.y);
        assert!(t.up().y > 0.99);
        assert!(
            t.pos.distance(start) < 0.3,
            "drifted {}",
            t.pos.distance(start)
        );
        assert_eq!(t.wheels_down, 6);
    }

    #[test]
    fn faster_than_tracks_in_a_straight_line() {
        let mut w = hover_world();
        run(&mut w, cmd(1.0, 0.0), 5.0);
        let hover = w.tanks[0].speed();
        let mut l = flat_world();
        run(&mut l, cmd(1.0, 0.0), 5.0);
        let tracks = l.tanks[0].speed();
        assert!(hover > tracks + 1.5, "hover {hover} vs tracks {tracks}");
    }

    #[test]
    fn slides_through_a_fast_turn() {
        let mut w = hover_world();
        run(&mut w, cmd(1.0, 0.0), 4.0);
        run(&mut w, cmd(1.0, 1.0), 0.6);
        let t = &w.tanks[0];
        let v = t.vel.normalize();
        // Velocity lags the nose: it's drifting, not carving.
        let slip = v.dot(t.forward()).acos();
        assert!(slip > 0.15, "slip {slip}");
        assert!(t.lean.abs() > 0.03, "lean {}", t.lean);
    }

    #[test]
    fn rides_up_over_a_low_ledge() {
        let mut w = ledge_world(0.6);
        w.set_hull(0, HullKind::Sethuun);
        run(&mut w, Command::default(), 2.0);
        // Onto the block (near face at z=110, 30 m deep) and riding on top.
        let mut on_top = false;
        for _ in 0..40 {
            run(&mut w, cmd(1.0, 0.0), 0.1);
            let t = &w.tanks[0];
            on_top |= t.pos.z > 115.0 && t.pos.z < 138.0 && t.pos.y > 0.4;
        }
        assert!(on_top, "never got on top; at {}", w.tanks[0].pos);
    }

    #[test]
    fn a_dead_hull_drops() {
        let mut w = hover_world();
        w.tanks[0].dead = true;
        run(&mut w, Command::default(), 2.0);
        assert!(
            w.tanks[0].pos.y < -0.4,
            "still floating at {}",
            w.tanks[0].pos.y
        );
    }
}
