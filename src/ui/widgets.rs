//! Hairline widgets. Each takes a `Painter` and logical-px positions and
//! keeps no state; anything that animates (eased values, rolling digits)
//! gets its value from the caller.

use std::f32::consts::{FRAC_PI_2, TAU};

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
            p.rect(min, size, theme::FG);
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
    let row = style.size * 1.6;
    let pad = style.size * 0.3;
    let lead_end = (value.max(0.0).floor() as u64).to_string().len() as u32;
    for i in 0..digits {
        let place = digits - 1 - i;
        let (d, roll) = digit_roll(value, place);
        let x = x0 + i as f32 * advance;
        let mut s = *style;
        if place >= lead_end.max(1) {
            s.color = theme::FAINT;
        }
        p.set_clip(Some((
            Vec2::new(x - pad, pos.y - pad),
            Vec2::new(x + advance, pos.y + style.size + pad),
        )));
        p.text(
            Vec2::new(x, pos.y - roll * row),
            &d.to_string(),
            &s,
            Align::Left,
        );
        if roll > 0.0 {
            p.text(
                Vec2::new(x, pos.y + (1.0 - roll) * row),
                &((d + 1) % 10).to_string(),
                &s,
                Align::Left,
            );
        }
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

/// Heading strip: ticks every 5°, labels every 30°, a caret and the heading
/// readout underneath. Fades out toward the ends. `heading` in degrees, 0 = N.
pub fn compass(p: &mut Painter, center: Vec2, width: f32, heading: f32) {
    let span = 120.0; // degrees visible across the strip
    let px_per_deg = width / span;
    let half = width * 0.5;
    let first = ((heading - span * 0.5) / 5.0).ceil() as i32 * 5;
    let mut deg = first;
    while (deg as f32) <= heading + span * 0.5 {
        let x = center.x + (deg as f32 - heading) * px_per_deg;
        let fade = (1.0 - ((x - center.x).abs() / half))
            .clamp(0.0, 1.0)
            .powf(0.7);
        let norm = deg.rem_euclid(360);
        let (h, c) = if norm % 30 == 0 {
            (8.0, theme::DIM)
        } else {
            (4.0, theme::FAINT)
        };
        p.line(
            Vec2::new(x, center.y),
            Vec2::new(x, center.y + h),
            1.0,
            with_alpha(c, fade),
        );
        if norm % 30 == 0 {
            let text = match norm {
                0 => "N".to_string(),
                90 => "E".to_string(),
                180 => "S".to_string(),
                270 => "W".to_string(),
                n => format!("{n:03}"),
            };
            let mut s = TextStyle::LABEL;
            s.tracking = 0.8;
            s.color = with_alpha(
                if norm % 90 == 0 {
                    theme::FG
                } else {
                    theme::DIM
                },
                fade,
            );
            p.text(Vec2::new(x, center.y + 13.0), &text, &s, Align::Center);
        }
        deg += 5;
    }
    p.line(
        Vec2::new(center.x - half, center.y),
        Vec2::new(center.x + half, center.y),
        1.0,
        theme::FAINT,
    );
    // Caret above the strip.
    p.polyline(
        &[
            center + Vec2::new(-4.0, -7.0),
            center + Vec2::new(0.0, -2.0),
            center + Vec2::new(4.0, -7.0),
        ],
        1.2,
        theme::FG,
    );
}

/// Top-down hull outline with the turret and barrel at `turret_rel` radians
/// (0 = straight ahead, positive = turned left), hull nose pointing up.
pub fn turret_gauge(p: &mut Painter, c: Vec2, turret_rel: f32) {
    let (hw, hh) = (13.0, 22.0);
    p.polyline(
        &[
            c + Vec2::new(-hw, -hh + 4.0),
            c + Vec2::new(-hw + 4.0, -hh),
            c + Vec2::new(hw - 4.0, -hh),
            c + Vec2::new(hw, -hh + 4.0),
            c + Vec2::new(hw, hh),
            c + Vec2::new(-hw, hh),
            c + Vec2::new(-hw, -hh + 4.0),
        ],
        1.0,
        theme::DIM,
    );
    let tc = c + Vec2::new(0.0, 2.0);
    p.arc(tc, 8.0, 0.0, TAU, 1.2, theme::FG);
    // Screen angle: up is -y; turning left (positive) rotates counter-clockwise on screen.
    let a = -FRAC_PI_2 - turret_rel;
    let dir = Vec2::new(a.cos(), a.sin());
    p.line(tc + dir * 8.0, tc + dir * 34.0, 1.6, theme::FG);
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
    fn widgets_emit_geometry() {
        let mut p = Painter::new(1.0);
        compass(&mut p, Vec2::new(400.0, 40.0), 360.0, 359.0);
        turret_gauge(&mut p, Vec2::new(100.0, 100.0), 0.5);
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
