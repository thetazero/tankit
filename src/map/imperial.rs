//! Imperium city blocks (the imperial city's edge), dressed as Imperium architecture: stone
//! plinths with arcades at street level, a regular order of windows above
//! (ribbons, a punched grid, or a colossal order of pilasters), a heavy
//! cornice and attic on top, and the Imperium's crimson and gold on the
//! buildings that face the avenues. Each block gets its own finish (raw
//! concrete, travertine, limestone, basalt, ochre or ox-blood render), so
//! the city reads as many buildings, not one box repeated.
//!
//! Blocks here are axis aligned (yaw 0); the brutalist slabs, which turn
//! with their lot, borrow the finishes.

use glam::{Vec2, Vec3};

use super::{Map, OBox};
use crate::render::material::Material;
use crate::rng::Rng;

/// A building's finish: the body, and the darker (or lighter) stone of its
/// plinth, cornice and string courses.
#[derive(Clone, Copy)]
pub(super) struct Finish {
    pub body: Vec3,
    pub trim: Vec3,
}

/// The finishes, weighted toward the plain ones (later entries are rarer).
const FINISHES: [Finish; 8] = [
    // Raw grey concrete on a dark base.
    Finish {
        body: Vec3::new(0.42, 0.42, 0.41),
        trim: Vec3::new(0.25, 0.25, 0.26),
    },
    // Travertine.
    Finish {
        body: Vec3::new(0.53, 0.46, 0.37),
        trim: Vec3::new(0.38, 0.33, 0.27),
    },
    // Pale limestone on granite.
    Finish {
        body: Vec3::new(0.56, 0.53, 0.48),
        trim: Vec3::new(0.32, 0.31, 0.3),
    },
    // Brown, rain-stained concrete.
    Finish {
        body: Vec3::new(0.4, 0.34, 0.29),
        trim: Vec3::new(0.24, 0.22, 0.2),
    },
    // Grey concrete again: the commonest.
    Finish {
        body: Vec3::new(0.48, 0.47, 0.45),
        trim: Vec3::new(0.3, 0.29, 0.28),
    },
    // Dark basalt with pale stone dressings.
    Finish {
        body: Vec3::new(0.23, 0.23, 0.25),
        trim: Vec3::new(0.56, 0.52, 0.45),
    },
    // Ochre render.
    Finish {
        body: Vec3::new(0.56, 0.4, 0.26),
        trim: Vec3::new(0.3, 0.27, 0.24),
    },
    // Ox-blood render: the ministries.
    Finish {
        body: Vec3::new(0.38, 0.16, 0.12),
        trim: Vec3::new(0.6, 0.55, 0.46),
    },
];

/// Glass and the dark behind it.
const GLASS: Vec3 = Vec3::new(0.1, 0.11, 0.13);
/// Paint for the friezes: a deep crimson that reads in shade.
const FRIEZE: Vec3 = Vec3::new(0.42, 0.06, 0.06);
const GOLD: Vec3 = Vec3::new(0.86, 0.66, 0.28);

/// Storey height.
const FLOOR: f32 = 3.6;

impl Finish {
    /// A finish, the plain ones most often.
    pub fn pick(rng: &mut Rng) -> Finish {
        // Squaring biases toward the start of the list.
        let r = rng.f32();
        let f = FINISHES[((r * r) * FINISHES.len() as f32) as usize % FINISHES.len()];
        let k = rng.range(0.92, 1.06);
        Finish {
            body: f.body * k,
            trim: f.trim * k,
        }
    }
}

/// How the windows are laid out on a facade.
#[derive(Clone, Copy, PartialEq)]
enum Order {
    /// A dark ribbon of glass round each floor.
    Ribbon,
    /// Ribbons crossed by pilasters: a grid of punched windows.
    Grid,
    /// Tall slots of glass between pilasters running the full height,
    /// with a spandrel band every two floors: the state style.
    Colossal,
}

/// An axis-aligned city block to build.
pub(super) struct Block {
    pub center: Vec2,
    pub half: Vec2,
    pub y0: f32,
    /// Height of the roof (before the cornice and any attic).
    pub h: f32,
    pub solid: bool,
    /// Shelled: the top is ragged and there's no cornice.
    pub broken: bool,
    /// Which faces to dress: +x, -x, +z, -z.
    pub faces: [bool; 4],
    /// Street level is close enough to see (arcades, roof clutter).
    pub near: bool,
    /// Banners on the dressed faces (they flap: they make a sound each).
    pub banners: bool,
}

/// The four faces: normal out of the block.
const FACES: [Vec2; 4] = [
    Vec2::new(1.0, 0.0),
    Vec2::new(-1.0, 0.0),
    Vec2::new(0.0, 1.0),
    Vec2::new(0.0, -1.0),
];

impl Map {
    /// A visual-only axis-aligned box from `lo` to `hi`.
    fn trim_box(&mut self, lo: Vec3, hi: Vec3, color: Vec3, mat: Material) {
        let c = (lo + hi) * 0.5;
        self.oboxes.push(OBox {
            center: Vec2::new(c.x, c.z),
            half: Vec2::new(hi.x - lo.x, hi.z - lo.z).abs() * 0.5,
            y0: lo.y.min(hi.y),
            y1: lo.y.max(hi.y),
            color,
            side_mat: mat,
            top_mat: mat,
            solid: false,
            ..OBox::default()
        });
    }

    /// Build `b`, dressed. Returns its finish.
    pub(super) fn imperial_block(&mut self, rng: &mut Rng, b: &Block) -> Finish {
        let fin = Finish::pick(rng);
        let order = match rng.f32() {
            x if x < 0.3 => Order::Ribbon,
            x if x < 0.7 => Order::Grid,
            _ => Order::Colossal,
        };
        let body = OBox {
            center: b.center,
            half: b.half,
            y0: b.y0,
            y1: b.h,
            color: fin.body,
            side_mat: Material::Concrete,
            top_mat: Material::Concrete,
            solid: b.solid,
            chip: if b.solid { 0.25 } else { 0.0 },
            ..OBox::default()
        };
        self.oboxes.push(body);
        if b.broken {
            self.ragged_top(rng, &body);
        } else {
            // The cornice: a heavy cap stepping out past the walls.
            let out = 0.45;
            self.trim_box(
                Vec3::new(
                    b.center.x - b.half.x - out,
                    b.h - 0.8,
                    b.center.y - b.half.y - out,
                ),
                Vec3::new(
                    b.center.x + b.half.x + out,
                    b.h + 0.1,
                    b.center.y + b.half.y + out,
                ),
                fin.trim,
                Material::Concrete,
            );
            // An attic storey set back on the roof, sometimes.
            if rng.chance(0.45) && b.half.min_element() > 5.0 {
                let inset = rng.range(1.5, 3.0);
                let ah = rng.range(2.2, 3.6);
                self.trim_box(
                    Vec3::new(
                        b.center.x - b.half.x + inset,
                        b.h,
                        b.center.y - b.half.y + inset,
                    ),
                    Vec3::new(
                        b.center.x + b.half.x - inset,
                        b.h + ah,
                        b.center.y + b.half.y - inset,
                    ),
                    fin.body * 0.94,
                    Material::Concrete,
                );
                self.trim_box(
                    Vec3::new(
                        b.center.x - b.half.x + inset - 0.25,
                        b.h + ah - 0.4,
                        b.center.y - b.half.y + inset - 0.25,
                    ),
                    Vec3::new(
                        b.center.x + b.half.x - inset + 0.25,
                        b.h + ah + 0.05,
                        b.center.y + b.half.y - inset + 0.25,
                    ),
                    fin.trim,
                    Material::Concrete,
                );
            } else if b.near && b.half.min_element() > 3.0 {
                let mut crng = Rng::new(
                    ((b.center.x * 911.0) as i64 as u64)
                        ^ ((b.center.y * 577.0) as i64 as u64) << 20,
                );
                for _ in 0..crng.int(1, 2) {
                    let q = Vec2::new(crng.range(-0.5, 0.5), crng.range(-0.5, 0.5)) * b.half;
                    let yaw = crng.range(0.0, std::f32::consts::TAU);
                    self.roof_clutter(
                        &mut crng,
                        Vec3::new(b.center.x + q.x, b.h + 0.1, b.center.y + q.y),
                        yaw,
                        1.0,
                    );
                }
            }
        }
        // Ragged tops lose their top floor of windows and trim.
        let top = if b.broken { b.h - 2.5 } else { b.h - 0.8 };
        let plinth = (b.y0 + 4.6).min(b.h - 2.0);
        let frieze = !b.broken && rng.chance(0.3);
        let emblem = !b.broken && rng.chance(0.25);
        for (k, &n) in FACES.iter().enumerate() {
            if !b.faces[k] {
                continue;
            }
            let along = Vec2::new(n.y.abs(), n.x.abs());
            let half_along = b.half.dot(along);
            let depth = b.half.dot(n.abs());
            // A box on this face: from s0 to s1 along it, y0 to y1, standing
            // out from the wall between d0 and d1.
            let face = |s0: f32, s1: f32, d0: f32, d1: f32| {
                let c = b.center + n * (depth + (d0 + d1) * 0.5) + along * (s0 + s1) * 0.5;
                let h = along * (s1 - s0) * 0.5 + n.abs() * (d1 - d0) * 0.5;
                (c, h)
            };
            let put = |map: &mut Map, s: (f32, f32), y: (f32, f32), d: (f32, f32), color: Vec3| {
                let (c, h) = face(s.0, s.1, d.0, d.1);
                map.trim_box(
                    Vec3::new(c.x - h.x, y.0, c.y - h.y),
                    Vec3::new(c.x + h.x, y.1, c.y + h.y),
                    color,
                    Material::Concrete,
                );
            };
            let l = half_along;
            // The plinth: rusticated stone up to the first floor, with a
            // string course over it.
            put(
                self,
                (-l - 0.3, l + 0.3),
                (b.y0, plinth),
                (-0.15, 0.3),
                fin.trim,
            );
            put(
                self,
                (-l - 0.4, l + 0.4),
                (plinth, plinth + 0.35),
                (-0.18, 0.42),
                fin.trim * 0.9,
            );
            // Bays along the face.
            let bays = ((2.0 * l / 3.4).round() as usize).max(1);
            let bw = 2.0 * l / bays as f32;
            if b.near {
                // An arcade: a dark opening in each bay but the ends.
                for i in 1..bays.saturating_sub(1) {
                    let s = -l + (i as f32 + 0.5) * bw;
                    put(
                        self,
                        (s - bw * 0.3, s + bw * 0.3),
                        (b.y0.max(0.0), plinth - 0.9),
                        (0.3, 0.34),
                        GLASS * 0.7,
                    );
                }
            }
            // The windows.
            let w0 = plinth + 0.35;
            if top > w0 + 2.0 {
                match order {
                    Order::Ribbon | Order::Grid => {
                        let mut y = w0 + 1.0;
                        while y + 1.4 < top {
                            put(
                                self,
                                (-l + 0.8, l - 0.8),
                                (y, y + 1.4),
                                (-0.03, 0.06),
                                GLASS,
                            );
                            if order == Order::Ribbon {
                                // A sill under each ribbon.
                                put(
                                    self,
                                    (-l + 0.6, l - 0.6),
                                    (y - 0.22, y),
                                    (-0.1, 0.18),
                                    fin.trim,
                                );
                            }
                            y += FLOOR;
                        }
                        if order == Order::Grid {
                            for i in 0..=bays {
                                let s = -l + i as f32 * bw;
                                let w = if i == 0 || i == bays { 0.9 } else { 0.4 };
                                let s0 = (s - w).max(-l - 0.05);
                                let s1 = (s + w).min(l + 0.05);
                                put(self, (s0, s1), (w0, top), (-0.09, 0.2), fin.body * 0.96);
                            }
                        }
                    }
                    Order::Colossal => {
                        for i in 0..bays {
                            let s = -l + (i as f32 + 0.5) * bw;
                            put(
                                self,
                                (s - bw * 0.22, s + bw * 0.22),
                                (w0 + 0.8, top - 0.6),
                                (-0.03, 0.06),
                                GLASS,
                            );
                        }
                        // Pilasters between the slots, standing proud.
                        for i in 0..=bays {
                            let s = -l + i as f32 * bw;
                            let s0 = (s - bw * 0.2).max(-l - 0.05);
                            let s1 = (s + bw * 0.2).min(l + 0.05);
                            put(self, (s0, s1), (w0, top), (-0.09, 0.35), fin.body * 1.04);
                        }
                        // Spandrels every two floors.
                        let mut y = w0 + 2.0 * FLOOR;
                        while y + 2.0 < top {
                            put(
                                self,
                                (-l - 0.015, l + 0.015),
                                (y, y + 0.5),
                                (-0.12, 0.2),
                                fin.body * 0.9,
                            );
                            y += 2.0 * FLOOR;
                        }
                    }
                }
            }
            // A crimson frieze under the cornice.
            if frieze && top > w0 + 3.0 {
                put(
                    self,
                    (-l - 0.06, l + 0.06),
                    (top - 1.3, top - 0.25),
                    (-0.21, 0.1),
                    FRIEZE,
                );
            }
            if emblem && top > w0 + 5.0 {
                let c = b.center + n * (depth + 0.45);
                self.disc(
                    Vec3::new(c.x, top - 2.6, c.y),
                    Vec3::new(n.x, 0.0, n.y) * 0.12,
                    1.3,
                    GOLD,
                    Material::Metal,
                );
            }
            if b.banners && top > w0 + 6.0 && rng.chance(0.4) {
                let s = if bays > 2 {
                    rng.range(-0.5, 0.5) * l
                } else {
                    0.0
                };
                let c = b.center + n * (depth + 0.45) + along * s;
                self.banner(
                    Vec3::new(c.x, top - 0.4, c.y),
                    n,
                    2.6,
                    ((top - w0) * 0.75).min(16.0),
                );
            }
        }
        fin
    }

    /// A tower out in the skyline (visual only): stepped back in tiers, or a
    /// slab, or a tower on a podium, windows all over (the `Wall` grid) and a
    /// cornice on every tier.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn skyline_tower(
        &mut self,
        rng: &mut Rng,
        p: Vec2,
        half: Vec2,
        h: f32,
        broken: bool,
        shade: f32,
        toward: Vec2,
    ) {
        let fin = Finish::pick(rng);
        let (body, trim) = (fin.body * shade, fin.trim * shade);
        // Tiers: (half, from, to).
        let mut tiers: Vec<(Vec2, f32, f32)> = Vec::new();
        match rng.f32() {
            // A ziggurat of setbacks.
            x if x < 0.4 && h > 18.0 => {
                let n = if h > 34.0 { 3 } else { 2 };
                let mut y = 0.0;
                let mut hh = half;
                for k in 0..n {
                    let to = if k + 1 == n {
                        h
                    } else {
                        y + h * rng.range(0.4, 0.55) / (k + 1) as f32
                    };
                    tiers.push((hh, y, to));
                    y = to;
                    hh = (hh - Vec2::splat(rng.range(1.8, 3.2))).max(Vec2::splat(3.0));
                }
            }
            // A tower on a podium.
            x if x < 0.65 && h > 22.0 => {
                let ph = rng.range(7.0, 11.0);
                tiers.push((half, 0.0, ph));
                tiers.push((half * rng.range(0.45, 0.65), ph, h));
            }
            _ => tiers.push((half, 0.0, h)),
        }
        let last = tiers.len() - 1;
        for (k, &(hh, y0, y1)) in tiers.iter().enumerate() {
            let tier = OBox {
                center: p,
                half: hh,
                y0,
                y1,
                color: body,
                side_mat: Material::Wall,
                top_mat: Material::Concrete,
                solid: false,
                ..OBox::default()
            };
            self.oboxes.push(tier);
            if k == last && broken {
                self.ragged_top(
                    rng,
                    &OBox {
                        side_mat: Material::Concrete,
                        ..tier
                    },
                );
                continue;
            }
            // Cornice.
            self.trim_box(
                Vec3::new(p.x - hh.x - 0.5, y1 - 0.9, p.y - hh.y - 0.5),
                Vec3::new(p.x + hh.x + 0.5, y1 + 0.1, p.y + hh.y + 0.5),
                trim,
                Material::Concrete,
            );
        }
        // A plinth of dark stone.
        self.trim_box(
            Vec3::new(p.x - half.x - 0.3, 0.0, p.y - half.y - 0.3),
            Vec3::new(p.x + half.x + 0.3, 5.0, p.y + half.y + 0.3),
            trim * 0.9,
            Material::Concrete,
        );
        if broken {
            return;
        }
        let (th, _, top) = tiers[last];
        // On the tallest: a mast with the Imperium's red at the top.
        if top > 40.0 && rng.chance(0.4) {
            let mh = rng.range(6.0, 14.0);
            self.trim_box(
                Vec3::new(p.x - 0.3, top, p.y - 0.3),
                Vec3::new(p.x + 0.3, top + mh, p.y + 0.3),
                Vec3::splat(0.2),
                Material::Metal,
            );
        }
        // A crimson band down the face toward the city, on a few.
        if rng.chance(0.18) {
            let (n, len) = if toward.x.abs() > 0.0 && rng.chance(0.5) {
                (Vec2::new(toward.x, 0.0), th.y)
            } else {
                (Vec2::new(0.0, toward.y), th.x)
            };
            let along = Vec2::new(n.y.abs(), n.x.abs());
            let depth = th.dot(n.abs());
            let w = (len * 0.3).min(4.0);
            let c = p + n * (depth + 0.1);
            let ext = along * w + n.abs() * 0.08;
            let y0 = tiers[last].1.max(top * 0.35);
            self.trim_box(
                Vec3::new(c.x - ext.x, y0, c.y - ext.y),
                Vec3::new(c.x + ext.x, top - 1.4, c.y + ext.y),
                FRIEZE * 1.1,
                Material::Cloth,
            );
        }
    }
}
