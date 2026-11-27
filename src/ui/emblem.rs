//! Each faction's emblem, drawn in hairlines like the rest of the UI, and
//! the emblem-then-name lockup used wherever a faction is named on screen.

use std::f32::consts::{PI, TAU};

use glam::Vec2;

use super::{Align, Painter, Rgba, TextStyle, measure};
use crate::faction::Faction;

/// Emblem size for text of this style: a little taller than the caps.
fn emblem_size(style: &TextStyle) -> f32 {
    (style.size * 1.6).max(11.0)
}

fn gap(style: &TextStyle) -> f32 {
    style.tracking.max(style.size * 0.6)
}

/// An arc smooth at any size (`Painter::arc` is coarse below ~10 px).
pub(super) fn arc(p: &mut Painter, c: Vec2, r: f32, a0: f32, a1: f32, width: f32, color: Rgba) {
    let n = 24;
    let pts: Vec<Vec2> = (0..=n)
        .map(|i| a0 + (a1 - a0) * i as f32 / n as f32)
        .map(|a| c + Vec2::new(a.cos(), a.sin()) * r)
        .collect();
    p.polyline(&pts, width, color);
}

/// A filled disc: one ring as wide as its radius.
pub(super) fn disc(p: &mut Painter, c: Vec2, r: f32, color: Rgba) {
    arc(p, c, r * 0.5, 0.0, TAU, r, color);
}

/// `f`'s emblem centred on `c`, `size` logical px across, strokes `weight` wide.
pub fn emblem(p: &mut Painter, f: Faction, c: Vec2, size: f32, weight: f32, color: Rgba) {
    let u = size * 0.5;
    let at = |x: f32, y: f32| c + Vec2::new(x, y) * u;
    match f {
        // A planet under a halo: a disc with an arch over it.
        Faction::Imperium => {
            let planet = at(0.0, 0.3);
            disc(p, planet, 0.48 * u, color);
            arc(p, planet, 0.86 * u, PI + 0.3, TAU - 0.3, weight, color);
        }
        // A crystal in a pointed-arch window, like a relic in its case.
        Faction::Resonant => {
            let (w, spring, base): (f32, f32, f32) = (0.62, 0.05, 1.0);
            let r = 2.0 * w;
            // Each jamb and the arc rising from it, centred on the far springer.
            let a_top = PI + (w / r).acos();
            let mut left: Vec<Vec2> = vec![at(-w, base), at(-w, spring)];
            let mut right: Vec<Vec2> = vec![at(w, base), at(w, spring)];
            let n = 10;
            for i in 1..=n {
                let a = PI + (a_top - PI) * i as f32 / n as f32;
                left.push(at(w + r * a.cos(), spring + r * a.sin()));
                let b = TAU - (a - PI);
                right.push(at(-w + r * b.cos(), spring + r * b.sin()));
            }
            p.polyline(&left, weight, color);
            p.polyline(&right, weight, color);
            p.line(at(-w, base), at(w, base), weight, color);
            let gem = [
                at(0.0, -0.5),
                at(0.26, -0.18),
                at(0.26, 0.42),
                at(0.0, 0.72),
                at(-0.26, 0.42),
                at(-0.26, -0.18),
                at(0.0, -0.5),
            ];
            p.polyline(&gem, weight, color);
            p.line(at(0.0, -0.5), at(0.0, 0.72), weight * 0.8, color);
        }
        // A tungsten cube, the thing they make.
        Faction::Uthenn => {
            let v = |k: i32| {
                let a = PI / 6.0 + k as f32 * PI / 3.0;
                at(a.cos() * 0.95, a.sin() * 0.95)
            };
            let hex: Vec<Vec2> = (0..=6).map(v).collect();
            p.polyline(&hex, weight, color);
            // Vertices: 1 bottom, 3 upper left, 5 upper right; the near corner in the middle.
            let mid = at(0.0, 0.0);
            for k in [1, 3, 5] {
                p.line(mid, v(k), weight, color);
            }
        }
        // Plasma held in crystal: an orb in an almond-shaped lens.
        Faction::Ithessar => {
            let (h, half): (f32, f32) = (0.58, 1.0);
            let off = (half * half - h * h) / (2.0 * h);
            let r = off + h;
            let a = (off / half).atan();
            arc(p, at(0.0, off), r * u, PI + a, TAU - a, weight, color);
            arc(p, at(0.0, -off), r * u, a, PI - a, weight, color);
            disc(p, at(0.0, 0.0), 0.3 * u, color);
        }
        // The Company: a blade through a coin. Paid work, done with a weapon.
        Faction::Company => {
            arc(p, at(0.0, 0.1), 0.62 * u, 0.0, TAU, weight, color);
            // The blade, point down through the coin, its hilt above it.
            p.polyline(
                &[
                    at(-0.13, -0.55),
                    at(-0.13, 0.62),
                    at(0.0, 0.98),
                    at(0.13, 0.62),
                    at(0.13, -0.55),
                ],
                weight,
                color,
            );
            p.line(at(-0.42, -0.55), at(0.42, -0.55), weight, color);
            p.line(at(0.0, -0.55), at(0.0, -0.95), weight, color);
        }
    }
}

/// Width of `tagged` with this text and style.
pub fn tagged_width(text: &str, style: &TextStyle) -> f32 {
    mark_width(style) + measure(text, style)
}

/// The faction's emblem, then its name, laid out like `Painter::text`.
pub fn faction_name(
    p: &mut Painter,
    pos: Vec2,
    f: Faction,
    style: &TextStyle,
    align: Align,
) -> f32 {
    tagged(p, pos, f, f.name(), style, align)
}

/// The faction's emblem, then `text`, laid out like `Painter::text`: top of
/// the caps at `pos.y`, the whole lockup aligned around `pos.x`. Returns its width.
pub fn tagged(
    p: &mut Painter,
    pos: Vec2,
    f: Faction,
    text: &str,
    style: &TextStyle,
    align: Align,
) -> f32 {
    let w = tagged_width(text, style);
    let x = match align {
        Align::Left => pos.x,
        Align::Center => pos.x - w * 0.5,
        Align::Right => pos.x - w,
    };
    let x = x + mark(p, Vec2::new(x, pos.y), f, style);
    p.text(Vec2::new(x, pos.y), text, style, Align::Left);
    w
}

/// Just the emblem, sized and placed as if it were the first glyph of text
/// in `style` at `pos`. Returns how far the text after it should start.
pub fn mark(p: &mut Painter, pos: Vec2, f: Faction, style: &TextStyle) -> f32 {
    let size = emblem_size(style);
    let c = Vec2::new(pos.x + size * 0.5, pos.y + style.size * 0.5);
    emblem(p, f, c, size, style.weight.max(1.0), style.color);
    size + gap(style)
}

/// What `mark` advances by.
pub fn mark_width(style: &TextStyle) -> f32 {
    emblem_size(style) + gap(style)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lockup_width_matches_drawn() {
        for f in Faction::ALL.into_iter().chain([Faction::Company]) {
            let s = TextStyle::value(11.0);
            let mut p = Painter::new(1.0);
            let w = faction_name(&mut p, Vec2::ZERO, f, &s, Align::Right);
            assert_eq!(w, tagged_width(f.name(), &s));
            assert!(!p.finish().is_empty());
        }
    }
}
