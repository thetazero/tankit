//! Garage: from the main menu, or G mid-match to park and swing the camera
//! round your tank. Pick the hull and turret for your next respawn, and buy
//! or upgrade parts with crystals. The tank you're driving doesn't change;
//! the orbit camera shows the highlighted kit as a preview (parts you don't
//! own too). Deploying keeps the pick queued, reverting puts back what was
//! queued when you came in.
//!
//! Drawn in the hairline style, as a panel on the left over a soft dark
//! backdrop, the tank framed in the space to its right.

use glam::{Vec2, Vec3};

use crate::economy::{Levels, Part};
use crate::profile::{Profile, Refusal};
use crate::render::{Camera, UiVertex};
use crate::sim::{HullKind, HullStats, Tank, TurretKind, World, wrap_angle};
use crate::ui::widgets::{self, label};
use crate::ui::{Align, Painter, TextStyle, theme};

const REF_HEIGHT: f32 = 900.0;
pub const PANEL_X: f32 = 64.0;
pub const PANEL_W: f32 = 340.0;
/// Open/close transition speed (1 / seconds).
const BLEND_RATE: f32 = 2.5;
/// Orbit camera: distance, height and slow drift around the tank (rad/s).
const ORBIT_DIST: f32 = 8.5;
const ORBIT_HEIGHT: f32 = 2.6;
const ORBIT_DRIFT: f32 = 0.12;

pub type Loadout = (HullKind, TurretKind);

pub const HULLS: [HullKind; 4] = [
    HullKind::Kataphrakt,
    HullKind::Kastron,
    HullKind::Sethuun,
    HullKind::Vigil,
];
pub const TURRETS: [TurretKind; 6] = TurretKind::ALL;

pub fn hull_name(k: HullKind) -> &'static str {
    match k {
        HullKind::Kataphrakt => "KATAPHRAKT",
        HullKind::Kastron => "KASTRON",
        HullKind::Sethuun => "SETHUUN",
        HullKind::Vigil => "VIGIL",
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

const HULL_STATS: [Stat; 6] = [
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
    Left,
    Right,
    /// Buy the highlighted part, or upgrade it if owned.
    Act,
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

pub struct Garage {
    open: bool,
    /// 0 = closed .. 1 = fully open; eased toward `open`.
    blend: f32,
    /// 0 = hull, 1 = turret.
    section: usize,
    /// The highlighted parts. Owned picks are queued as you go; parts you
    /// don't own can be highlighted (and previewed) but not queued.
    view: Loadout,
    /// Queued loadout when the garage opened (the revert target).
    entry: Loadout,
    /// Orbit camera angle around the tank (world yaw of the camera's offset).
    orbit: f32,
    /// Where the orbit is drifting to; starts at a front three-quarter view.
    orbit_goal: f32,
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
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Eased 0..1 openness, for fading the battle HUD out.
    pub fn presence(&self) -> f32 {
        smooth(self.blend)
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
        match input {
            Input::Left | Input::Right => {
                self.section = 1 - self.section;
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
                self.queue_view(queued, profile)
            }
            Input::Act if profile.owns(self.focused()) => Some(Event::Upgrade(self.focused())),
            Input::Act => Some(Event::Buy(self.focused())),
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

    /// After a purchase: queue the highlighted part if it's owned now.
    pub fn bought(&mut self, queued: Loadout, profile: &Profile) -> Option<Event> {
        self.queue_view(queued, profile)
    }

    /// The highlighted parts that are owned, on top of what's queued.
    fn queue_view(&self, queued: Loadout, profile: &Profile) -> Option<Event> {
        let hull = if profile.owns(Part::Hull(self.view.0)) {
            self.view.0
        } else {
            queued.0
        };
        let turret = if profile.owns(Part::Turret(self.view.1)) {
            self.view.1
        } else {
            queued.1
        };
        ((hull, turret) != queued).then_some(Event::Queue((hull, turret)))
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
        let hit = world.camera_raycast(focus, dir / len, len);
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
    ) -> Vec<UiVertex> {
        let fade = 1.0 - self.presence();
        for v in &mut hud {
            v.color[3] *= fade;
        }
        if fade <= 0.0 {
            hud.clear();
        }
        hud.extend(self.draw(tank, queued, profile, height));
        hud
    }

    pub fn draw(
        &self,
        tank: &Tank,
        queued: Loadout,
        profile: &Profile,
        height: f32,
    ) -> Vec<UiVertex> {
        let e = smooth(self.blend);
        if e <= 0.0 {
            return Vec::new();
        }
        let scale = height / REF_HEIGHT;
        let h = REF_HEIGHT;
        let mut p = Painter::new(scale);
        let levels = &profile.levels;

        backdrop(&mut p, h);

        let x = PANEL_X - (1.0 - e) * 30.0;
        let mut y = 72.0;
        label(&mut p, Vec2::new(x, y), "GARAGE", Align::Left);
        let cw = label(&mut p, Vec2::new(x + PANEL_W, y), "CR", Align::Right);
        p.text(
            Vec2::new(x + PANEL_W - cw - 8.0, y - 2.0),
            &profile.crystals.to_string(),
            &TextStyle::value(11.0),
            Align::Right,
        );
        y += 18.0;
        let title = format!("{} · {}", hull_name(queued.0), turret_name(queued.1));
        p.text(
            Vec2::new(x, y),
            &title,
            &TextStyle::value(20.0),
            Align::Left,
        );
        y += 30.0;
        let status = if !profile.owns_kit(self.view) {
            "PREVIEW · NOT OWNED"
        } else if queued != (tank.hull_kind, tank.turret_kind) {
            "FITTED ON NEXT RESPAWN"
        } else {
            "IN USE"
        };
        label(&mut p, Vec2::new(x, y), status, Align::Left);
        y += 20.0;
        p.line(
            Vec2::new(x, y),
            Vec2::new(x + PANEL_W, y),
            1.0,
            theme::FAINT,
        );
        y += 26.0;

        let hulls: Vec<Part> = HULLS.iter().map(|&k| Part::Hull(k)).collect();
        let turrets: Vec<Part> = TURRETS.iter().map(|&k| Part::Turret(k)).collect();
        let sections = [
            (
                "01",
                "HULL",
                hulls,
                Part::Hull(self.view.0),
                Part::Hull(tank.hull_kind),
                &HULL_STATS[..],
            ),
            (
                "02",
                "TURRET",
                turrets,
                Part::Turret(self.view.1),
                Part::Turret(tank.turret_kind),
                &TURRET_STATS[..],
            ),
        ];
        for (si, (num, name, items, current, in_use, stats)) in sections.iter().enumerate() {
            let focused = si == self.section;
            let head = if focused { theme::FG } else { theme::DIM };
            let mut s = TextStyle::LABEL;
            s.color = head;
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
                let selected = part == current;
                let owned = profile.owns(*part);
                if selected {
                    // Bar beside the selected part; brightest in the focused section.
                    let c = if focused { theme::FG } else { theme::DIM };
                    p.line(
                        Vec2::new(x - 14.0, y - 3.0),
                        Vec2::new(x - 14.0, y + 18.0),
                        2.0,
                        c,
                    );
                    if focused {
                        p.polyline(
                            &[
                                Vec2::new(x + PANEL_W - 8.0, y + 2.0),
                                Vec2::new(x + PANEL_W - 3.0, y + 7.5),
                                Vec2::new(x + PANEL_W - 8.0, y + 13.0),
                            ],
                            1.2,
                            c,
                        );
                    }
                }
                let mut s = TextStyle::value(15.0);
                s.color = if selected && owned {
                    theme::FG
                } else if selected || owned {
                    theme::DIM
                } else {
                    theme::FAINT
                };
                let nw = p.text(Vec2::new(x, y), part.name(), &s, Align::Left);
                // Price or lock (or IN USE) on the right; upgrade level and
                // the part's maker after the name, the maker dropped when a
                // long name leaves it no room.
                let right = Vec2::new(x + PANEL_W - 16.0, y + 4.0);
                let tag = if part == in_use && !selected {
                    Some("IN USE".to_string())
                } else {
                    part.price().filter(|_| !owned).map(|price| {
                        if profile.standing_for(*part) < price.standing {
                            format!("TRUST {}", price.standing)
                        } else {
                            format!("{} CR", price.crystals)
                        }
                    })
                };
                let room = right.x
                    - tag
                        .as_deref()
                        .map_or(0.0, |t| crate::ui::measure(t, &TextStyle::LABEL) + 12.0)
                    - (x + nw + 12.0);
                let maker = part.maker().name();
                let level = format!("M{}", levels.of(*part));
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
                    label(
                        &mut p,
                        Vec2::new(x + nw + 12.0, y + 4.0),
                        &note,
                        Align::Left,
                    );
                }
                if let Some(tag) = tag {
                    label(&mut p, right, &tag, Align::Right);
                }
                y += 30.0;
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
                y += 22.0;
            }
            y += 26.0;
            if si == 0 {
                p.line(
                    Vec2::new(x, y - 14.0),
                    Vec2::new(x + PANEL_W, y - 14.0),
                    1.0,
                    theme::FAINT,
                );
                y += 12.0;
            }
        }

        // What E does for the highlighted part, then the key hints.
        let mut hy = h - 128.0;
        let (act, ready) = action(self.focused(), profile);
        let mut k = TextStyle::LABEL;
        k.color = if ready { theme::HOT } else { theme::DIM };
        p.text(Vec2::new(x, hy), "E", &k, Align::Left);
        let mut a = TextStyle::LABEL;
        a.color = if ready { theme::FG } else { theme::DIM };
        p.text(Vec2::new(x + 70.0, hy), &act, &a, Align::Left);
        hy += 28.0;
        for (key, what) in [
            ("W/S", "CHANGE PART"),
            ("A/D", "HULL / TURRET"),
            ("ENTER", "DEPLOY"),
            ("ESC", "CANCEL"),
        ] {
            let mut k = TextStyle::LABEL;
            k.color = theme::FG;
            p.text(Vec2::new(x, hy), key, &k, Align::Left);
            label(&mut p, Vec2::new(x + 70.0, hy), what, Align::Left);
            hy += 14.0;
        }

        let mut v = p.finish();
        for vert in &mut v {
            vert.color[3] *= e;
        }
        v
    }
}

/// What buying or upgrading `part` would do, and whether it can right now.
fn action(part: Part, profile: &Profile) -> (String, bool) {
    if profile.owns(part) {
        let next = profile.levels.of(part) + 1;
        match profile.can_upgrade(part) {
            Ok(cost) => (format!("UPGRADE TO M{next} · {cost} CR"), true),
            Err(Refusal::Crystals(cost)) => (format!("UPGRADE TO M{next} · NEED {cost} CR"), false),
            Err(_) => ("FULLY UPGRADED".to_string(), false),
        }
    } else {
        match profile.can_buy(part) {
            Ok(cost) => (format!("BUY · {cost} CR"), true),
            Err(Refusal::Trust(f, s)) => (format!("{} SELLS AT TRUST {s}", f.name()), false),
            Err(Refusal::Crystals(cost)) => (format!("BUY · NEED {cost} CR"), false),
            Err(_) => (String::new(), false),
        }
    }
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
    let hit = world.camera_raycast(focus, dir / len, len);
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
        World::new(crate::map::Map::generate(21), 0)
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
    fn picks_and_cancels() {
        let w = world();
        let p = rich();
        let mut g = Garage::new();
        let start = (HullKind::Kataphrakt, TurretKind::Kontos);
        // Closed: input does nothing.
        assert_eq!(g.input(Input::Down, start, &p), None);

        g.toggle(&w, &cam(&w), start);
        let l = (HullKind::Kastron, TurretKind::Kontos);
        assert_eq!(g.input(Input::Down, start, &p), Some(Event::Queue(l)));
        assert_eq!(g.input(Input::Right, l, &p), None);
        // The list wraps: up from the first turret is the last one.
        let l2 = (HullKind::Kastron, *TURRETS.last().unwrap());
        assert_eq!(g.input(Input::Up, l, &p), Some(Event::Queue(l2)));
        assert_eq!(g.input(Input::Revert, l2, &p), Some(Event::Queue(start)));
        assert!(!g.is_open());

        // Reopens on the section it was left on (turret): go back to the hull.
        g.toggle(&w, &cam(&w), start);
        g.input(Input::Left, start, &p);
        assert!(g.input(Input::Down, start, &p).is_some());
        assert_eq!(g.input(Input::Deploy, l, &p), None);
        assert!(!g.is_open());
    }

    #[test]
    fn unowned_parts_preview_but_dont_queue() {
        let w = world();
        let mut p = Profile::in_memory();
        let mut g = Garage::new();
        let start = (HullKind::Kataphrakt, TurretKind::Kontos);
        g.toggle(&w, &cam(&w), start);
        g.input(Input::Right, start, &p);
        // Pelekys isn't owned: highlighted and previewed, not queued; E asks to buy it.
        assert_eq!(g.input(Input::Down, start, &p), None);
        assert_eq!(g.view, (HullKind::Kataphrakt, TurretKind::Pelekys));
        let pelekys = Part::Turret(TurretKind::Pelekys);
        assert_eq!(g.input(Input::Act, start, &p), Some(Event::Buy(pelekys)));
        // Once bought it queues; E on an owned part upgrades.
        p.set_trust_all(100_000);
        p.crystals = 100_000;
        p.buy(pelekys).unwrap();
        let kit = (HullKind::Kataphrakt, TurretKind::Pelekys);
        assert_eq!(g.bought(start, &p), Some(Event::Queue(kit)));
        assert_eq!(g.input(Input::Act, kit, &p), Some(Event::Upgrade(pelekys)));
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
        assert!(g.draw(&w.tanks[0], kit, &p, 900.0).is_empty());
        g.toggle(&w, &cam(&w), kit);
        g.update(1.0);
        for (wd, ht) in [(1600.0, 900.0), (1024.0, 768.0), (3840.0, 2160.0)] {
            let v = g.draw(&w.tanks[0], kit, &p, ht);
            assert!(!v.is_empty());
            assert!(v.iter().all(|v| v.pos[0] > -4.0
                && v.pos[0] < wd + 4.0
                && v.pos[1] > -4.0
                && v.pos[1] < ht + 4.0));
        }
    }
}
