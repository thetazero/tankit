//! Raid contract (one-sided capture the flag, attack and defend), on a
//! two-sided map. Each side has a stand at its base (`Map::raid_stands`).
//! One side defends: its flag stands on its stand. The other side
//! attacks: one of its tanks drives over the flag to pick it up and carries
//! it home to its own stand, which is the capture point. A carrier that's
//! destroyed drops the flag where it went down; an attacker picks it up again
//! by driving over it, a defender sends it home by touching it, and left
//! alone it goes home by itself after `RETURN_TIME`.
//!
//! A match is one round on the standard five-minute clock, and the sides
//! don't swap: the player's side attacks, the other defends. The attackers
//! win by capturing, which ends the match there and then; the defenders win
//! by holding out until the clock runs out.
//!
//! The carrier drives at `CARRIER_THROTTLE` of full throttle, can still
//! fire, and is marked to everyone, through walls, at any range.

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::map::Map;
use crate::render::mesh::{Instance, MeshId};
use crate::sim::Tank;

/// The side that attacks: the player's.
pub const ATTACKERS: usize = 0;
/// Driving this close to the flag picks it up (or sends it home), m.
pub const TOUCH: f32 = 3.5;
/// The capture point's ring: the carrier inside it captures, m.
pub const CAPTURE_RADIUS: f32 = 7.0;
/// A dropped flag left alone this long goes home, s.
pub const RETURN_TIME: f32 = 20.0;
/// What the carrier's throttle is scaled by.
pub const CARRIER_THROTTLE: f32 = 0.85;
/// Renown the player earns for capturing, and for sending a dropped flag home.
pub const CAPTURE_RENOWN: u32 = 30;
pub const RETURN_RENOWN: u32 = 10;
/// Tanks further than this above or below the flag or the ring don't touch it.
const REACH_Y: f32 = 4.0;
const DASHES: usize = 36;
/// The flag's pole and cloth.
const POLE: f32 = 4.2;
const CLOTH: Vec2 = Vec2::new(2.2, 1.3);
const STRIPS: usize = 10;

/// Where the flag is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flag {
    /// On the defenders' stand.
    Home,
    /// On tank `i`.
    Carried(usize),
    /// On the ground where its carrier went down, for `for_` seconds.
    Dropped { at: Vec3, for_: f32 },
}

/// Something that happened to the flag, for the player's ears, the feed and pay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Tank `by` picked the flag up.
    Taken { by: usize },
    /// Its carrier went down and dropped it.
    Dropped { by: usize },
    /// Tank `by` touched it and sent it home; `None`: it went home by itself.
    Returned { by: Option<usize> },
    /// Tank `by` carried it to the capture point.
    Captured { by: usize },
    /// The clock ran out on the attackers.
    HeldOut,
}

/// How the raid ended.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    /// The attackers captured, this many seconds in.
    Captured(f32),
    /// The clock ran out on them.
    HeldOut,
}

pub struct Raid {
    /// Each side's stand, on the ground: the defenders' is their flag's
    /// home, the attackers' their capture point.
    pub stands: Vec<Vec3>,
    pub flag: Flag,
    /// How it ended; `None` while it's on.
    pub outcome: Option<Outcome>,
    /// Seconds into the round.
    pub clock: f32,
    /// Flag events since the match last took them.
    pub events: Vec<Event>,
    /// Ground height under each dash of each stand's ring.
    dash_y: Vec<Vec<f32>>,
}

impl Raid {
    pub fn new(map: &Map) -> Raid {
        assert_eq!(map.factions.len(), 2, "a raid takes two sides");
        let stands: Vec<Vec3> = (0..2)
            .map(|s| {
                let at = map.raid_stands.get(s).copied().unwrap_or_else(|| {
                    // No stand set: the open spot nearest the middle of the
                    // side's base.
                    let base = &map.bases[s];
                    let c = if base.is_empty() {
                        map.center()
                    } else {
                        base.iter().sum::<Vec2>() / base.len() as f32
                    };
                    map.open_spots
                        .iter()
                        .copied()
                        .min_by(|a, b| a.distance_squared(c).total_cmp(&b.distance_squared(c)))
                        .unwrap_or(c)
                });
                Vec3::new(at.x, map.ground_height(at.x, at.y), at.y)
            })
            .collect();
        let dash_y = stands
            .iter()
            .map(|s| {
                (0..DASHES)
                    .map(|i| {
                        let p = flat(*s) + ring_dir(i as f32 / DASHES as f32) * CAPTURE_RADIUS;
                        map.ground_height(p.x, p.y)
                    })
                    .collect()
            })
            .collect();
        Raid {
            stands,
            flag: Flag::Home,
            outcome: None,
            clock: 0.0,
            events: Vec::new(),
            dash_y,
        }
    }

    /// The side attacking: the player's, the whole match.
    pub fn attackers(&self) -> usize {
        ATTACKERS
    }

    pub fn defenders(&self) -> usize {
        1 - self.attackers()
    }

    /// Where the flag is now (on its carrier: `tanks[i]`'s position).
    pub fn flag_at(&self, tanks: &[Tank]) -> Vec3 {
        match self.flag {
            Flag::Home => self.stands[self.defenders()],
            Flag::Carried(i) => tanks[i].pos,
            Flag::Dropped { at, .. } => at,
        }
    }

    /// The capture point: the attackers' own stand.
    pub fn capture_point(&self) -> Vec3 {
        self.stands[self.attackers()]
    }

    pub fn carrier(&self) -> Option<usize> {
        match self.flag {
            Flag::Carried(i) => Some(i),
            _ => None,
        }
    }

    /// The raid is over: captured, or held.
    pub fn done(&self) -> bool {
        self.outcome.is_some()
    }

    /// The side that won: the attackers if they captured, the defenders if
    /// they held out; `None` while it's on.
    pub fn winner(&self) -> Option<usize> {
        self.outcome.map(|o| match o {
            Outcome::Captured(_) => self.attackers(),
            Outcome::HeldOut => self.defenders(),
        })
    }

    /// Advance the running round by `dt`. `side(i)` is tank `i`'s side, or
    /// `None` if it can't touch the flag (a wreck, a husk). Returns true when
    /// the round has just ended (captured, or the clock ran out at `time_left`).
    pub fn step(
        &mut self,
        dt: f32,
        time_left: f32,
        tanks: &[Tank],
        side: impl Fn(usize) -> Option<usize>,
    ) -> bool {
        if self.done() {
            return false;
        }
        self.clock += dt;
        let (att, def) = (self.attackers(), self.defenders());
        let touching = |i: usize, at: Vec3| {
            let p = tanks[i].pos;
            flat(p).distance(flat(at)) < TOUCH && (p.y - at.y).abs() < REACH_Y
        };
        // Nearest first, so the tank that got there first (about) gets it.
        let mut near: Vec<usize> = (0..tanks.len()).filter(|&i| side(i).is_some()).collect();
        let at = self.flag_at(tanks);
        near.sort_by(|&a, &b| {
            let d = |i: usize| tanks[i].pos.distance_squared(at);
            d(a).total_cmp(&d(b))
        });
        match self.flag {
            Flag::Carried(i) => {
                if side(i) != Some(att) {
                    let ground = tanks[i].pos.y - 0.4;
                    self.flag = Flag::Dropped {
                        at: Vec3::new(tanks[i].pos.x, ground, tanks[i].pos.z),
                        for_: 0.0,
                    };
                    self.events.push(Event::Dropped { by: i });
                } else {
                    let c = self.capture_point();
                    let p = tanks[i].pos;
                    if flat(p).distance(flat(c)) < CAPTURE_RADIUS && (p.y - c.y).abs() < REACH_Y {
                        self.events.push(Event::Captured { by: i });
                        self.outcome = Some(Outcome::Captured(self.clock));
                        return true;
                    }
                }
            }
            Flag::Home => {
                if let Some(&i) = near
                    .iter()
                    .find(|&&i| side(i) == Some(att) && touching(i, at))
                {
                    self.flag = Flag::Carried(i);
                    self.events.push(Event::Taken { by: i });
                }
            }
            Flag::Dropped { at, for_ } => {
                let first = near.iter().copied().find(|&i| touching(i, at));
                match first.map(|i| (i, side(i))) {
                    Some((i, Some(s))) if s == att => {
                        self.flag = Flag::Carried(i);
                        self.events.push(Event::Taken { by: i });
                    }
                    Some((i, Some(s))) if s == def => {
                        self.flag = Flag::Home;
                        self.events.push(Event::Returned { by: Some(i) });
                    }
                    _ if for_ + dt >= RETURN_TIME => {
                        self.flag = Flag::Home;
                        self.events.push(Event::Returned { by: None });
                    }
                    _ => {
                        self.flag = Flag::Dropped {
                            at,
                            for_: for_ + dt,
                        }
                    }
                }
            }
        }
        if time_left <= 0.0 {
            self.events.push(Event::HeldOut);
            self.outcome = Some(Outcome::HeldOut);
            return true;
        }
        false
    }

    /// Both stands, and the flag on its stand, on its carrier (`shown`, the
    /// tanks as drawn), or on the ground. The defenders' stand is a plain
    /// plinth with the flag on it; the attackers' is ringed in their paint,
    /// glowing brighter while one of theirs has the flag.
    pub fn instances(
        &self,
        time: f32,
        paint: &[Vec3],
        shown: impl Fn(usize) -> Tank,
        out: &mut Vec<(MeshId, Instance)>,
    ) {
        let (att, def) = (self.attackers(), self.defenders());
        let stone = Vec4::new(0.28, 0.27, 0.26, 0.0);
        for (s, &at) in self.stands.iter().enumerate() {
            // A low plinth.
            out.push((
                MeshId::CYLINDER,
                Instance::new(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(2.6, 0.5, 2.6),
                        Quat::IDENTITY,
                        at + Vec3::Y * 0.15,
                    ),
                    stone,
                ),
            ));
            out.push((
                MeshId::CYLINDER,
                Instance::new(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(2.2, 0.08, 2.2),
                        Quat::IDENTITY,
                        at + Vec3::Y * 0.42,
                    ),
                    paint[s].extend(0.5),
                ),
            ));
            if s != att || self.done() {
                continue;
            }
            // The capture ring, in the attackers' paint.
            let glow = if self.carrier().is_some() {
                0.6 + 0.6 * (time * 4.0).sin().abs()
            } else {
                0.35
            };
            let dash_len = std::f32::consts::TAU * CAPTURE_RADIUS / DASHES as f32 * 0.55;
            for (i, &y) in self.dash_y[s].iter().enumerate() {
                let f = i as f32 / DASHES as f32;
                let p = flat(at) + ring_dir(f) * CAPTURE_RADIUS;
                out.push((
                    MeshId::CUBE,
                    Instance::new(
                        Mat4::from_scale_rotation_translation(
                            Vec3::new(0.4, 0.06, dash_len),
                            Quat::from_rotation_y(-f * std::f32::consts::TAU),
                            Vec3::new(p.x, y + 0.03, p.y),
                        ),
                        paint[s].extend(glow),
                    ),
                ));
            }
            // A beacon of light standing over it.
            out.push((
                MeshId::CYLINDER,
                Instance::new(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(0.5, 14.0, 0.5),
                        Quat::IDENTITY,
                        at + Vec3::Y * 7.4,
                    ),
                    paint[s].extend(1.2 * glow),
                ),
            ));
        }
        if self.done() {
            return;
        }
        // The flag: the defenders' colours.
        let (base, lean) = match self.flag {
            Flag::Home => (self.stands[def] + Vec3::Y * 0.46, Quat::IDENTITY),
            Flag::Dropped { at, .. } => (at, Quat::from_rotation_z(0.5)),
            Flag::Carried(i) => {
                let t = shown(i);
                // Planted at the back corner of the hull, the cloth
                // streaming back.
                let c = &t.chassis;
                let back =
                    t.pos + t.rot * Vec3::new(c.hull_half_x * 0.6, c.hull_top, c.hull_z.0 * 0.75);
                (
                    back,
                    t.rot * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                )
            }
        };
        flag(base, lean, time, paint[def], out);
    }
}

/// A flag on its pole, standing from `base`, turned by `lean`, the cloth
/// rippling in strips.
fn flag(base: Vec3, lean: Quat, time: f32, color: Vec3, out: &mut Vec<(MeshId, Instance)>) {
    let pole = Vec4::new(0.18, 0.17, 0.16, 0.0);
    out.push((
        MeshId::CYLINDER,
        Instance::new(
            Mat4::from_scale_rotation_translation(
                Vec3::new(0.12, POLE, 0.12),
                lean,
                base + lean * Vec3::Y * (POLE * 0.5),
            ),
            pole,
        ),
    ));
    out.push((
        MeshId::SPHERE,
        Instance::new(
            Mat4::from_scale_rotation_translation(
                Vec3::splat(0.24),
                Quat::IDENTITY,
                base + lean * Vec3::Y * POLE,
            ),
            Vec4::new(0.85, 0.75, 0.45, 0.3),
        ),
    ));
    // The cloth's sideways ripple at `u` (0 at the pole, 1 at the free
    // end): a wave running out along it, growing toward the free end.
    let ripple = |u: f32| (time * 5.0 - u * 6.0).sin() * 0.3 * u;
    for k in 0..STRIPS {
        // Each strip runs between two points on the ripple, so they join up.
        let (u0, u1) = (k as f32 / STRIPS as f32, (k + 1) as f32 / STRIPS as f32);
        let a = Vec2::new(u0 * CLOTH.x, ripple(u0));
        let b = Vec2::new(u1 * CLOTH.x, ripple(u1));
        let d = b - a;
        let mid = (a + b) * 0.5;
        let local = Vec3::new(mid.x, POLE - CLOTH.y * 0.5 - 0.1, mid.y);
        // Lit as it faces the light and turns away.
        let shade = 0.82 + 0.18 * (time * 5.0 - (u0 + u1) * 3.0).cos();
        out.push((
            MeshId::CUBE,
            Instance::new(
                Mat4::from_scale_rotation_translation(
                    Vec3::new(d.length() + 0.04, CLOTH.y, 0.06),
                    lean * Quat::from_rotation_y(-d.y.atan2(d.x)),
                    base + lean * local,
                ),
                (color * shade).extend(0.25),
            ),
        ));
    }
}

fn flat(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
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

    /// Tanks 0 and 1 on side 0, 2 and 3 on side 1.
    fn sides(i: usize) -> Option<usize> {
        Some(if i < 2 { 0 } else { 1 })
    }

    fn setup() -> (World, Raid) {
        let mut map = Map::yard(200.0);
        map.factions = vec![
            crate::faction::Faction::Imperium,
            crate::faction::Faction::Resonant,
        ];
        map.raid_stands = vec![Vec2::new(40.0, 100.0), Vec2::new(160.0, 100.0)];
        let mut w = World::new(map, 0);
        for _ in 1..4 {
            w.spawn_tank(
                Vec3::ONE,
                Vec2::new(100.0, 20.0),
                0.0,
                crate::sim::HullKind::Kataphrakt,
                crate::sim::TurretKind::Kontos,
            );
        }
        let r = Raid::new(&w.map);
        for i in 0..4 {
            put(&mut w, i, Vec2::new(100.0, 20.0 + i as f32 * 10.0));
        }
        (w, r)
    }

    fn put(w: &mut World, i: usize, p: Vec2) {
        w.tanks[i].pos = Vec3::new(p.x, w.map.ground_height(p.x, p.y) + 0.5, p.y);
    }

    fn step(w: &World, r: &mut Raid) -> bool {
        r.step(DT, 100.0, &w.tanks, sides)
    }

    #[test]
    fn an_attacker_takes_the_flag_home_and_captures() {
        let (mut w, mut r) = setup();
        // A defender on the flag does nothing.
        put(&mut w, 2, Vec2::new(160.0, 101.0));
        step(&w, &mut r);
        assert_eq!(r.flag, Flag::Home);
        put(&mut w, 0, Vec2::new(161.0, 100.0));
        step(&w, &mut r);
        assert_eq!(r.flag, Flag::Carried(0));
        assert_eq!(r.events, [Event::Taken { by: 0 }]);
        // Carried along, then into the ring at home.
        put(&mut w, 0, Vec2::new(100.0, 100.0));
        assert!(!step(&w, &mut r));
        assert_eq!(r.flag_at(&w.tanks), w.tanks[0].pos);
        put(&mut w, 0, Vec2::new(40.0 + CAPTURE_RADIUS - 1.0, 100.0));
        assert!(step(&w, &mut r));
        assert!(matches!(r.outcome, Some(Outcome::Captured(_))));
        assert_eq!(r.winner(), Some(0));
        // That's the raid over: the sides don't swap for another go.
        assert!(!step(&w, &mut r));
        assert_eq!((r.attackers(), r.defenders()), (0, 1));
    }

    #[test]
    fn a_dropped_flag_is_retaken_returned_or_goes_home_alone() {
        let (mut w, mut r) = setup();
        put(&mut w, 0, Vec2::new(160.0, 100.0));
        step(&w, &mut r);
        put(&mut w, 0, Vec2::new(120.0, 100.0));
        // The carrier is wrecked.
        r.step(
            DT,
            100.0,
            &w.tanks,
            |i| if i == 0 { None } else { sides(i) },
        );
        assert!(matches!(r.flag, Flag::Dropped { .. }));
        // Its teammate picks it up (the wreck gone, respawned at home).
        put(&mut w, 0, Vec2::new(40.0, 60.0));
        put(&mut w, 1, Vec2::new(121.0, 100.0));
        step(&w, &mut r);
        assert_eq!(r.flag, Flag::Carried(1));
        r.step(
            DT,
            100.0,
            &w.tanks,
            |i| if i == 1 { None } else { sides(i) },
        );
        // A defender touches it: home.
        put(&mut w, 1, Vec2::new(100.0, 60.0));
        put(&mut w, 3, Vec2::new(121.0, 101.0));
        step(&w, &mut r);
        assert_eq!(r.flag, Flag::Home);
        assert_eq!(r.events.last(), Some(&Event::Returned { by: Some(3) }));
        // Dropped and left alone: home after RETURN_TIME.
        put(&mut w, 3, Vec2::new(100.0, 60.0));
        put(&mut w, 0, Vec2::new(160.0, 100.0));
        step(&w, &mut r);
        put(&mut w, 0, Vec2::new(100.0, 140.0));
        r.step(
            DT,
            100.0,
            &w.tanks,
            |i| if i == 0 { None } else { sides(i) },
        );
        for _ in 0..((RETURN_TIME / DT) as usize + 2) {
            r.step(
                DT,
                100.0,
                &w.tanks,
                |i| if i == 0 { None } else { sides(i) },
            );
        }
        assert_eq!(r.flag, Flag::Home);
        assert_eq!(r.events.last(), Some(&Event::Returned { by: None }));
    }

    #[test]
    fn the_clock_running_out_is_a_hold() {
        let (w, mut r) = setup();
        assert_eq!(r.winner(), None);
        assert!(r.step(DT, 0.0, &w.tanks, sides));
        assert_eq!(r.outcome, Some(Outcome::HeldOut));
        assert_eq!(r.events, [Event::HeldOut]);
        assert_eq!(r.winner(), Some(1), "the defenders held out");
    }

    /// On the Seam each side's stand is open, drivable ground near its base,
    /// and the stands are far apart.
    #[test]
    fn seam_stands_are_open_and_far_apart() {
        let m = Map::mine();
        let r = Raid::new(&m);
        let nav = crate::ai::NavGrid::build(&m);
        for (s, &at) in r.stands.iter().enumerate() {
            let base = m.bases[s].iter().sum::<Vec2>() / m.bases[s].len() as f32;
            assert!(
                flat(at).distance(base) < 45.0,
                "side {s}'s stand is far from its base"
            );
            let open = (0..24)
                .filter(|&i| !nav.is_blocked(flat(at) + ring_dir(i as f32 / 24.0) * 4.0))
                .count();
            assert!(
                open >= 22,
                "side {s}: only {open}/24 points round the stand are drivable"
            );
            assert!(!nav.is_blocked(flat(at)));
        }
        assert!(flat(r.stands[0]).distance(flat(r.stands[1])) > 110.0);
    }
}
