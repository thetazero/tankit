//! A picture of a long run of the war: the galaxy at a few moments (each
//! system in a shade of its people's color, one shade a realm, its seat
//! ringed; systems at war bright white), the same stretch of it close up at
//! those moments, and under them each faction's share of the galaxy over the
//! whole run, with the fraction of systems at war, and how many realms each
//! people is; and the mercenary companies (#318): their ships on the maps
//! (amber, ringed while on a contract), and how many there are, on contract,
//! and what they're worth over the run.

use ab_glyph::{Font as _, FontRef, PxScale, ScaleFont as _, point};
use image::{Rgb, RgbImage};

use glam::Vec2;

use super::{Census, Galaxy, REGION, SIDES};
use crate::faction::Faction;

const FONT: &[u8] = include_bytes!("../../assets/fonts/Inter-Medium.otf");
const W: u32 = 1600;
const MAP_W: u32 = 790;
const MAP_H: u32 = (MAP_W as f32 * REGION.y / REGION.x) as u32;
const CHART_H: u32 = 420;
/// The close-ups: a window of the region (ly), and each panel's width.
const ZOOM_AT: Vec2 = Vec2::new(1350.0, 1500.0);
const ZOOM: Vec2 = Vec2::new(1100.0, 690.0);
const ZOOM_W: u32 = (W - 5 * PAD) / 4;
const ZOOM_H: u32 = (ZOOM_W as f32 * ZOOM.y / ZOOM.x) as u32;
const REALMS_H: u32 = 180;
const MERCS_H: u32 = 200;
const AMBER: Rgb<u8> = Rgb([255, 189, 56]);
const WORTH: Rgb<u8> = Rgb([110, 200, 225]);
const PAD: u32 = 10;
const BG: Rgb<u8> = Rgb([12, 13, 16]);

fn color(f: usize) -> [f32; 3] {
    let c = Faction::side(f).ui();
    [c[0], c[1], c[2]]
}

fn rgb(c: [f32; 3], k: f32) -> Rgb<u8> {
    Rgb(c.map(|x| (x * k * 255.0).clamp(0.0, 255.0) as u8))
}

/// One moment of the war, for the map panels.
pub struct Snapshot {
    tick: u64,
    /// (position in the plane, ly; owner; a tint for its realm; its realm's seat; at war)
    dots: Vec<(Vec2, u8, [f32; 3], bool, bool)>,
    /// Each company's ship: where, and whether it's on a contract.
    ships: Vec<(Vec2, bool)>,
}

/// Realm `r`'s tint of its people's color: brighter or darker, a little
/// off in hue, so neighbors mostly come out different.
fn shade(r: u32) -> [f32; 3] {
    let h = r.wrapping_mul(2_654_435_761);
    let k = |shift: u32, steps: u32| ((h >> shift) % steps) as f32 / (steps - 1) as f32;
    let light = 0.35 + 0.75 * k(26, 5);
    [0, 1, 2].map(|c| light * (0.7 + 0.6 * k(8 + 6 * c, 4)))
}

fn tinted(f: usize, tint: [f32; 3]) -> Rgb<u8> {
    let c = color(f);
    Rgb([0, 1, 2].map(|i| (c[i] * tint[i] * 255.0).clamp(0.0, 255.0) as u8))
}

impl Snapshot {
    pub fn take(g: &Galaxy) -> Snapshot {
        Snapshot {
            tick: g.tick,
            dots: (0..g.systems.len())
                .map(|i| {
                    let s = &g.systems[i];
                    let r = g.holder(i);
                    let seat = r != super::realm::NONE && g.realms[r as usize].seat as usize == i;
                    (
                        s.pos.truncate(),
                        s.owner() as u8,
                        shade(r),
                        seat,
                        s.at_war(),
                    )
                })
                .collect(),
            ships: g
                .companies
                .iter()
                .map(|c| {
                    (
                        g.systems[c.at as usize].pos.truncate(),
                        c.contract.is_some(),
                    )
                })
                .collect(),
        }
    }

    /// Draws the region's window `at`..`at + size` (ly) into a `w` x `h` panel at (x0, y0).
    #[allow(clippy::too_many_arguments)]
    fn draw(
        &self,
        img: &mut RgbImage,
        x0: u32,
        y0: u32,
        w: u32,
        h: u32,
        at: Vec2,
        size: Vec2,
        big: bool,
    ) {
        fill(img, x0, y0, w, h, Rgb([20, 22, 27]));
        let k = w as f32 / size.x;
        let px = |p: Vec2| {
            let q = (p - at) * k;
            (q.x >= 0.0 && q.y >= 0.0 && q.x < w as f32 && q.y < h as f32)
                .then(|| (x0 + q.x as u32, y0 + q.y as u32))
        };
        let r = if big { 3 } else { 1 };
        // Quiet systems first, then seats, then the ones at war on top.
        for pass in 0..3 {
            for &(p, owner, k, seat, war) in &self.dots {
                let Some((x, y)) = px(p) else { continue };
                match pass {
                    0 if !war => dot(img, x, y, r, tinted(owner as usize, k)),
                    1 if seat && !war && big => {
                        ring(img, x, y, r + 3, Rgb([235, 235, 235]));
                        dot(img, x, y, r, tinted(owner as usize, k));
                    }
                    2 if war => dot(img, x, y, r + 1, Rgb([255, 255, 255])),
                    _ => {}
                }
            }
        }
        // The companies' ships over it all.
        for &(p, working) in &self.ships {
            let Some((x, y)) = px(p) else { continue };
            dot(img, x, y, r + 1, AMBER);
            if working {
                ring(img, x, y, r + 4, AMBER);
            }
        }
    }
}

/// Four snapshots over a 2x2 grid, the close-ups of them in a row, then the
/// share chart over `history`, and the realms.
pub fn render(snaps: &[Snapshot], history: &[Census]) -> RgbImage {
    let maps_h = 2 * (MAP_H + PAD) + PAD;
    let zoom_y = maps_h;
    let maps_h = maps_h + ZOOM_H + PAD;
    let h = maps_h + CHART_H + REALMS_H + MERCS_H + PAD;
    let mut img = RgbImage::from_pixel(W, h, BG);
    let face = FontRef::try_from_slice(FONT).expect("bundled font parses");
    for (i, snap) in snaps.iter().take(4).enumerate() {
        let x0 = PAD + (i as u32 % 2) * (MAP_W + PAD);
        let y0 = PAD + (i as u32 / 2) * (MAP_H + PAD);
        snap.draw(&mut img, x0, y0, MAP_W, MAP_H, Vec2::ZERO, REGION, false);
        // Where the close-up is.
        let k = MAP_W as f32 / REGION.x;
        frame(
            &mut img,
            x0 + (ZOOM_AT.x * k) as u32,
            y0 + (ZOOM_AT.y * k) as u32,
            (ZOOM.x * k) as u32,
            (ZOOM.y * k) as u32,
            Rgb([120, 120, 120]),
        );
        text(
            &mut img,
            &face,
            x0 + 10,
            y0 + 8,
            22.0,
            &format!("TICK {}", snap.tick),
            Rgb([230, 230, 230]),
        );
        let zx = PAD + i as u32 * (ZOOM_W + PAD);
        snap.draw(&mut img, zx, zoom_y, ZOOM_W, ZOOM_H, ZOOM_AT, ZOOM, true);
        text(
            &mut img,
            &face,
            zx + 8,
            zoom_y + 6,
            16.0,
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
            Faction::side(f).name(),
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
    // Realms: how many each people is, over the run.
    let (ry, rh) = (cy + ch + 56, REALMS_H - 66);
    fill(&mut img, cx, ry, cw, rh, Rgb([20, 22, 27]));
    let counts = || history.iter().flat_map(|c| c.realms);
    let lo = counts().min().unwrap_or(0).saturating_sub(10) as f32;
    let hi = counts().max().unwrap_or(1) as f32 + 10.0;
    for f in 0..SIDES {
        for px in 0..cw {
            let c = &history[(px as usize * (n - 1) / (cw as usize - 1)).min(history.len() - 1)];
            let v = (c.realms[f] as f32 - lo) / (hi - lo);
            let y = ry + rh - 2 - (v * (rh - 2) as f32) as u32;
            img.put_pixel(cx + px, y, rgb(color(f), 1.0));
            img.put_pixel(cx + px, y + 1, rgb(color(f), 1.0));
        }
    }
    text(
        &mut img,
        &face,
        PAD,
        ry - 2,
        16.0,
        &format!("{hi:.0}"),
        Rgb([180, 180, 180]),
    );
    text(
        &mut img,
        &face,
        PAD,
        ry + rh - 16,
        16.0,
        &format!("{lo:.0}"),
        Rgb([180, 180, 180]),
    );
    let (first, last) = (
        history.first().copied().unwrap_or_default(),
        history.last().copied().unwrap_or_default(),
    );
    text(
        &mut img,
        &face,
        cx,
        ry - 26,
        18.0,
        &format!(
            "REALMS OF EACH PEOPLE: {} IN ALL AT THE START, {} AT THE END  ·  {} BROKE AWAY, {} FOLDED TOGETHER",
            first.realms.iter().sum::<u32>(),
            last.realms.iter().sum::<u32>(),
            last.splits,
            last.merges,
        ),
        Rgb([230, 230, 230]),
    );
    // Companies: how many, how many on a contract (left scale), and what
    // they're worth all told (right scale), over the run.
    let (my, mh) = (ry + rh + 56, MERCS_H - 66);
    fill(&mut img, cx, my, cw, mh, Rgb([20, 22, 27]));
    let at = |px: u32| &history[(px as usize * (n - 1) / (cw as usize - 1)).min(history.len() - 1)];
    let count_top = history
        .iter()
        .map(|c| c.companies)
        .max()
        .unwrap_or(1)
        .max(1) as f32
        * 1.1;
    let busy_top = history
        .iter()
        .map(|c| c.contracts)
        .max()
        .unwrap_or(1)
        .max(1) as f32
        * 1.1;
    let worth_top = history.iter().map(|c| c.merc_capital).fold(1.0, f32::max) * 1.1;
    let line = |img: &mut RgbImage, v: &dyn Fn(&Census) -> f32, top: f32, c: Rgb<u8>| {
        for px in 0..cw {
            let y = my + mh - 2 - ((v(at(px)) / top).clamp(0.0, 1.0) * (mh - 2) as f32) as u32;
            img.put_pixel(cx + px, y, c);
            img.put_pixel(cx + px, y + 1, c);
        }
    };
    // Contracts: a shaded area, averaged over the ticks each column covers.
    for px in 0..cw {
        let a = (px as usize * (n - 1) / cw as usize).min(history.len() - 1);
        let b = ((px as usize + 1) * (n - 1) / cw as usize).clamp(a + 1, history.len());
        let span = &history[a..b];
        let mean = span.iter().map(|c| c.contracts as f32).sum::<f32>() / span.len() as f32;
        let top = my + mh - ((mean / busy_top).clamp(0.0, 1.0) * mh as f32) as u32;
        for y in top..my + mh {
            img.put_pixel(cx + px, y, Rgb([70, 72, 82]));
        }
    }
    line(&mut img, &|c| c.merc_capital, worth_top, WORTH);
    line(&mut img, &|c| c.companies as f32, count_top, AMBER);
    text(
        &mut img,
        &face,
        PAD,
        my - 2,
        16.0,
        &format!("{count_top:.0}"),
        Rgb([180, 180, 180]),
    );
    text(
        &mut img,
        &face,
        PAD,
        my + mh - 16,
        16.0,
        "0",
        Rgb([180, 180, 180]),
    );
    text(
        &mut img,
        &face,
        rx,
        my - 2,
        16.0,
        &format!("{:.0}K", worth_top / 1000.0),
        WORTH,
    );
    text(&mut img, &face, rx, my + mh - 16, 16.0, "0", WORTH);
    text(
        &mut img,
        &face,
        cx,
        my - 26,
        18.0,
        &format!(
            "COMPANIES (AMBER): {} AT THE START, {} AT THE END  ·  ON A CONTRACT (GRAY, OWN SCALE TO {})  ·  WORTH IN CRYSTAL (BLUE, RIGHT SCALE): {:.0}K TO {:.0}K",
            first.companies,
            last.companies,
            busy_top as u32,
            first.merc_capital / 1000.0,
            last.merc_capital / 1000.0,
        ),
        AMBER,
    );
    img
}

fn ring(img: &mut RgbImage, x: u32, y: u32, r: i32, c: Rgb<u8>) {
    for dy in -r..=r {
        for dx in -r..=r {
            let d = dx * dx + dy * dy;
            if d > r * r + 1 || d < (r - 1) * (r - 1) {
                continue;
            }
            let (px, py) = (x as i32 + dx, y as i32 + dy);
            if px >= 0 && py >= 0 && (px as u32) < img.width() && (py as u32) < img.height() {
                img.put_pixel(px as u32, py as u32, c);
            }
        }
    }
}

fn frame(img: &mut RgbImage, x0: u32, y0: u32, w: u32, h: u32, c: Rgb<u8>) {
    for x in x0..(x0 + w).min(img.width()) {
        for y in [y0, y0 + h] {
            if y < img.height() {
                img.put_pixel(x, y, c);
            }
        }
    }
    for y in y0..(y0 + h).min(img.height()) {
        for x in [x0, x0 + w] {
            if x < img.width() {
                img.put_pixel(x, y, c);
            }
        }
    }
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
