//! What shell impacts, tank explosions and burning wrecks look like: fire as
//! self-lit glow, smoke and dust as lit, billowing puffs, sparks as streaks
//! and debris as charred chunks. Everything is worked out from a blast's age
//! and where it went off, so a blast needs no state beyond its own.

use glam::{Mat4, Quat, Vec3};

use crate::render::Particle;
use crate::render::mesh::{Instance, MeshId};

/// Flame at its hottest: yellow-white.
const FIRE_HOT: Vec3 = Vec3::new(1.0, 0.55, 0.14);
/// Flame cooling: orange.
const FIRE: Vec3 = Vec3::new(1.0, 0.28, 0.03);
/// Flame dying: deep red.
const EMBER: Vec3 = Vec3::new(0.75, 0.1, 0.02);
/// Smoke off burning fuel and paint.
const SOOT: Vec3 = Vec3::new(0.07, 0.065, 0.06);
/// Dust and grit thrown up by a shell landing.
const DUST: Vec3 = Vec3::new(0.5, 0.46, 0.4);

const G: f32 = 9.8;

/// A repeatable 0..1 for this blast (`seed`) and part `k`.
fn rand(seed: f32, k: u32) -> f32 {
    let x = (seed * 12.9898 + k as f32 * 78.233).sin() * 43758.547;
    x - x.floor()
}

/// A repeatable seed for a blast where it went off.
pub fn seed_at(p: Vec3) -> f32 {
    rand(p.x * 0.731 + p.z * 1.379 + p.y * 0.173, 0)
}

/// A direction out from a blast: all round, tipped upward by `lift` (0..1).
fn spray(seed: f32, k: u32, lift: f32) -> Vec3 {
    let a = rand(seed, k * 7 + 1) * std::f32::consts::TAU;
    let y = lift + (1.0 - lift) * rand(seed, k * 7 + 2);
    let r = (1.0 - y * y).max(0.0).sqrt();
    Vec3::new(a.cos() * r, y, a.sin() * r)
}

/// How far something launched at `speed` and slowed by `drag` (1/s) has
/// gone after `t` s.
fn coast(speed: f32, drag: f32, t: f32) -> f32 {
    speed * (1.0 - (-drag * t).exp()) / drag
}

/// Fire cooling from yellow-white through orange to red as `heat` goes 1 to 0.
fn flame(heat: f32) -> Vec3 {
    if heat > 0.5 {
        FIRE.lerp(FIRE_HOT, (heat - 0.5) * 2.0)
    } else {
        EMBER.lerp(FIRE, heat * 2.0)
    }
}

/// Hot sparks thrown out in arcs, streaked along their flight.
fn sparks(at: Vec3, seed: f32, n: u32, speed: f32, life: f32, t: f32, out: &mut Vec<Particle>) {
    for k in 0..n {
        let life = life * (0.6 + 0.4 * rand(seed, k * 3 + 50));
        if t >= life {
            continue;
        }
        let v0 = spray(seed, k + 20, 0.25) * speed * (0.5 + 0.5 * rand(seed, k * 3 + 51));
        let p = at + v0 * t - Vec3::Y * 0.5 * G * t * t;
        let v = v0 - Vec3::Y * G * t;
        let f = t / life;
        out.push(Particle::glow(
            p,
            0.05 + 0.04 * rand(seed, k * 3 + 52),
            flame(1.0 - f),
            3.0 * (1.0 - f),
            v.normalize_or_zero() * (1.5 + v.length() * 0.12),
        ));
    }
}

/// A tank blowing up, `t` s in: a white flash, a fireball of rolling flame
/// that climbs and cools, a column of black smoke boiling up out of it, and
/// sparks thrown out in arcs.
pub fn kill(at: Vec3, t: f32, out: &mut Vec<Particle>) {
    let seed = seed_at(at);
    // Smoke first, so the fire inside it shows through.
    for k in 0..14 {
        let born = 0.08 + 0.5 * rand(seed, k * 5 + 100);
        let life = 2.4 + 1.2 * rand(seed, k * 5 + 101);
        let age = t - born;
        if age <= 0.0 || age >= life {
            continue;
        }
        let f = age / life;
        let dir = spray(seed, k + 100, 0.2);
        let side = Vec3::new(dir.x, 0.0, dir.z) * coast(3.5, 1.5, age);
        let rise = Vec3::Y * (0.8 + 1.4 * rand(seed, k * 5 + 102)) * age * (1.0 + age * 0.5);
        let p = at + dir * 1.2 + side + rise;
        let size = 1.3 + 3.0 * (1.0 - (1.0 - f).powi(2)) * (0.7 + 0.6 * rand(seed, k * 5 + 103));
        let alpha = (age / 0.25).min(1.0) * (1.0 - f).powf(1.3) * 0.85;
        // Sooty black at first, thinning to grey as it spreads.
        let shade = SOOT.lerp(Vec3::splat(0.22), f * 0.8);
        out.push(Particle::smoke(
            p,
            size,
            shade,
            alpha,
            rand(seed, k * 5 + 104),
        ));
    }
    // The flash.
    let flash = (-t * 12.0).exp();
    if flash > 0.01 {
        out.push(Particle::glow(
            at,
            3.5 + 6.0 * t,
            FIRE_HOT,
            4.0 * flash,
            Vec3::ZERO,
        ));
    }
    // The fireball: blobs of flame bursting outward and rolling upward.
    for k in 0..16 {
        let cool = 2.2 + 1.6 * rand(seed, k * 5 + 1);
        let heat = (-t * cool).exp();
        if heat < 0.04 {
            continue;
        }
        let dir = spray(seed, k, 0.1);
        let reach = 1.2 + 2.6 * rand(seed, k * 5 + 2);
        let p = at + dir * coast(reach * 6.0, 6.0, t) + Vec3::Y * 1.6 * t * t;
        let size = (0.9 + 0.8 * rand(seed, k * 5 + 3)) * (0.5 + 1.2 * (1.0 - (-t * 5.0).exp()));
        let grow = (t / 0.04).min(1.0);
        // Its lumps churn slowly as it burns.
        let churn = rand(seed, k * 5 + 4) * 0.9 + t * 0.04;
        out.push(Particle::fire(
            p,
            size * 1.2,
            flame(heat),
            3.2 * heat * grow,
            Vec3::ZERO,
            churn,
        ));
    }
    sparks(at, seed, 14, 16.0, 1.3, t, out);
}

/// Charred chunks of a tank flung out of its explosion, `t` s in.
pub fn kill_debris(at: Vec3, t: f32, out: &mut Vec<(MeshId, Instance)>) {
    let seed = seed_at(at);
    for k in 0..8 {
        let life = 1.0 + 0.6 * rand(seed, k * 4 + 200);
        if t >= life {
            continue;
        }
        let v0 = spray(seed, k + 200, 0.45) * (6.0 + 7.0 * rand(seed, k * 4 + 201));
        let p = at + v0 * t - Vec3::Y * 0.5 * G * t * t;
        let spin =
            Quat::from_axis_angle(spray(seed, k + 300, 0.0), t * (6.0 + 8.0 * rand(seed, k)));
        let s = (0.12 + 0.22 * rand(seed, k * 4 + 202)) * (1.0 - (t / life).powi(4));
        // Glowing hot as it leaves, charred black once it cools.
        let heat = (-t * 3.0).exp();
        let tint = Vec3::new(0.05, 0.045, 0.04)
            .lerp(FIRE, heat)
            .extend(heat * 3.0);
        out.push((
            MeshId::CUBE,
            Instance::new(
                Mat4::from_scale_rotation_translation(Vec3::new(s, s * 0.6, s * 1.4), spin, p),
                tint,
            ),
        ));
    }
}

/// A shell striking, `t` s in: a short flash, sparks, and a puff of grit
/// (off the ground or a wall) or of smoke (off a tank's armour, `hit`).
pub fn impact(at: Vec3, hit: bool, t: f32, out: &mut Vec<Particle>) {
    let seed = seed_at(at);
    let (n, color, scale) = if hit {
        (4, SOOT * 2.0, 1.2)
    } else {
        (4, DUST, 1.0)
    };
    for k in 0..n {
        let life = 0.9 + 0.6 * rand(seed, k * 4 + 10);
        if t >= life {
            continue;
        }
        let f = t / life;
        let dir = spray(seed, k + 10, 0.4);
        let p = at + dir * coast(4.0, 4.0, t) * scale + Vec3::Y * 0.5 * t;
        let size = scale * (0.35 + 1.1 * (1.0 - (1.0 - f).powi(2)));
        let alpha = (t / 0.05).min(1.0) * (1.0 - f).powi(2) * 0.7;
        out.push(Particle::smoke(
            p,
            size,
            color,
            alpha,
            rand(seed, k * 4 + 11),
        ));
    }
    let flash = (-t * if hit { 14.0 } else { 20.0 }).exp();
    let (big, hot) = if hit { (1.8, 4.0) } else { (1.1, 3.0) };
    if flash > 0.01 {
        out.push(Particle::glow(
            at,
            big * (1.0 + 2.0 * t),
            flame(flash),
            hot * flash,
            Vec3::ZERO,
        ));
    }
    sparks(
        at,
        seed,
        if hit { 9 } else { 6 },
        if hit { 11.0 } else { 8.0 },
        0.45,
        t,
        out,
    );
}

/// One puff off a burning wreck, `f` of the way through its life: it leaves
/// the hull as a lick of flame and rolls up and away as thick black smoke,
/// thinning and greying as it goes. `drift` is how far it travels.
pub fn wreck_puff(at: Vec3, drift: Vec3, f: f32, out: &mut Vec<Particle>) {
    let seed = seed_at(at);
    // Quick off the hull, slowing as it climbs.
    let p = at + drift * (1.0 - (1.0 - f).powf(1.6));
    let size = 0.6 + 2.8 * f.powf(0.7) * (0.8 + 0.4 * rand(seed, 1));
    let alpha = (f / 0.12).min(1.0) * (1.0 - f).powf(1.5) * 0.9;
    let shade = SOOT.lerp(Vec3::splat(0.2), f);
    out.push(Particle::smoke(p, size, shade, alpha, rand(seed, 2)));
    // The flame it starts as, licking up off the hull.
    let fire = 1.0 - f / 0.22;
    if fire > 0.0 {
        let flicker = 0.8 + 0.2 * rand(seed, 3);
        let up = at + Vec3::Y * 0.6 * f / 0.22;
        out.push(Particle::fire(
            up,
            0.5 + 0.35 * rand(seed, 4),
            flame(fire),
            1.8 * fire * flicker,
            Vec3::Y * 0.8,
            rand(seed, 5) * 0.9 + f * 0.05,
        ));
    }
}

/// Where the fire on a tank at height `tank` burns from, in a plasma river
/// whose surface is at `surface`: off its hull, or off the surface it's sunk
/// under.
pub fn fire_base(tank: f32, surface: f32) -> f32 {
    (tank + 1.0).max(surface + 0.1)
}

/// A tank burning in a plasma river, at `time`: flames licking up all round
/// and over it, and sparks spat out. `at` is the middle of the fire's foot.
pub fn plasma_fire(at: Vec3, rot: Quat, time: f32, out: &mut Vec<Particle>) {
    const TONGUES: u32 = 18;
    for k in 0..TONGUES {
        let life = 0.45 + 0.35 * rand(0.37, k);
        let t = time / life + rand(0.71, k);
        // Each lick's different from the one before it.
        let (f, seed) = (t.fract(), rand(t.floor().rem_euclid(97.0) * 0.173, k));
        let a = k as f32 / TONGUES as f32 * std::f32::consts::TAU + seed;
        let r = 0.35 + 0.65 * rand(seed, 1);
        let mut off = rot * Vec3::new(a.cos() * 1.7 * r, 0.0, a.sin() * 2.8 * r);
        off.y = 0.0;
        // They lean in and up as they go.
        let p = at + off * (1.0 - 0.45 * f) + Vec3::Y * (0.2 + 2.6 * f * (0.7 + 0.6 * seed));
        let heat = (1.0 - f) * (f / 0.12).min(1.0);
        out.push(Particle::fire(
            p,
            (0.6 + 0.5 * rand(seed, 2)) * (1.0 - 0.5 * f),
            flame(1.0 - f),
            2.4 * heat,
            Vec3::Y * 0.9,
            rand(seed, 3) * 0.9 + f * 0.05,
        ));
    }
    // Spits of sparks, one round after another.
    for k in 0..2 {
        let t = time / 0.5 + k as f32 * 0.5;
        let seed = rand(t.floor().rem_euclid(97.0) * 0.319, k);
        let p = at + rot * Vec3::new(seed * 2.0 - 1.0, 0.0, rand(seed, 4) * 4.0 - 2.0);
        sparks(p, seed, 5, 7.0, 0.45, t.fract() * 0.5, out);
    }
    // And the glow of it all.
    let shiver = 1.0 + 0.1 * (time * 23.0).sin() + 0.07 * (time * 41.0).sin();
    out.push(Particle::glow(
        at + Vec3::Y * 0.8,
        3.2 * shiver,
        FIRE,
        0.5,
        Vec3::ZERO,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explosion_burns_out_into_smoke() {
        let at = Vec3::new(10.0, 2.0, -4.0);
        let mut early = Vec::new();
        kill(at, 0.1, &mut early);
        assert!(early.iter().any(|p| p.glow > 1.0), "no fire at first");
        let mut late = Vec::new();
        kill(at, 2.0, &mut late);
        assert!(late.iter().all(|p| p.glow == 0.0), "still burning at 2 s");
        assert!(
            late.iter().any(|p| p.billow > 0.0 && p.glow == 0.0),
            "no smoke left at 2 s"
        );
        // Smoke climbs.
        let top = late.iter().map(|p| p.pos[1]).fold(f32::MIN, f32::max);
        assert!(top > at.y + 3.0, "smoke tops out at {top}");
    }

    #[test]
    fn same_blast_looks_the_same_each_frame() {
        let at = Vec3::new(3.0, 1.0, 7.0);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        kill(at, 0.4, &mut a);
        kill(at, 0.4, &mut b);
        assert_eq!(
            bytemuck::cast_slice::<_, u8>(&a),
            bytemuck::cast_slice::<_, u8>(&b)
        );
    }

    #[test]
    fn wreck_puff_starts_as_flame_and_ends_as_smoke() {
        let mut first = Vec::new();
        wreck_puff(Vec3::ZERO, Vec3::Y * 4.0, 0.05, &mut first);
        assert!(first.iter().any(|p| p.glow > 0.0));
        let mut last = Vec::new();
        wreck_puff(Vec3::ZERO, Vec3::Y * 4.0, 0.8, &mut last);
        assert!(last.iter().all(|p| p.billow > 0.0 && p.glow == 0.0));
        assert!(last[0].pos[1] > 3.0);
    }
}
