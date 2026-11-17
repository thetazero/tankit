//! The UI font: Inter Medium (SIL Open Font License, `assets/fonts/OFL.txt`),
//! drawn from a signed distance field so it stays smooth at every size and
//! scale. Each glyph is rasterized large once, turned into distances to its
//! outline, and shrunk into a small single-channel atlas; the UI shader turns
//! a distance back into crisp, anti-aliased coverage, and the same distances
//! give text its soft dark halo.
//!
//! Text is always set in capitals, like the HUD's labels always were.
//! Digits share one advance (tabular figures) so counters and timers don't
//! shuffle as they change.

use std::collections::HashMap;
use std::sync::OnceLock;

use ab_glyph::{Font as _, FontRef, GlyphId, PxScale, point};
use glam::Vec2;

const FONT: &[u8] = include_bytes!("../../assets/fonts/Inter-Medium.otf");

/// Characters in the atlas. Anything else is drawn as a space.
const CHARSET: &str = concat!(
    " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`{|}~",
    "abcdefghijklmnopqrstuvwxyz",
    "·°×–—←↑→↓•"
);

/// Atlas pixels per em.
const EM: f32 = 48.0;
/// The glyphs are rasterized this many times larger before being shrunk into the atlas.
const OVERSAMPLE: usize = 4;
/// How far (atlas px) the field reaches either side of an outline; also each cell's padding.
pub const SPREAD: f32 = 8.0;
/// Atlas width in px; rows are added as needed.
const ATLAS_W: usize = 512;
/// Mip levels, so small text doesn't shimmer.
pub const MIPS: usize = 4;

pub struct Glyph {
    /// Advance in em, before tracking.
    pub advance: f32,
    /// Where the quad's top-left sits relative to the pen on the baseline, in em (y down).
    pub offset: Vec2,
    /// Quad size in em (padding included).
    pub size: Vec2,
    /// Atlas cell, in 0..1 texture coordinates.
    pub uv_min: Vec2,
    pub uv_max: Vec2,
    id: GlyphId,
    /// Atlas cell, in atlas px: (x, y, w, h).
    cell: (usize, usize, usize, usize),
    /// Large-scale rasterizing: glyph origin within the oversampled cell.
    origin: Vec2,
}

pub struct Font {
    face: FontRef<'static>,
    glyphs: HashMap<char, Glyph>,
    /// Height of the capitals, in em.
    pub cap_height: f32,
    /// A space's advance, in em.
    pub space: f32,
    pub atlas_size: (u32, u32),
}

/// The UI font (metrics and atlas layout; the atlas pixels come from `atlas_pixels`).
pub fn font() -> &'static Font {
    static F: OnceLock<Font> = OnceLock::new();
    F.get_or_init(Font::load)
}

impl Font {
    fn load() -> Font {
        let face = FontRef::try_from_slice(FONT).expect("bundled font parses");
        let upem = face.units_per_em().expect("font has units per em");
        let em = |units: f32| units / upem;
        let digits_advance = ('0'..='9')
            .map(|c| em(face.h_advance_unscaled(face.glyph_id(c))))
            .fold(0.0, f32::max);
        let hi = big_scale(&face);
        let pad = SPREAD as usize;
        let (mut x, mut y, mut row_h) = (0usize, 0usize, 0usize);
        let mut glyphs = HashMap::new();
        for c in CHARSET.chars() {
            let id = face.glyph_id(c);
            assert!(id.0 != 0 || c == ' ', "font lacks {c:?}");
            let mut advance = em(face.h_advance_unscaled(id));
            let mut shift = 0.0;
            if c.is_ascii_digit() {
                shift = (digits_advance - advance) * 0.5;
                advance = digits_advance;
            }
            let outlined = face.outline_glyph(id.with_scale_and_position(hi, point(0.0, 0.0)));
            let Some(o) = outlined else {
                glyphs.insert(
                    c,
                    Glyph {
                        advance,
                        offset: Vec2::ZERO,
                        size: Vec2::ZERO,
                        uv_min: Vec2::ZERO,
                        uv_max: Vec2::ZERO,
                        id,
                        cell: (0, 0, 0, 0),
                        origin: Vec2::ZERO,
                    },
                );
                continue;
            };
            let b = o.px_bounds();
            // Cell in atlas px: the glyph's box, snapped out to whole atlas px, plus padding.
            let s = OVERSAMPLE as f32;
            let (x0, y0) = ((b.min.x / s).floor(), (b.min.y / s).floor());
            let (x1, y1) = ((b.max.x / s).ceil(), (b.max.y / s).ceil());
            let w = (x1 - x0) as usize + 2 * pad;
            let h = (y1 - y0) as usize + 2 * pad;
            if x + w > ATLAS_W {
                x = 0;
                y += row_h + 1;
                row_h = 0;
            }
            let cell = (x, y, w, h);
            x += w + 1;
            row_h = row_h.max(h);
            let corner = Vec2::new(x0 - SPREAD, y0 - SPREAD);
            glyphs.insert(
                c,
                Glyph {
                    advance,
                    offset: (corner / EM) + Vec2::new(shift, 0.0),
                    size: Vec2::new(w as f32, h as f32) / EM,
                    uv_min: Vec2::ZERO,
                    uv_max: Vec2::ZERO,
                    id,
                    cell,
                    // Glyph origin within the oversampled cell.
                    origin: -corner * s,
                },
            );
        }
        let atlas_h = (y + row_h).next_multiple_of(1 << MIPS) as u32;
        let atlas_w = ATLAS_W as u32;
        for g in glyphs.values_mut() {
            let (x, y, w, h) = g.cell;
            let size = Vec2::new(atlas_w as f32, atlas_h as f32);
            g.uv_min = Vec2::new(x as f32, y as f32) / size;
            g.uv_max = Vec2::new((x + w) as f32, (y + h) as f32) / size;
        }
        let h = face.glyph_id('H');
        let cap_height = face
            .outline_glyph(h.with_scale_and_position(hi, point(0.0, 0.0)))
            .map(|o| -o.px_bounds().min.y / (EM * OVERSAMPLE as f32))
            .expect("font has an H");
        let space = em(face.h_advance_unscaled(face.glyph_id(' ')));
        Font {
            face,
            glyphs,
            cap_height,
            space,
            atlas_size: (atlas_w, atlas_h),
        }
    }

    /// Glyph for `c`, set in capitals; `None` if the font doesn't carry it.
    /// `c`'s glyph, as a capital: most of the UI is set in caps whatever it's given.
    pub fn glyph(&self, c: char) -> Option<&Glyph> {
        self.glyph_as(c, false)
    }

    /// `c`'s glyph, its own case kept when `lower`.
    pub fn glyph_as(&self, c: char, lower: bool) -> Option<&Glyph> {
        let c = if lower {
            c
        } else {
            c.to_uppercase().next().unwrap_or(c)
        };
        self.glyphs.get(&c)
    }

    /// Kerning between two glyphs, in em.
    pub fn kern(&self, a: &Glyph, b: &Glyph) -> f32 {
        let upem = self.face.units_per_em().unwrap_or(1000.0);
        self.face.kern_unscaled(a.id, b.id) / upem
    }

    /// The atlas as one byte per pixel (0.5 = on the outline, higher inside),
    /// largest mip first, each level half the size of the one before.
    pub fn atlas_pixels(&self) -> Vec<Vec<u8>> {
        let (aw, ah) = (self.atlas_size.0 as usize, self.atlas_size.1 as usize);
        let mut atlas = vec![0u8; aw * ah];
        let hi = big_scale(&self.face);
        let s = OVERSAMPLE;
        for g in self.glyphs.values() {
            let (cx, cy, cw, ch) = g.cell;
            if cw == 0 {
                continue;
            }
            let (w, h) = (cw * s, ch * s);
            let mut inside = vec![false; w * h];
            let glyph =
                g.id.with_scale_and_position(hi, point(g.origin.x, g.origin.y));
            if let Some(o) = self.face.outline_glyph(glyph) {
                let b = o.px_bounds();
                let (bx, by) = (b.min.x as i64, b.min.y as i64);
                o.draw(|x, y, cov| {
                    let (px, py) = (bx + x as i64, by + y as i64);
                    if cov >= 0.5 && px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                        inside[py as usize * w + px as usize] = true;
                    }
                });
            }
            let dist = signed_distance(&inside, w, h);
            // Average each OVERSAMPLE² block, in atlas px.
            for ay in 0..ch {
                for ax in 0..cw {
                    let mut sum = 0.0;
                    for dy in 0..s {
                        for dx in 0..s {
                            sum += dist[(ay * s + dy) * w + ax * s + dx];
                        }
                    }
                    let d = sum / (s * s) as f32 / s as f32;
                    atlas[(cy + ay) * aw + cx + ax] = encode(d);
                }
            }
        }
        let mut levels = vec![atlas];
        let (mut w, mut h) = (aw, ah);
        while levels.len() < MIPS {
            let prev = levels.last().expect("level 0 is in");
            let (nw, nh) = (w / 2, h / 2);
            let mut next = vec![0u8; nw * nh];
            for y in 0..nh {
                for x in 0..nw {
                    let at = |dx: usize, dy: usize| prev[(2 * y + dy) * w + 2 * x + dx] as u32;
                    next[y * nw + x] = ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8;
                }
            }
            levels.push(next);
            (w, h) = (nw, nh);
        }
        levels
    }
}

/// Oversampled rasterizing scale: `EM * OVERSAMPLE` px per em.
fn big_scale(face: &FontRef<'static>) -> PxScale {
    let upem = face.units_per_em().unwrap_or(1000.0);
    PxScale::from(EM * OVERSAMPLE as f32 * face.height_unscaled() / upem)
}

/// Distance in atlas px (positive inside) to a byte: 0.5 on the outline, ±SPREAD at the ends.
fn encode(d: f32) -> u8 {
    ((0.5 + d / (2.0 * SPREAD)).clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Per pixel, the distance (px) to the nearest pixel on the other side of
/// the outline, positive inside and negative outside.
fn signed_distance(inside: &[bool], w: usize, h: usize) -> Vec<f32> {
    let to_inside = squared_distance(inside, w, h, true);
    let to_outside = squared_distance(inside, w, h, false);
    (0..w * h)
        .map(|i| {
            if inside[i] {
                to_outside[i].sqrt() - 0.5
            } else {
                -(to_inside[i].sqrt() - 0.5)
            }
        })
        .collect()
}

/// Exact squared Euclidean distance to the nearest pixel where `inside == target`
/// (Felzenszwalb and Huttenlocher: a 1D pass down the columns, then along the rows).
fn squared_distance(inside: &[bool], w: usize, h: usize, target: bool) -> Vec<f32> {
    const FAR: f32 = 1e10;
    let mut grid: Vec<f32> = inside
        .iter()
        .map(|&v| if v == target { 0.0 } else { FAR })
        .collect();
    let n = w.max(h);
    let (mut f, mut d) = (vec![0.0; n], vec![0.0; n]);
    let (mut v, mut z) = (vec![0usize; n], vec![0.0f32; n + 1]);
    for x in 0..w {
        for y in 0..h {
            f[y] = grid[y * w + x];
        }
        edt_1d(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            grid[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&grid[y * w..(y + 1) * w]);
        edt_1d(&f[..w], &mut d[..w], &mut v, &mut z);
        grid[y * w..(y + 1) * w].copy_from_slice(&d[..w]);
    }
    grid
}

/// 1D squared distance transform of the sampled function `f` into `d`.
fn edt_1d(f: &[f32], d: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        let qf = q as f32;
        let mut s;
        loop {
            let p = v[k];
            let pf = p as f32;
            s = ((f[q] + qf * qf) - (f[p] + pf * pf)) / (2.0 * qf - 2.0 * pf);
            if s <= z[k] && k > 0 {
                k -= 1;
            } else {
                break;
            }
        }
        if s <= z[k] {
            // k == 0 and the new parabola wins everywhere.
            v[0] = q;
            z[0] = f32::NEG_INFINITY;
            z[1] = f32::INFINITY;
            continue;
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f32::INFINITY;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        let qf = q as f32;
        while z[k + 1] < qf {
            k += 1;
        }
        let p = v[k] as f32;
        *out = (qf - p) * (qf - p) + f[v[k]];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_what_the_ui_prints() {
        let f = font();
        for c in ('A'..='Z')
            .chain('a'..='z')
            .chain('0'..='9')
            .chain("·°+-%:./()".chars())
        {
            assert!(f.glyph(c).is_some(), "missing {c:?}");
        }
        assert!(f.glyph('~').is_some());
        assert!(f.glyph('€').is_none());
        assert!(
            f.cap_height > 0.6 && f.cap_height < 0.8,
            "cap {}",
            f.cap_height
        );
    }

    #[test]
    fn digits_are_tabular() {
        let f = font();
        let one = f.glyph('1').unwrap().advance;
        assert!(('0'..='9').all(|c| f.glyph(c).unwrap().advance == one));
    }

    #[test]
    fn distance_transform_is_exact_on_a_square() {
        // A 3×3 block in a 9×9 grid.
        let (w, h) = (9, 9);
        let inside: Vec<bool> = (0..w * h)
            .map(|i| (3..6).contains(&(i % w)) && (3..6).contains(&(i / w)))
            .collect();
        let d = signed_distance(&inside, w, h);
        // Two to the right of the block's edge column, on its middle row.
        assert!((d[4 * w + 7] - -1.5).abs() < 1e-4);
        // The block's centre is two columns from the outside.
        assert!((d[4 * w + 4] - 1.5).abs() < 1e-4);
        // Diagonal from the corner.
        assert!((d[w + 1] - -(8f32.sqrt() - 0.5)).abs() < 1e-4);
    }

    #[test]
    fn atlas_has_ink_where_the_glyphs_are() {
        let f = font();
        let levels = f.atlas_pixels();
        assert_eq!(levels.len(), MIPS);
        let (w, _) = f.atlas_size;
        let g = f.glyph('I').unwrap();
        let (x, y, cw, ch) = g.cell;
        // The middle of an I is solid; its cell's corner is well outside.
        assert!(levels[0][(y + ch / 2) * w as usize + x + cw / 2] > 160);
        assert!(levels[0][y * w as usize + x] < 20);
    }
}
