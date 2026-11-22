//! Main menu, mission select, settings and the pause menu, in the hairline style.
//!
//! The main menu and mission select sit in the same left-hand panel as the
//! garage, over a mission's map with your tank parked in it: the first
//! mission's, then whichever contract the mission list points at. The pause
//! menu is a small centred box over the frozen battle.
//!
//! Names and numbers are set in display caps; everything else is sentence-case
//! body text. Colour is for signals only: who's hiring, and what's locked.

use glam::Vec2;
use winit::keyboard::KeyCode;

use crate::MapKind;
use crate::binds::{self, ACTIONS, Binds, Context, SLOTS};
use crate::economy::Part;
use crate::faction::Faction;
use crate::galaxy::theater::{CrewNews, Ship, SiteView, Sortie, Theater, ship_name, system_name};
use crate::garage::{self, Loadout, PANEL_W, PANEL_X};
use crate::mission::{Contract, MISSIONS};
use crate::profile::{Profile, Settings};
use crate::render::UiVertex;
use crate::theater_view::Transfer;
use crate::ui::emblem;
use crate::ui::widgets::{self, label};
use crate::ui::{Align, Hits, Layer, Painter, Rgba, TextStyle, measure, theme};

const REF_HEIGHT: f32 = 900.0;

/// Navigation input, mapped from keys by the app.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    Select,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Screen {
    Main,
    /// The theater your company's ship is on contract in: its system, with
    /// the contested sites to pick from.
    Missions,
    /// One contested site: what fighting there means, and deploy if your
    /// side commits to it.
    Site,
    Settings,
    /// Settings' key bindings.
    Controls,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// Deploy into the battle at a theater's site.
    Fight(Sortie),
    /// Enter on a site your side isn't fighting for this tick.
    Refused,
    Garage,
    Setting(Change),
    Quit,
}

/// A change on the settings panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    /// Flip the toggle `SETTINGS[i]`.
    Toggle(usize),
    /// Volume this many steps up or down.
    Volume(i32),
    /// Volume one step up, wrapping round to silent past the top (Enter on the row).
    CycleVolume,
    /// Put the key on this action's slot.
    Bind(binds::Action, usize, KeyCode),
    /// Clear this action's slot.
    Unbind(binds::Action, usize),
    /// Every key back to the defaults.
    ResetBinds,
}

/// What can be clicked on the menus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// Row `i` of the list on screen.
    Row(usize),
    /// The volume bar; where along it picks the level.
    Volume,
    /// On the controls screen: action row `.0`'s key slot `.1`.
    Bind(usize, usize),
}

/// A click: the key it stands in for (for the sound), and what it did.
pub type Click<A> = (Nav, Option<A>);

/// The volume change that a click at `frac` (0..1) along the bar makes.
fn volume_click(settings: &Settings, frac: f32) -> Change {
    let to = (frac.clamp(0.0, 1.0) * Settings::MAX_VOLUME as f32).round() as i32;
    Change::Volume(to - settings.volume as i32)
}

/// A row on a menu list.
#[derive(Clone, Copy)]
enum Item<'a> {
    /// Leads to another screen: a chevron when selected.
    Opens(&'a str),
    /// Acts straight away (deploys): no chevron, so whatever sits on the
    /// right of the row lines up on every row.
    Acts(&'a str),
    /// Acts straight away and takes you out (of the match, or the game): set
    /// apart under a rule, with what it does spelled out on the right, and
    /// lit hot when selected.
    Leaves(&'a str, &'a str),
}

impl<'a> Item<'a> {
    fn name(self) -> &'a str {
        match self {
            Item::Opens(n) | Item::Acts(n) | Item::Leaves(n, _) => n,
        }
    }
}

const MAIN_ITEMS: [Item<'static>; 4] = [
    Item::Opens("MISSIONS"),
    Item::Opens("GARAGE"),
    Item::Opens("SETTINGS"),
    Item::Leaves("QUIT", "CLOSES THE GAME"),
];

/// Picks one toggle out of the settings.
type Flag = fn(&mut Settings) -> &mut bool;

/// The settings panel's toggles: name, a line on what it does, and where it lives.
pub const SETTINGS: [(&str, &str, Flag); 1] =
    [("DEBUG MODE", "FPS COUNTER IN THE TOP RIGHT CORNER", |s| {
        &mut s.debug
    })];

/// Rows on the settings panel: the toggles, the volume, then the way into the controls.
const SETTING_ROWS: usize = SETTINGS.len() + 2;
const VOLUME_ROW: usize = SETTINGS.len();
const CONTROLS_ROW: usize = SETTINGS.len() + 1;

/// Rows on the controls screen: one per action, then RESET.
const CONTROL_ROWS: usize = ACTIONS.len() + 1;
const RESET_ROW: usize = ACTIONS.len();

/// `settings` after `change`.
pub fn apply(mut settings: Settings, change: Change) -> Settings {
    match change {
        Change::Toggle(i) => {
            let flag = (SETTINGS[i].2)(&mut settings);
            *flag = !*flag;
            settings
        }
        Change::Volume(d) => settings.nudge_volume(d),
        Change::CycleVolume if settings.volume >= Settings::MAX_VOLUME => Settings {
            volume: 0,
            ..settings
        },
        Change::CycleVolume => settings.nudge_volume(1),
        Change::Bind(action, slot, code) => {
            settings.binds.bind(action, slot, code);
            settings
        }
        Change::Unbind(action, slot) => {
            settings.binds.unbind(action, slot);
            settings
        }
        Change::ResetBinds => Settings {
            binds: Binds::default(),
            ..settings
        },
    }
}

/// What a key does on settings row `i` (the controls row opens its screen instead).
fn setting_input(i: usize, nav: Nav) -> Option<Change> {
    match (i == VOLUME_ROW, nav) {
        _ if i == CONTROLS_ROW => None,
        (true, Nav::Left) => Some(Change::Volume(-1)),
        (true, Nav::Right) => Some(Change::Volume(1)),
        (true, Nav::Select) => Some(Change::CycleVolume),
        (false, Nav::Select) => Some(Change::Toggle(i)),
        _ => None,
    }
}

/// The controls screen: which action and slot is selected, and whether it's
/// waiting for a key.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    row: usize,
    slot: usize,
    /// Waiting for the key to put on the selected slot.
    capturing: bool,
}

impl Controls {
    fn input(&mut self, nav: Nav) -> Option<Change> {
        match nav {
            Nav::Up | Nav::Down => self.row = step(self.row, CONTROL_ROWS, nav),
            Nav::Left => self.slot = self.slot.saturating_sub(1),
            Nav::Right => self.slot = (self.slot + 1).min(SLOTS - 1),
            Nav::Select if self.row == RESET_ROW => return Some(Change::ResetBinds),
            Nav::Select => self.capturing = true,
            Nav::Back => {}
        }
        None
    }

    /// A key pressed while waiting for one: it goes on the slot. Esc gives up,
    /// Backspace clears the slot; keys that can't be bound are ignored.
    fn key(&mut self, code: KeyCode) -> Option<Change> {
        let action = ACTIONS[self.row];
        let change = match code {
            KeyCode::Escape => None,
            KeyCode::Backspace => Some(Change::Unbind(action, self.slot)),
            c if binds::reserved(c) || binds::key_name(c).is_none() => return None,
            c => Some(Change::Bind(action, self.slot, c)),
        };
        self.capturing = false;
        change
    }

    fn hover(&mut self, hit: Hit) -> bool {
        let (row, slot) = match hit {
            Hit::Row(i) => (i, self.slot),
            Hit::Bind(i, s) => (i, s),
            Hit::Volume => return false,
        };
        if self.capturing || (row, slot) == (self.row, self.slot) {
            return false;
        }
        (self.row, self.slot) = (row, slot);
        true
    }

    /// A click on a row or slot: start rebinding it (or reset, on RESET).
    /// While waiting for a key, any click gives up.
    fn click(&mut self, hit: Hit) -> (Nav, Option<Change>) {
        if self.capturing {
            self.capturing = false;
            return (Nav::Back, None);
        }
        self.hover(hit);
        (Nav::Select, self.input(Nav::Select))
    }

    /// The keys to show at the bottom.
    fn hint(&self) -> &'static str {
        if self.capturing {
            "Press a key     Backspace  clear     Esc  cancel"
        } else if self.row == RESET_ROW {
            "Enter  reset all     Esc  back"
        } else {
            "Enter  rebind     ← →  slot     Esc  back"
        }
    }
}

pub struct Menu {
    pub screen: Screen,
    cursor: usize,
    /// The theater on the missions screen (the app keeps it up to date).
    theater: Option<Theater>,
    /// The last theater shown aboard a ship, to tell a move onto the next
    /// contract by; and that move, while it plays over the system view.
    seen: Option<Theater>,
    transfer: Option<Transfer>,
    /// Index into the theater's contested sites.
    site: usize,
    setting: usize,
    controls: Controls,
}

impl Menu {
    pub fn new() -> Menu {
        Menu {
            screen: Screen::Main,
            cursor: 0,
            theater: None,
            seen: None,
            transfer: None,
            site: 0,
            setting: 0,
            controls: Controls::default(),
        }
    }

    /// Waiting for a key to bind: the app sends it here (`capture`) rather than as `Nav`.
    pub fn capturing(&self) -> bool {
        self.screen == Screen::Controls && self.controls.capturing
    }

    /// The key pressed while `capturing`.
    pub fn capture(&mut self, code: KeyCode) -> Option<Action> {
        self.controls.key(code).map(Action::Setting)
    }

    pub fn input(&mut self, nav: Nav) -> Option<Action> {
        match (self.screen, nav) {
            (Screen::Main, Nav::Up | Nav::Down) => {
                self.cursor = step(self.cursor, MAIN_ITEMS.len(), nav)
            }
            (Screen::Main, Nav::Select) => match self.cursor {
                0 => self.screen = Screen::Missions,
                1 => return Some(Action::Garage),
                2 => self.screen = Screen::Settings,
                _ => return Some(Action::Quit),
            },
            (Screen::Main, Nav::Back | Nav::Left | Nav::Right) => {}
            (Screen::Missions | Screen::Site, Nav::Up | Nav::Down) => {
                let n = self.contested();
                if n > 0 {
                    self.site = step(self.site, n, nav);
                }
            }
            (Screen::Missions, Nav::Select) => {
                if self.contested() > 0 {
                    self.screen = Screen::Site;
                }
            }
            (Screen::Missions, Nav::Back) => self.screen = Screen::Main,
            (Screen::Site, Nav::Select) => {
                return Some(
                    self.theater
                        .as_ref()
                        .and_then(|t| t.sortie(self.site))
                        .map_or(Action::Refused, Action::Fight),
                );
            }
            (Screen::Site, Nav::Back) => self.screen = Screen::Missions,
            (Screen::Missions | Screen::Site, Nav::Left | Nav::Right) => {}
            (Screen::Settings, Nav::Up | Nav::Down) => {
                self.setting = step(self.setting, SETTING_ROWS, nav)
            }
            (Screen::Settings, Nav::Back) => self.screen = Screen::Main,
            (Screen::Settings, Nav::Select) if self.setting == CONTROLS_ROW => {
                self.controls = Controls::default();
                self.screen = Screen::Controls;
            }
            (Screen::Settings, _) => return setting_input(self.setting, nav).map(Action::Setting),
            (Screen::Controls, Nav::Back) => self.screen = Screen::Settings,
            (Screen::Controls, _) => return self.controls.input(nav).map(Action::Setting),
        }
        None
    }

    /// The screen is showing the theater: the app keeps it up to date.
    pub fn wants_theater(&self) -> bool {
        matches!(self.screen, Screen::Missions | Screen::Site)
    }

    pub fn theater(&self) -> Option<&Theater> {
        self.theater.as_ref()
    }

    /// Show `t` on the missions screen. The same site stays selected if it's
    /// still contested; else the first your side fights for.
    pub fn set_theater(&mut self, t: Option<Theater>) {
        let was = self
            .theater
            .as_ref()
            .and_then(|old| old.contested_site(self.site).map(|s| (old.system, s.index)));
        self.site = t
            .as_ref()
            .and_then(|t| {
                let same = (0..t.contested.len()).find(|&k| {
                    was == Some((
                        t.system,
                        t.contested_site(k).map_or(usize::MAX, |s| s.index),
                    ))
                });
                same.or_else(|| (0..t.contested.len()).find(|&k| t.sortie(k).is_some()))
            })
            .unwrap_or(0);
        if t.as_ref().is_none_or(|t| t.contested.is_empty()) && self.screen == Screen::Site {
            self.screen = Screen::Missions;
        }
        // Onto the next contract (another ship's, or in another system):
        // the move plays over the system view.
        if let Some(new) = t.as_ref().filter(|t| t.ship.is_some()) {
            if let Some(old) = self.seen.take().filter(|old| Transfer::between(old, new)) {
                self.transfer = Some(Transfer::new(old));
            }
            self.seen = Some(new.clone());
        }
        self.theater = t;
    }

    /// `dt` seconds on: a transfer plays on while the system view shows.
    pub fn advance(&mut self, dt: f32) {
        let Some(t) = self.system() else { return };
        let done = self.transfer.as_ref().is_some_and(|tr| tr.leg(t).is_none());
        if done {
            self.transfer = None;
        } else if let Some(tr) = &mut self.transfer {
            tr.age += dt;
        }
    }

    /// The move onto the next contract, while it plays.
    pub fn transfer(&self) -> Option<&Transfer> {
        self.transfer.as_ref()
    }

    /// Select contested site `k` (on the missions or site screen).
    pub fn pick_site(&mut self, k: usize) {
        if k < self.contested() {
            self.site = k;
        }
    }

    /// Contested sites in the theater on screen.
    fn contested(&self) -> usize {
        self.theater.as_ref().map_or(0, |t| t.contested.len())
    }

    /// The selected contested site.
    fn selected_site(&self) -> Option<&SiteView> {
        self.theater.as_ref()?.contested_site(self.site)
    }

    /// The pointer moved onto `hit`: select that row. True if the selection changed.
    pub fn hover(&mut self, hit: Hit) -> bool {
        if self.screen == Screen::Controls {
            return self.controls.hover(hit);
        }
        let i = match hit {
            Hit::Row(i) | Hit::Bind(i, _) => i,
            Hit::Volume => VOLUME_ROW,
        };
        let slot = match self.screen {
            Screen::Main => &mut self.cursor,
            Screen::Missions => &mut self.site,
            // Its one row, DEPLOY: nothing to select.
            Screen::Site => return false,
            Screen::Settings | Screen::Controls => &mut self.setting,
        };
        std::mem::replace(slot, i) != i
    }

    /// A click on `hit`, `at` (0..1) across it.
    pub fn click(&mut self, hit: Hit, at: Vec2, settings: &Settings) -> Click<Action> {
        if self.screen == Screen::Controls {
            let (nav, change) = self.controls.click(hit);
            return (nav, change.map(Action::Setting));
        }
        self.hover(hit);
        let nav = match hit {
            Hit::Volume => {
                return (
                    Nav::Right,
                    Some(Action::Setting(volume_click(settings, at.x))),
                );
            }
            Hit::Row(_) | Hit::Bind(..) => Nav::Select,
        };
        (nav, self.input(nav))
    }

    /// The theater whose system the screen shows in 3D instead of a map
    /// (the missions screen, with sites to pick from).
    pub fn system(&self) -> Option<&Theater> {
        self.theater
            .as_ref()
            .filter(|t| self.screen == Screen::Missions && !t.contested.is_empty())
    }

    /// The map the selected site is fought on, and the side you'd take in
    /// it (always the first: the theater's sides stand in for the map's):
    /// the menu's backdrop shows it (under the system view on the missions
    /// screen, loaded ready for the site's). None off the theater.
    pub fn backdrop(&self) -> Option<(MapKind, usize)> {
        matches!(self.screen, Screen::Missions | Screen::Site)
            .then(|| self.selected_site().map(|s| (s.map_kind(), 0)))
            .flatten()
    }

    /// The panel; `alpha` fades it (out while the garage is open over it).
    pub fn draw(&self, profile: &Profile, width: f32, height: f32, alpha: f32) -> Layer<Hit> {
        if alpha <= 0.0 {
            return Layer::empty();
        }
        let scale = height / REF_HEIGHT;
        let h = REF_HEIGHT;
        let w = width / scale;
        let mut p = Painter::new(scale);
        let mut hits = Hits::new();
        match self.system() {
            Some(t) => {
                crate::theater_view::veil(&mut p, t, self.transfer.as_ref(), w, h);
                garage::backdrop_fading(&mut p, h, 50.0);
            }
            None => garage::backdrop(&mut p, h),
        }
        let x = PANEL_X;
        let mut y = 72.0;

        // The game, and what you have to spend.
        p.text(
            Vec2::new(x, y),
            "TANKIT",
            &TextStyle::value(28.0),
            Align::Left,
        );
        crystals(&mut p, Vec2::new(x + PANEL_W, y + 10.0), profile.crystals);
        y += 72.0;

        let keys = match self.screen {
            Screen::Main => {
                y += rows(&mut p, &mut hits, Vec2::new(x, y), &MAIN_ITEMS, self.cursor) + 24.0;
                body(&mut p, Vec2::new(x, y), "Deploying in", theme::DIM);
                y += 20.0;
                p.text(
                    Vec2::new(x, y),
                    &kit_name(profile.loadout),
                    &TextStyle::value(12.0),
                    Align::Left,
                );
                y += 34.0;
                y += company_rank(&mut p, Vec2::new(x, y), profile) + 4.0;
                your_company(&mut p, Vec2::new(x, y), profile);
                "Enter  select"
            }
            Screen::Missions => match &self.theater {
                Some(t) if !t.contested.is_empty() => {
                    self.draw_theater(&mut p, &mut hits, Vec2::new(x, y), t);
                    crate::theater_view::draw(
                        &mut p,
                        &mut hits,
                        t,
                        self.transfer.as_ref(),
                        self.site,
                        w,
                        h,
                    );
                    "Enter  open site     ↑ ↓  site     Esc  back"
                }
                _ => {
                    self.no_posting(&mut p, Vec2::new(x, y), profile);
                    "Esc  back"
                }
            },
            Screen::Site => match (&self.theater, self.selected_site()) {
                (Some(t), Some(site)) => {
                    if self.draw_site(&mut p, &mut hits, Vec2::new(x, y), t, site, profile) {
                        "Enter  deploy     ↑ ↓  site     Esc  back"
                    } else {
                        "↑ ↓  site     Esc  back"
                    }
                }
                _ => {
                    self.no_posting(&mut p, Vec2::new(x, y), profile);
                    "Esc  back"
                }
            },
            Screen::Settings => {
                body(&mut p, Vec2::new(x, y), "Settings", theme::DIM);
                y += 26.0;
                settings_rows(
                    &mut p,
                    &mut hits,
                    Vec2::new(x, y),
                    &profile.settings,
                    self.setting,
                );
                "Enter  select     Esc  back"
            }
            Screen::Controls => {
                body(&mut p, Vec2::new(x, y), "Settings · Controls", theme::DIM);
                y += 26.0;
                controls_rows(
                    &mut p,
                    &mut hits,
                    Vec2::new(x, y),
                    &profile.settings.binds,
                    &self.controls,
                );
                self.controls.hint()
            }
        };
        body(&mut p, Vec2::new(x, h - 64.0), keys, theme::FAINT);
        Layer {
            verts: fade(p.finish(), alpha),
            hits,
        }
    }
}

impl Menu {
    /// No theater to show: your company has no battle for you yet (or
    /// you're not with one, and none has; or the galaxy is still loading).
    fn no_posting(&self, p: &mut Painter, at: Vec2, profile: &Profile) {
        let text = match profile.company_name() {
            Some(name) => {
                faction_line(p, at, Faction::Company, &format!("The {}", title(&name)));
                format!(
                    "Between contracts: the war goes on while the {} looks for work",
                    title(&name)
                )
            }
            None => {
                faction_line(p, at, Faction::Company, "The Company");
                "No contract: none of the Company's ships has a battle to join".to_string()
            }
        };
        wrap(p, at + Vec2::new(0.0, 26.0), &text, theme::DIM);
    }

    /// The theater's panel: your company, the system, your contract, then
    /// its contested sites to pick from. (How each side's doing there is in
    /// the system view beside it.)
    fn draw_theater(&self, p: &mut Painter, hits: &mut Hits<Hit>, at: Vec2, t: &Theater) {
        let (x, mut y) = (at.x, at.y);
        let employer = Faction::ALL[t.employer];
        match &t.ship {
            Some(ship) => faction_line(
                p,
                Vec2::new(x, y),
                Faction::Company,
                &format!("The {}", title(&ship.name)),
            ),
            None => faction_line(
                p,
                Vec2::new(x, y),
                employer,
                &format!("{} posting", title(employer.name())),
            ),
        };
        y += 28.0;
        p.text(
            Vec2::new(x, y),
            &t.name,
            &TextStyle::value(22.0),
            Align::Left,
        );
        y += 34.0;
        let planets = match t.planets {
            1 => "1 world".to_string(),
            n => format!("{n} worlds"),
        };
        let over = match &t.ship {
            Some(ship) => format!("Over {} {} · ", title(&t.name), numeral(ship.over)),
            None => String::new(),
        };
        body(
            p,
            Vec2::new(x, y),
            &format!("{over}{planets} · {} sites", t.sites.len()),
            theme::DIM,
        );
        y += 30.0;
        if let Some(ship) = &t.ship {
            y = contract(p, Vec2::new(x, y), ship) + 16.0;
        }

        // The contested sites.
        body(p, Vec2::new(x, y), "Contested", theme::DIM);
        y += 24.0;
        let names: Vec<String> = t
            .contested
            .iter()
            .map(|&i| {
                let s = &t.sites[i];
                let kind = s.kind_name().trim_end_matches(" GENERATOR");
                match s.repairs() {
                    Some((0, _)) => format!("{} · {kind} · WRECK", numeral(s.planet)),
                    Some((done, of)) => {
                        format!("{} · {kind} · REPAIR {done}/{of}", numeral(s.planet))
                    }
                    None => format!("{} · {kind}", numeral(s.planet)),
                }
            })
            .collect();
        let items: Vec<Item> = names.iter().map(|n| Item::Acts(n)).collect();
        rows(p, hits, Vec2::new(x, y), &items, self.site);
        for (k, &i) in t.contested.iter().enumerate() {
            let s = &t.sites[i];
            let (tag, color) = if s.committed {
                ("Fighting", theme::HOT)
            } else {
                ("", theme::DIM)
            };
            let mut style = BODY;
            style.color = color;
            let ry = y + k as f32 * ROW + 3.0;
            let right = x + PANEL_W;
            let tw = p.text(Vec2::new(right, ry), tag, &style, Align::Right);
            // Whose it is.
            let owner = Faction::ALL[s.owner];
            let size = 12.0;
            emblem::emblem(
                p,
                owner,
                Vec2::new(right - tw - 14.0 - size * 0.5, ry + 5.0),
                size,
                1.1,
                owner.ui(),
            );
        }
    }

    /// The site screen's panel: what the site is and where, who you'd fight,
    /// the contract and map it's fought with, what a win does, and the pay.
    /// True if you can deploy.
    fn draw_site(
        &self,
        p: &mut Painter,
        hits: &mut Hits<Hit>,
        at: Vec2,
        t: &Theater,
        site: &SiteView,
        profile: &Profile,
    ) -> bool {
        let (x, mut y) = (at.x, at.y);
        let employer = Faction::ALL[t.employer];
        let owner = Faction::ALL[site.owner];
        let enemy = Faction::ALL[site.enemy];
        if let Some(ship) = &t.ship {
            faction_line(
                p,
                Vec2::new(x, y),
                Faction::Company,
                &format!("The {}", title(&ship.name)),
            );
            y += 28.0;
        }
        p.text(
            Vec2::new(x, y),
            site.kind_name(),
            &TextStyle::value(22.0),
            Align::Left,
        );
        y += 34.0;
        // Where, and (a generator) whether it's up.
        let state = match (site.is_shield(), site.repairs()) {
            (true, None) => " · up",
            (true, Some((0, _))) => " · wrecked",
            (true, Some(_)) => " · under repair",
            _ => "",
        };
        body(
            p,
            Vec2::new(x, y),
            &format!("{}{state}", planet(t, site)),
            theme::DIM,
        );
        y += 20.0;
        // A generator that's down: how far its repairs have got, a step a tick.
        if let Some((done, of)) = site.repairs() {
            let lead = body(p, Vec2::new(x, y), "Repairs ", theme::DIM);
            widgets::segments(
                p,
                Vec2::new(x + lead + 4.0, y + 6.0),
                120.0,
                of as usize,
                done as usize,
                None,
            );
            body(
                p,
                Vec2::new(x + lead + 134.0, y),
                &format!("{done} of {of}"),
                theme::DIM,
            );
            y += 20.0;
        }
        // Who you fight, and for whom.
        let tag = |p: &mut Painter, at: Vec2, f: Faction| {
            let mut style = BODY;
            style.color = f.ui();
            emblem::tagged(p, at, f, &title(f.name()), &style, Align::Left)
        };
        let mut cx = x;
        if site.owner == t.employer {
            cx += body(p, Vec2::new(cx, y), "Defend from ", theme::DIM);
            tag(p, Vec2::new(cx, y), enemy);
        } else if site.owner == site.enemy {
            cx += body(p, Vec2::new(cx, y), "Attack ", theme::DIM);
            tag(p, Vec2::new(cx, y), enemy);
        } else {
            cx += body(p, Vec2::new(cx, y), "Help ", theme::DIM);
            cx += tag(p, Vec2::new(cx, y), owner);
            cx += body(p, Vec2::new(cx, y), " against ", theme::DIM);
            tag(p, Vec2::new(cx, y), enemy);
        }
        y += 32.0;

        let m = &MISSIONS[site.mission];
        body(
            p,
            Vec2::new(x, y),
            &format!("{} · {}", title(m.contract.name()), title(m.map)),
            theme::FG,
        );
        y += 22.0;
        y = wrap(
            p,
            Vec2::new(x, y),
            &stake(m.contract, site, employer),
            theme::DIM,
        );
        y += 18.0;
        let big = TextStyle::value(22.0);
        let col = x + 150.0;
        p.text(Vec2::new(x, y), &m.rating_label(), &big, Align::Left);
        let pw = p.text(
            Vec2::new(col, y),
            &m.win_pay().to_string(),
            &big,
            Align::Left,
        );
        body(p, Vec2::new(col + pw + 6.0, y + 11.0), "CR", theme::DIM);
        y += 32.0;
        body(p, Vec2::new(x, y), "Rating", theme::DIM);
        body(p, Vec2::new(col, y), "Win, plus kills", theme::DIM);
        y += 34.0;
        prize(
            p,
            Vec2::new(x, y),
            site.prize(|part| profile.owns(part)),
            profile,
        );
        y += 58.0;
        if site.committed {
            rows(p, hits, Vec2::new(x, y), &[Item::Acts("DEPLOY")], 0);
            true
        } else {
            body(
                p,
                Vec2::new(x, y),
                &format!(
                    "The {} aren't fighting here this tick",
                    title(employer.name())
                ),
                theme::HOT,
            );
            false
        }
    }
}

/// `text` after `faction`'s emblem, in its colour: who's hiring, or whose ship.
fn faction_line(p: &mut Painter, at: Vec2, faction: Faction, text: &str) {
    let mut style = BODY;
    style.color = faction.ui();
    emblem::tagged(p, at, faction, text, &style, Align::Left);
}

/// Width of the labels down the left of the panel's label-value rows.
const LABEL_W: f32 = 72.0;

/// A row of the panel at `at`: `label`, dim, then `value` after it.
fn field(p: &mut Painter, at: Vec2, label: &str, value: &str) {
    body(p, at, label, theme::DIM);
    body(p, at + Vec2::new(LABEL_W, 0.0), value, theme::FG);
}

/// Your ship's contract at `at`: who hired it and its hulls; and, if the
/// Company just moved you, from where. (Its time left and how many hulls it
/// can lose are over the system view. The company's pay and crystal aren't
/// shown: they're its business, not the pilot's.) Returns the y under it.
fn contract(p: &mut Painter, at: Vec2, ship: &Ship) -> f32 {
    let (x, mut y) = (at.x, at.y);
    let employer = Faction::ALL[ship.side];
    body(p, Vec2::new(x, y), "Hired by", theme::DIM);
    let mut style = BODY;
    style.color = employer.ui();
    emblem::tagged(
        p,
        Vec2::new(x + LABEL_W, y),
        employer,
        &format!("{} of {}", title(employer.name()), title(&ship.realm)),
        &style,
        Align::Left,
    );
    y += 22.0;
    let n = |v: f32| v.round().max(0.0) as u32;
    field(
        p,
        Vec2::new(x, y),
        "Hulls",
        &format!("{} fighting · {} spare", n(ship.field), n(ship.aboard)),
    );
    y += 22.0;
    let news = match ship.news {
        Some(CrewNews::Joined { from }) => Some(format!(
            "The {} merged into this ship",
            title(&ship_name(from))
        )),
        Some(CrewNews::Sailed { from }) => Some(format!(
            "Sailed here from {} for work",
            title(&system_name(from as usize))
        )),
        Some(CrewNews::Gone { from }) => Some(format!(
            "Moved here: the {} is gone",
            title(&ship_name(from))
        )),
        None => None,
    };
    if let Some(news) = news {
        y = wrap(p, Vec2::new(x, y), &news, theme::HOT);
    }
    y
}

/// "Lirset II": the site's world, in title case.
fn planet(t: &Theater, site: &SiteView) -> String {
    format!("{} {}", title(&t.name), numeral(site.planet))
}

/// "II": planet `p`'s numeral.
fn numeral(p: u8) -> &'static str {
    ["I", "II", "III", "IV"][(p as usize).min(3)]
}

/// What a win at `site` does in the war, by the contract it's fought with.
fn stake(contract: Contract, site: &SiteView, employer: Faction) -> String {
    let ours = site.owner == employer.index();
    let shield_up = site.is_shield() && site.down().is_none();
    let what = match contract {
        Contract::Attrition => "they lose hulls; no ground changes hands",
        Contract::Hold if ours => "it stays ours",
        Contract::Hold if shield_up => "it goes down",
        Contract::Hold if site.is_shield() => "it's ours, and we put up our own",
        Contract::Hold => "it's ours",
        Contract::Raid if shield_up && !ours => "it goes down",
        Contract::Raid => "one of their generators goes down",
    };
    let how = match contract {
        Contract::Attrition => "Win",
        Contract::Hold => "Hold it longest",
        Contract::Raid => "Bring their flag home",
    };
    format!("{how}: {what}")
}

/// Your company at `at`, under the rank: its ship's name, or that you're not
/// with one yet. Returns its height.
fn your_company(p: &mut Painter, at: Vec2, profile: &Profile) -> f32 {
    body(p, at, "Your company", theme::DIM);
    let at = at + Vec2::new(0.0, 22.0);
    match profile.company_name() {
        Some(name) => {
            let mut style = TextStyle::value(14.0);
            style.color = Faction::Company.ui();
            emblem::tagged(
                p,
                at,
                Faction::Company,
                &format!("THE {name}"),
                &style,
                Align::Left,
            );
        }
        None => {
            body(p, at, "None yet: open Missions to join one", theme::FAINT);
        }
    }
    44.0
}

/// Your Company rank at `at`: the Company's emblem and name, the rank's number
/// and title, and the bar to the next. Returns its height.
fn company_rank(p: &mut Painter, at: Vec2, profile: &Profile) -> f32 {
    let (x, mut y) = (at.x, at.y);
    let r = profile.rank();
    let mut style = BODY;
    style.color = Faction::Company.ui();
    emblem::tagged(
        p,
        Vec2::new(x, y),
        Faction::Company,
        "The Company",
        &style,
        Align::Left,
    );
    y += 24.0;
    p.text(
        Vec2::new(x, y),
        &format!("Rank {}", r.level),
        &TextStyle::value(22.0),
        Align::Left,
    );
    p.text(
        Vec2::new(x + PANEL_W, y + 11.0),
        &title(r.title()),
        &BODY,
        Align::Right,
    );
    y += 34.0;
    widgets::hairbar(p, Vec2::new(x, y), PANEL_W, r.progress(), theme::FG);
    y += 10.0;
    let mut faint = BODY;
    faint.color = theme::FAINT;
    p.text(
        Vec2::new(x + PANEL_W, y),
        &format!("{} / {} renown to Rank {}", r.into, r.span, r.level + 1),
        &faint,
        Align::Right,
    );
    y += 26.0;
    y - at.y
}

/// A contract's prize at `at`: the schematic a win steals, its maker's emblem
/// beside it; or that you already have it, or that there's none.
fn prize(p: &mut Painter, at: Vec2, part: Option<Part>, profile: &Profile) {
    let (x, y) = (at.x, at.y);
    match part {
        Some(part) if !profile.owns(part) => {
            let mut style = TextStyle::value(16.0);
            style.color = theme::HOT;
            emblem::tagged(
                p,
                Vec2::new(x, y),
                part.maker(),
                part.name(),
                &style,
                Align::Left,
            );
            let kind = match part {
                Part::Hull(_) => "hull",
                Part::Turret(_) => "turret",
            };
            body(
                p,
                Vec2::new(x, y + 28.0),
                &format!("Prize: their {kind} schematic, if you win"),
                theme::DIM,
            );
        }
        Some(part) => {
            body(
                p,
                Vec2::new(x, y),
                &format!("{} schematic: yours already", title(part.name())),
                theme::DIM,
            );
        }
        None => {
            body(p, Vec2::new(x, y), "No schematic to win", theme::DIM);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PauseAction {
    Resume,
    /// Open the garage over the frozen match: what you pick there is what you
    /// respawn in next.
    Garage,
    Setting(Change),
    ExitToMenu,
    Quit,
}

const PAUSE_ITEMS: [Item<'static>; 5] = [
    Item::Opens("RESUME"),
    Item::Opens("GARAGE"),
    Item::Opens("SETTINGS"),
    Item::Leaves("EXIT TO MENU", "ENDS THE MATCH"),
    Item::Leaves("QUIT", "CLOSES THE GAME"),
];

pub struct Pause {
    cursor: usize,
    /// The settings list is open in the box, on this row.
    settings: Option<usize>,
    /// The controls screen is open in the box (over the settings).
    controls: Option<Controls>,
}

impl Pause {
    pub fn new() -> Pause {
        Pause {
            cursor: 0,
            settings: None,
            controls: None,
        }
    }

    /// Waiting for a key to bind: the app sends it here (`capture`) rather than as `Nav`.
    pub fn capturing(&self) -> bool {
        self.controls.is_some_and(|c| c.capturing)
    }

    /// The key pressed while `capturing`.
    pub fn capture(&mut self, code: KeyCode) -> Option<PauseAction> {
        let c = self.controls.as_mut()?;
        c.key(code).map(PauseAction::Setting)
    }

    pub fn input(&mut self, nav: Nav) -> Option<PauseAction> {
        if let Some(c) = &mut self.controls {
            if nav == Nav::Back {
                self.controls = None;
                return None;
            }
            return c.input(nav).map(PauseAction::Setting);
        }
        if let Some(i) = &mut self.settings {
            match nav {
                Nav::Up | Nav::Down => *i = step(*i, SETTING_ROWS, nav),
                Nav::Back => self.settings = None,
                Nav::Select if *i == CONTROLS_ROW => self.controls = Some(Controls::default()),
                _ => return setting_input(*i, nav).map(PauseAction::Setting),
            }
            return None;
        }
        match nav {
            Nav::Up | Nav::Down => self.cursor = step(self.cursor, PAUSE_ITEMS.len(), nav),
            Nav::Back => return Some(PauseAction::Resume),
            Nav::Left | Nav::Right => {}
            Nav::Select => match self.cursor {
                0 => return Some(PauseAction::Resume),
                1 => return Some(PauseAction::Garage),
                2 => self.settings = Some(0),
                3 => return Some(PauseAction::ExitToMenu),
                _ => return Some(PauseAction::Quit),
            },
        }
        None
    }

    /// The pointer moved onto `hit`: select that row. True if the selection changed.
    pub fn hover(&mut self, hit: Hit) -> bool {
        if let Some(c) = &mut self.controls {
            return c.hover(hit);
        }
        let i = match hit {
            Hit::Row(i) | Hit::Bind(i, _) => i,
            Hit::Volume => VOLUME_ROW,
        };
        let slot = self.settings.as_mut().unwrap_or(&mut self.cursor);
        std::mem::replace(slot, i) != i
    }

    /// A click on `hit`, `at` (0..1) across it.
    pub fn click(&mut self, hit: Hit, at: Vec2, settings: &Settings) -> Click<PauseAction> {
        if let Some(c) = &mut self.controls {
            let (nav, change) = c.click(hit);
            return (nav, change.map(PauseAction::Setting));
        }
        self.hover(hit);
        let nav = match hit {
            Hit::Volume => {
                return (
                    Nav::Right,
                    Some(PauseAction::Setting(volume_click(settings, at.x))),
                );
            }
            Hit::Row(_) | Hit::Bind(..) => Nav::Select,
        };
        (nav, self.input(nav))
    }

    /// Dims the battle and draws the box. `match_renown` is what this match has
    /// earned you with the Company so far.
    pub fn draw(
        &self,
        profile: &Profile,
        match_renown: u32,
        width: f32,
        height: f32,
    ) -> Layer<Hit> {
        let scale = height / REF_HEIGHT;
        let (w, h) = (width / scale, REF_HEIGHT);
        let mut p = Painter::new(scale);
        let mut hits = Hits::new();
        p.rect(Vec2::ZERO, Vec2::new(w, h), [0.0, 0.0, 0.0, 0.45]);
        // The controls list is long: the box grows to hold it and its keys line.
        let tall = 90.0 + CONTROLS_H + 44.0;
        let size = Vec2::new(
            PANEL_W + 48.0,
            if self.controls.is_some() { tall } else { 380.0 },
        );
        let min = Vec2::new(w, h) * 0.5 - size * 0.5;
        p.rect(min, size, theme::PANEL);
        widgets::corners(&mut p, min, min + size, 14.0, theme::DIM);

        let x = min.x + 24.0;
        let mut y = min.y + 28.0;
        label(&mut p, Vec2::new(x, y), "LINK HELD", Align::Left);
        // Whose company you're fighting with.
        if let Some(name) = profile.company_name() {
            let mut style = TextStyle::LABEL;
            style.color = Faction::Company.ui();
            emblem::tagged(
                &mut p,
                Vec2::new(x + PANEL_W, y),
                Faction::Company,
                &format!("THE {name}"),
                &style,
                Align::Right,
            );
        }
        y += 18.0;
        let title = match (self.controls, self.settings) {
            (Some(_), _) => "CONTROLS",
            (None, Some(_)) => "SETTINGS",
            (None, None) => "PAUSED",
        };
        p.text(Vec2::new(x, y), title, &TextStyle::value(20.0), Align::Left);
        y += 44.0;
        if let Some(c) = &self.controls {
            controls_rows(
                &mut p,
                &mut hits,
                Vec2::new(x, y),
                &profile.settings.binds,
                c,
            );
            body(
                &mut p,
                Vec2::new(x, min.y + size.y - 32.0),
                c.hint(),
                theme::FAINT,
            );
            return Layer {
                verts: p.finish(),
                hits,
            };
        }
        if let Some(i) = self.settings {
            settings_rows(&mut p, &mut hits, Vec2::new(x, y), &profile.settings, i);
            return Layer {
                verts: p.finish(),
                hits,
            };
        }
        y += rows(
            &mut p,
            &mut hits,
            Vec2::new(x, y),
            &PAUSE_ITEMS,
            self.cursor,
        ) + 8.0;
        p.line(
            Vec2::new(x, y),
            Vec2::new(x + PANEL_W, y),
            1.0,
            theme::FAINT,
        );
        y += 16.0;
        label(&mut p, Vec2::new(x, y + 1.0), "THIS MATCH", Align::Left);
        p.text(
            Vec2::new(x + PANEL_W, y),
            &format!("+{match_renown} RENOWN"),
            &TextStyle::value(11.0),
            Align::Right,
        );
        y += 20.0;
        let r = profile.rank();
        // The bar starts past the name, however long it runs.
        let mut style = TextStyle::LABEL;
        style.color = Faction::Company.ui();
        let bar_x = 132.0_f32.max(
            emblem::tagged(
                &mut p,
                Vec2::new(x, y + 1.0),
                Faction::Company,
                &r.label(),
                &style,
                Align::Left,
            ) + 12.0,
        );
        widgets::hairbar(
            &mut p,
            Vec2::new(x + bar_x, y + 5.0),
            PANEL_W - bar_x,
            r.progress(),
            theme::FG,
        );
        y += 22.0;
        label(
            &mut p,
            Vec2::new(x, y),
            "LEAVING FORFEITS THE CONTRACT PAY",
            Align::Left,
        );
        Layer {
            verts: p.finish(),
            hits,
        }
    }
}

/// The settings toggles: each a row with a checkbox, and under it what it does.
fn settings_rows(
    p: &mut Painter,
    hits: &mut Hits<Hit>,
    at: Vec2,
    settings: &Settings,
    selected: usize,
) {
    for (i, (name, what, flag)) in SETTINGS.iter().enumerate() {
        let y = at.y + i as f32 * 44.0;
        garage::row_hit(p, hits, Vec2::new(at.x, y), 44.0, Hit::Row(i));
        let on = i == selected;
        if on {
            p.line(
                Vec2::new(at.x - 14.0, y - 3.0),
                Vec2::new(at.x - 14.0, y + 18.0),
                2.0,
                theme::FG,
            );
        }
        let mut s = TextStyle::value(15.0);
        s.color = if on { theme::FG } else { theme::DIM };
        p.text(Vec2::new(at.x, y), name, &s, Align::Left);
        let mut copy = *settings;
        widgets::checkbox(
            p,
            Vec2::new(at.x + PANEL_W - 14.0, y + 1.0),
            14.0,
            *flag(&mut copy),
        );
        label(p, Vec2::new(at.x, y + 22.0), what, Align::Left);
    }
    let y = at.y + VOLUME_ROW as f32 * 44.0;
    garage::row_hit(p, hits, Vec2::new(at.x, y), 44.0, Hit::Row(VOLUME_ROW));
    // The bar itself, exactly as wide as drawn so the click lands on the level under it.
    hits.add(
        p,
        Vec2::new(at.x, y + 18.0),
        Vec2::new(PANEL_W, 16.0),
        Hit::Volume,
    );
    let on = selected == VOLUME_ROW;
    if on {
        p.line(
            Vec2::new(at.x - 14.0, y - 3.0),
            Vec2::new(at.x - 14.0, y + 18.0),
            2.0,
            theme::FG,
        );
    }
    let mut s = TextStyle::value(15.0);
    s.color = if on { theme::FG } else { theme::DIM };
    p.text(Vec2::new(at.x, y), "SOUND VOLUME", &s, Align::Left);
    let pct = (settings.volume() * 100.0).round() as u32;
    let value = if pct == 0 {
        "OFF".to_string()
    } else {
        format!("{pct}%")
    };
    p.text(
        Vec2::new(at.x + PANEL_W, y + 2.0),
        &value,
        &TextStyle::value(11.0),
        Align::Right,
    );
    widgets::hairbar(
        p,
        Vec2::new(at.x, y + 26.0),
        PANEL_W,
        settings.volume(),
        theme::FG,
    );

    // The way into the key bindings.
    let y = at.y + CONTROLS_ROW as f32 * 44.0;
    garage::row_hit(p, hits, Vec2::new(at.x, y), 44.0, Hit::Row(CONTROLS_ROW));
    let on = selected == CONTROLS_ROW;
    if on {
        selection_bar(p, Vec2::new(at.x, y), theme::FG);
        chevron(p, Vec2::new(at.x + PANEL_W, y));
    }
    let mut s = TextStyle::value(15.0);
    s.color = if on { theme::FG } else { theme::DIM };
    p.text(Vec2::new(at.x, y), "CONTROLS", &s, Align::Left);
    let changed = settings.binds != Binds::default();
    let what = if changed {
        "KEY BINDINGS · CHANGED"
    } else {
        "KEY BINDINGS · DEFAULT"
    };
    label(p, Vec2::new(at.x, y + 22.0), what, Align::Left);
}

/// Pitch of a row on the controls screen.
const CONTROL_ROW: f32 = 24.0;
/// Height of a section label (BATTLE, GARAGE) on the controls screen.
const CONTROL_LABEL: f32 = 22.0;
/// Width of a key slot.
const SLOT_W: f32 = 76.0;

/// Height the controls screen draws.
const CONTROLS_H: f32 = CONTROL_LABEL * 2.0
    + ACTIONS.len() as f32 * CONTROL_ROW
    + 10.0
    + LEAVE_GAP
    + 10.0
    + CONTROL_ROW;

/// The marker beside a selected row.
fn selection_bar(p: &mut Painter, at: Vec2, color: Rgba) {
    p.line(
        Vec2::new(at.x - 14.0, at.y - 3.0),
        Vec2::new(at.x - 14.0, at.y + 18.0),
        2.0,
        color,
    );
}

/// The chevron on a selected row that opens a screen; `right` is the row's right edge.
fn chevron(p: &mut Painter, right: Vec2) {
    let (cx, y) = (right.x, right.y);
    p.polyline(
        &[
            Vec2::new(cx - 8.0, y + 2.0),
            Vec2::new(cx - 3.0, y + 7.5),
            Vec2::new(cx - 8.0, y + 13.0),
        ],
        1.2,
        theme::FG,
    );
}

/// The controls screen: each action with its two key slots, battle controls
/// then the garage's, then RESET under a rule.
fn controls_rows(p: &mut Painter, hits: &mut Hits<Hit>, at: Vec2, binds: &Binds, c: &Controls) {
    let mut y = at.y;
    let mut section = None;
    for (i, &action) in ACTIONS.iter().enumerate() {
        let garage_only = action.context() == Context::Garage;
        if section != Some(garage_only) {
            if section.is_some() {
                y += 10.0;
            }
            section = Some(garage_only);
            let name = if garage_only {
                "IN THE GARAGE"
            } else {
                "IN BATTLE"
            };
            label(p, Vec2::new(at.x, y), name, Align::Left);
            y += CONTROL_LABEL;
        }
        garage::row_hit(p, hits, Vec2::new(at.x, y + 3.0), CONTROL_ROW, Hit::Row(i));
        let on = i == c.row;
        if on {
            selection_bar(
                p,
                Vec2::new(at.x, y),
                if c.capturing { theme::HOT } else { theme::FG },
            );
        }
        let mut s = TextStyle::value(12.0);
        s.color = if on { theme::FG } else { theme::DIM };
        p.text(Vec2::new(at.x, y + 2.0), action.name(), &s, Align::Left);
        for (slot, key) in binds.keys(action).into_iter().enumerate() {
            let min = Vec2::new(
                at.x + PANEL_W - (SLOTS - slot) as f32 * SLOT_W + 6.0,
                y - 1.0,
            );
            let size = Vec2::new(SLOT_W - 6.0, 18.0);
            hits.add(p, min, size, Hit::Bind(i, slot));
            let picked = on && slot == c.slot;
            let waiting = picked && c.capturing;
            let edge = match (picked, waiting) {
                (_, true) => theme::HOT,
                (true, false) => theme::FG,
                (false, false) => theme::FAINT,
            };
            p.rect_outline(min, size, 1.0, edge);
            let (text, color) = match key.and_then(binds::key_name) {
                _ if waiting => ("PRESS", theme::HOT),
                Some(name) => (name, if on { theme::FG } else { theme::DIM }),
                None => ("—", theme::FAINT),
            };
            let mut s = TextStyle::LABEL;
            s.color = color;
            p.text(
                Vec2::new(min.x + size.x * 0.5, min.y + (size.y - s.size) * 0.5),
                text,
                &s,
                Align::Center,
            );
        }
        y += CONTROL_ROW;
    }

    // Back to the defaults, set apart like the menus' leaving rows.
    let ry = y + LEAVE_GAP * 0.5;
    p.line(
        Vec2::new(at.x, ry),
        Vec2::new(at.x + PANEL_W, ry),
        1.0,
        theme::FAINT,
    );
    y += LEAVE_GAP + 10.0;
    garage::row_hit(
        p,
        hits,
        Vec2::new(at.x, y + 3.0),
        CONTROL_ROW,
        Hit::Row(RESET_ROW),
    );
    let on = c.row == RESET_ROW;
    if on {
        selection_bar(p, Vec2::new(at.x, y), theme::HOT);
    }
    let mut s = TextStyle::value(12.0);
    s.color = if on { theme::HOT } else { theme::DIM };
    p.text(Vec2::new(at.x, y + 2.0), "RESET CONTROLS", &s, Align::Left);
    let mut s = TextStyle::LABEL;
    if on {
        s.color = theme::HOT;
    }
    let what = if *binds == Binds::default() {
        "ALL DEFAULT"
    } else {
        "TO DEFAULT KEYS"
    };
    p.text(Vec2::new(at.x + PANEL_W, y + 5.0), what, &s, Align::Right);
}

/// Body text: sentence case, for everything that isn't a name or a number.
const BODY: TextStyle = TextStyle {
    size: 10.0,
    tracking: 0.3,
    weight: 1.1,
    color: theme::DIM,
    lower: true,
};

/// Line height of wrapped body text.
const BODY_LINE: f32 = 17.0;

/// Height of a list row.
const ROW: f32 = 30.0;

fn body(p: &mut Painter, at: Vec2, text: &str, color: Rgba) -> f32 {
    let mut style = BODY;
    style.color = color;
    p.text(at, text, &style, Align::Left)
}

/// Your crystals, right-aligned at `at`.
fn crystals(p: &mut Painter, at: Vec2, crystals: u32) {
    let w = body(
        p,
        at + Vec2::new(-measure("CR", &BODY), 2.0),
        "CR",
        theme::DIM,
    );
    p.text(
        at - Vec2::new(w + 6.0, 0.0),
        &crystals.to_string(),
        &TextStyle::value(13.0),
        Align::Right,
    );
}

/// "THE CAPITAL" as "The Capital".
fn title(caps: &str) -> String {
    caps.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_string() + &c.as_str().to_lowercase())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Gap above the rule that sets the leaving rows apart.
const LEAVE_GAP: f32 = 14.0;

/// A selectable list, with the garage's bar-and-chevron marker on the selected
/// row. Rows that leave sit under a rule, say what they do, and have no
/// chevron. Returns the height drawn.
fn rows(p: &mut Painter, hits: &mut Hits<Hit>, at: Vec2, items: &[Item], selected: usize) -> f32 {
    let mut y = at.y;
    let mut ruled = false;
    for (i, &item) in items.iter().enumerate() {
        let leaves = matches!(item, Item::Leaves(..));
        if leaves && !ruled && i > 0 {
            ruled = true;
            let ry = y + LEAVE_GAP * 0.5 - 8.0;
            p.line(
                Vec2::new(at.x, ry),
                Vec2::new(at.x + PANEL_W, ry),
                1.0,
                theme::FAINT,
            );
            y += LEAVE_GAP;
        }
        garage::row_hit(p, hits, Vec2::new(at.x, y), ROW, Hit::Row(i));
        let on = i == selected;
        let lit = match (on, leaves) {
            (true, true) => theme::HOT,
            (true, false) => theme::FG,
            (false, _) => theme::DIM,
        };
        if on {
            p.line(
                Vec2::new(at.x - 14.0, y - 3.0),
                Vec2::new(at.x - 14.0, y + 18.0),
                2.0,
                lit,
            );
        }
        match item {
            Item::Opens(_) if on => {
                let cx = at.x + PANEL_W;
                p.polyline(
                    &[
                        Vec2::new(cx - 8.0, y + 2.0),
                        Vec2::new(cx - 3.0, y + 7.5),
                        Vec2::new(cx - 8.0, y + 13.0),
                    ],
                    1.2,
                    theme::FG,
                );
            }
            Item::Opens(_) | Item::Acts(_) => {}
            Item::Leaves(_, what) => {
                let mut s = TextStyle::LABEL;
                if on {
                    s.color = theme::HOT;
                }
                p.text(Vec2::new(at.x + PANEL_W, y + 4.0), what, &s, Align::Right);
            }
        }
        let mut s = TextStyle::value(15.0);
        s.color = lit;
        p.text(Vec2::new(at.x, y), item.name(), &s, Align::Left);
        y += ROW;
    }
    y - at.y
}

/// Body text wrapped to the panel's width. Returns where the line after it goes.
fn wrap(p: &mut Painter, at: Vec2, text: &str, color: Rgba) -> f32 {
    let mut line = String::new();
    let mut y = at.y;
    for word in text.split(' ') {
        let longer = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && measure(&longer, &BODY) > PANEL_W {
            body(p, Vec2::new(at.x, y), &line, color);
            y += BODY_LINE;
            line = word.to_string();
        } else {
            line = longer;
        }
    }
    body(p, Vec2::new(at.x, y), &line, color);
    y + BODY_LINE
}

pub fn kit_name(kit: Loadout) -> String {
    format!(
        "{} · {}",
        garage::hull_name(kit.0),
        garage::turret_name(kit.1)
    )
}

fn step(i: usize, n: usize, nav: Nav) -> usize {
    if nav == Nav::Down {
        (i + 1) % n
    } else {
        (i + n - 1) % n
    }
}

/// The battle HUD knocked back under the pause menu.
pub fn dim(v: Vec<UiVertex>) -> Vec<UiVertex> {
    fade(v, 0.3)
}

fn fade(mut v: Vec<UiVertex>, alpha: f32) -> Vec<UiVertex> {
    for vert in &mut v {
        vert.color[3] *= alpha.clamp(0.0, 1.0);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_menu_navigation() {
        let mut m = Menu::new();
        assert_eq!(m.input(Nav::Down), None);
        assert_eq!(m.input(Nav::Select), Some(Action::Garage));
        assert_eq!(m.input(Nav::Down), None);
        assert_eq!(m.input(Nav::Select), None);
        assert_eq!(m.screen, Screen::Settings);
        assert_eq!(
            m.input(Nav::Select),
            Some(Action::Setting(Change::Toggle(0)))
        );
        m.input(Nav::Down);
        assert_eq!(
            m.input(Nav::Left),
            Some(Action::Setting(Change::Volume(-1)))
        );
        assert_eq!(
            m.input(Nav::Select),
            Some(Action::Setting(Change::CycleVolume))
        );
        m.input(Nav::Back);
        assert_eq!(m.input(Nav::Down), None);
        assert_eq!(m.input(Nav::Select), Some(Action::Quit));
        // Wraps round to MISSIONS, which opens your ship's theater: with
        // none to show yet, Enter goes nowhere.
        m.input(Nav::Down);
        assert_eq!(m.input(Nav::Select), None);
        assert_eq!(m.screen, Screen::Missions);
        assert!(m.wants_theater());
        assert_eq!(m.input(Nav::Select), None);
        assert_eq!(m.screen, Screen::Missions);
        // With one, Enter opens the site your side fights for, and Enter there deploys.
        m.set_theater(theater(Faction::Imperium));
        let t = m.theater().unwrap().clone();
        assert!(
            t.sortie(m.site).is_some(),
            "starts on a site you can fight for"
        );
        assert_eq!(m.input(Nav::Select), None);
        assert_eq!(m.screen, Screen::Site);
        assert_eq!(
            m.input(Nav::Select),
            Some(Action::Fight(t.sortie(m.site).unwrap()))
        );
        // A site your side isn't fighting for: refused.
        if let Some(k) = (0..t.contested.len()).find(|&k| t.sortie(k).is_none()) {
            m.pick_site(k);
            assert_eq!(m.input(Nav::Select), Some(Action::Refused));
        }
        m.input(Nav::Back);
        assert_eq!(m.screen, Screen::Missions);
        // Back to the main menu.
        m.input(Nav::Back);
        assert_eq!(m.screen, Screen::Main);
        assert!(!m.wants_theater());
    }

    /// A small war, some ticks in: the theater aboard a ship working for
    /// `f` (or any, if none is), as a new pilot sees it.
    fn theater(f: Faction) -> Option<Theater> {
        use crate::galaxy::{Galaxy, REGION, Rules, SYSTEMS};
        let mut g = Galaxy::with_size(3, SYSTEMS / 4, REGION * 0.5, Rules::default());
        for _ in 0..60 {
            g.step();
        }
        let (t, _) = g.await_work(Some(f.index()), 1, 400);
        assert!(t.as_ref().is_some_and(|t| t.ship.is_some()));
        t
    }

    #[test]
    fn pause_navigation() {
        let mut p = Pause::new();
        assert_eq!(p.input(Nav::Back), Some(PauseAction::Resume));
        assert_eq!(p.input(Nav::Select), Some(PauseAction::Resume));
        p.input(Nav::Down);
        assert_eq!(p.input(Nav::Select), Some(PauseAction::Garage));
        p.input(Nav::Down);
        assert_eq!(p.input(Nav::Select), None);
        assert_eq!(
            p.input(Nav::Select),
            Some(PauseAction::Setting(Change::Toggle(0)))
        );
        assert_eq!(p.input(Nav::Left), None);
        // Up wraps round to CONTROLS, which Left and Right leave alone; volume is above it.
        p.input(Nav::Up);
        assert_eq!(p.input(Nav::Right), None);
        p.input(Nav::Up);
        assert_eq!(
            p.input(Nav::Right),
            Some(PauseAction::Setting(Change::Volume(1)))
        );
        p.input(Nav::Down);
        // Esc closes settings, then resumes.
        assert_eq!(p.input(Nav::Back), None);
        p.input(Nav::Down);
        assert_eq!(p.input(Nav::Select), Some(PauseAction::ExitToMenu));
        p.input(Nav::Down);
        assert_eq!(p.input(Nav::Select), Some(PauseAction::Quit));
    }

    #[test]
    fn draws_on_screen_at_any_size() {
        let mut profile = Profile::in_memory();
        let mut m = Menu::new();
        let mut settings = Pause::new();
        settings.input(Nav::Down);
        settings.input(Nav::Down);
        settings.input(Nav::Select);
        let mut controls = Pause::new();
        controls.settings = Some(CONTROLS_ROW);
        controls.input(Nav::Select);
        controls.input(Nav::Select);
        assert!(controls.capturing());
        let t = theater(Faction::Imperium);
        for (screen, t) in [
            (Screen::Main, None),
            (Screen::Missions, None),
            (Screen::Missions, t.clone()),
            (Screen::Site, t),
            (Screen::Settings, None),
            (Screen::Controls, None),
        ] {
            m.set_theater(t);
            m.screen = screen;
            // With a company and without.
            for (w, h, ship) in [
                (1600.0, 900.0, None),
                (1600.0, 900.0, Some(7)),
                (1024.0, 768.0, Some(7)),
                (3840.0, 2160.0, Some(7)),
            ] {
                profile.ship = ship;
                for v in [
                    m.draw(&profile, w, h, 1.0).verts,
                    Pause::new().draw(&profile, 30, w, h).verts,
                    settings.draw(&profile, 30, w, h).verts,
                    controls.draw(&profile, 30, w, h).verts,
                ] {
                    assert!(!v.is_empty());
                    assert!(v.iter().all(|v| v.pos[0] > -4.0
                        && v.pos[0] < w + 4.0
                        && v.pos[1] > -4.0
                        && v.pos[1] < h + 4.0));
                }
            }
        }
        assert!(m.draw(&profile, 1600.0, 900.0, 0.0).verts.is_empty());
    }

    /// Clicks the middle of `hit` where `draw` put it.
    fn click_drawn(m: &mut Menu, profile: &Profile, hit: Hit) -> Click<Action> {
        let hits = m.draw(profile, 1600.0, 900.0, 1.0).hits;
        let (id, at) = hits.at(hits.center_of(hit).unwrap()).unwrap();
        assert_eq!(id, hit);
        m.click(id, at, &profile.settings)
    }

    #[test]
    fn clicking_the_menu() {
        let profile = Profile::in_memory();
        let mut m = Menu::new();
        let hits = m.draw(&profile, 1600.0, 900.0, 1.0).hits;
        // Off the panel there's nothing to click.
        assert_eq!(hits.at(Vec2::new(1200.0, 450.0)), None);
        assert!(m.hover(Hit::Row(3)));
        assert!(!m.hover(Hit::Row(3)));
        assert_eq!(
            click_drawn(&mut m, &profile, Hit::Row(1)),
            (Nav::Select, Some(Action::Garage))
        );
        assert_eq!(
            click_drawn(&mut m, &profile, Hit::Row(0)),
            (Nav::Select, None)
        );
        assert_eq!(m.screen, Screen::Missions);
        m.set_theater(theater(Faction::Ithessar));
        let t = m.theater().unwrap().clone();
        let k = (0..t.contested.len())
            .find(|&k| t.sortie(k).is_some())
            .unwrap();
        // A site in the picture (drawn after the list, so it's the one
        // clicked here) is the same row as in the list: clicking it opens it.
        let hits = m.draw(&profile, 1600.0, 900.0, 1.0).hits;
        assert!(hits.center_of(Hit::Row(k)).unwrap().x > PANEL_X + PANEL_W);
        assert_eq!(
            click_drawn(&mut m, &profile, Hit::Row(k)),
            (Nav::Select, None)
        );
        assert_eq!(m.screen, Screen::Site);
        // Its DEPLOY row deploys.
        assert_eq!(
            click_drawn(&mut m, &profile, Hit::Row(0)),
            (Nav::Select, Some(Action::Fight(t.sortie(k).unwrap())))
        );
        // Esc, or a right click, goes back.
        m.input(Nav::Back);
        m.input(Nav::Back);
        assert_eq!(m.screen, Screen::Main);

        m.screen = Screen::Settings;
        assert_eq!(
            click_drawn(&mut m, &profile, Hit::Row(0)).1,
            Some(Action::Setting(Change::Toggle(0)))
        );
        // The volume bar sets the level under the click: its middle is half volume.
        let half = Settings::MAX_VOLUME as i32 / 2 - profile.settings.volume as i32;
        assert_eq!(
            click_drawn(&mut m, &profile, Hit::Volume).1,
            Some(Action::Setting(Change::Volume(half)))
        );
        assert_eq!(m.setting, VOLUME_ROW);
    }

    #[test]
    fn clicking_the_pause_box() {
        let profile = Profile::in_memory();
        let mut p = Pause::new();
        let click = |p: &mut Pause, hit: Hit| {
            let hits = p.draw(&profile, 0, 1600.0, 900.0).hits;
            let (id, at) = hits.at(hits.center_of(hit).unwrap()).unwrap();
            p.click(id, at, &profile.settings)
        };
        assert_eq!(
            click(&mut p, Hit::Row(3)),
            (Nav::Select, Some(PauseAction::ExitToMenu))
        );
        assert_eq!(
            click(&mut p, Hit::Row(1)),
            (Nav::Select, Some(PauseAction::Garage))
        );
        assert_eq!(click(&mut p, Hit::Row(2)), (Nav::Select, None));
        assert_eq!(
            click(&mut p, Hit::Row(0)).1,
            Some(PauseAction::Setting(Change::Toggle(0)))
        );
        // Back leaves settings.
        p.input(Nav::Back);
        assert_eq!(p.settings, None);
    }

    /// Settings → CONTROLS → pick a slot → press a key: it's bound there.
    #[test]
    fn rebinding_from_the_menu() {
        let mut m = Menu::new();
        m.screen = Screen::Settings;
        m.setting = CONTROLS_ROW;
        assert_eq!(m.input(Nav::Select), None);
        assert_eq!(m.screen, Screen::Controls);
        assert!(!m.capturing());
        // FIRE, second slot.
        for _ in 0..binds::Action::Fire.index() {
            m.input(Nav::Down);
        }
        m.input(Nav::Right);
        assert_eq!(m.input(Nav::Select), None);
        assert!(m.capturing());
        // Keys that can't be bound don't end the wait.
        assert_eq!(m.capture(KeyCode::Enter), None);
        assert!(m.capturing());
        assert_eq!(
            m.capture(KeyCode::KeyF),
            Some(Action::Setting(Change::Bind(
                binds::Action::Fire,
                1,
                KeyCode::KeyF
            )))
        );
        assert!(!m.capturing());
        // Esc gives up; Backspace clears.
        m.input(Nav::Select);
        assert_eq!(m.capture(KeyCode::Escape), None);
        assert!(!m.capturing());
        m.input(Nav::Select);
        assert_eq!(
            m.capture(KeyCode::Backspace),
            Some(Action::Setting(Change::Unbind(binds::Action::Fire, 1)))
        );
        // RESET, at the bottom (Up wraps there from the top).
        for _ in 0..=binds::Action::Fire.index() {
            m.input(Nav::Up);
        }
        assert_eq!(
            m.input(Nav::Select),
            Some(Action::Setting(Change::ResetBinds))
        );
        m.input(Nav::Back);
        assert_eq!(m.screen, Screen::Settings);
    }

    #[test]
    fn rebinding_from_the_pause_box() {
        let mut p = Pause::new();
        p.input(Nav::Down);
        p.input(Nav::Down);
        p.input(Nav::Select);
        p.input(Nav::Up);
        assert_eq!(p.settings, Some(CONTROLS_ROW));
        p.input(Nav::Select);
        assert!(p.controls.is_some());
        p.input(Nav::Select);
        assert_eq!(
            p.capture(KeyCode::KeyI),
            Some(PauseAction::Setting(Change::Bind(
                binds::Action::Forward,
                0,
                KeyCode::KeyI
            )))
        );
        // Esc steps back out: controls, settings, then resumes.
        assert_eq!(p.input(Nav::Back), None);
        assert_eq!(p.controls, None);
        assert_eq!(p.input(Nav::Back), None);
        assert_eq!(p.input(Nav::Back), Some(PauseAction::Resume));
    }

    #[test]
    fn clicking_a_key_slot_waits_for_a_key() {
        let profile = Profile::in_memory();
        let mut m = Menu::new();
        m.screen = Screen::Controls;
        let hit = Hit::Bind(binds::Action::Garage.index(), 1);
        assert_eq!(click_drawn(&mut m, &profile, hit), (Nav::Select, None));
        assert!(m.capturing());
        assert_eq!(
            m.capture(KeyCode::KeyH),
            Some(Action::Setting(Change::Bind(
                binds::Action::Garage,
                1,
                KeyCode::KeyH
            )))
        );
        // Any click while waiting gives up.
        click_drawn(&mut m, &profile, Hit::Row(0));
        assert!(m.capturing());
        assert_eq!(
            click_drawn(&mut m, &profile, Hit::Row(0)),
            (Nav::Back, None)
        );
        assert!(!m.capturing());
        assert_eq!(
            click_drawn(&mut m, &profile, Hit::Row(RESET_ROW)),
            (Nav::Select, Some(Action::Setting(Change::ResetBinds)))
        );
    }

    #[test]
    fn binds_apply_and_reset() {
        let s = apply(
            Settings::default(),
            Change::Bind(binds::Action::Fire, 0, KeyCode::KeyF),
        );
        assert!(s.binds.is(binds::Action::Fire, KeyCode::KeyF));
        assert!(!s.binds.is(binds::Action::Fire, KeyCode::Space));
        let s = apply(s, Change::Unbind(binds::Action::Forward, 0));
        assert_eq!(s.binds.keys(binds::Action::Forward)[0], None);
        assert_eq!(apply(s, Change::ResetBinds).binds, Binds::default());
    }

    #[test]
    fn debug_toggle() {
        let s = apply(Settings::default(), Change::Toggle(0));
        assert!(s.debug);
        assert!(!apply(s, Change::Toggle(0)).debug);
    }

    #[test]
    fn volume_steps_and_wraps() {
        let mut s = Settings::default();
        for _ in 0..20 {
            s = apply(s, Change::Volume(1));
        }
        assert_eq!(s.volume, Settings::MAX_VOLUME);
        assert_eq!(apply(s, Change::CycleVolume).volume, 0);
        assert_eq!(
            apply(s, Change::Volume(-3)).volume,
            Settings::MAX_VOLUME - 3
        );
        let silent = Settings { volume: 0, ..s };
        assert_eq!(apply(silent, Change::Volume(-1)).volume, 0);
        assert_eq!(apply(silent, Change::CycleVolume).volume, 1);
    }

    /// Moving onto the next contract plays a transfer over the system view
    /// (#348): to another system it jumps, else it moves; a new tick on the
    /// same contract plays nothing. It ends on its own.
    #[test]
    fn next_contract_plays_a_transfer() {
        let mut m = Menu::new();
        m.input(Nav::Select);
        let t = theater(Faction::Imperium).unwrap();
        m.set_theater(Some(t.clone()));
        assert!(m.transfer().is_none());

        // The same contract, a tick on: nothing.
        let mut later = t.clone();
        later.tick += 1;
        m.set_theater(Some(later.clone()));
        assert!(m.transfer().is_none());

        // A new contract in the same system: a move.
        let mut next = later.clone();
        next.ship.as_mut().unwrap().signed += 5;
        m.set_theater(Some(next.clone()));
        let tr = m.transfer().expect("a transfer");
        assert!(!tr.jumps(&next));
        assert!(matches!(
            tr.leg(&next),
            Some(crate::theater_view::Leg::Move(_))
        ));
        for _ in 0..1000 {
            m.advance(0.01);
        }
        assert!(m.transfer().is_none(), "it ends");

        // Through a spell between contracts, to another system: a jump.
        m.set_theater(None);
        let mut far = next.clone();
        far.system += 1;
        m.set_theater(Some(far.clone()));
        let tr = m.transfer().expect("a transfer");
        assert!(tr.jumps(&far));
        let legs: Vec<_> = (0..600)
            .filter_map(|k| {
                let mut tr = tr.clone();
                tr.age = k as f32 * 0.01;
                tr.leg(&far)
            })
            .map(|l| std::mem::discriminant(&l))
            .collect::<Vec<_>>();
        let mut order = legs.clone();
        order.dedup();
        use crate::theater_view::Leg;
        assert_eq!(
            order,
            [Leg::Leave(0.0), Leg::Jump(0.0), Leg::Arrive(0.0)].map(|l| std::mem::discriminant(&l))
        );
    }
}
