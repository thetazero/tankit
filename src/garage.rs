//! Garage: from the main menu, or G mid-match to park and swing the camera
//! round your tank. Pick the hull and turret for your next respawn, and buy
//! or upgrade parts with crystals. The tank you're driving doesn't change.
//!
//! Browsing only previews: the orbit camera shows the highlighted kit (parts
//! you don't own too), and nothing is equipped until you press E (equip, or
//! buy) on a part. U upgrades the highlighted part. Deploying keeps what you
//! equipped, cancelling puts back what was equipped when you came in.
//!
//! Drawn as a solid panel on the left, the tank framed in the space to its right.

use glam::{Vec2, Vec3};

use crate::economy::{Levels, Part};
use crate::profile::{Profile, Refusal};
use crate::render::{Camera, UiVertex};
use crate::sim::{HullKind, HullStats, Tank, TurretKind, World, wrap_angle};
use crate::ui::widgets::{self, label};
use crate::ui::{Align, Hits, Layer, Painter, Rgba, TextStyle, theme};

const REF_HEIGHT: f32 = 900.0;
pub const PANEL_X: f32 = 64.0;
pub const PANEL_W: f32 = 340.0;
/// The solid panel's margin around the content, and its gap from the screen edges.
const PANEL_PAD: f32 = 28.0;
const PANEL_TOP: f32 = 32.0;
const ROW_PITCH: f32 = 28.0;
const BUTTON_H: f32 = 30.0;
/// Near-opaque so the text reads over any map.
const PANEL_FILL: Rgba = [0.006, 0.007, 0.009, 0.95];
const SELECT_FILL: Rgba = [0.94, 0.91, 0.86, 0.16];
const HOVER_FILL: Rgba = [0.94, 0.91, 0.86, 0.07];
/// What you'll deploy with.
const EQUIPPED: Rgba = [0.52, 0.86, 0.66, 1.0];
/// Parts you can't buy yet (not enough trust with their maker).
const LOCKED: Rgba = [0.86, 0.47, 0.38, 0.85];
const LOCKED_NAME: Rgba = [0.86, 0.47, 0.38, 0.5];
/// Open/close transition speed (1 / seconds).
const BLEND_RATE: f32 = 2.5;
/// Orbit camera: distance, height and slow drift around the tank (rad/s).
const ORBIT_DIST: f32 = 8.5;
const ORBIT_HEIGHT: f32 = 2.6;
const ORBIT_DRIFT: f32 = 0.12;

pub type Loadout = (HullKind, TurretKind);

pub const HULLS: [HullKind; 5] = [
    HullKind::Kataphrakt,
    HullKind::Kastron,
    HullKind::Sethuun,
    HullKind::Vigil,
    HullKind::Gravis,
];
pub const TURRETS: [TurretKind; 6] = TurretKind::ALL;

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
        get: |p, _| HullStats::of(hull_of(p)).max_speed * 3.6,
    },
    Stat {
        name: "ACCEL",
        unit: "M/S2",
        decimals: 1,
        get: |p, _| HullStats::of(hull_of(p)).accel,
    },
    Stat {
        name: "TURN",
        unit: "°/S",
        decimals: 0,
        get: |p, _| HullStats::of(hull_of(p)).turn_rate.to_degrees(),
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
        decimals: 0,
        get: |p, _| crate::sim::Chassis::of(hull_of(p)).mass / 1000.0,
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
    Up,
    Down,
    /// Focus the hull list.
    Left,
    /// Focus the turret list.
    Right,
    /// Equip the highlighted part, or buy it if it isn't owned.
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
    Buy(Part),
    Upgrade(Part),
}

/// What can be clicked in the garage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// Highlight (preview) this part.
    Part(Part),
    /// A button or key hint: clicking it presses the key.
    Key(Input),
}

pub struct Garage {
    open: bool,
    /// 0 = closed .. 1 = fully open; eased toward `open`.
    blend: f32,
    /// 0 = hull, 1 = turret.
    section: usize,
    /// The highlighted parts, previewed on the tank. Only E equips them.
    view: Loadout,
    /// Queued loadout when the garage opened (the revert target).
    entry: Loadout,
    /// Orbit camera angle around the tank (world yaw of the camera's offset).
    orbit: f32,
    /// Where the orbit is drifting to; starts at a front three-quarter view.
    orbit_goal: f32,
    /// What the pointer is over, for hover feedback.
    hover: Option<Hit>,
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
            hover: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Eased 0..1 openness, for fading the battle HUD out.
    pub fn presence(&self) -> f32 {
        smooth(self.blend)
    }

    /// The pointer moved over `hit` (or off the garage).
    pub fn hover(&mut self, hit: Option<Hit>) {
        self.hover = hit;
    }

    /// Highlight `kit` (e.g. for a screenshot of a part being previewed).
    pub fn browse(&mut self, kit: Loadout) {
        self.view = kit;
    }

    /// The highlighted part in the focused section.
    fn focused(&self) -> Part {
        if self.section == 0 {
            Part::Hull(self.view.0)
        } else {
            Part::Turret(self.view.1)
        }
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
        // Start the orbit where the chase camera is (unless we're mid-transition) and swing round to the front.
        if self.blend <= 0.0 {
            let off = camera.pos - t.pos;
            self.orbit = off.x.atan2(off.z);
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
        // Wrapping step through a list of `n`.
        let next = |i: usize, n: usize| {
            if input == Input::Down {
                (i + 1) % n
            } else {
                (i + n - 1) % n
            }
        };
        let part = self.focused();
        match input {
            Input::Left => {
                self.section = 0;
                None
            }
            Input::Right => {
                self.section = 1;
                None
            }
            Input::Up | Input::Down => {
                if self.section == 0 {
                    let i = HULLS.iter().position(|&h| h == self.view.0).unwrap_or(0);
                    self.view.0 = HULLS[next(i, HULLS.len())];
                } else {
                    let i = TURRETS.iter().position(|&k| k == self.view.1).unwrap_or(0);
                    self.view.1 = TURRETS[next(i, TURRETS.len())];
                }
                None
            }
            Input::Act if profile.owns(part) => {
                let kit = with_part(queued, part);
                (kit != queued).then_some(Event::Queue(kit))
            }
            Input::Act => Some(Event::Buy(part)),
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

    /// A click on `hit`: a part row highlights it, a button presses its key.
    pub fn click(&mut self, hit: Hit, queued: Loadout, profile: &Profile) -> Option<Event> {
        match hit {
            Hit::Key(input) => self.input(input, queued, profile),
            Hit::Part(part) if self.open => {
                match part {
                    Part::Hull(k) => (self.section, self.view.0) = (0, k),
                    Part::Turret(k) => (self.section, self.view.1) = (1, k),
                }
                None
            }
            Hit::Part(_) => None,
        }
    }

    /// After a purchase: equip the part just bought (the highlighted one).
    pub fn bought(&mut self, queued: Loadout, profile: &Profile) -> Option<Event> {
        let part = self.focused();
        let kit = with_part(queued, part);
        (profile.owns(part) && kit != queued).then_some(Event::Queue(kit))
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
        self.orbit += (self.orbit_goal - self.orbit) * (1.0 - (-dt * 2.5).exp());
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
        let shift = (focus - pos).length() * 0.28 * (aspect / (16.0 / 9.0)).clamp(0.6, 1.2);
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
        let kit = self.view;
        if self.blend < 0.5 || (tank.hull_kind, tank.turret_kind) == kit {
            return;
        }
        let chassis = crate::sim::Chassis::of(kit.0);
        tank.hull_kind = kit.0;
        tank.turret_kind = kit.1;
        tank.chassis = chassis;
        tank.wheel_lift.resize(chassis.wheel_count(), 0.0);
    }

    /// The battle HUD faded out as the garage comes in, with the garage panel on top.
    /// `tank` is the one being driven, `queued` the kit for the next respawn.
    pub fn overlay(
        &self,
        mut hud: Vec<UiVertex>,
        tank: &Tank,
        queued: Loadout,
        profile: &Profile,
        height: f32,
    ) -> Layer<Hit> {
        let fade = 1.0 - self.presence();
        for v in &mut hud {
            v.color[3] *= fade;
        }
        if fade <= 0.0 {
            hud.clear();
        }
        let garage = self.draw(tank, queued, profile, height);
        hud.extend(garage.verts);
        Layer {
            verts: hud,
            hits: garage.hits,
        }
    }

    pub fn draw(&self, tank: &Tank, queued: Loadout, profile: &Profile, height: f32) -> Layer<Hit> {
        let e = smooth(self.blend);
        if e <= 0.0 {
            return Layer::empty();
        }
        let scale = height / REF_HEIGHT;
        let h = REF_HEIGHT;
        let mut p = Painter::new(scale);
        let mut hits = Hits::new();
        let levels = &profile.levels;

        backdrop(&mut p, h);
        let x = PANEL_X - (1.0 - e) * 30.0;
        p.rect(
            Vec2::new(x - PANEL_PAD, PANEL_TOP),
            Vec2::new(PANEL_W + 2.0 * PANEL_PAD, h - 2.0 * PANEL_TOP),
            PANEL_FILL,
        );

        let mut y = 64.0;
        label(&mut p, Vec2::new(x, y), "GARAGE", Align::Left);
        let cw = label(&mut p, Vec2::new(x + PANEL_W, y), "CR", Align::Right);
        p.text(
            Vec2::new(x + PANEL_W - cw - 8.0, y - 2.0),
            &profile.crystals.to_string(),
            &TextStyle::value(11.0),
            Align::Right,
        );
        y += 18.0;
        // The title is what you'll deploy in; the status says what the tank is showing.
        let title = format!("{} · {}", hull_name(queued.0), turret_name(queued.1));
        // Shrunk to fit the panel when the names are long.
        let mut ts = TextStyle::value(20.0);
        while ts.size > 12.0 && crate::ui::measure(&title, &ts) > PANEL_W {
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
        y += 18.0;
        p.line(
            Vec2::new(x, y),
            Vec2::new(x + PANEL_W, y),
            1.0,
            theme::FAINT,
        );
        y += 18.0;

        let hulls: Vec<Part> = HULLS.iter().map(|&k| Part::Hull(k)).collect();
        let turrets: Vec<Part> = TURRETS.iter().map(|&k| Part::Turret(k)).collect();
        let sections = [
            (
                "01",
                "HULL",
                hulls,
                Part::Hull(self.view.0),
                Part::Hull(queued.0),
                Part::Hull(tank.hull_kind),
                &HULL_STATS[..],
            ),
            (
                "02",
                "TURRET",
                turrets,
                Part::Turret(self.view.1),
                Part::Turret(queued.1),
                Part::Turret(tank.turret_kind),
                &TURRET_STATS[..],
            ),
        ];
        for (si, (num, name, items, current, equipped, in_use, stats)) in
            sections.iter().enumerate()
        {
            let focused = si == self.section;
            let mut s = TextStyle::LABEL;
            s.color = if focused { theme::FG } else { theme::DIM };
            p.text(Vec2::new(x, y), &format!("{num}  {name}"), &s, Align::Left);
            label(
                &mut p,
                Vec2::new(x + PANEL_W, y),
                &format!(
                    "{}/{}",
                    items.iter().position(|q| q == current).unwrap_or(0) + 1,
                    items.len()
                ),
                Align::Right,
            );
            y += 20.0;

            for part in items {
                self.draw_row(
                    &mut p,
                    &mut hits,
                    Vec2::new(x, y),
                    *part,
                    part == current,
                    focused,
                    part == equipped,
                    profile,
                );
                y += ROW_PITCH;
            }

            y += 4.0;
            for stat in stats.iter() {
                draw_stat(
                    &mut p,
                    Vec2::new(x, y),
                    stat,
                    *current,
                    *in_use,
                    items,
                    levels,
                );
                y += 20.0;
            }
            if si == 0 {
                y += 4.0;
                p.line(
                    Vec2::new(x, y),
                    Vec2::new(x + PANEL_W, y),
                    1.0,
                    theme::FAINT,
                );
                y += 18.0;
            }
        }

        // Buttons for the highlighted part: equip / buy, then upgrade.
        let part = self.focused();
        let mut by = h - 196.0;
        let (act, ready) = primary_action(part, queued, profile);
        self.button(
            &mut p,
            &mut hits,
            Vec2::new(x, by),
            "E",
            &act,
            ready,
            Input::Act,
        );
        by += BUTTON_H + 8.0;
        if let Some((up, ready)) = upgrade_action(part, profile) {
            self.button(
                &mut p,
                &mut hits,
                Vec2::new(x, by),
                "U",
                &up,
                ready,
                Input::Upgrade,
            );
        }

        let mut hy = h - 106.0;
        for (key, what, input) in [
            ("W/S", "BROWSE", None),
            ("A/D", "HULL / TURRET", None),
            ("ENTER", "DEPLOY", Some(Input::Deploy)),
            ("ESC", "CANCEL", Some(Input::Revert)),
        ] {
            let mut k = TextStyle::LABEL;
            k.color = theme::FG;
            p.text(Vec2::new(x, hy), key, &k, Align::Left);
            let w = label(&mut p, Vec2::new(x + 70.0, hy), what, Align::Left);
            if let Some(input) = input {
                hits.add(
                    &p,
                    Vec2::new(x - 4.0, hy - 3.0),
                    Vec2::new(78.0 + w, 14.0),
                    Hit::Key(input),
                );
            }
            hy += 14.0;
        }

        let mut v = p.finish();
        for vert in &mut v {
            vert.color[3] *= e;
        }
        // Closing: still fading out, but nothing left to click.
        if !self.open {
            hits.clear();
        }
        Layer { verts: v, hits }
    }

    /// One part in a list: a filled band when highlighted (brighter in the
    /// focused list), its name coloured by whether you own it, can buy it or
    /// are locked out, and on the right EQUIPPED, a price or a lock.
    #[allow(clippy::too_many_arguments)]
    fn draw_row(
        &self,
        p: &mut Painter,
        hits: &mut Hits<Hit>,
        at: Vec2,
        part: Part,
        selected: bool,
        focused: bool,
        equipped: bool,
        profile: &Profile,
    ) {
        let (x, y) = (at.x, at.y);
        let band_min = Vec2::new(x - PANEL_PAD + 4.0, y - 7.0);
        let band = Vec2::new(PANEL_W + 2.0 * PANEL_PAD - 8.0, ROW_PITCH - 2.0);
        hits.add(p, band_min, band, Hit::Part(part));
        p.set_halo(None);
        if selected {
            let fill = if focused { SELECT_FILL } else { HOVER_FILL };
            p.rect(band_min, band, fill);
            let bar = if focused { theme::FG } else { theme::DIM };
            p.rect(band_min, Vec2::new(3.0, band.y), bar);
        } else if self.hover == Some(Hit::Part(part)) {
            p.rect(band_min, band, HOVER_FILL);
        }
        p.set_halo(Some((theme::HALO, theme::HALO_PX)));

        let owned = profile.owns(part);
        let price = part.price().filter(|_| !owned);
        let locked = price.is_some_and(|pr| profile.standing_for(part) < pr.standing);
        let mut s = TextStyle::value(15.0);
        s.color = if owned {
            theme::FG
        } else if locked {
            LOCKED_NAME
        } else {
            theme::DIM
        };
        let nw = p.text(Vec2::new(x, y), part.name(), &s, Align::Left);

        // Right-hand tag, then the upgrade level and maker after the name in
        // whatever room is left.
        let right = Vec2::new(x + PANEL_W - 16.0, y + 4.0);
        let mut tag_w = 0.0;
        let mut tag = |p: &mut Painter, text: &str, color: Rgba| {
            let mut t = TextStyle::LABEL;
            t.color = color;
            tag_w = p.text(right, text, &t, Align::Right);
        };
        if equipped {
            tag(p, "EQUIPPED", EQUIPPED);
        } else if let Some(price) = price {
            if locked {
                tag(p, &format!("TRUST {}", price.standing), LOCKED);
                padlock(p, right - Vec2::new(tag_w + 16.0, 1.0), LOCKED);
                tag_w += 18.0;
            } else {
                let afford = profile.crystals >= price.crystals;
                let c = if afford { theme::FG } else { theme::DIM };
                tag(p, &format!("{} CR", price.crystals), c);
            }
        }
        if selected && focused {
            p.polyline(
                &[
                    Vec2::new(x + PANEL_W - 8.0, y + 2.0),
                    Vec2::new(x + PANEL_W - 3.0, y + 7.5),
                    Vec2::new(x + PANEL_W - 8.0, y + 13.0),
                ],
                1.2,
                theme::FG,
            );
        }
        let room = right.x - if tag_w > 0.0 { tag_w + 12.0 } else { 0.0 } - (x + nw + 12.0);
        let maker = part.maker().name();
        let level = format!("M{}", profile.levels.of(part));
        let note = [
            if owned {
                format!("{level}  {maker}")
            } else {
                maker.to_string()
            },
            if owned { level } else { String::new() },
        ]
        .into_iter()
        .find(|n| crate::ui::measure(n, &TextStyle::LABEL) <= room)
        .unwrap_or_default();
        if !note.is_empty() {
            label(p, Vec2::new(x + nw + 12.0, y + 4.0), &note, Align::Left);
        }
    }

    /// A full-width button: key on the left, what it does after it. Disabled
    /// ones are drawn faint and still say why (and can't be clicked).
    #[allow(clippy::too_many_arguments)]
    fn button(
        &self,
        p: &mut Painter,
        hits: &mut Hits<Hit>,
        at: Vec2,
        key: &str,
        text: &str,
        ready: bool,
        input: Input,
    ) {
        let size = Vec2::new(PANEL_W, BUTTON_H);
        let min = at - Vec2::new(0.0, 0.0);
        let hovered = ready && self.hover == Some(Hit::Key(input));
        p.set_halo(None);
        if ready {
            p.rect(min, size, if hovered { SELECT_FILL } else { HOVER_FILL });
        }
        p.set_halo(Some((theme::HALO, theme::HALO_PX)));
        p.rect_outline(min, size, 1.0, if ready { theme::FG } else { theme::FAINT });
        let ty = min.y + (BUTTON_H - TextStyle::LABEL.size) * 0.5;
        let mut k = TextStyle::LABEL;
        k.color = if ready { theme::HOT } else { theme::DIM };
        p.text(Vec2::new(min.x + 12.0, ty), key, &k, Align::Left);
        let mut a = TextStyle::LABEL;
        a.color = if ready { theme::FG } else { theme::DIM };
        p.text(Vec2::new(min.x + 36.0, ty), text, &a, Align::Left);
        if ready {
            hits.add(p, min, size, Hit::Key(input));
        }
    }
}

/// `kit` with `part` swapped in.
fn with_part(kit: Loadout, part: Part) -> Loadout {
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

/// What E does for `part` (equip it, or buy it), and whether it can right now.
fn primary_action(part: Part, queued: Loadout, profile: &Profile) -> (String, bool) {
    if profile.owns(part) {
        if with_part(queued, part) == queued {
            ("EQUIPPED".to_string(), false)
        } else {
            (format!("EQUIP {}", part.name()), true)
        }
    } else {
        match profile.can_buy(part) {
            Ok(cost) => (format!("BUY · {cost} CR"), true),
            Err(Refusal::Trust(f, s)) => {
                let have = profile.standing_for(part);
                // The longest that fits the button.
                let text = [
                    format!("LOCKED · {} TRUST {s} · YOU {have}", f.name()),
                    format!("NEEDS {} TRUST {s} · YOU {have}", f.name()),
                    format!("NEEDS TRUST {s} · YOU {have}"),
                ]
                .into_iter()
                .find(|t| crate::ui::measure(t, &TextStyle::LABEL) <= PANEL_W - 48.0)
                .unwrap_or_default();
                (text, false)
            }
            Err(Refusal::Crystals(cost)) => (format!("BUY · NEED {cost} CR"), false),
            Err(_) => (String::new(), false),
        }
    }
}

/// What U does for `part`, if it's owned, and whether it can right now.
fn upgrade_action(part: Part, profile: &Profile) -> Option<(String, bool)> {
    if !profile.owns(part) {
        return None;
    }
    let next = profile.levels.of(part) + 1;
    Some(match profile.can_upgrade(part) {
        Ok(cost) => (format!("UPGRADE TO M{next} · {cost} CR"), true),
        Err(Refusal::Crystals(cost)) => (format!("UPGRADE TO M{next} · NEED {cost} CR"), false),
        Err(_) => ("FULLY UPGRADED".to_string(), false),
    })
}

/// A slow orbit around `tank` at the garage's framing (tank right of the
/// panel), for the main menu. `angle` is the camera's world yaw around it.
pub fn orbit_camera(world: &World, tank: &Tank, angle: f32, aspect: f32) -> Camera {
    let focus = tank.pos + Vec3::Y * 1.0;
    let dir = Vec3::new(
        angle.sin() * ORBIT_DIST,
        ORBIT_HEIGHT,
        angle.cos() * ORBIT_DIST,
    );
    let len = dir.length();
    let hit = world.raycast(focus, dir / len, len);
    let pos = focus + dir / len * (hit - 0.6).max(3.0).min(len);
    let right = (focus - pos).cross(Vec3::Y).normalize_or_zero();
    let shift = (focus - pos).length() * 0.28 * (aspect / (16.0 / 9.0)).clamp(0.6, 1.2);
    Camera {
        pos,
        target: focus - right * shift,
        fov_y: 50f32.to_radians(),
    }
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
    let shade = [0.03, 0.03, 0.035, 0.72];
    let solid = PANEL_X + PANEL_W + 40.0;
    p.gradient_h(Vec2::ZERO, Vec2::new(solid, h), shade, shade);
    p.gradient_h(
        Vec2::new(solid, 0.0),
        Vec2::new(260.0, h),
        shade,
        [0.0, 0.0, 0.0, 0.0],
    );
}

/// Label, bar scaled to the kit's best, value; a faint tick where the part in use sits.
fn draw_stat(
    p: &mut Painter,
    pos: Vec2,
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
    let bar_w = 132.0;
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
    let right = pos + Vec2::new(PANEL_W, 0.0);
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
        p.set_trust_all(100_000);
        p.crystals = 100_000;
        for part in crate::economy::PARTS {
            let _ = p.buy(part);
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
        // Browsing highlights without equipping.
        assert_eq!(g.input(Input::Down, start, &p), None);
        assert_eq!(g.view, (HullKind::Kastron, TurretKind::Kontos));
        let l = (HullKind::Kastron, TurretKind::Kontos);
        assert_eq!(g.input(Input::Act, start, &p), Some(Event::Queue(l)));
        // E on the equipped part does nothing.
        assert_eq!(g.input(Input::Act, l, &p), None);
        // A and D pick a list, they don't toggle.
        g.input(Input::Right, l, &p);
        g.input(Input::Right, l, &p);
        assert_eq!(g.section, 1);
        // The list wraps: up from the first turret is the last one.
        assert_eq!(g.input(Input::Up, l, &p), None);
        let l2 = (HullKind::Kastron, *TURRETS.last().unwrap());
        assert_eq!(g.input(Input::Act, l, &p), Some(Event::Queue(l2)));
        assert_eq!(
            g.input(Input::Upgrade, l2, &p),
            Some(Event::Upgrade(Part::Turret(*TURRETS.last().unwrap())))
        );
        assert_eq!(g.input(Input::Revert, l2, &p), Some(Event::Queue(start)));
        assert!(!g.is_open());

        g.toggle(&w, &cam(&w), start);
        g.input(Input::Left, start, &p);
        assert_eq!(g.section, 0);
        assert_eq!(g.input(Input::Deploy, start, &p), None);
        assert!(!g.is_open());
    }

    #[test]
    fn unowned_parts_preview_and_e_buys() {
        let w = world();
        let mut p = Profile::in_memory();
        let mut g = Garage::new();
        let start = (HullKind::Kataphrakt, TurretKind::Kontos);
        g.toggle(&w, &cam(&w), start);
        g.input(Input::Right, start, &p);
        // Pelekys isn't owned: highlighted and previewed; E asks to buy it, U does nothing.
        assert_eq!(g.input(Input::Down, start, &p), None);
        assert_eq!(g.view, (HullKind::Kataphrakt, TurretKind::Pelekys));
        let pelekys = Part::Turret(TurretKind::Pelekys);
        assert_eq!(g.input(Input::Act, start, &p), Some(Event::Buy(pelekys)));
        assert_eq!(g.input(Input::Upgrade, start, &p), None);
        // Once bought it's equipped.
        p.set_trust_all(100_000);
        p.crystals = 100_000;
        p.buy(pelekys).unwrap();
        let kit = (HullKind::Kataphrakt, TurretKind::Pelekys);
        assert_eq!(g.bought(start, &p), Some(Event::Queue(kit)));
        assert_eq!(
            g.input(Input::Upgrade, kit, &p),
            Some(Event::Upgrade(pelekys))
        );
    }

    #[test]
    fn clicking_parts_and_buttons() {
        let w = world();
        let p = rich();
        let mut g = Garage::new();
        let start = (HullKind::Kataphrakt, TurretKind::Kontos);
        let hits = g.draw(&w.tanks[0], start, &p, 900.0).hits;
        assert!(hits.is_empty(), "closed: nothing to click");
        g.toggle(&w, &cam(&w), start);
        g.update(1.0);
        let click = |g: &mut Garage, hit: Hit| {
            let hits = g.draw(&w.tanks[0], start, &p, 900.0).hits;
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
        assert_eq!(
            click(&mut g, Hit::Part(Part::Turret(TurretKind::Pelekys))),
            None
        );
        assert_eq!(g.section, 1);
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
        let hits = g.draw(&w.tanks[0], start, &p, 900.0).hits;
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
        assert!(g.draw(&w.tanks[0], kit, &p, 900.0).verts.is_empty());
        g.toggle(&w, &cam(&w), kit);
        g.update(1.0);
        for (wd, ht) in [(1600.0, 900.0), (1024.0, 768.0), (3840.0, 2160.0)] {
            let v = g.draw(&w.tanks[0], kit, &p, ht).verts;
            assert!(!v.is_empty());
            assert!(v.iter().all(|v| v.pos[0] > -4.0
                && v.pos[0] < wd + 4.0
                && v.pos[1] > -4.0
                && v.pos[1] < ht + 4.0));
        }
    }
}
