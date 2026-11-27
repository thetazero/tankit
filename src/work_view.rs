//! Finding work: the board, a table of the contracts on offer to compare and
//! filter (`Board`), and the deal, one of them to take or not: your side's
//! tanks facing the enemy's under the sky of the system they'd fight over,
//! what you'd gain, and the contract you'd leave for it.

use std::cell::Cell;
use std::f32::consts::FRAC_PI_2;

use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

use crate::economy::level_label;
use crate::faction::Faction;
use crate::galaxy::theater::Offer;
use crate::galaxy::{RAID, SIDES};
use crate::garage::{self, Loadout};
use crate::menu::{Hit, Nav, back_at_foot};
use crate::render::mesh::{Instance, MeshId};
use crate::render::{Atmosphere, Camera, PointLight, Scene};
use crate::sim::Tank;
use crate::star::Star;
use crate::tank::TankArt;
use crate::theater_view::{self, MAX_PLANETS, SPACE_R, SystemArt};
use crate::ui::design::{self, AMBER, BLUE, BODY, HEAD, MARGIN, SMALL, TEXT, TEXT_2, UNIT};
use crate::ui::{Align, Hits, Painter, Rgba, emblem, with_alpha};

/// What the board can be sorted by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sort {
    Strength,
    Gear,
    Pay,
}

const SORTS: [(Sort, &str); 3] = [
    (Sort::Strength, "Strength"),
    (Sort::Gear, "Gear"),
    (Sort::Pay, "Pay"),
];

/// The filters, in order: who you'd work for, who you'd fight, the job.
/// A group with none on takes any.
const FILTERS: usize = 2 * SIDES + JOBS.len();
const JOBS: [&str; 3] = ["Take", "Hold", "Plunder"];

/// Which of `JOBS` an offer is.
fn kind(o: &Offer) -> usize {
    match o.side {
        RAID => 2,
        _ => o.defend as usize,
    }
}

/// The contracts on offer, and how they're filtered, sorted and picked.
#[derive(Clone, Debug, Default)]
pub struct Board {
    offers: Vec<Offer>,
    /// The contract you're on.
    pub mine: Option<Offer>,
    filters: [bool; FILTERS],
    /// By what, and whether most first.
    sort: Option<(Sort, bool)>,
    /// The offer picked, by its ship.
    pick: Option<u32>,
    /// The filter the keys are on, when they're on the filters.
    focus: Option<usize>,
    /// The first row in view, and how many fit, as last drawn.
    first: Cell<usize>,
    fit: Cell<usize>,
    /// None on offer, and the war's going on until there's one.
    pub waiting: bool,
}

impl Board {
    /// The offers now; the same one stays picked while it's listed.
    pub fn set(&mut self, offers: Vec<Offer>, mine: Option<Offer>) {
        self.offers = offers;
        self.mine = mine;
        self.settle();
    }

    /// Something listed is always picked, if anything's listed.
    fn settle(&mut self) {
        let listed = self.listed();
        if !listed.iter().any(|o| Some(o.id) == self.pick) {
            self.pick = listed.first().map(|o| o.id);
        }
    }

    /// Nothing's on offer.
    pub fn empty(&self) -> bool {
        self.offers.is_empty()
    }

    pub fn offer(&self, id: u32) -> Option<&Offer> {
        self.offers.iter().find(|o| o.id == id)
    }

    pub fn picked(&self) -> Option<&Offer> {
        self.offer(self.pick?)
    }

    /// The offers the filters let through, sorted.
    pub fn listed(&self) -> Vec<&Offer> {
        let any = |group: &[bool], i: usize| group[i] || group.iter().all(|f| !f);
        let (work, rest) = self.filters.split_at(SIDES);
        let (fight, job) = rest.split_at(SIDES);
        let mut out: Vec<&Offer> = self
            .offers
            .iter()
            .filter(|o| any(work, o.side) && any(fight, o.enemy) && any(job, kind(o)))
            .collect();
        if let Some((by, most)) = self.sort {
            let key = |o: &Offer| match by {
                Sort::Strength => o.strength(),
                Sort::Gear => o.tier as f32,
                Sort::Pay => o.wage as f32,
            };
            out.sort_by(|a, b| {
                let o = key(a).total_cmp(&key(b));
                if most { o.reverse() } else { o }
            });
        }
        out
    }

    pub fn toggle(&mut self, k: usize) {
        if let Some(f) = self.filters.get_mut(k) {
            *f = !*f;
        }
        self.settle();
    }

    /// Sort by `SORTS[k]`: most first, then least, then not at all. The view
    /// and the row picked stay put: the offers move under them.
    pub fn sort_by(&mut self, k: usize) {
        let Some(&(by, _)) = SORTS.get(k) else { return };
        let row = self.row();
        self.sort = match self.sort {
            Some((s, true)) if s == by => Some((by, false)),
            Some((s, false)) if s == by => None,
            _ => Some((by, true)),
        };
        self.pick = self.listed().get(row).map(|o| o.id);
    }

    /// Pick the `i`th listed. True if that's a change.
    pub fn pick_row(&mut self, i: usize) -> bool {
        self.focus = None;
        let Some(id) = self.listed().get(i).map(|o| o.id) else {
            return false;
        };
        self.pick.replace(id) != Some(id)
    }

    fn row(&self) -> usize {
        let listed = self.listed();
        listed
            .iter()
            .position(|o| Some(o.id) == self.pick)
            .unwrap_or(0)
    }

    /// Move the pick `by` rows, stopping at the ends.
    fn step(&mut self, by: i32) {
        let last = self.listed().len() as i32 - 1;
        self.pick_row((self.row() as i32 + by).clamp(0, last.max(0)) as usize);
    }

    /// Scroll the view `by` rows, stopping at the ends, the pick kept in it.
    /// True if it moved.
    pub fn scroll(&mut self, by: i32) -> bool {
        let fit = self.fit.get().max(1);
        let last = self.listed().len().saturating_sub(fit);
        let was = self.first.get();
        let first = (was as i32 + by).clamp(0, last as i32) as usize;
        self.first.set(first);
        let row = self.row().clamp(first, first + fit - 1);
        self.pick = self.listed().get(row).map(|o| o.id);
        first != was
    }

    /// Keys on the board. Some offer, if one's opened.
    pub fn input(&mut self, nav: Nav) -> Option<u32> {
        match (self.focus, nav) {
            (None, Nav::Up) => {
                self.step(-1);
            }
            (None, Nav::Down) => {
                self.step(1);
            }
            (None, Nav::Left) => self.focus = Some(0),
            (None, Nav::Select) => return self.picked().map(|o| o.id),
            (Some(k), Nav::Up) => self.focus = Some((k + FILTERS - 1) % FILTERS),
            (Some(k), Nav::Down) => self.focus = Some((k + 1) % FILTERS),
            (Some(k), Nav::Select) => self.toggle(k),
            (Some(_), Nav::Right) => self.focus = None,
            _ => {}
        }
        None
    }

    /// The board over a `w` by `h` screen, the pointer `over` something on it.
    pub fn draw(
        &self,
        p: &mut Painter,
        hits: &mut Hits<Hit>,
        over: Option<Hit>,
        time: f32,
        w: f32,
        h: f32,
    ) {
        p.set_halo(None);
        p.rect(Vec2::ZERO, Vec2::new(w, h), [0.0, 0.0, 0.0, 0.95]);
        design::panel(p, h);
        self.draw_filters(p, hits, over);
        back_at_foot(p, hits, over == Some(Hit::Back), h);

        let x0 = MARGIN * 3.0 + design::COLUMN;
        let cols = Columns::new(x0, w - MARGIN);
        let mut y = MARGIN;
        for (k, (by, name)) in SORTS.into_iter().enumerate() {
            let on = self.sort.is_some_and(|(s, _)| s == by);
            let color = if on { AMBER } else { TEXT_2 };
            let style = SMALL.colored(design::hovered(color, over == Some(Hit::Sort(k))));
            let (x, align) = match by {
                Sort::Strength => (cols.bar.0, Align::Left),
                Sort::Gear => (cols.gear, Align::Left),
                Sort::Pay => (cols.win, Align::Right),
            };
            let tw = p.text(Vec2::new(x, y), name, &style, align);
            // Beside its name, which way it's sorted: down for most first.
            let (left, arrow) = match align {
                Align::Right => (x - tw - ARROW, x - tw - ARROW * 0.5),
                _ => (x, x + tw + ARROW * 0.5),
            };
            if let Some((_, most)) = self.sort.filter(|_| on) {
                let c = Vec2::new(arrow, y + SMALL.size * 0.5);
                let tip = if most { 1.0 } else { -1.0 };
                let pts = [(-5.0, -4.0), (5.0, -4.0), (0.0, 4.0)]
                    .map(|(dx, dy)| c + Vec2::new(dx, dy * tip));
                p.poly(&pts, style.color);
            }
            let min = Vec2::new(left, y) - Vec2::splat(UNIT);
            let size = Vec2::new(tw + ARROW, SMALL.size) + Vec2::splat(UNIT * 2.0);
            hits.add(p, min, size, Hit::Sort(k));
        }
        let style = SMALL.colored(TEXT_2);
        p.text(Vec2::new(x0, y), "For", &style, Align::Left);
        p.text(Vec2::new(cols.enemy, y), "Vs", &style, Align::Center);
        p.text(Vec2::new(cols.job, y), "Job", &style, Align::Left);
        y += UNIT * 4.0;

        // The contract you're on, to compare with.
        if let Some(mine) = &self.mine {
            draw_row(p, &cols, y, mine, AMBER, false);
            y += ROW;
            let rule = with_alpha(AMBER, 0.6);
            p.rect(Vec2::new(x0, y), Vec2::new(cols.win - x0, 1.0), rule);
            y += UNIT * 2.0;
        }
        let listed = self.listed();
        if listed.is_empty() {
            let mut at = Vec2::new(x0, y + UNIT * 3.0);
            let none = if self.waiting {
                let c = at + Vec2::new(8.0, BODY.size * 0.5);
                crate::hud::spinner_small(p, c, time);
                at.x += UNIT * 4.0;
                "Waiting for contracts"
            } else if self.offers.is_empty() {
                "No contracts on offer"
            } else {
                "None match your filters"
            };
            p.text(at, none, &BODY.colored(TEXT_2), Align::Left);
            return;
        }
        // As many rows as fit, from where it's scrolled, the picked one among them.
        let fit = (((h - MARGIN - y) / ROW) as usize).max(1);
        let picked = self.row();
        let first = self.first.get().min(listed.len().saturating_sub(fit));
        let first = first.clamp((picked + 1).saturating_sub(fit), picked);
        self.first.set(first);
        self.fit.set(fit);
        for (i, o) in listed.iter().enumerate().skip(first).take(fit) {
            let min = Vec2::new(x0 - UNIT * 2.0, y);
            let size = Vec2::new(cols.win - x0 + UNIT * 4.0, ROW);
            let on = i == picked;
            if on {
                p.rect(min, size, with_alpha(TEXT, 0.014));
                let bar = Vec2::new(3.0, ROW);
                let k = if self.focus.is_none() { 1.0 } else { 0.3 };
                p.rect(min, bar, with_alpha(AMBER, k));
            }
            draw_row(p, &cols, y, o, TEXT, over == Some(Hit::Row(i)));
            hits.add(p, min, size, Hit::Row(i));
            y += ROW;
        }
    }

    fn draw_filters(&self, p: &mut Painter, hits: &mut Hits<Hit>, over: Option<Hit>) {
        let x = MARGIN;
        let mut y = MARGIN;
        p.text(Vec2::new(x, y), "Find work", &HEAD, Align::Left);
        y += UNIT * 9.0;
        for (name, from, n) in [
            ("Work for", 0, SIDES),
            ("Fight", SIDES, SIDES),
            ("Job", 2 * SIDES, JOBS.len()),
        ] {
            let at = Vec2::new(x, y);
            p.text(at, name, &SMALL.colored(TEXT_2), Align::Left);
            y += UNIT * 3.0;
            let any = self.filters[from..from + n].iter().all(|f| !f);
            // Words get a line each.
            let words = from == 2 * SIDES;
            let pitch = match words {
                true => design::COLUMN,
                false => design::COLUMN / n as f32,
            };
            let size = Vec2::new(pitch - UNIT, UNIT * 5.0);
            for i in 0..n {
                let k = from + i;
                let min = match words {
                    true => Vec2::new(x, y + size.y * i as f32),
                    false => Vec2::new(x + pitch * i as f32, y),
                };
                let on = self.filters[k];
                let lit = if on || any { 1.0 } else { 0.25 };
                let hover = over == Some(Hit::Filter(k));
                // Emblems have no text to darken, so the box shows it too.
                let fill = match (on, hover) {
                    (true, _) => 0.03,
                    (false, true) => 0.014,
                    (false, false) => 0.0,
                };
                if fill > 0.0 {
                    p.rect(min, size, with_alpha(TEXT, fill));
                }
                if hover {
                    p.rect_outline(min, size, 1.0, TEXT_2);
                }
                if self.focus == Some(k) {
                    p.rect_outline(min, size, 1.0, AMBER);
                }
                let c = min + size * 0.5;
                if from < 2 * SIDES {
                    let f = Faction::side(i);
                    emblem::emblem(p, f, c, 22.0, 1.5, with_alpha(f.ui(), lit));
                } else {
                    let name = JOBS[i];
                    let style = BODY.colored(design::hovered(with_alpha(TEXT, lit), hover));
                    let at = Vec2::new(x + UNIT * 1.5, c.y - BODY.size * 0.5);
                    p.text(at, name, &style, Align::Left);
                }
                hits.add(p, min, size, Hit::Filter(k));
            }
            y += size.y + UNIT * 5.0;
        }
    }
}

const ROW: f32 = UNIT * 8.0;
/// The room beside a sort's name for its arrow.
const ARROW: f32 = UNIT * 2.5;

/// Where the board's columns are.
struct Columns {
    side: f32,
    enemy: f32,
    job: f32,
    bar: (f32, f32),
    gear: f32,
    win: f32,
}

impl Columns {
    fn new(x0: f32, x1: f32) -> Columns {
        let at = |k: f32| x0 + (x1 - x0) * k;
        Columns {
            side: x0 + UNIT * 1.5,
            enemy: x0 + UNIT * 9.0,
            job: x0 + UNIT * 16.0,
            bar: (at(0.46), at(0.72)),
            gear: at(0.8),
            win: x1,
        }
    }
}

/// An offer's row, its top at `y`, darker under the pointer (`hover`).
fn draw_row(p: &mut Painter, c: &Columns, y: f32, o: &Offer, color: Rgba, hover: bool) {
    let color = design::hovered(color, hover);
    let mid = y + ROW * 0.5;
    let top = mid - BODY.size * 0.5;
    for (x, f) in [(c.side, o.side), (c.enemy, o.enemy)] {
        let f = Faction::side(f);
        emblem::emblem(p, f, Vec2::new(x, mid), 22.0, 1.4, f.ui());
    }
    let at = Vec2::new(c.job, top);
    p.text(at, &o.job(), &BODY.colored(color), Align::Left);
    share(p, o, Vec2::new(c.bar.0, mid), c.bar.1 - c.bar.0);
    let gear = BODY.colored(design::hovered(TEXT, hover));
    let at = Vec2::new(c.gear, top);
    p.text(at, &level_label(o.tier), &gear, Align::Left);
    let pay = o.wage.to_string();
    let at = Vec2::new(c.win, top);
    p.text(at, &pay, &BODY.colored(BLUE), Align::Right);
}

/// Your side's share of the hulls, the same on every row whoever's in it:
/// filled for yours, with a mark at even. `at` is its left end's middle.
fn share(p: &mut Painter, o: &Offer, at: Vec2, w: f32) {
    let h = 6.0;
    let min = at - Vec2::Y * h * 0.5;
    p.rect(min, Vec2::new(w, h), with_alpha(TEXT_2, 0.2));
    let ahead = o.strength() >= 0.5;
    let fill = if ahead { TEXT } else { TEXT_2 };
    p.rect(min, Vec2::new(w * o.strength(), h), fill);
    let mark = Vec2::new(at.x + w * 0.5 - 1.0, at.y - h);
    p.rect(mark, Vec2::new(2.0, h * 2.0), TEXT);
}

/// How it's going: the side's hulls, its mercs' in amber, and the enemy's.
fn strength(p: &mut Painter, o: &Offer, at: Vec2, w: f32, h: f32) {
    let total = (o.hulls + o.foes).max(1) as f32;
    let mut x = at.x;
    for (n, color) in [
        (o.hulls - o.mercs, Faction::side(o.side).ui()),
        (o.mercs, AMBER),
        (o.foes, Faction::side(o.enemy).ui()),
    ] {
        if n > 0 {
            let wide = w * n as f32 / total;
            let size = Vec2::new((wide - 2.0).max(1.0), h);
            p.rect(Vec2::new(x, at.y), size, color);
            x += wide;
        }
    }
}

/// Where the enemy's tanks stand, and your side's: yours, then your employer's.
fn stands(enemy: Faction) -> Vec<(Loadout, Vec3)> {
    let (hulls, turrets) = (enemy.hulls(), enemy.turrets());
    let n = hulls.len().max(turrets.len());
    (0..n)
        .map(|i| {
            let kit = (hulls[i % hulls.len()], turrets[i % turrets.len()]);
            let x = if n == 1 { 6.0 } else { 3.4 + 5.6 * i as f32 };
            (kit, Vec3::new(x, 0.0, -0.8 * i as f32))
        })
        .collect()
}

const YOURS: Vec3 = Vec3::new(-3.6, 0.0, 0.0);
const THEIRS: Vec3 = Vec3::new(-9.8, 0.0, -2.6);
/// How far each tank's turned to the camera from facing the other side.
const TURNED: f32 = 0.65;

/// The system's sky: where its star and worlds hang, how far back, and how
/// big, in halves of the screen from its middle, seen with no sway.
const STAR: (Vec2, f32) = (Vec2::new(-0.78, 0.46), 0.055);
const WORLDS: [(Vec2, f32); MAX_PLANETS] = [
    (Vec2::new(0.06, 0.3), 0.29),
    (Vec2::new(0.6, 0.48), 0.12),
    (Vec2::new(-0.4, 0.5), 0.08),
];
const SKY_Z: f32 = -156.0;

/// Where in the sky `(at, size)` is, and its radius.
fn in_sky((at, size): (Vec2, f32), aspect: f32) -> (Vec3, f32) {
    let cam = camera(0.0, aspect);
    let ahead = (cam.target - cam.pos).normalize();
    let up = Vec3::X.cross(ahead);
    let half = (cam.fov_y * 0.5).tan();
    let ray = ahead + (Vec3::X * at.x * aspect + up * at.y) * half;
    let depth = (SKY_Z - cam.pos.z) / ray.z;
    (cam.pos + ray * depth, size * half * depth)
}

fn camera(time: f32, aspect: f32) -> Camera {
    let sway = (time * 0.15).sin() * 0.05;
    // Wide enough for both sides on any screen.
    let dist = 24.0 * (16.0 / 9.0 / aspect).max(1.0);
    Camera {
        pos: Vec3::new(sway.sin() * dist, 3.4, sway.cos() * dist),
        target: Vec3::new(0.0, 1.0, 0.0),
        fov_y: 0.62,
    }
}

/// Offer `o`'s two sides, facing off: you in `kit` and your employer on the
/// left, the enemy on the right, and the system's star and worlds (`sys`,
/// showing them) over them. `base` is a tank to dress up.
pub fn scene(
    art: &TankArt,
    sys: &SystemArt,
    base: &Tank,
    kit: Loadout,
    o: &Offer,
    time: f32,
    aspect: f32,
) -> Scene {
    let (side, enemy) = (Faction::side(o.side), Faction::side(o.enemy));
    let mut instances = Vec::new();
    let mut particles = Vec::new();
    let mut put = |kit: Loadout, at: Vec3, yaw: f32, company: bool| {
        let mut t = base.clone();
        garage::dress(&mut t, kit);
        t.color = crate::combat::TANK_PAINT;
        t.company = company;
        t.pos = at;
        t.rot = Quat::from_rotation_y(yaw);
        t.vel = Vec3::ZERO;
        t.turret_rel = 0.0;
        t.gun_pitch = 0.0;
        t.since_fire = 100.0;
        t.empty = false;
        t.dead = false;
        t.husk = None;
        t.wheel_lift.iter_mut().for_each(|l| *l = 0.0);
        art.instances(&t, &mut instances, &mut particles);
    };
    let left = FRAC_PI_2 - TURNED;
    put(kit, YOURS, left, true);
    let company = |f| f == Faction::Company;
    let theirs = (side.hulls()[0], side.turrets()[0]);
    put(theirs, THEIRS, left, company(side));
    for (kit, at) in stands(enemy) {
        put(kit, at, -left, company(enemy));
    }
    let dark = Vec4::new(0.0015, 0.0018, 0.0025, 1.0);
    let camera = camera(time, aspect);
    let space = Mat4::from_scale_rotation_translation(
        Vec3::splat(SPACE_R * 2.0),
        Quat::IDENTITY,
        camera.pos,
    );
    instances.push((MeshId::SPHERE, Instance::new(space, dark)));
    theater_view::stars(camera.pos, &mut particles);
    let (at, r) = in_sky(STAR, aspect);
    let star = Star::of(o.starlight);
    sys.star(star, at, r, &mut instances, &mut particles);
    for (i, (&holder, &place)) in o.holders.iter().zip(&WORLDS).enumerate() {
        let (at, r) = in_sky(place, aspect);
        let out = (&mut instances, &mut particles);
        sys.world(i, at, r, holder, camera.pos, time, out);
    }
    let floor = Mat4::from_scale_rotation_translation(
        Vec3::new(120.0, 0.2, 120.0),
        Quat::IDENTITY,
        Vec3::Y * -0.1,
    );
    let grey = Vec4::new(0.035, 0.037, 0.042, 0.0);
    instances.push((MeshId::CYLINDER, Instance::new(floor, grey)));
    // Each side stands in its own light.
    let lamp = |f: Faction, pos: Vec3| PointLight {
        pos,
        color: Vec3::from_slice(&f.ui()[..3]) * 26.0,
        radius: 16.0,
    };
    let atmosphere = Atmosphere {
        sun_dir: Vec3::new(-0.6, 0.6, 0.55).normalize(),
        sun_color: Vec3::new(1.1, 1.06, 1.02) * star.light(),
        sky_zenith: Vec3::splat(0.12),
        sky_horizon: Vec3::splat(0.012),
        ground_color: Vec3::splat(0.03),
        fog_density: 0.0,
        blowing_sand: 0.0,
        ambient: 0.6,
        stars: 0.0,
        ..Atmosphere::default()
    };
    Scene {
        camera,
        time,
        sun_dir: atmosphere.sun_dir,
        atmosphere,
        shadow_center: Vec3::ZERO,
        shadow_radius: 22.0,
        instances,
        particles,
        rivers: Default::default(),
        lights: vec![
            lamp(side, Vec3::new(-9.5, 4.5, -5.0)),
            lamp(Faction::Company, Vec3::new(-3.0, 4.0, -4.5)),
            lamp(enemy, Vec3::new(5.0, 4.5, -5.0)),
            lamp(enemy, Vec3::new(10.5, 4.5, -4.5)),
        ],
        ui: Vec::new(),
        world: false,
    }
}

/// The deal over its scene: `o`, against `mine`, the contract you'd leave.
#[allow(clippy::too_many_arguments)]
pub fn draw_deal(
    p: &mut Painter,
    hits: &mut Hits<Hit>,
    o: Option<&Offer>,
    mine: Option<&Offer>,
    (back_lit, sign_lit): (bool, bool),
    w: f32,
    h: f32,
) {
    p.set_halo(None);
    back_at_foot(p, hits, back_lit, h);
    let mid = w * 0.5;
    let Some(o) = o else {
        p.rect(Vec2::ZERO, Vec2::new(w, h), [0.0, 0.0, 0.0, 1.0]);
        let at = Vec2::new(mid, h * 0.5);
        p.text(at, "The offer's gone", &BODY, Align::Center);
        return;
    };
    let (side, enemy) = (Faction::side(o.side), Faction::side(o.enemy));

    // The job, and how it's going: your side on the left, theirs the right.
    let mut y = MARGIN;
    p.text(Vec2::new(mid, y), &o.job(), &design::TITLE, Align::Center);
    y += UNIT * 8.0;
    let wide = (w - MARGIN * 4.0).min(UNIT * 110.0);
    let (x0, x1) = (mid - wide * 0.5, mid + wide * 0.5);
    let names = BODY.colored(side.ui());
    let mut x = x0 + emblem::faction_name(p, Vec2::new(x0, y), side, &names, Align::Left);
    let mut count = |p: &mut Painter, n: u32, color: Rgba| {
        x += UNIT * 2.0;
        x += p.text(
            Vec2::new(x, y),
            &n.to_string(),
            &BODY.colored(color),
            Align::Left,
        );
    };
    let own = o.hulls - o.mercs;
    if own > 0 || o.mercs == 0 {
        count(p, own, side.ui());
    }
    if o.mercs > 0 {
        count(p, o.mercs, AMBER);
    }
    let names = BODY.colored(enemy.ui());
    let nw = emblem::faction_name(p, Vec2::new(x1, y), enemy, &names, Align::Right);
    let at = Vec2::new(x1 - nw - UNIT * 2.0, y);
    p.text(at, &o.foes.to_string(), &names, Align::Right);
    y += UNIT * 3.0;
    strength(p, o, Vec2::new(x0, y), wide, UNIT);

    // The trade: what you'd gain on the left, what you'd leave on the right.
    let top = h - MARGIN - UNIT * 22.0;
    let col = UNIT * 38.0;
    let line = |p: &mut Painter, x: f32, k: usize, name: &str, value: &str, color| {
        let y = top + UNIT * 3.0 * k as f32;
        p.text(Vec2::new(x, y), name, &SMALL.colored(TEXT_2), Align::Left);
        let at = Vec2::new(x + col, y);
        p.text(at, value, &SMALL.colored(color), Align::Right);
    };
    line(p, x0, 0, "A battle", &format!("{} cr", o.wage), BLUE);
    line(p, x0, 1, "A kill", &format!("{} cr", o.kill), BLUE);
    line(p, x0, 2, "Your hulls", &o.own.to_string(), AMBER);
    line(p, x0, 3, "Enemy gear", &level_label(o.tier), TEXT);
    let land = match o.flight {
        0 => "now".to_string(),
        n => format!("in {n} ticks"),
    };
    line(p, x0, 4, "Lands", &land, TEXT);
    line(
        p,
        x0,
        5,
        "Lasts about",
        &match o.lasts {
            1 => "a battle".to_string(),
            n => format!("{n} battles"),
        },
        TEXT,
    );
    if let Some(m) = mine {
        let x = x1 - col;
        line(p, x, 0, "You leave", &m.job(), TEXT);
        line(p, x, 1, "Hulls left there", &m.own.to_string(), AMBER);
        line(p, x, 2, "A battle there", &format!("{} cr", m.wage), BLUE);
    }
    let size = Vec2::new(UNIT * 26.0, design::BUTTON);
    let min = Vec2::new(mid - size.x * 0.5, top + UNIT * 1.5);
    design::button(p, min, size.x, "Sign on", true, sign_lit);
    hits.add(p, min, size, Hit::Deploy);
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn offer(id: u32, side: usize, enemy: usize) -> Offer {
        Offer {
            id,
            at: 0,
            system: "TORKAR".into(),
            starlight: 1.0,
            holders: vec![enemy],
            side,
            enemy,
            defend: false,
            hulls: 60,
            mercs: 30,
            foes: 10 + id,
            tier: id as u8,
            own: 2,
            wage: 80 + id,
            kill: 24,
            flight: 0,
            lasts: 5,
        }
    }

    #[test]
    fn filters_narrow_and_sorts_order() {
        let mut b = Board::default();
        b.set(vec![offer(1, 0, 1), offer(2, 2, 3), offer(3, 3, 2)], None);
        let ids = |b: &Board| b.listed().iter().map(|o| o.id).collect::<Vec<_>>();
        assert_eq!(ids(&b), [1, 2, 3]);
        assert_eq!(b.picked().unwrap().id, 1);
        // Work for the Uthenn or Ithessar; then only against the Uthenn.
        b.toggle(2);
        b.toggle(3);
        assert_eq!(ids(&b), [2, 3]);
        assert_eq!(b.picked().unwrap().id, 2, "the pick stays listed");
        b.toggle(SIDES + 2);
        assert_eq!(ids(&b), [3]);
        // Nobody's holding anything.
        b.toggle(2 * SIDES + 1);
        assert!(ids(&b).is_empty() && b.picked().is_none());
        assert_eq!(b.input(Nav::Select), None);
        for k in [2, 3, SIDES + 2, 2 * SIDES + 1] {
            b.toggle(k);
        }
        b.sort_by(1);
        assert_eq!(ids(&b), [3, 2, 1]);
        b.sort_by(1);
        assert_eq!(ids(&b), [1, 2, 3]);
        b.sort_by(0);
        assert_eq!(ids(&b), [1, 2, 3], "fewest foes is strongest");
    }

    #[test]
    fn keys_reach_the_filters_and_the_rows() {
        let mut b = Board::default();
        b.set((1..=20).map(|id| offer(id, 0, 1)).collect(), None);
        b.input(Nav::Down);
        assert_eq!(b.input(Nav::Select), Some(2));
        b.input(Nav::Left);
        b.input(Nav::Down);
        assert_eq!(b.input(Nav::Select), None);
        assert!(b.listed().is_empty(), "nobody hires for the Resonant");
        b.input(Nav::Select);
        b.input(Nav::Right);
        assert_eq!(b.listed().len(), 20);
        for _ in 0..50 {
            b.input(Nav::Down);
        }
        assert_eq!(b.picked().unwrap().id, 20);
    }

    /// The wheel scrolls the view; sorting leaves it, and the row picked, put.
    #[test]
    fn sorting_leaves_the_scroll() {
        let mut b = Board::default();
        b.set((1..=30).map(|id| offer(id, 0, 1)).collect(), None);
        // The first row in view, and the last.
        let view = |b: &Board| {
            let (mut p, mut hits) = (Painter::new(1.0), Hits::new());
            b.draw(&mut p, &mut hits, None, 1.0, 1600.0, 900.0);
            let seen = |&i: &usize| hits.center_of(Hit::Row(i)).is_some();
            let rows: Vec<usize> = (0..30).filter(seen).collect();
            (rows[0], rows[rows.len() - 1])
        };
        let (_, last) = view(&b);
        assert!(b.scroll(5));
        assert_eq!(view(&b), (5, last + 5));
        assert_eq!(b.row(), 5, "the pick's kept in view");
        for k in [0, 0, 0, 1, 2, 2] {
            b.sort_by(k);
            assert_eq!((view(&b), b.row()), ((5, last + 5), 5));
        }
        assert!(b.scroll(-50) && !b.scroll(-1));
        assert_eq!((view(&b), b.row()), ((0, last), 5));
        assert!(b.scroll(50) && !b.scroll(1));
        assert_eq!(view(&b), (29 - last, 29));
        // Keys pull the view along.
        b.step(-50);
        assert_eq!(view(&b), (0, last));
    }

    #[test]
    fn both_pages_stay_on_screen() {
        let mut b = Board::default();
        b.set(
            (1..=30).map(|id| offer(id, 0, 3)).collect(),
            Some(offer(40, 1, 2)),
        );
        b.pick_row(29);
        for (w, h) in [(1600.0, 900.0), (1200.0, 900.0), (2100.0, 900.0)] {
            for side in 0..4 {
                let (mut p, mut q) = (Painter::new(1.0), Painter::new(1.0));
                let mut hits = Hits::new();
                b.draw(&mut p, &mut hits, None, 1.0, w, h);
                assert!(hits.center_of(Hit::Row(29)).is_some(), "the pick's in view");
                let o = offer(1, 0, side);
                let mine = b.mine.as_ref();
                draw_deal(&mut q, &mut hits, Some(&o), mine, (false, false), w, h);
                assert!(hits.center_of(Hit::Deploy).is_some());
                assert!(p.finish().iter().chain(&q.finish()).all(|v| {
                    let [x, y] = v.pos;
                    x > -4.0 && x < w + 4.0 && y > -4.0 && y < h + 4.0
                }));
            }
        }
    }
}
