//! Procedural tank kit geometry. Everything is assembled from beveled armor
//! panels (`plate`) and chamfered lofted solids (`solid`), flat shaded, so
//! the vehicle reads as physically built from separate plates.
//!
//! Local frame for every part: +z forward, +y up, x across. Hulls sit on the
//! ground at y = 0; turrets are built around their yaw ring at the origin;
//! guns around their pitch pivot.

use crate::render::material::Material;
use std::f32::consts::TAU;

use glam::{Vec2, Vec3, Vec4};

use super::track::Wheel;
use crate::render::mesh::MeshData;

#[derive(Clone, Copy)]
pub struct Skin {
    face: Vec4,
    edge: Vec4,
}

pub const CARBON: Skin = Skin {
    face: Vec4::new(0.034, 0.037, 0.042, Material::Carbon.id()),
    edge: Vec4::new(0.09, 0.095, 0.1, Material::Carbon.id()),
};
pub const CARBON_LIGHT: Skin = Skin {
    face: Vec4::new(0.075, 0.08, 0.088, Material::Carbon.id()),
    edge: Vec4::new(0.15, 0.16, 0.17, Material::Carbon.id()),
};
pub const PAINT: Skin = Skin {
    face: Vec4::new(0.9, 0.9, 0.9, Material::Paint.id()),
    edge: Vec4::new(0.9, 0.9, 0.9, Material::Worn.id()),
};
pub const METAL: Skin = Skin {
    face: Vec4::new(0.36, 0.37, 0.39, Material::Metal.id()),
    edge: Vec4::new(0.5, 0.5, 0.52, Material::Metal.id()),
};
pub const TREAD: Skin = Skin {
    face: Vec4::new(0.15, 0.15, 0.16, Material::Tread.id()),
    edge: Vec4::new(0.24, 0.24, 0.25, Material::Tread.id()),
};
pub const RUBBER: Skin = Skin {
    face: Vec4::new(0.06, 0.06, 0.065, Material::Carbon.id()),
    edge: Vec4::new(0.1, 0.1, 0.11, Material::Carbon.id()),
};
pub const GLOW: Skin = Skin {
    face: Vec4::new(1.0, 0.2, 0.03, Material::Glow.id()),
    edge: Vec4::new(1.0, 0.2, 0.03, Material::Glow.id()),
};
pub const BORE: Skin = Skin {
    face: Vec4::new(0.01, 0.01, 0.012, Material::Carbon.id()),
    edge: Vec4::new(0.01, 0.01, 0.012, Material::Carbon.id()),
};

// ---------------------------------------------------------------------------
// Shape kit

/// Quad outline with each corner cut by `k`: 8 points, and the odd-indexed
/// sides (point i -> i+1) are the corner chamfers.
fn cut_quad<V>(c: [V; 4], k: f32) -> Vec<V>
where
    V: Copy
        + std::ops::Add<Output = V>
        + std::ops::Sub<Output = V>
        + std::ops::Mul<f32, Output = V>
        + Norm,
{
    let mut out = Vec::with_capacity(8);
    for i in 0..4 {
        let (p, cur, n) = (c[(i + 3) % 4], c[i], c[(i + 1) % 4]);
        out.push(cur + (p - cur).unit() * k);
        out.push(cur + (n - cur).unit() * k);
    }
    out.rotate_left(1);
    out
}

trait Norm {
    fn unit(self) -> Self;
}
impl Norm for Vec2 {
    fn unit(self) -> Self {
        self.normalize_or_zero()
    }
}
impl Norm for Vec3 {
    fn unit(self) -> Self {
        self.normalize_or_zero()
    }
}

/// Trapezoid cross-section (x, y): bottom half-width `hb`, top half-width `ht`.
pub fn trap(hb: f32, ht: f32, y0: f32, y1: f32, k: f32) -> Vec<Vec2> {
    cut_quad(
        [
            Vec2::new(-hb, y0),
            Vec2::new(hb, y0),
            Vec2::new(ht, y1),
            Vec2::new(-ht, y1),
        ],
        k,
    )
}

/// Rectangle cross-section centered at `c` with half extents `h`.
pub fn rect(c: Vec2, h: Vec2, k: f32) -> Vec<Vec2> {
    cut_quad(
        [
            c - h,
            c + Vec2::new(h.x, -h.y),
            c + h,
            c + Vec2::new(-h.x, h.y),
        ],
        k,
    )
}

/// Regular polygon of radius `r`, rotated so a flat side faces up.
pub fn ngon(n: usize, r: f32, c: Vec2) -> Vec<Vec2> {
    let off = if n.is_multiple_of(2) {
        TAU / (2 * n) as f32
    } else {
        0.0
    };
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * TAU + off;
            c + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect()
}

/// Cross-section in the XY plane at depth z.
pub fn at_z(p: &[Vec2], z: f32) -> Vec<Vec3> {
    p.iter().map(|q| Vec3::new(q.x, q.y, z)).collect()
}

/// Cross-section in the ZY plane (p = (z, y)) at offset x.
pub fn at_x(p: &[Vec2], x: f32) -> Vec<Vec3> {
    p.iter().map(|q| Vec3::new(x, q.y, q.x)).collect()
}

/// Cross-section in the XZ plane (p = (x, z)) at height y.
pub fn at_y(p: &[Vec2], y: f32) -> Vec<Vec3> {
    p.iter().map(|q| Vec3::new(q.x, y, q.y)).collect()
}

fn centroid(r: &[Vec3]) -> Vec3 {
    r.iter().copied().sum::<Vec3>() / r.len() as f32
}

fn shrink(r: &[Vec3], d: f32) -> Vec<Vec3> {
    let c = centroid(r);
    r.iter()
        .map(|&p| {
            let v = c - p;
            p + v.normalize_or_zero() * d.min(v.length() * 0.9)
        })
        .collect()
}

/// Lofted solid through `rings` with beveled ends. Chamfer sides of `cut_quad`
/// outlines (odd sides) and the end bevels use the skin's edge color, so
/// edges catch light (and show wear on paint).
pub fn solid(m: &mut MeshData, rings: &[Vec<Vec3>], bevel: f32, skin: Skin) {
    solid_with(m, rings, bevel, skin, true);
}

fn solid_with(m: &mut MeshData, rings: &[Vec<Vec3>], bevel: f32, skin: Skin, odd_edges: bool) {
    let n = rings.len();
    let (c0, c1) = (centroid(&rings[0]), centroid(&rings[1]));
    let (cl, cp) = (centroid(&rings[n - 1]), centroid(&rings[n - 2]));
    let d0 = (c1 - c0).normalize_or_zero();
    let d1 = (cl - cp).normalize_or_zero();
    let mut seq = Vec::with_capacity(n + 2);
    seq.push(shrink(&rings[0], bevel));
    for (i, r) in rings.iter().enumerate() {
        let off = if i == 0 {
            d0 * bevel
        } else if i == n - 1 {
            -d1 * bevel
        } else {
            Vec3::ZERO
        };
        seq.push(r.iter().map(|&p| p + off).collect::<Vec<_>>());
    }
    seq.push(shrink(&rings[n - 1], bevel));

    let inside = rings.iter().map(|r| centroid(r)).sum::<Vec3>() / n as f32;
    let k = seq[0].len();
    let last = seq.len() - 2;
    for (bi, w) in seq.windows(2).enumerate() {
        // Bending lofts (none so far) would want a per-band center; ours are convex.
        let band_inside = (centroid(&w[0]) + centroid(&w[1])) * 0.5;
        let inside = band_inside.lerp(inside, 0.5);
        for j in 0..k {
            let jn = (j + 1) % k;
            let end_band = bi == 0 || bi == last;
            let c = if end_band || (odd_edges && j % 2 == 1) {
                skin.edge
            } else {
                skin.face
            };
            let (a, b, cc, d) = (w[0][j], w[0][jn], w[1][jn], w[1][j]);
            m.facet([a, b, cc], inside, c);
            m.facet([a, cc, d], inside, c);
        }
    }
    for (ring, other) in [
        (&seq[0], &seq[1]),
        (&seq[seq.len() - 1], &seq[seq.len() - 2]),
    ] {
        let centre = centroid(ring);
        let away = centroid(other);
        let mut nrm = Vec3::ZERO;
        for j in 0..k {
            nrm += (ring[j] - centre).cross(ring[(j + 1) % k] - centre);
        }
        let mut nrm = nrm.normalize_or_zero();
        if nrm.dot(centre - away) < 0.0 {
            nrm = -nrm;
        }
        for j in 0..k {
            m.tri([centre, ring[j], ring[(j + 1) % k]], nrm, skin.face);
        }
    }
}

/// Round solid (wheels, barrels): same as `solid`, but all sides use the face color.
pub fn round(m: &mut MeshData, rings: &[Vec<Vec3>], bevel: f32, skin: Skin) {
    solid_with(m, rings, bevel, skin, false);
}

/// An armor plate: `top` is its outer face (4 corners, in order), extruded
/// `t` toward `inside`. Corners are cut and the outer face is beveled.
pub fn plate(m: &mut MeshData, top: [Vec3; 4], t: f32, inside: Vec3, skin: Skin) {
    let mut n = (top[2] - top[0]).cross(top[3] - top[1]).normalize_or_zero();
    let c = (top[0] + top[1] + top[2] + top[3]) * 0.25;
    if n.dot(c - inside) < 0.0 {
        n = -n;
    }
    let k = (t * 0.6).min(0.05);
    let outer = cut_quad(top, k);
    let inner: Vec<Vec3> = outer.iter().map(|&p| p - n * t).collect();
    solid(m, &[inner, outer], (t * 0.3).min(0.025), skin);
}

/// Axis-aligned beveled box.
pub fn block(m: &mut MeshData, min: Vec3, max: Vec3, bevel: f32, skin: Skin) {
    let c = Vec2::new((min.x + max.x) * 0.5, (min.y + max.y) * 0.5);
    let h = Vec2::new((max.x - min.x) * 0.5, (max.y - min.y) * 0.5);
    let r = rect(c, h, bevel.min(h.x * 0.5).min(h.y * 0.5));
    solid(m, &[at_z(&r, min.z), at_z(&r, max.z)], bevel, skin);
}

fn mirror(p: [Vec3; 4]) -> [Vec3; 4] {
    p.map(|v| Vec3::new(-v.x, v.y, v.z))
}

fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
}

/// A plate and its mirror image on the other side.
fn plate2(m: &mut MeshData, top: [Vec3; 4], t: f32, inside: Vec3, skin: Skin) {
    plate(m, top, t, inside, skin);
    plate(m, mirror(top), t, inside * Vec3::new(-1.0, 1.0, 1.0), skin);
}

fn block2(m: &mut MeshData, min: Vec3, max: Vec3, bevel: f32, skin: Skin) {
    block(m, min, max, bevel, skin);
    block(
        m,
        v(-max.x, min.y, min.z),
        v(-min.x, max.y, max.z),
        bevel,
        skin,
    );
}

// ---------------------------------------------------------------------------
// Running gear

/// One tread link in belt space: x across, y = outward from the wheels, z = along the belt.
pub fn link(width: f32, pitch: f32) -> MeshData {
    let mut m = MeshData::default();
    let hw = width * 0.5;
    let hl = pitch * 0.43;
    // Shoe plate.
    block(&mut m, v(-hw, -0.035, -hl), v(hw, 0.0, hl), 0.012, TREAD);
    // Chevron grousers: two raised bars angled back from the center line.
    for s in [-1.0f32, 1.0] {
        let bar =
            |x: f32, z: f32| at_x(&rect(Vec2::new(z, 0.02), Vec2::new(0.022, 0.02), 0.008), x);
        let (x0, x1) = (s * 0.015, s * (hw - 0.02));
        let mut rings = [bar(x0, 0.03), bar(x1, -0.035)];
        if s < 0.0 {
            rings.reverse();
        }
        solid(&mut m, &rings, 0.006, TREAD);
        // End connectors where the link pins would be.
        block(
            &mut m,
            v(s * hw - 0.04, -0.03, -hl - 0.012),
            v(s * hw + 0.008, 0.005, -hl + 0.04),
            0.008,
            METAL,
        );
    }
    // Guide horn on the inside, running between the split road wheels.
    let horn = cut_quad(
        [
            Vec2::new(-0.055, -0.03),
            Vec2::new(0.055, -0.03),
            Vec2::new(0.03, -0.12),
            Vec2::new(-0.03, -0.12),
        ],
        0.012,
    );
    solid(
        &mut m,
        &[at_x(&horn, -0.03), at_x(&horn, 0.03)],
        0.008,
        METAL,
    );
    m
}

/// Split road wheel (two discs with a gap for the guide horns), axle along x,
/// outer face toward +x. Painted hub spokes make the rotation easy to see.
pub fn road_wheel(r: f32, width: f32) -> MeshData {
    let mut m = MeshData::default();
    let gap = 0.13;
    let dw = (width - gap) * 0.5;
    for s in [-1.0f32, 1.0] {
        let (xa, xb) = (s * gap * 0.5, s * (gap * 0.5 + dw));
        round(
            &mut m,
            &[
                at_x(&ngon(18, r, Vec2::ZERO), xa),
                at_x(&ngon(18, r, Vec2::ZERO), xb),
            ],
            0.025,
            RUBBER,
        );
        // Steel wheel face inset into the tire.
        let face = xb + s * 0.012;
        round(
            &mut m,
            &[
                at_x(&ngon(14, r * 0.78, Vec2::ZERO), xb),
                at_x(&ngon(14, r * 0.74, Vec2::ZERO), face),
            ],
            0.01,
            CARBON_LIGHT,
        );
    }
    let xo = gap * 0.5 + dw + 0.012;
    // Hub cap and three painted spokes on the outer face.
    round(
        &mut m,
        &[
            at_x(&ngon(8, r * 0.3, Vec2::ZERO), xo),
            at_x(&ngon(8, r * 0.24, Vec2::ZERO), xo + 0.05),
        ],
        0.012,
        METAL,
    );
    for i in 0..3 {
        let a = i as f32 / 3.0 * TAU;
        let d = Vec2::new(a.cos(), a.sin());
        let side = Vec2::new(-d.y, d.x);
        let (p0, p1) = (d * r * 0.3, d * r * 0.7);
        let q = |p: Vec2, w: f32, x: f32| v(x, p.y + side.y * w, p.x + side.x * w);
        plate(
            &mut m,
            [
                q(p0, 0.06, xo + 0.025),
                q(p1, 0.04, xo + 0.015),
                q(p1, -0.04, xo + 0.015),
                q(p0, -0.06, xo + 0.025),
            ],
            0.02,
            v(0.0, 0.0, 0.0),
            PAINT,
        );
    }
    m
}

/// Toothed drive sprocket.
pub fn sprocket(r: f32, width: f32) -> MeshData {
    let mut m = MeshData::default();
    let gap = 0.13;
    let dw = (width - gap) * 0.5;
    for s in [-1.0f32, 1.0] {
        let (xa, xb) = (s * gap * 0.5, s * (gap * 0.5 + dw * 0.7));
        round(
            &mut m,
            &[
                at_x(&ngon(20, r * 0.93, Vec2::ZERO), xa),
                at_x(&ngon(20, r * 0.93, Vec2::ZERO), xb),
            ],
            0.02,
            CARBON_LIGHT,
        );
        for i in 0..12 {
            let a = i as f32 / 12.0 * TAU;
            let d = Vec2::new(a.cos(), a.sin());
            let side = Vec2::new(-d.y, d.x) * 0.045;
            let (b0, b1) = (d * r * 0.85, d * (r + 0.045));
            let tooth = [b0 - side, b0 + side, b1 + side * 0.45, b1 - side * 0.45];
            let ring = |x: f32| tooth.iter().map(|p| v(x, p.y, p.x)).collect::<Vec<_>>();
            round(&mut m, &[ring(xa), ring(xb)], 0.006, METAL);
        }
    }
    let xo = gap * 0.5 + dw * 0.7;
    round(
        &mut m,
        &[
            at_x(&ngon(10, r * 0.55, Vec2::ZERO), xo),
            at_x(&ngon(10, r * 0.4, Vec2::ZERO), xo + 0.08),
        ],
        0.015,
        PAINT,
    );
    round(
        &mut m,
        &[
            at_x(&ngon(6, r * 0.2, Vec2::ZERO), xo + 0.07),
            at_x(&ngon(6, r * 0.16, Vec2::ZERO), xo + 0.12),
        ],
        0.01,
        METAL,
    );
    m
}

/// Plain idler wheel.
pub fn idler(r: f32, width: f32) -> MeshData {
    let mut m = MeshData::default();
    let gap = 0.13;
    let dw = (width - gap) * 0.5;
    for s in [-1.0f32, 1.0] {
        let (xa, xb) = (s * gap * 0.5, s * (gap * 0.5 + dw));
        round(
            &mut m,
            &[
                at_x(&ngon(18, r, Vec2::ZERO), xa),
                at_x(&ngon(18, r, Vec2::ZERO), xb),
            ],
            0.02,
            CARBON_LIGHT,
        );
    }
    let xo = gap * 0.5 + dw;
    round(
        &mut m,
        &[
            at_x(&ngon(6, r * 0.5, Vec2::ZERO), xo),
            at_x(&ngon(6, r * 0.4, Vec2::ZERO), xo + 0.05),
        ],
        0.012,
        METAL,
    );
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU;
        let p = Vec2::new(a.cos(), a.sin()) * r * 0.68;
        block(
            &mut m,
            v(xo - 0.01, p.y - 0.03, p.x - 0.03),
            v(xo + 0.03, p.y + 0.03, p.x + 0.03),
            0.01,
            METAL,
        );
    }
    m
}

// ---------------------------------------------------------------------------
// Hulls

/// Side-profile layout of one track.
#[derive(Clone, Debug)]
pub struct Running {
    /// Track center line offset from the hull center (both sides).
    pub track_x: f32,
    pub track_width: f32,
    pub sprocket: Wheel,
    pub idler: Wheel,
    pub road: Vec<Wheel>,
}

pub struct HullModel {
    pub body: MeshData,
    pub running: Running,
    /// Turret ring position.
    pub mount: Vec3,
}

/// Light, sleek hull: a low wedge with sharp white fender plates sweeping
/// forward over the sprockets and a dark composite core.
pub fn lancer() -> HullModel {
    let mut m = MeshData::default();
    let inside = v(0.0, 0.8, 0.0);

    // Lower tub between the tracks.
    solid(
        &mut m,
        &[
            at_z(&trap(0.74, 0.82, 0.45, 1.02, 0.08), -2.38),
            at_z(&trap(0.86, 0.9, 0.3, 1.1, 0.1), -1.9),
            at_z(&trap(0.86, 0.9, 0.3, 1.1, 0.1), 1.6),
            at_z(&trap(0.66, 0.76, 0.5, 0.92, 0.08), 2.45),
        ],
        0.04,
        CARBON,
    );
    // Upper deck spanning the tracks, wedge nose.
    solid(
        &mut m,
        &[
            at_z(&trap(1.32, 1.18, 0.98, 1.3, 0.06), -2.42),
            at_z(&trap(1.46, 1.32, 0.96, 1.42, 0.08), -1.95),
            at_z(&trap(1.46, 1.32, 0.96, 1.42, 0.08), 0.95),
            at_z(&trap(1.26, 0.98, 0.92, 1.25, 0.07), 1.95),
            at_z(&trap(0.8, 0.52, 0.86, 1.02, 0.05), 2.55),
        ],
        0.04,
        CARBON,
    );

    // White fenders over the tracks: rear-to-mid plate, then a forward slope.
    plate2(
        &mut m,
        [
            v(0.98, 1.46, -2.3),
            v(1.66, 1.27, -2.3),
            v(1.66, 1.27, -0.7),
            v(0.98, 1.46, -0.7),
        ],
        0.07,
        inside,
        PAINT,
    );
    plate2(
        &mut m,
        [
            v(0.98, 1.51, -0.74),
            v(1.68, 1.31, -0.74),
            v(1.68, 1.31, 1.05),
            v(0.98, 1.51, 1.05),
        ],
        0.07,
        inside,
        PAINT,
    );
    plate2(
        &mut m,
        [
            v(0.98, 1.5, 1.09),
            v(1.66, 1.3, 1.09),
            v(1.52, 0.98, 2.3),
            v(0.96, 1.2, 2.3),
        ],
        0.07,
        inside,
        PAINT,
    );
    // Claw plates angling down over the front of the sprockets.
    plate2(
        &mut m,
        [
            v(0.95, 1.16, 2.33),
            v(1.55, 0.95, 2.33),
            v(1.42, 0.5, 2.68),
            v(0.98, 0.6, 2.62),
        ],
        0.065,
        inside,
        PAINT,
    );
    // Dark inner fender liners under the white plates (visible from the front).
    plate2(
        &mut m,
        [
            v(0.94, 1.43, -2.2),
            v(1.6, 1.25, -2.2),
            v(1.6, 1.25, 1.9),
            v(0.94, 1.43, 1.9),
        ],
        0.04,
        inside,
        CARBON,
    );

    // Layered side skirts: white, dark, white, overlapping like real panels,
    // with raked lower edges so the side silhouette stays angular.
    let skirt = |z0: f32, z1: f32, x: f32, low0: f32, low1: f32| {
        [
            v(x, 1.27, z0),
            v(x, 1.27, z1),
            v(x + 0.08, low1, z1),
            v(x + 0.08, low0, z0),
        ]
    };
    plate2(
        &mut m,
        skirt(-2.32, -0.75, 1.64, 1.02, 0.78),
        0.05,
        inside,
        PAINT,
    );
    plate2(
        &mut m,
        skirt(-0.85, 0.55, 1.69, 0.72, 0.72),
        0.05,
        inside,
        CARBON_LIGHT,
    );
    plate2(
        &mut m,
        skirt(0.45, 1.8, 1.64, 0.78, 1.1),
        0.05,
        inside,
        PAINT,
    );
    // Angled armor wedge bridging the front skirt and the claw.
    plate2(
        &mut m,
        [
            v(1.62, 1.27, 1.75),
            v(1.5, 1.05, 2.3),
            v(1.52, 0.86, 2.2),
            v(1.72, 1.1, 1.75),
        ],
        0.05,
        inside,
        CARBON_LIGHT,
    );
    // Light strip along the dark middle skirt.
    plate2(
        &mut m,
        [
            v(1.75, 1.12, -0.6),
            v(1.75, 1.12, 0.3),
            v(1.757, 1.06, 0.3),
            v(1.757, 1.06, -0.6),
        ],
        0.015,
        inside,
        GLOW,
    );

    // Raised glacis plate with twin light bars.
    plate(
        &mut m,
        [
            v(-0.6, 1.42, 1.1),
            v(0.6, 1.42, 1.1),
            v(0.42, 1.11, 2.38),
            v(-0.42, 1.11, 2.38),
        ],
        0.06,
        inside,
        CARBON_LIGHT,
    );
    plate2(
        &mut m,
        [
            v(0.2, 1.46, 1.25),
            v(0.42, 1.46, 1.25),
            v(0.36, 1.32, 1.8),
            v(0.2, 1.32, 1.8),
        ],
        0.02,
        inside,
        GLOW,
    );
    // Nose: armored beak under the glacis.
    plate(
        &mut m,
        [
            v(-0.66, 1.0, 2.5),
            v(0.66, 1.0, 2.5),
            v(0.5, 0.62, 2.62),
            v(-0.5, 0.62, 2.62),
        ],
        0.06,
        inside,
        PAINT,
    );
    plate2(
        &mut m,
        [
            v(0.3, 0.92, 2.6),
            v(0.5, 0.9, 2.6),
            v(0.47, 0.82, 2.63),
            v(0.3, 0.84, 2.63),
        ],
        0.015,
        inside,
        GLOW,
    );

    // Engine deck: grille well with metal slats behind the turret.
    block(
        &mut m,
        v(-0.85, 1.38, -2.05),
        v(0.85, 1.45, -1.3),
        0.02,
        CARBON,
    );
    for i in 0..5 {
        let z = -1.98 + i as f32 * 0.15;
        block(
            &mut m,
            v(-0.8, 1.43, z),
            v(0.8, 1.48, z + 0.07),
            0.012,
            METAL,
        );
    }
    // Tail lights and exhaust boxes on the rear plate.
    plate2(
        &mut m,
        [
            v(0.7, 1.24, -2.45),
            v(1.15, 1.24, -2.45),
            v(1.15, 1.18, -2.45),
            v(0.7, 1.18, -2.45),
        ],
        0.015,
        inside,
        GLOW,
    );
    block2(&mut m, v(0.25, 0.7, -2.5), v(0.6, 0.9, -2.3), 0.03, METAL);

    HullModel {
        body: m,
        running: Running {
            track_x: 1.22,
            track_width: 0.56,
            sprocket: Wheel {
                center: Vec2::new(2.05, 0.66),
                radius: 0.36,
            },
            idler: Wheel {
                center: Vec2::new(-2.1, 0.62),
                radius: 0.32,
            },
            road: [-1.35f32, -0.45, 0.45, 1.35]
                .iter()
                .map(|&z| Wheel {
                    center: Vec2::new(z, 0.38),
                    radius: 0.3,
                })
                .collect(),
        },
        mount: v(0.0, 1.42, -0.3),
    }
}

/// Heavy hull: taller and boxier, with thick segmented skirts, a bull-nose
/// front plate and more hardware on the deck.
pub fn bastion() -> HullModel {
    let mut m = MeshData::default();
    let inside = v(0.0, 0.9, 0.0);

    solid(
        &mut m,
        &[
            at_z(&trap(0.8, 0.86, 0.42, 1.2, 0.08), -2.42),
            at_z(&trap(0.9, 0.94, 0.32, 1.28, 0.1), -2.0),
            at_z(&trap(0.9, 0.94, 0.32, 1.28, 0.1), 1.8),
            at_z(&trap(0.8, 0.88, 0.45, 1.08, 0.08), 2.45),
        ],
        0.04,
        CARBON,
    );
    solid(
        &mut m,
        &[
            at_z(&trap(1.5, 1.4, 1.08, 1.58, 0.08), -2.48),
            at_z(&trap(1.56, 1.46, 1.06, 1.62, 0.09), -2.1),
            at_z(&trap(1.56, 1.46, 1.06, 1.62, 0.09), 1.3),
            at_z(&trap(1.5, 1.3, 1.0, 1.36, 0.08), 2.3),
            at_z(&trap(1.3, 1.1, 0.95, 1.12, 0.06), 2.6),
        ],
        0.04,
        CARBON,
    );

    // Upper glacis: big white plate, split down the middle by a dark spine.
    plate2(
        &mut m,
        [
            v(0.18, 1.66, 1.25),
            v(1.42, 1.66, 1.25),
            v(1.24, 1.38, 2.35),
            v(0.16, 1.38, 2.35),
        ],
        0.08,
        inside,
        PAINT,
    );
    plate(
        &mut m,
        [
            v(-0.16, 1.7, 1.15),
            v(0.16, 1.7, 1.15),
            v(0.13, 1.4, 2.4),
            v(-0.13, 1.4, 2.4),
        ],
        0.06,
        inside,
        CARBON_LIGHT,
    );
    // Bull-nose lower plate with light slits.
    plate(
        &mut m,
        [
            v(-1.25, 1.12, 2.62),
            v(1.25, 1.12, 2.62),
            v(1.05, 0.6, 2.72),
            v(-1.05, 0.6, 2.72),
        ],
        0.08,
        inside,
        PAINT,
    );
    for s in [-1.0f32, 1.0] {
        plate(
            &mut m,
            [
                v(s * 0.35, 1.0, 2.68),
                v(s * 0.8, 1.0, 2.68),
                v(s * 0.78, 0.94, 2.69),
                v(s * 0.35, 0.94, 2.69),
            ],
            0.015,
            inside,
            GLOW,
        );
    }

    // Four heavy skirt panels per side, alternately stood off.
    for i in 0..4 {
        let z0 = -2.35 + i as f32 * 1.12;
        let z1 = z0 + 1.2;
        let x = if i % 2 == 0 { 1.66 } else { 1.71 };
        let low = if i == 3 { 0.62 } else { 0.5 };
        let skin = if i == 1 { CARBON_LIGHT } else { PAINT };
        plate2(
            &mut m,
            [
                v(x, 1.42, z0),
                v(x, 1.42, z1),
                v(x + 0.07, low, z1 - 0.12 * (i == 3) as i32 as f32),
                v(x + 0.07, 0.5, z0),
            ],
            0.08,
            inside,
            skin,
        );
        // Bolt heads along the top edge.
        for b in 0..3 {
            let z = z0 + 0.25 + b as f32 * 0.35;
            for s in [-1.0f32, 1.0] {
                let xb = s * (x + 0.012);
                let o = s * 0.04;
                block(
                    &mut m,
                    v(xb.min(xb + o), 1.3, z - 0.03),
                    v(xb.max(xb + o), 1.36, z + 0.03),
                    0.01,
                    METAL,
                );
            }
        }
    }
    plate2(
        &mut m,
        [
            v(1.81, 1.18, -1.1),
            v(1.81, 1.18, -0.2),
            v(1.815, 1.12, -0.2),
            v(1.815, 1.12, -1.1),
        ],
        0.015,
        inside,
        GLOW,
    );
    // Top of the skirts: armored mud guard shelf.
    plate2(
        &mut m,
        [
            v(1.36, 1.6, -2.4),
            v(1.8, 1.45, -2.4),
            v(1.8, 1.45, 2.2),
            v(1.36, 1.6, 2.2),
        ],
        0.06,
        inside,
        CARBON_LIGHT,
    );

    // Rear deck: two stowage boxes and a grille.
    block2(
        &mut m,
        v(0.55, 1.6, -2.35),
        v(1.25, 1.88, -1.55),
        0.04,
        CARBON_LIGHT,
    );
    plate2(
        &mut m,
        [
            v(0.6, 1.9, -2.3),
            v(1.2, 1.9, -2.3),
            v(1.2, 1.9, -1.6),
            v(0.6, 1.9, -1.6),
        ],
        0.03,
        inside + Vec3::Y,
        PAINT,
    );
    block(
        &mut m,
        v(-0.45, 1.58, -2.4),
        v(0.45, 1.66, -1.6),
        0.02,
        CARBON,
    );
    for i in 0..5 {
        let z = -2.32 + i as f32 * 0.15;
        block(
            &mut m,
            v(-0.4, 1.64, z),
            v(0.4, 1.7, z + 0.07),
            0.012,
            METAL,
        );
    }
    // Rear lights and exhausts.
    plate2(
        &mut m,
        [
            v(0.9, 1.45, -2.5),
            v(1.35, 1.45, -2.5),
            v(1.35, 1.38, -2.5),
            v(0.9, 1.38, -2.5),
        ],
        0.015,
        inside,
        GLOW,
    );
    for s in [-1.0f32, 1.0] {
        round(
            &mut m,
            &[
                at_z(&ngon(8, 0.11, Vec2::new(s * 0.5, 0.95)), -2.55),
                at_z(&ngon(8, 0.11, Vec2::new(s * 0.5, 0.95)), -2.3),
            ],
            0.02,
            METAL,
        );
    }

    HullModel {
        body: m,
        running: Running {
            track_x: 1.28,
            track_width: 0.64,
            sprocket: Wheel {
                center: Vec2::new(-2.15, 0.7),
                radius: 0.38,
            },
            idler: Wheel {
                center: Vec2::new(2.15, 0.68),
                radius: 0.33,
            },
            road: [-1.6f32, -0.8, 0.0, 0.8, 1.6]
                .iter()
                .map(|&z| Wheel {
                    center: Vec2::new(z, 0.4),
                    radius: 0.32,
                })
                .collect(),
        },
        mount: v(0.0, 1.62, -0.2),
    }
}

// ---------------------------------------------------------------------------
// Turrets

pub struct TurretModel {
    /// Rotates with turret yaw.
    pub head: MeshData,
    /// Pitches with the gun.
    pub mantlet: MeshData,
    /// One barrel; drawn once per entry in `barrels`, slides back on recoil.
    pub barrel: MeshData,
    /// Gun pitch pivot in turret space.
    pub pivot: Vec3,
    /// Barrel origins in gun space.
    pub barrels: Vec<Vec3>,
    /// Muzzle distance along +z from a barrel's origin.
    pub muzzle: f32,
    /// How far a barrel slides back when it fires.
    pub recoil: f32,
}

fn turret_ring(m: &mut MeshData) {
    round(
        m,
        &[
            at_y(&ngon(16, 0.86, Vec2::ZERO), -0.05),
            at_y(&ngon(16, 0.82, Vec2::ZERO), 0.12),
        ],
        0.02,
        CARBON,
    );
}

/// Long-barrel rail cannon: faceted wedge head, white cheek armor, glowing
/// coil slots along the barrel.
pub fn lance() -> TurretModel {
    let mut head = MeshData::default();
    let inside = v(0.0, 0.45, -0.2);
    turret_ring(&mut head);
    solid(
        &mut head,
        &[
            at_z(&trap(0.46, 0.38, 0.14, 0.6, 0.1), -1.35),
            at_z(&trap(0.82, 0.68, 0.1, 0.8, 0.14), -0.95),
            at_z(&trap(0.88, 0.74, 0.1, 0.82, 0.15), 0.4),
            at_z(&trap(0.5, 0.4, 0.18, 0.64, 0.1), 0.95),
        ],
        0.04,
        CARBON,
    );
    // Cheek armor: side slab and a forward-raked plate on each side.
    plate2(
        &mut head,
        [
            v(0.84, 0.74, -0.92),
            v(0.84, 0.74, 0.36),
            v(0.96, 0.22, 0.28),
            v(0.94, 0.22, -0.98),
        ],
        0.07,
        inside,
        PAINT,
    );
    plate2(
        &mut head,
        [
            v(0.84, 0.74, 0.4),
            v(0.52, 0.7, 0.98),
            v(0.54, 0.2, 1.0),
            v(0.96, 0.22, 0.32),
        ],
        0.07,
        inside,
        PAINT,
    );
    // Raised roof panel and a sensor pod with an eye.
    plate(
        &mut head,
        [
            v(-0.55, 0.86, -0.85),
            v(0.55, 0.86, -0.85),
            v(0.5, 0.86, 0.3),
            v(-0.5, 0.86, 0.3),
        ],
        0.05,
        inside,
        CARBON_LIGHT,
    );
    block(
        &mut head,
        v(0.25, 0.86, -1.0),
        v(0.62, 1.1, -0.55),
        0.04,
        CARBON,
    );
    plate(
        &mut head,
        [
            v(0.32, 1.05, -0.54),
            v(0.55, 1.05, -0.54),
            v(0.55, 0.92, -0.54),
            v(0.32, 0.92, -0.54),
        ],
        0.015,
        inside,
        GLOW,
    );
    // Rear antenna fins.
    for s in [-1.0f32, 1.0] {
        plate(
            &mut head,
            [
                v(s * 0.3, 0.86, -1.3),
                v(s * 0.3, 0.86, -0.95),
                v(s * 0.34, 1.25, -1.15),
                v(s * 0.34, 1.25, -1.32),
            ],
            0.03,
            v(0.0, 0.5, -1.1),
            PAINT,
        );
    }
    // Rear light bar.
    plate(
        &mut head,
        [
            v(-0.3, 0.5, -1.36),
            v(0.3, 0.5, -1.36),
            v(0.3, 0.44, -1.36),
            v(-0.3, 0.44, -1.36),
        ],
        0.015,
        inside,
        GLOW,
    );

    let mut mantlet = MeshData::default();
    solid(
        &mut mantlet,
        &[
            at_z(&rect(Vec2::ZERO, Vec2::new(0.28, 0.24), 0.07), -0.3),
            at_z(&rect(Vec2::ZERO, Vec2::new(0.24, 0.2), 0.06), 0.32),
        ],
        0.03,
        CARBON,
    );
    round(
        &mut mantlet,
        &[
            at_x(&ngon(10, 0.11, Vec2::ZERO), -0.48),
            at_x(&ngon(10, 0.11, Vec2::ZERO), 0.48),
        ],
        0.02,
        METAL,
    );
    // White yoke brackets either side of the gun.
    for s in [-1.0f32, 1.0] {
        plate(
            &mut mantlet,
            [
                v(s * 0.36, 0.28, -0.25),
                v(s * 0.36, 0.28, 0.25),
                v(s * 0.36, -0.18, 0.18),
                v(s * 0.36, -0.18, -0.28),
            ],
            0.07,
            Vec3::ZERO,
            PAINT,
        );
    }

    let mut barrel = MeshData::default();
    let oct = |r: f32| ngon(8, r, Vec2::ZERO);
    // Breech sleeve, then the long tube.
    solid(
        &mut barrel,
        &[at_z(&oct(0.2), 0.15), at_z(&oct(0.2), 1.3)],
        0.03,
        CARBON,
    );
    round(
        &mut barrel,
        &[at_z(&oct(0.145), 1.25), at_z(&oct(0.135), 3.15)],
        0.0,
        CARBON_LIGHT,
    );
    // Coil sleeves with glowing slots on each side.
    for i in 0..3 {
        let z = 1.55 + i as f32 * 0.5;
        solid(
            &mut barrel,
            &[at_z(&oct(0.185), z), at_z(&oct(0.185), z + 0.36)],
            0.025,
            CARBON,
        );
        for s in [-1.0f32, 1.0] {
            let x = s * 0.178;
            plate(
                &mut barrel,
                [
                    v(x, 0.035, z + 0.06),
                    v(x, 0.035, z + 0.3),
                    v(x, -0.035, z + 0.3),
                    v(x, -0.035, z + 0.06),
                ],
                0.01,
                Vec3::new(0.0, 0.0, z),
                GLOW,
            );
        }
    }
    // Hex muzzle with a dark bore.
    solid(
        &mut barrel,
        &[
            at_z(&ngon(6, 0.24, Vec2::ZERO), 3.08),
            at_z(&ngon(6, 0.27, Vec2::ZERO), 3.5),
        ],
        0.03,
        CARBON,
    );
    round(
        &mut barrel,
        &[
            at_z(&ngon(10, 0.12, Vec2::ZERO), 3.4),
            at_z(&ngon(10, 0.12, Vec2::ZERO), 3.505),
        ],
        0.0,
        BORE,
    );
    for s in [-1.0f32, 1.0] {
        plate(
            &mut barrel,
            [
                v(s * 0.25, 0.04, 3.15),
                v(s * 0.25, 0.04, 3.42),
                v(s * 0.25, -0.04, 3.42),
                v(s * 0.25, -0.04, 3.15),
            ],
            0.01,
            Vec3::new(0.0, 0.0, 3.3),
            GLOW,
        );
    }

    TurretModel {
        head,
        mantlet,
        barrel,
        pivot: v(0.0, 0.45, 0.88),
        barrels: vec![Vec3::ZERO],
        muzzle: 3.55,
        recoil: 0.8,
    }
}

/// Twin short cannons in a wide, low turret with big shoulder plates.
pub fn maul() -> TurretModel {
    let mut head = MeshData::default();
    let inside = v(0.0, 0.4, -0.2);
    turret_ring(&mut head);
    solid(
        &mut head,
        &[
            at_z(&trap(0.78, 0.68, 0.1, 0.58, 0.1), -1.25),
            at_z(&trap(1.0, 0.88, 0.08, 0.72, 0.12), -0.92),
            at_z(&trap(1.0, 0.88, 0.08, 0.72, 0.12), 0.6),
            at_z(&trap(0.8, 0.7, 0.14, 0.56, 0.1), 1.0),
        ],
        0.04,
        CARBON,
    );
    plate2(
        &mut head,
        [
            v(0.5, 0.82, -1.0),
            v(0.5, 0.82, 0.68),
            v(1.1, 0.5, 0.56),
            v(1.1, 0.5, -0.94),
        ],
        0.08,
        inside,
        PAINT,
    );
    plate2(
        &mut head,
        [
            v(1.1, 0.48, -0.9),
            v(1.1, 0.48, 0.5),
            v(1.06, 0.12, 0.45),
            v(1.06, 0.12, -0.85),
        ],
        0.06,
        inside,
        CARBON_LIGHT,
    );
    plate(
        &mut head,
        [
            v(-0.48, 0.8, 0.66),
            v(0.48, 0.8, 0.66),
            v(0.44, 0.6, 1.04),
            v(-0.44, 0.6, 1.04),
        ],
        0.06,
        inside,
        PAINT,
    );
    plate(
        &mut head,
        [
            v(-0.55, 0.5, 1.03),
            v(0.55, 0.5, 1.03),
            v(0.55, 0.45, 1.03),
            v(-0.55, 0.45, 1.03),
        ],
        0.015,
        inside,
        GLOW,
    );
    // Ammo drum housing at the back.
    solid(
        &mut head,
        &[
            at_x(&ngon(8, 0.32, Vec2::new(-1.2, 0.5)), -0.55),
            at_x(&ngon(8, 0.32, Vec2::new(-1.2, 0.5)), 0.55),
        ],
        0.04,
        CARBON_LIGHT,
    );
    plate(
        &mut head,
        [
            v(-0.5, 0.86, -1.42),
            v(0.5, 0.86, -1.42),
            v(0.5, 0.86, -1.0),
            v(-0.5, 0.86, -1.0),
        ],
        0.03,
        v(0.0, 0.4, -1.2),
        PAINT,
    );
    for s in [-1.0f32, 1.0] {
        plate(
            &mut head,
            [
                v(s * 0.57, 0.62, -1.15),
                v(s * 0.57, 0.62, -0.95),
                v(s * 0.57, 0.38, -0.95),
                v(s * 0.57, 0.38, -1.15),
            ],
            0.015,
            v(0.0, 0.5, -1.2),
            GLOW,
        );
    }

    let mut mantlet = MeshData::default();
    solid(
        &mut mantlet,
        &[
            at_z(&rect(Vec2::ZERO, Vec2::new(0.62, 0.22), 0.07), -0.3),
            at_z(&rect(Vec2::ZERO, Vec2::new(0.58, 0.19), 0.06), 0.3),
        ],
        0.03,
        CARBON,
    );
    round(
        &mut mantlet,
        &[
            at_x(&ngon(10, 0.1, Vec2::ZERO), -0.72),
            at_x(&ngon(10, 0.1, Vec2::ZERO), 0.72),
        ],
        0.02,
        METAL,
    );
    plate(
        &mut mantlet,
        [
            v(-0.12, 0.22, -0.2),
            v(0.12, 0.22, -0.2),
            v(0.12, 0.22, 0.25),
            v(-0.12, 0.22, 0.25),
        ],
        0.03,
        Vec3::ZERO,
        PAINT,
    );

    let mut barrel = MeshData::default();
    let sq = |h: f32| rect(Vec2::ZERO, Vec2::splat(h), h * 0.3);
    solid(
        &mut barrel,
        &[at_z(&sq(0.17), 0.1), at_z(&sq(0.16), 1.45)],
        0.03,
        CARBON,
    );
    plate(
        &mut barrel,
        [
            v(-0.05, 0.172, 0.4),
            v(0.05, 0.172, 0.4),
            v(0.05, 0.172, 1.2),
            v(-0.05, 0.172, 1.2),
        ],
        0.01,
        Vec3::ZERO,
        GLOW,
    );
    plate(
        &mut barrel,
        [
            v(-0.17, 0.17, 0.3),
            v(-0.07, 0.17, 0.3),
            v(-0.07, 0.17, 1.3),
            v(-0.17, 0.17, 1.3),
        ],
        0.02,
        Vec3::ZERO,
        PAINT,
    );
    round(
        &mut barrel,
        &[
            at_z(&ngon(8, 0.105, Vec2::ZERO), 1.4),
            at_z(&ngon(8, 0.1, Vec2::ZERO), 2.45),
        ],
        0.0,
        METAL,
    );
    solid(
        &mut barrel,
        &[at_z(&sq(0.15), 2.4), at_z(&sq(0.16), 2.78)],
        0.025,
        CARBON,
    );
    round(
        &mut barrel,
        &[
            at_z(&ngon(8, 0.07, Vec2::ZERO), 2.7),
            at_z(&ngon(8, 0.07, Vec2::ZERO), 2.785),
        ],
        0.0,
        BORE,
    );

    TurretModel {
        head,
        mantlet,
        barrel,
        pivot: v(0.0, 0.38, 0.92),
        barrels: vec![v(-0.34, 0.0, 0.0), v(0.34, 0.0, 0.0)],
        muzzle: 2.82,
        recoil: 0.6,
    }
}

// ---------------------------------------------------------------------------
// Faction turrets. The Imperium builds the plated Lance and Maul above; the
// others below each have their own shape language and glow color.

/// Sorn plasma glow.
const SORN_GLOW: Skin = Skin {
    face: Vec4::new(0.25, 1.0, 0.6, Material::Glow.id()),
    edge: Vec4::new(0.25, 1.0, 0.6, Material::Glow.id()),
};
/// Machine laser red.
const LASER_GLOW: Skin = Skin {
    face: Vec4::new(1.0, 0.07, 0.04, Material::Glow.id()),
    edge: Vec4::new(1.0, 0.07, 0.04, Material::Glow.id()),
};
/// Resonant crystal violet.
const CRYSTAL: Skin = Skin {
    face: Vec4::new(0.6, 0.25, 1.0, Material::Glow.id()),
    edge: Vec4::new(0.8, 0.55, 1.0, Material::Glow.id()),
};

/// An oval dome lofted from horizontal rings: half extents `rx` x `rz` at
/// `y0`, rising `h`, centered at z = `cz`.
fn dome(m: &mut MeshData, rx: f32, rz: f32, cz: f32, y0: f32, h: f32, skin: Skin) {
    let steps = 5;
    let rings: Vec<Vec<Vec3>> = (0..=steps)
        .map(|i| {
            let f = i as f32 / steps as f32;
            // Quarter-ellipse profile, with a small flat cap on top.
            let r = (1.0 - f * f).sqrt().max(0.18);
            let y = y0 + h * (f * std::f32::consts::FRAC_PI_2).sin();
            ngon(18, 1.0, Vec2::ZERO)
                .iter()
                .map(|p| Vec3::new(p.x * rx * r, y, cz + p.y * rz * r))
                .collect()
        })
        .collect();
    round(m, &rings, 0.02, skin);
}

/// A crystal shard: a hexagonal prism from `base` along `dir`, tapering to a point.
fn shard(m: &mut MeshData, base: Vec3, dir: Vec3, len: f32, r: f32, skin: Skin) {
    let d = dir.normalize();
    let side = if d.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let a = d.cross(side).normalize();
    let b = d.cross(a);
    let ring = |at: Vec3, rr: f32| -> Vec<Vec3> {
        ngon(6, rr, Vec2::ZERO)
            .iter()
            .map(|p| at + a * p.x + b * p.y)
            .collect()
    };
    solid(
        m,
        &[
            ring(base, r * 0.7),
            ring(base + d * len * 0.25, r),
            ring(base + d * len * 0.7, r * 0.85),
            ring(base + d * len, r * 0.08),
        ],
        0.005,
        skin,
    );
}

/// Sorn orb launcher: a smooth shell of a head with glowing seams, and one
/// fat emitter that throws bouncing plasma.
pub fn wisp() -> TurretModel {
    let mut head = MeshData::default();
    turret_ring(&mut head);
    dome(&mut head, 0.95, 1.2, -0.15, 0.1, 0.72, CARBON);
    // A painted carapace over the top, and a glowing seam band around the dome.
    dome(&mut head, 0.7, 0.85, -0.3, 0.55, 0.38, PAINT);
    round(
        &mut head,
        &[
            at_y(&ngon(18, 1.0, Vec2::ZERO), 0.0)
                .iter()
                .map(|p| Vec3::new(p.x * 0.97, 0.34, -0.15 + p.z * 1.22))
                .collect(),
            at_y(&ngon(18, 1.0, Vec2::ZERO), 0.0)
                .iter()
                .map(|p| Vec3::new(p.x * 0.95, 0.4, -0.15 + p.z * 1.2))
                .collect(),
        ],
        0.0,
        SORN_GLOW,
    );
    // Two swept-back fins.
    for s in [-1.0f32, 1.0] {
        plate(
            &mut head,
            [
                v(s * 0.45, 0.8, -0.6),
                v(s * 0.45, 0.85, -0.95),
                v(s * 0.6, 1.25, -1.45),
                v(s * 0.6, 1.15, -1.1),
            ],
            0.04,
            v(0.0, 0.6, -0.9),
            CARBON_LIGHT,
        );
    }

    let mut mantlet = MeshData::default();
    round(
        &mut mantlet,
        &[
            at_z(&ngon(12, 0.2, Vec2::ZERO), -0.4),
            at_z(&ngon(12, 0.36, Vec2::ZERO), -0.15),
            at_z(&ngon(12, 0.36, Vec2::ZERO), 0.2),
            at_z(&ngon(12, 0.26, Vec2::ZERO), 0.42),
        ],
        0.02,
        CARBON,
    );

    let mut barrel = MeshData::default();
    let c = |r: f32| ngon(14, r, Vec2::ZERO);
    round(
        &mut barrel,
        &[
            at_z(&c(0.22), 0.3),
            at_z(&c(0.27), 0.9),
            at_z(&c(0.25), 1.7),
        ],
        0.02,
        CARBON_LIGHT,
    );
    // Glowing containment rings, then a flared bell around a bright core.
    for i in 0..3 {
        let z = 0.75 + i as f32 * 0.32;
        round(
            &mut barrel,
            &[at_z(&c(0.3), z), at_z(&c(0.3), z + 0.09)],
            0.0,
            SORN_GLOW,
        );
    }
    round(
        &mut barrel,
        &[
            at_z(&c(0.26), 1.65),
            at_z(&c(0.4), 2.0),
            at_z(&c(0.43), 2.1),
        ],
        0.02,
        CARBON,
    );
    round(
        &mut barrel,
        &[at_z(&c(0.3), 2.0), at_z(&c(0.3), 2.11)],
        0.0,
        SORN_GLOW,
    );

    TurretModel {
        head,
        mantlet,
        barrel,
        pivot: v(0.0, 0.5, 0.85),
        barrels: vec![Vec3::ZERO],
        muzzle: 2.15,
        recoil: 0.3,
    }
}

/// Sorn rapid plasma: a long low shell with three thin barrels in a triangle
/// that fire in turn.
pub fn triad() -> TurretModel {
    let mut head = MeshData::default();
    turret_ring(&mut head);
    dome(&mut head, 0.85, 1.35, -0.1, 0.1, 0.6, CARBON);
    dome(&mut head, 0.55, 1.0, -0.2, 0.45, 0.3, PAINT);
    // Glowing gill slits down each flank.
    for s in [-1.0f32, 1.0] {
        for k in 0..3 {
            let z = -0.7 + k as f32 * 0.28;
            plate(
                &mut head,
                [
                    v(s * 0.8, 0.42, z),
                    v(s * 0.8, 0.42, z + 0.14),
                    v(s * 0.83, 0.22, z + 0.2),
                    v(s * 0.83, 0.22, z + 0.06),
                ],
                0.015,
                v(0.0, 0.35, z),
                SORN_GLOW,
            );
        }
    }
    // A sensor crest on top.
    plate(
        &mut head,
        [
            v(0.0, 0.75, -1.1),
            v(0.0, 0.8, 0.4),
            v(0.0, 1.0, -0.1),
            v(0.0, 1.05, -0.9),
        ],
        0.05,
        v(0.3, 0.5, -0.4),
        CARBON_LIGHT,
    );

    let mut mantlet = MeshData::default();
    // Triangular collar around the barrel cluster.
    round(
        &mut mantlet,
        &[
            at_z(&ngon(3, 0.42, Vec2::ZERO), -0.35),
            at_z(&ngon(3, 0.46, Vec2::ZERO), 0.1),
            at_z(&ngon(3, 0.4, Vec2::ZERO), 0.35),
        ],
        0.06,
        CARBON,
    );
    round(
        &mut mantlet,
        &[
            at_z(&ngon(9, 0.1, Vec2::ZERO), 0.3),
            at_z(&ngon(9, 0.1, Vec2::ZERO), 0.4),
        ],
        0.0,
        SORN_GLOW,
    );

    let mut barrel = MeshData::default();
    let c = |r: f32| ngon(10, r, Vec2::ZERO);
    round(
        &mut barrel,
        &[at_z(&c(0.09), 0.2), at_z(&c(0.08), 2.1)],
        0.0,
        CARBON_LIGHT,
    );
    round(
        &mut barrel,
        &[at_z(&c(0.12), 0.9), at_z(&c(0.12), 1.15)],
        0.01,
        CARBON,
    );
    round(
        &mut barrel,
        &[at_z(&c(0.11), 2.05), at_z(&c(0.11), 2.25)],
        0.0,
        SORN_GLOW,
    );

    let r = 0.17;
    TurretModel {
        head,
        mantlet,
        barrel,
        pivot: v(0.0, 0.42, 0.95),
        barrels: (0..3)
            .map(|k| {
                let a = std::f32::consts::FRAC_PI_2 + k as f32 / 3.0 * TAU;
                v(a.cos() * r, a.sin() * r, 0.0)
            })
            .collect(),
        muzzle: 2.3,
        recoil: 0.25,
    }
}

/// Machine laser: a blocky, asymmetric housing stacked with heat-sink fins,
/// and a long focusing rod ending in a red lens.
pub fn lumen() -> TurretModel {
    let mut head = MeshData::default();
    let inside = v(0.0, 0.45, -0.2);
    turret_ring(&mut head);
    solid(
        &mut head,
        &[
            at_z(
                &rect(Vec2::new(0.0, 0.42), Vec2::new(0.62, 0.32), 0.05),
                -1.2,
            ),
            at_z(
                &rect(Vec2::new(0.0, 0.42), Vec2::new(0.7, 0.36), 0.06),
                -0.3,
            ),
            at_z(&rect(Vec2::new(0.0, 0.4), Vec2::new(0.6, 0.3), 0.06), 0.9),
        ],
        0.03,
        CARBON,
    );
    // Power block offset to one side, and an armored cap on the other.
    block(
        &mut head,
        v(-0.78, 0.15, -1.0),
        v(-0.3, 0.98, 0.1),
        0.04,
        PAINT,
    );
    block(
        &mut head,
        v(0.3, 0.78, -0.9),
        v(0.72, 0.9, 0.6),
        0.03,
        PAINT,
    );
    // Heat-sink fins: a stack of thin metal plates down the right flank.
    for k in 0..7 {
        let z = -1.05 + k as f32 * 0.16;
        block(
            &mut head,
            v(0.7, 0.2, z),
            v(0.86, 0.74, z + 0.05),
            0.01,
            METAL,
        );
    }
    // Red status slit and a rear exhaust grille.
    plate(
        &mut head,
        [
            v(-0.8, 0.8, -0.7),
            v(-0.8, 0.8, -0.1),
            v(-0.8, 0.72, -0.1),
            v(-0.8, 0.72, -0.7),
        ],
        0.015,
        inside,
        LASER_GLOW,
    );
    plate(
        &mut head,
        [
            v(-0.2, 0.62, -1.21),
            v(0.45, 0.62, -1.21),
            v(0.45, 0.3, -1.21),
            v(-0.2, 0.3, -1.21),
        ],
        0.015,
        inside,
        METAL,
    );

    let mut mantlet = MeshData::default();
    block(
        &mut mantlet,
        v(-0.3, -0.22, -0.35),
        v(0.3, 0.22, 0.4),
        0.04,
        CARBON,
    );
    round(
        &mut mantlet,
        &[
            at_x(&ngon(8, 0.1, Vec2::ZERO), -0.4),
            at_x(&ngon(8, 0.1, Vec2::ZERO), 0.4),
        ],
        0.02,
        METAL,
    );

    let mut barrel = MeshData::default();
    let sq = |h: f32| rect(Vec2::ZERO, Vec2::splat(h), h * 0.35);
    solid(
        &mut barrel,
        &[at_z(&sq(0.2), 0.3), at_z(&sq(0.18), 1.1)],
        0.03,
        CARBON_LIGHT,
    );
    round(
        &mut barrel,
        &[
            at_z(&ngon(8, 0.07, Vec2::ZERO), 1.05),
            at_z(&ngon(8, 0.07, Vec2::ZERO), 2.9),
        ],
        0.0,
        METAL,
    );
    // Focusing coils along the rod.
    for i in 0..4 {
        let z = 1.3 + i as f32 * 0.38;
        solid(
            &mut barrel,
            &[at_z(&sq(0.13), z), at_z(&sq(0.13), z + 0.12)],
            0.015,
            CARBON,
        );
    }
    // Lens housing and the red lens.
    solid(
        &mut barrel,
        &[
            at_z(&ngon(8, 0.16, Vec2::ZERO), 2.85),
            at_z(&ngon(8, 0.19, Vec2::ZERO), 3.15),
        ],
        0.02,
        CARBON,
    );
    round(
        &mut barrel,
        &[
            at_z(&ngon(10, 0.11, Vec2::ZERO), 3.1),
            at_z(&ngon(10, 0.11, Vec2::ZERO), 3.16),
        ],
        0.0,
        LASER_GLOW,
    );

    TurretModel {
        head,
        mantlet,
        barrel,
        pivot: v(0.0, 0.42, 0.92),
        barrels: vec![Vec3::ZERO],
        muzzle: 3.2,
        recoil: 0.0,
    }
}

/// Resonant flamethrower: a rounded, painted reliquary of a head crowned with
/// glowing crystals, and a short wide nozzle caged in crystal.
pub fn hymn() -> TurretModel {
    let mut head = MeshData::default();
    turret_ring(&mut head);
    round(
        &mut head,
        &[
            at_y(&ngon(8, 0.9, Vec2::new(0.0, -0.15)), 0.1),
            at_y(&ngon(8, 0.95, Vec2::new(0.0, -0.15)), 0.45),
            at_y(&ngon(8, 0.7, Vec2::new(0.0, -0.2)), 0.8),
        ],
        0.04,
        PAINT,
    );
    // Dark band with a glowing script line around the waist.
    round(
        &mut head,
        &[
            at_y(&ngon(8, 0.965, Vec2::new(0.0, -0.15)), 0.4),
            at_y(&ngon(8, 0.965, Vec2::new(0.0, -0.15)), 0.5),
        ],
        0.0,
        CARBON,
    );
    round(
        &mut head,
        &[
            at_y(&ngon(8, 0.97, Vec2::new(0.0, -0.15)), 0.44),
            at_y(&ngon(8, 0.97, Vec2::new(0.0, -0.15)), 0.46),
        ],
        0.0,
        CRYSTAL,
    );
    // Crown of crystals rising out of the back.
    shard(
        &mut head,
        v(0.0, 0.75, -0.45),
        v(0.0, 1.0, -0.25),
        0.8,
        0.13,
        CRYSTAL,
    );
    shard(
        &mut head,
        v(0.28, 0.72, -0.35),
        v(0.35, 1.0, -0.2),
        0.55,
        0.1,
        CRYSTAL,
    );
    shard(
        &mut head,
        v(-0.28, 0.72, -0.35),
        v(-0.35, 1.0, -0.2),
        0.55,
        0.1,
        CRYSTAL,
    );
    shard(
        &mut head,
        v(0.15, 0.7, -0.75),
        v(0.2, 0.8, -0.6),
        0.45,
        0.08,
        CRYSTAL,
    );
    shard(
        &mut head,
        v(-0.15, 0.7, -0.75),
        v(-0.2, 0.8, -0.6),
        0.45,
        0.08,
        CRYSTAL,
    );
    // Fuel reservoir: two tanks slung at the back.
    for s in [-1.0f32, 1.0] {
        round(
            &mut head,
            &[
                at_z(&ngon(10, 0.17, Vec2::new(s * 0.45, 0.35)), -1.25),
                at_z(&ngon(10, 0.17, Vec2::new(s * 0.45, 0.35)), -0.75),
            ],
            0.04,
            METAL,
        );
    }

    let mut mantlet = MeshData::default();
    round(
        &mut mantlet,
        &[
            at_z(&ngon(8, 0.26, Vec2::ZERO), -0.35),
            at_z(&ngon(8, 0.32, Vec2::ZERO), 0.0),
            at_z(&ngon(8, 0.28, Vec2::ZERO), 0.35),
        ],
        0.03,
        CARBON,
    );

    let mut barrel = MeshData::default();
    let c = |r: f32| ngon(12, r, Vec2::ZERO);
    round(
        &mut barrel,
        &[at_z(&c(0.17), 0.3), at_z(&c(0.15), 1.2)],
        0.02,
        CARBON_LIGHT,
    );
    round(
        &mut barrel,
        &[
            at_z(&c(0.16), 1.15),
            at_z(&c(0.3), 1.65),
            at_z(&c(0.32), 1.8),
        ],
        0.02,
        PAINT,
    );
    round(
        &mut barrel,
        &[at_z(&c(0.22), 1.75), at_z(&c(0.22), 1.81)],
        0.0,
        CRYSTAL,
    );
    // Crystal cage: four shards along the nozzle pointing forward.
    for k in 0..4 {
        let a = k as f32 / 4.0 * TAU + std::f32::consts::FRAC_PI_4;
        let o = v(a.cos() * 0.24, a.sin() * 0.24, 0.6);
        shard(
            &mut barrel,
            o,
            v(a.cos() * 0.15, a.sin() * 0.15, 1.0),
            0.75,
            0.06,
            CRYSTAL,
        );
    }

    TurretModel {
        head,
        mantlet,
        barrel,
        pivot: v(0.0, 0.45, 0.8),
        barrels: vec![Vec3::ZERO],
        muzzle: 1.85,
        recoil: 0.0,
    }
}
