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
pub const BORE: Skin = Skin { face: Vec4::new(0.01, 0.01, 0.012, mat::STEEL), edge: Vec4::new(0.01, 0.01, 0.012, mat::STEEL) };

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

/// Non-uniform scale of a finished mesh (normals corrected).
pub fn stretch(m: &mut MeshData, k: Vec3) {
    for vtx in &mut m.verts {
        vtx.pos = (Vec3::from(vtx.pos) * k).to_array();
        vtx.normal = (Vec3::from(vtx.normal) / k).normalize_or_zero().to_array();
    }
}

/// Low, long proportions (design direction: silhouette first). Applied to
/// every hull and turret after it's built.
pub const HULL_STRETCH: Vec3 = Vec3::new(1.0, 0.9, 1.1);
pub const HEAD_STRETCH: Vec3 = Vec3::new(1.05, 0.85, 1.1);
pub const BARREL_STRETCH: Vec3 = Vec3::new(1.0, 1.0, 1.2);

impl HullModel {
    pub fn stretched(mut self) -> HullModel {
        let k = HULL_STRETCH;
        stretch(&mut self.body, k);
        self.mount *= k;
        self.shock_top *= k.y;
        let r = &mut self.running;
        for w in r.road.iter_mut().chain([&mut r.sprocket, &mut r.idler]) {
            w.center.x *= k.z;
        }
        self
    }
}

impl TurretModel {
    pub fn stretched(mut self) -> TurretModel {
        let (h, b) = (HEAD_STRETCH, BARREL_STRETCH);
        let gun = Vec3::new(h.x, h.y, 1.0);
        stretch(&mut self.head, h);
        stretch(&mut self.mantlet, gun);
        stretch(&mut self.barrel, b);
        self.pivot *= h;
        self.ram_anchor *= h;
        self.ram_lug *= gun;
        for o in &mut self.barrels {
            *o *= gun;
        }
        self.muzzle *= b.z;
        self
    }
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

    // Fenders: one long slab per side, then a forward slope.
    plate2(&mut m, [v(0.98, 1.5, -2.4), v(1.7, 1.3, -2.4), v(1.7, 1.3, 1.05), v(0.98, 1.5, 1.05)], 0.08, inside, PAINT);
    plate2(&mut m, [v(0.98, 1.5, 1.09), v(1.66, 1.3, 1.09), v(1.52, 0.98, 2.3), v(0.96, 1.2, 2.3)], 0.07, inside, PAINT);
    // Claw plates angling down over the front of the sprockets.
    plate2(&mut m, [v(0.95, 1.16, 2.33), v(1.55, 0.95, 2.33), v(1.42, 0.5, 2.68), v(0.98, 0.6, 2.62)], 0.065, inside, PAINT);

    // No side skirts: the suspension arms and shocks stay exposed.
    // Blunt armor wedge bridging the fender and the front plate.
    plate2(&mut m, [v(1.62, 1.27, 1.75), v(1.5, 1.05, 2.3), v(1.52, 0.86, 2.2), v(1.72, 1.1, 1.75)], 0.05, inside, CARBON_LIGHT);

    // Raised glacis plate with twin light bars.
    plate(&mut m, [v(-0.6, 1.42, 1.1), v(0.6, 1.42, 1.1), v(0.42, 1.11, 2.38), v(-0.42, 1.11, 2.38)], 0.06, inside, CARBON_LIGHT);
    // Nose: armored beak under the glacis.
    plate(&mut m, [v(-0.66, 1.0, 2.5), v(0.66, 1.0, 2.5), v(0.5, 0.62, 2.62), v(-0.5, 0.62, 2.62)], 0.06, inside, PAINT);

    // Tail lights and exhaust boxes on the rear plate.

    HullModel {
        body: m,
        running: Running {
            track_x: 1.22,
            track_width: 0.56,
            sprocket: Wheel { center: Vec2::new(2.05, 0.66), radius: 0.36 },
            idler: Wheel { center: Vec2::new(-2.1, 0.62), radius: 0.32 },
            road: [-1.35f32, -0.45, 0.45, 1.35].iter().map(|&z| Wheel { center: Vec2::new(z, 0.38), radius: 0.3 }).collect(),
        },
        mount: v(0.0, 1.42, -0.3),
        shock_top: 1.28,
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
    plate2(&mut m, [v(0.18, 1.66, 1.25), v(1.42, 1.66, 1.25), v(1.24, 1.38, 2.35), v(0.16, 1.38, 2.35)], 0.08, inside, PAINT);
    plate(&mut m, [v(-0.16, 1.7, 1.15), v(0.16, 1.7, 1.15), v(0.13, 1.4, 2.4), v(-0.13, 1.4, 2.4)], 0.06, inside, CARBON_LIGHT);
    // Bull-nose lower plate with light slits.
    plate(&mut m, [v(-1.25, 1.12, 2.62), v(1.25, 1.12, 2.62), v(1.05, 0.6, 2.72), v(-1.05, 0.6, 2.72)], 0.08, inside, PAINT);

    // Two long upper skirt slabs per side, set high so the legs show.
    for (z0, z1, x) in [(-2.45f32, 0.1f32, 1.68f32), (0.0, 2.3, 1.72)] {
        plate2(&mut m, [v(x, 1.48, z0), v(x, 1.48, z1), v(x + 0.07, 1.14, z1 - 0.15), v(x + 0.07, 1.14, z0)], 0.08, inside, PAINT);
    }
    // Top of the skirts: armored mud guard shelf.
    plate2(&mut m, [v(1.36, 1.6, -2.4), v(1.8, 1.45, -2.4), v(1.8, 1.45, 2.2), v(1.36, 1.6, 2.2)], 0.06, inside, CARBON_LIGHT);

    // Rear deck: one raised engine block.
    block(&mut m, v(-0.9, 1.58, -2.4), v(0.9, 1.78, -1.4), 0.05, CARBON_LIGHT);

    HullModel {
        body: m,
        running: Running {
            track_x: 1.28,
            track_width: 0.64,
            sprocket: Wheel { center: Vec2::new(-2.15, 0.7), radius: 0.38 },
            idler: Wheel { center: Vec2::new(2.15, 0.68), radius: 0.33 },
            road: [-1.6f32, -0.8, 0.0, 0.8, 1.6].iter().map(|&z| Wheel { center: Vec2::new(z, 0.4), radius: 0.32 }).collect(),
        },
        mount: v(0.0, 1.62, -0.2),
        shock_top: 1.14,
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
    round(m, &[at_y(&ngon(16, 0.86, Vec2::ZERO), -0.05), at_y(&ngon(16, 0.82, Vec2::ZERO), 0.12)], 0.02, CARBON);
    for i in 0..32 {
        let a = i as f32 / 32.0 * TAU;
        let d = Vec2::new(a.cos(), a.sin());
        let side = Vec2::new(-d.y, d.x) * 0.035;
        let (b0, b1) = (d * 0.84, d * 0.93);
        let tooth = [b0 - side, b0 + side, b1 + side * 0.5, b1 - side * 0.5];
        round(m, &[at_y(&tooth, -0.04), at_y(&tooth, 0.04)], 0.006, METAL);
    }
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
    plate2(&mut head, [v(0.84, 0.74, -0.92), v(0.84, 0.74, 0.36), v(0.96, 0.22, 0.28), v(0.94, 0.22, -0.98)], 0.07, inside, PAINT);
    plate2(&mut head, [v(0.84, 0.74, 0.4), v(0.52, 0.7, 0.98), v(0.54, 0.2, 1.0), v(0.96, 0.22, 0.32)], 0.07, inside, PAINT);
    // Raised roof panel and a sensor pod with an eye.
    // Blunt periscope housing with a vision block.
    block(&mut head, v(-0.42, 0.86, -1.2), v(-0.1, 1.06, -0.9), 0.04, CARBON_LIGHT);
    plate(&mut head, [v(-0.38, 1.02, -0.89), v(-0.14, 1.02, -0.89), v(-0.14, 0.94, -0.89), v(-0.38, 0.94, -0.89)], 0.015, inside, LENS);
    // Rear light bar.

    let mut mantlet = MeshData::default();
    solid(&mut mantlet, &[at_z(&rect(Vec2::ZERO, Vec2::new(0.28, 0.24), 0.07), -0.3), at_z(&rect(Vec2::ZERO, Vec2::new(0.24, 0.2), 0.06), 0.32)], 0.03, CARBON);
    round(&mut mantlet, &[at_x(&ngon(10, 0.11, Vec2::ZERO), -0.48), at_x(&ngon(10, 0.11, Vec2::ZERO), 0.48)], 0.02, METAL);
    // White yoke brackets either side of the gun.
    for s in [-1.0f32, 1.0] {
        plate(&mut mantlet, [v(s * 0.36, 0.28, -0.25), v(s * 0.36, 0.28, 0.25), v(s * 0.36, -0.18, 0.18), v(s * 0.36, -0.18, -0.28)], 0.07, Vec3::ZERO, PAINT);
    }

    let mut barrel = MeshData::default();
    let oct = |r: f32| ngon(8, r, Vec2::ZERO);
    // Breech sleeve, then the long tube.
    solid(&mut barrel, &[at_z(&oct(0.2), 0.15), at_z(&oct(0.2), 1.3)], 0.03, CARBON);
    round(&mut barrel, &[at_z(&oct(0.145), 1.25), at_z(&oct(0.135), 3.15)], 0.0, CARBON_LIGHT);
    // Coil sleeves with glowing slots on each side.
    for i in 0..1 {
        let z = 1.4 + i as f32 * 0.5;
        solid(&mut barrel, &[at_z(&oct(0.185), z), at_z(&oct(0.185), z + 0.36)], 0.025, CARBON);

    }
    // Hex muzzle with a dark bore.
    solid(&mut barrel, &[at_z(&ngon(6, 0.24, Vec2::ZERO), 3.08), at_z(&ngon(6, 0.27, Vec2::ZERO), 3.5)], 0.03, CARBON);
    round(&mut barrel, &[at_z(&ngon(10, 0.12, Vec2::ZERO), 3.4), at_z(&ngon(10, 0.12, Vec2::ZERO), 3.505)], 0.0, BORE);
    // Muzzle brake ports.
    for s in [-1.0f32, 1.0] {
        block(&mut barrel, v(s * 0.2 - 0.03, -0.05, 3.18), v(s * 0.2 + 0.07, 0.05, 3.3), 0.015, BORE);
    }

    let cyl = [v(-0.24, 0.17, 0.0), v(0.24, 0.17, 0.0)];
    recoil_housings(&mut mantlet, &cyl);
    recoil_rods(&mut barrel, &cyl);
    // Lug for the elevation ram on top of the cradle.
    block(&mut mantlet, v(-0.06, 0.2, 0.0), v(0.06, 0.34, 0.2), 0.02, METAL);
    block(&mut head, v(-0.1, 0.84, -0.32), v(0.1, 0.98, -0.12), 0.02, METAL);

    TurretModel {
        head,
        mantlet,
        barrel,
        pivot: v(0.0, 0.45, 0.88),
        barrels: vec![Vec3::ZERO],
        muzzle: 3.55,
        recoil: 0.45,
        ram_anchor: v(0.0, 0.92, -0.22),
        ram_lug: v(0.0, 0.3, 0.1),
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
    plate2(&mut head, [v(0.5, 0.82, -1.0), v(0.5, 0.82, 0.68), v(1.1, 0.5, 0.56), v(1.1, 0.5, -0.94)], 0.08, inside, PAINT);
    plate2(&mut head, [v(1.1, 0.48, -0.9), v(1.1, 0.48, 0.5), v(1.06, 0.12, 0.45), v(1.06, 0.12, -0.85)], 0.06, inside, CARBON_LIGHT);
    plate(&mut head, [v(-0.48, 0.8, 0.66), v(0.48, 0.8, 0.66), v(0.44, 0.6, 1.04), v(-0.44, 0.6, 1.04)], 0.06, inside, PAINT);
    // Ammo drum housing at the back.
    solid(&mut head, &[at_x(&ngon(8, 0.32, Vec2::new(-1.2, 0.5)), -0.55), at_x(&ngon(8, 0.32, Vec2::new(-1.2, 0.5)), 0.55)], 0.04, CARBON_LIGHT);

    let mut mantlet = MeshData::default();
    solid(&mut mantlet, &[at_z(&rect(Vec2::ZERO, Vec2::new(0.62, 0.22), 0.07), -0.3), at_z(&rect(Vec2::ZERO, Vec2::new(0.58, 0.19), 0.06), 0.3)], 0.03, CARBON);
    round(&mut mantlet, &[at_x(&ngon(10, 0.1, Vec2::ZERO), -0.72), at_x(&ngon(10, 0.1, Vec2::ZERO), 0.72)], 0.02, METAL);
    plate(&mut mantlet, [v(-0.12, 0.22, -0.2), v(0.12, 0.22, -0.2), v(0.12, 0.22, 0.25), v(-0.12, 0.22, 0.25)], 0.03, Vec3::ZERO, PAINT);

    for s in [-1.0f32, 1.0] {
        recoil_housings(&mut mantlet, &[v(s * 0.34, 0.27, 0.0)]);
    }
    block(&mut mantlet, v(-0.06, 0.2, 0.05), v(0.06, 0.34, 0.25), 0.02, METAL);
    block(&mut head, v(-0.1, 0.8, 0.15), v(0.1, 0.92, 0.35), 0.02, METAL);

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
        pivot: v(0.0, 0.38, 0.92),
        barrels: vec![v(-0.34, 0.0, 0.0), v(0.34, 0.0, 0.0)],
        muzzle: 2.82,
        recoil: 0.35,
        ram_anchor: v(0.0, 0.88, 0.25),
        ram_lug: v(0.0, 0.3, 0.15),
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

/// Coil spring over a shock, unit length along +z: drawn scaled to the
/// shock's current length so it visibly compresses.
pub fn spring() -> MeshData {
    let mut m = MeshData::default();
    let coils = 6;
    for i in 0..coils {
        let z = 0.1 + i as f32 / (coils - 1) as f32 * 0.8;
        round(&mut m, &[at_z(&ngon(12, 0.075, Vec2::ZERO), z - 0.025), at_z(&ngon(12, 0.075, Vec2::ZERO), z + 0.025)], 0.01, PAINT);
    }
    m
}
