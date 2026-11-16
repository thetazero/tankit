//! Procedural tank kit geometry. Everything is assembled from beveled armor
//! panels (`plate`) and chamfered lofted solids (`solid`), flat shaded, so
//! the vehicle reads as physically built from separate plates.
//!
//! Local frame for every part: +z forward, +y up, x across. Hulls sit on the
//! ground at y = 0; turrets are built around their yaw ring at the origin;
//! guns around their pitch pivot.

use std::f32::consts::TAU;

use glam::{Vec2, Vec3, Vec4};

use super::track::Wheel;
use crate::render::mesh::MeshData;

/// Material ids understood by `tank_surface` in scene.wgsl.
pub mod mat {
    /// Bare rolled/cast armor steel, mottled and weathered.
    pub const STEEL: f32 = 6.0;
    /// Weathered paint over steel; takes a muted team color.
    pub const PAINT: f32 = 7.0;
    pub const TREAD: f32 = 8.0;
    /// Machined metal: rods, axles, pins.
    pub const METAL: f32 = 10.0;
    /// Paint on chamfered edges, chipped through to steel.
    pub const WORN: f32 = 11.0;
    /// Vision blocks and sensor glass: dark, glossy, never emissive.
    pub const LENS: f32 = 12.0;
}

// Design direction (docs/design-direction.md): solid weathered gunmetal,
// blunt chunky shapes, nothing glows on the machine itself.

#[derive(Clone, Copy)]
pub struct Skin {
    face: Vec4,
    edge: Vec4,
}

pub const CARBON: Skin = Skin { face: Vec4::new(0.13, 0.125, 0.115, mat::STEEL), edge: Vec4::new(0.17, 0.165, 0.15, mat::STEEL) };
pub const CARBON_LIGHT: Skin = Skin { face: Vec4::new(0.16, 0.155, 0.14, mat::STEEL), edge: Vec4::new(0.2, 0.19, 0.17, mat::STEEL) };
pub const PAINT: Skin = Skin { face: Vec4::new(0.22, 0.21, 0.19, mat::PAINT), edge: Vec4::new(0.22, 0.21, 0.19, mat::WORN) };
pub const METAL: Skin = Skin { face: Vec4::new(0.3, 0.3, 0.31, mat::METAL), edge: Vec4::new(0.4, 0.4, 0.4, mat::METAL) };
pub const TREAD: Skin = Skin { face: Vec4::new(0.1, 0.1, 0.1, mat::TREAD), edge: Vec4::new(0.18, 0.18, 0.18, mat::TREAD) };
pub const RUBBER: Skin = Skin { face: Vec4::new(0.07, 0.07, 0.07, mat::STEEL), edge: Vec4::new(0.1, 0.1, 0.1, mat::STEEL) };
pub const LENS: Skin = Skin { face: Vec4::new(0.03, 0.035, 0.04, mat::LENS), edge: Vec4::new(0.03, 0.035, 0.04, mat::LENS) };
pub const BORE: Skin = Skin { face: Vec4::new(0.005, 0.005, 0.005, mat::LENS), edge: Vec4::new(0.005, 0.005, 0.005, mat::LENS) };

// ---------------------------------------------------------------------------
// Shape kit

/// Quad outline with each corner cut by `k`: 8 points, and the odd-indexed
/// sides (point i -> i+1) are the corner chamfers.
fn cut_quad<V>(c: [V; 4], k: f32) -> Vec<V>
where
    V: Copy + std::ops::Add<Output = V> + std::ops::Sub<Output = V> + std::ops::Mul<f32, Output = V> + Norm,
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
    cut_quad([Vec2::new(-hb, y0), Vec2::new(hb, y0), Vec2::new(ht, y1), Vec2::new(-ht, y1)], k)
}

/// Rectangle cross-section centered at `c` with half extents `h`.
pub fn rect(c: Vec2, h: Vec2, k: f32) -> Vec<Vec2> {
    cut_quad([c - h, c + Vec2::new(h.x, -h.y), c + h, c + Vec2::new(-h.x, h.y)], k)
}

/// Regular polygon of radius `r`, rotated so a flat side faces up.
pub fn ngon(n: usize, r: f32, c: Vec2) -> Vec<Vec2> {
    let off = if n.is_multiple_of(2) { TAU / (2 * n) as f32 } else { 0.0 };
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
        let off = if i == 0 { d0 * bevel } else if i == n - 1 { -d1 * bevel } else { Vec3::ZERO };
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
            let c = if end_band || (odd_edges && j % 2 == 1) { skin.edge } else { skin.face };
            let (a, b, cc, d) = (w[0][j], w[0][jn], w[1][jn], w[1][j]);
            m.facet([a, b, cc], inside, c);
            m.facet([a, cc, d], inside, c);
        }
    }
    for (ring, other) in [(&seq[0], &seq[1]), (&seq[seq.len() - 1], &seq[seq.len() - 2])] {
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
    // Thick, solid plates: nothing should read as sheet metal.
    let t = t * 1.5;
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


fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x, y, z)
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
        let bar = |x: f32, z: f32| at_x(&rect(Vec2::new(z, 0.02), Vec2::new(0.022, 0.02), 0.008), x);
        let (x0, x1) = (s * 0.015, s * (hw - 0.02));
        let mut rings = [bar(x0, 0.03), bar(x1, -0.035)];
        if s < 0.0 {
            rings.reverse();
        }
        solid(&mut m, &rings, 0.006, TREAD);
        // End connectors where the link pins would be.
        block(&mut m, v(s * hw - 0.04, -0.03, -hl - 0.012), v(s * hw + 0.008, 0.005, -hl + 0.04), 0.008, METAL);
    }
    // Guide horn on the inside, running between the split road wheels.
    let horn = cut_quad([Vec2::new(-0.055, -0.03), Vec2::new(0.055, -0.03), Vec2::new(0.03, -0.12), Vec2::new(-0.03, -0.12)], 0.012);
    solid(&mut m, &[at_x(&horn, -0.03), at_x(&horn, 0.03)], 0.008, METAL);
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
        round(&mut m, &[at_x(&ngon(18, r, Vec2::ZERO), xa), at_x(&ngon(18, r, Vec2::ZERO), xb)], 0.025, RUBBER);
        // Steel wheel face inset into the tire.
        let face = xb + s * 0.012;
        round(&mut m, &[at_x(&ngon(14, r * 0.78, Vec2::ZERO), xb), at_x(&ngon(14, r * 0.74, Vec2::ZERO), face)], 0.01, CARBON_LIGHT);
    }
    let xo = gap * 0.5 + dw + 0.012;
    // Hub cap and three painted spokes on the outer face.
    round(&mut m, &[at_x(&ngon(8, r * 0.3, Vec2::ZERO), xo), at_x(&ngon(8, r * 0.24, Vec2::ZERO), xo + 0.05)], 0.012, METAL);
    for i in 0..3 {
        let a = i as f32 / 3.0 * TAU;
        let d = Vec2::new(a.cos(), a.sin());
        let side = Vec2::new(-d.y, d.x);
        let (p0, p1) = (d * r * 0.3, d * r * 0.7);
        let q = |p: Vec2, w: f32, x: f32| v(x, p.y + side.y * w, p.x + side.x * w);
        plate(
            &mut m,
            [q(p0, 0.06, xo + 0.025), q(p1, 0.04, xo + 0.015), q(p1, -0.04, xo + 0.015), q(p0, -0.06, xo + 0.025)],
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
        round(&mut m, &[at_x(&ngon(20, r * 0.93, Vec2::ZERO), xa), at_x(&ngon(20, r * 0.93, Vec2::ZERO), xb)], 0.02, CARBON_LIGHT);
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
    round(&mut m, &[at_x(&ngon(10, r * 0.55, Vec2::ZERO), xo), at_x(&ngon(10, r * 0.4, Vec2::ZERO), xo + 0.08)], 0.015, PAINT);
    round(&mut m, &[at_x(&ngon(6, r * 0.2, Vec2::ZERO), xo + 0.07), at_x(&ngon(6, r * 0.16, Vec2::ZERO), xo + 0.12)], 0.01, METAL);
    m
}

/// Plain idler wheel.
pub fn idler(r: f32, width: f32) -> MeshData {
    let mut m = MeshData::default();
    let gap = 0.13;
    let dw = (width - gap) * 0.5;
    for s in [-1.0f32, 1.0] {
        let (xa, xb) = (s * gap * 0.5, s * (gap * 0.5 + dw));
        round(&mut m, &[at_x(&ngon(18, r, Vec2::ZERO), xa), at_x(&ngon(18, r, Vec2::ZERO), xb)], 0.02, CARBON_LIGHT);
    }
    let xo = gap * 0.5 + dw;
    round(&mut m, &[at_x(&ngon(6, r * 0.5, Vec2::ZERO), xo), at_x(&ngon(6, r * 0.4, Vec2::ZERO), xo + 0.05)], 0.012, METAL);
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU;
        let p = Vec2::new(a.cos(), a.sin()) * r * 0.68;
        block(&mut m, v(xo - 0.01, p.y - 0.03, p.x - 0.03), v(xo + 0.03, p.y + 0.03, p.x + 0.03), 0.01, METAL);
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
    /// Height of the suspension shock mounts on the hull side.
    pub shock_top: f32,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn quad_normal(q: &[Vec3; 4], away_from: Vec3) -> Vec3 {
    let n = (q[2] - q[0]).cross(q[3] - q[1]).normalize_or_zero();
    let c = (q[0] + q[1] + q[2] + q[3]) * 0.25;
    if n.dot(c - away_from) < 0.0 { -n } else { n }
}

/// Shrink a (roughly planar) quad toward its center by `d`.
fn inset(q: [Vec3; 4], d: f32) -> [Vec3; 4] {
    let c = (q[0] + q[1] + q[2] + q[3]) * 0.25;
    q.map(|p| p + (c - p).normalize_or_zero() * d)
}

/// An armor plate laid ON a surface: `q` is the patch of surface it covers,
/// the plate stands proud of it by its thickness. Returns the plate's outer face.
fn armor(m: &mut MeshData, q: [Vec3; 4], body: Vec3, t: f32, skin: Skin) -> [Vec3; 4] {
    let n = quad_normal(&q, body);
    let lift = t * 1.5;
    let top = q.map(|p| p + n * lift);
    let c = (q[0] + q[1] + q[2] + q[3]) * 0.25;
    plate(m, top, t, c - n, skin);
    top
}

/// Stepped applique: each layer smaller and standing on the previous one.
fn layered(m: &mut MeshData, q: [Vec3; 4], body: Vec3, layers: &[(f32, f32, Skin)]) {
    let mut q = q;
    for &(ins, t, skin) in layers {
        q = armor(m, inset(q, ins), body, t, skin);
    }
}

/// A cylinder between two arbitrary points.
fn tube(m: &mut MeshData, a: Vec3, b: Vec3, r: f32, sides: usize, skin: Skin) {
    let axis = (b - a).normalize_or_zero();
    let up = if axis.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
    let u = axis.cross(up).normalize();
    let w = axis.cross(u);
    let ring = |c: Vec3| {
        (0..sides)
            .map(|i| {
                let t = i as f32 / sides as f32 * TAU;
                c + (u * t.cos() + w * t.sin()) * r
            })
            .collect::<Vec<_>>()
    };
    round(m, &[ring(a), ring(b)], (r * 0.25).min(0.015), skin);
}

fn mirror_x(q: [Vec3; 4]) -> [Vec3; 4] {
    q.map(|p| Vec3::new(-p.x, p.y, p.z))
}

struct HullSpec {
    front: f32,
    rear: f32,
    /// Half width over the sponsons.
    hw: f32,
    deck: f32,
    /// Where the upper glacis starts sloping down.
    glacis: f32,
    /// Height of the prow's top edge.
    prow: f32,
    skirts: usize,
    skirt_low: f32,
    heavy: bool,
    running: Running,
}

/// Shared hull form: a lower tub between the tracks, a superstructure over
/// them with a long raked glacis, layered applique armor on the glacis,
/// segmented raked skirts, stowage bins along the sponsons and a louvered
/// engine deck. The two hulls are proportions of this.
fn hull(p: HullSpec) -> HullModel {
    let mut m = MeshData::default();
    let hw = p.hw;
    let body = v(0.0, p.deck * 0.55, 0.0);
    let g_end = p.front - 0.38;
    let prow_top = p.prow + 0.16;

    // Lower tub.
    let tub_top = p.deck - 0.3;
    solid(
        &mut m,
        &[
            at_z(&trap(0.8, 0.86, 0.5, tub_top, 0.08), p.rear + 0.1),
            at_z(&trap(0.88, 0.9, 0.32, tub_top, 0.1), p.rear + 0.45),
            at_z(&trap(0.88, 0.9, 0.32, tub_top, 0.1), p.front - 0.75),
            at_z(&trap(0.74, 0.8, 0.6, tub_top - 0.15, 0.08), p.front - 0.12),
        ],
        0.04,
        CARBON,
    );
    // Superstructure with the raked glacis.
    solid(
        &mut m,
        &[
            at_z(&trap(hw - 0.14, hw - 0.3, 1.0, p.deck - 0.14, 0.06), p.rear),
            at_z(&trap(hw, hw - 0.14, 0.96, p.deck, 0.08), p.rear + 0.3),
            at_z(&trap(hw, hw - 0.14, 0.96, p.deck, 0.08), p.glacis),
            at_z(&trap(hw - 0.06, hw - 0.44, 0.94, prow_top, 0.07), g_end),
            at_z(&trap(hw - 0.42, hw - 0.72, 0.9, p.prow, 0.05), p.front),
        ],
        0.05,
        CARBON_LIGHT,
    );

    // Glacis applique: two stepped layers per side around a raised spine.
    let gy = |z: f32| lerp(p.deck, prow_top, ((z - p.glacis) / (g_end - p.glacis)).clamp(0.0, 1.0));
    let gx = |z: f32| lerp(hw - 0.14, hw - 0.44, ((z - p.glacis) / (g_end - p.glacis)).clamp(0.0, 1.0));
    let (z0, z1) = (p.glacis + 0.06, g_end - 0.04);
    for s in [1.0f32, -1.0] {
        let q = [v(s * 0.17, gy(z0), z0), v(s * (gx(z0) - 0.04), gy(z0), z0), v(s * (gx(z1) - 0.04), gy(z1), z1), v(s * 0.17, gy(z1), z1)];
        let mut layers = vec![(0.04, 0.06, PAINT), (0.2, 0.05, PAINT)];
        if p.heavy {
            layers.push((0.18, 0.05, CARBON_LIGHT));
        }
        layered(&mut m, q, body, &layers);
    }
    armor(&mut m, [v(-0.12, gy(z0 - 0.05), z0 - 0.05), v(0.12, gy(z0 - 0.05), z0 - 0.05), v(0.1, gy(z1), z1), v(-0.1, gy(z1), z1)], body, 0.09, CARBON);
    // Lower front plate under the prow, with tow hooks.
    let lf = [v(-(hw - 0.78), p.prow - 0.02, p.front + 0.0), v(hw - 0.78, p.prow - 0.02, p.front), v(0.72, 0.62, p.front - 0.16), v(-0.72, 0.62, p.front - 0.16)];
    armor(&mut m, lf, body, 0.07, PAINT);
    for s in [1.0f32, -1.0] {
        block(&mut m, v(s * 0.5 - 0.07, 0.6, p.front - 0.12), v(s * 0.5 + 0.07, 0.74, p.front + 0.12), 0.025, METAL);
        // Headlight guards at the prow corners.
        let hx = s * (hw - 0.62);
        block(&mut m, v(hx - 0.13, p.prow + 0.02, p.front - 0.22), v(hx + 0.13, p.prow + 0.2, p.front - 0.02), 0.03, CARBON);
        plate(&mut m, [v(hx - 0.08, p.prow + 0.16, p.front - 0.01), v(hx + 0.08, p.prow + 0.16, p.front - 0.01), v(hx + 0.08, p.prow + 0.07, p.front - 0.01), v(hx - 0.08, p.prow + 0.07, p.front - 0.01)], 0.01, body, LENS);
    }
    // Driver's hatch and vision blocks just behind the glacis.
    let hz = p.glacis - 0.3;
    round(&mut m, &[at_y(&ngon(8, 0.26, Vec2::new(-0.62, hz)), p.deck - 0.01), at_y(&ngon(8, 0.22, Vec2::new(-0.62, hz)), p.deck + 0.07)], 0.02, PAINT);
    for k in 0..3 {
        let x = -0.62 + (k as f32 - 1.0) * 0.17;
        block(&mut m, v(x - 0.06, p.deck, hz + 0.24), v(x + 0.06, p.deck + 0.09, hz + 0.34), 0.015, CARBON);
    }

    // Stowage bins along the sponsons, clear of the turret overhang.
    let (bz0, bz1) = (p.rear + 0.45, p.glacis - 0.2);
    let bl = (bz1 - bz0) / 2.0;
    for s in [1.0f32, -1.0] {
        for i in 0..2 {
            let za = bz0 + i as f32 * bl + 0.03;
            let zb = za + bl - 0.06;
            let (xa, xb) = (hw - 0.3, hw - 0.04);
            let (x0, x1) = if s > 0.0 { (xa, xb) } else { (-xb, -xa) };
            block(&mut m, v(x0, p.deck - 0.02, za), v(x1, p.deck + 0.13, zb), 0.025, CARBON);
            block(&mut m, v(x0 - 0.02, p.deck + 0.12, za - 0.02), v(x1 + 0.02, p.deck + 0.16, zb + 0.02), 0.012, PAINT);
            for zl in [za + 0.2, zb - 0.2] {
                let xo = s * (hw - 0.04);
                block(&mut m, v(xo - 0.025, p.deck + 0.03, zl - 0.04), v(xo + 0.025, p.deck + 0.11, zl + 0.04), 0.008, METAL);
            }
        }
    }

    // Engine deck: louvers recessed in a raised frame, behind the turret.
    let (ez0, ez1, ex) = (p.rear + 0.32, p.rear + 1.2, 0.78);
    let fy = p.deck + 0.09;
    block(&mut m, v(-ex - 0.08, p.deck - 0.02, ez0 - 0.08), v(ex + 0.08, fy, ez0), 0.02, CARBON_LIGHT);
    block(&mut m, v(-ex - 0.08, p.deck - 0.02, ez1), v(ex + 0.08, fy, ez1 + 0.08), 0.02, CARBON_LIGHT);
    block(&mut m, v(-ex - 0.08, p.deck - 0.02, ez0), v(-ex, fy, ez1), 0.02, CARBON_LIGHT);
    block(&mut m, v(ex, p.deck - 0.02, ez0), v(ex + 0.08, fy, ez1), 0.02, CARBON_LIGHT);
    block(&mut m, v(-0.03, p.deck - 0.02, ez0), v(0.03, fy - 0.01, ez1), 0.01, CARBON_LIGHT);
    let slats = 7;
    for i in 0..slats {
        let z = ez0 + 0.04 + i as f32 * (ez1 - ez0 - 0.08) / slats as f32;
        armor(&mut m, [v(-ex, p.deck, z), v(ex, p.deck, z), v(ex, p.deck, z + 0.07), v(-ex, p.deck, z + 0.07)], body, 0.035, METAL);
    }
    // Rear plate: exhaust grilles and tow hooks.
    for s in [1.0f32, -1.0] {
        let (x0, x1) = if s > 0.0 { (0.35, 0.95) } else { (-0.95, -0.35) };
        block(&mut m, v(x0, p.deck - 0.5, p.rear - 0.1), v(x1, p.deck - 0.2, p.rear + 0.05), 0.03, CARBON);
        for k in 0..4 {
            let y = p.deck - 0.46 + k as f32 * 0.07;
            block(&mut m, v(x0 + 0.04, y, p.rear - 0.13), v(x1 - 0.04, y + 0.03, p.rear - 0.09), 0.008, METAL);
        }
        block(&mut m, v(s * 0.6 - 0.07, 0.62, p.rear - 0.15), v(s * 0.6 + 0.07, 0.76, p.rear + 0.1), 0.025, METAL);
    }

    // Skirts: raked outward, overlapping segments with a reinforcing rib,
    // the front segment swept up; a rail runs along their top.
    let (zs, ze) = (p.rear + 0.15, p.front - 0.72);
    let seg = (ze - zs) / p.skirts as f32;
    for s in [1.0f32, -1.0] {
        for i in 0..p.skirts {
            let za = zs + i as f32 * seg - 0.04;
            let zb = za + seg + 0.08;
            let off = if i % 2 == 0 { 0.0 } else { 0.035 };
            let x_top = hw + 0.04 + off;
            let x_bot = hw + 0.15 + off;
            let low_b = if i == p.skirts - 1 { p.skirt_low + 0.28 } else { p.skirt_low };
            let face = [v(x_top, p.deck - 0.1, za), v(x_top, p.deck - 0.1, zb), v(x_bot, low_b, zb), v(x_bot, p.skirt_low, za)];
            let face = if s > 0.0 { face } else { mirror_x(face) };
            plate(&mut m, face, 0.06, body, PAINT);
            // Rib across the middle of the segment.
            let mid = |a: Vec3, b: Vec3, t: f32| a.lerp(b, t);
            let rib = [mid(face[0], face[3], 0.45), mid(face[1], face[2], 0.45), mid(face[1], face[2], 0.6), mid(face[0], face[3], 0.6)];
            armor(&mut m, inset(rib, 0.06), body, 0.03, CARBON_LIGHT);
        }
        // Top rail and the mudguard sweeping down over the front of the track.
        let rx = s * (hw + 0.08);
        block(&mut m, v(rx.min(rx + s * 0.1) - 0.0, p.deck - 0.12, zs - 0.05), v(rx.max(rx + s * 0.1), p.deck - 0.04, ze + 0.05), 0.015, CARBON);
        let guard = [v(hw - 0.02, p.deck - 0.06, ze), v(hw + 0.14, p.deck - 0.1, ze), v(hw - 0.06, p.prow + 0.08, p.front - 0.12), v(hw - 0.3, p.prow + 0.14, p.front - 0.12)];
        plate(&mut m, if s > 0.0 { guard } else { mirror_x(guard) }, 0.06, body, PAINT);
    }

    HullModel {
        body: m,
        mount: v(0.0, p.deck - 0.05, p.glacis - 1.3),
        shock_top: p.deck - 0.28,
        running: p.running,
    }
}

/// Light hull: low and long, two-layer glacis, three skirt segments.
pub fn lancer() -> HullModel {
    hull(HullSpec {
        front: 2.85,
        rear: -2.65,
        hw: 1.55,
        deck: 1.42,
        glacis: 1.2,
        prow: 0.95,
        skirts: 3,
        skirt_low: 0.8,
        heavy: false,
        running: Running {
            track_x: 1.22,
            track_width: 0.56,
            sprocket: Wheel { center: Vec2::new(2.05, 0.66), radius: 0.36 },
            idler: Wheel { center: Vec2::new(-2.1, 0.62), radius: 0.32 },
            road: [-1.35f32, -0.45, 0.45, 1.35].iter().map(|&z| Wheel { center: Vec2::new(z, 0.38), radius: 0.3 }).collect(),
        },
    })
}

/// Heavy hull: wider and taller, three-layer glacis, four skirt segments.
pub fn bastion() -> HullModel {
    hull(HullSpec {
        front: 2.95,
        rear: -2.75,
        hw: 1.68,
        deck: 1.6,
        glacis: 0.95,
        prow: 1.02,
        skirts: 4,
        skirt_low: 0.78,
        heavy: true,
        running: Running {
            track_x: 1.28,
            track_width: 0.64,
            sprocket: Wheel { center: Vec2::new(-2.15, 0.7), radius: 0.38 },
            idler: Wheel { center: Vec2::new(2.15, 0.68), radius: 0.33 },
            road: [-1.6f32, -0.8, 0.0, 0.8, 1.6].iter().map(|&z| Wheel { center: Vec2::new(z, 0.4), radius: 0.32 }).collect(),
        },
    })
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
    /// Elevation ram: anchored on the turret (turret space), pushing a lug on the gun (gun space).
    pub ram_anchor: Vec3,
    pub ram_lug: Vec3,
}

/// Recoil cylinder housings on the cradle; matching rods ride on the barrel.
fn recoil_housings(m: &mut MeshData, at: &[Vec3]) {
    for &p in at {
        round(m, &[at_z(&ngon(10, 0.06, Vec2::new(p.x, p.y)), -0.1), at_z(&ngon(10, 0.06, Vec2::new(p.x, p.y)), 0.42)], 0.015, CARBON_LIGHT);
    }
}

fn recoil_rods(m: &mut MeshData, at: &[Vec3]) {
    for &p in at {
        round(m, &[at_z(&ngon(8, 0.028, Vec2::new(p.x, p.y)), 0.2), at_z(&ngon(8, 0.028, Vec2::new(p.x, p.y)), 0.9)], 0.0, METAL);
        block(m, v(p.x - 0.05, p.y - 0.05, 0.85), v(p.x + 0.05, p.y + 0.05, 0.98), 0.015, METAL);
    }
}

/// Turret race with an exposed ring gear: the teeth turn with the turret.
fn turret_ring(m: &mut MeshData) {
    round(m, &[at_y(&ngon(16, 0.86, Vec2::ZERO), -0.05), at_y(&ngon(16, 0.82, Vec2::ZERO), 0.14)], 0.02, CARBON);
    for i in 0..32 {
        let a = i as f32 / 32.0 * TAU;
        let d = Vec2::new(a.cos(), a.sin());
        let side = Vec2::new(-d.y, d.x) * 0.035;
        let (b0, b1) = (d * 0.84, d * 0.93);
        let tooth = [b0 - side, b0 + side, b1 + side * 0.5, b1 - side * 0.5];
        round(m, &[at_y(&tooth, -0.04), at_y(&tooth, 0.04)], 0.006, METAL);
    }
}

struct HeadSpec {
    /// Half width at the shoulders.
    w: f32,
    /// Roof height.
    h: f32,
    front: f32,
    rear: f32,
}

/// Shared turret form: an arrowhead in plan view (wide shoulders, front
/// cheeks raking in to the gun), stepped cheek armor, roof plates, a
/// commander's cupola, a sensor housing, smoke dischargers and a bustle rack.
fn head(p: &HeadSpec) -> MeshData {
    let mut m = MeshData::default();
    let HeadSpec { w, h, front, rear } = *p;
    let body = v(0.0, h * 0.5, rear * 0.3);
    let bot = 0.14;
    turret_ring(&mut m);
    solid(
        &mut m,
        &[
            at_z(&trap(w * 0.6, w * 0.52, bot + 0.06, h - 0.06, 0.06), rear),
            at_z(&trap(w * 0.94, w * 0.86, bot, h, 0.08), rear + 0.32),
            at_z(&trap(w, w * 0.9, bot, h, 0.08), 0.05),
            at_z(&trap(w * 0.38, w * 0.32, bot + 0.06, h - 0.12, 0.06), front),
        ],
        0.04,
        CARBON_LIGHT,
    );
    // Cheek armor: two stepped layers on each raked front face.
    for s in [1.0f32, -1.0] {
        let q = [v(s * w * 0.98, h - 0.03, 0.08), v(s * w * 0.4, h - 0.12, front - 0.02), v(s * w * 0.4, bot + 0.08, front - 0.02), v(s * w * 0.98, bot + 0.03, 0.08)];
        layered(&mut m, inset(q, 0.03), body, &[(0.02, 0.07, PAINT), (0.12, 0.06, PAINT)]);
        // Side armor along the flank.
        let side = [v(s * w * 0.99, h - 0.05, rear + 0.38), v(s * w * 0.99, h - 0.05, 0.0), v(s * w * 0.99, bot + 0.05, 0.0), v(s * w * 0.99, bot + 0.08, rear + 0.38)];
        armor(&mut m, inset(side, 0.04), body, 0.05, PAINT);
        // Roof plates either side of the center channel.
        let roof = [v(s * 0.16, h, rear + 0.42), v(s * w * 0.8, h, rear + 0.42), v(s * w * 0.8, h, -0.05), v(s * 0.16, h, -0.05)];
        armor(&mut m, roof, body, 0.035, PAINT);
        // Smoke dischargers on a bracket at each shoulder.
        let bx = s * w * 0.82;
        block(&mut m, v(bx - 0.14, h - 0.02, rear + 0.6), v(bx + 0.14, h + 0.08, rear + 0.85), 0.02, CARBON);
        for k in 0..3 {
            let base = v(bx + (k as f32 - 1.0) * 0.09, h + 0.06, rear + 0.75);
            tube(&mut m, base, base + v(s * 0.08, 0.14, 0.2), 0.04, 6, CARBON);
        }
    }
    // Commander's cupola with periscopes, offset right.
    let c = Vec2::new(w * 0.45, rear + 0.95);
    round(&mut m, &[at_y(&ngon(8, 0.3, c), h - 0.02), at_y(&ngon(8, 0.27, c), h + 0.16)], 0.03, CARBON);
    round(&mut m, &[at_y(&ngon(8, 0.21, c), h + 0.15), at_y(&ngon(8, 0.18, c), h + 0.21)], 0.02, PAINT);
    for k in 0..5 {
        let a = std::f32::consts::FRAC_PI_2 + (k as f32 - 2.0) * 0.55;
        let d = Vec2::new(a.cos(), a.sin()) * 0.29 + c;
        block(&mut m, v(d.x - 0.05, h + 0.04, d.y - 0.05), v(d.x + 0.05, h + 0.13, d.y + 0.05), 0.015, CARBON_LIGHT);
    }
    // Sensor housing, offset left.
    let sx = -w * 0.5;
    block(&mut m, v(sx - 0.2, h - 0.02, -0.3), v(sx + 0.2, h + 0.24, 0.1), 0.04, CARBON);
    plate(&mut m, [v(sx - 0.14, h + 0.19, 0.11), v(sx + 0.14, h + 0.19, 0.11), v(sx + 0.14, h + 0.04, 0.11), v(sx - 0.14, h + 0.04, 0.11)], 0.012, body, LENS);
    // Bustle rack: two rails on uprights around the rear.
    for y in [h - 0.05, h + 0.12] {
        tube(&mut m, v(-w * 0.62, y, rear - 0.22), v(w * 0.62, y, rear - 0.22), 0.025, 6, CARBON);
    }
    for x in [-w * 0.55, 0.0, w * 0.55] {
        tube(&mut m, v(x, h - 0.12, rear + 0.05), v(x, h - 0.12, rear - 0.24), 0.025, 6, CARBON);
        tube(&mut m, v(x, h - 0.12, rear - 0.22), v(x, h + 0.14, rear - 0.22), 0.025, 6, CARBON);
    }
    m
}

/// Long main gun: arrowhead turret, single barrel with a fume extractor and
/// thermal sleeve bands.
pub fn lance() -> TurretModel {
    let spec = HeadSpec { w: 1.17, h: 0.74, front: 1.15, rear: -1.55 };
    let mut head = head(&spec);
    let pivot = v(0.0, 0.44, spec.front - 0.12);
    // Ram mount on the roof channel.
    block(&mut head, v(-0.1, spec.h - 0.02, pivot.z - 0.75), v(0.1, spec.h + 0.1, pivot.z - 0.55), 0.02, METAL);

    let mut mantlet = MeshData::default();
    solid(&mut mantlet, &[at_z(&rect(Vec2::ZERO, Vec2::new(0.34, 0.25), 0.08), -0.35), at_z(&rect(Vec2::ZERO, Vec2::new(0.3, 0.21), 0.07), 0.3)], 0.03, CARBON);
    armor(&mut mantlet, [v(-0.28, 0.2, 0.31), v(0.28, 0.2, 0.31), v(0.28, -0.18, 0.31), v(-0.28, -0.18, 0.31)], Vec3::ZERO, 0.05, PAINT);
    round(&mut mantlet, &[at_x(&ngon(10, 0.11, Vec2::ZERO), -0.46), at_x(&ngon(10, 0.11, Vec2::ZERO), 0.46)], 0.02, METAL);
    let cyl = [v(-0.2, 0.3, 0.0), v(0.2, 0.3, 0.0)];
    recoil_housings(&mut mantlet, &cyl);
    block(&mut mantlet, v(-0.06, 0.24, -0.1), v(0.06, 0.36, 0.1), 0.02, METAL);

    let mut barrel = MeshData::default();
    let oct = |r: f32| ngon(10, r, Vec2::ZERO);
    round(&mut barrel, &[at_z(&oct(0.17), 0.2), at_z(&oct(0.16), 1.2)], 0.02, CARBON);
    round(&mut barrel, &[at_z(&oct(0.135), 1.15), at_z(&oct(0.125), 3.65)], 0.0, CARBON_LIGHT);
    // Fume extractor bulge and thermal sleeve bands.
    round(&mut barrel, &[at_z(&oct(0.15), 1.75), at_z(&oct(0.2), 1.9), at_z(&oct(0.2), 2.2), at_z(&oct(0.15), 2.35)], 0.0, CARBON);
    for z in [1.3, 2.75, 3.2] {
        round(&mut barrel, &[at_z(&oct(0.15), z), at_z(&oct(0.15), z + 0.07)], 0.01, CARBON);
    }
    solid(&mut barrel, &[at_z(&ngon(6, 0.2, Vec2::ZERO), 3.6), at_z(&ngon(6, 0.22, Vec2::ZERO), 3.95)], 0.025, CARBON);
    round(&mut barrel, &[at_z(&ngon(10, 0.1, Vec2::ZERO), 3.85), at_z(&ngon(10, 0.1, Vec2::ZERO), 3.955)], 0.0, BORE);
    for s in [-1.0f32, 1.0] {
        block(&mut barrel, v(s * 0.17 - 0.03, -0.045, 3.68), v(s * 0.17 + 0.06, 0.045, 3.8), 0.012, BORE);
    }
    recoil_rods(&mut barrel, &cyl);

    TurretModel {
        head,
        mantlet,
        barrel,
        pivot,
        barrels: vec![Vec3::ZERO],
        muzzle: 4.0,
        recoil: 0.45,
        ram_anchor: v(0.0, spec.h + 0.05, pivot.z - 0.65),
        ram_lug: v(0.0, 0.3, 0.0),
    }
}

/// Twin short cannons in a wider, lower arrowhead turret.
pub fn maul() -> TurretModel {
    let spec = HeadSpec { w: 1.24, h: 0.64, front: 0.95, rear: -1.45 };
    let mut head = head(&spec);
    let pivot = v(0.0, 0.38, spec.front - 0.1);
    block(&mut head, v(-0.1, spec.h - 0.02, pivot.z - 0.7), v(0.1, spec.h + 0.1, pivot.z - 0.5), 0.02, METAL);

    let mut mantlet = MeshData::default();
    solid(&mut mantlet, &[at_z(&rect(Vec2::ZERO, Vec2::new(0.6, 0.22), 0.07), -0.3), at_z(&rect(Vec2::ZERO, Vec2::new(0.56, 0.19), 0.06), 0.3)], 0.03, CARBON);
    armor(&mut mantlet, [v(-0.16, 0.17, 0.31), v(0.16, 0.17, 0.31), v(0.16, -0.15, 0.31), v(-0.16, -0.15, 0.31)], Vec3::ZERO, 0.05, PAINT);
    round(&mut mantlet, &[at_x(&ngon(10, 0.1, Vec2::ZERO), -0.7), at_x(&ngon(10, 0.1, Vec2::ZERO), 0.7)], 0.02, METAL);
    for s in [-1.0f32, 1.0] {
        recoil_housings(&mut mantlet, &[v(s * 0.34, 0.27, 0.0)]);
    }
    block(&mut mantlet, v(-0.06, 0.2, -0.05), v(0.06, 0.34, 0.15), 0.02, METAL);

    let mut barrel = MeshData::default();
    recoil_rods(&mut barrel, &[v(0.0, 0.27, 0.0)]);
    let sq = |h: f32| rect(Vec2::ZERO, Vec2::splat(h), h * 0.3);
    solid(&mut barrel, &[at_z(&sq(0.17), 0.1), at_z(&sq(0.16), 1.45)], 0.03, CARBON);
    round(&mut barrel, &[at_z(&ngon(8, 0.105, Vec2::ZERO), 1.4), at_z(&ngon(8, 0.1, Vec2::ZERO), 2.45)], 0.0, METAL);
    solid(&mut barrel, &[at_z(&sq(0.15), 2.4), at_z(&sq(0.16), 2.78)], 0.025, CARBON);
    round(&mut barrel, &[at_z(&ngon(8, 0.07, Vec2::ZERO), 2.7), at_z(&ngon(8, 0.07, Vec2::ZERO), 2.785)], 0.0, BORE);

    TurretModel {
        head,
        mantlet,
        barrel,
        pivot,
        barrels: vec![v(-0.34, 0.0, 0.0), v(0.34, 0.0, 0.0)],
        muzzle: 2.82,
        recoil: 0.35,
        ram_anchor: v(0.0, spec.h + 0.05, pivot.z - 0.6),
        ram_lug: v(0.0, 0.3, 0.05),
    }
}

// ---------------------------------------------------------------------------
// Exposed machinery (drawn between moving frames each frame)

/// Nominal suspension arm length; instances stretch it slightly to fit.
pub const ARM_LEN: f32 = 0.62;
/// Hydraulic cylinder and rod lengths (they telescope over each other).
pub const CYL_LEN: f32 = 0.32;
pub const ROD_LEN: f32 = 0.36;

/// Suspension arm along +z from its pivot (z = 0) to the wheel hub (z = ARM_LEN).
pub fn arm() -> MeshData {
    let mut m = MeshData::default();
    solid(&mut m, &[at_z(&rect(Vec2::ZERO, Vec2::new(0.05, 0.09), 0.025), 0.0), at_z(&rect(Vec2::ZERO, Vec2::new(0.045, 0.06), 0.02), ARM_LEN)], 0.015, CARBON_LIGHT);
    for z in [0.0, ARM_LEN] {
        round(&mut m, &[at_x(&ngon(10, 0.1, Vec2::new(z, 0.0)), -0.065), at_x(&ngon(10, 0.1, Vec2::new(z, 0.0)), 0.065)], 0.015, METAL);
    }
    m
}

/// Hydraulic cylinder body along +z from its mount eye.
pub fn cylinder() -> MeshData {
    let mut m = MeshData::default();
    round(&mut m, &[at_x(&ngon(8, 0.045, Vec2::ZERO), -0.04), at_x(&ngon(8, 0.045, Vec2::ZERO), 0.04)], 0.01, METAL);
    round(&mut m, &[at_z(&ngon(10, 0.055, Vec2::ZERO), 0.02), at_z(&ngon(10, 0.055, Vec2::ZERO), CYL_LEN)], 0.012, CARBON);
    round(&mut m, &[at_z(&ngon(10, 0.062, Vec2::ZERO), CYL_LEN - 0.05), at_z(&ngon(10, 0.062, Vec2::ZERO), CYL_LEN)], 0.01, METAL);
    m
}

/// Polished piston rod along +z from its eye.
pub fn rod() -> MeshData {
    let mut m = MeshData::default();
    round(&mut m, &[at_x(&ngon(8, 0.04, Vec2::ZERO), -0.035), at_x(&ngon(8, 0.04, Vec2::ZERO), 0.035)], 0.01, METAL);
    round(&mut m, &[at_z(&ngon(8, 0.024, Vec2::ZERO), 0.0), at_z(&ngon(8, 0.024, Vec2::ZERO), ROD_LEN)], 0.0, METAL);
    m
}
