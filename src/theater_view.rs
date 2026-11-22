//! The missions screen's picture of the theater (#311): a stylized view of
//! the system you're posted to, in the UI's hairlines. Its star on the left,
//! its worlds on their orbits, and each world's sites on a ring around it:
//! shield generators as domes, foundries as squares, crystal mines as
//! diamonds, in their holder's colour. Contested sites are ringed; the ones
//! your side fights for this tick are ringed hot and filled. Every contested
//! site can be clicked (it's a row of the list beside it). A generator
//! that's down is a ghost of itself, ringed with its repairs: a step a
//! tick, those done in its holder's colour (#341). Your company's
//! ship hangs in amber over the world it's parked over (#327).

use std::f32::consts::{PI, TAU};

use glam::Vec2;

use crate::faction::Faction;
use crate::galaxy::theater::{SiteView, Theater, planet_name};
use crate::menu::Hit;
use crate::ui::widgets;
use crate::ui::{Align, Hits, Painter, Rgba, TextStyle, measure, theme, with_alpha};

const STAR: Rgba = [1.0, 0.86, 0.62, 1.0];
/// The frame's fill: opaque, so nothing of the map behind shows through.
const BG: Rgba = [0.025, 0.025, 0.03, 1.0];
/// Height of the legend strip along the bottom.
const LEGEND_H: f32 = 46.0;

/// `a` over `b`, `k` of the way (opaque results stay opaque: no alpha
/// overlaps where strokes meet).
fn mix(b: Rgba, a: Rgba, k: f32) -> Rgba {
    [
        b[0] + (a[0] - b[0]) * k,
        b[1] + (a[1] - b[1]) * k,
        b[2] + (a[2] - b[2]) * k,
        b[3] + (a[3] - b[3]) * k,
    ]
}

/// A full circle, smooth at any size.
fn ring(p: &mut Painter, c: Vec2, r: f32, width: f32, color: Rgba) {
    let n = ((r * 1.2) as usize).clamp(20, 160);
    let pts: Vec<Vec2> = (0..=n)
        .map(|i| TAU * i as f32 / n as f32)
        .map(|a| c + Vec2::new(a.cos(), a.sin()) * r)
        .collect();
    p.polyline(&pts, width, color);
}

/// A solid disc with a crisp edge.
fn dot(p: &mut Painter, c: Vec2, r: f32, color: Rgba) {
    p.disc(c, r, color, color);
    ring(p, c, r - 0.5, 1.0, color);
}

/// Text on a backing of the frame's colour, so orbits pass behind it.
fn label(p: &mut Painter, at: Vec2, text: &str, style: &TextStyle) {
    let w = measure(text, style);
    p.rect(
        Vec2::new(at.x - w * 0.5 - 6.0, at.y - 3.0),
        Vec2::new(w + 12.0, style.size + 6.0),
        BG,
    );
    p.text(at, text, style, Align::Center);
}

/// Draw `t` in the box at `min`, `size` (logical px), contested site `selected`
/// marked. Each contested site's marker is a `Hit::Row` of its index in the list.
pub fn draw(
    p: &mut Painter,
    hits: &mut Hits<Hit>,
    t: &Theater,
    selected: usize,
    min: Vec2,
    size: Vec2,
) {
    let max = min + size;
    p.rect(min, size, BG);
    widgets::corners(p, min, max, 14.0, theme::DIM);

    // The sun on the left, the worlds on a line out from it, the space
    // between the header and the legend theirs.
    let top = min.y + 70.0;
    let bottom = max.y - LEGEND_H;
    let cy = (top + bottom) * 0.5;
    let star = Vec2::new(min.x + 92.0, cy);
    let n = t.planets.max(1) as usize;
    let first = star.x + 200.0f32.min(size.x * 0.28);
    let last = max.x - 120.0;
    let xs: Vec<f32> = (0..n)
        .map(|i| match n {
            1 => (first + last) * 0.5,
            _ => first + (last - first) * i as f32 / (n - 1) as f32,
        })
        .collect();
    // As big as the height allows, a little bigger with more sites.
    let big = ((bottom - top) * 0.09).clamp(26.0, 64.0);

    // Orbits: whole circles about the sun, cut by the frame (and the legend).
    p.set_clip(Some((
        min + Vec2::splat(2.0),
        Vec2::new(max.x - 2.0, bottom),
    )));
    for &x in &xs {
        ring(p, star, x - star.x, 1.0, mix(BG, theme::FG, 0.16));
    }
    p.set_clip(None);
    p.line(
        Vec2::new(min.x + 16.0, bottom),
        Vec2::new(max.x - 16.0, bottom),
        1.0,
        mix(BG, theme::FG, 0.12),
    );

    // The sun: a glow, then the disc.
    p.disc(star, 88.0, with_alpha(STAR, 0.22), with_alpha(STAR, 0.0));
    p.disc(star, 30.0, [1.0, 0.97, 0.9, 1.0], STAR);
    ring(p, star, 29.5, 1.0, STAR);

    for (pl, &x) in xs.iter().enumerate() {
        let c = Vec2::new(x, cy);
        let sites: Vec<(usize, &SiteView)> = t
            .sites
            .iter()
            .enumerate()
            .filter(|(_, s)| s.planet as usize == pl)
            .collect();
        let r = big * (0.8 + 0.06 * sites.len() as f32).min(1.1);
        let mut held = [0usize; 4];
        for (_, s) in &sites {
            held[s.owner] += 1;
        }
        let holder = (0..4).max_by_key(|&f| held[f]).unwrap_or(0);
        let tint = Faction::ALL[holder].ui();

        // The world: an atmosphere in its holder's colour, a dark ball lit
        // from the sun's side, and its rim.
        p.disc(c, r + 14.0, with_alpha(tint, 0.16), with_alpha(tint, 0.0));
        p.disc(c, r, mix(BG, theme::FG, 0.1), mix(BG, theme::FG, 0.04));
        p.arc(c, r - 5.0, PI * 0.7, PI * 1.3, 3.0, mix(BG, STAR, 0.45));
        ring(p, c, r, 1.6, mix(BG, tint, 0.9));

        // Its sites on a ring round it, from the top.
        let ring_r = r + 30.0;
        let k = sites.len().max(1);
        for (j, &(i, s)) in sites.iter().enumerate() {
            let a = -PI * 0.5 + TAU * j as f32 / k as f32;
            let dir = Vec2::new(a.cos(), a.sin());
            let at = c + dir * ring_r;
            p.line(
                c + dir * (r + 3.0),
                c + dir * (ring_r - 15.0),
                1.0,
                mix(BG, tint, 0.5),
            );
            let row = t.contested.iter().position(|&x| x == i);
            site_marker(p, at, s, row == Some(selected));
            if let Some(row) = row {
                hits.add(p, at - Vec2::splat(18.0), Vec2::splat(36.0), Hit::Row(row));
            }
        }

        // Your ship, parked over it.
        if let Some(ship) = t.ship.as_ref().filter(|s| s.over as usize == pl) {
            let at = Vec2::new(c.x, (c.y - ring_r - 70.0).max(top + 40.0));
            ship_glyph(p, at, 1.4);
            let mut style = TextStyle::LABEL;
            style.color = Faction::Company.ui();
            label(p, at - Vec2::new(0.0, 40.0), &ship.name, &style);
        }

        // Its name, and who holds what.
        let below = c.y + ring_r + 30.0;
        label(
            p,
            Vec2::new(c.x, below),
            &planet_name(&t.name, pl as u8),
            &TextStyle::value(13.0),
        );
        let parts: Vec<String> = (0..4)
            .filter(|&f| held[f] > 0)
            .map(|f| format!("{} {}", Faction::ALL[f].name(), held[f]))
            .collect();
        let mut style = TextStyle::LABEL;
        style.color = theme::DIM;
        label(p, Vec2::new(c.x, below + 22.0), &parts.join(" · "), &style);
    }

    // The theater's name over it, and what the marks mean under it.
    p.text(
        Vec2::new(min.x + 24.0, min.y + 22.0),
        &format!("{} SYSTEM", t.name),
        &TextStyle::value(14.0),
        Align::Left,
    );
    let mut style = TextStyle::LABEL;
    style.color = theme::DIM;
    p.text(
        Vec2::new(min.x + 24.0, min.y + 44.0),
        &format!("CATALOGUE {:04} · TICK {}", t.system, t.tick),
        &style,
        Align::Left,
    );
    legend(p, Vec2::new(min.x + 24.0, bottom + 17.0), size.x - 48.0);
}

/// A company ship at `at`, `k` times its size: a long hull, bow to the
/// right, its drives aglow at the stern, in the Company's amber.
fn ship_glyph(p: &mut Painter, at: Vec2, k: f32) {
    let amber = Faction::Company.ui();
    let pts: Vec<Vec2> = [
        (30.0, 0.0),
        (16.0, -6.0),
        (-6.0, -6.0),
        (-12.0, -11.0),
        (-22.0, -11.0),
        (-20.0, -5.0),
        (-24.0, -5.0),
        (-24.0, 5.0),
        (-20.0, 5.0),
        (-22.0, 11.0),
        (-12.0, 11.0),
        (-6.0, 6.0),
        (16.0, 6.0),
        (30.0, 0.0),
    ]
    .iter()
    .map(|&(x, y)| at + Vec2::new(x, y) * k)
    .collect();
    // Drives, then the hull's fill and outline.
    for dy in [-2.5, 2.5] {
        let d = at + Vec2::new(-27.0, dy) * k;
        p.disc(d, 6.0 * k, with_alpha(amber, 0.5), with_alpha(amber, 0.0));
    }
    p.rect(
        at + Vec2::new(-20.0, -4.5) * k,
        Vec2::new(36.0, 9.0) * k,
        mix(BG, amber, 0.25),
    );
    p.polyline(&pts, 1.5, amber);
    p.line(
        at + Vec2::new(-6.0, 0.0) * k,
        at + Vec2::new(14.0, 0.0) * k,
        1.0,
        mix(BG, amber, 0.6),
    );
}

/// What kind of site it is, as a small glyph in `color` at `at`.
fn glyph(p: &mut Painter, at: Vec2, s: &SiteView, color: Rgba) {
    if s.is_shield() {
        // A dome on its base (smooth: `Painter::arc` is coarse this small).
        let base = at + Vec2::new(0.0, 3.5);
        let dome: Vec<Vec2> = (0..=16)
            .map(|i| PI + PI * i as f32 / 16.0)
            .map(|a| base + Vec2::new(a.cos(), a.sin()) * 7.0)
            .collect();
        p.polyline(&dome, 1.6, color);
        p.line(
            base + Vec2::new(-9.0, 0.0),
            base + Vec2::new(9.0, 0.0),
            1.6,
            color,
        );
    } else if s.kind_name() == "CRYSTAL MINE" {
        p.polyline(
            &[
                at + Vec2::new(0.0, -7.0),
                at + Vec2::new(5.5, 0.0),
                at + Vec2::new(0.0, 7.0),
                at + Vec2::new(-5.5, 0.0),
                at + Vec2::new(0.0, -7.0),
            ],
            1.5,
            color,
        );
    } else {
        p.rect_outline(at - Vec2::splat(5.5), Vec2::splat(11.0), 1.5, color);
    }
}

fn site_marker(p: &mut Painter, at: Vec2, s: &SiteView, selected: bool) {
    let owner = Faction::ALL[s.owner].ui();
    // A downed generator is a ghost of itself.
    let color = if s.down().is_some() {
        mix(BG, owner, 0.4)
    } else {
        owner
    };
    if s.committed {
        dot(p, at, 15.0, mix(BG, theme::HOT, 0.3));
        ring(p, at, 15.0, 2.0, theme::HOT);
    } else {
        dot(p, at, 15.0, BG);
        if s.contested {
            ring(p, at, 15.0, 1.3, mix(BG, theme::HOT, 0.75));
        }
    }
    glyph(p, at, s, color);
    // A generator that's down: its repairs as steps round the marker, those
    // done in its holder's colour (none yet: a wreck).
    if let Some((done, of)) = s.repairs() {
        let n = of.max(1) as usize;
        let gap = 0.35;
        for i in 0..n {
            let a0 = -PI * 0.5 + TAU * i as f32 / n as f32 + gap * 0.5;
            let a1 = a0 + TAU / n as f32 - gap;
            let pts: Vec<Vec2> = (0..=8)
                .map(|k| a0 + (a1 - a0) * k as f32 / 8.0)
                .map(|a| at + Vec2::new(a.cos(), a.sin()) * 19.5)
                .collect();
            if i < done as usize {
                p.polyline(&pts, 2.5, owner);
            } else {
                p.polyline(&pts, 1.0, mix(BG, owner, 0.45));
            }
        }
    }
    if selected {
        widgets::corners(
            p,
            at - Vec2::splat(22.0),
            at + Vec2::splat(22.0),
            7.0,
            theme::FG,
        );
    }
}

/// The key to the marks, in a row along the bottom.
fn legend(p: &mut Painter, at: Vec2, width: f32) {
    let mut style = TextStyle::LABEL;
    style.color = theme::DIM;
    let shield = SiteView::example(true);
    let mine = SiteView::example(false);
    let entries: [(&str, Box<dyn Fn(&mut Painter, Vec2)>); 6] = [
        ("SHIELD", Box::new(|p, c| glyph(p, c, &shield, theme::DIM))),
        (
            "FOUNDRY",
            Box::new(|p, c| {
                p.rect_outline(c - Vec2::splat(5.5), Vec2::splat(11.0), 1.5, theme::DIM)
            }),
        ),
        (
            "CRYSTAL MINE",
            Box::new(|p, c| glyph(p, c, &mine, theme::DIM)),
        ),
        (
            "CONTESTED",
            Box::new(|p, c| ring(p, c, 8.0, 1.2, mix(BG, theme::HOT, 0.75))),
        ),
        (
            "YOUR SIDE FIGHTS HERE",
            Box::new(|p, c| {
                dot(p, c, 8.0, mix(BG, theme::HOT, 0.3));
                ring(p, c, 8.0, 2.0, theme::HOT);
            }),
        ),
        (
            "YOUR SHIP",
            Box::new(|p, c| ship_glyph(p, c + Vec2::new(-1.0, 0.0), 0.36)),
        ),
    ];
    let mut x = at.x;
    for (name, mark) in entries {
        let w = crate::ui::measure(name, &style);
        if x + 25.0 + w > at.x + width {
            break;
        }
        mark(p, Vec2::new(x + 9.0, at.y + 5.0));
        p.text(Vec2::new(x + 25.0, at.y), name, &style, Align::Left);
        x += 25.0 + w + 26.0;
    }
}
