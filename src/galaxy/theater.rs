//! The theater you're posted to (#311): one contested star system, as the
//! missions screen shows it.
//!
//! **Contested sites.** Every side fighting in the system has a front: on each
//! planet with an enemy site, the enemy's shield generators still up, or, once
//! they're all down, the enemy's sites there. Those are the contested sites,
//! and any of them can be looked at.
//!
//! **Committed sites.** Only the battles your side's automated combat commits
//! to this tick can be entered: the site it attacks (the one the simulation's
//! tick would go for, see `Galaxy::target`) and any of its own sites an enemy
//! attacks. A side at war always has one: an attacker always has a target,
//! and a defender is always being attacked.
//!
//! **Your ship** (#327). You're aboard one of the galaxy's free companies
//! (see `merc`), and the theater is its contract's: the system it's parked
//! over, fighting for the realm that hired it. You stay aboard while it's
//! on the contract; a ship between contracts has no battles to give you, so
//! the Company moves you to the nearest ship that has (see `Galaxy::berth`).
//! While there's no battle for you, the war goes on without you until there
//! is (`Galaxy::await_work`): the galaxy otherwise only ticks with battles.
//!
//! **Maps.** A shield generator is fought over on the shield city (#341), a
//! Hold for the generator: its holder's model, in the shape the site's in
//! (up, wrecked, or going back up as it's repaired). Until there are
//! generated maps for the other sites (#308), each is fought on one of the
//! existing missions' maps, picked at random (stable for the site).

use super::{AT_WAR, Galaxy, REPAIR_TICKS, SIDES, Site, SiteKind};
use crate::economy::{PARTS, Part};
use crate::faction::Faction;
use crate::map::procgen::shield::Condition;
use crate::mission::MISSIONS;
use crate::rng::Rng;
use crate::{Generator, MapKind};

/// The theater as the missions screen shows it: a snapshot of one system.
#[derive(Clone, Debug)]
pub struct Theater {
    /// Index into `Galaxy::systems`.
    pub system: usize,
    pub name: String,
    /// Who you're posted for: index into `Faction::ALL`.
    pub employer: usize,
    pub planets: u8,
    /// Each side's hulls here, and sites held.
    pub hulls: [f32; SIDES],
    pub held: [u8; SIDES],
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
    /// The world it's parked over: where its side's attack goes in this tick.
    pub over: u8,
    /// Its crystal, and hulls still aboard (not in the fight).
    pub crystals: f32,
    pub aboard: f32,
    /// The realm that hired it, by its seat's name, and that realm's people
    /// (index into `Faction::ALL`).
    pub realm: String,
    pub side: usize,
    pub upfront: f32,
    /// Paid if the world falls.
    pub bonus: f32,
    /// Its hulls in the fight, lost so far, and the cap on losses.
    pub field: f32,
    pub lost: f32,
    pub max_loss: f32,
    /// Ticks until the contract runs out.
    pub ticks_left: u64,
    /// The ship you were aboard before, if the Company just moved you off it.
    pub transferred: Option<String>,
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

/// Planet `p` of a system called `name`: "KESSARA II".
pub fn planet_name(name: &str, p: u8) -> String {
    const NUMERALS: [&str; 4] = ["I", "II", "III", "IV"];
    format!("{name} {}", NUMERALS[(p as usize).min(3)])
}

/// The schematic a win at site `site` (index into `Galaxy::sites`) steals
/// from `enemy`: one of theirs you don't have yet, or none (crystals only).
pub fn prize(enemy: Faction, site: usize, owns: impl Fn(Part) -> bool) -> Option<Part> {
    let left: Vec<Part> = PARTS
        .into_iter()
        .filter(|&p| p.maker() == enemy && !p.starter() && !owns(p))
        .collect();
    (!left.is_empty()).then(|| left[site % left.len()])
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
                SiteKind::Shield { down: 0 }
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
        }
    }

    pub fn is_shield(&self) -> bool {
        matches!(self.kind, SiteKind::Shield { .. })
    }

    /// The map it's fought on: its mission's, and for a shield generator,
    /// the holder's model in the shape it's in.
    pub fn map_kind(&self) -> MapKind {
        match (self.kind, MISSIONS[self.mission].map_kind) {
            (SiteKind::Shield { down }, MapKind::ShieldSite(_)) => MapKind::ShieldSite(Generator {
                owner: Faction::ALL[self.owner],
                condition: Condition::down(down, REPAIR_TICKS),
            }),
            (_, kind) => kind,
        }
    }

    /// A generator that's down: how far its repairs have got, (ticks done,
    /// ticks it takes).
    pub fn repairs(&self) -> Option<(u8, u8)> {
        self.down()
            .map(|d| (REPAIR_TICKS.saturating_sub(d), REPAIR_TICKS))
    }

    /// A generator that's down: repair ticks still to go.
    pub fn down(&self) -> Option<u8> {
        match self.kind {
            SiteKind::Shield { down } if down > 0 => Some(down),
            _ => None,
        }
    }

    /// The schematic a win here steals (see `prize`).
    pub fn prize(&self, owns: impl Fn(Part) -> bool) -> Option<Part> {
        prize(Faction::ALL[self.enemy], self.index, owns)
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
            factions: [Faction::ALL[self.employer], Faction::ALL[s.enemy]],
        })
    }

    /// The sides in the war here (hulls or sites), most hulls first.
    pub fn sides(&self) -> Vec<usize> {
        let mut out: Vec<usize> = (0..SIDES)
            .filter(|&f| self.hulls[f] >= AT_WAR || self.held[f] > 0)
            .collect();
        out.sort_by(|&a, &b| self.hulls[b].total_cmp(&self.hulls[a]));
        out
    }

    /// Generators up and down, all sides'.
    pub fn shields(&self) -> (usize, usize) {
        let gens = self.sites.iter().filter(|s| s.is_shield());
        let down = gens.clone().filter(|s| s.down().is_some()).count();
        (gens.count() - down, down)
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
    /// still up; once those are down, its production; its generators last.
    /// The same pick as the tick's `advance` makes first.
    pub fn target(&self, s: usize, f: usize) -> Option<usize> {
        let (up, enemy) = self.in_the_way(s, f);
        let p = (0..self.systems[s].planets as usize)
            .filter(|&p| enemy[p] > 0)
            .min_by_key(|&p| (up[p], enemy[p]))?;
        let candidate = |site: &Site, production: bool| {
            site.planet as usize == p
                && self.hostile(f, site.owner as usize)
                && if up[p] > 0 {
                    site.kind == (SiteKind::Shield { down: 0 })
                } else {
                    matches!(site.kind, SiteKind::Production { .. }) == production
                }
        };
        let r = self.site_range(s);
        r.clone()
            .find(|&i| candidate(&self.sites[i], true))
            .or_else(|| r.clone().find(|&i| candidate(&self.sites[i], false)))
    }

    /// The sites side `f` is fighting for in system `s`: on each planet with an
    /// enemy site, the enemy generators up, or (all down) the enemy's sites.
    pub fn front(&self, s: usize, f: usize) -> Vec<usize> {
        if !self.fighting(s, f) {
            return Vec::new();
        }
        let (up, _) = self.in_the_way(s, f);
        self.site_range(s)
            .filter(|&i| {
                let site = &self.sites[i];
                self.hostile(f, site.owner as usize)
                    && (up[site.planet as usize] == 0
                        || site.kind == (SiteKind::Shield { down: 0 }))
            })
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

    /// The ships you could be aboard: companies on a contract with a battle
    /// to join this tick (indices into `companies`).
    fn berths(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.companies.len()).filter(|&c| {
            self.companies[c]
                .contract
                .is_some_and(|k| !self.commits(k.target as usize, k.side as usize).is_empty())
        })
    }

    /// The ship you're aboard (an index into `companies`), if it has a
    /// battle for you this tick. You stay aboard `keep` (a company's id)
    /// while it's on a contract: none if that has no battle this tick. Off
    /// contract (or gone), the Company moves you to the nearest ship that
    /// has a battle (nearest `keep`'s ship, if it's still about); a new
    /// pilot goes to the one in the biggest fight. `prefer` (a side) goes
    /// first, for shots and tests.
    pub fn berth(&self, keep: Option<u32>, prefer: Option<usize>) -> Option<usize> {
        let old = keep.and_then(|id| self.companies.iter().position(|c| c.id == id));
        if let Some(c) = old.filter(|&c| self.companies[c].contract.is_some()) {
            return self.berths().find(|&b| b == c);
        }
        let from = old.map(|c| self.systems[self.companies[c].at as usize].pos);
        self.berths().max_by(|&a, &b| {
            let (ka, kb) = (
                self.companies[a].contract.unwrap(),
                self.companies[b].contract.unwrap(),
            );
            let side = |k: super::merc::Contract| prefer == Some(k.side as usize);
            let near = |k: super::merc::Contract| {
                from.map_or(0.0, |p| -self.systems[k.target as usize].pos.distance(p))
            };
            let sites = |k: super::merc::Contract| self.site_range(k.target as usize).len();
            side(ka)
                .cmp(&side(kb))
                .then(near(ka).total_cmp(&near(kb)))
                .then(sites(ka).cmp(&sites(kb)))
                .then(ka.field.total_cmp(&kb.field))
                .then(b.cmp(&a))
        })
    }

    /// The theater aboard your ship, as `aboard`; with no battle for you
    /// this tick, the war goes on without you (up to `ticks` ticks) until
    /// there is one. True in the second if the galaxy ticked.
    pub fn await_work(
        &mut self,
        keep: Option<u32>,
        prefer: Option<usize>,
        rank: u32,
        ticks: u32,
    ) -> (Option<Theater>, bool) {
        for i in 0..=ticks {
            if let Some(t) = self.aboard(keep, prefer, rank) {
                return (Some(t), i > 0);
            }
            if i < ticks {
                self.step();
            }
        }
        (None, ticks > 0)
    }

    /// The theater aboard your ship (see `berth`): its contract's system,
    /// for the side that hired it. None if there's no battle for you this tick.
    pub fn aboard(&self, keep: Option<u32>, prefer: Option<usize>, rank: u32) -> Option<Theater> {
        let c = self.berth(keep, prefer)?;
        let co = &self.companies[c];
        let k = co.contract?;
        let (s, f) = (k.target as usize, k.side as usize);
        let mut t = self.theater_at(s, f, rank);
        let over = self.target(s, f).map_or(0, |i| self.sites[i].planet);
        t.ship = Some(Ship {
            id: co.id,
            name: ship_name(co.id),
            over,
            crystals: co.crystals,
            aboard: co.hulls,
            realm: system_name(self.realms[k.realm as usize].seat as usize),
            side: f,
            upfront: k.upfront,
            bonus: k.bonus,
            field: k.field,
            lost: k.lost,
            max_loss: k.max_loss,
            ticks_left: (k.signed + self.rules.merc_term as u64).saturating_sub(self.tick),
            transferred: keep.filter(|&id| id != co.id).map(ship_name),
        });
        Some(t)
    }

    /// System `s` as side `f` sees it, with maps from the missions open at `rank`.
    pub fn theater_at(&self, s: usize, f: usize, rank: u32) -> Theater {
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
            .max_by(|&a, &b| sys.hulls[a].total_cmp(&sys.hulls[b]))
            .unwrap_or((f + 1) % SIDES);
        let pool = crate::mission::for_sites(rank);
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
                SiteView {
                    index: i,
                    planet: site.planet,
                    owner,
                    kind: site.kind,
                    contested: contested.contains(&i),
                    committed: commits.contains(&i),
                    enemy,
                    mission: match (site.kind, pool[r.index(pool.len())]) {
                        (SiteKind::Shield { .. }, _) => shield,
                        (_, m) => m,
                    },
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
            held: sys.held,
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

    /// A small war, ticked on until some ship has a battle to join.
    fn galaxy_with_work() -> Galaxy {
        let mut g = galaxy();
        assert!(
            g.await_work(None, None, 1, 400).0.is_some(),
            "no work in 460 ticks"
        );
        g
    }

    /// Every ship on a contract with a battle to join keeps you aboard, and
    /// there's a battle for you there, for the side that hired it.
    #[test]
    fn aboard_a_ship_there_is_a_battle_to_join() {
        let g = galaxy_with_work();
        let ships: Vec<usize> = g.berths().collect();
        assert!(!ships.is_empty(), "no ship on a contract");
        for c in ships {
            let co = &g.companies[c];
            let k = co.contract.unwrap();
            let t = g.aboard(Some(co.id), None, 1).expect("a theater");
            let ship = t.ship.as_ref().unwrap();
            assert_eq!(ship.id, co.id);
            assert_eq!(ship.transferred, None);
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

    /// A new pilot goes aboard the ship in the biggest fight (or one working
    /// for the side asked for); one whose ship's contract is over, or whose
    /// ship is gone, is moved to the nearest ship that has a fight.
    #[test]
    fn the_company_moves_you_off_an_idle_ship() {
        let mut g = galaxy_with_work();
        let first = g.berth(None, None).expect("a ship");
        for f in 0..SIDES {
            if let Some(c) = g.berth(None, Some(f))
                && g.berths()
                    .any(|b| g.companies[b].contract.unwrap().side as usize == f)
            {
                assert_eq!(g.companies[c].contract.unwrap().side as usize, f);
            }
        }
        let id = g.companies[first].id;
        let at = g.companies[first].contract.unwrap().target as usize;
        g.companies[first].contract = None;
        if g.berths().next().is_none() {
            return; // Its was the only work going.
        }
        let t = g.aboard(Some(id), None, 1).expect("another ship");
        let ship = t.ship.unwrap();
        assert_ne!(ship.id, id);
        assert_eq!(ship.transferred, Some(ship_name(id)));
        let d = |s: usize| g.systems[s].pos.distance(g.systems[at].pos);
        for b in g.berths() {
            assert!(d(t.system) <= d(g.companies[b].contract.unwrap().target as usize));
        }
        // A ship that's gone (joined another) moves you too.
        assert!(
            g.aboard(Some(u32::MAX), None, 1)
                .unwrap()
                .ship
                .unwrap()
                .transferred
                .is_some()
        );
    }

    /// While your ship is on its contract you stay aboard, even on a tick
    /// with no battle there for you.
    #[test]
    fn you_stay_aboard_while_the_contract_lasts() {
        let mut g = galaxy_with_work();
        let c = g.berth(None, None).unwrap();
        let id = g.companies[c].id;
        for _ in 0..40 {
            if g.companies
                .iter()
                .find(|co| co.id == id)
                .is_none_or(|co| co.contract.is_none())
            {
                break;
            }
            match g.berth(Some(id), None) {
                Some(b) => assert_eq!(g.companies[b].id, id),
                None => assert!(g.aboard(Some(id), None, 1).is_none()),
            }
            g.step();
        }
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
        let t = g.await_work(None, None, 1, 400).0.unwrap();
        for s in &t.sites {
            match (s.kind, s.map_kind()) {
                (SiteKind::Shield { down }, MapKind::ShieldSite(made)) => {
                    assert_eq!(MISSIONS[s.mission].contract, crate::mission::Contract::Hold);
                    assert_eq!(made.owner, Faction::ALL[s.owner]);
                    assert_eq!(made.condition, Condition::down(down, REPAIR_TICKS));
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

    /// A site's map is one of the missions open at your rank, on two sides.
    #[test]
    fn sites_get_open_two_sided_maps() {
        let mut g = galaxy();
        let t = g.await_work(None, None, 1, 400).0.unwrap();
        for s in &t.sites {
            let m = &MISSIONS[s.mission];
            assert!(m.open_at(1) && m.sides.len() == 2, "{}", m.name);
        }
    }
}
