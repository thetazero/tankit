//! The pilot profile: trust with each faction, crystals, owned parts and the
//! loadout you deploy in. One profile, saved as JSON in the OS data directory
//! (`~/.local/share/tankit/profile.json` on Linux) whenever it changes, so
//! there's no save button.
//!
//! Every faction starts you on the basic kit. Work its contracts and it trusts
//! you more; its standing (derived from that trust) decides which of its parts
//! it will sell you. Crystals, paid for finished contracts, buy parts and upgrades.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::economy::{Levels, PARTS, Part, upgrade_cost};
use crate::faction::Faction;
use crate::garage::Loadout;
use crate::sim::{HullKind, TurretKind};

/// Trust from one standing to the next: 100, then 50 more for each after.
fn standing_span(level: u32) -> u32 {
    100 + 50 * (level - 1)
}

/// Where a faction's trust in you sits on the standing ladder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Standing {
    /// Starts at 1: the basic kit.
    pub level: u32,
    /// Trust earned since reaching this standing.
    pub into: u32,
    /// Trust this standing takes to get through.
    pub span: u32,
}

impl Standing {
    pub fn of(trust: u32) -> Standing {
        let (mut level, mut left) = (1, trust);
        while left >= standing_span(level) {
            left -= standing_span(level);
            level += 1;
        }
        Standing {
            level,
            into: left,
            span: standing_span(level),
        }
    }

    pub fn progress(&self) -> f32 {
        self.into as f32 / self.span as f32
    }
}

/// What's on disk. Parts are stored by name so the file stays readable;
/// fields added later default, so older profiles still load.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Saved {
    /// Trust by faction name; missing means none.
    trust: BTreeMap<String, u32>,
    /// Profiles from before trust had one XP total, earned working for the
    /// Imperium (the only side that hired). Read once, never written.
    #[serde(skip_serializing)]
    xp: u32,
    crystals: u32,
    hull: String,
    turret: String,
    /// Bought parts (starters are always owned).
    owned: Vec<String>,
    /// Upgrade level by part name; missing means M0.
    levels: BTreeMap<String, u8>,
    settings: Settings,
}

/// Player options from the settings panel, saved with the profile.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Debug overlay: an FPS counter in the corner.
    pub debug: bool,
    /// Sound volume in steps, 0 (silent) to [`Settings::MAX_VOLUME`].
    pub volume: u8,
}

impl Settings {
    pub const MAX_VOLUME: u8 = 10;

    /// Volume as a fraction, 0..1.
    pub fn volume(&self) -> f32 {
        self.volume.min(Self::MAX_VOLUME) as f32 / Self::MAX_VOLUME as f32
    }

    /// Volume `delta` steps up or down, held within range.
    pub fn nudge_volume(mut self, delta: i32) -> Settings {
        self.volume = (self.volume as i32 + delta).clamp(0, Self::MAX_VOLUME as i32) as u8;
        self
    }
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            debug: false,
            volume: 8,
        }
    }
}

/// Why a purchase didn't go through.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Refusal {
    Owned,
    NotOwned,
    /// Needs this standing with the part's maker.
    Trust(Faction, u32),
    Crystals(u32),
    MaxLevel,
}

pub struct Profile {
    /// Trust with each faction, by `Faction::index`.
    pub trust: [u32; Faction::ALL.len()],
    pub crystals: u32,
    /// The kit you deploy in (and respawn in, until the garage changes it).
    pub loadout: Loadout,
    /// Bought parts, starters not included.
    bought: Vec<Part>,
    pub levels: Levels,
    pub settings: Settings,
    /// Where to save; `None` keeps the profile in memory only (headless runs, tests).
    path: Option<PathBuf>,
}

impl Profile {
    /// A fresh pilot that is never written to disk.
    pub fn in_memory() -> Profile {
        Profile {
            trust: [0; Faction::ALL.len()],
            crystals: 0,
            loadout: (HullKind::Kataphrakt, TurretKind::Kontos),
            bought: Vec::new(),
            levels: Levels::default(),
            settings: Settings::default(),
            path: None,
        }
    }

    pub fn default_path() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("tankit").join("profile.json"))
    }

    /// Load from `path`, or start a fresh pilot there if it's missing or unreadable.
    pub fn load(path: PathBuf) -> Profile {
        let mut p = Profile {
            path: Some(path.clone()),
            ..Profile::in_memory()
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Saved>(&text) {
                Ok(s) => p.restore(s),
                Err(e) => log::warn!("profile {} unreadable, starting fresh: {e}", path.display()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::info!("new pilot profile at {}", path.display())
            }
            Err(e) => log::warn!("profile {} unreadable, starting fresh: {e}", path.display()),
        }
        p
    }

    fn restore(&mut self, s: Saved) {
        for (name, &t) in &s.trust {
            if let Some(f) = Faction::by_name(name) {
                self.trust[f.index()] = t;
            }
        }
        if s.trust.is_empty() {
            self.trust[Faction::Imperium.index()] = s.xp;
        }
        self.crystals = s.crystals;
        self.settings = s.settings;
        self.bought = s.owned.iter().filter_map(|n| Part::by_name(n)).collect();
        for (name, level) in &s.levels {
            if let Some(part) = Part::by_name(name) {
                self.levels.set(part, *level);
            }
        }
        // Only deploy in parts you own (older profiles predate buying).
        if let Some(Part::Hull(h)) = Part::by_name(&s.hull).filter(|&p| self.owns(p)) {
            self.loadout.0 = h;
        }
        if let Some(Part::Turret(t)) = Part::by_name(&s.turret).filter(|&p| self.owns(p)) {
            self.loadout.1 = t;
        }
    }

    pub fn trust_of(&self, faction: Faction) -> u32 {
        self.trust[faction.index()]
    }

    pub fn standing(&self, faction: Faction) -> Standing {
        Standing::of(self.trust_of(faction))
    }

    /// Standing that decides whether `part` is for sale: with its maker.
    pub fn standing_for(&self, part: Part) -> u32 {
        self.standing(part.maker()).level
    }

    /// Every faction trusts you this much (headless shots, tests).
    pub fn set_trust_all(&mut self, trust: u32) {
        self.trust = [trust; Faction::ALL.len()];
    }

    pub fn owns(&self, part: Part) -> bool {
        part.price().is_none() || self.bought.contains(&part)
    }

    pub fn owns_kit(&self, kit: Loadout) -> bool {
        self.owns(Part::Hull(kit.0)) && self.owns(Part::Turret(kit.1))
    }

    /// Can `part` be bought right now? Its price if so.
    pub fn can_buy(&self, part: Part) -> Result<u32, Refusal> {
        let Some(price) = part.price().filter(|_| !self.owns(part)) else {
            return Err(Refusal::Owned);
        };
        if self.standing_for(part) < price.standing {
            return Err(Refusal::Trust(part.maker(), price.standing));
        }
        if self.crystals < price.crystals {
            return Err(Refusal::Crystals(price.crystals));
        }
        Ok(price.crystals)
    }

    pub fn buy(&mut self, part: Part) -> Result<(), Refusal> {
        let cost = self.can_buy(part)?;
        self.crystals -= cost;
        self.bought.push(part);
        self.save();
        Ok(())
    }

    /// Can `part` be upgraded right now? The cost if so.
    pub fn can_upgrade(&self, part: Part) -> Result<u32, Refusal> {
        if !self.owns(part) {
            return Err(Refusal::NotOwned);
        }
        let cost = upgrade_cost(self.levels.of(part)).ok_or(Refusal::MaxLevel)?;
        if self.crystals < cost {
            return Err(Refusal::Crystals(cost));
        }
        Ok(cost)
    }

    pub fn upgrade(&mut self, part: Part) -> Result<(), Refusal> {
        let cost = self.can_upgrade(part)?;
        self.crystals -= cost;
        self.levels.set(part, self.levels.of(part) + 1);
        self.save();
        Ok(())
    }

    /// Bank trust earned working for `faction` and save.
    pub fn add_trust(&mut self, faction: Faction, trust: u32) {
        if trust > 0 {
            self.trust[faction.index()] += trust;
            self.save();
        }
    }

    /// Bank contract pay and save.
    pub fn add_crystals(&mut self, crystals: u32) {
        if crystals > 0 {
            self.crystals += crystals;
            self.save();
        }
    }

    /// Change the deploy kit and save.
    pub fn set_loadout(&mut self, kit: Loadout) {
        if kit != self.loadout {
            self.loadout = kit;
            self.save();
        }
    }

    /// Change the settings and save.
    pub fn set_settings(&mut self, settings: Settings) {
        if settings != self.settings {
            self.settings = settings;
            self.save();
        }
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Err(e) = self.write(path) {
            log::warn!("couldn't save profile to {}: {e:#}", path.display());
        }
    }

    fn write(&self, path: &Path) -> anyhow::Result<()> {
        let name = |p: Part| p.name().to_ascii_lowercase();
        let saved = Saved {
            trust: Faction::ALL
                .into_iter()
                .filter(|&f| self.trust_of(f) > 0)
                .map(|f| (f.name().to_ascii_lowercase(), self.trust_of(f)))
                .collect(),
            xp: 0,
            crystals: self.crystals,
            hull: name(Part::Hull(self.loadout.0)),
            turret: name(Part::Turret(self.loadout.1)),
            owned: self.bought.iter().map(|&p| name(p)).collect(),
            levels: PARTS
                .iter()
                .filter(|&&p| self.levels.of(p) > 0)
                .map(|&p| (name(p), self.levels.of(p)))
                .collect(),
            settings: self.settings,
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write then rename, so a crash mid-save can't leave a half-written profile.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&saved)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_ladder() {
        assert_eq!(
            Standing::of(0),
            Standing {
                level: 1,
                into: 0,
                span: 100
            }
        );
        assert_eq!(Standing::of(99).level, 1);
        assert_eq!(
            Standing::of(100),
            Standing {
                level: 2,
                into: 0,
                span: 150
            }
        );
        assert_eq!(
            Standing::of(260),
            Standing {
                level: 3,
                into: 10,
                span: 200
            }
        );
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("tankit-profile-test-{}", std::process::id()));
        let path = dir.join("profile.json");
        let _ = std::fs::remove_dir_all(&dir);

        let mut p = Profile::load(path.clone());
        assert_eq!(p.trust_of(Faction::Imperium), 0);
        p.add_trust(Faction::Imperium, 300);
        p.add_trust(Faction::Resonant, 40);
        p.add_crystals(2000);
        p.buy(PELEKYS).unwrap();
        p.upgrade(PELEKYS).unwrap();
        p.set_loadout((HullKind::Kataphrakt, TurretKind::Pelekys));
        p.set_settings(Settings {
            debug: true,
            volume: 3,
        });

        let back = Profile::load(path.clone());
        assert_eq!(back.trust_of(Faction::Imperium), 300);
        assert_eq!(back.trust_of(Faction::Resonant), 40);
        assert_eq!(back.trust_of(Faction::Ithessar), 0);
        assert_eq!(back.crystals, p.crystals);
        assert!(back.owns(PELEKYS));
        assert_eq!(back.levels.of(PELEKYS), 1);
        assert_eq!(back.loadout, (HullKind::Kataphrakt, TurretKind::Pelekys));
        assert!(back.settings.debug);
        assert_eq!(back.settings.volume, 3);

        // An older profile (XP, no crystals, deploying in a part it never
        // bought) still loads; its XP was earned for the Imperium.
        std::fs::write(&path, r#"{"xp": 40, "hull": "bastion", "turret": "lance"}"#).unwrap();
        let old = Profile::load(path.clone());
        assert_eq!(old.trust_of(Faction::Imperium), 40);
        assert_eq!(old.trust_of(Faction::Resonant), 0);
        assert_eq!(old.crystals, 0);
        assert_eq!(old.loadout, (HullKind::Kataphrakt, TurretKind::Kontos));
        assert!(!old.settings.debug);
        // Profiles from before sound get the default volume, not silence.
        assert_eq!(old.settings.volume, Settings::default().volume);

        // A corrupt file starts a fresh pilot rather than failing.
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Profile::load(path).trust, [0; Faction::ALL.len()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    const PELEKYS: Part = Part::Turret(TurretKind::Pelekys);

    #[test]
    fn buying_needs_trust_and_crystals() {
        let mut p = Profile::in_memory();
        assert!(p.owns(Part::Hull(HullKind::Kataphrakt)));
        assert!(!p.owns(PELEKYS));
        let price = PELEKYS.price().unwrap();
        let locked = Err(Refusal::Trust(Faction::Imperium, price.standing));
        assert_eq!(p.buy(PELEKYS), locked);
        // Trust with someone else doesn't sell you Imperium guns.
        p.add_trust(Faction::Resonant, 10_000);
        assert_eq!(p.buy(PELEKYS), locked);
        p.add_trust(Faction::Imperium, 10_000);
        assert_eq!(p.buy(PELEKYS), Err(Refusal::Crystals(price.crystals)));
        assert_eq!(p.upgrade(PELEKYS), Err(Refusal::NotOwned));
        p.crystals = price.crystals;
        p.buy(PELEKYS).unwrap();
        assert_eq!(p.crystals, 0);
        assert_eq!(p.buy(PELEKYS), Err(Refusal::Owned));
        p.crystals = 100_000;
        for _ in 0..crate::economy::MAX_LEVEL {
            p.upgrade(PELEKYS).unwrap();
        }
        assert_eq!(p.upgrade(PELEKYS), Err(Refusal::MaxLevel));
    }

    /// Hulls have makers too: only their maker's trust puts them up for sale.
    #[test]
    fn hulls_sell_on_their_makers_trust() {
        let kastron = Part::Hull(HullKind::Kastron);
        let need = kastron.price().unwrap().standing;
        let mut p = Profile::in_memory();
        p.crystals = 100_000;
        p.add_trust(Faction::Resonant, 10_000);
        assert_eq!(
            p.can_buy(kastron),
            Err(Refusal::Trust(Faction::Imperium, need))
        );
        p.add_trust(Faction::Imperium, 10_000);
        assert!(p.can_buy(kastron).is_ok());
    }
}
