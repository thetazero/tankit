//! Small immediate-mode drawing kit for the 2D overlay: anti-aliased lines,
//! rects, text (see `font`), and a few hairline widgets (see `widgets`). Output is
//! plain `UiVertex` triangles for the existing UI pipeline.
//!
//! Coordinates are logical pixels at a 900 px tall reference screen; `scale`
//! maps them to the real framebuffer. Every primitive can also leave a soft
//! dark halo underneath so thin pale lines stay readable over bright sky.

// Segments, pips and the combat colour are ready for hull and ammo, which the sim doesn't have yet.
#![allow(dead_code)]

pub mod emblem;
pub mod font;
pub mod widgets;

use glam::Vec2;

use crate::render::{NO_SDF, NO_UV, UiVertex};

pub type Rgba = [f32; 4];

/// Hairline palette: one pale colour at three strengths, plus orange reserved for combat.
pub mod theme {
    use super::Rgba;
    pub const FG: Rgba = [0.94, 0.91, 0.86, 0.95];
    pub const DIM: Rgba = [0.94, 0.91, 0.86, 0.6];
    pub const FAINT: Rgba = [0.94, 0.91, 0.86, 0.24];
    pub const HOT: Rgba = [1.0, 0.42, 0.16, 1.0];
    /// Repair crates and hull mended by them.
    pub const MEND: Rgba = [0.45, 1.0, 0.58, 1.0];
    pub const HALO: Rgba = [0.0, 0.0, 0.0, 0.4];
    /// Fill behind boxes of text (scoreboard, pause, settings): dark and
    /// opaque, so the battle behind never competes with what's written on it.
    /// The UI blends in linear light, where even 6% of bright sand behind a
    /// near-black fill lifts it to a mid grey with the scene plainly showing
    /// through, so anything short of 1.0 reads as see-through.
    pub const PANEL: Rgba = [0.03, 0.03, 0.035, 1.0];
    /// Extra halo width on each side, logical px.
    pub const HALO_PX: f32 = 1.5;
    /// Text's halo: this dark at full strength, and full strength once the
    /// text is `1 / TEXT_HALO_GAIN` opaque, so even dim labels get a solid rim.
    pub const TEXT_HALO_ALPHA: f32 = 0.8;
    pub const TEXT_HALO_GAIN: f32 = 1.8;
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
    /// Boldness: 1.0 is the font's own weight, each 0.1 above thickens strokes by 0.05 px.
    pub weight: f32,
    pub color: Rgba,
    /// Keep lowercase letters (sentence-case body text); otherwise all caps.
    pub lower: bool,
}

impl TextStyle {
    /// Small letter-spaced caps for labels.
    pub const LABEL: TextStyle = TextStyle {
        size: 8.5,
        tracking: 1.6,
        weight: 1.0,
        color: theme::DIM,
        lower: false,
    };
    /// Readout numbers.
    pub fn value(size: f32) -> TextStyle {
        TextStyle {
            size,
            tracking: size * 0.06,
            weight: 1.2,
            color: theme::FG,
            lower: false,
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
    /// Text goes straight onto the 3D scene (the HUD), not onto a panel.
    over_scene: bool,
    clip: Option<(Vec2, Vec2)>,
    verts: Vec<UiVertex>,
    under: Vec<UiVertex>,
}

impl Painter {
    pub fn new(scale: f32) -> Painter {
        Painter {
            scale,
            halo: Some((theme::HALO, theme::HALO_PX)),
            over_scene: false,
            clip: None,
            verts: Vec::new(),
            under: Vec::new(),
        }
    }

    pub fn set_halo(&mut self, halo: Option<(Rgba, f32)>) {
        self.halo = halo;
    }

    /// Text from here on lands straight on the 3D scene (or, `false`, on a
    /// panel). Over the scene, text that's merely dim on a dark panel would
    /// vanish into bright sky and sand: its fill is lifted toward opaque
    /// (see `scene_alpha`), keeping the order of strengths and fades to 0.
    pub fn set_over_scene(&mut self, over_scene: bool) {
        self.over_scene = over_scene;
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

    /// A filled disc as a triangle fan, `inner` at the centre blending to
    /// `outer` at the rim: a soft glow with a transparent rim, or a shaded
    /// ball. No halo, and not clipped (keep it inside any clip).
    pub fn disc(&mut self, c: Vec2, r: f32, inner: Rgba, outer: Rgba) {
        let s = self.scale;
        let n = ((r * s * 0.6) as usize).clamp(24, 180);
        let v = |p: Vec2, color: Rgba| UiVertex {
            pos: [p.x * s, p.y * s],
            color,
            uv: NO_UV,
            sdf: NO_SDF,
        };
        let at = |i: usize| {
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            c + Vec2::new(a.cos(), a.sin()) * r
        };
        for i in 0..n {
            self.verts
                .extend_from_slice(&[v(c, inner), v(at(i), outer), v(at(i + 1), outer)]);
        }
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
            sdf: NO_SDF,
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
            sdf: NO_SDF,
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

    /// Draws `text` with its top edge (the top of the capitals) at `pos.y`,
    /// aligned around `pos.x`. Returns its width.
    pub fn text(&mut self, pos: Vec2, text: &str, style: &TextStyle, align: Align) -> f32 {
        let w = measure(text, style);
        let x = match align {
            Align::Left => pos.x,
            Align::Center => pos.x - w * 0.5,
            Align::Right => pos.x - w,
        };
        let f = font::font();
        let em = style.size / f.cap_height;
        let baseline = pos.y + style.size;
        let s = self.scale;
        // Heavier styles thicken the strokes a touch; the halo is a soft dark rim around them.
        let dilate = (style.weight - 1.0) * 0.5 * s;
        let mut style = *style;
        if self.over_scene {
            style.color[3] = scene_alpha(style.color[3]);
        }
        let style = &style;
        let halo = self
            .halo
            .map(|(hc, hw)| text_halo(hc, hw, style, em * s, s, dilate));
        for (pen, g) in layout(text, style) {
            if g.size == Vec2::ZERO {
                continue;
            }
            let min = Vec2::new(x + pen, baseline) + g.offset * em;
            let size = g.size * em;
            if let Some((hc, hw)) = halo {
                self.glyph_quad(true, min, size, g, hc, [dilate + hw.0, hw.1]);
            }
            self.glyph_quad(false, min, size, g, style.color, [dilate, 1.0]);
        }
        w
    }

    /// One glyph's atlas cell at `min`/`size` (logical px), clipped like a rect.
    fn glyph_quad(
        &mut self,
        under: bool,
        min: Vec2,
        size: Vec2,
        g: &font::Glyph,
        color: Rgba,
        sdf: [f32; 2],
    ) {
        let Some((cmin, csize)) = clip_rect(min, size, self.clip) else {
            return;
        };
        let uv_at = |p: Vec2| g.uv_min + (g.uv_max - g.uv_min) * ((p - min) / size);
        let (u0, u1) = (uv_at(cmin), uv_at(cmin + csize));
        let s = self.scale;
        let (a, b) = (cmin * s, (cmin + csize) * s);
        let v = |x: f32, y: f32, u: f32, w: f32| UiVertex {
            pos: [x, y],
            color,
            uv: [u, w],
            sdf,
        };
        let out = if under {
            &mut self.under
        } else {
            &mut self.verts
        };
        out.extend_from_slice(&[
            v(a.x, a.y, u0.x, u0.y),
            v(b.x, a.y, u1.x, u0.y),
            v(b.x, b.y, u1.x, u1.y),
            v(a.x, a.y, u0.x, u0.y),
            v(b.x, b.y, u1.x, u1.y),
            v(a.x, b.y, u0.x, u1.y),
        ]);
    }
}

/// Text alpha over the 3D scene: dim (0.6) comes up to about 0.9 and faint
/// (0.24) to a half, while full stays full and 0 stays 0. The UI blends in
/// linear light, where a 60% pale fill over bright sand is barely brighter
/// than the sand.
pub fn scene_alpha(a: f32) -> f32 {
    let a = a.clamp(0.0, 1.0);
    1.0 - (1.0 - a).powf(2.5)
}

/// A text halo's colour and its distance-field (offset, ramp) in screen px,
/// for a line halo of `hc` and `hw` (logical px), text drawn `em_px` screen
/// px per em, and `scale` screen px per logical px.
///
/// Much darker and wider than a line's halo: pale letters a few pixels tall
/// over bright sky or sand only stay readable on a solid dark rim. The rim
/// is as wide as the glyph's distance field reaches, less a margin; past
/// that the field runs out at the cell edge and the halo would show as a box.
fn text_halo(
    hc: Rgba,
    hw: f32,
    style: &TextStyle,
    em_px: f32,
    scale: f32,
    dilate: f32,
) -> (Rgba, (f32, f32)) {
    let alpha = (style.color[3] * theme::TEXT_HALO_GAIN).min(1.0);
    let color = [hc[0], hc[1], hc[2], theme::TEXT_HALO_ALPHA * alpha];
    // How far past the outline the field reaches, screen px.
    let reach = font::SPREAD * em_px / font::EM;
    let ramp = (0.6 * hw * scale).max(1.0);
    let offset = (0.8 * hw * scale)
        .min(0.8 * reach - dilate - 0.5 * ramp)
        .max(0.0);
    (color, (offset, ramp))
}

/// Each glyph of `text` with its pen position (logical px from the start):
/// advances, kerning and tracking. Characters the font lacks count as spaces.
fn layout<'a>(text: &str, style: &TextStyle) -> Vec<(f32, &'a font::Glyph)> {
    let f = font::font();
    let em = style.size / f.cap_height;
    let mut out = Vec::new();
    let mut x = 0.0;
    let mut prev: Option<&font::Glyph> = None;
    for c in text.chars() {
        let Some(g) = f.glyph_as(c, style.lower) else {
            x += f.space * em + style.tracking;
            prev = None;
            continue;
        };
        if let Some(p) = prev {
            x += f.kern(p, g) * em;
        }
        out.push((x, g));
        x += g.advance * em + style.tracking;
        prev = Some(g);
    }
    out
}

/// Pips a row in [`hull_pips`].
pub const PIPS_A_ROW: u32 = 20;

/// Hulls as boxes, from the top left (#385, #450): a pip a hull up to
/// [`PIPS_A_ROW`]; past that, yours and the rest each counted out in
/// boxes of 100 (a square), 10 (a tall bar) and 1 (a pip), so hundreds
/// of hulls stay a short row. Each box's offset, size, and whether it's
/// yours (lit), with rows no wider than [`PIPS_A_ROW`] pips.
pub fn hull_boxes(total: u32, lit: u32, cell: Vec2, gap: f32) -> Vec<(Vec2, Vec2, bool)> {
    let lit = lit.min(total);
    let mut out = Vec::new();
    if total <= PIPS_A_ROW {
        for i in 0..total {
            out.push((Vec2::new(i as f32 * (cell.x + gap), 0.0), cell, i < lit));
        }
        return out;
    }
    let tall = 2.0 * cell.y + gap;
    let ten = Vec2::new(cell.x, tall);
    let hundred = Vec2::new(tall, tall);
    let max_w = PIPS_A_ROW as f32 * (cell.x + gap) - gap;
    let (mut x, mut y) = (0.0f32, 0.0f32);
    for (n, mine) in [(lit, true), (total - lit, false)] {
        let sizes = [(n / 100, hundred), (n / 10 % 10, ten), (n % 10, cell)];
        for (count, size) in sizes {
            for _ in 0..count {
                if x > 0.0 && x + size.x > max_w {
                    (x, y) = (0.0, y + tall + gap);
                }
                // Bottom-aligned, so pips sit on the bars' line.
                out.push((Vec2::new(x, y + tall - size.y), size, mine));
                x += size.x + gap;
            }
        }
        // A wider gap between yours and the rest.
        if n > 0 && x > 0.0 {
            x += 2.0 * gap;
        }
    }
    out
}

/// The width and height the boxes of [`hull_boxes`] take.
pub fn hull_boxes_size(boxes: &[(Vec2, Vec2, bool)]) -> Vec2 {
    boxes
        .iter()
        .fold(Vec2::ZERO, |m, (at, size, _)| m.max(*at + *size))
}

/// Hulls as boxes ([`hull_boxes`]) from `min` (the top left): yours in
/// `color`, the rest of `total` dimmed. Returns the height drawn.
pub fn hull_pips(
    p: &mut Painter,
    min: Vec2,
    total: u32,
    lit: u32,
    color: Rgba,
    cell: Vec2,
    gap: f32,
) -> f32 {
    let boxes = hull_boxes(total, lit, cell, gap);
    for (at, size, mine) in &boxes {
        let c = if *mine { color } else { with_alpha(color, 0.3) };
        p.rect(min + *at, *size, c);
    }
    hull_boxes_size(&boxes).y
}

/// Width of `text` in logical px (no trailing tracking).
pub fn measure(text: &str, style: &TextStyle) -> f32 {
    let f = font::font();
    let em = style.size / f.cap_height;
    let mut w = 0.0;
    let mut prev: Option<&font::Glyph> = None;
    let mut n = 0;
    for c in text.chars() {
        n += 1;
        let Some(g) = f.glyph_as(c, style.lower) else {
            w += f.space * em + style.tracking;
            prev = None;
            continue;
        };
        if let Some(p) = prev {
            w += f.kern(p, g) * em;
        }
        w += g.advance * em + style.tracking;
        prev = Some(g);
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
        sdf: NO_SDF,
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
            sdf: NO_SDF,
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
    fn hull_boxes_count_in_tens_and_hundreds_when_many() {
        let (cell, gap) = (Vec2::new(9.0, 12.0), 3.0);
        // A few: a pip each, yours first.
        let b = hull_boxes(5, 2, cell, gap);
        assert_eq!(b.len(), 5);
        assert_eq!(b.iter().filter(|x| x.2).count(), 2);
        // Many (#450): 2 of yours, 633 the rest's: 2 + 6 + 3 + 3 boxes.
        let b = hull_boxes(635, 2, cell, gap);
        assert_eq!(b.len(), 14);
        assert_eq!(b.iter().filter(|x| x.2).count(), 2);
        assert!(b[..2].iter().all(|x| x.2), "yours come first");
        // What the boxes stand for adds back up.
        let worth = |s: Vec2| {
            if s.x > cell.x {
                100
            } else if s.y > cell.y {
                10
            } else {
                1
            }
        };
        let sum =
            |mine: bool| -> u32 { b.iter().filter(|x| x.2 == mine).map(|x| worth(x.1)).sum() };
        assert_eq!((sum(true), sum(false)), (2, 633));
        // It stays small: no wider than a row of pips, a couple of rows.
        let size = hull_boxes_size(&b);
        assert!(size.x <= PIPS_A_ROW as f32 * (cell.x + gap) - gap);
        assert!(size.y < 4.0 * (cell.y + gap), "{size}");
        // Even thousands stay bounded in width.
        let size = hull_boxes_size(&hull_boxes(5000, 300, cell, gap));
        assert!(size.x <= PIPS_A_ROW as f32 * (cell.x + gap) - gap);
    }

    #[test]
    fn scene_alpha_lifts_dim_text_and_keeps_order() {
        assert_eq!(scene_alpha(0.0), 0.0);
        assert_eq!(scene_alpha(1.0), 1.0);
        assert!(scene_alpha(theme::DIM[3]) > 0.85);
        let mut last = 0.0;
        for i in 1..=100 {
            let a = scene_alpha(i as f32 / 100.0);
            assert!(a > last);
            last = a;
        }
    }

    #[test]
    fn text_halo_stays_inside_the_glyph_field() {
        // Past the field's reach the halo would fill the whole glyph cell.
        let f = font::font();
        for scale in [0.5, 1.0, 1.2, 2.4] {
            for size in [6.0, 8.5, 10.0, 16.0, 30.0] {
                for weight in [1.0, 1.2, 1.6] {
                    let style = TextStyle {
                        size,
                        weight,
                        ..TextStyle::LABEL
                    };
                    let em_px = size / f.cap_height * scale;
                    let dilate = (weight - 1.0) * 0.5 * scale;
                    let (_, (offset, ramp)) =
                        text_halo(theme::HALO, theme::HALO_PX, &style, em_px, scale, dilate);
                    let reach = font::SPREAD * em_px / font::EM;
                    assert!(offset >= 0.0 && ramp >= 1.0);
                    assert!(
                        dilate + offset + 0.5 * ramp <= reach.max(dilate + 0.5 * ramp) + 1e-4,
                        "size {size} scale {scale} weight {weight}"
                    );
                }
            }
        }
    }

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
        let f = font::font();
        let em = 12.0 / f.cap_height;
        // "10": two tabular digits plus one tracking gap.
        let digit = f.glyph('0').unwrap().advance * em;
        assert!((measure("10", &s) - (2.0 * digit + s.tracking)).abs() < 1e-3);
        assert_eq!(measure("", &s), 0.0);
        // Lowercase is set in capitals.
        assert_eq!(measure("kontos", &s), measure("KONTOS", &s));
        let mut p = Painter::new(1.0);
        assert_eq!(
            p.text(Vec2::ZERO, "10", &s, Align::Right),
            measure("10", &s)
        );
        // Each glyph: a halo quad under it and the glyph itself, six vertices each.
        assert_eq!(p.finish().len(), 2 * 2 * 6);
    }
}
