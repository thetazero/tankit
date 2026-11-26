//! Hairline widgets. Each takes a `Painter` and logical-px positions and
//! keeps no state; anything that animates (eased values, rolling digits)
//! gets its value from the caller.

use glam::Vec2;

use super::{Align, Painter, Rgba, TextStyle, theme, with_alpha};

/// Faint L-brackets at the four corners of a box.
pub fn corners(p: &mut Painter, min: Vec2, max: Vec2, len: f32, color: Rgba) {
    let (x0, y0, x1, y1) = (min.x, min.y, max.x, max.y);
    for (c, dx, dy) in [
        (Vec2::new(x0, y0), 1.0, 1.0),
        (Vec2::new(x1, y0), -1.0, 1.0),
        (Vec2::new(x0, y1), 1.0, -1.0),
        (Vec2::new(x1, y1), -1.0, -1.0),
    ] {
        p.polyline(
            &[
                c + Vec2::new(0.0, dy * len),
                c,
                c + Vec2::new(dx * len, 0.0),
            ],
            1.0,
            color,
        );
    }
}

pub fn label(p: &mut Painter, pos: Vec2, text: &str, align: Align) -> f32 {
    p.text(pos, text, &TextStyle::LABEL, align)
}

/// Height of a `back_button`.
pub const BACK_H: f32 = 30.0;

/// Width of a `back_button`.
pub fn back_width() -> f32 {
    BACK_PAD * 2.0 + BACK_CHEVRON + super::measure("BACK", &TextStyle::LABEL)
}

const BACK_PAD: f32 = 12.0;
/// The chevron's width and the gap after it.
const BACK_CHEVRON: f32 = 14.0;

/// The way back off a screen, for the pointer as Esc is for the keys: a
/// hairline box saying BACK behind a left chevron, filled in when `lit`
/// (under the pointer). `min` is its top left; it's `back_width()` by
/// `BACK_H`, and the caller records the click.
pub fn back_button(p: &mut Painter, min: Vec2, lit: bool) {
    let size = Vec2::new(back_width(), BACK_H);
    p.rect(
        min,
        size,
        with_alpha(theme::FG, if lit { 0.16 } else { 0.07 }),
    );
    p.rect_outline(min, size, 1.0, theme::FG);
    let style = TextStyle {
        color: theme::FG,
        ..TextStyle::LABEL
    };
    let ty = min.y + (BACK_H - style.size) * 0.5;
    let (cx, cy) = (min.x + BACK_PAD, min.y + BACK_H * 0.5);
    p.polyline(
        &[
            Vec2::new(cx + 4.5, cy - 4.5),
            Vec2::new(cx, cy),
            Vec2::new(cx + 4.5, cy + 4.5),
        ],
        1.2,
        theme::FG,
    );
    p.text(
        Vec2::new(cx + BACK_CHEVRON, ty),
        "BACK",
        &style,
        Align::Left,
    );
}

/// A square hairline box, filled in when `on`. `pos` is its top left.
pub fn checkbox(p: &mut Painter, pos: Vec2, size: f32, on: bool) {
    p.rect_outline(
        pos,
        Vec2::splat(size),
        1.0,
        if on { theme::FG } else { theme::DIM },
    );
    if on {
        p.rect(pos + Vec2::splat(3.0), Vec2::splat(size - 6.0), theme::FG);
    }
}

/// A 1 px track with a brighter 2 px fill and an end tick. `frac` in 0..1.
pub fn hairbar(p: &mut Painter, pos: Vec2, width: f32, frac: f32, fill: Rgba) {
    let end = pos + Vec2::new(width, 0.0);
    p.line(pos, end, 1.0, theme::DIM);
    p.line(
        end - Vec2::new(0.0, 4.0),
        end + Vec2::new(0.0, 4.0),
        1.0,
        theme::DIM,
    );
    let f = frac.clamp(0.0, 1.0);
    if f > 0.0 {
        p.line(pos, pos + Vec2::new(width * f, 0.0), 2.0, fill);
    }
}

/// A row of outlined segments, the first `lit` filled. `hot` marks one segment in the combat colour.
pub fn segments(
    p: &mut Painter,
    pos: Vec2,
    width: f32,
    count: usize,
    lit: usize,
    hot: Option<usize>,
) {
    let gap = 3.0;
    let w = (width - gap * (count as f32 - 1.0)) / count as f32;
    for i in 0..count {
        let min = pos + Vec2::new(i as f32 * (w + gap), 0.0);
        let size = Vec2::new(w, 4.0);
        if hot == Some(i) {
            p.rect(min, size, theme::HOT);
        } else if i < lit {
            p.rect(min, size, super::design::TEXT);
        } else {
            p.rect_outline(min, size, 1.0, theme::DIM);
        }
    }
}

/// Small square pips, the first `lit` filled.
pub fn pips(p: &mut Painter, pos: Vec2, count: usize, lit: usize) {
    for i in 0..count {
        let min = pos + Vec2::new(i as f32 * 10.0, 0.0);
        if i < lit {
            p.rect(min, Vec2::splat(6.0), theme::FG);
        } else {
            p.rect_outline(min, Vec2::splat(6.0), 1.0, theme::DIM);
        }
    }
}

/// Share of each step spent rolling; the rest of the time the digit sits still and readable.
const ROLL: f32 = 0.2;

/// Which digit shows in decimal `place` (0 = ones) and how far it has rolled
/// toward the next one (0..1), odometer-style: digits hold still and roll over
/// in the last `ROLL` of each step, and a place only moves while every place
/// below it is rolling over from 9.
pub fn digit_roll(value: f32, place: u32) -> (u32, f32) {
    let v = value.max(0.0);
    let unit = 10f32.powi(place as i32);
    let digit = (v / unit).floor() as u32 % 10;
    let roll = ((v % unit - (unit - ROLL)) / ROLL).clamp(0.0, 1.0);
    (digit, roll)
}

/// Fixed-width number whose digits roll like a mechanical counter. Leading
/// zeros are drawn faint. `value` may be fractional; feed it an eased value.
pub fn odometer(
    p: &mut Painter,
    pos: Vec2,
    value: f32,
    digits: u32,
    style: &TextStyle,
    align: Align,
) -> f32 {
    let advance = super::measure("0", style) + style.tracking;
    let total = advance * digits as f32 - style.tracking;
    let x0 = match align {
        Align::Left => pos.x,
        Align::Center => pos.x - total * 0.5,
        Align::Right => pos.x - total,
    };
    // A rolling digit is cut to the number's own band: just room for the halo
    // and the round digits' overshoot, so nothing spills above or below.
    let pad = theme::HALO_PX + style.size * 0.04;
    // Digits on the wheel sit one band apart, so mid-roll shows a readable
    // half of each rather than two slivers across an empty gap.
    let row = style.size + 2.0 * pad + style.size * 0.1;
    let lead_end = (value.max(0.0).floor() as u64).to_string().len() as u32;
    for i in 0..digits {
        let place = digits - 1 - i;
        let (d, roll) = digit_roll(value, place);
        let x = x0 + i as f32 * advance;
        let mut s = *style;
        if place >= lead_end.max(1) {
            s.color = theme::FAINT;
        }
        if roll == 0.0 {
            // At rest: drawn whole, like any other text.
            p.text(Vec2::new(x, pos.y), &d.to_string(), &s, Align::Left);
            continue;
        }
        p.set_clip(Some((
            Vec2::new(x - pad, pos.y - pad),
            Vec2::new(x + advance + pad, pos.y + style.size + pad),
        )));
        p.text(
            Vec2::new(x, pos.y - roll * row),
            &d.to_string(),
            &s,
            Align::Left,
        );
        p.text(
            Vec2::new(x, pos.y + (1.0 - roll) * row),
            &((d + 1) % 10).to_string(),
            &s,
            Align::Left,
        );
        p.set_clip(None);
    }
    total
}

/// Four hairlines around a centre dot. `kick` (0..1) opens the gap, e.g. right after firing.
pub fn crosshair(p: &mut Painter, c: Vec2, kick: f32, color: Rgba) {
    let gap = 7.0 + kick * 9.0;
    let len = 10.0;
    for d in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
        p.line(c + d * gap, c + d * (gap + len), 1.2, color);
    }
    p.rect(c - 1.0, Vec2::splat(2.0), color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odometer_digits_roll_over_like_a_counter() {
        assert_eq!(digit_roll(42.0, 0), (2, 0.0));
        assert_eq!(digit_roll(42.0, 1), (4, 0.0));
        // Holds still for most of the step, then rolls.
        assert_eq!(digit_roll(42.5, 0), (2, 0.0));
        let (d, r) = digit_roll(42.9, 0);
        assert_eq!(d, 2);
        assert!((r - 0.5).abs() < 1e-3);
        // Tens only move while the ones roll 9 -> 0.
        assert_eq!(digit_roll(45.9, 1).1, 0.0);
        let (d, r) = digit_roll(49.9, 1);
        assert_eq!(d, 4);
        assert!((r - 0.5).abs() < 1e-3);
        let (d, r) = digit_roll(99.9, 2);
        assert_eq!(d, 0);
        assert!((r - 0.5).abs() < 1e-3);
    }

    #[test]
    fn rolling_digit_stays_in_its_band() {
        let style = TextStyle::value(22.0);
        let (top, size) = (100.0, style.size);
        let pad = theme::HALO_PX + size * 0.04;
        for i in 1..20 {
            let mut p = Painter::new(1.0);
            let value = 6.8 + i as f32 * 0.01;
            assert!(digit_roll(value, 0).1 > 0.0);
            odometer(&mut p, Vec2::new(0.0, top), value, 1, &style, Align::Left);
            for v in p.finish() {
                let y = v.pos[1];
                assert!(
                    y >= top - pad - 1e-3 && y <= top + size + pad + 1e-3,
                    "{value}: vertex at y={y} outside {top}..{}",
                    top + size
                );
            }
        }
    }

    #[test]
    fn widgets_emit_geometry() {
        let mut p = Painter::new(1.0);
        odometer(
            &mut p,
            Vec2::ZERO,
            49.5,
            3,
            &TextStyle::value(20.0),
            Align::Left,
        );
        assert!(!p.finish().is_empty());
    }
}
