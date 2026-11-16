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

/// Same skin, a touch lighter or darker, so neighboring panels read apart.
fn shade(s: Skin, k: f32) -> Skin {
    let f = |c: Vec4| Vec4::new(c.x * k, c.y * k, c.z * k, c.w);
    Skin { face: f(s.face), edge: f(s.edge) }
}

/// Mirror a half profile (x >= 0, listed from the top center outward and
/// down) into a full cross-section ring at depth z.
fn full_ring(half: &[Vec2], z: f32) -> Vec<Vec3> {
    let mut r: Vec<Vec3> = half.iter().rev().map(|p| v(-p.x, p.y, z)).collect();
    r.pop(); // the center point appears once
    r.extend(half.iter().map(|p| v(p.x, p.y, z)));
    r
}

/// An armored shell built from separate panels. `stations` are cross-section
/// rings from rear to front. Every facet between two stations becomes its own
/// thick plate, inset to leave a dark gap, with its rear edge lifted and
/// pushed back so it laps over the plate behind it like a scale. A dark core
/// fills the inside so gaps read as depth. `skin(row, col)` picks each
/// panel's finish (row = station band, col = facet around the profile).
fn shell(m: &mut MeshData, stations: &[Vec<Vec3>], t: f32, gap: f32, lap: f32, skin: impl Fn(usize, usize) -> Skin) {
    let core: Vec<Vec<Vec3>> = stations.iter().map(|r| shrink(r, t * 1.2)).collect();
    solid(m, &core, 0.02, CARBON);
    let body = stations.iter().map(|r| centroid(r)).sum::<Vec3>() / stations.len() as f32;
    for (row, w) in stations.windows(2).enumerate() {
        let (rear, front) = (&w[0], &w[1]);
        let band_mid = (centroid(rear) + centroid(front)) * 0.5;
        let inside = band_mid.lerp(body, 0.5);
        for col in 0..rear.len() - 1 {
            let q = [rear[col], rear[col + 1], front[col + 1], front[col]];
            if (q[0] - q[1]).length() < 0.05 && (q[2] - q[3]).length() < 0.05 {
                continue;
            }
            let n = quad_normal(&q, inside);
            let mut q = inset(q, gap);
            // Scale lap: rear edge stands proud and reaches back over the previous plate.
            let back = (q[0] - q[3]).normalize_or_zero();
            for i in [0, 1] {
                q[i] += n * lap + back * lap * 1.6;
            }
            let top = q.map(|p| p + n * t * 1.5);
            plate(m, top, t, band_mid - n * 2.0, skin(row, col));
        }
    }
}

struct HullSpec {
    /// Width and height scale on the base profile.
    sx: f32,
    sy: f32,
    /// Station z positions (rear to front) are scaled by this.
    sz: f32,
    heavy: bool,
    running: Running,
}

/// Half profile of the hull at one station: raised spine, deck, sponson
/// shoulder over the track, sloped upper side, skirt edge.
fn hull_profile(spine: f32, deck: f32, shoulder: Vec2, side: Vec2, low: f32, k: f32) -> Vec<Vec2> {
    vec![
        Vec2::new(0.0, spine),
        Vec2::new(0.3 * k, spine - 0.02),
        Vec2::new(0.62 * k, deck),
        Vec2::new(shoulder.x * k, shoulder.y),
        Vec2::new(side.x * k, side.y),
        Vec2::new((side.x - 0.05) * k, low),
    ]
}

/// Shared hull: a faceted carapace of overlapping armor panels over a dark
/// core, with a pointed, stepped prow and a tapered tail, a lower tub between
/// the tracks, a louvered engine grille, headlight clusters and exhausts.
fn hull(p: HullSpec) -> HullModel {
    let mut m = MeshData::default();
    let (sx, sy, sz) = (p.sx, p.sy, p.sz);
    let y = |h: f32| h * sy;
    let st = |z: f32, spine: f32, deck: f32, sh: (f32, f32), side: (f32, f32), low: f32, k: f32| {
        let half = hull_profile(y(spine), y(deck), Vec2::new(sh.0 * sx, y(sh.1)), Vec2::new(side.0 * sx, y(side.1)), low, k);
        full_ring(&half, z * sz)
    };
    let stations = vec![
        st(-2.78, 1.28, 1.24, (1.25, 1.16), (1.42, 1.0), 0.86, 0.92),
        st(-2.35, 1.46, 1.42, (1.5, 1.31), (1.68, 1.12), 0.76, 1.0),
        st(0.1, 1.56, 1.47, (1.55, 1.35), (1.73, 1.13), 0.73, 1.0),
        st(1.25, 1.5, 1.4, (1.53, 1.29), (1.71, 1.08), 0.75, 1.0),
        st(1.95, 1.3, 1.2, (1.46, 1.1), (1.63, 0.93), 0.72, 0.97),
        st(2.98, 0.9, 0.84, (0.95, 0.78), (1.05, 0.7), 0.62, 0.86),
    ];
    // Finish: spine and deck in paint, shoulders darker steel, sides paint
    // with alternating tone, prow plates a shade lighter.
    let skin = |row: usize, col: usize| {
        let mid = 5; // the ring has 11 points: col 4..6 straddle the spine
        let d = (col as i32 - mid as i32).unsigned_abs() as usize;
        let base = match d {
            0 | 1 => PAINT,
            2 => CARBON_LIGHT,
            3 => PAINT,
            _ => CARBON_LIGHT,
        };
        let k = if (row + d) % 2 == 0 { 1.0 } else { 0.86 };
        let k = if row >= 4 { k * 1.08 } else { k };
        shade(base, k)
    };
    shell(&mut m, &stations, 0.06, 0.022, 0.025, skin);

    // Lower tub between the tracks.
    let tub_top = y(1.1);
    solid(
        &mut m,
        &[
            at_z(&trap(0.8, 0.86, 0.5, tub_top, 0.08), -2.6 * sz),
            at_z(&trap(0.88, 0.9, 0.32, tub_top, 0.1), -2.2 * sz),
            at_z(&trap(0.88, 0.9, 0.32, tub_top, 0.1), 2.0 * sz),
            at_z(&trap(0.74, 0.8, 0.58, y(0.85), 0.08), 2.8 * sz),
        ],
        0.04,
        CARBON,
    );
    let body = v(0.0, y(1.0), 0.0);

    // Engine grille on the rear deck: louvers in a recessed frame.
    let (ez0, ez1, ex) = (-2.2 * sz, -1.45 * sz, 0.55 * sx);
    let gy = y(1.47) + 0.1;
    block(&mut m, v(-ex - 0.07, gy - 0.12, ez0 - 0.07), v(ex + 0.07, gy, ez1 + 0.07), 0.02, CARBON);
    for i in 0..6 {
        let z = ez0 + 0.02 + i as f32 * (ez1 - ez0) / 6.0;
        block(&mut m, v(-ex, gy - 0.04, z), v(ex, gy + 0.02, z + 0.06), 0.01, METAL);
    }
    // Headlight clusters tucked under the prow's shoulders.
    for s in [1.0f32, -1.0] {
        let c = v(s * 1.05 * sx, y(0.86), 2.62 * sz);
        block(&mut m, c - v(0.16, 0.07, 0.12), c + v(0.16, 0.07, 0.04), 0.025, CARBON);
        for k in [-0.08f32, 0.08] {
            plate(&mut m, [c + v(k - 0.05, 0.04, 0.05), c + v(k + 0.05, 0.04, 0.05), c + v(k + 0.05, -0.04, 0.05), c + v(k - 0.05, -0.04, 0.05)], 0.01, body, LENS);
        }
        // Exhausts out of the tail.
        let e = v(s * 0.6 * sx, y(1.05), -2.8 * sz);
        tube(&mut m, e + v(0.0, 0.0, 0.25), e - v(0.0, 0.0, 0.12), 0.09, 8, CARBON);
        // Small tow eyes, flush with the tub ends.
        block(&mut m, v(s * 0.5 - 0.05, 0.62, 2.72 * sz), v(s * 0.5 + 0.05, 0.72, 2.84 * sz), 0.02, CARBON);
        block(&mut m, v(s * 0.5 - 0.05, 0.62, -2.68 * sz), v(s * 0.5 + 0.05, 0.72, -2.56 * sz), 0.02, CARBON);
    }
    if p.heavy {
        // Extra spaced armor: a second layer of plates on the glacis shoulders.
        for s in [1.0f32, -1.0] {
            let q = [v(s * 0.4 * sx, y(1.42), 1.3 * sz), v(s * 1.3 * sx, y(1.27), 1.3 * sz), v(s * 1.2 * sx, y(1.02), 2.05 * sz), v(s * 0.4 * sx, y(1.15), 2.05 * sz)];
            armor(&mut m, q, body, 0.05, shade(PAINT, 0.92));
        }
    }

    HullModel { body: m, mount: v(0.0, y(1.5), -0.35 * sz), shock_top: y(1.0), running: p.running }
}

/// Light hull.
pub fn lancer() -> HullModel {
    hull(HullSpec {
        sx: 1.0,
        sy: 1.0,
        sz: 1.0,
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

/// Heavy hull: the same carapace, wider, taller and longer, with spaced armor.
pub fn bastion() -> HullModel {
    hull(HullSpec {
        sx: 1.08,
        sy: 1.1,
        sz: 1.04,
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

/// Shared turret: a faceted arrowhead carapace of overlapping panels (roof,
/// shoulder bevels, raked cheeks, flanks) over a dark core, plus a cupola, a
/// sensor housing, smoke dischargers and a bustle rack.
fn head(p: &HeadSpec) -> MeshData {
    let mut m = MeshData::default();
    let HeadSpec { w, h, front, rear } = *p;
    let body = v(0.0, h * 0.5, rear * 0.3);
    turret_ring(&mut m);
    let half = |k: f32, top: f32| {
        vec![
            Vec2::new(0.0, top),
            Vec2::new(w * 0.42 * k, top),
            Vec2::new(w * 0.86 * k, top - 0.1),
            Vec2::new(w * k, top - 0.26),
            Vec2::new(w * 0.96 * k, 0.12),
        ]
    };
    let stations = vec![
        full_ring(&half(0.62, h - 0.1), rear),
        full_ring(&half(0.92, h), rear + 0.35),
        full_ring(&half(1.0, h - 0.02), -0.05),
        full_ring(&half(0.72, h - 0.06), front - 0.42),
        full_ring(&half(0.4, h - 0.14), front),
    ];
    let skin = |row: usize, col: usize| {
        let d = (col as i32 - 4).unsigned_abs() as usize;
        let base = if d <= 1 || d == 3 { PAINT } else { CARBON_LIGHT };
        shade(base, if (row + d) % 2 == 0 { 1.0 } else { 0.87 })
    };
    shell(&mut m, &stations, 0.055, 0.02, 0.022, skin);

    for s in [1.0f32, -1.0] {
        // Smoke dischargers on a bracket at each shoulder.
        let bx = s * w * 0.8;
        block(&mut m, v(bx - 0.14, h - 0.08, rear + 0.55), v(bx + 0.14, h + 0.04, rear + 0.8), 0.02, CARBON);
        for k in 0..3 {
            let base = v(bx + (k as f32 - 1.0) * 0.09, h + 0.02, rear + 0.7);
            tube(&mut m, base, base + v(s * 0.08, 0.14, 0.2), 0.04, 6, CARBON);
        }
    }
    // Commander's cupola with periscopes, offset right.
    let c = Vec2::new(w * 0.42, rear + 0.95);
    round(&mut m, &[at_y(&ngon(8, 0.3, c), h - 0.02), at_y(&ngon(8, 0.27, c), h + 0.18)], 0.03, CARBON);
    round(&mut m, &[at_y(&ngon(8, 0.21, c), h + 0.17), at_y(&ngon(8, 0.18, c), h + 0.23)], 0.02, PAINT);
    for k in 0..5 {
        let a = std::f32::consts::FRAC_PI_2 + (k as f32 - 2.0) * 0.55;
        let d = Vec2::new(a.cos(), a.sin()) * 0.29 + c;
        block(&mut m, v(d.x - 0.05, h + 0.06, d.y - 0.05), v(d.x + 0.05, h + 0.15, d.y + 0.05), 0.015, CARBON_LIGHT);
    }
    // Sensor housing, offset left.
    let sx = -w * 0.48;
    block(&mut m, v(sx - 0.2, h - 0.02, -0.4), v(sx + 0.2, h + 0.24, 0.0), 0.04, CARBON);
    plate(&mut m, [v(sx - 0.14, h + 0.19, 0.01), v(sx + 0.14, h + 0.19, 0.01), v(sx + 0.14, h + 0.04, 0.01), v(sx - 0.14, h + 0.04, 0.01)], 0.012, body, LENS);
    // Bustle rack around the rear.
    for yy in [h - 0.12, h + 0.06] {
        tube(&mut m, v(-w * 0.62, yy, rear - 0.22), v(w * 0.62, yy, rear - 0.22), 0.025, 6, CARBON);
    }
    for x in [-w * 0.55, 0.0, w * 0.55] {
        tube(&mut m, v(x, h - 0.18, rear + 0.05), v(x, h - 0.18, rear - 0.24), 0.025, 6, CARBON);
        tube(&mut m, v(x, h - 0.18, rear - 0.22), v(x, h + 0.08, rear - 0.22), 0.025, 6, CARBON);
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
    block(&mut mantlet, v(-0.06, 0.2, -0.05), v(0.06, 0.34, 0.15), 0.02, METAL);

    let mut barrel = MeshData::default();
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
