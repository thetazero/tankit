//! The Imperium's style: their cities, half ruined by the war, still kept
//! by ritual. Roman scale in raw concrete: mass-made monuments, ritual for
//! its own sake (see docs/setting.md).
//!
//! - the landmark: the rotunda, a domed basilica in the Roman way (a drum
//!   of concrete, an oculus in the dome), fallen in on one side, the emblem
//!   on its dais inside, a ring of columns round it on its precinct.
//! - gates: triumphal arches. Monuments: robed statues on plinths. Masts:
//!   crimson banners on gold crossbars. Fountains: dry basins round broken
//!   obelisks.
//! - lots: ordinary concrete blocks (many shelled), brutalist slabs on
//!   pillars, lots of rubble. The edge: Imperium blocks shoulder to
//!   shoulder, rubble banked across each avenue's end.
//! - bases: staging courts behind bastion walls, in each side's colors.
//! - beyond: the city goes on to the haze, a great tower hung with crimson
//!   closing each avenue's vista.
//! - their shield generator (`shield`; see `procgen::shield`).

mod civics;
mod great;
pub(super) mod shield;

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Mat3, Quat, Vec2, Vec3};

use super::skeleton::*;
use super::{Centerpiece, Style};
use crate::faction::Faction;
use crate::map::landmarks::Rig;
use crate::map::rocks;
use crate::map::ruins::{self, Drift};
use crate::map::{Ambient, Map, OBox, Terrain};
use crate::render::material::Material;
use crate::rng::Rng;

const GROUND: Vec3 = Vec3::new(0.32, 0.32, 0.33);
const CONCRETE: Vec3 = Vec3::new(0.54, 0.53, 0.51);
const CONCRETE_DARK: Vec3 = Vec3::new(0.3, 0.3, 0.3);
const STONE: Vec3 = Vec3::new(0.86, 0.83, 0.76);
const CRIMSON: Vec3 = Vec3::new(0.46, 0.05, 0.06);
const GOLD: Vec3 = Vec3::new(0.86, 0.66, 0.28);

/// The rotunda's drum: wall thickness, height.
const DRUM_T: f32 = 2.6;
const DRUM_H: f32 = 16.0;

/// The ordinary blocks' walls: raw concrete, travertine, stained brown,
/// dark grey and an ochre render, dark enough to keep their colour in the
/// sun.
const CONCRETES: [Vec3; 7] = [
    Vec3::new(0.46, 0.45, 0.43),
    Vec3::new(0.38, 0.38, 0.39),
    Vec3::new(0.56, 0.5, 0.41),
    Vec3::new(0.42, 0.36, 0.31),
    Vec3::new(0.3, 0.3, 0.32),
    Vec3::new(0.52, 0.41, 0.3),
    Vec3::new(0.5, 0.48, 0.44),
];
/// The ordinary blocks: cast concrete in greys.
const CAST: ruins::Style = ruins::Style {
    colors: &CONCRETES,
    wall: Material::Concrete,
};

/// The four directions on the map: east, north, west, south.
const AXES: [Vec2; 4] = [
    Vec2::new(1.0, 0.0),
    Vec2::new(0.0, 1.0),
    Vec2::new(-1.0, 0.0),
    Vec2::new(0.0, -1.0),
];

/// A rotation whose local x, y, z axes are `x`, `y` and their cross.
pub(super) fn frame(x: Vec3, y: Vec3) -> Quat {
    let z = x.cross(y);
    Quat::from_mat3(&Mat3::from_cols(x, y, z))
}

/// The angle of a direction on the ground plane.
fn angle(d: Vec2) -> f32 {
    d.y.atan2(d.x)
}

/// How far round from `a` to `b`, radians, either way (0..PI).
fn apart(a: f32, b: f32) -> f32 {
    ((a - b + PI).rem_euclid(TAU) - PI).abs()
}

pub(super) struct Imperium;

impl Style for Imperium {
    fn ground(&self) -> Vec3 {
        GROUND
    }

    fn road(&self) -> Vec3 {
        Vec3::new(0.27, 0.27, 0.29)
    }

    fn stone(&self) -> super::civic::Stone {
        super::civic::Stone {
            wall: CONCRETE,
            mat: Material::Concrete,
            trim: STONE * 0.9,
            roof: CONCRETE_DARK,
            land: GROUND,
        }
    }

    fn civic(&self, map: &mut Map, lot: &Lot, kind: CivicKind, rng: &mut Rng) {
        civics::build(map, rng, lot, kind, self.stone());
    }

    fn hold_name(&self, piece: Centerpiece, kind: LandmarkKind) -> &'static str {
        match piece {
            Centerpiece::Monument => match kind {
                LandmarkKind::Great(1) => "AMPHITHEATRE",
                LandmarkKind::Great(2) => "FORUM",
                _ => "ROTUNDA",
            },
            Centerpiece::ShieldGenerator(..) => "GENERATOR",
        }
    }

    fn landmark(&self, map: &mut Map, sk: &Skeleton, lm: &Landmark, rng: &mut Rng) -> Vec<Drift> {
        match lm.kind {
            LandmarkKind::Great(1) => {
                return great::amphitheatre(map, rng, lm, sk.ground(lm.center));
            }
            LandmarkKind::Great(2) => return great::forum(map, rng, lm, sk.ground(lm.center)),
            _ => {}
        }
        let drifts = map.rotunda(rng, lm, sk.ground(lm.center));
        map.colonnade(rng, lm, sk.ground(lm.center));
        drifts
    }

    fn platform(&self, map: &mut Map, sk: &Skeleton, p: &Platform, _rng: &mut Rng) {
        map.paving(p, &sk.landmarks, STONE);
    }

    fn street(&self, map: &mut Map, sk: &Skeleton, s: &Street, _rng: &mut Rng) {
        let _ = (map, sk, s);
    }

    fn span(&self, map: &mut Map, sk: &Skeleton, s: &Span, rng: &mut Rng) {
        map.arch(rng, s, sk.ground(s.at));
    }

    fn lot(&self, map: &mut Map, sk: &Skeleton, lot: &Lot, rng: &mut Rng) -> Vec<Drift> {
        match lot.kind {
            LotKind::Big | LotKind::Small => {
                ruins::building_in(map, rng, lot.footprint, lot.front, &|_| true, &CAST)
            }
            LotKind::Raised => {
                map.brutalist(rng, &lot.footprint, lot.front);
                Vec::new()
            }
            LotKind::Rubble => {
                map.rubble_lot(rng, &lot.footprint, CONCRETE);
                Vec::new()
            }
            LotKind::Civic(_) | LotKind::Spires | LotKind::Fins | LotKind::Boulders => Vec::new(),
            LotKind::Distant => {
                map.distant_block(rng, &lot.footprint, sk);
                Vec::new()
            }
        }
    }

    fn base(&self, map: &mut Map, sk: &Skeleton, side: usize, b: &Base, rng: &mut Rng) {
        map.staging_court(rng, side, b, sk.center(), CONCRETE);
    }

    fn edge(&self, map: &mut Map, _sk: &Skeleton, e: &Edge, rng: &mut Rng) {
        match e.kind {
            EdgeKind::Barricade => map.street_barricade(rng, e, CONCRETE),
            EdgeKind::Blockade => {
                map.rubble_bank(rng, e.start, e.along, e.inward, (e.from, e.to), CONCRETE)
            }
        }
    }

    fn deco(&self, map: &mut Map, sk: &Skeleton, d: &Deco, rng: &mut Rng) {
        let g = sk.ground(d.at);
        match d.kind {
            DecoKind::Monument => map.statue(rng, d.at, d.face, g),
            DecoKind::Mast => map.mast(d.at, d.face, g),
            DecoKind::Fountain => map.fountain(rng, d.at, g),
            DecoKind::Cover => map.cover(rng, d.at, d.face, g),
        }
    }

    fn beyond(&self, map: &mut Map, sk: &Skeleton, rng: &mut Rng) {
        map.skyline(rng, sk);
    }

    fn finish(&self, map: &mut Map) {
        for o in &mut map.oboxes {
            let masonry =
                [Material::Plaster, Material::Concrete, Material::Rubble].contains(&o.side_mat);
            if o.solid && o.tilt == 0.0 && o.chip == 0.0 && masonry {
                o.chip = 0.25;
            }
        }
    }
}

impl Map {
    /// A plain box from `lo` to `hi` (axis aligned), worn at the edges.
    #[allow(clippy::too_many_arguments)]
    fn slab(&mut self, lo: Vec3, hi: Vec3, yaw: f32, color: Vec3, mat: Material, solid: bool) {
        let c = (lo + hi) * 0.5;
        self.oboxes.push(OBox {
            center: Vec2::new(c.x, c.z),
            yaw,
            half: Vec2::new(hi.x - lo.x, hi.z - lo.z) * 0.5,
            y0: lo.y,
            y1: hi.y,
            color,
            side_mat: mat,
            top_mat: mat,
            solid,
            ..OBox::default()
        });
    }

    /// The emblem, big, standing upright facing `out` at `at` (its center):
    /// a gold disc of radius `r` under a halo arch. Visual only.
    fn standing_emblem(&mut self, at: Vec3, out: Vec2, r: f32) {
        let n = Vec3::new(out.x, 0.0, out.y).normalize();
        let disc_c = at - Vec3::Y * r * 0.3;
        self.disc(disc_c, n * 0.15, r, GOLD, Material::Metal);
        // The halo: short plates round an arch over the disc.
        let right = Vec3::Y.cross(n).normalize();
        let (ar, segs) = (r * 1.55, 14);
        for k in 0..segs {
            let s = (k as f32 + 0.5) / segs as f32;
            let a = PI * (0.08 + 0.84 * s);
            let w = 0.3 * r * (PI * s).sin().sqrt().max(0.25);
            let radial = right * a.cos() + Vec3::Y * a.sin();
            let tangent = n.cross(radial).normalize();
            let c = disc_c + radial * ar;
            let rig = Rig {
                pos: c,
                rot: frame(tangent, radial),
            };
            rig.part(
                self,
                Vec3::ZERO,
                Vec3::new(ar * PI * 0.84 / segs as f32 * 0.56, w * 0.5, 0.15),
                Quat::IDENTITY,
                GOLD,
                Material::Metal,
                false,
                0.0,
            );
        }
    }

    /// A crimson banner hanging flat against a wall facing `out`, from
    /// `top` down `len`, `w` wide, with a small gold disc near the top.
    pub(in crate::map) fn banner(&mut self, top: Vec3, out: Vec2, w: f32, len: f32) {
        self.side_banner(top, out, w, len, CRIMSON, true);
    }

    /// A banner in `color`; `emblem` puts the Imperium's disc on it (only
    /// theirs carry it).
    pub(super) fn side_banner(
        &mut self,
        top: Vec3,
        out: Vec2,
        w: f32,
        len: f32,
        color: Vec3,
        emblem: bool,
    ) {
        self.sound(Ambient::Banner, top - Vec3::Y * len * 0.4);
        let n = Vec3::new(out.x, 0.0, out.y).normalize();
        let right = Vec3::Y.cross(n).normalize();
        let rig = Rig {
            pos: top + n * 0.12 - Vec3::Y * len * 0.5,
            rot: frame(right, Vec3::Y),
        };
        rig.part(
            self,
            Vec3::ZERO,
            Vec3::new(w * 0.5, len * 0.5, 0.05),
            Quat::IDENTITY,
            color,
            Material::Cloth,
            false,
            0.0,
        );
        // Gold bar across the top, and the disc.
        rig.part(
            self,
            Vec3::new(0.0, len * 0.5 - 0.1, 0.0),
            Vec3::new(w * 0.55, 0.1, 0.1),
            Quat::IDENTITY,
            GOLD,
            Material::Metal,
            false,
            0.0,
        );
        if emblem {
            let d = top + n * 0.2 - Vec3::Y * w * 0.75;
            self.disc(d, n * 0.04, w * 0.28, GOLD, Material::Metal);
        }
    }

    // -----------------------------------------------------------------------
    // The middle

    /// Large paving slabs over the plaza top (visual; the terrain carries you).
    pub(super) fn paving(&mut self, p: &Platform, lms: &[Landmark], stone: Vec3) {
        let step = 8.0;
        let n = (p.half * 2.0 / step) as i32;
        for i in 0..n {
            for j in 0..n {
                let c = p.center - Vec2::splat(p.half)
                    + Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * step;
                // Only where the top is flat, and not under the rotunda.
                if p.sdf(c) > -step * 0.75
                    || lms.iter().any(|l| c.distance(l.center) < l.radius + 2.0)
                {
                    continue;
                }
                let shade = 0.94 + 0.06 * (((i * 7 + j * 13) % 5) as f32 / 4.0);
                self.slab(
                    Vec3::new(c.x - step * 0.48, p.height - 0.1, c.y - step * 0.48),
                    Vec3::new(c.x + step * 0.48, p.height + 0.06, c.y + step * 0.48),
                    0.0,
                    stone * shade,
                    Material::Concrete,
                    false,
                );
            }
        }
    }

    /// The rotunda: a drum of concrete piers with a door on each axis, a
    /// coffered dome with an oculus, and the northeast side fallen in (drum
    /// broken down to stumps, dome gone, rubble heaped inside and out).
    /// Inside, a ring of columns and the emblem on its dais.
    fn rotunda(&mut self, rng: &mut Rng, lm: &Landmark, base: f32) -> Vec<Drift> {
        let mut drifts = Vec::new();
        let (center, radius) = (lm.center, lm.radius);
        // The way it fell, and how far round from it a piece is.
        let fall_a = lm.fallen.map(angle);
        let from_fall = |a: f32| fall_a.map_or(PI, |f| apart(a, f));
        let door_a: Vec<f32> = lm.doors.iter().map(|&d| angle(d)).collect();
        self.sound(
            Ambient::Dome,
            Vec3::new(center.x, base + DRUM_H * 0.6, center.y),
        );
        let segs = 40;
        let collapsed = |a: f32| from_fall(a) < 0.75;
        let door = |a: f32| door_a.iter().any(|&d| apart(a, d) < 0.24);
        let r_mid = radius - DRUM_T * 0.5;
        // Wide enough that the outer faces meet: no slits between piers.
        let seg_half = radius * PI / segs as f32 * 1.02;
        for k in 0..segs {
            let a = (k as f32 + 0.5) / segs as f32 * TAU;
            let dir = Vec2::new(a.cos(), a.sin());
            let c = center + dir * r_mid;
            let yaw = -a + FRAC_PI_2;
            let (lo, hi) = (
                Vec3::new(c.x - seg_half, 0.0, c.y - DRUM_T * 0.5),
                Vec3::new(c.x + seg_half, 0.0, c.y + DRUM_T * 0.5),
            );
            let wall = |y0: f32, y1: f32| {
                (
                    Vec3::new(lo.x, base + y0, lo.z),
                    Vec3::new(hi.x, base + y1, hi.z),
                )
            };
            if collapsed(a) {
                // Stumps of broken wall, a few meters at most.
                let h = rng.range(0.8, 4.5);
                let (l, u) = wall(0.0, h);
                self.slab(
                    l,
                    u,
                    yaw,
                    CONCRETE * rng.range(0.9, 1.0),
                    Material::Concrete,
                    true,
                );
                self.oboxes.last_mut().unwrap().broken = true;
                continue;
            }
            if door(a) {
                // A tall doorway: the lintel and wall over it only.
                let (l, u) = wall(10.0, DRUM_H);
                self.slab(l, u, yaw, CONCRETE, Material::Concrete, true);
                self.oboxes.last_mut().unwrap().chip = 0.01;
                continue;
            }
            let (l, u) = wall(0.0, DRUM_H);
            self.slab(l, u, yaw, CONCRETE, Material::Concrete, true);
            // One continuous wall: no chipped edges opening slits between
            // the piers (a tiny chip keeps the map-wide wear off them).
            self.oboxes.last_mut().unwrap().chip = 0.01;
            // Buttress pilasters on every fourth pier.
            if k % 4 == 2 {
                let p = center + dir * (radius + 0.6);
                self.slab(
                    Vec3::new(p.x - 1.0, base, p.y - 0.9),
                    Vec3::new(p.x + 1.0, base + DRUM_H + 1.2, p.y + 0.9),
                    yaw,
                    CONCRETE * 0.94,
                    Material::Concrete,
                    true,
                );
            }
            // Cornice ring, overhanging.
            let p = center + dir * (radius + 0.2);
            self.slab(
                Vec3::new(p.x - seg_half * 1.05, base + DRUM_H, p.y - 1.4),
                Vec3::new(p.x + seg_half * 1.05, base + DRUM_H + 1.1, p.y + 1.4),
                yaw,
                STONE * 0.92,
                Material::Concrete,
                false,
            );
        }
        // Banners between the doors, outside.
        for k in 0..8 {
            let a = (k as f32 + 0.5) / 8.0 * TAU;
            if collapsed(a) {
                continue;
            }
            let dir = Vec2::new(a.cos(), a.sin());
            let p = center + dir * (radius + 0.05);
            self.banner(Vec3::new(p.x, base + DRUM_H - 0.6, p.y), dir, 3.0, 11.0);
        }

        // The dome: rings of plates from the drum's top to the oculus, skipped
        // where it fell in (with a ragged edge).
        let dome_base = base + DRUM_H + 1.0;
        let rings = 7;
        let r = radius - 0.6;
        for i in 0..rings {
            let p0 = i as f32 / rings as f32 * FRAC_PI_2 * 0.86;
            let p1 = (i + 1) as f32 / rings as f32 * FRAC_PI_2 * 0.86;
            let phi = (p0 + p1) * 0.5;
            let n = (segs as f32 * phi.cos()).round().max(10.0) as usize;
            for k in 0..n {
                let a = (k as f32 + 0.5) / n as f32 * TAU;
                let d = from_fall(a);
                // Gone over the fallen drum and up past the crown, ragged.
                // Widest over the fallen drum, narrowing toward the crown so
                // the oculus ring still has dome round it.
                if d.abs() < 1.1 + 0.25 * rng.f32() - 0.1 * i as f32 {
                    continue;
                }
                let radial = Vec3::new(a.cos() * phi.cos(), phi.sin(), a.sin() * phi.cos());
                let tangent = Vec3::new(-a.sin(), 0.0, a.cos());
                let c = Vec3::new(center.x, dome_base, center.y) + radial * r;
                let rig = Rig {
                    pos: c,
                    rot: frame(tangent, radial),
                };
                let w = r * phi.cos() * PI / n as f32 * 1.06;
                let h = r * (p1 - p0) * 0.53;
                let shade = if (i + k) % 2 == 0 { 1.0 } else { 0.95 };
                rig.part(
                    self,
                    Vec3::ZERO,
                    Vec3::new(w, 0.45, h),
                    Quat::IDENTITY,
                    CONCRETE * shade,
                    Material::Concrete,
                    false,
                    0.0,
                );
            }
        }
        // Ribs inside the shell (hidden from outside, a lattice of coffers
        // through the hole): meridians, and rings between the plate rings.
        let inner = r - 0.9;
        let at = |a: f32, phi: f32| {
            Vec3::new(center.x, dome_base, center.y)
                + Vec3::new(a.cos() * phi.cos(), phi.sin(), a.sin() * phi.cos()) * inner
        };
        for k in 0..20 {
            let a = (k as f32) / 20.0 * TAU;
            if from_fall(a) < 1.05 {
                continue;
            }
            for i in 0..rings {
                let p0 = i as f32 / rings as f32 * FRAC_PI_2 * 0.86;
                let p1 = (i + 1) as f32 / rings as f32 * FRAC_PI_2 * 0.86;
                let (u, v) = (at(a, p0), at(a, p1));
                let rig = Rig {
                    pos: (u + v) * 0.5,
                    rot: frame(
                        Vec3::new(-a.sin(), 0.0, a.cos()),
                        ((u + v) * 0.5 - Vec3::new(center.x, dome_base, center.y)).normalize(),
                    ),
                };
                rig.part(
                    self,
                    Vec3::ZERO,
                    Vec3::new(0.35, 0.4, u.distance(v) * 0.5 + 0.2),
                    Quat::IDENTITY,
                    STONE * 0.8,
                    Material::Concrete,
                    false,
                    0.0,
                );
            }
        }
        // The oculus ring.
        let phi = FRAC_PI_2 * 0.86;
        for k in 0..16 {
            let a = (k as f32 + 0.5) / 16.0 * TAU;
            let radial = Vec3::new(a.cos() * phi.cos(), phi.sin(), a.sin() * phi.cos());
            let tangent = Vec3::new(-a.sin(), 0.0, a.cos());
            let rig = Rig {
                pos: Vec3::new(center.x, dome_base, center.y) + radial * (r + 0.3),
                rot: frame(tangent, Vec3::Y),
            };
            rig.part(
                self,
                Vec3::ZERO,
                Vec3::new(r * phi.cos() * PI / 16.0 * 1.08, 0.5, 0.7),
                Quat::IDENTITY,
                STONE * 0.9,
                Material::Concrete,
                false,
                0.0,
            );
        }

        // Inside: a ring of columns (two fallen), and the emblem on its dais.
        for k in 0..8 {
            let a = (k as f32 + 0.5) / 8.0 * TAU;
            let p = center + Vec2::new(a.cos(), a.sin()) * 14.0;
            if collapsed(a) {
                // Fallen inward, lying across the floor.
                let along = Vec2::new(-a.sin(), a.cos()) * 0.4 - Vec2::new(a.cos(), a.sin());
                let along = along.normalize();
                let e = p + along * 12.0;
                self.drum(
                    Vec3::new(p.x, base + 1.0, p.y),
                    Vec3::new(e.x, base + 1.0, e.y),
                    1.0,
                    STONE,
                );
                continue;
            }
            self.column(Vec3::new(p.x, base, p.y), 1.0, DRUM_H - 1.0, false);
        }
        self.drum(
            Vec3::new(center.x, base, center.y),
            Vec3::new(center.x, base + 1.2, center.y),
            4.5,
            STONE,
        );
        // A square stele on the dais, the emblem on each face, one to
        // greet each door.
        let stele = 1.3;
        self.slab(
            Vec3::new(center.x - stele, base + 1.2, center.y - stele),
            Vec3::new(center.x + stele, base + 8.0, center.y + stele),
            0.0,
            STONE * 0.95,
            Material::Concrete,
            true,
        );
        self.slab(
            Vec3::new(center.x - stele - 0.3, base + 8.0, center.y - stele - 0.3),
            Vec3::new(center.x + stele + 0.3, base + 8.6, center.y + stele + 0.3),
            0.0,
            STONE * 0.9,
            Material::Concrete,
            false,
        );
        for out in AXES {
            let at = center + out * (stele + 0.16);
            self.standing_emblem(Vec3::new(at.x, base + 5.4, at.y), out, 0.62);
        }

        // Rubble where it fell: heaps banked against the broken wall, chunks
        // of dome scattered inside and out.
        let Some(fall) = lm.fallen else {
            return drifts;
        };
        for (side, sign) in [(-1.0f32, 1.0f32), (1.0, -1.0)] {
            let p = center + fall * (radius + side * 4.0);
            drifts.push(Drift {
                b: OBox {
                    center: p,
                    yaw: -angle(fall),
                    half: Vec2::new(10.0, 2.0),
                    y0: 0.0,
                    y1: 1.0,
                    ..OBox::default()
                },
                axis: 1,
                sign,
                height: 1.6,
                run: 9.0,
            });
        }
        for _ in 0..34 {
            let a = angle(fall) + rng.range(-0.7, 0.7);
            let d = rng.range(6.0, radius + 14.0);
            let p = center + Vec2::new(a.cos(), a.sin()) * d;
            let s = rng.range(0.8, 2.6);
            let yaw = rng.range(0.0, TAU);
            let half = Vec2::new(s, s * rng.range(0.4, 0.9));
            let top = base + s * rng.range(0.3, 0.7);
            let (tilt, roll) = (rng.range(-0.4, 0.4), rng.range(-0.3, 0.3));
            let color = CONCRETE * rng.range(0.85, 1.0);
            self.oboxes.push(OBox {
                tilt,
                roll,
                // None that blocks lies in the middle, round the dais.
                solid: s > 1.6 && d > 17.0,
                ..rocks::rubble(p, yaw, half, base - 0.3, top, color)
            });
        }
        drifts
    }

    /// A dry fountain: a low round basin wall with a broken obelisk
    /// standing in it. Cover on the open paving.
    fn fountain(&mut self, rng: &mut Rng, c: Vec2, g: f32) {
        let (r, segs) = (6.0, 16);
        for i in 0..segs {
            let t = (i as f32 + 0.5) / segs as f32 * TAU;
            let p = c + Vec2::new(t.cos(), t.sin()) * r;
            let half = r * PI / segs as f32 * 1.05;
            self.slab(
                Vec3::new(p.x - half, g - 0.1, p.y - 0.45),
                Vec3::new(p.x + half, g + 1.2, p.y + 0.45),
                -t + FRAC_PI_2,
                STONE * 0.9,
                Material::Concrete,
                true,
            );
            self.oboxes.last_mut().unwrap().chip = 0.01;
        }
        let h = rng.range(3.0, 6.0);
        self.slab(
            Vec3::new(c.x - 1.2, g, c.y - 1.2),
            Vec3::new(c.x + 1.2, g + 1.0, c.y + 1.2),
            0.0,
            STONE * 0.85,
            Material::Concrete,
            true,
        );
        self.slab(
            Vec3::new(c.x - 0.7, g + 1.0, c.y - 0.7),
            Vec3::new(c.x + 0.7, g + 1.0 + h, c.y + 0.7),
            rng.range(0.0, TAU),
            STONE * 0.95,
            Material::Concrete,
            true,
        );
        self.oboxes.last_mut().unwrap().broken = true;
    }

    /// Low cover at `p`, lying along `face`: a stub of broken wall, a row
    /// of cast barrier blocks, or a column drum on its side.
    fn cover(&mut self, rng: &mut Rng, p: Vec2, face: Vec2, g: f32) {
        let yaw = angle(face);
        match rng.f32() {
            x if x < 0.5 => {
                // A stub of wall, broken along the top.
                let h = rng.range(1.6, 3.4);
                self.oboxes.push(OBox {
                    center: p,
                    yaw,
                    half: Vec2::new(rng.range(3.5, 7.5), 0.5),
                    y0: g - 0.2,
                    y1: g + h,
                    color: CONCRETES[rng.index(CONCRETES.len())],
                    side_mat: Material::Concrete,
                    top_mat: Material::Concrete,
                    broken: true,
                    ..OBox::default()
                });
            }
            x if x < 0.8 => {
                // A row of cast barrier blocks, one knocked askew.
                let ax = Vec2::new(yaw.cos(), -yaw.sin());
                for k in -1..=1 {
                    let c = p + ax * k as f32 * 2.3;
                    let skew = if k == 1 { rng.range(0.2, 0.6) } else { 0.0 };
                    self.oboxes.push(OBox {
                        center: c,
                        yaw: yaw + skew,
                        half: Vec2::new(1.0, 0.7),
                        y0: g,
                        y1: g + 1.3,
                        color: CONCRETE * 0.95,
                        side_mat: Material::Concrete,
                        top_mat: Material::Concrete,
                        ..OBox::default()
                    });
                }
            }
            _ => {
                // A column drum on its side, carted off and left.
                let a = Vec2::new(yaw.cos(), yaw.sin());
                let (u, v) = (p - a * 2.2, p + a * 2.2);
                self.drum(
                    Vec3::new(u.x, g + 1.0, u.y),
                    Vec3::new(v.x, g + 1.0, v.y),
                    1.05,
                    STONE * 0.95,
                );
            }
        }
    }

    /// A solid round drum of stone from `a` to `b`: the pipe's ring of
    /// plates, capped at both ends (a broken or fallen column shows its
    /// section, not a hollow tube).
    ///
    /// The caps sit a few centimetres in from the pipe's ends, their rims
    /// halfway through its plates: flush with either (the end plane or the
    /// plates' outer faces) they'd z-fight wherever the map's millimetre
    /// separation happened to land two faces together.
    pub(super) fn drum(&mut self, a: Vec3, b: Vec3, r: f32, color: Vec3) {
        self.pipe(a, b, r, color, Material::Concrete, true);
        let axis = (b - a).normalize_or(Vec3::Y);
        let d = axis * 0.03;
        // The pipe's plates: 12 of them, `thick` through (see `pipe`).
        let thick = (r * 0.12).clamp(0.01, 0.06);
        let cos15 = (std::f32::consts::PI / 12.0).cos();
        let cap_r = r - thick / cos15;
        let inset = axis * 0.05;
        for end in [a + d + inset, b - d - inset] {
            self.disc(end, d, cap_r, color * 0.9, Material::Concrete);
        }
    }

    /// A big mass broken off at the top, drawn as what's left of the next
    /// floor: stubs of wall and column standing on its flat top. (The
    /// renderer's jagged `broken` top is for thin pieces; on a wide top its
    /// cuts run all the way across and shade into long stripes.)
    pub(in crate::map) fn ragged_top(&mut self, rng: &mut Rng, o: &OBox) {
        let (ax, az) = o.axes();
        let stubs = 3 + (o.half.x * o.half.y / 40.0).min(4.0) as usize;
        for _ in 0..stubs {
            let half = Vec2::new(
                rng.range(0.3, (o.half.x * 0.3).clamp(0.4, 2.5)),
                rng.range(0.3, (o.half.y * 0.3).clamp(0.4, 2.5)),
            );
            let q = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * (o.half - half);
            self.oboxes.push(OBox {
                center: o.center + ax * q.x + az * q.y,
                yaw: o.yaw,
                half,
                y0: o.y1 - 0.05,
                y1: o.y1 + rng.range(0.6, 3.2),
                color: o.color * rng.range(0.9, 1.0),
                side_mat: o.side_mat,
                top_mat: o.top_mat,
                solid: false,
                // Small enough that the jagged top reads as broken, not a fan.
                broken: half.max_element() < 1.0,
                chip: 0.01,
                ..OBox::default()
            });
        }
    }

    /// A round column with a square base and capital, standing at `at`.
    fn column(&mut self, at: Vec3, r: f32, h: f32, broken: bool) {
        self.slab(
            at + Vec3::new(-r * 1.3, 0.0, -r * 1.3),
            at + Vec3::new(r * 1.3, 0.8, r * 1.3),
            0.0,
            STONE * 0.92,
            Material::Concrete,
            true,
        );
        self.drum(at + Vec3::Y * 0.8, at + Vec3::Y * (h - 0.6), r, STONE);
        if broken {
            return;
        }
        self.slab(
            at + Vec3::new(-r * 1.4, h - 0.6, -r * 1.4),
            at + Vec3::new(r * 1.4, h, r * 1.4),
            0.0,
            STONE * 0.92,
            Material::Concrete,
            false,
        );
    }

    /// The ring of columns round the rotunda, carrying a beam, opening wide
    /// where the avenues come in. A quarter of the columns are down: broken
    /// off, or lying where they fell.
    fn colonnade(&mut self, rng: &mut Rng, lm: &Landmark, base: f32) {
        let n = 36;
        let h = 11.0;
        let mut standing = vec![false; n];
        for (k, up) in standing.iter_mut().enumerate() {
            let a = (k as f32 + 0.5) / n as f32 * TAU;
            // Open on the avenues.
            let gate = lm.doors.iter().any(|&d| apart(a, angle(d)) < 0.2);
            if gate {
                continue;
            }
            let dir = Vec2::new(a.cos(), a.sin());
            let p = lm.center + dir * lm.precinct;
            let at = Vec3::new(p.x, base, p.y);
            match rng.f32() {
                x if x < 0.12 => {
                    // Fell outward and sideways, broken in two.
                    let along = (dir + Vec2::new(-dir.y, dir.x) * rng.range(-0.8, 0.8)).normalize();
                    self.slab(
                        at + Vec3::new(-1.3, 0.0, -1.3),
                        at + Vec3::new(1.3, 0.8, 1.3),
                        0.0,
                        STONE * 0.92,
                        Material::Concrete,
                        true,
                    );
                    let a0 = p + along * 2.0;
                    let a1 = p + along * 6.5;
                    let b0 = p + along * 7.3;
                    let b1 = p + along * 11.0;
                    for (u, v) in [(a0, a1), (b0, b1)] {
                        self.drum(
                            Vec3::new(u.x, base + 0.9, u.y),
                            Vec3::new(v.x, base + 0.9, v.y),
                            0.9,
                            STONE,
                        );
                    }
                }
                x if x < 0.25 => self.column(at, 0.9, rng.range(3.0, 7.0), true),
                _ => {
                    self.column(at, 0.9, h, false);
                    *up = true;
                }
            }
        }
        // The beam, only between two standing columns.
        let r_beam = lm.precinct;
        for k in 0..n {
            let k1 = (k + 1) % n;
            if !(standing[k] && standing[k1]) {
                continue;
            }
            let a = (k as f32 + 1.0) / n as f32 * TAU;
            let dir = Vec2::new(a.cos(), a.sin());
            let p = lm.center + dir * r_beam;
            let half = r_beam * PI / n as f32 + 1.2;
            self.slab(
                Vec3::new(p.x - half, base + h, p.y - 1.1),
                Vec3::new(p.x + half, base + h + 1.4, p.y + 1.1),
                -a + FRAC_PI_2,
                STONE * 0.95,
                Material::Concrete,
                false,
            );
        }
    }

    // -----------------------------------------------------------------------
    // The avenues

    /// A statue on a plinth facing `face`: a robed figure in blocks. Some
    /// stand, some have lost their heads, some lie toppled beside the plinth.
    fn statue(&mut self, rng: &mut Rng, at: Vec2, face: Vec2, g: f32) {
        let yaw = (-face.y).atan2(face.x) + FRAC_PI_2;
        let plinth = 3.6;
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        rig.part(
            self,
            Vec3::new(0.0, plinth * 0.5, 0.0),
            Vec3::new(1.6, plinth * 0.5, 1.6),
            Quat::IDENTITY,
            CONCRETE * 0.95,
            Material::Concrete,
            true,
            0.3,
        );
        rig.part(
            self,
            Vec3::new(0.0, plinth + 0.15, 0.0),
            Vec3::new(1.8, 0.15, 1.8),
            Quat::IDENTITY,
            STONE * 0.9,
            Material::Concrete,
            false,
            0.0,
        );
        let state = rng.f32();
        let figure = |rig: &Rig, map: &mut Map, base: Vec3, rot: Quat, head: bool| {
            let part = |map: &mut Map, c: Vec3, h: Vec3| {
                rig.part(
                    map,
                    base + rot * c,
                    h,
                    rot,
                    STONE,
                    Material::Concrete,
                    false,
                    0.0,
                );
            };
            // Robe (wider at the hem), torso, shoulders, arm raised.
            part(map, Vec3::new(0.0, 1.3, 0.0), Vec3::new(0.75, 1.3, 0.55));
            part(map, Vec3::new(0.0, 3.1, 0.0), Vec3::new(0.6, 0.6, 0.45));
            part(map, Vec3::new(0.0, 3.75, 0.0), Vec3::new(0.85, 0.18, 0.4));
            part(map, Vec3::new(0.95, 4.3, 0.15), Vec3::new(0.18, 0.75, 0.18));
            if head {
                part(map, Vec3::new(0.0, 4.35, 0.0), Vec3::new(0.3, 0.38, 0.3));
            }
        };
        let top = Vec3::new(0.0, plinth + 0.3, 0.0);
        if state < 0.8 {
            // The raised hand holds a standard, the emblem's disc on top.
            let hand = rig.point(top + Vec3::new(0.95, 4.6, 0.15));
            let tip = hand + Vec3::Y * 3.2;
            self.pipe(
                hand - Vec3::Y * 4.4,
                tip,
                0.07,
                GOLD * 0.8,
                Material::Metal,
                false,
            );
            let n = Vec3::new(face.x, 0.0, face.y);
            self.disc(tip + Vec3::Y * 0.5, n * 0.05, 0.5, GOLD, Material::Metal);
        }
        if state < 0.6 {
            figure(&rig, self, top, Quat::IDENTITY, true);
        } else if state < 0.8 {
            figure(&rig, self, top, Quat::IDENTITY, false);
        } else {
            // Toppled off the plinth, lying face down beside it.
            let rot = Quat::from_rotation_x(FRAC_PI_2 * 0.98);
            figure(&rig, self, Vec3::new(0.0, 0.7, 2.4), rot, true);
        }
    }

    /// A tall mast with a long crimson banner on a crossbar.
    fn mast(&mut self, at: Vec2, along: Vec2, g: f32) {
        let h = 14.0;
        self.pipe(
            Vec3::new(at.x, g, at.y),
            Vec3::new(at.x, g + h, at.y),
            0.18,
            CONCRETE_DARK,
            Material::Painted,
            true,
        );
        // Crossbar across the avenue, so the banner faces down it: you see
        // it driving either way.
        let a = Vec3::new(-along.y, 0.0, along.x);
        let top = Vec3::new(at.x, g + h - 0.4, at.y);
        self.pipe(
            top - a * 1.4,
            top + a * 1.4,
            0.08,
            GOLD,
            Material::Metal,
            false,
        );
        let face = along;
        self.banner(
            top - Vec3::Y * 0.1 - Vec3::new(face.x, 0.0, face.y) * 0.12,
            face,
            2.4,
            8.5,
        );
    }

    /// A triumphal arch across the avenue along `dir`: two massive piers, an
    /// arched opening, an attic on top with the emblem both ways and
    /// banners down the piers. `ruined`: one pier sheared off and the attic
    /// fallen across the road.
    fn arch(&mut self, rng: &mut Rng, s: &Span, g: f32) {
        let (c, dir, ruined) = (s.at, s.along, s.ruined);
        let yaw = (-dir.y).atan2(dir.x);
        let rig = Rig {
            pos: Vec3::new(c.x, g, c.y),
            // Local x across the avenue, z along it.
            rot: Quat::from_rotation_y(yaw - FRAC_PI_2),
        };
        let (opening, pier_w, depth, pier_h) = (s.opening, 6.0, 5.0, 13.0);
        let pier_x = opening + pier_w * 0.5;
        for s in [-1.0f32, 1.0] {
            let broken = ruined && s > 0.0;
            let h = if broken { 7.5 } else { pier_h };
            rig.part(
                self,
                Vec3::new(s * pier_x, h * 0.5, 0.0),
                Vec3::new(pier_w * 0.5, h * 0.5, depth * 0.5),
                Quat::IDENTITY,
                CONCRETE,
                Material::Concrete,
                true,
                0.01,
            );
            if broken {
                let o = *self.oboxes.last().unwrap();
                self.ragged_top(rng, &o);
                continue;
            }
            // Half columns on the pier faces, both ways.
            for f in [-1.0f32, 1.0] {
                rig.part(
                    self,
                    Vec3::new(
                        s * (pier_x + pier_w * 0.3),
                        pier_h * 0.5,
                        f * (depth * 0.5 + 0.35),
                    ),
                    Vec3::new(0.6, pier_h * 0.5, 0.35),
                    Quat::IDENTITY,
                    STONE * 0.94,
                    Material::Concrete,
                    false,
                    0.0,
                );
                let face = dir * f;
                let p = rig.point(Vec3::new(s * (pier_x - 1.2), 0.0, f * depth * 0.5));
                self.banner(Vec3::new(p.x, pier_h - 1.0, p.z), face, 2.2, 7.5);
            }
        }
        // The arch over the opening: a half circle springing from the piers
        // at `spring`, its crown meeting the attic. The spandrels (stone
        // between the curve and the attic) fill in strip by strip, and a
        // ring of voussoirs stands proud round the curve on both faces.
        let spring = pier_h - opening;
        let strips = 13;
        for k in 0..strips {
            let (x0, x1) = (
                -opening + 2.0 * opening * k as f32 / strips as f32,
                -opening + 2.0 * opening * (k + 1) as f32 / strips as f32,
            );
            if ruined && x1 > 0.0 {
                break;
            }
            // The curve's height under this strip, taken where it's highest
            // (the strip's inner edge), so no strip hangs below the curve:
            // the voussoirs cover the steps.
            let edge = if x0 < 0.0 && x1 > 0.0 {
                0.0
            } else {
                x0.abs().min(x1.abs())
            }
            .min(opening);
            let y = spring + (opening * opening - edge * edge).sqrt();
            if pier_h - y < 0.05 {
                continue;
            }
            rig.part(
                self,
                Vec3::new((x0 + x1) * 0.5, (y + pier_h) * 0.5, 0.0),
                Vec3::new((x1 - x0) * 0.5 + 0.01, (pier_h - y) * 0.5, depth * 0.5),
                Quat::IDENTITY,
                CONCRETE,
                Material::Concrete,
                true,
                0.01,
            );
            // The ruined arch ends in a broken edge at the crown.
            if ruined && x1 + 2.0 * opening / strips as f32 > 0.0 {
                self.oboxes.last_mut().unwrap().broken = true;
            }
        }
        let arc_n = 15;
        for k in 0..arc_n {
            let a = PI * (k as f32 + 0.5) / arc_n as f32;
            if ruined && a < FRAC_PI_2 {
                continue;
            }
            let p = Vec3::new(
                a.cos() * (opening + 0.45),
                spring + a.sin() * (opening + 0.45),
                0.0,
            );
            rig.part(
                self,
                p,
                Vec3::new(
                    0.6,
                    (opening + 0.45) * PI / arc_n as f32 * 0.56,
                    depth * 0.5 + 0.12,
                ),
                Quat::from_rotation_z(a),
                STONE * 0.9,
                Material::Concrete,
                false,
                0.0,
            );
        }
        let attic_w = opening + pier_w;
        if ruined {
            // The attic came down in two pieces either side of the road.
            // Beside the piers, off the opening: the road through stays clear.
            for (x, tilt, roll) in [(-12.0f32, 0.25f32, 0.1f32), (12.0, -0.2, -0.35)] {
                rig.part(
                    self,
                    Vec3::new(x, 1.8, rng.range(4.0, 6.0)),
                    Vec3::new(4.8, 1.8, depth * 0.5),
                    Quat::from_rotation_z(roll) * Quat::from_rotation_x(tilt),
                    CONCRETE * 0.95,
                    Material::Concrete,
                    true,
                    0.4,
                );
            }
            let e = rig.point(Vec3::new(-12.0, 2.3, 6.0 + depth * 0.5 + 0.2));
            self.standing_emblem(e, dir, 1.4);
            return;
        }
        rig.part(
            self,
            Vec3::new(0.0, pier_h + 3.5, 0.0),
            Vec3::new(attic_w, 3.5, depth * 0.5 + 0.3),
            Quat::IDENTITY,
            CONCRETE,
            Material::Concrete,
            true,
            0.01,
        );
        rig.part(
            self,
            Vec3::new(0.0, pier_h + 7.3, 0.0),
            Vec3::new(attic_w + 0.6, 0.3, depth * 0.5 + 0.7),
            Quat::IDENTITY,
            STONE * 0.92,
            Material::Concrete,
            false,
            0.0,
        );
        for f in [-1.0f32, 1.0] {
            let p = rig.point(Vec3::new(0.0, pier_h + 3.6, f * (depth * 0.5 + 0.35)));
            self.standing_emblem(p, dir * f, 1.6);
        }
    }

    // -----------------------------------------------------------------------
    // The districts

    /// A brutalist slab: a heavy mass of concrete lifted on square pillars
    /// round a core, cantilevered out over the street front, banded with
    /// dark ribbon windows. Sometimes shelled: one end blown away.
    fn brutalist(&mut self, rng: &mut Rng, lot: &OBox, front: (usize, f32)) {
        let h = lot.half;
        // Local frame: x along the front, z out toward it.
        let (fx, fz) = if front.0 == 0 {
            (Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0) * front.1)
        } else {
            (Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0) * front.1)
        };
        let (hw, hd) = if front.0 == 0 { (h.y, h.x) } else { (h.x, h.y) };
        let (ax, az) = lot.axes();
        let wx = ax * fx.x + az * fx.y;
        let wz = ax * fz.x + az * fz.y;
        let rig = Rig {
            pos: Vec3::new(lot.center.x, 0.0, lot.center.y),
            rot: frame(Vec3::new(wx.x, 0.0, wx.y), Vec3::Y),
        };
        // `frame` makes local z = x × y; flip depth so local +z is the front.
        let zs = if Vec3::new(wx.x, 0.0, wx.y)
            .cross(Vec3::Y)
            .dot(Vec3::new(wz.x, 0.0, wz.y))
            > 0.0
        {
            1.0
        } else {
            -1.0
        };
        let lift = 6.5;
        let floors = rng.int(3, 6) as f32;
        let top = lift + floors * 3.6;
        let fin = crate::map::imperial::Finish::pick(rng);
        let color = fin.body;
        let put = |map: &mut Map, c: Vec3, half: Vec3, color: Vec3, mat: Material, solid: bool| {
            rig.part(
                map,
                Vec3::new(c.x, c.y, c.z * zs),
                half,
                Quat::IDENTITY,
                color,
                mat,
                solid,
                0.0,
            );
        };
        // Core and pillars.
        put(
            self,
            Vec3::new(0.0, lift * 0.5, -hd * 0.35),
            Vec3::new(hw * 0.32, lift * 0.5, hd * 0.4),
            fin.trim,
            Material::Concrete,
            true,
        );
        for px in [-1.0f32, 0.0, 1.0] {
            for pz in [-1.0f32, 1.0] {
                put(
                    self,
                    Vec3::new(px * (hw - 1.2), lift * 0.5, pz * (hd - 1.2)),
                    Vec3::new(0.7, lift * 0.5, 0.7),
                    color,
                    Material::Concrete,
                    true,
                );
            }
        }
        // The mass: cantilevered 3 m out over the front. Shelled: the
        // left half stops short, broken.
        let shelled = rng.f32() < 0.45;
        let over = 3.0;
        let halves: [(f32, f32, f32); 2] = if shelled {
            [(-hw, 0.0, lift + rng.range(1.0, 2.5) * 3.6), (0.0, hw, top)]
        } else {
            [(-hw, 0.0, top), (0.0, hw, top)]
        };
        for (x0, x1, y1) in halves {
            let c = Vec3::new((x0 + x1) * 0.5, (lift + y1) * 0.5, over * 0.5);
            put(
                self,
                c,
                Vec3::new((x1 - x0) * 0.5, (y1 - lift) * 0.5, hd + over * 0.5),
                color,
                Material::Concrete,
                true,
            );
            if shelled && y1 < top {
                let o = *self.oboxes.last().unwrap();
                self.ragged_top(rng, &o);
            }
            // Ribbon windows round each floor (front and back faces).
            let mut y = lift + 1.6;
            while y + 1.0 < y1 {
                for f in [-1.0f32, 1.0] {
                    let z = if f > 0.0 {
                        hd + over + 0.06
                    } else {
                        -hd - 0.06
                    };
                    put(
                        self,
                        Vec3::new((x0 + x1) * 0.5, y, z),
                        Vec3::new((x1 - x0) * 0.5 - 0.6, 0.55, 0.08),
                        CONCRETE_DARK * 0.6,
                        Material::Concrete,
                        false,
                    );
                }
                y += 3.6;
            }
            // Slit windows down the end wall.
            let end = if x0 < 0.0 { -hw - 0.06 } else { hw + 0.06 };
            let mut z = -hd + 1.4;
            while z < hd + over - 1.0 {
                put(
                    self,
                    Vec3::new(end, (lift + 1.4 + y1 - 1.8) * 0.5, z),
                    Vec3::new(0.08, (y1 - 1.8 - lift - 1.4).max(0.2) * 0.5, 0.35),
                    CONCRETE_DARK * 0.6,
                    Material::Concrete,
                    false,
                );
                z += 2.6;
            }
            // The underside, in shadow, reads darker: a soffit.
            put(
                self,
                Vec3::new((x0 + x1) * 0.5, lift - 0.15, over * 0.5),
                Vec3::new((x1 - x0) * 0.5 - 0.1, 0.2, hd + over * 0.5 - 0.1),
                fin.trim * 0.8,
                Material::Concrete,
                false,
            );
        }
        // A fascia of dressed stone round the top of what stands.
        let (fx0, fx1) = if shelled { (0.0, hw) } else { (-hw, hw) };
        put(
            self,
            Vec3::new((fx0 + fx1) * 0.5, top - 0.5, over * 0.5),
            Vec3::new((fx1 - fx0) * 0.5 + 0.3, 0.6, hd + over * 0.5 + 0.3),
            fin.trim,
            Material::Concrete,
            false,
        );
        // Fins up the front, and the banner.
        if !shelled {
            let mut x = -hw + 1.5;
            while x < hw - 1.0 {
                put(
                    self,
                    Vec3::new(x, (lift + top) * 0.5, hd + over + 0.4),
                    Vec3::new(0.18, (top - lift) * 0.5, 0.5),
                    color * 0.95,
                    Material::Concrete,
                    false,
                );
                x += 3.0;
            }
        }
        let face = Vec2::new(wz.x, wz.y);
        let p = rig.point(Vec3::new(hw * 0.5, 0.0, (hd + over + 0.95) * zs));
        self.banner(
            Vec3::new(p.x, top - 0.8, p.z),
            face,
            3.0,
            (top - lift) * 0.8,
        );
    }

    /// An empty lot: a cracked slab and heaps of broken concrete.
    pub(super) fn rubble_lot(&mut self, rng: &mut Rng, lot: &OBox, stone: Vec3) {
        for _ in 0..rng.int(5, 10) {
            let q = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * lot.half * 0.8;
            let (ax, az) = lot.axes();
            let p = lot.center + ax * q.x + az * q.y;
            let s = rng.range(0.8, 2.4);
            let yaw = rng.range(0.0, TAU);
            let half = Vec2::new(s, s * rng.range(0.4, 1.0));
            let top = s * rng.range(0.4, 0.9);
            let (tilt, roll) = (rng.range(-0.3, 0.3), rng.range(-0.3, 0.3));
            let color = stone * rng.range(0.8, 1.0);
            self.oboxes.push(OBox {
                tilt,
                roll,
                solid: s > 1.5,
                ..rocks::rubble(p, yaw, half, -0.3, top, color)
            });
        }
    }

    /// A building far out past the edge, seen only from afar: a plain
    /// tower on its lot, turned to the lot's street grid, taller the further
    /// out. Visual only.
    fn distant_block(&mut self, rng: &mut Rng, lot: &OBox, sk: &Skeleton) {
        let p = lot.center;
        let out = (-p).max(p - Vec2::splat(sk.size)).max_element();
        let broken = rng.f32() < 0.3;
        // Built square to the map round its middle, then turned onto the lot.
        let toward = lot.local_point(sk.center()).signum();
        let start = self.oboxes.len();
        if out < super::city::NEAR + 40.0 {
            // The first rows: dressed on the faces toward the city.
            let mut faces = [false; 4];
            for (k, n) in [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y].iter().enumerate() {
                faces[k] = n.dot(toward) > 0.0;
            }
            let h = rng.range(12.0, 26.0);
            self.imperial_block(
                rng,
                &crate::map::imperial::Block {
                    center: p,
                    half: lot.half,
                    y0: -0.5,
                    h,
                    solid: false,
                    broken,
                    faces,
                    near: false,
                    banners: false,
                },
            );
        } else {
            let h = rng.range(10.0, 26.0) + out * rng.range(0.03, 0.12);
            let shade = rng.range(0.82, 0.95);
            self.skyline_tower(rng, p, lot.half, h, broken, shade, toward);
        }
        let (ax, az) = lot.axes();
        for o in &mut self.oboxes[start..] {
            let d = o.center - p;
            o.center = p + ax * d.x + az * d.y;
            o.yaw += lot.yaw;
        }
    }

    /// Barriers across a side street where it crosses the edge: concrete
    /// blocks and barriers set down in a ragged line, a gap here and there.
    pub(super) fn street_barricade(&mut self, rng: &mut Rng, e: &Edge, stone: Vec3) {
        let mut t = e.from + rng.range(0.0, 1.5);
        while t < e.to {
            let at = e.start + e.along * t - e.inward * rng.range(-1.5, 1.5);
            let across = angle(e.along) + rng.range(-0.35, 0.35);
            let color = stone * rng.range(0.8, 1.0);
            let half = if rng.chance(0.6) {
                // A road barrier.
                Vec2::new(rng.range(1.6, 2.2), 0.35)
            } else {
                // A block.
                Vec2::splat(rng.range(0.7, 1.0))
            };
            let h = rng.range(1.0, 1.4);
            self.oboxes.push(OBox {
                center: at,
                yaw: -across,
                half,
                y0: -0.2,
                y1: h,
                color,
                side_mat: Material::Concrete,
                top_mat: Material::Concrete,
                solid: true,
                ..OBox::default()
            });
            t += half.x * 2.0 + rng.range(0.6, 4.5);
        }
    }

    /// Rubble banked across an avenue's end (`span` along the edge): big
    /// lumps of concrete heaped along the edge, smaller ones spilled out in
    /// front.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn rubble_bank(
        &mut self,
        rng: &mut Rng,
        start: Vec2,
        along: Vec2,
        inward: Vec2,
        span: (f32, f32),
        stone: Vec3,
    ) {
        let lump = |rng: &mut Rng, at: Vec2, s: f32, h: f32, solid: bool| {
            let yaw = rng.range(0.0, TAU);
            let half = Vec2::new(s, s * rng.range(0.7, 1.0));
            let (tilt, roll) = (rng.range(-0.2, 0.2), rng.range(-0.2, 0.2));
            let color = stone * rng.range(0.75, 1.0);
            OBox {
                tilt,
                roll,
                solid,
                ..rocks::rubble(at, yaw, half, -0.8, h, color)
            }
        };
        // Three rows, heaped highest mid-avenue and toward the back: (how
        // far out past the edge, spacing, lowest height, how much the heap
        // adds to it).
        for (out, step, low, heap) in [
            (3.0, 2.6, 5.0, 4.0),
            (0.6, 2.4, 3.6, 2.2),
            (-2.0, 3.0, 2.8, 1.0),
        ] {
            let mut t = span.0 - 2.0 + rng.range(0.0, step);
            while t < span.1 + 2.0 {
                let u = ((t - span.0) / (span.1 - span.0)).clamp(0.0, 1.0);
                let h = low + heap * (std::f32::consts::PI * u).sin() + rng.range(-0.6, 0.6);
                let s = rng.range(2.2, 3.8);
                let at = start + along * (t + rng.range(-0.5, 0.5))
                    - inward * (out + rng.range(-0.6, 0.6));
                let o = lump(rng, at, s, h.max(2.9), true);
                self.oboxes.push(o);
                t += step * rng.range(0.85, 1.1);
            }
        }
        // Spill in front.
        for _ in 0..18 {
            let at = start + along * rng.range(span.0, span.1) + inward * rng.range(2.5, 9.0);
            let s = rng.range(0.4, 1.4);
            let h = s * rng.range(0.5, 1.0);
            let o = lump(rng, at, s, h, false);
            self.oboxes.push(o);
        }
        // Floor slabs of the block that came down, broken and leaning on
        // the heap.
        for _ in 0..3 {
            let t = rng.range(span.0 + 4.0, span.1 - 4.0);
            let half = Vec2::new(rng.range(2.5, 4.5), 0.25);
            let lean = rng.range(0.5, 0.9);
            let yaw = along.y.atan2(along.x) + rng.range(-0.3, 0.3);
            let y = rng.range(1.5, 3.0);
            self.oboxes.push(OBox {
                center: start + along * t + inward * rng.range(0.5, 1.5),
                yaw: -yaw,
                half,
                y0: y,
                y1: y + 0.5,
                roll: lean,
                color: stone * rng.range(0.85, 1.0),
                side_mat: Material::Concrete,
                top_mat: Material::Concrete,
                solid: false,
                broken: true,
                ..OBox::default()
            });
        }
    }

    // -----------------------------------------------------------------------
    // Bases, spawns, the edge

    /// Each corner: a staging court behind low barriers, open toward the
    /// middle, with two masts flying its side's color.
    pub(super) fn staging_court(
        &mut self,
        rng: &mut Rng,
        k: usize,
        b: &Base,
        middle: Vec2,
        stone: Vec3,
    ) {
        let (corner, inward, court) = (b.corner, b.inward, b.size);
        let at = |u: f32, v: f32| corner + Vec2::new(inward.x * u, inward.y * v);
        let box_uv = |map: &mut Map,
                      u0: f32,
                      v0: f32,
                      u1: f32,
                      v1: f32,
                      y1: f32,
                      color: Vec3,
                      mat: Material,
                      solid: bool| {
            let (a, b) = (at(u0, v0), at(u1, v1));
            let (lo, hi) = (a.min(b), a.max(b));
            map.slab(
                Vec3::new(lo.x, if solid { 0.0 } else { -0.1 }, lo.y),
                Vec3::new(hi.x, y1, hi.y),
                0.0,
                color,
                mat,
                solid,
            );
        };
        let side = self.factions[k];
        let color = side.paint();
        let ours = side == Faction::Imperium;
        // A cast pad under the court.
        let tile = 6.0;
        let n = (court / tile) as i32;
        for i in 0..n {
            for j in 0..n {
                let (u, v) = (i as f32 * tile, j as f32 * tile);
                let shade = 0.62 + 0.05 * (((i * 3 + j * 5) % 3) as f32);
                box_uv(
                    self,
                    u + 0.08,
                    v + 0.08,
                    u + tile - 0.08,
                    v + tile - 0.08,
                    // Flush with the ground, barely proud: a parked
                    // tank stands on it, not on a step.
                    0.02,
                    stone * shade,
                    Material::Concrete,
                    false,
                );
            }
        }
        // Bastion walls on the two map-edge sides, banners down their
        // inner faces in the side's color.
        box_uv(
            self,
            0.0,
            0.0,
            court + 1.4,
            1.8,
            6.5,
            stone * 0.8,
            Material::Concrete,
            true,
        );
        box_uv(
            self,
            0.0,
            0.0,
            1.8,
            court + 1.4,
            6.5,
            stone * 0.8,
            Material::Concrete,
            true,
        );
        for t in [0.3f32, 0.55, 0.8] {
            let p = at(court * t, 1.8);
            self.side_banner(
                Vec3::new(p.x, 6.2, p.y),
                Vec2::new(0.0, inward.y),
                2.6,
                5.0,
                color,
                ours,
            );
            let p = at(1.8, court * t);
            self.side_banner(
                Vec3::new(p.x, 6.2, p.y),
                Vec2::new(inward.x, 0.0),
                2.6,
                5.0,
                color,
                ours,
            );
        }
        // Barriers along the two inner edges, with a wide gap in each.
        for (u0, v0, u1, v1) in [
            (2.0, court, court * 0.35, court + 1.4),
            (court * 0.65, court, court + 1.4, court + 1.4),
            (court, 2.0, court + 1.4, court * 0.35),
            (court, court * 0.65, court + 1.4, court),
        ] {
            box_uv(
                self,
                u0,
                v0,
                u1,
                v1,
                1.7,
                stone * 0.85,
                Material::Concrete,
                true,
            );
        }
        // Supply containers stacked in the corner, in the side's color.
        for (u, v, h) in [(3.0f32, 4.0f32, 0.0f32), (3.0, 4.0, 2.6), (7.0, 3.0, 0.0)] {
            let shade = rng.range(0.8, 1.0);
            let (a, b) = (at(u - 1.2, v - 0.0), at(u + 1.2, v + 5.0));
            let (lo, hi) = (a.min(b), a.max(b));
            self.slab(
                Vec3::new(lo.x, h, lo.y),
                Vec3::new(hi.x, h + 2.5, hi.y),
                0.0,
                color * 0.8 * shade + Vec3::splat(0.05),
                Material::Painted,
                true,
            );
        }
        // Masts at the gaps, flying the side's color.
        for (u, v) in [(court - 3.0, court * 0.5), (court * 0.5, court - 3.0)] {
            let p = at(u, v);
            self.pipe(
                Vec3::new(p.x, 0.0, p.y),
                Vec3::new(p.x, 12.0, p.y),
                0.16,
                CONCRETE_DARK,
                Material::Painted,
                true,
            );
            let face = (middle - p).normalize();
            self.side_banner(
                Vec3::new(p.x, 11.6, p.y) + Vec3::new(face.x, 0.0, face.y) * 0.1,
                face,
                2.4,
                6.0,
                color,
                ours,
            );
        }
    }

    /// Beyond the edge the city goes on, on flat ground out to the haze:
    /// concrete towers and slabs, banded with dark ribbon windows on the
    /// faces toward the city, some broken off, taller the further out. The
    /// avenues' vistas stay open, each closed far off by a great tower
    /// hung with crimson. Visual only.
    fn skyline(&mut self, rng: &mut Rng, sk: &Skeleton) {
        let avenues: Vec<Street> = sk.avenues().filter(|a| a.out).copied().collect();
        self.skyline_ground(sk, GROUND);
        self.skyline_lots(rng, sk, |map, rng, lot, h, broken, (shade, toward)| {
            map.skyline_tower(rng, lot.center, lot.half, h, broken, shade, toward);
        });

        // At the end of each vista, far out: a great tower, crimson down
        // its face.
        for a in &avenues {
            let dir = a.dir();
            let p = a.to + dir * 220.0;
            let mark = Mark::of(self);
            let yaw = -angle(dir);
            let half = Vec2::new(12.0, 16.0);
            let h = 110.0;
            self.oboxes.push(OBox {
                center: p,
                yaw,
                half,
                y0: 0.0,
                y1: h,
                color: CONCRETE * 0.7,
                side_mat: Material::Wall,
                top_mat: Material::Concrete,
                solid: false,
                ..OBox::default()
            });
            // A crown of setbacks, each under a cornice, and a mast.
            let mut hh = half;
            let mut y = h;
            for rise in [0.0, 7.0, 5.0] {
                if rise > 0.0 {
                    self.oboxes.push(OBox {
                        center: p,
                        yaw,
                        half: hh,
                        y0: y,
                        y1: y + rise,
                        color: CONCRETE * 0.66,
                        side_mat: Material::Concrete,
                        top_mat: Material::Concrete,
                        solid: false,
                        ..OBox::default()
                    });
                    y += rise;
                }
                self.oboxes.push(OBox {
                    center: p,
                    yaw,
                    half: hh + Vec2::splat(0.8),
                    y0: y - 1.2,
                    y1: y + 0.1,
                    color: CONCRETE_DARK,
                    side_mat: Material::Concrete,
                    top_mat: Material::Concrete,
                    solid: false,
                    ..OBox::default()
                });
                hh -= Vec2::splat(3.0);
            }
            self.oboxes.push(OBox {
                center: p,
                half: Vec2::splat(0.5),
                y0: y,
                y1: y + 22.0,
                color: CONCRETE_DARK,
                side_mat: Material::Metal,
                top_mat: Material::Metal,
                solid: false,
                ..OBox::default()
            });
            let face = Vec3::new(p.x, 0.0, p.y) - Vec3::new(dir.x, 0.0, dir.y) * (half.x + 0.1);
            self.side_banner(
                Vec3::new(face.x, h - 6.0, face.z),
                -dir,
                14.0,
                70.0,
                CRIMSON,
                true,
            );
            mark.raise(self, sk.ground(p));
        }
    }

    /// The flat ground beyond the edge, `color`.
    pub(super) fn skyline_ground(&mut self, sk: &Skeleton, color: Vec3) {
        let size = sk.size;
        // Flat ground from the edge out past the far plane, matching the
        // play ground at the edge (grid lines on 0 and size); under the
        // arena it sinks out of sight.
        let (cell, reach) = (4.0f32, 560.0f32);
        let n = ((size + 2.0 * reach) / cell) as usize + 1;
        // (A shallow step at the edge, then down: its normals at the edge stay
        // flat, so it shades like the play ground there and no seam shows.)
        let mut t = Terrain::from_fn(Vec2::splat(-reach), cell, n, n, |x, z| {
            let inside = |m: f32| x > m && z > m && x < size - m && z < size - m;
            let h = sk.ground(Vec2::new(x, z));
            if inside(cell + 0.01) {
                h - 8.0
            } else if inside(0.01) {
                h - 0.25
            } else {
                h
            }
        });
        t.color = color;
        t.material = Material::Concrete;
        self.scenery_terrain = Some(t);
    }

    /// Lots for the skyline, past where the city's own streets are laid out
    /// and off the avenues' vistas: `tower` builds on each, given its lot,
    /// its height, whether it's broken, and how dark it is and which way
    /// the city lies.
    pub(super) fn skyline_lots(
        &mut self,
        rng: &mut Rng,
        sk: &Skeleton,
        tower: impl Fn(&mut Map, &mut Rng, &OBox, f32, bool, (f32, Vec2)),
    ) {
        let size = sk.size;
        let mid = size * 0.5;
        let avenues: Vec<Street> = sk.avenues().filter(|a| a.out).copied().collect();
        let mut placed: Vec<OBox> = Vec::new();
        for _ in 0..1600 {
            let p = Vec2::new(
                rng.range(-380.0, size + 380.0),
                rng.range(-380.0, size + 380.0),
            );
            // Past where the city's own streets are laid out.
            let edge = super::city::SPRAWL - 6.0;
            let outside = p.x < -edge || p.y < -edge || p.x > size + edge || p.y > size + edge;
            let d = p - Vec2::splat(mid);
            // Kept open down each avenue, out past its end.
            let vista = avenues.iter().any(|a| {
                let t = (p - a.from).dot(a.dir());
                t > 0.0 && (p - a.from).perp_dot(a.dir()).abs() < a.half + 10.0
            });
            if !outside || vista {
                continue;
            }
            let far = (d.abs().max_element() - mid).max(0.0);
            // Thinner out in the haze.
            if rng.f32() < far / 400.0 {
                continue;
            }
            let half = Vec2::new(rng.range(7.0, 18.0), rng.range(7.0, 18.0));
            let lot = OBox {
                center: p,
                half,
                ..OBox::default()
            };
            if placed.iter().any(|o| o.overlaps(&lot, 5.0)) {
                continue;
            }
            let h = rng.range(10.0, 26.0) + far * rng.range(0.1, 0.3);
            // A little darker than the city inside: they sit back in the haze.
            let broken = rng.f32() < 0.3;
            let toward = Vec2::new(-d.x.signum(), -d.y.signum());
            let shade = rng.range(0.8, 0.95);
            let mark = Mark::of(self);
            tower(self, rng, &lot, h, broken, (shade, toward));
            mark.raise(self, sk.ground(lot.center));
            placed.push(lot);
        }
    }
}
