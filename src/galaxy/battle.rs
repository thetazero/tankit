//! The battles the player plays, as part of the galaxy's war (#304).
//!
//! **In.** A battle is fought in one system, between the mission's sides. Each
//! side gets a **hull allocation**: the hulls it can bring, drawn from its
//! storage in that system and, when that's short, from the systems within FTL
//! range, up to [`PER_SEAT`] hulls a seat. Every tank a side puts in play,
//! the first ones included, is one of those hulls; when they're spent, the
//! side has no more tanks. The seats (tanks in play at once) stay capped as
//! before.
//!
//! **Out.** When the battle's done, each side's hulls lost (its tanks
//! destroyed) come off the storage they were drawn from, in proportion;
//! survivors go back where they came from. What the win changes depends on
//! the contract (see [`Stake`]). Then the whole galaxy ticks once.
//!
//! **Mercs** (#320). A company under contract to take the battle's system
//! has its hulls there among its employer's side's, so a side's allocation
//! drawn from that system is part merc, in proportion: if a third of the
//! side's hulls here are a company's, a third of what it brings is. (Mercs
//! on contract elsewhere stay on their own contracts: they're never drawn
//! from a neighbor.) The side's **seats** are split by the mercs' share of
//! its force in the system itself (the attack they were hired to make up),
//! rounded, but never more seats than merc hulls, and never all of them
//! while the side brings any hulls of the realm's own: that many of its
//! tanks are the Company's, in its colors. You're always one of them on your side (with
//! no company there, the only one). A Company tank coming back takes a merc
//! hull while there are any, and after that comes back as the realm's. Hulls
//! the Company's tanks lose come off their contracts (and count towards
//! their caps); the rest come off the realm's.

use super::{AT_WAR, Galaxy, SIDES, SiteKind};
use crate::combat::Match;
use crate::faction::Faction;
use crate::mission::Contract;

/// Most hulls a side brings to a battle, per seat: about twice what a side
/// loses in an even five-minute round (25 to 35 tanks for six seats), so
/// most battles end on the objective, not with a side out of hulls.
pub const PER_SEAT: u32 = 10;

/// What a win in a played battle takes from the losers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stake {
    /// A raid on an enemy camp: no ground changes hands, only hulls are lost.
    Hulls,
    /// The battle's site: won by anyone but its holder, it changes hands.
    Site,
    /// One of the losers' shield generators in the system, knocked down.
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
    /// What a contract's win takes. Attrition is a raid on an enemy camp:
    /// only hulls. Hold is for the site. A Raid carries off the flag and
    /// knocks out a shield generator with it.
    pub fn of(contract: Contract) -> Stake {
        match contract {
            Contract::Attrition => Stake::Hulls,
            Contract::Hold => Stake::Site,
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

/// Draw match `m`'s battle at a theater's site: in `system`, over `site`
/// (index into `Galaxy::sites`), between the match's teams.
pub fn deploy_at(g: &Galaxy, m: &mut Match, system: usize, site: usize) -> Battle {
    let b = g.muster_at(system, &m.factions, seats(m), Some(site));
    begin(g, m, b)
}

/// Draw match `m`'s battle where a company is under contract for its first
/// side (yours), against its second: the most merc-heavy such battle, or
/// none if no company is working for that side (shots: `--hired`).
pub fn deploy_hired(g: &Galaxy, m: &mut Match) -> Option<Battle> {
    let f = m.factions.first()?.index();
    let seats = seats(m);
    let b = g
        .companies
        .iter()
        .filter_map(|c| c.contract)
        .filter(|k| k.side as usize == f)
        .map(|k| g.muster_at(k.target as usize, &m.factions, seats, None))
        .max_by_key(|b| b.merc_seats(0, seats))?;
    Some(begin(g, m, b))
}

fn seats(m: &Match) -> usize {
    (0..m.factions.len())
        .map(|t| m.fighters.iter().filter(|f| f.team == t).count())
        .max()
        .unwrap_or(1)
}

fn begin(g: &Galaxy, m: &mut Match, b: Battle) -> Battle {
    m.set_allocation(&b.allocation);
    for team in 0..b.sides.len() {
        let seats = m.fighters.iter().filter(|f| f.team == team).count();
        m.set_company(
            team,
            b.merc_seats(team, seats),
            b.mercs(team).round() as u32,
        );
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
    g.step();
    s
}

impl Battle {
    /// Team `team`'s merc hulls, all told.
    pub fn mercs(&self, team: usize) -> f32 {
        self.hired
            .get(team)
            .map_or(0.0, |h| h.iter().map(|&(_, n)| n).sum())
    }

    /// Of team `team`'s `seats`, how many are the Company's: the mercs'
    /// share of its hulls in the system, rounded; never more than its merc
    /// hulls, and not all of them (it's the realm's fight) unless the side
    /// brings no hulls of the realm's own.
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
    /// Side `f`'s hulls in system `s` and within FTL range of it, nearest
    /// first: (system, hulls). Mercs' count only in `s` itself: those
    /// elsewhere are on their own contracts.
    fn within_reach(&self, s: usize, f: usize) -> impl Iterator<Item = (usize, f32)> + '_ {
        std::iter::once(s)
            .chain(self.neighbors(s).map(|(n, _)| n))
            .map(move |n| {
                let h = self.systems[n].hulls[f];
                let away = if n == s {
                    0.0
                } else {
                    self.on_contract(n, f).1
                };
                (n, (h - away).max(0.0))
            })
            .filter(|&(_, h)| h > 0.0)
    }

    /// The companies under contract to take system `t` for side `f`, and
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
        self.within_reach(s, f).map(|(_, h)| h).sum()
    }

    /// Where to fight a battle between `factions` (the match's teams, in
    /// order), with `seats` tanks a side in play at once, and what each side
    /// brings. The galaxy picks: a system one of them holds, where they're at
    /// war if anywhere, and where every side can bring a full allocation if
    /// any can; at random among the best.
    pub fn muster(&mut self, factions: &[Faction], seats: usize) -> Battle {
        let sides: Vec<usize> = factions.iter().map(|f| f.index()).collect();
        let cap = PER_SEAT as f32 * seats as f32;
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
            let full = least >= cap;
            let tier = 1 + full as u8 + 2 * (fighting && least >= seats as f32) as u8;
            // Below a full allocation, more is better.
            let score = if full { cap } else { least };
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
        self.muster_at(system, factions, seats, None)
    }

    /// A battle between `factions` in system `system`, over `site` (index into
    /// `Galaxy::sites`) if it's given, else a production site of whichever
    /// side holds most there: what each side brings, as for `muster`.
    pub fn muster_at(
        &self,
        system: usize,
        factions: &[Faction],
        seats: usize,
        site: Option<usize>,
    ) -> Battle {
        let sides: Vec<usize> = factions.iter().map(|f| f.index()).collect();
        let cap = PER_SEAT as f32 * seats as f32;
        let mut allocation = Vec::new();
        let mut sources = Vec::new();
        let mut hired = Vec::new();
        let mut share = Vec::new();
        for &f in &sides {
            // At least a hull a seat, even for a side that can't really
            // bring it (what's not there just isn't lost).
            let want = self.reach(system, f).min(cap).floor().max(seats as f32);
            let mut left = want;
            let mut from = Vec::new();
            for (n, h) in self.within_reach(system, f) {
                if left <= 0.0 {
                    break;
                }
                let take = h.min(left);
                from.push((n, take));
                left -= take;
            }
            // What's drawn here is the mercs' in proportion to their share
            // of the side's hulls here.
            let (companies, mercs) = self.on_contract(system, f);
            let mut theirs = Vec::new();
            if let Some(here) = from.first_mut().filter(|h| h.0 == system)
                && mercs > 0.0
            {
                let k = here.1 / self.systems[system].hulls[f];
                theirs = companies.iter().map(|&(id, h)| (id, h * k)).collect();
                here.1 -= mercs * k;
            }
            let here = self.systems[system].hulls[f];
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
        Battle {
            system,
            sides,
            allocation,
            sources,
            hired,
            share,
            site,
        }
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
            match stake {
                Stake::Hulls => {}
                Stake::Site => {
                    // Only a loser's site is lost (never a bystander's).
                    let at_stake = battle.site.filter(|&i| {
                        let owner = self.sites[i].owner as usize;
                        owner != w && battle.sides.contains(&owner)
                    });
                    if let Some(i) = at_stake
                        && self.sites[i].kind == (SiteKind::Shield { down: 0 })
                    {
                        // A generator that's up is knocked down first, as
                        // the galaxy's own battles do; taken once it's down.
                        self.sites[i].kind = SiteKind::Shield {
                            down: super::REPAIR_TICKS,
                        };
                        shield_down = true;
                    } else if let Some(i) = at_stake {
                        let old = self.sites[i].owner as usize;
                        self.sites[i].owner = w as u8;
                        // A taken generator has to be repaired, as in
                        // the galaxy's own battles: its new holder's
                        // model, from the wreck.
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
                }
                Stake::Shield => {
                    let (a, b) = self.systems[battle.system].sites;
                    let losers: Vec<usize> =
                        battle.sides.iter().copied().filter(|&f| f != w).collect();
                    let up = |i: &usize| {
                        let site = &self.sites[*i];
                        losers.contains(&(site.owner as usize))
                            && site.kind == (SiteKind::Shield { down: 0 })
                    };
                    // The battle's own site if it's a losers' generator.
                    let i = battle
                        .site
                        .filter(up)
                        .or_else(|| (a as usize..b as usize).find(up));
                    if let Some(i) = i {
                        self.sites[i].kind = SiteKind::Shield {
                            down: super::REPAIR_TICKS,
                        };
                        shield_down = true;
                    }
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
    /// at least a hull a seat and at most `PER_SEAT` a seat.
    #[test]
    fn a_battle_draws_on_the_systems_hulls() {
        let mut g = galaxy();
        for factions in [&PAIR[..], &Faction::ALL[..]] {
            let b = g.muster(factions, 6);
            let sys = &g.systems[b.system];
            assert!(b.sides.iter().any(|&f| sys.held[f] > 0));
            for (team, &f) in b.sides.iter().enumerate() {
                let a = b.allocation[team];
                assert!((6..=6 * PER_SEAT).contains(&a), "{a}");
                // What's drawn is really there, and the battle's system goes first.
                let drawn: f32 = b.sources[team].iter().map(|s| s.1).sum();
                assert!(drawn <= g.reach(b.system, f) + 1e-3);
                if sys.hulls[f] > 0.0 {
                    assert_eq!(b.sources[team][0].0, b.system);
                }
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
        let s = g.settle(&b, &lost, &[0, 0], Some(0), Stake::Hulls);
        assert!((before[0] - total(&g, imp) - lost[0] as f32).abs() < 0.01);
        assert!((before[1] - total(&g, res) - lost[1] as f32).abs() < 0.01);
        assert!((s.lost[imp] - lost[0] as f32).abs() < 1e-3);
        assert_eq!(s.before.held, s.after.held, "a raid takes no ground");
        assert!(!s.taken && !s.shield_down);
    }

    /// A Hold won by the side that doesn't hold the site takes it; won by
    /// its holder, nothing changes hands.
    #[test]
    fn a_won_site_changes_hands() {
        let mut g = galaxy();
        let b = g.muster(&PAIR, 6);
        let site = b.site.expect("a site at stake");
        let holder = g.sites[site].owner as usize;
        let team = |f: usize| b.sides.iter().position(|&x| x == f).unwrap();
        let other = 1 - team(holder);
        let kept = g.settle(&b, &[0, 0], &[0, 0], Some(team(holder)), Stake::Site);
        assert!(!kept.taken);
        let s = g.settle(&b, &[0, 0], &[0, 0], Some(other), Stake::Site);
        assert!(s.taken);
        assert_eq!(g.sites[site].owner as usize, b.sides[other]);
        assert_eq!(s.after.held[holder] + 1, s.before.held[holder]);
        assert_eq!(
            s.after.held[b.sides[other]],
            s.before.held[b.sides[other]] + 1
        );
    }

    /// A won Raid knocks down one of the losers' generators.
    #[test]
    fn a_won_raid_downs_a_shield() {
        let mut g = galaxy();
        let b = g.muster(&PAIR, 6);
        let holder = g.sites[b.site.unwrap()].owner as usize;
        let team = b.sides.iter().position(|&x| x == holder).unwrap();
        let s = g.settle(&b, &[0, 0], &[0, 0], Some(1 - team), Stake::Shield);
        if s.before.shields[holder] > 0 {
            assert!(s.shield_down);
            assert_eq!(s.after.shields[holder] + 1, s.before.shields[holder]);
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

    /// A battle where a company is under contract, from a warmed galaxy:
    /// the company's side first, against the strongest other side there.
    fn hired_battle(g: &Galaxy) -> Option<Battle> {
        let k = g
            .companies
            .iter()
            .filter_map(|c| c.contract)
            .find(|k| k.field >= 3.0)?;
        let (t, f) = (k.target as usize, k.side as usize);
        let enemy = (0..SIDES)
            .filter(|&e| e != f)
            .max_by(|&a, &b| g.systems[t].hulls[a].total_cmp(&g.systems[t].hulls[b]))?;
        let factions = [Faction::ALL[f], Faction::ALL[enemy]];
        Some(g.muster_at(t, &factions, 6, None))
    }

    /// Mercs under contract in the battle's system are part of their side's
    /// allocation, in proportion to their share of its hulls there, and some
    /// of its seats are the Company's.
    #[test]
    fn hired_sides_bring_mercs() {
        let g = Galaxy::warmed(7);
        let b = hired_battle(&g).expect("a company under contract");
        let f = b.sides[0];
        let mercs = b.mercs(0);
        assert!(mercs > 0.0);
        let realm: f32 = b.sources[0].iter().map(|s| s.1).sum();
        assert!(mercs + realm <= b.allocation[0] as f32 + 1e-3);
        let share = g.on_contract(b.system, f).1 / g.systems[b.system].hulls[f];
        let drawn_here = mercs + b.sources[0][0].1;
        assert!((mercs / drawn_here - share).abs() < 1e-3);
        let seats = b.merc_seats(0, 6);
        assert!(seats < 6 || realm < 1.0);
        let want = ((share * 6.0).round() as usize)
            .min(5)
            .min(mercs.round() as usize);
        assert_eq!(seats, want);
        // A side with no company there brings none.
        let (_, theirs) = g.on_contract(b.system, b.sides[1]);
        if theirs == 0.0 {
            assert_eq!(b.mercs(1), 0.0);
            assert_eq!(b.merc_seats(1, 6), 0);
        }
    }

    /// Some battles are more merc-heavy than others: across the contracts of
    /// a warmed galaxy, the Company's seats differ.
    #[test]
    fn some_battles_have_more_mercs() {
        let g = Galaxy::warmed(7);
        let mut seats = std::collections::BTreeSet::new();
        for k in g.companies.iter().filter_map(|c| c.contract) {
            let (t, f) = (k.target as usize, k.side as usize);
            let enemy = (0..SIDES).find(|&e| e != f).unwrap();
            let b = g.muster_at(t, &[Faction::ALL[f], Faction::ALL[enemy]], 6, None);
            seats.insert(b.merc_seats(0, 6));
        }
        assert!(seats.len() >= 2, "{seats:?}");
    }

    /// The Company's tanks lost come off their contracts (and count towards
    /// their caps); the rest come off the realm's; the side loses them all.
    #[test]
    fn merc_losses_come_off_their_contracts() {
        let mut g = Galaxy::warmed(7);
        let b = hired_battle(&g).expect("a company under contract");
        let f = b.sides[0];
        let field = |g: &Galaxy| -> f32 {
            b.hired[0]
                .iter()
                .filter_map(|&(id, _)| g.companies.iter().find(|c| c.id == id))
                .filter_map(|c| c.contract)
                .map(|k| k.field)
                .sum()
        };
        let total = |g: &Galaxy| g.systems.iter().map(|s| s.hulls[f] as f64).sum::<f64>();
        let (field0, total0) = (field(&g), total(&g));
        let m = b.mercs(0).floor().min(3.0) as u32;
        assert!(m >= 1);
        let s = g.settle(&b, &[m + 2, 0], &[m, 0], None, Stake::Hulls);
        assert!(
            (s.merc_lost[f] - m as f32).abs() < 1e-3,
            "{:?}",
            s.merc_lost
        );
        assert!((field0 - field(&g) - m as f32).abs() < 1e-3);
        assert!((total0 - total(&g) - (m + 2) as f64).abs() < 1e-2);
    }
}
