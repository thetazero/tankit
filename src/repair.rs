//! Repair drops: crates that float down on parachutes at fixed spots on each
//! map (`Map::repair_spots`), each spot on its own timer. Drive over a landed
//! crate to patch your hull; the spot then counts down to its next drop.

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::map::Map;
use crate::render::mesh::{Instance, MeshId};
use crate::sim::Tank;

/// Seconds into the round before the first spot drops.
pub const FIRST_DROP: f32 = 12.0;
/// Each further spot's first drop comes this much later than the last.
pub const STAGGER: f32 = 6.0;
/// Seconds from a pickup to that spot's next drop.
pub const COOLDOWN: f32 = 30.0;
/// Crates release this high above the ground.
pub const DROP_HEIGHT: f32 = 40.0;
/// Descent under the canopy, m/s.
pub const FALL_SPEED: f32 = 4.5;
/// Share of the hull's maximum health a crate restores.
pub const HEAL_FRACTION: f32 = 0.5;
/// A tank this close (horizontally) to a landed crate picks it up.
pub const PICKUP_RADIUS: f32 = 3.0;

const CRATE: f32 = 1.3;
/// Canopy center above the crate's base while falling.
const CHUTE_RISE: f32 = 4.6;
const COLLAPSE_TIME: f32 = 1.2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    /// Next crate drops in this many seconds.
    Waiting(f32),
    /// Crate's base is this high above the ground.
    Falling(f32),
    /// On the ground for this many seconds, ready to pick up.
    Landed(f32),
}

pub struct RepairDrop {
    pub at: Vec2,
    pub ground: f32,
    pub state: State,
    /// Seconds since this crate released (drives the sway).
    age: f32,
}

impl RepairDrop {
    /// The crate's base, if one is out.
    pub fn crate_pos(&self) -> Option<Vec3> {
        let h = match self.state {
            State::Waiting(_) => return None,
            State::Falling(h) => h,
            State::Landed(_) => 0.0,
        };
        Some(Vec3::new(self.at.x, self.ground + h, self.at.y))
    }
}

pub struct Repairs {
    pub drops: Vec<RepairDrop>,
}

impl Repairs {
    pub fn new(map: &Map) -> Repairs {
        let drops = map
            .repair_spots
            .iter()
            .enumerate()
            .map(|(i, &at)| RepairDrop {
                at,
                ground: map.ground_height(at.x, at.y),
                state: State::Waiting(FIRST_DROP + i as f32 * STAGGER),
                age: 0.0,
            })
            .collect();
        Repairs { drops }
    }

    /// Advance every spot by `dt`. A landed crate goes to the nearest tank on
    /// it for which `wants` is true; returns (tank, crate position) per pickup.
    pub fn step(
        &mut self,
        dt: f32,
        tanks: &[Tank],
        wants: impl Fn(usize) -> bool,
    ) -> Vec<(usize, Vec3)> {
        let mut picked = Vec::new();
        for d in &mut self.drops {
            d.age += dt;
            d.state = match d.state {
                State::Waiting(t) if t - dt <= 0.0 => {
                    d.age = 0.0;
                    State::Falling(DROP_HEIGHT)
                }
                State::Waiting(t) => State::Waiting(t - dt),
                State::Falling(h) if h - FALL_SPEED * dt <= 0.0 => State::Landed(0.0),
                State::Falling(h) => State::Falling(h - FALL_SPEED * dt),
                State::Landed(t) => State::Landed(t + dt),
            };
            let State::Landed(_) = d.state else { continue };
            let base = Vec3::new(d.at.x, d.ground, d.at.y);
            let taker = tanks
                .iter()
                .enumerate()
                .filter(|(i, t)| {
                    Vec2::new(t.pos.x, t.pos.z).distance(d.at) < PICKUP_RADIUS
                        && (t.pos.y - d.ground).abs() < 3.0
                        && wants(*i)
                })
                .min_by(|a, b| a.1.pos.distance(base).total_cmp(&b.1.pos.distance(base)));
            if let Some((i, _)) = taker {
                picked.push((i, base + Vec3::Y * CRATE * 0.5));
                d.state = State::Waiting(COOLDOWN);
            }
        }
        picked
    }

    /// Crates, parachutes and cords as render instances.
    pub fn instances(&self, time: f32, out: &mut Vec<(MeshId, Instance)>) {
        for d in &self.drops {
            let Some(base) = d.crate_pos() else { continue };
            let landed = match d.state {
                State::Landed(t) => Some(t),
                _ => None,
            };
            // Swing under the canopy like a pendulum while falling; settles in
            // the first seconds after release.
            let settle = 1.0 - (-d.age * 0.8).exp();
            let sway = if landed.is_some() {
                Quat::IDENTITY
            } else {
                let a = 0.12 * settle;
                Quat::from_rotation_x((d.age * 1.3).sin() * a)
                    * Quat::from_rotation_z((d.age * 1.1 + 1.0).sin() * a)
            };
            let pivot = base + Vec3::Y * CHUTE_RISE;
            let frame = Mat4::from_translation(pivot)
                * Mat4::from_quat(sway)
                * Mat4::from_translation(-Vec3::Y * CHUTE_RISE);
            let spin = Quat::from_rotation_y(d.at.x * 0.37 + d.at.y * 0.11);

            // The crate: olive box with a glowing green cross on the top and every side.
            let box_c = Vec3::Y * CRATE * 0.5;
            let crate_m =
                frame * Mat4::from_scale_rotation_translation(Vec3::splat(CRATE), spin, box_c);
            out.push((
                MeshId::CUBE,
                Instance::new(crate_m, Vec4::new(0.36, 0.4, 0.27, 0.0)),
            ));
            let pulse = if landed.is_some() {
                1.6 + 0.8 * (time * 4.0).sin()
            } else {
                1.2
            };
            let glow = Vec4::new(0.35, 1.0, 0.5, pulse);
            let faces = [
                Quat::IDENTITY,
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2),
            ];
            for f in faces {
                for bar in [Vec3::new(0.62, 0.04, 0.18), Vec3::new(0.18, 0.04, 0.62)] {
                    let q = spin * f;
                    let p = box_c + q * Vec3::Y * (CRATE * 0.5 + 0.01);
                    out.push((
                        MeshId::CUBE,
                        Instance::new(
                            frame * Mat4::from_scale_rotation_translation(bar * CRATE, q, p),
                            glow,
                        ),
                    ));
                }
            }

            // Canopy: full while falling; after landing it sags, slides off to
            // one side and settles flat, then is packed away.
            let k = landed.map_or(0.0, |t| (t / COLLAPSE_TIME).min(1.0));
            let gone = landed.map_or(1.0, |t| (1.0 - (t - 3.0) / 0.6).clamp(0.0, 1.0));
            if gone <= 0.0 {
                continue;
            }
            let ease = k * k * (3.0 - 2.0 * k);
            let side = spin * Vec3::X;
            let chute_c =
                Vec3::Y * (CHUTE_RISE - 0.6 + (0.05 - CHUTE_RISE + 0.6) * ease) + side * 2.2 * ease;
            let chute_s = Vec3::new(4.6 - 1.0 * ease, 2.3 - 2.1 * ease, 4.6 - 1.0 * ease) * gone;
            let chute_m = frame * Mat4::from_scale_rotation_translation(chute_s, spin, chute_c);
            out.push((
                MeshId::CANOPY,
                Instance::new(chute_m, Vec4::new(1.0, 1.0, 1.0, 0.0)),
            ));

            // Cords from the crate's top corners to the canopy rim, while it's up.
            if landed.is_none() {
                for (sx, sz) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                    let a = box_c + spin * Vec3::new(sx * 0.5, 0.5, sz * 0.5) * CRATE;
                    let b = chute_c + spin * Vec3::new(sx, 0.0, sz).normalize() * chute_s.x * 0.48;
                    let (a, b) = (frame.transform_point3(a), frame.transform_point3(b));
                    let dir = b - a;
                    let q = Quat::from_rotation_arc(Vec3::Y, dir.normalize());
                    let m = Mat4::from_scale_rotation_translation(
                        Vec3::new(0.04, dir.length(), 0.04),
                        q,
                        (a + b) * 0.5,
                    );
                    out.push((
                        MeshId::CUBE,
                        Instance::new(m, Vec4::new(0.85, 0.82, 0.74, 0.0)),
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn landed_spot(r: &mut Repairs) {
        r.drops[0].state = State::Falling(0.01);
        r.step(0.1, &[], |_| true);
        assert!(matches!(r.drops[0].state, State::Landed(_)));
    }

    #[test]
    fn crates_drop_on_the_timer_and_land() {
        let map = Map::yard(200.0);
        let mut r = Repairs::new(&map);
        let dt = 0.1;
        let steps = |s: f32| (s / dt).round() as usize;
        for _ in 0..steps(FIRST_DROP - 0.5) {
            r.step(dt, &[], |_| true);
        }
        assert!(matches!(r.drops[0].state, State::Waiting(_)));
        for _ in 0..steps(1.0) {
            r.step(dt, &[], |_| true);
        }
        assert!(matches!(r.drops[0].state, State::Falling(h) if h > DROP_HEIGHT - 5.0));
        // The second spot is staggered.
        assert!(matches!(r.drops[1].state, State::Waiting(_)));
        for _ in 0..steps(DROP_HEIGHT / FALL_SPEED + 0.5) {
            r.step(dt, &[], |_| true);
        }
        assert!(matches!(r.drops[0].state, State::Landed(_)));
    }

    #[test]
    fn landed_crate_stays_until_wanted() {
        let world = crate::sim::World::new(Map::yard(200.0), 0);
        let mut r = Repairs::new(&world.map);
        let t = &world.tanks[0];
        r.drops[0].at = Vec2::new(t.pos.x, t.pos.z);
        r.drops[0].ground = world.map.ground_height(t.pos.x, t.pos.z);
        landed_spot(&mut r);
        assert!(r.step(0.1, &world.tanks, |_| false).is_empty());
        assert!(matches!(r.drops[0].state, State::Landed(_)));
        let picked = r.step(0.1, &world.tanks, |_| true);
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].0, 0);
        assert_eq!(r.drops[0].state, State::Waiting(COOLDOWN));
    }
}
