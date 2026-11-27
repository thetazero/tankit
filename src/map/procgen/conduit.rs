//! What joins an Ithessar great building to the plasma river: a conduit
//! carried overhead, down the way to the quay and out over the channel's
//! wall, on portals that straddle the way. The Sun's runs down from its orb
//! and pours into the river; the Weir's is drawn up out of it.

use std::f32::consts::FRAC_PI_2;

use glam::{Vec2, Vec3};

use super::channel::{Channel, WALL};
use super::skeleton::*;
use crate::map::{Ambient, Map};
use crate::render::PointLight;
use crate::render::material::Material;
use crate::tank::ithessar::PLASMA;

/// How far past its building's ring the steep stretch ends.
const CLEAR: f32 = 6.0;
/// Its portals are at least this far apart.
const BAY: f32 = 13.0;
/// Half a leg's width.
const LEG: f32 = 0.7;

#[derive(Clone, Debug)]
pub struct Conduit {
    /// Which great building's it is.
    pub landmark: usize,
    /// Out from the building's middle to over the river's edge.
    pub way: [Vec2; 3],
    /// Where along the river it ends.
    pub t: f32,
    /// How far along its way each portal is, and how far its legs are apart
    /// from the middle.
    pub portals: Vec<(f32, f32)>,
}

impl Conduit {
    pub fn len(&self) -> f32 {
        self.way[0].distance(self.way[1]) + self.way[1].distance(self.way[2])
    }

    /// `s` m along it, and which way it's going.
    pub fn point(&self, s: f32) -> (Vec2, Vec2) {
        let [a, b, c] = self.way;
        let first = a.distance(b);
        if s < first {
            let d = (b - a).normalize();
            (a + d * s, d)
        } else {
            let d = (c - b).normalize();
            (b + d * (s - first), d)
        }
    }

    pub fn legs(&self) -> Vec<Vec2> {
        self.portals
            .iter()
            .flat_map(|&(s, half)| {
                let (p, d) = self.point(s);
                [p + d.perp() * half, p - d.perp() * half]
            })
            .collect()
    }
}

/// A leg at `p` stands in nobody's way.
fn out_of_the_way(sk: &Skeleton, p: Vec2) -> bool {
    sk.streets.iter().all(|s| {
        let keep = if s.rank == Rank::Avenue {
            s.clear + 1.0
        } else {
            s.half
        };
        s.distance(p) > keep + LEG
    })
}

/// Plans one from each Sun and Weir, and clears what's under it.
pub fn plan(sk: &mut Skeleton) {
    let Some(c) = sk.land.channel.clone() else {
        return;
    };
    for k in 0..sk.landmarks.len() {
        if sk.landmarks[k].kind != LandmarkKind::Great(1)
            && let Some(conduit) = route(sk, &c, k)
        {
            let legs = conduit.legs();
            let n = conduit.len().ceil() as i32;
            let line: Vec<Vec2> = (0..=n).map(|s| conduit.point(s as f32).0).collect();
            sk.lots.retain(|l| {
                matches!(l.kind, LotKind::Civic(_))
                    || (line.iter().all(|&p| l.footprint.distance(p) > 3.0)
                        && legs.iter().all(|&p| l.footprint.distance(p) > 2.0))
            });
            sk.spans
                .retain(|s| line.iter().all(|p| p.distance(s.at) > 5.0));
            sk.decos.retain(|d| {
                legs.iter()
                    .all(|p| p.distance(d.at) > d.kind.reach() + LEG + 1.5)
            });
            sk.conduits.push(conduit);
        }
    }
}

fn route(sk: &Skeleton, c: &Channel, landmark: usize) -> Option<Conduit> {
    let lm = &sk.landmarks[landmark];
    // Its first door's toward the quay.
    let d = *lm.doors.first()?;
    let to = (0..200)
        .map(|s| lm.center + d * s as f32)
        .find(|&p| c.wet(p, -WALL))?;
    let (mid, off) = c.local(to);
    // Straight on, or as little aside as clears the bridges.
    let t = (0..30)
        .flat_map(|k| [mid + k as f32 * 2.0, mid - k as f32 * 2.0])
        .find(|t| c.bridges.iter().all(|b| (b.t - t).abs() > b.half + 5.0))?;
    let mut conduit = Conduit {
        landmark,
        way: [
            lm.center,
            lm.center + d * (lm.radius + CLEAR),
            c.point(t, off.signum() * (c.half - 1.6)),
        ],
        t,
        portals: Vec::new(),
    };
    // The last stands on the channel's wall, the rest beside the way.
    let last = conduit.len() - 5.0;
    let half = sk
        .avenues()
        .filter(|s| s.distance(conduit.way[1]) < s.half)
        .map(|s| s.half)
        .fold(4.0, f32::max)
        + 1.2;
    let mut s = lm.radius + CLEAR;
    while s < last - BAY * 0.6 {
        let (p, d) = conduit.point(s);
        if [half, -half].iter().all(|&e| {
            let q = p + d.perp() * e;
            out_of_the_way(sk, q) && !c.wet(q, 1.0)
        }) {
            conduit.portals.push((s, half));
            s += BAY;
        } else {
            s += 1.0;
        }
    }
    conduit.portals.push((last, 2.0));
    Some(conduit)
}

/// Builds it: `from` is how far out from its building's middle it starts,
/// and how high over its floor there, past the ring and (over the quay) at
/// the river.
pub fn build(map: &mut Map, sk: &Skeleton, conduit: &Conduit, from: f32, high: [f32; 3]) {
    let Some(c) = &sk.land.channel else {
        return;
    };
    let lm = &sk.landmarks[conduit.landmark];
    let base = sk.ground(lm.center);
    let bend = conduit.way[0].distance(conduit.way[1]);
    let (len, quay) = (conduit.len(), c.quay_at(conduit.t));
    let levels = [base + high[0], base + high[1], quay + high[2]];
    let y = |s: f32| {
        if s < bend {
            levels[0] + (levels[1] - levels[0]) * ((s - from) / (bend - from))
        } else {
            levels[1] + (levels[2] - levels[1]) * ((s - bend) / (len - bend))
        }
    };
    let at = |s: f32| {
        let (p, _) = conduit.point(s);
        Vec3::new(p.x, y(s), p.y)
    };
    let glass = Vec3::new(0.05, 0.065, 0.11);
    let (dark, glow) = (Material::Vhaal, Material::Glow);
    for (a, b) in [(from, bend), (bend, len)] {
        // The plasma, between two rails.
        map.pipe(at(a), at(b), 0.62, PLASMA * 0.85, glow, false);
        let (_, d) = conduit.point((a + b) * 0.5);
        let side = Vec3::new(-d.y, 0.0, d.x);
        for e in [-0.95f32, 0.95] {
            map.pipe(
                at(a) + side * e,
                at(b) + side * e,
                0.3,
                glass * 1.6,
                dark,
                false,
            );
        }
    }
    // Collars holding rails and plasma together, and the portals under it.
    let mut s = from + 4.0;
    while s < len - 2.0 {
        let (p, d) = conduit.point(s);
        map.block(
            p,
            Vec2::new(1.35, 0.22),
            -d.to_angle() + FRAC_PI_2,
            (y(s) - 0.95, y(s) - 0.68),
            glass,
            dark,
            false,
        );
        s += 6.0;
    }
    let stone = Vec3::new(0.2, 0.21, 0.25);
    for &(s, half) in &conduit.portals {
        let (p, d) = conduit.point(s);
        let yaw = -d.to_angle() + FRAC_PI_2;
        let top = y(s);
        map.block(
            p,
            Vec2::new(half + LEG * 0.8, 0.5),
            yaw,
            (top - 1.75, top - 1.0),
            glass * 1.6,
            dark,
            false,
        );
        for e in [-half, half] {
            let q = p + d.perp() * e;
            let g = sk
                .ground(q)
                .max(if c.wet(q, 0.0) { quay } else { f32::MIN });
            map.block(
                q,
                Vec2::splat(LEG),
                yaw,
                (g - 1.5, top + 0.9),
                stone,
                Material::Concrete,
                true,
            )
            .chip = 0.01;
        }
    }
    // Over the river: its head, and the plasma between that and the surface.
    let (p, d) = conduit.point(len);
    let (head, surface) = (at(len), c.surface_at(conduit.t));
    map.block(
        p,
        Vec2::new(1.5, 1.3),
        -d.to_angle() + FRAC_PI_2,
        (head.y - 0.8, head.y + 1.0),
        glass * 1.6,
        dark,
        false,
    );
    map.pipe(
        Vec3::new(p.x, surface - 0.4, p.y),
        head - Vec3::Y * 0.5,
        0.85,
        PLASMA * 0.9,
        glow,
        false,
    );
    map.lights.push(PointLight {
        pos: Vec3::new(p.x, quay + 2.5, p.y),
        color: PLASMA * 12.0,
        radius: 22.0,
    });
    map.sound(Ambient::River, Vec3::new(p.x, surface + 1.2, p.y));
}
