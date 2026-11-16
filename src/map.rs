//! Arena geometry: the hand-built maps (Saltmarket, the Sanctum, the Kiln)
//! and the shapes they're made of: boxes, wedges (ramps), turned boxes,
//! crystals and an optional heightfield ground.

use crate::render::material::Material;
use glam::{Vec2, Vec3};

use crate::faction::Faction;

mod capital;
mod kiln;
mod landmarks;
mod props;
mod ruins;
mod saltmarket;
mod sanctum;
#[cfg(test)]
mod test_kit;
mod valley;

/// Blocking collider.
#[derive(Clone, Copy, Debug)]
pub struct Solid {
    pub min: Vec3,
    pub max: Vec3,
}

impl Solid {
    pub fn min2(&self) -> Vec2 {
        Vec2::new(self.min.x, self.min.z)
    }
    pub fn max2(&self) -> Vec2 {
        Vec2::new(self.max.x, self.max.z)
    }
}

/// Visual-only box (colliders are listed separately in `solids`).
#[derive(Clone, Copy, Debug)]
pub struct VisBox {
    pub min: Vec3,
    pub max: Vec3,
    pub color: Vec3,
    pub side_mat: Material,
    pub top_mat: Material,
}

/// A box turned about the vertical axis (buildings and walls that don't line
/// up with the world grid), optionally tilted about its own local x axis
/// (collapsed slabs that slope to the ground). Visible; blocking unless
/// `solid` is false (small debris).
#[derive(Clone, Copy, Debug)]
pub struct OBox {
    /// Footprint center on the ground plane (x, z).
    pub center: Vec2,
    /// Rotation about +y, radians (local +x turns toward +z... standard glam rotation_y).
    pub yaw: f32,
    /// Footprint half extents along the box's local x and z.
    pub half: Vec2,
    pub y0: f32,
    pub y1: f32,
    pub color: Vec3,
    pub side_mat: Material,
    pub top_mat: Material,
    /// Rotation about the box's local x axis after the yaw, radians: positive
    /// tips its local +z end down. The box turns about its own center.
    pub tilt: f32,
    /// Rotation about the box's own long (local z) axis after the tilt:
    /// several boxes rolled to different angles make a round pipe or hull.
    pub roll: f32,
    /// Collides and blocks navigation. Debris is visual only.
    pub solid: bool,
    /// Visual wear (the collider stays a box): how big the chips knocked out
    /// of its vertical edges and corners are, in meters. 0 = clean box.
    pub chip: f32,
    /// The top is a broken edge, not a finished one: drawn jagged.
    pub broken: bool,
    /// A drivable ramp (set explicitly; a tilted log is not a ramp).
    pub ramp: bool,
}

impl Default for OBox {
    fn default() -> OBox {
        OBox {
            center: Vec2::ZERO,
            yaw: 0.0,
            half: Vec2::ONE,
            y0: 0.0,
            y1: 1.0,
            color: Vec3::ONE,
            side_mat: Material::Plain,
            top_mat: Material::Plain,
            tilt: 0.0,
            roll: 0.0,
            solid: true,
            chip: 0.0,
            broken: false,
            ramp: false,
        }
    }
}

/// Convex hull of points on the ground plane, counter-clockwise (Andrew's
/// monotone chain).
fn convex_hull(mut pts: Vec<Vec2>) -> Vec<Vec2> {
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);
    let mut hull: Vec<Vec2> = Vec::new();
    for pass in 0..2 {
        let start = hull.len();
        for k in 0..pts.len() {
            let p = if pass == 0 {
                pts[k]
            } else {
                pts[pts.len() - 1 - k]
            };
            while hull.len() >= start + 2
                && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// Distance from `p` to a convex counter-clockwise polygon (0 inside).
fn polygon_distance(poly: &[Vec2], p: Vec2) -> f32 {
    match poly.len() {
        0 => return f32::MAX,
        1 => return poly[0].distance(p),
        _ => {}
    }
    let n = poly.len();
    let mut inside = n >= 3;
    let mut best = f32::MAX;
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (b - a).perp_dot(p - a) < 0.0 {
            inside = false;
        }
        let t = ((p - a).dot(b - a) / (b - a).length_squared().max(1e-9)).clamp(0.0, 1.0);
        best = best.min(p.distance(a + (b - a) * t));
    }
    if inside { 0.0 } else { best }
}

impl OBox {
    /// Full rotation: yaw about +y, then tilt about the local x axis.
    pub fn rotation(&self) -> glam::Quat {
        glam::Quat::from_rotation_y(self.yaw)
            * glam::Quat::from_rotation_x(self.tilt)
            * glam::Quat::from_rotation_z(self.roll)
    }

    /// Box center in 3D.
    pub fn center3(&self) -> Vec3 {
        Vec3::new(self.center.x, (self.y0 + self.y1) * 0.5, self.center.y)
    }

    /// A sloped slab tanks drive up (gets ground friction, doesn't block bots).
    pub fn is_ramp(&self) -> bool {
        self.ramp
    }

    /// Local axes on the ground plane: (right, forward), as world xz directions.
    pub fn axes(&self) -> (Vec2, Vec2) {
        // glam's rotation_y maps local +x to (cos, -sin) and +z to (sin, cos) in xz.
        let (s, c) = self.yaw.sin_cos();
        (Vec2::new(c, -s), Vec2::new(s, c))
    }

    /// World point (x, z) in the box's local footprint frame.
    pub fn local_point(&self, p: Vec2) -> Vec2 {
        let (ax, az) = self.axes();
        let d = p - self.center;
        Vec2::new(d.dot(ax), d.dot(az))
    }

    /// Whether the box is turned out of the upright (tilted or rolled).
    pub fn is_turned(&self) -> bool {
        self.tilt != 0.0 || self.roll != 0.0
    }

    /// The eight corners in world space.
    pub fn corners3(&self) -> [Vec3; 8] {
        let (c, r) = (self.center3(), self.rotation());
        let h = Vec3::new(self.half.x, (self.y1 - self.y0) * 0.5, self.half.y);
        let mut out = [Vec3::ZERO; 8];
        for (k, o) in out.iter_mut().enumerate() {
            let s = Vec3::new(
                if k & 1 == 0 { -1.0 } else { 1.0 },
                if k & 2 == 0 { -1.0 } else { 1.0 },
                if k & 4 == 0 { -1.0 } else { 1.0 },
            );
            *o = c + r * (h * s);
        }
        out
    }

    /// Lowest and highest points of the box as it actually stands.
    pub fn y_range(&self) -> (f32, f32) {
        if !self.is_turned() {
            return (self.y0, self.y1);
        }
        self.corners3()
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), p| {
                (lo.min(p.y), hi.max(p.y))
            })
    }

    /// The box's shadow on the ground (straight down): a convex polygon,
    /// counter-clockwise. For an upright box, its footprint.
    pub fn shadow(&self) -> Vec<Vec2> {
        if !self.is_turned() {
            return self.corners().to_vec();
        }
        convex_hull(
            self.corners3()
                .iter()
                .map(|p| Vec2::new(p.x, p.z))
                .collect(),
        )
    }

    /// The shadow of only the part of the box from its lowest point up to
    /// `height` above it: what a tank on the ground actually runs into. A
    /// spire leaning out over open ground blocks around its root, not
    /// everywhere under its overhang.
    pub fn base_shadow(&self, height: f32) -> Vec<Vec2> {
        if !self.is_turned() {
            return self.corners().to_vec();
        }
        let c = self.corners3();
        let lo = c.iter().fold(f32::MAX, |m, p| m.min(p.y));
        let hi = lo + height;
        let mut pts: Vec<Vec2> = c
            .iter()
            .filter(|p| p.y <= hi)
            .map(|p| Vec2::new(p.x, p.z))
            .collect();
        for i in 0..8 {
            for bit in [1, 2, 4] {
                let j = i ^ bit;
                if j < i {
                    continue;
                }
                let (a, b) = (c[i], c[j]);
                if (a.y - hi) * (b.y - hi) < 0.0 {
                    let t = (hi - a.y) / (b.y - a.y);
                    let q = a.lerp(b, t);
                    pts.push(Vec2::new(q.x, q.z));
                }
            }
        }
        convex_hull(pts)
    }

    /// Distance from a ground point to the part of the box within `height`
    /// of its lowest point (0 inside).
    pub fn base_distance(&self, p: Vec2, height: f32) -> f32 {
        if !self.is_turned() {
            return self.distance(p);
        }
        polygon_distance(&self.base_shadow(height), p)
    }

    /// Distance from a ground point to the box's shadow on the ground (0
    /// inside). Follows tilt and roll: a standing post is a small square, not
    /// a beam as long as the post is tall.
    pub fn distance(&self, p: Vec2) -> f32 {
        if !self.is_turned() {
            let q = self.local_point(p).abs() - self.half;
            return q.max(Vec2::ZERO).length();
        }
        polygon_distance(&self.shadow(), p)
    }

    /// Footprint corners in world xz, counter-clockwise.
    pub fn corners(&self) -> [Vec2; 4] {
        let (ax, az) = self.axes();
        let (x, z) = (ax * self.half.x, az * self.half.y);
        [
            self.center - x - z,
            self.center + x - z,
            self.center + x + z,
            self.center - x + z,
        ]
    }

    /// Footprints overlap (separating axis test), with `margin` of required gap.
    pub fn overlaps(&self, o: &OBox, margin: f32) -> bool {
        let (a0, a1) = self.axes();
        let (b0, b1) = o.axes();
        for axis in [a0, a1, b0, b1] {
            let proj = |b: &OBox| {
                let cs = b.corners().map(|c| c.dot(axis));
                (
                    cs.iter().copied().fold(f32::INFINITY, f32::min),
                    cs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                )
            };
            let (lo0, hi0) = proj(self);
            let (lo1, hi1) = proj(o);
            if hi0 + margin < lo1 || hi1 + margin < lo0 {
                return false;
            }
        }
        true
    }
}

/// Which way a wedge's slope climbs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rise {
    PosX,
    NegX,
    PosZ,
    NegZ,
}

/// A ramp: rectangular footprint, flat bottom at `base_y`, sloped top that
/// climbs from 0 to `height` in the `rise` direction. Solid and drivable.
#[derive(Clone, Copy, Debug)]
pub struct Wedge {
    pub min: Vec2,
    pub max: Vec2,
    pub base_y: f32,
    pub height: f32,
    pub rise: Rise,
    pub color: Vec3,
    /// Surface material; None picks by slope (paving on top, plain sides).
    pub material: Option<Material>,
}

impl Wedge {
    /// Corners: [low0, low1, high_base0, high_base1, high_top0, high_top1].
    pub fn points(&self) -> [Vec3; 6] {
        let (a, b, y0, y1) = (self.min, self.max, self.base_y, self.base_y + self.height);
        let p = |x: f32, z: f32, y: f32| Vec3::new(x, y, z);
        match self.rise {
            Rise::PosX => [
                p(a.x, a.y, y0),
                p(a.x, b.y, y0),
                p(b.x, a.y, y0),
                p(b.x, b.y, y0),
                p(b.x, a.y, y1),
                p(b.x, b.y, y1),
            ],
            Rise::NegX => [
                p(b.x, a.y, y0),
                p(b.x, b.y, y0),
                p(a.x, a.y, y0),
                p(a.x, b.y, y0),
                p(a.x, a.y, y1),
                p(a.x, b.y, y1),
            ],
            Rise::PosZ => [
                p(a.x, a.y, y0),
                p(b.x, a.y, y0),
                p(a.x, b.y, y0),
                p(b.x, b.y, y0),
                p(a.x, b.y, y1),
                p(b.x, b.y, y1),
            ],
            Rise::NegZ => [
                p(a.x, b.y, y0),
                p(b.x, b.y, y0),
                p(a.x, a.y, y0),
                p(b.x, a.y, y0),
                p(a.x, a.y, y1),
                p(b.x, a.y, y1),
            ],
        }
    }

    /// Faces as point-index polygons (convex), for rendering.
    pub const FACES: [&'static [usize]; 5] = [
        &[0, 1, 5, 4],
        &[2, 3, 5, 4],
        &[0, 1, 3, 2],
        &[0, 2, 4],
        &[1, 3, 5],
    ];
}

/// Heightfield ground. Heights are stored row-major, z rows then x columns.
#[derive(Clone, Debug)]
pub struct Terrain {
    pub origin: Vec2,
    pub cell: f32,
    pub nx: usize,
    pub nz: usize,
    pub heights: Vec<f32>,
    pub color: Vec3,
    pub material: Material,
}

impl Terrain {
    pub fn from_fn(
        origin: Vec2,
        cell: f32,
        nx: usize,
        nz: usize,
        f: impl Fn(f32, f32) -> f32,
    ) -> Terrain {
        let mut heights = Vec::with_capacity(nx * nz);
        for iz in 0..nz {
            for ix in 0..nx {
                heights.push(f(origin.x + ix as f32 * cell, origin.y + iz as f32 * cell));
            }
        }
        Terrain {
            origin,
            cell,
            nx,
            nz,
            heights,
            color: Vec3::new(0.36, 0.5, 0.28),
            material: Material::Grass,
        }
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new((self.nx - 1) as f32, (self.nz - 1) as f32) * self.cell
    }

    pub fn at(&self, ix: usize, iz: usize) -> f32 {
        self.heights[iz * self.nx + ix]
    }

    /// Bilinear height (close to, not exactly, the collider's triangles).
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let g = (Vec2::new(x, z) - self.origin) / self.cell;
        let gx = g.x.clamp(0.0, (self.nx - 1) as f32 - 0.001);
        let gz = g.y.clamp(0.0, (self.nz - 1) as f32 - 0.001);
        let (ix, iz) = (gx as usize, gz as usize);
        let (fx, fz) = (gx - ix as f32, gz - iz as f32);
        let h0 = self.at(ix, iz) * (1.0 - fx) + self.at(ix + 1, iz) * fx;
        let h1 = self.at(ix, iz + 1) * (1.0 - fx) + self.at(ix + 1, iz + 1) * fx;
        h0 * (1.0 - fz) + h1 * fz
    }
}

#[derive(Clone)]
pub struct Map {
    /// Arena spans [0, size] on x and z.
    pub size: f32,
    pub solids: Vec<Solid>,
    pub boxes: Vec<VisBox>,
    pub wedges: Vec<Wedge>,
    /// Rotated solid boxes (blocking and visible).
    pub oboxes: Vec<OBox>,
    pub terrain: Option<Terrain>,
    /// Visual-only ground beyond the arena (dunes to the horizon). Drawn
    /// instead of the flat apron; never collides.
    pub scenery_terrain: Option<Terrain>,
    /// Open positions usable for spawns and pickups.
    pub open_spots: Vec<Vec2>,
    /// Fixed spawn heading; None means face the map center.
    pub spawn_yaw: Option<f32>,
    /// Sun, sky and haze.
    pub atmosphere: crate::render::Atmosphere,
    /// Each side's spawn spots, one per faction (all empty: spawn anywhere).
    pub bases: Vec<Vec<Vec2>>,
    /// Who fights here: side 0 (the player's), then the others. Two sides
    /// on most maps; free-for-all maps field more.
    pub factions: Vec<Faction>,
    /// Where repair crates parachute in (see `repair`).
    pub repair_spots: Vec<Vec2>,
    /// The Hold contract's site; `None` picks the open spot nearest the middle (see `hold`).
    pub hold_site: Option<Vec2>,
    /// Ithessar cores: plasma held in the air between crystal monoliths.
    /// Drawn (and animated) with the effects.
    pub cores: Vec<Core>,
    /// Faceted crystals (Ithessar ground).
    pub crystals: Vec<Crystal>,
    /// Glowing ribbons laid on the ground (a river of plasma). Visual only.
    pub glow_paths: Vec<GlowPath>,
    /// Maps up high (a mesa): a tank that drops below this height has gone
    /// over the edge and dies on impact.
    pub fall_y: Option<f32>,
    /// What the Hold site is called on the HUD.
    pub hold_name: &'static str,
}

/// A glowing ribbon on the ground: a bright core fading out to its edges.
/// Drawn as one continuous strip, so the glow's pulse runs along it.
#[derive(Clone, Debug)]
pub struct GlowPath {
    /// Center line, already at ground height.
    pub points: Vec<Vec3>,
    /// Half-width at each point.
    pub widths: Vec<f32>,
    pub color: Vec3,
}

impl GlowPath {
    /// Append its mesh: a hot core band and dimmer bands to each side.
    pub fn mesh(&self, m: &mut crate::render::mesh::MeshData) {
        let n = self.points.len();
        if n < 2 {
            return;
        }
        // (offset across as a fraction of the half-width, brightness)
        let bands = [(-1.0f32, 0.45f32), (-0.35, 1.0), (0.35, 1.0), (1.0, 0.45)];
        let side = |i: usize| {
            let a = self.points[i.saturating_sub(1)];
            let b = self.points[(i + 1).min(n - 1)];
            let d = Vec2::new(b.x - a.x, b.z - a.z).normalize_or_zero();
            Vec3::new(-d.y, 0.0, d.x)
        };
        for i in 0..n - 1 {
            let (s0, s1) = (side(i), side(i + 1));
            for w in bands.windows(2) {
                let (f0, c0) = w[0];
                let (f1, c1) = w[1];
                let col = self.color * (c0 + c1) * 0.5;
                let c = col.extend(Material::Glow.id());
                let p = |j: usize, s: Vec3, f: f32| self.points[j] + s * self.widths[j] * f;
                m.quad(
                    [
                        p(i, s0, f0),
                        p(i + 1, s1, f0),
                        p(i + 1, s1, f1),
                        p(i, s0, f1),
                    ],
                    Vec3::Y,
                    c,
                );
            }
        }
    }
}

/// A worked crystal: an irregular faceted prism from a base sunk in the
/// ground, along its axis, narrowing to a point. Blocks tanks and shots
/// (which glance off its faces) unless `solid` is false.
#[derive(Clone, Copy, Debug)]
pub struct Crystal {
    pub base: Vec3,
    /// Base to tip.
    pub axis: Vec3,
    pub radius: f32,
    pub sides: u32,
    /// Varies the facets.
    pub seed: u32,
    pub color: Vec3,
    /// A seam of plasma up one face.
    pub lit: bool,
    pub solid: bool,
}

impl Crystal {
    /// Cross-section rings from base to tip, `sides` points each. Every
    /// vertex keeps its own radial and angular jitter all the way up, so the
    /// facets run the full length like knapped glass.
    pub fn rings(&self) -> Vec<Vec<Vec3>> {
        let len = self.axis.length().max(0.01);
        let d = self.axis / len;
        let side = if d.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
        let a = d.cross(side).normalize();
        let b = d.cross(a);
        let n = self.sides.max(3) as usize;
        let hash = |k: u32| {
            let h =
                (self.seed.wrapping_mul(2_654_435_761) ^ k.wrapping_mul(40_503)).wrapping_mul(97);
            (h % 1000) as f32 / 1000.0
        };
        let jit: Vec<(f32, f32)> = (0..n as u32)
            .map(|k| (0.82 + 0.3 * hash(k), (hash(k + 100) - 0.5) * 0.5))
            .collect();
        let tip_lean = (hash(200) - 0.5) * 0.3;
        [
            (0.0f32, 1.0f32),
            (0.5, 0.96),
            (0.76, 0.74),
            (0.9, 0.36),
            (1.0, 0.04),
        ]
        .iter()
        .map(|&(t, r)| {
            let c = self.base + self.axis * t + a * tip_lean * self.radius * t * t;
            (0..n)
                .map(|k| {
                    let (rj, aj) = jit[k];
                    let ang = (k as f32 + aj) / n as f32 * std::f32::consts::TAU + t * 0.15;
                    c + (a * ang.cos() + b * ang.sin()) * self.radius * r * rj
                })
                .collect()
        })
        .collect()
    }

    /// Every vertex (for the collider).
    pub fn points(&self) -> Vec<Vec3> {
        self.rings().into_iter().flatten().collect()
    }

    /// Ground-plane circles covering the part of it a tank could hit (up to
    /// `reach` above `ground`): (center, radius).
    pub fn footprint(&self, ground: f32, reach: f32) -> Vec<(Vec2, f32)> {
        (0..=4)
            .filter_map(|i| {
                let t = i as f32 / 4.0 * 0.6;
                let p = self.base + self.axis * t;
                (p.y < ground + reach).then(|| (Vec2::new(p.x, p.z), self.radius * (1.0 - t * 0.3)))
            })
            .collect()
    }

    /// Distance from a ground point to its footprint (0 inside).
    pub fn distance(&self, p: Vec2) -> f32 {
        self.footprint(self.base.y.max(-100.0) + 1.5, 4.0)
            .iter()
            .map(|(c, r)| (c.distance(p) - r).max(0.0))
            .fold(f32::INFINITY, f32::min)
    }

    /// Append its mesh: dark worked crystal, and a seam of plasma up one
    /// face if it's lit.
    pub fn mesh(&self, m: &mut crate::render::mesh::MeshData) {
        use crate::tank::model::{Skin, round};
        let c = self.color;
        let skin = Skin::new(
            c.extend(Material::Vhaal.id()),
            (c * 1.3).extend(Material::Vhaal.id()),
        );
        let rings = self.rings();
        // Every facet the same glass (no lighter chamfer faces).
        round(m, &rings, 0.0, skin);
        if self.lit {
            // Up the middle of face 0, from near the ground to near the tip,
            // standing just proud of it.
            let glow = crate::tank::ithessar::PLASMA.extend(Material::Glow.id());
            let (lo, hi) = (0usize, 3usize);
            let mid = |r: &Vec<Vec3>, f: f32| r[0].lerp(r[1], f);
            for w in rings[lo..=hi].windows(2) {
                let (p0, p1, p2, p3) = (
                    mid(&w[0], 0.42),
                    mid(&w[0], 0.58),
                    mid(&w[1], 0.58),
                    mid(&w[1], 0.42),
                );
                let mut nrm = (p1 - p0).cross(p3 - p0).normalize_or_zero();
                let center = (w[0].iter().copied().sum::<Vec3>()) / w[0].len() as f32;
                if nrm.dot(p0 - center) < 0.0 {
                    nrm = -nrm;
                }
                let lift = nrm * 0.03;
                m.quad([p0 + lift, p1 + lift, p2 + lift, p3 + lift], nrm, glow);
            }
        }
    }
}

/// A ball of plasma held in the air between crystal monoliths.
#[derive(Clone, Debug)]
pub struct Core {
    pub at: Vec3,
    pub radius: f32,
    /// The monoliths' tips that hold it (arcs jump from them to the core).
    pub prongs: Vec<Vec3>,
}

/// The matchup on maps that don't name their own.
pub const DEFAULT_SIDES: [Faction; 2] = [Faction::Imperium, Faction::Resonant];

impl Map {
    /// Put the player on side `side`: it trades places with side 0, bases
    /// and all.
    pub fn swap_sides(&mut self, side: usize) {
        self.factions.swap(0, side);
        self.bases.swap(0, side);
    }

    /// No bases: one empty spawn list per side (spawn anywhere).
    pub fn no_bases(factions: &[Faction]) -> Vec<Vec<Vec2>> {
        vec![Vec::new(); factions.len()]
    }

    /// Visible arena walls (the test kit's yard).
    #[cfg(test)]
    fn perimeter_walls(&mut self) {
        self.perimeter(true);
    }

    /// Invisible arena walls spanning heights `y0`..`y1` (a map whose floor
    /// isn't at 0).
    fn perimeter_at(&mut self, y0: f32, y1: f32) {
        let s = self.size;
        let t = 2.0;
        for (min, max) in [
            (Vec3::new(-t, y0, -t), Vec3::new(s + t, y1, 0.0)),
            (Vec3::new(-t, y0, s), Vec3::new(s + t, y1, s + t)),
            (Vec3::new(-t, y0, 0.0), Vec3::new(0.0, y1, s)),
            (Vec3::new(s, y0, 0.0), Vec3::new(s + t, y1, s)),
        ] {
            self.solids.push(Solid { min, max });
        }
    }

    /// The arena boundary: solid walls, drawn as grey walls or (for maps
    /// whose edge is scenery, like dunes) invisible.
    fn perimeter(&mut self, visible: bool) {
        let s = self.size;
        let (t, h) = (2.0, 5.0);
        let walls = [
            (Vec3::new(-t, 0.0, -t), Vec3::new(s + t, h, 0.0)),
            (Vec3::new(-t, 0.0, s), Vec3::new(s + t, h, s + t)),
            (Vec3::new(-t, 0.0, 0.0), Vec3::new(0.0, h, s)),
            (Vec3::new(s, 0.0, 0.0), Vec3::new(s + t, h, s)),
        ];
        for (min, max) in walls {
            self.solids.push(Solid { min, max });
            if !visible {
                continue;
            }
            self.boxes.push(VisBox {
                min,
                max,
                color: Vec3::new(0.40, 0.40, 0.42),
                side_mat: Material::Plain,
                top_mat: Material::Plain,
            });
            self.solids.push(Solid { min, max });
        }
    }

    pub fn center(&self) -> Vec2 {
        Vec2::splat(self.size * 0.5)
    }

    /// Flat ground and nothing else (the showroom yard).
    pub fn empty(size: f32) -> Map {
        Map {
            size,
            solids: Vec::new(),
            boxes: Vec::new(),
            wedges: Vec::new(),
            oboxes: Vec::new(),
            terrain: None,
            scenery_terrain: None,
            open_spots: vec![Vec2::ZERO],
            spawn_yaw: Some(0.0),
            atmosphere: crate::render::Atmosphere::default(),
            bases: Map::no_bases(&DEFAULT_SIDES),
            factions: DEFAULT_SIDES.to_vec(),
            repair_spots: Vec::new(),
            hold_site: None,
            cores: Vec::new(),
            crystals: Vec::new(),
            glow_paths: Vec::new(),
            fall_y: None,
            hold_name: "DEPOSIT",
        }
    }

    /// Ground height at a point (terrain if present, else flat 0).
    pub fn ground_height(&self, x: f32, z: f32) -> f32 {
        self.terrain.as_ref().map_or(0.0, |t| t.height_at(x, z))
    }

    #[cfg(test)]
    fn add_block(&mut self, min: Vec3, max: Vec3, color: Vec3) {
        self.boxes.push(VisBox {
            min,
            max,
            color,
            side_mat: Material::Plain,
            top_mat: Material::Paving,
        });
        self.solids.push(Solid { min, max });
    }
}

#[cfg(test)]
mod footprint_tests {
    use super::*;

    /// A post stood upright (a segment tipped up) blocks a small square, not
    /// a strip as long as the post is tall.
    #[test]
    fn standing_post_has_a_small_footprint() {
        let post = OBox {
            center: Vec2::new(10.0, 10.0),
            tilt: -std::f32::consts::FRAC_PI_2,
            half: Vec2::new(0.3, 4.0),
            y0: 3.7,
            y1: 4.3,
            ..OBox::default()
        };
        assert!(post.distance(Vec2::new(10.0, 13.0)) > 2.0);
        assert!(post.distance(Vec2::new(10.0, 10.1)) == 0.0);
        let (lo, hi) = post.y_range();
        assert!(lo < 0.1 && hi > 7.9, "stands 0..8 m, got {lo}..{hi}");
    }

    /// A spire leaning far out blocks around its root; tanks can drive
    /// under the overhang.
    #[test]
    fn leaning_spire_blocks_only_near_its_root() {
        let spire = OBox {
            center: Vec2::ZERO,
            tilt: -0.9,
            half: Vec2::new(0.5, 6.0),
            y0: 4.5,
            y1: 5.5,
            ..OBox::default()
        };
        let shadow_far = spire.distance(Vec2::new(0.0, 3.5));
        let base_far = spire.base_distance(Vec2::new(0.0, 3.5), 3.0);
        assert_eq!(shadow_far, 0.0, "under the spire's shadow");
        assert!(
            base_far > 0.5,
            "but clear of what a tank would hit ({base_far})"
        );
    }
}
