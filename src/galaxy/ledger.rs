//! The battle log (#494): every battle you play is appended to
//! `battles.jsonl` next to the profile, one JSON object a line, so played
//! battles can be set beside what the galaxy's own battle math (`sim`)
//! makes of the same setup, and its curves tuned to match.
//!
//! Each entry has what the battle started with (each side's hulls, gear,
//! seats and mercs, the site, its holder and the contract), what the sim
//! expects of it (`Galaxy::predict`: each side's fire, hulls lost on
//! average and chance to win), and how it went (each side's losses and
//! kills, how long it took, who won, and what that did to the galaxy).
//! It's written when the battle's settled: one you leave early is logged
//! once it's played out without you.

use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use super::battle::{Battle, Settled};
use super::{Galaxy, SiteKind};
use crate::combat::Match;
use crate::economy::Part;
use crate::faction::Faction;
use crate::garage::{hull_name, turret_name};

/// Bumped whenever an entry's fields change meaning.
const VERSION: u32 = 1;

/// Where the log lives: `battles.jsonl` in the game's data directory
/// (`~/.local/share/tankit/` on Linux), beside `hitches.log`.
pub fn path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("tankit").join("battles.jsonl"))
}

/// What the galaxy's battle math expects of one side in a battle
/// (`Galaxy::predict`), with no luck either way.
#[derive(Clone, Debug, Serialize)]
pub struct Predicted {
    pub faction: String,
    /// Its hulls in the system: all of them fight the galaxy's battles.
    pub hulls: u32,
    /// Its gear level there (`Galaxy::gear`): realm and mercs, by hulls.
    pub gear: u8,
    /// Its fire (`Galaxy::fire`): strength, frontage, guns and shields.
    pub fire: f32,
    /// Hulls it loses on average.
    pub lost: f32,
    /// Its chance to win the site.
    pub win: f32,
}

/// A played battle's setup, as it's drawn (`battle::begin`): the galaxy
/// doesn't tick until it's settled, so this is the galaxy it's fought in.
#[derive(Clone, Debug, Serialize)]
pub struct Setup {
    pub galaxy_seed: u64,
    pub tick: u64,
    pub system: usize,
    pub system_name: String,
    /// The site fought over (index into `Galaxy::sites`), its kind, and
    /// whose it was.
    pub site: Option<usize>,
    pub site_kind: Option<String>,
    pub holder: Option<String>,
    pub contract: String,
    /// The enemy realm's gear tier (`Match::rating`).
    pub rating: u8,
    /// Each of the match's teams, yours first.
    pub sides: Vec<Side>,
    /// The galaxy's battle math on the same setup: the attackers, then
    /// the holder (no one's chance to win if the holder isn't fighting).
    pub predicted: Vec<Predicted>,
}

/// One team of a played battle, as it started.
#[derive(Clone, Debug, Serialize)]
pub struct Side {
    pub faction: String,
    /// Hulls it brings, all told (its allocation), mercs' among them.
    pub hulls: u32,
    /// Of those, the realm's own.
    pub realm_hulls: u32,
    /// The realm's gear tier: every part of its own tanks at it.
    pub realm_tier: u8,
    /// Tanks in play at once, and whether it's a seat short under the
    /// holder's shield.
    pub seats: usize,
    pub shielded: bool,
    pub mercs: Vec<Mercs>,
}

/// A merc company on a side.
#[derive(Clone, Debug, Serialize)]
pub struct Mercs {
    pub company: u32,
    pub name: String,
    pub hulls: u32,
    pub seats: usize,
    pub pilots: Vec<Pilot>,
}

/// A merc pilot's kit (#481): hull, turret, and their level in them.
#[derive(Clone, Debug, Serialize)]
pub struct Pilot {
    pub hull: String,
    pub turret: String,
    pub level: f32,
}

/// How a played battle went.
#[derive(Clone, Debug, Serialize)]
struct Result {
    /// Seconds of the round's clock it ran.
    seconds: f32,
    /// Each team's hulls lost (its tanks destroyed), and of those each of
    /// its companies' (as listed in its `mercs`).
    lost: Vec<u32>,
    company_lost: Vec<Vec<u32>>,
    kills: Vec<u32>,
    /// The team ahead at the end (none: a draw), and the side the site
    /// went to (a draw: its holder).
    winner: Option<String>,
    site_to: Option<String>,
    /// The site changed hands; a shield generator went down.
    taken: bool,
    shield_down: bool,
}

/// You, as the battle ended.
#[derive(Clone, Debug, Serialize)]
struct You {
    company: Option<u32>,
    hull: String,
    turret: String,
    hull_level: u8,
    turret_level: u8,
    kills: u32,
    deaths: u32,
    own_lost: u32,
}

#[derive(Serialize)]
struct Entry<'a> {
    version: u32,
    /// Unix seconds when it was settled.
    at: u64,
    #[serde(flatten)]
    setup: &'a Setup,
    you: You,
    result: Result,
}

impl Setup {
    /// Battle `b`'s setup in galaxy `g`, its match `m` set up for it.
    pub fn of(g: &Galaxy, b: &Battle, m: &Match) -> Setup {
        let s = b.system;
        let holder = b.site.map(|i| g.sites[i].owner as usize);
        let tier = |f: usize| match g.systems[s].realms[f] {
            super::realm::NONE => 0,
            r => g.realms[r as usize].tier,
        };
        let sides = (0..b.sides.len())
            .map(|team| {
                let f = b.sides[team];
                let seats = m.seats(team);
                let hired = b.hired(team);
                let mercs = hired
                    .iter()
                    .zip(b.company_seats(team, seats))
                    .map(|(&(id, hulls), seats)| {
                        let pilots = g
                            .companies
                            .iter()
                            .find(|c| c.id == id)
                            .map(|c| {
                                c.pilots
                                    .iter()
                                    .map(|p| {
                                        let (h, t) = p.kit();
                                        Pilot {
                                            hull: hull_name(h).into(),
                                            turret: turret_name(t).into(),
                                            level: p.level(),
                                        }
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        Mercs {
                            company: id,
                            name: super::theater::ship_name(id),
                            hulls,
                            seats,
                            pilots,
                        }
                    })
                    .collect();
                Side {
                    faction: Faction::ALL[f].name().into(),
                    hulls: b.allocation[team],
                    realm_hulls: b.allocation[team].saturating_sub(b.mercs(team)),
                    realm_tier: tier(f),
                    seats,
                    shielded: b.shielded[team],
                    mercs,
                }
            })
            .collect();
        let predicted = match (b.site, holder) {
            (Some(t), Some(h)) => {
                let mut order: Vec<usize> = b.sides.iter().copied().filter(|&f| f != h).collect();
                let fighting = b.sides.contains(&h);
                if fighting {
                    order.push(h);
                }
                let mut p = g.predict(s, t, &order);
                if !fighting {
                    p.iter_mut().for_each(|p| p.win = 0.0);
                }
                p
            }
            _ => Vec::new(),
        };
        Setup {
            galaxy_seed: g.seed,
            tick: g.tick,
            system: s,
            system_name: super::battle::system_name(s),
            site: b.site,
            site_kind: b.site.map(|i| match g.sites[i].kind {
                SiteKind::Production { .. } => "production".into(),
                SiteKind::Shield { damaged: false } => "shield".into(),
                SiteKind::Shield { damaged: true } => "shield (damaged)".into(),
            }),
            holder: holder.map(|f| Faction::ALL[f].name().into()),
            contract: format!("{:?}", m.contract),
            rating: m.rating,
            sides,
            predicted,
        }
    }
}

/// Battle `b` is over (its match `m`) and settled (`s`): its log entry,
/// one line of JSON. None if it has no setup (it's not one you played).
pub fn entry(g: &Galaxy, b: &Battle, m: &Match, s: &Settled) -> Option<String> {
    let setup = b.setup.as_ref()?;
    let name = |t: usize| b.sides.get(t).map(|&f| Faction::ALL[f].name().to_string());
    let p = m.player();
    let tank = &m.world.tanks[0];
    let you = You {
        company: b.crew,
        hull: hull_name(tank.hull_kind).into(),
        turret: turret_name(tank.turret_kind).into(),
        hull_level: p.levels.of(Part::Hull(tank.hull_kind)),
        turret_level: p.levels.of(Part::Turret(tank.turret_kind)),
        kills: p.kills,
        deaths: p.deaths,
        own_lost: m.own_lost(),
    };
    let kills = (0..b.sides.len())
        .map(|t| {
            m.fighters
                .iter()
                .filter(|f| f.team == t)
                .map(|f| f.kills)
                .sum()
        })
        .collect();
    let result = Result {
        seconds: m.round_time() - m.time_left.max(0.0),
        lost: m.lost(),
        company_lost: m.company_lost(),
        kills,
        winner: m.leader().and_then(name),
        site_to: b
            .site
            .map(|i| Faction::ALL[g.sites[i].owner as usize].name().into()),
        taken: s.taken,
        shield_down: s.shield_down,
    };
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let e = Entry {
        version: VERSION,
        at,
        setup,
        you,
        result,
    };
    serde_json::to_string(&e).ok()
}

/// Append battle `b`'s entry to the log (see `entry`).
pub fn record(g: &Galaxy, b: &Battle, m: &Match, s: &Settled) {
    let Some(line) = entry(g, b, m, s) else {
        return;
    };
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| writeln!(f, "{line}"))
    {
        Ok(()) => log::info!("battle logged to {}", path.display()),
        Err(e) => log::warn!("couldn't log the battle to {}: {e}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::SIDES;

    /// A galaxy with you in a company under contract, and its battle,
    /// drawn as the game draws one you play.
    fn played() -> (Galaxy, Battle, Match) {
        let mut g = Galaxy::warmed_by(7, 200);
        let c = g
            .companies
            .iter()
            .find(|c| c.contract.is_some_and(|k| k.field >= 4))
            .expect("a company under contract");
        let (id, k) = (c.id, c.contract.unwrap());
        g.crew = Some(id);
        let (t, f) = (k.target as usize, k.side as usize);
        let enemy = (0..SIDES)
            .filter(|&e| e != f)
            .max_by_key(|&a| g.systems[t].hulls[a])
            .unwrap();
        let mut map = crate::map::Map::saltmarket();
        map.factions = vec![Faction::ALL[f], Faction::ALL[enemy]];
        let mut m = Match::new(map, 0, 5, 21);
        let b = g.muster_crew(t, &m.factions, None).expect("your battle");
        let mut b = super::super::battle::begin(&g, &mut m, b);
        b.setup = Some(Box::new(Setup::of(&g, &b, &m)));
        m.start();
        (g, b, m)
    }

    /// An entry has what the battle started with, what the galaxy's math
    /// expects of it, and how it went; one line of JSON.
    #[test]
    fn a_played_battle_is_logged_whole() {
        let (mut g, b, mut m) = played();
        m.destroy_player();
        let s = super::super::battle::conclude(&mut g, &b, &m);
        let line = entry(&g, &b, &m, &s).expect("an entry");
        assert!(!line.contains('\n'));
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["version"], VERSION);
        assert_eq!(v["system"], b.system);
        let sides = v["sides"].as_array().unwrap();
        assert_eq!(sides.len(), b.sides.len());
        assert_eq!(sides[0]["hulls"], b.allocation[0]);
        assert!(
            !sides[0]["mercs"].as_array().unwrap().is_empty(),
            "your company's there"
        );
        assert!(!v["predicted"].as_array().unwrap().is_empty());
        assert_eq!(v["you"]["deaths"], 1);
        assert_eq!(v["result"]["lost"][0], 1);
        assert!(v["result"]["seconds"].as_f64().unwrap() >= 0.0);
    }

    /// A battle drawn some other way (bots only, the galaxy's) isn't logged.
    #[test]
    fn only_played_battles_are_logged() {
        let (mut g, mut b, m) = played();
        b.setup = None;
        let s = super::super::battle::conclude(&mut g, &b, &m);
        assert!(entry(&g, &b, &m, &s).is_none());
    }

    /// The prediction is the sim's own math: over many sims of the same
    /// battle, each side's hulls lost average out to what it predicts,
    /// and the holder keeps the site about as often.
    #[test]
    fn the_prediction_is_what_the_sim_averages() {
        let (mut g, b, _) = played();
        let t = b.site.expect("a site at stake");
        let h = g.sites[t].owner as usize;
        let mut sides: Vec<usize> = b.sides.iter().copied().filter(|&f| f != h).collect();
        sides.push(h);
        let p = g.predict(b.system, t, &sides);
        let runs = 400;
        let (mut lost, mut kept) = ([0.0f32; SIDES], 0);
        for _ in 0..runs {
            let o = g.sim(b.system, t, &sides);
            for &f in &sides {
                let (realm, mercs) = &o.lost[f];
                lost[f] += (realm + mercs.iter().map(|m| m.1).sum::<u32>()) as f32;
            }
            kept += (o.winner == Some(h)) as u32;
        }
        for (i, &f) in sides.iter().enumerate() {
            let mean = lost[f] / runs as f32;
            let want = p[i].lost;
            assert!(
                (mean - want).abs() <= 0.15 * want + 0.5,
                "{}: simmed {mean:.2}, predicted {want:.2}",
                p[i].faction
            );
        }
        let keep = kept as f32 / runs as f32;
        let want = p.last().unwrap().win;
        assert!(
            (keep - want).abs() < 0.08,
            "kept {keep:.2}, predicted {want:.2}"
        );
    }
}
