//! Value noise.

pub(super) fn hash(i: i32, j: i32, k: u32) -> f32 {
    let h = (i as u32).wrapping_mul(0x9E37_79B9)
        ^ (j as u32).wrapping_mul(0x7F4A_7C15)
        ^ k.wrapping_mul(0x85EB_CA6B);
    let h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
    let h = (h ^ (h >> 12)).wrapping_mul(0x297A_2D39);
    (h ^ (h >> 15)) as f32 / u32::MAX as f32
}

fn fade(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Smooth value noise along a line, 0..1, one bump per unit.
pub(super) fn noise(x: f32, k: u32) -> f32 {
    let (i, f) = (x.floor(), fade(x.fract()));
    let i = i as i32;
    hash(i, 0, k) * (1.0 - f) + hash(i + 1, 0, k) * f
}

/// Smooth value noise over a plane, 0..1.
pub(super) fn noise2(x: f32, y: f32, k: u32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (fade(x - x.floor()), fade(y - y.floor()));
    let a = hash(ix, iy, k) * (1.0 - fx) + hash(ix + 1, iy, k) * fx;
    let b = hash(ix, iy + 1, k) * (1.0 - fx) + hash(ix + 1, iy + 1, k) * fx;
    a * (1.0 - fy) + b * fy
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A bump 1 at 0 falling to 0 at `w` either side.
pub(super) fn bump(d: f32, w: f32) -> f32 {
    1.0 - smoothstep(0.0, w, d.abs())
}
