//! Saltmarket's edge: what boxes the town in. The reservoir was dammed in a
//! dry valley, so sandstone cliffs close the south, east and north sides; the
//! west, where the valley opens and the main road ran out, was walled off in
//! the war with a barricade of stacked shipping containers.
//!
//! The cliff doesn't follow the arena's square. Its foot wanders well out
//! past the old town edge in bays, gullies cut back into the rock, and
//! headlands push in. A talus slope of sand and fallen rock leans against
//! its foot, and the face above is one sculpted surface (like the Sanctum's
//! mesa) with colliders that follow it. Behind it, the ground is the
//! plateau. The map's playable ground reaches `MARGIN` past the town square
//! on every side to make room.
//!
//! Everything here works in the town's frame (the town square is
//! 0..`SIZE`); the map is shifted at the end so the arena starts at 0.

use glam::{Vec2, Vec3};

use super::saltmarket::SIZE;
use super::{Map, OBox};
use crate::render::material::Material;
use crate::render::mesh::{MeshData, Vertex};
use crate::rng::Rng;

/// How far the playable ground reaches past the town square, m.
pub(super) const MARGIN: f32 = 40.0;
/// Radius of the U's rounded corners (see `locate`).
const CORNER: f32 = 34.0;

/// Sandstone strata.
const STRATA: [Vec3; 6] = [
    Vec3::new(0.7, 0.5, 0.34),
    Vec3::new(0.55, 0.36, 0.24),
    Vec3::new(0.79, 0.62, 0.44),
    Vec3::new(0.5, 0.33, 0.23),
    Vec3::new(0.66, 0.45, 0.3),
    Vec3::new(0.74, 0.55, 0.38),
];

const SAND: Vec3 = Vec3::new(0.72, 0.62, 0.45);

/// Faded container paint.
const CONTAINERS: [Vec3; 5] = [
    Vec3::new(0.42, 0.2, 0.14),
    Vec3::new(0.24, 0.31, 0.38),
    Vec3::new(0.3, 0.35, 0.25),
    Vec3::new(0.52, 0.4, 0.22),
    Vec3::new(0.46, 0.45, 0.42),
];

/// A straight edge: its line runs from `origin` along `along`; `inward`
/// points to the open side.
struct Side {
    origin: Vec2,
    along: Vec2,
    inward: Vec2,
}

impl Side {
    fn yaw(&self) -> f32 {
        // OBox local +x is (cos yaw, -sin yaw) on the ground.
        (-self.along.y).atan2(self.along.x)
    }

    /// How far a box reaches past the line toward the open side.
    fn reach(&self, o: &OBox) -> f32 {
        o.corners()
            .iter()
            .map(|c| (*c - self.origin).dot(self.inward))
            .fold(f32::MIN, f32::max)
    }

    /// Slides the box across the line so its inner face is `inset` in.
    fn seat(&self, o: &mut OBox, inset: f32) {
        let r = self.reach(o);
        o.center += self.inward * (inset - r);
    }

    /// Slides the box across the line so even the shallower end of its
    /// (slightly turned) inner face is `inset` in.
    fn seat_whole(&self, o: &mut OBox, inset: f32) {
        let mut r: Vec<f32> = o
            .corners()
            .iter()
            .map(|c| (*c - self.origin).dot(self.inward))
            .collect();
        r.sort_by(f32::total_cmp);
        o.center += self.inward * (inset - r[2]);
    }
}

// ---- Noise ----------------------------------------------------------------

fn hash(i: i32, j: i32, k: u32) -> f32 {
    let h = (i as u32).wrapping_mul(0x9E37_79B9)
        ^ (j as u32).wrapping_mul(0x7F4A_7C15)
        ^ k.wrapping_mul(0x85EB_CA6B);
    let h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
    let h = (h ^ (h >> 12)).wrapping_mul(0x297A_2D39);
    (h ^ (h >> 15)) as f32 / u32::MAX as f32
}

fn fade(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Smooth value noise along a line, 0..1, one bump per unit.
fn noise(x: f32, k: u32) -> f32 {
    let (i, f) = (x.floor(), fade(x.fract()));
    let i = i as i32;
    hash(i, 0, k) * (1.0 - f) + hash(i + 1, 0, k) * f
}

/// Smooth value noise over a plane, 0..1.
fn noise2(x: f32, y: f32, k: u32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (fade(x - x.floor()), fade(y - y.floor()));
    let a = hash(ix, iy, k) * (1.0 - fx) + hash(ix + 1, iy, k) * fx;
    let b = hash(ix, iy + 1, k) * (1.0 - fx) + hash(ix + 1, iy + 1, k) * fx;
    a * (1.0 - fy) + b * fy
}

/// -1..1 noise with detail: `octaves` layers, each twice as fine and half
/// as strong.
fn fbm(x: f32, octaves: u32, k: u32) -> f32 {
    let (mut sum, mut amp, mut f, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += amp * (noise(x * f, k + o * 31) * 2.0 - 1.0);
        norm += amp;
        amp *= 0.5;
        f *= 2.03;
    }
    sum / norm
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A bump 1 at 0 falling to 0 at `w` either side.
fn bump(d: f32, w: f32) -> f32 {
    1.0 - smoothstep(0.0, w, d.abs())
}

/// Smooth max / min, blending over `k`.
fn smax(a: f32, b: f32, k: f32) -> f32 {
    let h = (0.5 + 0.5 * (a - b) / k).clamp(0.0, 1.0);
    b + (a - b) * h + k * h * (1.0 - h)
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    -smax(-a, -b, k)
}

// ---- The cliff's outline --------------------------------------------------

/// Where `p` stands against the U the cliff is laid out on: the town square's
/// south, east and north sides with rounded corners, open to the west.
/// Returns (how far out past the U, positive outward; distance along it from
/// the south side's x = 0; the U's inward normal there).
fn locate(p: Vec2) -> (f32, f32, Vec2) {
    let x1 = SIZE - CORNER;
    let arc = CORNER * std::f32::consts::FRAC_PI_2;
    let mut best = (f32::MIN, 0.0, Vec2::Y);
    let mut take = |d: f32, u: f32, n: Vec2| {
        if d > best.0 {
            best = (d, u, n);
        }
    };
    if p.x <= x1 {
        take(-p.y, p.x, Vec2::Y);
        take(
            p.y - SIZE,
            x1 + 2.0 * arc + (SIZE - 2.0 * CORNER) + (x1 - p.x),
            Vec2::NEG_Y,
        );
    } else {
        if p.y < CORNER {
            let v = p - Vec2::new(x1, CORNER);
            let a = v.x.atan2(-v.y).clamp(0.0, std::f32::consts::FRAC_PI_2);
            take(
                v.length() - CORNER,
                x1 + CORNER * a,
                -v.normalize_or(Vec2::NEG_Y),
            );
        }
        if p.y > SIZE - CORNER {
            let v = p - Vec2::new(x1, SIZE - CORNER);
            let a = v.y.atan2(v.x).clamp(0.0, std::f32::consts::FRAC_PI_2);
            take(
                v.length() - CORNER,
                x1 + arc + (SIZE - 2.0 * CORNER) + CORNER * a,
                -v.normalize_or(Vec2::X),
            );
        }
    }
    if p.y >= CORNER && p.y <= SIZE - CORNER {
        take(p.x - SIZE, x1 + arc + (p.y - CORNER), Vec2::NEG_X);
    }
    best
}

/// The U at distance `u` along it: (point, inward normal).
fn on_u(u: f32) -> (Vec2, Vec2) {
    let x1 = SIZE - CORNER;
    let arc = CORNER * std::f32::consts::FRAC_PI_2;
    let side = SIZE - 2.0 * CORNER;
    if u <= x1 {
        (Vec2::new(u, 0.0), Vec2::Y)
    } else if u <= x1 + arc {
        let a = (u - x1) / CORNER;
        let out = Vec2::new(a.sin(), -a.cos());
        (Vec2::new(x1, CORNER) + out * CORNER, -out)
    } else if u <= x1 + arc + side {
        (Vec2::new(SIZE, CORNER + (u - x1 - arc)), Vec2::NEG_X)
    } else if u <= x1 + 2.0 * arc + side {
        let a = (u - x1 - arc - side) / CORNER;
        let out = Vec2::new(a.cos(), a.sin());
        (Vec2::new(x1, SIZE - CORNER) + out * CORNER, -out)
    } else {
        let back = u - (x1 + 2.0 * arc + side);
        (Vec2::new(x1 - back, SIZE), Vec2::NEG_Y)
    }
}

/// Where the U runs from and to: the south side at the west edge of the
/// playable ground, round to the north side there.
fn u_range() -> (f32, f32) {
    let x1 = SIZE - CORNER;
    let total = x1 + 2.0 * CORNER * std::f32::consts::FRAC_PI_2 + (SIZE - 2.0 * CORNER) + x1;
    (-MARGIN, total + MARGIN)
}

/// How far out past the U the cliff's foot stands at `u`, m. Wanders on
/// several scales; gullies cut back into the rock; headlands push in. Kept
/// clear of the south team's line and inside the playable ground.
fn foot_out(u: f32) -> f32 {
    let base = 15.0 + 11.0 * fbm(u / 90.0, 3, 11) + 3.0 * fbm(u / 14.0, 2, 12);
    // Gullies: (where, half-width, depth). Headlands: (where, half-width, how
    // far in).
    let gullies = [
        (58.0, 9.0, 17.0),
        (318.0, 11.0, 16.0),
        (470.0, 8.0, 13.0),
        (612.0, 10.0, 18.0),
    ];
    let heads = [
        (272.0, 14.0, 15.0),
        (392.0, 18.0, 17.0),
        (545.0, 15.0, 12.0),
        (650.0, 13.0, 11.0),
    ];
    let mut out = base;
    for (c, w, d) in gullies {
        // A gully's head is narrow, its mouth wider: the bump widens as it
        // comes back out to the foot line.
        out += d * bump(u - c, w).powf(0.7);
    }
    for (c, w, d) in heads {
        out -= d * bump(u - c, w);
    }
    // The south team lines up 6 m in from the town's south edge; the talus
    // stays clear of it.
    let floor = if u > 14.0 && u < 235.0 { 6.0 } else { -4.0 };
    let out = smax(out, floor, 4.0);
    // Well inside the playable ground's edge, so the plateau there is flat
    // where it meets the scenery.
    smin(out, MARGIN - 12.0, 4.0)
}

/// Signed distance past the cliff's foot (positive: into the rock), and the
/// distance along the U.
pub(super) fn past_foot(p: Vec2) -> (f32, f32) {
    let (d, u, _) = locate(p);
    (d - foot_out(u), u)
}

/// The talus at `u`: how far out from the foot it reaches, and how high it
/// stands against the foot.
fn talus(u: f32) -> (f32, f32) {
    // Narrower along the town's south edge, where the south team lines up.
    let south = if u > 14.0 && u < 235.0 { 0.6 } else { 1.0 };
    let w = (9.0 + 5.0 * fbm(u / 35.0, 2, 21)) * south;
    let h = w * (0.42 + 0.12 * noise(u / 20.0, 22));
    (w, h * (0.4 + 0.6 * taper(u)))
}

/// How much of its height the valley side keeps at `u`: all of it until
/// past the barricade, then it falls away into the sand toward the open
/// desert, west.
fn taper(u: f32) -> f32 {
    let (u0, u1) = u_range();
    smoothstep(u0 + 4.0, u0 + 36.0, u) * smoothstep(u1 - 4.0, u1 - 36.0, u)
}

/// The plateau's height over the town ground behind the cliff at `u`.
fn plateau(u: f32) -> f32 {
    WEST_SAND + (17.0 + 5.0 * fbm(u / 70.0, 3, 31) - WEST_SAND) * taper(u)
}

/// How high the sand drifts behind the barricade, m.
const WEST_SAND: f32 = 4.5;

/// The ground everywhere on the playable ground and just past it, given the
/// town's own `ground`: the talus against the cliff's foot, the plateau
/// behind it, and the sand banked behind the barricade.
pub(super) fn land(p: Vec2, ground: fn(f32, f32) -> f32) -> f32 {
    let g = ground(p.x, p.y);
    let (s, u) = past_foot(p);
    let (w, h) = talus(u);
    // No talus up against the barricade: the rock meets the containers
    // directly, so there's no sand ramp up onto them.
    let h = h * smoothstep(5.0, 18.0, p.x);
    let slope = h * smoothstep(-w, 0.0, s.min(0.0)).powf(1.4);
    // Into the rock: up from the talus onto the plateau, out of sight inside
    // the face.
    let rock = g + h + (plateau(u) - h) * smoothstep(4.5, 8.5, s);
    let mut y = if s > 0.0 { rock } else { g + slope };
    // Behind the barricade: drifted sand.
    if p.x < -1.0 && s < 2.5 {
        let drift =
            WEST_SAND * smoothstep(-1.0, -14.0, p.x) * (0.75 + 0.25 * noise(p.y / 18.0, 41));
        y = y.max(g + drift);
    }
    y
}

/// The barricade's ends: where the south and north cliffs' feet cross x = 0,
/// pushed into the rock.
fn barricade_ends() -> (f32, f32) {
    let mut z0 = 0.0;
    while past_foot(Vec2::new(0.0, z0)).0 < 0.0 {
        z0 -= 0.5;
    }
    let mut z1 = SIZE;
    while past_foot(Vec2::new(0.0, z1)).0 < 0.0 {
        z1 += 0.5;
    }
    (z0 - 1.5, z1 + 1.5)
}

/// Where the cliff's foot crosses the line z = `z`, going east: for the dam
/// to key into it.
pub(super) fn foot_east(z: f32) -> f32 {
    let mut x = SIZE - 20.0;
    while past_foot(Vec2::new(x, z)).0 < 0.0 && x < SIZE + MARGIN {
        x += 0.25;
    }
    x
}

impl Map {
    /// The cliff round the south, east and north, and the barricade across
    /// the west. `ground` is the town's ground (without the talus).
    pub(super) fn valley_edge(&mut self, rng: &mut Rng, ground: fn(f32, f32) -> f32) {
        let land = move |p: Vec2| land(p, ground);
        let path = foot_path();
        // The plateau's height above a point on the foot.
        let top = move |p: Vec2| ground(p.x, p.y) + plateau(past_foot(p).1);
        let fronts = sculpt_cliff(&mut self.sculpted, &path, &land, &top);
        // Colliders: one thin slab per stretch of face, from its most
        // forward point at tank height back into the rock.
        for k in (0..path.len() - 1).step_by(2) {
            let j = (k + 2).min(path.len() - 1);
            let (a, b) = (fronts[k], fronts[j]);
            let along = b.0 - a.0;
            let len = along.length();
            if len < 1e-3 {
                continue;
            }
            let nrm = (path[k].1 + path[j].1).normalize_or(path[k].1);
            // The slab's front is the more forward of the two ends.
            let front = (a.0 - path[k].0).dot(nrm).max((b.0 - path[j].0).dot(nrm));
            let mid = (path[k].0 + path[j].0) * 0.5 + nrm * front;
            let depth = 3.0;
            let dir = along / len;
            self.oboxes.push(OBox {
                center: mid - nrm * depth * 0.5,
                // OBox local +x is (cos yaw, -sin yaw) on the ground.
                yaw: (-dir.y).atan2(dir.x),
                half: Vec2::new(len * 0.5 + 0.35, depth * 0.5),
                y0: a.1.min(b.1) - 3.0,
                y1: a.2.max(b.2) + 1.0,
                hidden: true,
                ..OBox::default()
            });
        }
        self.talus_rocks(rng, &land);
        let (z0, z1) = barricade_ends();
        self.barricade(rng, &land, z0, z1);
    }

    /// Rock fallen from the face, lying on the talus: big blocks near the
    /// foot, smaller ones scattered further out.
    fn talus_rocks(&mut self, rng: &mut Rng, land: &dyn Fn(Vec2) -> f32) {
        let (u0, u1) = u_range();
        let mut placed = 0;
        for _ in 0..2000 {
            if placed >= 170 {
                break;
            }
            let u = rng.range(u0 + 30.0, u1 - 30.0);
            let (base, nrm) = on_u(u);
            let (w, _) = talus(u);
            // Out from the foot, across the talus (and a little past it).
            let t = rng.f32().powf(2.2);
            let size = rng.range(0.5, 1.4) + 2.4 * (1.0 - t) * rng.f32().powi(2);
            let p = base - nrm * (foot_out(u) - 0.8 - t * (w + 4.0));
            let (s, _) = past_foot(p);
            // Keep off the town square's south strip (the south team's line)
            // and out of the barricade.
            if s > -0.5 || p.x < 3.0 || (p.y > -2.0 && p.y < 14.0 && p.x < 232.0) {
                continue;
            }
            let g = land(p);
            let color = STRATA[rng.index(STRATA.len())] * rng.range(0.7, 0.88);
            // Two blocks through each other, tipped: a lump of rock.
            for k in 0..2 {
                let sz = size * if k == 0 { 1.0 } else { rng.range(0.55, 0.85) };
                let off = Vec2::new(rng.range(-0.3, 0.3), rng.range(-0.3, 0.3)) * sz;
                let tip = if sz > 1.6 { 0.25 } else { 0.6 };
                self.oboxes.push(OBox {
                    center: p + off,
                    yaw: rng.range(0.0, std::f32::consts::TAU),
                    half: Vec2::new(sz * rng.range(0.6, 0.95), sz * rng.range(0.5, 0.8)),
                    y0: g - sz * 0.7,
                    y1: g + sz * rng.range(0.35, 0.8),
                    color,
                    side_mat: Material::Rubble,
                    top_mat: Material::Rubble,
                    tilt: rng.range(-tip, tip),
                    roll: rng.range(-tip, tip),
                    chip: sz * 0.45,
                    // Big ones stop a tank; small ones it rolls over.
                    solid: sz > 1.3,
                    ..OBox::default()
                });
            }
            placed += 1;
        }
    }

    /// The west barricade across the valley mouth: 40-foot shipping
    /// containers end to end along the edge, stacked two high and three in
    /// places, a few shoved askew; steel tank traps where the roads ran out.
    fn barricade(&mut self, rng: &mut Rng, land: &dyn Fn(Vec2) -> f32, z0: f32, z1: f32) {
        let side = Side {
            origin: Vec2::ZERO,
            along: Vec2::Y,
            inward: Vec2::X,
        };
        let (wide, tall) = (2.44f32, 2.6f32);
        // Lowest and highest ground under a container.
        let foot = |z0: f32, z1: f32| {
            (0..=4)
                .map(|k| land(Vec2::new(0.0, z0 + (z1 - z0) * k as f32 / 4.0)))
                .fold((f32::MAX, f32::MIN), |(lo, hi), g| (lo.min(g), hi.max(g)))
        };
        // Sun-faded paint, or rusted through to bare steel.
        let paint = |rng: &mut Rng| {
            if rng.chance(0.3) {
                (
                    Vec3::new(0.42, 0.27, 0.17) * rng.range(0.8, 1.1),
                    Material::Rust,
                )
            } else {
                let c = CONTAINERS[rng.index(CONTAINERS.len())] * rng.range(0.8, 1.05);
                (c, Material::Painted)
            }
        };
        let mut z = z0;
        while z < z1 {
            // Bottom row: tight end to end, sunk a little into the sand.
            // 40-foot containers, or 20-foot ones where the ground climbs
            // too fast under a long one (up onto the north ridge).
            let mut len = 12.2;
            let (mut g, mut high) = foot(z, z + len);
            if high - g > 0.6 {
                len = 6.06;
                (g, high) = foot(z, z + len);
            }
            let (color, side_mat) = paint(rng);
            let mut base = OBox {
                center: Vec2::new(0.0, z + len * 0.5),
                yaw: side.yaw() + rng.range(-0.03, 0.03),
                half: Vec2::new(len * 0.5, wide * 0.5),
                y0: g - 0.15,
                y1: g - 0.15 + tall,
                color,
                side_mat,
                top_mat: Material::Rust,
                ..OBox::default()
            };
            side.seat_whole(&mut base, rng.range(0.5, 1.0));
            self.oboxes.push(base);
            // A second one stacked on top (always, so the wall is too tall
            // to climb), often shifted along and turned a touch.
            let (color, side_mat) = paint(rng);
            let mut upper = OBox {
                center: base.center + Vec2::new(0.0, rng.range(-0.4, 0.4)),
                yaw: base.yaw + rng.range(-0.08, 0.08),
                y0: base.y1,
                y1: base.y1 + tall,
                color,
                side_mat,
                ..base
            };
            side.seat_whole(&mut upper, rng.range(0.2, 0.8));
            self.oboxes.push(upper);
            // A third here and there, and always where the ground rises
            // under the stack (toward the north ridge) and two would leave it
            // low on the uphill end.
            let steep = high - g > 0.3;
            if steep || rng.chance(0.3) {
                let (color, side_mat) = paint(rng);
                let shift = if steep { 0.4 } else { len * 0.25 };
                let mut third = OBox {
                    center: upper.center + Vec2::new(-0.3, rng.range(-shift, shift)),
                    yaw: upper.yaw + rng.range(-0.15, 0.15) * shift / 3.0,
                    y0: upper.y1,
                    y1: upper.y1 + tall,
                    color,
                    side_mat,
                    ..upper
                };
                if steep {
                    side.seat_whole(&mut third, rng.range(0.0, 0.5));
                } else {
                    side.seat(&mut third, rng.range(-0.5, 0.8));
                }
                self.oboxes.push(third);
            }
            // Jammed end to end: no gap a shell (or a sand-drift climb) gets through.
            z += len - rng.range(0.1, 0.3);
        }
        // Tank traps (three crossed girders) where the roads met the wall.
        for (cz, n) in [(108.0f32, 3), (56.0, 2), (22.0, 2)] {
            for k in 0..n {
                let c = Vec2::new(
                    rng.range(4.0, 6.0),
                    cz + (k as f32 - (n - 1) as f32 * 0.5) * 4.5,
                );
                self.hedgehog(rng, c, land(c));
            }
        }
    }

    /// A Czech hedgehog: three steel girders crossed at their middles.
    fn hedgehog(&mut self, rng: &mut Rng, at: Vec2, g: f32) {
        let steel = Vec3::new(0.2, 0.16, 0.13);
        let mid = Vec3::new(at.x, g + 0.75, at.y);
        let turn = rng.range(0.0, std::f32::consts::TAU);
        let axes = [
            Vec3::new(1.0, 0.75, 0.0),
            Vec3::new(-0.5, 0.75, 0.87),
            Vec3::new(-0.5, 0.75, -0.87),
        ];
        for a in axes {
            let a = glam::Quat::from_rotation_y(turn) * a.normalize() * 1.1;
            self.segment(mid - a, mid + a, 0.12, steel, Material::Rust, true);
        }
    }
}

/// The cliff's foot, end to end, sampled about every half meter along it:
/// (point, inward normal).
fn foot_path() -> Vec<(Vec2, Vec2)> {
    let (u0, u1) = u_range();
    // Finely by u first, then evenly by distance along the foot itself (a
    // gully's walls are long for little u).
    let mut fine = Vec::new();
    let mut u = u0;
    while u <= u1 {
        // Not where the valley side has fallen away into the sand.
        if taper(u) > 0.03 {
            let (b, n) = on_u(u);
            fine.push(b - n * foot_out(u));
        }
        u += 0.1;
    }
    let mut pts = vec![fine[0]];
    let mut acc = 0.0;
    for w in fine.windows(2) {
        acc += w[0].distance(w[1]);
        if acc >= 0.55 {
            pts.push(w[1]);
            acc = 0.0;
        }
    }
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (a, b) = (pts[i.saturating_sub(1)], pts[(i + 1).min(n - 1)]);
            // The U runs counterclockwise round the town: inward is left.
            (pts[i], (b - a).normalize().perp())
        })
        .collect()
}

/// Sculpt the cliff face along `path` (its foot, normal toward the open
/// side) into `m`, as one smooth-shaded surface: a lip at the plateau's edge
/// with a cap running back under the plateau's sand; masses that bulge and
/// sink; strata bands, warped, set in or out, with ledges; fluting only in
/// stretches, wandering and breaking off; joints cut down from the lip; dark
/// varnish streaks under the ledges; the foot buried in the talus. Returns,
/// per path point, the face's most forward point at tank height, the floor
/// there and the lip.
fn sculpt_cliff(
    m: &mut MeshData,
    path: &[(Vec2, Vec2)],
    land: &dyn Fn(Vec2) -> f32,
    top: &dyn Fn(Vec2) -> f32,
) -> Vec<(Vec2, f32, f32)> {
    const BAND: f32 = 3.6;
    const STEP: f32 = 0.6;
    // Back from the foot to where the lip's height is read, and where the
    // cap tucks under the plateau.
    const BACK: f32 = 10.0;
    // How far forward of (positive) or back from the foot the face stands,
    // `a` m along the foot and `depth` m down from the lip, on a face `tall`
    // m from lip to floor.
    let face = |a: f32, depth: f32, tall: f32, bend: f32| {
        let up = tall - depth;
        // Big masses, a few columns across, bulging and sinking.
        // Buttresses standing out and bays sunk back, tens of meters
        // across, then smaller bulges on them.
        // (Taller than wide: the rock splits on vertical joints.)
        let mass = 2.4 * (noise2(a / 24.0, depth / 40.0, 1) * 2.0 - 1.0)
            + 0.8 * (noise2(a / 6.0, depth / 30.0, 2) * 2.0 - 1.0)
            + 0.25 * (noise2(a / 2.5, depth / 6.0, 13) * 2.0 - 1.0);
        // Strata: bands of harder rock, their lines warped.
        let dw = depth + 2.2 * (noise2(a / 28.0, depth / 12.0, 3) * 2.0 - 1.0);
        let band = (dw / BAND).floor();
        let f = dw / BAND - band;
        let set = 0.55 * (noise(a / 22.0 + band * 7.31, 4) * 2.0 - 1.0);
        let ledge = 0.65
            * (noise(a / 9.0 + band * 3.17, 5) * 1.7 - 0.7).max(0.0)
            * smoothstep(0.72, 0.94, f);
        // Fluting, only where the rock takes it: grooves that wander and
        // break off.
        let present = smoothstep(0.52, 0.72, noise(a / 38.0, 6));
        let r = noise2(a / 1.9, depth / 9.0, 7);
        let ridge = (1.0 - (2.0 * r - 1.0).abs()).powf(1.6);
        let flute = present * 0.75 * ridge * smoothstep(1.0, 5.0, up);
        // Joints: clefts cut down from the lip, some way down the face.
        let cell = (a / 11.0).floor() as i32;
        let at = (cell as f32 + 0.15 + 0.7 * hash(cell, 1, 8)) * 11.0;
        let cleft = if hash(cell, 2, 8) > 0.55 {
            let width = 0.5 + 0.7 * hash(cell, 3, 8);
            let reach = tall * (0.25 + 0.6 * hash(cell, 4, 8));
            -1.5 * bump(a - at, width) * (1.0 - smoothstep(reach - 2.0, reach, depth))
        } else {
            0.0
        };
        // The caprock overhangs a little; the face leans back as it rises;
        // the foot spreads under the sand.
        let undercut = 0.7 * smoothstep(0.3, 1.5, depth) * (1.0 - smoothstep(2.5, 6.0, depth));
        let lean = -0.06 * (up - 2.0);
        let flare = 0.6 * smoothstep(1.0, -2.0, up);
        // A short face (where the valley side falls away, west) is plainer,
        // and so is one round a tight bend (a gully's head), where bumps out
        // from both walls would cross.
        let relief = smoothstep(1.0, 8.0, tall) * bend;
        let out = (mass + set + ledge + flute + cleft) * relief + lean + flare - undercut * relief;
        // Never so far back the plateau's rise behind shows through, nor so
        // far forward it's past its colliders' reach.
        smin(smax(out, -3.6, 0.8), 3.0, 0.8)
    };
    let shade = |a: f32, depth: f32| {
        let dw = depth + 2.2 * (noise2(a / 28.0, depth / 12.0, 3) * 2.0 - 1.0);
        let b = dw / BAND;
        let (i, f) = (b.floor().max(0.0) as usize, b.fract());
        let blend = smoothstep(0.82, 1.0, f);
        let pick = |i: usize| STRATA[(i * 7 + 3) % STRATA.len()];
        let band = pick(i).lerp(pick(i + 1), blend);
        // Desert varnish: dark streaks running down from the ledges.
        let streak = noise2(a / 0.8, depth / 30.0, 9) * noise(a / 12.0, 19);
        let varnish = 1.0 - 0.2 * smoothstep(0.4, 0.6, streak);
        let mottle = 0.9 + 0.2 * noise2(a / 6.0, depth / 5.0, 10);
        band * varnish * mottle
    };

    let n = path.len();
    let mut along = vec![0.0f32; n];
    for i in 1..n {
        along[i] = along[i - 1] + path[i].0.distance(path[i - 1].0);
    }
    // The rim breaks unevenly: blocks fallen out of it, steps, a ragged
    // edge, on top of the plateau's own rise and fall.
    // How far back the cap reaches: BACK, or less where the ground behind
    // isn't rock that far (across a narrow gully, the far wall's open
    // ground).
    let backs: Vec<f32> = path
        .iter()
        .map(|&(p, nrm)| {
            let mut t = BACK;
            while t > 6.0 && past_foot(p - nrm * t).0 < 6.0 {
                t -= 0.5;
            }
            t
        })
        .collect();
    let lips: Vec<f32> = path
        .iter()
        .enumerate()
        .map(|(i, &(p, _))| {
            let a = along[i];
            let steps = 1.6 * ((noise(a / 7.0, 14) * 4.0).floor() / 4.0 - 0.4);
            let ragged = 0.5 * (noise(a / 1.3, 15) - 0.5) + 0.9 * (noise(a / 4.0, 16) - 0.5);
            let notch = -3.5 * smoothstep(0.78, 0.92, noise(a / 16.0, 17));
            top(p) + (steps + ragged + notch) * taper(past_foot(p).1)
        })
        .collect();
    let floors: Vec<f32> = path.iter().map(|&(p, _)| land(p)).collect();
    let deepest = (0..n).map(|i| lips[i] - floors[i]).fold(0.0, f32::max) + 3.0;
    let mut depths = Vec::new();
    let mut d = 0.5;
    while d < deepest {
        depths.push(d);
        d += STEP;
    }
    let rows = 2 + depths.len();
    let mut pos = vec![Vec3::ZERO; n * rows];
    let mut col = vec![Vec3::ZERO; n * rows];
    let mut sandy = vec![false; n * rows];
    // How sharply the foot turns round each point, over a few meters.
    let bends: Vec<f32> = (0..n)
        .map(|i| {
            let (a, b) = (path[i.saturating_sub(6)].1, path[(i + 6).min(n - 1)].1);
            1.0 - smoothstep(0.25, 1.0, a.angle_to(b).abs())
        })
        .collect();
    let mut fronts = Vec::with_capacity(n);
    for i in 0..n {
        let (p, nrm) = path[i];
        let bend = bends[i];
        let a = along[i];
        let (lip, fl) = (lips[i], floors[i]);
        let tall = lip - fl;
        let mut front = (f32::MIN, p);
        for r in 0..rows {
            let (y, out, depth) = match r {
                0 => {
                    let t = backs[i];
                    // Held up level with the rim (over the plateau's rise),
                    // but on the sand where the valley side falls away.
                    let hold = lip - 0.4 - 8.0 * (1.0 - taper(past_foot(p).1));
                    ((land(p - nrm * t) + 0.05).max(hold), -t, 0.0)
                }
                1 => (lip + 0.1, face(a, 0.0, tall, bend), 0.0),
                _ => {
                    let jd = hash(i as i32, r as i32, 12) - 0.5;
                    let dp = (depths[r - 2] + jd * STEP * 0.4).max(0.3);
                    (lip - dp, face(a, dp, tall, bend), dp)
                }
            };
            let q = p + nrm * out;
            pos[i * rows + r] = Vec3::new(q.x, y, q.y);
            if y > fl + 0.4 && y < fl + 4.5 && out > front.0 {
                front = (out, q);
            }
            sandy[i * rows + r] = r == 0;
            // Sand drifted up its foot, and lying on its ledges' tops.
            let drift = smoothstep(2.5, 0.4, y - fl);
            col[i * rows + r] = if r == 0 {
                SAND * 0.95
            } else {
                shade(a, depth.max(0.01)).lerp(SAND * 0.95, drift)
            };
        }
        if front.0 == f32::MIN {
            front = (0.0, p);
        }
        fronts.push((front.1, fl, lip));
    }
    // The cap takes its own copy of the lip row, so the edge stays crisp.
    let first_dup = pos.len();
    for i in 0..n {
        pos.push(pos[i * rows + 1]);
        col.push(SAND * 0.95);
        sandy.push(true);
    }
    let mut tris: Vec<[usize; 3]> = Vec::new();
    for i in 0..n - 1 {
        let nm = (path[i].1 + path[i + 1].1).normalize_or_zero();
        let out = Vec3::new(nm.x, 0.0, nm.y);
        for r in 0..rows - 1 {
            let cap = r == 0;
            let v = |i: usize, r: usize| {
                if cap && r == 1 {
                    first_dup + i
                } else {
                    i * rows + r
                }
            };
            let q = [v(i, r), v(i + 1, r), v(i + 1, r + 1), v(i, r + 1)];
            let pair = if (i + r) % 2 == 0 {
                [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]
            } else {
                [[q[0], q[1], q[3]], [q[1], q[2], q[3]]]
            };
            let want = if cap { Vec3::Y } else { out };
            tris.extend(pair.into_iter().map(|t| orient(&pos, t, want)));
        }
    }
    let mut nrm = vec![Vec3::ZERO; pos.len()];
    for t in &tris {
        let f = (pos[t[1]] - pos[t[0]]).cross(pos[t[2]] - pos[t[0]]);
        for &k in t {
            nrm[k] += f;
        }
    }
    let base = m.verts.len() as u32;
    for k in 0..pos.len() {
        let mat = if sandy[k] {
            Material::Sand
        } else {
            Material::Rubble
        };
        m.verts.push(Vertex {
            pos: pos[k].to_array(),
            normal: nrm[k].normalize_or_zero().to_array(),
            color: col[k].extend(mat.id()).to_array(),
        });
    }
    for t in tris {
        m.indices.extend(t.iter().map(|&k| base + k as u32));
    }
    fronts
}

/// Triangle `t` wound to face along `want`.
fn orient(pos: &[Vec3], mut t: [usize; 3], want: Vec3) -> [usize; 3] {
    if (pos[t[1]] - pos[t[0]])
        .cross(pos[t[2]] - pos[t[0]])
        .dot(want)
        < 0.0
    {
        t.swap(1, 2);
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The arena's own boundary (past it a tank is out, and dies) is out
    /// of reach everywhere: deep in the rock, or behind the barricade.
    #[test]
    fn arena_edge_is_out_of_reach() {
        let (lo, hi) = (-MARGIN, SIZE + MARGIN);
        let mut t = lo;
        while t <= hi {
            for p in [
                Vec2::new(t, lo),
                Vec2::new(t, hi),
                Vec2::new(lo, t),
                Vec2::new(hi, t),
            ] {
                let (s, _) = past_foot(p);
                assert!(
                    s > 9.0 || p.x < -2.0,
                    "the edge at {p} is only {s} m into the rock"
                );
            }
            t += 1.0;
        }
    }

    /// The cliff's foot wanders: bays and headlands tens of meters apart,
    /// not the town's square.
    #[test]
    fn outline_is_not_a_square() {
        let (u0, u1) = u_range();
        let outs: Vec<f32> = (0..2000)
            .map(|k| foot_out(u0 + (u1 - u0) * k as f32 / 1999.0))
            .collect();
        let (lo, hi) = outs
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &o| (a.min(o), b.max(o)));
        assert!(hi - lo > 25.0, "the foot only wanders {lo}..{hi} m");
    }

    /// No gaps: along the barricade every point is held by containers too
    /// tall to climb, and along the cliff a collider stands right where the
    /// rock face is at tank height, tall enough no tank gets over it.
    #[test]
    fn edge_is_closed_by_what_you_see() {
        let m = Map::saltmarket_town();
        let land = |p: Vec2| land(p, super::super::saltmarket::ground);
        let (z0, z1) = barricade_ends();
        let mut z = z0 + 1.5;
        while z < z1 - 1.5 {
            let p = Vec2::new(0.05, z);
            let g = land(p);
            let over = |q: Vec2| {
                m.oboxes
                    .iter()
                    .filter(move |o| o.solid && !o.is_turned() && o.distance(q) == 0.0)
            };
            let in_rock = past_foot(p).0 > 0.0;
            assert!(
                in_rock || over(p).any(|o| o.y0 <= g && o.y1 > g + 1.0),
                "nothing stops a tank at the barricade at {p}"
            );
            // Too tall to climb, but for gaps narrower than a tank.
            let top = [-1.0, 0.0, 1.0]
                .iter()
                .flat_map(|&s| over(p + Vec2::Y * s))
                .map(|o| o.y1)
                .fold(f32::MIN, f32::max);
            assert!(
                in_rock || top > g + 4.0,
                "the barricade at {p} is only {} m high",
                top - g
            );
            z += 0.5;
        }
        let colliders: Vec<&OBox> = m.oboxes.iter().filter(|o| o.hidden && o.solid).collect();
        let path = foot_path();
        for (i, &(p, nrm)) in path.iter().enumerate() {
            let (_, u) = past_foot(p);
            if taper(u) < 0.5 || p.x < 2.0 {
                // Behind the barricade, out of reach.
                continue;
            }
            let g = land(p);
            // Just in front of the face's foot.
            let near = colliders.iter().any(|o| {
                (-14..=14).any(|k| o.distance(p + nrm * (k as f32 * 0.25)) == 0.0)
                    && o.y0 <= g
                    && o.y1 > g + 6.0
            });
            assert!(near, "no collider at the cliff face at {p} (sample {i})");
        }
    }

    /// The U's frame round-trips: a point placed out from the U at `u`
    /// locates back to the same `u` and distance.
    #[test]
    fn frame_round_trips() {
        let (u0, u1) = u_range();
        let mut u = u0 + 1.0;
        while u < u1 - 1.0 {
            let (b, n) = on_u(u);
            for out in [-10.0, 0.0, 12.0] {
                let (d, uu, _) = locate(b - n * out);
                assert!((d - out).abs() < 0.05, "at u {u}, {out} out: {d}");
                assert!((uu - u).abs() < 0.05, "at u {u}, {out} out: u {uu}");
            }
            u += 3.7;
        }
    }
}
