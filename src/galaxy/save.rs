//! The galaxy on disk: `galaxy.json` beside the profile. Only what changes is
//! saved (who owns each site, which generators are down, every side's hulls
//! in every system and whose realm they are, the realms, the mercenary
//! companies, the clock and the dice); the layout of systems and sites is
//! regenerated from the seed.

use std::path::{Path, PathBuf};

use glam::Vec2;
use serde::{Deserialize, Serialize};

use super::merc::{Company, Contract, Debt, Pilot, Pricing, RULES};
use super::realm::{NONE, Realm};
use super::{Galaxy, PEOPLES, REGION, Rules, SIDES, SiteKind};
use crate::rng::Rng;

/// The war every new pilot joins: one seed for everyone.
pub const SEED: u64 = 1;
/// Ticks a new galaxy runs before you join it: long enough that it's a
/// war well under way, its fronts drawn, its realms' research and its merc
/// pilots' gear grown (#438), not one just starting.
const WARMUP: u64 = 600;

#[derive(Serialize, Deserialize)]
struct Saved {
    seed: u64,
    systems: usize,
    region: [f32; 2],
    tick: u64,
    rng: u64,
    /// Each site's owner, and repair ticks to go (0 for production sites and
    /// generators that are up).
    owners: Vec<u8>,
    down: Vec<u8>,
    /// Each system's hulls, side by side: `SIDES` numbers a system. Whole
    /// hulls, kept as numbers that needn't be (older saves had
    /// fractions of hulls: rounded on loading).
    hulls: Vec<f32>,
    /// Each system's yards, likewise: how far along their next hulls are.
    #[serde(default)]
    building: Vec<f32>,
    /// Each system's realm for each side, likewise (`u32::MAX` for none).
    tags: Vec<u32>,
    /// Every realm slot. (A galaxy saved before realms has none of these,
    /// and isn't read: a fresh one starts.)
    realms: Vec<SavedRealm>,
    splits: u64,
    merges: u64,
    /// The mercenary companies (a galaxy saved before them has none, and
    /// keeps the ones a fresh galaxy starts with).
    #[serde(default)]
    companies: Option<Vec<SavedCompany>>,
    #[serde(default)]
    next_company: u32,
    /// The last pilot id handed out.
    #[serde(default)]
    next_pilot: u32,
    /// Your pilot's id.
    #[serde(default)]
    you: Option<u32>,
    /// Your back pay: what companies owe you, and what's reached you.
    #[serde(default)]
    owed_you: Vec<(u32, f32)>,
    #[serde(default)]
    your_wallet: f32,
    /// Pilots on no ship, and what pilots have been signing for.
    #[serde(default)]
    pool: Vec<SavedPilot>,
    #[serde(default)]
    wage_curve: Option<[f32; super::merc::MARKS]>,
    /// What realms pay their pilots, on average; how far they trust their
    /// forecasts of contracts.
    #[serde(default)]
    pay: Option<f32>,
    #[serde(default)]
    trust_ticks: Option<f32>,
    /// What each system's lately been through, and the market's going rates:
    /// (when taken), (chance lost new, old), hull rate, savings.
    #[serde(default)]
    taken_at: Vec<u64>,
    #[serde(default)]
    rates: Option<[f32; 4]>,
    /// What companies have lately made, each, and were all worth.
    #[serde(default)]
    profit: Option<(f32, f32)>,
    /// Every planet's vault: its side, its realm, its crystal.
    #[serde(default)]
    vaults: Vec<(u8, u32, f32)>,
    /// Hulls in flight.
    #[serde(default)]
    fleets: Vec<super::Fleet>,
    /// What a hull's worth in each system, and its best use this tick.
    #[serde(default)]
    hull_worth: Vec<(f32, f32)>,
    /// The share of merc pilots in work lately.
    #[serde(default)]
    employment: Option<f32>,
    /// Where realms have lately looked for a company and found none
    /// (system, how much); ended contracts not yet looked back on.
    #[serde(default)]
    unmet: Vec<(u32, f32)>,
    /// Bonuses realms owe companies: (realm, company id, crystals).
    #[serde(default)]
    debts: Vec<(u32, u32, f32)>,
    /// What's paid back of each a tick.
    #[serde(default)]
    dues: Vec<f32>,
    #[serde(default)]
    destroyed: f64,
    /// Your company's id (#350).
    #[serde(default)]
    crew: Option<u32>,
    /// Your own hulls on its contract (#385).
    #[serde(default)]
    own: Option<super::theater::Own>,
    /// Its last few ended contracts (#414).
    #[serde(default)]
    ended: Vec<super::theater::Ended>,
}

#[derive(Serialize, Deserialize)]
struct SavedCompany {
    at: u32,
    crystals: f32,
    hulls: f32,
    margin: f32,
    founded: u64,
    /// Saves from before #355 have none: idle since the save, then.
    #[serde(default)]
    idle_since: Option<u64>,
    id: u32,
    contract: Option<SavedContract>,
    /// Its crew.
    #[serde(default)]
    crew: Vec<SavedPilot>,
    /// Older saves: its pilots' knowledge, and their ids.
    #[serde(default, skip_serializing)]
    pilots: Vec<Vec<f32>>,
    #[serde(default, skip_serializing)]
    pilot_ids: Vec<u32>,
    /// How it prices pilots (its rule, as an index into `RULES`, and how
    /// far over it bids); its earnings and pilot-ticks worked lately.
    #[serde(default)]
    pricing: Option<(u8, f32)>,
    #[serde(default)]
    patience: Option<f32>,
    #[serde(default)]
    earned: f32,
    #[serde(default)]
    worked: f32,
    /// Its guesses at a contract's losses and kills a hull a tick, and its
    /// contract's hulls lost as of last tick.
    #[serde(default)]
    loss_rate: f32,
    #[serde(default)]
    kill_rate: f32,
    #[serde(default)]
    lost_before: u32,
}

#[derive(Serialize, Deserialize)]
struct SavedPilot {
    id: u32,
    /// What they know of each part.
    known: Vec<f32>,
    #[serde(default)]
    wallet: f32,
    #[serde(default)]
    owed: f32,
    #[serde(default)]
    wage: f32,
    #[serde(default)]
    idle: u32,
    #[serde(default)]
    earning: f32,
    #[serde(default)]
    home: Option<u32>,
}

impl SavedPilot {
    fn of(p: &Pilot) -> SavedPilot {
        SavedPilot {
            id: p.id,
            known: p.known.steps(),
            wallet: p.wallet,
            owed: p.owed,
            wage: p.wage,
            idle: p.idle,
            earning: p.earning,
            home: p.home,
        }
    }

    fn pilot(&self) -> Pilot {
        let mut p = Pilot {
            id: self.id,
            known: crate::economy::Known::from_steps(&self.known),
            rank: 0.0,
            wallet: self.wallet,
            owed: self.owed,
            wage: self.wage,
            idle: self.idle,
            earning: self.earning,
            home: self.home,
        };
        p.learned();
        p
    }
}

#[derive(Serialize, Deserialize)]
struct SavedContract {
    realm: u32,
    side: u8,
    target: u32,
    signed: u64,
    upfront: f32,
    /// What it's billed a tick, and has been so far (older saves had a
    /// bonus instead: they finish unbilled).
    #[serde(default)]
    rate: f32,
    #[serde(default)]
    billed: f32,
    #[serde(default)]
    wage: f32,
    #[serde(alias = "max_loss")]
    committed: f32,
    field: f32,
    lost: f32,
    #[serde(default)]
    defend: bool,
    #[serde(default)]
    lent: bool,
    #[serde(default)]
    seats: u32,
    #[serde(default)]
    arrives: u64,
    #[serde(default)]
    inbound: u32,
    /// What the realm expected of it, and the ticks it expected to pay.
    #[serde(default)]
    expected: f32,
    #[serde(default)]
    expected_ticks: f32,
    #[serde(default)]
    held: u8,
    #[serde(default)]
    worth: f32,
}

impl SavedContract {
    fn of(k: &Contract) -> SavedContract {
        SavedContract {
            realm: k.realm,
            side: k.side,
            target: k.target,
            signed: k.signed,
            upfront: k.upfront,
            rate: k.rate,
            billed: k.billed,
            wage: k.wage,
            committed: k.committed as f32,
            field: k.field as f32,
            lost: k.lost as f32,
            defend: k.defend,
            lent: k.lent,
            seats: k.seats,
            arrives: k.arrives,
            inbound: k.inbound,
            expected: k.expected,
            expected_ticks: k.expected_ticks,
            held: k.held,
            worth: k.worth,
        }
    }

    fn contract(&self) -> Contract {
        Contract {
            realm: self.realm,
            side: self.side,
            target: self.target,
            signed: self.signed,
            upfront: self.upfront,
            rate: self.rate,
            billed: self.billed,
            wage: self.wage,
            committed: whole(self.committed),
            field: whole(self.field),
            lost: whole(self.lost),
            defend: self.defend,
            lent: self.lent,
            seats: self.seats,
            arrives: self.arrives,
            inbound: self.inbound,
            expected: self.expected,
            expected_ticks: self.expected_ticks,
            held: self.held,
            worth: self.worth,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SavedRealm {
    faction: u8,
    seat: u32,
    crystals: f32,
    income: f32,
    founded: u64,
    /// Its gear tier and research toward the next (#438).
    #[serde(default)]
    tier: u8,
    #[serde(default)]
    research: f32,
    /// Its pilots; older saves have none.
    #[serde(default)]
    pilots: Option<u32>,
    /// What it pays each.
    #[serde(default)]
    wage: f32,
    /// Whether it hires mercs (older saves: they all do).
    #[serde(default)]
    hires: Option<bool>,
    /// Systems it was fighting in.
    #[serde(default)]
    fronts: u32,
    /// What a pilot, and a hull, have lately been worth to it.
    #[serde(default)]
    senses: Option<[f32; 4]>,
}

impl Galaxy {
    fn saved(&self, region: Vec2) -> Saved {
        Saved {
            seed: self.seed,
            systems: self.systems.len(),
            region: region.to_array(),
            tick: self.tick,
            rng: self.rng.state(),
            owners: self.sites.iter().map(|s| s.owner).collect(),
            down: self
                .sites
                .iter()
                .map(|s| match s.kind {
                    SiteKind::Shield { damaged } => damaged as u8,
                    SiteKind::Production { .. } => 0,
                })
                .collect(),
            hulls: self
                .systems
                .iter()
                .flat_map(|s| s.hulls.map(|h| h as f32))
                .collect(),
            building: self.systems.iter().flat_map(|s| s.building).collect(),
            tags: self.systems.iter().flat_map(|s| s.realms).collect(),
            realms: self
                .realms
                .iter()
                .map(|r| SavedRealm {
                    faction: r.faction,
                    seat: r.seat,
                    crystals: r.crystals,
                    income: r.income,
                    founded: r.founded,
                    tier: r.tier,
                    research: r.research,
                    pilots: Some(r.pilots),
                    wage: r.wage,
                    hires: Some(r.hires),
                    fronts: r.fronts,
                    senses: Some([r.pilot_worth, r.hull_worth, r.pilot_use, r.hull_use]),
                })
                .collect(),
            splits: self.splits,
            merges: self.merges,
            companies: Some(
                self.companies
                    .iter()
                    .map(|c| SavedCompany {
                        at: c.at,
                        crystals: c.crystals,
                        hulls: c.hulls as f32,
                        margin: c.margin,
                        founded: c.founded,
                        idle_since: Some(c.idle_since),
                        id: c.id,
                        contract: c.contract.as_ref().map(SavedContract::of),
                        crew: c.pilots.iter().map(SavedPilot::of).collect(),
                        pricing: Some((
                            RULES.iter().position(|&r| r == c.pricing.rule).unwrap_or(0) as u8,
                            c.pricing.k,
                        )),
                        patience: Some(c.patience),
                        earned: c.earned,
                        worked: c.worked,
                        loss_rate: c.loss_rate,
                        kill_rate: c.kill_rate,
                        lost_before: c.lost_before,
                        pilots: Vec::new(),
                        pilot_ids: Vec::new(),
                    })
                    .collect(),
            ),
            next_company: self.next_company,
            next_pilot: self.next_pilot,
            you: Some(self.you.id),
            pool: self.pool.iter().map(SavedPilot::of).collect(),
            wage_curve: Some(self.wage_curve),
            pay: Some(self.pay),
            trust_ticks: Some(self.trust_ticks),
            profit: Some((self.profit, self.worth_before)),
            taken_at: self.taken_at.clone(),
            fleets: self.fleets.clone(),
            hull_worth: self.hull_marks(),
            vaults: self
                .vaults
                .iter()
                .map(|v| (v.side, v.realm, v.crystals))
                .collect(),
            rates: Some([self.lose_new, self.lose_old, self.hull_rate, self.savings]),
            employment: Some(self.employment),
            unmet: self
                .unmet
                .iter()
                .enumerate()
                .filter(|&(_, &u)| u > 1e-3)
                .map(|(s, &u)| (s as u32, u))
                .collect(),
            owed_you: self.owed_you.clone(),
            your_wallet: self.you.wallet,
            debts: self
                .debts
                .iter()
                .map(|d| (d.realm, d.company, d.crystals))
                .collect(),
            dues: self.debts.iter().map(|d| d.due).collect(),
            destroyed: self.destroyed,
            crew: self.crew,
            own: self.own,
            ended: self.ended.clone(),
        }
    }

    /// The full galaxy as JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.saved(REGION)).unwrap_or_default()
    }

    fn restore(s: Saved) -> anyhow::Result<Galaxy> {
        let region = Vec2::from_array(s.region);
        let mut g = Galaxy::with_size(s.seed, s.systems, region, Rules::default());
        anyhow::ensure!(
            s.owners.len() == g.sites.len()
                && s.down.len() == g.sites.len()
                && (s.hulls.len() == g.systems.len() * SIDES
                    || s.hulls.len() == g.systems.len() * PEOPLES)
                && s.tags.len() == s.hulls.len(),
            "saved for a different layout"
        );
        for (i, site) in g.sites.iter_mut().enumerate() {
            anyhow::ensure!((s.owners[i] as usize) < SIDES, "a site with no side");
            site.owner = s.owners[i];
            if let SiteKind::Shield { damaged } = &mut site.kind {
                // (An older save counted repair ticks: any left is damaged.)
                *damaged = s.down[i] > 0;
            }
        }
        anyhow::ensure!(
            s.realms
                .iter()
                .all(|r| (r.faction as usize) < SIDES && (r.seat as usize) < g.systems.len()),
            "a realm with no side or seat"
        );
        anyhow::ensure!(
            s.tags
                .iter()
                .all(|&t| t == NONE || (t as usize) < s.realms.len()),
            "hulls of no realm"
        );
        // Older saves have only the peoples' sides.
        let sides = s.hulls.len() / g.systems.len().max(1);
        for (i, sys) in g.systems.iter_mut().enumerate() {
            for f in 0..sides {
                sys.hulls[f] = whole(s.hulls[i * sides + f]);
                sys.building[f] = s.building.get(i * sides + f).copied().unwrap_or(0.0);
            }
            sys.realms[..sides].copy_from_slice(&s.tags[i * sides..(i + 1) * sides]);
            sys.held = [0; SIDES];
            for site in &g.sites[sys.sites.0 as usize..sys.sites.1 as usize] {
                sys.held[site.owner as usize] += 1;
            }
        }
        let rules = g.rules.clone();
        g.realms = s
            .realms
            .iter()
            .map(|r| Realm {
                pilots: r.pilots.unwrap_or_else(|| {
                    Realm {
                        income: r.income,
                        ..Realm::new(r.faction, r.seat, r.founded)
                    }
                    .pilots_wanted(&rules)
                }),
                wage: r.wage,
                hires: r.hires.unwrap_or(true),
                fronts: r.fronts,
                pilot_worth: r.senses.map_or(0.1, |x| x[0]),
                hull_worth: r.senses.map_or(0.2, |x| x[1]),
                pilot_use: r.senses.map_or(0.0, |x| x[2]),
                hull_use: r.senses.map_or(0.0, |x| x[3]),
                crystals: r.crystals,
                income: r.income,
                tier: r.tier.min(crate::economy::MAX_LEVEL),
                research: r.research,
                ..Realm::new(r.faction, r.seat, r.founded)
            })
            .collect();
        g.spans = vec![(0, 0); g.realms.len()];
        // New ids start past every saved one. Saves without companies keep
        // the fresh galaxy's pilots, so start past those too.
        let fresh = if s.companies.is_none() {
            g.next_pilot
        } else {
            0
        };
        g.next_pilot = fresh.max(s.next_pilot).max(
            s.companies
                .iter()
                .flatten()
                .flat_map(|c| {
                    c.pilot_ids
                        .iter()
                        .copied()
                        .chain(c.crew.iter().map(|p| p.id))
                })
                .chain(s.pool.iter().map(|p| p.id))
                .chain(s.you)
                .max()
                .unwrap_or(0),
        );
        g.you = match s.you {
            Some(id) => Pilot::rookie(id),
            None => g.new_pilot(),
        };
        g.you.wallet = s.your_wallet;
        g.owed_you = s.owed_you;
        g.pool = s.pool.iter().map(SavedPilot::pilot).collect();
        if let Some(curve) = s.wage_curve {
            g.wage_curve = curve;
        }
        if let Some(pay) = s.pay {
            g.pay = pay;
        }
        if let Some(e) = s.employment {
            g.employment = e;
        }
        for &(sys, u) in &s.unmet {
            if let Some(x) = g.unmet.get_mut(sys as usize) {
                *x = u;
            }
        }
        if let Some(ticks) = s.trust_ticks {
            g.trust_ticks = ticks;
        }
        if let Some((profit, worth)) = s.profit {
            (g.profit, g.worth_before) = (profit, worth);
        }
        if s.taken_at.len() == g.taken_at.len() {
            g.taken_at = s.taken_at;
        }
        if let Some(x) = s.rates {
            [g.lose_new, g.lose_old, g.hull_rate, g.savings] = x;
        }
        if let Some(companies) = s.companies {
            let (systems, realms) = (g.systems.len(), g.realms.len());
            anyhow::ensure!(
                companies.iter().all(|c| (c.at as usize) < systems
                    && c.contract
                        .as_ref()
                        .is_none_or(|k| ((k.realm as usize) < realms
                            || k.side as usize == super::RAID)
                            && (k.side as usize) < SIDES
                            && (k.target as usize) < systems)),
                "a company nowhere, or working for nobody"
            );
            g.companies = companies
                .into_iter()
                .map(|c| {
                    let mut pilots: Vec<Pilot> = c.crew.iter().map(SavedPilot::pilot).collect();
                    // Older saves: their pilots, given ids if they had none.
                    for (i, k) in c.pilots.iter().enumerate() {
                        let id = c.pilot_ids.get(i).copied();
                        pilots.push(Pilot {
                            known: crate::economy::Known::from_steps(k),
                            ..id.map_or_else(|| g.new_pilot(), Pilot::rookie)
                        });
                    }
                    // Saves from before pilots had ids had crews of six: fill
                    // them up.
                    if s.next_pilot == 0 && c.crew.is_empty() && c.pilot_ids.is_empty() {
                        while pilots.len() < g.rules.crew as usize {
                            pilots.push(g.new_pilot());
                        }
                    }
                    Company {
                        at: c.at,
                        crystals: c.crystals,
                        hulls: whole(c.hulls),
                        margin: c.margin,
                        founded: c.founded,
                        idle_since: c.idle_since.unwrap_or(s.tick),
                        id: c.id,
                        contract: c.contract.as_ref().map(SavedContract::contract),
                        pilots,
                        // Older saves: a rule by its id, at par.
                        pricing: c.pricing.map_or(
                            Pricing {
                                rule: RULES[c.id as usize % RULES.len()],
                                k: 1.0,
                            },
                            |(r, k)| Pricing {
                                rule: RULES[(r as usize).min(RULES.len() - 1)],
                                k,
                            },
                        ),
                        patience: c.patience.unwrap_or(100.0),
                        earned: c.earned,
                        worked: c.worked,
                        loss_rate: c.loss_rate,
                        kill_rate: c.kill_rate,
                        kills: 0,
                        lost_before: c.lost_before,
                    }
                })
                .collect();
            g.next_company = s.next_company;
            anyhow::ensure!(
                s.debts.iter().all(|&(r, _, _)| (r as usize) < realms),
                "a debt owed by nobody"
            );
            g.debts = s
                .debts
                .iter()
                .enumerate()
                .map(|(i, &(realm, company, crystals))| Debt {
                    realm,
                    company,
                    crystals,
                    due: s.dues.get(i).copied().unwrap_or(crystals / 100.0),
                })
                .collect();
        }
        g.destroyed = s.destroyed;
        g.crew = s.crew;
        g.own = s.own;
        g.ended = s.ended;
        (g.tick, g.splits, g.merges) = (s.tick, s.splits, s.merges);
        g.rng = Rng::from_state(s.rng);
        g.set_hull_marks(&s.hull_worth);
        g.fleets = s.fleets;
        let (systems, realms) = (g.systems.len() as u32, g.realms.len() as u32);
        g.fleets.retain(|x| {
            x.to < systems && (x.side as usize) < SIDES && (x.realm == NONE || x.realm < realms)
        });
        if s.vaults.len() == g.vaults.len() {
            for (v, x) in g.vaults.iter_mut().zip(&s.vaults) {
                let realm = if (x.1 as usize) < g.realms.len() {
                    x.1
                } else {
                    NONE
                };
                *v = super::Vault {
                    side: x.0.min(SIDES as u8 - 1),
                    realm,
                    crystals: x.2,
                };
            }
        } else {
            // An older save: each planet's vault is whoever holds most of
            // its sites'.
            for i in 0..g.systems.len() {
                let (a, b) = g.systems[i].sites;
                for planet in 0..g.systems[i].planets {
                    let mut held = [0; SIDES];
                    for x in &g.sites[a as usize..b as usize] {
                        held[x.owner as usize] += (x.planet == planet) as u32;
                    }
                    let side = (0..PEOPLES).max_by_key(|&f| held[f]).unwrap_or(0);
                    let v = &mut g.vaults[g.vault_at[i] as usize + planet as usize];
                    (v.side, v.realm) = (side as u8, NONE);
                }
            }
            g.fill_vaults();
        }
        g.update_share();
        g.refresh();
        Ok(g)
    }

    pub fn from_json(text: &str) -> anyhow::Result<Galaxy> {
        Galaxy::restore(serde_json::from_str(text)?)
    }

    /// A new galaxy for the game: the war already going.
    pub fn fresh() -> Galaxy {
        Galaxy::warmed(SEED)
    }

    /// A galaxy from `seed`, the war already going: as a new pilot joins it.
    pub fn warmed(seed: u64) -> Galaxy {
        Galaxy::warmed_by(seed, WARMUP)
    }

    /// A galaxy from `seed` run `ticks` ticks (tests: a war going, quicker
    /// than the one a pilot joins).
    pub fn warmed_by(seed: u64, ticks: u64) -> Galaxy {
        let mut g = Galaxy::new(seed);
        for _ in 0..ticks {
            g.step();
        }
        // A pilot joining sees what happens from here on.
        g.clear_moves();
        g
    }

    /// Tests: a war where a company's at work for a realm, with hulls of its
    /// own.
    #[cfg(test)]
    pub fn warmed_hired(seed: u64) -> Galaxy {
        let mut g = Galaxy::warmed_by(seed, 100);
        let hired = |g: &Galaxy| {
            g.companies.iter().any(|c| {
                c.pilots.len() >= 4
                    && c.contract.is_some_and(|k| {
                        let (s, f) = (k.target as usize, k.side as usize);
                        !k.lent
                            && f != super::RAID
                            && k.field >= 8
                            && g.fighting(s, f)
                            && !g.commits(s, f).is_empty()
                    })
            })
        };
        while !hired(&g) {
            g.step();
        }
        // Only that work, so it's what's found.
        for c in 0..g.companies.len() {
            let keep = g.companies[c]
                .contract
                .is_some_and(|k| !k.lent && k.side as usize != super::RAID && k.field >= 8);
            if !keep {
                g.companies[c].contract = None;
            }
        }
        g.clear_moves();
        g
    }

    pub fn default_path() -> Option<PathBuf> {
        crate::profile::Profile::default_path().map(|p| p.with_file_name("galaxy.json"))
    }

    /// The galaxy saved at `path`, or a fresh one if there's none (or it
    /// can't be read).
    pub fn load(path: &Path) -> Galaxy {
        match std::fs::read_to_string(path) {
            Ok(text) => match Galaxy::from_json(&text) {
                Ok(g) => {
                    log::info!("galaxy at tick {} from {}", g.tick, path.display());
                    return g;
                }
                Err(e) => log::warn!("galaxy {} unreadable, starting fresh: {e}", path.display()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::info!("new galaxy at {}", path.display())
            }
            Err(e) => log::warn!("galaxy {} unreadable, starting fresh: {e}", path.display()),
        }
        Galaxy::fresh()
    }

    /// Write the galaxy to `path` on a worker thread (write, then rename).
    pub fn save(&self, path: &Path) {
        let (text, path) = (self.to_json(), path.to_path_buf());
        std::thread::spawn(move || {
            if let Err(e) = crate::profile::write_file(&path, &text) {
                log::warn!("couldn't save the galaxy to {}: {e:#}", path.display());
            }
        });
    }
}

/// The galaxy is generated whole if it's not on disk yet, which isn't free
/// (a few hundred ticks of war): so it's loaded on a worker thread at start,
/// and waited for (if it's not in yet) when a battle needs it.
pub struct War {
    job: Option<std::sync::mpsc::Receiver<Galaxy>>,
    galaxy: Option<Galaxy>,
    path: Option<PathBuf>,
}

impl War {
    /// Load the saved galaxy (or make one) in the background.
    pub fn load() -> War {
        let path = Galaxy::default_path();
        let (tx, rx) = std::sync::mpsc::channel();
        let from = path.clone();
        std::thread::spawn(move || {
            let g = match &from {
                Some(p) => Galaxy::load(p),
                None => Galaxy::fresh(),
            };
            let _ = tx.send(g);
        });
        War {
            job: Some(rx),
            galaxy: None,
            path,
        }
    }

    /// The galaxy, waiting for it if it's still loading.
    pub fn get(&mut self) -> &mut Galaxy {
        if let Some(rx) = self.job.take() {
            self.galaxy = Some(rx.recv().unwrap_or_else(|_| Galaxy::fresh()));
        }
        self.galaxy.get_or_insert_with(Galaxy::fresh)
    }

    /// The galaxy, if it's in (never waits).
    pub fn try_get(&mut self) -> Option<&mut Galaxy> {
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok(g) => {
                    self.galaxy = Some(g);
                    self.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.job = None,
            }
        }
        Some(self.galaxy.get_or_insert_with(Galaxy::fresh))
    }

    pub fn save(&mut self) {
        if let Some(path) = self.path.clone() {
            self.get().save(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::SYSTEMS;

    /// Pilot ids stay unique through splits, joins and old saves.
    #[test]
    fn every_pilot_has_their_own_id() {
        let unique = |g: &Galaxy| {
            let mut ids: Vec<u32> = g
                .companies
                .iter()
                .flat_map(|c| c.pilots.iter().map(|p| p.id))
                .chain([g.you.id])
                .collect();
            let n = ids.len();
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), n, "a pilot id used twice");
            assert!(ids.iter().all(|&id| id <= g.next_pilot));
        };
        let mut g = Galaxy::with_size(5, SYSTEMS / 10, REGION * 0.3, Rules::default());
        for _ in 0..30 {
            g.step();
        }
        // One splits, one joins another.
        for c in &mut g.companies {
            c.contract = None;
            c.crystals = 100.0;
        }
        g.debts.clear();
        g.rules.merc_split = 1000.0;
        g.companies[0].crystals = g.rules.merc_split * 3.0;
        g.companies[1].crystals = g.rules.merc_join / 2.0;
        g.split_and_join();
        assert_eq!((g.books.splits, g.books.joins), (1, 1));
        unique(&g);
        // An old save without pilot ids.
        let mut v: serde_json::Value = serde_json::to_value(g.saved(REGION * 0.3)).unwrap();
        v.as_object_mut().unwrap().remove("next_pilot");
        v.as_object_mut().unwrap().remove("you");
        for c in v["companies"].as_array_mut().unwrap() {
            let c = c.as_object_mut().unwrap();
            let crew = c.remove("crew").unwrap();
            let known: Vec<serde_json::Value> = crew
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["known"].clone())
                .collect();
            c.insert("pilots".into(), known.into());
        }
        let mut old = Galaxy::from_json(&v.to_string()).unwrap();
        unique(&old);
        for _ in 0..100 {
            old.step();
        }
        unique(&old);
    }

    /// A save from before vaults gives each to whoever holds its planet.
    #[test]
    fn older_saves_get_vaults() {
        let region = REGION * 0.3;
        let mut g = Galaxy::with_size(5, SYSTEMS / 10, region, Rules::default());
        for _ in 0..60 {
            g.step();
        }
        let mut v = serde_json::to_value(g.saved(region)).unwrap();
        v.as_object_mut().unwrap().remove("vaults");
        let old = Galaxy::from_json(&v.to_string()).unwrap();
        for s in 0..old.systems.len() {
            let (a, b) = old.systems[s].sites;
            for (p, vault) in old.vaults_of(s).iter().enumerate() {
                let held = old.sites[a as usize..b as usize]
                    .iter()
                    .any(|x| x.planet as usize == p && x.owner == vault.side);
                assert!(held, "system {s}: a vault of nobody's here");
            }
        }
        let banks: f32 = old.living().map(|(_, r)| r.crystals).sum();
        let vaults: f32 = old.vaults.iter().map(|v| v.crystals).sum();
        assert!((vaults / banks - 1.0).abs() < 0.05, "{vaults} of {banks}");
    }

    /// A saved galaxy comes back the same, and goes on the same.
    #[test]
    fn a_saved_galaxy_picks_up_where_it_left_off() {
        let mut g = Galaxy::with_size(5, SYSTEMS / 10, REGION * 0.3, Rules::default());
        for _ in 0..30 {
            g.step();
        }
        g.crew = Some(g.companies[0].id);
        g.ended = vec![crate::galaxy::theater::Ended {
            ship: g.companies[0].id,
            signed: 3,
            tick: 20,
            outcome: crate::galaxy::theater::Outcome::Fell,
            defend: true,
            committed: 30,
            lost: 21,
            own: Default::default(),
        }];
        let text = serde_json::to_string(&g.saved(REGION * 0.3)).unwrap();
        let mut back = Galaxy::from_json(&text).unwrap();
        assert_eq!(back.tick, g.tick);
        assert_eq!(back.crew, g.crew);
        assert_eq!(back.ended, g.ended);
        assert_eq!(back.you.id, g.you.id);
        assert_eq!(back.next_pilot, g.next_pilot);
        for (a, b) in g.companies.iter().zip(&back.companies) {
            assert_eq!(a.pilots, b.pilots);
        }
        for (a, b) in g.systems.iter().zip(&back.systems) {
            assert_eq!(a.held, b.held);
            assert_eq!(a.hulls, b.hulls);
            assert_eq!(a.realms, b.realms);
        }
        assert_eq!(g.living().count(), back.living().count());
        assert_eq!(g.companies.len(), back.companies.len());
        assert_eq!(
            g.debts
                .iter()
                .map(|d| (d.realm, d.company, d.crystals))
                .collect::<Vec<_>>(),
            back.debts
                .iter()
                .map(|d| (d.realm, d.company, d.crystals))
                .collect::<Vec<_>>()
        );
        for (a, b) in g.companies.iter().zip(&back.companies) {
            assert_eq!((a.at, a.crystals, a.hulls), (b.at, b.crystals, b.hulls));
            assert_eq!(a.contract.map(|k| k.target), b.contract.map(|k| k.target));
            assert_eq!(
                a.contract.map(|k| (k.field, k.defend)),
                b.contract.map(|k| (k.field, k.defend))
            );
        }
        for _ in 0..10 {
            g.step();
            back.step();
        }
        assert_eq!(g.share, back.share);
    }
}

/// A count of hulls as saved: whole (an older save may have
/// fractions, rounded).
fn whole(h: f32) -> u32 {
    h.max(0.0).round() as u32
}

/// Deserializes a count of hulls saved as a number that may have a
/// fraction (older saves), rounded to whole hulls.
pub(crate) fn whole_hulls<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    f32::deserialize(d).map(whole)
}
