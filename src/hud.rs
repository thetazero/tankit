//! Driving HUD in the hairline style: readouts in the four corners inside
//! faint brackets, the middle left to the city and the crosshair.
//!
//! Team deathmatch readouts: round clock and kills top left, team scores
//! under the compass, kill feed top right, hull and reload bottom centre,
//! team-coloured health bars over tanks, a death screen, and the scoreboard
//! (Tab, or when the round ends). Tanks have no names: "YOU", or their turret.

use glam::{Vec2, Vec3};

use crate::combat::{FEED_TIME, Match, Phase, XP_POP_TIME};
use crate::economy::{Outcome, Payout};
use crate::render::{Camera, UiVertex};
use crate::repair::State;
use crate::scene::project;
use crate::sim::Tank;
use crate::ui::widgets::{self, label};
use crate::ui::{Align, Painter, Rgba, TextStyle, theme, with_alpha};

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

/// Eased values so readouts glide instead of snapping.
pub struct Hud {
    speed: f32,
    heading: f32,
    initialized: bool,
}

impl Hud {
    pub fn new() -> Hud {
        Hud {
            speed: 0.0,
            heading: 0.0,
            initialized: false,
        }
    }

    /// `tanks` are the tanks as drawn this frame (interpolated); 0 is the player.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        m: &Match,
        tanks: &[Tank],
        seed: u64,
        camera: &Camera,
        width: f32,
        height: f32,
        dt: f32,
        scores: bool,
    ) -> Vec<UiVertex> {
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

        widgets::corners(
            &mut p,
            Vec2::splat(18.0),
            Vec2::new(w - 18.0, h - 18.0),
            22.0,
            theme::FAINT,
        );

        // Top left: round clock, kills / deaths, map seed.
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
            "XP",
            Align::Left,
        );
        p.text(
            Vec2::new(MARGIN + 150.0, MARGIN + 64.0),
            &format!("+{}", m.xp),
            &TextStyle::value(14.0),
            Align::Left,
        );
        label(
            &mut p,
            Vec2::new(MARGIN, MARGIN + 92.0),
            &format!("GRID {seed}"),
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
            p.text(Vec2::new(top.x + dir * 52.0, sy + 6.0), f.name(), &l, align);
        }
        p.line(
            Vec2::new(top.x, sy + 2.0),
            Vec2::new(top.x, sy + 14.0),
            1.0,
            theme::FAINT,
        );

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
        let lit = ((me.health / max_hp) * segs as f32).ceil() as usize;
        let hot = (me.since_hit < 0.3 && lit < segs).then_some(lit);
        label(&mut p, bc + Vec2::new(-130.0, -40.0), "HULL", Align::Left);
        p.text(
            bc + Vec2::new(130.0, -42.0),
            &format!("{:.0}", me.health.max(0.0)),
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
                p.text(
                    c + Vec2::new(0.0, 42.0),
                    &format!(
                        "BY {} {}",
                        m.factions[m.fighters[k].team].name(),
                        callsign(m, k)
                    ),
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

        // XP popups, just above the crosshair, drifting up as they fade.
        if !board {
            for (row, x) in m.xp_pops.iter().rev().take(3).enumerate() {
                let fade =
                    ((XP_POP_TIME - x.age) / 0.6).clamp(0.0, 1.0) * (x.age / 0.08).clamp(0.0, 1.0);
                let at = Vec2::new(w * 0.5, h * 0.30 - x.age * 14.0 - row as f32 * 30.0);
                let mut s = TextStyle::value(16.0);
                s.color = with_alpha(theme::HOT, fade);
                p.text(at, &format!("+{} XP", x.amount), &s, Align::Center);
                let mut l = TextStyle::LABEL;
                l.color = with_alpha(theme::FG, 0.8 * fade);
                p.text(at + Vec2::new(0.0, 24.0), x.reason, &l, Align::Center);
            }
        }

        if board {
            scoreboard(&mut p, m, Vec2::new(w * 0.5, h * 0.28));
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

        p.finish()
    }
}

/// Two standings tables side by side (your side on the left), with a winner
/// banner once the round is over.
fn scoreboard(p: &mut Painter, m: &Match, top: Vec2) {
    let col = 330.0;
    let gap = 48.0;
    let rows = (0..2)
        .map(|t| m.fighters.iter().filter(|f| f.team == t).count())
        .max()
        .unwrap_or(0) as f32;
    let width = col * 2.0 + gap;
    let mut y = top.y;
    let (x0, x1) = (top.x - width * 0.5, top.x + width * 0.5);
    let backdrop = [0.02, 0.02, 0.025, 0.7];
    let height = if m.phase == Phase::Over { 362.0 } else { 84.0 } + rows * 22.0;
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
        let line = match m.leader() {
            Some(t) => {
                s.color = m.factions[t].ui();
                format!("{} WINS", m.factions[t].name())
            }
            None => "DRAW".to_string(),
        };
        p.text(Vec2::new(top.x, y + 34.0), &line, &s, Align::Center);
        label(
            p,
            Vec2::new(top.x, y + 56.0),
            &format!("+{} XP THIS MATCH", m.xp),
            Align::Center,
        );
        y += 84.0;
    }
    let order = m.standings();
    for team in 0..2 {
        let (a, b) = if team == 0 {
            (x0, x0 + col)
        } else {
            (x1 - col, x1)
        };
        let f = m.factions[team];
        let mut name = TextStyle::value(12.0);
        name.color = f.ui();
        p.text(Vec2::new(a, y), f.name(), &name, Align::Left);
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
    y += 56.0 + rows * 22.0;
    if m.phase == Phase::Over {
        contract_pay(p, m, x0, x1, y + 14.0);
        label(
            p,
            Vec2::new(top.x, y + 158.0),
            "ENTER  AGAIN     ESC  MENU",
            Align::Center,
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Camera;
    use glam::Vec3;

    #[test]
    fn draws_at_any_size() {
        let mut m = Match::new(crate::map::Map::generate(21), 0, 11, 21);
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
            let v = hud.draw(&m, &m.world.tanks, 21, &camera, w, h, 1.0 / 60.0, true);
            assert!(!v.is_empty());
            // A little slack for halos and anti-aliasing at the screen edge.
            assert!(v.iter().all(|v| v.pos[0] > -4.0
                && v.pos[0] < w + 4.0
                && v.pos[1] > -4.0
                && v.pos[1] < h + 4.0));
        }
    }
}
