//! Garage: from the main menu, or G mid-match to park and swing the camera
//! round your tank. Pick the hull and turret for your next respawn, and
//! upgrade parts with crystals. The tank you're driving doesn't change. Parts
//! you don't have yet say whose tanks to wreck to learn them, and how far
//! you've got.
//!
//! Every part is a picture in a strip along the bottom: hulls, then turrets.
//! Browsing only previews: the tank wears the highlighted kit (parts you don't
//! own too), and nothing is equipped until you press E (equip) on a part. U
//! upgrades the highlighted part. Deploying keeps what you equipped,
//! cancelling puts back what was equipped when you came in.
//!
//! Drawn as a dock along the bottom: the stats the highlighted part would
//! change, what you can do with it, and the strip. The tank has the rest.
//!
//! It opens in the ship's bay (`bay`): the camera swings round the tank where
//! it stands, the screen dips to black, and comes up on the tank turning on
//! the bay's turntable. Closing runs the same way back.

use glam::{Vec2, Vec3};

use crate::economy::{Levels, Part};
use crate::profile::{Profile, Refusal};
use crate::render::{Camera, UiVertex};
use crate::sim::{Chassis, HullKind, HullStats, Tank, TurretKind, World, wrap_angle};
use crate::thumbs::Thumbs;
use crate::ui::{Align, Hits, Layer, Painter, design, measure, theme, with_alpha};

const REF_HEIGHT: f32 = 900.0;
pub const PANEL_X: f32 = 64.0;
pub const PANEL_W: f32 = 340.0;
/// The dock along the bottom: its height, and its rows' tops under its own.
const DOCK_H: f32 = design::UNIT * 34.0;
const STATS_TOP: f32 = design::UNIT * 4.0;
const STRIP_TOP: f32 = design::UNIT * 13.0;
const FOOT_TOP: f32 = design::UNIT * 30.0;
/// The strip's cards: their gap, the one between hulls and turrets, and how
/// tall a card's picture and the whole of it are.
const CARD_GAP: f32 = design::UNIT;
const KIND_GAP: f32 = design::UNIT * 5.0;
const THUMB_H: f32 = design::UNIT * 9.0;
const CARD_H: f32 = THUMB_H + design::UNIT * 5.0;
/// What the actions, right of the stats, take of the dock's width.
const ACTIONS_W: f32 = design::UNIT * 44.0;
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

/// One of a part's stats.
struct Stat {
    name: &'static str,
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
        decimals: 0,
        get: |p, l| l.max_health(hull_of(p)),
    },
    Stat {
        name: "TOP SPEED",
        decimals: 0,
        get: |p, l| l.hull(hull_of(p)).max_speed * 3.6,
    },
    Stat {
        name: "ACCEL",
        decimals: 1,
        get: |p, l| l.hull(hull_of(p)).accel,
    },
    Stat {
        name: "TURN",
        decimals: 0,
        get: |p, l| l.hull(hull_of(p)).turn_rate.to_degrees(),
    },
    Stat {
        name: "TRAVERSE",
        decimals: 0,
        get: |p, _| HullStats::of(hull_of(p)).turret_speed.to_degrees(),
    },
    Stat {
        name: "MASS",
        decimals: 1,
        get: |p, l| crate::sim::Chassis::of(hull_of(p)).mass * l.hull(hull_of(p)).heft / 1000.0,
    },
    Stat {
        // Seconds of beam fire it adds before overheating.
        name: "THERMAL",
        decimals: 1,
        get: |p, _| crate::combat::thermal_mass(hull_of(p)),
    },
];

const TURRET_STATS: [Stat; 3] = [
    Stat {
        name: "DPS",
        decimals: 0,
        get: |p, l| l.gun(turret_of(p)).dps(),
    },
    Stat {
        name: "PER HIT",
        decimals: 0,
        // A beam has no single hit: show what one second of contact does.
        get: |p, l| l.gun(turret_of(p)).damage,
    },
    Stat {
        name: "RANGE",
        decimals: 0,
        get: |p, l| l.gun(turret_of(p)).range(),
    },
];

/// Navigation input, mapped from keys by the app.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    /// Left and right along the strip; up and down between hulls and turrets.
    Up,
    Down,
    Left,
    Right,
    /// Between hulls and turrets.
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
    /// A button: clicking it presses its key.
    Key(Input),
}

pub struct Garage {
    open: bool,
    /// 0 = closed .. 1 = fully open; eased toward `open`.
    blend: f32,
    /// Which kind's highlighted: 0 = hull, 1 = turret.
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
    thumbs: Thumbs,
}

fn tab_parts(section: usize) -> Vec<Part> {
    if section == 0 {
        HULLS.iter().map(|&k| Part::Hull(k)).collect()
    } else {
        TURRETS.iter().map(|&k| Part::Turret(k)).collect()
    }
}

/// Every part, in the strip's order: hulls, then turrets.
fn strip_parts() -> Vec<Part> {
    (0..2).flat_map(tab_parts).collect()
}

/// The strip on a screen `w` wide: each part's card, from the strip's top
/// left, and how wide they are.
fn strip(w: f32) -> (Vec<(Part, f32)>, f32) {
    let parts = strip_parts();
    let gaps = CARD_GAP * (parts.len() - 2) as f32 + KIND_GAP;
    let card = ((w - design::MARGIN * 2.0 - gaps) / parts.len() as f32).floor();
    let mut x = design::MARGIN;
    let mut out = Vec::new();
    for (i, &part) in parts.iter().enumerate() {
        if i == HULLS.len() {
            x += KIND_GAP - CARD_GAP;
        }
        out.push((part, x));
        x += card + CARD_GAP;
    }
    (out, card)
}

/// How far along `part` is to its next step, 0..1: full once that can be
/// bought (or there's none left).
fn progress(part: Part, profile: &Profile) -> f32 {
    let known = profile.known.of(part);
    let cap = profile.known.cap(part);
    if cap > profile.levels.of(part) || cap >= crate::economy::MAX_LEVEL {
        1.0
    } else {
        known - known.floor()
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
    pub fn bay_camera(&self) -> crate::render::Camera {
        let k = smooth(((self.blend - BAY_AT) / (1.0 - BAY_AT)).clamp(0.0, 1.0));
        let c = Chassis::of(self.view.0);
        let length = (c.front.0 + c.front.2) - (c.rear.0 - c.rear.2);
        let fit = (length / BAY_FRAMED_LENGTH).max(1.0);
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
        // On whatever differs from what's equipped.
        self.section = usize::from(kit.0 == self.entry.0 && kit.1 != self.entry.1);
    }

    /// The wheel: along the strip, down to the right.
    pub fn scroll(&mut self, delta: f32) {
        if delta != 0.0 {
            self.move_by(delta.signum() as isize);
        }
    }

    /// Move the highlight `n` along the strip, stopping at its ends.
    fn move_by(&mut self, n: isize) {
        let parts = strip_parts();
        let at = parts.iter().position(|&p| p == self.focused()).unwrap_or(0);
        let to = (at as isize + n).clamp(0, parts.len() as isize - 1);
        self.highlight(parts[to as usize]);
    }

    /// The highlighted part.
    fn focused(&self) -> Part {
        if self.section == 0 {
            Part::Hull(self.view.0)
        } else {
            Part::Turret(self.view.1)
        }
    }

    /// Highlight `part`.
    fn highlight(&mut self, part: Part) {
        match part {
            Part::Hull(k) => (self.section, self.view.0) = (0, k),
            Part::Turret(k) => (self.section, self.view.1) = (1, k),
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
            Input::Left => {
                self.move_by(-1);
                None
            }
            Input::Right => {
                self.move_by(1);
                None
            }
            Input::Switch | Input::Up | Input::Down => {
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

    /// A click on `hit`: a card highlights it, an action presses its key.
    pub fn click(&mut self, hit: Hit, queued: Loadout, profile: &Profile) -> Option<Event> {
        match hit {
            Hit::Key(input) => self.input(input, queued, profile),
            Hit::Part(part) if self.open => {
                self.highlight(part);
                None
            }
            Hit::Part(_) => None,
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
    }

    /// Draw up to `budget` of the cards' thumbnails while the garage shows,
    /// for a screen `width` by `height` px with `queued` equipped.
    #[allow(clippy::too_many_arguments)]
    pub fn render_thumbs(
        &mut self,
        r: &mut crate::render::Renderer,
        art: &crate::tank::TankArt,
        world: &World,
        tank: &Tank,
        queued: Loadout,
        width: f32,
        height: f32,
        budget: usize,
    ) {
        if self.blend <= 0.0 {
            return;
        }
        let scale = height / REF_HEIGHT;
        let cell = (
            (strip(width / scale).1 * scale).ceil() as u32,
            (THUMB_H * scale).ceil() as u32,
        );
        self.thumbs
            .update(r, art, world, tank, queued, cell, budget);
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
        // Aim under the tank, so it sits in the space over the dock.
        let target = focus - Vec3::Y * (focus - pos).length() * crate::bay::AIM_UNDER;
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
        use design::{AMBER, BLUE, GROUND, HEAD, MARGIN, SMALL, TEXT, TEXT_2};
        let e = smooth(self.blend);
        if e <= 0.0 {
            return Layer::empty();
        }
        let scale = height / REF_HEIGHT;
        let h = REF_HEIGHT;
        let w = width / scale;
        let mut p = Painter::new(scale);
        let mut hits = Hits::new();

        // Mid-battle, when you'll get what you've changed; and what you have
        // to spend.
        if queued != (tank.hull_kind, tank.turret_kind) {
            let at = Vec2::splat(MARGIN);
            p.text(at, "On your next respawn", &design::BODY, Align::Left);
        }
        let at = Vec2::new(w - MARGIN, MARGIN);
        let cr = format!("{} cr", profile.crystals);
        p.text(at, &cr, &HEAD.colored(BLUE), Align::Right);

        // The dock, sliding up.
        p.set_halo(None);
        let top = h - DOCK_H + (1.0 - e) * 30.0;
        p.rect(
            Vec2::new(0.0, top),
            Vec2::new(w, DOCK_H.min(h - top)),
            GROUND,
        );
        self.draw_stats(
            &mut p,
            Vec2::new(MARGIN, top + STATS_TOP),
            w,
            queued,
            profile,
        );
        let at = Vec2::new(w - MARGIN, top + STATS_TOP);
        self.draw_actions(&mut p, &mut hits, at, queued, profile);
        self.draw_strip(&mut p, &mut hits, top + STRIP_TOP, w, queued, profile);

        // Back out, putting the kit back as Esc does; or keep it as it stands.
        let foot = top + FOOT_TOP;
        for (name, input, x, align, color) in [
            ("Back", Input::Revert, MARGIN, Align::Left, TEXT_2),
            ("Done", Input::Deploy, w - MARGIN, Align::Right, AMBER),
        ] {
            let on = self.hover == Some(Hit::Key(input));
            let style = SMALL.colored(if on { TEXT } else { color });
            let tw = p.text(Vec2::new(x, foot), name, &style, align);
            let left = if align == Align::Right { x - tw } else { x };
            let pad = Vec2::splat(design::UNIT * 1.5);
            let min = Vec2::new(left, foot) - pad;
            let size = Vec2::new(tw, SMALL.size) + pad * 2.0;
            hits.add(&p, min, size, Hit::Key(input));
        }

        let mut v = p.finish();
        for vert in &mut v {
            vert.color[3] *= e;
        }
        // The dip to black for the cut to and from the bay, under the dock.
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

    /// Every part's card along the strip, `y` its top: its picture (dark if
    /// it isn't yours), its name, and how far along it is.
    fn draw_strip(
        &self,
        p: &mut Painter,
        hits: &mut Hits<Hit>,
        y: f32,
        w: f32,
        queued: Loadout,
        profile: &Profile,
    ) {
        use design::{AMBER, SMALL, TEXT, TEXT_2, UNIT};
        let (cards, card) = strip(w);
        for (part, x) in cards {
            let min = Vec2::new(x, y);
            let size = Vec2::new(card, CARD_H);
            let owned = profile.owns(part);
            let equipped = with_part(queued, part) == queued;
            let picked = part == self.focused();
            let thumb = Vec2::new(card, THUMB_H);
            match self.thumbs.uv(part) {
                Some((u0, u1)) => {
                    let k = if owned { 1.0 } else { 0.25 };
                    p.image(min, thumb, u0, u1, [k, k, k, 1.0]);
                }
                None => p.rect(min, thumb, with_alpha(TEXT, 0.02)),
            }
            if picked {
                p.rect_outline(min, thumb, 2.0, TEXT);
            } else if self.hover == Some(Hit::Part(part)) {
                p.rect_outline(min, thumb, 1.0, TEXT_2);
            }
            // Shrunk to fit a narrow screen's cards.
            let name = part.name();
            let mut style = SMALL.colored(match (equipped, owned) {
                (true, _) => AMBER,
                (_, true) => TEXT,
                _ => TEXT_2,
            });
            let fit = card / measure(name, &style);
            style.size *= fit.min(1.0);
            style.tracking *= fit.min(1.0);
            let at = min + Vec2::Y * (THUMB_H + UNIT);
            p.text(at, name, &style, Align::Left);
            let bar = min + Vec2::Y * (THUMB_H + UNIT * 3.0);
            p.rect(bar, Vec2::new(card, 2.0), with_alpha(TEXT_2, 0.3));
            let k = progress(part, profile);
            let color = if k >= 1.0 { AMBER } else { TEXT };
            p.rect(bar, Vec2::new(card * k, 2.0), color);
            hits.add(p, min, size, Hit::Part(part));
        }
    }

    /// The highlighted part's stats from `at`: against what's equipped, or on
    /// what's equipped, against its next step, if that's known.
    fn draw_stats(&self, p: &mut Painter, at: Vec2, w: f32, queued: Loadout, profile: &Profile) {
        use design::{BODY, SMALL, TEXT, TEXT_2, UNIT};
        let part = self.focused();
        let (stats, equipped) = match part {
            Part::Hull(_) => (&HULL_STATS[..], Part::Hull(queued.0)),
            Part::Turret(_) => (&TURRET_STATS[..], Part::Turret(queued.1)),
        };
        let level = profile.levels.of(part);
        let mut next = profile.levels;
        if part == equipped && profile.known.cap(part) > level {
            next.set(part, level + 1);
        }
        let room = w - design::MARGIN * 2.0 - ACTIONS_W;
        let col = (room / stats.len() as f32).min(UNIT * 20.0);
        for (i, stat) in stats.iter().enumerate() {
            let at = at + Vec2::X * col * i as f32;
            p.text(at, stat.name, &SMALL.colored(TEXT_2), Align::Left);
            let show = |v: f32| format!("{v:.prec$}", prec = stat.decimals);
            let was = show((stat.get)(equipped, &profile.levels));
            let now = show((stat.get)(part, &next));
            let (from, to) = if was == now {
                (String::new(), now)
            } else {
                (format!("{was} → "), now)
            };
            let mut style = BODY;
            let fit = (col - UNIT) / measure(&format!("{from}{to}"), &style);
            style.size *= fit.min(1.0);
            style.tracking *= fit.min(1.0);
            let y = at + Vec2::Y * UNIT * 2.5;
            let x = p.text(y, &from, &style.colored(TEXT_2), Align::Left);
            p.text(y + Vec2::X * x, &to, &style.colored(TEXT), Align::Left);
        }
    }

    /// What you can do with the highlighted part, right-aligned at `at`: its
    /// name and tier, then each thing in words.
    fn draw_actions(
        &self,
        p: &mut Painter,
        hits: &mut Hits<Hit>,
        at: Vec2,
        queued: Loadout,
        profile: &Profile,
    ) {
        use design::{AMBER, BODY, TEXT, TEXT_2, UNIT};
        let part = self.focused();
        let mut title = part.name().to_string();
        if profile.owns(part) {
            let tier = crate::economy::level_label(profile.levels.of(part));
            title = format!("{title} {tier}");
        }
        p.text(at, &title, &BODY.colored(TEXT), Align::Right);
        let mut x = at.x;
        let y = at.y + UNIT * 3.5;
        for (text, input) in actions(part, queued, profile) {
            let style = BODY.colored(match input {
                None => TEXT_2,
                Some(i) if self.hover == Some(Hit::Key(i)) => TEXT,
                Some(_) => AMBER,
            });
            let tw = p.text(Vec2::new(x, y), &text, &style, Align::Right);
            if let Some(input) = input {
                let pad = Vec2::splat(UNIT);
                let min = Vec2::new(x - tw, y) - pad;
                let size = Vec2::new(tw, style.size) + pad * 2.0;
                hits.add(p, min, size, Hit::Key(input));
            }
            x -= tw + UNIT * 4.0;
        }
    }
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
        Ok(cost) => (format!("Upgrade {cost} cr"), Some(Input::Upgrade)),
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

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Known further: the bar's full, and crystals decide.
        p.known.set(kastron, 2.5);
        p.crystals = 0;
        assert_eq!(progress(kastron, &p), 1.0);
        assert!(words(&p, kastron)[1].0.contains("costs"));
        p.crystals = 100_000;
        assert!(words(&p, kastron)[1].1);
    }

    /// Every part is on the strip, on screen, at any shape; the keys run
    /// along it and stop at its ends.
    #[test]
    fn the_strip_holds_every_part() {
        for w in [1125.0, 1200.0, 1600.0, 3200.0] {
            let (cards, card) = strip(w);
            assert_eq!(cards.len(), HULLS.len() + TURRETS.len());
            assert!(card > 60.0);
            assert!(cards.windows(2).all(|c| c[1].1 - c[0].1 >= card));
            assert!(cards.last().unwrap().1 + card <= w - design::MARGIN + 1.0);
        }
        let w = world();
        let p = rich();
        let mut g = Garage::new();
        let start = (HullKind::Kataphrakt, TurretKind::Kontos);
        g.toggle(&w, &cam(&w), start);
        g.input(Input::Left, start, &p);
        assert_eq!(g.view, start);
        for _ in 0..20 {
            g.input(Input::Right, start, &p);
        }
        assert_eq!(g.focused(), *strip_parts().last().unwrap());
        g.scroll(-1.0);
        assert_eq!(g.focused(), strip_parts()[10]);
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
        // Tab goes to the turrets; right of Kontos is the next.
        g.input(Input::Switch, l, &p);
        assert_eq!(g.section, 1);
        g.input(Input::Right, l, &p);
        let next = Part::Turret(TURRETS[1]);
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
    fn clicking_cards_and_actions() {
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
        let pelekys = Hit::Part(Part::Turret(TurretKind::Pelekys));
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
    fn actions_that_cant_be_done_cant_be_clicked() {
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
