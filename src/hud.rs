//! Driving HUD in the hairline style: readouts in the four corners inside
//! faint brackets, the middle left to the city and the crosshair.
//!
//! Team deathmatch readouts: round clock and kills top left, team scores
//! under the compass, kill feed top right, hull and reload bottom centre,
//! team-coloured health bars over tanks, a death screen, and the scoreboard
//! (Tab, or when the round ends). Tanks have no names: "YOU", or their turret.

use glam::{Vec2, Vec3};

use crate::combat::{FEED_TIME, Match, Phase, Pilot, RENOWN_POP_TIME, SCRAP_POP_TIME};
use crate::economy::{self, Payout, Scrapped};
use crate::faction::Faction;
use crate::galaxy::Moves;
use crate::galaxy::theater::{SideChange, TickSummary};
use crate::hold::{self, Control};
use crate::mission::Contract;
use crate::profile::{Profile, Rank};
use crate::raid;
use crate::render::{Camera, UiVertex};
use crate::scene::project;
use crate::sim::Tank;
use crate::ui::widgets::{self, label};
use crate::ui::{Align, Hits, Layer, Painter, Rgba, TextStyle, measure, theme, with_alpha};
use crate::ui::{design, emblem};

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
    /// Esc or Enter: back to the contract screen.
    Missions,
    /// The tick summary's CONTINUE (#475): on to the contract screen.
    Continue,
}

/// The pilot's books as the match started, for the round-over screen to count
/// up from: renown with the Company before this match's was banked, and
/// crystals before the contract paid.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ledger {
    pub renown: u32,
    pub crystals: u32,
}

impl Ledger {
    /// Backs this match's earnings out of `profile`: `banked` renown is
    /// already in it, and the contract pay too once `paid`.
    pub fn before(profile: &Profile, m: &Match, banked: u32, paid: bool) -> Ledger {
        let pay = if paid { Payout::of(m).total() } else { 0 };
        Ledger {
            renown: profile.renown.saturating_sub(banked),
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
    /// What your battle did to the war, once it's settled: the debrief
    /// says it (none outside the war).
    pub debrief: Option<WarResult>,
}

impl Hud {
    pub fn new() -> Hud {
        Hud {
            speed: 0.0,
            heading: 0.0,
            initialized: false,
            hover: None,
            debrief: None,
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
        p.set_over_scene(true);
        let mut hits = Hits::new();

        widgets::corners(
            &mut p,
            Vec2::splat(18.0),
            Vec2::new(w - 18.0, h - 18.0),
            22.0,
            theme::FAINT,
        );

        // Top left: round clock, kills / deaths, renown.
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
            "RENOWN",
            Align::Left,
        );
        p.text(
            Vec2::new(MARGIN + 150.0, MARGIN + 64.0),
            &format!("+{}", m.renown),
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
                let score = m.score(team).to_string();
                p.text(Vec2::new(top.x + dir * SCORE_IN, sy), &score, &s, align);
                let mut l = TextStyle::LABEL;
                l.color = with_alpha(f.ui(), 0.8);
                emblem::faction_name(
                    &mut p,
                    Vec2::new(top.x + dir * name_in(&score), sy + 6.0),
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

        if let Some(r) = &m.raid
            && !board
            && !r.done()
        {
            raid_meter(&mut p, m, r, Vec2::new(top.x, sy + 32.0));
            let mut tag = |at: Vec3, text: &str, color: Rgba| {
                if let Some((x, y)) = project(vp, at, width, height) {
                    let at = Vec2::new(x, y) / scale;
                    if at.x >= 0.0 && at.x <= w && at.y >= 0.0 && at.y <= h {
                        let mut s = TextStyle::LABEL;
                        s.color = color;
                        p.text(at, text, &s, Align::Center);
                    }
                }
            };
            let dist = |at: Vec3| Vec2::new(tank.pos.x, tank.pos.z).distance(Vec2::new(at.x, at.z));
            // The flag, wherever it is, through walls: unless you have it.
            let flag = r.flag_at(tanks);
            if r.carrier() != Some(0) {
                let color = m.factions[r.defenders()].ui();
                tag(
                    flag + Vec3::Y * 7.0,
                    &format!("FLAG  {:.0} M", dist(flag)),
                    color,
                );
            }
            // Where it's being carried to, once it's on its way.
            if r.carrier().is_some() {
                let home = r.capture_point();
                tag(
                    home + Vec3::Y * 9.0,
                    &format!("CAPTURE  {:.0} M", dist(home)),
                    m.factions[r.attackers()].ui(),
                );
            }
        }

        // Crosshair where the shot would land, down the gun as the aim assist
        // lays it (so it moves onto a tank the assist bends onto); the gap
        // kicks open when firing.
        let aim_point = m.aim_landing(tank);
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
            // Your side's allocation spent, or your company's: there's no
            // chassis to come back in, and the match plays out without you.
            let out = if m.crew_out() {
                Some("COMPANY OUT OF HULLS")
            } else if m.hulls_left(me.team) == Some(0) {
                Some("SIDE OUT OF HULLS")
            } else {
                None
            };
            if let Some(head) = out {
                let mut s = TextStyle::value(16.0);
                s.color = theme::HOT;
                p.text(c + Vec2::new(0.0, 84.0), head, &s, Align::Center);
                if m.sitting_out() {
                    // It plays out fast: how far it's got, so it doesn't
                    // look stuck (#427).
                    p.text(
                        c + Vec2::new(0.0, 116.0),
                        "PLAYING OUT",
                        &TextStyle::LABEL,
                        Align::Center,
                    );
                    let o = c + Vec2::new(-80.0, 138.0);
                    widgets::hairbar(&mut p, o, 160.0, m.progress(), theme::HOT);
                }
            } else {
                // The countdown: whole seconds left, big, over a bar that fills
                // as the new chassis comes up.
                let left = me.respawn_in.max(0.0);
                p.text(
                    c + Vec2::new(0.0, 74.0),
                    "RESPAWN IN",
                    &TextStyle::LABEL,
                    Align::Center,
                );
                p.text(
                    c + Vec2::new(0.0, 104.0),
                    &(left.ceil().max(1.0) as u32).to_string(),
                    &TextStyle::value(30.0),
                    Align::Center,
                );
                let done = 1.0 - (left / crate::combat::RESPAWN_TIME).clamp(0.0, 1.0);
                let (bw, o) = (160.0, c + Vec2::new(-80.0, 146.0));
                p.line(o, o + Vec2::new(bw, 0.0), 1.0, theme::DIM);
                p.line(o, o + Vec2::new(bw * done, 0.0), 2.0, theme::HOT);
            }
        }

        // Renown popups, just above the crosshair, drifting up as they fade.
        if !board {
            for (row, x) in m.renown_pops.iter().rev().take(3).enumerate() {
                let fade = ((RENOWN_POP_TIME - x.age) / 0.6).clamp(0.0, 1.0)
                    * (x.age / 0.08).clamp(0.0, 1.0);
                let at = Vec2::new(w * 0.5, h * 0.30 - x.age * 14.0 - row as f32 * 30.0);
                let mut s = TextStyle::value(16.0);
                s.color = with_alpha(theme::HOT, fade);
                p.text(at, &format!("+{} RENOWN", x.amount), &s, Align::Center);
                let mut l = TextStyle::LABEL;
                l.color = with_alpha(theme::FG, 0.8 * fade);
                p.text(at + Vec2::new(0.0, 24.0), x.reason, &l, Align::Center);
            }
        }

        // Scrap popups (#444), off to the side (#474): under your renown in
        // the top-left stats, what the newest kill's wreck taught of each
        // part, with a bar for how far into its next step you are. They
        // slide in, sit, and fade, leaving the middle of the screen clear.
        if !board {
            let n = m.scrap_pops.len();
            let top = MARGIN + 100.0;
            let shown = &m.scrap_pops[n.saturating_sub(2)..];
            // The header fades with the newest kill's popups.
            if let Some(x) = shown.last() {
                let fade = ((SCRAP_POP_TIME - x.age) / 0.6).clamp(0.0, 1.0)
                    * (x.age / 0.12).clamp(0.0, 1.0);
                let mut l = TextStyle::LABEL;
                l.color = with_alpha(theme::FG, 0.8 * fade);
                p.text(Vec2::new(MARGIN, top), "SCRAP", &l, Align::Left);
            }
            for (row, x) in shown.iter().enumerate() {
                let fade = ((SCRAP_POP_TIME - x.age) / 0.6).clamp(0.0, 1.0)
                    * (x.age / 0.12).clamp(0.0, 1.0);
                let slide = (1.0 - ease_out(x.age / 0.25)) * -16.0;
                let at = Vec2::new(MARGIN + slide, top + 22.0 + row as f32 * SCRAP_POP_ROW);
                scrap_pop(&mut p, &x.scrapped, at, fade);
            }
        }

        // Round over: the debrief, unless Tab's held for the tables.
        let debrief = m.phase == Phase::Over && !scores;
        if board && !debrief {
            // Four sides stack two rows of tables: start higher.
            let top = if m.factions.len() > 2 { 0.17 } else { 0.28 };
            // The tables sit on opaque panels.
            p.set_over_scene(false);
            scoreboard(&mut p, m, Vec2::new(w * 0.5, h * top), h);
            p.set_over_scene(true);
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

        if debrief {
            // Over the battlefield alone, the HUD gone.
            p = Painter::new(scale);
            let view = DebriefView {
                m,
                ledger,
                war: self.debrief.as_ref(),
            };
            draw_debrief(&mut p, &mut hits, &view, h);
        }

        Layer {
            verts: p.finish(),
            hits,
        }
    }
}

/// How far a side's score sits from the top centre, toward its side.
const SCORE_IN: f32 = 14.0;

/// How far a side's faction name sits from the top centre, next to its
/// `score`: where it's always been for up to two digits, and pushed out by
/// any wider score so the two never touch.
fn name_in(score: &str) -> f32 {
    let style = TextStyle::value(16.0);
    let two = measure("00", &style);
    52.0 + (measure(score, &style) - two).max(0.0)
}

/// Each side's standings table, two to a row (your side first, top left).
fn scoreboard(p: &mut Painter, m: &Match, top: Vec2, screen_h: f32) {
    let holding = m.contract == Contract::Hold;
    let cols = StatColumns::new(holding);
    // Wide enough for the longest tank name in the match beside the stat columns.
    let pilots = m.pilots();
    let widest = pilots
        .iter()
        .map(|pi| row_name_width(pi))
        .fold(0.0, f32::max);
    let col = (widest + NAME_GAP + cols.width).max(380.0).ceil();
    let gap = 48.0;
    let sides = m.factions.len();
    let rows = (0..sides)
        .map(|t| pilots.iter().filter(|pi| pi.team == t).count())
        .max()
        .unwrap_or(0) as f32;
    // Under each table, its hulls left (#423): a line for the realm and
    // one per merc company, under an allocation.
    let summaries: Vec<_> = (0..sides).map(|t| m.hull_summary(t)).collect();
    let lines = summaries
        .iter()
        .flatten()
        .map(|(_, c)| 1 + c.len())
        .max()
        .map_or(0.0, |n| SUMMARY_TOP + n as f32 * SUMMARY_ROW);
    // Tables stack two to a row.
    let block = 56.0 + rows * 22.0 + lines;
    let block_gap = 24.0;
    let blocks = sides.div_ceil(2) as f32;
    let body = blocks * block + (blocks - 1.0) * block_gap;
    let width = col * 2.0 + gap;
    let y = top.y;
    let (x0, x1) = (top.x - width * 0.5, top.x + width * 0.5);
    let height = 28.0 + body;
    // Too tall for where it was asked to start: up, as far as needed to fit.
    let y = y.min(screen_h - 24.0 - height + 24.0).max(48.0);
    p.rect(
        Vec2::new(x0 - 24.0, y - 24.0),
        Vec2::new(width + 48.0, height),
        theme::PANEL,
    );
    widgets::corners(
        p,
        Vec2::new(x0 - 24.0, y - 24.0),
        Vec2::new(x1 + 24.0, y - 24.0 + height),
        14.0,
        theme::DIM,
    );
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
        for pi in pilots.iter().filter(|pi| pi.team == team) {
            let s = row_style(pi);
            let mut x = a;
            if pi.company {
                // A merc: the Company's mark, in its colors (#389).
                let mut c = s;
                c.color = if pi.active {
                    Faction::Company.ui()
                } else {
                    s.color
                };
                x += emblem::mark(p, Vec2::new(x, ty), Faction::Company, &c);
            }
            p.text(Vec2::new(x, ty), &kit_name(pi), &s, Align::Left);
            if let Some(dx) = cols.held {
                p.text(
                    Vec2::new(b - dx, ty),
                    &format!("{:.0}", pi.held.floor()),
                    &s,
                    Align::Right,
                );
            }
            p.text(
                Vec2::new(b - cols.kills, ty),
                &pi.kills.to_string(),
                &s,
                Align::Right,
            );
            p.text(Vec2::new(b, ty), &pi.deaths.to_string(), &s, Align::Right);
            ty += 22.0;
        }
        if let Some(summary) = &summaries[team] {
            let ty = y + 56.0 + rows * 22.0;
            hull_summary(p, m, team, summary, a, b, ty);
        }
    }
}

/// The hull summary under a table: the space above it, and each line.
const SUMMARY_TOP: f32 = 32.0;
const SUMMARY_ROW: f32 = 22.0;

/// Side `team`'s hulls left (#423), between `a` and `b` from `y` down:
/// under a rule and its header, the realm's, then each merc company's on
/// its side. Hot when the last few are going; dim once there are none.
fn hull_summary(
    p: &mut Painter,
    m: &Match,
    team: usize,
    (realm, companies): &(u32, Vec<(String, u32)>),
    a: f32,
    b: f32,
    y: f32,
) {
    let f = m.factions[team];
    p.line(
        Vec2::new(a, y + 4.0),
        Vec2::new(b, y + 4.0),
        1.0,
        with_alpha(f.ui(), 0.25),
    );
    label(p, Vec2::new(a, y + 12.0), "HULLS LEFT", Align::Left);
    let seats = m.seats(team) as u32;
    let lines = std::iter::once((f, f.name().to_string(), *realm)).chain(
        companies
            .iter()
            .map(|(n, h)| (Faction::Company, n.clone(), *h)),
    );
    let mut ly = y + SUMMARY_TOP;
    for (who, name, n) in lines {
        let mut s = TextStyle::value(ROW_SIZE);
        s.color = if n == 0 { theme::DIM } else { who.ui() };
        emblem::tagged(p, Vec2::new(a, ly), who, &name, &s, Align::Left);
        let mut v = TextStyle::value(ROW_SIZE);
        v.color = if n == 0 {
            theme::DIM
        } else if n <= seats {
            theme::HOT
        } else {
            theme::FG
        };
        p.text(Vec2::new(b, ly), &n.to_string(), &v, Align::Right);
        ly += SUMMARY_ROW;
    }
}

/// What your battle did to the war (its site), for the debrief.
#[derive(Clone, Debug, PartialEq)]
pub struct WarResult {
    /// The world it was fought on: "DORONE I".
    pub world: String,
    /// What was fought over: "SHIELD GENERATOR".
    pub site: &'static str,
    pub verdict: Verdict,
}

/// What became of the site your battle was for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// You attacked it and took it.
    Taken,
    /// You attacked it and its holder kept it.
    Holds,
    /// You held it.
    Held,
    /// You held it, and lost it.
    Lost,
}

impl Verdict {
    /// From where you stood (`defending` it or not) and whether it changed hands.
    pub fn of(defending: bool, taken: bool) -> Verdict {
        match (defending, taken) {
            (false, true) => Verdict::Taken,
            (false, false) => Verdict::Holds,
            (true, false) => Verdict::Held,
            (true, true) => Verdict::Lost,
        }
    }

    pub fn good(self) -> bool {
        matches!(self, Verdict::Taken | Verdict::Held)
    }

    pub fn word(self) -> &'static str {
        match self {
            Verdict::Taken => "TAKEN",
            Verdict::Holds => "HOLDS",
            Verdict::Held => "HELD",
            Verdict::Lost => "LOST",
        }
    }
}

/// What the debrief draws from.
struct DebriefView<'a> {
    m: &'a Match,
    ledger: Ledger,
    war: Option<&'a WarResult>,
}

/// The round's debrief, a panel beside the battlefield: how it went
/// (victory, defeat, draw), what it did to the site it was for, the score,
/// then what you earned. Tab shows the tables instead.
fn draw_debrief(p: &mut Painter, hits: &mut Hits<Hit>, v: &DebriefView, h: f32) {
    use design::{AMBER, BLUE, BODY, COLUMN, HEAD, MARGIN, SMALL, TEXT, TEXT_2, TITLE, UNIT};
    let m = v.m;
    let t = m.over_for - RESULTS_DELAY;
    design::panel(p, h);
    let (x, right) = (MARGIN, MARGIN + COLUMN);
    let mut y = MARGIN;
    let (title, color) = match m.leader() {
        Some(t) if t == m.player().team => ("VICTORY", AMBER),
        Some(_) => ("DEFEAT", theme::HOT),
        None => ("DRAW", TEXT),
    };
    p.text(Vec2::new(x, y), title, &TITLE.colored(color), Align::Left);
    y += UNIT * 6.0;
    if let Some(r) = v.war {
        let style = SMALL.colored(TEXT_2);
        p.text(Vec2::new(x, y), &r.world, &style, Align::Left);
        y += UNIT * 2.5;
        let text = format!(
            "{} {}",
            r.site
                .trim_end_matches(" GENERATOR")
                .trim_start_matches("HULL ")
                .trim_start_matches("CRYSTAL "),
            r.verdict.word()
        );
        let color = if r.verdict.good() { TEXT } else { TEXT_2 };
        p.text(Vec2::new(x, y), &text, &BODY.colored(color), Align::Left);
        y += UNIT * 5.0;
    }
    if m.factions.len() == 2 {
        for (team, at, align) in [(0, x, Align::Left), (1, right, Align::Right)] {
            let f = m.factions[team];
            let score = m.score(team).to_string();
            let style = HEAD.colored(f.ui());
            emblem::tagged(p, Vec2::new(at, y), f, &score, &style, align);
        }
        y += UNIT * 3.5;
        let (a, b) = (m.score(0) as f32, m.score(1) as f32);
        let share = if a + b > 0.0 { a / (a + b) } else { 0.5 };
        let wide = COLUMN * share;
        p.rect(Vec2::new(x, y), Vec2::new(wide, 4.0), m.factions[0].ui());
        let rest = Vec2::new(COLUMN - wide, 4.0);
        p.rect(Vec2::new(x + wide, y), rest, m.factions[1].ui());
        y += UNIT * 7.0;
    }

    // Your rank, filling from where you stood.
    let shown = renown_shown(m.renown, t);
    let start = Rank::of(v.ledger.renown);
    let now = Rank::of(v.ledger.renown + shown);
    let up = now.level > start.level;
    let rank = format!("{} {}", now.title(), now.level);
    let style = BODY.colored(if up { AMBER } else { TEXT });
    p.text(Vec2::new(x, y), &rank, &style, Align::Left);
    let style = BODY.colored(AMBER);
    p.text(
        Vec2::new(right, y),
        &format!("+{shown}"),
        &style,
        Align::Right,
    );
    y += UNIT * 3.0;
    gain_bar(
        p,
        Vec2::new(x, y),
        if up { 0.0 } else { start.progress() },
        now.progress(),
    );
    y += UNIT * 4.0;

    // Your crystals, the pay folding in.
    let cr = tally(v.ledger.crystals, Payout::of(m).total(), t);
    let style = BODY.colored(BLUE);
    p.text(
        Vec2::new(x, y),
        &format!("{} cr", cr.balance),
        &style,
        Align::Left,
    );
    if let Some((k, alpha)) = cr.plus {
        let style = BODY.colored(with_alpha(BLUE, alpha));
        p.text(Vec2::new(right, y), &format!("+{k}"), &style, Align::Right);
    }
    y += UNIT * 6.0;

    // The parts you've got further with.
    for line in scrap_lines(m) {
        match line {
            ScrapLine::Part(sc) => {
                scrap_row(p, &sc, t, Vec2::new(x, y));
                y += UNIT * 4.0;
            }
            ScrapLine::More(n) => {
                let style = SMALL.colored(TEXT_2);
                p.text(Vec2::new(x, y), &format!("+{n}"), &style, Align::Left);
            }
        }
    }

    let (min, size) = design::action(p, design::on_at(h), "Continue", true);
    hits.add(p, min, size, Hit::Missions);
    let at = Vec2::new(x, h - MARGIN - SMALL.size);
    p.text(at, "Tab  Scoreboard", &SMALL.colored(TEXT_2), Align::Left);
}

/// A column-wide bar at `at`: what you had, dim, and the gain, lit.
fn gain_bar(p: &mut Painter, at: Vec2, from: f32, to: f32) {
    let w = design::COLUMN;
    let h = 2.0;
    let rail = with_alpha(design::TEXT_2, 0.4);
    p.rect(at, Vec2::new(w, h), rail);
    p.rect(at, Vec2::new(w * from, h), design::TEXT_2);
    if to > from {
        let size = Vec2::new(w * (to - from), h);
        p.rect(at + Vec2::X * w * from, size, design::AMBER);
    }
}

/// Scoreboard row text size.
const ROW_SIZE: f32 = 11.0;
/// Space between a tank's name and the first stat column, and between stat columns.
const NAME_GAP: f32 = 20.0;
const STAT_GAP: f32 = 22.0;

/// A pilot's scoreboard name: their hull and turret, yours marked.
fn kit_name(pi: &Pilot) -> String {
    format!(
        "{}{} · {}",
        if pi.seat == 0 { "YOU  " } else { "" },
        crate::garage::hull_name(pi.hull),
        crate::garage::turret_name(pi.turret)
    )
}

/// A scoreboard row's text: yours hot, and greyed out once the pilot's
/// out of the fight (#389).
fn row_style(pi: &Pilot) -> TextStyle {
    let mut s = TextStyle::value(ROW_SIZE);
    if !pi.active {
        s.color = OUT_ROW;
    } else if pi.seat == 0 {
        s.color = theme::HOT;
    }
    s
}

/// A pilot out of the fight, on the scoreboard: there, but faint.
const OUT_ROW: Rgba = [0.94, 0.91, 0.86, 0.32];

/// How wide a pilot's name is on its row, the Company's mark among it.
fn row_name_width(pi: &Pilot) -> f32 {
    let s = TextStyle::value(ROW_SIZE);
    let mark = if pi.company {
        emblem::mark_width(&s)
    } else {
        0.0
    };
    mark + measure(&kit_name(pi), &s)
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

/// The Raid under the scores: whether your side attacks or defends, then
/// where the flag is.
fn raid_meter(p: &mut Painter, m: &Match, r: &raid::Raid, at: Vec2) {
    let mine = m.player().team;
    let attacking = r.attackers() == mine;
    let mut l = TextStyle::LABEL;
    l.color = m.factions[mine].ui();
    p.text(
        at,
        if attacking { "ATTACK" } else { "DEFEND" },
        &l,
        Align::Center,
    );
    let (text, color) = match r.flag {
        raid::Flag::Home if attacking => ("TAKE THEIR FLAG".to_string(), theme::DIM),
        raid::Flag::Home => ("YOUR FLAG IS HOME".to_string(), theme::DIM),
        raid::Flag::Carried(0) => ("YOU HAVE THE FLAG: TAKE IT HOME".to_string(), theme::HOT),
        raid::Flag::Carried(i) => (
            format!("{} HAS THE FLAG", m.factions[m.fighters[i].team].name()),
            m.factions[m.fighters[i].team].ui(),
        ),
        raid::Flag::Dropped { for_, .. } => (
            format!(
                "FLAG DROPPED  ·  HOME IN {:.0}",
                (raid::RETURN_TIME - for_).ceil()
            ),
            theme::HOT,
        ),
    };
    let mut s = TextStyle::LABEL;
    s.color = color;
    p.text(at + Vec2::new(0.0, 18.0), &text, &s, Align::Center);
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
/// How many parts get a line in the round-over scrap list (#444, #480)
/// before the rest fold into one "+N MORE PARTS".
const SCRAP_LINES: usize = 6;
/// One line of the round-over scrap summary.
enum ScrapLine {
    Part(Scrapped),
    More(usize),
}

/// The round-over scrap summary (#444): what your wrecks taught, part by
/// part. Empty mid-round, outside the war, or if they taught nothing.
fn scrap_lines(m: &Match) -> Vec<ScrapLine> {
    let Some(before) = m.known.filter(|_| m.phase == Phase::Over) else {
        return Vec::new();
    };
    let all = economy::scrapped(before, &m.player().scrap);
    let shown = if all.len() > SCRAP_LINES {
        SCRAP_LINES - 1
    } else {
        all.len()
    };
    let mut out: Vec<ScrapLine> = all[..shown].iter().copied().map(ScrapLine::Part).collect();
    if all.len() > shown {
        out.push(ScrapLine::More(all.len() - shown));
    }
    out
}

/// Where a part stands after scrap: how far to its next step, or the step
/// it just reached.
fn scrap_progress(sc: &Scrapped) -> String {
    let whole = sc.known.floor() as u8;
    if sc.stepped() {
        if whole == 1 {
            "SCHEMATIC REVERSE-ENGINEERED".to_string()
        } else {
            format!("{} REVERSE-ENGINEERED", economy::level_label(whole))
        }
    } else {
        match sc.toward() {
            Some((pct, next)) => format!("{pct}% TO {}", economy::level_label(next)),
            None => "FULLY KNOWN".to_string(),
        }
    }
}

/// Each in-battle scrap popup's height, and its width (the speed bar's).
const SCRAP_POP_ROW: f32 = 50.0;
const SCRAP_POP_W: f32 = 200.0;

/// One part's scrap popup at `at` (its top left): the part and the gain
/// over a bar for how far into its next step it is, then where it stands.
fn scrap_pop(p: &mut Painter, sc: &Scrapped, at: Vec2, fade: f32) {
    let mut name = TextStyle::value(11.0);
    name.color = with_alpha(theme::FG, fade);
    emblem::tagged(p, at, sc.part.maker(), sc.part.name(), &name, Align::Left);
    let mut gain = TextStyle::value(11.0);
    gain.color = with_alpha(theme::HOT, fade);
    p.text(
        at + Vec2::new(SCRAP_POP_W, 0.0),
        &format!("+{}%", sc.percent()),
        &gain,
        Align::Right,
    );
    let (from, to) = scrap_bar(sc);
    let bar = at + Vec2::new(0.0, 18.0);
    p.line(
        bar,
        bar + Vec2::new(SCRAP_POP_W, 0.0),
        1.0,
        with_alpha(theme::DIM, fade),
    );
    if from > 0.0 {
        p.line(
            bar,
            bar + Vec2::new(SCRAP_POP_W * from, 0.0),
            2.0,
            with_alpha(theme::FG, fade),
        );
    }
    p.line(
        bar + Vec2::new(SCRAP_POP_W * from, 0.0),
        bar + Vec2::new(SCRAP_POP_W * to, 0.0),
        2.0,
        with_alpha(theme::HOT, fade),
    );
    let mut l = TextStyle::LABEL;
    l.color = with_alpha(if sc.stepped() { theme::HOT } else { theme::DIM }, fade);
    p.text(
        at + Vec2::new(SCRAP_POP_W, 24.0),
        &scrap_progress(sc),
        &l,
        Align::Right,
    );
}

/// A part's bar after scrap, as fractions of its current step: where it
/// stood before (0 if this gain reached a new step), and where it stands.
fn scrap_bar(sc: &Scrapped) -> (f32, f32) {
    if sc.stepped() {
        return (0.0, 1.0);
    }
    match sc.toward() {
        Some(_) => {
            let into = sc.known.fract();
            ((into - sc.gained).max(0.0), into)
        }
        None => (1.0, 1.0),
    }
}

/// Renown: the bar fills from where you stood over this long.
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

/// How much of `gained` the renown graphic shows `t` seconds in.
fn renown_shown(gained: u32, t: f32) -> u32 {
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

/// One part's line in the debrief: the part, the step it's at or heading
/// for, and a bar filling from where it stood, on renown's timer.
fn scrap_row(p: &mut Painter, sc: &Scrapped, t: f32, at: Vec2) {
    use design::{AMBER, SMALL, TEXT_2, UNIT};
    let k = ease_out(t / REP_FILL);
    let maker = sc.part.maker();
    let style = SMALL.colored(maker.ui());
    emblem::tagged(p, at, maker, sc.part.name(), &style, Align::Left);
    let step = match sc.toward().filter(|_| !sc.stepped()) {
        Some((_, next)) => next,
        None => sc.known.floor() as u8,
    };
    let reached = sc.stepped() && k >= 1.0;
    let style = SMALL.colored(if reached { AMBER } else { TEXT_2 });
    let right = at + Vec2::X * design::COLUMN;
    let label = economy::level_label(step);
    p.text(right, &label, &style, Align::Right);
    let (from, to) = scrap_bar(sc);
    gain_bar(p, at + Vec2::Y * UNIT * 2.0, from, from + (to - from) * k);
}

/// The debug overlay's frame rate, small in the top right corner above the HUD brackets.
/// Over a battle you've left (#427), dimmed under it: the match playing
/// out without you, fast, with a bar for how far through it is.
pub fn playing_out(m: &Match, width: f32, height: f32) -> Vec<UiVertex> {
    let scale = height / REF_HEIGHT;
    let (w, h) = (width / scale, REF_HEIGHT);
    let mut p = Painter::new(scale);
    p.rect(Vec2::ZERO, Vec2::new(w, h), [0.0, 0.0, 0.0, 0.55]);
    let size = Vec2::new(480.0, 112.0);
    let min = Vec2::new(w, h) * 0.5 - size * 0.5;
    p.rect(min, size, theme::PANEL);
    widgets::corners(&mut p, min, min + size, 14.0, theme::DIM);
    let x = min.x + 32.0;
    label(
        &mut p,
        Vec2::new(x, min.y + 30.0),
        "LEFT THE BATTLE",
        Align::Left,
    );
    p.text(
        Vec2::new(x, min.y + 48.0),
        "PLAYING OUT",
        &TextStyle::value(20.0),
        Align::Left,
    );
    widgets::hairbar(
        &mut p,
        Vec2::new(x, min.y + 86.0),
        size.x - 64.0,
        m.progress(),
        theme::HOT,
    );
    p.finish()
}

/// What moved a side's hulls in the system (#475), signed: what came in,
/// then what went. None of what didn't happen.
pub fn causes(m: &Moves) -> Vec<(i64, &'static str)> {
    let ins = [
        (m.arrived, "arrived"),
        (m.hired, "hired"),
        (m.built, "built"),
    ];
    let outs = [
        (m.lost, "lost"),
        (m.left, "sent away"),
        (m.released, "released"),
    ];
    let ins = ins.into_iter().map(|(n, w)| (n as i64, w));
    let outs = outs.into_iter().map(|(n, w)| (-(n as i64), w));
    ins.chain(outs).filter(|(n, _)| *n != 0).collect()
}

/// A change in hulls, signed: "+50", "-3", "0".
fn signed(n: i64) -> String {
    if n > 0 {
        format!("+{n}")
    } else {
        n.to_string()
    }
}

/// What the tick summary screen's showing besides the summary (#475).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SummaryState {
    /// The contract screen's still loading behind it.
    pub loading: bool,
    /// CONTINUE's been pressed: it goes as soon as it's loaded.
    pub going: bool,
    /// Seconds it's been up (turns the spinner).
    pub time: f32,
}

/// The tick summary (#475), its own screen between your battle's debrief
/// and the contract screen: over the system it was fought in (drawn by
/// `theater_view`), the sites that changed hands lit on it, and a panel with
/// each side's hulls there, what they lost or gained, and why. The contract
/// screen loads behind it; CONTINUE goes on to it.
pub fn tick_summary(t: &TickSummary, state: SummaryState, width: f32, height: f32) -> Layer<Hit> {
    use design::{AMBER, BODY, COLUMN, HEAD, MARGIN, SMALL, TEXT, TEXT_2, UNIT};
    let scale = height / REF_HEIGHT;
    let (w, h) = (width / scale, REF_HEIGHT);
    let mut p = Painter::new(scale);
    let mut hits = Hits::new();
    crate::theater_view::aftermath(&mut p, t, state.time, w, h);
    design::panel(&mut p, h);
    let (x, right) = (MARGIN, MARGIN + COLUMN);
    let mut y = MARGIN;
    p.text(Vec2::new(x, y), &t.name, &HEAD, Align::Left);
    y += UNIT * 9.0;
    for c in &t.sides {
        let (side, name) = who(c);
        let color = if c.merc { AMBER } else { side.ui() };
        let at = Vec2::new(x, y);
        emblem::tagged(&mut p, at, side, name, &BODY.colored(color), Align::Left);
        p.text(Vec2::new(right, y), &c.now.to_string(), &BODY, Align::Right);
        y += UNIT * 2.5;
        // What it has of the most it had: kept, then lost hot or gained lit.
        let most = c.before.max(c.now).max(1) as f32;
        let at = |n: u32| COLUMN * n as f32 / most;
        let keep = c.before.min(c.now);
        let h = 4.0;
        p.rect(Vec2::new(x, y), Vec2::new(at(keep), h), color);
        let diff = Vec2::new(at(c.before.max(c.now)) - at(keep), h);
        let lost = c.now < c.before;
        let lit = if lost {
            with_alpha(theme::HOT, 0.35)
        } else {
            TEXT
        };
        p.rect(Vec2::new(x + at(keep), y), diff, lit);
        y += UNIT * 2.0;
        // Why, a line each.
        for (n, why) in causes(&c.moves) {
            let color = if n < 0 { theme::HOT } else { TEXT };
            let at = Vec2::new(x, y);
            p.text(at, why, &SMALL.colored(TEXT_2), Align::Left);
            let style = SMALL.colored(color);
            p.text(Vec2::new(right, y), &signed(n), &style, Align::Right);
            y += UNIT * 2.0;
        }
        y += UNIT * 3.0;
    }

    let wait = state.going && state.loading;
    let go = if wait { "Loading" } else { "Continue" };
    let at = design::on_at(h);
    let (min, size) = design::action(&mut p, at, go, true);
    hits.add(&p, min, size, Hit::Continue);
    if wait {
        spinner_small(&mut p, at + Vec2::new(COLUMN, 8.0), state.time);
    }
    Layer {
        verts: p.finish(),
        hits,
    }
}

/// Whose line it is on the tick summary: the side's, or the company's
/// under it, working for it.
fn who(c: &SideChange) -> (Faction, &'static str) {
    if c.merc {
        (Faction::Company, "MERCS")
    } else {
        let f = Faction::ALL[c.side];
        (f, f.name())
    }
}

/// `spinner`, small enough to sit beside a label.
pub fn spinner_small(p: &mut Painter, c: Vec2, time: f32) {
    let r = 7.0;
    let tau = std::f32::consts::TAU;
    p.arc(c, r, 0.0, tau, 1.5, theme::DIM);
    let a = (time / SPIN_PERIOD).fract() * tau;
    p.arc(c, r, a, a + tau * 0.3, 2.0, theme::HOT);
}

/// Seconds the loading spinner takes to go round once.
const SPIN_PERIOD: f32 = 1.2;

/// A loading panel over the screen, `time` seconds after it went up:
/// what's loading, small over it what for, and a spinner turning, so the
/// screen never sits frozen while a map's built.
pub fn loading(what: &str, title: &str, time: f32, width: f32, height: f32) -> Vec<UiVertex> {
    let scale = height / REF_HEIGHT;
    let (w, h) = (width / scale, REF_HEIGHT);
    let mut p = Painter::new(scale);
    p.rect(Vec2::ZERO, Vec2::new(w, h), [0.0, 0.0, 0.0, 0.55]);
    let size = Vec2::new(480.0, 100.0);
    let min = Vec2::new(w, h) * 0.5 - size * 0.5;
    p.rect(min, size, theme::PANEL);
    widgets::corners(&mut p, min, min + size, 14.0, theme::DIM);
    let x = min.x + 32.0;
    label(&mut p, Vec2::new(x, min.y + 30.0), what, Align::Left);
    p.text(
        Vec2::new(x, min.y + 48.0),
        title,
        &TextStyle::value(20.0),
        Align::Left,
    );
    spinner(
        &mut p,
        Vec2::new(min.x + size.x - 56.0, min.y + size.y * 0.5),
        time,
    );
    p.finish()
}

/// A loading spinner at `c`, `time` seconds in: a faint ring with a bright
/// arc going round it.
fn spinner(p: &mut Painter, c: Vec2, time: f32) {
    let r = 22.0;
    let tau = std::f32::consts::TAU;
    p.arc(c, r, 0.0, tau, 2.0, theme::DIM);
    let a = (time / SPIN_PERIOD).fract() * tau;
    p.arc(c, r, a, a + tau * 0.3, 3.0, theme::HOT);
}

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

    /// The loading panel on the way back to the contract screen (#452)
    /// animates: its spinner's somewhere else a moment later.
    #[test]
    fn the_loading_panel_spins() {
        let a = loading("ROUND OVER", "LOADING", 0.0, 1600.0, 900.0);
        let b = loading("ROUND OVER", "LOADING", SPIN_PERIOD * 0.25, 1600.0, 900.0);
        assert!(!a.is_empty());
        let pos = |v: &Vec<UiVertex>| v.iter().map(|v| format!("{:?}", v)).collect::<Vec<_>>();
        assert_ne!(pos(&a), pos(&b), "the spinner didn't move");
        // A full turn later it's back where it was.
        let c = loading("ROUND OVER", "LOADING", SPIN_PERIOD, 1600.0, 900.0);
        assert_eq!(a.len(), c.len());
    }

    #[test]
    fn a_score_of_any_width_clears_its_faction_name() {
        let style = TextStyle::value(16.0);
        let gap = name_in("52") - SCORE_IN - measure("52", &style);
        assert!(gap > 4.0, "two digits leave {gap}");
        for score in ["0", "7", "52", "120", "999", "1000", "12345"] {
            let clear = name_in(score) - SCORE_IN - measure(score, &style);
            assert!(clear >= gap - 0.01, "{score} leaves {clear}, not {gap}");
        }
    }

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
    fn renown_fills_to_what_was_gained() {
        assert_eq!(renown_shown(37, -1.0), 0);
        assert!(renown_shown(37, REP_FILL * 0.5) < 37);
        assert_eq!(renown_shown(37, REP_FILL), 37);
    }

    #[test]
    fn ledger_backs_out_this_matchs_earnings() {
        let mut m = Match::new(crate::map::Map::saltmarket(), 0, 11, 21);
        m.start();
        m.phase = Phase::Over;
        let mut p = Profile::in_memory();
        p.renown = 140;
        p.crystals = 1000 + Payout::of(&m).total();
        let l = Ledger::before(&p, &m, 40, true);
        assert_eq!((l.renown, l.crystals), (100, 1000));
        // Pay not banked yet: the balance is still the one before.
        p.crystals = 1000;
        assert_eq!(Ledger::before(&p, &m, 40, false).crystals, 1000);
    }

    #[test]
    fn round_over_has_buttons() {
        let mut m = Match::new(crate::map::Map::saltmarket(), 0, 11, 21);
        m.start();
        let t = &m.world.tanks[0];
        let camera = Camera {
            pos: t.pos + Vec3::new(0.0, 6.0, -10.0),
            target: t.pos,
            fov_y: 1.1,
        };
        let mut hud = Hud::new();
        let draw = |hud: &mut Hud, m: &Match, scores: bool| {
            hud.draw(
                m,
                &m.world.tanks,
                &camera,
                1600.0,
                900.0,
                0.0,
                scores,
                Ledger::default(),
            )
            .hits
        };
        // Mid-round the scoreboard has nothing to click.
        assert!(draw(&mut hud, &m, true).is_empty());
        m.phase = Phase::Over;
        // Over, Tab shows the tables alone; else the debrief, with its button.
        assert!(draw(&mut hud, &m, true).is_empty());
        let hits = draw(&mut hud, &m, false);
        let at = hits.center_of(Hit::Missions).expect("button drawn");
        assert_eq!(hits.at(at).map(|(h, _)| h), Some(Hit::Missions));
    }

    /// The debrief says what became of the site, from where you stood.
    #[test]
    fn a_verdict_is_from_your_side() {
        assert_eq!(Verdict::of(false, true), Verdict::Taken);
        assert_eq!(Verdict::of(false, false), Verdict::Holds);
        assert_eq!(Verdict::of(true, false), Verdict::Held);
        assert_eq!(Verdict::of(true, true), Verdict::Lost);
        assert!(Verdict::Taken.good() && Verdict::Held.good());
        assert!(!Verdict::Holds.good() && !Verdict::Lost.good());
    }

    #[test]
    fn draws_at_any_size() {
        for contract in [Contract::Attrition, Contract::Hold, Contract::Raid] {
            draws_at_any_size_under(contract);
        }
    }

    fn draws_at_any_size_under(contract: Contract) {
        let mut m = Match::new(crate::map::Map::saltmarket(), 0, 11, 21);
        m.start();
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
