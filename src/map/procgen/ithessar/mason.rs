//! How the Ithessar's masonry is drawn: sculpted meshes over hidden boxes
//! that collide.
//!
//! - a **mass** (`Mass`): squared stone twisting as it rises, ribbed, its
//!   corners run with worked glass; almond windows (their emblem) sunk deep
//!   in the bays, plasma in them; a glass crown, a vault, and blades up the
//!   corners leaning in over it.
//! - a **curved wall** (`Curve`): swept round an arc, plinth, piers, coping.
//! - a **vault** (`lens_vault`): a lens-shaped roof.

use super::*;
use crate::render::mesh::MeshData;
use glam::Vec4;

/// Stone, a shade darker than the slabs it's drawn over were: it's the glass
/// and the light that ought to show.
const SHADE: f32 = 0.72;

fn paint(c: Vec3, m: Material) -> Vec4 {
    c.extend(m.id())
}

/// A patch of surface, `at` of (0..1, 0..1), in `nu` by `nv` quads, smooth
/// across and facing away from `inside`.
fn sheet(
    m: &mut MeshData,
    (nu, nv): (usize, usize),
    at: impl Fn(f32, f32) -> Vec3,
    inside: Vec3,
    c: Vec4,
) {
    let base = m.verts.len() as u32;
    const E: f32 = 0.01;
    for j in 0..=nv {
        for i in 0..=nu {
            let (u, v) = (i as f32 / nu as f32, j as f32 / nv as f32);
            let (u0, u1) = ((u - E).max(0.0), (u + E).min(1.0));
            let (v0, v1) = ((v - E).max(0.0), (v + E).min(1.0));
            // (Off a little where it's drawn in to a point.)
            let (iu, iv) = (u.clamp(E, 1.0 - E), v.clamp(E, 1.0 - E));
            let du = at(u1, iv) - at(u0, iv);
            let dv = at(iu, v1) - at(iu, v0);
            let p = at(u, v);
            let mut n = dv.cross(du).normalize_or(Vec3::Y);
            if n.dot(p - inside) < 0.0 {
                n = -n;
            }
            m.vert(p, n, c);
        }
    }
    let w = nu as u32 + 1;
    for j in 0..nv as u32 {
        for i in 0..nu as u32 {
            let k = base + j * w + i;
            m.indices.extend([k, k + 1, k + w + 1, k, k + w + 1, k + w]);
        }
    }
}

/// How a mass's width goes as it rises.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Form {
    /// Drawn in a little, flat-crowned.
    Shaft,
    /// Closing in to a blade.
    Bud,
    /// Waisted, flaring again at its crown.
    Waist,
}

#[derive(Clone, Copy)]
pub(super) struct Mass {
    pub center: Vec2,
    pub yaw: f32,
    pub half: Vec2,
    pub y0: f32,
    pub y1: f32,
    /// How far it twists, foot to crown.
    pub turn: f32,
    pub form: Form,
    pub color: Vec3,
    /// How tall its plinth is; none when it stands on legs.
    pub plinth: f32,
}

/// Where on a wall: along it from its middle, and the height.
type On = Vec2;

impl Mass {
    /// Up to where it stands straight: over a tank's height, so what a tank
    /// drives round is its footprint.
    fn straight(&self) -> f32 {
        if self.plinth > 0.0 {
            self.y0 + (self.y1 - self.y0) * 0.34
        } else {
            self.y0
        }
    }

    fn rise(&self, y: f32) -> f32 {
        let foot = self.straight();
        ((y - foot) / (self.y1 - foot)).clamp(0.0, 1.0)
    }

    fn scale(&self, y: f32) -> f32 {
        let t = self.rise(y);
        match self.form {
            Form::Shaft => 1.0 - 0.13 * t,
            Form::Bud => 1.0 - 0.5 * t.powf(2.2),
            Form::Waist => 1.0 - 0.13 * (t * 2.4).sin(),
        }
    }

    fn turned(&self, y: f32) -> f32 {
        self.yaw + self.turn * self.rise(y)
    }

    /// Its corners' radius.
    fn round(&self) -> f32 {
        (self.half.min_element() * 0.18).clamp(0.25, 0.7)
    }

    fn place(&self, q: Vec2, y: f32) -> Vec3 {
        // (An OBox's frame: x to (cos, -sin), z to (sin, cos).)
        let (s, c) = self.turned(y).sin_cos();
        let p = self.center + Vec2::new(q.x * c + q.y * s, -q.x * s + q.y * c);
        Vec3::new(p.x, y, p.y)
    }

    fn axis(&self, y: f32) -> Vec3 {
        Vec3::new(self.center.x, y, self.center.y)
    }

    /// Face `f`'s way out, the way along it, how far out it is and how wide
    /// to the start of its corners.
    fn face(f: usize, half: Vec2, r: f32) -> (Vec2, Vec2, f32, f32) {
        let n = [Vec2::X, Vec2::Y, Vec2::NEG_X, Vec2::NEG_Y][f];
        let (depth, wide) = if f % 2 == 0 {
            (half.x, half.y)
        } else {
            (half.y, half.x)
        };
        (n, n.perp(), depth, wide - r)
    }

    /// A point `out` proud of face `f`.
    fn on(&self, f: usize, p: On, out: f32) -> Vec3 {
        let (n, t, depth, _) = Self::face(f, self.half, 0.0);
        let k = self.scale(p.y);
        self.place(n * (depth * k + out) + t * p.x * k, p.y)
    }

    /// Round the corner after face `f`, `u` from 0 to 1. Drawn in by `shrink`.
    fn corner(&self, f: usize, u: f32, y: f32, out: f32, shrink: f32) -> Vec3 {
        let r = self.round();
        let (n, t, depth, wide) = Self::face(f, self.half, r);
        let (s, c) = (u * FRAC_PI_2).sin_cos();
        let k = self.scale(y) * shrink;
        self.place(
            (n * (depth - r) + t * wide) * k + (n * c + t * s) * (r * k + out),
            y,
        )
    }

    /// A moulding swept all the way round through (height, out, shrink),
    /// facing away from its axis at height `inside`.
    fn course(&self, m: &mut MeshData, profile: &[(f32, f32, f32)], inside: f32, c: Vec4) {
        for w in profile.windows(2) {
            let (a, b) = (w[0], w[1]);
            let inside = self.axis(inside);
            let mid = |v: f32| {
                let l = |x: f32, y: f32| x + (y - x) * v;
                (l(a.0, b.0), l(a.1, b.1), l(a.2, b.2))
            };
            for f in 0..4 {
                sheet(
                    m,
                    (3, 1),
                    |u, v| {
                        let (y, out, s) = mid(v);
                        self.corner(f, u, y, out, s)
                    },
                    inside,
                    c,
                );
                sheet(
                    m,
                    (2, 1),
                    |u, v| {
                        let (y, out, s) = mid(v);
                        let from = self.corner((f + 3) % 4, 1.0, y, out, s);
                        from.lerp(self.corner(f, 0.0, y, out, s), u)
                    },
                    inside,
                    c,
                );
            }
        }
    }

    /// The boxes it collides as.
    pub fn collide(&self, map: &mut Map) {
        let n = ((self.y1 - self.straight()) / 1.8).ceil().max(2.0);
        let body = self.straight();
        let mut spans = vec![];
        if self.plinth > 0.0 {
            spans.push((self.y0 - 0.5, body));
        }
        let step = (self.y1 - body) / n;
        spans.extend((0..n as i32).map(|k| {
            let y = body + k as f32 * step;
            (y, y + step)
        }));
        for (a, b) in spans {
            let y = (a + b) * 0.5;
            map.oboxes.push(OBox {
                center: self.center,
                yaw: self.turned(y),
                half: self.half * self.scale(y),
                y0: a,
                y1: b,
                color: self.color,
                side_mat: C,
                top_mat: C,
                solid: true,
                hidden: true,
                ..OBox::default()
            });
        }
    }

    /// Drawn in full: for what you drive by.
    pub fn draw(&self, map: &mut Map, rng: &mut Rng) {
        let m = &mut map.sculpted;
        let stone = paint(self.color * SHADE, C);
        let trim = paint(self.color * SHADE * 0.72, C);
        let glass = paint(GLASS * 0.55, V);
        let (body, top) = (self.y0 + self.plinth, self.y1 - 0.9);
        let tall = top - body;
        if self.plinth > 0.0 {
            let (y, p) = (self.y0, body);
            self.course(
                m,
                &[
                    (y - 0.5, 0.3, 1.0),
                    (p - 0.35, 0.3, 1.0),
                    (p - 0.08, 0.1, 1.0),
                    (p, -0.02, 1.0),
                ],
                p - 1.0,
                trim,
            );
        }
        let rows = (tall / 4.2).round().clamp(1.0, 4.0) as usize;
        let row = tall / rows as f32;
        let r = self.round();
        // The ribs' and the corner shafts' bends.
        let bends = rows * 2;
        for f in 0..4 {
            let (_, _, _, wide) = Self::face(f, self.half, r);
            let bays = (wide * 2.0 / 2.7).round().max(1.0) as usize;
            let bay = wide * 2.0 / bays as f32;
            let beat = rng.index(2);
            for j in 0..rows {
                for i in 0..bays {
                    let s = -wide + i as f32 * bay;
                    let y = body + j as f32 * row;
                    let cell = (Vec2::new(s, y), Vec2::new(s + bay, y + row));
                    let lit = (i + j + beat) % 2 == 0 && bay > 1.2 && rng.chance(0.62);
                    if lit {
                        self.window(m, f, cell, stone, glass);
                    } else {
                        self.stones(m, rng, f, cell);
                    }
                }
            }
            for i in 1..bays {
                self.rib(m, f, -wide + i as f32 * bay, (body, top), bends, trim);
            }
            let inside = self.axis((body + top) * 0.5);
            sheet(
                m,
                (4, bends),
                |u, v| self.corner(f, u, body + tall * v, 0.16, 1.0),
                inside,
                glass,
            );
            // Its edges, back to the wall.
            for e in [0.0, 1.0] {
                sheet(
                    m,
                    (1, bends),
                    |u, v| self.corner(f, e, body + tall * v, 0.16 - 0.2 * u, 1.0),
                    self.corner(f, 0.5, (body + top) * 0.5, 0.0, 1.0),
                    glass,
                );
            }
        }
        for j in 1..rows {
            let y = body + j as f32 * row;
            self.course(
                m,
                &[
                    (y - 0.2, 0.02, 1.0),
                    (y - 0.12, 0.26, 1.0),
                    (y + 0.1, 0.26, 1.0),
                    (y + 0.2, 0.02, 1.0),
                ],
                y,
                trim,
            );
        }
        self.crown(m, glass, trim);
        if self.plinth == 0.0 {
            // Its belly, under it, and a thread of light round.
            let y = self.y0;
            self.course(
                m,
                &[
                    (y + 0.1, 0.3, 1.0),
                    (y - 0.15, 0.2, 0.98),
                    (y - 0.3, 0.0, 0.8),
                    (y - 0.32, 0.0, 0.0),
                ],
                y + 1.0,
                glass,
            );
            self.course(m, &[(y + 0.1, 0.3, 1.0), (y + 0.4, 0.04, 1.0)], y, trim);
            self.course(
                m,
                &[(y - 0.02, 0.29, 1.0), (y + 0.06, 0.33, 1.0)],
                y + 1.0,
                paint(PLASMA * 0.8, G),
            );
        }
        if rng.chance(0.6) {
            self.fins(m, rng.range(0.35, 0.6), rng.range(1.4, 2.8), glass);
        }
    }

    /// Drawn for far off: the same outline, fewer faces.
    pub fn draw_far(&self, map: &mut Map, rng: &mut Rng) {
        let m = &mut map.sculpted;
        let stone = paint(self.color, C);
        let glass = paint(GLASS * 0.7, V);
        let inside = self.axis((self.y0 + self.y1) * 0.5);
        let tall = self.y1 - 0.9 - self.y0 + 6.0;
        for f in 0..4 {
            let y = |v: f32| self.y0 - 6.0 + tall * v;
            sheet(
                m,
                (2, 6),
                |u, v| self.corner(f, u, y(v), 0.1, 1.0),
                inside,
                glass,
            );
            sheet(
                m,
                (1, 6),
                |u, v| {
                    let from = self.corner((f + 3) % 4, 1.0, y(v), 0.0, 1.0);
                    from.lerp(self.corner(f, 0.0, y(v), 0.0, 1.0), u)
                },
                inside,
                stone,
            );
            let (_, _, _, wide) = Self::face(f, self.half, self.round());
            let bays = (wide * 2.0 / 5.0).round().max(1.0) as usize;
            for i in 1..bays {
                let s = -wide + i as f32 * wide * 2.0 / bays as f32;
                self.rib(m, f, s, (self.y0, self.y1 - 0.9), 4, stone);
            }
            if rng.chance(0.6) {
                // A lit almond, long and thin.
                let i = rng.index(bays);
                let s = -wide + (i as f32 + 0.5) * wide * 2.0 / bays as f32;
                let (a, b) = (rng.range(0.15, 0.45), rng.range(0.55, 0.9));
                let h = self.y1 - self.y0;
                self.almond(
                    m,
                    f,
                    Vec2::new(s, self.y0 + h * (a + b) * 0.5),
                    Vec2::new(0.45, h * (b - a) * 0.5),
                    0.06,
                    paint(PLASMA * 0.8, G),
                );
            }
        }
        self.crown(m, glass, glass);
        if rng.chance(0.5) {
            let up = rng.range(2.0, 5.0);
            self.fins(m, 0.5, up, glass);
        }
    }

    fn crown(&self, m: &mut MeshData, glass: Vec4, under: Vec4) {
        let y = self.y1 - 0.9;
        self.course(
            m,
            &[(y - 0.15, 0.0, 1.0), (y + 0.15, 0.36, 1.0)],
            y + 1.0,
            under,
        );
        self.course(
            m,
            &[
                (y + 0.15, 0.36, 1.0),
                (y + 0.7, 0.42, 1.0),
                (y + 0.9, 0.2, 1.0),
                (y + 0.9, 0.0, 0.86),
            ],
            y,
            glass,
        );
        // A vault over it, steep over a bud.
        let r = self.half.min_element() * self.scale(self.y1);
        let up = r * if self.form == Form::Bud { 1.1 } else { 0.28 };
        let top = y + 0.9;
        let mut ring = (top, 0.0, 0.86);
        for k in 1..=4 {
            let t = k as f32 / 4.0;
            let next = (
                top + up * (t * FRAC_PI_2).sin(),
                0.0,
                0.86 * (1.0 - t).max(0.0),
            );
            self.course(m, &[ring, next], top - 0.5, glass);
            ring = next;
        }
    }

    /// A pilaster up face `f` at `s`.
    fn rib(&self, m: &mut MeshData, f: usize, s: f32, (a, b): (f32, f32), n: usize, c: Vec4) {
        let side = [(-0.22, -0.08), (-0.11, 0.2), (0.11, 0.2), (0.22, -0.08)];
        let inside = self.on(f, Vec2::new(s, (a + b) * 0.5), -1.0);
        for w in side.windows(2) {
            sheet(
                m,
                (1, n),
                |u, v| {
                    let x = w[0].0 + (w[1].0 - w[0].0) * u;
                    let out = w[0].1 + (w[1].1 - w[0].1) * u;
                    self.on(f, Vec2::new(s + x, a + (b - a) * v), out)
                },
                inside,
                c,
            );
        }
    }

    /// A blind bay: squared stones, laid breaking joint, their edges dressed
    /// back.
    fn stones(&self, m: &mut MeshData, rng: &mut Rng, f: usize, (a, b): (On, On)) {
        const EDGE: f32 = 0.035;
        let inside = self.on(f, (a + b) * 0.5, -3.0);
        let rows = ((b.y - a.y) / 1.05).round().max(1.0) as usize;
        let tall = (b.y - a.y) / rows as f32;
        for j in 0..rows {
            let wide = b.x - a.x;
            let cut = match j % 2 {
                0 => rng.range(0.34, 0.46),
                _ => rng.range(0.54, 0.66),
            };
            let ends: &[f32] = match wide < 1.1 {
                true => &[0.0, 1.0],
                false => &[0.0, cut, 1.0],
            };
            for w in ends.windows(2) {
                let y = a.y + j as f32 * tall;
                let lo = Vec2::new(a.x + wide * w[0], y);
                let hi = Vec2::new(a.x + wide * w[1], y + tall);
                let c = paint(self.color * SHADE * rng.range(0.84, 1.1), C);
                let out = rng.range(0.0, 0.05);
                let at = |k: usize, d: f32, out: f32| {
                    let q = match k % 4 {
                        0 => lo + d,
                        1 => Vec2::new(hi.x - d, lo.y + d),
                        2 => hi - d,
                        _ => Vec2::new(lo.x + d, hi.y - d),
                    };
                    self.on(f, q, out)
                };
                for k in 0..4 {
                    sheet(
                        m,
                        (1, 1),
                        |u, v| {
                            let (d, o) = (EDGE * v, -0.035 + (out + 0.035) * v);
                            at(k, d, o).lerp(at(k + 1, d, o), u)
                        },
                        inside,
                        c,
                    );
                }
                sheet(
                    m,
                    (1, 1),
                    |u, v| {
                        let low = at(0, EDGE, out).lerp(at(1, EDGE, out), u);
                        low.lerp(at(3, EDGE, out).lerp(at(2, EDGE, out), u), v)
                    },
                    inside,
                    c,
                );
            }
        }
    }

    /// An almond, pointed top and bottom, flat on face `f`.
    fn almond(&self, m: &mut MeshData, f: usize, at: On, half: Vec2, out: f32, c: Vec4) {
        sheet(
            m,
            (1, 8),
            |u, v| {
                let w = (PI * v).sin();
                let p = Vec2::new((u * 2.0 - 1.0) * w, -(PI * v).cos());
                self.on(f, at + p * half, out)
            },
            self.on(f, at, out - 1.0),
            c,
        );
    }

    /// A bay with an almond window sunk in it: a glass frame standing proud,
    /// a deep reveal, dark glass, and plasma in the middle.
    fn window(&self, m: &mut MeshData, f: usize, (a, b): (On, On), face: Vec4, glass: Vec4) {
        let mid = (a + b) * 0.5;
        let half = (b - a) * 0.5;
        let hole = Vec2::new(half.x * 0.27, half.y * 0.84);
        let inside = self.on(f, mid, -3.0);
        // Round the hole: out to the edge of the bay, or on the almond.
        let almond = |t: f32, grow: f32| {
            let (s, c) = t.sin_cos();
            mid + Vec2::new(s.abs() * s * (hole.x + grow), c * (hole.y + grow * 1.5))
        };
        let square = |t: f32| {
            let (s, c) = t.sin_cos();
            mid + half * Vec2::new(s, c) / s.abs().max(c.abs())
        };
        let mut ring = |from: &dyn Fn(f32) -> On,
                        to: &dyn Fn(f32) -> On,
                        (p, q): (f32, f32),
                        c,
                        flip: bool| {
            for k in 0..8 {
                let t = (k as f32 + 0.5) / 8.0 * TAU;
                let core = match flip {
                    true => self.on(f, mid + (from(t) - mid) * 5.0, 0.0),
                    false => inside,
                };
                sheet(
                    m,
                    (2, 1),
                    |u, v| {
                        let t = (k as f32 + u) / 8.0 * TAU;
                        self.on(f, from(t).lerp(to(t), v), p + (q - p) * v)
                    },
                    core,
                    c,
                );
            }
        };
        ring(&square, &|t| almond(t, 0.14), (0.0, 0.0), face, false);
        ring(
            &|t| almond(t, 0.16),
            &|t| almond(t, 0.1),
            (-0.02, 0.12),
            glass,
            false,
        );
        ring(
            &|t| almond(t, 0.1),
            &|t| almond(t, 0.0),
            (0.12, 0.12),
            glass,
            false,
        );
        ring(
            &|t| almond(t, 0.0),
            &|t| almond(t, -0.06),
            (0.12, -0.24),
            glass,
            true,
        );
        self.almond(m, f, mid, hole, -0.22, paint(GLASS * 0.8, V));
        self.almond(
            m,
            f,
            mid,
            hole * Vec2::new(0.5, 0.85),
            -0.12,
            paint(PLASMA * 0.85, G),
        );
    }

    /// Blades of glass up its corners, from `from` of the way up to `up` over
    /// its crown, bowed out and leaning in.
    fn fins(&self, m: &mut MeshData, from: f32, up: f32, c: Vec4) {
        let a = self.y0 + (self.y1 - self.y0) * from;
        let b = self.y1 + up;
        for f in 0..4 {
            let at = |v: f32, off: f32, out: f32| {
                let y = a + (b - a) * v;
                let thick = (PI * v.max(0.02)).sin().max(0.0).powf(0.6);
                let bow = 0.1 + 0.55 * (PI * v * 0.85).sin() - 0.9 * (v - 0.75).max(0.0) * up;
                self.corner(f, 0.5 + off * thick, y, bow + out * thick, 1.0)
            };
            let mid = at(0.5, 0.0, 0.0);
            // Its two cheeks, meeting in an edge.
            for e in [-0.3f32, 0.3] {
                sheet(
                    m,
                    (1, 8),
                    |u, v| at(v, e * (1.0 - u), 0.45 * u - 0.25),
                    mid,
                    c,
                );
            }
            sheet(
                m,
                (1, 8),
                |u, v| at(v, 0.6 * u - 0.3, -0.25),
                mid + (mid - self.axis(mid.y)),
                c,
            );
        }
    }
}

/// A wall swept round an arc about `center`, from angle `a0` to `a1`, its
/// height at each.
pub(super) struct Curve {
    pub center: Vec2,
    pub r: f32,
    pub thick: f32,
    pub a0: f32,
    pub a1: f32,
    pub color: Vec3,
    /// An almond of plasma in every `lit`th bay of its inner face; 0: none.
    pub lit: usize,
}

impl Curve {
    pub fn draw(&self, map: &mut Map, height: impl Fn(f32) -> f32) {
        let m = &mut map.sculpted;
        let stone = paint(self.color * SHADE, C);
        let trim = paint(self.color * SHADE * 0.72, C);
        let glass = paint(GLASS * 0.55, V);
        let bays = (((self.a1 - self.a0) * self.r) / 2.6).round().max(1.0) as usize;
        let n = bays * 3;
        let at = |u: f32, off: f32, y: f32| {
            let d = dir(self.a0 + (self.a1 - self.a0) * u);
            let p = self.center + d * (self.r + off);
            Vec3::new(p.x, y, p.y)
        };
        let h = |u: f32| height(self.a0 + (self.a1 - self.a0) * u);
        let t = self.thick;
        // Its section, from the foot outside over the top and down: how far
        // off the middle, and how high, over so much of its height.
        let section: [((f32, f32, f32), Vec4); 14] = [
            ((t + 0.25, -0.5, 0.0), trim),
            ((t + 0.25, 0.6, 0.0), trim),
            ((t, 0.85, 0.0), stone),
            ((t, 0.0, 0.5), trim),
            ((t + 0.22, 0.1, 0.5), trim),
            ((t + 0.22, 0.35, 0.5), trim),
            ((t, 0.45, 0.5), stone),
            ((t, -0.3, 1.0), trim),
            ((t + 0.28, -0.12, 1.0), glass),
            ((t + 0.28, 0.22, 1.0), glass),
            ((0.0, 0.6, 1.0), glass),
            ((-t - 0.28, 0.22, 1.0), glass),
            ((-t - 0.28, -0.12, 1.0), trim),
            ((-t, -0.3, 1.0), stone),
        ];
        let mut line: Vec<((f32, f32, f32), Vec4)> = section.to_vec();
        line.push(((-t, 0.85, 0.0), trim));
        line.push(((-t - 0.25, 0.6, 0.0), trim));
        line.push(((-t - 0.25, -0.5, 0.0), trim));
        let y = |u: f32, (_, y, k): (f32, f32, f32)| h(u) * k + y;
        for w in line.windows(2) {
            let (p, q) = (w[0].0, w[1].0);
            sheet(
                m,
                (n, 1),
                |u, v| {
                    let off = p.0 + (q.0 - p.0) * v;
                    let (a, b) = (y(u, p), y(u, q));
                    at(u, off, a + (b - a) * v)
                },
                at(0.5, 0.0, h(0.5) * 0.5),
                w[0].1,
            );
        }
        // (Facing out of each strip: a wall's bent, so its middle isn't
        // inside it.)
        let strip = (n as u32 + 1) * 2;
        let first = m.verts.len() as u32 - strip * (line.len() as u32 - 1);
        for (k, w) in line.windows(2).enumerate() {
            let (p, q) = (w[0].0, w[1].0);
            for j in 0..2u32 {
                for i in 0..=n as u32 {
                    let u = i as f32 / n as f32;
                    let d = dir(self.a0 + (self.a1 - self.a0) * u);
                    let (dx, dy) = (q.0 - p.0, y(u, q) - y(u, p));
                    // The section's walked up outside and down inside.
                    let nrm = Vec2::new(dy, -dx).normalize_or(Vec2::Y);
                    let v =
                        &mut m.verts[(first + strip * k as u32 + j * (n as u32 + 1) + i) as usize];
                    v.normal = Vec3::new(d.x * nrm.x, nrm.y, d.y * nrm.x).to_array();
                }
            }
        }
        // Closed at its ends.
        for e in [0.0f32, 1.0] {
            let mid = at(e, 0.0, h(e) * 0.5);
            let pts: Vec<Vec3> = line.iter().map(|l| at(e, l.0.0, y(e, l.0))).collect();
            let back = at(0.5 + (e - 0.5) * 0.9, 0.0, h(e) * 0.5);
            for w in pts.windows(2) {
                m.facet([w[0], w[1], mid], back, trim);
            }
            m.facet([pts[pts.len() - 1], pts[0], mid], back, trim);
        }
        // Piers between the bays, and the light in them.
        let half = 0.26 / (self.a1 - self.a0) / self.r;
        for k in 0..=bays {
            let u = (k as f32 / bays as f32).clamp(half * 1.2, 1.0 - half * 1.2);
            for e in [-1.0f32, 1.0] {
                let side = [
                    (-half, -0.02),
                    (-half * 0.6, 0.18),
                    (half * 0.6, 0.18),
                    (half, -0.02),
                ];
                for w in side.windows(2) {
                    let core = at(u, 0.0, h(u) * 0.5);
                    sheet(
                        m,
                        (1, 1),
                        |a, v| {
                            let s = w[0].0 + (w[1].0 - w[0].0) * a;
                            let out = w[0].1 + (w[1].1 - w[0].1) * a;
                            at(u + s, e * (t + out), 0.6 + (h(u + s) - 0.8) * v)
                        },
                        core,
                        trim,
                    );
                }
            }
            if k < bays && self.lit > 0 && k % self.lit == 0 {
                let u = (k as f32 + 0.5) / bays as f32;
                let (lo, hi) = (1.1, h(u) - 0.6);
                if hi - lo < 1.2 {
                    continue;
                }
                let wide = 0.5 / (self.a1 - self.a0) / self.r;
                for (grow, off, c) in [
                    (1.0, 0.04, paint(GLASS * 0.8, V)),
                    (0.42, 0.07, paint(PLASMA * 0.85, G)),
                ] {
                    let tall = (hi - lo) * 0.5 * (0.6 + 0.4 * grow);
                    sheet(
                        m,
                        (1, 8),
                        |a, v| {
                            let w = (PI * v).sin() * wide * grow;
                            let y = (lo + hi) * 0.5 - (PI * v).cos() * tall;
                            at(u + (a * 2.0 - 1.0) * w, -t - off, y)
                        },
                        at(u, 0.0, (lo + hi) * 0.5),
                        c,
                    );
                }
            }
        }
    }
}

/// A round column, waisted, `r` at its ends, plasma up four sides.
pub(super) fn column(map: &mut Map, at: Vec2, r: f32, (y0, y1): (f32, f32), color: Vec3) {
    let m = &mut map.sculpted;
    let wide = |v: f32| r * (1.0 - 0.34 * (PI * v).sin());
    let p = |u: f32, v: f32, out: f32| {
        let q = at + dir(u * TAU) * (wide(v) + out);
        Vec3::new(q.x, y0 + (y1 - y0) * v, q.y)
    };
    let inside = Vec3::new(at.x, (y0 + y1) * 0.5, at.y);
    let stone = paint(color * SHADE, C);
    sheet(m, (24, 10), |u, v| p(u, v, 0.0), inside, stone);
    for k in 0..4 {
        let u0 = k as f32 / 4.0;
        for (w, out, c) in [
            (0.05, 0.03, paint(GLASS, V)),
            (0.02, 0.06, paint(PLASMA * 0.85, G)),
        ] {
            sheet(
                m,
                (1, 8),
                |u, v| {
                    let s = (PI * v).sin() * w;
                    p(u0 + (u * 2.0 - 1.0) * s, 0.1 + 0.8 * v, out)
                },
                inside,
                c,
            );
        }
    }
}

/// A roof shaped like a lens, in `o`'s frame: `w` wide at its middle, `l`
/// half its length, over `y`, a seam of light along its underside.
pub(super) fn lens_vault(map: &mut Map, o: &OBox, (w, l): (f32, f32), y: f32, color: Vec3) {
    let (ax, az) = o.axes();
    let wide = |t: f32| w * (1.0 - 0.42 * t * t);
    let p = |x: f32, t: f32, y: f32| {
        let q = o.center + ax * x + az * (t * l);
        Vec3::new(q.x, y, q.y)
    };
    let m = &mut map.sculpted;
    let mid = p(0.0, 0.0, y + 0.4);
    let (stone, glass) = (paint(color * SHADE, C), paint(GLASS * 0.7, V));
    // Across it: where (of its width), how high, and what of.
    let line = [
        (0.0f32, 0.1, stone),
        (0.7, 0.0, stone),
        (0.97, 0.05, glass),
        (1.0, 0.45, glass),
        (0.95, 0.85, stone),
        (0.6, 1.3, glass),
        (0.0, 1.9, glass),
    ];
    for e in [-1.0f32, 1.0] {
        for k in line.windows(2) {
            let (a, b) = (k[0], k[1]);
            sheet(
                m,
                (16, 1),
                |u, v| {
                    let t = u * 2.0 - 1.0;
                    let x = a.0 + (b.0 - a.0) * v;
                    // (Its crown comes down toward the ends.)
                    let h = a.1 + (b.1 - a.1) * v;
                    let h = if h > 0.85 {
                        0.85 + (h - 0.85) * (1.0 - t * t)
                    } else {
                        h
                    };
                    p(e * x * wide(t), t, y + h)
                },
                mid,
                a.2,
            );
        }
        for q in line.windows(2) {
            let end = |k: (f32, f32, Vec4)| p(k.0 * wide(1.0), e, y + k.1.min(0.85));
            let mirror = |k: (f32, f32, Vec4)| p(-k.0 * wide(1.0), e, y + k.1.min(0.85));
            m.facet([end(q[0]), end(q[1]), p(0.0, e, y + 0.45)], mid, glass);
            m.facet(
                [mirror(q[0]), mirror(q[1]), p(0.0, e, y + 0.45)],
                mid,
                glass,
            );
        }
    }
    sheet(
        m,
        (1, 16),
        |u, v| {
            let t = v * 2.0 - 1.0;
            let x = (u * 2.0 - 1.0) * 0.5 * (PI * v).sin();
            p(x, t * 0.85, y + 0.02)
        },
        mid,
        paint(PLASMA * 0.9, G),
    );
    // Ribs over it, out to its edges.
    let n = (l / 2.2) as i32;
    for k in -n..=n {
        let t = k as f32 / (n as f32 + 0.6);
        for e in [-1.0f32, 1.0] {
            sheet(
                m,
                (1, 4),
                |u, v| {
                    let s = t + (u * 2.0 - 1.0) * 0.16 / l;
                    let x = 0.97 * v;
                    let h = if x < 0.6 {
                        1.9 - 0.6 * x / 0.6
                    } else {
                        1.3 - 0.45 * (x - 0.6) / 0.35
                    };
                    let h = 0.85 + (h - 0.85).max(0.0) * (1.0 - s * s);
                    p(e * x * wide(s), s, y + h + 0.06)
                },
                mid,
                glass,
            );
        }
    }
}
