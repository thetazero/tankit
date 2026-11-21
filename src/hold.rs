//! Hold contract (king of the hill, side against side): one site per map,
//! a crystal deposit (`Map::hold_site`, else the open spot nearest the
//! middle). A side with tanks in the ring and no other side's holds it and
//! banks time; two or more sides in it contest it and nobody scores. First side to
//! `TARGET` seconds held ends the round, otherwise the clock does.
//!
//! Each tank's own time in the ring while its side holds is kept too: it
//! orders the scoreboard and pays the player's HOLD line.

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::map::Map;
use crate::render::mesh::{Instance, MeshId};
use crate::sim::Tank;

/// Ring radius, m.
pub const RADIUS: f32 = 13.0;
/// Seconds held by one side that win the round outright.
pub const TARGET: f32 = 120.0;
/// The player earns `HOLD_RENOWN` for every `RENOWN_EVERY` seconds of their own on the site.
pub const RENOWN_EVERY: f32 = 10.0;
pub const HOLD_RENOWN: u32 = 5;
/// Tanks further than this above or below the ring's ground (a roof, a ledge) don't count.
const REACH_Y: f32 = 4.0;
/// Seconds a new state of the site must last before it's announced, so a
/// tank on the ring's edge doesn't make it chatter.
const SETTLE: f32 = 0.3;
const DASHES: usize = 40;
const POSTS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// Nobody in the ring.
    Open,
    /// Only this side's tanks are in it.
    Held(usize),
    /// More than one side is in it.
    Contested,
}

/// A change at the site the player hears, from side 0's point of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    /// Your side took the site.
    Taken,
    /// Your side's hold is gone, or another side took the site.
    Lost,
    /// Two or more sides are in the ring.
    Contested,
    /// You were paid for your time on the site.
    Paid,
}

pub struct Hold {
    pub at: Vec2,
    pub ground: f32,
    /// Seconds each side has held the site this round.
    pub held: Vec<f32>,
    /// Seconds each tank has spent on the site while its side held it.
    pub own: Vec<f32>,
    pub control: Control,
    /// Ground height under each dash of the ring.
    dash_y: Vec<f32>,
    /// The state last announced, and how long `control` has stood unchanged.
    heard: Control,
    steady: f32,
}

impl Hold {
    pub fn new(map: &Map, tanks: usize) -> Hold {
        let sides = map.factions.len();
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
            held: vec![0.0; sides],
            own: vec![0.0; tanks],
            control: Control::Open,
            dash_y,
            heard: Control::Open,
            steady: 0.0,
        }
    }

    /// Is a tank at `p` inside the ring?
    pub fn inside(&self, p: Vec3) -> bool {
        Vec2::new(p.x, p.z).distance(self.at) < RADIUS && (p.y - self.ground).abs() < REACH_Y
    }

    /// Advance by `dt`. `side(i)` is tank `i`'s side, or `None` if it doesn't
    /// count (a wreck). Returns the side holding, if any.
    pub fn step(
        &mut self,
        dt: f32,
        tanks: &[Tank],
        side: impl Fn(usize) -> Option<usize>,
    ) -> Option<usize> {
        self.own.resize(tanks.len(), 0.0);
        let on: Vec<(usize, usize)> = (0..tanks.len())
            .filter_map(|i| side(i).map(|s| (i, s)))
            .filter(|&(i, _)| self.inside(tanks[i].pos))
            .collect();
        let mut present: Vec<usize> = on.iter().map(|&(_, s)| s).collect();
        present.sort_unstable();
        present.dedup();
        let control = match present[..] {
            [] => Control::Open,
            [s] => Control::Held(s),
            _ => Control::Contested,
        };
        self.steady = if control == self.control {
            self.steady + dt
        } else {
            0.0
        };
        self.control = control;
        let Control::Held(s) = self.control else {
            return None;
        };
        self.held[s] = (self.held[s] + dt).min(TARGET);
        for &(i, _) in &on {
            self.own[i] += dt;
        }
        Some(s)
    }

    /// What the player should hear about the site now, if anything: once a
    /// new state has lasted `SETTLE`, the cue for the change. Call once a step.
    pub fn cue(&mut self) -> Option<Cue> {
        if self.steady < SETTLE || self.control == self.heard {
            return None;
        }
        let was = std::mem::replace(&mut self.heard, self.control);
        match (was, self.control) {
            (_, Control::Held(0)) => Some(Cue::Taken),
            (_, Control::Contested) => Some(Cue::Contested),
            (_, Control::Held(_)) | (Control::Held(0), Control::Open) => Some(Cue::Lost),
            _ => None,
        }
    }

    /// The side that has reached `TARGET`.
    pub fn winner(&self) -> Option<usize> {
        (0..self.held.len()).find(|&s| self.held[s] >= TARGET)
    }

    /// The ring painted on the ground and its four marker posts, tinted by
    /// who has it: plain paint when open, the holding side's paint,
    /// flickering orange when contested.
    pub fn instances(&self, time: f32, paint: &[Vec3], out: &mut Vec<(MeshId, Instance)>) {
        let plain = Vec3::new(0.86, 0.82, 0.72);
        let (color, glow) = match self.control {
            Control::Open => (plain, 0.0),
            Control::Held(s) => (paint[s], 0.9),
            Control::Contested => (
                Vec3::new(1.0, 0.55, 0.25),
                if (time * 5.0).fract() < 0.5 { 1.2 } else { 0.3 },
            ),
        };
        let dash_len = std::f32::consts::TAU * RADIUS / DASHES as f32 * 0.55;
        for (i, &y) in self.dash_y.iter().enumerate() {
            let f = i as f32 / DASHES as f32;
            let p = self.at + ring_dir(f) * RADIUS;
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
        let mut w = World::new(Map::yard(200.0), 0);
        for _ in 1..n {
            w.spawn_tank(
                Vec3::ONE,
                Vec2::ZERO,
                0.0,
                crate::sim::HullKind::Kataphrakt,
                crate::sim::TurretKind::Kontos,
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
    fn any_two_sides_on_the_site_contest_it() {
        let (mut w, mut h) = world_with(3);
        h.held = vec![0.0; 4];
        for i in 0..3 {
            put(&mut w, &h, i, Vec2::new(i as f32, 0.0));
        }
        // Sides 2 alone, then 2 and 3: held, then contested.
        let side = |only: bool| {
            move |i: usize| match i {
                0 => Some(2),
                _ if only => None,
                _ => Some(3),
            }
        };
        assert_eq!(h.step(0.5, &w.tanks, side(true)), Some(2));
        assert_eq!(h.held[2], 0.5);
        assert_eq!(h.step(0.5, &w.tanks, side(false)), None);
        assert_eq!(h.control, Control::Contested);
    }
    /// Tanks 0 and 1 on side 0, the rest on side 1.
    fn sides(i: usize) -> Option<usize> {
        Some(if i < 2 { 0 } else { 1 })
    }

    #[test]
    fn one_side_alone_holds_and_banks_time() {
        let (mut w, mut h) = world_with(3);
        put(&mut w, &h, 0, Vec2::new(2.0, 0.0));
        put(&mut w, &h, 1, Vec2::new(-4.0, 3.0));
        put(&mut w, &h, 2, Vec2::new(RADIUS + 10.0, 0.0));
        for _ in 0..60 {
            assert_eq!(h.step(DT, &w.tanks, sides), Some(0));
        }
        assert_eq!(h.control, Control::Held(0));
        assert!((h.held[0] - 60.0 * DT).abs() < 1e-4);
        assert_eq!(h.held[1], 0.0);
        // Both allies on the site get the time.
        assert!((h.own[0] - 60.0 * DT).abs() < 1e-4 && h.own[0] == h.own[1]);
        assert_eq!(h.own[2], 0.0);
    }

    #[test]
    fn both_sides_in_the_ring_contest_it() {
        let (mut w, mut h) = world_with(3);
        put(&mut w, &h, 0, Vec2::new(2.0, 0.0));
        put(&mut w, &h, 2, Vec2::new(-3.0, 1.0));
        put(&mut w, &h, 1, Vec2::new(RADIUS + 10.0, 0.0));
        assert_eq!(h.step(DT, &w.tanks, sides), None);
        assert_eq!(h.control, Control::Contested);
        assert_eq!(h.held, [0.0, 0.0]);
        // An enemy wreck in the ring doesn't count.
        assert_eq!(
            h.step(DT, &w.tanks, |i| if i == 2 { None } else { sides(i) }),
            Some(0)
        );
    }

    /// Steps for `secs` with tank `i` on side `side_of(i)`, collecting cues.
    fn run(
        h: &mut Hold,
        w: &World,
        secs: f32,
        side_of: impl Fn(usize) -> Option<usize> + Copy,
    ) -> Vec<Cue> {
        (0..(secs / DT).round() as usize)
            .filter_map(|_| {
                h.step(DT, &w.tanks, side_of);
                h.cue()
            })
            .collect()
    }

    #[test]
    fn taking_contesting_and_losing_the_site_are_each_heard_once() {
        let (mut w, mut h) = world_with(3);
        let out = Vec2::new(RADIUS + 10.0, 0.0);
        put(&mut w, &h, 0, Vec2::ZERO);
        put(&mut w, &h, 1, out);
        put(&mut w, &h, 2, out);
        assert_eq!(run(&mut h, &w, 2.0, sides), [Cue::Taken]);
        put(&mut w, &h, 2, Vec2::new(3.0, 0.0));
        assert_eq!(run(&mut h, &w, 2.0, sides), [Cue::Contested]);
        put(&mut w, &h, 0, out);
        assert_eq!(run(&mut h, &w, 2.0, sides), [Cue::Lost]);
        // The enemy leaving an empty site says nothing; your side walking off yours is a loss.
        put(&mut w, &h, 2, out);
        assert_eq!(run(&mut h, &w, 2.0, sides), []);
        put(&mut w, &h, 0, Vec2::ZERO);
        assert_eq!(run(&mut h, &w, 2.0, sides), [Cue::Taken]);
        put(&mut w, &h, 0, out);
        assert_eq!(run(&mut h, &w, 2.0, sides), [Cue::Lost]);
    }

    /// A tank flickering across the ring's edge every few steps stays quiet.
    #[test]
    fn a_flickering_edge_doesnt_chatter() {
        let (mut w, mut h) = world_with(3);
        put(&mut w, &h, 1, Vec2::new(RADIUS + 10.0, 0.0));
        put(&mut w, &h, 2, Vec2::new(RADIUS + 10.0, 5.0));
        let mut cues = Vec::new();
        for k in 0..120 {
            let r = if (k / 4) % 2 == 0 {
                RADIUS - 0.2
            } else {
                RADIUS + 0.2
            };
            put(&mut w, &h, 0, Vec2::new(r, 0.0));
            h.step(DT, &w.tanks, sides);
            cues.extend(h.cue());
        }
        assert!(cues.is_empty(), "{cues:?}");
    }

    #[test]
    fn tanks_overhead_dont_count() {
        let (mut w, mut h) = world_with(1);
        put(&mut w, &h, 0, Vec2::ZERO);
        w.tanks[0].pos.y += 8.0;
        assert_eq!(h.step(DT, &w.tanks, sides), None);
        assert_eq!(h.control, Control::Open);
    }

    #[test]
    fn reaching_the_target_wins() {
        let (mut w, mut h) = world_with(3);
        put(&mut w, &h, 2, Vec2::ZERO);
        put(&mut w, &h, 0, Vec2::new(RADIUS + 10.0, 0.0));
        put(&mut w, &h, 1, Vec2::new(RADIUS + 10.0, 5.0));
        h.held[1] = TARGET - DT * 0.5;
        assert_eq!(h.winner(), None);
        h.step(DT, &w.tanks, sides);
        assert_eq!(h.winner(), Some(1));
        assert_eq!(h.held[1], TARGET);
    }

    #[test]
    fn sanctum_site_is_the_altar_with_room_to_drive_in_the_ring() {
        let m = Map::sanctum();
        let h = Hold::new(&m, 1);
        assert_eq!(Some(h.at), m.hold_site);
        let nav = crate::ai::NavGrid::build(&m);
        // The altar and the crossing's piers stand in it; most of the ring is open.
        let open = (0..32)
            .filter(|&i| !nav.is_blocked(h.at + ring_dir(i as f32 / 32.0) * RADIUS * 0.7))
            .count();
        assert!(open >= 24, "only {open}/32 points in the ring are drivable");
    }
}
