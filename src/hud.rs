//! Driving HUD in the hairline style: readouts in the four corners inside
//! faint brackets, the middle left to the city and the crosshair.
//!
//! Team deathmatch readouts: round clock and kills top left, team scores
//! under the compass, kill feed top right, hull and reload bottom centre,
//! team-coloured health bars over tanks, a death screen, and the scoreboard
//! (Tab, or when the round ends). Tanks have no names: "YOU", or their turret.

use glam::{Vec2, Vec3};

use crate::combat::{FEED_TIME, Match, Phase, TRUST_POP_TIME};
use crate::economy::{Outcome, Payout};
use crate::hold::{self, Control};
use crate::mission::Contract;
use crate::render::{Camera, UiVertex};
use crate::repair::State;
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

        // Top left: round clock, kills / deaths, trust.
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
            "TRUST",
            Align::Left,
        );
        p.text(
            Vec2::new(MARGIN + 150.0, MARGIN + 64.0),
            &format!("+{}", m.trust),
            &TextStyle::value(14.0),
            Align::Left,
        );

        // Top right: kill feed, newest on top, each side in its colour.
        let tr = Vec2::new(w - MARGIN, MARGIN);
        for (row, k) in m.feed.iter().rev().take(5).enumerate() {
            let fade = ((FEED_TIME - k.age) / 0.8).clamp(0.0, 1.0);
            let y = tr.y + row as f32 * 18.0;
            let mut s = TextStyle::value(10.0);
            s.tracking = 2.5;
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

        // Repair drop zones in view: a countdown over each empty pad, a marker over each crate.
        for d in &m.repairs.drops {
            let ground = Vec3::new(d.at.x, d.ground, d.at.y);
            let (anchor, text, color) = match d.state {
                State::Waiting(t) => (
                    ground + Vec3::Y * 1.6,
                    format!("REPAIR {}", t.ceil() as u32),
                    theme::DIM,
                ),
                _ => (
                    d.crate_pos().unwrap_or(ground) + Vec3::Y * 2.4,
                    "REPAIR".to_string(),
                    theme::MEND,
                ),
            };
            let dist = anchor.distance(camera.pos);
            if board || dist > 140.0 {
                continue;
            }
            let Some((x, y)) = project(vp, anchor, width, height) else {
                continue;
            };
            let at = Vec2::new(x, y) / scale;
            if at.x < 0.0 || at.x > w || at.y < 0.0 || at.y > h {
                continue;
            }
            let mut s = TextStyle::LABEL;
            s.color = with_alpha(
                color,
                color[3] * (1.0 - (dist - 100.0) / 40.0).clamp(0.0, 1.0),
            );
            p.text(at, &text, &s, Align::Center);
        }

        // Top centre: turret heading, and under it the team score (your side on the left).
        let top = Vec2::new(w * 0.5, MARGIN + 6.0);
        widgets::compass(&mut p, top, 360.0, self.heading);
        let hdg = self.heading.round() as u32 % 360;
        p.text(
            top + Vec2::new(0.0, 32.0),
            &format!("{hdg:03}"),
            &TextStyle::value(11.0),
            Align::Center,
        );
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
        // Where the gun actually points, including hull tilt.
        let aim_point = tank.pos + tank.up() * tank.gun_height() + tank.turret_dir() * 40.0;
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

        // Trust popups, just above the crosshair, drifting up as they fade.
        if !board {
            for (row, x) in m.trust_pops.iter().rev().take(3).enumerate() {
                let fade = ((TRUST_POP_TIME - x.age) / 0.6).clamp(0.0, 1.0)
                    * (x.age / 0.08).clamp(0.0, 1.0);
                let at = Vec2::new(w * 0.5, h * 0.30 - x.age * 14.0 - row as f32 * 30.0);
                let mut s = TextStyle::value(16.0);
                s.color = with_alpha(theme::HOT, fade);
                p.text(at, &format!("+{} TRUST", x.amount), &s, Align::Center);
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
        let rel = tank.turret_rel.to_degrees().round() as i32;
        let side = if rel > 0 {
            " L"
        } else if rel < 0 {
            " R"
        } else {
            ""
        };
        label(&mut p, br + Vec2::new(-84.0, -62.0), "TURRET", Align::Right);
        p.text(
            br + Vec2::new(-84.0, -48.0),
            &format!("{:03}{side}", rel.abs()),
            &TextStyle::value(14.0),
            Align::Right,
        );

        // Bottom centre: how to get to the garage.
        let mut key = TextStyle::LABEL;
        key.color = theme::FG;
        let hint = Vec2::new(w * 0.5, h - MARGIN - 8.0);
        let gw = crate::ui::measure("GARAGE", &TextStyle::LABEL);
        p.text(
            hint - Vec2::new(gw * 0.5 + 10.0, 0.0),
            "G",
            &key,
            Align::Right,
        );
        label(&mut p, hint, "GARAGE", Align::Center);
        // Above it: the kit queued for the next respawn, if any.
        if let Some((hull, turret)) = me.next_kit {
            let kit = format!(
                "{} · {}",
                crate::garage::hull_name(hull),
                crate::garage::turret_name(turret)
            );
            label(
                &mut p,
                hint - Vec2::new(0.0, 30.0),
                "NEXT SPAWN",
                Align::Center,
            );
            p.text(
                hint - Vec2::new(0.0, 18.0),
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
fn scoreboard(p: &mut Painter, hits: &mut Hits<Hit>, hover: Option<Hit>, m: &Match, top: Vec2) {
    let holding = m.contract == Contract::Hold;
    // Hold adds a HELD column: wider tables so it clears the tank names.
    let col = if holding { 410.0 } else { 380.0 };
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
    let backdrop = [0.02, 0.02, 0.025, 0.7];
    let extra = if holding { 18.0 } else { 0.0 };
    let height = if m.phase == Phase::Over {
        306.0 + extra
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
        emblem::tagged(
            p,
            Vec2::new(top.x, y + 56.0),
            m.factions[0],
            &format!("+{} {} TRUST THIS MATCH", m.trust, m.factions[0].name()),
            &TextStyle::LABEL,
            Align::Center,
        );
        y += 84.0;
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
        if holding {
            label(p, Vec2::new(b - 130.0, ty), "HELD", Align::Right);
        }
        label(p, Vec2::new(b - 70.0, ty), "KILLS", Align::Right);
        label(p, Vec2::new(b, ty), "DEATHS", Align::Right);
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
            let t = &m.world.tanks[i];
            let mut s = TextStyle::value(11.0);
            if i == 0 {
                s.color = theme::HOT;
            }
            let kit = format!(
                "{}{} · {}",
                if i == 0 { "YOU  " } else { "" },
                crate::garage::hull_name(t.hull_kind),
                crate::garage::turret_name(t.turret_kind)
            );
            p.text(Vec2::new(a, ty), &kit, &s, Align::Left);
            if holding {
                p.text(
                    Vec2::new(b - 130.0, ty),
                    &format!("{:.0}", m.held(i).floor()),
                    &s,
                    Align::Right,
                );
            }
            p.text(
                Vec2::new(b - 70.0, ty),
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
        contract_pay(p, m, x0, x1, y + 14.0);
        round_over_buttons(p, hits, hover, Vec2::new(top.x, y + 150.0 + extra));
    }
}

const BUTTON_H: f32 = 26.0;
const BUTTON_PAD: f32 = 12.0;
const BUTTON_GAP: f32 = 16.0;

/// The round-over choices, side by side and centred on `top`: buttons that
/// click like their keys.
fn round_over_buttons(p: &mut Painter, hits: &mut Hits<Hit>, hover: Option<Hit>, top: Vec2) {
    let buttons = [("ENTER", "AGAIN", Hit::Again), ("ESC", "MENU", Hit::Menu)];
    let style = TextStyle::LABEL;
    let key_gap = 10.0;
    let width = |key: &str, what: &str| {
        BUTTON_PAD * 2.0 + measure(key, &style) + key_gap + measure(what, &style)
    };
    let total = buttons.iter().map(|(k, w, _)| width(k, w)).sum::<f32>()
        + BUTTON_GAP * (buttons.len() - 1) as f32;
    let mut x = top.x - total * 0.5;
    for (key, what, id) in buttons {
        let min = Vec2::new(x, top.y);
        let size = Vec2::new(width(key, what), BUTTON_H);
        let fill = if hover == Some(id) { 0.16 } else { 0.07 };
        p.rect(min, size, with_alpha(theme::FG, fill));
        p.rect_outline(min, size, 1.0, theme::FG);
        let ty = min.y + (BUTTON_H - style.size) * 0.5;
        let mut k = style;
        k.color = theme::HOT;
        let kw = p.text(Vec2::new(min.x + BUTTON_PAD, ty), key, &k, Align::Left);
        let mut a = style;
        a.color = theme::FG;
        p.text(
            Vec2::new(min.x + BUTTON_PAD + kw + key_gap, ty),
            what,
            &a,
            Align::Left,
        );
        hits.add(p, min, size, id);
        x += size.x + BUTTON_GAP;
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
    label(
        p,
        Vec2::new(at.x, y + 8.0),
        &format!("FIRST TO {:.0} S", hold::TARGET),
        Align::Center,
    );
}

fn hold_color(m: &Match, c: Control) -> Rgba {
    match c {
        Control::Held(s) => m.factions[s].ui(),
        Control::Contested => theme::HOT,
        Control::Open => theme::DIM,
    }
}

/// Seconds after the round ends before the pay lines start, and between them.
const PAY_DELAY: f32 = 0.6;
const PAY_STEP: f32 = 0.35;

/// The contract's crystal pay, line by line, then the total counting up.
fn contract_pay(p: &mut Painter, m: &Match, x0: f32, x1: f32, mut y: f32) {
    let pay = Payout::of(m);
    let t = m.over_for - PAY_DELAY;
    label(p, Vec2::new(x0, y), "CONTRACT PAY", Align::Left);
    label(p, Vec2::new(x1, y), "CR", Align::Right);
    y += 14.0;
    p.line(Vec2::new(x0, y), Vec2::new(x1, y), 1.0, theme::FAINT);
    y += 10.0;
    let lines = [
        ("COMPLETION".to_string(), pay.completion),
        (
            match pay.outcome {
                Outcome::Won => "VICTORY",
                Outcome::Draw => "DRAW",
                Outcome::Lost => "DEFEAT",
            }
            .to_string(),
            pay.victory,
        ),
        (
            format!("CONTRIBUTION  {:.0} HP", pay.damage),
            pay.contribution,
        ),
        (format!("BOUNTY  {} KILLS", pay.kills), pay.bounty),
    ];
    let hold = (m.contract == Contract::Hold).then(|| {
        (
            format!("ON THE {}  {:.0} S", m.world.map.hold_name, pay.held),
            pay.hold,
        )
    });
    let lines: Vec<_> = lines.into_iter().chain(hold).collect();
    let n = lines.len() as f32;
    for (i, (what, amount)) in lines.iter().enumerate() {
        let a = ((t - i as f32 * PAY_STEP) / 0.2).clamp(0.0, 1.0);
        if a > 0.0 {
            let mut s = TextStyle::value(10.0);
            s.color = with_alpha(theme::FG, a * 0.9);
            let mut l = TextStyle::LABEL;
            l.color = with_alpha(theme::DIM, a);
            p.text(Vec2::new(x0, y + 1.0), what, &l, Align::Left);
            p.text(Vec2::new(x1, y), &amount.to_string(), &s, Align::Right);
        }
        y += 18.0;
    }
    // The total rolls up once the lines are in.
    let roll = ((t - n * PAY_STEP) / 1.0).clamp(0.0, 1.0);
    if roll > 0.0 {
        let shown = (pay.total() as f32 * (1.0 - (1.0 - roll).powi(3))).round() as u32;
        let mut s = TextStyle::value(14.0);
        s.color = theme::HOT;
        label(p, Vec2::new(x0, y + 5.0), "TOTAL", Align::Left);
        p.text(Vec2::new(x1, y), &format!("+{shown}"), &s, Align::Right);
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
            hud.draw(m, &m.world.tanks, &camera, 1600.0, 900.0, 0.0, true)
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
                .draw(&m, &m.world.tanks, &camera, w, h, 1.0 / 60.0, true)
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
