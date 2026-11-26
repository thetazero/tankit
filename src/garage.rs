//! Garage: from the main menu, or G mid-match to park and swing the camera
//! round your tank. Pick the hull and turret for your next respawn, and
//! upgrade parts with crystals. The tank you're driving doesn't change. Parts
//! you don't have yet say whose tanks to wreck to learn them, and how far
//! you've got.
//!
//! Hulls and turrets each have a tab: a scrolling list of rows, each the part
//! alone in a picture with how far along it is, grouped by maker. Browsing
//! only previews: the orbit camera shows the highlighted kit (parts you don't
//! own too), and nothing is
//! equipped until you press E (equip) on a part. U upgrades the
//! highlighted part. Deploying keeps what you equipped, cancelling puts back
//! what was equipped when you came in.
//!
//! Drawn as a panel on the left, the highlighted part's stats and what you can
//! do with it in a card top right, the tank in the middle between.
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
use crate::ui::{Align, Hits, Layer, Painter, TextStyle, design, measure, with_alpha};

const REF_HEIGHT: f32 = 900.0;
/// The tabs, and the list's viewport under them.
const TABS_TOP: f32 = design::UNIT * 11.0;
const TAB_H: f32 = design::UNIT * 4.0;
const GRID_TOP: f32 = design::UNIT * 17.0;
const GRID_VIEW: f32 = design::UNIT * 74.0;
const GRID_W: f32 = design::COLUMN;
/// A part's row, its picture, and the pitch; and its maker's head above.
const THUMB: Vec2 = Vec2::new(design::UNIT * 9.0, design::UNIT * 5.0);
const CARD_H: f32 = THUMB.y;
const CARD_PITCH: f32 = design::UNIT * 6.0;
const HEAD_H: f32 = design::UNIT * 3.5;
/// Under each maker's rows.
const GROUP_GAP: f32 = design::UNIT * 2.0;
/// The stats card: its content's width, its padding, and a stat's pitch.
const STATS_W: f32 = design::UNIT * 40.0;
const STATS_PAD: f32 = design::UNIT * 3.5;
const STAT_ROW: f32 = design::UNIT * 2.75;
/// How narrow a screen gets (its shape) before the bay camera backs off.
const ROOMY: f32 = 1.6;
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
    /// E on a part you don't have: refused, it has to be reverse-engineered.
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

/// A row's place in a tab's list, from the list's top left before scrolling.
#[derive(Clone, Copy, Debug)]
struct Card {
    part: Part,
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

/// How far along learning `part`'s next step is, 0..1 (full once there's none).
fn progress(part: Part, profile: &Profile) -> f32 {
    let known = profile.known.of(part);
    if profile.known.cap(part) >= crate::economy::MAX_LEVEL {
        1.0
    } else {
        known - known.floor()
    }
}

/// `part`'s tiers at `at`, aligned like text: the one it's built to in amber,
/// then, if you've learned further, how far in white, as on its track.
fn tiers(p: &mut Painter, at: Vec2, part: Part, profile: &Profile, style: TextStyle, align: Align) {
    use design::{AMBER, TEXT, TEXT_2};
    if !profile.owns(part) {
        return;
    }
    let (level, cap) = (profile.levels.of(part), profile.known.cap(part));
    let mut text = vec![(crate::economy::level_label(level), AMBER)];
    if cap > level {
        text.push((" / ".to_string(), TEXT_2));
        text.push((crate::economy::level_label(cap), TEXT));
    }
    let gap = style.tracking;
    let wide: f32 = text.iter().map(|(t, _)| measure(t, &style) + gap).sum();
    let mut x = match align {
        Align::Right => at.x - wide + gap,
        _ => at.x,
    };
    for (t, color) in text {
        p.text(Vec2::new(x, at.y), &t, &style.colored(color), Align::Left);
        x += measure(&t, &style) + gap;
    }
}

fn grid(section: usize) -> Grid {
    let parts = tab_parts(section);
    let (mut cards, mut heads) = (Vec::new(), Vec::new());
    let mut y = 0.0;
    for faction in Faction::ALL {
        let group = parts.iter().copied().filter(|p| p.maker() == faction);
        for (i, part) in group.enumerate() {
            if i == 0 {
                if !heads.is_empty() {
                    y += GROUP_GAP;
                }
                heads.push((faction, y));
                y += HEAD_H;
            }
            cards.push(Card {
                part,
                pos: Vec2::new(0.0, y),
                first: i == 0,
            });
            y += CARD_PITCH;
        }
    }
    Grid {
        cards,
        heads,
        height: y,
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
        let fit = (length / BAY_FRAMED_LENGTH).max(1.0) * (ROOMY / aspect).max(1.0);
        crate::bay::camera(fit * (1.0 + 0.2 * (1.0 - k)))
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
                let next = match input {
                    Input::Up | Input::Left => cur.saturating_sub(1),
                    _ => (cur + 1).min(g.cards.len() - 1),
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
    /// for a screen `height` px tall.
    #[allow(clippy::too_many_arguments)]
    pub fn render_thumbs(
        &mut self,
        r: &mut crate::render::Renderer,
        art: &crate::tank::TankArt,
        world: &World,
        tank: &Tank,
        height: f32,
        budget: usize,
    ) {
        if self.blend <= 0.0 {
            return;
        }
        let scale = height / REF_HEIGHT;
        let cell = (
            // Twice the size it's shown, to be smooth.
            (THUMB.x * scale * 2.0).ceil() as u32,
            (THUMB.y * scale * 2.0).ceil() as u32,
        );
        self.thumbs.update(r, art, world, tank, cell, budget);
    }

    /// Blend the chase camera into a slow orbit around `tank`, framed right of the panel.
    pub fn camera(&self, chase: Camera, world: &World, tank: &Tank) -> Camera {
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
        // The tank in the middle, between the panel and the card.
        let target = focus;
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
        use design::{BLUE, BODY, MARGIN, SMALL, TEXT_2, UNIT};
        let e = smooth(self.blend);
        if e <= 0.0 {
            return Layer::empty();
        }
        let scale = height / REF_HEIGHT;
        let h = REF_HEIGHT;
        let w = width / scale;
        let mut p = Painter::new(scale);
        let mut hits = Hits::new();

        design::panel(&mut p, h);
        let x = MARGIN;
        let cr = format!("{} cr", profile.crystals);
        let at = Vec2::splat(MARGIN);
        p.text(at, &cr, &BODY.colored(BLUE), Align::Left);
        self.draw_tabs(&mut p, &mut hits, x);
        self.draw_grid(&mut p, &mut hits, x, queued, profile);

        // Back out, putting the kit back as Esc does; or keep it as it stands.
        let foot = design::foot(h, 1).y;
        if queued != (tank.hull_kind, tank.turret_kind) {
            let at = Vec2::new(x, foot - UNIT * 3.0);
            let style = SMALL.colored(TEXT_2);
            p.text(at, "On your next respawn", &style, Align::Left);
        }
        for (k, (name, input)) in [("Back", Input::Revert), ("Done", Input::Deploy)]
            .into_iter()
            .enumerate()
        {
            let min = design::foot(h, k);
            self.button(&mut p, &mut hits, min, GRID_W, name, Some(input));
        }

        self.draw_stats(&mut p, &mut hits, w, queued, profile);

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

    /// The hulls' and turrets' tabs.
    fn draw_tabs(&self, p: &mut Painter, hits: &mut Hits<Hit>, x: f32) {
        use design::{AMBER, SMALL, TEXT, TEXT_2};
        let tw = GRID_W * 0.5;
        for (s, name) in ["Hulls", "Turrets"].into_iter().enumerate() {
            let min = Vec2::new(x + s as f32 * tw, TABS_TOP);
            let size = Vec2::new(tw, TAB_H);
            let active = s == self.section;
            let lit = active || self.hover == Some(Hit::Tab(s));
            let style = SMALL.colored(if lit { TEXT } else { TEXT_2 });
            let at = min + Vec2::Y * (TAB_H - style.size) * 0.5;
            p.text(at, name, &style, Align::Left);
            let rule = min + Vec2::Y * (TAB_H - 2.0);
            let color = if active {
                AMBER
            } else {
                with_alpha(TEXT_2, 0.3)
            };
            p.rect(rule, Vec2::new(tw, 2.0), color);
            hits.add(p, min, size, Hit::Tab(s));
        }
    }

    /// The tab's rows, grouped under their makers, scrolled and clipped to
    /// the list's viewport, with a scroll bar when they don't all fit.
    fn draw_grid(
        &self,
        p: &mut Painter,
        hits: &mut Hits<Hit>,
        x: f32,
        queued: Loadout,
        profile: &Profile,
    ) {
        use design::{SMALL, TEXT_2, UNIT};
        let s = self.section;
        let g = grid(s);
        let origin = Vec2::new(x, GRID_TOP - self.scroll[s]);
        let view = (
            Vec2::new(0.0, GRID_TOP),
            Vec2::new(x + GRID_W + UNIT * 4.0, GRID_TOP + GRID_VIEW),
        );
        p.set_clip(Some(view));
        for &(faction, hy) in &g.heads {
            let style = SMALL.colored(faction.ui());
            let at = origin + Vec2::Y * (hy + 4.0);
            emblem::faction_name(p, at, faction, &style, Align::Left);
        }
        for c in &g.cards {
            let min = origin + c.pos;
            let equipped = with_part(queued, c.part) == queued;
            self.draw_row(p, min, c.part, equipped, profile);
            // Only the part of a row inside the viewport can be clicked.
            let (a, b) = (
                min.max(view.0),
                (min + Vec2::new(GRID_W, CARD_H)).min(view.1),
            );
            if b.x > a.x && b.y > a.y {
                hits.add(p, a, b - a, Hit::Part(c.part));
            }
        }
        p.set_clip(None);

        // Scroll bar beside the list.
        let over = g.height - GRID_VIEW;
        if over > 0.5 {
            let bx = x + GRID_W + UNIT * 3.0;
            let len = GRID_VIEW * GRID_VIEW / g.height;
            let top = GRID_TOP + (GRID_VIEW - len) * (self.scroll[s] / over).clamp(0.0, 1.0);
            p.rect(Vec2::new(bx, top), Vec2::new(2.0, len), TEXT_2);
        }
    }

    /// One row: the part (faded if it isn't yours), its name (amber if it's
    /// on), its tiers once it's yours, and how far along learning the next is.
    fn draw_row(&self, p: &mut Painter, min: Vec2, part: Part, equipped: bool, profile: &Profile) {
        use design::{AMBER, SMALL, TEXT, TEXT_2, UNIT};
        let owned = profile.owns(part);
        let picked = part == self.focused();
        let hovered = self.hover == Some(Hit::Part(part));
        if picked || hovered {
            let pad = Vec2::new(UNIT, UNIT * 0.5);
            let size = Vec2::new(GRID_W, CARD_H) + pad * 2.0;
            let k = if picked { 0.016 } else { 0.006 };
            p.rect(min - pad, size, with_alpha(TEXT, k));
            if picked {
                p.rect(min - pad, Vec2::new(3.0, size.y), AMBER);
            }
        }
        if let Some((u0, u1)) = self.thumbs.uv(part) {
            // Faded, not darkened: its ground is the panel's.
            let k = if owned { 1.0 } else { 0.45 };
            p.image(min, THUMB, u0, u1, [1.0, 1.0, 1.0, k]);
        }
        let x = min.x + THUMB.x + UNIT * 1.5;
        let right = min.x + GRID_W;
        let style = SMALL.colored(match (equipped, owned) {
            (true, _) => AMBER,
            (_, true) => TEXT,
            _ => TEXT_2,
        });
        p.text(Vec2::new(x, min.y + 2.0), part.name(), &style, Align::Left);
        let at = Vec2::new(x, min.y + UNIT * 2.25);
        tiers(p, at, part, profile, SMALL, Align::Left);
        let bar = Vec2::new(x, min.y + UNIT * 4.5);
        let w = right - x;
        p.rect(bar, Vec2::new(w, 2.0), with_alpha(TEXT_2, 0.3));
        p.rect(bar, Vec2::new(w * progress(part, profile), 2.0), TEXT);
    }

    /// The highlighted part's card, top right over the scene: what it is, how
    /// far it's upgraded, its stats, and what you can do with it.
    fn draw_stats(
        &self,
        p: &mut Painter,
        hits: &mut Hits<Hit>,
        w: f32,
        queued: Loadout,
        profile: &Profile,
    ) {
        use design::{BODY, BUTTON, GROUND, MARGIN, SMALL, UNIT};
        let part = self.focused();
        let (items, stats, equipped) = if self.section == 0 {
            (tab_parts(0), &HULL_STATS[..], Part::Hull(queued.0))
        } else {
            (tab_parts(1), &TURRET_STATS[..], Part::Turret(queued.1))
        };
        let actions = actions(part, queued, profile);
        let card_w = STATS_W + 2.0 * STATS_PAD;
        let x = w - MARGIN - card_w;
        let buttons = actions.len() as f32 * (BUTTON + UNIT);
        let card_h = STATS_PAD * 2.0
            + UNIT * 6.0
            + UPGRADE_TRACK_H
            + stats.len() as f32 * STAT_ROW
            + UNIT
            + buttons;
        p.rect(Vec2::new(x, MARGIN), Vec2::new(card_w, card_h), GROUND);
        let x = x + STATS_PAD;
        let mut y = MARGIN + STATS_PAD;
        let maker = part.maker();
        let style = SMALL.colored(maker.ui());
        emblem::faction_name(p, Vec2::new(x, y), maker, &style, Align::Left);
        y += UNIT * 2.5;
        p.text(Vec2::new(x, y), part.name(), &BODY, Align::Left);
        let at = Vec2::new(x + STATS_W, y);
        tiers(p, at, part, profile, BODY, Align::Right);
        y += UNIT * 3.5;
        let (level, cap) = (profile.levels.of(part), profile.known.cap(part));
        upgrade_track(p, Vec2::new(x, y), STATS_W, level, cap);
        y += UPGRADE_TRACK_H;
        for stat in stats {
            let at = Vec2::new(x, y);
            draw_stat(
                p,
                at,
                STATS_W,
                stat,
                part,
                equipped,
                &items,
                &profile.levels,
            );
            y += STAT_ROW;
        }
        y += UNIT;
        for (text, input) in &actions {
            let min = Vec2::new(x, y);
            self.button(p, hits, min, STATS_W, text, *input);
            y += BUTTON + UNIT;
        }
    }

    /// A button that presses `input`'s key, if it can be pressed.
    #[allow(clippy::too_many_arguments)]
    fn button(
        &self,
        p: &mut Painter,
        hits: &mut Hits<Hit>,
        min: Vec2,
        width: f32,
        text: &str,
        input: Option<Input>,
    ) {
        let hover = input.is_some_and(|i| self.hover == Some(Hit::Key(i)));
        design::button(p, min, width, text, input.is_some(), hover);
        if let Some(i) = input {
            let size = Vec2::new(width, design::BUTTON);
            hits.add(p, min, size, Hit::Key(i));
        }
    }
}

/// Height the upgrade track adds to the spec card.
const UPGRADE_TRACK_H: f32 = design::UNIT * 3.0;

/// A part's climb to T3: one notch per upgrade step, with the marks (T1, T2) standing taller. Each notch says how far you've got
/// (#451): bought up to `level`, known and ready to buy up to `cap`, locked past it.
fn upgrade_track(p: &mut Painter, at: Vec2, w: f32, level: u8, cap: u8) {
    use crate::economy::{MAX_LEVEL, STEPS_PER_MARK};
    let y = at.y;
    let gap = 2.0;
    let n = MAX_LEVEL as f32;
    let notch = (w - gap * (n - 1.0)) / n;
    for i in 0..MAX_LEVEL {
        let x = at.x + i as f32 * (notch + gap);
        let mark = (i + 1) % STEPS_PER_MARK == 0;
        let h = if mark { 8.0 } else { 5.0 };
        let color = match step_state(i, level, cap) {
            Step::Bought => design::AMBER,
            Step::Available => design::TEXT,
            Step::Locked => with_alpha(design::TEXT_2, 0.3),
        };
        p.rect(Vec2::new(x, y + 8.0 - h), Vec2::new(notch, h), color);
    }
}

/// Where an upgrade step stands for a part at `level` that can be built to `cap`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Bought,
    Available,
    Locked,
}

/// Step `i` (0-based: step `i` takes the part to level `i + 1`).
fn step_state(i: u8, level: u8, cap: u8) -> Step {
    if i < level {
        Step::Bought
    } else if i < cap {
        Step::Available
    } else {
        Step::Locked
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

/// What you can do with `part`, or what's stopping you, in words: each with
/// the key that does it, if it can be done.
fn actions(part: Part, queued: Loadout, profile: &Profile) -> Vec<(String, Option<Input>)> {
    if !profile.owns(part) {
        return vec![("Not learned yet".to_string(), None)];
    }
    let mut out = Vec::new();
    if with_part(queued, part) != queued {
        out.push(("Equip".to_string(), Some(Input::Act)));
    }
    let next = crate::economy::level_label(profile.levels.of(part) + 1);
    out.push(match profile.can_upgrade(part) {
        Ok(cost) => (format!("Upgrade  {cost} cr"), Some(Input::Upgrade)),
        Err(Refusal::Crystals(cost)) => (format!("{next} costs {cost} cr"), None),
        Err(Refusal::Unknown) => (format!("{next} not learned yet"), None),
        Err(_) => ("Fully upgraded".to_string(), None),
    });
    out
}

/// The front three-quarter view angle for `tank`, where the garage swings to.
pub fn front_angle(tank: &Tank) -> f32 {
    let f = tank.forward();
    f.x.atan2(f.z) + 0.65
}

/// The room a stat's unit takes.
const UNIT_W: f32 = design::UNIT * 7.0;

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
    use design::{BODY, SMALL, TEXT, TEXT_2};
    let name = pos + Vec2::Y * 1.0;
    p.text(name, stat.name, &SMALL.colored(TEXT_2), Align::Left);
    let bar = pos + Vec2::new(110.0, 5.0);
    let bar_w = width - 110.0 - 112.0;
    p.rect(bar, Vec2::new(bar_w, 2.0), with_alpha(TEXT_2, 0.3));
    let fill = bar_w * frac(current).clamp(0.0, 1.0);
    p.rect(bar, Vec2::new(fill, 2.0), TEXT);
    if in_use != current {
        let tx = bar.x + bar_w * frac(in_use).clamp(0.0, 1.0);
        let tick = Vec2::new(tx - 1.0, bar.y - 4.0);
        p.rect(tick, Vec2::new(2.0, 10.0), design::AMBER);
    }
    let v = get(current);
    let text = format!("{v:.prec$}", prec = stat.decimals);
    // The numbers end in line, their units after them.
    let right = pos + Vec2::new(width, 0.0);
    let unit = right - Vec2::X * UNIT_W;
    let at = unit + Vec2::new(8.0, 1.0);
    p.text(at, stat.unit, &SMALL.colored(TEXT_2), Align::Left);
    let style = TextStyle { size: 10.0, ..BODY };
    p.text(unit, &text, &style, Align::Right);
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_steps_are_bought_then_available_then_locked() {
        // Level 2, known to level 5: two bought, three to buy, the rest locked.
        let states: Vec<Step> = (0..7).map(|i| step_state(i, 2, 5)).collect();
        use Step::*;
        assert_eq!(
            states,
            [
                Bought, Bought, Available, Available, Available, Locked, Locked
            ]
        );
        // Nothing known past what's built: nothing to buy.
        assert_eq!(step_state(3, 3, 3), Locked);
    }

    /// A part says what can be done with it, or what's stopping you.
    #[test]
    fn actions_say_what_and_why_not() {
        let mut p = Profile::in_memory();
        let queued = (HullKind::Kataphrakt, TurretKind::Kontos);
        let words = |p: &Profile, part| -> Vec<(String, bool)> {
            actions(part, queued, p)
                .into_iter()
                .map(|(text, key)| (text, key.is_some()))
                .collect()
        };
        let kastron = Part::Hull(HullKind::Kastron);
        p.known.set(kastron, 0.25);
        assert_eq!(words(&p, kastron), [("Not learned yet".into(), false)]);
        assert_eq!(progress(kastron, &p), 0.25);
        // Known to 1.5 and built to 1: the next step is half learned.
        p.known.set(kastron, 1.5);
        p.levels.set(kastron, 1);
        let next = crate::economy::level_label(2);
        assert_eq!(
            words(&p, kastron),
            [
                ("Equip".into(), true),
                (format!("{next} not learned yet"), false)
            ]
        );
        assert_eq!(progress(kastron, &p), 0.5);
        // Known further: crystals decide.
        p.known.set(kastron, 2.5);
        p.crystals = 0;
        assert_eq!(progress(kastron, &p), 0.5);
        assert!(words(&p, kastron)[1].0.contains("costs"));
        p.crystals = 100_000;
        assert!(words(&p, kastron)[1].1);
    }

    /// Every part has a row under its maker, none overlapping, and the keys
    /// run down them, stopping at the ends.
    #[test]
    fn rows_are_grouped_by_maker() {
        for s in 0..2 {
            let g = grid(s);
            assert_eq!(g.cards.len(), tab_parts(s).len());
            let ys = g.cards.windows(2);
            assert!(
                ys.into_iter()
                    .all(|c| c[1].pos.y - c[0].pos.y >= CARD_PITCH)
            );
            let makers = g.cards.iter().filter(|c| c.first).count();
            assert_eq!(makers, g.heads.len());
        }
        let room = GRID_W - THUMB.x - design::UNIT * 1.5;
        for part in crate::economy::PARTS {
            assert!(measure(part.name(), &design::SMALL) <= room);
        }
        let w = world();
        let p = rich();
        let mut g = Garage::new();
        let start = (HullKind::Kataphrakt, TurretKind::Kontos);
        g.toggle(&w, &cam(&w), start);
        g.input(Input::Up, start, &p);
        assert_eq!(g.view, start);
        for _ in 0..20 {
            g.input(Input::Down, start, &p);
        }
        assert_eq!(g.focused(), grid(0).cards.last().unwrap().part);
    }

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
        // Tab shows the turrets; down from Kontos is the next.
        g.input(Input::Switch, l, &p);
        assert_eq!(g.section, 1);
        g.input(Input::Down, l, &p);
        let next = grid(1).cards[1].part;
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
    fn unowned_parts_preview_and_say_whose_wrecks_teach_them() {
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
        // Once reverse-engineered, E equips it and U upgrades it.
        p.crystals = 100_000;
        p.known.set(pelekys, 2.0);
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
        // BACK leaves too, as Esc does.
        g.toggle(&w, &cam(&w), start);
        g.update(1.0);
        assert_eq!(click(&mut g, Hit::Key(Input::Revert)), None);
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
        assert_eq!(g.camera(c, &w, t).pos, c.pos);
        g.toggle(&w, &c, (HullKind::Kataphrakt, TurretKind::Kontos));
        for _ in 0..120 {
            g.update(1.0 / 60.0);
        }
        assert_eq!(g.presence(), 1.0);
        let o = g.camera(c, &w, t);
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
