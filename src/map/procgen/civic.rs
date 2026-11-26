//! Medium structures: bigger than a house, smaller than a great building,
//! each a fight of its own. Anyone's: a style lends its stone.
//!
//! - **Courtyard**: a wall with a gate in each side, towers at its corners.
//! - **Market**: a great roof on pillars, open all round.
//! - **Terrace**: a raised floor with a slope up two sides: high ground.

use glam::{Vec2, Vec3};

use super::skeleton::{CivicKind, Lot};
use crate::map::{Map, OBox};
use crate::render::material::Material;
use crate::rng::Rng;

/// What a style builds with: its stone, its trim and its roofs.
#[derive(Clone, Copy)]
pub struct Stone {
    pub wall: Vec3,
    pub mat: Material,
    pub trim: Vec3,
    pub roof: Vec3,
    /// The ground it's built on.
    pub land: Vec3,
}

/// Wall and roof heights; a terrace's floor and its slopes' pitch.
const WALL_H: f32 = 4.2;
const ROOF_H: f32 = 7.0;
const FLOOR_H: f32 = 2.2;
const PITCH: f32 = 0.3;

struct Site<'a> {
    map: &'a mut Map,
    f: OBox,
}

impl Site<'_> {
    /// A box `half` across at (`x`, `z`) on the site, from `y0` to `y1`.
    fn put(&mut self, at: Vec2, half: Vec2, y: (f32, f32), c: Vec3, m: Material, solid: bool) {
        let (ax, az) = self.f.axes();
        self.map.oboxes.push(OBox {
            center: self.f.center + ax * at.x + az * at.y,
            yaw: self.f.yaw,
            half,
            y0: y.0,
            y1: y.1,
            color: c,
            side_mat: m,
            top_mat: m,
            solid,
            ..OBox::default()
        });
    }
}

pub fn build(map: &mut Map, rng: &mut Rng, lot: &Lot, kind: CivicKind, s: Stone) {
    let f = lot.footprint;
    let (w, l) = (f.half.x, f.half.y);
    let mut site = Site { map, f };
    match kind {
        CivicKind::Courtyard => {
            let gate = 4.5;
            for e in [-1.0f32, 1.0] {
                for k in [-1.0f32, 1.0] {
                    // Either side of the gate in each wall; now and then a
                    // stretch is broken down.
                    for along_x in [true, false] {
                        let (len, off) = if along_x { (w, l) } else { (l, w) };
                        let half = (len - gate) * 0.5;
                        let mid = k * (gate + half);
                        let h = if rng.chance(0.25) {
                            rng.range(1.0, 2.2)
                        } else {
                            WALL_H
                        };
                        let (at, hf) = if along_x {
                            (Vec2::new(mid, e * (off - 0.6)), Vec2::new(half, 0.55))
                        } else {
                            (Vec2::new(e * (off - 0.6), mid), Vec2::new(0.55, half))
                        };
                        site.put(at, hf, (-0.1, h), s.wall, s.mat, true);
                        site.map.oboxes.last_mut().unwrap().broken = h < WALL_H;
                    }
                    let at = Vec2::new(k * (w - 1.3), e * (l - 1.3));
                    let h = WALL_H + rng.range(2.0, 4.0);
                    site.put(at, Vec2::splat(1.35), (-0.1, h), s.wall * 0.95, s.mat, true);
                    site.put(at, Vec2::splat(1.6), (h, h + 0.5), s.trim, s.mat, false);
                }
            }
            // Something low to fight round in the middle.
            site.put(
                Vec2::ZERO,
                Vec2::splat(2.0),
                (-0.1, 1.3),
                s.wall * 0.9,
                s.mat,
                true,
            );
            site.put(
                Vec2::ZERO,
                Vec2::splat(0.7),
                (1.3, 5.0),
                s.trim,
                s.mat,
                false,
            );
        }
        CivicKind::Market => {
            let (nx, nz) = (2, ((l * 2.0) / 9.0).round().max(2.0) as i32);
            for i in 0..=nx {
                for j in 0..=nz {
                    let at = Vec2::new(
                        (i as f32 / nx as f32 * 2.0 - 1.0) * (w - 1.2),
                        (j as f32 / nz as f32 * 2.0 - 1.0) * (l - 1.2),
                    );
                    site.put(at, Vec2::splat(0.6), (-0.1, ROOF_H), s.wall, s.mat, true);
                }
            }
            site.put(
                Vec2::ZERO,
                Vec2::new(w, l),
                (ROOF_H, ROOF_H + 0.8),
                s.trim,
                s.mat,
                true,
            );
            for k in 1..4 {
                let u = 1.0 - k as f32 * 0.24;
                let y = ROOF_H + 0.8 + (k - 1) as f32 * 1.1;
                let half = Vec2::new(w * u, l - k as f32 * 1.2);
                site.put(Vec2::ZERO, half, (y, y + 1.1), s.roof, s.mat, false);
            }
            for _ in 0..4 {
                let at = Vec2::new(rng.range(-0.55, 0.55) * w, rng.range(-0.7, 0.7) * l);
                site.put(
                    at,
                    Vec2::new(rng.range(0.8, 1.6), 0.7),
                    (-0.1, 1.1),
                    s.trim * 0.8,
                    Material::Wood,
                    false,
                );
            }
        }
        CivicKind::Terrace => {
            let len = FLOOR_H / PITCH.sin();
            let run = len * PITCH.cos();
            let top = l - run;
            site.put(
                Vec2::ZERO,
                Vec2::new(w, top),
                (-0.3, FLOOR_H),
                s.wall,
                s.mat,
                true,
            );
            site.map.oboxes.last_mut().unwrap().chip = 0.01;
            let (_, az) = f.axes();
            for e in [-1.0f32, 1.0] {
                let down = az * e;
                let c = f.center + down * (top + run * 0.5);
                site.map.oboxes.push(OBox {
                    center: c,
                    yaw: down.x.atan2(down.y),
                    half: Vec2::new(w * 0.6, len * 0.5),
                    y0: FLOOR_H * 0.5 - 0.3,
                    y1: FLOOR_H * 0.5,
                    color: s.wall * 0.92,
                    side_mat: s.mat,
                    top_mat: s.mat,
                    tilt: PITCH,
                    ramp: true,
                    ..OBox::default()
                });
                // A parapet down each long side, and pillars at its ends.
                site.put(
                    Vec2::new(e * (w - 0.35), 0.0),
                    Vec2::new(0.35, top * 0.6),
                    (FLOOR_H, FLOOR_H + 1.0),
                    s.trim,
                    s.mat,
                    true,
                );
                for k in [-1.0f32, 1.0] {
                    let at = Vec2::new(e * (w - 0.7), k * (top - 0.7));
                    site.put(
                        at,
                        Vec2::splat(0.6),
                        (FLOOR_H, FLOOR_H + rng.range(3.0, 7.0)),
                        s.wall,
                        s.mat,
                        true,
                    );
                }
            }
        }
    }
}
