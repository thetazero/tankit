//! The theater you're posted to (#311): one contested star system, as the
//! missions screen shows it.
//!
//! **Contested sites.** Every side fighting in the system has a front: the
//! enemy's sites there, any of which it can take (#426: shields don't stand
//! in the way, they only cost an attacker a seat). Those are the contested
//! sites, and any of them can be looked at.
//!
//! **Committed sites.** Only the battles your side's automated combat commits
//! to this tick can be entered: the site it attacks (the one the simulation's
//! tick would go for, see `Galaxy::target`) and any of its own sites an enemy
//! attacks. A side at war always has one: an attacker always has a target,
//! and a defender is always being attacked.
//!
//! **Your company** (#327, #350). You're with one of the galaxy's free
//! companies (see `merc`), `Galaxy::crew`, and the theater is its
//! contract's: the system its ship is parked over, fighting for the realm
//! that hired it, to take the system or to hold it (#366: then it's parked
//! over the world under attack). A new pilot joins the company in the biggest fight
//! (`Galaxy::enlist`), and stays with it: if it joins another company, you
//! go with it. While it has no battle for you (between contracts), the war
//! goes on without you until it has (`Galaxy::await_work`): the galaxy
//! otherwise only ticks with battles. Like every company, yours sails to
//! the nearest war to look for work when it's idle too long
//! (`Galaxy::seek_work`, #355).
//!
//! **Maps.** A shield generator is fought over on the shield city (#341), or
//! on a desert world perhaps the shield desert (#337), a Hold for the
//! generator: its holder's model, in the shape the site's in
//! (up, wrecked, or going back up as it's repaired). Every other site is
//! fought on one of the missions' maps for its planet's class (`Class`),
//! picked at random, and on a generated kind the one map its seed names
//! (`SiteView::map_seed`): the site is one place, the same each visit.

use super::{AT_WAR, Galaxy, PEOPLES, SIDES, SiteKind};
use crate::conditions::Conditions;
use crate::faction::Faction;
use crate::map::procgen::Look;
use crate::map::procgen::foundry::Ground;
use crate::map::procgen::setting::Setting;
use crate::map::procgen::shield::Condition;
use crate::mission::MISSIONS;
use crate::rng::Rng;
use crate::{Generator, MapKind};
use serde::{Deserialize, Serialize};

/// The theater as the missions screen shows it: a snapshot of one system.
#[derive(Clone, Debug)]
pub struct Theater {
    /// Index into `Galaxy::systems`.
    pub system: usize,
    pub name: String,
    /// Who you're posted for: index into `Faction::ALL`.
    pub employer: usize,
    pub planets: u8,
    /// Each side's hulls here (and how many of them are mercs' own), and sites held.
    pub hulls: [u32; SIDES],
    pub mercs: [u32; SIDES],
    pub held: [u8; SIDES],
    /// Each side's gear tier here (#438): its realm's, in upgrade steps.
    pub tiers: [u8; SIDES],
    /// Every site in the system, in the galaxy's order.
    pub sites: Vec<SiteView>,
    /// The contested sites, as indices into `sites`: the rows of the list.
    pub contested: Vec<usize>,
    /// The galaxy's tick this was taken at.
    pub tick: u64,
    /// The ship you're aboard, parked here on its contract.
    pub ship: Option<Ship>,
}

/// Your company's ship, and the contract it's here on.
#[derive(Clone, Debug)]
pub struct Ship {
    /// The company's id (`Company::id`): what the profile keeps.
    pub id: u32,
    pub name: String,
    /// The world it's parked over: where its side's attack goes in this
    /// tick, or on a defense, the world under attack.
    pub over: u8,
    /// The realm that hired it, by its seat's name, and that realm's people
    /// (index into `Faction::ALL`).
    pub realm: String,
    pub side: usize,
    /// Hired to hold one of the realm's own worlds (#340, #366), not to
    /// take one: it's done when the attack is beaten off.
    pub defend: bool,
    /// Ticks until the contract runs out (one a battle).
    pub ticks_left: u64,
    /// The tick it was signed: a new one is a new contract.
    pub signed: u64,
    /// Its hulls still committed to the contract (in the fight), and of them yours alone (#385): left, of how many.
    pub committed: u32,
    pub own: Own,
    /// What it pays you, CR: a battle, and a kill.
    pub pay: (u32, u32),
    /// Your company's last few ended contracts, newest last (#414).
    pub ended: Vec<Ended>,
}

/// Your own hulls on your company's contract (#385): set once when you
/// start on it, by your rank then (`battle::own_hulls`), never more than
/// half the hulls it has committed. Bots can't spend them; only your
/// deaths do.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Own {
    /// The company and contract they're on (`Company::id`, `Contract::signed`).
    pub ship: u32,
    pub signed: u64,
    pub left: u32,
    pub of: u32,
}

/// The contract you're signed on for (#557): whose, and what it pays you.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Job {
    /// The company and contract (`Company::id`, `Contract::signed`).
    pub ship: u32,
    pub signed: u64,
    /// Crystals a battle, as you signed for; a kill is worth
    /// `Rules::kill_share` of it.
    pub wage: f32,
}

/// How a contract ended (#414).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// Succeeded: the system was taken, or (on a defense) the attack was repelled.
    Won,
    /// Failed a defense: the world was lost.
    Fell,
    /// Failed: every hull committed to it was lost.
    Spent,
    /// Failed: the employer no longer exists (merged into another realm, or lost all its worlds).
    Gone,
    /// Failed: time ran out.
    Expired,
}

impl Outcome {
    /// True if the contract succeeded.
    pub fn won(self) -> bool {
        self == Outcome::Won
    }
}

/// A finished contract of the player's company, with the stats shown on the
/// contract summary card (#414).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ended {
    /// Which company (`Company::id`) and which contract (`Contract::signed`, the tick it was signed).
    pub ship: u32,
    pub signed: u64,
    /// The tick the contract ended on.
    pub tick: u64,
    pub outcome: Outcome,
    pub defend: bool,
    /// Company hulls committed to the contract, and how many were lost.
    #[serde(deserialize_with = "super::save::whole_hulls")]
    pub committed: u32,
    #[serde(deserialize_with = "super::save::whole_hulls")]
    pub lost: u32,
    /// The player's own hulls on the contract (#385) at the end.
    pub own: Own,
}

/// How many finished contracts of the player's company to keep. More than
/// one, since several can end while the game is closed.
pub const ENDED_KEPT: usize = 8;

/// The contract you were last shown on the missions screen (#386): kept in
/// the profile, so that a move onto the next contract plays whenever it
/// happened (in the battle just fought, between battles, or while the game
/// was closed). Enough to show the theater it was in again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shown {
    /// The ship you were aboard (`Company::id`) and its contract's tick signed.
    pub ship: u32,
    pub signed: u64,
    /// The system it was in, the side that hired it, the world it was over,
    /// and that side's seat.
    pub system: usize,
    pub side: usize,
    pub over: u8,
    pub realm: String,
    /// It was hired to hold that world, not take it (#366).
    #[serde(default)]
    pub defend: bool,
}

impl Shown {
    /// What theater `t` shows of your contract, if you're aboard a ship.
    pub fn of(t: &Theater) -> Option<Shown> {
        t.ship.as_ref().map(|s| Shown {
            ship: s.id,
            signed: s.signed,
            system: t.system,
            side: s.side,
            over: s.over,
            realm: s.realm.clone(),
            defend: s.defend,
        })
    }
}

/// One site of the theater.
#[derive(Clone, Copy, Debug)]
pub struct SiteView {
    /// Index into `Galaxy::sites`.
    pub index: usize,
    pub planet: u8,
    /// What kind of world that is.
    pub class: Class,
    /// Index into `Faction::ALL`.
    pub owner: usize,
    pub kind: SiteKind,
    pub contested: bool,
    /// Your side commits to a battle here this tick: you can enter it.
    pub committed: bool,
    /// Who you'd fight here: its owner if it's an enemy's, else whoever's
    /// attacking it (or the strongest enemy in the system).
    pub enemy: usize,
    /// The mission (index into `MISSIONS`) whose map and contract it's fought with.
    pub mission: usize,
    /// Which map a generated kind builds here: the site's own, the same
    /// each time it's fought over.
    pub map_seed: u64,
}

/// A contract on offer (another company's ship's), as the board compares them.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    /// The ship's company (`Company::id`).
    pub id: u32,
    /// The system it's over.
    pub system: String,
    /// Who hires, and who you'd fight most there.
    pub side: usize,
    pub enemy: usize,
    /// To hold it, not take it.
    pub defend: bool,
    /// Your side's hulls there, of them mercs' own, and the enemy's.
    pub hulls: u32,
    pub mercs: u32,
    pub foes: u32,
    /// The enemy's gear tier.
    pub tier: u8,
    /// How many hulls you'd have of your own.
    pub own: u32,
    /// What it pays you, CR: a battle, and a kill.
    pub wage: u32,
    pub kill: u32,
    /// Ticks till the ship lands there, and how long the work's expected
    /// to go on then, about.
    pub flight: u32,
    pub lasts: u32,
}

impl Offer {
    /// "TAKE TORKAR".
    pub fn job(&self) -> String {
        let job = if self.defend { "HOLD" } else { "TAKE" };
        format!("{job} {}", self.system)
    }

    /// Your side's share of the hulls there.
    pub fn strength(&self) -> f32 {
        self.hulls as f32 / (self.hulls + self.foes).max(1) as f32
    }
}

/// A battle at a site, to deploy into.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sortie {
    pub system: usize,
    /// Index into `Galaxy::sites`.
    pub site: usize,
    /// Index into `MISSIONS`: the contract, and the map's kind.
    pub mission: usize,
    /// The map, as the site is (a shield generator's its holder's, in its shape).
    pub map_kind: MapKind,
    /// Which map a generated kind builds: the site's (`SiteView::map_seed`).
    pub map_seed: u64,
    /// The site's (`SiteView::conditions`).
    pub conditions: Conditions,
    /// The match's teams: you (your employer) first, then the enemy.
    pub factions: [Faction; 2],
}

/// What the war did to a system's hulls since your battle there began
/// (#475): the tick summary screen between the battle's results and the
/// contract screen, so a count gone from 5 to 55 says why.
#[derive(Clone, Debug, PartialEq)]
pub struct TickSummary {
    pub system: usize,
    pub name: String,
    /// Ticks since the battle began: its own, and the ones the war went on
    /// without you after it.
    pub ticks: u64,
    /// Every side with hulls there before or now, the most now first, each
    /// followed by the company on contract for it there, if any.
    pub sides: Vec<SideChange>,
    /// The system's sites that changed hands since, in order.
    pub flips: Vec<Flip>,
}

/// A site that changed hands, in a `TickSummary`.
#[derive(Clone, Debug, PartialEq)]
pub struct Flip {
    /// Which world of the system it's on (0 for the first), and what it is.
    pub planet: u8,
    pub kind: &'static str,
    /// Who held it, and who does now (indices into `Faction::ALL`).
    pub from: usize,
    pub to: usize,
}

/// One side's hulls in a `TickSummary`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SideChange {
    /// Index into `Faction::ALL`.
    pub side: usize,
    /// The company's hulls on contract for `side`, not the side's own.
    pub merc: bool,
    pub before: u32,
    pub now: u32,
    pub moves: super::Moves,
}

/// A system's name: two or three syllables, the same for every pilot (it
/// comes from the system's catalogue number).
pub fn system_name(s: usize) -> String {
    const START: [&str; 24] = [
        "KES", "VAR", "THU", "OR", "SEL", "MAR", "IST", "DOR", "HAL", "NE", "CAR", "VEL", "ASH",
        "TOR", "YS", "BRE", "KAL", "ULM", "SER", "PHA", "ZEN", "GOR", "ETH", "LIR",
    ];
    const MID: [&str; 12] = [
        "A", "E", "I", "O", "U", "AN", "EL", "OR", "IS", "UN", "AR", "EN",
    ];
    const END: [&str; 16] = [
        "RA", "TH", "NE", "MOS", "DIS", "KAR", "LON", "VIA", "RUN", "SET", "TIS", "GAL", "NOR",
        "PHE", "XA", "DUN",
    ];
    let mut r = Rng::new(0x5EED_0000 ^ s as u64);
    let mut name = START[r.index(START.len())].to_string();
    if r.chance(0.6) {
        name += MID[r.index(MID.len())];
    }
    name += END[r.index(END.len())];
    name
}

/// A company ship's name, from the company's id: "THE IRON TITHE" without
/// the article.
pub fn ship_name(id: u32) -> String {
    const FIRST: [&str; 24] = [
        "IRON", "SALT", "GREY", "LAST", "BLACK", "HOLLOW", "BRASS", "COLD", "SILENT", "RED",
        "BITTER", "PALE", "FAR", "WORN", "LONG", "DRY", "ASHEN", "NINTH", "LEAN", "QUIET",
        "BROKEN", "GILT", "DUSK", "KEEN",
    ];
    const SECOND: [&str; 24] = [
        "TITHE",
        "LEDGER",
        "WIDOW",
        "LANTERN",
        "HOUND",
        "MARGIN",
        "PROMISE",
        "OATH",
        "COIN",
        "ANVIL",
        "HARVEST",
        "WAGER",
        "ERRAND",
        "DEBT",
        "VIGIL",
        "MERCY",
        "WAKE",
        "TALLY",
        "BARGAIN",
        "HAMMER",
        "PILGRIM",
        "WARRANT",
        "FURROW",
        "RECKONING",
    ];
    let mut r = Rng::new(0x5817_0000 ^ id as u64);
    format!(
        "{} {}",
        FIRST[r.index(FIRST.len())],
        SECOND[r.index(SECOND.len())]
    )
}

/// What kind of world a planet is, which sets the maps its sites are
/// fought on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// A desert world: its sites are generated (cities, mines,
    /// reservoir towns, open desert).
    Desert,
    /// A frozen world: its foundries are generated, its other sites fought
    /// on the hand-built maps.
    Tundra,
    /// A green world, likewise.
    Grassland,
    /// Not made yet: its sites are fought on the hand-built maps.
    Placeholder,
}

impl Class {
    /// Every class, in the order of their discriminants.
    pub const ALL: [Class; 4] = [
        Class::Desert,
        Class::Tundra,
        Class::Grassland,
        Class::Placeholder,
    ];

    /// The ground its hull foundries stand on, if it has them.
    pub fn ground(self) -> Option<Ground> {
        match self {
            Class::Desert => Some(Ground::Desert),
            Class::Tundra => Some(Ground::Tundra),
            Class::Grassland => Some(Ground::Grassland),
            Class::Placeholder => None,
        }
    }

    /// Whether a mission's map is one this class's sites are fought on.
    pub fn fights_on(self, kind: MapKind) -> bool {
        match kind {
            MapKind::City(..) | MapKind::Mine | MapKind::Desert | MapKind::Reservoir => {
                self == Class::Desert
            }
            MapKind::Foundry(..) => self.ground().is_some(),
            MapKind::Sanctum | MapKind::Kiln => self != Class::Desert,
            MapKind::ShieldSite(..) | MapKind::ShieldDesert(_) => false,
        }
    }

    /// Whether a mission's map is one this class's shield generators are
    /// fought over on.
    pub fn shields_on(self, kind: MapKind) -> bool {
        match kind {
            MapKind::ShieldSite(..) => true,
            MapKind::ShieldDesert(_) => self == Class::Desert,
            _ => false,
        }
    }
}

/// Planet `planet` of system `system`, the same for every pilot: its
/// class, and how it looks from orbit (the seed and look its globe is made
/// from). A desert world is dry, a tundra one icy, a grassland one green.
pub fn world(system: usize, planet: usize) -> (Class, u64, crate::planet::Look) {
    let seed = (system as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ ((planet as u64 + 1) * 0x5851);
    let worlds = [
        (Class::Desert, crate::planet::DRY),
        (Class::Grassland, crate::planet::GREEN),
        (Class::Tundra, crate::planet::ICE),
        (Class::Placeholder, crate::planet::DRY),
    ];
    let (class, look) = worlds[(seed >> 7) as usize % worlds.len()];
    (class, seed, look)
}

/// Planet `p` of a system called `name`: "KESSARA II".
pub fn planet_name(name: &str, p: u8) -> String {
    const NUMERALS: [&str; 4] = ["I", "II", "III", "IV"];
    format!("{name} {}", NUMERALS[(p as usize).min(3)])
}

/// The `kind_name` of a production site that makes mostly crystals.
pub const CRYSTAL_MINE: &str = "CRYSTAL MINE";

/// What a site is, in caps: a shield generator, an outpost, or what a production site
/// makes most of, for what sites make (1 to 5 hulls, 0.02 to 0.1 crystals):
/// about half are mines.
pub fn kind_name(kind: SiteKind) -> &'static str {
    match kind {
        SiteKind::Shield { .. } => "SHIELD GENERATOR",
        SiteKind::Production { hulls, crystals } if crystals * 50.0 > hulls => CRYSTAL_MINE,
        SiteKind::Production { .. } => "HULL FOUNDRY",
        SiteKind::Outpost => "OUTPOST",
    }
}

impl SiteView {
    /// What it is, in caps (see the free `kind_name`).
    pub fn kind_name(&self) -> &'static str {
        kind_name(self.kind)
    }

    /// What it is, in a word, in caps.
    pub fn short_name(&self) -> &'static str {
        match self.kind_name() {
            "SHIELD GENERATOR" => "SHIELD",
            "CRYSTAL MINE" => "MINE",
            "OUTPOST" => "OUTPOST",
            _ => "FOUNDRY",
        }
    }

    #[cfg(test)]
    /// A stand-in site, for tests: a shield generator, or a crystal mine.
    pub fn example(shield: bool) -> SiteView {
        SiteView {
            index: 0,
            planet: 0,
            class: Class::Placeholder,
            owner: 0,
            kind: if shield {
                SiteKind::Shield { damaged: false }
            } else {
                SiteKind::Production {
                    hulls: 0.1,
                    crystals: 0.1,
                }
            },
            contested: false,
            committed: false,
            enemy: 1,
            mission: 0,
            map_seed: 0,
        }
    }

    pub fn is_shield(&self) -> bool {
        matches!(self.kind, SiteKind::Shield { .. })
    }

    /// The map it's fought on: its mission's, a city in its founder's style,
    /// a foundry in its founder's own on its world's ground,
    /// and for a shield generator,
    /// the holder's model in the shape it's in.
    pub fn map_kind(&self) -> MapKind {
        let kind = MISSIONS[self.mission].map_kind;
        let look = Look::of(self.founder());
        let setting = match self.class {
            Class::Desert => Setting::Desert,
            Class::Tundra | Class::Grassland | Class::Placeholder => Setting::Plain,
        };
        let SiteKind::Shield { damaged } = self.kind else {
            return match kind {
                MapKind::City(..) => MapKind::City(look, setting),
                MapKind::Foundry(_, ground) => MapKind::Foundry(
                    Look::own(self.founder()),
                    self.class.ground().unwrap_or(ground),
                ),
                kind => kind,
            };
        };
        let made = Generator {
            owner: Faction::side(self.owner),
            condition: if damaged {
                Condition::DAMAGED
            } else {
                Condition::Up
            },
        };
        match kind {
            MapKind::ShieldSite(..) => MapKind::ShieldSite(look, setting, made),
            MapKind::ShieldDesert(_) => MapKind::ShieldDesert(made),
            kind => kind,
        }
    }

    /// The time of day and weather it's fought in: the site's own, the same
    /// each time it's fought over, like its map.
    pub fn conditions(&self) -> Conditions {
        Conditions::of_site(self.map_seed)
    }

    /// Who built the place, whoever holds it now: a city's in their style.
    pub fn founder(&self) -> Faction {
        let mut r = Rng::new(self.map_seed ^ 0xF0_07DE);
        Faction::ALL[r.index(Faction::ALL.len())]
    }

    /// A damaged generator (#422): its shield's down until it's working again.
    pub fn damaged(&self) -> bool {
        self.kind == SiteKind::Shield { damaged: true }
    }
}

impl Theater {
    /// The site `k`th in the contested list.
    pub fn contested_site(&self, k: usize) -> Option<&SiteView> {
        self.contested.get(k).map(|&i| &self.sites[i])
    }

    /// The battle at contested site `k`, if your side commits to it.
    pub fn sortie(&self, k: usize) -> Option<Sortie> {
        let s = self.contested_site(k)?;
        s.committed.then(|| Sortie {
            system: self.system,
            site: s.index,
            mission: s.mission,
            map_kind: s.map_kind(),
            map_seed: s.map_seed,
            conditions: s.conditions(),
            factions: [Faction::side(self.employer), Faction::side(s.enemy)],
        })
    }

    /// The sides here from off it, whose fleets are in orbit: everyone with
    /// hulls but the defender, whose are all on the ground.
    pub fn attackers(&self) -> Vec<usize> {
        let defend = self.ship.as_ref().is_some_and(|s| s.defend);
        let holder = (0..PEOPLES)
            .filter(|&f| defend || f != self.employer)
            .max_by_key(|&f| self.held[f]);
        let defender = if defend {
            Some(self.employer)
        } else {
            holder.filter(|&f| self.held[f] > 0)
        };
        (0..PEOPLES)
            .filter(|&f| self.hulls[f] >= AT_WAR && Some(f) != defender)
            .collect()
    }

    /// The sides in the war here (hulls or sites), most hulls first.
    pub fn sides(&self) -> Vec<usize> {
        let mut out: Vec<usize> = (0..PEOPLES)
            .filter(|&f| self.hulls[f] >= AT_WAR || self.held[f] > 0)
            .collect();
        out.sort_by_key(|&a| std::cmp::Reverse(self.hulls[a]));
        out
    }
}

impl Galaxy {
    pub(crate) fn present(&self, s: usize) -> [bool; SIDES] {
        self.systems[s].hulls.map(|h| h >= AT_WAR)
    }

    /// Side `f` has hulls in system `s` and an enemy there with hulls too.
    pub fn fighting(&self, s: usize, f: usize) -> bool {
        let present = self.present(s);
        present[f] && (0..SIDES).any(|g| present[g] && self.hostile(f, g))
    }

    fn site_range(&self, s: usize) -> std::ops::Range<usize> {
        let (a, b) = self.systems[s].sites;
        a as usize..b as usize
    }

    /// The sites side `f` goes for in system `s` this tick, all at once
    /// (#460): every enemy shield generator, on every planet, damaged ones
    /// too (left alone, a damaged one is working again by the next tick);
    /// and on each planet with none of an enemy's up, every enemy site
    /// there. Shields first: a planet's other sites wait for its
    /// generators. The picks the tick's `battles` makes, in site order.
    pub fn targets(&self, s: usize, f: usize) -> Vec<usize> {
        let up = self.shields_up(s, f);
        self.site_range(s)
            .filter(|&i| {
                let site = &self.sites[i];
                self.hostile(f, site.owner as usize)
                    && (up[site.planet as usize] == 0
                        || matches!(site.kind, SiteKind::Shield { .. }))
            })
            .collect()
    }

    /// The sites side `f` is fighting for in system `s`: every enemy site there.
    pub fn front(&self, s: usize, f: usize) -> Vec<usize> {
        if !self.fighting(s, f) {
            return Vec::new();
        }
        self.site_range(s)
            .filter(|&i| self.hostile(f, self.sites[i].owner as usize))
            .collect()
    }

    /// The battles side `f` commits to in system `s` this tick: its own
    /// attacks, and the defense of each of its sites an enemy attacks.
    pub fn commits(&self, s: usize, f: usize) -> Vec<usize> {
        if !self.fighting(s, f) {
            return Vec::new();
        }
        let present = self.present(s);
        let mut out = self.targets(s, f);
        for g in (0..PEOPLES).filter(|&g| present[g] && self.hostile(f, g) && self.fighting(s, g)) {
            for t in self.targets(s, g) {
                if self.sites[t].owner as usize == f && !out.contains(&t) {
                    out.push(t);
                }
            }
        }
        // Not against a raiding company: there's no such battle to play yet.
        if present[super::RAID] || self.systems[s].held[super::RAID] > 0 {
            let first = self.site_range(s).start;
            let at = self.site_enemies(s, f);
            out.retain(|&i| at[i - first] != super::RAID);
        }
        out.sort();
        out
    }

    /// The ships with work for you: companies on a contract with a battle
    /// to join this tick (indices into `companies`).
    fn berths(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.companies.len()).filter(|&c| {
            let co = &self.companies[c];
            !co.pilots.is_empty()
                && co.contract.is_some_and(|k| {
                    k.side as usize != super::RAID
                        && k.field > 0
                        && !self.commits(k.target as usize, k.side as usize).is_empty()
                })
        })
    }

    /// Company `id`'s index in `companies`, if it's still about.
    pub fn company(&self, id: u32) -> Option<usize> {
        self.companies.iter().position(|c| c.id == id)
    }

    /// Your company (an index into `companies`), if you're with one.
    pub fn crew_company(&self) -> Option<usize> {
        self.company(self.crew?)
    }

    /// Put a pilot with no company yet (or whose company is gone) on a job:
    /// the biggest fight there is this tick. `prefer` (a side) goes first,
    /// for shots and tests. Nothing happens while you have a company, or
    /// while there's no fight to give you.
    pub fn enlist(&mut self, prefer: Option<usize>) {
        if self.crew_company().is_some() {
            return;
        }
        self.take_any(prefer);
    }

    /// Sign on for the biggest fight there is this tick (`enlist`). False if
    /// there's none.
    fn take_any(&mut self, prefer: Option<usize>) -> bool {
        let best = self.berths().max_by(|&a, &b| {
            let (ka, kb) = (
                self.companies[a].contract.unwrap(),
                self.companies[b].contract.unwrap(),
            );
            let side = |k: super::merc::Contract| prefer == Some(k.side as usize);
            let sites = |k: super::merc::Contract| self.site_range(k.target as usize).len();
            side(ka)
                .cmp(&side(kb))
                .then(sites(ka).cmp(&sites(kb)))
                .then(ka.field.cmp(&kb.field))
                .then(b.cmp(&a))
        });
        let Some(c) = best else { return false };
        self.sign_with(c);
        true
    }

    /// You sign on for company `c`'s contract (an index into `companies`).
    fn sign_with(&mut self, c: usize) {
        let Some(k) = self.companies[c].contract else {
            return;
        };
        self.crew = Some(self.companies[c].id);
        self.job = Some(Job {
            ship: self.companies[c].id,
            signed: k.signed,
            wage: self.wage_for_you(c),
        });
    }

    /// You're signed on for whatever your company's on (older saves).
    pub(crate) fn stay_on(&mut self) {
        if let Some(c) = self.crew_company() {
            self.sign_with(c);
        }
    }

    /// What company `c` pays you a battle on its contract, crystals: what its
    /// crew's paid, by how well your gear fights.
    fn wage_for_you(&self, c: usize) -> f32 {
        let crew = &self.companies[c].pilots;
        let each = |p: &super::merc::Pilot| p.wage / super::merc::gear_value(p.rank);
        let going = crew.iter().map(each).sum::<f32>() / crew.len().max(1) as f32;
        going * super::merc::gear_value(self.you.level())
    }

    /// The contract you're signed on for, while it runs: your company (an
    /// index into `companies`) and the contract.
    pub fn on_job(&self) -> Option<(usize, super::merc::Contract)> {
        let job = self.job?;
        let c = self.crew_company()?;
        let k = self.companies[c].contract?;
        (self.companies[c].id == job.ship && k.signed == job.signed).then_some((c, k))
    }

    /// What your job pays you, CR: a battle, and a kill.
    pub fn pay_rates(&self) -> Option<(u32, u32)> {
        self.on_job()?;
        Some(self.in_cr(self.job?.wage))
    }

    /// A wage of `wage` crystals in CR: a battle, and a kill.
    fn in_cr(&self, wage: f32) -> (u32, u32) {
        let cr = wage * self.cr_per_crystal();
        let kill = (cr * self.rules.kill_share).round() as u32;
        (cr.round() as u32, kill)
    }

    /// The companies that would take you on: those with a battle for you
    /// this tick, and those on their way to one, that can pay you.
    fn hiring(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.companies.len()).filter(|&c| {
            let co = &self.companies[c];
            let Some(k) = co.contract else { return false };
            let ready = if self.tick < k.arrives {
                k.inbound > 0
            } else {
                k.field > 0 && !self.commits(k.target as usize, k.side as usize).is_empty()
            };
            let wage = self.wage_for_you(c);
            k.side as usize != super::RAID
                && ready
                && !co.pilots.is_empty()
                && wage > 0.0
                && wage <= co.crystals
                && self.on_job().is_none_or(|(mine, _)| mine != c)
        })
    }

    /// Company `c`'s contract as an offer to you (or, yours, as you signed).
    fn offer_of(&self, c: usize, rank: u32) -> Option<Offer> {
        let co = &self.companies[c];
        let k = co.contract.filter(|k| k.side as usize != super::RAID)?;
        let (s, f) = (k.target as usize, k.side as usize);
        let sys = &self.systems[s];
        let at = self.site_enemies(s, f);
        let first = self.site_range(s).start;
        let commits = self.commits(s, f);
        let enemy = (0..PEOPLES).filter(|&e| e != f).max_by_key(|&e| {
            let fights = commits.iter().filter(|&&i| at[i - first] == e).count();
            (fights, sys.hulls[e], sys.held[e])
        })?;
        // Nobody there to fight, or only raiders.
        if sys.hulls[enemy] == 0 {
            return None;
        }
        let wage = match self.on_job() {
            Some((mine, _)) if mine == c => self.job.map_or(0.0, |j| j.wage),
            _ => self.wage_for_you(c),
        };
        let (wage, kill) = self.in_cr(wage);
        let mercs: u32 = self
            .companies
            .iter()
            .filter_map(|o| o.contract)
            .filter(|o| o.target as usize == s && o.side as usize == f)
            .map(|o| o.field)
            .sum();
        let spent = self.tick.saturating_sub(k.arrives) as f32;
        let own = self.own_on(co.id, k, rank).left;
        Some(Offer {
            id: co.id,
            system: system_name(s),
            side: f,
            enemy,
            defend: k.defend,
            hulls: sys.hulls[f] + k.inbound,
            mercs: mercs.min(sys.hulls[f]) + k.inbound,
            foes: sys.hulls[enemy],
            tier: match sys.realms[enemy] {
                super::realm::NONE => 0,
                r => self.realms[r as usize].tier,
            },
            own: own.min(k.field + k.inbound),
            wage,
            kill,
            flight: k.arrives.saturating_sub(self.tick) as u32,
            lasts: (k.expected_ticks * self.trust_ticks - spent)
                .max(1.0)
                .round() as u32,
        })
    }

    /// Your own contract, as the board shows it.
    pub fn my_work(&self, rank: u32) -> Option<Offer> {
        self.offer_of(self.on_job()?.0, rank)
    }

    /// The contracts you could sign on to, nearest first: one for each side
    /// hiring in a system (whoever pays you most), and not the one you're on.
    pub fn open_work(&self, rank: u32) -> Vec<Offer> {
        let mine = self.my_work(rank);
        let same = |a: &Offer, b: &Offer| a.system == b.system && a.side == b.side;
        let here = self
            .crew_company()
            .map(|c| self.systems[self.companies[c].at as usize].pos);
        let mut near: Vec<usize> = self.hiring().collect();
        let key = |c: usize| {
            let to = self.systems[self.companies[c].contract.map_or(0, |k| k.target) as usize].pos;
            here.map_or(0.0, |h| to.distance(h))
        };
        near.sort_by(|&a, &b| key(a).total_cmp(&key(b)).then(a.cmp(&b)));
        let mut out: Vec<Offer> = Vec::new();
        for c in near {
            let Some(o) = self
                .offer_of(c, rank)
                .filter(|o| !mine.as_ref().is_some_and(|m| same(m, o)))
            else {
                continue;
            };
            match out.iter_mut().find(|x| same(x, &o)) {
                Some(x) if x.wage < o.wage => *x = o,
                Some(_) => {}
                None => out.push(o),
            }
        }
        out
    }

    /// Sign on with ship `id` for its contract (leaving the one you're on,
    /// and the rest of its pay, if there's one). Your own hulls stay with
    /// the contract you leave. False (and nothing changes) if it isn't hiring.
    pub fn sign_on(&mut self, id: u32) -> bool {
        let Some(c) = self.company(id).filter(|&c| self.hiring().any(|b| b == c)) else {
            return false;
        };
        self.sign_with(c);
        true
    }

    /// The theater with your company; while it has no battle for you on the
    /// contract you're signed on for (it's flying there, say), the war goes
    /// on without you (up to `ticks` ticks) until it has. With that contract
    /// over, it goes on only till someone's hiring. True in the second if
    /// the galaxy ticked.
    pub fn await_work(
        &mut self,
        prefer: Option<usize>,
        rank: u32,
        ticks: u32,
    ) -> (Option<Theater>, bool) {
        for i in 0..=ticks {
            self.enlist(prefer);
            self.claim_own(rank);
            if let Some(t) = self.aboard(rank) {
                return (Some(t), i > 0);
            }
            if self.free_to_choose() {
                return (None, i > 0);
            }
            if i < ticks {
                self.step();
            }
        }
        (None, ticks > 0)
    }

    /// Your last contract's over, and someone's hiring.
    pub fn free_to_choose(&self) -> bool {
        self.crew_company().is_some()
            && self.on_job().is_none()
            && self.hiring().any(|c| self.offer_of(c, 1).is_some())
    }

    /// `await_work` a slice at a time (#452), so a frame needn't hold up
    /// on all of it: as many of the `left` ticks as go before `enough`
    /// says stop (at least one, if any are left). True once it's done:
    /// there's a battle for you, or work to choose, or no ticks are left.
    pub fn await_work_slice(
        &mut self,
        prefer: Option<usize>,
        rank: u32,
        left: &mut u32,
        mut enough: impl FnMut() -> bool,
    ) -> bool {
        let mut stepped = false;
        loop {
            let (t, _) = self.await_work(prefer, rank, 0);
            if t.is_some() || self.free_to_choose() || *left == 0 {
                return true;
            }
            if stepped && enough() {
                return false;
            }
            self.step();
            *left -= 1;
            stepped = true;
        }
    }

    /// `await_work`, but whenever you're free you sign on for the biggest
    /// fight (for shots and tests, where there's nobody to choose).
    pub fn await_work_staying(
        &mut self,
        prefer: Option<usize>,
        rank: u32,
        ticks: u32,
    ) -> (Option<Theater>, bool) {
        let mut ticked = false;
        for _ in 0..=ticks {
            if self.on_job().is_none() {
                self.take_any(prefer);
            }
            if let (Some(t), _) = self.await_work(prefer, rank, 0) {
                return (Some(t), ticked);
            }
            self.step();
            ticked = true;
        }
        (None, ticked)
    }

    /// Your own hulls on contract `k` of company `ship` (#385): the ones
    /// you have, if they're on it; else the ones you'd get at `rank`.
    fn own_on(&self, ship: u32, k: super::merc::Contract, rank: u32) -> Own {
        match self.own {
            Some(o) if o.ship == ship && o.signed == k.signed => o,
            _ => {
                let half = (k.field + k.inbound).div_ceil(2);
                let of = super::battle::own_hulls(rank).min(half);
                Own {
                    ship,
                    signed: k.signed,
                    left: of,
                    of,
                }
            }
        }
    }

    /// Set your own hulls for your company's contract at `rank` (#385),
    /// unless they're already set for it: once a contract, so ranking up
    /// on it doesn't add any. None without a contract.
    pub fn claim_own(&mut self, rank: u32) {
        let on = self.on_job().map(|(c, k)| (self.companies[c].id, k));
        self.own = on.map(|(ship, k)| self.own_on(ship, k, rank));
    }

    /// Your own hulls left on your company's contract (0 if none are set).
    pub fn own_left(&self) -> u32 {
        let on = self.on_job().map(|(c, k)| (self.companies[c].id, k.signed));
        match (self.own, on) {
            (Some(o), Some((ship, signed))) if o.ship == ship && o.signed == signed => o.left,
            _ => 0,
        }
    }

    /// You lost `n` of your own hulls in a battle.
    pub fn spend_own(&mut self, n: u32) {
        if let Some(o) = &mut self.own {
            o.left = o.left.saturating_sub(n);
        }
    }

    /// The theater aboard your company's ship: its contract's system, for
    /// the side that hired it. None if you're on no job, or it has no battle
    /// for you this tick.
    pub fn aboard(&self, rank: u32) -> Option<Theater> {
        self.aboard_company(self.on_job()?.0, rank)
    }

    /// The theater aboard company `c` (an index into `companies`).
    fn aboard_company(&self, c: usize, rank: u32) -> Option<Theater> {
        let co = &self.companies[c];
        let k = co.contract.filter(|k| k.side as usize != super::RAID)?;
        let (s, f) = (k.target as usize, k.side as usize);
        if k.field == 0 || co.pilots.is_empty() || self.commits(s, f).is_empty() {
            return None;
        }
        let mut t = self.theater_at(s, f);
        // Over the world its side attacks; on a defense, the world under
        // attack (#366). Else whichever it fights over first.
        let commits = self.commits(s, f);
        let attacked = commits
            .iter()
            .copied()
            .find(|&i| self.sites[i].owner as usize == f);
        let attack = self.targets(s, f).first().copied();
        let over = if k.defend {
            attacked.or(attack)
        } else {
            attack.or(attacked)
        }
        .or(commits.first().copied())
        .map_or(0, |i| self.sites[i].planet);
        t.ship = Some(Ship {
            id: co.id,
            name: ship_name(co.id),
            over,
            realm: system_name(self.realms[k.realm as usize].seat as usize),
            side: f,
            defend: k.defend,
            ticks_left: (k.arrives + self.rules.merc_term as u64).saturating_sub(self.tick),
            signed: k.signed,
            committed: k.field,
            own: self.own_on(co.id, k, rank),
            pay: self.offer_of(c, rank).map_or((0, 0), |o| (o.wage, o.kill)),
            ended: self.ended.clone(),
        });
        Some(t)
    }

    /// The theater of contract `shown`, as its system is now: what a move
    /// onto the next contract leaves (#386). None if the galaxy has no such
    /// system (a new war since).
    pub fn shown(&self, shown: &Shown) -> Option<Theater> {
        if shown.system >= self.systems.len() || shown.side >= SIDES {
            return None;
        }
        let mut t = self.theater_at(shown.system, shown.side);
        t.ship = Some(Ship {
            id: shown.ship,
            name: ship_name(shown.ship),
            over: shown.over,
            realm: shown.realm.clone(),
            side: shown.side,
            defend: shown.defend,
            ticks_left: 0,
            signed: shown.signed,
            committed: 0,
            own: Own::default(),
            pay: (0, 0),
            ended: Vec::new(),
        });
        Some(t)
    }

    /// Who side `f` would fight at each site of system `s` (in order): its
    /// owner if it's an enemy's, else whoever's attacking it (or the
    /// strongest enemy in the system).
    fn site_enemies(&self, s: usize, f: usize) -> Vec<usize> {
        let sys = &self.systems[s];
        let present = self.present(s);
        // Who attacks each site this tick.
        let mut attacker: Vec<Option<usize>> = vec![None; self.site_range(s).len()];
        for g in (0..SIDES).filter(|&g| self.fighting(s, g)) {
            for t in self.targets(s, g) {
                attacker[t - sys.sites.0 as usize].get_or_insert(g);
            }
        }
        let strongest = (0..PEOPLES)
            .filter(|&g| present[g] && self.hostile(f, g))
            .max_by_key(|&a| sys.hulls[a])
            .unwrap_or((f + 1) % PEOPLES);
        self.site_range(s)
            .map(|i| {
                let owner = self.sites[i].owner as usize;
                if self.hostile(f, owner) {
                    owner
                } else {
                    attacker[i - sys.sites.0 as usize]
                        .filter(|&g| self.hostile(f, g))
                        .unwrap_or(strongest)
                }
            })
            .collect()
    }

    /// System `s`'s tick summary (#475): its hulls since the battle fought
    /// there at tick `fought_at` began (`moves` start over then).
    pub fn tick_summary(&self, s: usize, fought_at: u64, owners: &[u8]) -> TickSummary {
        let sys = &self.systems[s];
        let (a, b) = (sys.sites.0 as usize, sys.sites.1 as usize);
        let flips = (a..b)
            .filter_map(|i| {
                let (site, was) = (&self.sites[i], *owners.get(i)?);
                (site.owner != was).then(|| Flip {
                    planet: site.planet,
                    kind: kind_name(site.kind),
                    from: was as usize,
                    to: site.owner as usize,
                })
            })
            .collect();
        let row = |f, merc, now: u32, moves: super::Moves| SideChange {
            side: f,
            merc,
            before: (now as i64 - moves.net()).max(0) as u32,
            now,
            moves,
        };
        let shown = |c: &SideChange| c.before > 0 || c.now > 0;
        // The companies' hulls on contract here, each side's.
        let mut field = [0u32; SIDES];
        for k in self.companies.iter().filter_map(|c| c.contract.as_ref()) {
            if k.target as usize == s {
                field[k.side as usize] += k.field;
            }
        }
        let mut pairs: Vec<(SideChange, SideChange)> = (0..SIDES)
            .map(|f| {
                let mercs = self.merc_moves[s][f];
                let now = sys.hulls[f].saturating_sub(field[f]);
                let own = row(f, false, now, self.moves[s][f].less(&mercs));
                (own, row(f, true, field[f], mercs))
            })
            .filter(|(own, mercs)| shown(own) || shown(mercs))
            .collect();
        pairs.sort_by_key(|(c, m)| std::cmp::Reverse((c.now + m.now, c.before + m.before)));
        let mut sides = Vec::new();
        for (own, mercs) in pairs {
            sides.push(own);
            if shown(&mercs) {
                sides.push(mercs);
            }
        }
        TickSummary {
            system: s,
            name: system_name(s),
            ticks: self.tick.saturating_sub(fought_at),
            sides,
            flips,
        }
    }

    /// Who holds each site (`Galaxy::sites`), as `tick_summary` compares it.
    pub fn owners(&self) -> Vec<u8> {
        self.sites.iter().map(|s| s.owner).collect()
    }

    /// System `s` as side `f` sees it. Each site's mission is its own,
    /// whatever your rank (#437).
    pub fn theater_at(&self, s: usize, f: usize) -> Theater {
        let sys = &self.systems[s];
        let present = self.present(s);
        let mut contested: Vec<usize> = Vec::new();
        for g in (0..SIDES).filter(|&g| present[g]) {
            contested.extend(self.front(s, g));
        }
        contested.sort();
        contested.dedup();
        let commits = self.commits(s, f);
        let enemies = self.site_enemies(s, f);
        let foundries = Class::ALL.map(crate::mission::for_foundries);
        let mines = Class::ALL.map(crate::mission::for_mines);
        let shields = Class::ALL.map(crate::mission::for_shields);
        let outposts = Class::ALL.map(crate::mission::for_outposts);
        let sites: Vec<SiteView> = self
            .site_range(s)
            .map(|i| {
                let site = self.sites[i];
                let owner = site.owner as usize;
                let enemy = enemies[i - sys.sites.0 as usize];
                let mut r = Rng::new(self.seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                let world_class = world(s, site.planet as usize).0;
                let class = world_class as usize;
                // A generator's mission is drawn after its map's seed.
                let (mission, map_seed) = match site.kind {
                    SiteKind::Shield { .. } => {
                        let map_seed = r.next_u64();
                        let pool = &shields[class];
                        (pool[r.index(pool.len())], map_seed)
                    }
                    _ => {
                        let pool = match kind_name(site.kind) {
                            _ if site.kind == SiteKind::Outpost => &outposts[class],
                            CRYSTAL_MINE => &mines[class],
                            _ => &foundries[class],
                        };
                        (pool[r.index(pool.len())], r.next_u64())
                    }
                };
                SiteView {
                    index: i,
                    planet: site.planet,
                    class: world_class,
                    owner,
                    kind: site.kind,
                    contested: contested.contains(&i),
                    committed: commits.contains(&i),
                    enemy,
                    mission,
                    map_seed,
                }
            })
            .collect();
        let contested = (0..sites.len()).filter(|&k| sites[k].contested).collect();
        Theater {
            system: s,
            name: system_name(s),
            employer: f,
            planets: sys.planets,
            hulls: sys.hulls,
            mercs: {
                let mut mercs = [0; SIDES];
                for k in self.companies.iter().filter_map(|c| c.contract) {
                    if k.target as usize == s {
                        mercs[k.side as usize] += k.field;
                    }
                }
                std::array::from_fn(|f| mercs[f].min(sys.hulls[f]))
            },
            held: sys.held,
            tiers: std::array::from_fn(|g| match sys.realms[g] {
                super::realm::NONE => 0,
                r => self.realms[r as usize].tier,
            }),
            sites,
            contested,
            tick: self.tick,
            ship: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::{REGION, Rules, SYSTEMS};
    use crate::mission::MISSIONS;

    fn galaxy() -> Galaxy {
        let mut g = Galaxy::with_size(3, SYSTEMS / 4, REGION * 0.5, Rules::default());
        for _ in 0..60 {
            g.step();
        }
        g
    }

    /// A small war, ticked on until some ship has a battle to join, with
    /// you in a company.
    fn galaxy_with_work() -> Galaxy {
        let mut g = galaxy();
        assert!(
            g.await_work_staying(None, 1, 400).0.is_some(),
            "no work in 460 ticks"
        );
        g
    }

    /// With any company that has a battle to join, there's a battle for you
    /// there, for the side that hired it.
    #[test]
    fn with_a_company_at_work_there_is_a_battle_to_join() {
        let mut g = galaxy_with_work();
        let ships: Vec<usize> = g.berths().collect();
        assert!(!ships.is_empty(), "no ship on a contract");
        for c in ships {
            let co = g.companies[c].clone();
            let k = co.contract.unwrap();
            g.crew = Some(co.id);
            let t = g.aboard(1).expect("a theater");
            let ship = t.ship.as_ref().unwrap();
            assert_eq!(ship.id, co.id);
            assert_eq!((t.system, t.employer), (k.target as usize, k.side as usize));
            assert!(ship.over < t.planets);
            let committed: Vec<&SiteView> = t.sites.iter().filter(|s| s.committed).collect();
            assert!(!committed.is_empty(), "{}", ship.name);
            for s in &committed {
                assert!(s.contested, "a committed site that isn't contested");
                assert!(g.hostile(t.employer, s.enemy));
            }
            assert!((0..t.contested.len()).any(|k| t.sortie(k).is_some()));
        }
    }

    /// With your company hired to hold one of its employer's worlds
    /// (#366): the theater is that system, for the employer; the ship says
    /// it's a defense and is parked over a world under attack; some battle
    /// is the defense of one of the employer's own sites; and every battle
    /// you can enter musters with your company in it.
    #[test]
    fn your_company_hired_to_defend() {
        let mut g = galaxy();
        let mut seen = 0;
        for _ in 0..1500 {
            g.step();
            let ids: Vec<u32> = g
                .companies
                .iter()
                .filter(|c| c.contract.is_some_and(|k| k.defend))
                .map(|c| c.id)
                .collect();
            for id in ids {
                g.crew = Some(id);
                g.stay_on();
                let Some(t) = g.aboard(1) else { continue };
                let k = g.companies[g.crew_company().unwrap()].contract.unwrap();
                let ship = t.ship.as_ref().unwrap();
                assert!(ship.defend);
                assert_eq!((t.system, t.employer), (k.target as usize, k.side as usize));
                let committed: Vec<&SiteView> = t.sites.iter().filter(|s| s.committed).collect();
                let ours: Vec<&&SiteView> =
                    committed.iter().filter(|s| s.owner == t.employer).collect();
                if g.targets(t.system, t.employer).is_empty() {
                    assert!(!ours.is_empty(), "nothing to attack, nothing to defend");
                }
                if !ours.is_empty() {
                    assert!(
                        ours.iter().any(|s| s.planet == ship.over),
                        "parked over {}, not a world under attack",
                        ship.over
                    );
                }
                for s in &committed {
                    let sides = [Faction::side(t.employer), Faction::side(s.enemy)];
                    let b = g
                        .muster_crew(t.system, &sides, Some(s.index))
                        .expect("a battle with your company in it");
                    assert_eq!(b.crew, Some(id));
                    assert!(b.mercs(0) >= 1);
                }
                seen += 1;
            }
            if seen >= 20 {
                return;
            }
        }
        panic!("only {seen} defenses with a battle for you in 1500 ticks");
    }

    /// A new pilot joins the company in the biggest fight (or one working
    /// for the side asked for).
    #[test]
    fn a_new_pilot_joins_a_company_at_work() {
        let mut g = galaxy_with_work();
        for f in 0..SIDES {
            g.crew = None;
            g.enlist(Some(f));
            let c = g.crew_company().expect("a company");
            assert!(g.berths().any(|b| b == c));
            if g.berths()
                .any(|b| g.companies[b].contract.unwrap().side as usize == f)
            {
                assert_eq!(g.companies[c].contract.unwrap().side as usize, f);
            }
        }
    }

    /// Your company between contracts: no theater, and the war goes on
    /// until it has one; idle that long, it sails to the nearest war, by the
    /// same rule as every company (#355).
    #[test]
    fn an_idle_company_waits_then_looks_for_work() {
        let mut g = galaxy_with_work();
        let c = g.crew_company().unwrap();
        let id = g.companies[c].id;
        g.companies[c].contract = None;
        assert!(g.aboard(1).is_none());
        let (t, ticked) = g.await_work(None, 1, 0);
        assert!(t.is_none() && !ticked);
        let at = g.companies[c].at;
        g.companies[c].idle_since = g.tick - g.rules.merc_idle as u64;
        // Realms at war nearby are asking for a company, and none's near.
        let here = g.systems[at as usize].pos;
        let near = (0..g.systems.len())
            .filter(|&s| s != at as usize && g.systems[s].at_war())
            .min_by(|&a, &b| {
                let d = |s: usize| g.systems[s].pos.distance(here);
                d(a).total_cmp(&d(b))
            })
            .unwrap();
        g.unmet.fill(0.0);
        g.job_worth = 1000.0;
        g.unmet[near] = 5.0;
        // And contracts pay well enough for the fare.
        for c in &mut g.companies {
            c.earned = 1000.0;
        }
        assert!(g.seek_work() >= 1);
        assert_eq!(g.crew, Some(id));
        let c = g.crew_company().unwrap();
        let to = g.companies[c].at as usize;
        assert_ne!(to as u32, at);
        assert!(g.systems[to].at_war());
        assert!(g.companies[c].idle_since > g.tick, "not there yet");
    }

    /// You sign on for one contract (#557): it pays what it offered while it
    /// runs; once it's over you're free, still aboard, and pick the next.
    #[test]
    fn you_join_a_company_for_a_contract() {
        let mut g = galaxy_with_work();
        let (c, k) = g.on_job().expect("a job");
        let (id, (wage, kill)) = (g.companies[c].id, g.pay_rates().unwrap());
        assert!(wage > 0 && kill > 0 && kill < wage);
        assert_eq!(g.my_work(1).unwrap().wage, wage);
        // Better gear later doesn't change what you signed for.
        let mut known = crate::economy::Known::default();
        for p in crate::economy::PARTS {
            known.set(p, 12.0);
        }
        g.know(known);
        assert_eq!(g.pay_rates().unwrap().0, wage);
        // Over, whoever's: no battle, no pay, but you're still aboard.
        g.companies[c].contract = None;
        assert!(g.on_job().is_none() && g.pay_rates().is_none() && g.aboard(1).is_none());
        assert_eq!(g.crew, Some(id));
        // Its next contract isn't yours till you sign on for it.
        g.companies[c].contract = Some(super::super::merc::Contract {
            signed: g.tick + 1,
            ..k
        });
        assert!(g.on_job().is_none() && g.aboard(1).is_none());
        let hiring = g.hiring().any(|b| b == c);
        assert_eq!(g.sign_on(id), hiring);
        assert_eq!(g.on_job().is_some(), hiring);
    }

    /// The board: offers that can pay you, not your own, better paid for
    /// better gear; signing on elsewhere leaves the one you're on.
    #[test]
    fn offers_go_by_your_gear() {
        let mut g = galaxy_with_work();
        let mut offers = Vec::new();
        for _ in 0..200 {
            offers = g.open_work(1);
            if offers.len() >= 2 {
                break;
            }
            g.step();
        }
        assert!(offers.len() >= 2, "too few offers");
        for o in &offers {
            let c = g.company(o.id).unwrap();
            assert!(o.wage > 0 && o.lasts >= 1);
            assert!(g.wage_for_you(c) <= g.companies[c].crystals);
            assert!(g.on_job().is_none_or(|(mine, _)| mine != c));
            assert_eq!(
                o.flight > 0,
                g.tick < g.companies[c].contract.unwrap().arrives
            );
        }
        // One a side of a system.
        let mut jobs: Vec<_> = offers.iter().map(|o| (o.system.clone(), o.side)).collect();
        jobs.sort();
        jobs.dedup();
        assert_eq!(jobs.len(), offers.len());
        let mut known = crate::economy::Known::default();
        for p in crate::economy::PARTS {
            known.set(p, 12.0);
        }
        let was = offers[0].clone();
        g.know(known);
        let now = g.open_work(1).into_iter().find(|o| o.id == was.id).unwrap();
        assert!(now.wage > was.wage, "{} then {}", was.wage, now.wage);
        // A company that can't pay you makes no offer.
        let c = g.company(now.id).unwrap();
        let had = std::mem::take(&mut g.companies[c].crystals);
        assert!(g.open_work(1).iter().all(|o| o.id != now.id) && !g.sign_on(now.id));
        g.companies[c].crystals = had;
        assert!(!g.sign_on(u32::MAX));
        assert!(g.sign_on(now.id));
        assert_eq!(g.crew, Some(now.id));
        assert_eq!(g.pay_rates().unwrap().0, now.wage);
        assert!(!g.sign_on(now.id), "already on it");
    }

    /// Signed on for a job a flight away, you skip ahead to its landing.
    #[test]
    fn you_skip_ahead_to_landing() {
        let mut g = galaxy_with_work();
        let o = (0..300)
            .find_map(|_| {
                g.step();
                g.open_work(1).into_iter().find(|o| o.flight > 2)
            })
            .expect("a job a flight away");
        assert!(g.sign_on(o.id) && g.aboard(1).is_none());
        let at = g.tick;
        let (t, ticked) = g.await_work(None, 1, 400);
        assert!(ticked && g.tick >= at + o.flight as u64 || g.on_job().is_none());
        assert!(t.is_some() || g.on_job().is_none(), "landed, or called off");
    }

    /// Waiting for work a slice at a time (#452, a few ticks a frame) ends
    /// where waiting for it all at once does, tick for tick.
    #[test]
    fn waiting_in_slices_is_waiting_all_at_once() {
        // A stretch with a wait of several slices in it.
        let mut g = galaxy_with_work();
        let start = (0..200)
            .find_map(|_| {
                g.step();
                let mut x = g.clone();
                // A job a flight away.
                let o = x.open_work(1).into_iter().find(|o| o.flight > 8)?;
                assert!(x.sign_on(o.id));
                Some(x)
            })
            .expect("a flight");
        let (mut whole, mut sliced) = (start.clone(), start);
        let (t, ticked) = whole.await_work(None, 1, 200);
        assert!(ticked, "work right away: the test shows nothing");
        let mut left = 200;
        let mut slices = 0;
        loop {
            slices += 1;
            let mut n = 0;
            if sliced.await_work_slice(None, 1, &mut left, || {
                n += 1;
                n > 3
            }) {
                break;
            }
        }
        assert!(slices > 1, "all in one slice: the test shows nothing");
        assert_eq!(sliced.tick, whole.tick);
        assert_eq!(sliced.crew, whole.crew);
        assert_eq!(sliced.aboard(1).is_some(), t.is_some());
    }

    /// A pilot whose company is gone (say, a save from another galaxy) is put
    /// with another, and told.
    #[test]
    fn a_pilot_whose_company_is_gone_gets_another() {
        let mut g = galaxy_with_work();
        g.crew = Some(u32::MAX);
        g.enlist(None);
        assert!(g.crew_company().is_some());
    }

    #[test]
    fn ship_names_are_stable_and_varied() {
        assert_eq!(ship_name(7), ship_name(7));
        let names: std::collections::HashSet<String> = (0..100).map(ship_name).collect();
        assert!(names.len() > 80, "{} names", names.len());
    }

    #[test]
    fn system_names_are_stable_and_varied() {
        assert_eq!(system_name(42), system_name(42));
        let names: std::collections::HashSet<String> = (0..200).map(system_name).collect();
        assert!(names.len() > 150, "{} names", names.len());
    }

    /// A shield generator is fought over on a shield map, a Hold for it,
    /// its holder's model in the shape it's in (#341); nothing else is.
    #[test]
    fn generators_are_fought_over_on_shield_maps() {
        let g = galaxy();
        for s in (0..g.systems.len()).flat_map(|sys| g.theater_at(sys, 0).sites) {
            match (s.kind, s.map_kind()) {
                (
                    SiteKind::Shield { damaged },
                    MapKind::ShieldSite(_, _, made) | MapKind::ShieldDesert(made),
                ) => {
                    assert_eq!(MISSIONS[s.mission].contract, crate::mission::Contract::Hold);
                    assert_eq!(made.owner, Faction::side(s.owner));
                    let shape = if damaged {
                        Condition::DAMAGED
                    } else {
                        Condition::Up
                    };
                    assert_eq!(made.condition, shape);
                }
                (SiteKind::Shield { .. }, k) => panic!("a generator fought on {k:?}"),
                (_, k) => assert!(
                    !matches!(k, MapKind::ShieldSite(..) | MapKind::ShieldDesert(_)),
                    "{k:?}"
                ),
            }
        }
    }

    /// Ticks of repair still to go, as how far the repairs have got.
    #[test]
    fn a_downed_generator_is_repaired_a_step_a_tick() {
        assert_eq!(Condition::down(0, 4), Condition::Up);
        assert_eq!(Condition::down(4, 4), Condition::Down { done: 0, of: 4 });
        assert_eq!(Condition::down(1, 4).repaired(), Some(0.75));
    }

    /// A site's map is a two-sided mission.
    #[test]
    fn sites_get_two_sided_maps() {
        let mut g = galaxy();
        let t = g.await_work_staying(None, 1, 400).0.unwrap();
        for s in &t.sites {
            let m = &MISSIONS[s.mission];
            assert_eq!(m.sides.len(), 2, "{}", m.name);
        }
    }

    /// A desert world's sites are generated, a placeholder's hand-built;
    /// a shield generator is the shield city on either, or on a desert
    /// world the shield desert.
    #[test]
    fn a_sites_map_is_its_worlds_class() {
        let g = galaxy();
        let mut seen = [false; 4];
        let mut shields = [0; 2];
        for sys in 0..g.systems.len() {
            for s in &g.theater_at(sys, 0).sites {
                let class = world(sys, s.planet as usize).0;
                seen[class as usize] = true;
                let kind = s.map_kind();
                if s.is_shield() {
                    assert!(class.shields_on(kind), "{class:?} on {kind:?}");
                    if class == Class::Desert {
                        shields[matches!(kind, MapKind::ShieldDesert(_)) as usize] += 1;
                    }
                } else {
                    assert!(class.fights_on(kind), "{class:?} on {kind:?}");
                }
            }
        }
        assert_eq!(seen, [true; 4], "every class of world has sites");
        assert!(shields.iter().all(|&n| n > 0), "{shields:?}");
    }

    /// About half the production sites are crystal mines, half foundries.
    #[test]
    fn production_sites_are_mines_and_foundries() {
        let g = galaxy();
        let made: Vec<&str> = g
            .sites
            .iter()
            .filter(|s| matches!(s.kind, SiteKind::Production { .. }))
            .map(|s| kind_name(s.kind))
            .collect();
        let mines = made.iter().filter(|&&k| k == CRYSTAL_MINE).count();
        let share = mines as f32 / made.len() as f32;
        assert!((0.35..0.65).contains(&share), "{share}");
    }

    /// A desert world's crystal mines are fought in mines (#523) and its hull
    /// foundries in foundries (#525). Either keeps its mission whoever's looking.
    #[test]
    fn desert_crystal_mines_are_fought_in_mines() {
        for mine in [true, false] {
            let mut g = galaxy();
            for site in &mut g.sites {
                if let SiteKind::Production { hulls, crystals } = &mut site.kind {
                    (*hulls, *crystals) = if mine { (0.0, 0.1) } else { (5.0, 0.0) };
                }
            }
            let mut in_mine = [0; 2];
            for sys in 0..g.systems.len() {
                let sites = g.theater_at(sys, 0).sites;
                for (s, other) in sites.iter().zip(&g.theater_at(sys, 1).sites) {
                    if world(sys, s.planet as usize).0 != Class::Desert
                        || !matches!(s.kind, SiteKind::Production { .. })
                    {
                        continue;
                    }
                    assert_eq!(s.kind_name() == CRYSTAL_MINE, mine);
                    assert_eq!((s.mission, s.map_seed), (other.mission, other.map_seed));
                    in_mine[(s.map_kind() == MapKind::Mine) as usize] += 1;
                }
            }
            assert!(in_mine[mine as usize] > 0, "{in_mine:?}");
            assert_eq!(in_mine[!mine as usize], 0, "{in_mine:?}");
        }
    }

    /// Hull foundries are fought in foundries on every class of world with
    /// a ground of its own, in their founder's style on that ground (#525),
    /// and nothing else is.
    #[test]
    fn foundries_stand_on_their_worlds_ground() {
        let g = galaxy();
        let mut seen = std::collections::HashSet::new();
        for sys in 0..g.systems.len() {
            for s in &g.theater_at(sys, 0).sites {
                let class = world(sys, s.planet as usize).0;
                let foundry = s.kind_name() == "HULL FOUNDRY";
                match (foundry.then(|| class.ground()).flatten(), s.map_kind()) {
                    (Some(ground), kind) => {
                        assert_eq!(kind, MapKind::Foundry(Look::own(s.founder()), ground));
                        seen.insert((ground, s.founder()));
                    }
                    (None, kind) => {
                        assert!(!matches!(kind, MapKind::Foundry(..)), "{class:?}: {kind:?}")
                    }
                }
            }
        }
        assert_eq!(seen.len(), 12, "{seen:?}");
    }

    /// Outposts are fought on no production site's map (#547): on desert
    /// worlds the city, the open desert and the reservoir town, elsewhere the hand-built maps.
    #[test]
    fn outposts_have_maps_of_their_own() {
        let g = galaxy();
        let mut seen = std::collections::HashSet::new();
        for sys in 0..g.systems.len() {
            for s in &g.theater_at(sys, 0).sites {
                if s.kind != SiteKind::Outpost {
                    continue;
                }
                assert_eq!((s.kind_name(), s.short_name()), ("OUTPOST", "OUTPOST"));
                let desert = s.class == Class::Desert;
                match s.map_kind() {
                    MapKind::City(..) | MapKind::Desert | MapKind::Reservoir => assert!(desert),
                    MapKind::Sanctum | MapKind::Kiln => assert!(!desert),
                    k => panic!("an outpost fought on {k:?}"),
                }
                seen.insert(s.mission);
            }
        }
        for name in [
            "BROKEN STREETS",
            "HIGH SEAT",
            "DRY WIND",
            "LONG SHADOWS",
            "DEAD TOWN",
            "LOW WATER",
            "HOLY GROUND",
            "KEPT FIRE",
        ] {
            assert!(
                seen.iter().any(|&i| MISSIONS[i].name == name),
                "no outpost has {name}"
            );
        }
    }

    /// A site is one place: the same map each time it's looked at, and
    /// another site's another one.
    #[test]
    fn a_site_keeps_its_map() {
        let g = galaxy();
        let a = g.theater_at(0, 0).sites;
        let b = g.theater_at(0, 0).sites;
        for (x, y) in a.iter().zip(&b) {
            assert_eq!((x.mission, x.map_seed), (y.mission, y.map_seed));
            assert_eq!(x.conditions(), y.conditions());
        }
        // Whoever takes it, the city's its founder's.
        let shield = a.iter().find(|s| s.is_shield()).unwrap();
        let taken = SiteView {
            owner: (shield.owner + 1) % SIDES,
            ..*shield
        };
        assert_eq!(shield.founder(), taken.founder());
        assert_eq!(shield.conditions(), taken.conditions());
        let seeds: std::collections::HashSet<u64> = a.iter().map(|s| s.map_seed).collect();
        assert_eq!(seeds.len(), a.len());
    }
}
