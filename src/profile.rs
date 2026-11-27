//! The pilot profile: your Company rank, crystals, the parts you've
//! reverse-engineered and the loadout you deploy in. One profile, saved as JSON in the OS data
//! directory (`~/.local/share/tankit/profile.json` on Linux) whenever it
//! changes, so there's no save button.
//!
//! You're the Company's, whoever hires you. Renown earned in matches climbs one
//! Company rank, which decides the contracts it takes. The Company starts you
//! on the Imperium's basic kit; every other part you reverse-engineer from
//! the wrecks of the enemies you kill (#438): its schematic is yours once you
//! know its first step. Crystals, paid for finished contracts, buy upgrades
//! up to what you know.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::JoinHandle;

use crate::galaxy::theater::Shown;
use serde::{Deserialize, Serialize};

use crate::binds::Binds;
use crate::economy::{Known, Levels, PARTS, Part, upgrade_cost};
use crate::garage::Loadout;
use crate::sim::{HullKind, TurretKind};

/// Renown from one rank to the next: 100, then 20 more for each after, so the
/// ranks come quickly at first and slow into a long climb.
fn rank_span(level: u32) -> u32 {
    100 + 20 * (level - 1)
}

/// The Company's titles, lowest first, each held for [`RANKS_PER_TITLE`] ranks;
/// past the last, the last title holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Title {
    Recruit,
    Hireling,
    Freelance,
    Gunhand,
    Veteran,
    Sergeant,
    Lieutenant,
    Captain,
    Marshal,
    Warlord,
}

impl Title {
    pub const ALL: [Title; 10] = [
        Title::Recruit,
        Title::Hireling,
        Title::Freelance,
        Title::Gunhand,
        Title::Veteran,
        Title::Sergeant,
        Title::Lieutenant,
        Title::Captain,
        Title::Marshal,
        Title::Warlord,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Title::Recruit => "RECRUIT",
            Title::Hireling => "HIRELING",
            Title::Freelance => "FREELANCE",
            Title::Gunhand => "GUNHAND",
            Title::Veteran => "VETERAN",
            Title::Sergeant => "SERGEANT",
            Title::Lieutenant => "LIEUTENANT",
            Title::Captain => "CAPTAIN",
            Title::Marshal => "MARSHAL",
            Title::Warlord => "WARLORD",
        }
    }
}

pub const RANKS_PER_TITLE: u32 = 5;

/// Where your renown sits on the Company's ladder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rank {
    /// Starts at 1.
    pub level: u32,
    /// Renown earned since reaching this rank.
    pub into: u32,
    /// Renown this rank takes to get through.
    pub span: u32,
}

impl Rank {
    pub fn of(renown: u32) -> Rank {
        let (mut level, mut left) = (1, renown);
        while left >= rank_span(level) {
            left -= rank_span(level);
            level += 1;
        }
        Rank {
            level,
            into: left,
            span: rank_span(level),
        }
    }

    pub fn progress(&self) -> f32 {
        self.into as f32 / self.span as f32
    }

    /// The title that goes with the number.
    pub fn title(&self) -> Title {
        let i = ((self.level - 1) / RANKS_PER_TITLE) as usize;
        Title::ALL[i.min(Title::ALL.len() - 1)]
    }
}

/// What's on disk. Parts are stored by name so the file stays readable;
/// fields added later default, so older profiles still load.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Saved {
    /// Renown with the Company, all told.
    renown: u32,
    crystals: u32,
    hull: String,
    turret: String,
    /// Stolen schematics (starters are always owned).
    owned: Vec<String>,
    /// Upgrade level in steps (see [`crate::economy::STEPS_PER_MARK`]) by part name;
    /// missing means M0.
    steps: BTreeMap<String, u8>,
    /// Profiles from before upgrades came in steps had whole marks (M1 = 1). Read once,
    /// never written.
    #[serde(skip_serializing)]
    levels: BTreeMap<String, u8>,
    /// How far each part is reverse-engineered (#438), in steps, by part
    /// name; missing means only as far as it's built.
    known: BTreeMap<String, f32>,
    settings: Settings,
    /// The company ship you're aboard, by its id in the galaxy.
    ship: Option<u32>,
    /// The contract the missions screen last showed you (#386).
    shown: Option<Shown>,
}

/// Player options from the settings panel, saved with the profile.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Debug overlay: an FPS counter in the corner.
    pub debug: bool,
    /// Sound volume in steps, 0 (silent) to [`Settings::MAX_VOLUME`].
    pub volume: u8,
    /// Which keys do what.
    pub binds: Binds,
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
            binds: Binds::default(),
        }
    }
}

/// Why an upgrade didn't go through.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Refusal {
    /// The part's schematic isn't yours yet: not reverse-engineered to its first step.
    NotOwned,
    Crystals(u32),
    MaxLevel,
    /// The next step isn't known yet: it has to be reverse-engineered from
    /// wrecks of better gear (#438).
    Unknown,
}

pub struct Profile {
    /// Renown with the Company, all told: your rank.
    pub renown: u32,
    pub crystals: u32,
    /// The kit you deploy in (and respawn in, until the garage changes it).
    pub loadout: Loadout,
    /// Stolen schematics, starters not included.
    pub levels: Levels,
    /// How far you've reverse-engineered each part (#438): you can build a
    /// part up to what you know of it.
    pub known: Known,
    pub settings: Settings,
    /// Your company (`Company::id`), once you're with one: kept from the
    /// galaxy's `crew` for the UI (#350).
    pub ship: Option<u32>,
    /// The contract the missions screen last showed you, so a move onto the
    /// next one plays even after a battle or a relaunch (#386).
    pub shown: Option<Shown>,
    /// Where to save; `None` keeps the profile in memory only (headless runs, tests).
    disk: Option<Disk>,
}

/// The profile's file, written on a worker thread: a save lands mid-match on
/// every kill, and a write that waits on a busy disk mustn't stall the frame.
struct Disk {
    path: PathBuf,
    /// The latest profile text, for the worker to write.
    tx: Option<mpsc::Sender<String>>,
    worker: Option<JoinHandle<()>>,
}

impl Disk {
    fn new(path: PathBuf) -> Disk {
        let (tx, rx) = mpsc::channel::<String>();
        let to = path.clone();
        let worker = std::thread::Builder::new()
            .name("profile save".into())
            .spawn(move || {
                while let Ok(mut text) = rx.recv() {
                    // Saves queued up behind a slow write: only the newest matters.
                    if let Some(latest) = rx.try_iter().last() {
                        text = latest;
                    }
                    if let Err(e) = write_file(&to, &text) {
                        log::warn!("couldn't save profile to {}: {e:#}", to.display());
                    }
                }
            })
            .ok();
        Disk {
            path,
            tx: Some(tx),
            worker,
        }
    }

    fn save(&self, text: String) {
        let sent = self
            .tx
            .as_ref()
            .is_some_and(|tx| tx.send(text.clone()).is_ok());
        if !sent || self.worker.is_none() {
            // No worker thread: write here rather than lose the save.
            if let Err(e) = write_file(&self.path, &text) {
                log::warn!("couldn't save profile to {}: {e:#}", self.path.display());
            }
        }
    }
}

impl Drop for Disk {
    /// Quitting waits for the last save to reach the disk.
    fn drop(&mut self) {
        self.tx = None;
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

/// Write then rename, so a crash mid-save can't leave a half-written profile.
pub(crate) fn write_file(path: &Path, text: &str) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

impl Profile {
    /// A fresh pilot that is never written to disk.
    pub fn in_memory() -> Profile {
        Profile {
            renown: 0,
            crystals: 0,
            loadout: (HullKind::Kataphrakt, TurretKind::Kontos),
            levels: Levels::default(),
            known: Known::default(),
            settings: Settings::default(),
            ship: None,
            shown: None,
            disk: None,
        }
    }

    pub fn default_path() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("tankit").join("profile.json"))
    }

    /// Load from `path`, or start a fresh pilot there if it's missing or unreadable.
    pub fn load(path: PathBuf) -> Profile {
        let mut p = Profile {
            disk: Some(Disk::new(path.clone())),
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
        self.renown = s.renown;
        self.crystals = s.crystals;
        self.settings = s.settings;
        self.ship = s.ship;
        self.shown = s.shown;
        // Schematics owned (stolen, before #438): known to their first step.
        for part in s.owned.iter().filter_map(|n| Part::by_name(n)) {
            self.known.at_least(part, 1);
        }
        for (name, &mark) in &s.levels {
            if let Some(part) = Part::by_name(name) {
                self.levels
                    .set(part, mark.saturating_mul(crate::economy::STEPS_PER_MARK));
            }
        }
        for (name, &level) in &s.steps {
            if let Some(part) = Part::by_name(name) {
                self.levels.set(part, level);
            }
        }
        for (name, &known) in &s.known {
            if let Some(part) = Part::by_name(name) {
                self.known.set(part, known);
            }
        }
        // What you've built, you know (profiles from before #438).
        for part in crate::economy::PARTS {
            self.known.at_least(part, self.levels.of(part));
        }
        // Only deploy in parts you own (older profiles predate owning).
        if let Some(Part::Hull(h)) = Part::by_name(&s.hull).filter(|&p| self.owns(p)) {
            self.loadout.0 = h;
        }
        if let Some(Part::Turret(t)) = Part::by_name(&s.turret).filter(|&p| self.owns(p)) {
            self.loadout.1 = t;
        }
    }

    pub fn rank(&self) -> Rank {
        Rank::of(self.renown)
    }

    /// Own `part` without reverse-engineering it (headless shots, tests).
    pub fn grant(&mut self, part: Part) {
        self.known.at_least(part, 1);
    }

    /// Its schematic is yours: a starter, or reverse-engineered to its
    /// first step (#438).
    pub fn owns(&self, part: Part) -> bool {
        part.starter() || self.known.cap(part) >= 1
    }

    /// Can `part` be upgraded right now? The cost if so.
    pub fn can_upgrade(&self, part: Part) -> Result<u32, Refusal> {
        if !self.owns(part) {
            return Err(Refusal::NotOwned);
        }
        let level = self.levels.of(part);
        let cost = upgrade_cost(level).ok_or(Refusal::MaxLevel)?;
        if level >= self.known.cap(part) {
            return Err(Refusal::Unknown);
        }
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

    /// Take apart the wrecks of what you killed (#438): each a part and the
    /// level it was built to. Saves. Returns the schematics that made yours.
    pub fn scrap(&mut self, wrecks: &[(Part, u8)]) -> Vec<Part> {
        if wrecks.is_empty() {
            return Vec::new();
        }
        let had: Vec<bool> = PARTS.iter().map(|&p| self.owns(p)).collect();
        for &(part, level) in wrecks {
            self.known.scrap(part, level);
        }
        self.save();
        PARTS
            .iter()
            .zip(had)
            .filter(|&(&p, had)| !had && self.owns(p))
            .map(|(&p, _)| p)
            .collect()
    }

    /// Bank renown earned for the Company and save.
    pub fn add_renown(&mut self, renown: u32) {
        if renown > 0 {
            self.renown += renown;
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

    /// Your company's name ("IRON TITHE"), once you're with one.
    pub fn company_name(&self) -> Option<String> {
        self.ship.map(crate::galaxy::theater::ship_name)
    }

    /// Go aboard company ship `id` (see `ship`) and save.
    pub fn board(&mut self, id: Option<u32>) {
        if id != self.ship {
            self.ship = id;
            self.save();
        }
    }

    /// You've been shown contract `shown` (see `shown`): save it if it's new.
    pub fn set_shown(&mut self, shown: Option<Shown>) {
        if shown.is_some() && shown != self.shown {
            self.shown = shown;
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

    /// Queue the profile to be written; the file is written off this thread.
    fn save(&self) {
        let Some(disk) = &self.disk else { return };
        match self.text() {
            Ok(text) => disk.save(text),
            Err(e) => log::warn!("couldn't save profile to {}: {e:#}", disk.path.display()),
        }
    }

    /// The profile as it's saved.
    fn text(&self) -> anyhow::Result<String> {
        let name = |p: Part| p.name().to_ascii_lowercase();
        let saved = Saved {
            renown: self.renown,
            crystals: self.crystals,
            hull: name(Part::Hull(self.loadout.0)),
            turret: name(Part::Turret(self.loadout.1)),
            owned: PARTS
                .iter()
                .filter(|&&p| !p.starter() && self.owns(p))
                .map(|&p| name(p))
                .collect(),
            steps: PARTS
                .iter()
                .filter(|&&p| self.levels.of(p) > 0)
                .map(|&p| (name(p), self.levels.of(p)))
                .collect(),
            levels: BTreeMap::new(),
            known: PARTS
                .iter()
                .filter(|&&p| self.known.of(p) > 0.0)
                .map(|&p| (name(p), self.known.of(p)))
                .collect(),
            settings: self.settings,
            ship: self.ship,
            shown: self.shown.clone(),
        };
        Ok(serde_json::to_string_pretty(&saved)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_ladder() {
        assert_eq!(
            Rank::of(0),
            Rank {
                level: 1,
                into: 0,
                span: 100
            }
        );
        assert_eq!(Rank::of(99).level, 1);
        assert_eq!(
            Rank::of(100),
            Rank {
                level: 2,
                into: 0,
                span: 120
            }
        );
        assert_eq!(
            Rank::of(230),
            Rank {
                level: 3,
                into: 10,
                span: 140
            }
        );
    }

    /// Lots of ranks, each a number with a title: dozens of them before the titles
    /// run out, the climb getting longer as it goes.
    #[test]
    fn ranks_are_many_and_titled() {
        assert_eq!(Rank::of(0).title(), Rank::of(399).title());
        let top = Title::ALL.len() as u32 * RANKS_PER_TITLE;
        assert!(top >= 50);
        // Renown to reach the last title's first rank.
        let to = |level: u32| (1..level).map(rank_span).sum::<u32>();
        let last = to(top - RANKS_PER_TITLE + 1);
        assert_eq!(Rank::of(last).title(), Title::Warlord);
        assert_eq!(Rank::of(last - 1).title(), Title::Marshal);
        // At ~60 renown a match, the top title is hundreds of matches off.
        assert!(last / 60 > 300, "{}", last / 60);
        assert_eq!(Rank::of(to(top + 20)).title(), Title::Warlord);
        assert!(rank_span(40) > rank_span(2) * 3);
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("tankit-profile-test-{}", std::process::id()));
        let path = dir.join("profile.json");
        let _ = std::fs::remove_dir_all(&dir);

        let mut p = Profile::load(path.clone());
        assert_eq!(p.renown, 0);
        p.add_renown(300);
        p.add_renown(40);
        p.add_crystals(2000);
        p.known.set(PELEKYS, 1.5);
        p.upgrade(PELEKYS).unwrap();
        p.set_loadout((HullKind::Kataphrakt, TurretKind::Pelekys));
        let mut binds = Binds::default();
        binds.bind(
            crate::binds::Action::Fire,
            0,
            winit::keyboard::KeyCode::KeyF,
        );
        p.set_settings(Settings {
            debug: true,
            volume: 3,
            binds,
        });
        let shown = Shown {
            ship: 7,
            signed: 201,
            system: 3,
            side: 1,
            over: 2,
            realm: "KALARUN".into(),
            defend: true,
        };
        p.set_shown(Some(shown.clone()));
        let crystals = p.crystals;
        // Dropping the profile waits for its last save to land.
        drop(p);

        let back = Profile::load(path.clone());
        assert_eq!(back.renown, 340);
        assert_eq!(back.crystals, crystals);
        assert!(back.owns(PELEKYS));
        assert_eq!(back.levels.of(PELEKYS), 1);
        assert_eq!(
            back.known.of(PELEKYS),
            1.5,
            "what you've reverse-engineered"
        );
        assert_eq!(back.loadout, (HullKind::Kataphrakt, TurretKind::Pelekys));
        assert!(back.settings.debug);
        assert_eq!(back.settings.volume, 3);
        assert_eq!(back.settings.binds, binds);
        assert_eq!(back.shown, Some(shown));

        // An older profile (no crystals, deploying in a part it never owned)
        // still loads.
        std::fs::write(&path, r#"{"hull": "bastion", "turret": "lance"}"#).unwrap();
        let old = Profile::load(path.clone());
        assert_eq!(old.renown, 0);
        assert_eq!(old.crystals, 0);
        assert_eq!(old.shown, None);
        assert_eq!(old.loadout, (HullKind::Kataphrakt, TurretKind::Kontos));
        assert!(!old.settings.debug);
        // Profiles from before sound get the default volume, not silence.
        assert_eq!(old.settings.volume, Settings::default().volume);
        // ...and the default keys.
        assert_eq!(old.settings.binds, Binds::default());

        // Profiles from when upgrades were whole marks keep the mark they reached.
        std::fs::write(
            &path,
            r#"{"owned": ["pelekys"], "levels": {"pelekys": 2, "kontos": 3}}"#,
        )
        .unwrap();
        let marks = Profile::load(path.clone());
        assert_eq!(marks.levels.of(PELEKYS), 20);
        assert_eq!(
            marks.levels.of(Part::Turret(TurretKind::Kontos)),
            crate::economy::MAX_LEVEL
        );

        // A corrupt file starts a fresh pilot rather than failing.
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Profile::load(path).renown, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    const PELEKYS: Part = Part::Turret(TurretKind::Pelekys);

    /// The starters are yours from the start; anything else only once it's
    /// reverse-engineered to its first step, and then crystals upgrade it up
    /// to what's known (#438).
    #[test]
    fn parts_are_reverse_engineered_then_upgraded() {
        let mut p = Profile::in_memory();
        assert!(p.owns(Part::Hull(HullKind::Kataphrakt)));
        assert!(p.owns(Part::Turret(TurretKind::Kontos)));
        assert!(!p.owns(PELEKYS));
        p.crystals = 100_000;
        assert_eq!(p.upgrade(PELEKYS), Err(Refusal::NotOwned));
        // Wrecks of it, from T0.1 gear: the one that makes a whole step.
        let mut wrecks = 0;
        let got = loop {
            assert!(!p.owns(PELEKYS), "not yet");
            wrecks += 1;
            let got = p.scrap(&[(PELEKYS, 1)]);
            if !got.is_empty() {
                break got;
            }
        };
        assert_eq!(got, vec![PELEKYS], "now yours");
        assert_eq!(wrecks, 14, "each 0.08 of a step, and less as you learn");
        assert!(p.owns(PELEKYS));
        // Only as far as it's known.
        p.upgrade(PELEKYS).unwrap();
        assert_eq!(p.upgrade(PELEKYS), Err(Refusal::Unknown));
        p.known.set(PELEKYS, 2.7);
        p.upgrade(PELEKYS).unwrap();
        assert_eq!(p.upgrade(PELEKYS), Err(Refusal::Unknown));
        p.known.set(PELEKYS, crate::economy::MAX_LEVEL as f32);
        for _ in 2..crate::economy::MAX_LEVEL {
            p.upgrade(PELEKYS).unwrap();
        }
        assert_eq!(p.upgrade(PELEKYS), Err(Refusal::MaxLevel));
        p.known.set(Part::Turret(TurretKind::Kontos), 1.0);
        p.crystals = 0;
        assert_eq!(
            p.upgrade(Part::Turret(TurretKind::Kontos)),
            Err(Refusal::Crystals(upgrade_cost(0).unwrap()))
        );
    }
}
