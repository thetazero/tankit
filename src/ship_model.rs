//! The Company ship you're aboard, as the system view (#346, #351) shows
//! it: a long armoured carrier, built at full size so the hull materials'
//! panel seams, grain and scuffs come out at the scale of a ship.
//!
//! Bow along +x, up +y, starboard +z, centered on its middle. The hull is
//! smooth-shaded: rounded-box sections lofted down its length, a spine and a
//! bridge tower on top, an engine pod on a swept pylon each side, and a
//! cluster of drive bells at the stern. Over that, the detail that gives it
//! its size: plating, vents and housings scattered over the decks, gun
//! turrets, hangar doors with their lights on, rows of lit ports down the
//! flanks, running lights, and a mast with a dish.

use std::f32::consts::TAU;

use glam::{Mat3, Mat4, Quat, Vec3, Vec4};

use crate::faction::Faction;
use crate::render::material::Material;
use crate::render::mesh::{MeshData, Vertex};

/// Its length, prow to the drive bells' mouths, in the system view's units.
pub const LEN: f32 = 32.0;

/// Where the hull starts, bow and stern.
const BOW: f32 = 16.0;
const STERN: f32 = -12.5;

/// Its drives: each bell's mouth, and its radius.
pub const DRIVES: [(Vec3, f32); 7] = [
    (Vec3::new(-15.6, 0.35, 0.0), 1.0),
    (Vec3::new(-15.4, 0.05, -1.5), 0.78),
    (Vec3::new(-15.4, 0.05, 1.5), 0.78),
    (Vec3::new(-15.2, -0.95, -0.7), 0.42),
    (Vec3::new(-15.2, -0.95, 0.7), 0.42),
    (Vec3::new(-13.4, -0.35, -POD_Z), 0.6),
    (Vec3::new(-13.4, -0.35, POD_Z), 0.6),
];

/// How far out the engine pods ride.
const POD_Z: f32 = 4.3;

pub(crate) fn col(c: [f32; 3], m: Material) -> Vec4 {
    Vec4::new(c[0], c[1], c[2], m.id())
}

/// Its paints.
struct Paints {
    hull: Vec4,
    sheen: Vec4,
    plate: Vec4,
    under: Vec4,
    /// Fittings: a darker grey.
    metal: Vec4,
    /// The drive bells' bare metal.
    bell: Vec4,
    amber: Vec4,
    window: Vec4,
    drive: Vec4,
    red: Vec4,
    green: Vec4,
}

impl Paints {
    fn new() -> Paints {
        let a = Faction::Company.ui();
        Paints {
            hull: col([0.55, 0.56, 0.58], Material::Paint),
            sheen: col([0.49, 0.5, 0.53], Material::Paint),
            plate: col([0.42, 0.43, 0.46], Material::Paint),
            under: col([0.22, 0.22, 0.24], Material::Carbon),
            metal: col([0.36, 0.37, 0.4], Material::Paint),
            bell: col([0.5, 0.51, 0.54], Material::Metal),
            amber: col([a[0], a[1], a[2]], Material::Paint),
            window: col([1.0, 0.84, 0.58], Material::Glow),
            drive: col([1.0, 0.78, 0.45], Material::Glow),
            red: col([1.0, 0.15, 0.1], Material::Glow),
            green: col([0.2, 1.0, 0.35], Material::Glow),
        }
    }
}

/// A hull section: half width and half height, how high its middle sits,
/// and how boxy it is (2 an ellipse, higher squarer).
#[derive(Clone, Copy)]
pub(crate) struct Section {
    pub w: f32,
    pub h: f32,
    pub y: f32,
    pub n: f32,
}

impl Section {
    /// The point `t` of the way round (0 the starboard side, a quarter the
    /// top).
    pub fn at(self, x: f32, t: f32) -> Vec3 {
        let (s, c) = (TAU * t).sin_cos();
        let e = 2.0 / self.n;
        Vec3::new(
            x,
            self.y + self.h * s.signum() * s.abs().powf(e),
            self.w * c.signum() * c.abs().powf(e),
        )
    }
}

/// A smooth-shaded skin through `stations` (x, its section), `around`
/// quads round, each coloured by `paint(middle, t round, station)`; both
/// ends capped.
pub(crate) fn loft(
    m: &mut MeshData,
    stations: &[(f32, Section)],
    around: usize,
    paint: impl Fn(Vec3, f32, usize) -> Vec4,
) {
    let n = stations.len();
    let grid: Vec<Vec<Vec3>> = stations
        .iter()
        .map(|&(x, s)| {
            (0..around)
                .map(|k| s.at(x, k as f32 / around as f32))
                .collect()
        })
        .collect();
    let axis = |i: usize| Vec3::new(stations[i].0, stations[i].1.y, 0.0);
    let normal = |i: usize, k: usize| {
        let p = grid[i][k];
        let along = grid[(i + 1).min(n - 1)][k] - grid[i.saturating_sub(1)][k];
        let round = grid[i][(k + 1) % around] - grid[i][(k + around - 1) % around];
        let v = round.cross(along).normalize_or_zero();
        if v.dot(p - axis(i)) < 0.0 { -v } else { v }
    };
    let normals: Vec<Vec<Vec3>> = (0..n)
        .map(|i| (0..around).map(|k| normal(i, k)).collect())
        .collect();
    for i in 0..n - 1 {
        for k in 0..around {
            let g = (k + 1) % around;
            let corners = [(i, k), (i, g), (i + 1, g), (i + 1, k)];
            let mid = corners.iter().map(|&(a, b)| grid[a][b]).sum::<Vec3>() * 0.25;
            let c = paint(mid, (k as f32 + 0.5) / around as f32, i);
            let base = m.verts.len() as u32;
            for (a, b) in corners {
                m.verts.push(Vertex {
                    pos: grid[a][b].to_array(),
                    normal: normals[a][b].to_array(),
                    color: c.to_array(),
                });
            }
            m.indices
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    for (i, inner) in [(0, 1), (n - 1, n - 2)] {
        let c = paint(axis(i), 0.75, usize::MAX);
        let centre = grid[i].iter().copied().sum::<Vec3>() / around as f32;
        let inside = centre + (axis(inner) - axis(i)) * 0.5;
        for k in 0..around {
            m.facet([centre, grid[i][k], grid[i][(k + 1) % around]], inside, c);
        }
    }
}

/// A body of revolution round the x axis through `c`: rings (x offset,
/// radius) from front to back, `segs` round, smooth, one colour.
pub(crate) fn lathe(m: &mut MeshData, c: Vec3, rings: &[(f32, f32)], segs: usize, color: Vec4) {
    let n = rings.len();
    let at = |i: usize, k: usize| {
        let a = TAU * k as f32 / segs as f32;
        let (dx, r) = rings[i];
        c + Vec3::new(dx, a.sin() * r, a.cos() * r)
    };
    let normal = |i: usize, k: usize| {
        let a = TAU * k as f32 / segs as f32;
        let radial = Vec3::new(0.0, a.sin(), a.cos());
        let (x0, r0) = rings[i.saturating_sub(1)];
        let (x1, r1) = rings[(i + 1).min(n - 1)];
        // The profile's slope tips the normal along the axis.
        let (dx, dr) = (x1 - x0, r1 - r0);
        (radial * dx.abs() - Vec3::X * dr * dx.signum()).normalize_or_zero()
    };
    for i in 0..n - 1 {
        for k in 0..segs {
            let g = (k + 1) % segs;
            let base = m.verts.len() as u32;
            for (a, b) in [(i, k), (i, g), (i + 1, g), (i + 1, k)] {
                m.verts.push(Vertex {
                    pos: at(a, b).to_array(),
                    normal: normal(a, b).to_array(),
                    color: color.to_array(),
                });
            }
            m.indices
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
}

/// A flat disc facing `dir` (along x, ±1) at `c`, radius `r`.
pub(crate) fn disc(m: &mut MeshData, c: Vec3, r: f32, dir: f32, segs: usize, color: Vec4) {
    let inside = c - Vec3::X * dir;
    for k in 0..segs {
        let a0 = TAU * k as f32 / segs as f32;
        let a1 = TAU * (k + 1) as f32 / segs as f32;
        let p = |a: f32| c + Vec3::new(0.0, a.sin() * r, a.cos() * r);
        m.facet([c, p(a0), p(a1)], inside, color);
    }
}

/// A box `size` big at `c`, turned by `rot`.
pub(crate) fn block(m: &mut MeshData, c: Vec3, size: Vec3, rot: Quat, color: Vec4) {
    m.append(
        &MeshData::cube(),
        Mat4::from_scale_rotation_translation(size, rot, c),
        color,
    );
}

/// A tiny deterministic random source, so the ship is the same every time.
pub(crate) struct Rng(pub u32);

impl Rng {
    pub fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / 16_777_216.0
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f()
    }
}

/// `x` eased from 0 at `a` to 1 at `b`.
pub(crate) fn ease(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The main hull's section at `x`.
fn hull_at(x: f32) -> Section {
    // The prow: from x 4 forward the hull narrows to a blade, its deck
    // sloping down to it faster than its keel rises.
    let t = ((x - 4.0) / (BOW - 4.0)).clamp(0.0, 1.0);
    let w = 2.45 * (1.0 - t.powf(1.7)).max(0.0).powf(0.55);
    let h = 1.35 * (1.0 - t.powf(1.35)).max(0.0).powf(0.6);
    // The stern swells into the drive housing.
    let aft = ease(-8.5, -10.5, x);
    Section {
        w: (w + 0.4 * aft).max(0.04),
        h: (h + 0.35 * aft).max(0.04),
        y: -0.35 * t + 0.1 * aft,
        n: 2.0 + 1.0 * (1.0 - t) + 0.6 * aft,
    }
}

/// Where the main hull's surface is at `x`, `t` round, and which way it
/// faces there.
fn on_hull(x: f32, t: f32) -> (Vec3, Vec3) {
    let s = hull_at(x);
    let p = s.at(x, t);
    let dt = 0.002;
    let round = s.at(x, t + dt) - s.at(x, t - dt);
    let along = hull_at(x + 0.05).at(x + 0.05, t) - hull_at(x - 0.05).at(x - 0.05, t);
    let mut n = round.cross(along).normalize_or_zero();
    if n.dot(p - Vec3::new(x, s.y, 0.0)) < 0.0 {
        n = -n;
    }
    (p, n)
}

/// A frame standing on the surface at normal `n`, its long side along x.
pub(crate) fn standing(n: Vec3) -> Quat {
    let fwd = (Vec3::X - n * n.dot(Vec3::X)).normalize_or(Vec3::X);
    let side = fwd.cross(n);
    Quat::from_mat3(&Mat3::from_cols(fwd, n, side))
}

/// The ship, built at full size.
pub fn mesh() -> MeshData {
    let mut m = MeshData::default();
    let p = Paints::new();

    main_hull(&mut m, &p);
    spine(&mut m, &p);
    bridge(&mut m, &p);
    pods(&mut m, &p);
    drives(&mut m, &p);
    hangars(&mut m, &p);
    ports(&mut m, &p);
    greebles(&mut m, &p);
    turrets(&mut m, &p);
    m
}

fn main_hull(m: &mut MeshData, p: &Paints) {
    let count = 72;
    let stations: Vec<(f32, Section)> = (0..=count)
        .map(|i| {
            // Closer together toward the prow, where it curves most.
            let u = i as f32 / count as f32;
            let x = BOW - (BOW - STERN) * u.powf(1.25);
            (x, hull_at(x))
        })
        .collect();
    loft(m, &stations, 48, |c, t, i| {
        let up = (TAU * t).sin();
        let side = (TAU * t).cos().abs();
        if i == usize::MAX {
            return p.metal;
        }
        if up < -0.45 {
            return p.under;
        }
        // The Company's band behind the prow, and a stripe down each
        // shoulder to the drive housing.
        if (7.0..8.6).contains(&c.x) && up > -0.3 {
            return p.amber;
        }
        if (-10.6..7.0).contains(&c.x) && (0.42..0.56).contains(&up) && side > 0.3 {
            return p.amber;
        }
        if c.x < -10.6 && up > 0.2 && side > 0.3 {
            return p.amber;
        }
        if up > 0.55 {
            // Deck plating in bands.
            if (c.x * 0.5).floor() as i32 % 2 == 0 {
                p.hull
            } else {
                p.sheen
            }
        } else {
            p.plate
        }
    });
}

/// The spine down the deck, and the bridge's base.
fn spine(m: &mut MeshData, p: &Paints) {
    let stations: Vec<(f32, Section)> = (0..=40)
        .map(|i| {
            let x = 5.5 - 17.0 * i as f32 / 40.0;
            let front = ease(5.5, 1.5, x);
            let back = ease(-11.5, -10.0, x);
            let k = front * back;
            let deck = hull_at(x);
            (
                x,
                Section {
                    w: 0.15 + 1.0 * k,
                    h: 0.1 + 0.55 * k,
                    y: deck.y + deck.h * 0.92 + 0.3 * k - 0.1,
                    n: 3.5,
                },
            )
        })
        .collect();
    loft(m, &stations, 32, |c, t, _| {
        let up = (TAU * t).sin();
        if up > 0.6 {
            if (c.x * 0.7).floor() as i32 % 2 == 0 {
                p.hull
            } else {
                p.sheen
            }
        } else if up > 0.25 && c.x > -10.0 && c.x < 4.0 {
            p.metal
        } else {
            p.plate
        }
    });
}

/// The bridge tower aft of midships, raked back, its windows lit.
fn bridge(m: &mut MeshData, p: &Paints) {
    let base = hull_at(-6.0);
    let foot = base.y + base.h + 0.5;
    let stations: Vec<(f32, Section)> = (0..=24)
        .map(|i| {
            let x = -3.6 - 5.4 * i as f32 / 24.0;
            // Raked front, square back.
            let rise = ease(-3.6, -5.0, x) * ease(-9.0, -8.5, x);
            (
                x,
                Section {
                    w: 0.55 + 0.25 * rise,
                    h: 0.2 + 0.85 * rise,
                    y: foot + 0.85 * rise,
                    n: 5.0,
                },
            )
        })
        .collect();
    let top = foot + 1.7;
    loft(m, &stations, 32, |c, t, _| {
        let up = (TAU * t).sin();
        // A band of windows round its brow.
        if (top - 0.5..top - 0.22).contains(&c.y) && up < 0.9 {
            p.window
        } else if up > 0.8 {
            p.hull
        } else {
            p.sheen
        }
    });
    // Its mast, a dish on it, and a light at the top.
    let mast = Vec3::new(-7.2, top, 0.0);
    m.append(
        &MeshData::cylinder(8),
        Mat4::from_scale_rotation_translation(
            Vec3::new(0.08, 1.8, 0.08),
            Quat::IDENTITY,
            mast + Vec3::Y * 0.9,
        ),
        p.metal,
    );
    block(
        m,
        mast + Vec3::Y * 1.2,
        Vec3::new(0.05, 0.05, 0.9),
        Quat::IDENTITY,
        p.metal,
    );
    block(
        m,
        mast + Vec3::Y * 0.7,
        Vec3::new(0.05, 0.05, 0.6),
        Quat::IDENTITY,
        p.metal,
    );
    m.append(
        &MeshData::sphere(8, 6),
        Mat4::from_scale_rotation_translation(
            Vec3::splat(0.16),
            Quat::IDENTITY,
            mast + Vec3::Y * 1.85,
        ),
        p.red,
    );
    let dish = Vec3::new(-8.3, top + 0.35, 0.0);
    m.append(
        &MeshData::cylinder(6),
        Mat4::from_scale_rotation_translation(
            Vec3::new(0.12, 0.5, 0.12),
            Quat::IDENTITY,
            dish - Vec3::Y * 0.2,
        ),
        p.metal,
    );
    m.append(
        &MeshData::sphere(16, 8),
        Mat4::from_scale_rotation_translation(
            Vec3::new(1.0, 0.18, 1.0),
            Quat::from_rotation_z(0.5) * Quat::from_rotation_x(0.3),
            dish + Vec3::Y * 0.1,
        ),
        p.hull,
    );
}

/// An engine pod each side on a swept pylon.
fn pods(m: &mut MeshData, p: &Paints) {
    for z in [-POD_Z, POD_Z] {
        let stations: Vec<(f32, Section)> = (0..=36)
            .map(|i| {
                let x = -1.0 - 12.0 * i as f32 / 36.0;
                let nose = ((x + 1.0) / -2.2).clamp(0.0, 1.0);
                let r = 0.95 * (1.0 - (1.0 - nose).powi(2)).sqrt().max(0.05);
                let tail = ease(-11.5, -12.6, x);
                (
                    x,
                    Section {
                        w: r * (1.0 - 0.25 * tail),
                        h: r * 0.85 * (1.0 - 0.25 * tail),
                        y: -0.35,
                        n: 2.4,
                    },
                )
            })
            .collect();
        let mut pod = MeshData::default();
        loft(&mut pod, &stations, 32, |c, t, i| {
            let up = (TAU * t).sin();
            if i == usize::MAX {
                p.metal
            } else if (-2.6..-2.0).contains(&c.x) {
                p.amber
            } else if up < -0.4 {
                p.under
            } else if (-7.5..-6.6).contains(&c.x) {
                p.metal
            } else if up > 0.5 {
                p.hull
            } else {
                p.plate
            }
        });
        for v in &mut pod.verts {
            v.pos[2] += z;
        }
        let base = m.verts.len() as u32;
        m.verts.extend(pod.verts);
        m.indices.extend(pod.indices.iter().map(|i| i + base));

        // The pylon: a thick swept blade from the hull's flank to the pod.
        let s = z.signum();
        let root = hull_at(-6.0).w - 0.3;
        let (rf, rb) = (
            Vec3::new(-3.4, -0.2, root * s),
            Vec3::new(-9.6, -0.2, root * s),
        );
        let (tf, tb) = (Vec3::new(-5.2, -0.3, z), Vec3::new(-9.8, -0.3, z));
        let quad = [rf, tf, tb, rb];
        let thick = Vec3::Y * 0.16;
        let inside = quad.iter().copied().sum::<Vec3>() * 0.25;
        let up: Vec<Vec3> = quad.iter().map(|&v| v + thick).collect();
        let dn: Vec<Vec3> = quad.iter().map(|&v| v - thick).collect();
        m.facet([up[0], up[1], up[2]], inside, p.sheen);
        m.facet([up[0], up[2], up[3]], inside, p.sheen);
        m.facet([dn[0], dn[2], dn[1]], inside, p.under);
        m.facet([dn[0], dn[3], dn[2]], inside, p.under);
        for e in 0..4 {
            let f = (e + 1) % 4;
            let c = if e == 0 { p.amber } else { p.metal };
            m.facet([up[e], dn[e], dn[f]], inside, c);
            m.facet([up[e], dn[f], up[f]], inside, c);
        }
        // Radiator vanes along the pylon's top.
        for i in 0..5 {
            let k = (i as f32 + 0.5) / 5.0;
            let c = rf.lerp(tf, k).lerp(rb.lerp(tb, k), 0.45) + thick + Vec3::Y * 0.12;
            block(m, c, Vec3::new(3.0, 0.22, 0.12), Quat::IDENTITY, p.metal);
        }
        // Running lights at the pod's nose: red to port, green to starboard.
        let light = if s > 0.0 { p.green } else { p.red };
        m.append(
            &MeshData::sphere(8, 6),
            Mat4::from_scale_rotation_translation(
                Vec3::splat(0.22),
                Quat::IDENTITY,
                Vec3::new(-2.0, 0.35, z + 0.75 * s),
            ),
            light,
        );
    }
}

/// The drive bells at the stern and on the pods, aglow inside.
fn drives(m: &mut MeshData, p: &Paints) {
    for &(mouth, r) in &DRIVES {
        let back = mouth.x;
        let c = Vec3::new(back, mouth.y, mouth.z);
        let len = r * 2.2;
        // A collar, then the bell flaring out to its mouth.
        let rings: Vec<(f32, f32)> = (0..=12)
            .map(|i| {
                let u = i as f32 / 12.0;
                (len * (1.0 - u), r * (0.72 + 0.28 * u.powf(1.8)))
            })
            .collect();
        lathe(m, c, &rings, 24, p.bell);
        lathe(
            m,
            c,
            &[(len * 0.85, r * 0.85), (len * 0.7, r * 0.85)],
            24,
            p.amber,
        );
        // Inside: the bell's throat, white-hot.
        lathe(
            m,
            c,
            &[(0.05, r * 0.95), (len * 0.5, r * 0.55)],
            24,
            p.drive,
        );
        disc(m, c + Vec3::X * len * 0.5, r * 0.56, -1.0, 24, p.drive);
    }
}

/// Hangar doors in both flanks forward of the bridge, lit along their tops.
fn hangars(m: &mut MeshData, p: &Paints) {
    for s in [-1.0f32, 1.0] {
        for x in [3.3, 0.6] {
            let (at, _) = on_hull(x, if s > 0.0 { 0.0 } else { 0.5 });
            let face = Vec3::new(0.0, 0.0, s);
            let c = at + face * 0.02 - Vec3::Y * 0.1;
            block(m, c, Vec3::new(2.3, 1.0, 0.12), Quat::IDENTITY, p.metal);
            block(
                m,
                c + face * 0.04,
                Vec3::new(2.1, 0.86, 0.12),
                Quat::IDENTITY,
                p.under,
            );
            block(
                m,
                c + face * 0.08 + Vec3::Y * 0.36,
                Vec3::new(2.0, 0.08, 0.1),
                Quat::IDENTITY,
                p.window,
            );
            // Its number plate, in amber.
            block(
                m,
                c + face * 0.08 + Vec3::new(0.85, -0.2, 0.0),
                Vec3::new(0.25, 0.35, 0.1),
                Quat::IDENTITY,
                p.amber,
            );
        }
    }
}

/// Rows of lit ports down the flanks, a few dark.
fn ports(m: &mut MeshData, p: &Paints) {
    let mut rng = Rng(0x5eed_1234);
    for s in [-1.0f32, 1.0] {
        for t in [-0.005f32, -0.04] {
            let mut x = -9.5;
            while x < 6.0 {
                if !(-0.2..4.6).contains(&x) && rng.f() > 0.18 {
                    let tt = if s > 0.0 { t } else { 0.5 - t };
                    let (at, n) = on_hull(x, tt);
                    block(
                        m,
                        at + n * 0.02,
                        Vec3::new(0.22, 0.12, 0.12),
                        standing(n),
                        p.window,
                    );
                }
                x += 0.42;
            }
        }
    }
}

/// Plating, vents and housings scattered over the decks and flanks.
fn greebles(m: &mut MeshData, p: &Paints) {
    let mut rng = Rng(0x0c0f_fee5);
    let picks = [p.hull, p.sheen, p.sheen, p.plate];
    for _ in 0..240 {
        let x = rng.range(-11.5, 7.5);
        // On the deck and shoulders, either side of the spine.
        let t = if rng.f() < 0.5 {
            rng.range(0.07, 0.2)
        } else {
            rng.range(0.3, 0.43)
        };
        let (at, n) = on_hull(x, t);
        let size = Vec3::new(
            rng.range(0.3, 1.6),
            rng.range(0.05, 0.16),
            rng.range(0.2, 0.6),
        );
        let c = picks[(rng.f() * picks.len() as f32) as usize % picks.len()];
        block(m, at + n * (size.y * 0.5 - 0.03), size, standing(n), c);
        // Now and then a vent stack on it.
        if rng.f() < 0.12 {
            let top = at + n * size.y;
            m.append(
                &MeshData::cylinder(8),
                Mat4::from_scale_rotation_translation(
                    Vec3::new(0.16, 0.35, 0.16),
                    Quat::from_rotation_arc(Vec3::Y, n),
                    top + n * 0.15,
                ),
                p.metal,
            );
        }
    }
    // An armour belt down each flank, in long plates, over the ports.
    for t in [0.0f32, 0.5] {
        let mut x = -10.2;
        while x < 6.8 {
            let len = rng.range(1.6, 3.0).min(6.8 - x);
            if len > 0.4 && !(-0.6..4.9).contains(&(x + len * 0.5)) {
                let (at, n) = on_hull(x + len * 0.5, t + 0.045);
                block(
                    m,
                    at + n * 0.05,
                    Vec3::new(len - 0.1, 0.14, 0.42),
                    standing(n),
                    p.sheen,
                );
            }
            x += len;
        }
    }
    // Long armour strakes down the shoulders.
    for s in [0.12f32, 0.38] {
        let mut x = -10.0;
        while x < 6.5 {
            let len = rng.range(1.2, 2.6);
            let (at, n) = on_hull(x + len * 0.5, s);
            block(
                m,
                at + n * 0.04,
                Vec3::new(len - 0.12, 0.12, 0.5),
                standing(n),
                p.sheen,
            );
            x += len;
        }
    }
}

/// Gun turrets on the deck: a ring, a dome, twin barrels toward the bow.
fn turrets(m: &mut MeshData, p: &Paints) {
    for &(x, t) in &[
        (5.2, 0.17),
        (5.2, 0.33),
        (-1.5, 0.14),
        (-1.5, 0.36),
        (2.0, 0.25),
    ] {
        let (at, n) = on_hull(x, t);
        let up = Quat::from_rotation_arc(Vec3::Y, n);
        let base = at + n * 0.15;
        m.append(
            &MeshData::cylinder(16),
            Mat4::from_scale_rotation_translation(Vec3::new(1.0, 0.3, 1.0), up, base),
            p.metal,
        );
        m.append(
            &MeshData::sphere(16, 8),
            Mat4::from_scale_rotation_translation(Vec3::new(0.85, 0.55, 0.85), up, base + n * 0.15),
            p.hull,
        );
        for dz in [-0.14, 0.14] {
            let side = up * Vec3::Z * dz;
            block(
                m,
                base + n * 0.28 + side + Vec3::X * 0.75,
                Vec3::new(1.2, 0.09, 0.09),
                up,
                p.metal,
            );
        }
    }
}
