//! The battles the player plays, as part of the galaxy's war (#304).
//!
//! **In.** A battle is fought in one system, between the mission's sides. Each
//! side gets a **hull allocation**: its hulls in that system, the same ones
//! the galaxy's own battle there would fight with (hulls from other systems
//! only arrive the galaxy's way, sent on a tick and paid for). Every tank a
//! side puts in play, the first ones included, is one of those hulls; when
//! they're spent, the side has no more tanks. The seats (tanks in play at
//! once) stay capped as before.
//!
//! **Shields** (#426). While the site's holder has a shield generator up on
//! the site's planet, every other side (its mercs among it) fights
//! `SHIELD_SEATS` seats short: its last seats stay empty, so its frontage
//! is smaller and it takes more casualties than the defender. The holder is
//! never short. That's all shields do here, the same as in the galaxy's
//! battles.
//!
//! **Out.** When the battle's done, each side's hulls lost (its tanks
//! destroyed) come off the system's storage; survivors stay there. A win at the losers' site takes it,
//! whatever the contract (#419); a Raid's also knocks down one of their
//! generators (see [`Stake`]). It's that system's battle for the tick:
//! the rest of the galaxy ticks once with it (`Galaxy::step_played`).
//!
//! **Mercs** (#320). A company under contract for the battle's system (to
//! take it, or to hold it: #340) has its hulls there among its employer's side's, so a side's allocation
//! drawn from that system is part merc, in proportion: if a third of the
//! side's hulls here are a company's, a third of what it brings is. (Mercs
//! on contract elsewhere stay on their own contracts.) The side's **seats** are split by the mercs' share of
//! its force in the system itself (the attack they were hired to make up),
//! rounded, but never more seats than merc hulls, and never all of them
//! while the side brings any hulls of the realm's own: that many of its
//! tanks are the Company's, in its colors. A Company tank coming back takes a
//! merc hull while there are any, and after that comes back as the realm's.
//! Hulls the Company's tanks lose come off their contracts (and count towards
//! their caps); the rest come off the realm's.
//!
//! **Your company** (#361, #362, #363). You only fight in battles your
//! company (`Galaxy::crew`) is in: on its contract, for the side that hired
//! it (`deploy_at`). On your side, the mercs are your company's alone:
//! another company hired for the same side keeps its hulls out of your
//! battle, so they can't keep you in the fight and your losses never come
//! off them. You're one of your company's seats, in one of its hulls; when
//! its hulls are spent, so are you, and the match plays out without you
//! (see `Match::crew_out`, #373): its result is whatever your side's realm
//! hulls make of it. Some of its hulls are yours alone, set when you start
//! on its contract, more at higher rank ([`own_hulls`], #385): its bots
//! can't spend them, only your deaths do.

use super::{AT_WAR, Galaxy, SHIELD_SEATS, SIDES, SiteKind};
use crate::combat::Match;
use crate::faction::Faction;
use crate::mission::Contract;

/// Of your company's hulls on a contract, how many are yours alone if you
/// start on it at rank `rank` (#385): two at first, one more with each
/// title (every five ranks); never more than half its hulls (see
/// `Galaxy::claim_own`).
pub fn own_hulls(rank: u32) -> u32 {
    2 + rank.saturating_sub(1) / crate::profile::RANKS_PER_TITLE
}

/// What a win in a played battle takes from the losers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stake {
    /// The battle's site: won by anyone but its holder, it changes hands
    /// (a shield generator too, up or down).
    Site,
    /// The site, and one of the losers' shield generators in the system
    /// knocked down besides.
    Shield,
}

/// A played battle drawn from the galaxy, until it's settled.
#[derive(Clone, Debug)]
pub struct Battle {
    /// Where it's fought: an index into `Galaxy::systems`.
    pub system: usize,
    /// Each of the match's teams as a galaxy side (index into `Faction::ALL`).
    pub sides: Vec<usize>,
    /// Hulls each team brings (tanks it can put in play, all told).
    pub allocation: Vec<u32>,
    /// Where each team's realm hulls come from: (system, hulls), the
    /// battle's own system first.
    sources: Vec<Vec<(usize, f32)>>,
    /// Each team's merc hulls, by company: (`Company::id`, hulls). They're
    /// among its allocation.
    hired: Vec<Vec<(u32, f32)>>,
    /// Each team's mercs' share of its hulls in the battle's system.
    share: Vec<f32>,
    /// The site fought over (index into `Galaxy::sites`): a production site
    /// of the system's holder among the sides.
    pub site: Option<usize>,
    /// Each team fights under the site holder's shield: it isn't the holder,
    /// and the holder has a generator up on the site's planet. Such a team
    /// fights `SHIELD_SEATS` seats short.
    pub shielded: Vec<bool>,
    /// Your company (`Company::id`), fighting on the first side: its mercs
    /// are the only ones there. None for a battle you're not in (the
    /// galaxy's picks, for bots-only rounds).
    pub crew: Option<u32>,
}

/// A system's numbers, for the before and after of a battle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    pub hulls: [f32; SIDES],
    pub held: [u8; SIDES],
    /// Shield generators up, each side's.
    pub shields: [u8; SIDES],
}

/// What settling a battle did to the galaxy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settled {
    /// The battle's system just before it was settled, and after (before the
    /// galaxy's tick).
    pub before: Snapshot,
    pub after: Snapshot,
    /// Hulls each side lost, all told (from every system they were drawn from).
    pub lost: [f32; SIDES],
    /// The site changed hands.
    pub taken: bool,
    /// A shield generator went down.
    pub shield_down: bool,
    /// Of `lost`, the hulls that were mercs'.
    pub merc_lost: [f32; SIDES],
}

/// A system's name for the HUD: its name, and its catalogue number.
pub fn system_name(s: usize) -> String {
    format!("{} · {s:04}", super::theater::system_name(s))
}

impl Stake {
    /// What a contract's win takes. Attrition and Hold take the site. A
    /// Raid carries off the flag and knocks out a shield generator with it.
    pub fn of(contract: Contract) -> Stake {
        match contract {
            Contract::Attrition | Contract::Hold => Stake::Site,
            Contract::Raid => Stake::Shield,
        }
    }
}

/// Draw match `m`'s battle from the galaxy (before the round starts): where
/// it's fought, and each side's hull allocation.
pub fn deploy(g: &mut Galaxy, m: &mut Match) -> Battle {
    let b = g.muster(&m.factions, seats(m));
    begin(g, m, b)
}

/// Draw match `m`'s battle at a theater's site, with your company: in
/// `system`, over `site` (index into `Galaxy::sites`), between the match's
/// teams. None unless your company is there (#362): under contract to take
/// or hold `system` for the match's first side, and bringing at least a hull.
pub fn deploy_at(g: &Galaxy, m: &mut Match, system: usize, site: usize) -> Option<Battle> {
    let b = g.muster_crew(system, &m.factions, Some(site))?;
    Some(begin(g, m, b))
}

/// Draw match `m`'s battle where a company is under contract for its first
/// side, against its second: the most merc-heavy such battle, with you in
/// that company (it becomes `Galaxy::crew`); none if no company is working
/// for that side (shots: `--hired`). With `defend`, only a company hired
/// to hold one of the side's worlds (shots: `--hired --defend`, #366).
pub fn deploy_hired(g: &mut Galaxy, m: &mut Match, defend: bool) -> Option<Battle> {
    let f = m.factions.first()?.index();
    let seats = seats(m);
    let b = g
        .companies
        .iter()
        .filter(|c| {
            c.contract
                .is_some_and(|k| k.side as usize == f && (k.defend || !defend))
        })
        .filter_map(|c| {
            let k = c.contract?;
            g.muster_at(k.target as usize, &m.factions, None, Some(c.id))
        })
        .filter(|b| b.mercs(0).round() >= 1.0)
        .max_by_key(|b| b.merc_seats(0, seats))?;
    g.crew = b.crew;
    Some(begin(g, m, b))
}

fn seats(m: &Match) -> usize {
    (0..m.factions.len())
        .map(|t| m.fighters.iter().filter(|f| f.team == t).count())
        .max()
        .unwrap_or(1)
}

fn begin(g: &Galaxy, m: &mut Match, b: Battle) -> Battle {
    for team in 0..b.sides.len() {
        if b.shielded[team] {
            m.close_seats(team, SHIELD_SEATS);
        }
    }
    m.set_allocation(&b.allocation);
    for team in 0..b.sides.len() {
        let seats = m.seats(team);
        m.set_company(
            team,
            b.merc_seats(team, seats),
            b.mercs(team).round() as u32,
        );
        // Each company by name, for the scoreboard's hull summary (#423):
        // those bringing at least half a hull, so none shows up empty-handed.
        let named = b.hired[team]
            .iter()
            .filter(|&&(_, h)| h >= 0.5)
            .map(|&(id, h)| (super::theater::ship_name(id), h))
            .collect();
        m.set_companies(team, named);
    }
    m.place = Some(system_name(b.system));
    log::info!(
        "battle in {} (galaxy tick {}): {}",
        system_name(b.system),
        g.tick,
        describe_allocation(&b)
    );
    b
}

/// Feed match `m`'s result (it's over) back into the galaxy, and tick it once.
pub fn conclude(g: &mut Galaxy, b: &Battle, m: &Match) -> Settled {
    let s = g.settle(
        b,
        &m.lost(),
        &m.company_lost(),
        m.leader(),
        Stake::of(m.contract),
    );
    g.spend_own(m.own_lost());
    g.step_played(b.system);
    s
}

impl Battle {
    /// Team `team`'s merc hulls, all told (on your side, your company's).
    pub fn mercs(&self, team: usize) -> f32 {
        self.hired
            .get(team)
            .map_or(0.0, |h| h.iter().map(|&(_, n)| n).sum())
    }

    /// Of team `team`'s `seats`, how many are the Company's: the mercs'
    /// share of its hulls in the system, rounded; never more than its merc
    /// hulls, and not all of them (it's the realm's fight) unless the side
    /// brings no hulls of the realm's own. With your company there, at least
    /// one: yours.
    pub fn merc_seats(&self, team: usize, seats: usize) -> usize {
        let share = self.share.get(team).copied().unwrap_or(0.0).min(1.0);
        let mut n = (share * seats as f32).round() as usize;
        let realm: f32 = self
            .sources
            .get(team)
            .map_or(0.0, |s| s.iter().map(|h| h.1).sum());
        if realm >= 1.0 {
            n = n.min(seats.saturating_sub(1));
        }
        if team == 0 && self.crew.is_some() {
            n = n.max(1);
        }
        n.min(self.mercs(team).round() as usize)
    }
}

fn describe_allocation(b: &Battle) -> String {
    b.sides
        .iter()
        .zip(&b.allocation)
        .enumerate()
        .map(|(t, (&f, a))| {
            let mercs = b.mercs(t);
            if mercs > 0.0 {
                format!(
                    "{} {a} hulls ({mercs:.0} mercs', {} companies)",
                    Faction::ALL[f].name(),
                    b.hired[t].len()
                )
            } else {
                format!("{} {a} hulls", Faction::ALL[f].name())
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A settled battle in a line or three: each side's hulls in the system
/// before and after, what it lost all told, and what changed hands.
pub fn describe(b: &Battle, s: &Settled) -> String {
    let mut out = format!("{} settled:", system_name(b.system));
    for &f in &b.sides {
        out += &format!(
            "\n  {:<9} hulls here {:6.1} -> {:6.1} (lost {:4.1} all told)  sites {} -> {}  shields up {} -> {}",
            Faction::ALL[f].name(),
            s.before.hulls[f],
            s.after.hulls[f],
            s.lost[f],
            s.before.held[f],
            s.after.held[f],
            s.before.shields[f],
            s.after.shields[f],
        );
    }
    if s.taken {
        out += "\n  the site changed hands";
    }
    if s.shield_down {
        out += "\n  a shield generator went down";
    }
    for &f in &b.sides {
        if s.merc_lost[f] > 0.0 {
            out += &format!(
                "\n  the {}' mercs lost {:.0} of those",
                Faction::ALL[f].name(),
                s.merc_lost[f]
            );
        }
    }
    out
}

impl Galaxy {
    /// Side `f`'s hulls in system `s`, but for `kept` of them (another
    /// company's, on your side).
    fn within_reach(&self, s: usize, f: usize, kept: f32) -> f32 {
        (self.systems[s].hulls[f] - kept).max(0.0)
    }

    /// The companies under contract to take or hold system `t` for side `f`, and
    /// their hulls there: ((`Company::id`, hulls), all told). Never more
    /// than the side has there.
    fn on_contract(&self, t: usize, f: usize) -> (Vec<(u32, f32)>, f32) {
        let mut out = Vec::new();
        for co in &self.companies {
            if let Some(k) = co.contract
                && k.target as usize == t
                && k.side as usize == f
                && k.field > 0.0
            {
                out.push((co.id, k.field));
            }
        }
        let total: f32 = out.iter().map(|&(_, h)| h).sum();
        let here = self.systems[t].hulls[f];
        if total > here {
            let k = here / total;
            out.iter_mut().for_each(|(_, h)| *h *= k);
        }
        (out, total.min(here))
    }

    fn reach(&self, s: usize, f: usize) -> f32 {
        self.within_reach(s, f, 0.0)
    }

    /// Where to fight a battle between `factions` (the match's teams, in
    /// order), with `seats` tanks a side in play at once, and what each side
    /// brings. The galaxy picks: a system one of them holds, where they're at
    /// war if anywhere, and where the worst-off side brings the most; at
    /// random among the best.
    pub fn muster(&mut self, factions: &[Faction], seats: usize) -> Battle {
        let sides: Vec<usize> = factions.iter().map(|f| f.index()).collect();
        // Each system's tier (higher is better), and the worst-off side's reach.
        let mut best = (0u8, 0.0f32);
        let mut picks: Vec<usize> = Vec::new();
        for s in 0..self.systems.len() {
            let sys = &self.systems[s];
            if !sides.iter().any(|&f| sys.held[f] > 0) {
                continue;
            }
            let fighting = sides.iter().filter(|&&f| sys.hulls[f] >= AT_WAR).count() >= 2;
            let least = sides
                .iter()
                .map(|&f| self.reach(s, f))
                .fold(f32::INFINITY, f32::min);
            let tier = 1 + (fighting && least >= seats as f32) as u8;
            let score = least.floor();
            if (tier, score) > best {
                best = (tier, score);
                picks.clear();
            }
            if (tier, score) == best {
                picks.push(s);
            }
        }
        let system = if picks.is_empty() {
            0
        } else {
            picks[self.rng.index(picks.len())]
        };
        self.muster_at(system, factions, None, None)
            .expect("a battle without your company")
    }

    /// A battle with your company (`Galaxy::crew`) in it, as for
    /// `muster_at`: None unless it's under contract to take or hold `system` for
    /// the first of `factions` and brings at least a hull.
    pub fn muster_crew(
        &self,
        system: usize,
        factions: &[Faction],
        site: Option<usize>,
    ) -> Option<Battle> {
        let b = self.muster_at(system, factions, site, Some(self.crew?))?;
        (b.mercs(0).round() >= 1.0).then_some(b)
    }

    /// A battle between `factions` in system `system`, over `site` (index into
    /// `Galaxy::sites`) if it's given, else a production site of whichever
    /// side holds most there: what each side brings, as for `muster`. With
    /// `crew` (a `Company::id`), you're in that company, on the first side:
    /// its mercs are the only ones there, and none if it isn't under
    /// contract there for that side.
    pub fn muster_at(
        &self,
        system: usize,
        factions: &[Faction],
        site: Option<usize>,
        crew: Option<u32>,
    ) -> Option<Battle> {
        let sides: Vec<usize> = factions.iter().map(|f| f.index()).collect();
        let mut allocation = Vec::new();
        let mut sources = Vec::new();
        let mut hired = Vec::new();
        let mut share = Vec::new();
        for (team, &f) in sides.iter().enumerate() {
            let (mut companies, mut mercs) = self.on_contract(system, f);
            // On your side, only your company's mercs: the others' hulls
            // here stay out of it.
            let mut kept = 0.0;
            if team == 0
                && let Some(id) = crew
            {
                companies.iter().find(|c| c.0 == id)?;
                kept = companies.iter().filter(|c| c.0 != id).map(|c| c.1).sum();
                companies.retain(|c| c.0 == id);
                mercs = companies.iter().map(|c| c.1).sum();
            }
            let want = self.within_reach(system, f, kept).floor();
            let mut from = vec![(system, want)];
            // What's drawn here is the mercs' in proportion to their share
            // of the side's hulls here.
            let here = (self.systems[system].hulls[f] - kept).max(0.0);
            let mut theirs = Vec::new();
            if let Some(drawn) = from.first_mut().filter(|h| h.0 == system)
                && mercs > 0.0
            {
                let k = drawn.1 / here;
                theirs = companies.iter().map(|&(id, h)| (id, h * k)).collect();
                drawn.1 -= mercs * k;
            }
            share.push(if here > 0.0 { mercs / here } else { 0.0 });
            allocation.push(want as u32);
            sources.push(from);
            hired.push(theirs);
        }
        // The site at stake: the one asked for, else a production site of
        // whichever side holds most here.
        let sys = &self.systems[system];
        let holder = sides.iter().copied().max_by_key(|&f| sys.held[f]);
        let (a, b) = sys.sites;
        let site = site.or_else(|| {
            (a as usize..b as usize).find(|&i| {
                let site = &self.sites[i];
                Some(site.owner as usize) == holder
                    && matches!(site.kind, SiteKind::Production { .. })
            })
        });
        let shielded = sides.iter().map(|&f| self.under_shield(system, site, f)).collect();
        Some(Battle {
            system,
            sides,
            allocation,
            sources,
            hired,
            share,
            site,
            shielded,
            crew,
        })
    }

    /// Side `f` would fight for `site` (in system `s`) under its holder's
    /// shield: `f` isn't the holder, and the holder has a generator up on
    /// the site's planet.
    pub fn under_shield(&self, s: usize, site: Option<usize>, f: usize) -> bool {
        let Some(i) = site else { return false };
        let (owner, planet) = (self.sites[i].owner, self.sites[i].planet);
        let (a, b) = self.systems[s].sites;
        owner as usize != f
            && self.sites[a as usize..b as usize].iter().any(|x| {
                x.owner == owner && x.planet == planet && x.kind == (SiteKind::Shield { down: 0 })
            })
    }

    pub fn snapshot(&self, s: usize) -> Snapshot {
        let sys = &self.systems[s];
        let mut shields = [0; SIDES];
        for site in &self.sites[sys.sites.0 as usize..sys.sites.1 as usize] {
            if site.kind == (SiteKind::Shield { down: 0 }) {
                shields[site.owner as usize] += 1;
            }
        }
        Snapshot {
            hulls: sys.hulls,
            held: sys.held,
            shields,
        }
    }

    /// Feed a played battle's result back in: `lost` hulls for each team (its
    /// tanks destroyed), `company` of them the Company's tanks, and `winner`
    /// (a team, or none for a draw) takes `stake`. Doesn't tick the galaxy:
    /// the caller does (one tick a battle).
    pub fn settle(
        &mut self,
        battle: &Battle,
        lost: &[u32],
        company: &[u32],
        winner: Option<usize>,
        stake: Stake,
    ) -> Settled {
        let before = self.snapshot(battle.system);
        let mut total = [0.0; SIDES];
        let mut merc_lost = [0.0; SIDES];
        for (team, &f) in battle.sides.iter().enumerate() {
            let from = &battle.sources[team];
            let drawn: f32 = from.iter().map(|&(_, h)| h).sum();
            let mercs = battle.mercs(team);
            let lost = lost.get(team).copied().unwrap_or(0) as f32;
            // The Company's tanks lost are the mercs'; a side that loses more
            // of either than it brought loses the rest from the other.
            let theirs = company.get(team).copied().unwrap_or(0) as f32;
            let mut m = theirs.min(lost).min(mercs);
            let r = (lost - m).min(drawn);
            m = (lost - r).min(mercs);
            if drawn > 0.0 {
                // Losses come off each source in proportion; never more than it gave.
                let k = (r / drawn).min(1.0);
                for &(n, h) in from {
                    let off = (h * k).min(self.systems[n].hulls[f]);
                    self.systems[n].hulls[f] -= off;
                    total[f] += off;
                }
            }
            if mercs > 0.0 && m > 0.0 {
                let k = m / mercs;
                let mut off_all = 0.0;
                for &(id, h) in &battle.hired[team] {
                    let Some(co) = self.companies.iter_mut().find(|c| c.id == id) else {
                        continue;
                    };
                    let Some(c) = co.contract.as_mut() else {
                        continue;
                    };
                    let off = (h * k).min(c.field);
                    c.field -= off;
                    c.lost += off;
                    off_all += off;
                }
                let off_all = off_all.min(self.systems[battle.system].hulls[f]);
                self.systems[battle.system].hulls[f] -= off_all;
                self.books.lost += off_all as f64;
                total[f] += off_all;
                merc_lost[f] += off_all;
            }
        }
        let (mut taken, mut shield_down) = (false, false);
        if let Some(w) = winner.and_then(|t| battle.sides.get(t).copied()) {
            // Whatever the contract, a win at a loser's site takes it (#419):
            // never a bystander's. A generator taken, up or down, has to be
            // repaired: its new holder's model, from the wreck.
            let at_stake = battle.site.filter(|&i| {
                let owner = self.sites[i].owner as usize;
                owner != w && battle.sides.contains(&owner)
            });
            if let Some(i) = at_stake {
                let old = self.sites[i].owner as usize;
                self.sites[i].owner = w as u8;
                if let SiteKind::Shield { down } = &mut self.sites[i].kind {
                    *down = super::REPAIR_TICKS;
                }
                let sys = &mut self.systems[battle.system];
                sys.held[old] -= 1;
                sys.held[w] += 1;
                self.claim(battle.system, w);
                taken = true;
                self.update_share();
            }
            if stake == Stake::Shield {
                // A Raid knocks out one of the losers' generators besides.
                let (a, b) = self.systems[battle.system].sites;
                let losers: Vec<usize> = battle.sides.iter().copied().filter(|&f| f != w).collect();
                let up = |i: &usize| {
                    let site = &self.sites[*i];
                    losers.contains(&(site.owner as usize))
                        && site.kind == (SiteKind::Shield { down: 0 })
                };
                if let Some(i) = (a as usize..b as usize).find(up) {
                    self.sites[i].kind = SiteKind::Shield {
                        down: super::REPAIR_TICKS,
                    };
                    shield_down = true;
                }
            }
        }
        Settled {
            before,
            after: self.snapshot(battle.system),
            lost: total,
            taken,
            shield_down,
            merc_lost,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::{REGION, Rules, SYSTEMS};

    fn galaxy() -> Galaxy {
        let mut g = Galaxy::with_size(3, SYSTEMS / 4, REGION * 0.5, Rules::default());
        for _ in 0..50 {
            g.step();
        }
        g
    }

    const PAIR: [Faction; 2] = [Faction::Imperium, Faction::Resonant];

    /// The battle is somewhere one of its sides holds, and every side brings
    /// its hulls there, from there only.
    #[test]
    fn a_battle_draws_on_the_systems_hulls() {
        let mut g = galaxy();
        for factions in [&PAIR[..], &Faction::ALL[..]] {
            let b = g.muster(factions, 6);
            let sys = &g.systems[b.system];
            assert!(b.sides.iter().any(|&f| sys.held[f] > 0));
            for (team, &f) in b.sides.iter().enumerate() {
                assert_eq!(b.allocation[team], sys.hulls[f].floor() as u32);
                assert!(b.sources[team].iter().all(|s| s.0 == b.system));
            }
        }
    }

    /// Two sides at war somewhere: that's where the battle is.
    #[test]
    fn battles_are_fought_on_the_fronts() {
        let mut g = galaxy();
        let b = g.muster(&PAIR, 6);
        let at_war = (0..g.systems.len()).any(|s| {
            let h = g.systems[s].hulls;
            PAIR.iter().all(|f| h[f.index()] >= AT_WAR)
                && PAIR.iter().any(|f| g.systems[s].held[f.index()] > 0)
        });
        if at_war {
            let h = g.systems[b.system].hulls;
            assert!(PAIR.iter().all(|f| h[f.index()] >= AT_WAR));
        }
    }

    /// Hulls lost come off the galaxy, all of them; nothing else does.
    #[test]
    fn losses_go_back_into_the_galaxy() {
        let mut g = galaxy();
        let b = g.muster(&PAIR, 6);
        let total = |g: &Galaxy, f: usize| g.systems.iter().map(|s| s.hulls[f]).sum::<f32>();
        let (imp, res) = (Faction::Imperium.index(), Faction::Resonant.index());
        let before = [total(&g, imp), total(&g, res)];
        let lost = [b.allocation[0].min(20), b.allocation[1].min(7)];
        let s = g.settle(&b, &lost, &[0, 0], None, Stake::Site);
        assert!((before[0] - total(&g, imp) - lost[0] as f32).abs() < 0.01);
        assert!((before[1] - total(&g, res) - lost[1] as f32).abs() < 0.01);
        assert!((s.lost[imp] - lost[0] as f32).abs() < 1e-3);
        assert_eq!(s.before.held, s.after.held, "a draw takes no ground");
        assert!(!s.taken && !s.shield_down);
    }

    /// A battle won by the side that doesn't hold the site takes it, under
    /// any contract (#419); won by its holder, nothing changes hands.
    #[test]
    fn a_won_site_changes_hands() {
        for contract in [Contract::Attrition, Contract::Hold, Contract::Raid] {
            let mut g = galaxy();
            let b = g.muster(&PAIR, 6);
            let site = b.site.expect("a site at stake");
            let holder = g.sites[site].owner as usize;
            let team = |f: usize| b.sides.iter().position(|&x| x == f).unwrap();
            let other = 1 - team(holder);
            let stake = Stake::of(contract);
            let kept = g.settle(&b, &[0, 0], &[0, 0], Some(team(holder)), stake);
            assert!(!kept.taken, "{contract:?}");
            let s = g.settle(&b, &[0, 0], &[0, 0], Some(other), stake);
            assert!(s.taken, "{contract:?}");
            assert_eq!(g.sites[site].owner as usize, b.sides[other]);
            assert_eq!(s.after.held[holder] + 1, s.before.held[holder]);
            assert_eq!(
                s.after.held[b.sides[other]],
                s.before.held[b.sides[other]] + 1
            );
        }
    }

    /// Your battle is its system's battle for the tick (#419): the
    /// galaxy's tick that goes with it doesn't fight there again, so a site
    /// you've just won is still yours after it.
    #[test]
    fn a_won_site_survives_its_tick() {
        for seed in 0..20 {
            let mut g = Galaxy::warmed(seed);
            let b = g.muster(&PAIR, 6);
            let Some(site) = b.site else { continue };
            let holder = g.sites[site].owner as usize;
            let Some(t) = b.sides.iter().position(|&x| x == holder) else {
                continue;
            };
            let s = g.settle(&b, &[0, 0], &[0, 0], Some(1 - t), Stake::Site);
            let hulls = g.systems[b.system].hulls;
            g.step_played(b.system);
            assert!(s.taken);
            assert_eq!(g.sites[site].owner as usize, b.sides[1 - t], "seed {seed}");
            // No second round of fighting there: only wear thins its hulls.
            let wear = 1.0 - g.rules.wear;
            for f in 0..SIDES {
                assert!(g.systems[b.system].hulls[f] >= hulls[f] * wear - 1e-3);
            }
        }
    }

    /// A won battle over a shield generator that's up takes it, not only
    /// knocks it down (#419): it's the winner's, to be repaired.
    #[test]
    fn a_won_generator_is_taken_even_up() {
        let mut g = galaxy();
        let b = g.muster(&PAIR, 6);
        // The site at stake, made a generator that's up.
        let gen_ = b.site.unwrap();
        g.sites[gen_].kind = SiteKind::Shield { down: 0 };
        let holder = g.sites[gen_].owner as usize;
        let other = 1 - b.sides.iter().position(|&x| x == holder).unwrap();
        let s = g.settle(&b, &[0, 0], &[0, 0], Some(other), Stake::Site);
        assert!(s.taken);
        assert_eq!(g.sites[gen_].owner as usize, b.sides[other]);
        assert_eq!(
            g.sites[gen_].kind,
            SiteKind::Shield {
                down: crate::galaxy::REPAIR_TICKS
            }
        );
    }

    /// A won Raid takes the site and knocks down another of the losers'
    /// generators, if one's up.
    #[test]
    fn a_won_raid_downs_a_shield() {
        let mut g = galaxy();
        let b = g.muster(&PAIR, 6);
        let site = b.site.unwrap();
        let holder = g.sites[site].owner as usize;
        let team = b.sides.iter().position(|&x| x == holder).unwrap();
        let site_up = g.sites[site].kind == (SiteKind::Shield { down: 0 });
        let s = g.settle(&b, &[0, 0], &[0, 0], Some(1 - team), Stake::Shield);
        assert!(s.taken);
        let others_up = s.before.shields[holder] - site_up as u8;
        if others_up > 0 {
            assert!(s.shield_down);
            assert_eq!(s.after.shields[holder] + 1, others_up);
        }
        // A draw takes nothing.
        let d = g.settle(&b, &[0, 0], &[0, 0], None, Stake::Shield);
        assert!(!d.shield_down && !d.taken);
    }
}

#[cfg(test)]
mod merc_tests {
    use super::*;
    use crate::galaxy::Galaxy;

    /// Your own hulls on a contract (#385): a small few at rank 1, more
    /// with each title, never fewer at a higher rank.
    #[test]
    fn own_hulls_grow_with_rank() {
        assert_eq!(own_hulls(1), 2);
        assert_eq!(own_hulls(5), 2);
        assert_eq!(own_hulls(6), 3);
        assert_eq!(own_hulls(46), 11);
        for r in 1..60 {
            assert!(own_hulls(r + 1) >= own_hulls(r));
        }
    }

    /// A galaxy with you in a company under contract with at least
    /// `hulls` committed.
    fn on_contract(hulls: f32) -> Galaxy {
        let mut g = Galaxy::warmed(7);
        let c = g
            .companies
            .iter()
            .find(|c| c.contract.is_some_and(|k| k.field >= hulls))
            .expect("a company under contract")
            .id;
        g.crew = Some(c);
        g
    }

    /// Your own hulls are set once when you start on a contract, by your
    /// rank then: ranking up on it adds none, and only your losses take
    /// them. They're never more than half its hulls.
    #[test]
    fn own_hulls_are_set_once_a_contract() {
        let mut g = on_contract(8.0);
        assert_eq!(g.own_left(), 0, "none until you start on it");
        g.claim_own(1);
        assert_eq!(g.own_left(), 2);
        g.claim_own(46);
        assert_eq!(g.own_left(), 2, "a rank up mid-contract adds none");
        g.spend_own(1);
        g.claim_own(1);
        assert_eq!(g.own_left(), 1, "spent ones stay spent");
        let o = g.own.unwrap();
        assert_eq!((o.left, o.of), (1, 2));

        // A new contract (or ship) is a new allocation, at your rank then.
        let mut h = on_contract(8.0);
        h.claim_own(1);
        let c = h.crew_company().unwrap();
        let k = h.companies[c].contract.as_mut().unwrap();
        k.signed += 1;
        let half = ((k.field).round() as u32).div_ceil(2);
        h.claim_own(46);
        assert_eq!(h.own_left(), own_hulls(46).min(half));
        assert!(h.own_left() <= half);
    }

    /// Your own hulls lost in a battle come off yours on the contract.
    #[test]
    fn a_battle_spends_your_own_hulls() {
        let mut g = on_contract(8.0);
        g.claim_own(1);
        let c = g.crew_company().unwrap();
        let k = g.companies[c].contract.unwrap();
        let (t, f) = (k.target as usize, k.side as usize);
        let enemy = (0..SIDES)
            .filter(|&e| e != f)
            .max_by(|&a, &b| g.systems[t].hulls[a].total_cmp(&g.systems[t].hulls[b]))
            .unwrap();
        let mut map = crate::map::Map::saltmarket();
        map.factions = vec![Faction::ALL[f], Faction::ALL[enemy]];
        let mut m = crate::combat::Match::new(map, 0, 5, 21);
        let b = g
            .muster_crew(t, &m.factions, None)
            .expect("your company's battle");
        let b = begin(&g, &mut m, b);
        m.set_own_hulls(g.own_left());
        assert_eq!(m.own_hulls, 2);
        m.start();
        m.destroy_player();
        conclude(&mut g, &b, &m);
        assert_eq!(g.own.unwrap().left, 1);
    }

    /// A fresh galaxy staged for a hired battle, so the numbers are known:
    /// in system `t`, side `f` has `realm` hulls of its realm's own and
    /// company 0's `mercs` there on contract; its enemy has 50. No other
    /// company is on contract anywhere.
    fn staged(mercs: f32, realm: f32) -> (Galaxy, usize, [Faction; 2]) {
        let mut g = Galaxy::new(1);
        let t = 0;
        let e = g.systems[t].owner();
        let r = g
            .realms
            .iter()
            .position(|x| x.faction as usize != e && x.systems > 0)
            .expect("a realm of another people") as u32;
        let f = g.realms[r as usize].faction as usize;
        g.systems[t].hulls = [0.0; SIDES];
        g.systems[t].hulls[f] = realm + mercs;
        g.systems[t].hulls[e] = 50.0;
        g.systems[t].realms[f] = r;
        for c in &mut g.companies {
            c.contract = None;
        }
        g.companies[0].contract = Some(super::super::merc::Contract {
            realm: r,
            side: f as u8,
            target: t as u32,
            signed: g.tick,
            upfront: 0.0,
            bonus: 0.0,
            committed: mercs,
            field: mercs,
            lost: 0.0,
            defend: false,
        });
        (g, t, [Faction::ALL[f], Faction::ALL[e]])
    }

    /// Fighting for a site under its holder's shield, the attacker (mercs
    /// and all) fights a seat short (#426): its last seat stays empty. The
    /// holder never does, and with the planet's generators down, nor does
    /// the attacker.
    #[test]
    fn shields_close_an_attackers_seat() {
        let (mut g, t, sides) = staged(20.0, 40.0);
        let battle = |g: &Galaxy| {
            let mut map = crate::map::Map::saltmarket();
            map.factions = sides.to_vec();
            let mut m = crate::combat::Match::new(map, 0, 11, 21);
            let b = begin(g, &mut m, g.muster_at(t, &sides, None, None).unwrap());
            m.start();
            (b, m)
        };
        let (b, m) = battle(&g);
        assert_eq!(b.shielded, vec![true, false]);
        assert_eq!((m.seats(0), m.seats(1)), (6 - SHIELD_SEATS, 6));
        let fielded = |m: &Match, team: usize| {
            m.fighters.iter().filter(|f| f.team == team && f.alive).count()
        };
        assert_eq!(fielded(&m, 0), 6 - SHIELD_SEATS);
        assert_eq!(fielded(&m, 1), 6);
        assert!(m.fighters[0].alive, "never your seat");
        // Its generators on the site's planet down: no shield.
        let site = b.site.unwrap();
        let planet = g.sites[site].planet;
        let (a, z) = g.systems[t].sites;
        for x in &mut g.sites[a as usize..z as usize] {
            if x.planet == planet && matches!(x.kind, SiteKind::Shield { .. }) {
                x.kind = SiteKind::Shield { down: 2 };
            }
        }
        let (b, m) = battle(&g);
        assert_eq!(b.shielded, vec![false, false]);
        assert_eq!((fielded(&m, 0), fielded(&m, 1)), (6, 6));
    }

    /// A second company, on the same contract as company 0, with `hulls`
    /// of its own there. Returns its id.
    fn second_company(g: &mut Galaxy, hulls: f32) -> u32 {
        let k = g.companies[0].contract.unwrap();
        let id = g.companies.iter().map(|c| c.id).max().unwrap() + 1;
        g.companies.push(super::super::merc::Company {
            id,
            contract: Some(super::super::merc::Contract {
                committed: hulls,
                field: hulls,
                ..k
            }),
            ..g.companies[0]
        });
        g.systems[k.target as usize].hulls[k.side as usize] += hulls;
        id
    }

    /// Mercs under contract in the battle's system are part of their side's
    /// allocation, in proportion to their share of its hulls there, and so
    /// are its seats: never all of them while the realm brings hulls of its
    /// own. A side with no company there brings no mercs.
    #[test]
    fn hired_sides_bring_mercs() {
        let (g, t, sides) = staged(20.0, 40.0);
        let b = g.muster_at(t, &sides, None, None).unwrap();
        assert_eq!(b.allocation, vec![60, 50]);
        assert!((b.mercs(0) - 20.0).abs() < 1e-3);
        assert_eq!(b.sources[0].len(), 1);
        assert_eq!(b.sources[0][0].0, t);
        assert!((b.sources[0][0].1 - 40.0).abs() < 1e-3);
        assert_eq!(
            b.merc_seats(0, 6),
            2,
            "a third of the hulls, a third of the seats"
        );
        assert_eq!(b.mercs(1), 0.0);
        assert_eq!(b.merc_seats(1, 6), 0);
        // Mostly mercs: all but one seat, the realm's.
        let (g, t, sides) = staged(100.0, 5.0);
        let b = g.muster_at(t, &sides, None, None).unwrap();
        assert_eq!(b.merc_seats(0, 6), 5);
        // All mercs: every seat.
        let (g, t, sides) = staged(20.0, 0.0);
        let b = g.muster_at(t, &sides, None, None).unwrap();
        assert_eq!(b.merc_seats(0, 6), 6);
        // Never more seats than merc hulls.
        let (g, t, sides) = staged(2.0, 0.0);
        let b = g.muster_at(t, &sides, None, None).unwrap();
        assert_eq!(b.merc_seats(0, 6), 2);
    }

    /// The Company's tanks lost come off its contract; the rest come off
    /// the realm's hulls; the side loses them all.
    #[test]
    fn merc_losses_come_off_their_contracts() {
        let (mut g, t, sides) = staged(20.0, 40.0);
        let f = sides[0].index();
        let b = g.muster_at(t, &sides, None, None).unwrap();
        let s = g.settle(&b, &[5, 0], &[3, 0], None, Stake::Site);
        assert!((s.merc_lost[f] - 3.0).abs() < 1e-3, "{:?}", s.merc_lost);
        assert!((s.lost[f] - 5.0).abs() < 1e-3, "{:?}", s.lost);
        let k = g.companies[0].contract.unwrap();
        assert!((k.field - 17.0).abs() < 1e-3);
        assert!((k.lost - 3.0).abs() < 1e-3);
        assert!((g.systems[t].hulls[f] - 55.0).abs() < 1e-3);
        // More Company tanks lost than it brought: the rest are the realm's.
        let (mut g, t, sides) = staged(2.0, 40.0);
        let b = g.muster_at(t, &sides, None, None).unwrap();
        let s = g.settle(&b, &[5, 0], &[5, 0], None, Stake::Site);
        assert!((s.merc_lost[f] - 2.0).abs() < 1e-3, "{:?}", s.merc_lost);
        assert!((g.systems[t].hulls[f] - 37.0).abs() < 1e-3);
    }

    /// You only fight in battles your company is in (#362): on its
    /// contract, for the side that hired it. Anywhere else, or without a
    /// company, there's no battle for you.
    #[test]
    fn you_only_fight_where_your_company_is() {
        let (mut g, t, sides) = staged(20.0, 40.0);
        g.crew = None;
        assert!(g.muster_crew(t, &sides, None).is_none(), "no company");
        g.crew = Some(g.companies[0].id);
        let b = g
            .muster_crew(t, &sides, None)
            .expect("your company's battle");
        assert_eq!(b.crew, g.crew);
        assert!((b.mercs(0) - 20.0).abs() < 1e-3);
        assert!(b.merc_seats(0, 6) >= 1, "you're one of its seats");
        // For the other side, or in another system, it isn't there.
        let flipped = [sides[1], sides[0]];
        assert!(g.muster_crew(t, &flipped, None).is_none());
        assert!(g.muster_crew(t + 1, &sides, None).is_none());
        // Its contract over, it's not in any battle.
        g.companies[0].contract = None;
        assert!(g.muster_crew(t, &sides, None).is_none());
    }

    /// In a battle you play, your side's mercs are your company's alone
    /// (#363): another company hired for the same side brings none of its
    /// hulls, and none of your losses come off it.
    #[test]
    fn your_side_has_only_your_companys_mercs() {
        let (mut g, t, sides) = staged(20.0, 40.0);
        let f = sides[0].index();
        let mine = g.companies[0].id;
        g.crew = Some(mine);
        let alone = g.muster_crew(t, &sides, None).unwrap();
        let other = second_company(&mut g, 30.0);
        let b = g.muster_crew(t, &sides, None).unwrap();
        assert_eq!(b.hired[0].len(), 1);
        assert_eq!(b.hired[0][0].0, mine);
        // The other company's hulls don't keep you in the fight: what your
        // side brings is the same as without it.
        assert_eq!(b.allocation[0], alone.allocation[0]);
        assert!((b.mercs(0) - alone.mercs(0)).abs() < 1e-3);
        assert_eq!(b.merc_seats(0, 6), alone.merc_seats(0, 6));
        // Pooled (a battle you're not in), they'd have been there.
        let pooled = g.muster_at(t, &sides, None, None).unwrap();
        assert!(pooled.hired[0].iter().any(|&(id, _)| id == other));
        // Your losses come off your company's contract, not theirs.
        let field = |g: &Galaxy, id: u32| {
            g.companies
                .iter()
                .find(|c| c.id == id)
                .unwrap()
                .contract
                .unwrap()
                .field
        };
        let s = g.settle(&b, &[4, 0], &[3, 0], None, Stake::Site);
        assert!((s.merc_lost[f] - 3.0).abs() < 1e-3);
        assert!((field(&g, mine) - 17.0).abs() < 1e-3);
        assert_eq!(field(&g, other), 30.0, "their hulls are their own");
    }
}
