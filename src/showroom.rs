//! Showroom: every turret (on alternating hulls) on an open yard, each running a
//! different animation (pivot turns, driving, turret sweeps, firing), with an
//! orbiting camera. `--showroom` opens it in the window or for `--shot`.

use crate::render::material::Material;
use glam::{Vec2, Vec3, Vec4};

use crate::game::DT;
use crate::map::Map;
use crate::render::Camera;
use crate::render::mesh::MeshData;
use crate::sim::{Command, HullKind, TurretKind, World};

pub const SPOTS: [Vec2; 6] = [
    Vec2::new(-10.0, -4.0),
    Vec2::new(0.0, -4.0),
    Vec2::new(10.0, -4.0),
    Vec2::new(-10.0, 5.0),
    Vec2::new(0.0, 5.0),
    Vec2::new(10.0, 5.0),
];

pub fn world() -> World {
    let mut w = World::empty(Map::empty(100.0));
    // Every turret once, on alternating hulls, in its maker's paint.
    let looks: Vec<(HullKind, TurretKind, Vec3)> = TurretKind::ALL
        .iter()
        .enumerate()
        .map(|(i, &tu)| {
            let h = if tu.maker() == crate::faction::Faction::Ithessar {
                HullKind::Sethuun
            } else if i % 2 == 0 {
                HullKind::Kataphrakt
            } else {
                HullKind::Kastron
            };
            (h, tu, tu.maker().paint())
        })
        .collect();
    for (&(h, tu, c), p) in looks.iter().zip(SPOTS) {
        w.spawn_tank(c, p, 0.4, h, tu);
    }
    w
}

/// Scripted inputs: tank 0 pivots in place, tank 1 shuttles back and forth,
/// the other two stay put sweeping their turrets.
pub fn commands(w: &World) -> Vec<Command> {
    let t = w.time;
    w.tanks
        .iter()
        .enumerate()
        .map(|(i, _)| match i {
            0 => Command {
                throttle: 0.0,
                steer: (t * 0.6).sin().signum() * ((t * 0.6).sin().abs() > 0.3) as i32 as f32,
                turret: (t * 0.5).cos() * 0.4,
                center_turret: false,
            },
            // Swings about +-2 m around its spot: tanks are solid and the neighbor is close.
            1 => Command {
                throttle: (t * 0.8).cos() * 0.12,
                steer: 0.0,
                turret: 0.0,
                center_turret: false,
            },
            _ => Command {
                throttle: 0.0,
                steer: 0.0,
                turret: (t * 0.4 + i as f32).cos() * 0.6,
                center_turret: false,
            },
        })
        .collect()
}

/// Things the sim doesn't drive yet: gun elevation and firing.
pub fn after_step(w: &mut World) {
    let t = w.time;
    for i in 0..w.tanks.len() {
        w.tanks[i].gun_pitch = 0.06 + (t * 0.7 + i as f32 * 1.3).sin() * 0.1;
        // Spin-up guns wind up and down on a slow cycle.
        if crate::combat::gun(w.tanks[i].turret_kind).spool > 0.0 {
            let tk = &mut w.tanks[i];
            tk.spool = (0.5 - 0.5 * (t * 0.8).cos()).clamp(0.0, 1.0);
            tk.spin += DT * (1.5 + 26.0 * tk.spool);
        }
        let (period, off) = (2.2 + i as f32 * 0.35, i as f32 * 0.6);
        if ((t + off) / period).floor() != ((t - DT + off) / period).floor() {
            w.fire(i);
        }
    }
}

/// Orbit around the yard (or one tank when `focus` is set).
pub fn camera(w: &World, angle: f32, dist: f32, focus: Option<usize>) -> Camera {
    let target = match focus {
        Some(i) => w.tanks[i.min(w.tanks.len() - 1)].pos + Vec3::Y * 1.2,
        None => Vec3::new(0.0, 1.0, 0.5),
    };
    let pos = target + Vec3::new(angle.sin() * dist, dist * 0.38, angle.cos() * dist);
    Camera {
        pos,
        target,
        fov_y: 50f32.to_radians(),
    }
}

pub fn ground_mesh() -> MeshData {
    let mut m = MeshData::default();
    let asphalt = Vec4::new(0.17, 0.17, 0.18, Material::Asphalt.id());
    m.add_box(
        Vec3::new(-150.0, -0.2, -150.0),
        Vec3::new(150.0, -0.02, 150.0),
        asphalt,
        asphalt,
    );
    let pave = Vec4::new(0.6, 0.57, 0.52, Material::Paving.id());
    m.add_box(
        Vec3::new(-14.0, -0.1, -13.0),
        Vec3::new(14.0, 0.0, 14.0),
        pave,
        pave,
    );
    m
}
