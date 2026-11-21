//! A picture of a long run of the war: the galaxy at a few moments (each
//! system in its holder's color, systems at war bright white), and under it
//! each faction's share of the galaxy over the whole run, with the fraction of
//! systems at war.

use ab_glyph::{Font as _, FontRef, PxScale, ScaleFont as _, point};
use image::{Rgb, RgbImage};

use super::{Census, Galaxy, REGION, SIDES};
use crate::faction::Faction;

const FONT: &[u8] = include_bytes!("../../assets/fonts/Inter-Medium.otf");
const W: u32 = 1600;
const MAP_W: u32 = 790;
const MAP_H: u32 = (MAP_W as f32 * REGION.y / REGION.x) as u32;
const CHART_H: u32 = 420;
const PAD: u32 = 10;
const BG: Rgb<u8> = Rgb([12, 13, 16]);

fn color(f: usize) -> [f32; 3] {
    let c = Faction::ALL[f].ui();
    [c[0], c[1], c[2]]
}

fn rgb(c: [f32; 3], k: f32) -> Rgb<u8> {
    Rgb(c.map(|x| (x * k * 255.0).clamp(0.0, 255.0) as u8))
}

/// One map panel's pixels.
pub struct Snapshot {
    tick: u64,
    /// (x px, y px, owner, at war)
    dots: Vec<(u32, u32, u8, bool)>,
}

impl Snapshot {
    pub fn take(g: &Galaxy) -> Snapshot {
        let k = MAP_W as f32 / REGION.x;
        Snapshot {
            tick: g.tick,
            dots: g
                .systems
                .iter()
                .map(|s| {
                    (
                        ((s.pos.x * k) as u32).min(MAP_W - 1),
                        ((s.pos.y * k) as u32).min(MAP_H - 1),
                        s.owner() as u8,
                        s.at_war(),
                    )
                })
                .collect(),
        }
    }
}

/// Four snapshots over a 2x2 grid, then the share chart over `history`.
pub fn render(snaps: &[Snapshot], history: &[Census]) -> RgbImage {
    let maps_h = 2 * (MAP_H + PAD) + PAD;
    let h = maps_h + CHART_H + PAD;
    let mut img = RgbImage::from_pixel(W, h, BG);
    let face = FontRef::try_from_slice(FONT).expect("bundled font parses");
    for (i, snap) in snaps.iter().take(4).enumerate() {
        let x0 = PAD + (i as u32 % 2) * (MAP_W + PAD);
        let y0 = PAD + (i as u32 / 2) * (MAP_H + PAD);
        fill(&mut img, x0, y0, MAP_W, MAP_H, Rgb([20, 22, 27]));
        // Quiet systems first, then the ones at war on top.
        for war in [false, true] {
            for &(x, y, owner, at_war) in &snap.dots {
                if at_war != war {
                    continue;
                }
                let (c, r) = if at_war {
                    (Rgb([255, 255, 255]), 2)
                } else {
                    (rgb(color(owner as usize), 0.6), 1)
                };
                dot(&mut img, x0 + x, y0 + y, r, c);
            }
        }
        text(
            &mut img,
            &face,
            x0 + 10,
            y0 + 8,
            22.0,
            &format!("TICK {}", snap.tick),
            Rgb([230, 230, 230]),
        );
    }
    // Shares, stacked, over the run.
    let (cx, cy, cw, ch) = (PAD + 60, maps_h + 30, W - 2 * PAD - 120, CHART_H - 70);
    fill(&mut img, cx, cy, cw, ch, Rgb([20, 22, 27]));
    let n = history.len().max(2);
    for px in 0..cw {
        let c = &history[(px as usize * (n - 1) / (cw as usize - 1)).min(history.len() - 1)];
        let mut acc = 0.0;
        for f in 0..SIDES {
            let lo = acc;
            acc += c.share[f];
            let (y1, y2) = (
                cy + ch - (acc * ch as f32) as u32,
                cy + ch - (lo * ch as f32) as u32,
            );
            for y in y1..y2.min(cy + ch) {
                img.put_pixel(cx + px, y, rgb(color(f), 0.7));
            }
        }
    }
    // Grid lines at 25/50/75%.
    for q in 1..4 {
        let y = cy + ch - ch * q / 4;
        for px in (0..cw).step_by(4) {
            img.put_pixel(cx + px, y, Rgb([0, 0, 0]));
        }
        text(
            &mut img,
            &face,
            PAD,
            y - 9,
            16.0,
            &format!("{}%", q * 25),
            Rgb([180, 180, 180]),
        );
    }
    // At war: white line, on its own scale (right axis, 0..WAR_TOP).
    let peak = history.iter().map(|c| c.at_war).fold(0.0, f32::max);
    let top = [0.05, 0.1, 0.2, 0.5, 1.0]
        .into_iter()
        .find(|&t| peak <= t)
        .unwrap_or(1.0);
    let mut last = None;
    for px in 0..cw {
        let c = &history[(px as usize * (n - 1) / (cw as usize - 1)).min(history.len() - 1)];
        let y = cy + ch - 1 - ((c.at_war / top) * (ch - 1) as f32) as u32;
        let (a, b) = match last {
            Some(l) => (u32::min(l, y), u32::max(l, y)),
            None => (y, y),
        };
        for yy in a..=b {
            img.put_pixel(cx + px, yy, Rgb([255, 255, 255]));
            if px + 1 < cw {
                img.put_pixel(cx + px + 1, yy, Rgb([255, 255, 255]));
            }
        }
        last = Some(y);
    }
    let rx = cx + cw + 8;
    text(
        &mut img,
        &face,
        rx,
        cy - 8,
        16.0,
        &format!("{:.0}%", top * 100.0),
        Rgb([255, 255, 255]),
    );
    text(
        &mut img,
        &face,
        rx,
        cy + ch / 2 - 8,
        16.0,
        &format!("{:.0}%", top * 50.0),
        Rgb([255, 255, 255]),
    );
    text(
        &mut img,
        &face,
        rx,
        cy + ch - 16,
        16.0,
        "0%",
        Rgb([255, 255, 255]),
    );
    // Legend and axis.
    let mut lx = cx;
    let ly = maps_h + 4;
    for f in 0..SIDES {
        fill(&mut img, lx, ly + 4, 14, 14, rgb(color(f), 1.0));
        lx += 20;
        lx += text(
            &mut img,
            &face,
            lx,
            ly,
            18.0,
            Faction::ALL[f].name(),
            Rgb([230, 230, 230]),
        ) + 24;
    }
    fill(&mut img, lx, ly + 10, 24, 3, Rgb([255, 255, 255]));
    lx += 30;
    text(
        &mut img,
        &face,
        lx,
        ly,
        18.0,
        "SYSTEMS AT WAR (RIGHT SCALE)",
        Rgb([230, 230, 230]),
    );
    let last_tick = history.last().map(|c| c.tick).unwrap_or(0);
    text(
        &mut img,
        &face,
        cx,
        cy + ch + 8,
        16.0,
        "TICK 0",
        Rgb([180, 180, 180]),
    );
    let end = format!("TICK {last_tick}");
    text(
        &mut img,
        &face,
        cx + cw - 90,
        cy + ch + 8,
        16.0,
        &end,
        Rgb([180, 180, 180]),
    );
    img
}

fn fill(img: &mut RgbImage, x0: u32, y0: u32, w: u32, h: u32, c: Rgb<u8>) {
    for y in y0..(y0 + h).min(img.height()) {
        for x in x0..(x0 + w).min(img.width()) {
            img.put_pixel(x, y, c);
        }
    }
}

fn dot(img: &mut RgbImage, x: u32, y: u32, r: i32, c: Rgb<u8>) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy > r * r + 1 {
                continue;
            }
            let (px, py) = (x as i32 + dx, y as i32 + dy);
            if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                img.put_pixel(px as u32, py as u32, c);
            }
        }
    }
}

/// Draws `s` with its top-left at (x, y); returns its width in px.
fn text(img: &mut RgbImage, face: &FontRef, x: u32, y: u32, size: f32, s: &str, c: Rgb<u8>) -> u32 {
    let scale = PxScale::from(size);
    let sf = face.as_scaled(scale);
    let mut pen = x as f32;
    let base = y as f32 + sf.ascent();
    for ch in s.chars() {
        let id = face.glyph_id(ch);
        let g = id.with_scale_and_position(scale, point(pen, base));
        pen += sf.h_advance(id);
        let Some(o) = face.outline_glyph(g) else {
            continue;
        };
        let b = o.px_bounds();
        o.draw(|gx, gy, cov| {
            let (px, py) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
            if px < 0 || py < 0 || px as u32 >= img.width() || py as u32 >= img.height() {
                return;
            }
            let p = img.get_pixel_mut(px as u32, py as u32);
            for i in 0..3 {
                p.0[i] = (p.0[i] as f32 * (1.0 - cov) + c.0[i] as f32 * cov) as u8;
            }
        });
    }
    (pen - x as f32) as u32
}
