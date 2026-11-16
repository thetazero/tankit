//! Procedural surface textures, generated once at startup: tileable maps of
//! the small-scale detail flat-shaded geometry can't carry (hammered and dented
//! plate, scratches, chips, grime streaks). Sampled triplanar in object space
//! by the scene shader, so they need no UVs and stick to moving parts.
//!
//! Layers (RGBA8, full mip chain):
//! - 0 `PLATE_NORMAL`: rg = tangent-space normal xy, b = height, a = cavity (1 = open)
//! - 1 `PLATE_MASKS`: r = albedo variation (0.5 neutral), g = roughness offset
//!   (0.5 neutral), b = wear (scratches/chips to bare metal), a = grime streaks

pub const SIZE: usize = 512;
pub const LAYERS: u32 = 2;

/// Deterministic hash -> [0, 1).
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 16_777_216.0
}

/// Tileable value noise: lattice of `period` cells across the texture.
fn value(u: f32, v: f32, period: i32, seed: u32) -> f32 {
    let (x, y) = (u * period as f32, v * period as f32);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let w = |i: i32| i.rem_euclid(period);
    let (xi, yi) = (x0 as i32, y0 as i32);
    let a = hash(w(xi), w(yi), seed);
    let b = hash(w(xi + 1), w(yi), seed);
    let c = hash(w(xi), w(yi + 1), seed);
    let d = hash(w(xi + 1), w(yi + 1), seed);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sy
}

fn fbm(u: f32, v: f32, base: i32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut norm) = (0.0, 0.5, 0.0);
    for o in 0..octaves {
        sum += value(u, v, base << o, seed + o * 17) * amp;
        norm += amp;
        amp *= 0.5;
    }
    sum / norm
}

/// Tileable cellular noise: distance to the nearest feature point (0..~1).
fn cells(u: f32, v: f32, period: i32, seed: u32) -> f32 {
    let (x, y) = (u * period as f32, v * period as f32);
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let mut best = 9.0f32;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (cx, cy) = (xi + dx, yi + dy);
            let (wx, wy) = (cx.rem_euclid(period), cy.rem_euclid(period));
            let px = cx as f32 + hash(wx, wy, seed);
            let py = cy as f32 + hash(wx, wy, seed + 7);
            best = best.min(((px - x).powi(2) + (py - y).powi(2)).sqrt());
        }
    }
    best
}

struct Field {
    v: Vec<f32>,
}

impl Field {
    fn new(f: impl Fn(f32, f32) -> f32) -> Field {
        let mut v = vec![0.0; SIZE * SIZE];
        for y in 0..SIZE {
            for x in 0..SIZE {
                v[y * SIZE + x] = f(
                    (x as f32 + 0.5) / SIZE as f32,
                    (y as f32 + 0.5) / SIZE as f32,
                );
            }
        }
        Field { v }
    }
    fn at(&self, x: i32, y: i32) -> f32 {
        let s = SIZE as i32;
        self.v[(y.rem_euclid(s) * s + x.rem_euclid(s)) as usize]
    }
}

/// Thin random scratches (wrapping across the tile edges): returns a mask.
fn scratches(count: usize, seed: u32) -> Vec<f32> {
    let mut m = vec![0.0f32; SIZE * SIZE];
    for i in 0..count {
        let r = |k: u32| hash(i as i32, k as i32, seed);
        let (x0, y0) = (r(0) * SIZE as f32, r(1) * SIZE as f32);
        // Mostly long, shallow-angled strokes, like scrapes along a hull.
        let ang = (r(2) - 0.5) * 0.9 + if r(3) > 0.7 { 1.57 } else { 0.0 };
        let len = 20.0 + r(4) * 140.0;
        let strength = 0.4 + 0.6 * r(5);
        let steps = len as usize * 2;
        for s in 0..steps {
            let t = s as f32 / 2.0;
            // Slight wobble so they don't look ruled.
            let wob = (t * 0.05 + r(6) * 6.0).sin() * 1.5;
            let x = (x0 + ang.cos() * t - ang.sin() * wob).rem_euclid(SIZE as f32) as usize;
            let y = (y0 + ang.sin() * t + ang.cos() * wob).rem_euclid(SIZE as f32) as usize;
            let fade = (1.0 - (t / len - 0.5).abs() * 2.0).max(0.0).sqrt();
            let idx = y.min(SIZE - 1) * SIZE + x.min(SIZE - 1);
            m[idx] = m[idx].max(strength * fade);
        }
    }
    m
}

fn pack(c: f32) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// Generate both layers, each as a list of mip levels (RGBA8, tightly packed).
pub fn generate() -> Vec<Vec<Vec<u8>>> {
    // Height: broad dents, hammered pitting from cellular noise, fine grain,
    // and scratches cut slightly into the surface.
    let scr = scratches(70, 11);
    let height = Field::new(|u, v| {
        let dents = fbm(u, v, 3, 3, 1);
        let hammer = cells(u, v, 22, 5);
        let grain = fbm(u, v, 64, 2, 9);
        dents * 0.25 + (hammer * 0.9).min(1.0) * 0.15 + grain * 0.12
    });
    let height = Field {
        v: height
            .v
            .iter()
            .zip(&scr)
            .map(|(h, s)| h - s * 0.12)
            .collect(),
    };

    // Normal map from the height, with wraparound differences.
    let strength = 4.0;
    let mut normal = Vec::with_capacity(SIZE * SIZE * 4);
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let dx = (height.at(x + 1, y) - height.at(x - 1, y)) * strength;
            let dy = (height.at(x, y + 1) - height.at(x, y - 1)) * strength;
            let n = glam::Vec3::new(-dx, -dy, 1.0).normalize();
            // Cavity: how far below its neighborhood this texel sits.
            let around = (height.at(x + 3, y)
                + height.at(x - 3, y)
                + height.at(x, y + 3)
                + height.at(x, y - 3))
                * 0.25;
            let cavity = 1.0 - ((around - height.at(x, y)) * 6.0).clamp(0.0, 0.6);
            normal.extend_from_slice(&[
                pack(n.x * 0.5 + 0.5),
                pack(n.y * 0.5 + 0.5),
                pack(height.at(x, y)),
                pack(cavity),
            ]);
        }
    }

    // Masks.
    let mut masks = Vec::with_capacity(SIZE * SIZE * 4);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (u, v) = (
                (x as f32 + 0.5) / SIZE as f32,
                (y as f32 + 0.5) / SIZE as f32,
            );
            // Paint fade and repaint patches.
            let blotch = fbm(u, v, 4, 4, 21);
            let albedo = 0.5 + (blotch - 0.5) * 0.9;
            // Roughness: uneven sheen, scratches polished smooth.
            let s = scr[y * SIZE + x];
            let rough = 0.5 + (fbm(u, v, 6, 3, 31) - 0.5) * 0.9 - s * 0.3;
            // Wear: scratches plus small chips where cells are tight.
            let chip = 1.0 - (cells(u, v, 40, 41) * 3.2).min(1.0);
            let chip = if fbm(u, v, 5, 2, 43) > 0.58 {
                chip
            } else {
                0.0
            };
            let wear = s.max(chip * 0.9);
            // Grime: vertical streaks (v runs down a vertical face).
            let streak = value(u, v * 0.15, 48, 51) * 0.6 + value(u, v * 0.3, 96, 53) * 0.4;
            let grime = ((streak - 0.45) * 2.2).clamp(0.0, 1.0) * (0.5 + 0.5 * fbm(u, v, 3, 2, 57));
            masks.extend_from_slice(&[pack(albedo), pack(rough), pack(wear), pack(grime)]);
        }
    }

    vec![mips(normal, true), mips(masks, false)]
}

/// Box-filtered mip chain. Normal layers renormalize xy after averaging, so
/// detail fades to flat with distance instead of shimmering.
fn mips(level0: Vec<u8>, is_normal: bool) -> Vec<Vec<u8>> {
    let mut out = vec![level0];
    let mut size = SIZE;
    while size > 1 {
        let prev = out.last().unwrap();
        let half = size / 2;
        let mut next = vec![0u8; half * half * 4];
        for y in 0..half {
            for x in 0..half {
                for c in 0..4 {
                    let s = |dx: usize, dy: usize| {
                        prev[((y * 2 + dy) * size + x * 2 + dx) * 4 + c] as u32
                    };
                    next[(y * half + x) * 4 + c] =
                        ((s(0, 0) + s(1, 0) + s(0, 1) + s(1, 1) + 2) / 4) as u8;
                }
                if is_normal {
                    let i = (y * half + x) * 4;
                    let nx = next[i] as f32 / 127.5 - 1.0;
                    let ny = next[i + 1] as f32 / 127.5 - 1.0;
                    let len = (nx * nx + ny * ny).sqrt();
                    if len > 1.0 {
                        next[i] = pack((nx / len) * 0.5 + 0.5);
                        next[i + 1] = pack((ny / len) * 0.5 + 0.5);
                    }
                }
            }
        }
        out.push(next);
        size = half;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_tiles_seamlessly() {
        // Values on opposite edges of the tile must match.
        for i in 0..32 {
            let t = i as f32 / 32.0;
            assert!((value(0.0, t, 8, 3) - value(1.0, t, 8, 3)).abs() < 1e-5);
            assert!((cells(t, 0.0, 10, 3) - cells(t, 1.0, 10, 3)).abs() < 1e-4);
        }
    }

    #[test]
    fn mip_chain_is_complete() {
        let m = mips(vec![128u8; SIZE * SIZE * 4], true);
        assert_eq!(m.len(), SIZE.trailing_zeros() as usize + 1);
        assert_eq!(m.last().unwrap().len(), 4);
    }
}
