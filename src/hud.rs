//! Driving HUD in the hairline style: readouts in the four corners inside
//! faint brackets, the middle left to the city and the crosshair.
//!
//! Team deathmatch readouts: round clock and kills top left, team scores
//! under the compass, kill feed top right, hull and reload bottom centre,
//! team-coloured health bars over tanks, a death screen, and the scoreboard
//! (Tab, or when the round ends). Tanks have no names: "YOU", or their turret.

use glam::{Vec2, Vec3};

use crate::combat::{FEED_TIME, Match, Phase, REPUTATION_POP_TIME};
use crate::economy::Payout;
use crate::hold::{self, Control};
use crate::mission::Contract;
use crate::profile::{Profile, Standing};
use crate::render::{Camera, UiVertex};
use crate::scene::project;
use crate::sim::Tank;
use crate::ui::emblem;
use crate::ui::widgets::{self, label};
use crate::ui::{Align, Hits, Layer, Painter, Rgba, TextStyle, measure, theme, with_alpha};

/// What the HUD calls fighter `i`: "YOU", or its turret.
pub fn callsign(m: &Match, i: usize) -> &'static str {
    if i == 0 {
        "YOU"
    } else {
        crate::garage::turret_name(m.world.tanks[i].turret_kind)
    }
}

/// Fighter `i`'s side colour (the player in the combat orange).
fn side_color(m: &Match, i: usize) -> Rgba {
    if i == 0 {
        theme::HOT
    } else {
        m.factions[m.fighters[i].team].ui()
    }
}

/// Logical screen height; everything is laid out at this size and scaled.
const REF_HEIGHT: f32 = 900.0;
const MARGIN: f32 = 40.0;

/// Something on the HUD that can be clicked: the round-over buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    /// Enter: play the round again.
    Again,
    /// Esc: back to the menu.
    Menu,
}

/// The pilot's books as the match started, for the round-over screen to count
/// up from: reputation with the employer before this match's was banked, and
/// crystals before the contract paid.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ledger {
    pub reputation: u32,
    pub crystals: u32,
}

impl Ledger {
    /// Backs this match's earnings out of `profile`: `banked` reputation is
    /// already in it, and the contract pay too once `paid`.
    pub fn before(profile: &Profile, m: &Match, banked: u32, paid: bool) -> Ledger {
        let pay = if paid { Payout::of(m).total() } else { 0 };
        Ledger {
            reputation: profile.reputation_of(m.factions[0]).saturating_sub(banked),
            crystals: profile.crystals.saturating_sub(pay),
        }
    }
}

/// Eased values so readouts glide instead of snapping.
pub struct Hud {
    speed: f32,
    heading: f32,
    initialized: bool,
    /// The button under the pointer, lit.
    pub hover: Option<Hit>,
}

impl Hud {
    pub fn new() -> Hud {
        Hud {
            speed: 0.0,
            heading: 0.0,
            initialized: false,
            hover: None,
        }
    }

    /// `tanks` are the tanks as drawn this frame (interpolated); 0 is the player.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        m: &Match,
        tanks: &[Tank],
        camera: &Camera,
        width: f32,
        height: f32,
        dt: f32,
        scores: bool,
        ledger: Ledger,
    ) -> Layer<Hit> {
        let tank = &tanks[0];
        let me = m.player();
        let board = scores || m.phase == Phase::Over;
        let speed = tank.speed().abs() * 3.6;
        let max_speed = tank.hull.max_speed * 3.6;
        // Turning right (negative yaw) should read as an increasing compass heading.
        let heading = (-tank.turret_yaw()).to_degrees().rem_euclid(360.0);

        if !self.initialized {
            (self.speed, self.heading, self.initialized) = (speed, heading, true);
        }
        let k = 1.0 - (-dt * 10.0).exp();
        self.speed += (speed - self.speed) * k;
        let dh = (heading - self.heading + 540.0).rem_euclid(360.0) - 180.0;
        self.heading = (self.heading + dh * k).rem_euclid(360.0);

        let scale = height / REF_HEIGHT;
        let (w, h) = (width / scale, REF_HEIGHT);
        let mut p = Painter::new(scale);
        let mut hits = Hits::new();

        widgets::corners(
            &mut p,
            Vec2::splat(18.0),
            Vec2::new(w - 18.0, h - 18.0),
            22.0,
            theme::FAINT,
        );

        // Top left: round clock, kills / deaths, reputation.
        let secs = m.time_left.ceil() as u32;
        label(&mut p, Vec2::new(MARGIN, MARGIN), "ROUND", Align::Left);
        let mut clock = TextStyle::value(20.0);
        if m.time_left < 30.0 && m.phase == Phase::Playing {
            clock.color = theme::HOT;
        }
        p.text(
            Vec2::new(MARGIN, MARGIN + 14.0),
            &format!("{}:{:02}", secs / 60, secs % 60),
            &clock,
            Align::Left,
        );
        label(
            &mut p,
            Vec2::new(MARGIN, MARGIN + 50.0),
            "KILLS",
            Align::Left,
        );
        p.text(
            Vec2::new(MARGIN, MARGIN + 64.0),
            &me.kills.to_string(),
            &TextStyle::value(14.0),
            Align::Left,
        );
        label(
            &mut p,
            Vec2::new(MARGIN + 70.0, MARGIN + 50.0),
            "DEATHS",
            Align::Left,
        );
        p.text(
            Vec2::new(MARGIN + 70.0, MARGIN + 64.0),
            &me.deaths.to_string(),
            &TextStyle::value(14.0),
            Align::Left,
        );
        label(
            &mut p,
            Vec2::new(MARGIN + 150.0, MARGIN + 50.0),
            "REPUTATION",
            Align::Left,
        );
        p.text(
            Vec2::new(MARGIN + 150.0, MARGIN + 64.0),
            &format!("+{}", m.reputation),
            &TextStyle::value(14.0),
            Align::Left,
        );

        // Top right: kill feed, newest on top, each side in its colour.
        let tr = Vec2::new(w - MARGIN, MARGIN);
        for (row, k) in m.feed.iter().rev().take(5).enumerate() {
            let fade = ((FEED_TIME - k.age) / 0.8).clamp(0.0, 1.0);
            let y = tr.y + row as f32 * 18.0;
            let mut s = TextStyle::value(10.0);
            s.tracking = 1.0;
            s.color = with_alpha(side_color(m, k.victim), fade * 0.95);
            let vw = p.text(Vec2::new(tr.x, y), callsign(m, k.victim), &s, Align::Right);
            // A small chevron between killer and victim.
            let cx = tr.x - vw - 14.0;
            let c = with_alpha(theme::DIM, fade);
            p.polyline(
                &[
                    Vec2::new(cx - 3.0, y + 1.0),
                    Vec2::new(cx + 2.0, y + 5.0),
                    Vec2::new(cx - 3.0, y + 9.0),
                ],
                1.2,
                c,
            );
            if k.killer != k.victim {
                s.color = with_alpha(side_color(m, k.killer), fade * 0.95);
                p.text(
                    Vec2::new(cx - 12.0, y),
                    callsign(m, k.killer),
                    &s,
                    Align::Right,
                );
            }
        }

        // Health over the other tanks in view, in their side's colour; allies get a tick above.
        let vp = camera.view_proj(width / height);
        for (i, (t, f)) in tanks.iter().zip(&m.fighters).enumerate().skip(1) {
            let d = t.pos.distance(camera.pos);
            if board || !f.alive || d > 160.0 {
                continue;
            }
            let Some((x, y)) = project(
                vp,
                t.pos + Vec3::Y * (t.chassis.hull_top + 2.2),
                width,
                height,
            ) else {
                continue;
            };
            let at = Vec2::new(x, y) / scale;
            if at.x < 0.0 || at.x > w || at.y < 0.0 || at.y > h {
                continue;
            }
            let fade = (1.0 - (d - 110.0) / 50.0).clamp(0.0, 1.0);
            let side = m.factions[f.team].ui();
            if f.team == me.team {
                p.polyline(
                    &[
                        at + Vec2::new(-4.0, -10.0),
                        at + Vec2::new(0.0, -5.0),
                        at + Vec2::new(4.0, -10.0),
                    ],
                    1.2,
                    with_alpha(side, 0.9 * fade),
                );
            }
            let frac = f.health / f.max_health(&m.world.tanks[i]);
            let bw = 44.0;
            let fill = if f.since_hit < 0.25 {
                theme::HOT
            } else {
                with_alpha(side, fade)
            };
            let o = at - Vec2::new(bw * 0.5, 0.0);
            p.line(o, o + Vec2::new(bw, 0.0), 1.0, with_alpha(theme::DIM, fade));
            p.line(o, o + Vec2::new(bw * frac.clamp(0.0, 1.0), 0.0), 2.0, fill);
        }

        // Top centre: turret heading, and under it the team score (your side on the left).
        let top = Vec2::new(w * 0.5, MARGIN + 6.0);
        widgets::compass(&mut p, top, 360.0, self.heading);
        let sy = top.y + 58.0;
        let sides = m.factions.len();
        if sides == 2 {
            for (team, dir, align) in [(0, -1.0, Align::Right), (1, 1.0, Align::Left)] {
                let f = m.factions[team];
                let mut s = TextStyle::value(16.0);
                s.color = f.ui();
                p.text(
                    Vec2::new(top.x + dir * 14.0, sy),
                    &m.score(team).to_string(),
                    &s,
                    align,
                );
                let mut l = TextStyle::LABEL;
                l.color = with_alpha(f.ui(), 0.8);
                emblem::faction_name(
                    &mut p,
                    Vec2::new(top.x + dir * 52.0, sy + 6.0),
                    f,
                    &l,
                    align,
                );
            }
            p.line(
                Vec2::new(top.x, sy + 2.0),
                Vec2::new(top.x, sy + 14.0),
                1.0,
                theme::FAINT,
            );
        } else {
            // Free-for-all: every side's score in a row, yours first.
            let step = 110.0;
            for (team, &f) in m.factions.iter().enumerate() {
                let x = top.x + (team as f32 - (sides - 1) as f32 * 0.5) * step;
                let mut s = TextStyle::value(16.0);
                s.color = f.ui();
                p.text(
                    Vec2::new(x, sy),
                    &m.score(team).to_string(),
                    &s,
                    Align::Center,
                );
                let mut l = TextStyle::LABEL;
                l.color = with_alpha(f.ui(), 0.8);
                emblem::faction_name(&mut p, Vec2::new(x, sy + 20.0), f, &l, Align::Center);
            }
        }
        if let Some(hd) = &m.hold
            && !board
        {
            hold_meter(&mut p, m, hd, Vec2::new(top.x, sy + 32.0));
            // The site's tag, unless you're on it.
            let anchor = Vec3::new(hd.at.x, hd.ground + 9.0, hd.at.y);
            if !hd.inside(tank.pos)
                && let Some((x, y)) = project(vp, anchor, width, height)
            {
                let at = Vec2::new(x, y) / scale;
                if at.x >= 0.0 && at.x <= w && at.y >= 0.0 && at.y <= h {
                    let mut s = TextStyle::LABEL;
                    s.color = hold_color(m, hd.control);
                    let d = Vec2::new(tank.pos.x, tank.pos.z).distance(hd.at);
                    p.text(
                        at,
                        &format!("{}  {d:.0} M", m.world.map.hold_name),
                        &s,
                        Align::Center,
                    );
                }
            }
        }

        // Crosshair where the turret points; the gap kicks open when firing.
        // Where the gun actually points, including hull tilt; for a gun that
        // lands instantly, right where the shot would land.
        let aim_point = m
            .instant_landing(tank)
            .unwrap_or_else(|| tank.pos + tank.up() * tank.gun_height() + tank.turret_dir() * 40.0);
        if me.alive
            && !board
            && let Some((x, y)) = project(vp, aim_point, width, height)
        {
            let kick = (-tank.since_fire * 12.0).exp();
            // Flashes hot when one of your shells lands on someone.
            let color = if me.since_dealt < 0.2 {
                theme::HOT
            } else {
                theme::FG
            };
            widgets::crosshair(&mut p, Vec2::new(x, y) / scale, kick, color);
        }

        // Bottom centre: hull and reload.
        let bc = Vec2::new(w * 0.5, h - MARGIN);
        let max_hp = me.max_health(&m.world.tanks[0]);
        let segs = 12;
        // A husk (broken Vigil running on): the bar is the strength it has left.
        let (fill, name, value) = if me.is_husk() {
            (me.strength(), "HUSK", format!("{:.0}", me.husk.ceil()))
        } else {
            (
                me.health / max_hp,
                "HULL",
                format!("{:.0}", me.health.max(0.0)),
            )
        };
        let lit = (fill * segs as f32).ceil() as usize;
        let hot = (me.since_hit < 0.3 && lit < segs).then_some(lit);
        label(&mut p, bc + Vec2::new(-130.0, -40.0), name, Align::Left);
        p.text(
            bc + Vec2::new(130.0, -42.0),
            &value,
            &TextStyle::value(10.0),
            Align::Right,
        );
        widgets::segments(&mut p, bc + Vec2::new(-130.0, -26.0), 260.0, segs, lit, hot);
        let g = me.levels.gun(tank.turret_kind);
        if let crate::combat::Action::Beam { .. } = g.action {
            // Beam: the bar is heat. It glows hot as it nears the limit, and
            // stays hot until it has cooled right down after overheating.
            let color = if me.overheated {
                theme::HOT
            } else if me.heat > 0.75 {
                with_alpha(theme::HOT, 0.8)
            } else {
                theme::DIM
            };
            widgets::hairbar(&mut p, bc + Vec2::new(-130.0, -10.0), 260.0, me.heat, color);
            if me.overheated {
                let mut s = TextStyle::LABEL;
                s.color = theme::HOT;
                p.text(bc + Vec2::new(0.0, 4.0), "OVERHEATED", &s, Align::Center);
            }
        } else if me.charge > 0.0 {
            // Charge gun ringing up (or draining after a cancel): the bar is
            // the charge, hot while it builds.
            widgets::hairbar(
                &mut p,
                bc + Vec2::new(-130.0, -10.0),
                260.0,
                me.charge,
                if me.charging { theme::HOT } else { theme::DIM },
            );
        } else {
            let ready = 1.0 - (me.reload_in / g.reload).clamp(0.0, 1.0);
            widgets::hairbar(
                &mut p,
                bc + Vec2::new(-130.0, -10.0),
                260.0,
                ready,
                if ready >= 1.0 { theme::FG } else { theme::DIM },
            );
        }
        if me.since_repair < 1.8 {
            let mut s = TextStyle::value(10.0);
            s.color = with_alpha(
                theme::MEND,
                (1.0 - (me.since_repair - 1.2) / 0.6).clamp(0.0, 1.0),
            );
            p.text(
                bc + Vec2::new(0.0, -78.0),
                &format!("+{:.0} HULL", me.repaired),
                &s,
                Align::Center,
            );
        }
        if me.shield > 0.0 && me.alive {
            label(
                &mut p,
                bc + Vec2::new(0.0, -60.0),
                "SPAWN SHIELD",
                Align::Center,
            );
        }

        // Death screen.
        if !me.alive && m.phase == Phase::Playing {
            let c = Vec2::new(w * 0.5, h * 0.36);
            let mut big = TextStyle::value(26.0);
            big.color = theme::HOT;
            p.text(c, "DESTROYED", &big, Align::Center);
            if let Some(k) = me.killed_by.filter(|&k| k != 0) {
                let mut s = TextStyle::LABEL;
                s.color = side_color(m, k);
                let f = m.factions[m.fighters[k].team];
                emblem::tagged(
                    &mut p,
                    c + Vec2::new(0.0, 42.0),
                    f,
                    &format!("BY {} {}", f.name(), callsign(m, k)),
                    &s,
                    Align::Center,
                );
            }
            p.text(
                c + Vec2::new(0.0, 62.0),
                &format!("RESPAWN {}", me.respawn_in.ceil().max(1.0) as u32),
                &TextStyle::value(11.0),
                Align::Center,
            );
        }

        // Reputation popups, just above the crosshair, drifting up as they fade.
        if !board {
            for (row, x) in m.reputation_pops.iter().rev().take(3).enumerate() {
                let fade = ((REPUTATION_POP_TIME - x.age) / 0.6).clamp(0.0, 1.0)
                    * (x.age / 0.08).clamp(0.0, 1.0);
                let at = Vec2::new(w * 0.5, h * 0.30 - x.age * 14.0 - row as f32 * 30.0);
                let mut s = TextStyle::value(16.0);
                s.color = with_alpha(theme::HOT, fade);
                p.text(at, &format!("+{} REPUTATION", x.amount), &s, Align::Center);
                let mut l = TextStyle::LABEL;
                l.color = with_alpha(theme::FG, 0.8 * fade);
                p.text(at + Vec2::new(0.0, 24.0), x.reason, &l, Align::Center);
            }
        }

        if board {
            // Four sides stack two rows of tables: start higher.
            let top = if m.factions.len() > 2 { 0.17 } else { 0.28 };
            scoreboard(
                &mut p,
                &mut hits,
                self.hover,
                m,
                ledger,
                Vec2::new(w * 0.5, h * top),
            );
        }

        // Bottom left: speed.
        let bl = Vec2::new(MARGIN, h - MARGIN);
        label(&mut p, bl + Vec2::new(0.0, -62.0), "SPEED", Align::Left);
        let num = TextStyle::value(22.0);
        let nw = widgets::odometer(
            &mut p,
            bl + Vec2::new(0.0, -48.0),
            self.speed,
            3,
            &num,
            Align::Left,
        );
        label(
            &mut p,
            bl + Vec2::new(nw + 10.0, -48.0 + num.size - TextStyle::LABEL.size),
            "KM/H",
            Align::Left,
        );
        widgets::hairbar(
            &mut p,
            bl + Vec2::new(0.0, -8.0),
            200.0,
            self.speed / max_speed,
            theme::FG,
        );

        // Bottom right: turret angle relative to the hull.
        let br = Vec2::new(w - MARGIN, h - MARGIN);
        widgets::turret_gauge(&mut p, br + Vec2::new(-30.0, -30.0), tank.turret_rel);

        // Bottom centre: the kit queued for the next respawn, if any.
        let at = Vec2::new(w * 0.5, h - MARGIN - 8.0);
        if let Some((hull, turret)) = me.next_kit {
            let kit = format!(
                "{} · {}",
                crate::garage::hull_name(hull),
                crate::garage::turret_name(turret)
            );
            label(
                &mut p,
                at - Vec2::new(0.0, 30.0),
                "NEXT SPAWN",
                Align::Center,
            );
            p.text(
                at - Vec2::new(0.0, 18.0),
                &kit,
                &TextStyle::value(10.0),
                Align::Center,
            );
        }

        Layer {
            verts: p.finish(),
            hits,
        }
    }
}

/// Each side's standings table, two to a row (your side first, top left),
/// with a winner banner once the round is over.
fn scoreboard(
    p: &mut Painter,
    hits: &mut Hits<Hit>,
    hover: Option<Hit>,
    m: &Match,
    ledger: Ledger,
    top: Vec2,
) {
    let holding = m.contract == Contract::Hold;
    let cols = StatColumns::new(holding);
    // Wide enough for the longest tank name in the match beside the stat columns.
    let widest = (0..m.fighters.len())
        .map(|i| measure(&kit_name(m, i), &TextStyle::value(ROW_SIZE)))
        .fold(0.0, f32::max);
    let col = (widest + NAME_GAP + cols.width).max(380.0).ceil();
    let gap = 48.0;
    let sides = m.factions.len();
    let rows = (0..sides)
        .map(|t| m.fighters.iter().filter(|f| f.team == t).count())
        .max()
        .unwrap_or(0) as f32;
    // Tables stack two to a row.
    let block = 56.0 + rows * 22.0;
    let block_gap = 24.0;
    let blocks = sides.div_ceil(2) as f32;
    let body = blocks * block + (blocks - 1.0) * block_gap;
    let width = col * 2.0 + gap;
    let mut y = top.y;
    let (x0, x1) = (top.x - width * 0.5, top.x + width * 0.5);
    let backdrop = theme::PANEL;
    let height = if m.phase == Phase::Over {
        24.0 + 66.0 + 18.0 + RESULTS_H + 24.0 + BUTTON_H + 24.0
    } else {
        28.0
    } + body;
    p.rect(
        Vec2::new(x0 - 24.0, y - 24.0),
        Vec2::new(width + 48.0, height),
        backdrop,
    );
    widgets::corners(
        p,
        Vec2::new(x0 - 24.0, y - 24.0),
        Vec2::new(x1 + 24.0, y - 24.0 + height),
        14.0,
        theme::DIM,
    );
    if m.phase == Phase::Over {
        let mut big = TextStyle::value(22.0);
        big.color = theme::HOT;
        p.text(Vec2::new(top.x, y), "ROUND OVER", &big, Align::Center);
        let mut s = TextStyle::value(12.0);
        let at = Vec2::new(top.x, y + 34.0);
        match m.leader() {
            Some(t) => {
                let f = m.factions[t];
                s.color = f.ui();
                emblem::tagged(p, at, f, &format!("{} WINS", f.name()), &s, Align::Center);
            }
            None => {
                p.text(at, "DRAW", &s, Align::Center);
            }
        }
        y += 66.0;
    }
    let order = m.standings();
    for team in 0..sides {
        let (a, b) = if team % 2 == 0 {
            (x0, x0 + col)
        } else {
            (x1 - col, x1)
        };
        let y = y + (team / 2) as f32 * (block + block_gap);
        let f = m.factions[team];
        let mut name = TextStyle::value(12.0);
        name.color = f.ui();
        emblem::faction_name(p, Vec2::new(a, y), f, &name, Align::Left);
        let mut score = TextStyle::value(18.0);
        score.color = f.ui();
        p.text(
            Vec2::new(b, y - 4.0),
            &m.score(team).to_string(),
            &score,
            Align::Right,
        );
        let mut ty = y + 32.0;
        label(p, Vec2::new(a, ty), "TANK", Align::Left);
        for (name, dx) in cols.headers() {
            label(p, Vec2::new(b - dx, ty), name, Align::Right);
        }
        ty += 14.0;
        p.line(
            Vec2::new(a, ty),
            Vec2::new(b, ty),
            1.0,
            with_alpha(f.ui(), 0.4),
        );
        ty += 10.0;
        for &i in order.iter().filter(|&&i| m.fighters[i].team == team) {
            let fi = &m.fighters[i];
            let mut s = TextStyle::value(ROW_SIZE);
            if i == 0 {
                s.color = theme::HOT;
            }
            p.text(Vec2::new(a, ty), &kit_name(m, i), &s, Align::Left);
            if let Some(dx) = cols.held {
                p.text(
                    Vec2::new(b - dx, ty),
                    &format!("{:.0}", m.held(i).floor()),
                    &s,
                    Align::Right,
                );
            }
            p.text(
                Vec2::new(b - cols.kills, ty),
                &fi.kills.to_string(),
                &s,
                Align::Right,
            );
            p.text(Vec2::new(b, ty), &fi.deaths.to_string(), &s, Align::Right);
            ty += 22.0;
        }
    }
    y += body;
    if m.phase == Phase::Over {
        let y = y + 18.0;
        let employer = m.factions[0];
        let t = m.over_for - RESULTS_DELAY;
        reputation_gained(
            p,
            employer,
            ledger.reputation,
            m.reputation,
            t,
            x0,
            x0 + col,
            y,
        );
        crystal_tally(
            p,
            ledger.crystals,
            Payout::of(m).total(),
            t,
            x1 - col,
            x1,
            y,
        );
        round_over_buttons(p, hits, hover, Vec2::new(top.x, y + RESULTS_H + 24.0));
    }
}

const BUTTON_H: f32 = 26.0;
const BUTTON_PAD: f32 = 12.0;
const BUTTON_GAP: f32 = 16.0;

/// The round-over choices, side by side and centred on `top`: buttons that
/// click like their keys.
fn round_over_buttons(p: &mut Painter, hits: &mut Hits<Hit>, hover: Option<Hit>, top: Vec2) {
    let buttons = [("AGAIN", Hit::Again), ("MENU", Hit::Menu)];
    let style = TextStyle::LABEL;
    let width = |what: &str| BUTTON_PAD * 2.0 + measure(what, &style);
    let total = buttons.iter().map(|(w, _)| width(w)).sum::<f32>()
        + BUTTON_GAP * (buttons.len() - 1) as f32;
    let mut x = top.x - total * 0.5;
    for (what, id) in buttons {
        let min = Vec2::new(x, top.y);
        let size = Vec2::new(width(what), BUTTON_H);
        let fill = if hover == Some(id) { 0.16 } else { 0.07 };
        p.rect(min, size, with_alpha(theme::FG, fill));
        p.rect_outline(min, size, 1.0, theme::FG);
        let ty = min.y + (BUTTON_H - style.size) * 0.5;
        let mut a = style;
        a.color = theme::FG;
        p.text(Vec2::new(min.x + BUTTON_PAD, ty), what, &a, Align::Left);
        hits.add(p, min, size, id);
        x += size.x + BUTTON_GAP;
    }
}

/// Scoreboard row text size.
const ROW_SIZE: f32 = 11.0;
/// Space between a tank's name and the first stat column, and between stat columns.
const NAME_GAP: f32 = 20.0;
const STAT_GAP: f32 = 22.0;

/// A tank's scoreboard name: hull and turret, yours marked.
fn kit_name(m: &Match, i: usize) -> String {
    let t = &m.world.tanks[i];
    format!(
        "{}{} · {}",
        if i == 0 { "YOU  " } else { "" },
        crate::garage::hull_name(t.hull_kind),
        crate::garage::turret_name(t.turret_kind)
    )
}

/// The scoreboard's right-aligned stat columns, each one's right edge as a
/// distance from the table's right edge, spaced by their headers' widths
/// (wider than any count under them).
struct StatColumns {
    held: Option<f32>,
    kills: f32,
    /// From the left edge of the leftmost header to the table's right edge.
    width: f32,
}

impl StatColumns {
    fn new(holding: bool) -> StatColumns {
        let w = |s: &str| {
            measure(s, &TextStyle::LABEL).max(measure("000", &TextStyle::value(ROW_SIZE)))
        };
        let kills = w("DEATHS") + STAT_GAP;
        let held = holding.then(|| kills + w("KILLS") + STAT_GAP);
        let width = match held {
            Some(h) => h + w("HELD"),
            None => kills + w("KILLS"),
        };
        StatColumns { held, kills, width }
    }

    /// Each header with its right edge's distance from the table's right edge.
    fn headers(&self) -> Vec<(&'static str, f32)> {
        let mut out: Vec<_> = self.held.map(|h| ("HELD", h)).into_iter().collect();
        out.push(("KILLS", self.kills));
        out.push(("DEATHS", 0.0));
        out
    }
}

/// Who has the site, then each side's time held against the target: your
/// side's bar grows left from the middle, theirs right.
fn hold_meter(p: &mut Painter, m: &Match, hd: &hold::Hold, at: Vec2) {
    let status = match hd.control {
        Control::Open => format!("{} OPEN", m.world.map.hold_name),
        Control::Held(s) => format!("{} HOLDS", m.factions[s].name()),
        Control::Contested => "CONTESTED".to_string(),
    };
    let mut l = TextStyle::LABEL;
    l.color = hold_color(m, hd.control);
    match hd.control {
        Control::Held(s) => emblem::tagged(p, at, m.factions[s], &status, &l, Align::Center),
        _ => p.text(at, &status, &l, Align::Center),
    };
    let half = 110.0;
    let y = at.y + 20.0;
    p.line(
        Vec2::new(at.x - half, y),
        Vec2::new(at.x + half, y),
        1.0,
        theme::FAINT,
    );
    for (side, dir) in [(0, -1.0), (1, 1.0)] {
        let f = (hd.held[side] / hold::TARGET).clamp(0.0, 1.0);
        if f > 0.0 {
            p.line(
                Vec2::new(at.x, y),
                Vec2::new(at.x + dir * half * f, y),
                2.0,
                m.factions[side].ui(),
            );
        }
        // Tick at each end: the target.
        let end = at.x + dir * half;
        p.line(
            Vec2::new(end, y - 4.0),
            Vec2::new(end, y + 4.0),
            1.0,
            theme::DIM,
        );
    }
}

fn hold_color(m: &Match, c: Control) -> Rgba {
    match c {
        Control::Held(s) => m.factions[s].ui(),
        Control::Contested => theme::HOT,
        Control::Open => theme::DIM,
    }
}

/// Seconds after the round ends before the results start moving.
const RESULTS_DELAY: f32 = 0.6;
/// Height of the reputation and crystal blocks under the tables.
const RESULTS_H: f32 = 84.0;

/// Reputation: the bar fills from where you stood over this long.
const REP_FILL: f32 = 1.2;
/// Crystals: "+0" joins the balance, ticks up to the pay, then folds into it.
const CR_PLUS: f32 = 0.2;
const CR_TICK: f32 = 0.6;
const CR_TICK_TIME: f32 = 1.0;
const CR_FOLD: f32 = 1.9;
const CR_FOLD_TIME: f32 = 0.4;

fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

/// How much of `gained` the reputation graphic shows `t` seconds in.
fn reputation_shown(gained: u32, t: f32) -> u32 {
    (gained as f32 * ease_out(t / REP_FILL)).round() as u32
}

/// What the crystal tally reads `t` seconds in: the balance, and the pay
/// beside it (with its opacity) until it folds in.
#[derive(Debug, PartialEq)]
struct Tally {
    balance: u32,
    plus: Option<(u32, f32)>,
}

fn tally(before: u32, pay: u32, t: f32) -> Tally {
    let fold = ((t - CR_FOLD) / CR_FOLD_TIME).clamp(0.0, 1.0);
    let balance = before + (pay as f32 * ease_out(fold)).round() as u32;
    let plus = (t >= CR_PLUS && fold < 1.0).then(|| {
        // Ticks: the count steps evenly, so every crystal shows.
        let k = (pay as f32 * ((t - CR_TICK) / CR_TICK_TIME).clamp(0.0, 1.0)).floor() as u32;
        let alpha = (((t - CR_PLUS) / 0.15).clamp(0.0, 1.0)) * (1.0 - fold);
        (k, alpha)
    });
    Tally { balance, plus }
}

/// Reputation gained with your employer, as a graphic: the standing bar filling
/// from where you stood, the new part lit, the standing ticking over if it fills.
#[allow(clippy::too_many_arguments)]
fn reputation_gained(
    p: &mut Painter,
    employer: crate::faction::Faction,
    before: u32,
    gained: u32,
    t: f32,
    a: f32,
    b: f32,
    y: f32,
) {
    let shown = reputation_shown(gained, t);
    let start = Standing::of(before);
    let now = Standing::of(before + shown);
    label(p, Vec2::new(a, y), "REPUTATION", Align::Left);
    let mut name = TextStyle::LABEL;
    name.color = employer.ui();
    emblem::faction_name(p, Vec2::new(b, y), employer, &name, Align::Right);

    let mut big = TextStyle::value(22.0);
    big.color = theme::HOT;
    p.text(
        Vec2::new(a, y + 18.0),
        &format!("+{shown}"),
        &big,
        Align::Left,
    );
    let mut tier = TextStyle::value(14.0);
    let up = now.level > start.level;
    if up {
        tier.color = theme::HOT;
    }
    let tw = p.text(
        Vec2::new(b, y + 24.0),
        &format!("T{}", now.level),
        &tier,
        Align::Right,
    );
    label(
        p,
        Vec2::new(b - tw - 8.0, y + 27.0),
        if up { "STANDING UP" } else { "STANDING" },
        Align::Right,
    );

    // The bar: where you stood in the employer's colour's neutral, this match lit.
    let (bar_y, bar_h, width) = (y + 56.0, 6.0, b - a);
    p.rect(Vec2::new(a, bar_y), Vec2::new(width, bar_h), theme::FAINT);
    let from = if up { 0.0 } else { start.progress() };
    let to = now.progress();
    if from > 0.0 {
        p.rect(
            Vec2::new(a, bar_y),
            Vec2::new(width * from, bar_h),
            theme::DIM,
        );
    }
    if to > from {
        p.rect(
            Vec2::new(a + width * from, bar_y),
            Vec2::new(width * (to - from), bar_h),
            theme::HOT,
        );
    }
    // Quarter ticks under the bar, so the fill reads as distance.
    for q in 0..=4 {
        let x = a + width * q as f32 / 4.0;
        p.line(
            Vec2::new(x, bar_y + bar_h + 2.0),
            Vec2::new(x, bar_y + bar_h + 6.0),
            1.0,
            theme::FAINT,
        );
    }
    label(
        p,
        Vec2::new(b, bar_y + bar_h + 10.0),
        &format!("{} / {} TO T{}", now.into, now.span, now.level + 1),
        Align::Right,
    );
}

/// Contract pay as one clean number: your crystals, then "+0" beside them
/// ticking up to the pay, which then folds into the balance.
fn crystal_tally(p: &mut Painter, before: u32, pay: u32, t: f32, a: f32, b: f32, y: f32) {
    let now = tally(before, pay, t);
    label(p, Vec2::new(a, y), "CRYSTALS", Align::Left);
    label(p, Vec2::new(b, y), "CR", Align::Right);
    let big = TextStyle::value(22.0);
    let bw = p.text(
        Vec2::new(a, y + 18.0),
        &now.balance.to_string(),
        &big,
        Align::Left,
    );
    if let Some((k, alpha)) = now.plus {
        let mut s = TextStyle::value(22.0);
        s.color = with_alpha(theme::HOT, alpha);
        p.text(
            Vec2::new(a + bw + 14.0, y + 18.0),
            &format!("+{k}"),
            &s,
            Align::Left,
        );
    }
    p.line(
        Vec2::new(a, y + 59.0),
        Vec2::new(b, y + 59.0),
        1.0,
        theme::FAINT,
    );
    // Once folded in, what the contract paid, small.
    let done = ((t - CR_FOLD - CR_FOLD_TIME) / 0.3).clamp(0.0, 1.0);
    if done > 0.0 {
        let mut l = TextStyle::LABEL;
        l.color = with_alpha(theme::DIM, done);
        p.text(
            Vec2::new(b, y + 66.0),
            &format!("+{pay} CONTRACT PAY"),
            &l,
            Align::Right,
        );
    }
}

/// The debug overlay's frame rate, small in the top right corner above the HUD brackets.
pub fn fps_counter(fps: f32, width: f32, height: f32) -> Vec<UiVertex> {
    let scale = height / REF_HEIGHT;
    let w = width / scale;
    let mut p = Painter::new(scale);
    let mut s = TextStyle::LABEL;
    s.color = theme::FG;
    p.text(
        Vec2::new(w - 24.0, 4.0),
        &format!("{fps:.0} FPS"),
        &s,
        Align::Right,
    );
    p.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Camera;
    use glam::Vec3;

    #[test]
    fn scoreboard_fits_any_kit_on_a_square_screen() {
        // The longest hull and turret names, marked as yours, beside every stat column.
        let widest = crate::garage::HULLS
            .iter()
            .flat_map(|&h| crate::garage::TURRETS.iter().map(move |&t| (h, t)))
            .map(|(h, t)| {
                let name = format!(
                    "YOU  {} · {}",
                    crate::garage::hull_name(h),
                    crate::garage::turret_name(t)
                );
                measure(&name, &TextStyle::value(ROW_SIZE))
            })
            .fold(0.0, f32::max);
        let col = widest + NAME_GAP + StatColumns::new(true).width;
        // Two tables, the gap between them and the panel's margins, at 4:3 (1200 logical px wide).
        assert!(col * 2.0 + 48.0 + 48.0 < 1200.0, "table {col} px");
    }

    #[test]
    fn stat_columns_dont_touch() {
        for holding in [false, true] {
            let c = StatColumns::new(holding);
            let h = c.headers();
            for pair in h.windows(2) {
                let (left, l) = pair[0];
                let (right, r) = pair[1];
                // The right column's header starts after the left column ends.
                assert!(
                    r + measure(right, &TextStyle::LABEL) + 8.0 < l,
                    "{left} {right}"
                );
            }
        }
    }

    #[test]
    fn crystals_show_x_then_x_plus_0_then_tick_up_and_fold_in() {
        // Before anything moves: just the balance.
        assert_eq!(
            tally(1200, 250, -0.5),
            Tally {
                balance: 1200,
                plus: None
            }
        );
        // Then "+0" beside it.
        let t = tally(1200, 250, CR_PLUS + 0.2);
        assert_eq!((t.balance, t.plus.map(|p| p.0)), (1200, Some(0)));
        // Ticking up, never past the pay, never backwards.
        let mut last = 0;
        let mut t = CR_TICK;
        while t < CR_FOLD {
            let k = tally(1200, 250, t).plus.unwrap().0;
            assert!(k >= last && k <= 250);
            last = k;
            t += 1.0 / 60.0;
        }
        assert_eq!(last, 250);
        // Folded in: one clean number.
        let end = tally(1200, 250, CR_FOLD + CR_FOLD_TIME + 1.0);
        assert_eq!(
            end,
            Tally {
                balance: 1450,
                plus: None
            }
        );
    }

    #[test]
    fn reputation_fills_to_what_was_gained() {
        assert_eq!(reputation_shown(37, -1.0), 0);
        assert!(reputation_shown(37, REP_FILL * 0.5) < 37);
        assert_eq!(reputation_shown(37, REP_FILL), 37);
    }

    #[test]
    fn ledger_backs_out_this_matchs_earnings() {
        let mut m = Match::new(crate::map::Map::saltmarket(), 0, 11, 21);
        m.phase = Phase::Over;
        let mut p = Profile::in_memory();
        p.reputation[m.factions[0].index()] = 140;
        p.crystals = 1000 + Payout::of(&m).total();
        let l = Ledger::before(&p, &m, 40, true);
        assert_eq!((l.reputation, l.crystals), (100, 1000));
        // Pay not banked yet: the balance is still the one before.
        p.crystals = 1000;
        assert_eq!(Ledger::before(&p, &m, 40, false).crystals, 1000);
    }

    #[test]
    fn round_over_has_buttons() {
        let mut m = Match::new(crate::map::Map::saltmarket(), 0, 11, 21);
        let t = &m.world.tanks[0];
        let camera = Camera {
            pos: t.pos + Vec3::new(0.0, 6.0, -10.0),
            target: t.pos,
            fov_y: 1.1,
        };
        let mut hud = Hud::new();
        let draw = |hud: &mut Hud, m: &Match| {
            hud.draw(
                m,
                &m.world.tanks,
                &camera,
                1600.0,
                900.0,
                0.0,
                true,
                Ledger::default(),
            )
            .hits
        };
        // Mid-round the scoreboard has nothing to click.
        assert!(draw(&mut hud, &m).is_empty());
        m.phase = Phase::Over;
        let hits = draw(&mut hud, &m);
        for id in [Hit::Again, Hit::Menu] {
            let at = hits.center_of(id).expect("button drawn");
            assert_eq!(hits.at(at).map(|(h, _)| h), Some(id));
        }
        // Side by side, not on top of each other.
        assert!(hits.center_of(Hit::Again).unwrap().x < hits.center_of(Hit::Menu).unwrap().x);
    }

    #[test]
    fn draws_at_any_size() {
        for contract in [Contract::Attrition, Contract::Hold] {
            draws_at_any_size_under(contract);
        }
    }

    fn draws_at_any_size_under(contract: Contract) {
        let mut m = Match::new(crate::map::Map::saltmarket(), 0, 11, 21);
        m.set_contract(contract);
        m.fighters[0].alive = false;
        m.phase = Phase::Over;
        let t = &m.world.tanks[0];
        // Chase-camera framing: behind the turret, looking where it points.
        let d = t.turret_dir();
        let camera = Camera {
            pos: t.pos - d * 10.0 + Vec3::Y * 6.0,
            target: t.pos + d * 6.0,
            fov_y: 1.1,
        };
        let mut hud = Hud::new();
        for (w, h) in [(1600.0, 900.0), (800.0, 600.0), (3840.0, 2160.0)] {
            let v = hud
                .draw(
                    &m,
                    &m.world.tanks,
                    &camera,
                    w,
                    h,
                    1.0 / 60.0,
                    true,
                    Ledger::default(),
                )
                .verts;
            assert!(!v.is_empty());
            // A little slack for halos and anti-aliasing at the screen edge.
            assert!(v.iter().all(|v| v.pos[0] > -4.0
                && v.pos[0] < w + 4.0
                && v.pos[1] > -4.0
                && v.pos[1] < h + 4.0));
        }
    }
}
