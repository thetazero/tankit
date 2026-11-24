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
//! **Maps.** A shield generator is fought over on the shield city (#341), a
//! Hold for the generator: its holder's model, in the shape the site's in
//! (up, wrecked, or going back up as it's repaired). Every other site is
//! fought on one of the missions' maps for its planet's class (`Class`),
//! picked at random, and on a generated kind the one map its seed names
//! (`SiteView::map_seed`): the site is one place, the same each visit.

use super::{AT_WAR, Galaxy, SIDES, Site, SiteKind};
use crate::faction::Faction;
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
    /// Each side's hulls here, hulls its yards here build a tick, and sites held.
    pub hulls: [u32; SIDES],
    pub built: [f32; SIDES],
    pub held: [u8; SIDES],
    /// Each side's gear tier here (#438): its realm's, in upgrade steps.
    pub tiers: [u8; SIDES],
    /// How each side's hulls here came and went since your last battle
    /// began (#445; `Galaxy::moves`): reinforcements, mercs, wear.
    pub moves: [super::Moves; SIDES],
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
    /// What just happened to the company, if anything.
    pub news: Option<CrewNews>,
    /// Its hulls still committed to the contract (in the fight), and of them yours alone (#385): left, of how many.
    pub committed: u32,
    pub own: Own,
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

/// What happened to your company that the missions screen tells you.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CrewNews {
    /// It joined another (yours now): `from` was its id.
    Joined { from: u32 },
    /// It had no work for long, and sailed from system `from` for some.
    Sailed { from: u32 },
    /// It's gone, and the Company put you with another: `from` was its id.
    Gone { from: u32 },
    /// It split in two, and you went with this half: `other` is the other's id.
    Split { other: u32 },
    /// It joined another, and you went with this one instead: `from` was its id.
    Left { from: u32 },
    /// You left it for this one (#365): `from` was its id.
    Switched { from: u32 },
}

/// Your company just split or joined another: the two ships you can go
/// with (#364). You're with the first until you choose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrewChoice {
    /// Your company as it was (`Company::id`).
    pub from: u32,
    /// It joined another (else it split): the first ship is the one it
    /// joined, the second the next nearest company.
    pub joined: bool,
    pub ships: [u32; 2],
}

/// A choice of ship, as the missions screen shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct ChoiceView {
    pub choice: CrewChoice,
    pub ships: [Berth; 2],
}

/// A ship you could go with: where it is, what it has, its work.
#[derive(Clone, Debug, PartialEq)]
pub struct Berth {
    pub id: u32,
    pub name: String,
    /// The system it's over.
    pub system: String,
    pub hulls: u32,
    pub crystals: f32,
    /// On contract: what for (see `Work`).
    pub work: Option<Work>,
}

/// A company's contract, as the missions screen names it.
#[derive(Clone, Debug, PartialEq)]
pub struct Work {
    /// The side that hired it (index into `Faction::ALL`), and the realm by
    /// its seat's name.
    pub side: usize,
    pub realm: String,
    /// The system it's fighting over, and whether to hold it (else take it).
    pub system: String,
    pub defend: bool,
}

/// One site of the theater.
#[derive(Clone, Copy, Debug)]
pub struct SiteView {
    /// Index into `Galaxy::sites`.
    pub index: usize,
    pub planet: u8,
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
    /// The match's teams: you (your employer) first, then the enemy.
    pub factions: [Faction; 2],
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
    /// A desert world: its sites are generated (cities, mines, open desert).
    Desert,
    /// Not made yet: its sites are fought on the hand-built maps.
    Placeholder,
}

impl Class {
    /// Whether a mission's map is one this class's sites are fought on.
    pub fn fights_on(self, kind: MapKind) -> bool {
        match kind {
            MapKind::City | MapKind::Mine | MapKind::Desert => self == Class::Desert,
            MapKind::Saltmarket | MapKind::Sanctum | MapKind::Kiln | MapKind::Foundry => {
                self == Class::Placeholder
            }
            MapKind::ShieldCity(_) | MapKind::ShieldSite(_) | MapKind::ShieldDesert(_) => false,
        }
    }
}

/// Planet `planet` of system `system`, the same for every pilot: its
/// class, and how it looks from orbit (the seed and look its globe is made
/// from). A desert world is dry; a placeholder one any of the others.
pub fn world(system: usize, planet: usize) -> (Class, u64, crate::planet::Look) {
    let seed = (system as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ ((planet as u64 + 1) * 0x5851);
    let worlds = [
        (Class::Desert, crate::planet::DRY),
        (Class::Placeholder, crate::map::procgen::city::PLANET),
        (Class::Placeholder, crate::planet::ICE),
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

impl SiteView {
    /// What it is, in caps: a shield generator, or what a production site makes most of.
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            SiteKind::Shield { .. } => "SHIELD GENERATOR",
            SiteKind::Production { hulls, crystals } if crystals * 5.0 > hulls => "CRYSTAL MINE",
            SiteKind::Production { .. } => "HULL FOUNDRY",
        }
    }

    /// A stand-in site, for keys and legends: a shield generator, or a crystal mine.
    pub fn example(shield: bool) -> SiteView {
        SiteView {
            index: 0,
            planet: 0,
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

    /// The map it's fought on: its mission's, and for a shield generator,
    /// the holder's model in the shape it's in.
    pub fn map_kind(&self) -> MapKind {
        match (self.kind, MISSIONS[self.mission].map_kind) {
            (SiteKind::Shield { damaged }, MapKind::ShieldSite(_)) => {
                MapKind::ShieldSite(Generator {
                    owner: Faction::ALL[self.owner],
                    condition: if damaged {
                        Condition::DAMAGED
                    } else {
                        Condition::Up
                    },
                })
            }
            (_, kind) => kind,
        }
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
            factions: [Faction::ALL[self.employer], Faction::ALL[s.enemy]],
        })
    }

    /// The sides in the war here (hulls or sites), most hulls first.
    pub fn sides(&self) -> Vec<usize> {
        let mut out: Vec<usize> = (0..SIDES)
            .filter(|&f| self.hulls[f] >= AT_WAR || self.held[f] > 0)
            .collect();
        out.sort_by_key(|&a| std::cmp::Reverse(self.hulls[a]));
        out
    }
}

impl Galaxy {
    fn present(&self, s: usize) -> [bool; SIDES] {
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

    /// Each planet of system `s`: (enemy generators up, enemy sites), as `f` sees them.
    fn in_the_way(&self, s: usize, f: usize) -> ([u8; 4], [u8; 4]) {
        let up = self.shields_up(s, f);
        let mut enemy = [0u8; 4];
        for site in &self.sites[self.site_range(s)] {
            if self.hostile(f, site.owner as usize) {
                enemy[site.planet as usize] += 1;
            }
        }
        (up, enemy)
    }

    /// The site side `f` would go for in system `s` this tick, if it won the
    /// tick: the planet with least in its way, and on it an enemy generator
    /// still up (it costs a seat while it is: `shielded`); else its
    /// production; its downed generators last. The pick the tick's `advance`
    /// makes.
    pub fn target(&self, s: usize, f: usize) -> Option<usize> {
        let (up, enemy) = self.in_the_way(s, f);
        let p = (0..self.systems[s].planets as usize)
            .filter(|&p| enemy[p] > 0)
            .min_by_key(|&p| (up[p], enemy[p]))?;
        let candidate = |site: &Site, production: bool| {
            site.planet as usize == p
                && self.hostile(f, site.owner as usize)
                && if production {
                    matches!(site.kind, SiteKind::Production { .. })
                } else {
                    matches!(site.kind, SiteKind::Shield { .. })
                }
        };
        let r = self.site_range(s);
        let up = |i: &usize| {
            let site = &self.sites[*i];
            candidate(site, false) && site.kind == (SiteKind::Shield { damaged: false })
        };
        r.clone()
            .find(up)
            .or_else(|| r.clone().find(|&i| candidate(&self.sites[i], true)))
            .or_else(|| r.clone().find(|&i| candidate(&self.sites[i], false)))
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
    /// attack, and the defense of each of its sites an enemy attacks.
    pub fn commits(&self, s: usize, f: usize) -> Vec<usize> {
        if !self.fighting(s, f) {
            return Vec::new();
        }
        let present = self.present(s);
        let mut out: Vec<usize> = self.target(s, f).into_iter().collect();
        for g in (0..SIDES).filter(|&g| present[g] && self.hostile(f, g) && self.fighting(s, g)) {
            if let Some(t) = self.target(s, g)
                && self.sites[t].owner as usize == f
                && !out.contains(&t)
            {
                out.push(t);
            }
        }
        out.sort();
        out
    }

    /// The ships with work for you: companies on a contract with a battle
    /// to join this tick (indices into `companies`).
    fn berths(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.companies.len()).filter(|&c| {
            self.companies[c]
                .contract
                .is_some_and(|k| !self.commits(k.target as usize, k.side as usize).is_empty())
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

    /// Put a pilot with no company (or whose company is gone) with one: the
    /// one in the biggest fight this tick. `prefer` (a side) goes first, for
    /// shots and tests. Nothing happens while you have a company, or while
    /// no company has a fight to give you.
    pub fn enlist(&mut self, prefer: Option<usize>) {
        if self.crew_company().is_some() {
            return;
        }
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
        let Some(c) = best else { return };
        if let Some(from) = self.crew {
            self.crew_news = Some(CrewNews::Gone { from });
        }
        self.crew = Some(self.companies[c].id);
    }

    /// The choice of ship open to you, if any (dropped once either ship's
    /// gone, or you're with neither).
    pub fn crew_choice(&mut self) -> Option<ChoiceView> {
        let choice = self.crew_choice?;
        let open = self.crew.is_some_and(|c| choice.ships.contains(&c))
            && choice.ships.iter().all(|&id| self.company(id).is_some());
        if !open {
            self.crew_choice = None;
            return None;
        }
        let ships = choice.ships.map(|id| self.berth(self.company(id).unwrap()));
        Some(ChoiceView { choice, ships })
    }

    /// Company `c` (an index into `companies`) as a ship you could go with.
    fn berth(&self, c: usize) -> Berth {
        let co = &self.companies[c];
        Berth {
            id: co.id,
            name: ship_name(co.id),
            system: system_name(co.at as usize),
            hulls: co.hulls + co.contract.map_or(0, |k| k.field),
            crystals: co.crystals,
            work: co.contract.map(|k| Work {
                side: k.side as usize,
                realm: system_name(self.realms[k.realm as usize].seat as usize),
                system: system_name(k.target as usize),
                defend: k.defend,
            }),
        }
    }

    /// The ships you could switch to (#365): up to `n` other companies with
    /// a battle for you this tick, nearest your ship first (biggest fight
    /// first while you're with none).
    pub fn other_ships(&self, n: usize) -> Vec<Berth> {
        let mine = self.crew_company();
        let here = mine.map(|c| self.systems[self.companies[c].at as usize].pos);
        let mut ships: Vec<usize> = self.berths().filter(|&c| Some(c) != mine).collect();
        let key = |c: usize| match here {
            Some(h) => self.systems[self.companies[c].at as usize].pos.distance(h),
            None => -(self.companies[c].contract.map_or(0, |k| k.field) as f32),
        };
        ships.sort_by(|&a, &b| key(a).total_cmp(&key(b)).then(a.cmp(&b)));
        ships.truncate(n);
        ships.into_iter().map(|c| self.berth(c)).collect()
    }

    /// Leave your company for ship `id` (#365): another company with a
    /// battle for you this tick. Any choice of ship open is dropped, and
    /// your own hulls stay with the contract you leave. False (and nothing
    /// changes) if it's yours, or has no battle for you.
    pub fn switch_crew(&mut self, id: u32) -> bool {
        let Some(c) = self.company(id) else {
            return false;
        };
        if self.crew == Some(id) || !self.berths().any(|b| b == c) {
            return false;
        }
        self.crew_news = self.crew.map(|from| CrewNews::Switched { from });
        self.crew = Some(id);
        self.crew_choice = None;
        true
    }

    /// Go with ship `id`, one of those the open choice offers: the choice
    /// is made. False (and nothing changes) if it isn't one of them.
    pub fn choose_crew(&mut self, id: u32) -> bool {
        let Some(view) = self.crew_choice() else {
            return false;
        };
        let c = view.choice;
        let Some(k) = c.ships.iter().position(|&s| s == id) else {
            return false;
        };
        self.crew = Some(id);
        self.crew_choice = None;
        self.crew_news = match (c.joined, k) {
            (true, 0) => Some(CrewNews::Joined { from: c.from }),
            (true, _) => Some(CrewNews::Left { from: c.from }),
            (false, _) => Some(CrewNews::Split {
                other: c.ships[1 - k],
            }),
        };
        true
    }

    /// The theater with your company (see `enlist`); while it has no battle
    /// for you, the war goes on without you (up to `ticks` ticks) until it
    /// has, or until you have a ship to choose (`crew_choice`). Idle long enough, it goes looking for work like any company
    /// (`Galaxy::seek_work`, in the galaxy's tick). True in the second if
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
            if self.crew_choice().is_some() {
                return (self.aboard(rank), i > 0);
            }
            if let Some(t) = self.aboard(rank) {
                return (Some(t), i > 0);
            }
            if i < ticks {
                self.step();
            }
        }
        (None, ticks > 0)
    }

    /// `await_work`, but any choice of ship on the way is made for you:
    /// you stay with the one you're on (for shots and tests, where there's
    /// nobody to choose).
    pub fn await_work_staying(
        &mut self,
        prefer: Option<usize>,
        rank: u32,
        ticks: u32,
    ) -> (Option<Theater>, bool) {
        let (mut t, mut ticked) = self.await_work(prefer, rank, ticks);
        while let Some(crew) = self.crew.filter(|_| self.crew_choice.is_some()) {
            self.choose_crew(crew);
            self.crew_choice = None;
            let (next, more) = self.await_work(prefer, rank, ticks);
            (t, ticked) = (next, ticked || more);
        }
        (t, ticked)
    }

    /// Your own hulls on contract `k` of company `ship` (#385): the ones
    /// you have, if they're on it; else the ones you'd get at `rank`.
    fn own_on(&self, ship: u32, k: super::merc::Contract, rank: u32) -> Own {
        match self.own {
            Some(o) if o.ship == ship && o.signed == k.signed => o,
            _ => {
                let half = k.field.div_ceil(2);
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
        let on = self.crew_company().and_then(|c| {
            let co = &self.companies[c];
            co.contract.map(|k| (co.id, k))
        });
        self.own = on.map(|(ship, k)| self.own_on(ship, k, rank));
    }

    /// Your own hulls left on your company's contract (0 if none are set).
    pub fn own_left(&self) -> u32 {
        let on = self.crew_company().and_then(|c| {
            let co = &self.companies[c];
            co.contract.map(|k| (co.id, k.signed))
        });
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
    /// the side that hired it. None if you have no company, or it has no
    /// battle for you this tick.
    pub fn aboard(&self, rank: u32) -> Option<Theater> {
        let c = self.crew_company()?;
        let co = &self.companies[c];
        let k = co.contract?;
        let (s, f) = (k.target as usize, k.side as usize);
        if self.commits(s, f).is_empty() {
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
        let over = if k.defend {
            attacked.or(self.target(s, f))
        } else {
            self.target(s, f).or(attacked)
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
            ticks_left: (k.signed + self.rules.merc_term as u64).saturating_sub(self.tick),
            signed: k.signed,
            news: self.crew_news,
            committed: k.field,
            own: self.own_on(co.id, k, rank),
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
            news: None,
            committed: 0,
            own: Own::default(),
            ended: Vec::new(),
        });
        Some(t)
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
        // Who attacks each site this tick.
        let mut attacker: Vec<Option<usize>> = vec![None; self.site_range(s).len()];
        for g in (0..SIDES).filter(|&g| self.fighting(s, g)) {
            if let Some(t) = self.target(s, g) {
                attacker[t - sys.sites.0 as usize].get_or_insert(g);
            }
        }
        let strongest = (0..SIDES)
            .filter(|&g| present[g] && self.hostile(f, g))
            .max_by_key(|&a| sys.hulls[a])
            .unwrap_or((f + 1) % SIDES);
        let pools = [Class::Desert, Class::Placeholder].map(|c| crate::mission::for_sites(c));
        let shield = crate::mission::for_shields();
        let sites: Vec<SiteView> = self
            .site_range(s)
            .map(|i| {
                let site = self.sites[i];
                let owner = site.owner as usize;
                let enemy = if self.hostile(f, owner) {
                    owner
                } else {
                    attacker[i - sys.sites.0 as usize]
                        .filter(|&g| self.hostile(f, g))
                        .unwrap_or(strongest)
                };
                let mut r = Rng::new(self.seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                let pool = match world(s, site.planet as usize).0 {
                    Class::Desert => &pools[0],
                    Class::Placeholder => &pools[1],
                };
                SiteView {
                    index: i,
                    planet: site.planet,
                    owner,
                    kind: site.kind,
                    contested: contested.contains(&i),
                    committed: commits.contains(&i),
                    enemy,
                    mission: match site.kind {
                        SiteKind::Shield { .. } => shield,
                        _ => pool[r.index(pool.len())],
                    },
                    map_seed: r.next_u64(),
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
            built: self.built(s),
            held: sys.held,
            tiers: std::array::from_fn(|g| match sys.realms[g] {
                super::realm::NONE => 0,
                r => self.realms[r as usize].tier,
            }),
            moves: self.moves[s],
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
                let Some(t) = g.aboard(1) else { continue };
                let k = g.companies[g.crew_company().unwrap()].contract.unwrap();
                let ship = t.ship.as_ref().unwrap();
                assert!(ship.defend);
                assert_eq!((t.system, t.employer), (k.target as usize, k.side as usize));
                let committed: Vec<&SiteView> = t.sites.iter().filter(|s| s.committed).collect();
                let ours: Vec<&&SiteView> =
                    committed.iter().filter(|s| s.owner == t.employer).collect();
                if g.target(t.system, t.employer).is_none() {
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
                    let sides = [Faction::ALL[t.employer], Faction::ALL[s.enemy]];
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
            assert_eq!(g.crew_news, None);
        }
    }

    /// You stay with your company through the war: between contracts, and
    /// when it has no battle for you, it's still yours (#350).
    #[test]
    fn you_stay_with_your_company() {
        let mut g = galaxy_with_work();
        let id = g.crew.unwrap();
        let mut idle = 0;
        for _ in 0..300 {
            if g.crew != Some(id) {
                // Only by joining another company.
                assert_eq!(g.crew_news, Some(CrewNews::Joined { from: id }));
                return;
            }
            assert!(g.crew_company().is_some(), "your company's gone");
            match g.aboard(1) {
                Some(t) => assert_eq!(t.ship.unwrap().id, id),
                None => idle += 1,
            }
            g.enlist(None);
            g.step();
        }
        assert!(idle > 0, "never between contracts: the test shows nothing");
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
        assert!(g.seek_work() >= 1);
        assert_eq!(g.crew, Some(id));
        let c = g.crew_company().unwrap();
        let to = g.companies[c].at as usize;
        assert_ne!(to as u32, at);
        assert!(g.systems[to].at_war());
        assert_eq!(g.companies[c].idle_since, g.tick);
        assert_eq!(g.crew_news, Some(CrewNews::Sailed { from: at }));
    }

    /// When your company joins another, you go with it.
    #[test]
    fn you_go_with_your_company_when_it_joins_another() {
        let mut g = galaxy_with_work();
        let c = (0..g.companies.len())
            .find(|&c| g.companies[c].contract.is_none())
            .expect("an idle company");
        let id = g.companies[c].id;
        g.crew = Some(id);
        (g.companies[c].crystals, g.companies[c].hulls) = (0.0, 0);
        g.split_and_join();
        assert!(g.company(id).is_none(), "it didn't join");
        assert_ne!(g.crew, Some(id));
        assert!(g.crew_company().is_some());
        assert_eq!(g.crew_news, Some(CrewNews::Joined { from: id }));
        // And you may go with the next nearest instead (#364).
        let v = g.crew_choice().expect("a choice");
        assert!(v.choice.joined);
        assert_eq!(v.choice.from, id);
        assert_eq!(Some(v.ships[0].id), g.crew);
        let other = v.ships[1].id;
        assert!(g.choose_crew(other));
        assert_eq!(g.crew, Some(other));
        assert_eq!(g.crew_news, Some(CrewNews::Left { from: id }));
        assert!(g.crew_choice().is_none());
    }

    /// You can leave your company for another ship with a battle for you
    /// (#365): its theater is yours, and the missions screen says so.
    #[test]
    fn you_can_switch_to_another_company() {
        let mut g = galaxy_with_work();
        for _ in 0..400 {
            if !g.other_ships(1).is_empty() {
                break;
            }
            g.step();
            g.await_work_staying(None, 1, 400);
        }
        let mine = g.crew.unwrap();
        let ships = g.other_ships(5);
        assert!(!ships.is_empty(), "no other ship at work");
        assert!(ships.len() <= 5);
        assert!(ships.iter().all(|b| b.id != mine && b.work.is_some()));
        // Nearest first.
        let here = g.systems[g.companies[g.crew_company().unwrap()].at as usize].pos;
        let d = |b: &Berth| {
            let c = g.company(b.id).unwrap();
            g.systems[g.companies[c].at as usize].pos.distance(here)
        };
        assert!(ships.windows(2).all(|w| d(&w[0]) <= d(&w[1])));
        // Not yours, nor a ship that's gone or has no battle.
        assert!(!g.switch_crew(mine));
        assert!(!g.switch_crew(u32::MAX));
        let idle = g
            .companies
            .iter()
            .find(|c| c.contract.is_none())
            .map(|c| c.id);
        if let Some(idle) = idle {
            assert!(!g.switch_crew(idle));
        }
        assert_eq!(g.crew, Some(mine));
        let to = ships[0].id;
        assert!(g.switch_crew(to));
        assert_eq!(g.crew, Some(to));
        assert_eq!(g.crew_news, Some(CrewNews::Switched { from: mine }));
        let t = g.aboard(1).expect("a battle aboard the new ship");
        assert_eq!(t.ship.unwrap().id, to);
        // It's among the ships you could go back to, and yours isn't.
        assert!(g.other_ships(usize::MAX).iter().all(|b| b.id != to));
    }

    /// Switching drops a choice of ship left open.
    #[test]
    fn switching_closes_a_choice_of_ship() {
        let mut g = galaxy_with_work();
        let _ = your_company_splits(&mut g);
        let to = g.other_ships(1)[0].id;
        assert!(g.switch_crew(to));
        assert!(g.crew_choice().is_none());
        assert_eq!(g.crew, Some(to));
    }

    /// An idle company of yours too rich for one ship.
    fn your_company_splits(g: &mut Galaxy) -> (u32, u32) {
        let c = (0..g.companies.len())
            .find(|&c| g.companies[c].contract.is_none())
            .expect("an idle company");
        let id = g.companies[c].id;
        g.crew = Some(id);
        g.companies[c].crystals = g.rules.merc_split * 1.6;
        g.split_and_join();
        let v = g.crew_choice().expect("a choice");
        assert!(!v.choice.joined);
        assert_eq!(v.choice.ships[0], id);
        (id, v.choice.ships[1])
    }

    /// When your company splits, you're with the half that keeps its id
    /// until you choose; you may go with the other (#364).
    #[test]
    fn you_choose_which_half_of_a_split_you_go_with() {
        let mut g = galaxy_with_work();
        let (id, new) = your_company_splits(&mut g);
        assert_ne!(id, new);
        assert_eq!(g.crew, Some(id));
        let v = g.crew_choice().unwrap();
        assert_eq!(v.ships[0].crystals, v.ships[1].crystals);
        // Not a ship on offer: nothing changes.
        assert!(!g.choose_crew(u32::MAX));
        assert_eq!(g.crew, Some(id));
        assert!(g.choose_crew(new));
        assert_eq!(g.crew, Some(new));
        assert_eq!(g.crew_news, Some(CrewNews::Split { other: id }));
        assert!(g.crew_choice().is_none());
    }

    /// Staying is a choice too.
    #[test]
    fn you_may_stay_with_your_half() {
        let mut g = galaxy_with_work();
        let (id, new) = your_company_splits(&mut g);
        assert!(g.choose_crew(id));
        assert_eq!(g.crew, Some(id));
        assert_eq!(g.crew_news, Some(CrewNews::Split { other: new }));
    }

    /// The war waits on your choice: no ticks go by on the missions screen
    /// while it's open.
    #[test]
    fn the_war_waits_on_your_choice() {
        let mut g = galaxy_with_work();
        your_company_splits(&mut g);
        let tick = g.tick;
        let (_, ticked) = g.await_work(None, 1, 50);
        assert!(!ticked);
        assert_eq!(g.tick, tick);
    }

    /// A choice with one of its ships gone is no choice.
    #[test]
    fn a_choice_closes_when_a_ship_is_gone() {
        let mut g = galaxy_with_work();
        let (_, new) = your_company_splits(&mut g);
        let c = g.company(new).unwrap();
        g.companies.remove(c);
        assert!(g.crew_choice().is_none());
        assert!(!g.choose_crew(new));
    }

    /// A pilot whose company is gone (say, a save from another galaxy) is put
    /// with another, and told.
    #[test]
    fn a_pilot_whose_company_is_gone_gets_another() {
        let mut g = galaxy_with_work();
        g.crew = Some(u32::MAX);
        g.enlist(None);
        assert!(g.crew_company().is_some());
        assert_eq!(g.crew_news, Some(CrewNews::Gone { from: u32::MAX }));
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

    /// A shield generator is fought over on the shield city, a Hold for
    /// it, its holder's model in the shape it's in (#341); nothing else is.
    #[test]
    fn generators_are_fought_over_on_the_shield_city() {
        let mut g = galaxy();
        let t = g.await_work_staying(None, 1, 400).0.unwrap();
        for s in &t.sites {
            match (s.kind, s.map_kind()) {
                (SiteKind::Shield { damaged }, MapKind::ShieldSite(made)) => {
                    assert_eq!(MISSIONS[s.mission].contract, crate::mission::Contract::Hold);
                    assert_eq!(made.owner, Faction::ALL[s.owner]);
                    let shape = if damaged {
                        Condition::DAMAGED
                    } else {
                        Condition::Up
                    };
                    assert_eq!(made.condition, shape);
                }
                (SiteKind::Shield { .. }, k) => panic!("a generator fought on {k:?}"),
                (_, k) => assert!(!matches!(k, MapKind::ShieldSite(_)), "{k:?}"),
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
    /// a shield generator is the shield city on either.
    #[test]
    fn a_sites_map_is_its_worlds_class() {
        let g = galaxy();
        let mut seen = [false; 2];
        for sys in 0..g.systems.len() {
            for s in &g.theater_at(sys, 0).sites {
                let class = world(sys, s.planet as usize).0;
                seen[class as usize] = true;
                let kind = s.map_kind();
                if s.is_shield() {
                    assert!(matches!(kind, MapKind::ShieldSite(_)), "{kind:?}");
                } else {
                    assert!(class.fights_on(kind), "{class:?} on {kind:?}");
                }
            }
        }
        assert_eq!(seen, [true; 2], "both classes of world have sites");
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
        }
        let seeds: std::collections::HashSet<u64> = a.iter().map(|s| s.map_seed).collect();
        assert_eq!(seeds.len(), a.len());
    }
}
