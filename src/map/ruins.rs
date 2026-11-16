//! Ruined buildings: built as real structures first, then damaged.
//!
//! Construction: a concrete frame with a tall ground floor (shopfronts) and
//! upper storeys; walls made of piers, sills and lintels around real window
//! and door openings; floor slabs; corner columns; a parapet.
//!
//! Damage: a "bite" that takes out part of the building from one direction,
//! so the remaining storeys step down toward it. Walls end in broken piers,
//! slab edges go ragged, a collapsed floor section can slope from the first
//! floor down to the street (a drivable ramp), and rubble heaps up at the
//! foot of the damage (a terrain mound plus loose debris).
//!
//! Everything is built in the lot's local frame: x along the street frontage,
//! z into the depth, y up.

use crate::render::material::Material;
use glam::{Vec2, Vec3};

use super::{Map, OBox};
use crate::rng::Rng;

/// Ground floor is tall (shopfronts a tank fits through); upper storeys less so.
const GROUND_H: f32 = 3.8;
/// Height of the first-floor slab (top of a collapsed-floor ramp).
#[cfg(test)]
pub(super) const FIRST_FLOOR: f32 = GROUND_H;
const STOREY_H: f32 = 3.1;
const SLAB: f32 = 0.3;
const WALL_T: f32 = 0.45;
/// Target bay width along a wall: one opening per bay.
const BAY: f32 = 3.4;
const PIER: f32 = 0.55;
/// Width of a collapsed-floor ramp.
const RAMP_W: f32 = 4.5;

const PLASTERS: [Vec3; 9] = [
    Vec3::new(0.80, 0.70, 0.52), // ochre
    Vec3::new(0.76, 0.74, 0.69), // pale grey
    Vec3::new(0.72, 0.58, 0.44), // sandstone
    Vec3::new(0.86, 0.82, 0.72), // bleached cream
    Vec3::new(0.66, 0.62, 0.57), // dust grey
    Vec3::new(0.74, 0.47, 0.36), // faded terracotta
    Vec3::new(0.52, 0.64, 0.66), // faded teal
    Vec3::new(0.58, 0.62, 0.72), // faded blue
    Vec3::new(0.80, 0.62, 0.42), // apricot
];
const SLAB_C: Vec3 = Vec3::new(0.6, 0.58, 0.54);
const RUBBLE_C: Vec3 = Vec3::new(0.62, 0.57, 0.5);

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A sand or rubble heap banked against one face of a footprint, raising the terrain.
#[derive(Clone, Copy)]
pub(super) struct Drift {
    pub b: OBox,
    /// Which face: local axis (0 = x, 1 = z) and side.
    pub axis: usize,
    pub sign: f32,
    pub height: f32,
    pub run: f32,
}

impl Drift {
    pub fn height_at(&self, p: Vec2) -> f32 {
        let q = self.b.local_point(p);
        let (along, across, half_along, half_across) = if self.axis == 0 {
            (q.x, q.y, self.b.half.x, self.b.half.y)
        } else {
            (q.y, q.x, self.b.half.y, self.b.half.x)
        };
        let d = along * self.sign - half_along;
        if d < -0.5 || d > self.run {
            return 0.0;
        }
        // Smooth all round: rolls in from the street and tapers off past the
        // building's corners over a distance like its run, so it never makes
        // a ramp-ended kicker for tanks driving along the street.
        let taper = self.run.max(4.0) * 0.6;
        let lateral = 1.0 - smoothstep(half_across * 0.5, half_across + taper, across.abs());
        let out = 1.0 - smoothstep(0.0, self.run, d.max(0.0));
        self.height * out * lateral
    }
}

/// Places boxes in a lot's local frame.
struct Frame<'a> {
    map: &'a mut Map,
    lot: OBox,
}

impl Frame<'_> {
    /// Box from local `lo` to `hi` (x, y, z).
    fn put(&mut self, lo: Vec3, hi: Vec3, color: Vec3, solid: bool, material: Material) {
        let (ax, az) = self.lot.axes();
        let mid = (lo + hi) * 0.5;
        self.map.oboxes.push(OBox {
            center: self.lot.center + ax * mid.x + az * mid.z,
            yaw: self.lot.yaw,
            half: Vec2::new((hi.x - lo.x) * 0.5, (hi.z - lo.z) * 0.5),
            y0: lo.y,
            y1: hi.y,
            color,
            side_mat: material,
            top_mat: material,
            tilt: 0.0,
            solid,
            // Masonry and concrete pieces get knocked edges (visual only).
            chip: if material == Material::Plaster || material == Material::Concrete {
                0.22
            } else {
                0.0
            },
            broken: false,
            ramp: false,
        });
    }

    /// A piece whose top is a broken edge: drawn jagged, with rebar sticking
    /// out of it where it's long enough to have had some.
    fn put_broken(&mut self, rng: &mut Rng, lo: Vec3, hi: Vec3, color: Vec3, material: Material) {
        self.put(lo, hi, color, true, material);
        if let Some(o) = self.map.oboxes.last_mut() {
            o.broken = true;
        }
        let size = hi - lo;
        let along_x = size.x >= size.z;
        let len = size.x.max(size.z);
        if len < 1.0 || !rng.chance(0.6) {
            return;
        }
        let rods = rng.int(1, (len / 0.7).clamp(1.0, 4.0) as i32);
        for _ in 0..rods {
            let t = rng.range(0.15, 0.85);
            let p = if along_x {
                Vec3::new(
                    lo.x + size.x * t,
                    0.0,
                    (lo.z + hi.z) * 0.5 + rng.range(-0.08, 0.08),
                )
            } else {
                Vec3::new(
                    (lo.x + hi.x) * 0.5 + rng.range(-0.08, 0.08),
                    0.0,
                    lo.z + size.z * t,
                )
            };
            let h = rng.range(0.3, 0.8);
            self.put(
                Vec3::new(p.x - 0.025, hi.y - 0.45, p.z - 0.025),
                Vec3::new(p.x + 0.025, hi.y + h, p.z + 0.025),
                Vec3::new(0.32, 0.2, 0.13),
                false,
                Material::Rust,
            );
            if let Some(o) = self.map.oboxes.last_mut() {
                o.tilt = rng.range(-0.35, 0.35);
            }
        }
    }

    /// A slab of width `w` and length `len` that starts at local point `top`
    /// (its high end, at height top.y) and slopes down along local direction
    /// `dir` at `angle`, ending on the ground.
    fn sloped_slab(&mut self, top: Vec3, dir: Vec2, w: f32, len: f32, angle: f32, color: Vec3) {
        let (ax, az) = self.lot.axes();
        let world_dir = (ax * dir.x + az * dir.y).normalize();
        let run = len * angle.cos();
        let mid = Vec2::new(top.x, top.z) + dir * run * 0.5;
        let center = self.lot.center + ax * mid.x + az * mid.y;
        let y_mid = top.y - len * angle.sin() * 0.5;
        self.map.oboxes.push(OBox {
            center,
            // Local +z of the slab points down the slope.
            yaw: world_dir.x.atan2(world_dir.y),
            half: Vec2::new(w * 0.5, len * 0.5),
            y0: y_mid - SLAB * 0.5,
            y1: y_mid + SLAB * 0.5,
            color,
            side_mat: Material::Concrete,
            top_mat: Material::Concrete,
            tilt: angle,
            solid: true,
            chip: 0.0,
            broken: false,
            ramp: true,
        });
    }

    /// A loose chunk of debris (visual only), randomly turned and tipped.
    fn chunk(&mut self, rng: &mut Rng, at: Vec2, size: f32, color: Vec3) {
        let (ax, az) = self.lot.axes();
        let s = size * rng.range(0.6, 1.0);
        self.map.oboxes.push(OBox {
            center: self.lot.center + ax * at.x + az * at.y,
            yaw: rng.range(0.0, 6.28),
            half: Vec2::new(s * rng.range(0.6, 1.2), s * rng.range(0.4, 1.0)),
            y0: -s * 0.3,
            y1: s * rng.range(0.4, 0.9),
            color,
            side_mat: Material::Rubble,
            top_mat: Material::Rubble,
            tilt: rng.range(-0.5, 0.5),
            solid: false,
            chip: 0.0,
            broken: false,
            ramp: false,
        });
    }
}

/// How the building got hurt.
#[derive(Clone, Copy, PartialEq)]
enum Damage {
    /// Mostly standing: a bite out of the top corner.
    Light,
    /// One side down to the ground floor, the far side still at full height.
    Heavy,
    /// Floors largely gone, walls standing at ragged heights.
    Gutted,
}

/// Build one ruined building on `lot` (footprint in the world). `front` is the
/// local face toward the street: (axis, sign). Returns heaps for the terrain.
/// `clear` says whether a world point is free ground (no other building).
pub(super) fn building(
    map: &mut Map,
    rng: &mut Rng,
    lot: OBox,
    front: (usize, f32),
    clear: &dyn Fn(Vec2) -> bool,
) -> Vec<Drift> {
    let mut drifts = Vec::new();
    let half = lot.half;
    let color = PLASTERS[rng.index(PLASTERS.len())] * rng.range(0.94, 1.04);
    let small = half.x.min(half.y) < 4.0;
    let storeys: usize = if small { 1 } else { rng.int(2, 4) as usize };
    let damage = match rng.f32() {
        x if x < 0.35 => Damage::Light,
        x if x < 0.85 => Damage::Heavy,
        _ => Damage::Gutted,
    };
    let floor_y = |k: usize| {
        if k == 0 {
            0.0
        } else {
            GROUND_H + (k - 1) as f32 * STOREY_H
        }
    };
    let top_y = floor_y(storeys);

    // The bite: from a random direction, deeper on that side.
    let a = rng.range(0.0, std::f32::consts::TAU);
    let bite_dir = Vec2::new(a.cos(), a.sin());
    let bite_depth = match damage {
        Damage::Light => rng.range(1.0, 1.6) * STOREY_H,
        Damage::Heavy => top_y - GROUND_H + rng.range(0.5, 2.0),
        Damage::Gutted => top_y,
    };
    let noise_seed = rng.f32() * 100.0;
    // Height the structure still reaches at local point p.
    let cut = |p: Vec2| {
        let n = (p / half).clamp(Vec2::splat(-1.0), Vec2::splat(1.0));
        let side = smoothstep(-0.3, 0.9, n.dot(bite_dir));
        let wobble = ((p.x * 1.7 + noise_seed).sin() * (p.y * 1.3 - noise_seed).cos()) * 0.8;
        let base = top_y - bite_depth * side + wobble;
        if damage == Damage::Gutted {
            (base * 0.6 + top_y * 0.4 * (1.0 - side)).max(2.0)
        } else {
            base.max(1.0)
        }
    };

    // The face most exposed to the bite: where things fell.
    let (fall_axis, fall_sign) = if bite_dir.x.abs() / half.x > bite_dir.y.abs() / half.y {
        (0, bite_dir.x.signum())
    } else {
        (1, bite_dir.y.signum())
    };
    let fall_out = if fall_axis == 0 {
        Vec2::new(fall_sign, 0.0)
    } else {
        Vec2::new(0.0, fall_sign)
    };
    let face_half = if fall_axis == 0 { half.x } else { half.y };
    let across_half = if fall_axis == 0 { half.y } else { half.x };

    // A collapsed first-floor section sloping down out of the fall face: a
    // ramp up into the building, where the first floor still stands behind it.
    // (top point, angle, length); its path is carved through the face wall
    // and the slab edge below.
    // It comes down out of a face with free ground in front of it (the street
    // face first), so there's room to drive at it. Picked as (out direction,
    // top point, angle, length).
    let wants_ramp = damage != Damage::Light && storeys >= 2 && rng.chance(0.75);
    // 17-21 degrees: steeper ones are too much for the light hull to climb reliably.
    let angle = rng.range(0.3, 0.36);
    let len = GROUND_H / angle.sin();
    let lateral_t = rng.range(-0.25, 0.25);
    let faces = [
        front,
        (front.0, -front.1),
        (1 - front.0, 1.0),
        (1 - front.0, -1.0),
    ];
    // Ramps already built nearby (with their run-up): a new one must not
    // come down onto them.
    let others: Vec<OBox> = map
        .oboxes
        .iter()
        .filter(|o| o.solid && o.is_ramp() && o.center.distance(lot.center) < 40.0)
        .map(|o| OBox {
            half: o.half + Vec2::new(1.5, 6.0),
            center: o.center + o.axes().1 * 3.0,
            ..*o
        })
        .collect();
    let ramp = faces
        .iter()
        .filter(|_| wants_ramp)
        .find_map(|&(axis, sign)| {
            let out = if axis == 0 {
                Vec2::new(sign, 0.0)
            } else {
                Vec2::new(0.0, sign)
            };
            let (face, across_half) = if axis == 0 {
                (half.x, half.y)
            } else {
                (half.y, half.x)
            };
            if across_half < RAMP_W * 0.5 + 1.0 {
                return None;
            }
            let across = Vec2::new(out.y.abs(), out.x.abs());
            let top = out * (face - 2.5) + across * lateral_t * (across_half - RAMP_W * 0.5);
            if cut(top) < GROUND_H + 0.2 {
                return None;
            }
            // The ramp's foot plus a short run-up lands on free ground.
            let (ax, az) = lot.axes();
            let reach = len * angle.cos() + 3.5;
            let free = (0..=10).all(|i| {
                let local = top + out * (2.5 + reach * i as f32 / 10.0);
                [-0.4f32, 0.0, 0.4].iter().all(|&s| {
                    let p = local + across * s * RAMP_W;
                    p.dot(out) < face + 0.5 || {
                        let w = lot.center + ax * p.x + az * p.y;
                        clear(w) && others.iter().all(|o| o.distance(w) > 1.0)
                    }
                })
            });
            free.then_some((out, top))
        });
    let (ramp_out, ramp_face) = ramp.map_or((Vec2::X, half.x), |(out, _)| {
        (out, if out.x != 0.0 { half.x } else { half.y })
    });
    let across_dir = Vec2::new(ramp_out.y.abs(), ramp_out.x.abs());
    let ramp = ramp.map(|(_, top)| (top, angle, len));
    // In the ramp's path: within its width (plus room) and between the face and its top.
    let in_ramp_path = |p: Vec2| {
        ramp.is_some_and(|(top, _, _)| {
            let lateral = (p - top).dot(across_dir).abs();
            let out = p.dot(ramp_out);
            lateral < RAMP_W * 0.5 + 1.2 && out > ramp_face - 3.5
        })
    };

    let mut f = Frame { map, lot };

    // Walls, storey by storey, bay by bay.
    for (axis, sign) in [(0usize, 1.0f32), (0, -1.0), (1, 1.0), (1, -1.0)] {
        // The walls along x (axis 1) stop inside the cross walls so the corner
        // pieces don't overlap (coplanar faces z-fight).
        let len = if axis == 0 {
            half.y * 2.0
        } else {
            half.x * 2.0 - WALL_T * 2.0 - 0.02
        };
        let off = sign * ((if axis == 0 { half.x } else { half.y }) - WALL_T * 0.5);
        let bays = ((len / BAY).round() as usize).max(1);
        let bw = len / bays as f32;
        let street = (axis, sign) == front;
        // Local point on this wall at position s along it.
        let at = |s: f32| {
            if axis == 0 {
                Vec2::new(off, s)
            } else {
                Vec2::new(s, off)
            }
        };
        // Box spanning wall positions s0..s1 between heights y0..y1.
        let span = |s0: f32, s1: f32, y0: f32, y1: f32| {
            let (a, b) = (at(s0), at(s1));
            let lo = Vec3::new(a.x.min(b.x), y0, a.y.min(b.y));
            let hi = Vec3::new(a.x.max(b.x), y1, a.y.max(b.y));
            let t = Vec3::new(
                if axis == 0 { WALL_T * 0.5 } else { 0.0 },
                0.0,
                if axis == 1 { WALL_T * 0.5 } else { 0.0 },
            );
            (lo - t, hi + t)
        };
        for k in 0..storeys {
            let (y0, y1) = (floor_y(k), floor_y(k + 1));
            // Opening per bay: ground floor street bays are shopfronts (open to
            // 3.2 m: tanks fit), some back bays are doors, the rest windows.
            let opening = |i: usize| -> (f32, f32) {
                if k == 0 && ((street && i.is_multiple_of(2)) || i == bays / 2) {
                    (0.0, 3.2)
                } else {
                    (y0 + 1.0, y0 + 2.4)
                }
            };
            for i in 0..bays {
                let s0 = -len * 0.5 + i as f32 * bw;
                let s1 = s0 + bw;
                let mid = at((s0 + s1) * 0.5);
                // How much of this storey's wall survives here (a parapet
                // tops the last storey).
                let full = if k + 1 == storeys { y1 + 0.9 } else { y1 };
                let top_here = cut(mid).min(full);
                // Torn out where the floor came down: any bay touching the path.
                let touches = (0..=4).any(|q| in_ramp_path(at(s0 + (s1 - s0) * q as f32 / 4.0)));
                if k <= 1 && touches {
                    continue;
                }
                if top_here < y0 + 0.4 {
                    continue;
                }
                let (o0, o1) = opening(i);
                if top_here < o0 + 0.3 {
                    // Broken off below the window: just a ragged stub of wall.
                    let (lo, hi) = span(s0, s1, y0, top_here);
                    f.put_broken(rng, lo, hi, color, Material::Plaster);
                    continue;
                }
                // Piers either side, each broken at its own height.
                for (p0, p1) in [(s0, s0 + PIER), (s1 - PIER, s1)] {
                    let h = cut(at((p0 + p1) * 0.5)).min(full).max(o0.min(top_here));
                    let (lo, hi) = span(p0, p1, y0, h);
                    if h < full - 0.05 {
                        f.put_broken(rng, lo, hi, color, Material::Plaster);
                    } else {
                        f.put(lo, hi, color, true, Material::Plaster);
                    }
                }
                // Sill under the opening.
                if o0 > y0 + 0.01 {
                    let (lo, hi) = span(s0 + PIER, s1 - PIER, y0, o0);
                    f.put(lo, hi, color, true, Material::Plaster);
                }
                // Wall over the opening, up to whatever height survives (jagged
                // where it's broken: the top steps across the bay).
                if top_here > o1 + 0.25 {
                    let (lo, hi) = span(s0 + PIER, s1 - PIER, o1, top_here);
                    if top_here < full - 0.05 {
                        f.put_broken(rng, lo, hi, color * 0.97, Material::Plaster);
                    } else {
                        f.put(lo, hi, color * 0.97, true, Material::Plaster);
                    }
                } else if top_here > o1 - 0.2 && rng.chance(0.5) {
                    let (lo, hi) = span(s0 + PIER, s0 + PIER + bw * 0.35, o1, o1 + 0.45);
                    f.put_broken(rng, lo, hi, color * 0.95, Material::Plaster);
                }
            }
        }
    }

    // Floor slabs on a grid of cells; ragged where the bite reaches.
    let cells =
        (Vec2::new((half.x * 2.0 / 3.0).ceil(), (half.y * 2.0 / 3.0).ceil())).max(Vec2::ONE);
    let (nx, nz) = (cells.x as usize, cells.y as usize);
    let cell = half * 2.0 / cells;
    for k in 1..=storeys {
        let y = floor_y(k);
        for iz in 0..nz {
            for ix in 0..nx {
                let lo = Vec2::new(-half.x + ix as f32 * cell.x, -half.y + iz as f32 * cell.y);
                // Slabs span between the walls, not over them (so slab and wall
                // tops never coincide).
                let inner = half - Vec2::splat(WALL_T + 0.02);
                let c = lo + cell * 0.5;
                // The slab at height y exists if the storey below it still reaches it.
                if cut(c) < y + 0.2
                    || (damage == Damage::Gutted && rng.chance(0.45))
                    || (k == 1 && in_ramp_path(c))
                {
                    continue;
                }
                let edge = cut(c) < y + 2.0;
                let shrink = if edge {
                    Vec2::new(rng.range(0.0, 0.8), rng.range(0.0, 0.8))
                } else {
                    Vec2::ZERO
                };
                f.put(
                    Vec3::new(
                        (lo.x + shrink.x).max(-inner.x),
                        y - SLAB,
                        (lo.y + shrink.y).max(-inner.y),
                    ),
                    Vec3::new(
                        (lo.x + cell.x - shrink.x * 0.3).min(inner.x),
                        y,
                        (lo.y + cell.y - shrink.y * 0.3).min(inner.y),
                    ),
                    SLAB_C,
                    true,
                    Material::Concrete,
                );
            }
        }
    }
    // Corner columns up to the broken height.
    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let c = Vec2::new(sx * (half.x - 0.35), sz * (half.y - 0.35));
        if in_ramp_path(c) {
            continue;
        }
        // A touch taller than the walls it sits in, so their tops never coincide.
        let h = cut(c).min(top_y + 0.9) + 0.07;
        let (lo, hi) = (
            Vec3::new(c.x - 0.4, 0.0, c.y - 0.4),
            Vec3::new(c.x + 0.4, h, c.y + 0.4),
        );
        if cut(c) < top_y + 0.85 {
            f.put_broken(rng, lo, hi, color * 0.92, Material::Plaster);
        } else {
            f.put(lo, hi, color * 0.92, true, Material::Plaster);
        }
    }

    // The collapsed floor section (placed before the walls so it can carve its way).
    if let Some((top, angle, len)) = ramp {
        f.sloped_slab(
            Vec3::new(top.x, GROUND_H, top.y),
            ramp_out,
            RAMP_W,
            len,
            angle,
            SLAB_C * 0.95,
        );
        // A landing at the top joins the ramp to the floor that's left (the
        // floor right behind it was carved away with the ramp's path).
        let back = top - ramp_out * 3.2;
        let lo = top.min(back) - across_dir * (RAMP_W * 0.5);
        let hi = top.max(back) + across_dir * (RAMP_W * 0.5);
        f.put(
            // 3 cm thicker than the floor both ways so its faces never coincide with a slab's.
            Vec3::new(lo.x, GROUND_H - SLAB - 0.03, lo.y),
            Vec3::new(hi.x, GROUND_H + 0.03, hi.y),
            SLAB_C,
            true,
            Material::Concrete,
        );
    }

    // Rubble heaped at the foot of the damage, and loose debris around it.
    if damage != Damage::Light {
        // Not where a collapsed floor comes down: the heap would tip tanks
        // off the side of the ramp.
        let heap = (rng.range(0.7, 1.1), rng.range(6.0, 9.0));
        if !(ramp.is_some() && ramp_out == fall_out) {
            drifts.push(Drift {
                b: lot,
                axis: fall_axis,
                sign: fall_sign,
                height: heap.0,
                run: heap.1,
            });
        }
        for _ in 0..rng.int(8, 16) {
            let along = rng.range(-across_half, across_half);
            let out = rng.range(-1.0, 5.0);
            let p = fall_out * (face_half + out)
                + Vec2::new(fall_out.y.abs(), fall_out.x.abs()) * along;
            let (size, c) = (
                rng.range(0.4, 1.3),
                if rng.chance(0.5) {
                    color * 0.9
                } else {
                    RUBBLE_C
                },
            );
            f.chunk(rng, p, size, c);
        }
    }
    // Interior debris on the ground floor.
    for _ in 0..rng.int(2, 6) {
        let p = Vec2::new(rng.range(-half.x, half.x), rng.range(-half.y, half.y)) * 0.7;
        let size = rng.range(0.4, 1.0);
        f.chunk(rng, p, size, RUBBLE_C);
    }
    // Sand blown up against the street face.
    if rng.chance(0.5) {
        drifts.push(Drift {
            b: lot,
            axis: front.0,
            sign: front.1,
            height: 0.8,
            run: 3.5,
        });
    }
    drifts
}
