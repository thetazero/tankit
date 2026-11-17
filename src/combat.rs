//! Team fights: two factions, five-minute rounds, no friendly fire; each side
//! spawns at its own base. Under the Attrition contract the side with the
//! most kills wins; under Hold, the side that holds the site longest (`hold`).
//!
//! Wraps the physics `World` with what a fight needs: shells in flight, hull
//! health, kills and deaths, respawns, the round clock, and the bots. Tank 0
//! is the player, on side 0; bots fill both sides.

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use rapier3d::prelude::{Collider, ColliderHandle, QueryFilter, Ray};

use crate::ai::{self, Bot, Contact, NavGrid};
use crate::arrival::{ARRIVAL_TIME, Arrival};
use crate::economy::Levels;
use crate::faction::Faction;
use crate::game::DT;
use crate::hold::{self, Hold};
use crate::map::Map;
use crate::mission::Contract;
use crate::render::Particle;
use crate::render::mesh::{Instance, MeshId};
use crate::repair::Repairs;
use crate::rng::Rng;
use crate::sim::{Command, GRAVITY, HullKind, Tank, TurretKind, World, wrap_angle, yaw_of};

pub const ROUND_TIME: f32 = 300.0;
pub const RESPAWN_TIME: f32 = 4.0;
/// Tanks wear their own paint, not their side's color: sides are told apart
/// by their markers on the HUD.
pub const TANK_PAINT: Vec3 = Vec3::ONE;
/// Freshly spawned tanks can't be hurt (and bots ignore them) for this long.
pub const SPAWN_SHIELD: f32 = 2.0;
/// Imperium cannon shots land the moment they're fired (a raycast); this is
/// only how fast the tracer they leave is drawn flying to where they landed, m/s.
pub const TRACER_SPEED: f32 = 1500.0;
/// How far an Imperium shot reaches: corner to corner of the biggest map, m.
pub const CANNON_RANGE: f32 = 500.0;
/// Length of that tracer's streak, m: about a frame's flight.
const TRACER_STREAK: f32 = 22.0;
/// ...and how long it lingers, fading, once it's reached where the shot
/// landed, s (at close range the flight itself is shorter than a frame).
const TRACER_FADE: f32 = 0.08;
/// Vertical aim assist: a target this close (horizontally, radians) to the gun line gets the elevation.
const ASSIST_CONE: f32 = 0.09;
const ASSIST_RANGE: f32 = 150.0;
pub const FEED_TIME: f32 = 6.0;
/// Knocked off the edge this soon after a hit: the hit gets the kill, s.
const FALL_CREDIT: f32 = 6.0;
/// A broken Vigil keeps running: at full strength for this long, s...
pub const HUSK_STRONG: f32 = 2.0;
/// ...then its speed and firepower fade out over this long, s.
pub const HUSK_FADE: f32 = 3.0;
/// A husk's cracked bell tolls this often while it runs, s.
pub const HUSK_TOLL: f32 = 1.25;
/// Reputation with your employer for each enemy the player kills, paid the moment it dies.
pub const KILL_REPUTATION: u32 = 10;
/// How long a reputation popup stays on screen, seconds.
pub const REPUTATION_POP_TIME: f32 = 2.0;

/// Weapon numbers per turret.
#[derive(Clone, Copy)]
pub struct Gun {
    /// Per shot; for a beam, per second of contact.
    pub damage: f32,
    /// Seconds between shots (0 for a beam). For a spin-up gun, at full spin.
    pub reload: f32,
    /// Spin-up guns: seconds of holding the trigger from still to full spin.
    /// They fire from the first moment, but slowly (`SPOOL_FLOOR`). 0: none.
    pub spool: f32,
    pub action: Action,
}

/// A spin-up gun's fire rate when it's barely turning, as a fraction of full.
pub const SPOOL_FLOOR: f32 = 0.3;

#[derive(Clone, Copy)]
pub enum Action {
    /// Fires projectiles.
    Shell(ShellSpec),
    /// A hitscan beam for as long as the trigger is held, until it overheats.
    /// Firing makes one unit of heat a second; the tank holds as much as its
    /// hull's and turret's thermal mass together (`heat_capacity`), and sheds
    /// it at the flat `COOL_RATE`. Once overheated the beam stays locked until
    /// it has cooled all the way.
    Beam {
        range: f32,
        /// The turret's share of the heat capacity: seconds of fire.
        thermal_mass: f32,
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
    /// Lands the instant it's fired, along a straight line out to its range
    /// (`speed * life`): no travel time to lead, nothing to dodge. What flies
    /// afterwards is only its tracer, which hits nothing.
    pub instant: bool,
}

/// How a projectile is drawn, and how hard it lands.
#[derive(Clone, Copy, PartialEq)]
pub enum Look {
    /// Orange cannon tracer streak.
    Tracer,
    /// A ball of plasma, `size` m across, held in its own skin.
    Plasma { size: f32 },
    /// A dart of plasma, `size` m long.
    Bolt { size: f32 },
    /// A puff of crystal fire that swells as it flies.
    Flame,
}

/// How big a flame puff is drawn (its scale, m) at `f` of its life: small
/// at the nozzle, swelling, then guttering out.
fn flame_size(f: f32) -> f32 {
    (0.3 + f * 2.6) * (1.0 - f * f * f).max(0.0).sqrt()
}

/// A puff is drawn as three balls of radius 0.375-0.475 `flame_size` set
/// 0.3 of it off its centre, so the fire it shows reaches about this far.
const FLAME_BODY: f32 = 0.7;

/// How far from its centre a flame puff burns: the fire it shows.
fn flame_reach(s: &Shell) -> f32 {
    FLAME_BODY * flame_size(s.age / s.spec.life)
}

/// An Ammeth orb shrinks a little at every skip.
fn plasma_shrink(s: &Shell) -> f32 {
    let spent = 1.0 - s.bounces as f32 / s.spec.bounces.max(1) as f32;
    1.0 - 0.18 * spent
}

/// Ithessar plasma.
const PLASMA: Vec3 = crate::tank::ithessar::PLASMA;
const PLASMA_HOT: Vec3 = crate::tank::ithessar::PLASMA_HOT;
/// Resonant crystal light (husks, their burst).
const CRYSTAL: Vec3 = crate::tank::resonant::CRYSTAL_LIGHT;
const CRYSTAL_HOT: Vec3 = crate::tank::resonant::CRYSTAL_LIGHT_HOT;
/// Uthenn laser green.
const LASER: Vec3 = crate::tank::uthenn::LASER_LIGHT;
/// Heat a beam sheds per second, the same for every tank.
pub const COOL_RATE: f32 = 1.35;
/// A beam does full damage out to this fraction of its range...
const BEAM_FULL: f32 = 0.4;
/// ...and this fraction of it at full range.
const BEAM_FAR: f32 = 0.6;
/// A beam bends onto an enemy this close (radians) to where it's aimed.
const BEAM_ASSIST: f32 = 0.035;

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

    /// Projectile speed for leading a target (beams and instant shots land
    /// at once).
    pub fn speed(&self) -> f32 {
        match self.action {
            Action::Beam { .. } => f32::INFINITY,
            Action::Shell(s) if s.instant => f32::INFINITY,
            Action::Shell(s) => s.speed,
        }
    }

    /// Projectile drop, m/s².
    pub fn gravity(&self) -> f32 {
        match self.action {
            Action::Beam { .. } => 0.0,
            Action::Shell(s) if s.instant => 0.0,
            Action::Shell(s) => s.gravity * GRAVITY,
        }
    }
}

pub fn gun(kind: TurretKind) -> Gun {
    let cannon = ShellSpec {
        speed: TRACER_SPEED,
        life: CANNON_RANGE / TRACER_SPEED,
        gravity: 0.0,
        bounces: 0,
        spread: 0.0,
        look: Look::Tracer,
        instant: true,
    };
    match kind {
        // Imperium long rail cannon: hits hard, slow to reload, lands instantly.
        TurretKind::Kontos => Gun {
            damage: 38.0,
            reload: 1.4,
            spool: 0.0,
            action: Action::Shell(cannon),
        },
        // Imperium twin short guns: light hits, alternating barrels, fast;
        // land instantly too.
        TurretKind::Pelekys => Gun {
            damage: 15.0,
            reload: 0.5,
            spool: 0.0,
            action: Action::Shell(cannon),
        },
        // Ithessar: an orb of plasma charged in a crystal cradle. Slow, and it
        // skips off walls and ground, a little weaker each time.
        TurretKind::Ammeth => Gun {
            damage: 32.0,
            reload: 1.2,
            spool: 0.0,
            action: Action::Shell(ShellSpec {
                speed: 38.0,
                life: 3.4,
                gravity: 0.0,
                bounces: 3,
                spread: 0.0,
                look: Look::Plasma { size: 1.3 },
                instant: false,
            }),
        },
        // Ithessar: three crystal needles spinning up into a stream of darts.
        TurretKind::Ossrin => Gun {
            damage: 6.0,
            reload: 0.11,
            spool: 1.1,
            action: Action::Shell(ShellSpec {
                speed: 68.0,
                life: 1.4,
                gravity: 0.0,
                bounces: 0,
                spread: 0.014,
                look: Look::Bolt { size: 1.0 },
                instant: false,
            }),
        },
        // Uthenn: a laser that burns while held, then overheats.
        TurretKind::Lumen => Gun {
            damage: 34.0,
            reload: 0.0,
            spool: 0.0,
            action: Action::Beam {
                range: 160.0,
                thermal_mass: 1.6,
            },
        },
        // Resonant: a short gout of crystal fire; deadly up close, nothing at range.
        TurretKind::Hymn => Gun {
            damage: 1.0,
            reload: 1.0 / 60.0,
            spool: 0.0,
            action: Action::Shell(ShellSpec {
                speed: 24.0,
                life: 0.65,
                gravity: -0.3,
                bounces: 0,
                spread: 0.11,
                look: Look::Flame,
                instant: false,
            }),
        },
    }
}

/// Seconds of beam fire a hull's bulk soaks up before it overheats (the
/// turret adds its own `thermal_mass`).
pub fn thermal_mass(kind: HullKind) -> f32 {
    match kind {
        HullKind::Kataphrakt => 1.4,
        HullKind::Kastron => 2.2,
        HullKind::Sethuun => 1.0,
        HullKind::Vigil => 1.1,
        HullKind::Gravis => 3.6,
    }
}

/// Heat a tank can hold before its beam overheats: hull plus turret.
pub fn heat_capacity(hull: HullKind, turret_mass: f32) -> f32 {
    thermal_mass(hull) + turret_mass
}

/// How close to the ground under it a tank that went over the edge has to
/// be to count as having hit the floor, m (a hover hull rides ~1 m up).
const LANDED_CLEARANCE: f32 = 2.5;

/// A tank past the fall line has hit bottom: it's down near the ground under
/// it, or sitting still on something (a rock, another wreck). Not while it's
/// still dropping past the cliff face, so the fall plays out to the floor.
fn landed(map: &Map, t: &Tank) -> bool {
    let clearance = t.pos.y - map.ground_height(t.pos.x, t.pos.z);
    clearance < LANDED_CLEARANCE || (t.grounded() && t.vel.y > -1.0)
}

/// A beam's damage multiplier at distance `d` out of `range`.
pub fn beam_falloff(d: f32, range: f32) -> f32 {
    let x = ((d / range - BEAM_FULL) / (1.0 - BEAM_FULL)).clamp(0.0, 1.0);
    1.0 + (BEAM_FAR - 1.0) * x
}

pub fn max_health(kind: HullKind) -> f32 {
    match kind {
        HullKind::Kataphrakt => 110.0,
        HullKind::Kastron => 160.0,
        // Light: speed is its armor.
        HullKind::Sethuun => 90.0,
        // Light: the crystal is its armor (it keeps running once broken).
        HullKind::Vigil => 85.0,
        // Heavy: the most armor, and the most thermal mass.
        HullKind::Gravis => 210.0,
    }
}

pub struct Fighter {
    /// Side 0 (the player's) or 1.
    pub team: usize,
    pub health: f32,
    pub alive: bool,
    /// Seconds until respawn while dead.
    pub respawn_in: f32,
    /// Seconds of spawn protection left.
    pub shield: f32,
    /// Seconds since this tank last spawned or respawned (drives its
    /// side's arrival, which is only looks: see `arrival`).
    pub arrived: f32,
    /// Seconds until the gun can fire again.
    pub reload_in: f32,
    pub kills: u32,
    pub deaths: u32,
    /// Who killed this tank last (for the death screen).
    pub killed_by: Option<usize>,
    /// Seconds since this tank last took a hit, and who hit it.
    pub since_hit: f32,
    pub hit_by: Option<usize>,
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
    /// What that beam is burning (for the sound of it).
    pub burning: Burn,
    /// Part upgrade levels (bots stay at M0), and new levels to fit at the next respawn.
    pub levels: Levels,
    pub next_levels: Option<Levels>,
    /// Hull points of damage this tank has dealt to others this round.
    pub damage_dealt: f32,
    /// Seconds a broken Vigil has left to run; 0 while whole. A husk is
    /// already counted dead (the kill is paid) and can't be hurt.
    pub husk: f32,
}

impl Fighter {
    pub fn max_health(&self, t: &Tank) -> f32 {
        self.levels.max_health(t.hull_kind)
    }

    /// Running on, broken (Vigil).
    pub fn is_husk(&self) -> bool {
        self.husk > 0.0
    }

    /// How much of its speed and firepower a tank has: 1, or a husk's
    /// strength fading out to 0.
    pub fn strength(&self) -> f32 {
        if self.is_husk() {
            (self.husk / HUSK_FADE).min(1.0)
        } else {
            1.0
        }
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
    /// Random 0..1 per shot, for visual variety.
    pub seed: f32,
    /// Tanks a flame puff has already poured through (and burnt, if they
    /// were enemies): it passes them by from then on.
    pub passed: Vec<usize>,
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
    /// Plasma bursting, `size` m across (Ithessar shots).
    Plasma(f32),
    /// Crystal light bursting out of a Vigil as it breaks, `size` m across.
    Shatter(f32),
    /// A mote of crystal light leaking off a husk, drifting up.
    Mote,
}

/// Damage an Ithessar orb keeps through each skip.
const BOUNCE_KEEP: f32 = 0.85;

/// The burst a shot makes where it lands (on a tank: `hit`).
fn impact_kind(spec: &ShellSpec, hit: bool) -> BlastKind {
    match spec.look {
        Look::Plasma { size } => BlastKind::Plasma(size * if hit { 1.6 } else { 1.0 }),
        Look::Bolt { size } => BlastKind::Plasma(size * if hit { 0.9 } else { 0.6 }),
        _ if hit => BlastKind::Hit,
        _ => BlastKind::Impact,
    }
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
            BlastKind::Plasma(size) | BlastKind::Shatter(size) => 0.25 + size * 0.15,
            BlastKind::Mote => 0.9,
        }
    }
}

pub struct Kill {
    pub killer: usize,
    pub victim: usize,
    pub age: f32,
}

/// Reputation the player just earned, for the HUD popup.
pub struct ReputationPop {
    pub amount: u32,
    pub reason: &'static str,
    pub age: f32,
}

/// Something audible that happened this step. The app drains these into the audio.
///
/// A new variant goes next to its family's others (or the event it's most
/// like), not at the end, so sound PRs don't all edit the same lines; see
/// `audio/sounds/mod.rs`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sound {
    Shot {
        tank: usize,
        turret: TurretKind,
        at: Vec3,
    },
    /// A round spent on a wall or the ground.
    Impact {
        at: Vec3,
        plasma: bool,
    },
    /// An orb skipping off a wall.
    Bounce {
        at: Vec3,
    },
    /// A round striking a tank.
    Hit {
        at: Vec3,
        victim: usize,
    },
    Kill {
        at: Vec3,
    },
    Repair {
        at: Vec3,
    },
    /// A repair crate released high over its spot, its canopy opening.
    RepairDrop {
        at: Vec3,
    },
    /// A repair crate touched down, ready to pick up.
    RepairLanded {
        at: Vec3,
    },
    /// A beam locked out hot.
    Overheat {
        tank: usize,
        at: Vec3,
    },
    /// An overheated beam has cooled right down: it can fire again.
    BeamReady {
        tank: usize,
        at: Vec3,
    },
    /// The player's shot made a kill.
    Confirm,
    /// The player's link came back in a fresh chassis.
    Link,
    /// The Hold site changed hands, or paid you for holding it.
    Hold(hold::Cue),
    /// The round starting, its clock running out, and how it ended.
    Round(RoundCue),
    /// A Vigil's crystal shell cracking open: glass, a rush of light, and
    /// its bell struck hard.
    Shatter {
        at: Vec3,
    },
    /// A husk's cracked bell tolling as it runs on, `strength` 1..0.
    Toll {
        at: Vec3,
        strength: f32,
    },
    /// A husk going out: the bell chokes and the crystal falls in.
    Gutter {
        at: Vec3,
    },
    /// A tank's spawn shield ran out: it can be hurt now.
    ShieldDown {
        tank: usize,
        at: Vec3,
    },
    /// A tank ran into a wall or another tank, came down hard, or went over.
    Knock(crate::sim::Knock),
    /// A tank arriving (spawn or respawn), its team's way.
    Arrival {
        tank: usize,
        faction: Faction,
        at: Vec3,
    },
}

/// A moment in the round's flow the player hears, wherever they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundCue {
    /// The first step of a round.
    Start,
    /// The clock passing a whole second in the last `COUNTDOWN`: this many left.
    Count(u8),
    /// The round is over (clock out, or the Hold target reached).
    End,
    /// How it went for the player's side, heard with `End`.
    Won,
    Draw,
    Lost,
}

/// The clock ticks aloud for this many final seconds.
pub const COUNTDOWN: u8 = 10;

/// What a beam's far end is touching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Burn {
    /// Nothing within range: it fades out in the air.
    Air,
    /// A wall, the ground, a crate.
    Surface,
    /// This tank's hull.
    Tank(usize),
}

/// Most sound events kept if nobody drains them (headless runs, tests).
const MAX_SOUNDS: usize = 256;

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
    /// Reputation the player has earned with their side (`factions[0]`) this match.
    pub reputation: u32,
    pub reputation_pops: Vec<ReputationPop>,
    /// Audible events since the app last took them.
    pub sounds: Vec<Sound>,
    pub repairs: Repairs,
    /// Side 0 (the player's) and the rest: two on most maps, more in a
    /// free-for-all.
    pub factions: Vec<Faction>,
    pub time_left: f32,
    pub phase: Phase,
    pub contract: Contract,
    /// The site, under the Hold contract.
    pub hold: Option<Hold>,
    /// Seconds since the round ended (the results screen counts up with it).
    pub over_for: f32,
    nav: NavGrid,
    rng: Rng,
    /// Blowing sand, drifting dust, trickles, track dust (visual only).
    pub ambience: crate::ambience::Ambience,
}

impl Match {
    /// A fresh round on `map` with `bots` bots, dealt round the sides
    /// starting after the player's (so filling every side but one short by
    /// the player makes even teams). The player
    /// starts at their side's base, or at spawn point `spawn` on maps without
    /// bases.
    pub fn new(map: Map, spawn: usize, bots: usize, seed: u64) -> Match {
        let nav = NavGrid::build(&map);
        let repairs = Repairs::new(&map);
        let factions = map.factions.clone();
        let mut world = World::new(map, spawn);
        let mut rng = Rng::new(seed ^ 0xD3A7);
        world.tanks[0].color = TANK_PAINT;
        if !world.map.bases[0].is_empty() {
            let (at, yaw) = spawn_point(&world, &mut rng, 0, &[]);
            world.teleport(0, at, yaw);
        }
        let mut fighters = vec![fighter(0, max_health(world.tanks[0].hull_kind))];
        let mut brains = vec![None];
        for b in 0..bots {
            let team = (b + 1) % factions.len();
            // Each side drives its own faction's hulls, too.
            let hulls = factions[team].hulls();
            let hull = hulls[(rng.f32() * hulls.len() as f32) as usize % hulls.len()];
            // Each side fields its own faction's guns.
            let guns = factions[team].turrets();
            let turret = guns[rng.index(guns.len())];
            let placed: Vec<(usize, Vec2)> = (0..world.tanks.len())
                .map(|j| (fighters[j].team, flat(world.tanks[j].pos)))
                .collect();
            let (at, yaw) = spawn_point(&world, &mut rng, team, &placed);
            world.spawn_tank(TANK_PAINT, at, yaw, hull, turret);
            fighters.push(fighter(team, max_health(hull)));
            brains.push(Some(Bot::new(seed.wrapping_add(b as u64 * 7919))));
        }
        let ambience = crate::ambience::Ambience::new(&world.map);
        Match {
            ambience,
            world,
            fighters,
            bots: brains,
            shells: Vec::new(),
            blasts: Vec::new(),
            feed: Vec::new(),
            reputation: 0,
            reputation_pops: Vec::new(),
            sounds: Vec::new(),
            repairs,
            factions,
            time_left: ROUND_TIME,
            contract: Contract::Attrition,
            hold: None,
            phase: Phase::Playing,
            over_for: 0.0,
            nav,
            rng,
        }
    }

    pub fn player(&self) -> &Fighter {
        &self.fighters[0]
    }

    /// Switch to `contract` (before the round starts).
    pub fn set_contract(&mut self, contract: Contract) {
        self.contract = contract;
        self.hold =
            (contract == Contract::Hold).then(|| Hold::new(&self.world.map, self.fighters.len()));
    }

    /// Side `team`'s score: kills under Attrition, whole seconds held under Hold.
    pub fn score(&self, team: usize) -> u32 {
        match &self.hold {
            Some(h) => h.held[team] as u32,
            None => self
                .fighters
                .iter()
                .filter(|f| f.team == team)
                .map(|f| f.kills)
                .sum(),
        }
    }

    /// Seconds tank `i` has spent on the site while its side held it (0
    /// outside the Hold contract).
    pub fn held(&self, i: usize) -> f32 {
        self.hold.as_ref().map_or(0.0, |h| h.own[i])
    }

    /// The side ahead on score, or None while the lead is shared.
    pub fn leader(&self) -> Option<usize> {
        let scores: Vec<u32> = (0..self.factions.len()).map(|t| self.score(t)).collect();
        let best = *scores.iter().max()?;
        let mut top = (0..scores.len()).filter(|&t| scores[t] == best);
        let first = top.next()?;
        top.next().is_none().then_some(first)
    }

    /// Fighter indices by standing: most time on the site (under Hold), then
    /// most kills, then fewest deaths.
    pub fn standings(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.fighters.len()).collect();
        order.sort_by_key(|&i| {
            (
                std::cmp::Reverse(self.held(i) as u32),
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
        let frac = f.health / f.levels.max_health(self.world.tanks[0].hull_kind);
        self.world.set_hull(0, kind);
        f.health = frac * f.levels.max_health(kind);
    }

    /// Set the player's upgrade levels now, at full health (before a round starts).
    pub fn set_player_levels(&mut self, levels: Levels) {
        let f = &mut self.fighters[0];
        f.levels = levels;
        f.health = levels.max_health(self.world.tanks[0].hull_kind);
    }

    /// One fixed step. `player` drives tank 0; `fire` is the trigger held.
    pub fn step(&mut self, player: Command, fire: bool) {
        if self.sounds.len() > MAX_SOUNDS {
            self.sounds.drain(..self.sounds.len() - MAX_SOUNDS / 2);
        }
        if self.phase == Phase::Playing {
            if self.time_left == ROUND_TIME {
                self.sounds.push(Sound::Round(RoundCue::Start));
            }
            let before = self.time_left;
            self.time_left -= DT;
            // A tick each time the clock (which shows whole seconds, rounded
            // up) turns over to 10, 9 ... 1.
            let left = self.time_left.ceil();
            if left < before.ceil() && left <= COUNTDOWN as f32 && self.time_left > 0.0 {
                self.sounds.push(Sound::Round(RoundCue::Count(left as u8)));
            }
            if self.time_left <= 0.0 {
                self.time_left = 0.0;
                self.end_round();
            }
        } else {
            self.over_for += DT;
        }
        let playing = self.phase == Phase::Playing;

        let contacts: Vec<Contact> = self
            .fighters
            .iter()
            .zip(&self.world.tanks)
            .map(|(f, t)| Contact {
                targetable: f.alive && f.shield <= 0.0 && !f.is_husk(),
                team: f.team,
                health: f.health / f.max_health(t),
            })
            .collect();
        let objective = self.hold.as_ref().map(|h| (h.at, hold::RADIUS));
        let mut cmds = Vec::with_capacity(self.fighters.len());
        let mut triggers = Vec::with_capacity(self.fighters.len());
        for i in 0..self.fighters.len() {
            let (c, f) = match &mut self.bots[i] {
                _ if !self.fighters[i].alive || !playing => (Command::default(), false),
                Some(bot) => bot.think(i, &self.world, &contacts, objective, &self.nav, DT),
                None => (player, fire),
            };
            // A husk's drive fades with it.
            let s = self.fighters[i].strength();
            cmds.push(Command {
                throttle: c.throttle * s,
                steer: c.steer * s.sqrt(),
                ..c
            });
            triggers.push(f);
        }

        self.world.step(&cmds);
        self.sounds
            .extend(self.world.knocks.iter().map(|&k| Sound::Knock(k)));
        self.falls();
        self.husks();
        self.aim_assist();

        for (i, &trigger) in triggers.iter().enumerate() {
            let f = &mut self.fighters[i];
            f.reload_in -= DT;
            let shielded = f.shield > 0.0;
            f.shield -= DT;
            if shielded && f.shield <= 0.0 && f.alive {
                self.sounds.push(Sound::ShieldDown {
                    tank: i,
                    at: self.world.tanks[i].pos,
                });
            }
            if f.alive && f.arrived == 0.0 {
                self.sounds.push(Sound::Arrival {
                    tank: i,
                    faction: self.factions[f.team],
                    at: self.world.tanks[i].pos,
                });
            }
            f.arrived += DT;
            f.since_hit += DT;
            f.since_dealt += DT;
            f.since_repair += DT;
            f.beam = None;
            f.burning = Burn::Air;
            match gun(self.world.tanks[i].turret_kind).action {
                Action::Beam {
                    range,
                    thermal_mass,
                } => {
                    let capacity = heat_capacity(self.world.tanks[i].hull_kind, thermal_mass);
                    if trigger && f.alive && !f.overheated {
                        f.heat += DT / capacity;
                        if f.heat >= 1.0 {
                            f.heat = 1.0;
                            f.overheated = true;
                            self.sounds.push(Sound::Overheat {
                                tank: i,
                                at: muzzle(&self.world.tanks[i]),
                            });
                        }
                        self.beam(i, range);
                    } else {
                        f.heat = (f.heat - COOL_RATE * DT / capacity).max(0.0);
                        if f.heat == 0.0 && f.overheated {
                            f.overheated = false;
                            if f.alive {
                                self.sounds.push(Sound::BeamReady {
                                    tank: i,
                                    at: muzzle(&self.world.tanks[i]),
                                });
                            }
                        }
                    }
                }
                Action::Shell(_) => {
                    let g = gun(self.world.tanks[i].turret_kind);
                    if g.spool > 0.0 {
                        // Spin up while the trigger is held; run down slower.
                        let t = &mut self.world.tanks[i];
                        t.spool = if trigger && f.alive {
                            (t.spool + DT / g.spool).min(1.0)
                        } else {
                            (t.spool - DT / (g.spool * 1.6)).max(0.0)
                        };
                        t.spin += DT * (1.5 + 26.0 * t.spool);
                    }
                    if trigger && f.alive && f.reload_in <= 0.0 {
                        self.shoot(i);
                    }
                }
            }
        }

        for (t, f) in self.world.tanks.iter_mut().zip(&self.fighters) {
            t.heat = f.heat;
            t.overheated = f.overheated;
            t.beaming = f.beam.is_some();
        }

        self.fly_shells();
        self.respawns();
        let tanks: Vec<(&crate::sim::Tank, bool)> = self
            .world
            .tanks
            .iter()
            .zip(&self.fighters)
            .map(|(t, f)| (t, f.alive))
            .collect();
        self.ambience.step(&self.world.map, self.world.time, &tanks);
        if playing {
            self.pick_up_repairs();
            self.hold_site();
        }

        for b in &mut self.blasts {
            b.age += DT;
        }
        self.blasts.retain(|b| b.age < b.life());
        for k in &mut self.feed {
            k.age += DT;
        }
        self.feed.retain(|k| k.age < FEED_TIME);
        for x in &mut self.reputation_pops {
            x.age += DT;
        }
        self.reputation_pops.retain(|x| x.age < REPUTATION_POP_TIME);
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
                if j == i
                    || !self.fighters[j].alive
                    || self.fighters[j].is_husk()
                    || self.fighters[j].team == self.fighters[i].team
                {
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
        let g = self.fighters[i].levels.gun(t.turret_kind);
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
        self.sounds.push(Sound::Shot {
            tank: i,
            turret: t.turret_kind,
            at: from,
        });
        let mut shell = Shell {
            pos: from,
            prev: from,
            // An instant shot goes exactly where the gun points.
            vel: dir * spec.speed + if spec.instant { Vec3::ZERO } else { t.vel },
            owner: i,
            damage: g.damage * self.fighters[i].strength(),
            age: 0.0,
            spec,
            bounces: spec.bounces,
            seed: self.rng.f32(),
            passed: Vec::new(),
        };
        if spec.instant {
            // Land it now, then send off a tracer that stops where it landed.
            let reach = spec.speed * spec.life;
            self.travel(&mut shell, dir * reach);
            shell.spec.life = shell.pos.distance(from) / spec.speed;
            shell.pos = from;
            shell.prev = from;
        }
        self.shells.push(shell);
        // Carry the leftover fraction of a step so fast guns keep their exact rate.
        let reload = if g.spool > 0.0 {
            let spool = self.world.tanks[i].spool;
            g.reload / (SPOOL_FLOOR + (1.0 - SPOOL_FLOOR) * spool)
        } else {
            g.reload
        };
        let f = &mut self.fighters[i];
        f.reload_in = f.reload_in.max(-DT) + reload;
        if spec.look != Look::Flame {
            self.world.fire(i);
        } else {
            // A flamethrower pours: each puff pushes back, but no muzzle flash.
            self.world.recoil(i);
        }
        if let Some(b) = &mut self.bots[i] {
            b.fired();
        }
    }

    /// One step of tank `i`'s beam: burn whatever it touches first, harder
    /// up close. It bends onto an enemy just off its line.
    fn beam(&mut self, i: usize, range: f32) {
        let t = &self.world.tanks[i];
        let from = muzzle(t);
        let dir = self.beam_assist(i, from, t.gun_dir(), range);
        let t = &self.world.tanks[i];
        let hit = self.world.physics.cast_ray(
            &Ray::new(from, dir),
            range,
            true,
            QueryFilter::default().exclude_collider(t.collider),
        );
        let (end, victim, d) = match hit {
            Some((c, toi)) => (from + dir * toi, self.world.tank_of(c), toi),
            None => (from + dir * range, None, range),
        };
        self.fighters[i].beam = Some(end);
        self.fighters[i].burning = match (hit, victim) {
            (_, Some(j)) => Burn::Tank(j),
            (Some(_), None) => Burn::Surface,
            (None, None) => Burn::Air,
        };
        let dmg = self.fighters[i]
            .levels
            .gun(self.world.tanks[i].turret_kind)
            .damage
            * self.fighters[i].strength()
            * beam_falloff(d, range)
            * DT;
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

    /// Where tank `i`'s beam goes: `dir`, or straight onto the visible enemy
    /// closest (by angle, within `BEAM_ASSIST`) to it.
    fn beam_assist(&self, i: usize, from: Vec3, dir: Vec3, range: f32) -> Vec3 {
        let mut best: Option<(f32, Vec3)> = None;
        for (j, o) in self.world.tanks.iter().enumerate() {
            let f = &self.fighters[j];
            if j == i || !f.alive || f.is_husk() || f.team == self.fighters[i].team {
                continue;
            }
            let to = o.pos + o.up() * (o.chassis.hull_top * 0.6) - from;
            let d = to.length();
            if !(2.0..=range).contains(&d) {
                continue;
            }
            let off = dir.angle_between(to / d);
            if off > BEAM_ASSIST || best.is_some_and(|(b, _)| off >= b) {
                continue;
            }
            if ai::can_see(&self.world, from, o) {
                best = Some((off, to / d));
            }
        }
        best.map_or(dir, |(_, d)| d)
    }

    fn fly_shells(&mut self) {
        let mut shells = std::mem::take(&mut self.shells);
        shells.retain_mut(|s| {
            s.age += DT;
            // An instant shot's tracer lingers a moment where it landed.
            let fade = if s.spec.instant { TRACER_FADE } else { 0.0 };
            if s.age > s.spec.life + fade {
                return false;
            }
            s.vel.y -= s.spec.gravity * GRAVITY * DT;
            s.prev = s.pos;
            let step = s.vel * DT;
            if s.spec.instant {
                // Already landed: only its tracer is still flying, and it
                // stops where the shot landed.
                let left = (s.spec.life - (s.age - DT)) / DT;
                s.pos += step * left.clamp(0.0, 1.0);
                return true;
            }
            let alive = self.travel(s, step);
            if alive && s.spec.look == Look::Flame {
                self.engulf(s);
            }
            alive
        });
        self.shells = shells;
    }

    /// Move shell `s` by `step`, or until it hits something on the way: a
    /// tank takes the hit, a wall stops it or (if it has bounces left) turns
    /// it. A flame pours on through tanks, burning each enemy once, and only
    /// walls stop it. False once it's spent.
    fn travel(&mut self, s: &mut Shell, mut step: Vec3) -> bool {
        loop {
            let len = step.length();
            if len < 1e-6 {
                return true;
            }
            let owner = self.world.tanks[s.owner].collider;
            let passed: Vec<_> = s
                .passed
                .iter()
                .filter_map(|&j| self.world.tanks.get(j).map(|t| t.collider))
                .collect();
            let skip = |c: ColliderHandle, _: &Collider| !passed.contains(&c);
            let hit = self.world.physics.cast_ray_and_get_normal(
                &Ray::new(s.pos, step / len),
                len,
                true,
                QueryFilter::default()
                    .exclude_collider(owner)
                    .predicate(&skip),
            );
            let Some((c, ri)) = hit else {
                s.pos += step;
                return true;
            };
            let at = s.pos + step / len * ri.time_of_impact;
            match self.world.tank_of(c) {
                Some(j) if s.spec.look == Look::Flame => {
                    self.hit(s, j, at);
                    s.passed.push(j);
                    // Carry on from where it touched, with the rest of the step.
                    step -= at - s.pos;
                    s.pos = at;
                }
                _ => return self.land(s, c, at, ri.normal),
            }
        }
    }

    /// Flame puff `s` burns every tank inside the fire it shows (not just
    /// those its centre passes through), each once, unless a wall stands
    /// between its centre and the tank.
    fn engulf(&mut self, s: &mut Shell) {
        let reach = flame_reach(s);
        let colliders: Vec<_> = self.world.tanks.iter().map(|t| t.collider).collect();
        for (j, &c) in colliders.iter().enumerate() {
            if j == s.owner || s.passed.contains(&j) {
                continue;
            }
            let Some(col) = self.world.physics.colliders.get(c) else {
                continue;
            };
            let near = col.shape().project_point(col.position(), s.pos, true).point;
            let to = near - s.pos;
            let d = to.length();
            if d > reach {
                continue;
            }
            // Walls (and the ground) still stop it: only tanks are see-through.
            let tank = |c: ColliderHandle, _: &Collider| !colliders.contains(&c);
            let blocked = d > 1e-3
                && self
                    .world
                    .physics
                    .cast_ray(
                        &Ray::new(s.pos, to / d),
                        d,
                        true,
                        QueryFilter::default().predicate(&tank),
                    )
                    .is_some();
            if blocked {
                continue;
            }
            self.hit(s, j, near);
            s.passed.push(j);
        }
    }

    /// Shell `s` ran into collider `c` at `at` (surface normal `n`).
    fn land(&mut self, s: &mut Shell, c: ColliderHandle, at: Vec3, n: Vec3) -> bool {
        s.pos = at;
        match self.world.tank_of(c) {
            Some(j) => {
                self.hit(s, j, at);
                false
            }
            None if s.bounces > 0 => {
                // Ricochet: mirror off the surface and carry on.
                s.vel -= 2.0 * s.vel.dot(n) * n;
                s.pos = at + n * 0.05;
                s.bounces -= 1;
                // The skin sheds plasma at every skip.
                s.damage *= BOUNCE_KEEP;
                self.blasts
                    .push(Blast::new(at, impact_kind(&s.spec, false)));
                self.sounds.push(Sound::Bounce { at });
                true
            }
            None => {
                if s.spec.look != Look::Flame {
                    self.blasts
                        .push(Blast::new(at, impact_kind(&s.spec, false)));
                    self.sounds.push(Sound::Impact {
                        at,
                        plasma: matches!(s.spec.look, Look::Plasma { .. } | Look::Bolt { .. }),
                    });
                }
                false
            }
        }
    }

    fn hit(&mut self, s: &Shell, j: usize, at: Vec3) {
        // No friendly fire: an ally just stops the shot (a flame pours on
        // past it unharmed), though the shot still shoves it.
        if self.fighters[j].team == self.fighters[s.owner].team {
            if s.spec.look != Look::Flame {
                self.blasts
                    .push(Blast::new(at, impact_kind(&s.spec, false)));
                self.sounds.push(Sound::Hit { at, victim: j });
                self.knock(s, j, at);
            }
            return;
        }
        self.knock(s, j, at);
        if s.spec.look != Look::Flame {
            self.blasts.push(Blast::new(at, impact_kind(&s.spec, true)));
            self.sounds.push(if self.fighters[j].alive {
                Sound::Hit { at, victim: j }
            } else {
                Sound::Impact {
                    at,
                    plasma: matches!(s.spec.look, Look::Plasma { .. } | Look::Bolt { .. }),
                }
            });
        }
        self.damage(s.owner, j, s.damage);
    }

    /// Shot `s` landing on tank `j` at `at` hands it the momentum its gun
    /// kicked back with (less what it shed skipping off walls), along its
    /// flight. It pushes where it lands, so a hit off-centre slews the hull
    /// round, but no higher than the gun: a broadside rocks a tank, never
    /// rolls it.
    fn knock(&mut self, s: &Shell, j: usize, at: Vec3) {
        let kind = self.world.tanks[s.owner].turret_kind;
        let skips = s.spec.bounces.saturating_sub(s.bounces) as i32;
        let momentum = kind.recoil() * BOUNCE_KEEP.powi(skips);
        let t = &self.world.tanks[j];
        let up = t.up();
        let over = (at - t.pos).dot(up);
        let at = at - up * (over - over.clamp(0.0, t.gun_height()));
        self.world.push(j, s.vel.normalize_or_zero() * momentum, at);
    }

    /// `owner` deals `amount` to tank `j` (spawn shields and the round's end permitting).
    fn damage(&mut self, owner: usize, j: usize, amount: f32) {
        // No friendly fire (beams land here directly).
        if owner != j && self.fighters[owner].team == self.fighters[j].team {
            return;
        }
        let v = &mut self.fighters[j];
        if !v.alive || v.shield > 0.0 || v.is_husk() || self.phase != Phase::Playing {
            return;
        }
        let dealt = amount.min(v.health);
        v.health -= amount;
        v.since_hit = 0.0;
        if owner != j {
            v.hit_by = Some(owner);
        }
        let o = &mut self.fighters[owner];
        o.since_dealt = 0.0;
        if owner != j {
            o.damage_dealt += dealt;
        }
        if self.fighters[j].health <= 0.0 {
            self.kill(owner, j);
        }
    }

    /// `victim` is destroyed: the kill is paid now. A Vigil doesn't go down
    /// yet; it runs on as a husk and is wrecked when that runs out.
    fn kill(&mut self, killer: usize, victim: usize) {
        let v = &mut self.fighters[victim];
        v.health = 0.0;
        v.deaths += 1;
        v.killed_by = Some(killer);
        if killer != victim {
            self.fighters[killer].kills += 1;
            if killer == 0 {
                self.award(KILL_REPUTATION, "KILL");
                self.sounds.push(Sound::Confirm);
            }
        }
        self.feed.push(Kill {
            killer,
            victim,
            age: 0.0,
        });
        let t = &self.world.tanks[victim];
        if t.hull_kind == HullKind::Vigil {
            // The crystal won't let it stop: its shell cracks and the light
            // pours out, but it keeps going.
            let at = t.pos + t.up() * 1.3;
            self.fighters[victim].husk = HUSK_STRONG + HUSK_FADE;
            self.world.tanks[victim].husk = Some(1.0);
            self.blasts.push(Blast::new(at, BlastKind::Shatter(3.0)));
            self.sounds.push(Sound::Shatter { at });
            return;
        }
        self.wreck(victim);
    }

    /// `victim` goes down for good: a wreck until it respawns.
    fn wreck(&mut self, victim: usize) {
        let v = &mut self.fighters[victim];
        v.alive = false;
        v.husk = 0.0;
        v.respawn_in = RESPAWN_TIME;
        self.world.tanks[victim].dead = true;
        self.world.tanks[victim].husk = None;
        let t = &self.world.tanks[victim];
        let (pos, mass) = (t.pos, t.chassis.mass);
        if t.hull_kind == HullKind::Vigil {
            // The last of the light goes out.
            let at = pos + t.up() * 1.3;
            self.blasts.push(Blast::new(at, BlastKind::Shatter(1.5)));
            self.sounds.push(Sound::Gutter { at });
        } else {
            self.blasts
                .push(Blast::new(pos + Vec3::Y * 1.2, BlastKind::Kill));
            self.sounds.push(Sound::Kill {
                at: pos + Vec3::Y * 1.2,
            });
        }
        if t.chassis.hover {
            // The field lets go: its plasma bursts out of the split spine.
            self.blasts
                .push(Blast::new(pos + t.up() * 1.3, BlastKind::Plasma(3.5)));
        }
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

    /// Over the edge (a mesa map): dead on impact, once it hits the floor
    /// (`landed`), not as it drops past the fall line. Whoever hit it last in
    /// the last few seconds knocked it off and gets the kill.
    fn falls(&mut self) {
        let Some(fall_y) = self.world.map.fall_y else {
            return;
        };
        for i in 0..self.fighters.len() {
            let f = &self.fighters[i];
            let t = &self.world.tanks[i];
            if !f.alive || f.is_husk() || t.pos.y > fall_y || !landed(&self.world.map, t) {
                continue;
            }
            let killer = f.hit_by.filter(|_| f.since_hit < FALL_CREDIT).unwrap_or(i);
            self.kill(killer, i);
        }
    }

    /// Run down every husk's time: light leaks out of its cracks, and when
    /// the time is up it's wrecked.
    fn husks(&mut self) {
        for i in 0..self.fighters.len() {
            if !self.fighters[i].is_husk() {
                continue;
            }
            let before = self.fighters[i].husk;
            self.fighters[i].husk -= DT;
            let s = self.fighters[i].strength();
            self.world.tanks[i].husk = Some(s);
            // Its bell tolls as it goes, each a little weaker; the last is
            // the one it goes out on (`wreck`).
            let run = HUSK_STRONG + HUSK_FADE;
            let (was, now) = (run - before, run - self.fighters[i].husk);
            if (was / HUSK_TOLL).floor() != (now / HUSK_TOLL).floor()
                && self.fighters[i].husk > HUSK_TOLL * 0.5
            {
                let t = &self.world.tanks[i];
                self.sounds.push(Sound::Toll {
                    at: t.pos + t.up() * 1.3,
                    strength: s,
                });
            }
            // Motes of light leak out, fewer as it fades.
            let every = 0.06 / s.max(0.25);
            if (before / every).floor() != (self.fighters[i].husk / every).floor() {
                let t = &self.world.tanks[i];
                let off = t.rot
                    * Vec3::new(
                        self.rng.range(-1.4, 1.4),
                        self.rng.range(0.6, 1.6),
                        self.rng.range(-2.0, 2.0),
                    );
                let drift = off.normalize_or_zero() * self.rng.range(0.5, 1.5)
                    + Vec3::Y * self.rng.range(1.5, 3.5);
                self.blasts.push(Blast {
                    drift,
                    ..Blast::new(t.pos + off, BlastKind::Mote)
                });
            }
            if self.fighters[i].husk <= 0.0 {
                self.wreck(i);
            }
        }
    }

    fn award(&mut self, amount: u32, reason: &'static str) {
        self.reputation += amount;
        self.reputation_pops.push(ReputationPop {
            amount,
            reason,
            age: 0.0,
        });
    }

    /// Bank time for the side holding the site; pay the player reputation for their
    /// own time on it, and end the round when a side reaches the target.
    fn hold_site(&mut self) {
        let Some(h) = &mut self.hold else { return };
        let fighters = &self.fighters;
        let before = h.own[0];
        h.step(DT, &self.world.tanks, |i| {
            fighters[i].alive.then_some(fighters[i].team)
        });
        let paid =
            (h.own[0] / hold::REPUTATION_EVERY).floor() > (before / hold::REPUTATION_EVERY).floor();
        let won = h.winner().is_some();
        if let Some(cue) = h.cue() {
            self.sounds.push(Sound::Hold(cue));
        }
        if paid {
            self.sounds.push(Sound::Hold(hold::Cue::Paid));
            self.award(hold::HOLD_REPUTATION, "HOLDING");
        }
        if won {
            self.end_round();
        }
    }

    /// The round is over: stop the fight and say how it went.
    fn end_round(&mut self) {
        self.phase = Phase::Over;
        self.shells.clear();
        self.sounds.push(Sound::Round(RoundCue::End));
        self.sounds.push(Sound::Round(
            match crate::economy::Payout::of(self).outcome {
                crate::economy::Outcome::Won => RoundCue::Won,
                crate::economy::Outcome::Draw => RoundCue::Draw,
                crate::economy::Outcome::Lost => RoundCue::Lost,
            },
        ));
    }

    fn pick_up_repairs(&mut self) {
        let (fighters, tanks) = (&self.fighters, &self.world.tanks);
        let picked = self.repairs.step(DT, tanks, |i| {
            fighters[i].alive
                && !fighters[i].is_husk()
                && fighters[i].health < fighters[i].max_health(&tanks[i])
        });
        for (i, at) in picked {
            let max = self.fighters[i].max_health(&self.world.tanks[i]);
            let f = &mut self.fighters[i];
            let before = f.health;
            f.health = (f.health + max * crate::repair::HEAL_FRACTION).min(max);
            f.repaired = f.health - before;
            f.since_repair = 0.0;
            self.blasts.push(Blast::new(at, BlastKind::Repair));
            self.sounds.push(Sound::Repair { at });
        }
        for (m, at) in self.repairs.moments.drain(..) {
            self.sounds.push(match m {
                crate::repair::Moment::Released => Sound::RepairDrop { at },
                crate::repair::Moment::Landed => Sound::RepairLanded { at },
            });
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
            if let Some(levels) = self.fighters[i].next_levels.take() {
                self.fighters[i].levels = levels;
            }
            let living: Vec<(usize, Vec2)> = (0..self.fighters.len())
                .filter(|&j| j != i && self.fighters[j].alive)
                .map(|j| (self.fighters[j].team, flat(self.world.tanks[j].pos)))
                .collect();
            let team = self.fighters[i].team;
            let (at, yaw) = spawn_point(&self.world, &mut self.rng, team, &living);
            self.world.teleport(i, at, yaw);
            self.world.tanks[i].dead = false;
            let f = &mut self.fighters[i];
            f.alive = true;
            f.health = f.levels.max_health(self.world.tanks[i].hull_kind);
            f.shield = SPAWN_SHIELD;
            f.arrived = 0.0;
            f.hit_by = None;
            f.reload_in = 0.5;
            f.heat = 0.0;
            f.overheated = false;
            if i == 0 {
                self.sounds.push(Sound::Link);
            }
        }
    }

    /// The arrivals playing: tanks that spawned or respawned in the last
    /// `ARRIVAL_TIME`, each arriving its team's way.
    fn arrivals(&self) -> impl Iterator<Item = (usize, Arrival<'_>)> {
        self.fighters
            .iter()
            .enumerate()
            .filter(|(_, f)| f.alive && f.arrived < ARRIVAL_TIME)
            .map(|(i, f)| {
                let tank = &self.world.tanks[i];
                let arrival = Arrival {
                    faction: self.factions[f.team],
                    t: f.arrived,
                    tank,
                    ground: self.world.map.ground_height(tank.pos.x, tank.pos.z),
                    seed: (i as f32 * 0.618_034).fract(),
                };
                (i, arrival)
            })
    }

    /// Tank `i` interpolated, where it really is: what the camera follows
    /// (an arrival can draw the tank elsewhere for a moment).
    pub fn camera_tank(&self, i: usize, alpha: f32) -> Tank {
        self.world.tanks[i].interpolated(alpha)
    }

    /// Where a shot from `t` would land if it fired now, for a gun that
    /// lands instantly (else None): the first thing along its line, or the
    /// end of its reach.
    pub fn instant_landing(&self, t: &Tank) -> Option<Vec3> {
        let Action::Shell(spec) = gun(t.turret_kind).action else {
            return None;
        };
        if !spec.instant {
            return None;
        }
        let (from, dir) = (muzzle(t), t.gun_dir());
        let reach = spec.speed * spec.life;
        let toi = self
            .world
            .physics
            .cast_ray(
                &Ray::new(from, dir),
                reach,
                true,
                QueryFilter::default().exclude_collider(t.collider),
            )
            .map_or(reach, |(_, toi)| toi);
        Some(from + dir * toi)
    }

    /// The tanks to draw this frame: interpolated, with wrecks charred and
    /// arrivals still bringing tanks in.
    pub fn display_tanks(&self, alpha: f32) -> Vec<Tank> {
        let mut tanks: Vec<Tank> = self
            .world
            .tanks
            .iter()
            .zip(&self.fighters)
            .map(|(t, f)| {
                let mut t = t.interpolated(alpha);
                if !f.alive {
                    t.color *= 0.12;
                    t.since_fire = 10.0;
                } else if f.is_husk() {
                    // Cracked and dark, lit only by what leaks out of it.
                    t.color *= 0.3;
                }
                t
            })
            .collect();
        for (i, a) in self.arrivals() {
            a.pose(&mut tanks[i]);
        }
        tanks
    }

    /// Plasma's light: soft additive glow for shells in flight and the
    /// bursts they leave, drawn in the particle pass.
    pub fn glow_particles(&self, alpha: f32, out: &mut Vec<Particle>) {
        let glow = Particle::glow;
        for c in &self.world.map.cores {
            core_glow(c, self.world.time, out);
        }
        for (_, a) in self.arrivals() {
            a.glow(out);
        }
        for s in &self.shells {
            let p = s.prev.lerp(s.pos, alpha);
            let d = s.vel.normalize_or_zero();
            // Fades in over the first few hundredths so it leaves the muzzle
            // rather than appearing in front of it.
            let born = (s.age / 0.04).min(1.0);
            match s.spec.look {
                // An orb held in its own skin: a halo that shivers, a white
                // heart, a comet tail of shed plasma behind it, and two motes
                // circling. It shrinks a little at every skip.
                Look::Plasma { size } => {
                    let size = size * plasma_shrink(s);
                    let shiver = 1.0 + 0.12 * (s.age * 38.0 + s.seed * 6.0).sin();
                    out.push(glow(p, size * 2.2 * shiver, PLASMA, 1.6 * born, Vec3::ZERO));
                    out.push(glow(p, size * 0.9, PLASMA_HOT, 2.4 * born, Vec3::ZERO));
                    // The tail, stretched along the flight path.
                    out.push(glow(
                        p - d * size * 1.6,
                        size * 0.7,
                        PLASMA,
                        1.1 * born,
                        d * 2.5,
                    ));
                    for k in 1..5 {
                        let k = k as f32;
                        let flick = 0.8 + 0.2 * (s.age * 27.0 + k * 2.3 + s.seed * 9.0).sin();
                        out.push(glow(
                            p - d * size * 0.8 * k,
                            size * (1.1 - 0.18 * k),
                            PLASMA,
                            (1.2 - 0.25 * k) * flick * born,
                            Vec3::ZERO,
                        ));
                    }
                    let side = d.any_orthonormal_vector();
                    for k in 0..2 {
                        let a = s.age * 14.0 + s.seed * 6.0 + k as f32 * std::f32::consts::PI;
                        let off = Quat::from_axis_angle(d, a) * side * size * 0.9;
                        out.push(glow(
                            p + off,
                            size * 0.3,
                            PLASMA_HOT,
                            1.8 * born,
                            Vec3::ZERO,
                        ));
                    }
                }
                // A dart: a long rose streak around a hot head.
                Look::Bolt { size } => {
                    out.push(glow(
                        p - d * size * 1.6,
                        size * 0.38,
                        PLASMA,
                        2.4 * born,
                        d * 6.0,
                    ));
                    out.push(glow(
                        p - d * size * 0.5,
                        size * 0.16,
                        PLASMA_HOT,
                        2.6 * born,
                        d * 5.0,
                    ));
                    out.push(glow(p, size * 0.5, PLASMA, 1.6 * born, Vec3::ZERO));
                }
                _ => {}
            }
        }
        // Where plasma bursts: the skin pops in a flash of light that swells
        // and fades, throwing droplets out in a ring that arc and wink out.
        for b in &self.blasts {
            let BlastKind::Plasma(size) = b.kind else {
                continue;
            };
            let f = b.age / b.life();
            let fade = (1.0 - f) * (1.0 - f);
            out.push(glow(
                b.pos,
                size * (1.6 + 2.4 * f),
                PLASMA,
                2.5 * fade,
                Vec3::ZERO,
            ));
            out.push(glow(
                b.pos,
                size * (0.8 + 0.6 * f),
                PLASMA_HOT,
                3.0 * fade,
                Vec3::ZERO,
            ));
            let seed = (b.pos.x * 12.9898 + b.pos.z * 78.233).sin() * 43758.5;
            let spin = seed.fract() * std::f32::consts::TAU;
            let n = 7;
            for k in 0..n {
                let a = spin + k as f32 / n as f32 * std::f32::consts::TAU;
                let dir = Vec3::new(a.cos(), 0.6 + 0.4 * (a * 3.0).sin(), a.sin()).normalize();
                let travel = size * 2.2 * (1.0 - (1.0 - f).powi(2));
                let at = b.pos + dir * travel - Vec3::Y * f * f * size * 1.5;
                out.push(glow(
                    at,
                    size * 0.18,
                    PLASMA_HOT,
                    2.5 * (1.0 - f),
                    dir * 2.0,
                ));
            }
        }
    }

    /// Shells, impacts, explosions and smoke as render instances.
    pub fn effect_instances(&self, alpha: f32, out: &mut Vec<(MeshId, Instance)>) {
        self.repairs.instances(self.world.time, out);
        for (_, a) in self.arrivals() {
            a.instances(out);
        }
        for c in &self.world.map.cores {
            core_instances(c, self.world.time, out);
        }
        if let Some(h) = &self.hold {
            let paint: Vec<Vec3> = self.factions.iter().map(|f| f.paint()).collect();
            h.instances(self.world.time, &paint, out);
        }
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
                // An instant shot's tracer covers a frame's flight in a long
                // streak, never trailing back past the muzzle; once it's
                // where the shot landed it lingers there, thinning out.
                Look::Tracer if s.spec.instant => {
                    let age = s.age - (1.0 - alpha) * DT;
                    let flown = age.clamp(0.0, s.spec.life) * s.spec.speed;
                    let fade = 1.0 - ((age - s.spec.life) / TRACER_FADE).clamp(0.0, 1.0);
                    out.push(streak(0.2 * fade, flown.min(TRACER_STREAK), tracer));
                }
                // A streak trailing back from the shell.
                Look::Tracer => out.push(streak(0.22, 3.2, tracer)),
                // Plasma is mostly light (see `glow_particles`); only its
                // white-hot heart is solid.
                Look::Plasma { size } => {
                    out.push(ball(
                        p,
                        size * plasma_shrink(s) * 0.26,
                        PLASMA_HOT.extend(6.0),
                    ));
                }
                Look::Bolt { size } => {
                    out.push(streak(0.06 * size, 1.4 * size, PLASMA_HOT.extend(6.0)));
                }
                // Crystal fire: white-hot at the nozzle, swelling into blue, then gone.
                Look::Flame => {
                    // A lumpy cloud of three blobs, tumbling as it swells.
                    let f = s.age / s.spec.life;
                    let size = flame_size(f);
                    let hot = Vec3::new(0.9, 0.97, 1.0)
                        .lerp(Vec3::new(0.25, 0.55, 1.0), (f * 2.0).min(1.0))
                        .lerp(Vec3::new(0.06, 0.15, 0.7), (f * 1.5 - 0.5).clamp(0.0, 1.0));
                    let glow = 1.0 + 3.0 * (1.0 - f);
                    let spin = s.seed * std::f32::consts::TAU + s.age * 9.0;
                    for k in 0..3 {
                        let a = spin + k as f32 * 2.1;
                        let off = Vec3::new(a.cos(), (a * 1.7).sin() * 0.6, a.sin()) * size * 0.3;
                        let r = size * (0.75 + 0.2 * (k as f32 + s.seed * 3.0).sin());
                        out.push(ball(p + off, r, hot.extend(glow)));
                    }
                }
            }
        }
        // Laser beams: a hot core inside a wider green glow, and a burn spot where it lands.
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
                (0.1, Vec4::new(0.75, 1.0, 0.8, 4.5)),
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
                    Vec4::new(0.4, 1.0, 0.5, 4.0),
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
                // Plasma bursts are all light (`glow_particles`).
                BlastKind::Plasma(_) => continue,
                BlastKind::Shatter(size) => {
                    let (cool, hot) = (CRYSTAL, CRYSTAL_HOT);
                    // The skin bursts: a flash that swells and thins out, and
                    // droplets thrown out in a ring that arc and wink out.
                    let seed = (b.pos.x * 12.9898 + b.pos.z * 78.233).sin() * 43758.5;
                    let spin = seed.fract() * std::f32::consts::TAU;
                    let n = 7;
                    for k in 0..n {
                        let a = spin + k as f32 / n as f32 * std::f32::consts::TAU;
                        let out_dir = Vec3::new(a.cos(), 0.6 + 0.4 * (a * 3.0).sin(), a.sin());
                        let travel = size * 2.2 * (1.0 - (1.0 - f).powi(2));
                        let p = b.pos + out_dir * travel - Vec3::Y * f * f * size * 1.5;
                        let q = Quat::from_rotation_arc(Vec3::Y, out_dir.normalize());
                        out.push((
                            MeshId::GEM,
                            Instance::new(
                                Mat4::from_scale_rotation_translation(
                                    Vec3::new(0.12, 0.6, 0.12) * size * (1.0 - f),
                                    q,
                                    p,
                                ),
                                hot.extend(4.0),
                            ),
                        ));
                    }
                    out.push((
                        MeshId::SPHERE,
                        Instance::new(
                            Mat4::from_scale_rotation_translation(
                                Vec3::splat(size * 0.7 * (1.0 - f)),
                                Quat::IDENTITY,
                                b.pos,
                            ),
                            hot.extend(5.0),
                        ),
                    ));
                    (
                        MeshId::SPHERE,
                        size * (0.6 + f * 1.6) * (1.0 - f * f),
                        cool.extend(1.0 + 2.0 * (1.0 - f)),
                    )
                }
                BlastKind::Mote => {
                    let p = b.pos + b.drift * f;
                    let s = 0.18 * (1.0 - f);
                    out.push((
                        MeshId::GEM,
                        Instance::new(
                            Mat4::from_scale_rotation_translation(
                                Vec3::new(s, s * 2.5, s),
                                Quat::from_rotation_y(f * 6.0),
                                p,
                            ),
                            CRYSTAL_HOT.extend(4.0 * (1.0 - f)),
                        ),
                    ));
                    continue;
                }
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

/// An Ithessar core's light: a breathing ball of plasma with a white-hot
/// heart, and motes circling it on tilted orbits.
fn core_glow(c: &crate::map::Core, time: f32, out: &mut Vec<Particle>) {
    let breathe = 1.0 + 0.05 * (time * 1.7).sin() + 0.02 * (time * 5.3).sin();
    let r = c.radius;
    out.push(Particle::glow(
        c.at,
        1.9 * r * breathe,
        PLASMA,
        0.8,
        Vec3::ZERO,
    ));
    out.push(Particle::glow(
        c.at,
        0.9 * r * breathe,
        PLASMA_HOT,
        1.0,
        Vec3::ZERO,
    ));
    for k in 0..9 {
        let k = k as f32;
        let tilt = Quat::from_rotation_x(0.5 + k * 0.7) * Quat::from_rotation_z(k * 1.3);
        let a = time * (0.6 + 0.07 * k) + k * 2.4;
        let p = c.at + tilt * Vec3::new(a.cos(), 0.0, a.sin()) * r * (1.45 + 0.1 * (k % 3.0));
        out.push(Particle::glow(
            p,
            0.35 + 0.1 * (k % 2.0),
            PLASMA_HOT,
            1.3,
            Vec3::ZERO,
        ));
    }
}

/// An Ithessar core: a ball of plasma that breathes, motes circling it on
/// tilted orbits, and arcs jumping to it from the monoliths that hold it.
pub fn core_instances(c: &crate::map::Core, time: f32, out: &mut Vec<(MeshId, Instance)>) {
    let breathe = 1.0 + 0.05 * (time * 1.7).sin() + 0.02 * (time * 5.3).sin();
    let r = c.radius;
    let ball = |at: Vec3, d: f32, color: Vec4| {
        (
            MeshId::SPHERE,
            Instance::new(
                Mat4::from_scale_rotation_translation(Vec3::splat(d), Quat::IDENTITY, at),
                color,
            ),
        )
    };
    // The ball and its motes are light (`core_glow`); only the white heart is solid.
    out.push(ball(c.at, 0.45 * r * breathe, PLASMA_HOT.extend(6.0)));
    for (k, &tip) in c.prongs.iter().enumerate() {
        let beat = (time * 6.0 + k as f32 * 1.9).sin() + (time * 13.0 + k as f32).sin() * 0.5;
        if beat < 0.6 {
            continue;
        }
        let to = c.at - tip;
        let len = to.length() - r * 0.9;
        let dir = to / to.length();
        let q = Quat::from_rotation_arc(Vec3::Y, dir);
        // Two strokes, a little apart: a crooked arc.
        let bend = q * Vec3::X * 0.4 * (time * 31.0 + k as f32).sin();
        for (from, l, w) in [
            (tip, len * 0.55, 0.22),
            (tip + dir * len * 0.5 + bend, len * 0.5, 0.18),
        ] {
            out.push((
                MeshId::GEM,
                Instance::new(
                    Mat4::from_scale_rotation_translation(
                        Vec3::new(w, l, w),
                        q,
                        from + dir * l * 0.5,
                    ),
                    PLASMA_HOT.extend(4.0),
                ),
            ));
        }
    }
}

fn fighter(team: usize, health: f32) -> Fighter {
    Fighter {
        team,
        health,
        alive: true,
        respawn_in: 0.0,
        shield: SPAWN_SHIELD,
        arrived: 0.0,
        reload_in: 0.5,
        kills: 0,
        deaths: 0,
        killed_by: None,
        since_hit: 10.0,
        hit_by: None,
        since_dealt: 10.0,
        next_kit: None,
        since_repair: 10.0,
        repaired: 0.0,
        heat: 0.0,
        overheated: false,
        beam: None,
        burning: Burn::Air,
        levels: Levels::default(),
        next_levels: None,
        damage_dealt: 0.0,
        husk: 0.0,
    }
}

/// Where tank `t`'s shots leave the gun.
pub(crate) fn muzzle(t: &Tank) -> Vec3 {
    t.pos + t.up() * t.gun_height() + t.gun_dir() * 2.6
}

fn flat(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}

/// A spawn spot for side `team`: at its base if the map has bases (else
/// anywhere open), well away from `others` (side, position), with enemies
/// counting as twice as close. Picked at random among the safest few, facing
/// the enemy base (or the map center).
fn spawn_point(world: &World, rng: &mut Rng, team: usize, others: &[(usize, Vec2)]) -> (Vec2, f32) {
    let map = &world.map;
    let spots = match &map.bases[team] {
        base if !base.is_empty() => base,
        _ => &map.open_spots,
    };
    let mut scored: Vec<(f32, Vec2)> = spots
        .iter()
        .map(|&s| {
            let room = others
                .iter()
                .map(|&(side, o)| o.distance(s) * if side == team { 1.0 } else { 0.5 })
                .fold(f32::INFINITY, f32::min);
            (room, s)
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let pick = scored[rng.index(scored.len().min(4))].1;
    // Face the enemy base on a two-sided map; with more sides (or no
    // bases), the middle.
    let enemy_base = match map.bases.len() {
        2 => &map.bases[1 - team],
        _ => &Vec::new(),
    };
    let look = if enemy_base.is_empty() {
        map.center()
    } else {
        enemy_base.iter().sum::<Vec2>() / enemy_base.len() as f32
    };
    let to = look - pick;
    let yaw = if to.length_squared() > 1.0 {
        yaw_of(to)
    } else {
        rng.range(0.0, std::f32::consts::TAU)
    };
    (pick, yaw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::dir2;

    /// A real map with room for a full match.
    fn arena(bots: usize) -> Match {
        Match::new(Map::saltmarket(), 0, bots, 21)
    }

    /// A free-for-all: all four factions, in the Capital.
    fn four_sides(bots: usize) -> Match {
        let map = Map::capital();
        Match::new(map, 0, bots, 21)
    }

    #[test]
    fn four_sides_deal_even_teams_and_need_a_sole_leader() {
        let mut m = four_sides(15);
        for t in 0..4 {
            assert_eq!(m.fighters.iter().filter(|f| f.team == t).count(), 4);
        }
        assert_eq!(m.leader(), None);
        let by_team = |m: &Match, t: usize| {
            (0..m.fighters.len())
                .find(|&i| m.fighters[i].team == t)
                .unwrap()
        };
        let (a, b) = (by_team(&m, 2), by_team(&m, 3));
        m.fighters[a].kills = 3;
        assert_eq!(m.leader(), Some(2));
        m.fighters[b].kills = 3;
        assert_eq!(m.leader(), None, "a shared lead is no leader");
        // The player's side trails the tie: a loss, not a draw.
        assert_eq!(
            crate::economy::Payout::of(&m).outcome,
            crate::economy::Outcome::Lost
        );
    }

    #[test]
    fn spawns_player_and_bots_apart() {
        let m = arena(11);
        assert_eq!(m.world.tanks.len(), 12);
        for i in 0..12 {
            for j in i + 1..12 {
                let d = m.world.tanks[i].pos.distance(m.world.tanks[j].pos);
                assert!(d > 8.0, "tanks {i} and {j} spawned {d:.1} m apart");
            }
        }
    }

    #[test]
    fn shell_hits_and_kills_a_tank_in_front() {
        let mut m = Match::new(Map::yard(200.0), 0, 0, 1);
        // A dummy straight ahead of the player.
        let t = &m.world.tanks[0];
        let ahead = Vec2::new(t.pos.x, t.pos.z) + dir2(t.yaw()) * 25.0;
        m.world.spawn_tank(
            Vec3::ONE,
            ahead,
            0.0,
            HullKind::Kataphrakt,
            TurretKind::Kontos,
        );
        m.fighters
            .push(fighter(1, max_health(HullKind::Kataphrakt)));
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
        // Damage counted as dealt, capped at what the target had left.
        let dealt = m.fighters[0].damage_dealt;
        assert!(
            dealt > 0.0
                && dealt <= (m.fighters[1].deaths + 1) as f32 * max_health(HullKind::Kataphrakt)
        );
        assert!(m.feed.iter().any(|k| k.killer == 0 && k.victim == 1));
        // Paid per kill, on the spot.
        assert_eq!(m.reputation, m.fighters[0].kills * KILL_REPUTATION);
    }

    /// The player with `turret` and a dummy `dist` m straight ahead, shields
    /// down, settled. The dummy never fires (with no bot, it would pull the
    /// player's trigger).
    fn duel(turret: TurretKind, dist: f32) -> Match {
        let mut m = Match::new(Map::yard(200.0), 0, 0, 1);
        m.world.tanks[0].turret_kind = turret;
        let t = &m.world.tanks[0];
        let ahead = Vec2::new(t.pos.x, t.pos.z) + dir2(t.yaw()) * dist;
        m.world.spawn_tank(
            Vec3::ONE,
            ahead,
            0.0,
            HullKind::Kataphrakt,
            TurretKind::Kontos,
        );
        m.fighters
            .push(fighter(1, max_health(HullKind::Kataphrakt)));
        m.bots.push(None);
        for f in &mut m.fighters {
            f.shield = 0.0;
        }
        m.fighters[1].reload_in = f32::INFINITY;
        for _ in 0..60 {
            m.step(Command::default(), false);
        }
        m
    }

    /// Every tank's arrival is heard once, its team's way: all of them at the
    /// round start, and a tank again when it respawns.
    #[test]
    fn arrivals_sound_once_per_spawn() {
        let mut m = four_sides(7);
        let n = m.fighters.len();
        let mut heard = vec![0; n];
        let mut count = |m: &mut Match, heard: &mut Vec<usize>| {
            for s in m.sounds.drain(..) {
                if let Sound::Arrival { tank, faction, .. } = s {
                    assert_eq!(faction, m.factions[m.fighters[tank].team]);
                    heard[tank] += 1;
                }
            }
        };
        for _ in 0..30 {
            m.step(Command::default(), false);
            count(&mut m, &mut heard);
        }
        assert!(heard.iter().all(|&h| h == 1), "{heard:?}");
        for side in Faction::ALL {
            assert!(m.fighters.iter().any(|f| m.factions[f.team] == side));
        }
        m.kill(1, 0);
        for _ in 0..((RESPAWN_TIME + 0.2) / DT) as usize {
            m.step(Command::default(), false);
            count(&mut m, &mut heard);
        }
        assert_eq!(heard[0], 2, "{heard:?}");
    }

    /// Holding the trigger keeps a shell gun firing at its reload rate.
    #[test]
    fn held_trigger_keeps_firing() {
        for turret in [
            TurretKind::Kontos,
            TurretKind::Pelekys,
            TurretKind::Ammeth,
            TurretKind::Ossrin,
            TurretKind::Hymn,
        ] {
            let mut m = duel(turret, 200.0);
            let reload = gun(turret).reload.max(DT);
            let secs = 6.0;
            let mut shots = 0;
            for _ in 0..(secs / DT) as usize {
                m.step(Command::default(), true);
                shots += m
                    .sounds
                    .drain(..)
                    .filter(|s| matches!(s, Sound::Shot { tank: 0, .. }))
                    .count();
            }
            let most = (secs / reload) as usize + 1;
            assert!(
                shots >= 3 && shots <= most,
                "{turret:?}: {shots} shots in {secs}s (reload {reload})"
            );
        }
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

    /// Imperium guns land on the step they fire, however far the target
    /// is, and leave a tracer that stops where they landed.
    #[test]
    fn imperium_shots_land_the_moment_they_fire() {
        for turret in [TurretKind::Kontos, TurretKind::Pelekys] {
            let mut m = duel(turret, 90.0);
            let hp = m.fighters[1].health;
            m.step(Command::default(), true);
            assert!(
                m.fighters[1].health < hp,
                "{turret:?}: a target 90 m off wasn't hit on the shot's own step"
            );
            let mine: Vec<&Shell> = m.shells.iter().filter(|s| s.owner == 0).collect();
            assert_eq!(mine.len(), 1, "{turret:?}: the shot leaves a tracer");
            let s = mine[0];
            let reach = s.spec.speed * s.spec.life;
            assert!(
                (80.0..90.0).contains(&reach),
                "{turret:?}: tracer flies {reach:.1} m, not to the target"
            );
            // The tracer is only a picture: it's gone soon and hurts no one.
            let after = m.fighters[1].health;
            for _ in 0..(0.2 / DT) as usize {
                m.step(Command::default(), false);
            }
            assert!(m.shells.is_empty());
            assert_eq!(m.fighters[1].health, after);
        }
    }

    /// Every action has an equal and opposite reaction: whatever the gun kicks
    /// its own tank back with, the shot hands the tank it lands on. Up in the
    /// air, away from the ground's friction, the two cancel exactly.
    #[test]
    fn a_hit_shoves_the_target_as_hard_as_the_gun_kicks_back() {
        for &k in &TurretKind::ALL {
            if !matches!(gun(k).action, Action::Shell(_)) {
                continue;
            }
            let mut m = duel(k, 10.0);
            for t in &m.world.tanks {
                let b = &mut m.world.physics.bodies[t.body];
                b.set_translation(b.translation() + Vec3::Y * 40.0, true);
                b.set_linvel(Vec3::ZERO, true);
                b.set_angvel(Vec3::ZERO, true);
            }
            let ahead = m.world.tanks[0].turret_dir();
            let flat = |m: &Match, i: usize| {
                let b = &m.world.physics.bodies[m.world.tanks[i].body];
                b.linvel() * Vec3::new(1.0, 0.0, 1.0) * b.mass()
            };
            m.step(Command::default(), true);
            for _ in 0..(0.6 / DT) as usize {
                m.step(Command::default(), false);
            }
            let (kick, shove) = (flat(&m, 0), flat(&m, 1));
            let shot = k.recoil();
            assert!(
                kick.dot(ahead) < -0.95 * shot,
                "{k:?}: kicked back {:.0} N·s, wants {shot:.0}",
                -kick.dot(ahead)
            );
            assert!(
                shove.dot(ahead) > 0.9 * shot,
                "{k:?}: the target took {:.0} N·s of {shot:.0}",
                shove.dot(ahead)
            );
            assert!(
                (kick + shove).length() < 0.1 * shot,
                "{k:?}: momentum not conserved: {kick} vs {shove}"
            );
        }
    }

    /// On the ground, a rail cannon hit is plain to see: the shooter rocks and
    /// slides back, and the target slides away. (Taken low on its back plate,
    /// near its centre of mass, the target rocks less than the shooter, whose
    /// kick comes at gun height.)
    #[test]
    fn a_cannon_hit_visibly_knocks_both_tanks() {
        let mut m = duel(TurretKind::Kontos, 20.0);
        let ahead = m.world.tanks[0].turret_dir();
        let p0 = [m.world.tanks[0].pos, m.world.tanks[1].pos];
        m.step(Command::default(), true);
        let mut tilt = [0.0f32; 2];
        for _ in 0..(2.0 / DT) as usize {
            m.step(Command::default(), false);
            for i in 0..2 {
                tilt[i] = tilt[i].max(m.world.tanks[i].up().angle_between(Vec3::Y).to_degrees());
            }
        }
        let moved = |i: usize| (m.world.tanks[i].pos - p0[i]).dot(ahead);
        assert!(moved(0) < -0.3, "shooter slid back only {:.2} m", -moved(0));
        assert!(moved(1) > 0.3, "target slid away only {:.2} m", moved(1));
        assert!(tilt[0] > 8.0 && tilt[1] > 2.0, "rocked {tilt:?} deg");
        assert!(m.world.tanks[1].up().y > 0.9, "the hit must not roll it");
    }

    /// A broadside from the heaviest gun rocks the lightest tracked hull hard,
    /// but never rolls it.
    #[test]
    fn a_broadside_rocks_but_never_rolls() {
        let mut m = duel(TurretKind::Kontos, 20.0);
        m.world.set_hull(1, HullKind::Vigil);
        // Turn the target side-on.
        let t = &m.world.tanks[1];
        let at = Vec2::new(t.pos.x, t.pos.z);
        m.world
            .teleport(1, at, t.yaw() + std::f32::consts::FRAC_PI_2);
        for _ in 0..60 {
            m.step(Command::default(), false);
        }
        let mut worst = 1.0f32;
        for _ in 0..4 {
            m.step(Command::default(), true);
            for _ in 0..(1.5 / DT) as usize {
                m.step(Command::default(), false);
                worst = worst.min(m.world.tanks[1].up().y);
            }
        }
        assert!(worst > 0.5, "the target tipped to up.y = {worst:.2}");
    }

    /// Nothing to lead with an Imperium gun; everyone else's shells still fly.
    #[test]
    fn only_imperium_guns_are_instant() {
        for &k in &TurretKind::ALL {
            let g = gun(k);
            let instant = matches!(k, TurretKind::Kontos | TurretKind::Pelekys);
            match g.action {
                Action::Shell(_) => {
                    assert_eq!(g.speed().is_infinite(), instant, "{k:?}");
                    if instant {
                        assert_eq!(g.gravity(), 0.0);
                        assert!(g.range() >= 480.0, "{k:?} can't reach across a map");
                    }
                }
                Action::Beam { .. } => assert!(!instant),
            }
        }
    }

    #[test]
    fn flame_falls_short_at_range() {
        let mut m = duel(TurretKind::Hymn, 40.0);
        for _ in 0..(3.0 / DT) as usize {
            m.step(Command::default(), true);
        }
        assert_eq!(m.fighters[1].health, max_health(HullKind::Kataphrakt));
        assert!(!m.shells.is_empty(), "the flamer should be pouring fire");
    }

    /// The flamer pours at a tank 11 m off with something 5 m off in the
    /// way: another tank of team `front` (None: nothing), or a wall. How
    /// much the tank at the back has lost after a second of fire.
    fn flame_through(front: Option<usize>, wall: bool) -> f32 {
        let probe = Match::new(Map::yard(200.0), 0, 0, 1);
        let (pos, yaw) = (probe.world.tanks[0].pos, probe.world.tanks[0].yaw());
        let at = |d: f32| Vec2::new(pos.x, pos.z) + dir2(yaw) * d;
        let mut map = Map::yard(200.0);
        if wall {
            let c = at(5.5);
            map.solids.push(crate::map::Solid {
                min: Vec3::new(c.x - 3.0, 0.0, c.y - 3.0),
                max: Vec3::new(c.x + 3.0, 6.0, c.y + 3.0),
            });
        }
        let mut m = Match::new(map, 0, 0, 1);
        assert_eq!(m.world.tanks[0].pos, pos);
        m.world.tanks[0].turret_kind = TurretKind::Hymn;
        let spawn = |m: &mut Match, d: f32, team: usize| {
            m.world.spawn_tank(
                Vec3::ONE,
                at(d),
                0.0,
                HullKind::Kataphrakt,
                TurretKind::Kontos,
            );
            m.fighters
                .push(fighter(team, max_health(HullKind::Kataphrakt)));
            m.bots.push(None);
        };
        if let Some(team) = front {
            spawn(&mut m, 5.0, team);
        }
        spawn(&mut m, 11.0, 1);
        let back = m.fighters.len() - 1;
        for f in &mut m.fighters {
            f.shield = 0.0;
        }
        for _ in 0..60 {
            m.step(Command::default(), false);
        }
        for _ in 0..(1.0 / DT) as usize {
            m.step(Command::default(), true);
        }
        max_health(HullKind::Kataphrakt) - m.fighters[back].health
    }

    /// How much an enemy `ahead` m in front of the flamer and `side` m off
    /// its line loses to a second of fire, with (if `wall`) a thin wall
    /// standing across the line from `wall` m out.
    fn flame_burn(ahead: f32, side: f32, wall: Option<f32>) -> f32 {
        let probe = Match::new(Map::yard(200.0), 0, 0, 1);
        let t = &probe.world.tanks[0];
        let (pos, yaw, fwd) = (Vec2::new(t.pos.x, t.pos.z), t.yaw(), dir2(t.yaw()));
        let at = |d: f32, x: f32| pos + fwd * d + Vec2::new(-fwd.y, fwd.x) * x;
        let mut map = Map::yard(200.0);
        if let Some(w) = wall {
            // 0.4 m thick and 8 m wide, facing the flamer (which faces down an axis).
            let (a, b) = (at(w, -4.0), at(w + 0.4, 4.0));
            map.solids.push(crate::map::Solid {
                min: Vec3::new(a.x.min(b.x), 0.0, a.y.min(b.y)),
                max: Vec3::new(a.x.max(b.x), 6.0, a.y.max(b.y)),
            });
        }
        let mut m = Match::new(map, 0, 0, 1);
        m.world.tanks[0].turret_kind = TurretKind::Hymn;
        m.world.spawn_tank(
            Vec3::ONE,
            at(ahead, side),
            yaw,
            HullKind::Kataphrakt,
            TurretKind::Kontos,
        );
        m.fighters
            .push(fighter(1, max_health(HullKind::Kataphrakt)));
        m.bots.push(None);
        for f in &mut m.fighters {
            f.shield = 0.0;
        }
        for _ in 0..60 {
            m.step(Command::default(), false);
        }
        for _ in 0..(1.0 / DT) as usize {
            m.step(Command::default(), true);
        }
        max_health(HullKind::Kataphrakt) - m.fighters[1].health
    }

    /// The flame burns what it visibly engulfs, not just what its puffs'
    /// centres pass through, and still nothing clearly outside it.
    #[test]
    fn flame_burns_what_it_engulfs() {
        let dead_on = flame_burn(8.0, 0.0, None);
        assert!(dead_on > 40.0, "dead on at 8 m: {dead_on}");
        // Hull side about 1 m off the barrel line, well inside the fire.
        let edge = flame_burn(8.0, 2.5, None);
        assert!(edge > 10.0, "engulfed at its edge: {edge}");
        // Further out, where the fire has swelled: hull side 1.5 m off.
        let far = flame_burn(11.0, 3.0, None);
        assert!(far > 3.0, "engulfed at 11 m: {far}");
        // Hull side 3.5 m off: nowhere near the fire.
        assert_eq!(flame_burn(8.0, 5.0, None), 0.0);
    }

    /// The fire a puff shows doesn't reach round a thin wall onto a tank
    /// pressed right up behind it.
    #[test]
    fn flame_fire_stops_at_thin_walls() {
        // The hull's nose is 0.3 m behind the wall's back face.
        let open = flame_burn(9.7, 0.0, None);
        assert!(open > 20.0, "nothing in the way: {open}");
        assert_eq!(flame_burn(9.7, 0.0, Some(7.0)), 0.0);
    }

    #[test]
    fn flame_pours_through_tanks_but_not_walls() {
        let open = flame_through(None, false);
        assert!(open > 5.0, "the flamer should reach 11 m: {open}");
        // An enemy in front burns too, and doesn't shield the one behind.
        let past_enemy = flame_through(Some(1), false);
        assert!(
            past_enemy > open * 0.5,
            "open {open}, past an enemy {past_enemy}"
        );
        // Nor does an ally.
        let past_ally = flame_through(Some(0), false);
        assert!(
            past_ally > open * 0.5,
            "open {open}, past an ally {past_ally}"
        );
        // A wall does.
        assert_eq!(flame_through(None, true), 0.0);
    }

    /// Damage per second of beam contact at distance `dist`.
    fn beam_dps_at(dist: f32) -> f32 {
        let mut m = duel(TurretKind::Lumen, dist);
        let hp = m.fighters[1].health;
        for _ in 0..30 {
            m.step(Command::default(), true);
        }
        (hp - m.fighters[1].health) / (30.0 * DT)
    }

    #[test]
    fn beam_hurts_less_far_away_but_still_hurts() {
        // The duel yard only reaches 95 m out; past that, check the curve.
        let near = beam_dps_at(30.0);
        let far = beam_dps_at(95.0);
        assert!(near > 0.0, "near {near}");
        let want = near * beam_falloff(95.0, 160.0);
        assert!((far - want).abs() < near * 0.05, "near {near}, far {far}");
        assert!(far < near * 0.95, "near {near}, far {far}");
        assert_eq!(beam_falloff(50.0, 160.0), 1.0);
        assert!((beam_falloff(160.0, 160.0) - 0.6).abs() < 1e-6);
    }

    #[test]
    fn beam_bends_onto_a_target_just_off_its_line() {
        let mut m = duel(TurretKind::Lumen, 80.0);
        // Swing the turret a little off the target: a narrow miss.
        m.world.tanks[0].turret_rel += 0.02;
        let hp = m.fighters[1].health;
        for _ in 0..10 {
            m.step(Command::default(), true);
        }
        assert!(m.fighters[1].health < hp);
        // Well off the line it misses.
        let mut m = duel(TurretKind::Lumen, 80.0);
        m.world.tanks[0].turret_rel += 0.2;
        let hp = m.fighters[1].health;
        for _ in 0..10 {
            m.step(Command::default(), true);
        }
        assert_eq!(m.fighters[1].health, hp);
    }

    #[test]
    fn heavier_hulls_hold_the_beam_longer() {
        let t = 1.6;
        assert!(heat_capacity(HullKind::Gravis, t) > heat_capacity(HullKind::Kastron, t));
        assert!(heat_capacity(HullKind::Kastron, t) > heat_capacity(HullKind::Kataphrakt, t));
        assert!(heat_capacity(HullKind::Kataphrakt, t) > heat_capacity(HullKind::Sethuun, t));
    }

    #[test]
    fn beam_overheats_and_locks_until_cool() {
        let mut m = duel(TurretKind::Lumen, 60.0);
        let Action::Beam { thermal_mass, .. } = gun(TurretKind::Lumen).action else {
            panic!("Lumen is a beam");
        };
        let heat_time = heat_capacity(m.world.tanks[0].hull_kind, thermal_mass);
        let cool_time = heat_time / COOL_RATE;
        for _ in 0..((heat_time + 0.1) / DT) as usize {
            m.step(Command::default(), true);
        }
        assert!(m.fighters[0].overheated);
        m.sounds.clear();
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
        // Cooled right down: you hear it's ready, once.
        let ready = m
            .sounds
            .iter()
            .filter(|s| matches!(s, Sound::BeamReady { tank: 0, .. }))
            .count();
        assert_eq!(ready, 1);
    }

    #[test]
    fn plasma_orbs_bounce_off_the_ground() {
        let mut m = Match::new(Map::yard(200.0), 0, 0, 1);
        let Action::Shell(spec) = gun(TurretKind::Ammeth).action else {
            panic!("Ammeth fires shells");
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
            seed: 0.0,
            passed: Vec::new(),
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
        // Darts just stop.
        let Action::Shell(dart) = gun(TurretKind::Ossrin).action else {
            panic!();
        };
        m.shells = vec![Shell {
            pos: from,
            prev: from,
            vel: Vec3::new(0.0, -dart.speed, 0.0),
            owner: 0,
            damage: 1.0,
            age: 0.0,
            spec: dart,
            bounces: 0,
            seed: 0.0,
            passed: Vec::new(),
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
        assert_eq!(m.reputation, 0);
        m.kill(0, 1);
        assert_eq!(m.reputation, KILL_REPUTATION);
        assert_eq!(
            m.reputation_pops.len(),
            1,
            "one popup, for the player's own kill"
        );
    }

    #[test]
    fn round_end_starts_the_results_clock() {
        let mut m = arena(1);
        m.time_left = DT * 0.5;
        m.step(Command::default(), false);
        assert_eq!(m.phase, Phase::Over);
        m.step(Command::default(), false);
        assert!(m.over_for > 0.0);
    }

    #[test]
    fn upgrades_fit_at_the_next_respawn() {
        let mut m = arena(1);
        let mut up = Levels::default();
        up.set(crate::economy::Part::Hull(m.world.tanks[0].hull_kind), 2);
        m.fighters[0].next_levels = Some(up);
        let before = m.fighters[0].max_health(&m.world.tanks[0]);
        m.step(Command::default(), false);
        assert_eq!(
            m.fighters[0].max_health(&m.world.tanks[0]),
            before,
            "not while alive"
        );
        m.kill(1, 0);
        for _ in 0..(RESPAWN_TIME / DT) as usize + 2 {
            m.step(Command::default(), false);
        }
        assert!(m.fighters[0].alive);
        assert!(m.fighters[0].health > before);
    }

    #[test]
    fn dead_tanks_respawn_with_full_health() {
        let mut m = arena(1);
        // A Kataphrakt: a Vigil would run on as a husk first.
        m.world.set_hull(1, HullKind::Kataphrakt);
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

    /// A broken Vigil is paid as a kill at once, but runs on as a husk that
    /// can't be hurt, at full strength and then fading, before it's wrecked.
    #[test]
    fn vigil_runs_on_as_a_husk() {
        let mut m = duel(TurretKind::Kontos, 20.0);
        m.world.set_hull(1, HullKind::Vigil);
        m.damage(0, 1, 1000.0);
        let f = &m.fighters[1];
        assert_eq!(
            (f.deaths, m.fighters[0].kills),
            (1, 1),
            "the kill is paid at once"
        );
        assert!(f.alive && f.is_husk());
        assert_eq!(f.strength(), 1.0);
        // Untouchable while it runs.
        m.damage(0, 1, 1000.0);
        assert_eq!((m.fighters[1].deaths, m.fighters[0].kills), (1, 1));
        for _ in 0..((HUSK_STRONG + 0.1) / DT) as usize {
            m.step(Command::default(), false);
        }
        let s = m.fighters[1].strength();
        assert!(m.fighters[1].alive && s < 1.0 && s > 0.8, "fading: {s}");
        for _ in 0..(HUSK_FADE / DT) as usize {
            m.step(Command::default(), false);
        }
        assert!(!m.fighters[1].alive, "wrecked once its time runs out");
        assert!(!m.fighters[1].is_husk());
        assert_eq!(m.fighters[1].deaths, 1, "one death, not two");
        assert_eq!(m.world.tanks[1].husk, None);
    }

    /// Issue #115: a Vigil breaking, running on and going out each sound like
    /// the Resonant: the shell shatters (not a generic blast), its bell tolls
    /// weaker and weaker as the husk runs, and it gutters out at the end.
    #[test]
    fn a_vigil_rings_as_it_breaks_and_goes_out() {
        let mut m = duel(TurretKind::Kontos, 20.0);
        m.world.set_hull(1, HullKind::Vigil);
        m.sounds.clear();
        m.damage(0, 1, 1000.0);
        let breaking = std::mem::take(&mut m.sounds);
        assert!(breaking.iter().any(|s| matches!(s, Sound::Shatter { .. })));
        assert!(!breaking.iter().any(|s| matches!(s, Sound::Kill { .. })));
        let mut tolls = Vec::new();
        let mut gutters = 0;
        for _ in 0..((HUSK_STRONG + HUSK_FADE + 0.2) / DT) as usize {
            m.step(Command::default(), false);
            for s in m.sounds.drain(..) {
                match s {
                    Sound::Toll { strength, .. } => tolls.push(strength),
                    Sound::Gutter { .. } => gutters += 1,
                    Sound::Kill { .. } | Sound::Shatter { .. } => panic!("blew up again"),
                    _ => {}
                }
            }
        }
        assert!(!m.fighters[1].alive);
        assert_eq!(gutters, 1, "goes out once");
        assert_eq!(tolls.len(), 3, "tolls {tolls:?}");
        assert!(
            tolls.windows(2).all(|w| w[1] <= w[0]) && tolls[2] < tolls[0],
            "each toll weaker: {tolls:?}"
        );
    }

    /// Record a Vigil breaking 12 m in front of the player, running on as a
    /// husk and going out, as the player hears it, to `$VIGIL_WAV`
    /// (`docs/audio/vigil-husk.wav` in the PR for #115).
    #[test]
    #[ignore]
    fn record_a_vigil_going_out() {
        const RATE: u32 = 48_000;
        let mut m = duel(TurretKind::Kontos, 12.0);
        m.world.set_hull(1, HullKind::Vigil);
        let mut audio = crate::audio::Audio::offline(RATE);
        audio.set_volume(0.8);
        let mut cam = crate::scene::ChaseCam::new();
        let mut out = Vec::new();
        let mut owed = 0.0;
        // Let the arrivals ring out first.
        for _ in 0..(2.0 / DT) as usize {
            m.step(Command::default(), false);
        }
        m.sounds.clear();
        for n in 0..(9.0 / DT) as usize {
            if n == (0.5 / DT) as usize {
                m.damage(0, 1, 1000.0);
            }
            m.step(Command::default(), false);
            let camera = cam.update(&m.world, &m.world.tanks[0], DT);
            let sounds = std::mem::take(&mut m.sounds);
            audio.update(&m, &sounds, &camera, true, DT);
            owed += RATE as f32 * DT;
            let k = owed as usize;
            owed -= k as f32;
            let start = out.len();
            out.resize(start + k * 2, 0.0);
            audio.render(&mut out[start..]);
        }
        let path = std::env::var("VIGIL_WAV").unwrap_or("vigil-husk.wav".into());
        std::fs::write(path, crate::audio::wav(&out, RATE)).unwrap();
    }

    /// Other hulls just die.
    #[test]
    fn other_hulls_do_not_run_on() {
        let mut m = duel(TurretKind::Kontos, 20.0);
        m.damage(0, 1, 1000.0);
        assert!(!m.fighters[1].alive);
        assert!(!m.fighters[1].is_husk());
    }

    /// A fading husk's shots hit softer.
    #[test]
    fn husk_firepower_fades() {
        let mut m = duel(TurretKind::Kontos, 20.0);
        m.world.set_hull(0, HullKind::Vigil);
        m.fighters[0].husk = HUSK_FADE * 0.5;
        m.fighters[0].reload_in = 0.0;
        m.shoot(0);
        let full = gun(TurretKind::Kontos).damage;
        let dealt = m.shells.last().unwrap().damage;
        assert!((dealt - full * 0.5).abs() < 1e-3, "{dealt} of {full}");
    }

    #[test]
    fn queued_kit_is_fitted_at_respawn() {
        let mut m = arena(1);
        // Whatever the player has now, queue the other hull.
        let hull = if m.world.tanks[0].hull_kind == HullKind::Kastron {
            HullKind::Kataphrakt
        } else {
            HullKind::Kastron
        };
        let kit = (hull, TurretKind::Pelekys);
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

    fn round_cues(m: &Match) -> Vec<RoundCue> {
        m.sounds
            .iter()
            .filter_map(|s| match s {
                Sound::Round(c) => Some(*c),
                _ => None,
            })
            .collect()
    }

    /// Issue #117: the round's start, its last ten seconds, its end and
    /// how it went are each heard, once.
    #[test]
    fn the_round_is_heard_starting_counting_down_and_ending() {
        let mut m = arena(0);
        m.step(Command::default(), false);
        m.step(Command::default(), false);
        assert_eq!(round_cues(&m), [RoundCue::Start]);
        m.sounds.clear();
        m.time_left = 10.5;
        m.fighters[0].kills = 1;
        for _ in 0..(12.0 / DT) as usize {
            m.step(Command::default(), false);
        }
        let mut want: Vec<_> = (1..=COUNTDOWN).rev().map(RoundCue::Count).collect();
        want.extend([RoundCue::End, RoundCue::Won]);
        assert_eq!(round_cues(&m), want);
    }

    #[test]
    fn a_round_lost_on_the_clock_sounds_lost() {
        let mut m = arena(1);
        m.fighters[1].kills = 2;
        m.time_left = DT * 0.5;
        m.step(Command::default(), false);
        assert_eq!(round_cues(&m), [RoundCue::End, RoundCue::Lost]);
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
        let mut m = Match::new(Map::yard(200.0), 0, 0, 1);
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
    fn holding_pays_reputation_and_reaching_the_target_ends_the_round() {
        let mut m = Match::new(Map::yard(200.0), 0, 0, 3);
        m.set_contract(Contract::Hold);
        let at = m.hold.as_ref().unwrap().at;
        m.world.teleport(0, at, 0.0);
        m.hold.as_mut().unwrap().own[0] = hold::REPUTATION_EVERY - 0.1;
        for _ in 0..60 {
            m.step(Command::default(), false);
        }
        assert_eq!(m.reputation, hold::HOLD_REPUTATION);
        assert_eq!(m.reputation_pops.last().map(|x| x.reason), Some("HOLDING"));
        let heard: Vec<_> = m
            .sounds
            .iter()
            .filter_map(|s| match s {
                Sound::Hold(c) => Some(*c),
                _ => None,
            })
            .collect();
        // Paid first here only because the test starts a hair short of a payout.
        assert_eq!(heard, [hold::Cue::Paid, hold::Cue::Taken]);
        assert_eq!(m.phase, Phase::Playing);
        m.hold.as_mut().unwrap().held[0] = hold::TARGET - 0.1;
        for _ in 0..30 {
            m.step(Command::default(), false);
        }
        assert_eq!(m.phase, Phase::Over);
        assert_eq!(m.leader(), Some(0));
    }

    #[test]
    fn hold_score_is_time_held_not_kills() {
        let mut m = arena(3);
        m.set_contract(Contract::Hold);
        for f in m.fighters.iter_mut().filter(|f| f.team == 1) {
            f.kills = 9;
        }
        m.hold.as_mut().unwrap().held[0] = 5.5;
        assert_eq!((m.score(0), m.score(1)), (5, 0));
        assert_eq!(m.leader(), Some(0));
    }

    /// Off the mesa's edge: dead when it drops past the fall line, and the
    /// kill goes to whoever hit it last.
    #[test]
    fn falling_off_the_edge_kills() {
        let mut m = Match::new(Map::sanctum(), 0, 1, 1);
        for f in &mut m.fighters {
            f.shield = 0.0;
        }
        m.world.set_hull(0, HullKind::Kataphrakt);
        m.damage(1, 0, 1.0);
        // Out over the desert floor, well clear of the cliff.
        m.world.teleport(0, Vec2::new(150.0, 285.0), 0.0);
        for _ in 0..(1.0 / DT) as usize {
            m.step(Command::default(), false);
        }
        assert!(
            !m.fighters[0].alive,
            "still alive at y {}",
            m.world.tanks[0].pos.y
        );
        assert_eq!(m.fighters[0].deaths, 1);
        assert_eq!(m.fighters[1].kills, 1, "the last hit knocked it off");
    }

    /// Drop tank `i` from the mesa's height over the desert floor, run until
    /// it dies (or `secs` pass), and give the lowest height above the floor it
    /// was still alive at, the height it died at, and how far it fell.
    fn drop_from_the_mesa(m: &mut Match, i: usize, secs: f32) -> (f32, Option<f32>, f32) {
        let at = Vec2::new(150.0, 285.0);
        m.world.teleport(i, at, 0.0);
        let floor = m.world.map.ground_height(at.x, at.y);
        let b = &mut m.world.physics.bodies[m.world.tanks[i].body];
        b.set_translation(Vec3::new(at.x, 0.5, at.y), true);
        let (mut alive_low, mut died_at) = (f32::INFINITY, None);
        let deaths = m.fighters[i].deaths;
        for _ in 0..(secs / DT) as usize {
            m.step(Command::default(), false);
            let h = m.world.tanks[i].pos.y - floor;
            if m.fighters[i].deaths > deaths {
                died_at = Some(h);
                break;
            }
            alive_low = alive_low.min(h);
        }
        (alive_low, died_at, 0.5 - floor)
    }

    /// Over the edge, it falls the whole way and blows up on the floor, not
    /// in mid-air as it passes the fall line.
    #[test]
    fn a_tank_that_goes_over_dies_on_the_floor_not_in_the_air() {
        let mut m = Match::new(Map::sanctum(), 0, 0, 1);
        m.fighters[0].shield = 0.0;
        m.world.set_hull(0, HullKind::Kataphrakt);
        let (alive_low, died_at, drop) = drop_from_the_mesa(&mut m, 0, 6.0);
        assert!(drop > 25.0, "only a {drop} m drop");
        let died_at = died_at.expect("never died");
        assert!(died_at < LANDED_CLEARANCE, "died {died_at:.1} m up");
        assert!(alive_low < 3.0, "only fell to {alive_low:.1} m up alive");
        assert!(!m.fighters[0].alive);
    }

    /// Bots and hover hulls too, and a Vigil runs on at the bottom.
    #[test]
    fn every_hull_falls_all_the_way_bots_included() {
        for hull in crate::garage::HULLS {
            let mut m = Match::new(Map::sanctum(), 0, 1, 1);
            for f in &mut m.fighters {
                f.shield = 0.0;
            }
            m.world.set_hull(1, hull);
            let (_, died_at, _) = drop_from_the_mesa(&mut m, 1, 6.0);
            let died_at = died_at.unwrap_or_else(|| panic!("{hull:?} never died"));
            assert!(
                died_at < LANDED_CLEARANCE,
                "{hull:?} died {died_at:.1} m up"
            );
            if hull == HullKind::Vigil {
                assert!(m.fighters[1].alive && m.fighters[1].is_husk());
            } else {
                assert!(!m.fighters[1].alive, "{hull:?}");
            }
        }
    }

    /// A Vigil that goes over runs on at the bottom, as it does anywhere.
    #[test]
    fn a_vigil_that_falls_runs_on() {
        let mut m = Match::new(Map::sanctum(), 0, 0, 1);
        m.fighters[0].shield = 0.0;
        m.world.set_hull(0, HullKind::Vigil);
        m.world.teleport(0, Vec2::new(150.0, 285.0), 0.0);
        for _ in 0..(1.0 / DT) as usize {
            m.step(Command::default(), false);
        }
        assert!(m.fighters[0].alive && m.fighters[0].is_husk());
        assert_eq!(m.fighters[0].deaths, 1);
    }

    /// Under Hold, bots of both sides go for the site and someone holds it.
    #[test]
    fn bots_go_for_the_hold_site() {
        let mut m = Match::new(Map::sanctum(), 0, 5, 5);
        m.set_contract(Contract::Hold);
        for _ in 0..(60.0 / DT) as usize {
            m.fighters[0].shield = 1.0;
            m.step(Command::default(), false);
        }
        let h = m.hold.as_ref().unwrap();
        assert!(
            h.held[0] + h.held[1] > 5.0,
            "the site was held for only {:.1} s in a minute",
            h.held[0] + h.held[1]
        );
        assert!(
            (1..m.fighters.len()).any(|i| m.fighters[i].team == 1 && m.held(i) > 0.0),
            "no enemy bot ever stood on a held site"
        );
    }

    #[test]
    fn odd_bot_counts_make_even_teams_in_their_own_paint() {
        let m = arena(11);
        for team in 0..2 {
            let side: Vec<usize> = (0..12).filter(|&i| m.fighters[i].team == team).collect();
            assert_eq!(side.len(), 6);
            for i in side {
                assert_eq!(m.world.tanks[i].color, TANK_PAINT);
            }
        }
        assert_eq!(m.fighters[0].team, 0);
    }

    #[test]
    fn each_side_spawns_at_its_own_base() {
        for map in [Map::saltmarket(), Map::sanctum(), Map::kiln()] {
            let m = Match::new(map, 0, 11, 5);
            for (i, f) in m.fighters.iter().enumerate() {
                let p = Vec2::new(m.world.tanks[i].pos.x, m.world.tanks[i].pos.z);
                assert!(
                    m.world.map.bases[f.team]
                        .iter()
                        .any(|b| b.distance(p) < 0.5),
                    "tank {i} (side {}) spawned off base at {p}",
                    f.team
                );
            }
        }
    }

    #[test]
    fn allies_take_no_damage_from_each_other() {
        let mut m = arena(3);
        let ally = (1..4).find(|&i| m.fighters[i].team == 0).unwrap();
        let enemy = (1..4).find(|&i| m.fighters[i].team == 1).unwrap();
        for f in &mut m.fighters {
            f.shield = 0.0;
        }
        let shell = |owner| Shell {
            pos: Vec3::ZERO,
            prev: Vec3::ZERO,
            vel: Vec3::X,
            owner,
            damage: 1000.0,
            age: 0.0,
            spec: match gun(TurretKind::Kontos).action {
                Action::Shell(spec) => spec,
                Action::Beam { .. } => unreachable!(),
            },
            bounces: 0,
            seed: 0.0,
            passed: Vec::new(),
        };
        m.hit(&shell(0), ally, Vec3::ZERO);
        assert!(m.fighters[ally].alive);
        assert_eq!(
            m.fighters[ally].health,
            max_health(m.world.tanks[ally].hull_kind)
        );
        m.hit(&shell(0), enemy, Vec3::ZERO);
        assert_eq!(m.fighters[enemy].deaths, 1);
        assert_eq!(m.score(0), 1);
        assert_eq!(m.leader(), Some(0));
    }

    #[test]
    fn the_side_with_more_kills_wins_even_if_you_scored_none() {
        let mut m = arena(3);
        let ally = (1..4).find(|&i| m.fighters[i].team == 0).unwrap();
        let enemy = (1..4).find(|&i| m.fighters[i].team == 1).unwrap();
        m.fighters[ally].kills = 3;
        m.fighters[enemy].kills = 2;
        m.time_left = DT * 0.5;
        m.step(Command::default(), false);
        assert_eq!(m.phase, Phase::Over);
        assert_eq!(m.leader(), Some(0));
        assert_eq!(
            crate::economy::Payout::of(&m).outcome,
            crate::economy::Outcome::Won
        );
        // Level: a draw.
        m.fighters[enemy].kills = 3;
        assert_eq!(m.leader(), None);
    }

    #[test]
    fn standings_rank_by_kills_then_deaths() {
        let mut m = arena(2);
        m.fighters[2].kills = 3;
        m.fighters[1].kills = 3;
        m.fighters[1].deaths = 1;
        assert_eq!(m.standings(), vec![2, 1, 0]);
    }

    /// Left alone, the two sides find each other and fight: kills happen
    /// within a minute, and nobody's killer is on their own side.
    #[test]
    fn bots_fight_each_other() {
        let mut m = arena(5);
        // Park the player out of the way, invulnerable.
        for _ in 0..(60.0 / DT) as usize {
            m.fighters[0].shield = 1.0;
            m.step(Command::default(), false);
        }
        let kills: u32 = m.fighters.iter().map(|f| f.kills).sum();
        for (i, f) in m.fighters.iter().enumerate() {
            if let Some(k) = f.killed_by.filter(|&k| k != i) {
                assert_ne!(m.fighters[k].team, f.team, "tank {k} killed its ally {i}");
            }
        }
        assert!(
            kills >= 2,
            "only {kills} kills in a minute of bot deathmatch"
        );
    }
}

#[cfg(test)]
mod hover_bots_stay_upright {
    use super::*;

    /// Ithessar bots used to go end over end within seconds of the start:
    /// running into each other at full speed off their spawn columns, or
    /// driving up crystal faces. A match's worth of them stays upright.
    #[test]
    fn ithessar_bots_dont_flip_on_the_kiln() {
        let mut m = Match::new(Map::kiln(), 0, 11, 1);
        let n = m.world.tanks.len();
        let mut tipped = vec![false; n];
        for _ in 0..(20.0 / DT) as usize {
            m.fighters[0].shield = 1.0;
            m.step(Command::default(), false);
            for (i, t) in m.world.tanks.iter().enumerate().skip(1) {
                tipped[i] |= t.hull_kind == HullKind::Sethuun && !t.dead && t.up().y < 0.3;
            }
        }
        let hover = m
            .world
            .tanks
            .iter()
            .filter(|t| t.hull_kind == HullKind::Sethuun)
            .count();
        assert!(hover >= 5, "only {hover} hover hulls");
        let flipped: Vec<usize> = (0..n).filter(|&i| tipped[i]).collect();
        assert!(flipped.is_empty(), "hover bots {flipped:?} tipped over");
    }
}
