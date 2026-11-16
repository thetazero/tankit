//! In-battle garage: press G mid-match to park, swing the camera round your
//! tank, and pick the hull and turret for your next respawn. The tank you're
//! driving doesn't change; the orbit camera shows the picked kit as a
//! preview. Deploying keeps the pick queued, reverting puts back what was
//! queued when you came in.
//!
//! Drawn in the hairline style, as a panel on the left over a soft dark
//! backdrop, the tank framed in the space to its right.

use glam::{Vec2, Vec3};

use crate::combat;
use crate::render::{Camera, UiVertex};
use crate::sim::{HullKind, HullStats, Tank, TurretKind, World, wrap_angle};
use crate::ui::widgets::{self, label};
use crate::ui::{Align, Painter, TextStyle, theme};

const REF_HEIGHT: f32 = 900.0;
const PANEL_X: f32 = 64.0;
const PANEL_W: f32 = 340.0;
/// Open/close transition speed (1 / seconds).
const BLEND_RATE: f32 = 2.5;
/// Orbit camera: distance, height and slow drift around the tank (rad/s).
const ORBIT_DIST: f32 = 8.5;
const ORBIT_HEIGHT: f32 = 2.6;
const ORBIT_DRIFT: f32 = 0.12;

pub type Loadout = (HullKind, TurretKind);

pub const HULLS: [HullKind; 2] = [HullKind::Lancer, HullKind::Bastion];
pub const TURRETS: [TurretKind; 2] = [TurretKind::Lance, TurretKind::Maul];

pub fn hull_name(k: HullKind) -> &'static str {
    match k {
        HullKind::Lancer => "LANCER",
        HullKind::Bastion => "BASTION",
    }
}

pub fn turret_name(k: TurretKind) -> &'static str {
    match k {
        TurretKind::Lance => "LANCE",
        TurretKind::Maul => "MAUL",
    }
}

/// One stat row: label, value, unit. Bars are scaled to the best in the kit.
struct Stat {
    name: &'static str,
    unit: &'static str,
    decimals: usize,
    get: fn(Part) -> f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Hull(HullKind),
    Turret(TurretKind),
}

fn hull_of(p: Part) -> HullStats {
    match p {
        Part::Hull(k) => HullStats::of(k),
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
        get: |p| match p {
            Part::Hull(k) => combat::max_health(k),
            Part::Turret(_) => unreachable!(),
        },
    },
    Stat { name: "TOP SPEED", unit: "KM/H", decimals: 0, get: |p| hull_of(p).max_speed * 3.6 },
    Stat { name: "ACCEL", unit: "M/S2", decimals: 1, get: |p| hull_of(p).accel },
    Stat { name: "TURN", unit: "°/S", decimals: 0, get: |p| hull_of(p).turn_rate.to_degrees() },
    Stat { name: "TRAVERSE", unit: "°/S", decimals: 0, get: |p| hull_of(p).turret_speed.to_degrees() },
    Stat {
        name: "MASS",
        unit: "T",
        decimals: 0,
        get: |p| match p {
            Part::Hull(k) => crate::sim::Chassis::of(k).mass / 1000.0,
            Part::Turret(_) => unreachable!(),
        },
    },
];

const TURRET_STATS: [Stat; 3] = [
    Stat { name: "DAMAGE", unit: "HP", decimals: 0, get: |p| combat::gun(turret_of(p)).damage },
    Stat { name: "FIRE RATE", unit: "/MIN", decimals: 0, get: |p| 60.0 / combat::gun(turret_of(p)).reload },
    Stat {
        name: "DPS",
        unit: "HP/S",
        decimals: 0,
        get: |p| {
            let g = combat::gun(turret_of(p));
            g.damage / g.reload
        },
    },
];

/// Navigation input, mapped from keys by the app.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    Up,
    Down,
    Left,
    Right,
    /// Keep the loadout and drive off.
    Deploy,
    /// Put back the loadout from when the garage opened, and close.
    Revert,
}

pub struct Garage {
    open: bool,
    /// 0 = closed .. 1 = fully open; eased toward `open`.
    blend: f32,
    /// 0 = hull, 1 = turret.
    section: usize,
    /// Queued loadout when the garage opened (the revert target).
    entry: Loadout,
    /// Orbit camera angle around the tank (world yaw of the camera's offset).
    orbit: f32,
    /// Where the orbit is drifting to; starts at a front three-quarter view.
    orbit_goal: f32,
}

impl Garage {
    pub fn new() -> Garage {
        Garage {
            open: false,
            blend: 0.0,
            section: 0,
            entry: (HullKind::Lancer, TurretKind::Lance),
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

    /// Open on the player's tank (index 0) with `queued` as the pick, or close.
    pub fn toggle(&mut self, world: &World, camera: &Camera, queued: Loadout) {
        if self.open {
            self.open = false;
            return;
        }
        let t = &world.tanks[0];
        self.open = true;
        self.entry = queued;
        // Start the orbit where the chase camera is (unless we're mid-transition) and swing round to the front.
        if self.blend <= 0.0 {
            let off = camera.pos - t.pos;
            self.orbit = off.x.atan2(off.z);
        }
        let f = t.forward();
        self.orbit_goal = self.orbit + wrap_angle(f.x.atan2(f.z) + 0.65 - self.orbit);
    }

    /// Close without touching the loadout (e.g. the player was killed).
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Handle a key while open, given the loadout queued for the next respawn.
    /// Returns the new loadout to queue, if it changed.
    pub fn input(&mut self, input: Input, current: Loadout) -> Option<Loadout> {
        if !self.open {
            return None;
        }
        let (hull, turret) = current;
        // Wrapping step through a list of `n`.
        let next = |i: usize, n: usize| if input == Input::Down { (i + 1) % n } else { (i + n - 1) % n };
        let want = match input {
            Input::Left | Input::Right => {
                self.section = 1 - self.section;
                current
            }
            Input::Up | Input::Down if self.section == 0 => {
                let i = HULLS.iter().position(|&h| h == hull).unwrap_or(0);
                (HULLS[next(i, HULLS.len())], turret)
            }
            Input::Up | Input::Down => {
                let i = TURRETS.iter().position(|&k| k == turret).unwrap_or(0);
                (hull, TURRETS[next(i, TURRETS.len())])
            }
            Input::Deploy => {
                self.open = false;
                current
            }
            Input::Revert => {
                self.open = false;
                self.entry
            }
        };
        (want != current).then_some(want)
    }

    pub fn update(&mut self, dt: f32) {
        let goal = if self.open { 1.0 } else { 0.0 };
        let d = BLEND_RATE * dt;
        self.blend = if self.blend < goal { (self.blend + d).min(goal) } else { (self.blend - d).max(goal) };
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
    /// queued kit, so you see what you'll respawn in.
    pub fn preview(&self, tank: &mut Tank, queued: Loadout) {
        if self.blend < 0.5 || (tank.hull_kind, tank.turret_kind) == queued {
            return;
        }
        let chassis = crate::sim::Chassis::of(queued.0);
        tank.hull_kind = queued.0;
        tank.turret_kind = queued.1;
        tank.chassis = chassis;
        tank.wheel_lift.resize(chassis.wheel_count(), 0.0);
    }

    /// The battle HUD faded out as the garage comes in, with the garage panel on top.
    /// `tank` is the one being driven, `queued` the kit for the next respawn.
    pub fn overlay(&self, mut hud: Vec<UiVertex>, tank: &Tank, queued: Loadout, height: f32) -> Vec<UiVertex> {
        let fade = 1.0 - self.presence();
        for v in &mut hud {
            v.color[3] *= fade;
        }
        if fade <= 0.0 {
            hud.clear();
        }
        hud.extend(self.draw(tank, queued, height));
        hud
    }

    pub fn draw(&self, tank: &Tank, queued: Loadout, height: f32) -> Vec<UiVertex> {
        let e = smooth(self.blend);
        if e <= 0.0 {
            return Vec::new();
        }
        let scale = height / REF_HEIGHT;
        let h = REF_HEIGHT;
        let mut p = Painter::new(scale);

        // Backdrop so the hairlines read over a bright city.
        let shade = [0.03, 0.03, 0.035, 0.72];
        let solid = PANEL_X + PANEL_W + 40.0;
        p.gradient_h(Vec2::ZERO, Vec2::new(solid, h), shade, shade);
        p.gradient_h(Vec2::new(solid, 0.0), Vec2::new(260.0, h), shade, [0.0, 0.0, 0.0, 0.0]);

        let x = PANEL_X - (1.0 - e) * 30.0;
        let mut y = 72.0;
        label(&mut p, Vec2::new(x, y), "GARAGE", Align::Left);
        label(&mut p, Vec2::new(x + PANEL_W, y), "LOADOUT", Align::Right);
        y += 18.0;
        let title = format!("{} · {}", hull_name(queued.0), turret_name(queued.1));
        p.text(Vec2::new(x, y), &title, &TextStyle::value(20.0), Align::Left);
        y += 30.0;
        let pending = queued != (tank.hull_kind, tank.turret_kind);
        label(&mut p, Vec2::new(x, y), if pending { "FITTED ON NEXT RESPAWN" } else { "IN USE" }, Align::Left);
        y += 20.0;
        p.line(Vec2::new(x, y), Vec2::new(x + PANEL_W, y), 1.0, theme::FAINT);
        y += 26.0;

        let hulls: Vec<_> = HULLS.iter().map(|&k| (Part::Hull(k), hull_name(k))).collect();
        let turrets: Vec<_> = TURRETS.iter().map(|&k| (Part::Turret(k), turret_name(k))).collect();
        let sections = [
            ("01", "HULL", hulls, Part::Hull(queued.0), Part::Hull(tank.hull_kind), &HULL_STATS[..]),
            ("02", "TURRET", turrets, Part::Turret(queued.1), Part::Turret(tank.turret_kind), &TURRET_STATS[..]),
        ];
        for (si, (num, name, items, current, in_use, stats)) in sections.iter().enumerate() {
            let focused = si == self.section;
            let head = if focused { theme::FG } else { theme::DIM };
            let mut s = TextStyle::LABEL;
            s.color = head;
            p.text(Vec2::new(x, y), &format!("{num}  {name}"), &s, Align::Left);
            label(&mut p, Vec2::new(x + PANEL_W, y), &format!("{}/{}", index_of(items, *current) + 1, items.len()), Align::Right);
            y += 20.0;

            for (part, item) in items {
                let selected = part == current;
                if selected {
                    // Bar beside the selected part; brightest in the focused section.
                    let c = if focused { theme::FG } else { theme::DIM };
                    p.line(Vec2::new(x - 14.0, y - 3.0), Vec2::new(x - 14.0, y + 18.0), 2.0, c);
                    if focused {
                        p.polyline(&[Vec2::new(x + PANEL_W - 8.0, y + 2.0), Vec2::new(x + PANEL_W - 3.0, y + 7.5), Vec2::new(x + PANEL_W - 8.0, y + 13.0)], 1.2, c);
                    }
                }
                let mut s = TextStyle::value(15.0);
                s.color = if selected { theme::FG } else { theme::DIM };
                p.text(Vec2::new(x, y), item, &s, Align::Left);
                if part == in_use && !selected {
                    label(&mut p, Vec2::new(x + PANEL_W - 16.0, y + 4.0), "IN USE", Align::Right);
                }
                y += 30.0;
            }

            y += 4.0;
            let parts: Vec<Part> = items.iter().map(|i| i.0).collect();
            for stat in stats.iter() {
                draw_stat(&mut p, Vec2::new(x, y), stat, *current, *in_use, &parts);
                y += 22.0;
            }
            y += 26.0;
            if si == 0 {
                p.line(Vec2::new(x, y - 14.0), Vec2::new(x + PANEL_W, y - 14.0), 1.0, theme::FAINT);
                y += 12.0;
            }
        }

        // Key hints along the bottom of the panel.
        let mut hy = h - 100.0;
        for (key, what) in [("W/S", "CHANGE PART"), ("A/D", "HULL / TURRET"), ("ENTER", "DEPLOY"), ("ESC", "CANCEL")] {
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

fn index_of(items: &[(Part, &str)], part: Part) -> usize {
    items.iter().position(|i| i.0 == part).unwrap_or(0)
}

/// Label, bar scaled to the kit's best, value; a faint tick where the part in use sits.
fn draw_stat(p: &mut Painter, pos: Vec2, stat: &Stat, current: Part, in_use: Part, kit: &[Part]) {
    let best = kit.iter().map(|&k| (stat.get)(k)).fold(0.0f32, f32::max).max(1e-3);
    let frac = |part: Part| (stat.get)(part) / (best * 1.2);
    label(p, pos + Vec2::new(0.0, 1.0), stat.name, Align::Left);
    let bar = pos + Vec2::new(110.0, 5.0);
    let bar_w = 132.0;
    widgets::hairbar(p, bar, bar_w, frac(current), theme::FG);
    if in_use != current {
        let tx = bar.x + bar_w * frac(in_use).clamp(0.0, 1.0);
        p.line(Vec2::new(tx, bar.y - 5.0), Vec2::new(tx, bar.y + 5.0), 1.0, theme::DIM);
    }
    let v = (stat.get)(current);
    let text = format!("{v:.prec$}", prec = stat.decimals);
    let right = pos + Vec2::new(PANEL_W, 0.0);
    let uw = if stat.unit.is_empty() { 0.0 } else { label(p, right, stat.unit, Align::Right) + 8.0 };
    p.text(right - Vec2::new(uw, 0.0), &text, &TextStyle::value(12.0), Align::Right);
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
        Camera { pos: t.pos - t.forward() * 10.0 + Vec3::Y * 5.0, target: t.pos, fov_y: 1.1 }
    }

    #[test]
    fn picks_and_cancels() {
        let w = world();
        let mut g = Garage::new();
        let start = (HullKind::Lancer, TurretKind::Lance);
        // Closed: input does nothing.
        assert_eq!(g.input(Input::Down, start), None);

        g.toggle(&w, &cam(&w), start);
        let l = g.input(Input::Down, start).unwrap();
        assert_eq!(l, (HullKind::Bastion, TurretKind::Lance));
        assert_eq!(g.input(Input::Right, l), None);
        let l = g.input(Input::Up, l).unwrap();
        assert_eq!(l, (HullKind::Bastion, TurretKind::Maul));
        assert_eq!(g.input(Input::Revert, l), Some(start));
        assert!(!g.is_open());

        // Reopens on the section it was left on (turret): go back to the hull.
        g.toggle(&w, &cam(&w), start);
        g.input(Input::Left, start);
        let l = g.input(Input::Down, start).unwrap();
        assert_eq!(g.input(Input::Deploy, l), None);
        assert!(!g.is_open());
    }

    #[test]
    fn preview_wears_the_queued_kit() {
        let w = world();
        let mut g = Garage::new();
        let kit = (HullKind::Bastion, TurretKind::Maul);
        let mut t = w.tanks[0].clone();
        g.preview(&mut t, kit);
        assert_eq!(t.hull_kind, HullKind::Lancer, "closed garage leaves the tank alone");
        g.toggle(&w, &cam(&w), kit);
        g.update(1.0);
        g.preview(&mut t, kit);
        assert_eq!((t.hull_kind, t.turret_kind), kit);
        assert_eq!(t.wheel_lift.len(), crate::sim::Chassis::of(HullKind::Bastion).wheel_count());
        // The real tank is untouched.
        assert_eq!(w.tanks[0].hull_kind, HullKind::Lancer);
    }

    #[test]
    fn camera_blends_from_chase_to_orbit() {
        let w = world();
        let mut g = Garage::new();
        let c = cam(&w);
        let t = &w.tanks[0];
        assert_eq!(g.camera(c, &w, t, 16.0 / 9.0).pos, c.pos);
        g.toggle(&w, &c, (HullKind::Lancer, TurretKind::Lance));
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
        let kit = (HullKind::Bastion, TurretKind::Maul);
        assert!(g.draw(&w.tanks[0], kit, 900.0).is_empty());
        g.toggle(&w, &cam(&w), kit);
        g.update(1.0);
        for (wd, ht) in [(1600.0, 900.0), (1024.0, 768.0), (3840.0, 2160.0)] {
            let v = g.draw(&w.tanks[0], kit, ht);
            assert!(!v.is_empty());
            assert!(v.iter().all(|v| v.pos[0] > -4.0 && v.pos[0] < wd + 4.0 && v.pos[1] > -4.0 && v.pos[1] < ht + 4.0));
        }
    }
}
