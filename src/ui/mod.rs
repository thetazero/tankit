//! Small immediate-mode drawing kit for the 2D overlay: anti-aliased lines,
//! rects, a stroke font, and a few hairline widgets (see `widgets`). Output is
//! plain `UiVertex` triangles for the existing UI pipeline.
//!
//! Coordinates are logical pixels at a 900 px tall reference screen; `scale`
//! maps them to the real framebuffer. Every primitive can also leave a soft
//! dark halo underneath so thin pale lines stay readable over bright sky.

// Segments, pips and the combat colour are ready for hull and ammo, which the sim doesn't have yet.
#![allow(dead_code)]

pub mod font;
pub mod widgets;

use glam::Vec2;

use crate::render::{NO_UV, UiVertex};

pub type Rgba = [f32; 4];

/// Hairline palette: one pale colour at three strengths, plus orange reserved for combat.
pub mod theme {
    use super::Rgba;
    pub const FG: Rgba = [0.94, 0.91, 0.86, 0.95];
    pub const DIM: Rgba = [0.94, 0.91, 0.86, 0.5];
    pub const FAINT: Rgba = [0.94, 0.91, 0.86, 0.24];
    pub const HOT: Rgba = [1.0, 0.42, 0.16, 1.0];
    /// Repair crates and hull mended by them.
    pub const MEND: Rgba = [0.45, 1.0, 0.58, 1.0];
    pub const HALO: Rgba = [0.0, 0.0, 0.0, 0.4];
    /// Extra halo width on each side, logical px.
    pub const HALO_PX: f32 = 1.5;
}

pub fn with_alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], c[3] * a]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    /// Cap height, logical px.
    pub size: f32,
    /// Extra space after each glyph, logical px.
    pub tracking: f32,
    /// Stroke width, logical px.
    pub weight: f32,
    pub color: Rgba,
}

impl TextStyle {
    /// Small wide-spaced caps for labels.
    pub const LABEL: TextStyle = TextStyle {
        size: 8.0,
        tracking: 4.0,
        weight: 1.0,
        color: theme::DIM,
    };
    /// Readout numbers.
    pub fn value(size: f32) -> TextStyle {
        TextStyle {
            size,
            tracking: size * 0.28,
            weight: 1.2,
            color: theme::FG,
        }
    }
}

/// A clickable rectangle in window pixels, recorded while drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit<T> {
    pub min: Vec2,
    pub max: Vec2,
    pub id: T,
}

/// The clickable areas a component drew this frame, later ones on top.
/// Recording them in `draw`, from the same rects that were drawn, keeps the
/// click targets in step with the layout without repeating it anywhere.
#[derive(Clone, Debug, PartialEq)]
pub struct Hits<T>(Vec<Hit<T>>);

impl<T> Default for Hits<T> {
    fn default() -> Self {
        Hits(Vec::new())
    }
}

impl<T: Copy> Hits<T> {
    pub fn new() -> Hits<T> {
        Hits::default()
    }

    /// Mark a logical-px rect drawn by `p` as `id`.
    pub fn add(&mut self, p: &Painter, min: Vec2, size: Vec2, id: T) {
        self.0.push(Hit {
            min: min * p.scale,
            max: (min + size) * p.scale,
            id,
        });
    }

    /// The topmost area under `at` (window px), and where in it `at` falls (0..1 on each axis).
    pub fn at(&self, at: Vec2) -> Option<(T, Vec2)> {
        self.0
            .iter()
            .rev()
            .find(|h| at.cmpge(h.min).all() && at.cmplt(h.max).all())
            .map(|h| (h.id, (at - h.min) / (h.max - h.min).max(Vec2::splat(1e-3))))
    }

    /// The centre of the topmost area marked `id`, in window px.
    pub fn center_of(&self, id: T) -> Option<Vec2>
    where
        T: PartialEq,
    {
        let h = self.0.iter().rev().find(|h| h.id == id)?;
        Some((h.min + h.max) * 0.5)
    }

    pub fn map<U>(self, f: impl Fn(T) -> U) -> Hits<U> {
        Hits(
            self.0
                .into_iter()
                .map(|h| Hit {
                    min: h.min,
                    max: h.max,
                    id: f(h.id),
                })
                .collect(),
        )
    }

    /// `other`'s areas, on top of these.
    pub fn extend(&mut self, other: Hits<T>) {
        self.0.extend(other.0);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// What a component drew: its triangles and where it can be clicked.
pub struct Layer<T> {
    pub verts: Vec<UiVertex>,
    pub hits: Hits<T>,
}

impl<T> Layer<T> {
    pub fn empty() -> Layer<T> {
        Layer {
            verts: Vec::new(),
            hits: Hits::default(),
        }
    }
}

pub struct Painter {
    pub scale: f32,
    halo: Option<(Rgba, f32)>,
    clip: Option<(Vec2, Vec2)>,
    verts: Vec<UiVertex>,
    under: Vec<UiVertex>,
}

impl Painter {
    pub fn new(scale: f32) -> Painter {
        Painter {
            scale,
            halo: Some((theme::HALO, theme::HALO_PX)),
            clip: None,
            verts: Vec::new(),
            under: Vec::new(),
        }
    }

    pub fn set_halo(&mut self, halo: Option<(Rgba, f32)>) {
        self.halo = halo;
    }

    /// Restrict drawing to a logical-px rectangle (min, max): lines, rects
    /// (so text too) and images. Gradients ignore it.
    pub fn set_clip(&mut self, clip: Option<(Vec2, Vec2)>) {
        self.clip = clip;
    }

    /// Halo layer first, then strokes, so no halo darkens a neighbouring stroke.
    pub fn finish(mut self) -> Vec<UiVertex> {
        self.under.append(&mut self.verts);
        self.under
    }

    pub fn line(&mut self, a: Vec2, b: Vec2, width: f32, color: Rgba) {
        let Some((a, b)) = clip_segment(a, b, self.clip) else {
            return;
        };
        let s = self.scale;
        let w = width * s;
        if let Some((hc, hw)) = self.halo {
            push_line(
                &mut self.under,
                a * s,
                b * s,
                w + 2.0 * hw * s,
                with_alpha(hc, color[3].min(1.0)),
            );
        }
        push_line(&mut self.verts, a * s, b * s, w, color);
    }

    pub fn polyline(&mut self, pts: &[Vec2], width: f32, color: Rgba) {
        for w in pts.windows(2) {
            self.line(w[0], w[1], width, color);
        }
    }

    /// Arc around `c`, angles in radians (0 = +x, y down so positive is clockwise on screen).
    pub fn arc(&mut self, c: Vec2, r: f32, a0: f32, a1: f32, width: f32, color: Rgba) {
        let n = (((a1 - a0).abs() * r / 4.0).ceil() as usize).clamp(4, 96);
        let pts: Vec<Vec2> = (0..=n)
            .map(|i| a0 + (a1 - a0) * i as f32 / n as f32)
            .map(|a| c + Vec2::new(a.cos(), a.sin()) * r)
            .collect();
        self.polyline(&pts, width, color);
    }

    pub fn rect(&mut self, min: Vec2, size: Vec2, color: Rgba) {
        let Some((min, size)) = clip_rect(min, size, self.clip) else {
            return;
        };
        let s = self.scale;
        if let Some((hc, hw)) = self.halo {
            let h = hw * s;
            push_rect(
                &mut self.under,
                min * s - h,
                size * s + 2.0 * h,
                with_alpha(hc, color[3].min(1.0)),
            );
        }
        push_rect(&mut self.verts, min * s, size * s, color);
    }

    /// A rect showing the thumbnail atlas between `uv_min` and `uv_max`,
    /// tinted by `tint`, no halo. Clipped (and its UVs with it) like a rect.
    pub fn image(&mut self, min: Vec2, size: Vec2, uv_min: Vec2, uv_max: Vec2, tint: Rgba) {
        let Some((cmin, csize)) = clip_rect(min, size, self.clip) else {
            return;
        };
        let uv_at = |p: Vec2| uv_min + (uv_max - uv_min) * ((p - min) / size);
        let (u0, u1) = (uv_at(cmin), uv_at(cmin + csize));
        let s = self.scale;
        let (a, b) = (cmin * s, (cmin + csize) * s);
        let v = |x: f32, y: f32, u: f32, w: f32| UiVertex {
            pos: [x, y],
            color: tint,
            uv: [u, w],
        };
        self.verts.extend_from_slice(&[
            v(a.x, a.y, u0.x, u0.y),
            v(b.x, a.y, u1.x, u0.y),
            v(b.x, b.y, u1.x, u1.y),
            v(a.x, a.y, u0.x, u0.y),
            v(b.x, b.y, u1.x, u1.y),
            v(a.x, b.y, u0.x, u1.y),
        ]);
    }

    /// Rect fading horizontally from `left` to `right` colour, no halo (for backdrops).
    pub fn gradient_h(&mut self, min: Vec2, size: Vec2, left: Rgba, right: Rgba) {
        let s = self.scale;
        let (x0, y0, x1, y1) = (
            min.x * s,
            min.y * s,
            (min.x + size.x) * s,
            (min.y + size.y) * s,
        );
        let v = |x: f32, y: f32, color: Rgba| UiVertex {
            pos: [x, y],
            color,
            uv: NO_UV,
        };
        self.under.extend_from_slice(&[
            v(x0, y0, left),
            v(x1, y0, right),
            v(x1, y1, right),
            v(x0, y0, left),
            v(x1, y1, right),
            v(x0, y1, left),
        ]);
    }

    pub fn rect_outline(&mut self, min: Vec2, size: Vec2, width: f32, color: Rgba) {
        let max = min + size;
        self.polyline(
            &[
                min,
                Vec2::new(max.x, min.y),
                max,
                Vec2::new(min.x, max.y),
                min,
            ],
            width,
            color,
        );
    }

    /// Draws `text` with its top edge at `pos.y`, aligned around `pos.x`. Returns its width.
    pub fn text(&mut self, pos: Vec2, text: &str, style: &TextStyle, align: Align) -> f32 {
        let w = measure(text, style);
        let mut x = match align {
            Align::Left => pos.x,
            Align::Center => pos.x - w * 0.5,
            Align::Right => pos.x - w,
        };
        let u = style.size / font::HEIGHT;
        for c in text.chars() {
            let Some(g) = font::glyph(c) else {
                x += 3.0 * u + style.tracking;
                continue;
            };
            for stroke in &g.strokes {
                let pts: Vec<Vec2> = stroke
                    .iter()
                    .map(|p| Vec2::new(x, pos.y) + *p * u)
                    .collect();
                if pts.len() == 2 && pts[0].distance(pts[1]) < style.weight {
                    // Dots: a tiny square reads better than a stub of line.
                    let d = style.weight.max(1.2);
                    self.rect(
                        (pts[0] + pts[1]) * 0.5 - d * 0.5,
                        Vec2::splat(d),
                        style.color,
                    );
                } else {
                    self.polyline(&pts, style.weight, style.color);
                }
            }
            x += g.width * u + style.tracking;
        }
        w
    }
}

/// Width of `text` in logical px (no trailing tracking).
pub fn measure(text: &str, style: &TextStyle) -> f32 {
    let u = style.size / font::HEIGHT;
    let mut w = 0.0;
    let mut n = 0;
    for c in text.chars() {
        w += font::glyph(c).map_or(3.0, |g| g.width) * u + style.tracking;
        n += 1;
    }
    if n > 0 { w - style.tracking } else { 0.0 }
}

/// The part of a rect inside the clip box, if any.
fn clip_rect(min: Vec2, size: Vec2, clip: Option<(Vec2, Vec2)>) -> Option<(Vec2, Vec2)> {
    let Some((cmin, cmax)) = clip else {
        return Some((min, size));
    };
    let (a, b) = (min.max(cmin), (min + size).min(cmax));
    (b.x > a.x && b.y > a.y).then(|| (a, b - a))
}

/// Liang–Barsky: the part of segment a→b inside the box, if any.
pub fn clip_segment(a: Vec2, b: Vec2, clip: Option<(Vec2, Vec2)>) -> Option<(Vec2, Vec2)> {
    let Some((min, max)) = clip else {
        return Some((a, b));
    };
    let d = b - a;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [
        (-d.x, a.x - min.x),
        (d.x, max.x - a.x),
        (-d.y, a.y - min.y),
        (d.y, max.y - a.y),
    ] {
        if p.abs() < 1e-9 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 > t1 {
            return None;
        }
    }
    Some((a + d * t0, a + d * t1))
}

fn push_rect(out: &mut Vec<UiVertex>, min: Vec2, size: Vec2, color: Rgba) {
    let v = |x: f32, y: f32| UiVertex {
        pos: [x, y],
        color,
        uv: NO_UV,
    };
    let (x0, y0, x1, y1) = (min.x, min.y, min.x + size.x, min.y + size.y);
    out.extend_from_slice(&[
        v(x0, y0),
        v(x1, y0),
        v(x1, y1),
        v(x0, y0),
        v(x1, y1),
        v(x0, y1),
    ]);
}

/// A line as three strips across its width: solid core with a 1 px alpha
/// ramp on each side. Lines thinner than 1 px fade out instead of thinning.
fn push_line(out: &mut Vec<UiVertex>, a: Vec2, b: Vec2, width: f32, color: Rgba) {
    let d = b - a;
    let len = d.length();
    let dir = if len > 1e-4 { d / len } else { Vec2::X };
    let n = dir.perp();
    let cover = width.min(1.0);
    let w = width.max(1.0);
    let inner = (w * 0.5 - 0.5).max(0.0);
    let outer = w * 0.5 + 0.5;
    // Square caps, so polylines join without gaps.
    let (a, b) = (a - dir * (w * 0.5), b + dir * (w * 0.5));
    let solid = with_alpha(color, cover);
    let clear = with_alpha(color, 0.0);
    let rows = [
        (-outer, clear),
        (-inner, solid),
        (inner, solid),
        (outer, clear),
    ];
    for pair in rows.windows(2) {
        let ((o0, c0), (o1, c1)) = (pair[0], pair[1]);
        let v = |p: Vec2, c: Rgba| UiVertex {
            pos: p.into(),
            color: c,
            uv: NO_UV,
        };
        let (p00, p01, p10, p11) = (a + n * o0, b + n * o0, a + n * o1, b + n * o1);
        out.extend_from_slice(&[
            v(p00, c0),
            v(p01, c0),
            v(p11, c1),
            v(p00, c0),
            v(p11, c1),
            v(p10, c1),
        ]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_is_three_strips_plus_halo() {
        let mut p = Painter::new(1.0);
        p.line(Vec2::ZERO, Vec2::new(10.0, 0.0), 1.0, theme::FG);
        assert_eq!(p.finish().len(), 36);
        let mut p = Painter::new(1.0);
        p.set_halo(None);
        p.line(Vec2::ZERO, Vec2::new(10.0, 0.0), 1.0, theme::FG);
        assert_eq!(p.finish().len(), 18);
    }

    #[test]
    fn halo_is_drawn_under_strokes() {
        let mut p = Painter::new(1.0);
        p.line(Vec2::ZERO, Vec2::new(10.0, 0.0), 1.0, theme::FG);
        let v = p.finish();
        assert_eq!(v[0].color[0], theme::HALO[0]);
        assert_eq!(v[18 + 1].color[0], theme::FG[0]);
    }

    #[test]
    fn hits_are_in_window_pixels_topmost_first() {
        let p = Painter::new(2.0);
        let mut hits = Hits::new();
        hits.add(&p, Vec2::ZERO, Vec2::splat(10.0), 1);
        hits.add(&p, Vec2::splat(5.0), Vec2::splat(10.0), 2);
        assert_eq!(hits.at(Vec2::new(4.0, 4.0)), Some((1, Vec2::splat(0.2))));
        // Overlap: the later one wins.
        assert_eq!(hits.at(Vec2::splat(15.0)).map(|h| h.0), Some(2));
        assert_eq!(hits.at(Vec2::splat(31.0)), None);
        assert_eq!(hits.map(|i| i * 10).at(Vec2::ONE).map(|h| h.0), Some(10));
    }

    #[test]
    fn clipping() {
        let clip = Some((Vec2::ZERO, Vec2::splat(10.0)));
        let (a, b) = clip_segment(Vec2::new(-5.0, 5.0), Vec2::new(15.0, 5.0), clip).unwrap();
        assert!(
            (a - Vec2::new(0.0, 5.0)).length() < 1e-4 && (b - Vec2::new(10.0, 5.0)).length() < 1e-4
        );
        assert!(clip_segment(Vec2::new(-5.0, -1.0), Vec2::new(15.0, -1.0), clip).is_none());
        assert!(clip_segment(Vec2::new(5.0, 20.0), Vec2::new(5.0, 30.0), clip).is_none());
        let inside = (Vec2::new(1.0, 1.0), Vec2::new(2.0, 3.0));
        assert_eq!(clip_segment(inside.0, inside.1, clip), Some(inside));
    }

    #[test]
    fn measure_matches_drawn_advance() {
        let s = TextStyle::value(12.0);
        // "10": widths 4 + 4 grid units at 2 px each, plus one tracking gap.
        assert!((measure("10", &s) - (16.0 + s.tracking)).abs() < 1e-4);
        assert_eq!(measure("", &s), 0.0);
        let mut p = Painter::new(1.0);
        assert_eq!(
            p.text(Vec2::ZERO, "10", &s, Align::Right),
            measure("10", &s)
        );
    }
}
