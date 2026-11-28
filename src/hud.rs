//! Driving HUD: readouts round the edges, the middle left to the city and the crosshair.
//!
//! Team deathmatch readouts: round clock top left, team scores
//! top centre, kill feed top right, hull and reload bottom centre,
//! team-coloured health bars over tanks, a death screen, and the scoreboard
//! (Tab, or when the round ends). Tanks have no names: "YOU", or their turret.

use glam::{Vec2, Vec3};

use crate::combat::{FEED_TIME, Match, Phase, Pilot, RENOWN_POP_TIME, SCRAP_POP_TIME};
use crate::economy::{self, Payout, Scrapped};
use crate::faction::Faction;
use crate::galaxy::theater::TickSummary;
use crate::hold::{self, Control};
use crate::minimap;
use crate::mission::Contract;
use crate::profile::{Profile, Rank};
use crate::raid;
use crate::render::{Camera, UiVertex};
use crate::scene::project;
use crate::sim::Tank;
use crate::ui::widgets;
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
        design::AMBER
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
    /// The map's minimap, if it was generated.
    chart: Option<minimap::Chart>,
    /// The whole map's up, over the middle of the screen.
    pub map_up: bool,
    speed: f32,
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
            chart: None,
            map_up: false,
            speed: 0.0,
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
        if !self.initialized {
            (self.speed, self.initialized) = (speed, true);
        }
        let k = 1.0 - (-dt * 10.0).exp();
        self.speed += (speed - self.speed) * k;

        let scale = height / REF_HEIGHT;
        let (w, h) = (width / scale, REF_HEIGHT);
        let mut p = Painter::new(scale);
        p.set_over_scene(true);
        let mut hits = Hits::new();

        let outline = Some((theme::HALO, theme::HALO_PX));
        p.set_halo(None);
        // Top left: round clock.
        let secs = m.time_left.ceil() as u32;
        plate(
            &mut p,
            Vec2::new(MARGIN, MARGIN),
            Vec2::new(84.0, 32.0),
            1.0,
        );
        small(&mut p, Vec2::new(MARGIN, MARGIN), "ROUND", Align::Left);
        let mut clock = hud_text(18.0);
        if m.time_left < 30.0 && m.phase == Phase::Playing {
            clock.color = theme::HOT;
        }
        p.text(
            Vec2::new(MARGIN, MARGIN + 14.0),
            &format!("{}:{:02}", secs / 60, secs % 60),
            &clock,
            Align::Left,
        );

        // Top right: the minimap, then the kill feed, newest on top, each
        // side in its colour.
        let mut tr = Vec2::new(w - MARGIN, MARGIN);
        minimap::keep(&mut self.chart, &m.world.map);
        if let Some(c) = self.chart.as_ref().filter(|_| !board) {
            // (Its edge with the other readouts' plates.)
            let min = tr + Vec2::new(10.0 - minimap::SIZE, -10.0);
            minimap::draw(&mut p, c, m, tanks, min, minimap::SIZE);
            tr.y += minimap::SIZE + 24.0;
        }
        for (row, k) in m.feed.iter().rev().take(5).enumerate() {
            let fade = ((FEED_TIME - k.age) / 0.8).clamp(0.0, 1.0);
            let y = tr.y + row as f32 * 26.0;
            let names = [callsign(m, k.killer), callsign(m, k.victim)];
            let mut wide = measure(names[1], &hud_text(9.0));
            if k.killer != k.victim {
                wide += measure(names[0], &hud_text(9.0)) + 28.0;
            }
            let size = Vec2::new(wide, 9.0);
            feed_plate(&mut p, Vec2::new(tr.x - wide, y), size, fade);
            let mut s = hud_text(9.0);
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

        p.set_halo(outline);
        // Health over the other tanks in view, in their side's colour; allies get a tick above.
        // What a storm hides has no bar (your own side's still show).
        let vp = camera.view_proj(width / height);
        let sight = m.world.map.atmosphere.sight();
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
            let mut fade = (1.0 - (d - 110.0) / 50.0).clamp(0.0, 1.0);
            if f.team != me.team {
                fade *= bar_fade(d, sight);
                if fade <= 0.0 {
                    continue;
                }
            }
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

        // Top centre: the team score (your side on the left).
        let top = Vec2::new(w * 0.5, MARGIN);
        let sy = top.y;
        p.set_halo(None);
        let sides = m.factions.len();
        if sides == 2 {
            let meter = match (&m.hold, &m.raid) {
                (Some(_), _) => 44.0,
                (_, Some(_)) => 26.0,
                _ => 0.0,
            };
            // Several sites' names take more.
            let sites = m.hold.as_ref().map_or(0, |h| h.sites.len());
            let wide = SCORES_W.max(if sites > 1 { 130.0 * sites as f32 } else { 0.0 });
            let size = Vec2::new(wide, 14.0 + meter);
            plate(&mut p, Vec2::new(top.x - size.x * 0.5, sy), size, 1.0);
        }
        if sides == 2 {
            for (team, dir, align) in [(0, -1.0, Align::Right), (1, 1.0, Align::Left)] {
                let f = m.factions[team];
                let mut s = hud_text(14.0);
                s.color = f.ui();
                let score = m.score(team).to_string();
                p.text(Vec2::new(top.x + dir * SCORE_IN, sy), &score, &s, align);
                let mut l = LABEL;
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
                let mut s = hud_text(14.0);
                s.color = f.ui();
                p.text(
                    Vec2::new(x, sy),
                    &m.score(team).to_string(),
                    &s,
                    Align::Center,
                );
                let mut l = LABEL;
                l.color = with_alpha(f.ui(), 0.8);
                emblem::faction_name(&mut p, Vec2::new(x, sy + 20.0), f, &l, Align::Center);
            }
        }
        if let Some(hd) = &m.hold
            && !board
        {
            hold_meter(&mut p, m, hd, Vec2::new(top.x, sy + 32.0));
            p.set_halo(outline);
            // Each site's tag, unless you're on it.
            for site in &hd.sites {
                let anchor = Vec3::new(site.at.x, site.ground + 9.0, site.at.y);
                if site.inside(tank.pos) {
                    continue;
                }
                let Some((x, y)) = project(vp, anchor, width, height) else {
                    continue;
                };
                let at = Vec2::new(x, y) / scale;
                if at.x >= 0.0 && at.x <= w && at.y >= 0.0 && at.y <= h {
                    let mut s = LABEL;
                    s.color = hold_color(m, site.control);
                    let d = Vec2::new(tank.pos.x, tank.pos.z).distance(site.at);
                    p.text(at, &format!("{}  {d:.0} M", site.name), &s, Align::Center);
                }
            }
        }

        if let Some(r) = &m.raid
            && !board
            && !r.done()
        {
            raid_meter(&mut p, m, r, Vec2::new(top.x, sy + 32.0));
            p.set_halo(outline);
            let mut tag = |at: Vec3, text: &str, color: Rgba| {
                if let Some((x, y)) = project(vp, at, width, height) {
                    let at = Vec2::new(x, y) / scale;
                    if at.x >= 0.0 && at.x <= w && at.y >= 0.0 && at.y <= h {
                        let mut s = LABEL;
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

        p.set_halo(outline);
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
                design::TEXT
            };
            widgets::crosshair(&mut p, Vec2::new(x, y) / scale, kick, color);
        }

        // Bottom centre: hull and reload.
        let bc = Vec2::new(w * 0.5, h - MARGIN);
        p.set_halo(None);
        plate(
            &mut p,
            bc + Vec2::new(-130.0, -42.0),
            Vec2::new(260.0, 42.0),
            1.0,
        );
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
        small(&mut p, bc + Vec2::new(-130.0, -40.0), name, Align::Left);
        p.text(
            bc + Vec2::new(130.0, -42.0),
            &value,
            &hud_text(9.0),
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
                let mut s = LABEL;
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
                if ready >= 1.0 {
                    design::TEXT
                } else {
                    theme::DIM
                },
            );
        }
        p.set_halo(outline);
        if me.since_repair < 1.8 {
            let mut s = hud_text(9.0);
            s.color = with_alpha(
                theme::MEND,
                (1.0 - (me.since_repair - 1.2) / 0.6).clamp(0.0, 1.0),
            );
            p.text(
                bc + Vec2::new(0.0, -100.0),
                &format!("+{:.0} HULL", me.repaired),
                &s,
                Align::Center,
            );
        }
        if me.shield > 0.0 && me.alive {
            small(
                &mut p,
                bc + Vec2::new(0.0, -78.0),
                "SPAWN SHIELD",
                Align::Center,
            );
        }

        // Death screen.
        if !me.alive && m.phase == Phase::Playing {
            p.set_halo(None);
            let c = Vec2::new(w * 0.5, h * 0.36);
            plate(&mut p, c - Vec2::X * 190.0, Vec2::new(380.0, 140.0), 1.0);
            let mut big = hud_text(24.0);
            big.color = theme::HOT;
            p.text(c, "DESTROYED", &big, Align::Center);
            if let Some(k) = me.killed_by.filter(|&k| k != 0) {
                let mut s = LABEL;
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
            if me.burned {
                let y = match me.killed_by {
                    Some(k) if k != 0 => 57.0,
                    _ => 42.0,
                };
                p.text(
                    c + Vec2::new(0.0, y),
                    "BURNED IN A PLASMA RIVER",
                    &LABEL,
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
                let mut s = hud_text(14.0);
                s.color = theme::HOT;
                p.text(c + Vec2::new(0.0, 84.0), head, &s, Align::Center);
                if m.sitting_out() {
                    // It plays out fast: how far it's got, so it doesn't
                    // look stuck (#427).
                    p.text(
                        c + Vec2::new(0.0, 116.0),
                        "PLAYING OUT",
                        &LABEL,
                        Align::Center,
                    );
                    let o = c + Vec2::new(-80.0, 138.0);
                    widgets::hairbar(&mut p, o, 160.0, m.progress(), theme::HOT);
                }
            } else {
                // The countdown: whole seconds left, big.
                let left = me.respawn_in.max(0.0);
                p.text(
                    c + Vec2::new(0.0, 74.0),
                    "RESPAWN IN",
                    &LABEL,
                    Align::Center,
                );
                p.text(
                    c + Vec2::new(0.0, 104.0),
                    &(left.ceil().max(1.0) as u32).to_string(),
                    &hud_text(26.0),
                    Align::Center,
                );
            }
        }

        p.set_halo(outline);
        // Renown popups, just above the crosshair, drifting up as they fade.
        if !board {
            for (row, x) in m.renown_pops.iter().rev().take(3).enumerate() {
                let fade = ((RENOWN_POP_TIME - x.age) / 0.6).clamp(0.0, 1.0)
                    * (x.age / 0.08).clamp(0.0, 1.0);
                let at = Vec2::new(w * 0.5, h * 0.30 - x.age * 14.0 - row as f32 * 30.0);
                let mut s = hud_text(14.0);
                s.color = with_alpha(design::AMBER, fade);
                p.text(at, &format!("+{} RENOWN", x.amount), &s, Align::Center);
                let mut l = LABEL;
                l.color = with_alpha(design::TEXT, 0.8 * fade);
                p.text(at + Vec2::new(0.0, 24.0), x.reason, &l, Align::Center);
            }
        }

        p.set_halo(None);
        // Scrap popups (#444), off to the side (#474): under the
        // clock, what the newest kill's wreck taught of each
        // part, with a bar for how far into its next step you are. They
        // slide in, sit, and fade, leaving the middle of the screen clear.
        if !board {
            let n = m.scrap_pops.len();
            let top = MARGIN + 70.0;
            let shown = &m.scrap_pops[n.saturating_sub(2)..];
            for (row, x) in shown.iter().enumerate() {
                let fade = ((SCRAP_POP_TIME - x.age) / 0.6).clamp(0.0, 1.0)
                    * (x.age / 0.12).clamp(0.0, 1.0);
                let slide = (1.0 - ease_out(x.age / 0.25)) * -16.0;
                let at = Vec2::new(MARGIN + slide, top + row as f32 * SCRAP_POP_ROW);
                scrap_pop(&mut p, &x.scrapped, at, fade);
            }
        }

        // Bottom left: speed.
        plate(
            &mut p,
            Vec2::new(MARGIN, h - MARGIN - 62.0),
            Vec2::new(200.0, 60.0),
            1.0,
        );
        let bl = Vec2::new(MARGIN, h - MARGIN);
        small(&mut p, bl + Vec2::new(0.0, -62.0), "SPEED", Align::Left);
        let num = hud_text(20.0);
        let nw = widgets::odometer(
            &mut p,
            bl + Vec2::new(0.0, -48.0),
            self.speed,
            3,
            &num,
            Align::Left,
        );
        small(
            &mut p,
            bl + Vec2::new(nw + 10.0, -48.0 + num.size - LABEL.size),
            "KM/H",
            Align::Left,
        );
        widgets::hairbar(
            &mut p,
            bl + Vec2::new(0.0, -8.0),
            200.0,
            self.speed / max_speed,
            design::TEXT,
        );

        // Bottom centre: the kit queued for the next respawn, if any.
        let at = Vec2::new(w * 0.5, h - MARGIN - 8.0);
        if let Some((hull, turret)) = me.next_kit {
            let kit = format!(
                "{} · {}",
                crate::garage::hull_name(hull),
                crate::garage::turret_name(turret)
            );
            small(
                &mut p,
                at - Vec2::new(0.0, 30.0),
                "NEXT SPAWN",
                Align::Center,
            );
            p.text(
                at - Vec2::new(0.0, 18.0),
                &kit,
                &hud_text(9.0),
                Align::Center,
            );
        }

        // The board, over everything: Tab's, or with the round over, the same
        // one opened up with how it went and what you got.
        if board {
            p.set_over_scene(false);
            p.set_halo(None);
            let end = (m.phase == Phase::Over).then_some(Debrief {
                ledger,
                war: self.debrief.as_ref(),
                lit: self.hover == Some(Hit::Missions),
            });
            draw_board(&mut p, &mut hits, m, end, w, h);
        }

        if let Some(c) = self.chart.as_ref().filter(|_| self.map_up && !board) {
            let size = h * minimap::BIG;
            let min = (Vec2::new(w, h) - Vec2::splat(size)) * 0.5;
            p.set_over_scene(false);
            minimap::draw(&mut p, c, m, tanks, min, size);
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
    SCORE_IN + measure(score, &hud_text(14.0)).max(measure("00", &hud_text(14.0))) + 14.0
}

/// The HUD's small text, and its readouts.
const LABEL: TextStyle = TextStyle {
    weight: 1.3,
    ..design::SMALL
};

/// The dark behind a readout `size` at `min`, with room round it.
/// How far an enemy's health bar has faded, `d` away where tanks can be made
/// out `sight` away: 1 is not at all. Gone by `sight`.
fn bar_fade(d: f32, sight: f32) -> f32 {
    if sight.is_finite() {
        ((sight - d) / (sight * 0.25)).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

fn plate(p: &mut Painter, min: Vec2, size: Vec2, fade: f32) {
    plate_by(p, min, size, Vec2::splat(design::UNIT * 1.75), fade);
}

/// `plate`, closer in: a line of the kill feed's.
fn feed_plate(p: &mut Painter, min: Vec2, size: Vec2, fade: f32) {
    plate_by(p, min, size, Vec2::splat(design::UNIT), fade);
}

fn plate_by(p: &mut Painter, min: Vec2, size: Vec2, pad: Vec2, fade: f32) {
    let color = with_alpha(design::GROUND, 0.88 * fade);
    p.rect(min - pad, size + pad * 2.0, color);
}

/// How wide the team scores' plate is.
const SCORES_W: f32 = 380.0;

fn small(p: &mut Painter, at: Vec2, text: &str, align: Align) -> f32 {
    p.text(at, text, &LABEL, align)
}

fn hud_text(size: f32) -> TextStyle {
    TextStyle {
        size,
        tracking: size * 0.1,
        weight: 1.3,
        ..design::BODY
    }
}

/// The board's row pitch; a table's head and foot (its side and score; its
/// hulls left); and the gaps between tables, across and down.
const BOARD_ROW: f32 = design::UNIT * 3.0;
const BOARD_HEAD: f32 = design::UNIT * 7.0;
const BOARD_FOOT: f32 = design::UNIT * 5.0;
const BOARD_GAP: f32 = design::UNIT * 8.0;
const BOARD_DOWN: f32 = design::UNIT * 3.0;
/// Round the tables, and how much the board opens above them at the end.
const BOARD_PAD: f32 = design::UNIT * 4.0;
const BOARD_OVER: f32 = design::UNIT * 12.0;
/// A line of what you got, and the way on under them.
const GAIN_ROW: f32 = design::UNIT * 4.0;
const BOARD_ON: f32 = design::UNIT * 7.0;
/// How long the board takes to open.
const BOARD_OPENS: f32 = 0.35;

/// What the board shows besides the tables once the round's over.
struct Debrief<'a> {
    ledger: Ledger,
    war: Option<&'a WarResult>,
    /// The pointer's on CONTINUE.
    lit: bool,
}

/// How wide a table is: the longest name in the match beside its columns.
fn table_width(pilots: &[Pilot], cols: &StatColumns) -> f32 {
    let widest = pilots.iter().map(row_name_width).fold(0.0, f32::max);
    (widest + NAME_GAP + cols.width).max(380.0).ceil()
}

/// The board: each side's table, two to a row (your side first, top left).
/// With the round over (`end`) it opens up round them, which stay where they
/// were: how it went above, what you got below.
fn draw_board(
    p: &mut Painter,
    hits: &mut Hits<Hit>,
    m: &Match,
    end: Option<Debrief>,
    w: f32,
    h: f32,
) {
    let cols = StatColumns::new(m.contract == Contract::Hold);
    let pilots = m.pilots();
    let col = table_width(&pilots, &cols);
    let sides = m.factions.len();
    let blocks = sides.div_ceil(2) as f32;
    // Kept the size it'll be at the end, so the tables don't move then.
    let under = BOARD_PAD + GAIN_ROW * SCRAP_LINES as f32 + BOARD_ON;
    let room = h - 48.0 - BOARD_OVER - under - BOARD_PAD * 2.0;
    let fixed = blocks * (BOARD_HEAD + BOARD_FOOT) + (blocks - 1.0) * BOARD_DOWN;
    let fit = (((room - fixed) / blocks / BOARD_ROW) as usize).max(1);
    let most = (0..sides)
        .map(|t| pilots.iter().filter(|pi| pi.team == t).count())
        .max()
        .unwrap_or(0);
    let rows = most.min(fit);
    let block = BOARD_HEAD + rows as f32 * BOARD_ROW + BOARD_FOOT;
    let body = blocks * block + (blocks - 1.0) * BOARD_DOWN;
    let width = col * 2.0 + BOARD_GAP;
    let (x0, x1) = ((w - width) * 0.5, (w + width) * 0.5);
    let top = (h - BOARD_OVER - body - under) * 0.5 + BOARD_OVER;

    // The gains take as many lines as there are.
    let scrap = if end.is_some() {
        scrap_lines(m)
    } else {
        Vec::new()
    };
    let lines = if scrap.is_empty() {
        1
    } else {
        scrap.len().max(2)
    };
    let opened = if end.is_some() {
        ease_out(m.over_for / BOARD_OPENS)
    } else {
        0.0
    };
    let above = BOARD_PAD + BOARD_OVER * opened;
    let below = BOARD_PAD * 2.0 + GAIN_ROW * lines as f32 + BOARD_ON;
    let below = BOARD_PAD + (below - BOARD_PAD) * opened;
    p.rect(
        Vec2::new(x0 - BOARD_PAD, top - above),
        Vec2::new(width + BOARD_PAD * 2.0, above + body + below),
        design::GROUND,
    );
    for team in 0..sides {
        let a = if team % 2 == 0 { x0 } else { x1 - col };
        let y = top + (team / 2) as f32 * (block + BOARD_DOWN);
        table(p, m, &pilots, team, &cols, Vec2::new(a, y), col, rows);
    }
    let Some(end) = end.filter(|_| opened > 0.9) else {
        return;
    };

    // How it went, and what became of the site.
    let mid = w * 0.5;
    let (word, color) = match m.leader() {
        Some(t) if t == m.player().team => ("Victory", design::AMBER),
        Some(_) => ("Defeat", theme::HOT),
        None => ("Draw", design::TEXT),
    };
    let y = top - BOARD_OVER;
    let style = design::TITLE.colored(color);
    p.text(Vec2::new(mid, y), word, &style, Align::Center);
    if let Some(r) = end.war {
        let style = design::BODY.colored(design::TEXT_2);
        let at = Vec2::new(mid, y + design::UNIT * 5.0);
        p.text(at, &r.line(), &style, Align::Center);
    }

    // What you got: your rank and crystals, and beside them the parts.
    let t = m.over_for - RESULTS_DELAY;
    let y = top + body + BOARD_PAD;
    rank_gain(p, &end.ledger, m.renown, t, Vec2::new(x0, y), col);
    let pay = Payout::of(m).total();
    let at = if scrap.is_empty() {
        Vec2::new(x1 - col, y)
    } else {
        Vec2::new(x0, y + GAIN_ROW)
    };
    crystal_gain(p, end.ledger.crystals, pay, t, at, col);
    for (k, line) in scrap.iter().enumerate() {
        let at = Vec2::new(x1 - col, y + GAIN_ROW * k as f32);
        match line {
            ScrapLine::Part(sc) => part_gain(p, sc, t, at, col),
            ScrapLine::More(n) => {
                let style = design::SMALL.colored(design::TEXT_2);
                p.text(at, &format!("+{n} more"), &style, Align::Left);
            }
        }
    }

    let size = Vec2::new(design::UNIT * 26.0, design::BUTTON);
    let down = GAIN_ROW * lines as f32 + design::UNIT * 2.0;
    let min = Vec2::new(mid - size.x * 0.5, y + down);
    design::button(p, min, size.x, "Continue", true, end.lit);
    hits.add(p, min, size, Hit::Missions);
}

/// Side `team`'s table at `at`, `col` wide, with its first `rows` pilots
/// (and you, always).
#[allow(clippy::too_many_arguments)]
fn table(
    p: &mut Painter,
    m: &Match,
    pilots: &[Pilot],
    team: usize,
    cols: &StatColumns,
    at: Vec2,
    col: f32,
    rows: usize,
) {
    use design::{BODY, HEAD, SMALL, TEXT_2, UNIT};
    let (a, b) = (at.x, at.x + col);
    let f = m.factions[team];
    let style = HEAD.colored(f.ui());
    emblem::tagged(p, at, f, &m.score(team).to_string(), &style, Align::Left);
    for (name, dx) in cols.headers() {
        let at = Vec2::new(b - dx, at.y + HEAD.size - SMALL.size);
        p.text(at, name, &SMALL.colored(TEXT_2), Align::Right);
    }
    let rule = Vec2::new(a, at.y + UNIT * 4.0);
    p.rect(rule, Vec2::new(col, 2.0), with_alpha(f.ui(), 0.7));

    let side: Vec<&Pilot> = pilots.iter().filter(|pi| pi.team == team).collect();
    let mut shown: Vec<&Pilot> = side.iter().copied().take(rows).collect();
    if let Some(you) = side.iter().skip(rows).find(|pi| pi.seat == 0) {
        shown.pop();
        shown.push(you);
    }
    let mut y = at.y + BOARD_HEAD;
    for pi in shown {
        let s = row_style(pi);
        let mut x = a;
        if pi.company {
            // A merc: the Company's mark, in its colors (#389).
            let mut c = s;
            if pi.active {
                c.color = design::AMBER;
            }
            x += emblem::mark(p, Vec2::new(x, y), Faction::Company, &c);
        }
        p.text(Vec2::new(x, y), &kit_name(pi), &s, Align::Left);
        let mut stat = |dx: f32, n: String| {
            p.text(Vec2::new(b - dx, y), &n, &s, Align::Right);
        };
        if let Some(dx) = cols.held {
            stat(dx, format!("{:.0}", pi.held.floor()));
        }
        stat(cols.kills, pi.kills.to_string());
        stat(0.0, pi.deaths.to_string());
        y += BOARD_ROW;
    }

    // Hulls left (#423): the realm's, and its mercs' together.
    let Some((realm, companies)) = m.hull_summary(team) else {
        return;
    };
    let y = at.y + BOARD_HEAD + rows as f32 * BOARD_ROW + UNIT;
    let mercs: u32 = companies.iter().map(|(_, n)| n).sum();
    let style = SMALL.colored(TEXT_2);
    p.text(Vec2::new(a, y + 1.5), "Hulls left", &style, Align::Left);
    let seats = m.seats(team) as u32;
    let mut x = b;
    let mut count = |f: Faction, n: u32, color: Rgba| {
        let color = match n {
            0 => TEXT_2,
            n if n <= seats => theme::HOT,
            _ => color,
        };
        let style = BODY.colored(color);
        x -= emblem::tagged(p, Vec2::new(x, y), f, &n.to_string(), &style, Align::Right);
        x -= UNIT * 2.0;
    };
    if !companies.is_empty() {
        count(Faction::Company, mercs, design::AMBER);
    }
    // Raiders are all mercs.
    if f != Faction::Company {
        count(f, realm, f.ui());
    }
}

/// A gain's bar, in the middle of its line at `at`: what you had, and what
/// you got lit.
fn gain_bar(p: &mut Painter, at: Vec2, col: f32, from: f32, to: f32) {
    let (x, w) = (at.x + col * 0.4, col * 0.45);
    let y = at.y + design::SMALL.size * 0.5 - 1.5;
    let h = 3.0;
    let rail = with_alpha(design::TEXT_2, 0.3);
    p.rect(Vec2::new(x, y), Vec2::new(w, h), rail);
    p.rect(Vec2::new(x, y), Vec2::new(w * from, h), design::TEXT_2);
    if to > from {
        let size = Vec2::new(w * (to - from), h);
        p.rect(Vec2::new(x + w * from, y), size, design::AMBER);
    }
}

/// Your rank, its bar filling by the renown you earned.
fn rank_gain(p: &mut Painter, ledger: &Ledger, gained: u32, t: f32, at: Vec2, col: f32) {
    let shown = renown_shown(gained, t);
    let start = Rank::of(ledger.renown);
    let now = Rank::of(ledger.renown + shown);
    let up = now.level > start.level;
    let style = design::SMALL.colored(if up { design::AMBER } else { design::TEXT });
    let name = format!("{} {}", now.title().name(), now.level);
    p.text(at, &name, &style, Align::Left);
    let from = if up { 0.0 } else { start.progress() };
    gain_bar(p, at, col, from, now.progress());
    let style = design::SMALL.colored(design::AMBER);
    let right = at + Vec2::X * col;
    p.text(right, &format!("+{shown}"), &style, Align::Right);
}

/// Your crystals going up by the pay, and the pay.
fn crystal_gain(p: &mut Painter, before: u32, pay: u32, t: f32, at: Vec2, col: f32) {
    let got = (pay as f32 * ease_out(t / REP_FILL)).round() as u32;
    let style = design::SMALL.colored(design::BLUE);
    p.text(at, &format!("{} cr", before + got), &style, Align::Left);
    let right = at + Vec2::X * col;
    p.text(right, &format!("+{got}"), &style, Align::Right);
}

/// A part you've got further with: its bar filling, and the step it's
/// heading for, or that it's new.
fn part_gain(p: &mut Painter, sc: &Scrapped, t: f32, at: Vec2, col: f32) {
    let k = ease_out(t / REP_FILL);
    let maker = sc.part.maker();
    let style = design::SMALL.colored(maker.ui());
    emblem::tagged(p, at, maker, sc.part.name(), &style, Align::Left);
    let (from, to) = scrap_bar(sc);
    gain_bar(p, at, col, from, from + (to - from) * k);
    let text = step_label(sc);
    let lit = sc.stepped() && k >= 1.0;
    let style = design::SMALL.colored(if lit { design::AMBER } else { design::TEXT_2 });
    p.text(at + Vec2::X * col, &text, &style, Align::Right);
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

impl WarResult {
    /// "FOUNDRY TAKEN".
    fn line(&self) -> String {
        let site = match self.site {
            "SHIELD GENERATOR" => "Shield",
            "CRYSTAL MINE" => "Mine",
            "TUNGSTEN HOARD" => "Hoard",
            "OUTPOST" => "Outpost",
            _ => "Foundry",
        };
        format!("{site} {}", self.verdict.word())
    }
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

    pub fn word(self) -> &'static str {
        match self {
            Verdict::Taken => "taken",
            Verdict::Holds => "not taken",
            Verdict::Held => "held",
            Verdict::Lost => "lost",
        }
    }
}

/// Scoreboard row text size.
const ROW_STYLE: TextStyle = design::SMALL;
/// Space between a tank's name and the first stat column, and between stat columns.
const NAME_GAP: f32 = 20.0;
const STAT_GAP: f32 = 22.0;

/// A pilot's name on the board: their hull and turret.
fn kit_name(pi: &Pilot) -> String {
    format!(
        "{} · {}",
        crate::garage::hull_name(pi.hull),
        crate::garage::turret_name(pi.turret)
    )
}

/// A scoreboard row's text: yours amber, and greyed out once the pilot's
/// out of the fight (#389).
fn row_style(pi: &Pilot) -> TextStyle {
    let mut s = ROW_STYLE;
    if !pi.active {
        s.color = OUT_ROW;
    } else if pi.seat == 0 {
        s.color = design::AMBER;
    }
    s
}

/// A pilot out of the fight, on the scoreboard: there, but faint.
const OUT_ROW: Rgba = [0.36, 0.365, 0.38, 0.45];

/// How wide a pilot's name is on its row, the Company's mark among it.
fn row_name_width(pi: &Pilot) -> f32 {
    let s = ROW_STYLE;
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
        let w = |s: &str| measure(s, &design::SMALL).max(measure("000", &ROW_STYLE));
        let kills = w("Deaths") + STAT_GAP;
        let held = holding.then(|| kills + w("Kills") + STAT_GAP);
        let width = match held {
            Some(h) => h + w("Held"),
            None => kills + w("Kills"),
        };
        StatColumns { held, kills, width }
    }

    /// Each header with its right edge's distance from the table's right edge.
    fn headers(&self) -> Vec<(&'static str, f32)> {
        let mut out: Vec<_> = self.held.map(|h| ("Held", h)).into_iter().collect();
        out.push(("Kills", self.kills));
        out.push(("Deaths", 0.0));
        out
    }
}

/// Who has the site, then each side's time held against the target: your
/// side's bar grows left from the middle, theirs right.
fn hold_meter(p: &mut Painter, m: &Match, hd: &hold::Hold, at: Vec2) {
    let mut l = LABEL;
    match &hd.sites[..] {
        [site] => {
            let status = match site.control {
                Control::Open => format!("{} OPEN", site.name),
                Control::Held(s) => format!("{} HOLDS", m.factions[s].name()),
                Control::Contested => "CONTESTED".to_string(),
            };
            l.color = hold_color(m, site.control);
            match site.control {
                Control::Held(s) => {
                    emblem::tagged(p, at, m.factions[s], &status, &l, Align::Center)
                }
                _ => p.text(at, &status, &l, Align::Center),
            };
        }
        // Several: each one's name, in the color of who has it.
        sites => {
            let wide = 130.0;
            for (k, site) in sites.iter().enumerate() {
                let x = (k as f32 - (sites.len() - 1) as f32 * 0.5) * wide;
                let at = at + Vec2::new(x, 0.0);
                l.color = hold_color(m, site.control);
                match site.control {
                    Control::Held(s) => {
                        emblem::tagged(p, at, m.factions[s], site.name, &l, Align::Center)
                    }
                    _ => p.text(at, site.name, &l, Align::Center),
                };
            }
        }
    }
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

/// The Raid under the scores: while the flag's home, whether your side
/// attacks or defends; once it's not, who has it, or how long it lies.
fn raid_meter(p: &mut Painter, m: &Match, r: &raid::Raid, at: Vec2) {
    let mine = m.player().team;
    let (text, color) = match r.flag {
        raid::Flag::Home => {
            let job = if r.attackers() == mine {
                "ATTACK"
            } else {
                "DEFEND"
            };
            (job.to_string(), m.factions[mine].ui())
        }
        raid::Flag::Carried(0) => ("YOU HAVE THE FLAG".to_string(), design::AMBER),
        raid::Flag::Carried(i) => {
            let f = m.factions[m.fighters[i].team];
            (format!("{} HAS THE FLAG", f.name()), f.ui())
        }
        raid::Flag::Dropped { for_, .. } => {
            let left = (raid::RETURN_TIME - for_).ceil();
            (format!("FLAG DROPPED  {left:.0}"), theme::HOT)
        }
    };
    p.text(at, &text, &LABEL.colored(color), Align::Center);
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
const SCRAP_LINES: usize = 4;
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

/// The step a part's heading for, the one it just reached, or that it's new.
fn step_label(sc: &Scrapped) -> String {
    let whole = sc.known.floor() as u8;
    match sc.toward().filter(|_| !sc.stepped()) {
        Some((_, next)) => economy::level_label(next),
        None if whole == 1 && !sc.part.starter() => "New".to_string(),
        None => economy::level_label(whole),
    }
}

/// Each in-battle scrap popup's height, and its width (the speed bar's).
const SCRAP_POP_ROW: f32 = 52.0;
const SCRAP_POP_W: f32 = 200.0;

/// One part's scrap popup at `at` (its top left): the part, the step it's
/// heading for or just reached, and a bar for how far into it you are.
fn scrap_pop(p: &mut Painter, sc: &Scrapped, at: Vec2, fade: f32) {
    plate(p, at, Vec2::new(SCRAP_POP_W, 18.0), fade);
    let faded = |c: Rgba| with_alpha(c, fade);
    let maker = sc.part.maker();
    let style = LABEL.colored(faded(maker.ui()));
    emblem::tagged(p, at, maker, sc.part.name(), &style, Align::Left);
    let color = if sc.stepped() {
        design::AMBER
    } else {
        design::TEXT
    };
    let right = at + Vec2::X * SCRAP_POP_W;
    let style = LABEL.colored(faded(color));
    p.text(right, &step_label(sc), &style, Align::Right);
    let (from, to) = scrap_bar(sc);
    let bar = at + Vec2::Y * 16.0;
    let piece = |p: &mut Painter, a: f32, b: f32, color: Rgba| {
        let min = bar + Vec2::X * SCRAP_POP_W * a;
        p.rect(min, Vec2::new(SCRAP_POP_W * (b - a), 2.0), faded(color));
    };
    piece(p, 0.0, 1.0, with_alpha(design::TEXT, 0.2));
    piece(p, 0.0, from, design::TEXT);
    piece(p, from, to, design::AMBER);
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

fn ease_out(x: f32) -> f32 {
    1.0 - (1.0 - x.clamp(0.0, 1.0)).powi(3)
}

/// How much of `gained` the renown graphic shows `t` seconds in.
fn renown_shown(gained: u32, t: f32) -> u32 {
    (gained as f32 * ease_out(t / REP_FILL)).round() as u32
}

/// The debug overlay's frame rate, small in the top right corner above the HUD brackets.
/// Over a battle you've left (#427), dimmed under it: the match playing
/// out without you, fast, with a bar for how far through it is.
pub fn playing_out(m: &Match, width: f32, height: f32) -> Vec<UiVertex> {
    let (mut p, at) = notice("Left the battle", "Playing out", width, height);
    let bar = at + Vec2::Y * design::UNIT * 9.0;
    design::bar(&mut p, bar, NOTICE_W, m.progress());
    p.finish()
}

/// How wide a notice's content is.
const NOTICE_W: f32 = design::UNIT * 50.0;

/// A box over the dimmed screen: `title`, under what it's about, small.
/// Returns where its content starts, with room under the title.
fn notice(what: &str, title: &str, width: f32, height: f32) -> (Painter, Vec2) {
    let scale = height / REF_HEIGHT;
    let (w, h) = (width / scale, REF_HEIGHT);
    let mut p = Painter::new(scale);
    p.set_halo(None);
    p.rect(Vec2::ZERO, Vec2::new(w, h), [0.0, 0.0, 0.0, 0.6]);
    let pad = Vec2::splat(design::UNIT * 5.0);
    let size = Vec2::new(NOTICE_W, design::UNIT * 9.0) + pad * 2.0;
    let min = (Vec2::new(w, h) - size) * 0.5;
    p.rect(min, size, design::GROUND);
    let at = min + pad;
    let style = design::SMALL.colored(design::TEXT_2);
    p.text(at, what, &style, Align::Left);
    let line = at + Vec2::Y * design::UNIT * 3.0;
    p.text(line, title, &design::TITLE, Align::Left);
    (p, at)
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
    pub hover: Option<Hit>,
}

/// What your battle did to its system (#475), a page between its results and
/// the contract screen, over the system (drawn by `theater_view::scene`). The
/// contract screen loads behind it; CONTINUE goes on to it.
pub fn tick_summary(t: &TickSummary, state: SummaryState, width: f32, height: f32) -> Layer<Hit> {
    let scale = height / REF_HEIGHT;
    let (w, h) = (width / scale, REF_HEIGHT);
    let mut p = Painter::new(scale);
    let mut hits = Hits::new();
    let ready = !(state.going && state.loading);
    let lit = state.hover == Some(Hit::Continue);
    let (min, size) = crate::theater_view::aftermath(&mut p, t, state.time, ready, lit, w, h);
    if ready {
        hits.add(&p, min, size, Hit::Continue);
    } else {
        let c = min + Vec2::new(size.x + design::UNIT * 3.0, size.y * 0.5);
        spinner_small(&mut p, c, state.time);
    }
    Layer {
        verts: p.finish(),
        hits,
    }
}

/// `spinner`, small enough to sit beside a label.
pub fn spinner_small(p: &mut Painter, c: Vec2, time: f32) {
    let r = 7.0;
    let tau = std::f32::consts::TAU;
    p.arc(c, r, 0.0, tau, 1.5, with_alpha(design::TEXT_2, 0.4));
    let a = (time / SPIN_PERIOD).fract() * tau;
    p.arc(c, r, a, a + tau * 0.3, 2.0, design::AMBER);
}

/// Seconds the loading spinner takes to go round once.
const SPIN_PERIOD: f32 = 1.2;

/// A loading panel over the screen, `time` seconds after it went up:
/// what's loading, small over it what for, and a spinner turning, so the
/// screen never sits frozen while a map's built.
pub fn loading(what: &str, title: &str, time: f32, width: f32, height: f32) -> Vec<UiVertex> {
    let (mut p, at) = notice(what, title, width, height);
    let c = at + Vec2::new(NOTICE_W - 22.0, design::UNIT * 4.5);
    spinner(&mut p, c, time);
    p.finish()
}

/// A loading spinner at `c`, `time` seconds in: a faint ring with a bright
/// arc going round it.
fn spinner(p: &mut Painter, c: Vec2, time: f32) {
    let r = 22.0;
    let tau = std::f32::consts::TAU;
    p.arc(c, r, 0.0, tau, 2.0, with_alpha(design::TEXT_2, 0.4));
    let a = (time / SPIN_PERIOD).fract() * tau;
    p.arc(c, r, a, a + tau * 0.3, 3.0, design::AMBER);
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
                    "{} · {}",
                    crate::garage::hull_name(h),
                    crate::garage::turret_name(t)
                );
                measure(&name, &ROW_STYLE)
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
                    r + measure(right, &design::SMALL) + 8.0 < l,
                    "{left} {right}"
                );
            }
        }
    }

    #[test]
    fn renown_fills_to_what_was_gained() {
        assert_eq!(renown_shown(37, -1.0), 0);
        assert!(renown_shown(37, REP_FILL * 0.5) < 37);
        assert_eq!(renown_shown(37, REP_FILL), 37);
    }

    #[test]
    fn ledger_backs_out_this_matchs_earnings() {
        let mut m = Match::new(crate::map::Map::reservoir(0), 0, 11, 21);
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
        let mut m = Match::new(crate::map::Map::reservoir(0), 0, 11, 21);
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
        // Over, it has its way on, Tab held or not.
        m.over_for = 1.0;
        assert!(!draw(&mut hud, &m, true).is_empty());
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
    }

    #[test]
    fn draws_at_any_size() {
        for contract in [Contract::Attrition, Contract::Hold, Contract::Raid] {
            draws_at_any_size_under(contract);
        }
    }

    fn draws_at_any_size_under(contract: Contract) {
        let mut m = Match::new(crate::map::Map::reservoir(0), 0, 11, 21);
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
