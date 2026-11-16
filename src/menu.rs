//! Main menu, mission select, settings and the pause menu, in the hairline style.
//!
//! The main menu and mission select sit in the same left-hand panel as the
//! garage, over the first mission's map with your tank parked in it. The
//! pause menu is a small centred box over the frozen battle.

use glam::Vec2;

use crate::faction::Faction;
use crate::garage::{self, Loadout, PANEL_W, PANEL_X};
use crate::mission::MISSIONS;
use crate::profile::{Profile, Settings};
use crate::render::UiVertex;
use crate::ui::widgets::{self, label};
use crate::ui::{Align, Painter, TextStyle, theme};

const REF_HEIGHT: f32 = 900.0;

/// Top of the main panel's list, below the header and pilot block.
const LIST_Y: f32 = 72.0 + 18.0 + 52.0 + PILOT_H + 26.0;
/// Row pitch of `rows` lists and of the settings rows.
const ROW_PITCH: f32 = 30.0;
const SETTING_PITCH: f32 = 44.0;
const DETAIL_PITCH: f32 = 22.0;
/// The pause box's size, and how far down it its list starts.
const PAUSE_SIZE: Vec2 = Vec2::new(PANEL_W + 48.0, 350.0);
const PAUSE_LIST: f32 = 28.0 + 18.0 + 44.0;

/// Logical px (the 900 px reference screen) from window pixels.
pub fn logical(px: Vec2, height: f32) -> Vec2 {
    px * (REF_HEIGHT / height)
}

/// Which of `n` rows starting at `top`, `pitch` apart, a list at `x` has under `at`.
fn row_at(at: Vec2, x: f32, top: f32, pitch: f32, n: usize) -> Option<usize> {
    if at.x < x - 20.0 || at.x > x + PANEL_W + 10.0 {
        return None;
    }
    let i = ((at.y - top + 6.0) / pitch).floor();
    (i >= 0.0 && (i as usize) < n).then_some(i as usize)
}

/// Volume steps for a click at `at.x` on the volume bar starting at `x`.
fn volume_at(at: Vec2, x: f32) -> u8 {
    let f = ((at.x - x) / PANEL_W).clamp(0.0, 1.0);
    (f * Settings::MAX_VOLUME as f32).round() as u8
}

/// A click on a menu: the key it stands in for (for the sound) and what it did.
pub type Click<A> = (Nav, Option<A>);

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
    Missions,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// Start mission `MISSIONS[i]`, hired by its side `.1` (0 or 1).
    Deploy(usize, usize),
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
}

const MAIN_ITEMS: [&str; 4] = ["MISSIONS", "GARAGE", "SETTINGS", "QUIT"];

/// Picks one toggle out of the settings.
type Flag = fn(&mut Settings) -> &mut bool;

/// The settings panel's toggles: name, a line on what it does, and where it lives.
pub const SETTINGS: [(&str, &str, Flag); 1] =
    [("DEBUG MODE", "FPS COUNTER IN THE TOP RIGHT CORNER", |s| {
        &mut s.debug
    })];

/// Rows on the settings panel: the toggles, then the volume.
const SETTING_ROWS: usize = SETTINGS.len() + 1;
const VOLUME_ROW: usize = SETTINGS.len();

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
    }
}

/// What a key does on settings row `i`.
fn setting_input(i: usize, nav: Nav) -> Option<Change> {
    match (i == VOLUME_ROW, nav) {
        (true, Nav::Left) => Some(Change::Volume(-1)),
        (true, Nav::Right) => Some(Change::Volume(1)),
        (true, Nav::Select) => Some(Change::CycleVolume),
        (false, Nav::Select) => Some(Change::Toggle(i)),
        _ => None,
    }
}

pub struct Menu {
    pub screen: Screen,
    cursor: usize,
    mission: usize,
    /// Which of the mission's sides you're taking the contract from.
    side: usize,
    setting: usize,
}

impl Menu {
    pub fn new() -> Menu {
        Menu {
            screen: Screen::Main,
            cursor: 0,
            mission: 0,
            side: 0,
            setting: 0,
        }
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
            (Screen::Missions, Nav::Up | Nav::Down) => {
                self.mission = step(self.mission, MISSIONS.len(), nav)
            }
            (Screen::Missions, Nav::Left | Nav::Right) => self.side = 1 - self.side,
            (Screen::Missions, Nav::Select) => {
                return Some(Action::Deploy(self.mission, self.side));
            }
            (Screen::Missions, Nav::Back) => self.screen = Screen::Main,
            (Screen::Settings, Nav::Up | Nav::Down) => {
                self.setting = step(self.setting, SETTING_ROWS, nav)
            }
            (Screen::Settings, Nav::Back) => self.screen = Screen::Main,
            (Screen::Settings, _) => return setting_input(self.setting, nav).map(Action::Setting),
        }
        None
    }

    /// What's under `at` (logical px): a row of the current list, or on the
    /// mission screen the employer line.
    fn target(&self, at: Vec2) -> Option<Target> {
        let x = PANEL_X;
        match self.screen {
            Screen::Main => row_at(at, x, LIST_Y, ROW_PITCH, MAIN_ITEMS.len()).map(Target::Row),
            Screen::Missions => {
                let top = LIST_Y + 20.0;
                if let Some(i) = row_at(at, x, top, ROW_PITCH, MISSIONS.len()) {
                    return Some(Target::Row(i));
                }
                let details = top + MISSIONS.len() as f32 * ROW_PITCH + 14.0;
                (row_at(at, x, details, DETAIL_PITCH, 3) == Some(2)).then_some(Target::Employer)
            }
            Screen::Settings => {
                row_at(at, x, LIST_Y + 20.0, SETTING_PITCH, SETTING_ROWS).map(Target::Row)
            }
        }
    }

    /// Moves the selection to the row under the pointer; true if it changed.
    pub fn hover(&mut self, at: Vec2) -> bool {
        let Some(Target::Row(i)) = self.target(at) else {
            return false;
        };
        let slot = match self.screen {
            Screen::Main => &mut self.cursor,
            Screen::Missions => &mut self.mission,
            Screen::Settings => &mut self.setting,
        };
        std::mem::replace(slot, i) != i
    }

    /// A left click at `at`; `None` if it missed everything.
    pub fn click(&mut self, at: Vec2, settings: &Settings) -> Option<Click<Action>> {
        let target = self.target(at)?;
        self.hover(at);
        let nav = match target {
            Target::Employer => Nav::Right,
            Target::Row(VOLUME_ROW) if self.screen == Screen::Settings => {
                let d = volume_at(at, PANEL_X) as i32 - settings.volume as i32;
                return Some((Nav::Right, Some(Action::Setting(Change::Volume(d)))));
            }
            Target::Row(_) => Nav::Select,
        };
        Some((nav, self.input(nav)))
    }

    /// The panel; `alpha` fades it (out while the garage is open over it).
    pub fn draw(&self, profile: &Profile, height: f32, alpha: f32) -> Vec<UiVertex> {
        if alpha <= 0.0 {
            return Vec::new();
        }
        let scale = height / REF_HEIGHT;
        let h = REF_HEIGHT;
        let mut p = Painter::new(scale);
        garage::backdrop(&mut p, h);
        let x = PANEL_X;
        let mut y = 72.0;

        // Header: the game, then the pilot.
        label(&mut p, Vec2::new(x, y), "MERCENARY LINK", Align::Left);
        label(&mut p, Vec2::new(x + PANEL_W, y), "ONLINE", Align::Right);
        y += 18.0;
        p.text(
            Vec2::new(x, y),
            "TANKIT",
            &TextStyle::value(28.0),
            Align::Left,
        );
        y += 52.0;
        pilot(&mut p, Vec2::new(x, y), profile);
        y += PILOT_H;
        p.line(
            Vec2::new(x, y),
            Vec2::new(x + PANEL_W, y),
            1.0,
            theme::FAINT,
        );
        y += 26.0;
        debug_assert_eq!(y, LIST_Y);

        match self.screen {
            Screen::Main => {
                rows(&mut p, Vec2::new(x, y), &MAIN_ITEMS, self.cursor);
                y += MAIN_ITEMS.len() as f32 * 30.0 + 20.0;
                label(&mut p, Vec2::new(x, y), "DEPLOYING IN", Align::Left);
                y += 16.0;
                p.text(
                    Vec2::new(x, y),
                    &kit_name(profile.loadout),
                    &TextStyle::value(12.0),
                    Align::Left,
                );
                hints(&mut p, x, h, &[("W/S", "SELECT"), ("ENTER", "OPEN")]);
            }
            Screen::Missions => {
                label(&mut p, Vec2::new(x, y), "01  MISSIONS", Align::Left);
                label(
                    &mut p,
                    Vec2::new(x + PANEL_W, y),
                    &format!("{}/{}", self.mission + 1, MISSIONS.len()),
                    Align::Right,
                );
                y += 20.0;
                let names: Vec<&str> = MISSIONS.iter().map(|m| m.name).collect();
                rows(&mut p, Vec2::new(x, y), &names, self.mission);
                y += names.len() as f32 * ROW_PITCH + 14.0;

                let m = &MISSIONS[self.mission];
                let (employer, against) = (m.sides[self.side], m.sides[1 - self.side]);
                let s = profile.standing(employer);
                for (k, v) in [
                    ("CONTRACT", m.contract.name().to_string()),
                    ("MAP", m.map.to_string()),
                    ("EMPLOYER", format!("< {} >", employer.name())),
                    ("AGAINST", against.name().to_string()),
                    (
                        "THEIR TRUST",
                        format!("T{}  {} / {}", s.level, s.into, s.span),
                    ),
                    ("TEAMS", m.teams()),
                    ("ROUND", m.round_time()),
                    ("PAY", m.pay()),
                ] {
                    label(&mut p, Vec2::new(x, y + 1.0), k, Align::Left);
                    p.text(
                        Vec2::new(x + PANEL_W, y),
                        &v,
                        &TextStyle::value(11.0),
                        Align::Right,
                    );
                    y += DETAIL_PITCH;
                }
                y += 8.0;
                label(&mut p, Vec2::new(x, y), m.brief, Align::Left);
                hints(
                    &mut p,
                    x,
                    h,
                    &[
                        ("W/S", "SELECT"),
                        ("A/D", "EMPLOYER"),
                        ("ENTER", "DEPLOY"),
                        ("ESC", "BACK"),
                    ],
                );
            }
            Screen::Settings => {
                label(&mut p, Vec2::new(x, y), "03  SETTINGS", Align::Left);
                y += 20.0;
                settings_rows(&mut p, Vec2::new(x, y), &profile.settings, self.setting);
                hints(
                    &mut p,
                    x,
                    h,
                    &[
                        ("W/S", "SELECT"),
                        ("ENTER", "TOGGLE"),
                        ("A/D", "VOLUME"),
                        ("ESC", "BACK"),
                    ],
                );
            }
        }
        fade(p.finish(), alpha)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Row(usize),
    Employer,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PauseAction {
    Resume,
    Setting(Change),
    ExitToMenu,
    Quit,
}

const PAUSE_ITEMS: [&str; 4] = ["RESUME", "SETTINGS", "EXIT TO MENU", "QUIT"];

pub struct Pause {
    cursor: usize,
    /// The settings list is open in the box, on this toggle.
    settings: Option<usize>,
}

impl Pause {
    pub fn new() -> Pause {
        Pause {
            cursor: 0,
            settings: None,
        }
    }

    pub fn input(&mut self, nav: Nav) -> Option<PauseAction> {
        if let Some(i) = &mut self.settings {
            match nav {
                Nav::Up | Nav::Down => *i = step(*i, SETTING_ROWS, nav),
                Nav::Back => self.settings = None,
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
                1 => self.settings = Some(0),
                2 => return Some(PauseAction::ExitToMenu),
                _ => return Some(PauseAction::Quit),
            },
        }
        None
    }

    /// The pause box's list origin on a `width` logical px wide screen.
    fn list_origin(width: f32) -> Vec2 {
        let min = Vec2::new(width, REF_HEIGHT) * 0.5 - PAUSE_SIZE * 0.5;
        min + Vec2::new(24.0, PAUSE_LIST)
    }

    fn target(&self, at: Vec2, width: f32) -> Option<usize> {
        let o = Self::list_origin(width);
        match self.settings {
            Some(_) => row_at(at, o.x, o.y, SETTING_PITCH, SETTING_ROWS),
            None => row_at(at, o.x, o.y, ROW_PITCH, PAUSE_ITEMS.len()),
        }
    }

    /// Moves the selection to the row under the pointer; true if it changed.
    pub fn hover(&mut self, at: Vec2, width: f32) -> bool {
        let Some(i) = self.target(at, width) else {
            return false;
        };
        let slot = self.settings.as_mut().unwrap_or(&mut self.cursor);
        std::mem::replace(slot, i) != i
    }

    /// A left click at `at` on a `width` logical px wide screen; `None` if it missed.
    pub fn click(
        &mut self,
        at: Vec2,
        width: f32,
        settings: &Settings,
    ) -> Option<Click<PauseAction>> {
        let i = self.target(at, width)?;
        self.hover(at, width);
        if self.settings.is_some() && i == VOLUME_ROW {
            let x = Self::list_origin(width).x;
            let d = volume_at(at, x) as i32 - settings.volume as i32;
            return Some((Nav::Right, Some(PauseAction::Setting(Change::Volume(d)))));
        }
        Some((Nav::Select, self.input(Nav::Select)))
    }

    /// Dims the battle and draws the box. `match_trust` is what this match has
    /// earned you with `employer` so far.
    pub fn draw(
        &self,
        profile: &Profile,
        employer: Faction,
        match_trust: u32,
        width: f32,
        height: f32,
    ) -> Vec<UiVertex> {
        let scale = height / REF_HEIGHT;
        let (w, h) = (width / scale, REF_HEIGHT);
        let mut p = Painter::new(scale);
        p.rect(Vec2::ZERO, Vec2::new(w, h), [0.0, 0.0, 0.0, 0.45]);
        let size = PAUSE_SIZE;
        let min = Vec2::new(w, h) * 0.5 - size * 0.5;
        p.rect(min, size, [0.03, 0.03, 0.035, 0.9]);
        widgets::corners(&mut p, min, min + size, 14.0, theme::DIM);

        let x = min.x + 24.0;
        let mut y = min.y + 28.0;
        label(&mut p, Vec2::new(x, y), "LINK HELD", Align::Left);
        y += 18.0;
        let title = if self.settings.is_some() {
            "SETTINGS"
        } else {
            "PAUSED"
        };
        p.text(Vec2::new(x, y), title, &TextStyle::value(20.0), Align::Left);
        y += 44.0;
        debug_assert_eq!(Vec2::new(x, y), Self::list_origin(w));
        if let Some(i) = self.settings {
            settings_rows(&mut p, Vec2::new(x, y), &profile.settings, i);
            label(
                &mut p,
                Vec2::new(x, min.y + size.y - 34.0),
                "ENTER TOGGLE    A/D VOLUME    ESC BACK",
                Align::Left,
            );
            return p.finish();
        }
        rows(&mut p, Vec2::new(x, y), &PAUSE_ITEMS, self.cursor);
        y += PAUSE_ITEMS.len() as f32 * 30.0 + 8.0;
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
            &format!("+{match_trust} TRUST"),
            &TextStyle::value(11.0),
            Align::Right,
        );
        y += 20.0;
        let r = profile.standing(employer);
        label(
            &mut p,
            Vec2::new(x, y + 1.0),
            &format!("{} T{}", employer.name(), r.level),
            Align::Left,
        );
        widgets::hairbar(
            &mut p,
            Vec2::new(x + 110.0, y + 5.0),
            PANEL_W - 110.0,
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
        p.finish()
    }
}

/// The settings toggles: each a row with a checkbox, and under it what it does.
fn settings_rows(p: &mut Painter, at: Vec2, settings: &Settings, selected: usize) {
    for (i, (name, what, flag)) in SETTINGS.iter().enumerate() {
        let y = at.y + i as f32 * SETTING_PITCH;
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
    let y = at.y + VOLUME_ROW as f32 * SETTING_PITCH;
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
}

/// Height of the pilot block.
const PILOT_H: f32 = 18.0 + Faction::ALL.len() as f32 * ROW_H + 40.0;
const ROW_H: f32 = 20.0;

/// Each faction's trust in you (standing and the bar to the next), then crystals.
fn pilot(p: &mut Painter, at: Vec2, profile: &Profile) {
    label(p, at, "TRUST", Align::Left);
    label(p, at + Vec2::new(PANEL_W, 0.0), "STANDING", Align::Right);
    let mut y = at.y + 18.0;
    for f in Faction::ALL {
        let s = profile.standing(f);
        let mut name = TextStyle::value(11.0);
        name.color = f.ui();
        p.text(Vec2::new(at.x, y), f.name(), &name, Align::Left);
        widgets::hairbar(
            p,
            Vec2::new(at.x + 110.0, y + 6.0),
            PANEL_W - 110.0 - 36.0,
            s.progress(),
            if profile.trust_of(f) > 0 {
                theme::FG
            } else {
                theme::FAINT
            },
        );
        p.text(
            Vec2::new(at.x + PANEL_W, y),
            &format!("T{}", s.level),
            &TextStyle::value(11.0),
            Align::Right,
        );
        y += ROW_H;
    }
    let cy = y + 10.0 - at.y;
    label(p, at + Vec2::new(0.0, cy + 2.0), "CRYSTALS", Align::Left);
    let cw = label(p, at + Vec2::new(PANEL_W, cy + 2.0), "CR", Align::Right);
    p.text(
        at + Vec2::new(PANEL_W - cw - 8.0, cy),
        &profile.crystals.to_string(),
        &TextStyle::value(12.0),
        Align::Right,
    );
}

/// A selectable list, with the garage's bar-and-chevron marker on the selected row.
fn rows(p: &mut Painter, at: Vec2, items: &[&str], selected: usize) {
    for (i, item) in items.iter().enumerate() {
        let y = at.y + i as f32 * ROW_PITCH;
        let on = i == selected;
        if on {
            p.line(
                Vec2::new(at.x - 14.0, y - 3.0),
                Vec2::new(at.x - 14.0, y + 18.0),
                2.0,
                theme::FG,
            );
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
        let mut s = TextStyle::value(15.0);
        s.color = if on { theme::FG } else { theme::DIM };
        p.text(Vec2::new(at.x, y), item, &s, Align::Left);
    }
}

fn hints(p: &mut Painter, x: f32, h: f32, keys: &[(&str, &str)]) {
    let mut y = h - 100.0;
    for (key, what) in keys {
        let mut k = TextStyle::LABEL;
        k.color = theme::FG;
        p.text(Vec2::new(x, y), key, &k, Align::Left);
        label(p, Vec2::new(x + 70.0, y), what, Align::Left);
        y += 14.0;
    }
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
        // Wraps round to MISSIONS, which opens the mission list.
        m.input(Nav::Down);
        assert_eq!(m.input(Nav::Select), None);
        assert_eq!(m.screen, Screen::Missions);
        assert_eq!(m.input(Nav::Select), Some(Action::Deploy(0, 0)));
        // A/D takes the contract from the other side instead.
        assert_eq!(m.input(Nav::Right), None);
        assert_eq!(m.input(Nav::Select), Some(Action::Deploy(0, 1)));
        m.input(Nav::Left);
        assert_eq!(m.input(Nav::Select), Some(Action::Deploy(0, 0)));
        m.input(Nav::Back);
        assert_eq!(m.screen, Screen::Main);
    }

    #[test]
    fn pause_navigation() {
        let mut p = Pause::new();
        assert_eq!(p.input(Nav::Back), Some(PauseAction::Resume));
        assert_eq!(p.input(Nav::Select), Some(PauseAction::Resume));
        p.input(Nav::Down);
        assert_eq!(p.input(Nav::Select), None);
        assert_eq!(
            p.input(Nav::Select),
            Some(PauseAction::Setting(Change::Toggle(0)))
        );
        assert_eq!(p.input(Nav::Left), None);
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
        let profile = Profile::in_memory();
        let mut m = Menu::new();
        let mut settings = Pause::new();
        settings.input(Nav::Down);
        settings.input(Nav::Select);
        for screen in [Screen::Main, Screen::Missions, Screen::Settings] {
            m.screen = screen;
            for (w, h) in [(1600.0, 900.0), (1024.0, 768.0), (3840.0, 2160.0)] {
                for v in [
                    m.draw(&profile, h, 1.0),
                    Pause::new().draw(&profile, Faction::Imperium, 30, w, h),
                    settings.draw(&profile, Faction::Imperium, 30, w, h),
                ] {
                    assert!(!v.is_empty());
                    assert!(v.iter().all(|v| v.pos[0] > -4.0
                        && v.pos[0] < w + 4.0
                        && v.pos[1] > -4.0
                        && v.pos[1] < h + 4.0));
                }
            }
        }
        assert!(m.draw(&profile, 900.0, 0.0).is_empty());
    }

    #[test]
    fn clicking_rows() {
        let settings = Settings::default();
        let mut m = Menu::new();
        let row = |i: usize| Vec2::new(PANEL_X + 40.0, LIST_Y + i as f32 * ROW_PITCH + 8.0);
        // Off the panel: nothing.
        assert_eq!(m.click(Vec2::new(900.0, 400.0), &settings), None);
        assert!(m.hover(row(3)));
        assert!(!m.hover(row(3)));
        assert_eq!(
            m.click(row(1), &settings),
            Some((Nav::Select, Some(Action::Garage)))
        );
        assert_eq!(m.click(row(0), &settings), Some((Nav::Select, None)));
        assert_eq!(m.screen, Screen::Missions);
        // The employer line flips sides.
        let employer =
            LIST_Y + 20.0 + MISSIONS.len() as f32 * ROW_PITCH + 14.0 + 2.0 * DETAIL_PITCH;
        m.click(Vec2::new(PANEL_X + 200.0, employer + 6.0), &settings);
        let mission = Vec2::new(PANEL_X + 40.0, LIST_Y + 20.0 + 8.0);
        assert_eq!(
            m.click(mission, &settings),
            Some((Nav::Select, Some(Action::Deploy(0, 1))))
        );

        let mut m = Menu::new();
        m.screen = Screen::Settings;
        let top = LIST_Y + 20.0;
        assert_eq!(
            m.click(Vec2::new(PANEL_X + 40.0, top + 8.0), &settings)
                .unwrap()
                .1,
            Some(Action::Setting(Change::Toggle(0)))
        );
        // Clicking the far end of the volume bar turns it all the way up.
        let bar = Vec2::new(
            PANEL_X + PANEL_W,
            top + SETTING_PITCH * VOLUME_ROW as f32 + 26.0,
        );
        let up = Settings::MAX_VOLUME as i32 - settings.volume as i32;
        assert_eq!(
            m.click(bar, &settings).unwrap().1,
            Some(Action::Setting(Change::Volume(up)))
        );
    }

    #[test]
    fn clicking_pause_rows() {
        let settings = Settings::default();
        let w = 1600.0;
        let o = Pause::list_origin(w);
        let row = |i: usize| o + Vec2::new(40.0, i as f32 * ROW_PITCH + 8.0);
        let mut p = Pause::new();
        assert_eq!(p.click(Vec2::ZERO, w, &settings), None);
        assert_eq!(
            p.click(row(2), w, &settings),
            Some((Nav::Select, Some(PauseAction::ExitToMenu)))
        );
        assert_eq!(p.click(row(1), w, &settings), Some((Nav::Select, None)));
        assert_eq!(
            p.click(row(0), w, &settings).unwrap().1,
            Some(PauseAction::Setting(Change::Toggle(0)))
        );
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
}
