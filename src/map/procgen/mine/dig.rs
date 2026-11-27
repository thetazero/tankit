//! What any mine has, whoever's it is: steel sets, railroads and bridges,
//! the tipple, headframes, the warehouse, carts, crates, cutting rigs, rock
//! and raw crystal. Each is built where it's put, whichever way it faces.

use std::f32::consts::{PI, TAU};
use std::ops::{Deref, DerefMut};

use glam::{Quat, Vec2, Vec3, Vec4};

use super::*;
use crate::map::landmarks::Rig;
use crate::map::rocks;
use crate::map::{Ambient, Crystal, Map, OBox};
use crate::render::material::Material;
use crate::render::mesh::MeshData;
use crate::rng::Rng;
use crate::tank::model::{self, Skin};

/// Sandstone, in a few shades; the desert floor.
/// The mesa's top: weathered rock, and the cave roofs laid flush with it.
pub(super) const MESA_TOP: Vec3 = Vec3::new(0.46, 0.26, 0.17);
/// Raw crystal, in the rock and in the ore: the crystal everyone's here
/// for, nobody's yet. Cloudy blue-grey, dull until it's cut.
pub(super) const RAW: Vec3 = Vec3::new(0.1, 0.125, 0.17);
pub(super) const STEEL: Vec3 = Vec3::new(0.2, 0.21, 0.24);
pub(super) const RUST: Vec3 = Vec3::new(0.36, 0.22, 0.14);
const TIMBER: Vec3 = Vec3::new(0.42, 0.32, 0.22);
/// Black iron, gone brown at the edges: cart wheels and binding.
const IRON: Vec3 = Vec3::new(0.16, 0.13, 0.11);
const CONCRETE: Vec3 = Vec3::new(0.6, 0.57, 0.52);
/// Mine machinery's paint, faded in the sun.
const MACHINE: Vec3 = Vec3::new(0.56, 0.42, 0.2);

/// A map being dug: the map, the layout it's dug to, the style it's
/// dressed in. Reads as the map (the builders' `self.oboxes` are its).
pub(in crate::map) struct Dig<'a> {
    pub map: &'a mut Map,
    pub l: &'a Skeleton,
    pub st: &'a dyn MineStyle,
    pub look: Look,
    /// Loose rock: the usual, dark and pale (`land::loose`).
    pub loose: [Vec3; 3],
    /// Building inside digs: the ground in one is its floor (else its top).
    pub inside: bool,
}

impl Deref for Dig<'_> {
    type Target = Map;
    fn deref(&self) -> &Map {
        self.map
    }
}

impl DerefMut for Dig<'_> {
    fn deref_mut(&mut self) -> &mut Map {
        self.map
    }
}

use crate::map::procgen::bridge::{self, Form};
use crate::map::procgen::crossing::Crossing;
use crate::map::procgen::skeleton::{Lot, LotKind};

impl Dig<'_> {
    /// The box just placed is rock from the rocks toolkit, of this `kind`
    /// (ore heaped in a bin or a car: broken lumps, the heap a block).
    pub(super) fn last_is_rock(&mut self, kind: rocks::Kind) {
        if let Some(o) = self.oboxes.last_mut() {
            o.rock = Some(kind);
        }
    }

    /// A boulder: a lump of rock (drawn and collided as one).
    pub(super) fn boulder(&mut self, rng: &mut Rng, at: Vec2, size: f32, solid: bool) {
        if solid && self.in_the_way(at, size * 1.2) {
            return;
        }
        let g = self.stand(at);
        let shades = self.loose;
        let yaw = rng.range(0.0, TAU);
        let half = Vec2::new(size * rng.range(0.8, 1.1), size * rng.range(0.6, 0.9));
        let top = g + size * rng.range(0.9, 1.4);
        let color = shades[rng.index(shades.len())] * rng.range(0.9, 1.08);
        self.oboxes.push(OBox {
            solid,
            ..rocks::boulder(at, yaw, half, g - size * 0.4, top, color)
        });
    }

    /// A rock bigger than a tank, often with a second block broken off
    /// against it, and a piece of rubble: cover. It grows bigger only where there's room for it (off the ways
    /// round, the fords, the doors): it stays as asked for otherwise.
    pub(super) fn rock_pile(&mut self, rng: &mut Rng, at: Vec2, size: f32) {
        let size = if self.room(at, size * 1.4 + 1.0) {
            size * 1.25
        } else {
            size
        };
        self.boulder(rng, at, size, true);
        if rng.chance(0.5) {
            let a = rng.range(0.0, TAU);
            let q = at + Vec2::new(a.cos(), a.sin()) * size * rng.range(1.0, 1.3);
            let s = size * rng.range(0.55, 0.75);
            if self.room(q, s * 1.2 + 1.0) {
                self.boulder(rng, q, s, true);
            }
        }
        if rng.chance(0.6) {
            let a = rng.range(0.0, TAU);
            let q = at + Vec2::new(a.cos(), a.sin()) * rng.range(size + 1.0, size + 3.0);
            let s = rng.range(0.4, 0.7);
            self.boulder(rng, q, s, false);
        }
    }

    /// One crystal standing in the ground at `at`: `h` tall, `r` thick,
    /// leaning `lean` toward `dir`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn shard(
        &mut self,
        seed: u64,
        at: Vec3,
        h: f32,
        r: f32,
        dir: Vec2,
        lean: f32,
        lit: bool,
        solid: bool,
    ) {
        let d = dir.normalize_or(Vec2::X);
        let axis = Vec3::new(d.x * lean.sin(), lean.cos(), d.y * lean.sin()) * (h + 1.0);
        self.crystals.push(Crystal {
            base: at - Vec3::Y * 1.0,
            axis,
            radius: r,
            sides: 5 + (seed % 3) as u32,
            seed: (seed >> 8) as u32,
            color: RAW * (0.85 + 0.3 * ((seed >> 40) % 1000) as f32 / 1000.0),
            lit,
            solid,
            raw: true,
        });
    }

    /// An outcrop of seam crystal at ground level: one big shard for cover
    /// and small ones round it.
    pub(super) fn outcrop(&mut self, rng: &mut Rng, p: Vec2, h: f32) {
        let g = self.ground_height(p.x, p.y);
        let lean_dir = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
        self.shard(
            rng.next_u64(),
            Vec3::new(p.x, g, p.y),
            h,
            1.1,
            lean_dir,
            rng.range(0.05, 0.3),
            true,
            true,
        );
        for _ in 0..4 {
            let a = rng.range(0.0, TAU);
            let q = p + Vec2::new(a.cos(), a.sin()) * rng.range(1.6, 2.8);
            let gq = self.ground_height(q.x, q.y);
            self.shard(
                rng.next_u64(),
                Vec3::new(q.x, gq, q.y),
                rng.range(0.8, 1.8),
                0.35,
                q - p,
                rng.range(0.3, 0.7),
                rng.chance(0.4),
                false,
            );
        }
    }

    /// The ground at a point: a dig's floor, in one.
    pub(super) fn ground_height(&self, x: f32, z: f32) -> f32 {
        self.stand(Vec2::new(x, z))
    }

    /// What something at `p` stands on: a dig's floor, in one.
    pub(super) fn stand(&self, p: Vec2) -> f32 {
        let digs = self.l.land.digs.iter().filter(|_| self.inside);
        let dig = digs.into_iter().find(|d| d.distance(p) < 0.0);
        dig.map_or_else(|| self.map.ground_height(p.x, p.y), |d| d.floor)
    }

    /// A steel set from `a` to `b`, its cap `h` up.
    pub(super) fn steel_set_to(&mut self, a: Vec2, b: Vec2, h: f32) {
        let mid = (a + b) * 0.5;
        let across = (b - a).normalize();
        for p in [a, b] {
            self.pipe(
                Vec3::new(p.x, -0.2, p.y),
                Vec3::new(p.x, h, p.y),
                0.26,
                STEEL,
                Material::Metal,
                true,
            );
        }
        // The cap, into the rock at each end (over every tank: it's never
        // in the way).
        let w = a.distance(b) * 0.5 + 0.8;
        self.oboxes.push(OBox {
            center: mid,
            yaw: yaw_along(across.perp()),
            half: Vec2::new(w, 0.22),
            y0: h - 0.05,
            y1: h + 0.45,
            color: STEEL * 0.9,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        // Knee braces.
        for (p, s) in [(a, 1.0), (b, -1.0)] {
            self.segment(
                Vec3::new(p.x, h - 1.3, p.y),
                Vec3::new(p.x + across.x * s * 1.3, h, p.y + across.y * s * 1.3),
                0.1,
                STEEL,
                Material::Metal,
                false,
            );
        }
    }

    /// A floodlight on a steel stand at `at`, its lamp turned toward `look`.
    pub(super) fn floodlight(&mut self, at: Vec2, look: Vec2) {
        let base = Vec3::new(at.x, 0.0, at.y);
        let top = base + Vec3::Y * 4.6;
        for k in 0..3 {
            let a = k as f32 * TAU / 3.0;
            let leg = base + Vec3::new(a.cos(), 0.0, a.sin()) * 0.9;
            self.segment(
                leg,
                top - Vec3::Y * 1.2,
                0.07,
                STEEL,
                Material::Metal,
                false,
            );
        }
        self.pipe(
            base + Vec3::Y * 1.0,
            top,
            0.09,
            STEEL,
            Material::Metal,
            false,
        );
        let d = (look - at).normalize_or(Vec2::X);
        let rig = Rig {
            pos: top,
            rot: Quat::from_rotation_y(yaw_along(d)) * Quat::from_rotation_x(0.35),
        };
        rig.part(
            self,
            Vec3::ZERO,
            Vec3::new(0.7, 0.4, 0.3),
            Quat::IDENTITY,
            STEEL * 0.7,
            Material::Metal,
            false,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, 0.0, 0.31),
            Vec3::new(0.6, 0.3, 0.02),
            Quat::IDENTITY,
            Vec3::new(1.0, 0.85, 0.95),
            Material::Glow,
            false,
            0.0,
        );
    }

    /// Cut crystal crated for the train: iron-bound timber crates, two
    /// stacked in the middle and one either side, and a low one behind
    /// with its lid off, crystal lumps showing dull in it. Plain boxes
    /// where the old cut blocks stood are what tanks hit and hide behind.
    pub(super) fn block_stack(&mut self, at: Vec2, yaw: f32) {
        // Its detail comes from its own stream.
        let rng = &mut own_rng(at);
        let g = self.stand(at);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        for (k, (dx, dz, h)) in [
            (-1.9, 0.0, 1.6),
            (0.0, 0.0, 3.0),
            (1.9, 0.0, 1.6),
            (0.0, 1.9, 1.2),
        ]
        .into_iter()
        .enumerate()
        {
            let half = 0.9 - 0.02 * k as f32;
            self.hidden_part(
                &rig,
                Vec3::new(dx, (h - 0.2) * 0.5, dz),
                Vec3::new(half, (h + 0.2) * 0.5, half),
            );
        }
        let turn = |rng: &mut Rng| rng.range(-0.08, 0.08);
        let t = turn(rng);
        self.crystal_crate(rng, &rig, Vec2::new(-1.9, 0.0), 0.0, 0.86, 1.55, t, false);
        let t = turn(rng);
        self.crystal_crate(rng, &rig, Vec2::new(0.0, 0.0), 0.0, 0.88, 1.5, t, false);
        // On the one under it, resting on its lid's battens.
        let t = turn(rng) + 0.15;
        self.crystal_crate(
            rng,
            &rig,
            Vec2::new(0.05, -0.05),
            1.625,
            0.76,
            1.375,
            t,
            false,
        );
        let t = turn(rng);
        self.crystal_crate(rng, &rig, Vec2::new(1.9, 0.0), 0.0, 0.86, 1.55, t, false);
        let t = turn(rng);
        self.crystal_crate(rng, &rig, Vec2::new(0.0, 1.9), 0.0, 0.84, 1.15, t, true);
    }

    /// One crate in `rig`'s frame, centered at `c` with its foot at `y0`,
    /// `half` across and `h` high, turned `turn`: plank sides, each board
    /// its own shade, iron at the corners and banded round, a plank lid,
    /// or (`open`) no lid and raw crystal heaped to the rim.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn crystal_crate(
        &mut self,
        rng: &mut Rng,
        rig: &Rig,
        c: Vec2,
        y0: f32,
        half: f32,
        h: f32,
        turn: f32,
        open: bool,
    ) {
        let rig = Rig {
            pos: rig.point(Vec3::new(c.x, y0, c.y)),
            rot: rig.rot * Quat::from_rotation_y(turn),
        };
        // Drawn sculpted, in the crate's frame: as map boxes, touching all
        // over, they'd be grown apart into each other (see `growths`).
        let mut m = MeshData::default();
        let part = |m: &mut MeshData, c: Vec3, h: Vec3, color: Vec3, mat: Material| {
            let col = color.extend(mat.id());
            m.add_box(c - h, c + h, col, col);
        };
        // Dark inside, seen between the boards, the lid resting on it;
        // lower when open, so the rim stands above what's in it. (Faces
        // here stay a few cm apart, never a hair, or they'd z-fight.)
        let core = if open { h - 0.2 } else { h + 0.005 };
        part(
            &mut m,
            Vec3::new(0.0, core * 0.5, 0.0),
            Vec3::new(half - 0.1, core * 0.5, half - 0.1),
            TIMBER * 0.2,
            Material::Wood,
        );
        // Boards round the sides.
        let n = ((h / 0.3).round() as usize).max(3);
        let pitch = h / n as f32;
        for k in 0..n {
            let y = pitch * (k as f32 + 0.5);
            let bh = pitch * 0.44;
            for s in [-1.0f32, 1.0] {
                let shade = TIMBER * rng.range(0.6, 0.85);
                part(
                    &mut m,
                    Vec3::new(s * (half - 0.03), y, 0.0),
                    Vec3::new(0.03, bh, half - 0.07),
                    shade,
                    Material::Wood,
                );
                let shade = TIMBER * rng.range(0.6, 0.85);
                part(
                    &mut m,
                    Vec3::new(0.0, y, s * (half - 0.03)),
                    Vec3::new(half - 0.07, bh, 0.03),
                    shade,
                    Material::Wood,
                );
            }
        }
        // Iron: angle down each corner, and two bands round over it.
        for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            part(
                &mut m,
                Vec3::new(x * (half - 0.04), h * 0.5, z * (half - 0.04)),
                Vec3::new(0.06, h * 0.5, 0.06),
                IRON * 0.8,
                Material::Metal,
            );
        }
        for y in [h * 0.2, h * 0.8] {
            for s in [-1.0f32, 1.0] {
                part(
                    &mut m,
                    Vec3::new(s * (half + 0.02), y, 0.0),
                    Vec3::new(0.025, 0.045, half + 0.045),
                    IRON * 0.7,
                    Material::Rust,
                );
                part(
                    &mut m,
                    Vec3::new(0.0, y, s * (half + 0.02)),
                    Vec3::new(half + 0.045, 0.045, 0.025),
                    IRON * 0.7,
                    Material::Rust,
                );
            }
        }
        if !open {
            // The lid: three planks across, nailed down on two battens.
            for k in 0..3 {
                let z = (k as f32 - 1.0) * half * 0.66;
                part(
                    &mut m,
                    Vec3::new(0.0, h + 0.04, z),
                    Vec3::new(half + 0.03, 0.035, half * 0.32),
                    TIMBER * rng.range(0.65, 0.9),
                    Material::Wood,
                );
            }
            for s in [-1.0f32, 1.0] {
                part(
                    &mut m,
                    Vec3::new(s * half * 0.6, h + 0.1, 0.0),
                    Vec3::new(0.06, 0.025, half - 0.04),
                    TIMBER * 0.55,
                    Material::Wood,
                );
            }
            self.sculpt(&rig, &m);
            return;
        }
        // Open: lumps of raw crystal heaped to the rim, dull blue-grey,
        // and one long piece laid across them, its faint light up a face.
        let inner = half - 0.12;
        for i in 0..3 {
            for j in 0..3 {
                let u = (i as f32 - 1.0) * inner * 0.62 + rng.range(-0.08, 0.08);
                let v = (j as f32 - 1.0) * inner * 0.62 + rng.range(-0.08, 0.08);
                let mound = 0.12 * (1.0 - (u * u + v * v) / (inner * inner)).max(0.0);
                let at = Vec3::new(u, core + 0.05 + mound, v);
                let s = rng.range(0.22, 0.32);
                let shade = RAW * rng.range(1.0, 1.3);
                ore_lump(&mut m, rng, at, s, shade, Material::Ore);
            }
        }
        self.sculpt(&rig, &m);
        let a = rig.point(Vec3::new(-inner * 0.8, core + 0.22, -inner * 0.3));
        let b = rig.point(Vec3::new(inner * 0.7, core + 0.32, inner * 0.35));
        self.crystals.push(Crystal {
            base: a,
            axis: b - a,
            radius: 0.17,
            sides: 6,
            seed: rng.next_u64() as u32,
            color: RAW * 1.1,
            lit: true,
            solid: false,
            raw: true,
        });
    }

    /// The railroads, two rails on sleepers: visual only.
    pub(super) fn railroad(&mut self) {
        let lay = self.l;
        for (k, line) in lay.tracks.iter().enumerate() {
            let off = lay.off_track(k);
            self.rails(line, off.map(|(m, _)| &lay.tracks[m][..]));
            if let Some((m, s)) = off {
                self.points(&lay.tracks[m], s, line);
            }
        }
    }

    /// The points lever where `spur` leaves `main`, `s` m along it, on the
    /// outside of the curve.
    fn points(&mut self, main: &[Vec2], s: f32, spur: &[Vec2]) {
        let (j, dir) = line_at(main, s);
        let turn = (spur[spur.len() - 1] - j).dot(dir.perp()).signum();
        let at = j - dir * 2.0 - dir.perp() * 2.8 * turn;
        let g = self.ground_height(at.x, at.y);
        self.oboxes.push(OBox {
            center: at,
            yaw: yaw_along(dir),
            half: Vec2::new(0.25, 0.35),
            y0: g - 0.1,
            y1: g + 0.5,
            color: STEEL,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        self.segment(
            Vec3::new(at.x, g + 0.45, at.y),
            Vec3::new(at.x + dir.x * 0.3, g + 1.3, at.y + dir.y * 0.3),
            0.04,
            RUST,
            Material::Rust,
            false,
        );
    }

    /// Rails on sleepers along `line`, following the ground (or the
    /// bridge's deck). A spur leaves out the sleepers where those of the line
    /// it's `off` already are.
    pub(super) fn rails(&mut self, line: &[Vec2], off: Option<&[Vec2]>) {
        let lay = self.l;
        let total = line_len(line, line.len() - 1);
        let y_at = |m: &Dig, s: f32| {
            let (p, dir) = line_at(line, s);
            if lay.on_bridge(p) {
                DECK_TOP
            } else {
                let side = dir.perp() * 1.5;
                m.stand(p).max(m.stand(p + side)).max(m.stand(p - side))
            }
        };
        let step = 1.6;
        let n = (total / step) as usize;
        for k in 0..n {
            let s = step * (k as f32 + 0.5);
            let (p, dir) = line_at(line, s);
            if off.is_some_and(|m| line_distance(m, p) < 2.4) {
                continue;
            }
            let y = y_at(self, s);
            self.oboxes.push(OBox {
                center: p,
                yaw: yaw_along(dir),
                half: Vec2::new(GAUGE * 0.5 + 0.7, 0.18),
                y0: y - 0.1,
                y1: y + 0.08,
                color: TIMBER * 0.8,
                side_mat: Material::Wood,
                top_mat: Material::Wood,
                solid: false,
                ..OBox::default()
            });
        }
        // Rails, in runs of up to 6 m following the ground, broken at the
        // line's bends.
        let mut s = if off.is_some() { 1.0 } else { 0.0 };
        // (Solid ground has finer bumps than a terrain.)
        let run = if self.solid.is_some() { 2.0 } else { 6.0 };
        let bends: Vec<f32> = (1..line.len()).map(|k| line_len(line, k)).collect();
        while s < total - 0.01 {
            let next_bend = bends
                .iter()
                .copied()
                .find(|&b| b > s + 0.01)
                .unwrap_or(total);
            let e = (s + run).min(next_bend);
            let (pa, dir) = line_at(line, s + 0.001);
            let (pb, _) = line_at(line, e - 0.001);
            let (ya, yb) = (y_at(self, s) + 0.16, y_at(self, e - 0.001) + 0.16);
            for side in [-1.0f32, 1.0] {
                let o = dir.perp() * side * GAUGE * 0.5;
                self.segment(
                    Vec3::new(pa.x + o.x, ya, pa.y + o.y),
                    Vec3::new(pb.x + o.x, yb, pb.y + o.y),
                    0.08,
                    STEEL * 1.4,
                    Material::Metal,
                    false,
                );
            }
            s = e;
        }
    }

    /// The rail bridge out of the adit's mouth over the wash: a deck on
    /// concrete piers, low steel girders along its edges, what its
    /// builders raise over them, tilted plates up onto it at each end.
    pub(super) fn rail_bridge(&mut self, br: &Crossing) {
        let lay = self.l;
        let (a, b) = (br.from, br.to);
        let len = a.distance(b);
        let dir = (b - a) / len;
        let yaw = yaw_along(dir);
        let mid = (a + b) * 0.5;
        self.oboxes.push(OBox {
            center: mid,
            yaw,
            half: Vec2::new(DECK_HALF, len * 0.5),
            y0: DECK_TOP - DECK_T,
            y1: DECK_TOP,
            color: STEEL * 1.2,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            ramp: true,
            ..OBox::default()
        });
        // Plates up onto it from the ground at each end.
        for (end, out) in [(a, -dir), (b, dir)] {
            let run = 2.6;
            let rise = DECK_TOP + 0.15;
            let slope = (rise / run).atan();
            let c = end + out * (run * 0.5 - 0.05);
            // Local +z points along `out`; tip its far (+z) end down.
            self.oboxes.push(OBox {
                center: c,
                yaw: yaw_along(out),
                half: Vec2::new(DECK_HALF - 0.4, run * 0.5 + 0.1),
                y0: DECK_TOP * 0.5 - 0.2 - 0.15,
                y1: DECK_TOP * 0.5 + 0.2 - 0.15,
                tilt: slope,
                color: STEEL,
                side_mat: Material::Metal,
                top_mat: Material::Metal,
                ramp: true,
                ..OBox::default()
            });
        }
        // Girders along each edge: low enough to shoot over, high enough to
        // keep a tank on the deck.
        let side_dir = dir.perp();
        for side in [-1.0f32, 1.0] {
            let off = side_dir * side * (DECK_HALF - 0.25);
            self.oboxes.push(OBox {
                center: mid + off,
                yaw,
                half: Vec2::new(0.25, len * 0.5),
                y0: DECK_TOP - 0.01,
                y1: DECK_TOP + 0.9,
                color: RUST,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
        }
        bridge::dress(self.map, br.deck(), self.look, Form::of(self.look));
        // Piers: a concrete column under each rail where the wash falls
        // away, a steel cap beam across their tops.
        // Ten meters apart: room to drive between them, and either side of
        // where the wash's middle runs under the deck (the way along its
        // floor).
        let (bw, bs) = lay.bridged(br);
        let cross = (line_at(&lay.land.washes[bw].line, bs).0 - a).dot(dir);
        let mut t = cross - 5.0;
        while t - 10.0 >= 4.0 {
            t -= 10.0;
        }
        while t < 4.0 {
            t += 10.0;
        }
        while t < len - 4.0 {
            let c = a + dir * t;
            let floor = self.ground_height(c.x, c.y);
            t += 10.0;
            if floor > -2.5 {
                continue;
            }
            for side in [-1.0f32, 1.0] {
                let at = c + side_dir * side * 2.4;
                let g = self.ground_height(at.x, at.y);
                self.oboxes.push(OBox {
                    center: at,
                    yaw: yaw + PI / 4.0,
                    half: Vec2::splat(0.8),
                    y0: g - 0.5,
                    y1: DECK_TOP - DECK_T,
                    color: CONCRETE * 0.8,
                    side_mat: Material::Concrete,
                    top_mat: Material::Concrete,
                    ..OBox::default()
                });
            }
            self.oboxes.push(OBox {
                center: c,
                yaw,
                half: Vec2::new(DECK_HALF + 0.3, 0.9),
                y0: DECK_TOP - DECK_T - 0.9,
                y1: DECK_TOP - DECK_T + 0.01,
                color: RUST,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
            let (l, r) = (c - side_dir * 2.4, c + side_dir * 2.4);
            self.segment(
                Vec3::new(l.x, floor + 3.0, l.y),
                Vec3::new(r.x, DECK_TOP - DECK_T - 0.9, r.y),
                0.1,
                STEEL,
                Material::Metal,
                false,
            );
        }
    }

    /// A small ore cart at `at` heading `yaw`: an open-top wooden tub,
    /// plank sides bound in iron, on an iron frame and four iron wheels,
    /// heaped over the rim with ore from the Crag. A plain box the size of
    /// the tub is what tanks hit and hide behind.
    pub(super) fn ore_cart(&mut self, rng: &mut Rng, at: Vec2, yaw: f32) {
        // Its detail comes from its own stream.
        let _ = rng;
        if self.in_the_way(at, 1.8) {
            return;
        }
        let rng = &mut own_rng(at);
        let g = self.stand(at);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        let part = |m: &mut Map, c: Vec3, h: Vec3, color: Vec3, mat: Material, solid: bool| {
            rig.part(m, c, h, Quat::IDENTITY, color, mat, solid, 0.0);
        };
        // The frame under the tub, and the axles.
        part(
            self,
            Vec3::new(0.0, 0.62, 0.0),
            Vec3::new(0.7, 0.1, 1.45),
            IRON,
            Material::Rust,
            false,
        );
        for z in [-1.0, 1.0] {
            let a = rig.point(Vec3::new(-0.92, 0.35, z));
            let b = rig.point(Vec3::new(0.92, 0.35, z));
            self.segment(a, b, 0.06, IRON * 0.8, Material::Metal, false);
            part(
                self,
                Vec3::new(0.0, 0.5, z),
                Vec3::new(0.75, 0.08, 0.1),
                IRON,
                Material::Rust,
                false,
            );
        }
        // Iron wheels: a flanged rim, spokes' dark between, and a hub.
        for (x, z) in [(-0.95, -1.0), (0.95, -1.0), (-0.95, 1.0), (0.95, 1.0)] {
            let c = rig.point(Vec3::new(x, 0.35, z));
            let axis = rig.rot * Vec3::X;
            self.disc(c, axis * 0.07, 0.35, IRON * 0.75, Material::Metal);
            self.disc(
                c - axis * (0.06 * x.signum()),
                axis * 0.03,
                0.42,
                IRON * 0.65,
                Material::Rust,
            );
            self.disc(
                c + axis * (0.07 * x.signum()),
                axis * 0.05,
                0.12,
                IRON * 1.2,
                Material::Metal,
            );
        }
        // The tub's floor.
        part(
            self,
            Vec3::new(0.0, 0.82, 0.0),
            Vec3::new(0.95, 0.07, 1.5),
            TIMBER * 0.7,
            Material::Wood,
            false,
        );
        // Dark inner boarding, seen through the gaps between the planks
        // (its top well under the top plank's, so they don't fight).
        for s in [-1.0f32, 1.0] {
            part(
                self,
                Vec3::new(s * 0.94, 1.33, 0.0),
                Vec3::new(0.02, 0.54, 1.5),
                TIMBER * 0.25,
                Material::Wood,
                false,
            );
            part(
                self,
                Vec3::new(0.0, 1.33, s * 1.49),
                Vec3::new(0.9, 0.54, 0.02),
                TIMBER * 0.25,
                Material::Wood,
                false,
            );
        }
        // Plank sides, three boards high, each its own shade.
        let (y0, board) = (0.75, 0.4);
        for k in 0..3 {
            let y = y0 + board * (k as f32 + 0.5);
            let h = board * 0.43;
            for s in [-1.0f32, 1.0] {
                let shade = TIMBER * rng.range(0.55, 0.75);
                part(
                    self,
                    Vec3::new(s * 0.98, y, 0.0),
                    Vec3::new(0.07, h, 1.6),
                    shade,
                    Material::Wood,
                    false,
                );
                let shade = TIMBER * rng.range(0.55, 0.75);
                part(
                    self,
                    Vec3::new(0.0, y, s * 1.53),
                    Vec3::new(0.91, h, 0.07),
                    shade,
                    Material::Wood,
                    false,
                );
            }
        }
        // Iron binding: corner posts, and a strap round the middle of each
        // side.
        for (x, z) in [(-1.0, -1.55), (1.0, -1.55), (-1.0, 1.55), (1.0, 1.55)] {
            part(
                self,
                Vec3::new(x, 1.35, z),
                Vec3::new(0.09, 0.62, 0.09),
                IRON * 0.8,
                Material::Metal,
                false,
            );
        }
        for s in [-1.0f32, 1.0] {
            for z in [-0.55, 0.55] {
                part(
                    self,
                    Vec3::new(s * 1.06, 1.35, z),
                    Vec3::new(0.02, 0.6, 0.08),
                    IRON * 0.8,
                    Material::Metal,
                    false,
                );
            }
            part(
                self,
                Vec3::new(0.0, 1.35, s * 1.61),
                Vec3::new(0.08, 0.6, 0.02),
                IRON * 0.8,
                Material::Metal,
                false,
            );
        }
        // What tanks hit: the old cart's frame and tub, exactly. The fill
        // that places cover after reads these, so the rest of the map keeps
        // its layout.
        self.hidden_part(&rig, Vec3::new(0.0, 0.5, 0.0), Vec3::new(0.9, 0.25, 1.5));
        self.hidden_part(&rig, Vec3::new(0.0, 1.35, 0.0), Vec3::new(1.05, 0.6, 1.6));
        // The ore, heaped over the rim, highest down the middle.
        self.heaped_ore(
            rng,
            &rig,
            |_| 0.88,
            1.43,
            1.8,
            0.5,
            (8, 14),
            16,
            (0.1, 0.2),
            2,
        );
    }

    /// Ore heaped in a tub (see `ore_heap`), drawn sculpted in `rig`'s
    /// frame: the mound, `lumps` loose stones between `size.0` and
    /// `size.1` across lying on it, and `crystals` lumps of raw crystal.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn heaped_ore(
        &mut self,
        rng: &mut Rng,
        rig: &Rig,
        half_w: impl Fn(f32) -> f32 + Copy,
        half_l: f32,
        y_edge: f32,
        rise: f32,
        grid: (usize, usize),
        lumps: usize,
        size: (f32, f32),
        crystals: usize,
    ) {
        let mut m = MeshData::default();
        ore_heap(&mut m, rng, half_w, half_l, y_edge, rise, grid.0, grid.1);
        let shades = [ORE * 0.8, ORE * 0.6, ORE * 0.95, MESA_TOP * 0.6];
        for k in 0..lumps + crystals {
            let (u, v) = (rng.range(-0.75, 0.75), rng.range(-0.8, 0.8));
            let z = v * half_l;
            let at = Vec3::new(u * half_w(z), heap_height(y_edge, rise, u, v), z);
            let s = rng.range(size.0, size.1);
            let (color, mat) = if k < lumps {
                (shades[rng.index(shades.len())], Material::Rubble)
            } else {
                (RAW * 1.15, Material::Ore)
            };
            ore_lump(&mut m, rng, at, s, color, mat);
        }
        self.sculpt(rig, &m);
    }

    /// A collider at `local` in `rig` (half extents `half`), not drawn: the
    /// box round something drawn sculpted.
    pub(super) fn hidden_part(&mut self, rig: &Rig, local: Vec3, half: Vec3) {
        rig.part(
            self,
            local,
            half,
            Quat::IDENTITY,
            Vec3::new(0.03, 0.07, 0.16),
            Material::Vhaal,
            true,
            0.0,
        );
        if let Some(o) = self.oboxes.last_mut() {
            o.hidden = true;
        }
    }

    /// Draws `m`, built in `rig`'s frame, into the sculpted surfaces.
    pub(super) fn sculpt(&mut self, rig: &Rig, m: &MeshData) {
        let base = self.sculpted.verts.len() as u32;
        for v in &m.verts {
            let mut v = *v;
            v.pos = rig.point(Vec3::from(v.pos)).to_array();
            v.normal = (rig.rot * Vec3::from(v.normal)).to_array();
            self.sculpted.verts.push(v);
        }
        self.sculpted
            .indices
            .extend(m.indices.iter().map(|i| i + base));
    }

    /// Ore cars under and past the tipple `s0` m along `track`, then
    /// the loco ahead.
    pub(super) fn loaded_train(&mut self, track: &[Vec2], s0: f32) {
        for (s, loco) in [(s0, false), (s0 + 10.0, false), (s0 + 22.0, true)] {
            let (p, dir) = line_at(track, s);
            self.rail_car(p, yaw_along(dir), 0.0, loco);
        }
    }

    /// A rail car of the style's (see `MineStyle::rail_car`).
    pub(super) fn rail_car(&mut self, at: Vec2, yaw: f32, roll: f32, loco: bool) {
        let st = self.st;
        st.rail_car(self, at, yaw, roll, loco);
    }

    /// A tipple over the track at `at`, which runs along `dir`.
    pub(super) fn tipple_at(&mut self, at: Vec2, dir: Vec2) {
        let lay = self.l;
        let g = self.ground_height(at.x, at.y);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw_along(dir)),
        };
        // Its detail from its own stream (the car under it has the one at
        // `at`), so nothing else on the map moves.
        let rng = &mut own_rng(at + Vec2::splat(0.5));
        // The legs' tops and the bin's floor, its rim, the hopper's throat
        // and the spout's mouth, the clearance over the rails.
        let (deck, rim, throat, mouth) = (8.0f32, 11.0f32, 6.9f32, 6.1f32);
        let (lx, lz) = (3.6f32, 5.4f32);
        let (bx, bz) = (3.9f32, 5.7f32);
        for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let foot = rig.point(Vec3::new(sx * lx, 0.0, sz * lz));
            let gf = self.ground_height(foot.x, foot.z);
            self.oboxes.push(OBox {
                center: Vec2::new(foot.x, foot.z),
                yaw: yaw_along(dir),
                half: Vec2::splat(0.3),
                y0: gf - 0.3,
                y1: g + deck,
                color: RUST,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
            // A footing under each leg.
            self.oboxes.push(OBox {
                center: Vec2::new(foot.x, foot.z),
                yaw: yaw_along(dir),
                half: Vec2::splat(0.55),
                y0: gf - 0.3,
                y1: gf + 0.35,
                color: STEEL * 1.4,
                side_mat: Material::Metal,
                top_mat: Material::Metal,
                chip: 0.08,
                ..OBox::default()
            });
        }
        let piece = |m: &mut Map, a: Vec3, b: Vec3, r: f32| {
            m.segment(rig.point(a), rig.point(b), r, RUST, Material::Rust, false);
        };
        // The long sides, outside the rails: a girt at half height and an X
        // in each bay, below it and above it.
        let girt = 4.0;
        for sx in [-1.0f32, 1.0] {
            let x = sx * (lx + 0.05);
            piece(self, Vec3::new(x, girt, -lz), Vec3::new(x, girt, lz), 0.16);
            for (y0, y1) in [(0.5, girt), (girt, deck - 0.4)] {
                piece(self, Vec3::new(x, y0, -lz), Vec3::new(x, y1, lz), 0.11);
                piece(self, Vec3::new(x, y0, lz), Vec3::new(x, y1, -lz), 0.11);
            }
        }
        // Across the track, only knee braces up under the frame, clear of
        // anything that runs under it.
        for sz in [-1.0f32, 1.0] {
            let z = sz * (lz + 0.05);
            for sx in [-1.0f32, 1.0] {
                piece(
                    self,
                    Vec3::new(sx * lx, deck - 2.0, z),
                    Vec3::new(sx * (lx - 1.8), deck - 0.4, z),
                    0.12,
                );
            }
        }
        // The frame round the legs' tops, the bin's floor sitting on it.
        for sx in [-1.0f32, 1.0] {
            rig.part(
                self,
                Vec3::new(sx * lx, deck - 0.25, 0.0),
                Vec3::new(0.3, 0.25, lz + 0.3),
                Quat::IDENTITY,
                RUST,
                Material::Rust,
                false,
                0.0,
            );
        }
        for sz in [-1.0f32, 1.0] {
            rig.part(
                self,
                Vec3::new(0.0, deck - 0.25, sz * lz),
                Vec3::new(lx + 0.3, 0.25, 0.3),
                Quat::IDENTITY,
                RUST,
                Material::Rust,
                false,
                0.0,
            );
        }
        // The bin: steel walls, the hopper under them narrowing to the
        // throat, the spout below that. What shots hit is a box round the
        // walls, as the old bin was.
        let mut m = MeshData::default();
        let inside = Vec3::new(0.0, (deck + rim) * 0.5, 0.0);
        let ring = |y: f32, hx: f32, hz: f32| {
            [
                Vec3::new(hx, y, -hz),
                Vec3::new(hx, y, hz),
                Vec3::new(-hx, y, hz),
                Vec3::new(-hx, y, -hz),
            ]
        };
        let (top, bot) = (ring(rim, bx, bz), ring(deck, bx, bz));
        let (hop, neck) = (ring(deck, bx, bz), ring(throat, 0.9, 1.1));
        let funnel = Vec3::new(0.0, deck + 2.0, 0.0);
        for i in 0..4 {
            let j = (i + 1) % 4;
            model::plate(&mut m, [bot[i], bot[j], top[j], top[i]], 0.14, inside, BIN);
            model::plate(
                &mut m,
                [neck[i], neck[j], hop[j], hop[i]],
                0.14,
                funnel,
                BIN,
            );
        }
        let spout = model::rect(Vec2::ZERO, Vec2::new(0.9, 1.1), 0.12);
        let lip = model::rect(Vec2::new(0.0, -0.3), Vec2::new(0.7, 0.8), 0.12);
        model::solid(
            &mut m,
            &[model::at_y(&lip, mouth), model::at_y(&spout, throat + 0.05)],
            0.04,
            BIN,
        );
        self.sculpt(&rig, &m);
        self.hidden_part(
            &rig,
            Vec3::new(0.0, (deck + rim) * 0.5, 0.0),
            Vec3::new(bx, (rim - deck) * 0.5, bz),
        );
        // Ribs up the walls and an angle round the rim.
        for sx in [-1.0f32, 1.0] {
            for k in 0..5 {
                let z = -bz + 0.4 + k as f32 * (2.0 * bz - 0.8) / 4.0;
                rig.part(
                    self,
                    Vec3::new(sx * (bx + 0.12), (deck + rim) * 0.5, z),
                    Vec3::new(0.1, (rim - deck) * 0.5, 0.14),
                    Quat::IDENTITY,
                    RUST,
                    Material::Rust,
                    false,
                    0.0,
                );
            }
            rig.part(
                self,
                Vec3::new(sx * (bx + 0.1), rim - 0.1, 0.0),
                Vec3::new(0.14, 0.14, bz + 0.2),
                Quat::IDENTITY,
                RUST,
                Material::Rust,
                false,
                0.0,
            );
        }
        for sz in [-1.0f32, 1.0] {
            for k in 0..3 {
                let x = -bx + 0.5 + k as f32 * (bx - 0.5);
                rig.part(
                    self,
                    Vec3::new(x, (deck + rim) * 0.5, sz * (bz + 0.12)),
                    Vec3::new(0.14, (rim - deck) * 0.5, 0.1),
                    Quat::IDENTITY,
                    RUST,
                    Material::Rust,
                    false,
                    0.0,
                );
            }
            rig.part(
                self,
                Vec3::new(0.0, rim - 0.1, sz * (bz + 0.1)),
                Vec3::new(bx + 0.2, 0.14, 0.14),
                Quat::IDENTITY,
                RUST,
                Material::Rust,
                false,
                0.0,
            );
        }
        // The ore, heaped up over the rim, highest under the conveyor's
        // head; raw crystal in it.
        let lump = |m: &mut Map, rng: &mut Rng, at: Vec3, s: f32| {
            let shade = [ORE, ORE * 0.75, ORE * 1.2, MESA_TOP * 0.8][rng.index(4)];
            let turn = Quat::from_euler(
                glam::EulerRot::YXZ,
                rng.range(0.0, TAU),
                rng.range(-0.6, 0.6),
                rng.range(-0.6, 0.6),
            );
            rig.part(
                m,
                at,
                Vec3::new(
                    s * rng.range(0.8, 1.2),
                    s * rng.range(0.55, 0.85),
                    s * rng.range(0.8, 1.2),
                ),
                turn,
                shade * rng.range(0.9, 1.1),
                Material::Rubble,
                false,
                0.0,
            );
            if let Some(o) = m.oboxes.last_mut() {
                o.rock = Some(rocks::Kind::Rubble);
            }
        };
        rig.part(
            self,
            Vec3::new(0.0, rim - 0.35, 0.0),
            Vec3::new(bx - 0.1, 0.3, bz - 0.1),
            Quat::IDENTITY,
            ORE * 0.7,
            Material::Rubble,
            false,
            0.0,
        );
        self.last_is_rock(rocks::Kind::Block);
        let heap = |x: f32, z: f32| {
            let u = (1.0 - (x / bx).powi(2)) * (1.0 - (z / bz).powi(2));
            rim - 0.2 + 1.5 * u.max(0.0)
        };
        for _ in 0..44 {
            let x = rng.range(-bx + 0.5, bx - 0.5);
            let z = rng.range(-bz + 0.5, bz - 0.5);
            let s = rng.range(0.35, 0.75);
            lump(self, rng, Vec3::new(x, heap(x, z) - s * 0.3, z), s);
        }
        for _ in 0..6 {
            let x = rng.range(-2.4, 2.4);
            let z = rng.range(-3.6, 3.6);
            let s = rng.range(0.25, 0.4);
            rig.part(
                self,
                Vec3::new(x, heap(x, z), z),
                Vec3::new(s, s * 0.7, s * 1.3),
                Quat::from_euler(glam::EulerRot::YXZ, rng.range(0.0, TAU), 0.4, 0.3),
                RAW * 1.15,
                Material::Ore,
                false,
                0.0,
            );
        }
        // Ore caught in the spout's mouth, and what's missed the cars and
        // lies along the rails.
        for _ in 0..3 {
            let at = Vec3::new(
                rng.range(-0.4, 0.4),
                mouth - rng.range(0.0, 0.25),
                -0.3 + rng.range(-0.5, 0.5),
            );
            let s = rng.range(0.25, 0.4);
            lump(self, rng, at, s);
        }
        for _ in 0..12 {
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let x = side * rng.range(1.6, 3.4);
            let z = rng.range(-lz, lz);
            let p = rig.point(Vec3::new(x, 0.0, z));
            let gp = self.ground_height(p.x, p.z) - g;
            let s = rng.range(0.2, 0.42);
            lump(self, rng, Vec3::new(x, gp + s * 0.3, z), s);
        }
        // The conveyor from beside the rig in the wash, up over the bank,
        // to its head over the heap.
        // (None without a wash under the Face to work.)
        // (None to a rig too far off for one.)
        let mut rigs = lay.lots_of(LotKind::Rig);
        let Some(digger) = rigs.find(|r| r.at().distance(at) < 60.0) else {
            return;
        };
        let (rig_at, rig_yaw) = (digger.at(), digger.footprint.yaw);
        let boom = Vec2::new(rig_yaw.sin(), rig_yaw.cos());
        let toward = (at - rig_at).normalize();
        let foot = rig_at + boom * 4.0 + toward * 3.5;
        let a = Vec3::new(foot.x, self.ground_height(foot.x, foot.y) + 1.4, foot.y);
        let b = rig.point(Vec3::new(-bx + 1.2, rim + 1.6, 0.0));
        self.segment(a, b, 0.5, STEEL, Material::Metal, false);
        if let Some(o) = self.oboxes.last_mut() {
            o.half.x = 0.9;
        }
        // Its head drum, and the ore going over it onto the heap.
        let across = (b - a).cross(Vec3::Y).normalize();
        self.segment(
            b - across * 1.0,
            b + across * 1.0,
            0.45,
            RUST,
            Material::Rust,
            false,
        );
        let ahead = (b - a).normalize();
        for k in 0..4 {
            let p = b + ahead * (0.5 + k as f32 * 0.25) + Vec3::Y * (0.4 - k as f32 * 0.35);
            let s = rng.range(0.22, 0.32);
            let at = Vec2::new(p.x, p.z) + Vec2::new(across.x, across.z) * rng.range(-0.5, 0.5);
            let yaw = rng.range(0.0, TAU);
            let color = ORE * rng.range(0.8, 1.2);
            self.oboxes.push(rocks::rubble(
                at,
                yaw,
                Vec2::splat(s),
                p.y - s,
                p.y + s,
                color,
            ));
        }
        // Its supports. The belt crosses the rails just short of the bin,
        // so its last bent stands on the frame's end beam over them, not
        // between them.
        let (la, lb) = (
            rig.rot.inverse() * (a - rig.pos),
            rig.rot.inverse() * (b - rig.pos),
        );
        let over_end = (-lz - la.z) / (lb.z - la.z);
        for f in [0.2f32, 0.42, 0.64, over_end] {
            let p = a.lerp(b, f);
            // Not on the line, the bridge's ends or the ramps (where the
            // belt crosses them it spans them).
            let q = Vec2::new(p.x, p.z);
            if f != over_end && (lay.track_distance(q) < DECK_HALF + 4.0 || self.on_way(q)) {
                continue;
            }
            let side = if f == over_end {
                rig.rot * Vec3::X
            } else {
                (b - a).cross(Vec3::Y).normalize()
            };
            for s in [-1.0f32, 1.0] {
                let top = p + side * s - Vec3::Y * 0.4;
                let foot = if f == over_end {
                    g + deck
                } else {
                    self.ground_height(top.x, top.z) - 0.3
                };
                self.segment(
                    Vec3::new(top.x, foot, top.z),
                    top,
                    0.18,
                    RUST,
                    Material::Rust,
                    true,
                );
            }
        }
    }

    /// The headframe over the hall's shaft, on the mesa: four legs and two
    /// back braces of steel, a sheave wheel on top, the hoist house behind,
    /// the shaft collar capped with concrete.
    pub(super) fn headframe(&mut self, at: Vec2) {
        let g = self.ground_height(at.x, at.y);
        let top = g + 16.0;
        let hx = 2.6;
        for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let foot = Vec3::new(at.x + sx * hx, g - 0.2, at.y + sz * hx);
            let head = Vec3::new(at.x + sx * 0.9, top, at.y + sz * 0.9);
            self.segment(foot, head, 0.22, RUST, Material::Rust, true);
        }
        for f in [0.3f32, 0.6, 0.85] {
            let y = g + (top - g) * f;
            let r = hx + (0.9 - hx) * f;
            let c = |sx: f32, sz: f32| Vec3::new(at.x + sx * r, y, at.y + sz * r);
            for (a, b) in [
                (c(-1.0, -1.0), c(1.0, -1.0)),
                (c(1.0, -1.0), c(1.0, 1.0)),
                (c(1.0, 1.0), c(-1.0, 1.0)),
                (c(-1.0, 1.0), c(-1.0, -1.0)),
            ] {
                self.segment(a, b, 0.1, RUST, Material::Rust, false);
            }
        }
        // Back braces west, toward the hoist house.
        for sz in [-1.0f32, 1.0] {
            let foot = Vec3::new(at.x - 9.0, g - 0.2, at.y + sz * 2.0);
            self.segment(
                foot,
                Vec3::new(at.x, top - 1.0, at.y + sz * 0.9),
                0.18,
                RUST,
                Material::Rust,
                true,
            );
        }
        let sheave = Vec3::new(at.x, top + 1.4, at.y);
        self.disc(
            sheave,
            Vec3::new(0.0, 0.0, 0.15),
            1.5,
            STEEL,
            Material::Metal,
        );
        self.oboxes.push(OBox {
            center: at,
            half: Vec2::splat(1.2),
            y0: top - 0.2,
            y1: top + 0.25,
            color: STEEL,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        let house = Vec2::new(at.x - 15.0, at.y);
        self.segment(
            sheave - Vec3::Y * 1.5,
            Vec3::new(at.x, g + 0.5, at.y),
            0.04,
            STEEL * 0.6,
            Material::Rust,
            false,
        );
        self.segment(
            sheave,
            Vec3::new(house.x + 3.2, g + 3.6, house.y),
            0.04,
            STEEL * 0.6,
            Material::Rust,
            false,
        );
        self.oboxes.push(OBox {
            center: at,
            half: Vec2::splat(1.6),
            y0: g - 0.3,
            y1: g + 0.22,
            color: CONCRETE * 0.9,
            side_mat: Material::Concrete,
            top_mat: Material::Concrete,
            ramp: true,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: house,
            half: Vec2::new(3.2, 2.6),
            y0: g - 0.3,
            y1: g + 4.2,
            color: RUST,
            side_mat: Material::Painted,
            top_mat: Material::Rust,
            ..OBox::default()
        });
        self.sound(Ambient::Cable, sheave);
    }

    /// The warehouse at the end of the branch: a plain steel shed on a
    /// concrete floor, pitched roof on portal frames, a rail door at the
    /// junction's end, a door at the far end and one in each side, all big
    /// enough to drive through. Open-top bins of crystal-rich ore stand
    /// along both walls, a hopper at the buffer stop. Only the plasma lamps,
    /// and the emblem small on the front, say whose it is.
    pub(super) fn warehouse(&mut self, rng: &mut Rng, lot: &Lot) {
        let st = self.st;
        let at = lot.at();
        let along = lot.footprint.axes().1;
        let g = self.ground_height(at.x, at.y);
        let yaw = yaw_along(along);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        let id = Quat::IDENTITY;
        let (hw, hl) = (SHED_HALF.x, SHED_HALF.y);
        let (eaves, ridge) = (SHED_EAVES, SHED_RIDGE);
        // Wall half-thickness; the doors' half-widths and height.
        let t = 0.2;
        let (end_door, side_door, door_h) = (5.5, 3.5, 6.0);
        let clad = Vec3::new(0.58, 0.56, 0.5);
        let dado = Vec3::new(0.42, 0.36, 0.3);
        let frame = Vec3::new(0.3, 0.31, 0.33);
        // The floor.
        rig.part(
            self,
            Vec3::new(0.0, -0.14, 0.0),
            Vec3::new(hw + 0.9, 0.18, hl + 0.9),
            id,
            CONCRETE,
            Material::Concrete,
            false,
            0.0,
        );
        // A wall panel: from `a` to `b` (local x, z) along the wall, `y0` to
        // `y1` up, `half_t` thick each side, with a grimy band low down.
        let panel = |m: &mut Map, a: Vec2, b: Vec2, y0: f32, y1: f32| {
            let mid = (a + b) * 0.5;
            let half = ((b - a).abs() * 0.5).max(Vec2::splat(t));
            rig.part(
                m,
                Vec3::new(mid.x, (y0 + y1) * 0.5, mid.y),
                Vec3::new(half.x, (y1 - y0) * 0.5, half.y),
                id,
                clad * (1.0 + 0.03 * (mid.x * 1.7 + mid.y * 0.9).sin()),
                Material::Painted,
                true,
                0.0,
            );
            if y0 < 0.5 {
                let grow = Vec2::splat(0.04);
                let h = ((b - a).abs() * 0.5).max(Vec2::splat(t)) + grow;
                rig.part(
                    m,
                    Vec3::new(mid.x, 0.55, mid.y),
                    Vec3::new(h.x, 0.65, h.y),
                    id,
                    dado,
                    Material::Concrete,
                    false,
                    0.0,
                );
            }
        };
        // The sides, a door in the middle of each.
        for sx in [-1.0f32, 1.0] {
            let x = sx * hw;
            panel(
                self,
                Vec2::new(x, -hl),
                Vec2::new(x, -side_door),
                0.0,
                eaves,
            );
            panel(self, Vec2::new(x, side_door), Vec2::new(x, hl), 0.0, eaves);
            panel(
                self,
                Vec2::new(x, -side_door),
                Vec2::new(x, side_door),
                door_h,
                eaves,
            );
        }
        // The ends, a door in the middle of each.
        for sz in [-1.0f32, 1.0] {
            let z = sz * hl;
            let w = hw - t;
            panel(self, Vec2::new(-w, z), Vec2::new(-end_door, z), 0.0, eaves);
            panel(self, Vec2::new(end_door, z), Vec2::new(w, z), 0.0, eaves);
            panel(
                self,
                Vec2::new(-end_door, z),
                Vec2::new(end_door, z),
                door_h,
                eaves,
            );
            // The gable over it: a plate under each slope of the roof, its
            // lower edge hidden in the wall.
            let rise = ridge - eaves;
            let a = (rise / hw).atan();
            let len = (hw * hw + rise * rise).sqrt();
            let d = rise / a.cos() + 0.2;
            for (sx, th) in [(1.0f32, 0.15), (-1.0, 0.13)] {
                let n = Vec2::new(-sx * a.sin(), -a.cos());
                let mid = Vec2::new(sx * hw * 0.5, (eaves + ridge) * 0.5) + n * (d * 0.5);
                rig.part(
                    self,
                    Vec3::new(mid.x, mid.y, z),
                    Vec3::new(len * 0.5, d * 0.5, th),
                    Quat::from_rotation_z(-sx * a),
                    clad * 0.97,
                    Material::Painted,
                    false,
                    0.0,
                );
            }
        }
        // The roof: two slopes overhanging the walls, a cap along the ridge.
        let rise = ridge - eaves;
        let a = (rise / hw).atan();
        let over = 0.8;
        let len = ((hw + over) * (hw + over) + (rise + over * a.tan()).powi(2)).sqrt();
        for sx in [-1.0f32, 1.0] {
            let up = Vec2::new(sx * a.sin(), a.cos()) * 0.13;
            let mid = Vec2::new(
                sx * (hw + over) * 0.5,
                ridge - (rise + over * a.tan()) * 0.5,
            ) + up;
            rig.part(
                self,
                Vec3::new(mid.x, mid.y, 0.0),
                Vec3::new(len * 0.5, 0.12, hl + 0.7),
                Quat::from_rotation_z(-sx * a),
                Vec3::new(0.44, 0.44, 0.43),
                Material::Painted,
                true,
                0.0,
            );
        }
        rig.part(
            self,
            Vec3::new(0.0, ridge + 0.28, 0.0),
            Vec3::new(0.35, 0.1, hl + 0.75),
            id,
            frame,
            Material::Metal,
            false,
            0.0,
        );
        // Portal frames inside: columns along the walls, a tie across at
        // the eaves, rafters up under the roof. Plasma lamps hang from the
        // ties over the aisles.
        let mut z = -hl + 6.0;
        while z < hl - 1.0 {
            for sx in [-1.0f32, 1.0] {
                if z.abs() > side_door + 0.4 {
                    rig.part(
                        self,
                        Vec3::new(sx * (hw - t - 0.25), eaves * 0.5, z),
                        Vec3::new(0.25, eaves * 0.5, 0.2),
                        id,
                        frame,
                        Material::Metal,
                        true,
                        0.0,
                    );
                }
                let e = rig.point(Vec3::new(sx * (hw - t), eaves - 0.4, z));
                let r = rig.point(Vec3::new(0.0, ridge - 0.4, z));
                self.segment(e, r, 0.18, frame, Material::Metal, false);
            }
            let (a, b) = (
                rig.point(Vec3::new(-(hw - t), eaves - 0.5, z)),
                rig.point(Vec3::new(hw - t, eaves - 0.5, z)),
            );
            self.segment(a, b, 0.12, frame, Material::Metal, false);
            for x in [-4.5f32, 4.5] {
                st.hanging_lamp(self, rig.point(Vec3::new(x, eaves - 0.6, z)), 1.0, 1.0);
            }
            z += 6.0;
        }
        // Outside: a rolled-up door drum over each door, a plasma lamp
        // either side of it.
        for (c, out, w) in [
            (Vec2::new(0.0, -hl), Vec2::new(0.0, -1.0), end_door),
            (Vec2::new(0.0, hl), Vec2::new(0.0, 1.0), end_door),
            (Vec2::new(-hw, 0.0), Vec2::new(-1.0, 0.0), side_door),
            (Vec2::new(hw, 0.0), Vec2::new(1.0, 0.0), side_door),
        ] {
            let face = c + out * (t + 0.4);
            let across = out.perp();
            let (a, b) = (face - across * (w + 0.3), face + across * (w + 0.3));
            self.pipe(
                rig.point(Vec3::new(a.x, door_h + 0.45, a.y)),
                rig.point(Vec3::new(b.x, door_h + 0.45, b.y)),
                0.42,
                frame * 1.2,
                Material::Painted,
                false,
            );
            for s in [-1.0f32, 1.0] {
                let p = c + out * (t + 0.02) + across * s * (w + 1.2);
                let dir = rig.rot * Vec3::new(out.x, 0.0, out.y);
                st.wall_lamp(
                    self,
                    rig.point(Vec3::new(p.x, door_h - 0.2, p.y)),
                    Vec2::new(dir.x, dir.z),
                    1.0,
                );
            }
        }
        // The emblem, small, high on the front over the rail door: plasma
        // held in crystal, an orb in an almond-shaped lens.
        st.emblem(self, &rig, Vec3::new(0.0, eaves + 1.4, -hl - 0.2), 0.7);
        // The bins of ore along the walls (a gap at each side door), one
        // left out in the aisle by the rail door.
        let colours = [
            Vec3::new(0.52, 0.3, 0.16),
            Vec3::new(0.55, 0.47, 0.25),
            Vec3::new(0.38, 0.4, 0.41),
            Vec3::new(0.3, 0.37, 0.31),
        ];
        let mut spots = Vec::new();
        for sx in [-1.0f32, 1.0] {
            for z in [-14.2f32, -8.4, 8.4, 14.2] {
                spots.push((Vec2::new(sx * 9.0, z), rng.range(-0.04, 0.04)));
            }
        }
        spots.push((Vec2::new(-4.6, -9.0), 0.45));
        for (p, turn) in spots {
            let w = rig.point(Vec3::new(p.x, 0.0, p.y));
            let c = colours[rng.index(colours.len())];
            self.ore_bin(rng, Vec2::new(w.x, w.z), yaw + turn, c);
        }
        // A hopper at the buffer stop, and the stop.
        let car = rig.point(Vec3::new(0.0, 0.0, SHED_BUFFER - 5.4));
        self.rail_car(Vec2::new(car.x, car.z), yaw, 0.0, false);
        rig.part(
            self,
            Vec3::new(0.0, 0.7, SHED_BUFFER + 0.3),
            Vec3::new(1.6, 0.7, 0.3),
            id,
            RUST,
            Material::Rust,
            true,
            0.0,
        );
        for sx in [-1.0f32, 1.0] {
            rig.part(
                self,
                Vec3::new(sx * GAUGE * 0.5, 0.85, SHED_BUFFER - 0.15),
                Vec3::new(0.22, 0.22, 0.2),
                id,
                STEEL,
                Material::Metal,
                false,
                0.0,
            );
        }
    }

    /// An open-top steel bin, tank high, heaped with crystal-rich ore:
    /// broken red rock with raw crystal through it.
    pub(super) fn ore_bin(&mut self, rng: &mut Rng, at: Vec2, yaw: f32, colour: Vec3) {
        let [stone, dark, _] = self.loose;
        let g = self.ground_height(at.x, at.y);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        let id = Quat::IDENTITY;
        let (hx, hz, h, t) = (1.5f32, 2.4f32, 1.8f32, 0.08f32);
        let paint = colour * rng.range(0.9, 1.05);
        // Floor, sides and ends, a rolled rim on top.
        rig.part(
            self,
            Vec3::new(0.0, 0.12, 0.0),
            Vec3::new(hx, 0.12, hz),
            id,
            paint * 0.8,
            Material::Painted,
            true,
            0.0,
        );
        for s in [-1.0f32, 1.0] {
            rig.part(
                self,
                Vec3::new(s * (hx - t), h * 0.5 + 0.12, 0.0),
                Vec3::new(t, h * 0.5, hz),
                id,
                paint,
                Material::Painted,
                true,
                0.0,
            );
            rig.part(
                self,
                Vec3::new(0.0, h * 0.5 + 0.12, s * (hz - t)),
                Vec3::new(hx - 2.0 * t, h * 0.5, t),
                id,
                paint,
                Material::Painted,
                true,
                0.0,
            );
            rig.part(
                self,
                Vec3::new(s * (hx - t), h + 0.16, 0.0),
                Vec3::new(t + 0.05, 0.06, hz + 0.05),
                id,
                paint * 0.85,
                Material::Painted,
                false,
                0.0,
            );
            // Ribs down the sides, a lifting lug at each end.
            for rz in [-1.2f32, 0.0, 1.2] {
                rig.part(
                    self,
                    Vec3::new(s * (hx + 0.05), h * 0.5 + 0.12, rz),
                    Vec3::new(0.05, h * 0.5, 0.09),
                    id,
                    paint * 0.9,
                    Material::Painted,
                    false,
                    0.0,
                );
            }
            rig.part(
                self,
                Vec3::new(0.0, h + 0.05, s * (hz + 0.1)),
                Vec3::new(0.3, 0.12, 0.08),
                id,
                STEEL,
                Material::Metal,
                false,
                0.0,
            );
        }
        // The ore, heaped just under the rim.
        let top = h + 0.02;
        for _ in 0..7 {
            let p = Vec3::new(
                rng.range(-hx + 0.5, hx - 0.5),
                top - rng.range(0.25, 0.5),
                rng.range(-hz + 0.5, hz - 0.5),
            );
            let s = rng.range(0.25, 0.5);
            let tip = Quat::from_euler(
                glam::EulerRot::YXZ,
                rng.range(0.0, TAU),
                rng.range(-0.5, 0.5),
                rng.range(-0.5, 0.5),
            );
            let shade = rng.range(0.75, 1.05);
            rig.part(
                self,
                p,
                Vec3::new(s, s * 0.7, s * 1.2),
                tip,
                stone * shade,
                Material::Rubble,
                false,
                0.0,
            );
            self.last_is_rock(rocks::Kind::Rubble);
        }
        rig.part(
            self,
            Vec3::new(0.0, top - 0.55, 0.0),
            Vec3::new(hx - 2.0 * t - 0.02, 0.3, hz - 2.0 * t - 0.02),
            id,
            dark,
            Material::Rubble,
            false,
            0.0,
        );
        self.last_is_rock(rocks::Kind::Block);
        for _ in 0..4 {
            let p = Vec3::new(
                rng.range(-hx + 0.4, hx - 0.4),
                top - 0.1,
                rng.range(-hz + 0.4, hz - 0.4),
            );
            let tip = Quat::from_euler(
                glam::EulerRot::YXZ,
                rng.range(0.0, TAU),
                rng.range(-0.6, 0.6),
                rng.range(-0.6, 0.6),
            );
            let s = rng.range(0.1, 0.18);
            rig.part(
                self,
                p,
                Vec3::new(s, s * 2.6, s),
                tip,
                RAW * (0.85 + (s - 0.1) * 3.75),
                Material::Ore,
                false,
                0.0,
            );
        }
    }

    /// Nothing to keep clear for driving within `r` of `at`: no way round,
    /// ford, track, or the run out of the warehouse's doors.
    pub(super) fn room(&self, at: Vec2, r: f32) -> bool {
        let lay = self.l;
        (0..9).all(|k| {
            let q = if k == 0 {
                at
            } else {
                let a = k as f32 / 8.0 * TAU;
                at + Vec2::new(a.cos(), a.sin()) * r
            };
            !self.on_way(q)
                && lay.track_distance(q) > 6.0
                && lay.shed_distance(q) > 14.0
                && !lay
                    .land
                    .fords
                    .iter()
                    .any(|&(s, e, h)| seg(q, s, e).0 < h + 4.0)
        })
    }

    /// Something `r` across at `p` would be in the way of driving: on or
    /// by one of the bots' ways round (its points or the stretches between
    /// them), a haul ramp, a crossing or a way up. Loose cover (boulders,
    /// carts, cribs, crystal) is never put there.
    pub(super) fn in_the_way(&self, p: Vec2, r: f32) -> bool {
        let lay = self.l;
        let flat = |w: &Vec3| Vec2::new(w.x, w.z);
        self.routes.iter().any(|way| {
            way.iter().any(|w| flat(w).distance(p) < r + 3.5)
                || way
                    .windows(2)
                    .any(|w| seg(p, flat(&w[0]), flat(&w[1])).0 < r + 3.0)
        }) || lay
            .land
            .fords
            .iter()
            .any(|&(a, b, h)| seg(p, a, b).0 < r + h + 1.5)
            || (0..9).any(|k| {
                // Nor where a side spawns.
                let a = k as f32 * TAU / 8.0;
                let q = if k == 0 {
                    p
                } else {
                    p + Vec2::new(a.cos(), a.sin()) * (r + 2.0)
                };
                lay.bases
                    .iter()
                    .any(|b| b.middle().distance(q) < b.size * 0.5)
            })
    }

    /// Near a point on one of the ways round the bots take, or the Hold
    /// site (kept open).
    pub(super) fn on_way(&self, q: Vec2) -> bool {
        self.routes
            .iter()
            .flatten()
            .any(|w| Vec2::new(w.x, w.z).distance(q) < 4.5)
            || self.hold_site.is_some_and(|h| h.distance(q) < 7.0)
    }
}

/// A cutting rig at `at` on ground at `floor`, its boom reaching out
/// along `yaw`: a painted steel body on four crawler units, a cab on
/// top, a toothed cutting drum at the end of the boom.
pub(in crate::map::procgen) fn cutting_rig(map: &mut Map, at: Vec2, yaw: f32, floor: f32) {
    let rig = Rig {
        pos: Vec3::new(at.x, floor, at.y),
        rot: Quat::from_rotation_y(yaw),
    };
    for (x, z) in [(-1.8, -2.2), (1.8, -2.2), (-1.8, 2.2), (1.8, 2.2)] {
        rig.part(
            map,
            Vec3::new(x, 0.45, z),
            Vec3::new(0.8, 0.3, 1.0),
            Quat::IDENTITY,
            STEEL,
            Material::Metal,
            true,
            0.0,
        );
        // The crawler's track, down on the ground under it.
        rig.part(
            map,
            Vec3::new(x, 0.12, z),
            Vec3::new(0.85, 0.12, 1.05),
            Quat::IDENTITY,
            STEEL * 0.6,
            Material::Metal,
            false,
            0.0,
        );
    }
    rig.part(
        map,
        Vec3::new(0.0, 1.7, 0.0),
        Vec3::new(2.2, 0.95, 3.2),
        Quat::IDENTITY,
        MACHINE,
        Material::Painted,
        true,
        0.0,
    );
    rig.part(
        map,
        Vec3::new(0.0, 3.1, -1.6),
        Vec3::new(1.4, 0.5, 1.2),
        Quat::IDENTITY,
        STEEL,
        Material::Metal,
        true,
        0.0,
    );
    let root = Vec3::new(0.0, 2.9, 2.6);
    let elbow = Vec3::new(0.0, 5.0, 6.0);
    let head = Vec3::new(0.0, 1.2, 8.5);
    for x in [-0.6, 0.6] {
        let o = Vec3::new(x, 0.0, 0.0);
        map.segment(
            rig.point(root + o),
            rig.point(elbow + o),
            0.28,
            STEEL,
            Material::Metal,
            false,
        );
        map.segment(
            rig.point(elbow + o),
            rig.point(head + o),
            0.24,
            STEEL,
            Material::Metal,
            false,
        );
    }
    let hc = rig.point(head);
    let axle = rig.rot * Vec3::X * 0.35;
    map.disc(hc, axle, 1.3, STEEL * 1.2, Material::Metal);
    map.disc(hc, axle * 1.25, 0.6, RUST, Material::Rust);
    // Picks round the drum.
    for k in 0..10 {
        let a = k as f32 * TAU / 10.0;
        let out = rig.rot * Vec3::new(0.0, a.cos(), a.sin());
        map.segment(
            hc + out * 1.2,
            hc + out * 1.55,
            0.09,
            STEEL * 0.7,
            Material::Metal,
            false,
        );
    }
}

/// A stream of its own for the thing at `at`.
pub(super) fn own_rng(at: Vec2) -> Rng {
    Rng::new(((at.x.to_bits() as u64) << 32) | at.y.to_bits() as u64)
}

/// The tipple's bin: plain mine steel, edges worn bright.
const BIN: Skin = Skin::new(
    Vec4::new(STEEL.x, STEEL.y, STEEL.z, Material::Metal.id()),
    Vec4::new(0.34, 0.33, 0.33, Material::Metal.id()),
);

/// Ore from the Crag's red beds.
pub(super) const ORE: Vec3 = Vec3::new(0.36, 0.15, 0.09);

/// A heap of broken ore filling a tub, in the tub's frame: a rough mound
/// of small flat facets, each its own shade of Crag rock, from `y_edge`
/// round the inside of the rim up to `y_edge + rise` in the middle. Its
/// outline is `half_w(z)` across at each `z`, `half_l` long.
#[allow(clippy::too_many_arguments)]
pub(super) fn ore_heap(
    m: &mut MeshData,
    rng: &mut Rng,
    half_w: impl Fn(f32) -> f32,
    half_l: f32,
    y_edge: f32,
    rise: f32,
    nu: usize,
    nv: usize,
) {
    let shades = [ORE * 0.9, ORE * 0.7, ORE * 0.55, ORE * 1.0, MESA_TOP * 0.6];
    // The grid's points, jittered so no two facets lie the same way.
    let mut pts = vec![Vec3::ZERO; (nu + 1) * (nv + 1)];
    let (cu, cv) = (1.0 / nu as f32, 1.0 / nv as f32);
    for j in 0..=nv {
        for i in 0..=nu {
            let edge = i == 0 || i == nu || j == 0 || j == nv;
            let mut u = i as f32 / nu as f32 * 2.0 - 1.0;
            let mut v = j as f32 / nv as f32 * 2.0 - 1.0;
            if !edge {
                u += rng.range(-0.35, 0.35) * 2.0 * cu;
                v += rng.range(-0.35, 0.35) * 2.0 * cv;
            }
            let z = v * half_l;
            let x = u * half_w(z);
            let mound = (1.0 - u * u).max(0.0).sqrt() * (1.0 - v * v).max(0.0).powf(0.6);
            let y = if edge {
                y_edge
            } else {
                y_edge + rise * mound + rise * rng.range(-0.14, 0.14) * (0.4 + mound)
            };
            pts[j * (nu + 1) + i] = Vec3::new(x, y, z);
        }
    }
    let face = |m: &mut MeshData, a: Vec3, b: Vec3, c: Vec3, rng: &mut Rng| {
        let mut n = (b - a).cross(c - a).normalize_or_zero();
        if n.y < 0.0 {
            n = -n;
        }
        let shade = shades[rng.index(shades.len())] * rng.range(0.85, 1.1);
        m.tri([a, b, c], n, shade.extend(Material::Rubble.id()));
    };
    for j in 0..nv {
        for i in 0..nu {
            let p = |i: usize, j: usize| pts[j * (nu + 1) + i];
            let (a, b, c, d) = (p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1));
            // Split each cell either way, so the facets don't line up.
            if rng.chance(0.5) {
                face(m, a, b, c, rng);
                face(m, a, c, d, rng);
            } else {
                face(m, a, b, d, rng);
                face(m, b, c, d, rng);
            }
        }
    }
}

/// Where an `ore_heap`'s mound stands at (u, v), without its roughness.
pub(super) fn heap_height(y_edge: f32, rise: f32, u: f32, v: f32) -> f32 {
    y_edge + rise * (1.0 - u * u).max(0.0).sqrt() * (1.0 - v * v).max(0.0).powf(0.6)
}

/// One lump of rock (or raw crystal) about `s` across at `c`: a broken,
/// many-faced stone, sunk a little into whatever it sits on.
pub(super) fn ore_lump(
    m: &mut MeshData,
    rng: &mut Rng,
    c: Vec3,
    s: f32,
    color: Vec3,
    mat: Material,
) {
    let turn = Quat::from_euler(
        glam::EulerRot::YXZ,
        rng.range(0.0, TAU),
        rng.range(-0.5, 0.5),
        rng.range(-0.5, 0.5),
    );
    let squash = Vec3::new(
        rng.range(0.8, 1.2),
        rng.range(0.5, 0.75),
        rng.range(0.8, 1.2),
    ) * s;
    let dirs = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ];
    let p: Vec<Vec3> = dirs
        .iter()
        .map(|d| c + turn * (*d * squash * rng.range(0.75, 1.15)))
        .collect();
    // An octahedron's eight faces: each pairs an x end and a z end with
    // the top or the bottom.
    for (a, b) in [(0, 4), (4, 1), (1, 5), (5, 0)] {
        for t in [2, 3] {
            let (pa, pb, pt) = (p[a], p[b], p[t]);
            let mut n = (pb - pa).cross(pt - pa).normalize_or_zero();
            if n.dot((pa + pb + pt) / 3.0 - c) < 0.0 {
                n = -n;
            }
            let shade = color * rng.range(0.85, 1.1);
            m.tri([pa, pb, pt], n, shade.extend(mat.id()));
        }
    }
}

impl Map {}
