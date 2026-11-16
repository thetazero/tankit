//! Hold contract (king of the hill, every tank for itself): one site per map
//! (`Map::hold_site`, else the open spot nearest the middle). A tank alone in
//! the ring holds it and banks held time; two or more in it contest it and
//! nobody scores. First to `TARGET` seconds held ends the round, otherwise
//! the clock does.

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::map::Map;
use crate::render::mesh::{Instance, MeshId};
use crate::sim::Tank;

/// Ring radius, m.
pub const RADIUS: f32 = 13.0;
/// Seconds held that win the round outright.
pub const TARGET: f32 = 90.0;
/// The player earns `HOLD_XP` for every `XP_EVERY` seconds held.
pub const XP_EVERY: f32 = 10.0;
pub const HOLD_XP: u32 = 5;
/// Tanks further than this above or below the ring's ground (a roof, a ledge) don't count.
const REACH_Y: f32 = 4.0;
const DASHES: usize = 40;
const POSTS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// Nobody in the ring.
    Open,
    /// This tank is alone in it.
    Held(usize),
    /// Two or more tanks in it.
    Contested,
}

pub struct Hold {
    pub at: Vec2,
    pub ground: f32,
    /// Seconds each tank has held the site this round.
    pub held: Vec<f32>,
    pub control: Control,
    /// Ground height under each dash of the ring.
    dash_y: Vec<f32>,
}

impl Hold {
    pub fn new(map: &Map, tanks: usize) -> Hold {
        let at = map.hold_site.unwrap_or_else(|| {
            let c = map.center();
            map.open_spots
                .iter()
                .copied()
                .min_by(|a, b| a.distance_squared(c).total_cmp(&b.distance_squared(c)))
                .unwrap_or(c)
        });
        let dash_y = (0..DASHES)
            .map(|i| {
                let p = at + ring_dir(i as f32 / DASHES as f32) * RADIUS;
                map.ground_height(p.x, p.y)
            })
            .collect();
        Hold {
            at,
            ground: map.ground_height(at.x, at.y),
            held: vec![0.0; tanks],
            control: Control::Open,
            dash_y,
        }
    }

    /// Is a tank at `p` inside the ring?
    pub fn inside(&self, p: Vec3) -> bool {
        Vec2::new(p.x, p.z).distance(self.at) < RADIUS && (p.y - self.ground).abs() < REACH_Y
    }

    /// Advance by `dt`: who's in the ring (of the tanks `counts` lets in), and
    /// bank time for a lone holder. Returns the holder, if any.
    pub fn step(
        &mut self,
        dt: f32,
        tanks: &[Tank],
        counts: impl Fn(usize) -> bool,
    ) -> Option<usize> {
        self.held.resize(tanks.len(), 0.0);
        let mut inside = (0..tanks.len()).filter(|&i| counts(i) && self.inside(tanks[i].pos));
        self.control = match (inside.next(), inside.next()) {
            (None, _) => Control::Open,
            (Some(i), None) => Control::Held(i),
            _ => Control::Contested,
        };
        let Control::Held(i) = self.control else {
            return None;
        };
        self.held[i] = (self.held[i] + dt).min(TARGET);
        Some(i)
    }

    /// Whoever has reached `TARGET`.
    pub fn winner(&self) -> Option<usize> {
        self.held.iter().position(|&h| h >= TARGET)
    }

    /// The ring painted on the ground and its four marker posts, tinted by
    /// who has it: plain paint when open, the holder's color, flickering when contested.
    pub fn instances(&self, time: f32, tanks: &[Tank], out: &mut Vec<(MeshId, Instance)>) {
        let paint = Vec3::new(0.86, 0.82, 0.72);
        let (color, glow) = match self.control {
            Control::Open => (paint, 0.0),
            Control::Held(i) => (tanks[i].color, 0.9),
            Control::Contested => (
                Vec3::new(1.0, 0.55, 0.25),
                if (time * 5.0).fract() < 0.5 { 1.2 } else { 0.3 },
            ),
        };
        let dash_len = std::f32::consts::TAU * RADIUS / DASHES as f32 * 0.55;
        for (i, &y) in self.dash_y.iter().enumerate() {
            let f = i as f32 / DASHES as f32;
            let d = ring_dir(f);
            let p = self.at + d * RADIUS;
            // Long side along the ring's tangent.
            let q = Quat::from_rotation_y(-f * std::f32::consts::TAU);
            out.push((
                MeshId::CUBE,
                Instance::new(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(0.35, 0.06, dash_len),
                        q,
                        Vec3::new(p.x, y + 0.03, p.y),
                    ),
                    color.extend(glow * 0.6),
                ),
            ));
        }
        for k in 0..POSTS {
            let i = k * DASHES / POSTS;
            let p = self.at + ring_dir(i as f32 / DASHES as f32) * RADIUS;
            let y = self.dash_y[i];
            out.push((
                MeshId::CYLINDER,
                Instance::new(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(0.22, 2.4, 0.22),
                        Quat::IDENTITY,
                        Vec3::new(p.x, y + 1.2, p.y),
                    ),
                    Vec4::new(0.2, 0.2, 0.21, 0.0),
                ),
            ));
            out.push((
                MeshId::GEM,
                Instance::new(
                    Mat4::from_scale_rotation_translation(
                        Vec3::splat(0.45),
                        Quat::from_rotation_y(time * 1.5),
                        Vec3::new(p.x, y + 2.7, p.y),
                    ),
                    color.extend(0.4 + glow),
                ),
            ));
        }
    }
}

/// Point on the unit circle a fraction `f` of the way round.
fn ring_dir(f: f32) -> Vec2 {
    let a = f * std::f32::consts::TAU;
    Vec2::new(a.sin(), a.cos())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::DT;
    use crate::sim::World;

    fn world_with(n: usize) -> (World, Hold) {
        let mut w = World::new(Map::proving_ground(), 0);
        for _ in 1..n {
            w.spawn_tank(
                Vec3::ONE,
                Vec2::ZERO,
                0.0,
                crate::sim::HullKind::Lancer,
                crate::sim::TurretKind::Lance,
            );
        }
        let h = Hold::new(&w.map, n);
        (w, h)
    }

    fn put(w: &mut World, h: &Hold, i: usize, off: Vec2) {
        let p = h.at + off;
        w.tanks[i].pos = Vec3::new(p.x, h.ground + 0.5, p.y);
    }

    #[test]
    fn a_lone_tank_holds_and_banks_time() {
        let (mut w, mut h) = world_with(2);
        put(&mut w, &h, 0, Vec2::new(2.0, 0.0));
        put(&mut w, &h, 1, Vec2::new(RADIUS + 10.0, 0.0));
        for _ in 0..60 {
            assert_eq!(h.step(DT, &w.tanks, |_| true), Some(0));
        }
        assert_eq!(h.control, Control::Held(0));
        assert!((h.held[0] - 60.0 * DT).abs() < 1e-4);
        assert_eq!(h.held[1], 0.0);
    }

    #[test]
    fn two_in_the_ring_contest_it() {
        let (mut w, mut h) = world_with(2);
        put(&mut w, &h, 0, Vec2::new(2.0, 0.0));
        put(&mut w, &h, 1, Vec2::new(-3.0, 1.0));
        assert_eq!(h.step(DT, &w.tanks, |_| true), None);
        assert_eq!(h.control, Control::Contested);
        assert!(h.held.iter().all(|&t| t == 0.0));
        // A wreck in the ring doesn't count.
        assert_eq!(h.step(DT, &w.tanks, |i| i == 0), Some(0));
    }

    #[test]
    fn tanks_overhead_dont_count() {
        let (mut w, mut h) = world_with(1);
        put(&mut w, &h, 0, Vec2::ZERO);
        w.tanks[0].pos.y += 8.0;
        assert_eq!(h.step(DT, &w.tanks, |_| true), None);
        assert_eq!(h.control, Control::Open);
    }

    #[test]
    fn reaching_the_target_wins() {
        let (mut w, mut h) = world_with(1);
        put(&mut w, &h, 0, Vec2::ZERO);
        h.held[0] = TARGET - DT * 0.5;
        assert_eq!(h.winner(), None);
        h.step(DT, &w.tanks, |_| true);
        assert_eq!(h.winner(), Some(0));
        assert_eq!(h.held[0], TARGET);
    }

    #[test]
    fn saltmarket_site_is_the_market_hall_and_reachable() {
        let m = Map::saltmarket();
        let h = Hold::new(&m, 1);
        assert_eq!(Some(h.at), m.hold_site);
        let nav = crate::ai::NavGrid::build(&m);
        assert!(!nav.is_blocked(h.at), "hold site center is blocked");
        let from = m.open_spots[0];
        assert!(!nav.path(from, h.at).is_empty(), "no path to the hold site");
    }
}
