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

use glam::{Mat4, Quat, Vec2, Vec3};
use rapier3d::prelude::*;

use crate::game::DT;
use crate::map::{Map, Terrain, Wedge};

pub const GRAVITY: f32 = 9.81;

/// Which kit parts a tank is built from (see `tank::model`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HullKind {
    Kataphrakt,
    Kastron,
    /// Ithessar: rides a plasma cushion instead of tracks.
    Sethuun,
    /// Resonant: a light vessel run by the crystal at its heart. Thin armor,
    /// but it keeps going for a few seconds after it's broken.
    Vigil,
    /// Uthenn: big, bulky and slow, with the thermal mass to hold a beam on.
    Gravis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurretKind {
    /// Imperium rail cannon.
    Kontos,
    /// Imperium twin autocannon.
    Pelekys,
    /// Ithessar plasma orb that bounces off walls.
    Ammeth,
    /// Ithessar three-barrel rapid plasma.
    Ossrin,
    /// Machine laser beam; overheats.
    Lumen,
    /// Resonant short-range crystal flamethrower.
    Hymn,
    /// Resonant siege bell: charges while held, then tolls one shot that
    /// throws both tanks.
    Knell,
}

impl HullKind {
    pub fn maker(self) -> crate::faction::Faction {
        use crate::faction::Faction;
        match self {
            HullKind::Kataphrakt | HullKind::Kastron => Faction::Imperium,
            HullKind::Sethuun => Faction::Ithessar,
            HullKind::Vigil => Faction::Resonant,
            HullKind::Gravis => Faction::Uthenn,
        }
    }

    /// Where the turret ring sits in the hull's body frame (the art's too).
    /// A hover hull's ring rides on its body, which floats [`HOVER_RIDE`]
    /// up and banks with [`Tank::lean`]; see [`Tank::turret_frame`].
    pub fn ring(self) -> Vec3 {
        match self {
            HullKind::Kataphrakt => Vec3::new(0.0, 1.42, -0.3),
            HullKind::Kastron => Vec3::new(0.0, 1.62, -0.2),
            HullKind::Sethuun => Vec3::new(0.0, 1.3, -0.15),
            HullKind::Vigil => Vec3::new(0.0, 1.38, -0.3),
            HullKind::Gravis => Vec3::new(0.0, 2.8, -0.4),
        }
    }
}

impl TurretKind {
    /// Every turret, grouped by maker. Also the kit's art order.
    pub const ALL: [TurretKind; 7] = [
        TurretKind::Kontos,
        TurretKind::Pelekys,
        TurretKind::Ammeth,
        TurretKind::Ossrin,
        TurretKind::Lumen,
        TurretKind::Hymn,
        TurretKind::Knell,
    ];

    pub fn maker(self) -> crate::faction::Faction {
        use crate::faction::Faction;
        match self {
            TurretKind::Kontos | TurretKind::Pelekys => Faction::Imperium,
            TurretKind::Ammeth | TurretKind::Ossrin => Faction::Ithessar,
            TurretKind::Lumen => Faction::Uthenn,
            TurretKind::Hymn | TurretKind::Knell => Faction::Resonant,
        }
    }

    /// Momentum one shot carries, N·s. Equal and opposite: the gun kicks its
    /// own hull back by exactly this much (at gun height, so it rocks back on
    /// its springs as well as sliding), and the shot shoves whatever tank it
    /// lands on forward by the same. Per shot, not per tank: a heavy hull
    /// shrugs off what sends a light one skidding.
    pub fn recoil(self) -> f32 {
        match self {
            // Long rail cannon: one big slam.
            TurretKind::Kontos => 60_000.0,
            // Twin short guns: smaller kicks, more often.
            TurretKind::Pelekys => 25_000.0,
            // A big, slow orb of plasma: a heavy shove.
            TurretKind::Ammeth => 45_000.0,
            // A stream of light darts: a steady push.
            TurretKind::Ossrin => 3_500.0,
            // Each puff of fire is light, but there are sixty a second.
            TurretKind::Hymn => 600.0,
            // The siege bell: it shoves its own hull back (a Kataphrakt at
            // ~4 m/s), and hands its target far more (`KNELL_KNOCK`).
            TurretKind::Knell => 75_000.0,
            // Light has no mass to speak of: a beam doesn't push.
            TurretKind::Lumen => 0.0,
        }
    }

    /// The gun's pitch pivot in turret space (the art's too).
    pub fn trunnion(self) -> Vec3 {
        match self {
            TurretKind::Kontos => Vec3::new(0.0, 0.45, 0.88),
            TurretKind::Pelekys => Vec3::new(0.0, 0.38, 0.92),
            TurretKind::Ammeth => Vec3::new(0.0, 0.5, 0.55),
            TurretKind::Ossrin => Vec3::new(0.0, 0.44, 0.62),
            // The Lumen is drawn 1.2x size, pivot and all.
            TurretKind::Lumen => Vec3::new(0.0, 0.42, 0.92) * 1.2,
            TurretKind::Hymn => Vec3::new(0.0, 0.45, 0.8),
            TurretKind::Knell => Vec3::new(0.0, 0.95, 0.3),
        }
    }

    /// How far along the gun axis, from the trunnion, its muzzle is (the
    /// art's too): where shots and the beam leave it.
    pub fn reach(self) -> f32 {
        match self {
            TurretKind::Kontos => 3.55,
            TurretKind::Pelekys => 2.82,
            TurretKind::Ammeth => 1.45,
            TurretKind::Ossrin => 2.35,
            TurretKind::Lumen => 3.2 * 1.2,
            TurretKind::Hymn => 1.85,
            TurretKind::Knell => 2.3,
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
    pub const KATAPHRAKT: Chassis = Chassis {
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
    pub const KASTRON: Chassis = Chassis {
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

    /// Resonant hull: Kataphrakt's running gear, lighter (no armor to speak of).
    pub const VIGIL: Chassis = Chassis {
        mass: 15_000.0,
        ..Chassis::KATAPHRAKT
    };

    /// Uthenn heavy hull: the biggest, longest and tallest, on seven road
    /// wheels a side. Its box is the citadel (the tall tier the turret sits
    /// on); the convex hull down to the track ends covers the lower shell.
    pub const GRAVIS: Chassis = Chassis {
        mass: 50_000.0,
        // 1.6 m plus the widening (`crate::tank::uthenn::WIDEN`).
        track_x: 1.85,
        track_width: 0.86,
        road_z: &[-2.7, -1.8, -0.9, 0.0, 0.9, 1.8, 2.7],
        road_y: 0.46,
        road_r: 0.38,
        front: (3.5, 0.82, 0.42),
        rear: (-3.45, 0.85, 0.46),
        hull_half_x: 2.15,
        hull_top: 2.8,
        hull_z: (-3.3, 1.9),
        center_of_mass: Vec3::new(0.0, 0.9, -0.15),
        hover: false,
    };

    pub fn of(kind: HullKind) -> &'static Chassis {
        match kind {
            HullKind::Gravis => &Chassis::GRAVIS,
            HullKind::Kataphrakt => &Chassis::KATAPHRAKT,
            HullKind::Vigil => &Chassis::VIGIL,
            HullKind::Kastron => &Chassis::KASTRON,
            HullKind::Sethuun => &Chassis::SETHUUN,
        }
    }

    pub fn wheel_count(&self) -> usize {
        self.road_z.len() * 2
    }

    /// How far from the body's origin this hull or its wheels' sweeps can
    /// reach (m).
    pub fn reach(kind: HullKind) -> f32 {
        use HullKind::*;
        // Exhaustive, so a new hull kind can't go without its reach.
        let index = |k: HullKind| match k {
            Kataphrakt => 0,
            Kastron => 1,
            Sethuun => 2,
            Vigil => 3,
            Gravis => 4,
        };
        static REACH: std::sync::OnceLock<[f32; 5]> = std::sync::OnceLock::new();
        let all = REACH.get_or_init(|| {
            let mut all = [0.0; 5];
            for k in [Kataphrakt, Kastron, Sethuun, Vigil, Gravis] {
                let c = Chassis::of(k);
                let hull = (c.collider_points().iter()).fold(0.0f32, |m, p| m.max(p.length()));
                let wheels = (0..c.wheel_count())
                    .fold(0.0f32, |m, i| m.max(c.mount(i).length() + c.free_len()));
                all[index(k)] = hull.max(wheels);
            }
            all
        });
        all[index(kind)]
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
pub const SUSPENSION_TRAVEL_UP: f32 = 0.55;
/// The swept wheel ball's radius as a fraction of the road wheel's: a bit
/// smaller, so it doesn't catch on walls beside the track.
const WHEEL_CAST_RADIUS: f32 = 0.85;
/// Static compression under the tank's own weight (sets spring stiffness).
pub const SUSPENSION_SAG: f32 = 0.28;
/// Extra damping on the end road wheels (multiplier added at the ends).
const END_STATION_DAMPING: f32 = 3.0;
const SUSPENSION_DAMPING_RATIO: f32 = 0.6;
/// Fraction of the tracks' sideways grip force's lever arm removed by
/// suspension geometry, so a hard turn doesn't heel the hull onto its
/// outer track.
const ANTI_ROLL: f32 = 0.6;
/// The same along the tracks while the push speeds the hull up (the
/// engine): pulling away squats it only a little, so the gun stays near its
/// mark.
const ANTI_SQUAT: f32 = 0.85;
/// ...and while it slows the hull (the brakes, pressing against the way
/// it's going): less, so a hard stop dives the nose (or from reverse,
/// throws it up), enough to tip the gun onto something.
const ANTI_DIVE: f32 = 0.35;
/// Below this speed, m/s, the brakes' lift fades into the engine's, so
/// braking through a stop into reverse (the push keeps its way while the
/// lift would change) and pivoting (the tracks push opposite ways) never
/// jolt the hull. Moving, the lift changes only as the push flips, when
/// it's nothing.
const DIVE_FADE_SPEED: f32 = 2.0;
/// Drivetrain inertia seen at each belt, as a fraction of the tank's mass.
const BELT_MASS_FRAC: f32 = 0.08;
/// Below this speed with no input, the parking brake holds the tank.
const PARK_SPEED: f32 = 0.3;
/// Friction of a wreck's hull against the ground. A live hull is frictionless
/// (the track model does the gripping); a dead one drags on its belly, side or
/// roof instead of sliding off like a puck.
const WRECK_FRICTION: f32 = 0.7;
/// Hover emitters: static compression under the hull's weight (soft: it bobs).
pub const HOVER_SAG: f32 = 0.16;
/// How high a hover hull's body (and so its turret) floats above its frame.
pub const HOVER_RIDE: f32 = 0.3;
const HOVER_DAMPING_RATIO: f32 = 0.45;
/// How far the belly sits above the emitters' rest line: their working range
/// before the hull itself touches.
const HOVER_BELLY_GAP: f32 = 0.42;
/// Beyond full extension the cushion still steers and thrusts, fading out
/// over this much more height (cresting a dune doesn't cut the drive).
const HOVER_REACH: f32 = 0.9;
/// Ground slope (radians) the cushion holds on fully, and the slope past
/// which it gets no footing at all.
const HOVER_FIRM: f32 = 40.0 * PI / 180.0;
const HOVER_STEEP: f32 = 55.0 * PI / 180.0;
/// Hover attitude hold (see `hover`): stiffness and damping per radian of
/// tilt, the cap on the angular acceleration it gives, and how much of it
/// still works with the whole cushion on the ground.
const HOVER_LEVEL_K: f32 = 40.0;
const HOVER_LEVEL_DAMP: f32 = 10.0;
const HOVER_LEVEL_MAX: f32 = 30.0;
const HOVER_LEVEL_ON_CUSHION: f32 = 0.25;
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
    /// The tank's mass as a multiple of its bare chassis's (`Chassis::mass`):
    /// upgraded hulls carry more plate. The body takes it on at the next step.
    pub heft: f32,
}

impl HullStats {
    pub const KATAPHRAKT: Self = Self {
        max_speed: 11.5,
        reverse_speed: 7.0,
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
        heft: 1.0,
    };
    /// Fast: Kataphrakt's old pace.
    pub const VIGIL: Self = Self {
        max_speed: 13.0,
        reverse_speed: 7.5,
        ..HullStats::KATAPHRAKT
    };
    pub const KASTRON: Self = Self {
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
        heft: 1.0,
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
        heft: 1.0,
    };

    /// Slow and heavy: the slowest hull, slow to turn and to traverse.
    pub const GRAVIS: Self = Self {
        max_speed: 7.8,
        reverse_speed: 4.5,
        accel: 5.5,
        power_knee: 4.5,
        brake: 18.0,
        rolling: 1.4,
        turn_rate: 1.6,
        // Its tracks are long for their spacing, so the ends scrub hard
        // sideways in a pivot: it needs more steering force and less side
        // grip than the others just to turn at all (at 11.0 and 1.9 it
        // managed 7 deg/s against a 92 deg/s ask).
        steer_accel: 18.0,
        grip_long: 1.9,
        grip_lat: 1.4,
        turret_speed: 0.8,
        turret_accel: 1.4,
        turret_brake: 6.0,
        heft: 1.0,
    };

    pub fn of(kind: HullKind) -> HullStats {
        match kind {
            HullKind::Gravis => HullStats::GRAVIS,
            HullKind::Kataphrakt => HullStats::KATAPHRAKT,
            HullKind::Kastron => HullStats::KASTRON,
            HullKind::Sethuun => HullStats::SETHUUN,
            HullKind::Vigil => HullStats::VIGIL,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tank {
    /// Multiplies the paint (white: the maker's own colors; dimmed when the tank is wrecked).
    pub color: Vec3,
    /// Painted over in the Company's colors: the player's tank, a mercenary among
    /// the side that hired it.
    pub company: bool,
    pub hull_kind: HullKind,
    pub turret_kind: TurretKind,
    pub chassis: &'static Chassis,
    pub hull: HullStats,
    pub body: RigidBodyHandle,
    pub collider: ColliderHandle,
    /// The turret's hitbox: hangs off the body, turns with the turret, and
    /// only shots see it (see [`insert_turret_hitbox`]).
    pub turret_hitbox: ColliderHandle,
    /// The turret the hitbox is shaped for (`turret_kind` can be swapped).
    hitbox_kind: TurretKind,

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
    /// The aim assist's small sideways nudge of the gun in its mantlet,
    /// radians (positive = left, toward +yaw), on top of the turret's heading.
    pub gun_yaw: f32,
    /// Distance each tread has travelled: [left (+x), right (-x)].
    pub track_dist: [f32; 2],
    /// Seconds since the gun last fired (drives recoil and muzzle flash).
    pub since_fire: f32,
    /// Which barrel fired last (twin guns alternate).
    pub fire_barrel: usize,
    /// Beam guns, mirrored from the fight for the art: heat 0..1, locked
    /// out hot, and whether the beam is on this step.
    pub heat: f32,
    pub overheated: bool,
    pub beaming: bool,
    pub flipped_time: f32,
    /// Seconds since the tank was spawned (drives idle animation).
    pub age: f32,
    /// Destroyed: a hover hull's field is out and it falls.
    pub dead: bool,
    /// An empty seat: no tank has been fielded in it (its side had no hull
    /// for it). Out of the world: its body is disabled (no collisions, no
    /// queries see it), it isn't driven or drawn. See [`World::set_empty`].
    pub empty: bool,
    /// Broken but still running (Vigil): how much strength it has left,
    /// 1 down to 0. `None` while whole.
    pub husk: Option<f32>,
    /// Hover hulls bank into turns (radians, + rolls right side down). Visual.
    pub lean: f32,
    /// Spin-up guns: how far spun up, 0..1, and the cluster's angle.
    pub spool: f32,
    pub spin: f32,
    /// Charge guns (Knell): how far the shot has built up, 0..1 (it fires at
    /// 1). Draws the art and the hum; the gun itself lives in combat.
    pub charge: f32,
    /// State at the start of the last step, for render interpolation.
    prev_pos: Vec3,
    prev_rot: Quat,
    prev_turret_rel: f32,
    prev_track_dist: [f32; 2],
    /// What the hull has been feeling, for [`Knock`]s.
    feel: Feel,
}

impl Tank {
    /// The instance tint for the tank's paint: `color`, or for a Company tank the
    /// Company's paint dimmed by it, flagged (alpha -1) for the shader to paint over
    /// the maker's colors rather than multiply them.
    pub fn paint_tint(&self) -> glam::Vec4 {
        if self.company {
            (self.color * crate::faction::Faction::Company.paint()).extend(-1.0)
        } else {
            self.color.extend(0.0)
        }
    }

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
        self.turret_frame().transform_vector3(Vec3::Z).normalize()
    }

    /// Where the gun points in the world, including elevation.
    pub fn gun_dir(&self) -> Vec3 {
        self.gun_frame().transform_vector3(Vec3::Z).normalize()
    }

    /// Height of the gun's pivot above the hull origin.
    pub fn gun_height(&self) -> f32 {
        self.chassis.hull_top + 0.4
    }

    /// The turret ring in the world, turned to the turret's heading: the
    /// frame the turret's art hangs from.
    pub fn turret_frame(&self) -> Mat4 {
        Mat4::from_rotation_translation(self.rot, self.pos) * self.turret_mount()
    }

    /// The turret ring in the hull's own frame, turned to the turret's heading.
    fn turret_mount(&self) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        if self.hull_kind == HullKind::Sethuun {
            m = Mat4::from_rotation_z(self.lean) * Mat4::from_translation(Vec3::Y * HOVER_RIDE);
        }
        m * Mat4::from_translation(self.hull_kind.ring()) * Mat4::from_rotation_y(self.turret_rel)
    }

    /// Hang the turret hitbox where the turret is drawn now, reshaping it if
    /// the turret was swapped. Takes effect at the next physics step.
    fn place_turret_hitbox(&mut self, physics: &mut PhysicsWorld) {
        let Some(c) = physics.colliders.get_mut(self.turret_hitbox) else {
            return;
        };
        if self.hitbox_kind != self.turret_kind {
            c.set_shape(turret_shape(self.turret_kind));
            self.hitbox_kind = self.turret_kind;
        }
        let (_, rot, at) = self.turret_mount().to_scale_rotation_translation();
        let pose = Pose::from_parts(at, rot);
        // Re-placing it makes rapier re-insert it in the broad phase and
        // recompute the body's mass: only when the turret has moved.
        if c.position_wrt_parent() != Some(&pose) {
            c.set_position_wrt_parent(pose);
        }
    }

    /// The gun in the world: origin at its trunnion, +z down the barrel.
    pub fn gun_frame(&self) -> Mat4 {
        self.turret_frame()
            * Mat4::from_translation(self.turret_kind.trunnion())
            * Mat4::from_rotation_y(self.gun_yaw)
            * Mat4::from_rotation_x(-self.gun_pitch)
    }

    /// Where the gun's axis leaves the muzzle, in the world.
    pub fn muzzle(&self) -> Vec3 {
        self.gun_frame()
            .transform_point3(Vec3::Z * self.turret_kind.reach())
    }

    /// The gun's trunnion in the world: on the gun's axis, inside the turret.
    pub fn breech(&self) -> Vec3 {
        self.gun_frame().transform_point3(Vec3::ZERO)
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
    /// The knocks tanks took in the last step.
    pub knocks: Vec<Knock>,
    /// The map's own colliders (its terrain and its compounds of everything
    /// else static): road wheels sweep against them in closed form (see
    /// `sweep`).
    ground: Vec<ColliderHandle>,
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
            HullKind::Kataphrakt,
            TurretKind::Kontos,
        );
        w
    }

    /// Map with static colliders and no tanks.
    pub fn empty(map: Map) -> World {
        let mut physics = PhysicsWorld::default();
        physics.gravity = Vec3::new(0.0, -GRAVITY, 0.0);
        physics.integration_parameters.dt = DT;
        let ground = add_static_colliders(&mut physics, &map);
        // Initialize the broad phase so the first suspension raycasts see the ground.
        physics.step();
        World {
            map,
            tanks: Vec::new(),
            physics,
            time: 0.0,
            knocks: Vec::new(),
            ground,
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
        let turret_hitbox = insert_turret_hitbox(&mut self.physics, body, turret);
        let chassis = Chassis::of(hull);
        let mut t = Tank {
            color,
            company: false,
            hull_kind: hull,
            turret_kind: turret,
            chassis,
            hull: HullStats::of(hull),
            body,
            collider,
            turret_hitbox,
            hitbox_kind: turret,
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
            gun_yaw: 0.0,
            track_dist: [0.0; 2],
            since_fire: 10.0,
            fire_barrel: 0,
            heat: 0.0,
            overheated: false,
            beaming: false,
            flipped_time: 0.0,
            age: 0.0,
            dead: false,
            empty: false,
            husk: None,
            lean: 0.0,
            spool: 0.0,
            charge: 0.0,
            spin: 0.0,
            prev_pos: Vec3::ZERO,
            prev_rot: Quat::IDENTITY,
            prev_turret_rel: 0.0,
            prev_track_dist: [0.0; 2],
            feel: Feel::default(),
        };
        sync_from_body(&mut t, &self.physics);
        t.place_turret_hitbox(&mut self.physics);
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
        self.physics.bodies[body].set_enabled(!t.empty);
        t.body = body;
        t.collider = collider;
        t.turret_hitbox = insert_turret_hitbox(&mut self.physics, body, t.turret_kind);
        t.hitbox_kind = t.turret_kind;
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
        self.recoil(i);
    }

    /// Kick tank `i` back by one shot's momentum, at gun height, opposite to
    /// where the gun points.
    pub fn recoil(&mut self, i: usize) {
        let t = &self.tanks[i];
        let b = &mut self.physics.bodies[t.body];
        let at = b.translation() + t.rot * Vec3::Y * t.gun_height();
        b.apply_impulse_at_point(-t.gun_dir() * t.turret_kind.recoil(), at, true);
    }

    /// Empty seat `i` (`empty`), taking its tank out of the world, or put
    /// a tank in it.
    pub fn set_empty(&mut self, i: usize, empty: bool) {
        let t = &mut self.tanks[i];
        t.empty = empty;
        self.physics.bodies[t.body].set_enabled(!empty);
    }

    /// Put tank `i` down at `at` facing `yaw`, at rest, turret forward (respawns).
    pub fn teleport(&mut self, i: usize, at: Vec2, yaw: f32) {
        let y = self.map.ground_height(at.x, at.y);
        self.put(i, Vec3::new(at.x, y, at.y), yaw);
    }

    /// `teleport`, onto the floor nearest `at`'s height (in a dig).
    #[cfg(test)]
    pub fn teleport_near(&mut self, i: usize, at: Vec3, yaw: f32) {
        let y = self.map.ground_near(at.x, at.y, at.z);
        self.put(i, Vec3::new(at.x, y, at.z), yaw);
    }

    fn put(&mut self, i: usize, at: Vec3, yaw: f32) {
        let t = &mut self.tanks[i];
        let b = &mut self.physics.bodies[t.body];
        b.set_position(
            Pose::from_parts(at + Vec3::Y * 0.15, Quat::from_rotation_y(yaw)),
            true,
        );
        b.set_linvel(Vec3::ZERO, true);
        b.set_angvel(Vec3::ZERO, true);
        t.turret_rel = 0.0;
        t.turret_rate = 0.0;
        t.turret_centering = false;
        t.gun_pitch = 0.0;
        t.gun_yaw = 0.0;
        t.belt = [0.0; 2];
        t.parked = true;
        t.flipped_time = 0.0;
        t.feel = Feel::default();
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

    /// Which tank owns a collider (its hull or its turret), if any.
    pub fn tank_of(&self, c: ColliderHandle) -> Option<usize> {
        self.tanks
            .iter()
            .position(|t| t.collider == c || t.turret_hitbox == c)
    }

    pub fn step(&mut self, cmds: &[Command]) {
        self.time += DT;
        for t in &mut self.tanks {
            t.snapshot();
        }
        let crowded = self.crowded();
        for i in 0..self.tanks.len() {
            let cmd = cmds.get(i).copied().unwrap_or_default();
            let t = &mut self.tanks[i];
            if t.empty {
                continue;
            }
            t.drive = cmd.throttle.abs().max(cmd.steer.abs()).min(1.0);
            let friction = if t.dead { WRECK_FRICTION } else { 0.0 };
            if let Some(c) = self.physics.colliders.get_mut(t.collider) {
                if c.friction() != friction {
                    c.set_friction(friction);
                }
            }
            fit_mass(t, &mut self.physics);
            drive(
                t,
                &cmd,
                &mut self.physics,
                &self.ground,
                &self.map.rivers,
                crowded[i],
            );
            turret(t, &cmd, DT);
            t.place_turret_hitbox(&mut self.physics);
            t.since_fire += DT;
            t.age += DT;
        }
        for t in &mut self.tanks {
            t.feel.vel = self.physics.bodies[t.body].linvel();
        }
        self.physics.step();
        self.knocks.clear();
        for i in 0..self.tanks.len() {
            if self.tanks[i].empty {
                continue;
            }
            sync_from_body(&mut self.tanks[i], &self.physics);
            self.feel(i);
            recover_if_flipped(&mut self.tanks[i], &mut self.physics);
        }
    }

    /// For each tank, whether its wheels could touch anything but the map
    /// (another tank, or anything else put in the world) this step. Most of
    /// the time they can't, and their sweeps skip the general cast.
    fn crowded(&self) -> Vec<bool> {
        // Anything that's neither the map nor a tank's (a test's ledge, say):
        // every tank's wheels might touch it.
        let tanks_own = |h: ColliderHandle| {
            (self.tanks.iter()).any(|t| t.collider == h || t.turret_hitbox == h)
        };
        let extra = (self.physics.colliders.iter())
            .any(|(h, _)| !self.ground.contains(&h) && !tanks_own(h));
        let reach: Vec<f32> = self
            .tanks
            .iter()
            .map(|t| Chassis::reach(t.hull_kind))
            .collect();
        (0..self.tanks.len())
            .map(|i| {
                extra
                    || (0..self.tanks.len()).any(|j| {
                        j != i
                            && !self.tanks[j].empty
                            && self.tanks[i].pos.distance(self.tanks[j].pos) < reach[i] + reach[j]
                    })
            })
            .collect()
    }

    /// What tank `i` felt in the step just taken: the speed the contact
    /// solver took off it (a wall, another tank, the ground coming up under
    /// its belly), touching down after a drop, or going over onto its side.
    fn feel(&mut self, i: usize) {
        let colliders: Vec<ColliderHandle> = self.tanks.iter().map(|t| t.collider).collect();
        let t = &mut self.tanks[i];
        let pose = *self.physics.bodies[t.body].position();
        // Gravity and the track model's own forces are in `feel.vel`, so
        // what's left is what the solver did: contacts.
        let jolt = t.vel - t.feel.vel + Vec3::Y * GRAVITY * DT;
        let (mut sum, mut weight, mut rammed) = (Vec3::ZERO, 0.0, false);
        for pair in self.physics.contact_pairs_with(t.collider) {
            let PairContacts::Rigid(rc) = &pair.contacts else {
                continue;
            };
            let ours_first = pair.collider1 == t.collider;
            let other = if ours_first {
                pair.collider2
            } else {
                pair.collider1
            };
            for m in rc.solver_manifolds() {
                for p in &m.points {
                    if p.data.impulse <= 0.0 {
                        continue;
                    }
                    let local = if ours_first { p.local_p1 } else { p.local_p2 };
                    sum += pose.transform_point(local) * p.data.impulse;
                    weight += p.data.impulse;
                    rammed |= colliders.contains(&other);
                }
            }
        }
        let at = if weight > 0.0 { sum / weight } else { t.pos };

        let (grounded, up, pos, speed) = (t.grounded(), t.up(), t.pos, t.vel.length());
        let f = &mut t.feel;
        f.since_land += DT;
        if jolt.length() > JOLT_MIN {
            if f.jolt == Vec3::ZERO {
                f.at = at;
            }
            f.jolt += jolt;
            f.rammed |= rammed;
        } else if f.jolt != Vec3::ZERO {
            let strength = f.jolt.length();
            let flat = Vec2::new(f.jolt.x, f.jolt.z).length();
            let kind = if f.rammed {
                Some(KnockKind::Tank)
            } else if flat > f.jolt.y.abs() {
                Some(KnockKind::Wall)
            } else if f.since_land > LAND_HOLD {
                // A belly slam (cresting a rise, coming down nose first).
                Some(KnockKind::Land)
            } else {
                // The touchdown already sounded for this.
                None
            };
            if let Some(kind) = kind.filter(|_| strength > KNOCK_MIN) {
                if kind == KnockKind::Land {
                    f.since_land = 0.0;
                }
                self.knocks.push(Knock {
                    tank: i,
                    kind,
                    at: f.at,
                    strength,
                });
            }
            f.jolt = Vec3::ZERO;
            f.rammed = false;
        }

        if grounded {
            if f.air > AIR_MIN && f.drop > KNOCK_MIN && f.since_land > LAND_HOLD {
                f.since_land = 0.0;
                self.knocks.push(Knock {
                    tank: i,
                    kind: KnockKind::Land,
                    at: pos - up * 0.5,
                    strength: f.drop,
                });
            }
            f.air = 0.0;
            f.drop = 0.0;
        } else {
            f.air += DT;
            f.drop = f.drop.max(-f.vel.y);
        }

        if !f.tipped && up.y < TIPPED {
            f.tipped = true;
            let spin = self.physics.bodies[t.body].angvel().length();
            let strength = spin * 2.0 + speed;
            if strength > KNOCK_MIN {
                self.knocks.push(Knock {
                    tank: i,
                    kind: KnockKind::Tip,
                    at: pos,
                    strength,
                });
            }
        } else if f.tipped && up.y > UPRIGHT {
            f.tipped = false;
        }
    }

    /// Distance along a ray until it hits static geometry, capped at `max`.
    /// Like [`World::raycast`], with the surface normal where it struck
    /// (None if nothing static is within `max`).
    pub fn raycast_normal(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<(f32, Vec3)> {
        let ray = Ray::new(origin, dir);
        self.physics
            .cast_ray_and_get_normal(&ray, max, true, QueryFilter::only_fixed())
            .map(|(_, ri)| (ri.time_of_impact, ri.normal))
    }

    pub fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> f32 {
        let ray = Ray::new(origin, dir);
        self.physics
            .cast_ray(&ray, max, true, QueryFilter::only_fixed())
            .map_or(max, |(_, toi)| toi)
    }

    /// How far a ball of radius `r` at `origin` moves along unit `dir` (up
    /// to `max`) before it touches the static world. Anything it starts in
    /// and moves out of doesn't stop it.
    pub fn ballcast(&self, origin: Vec3, dir: Vec3, r: f32, max: f32) -> f32 {
        let options = rapier3d::parry::query::ShapeCastOptions {
            max_time_of_impact: max,
            stop_at_penetration: false,
            ..Default::default()
        };
        self.physics
            .cast_shape(
                &Pose::from_translation(origin),
                dir,
                &Ball::new(r),
                options,
                QueryFilter::only_fixed(),
            )
            .map_or(max, |(_, h)| h.time_of_impact)
    }
}

/// The body's mass properties for `c` carrying `heft` times its bare mass.
fn tank_mass(c: &Chassis, heft: f32) -> MassProperties {
    MassProperties::new(c.center_of_mass, c.mass * heft, c.inertia() * heft)
}

/// Give the tank's body the mass its hull stats call for, if it hasn't it yet
/// (an upgraded hull is heavier). Takes effect at the next physics step.
fn fit_mass(t: &Tank, physics: &mut PhysicsWorld) {
    let Some(c) = physics.colliders.get_mut(t.collider) else {
        return;
    };
    let want = t.chassis.mass * t.hull.heft;
    if (c.mass() - want).abs() > 1e-3 * want {
        c.set_mass_properties(tank_mass(t.chassis, t.hull.heft));
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
        .mass_properties(tank_mass(c, 1.0))
        .friction(0.0)
        .friction_combine_rule(CoefficientCombineRule::Min)
        .restitution(0.0);
    let ch = physics.insert_collider(collider, Some(h));
    (h, ch)
}

/// A hitbox for `kind`'s turret on `body`: the convex hull of the head and
/// mantlet as drawn (the gun level), in the turret's frame. Shots see it,
/// nothing else does: a massless sensor in no collision group, so it never
/// touches the ground, walls or other tanks, and the drive's wheel casts skip
/// it. Its pose is kept on the turret by [`Tank::place_turret_hitbox`].
fn insert_turret_hitbox(
    physics: &mut PhysicsWorld,
    body: RigidBodyHandle,
    kind: TurretKind,
) -> ColliderHandle {
    let c = ColliderBuilder::new(turret_shape(kind))
        .sensor(true)
        .density(0.0)
        .collision_groups(InteractionGroups::none());
    physics.insert_collider(c, Some(body))
}

/// Each turret's hitbox shape, built once from its art.
/// A boulder's collider in its own frame (see `rocks`): the hull of the
/// surface drawn. Built once per rock; every world on the map shares it.
fn rock_hull(o: &crate::map::OBox) -> Option<SharedShape> {
    type Cache = std::sync::Mutex<std::collections::HashMap<[u32; 6], Option<SharedShape>>>;
    static CACHE: std::sync::LazyLock<Cache> = std::sync::LazyLock::new(Default::default);
    if !o.is_rock() {
        return None;
    }
    let rock = crate::map::rocks::Rock::of(o)?;
    let id = rock.id();
    if let Some(shape) = CACHE.lock().unwrap().get(&id) {
        return shape.clone();
    }
    let shape = SharedShape::convex_hull(&rock.points());
    CACHE.lock().unwrap().insert(id, shape.clone());
    shape
}

fn turret_shape(kind: TurretKind) -> SharedShape {
    static SHAPES: std::sync::OnceLock<Vec<SharedShape>> = std::sync::OnceLock::new();
    let shapes = SHAPES.get_or_init(|| {
        TurretKind::ALL
            .iter()
            .map(|&k| {
                let m = crate::tank::turret_model(k);
                let gun = k.trunnion();
                let pts: Vec<Vec3> = m
                    .head
                    .verts
                    .iter()
                    .map(|v| Vec3::from(v.pos))
                    .chain(m.mantlet.verts.iter().map(|v| Vec3::from(v.pos) + gun))
                    .collect();
                SharedShape::convex_hull(&pts).expect("turret hitbox shape")
            })
            .collect()
    });
    let i = TurretKind::ALL.iter().position(|&k| k == kind).unwrap();
    shapes[i].clone()
}

/// Returns the colliders added: the terrain heightfield's, if the map has
/// terrain, and the compounds'.
///
/// Everything else static (thousands of boxes, rocks, ramps and kerbs on a
/// city map) goes into one compound collider per friction. Contacts with a
/// compound are still made part by part, so tanks meet the same geometry, but
/// the broad phase tracks a handful of colliders instead of thousands: it
/// refits its whole tree on most steps, and that walk was a large share of
/// every step.
fn add_static_colliders(physics: &mut PhysicsWorld, map: &Map) -> Vec<ColliderHandle> {
    let ground_friction = 0.7;
    let wall_friction = 0.4;
    let mut added = Vec::new();
    // (friction, parts), one compound each.
    let mut groups: Vec<(f32, Vec<(Pose, SharedShape)>)> = Vec::new();
    let mut add = |friction: f32, pose: Pose, shape: SharedShape| match groups
        .iter_mut()
        .find(|g| g.0 == friction)
    {
        Some(g) => g.1.push((pose, shape)),
        None => groups.push((friction, vec![(pose, shape)])),
    };
    let cuboid = |min: Vec3, max: Vec3| {
        let half = (max - min) * 0.5;
        (
            Pose::from_translation(min + half),
            SharedShape::cuboid(half.x, half.y, half.z),
        )
    };
    match &map.terrain {
        Some(t) => {
            let ground = match &map.solid {
                Some(s) => surface_collider(s),
                None => terrain_collider(t, |_| true),
            };
            added.push(physics.insert_collider(ground.friction(ground_friction), None));
            // Flat apron around (not under) the terrain. Where the map has
            // ground of its own beyond the edge, only the part of the apron
            // inside the arena (if the play terrain stops short of its edge).
            let arena = |(min, max): (Vec2, Vec2)| match map.scenery_terrain {
                None => Some((min, max)),
                Some(_) => {
                    let (min, max) = (min.max(Vec2::ZERO), max.min(Vec2::splat(map.size)));
                    (min.x < max.x && min.y < max.y).then_some((min, max))
                }
            };
            let rects = apron_rects(t.origin, t.origin + t.size(), map.size);
            for (min, max) in rects.into_iter().filter_map(arena) {
                let (pose, shape) =
                    cuboid(Vec3::new(min.x, -2.0, min.y), Vec3::new(max.x, 0.0, max.y));
                add(ground_friction, pose, shape);
            }
        }
        None => {
            let (pose, shape) = cuboid(
                Vec3::new(
                    map.size * 0.5 - map.size * 2.0,
                    -2.0,
                    map.size * 0.5 - map.size * 2.0,
                ),
                Vec3::new(
                    map.size * 0.5 + map.size * 2.0,
                    0.0,
                    map.size * 0.5 + map.size * 2.0,
                ),
            );
            add(ground_friction, pose, shape);
        }
    }
    // The ground beyond the edge (dunes, rock, ash flats) is solid where
    // it's drawn: a tank knocked out of the arena lands on what you see
    // (and dies of it, see `combat`). Under the arena it's cut away.
    if let Some(st) = &map.scenery_terrain {
        let s = map.size;
        let outside = |c: Vec2| c.x < 0.0 || c.y < 0.0 || c.x > s || c.y > s;
        added.push(physics.insert_collider(
            terrain_collider(st, outside).friction(ground_friction),
            None,
        ));
    }
    for s in &map.solids {
        let (pose, shape) = cuboid(s.min, s.max);
        add(wall_friction, pose, shape);
    }
    for o in map.oboxes.iter().filter(|o| o.solid) {
        let hy = (o.y1 - o.y0) * 0.5;
        // Ramps (collapsed slabs) grip like ground; walls are slick-ish.
        let friction = if o.is_ramp() {
            ground_friction
        } else {
            wall_friction
        };
        // A boulder is drawn as a lump inside its box: collide with the lump.
        if let Some(b) = rock_hull(o) {
            add(friction, Pose::from_parts(o.center3(), o.rotation()), b);
            continue;
        }
        add(
            friction,
            Pose::from_parts(o.center3(), o.rotation()),
            SharedShape::cuboid(o.half.x, hy, o.half.y),
        );
    }
    for c in map.crystals.iter().filter(|c| c.solid) {
        if let Some(b) = SharedShape::convex_hull(&c.points()) {
            add(wall_friction, Pose::IDENTITY, b);
        }
    }
    // Low visual slabs (sidewalks, plazas, lawns) are real curbs to roll over.
    for b in &map.boxes {
        let h = b.max.y - b.min.y;
        if b.min.y.abs() < 1e-3 && h > 0.0 && h <= 0.3 {
            let (pose, shape) = cuboid(b.min, b.max);
            add(ground_friction, pose, shape);
        }
    }
    for w in &map.wedges {
        if let Some(b) = wedge_shape(w) {
            add(ground_friction, Pose::IDENTITY, b);
        }
    }
    for (friction, parts) in groups {
        added.push(physics.insert_collider(
            ColliderBuilder::new(SharedShape::compound(parts)).friction(friction),
            None,
        ));
    }
    added
}

/// A solid's surface as a collider.
fn surface_collider(s: &crate::map::solid::Surface) -> ColliderBuilder {
    ColliderBuilder::trimesh_with_flags(
        s.driven.0.clone(),
        s.driven.1.clone(),
        TriMeshFlags::FIX_INTERNAL_EDGES,
    )
    .expect("a solid's surface has triangles")
}

/// A heightfield collider for `t`, keeping only the cells whose centers
/// `keep` says yes to.
fn terrain_collider(t: &Terrain, keep: impl Fn(Vec2) -> bool) -> ColliderBuilder {
    use rapier3d::parry::shape::{HeightField, HeightFieldCellStatus};
    // Column-major, rows advance along z and columns along x.
    let mut data = Vec::with_capacity(t.nx * t.nz);
    for ix in 0..t.nx {
        for iz in 0..t.nz {
            data.push(t.at(ix, iz));
        }
    }
    let size = t.size();
    let heights = rapier3d::parry::utils::Array2::new(t.nz, t.nx, data);
    let mut hf = HeightField::new(heights, Vec3::new(size.x, 1.0, size.y));
    for iz in 0..t.nz - 1 {
        for ix in 0..t.nx - 1 {
            let c = t.origin + (Vec2::new(ix as f32, iz as f32) + 0.5) * t.cell;
            if !keep(c) {
                hf.set_cell_status(iz, ix, HeightFieldCellStatus::CELL_REMOVED);
            }
        }
    }
    ColliderBuilder::new(SharedShape::new(hf)).translation(Vec3::new(
        t.origin.x + size.x * 0.5,
        0.0,
        t.origin.y + size.y * 0.5,
    ))
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

fn wedge_shape(w: &Wedge) -> Option<SharedShape> {
    SharedShape::convex_hull(&w.points())
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

fn drive(
    t: &mut Tank,
    cmd: &Command,
    physics: &mut PhysicsWorld,
    ground: &[ColliderHandle],
    rivers: &[crate::map::river::River],
    crowded: bool,
) {
    if t.chassis.hover {
        hover(t, cmd, physics, rivers);
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
        // Turret hitboxes are for shots only: wheels pass through them. The
        // map itself is swept in closed form below; the general cast only
        // sees the rest (other tanks).
        let not_ground = |h: ColliderHandle, _: &Collider| !ground.contains(&h);
        let filter = QueryFilter::default()
            .exclude_rigid_body(t.body)
            .exclude_sensors()
            .predicate(&not_ground);
        let qp = physics.query_pipeline_with_filter(filter);
        // Each wheel is a ball swept down from its mount, not a thin ray: it
        // rolls up onto an edge as it reaches it instead of snapping to the
        // top when its center passes over (which jolted the hull on every
        // kerb, rubble step and wall stub). Results are reported as the
        // equivalent ray length, so the spring math is unchanged.
        let wheel_r = c.road_r * WHEEL_CAST_RADIUS;
        let ball = rapier3d::parry::shape::Ball::new(wheel_r);
        let options = rapier3d::parry::query::ShapeCastOptions {
            max_time_of_impact: c.free_len() - wheel_r,
            target_distance: 0.0,
            stop_at_penetration: true,
            compute_impact_geometry_on_penetration: true,
        };
        // The map's parts near any of the wheels, gathered once for all of them.
        use rapier3d::parry::bounding_volume::{Aabb, BoundingVolume};
        let mut around: Option<Aabb> = None;
        for i in 0..n_wheels {
            let origin = pose.transform_point(c.mount(i));
            let b = crate::sweep::swept_aabb(origin, -up, wheel_r, options.max_time_of_impact);
            around = Some(around.map_or(b, |a| a.merged(&b)));
        }
        let near: Vec<(&Collider, Vec<u32>)> = match around {
            Some(around) => ground
                .iter()
                .filter_map(|&h| physics.colliders.get(h))
                .map(|col| {
                    let parts = col.shape().as_compound().map_or(Vec::new(), |cp| {
                        crate::sweep::compound_parts_near(cp, col.position(), &around)
                    });
                    (col, parts)
                })
                .collect(),
            None => Vec::new(),
        };
        for (i, hit) in ray_hits.iter_mut().enumerate() {
            let origin = pose.transform_point(c.mount(i));
            let at = Pose::from_parts(origin, pose.rotation);
            // The ground's outward normal (normal2 is the wheel's own).
            let mut first = if crowded {
                qp.cast_shape(&at, -up, &ball, options)
                    .map(|(_, h)| (h.time_of_impact, h.normal1.normalize_or_zero()))
            } else {
                None
            };
            for (col, parts) in &near {
                let reach = first.map_or(options.max_time_of_impact, |f| f.0);
                let (shape, at) = (col.shape(), col.position());
                let hit = if let Some(hf) = shape.as_heightfield() {
                    crate::sweep::ball_heightfield(hf, at, origin, -up, wheel_r, reach)
                } else if let Some(mesh) = shape.as_trimesh() {
                    crate::sweep::ball_trimesh(mesh, at, origin, -up, wheel_r, reach)
                } else if let Some(cp) = shape.as_compound() {
                    let options = rapier3d::parry::query::ShapeCastOptions {
                        max_time_of_impact: reach,
                        ..options
                    };
                    crate::sweep::ball_compound(cp, at, parts, origin, -up, wheel_r, options)
                } else {
                    None
                };
                if let Some(g) = hit.filter(|g| first.is_none_or(|f| g.0 < f.0)) {
                    first = Some(g);
                }
            }
            if let Some((toi, n)) = first {
                let n = if n.dot(up) > 0.0 { n } else { up };
                *hit = Some((origin, n, toi + wheel_r));
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
    // The ground plane under the whole tank: the average of the wheels'
    // ground normals. Bump faces under one or two wheels barely tilt it; a
    // hill the tank is on does.
    let plane_n = {
        let sum: Vec3 = ray_hits.iter().flatten().map(|h| h.1).sum();
        if sum.length_squared() > 1e-6 {
            sum.normalize()
        } else {
            Vec3::Y
        }
    };
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
                // Damp motion toward/away from the ground plane under the
                // tank. Not along the hull's own up axis (with the nose a few
                // degrees up, forward speed read as sinking and the damper
                // lifted the hull off its tracks), and not against each
                // wheel's local patch (on a bump's face, forward speed read
                // as charging the ground and the damper launched the tank).
                let v_along = b.velocity_at_point(*origin).dot(plane_n);
                // Shock absorbers sit mainly on the end stations, like real
                // tanks': they damp pitching hard without making the ride
                // over a bump harsh.
                let z = c.road_z[i % c.road_z.len()];
                let reach = c.road_z.iter().fold(0.0f32, |m, z| m.max(z.abs()));
                let damp = damp * (0.6 + END_STATION_DAMPING * (z.abs() / reach).powi(2));
                let spring = k * compression;
                let force = (spring + k * 8.0 * over - damp * v_along).max(0.0);
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
    if t.dead {
        // A wreck's drive train is seized: the belts don't turn, so it skids
        // to a stop on its tracks (and holds on a slope) instead of coasting.
        t.belt = [0.0; 2];
        t.parked = true;
    } else if throttle.abs() > 0.05 && u_avg * throttle >= -0.5 {
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
    if !t.dead {
        let want_diff = steer * h.turn_rate * 2.0 * c.track_x;
        let limit = dv(mass * h.steer_accel);
        let corr = (want_diff - (t.belt[1] - t.belt[0])).clamp(-limit, limit);
        t.belt[0] -= corr * 0.5;
        t.belt[1] += corr * 0.5;
    }

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
        let impulses: Vec<(Vec3, Vec3, Vec3)> = contacts
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
                // A wreck's seized belt doesn't move, so it's all on the hull.
                let belt_share = m_belt / per_side[ct.side] as f32;
                let m_eff = if t.dead {
                    share
                } else {
                    1.0 / (1.0 / share + 1.0 / belt_share)
                };
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
                Some((tangent * j_long, lateral * j_lat, ct.point))
            })
            .collect();
        if !t.dead {
            for s in 0..2 {
                t.belt[s] += belt_reaction[s] / m_belt;
            }
        }
        let com = b.center_of_mass();
        // Track forces act part way up from the ground toward the center
        // of mass: along the track by [`ANTI_SQUAT`] speeding up and
        // [`ANTI_DIVE`] slowing down, across it by [`ANTI_ROLL`].
        let fade = (v_now.abs() / DIVE_FADE_SPEED).clamp(0.0, 1.0);
        let fade = fade * fade * (3.0 - 2.0 * fade);
        for (along, across, p) in impulses {
            let arm = (com - p).dot(up).max(0.0);
            let lift = if along.dot(fwd) * v_now < 0.0 {
                ANTI_SQUAT + (ANTI_DIVE - ANTI_SQUAT) * fade
            } else {
                ANTI_SQUAT
            };
            b.apply_impulse_at_point(along, p + up * arm * lift, true);
            b.apply_impulse_at_point(across, p + up * arm * ANTI_ROLL, true);
        }
    }
    t.track_dist[0] += t.belt[0] * DT;
    t.track_dist[1] += t.belt[1] * DT;
}

/// Hover drive: each emitter is a spring of plasma against whatever is below
/// it. Thrust, braking, sideways grip and turning all push through the
/// cushion, so they fade as the hull lifts off and stop in the air.
fn hover(
    t: &mut Tank,
    cmd: &Command,
    physics: &mut PhysicsWorld,
    rivers: &[crate::map::river::River],
) {
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
        // The cushion pushes off the ground, not off other tanks: riding up
        // onto a hull it ran into launched it end over end.
        let filter = QueryFilter::only_fixed();
        let qp = physics.query_pipeline_with_filter(filter);
        for (i, hit) in hits.iter_mut().enumerate() {
            let origin = pose.transform_point(c.mount(i));
            if let Some((_, h)) = qp.cast_ray_and_get_normal(&Ray::new(origin, -up), reach, true) {
                *hit = Some((origin, h.normal, h.time_of_impact));
            }
            // A plasma river's surface is as good as ground to it.
            let over = Vec2::new(origin.x, origin.z);
            if up.y > 0.3
                && let Some(y) = rivers.iter().find_map(|r| r.surface(over, 0.0))
            {
                let d = (origin.y - y).max(0.0) / up.y;
                if d < reach && hit.is_none_or(|(_, _, t)| d < t) {
                    *hit = Some((origin, Vec3::Y, d));
                }
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
        // A face too steep to drive up gives the emitter nothing to stand
        // on: otherwise the cushion holds the hull against gravity on it
        // and thrusts up it, and the hull climbs a crystal and rolls off.
        let footing =
            ((nrm.y - HOVER_STEEP.cos()) / (HOVER_FIRM.cos() - HOVER_STEEP.cos())).clamp(0.0, 1.0);
        let w = (1.0 - (toi - c.free_len()).max(0.0) / HOVER_REACH).clamp(0.0, 1.0) * footing;
        grip += w / n as f32;
        ground_n += nrm * w;
        if compression > 0.0 {
            t.wheels_down += 1;
            // Past the belly gap the hull is about to scrape: stiffen hard.
            let over = (compression - HOVER_SAG - HOVER_BELLY_GAP * 0.8).max(0.0);
            let v_along = b.velocity_at_point(origin).dot(up);
            let force = (k * compression + k * 6.0 * over - damp * v_along).max(0.0) * footing;
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

    // Attitude hold: the emitters gimbal to keep the hull level. On the
    // cushion the springs do that; as it loses the ground (off a crest, up
    // over a wreck or another tank's prow) the emitters swing to hold it, so
    // it lands on its belly instead of going end over end. Capped, so a hit
    // or a hard landing still knocks it about.
    let target = (gn * grip + Vec3::Y * (1.0 - grip)).normalize_or(Vec3::Y);
    let tilt = up.cross(target);
    let wv = b.angvel();
    let w_tip = wv - up * wv.dot(up);
    let level = (1.0 - grip).max(HOVER_LEVEL_ON_CUSHION);
    let alpha = (tilt * HOVER_LEVEL_K - w_tip * HOVER_LEVEL_DAMP) * level;
    let alpha = alpha.clamp_length_max(HOVER_LEVEL_MAX);
    let local = pose.rotation.inverse() * alpha;
    let torque = pose.rotation * (local * c.inertia());
    b.apply_torque_impulse(torque * DT, true);

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

/// A run of steps where the contact solver takes more than this off a
/// tank's speed, each step, is a knock, m/s. Resting and driving over
/// ordinary ground stay well under it.
const JOLT_MIN: f32 = 0.25;
/// Weaker knocks than this (speed lost, m/s) make no sound.
const KNOCK_MIN: f32 = 2.0;
/// Off the ground at least this long before touching down is a landing, s.
const AIR_MIN: f32 = 0.12;
/// After a landing, the next is a separate one only this much later, s.
const LAND_HOLD: f32 = 0.3;
/// The hull's up axis below this (y) has gone over onto its side, and back
/// above `UPRIGHT` it's back on its tracks.
const TIPPED: f32 = 0.3;
const UPRIGHT: f32 = 0.8;

/// Something a tank ran into, came down on, or fell over, this step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Knock {
    pub tank: usize,
    pub kind: KnockKind,
    /// Where it hit.
    pub at: Vec3,
    /// How hard: speed it lost, m/s (for `Tip`, how fast it went over).
    pub strength: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnockKind {
    /// A wall, a rock, anything static.
    Wall,
    /// Another tank (each of the two feels it).
    Tank,
    /// Coming down on the ground: a drop, a jump, a belly slam.
    Land,
    /// Going over onto its side or roof.
    Tip,
}

/// Per tank, between steps.
#[derive(Clone, Copy, Debug, Default)]
struct Feel {
    /// Velocity going into the solve.
    vel: Vec3,
    /// Speed the solver has taken off over this run of jolts, where it
    /// started, and whether another tank was in it.
    jolt: Vec3,
    at: Vec3,
    rammed: bool,
    /// Seconds off the ground, and the fastest it sank meanwhile, m/s.
    air: f32,
    drop: f32,
    since_land: f32,
    tipped: bool,
}

/// Upside down or on its side for a while: put it back on its tracks. A
/// wreck has no crew to right it: it stays however it landed.
fn recover_if_flipped(t: &mut Tank, physics: &mut PhysicsWorld) {
    if t.dead {
        t.flipped_time = 0.0;
        return;
    }
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

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    /// A 200 m walled yard, the tank in the middle facing +z.
    pub fn flat_world() -> World {
        World::new(Map::yard(200.0), 0)
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

    #[test]
    fn wheels_still_stand_on_another_tank() {
        // A tank dropped onto another's roof: its wheels land on that hull,
        // which only the general cast sees (the map's own sweep doesn't).
        let mut w = World::empty(Map::yard(200.0));
        let at = Vec2::new(100.0, 100.0);
        w.spawn_tank(Vec3::ONE, at, 0.0, HullKind::Gravis, TurretKind::Kontos);
        w.spawn_tank(
            Vec3::ONE,
            at + Vec2::new(60.0, 0.0),
            0.0,
            HullKind::Vigil,
            TurretKind::Kontos,
        );
        // Far apart, neither is crowded.
        assert_eq!(w.crowded(), vec![false, false]);
        // Just over the Gravis's roof.
        let roof = Vec3::new(at.x, Chassis::GRAVIS.hull_top + 0.15, at.y);
        w.physics.bodies[w.tanks[1].body].set_position(Pose::from_translation(roof), true);
        w.tanks[1].pos = roof;
        assert_eq!(w.crowded(), vec![true, true]);
        // A few steps: the physics moves its collider up there on the first.
        for _ in 0..10 {
            w.step(&[]);
        }
        assert!(
            w.tanks[1].wheels_down > 0,
            "its wheels found the hull under them"
        );
        assert!(
            w.tanks[1].pos.y > Chassis::GRAVIS.hull_top - 0.2,
            "and it stands up there"
        );
    }

    /// Ride on a course: (mean |pitch/roll rate|, peak pitch/roll rate,
    /// worst vertical velocity change in one step, airborne fraction).
    fn ride(map: Map, start: Vec2, yaw: f32, hull: HullKind, seconds: f32) -> (f32, f32, f32, f32) {
        let mut w = World::empty(map);
        w.spawn_tank(Vec3::ONE, start, yaw, hull, TurretKind::Kontos);
        run(&mut w, Command::default(), 1.0);
        let (mut sum, mut peak, mut jolt, mut air, mut n) =
            (0.0f32, 0.0f32, 0.0f32, 0usize, 0usize);
        let mut last = w.tanks[0].vel.y;
        for _ in 0..(seconds / DT) as usize {
            w.step(&[throttle(1.0)]);
            let t = &w.tanks[0];
            let local = t.rot.inverse() * w.physics.bodies[t.body].angvel();
            let r = Vec2::new(local.x, local.z).length();
            sum += r;
            peak = peak.max(r);
            jolt = jolt.max((t.vel.y - last).abs());
            last = t.vel.y;
            if t.wheels_down == 0 && t.face_contacts == 0 {
                air += 1;
            }
            n += 1;
        }
        (sum / n as f32, peak, jolt, air as f32 / n as f32)
    }

    /// The most a hull may tip pulling away flat out, degrees.
    const LAUNCH_PITCH: f32 = 3.0;

    fn hull_pitch(t: &Tank) -> f32 {
        t.forward().y.asin().to_degrees()
    }

    fn parked_tank(hull: HullKind) -> World {
        let mut w = World::empty(flat_world().map);
        w.spawn_tank(
            Vec3::ONE,
            Vec2::new(100.0, 20.0),
            0.0,
            hull,
            TurretKind::Kontos,
        );
        run(&mut w, Command::default(), 2.0);
        w
    }

    /// Pulling away from a standstill squats the hull only a little, so the
    /// gun stays near its mark; braking hard from top speed (the brakes push
    /// harder than the engine) dives it further, without lifting it off its
    /// tracks. Then it settles.
    #[test]
    fn launching_stays_calm_and_braking_rocks_harder() {
        for hull in [
            HullKind::Kataphrakt,
            HullKind::Vigil,
            HullKind::Kastron,
            HullKind::Gravis,
        ] {
            let mut w = parked_tank(hull);
            let wheels = w.tanks[0].chassis.wheel_count();
            let (mut up, mut down, mut fewest) = (0.0f32, 0.0f32, wheels);
            for i in 0..(7.0 / DT) as usize {
                let cmd = match i as f32 * DT {
                    t if t < 4.0 => throttle(1.0),
                    t if t < 4.8 => throttle(-1.0),
                    _ => Command::default(),
                };
                w.step(&[cmd]);
                let t = &w.tanks[0];
                up = up.max(hull_pitch(t));
                down = down.min(hull_pitch(t));
                fewest = fewest.min(t.wheels_down);
            }
            assert!(up < LAUNCH_PITCH, "{hull:?} squatted {up} deg pulling away");
            assert!(
                -down > 2.0 * up,
                "{hull:?}: braking dived {down} deg, launching {up}"
            );
            assert!(
                fewest >= wheels / 2,
                "{hull:?}: down to {fewest} of {wheels} wheels on the ground"
            );
            let t = &w.tanks[0];
            assert!(
                hull_pitch(t).abs() < 1.0 && t.speed() < 0.2,
                "{hull:?} still rocking 2 s after braking: {} deg",
                hull_pitch(t)
            );
        }
    }

    /// Braking to a stop and holding on into reverse, or pivoting in
    /// place, never jolts the hull: the brakes' lift fades into the
    /// engine's as the speed falls, so the hull's pitching never kicks
    /// harder than it did the moment the brakes went on.
    #[test]
    fn braking_through_a_stop_never_jolts() {
        let mut w = parked_tank(HullKind::Kataphrakt);
        run(&mut w, throttle(1.0), 4.0);
        let pitch = |w: &World| hull_pitch(&w.tanks[0]);
        // How sharply the pitch's acceleration changes in a step: a lift
        // that jumped would jump the torque, and with it this.
        let (mut last, mut rate, mut accel) = (pitch(&w), 0.0f32, 0.0f32);
        let (mut on, mut through) = (0.0f32, 0.0f32);
        for i in 0..(2.5 / DT) as usize {
            w.step(&[throttle(-1.0)]);
            let now = pitch(&w);
            let r = (now - last) / DT;
            let a = (r - rate) / DT;
            let kick = (a - accel).abs();
            (last, rate, accel) = (now, r, a);
            if (2..5).contains(&i) {
                on = on.max(kick);
            } else if i >= 5 && w.tanks[0].speed().abs() < 3.0 {
                through = through.max(kick);
            }
        }
        assert!(w.tanks[0].speed() < -2.0, "never got going in reverse");
        assert!(
            through < on,
            "pitch jolted {through:.0} deg/s² per step through the stop, \
             {on:.0} as the brakes went on"
        );
        let mut w = parked_tank(HullKind::Kataphrakt);
        let mut worst = 0.0f32;
        for _ in 0..(3.0 / DT) as usize {
            w.step(&[Command {
                steer: 1.0,
                ..Default::default()
            }]);
            worst = worst.max(pitch(&w).abs());
        }
        assert!(worst < 1.5, "pivoting pitched it {worst} deg");
    }

    /// Cruising flat out in a straight line, the hull rides level: no
    /// wobble unless you rock it.
    #[test]
    fn cruises_level() {
        for hull in [HullKind::Kataphrakt, HullKind::Kastron, HullKind::Gravis] {
            let mut w = parked_tank(hull);
            run(&mut w, throttle(1.0), 6.0);
            let mut worst = 0.0f32;
            for _ in 0..(3.0 / DT) as usize {
                w.step(&[throttle(1.0)]);
                worst = worst.max(hull_pitch(&w.tanks[0]).abs());
            }
            assert!(worst < 1.5, "{hull:?} pitched {worst} deg at top speed");
        }
    }

    /// Full throttle over speed bumps and rolling hills: the suspension
    /// soaks them up instead of throwing the tank in the air.
    #[test]
    fn suspension_soaks_up_bumps() {
        let mut bumpy = Map::yard(200.0);
        bumpy.speed_bumps(6.0, 18.0, 120.0, 6);
        let mut hilly = Map::yard(200.0);
        hilly.hills();
        for hull in [HullKind::Kataphrakt, HullKind::Kastron, HullKind::Gravis] {
            let bumps = ride(bumpy.clone(), Vec2::new(12.0, 108.0), 0.0, hull, 5.0);
            let hills = ride(hilly.clone(), Vec2::new(130.0, 20.0), 0.0, hull, 8.0);
            for (name, (rate, _, jolt, air)) in [("bumps", bumps), ("hills", hills)] {
                assert!(
                    air < 0.02,
                    "{hull:?} airborne {:.0}% of the time on {name}",
                    air * 100.0
                );
                assert!(jolt < 0.6, "{hull:?} {name}: jolt {jolt} m/s in one step");
                assert!(
                    rate < 0.15,
                    "{hull:?} {name}: mean pitch/roll rate {rate} rad/s"
                );
            }
        }
    }

    fn throttle(x: f32) -> Command {
        Command {
            throttle: x,
            ..Default::default()
        }
    }

    #[test]
    fn open_spots_are_drivable() {
        for (name, m) in [
            ("reservoir", Map::reservoir(0)),
            ("sanctum", Map::sanctum()),
            ("kiln", Map::kiln()),
        ] {
            assert!(!m.open_spots.is_empty(), "{name}: no open spots");
            for p in &m.open_spots {
                for s in &m.solids {
                    let cp = p.clamp(s.min2(), s.max2());
                    assert!(
                        cp.distance(*p) >= 2.3,
                        "{name}: open spot {p} overlaps a solid"
                    );
                }
            }
        }
    }

    #[test]
    fn settles_level_on_its_wheels() {
        for hull in [HullKind::Kataphrakt, HullKind::Kastron, HullKind::Gravis] {
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
            early > 0.33 * HullStats::KATAPHRAKT.max_speed,
            "sluggish start: {early} m/s after 0.5 s"
        );
        assert!(w.tanks[0].up().y > 0.98, "wheelie: {:?}", w.tanks[0].up());
        run(&mut w, throttle(1.0), 2.5);
        let s = w.tanks[0].speed();
        assert!(
            (s - HullStats::KATAPHRAKT.max_speed).abs() < 0.5,
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

    /// Degrees a hull turns in `seconds` of full steer (with `thr` throttle).
    fn degrees_turned(hull: HullKind, thr: f32, seconds: f32) -> f32 {
        let mut w = World::empty(Map::yard(400.0));
        w.spawn_tank(
            Vec3::ONE,
            Vec2::new(200.0, 200.0),
            0.0,
            hull,
            TurretKind::Kontos,
        );
        run(&mut w, Command::default(), 1.0);
        let (mut turned, mut last) = (0.0, w.tanks[0].yaw());
        for _ in 0..(seconds / DT) as usize {
            w.step(&[Command {
                steer: 1.0,
                throttle: thr,
                ..Default::default()
            }]);
            let y = w.tanks[0].yaw();
            turned += wrap_angle(y - last);
            last = y;
        }
        turned.to_degrees()
    }

    /// The Gravis's long tracks scrub hard in a turn, but it still turns at a
    /// usable pace, on the spot and on the move, just slower than the Kastron.
    #[test]
    fn gravis_turns_slowly_but_turns() {
        for thr in [0.0, 1.0] {
            let gravis = degrees_turned(HullKind::Gravis, thr, 3.0);
            let kastron = degrees_turned(HullKind::Kastron, thr, 3.0);
            assert!(
                gravis > 120.0,
                "Gravis only turned {gravis:.0} deg in 3 s (throttle {thr})"
            );
            assert!(
                gravis < kastron,
                "Gravis ({gravis:.0} deg) out-turned the Kastron ({kastron:.0} deg)"
            );
        }
    }

    #[test]
    fn holds_position_on_slope() {
        let mut map = Map::yard(200.0);
        // A uniform 20 degree slope rising along +x; tank parked across it.
        let grade = 20f32.to_radians().tan();
        map.terrain = Some(Terrain::from_fn(Vec2::ZERO, 2.0, 101, 101, move |x, _| {
            x * grade
        }));
        let mut w = World::new(map, 0);
        run(&mut w, Command::default(), 2.0);
        let p0 = w.tanks[0].pos;
        run(&mut w, Command::default(), 3.0);
        let drift = w.tanks[0].pos.distance(p0);
        assert!(drift < 0.2, "slid {drift} m down the slope");
    }

    /// Killed at full speed, a wreck's seized tracks drag it to a stop in a
    /// few meters instead of letting it coast on.
    #[test]
    fn a_wreck_skids_to_a_stop() {
        for hull in [HullKind::Kataphrakt, HullKind::Kastron, HullKind::Gravis] {
            let mut w = World::empty(flat_world().map);
            w.spawn_tank(
                Vec3::ONE,
                Vec2::new(100.0, 20.0),
                0.0,
                hull,
                TurretKind::Kontos,
            );
            run(&mut w, throttle(1.0), 5.0);
            let (p0, v0) = (w.tanks[0].pos, w.tanks[0].vel.length());
            assert!(v0 > 5.0, "{hull:?} only got to {v0} m/s");
            w.tanks[0].dead = true;
            run(&mut w, Command::default(), 2.5);
            let t = &w.tanks[0];
            let slid = t.pos.distance(p0);
            assert!(
                t.vel.length() < 0.2,
                "{hull:?} wreck still moving at {}",
                t.vel.length()
            );
            assert!(slid < 8.0, "{hull:?} wreck went {slid} m from {v0} m/s");
        }
    }

    /// A wreck heading downhill stops and stays put; it doesn't roll away.
    #[test]
    fn a_wreck_stops_on_a_slope() {
        let mut map = Map::yard(200.0);
        // A uniform 15 degree slope falling along +z.
        let grade = 15f32.to_radians().tan();
        map.terrain = Some(Terrain::from_fn(Vec2::ZERO, 2.0, 101, 101, move |_, z| {
            (200.0 - z) * grade
        }));
        let mut w = World::empty(map);
        w.spawn_tank(
            Vec3::ONE,
            Vec2::new(100.0, 30.0),
            0.0,
            HullKind::Kataphrakt,
            TurretKind::Kontos,
        );
        run(&mut w, throttle(1.0), 3.0);
        assert!(w.tanks[0].vel.length() > 5.0);
        w.tanks[0].dead = true;
        run(&mut w, Command::default(), 3.0);
        let p0 = w.tanks[0].pos;
        run(&mut w, Command::default(), 3.0);
        let creep = w.tanks[0].pos.distance(p0);
        assert!(creep < 0.2, "wreck crept {creep} m down the slope");
    }

    /// Upside down, a wreck grinds along on its roof rather than sliding
    /// like a puck (the live hull is frictionless).
    #[test]
    fn an_upturned_wreck_drags() {
        let mut w = flat_world();
        w.tanks[0].dead = true;
        place(
            &mut w,
            Vec3::new(100.0, 2.5, 100.0),
            Quat::from_rotation_z(PI),
        );
        run(&mut w, Command::default(), 1.0);
        assert!(w.tanks[0].up().y < 0.0, "should be upside down");
        let b = &mut w.physics.bodies[w.tanks[0].body];
        b.set_linvel(Vec3::new(0.0, 0.0, 8.0), true);
        let p0 = w.tanks[0].pos;
        run(&mut w, Command::default(), 1.4);
        let t = &w.tanks[0];
        let slid = t.pos.distance(p0);
        assert!(
            t.vel.length() < 0.3,
            "upturned wreck still moving at {}",
            t.vel.length()
        );
        assert!(slid < 7.0, "upturned wreck slid {slid} m");
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
            .insert_collider(ColliderBuilder::new(wedge_shape(&kicker).unwrap()), None);
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
    fn kickers_are_clearable_at_full_speed() {
        for deg in [14.0, 22.0, 30.0] {
            let mut map = Map::yard(200.0);
            let past = map.kicker_lane(100.0, 40.0, deg);
            map.open_spots = vec![Vec2::new(100.0, 12.0)];
            let mut w = World::new(map, 0);
            run(&mut w, throttle(1.0), 7.0);
            let t = &w.tanks[0];
            assert!(
                t.pos.z > past,
                "{deg} deg kicker: stopped at z={} before the end of the landing ramp at {past}",
                t.pos.z
            );
            assert!(t.up().y > 0.95, "{deg} deg kicker: not upright");
        }
    }

    #[test]
    fn heightfield_matches_terrain_heights() {
        let mut map = Map::yard(200.0);
        map.hills();
        let mut w = World::new(map, 0);
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
        let mut map = Map::yard(200.0);
        map.speed_bumps(6.0, 18.0, 120.0, 6);
        map.stairs(28.0, 40.0, 120.0);
        let mut w = World::new(map, 0);
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
        for hull in [HullKind::Kataphrakt, HullKind::Kastron, HullKind::Gravis] {
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
            !climbs(2.0, Vec2::new(100.0, 80.0), 0.0, HullKind::Kataphrakt),
            "drove up a wall"
        );
    }

    #[test]
    fn crosses_stairs_side_on() {
        // Stairs, approached from the side.
        let mut map = Map::yard(200.0);
        map.stairs(28.0, 40.0, 120.0);
        let mut w = World::new(map, 0);
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
        let h = HullStats::KATAPHRAKT;
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

    /// A shot over the hull's deck still hits the turret drawn there, from the
    /// front and, with the turret turned, from the side; above the turret's
    /// roof it misses.
    #[test]
    fn turrets_can_be_hit() {
        for &kind in &TurretKind::ALL {
            let mut w = flat_world();
            w.tanks[0].turret_kind = kind;
            run(&mut w, Command::default(), 1.0);
            let t = &w.tanks[0];
            let ring = t.turret_frame().transform_point3(Vec3::ZERO);
            let height = |m: &crate::render::mesh::MeshData| {
                m.verts.iter().map(|v| v.pos[1]).fold(0.0f32, f32::max)
            };
            let model = crate::tank::turret_model(kind);
            let roof = height(&model.head).max(height(&model.mantlet) + kind.trunnion().y);
            let shoot = |w: &World, dir: Vec3, y: f32| {
                let from = ring + Vec3::Y * y - dir * 20.0;
                w.physics
                    .cast_ray(&Ray::new(from, dir), 40.0, true, QueryFilter::default())
                    .and_then(|(c, _)| w.tank_of(c))
            };
            let deck = t.chassis.hull_top - t.hull_kind.ring().y;
            let over = deck + 0.15;
            assert_eq!(
                shoot(&w, -Vec3::Z, over),
                Some(0),
                "{kind:?} from the front"
            );
            assert_eq!(
                shoot(&w, Vec3::Z, roof + 0.1),
                None,
                "{kind:?} over its roof"
            );
            // Turn it a quarter round: its hitbox goes with it.
            run(
                &mut w,
                Command {
                    turret: 1.0,
                    ..Command::default()
                },
                1.5,
            );
            let t = &w.tanks[0];
            let (_, rot, at) = t.turret_frame().to_scale_rotation_translation();
            let pose = w.physics.colliders[t.turret_hitbox].position();
            assert!(
                pose.translation.distance(at) < 0.05,
                "{kind:?} hitbox off the ring"
            );
            assert!(
                pose.rotation.angle_between(rot) < 0.05,
                "{kind:?} hitbox didn't turn"
            );
            assert!(
                t.turret_rel.abs() > 1.0,
                "{kind:?} turret only turned {}",
                t.turret_rel
            );
            let side = t.turret_dir().cross(Vec3::Y).normalize();
            assert_eq!(
                shoot(&w, side, over),
                Some(0),
                "{kind:?} turned, from the side"
            );
        }
    }

    /// The turret hitbox touches nothing but shots: the hull's mass is
    /// unchanged, and the wheels ride over it as if it weren't there.
    #[test]
    fn turret_hitbox_is_for_shots_only() {
        let mut w = flat_world();
        let b = &w.physics.bodies[w.tanks[0].body];
        assert_eq!(b.mass(), w.tanks[0].chassis.mass);
        let c = &w.physics.colliders[w.tanks[0].turret_hitbox];
        assert!(c.is_sensor());
        assert_eq!(c.collision_groups(), InteractionGroups::none());
        run(&mut w, Command::default(), 1.0);
        assert!(
            w.physics
                .contact_pairs_with(w.tanks[0].turret_hitbox)
                .next()
                .is_none()
        );
    }

    /// Kicked while parked (no input), the brakes hold: the recoil skids it
    /// back a little, then it stays put instead of rolling away.
    #[test]
    fn parking_brake_holds() {
        let mut w = flat_world();
        run(&mut w, Command::default(), 1.0);
        let p0 = w.tanks[0].pos;
        w.fire(0);
        run(&mut w, Command::default(), 2.0);
        let p1 = w.tanks[0].pos;
        run(&mut w, Command::default(), 2.0);
        assert!(
            p1.distance(p0) < 1.0,
            "skidded {} m after firing while parked",
            p1.distance(p0)
        );
        assert!(
            w.tanks[0].pos.distance(p1) < 0.02,
            "still rolling {} m after the kick",
            w.tanks[0].pos.distance(p1)
        );
    }

    /// A wreck stays however it landed, upside down or on its side: only a
    /// live crew rights a tank.
    #[test]
    fn a_wreck_stays_flipped() {
        for (name, rot) in [
            ("upside down", Quat::from_rotation_z(PI)),
            (
                "on its side",
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ),
        ] {
            let mut w = flat_world();
            w.tanks[0].dead = true;
            place(&mut w, Vec3::new(100.0, 2.5, 100.0), rot);
            run(&mut w, Command::default(), 1.0);
            let up0 = w.tanks[0].up();
            assert!(up0.y < 0.3, "{name}: should be tipped first: {up0:?}");
            run(&mut w, Command::default(), 8.0);
            let up = w.tanks[0].up();
            assert!(up.y < 0.3, "{name}: wreck righted itself: {up:?}");
        }
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
        // Kontos fires forward: the nose kicks up as the hull rocks back.
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

#[cfg(test)]
mod rock_tests {
    use super::*;

    /// A boulder (from the rocks toolkit) is drawn as a lump inside its
    /// box. Shots and tanks must meet that lump: no invisible box corners
    /// round it, and no driving into it.
    #[test]
    fn rocks_collide_along_the_shape_drawn() {
        let mut map = Map::empty(100.0);
        let rock = crate::map::rocks::boulder(
            Vec2::new(50.0, 50.0),
            0.4,
            Vec2::new(5.0, 4.0),
            0.0,
            4.0,
            Vec3::ONE,
        );
        // The surface drawn, in the world.
        let mut drawn = crate::render::mesh::MeshData::default();
        crate::map::rocks::Rock::of(&rock).unwrap().draw(
            &mut drawn,
            rock.center3(),
            rock.rotation(),
            None,
        );
        map.oboxes.push(rock);
        let w = World::empty(map);
        let c = Vec2::new(50.0, 50.0);
        for y in [0.8, 2.0, 3.2] {
            // Where the drawn surface crosses this height, and its outline.
            let mut cut = Vec::new();
            for t in drawn.indices.chunks(3) {
                let p: Vec<Vec3> = t
                    .iter()
                    .map(|&i| Vec3::from(drawn.verts[i as usize].pos))
                    .collect();
                for k in 0..3 {
                    let (a, b) = (p[k], p[(k + 1) % 3]);
                    if (a.y - y) * (b.y - y) < 0.0 {
                        let q = a.lerp(b, (y - a.y) / (b.y - a.y));
                        cut.push(Vec2::new(q.x, q.z));
                    }
                }
            }
            let outline = crate::map::convex_hull(cut);
            // The outline's distance from the center along a direction.
            let edge = |d: Vec2| {
                let mut t = 0.0;
                while outline.iter().enumerate().all(|(i, &a)| {
                    let b = outline[(i + 1) % outline.len()];
                    (b - a).perp_dot(c + d * t - a) >= 0.0
                }) {
                    t += 0.01;
                }
                t
            };
            for k in 0..64 {
                let a = k as f32 / 64.0 * TAU;
                let d = Vec2::new(a.cos(), a.sin());
                let from = c + d * 20.0;
                let hit = w.raycast(
                    Vec3::new(from.x, y, from.y),
                    -Vec3::new(d.x, 0.0, d.y),
                    40.0,
                );
                let at = 20.0 - hit;
                let want = edge(d);
                assert!(
                    (at - want).abs() < 0.15,
                    "{y} m up, heading {a:.2}: hit {at:.2} m out, the rock drawn is {want:.2} m out"
                );
            }
        }
        // Box corners are well outside the rock: a shot past one flies on.
        let corner = Vec2::new(4.8, 3.8);
        let (s, co) = 0.4f32.sin_cos();
        let corner = c + Vec2::new(co * corner.x + s * corner.y, -s * corner.x + co * corner.y);
        let down = w.raycast(Vec3::new(corner.x, 10.0, corner.y), -Vec3::Y, 10.5);
        assert!(
            down > 9.95,
            "a box corner of empty air stopped a ray at {down:.2} (ground is at 10)"
        );
    }
}
