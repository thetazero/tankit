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
/// Imperium cannon shell speed, m/s.
pub const SHELL_SPEED: f32 = 160.0;
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
#[derive(Clone, Copy)]
pub struct Gun {
    /// Per shot; for a beam, per second of contact.
    pub damage: f32,
    /// Seconds between shots (0 for a beam).
    pub reload: f32,
    pub action: Action,
}

#[derive(Clone, Copy)]
pub enum Action {
    /// Fires projectiles.
    Shell(ShellSpec),
    /// A hitscan beam for as long as the trigger is held, until it overheats.
    Beam {
        range: f32,
        /// Seconds of continuous fire from cold to overheated.
        heat_time: f32,
        /// Seconds to cool from fully hot back to zero. Once overheated the
        /// beam stays locked until it has cooled all the way.
        cool_time: f32,
    },
}

#[derive(Clone, Copy)]
pub struct ShellSpec {
    /// Muzzle speed, m/s.
    pub speed: f32,
    /// Seconds before it fizzles out.
    pub life: f32,
    /// Multiple of real gravity (negative rises, like flame).
    pub gravity: f32,
    /// Times it ricochets off walls and ground before it's spent.
    pub bounces: u8,
    /// Random aim jitter per shot, radians.
    pub spread: f32,
    pub look: Look,
}

/// How a projectile is drawn, and how hard it lands.
#[derive(Clone, Copy, PartialEq)]
pub enum Look {
    /// Orange cannon tracer streak.
    Tracer,
    /// A glowing ball of plasma, `size` m across.
    Plasma { size: f32 },
    /// A puff of crystal fire that swells as it flies.
    Flame,
}

/// Sorn plasma glow.
const PLASMA: Vec3 = Vec3::new(0.35, 1.0, 0.7);
/// Machine laser.
const LASER: Vec3 = Vec3::new(1.0, 0.12, 0.08);

impl Gun {
    pub fn dps(&self) -> f32 {
        match self.action {
            Action::Beam { .. } => self.damage,
            Action::Shell(_) => self.damage / self.reload,
        }
    }

    pub fn range(&self) -> f32 {
        match self.action {
            Action::Beam { range, .. } => range,
            Action::Shell(s) => s.speed * s.life,
        }
    }

    /// Projectile speed for leading a target (a beam lands instantly).
    pub fn speed(&self) -> f32 {
        match self.action {
            Action::Beam { .. } => f32::INFINITY,
            Action::Shell(s) => s.speed,
        }
    }

    /// Projectile drop, m/s².
    pub fn gravity(&self) -> f32 {
        match self.action {
            Action::Beam { .. } => 0.0,
            Action::Shell(s) => s.gravity * GRAVITY,
        }
    }
}

pub fn gun(kind: TurretKind) -> Gun {
    let cannon = ShellSpec {
        speed: SHELL_SPEED,
        life: 1.6,
        gravity: 1.0,
        bounces: 0,
        spread: 0.0,
        look: Look::Tracer,
    };
    match kind {
        // Imperium long rail cannon: hits hard, slow.
        TurretKind::Lance => Gun {
            damage: 38.0,
            reload: 1.4,
            action: Action::Shell(cannon),
        },
        // Imperium twin short guns: light hits, alternating barrels, fast.
        TurretKind::Maul => Gun {
            damage: 15.0,
            reload: 0.5,
            action: Action::Shell(cannon),
        },
        // Sorn: a big slow orb of plasma that skips off walls and ground.
        TurretKind::Wisp => Gun {
            damage: 30.0,
            reload: 1.1,
            action: Action::Shell(ShellSpec {
                speed: 42.0,
                life: 3.2,
                gravity: 0.0,
                bounces: 3,
                spread: 0.0,
                look: Look::Plasma { size: 0.75 },
            }),
        },
        // Sorn: three barrels taking turns, a stream of small slow bolts.
        TurretKind::Triad => Gun {
            damage: 7.0,
            reload: 0.2,
            action: Action::Shell(ShellSpec {
                speed: 55.0,
                life: 1.8,
                gravity: 0.0,
                bounces: 0,
                spread: 0.012,
                look: Look::Plasma { size: 0.38 },
            }),
        },
        // Machines: a laser that burns while held, then overheats.
        TurretKind::Lumen => Gun {
            damage: 34.0,
            reload: 0.0,
            action: Action::Beam {
                range: 85.0,
                heat_time: 3.0,
                cool_time: 2.2,
            },
        },
        // Resonant: a short gout of crystal fire; deadly up close, nothing at range.
        TurretKind::Hymn => Gun {
            damage: 2.0,
            reload: 1.0 / 30.0,
            action: Action::Shell(ShellSpec {
                speed: 24.0,
                life: 0.65,
                gravity: -0.3,
                bounces: 0,
                spread: 0.09,
                look: Look::Flame,
            }),
        },
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
    /// Beam weapons: heat 0..1, and whether it overheated and is locked until cool.
    pub heat: f32,
    pub overheated: bool,
    /// Where this tank's beam ends this step, while it's firing one.
    pub beam: Option<Vec3>,
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
    pub spec: ShellSpec,
    pub bounces: u8,
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
        Blast {
            pos,
            kind,
            age: 0.0,
            drift: Vec3::ZERO,
        }
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
            let hull = if rng.chance(0.5) {
                HullKind::Lancer
            } else {
                HullKind::Bastion
            };
            let turret = TurretKind::ALL[rng.index(TurretKind::ALL.len())];
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
        order.sort_by_key(|&i| {
            (
                std::cmp::Reverse(self.fighters[i].kills),
                self.fighters[i].deaths,
                i,
            )
        });
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

        let contacts: Vec<Contact> = self
            .fighters
            .iter()
            .map(|f| Contact {
                targetable: f.alive && f.shield <= 0.0,
            })
            .collect();
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
            f.beam = None;
            match gun(self.world.tanks[i].turret_kind).action {
                Action::Beam {
                    range,
                    heat_time,
                    cool_time,
                } => {
                    if trigger && f.alive && !f.overheated {
                        f.heat += DT / heat_time;
                        if f.heat >= 1.0 {
                            f.heat = 1.0;
                            f.overheated = true;
                        }
                        self.beam(i, range);
                    } else {
                        f.heat = (f.heat - DT / cool_time).max(0.0);
                        if f.heat == 0.0 {
                            f.overheated = false;
                        }
                    }
                }
                Action::Shell(_) => {
                    if trigger && f.alive && f.reload_in <= 0.0 {
                        self.shoot(i);
                    }
                }
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
            let g = gun(t.turret_kind);
            let range = g.range().min(ASSIST_RANGE);
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
                if !(2.0..=range).contains(&d) || wrap_angle(yaw_of(flat) - yaw).abs() > ASSIST_CONE
                {
                    continue;
                }
                if !ai::can_see(&self.world, eye, o) {
                    continue;
                }
                // Lift for the shot's drop over the flight time.
                let t_flight = to.length() / g.speed();
                let pitch = ((to.y + 0.5 * g.gravity() * t_flight * t_flight) / d).atan();
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
        let Action::Shell(spec) = g.action else {
            return;
        };
        let mut dir = t.gun_dir();
        if spec.spread > 0.0 {
            let jitter = Vec3::new(
                self.rng.range(-1.0, 1.0),
                self.rng.range(-0.5, 0.5),
                self.rng.range(-1.0, 1.0),
            );
            dir = (dir + jitter * spec.spread).normalize();
        }
        let from = muzzle(t);
        self.shells.push(Shell {
            pos: from,
            prev: from,
            vel: dir * spec.speed + t.vel,
            owner: i,
            damage: g.damage,
            age: 0.0,
            spec,
            bounces: spec.bounces,
        });
        // Carry the leftover fraction of a step so fast guns keep their exact rate.
        let f = &mut self.fighters[i];
        f.reload_in = f.reload_in.max(-DT) + g.reload;
        if spec.look != Look::Flame {
            // A flamethrower pours: no per-puff kick or muzzle flash.
            self.world.fire(i);
        }
        if let Some(b) = &mut self.bots[i] {
            b.fired();
        }
    }

    /// One step of tank `i`'s beam: burn whatever it touches first.
    fn beam(&mut self, i: usize, range: f32) {
        let t = &self.world.tanks[i];
        let (from, dir) = (muzzle(t), t.gun_dir());
        let hit = self.world.physics.cast_ray(
            &Ray::new(from, dir),
            range,
            true,
            QueryFilter::default().exclude_collider(t.collider),
        );
        let (end, victim) = match hit {
            Some((c, toi)) => (from + dir * toi, self.world.tank_of(c)),
            None => (from + dir * range, None),
        };
        self.fighters[i].beam = Some(end);
        let dmg = gun(self.world.tanks[i].turret_kind).damage * DT;
        match victim {
            Some(j) => self.damage(i, j, dmg),
            // Bots correct their aim after a miss.
            None => {
                if let Some(b) = &mut self.bots[i] {
                    b.fired();
                }
            }
        }
    }

    fn fly_shells(&mut self) {
        let mut shells = std::mem::take(&mut self.shells);
        shells.retain_mut(|s| {
            s.age += DT;
            if s.age > s.spec.life {
                return false;
            }
            s.vel.y -= s.spec.gravity * GRAVITY * DT;
            s.prev = s.pos;
            let step = s.vel * DT;
            let len = step.length();
            if len < 1e-6 {
                return true;
            }
            let owner = self.world.tanks[s.owner].collider;
            let hit = self.world.physics.cast_ray_and_get_normal(
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
                Some((c, ri)) => {
                    let at = s.pos + step / len * ri.time_of_impact;
                    s.pos = at;
                    match self.world.tank_of(c) {
                        Some(j) => {
                            self.hit(s, j, at);
                            false
                        }
                        None if s.bounces > 0 => {
                            // Ricochet: mirror off the surface and carry on.
                            let n = ri.normal;
                            s.vel -= 2.0 * s.vel.dot(n) * n;
                            s.pos = at + n * 0.05;
                            s.bounces -= 1;
                            self.blasts.push(Blast::new(at, BlastKind::Impact));
                            true
                        }
                        None => {
                            if s.spec.look != Look::Flame {
                                self.blasts.push(Blast::new(at, BlastKind::Impact));
                            }
                            false
                        }
                    }
                }
            }
        });
        self.shells = shells;
    }

    fn hit(&mut self, s: &Shell, j: usize, at: Vec3) {
        if s.spec.look != Look::Flame {
            self.blasts.push(Blast::new(at, BlastKind::Hit));
            // A shove through the hull's centre, scaled by how hard the shot
            // hits: felt, but never enough to roll it.
            let (mass, center) = (
                self.world.tanks[j].chassis.mass,
                self.world.tanks[j].pos + self.world.tanks[j].up() * 0.8,
            );
            let shove = s.vel.normalize_or_zero() * Vec3::new(1.0, 0.3, 1.0);
            let weight = (s.damage / 38.0).min(1.0);
            self.world.push(j, shove * mass * 0.35 * weight, center);
        }
        self.damage(s.owner, j, s.damage);
    }

    /// `owner` deals `amount` to tank `j` (spawn shields and the round's end permitting).
    fn damage(&mut self, owner: usize, j: usize, amount: f32) {
        let v = &mut self.fighters[j];
        if !v.alive || v.shield > 0.0 || self.phase != Phase::Playing {
            return;
        }
        v.health -= amount;
        v.since_hit = 0.0;
        self.fighters[owner].since_dealt = 0.0;
        if self.fighters[j].health <= 0.0 {
            self.kill(owner, j);
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
        self.feed.push(Kill {
            killer,
            victim,
            age: 0.0,
        });
        let t = &self.world.tanks[victim];
        let (pos, mass) = (t.pos, t.chassis.mass);
        self.blasts
            .push(Blast::new(pos + Vec3::Y * 1.2, BlastKind::Kill));
        // Pop the wreck up and give it a lazy tumble.
        let off = Vec3::new(self.rng.range(-0.6, 0.6), 0.0, self.rng.range(-0.6, 0.6));
        self.world.push(victim, Vec3::Y * mass * 4.5, pos + off);
        self.world.spin(
            victim,
            Vec3::new(self.rng.range(-1.0, 1.0), 0.0, self.rng.range(-1.0, 1.0)) * mass * 1.2,
        );
        if let Some(b) = &mut self.bots[victim] {
            b.reset();
        }
    }

    fn award(&mut self, amount: u32, reason: &'static str) {
        self.xp += amount;
        self.xp_pops.push(XpPop {
            amount,
            reason,
            age: 0.0,
        });
    }

    fn pick_up_repairs(&mut self) {
        let (fighters, tanks) = (&self.fighters, &self.world.tanks);
        let picked = self.repairs.step(DT, tanks, |i| {
            fighters[i].alive && fighters[i].health < max_health(tanks[i].hull_kind)
        });
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
                self.blasts.push(Blast {
                    drift,
                    ..Blast::new(p, BlastKind::Smoke)
                });
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
            f.heat = 0.0;
            f.overheated = false;
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
            let streak = |w: f32, len: f32, color: Vec4| {
                (
                    MeshId::GEM,
                    Instance::new(
                        Mat4::from_scale_rotation_translation(
                            Vec3::new(w, len, w),
                            q,
                            p - d * len * 0.5,
                        ),
                        color,
                    ),
                )
            };
            let ball = |at: Vec3, size: f32, color: Vec4| {
                (
                    MeshId::SPHERE,
                    Instance::new(
                        Mat4::from_scale_rotation_translation(
                            Vec3::splat(size),
                            Quat::IDENTITY,
                            at,
                        ),
                        color,
                    ),
                )
            };
            match s.spec.look {
                // A streak trailing back from the shell.
                Look::Tracer => out.push(streak(0.22, 3.2, tracer)),
                // A shimmering core with a soft halo and a short wake.
                Look::Plasma { size } => {
                    let wobble = 1.0 + 0.12 * (s.age * 40.0).sin();
                    out.push(ball(p, size * wobble, PLASMA.extend(2.5)));
                    out.push(ball(p, size * 0.55, Vec4::new(0.85, 1.0, 0.95, 4.0)));
                    out.push(streak(size * 0.5, size * 3.0, PLASMA.extend(1.6)));
                }
                // Crystal fire: white-hot at the nozzle, swelling into violet, then gone.
                Look::Flame => {
                    let f = s.age / s.spec.life;
                    let size = (0.35 + f * 2.2) * (1.0 - f * f).max(0.0).sqrt();
                    let hot = Vec3::new(0.95, 0.85, 1.0).lerp(Vec3::new(0.55, 0.2, 1.0), f.sqrt());
                    out.push(ball(p, size, hot.extend(3.0 * (1.0 - f) + 0.3)));
                }
            }
        }
        // Laser beams: a hot core inside a wider red glow, and a burn spot where it lands.
        for (f, t) in self.fighters.iter().zip(&self.world.tanks) {
            let Some(end) = f.beam else { continue };
            let from = muzzle(t);
            let to = end - from;
            let len = to.length();
            if len < 0.1 {
                continue;
            }
            let q = Quat::from_rotation_arc(Vec3::Y, to / len);
            let flicker = 1.0 + 0.15 * (self.world.time * 70.0).sin();
            for (w, c) in [
                (0.32 * flicker, LASER.extend(2.2)),
                (0.1, Vec4::new(1.0, 0.75, 0.7, 4.5)),
            ] {
                out.push((
                    MeshId::CYLINDER,
                    Instance::new(
                        Mat4::from_scale_rotation_translation(
                            Vec3::new(w, len, w),
                            q,
                            from + to * 0.5,
                        ),
                        c,
                    ),
                ));
            }
            out.push((
                MeshId::SPHERE,
                Instance::new(
                    Mat4::from_scale_rotation_translation(
                        Vec3::splat(0.7 * flicker),
                        Quat::IDENTITY,
                        end,
                    ),
                    Vec4::new(1.0, 0.45, 0.3, 4.0),
                ),
            ));
        }
        for b in &self.blasts {
            let f = b.age / b.life();
            let (mesh, size, color) = match b.kind {
                BlastKind::Impact => (
                    MeshId::SPHERE,
                    0.4 + f * 1.4,
                    Vec4::new(1.0, 0.75, 0.4, 3.0 * (1.0 - f)),
                ),
                BlastKind::Hit => (
                    MeshId::SPHERE,
                    0.6 + f * 2.2,
                    Vec4::new(1.0, 0.45, 0.15, 3.5 * (1.0 - f)),
                ),
                BlastKind::Kill => {
                    // Fireball swells then collapses.
                    let s = (f * 4.0).min(1.0) * (1.0 - f).sqrt();
                    (
                        MeshId::SPHERE,
                        0.5 + s * 7.0,
                        Vec4::new(1.0, 0.5 + 0.3 * (1.0 - f), 0.15, 4.0 * (1.0 - f * f)),
                    )
                }
                BlastKind::Repair => (
                    MeshId::SPHERE,
                    1.0 + f * 4.0,
                    Vec4::new(0.35, 1.0, 0.5, 2.5 * (1.0 - f)),
                ),
                BlastKind::Smoke => {
                    let s = (f * 4.0).min(1.0) * (1.0 - f);
                    let c = 0.12 + f * 0.15;
                    let p = b.pos + b.drift * f;
                    out.push((
                        MeshId::SPHERE,
                        Instance::new(
                            Mat4::from_scale_rotation_translation(
                                Vec3::splat(s * 1.3),
                                Quat::IDENTITY,
                                p,
                            ),
                            Vec4::new(c, c, c, 0.0),
                        ),
                    ));
                    continue;
                }
            };
            out.push((
                mesh,
                Instance::new(
                    Mat4::from_scale_rotation_translation(Vec3::splat(size), Quat::IDENTITY, b.pos),
                    color,
                ),
            ));
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
        heat: 0.0,
        overheated: false,
        beam: None,
    }
}

/// Where tank `t`'s shots leave the gun.
fn muzzle(t: &Tank) -> Vec3 {
    t.pos + t.up() * t.gun_height() + t.gun_dir() * 2.6
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
        .map(|&s| {
            (
                others
                    .iter()
                    .map(|o| o.distance(s))
                    .fold(f32::INFINITY, f32::min),
                s,
            )
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let pick = scored[rng.index(scored.len().min(4))].1;
    let to_center = world.map.center() - pick;
    let yaw = if to_center.length_squared() > 1.0 {
        yaw_of(to_center)
    } else {
        rng.range(0.0, std::f32::consts::TAU)
    };
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
        m.world
            .spawn_tank(Vec3::ONE, ahead, 0.0, HullKind::Lancer, TurretKind::Lance);
        m.fighters
            .push(fighter("DUMMY", max_health(HullKind::Lancer)));
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
        assert!(
            m.fighters[1].deaths >= 1,
            "dummy has {} hp left",
            m.fighters[1].health
        );
        assert_eq!(m.fighters[0].kills, m.fighters[1].deaths);
        assert!(m.feed.iter().any(|k| k.killer == 0 && k.victim == 1));
        // Paid per kill, on the spot.
        assert_eq!(m.xp, m.fighters[0].kills * KILL_XP);
    }

    /// The player with `turret` and a dummy `dist` m straight ahead, shields down, settled.
    fn duel(turret: TurretKind, dist: f32) -> Match {
        let mut m = Match::new(Map::proving_ground(), 0, 0, 1);
        m.world.tanks[0].turret_kind = turret;
        let t = &m.world.tanks[0];
        let ahead = Vec2::new(t.pos.x, t.pos.z) + dir2(t.yaw()) * dist;
        m.world
            .spawn_tank(Vec3::ONE, ahead, 0.0, HullKind::Lancer, TurretKind::Lance);
        m.fighters
            .push(fighter("DUMMY", max_health(HullKind::Lancer)));
        m.bots.push(None);
        for f in &mut m.fighters {
            f.shield = 0.0;
        }
        for _ in 0..60 {
            m.step(Command::default(), false);
        }
        m
    }

    /// Every faction's turret can kill a tank in front of it at close range.
    #[test]
    fn every_turret_kills_at_close_range() {
        for &k in &TurretKind::ALL {
            let mut m = duel(k, 11.0);
            for _ in 0..(8.0 / DT) as usize {
                m.step(Command::default(), true);
            }
            assert!(
                m.fighters[1].deaths >= 1,
                "{k:?}: dummy has {} hp left",
                m.fighters[1].health
            );
        }
    }

    #[test]
    fn flame_falls_short_at_range() {
        let mut m = duel(TurretKind::Hymn, 40.0);
        for _ in 0..(3.0 / DT) as usize {
            m.step(Command::default(), true);
        }
        assert_eq!(m.fighters[1].health, max_health(HullKind::Lancer));
        assert!(!m.shells.is_empty(), "the flamer should be pouring fire");
    }

    #[test]
    fn beam_overheats_and_locks_until_cool() {
        let mut m = duel(TurretKind::Lumen, 60.0);
        let Action::Beam {
            heat_time,
            cool_time,
            ..
        } = gun(TurretKind::Lumen).action
        else {
            panic!("Lumen is a beam");
        };
        for _ in 0..((heat_time + 0.1) / DT) as usize {
            m.step(Command::default(), true);
        }
        assert!(m.fighters[0].overheated);
        // Still holding the trigger: nothing comes out while it cools.
        m.step(Command::default(), true);
        assert!(m.fighters[0].beam.is_none());
        let hp = m.fighters[1].health;
        for _ in 0..((cool_time - 0.2) / DT) as usize {
            m.step(Command::default(), true);
        }
        assert!(m.fighters[0].overheated);
        assert_eq!(m.fighters[1].health, hp);
        for _ in 0..(0.4 / DT) as usize {
            m.step(Command::default(), true);
        }
        assert!(!m.fighters[0].overheated);
        assert!(m.fighters[0].beam.is_some());
    }

    #[test]
    fn plasma_orbs_bounce_off_the_ground() {
        let mut m = Match::new(Map::proving_ground(), 0, 0, 1);
        let Action::Shell(spec) = gun(TurretKind::Wisp).action else {
            panic!("Wisp fires shells");
        };
        let t = &m.world.tanks[0];
        let from = t.pos + Vec3::new(8.0, 3.0, 8.0);
        m.shells.push(Shell {
            pos: from,
            prev: from,
            vel: Vec3::new(0.0, -spec.speed, 0.0),
            owner: 0,
            damage: 1.0,
            age: 0.0,
            spec,
            bounces: spec.bounces,
        });
        for _ in 0..(0.2 / DT) as usize {
            m.step(Command::default(), false);
        }
        assert_eq!(
            m.shells.len(),
            1,
            "the orb should survive hitting the ground"
        );
        assert!(m.shells[0].vel.y > 0.0, "and come back up");
        assert_eq!(m.shells[0].bounces, spec.bounces - 1);
        // Cannon shells just stop.
        let Action::Shell(cannon) = gun(TurretKind::Lance).action else {
            panic!();
        };
        m.shells = vec![Shell {
            pos: from,
            prev: from,
            vel: Vec3::new(0.0, -cannon.speed, 0.0),
            owner: 0,
            damage: 1.0,
            age: 0.0,
            spec: cannon,
            bounces: 0,
        }];
        for _ in 0..(0.2 / DT) as usize {
            m.step(Command::default(), false);
        }
        assert!(m.shells.is_empty());
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
        let hull = if m.world.tanks[0].hull_kind == HullKind::Bastion {
            HullKind::Lancer
        } else {
            HullKind::Bastion
        };
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
        assert_eq!(
            (m.world.tanks[0].hull_kind, m.world.tanks[0].turret_kind),
            kit
        );
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
        assert_eq!(
            m.fighters[0].health,
            10.0 + max * crate::repair::HEAL_FRACTION
        );
        assert!(
            matches!(m.repairs.drops[0].state, crate::repair::State::Waiting(t) if t > crate::repair::COOLDOWN - 0.1)
        );
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
        assert!(
            kills >= 2,
            "only {kills} kills in a minute of bot deathmatch"
        );
    }
}
