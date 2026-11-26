//! Main menu, mission select, settings and the pause menu, in the hairline style.
//!
//! The main menu is the design system's (`ui::design`), over the
//! galaxy around you. Mission select still sits in the garage's left-hand panel, over
//! the map of whichever contract it points at. The pause
//! menu is a small centred box over the frozen battle.
//!
//! Names and numbers are set in display caps; everything else is sentence-case
//! body text. Colour is for signals only: who's hiring, and what's locked.

use glam::Vec2;
use winit::keyboard::KeyCode;

use crate::binds::{self, ACTIONS, Binds, Context, SLOTS};
use crate::cluster_view::{self, Cluster};
use crate::galaxy::theater::{Ended, Offer, SiteView, Sortie, Theater, planet_name};
use crate::mission::MISSIONS;
use crate::profile::{Profile, Settings};
use crate::render::UiVertex;
use crate::theater_view::Transfer;
use crate::ui::design::UNIT;
use crate::ui::{Align, Hits, Layer, Painter, design, theme, with_alpha};
use crate::work_view::{self, Board};

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
    /// its fights to pick from and deploy into.
    Missions,
    Settings,
    /// Settings' key bindings.
    Controls,
    /// The other companies' ships with a battle for you, to switch to (#365).
    Ships,
    /// One of those ships' contract, previewed before you sign on (#453).
    Contract,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// Deploy into the battle at a theater's site.
    Fight(Sortie),
    /// Enter on a site your side isn't fighting for this tick.
    Refused,
    /// Leave your company for this ship (a `Company::id`), one of those
    /// the companies screen offers (#365).
    Switch(u32),
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
    /// The BACK button: what Esc does.
    Back,
    /// The CONTINUE button on the contract summary card (#414).
    Continue,
    /// On the board of work: filter `i`, and sort `i`.
    Filter(usize),
    Sort(usize),
    /// On the main menu: star `i` of the cluster map, to read.
    Star(usize),
    /// DEPLOY on the contract page.
    Deploy,
    /// On the settings page: setting `i` (its keys are `Row`s and `Bind`s).
    Setting(usize),
}

/// A click: the key it stands in for (for the sound), and what it did.
pub type Click<A> = (Nav, Option<A>);

/// The volume change that a click at `frac` (0..1) along the bar makes.
fn volume_click(settings: &Settings, frac: f32) -> Change {
    let to = (frac.clamp(0.0, 1.0) * Settings::MAX_VOLUME as f32).round() as i32;
    Change::Volume(to - settings.volume as i32)
}

/// The main menu's actions, what a merc's here for first; then, set apart
/// at the foot of the panel, the rest.
const MAIN_ITEMS: [&str; 5] = ["Contract", "Find work", "Garage", "Settings", "Quit"];
const MAIN_ACTIONS: usize = 3;

/// Picks one toggle out of the settings.
type Flag = fn(&mut Settings) -> &mut bool;

/// The settings panel's toggles: name, a line on what it does, and where it lives.
pub const SETTINGS: [(&str, &str, Flag); 1] = [("DEBUG MODE", "FPS COUNTER", |s| &mut s.debug)];

/// Rows on the settings panel: the toggles, the volume, then the way into the controls.
const SETTING_ROWS: usize = SETTINGS.len() + 2;
const VOLUME_ROW: usize = SETTINGS.len();
const CONTROLS_ROW: usize = SETTINGS.len() + 1;
/// Those on the menu's settings page, which has the controls on it.
const PAGE_SETTINGS: usize = CONTROLS_ROW;

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
            Hit::Volume
            | Hit::Back
            | Hit::Continue
            | Hit::Filter(_)
            | Hit::Sort(_)
            | Hit::Star(_)
            | Hit::Deploy
            | Hit::Setting(_) => return false,
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
}

/// The contract you're signed on for: where, and whether you're still on the
/// way there.
#[derive(Clone, Debug, PartialEq)]
pub struct JobAt {
    pub system: String,
    pub flying: bool,
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
    /// The contract that just ended (#414, #501), over the theater it was
    /// in: its card is up until you go on, before any move or wait.
    card: Option<(Theater, Ended)>,
    /// The ended contract whose card you've gone on from (its ship and
    /// tick signed), so it doesn't come up again.
    closed: Option<(u32, u64)>,
    /// The contract you're signed on for, once the galaxy's in (the app keeps
    /// it up to date): None inside if you've none.
    job: Option<Option<JobAt>>,
    /// Seconds the menu's been up (turns the waiting spinner).
    clock: f32,
    /// You just switched company (#449): the next move onto another ship
    /// is your hull warping across, not the ship flying.
    switched: bool,
    /// Index into the theater's contested sites.
    site: usize,
    /// The contracts on offer (the app keeps them up to date).
    pub board: Board,
    /// The ship whose offer's deal is on screen (a `Company::id`).
    previewing: u32,
    setting: usize,
    controls: Controls,
    /// The pointer is on the BACK button.
    back_lit: bool,
    /// The mouse is over the page's way on: CONTINUE, DEPLOY or SIGN ON.
    go_lit: bool,
    /// Your stretch of the war, behind the main menu (the app keeps it up
    /// to date), and the star the pointer's on.
    cluster: Option<Cluster>,
    star: Option<usize>,
}

impl Menu {
    pub fn new() -> Menu {
        Menu {
            screen: Screen::Main,
            cursor: 0,
            theater: None,
            seen: None,
            transfer: None,
            card: None,
            closed: None,
            job: None,
            clock: 0.0,
            switched: false,
            site: 0,
            board: Board::default(),
            previewing: 0,
            setting: 0,
            controls: Controls::default(),
            back_lit: false,
            go_lit: false,
            cluster: None,
            star: None,
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
                self.cursor = step(self.cursor, MAIN_ITEMS.len(), nav);
                if !self.opens(self.cursor) {
                    self.cursor = step(self.cursor, MAIN_ITEMS.len(), nav);
                }
            }
            (Screen::Main, Nav::Select) => match self.cursor {
                0 => self.screen = Screen::Missions,
                1 => self.screen = Screen::Ships,
                2 => return Some(Action::Garage),
                3 => {
                    self.setting = 0;
                    self.screen = Screen::Settings;
                }
                _ => return Some(Action::Quit),
            },
            (Screen::Main, Nav::Back | Nav::Left | Nav::Right) => {}
            (Screen::Missions, Nav::Select) if self.summary().is_some() => self.go_on(),
            (Screen::Missions, Nav::Up | Nav::Down) if self.summary().is_some() => {}
            (Screen::Missions, Nav::Up | Nav::Down) => {
                // Only the sites your side fights for: there's no entering the rest.
                let n = self.contested();
                for _ in 0..n {
                    self.site = step(self.site, n, nav);
                    if self.selected_site().is_some_and(|s| s.committed) {
                        break;
                    }
                }
            }
            (Screen::Missions, Nav::Select) if self.transfer.is_some() => self.transfer = None,
            (Screen::Missions, Nav::Select) => {
                let t = self.theater.as_ref().filter(|t| !t.contested.is_empty())?;
                return Some(t.sortie(self.site).map_or(Action::Refused, Action::Fight));
            }
            (Screen::Missions, Nav::Back) => self.screen = Screen::Main,
            (Screen::Missions, Nav::Left | Nav::Right) => {}
            // One page, one list: the settings, then the keys, round and round.
            (Screen::Settings, Nav::Up) if self.setting == 0 => {
                self.controls = Controls::default();
                self.controls.row = RESET_ROW;
                self.screen = Screen::Controls;
            }
            (Screen::Settings, Nav::Down) if self.setting + 1 >= PAGE_SETTINGS => {
                self.controls = Controls::default();
                self.screen = Screen::Controls;
            }
            (Screen::Settings, Nav::Up | Nav::Down) => {
                self.setting = step(self.setting, PAGE_SETTINGS, nav)
            }
            (Screen::Controls, Nav::Up) if self.controls.row == 0 => {
                self.setting = PAGE_SETTINGS - 1;
                self.screen = Screen::Settings;
            }
            (Screen::Controls, Nav::Down) if self.controls.row == RESET_ROW => {
                self.setting = 0;
                self.screen = Screen::Settings;
            }
            (Screen::Settings, Nav::Back) => self.screen = Screen::Main,
            (Screen::Settings, _) => return setting_input(self.setting, nav).map(Action::Setting),
            (Screen::Controls, Nav::Back) => self.screen = Screen::Main,
            (Screen::Controls, _) => return self.controls.input(nav).map(Action::Setting),
            (Screen::Ships, Nav::Back) => self.screen = Screen::Main,
            (Screen::Ships, _) => {
                if let Some(id) = self.board.input(nav) {
                    self.previewing = id;
                    self.screen = Screen::Contract;
                }
            }
            (Screen::Contract, Nav::Select) => {
                return self
                    .board
                    .offer(self.previewing)
                    .map(|o| Action::Switch(o.id));
            }
            (Screen::Contract, Nav::Back) => self.screen = Screen::Ships,
            (Screen::Contract, _) => {}
        }
        None
    }

    /// The screen is showing the theater: the app keeps it up to date.
    pub fn wants_theater(&self) -> bool {
        self.screen == Screen::Missions
    }

    /// The screen is showing the ships to switch to: the app keeps them up to date.
    pub fn wants_ships(&self) -> bool {
        matches!(self.screen, Screen::Ships | Screen::Contract)
    }

    /// Move the board's pick by the wheel. True if it moved.
    pub fn scroll(&mut self, steps: i32) -> bool {
        self.screen == Screen::Ships && self.board.scroll(steps)
    }

    /// The offer whose deal is on screen.
    pub fn deal(&self) -> Option<(&Offer, f32)> {
        let o = self.board.offer(self.previewing)?;
        (self.screen == Screen::Contract).then_some((o, self.clock))
    }

    pub fn theater(&self) -> Option<&Theater> {
        self.theater.as_ref()
    }

    /// Show `t` on the missions screen. The same site stays selected if it's
    /// still one your side fights for; else the first that is.
    pub fn set_theater(&mut self, t: Option<Theater>) {
        let was = self
            .theater
            .as_ref()
            .and_then(|old| old.contested_site(self.site).map(|s| (old.system, s.index)));
        self.site = t
            .as_ref()
            .and_then(|t| {
                let same = (0..t.contested.len()).find(|&k| {
                    t.sortie(k).is_some()
                        && was
                            == Some((
                                t.system,
                                t.contested_site(k).map_or(usize::MAX, |s| s.index),
                            ))
                });
                same.or_else(|| (0..t.contested.len()).find(|&k| t.sortie(k).is_some()))
            })
            .unwrap_or(0);
        // Onto the next contract (another ship's, or in another system):
        // the move plays over the system view.
        if let Some(new) = t.as_ref().filter(|t| t.ship.is_some()) {
            if let Some(ship) = &new.ship {
                self.set_ended(&ship.ended);
            }
            if let Some(old) = self.seen.take().filter(|old| Transfer::between(old, new)) {
                self.transfer = Some(if self.switched {
                    Transfer::warp(old)
                } else {
                    Transfer::after(old, new)
                });
            }
            self.switched = false;
            self.seen = Some(new.clone());
        }
        self.theater = t;
    }

    /// You switched company (#449): the move onto your new ship, as its
    /// contract next shows, is your hull warping across.
    pub fn switched(&mut self) {
        self.switched = true;
    }

    /// Whether the menu knows which contract it last showed (it doesn't
    /// after a battle or a launch: the app hands it over with `remember`).
    pub fn remembers(&self) -> bool {
        self.seen.is_some() || self.transfer.is_some()
    }

    /// The last theater you were shown aboard a ship, `t`, from before this
    /// menu (in the profile): a move on from it plays as the next one shows.
    pub fn remember(&mut self, t: Theater) {
        if !self.remembers() {
            self.seen = Some(t);
        }
    }

    /// The contract you've been shown all of: while a transfer plays, the
    /// one it leaves (so one cut short, by quitting say, plays again).
    pub fn shown(&self) -> Option<&Theater> {
        // Not a contract whose summary you've closed: it's done with.
        let closed = |t: &&Theater| {
            let ship = t.ship.as_ref();
            ship.is_some_and(|s| self.closed == Some((s.id, s.signed)))
        };
        self.card
            .as_ref()
            .map(|(t, _)| t)
            .or(self.transfer.as_ref().map(|tr| &tr.from))
            .or(self.seen.as_ref().filter(|t| !closed(t)))
    }

    /// You've seen the theater on screen (deploying into it, say): a
    /// transfer playing toward it needn't play again.
    pub fn settle(&mut self) {
        if let Some((_, e)) = self.card.take() {
            self.closed = Some((e.ship, e.signed));
        }
        if self.transfer.take().is_some() {
            self.seen = self.theater.clone().filter(|t| t.ship.is_some());
        }
    }

    /// `dt` seconds on: a transfer plays on while the system view shows
    /// it (not under a contract's card).
    pub fn advance(&mut self, dt: f32) {
        self.clock += dt;
        if self.card.is_some() {
            return;
        }
        let Some(t) = self.system() else { return };
        let done = self.transfer.as_ref().is_some_and(|tr| tr.leg(t).is_none());
        if done {
            self.transfer = None;
        } else if let Some(tr) = self.transfer.as_mut() {
            tr.age += dt;
        }
    }

    /// The galaxy's record of your company's ended contracts (newest
    /// last): if the one you were last shown is among them, its card comes
    /// up (#414), whether or not there's a next contract yet (#501).
    pub fn set_ended(&mut self, ended: &[Ended]) {
        if self.card.is_some() {
            return;
        }
        let Some(old) = self.seen.as_ref() else {
            return;
        };
        let Some(ship) = &old.ship else { return };
        let key = (ship.id, ship.signed);
        if self.closed == Some(key) {
            return;
        }
        if let Some(e) = ended.iter().rev().find(|e| (e.ship, e.signed) == key) {
            self.card = Some((old.clone(), *e));
        }
    }

    /// The contract card, if it's showing: how the contract you were on
    /// ended. A move to the next one waits until it's dismissed (#414).
    pub fn summary(&self) -> Option<&Ended> {
        self.card
            .as_ref()
            .map(|(_, e)| e)
            .filter(|_| self.screen == Screen::Missions)
    }

    /// Dismiss the contract card: on to the move to the next contract, or
    /// to waiting for one.
    pub fn go_on(&mut self) {
        if let Some((_, e)) = self.card.take() {
            self.closed = Some((e.ship, e.signed));
        }
        // Nothing signed next: what is, is yours to pick.
        if self.theater.as_ref().is_none_or(|t| t.contested.is_empty())
            && !matches!(self.job, Some(Some(_)))
        {
            self.screen = Screen::Ships;
        }
    }

    /// The move onto the next contract, while it plays (not under a card).
    pub fn transfer(&self) -> Option<&Transfer> {
        self.transfer.as_ref().filter(|_| self.card.is_none())
    }

    /// The move onto the next contract, set to play once any card's
    /// dismissed.
    pub fn next_move(&self) -> Option<&Transfer> {
        self.transfer.as_ref()
    }

    /// On the contract page with no battle to show: the war goes on (the app
    /// runs it, a slice a frame) until there is one. Not under the summary.
    pub fn waiting(&self) -> bool {
        self.screen == Screen::Missions
            && self.card.is_none()
            && self.theater.as_ref().is_none_or(|t| t.contested.is_empty())
    }

    /// On the board with no contract, and none on offer: the war goes on
    /// until there's one.
    pub fn waiting_for_offers(&self) -> bool {
        self.screen == Screen::Ships && self.job == Some(None) && self.board.empty()
    }

    /// The contract you're signed on for, if any.
    pub fn set_job(&mut self, job: Option<JobAt>) {
        self.job = Some(job);
        self.board.waiting = self.waiting_for_offers();
        if !self.opens(self.cursor) {
            self.cursor += 1;
        }
    }

    /// Whether main menu row `i` can be opened: Contract takes one, or one
    /// that's ended and you haven't seen how.
    fn opens(&self, i: usize) -> bool {
        i != 0 || self.card.is_some() || self.job != Some(None)
    }

    /// Select contested site `k` (on the missions screen).
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
        if hit == Hit::Back {
            self.go_lit = false;
            return !std::mem::replace(&mut self.back_lit, true);
        }
        self.back_lit = false;
        if matches!(hit, Hit::Continue | Hit::Deploy) {
            return !std::mem::replace(&mut self.go_lit, true);
        }
        self.go_lit = false;
        self.star = match hit {
            Hit::Star(i) => Some(i),
            _ => None,
        };
        if matches!(self.screen, Screen::Settings | Screen::Controls) {
            return self.hover_settings(hit);
        }
        let i = match hit {
            Hit::Row(i) | Hit::Bind(i, _) => i,
            Hit::Volume => VOLUME_ROW,
            Hit::Filter(_) | Hit::Sort(_) | Hit::Star(_) | Hit::Setting(_) => return false,
            Hit::Back | Hit::Continue | Hit::Deploy => unreachable!(),
        };
        let slot = match self.screen {
            Screen::Main => &mut self.cursor,
            // Picked by a click, so the way to DEPLOY doesn't pick others.
            Screen::Missions => return false,
            // Its one row, SIGN ON: nothing to select.
            Screen::Contract => return false,
            Screen::Settings | Screen::Controls => unreachable!("above"),
            Screen::Ships => return self.board.pick_row(i),
        };
        std::mem::replace(slot, i) != i
    }

    /// The pointer moved onto `hit` on the settings page: a setting, or a key.
    fn hover_settings(&mut self, hit: Hit) -> bool {
        // Waiting for a key, it stays put.
        if self.capturing() {
            return false;
        }
        let setting = match hit {
            Hit::Setting(i) => Some(i),
            Hit::Volume => Some(VOLUME_ROW),
            _ => None,
        };
        let was = self.screen;
        match setting {
            Some(i) => {
                self.screen = Screen::Settings;
                let moved = std::mem::replace(&mut self.setting, i) != i;
                moved || was != self.screen
            }
            None => {
                self.screen = Screen::Controls;
                self.controls.hover(hit) || was != self.screen
            }
        }
    }

    /// A click on `hit`, `at` (0..1) across it.
    pub fn click(&mut self, hit: Hit, at: Vec2, settings: &Settings) -> Click<Action> {
        if hit == Hit::Back {
            self.back_lit = false;
            self.controls.capturing = false;
            return (Nav::Back, self.input(Nav::Back));
        }
        if matches!(self.screen, Screen::Settings | Screen::Controls) {
            self.hover_settings(hit);
        }
        if self.screen == Screen::Controls {
            let (nav, change) = self.controls.click(hit);
            return (nav, change.map(Action::Setting));
        }
        // A star's only to read.
        if let Hit::Star(_) = hit {
            return (Nav::Right, None);
        }
        match hit {
            Hit::Filter(k) => {
                self.board.toggle(k);
                return (Nav::Right, None);
            }
            Hit::Sort(k) => {
                self.board.sort_by(k);
                return (Nav::Right, None);
            }
            _ => {}
        }
        self.hover(hit);
        let nav = match hit {
            Hit::Volume => {
                return (
                    Nav::Right,
                    Some(Action::Setting(volume_click(settings, at.x))),
                );
            }
            Hit::Row(k) if self.screen == Screen::Missions => {
                if self.summary().is_none() {
                    self.pick_site(k);
                }
                return (Nav::Down, None);
            }
            Hit::Row(_) | Hit::Bind(..) | Hit::Setting(_) | Hit::Continue | Hit::Deploy => {
                Nav::Select
            }
            Hit::Back | Hit::Filter(_) | Hit::Sort(_) | Hit::Star(_) => unreachable!(),
        };
        (nav, self.input(nav))
    }

    /// The mouse left the BACK button (or CONTINUE).
    pub fn unhover(&mut self) {
        self.back_lit = false;
        self.go_lit = false;
        self.star = None;
    }

    /// The theater whose system the screen shows in 3D instead of a map
    /// (the missions screen, with sites to pick from).
    /// The theater the system view shows: the contract card's, while it's
    /// up; else the one on screen; else, while you wait for work, the last
    /// one you were in.
    pub fn system(&self) -> Option<&Theater> {
        if self.screen != Screen::Missions {
            return None;
        }
        if let Some((t, _)) = &self.card {
            return Some(t);
        }
        match self.theater.as_ref().filter(|t| !t.contested.is_empty()) {
            Some(t) => Some(t),
            // On the way to another: between systems.
            None if matches!(&self.job, Some(Some(j)) if j.flying) => None,
            None => self.seen.as_ref(),
        }
    }

    /// The map the selected site is fought on, its conditions, and the side you'd take in
    /// it (always the first: the theater's sides stand in for the map's):
    /// the menu's backdrop shows it (under the system view on the missions
    /// screen, loaded ready to deploy). None off the theater.
    pub fn backdrop(&self) -> Option<crate::app::Backdrop> {
        (self.screen == Screen::Missions)
            .then(|| {
                self.selected_site()
                    .map(|s| (s.map_kind(), s.map_seed, s.conditions(), 0))
            })
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
        if self.screen == Screen::Main {
            self.draw_main(&mut p, &mut hits, profile, w, h);
            return Layer {
                verts: fade(p.finish(), alpha),
                hits,
            };
        }
        if matches!(self.screen, Screen::Ships | Screen::Contract) {
            if self.screen == Screen::Ships {
                self.board
                    .draw(&mut p, &mut hits, self.back_lit, self.clock, w, h);
            } else {
                let o = self.board.offer(self.previewing);
                let mine = self.board.mine.as_ref();
                let lit = (self.back_lit, self.go_lit);
                work_view::draw_deal(&mut p, &mut hits, o, mine, lit, w, h);
            }
            return Layer {
                verts: fade(p.finish(), alpha),
                hits,
            };
        }
        if matches!(self.screen, Screen::Settings | Screen::Controls) {
            self.draw_settings(&mut p, &mut hits, profile, w, h);
            return Layer {
                verts: fade(p.finish(), alpha),
                hits,
            };
        }
        if let Some(t) = self.system() {
            crate::theater_view::veil(&mut p, t, self.transfer.as_ref(), w, h);
        }
        self.draw_system(&mut p, &mut hits, w, h);
        Layer {
            verts: fade(p.finish(), alpha),
            hits,
        }
    }
}

impl Menu {
    /// The cluster the main menu shows.
    pub fn set_cluster(&mut self, c: Cluster) {
        self.cluster = Some(c);
        self.star = None;
    }

    /// Whether the main menu's showing a cluster other than as of `tick`, aboard `crew`.
    pub fn wants_cluster(&self, tick: u64, crew: Option<u32>) -> bool {
        self.screen == Screen::Main
            && self
                .cluster
                .as_ref()
                .is_none_or(|c| (c.tick, c.crew) != (tick, crew))
    }

    /// The stars behind the main menu, the board and settings, while they're up.
    pub fn cluster(&self) -> Option<(&Cluster, f32)> {
        let c = self.cluster.as_ref()?;
        use Screen::{Main, Ships};
        let adrift = self.screen == Screen::Missions && self.system().is_none();
        (adrift || matches!(self.screen, Main | Ships)).then_some((c, self.clock))
    }

    /// The main menu: who you are and what you can do, on a panel over the
    /// stars.
    fn draw_main(&self, p: &mut Painter, hits: &mut Hits<Hit>, profile: &Profile, w: f32, h: f32) {
        cluster_view::draw(p, hits, self.cluster.as_ref(), self.star, self.clock, w, h);
        design::panel(p, h);
        let x = design::MARGIN;

        // Who you are, and what you have to spend.
        let mut y = design::MARGIN;
        let r = profile.rank();
        p.text(
            Vec2::new(x, y),
            &format!("{} {}", title(r.title()), r.level),
            &design::BODY,
            Align::Left,
        );
        y += design::UNIT * 3.0;
        design::bar(p, Vec2::new(x, y), design::COLUMN, r.progress());
        y += design::UNIT * 3.0;
        p.text(
            Vec2::new(x, y),
            &format!("{} cr", profile.crystals),
            &design::BODY.colored(design::BLUE),
            Align::Left,
        );

        // What you can do.
        y += design::UNIT * 9.0;
        for (i, name) in MAIN_ITEMS.iter().enumerate().take(MAIN_ACTIONS) {
            let at = Vec2::new(x, y);
            y += design::ACTION;
            if !self.opens(i) {
                design::action_off(p, at, name);
                continue;
            }
            let (min, size) = design::action(p, at, name, i == self.cursor);
            hits.add(p, min, size, Hit::Row(i));
        }

        // And, at the foot, the rest.
        let mut fx = x;
        for (i, name) in MAIN_ITEMS.iter().enumerate().skip(MAIN_ACTIONS) {
            let at = Vec2::new(fx, h - design::MARGIN - design::SMALL.size);
            let (min, size) = design::link(p, at, name, i == self.cursor);
            hits.add(p, min, size, Hit::Row(i));
            fx += size.x + design::UNIT * 2.0;
        }
    }

    /// The settings page: the settings, then the keys in their columns.
    fn draw_settings(
        &self,
        p: &mut Painter,
        hits: &mut Hits<Hit>,
        profile: &Profile,
        w: f32,
        h: f32,
    ) {
        let on_keys = self.screen == Screen::Controls;
        let at = page(p, "Settings", 3, w, h);
        let setting = if on_keys { usize::MAX } else { self.setting };
        let set = &profile.settings;
        settings_rows(p, hits, at[0], set, setting, PAGE_SETTINGS, Hit::Setting);
        let mut keys = self.controls;
        if !on_keys {
            keys.row = usize::MAX;
        }
        controls_page(p, hits, [at[1], at[2]], &set.binds, &keys);
        back_button(p, hits, Vec2::new(at[0].x, back_y(h)), WIDE, self.back_lit);
    }

    /// The contract page: the system with its marks, and the panel over it.
    fn draw_system(&self, p: &mut Painter, hits: &mut Hits<Hit>, w: f32, h: f32) {
        use crate::theater_view;
        // The contract that ended, over the theater it was in (#414).
        if let Some((t, e)) = &self.card {
            theater_view::summary(p, hits, t, e, self.go_lit, w, h);
            return;
        }
        let theater = self.theater.as_ref().filter(|t| !t.contested.is_empty());
        if let Some(t) = theater {
            let moving = self.transfer.as_ref();
            theater_view::draw(p, hits, t, moving, self.site, w, h);
        } else if self.system().is_none() && self.cluster.is_none() {
            // The galaxy's still loading.
            p.rect(Vec2::ZERO, Vec2::new(w, h), [0.0, 0.0, 0.0, 1.0]);
        }
        design::panel(p, h);
        // Nothing to pick while you're on the move.
        let live = self.transfer.is_none();
        match (theater, &self.job) {
            (Some(t), _) => self.draw_job(p, hits, t, live, h),
            // Signed on, and no battle yet: the war goes on until there is.
            (None, Some(Some(job))) => {
                let at = Vec2::splat(design::MARGIN);
                p.text(at, &job.system, &design::HEAD, Align::Left);
                let at = at + Vec2::Y * UNIT * 5.0;
                let c = at + Vec2::new(8.0, design::SMALL.size * 0.5);
                crate::hud::spinner_small(p, c, self.clock);
                let text = if job.flying {
                    "On the way"
                } else {
                    "No battle yet"
                };
                let style = design::SMALL.colored(design::TEXT_2);
                let at = at + Vec2::X * UNIT * 3.5;
                p.text(at, text, &style, Align::Left);
            }
            (None, _) => {}
        }
        back_at_foot(p, hits, self.back_lit, h);
    }

    /// The contract page's panel: the system, and (while there's a pick to
    /// make: `live`) its fights to pick from, the stakes and DEPLOY. Whose
    /// side you're on is in the picture: whose fleet you fly with, or whose
    /// world you're on.
    fn draw_job(&self, p: &mut Painter, hits: &mut Hits<Hit>, t: &Theater, live: bool, h: f32) {
        use design::{BLUE, BODY, HEAD, SMALL, TEXT, TEXT_2, UNIT};
        let x = design::MARGIN;
        let right = x + design::COLUMN;
        let mut y = design::MARGIN;

        p.text(Vec2::new(x, y), &t.name, &HEAD, Align::Left);
        if !live {
            return;
        }

        // The fights, world by world.
        y += UNIT * 7.0;
        let fights: Vec<usize> = (0..t.contested.len())
            .filter(|&k| t.sites[t.contested[k]].committed)
            .collect();
        let any = !fights.is_empty();
        let site = |k: usize| &t.sites[t.contested[k]];
        let mut world = None;
        for &k in &fights {
            let s = site(k);
            if world != Some(s.planet) {
                if world.is_some() {
                    y += UNIT * 1.5;
                }
                world = Some(s.planet);
                let name = planet_name(&t.name, s.planet);
                p.text(Vec2::new(x, y), &name, &SMALL.colored(TEXT_2), Align::Left);
                y += UNIT * 3.0;
            }
            // Lettered, where a world has several of a kind.
            let alike: Vec<usize> = fights
                .iter()
                .copied()
                .filter(|&j| site(j).planet == s.planet && site(j).short_name() == s.short_name())
                .collect();
            let name = match alike.iter().position(|&j| j == k) {
                Some(n) if alike.len() > 1 => {
                    format!("{} {}", s.short_name(), (b'A' + n as u8) as char)
                }
                _ => s.short_name().to_string(),
            };
            let on = k == self.site;
            let style = BODY.colored(if on { design::AMBER } else { TEXT });
            p.text(Vec2::new(x, y), &name, &style, Align::Left);
            let mode = MISSIONS[s.mission].contract.name();
            let at = Vec2::new(right, y + (BODY.size - SMALL.size) * 0.5);
            p.text(at, mode, &SMALL.colored(TEXT_2), Align::Right);
            let pad = (UNIT * 3.5 - BODY.size) * 0.5;
            hits.add(
                p,
                Vec2::new(x - UNIT * 2.0, y - pad),
                Vec2::new(design::COLUMN + UNIT * 4.0, UNIT * 3.5),
                Hit::Row(k),
            );
            y += UNIT * 3.5;
        }
        let picked = self.selected_site().filter(|s| s.committed);
        let Some(site) = picked.filter(|_| any) else {
            let style = BODY.colored(TEXT_2);
            p.text(Vec2::new(x, y), "No fight yet", &style, Align::Left);
            return;
        };

        // What you're up against, what a win pays, and what you've got left.
        let deploy = design::foot(h, 1);
        let row = UNIT * 3.0;
        let mut y = deploy.y - UNIT * 2.0 - row * 3.0;
        let mut line = |p: &mut Painter, name: &str, value: &str, color| {
            let at = Vec2::new(x, y);
            p.text(at, name, &SMALL.colored(TEXT_2), Align::Left);
            let at = Vec2::new(right, y);
            p.text(at, value, &SMALL.colored(color), Align::Right);
            y += row;
        };
        let tier = t.tiers[site.enemy];
        let gear = crate::economy::level_label(tier);
        let (wage, kill) = t.ship.as_ref().map_or((0, 0), |s| s.pay);
        let own = t.ship.as_ref().map_or(0, |s| s.own.left.min(s.committed));
        line(p, "Enemy gear", &gear, TEXT);
        line(p, "Pay", &format!("{wage} cr + {kill} a kill"), BLUE);
        line(p, "Your hulls", &own.to_string(), design::AMBER);
        design::button(p, deploy, design::COLUMN, "Deploy", true, self.go_lit);
        let size = Vec2::new(design::COLUMN, design::BUTTON);
        hits.add(p, deploy, size, Hit::Deploy);
    }
}

/// The way back, as a button.
fn back_button(p: &mut Painter, hits: &mut Hits<Hit>, min: Vec2, width: f32, lit: bool) {
    design::button(p, min, width, "Back", true, lit);
    hits.add(p, min, Vec2::new(width, design::BUTTON), Hit::Back);
}

/// BACK, at a panel's foot.
pub fn back_at_foot(p: &mut Painter, hits: &mut Hits<Hit>, lit: bool, h: f32) {
    back_button(p, hits, design::foot(h, 0), design::COLUMN, lit);
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

const PAUSE_ITEMS: [&str; 5] = ["Resume", "Garage", "Settings", "Leave battle", "Quit"];
/// Of those, the ones that keep you in the battle; the rest are set apart.
const PAUSE_STAYS: usize = 3;

pub struct Pause {
    cursor: usize,
    /// The settings list is open in the box, on this row.
    settings: Option<usize>,
    /// The controls screen is open in the box (over the settings).
    controls: Option<Controls>,
    /// The pointer is on the BACK button.
    back_lit: bool,
}

impl Pause {
    pub fn new() -> Pause {
        Pause {
            cursor: 0,
            settings: None,
            controls: None,
            back_lit: false,
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
        if hit == Hit::Back {
            return !std::mem::replace(&mut self.back_lit, true);
        }
        self.back_lit = false;
        if let Some(c) = &mut self.controls {
            return c.hover(hit);
        }
        let i = match hit {
            Hit::Row(i) | Hit::Bind(i, _) => i,
            Hit::Volume => VOLUME_ROW,
            Hit::Back
            | Hit::Continue
            | Hit::Filter(_)
            | Hit::Sort(_)
            | Hit::Star(_)
            | Hit::Deploy
            | Hit::Setting(_) => {
                unreachable!()
            }
        };
        let slot = self.settings.as_mut().unwrap_or(&mut self.cursor);
        std::mem::replace(slot, i) != i
    }

    /// A click on `hit`, `at` (0..1) across it.
    pub fn click(&mut self, hit: Hit, at: Vec2, settings: &Settings) -> Click<PauseAction> {
        if hit == Hit::Back {
            self.back_lit = false;
            return (Nav::Back, self.input(Nav::Back));
        }
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
            Hit::Back
            | Hit::Continue
            | Hit::Filter(_)
            | Hit::Sort(_)
            | Hit::Star(_)
            | Hit::Deploy
            | Hit::Setting(_) => {
                unreachable!()
            }
        };
        (nav, self.input(nav))
    }

    /// The pointer left the BACK button.
    pub fn unhover(&mut self) {
        self.back_lit = false;
    }

    /// Dims the battle and draws the box.
    pub fn draw(&self, profile: &Profile, width: f32, height: f32) -> Layer<Hit> {
        let scale = height / REF_HEIGHT;
        let (w, h) = (width / scale, REF_HEIGHT);
        let mut p = Painter::new(scale);
        let mut hits = Hits::new();
        p.rect(Vec2::ZERO, Vec2::new(w, h), [0.0, 0.0, 0.0, 0.45]);
        if let Some(c) = &self.controls {
            let at = page(&mut p, "Controls", 2, w, h);
            controls_page(
                &mut p,
                &mut hits,
                [at[0], at[1]],
                &profile.settings.binds,
                c,
            );
            let foot = Vec2::new(at[0].x, back_y(h));
            back_button(&mut p, &mut hits, foot, WIDE, self.back_lit);
            return Layer {
                verts: p.finish(),
                hits,
            };
        }
        let inner = match self.settings {
            Some(_) => TITLE_H + SETTING_ROW * SETTING_ROWS as f32 + FOOT_GAP + design::BUTTON,
            None => {
                TITLE_H
                    + design::ACTION * (PAUSE_ITEMS.len() as f32 - 1.0)
                    + LEAVE_GAP
                    + design::HEAD.size
            }
        };
        let pad = Vec2::splat(UNIT * 5.0);
        let size = Vec2::new(WIDE, inner) + pad * 2.0;
        let min = (Vec2::new(w, h) - size) * 0.5;
        p.rect(min, size, design::GROUND);
        p.set_halo(None);
        let at = min + pad;
        // Settings and controls go back a step; the pause list has RESUME.
        let foot = Vec2::new(at.x, min.y + size.y - pad.y - design::BUTTON);
        let list = at + Vec2::Y * TITLE_H;
        let title = match self.settings {
            Some(i) => {
                let set = &profile.settings;
                settings_rows(&mut p, &mut hits, list, set, i, SETTING_ROWS, Hit::Row);
                "Settings"
            }
            None => {
                let mut y = list.y;
                for (i, name) in PAUSE_ITEMS.iter().enumerate() {
                    if i == PAUSE_STAYS {
                        y += LEAVE_GAP;
                    }
                    let on = i == self.cursor;
                    let (hit, size) = design::action(&mut p, Vec2::new(at.x, y), name, on);
                    hits.add(&p, hit, size, Hit::Row(i));
                    if i == PAUSE_STAYS {
                        let at = Vec2::new(at.x, y + UNIT * 3.25);
                        let style = design::SMALL.colored(design::TEXT_2);
                        p.text(at, "It plays out without you", &style, Align::Left);
                    }
                    y += design::ACTION;
                }
                "Paused"
            }
        };
        let listing = self.settings.is_none();
        let color = if listing {
            design::TEXT_2
        } else {
            design::TEXT
        };
        p.text(at, title, &design::HEAD.colored(color), Align::Left);
        if !listing {
            back_button(&mut p, &mut hits, foot, WIDE, self.back_lit);
        }
        Layer {
            verts: p.finish(),
            hits,
        }
    }
}

/// How wide a settings page's columns and the pause box's content are, and how tall
/// their title and the gap over the buttons at their foot.
const WIDE: f32 = UNIT * 42.0;
const TITLE_H: f32 = UNIT * 7.0;
const FOOT_GAP: f32 = UNIT * 3.0;
/// Over the pause rows that leave the battle.
const LEAVE_GAP: f32 = UNIT * 2.0;
/// A setting's row.
const SETTING_ROW: f32 = UNIT * 7.0;

/// A row's name, amber if it's selected.
fn row_name(p: &mut Painter, at: Vec2, name: &str, on: bool) {
    let color = if on { design::AMBER } else { design::TEXT };
    p.text(at, name, &design::BODY.colored(color), Align::Left);
}

/// The first `n` settings: a row each with its value on the right, and what
/// it does; clicked as `hit`.
fn settings_rows(
    p: &mut Painter,
    hits: &mut Hits<Hit>,
    at: Vec2,
    settings: &Settings,
    selected: usize,
    n: usize,
    hit: fn(usize) -> Hit,
) {
    use design::{AMBER, BODY, SMALL, TEXT, TEXT_2};
    let right = at.x + WIDE;
    let note = |p: &mut Painter, y: f32, text: &str| {
        let at = Vec2::new(at.x, y + UNIT * 2.5);
        p.text(at, text, &SMALL.colored(TEXT_2), Align::Left);
    };
    for i in 0..n {
        let y = at.y + SETTING_ROW * i as f32;
        let min = Vec2::new(at.x - UNIT * 2.0, y - UNIT * 1.5);
        let size = Vec2::new(WIDE + UNIT * 4.0, SETTING_ROW);
        hits.add(p, min, size, hit(i));
        let at = Vec2::new(at.x, y);
        let on = i == selected;
        match i {
            VOLUME_ROW => {
                row_name(p, at, "Sound volume", on);
                let pct = (settings.volume() * 100.0).round() as u32;
                let value = match pct {
                    0 => "Off".to_string(),
                    pct => format!("{pct}%"),
                };
                p.text(
                    Vec2::new(right, y),
                    &value,
                    &BODY.colored(TEXT),
                    Align::Right,
                );
                let bar = Vec2::new(at.x, y + UNIT * 3.0);
                design::bar(p, bar, WIDE, settings.volume());
                // Exactly as wide as the bar, so a click sets the level under it.
                let min = bar - Vec2::Y * UNIT;
                hits.add(p, min, Vec2::new(WIDE, UNIT * 2.0), Hit::Volume);
            }
            CONTROLS_ROW => {
                row_name(p, at, "Controls", on);
                let changed = settings.binds != Binds::default();
                note(p, y, if changed { "Changed" } else { "Default" });
            }
            _ => {
                let (name, what, flag) = &SETTINGS[i];
                row_name(p, at, name, on);
                note(p, y, what);
                let mut copy = *settings;
                let (text, color) = match *flag(&mut copy) {
                    true => ("On", AMBER),
                    false => ("Off", TEXT_2),
                };
                p.text(
                    Vec2::new(right, y),
                    text,
                    &BODY.colored(color),
                    Align::Right,
                );
            }
        }
    }
}

/// Between a page's columns, and from its top to its title.
const PAGE_GAP: f32 = UNIT * 5.0;
const PAGE_TOP: f32 = UNIT * 9.0;

/// A whole page titled `title`, with `n` columns in the middle of it: where
/// each starts.
fn page(p: &mut Painter, title: &str, n: usize, w: f32, h: f32) -> Vec<Vec2> {
    p.set_halo(None);
    p.rect(Vec2::ZERO, Vec2::new(w, h), design::GROUND);
    // Further apart where there's room.
    let spare = w - design::MARGIN * 2.0 - WIDE * n as f32;
    let gap = (spare / n as f32).clamp(PAGE_GAP, PAGE_GAP * 2.5);
    let pitch = WIDE + gap;
    let x = (w - pitch * n as f32 + gap) * 0.5;
    p.text(Vec2::new(x, PAGE_TOP), title, &design::TITLE, Align::Left);
    (0..n)
        .map(|k| Vec2::new(x + pitch * k as f32, PAGE_TOP + UNIT * 12.0))
        .collect()
}

/// Where a page's BACK goes.
fn back_y(h: f32) -> f32 {
    h - PAGE_TOP - design::BUTTON
}

/// Pitch of a key's row, and of the label over each column of them.
const CONTROL_ROW: f32 = UNIT * 4.5;
const CONTROL_LABEL: f32 = UNIT * 4.5;
/// A key slot, and the gap between two.
const SLOT: Vec2 = Vec2::new(UNIT * 9.0, UNIT * 3.5);
const SLOT_GAP: f32 = UNIT;

/// The keys: each action with its two slots, the battle's in the column at
/// `at[0]`, the garage's at `at[1]` with the reset under them.
fn controls_page(
    p: &mut Painter,
    hits: &mut Hits<Hit>,
    at: [Vec2; 2],
    binds: &Binds,
    c: &Controls,
) {
    use design::{AMBER, SMALL, TEXT, TEXT_2};
    for (at, name) in at.into_iter().zip(["In battle", "In the garage"]) {
        p.text(at, name, &SMALL.colored(TEXT_2), Align::Left);
    }
    let mut y = [at[0].y + CONTROL_LABEL, at[1].y + CONTROL_LABEL];
    for (i, &action) in ACTIONS.iter().enumerate() {
        let k = usize::from(action.context() == Context::Garage);
        let at = Vec2::new(at[k].x, y[k]);
        y[k] += CONTROL_ROW;
        let on = i == c.row;
        let mid = at.y + SLOT.y * 0.5;
        let min = Vec2::new(at.x - UNIT, mid - CONTROL_ROW * 0.5);
        hits.add(
            p,
            min,
            Vec2::new(WIDE + UNIT * 2.0, CONTROL_ROW),
            Hit::Row(i),
        );
        let name = Vec2::new(at.x, mid - SMALL.size * 0.5);
        let style = SMALL.colored(if on { AMBER } else { TEXT });
        p.text(name, action.name(), &style, Align::Left);
        for (slot, key) in binds.keys(action).into_iter().enumerate() {
            let left = (SLOTS - slot) as f32 * (SLOT.x + SLOT_GAP) - SLOT_GAP;
            let min = Vec2::new(at.x + WIDE - left, at.y);
            hits.add(p, min, SLOT, Hit::Bind(i, slot));
            let picked = on && slot == c.slot;
            let waiting = picked && c.capturing;
            p.rect(min, SLOT, with_alpha(TEXT, 0.015));
            if picked {
                let edge = if waiting { theme::HOT } else { AMBER };
                p.rect_outline(min, SLOT, 1.0, edge);
            }
            let (text, color) = match key.and_then(binds::key_name) {
                _ if waiting => ("Press", theme::HOT),
                Some(name) => (name, if picked { AMBER } else { TEXT }),
                None => continue,
            };
            let at = Vec2::new(min.x + SLOT.x * 0.5, mid - SMALL.size * 0.5);
            p.text(at, text, &SMALL.colored(color), Align::Center);
        }
    }

    // Back to the defaults, if they've been changed.
    let min = Vec2::new(at[1].x, y[1] + UNIT * 3.0);
    let changed = *binds != Binds::default();
    let on = c.row == RESET_ROW;
    let text = if changed { "Reset" } else { "All default" };
    design::button(p, min, WIDE, text, changed, on);
    let size = Vec2::new(WIDE, design::BUTTON);
    if on && !changed {
        p.rect_outline(min, size, 1.0, TEXT_2);
    }
    hits.add(p, min, size, Hit::Row(RESET_ROW));
}

/// "THE CAPITAL" as "The Capital".
pub fn title(caps: &str) -> String {
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
    use crate::faction::Faction;

    #[test]
    fn main_menu_navigation() {
        let mut m = Menu::new();
        assert_eq!(m.input(Nav::Down), None);
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
        // With one, Enter deploys to the site your side fights for.
        m.set_theater(theater(Faction::Imperium));
        let t = m.theater().unwrap().clone();
        assert!(
            t.sortie(m.site).is_some(),
            "starts on a site you can fight for"
        );
        assert_eq!(
            m.input(Nav::Select),
            Some(Action::Fight(t.sortie(m.site).unwrap()))
        );
        // Up and down only stop at sites your side fights for.
        for nav in [Nav::Down, Nav::Down, Nav::Up] {
            m.input(nav);
            assert!(t.sortie(m.site).is_some());
        }
        // A site your side isn't fighting for: refused.
        if let Some(k) = (0..t.contested.len()).find(|&k| t.sortie(k).is_none()) {
            m.pick_site(k);
            assert_eq!(m.input(Nav::Select), Some(Action::Refused));
        }
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
        let (t, _) = g.await_work_staying(Some(f.index()), 1, 400);
        assert!(t.as_ref().is_some_and(|t| t.ship.is_some()));
        t
    }

    /// Find work: the board, then one offer's deal, then signing on.
    #[test]
    fn switching_company() {
        let offer = |id| {
            let mut o = crate::work_view::tests::offer(1, 3, 0);
            o.id = id;
            o
        };
        let p = Profile::in_memory();
        let mut m = Menu::new();
        m.input(Nav::Down);
        assert_eq!(m.input(Nav::Select), None);
        assert_eq!(m.screen, Screen::Ships);
        assert!(m.wants_ships() && !m.wants_theater());
        // None on offer: Enter does nothing.
        assert_eq!(m.input(Nav::Select), None);
        assert_eq!(m.screen, Screen::Ships);
        m.board.set(vec![offer(4), offer(9)], None);
        m.input(Nav::Down);
        // Enter opens the deal; nothing switches.
        assert_eq!(m.input(Nav::Select), None);
        assert_eq!(m.screen, Screen::Contract);
        assert_eq!(m.deal().unwrap().0.id, 9);
        assert_eq!(m.input(Nav::Select), Some(Action::Switch(9)));
        m.input(Nav::Back);
        assert_eq!(m.screen, Screen::Ships);
        // The pick follows its offer as the board changes.
        m.board.set(vec![offer(2), offer(4), offer(9)], None);
        m.input(Nav::Select);
        assert_eq!(m.deal().unwrap().0.id, 9);
        m.input(Nav::Back);
        // And by click.
        assert_eq!(click_drawn(&mut m, &p, Hit::Row(0)).1, None);
        assert_eq!(m.deal().unwrap().0.id, 2);
        let signed = click_drawn(&mut m, &p, Hit::Deploy);
        assert_eq!(signed.1, Some(Action::Switch(2)));
        // Gone from the board: nothing to sign.
        m.board.set(vec![offer(4)], None);
        assert_eq!(m.input(Nav::Select), None);
        m.draw(&p, 1280.0, 720.0, 1.0);
        m.input(Nav::Back);
        // A click on a filter, and on a sort.
        click_drawn(&mut m, &p, Hit::Filter(0));
        click_drawn(&mut m, &p, Hit::Sort(0));
        m.input(Nav::Back);
        assert_eq!(m.screen, Screen::Main);
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
            (Screen::Missions, t),
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
                    Pause::new().draw(&profile, w, h).verts,
                    settings.draw(&profile, w, h).verts,
                    controls.draw(&profile, w, h).verts,
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
    fn back_buttons_go_back() {
        let profile = Profile::in_memory();
        let mut m = Menu::new();
        // Nowhere to go back to from the main menu: QUIT leaves.
        let hits = m.draw(&profile, 1600.0, 900.0, 1.0).hits;
        assert_eq!(hits.center_of(Hit::Back), None);
        m.set_theater(theater(Faction::Ithessar));
        for (screen, to) in [
            (Screen::Missions, Screen::Main),
            (Screen::Settings, Screen::Main),
            (Screen::Controls, Screen::Main),
        ] {
            m.screen = screen;
            assert!(m.hover(Hit::Back));
            assert!(!m.hover(Hit::Back));
            assert_eq!(click_drawn(&mut m, &profile, Hit::Back), (Nav::Back, None));
            assert_eq!(m.screen, to, "back from {screen:?}");
        }
        // Waiting for a key to bind: BACK gives up and leaves.
        m.screen = Screen::Controls;
        m.input(Nav::Select);
        assert!(m.capturing());
        click_drawn(&mut m, &profile, Hit::Back);
        assert_eq!(m.screen, Screen::Main);
        assert!(!m.capturing());

        // The pause box: back out of controls, then settings.
        let mut p = Pause::new();
        let click = |p: &mut Pause| {
            let hits = p.draw(&profile, 1600.0, 900.0).hits;
            let (id, at) = hits.at(hits.center_of(Hit::Back).unwrap()).unwrap();
            assert_eq!(id, Hit::Back);
            p.click(id, at, &profile.settings)
        };
        let hits = p.draw(&profile, 1600.0, 900.0).hits;
        assert_eq!(hits.center_of(Hit::Back), None, "RESUME is the way back");
        p.input(Nav::Down);
        p.input(Nav::Down);
        p.input(Nav::Select);
        for _ in 0..SETTINGS.len() + 1 {
            p.input(Nav::Down);
        }
        p.input(Nav::Select);
        assert!(p.controls.is_some());
        assert_eq!(click(&mut p), (Nav::Back, None));
        assert!(p.controls.is_none() && p.settings.is_some());
        assert_eq!(click(&mut p), (Nav::Back, None));
        assert!(p.settings.is_none());
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
            click_drawn(&mut m, &profile, Hit::Row(2)),
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
        // Its row in the list, or its mark in the picture: clicking picks it.
        let hits = m.draw(&profile, 1600.0, 900.0, 1.0).hits;
        assert!(hits.center_of(Hit::Row(k)).is_some());
        assert_eq!(
            click_drawn(&mut m, &profile, Hit::Row(k)),
            (Nav::Down, None)
        );
        assert_eq!(m.site, k);
        // DEPLOY deploys to it.
        assert_eq!(
            click_drawn(&mut m, &profile, Hit::Deploy),
            (Nav::Select, Some(Action::Fight(t.sortie(k).unwrap())))
        );
        // Esc, or a right click, goes back.
        m.input(Nav::Back);
        assert_eq!(m.screen, Screen::Main);

        m.screen = Screen::Settings;
        assert_eq!(
            click_drawn(&mut m, &profile, Hit::Setting(0)).1,
            Some(Action::Setting(Change::Toggle(0)))
        );
        // The volume bar sets the level under the click: its middle is half volume.
        let half = Settings::MAX_VOLUME as i32 / 2 - profile.settings.volume as i32;
        assert_eq!(
            click_drawn(&mut m, &profile, Hit::Volume).1,
            Some(Action::Setting(Change::Volume(half)))
        );
        assert_eq!(m.setting, VOLUME_ROW);
        // The keys are on the same page.
        let hit = Hit::Bind(0, 1);
        assert_eq!(click_drawn(&mut m, &profile, hit), (Nav::Select, None));
        assert!(m.screen == Screen::Controls && m.capturing());
    }

    #[test]
    fn clicking_the_pause_box() {
        let profile = Profile::in_memory();
        let mut p = Pause::new();
        let click = |p: &mut Pause, hit: Hit| {
            let hits = p.draw(&profile, 1600.0, 900.0).hits;
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
        // Down off the last setting is the first key, and up again goes back.
        m.setting = VOLUME_ROW;
        m.input(Nav::Down);
        assert_eq!(m.screen, Screen::Controls);
        m.input(Nav::Up);
        assert_eq!((m.screen, m.setting), (Screen::Settings, VOLUME_ROW));
        m.input(Nav::Down);
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
        // RESET, at the bottom (Up from the first setting wraps there).
        m.screen = Screen::Settings;
        m.setting = 0;
        m.input(Nav::Up);
        assert_eq!(
            m.input(Nav::Select),
            Some(Action::Setting(Change::ResetBinds))
        );
        // Down off it is the first setting.
        m.input(Nav::Down);
        assert_eq!((m.screen, m.setting), (Screen::Settings, 0));
        m.input(Nav::Back);
        assert_eq!(m.screen, Screen::Main);
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
    fn key_names_clear_their_slots() {
        let room = WIDE - (SLOT.x + SLOT_GAP) * SLOTS as f32;
        for action in ACTIONS {
            let wide = crate::ui::measure(action.name(), &design::SMALL);
            assert!(wide < room, "{} is {wide} of {room}", action.name());
        }
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

    /// Switching company moves only your hull (#449): it warps off the old
    /// ship and onto the new one, in this system or another, and the ship
    /// transit doesn't play. The next move after it is a transit again.
    #[test]
    fn switching_company_warps_your_hull() {
        use crate::theater_view::Leg;
        let legs = |m: &Menu, to: &Theater| {
            let tr = m.transfer().expect("a transfer").clone();
            let mut order: Vec<_> = (0..1000)
                .filter_map(|k| {
                    let mut tr = tr.clone();
                    tr.age = k as f32 * 0.01;
                    tr.leg(to)
                })
                .map(|l| std::mem::discriminant(&l))
                .collect();
            order.dedup();
            order
        };
        let warp = [Leg::WarpOut(0.0), Leg::WarpIn(0.0)].map(|l| std::mem::discriminant(&l));
        let t = theater(Faction::Imperium).unwrap();
        for system in [t.system, t.system + 1] {
            let mut m = Menu::new();
            m.input(Nav::Select);
            m.set_theater(Some(t.clone()));
            m.switched();
            let mut other = t.clone();
            other.system = system;
            other.ship.as_mut().unwrap().id += 1;
            m.set_theater(Some(other.clone()));
            let tr = m.transfer().expect("a transfer");
            assert!(tr.warp && m.summary().is_none());
            assert_eq!(legs(&m, &other), warp);
            for _ in 0..1000 {
                m.advance(0.01);
            }
            assert!(m.transfer().is_none(), "it ends");

            // On to that ship's next contract: the ship flies again.
            let mut next = other.clone();
            next.ship.as_mut().unwrap().signed += 5;
            m.set_theater(Some(next.clone()));
            assert!(!m.transfer().expect("a transfer").warp);
            assert_eq!(legs(&m, &next), [std::mem::discriminant(&Leg::Move(0.0))]);
        }
    }
    /// The move onto the next contract plays however it happened (#386):
    /// though the menu you were shown the old one on is gone (back from a
    /// battle, or a relaunch), the profile's record of it is enough.
    #[test]
    fn next_contract_plays_after_a_battle_or_a_relaunch() {
        use crate::galaxy::theater::Shown;
        use crate::galaxy::{Galaxy, REGION, Rules, SYSTEMS};
        let mut g = Galaxy::with_size(3, SYSTEMS / 4, REGION * 0.5, Rules::default());
        for _ in 0..60 {
            g.step();
        }
        let f = Some(Faction::Imperium.index());
        let (t, _) = g.await_work_staying(f, 1, 400);
        let t = t.expect("work");
        let mut m = Menu::new();
        m.input(Nav::Select);
        m.set_theater(Some(t.clone()));
        let shown = Shown::of(m.shown().unwrap()).unwrap();
        // Saved with the profile, and the menu gone.
        let shown: Shown = serde_json::from_str(&serde_json::to_string(&shown).unwrap()).unwrap();
        drop(m);

        // The war goes on (battles, or ticks while the game was closed)
        // until the company's on its next contract.
        let mut next = None;
        for _ in 0..400 {
            g.step();
            let (n, _) = g.await_work_staying(f, 1, 40);
            // Nobody hires it for a while: you go with another that's hired.
            let Some(n) = n else {
                g.crew = None;
                continue;
            };
            if Transfer::between(&t, &n) {
                next = Some(n);
                break;
            }
        }
        let next = next.expect("a next contract");

        let mut m = Menu::new();
        m.input(Nav::Select);
        assert!(!m.remembers());
        let old = g.shown(&shown).expect("the old theater");
        assert_eq!(old.system, t.system);
        assert!(Transfer::between(&old, &next));
        m.remember(old);
        m.set_theater(Some(next.clone()));
        let tr = m.next_move().expect("a transfer");
        assert_eq!(tr.jumps(&next), t.system != next.system);
        // Until it's played, what you've been shown is still the old one:
        // quit now and it plays again.
        assert_eq!(Shown::of(m.shown().unwrap()), Some(shown.clone()));
        // Same company, new contract: the old contract ended, so show its
        // summary card before the transfer animation (#414).
        let (a, b) = (t.ship.as_ref().unwrap(), next.ship.as_ref().unwrap());
        if a.id == b.id || m.summary().is_some() {
            let e = *m.summary().expect("the summary of the contract that ended");
            assert_eq!((e.ship, e.signed), (a.id, a.signed));
            assert!(e.tick >= e.signed && e.lost <= e.committed);
            for _ in 0..1000 {
                m.advance(0.01);
            }
            assert!(m.transfer().is_none(), "no move under the card");
            assert_eq!(m.next_move().unwrap().age, 0.0, "waits on CONTINUE");
            assert_eq!(m.input(Nav::Select), None);
            assert!(m.summary().is_none());
            assert_eq!(m.screen, Screen::Missions, "CONTINUE stays on the screen");
        }
        for _ in 0..1000 {
            m.advance(0.01);
        }
        assert!(m.transfer().is_none());
        assert_eq!(Shown::of(m.shown().unwrap()), Shown::of(&next));

        // With the new one remembered, it doesn't play again.
        let mut m = Menu::new();
        m.input(Nav::Select);
        m.remember(g.shown(&Shown::of(&next).unwrap()).unwrap());
        m.set_theater(Some(next.clone()));
        assert!(m.transfer().is_none());
    }

    /// When a contract ends, its summary card shows before the transfer
    /// animation (#414), and CONTINUE (key or click) starts the animation.
    /// If there's no record of how the contract ended (e.g. it was another
    /// ship's), the animation starts right away.
    #[test]
    fn a_summary_comes_before_the_move() {
        use crate::galaxy::theater::{Ended, Outcome, Own};
        let t = theater(Faction::Imperium).unwrap();
        let old = t.ship.clone().unwrap();
        let mut next = t.clone();
        let ended = Ended {
            ship: old.id,
            signed: old.signed,
            tick: old.signed + 4,
            outcome: Outcome::Spent,
            defend: false,
            committed: 60,
            lost: 58,
            own: Own::default(),
            loot: None,
        };
        {
            let s = next.ship.as_mut().unwrap();
            s.signed += 5;
            s.ended = vec![ended];
        }
        let profile = Profile::in_memory();
        for click in [false, true] {
            let mut m = Menu::new();
            m.input(Nav::Select);
            m.set_theater(Some(t.clone()));
            m.set_theater(Some(next.clone()));
            assert_eq!(m.summary(), Some(&ended));
            // Up/Down do nothing, and the animation doesn't start on its own.
            m.input(Nav::Down);
            m.advance(1.0);
            assert_eq!(m.next_move().unwrap().age, 0.0);
            let layer = m.draw(&profile, 1600.0, 900.0, 1.0);
            assert!(layer.hits.center_of(Hit::Continue).is_some());
            if click {
                assert!(m.hover(Hit::Continue));
                m.click(Hit::Continue, Vec2::ZERO, &profile.settings);
            } else {
                m.input(Nav::Select);
            }
            assert!(m.summary().is_none());
            assert!(m.transfer().is_some(), "the move plays next");
            assert!(m.transfer().unwrap().failed, "and says the contract failed");
            m.advance(0.5);
            assert!(m.transfer().unwrap().age > 0.0);
            let layer = m.draw(&profile, 1600.0, 900.0, 1.0);
            assert!(layer.hits.center_of(Hit::Continue).is_none());
        }

        // No record: the animation starts right away.
        next.ship.as_mut().unwrap().ended.clear();
        let mut m = Menu::new();
        m.input(Nav::Select);
        m.set_theater(Some(t.clone()));
        m.set_theater(Some(next));
        assert!(m.summary().is_none());
        m.advance(0.5);
        assert!(m.transfer().unwrap().age > 0.0);
    }

    /// A contract that ends with no next one yet still gets its card, over
    /// the theater it was in (#501); once it's dismissed, you're waiting
    /// for work, and the ticks you wait are counted.
    #[test]
    fn a_contract_ends_with_nothing_next() {
        use crate::galaxy::theater::{Ended, Outcome, Own};
        let t = theater(Faction::Imperium).unwrap();
        let old = t.ship.clone().unwrap();
        let ended = Ended {
            ship: old.id,
            signed: old.signed,
            tick: old.signed + 4,
            outcome: Outcome::Won,
            defend: false,
            committed: 60,
            lost: 12,
            own: Own::default(),
            loot: None,
        };
        let mut m = Menu::new();
        m.input(Nav::Select);
        m.set_theater(Some(t.clone()));
        assert!(!m.waiting());
        // The war goes on: the contract's over, and nothing's next.
        m.set_ended(&[ended]);
        m.set_theater(None);
        assert_eq!(m.summary(), Some(&ended));
        assert_eq!(
            m.system().map(|s| s.system),
            Some(t.system),
            "over the old theater"
        );
        assert!(!m.waiting(), "not under the summary");
        // No contract: what's next is yours to pick, off the board.
        m.set_job(None);
        m.input(Nav::Select);
        assert!(m.summary().is_none());
        assert_eq!(m.screen, Screen::Ships);
        assert!(m.waiting_for_offers());
        // The main menu's Contract can't be opened, by key or click.
        m.input(Nav::Back);
        for _ in 0..10 {
            m.input(Nav::Down);
            assert_ne!(m.cursor, 0);
        }
        let hits = m.draw(&Profile::in_memory(), 1600.0, 900.0, 1.0).hits;
        assert_eq!(hits.center_of(Hit::Row(0)), None);
        // Signed on: it can, and it's where the war's waited out.
        m.set_job(Some(JobAt {
            system: "TORKAR".into(),
            flying: true,
        }));
        assert!(m.opens(0));
        m.screen = Screen::Missions;
        assert!(m.waiting());
        // It doesn't come up again.
        m.set_ended(&[ended]);
        assert!(m.summary().is_none());
        // Work: the move to it plays, and the count starts over.
        let mut next = t.clone();
        next.ship.as_mut().unwrap().signed += 5;
        m.set_theater(Some(next));
        assert!(!m.waiting());
        assert!(m.transfer().is_some());
    }
    /// Deploying while a transfer plays: you've seen the new contract, so
    /// it doesn't play again after the battle.
    #[test]
    fn deploying_settles_a_transfer() {
        let mut m = Menu::new();
        m.input(Nav::Select);
        let t = theater(Faction::Imperium).unwrap();
        m.set_theater(Some(t.clone()));
        let mut next = t.clone();
        next.ship.as_mut().unwrap().signed += 5;
        m.set_theater(Some(next.clone()));
        assert!(m.transfer().is_some());
        assert_eq!(
            m.shown().unwrap().ship.as_ref().unwrap().signed,
            t.ship.as_ref().unwrap().signed
        );
        m.settle();
        assert!(m.transfer().is_none());
        assert_eq!(
            m.shown().unwrap().ship.as_ref().unwrap().signed,
            next.ship.as_ref().unwrap().signed
        );
    }
}
