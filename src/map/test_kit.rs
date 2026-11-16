//! Ground for tests (never built into the game): a walled, flat yard, and the
//! pieces physics tests drive over, each placed where the test says. A test
//! builds exactly the course it checks, so what it depends on is in the test.

use glam::{Vec2, Vec3};

use super::{Map, Rise, Terrain, Wedge};

/// Kicker and landing ramp height, m.
const KICKER_HEIGHT: f32 = 1.6;
/// Stair step rise, m, and how many steps up to the ledge.
pub const STEP: f32 = 0.25;
pub const STEPS: usize = 8;

impl Map {
    /// A flat square `size` m across, walled in, with one spawn in the
    /// middle facing +z and two repair crate spots.
    pub fn yard(size: f32) -> Map {
        let mut m = Map::empty(size);
        m.open_spots = vec![m.center()];
        m.repair_spots = vec![
            m.center() + Vec2::new(-30.0, -30.0),
            m.center() + Vec2::new(30.0, 30.0),
        ];
        m.perimeter_walls();
        m
    }

    /// A kicker `deg` steep across x (8 m wide, centred on `x`) starting at
    /// `z0`, and a landing ramp of the same height after a gap of about 70% of
    /// a full-speed jump, so a jump at full speed clears it and lands on the
    /// down slope. Returns where the landing ramp ends.
    pub fn kicker_lane(&mut self, x: f32, z0: f32, deg: f32) -> f32 {
        let h = KICKER_HEIGHT;
        let len = h / deg.to_radians().tan();
        self.wedges.push(Wedge {
            material: None,
            min: Vec2::new(x - 4.0, z0),
            max: Vec2::new(x + 4.0, z0 + len),
            base_y: 0.0,
            height: h,
            rise: Rise::PosZ,
            color: Vec3::new(0.86, 0.48, 0.18),
        });
        let (v, g) = (
            crate::sim::HullStats::KATAPHRAKT.max_speed,
            crate::sim::GRAVITY,
        );
        let flight = 2.0 * v * deg.to_radians().sin() / g * v * deg.to_radians().cos();
        let land_z = z0 + len + flight * 0.7;
        self.wedges.push(Wedge {
            material: None,
            min: Vec2::new(x - 5.0, land_z),
            max: Vec2::new(x + 5.0, land_z + 14.0),
            base_y: 0.0,
            height: h,
            rise: Rise::NegZ,
            color: Vec3::splat(0.6),
        });
        land_z + 14.0
    }

    /// `n` speed bumps (0.35 m tall, 2.4 m deep) across `x0..x1`, every 9 m from `z0`.
    pub fn speed_bumps(&mut self, x0: f32, x1: f32, z0: f32, n: usize) {
        for i in 0..n {
            let z = z0 + i as f32 * 9.0;
            for (rise, za, zb) in [(Rise::PosZ, z, z + 1.2), (Rise::NegZ, z + 1.2, z + 2.4)] {
                self.wedges.push(Wedge {
                    material: None,
                    min: Vec2::new(x0, za),
                    max: Vec2::new(x1, zb),
                    base_y: 0.0,
                    height: 0.35,
                    rise,
                    color: Vec3::new(0.85, 0.75, 0.2),
                });
            }
        }
    }

    /// Stairs across `x0..x1` climbing along +z from `z0`: `STEPS` steps of
    /// `STEP` m, each 2 m deep, up to a ledge 30 m long. Each step is its own
    /// column butted against the next (as a real map would build them).
    pub fn stairs(&mut self, x0: f32, x1: f32, z0: f32) {
        for i in 0..STEPS {
            let z = z0 + i as f32 * 2.0;
            let top = STEP * (i + 1) as f32;
            let end = if i == STEPS - 1 { z + 30.0 } else { z + 2.0 };
            self.add_block(
                Vec3::new(x0, 0.0, z),
                Vec3::new(x1, top, end),
                Vec3::splat(0.6),
            );
        }
    }

    /// Rolling hills, a 7 m mound at (160, 50) and a 5 m deep crater at
    /// (155, 150), over the whole yard, fading flat near the walls.
    pub fn hills(&mut self) {
        let size = self.size;
        let n = (size / 2.0) as usize + 1;
        self.terrain = Some(Terrain::from_fn(Vec2::ZERO, 2.0, n, n, move |x, z| {
            let hills =
                (x * 0.06).sin() * (z * 0.05).cos() * 3.0 + (x * 0.13 + z * 0.07).sin() * 1.2;
            let mound = 7.0 * (-((x - 160.0).powi(2) + (z - 50.0).powi(2)) / 300.0).exp();
            let crater_d = ((x - 155.0).powi(2) + (z - 150.0).powi(2)).sqrt();
            let rim = 2.0 * (-(crater_d - 22.0).powi(2) / 10.0).exp();
            let crater = if crater_d < 22.0 {
                -5.0 * (1.0 - (crater_d / 22.0).powi(2)) + rim
            } else {
                rim
            };
            let edge = (x.min(size - x).min(z).min(size - z) / 15.0).clamp(0.0, 1.0);
            edge * (hills.max(-1.5) + 1.5 + mound + crater)
        }));
    }
}
