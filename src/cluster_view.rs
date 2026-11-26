//! Your local cluster, behind the main menu: the stars around your ship,
//! each in its holder's color, lanes joining each realm's, and where
//! there's fighting. A picture of the war, not a way to pick contracts.

use std::f32::consts::TAU;

use glam::Vec2;

use crate::faction::Faction;
use crate::galaxy::theater::{ship_name, system_name};
use crate::galaxy::{Galaxy, realm};
use crate::menu::{Hit, title};
use crate::ui::{Align, Hits, Painter, design, with_alpha};

/// How far up and down from you, and across, the map reaches (ly).
const REACH: f32 = 330.0;
const WIDE: f32 = 3.0 * REACH;
/// Lanes are drawn between a realm's neighboring stars this close (ly).
const LANE: f32 = 125.0;
/// The margin the map keeps from the screen's edges (logical px).
const EDGE: f32 = 48.0;
/// How far the map drifts (ly), and how long that takes (s).
const DRIFT: f32 = 7.0;
const DRIFT_TIME: f32 = 70.0;

/// One star: where, from you.
#[derive(Clone, Debug)]
struct Star {
    at: Vec2,
    name: String,
    side: usize,
    /// Its realm's seat.
    seat: bool,
    /// Who's fighting there, if anyone.
    war: Vec<usize>,
}

/// Your contract.
#[derive(Clone, Debug)]
pub struct Job {
    pub system: String,
    pub side: usize,
    pub defend: bool,
}

/// The stars around you as of a tick.
#[derive(Clone, Debug)]
pub struct Cluster {
    pub tick: u64,
    pub crew: Option<u32>,
    pub job: Option<Job>,
    stars: Vec<Star>,
    /// Pairs of stars (by index) one realm holds.
    lanes: Vec<(usize, usize)>,
    ship: Option<String>,
}

impl Cluster {
    pub fn of(g: &Galaxy) -> Cluster {
        let company = g.crew_company().map(|c| &g.companies[c]);
        let here = company.map_or(0, |c| c.at as usize);
        let center = g.systems[here].pos.truncate();
        let mut index = vec![None; g.systems.len()];
        let mut near = Vec::new();
        for (i, s) in g.systems.iter().enumerate() {
            let d = s.pos.truncate() - center;
            if d.x.abs() < WIDE && d.y.abs() < REACH * 1.2 {
                index[i] = Some(near.len());
                near.push(i);
            }
        }
        let stars = near
            .iter()
            .map(|&i| {
                let s = &g.systems[i];
                let r = g.holder(i);
                Star {
                    at: s.pos.truncate() - center,
                    name: system_name(i),
                    side: s.owner(),
                    seat: r != realm::NONE && g.realms[r as usize].seat as usize == i,
                    war: if s.at_war() {
                        (0..s.hulls.len()).filter(|&f| s.hulls[f] > 0).collect()
                    } else {
                        Vec::new()
                    },
                }
            })
            .collect();
        let mut lanes = Vec::new();
        for (a, &i) in near.iter().enumerate() {
            for (j, _) in g.neighbors(i) {
                let close = g.systems[i].pos.distance(g.systems[j].pos) < LANE;
                if let Some(b) = index[j].filter(|&b| a < b && close) {
                    let r = g.holder(i);
                    if r != realm::NONE && r == g.holder(j) {
                        lanes.push((a, b));
                    }
                }
            }
        }
        Cluster {
            tick: g.tick,
            crew: g.crew,
            job: company.and_then(|c| c.contract).map(|k| Job {
                system: system_name(k.target as usize),
                side: k.side as usize,
                defend: k.defend,
            }),
            stars,
            lanes,
            ship: company.map(|c| ship_name(c.id)),
        }
    }
}

/// The map over a `w` by `h` screen, you in the middle of what the panel
/// leaves; `hover` is the star pointed at.
pub fn draw(
    p: &mut Painter,
    hits: &mut Hits<Hit>,
    c: Option<&Cluster>,
    hover: Option<usize>,
    time: f32,
    w: f32,
    h: f32,
) {
    p.set_halo(None);
    p.rect(Vec2::ZERO, Vec2::new(w, h), [0.002, 0.0022, 0.003, 1.0]);
    let Some(c) = c else { return };
    let left = design::MARGIN * 2.0 + design::COLUMN;
    let mid = Vec2::new((left + w) * 0.5, h * 0.5);
    let k = h * 0.5 / REACH;
    let a = TAU * time / DRIFT_TIME;
    let drift = Vec2::new(a.sin(), (a * 2.0).sin() * 0.5) * DRIFT;
    let put = |s: &Star| mid + (s.at + drift) * k;
    // Kept clear of the edges, so nothing's drawn off the screen.
    let on = |q: Vec2| q.x > left - 60.0 && q.x < w - EDGE && q.y > EDGE && q.y < h - EDGE;

    for &(a, b) in &c.lanes {
        let (sa, sb) = (&c.stars[a], &c.stars[b]);
        let (qa, qb) = (put(sa), put(sb));
        if on(qa) && on(qb) {
            let color = with_alpha(Faction::side(sa.side).ui(), 0.22);
            p.line(qa, qb, 1.0, color);
        }
    }
    let mut names = Vec::new();
    for (i, s) in c.stars.iter().enumerate() {
        let q = put(s);
        if !on(q) {
            continue;
        }
        let color = Faction::side(s.side).ui();
        let r = if s.seat { 5.0 } else { 2.5 };
        p.disc(q, 13.0, with_alpha(color, 0.16), with_alpha(color, 0.0));
        p.disc(q, r, color, with_alpha(color, 0.9));
        if !s.war.is_empty() {
            // Split between whoever's fighting, and pulsing.
            let pulse = 0.5 + 0.5 * (time * 2.2 + i as f32 * 1.7).sin();
            let part = TAU / s.war.len() as f32;
            for (n, &f) in s.war.iter().enumerate() {
                let a0 = part * n as f32 + time * 0.25;
                let side = with_alpha(Faction::side(f).ui(), 0.55 + 0.45 * pulse);
                p.arc(q, 11.0, a0, a0 + part * 0.82, 2.0, side);
            }
        }
        if hover == Some(i) {
            names.push((q, s));
        }
        hits.add(p, q - Vec2::splat(13.0), Vec2::splat(26.0), Hit::Star(i));
    }
    // You.
    let you = mid + drift * k;
    if let Some(ship) = &c.ship {
        p.arc(you, 20.0, 0.0, TAU, 1.5, design::AMBER);
        let name = title(ship);
        let style = design::BODY.colored(design::AMBER);
        p.text(you - Vec2::new(0.0, 44.0), &name, &style, Align::Center);
    }
    for (q, s) in names {
        let at = q + Vec2::new(0.0, 24.0);
        let name = title(&s.name);
        p.text(at, &name, &design::BODY, Align::Center);
        let holder = Faction::side(s.side);
        let style = design::SMALL.colored(holder.ui());
        let what = if s.war.is_empty() {
            title(holder.name())
        } else {
            let sides: Vec<_> = s
                .war
                .iter()
                .map(|&f| title(Faction::side(f).name()))
                .collect();
            sides.join(" vs ")
        };
        p.text(at + Vec2::new(0.0, 22.0), &what, &style, Align::Center);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::galaxy::Rules;

    #[test]
    fn the_cluster_is_round_you_and_on_screen() {
        let mut g = Galaxy::with_size(3, 400, Vec2::new(1800.0, 1100.0), Rules::default());
        for _ in 0..40 {
            g.step();
        }
        g.enlist(None);
        let c = Cluster::of(&g);
        assert!(c.stars.len() > 20);
        assert!(
            c.stars.iter().any(|s| s.at == Vec2::ZERO),
            "you're over a star"
        );
        assert_eq!(c.job.is_some(), g.crew_company().is_some());
        for (w, h) in [(1600.0, 900.0), (1200.0, 900.0)] {
            for time in [0.0, 17.0, 50.0] {
                let mut p = Painter::new(1.0);
                let mut hits = Hits::new();
                draw(&mut p, &mut hits, Some(&c), Some(0), time, w, h);
                assert!(!hits.is_empty());
                assert!(p.finish().iter().all(|v| {
                    let [x, y] = v.pos;
                    x > -4.0 && x < w + 4.0 && y > -4.0 && y < h + 4.0
                }));
            }
        }
    }
}
