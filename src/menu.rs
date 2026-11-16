//! Main menu, mission select and the pause menu, in the hairline style.
//!
//! The main menu and mission select sit in the same left-hand panel as the
//! garage, over the first mission's map with your tank parked in it. The
//! pause menu is a small centred box over the frozen battle.

use glam::Vec2;

use crate::garage::{self, Loadout, PANEL_W, PANEL_X};
use crate::mission::MISSIONS;
use crate::profile::Profile;
use crate::render::UiVertex;
use crate::ui::widgets::{self, label};
use crate::ui::{Align, Painter, TextStyle, theme};

const REF_HEIGHT: f32 = 900.0;

/// Navigation input, mapped from keys by the app.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Nav {
    Up,
    Down,
    Select,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Screen {
    Main,
    Missions,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    /// Start mission `MISSIONS[i]`.
    Deploy(usize),
    Garage,
    Quit,
}

const MAIN_ITEMS: [&str; 3] = ["MISSIONS", "GARAGE", "QUIT"];

pub struct Menu {
    pub screen: Screen,
    cursor: usize,
    mission: usize,
}

impl Menu {
    pub fn new() -> Menu {
        Menu {
            screen: Screen::Main,
            cursor: 0,
            mission: 0,
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
                _ => return Some(Action::Quit),
            },
            (Screen::Main, Nav::Back) => {}
            (Screen::Missions, Nav::Up | Nav::Down) => {
                self.mission = step(self.mission, MISSIONS.len(), nav)
            }
            (Screen::Missions, Nav::Select) => return Some(Action::Deploy(self.mission)),
            (Screen::Missions, Nav::Back) => self.screen = Screen::Main,
        }
        None
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
        y += 64.0;
        p.line(
            Vec2::new(x, y),
            Vec2::new(x + PANEL_W, y),
            1.0,
            theme::FAINT,
        );
        y += 26.0;

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
                y += names.len() as f32 * 30.0 + 14.0;

                let m = &MISSIONS[self.mission];
                for (k, v) in [
                    ("CONTRACT", m.mode.to_string()),
                    ("MAP", m.map.to_string()),
                    ("EMPLOYER", m.sides[0].name().to_string()),
                    ("AGAINST", m.sides[1].name().to_string()),
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
                    y += 22.0;
                }
                y += 8.0;
                label(&mut p, Vec2::new(x, y), m.brief, Align::Left);
                hints(
                    &mut p,
                    x,
                    h,
                    &[("W/S", "SELECT"), ("ENTER", "DEPLOY"), ("ESC", "BACK")],
                );
            }
        }
        fade(p.finish(), alpha)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PauseAction {
    Resume,
    ExitToMenu,
    Quit,
}

const PAUSE_ITEMS: [&str; 3] = ["RESUME", "EXIT TO MENU", "QUIT"];

pub struct Pause {
    cursor: usize,
}

impl Pause {
    pub fn new() -> Pause {
        Pause { cursor: 0 }
    }

    pub fn input(&mut self, nav: Nav) -> Option<PauseAction> {
        match nav {
            Nav::Up | Nav::Down => self.cursor = step(self.cursor, PAUSE_ITEMS.len(), nav),
            Nav::Back => return Some(PauseAction::Resume),
            Nav::Select => {
                return Some(
                    [
                        PauseAction::Resume,
                        PauseAction::ExitToMenu,
                        PauseAction::Quit,
                    ][self.cursor],
                );
            }
        }
        None
    }

    /// Dims the battle and draws the box. `match_xp` is what this match has paid so far.
    pub fn draw(&self, profile: &Profile, match_xp: u32, width: f32, height: f32) -> Vec<UiVertex> {
        let scale = height / REF_HEIGHT;
        let (w, h) = (width / scale, REF_HEIGHT);
        let mut p = Painter::new(scale);
        p.rect(Vec2::ZERO, Vec2::new(w, h), [0.0, 0.0, 0.0, 0.45]);
        let size = Vec2::new(PANEL_W + 48.0, 300.0);
        let min = Vec2::new(w, h) * 0.5 - size * 0.5;
        p.rect(min, size, [0.03, 0.03, 0.035, 0.9]);
        widgets::corners(&mut p, min, min + size, 14.0, theme::DIM);

        let x = min.x + 24.0;
        let mut y = min.y + 28.0;
        label(&mut p, Vec2::new(x, y), "LINK HELD", Align::Left);
        y += 18.0;
        p.text(
            Vec2::new(x, y),
            "PAUSED",
            &TextStyle::value(20.0),
            Align::Left,
        );
        y += 44.0;
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
            &format!("+{match_xp} XP"),
            &TextStyle::value(11.0),
            Align::Right,
        );
        y += 20.0;
        let r = profile.rank();
        label(
            &mut p,
            Vec2::new(x, y + 1.0),
            &format!("RANK {}", r.rank),
            Align::Left,
        );
        widgets::hairbar(
            &mut p,
            Vec2::new(x + 110.0, y + 5.0),
            PANEL_W - 110.0,
            r.progress(),
            theme::FG,
        );
        p.finish()
    }
}

/// Pilot rank, XP bar and totals.
fn pilot(p: &mut Painter, at: Vec2, profile: &Profile) {
    let r = profile.rank();
    label(p, at, "PILOT RANK", Align::Left);
    label(
        p,
        at + Vec2::new(PANEL_W, 0.0),
        &format!("{} XP", profile.xp),
        Align::Right,
    );
    p.text(
        at + Vec2::new(0.0, 16.0),
        &r.rank.to_string(),
        &TextStyle::value(20.0),
        Align::Left,
    );
    let bar = at + Vec2::new(56.0, 28.0);
    widgets::hairbar(p, bar, PANEL_W - 56.0, r.progress(), theme::FG);
    label(
        p,
        at + Vec2::new(PANEL_W, 38.0),
        &format!("{} / {} TO RANK {}", r.into, r.span, r.rank + 1),
        Align::Right,
    );
}

/// A selectable list, with the garage's bar-and-chevron marker on the selected row.
fn rows(p: &mut Painter, at: Vec2, items: &[&str], selected: usize) {
    for (i, item) in items.iter().enumerate() {
        let y = at.y + i as f32 * 30.0;
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
        assert_eq!(m.input(Nav::Select), Some(Action::Quit));
        // Wraps round to MISSIONS, which opens the mission list.
        m.input(Nav::Down);
        assert_eq!(m.input(Nav::Select), None);
        assert_eq!(m.screen, Screen::Missions);
        assert_eq!(m.input(Nav::Select), Some(Action::Deploy(0)));
        m.input(Nav::Back);
        assert_eq!(m.screen, Screen::Main);
    }

    #[test]
    fn pause_navigation() {
        let mut p = Pause::new();
        assert_eq!(p.input(Nav::Back), Some(PauseAction::Resume));
        assert_eq!(p.input(Nav::Select), Some(PauseAction::Resume));
        p.input(Nav::Down);
        assert_eq!(p.input(Nav::Select), Some(PauseAction::ExitToMenu));
        p.input(Nav::Down);
        assert_eq!(p.input(Nav::Select), Some(PauseAction::Quit));
    }

    #[test]
    fn draws_on_screen_at_any_size() {
        let profile = Profile::in_memory();
        let mut m = Menu::new();
        for screen in [Screen::Main, Screen::Missions] {
            m.screen = screen;
            for (w, h) in [(1600.0, 900.0), (1024.0, 768.0), (3840.0, 2160.0)] {
                for v in [
                    m.draw(&profile, h, 1.0),
                    Pause::new().draw(&profile, 30, w, h),
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
}
