//! Where the war is: a stretch of a spiral galaxy's disk.
//!
//! The galaxy is a four-armed logarithmic spiral, its stars thinning out
//! from the center (an exponential disk) and away from the midplane (a thin
//! disk, a few hundred light years thick). The war's region is a slab of it
//! some 26,000 ly out from the center, about where the Sun is in ours:
//! west to east it runs outward across two arms and the gap between them,
//! with a spur (a lesser branch, like the Sun's own Orion spur) bridging the
//! gap. Colonized systems sit in clumps (the star-forming complexes they grew
//! up around) strung along the arms, thinly in the gap.
//!
//! **Local frame.** `x` points outward from the galaxy's center (west is
//! inward), `y` along the direction of rotation, `z` out of the disk; the
//! region's corner is at the origin and the midplane at `z = 0`.

use glam::{Vec2, Vec3};

use crate::rng::Rng;

/// How far the region's center is from the galaxy's, light years.
pub const RADIUS: f32 = 26_000.0;
/// Spiral arms.
const ARMS: f32 = 4.0;
/// How tightly the arms wind: the angle between an arm and a circle.
const PITCH_DEG: f32 = 7.0;
/// Stars thin out by `e` this far further from the center.
const SCALE_LENGTH: f32 = 11_000.0;
/// ... and this far from the midplane (the young thin disk).
const SCALE_HEIGHT: f32 = 300.0;
/// How sharp the arms are (a higher power, a narrower arm).
const ARM_SHARPNESS: i32 = 4;
/// Density between the arms, against an arm's crest.
const GAP_FLOOR: f32 = 0.06;
/// The spur bridging the gap: its density against an arm's, and half-width (ly).
const SPUR: f32 = 0.45;
const SPUR_WIDTH: f32 = 320.0;
/// Systems that grew up around the same complex: the share of systems in
/// clumps, how many systems a clump holds on average, and its spread (ly).
const CLUMPED: f32 = 0.75;
const CLUMP_SIZE: usize = 30;
const CLUMP_SPREAD: Vec3 = Vec3::new(160.0, 160.0, 70.0);

/// The shape of the galaxy around a region `size` (ly, in the plane).
#[derive(Clone, Copy, Debug)]
pub struct Disk {
    size: Vec2,
    /// Turns the arms so one crests near the region's west edge.
    phase: f32,
    /// The spur: a point it passes through (local), and its direction.
    spur_at: Vec2,
    spur_dir: Vec2,
}

impl Disk {
    pub fn new(size: Vec2, rng: &mut Rng) -> Disk {
        let b = PITCH_DEG.to_radians().tan();
        // The inner arm crests a fifth of the way across (with some play),
        // so the region holds it, the gap, and the next arm out.
        let crest = RADIUS + size.x * (rng.range(0.17, 0.23) - 0.5);
        let phase = ARMS * (crest / RADIUS).ln() / b;
        let spur_at = size * Vec2::new(rng.range(0.4, 0.6), rng.range(0.35, 0.65));
        let angle =
            rng.range(35.0f32, 55.0).to_radians() * if rng.chance(0.5) { 1.0 } else { -1.0 };
        Disk {
            size,
            phase,
            spur_at,
            spur_dir: Vec2::new(angle.cos(), angle.sin()),
        }
    }

    /// A local point (in the plane) in the galaxy's own frame, center at the origin.
    pub fn galactic(&self, p: Vec2) -> Vec2 {
        Vec2::new(RADIUS - self.size.x / 2.0 + p.x, p.y - self.size.y / 2.0)
    }

    /// The galaxy's density of stars at a point of its plane (its own
    /// frame), relative to an arm's crest at the region's distance.
    pub fn density(&self, g: Vec2) -> f32 {
        let r = g.length().max(1.0);
        let theta = g.y.atan2(g.x);
        let b = PITCH_DEG.to_radians().tan();
        let psi = ARMS * (theta - (r / RADIUS).ln() / b) + self.phase;
        let arm = ((1.0 + psi.cos()) / 2.0).powi(ARM_SHARPNESS);
        (GAP_FLOOR + arm) * (-(r - RADIUS) / SCALE_LENGTH).exp()
    }

    /// The density at a local point, the spur included.
    fn local_density(&self, p: Vec2) -> f32 {
        let d = p - self.spur_at;
        let off = d.perp_dot(self.spur_dir);
        let spur = SPUR * (-(off / SPUR_WIDTH).powi(2)).exp();
        let falloff = (-(self.galactic(p).length() - RADIUS) / SCALE_LENGTH).exp();
        self.density(self.galactic(p)) + spur * falloff
    }

    /// `count` systems scattered by the galaxy's density over the region.
    pub fn scatter(&self, count: usize, rng: &mut Rng) -> Vec<Vec3> {
        // The densest the region gets, for rejection sampling (with room to spare).
        let peak = (self.size.x / 2.0 / SCALE_LENGTH).exp() * (1.0 + GAP_FLOOR + SPUR) * 1.1;
        let size = self.size;
        let field = |rng: &mut Rng| loop {
            let p = Vec2::new(rng.f32(), rng.f32()) * size;
            if rng.f32() * peak < self.local_density(p) {
                break Vec3::new(p.x, p.y, height(rng));
            }
        };
        let clumps: Vec<Vec3> = (0..(count / CLUMP_SIZE).max(1))
            .map(|_| field(rng))
            .collect();
        let mut out = Vec::with_capacity(count);
        while out.len() < count {
            let p = if rng.f32() < CLUMPED {
                let c = clumps[rng.index(clumps.len())];
                c + Vec3::new(rng.normal(), rng.normal(), rng.normal()) * CLUMP_SPREAD
            } else {
                field(rng)
            };
            if p.x >= 0.0 && p.y >= 0.0 && p.x < size.x && p.y < size.y {
                out.push(p);
            }
        }
        out
    }
}

/// A height above or below the midplane, from the thin disk's profile.
fn height(rng: &mut Rng) -> f32 {
    let h = -SCALE_HEIGHT * (1.0 - rng.f32()).max(1e-6).ln();
    if rng.chance(0.5) { h } else { -h }
}
