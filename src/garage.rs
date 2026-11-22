//! Garage: from the main menu, or G mid-match to park and swing the camera
//! round your tank. Pick the hull and turret for your next respawn, and
//! upgrade parts with crystals. The tank you're driving doesn't change. Parts
//! you don't have yet say which mission's win steals their schematic.
//!
//! Hulls and turrets each have a tab: a scrolling grid of cards with rendered
//! thumbnails, grouped by maker. Browsing only previews: the orbit camera
//! shows the highlighted kit (parts you don't own too), and nothing is
//! equipped until you press E (equip) on a part. U upgrades the
//! highlighted part. Deploying keeps what you equipped, cancelling puts back
//! what was equipped when you came in.
//!
//! Drawn as a solid panel on the left, the highlighted part's stats in a card
//! top right, the tank framed in the space between.
//!
//! It opens in the ship's bay (`bay`): the camera swings round the tank where
//! it stands, the screen dips to black, and comes up on the tank turning on
//! the bay's turntable. Closing runs the same way back.

use glam::{Vec2, Vec3};

use crate::economy::{Levels, Part};
use crate::faction::Faction;
use crate::profile::{Profile, Refusal};
use crate::render::{Camera, UiVertex};
use crate::sim::{Chassis, HullKind, HullStats, Tank, TurretKind, World, wrap_angle};
use crate::thumbs::Thumbs;
use crate::ui::emblem;
use crate::ui::widgets::{self, label};
use crate::ui::{Align, Hits, Layer, Painter, Rgba, TextStyle, theme, with_alpha};

const REF_HEIGHT: f32 = 900.0;
pub const PANEL_X: f32 = 64.0;
pub const PANEL_W: f32 = 340.0;
/// The solid panel's margin around the content, and its gap from the screen edges.
const PANEL_PAD: f32 = 28.0;
const PANEL_TOP: f32 = 32.0;
/// The garage panel's content width (wider than the menu's, for the card grid).
const GRID_W: f32 = 420.0;
const TABS_TOP: f32 = 140.0;
const TAB_H: f32 = 28.0;
/// The card grid's viewport: where it starts and how tall it is.
const GRID_TOP: f32 = 182.0;
const GRID_VIEW: f32 = 552.0;
/// Two across: no maker has more than two parts of a kind, so a third
/// column only ever stood empty.
const GRID_COLS: usize = 2;
const CARD_GAP: f32 = 10.0;
const CARD_W: f32 = (GRID_W - CARD_GAP * (GRID_COLS as f32 - 1.0)) / GRID_COLS as f32;
const THUMB_H: f32 = 68.0;
/// Under the thumbnail: the part's name, and under it whether it's
/// equipped or owned (a locked part has only its padlock).
const STRIP_H: f32 = 38.0;
const CARD_H: f32 = THUMB_H + STRIP_H;
/// The card's text inset from its edges.
const CARD_INSET: f32 = 8.0;
const NAME_SIZE: f32 = 11.0;
const CARD_PITCH: f32 = CARD_H + 8.0;
/// A maker's header above its group of cards.
const HEAD_H: f32 = 22.0;
const BUTTONS_TOP: f32 = 746.0;
const BUTTON_H: f32 = 30.0;
/// The stats card's content width.
const STATS_W: f32 = 300.0;
/// Opaque so the text reads over any map (see `theme::PANEL` for why not nearly).
const PANEL_FILL: Rgba = [0.006, 0.007, 0.009, 1.0];
const CARD_FILL: Rgba = [0.94, 0.91, 0.86, 0.035];
const SELECT_FILL: Rgba = [0.94, 0.91, 0.86, 0.16];
const HOVER_FILL: Rgba = [0.94, 0.91, 0.86, 0.07];
/// What you'll deploy with.
const EQUIPPED: Rgba = [0.52, 0.86, 0.66, 1.0];
/// Parts whose schematic you haven't stolen yet: the padlock.
const LOCKED: Rgba = [0.86, 0.47, 0.38, 0.85];
/// Open/close transition speed (1 / seconds).
const BLEND_RATE: f32 = 2.5;
/// Orbit camera: distance, height and slow drift around the tank (rad/s).
const ORBIT_DIST: f32 = 8.5;
const ORBIT_HEIGHT: f32 = 2.6;
const ORBIT_DRIFT: f32 = 0.12;
/// The bay's turntable (rad/s).
pub const SPIN_RATE: f32 = 0.16;
/// Past this much open, the bay is shown instead of the battlefield.
const BAY_AT: f32 = 0.5;
/// The longest hull (track end to track end, m) the bay camera frames
/// without standing back.
const BAY_FRAMED_LENGTH: f32 = 6.2;

pub type Loadout = (HullKind, TurretKind);

pub const HULLS: [HullKind; 5] = [
    HullKind::Kataphrakt,
    HullKind::Kastron,
    HullKind::Sethuun,
    HullKind::Vigil,
    HullKind::Gravis,
];
pub const TURRETS: [TurretKind; 7] = TurretKind::ALL;

pub fn hull_name(k: HullKind) -> &'static str {
    match k {
        HullKind::Kataphrakt => "KATAPHRAKT",
        HullKind::Kastron => "KASTRON",
        HullKind::Sethuun => "SETHUUN",
        HullKind::Vigil => "VIGIL",
        HullKind::Gravis => "GRAVIS",
    }
}

pub fn turret_name(k: TurretKind) -> &'static str {
    match k {
        TurretKind::Kontos => "KONTOS",
        TurretKind::Pelekys => "PELEKYS",
        TurretKind::Ammeth => "AMMETH",
        TurretKind::Ossrin => "OSSRIN",
        TurretKind::Lumen => "LUMEN",
        TurretKind::Hymn => "HYMN",
        TurretKind::Knell => "KNELL",
    }
}

/// One stat row: label, value, unit. Bars are scaled to the best in the kit.
struct Stat {
    name: &'static str,
    unit: &'static str,
    decimals: usize,
    /// The stat for a part at the given upgrade levels.
    get: fn(Part, &Levels) -> f32,
}

fn hull_of(p: Part) -> HullKind {
    match p {
        Part::Hull(k) => k,
        Part::Turret(_) => unreachable!(),
    }
}

fn turret_of(p: Part) -> TurretKind {
    match p {
        Part::Turret(k) => k,
        Part::Hull(_) => unreachable!(),
    }
}

const HULL_STATS: [Stat; 7] = [
    Stat {
        name: "ARMOR",
        unit: "HP",
        decimals: 0,
        get: |p, l| l.max_health(hull_of(p)),
    },
    Stat {
        name: "TOP SPEED",
        unit: "KM/H",
        decimals: 0,
        get: |p, l| l.hull(hull_of(p)).max_speed * 3.6,
    },
    Stat {
        name: "ACCEL",
        unit: "M/S2",
        decimals: 1,
        get: |p, l| l.hull(hull_of(p)).accel,
    },
    Stat {
        name: "TURN",
        unit: "°/S",
        decimals: 0,
        get: |p, l| l.hull(hull_of(p)).turn_rate.to_degrees(),
    },
    Stat {
        name: "TRAVERSE",
        unit: "°/S",
        decimals: 0,
        get: |p, _| HullStats::of(hull_of(p)).turret_speed.to_degrees(),
    },
    Stat {
        name: "MASS",
        unit: "T",
        decimals: 1,
        get: |p, l| crate::sim::Chassis::of(hull_of(p)).mass * l.hull(hull_of(p)).heft / 1000.0,
    },
    Stat {
        // Seconds of beam fire it adds before overheating.
        name: "THERMAL",
        unit: "S",
        decimals: 1,
        get: |p, _| crate::combat::thermal_mass(hull_of(p)),
    },
];

const TURRET_STATS: [Stat; 3] = [
    Stat {
        name: "DPS",
        unit: "HP/S",
        decimals: 0,
        get: |p, l| l.gun(turret_of(p)).dps(),
    },
    Stat {
        name: "PER HIT",
        unit: "HP",
        decimals: 0,
        // A beam has no single hit: show what one second of contact does.
        get: |p, l| l.gun(turret_of(p)).damage,
    },
    Stat {
        name: "RANGE",
        unit: "M",
        decimals: 0,
        get: |p, l| l.gun(turret_of(p)).range(),
    },
];

/// Navigation input, mapped from keys by the app.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    /// Move through the card grid.
    Up,
    Down,
    Left,
    Right,
    /// Switch between the hull and turret tabs.
    Switch,
    /// Equip the highlighted part (if owned).
    Act,
    /// Upgrade the highlighted part (if owned).
    Upgrade,
    /// Keep the loadout and drive off.
    Deploy,
    /// Put back the loadout from when the garage opened, and close.
    Revert,
}

/// What the app should do about a key press in the garage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Queue this kit for the next respawn.
    Queue(Loadout),
    /// E on a part you don't have: refused, it has to be stolen.
    Locked(Part),
    Upgrade(Part),
}

/// What can be clicked in the garage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// Highlight (preview) this part.
    Part(Part),
    /// Show this tab: 0 = hulls, 1 = turrets.
    Tab(usize),
    /// A button: clicking it presses its key.
    Key(Input),
}

pub struct Garage {
    open: bool,
    /// 0 = closed .. 1 = fully open; eased toward `open`.
    blend: f32,
    /// The tab shown: 0 = hull, 1 = turret.
    section: usize,
    /// The highlighted parts, previewed on the tank. Only E equips them.
    view: Loadout,
    /// Queued loadout when the garage opened (the revert target).
    entry: Loadout,
    /// Orbit camera angle around the tank (world yaw of the camera's offset).
    orbit: f32,
    /// Where the orbit is drifting to; starts at a front three-quarter view.
    orbit_goal: f32,
    /// The bay turntable's turn since the garage opened.
    spin: f32,
    /// What the pointer is over, for hover feedback.
    hover: Option<Hit>,
    /// Each tab's grid scroll in logical px, and where it's easing to.
    scroll: [f32; 2],
    scroll_goal: [f32; 2],
    thumbs: Thumbs,
}

/// A card's place in a tab's grid, from the grid's top left before scrolling.
#[derive(Clone, Copy, Debug)]
struct Card {
    part: Part,
    row: usize,
    col: usize,
    pos: Vec2,
    /// It's the first of its faction (the group header sits just above it).
    first: bool,
}

/// A tab's cards grouped by maker, each group under a header.
struct Grid {
    cards: Vec<Card>,
    /// Each group's maker and header y.
    heads: Vec<(Faction, f32)>,
    height: f32,
}

fn tab_parts(section: usize) -> Vec<Part> {
    if section == 0 {
        HULLS.iter().map(|&k| Part::Hull(k)).collect()
    } else {
        TURRETS.iter().map(|&k| Part::Turret(k)).collect()
    }
}

/// A card's status, right of its name: EQUIPPED or OWNED and the part's
/// tier; nothing for a part you haven't stolen (its padlock says so).
fn card_tag(part: Part, equipped: bool, profile: &Profile) -> Option<(String, Rgba)> {
    let level = crate::economy::level_label(profile.levels.of(part));
    if equipped {
        Some((format!("EQUIPPED · {level}"), EQUIPPED))
    } else if profile.owns(part) {
        Some((format!("OWNED · {level}"), theme::DIM))
    } else {
        None
    }
}

fn grid(section: usize) -> Grid {
    let parts = tab_parts(section);
    let (mut cards, mut heads) = (Vec::new(), Vec::new());
    let (mut y, mut row) = (0.0, 0);
    for faction in Faction::ALL {
        let group: Vec<Part> = parts
            .iter()
            .copied()
            .filter(|p| p.maker() == faction)
            .collect();
        if group.is_empty() {
            continue;
        }
        heads.push((faction, y));
        y += HEAD_H;
        for (i, &part) in group.iter().enumerate() {
            let col = i % GRID_COLS;
            if i > 0 && col == 0 {
                y += CARD_PITCH;
                row += 1;
            }
            cards.push(Card {
                part,
                row,
                col,
                pos: Vec2::new(col as f32 * (CARD_W + CARD_GAP), y),
                first: i == 0,
            });
        }
        y += CARD_PITCH;
        row += 1;
    }
    Grid {
        cards,
        heads,
        height: (y - CARD_GAP).max(0.0),
    }
}

impl Garage {
    pub fn new() -> Garage {
        let start = (HullKind::Kataphrakt, TurretKind::Kontos);
        Garage {
            open: false,
            blend: 0.0,
            section: 0,
            view: start,
            entry: start,
            orbit: 0.0,
            orbit_goal: 0.0,
            spin: 0.0,
            hover: None,
            scroll: [0.0; 2],
            scroll_goal: [0.0; 2],
            thumbs: Thumbs::new(),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Eased 0..1 openness, for fading the battle HUD out.
    pub fn presence(&self) -> f32 {
        smooth(self.blend)
    }

    /// Whether the bay is on screen (instead of the battlefield or menu backdrop).
    pub fn in_bay(&self) -> bool {
        self.blend >= BAY_AT
    }

    /// The bay turntable's angle (see `bay::scene`).
    pub fn spin(&self) -> f32 {
        self.spin
    }

    /// The bay camera: easing in from a little further out as the garage
    /// opens, and standing back for a hull too long to fit the usual frame.
    pub fn bay_camera(&self, aspect: f32) -> crate::render::Camera {
        let k = smooth(((self.blend - BAY_AT) / (1.0 - BAY_AT)).clamp(0.0, 1.0));
        let c = Chassis::of(self.view.0);
        let length = (c.front.0 + c.front.2) - (c.rear.0 - c.rear.2);
        let fit = (length / BAY_FRAMED_LENGTH).max(1.0);
        crate::bay::camera(aspect, fit * (1.0 + 0.2 * (1.0 - k)))
    }

    /// How dark the screen is for the cut between battlefield and bay:
    /// black around the switch, clear at either end.
    pub fn dip(&self) -> f32 {
        ((1.0 - (self.blend - BAY_AT).abs() / BAY_AT) * 1.8).clamp(0.0, 1.0)
    }

    /// The pointer moved over `hit` (or off the garage).
    pub fn hover(&mut self, hit: Option<Hit>) {
        self.hover = hit;
    }

    /// Highlight `kit` (e.g. for a screenshot of a part being previewed).
    pub fn browse(&mut self, kit: Loadout) {
        self.view = kit;
        // Show the tab of whatever differs from what's equipped.
        self.section = usize::from(kit.0 == self.entry.0 && kit.1 != self.entry.1);
        for s in 0..2 {
            self.reveal(s);
            self.scroll[s] = self.scroll_goal[s];
        }
    }

    /// Show the hull (0) or turret (1) tab.
    pub fn show_tab(&mut self, section: usize) {
        self.section = section.min(1);
    }

    /// Scroll the grid by `delta` logical px (mouse wheel; positive is down).
    pub fn scroll(&mut self, delta: f32) {
        let s = self.section;
        self.scroll_goal[s] = (self.scroll_goal[s] + delta).clamp(0.0, max_scroll(s));
    }

    /// The highlighted part in the tab shown.
    fn focused(&self) -> Part {
        if self.section == 0 {
            Part::Hull(self.view.0)
        } else {
            Part::Turret(self.view.1)
        }
    }

    /// Highlight `part` (switching to its tab) and scroll it into view.
    fn highlight(&mut self, part: Part) {
        match part {
            Part::Hull(k) => (self.section, self.view.0) = (0, k),
            Part::Turret(k) => (self.section, self.view.1) = (1, k),
        }
        self.reveal(self.section);
    }

    /// Scroll tab `s` so its highlighted card (and its group's header, if
    /// it's the first of the group) is in view.
    fn reveal(&mut self, s: usize) {
        let part = if s == 0 {
            Part::Hull(self.view.0)
        } else {
            Part::Turret(self.view.1)
        };
        let g = grid(s);
        let Some(c) = g.cards.iter().find(|c| c.part == part) else {
            return;
        };
        let top = c.pos.y - if c.first { HEAD_H } else { 4.0 };
        let bottom = c.pos.y + CARD_H + 4.0;
        let goal = self.scroll_goal[s].min(top).max(bottom - GRID_VIEW);
        self.scroll_goal[s] = goal.clamp(0.0, max_scroll(s));
    }

    /// Open on the player's tank (index 0) with `queued` as the pick, or close.
    pub fn toggle(&mut self, world: &World, camera: &Camera, queued: Loadout) {
        if self.open {
            self.open = false;
            return;
        }
        let t = &world.tanks[0];
        self.open = true;
        self.entry = queued;
        self.view = queued;
        for s in 0..2 {
            self.reveal(s);
        }
        // Start the orbit where the chase camera is (unless we're mid-transition) and swing round to the front.
        if self.blend <= 0.0 {
            let off = camera.pos - t.pos;
            self.orbit = off.x.atan2(off.z);
            self.spin = 0.0;
        }
        self.orbit_goal = self.orbit + wrap_angle(front_angle(t) - self.orbit);
    }

    /// Close without touching the loadout (e.g. the player was killed).
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Handle a key while open, given the loadout queued for the next respawn
    /// and the pilot's profile (what they own).
    pub fn input(&mut self, input: Input, queued: Loadout, profile: &Profile) -> Option<Event> {
        if !self.open {
            return None;
        }
        let part = self.focused();
        match input {
            Input::Up | Input::Down | Input::Left | Input::Right => {
                let g = grid(self.section);
                let cur = g.cards.iter().position(|c| c.part == part).unwrap_or(0);
                let c = g.cards[cur];
                let next = match input {
                    Input::Left => cur.saturating_sub(1),
                    Input::Right => (cur + 1).min(g.cards.len() - 1),
                    // The nearest card in the row above or below.
                    _ => {
                        let row = if input == Input::Up {
                            c.row.checked_sub(1)
                        } else {
                            Some(c.row + 1)
                        };
                        row.and_then(|r| {
                            g.cards
                                .iter()
                                .enumerate()
                                .filter(|(_, k)| k.row == r)
                                .min_by_key(|(_, k)| k.col.abs_diff(c.col))
                                .map(|(i, _)| i)
                        })
                        .unwrap_or(cur)
                    }
                };
                self.highlight(g.cards[next].part);
                None
            }
            Input::Switch => {
                self.section = 1 - self.section;
                None
            }
            Input::Act if profile.owns(part) => {
                let kit = with_part(queued, part);
                (kit != queued).then_some(Event::Queue(kit))
            }
            Input::Act => Some(Event::Locked(part)),
            Input::Upgrade => profile.owns(part).then_some(Event::Upgrade(part)),
            Input::Deploy => {
                self.open = false;
                None
            }
            Input::Revert => {
                self.open = false;
                (self.entry != queued).then_some(Event::Queue(self.entry))
            }
        }
    }

    /// A click on `hit`: a card highlights it, a tab shows it, a button presses its key.
    pub fn click(&mut self, hit: Hit, queued: Loadout, profile: &Profile) -> Option<Event> {
        match hit {
            Hit::Key(input) => self.input(input, queued, profile),
            Hit::Part(part) if self.open => {
                self.highlight(part);
                None
            }
            Hit::Tab(s) if self.open => {
                self.show_tab(s);
                None
            }
            Hit::Part(_) | Hit::Tab(_) => None,
        }
    }

    pub fn update(&mut self, dt: f32) {
        let goal = if self.open { 1.0 } else { 0.0 };
        let d = BLEND_RATE * dt;
        self.blend = if self.blend < goal {
            (self.blend + d).min(goal)
        } else {
            (self.blend - d).max(goal)
        };
        if self.open {
            self.orbit_goal += ORBIT_DRIFT * dt;
        }
        if self.in_bay() {
            self.spin += SPIN_RATE * dt;
        }
        self.orbit += (self.orbit_goal - self.orbit) * (1.0 - (-dt * 2.5).exp());
        let k = 1.0 - (-dt * 14.0).exp();
        for s in 0..2 {
            self.scroll[s] += (self.scroll_goal[s] - self.scroll[s]) * k;
        }
    }

    /// Draw up to `budget` of the cards' thumbnails while the garage shows,
    /// for a screen `height` px tall with `queued` equipped.
    #[allow(clippy::too_many_arguments)]
    pub fn render_thumbs(
        &mut self,
        r: &mut crate::render::Renderer,
        art: &crate::tank::TankArt,
        world: &World,
        tank: &Tank,
        queued: Loadout,
        height: f32,
        budget: usize,
    ) {
        if self.blend <= 0.0 {
            return;
        }
        let scale = height / REF_HEIGHT;
        let cell = (
            (CARD_W * scale).ceil() as u32,
            (THUMB_H * scale).ceil() as u32,
        );
        self.thumbs
            .update(r, art, world, tank, queued, cell, budget);
    }

    /// Blend the chase camera into a slow orbit around `tank`, framed right of the panel.
    pub fn camera(&self, chase: Camera, world: &World, tank: &Tank, aspect: f32) -> Camera {
        let e = smooth(self.blend);
        if e <= 0.0 {
            return chase;
        }
        // Blend in polar terms around the tank, so the camera swings round it
        // instead of cutting straight through it.
        let focus = tank.pos + Vec3::Y * 1.0;
        let off = chase.pos - focus;
        let chase_angle = off.x.atan2(off.z);
        let chase_dist = Vec2::new(off.x, off.z).length();
        let angle = chase_angle + wrap_angle(self.orbit - chase_angle) * e;
        let dist = chase_dist + (ORBIT_DIST - chase_dist) * e;
        let height = off.y + (ORBIT_HEIGHT - off.y) * e;
        let dir = Vec3::new(angle.sin() * dist, height, angle.cos() * dist);
        let len = dir.length();
        let hit = world.raycast(focus, dir / len, len);
        let pos = focus + dir / len * (hit - 0.6).max(3.0).min(len);
        // Aim left of the tank so it sits in the open space right of the panel
        // (more so on wide screens, where the panel takes less of the width).
        let right = (focus - pos).cross(Vec3::Y).normalize_or_zero();
        let shift = (focus - pos).length() * 0.36 * (aspect / (16.0 / 9.0)).clamp(0.6, 1.2);
        let target = focus - right * shift;
        Camera {
            pos,
            target: chase.target.lerp(target, e),
            fov_y: chase.fov_y + (50f32.to_radians() - chase.fov_y) * e,
        }
    }

    /// The player's tank as drawn while the garage is open: wearing the
    /// highlighted kit, so you see what you'd respawn in (or could buy).
    pub fn preview(&self, tank: &mut Tank) {
        if self.blend < 0.5 || (tank.hull_kind, tank.turret_kind) == self.view {
            return;
        }
        dress(tank, self.view);
    }

    /// The battle HUD faded out as the garage comes in, with the garage panel on top.
    /// `tank` is the one being driven, `queued` the kit for the next respawn.
    pub fn overlay(
        &self,
        mut hud: Vec<UiVertex>,
        tank: &Tank,
        queued: Loadout,
        profile: &Profile,
        width: f32,
        height: f32,
    ) -> Layer<Hit> {
        let fade = 1.0 - self.presence();
        for v in &mut hud {
            v.color[3] *= fade;
        }
        if fade <= 0.0 {
            hud.clear();
        }
        let garage = self.draw(tank, queued, profile, width, height);
        hud.extend(garage.verts);
        Layer {
            verts: hud,
            hits: garage.hits,
        }
    }

    pub fn draw(
        &self,
        tank: &Tank,
        queued: Loadout,
        profile: &Profile,
        width: f32,
        height: f32,
    ) -> Layer<Hit> {
        let e = smooth(self.blend);
        if e <= 0.0 {
            return Layer::empty();
        }
        let scale = height / REF_HEIGHT;
        let h = REF_HEIGHT;
        let w = width / scale;
        let mut p = Painter::new(scale);
        let mut hits = Hits::new();

        backdrop(&mut p, h);
        let x = PANEL_X - (1.0 - e) * 30.0;
        p.rect(
            Vec2::new(x - PANEL_PAD, PANEL_TOP),
            Vec2::new(GRID_W + 2.0 * PANEL_PAD, h - 2.0 * PANEL_TOP),
            PANEL_FILL,
        );

        let mut y = 64.0;
        label(&mut p, Vec2::new(x, y), "GARAGE", Align::Left);
        let cw = label(&mut p, Vec2::new(x + GRID_W, y), "CR", Align::Right);
        p.text(
            Vec2::new(x + GRID_W - cw - 8.0, y - 2.0),
            &profile.crystals.to_string(),
            &TextStyle::value(11.0),
            Align::Right,
        );
        y += 18.0;
        // The title is what you'll deploy in; the status says what the tank is showing.
        let title = format!("{} · {}", hull_name(queued.0), turret_name(queued.1));
        // Shrunk to fit the panel when the names are long.
        let mut ts = TextStyle::value(20.0);
        while ts.size > 12.0 && crate::ui::measure(&title, &ts) > GRID_W {
            ts = TextStyle::value(ts.size - 1.0);
        }
        p.text(Vec2::new(x, y + 20.0 - ts.size), &title, &ts, Align::Left);
        y += 30.0;
        let previewing: Vec<&str> = [
            (self.view.0 != queued.0).then(|| hull_name(self.view.0)),
            (self.view.1 != queued.1).then(|| turret_name(self.view.1)),
        ]
        .into_iter()
        .flatten()
        .collect();
        let (status, color) = if !previewing.is_empty() {
            (format!("PREVIEWING {}", previewing.join(" · ")), theme::DIM)
        } else if queued != (tank.hull_kind, tank.turret_kind) {
            ("EQUIPPED · FITTED ON NEXT RESPAWN".to_string(), EQUIPPED)
        } else {
            ("EQUIPPED".to_string(), EQUIPPED)
        };
        let mut s = TextStyle::LABEL;
        s.color = color;
        p.text(Vec2::new(x, y), &status, &s, Align::Left);

        self.draw_tabs(&mut p, &mut hits, x);
        self.draw_grid(&mut p, &mut hits, x, queued, profile);

        // Buttons for the highlighted part: equip / buy, then upgrade.
        let part = self.focused();
        let mut by = BUTTONS_TOP;
        let (act, ready) = primary_action(part, queued, profile);
        self.button(&mut p, &mut hits, Vec2::new(x, by), &act, ready, Input::Act);
        by += BUTTON_H + 8.0;
        if let Some((up, ready)) = upgrade_action(part, profile) {
            self.button(
                &mut p,
                &mut hits,
                Vec2::new(x, by),
                &up,
                ready,
                Input::Upgrade,
            );
        }

        // Deploy with the kit as it stands (Esc or a right click cancels).
        self.button(
            &mut p,
            &mut hits,
            Vec2::new(x, h - 72.0),
            "DEPLOY",
            true,
            Input::Deploy,
        );

        self.draw_stats(&mut p, w, queued, profile);

        let mut v = p.finish();
        for vert in &mut v {
            vert.color[3] *= e;
        }
        // The dip to black for the cut to and from the bay, under the panel.
        let dip = self.dip();
        if dip > 0.0 {
            let mut d = Painter::new(scale);
            d.rect(Vec2::ZERO, Vec2::new(w, h), [0.0, 0.0, 0.0, dip]);
            let mut under = d.finish();
            under.append(&mut v);
            v = under;
        }
        // Closing: still fading out, but nothing left to click.
        if !self.open {
            hits.clear();
        }
        Layer { verts: v, hits }
    }

    /// HULL and TURRET tabs, each with how many parts it holds.
    fn draw_tabs(&self, p: &mut Painter, hits: &mut Hits<Hit>, x: f32) {
        let tw = GRID_W * 0.5;
        for (s, name) in ["HULL", "TURRET"].into_iter().enumerate() {
            let min = Vec2::new(x + s as f32 * tw, TABS_TOP);
            let size = Vec2::new(tw - 4.0, TAB_H);
            let active = s == self.section;
            hits.add(p, min, size, Hit::Tab(s));
            p.set_halo(None);
            if active {
                p.rect(min, size, SELECT_FILL);
                p.rect(
                    min + Vec2::new(0.0, size.y - 2.0),
                    Vec2::new(size.x, 2.0),
                    theme::FG,
                );
            } else if self.hover == Some(Hit::Tab(s)) {
                p.rect(min, size, HOVER_FILL);
            }
            p.set_halo(Some((theme::HALO, theme::HALO_PX)));
            let mut t = TextStyle::LABEL;
            t.color = if active { theme::FG } else { theme::DIM };
            let ty = min.y + (TAB_H - t.size) * 0.5 - 1.0;
            p.text(Vec2::new(min.x + 12.0, ty), name, &t, Align::Left);
            let count = tab_parts(s).len().to_string();
            p.text(
                Vec2::new(min.x + size.x - 12.0, ty),
                &count,
                &t,
                Align::Right,
            );
        }
        p.line(
            Vec2::new(x, TABS_TOP + TAB_H),
            Vec2::new(x + GRID_W, TABS_TOP + TAB_H),
            1.0,
            theme::FAINT,
        );
    }

    /// The tab's cards, grouped under their makers, scrolled and clipped to
    /// the grid's viewport, with a scroll bar when they don't all fit.
    fn draw_grid(
        &self,
        p: &mut Painter,
        hits: &mut Hits<Hit>,
        x: f32,
        queued: Loadout,
        profile: &Profile,
    ) {
        let s = self.section;
        let g = grid(s);
        let origin = Vec2::new(x, GRID_TOP - self.scroll[s]);
        let view = (
            Vec2::new(x - PANEL_PAD, GRID_TOP),
            Vec2::new(x + GRID_W + PANEL_PAD, GRID_TOP + GRID_VIEW),
        );
        p.set_clip(Some(view));
        for &(faction, hy) in &g.heads {
            let at = origin + Vec2::new(0.0, hy + 4.0);
            let mut style = TextStyle::LABEL;
            style.color = with_alpha(faction.ui(), 0.85);
            let lw = emblem::faction_name(p, at, faction, &style, Align::Left);
            p.line(
                at + Vec2::new(12.0 + lw, 4.0),
                at + Vec2::new(GRID_W, 4.0),
                1.0,
                theme::FAINT,
            );
        }
        for c in &g.cards {
            let min = origin + c.pos;
            let selected = c.part == self.focused();
            let is_equipped = with_part(queued, c.part) == queued;
            self.draw_card(p, min, c.part, selected, is_equipped, profile);
            // Only the part of a card inside the viewport can be clicked.
            let (a, b) = (
                min.max(view.0),
                (min + Vec2::new(CARD_W, CARD_H)).min(view.1),
            );
            if b.x > a.x && b.y > a.y {
                hits.add(p, a, b - a, Hit::Part(c.part));
            }
        }
        p.set_clip(None);

        // Scroll bar beside the grid.
        let over = g.height - GRID_VIEW;
        if over > 0.5 {
            let bx = x + GRID_W + 14.0;
            p.line(
                Vec2::new(bx, GRID_TOP),
                Vec2::new(bx, GRID_TOP + GRID_VIEW),
                1.0,
                theme::FAINT,
            );
            let len = GRID_VIEW * GRID_VIEW / g.height;
            let top = GRID_TOP + (GRID_VIEW - len) * (self.scroll[s] / over).clamp(0.0, 1.0);
            p.line(
                Vec2::new(bx, top),
                Vec2::new(bx, top + len),
                2.0,
                theme::DIM,
            );
        }
    }

    /// One card: the part's thumbnail (dark and padlocked if it isn't yours),
    /// its name, and whether it's EQUIPPED or OWNED, at what tier. Who makes
    /// it is the header above it, so the card doesn't say it again (#387).
    fn draw_card(
        &self,
        p: &mut Painter,
        min: Vec2,
        part: Part,
        selected: bool,
        equipped: bool,
        profile: &Profile,
    ) {
        let size = Vec2::new(CARD_W, CARD_H);
        let owned = profile.owns(part);
        let locked = !owned;

        p.set_halo(None);
        p.rect(min, size, if selected { SELECT_FILL } else { CARD_FILL });
        let thumb = Vec2::new(CARD_W, THUMB_H);
        match self.thumbs.uv(part) {
            Some((u0, u1)) => {
                let k = if owned { 1.0 } else { 0.3 };
                p.image(min, thumb, u0, u1, [k, k, k, 1.0]);
            }
            None => p.rect(min, thumb, HOVER_FILL),
        }
        if locked {
            p.rect(min + Vec2::new(6.0, 6.0), Vec2::new(20.0, 20.0), PANEL_FILL);
        }
        p.set_halo(Some((theme::HALO, theme::HALO_PX)));
        if locked {
            padlock(p, min + Vec2::new(11.0, 10.0), LOCKED);
        }

        let outline = if selected {
            Some((2.0, theme::FG))
        } else if self.hover == Some(Hit::Part(part)) {
            Some((1.0, theme::DIM))
        } else if equipped {
            Some((1.0, with_alpha(EQUIPPED, 0.6)))
        } else {
            None
        };
        if let Some((wd, c)) = outline {
            p.rect_outline(min, size, wd, c);
        }

        let mut s = TextStyle::value(NAME_SIZE);
        s.color = if owned { theme::FG } else { theme::DIM };
        let y = min.y + THUMB_H + 8.0;
        p.text(
            Vec2::new(min.x + CARD_INSET, y),
            part.name(),
            &s,
            Align::Left,
        );

        if let Some((tag, color)) = card_tag(part, equipped, profile) {
            let mut t = TextStyle::LABEL;
            t.color = color;
            let at = Vec2::new(min.x + CARD_INSET, y + NAME_SIZE + 6.0);
            p.text(at, &tag, &t, Align::Left);
        }
    }

    /// The highlighted part's spec card, top right over the scene.
    fn draw_stats(&self, p: &mut Painter, w: f32, queued: Loadout, profile: &Profile) {
        let part = self.focused();
        let (items, stats, equipped) = if self.section == 0 {
            (tab_parts(0), &HULL_STATS[..], Part::Hull(queued.0))
        } else {
            (tab_parts(1), &TURRET_STATS[..], Part::Turret(queued.1))
        };
        let card_w = STATS_W + 2.0 * PANEL_PAD;
        let x = w - PANEL_TOP - card_w;
        let owned = profile.owns(part);
        let track_h = if owned { UPGRADE_TRACK_H } else { 0.0 };
        let card_h = 78.0 + track_h + stats.len() as f32 * 20.0 + 16.0;
        p.rect(
            Vec2::new(x, PANEL_TOP),
            Vec2::new(card_w, card_h),
            PANEL_FILL,
        );
        let x = x + PANEL_PAD;
        let mut y = PANEL_TOP + 26.0;
        let kind = if self.section == 0 { "HULL" } else { "TURRET" };
        let kw = label(p, Vec2::new(x, y), &format!("{kind} · "), Align::Left);
        emblem::faction_name(
            p,
            Vec2::new(x + kw + TextStyle::LABEL.tracking, y),
            part.maker(),
            &TextStyle::LABEL,
            Align::Left,
        );
        y += 16.0;
        p.text(
            Vec2::new(x, y),
            part.name(),
            &TextStyle::value(16.0),
            Align::Left,
        );
        y += 36.0;
        if owned {
            upgrade_track(p, Vec2::new(x, y - 6.0), STATS_W, profile.levels.of(part));
            y += track_h;
        }
        for stat in stats {
            draw_stat(
                p,
                Vec2::new(x, y),
                STATS_W,
                stat,
                part,
                equipped,
                &items,
                &profile.levels,
            );
            y += 20.0;
        }
    }

    /// A full-width button saying what it does. Disabled ones are drawn
    /// faint and still say why (and can't be clicked).
    fn button(
        &self,
        p: &mut Painter,
        hits: &mut Hits<Hit>,
        min: Vec2,
        text: &str,
        ready: bool,
        input: Input,
    ) {
        let size = Vec2::new(GRID_W, BUTTON_H);
        let hovered = ready && self.hover == Some(Hit::Key(input));
        p.set_halo(None);
        if ready {
            p.rect(min, size, if hovered { SELECT_FILL } else { HOVER_FILL });
        }
        p.set_halo(Some((theme::HALO, theme::HALO_PX)));
        p.rect_outline(min, size, 1.0, if ready { theme::FG } else { theme::FAINT });
        let ty = min.y + (BUTTON_H - TextStyle::LABEL.size) * 0.5;
        let mut a = TextStyle::LABEL;
        a.color = if ready { theme::FG } else { theme::DIM };
        p.text(Vec2::new(min.x + 12.0, ty), text, &a, Align::Left);
        if ready {
            hits.add(p, min, size, Hit::Key(input));
        }
    }
}

/// Height the upgrade track adds to the spec card.
const UPGRADE_TRACK_H: f32 = 30.0;

/// An owned part's climb to T3: its level, then one notch per upgrade step, filled up
/// to where it is, with the marks (T1, T2) standing taller.
fn upgrade_track(p: &mut Painter, at: Vec2, w: f32, level: u8) {
    use crate::economy::{MAX_LEVEL, STEPS_PER_MARK, level_label};
    let lw = label(p, at, "UPGRADE ", Align::Left);
    p.text(
        at + Vec2::new(lw + TextStyle::LABEL.tracking, 0.0),
        &level_label(level),
        &TextStyle::LABEL,
        Align::Left,
    );
    let y = at.y + 16.0;
    let gap = 2.0;
    let n = MAX_LEVEL as f32;
    let notch = (w - gap * (n - 1.0)) / n;
    for i in 0..MAX_LEVEL {
        let x = at.x + i as f32 * (notch + gap);
        let mark = (i + 1) % STEPS_PER_MARK == 0;
        let h = if mark { 8.0 } else { 5.0 };
        let color = if i < level { theme::FG } else { theme::FAINT };
        p.rect(Vec2::new(x, y + 8.0 - h), Vec2::new(notch, h), color);
    }
}

/// How far tab `s`'s grid can scroll.
fn max_scroll(s: usize) -> f32 {
    (grid(s).height - GRID_VIEW).max(0.0)
}

/// `tank` wearing `kit`: its hull, turret and the hull's running gear.
pub fn dress(tank: &mut Tank, kit: Loadout) {
    let chassis = crate::sim::Chassis::of(kit.0);
    tank.hull_kind = kit.0;
    tank.turret_kind = kit.1;
    tank.chassis = chassis;
    tank.wheel_lift.resize(chassis.wheel_count(), 0.0);
}

/// `kit` with `part` swapped in.
pub fn with_part(kit: Loadout, part: Part) -> Loadout {
    match part {
        Part::Hull(k) => (k, kit.1),
        Part::Turret(k) => (kit.0, k),
    }
}

/// A small padlock, `at` its top left (about 10 × 11 px).
fn padlock(p: &mut Painter, at: Vec2, color: Rgba) {
    p.arc(
        at + Vec2::new(5.0, 5.0),
        3.0,
        std::f32::consts::PI,
        std::f32::consts::TAU,
        1.2,
        color,
    );
    p.line(
        at + Vec2::new(2.0, 5.0),
        at + Vec2::new(2.0, 6.0),
        1.2,
        color,
    );
    p.line(
        at + Vec2::new(8.0, 5.0),
        at + Vec2::new(8.0, 6.0),
        1.2,
        color,
    );
    p.rect(at + Vec2::new(0.0, 6.0), Vec2::new(10.0, 6.0), color);
}

/// What E does for `part` (equip it), and whether it can right now; for a part
/// you don't have, where to steal it.
fn primary_action(part: Part, queued: Loadout, profile: &Profile) -> (String, bool) {
    if profile.owns(part) {
        if with_part(queued, part) == queued {
            ("EQUIPPED".to_string(), false)
        } else {
            (format!("EQUIP {}", part.name()), true)
        }
    } else {
        // Stolen in a theater, from a side you fight there (#311).
        let maker = part.maker().name();
        // The longest that fits the button.
        let text = [
            format!("STEAL IT: BEAT THE {maker} IN A THEATER"),
            format!("STEAL IT FROM THE {maker}"),
            format!("BEAT THE {maker}"),
        ]
        .into_iter()
        .find(|t| crate::ui::measure(t, &TextStyle::LABEL) <= PANEL_W - 48.0)
        .unwrap_or_default();
        (text, false)
    }
}

/// What U does for `part`, if it's owned, and whether it can right now.
fn upgrade_action(part: Part, profile: &Profile) -> Option<(String, bool)> {
    if !profile.owns(part) {
        return None;
    }
    let next = crate::economy::level_label(profile.levels.of(part) + 1);
    Some(match profile.can_upgrade(part) {
        Ok(cost) => (format!("UPGRADE TO {next} · {cost} CR"), true),
        Err(Refusal::Crystals(cost)) => (format!("UPGRADE TO {next} · NEED {cost} CR"), false),
        Err(_) => ("FULLY UPGRADED".to_string(), false),
    })
}

/// The front three-quarter view angle for `tank`, where the garage swings to.
pub fn front_angle(tank: &Tank) -> f32 {
    let f = tank.forward();
    f.x.atan2(f.z) + 0.65
}

/// A list row's clickable area: the whole width of the panel (and the
/// selection bar), from just above the text to the next row.
pub fn row_hit<T: Copy>(p: &Painter, hits: &mut Hits<T>, at: Vec2, pitch: f32, id: T) {
    hits.add(
        p,
        Vec2::new(at.x - 20.0, at.y - 6.0),
        Vec2::new(PANEL_W + 20.0, pitch),
        id,
    );
}

/// Dark backdrop behind the left-hand panel, so the hairlines read over a bright city.
pub fn backdrop(p: &mut Painter, h: f32) {
    backdrop_fading(p, h, 260.0);
}

/// `backdrop`, fading out over `fade` logical px: short over dark space,
/// where there's no bright scene to read the panel against.
pub fn backdrop_fading(p: &mut Painter, h: f32, fade: f32) {
    let shade = theme::PANEL;
    let solid = PANEL_X + PANEL_W + 40.0;
    p.gradient_h(Vec2::ZERO, Vec2::new(solid, h), shade, shade);
    p.gradient_h(
        Vec2::new(solid, 0.0),
        Vec2::new(fade, h),
        shade,
        [0.0, 0.0, 0.0, 0.0],
    );
}

/// Label, bar scaled to the kit's best, value; a faint tick where the part in use sits.
#[allow(clippy::too_many_arguments)]
fn draw_stat(
    p: &mut Painter,
    pos: Vec2,
    width: f32,
    stat: &Stat,
    current: Part,
    in_use: Part,
    kit: &[Part],
    levels: &Levels,
) {
    let get = |part: Part| (stat.get)(part, levels);
    let best = kit.iter().map(|&k| get(k)).fold(0.0f32, f32::max).max(1e-3);
    let frac = |part: Part| get(part) / (best * 1.2);
    label(p, pos + Vec2::new(0.0, 1.0), stat.name, Align::Left);
    let bar = pos + Vec2::new(110.0, 5.0);
    let bar_w = width - 110.0 - 98.0;
    widgets::hairbar(p, bar, bar_w, frac(current), theme::FG);
    if in_use != current {
        let tx = bar.x + bar_w * frac(in_use).clamp(0.0, 1.0);
        p.line(
            Vec2::new(tx, bar.y - 5.0),
            Vec2::new(tx, bar.y + 5.0),
            1.0,
            theme::DIM,
        );
    }
    let v = get(current);
    let text = format!("{v:.prec$}", prec = stat.decimals);
    let right = pos + Vec2::new(width, 0.0);
    let uw = if stat.unit.is_empty() {
        0.0
    } else {
        label(p, right, stat.unit, Align::Right) + 8.0
    };
    p.text(
        right - Vec2::new(uw, 0.0),
        &text,
        &TextStyle::value(12.0),
        Align::Right,
    );
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        World::new(crate::map::Map::saltmarket(), 0)
    }

    fn cam(w: &World) -> Camera {
        let t = &w.tanks[0];
        Camera {
            pos: t.pos - t.forward() * 10.0 + Vec3::Y * 5.0,
            target: t.pos,
            fov_y: 1.1,
        }
    }

    /// A pilot who owns everything.
    fn rich() -> Profile {
        let mut p = Profile::in_memory();
        p.crystals = 100_000;
        for part in crate::economy::PARTS {
            p.grant(part);
        }
        p
    }

    #[test]
    fn browsing_previews_and_e_equips() {
        let w = world();
        let p = rich();
        let mut g = Garage::new();
        let start = (HullKind::Kataphrakt, TurretKind::Kontos);
        // Closed: input does nothing.
        assert_eq!(g.input(Input::Down, start, &p), None);

        g.toggle(&w, &cam(&w), start);
        // Browsing highlights without equipping: right to the next Imperium hull.
        assert_eq!(g.input(Input::Right, start, &p), None);
        let l = (HullKind::Kastron, TurretKind::Kontos);
        assert_eq!(g.view, l);
        assert_eq!(g.input(Input::Act, start, &p), Some(Event::Queue(l)));
        // E on the equipped part does nothing.
        assert_eq!(g.input(Input::Act, l, &p), None);
        // Tab shows the turrets; down a row from Kontos is the next maker's first.
        g.input(Input::Switch, l, &p);
        assert_eq!(g.section, 1);
        g.input(Input::Down, l, &p);
        let next = grid(1).cards.iter().find(|c| c.row == 1).unwrap().part;
        assert_eq!(Part::Turret(g.view.1), next);
        let l2 = with_part(l, next);
        assert_eq!(g.input(Input::Act, l, &p), Some(Event::Queue(l2)));
        assert_eq!(g.input(Input::Upgrade, l2, &p), Some(Event::Upgrade(next)));
        assert_eq!(g.input(Input::Revert, l2, &p), Some(Event::Queue(start)));
        assert!(!g.is_open());

        g.toggle(&w, &cam(&w), start);
        g.input(Input::Switch, start, &p);
        assert_eq!(g.section, 0);
        assert_eq!(g.input(Input::Deploy, start, &p), None);
        assert!(!g.is_open());
    }

    #[test]
    fn grid_groups_by_maker_and_moves_by_rows() {
        for s in 0..2 {
            let g = grid(s);
            assert_eq!(g.cards.len(), tab_parts(s).len(), "every part has a card");
            // Each maker's cards are together, under one header.
            for (f, _) in &g.heads {
                let rows: Vec<usize> = g
                    .cards
                    .iter()
                    .filter(|c| c.part.maker() == *f)
                    .map(|c| c.row)
                    .collect();
                assert!(rows.windows(2).all(|r| r[1] - r[0] <= 1));
            }
            // No two cards overlap.
            for (i, a) in g.cards.iter().enumerate() {
                for b in &g.cards[i + 1..] {
                    assert!(a.pos.distance(b.pos) >= CARD_W.min(CARD_H) - 1.0);
                }
            }
        }
        // Up and down stop at the ends; left and right don't wrap.
        let w = world();
        let p = rich();
        let mut g = Garage::new();
        let start = (HullKind::Kataphrakt, TurretKind::Kontos);
        g.toggle(&w, &cam(&w), start);
        g.input(Input::Up, start, &p);
        g.input(Input::Left, start, &p);
        assert_eq!(g.view, start);
        for _ in 0..20 {
            g.input(Input::Down, start, &p);
        }
        assert_eq!(Part::Hull(g.view.0), grid(0).cards.last().unwrap().part);
    }

    #[test]
    fn scrolling_keeps_the_highlight_in_view() {
        let w = world();
        let p = rich();
        let mut g = Garage::new();
        let start = (HullKind::Kataphrakt, TurretKind::Kontos);
        g.toggle(&w, &cam(&w), start);
        for _ in 0..20 {
            g.input(Input::Down, start, &p);
            g.update(1.0);
            let gr = grid(0);
            let c = gr.cards.iter().find(|c| c.part == g.focused()).unwrap();
            let y = c.pos.y - g.scroll[0];
            assert!(y >= -0.5 && y + CARD_H <= GRID_VIEW + 0.5, "card at {y}");
        }
        // The wheel can't scroll past either end.
        g.scroll(-10_000.0);
        assert_eq!(g.scroll_goal[0], 0.0);
        g.scroll(10_000.0);
        assert_eq!(g.scroll_goal[0], max_scroll(0));
    }

    #[test]
    fn card_text_fits_its_card() {
        // At the longest tier label, every part's name and tag fit across
        // its card; locked cards have no tag.
        let mut p = rich();
        for part in crate::economy::PARTS {
            p.levels.set(part, crate::economy::MAX_LEVEL - 1);
        }
        let room = CARD_W - 2.0 * CARD_INSET;
        for part in crate::economy::PARTS {
            let name = crate::ui::measure(part.name(), &TextStyle::value(NAME_SIZE));
            for equipped in [true, false] {
                let (tag, _) = card_tag(part, equipped, &p).unwrap();
                let tag = crate::ui::measure(&tag, &TextStyle::LABEL);
                assert!(name.max(tag) <= room, "{} doesn't fit", part.name());
            }
            let fresh = Profile::in_memory();
            if !fresh.owns(part) {
                assert!(card_tag(part, false, &fresh).is_none());
            }
        }
    }

    #[test]
    fn unowned_parts_preview_and_say_where_to_steal_them() {
        let w = world();
        let mut p = Profile::in_memory();
        let mut g = Garage::new();
        let start = (HullKind::Kataphrakt, TurretKind::Kontos);
        g.toggle(&w, &cam(&w), start);
        g.input(Input::Switch, start, &p);
        // Pelekys isn't owned: highlighted and previewed; E is refused, U does nothing.
        assert_eq!(g.input(Input::Right, start, &p), None);
        assert_eq!(g.view, (HullKind::Kataphrakt, TurretKind::Pelekys));
        let pelekys = Part::Turret(TurretKind::Pelekys);
        assert_eq!(g.input(Input::Act, start, &p), Some(Event::Locked(pelekys)));
        assert_eq!(g.input(Input::Upgrade, start, &p), None);
        // The button says whose it is to steal: the side that makes it.
        let (text, can) = primary_action(pelekys, start, &p);
        assert!(!can);
        assert!(text.contains("IMPERIUM"), "{text}");
        // Once stolen, E equips it and U upgrades it.
        p.crystals = 100_000;
        assert!(p.steal(pelekys));
        let kit = (HullKind::Kataphrakt, TurretKind::Pelekys);
        assert_eq!(g.input(Input::Act, start, &p), Some(Event::Queue(kit)));
        assert_eq!(
            g.input(Input::Upgrade, kit, &p),
            Some(Event::Upgrade(pelekys))
        );
    }

    #[test]
    fn clicking_cards_tabs_and_buttons() {
        let w = world();
        let p = rich();
        let mut g = Garage::new();
        let start = (HullKind::Kataphrakt, TurretKind::Kontos);
        let hits = g.draw(&w.tanks[0], start, &p, 1600.0, 900.0).hits;
        assert!(hits.is_empty(), "closed: nothing to click");
        g.toggle(&w, &cam(&w), start);
        g.update(1.0);
        let click = |g: &mut Garage, hit: Hit| {
            let hits = g.draw(&w.tanks[0], start, &p, 1600.0, 900.0).hits;
            let (id, _) = hits.at(hits.center_of(hit).unwrap()).unwrap();
            assert_eq!(id, hit);
            g.click(id, start, &p)
        };
        let kastron = Part::Hull(HullKind::Kastron);
        // A click previews, it doesn't equip.
        assert_eq!(click(&mut g, Hit::Part(kastron)), None);
        assert_eq!(g.focused(), kastron);
        assert_eq!(
            click(&mut g, Hit::Key(Input::Act)),
            Some(Event::Queue((HullKind::Kastron, TurretKind::Kontos)))
        );
        // Turret cards aren't drawn (or clickable) until their tab is shown.
        let pelekys = Hit::Part(Part::Turret(TurretKind::Pelekys));
        let hits = g.draw(&w.tanks[0], start, &p, 1600.0, 900.0).hits;
        assert!(hits.center_of(pelekys).is_none());
        assert_eq!(click(&mut g, Hit::Tab(1)), None);
        assert_eq!(g.section, 1);
        assert_eq!(click(&mut g, pelekys), None);
        assert_eq!(click(&mut g, Hit::Key(Input::Deploy)), None);
        assert!(!g.is_open());
    }

    #[test]
    fn disabled_buttons_cant_be_clicked() {
        let w = world();
        let p = Profile::in_memory();
        let mut g = Garage::new();
        let start = (HullKind::Kataphrakt, TurretKind::Kontos);
        g.toggle(&w, &cam(&w), start);
        g.update(1.0);
        // The starter hull is equipped: nothing for E to do.
        let hits = g.draw(&w.tanks[0], start, &p, 1600.0, 900.0).hits;
        assert!(hits.center_of(Hit::Key(Input::Act)).is_none());
    }

    #[test]
    fn preview_wears_the_highlighted_kit() {
        let w = world();
        let mut g = Garage::new();
        let kit = (HullKind::Kastron, TurretKind::Pelekys);
        let mut t = w.tanks[0].clone();
        g.preview(&mut t);
        assert_eq!(
            t.hull_kind,
            HullKind::Kataphrakt,
            "closed garage leaves the tank alone"
        );
        g.toggle(&w, &cam(&w), kit);
        g.update(1.0);
        g.preview(&mut t);
        assert_eq!((t.hull_kind, t.turret_kind), kit);
        assert_eq!(
            t.wheel_lift.len(),
            crate::sim::Chassis::of(HullKind::Kastron).wheel_count()
        );
        // The real tank is untouched.
        assert_eq!(w.tanks[0].hull_kind, HullKind::Kataphrakt);
    }

    #[test]
    fn camera_blends_from_chase_to_orbit() {
        let w = world();
        let mut g = Garage::new();
        let c = cam(&w);
        let t = &w.tanks[0];
        assert_eq!(g.camera(c, &w, t, 16.0 / 9.0).pos, c.pos);
        g.toggle(&w, &c, (HullKind::Kataphrakt, TurretKind::Kontos));
        for _ in 0..120 {
            g.update(1.0 / 60.0);
        }
        assert_eq!(g.presence(), 1.0);
        let o = g.camera(c, &w, t, 16.0 / 9.0);
        let d = o.pos.distance(t.pos + Vec3::Y);
        assert!(d > 2.9 && d < ORBIT_DIST + 1.0, "orbit distance {d}");
    }

    #[test]
    fn draws_on_screen_at_any_size() {
        let w = world();
        let mut g = Garage::new();
        let kit = (HullKind::Kastron, TurretKind::Pelekys);
        let p = Profile::in_memory();
        assert!(g.draw(&w.tanks[0], kit, &p, 1600.0, 900.0).verts.is_empty());
        g.toggle(&w, &cam(&w), kit);
        g.update(1.0);
        for (wd, ht) in [(1600.0, 900.0), (1024.0, 768.0), (3840.0, 2160.0)] {
            let v = g.draw(&w.tanks[0], kit, &p, wd, ht).verts;
            assert!(!v.is_empty());
            assert!(v.iter().all(|v| v.pos[0] > -4.0
                && v.pos[0] < wd + 4.0
                && v.pos[1] > -4.0
                && v.pos[1] < ht + 4.0));
        }
    }
}
