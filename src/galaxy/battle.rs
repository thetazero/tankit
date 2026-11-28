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
//! **Out.** A battle is a battle (#422, #493): every one, played or the
//! galaxy's simmed ones, ends in an [`Outcome`] (each side's hulls lost,
//! each kill some pilot's, a winner) that goes into the galaxy the one way
//! (`Galaxy::fight`). Only where the outcome comes from differs: a played
//! battle's from the match (`Galaxy::settle`), the others' from a sim
//! (`Galaxy::sim`). Hulls lost come off the system's storage
//! (`Galaxy::lose`); survivors stay there. The winner gets what any
//! battle's winner gets (`Galaxy::win`): the site, from its holder, or, the
//! holder's win, its generator repaired. A draw goes to the holder, and the
//! contract doesn't change what a win does. It's one of its system's
//! battles for the tick, fought first; the others there follow, each with
//! the hulls left (`Galaxy::step_played`).
//!
//! **Mercs** (#320, #493). A company under contract for the battle's
//! system (to take it, or to hold it: #340) has its hulls there among its
//! employer's side's, so a side's allocation drawn from that system is part
//! merc, in proportion: if a third of the side's hulls here are a
//! company's, a third of what it brings is. (Mercs on contract elsewhere
//! stay on their own contracts.) The side's **seats** are split by the
//! mercs' share of its force in the system itself (the attack they were
//! hired to make up), rounded, but never more seats than merc hulls, and
//! never all of them while the side brings any hulls of the realm's own;
//! then between its companies by their hulls (`Battle::company_seats`).
//! Each company's tanks are in its colors, flown by its pilots, and come
//! back in its own hulls while it has any, after that as the realm's. Each
//! company's tanks lost come off its contract (and count towards its cap);
//! the rest come off the realm's.
//!
//! **Your company** (#361, #362, #363). You only fight in battles your
//! company (`Galaxy::crew`) is in: on its contract, for the side that hired
//! it (`deploy_at`). It's one of your side's companies like any (#493),
//! listed first. You're one of its seats, in one of its hulls; when its
//! hulls are spent, so are you, and the match plays out without you (see
//! `Match::crew_out`, #373): its result is whatever the rest of your side
//! makes of it. Some of its hulls are yours alone, set when you start on
//! its contract, more at higher rank ([`own_hulls`], #385): its bots can't
//! spend them, only your deaths do.

use super::{AT_WAR, Galaxy, SHIELD_SEATS, SIDES, SiteKind};
use crate::combat::Match;
use crate::faction::Faction;

/// Of your company's hulls on a contract, how many are yours alone if you
/// start on it at rank `rank` (#385): two at first, one more with each
/// title (every five ranks); never more than half its hulls (see
/// `Galaxy::claim_own`).
pub fn own_hulls(rank: u32) -> u32 {
    2 + rank.saturating_sub(1) / crate::profile::RANKS_PER_TITLE
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
    sources: Vec<Vec<(usize, u32)>>,
    /// Each team's merc hulls, by company: (`Company::id`, hulls). They're
    /// among its allocation.
    hired: Vec<Vec<(u32, u32)>>,
    /// How many of each company's hulls its crew can fly at once.
    crews: Vec<Vec<u32>>,
    /// Each team's mercs' share of its firing hulls in the system.
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
    /// The merc pilots flying the Company's tanks (#438): (fighter, the
    /// company's `Company::id`, the pilot's `Pilot::id`). Set when it begins.
    pub pilots: Vec<(usize, u32, u32)>,
    /// What it started with, for the battle log (#494): set for a battle
    /// you play (`ledger::Setup::of`), none for the others.
    pub setup: Option<Box<super::ledger::Setup>>,
}

/// A wreck a merc pilot takes apart (#438): (`Company::id`, the pilot's
/// `Pilot::id`, part, level).
pub type Scrap = (u32, u32, crate::economy::Part, u8);

/// How a battle ended (#493), played or simmed: the one thing every battle
/// gives the galaxy (`Galaxy::fight`).
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// The site fought over (index into `Galaxy::sites`).
    pub site: Option<usize>,
    /// Each side's hulls lost: the realm's, and each company's
    /// (`Company::id`, hulls).
    pub lost: [(u32, Vec<(u32, u32)>); SIDES],
    /// The side that won the site; none if it's nobody's to win here.
    pub winner: Option<usize>,
    /// The wrecks the kills left, each some merc pilot's to take apart.
    pub scrap: Vec<Scrap>,
}

/// A system's numbers, for the before and after of a battle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snapshot {
    pub hulls: [u32; SIDES],
    pub held: [u8; SIDES],
    /// Shield generators up, each side's.
    pub shields: [u8; SIDES],
}

/// How a played battle went (it's over), to settle it in the galaxy.
#[derive(Clone, Debug)]
pub struct Played {
    pub battle: Battle,
    /// Hulls each team lost (its tanks destroyed), and of those each of its
    /// companies' tanks (as listed in `Battle::hired`).
    pub lost: Vec<u32>,
    pub company: Vec<Vec<u32>>,
    /// The team that won; none for a draw (the holder's, then).
    pub winner: Option<usize>,
    /// Your own hulls lost (#385).
    pub own_lost: u32,
    /// The wrecks each merc pilot's kills left, to take apart (#438).
    pub scrap: Vec<Scrap>,
}

impl Played {
    /// Match `m`'s result (it's over), for battle `b`.
    pub fn of(b: &Battle, m: &Match) -> Played {
        let scrap = b
            .pilots
            .iter()
            .flat_map(|&(i, id, p)| {
                m.fighters[i]
                    .scrap
                    .iter()
                    .map(move |&(part, level)| (id, p, part, level))
            })
            .collect();
        Played {
            battle: b.clone(),
            lost: m.lost(),
            company: m.company_lost(),
            winner: m.leader(),
            own_lost: m.own_lost(),
            scrap,
        }
    }
}

/// What settling a battle did to the galaxy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settled {
    /// The battle's system just before it was settled, and after (in its
    /// tick, before the rest of the tick's battles there).
    pub before: Snapshot,
    pub after: Snapshot,
    /// Hulls each side lost, all told (from every system they were drawn from).
    pub lost: [u32; SIDES],
    /// The site changed hands.
    pub taken: bool,
    /// A shield generator went down.
    pub shield_down: bool,
    /// Of `lost`, the hulls that were mercs'.
    pub merc_lost: [u32; SIDES],
}

/// A system's name for the HUD: its name, and its catalogue number.
pub fn system_name(s: usize) -> String {
    format!("{} · {s:04}", super::theater::system_name(s))
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
        .filter(|b| b.mercs(0) >= 1 && b.allocation.iter().all(|&h| h >= 1))
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

pub(super) fn begin(g: &Galaxy, m: &mut Match, mut b: Battle) -> Battle {
    for team in 0..b.sides.len() {
        if b.shielded[team] {
            m.close_seats(team, SHIELD_SEATS);
        }
    }
    m.set_allocation(&b.allocation);
    for team in 0..b.sides.len() {
        let seats = b.company_seats(team, m.seats(team));
        let companies = b.hired[team]
            .iter()
            .zip(seats)
            .map(|(&(id, h), n)| (Some(super::theater::ship_name(id)), n, h))
            .collect();
        m.set_mercs(team, companies);
    }
    kit(g, m, &mut b);
    log::info!(
        "battle in {} (galaxy tick {}): {}",
        system_name(b.system),
        g.tick,
        describe_allocation(&b)
    );
    b
}

/// Every bot's gear (#438): a realm's tanks fly its tier, in its own
/// parts; a company's drive its pilots' own kit (`Pilot::kit`, #481)
/// whoever hired them, built to what they know, its pilots in turn. You
/// fly yours. The contract pays by the gear it's fought against: the enemy
/// realm's tier.
fn kit(g: &Galaxy, m: &mut Match, b: &mut Battle) {
    let s = b.system;
    let tier = |f: usize| g.tier_at(s, f);
    b.pilots.clear();
    for team in 0..b.sides.len() {
        let f = b.sides[team];
        let companies: Vec<_> = b.hired[team]
            .iter()
            .map(|&(id, _)| g.companies.iter().find(|c| c.id == id))
            .collect();
        let mut next = vec![0; companies.len()];
        for i in 1..m.fighters.len() {
            if m.fighters[i].team != team {
                continue;
            }
            let k = m.seat_company(i);
            match k.and_then(|k| companies[k].map(|c| (k, c))) {
                Some((k, c)) if !c.pilots.is_empty() => {
                    let p = next[k] % c.pilots.len();
                    next[k] += 1;
                    // Their own kit, whoever hired them (#481).
                    let (hull, turret) = c.pilots[p].kit();
                    m.set_kit(i, hull, turret, c.pilots[p].levels());
                    b.pilots.push((i, c.id, c.pilots[p].id));
                }
                _ => m.set_gear(i, crate::economy::Levels::all(tier(f))),
            }
        }
        m.set_realm_gear(team, crate::economy::Levels::all(tier(f)));
    }
    if let Some(&enemy) = b.sides.get(1) {
        m.rating = tier(enemy);
    }
    m.wage = b.crew.and_then(|_| g.pay_rates());
}

/// Feed match `m`'s result (it's over) back into the galaxy: its tick.
pub fn conclude(g: &mut Galaxy, b: &Battle, m: &Match) -> Settled {
    g.step_played(&Played::of(b, m))
}

impl Battle {
    /// Team `team`'s merc hulls, by company: (`Company::id`, hulls).
    pub fn hired(&self, team: usize) -> &[(u32, u32)] {
        self.hired.get(team).map_or(&[], |h| &h[..])
    }

    /// Team `team`'s merc hulls, all told.
    pub fn mercs(&self, team: usize) -> u32 {
        self.hired
            .get(team)
            .map_or(0, |h| h.iter().map(|&(_, n)| n).sum())
    }

    /// Of team `team`'s `seats`, how many are the Company's: the mercs'
    /// share of its hulls in the system, rounded; never more than its merc
    /// hulls, and not all of them (it's the realm's fight) unless the side
    /// brings no hulls of the realm's own. With your company there, at least
    /// one: yours.
    pub fn merc_seats(&self, team: usize, seats: usize) -> usize {
        let share = self.share.get(team).copied().unwrap_or(0.0).min(1.0);
        let mut n = (share * seats as f32).round() as usize;
        let realm: u32 = self
            .sources
            .get(team)
            .map_or(0, |s| s.iter().map(|h| h.1).sum());
        if realm >= 1 {
            n = n.min(seats.saturating_sub(1));
        }
        if team == 0 && self.crew.is_some() {
            n = n.max(1);
        }
        n.min(self.mercs(team) as usize)
    }

    /// Team `team`'s merc seats (`merc_seats`), each company's (#493), as
    /// they're listed in `hired`: by how many hulls each crew flies, never
    /// more seats than hulls; with your company there (first, on your
    /// side), at least one: yours.
    pub fn company_seats(&self, team: usize, seats: usize) -> Vec<usize> {
        let hired = self.hired.get(team).map_or(&[][..], |h| &h[..]);
        let mut n = self.merc_seats(team, seats);
        let mut out = vec![0; hired.len()];
        if team == 0 && self.crew.is_some() && n > 0 {
            out[0] = 1;
            n -= 1;
        }
        // Whole seats by hulls flown, never past a company's hulls.
        let crews = self.crews.get(team).map_or(&[][..], |c| &c[..]);
        let flown = |k: usize| crews.get(k).copied().unwrap_or(hired[k].1);
        while n > 0 {
            let share = |k: usize| flown(k) as f32 / (out[k] + 1) as f32;
            let Some(k) = (0..hired.len())
                .filter(|&k| (out[k] as u32) < hired[k].1)
                .max_by(|&a, &b| share(a).total_cmp(&share(b)))
            else {
                break;
            };
            out[k] += 1;
            n -= 1;
        }
        out
    }
}

fn describe_allocation(b: &Battle) -> String {
    b.sides
        .iter()
        .zip(&b.allocation)
        .enumerate()
        .map(|(t, (&f, a))| {
            let mercs = b.mercs(t);
            if mercs > 0 {
                format!(
                    "{} {a} hulls ({mercs} mercs', {} companies)",
                    Faction::side(f).name(),
                    b.hired[t].len()
                )
            } else {
                format!("{} {a} hulls", Faction::side(f).name())
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
            Faction::side(f).name(),
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
        if s.merc_lost[f] > 0 {
            out += &format!(
                "\n  the {}' mercs lost {:.0} of those",
                Faction::side(f).name(),
                s.merc_lost[f]
            );
        }
    }
    out
}

impl Galaxy {
    /// The companies under contract to take or hold system `t` for side `f`, and
    /// their hulls there: ((`Company::id`, hulls), all told). Never more
    /// than the side has there.
    fn on_contract(&self, t: usize, f: usize) -> (Vec<(u32, u32)>, u32) {
        let mut out = Vec::new();
        for co in &self.companies {
            if let Some(k) = co.contract
                && k.target as usize == t
                && k.side as usize == f
                && k.field > 0
            {
                out.push((co.id, k.field));
            }
        }
        // Never more than the side has here: hand what there is round.
        let mut left = self.systems[t].hulls[f];
        for (_, h) in &mut out {
            *h = (*h).min(left);
            left -= *h;
        }
        out.retain(|&(_, h)| h > 0);
        let total = out.iter().map(|&(_, h)| h).sum();
        (out, total)
    }

    fn reach(&self, s: usize, f: usize) -> u32 {
        self.systems[s].hulls[f]
    }

    /// Where to fight a battle between `factions` (the match's teams, in
    /// order), with `seats` tanks a side in play at once, and what each side
    /// brings. The galaxy picks: a system one of them holds, where they're at
    /// war if anywhere, and where the worst-off side brings the most; at
    /// random among the best.
    pub fn muster(&mut self, factions: &[Faction], seats: usize) -> Battle {
        let sides: Vec<usize> = factions.iter().map(|f| f.index()).collect();
        // Each system's tier (higher is better), and the worst-off side's reach.
        let mut best = (0u8, 0u32);
        let mut picks: Vec<usize> = Vec::new();
        for s in 0..self.systems.len() {
            let sys = &self.systems[s];
            if !sides.iter().any(|&f| sys.held[f] > 0) {
                continue;
            }
            let fighting = sides.iter().filter(|&&f| sys.hulls[f] >= AT_WAR).count() >= 2;
            let least = sides.iter().map(|&f| self.reach(s, f)).min().unwrap_or(0);
            let tier = 1 + (fighting && least >= seats as u32) as u8;
            let score = least;
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
        (b.mercs(0) >= 1).then_some(b)
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
        let mut crews = Vec::new();
        let mut share = Vec::new();
        for (team, &f) in sides.iter().enumerate() {
            let (mut companies, mercs) = self.on_contract(system, f);
            // On your side, your company first among them: you're its.
            if team == 0
                && let Some(id) = crew
            {
                let k = companies.iter().position(|c| c.0 == id)?;
                companies[..=k].rotate_right(1);
            }
            // Every hull it has here, the mercs' among them.
            let want = self.reach(system, f);
            let from = vec![(system, want.saturating_sub(mercs))];
            // Seats go by pilots, not hulls.
            let crew: Vec<u32> = companies
                .iter()
                .map(|&(id, h)| {
                    let pilots = self
                        .company(id)
                        .map_or(0, |c| self.companies[c].pilots.len());
                    h.min(pilots as u32)
                })
                .collect();
            let able = self.able(system, f);
            let crewed: u32 = crew.iter().sum();
            share.push(if able > 0.0 {
                (crewed as f32 / able).min(1.0)
            } else {
                0.0
            });
            crews.push(crew);
            let theirs = companies;
            allocation.push(want);
            sources.push(from);
            hired.push(theirs);
        }
        // The site at stake: the one asked for, else a production site of
        // whichever side holds most here (else any site of theirs).
        let sys = &self.systems[system];
        let holder = sides.iter().copied().max_by_key(|&f| sys.held[f]);
        let (a, b) = sys.sites;
        let held = |i: &usize| Some(self.sites[*i].owner as usize) == holder;
        let site = site.or_else(|| {
            (a as usize..b as usize)
                .filter(held)
                .find(|&i| self.sites[i].kind.produces())
                .or_else(|| (a as usize..b as usize).find(held))
        });
        let shielded = sides
            .iter()
            .map(|&f| self.under_shield(system, site, f))
            .collect();
        Some(Battle {
            system,
            sides,
            allocation,
            sources,
            hired,
            crews,
            share,
            site,
            shielded,
            crew,
            pilots: Vec::new(),
            setup: None,
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
                x.owner == owner
                    && x.planet == planet
                    && x.kind == (SiteKind::Shield { damaged: false })
            })
    }

    pub fn snapshot(&self, s: usize) -> Snapshot {
        let sys = &self.systems[s];
        let mut shields = [0; SIDES];
        for site in &self.sites[sys.sites.0 as usize..sys.sites.1 as usize] {
            if site.kind == (SiteKind::Shield { damaged: false }) {
                shields[site.owner as usize] += 1;
            }
        }
        Snapshot {
            hulls: sys.hulls,
            held: sys.held,
            shields,
        }
    }

    /// Battle `o` in system `s` is over (#493): the one way any battle,
    /// played or simmed, goes into the galaxy. Each side's hulls lost come
    /// off it (`lose`), the merc pilots take apart what they wrecked, and
    /// the winner wins the site (`win`). Returns each side's hulls lost, and
    /// of those the mercs'.
    pub(crate) fn fight(&mut self, s: usize, o: &Outcome) -> ([u32; SIDES], [u32; SIDES]) {
        let (mut total, mut mercs) = ([0; SIDES], [0; SIDES]);
        for (f, (realm, hired)) in o.lost.iter().enumerate() {
            if *realm > 0 || !hired.is_empty() {
                (total[f], mercs[f]) = self.lose(s, f, *realm, hired);
            }
        }
        // (wrecks come company by company, often)
        let mut last: Option<(u32, usize)> = None;
        for &(id, pilot, part, level) in &o.scrap {
            let c = match last {
                Some((at, c)) if at == id => Some(c),
                _ => self.company(id),
            };
            let Some(c) = c else { continue };
            last = Some((id, c));
            let Some(p) = self.companies[c].pilots.iter().position(|p| p.id == pilot) else {
                continue;
            };
            self.companies[c].pilots[p].known.scrap(part, level);
            self.companies[c].pilots[p].learned();
            // And their ship pays them for the kill: a wreck has one hull.
            if matches!(part, crate::economy::Part::Hull(_)) {
                let bonus = self.rules.kill_share * self.companies[c].pilots[p].wage;
                self.bonus(c, p, bonus);
                self.companies[c].kills += 1;
                self.books.kill_bonuses += bonus as f64;
            }
        }
        if let (Some(i), Some(w)) = (o.site, o.winner) {
            self.win(s, i, w);
        }
        (total, mercs)
    }

    /// Feed a played battle's result back in (#422): its outcome is the
    /// match's. Each team's hulls lost (its tanks destroyed, the Company's
    /// tanks' off its mercs), and the winner wins the site (a draw: its
    /// holder), go in the way every battle's do (`fight`); and your own
    /// hulls lost are spent. Doesn't tick the galaxy: it's called in the
    /// tick (`step_played`), the first of its system's battles.
    pub fn settle(&mut self, p: &Played) -> Settled {
        let battle = &p.battle;
        let s = battle.system;
        let before = self.snapshot(s);
        let (shields, taken) = (self.shields_down, self.taken);
        let mut o = Outcome {
            site: battle.site,
            scrap: p.scrap.clone(),
            ..Outcome::default()
        };
        for (team, &f) in battle.sides.iter().enumerate() {
            let drawn: u32 = battle.sources[team].iter().map(|&(_, h)| h).sum();
            let lost = p.lost.get(team).copied().unwrap_or(0);
            // Each company's tanks lost are its; the rest, the realm's.
            let theirs = p.company.get(team).map_or(&[][..], |c| &c[..]);
            let hired: Vec<(u32, u32)> = battle.hired[team]
                .iter()
                .zip(theirs)
                .map(|(&(id, h), &n)| (id, n.min(h)))
                .filter(|h| h.1 > 0)
                .collect();
            let mut hired = hired;
            let mercs: u32 = hired.iter().map(|h| h.1).sum();
            let realm = lost.saturating_sub(mercs).min(drawn);
            // More lost than the realm brought: the rest off its companies,
            // each never more than it brought.
            let mut over = lost.saturating_sub(mercs + realm);
            for &(id, h) in &battle.hired[team] {
                if over == 0 {
                    break;
                }
                let have = hired.iter().find(|x| x.0 == id).map_or(0, |x| x.1);
                let off = over.min(h - have);
                if off > 0 {
                    match hired.iter_mut().find(|x| x.0 == id) {
                        Some(x) => x.1 += off,
                        None => hired.push((id, off)),
                    }
                    over -= off;
                }
            }
            o.lost[f] = (realm, hired);
        }
        // A draw goes to the site's holder; a bystander's site is nobody's
        // to win here.
        if let Some(i) = battle.site {
            let holder = self.sites[i].owner as usize;
            o.winner = battle.sides.contains(&holder).then(|| {
                p.winner
                    .and_then(|t| battle.sides.get(t).copied())
                    .unwrap_or(holder)
            });
        }
        let (lost, merc_lost) = self.fight(s, &o);
        self.spend_own(p.own_lost);
        self.update_share();
        Settled {
            before,
            after: self.snapshot(s),
            lost,
            taken: self.taken > taken,
            shield_down: self.shields_down > shields,
            merc_lost,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::{REGION, Rules, SYSTEMS};

    fn galaxy() -> Galaxy {
        seeded(3)
    }

    /// A small galaxy, a little way in: quick to make, even unoptimized.
    fn seeded(seed: u64) -> Galaxy {
        static KEPT: crate::map::procgen::memo::Memo<u64, Galaxy, 20> =
            crate::map::procgen::memo::Memo::new();
        KEPT.get(&seed, || {
            let mut g = Galaxy::with_size(seed, SYSTEMS / 4, REGION * 0.5, Rules::default());
            for _ in 0..50 {
                g.step();
            }
            g
        })
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
                assert_eq!(b.allocation[team], sys.hulls[f]);
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

    /// Battle `b` played out: each team's `lost` hulls (`company` of them
    /// the Company's tanks), and `winner` (a team; none for a draw).
    pub(crate) fn played(
        b: &Battle,
        lost: &[u32],
        company: &[u32],
        winner: Option<usize>,
    ) -> Played {
        Played {
            battle: b.clone(),
            lost: lost.to_vec(),
            company: company.iter().map(|&c| vec![c]).collect(),
            winner,
            own_lost: 0,
            scrap: Vec::new(),
        }
    }

    /// Hulls lost come off the galaxy, all of them, and are counted;
    /// nothing else does.
    #[test]
    fn losses_go_back_into_the_galaxy() {
        let mut g = galaxy();
        let b = g.muster(&PAIR, 6);
        let total = |g: &Galaxy, f: usize| g.systems.iter().map(|s| s.hulls[f]).sum::<u32>();
        let (imp, res) = (Faction::Imperium.index(), Faction::Resonant.index());
        let before = [total(&g, imp), total(&g, res)];
        let destroyed = g.destroyed;
        let lost = [b.allocation[0].min(20), b.allocation[1].min(7)];
        let s = g.settle(&played(&b, &lost, &[0, 0], None));
        assert_eq!(before[0] - total(&g, imp), lost[0]);
        assert_eq!(before[1] - total(&g, res), lost[1]);
        assert_eq!(s.lost[imp], lost[0]);
        assert!((g.destroyed - destroyed - (lost[0] + lost[1]) as f64).abs() < 0.01);
        assert_eq!(s.before.held, s.after.held, "a draw is the holder's");
        assert!(!s.taken && !s.shield_down);
    }

    /// A battle won by the side that doesn't hold the site takes it
    /// (#419); won by its holder, or drawn, nothing changes hands. The
    /// contract doesn't come into it (#422).
    #[test]
    fn a_won_site_changes_hands() {
        let mut g = galaxy();
        let b = g.muster(&PAIR, 6);
        let site = b.site.expect("a site at stake");
        let holder = g.sites[site].owner as usize;
        let team = |f: usize| b.sides.iter().position(|&x| x == f).unwrap();
        let other = 1 - team(holder);
        let kept = g.settle(&played(&b, &[0, 0], &[0, 0], Some(team(holder))));
        assert!(!kept.taken);
        let drawn = g.settle(&played(&b, &[0, 0], &[0, 0], None));
        assert!(!drawn.taken);
        let s = g.settle(&played(&b, &[0, 0], &[0, 0], Some(other)));
        assert!(s.taken);
        assert_eq!(g.sites[site].owner as usize, b.sides[other]);
        assert_eq!(s.after.held[holder] + 1, s.before.held[holder]);
        assert_eq!(
            s.after.held[b.sides[other]],
            s.before.held[b.sides[other]] + 1
        );
    }

    /// Your battle is its system's battle between its sides for the tick
    /// (#419): the tick it's settled in doesn't fight them there again, so
    /// a site you've just won is still yours after it.
    #[test]
    fn a_won_site_survives_its_tick() {
        let mut tried = 0;
        for seed in 0..20 {
            let mut g = seeded(seed);
            let b = g.muster(&PAIR, 6);
            let Some(site) = b.site else { continue };
            let holder = g.sites[site].owner as usize;
            let Some(t) = b.sides.iter().position(|&x| x == holder) else {
                continue;
            };
            // Only the two of them there: nobody else to take it back.
            let sys = &g.systems[b.system];
            if (0..SIDES).any(|f| !b.sides.contains(&f) && sys.hulls[f] >= AT_WAR) {
                continue;
            }
            let at = g.tick;
            let s = g.step_played(&played(&b, &[0, 0], &[0, 0], Some(1 - t)));
            assert!(s.taken);
            // (with no war left, a step runs on through later ticks)
            if g.tick > at + 1 {
                continue;
            }
            tried += 1;
            assert_eq!(g.sites[site].owner as usize, b.sides[1 - t], "seed {seed}");
        }
        assert!(tried >= 5, "only {tried} seeds had a held site at stake");
    }

    /// A played battle is one of its system's battles, not a stand-in for
    /// all of them (#493): in its tick, its sides' other battles there are
    /// simmed after it, and cost them hulls on top of what the match cost.
    #[test]
    fn the_rest_of_the_system_still_fights() {
        let (mut tried, mut lost) = (0, 0);
        for seed in 0..20 {
            let mut g = seeded(seed);
            let b = g.muster(&PAIR, 6);
            let Some(site) = b.site else { continue };
            // Its sides have other battles there this tick.
            let others = |f: usize| g.commits(b.system, f).iter().any(|&i| i != site);
            if !b.sides.iter().all(|&f| others(f)) {
                continue;
            }
            tried += 1;
            let s = g.step_played(&played(&b, &[0, 0], &[0, 0], None));
            assert_eq!(s.lost, [0; SIDES], "the match itself lost nothing");
            lost += b
                .sides
                .iter()
                .map(|&f| g.moves[b.system][f].lost)
                .sum::<u32>();
        }
        assert!(
            tried >= 3,
            "only {tried} seeds had other battles in the system"
        );
        assert!(
            lost > 0,
            "no hull lost in {tried} played ticks' other battles"
        );
    }

    /// A shield generator is its holder's, working or damaged (#422): won
    /// by another side, it's theirs, damaged; won by its holder (a draw
    /// too), it's working again.
    #[test]
    fn a_generator_is_a_state_machine() {
        let mut g = galaxy();
        let b = g.muster(&PAIR, 6);
        let gen_ = b.site.unwrap();
        g.sites[gen_].kind = SiteKind::Shield { damaged: false };
        let a = g.sites[gen_].owner as usize;
        let ta = b.sides.iter().position(|&x| x == a).unwrap();
        let (tb, bside) = (1 - ta, b.sides[1 - ta]);
        let state = |g: &Galaxy| (g.sites[gen_].owner as usize, g.sites[gen_].kind);
        let working = SiteKind::Shield { damaged: false };
        let damaged = SiteKind::Shield { damaged: true };
        // A working, A wins: A working.
        g.settle(&played(&b, &[0, 0], &[0, 0], Some(ta)));
        assert_eq!(state(&g), (a, working));
        // A working, B wins: B damaged (a shield down).
        let s = g.settle(&played(&b, &[0, 0], &[0, 0], Some(tb)));
        assert!(s.taken && s.shield_down);
        assert_eq!(state(&g), (bside, damaged));
        // B damaged, A wins: A damaged.
        let s = g.settle(&played(&b, &[0, 0], &[0, 0], Some(ta)));
        assert!(s.taken && !s.shield_down);
        assert_eq!(state(&g), (a, damaged));
        // A damaged, a draw: A's, working.
        g.settle(&played(&b, &[0, 0], &[0, 0], None));
        assert_eq!(state(&g), (a, working));
    }

    /// The galaxy's battles follow the same machine: a damaged generator
    /// nobody fights over is working again after a tick, in a pocket too.
    #[test]
    fn a_damaged_generator_nobody_fights_over_is_repaired() {
        let mut g = galaxy();
        let i = g
            .sites
            .iter()
            .position(|x| matches!(x.kind, SiteKind::Shield { .. }))
            .unwrap();
        let s = (0..g.systems.len())
            .find(|&s| (g.systems[s].sites.0 as usize..g.systems[s].sites.1 as usize).contains(&i))
            .unwrap();
        // Nobody else there to fight over it.
        let owner = g.sites[i].owner as usize;
        for f in (0..SIDES).filter(|&f| f != owner) {
            g.systems[s].hulls[f] = 0;
        }
        g.sites[i].kind = SiteKind::Shield { damaged: true };
        g.step();
        assert_eq!(g.sites[i].kind, SiteKind::Shield { damaged: false });
    }

    /// A raid's battles play (#546), whichever side you're on: the raiders'
    /// tanks are all the company's.
    #[test]
    fn a_raid_can_be_played() {
        let mut g = galaxy();
        let (s, e) = loop {
            g.step();
            let raid = (0..g.companies.len()).find_map(|c| {
                let k = g.companies[c].contract?;
                let s = k.target as usize;
                let e = (0..SIDES).find(|&e| e != super::super::RAID && g.systems[s].hulls[e] > 5);
                (k.side as usize == super::super::RAID && k.field > 5)
                    .then_some((s, e?))
                    .filter(|_| !g.companies[c].pilots.is_empty())
            });
            if let Some(x) = raid {
                break x;
            }
            assert!(g.tick < 5000, "no raid");
        };
        for raider in [0, 1] {
            let mut map = crate::map::Map::sanctum();
            map.factions = vec![Faction::side(e); 2];
            map.factions[raider] = Faction::Company;
            let mut m = Match::new(map, 0, 7, 1);
            let b = g.muster_at(s, &m.factions, None, None).unwrap();
            let b = begin(&g, &mut m, b);
            m.start();
            assert_eq!(b.allocation[raider], b.mercs(raider));
            for i in (0..m.fighters.len()).filter(|&i| m.fighters[i].team == raider) {
                assert!(m.world.tanks[i].company || m.world.tanks[i].empty);
            }
            for _ in 0..120 {
                m.step(crate::sim::Command::default(), false);
            }
            m.time_left = 0.0;
            g.settle(&Played::of(&b, &m));
        }
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
    fn on_contract(hulls: u32) -> Galaxy {
        let mut g = Galaxy::warmed_hired(7);
        let c = g
            .companies
            .iter()
            .find(|c| c.contract.is_some_and(|k| k.field >= hulls))
            .expect("a company under contract")
            .id;
        g.crew = Some(c);
        g.stay_on();
        g
    }

    /// Your own hulls are set once when you start on a contract, by your
    /// rank then: ranking up on it adds none, and only your losses take
    /// them. They're never more than half its hulls.
    #[test]
    fn own_hulls_are_set_once_a_contract() {
        let mut g = on_contract(8);
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
        let mut h = on_contract(8);
        h.claim_own(1);
        let c = h.crew_company().unwrap();
        let k = h.companies[c].contract.as_mut().unwrap();
        k.signed += 1;
        let half = k.field.div_ceil(2);
        h.stay_on();
        h.claim_own(46);
        assert_eq!(h.own_left(), own_hulls(46).min(half));
        assert!(h.own_left() <= half);
    }

    /// Your own hulls lost in a battle come off yours on the contract.
    #[test]
    fn a_battle_spends_your_own_hulls() {
        let mut g = on_contract(8);
        g.claim_own(1);
        let c = g.crew_company().unwrap();
        let k = g.companies[c].contract.unwrap();
        let (t, f) = (k.target as usize, k.side as usize);
        let enemy = (0..SIDES)
            .filter(|&e| e != f)
            .max_by_key(|&a| g.systems[t].hulls[a])
            .unwrap();
        let mut map = crate::map::Map::reservoir(0);
        map.factions = vec![Faction::side(f), Faction::side(enemy)];
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
    fn staged(mercs: u32, realm: u32) -> (Galaxy, usize, [Faction; 2]) {
        let mut g = Galaxy::new(1);
        let t = 0;
        let e = g.systems[t].owner();
        let r = g
            .realms
            .iter()
            .position(|x| x.faction as usize != e && x.systems > 0)
            .expect("a realm of another people") as u32;
        let f = g.realms[r as usize].faction as usize;
        g.systems[t].hulls = [0; SIDES];
        g.systems[t].hulls[f] = realm + mercs;
        g.systems[t].hulls[e] = 50;
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
            rate: 0.0,
            billed: 0.0,
            wage: 0.0,
            committed: mercs,
            field: mercs,
            lost: 0,
            ..Default::default()
        });
        (g, t, [Faction::side(f), Faction::side(e)])
    }

    /// Fighting for a site under its holder's shield, the attacker (mercs
    /// and all) fights a seat short (#426): its last seat stays empty. The
    /// holder never does, and with the planet's generators down, nor does
    /// the attacker.
    #[test]
    fn shields_close_an_attackers_seat() {
        let (mut g, t, sides) = staged(20, 40);
        let battle = |g: &Galaxy| {
            let mut map = crate::map::Map::reservoir(0);
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
            m.fighters
                .iter()
                .filter(|f| f.team == team && f.alive)
                .count()
        };
        assert_eq!(fielded(&m, 0), 6 - SHIELD_SEATS);
        assert_eq!(fielded(&m, 1), 6);
        assert!(m.fighters[0].alive, "never your seat");
        // Its generators on the site's planet damaged: no shield.
        let site = b.site.unwrap();
        let planet = g.sites[site].planet;
        let (a, z) = g.systems[t].sites;
        for x in &mut g.sites[a as usize..z as usize] {
            if x.planet == planet && matches!(x.kind, SiteKind::Shield { .. }) {
                x.kind = SiteKind::Shield { damaged: true };
            }
        }
        let (b, m) = battle(&g);
        assert_eq!(b.shielded, vec![false, false]);
        assert_eq!((fielded(&m, 0), fielded(&m, 1)), (6, 6));
    }

    /// A second company, on the same contract as company 0, with `hulls`
    /// of its own there. Returns its id.
    fn second_company(g: &mut Galaxy, hulls: u32) -> u32 {
        let k = g.companies[0].contract.unwrap();
        let id = g.companies.iter().map(|c| c.id).max().unwrap() + 1;
        g.companies.push(super::super::merc::Company {
            id,
            contract: Some(super::super::merc::Contract {
                committed: hulls,
                field: hulls,
                ..k
            }),
            ..g.companies[0].clone()
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
        let (g, t, sides) = staged(20, 40);
        let b = g.muster_at(t, &sides, None, None).unwrap();
        assert_eq!(b.allocation, vec![60, 50]);
        assert_eq!(b.mercs(0), 20);
        assert_eq!(b.sources[0].len(), 1);
        assert_eq!(b.sources[0][0].0, t);
        assert_eq!(b.sources[0][0].1, 40);
        assert_eq!(
            b.merc_seats(0, 6),
            2,
            "a third of the hulls, a third of the seats"
        );
        assert_eq!(b.mercs(1), 0);
        assert_eq!(b.merc_seats(1, 6), 0);
        // Mostly mercs: all but one seat, the realm's.
        let (g, t, sides) = staged(100, 5);
        let b = g.muster_at(t, &sides, None, None).unwrap();
        assert_eq!(b.merc_seats(0, 6), 5);
        // All mercs: every seat.
        let (g, t, sides) = staged(20, 0);
        let b = g.muster_at(t, &sides, None, None).unwrap();
        assert_eq!(b.merc_seats(0, 6), 6);
        // Never more seats than merc hulls.
        let (g, t, sides) = staged(2, 0);
        let b = g.muster_at(t, &sides, None, None).unwrap();
        assert_eq!(b.merc_seats(0, 6), 2);
    }

    /// The Company's tanks lost come off its contract; the rest come off
    /// the realm's hulls; the side loses them all.
    #[test]
    fn merc_losses_come_off_their_contracts() {
        let (mut g, t, sides) = staged(20, 40);
        let f = sides[0].index();
        let b = g.muster_at(t, &sides, None, None).unwrap();
        let s = g.settle(&super::tests::played(&b, &[5, 0], &[3, 0], None));
        assert_eq!(s.merc_lost[f], 3, "{:?}", s.merc_lost);
        assert_eq!(s.lost[f], 5, "{:?}", s.lost);
        let k = g.companies[0].contract.unwrap();
        assert_eq!(k.field, 17);
        assert_eq!(k.lost, 3);
        assert_eq!(g.systems[t].hulls[f], 55);
        // More Company tanks lost than it brought: the rest are the realm's.
        let (mut g, t, sides) = staged(2, 40);
        let b = g.muster_at(t, &sides, None, None).unwrap();
        let s = g.settle(&super::tests::played(&b, &[5, 0], &[5, 0], None));
        assert_eq!(s.merc_lost[f], 2, "{:?}", s.merc_lost);
        assert_eq!(g.systems[t].hulls[f], 37);
    }

    /// You only fight in battles your company is in (#362): on its
    /// contract, for the side that hired it. Anywhere else, or without a
    /// company, there's no battle for you.
    #[test]
    fn you_only_fight_where_your_company_is() {
        let (mut g, t, sides) = staged(20, 40);
        g.crew = None;
        assert!(g.muster_crew(t, &sides, None).is_none(), "no company");
        g.crew = Some(g.companies[0].id);
        g.stay_on();
        let b = g
            .muster_crew(t, &sides, None)
            .expect("your company's battle");
        assert_eq!(b.crew, g.crew);
        assert_eq!(b.mercs(0), 20);
        assert!(b.merc_seats(0, 6) >= 1, "you're one of its seats");
        // For the other side, or in another system, it isn't there.
        let flipped = [sides[1], sides[0]];
        assert!(g.muster_crew(t, &flipped, None).is_none());
        assert!(g.muster_crew(t + 1, &sides, None).is_none());
        // Its contract over, it's not in any battle.
        g.companies[0].contract = None;
        assert!(g.muster_crew(t, &sides, None).is_none());
    }

    /// In a battle you play, every company on your side is in it, as in
    /// any battle (#493): yours first (you're its), each with its own hulls
    /// and seats, and each one's losses off its own contract.
    #[test]
    fn every_company_on_your_side_fights() {
        let (mut g, t, sides) = staged(20, 40);
        let f = sides[0].index();
        let mine = g.companies[0].id;
        g.crew = Some(mine);
        g.stay_on();
        let alone = g.muster_crew(t, &sides, None).unwrap();
        let other = second_company(&mut g, 30);
        let b = g.muster_crew(t, &sides, None).unwrap();
        assert_eq!(b.hired[0][0].0, mine, "yours first");
        assert!(b.hired[0].iter().any(|&(id, _)| id == other));
        assert!(b.mercs(0) > alone.mercs(0));
        let seats = b.company_seats(0, 6);
        assert!(seats[0] >= 1, "you're one of yours");
        assert_eq!(seats.iter().sum::<usize>(), b.merc_seats(0, 6));
        // A battle you're not in brings the same.
        let pooled = g.muster_at(t, &sides, None, None).unwrap();
        assert_eq!(pooled.allocation[0], b.allocation[0]);
        // Each company's losses come off its own contract.
        let field = |g: &Galaxy, id: u32| {
            g.companies
                .iter()
                .find(|c| c.id == id)
                .unwrap()
                .contract
                .unwrap()
                .field
        };
        let (had_mine, had_other) = (field(&g, mine), field(&g, other));
        let mut p = super::tests::played(&b, &[6, 0], &[0, 0], None);
        let k = b.hired[0].iter().position(|&(id, _)| id == other).unwrap();
        p.company[0] = vec![0; b.hired[0].len()];
        p.company[0][0] = 3;
        p.company[0][k] = 2;
        let s = g.settle(&p);
        assert_eq!(s.merc_lost[f], 5);
        assert_eq!(field(&g, mine), had_mine - 3);
        assert_eq!(field(&g, other), had_other - 2);
    }
}

#[cfg(test)]
mod gear_tests {
    use super::*;
    use crate::economy::{Known, Levels, PARTS, Part};

    /// The company under contract (8 hulls or more out) that's the biggest
    /// share of its side's hulls where it fights: it'll get bots' seats.
    fn hired_company(g: &Galaxy) -> usize {
        let share = |c: usize| {
            let k = g.companies[c].contract.unwrap();
            k.field as f32 / g.systems[k.target as usize].hulls[k.side as usize].max(1) as f32
        };
        (0..g.companies.len())
            .filter(|&c| g.companies[c].contract.is_some_and(|k| k.field >= 8))
            .max_by(|&a, &b| share(a).total_cmp(&share(b)))
            .expect("a company under contract")
    }

    /// A played battle's bots fly the gear of who they are (#438): a
    /// realm's tanks its tier, a company's its pilots' own. The contract
    /// pays by the enemy realm's tier. The pilots learn from their kills.
    #[test]
    fn bots_fly_their_realms_and_pilots_gear() {
        let mut g = Galaxy::warmed_hired(7);
        let c = hired_company(&g);
        let k = g.companies[c].contract.unwrap();
        let (t, f) = (k.target as usize, k.side as usize);
        g.crew = Some(g.companies[c].id);
        g.stay_on();
        let enemy = (0..SIDES)
            .filter(|&e| e != f)
            .max_by_key(|&a| g.systems[t].hulls[a])
            .unwrap();
        for side in [f, enemy] {
            let r = g.systems[t].realms[side];
            if r != crate::galaxy::realm::NONE {
                g.realms[r as usize].tier = if side == f { 4 } else { 13 };
            }
        }
        let mut veteran = Known::default();
        for p in PARTS {
            veteran.set(p, 7.5);
        }
        for p in &mut g.companies[c].pilots {
            p.known = veteran;
        }
        let mut map = crate::map::Map::reservoir(0);
        map.factions = vec![Faction::side(f), Faction::side(enemy)];
        let mut m = crate::combat::Match::new(map, 0, 11, 21);
        let b = g
            .muster_crew(t, &m.factions, None)
            .expect("your company's battle");
        let b = begin(&g, &mut m, b);
        assert!(!b.pilots.is_empty(), "the Company's bots fly its pilots");
        let enemy_realm = g.systems[t].realms[enemy] != crate::galaxy::realm::NONE;
        for i in 1..m.fighters.len() {
            let want = if m.world.tanks[i].company {
                Levels::all(7)
            } else if m.fighters[i].team == 1 {
                Levels::all(if enemy_realm { 13 } else { 0 })
            } else {
                Levels::all(4)
            };
            assert_eq!(m.fighters[i].levels, want, "fighter {i}");
        }
        if enemy_realm {
            assert_eq!(m.rating, 13, "pays by the enemy's gear");
        }
        // A pilot's kill of a T1.3 tank teaches them.
        let (i, id, p) = b.pilots[0];
        let kontos = Part::Turret(crate::sim::TurretKind::Kontos);
        m.fighters[i].scrap.push((kontos, 13));
        let played = Played::of(&b, &m);
        g.settle(&played);
        let co = g.companies.iter().find(|c| c.id == id).unwrap();
        let pilot = co.pilots.iter().find(|x| x.id == p).unwrap();
        assert!(pilot.known.of(kontos) > 7.5);
    }

    /// Mercs drive their own kit, not their employer's (#481): a company
    /// that knows Imperium parts best fights in them for whoever hired it,
    /// on its employer's side. A seat that goes over to the realm comes
    /// back in the realm's own parts and tier.
    #[test]
    fn mercs_drive_their_own_kit() {
        use crate::sim::{HullKind, TurretKind};
        let mut g = Galaxy::warmed_hired(7);
        let c = hired_company(&g);
        let k = g.companies[c].contract.unwrap();
        let (t, f) = (k.target as usize, k.side as usize);
        g.crew = Some(g.companies[c].id);
        g.stay_on();
        let enemy = (0..SIDES)
            .filter(|&e| e != f)
            .max_by_key(|&a| g.systems[t].hulls[a])
            .unwrap();
        // The Salt Promise's pilots: Imperium parts at 5, Resonant at 3.
        let mut salt = Known::default();
        for (part, at) in [
            (Part::Hull(HullKind::Kataphrakt), 5.2),
            (Part::Hull(HullKind::Kastron), 5.6),
            (Part::Turret(TurretKind::Kontos), 5.1),
            (Part::Turret(TurretKind::Pelekys), 5.4),
            (Part::Hull(HullKind::Vigil), 3.3),
            (Part::Turret(TurretKind::Hymn), 3.1),
            (Part::Turret(TurretKind::Knell), 3.0),
        ] {
            salt.set(part, at);
        }
        for p in &mut g.companies[c].pilots {
            p.known = salt;
        }
        let mut map = crate::map::Map::reservoir(0);
        map.factions = vec![Faction::side(f), Faction::side(enemy)];
        let mut m = crate::combat::Match::new(map, 0, 11, 21);
        let b = g
            .muster_crew(t, &m.factions, None)
            .expect("your company's battle");
        let b = begin(&g, &mut m, b);
        assert!(!b.pilots.is_empty());
        let realm_hulls = Faction::side(f).hulls();
        let mut seen = 0;
        for i in 1..m.fighters.len() {
            let tank = &m.world.tanks[i];
            if tank.company {
                seen += 1;
                assert_eq!(m.fighters[i].team, 0, "on their employer's side");
                assert_eq!(tank.hull_kind, HullKind::Kastron, "fighter {i}");
                assert_eq!(tank.turret_kind, TurretKind::Pelekys, "fighter {i}");
            } else if m.fighters[i].team == 0 {
                assert!(realm_hulls.contains(&tank.hull_kind), "the realm's own");
            }
        }
        assert!(seen >= 1, "some of the Company's bots fight");
        // In the galaxy's math, their gear is the kit they drive: T0.5.
        assert_eq!(g.companies[c].pilots[0].level(), 5.0);
    }

    /// Realms research their way up a tier at a time (#438), and one that
    /// breaks away keeps its parent's gear.
    #[test]
    fn realms_research_their_gear() {
        let mut g = Galaxy::warmed_hired(3);
        let tiers = |g: &Galaxy| g.living().map(|(_, r)| r.tier as u32).sum::<u32>();
        let (before, mut after) = (tiers(&g), 0);
        // (Until somebody has; every step is a good few ms.)
        for _ in 0..200 {
            g.step();
            after = tiers(&g);
            if after > before {
                break;
            }
        }
        assert!(
            after > before,
            "nobody researched anything: {before} -> {after}"
        );
        assert!(g.living().all(|(_, r)| r.tier <= crate::economy::MAX_LEVEL));
    }
}
