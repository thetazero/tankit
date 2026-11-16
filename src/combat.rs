//! Deathmatch: every tank for itself, five-minute rounds, most kills wins.
//!
//! Wraps the physics `World` with what a fight needs: shells in flight, hull
//! health, kills and deaths, respawns, the round clock, and the bots. Tank 0
//! is the player; the rest are bots.

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use rapier3d::prelude::{QueryFilter, Ray};

use crate::ai::{self, Bot, Contact, NavGrid};
use crate::game::DT;
use crate::map::Map;
use crate::render::mesh::{Instance, MeshId};
use crate::repair::Repairs;
use crate::rng::Rng;
use crate::sim::{Command, GRAVITY, HullKind, Tank, TurretKind, World, wrap_angle, yaw_of};

pub const ROUND_TIME: f32 = 300.0;
pub const RESPAWN_TIME: f32 = 4.0;
/// Freshly spawned tanks can't be hurt (and bots ignore them) for this long.
pub const SPAWN_SHIELD: f32 = 2.0;
pub const SHELL_SPEED: f32 = 160.0;
const SHELL_LIFE: f32 = 1.6;
/// Vertical aim assist: a target this close (horizontally, radians) to the gun line gets the elevation.
const ASSIST_CONE: f32 = 0.09;
const ASSIST_RANGE: f32 = 150.0;
pub const FEED_TIME: f32 = 6.0;
/// XP for each enemy the player kills, paid the moment it dies.
pub const KILL_XP: u32 = 10;
/// XP bonus for finishing the round on top.
pub const WIN_XP: u32 = 25;
/// How long an XP popup stays on screen, seconds.
pub const XP_POP_TIME: f32 = 2.0;

pub const BOT_NAMES: [&str; 7] = ["VEGA", "KITE", "ONYX", "RUST", "MOTH", "FLINT", "SABLE"];
const BOT_COLORS: [Vec3; 7] = [
    Vec3::new(0.95, 0.42, 0.32),
    Vec3::new(0.4, 0.62, 0.95),
    Vec3::new(0.95, 0.8, 0.35),
    Vec3::new(0.5, 0.82, 0.5),
    Vec3::new(0.75, 0.5, 0.9),
    Vec3::new(0.95, 0.6, 0.2),
    Vec3::new(0.4, 0.85, 0.85),
];

/// Weapon numbers per turret.
pub struct Gun {
    pub damage: f32,
    /// Seconds between shots.
    pub reload: f32,
}

pub fn gun(kind: TurretKind) -> Gun {
    match kind {
        // Long rail cannon: hits hard, slow.
        TurretKind::Lance => Gun { damage: 38.0, reload: 1.4 },
        // Twin short guns: light hits, alternating barrels, fast.
        TurretKind::Maul => Gun { damage: 15.0, reload: 0.5 },
    }
}

pub fn max_health(kind: HullKind) -> f32 {
    match kind {
        HullKind::Lancer => 110.0,
        HullKind::Bastion => 160.0,
    }
}

pub struct Fighter {
    pub name: String,
    pub health: f32,
    pub alive: bool,
    /// Seconds until respawn while dead.
    pub respawn_in: f32,
    /// Seconds of spawn protection left.
    pub shield: f32,
    /// Seconds until the gun can fire again.
    pub reload_in: f32,
    pub kills: u32,
    pub deaths: u32,
    /// Who killed this tank last (for the death screen).
    pub killed_by: Option<usize>,
    /// Seconds since this tank last took a hit.
    pub since_hit: f32,
    /// Seconds since this tank last landed a hit on someone.
    pub since_dealt: f32,
    /// Kit to fit at the next respawn (picked in the garage).
    pub next_kit: Option<(HullKind, TurretKind)>,
    /// Seconds since this tank last picked up a repair crate, and how much it mended.
    pub since_repair: f32,
    pub repaired: f32,
}

impl Fighter {
    pub fn max_health(&self, t: &Tank) -> f32 {
        max_health(t.hull_kind)
    }
}

pub struct Shell {
    pub pos: Vec3,
    pub prev: Vec3,
    pub vel: Vec3,
    pub owner: usize,
    pub damage: f32,
    pub age: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub enum BlastKind {
    /// Shell hitting a wall or the ground.
    Impact,
    /// Shell hitting a tank.
    Hit,
    /// A tank blowing up.
    Kill,
    /// Smoke puff rising off a wreck.
    Smoke,
    /// Green flash where a repair crate was picked up.
    Repair,
}

pub struct Blast {
    pub pos: Vec3,
    pub kind: BlastKind,
    pub age: f32,
    /// Where it drifts over its life (smoke), m.
    pub drift: Vec3,
}

impl Blast {
    fn new(pos: Vec3, kind: BlastKind) -> Blast {
        Blast { pos, kind, age: 0.0, drift: Vec3::ZERO }
    }
}

impl Blast {
    fn life(&self) -> f32 {
        match self.kind {
            BlastKind::Impact => 0.3,
            BlastKind::Hit => 0.4,
            BlastKind::Kill => 1.1,
            BlastKind::Smoke => 1.6,
            BlastKind::Repair => 0.7,
        }
    }
}

pub struct Kill {
    pub killer: usize,
    pub victim: usize,
    pub age: f32,
}

/// XP the player just earned, for the HUD popup.
pub struct XpPop {
    pub amount: u32,
    pub reason: &'static str,
    pub age: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Playing,
    Over,
}

pub struct Match {
    pub world: World,
    pub fighters: Vec<Fighter>,
    bots: Vec<Option<Bot>>,
    pub shells: Vec<Shell>,
    pub blasts: Vec<Blast>,
    pub feed: Vec<Kill>,
    /// XP the player has earned this match (kills plus the win bonus).
    pub xp: u32,
    pub xp_pops: Vec<XpPop>,
    pub repairs: Repairs,
    pub time_left: f32,
    pub phase: Phase,
    nav: NavGrid,
    rng: Rng,
}

impl Match {
    /// A fresh round on `map`: the player at spawn point `spawn` plus `bots` bots.
    pub fn new(map: Map, spawn: usize, bots: usize, seed: u64) -> Match {
        let nav = NavGrid::build(&map);
        let repairs = Repairs::new(&map);
        let mut world = World::new(map, spawn);
        let mut rng = Rng::new(seed ^ 0xD3A7);
        let mut fighters = vec![fighter("YOU", max_health(world.tanks[0].hull_kind))];
        let mut brains = vec![None];
        for b in 0..bots.min(BOT_NAMES.len()) {
            let hull = if rng.chance(0.5) { HullKind::Lancer } else { HullKind::Bastion };
            let turret = if rng.chance(0.5) { TurretKind::Lance } else { TurretKind::Maul };
            let (at, yaw) = spawn_point(&world, &mut rng, None);
            world.spawn_tank(BOT_COLORS[b], at, yaw, hull, turret);
            fighters.push(fighter(BOT_NAMES[b], max_health(hull)));
            brains.push(Some(Bot::new(seed.wrapping_add(b as u64 * 7919))));
        }
        Match {
            world,
            fighters,
            bots: brains,
            shells: Vec::new(),
            blasts: Vec::new(),
            feed: Vec::new(),
            xp: 0,
            xp_pops: Vec::new(),
            repairs,
            time_left: ROUND_TIME,
            phase: Phase::Playing,
            nav,
            rng,
        }
    }

    pub fn player(&self) -> &Fighter {
        &self.fighters[0]
    }

    /// Fighter indices by standing: most kills, then fewest deaths.
    pub fn standings(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.fighters.len()).collect();
        order.sort_by_key(|&i| (std::cmp::Reverse(self.fighters[i].kills), self.fighters[i].deaths, i));
        order
    }

    /// Swap the player's hull, keeping health as a fraction of the new maximum.
    pub fn set_player_hull(&mut self, kind: HullKind) {
        let f = &mut self.fighters[0];
        let frac = f.health / max_health(self.world.tanks[0].hull_kind);
        self.world.set_hull(0, kind);
        f.health = frac * max_health(kind);
    }

    /// One fixed step. `player` drives tank 0; `fire` is the trigger held.
    pub fn step(&mut self, player: Command, fire: bool) {
        if self.phase == Phase::Playing {
            self.time_left -= DT;
            if self.time_left <= 0.0 {
                self.time_left = 0.0;
                self.phase = Phase::Over;
                self.shells.clear();
                if self.standings()[0] == 0 {
                    self.award(WIN_XP, "ROUND WON");
                }
            }
        }
        let playing = self.phase == Phase::Playing;

        let contacts: Vec<Contact> =
            self.fighters.iter().map(|f| Contact { targetable: f.alive && f.shield <= 0.0 }).collect();
        let mut cmds = Vec::with_capacity(self.fighters.len());
        let mut triggers = Vec::with_capacity(self.fighters.len());
        for i in 0..self.fighters.len() {
            let (c, f) = match &mut self.bots[i] {
                _ if !self.fighters[i].alive || !playing => (Command::default(), false),
                Some(bot) => bot.think(i, &self.world, &contacts, &self.nav, DT),
                None => (player, fire),
            };
            cmds.push(c);
            triggers.push(f);
        }

        self.world.step(&cmds);
        self.aim_assist();

        for (i, &trigger) in triggers.iter().enumerate() {
            let f = &mut self.fighters[i];
            f.reload_in -= DT;
            f.shield -= DT;
            f.since_hit += DT;
            f.since_dealt += DT;
            f.since_repair += DT;
            if trigger && f.alive && f.reload_in <= 0.0 {
                self.shoot(i);
            }
        }

        self.fly_shells();
        self.respawns();
        if playing {
            self.pick_up_repairs();
        }

        for b in &mut self.blasts {
            b.age += DT;
        }
        self.blasts.retain(|b| b.age < b.life());
        for k in &mut self.feed {
            k.age += DT;
        }
        self.feed.retain(|k| k.age < FEED_TIME);
        for x in &mut self.xp_pops {
            x.age += DT;
        }
        self.xp_pops.retain(|x| x.age < XP_POP_TIME);
    }

    /// Elevate each gun onto a tank roughly along its line of fire (Tanki-style
    /// vertical auto-aim), so hills and ramps don't make every shot miss.
    fn aim_assist(&mut self) {
        let n = self.world.tanks.len();
        for i in 0..n {
            let t = &self.world.tanks[i];
            let eye = t.pos + t.up() * t.gun_height();
            let yaw = t.turret_yaw();
            let mut best: Option<(f32, f32)> = None;
            for j in 0..n {
                if j == i || !self.fighters[j].alive {
                    continue;
                }
                let o = &self.world.tanks[j];
                let aim = o.pos + o.up() * (o.chassis.hull_top * 0.7);
                let to = aim - eye;
                let flat = Vec2::new(to.x, to.z);
                let d = flat.length();
                if !(2.0..=ASSIST_RANGE).contains(&d) || wrap_angle(yaw_of(flat) - yaw).abs() > ASSIST_CONE {
                    continue;
                }
                if !ai::can_see(&self.world, eye, o) {
                    continue;
                }
                // Lift for the shell's drop over the flight time.
                let t_flight = to.length() / SHELL_SPEED;
                let pitch = ((to.y + 0.5 * GRAVITY * t_flight * t_flight) / d).atan();
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, pitch));
                }
            }
            let t = &mut self.world.tanks[i];
            let hull_pitch = t.turret_dir().y.clamp(-1.0, 1.0).asin();
            let want = best.map_or(0.0, |(_, p)| (p - hull_pitch).clamp(-0.15, 0.3));
            t.gun_pitch += (want - t.gun_pitch) * (1.0 - (-DT * 12.0).exp());
        }
    }

    fn shoot(&mut self, i: usize) {
        let t = &self.world.tanks[i];
        let g = gun(t.turret_kind);
        let dir = t.gun_dir();
        let muzzle = t.pos + t.up() * t.gun_height() + dir * 2.6;
        self.shells.push(Shell { pos: muzzle, prev: muzzle, vel: dir * SHELL_SPEED + t.vel, owner: i, damage: g.damage, age: 0.0 });
        self.fighters[i].reload_in = g.reload;
        self.world.fire(i);
        if let Some(b) = &mut self.bots[i] {
            b.fired();
        }
    }

    fn fly_shells(&mut self) {
        let mut shells = std::mem::take(&mut self.shells);
        shells.retain_mut(|s| {
            s.age += DT;
            if s.age > SHELL_LIFE {
                return false;
            }
            s.vel.y -= GRAVITY * DT;
            s.prev = s.pos;
            let step = s.vel * DT;
            let len = step.length();
            let owner = self.world.tanks[s.owner].collider;
            let hit = self.world.physics.cast_ray(
                &Ray::new(s.pos, step / len),
                len,
                true,
                QueryFilter::default().exclude_collider(owner),
            );
            match hit {
                None => {
                    s.pos += step;
                    true
                }
                Some((c, toi)) => {
                    let at = s.pos + step / len * toi;
                    s.pos = at;
                    match self.world.tank_of(c) {
                        Some(j) => self.hit(s, j, at),
                        None => self.blasts.push(Blast::new(at, BlastKind::Impact)),
                    }
                    false
                }
            }
        });
        self.shells = shells;
    }

    fn hit(&mut self, s: &Shell, j: usize, at: Vec3) {
        self.blasts.push(Blast::new(at, BlastKind::Hit));
        // A shove through the hull's centre: felt, but never enough to roll it.
        let (mass, center) = (self.world.tanks[j].chassis.mass, self.world.tanks[j].pos + self.world.tanks[j].up() * 0.8);
        let shove = s.vel.normalize_or_zero() * Vec3::new(1.0, 0.3, 1.0);
        self.world.push(j, shove * mass * 0.35, center);
        let v = &mut self.fighters[j];
        if !v.alive || v.shield > 0.0 || self.phase != Phase::Playing {
            return;
        }
        v.health -= s.damage;
        v.since_hit = 0.0;
        self.fighters[s.owner].since_dealt = 0.0;
        if self.fighters[j].health <= 0.0 {
            self.kill(s.owner, j);
        }
    }

    fn kill(&mut self, killer: usize, victim: usize) {
        let v = &mut self.fighters[victim];
        v.health = 0.0;
        v.alive = false;
        v.deaths += 1;
        v.respawn_in = RESPAWN_TIME;
        v.killed_by = Some(killer);
        if killer != victim {
            self.fighters[killer].kills += 1;
            if killer == 0 {
                self.award(KILL_XP, "KILL");
            }
        }
        self.feed.push(Kill { killer, victim, age: 0.0 });
        let t = &self.world.tanks[victim];
        let (pos, mass) = (t.pos, t.chassis.mass);
        self.blasts.push(Blast::new(pos + Vec3::Y * 1.2, BlastKind::Kill));
        // Pop the wreck up and give it a lazy tumble.
        let off = Vec3::new(self.rng.range(-0.6, 0.6), 0.0, self.rng.range(-0.6, 0.6));
        self.world.push(victim, Vec3::Y * mass * 4.5, pos + off);
        self.world.spin(victim, Vec3::new(self.rng.range(-1.0, 1.0), 0.0, self.rng.range(-1.0, 1.0)) * mass * 1.2);
        if let Some(b) = &mut self.bots[victim] {
            b.reset();
        }
    }

    fn award(&mut self, amount: u32, reason: &'static str) {
        self.xp += amount;
        self.xp_pops.push(XpPop { amount, reason, age: 0.0 });
    }

    fn pick_up_repairs(&mut self) {
        let (fighters, tanks) = (&self.fighters, &self.world.tanks);
        let picked = self.repairs.step(DT, tanks, |i| fighters[i].alive && fighters[i].health < max_health(tanks[i].hull_kind));
        for (i, at) in picked {
            let max = max_health(self.world.tanks[i].hull_kind);
            let f = &mut self.fighters[i];
            let before = f.health;
            f.health = (f.health + max * crate::repair::HEAL_FRACTION).min(max);
            f.repaired = f.health - before;
            f.since_repair = 0.0;
            self.blasts.push(Blast::new(at, BlastKind::Repair));
        }
    }

    fn respawns(&mut self) {
        for i in 0..self.fighters.len() {
            if self.fighters[i].alive {
                continue;
            }
            let f = &mut self.fighters[i];
            let before = f.respawn_in;
            f.respawn_in -= DT;
            // Wrecks smoke: small puffs scattered off the hull, drifting up and apart.
            if (before / 0.25).floor() != (f.respawn_in / 0.25).floor() {
                let off = Vec3::new(self.rng.range(-1.0, 1.0), 0.0, self.rng.range(-1.0, 1.0));
                let p = self.world.tanks[i].pos + Vec3::Y * 1.2 + off;
                let drift = off * 1.5 + Vec3::Y * self.rng.range(3.5, 5.5);
                self.blasts.push(Blast { drift, ..Blast::new(p, BlastKind::Smoke) });
            }
            if f.respawn_in > 0.0 || self.phase != Phase::Playing {
                continue;
            }
            if let Some((hull, turret)) = self.fighters[i].next_kit.take() {
                self.world.set_hull(i, hull);
                self.world.tanks[i].turret_kind = turret;
            }
            let (at, yaw) = spawn_point(&self.world, &mut self.rng, Some((i, &self.fighters)));
            self.world.teleport(i, at, yaw);
            let f = &mut self.fighters[i];
            f.alive = true;
            f.health = max_health(self.world.tanks[i].hull_kind);
            f.shield = SPAWN_SHIELD;
            f.reload_in = 0.5;
        }
    }

    /// The tanks to draw this frame: interpolated, with wrecks charred.
    pub fn display_tanks(&self, alpha: f32) -> Vec<Tank> {
        self.world
            .tanks
            .iter()
            .zip(&self.fighters)
            .map(|(t, f)| {
                let mut t = t.interpolated(alpha);
                if !f.alive {
                    t.color *= 0.12;
                    t.since_fire = 10.0;
                }
                t
            })
            .collect()
    }

    /// Shells, impacts, explosions and smoke as render instances.
    pub fn effect_instances(&self, alpha: f32, out: &mut Vec<(MeshId, Instance)>) {
        self.repairs.instances(self.world.time, out);
        let tracer = Vec4::new(1.0, 0.72, 0.3, 3.0);
        for s in &self.shells {
            let p = s.prev.lerp(s.pos, alpha);
            let d = s.vel.normalize_or_zero();
            let q = Quat::from_rotation_arc(Vec3::Y, d);
            // A streak trailing back from the shell.
            let len = 3.2;
            out.push((MeshId::GEM, Instance::new(Mat4::from_scale_rotation_translation(Vec3::new(0.22, len, 0.22), q, p - d * len * 0.5), tracer)));
        }
        for b in &self.blasts {
            let f = b.age / b.life();
            let (mesh, size, color) = match b.kind {
                BlastKind::Impact => (MeshId::SPHERE, 0.4 + f * 1.4, Vec4::new(1.0, 0.75, 0.4, 3.0 * (1.0 - f))),
                BlastKind::Hit => (MeshId::SPHERE, 0.6 + f * 2.2, Vec4::new(1.0, 0.45, 0.15, 3.5 * (1.0 - f))),
                BlastKind::Kill => {
                    // Fireball swells then collapses.
                    let s = (f * 4.0).min(1.0) * (1.0 - f).sqrt();
                    (MeshId::SPHERE, 0.5 + s * 7.0, Vec4::new(1.0, 0.5 + 0.3 * (1.0 - f), 0.15, 4.0 * (1.0 - f * f)))
                }
                BlastKind::Repair => (MeshId::SPHERE, 1.0 + f * 4.0, Vec4::new(0.35, 1.0, 0.5, 2.5 * (1.0 - f))),
                BlastKind::Smoke => {
                    let s = (f * 4.0).min(1.0) * (1.0 - f);
                    let c = 0.12 + f * 0.15;
                    let p = b.pos + b.drift * f;
                    out.push((MeshId::SPHERE, Instance::new(Mat4::from_scale_rotation_translation(Vec3::splat(s * 1.3), Quat::IDENTITY, p), Vec4::new(c, c, c, 0.0))));
                    continue;
                }
            };
            out.push((mesh, Instance::new(Mat4::from_scale_rotation_translation(Vec3::splat(size), Quat::IDENTITY, b.pos), color)));
        }
    }
}

fn fighter(name: &str, health: f32) -> Fighter {
    Fighter {
        name: name.to_string(),
        health,
        alive: true,
        respawn_in: 0.0,
        shield: SPAWN_SHIELD,
        reload_in: 0.5,
        kills: 0,
        deaths: 0,
        killed_by: None,
        since_hit: 10.0,
        since_dealt: 10.0,
        next_kit: None,
        since_repair: 10.0,
        repaired: 0.0,
    }
}

/// A spawn spot well away from the living (for tank `me`, if respawning),
/// picked at random among the safest few, facing the map center.
fn spawn_point(world: &World, rng: &mut Rng, me: Option<(usize, &[Fighter])>) -> (Vec2, f32) {
    let spots = &world.map.open_spots;
    let others: Vec<Vec2> = world
        .tanks
        .iter()
        .enumerate()
        .filter(|(j, _)| me.is_none_or(|(i, fs)| *j != i && fs[*j].alive))
        .map(|(_, t)| Vec2::new(t.pos.x, t.pos.z))
        .collect();
    let mut scored: Vec<(f32, Vec2)> = spots
        .iter()
        .map(|&s| (others.iter().map(|o| o.distance(s)).fold(f32::INFINITY, f32::min), s))
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let pick = scored[rng.index(scored.len().min(4))].1;
    let to_center = world.map.center() - pick;
    let yaw = if to_center.length_squared() > 1.0 { yaw_of(to_center) } else { rng.range(0.0, std::f32::consts::TAU) };
    (pick, yaw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::dir2;

    fn arena(bots: usize) -> Match {
        Match::new(Map::generate(21), 0, bots, 21)
    }

    #[test]
    fn spawns_player_and_bots_apart() {
        let m = arena(5);
        assert_eq!(m.world.tanks.len(), 6);
        for i in 0..6 {
            for j in i + 1..6 {
                let d = m.world.tanks[i].pos.distance(m.world.tanks[j].pos);
                assert!(d > 8.0, "tanks {i} and {j} spawned {d:.1} m apart");
            }
        }
    }

    #[test]
    fn shell_hits_and_kills_a_tank_in_front() {
        let mut m = Match::new(Map::proving_ground(), 0, 0, 1);
        // A dummy straight ahead of the player.
        let t = &m.world.tanks[0];
        let ahead = Vec2::new(t.pos.x, t.pos.z) + dir2(t.yaw()) * 25.0;
        m.world.spawn_tank(Vec3::ONE, ahead, 0.0, HullKind::Lancer, TurretKind::Lance);
        m.fighters.push(fighter("DUMMY", max_health(HullKind::Lancer)));
        m.bots.push(None);
        for f in &mut m.fighters {
            f.shield = 0.0;
        }
        // Settle, then hold the trigger.
        for _ in 0..60 {
            m.step(Command::default(), false);
        }
        for _ in 0..(6.0 / DT) as usize {
            m.step(Command::default(), true);
        }
        assert!(m.fighters[1].deaths >= 1, "dummy has {} hp left", m.fighters[1].health);
        assert_eq!(m.fighters[0].kills, m.fighters[1].deaths);
        assert!(m.feed.iter().any(|k| k.killer == 0 && k.victim == 1));
        // Paid per kill, on the spot.
        assert_eq!(m.xp, m.fighters[0].kills * KILL_XP);
    }

    #[test]
    fn bots_killing_bots_pays_the_player_nothing() {
        let mut m = arena(2);
        m.kill(1, 2);
        m.kill(0, 0);
        assert_eq!(m.xp, 0);
        m.kill(0, 1);
        assert_eq!(m.xp, KILL_XP);
        assert_eq!(m.xp_pops.len(), 1, "one popup, for the player's own kill");
    }

    #[test]
    fn winning_the_round_pays_a_bonus() {
        let mut m = arena(1);
        m.fighters[0].kills = 1;
        m.time_left = DT * 0.5;
        m.step(Command::default(), false);
        assert_eq!(m.phase, Phase::Over);
        assert_eq!(m.xp, WIN_XP);
        // Losing pays nothing extra.
        let mut m = arena(1);
        m.fighters[1].kills = 1;
        m.time_left = DT * 0.5;
        m.step(Command::default(), false);
        assert_eq!(m.xp, 0);
    }

    #[test]
    fn dead_tanks_respawn_with_full_health() {
        let mut m = arena(1);
        m.fighters[1].shield = 0.0;
        m.kill(0, 1);
        assert!(!m.fighters[1].alive);
        for _ in 0..((RESPAWN_TIME + 0.2) / DT) as usize {
            m.step(Command::default(), false);
        }
        assert!(m.fighters[1].alive);
        assert_eq!(m.fighters[1].health, max_health(m.world.tanks[1].hull_kind));
        assert!(m.fighters[1].shield > 0.0);
    }

    #[test]
    fn queued_kit_is_fitted_at_respawn() {
        let mut m = arena(1);
        // Whatever the player has now, queue the other hull.
        let hull = if m.world.tanks[0].hull_kind == HullKind::Bastion { HullKind::Lancer } else { HullKind::Bastion };
        let kit = (hull, TurretKind::Maul);
        m.fighters[0].next_kit = Some(kit);
        // Not while alive.
        for _ in 0..30 {
            m.step(Command::default(), false);
        }
        assert_ne!(m.world.tanks[0].hull_kind, kit.0);
        m.fighters[0].shield = 0.0;
        m.kill(1, 0);
        for _ in 0..((RESPAWN_TIME + 0.2) / DT) as usize {
            m.step(Command::default(), false);
        }
        assert!(m.fighters[0].alive);
        assert_eq!((m.world.tanks[0].hull_kind, m.world.tanks[0].turret_kind), kit);
        assert_eq!(m.fighters[0].health, max_health(kit.0));
        assert_eq!(m.fighters[0].next_kit, None);
    }

    #[test]
    fn round_ends_at_the_clock() {
        let mut m = arena(0);
        m.time_left = 0.05;
        for _ in 0..20 {
            m.step(Command::default(), false);
        }
        assert_eq!(m.phase, Phase::Over);
        assert_eq!(m.time_left, 0.0);
    }

    #[test]
    fn driving_onto_a_landed_crate_repairs_the_hull() {
        let mut m = Match::new(Map::proving_ground(), 0, 0, 1);
        for _ in 0..60 {
            m.step(Command::default(), false);
        }
        let max = max_health(m.world.tanks[0].hull_kind);
        m.fighters[0].health = 10.0;
        let t = &m.world.tanks[0];
        let d = &mut m.repairs.drops[0];
        d.at = Vec2::new(t.pos.x, t.pos.z);
        d.state = crate::repair::State::Falling(0.01);
        m.step(Command::default(), false);
        m.step(Command::default(), false);
        assert_eq!(m.fighters[0].health, 10.0 + max * crate::repair::HEAL_FRACTION);
        assert!(matches!(m.repairs.drops[0].state, crate::repair::State::Waiting(t) if t > crate::repair::COOLDOWN - 0.1));
        assert!(m.blasts.iter().any(|b| b.kind == BlastKind::Repair));
    }

    #[test]
    fn standings_rank_by_kills_then_deaths() {
        let mut m = arena(2);
        m.fighters[2].kills = 3;
        m.fighters[1].kills = 3;
        m.fighters[1].deaths = 1;
        assert_eq!(m.standings(), vec![2, 1, 0]);
    }

    /// Left alone, bots find each other and fight: kills happen within a minute.
    #[test]
    fn bots_fight_each_other() {
        let mut m = arena(4);
        // Park the player out of the way, invulnerable.
        for _ in 0..(60.0 / DT) as usize {
            m.fighters[0].shield = 1.0;
            m.step(Command::default(), false);
        }
        let kills: u32 = m.fighters.iter().map(|f| f.kills).sum();
        assert!(kills >= 2, "only {kills} kills in a minute of bot deathmatch");
    }
}
