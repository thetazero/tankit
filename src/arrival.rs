//! How a tank arrives when it spawns or respawns: each side brings its tanks
//! in its own way (see docs/setting.md). A tank arrives the way its team's
//! faction does, whatever its parts are made by.
//!
//! Purely looks. The tank is on the ground, under its spawn shield and free
//! to drive and fire, from the first step; all of this is drawn over that,
//! as a function of the seconds since it arrived, and is over well inside
//! the shield.
//!
//! - **Imperium**: sent down from orbit. A crimson-and-gold lance of light
//!   falls from the sky, the tank comes down it and lands in a ring of dust,
//!   and the emblem (a planet under a halo) burns on the ground for a moment
//!   around it: ceremony that long ago stopped needing a reason.
//! - **Resonant**: the vessel won't stay dead. It stands dark; motes of
//!   crystal light spiral in to its heart, and it kindles with a toll that
//!   rings out over the ground and echoes once.
//! - **Uthenn**: made, the way they make their cubes. A green frame marks
//!   the build volume, tungsten cubes drop into it, and a laser plane sweeps
//!   up through the hull, leaving finished machine behind it.
//! - **Ithessar**: lowered on plasma. A rose orb opens over the spot and the
//!   tank floats down out of it on a stream of plasma, then the orb bursts.

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::faction::Faction;
use crate::render::Particle;
use crate::render::mesh::{Instance, MeshId};
use crate::sim::Tank;

/// Seconds an arrival plays for. Inside the spawn shield.
pub const ARRIVAL_TIME: f32 = 1.6;

/// Imperium: seconds from the top of the lance to touchdown, and how high
/// the tank is first drawn.
const DROP_TIME: f32 = 0.4;
const DROP_HEIGHT: f32 = 32.0;
/// Resonant: when the motes reach the heart and the toll rings.
const TOLL: f32 = 0.7;
/// Uthenn: seconds the scan plane takes to sweep the hull.
const SCAN_TIME: f32 = 0.9;
/// Ithessar: seconds the tank takes to float down, from how high.
const DESCENT_TIME: f32 = 0.9;
const DESCENT_HEIGHT: f32 = 3.5;

const CRIMSON: Vec3 = Vec3::new(0.9, 0.12, 0.1);
const GOLD: Vec3 = Vec3::new(1.0, 0.68, 0.24);
const GOLD_HOT: Vec3 = Vec3::new(1.0, 0.9, 0.62);
const DUST: Vec3 = Vec3::new(0.42, 0.36, 0.28);
const TUNGSTEN: Vec3 = Vec3::new(0.3, 0.31, 0.33);
const CRYSTAL: Vec3 = crate::tank::resonant::CRYSTAL_LIGHT;
const CRYSTAL_HOT: Vec3 = crate::tank::resonant::CRYSTAL_LIGHT_HOT;
const LASER: Vec3 = crate::tank::uthenn::LASER_LIGHT;
const PLASMA: Vec3 = crate::tank::ithessar::PLASMA;
const PLASMA_HOT: Vec3 = crate::tank::ithessar::PLASMA_HOT;

/// One tank's arrival in progress.
pub struct Arrival<'a> {
    pub faction: Faction,
    /// Seconds since it arrived, 0..`ARRIVAL_TIME`.
    pub t: f32,
    /// The tank where it really is (not moved by `pose`).
    pub tank: &'a Tank,
    /// Ground height under it.
    pub ground: f32,
    /// Per-tank variation, 0..1.
    pub seed: f32,
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 0..1 over `a..b`, unclamped below and above.
fn span(a: f32, b: f32, x: f32) -> f32 {
    ((x - a) / (b - a)).clamp(0.0, 1.0)
}

fn inst(mesh: MeshId, scale: Vec3, rot: Quat, at: Vec3, tint: Vec4) -> (MeshId, Instance) {
    (
        mesh,
        Instance::new(Mat4::from_scale_rotation_translation(scale, rot, at), tint),
    )
}

/// A glowing rod from `a` to `b`, `w` thick.
fn rod(a: Vec3, b: Vec3, w: f32, color: Vec3, glow: f32) -> Option<(MeshId, Instance)> {
    let d = b - a;
    let len = d.length();
    (len > 1e-3 && w > 1e-3).then(|| {
        inst(
            MeshId::CYLINDER,
            Vec3::new(w, len, w),
            Quat::from_rotation_arc(Vec3::Y, d / len),
            a + d * 0.5,
            color.extend(glow),
        )
    })
}

/// Cheap stable hash of `k` and `seed` to 0..1.
fn hash(k: f32, seed: f32) -> f32 {
    ((k * 12.9898 + seed * 78.233).sin() * 43758.547)
        .fract()
        .abs()
}

impl Arrival<'_> {
    fn heart(&self) -> Vec3 {
        let t = self.tank;
        t.pos + t.up() * t.chassis.hull_top * 0.85
    }

    fn base(&self) -> Vec3 {
        let p = self.tank.pos;
        Vec3::new(p.x, self.ground + 0.04, p.z)
    }

    /// The tank as drawn: lifted (it's still coming down) or dark (it's
    /// still being kindled or made). `shown` is the interpolated tank.
    pub fn pose(&self, shown: &mut Tank) {
        let t = self.t;
        // The Company (never a side of its own yet) drops in the Imperium's way.
        match self.faction {
            Faction::Imperium | Faction::Company => {
                let f = span(0.0, DROP_TIME, t);
                shown.pos.y += DROP_HEIGHT * (1.0 - f * f);
            }
            Faction::Resonant => {
                shown.color *= 0.07 + 0.93 * smooth(TOLL - 0.35, TOLL + 0.45, t);
            }
            Faction::Uthenn => {
                let f = smooth(0.0, SCAN_TIME, t);
                shown.color = (shown.color * TUNGSTEN * 0.8).lerp(shown.color, f);
            }
            Faction::Ithessar => {
                let f = smooth(0.0, DESCENT_TIME, t);
                shown.pos.y += DESCENT_HEIGHT * (1.0 - f);
            }
        }
    }

    /// The solid and self-lit pieces of the arrival.
    pub fn instances(&self, out: &mut Vec<(MeshId, Instance)>) {
        match self.faction {
            Faction::Imperium | Faction::Company => self.imperium(out),
            Faction::Resonant => self.resonant(out),
            Faction::Uthenn => self.uthenn(out),
            Faction::Ithessar => self.ithessar(out),
        }
    }

    /// Its soft light, drawn in the particle pass.
    pub fn glow(&self, out: &mut Vec<Particle>) {
        match self.faction {
            Faction::Imperium | Faction::Company => self.imperium_glow(out),
            Faction::Resonant => self.resonant_glow(out),
            Faction::Uthenn => self.uthenn_glow(out),
            Faction::Ithessar => self.ithessar_glow(out),
        }
    }

    // --- Imperium: sent down from orbit -------------------------------------

    fn imperium(&self, out: &mut Vec<(MeshId, Instance)>) {
        let t = self.t;
        let base = self.base();
        // The lance: thin and hot as the tank rides it down, then it thins out.
        let lance = (1.0 - span(DROP_TIME, DROP_TIME + 0.5, t)) * span(0.0, 0.05, t);
        if lance > 0.0 {
            let top = base + Vec3::Y * 220.0;
            out.extend(rod(base, top, 0.45 * lance, CRIMSON, 2.0));
            out.extend(rod(base, top, 0.14 * lance, GOLD_HOT, 5.0));
        }
        let u = t - DROP_TIME;
        if u < 0.0 {
            return;
        }
        let yaw = self.tank.yaw();
        let face = Quat::from_rotation_y(yaw);
        // The emblem burns on the ground around it: the planet's rim, and
        // the halo arching over it ahead of the tank. It flares on
        // touchdown, then shrinks back into the ground.
        let burn = 1.0 - span(0.3, 1.15, u);
        if burn > 0.0 {
            let glow = 1.5 + 3.0 * (1.0 - span(0.0, 0.25, u));
            let w = 0.22 * burn;
            let ring = |r: f32, from: f32, to: f32, n: usize, w: f32, out: &mut Vec<_>| {
                let at = |a: f32| base + face * Vec3::new(a.cos() * r, 0.0, a.sin() * r);
                for k in 0..n {
                    let a0 = from + (to - from) * k as f32 / n as f32;
                    let a1 = from + (to - from) * (k + 1) as f32 / n as f32;
                    out.extend(rod(at(a0), at(a1), w, GOLD, glow));
                }
            };
            let pi = std::f32::consts::PI;
            // The planet: a full disc rim.
            ring(3.2, 0.0, 2.0 * pi, 24, w, out);
            // The halo: an arch over it, from side to side through the front.
            ring(4.4, 0.0, pi, 14, w * 1.4, out);
            // Its points, reaching down past the planet's sides.
            for s in [-1.0f32, 1.0] {
                out.extend(rod(
                    base + face * Vec3::new(4.4 * s, 0.0, 0.0),
                    base + face * Vec3::new(4.0 * s, 0.0, -1.4),
                    w * 1.1,
                    GOLD,
                    glow,
                ));
            }
        }
    }

    fn imperium_glow(&self, out: &mut Vec<Particle>) {
        let t = self.t;
        let base = self.base();
        let glow = Particle::glow;
        if t < DROP_TIME {
            // The retro flare under the falling tank, and the streak it leaves.
            let f = t / DROP_TIME;
            let at = base + Vec3::Y * DROP_HEIGHT * (1.0 - f * f);
            out.push(glow(at, 2.4, GOLD, 2.0, Vec3::ZERO));
            out.push(glow(at + Vec3::Y * 4.0, 1.4, CRIMSON, 2.0, Vec3::Y * 4.0));
            // Where it will land, already lit.
            out.push(glow(base, 2.5 * f, GOLD, 1.5 * f, Vec3::ZERO));
        } else {
            let u = t - DROP_TIME;
            let flash = 1.0 - span(0.0, 0.5, u);
            out.push(glow(
                base + Vec3::Y,
                2.5 + 5.0 * u,
                GOLD_HOT,
                3.0 * flash,
                Vec3::ZERO,
            ));
            out.push(glow(
                base,
                6.0,
                CRIMSON,
                1.5 * (1.0 - span(0.0, 1.0, u)),
                Vec3::ZERO,
            ));
            // Dust thrown out from the touchdown, rolling outward and settling.
            let dust = span(0.0, 1.3, u);
            if dust < 1.0 {
                let n = 22;
                for k in 0..n {
                    let a = (k as f32 + hash(k as f32, self.seed) * 0.6) / n as f32
                        * std::f32::consts::TAU;
                    let r = 2.0 + 7.5 * (1.0 - (1.0 - dust).powi(3));
                    let size = 0.8 + 1.8 * dust;
                    out.push(Particle {
                        pos: (base + Vec3::new(a.cos() * r, 0.4 + 0.6 * dust, a.sin() * r))
                            .to_array(),
                        size,
                        color: DUST.extend(0.55 * (1.0 - dust)).to_array(),
                        stretch: [0.0; 3],
                        glow: 0.0,
                        billow: 0.0,
                    });
                }
            }
        }
    }

    // --- Resonant: the vessel won't stay dead -------------------------------

    /// Where crystal mote `k` is: spiralling in from around the vessel to
    /// its heart, arriving at `TOLL`.
    fn mote(&self, k: usize) -> Option<(Vec3, f32)> {
        let n = 14.0;
        let k = k as f32;
        let start = 0.25 * hash(k, self.seed);
        let f = span(start, TOLL, self.t);
        if f >= 1.0 {
            return None;
        }
        let a0 = k / n * std::f32::consts::TAU + self.seed * 6.0;
        let a = a0 + f * 2.2;
        let r = 7.0 * (1.0 - f) * (1.0 - f);
        let h = 0.4 + 3.5 * hash(k + 0.5, self.seed);
        let heart = self.heart();
        let around = Vec3::new(a.cos() * r, 0.0, a.sin() * r);
        let at = (self.base() + around + Vec3::Y * h).lerp(heart + around, f);
        // Fade in as it gathers.
        Some((at, span(0.0, 0.15, f)))
    }

    /// A ring of the toll spreading over the ground: radius and strength,
    /// `delay` after the toll.
    fn toll_ring(&self, delay: f32, strength: f32) -> Option<(f32, f32)> {
        let u = (self.t - TOLL - delay) / 0.85;
        (0.0..1.0)
            .contains(&u)
            .then(|| (1.0 + 10.0 * (1.0 - (1.0 - u).powi(2)), strength * (1.0 - u)))
    }

    fn resonant(&self, out: &mut Vec<(MeshId, Instance)>) {
        for k in 0..14 {
            if let Some((at, s)) = self.mote(k) {
                let size = 0.22 * s;
                out.push(inst(
                    MeshId::GEM,
                    Vec3::new(size, size * 2.4, size),
                    Quat::from_rotation_y(self.t * 5.0 + k as f32),
                    at,
                    CRYSTAL_HOT.extend(4.0),
                ));
            }
        }
        // The toll's flash at the heart.
        let u = self.t - TOLL;
        if (0.0..0.4).contains(&u) {
            let s = (1.0 - u / 0.4) * 0.9;
            out.push(inst(
                MeshId::SPHERE,
                Vec3::splat(s),
                Quat::IDENTITY,
                self.heart(),
                CRYSTAL_HOT.extend(5.0),
            ));
        }
    }

    fn resonant_glow(&self, out: &mut Vec<Particle>) {
        let glow = Particle::glow;
        for k in 0..14 {
            if let Some((at, s)) = self.mote(k) {
                out.push(glow(at, 0.55, CRYSTAL, 1.6 * s, Vec3::ZERO));
            }
        }
        let heart = self.heart();
        // The heart gathering light as the motes come in, then the toll.
        let gather = span(0.0, TOLL, self.t);
        let after = 1.0 - span(TOLL, ARRIVAL_TIME, self.t);
        out.push(glow(
            heart,
            0.6 + 1.2 * gather,
            CRYSTAL,
            1.6 * gather * after,
            Vec3::ZERO,
        ));
        let u = self.t - TOLL;
        if u >= 0.0 {
            let flash = 1.0 - span(0.0, 0.6, u);
            out.push(glow(
                heart,
                2.0 + 4.0 * u,
                CRYSTAL_HOT,
                2.5 * flash,
                Vec3::ZERO,
            ));
            // A shaft of light up out of it.
            out.push(glow(
                heart + Vec3::Y * 4.0,
                1.2,
                CRYSTAL,
                1.4 * flash,
                Vec3::Y * 5.0,
            ));
        }
        // The toll, and its echo, rolling out over the ground.
        let base = self.base() + Vec3::Y * 0.2;
        for (delay, strength) in [(0.0, 1.6), (0.3, 0.8)] {
            if let Some((r, s)) = self.toll_ring(delay, strength) {
                let n = 36;
                for k in 0..n {
                    let a = k as f32 / n as f32 * std::f32::consts::TAU;
                    let tangent = Vec3::new(-a.sin(), 0.0, a.cos());
                    out.push(glow(
                        base + Vec3::new(a.cos() * r, 0.0, a.sin() * r),
                        0.35,
                        CRYSTAL,
                        s,
                        tangent * (r * 0.6).max(1.0),
                    ));
                }
            }
        }
    }

    // --- Uthenn: made, the way they make their cubes ------------------------

    /// The build volume: half extents across and along, and its height.
    fn volume(&self) -> (f32, f32, f32) {
        let c = &self.tank.chassis;
        let along = (c.hull_z.1 - c.hull_z.0).abs() * 0.5 + 0.5;
        (c.track_x + c.track_width + 0.3, along, c.hull_top + 0.6)
    }

    fn uthenn(&self, out: &mut Vec<(MeshId, Instance)>) {
        let t = self.t;
        let base = self.base();
        let face = Quat::from_rotation_y(self.tank.yaw());
        let (hx, hz, top) = self.volume();
        let corner = |x: f32, y: f32, z: f32| base + face * Vec3::new(x * hx, y, z * hz);
        let frame = span(0.0, 0.1, t) * (1.0 - span(SCAN_TIME + 0.2, ARRIVAL_TIME, t));
        if frame > 0.0 {
            let w = 0.07 * frame;
            let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
            for i in 0..4 {
                let (x0, z0) = corners[i];
                let (x1, z1) = corners[(i + 1) % 4];
                // Footprint and lid.
                out.extend(rod(corner(x0, 0.0, z0), corner(x1, 0.0, z1), w, LASER, 3.0));
                out.extend(rod(corner(x0, top, z0), corner(x1, top, z1), w, LASER, 3.0));
                // Uprights.
                out.extend(rod(corner(x0, 0.0, z0), corner(x0, top, z0), w, LASER, 3.0));
            }
        }
        // The scan plane, rising through the hull: a lattice of green bars.
        let scan = t / SCAN_TIME;
        if scan < 1.0 {
            let y = top * smooth(0.0, 1.0, scan);
            for k in 0..=6 {
                let z = -1.0 + 2.0 * k as f32 / 6.0;
                out.extend(rod(corner(-1.0, y, z), corner(1.0, y, z), 0.05, LASER, 4.0));
            }
            for x in [-1.0, 1.0] {
                out.extend(rod(corner(x, y, -1.0), corner(x, y, 1.0), 0.09, LASER, 5.0));
            }
        }
        // Tungsten cubes dropping into the volume, each taken in (shrinking
        // away) as the scan passes it.
        let n = 20;
        for k in 0..n {
            let kf = k as f32;
            let land = 0.08 + 0.35 * hash(kf, self.seed);
            let x = hash(kf + 0.3, self.seed) * 2.0 - 1.0;
            let z = hash(kf + 0.7, self.seed) * 2.0 - 1.0;
            let rest = 0.15 + hash(kf + 0.9, self.seed) * (top - 0.3);
            // Taken when the scan reaches its height.
            let taken = SCAN_TIME * (rest / top).sqrt();
            let gone = span(taken, taken + 0.15, t);
            if gone >= 1.0 {
                continue;
            }
            let fall = span(land - 0.25, land, t);
            if fall <= 0.0 {
                continue;
            }
            let y = rest + 6.0 * (1.0 - fall) * (1.0 - fall);
            let size = 0.38 * (1.0 - gone);
            let spin = Quat::from_rotation_y(hash(kf + 0.1, self.seed) * 1.5);
            out.push(inst(
                MeshId::CUBE,
                Vec3::splat(size),
                face * spin,
                corner(x * 0.8, y, z * 0.8),
                (TUNGSTEN * (1.0 + 3.0 * gone)).extend(0.0),
            ));
        }
    }

    fn uthenn_glow(&self, out: &mut Vec<Particle>) {
        let scan = self.t / SCAN_TIME;
        if scan >= 1.25 {
            return;
        }
        let (hx, hz, top) = self.volume();
        let face = Quat::from_rotation_y(self.tank.yaw());
        let y = top * smooth(0.0, 1.0, scan.min(1.0));
        let at = self.base() + Vec3::Y * y;
        let s = 1.0 - span(1.0, 1.25, scan);
        // The plane's light, spread over the volume's footprint.
        for (x, z) in [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)] {
            out.push(Particle::glow(
                at + face * Vec3::new(x * hx, 0.0, z * hz),
                1.4,
                LASER,
                0.9 * s,
                face * Vec3::Z * 1.5,
            ));
        }
    }

    // --- Ithessar: lowered on plasma ----------------------------------------

    fn orb(&self) -> Vec3 {
        self.base() + Vec3::Y * (DESCENT_HEIGHT + self.tank.chassis.hull_top + 2.2)
    }

    fn ithessar(&self, out: &mut Vec<(MeshId, Instance)>) {
        // The orb's white-hot heart, until it bursts.
        let t = self.t;
        if t < DESCENT_TIME {
            let s = smooth(0.0, 0.3, t) * (1.0 + 0.1 * (t * 40.0).sin());
            out.push(inst(
                MeshId::SPHERE,
                Vec3::splat(0.7 * s),
                Quat::IDENTITY,
                self.orb(),
                PLASMA_HOT.extend(6.0),
            ));
        }
    }

    fn ithessar_glow(&self, out: &mut Vec<Particle>) {
        let glow = Particle::glow;
        let t = self.t;
        let orb = self.orb();
        let base = self.base();
        let shown = base + Vec3::Y * DESCENT_HEIGHT * (1.0 - smooth(0.0, DESCENT_TIME, t));
        if t < DESCENT_TIME {
            let open = smooth(0.0, 0.3, t);
            let shiver = 1.0 + 0.1 * (t * 33.0 + self.seed * 9.0).sin();
            out.push(glow(orb, 2.6 * open * shiver, PLASMA, 1.8, Vec3::ZERO));
            out.push(glow(orb, 1.1 * open, PLASMA_HOT, 2.4, Vec3::ZERO));
            // The stream it lowers the tank on.
            let mid = (orb + shown) * 0.5 + Vec3::Y * 0.6;
            let len = (orb.y - shown.y).max(0.5);
            for k in 0..3 {
                let flick = 0.8 + 0.2 * (t * 25.0 + k as f32 * 2.1).sin();
                out.push(glow(
                    mid,
                    0.7 - 0.15 * k as f32,
                    PLASMA,
                    1.4 * open * flick,
                    Vec3::Y * len * 0.8,
                ));
            }
            // The cushion it rides down on.
            out.push(glow(
                shown + Vec3::Y * 0.2,
                2.4,
                PLASMA,
                1.2 * open,
                Vec3::ZERO,
            ));
        } else {
            // The orb bursts: a flash and droplets thrown out, arcing and
            // winking out, and a ring rolling out from under the tank.
            let f = span(DESCENT_TIME, ARRIVAL_TIME, t);
            let fade = (1.0 - f) * (1.0 - f);
            out.push(glow(orb, 2.6 + 4.0 * f, PLASMA, 2.4 * fade, Vec3::ZERO));
            out.push(glow(orb, 1.2 + 1.0 * f, PLASMA_HOT, 3.0 * fade, Vec3::ZERO));
            let n = 9;
            for k in 0..n {
                let a = (k as f32 + self.seed) / n as f32 * std::f32::consts::TAU;
                let dir = Vec3::new(a.cos(), 0.5 + 0.4 * (a * 3.0).sin(), a.sin()).normalize();
                let travel = 6.0 * (1.0 - (1.0 - f).powi(2));
                let at = orb + dir * travel - Vec3::Y * f * f * 4.0;
                out.push(glow(at, 0.35, PLASMA_HOT, 2.5 * (1.0 - f), dir * 2.0));
            }
            let r = 1.5 + 7.0 * (1.0 - (1.0 - f).powi(2));
            let n = 28;
            for k in 0..n {
                let a = k as f32 / n as f32 * std::f32::consts::TAU;
                out.push(glow(
                    base + Vec3::new(a.cos() * r, 0.25, a.sin() * r),
                    0.4,
                    PLASMA,
                    1.4 * (1.0 - f),
                    Vec3::new(-a.sin(), 0.0, a.cos()) * 1.5,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::SPAWN_SHIELD;

    #[test]
    fn arrivals_are_over_inside_the_spawn_shield() {
        assert!(ARRIVAL_TIME < SPAWN_SHIELD);
        assert!(DROP_TIME < ARRIVAL_TIME && DESCENT_TIME < ARRIVAL_TIME);
        assert!(SCAN_TIME < ARRIVAL_TIME && TOLL < ARRIVAL_TIME);
    }
}
