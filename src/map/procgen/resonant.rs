//! The Resonant's style: a city built round a crystal, as their machines
//! are. Ivory and gilt under steep slate roofs, pointed arches, bells,
//! banners, light running in the streets like veins (see docs/setting.md).
//! They mend what's broken and keep the scars: cracks are filled with gilt.
//!
//! - the landmark: the cathedral, a round choir under a tall slate cone
//!   carried on ribs, a lantern and a spire over it, its crystal floating
//!   over the altar, fallen in on one side; bells and shrines round it.
//! - gates: pointed arches with a bell hung in them. Monuments: relics in
//!   gilt cases. Masts: ivory posts with blue banners. Fountains: fonts
//!   with crystal growing in them.
//! - lots: steep-roofed halls with buttresses and pinnacles, ordinary
//!   houses (many shelled), cloisters raised on pillars, low chapels.
//! - the edge: rubble, tombs dragged across the side streets.
//! - beyond: gables and spires to the haze, a great bell tower closing each
//!   avenue's vista.

mod civics;
mod great;

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, TAU};

use glam::{Quat, Vec2, Vec3};

use super::imperium::frame;
use super::shield::{angle, apart, between_doors, dir, precinct_spots, ring, shelled};
use super::skeleton::*;
use super::{Centerpiece, Style};
use crate::map::landmarks::Rig;
use crate::map::ruins::{self, Drift};
use crate::map::{Ambient, CRYSTAL_LIGHT, Crystal, GlowPath, Map, OBox};
use crate::render::PointLight;
use crate::render::material::Material;
use crate::rng::Rng;

const GROUND: Vec3 = Vec3::new(0.38, 0.35, 0.3);
const IVORY: Vec3 = Vec3::new(0.74, 0.69, 0.58);
const GILT: Vec3 = Vec3::new(0.62, 0.47, 0.22);
const BRONZE: Vec3 = Vec3::new(0.5, 0.35, 0.18);
const SLATE: Vec3 = Vec3::new(0.17, 0.2, 0.28);
const CLOTH: Vec3 = Vec3::new(0.12, 0.26, 0.7);
const GLASS: Vec3 = Vec3::new(0.05, 0.1, 0.22);

const WALLS: [Vec3; 5] = [
    Vec3::new(0.74, 0.69, 0.58),
    Vec3::new(0.66, 0.6, 0.5),
    Vec3::new(0.7, 0.68, 0.63),
    Vec3::new(0.6, 0.52, 0.42),
    Vec3::new(0.5, 0.54, 0.6),
];
/// The ordinary houses: plaster over stone.
const HOUSES: ruins::Style = ruins::Style {
    colors: &WALLS,
    wall: Material::Plaster,
};

/// The cathedral's wall: thickness, height; its doorways' height.
const WALL_T: f32 = 2.2;
const WALL_H: f32 = 14.0;
/// The lantern's foot (radius, height) and the spire's tip.
const LANTERN_R: f32 = 5.0;
const LANTERN_Y: f32 = 36.0;
const SPIRE: f32 = 72.0;

pub(super) struct Resonant;

impl Style for Resonant {
    fn ground(&self) -> Vec3 {
        GROUND
    }

    fn road(&self) -> Vec3 {
        Vec3::new(0.3, 0.32, 0.38)
    }

    fn stone(&self) -> super::civic::Stone {
        super::civic::Stone {
            wall: IVORY,
            mat: Material::Plaster,
            trim: GILT,
            roof: SLATE,
            land: GROUND,
        }
    }

    fn civic(&self, map: &mut Map, lot: &Lot, kind: CivicKind, rng: &mut Rng) {
        civics::build(map, rng, lot, kind, self.stone());
    }

    fn hold_name(&self, piece: Centerpiece, kind: LandmarkKind) -> &'static str {
        match piece {
            Centerpiece::Monument => match kind {
                LandmarkKind::Great(1) => "ABBEY",
                LandmarkKind::Great(2) => "CARILLON",
                _ => "CATHEDRAL",
            },
            Centerpiece::ShieldGenerator(..) => "GENERATOR",
        }
    }

    fn landmark(&self, map: &mut Map, sk: &Skeleton, lm: &Landmark, rng: &mut Rng) -> Vec<Drift> {
        let base = sk.ground(lm.center);
        match lm.kind {
            LandmarkKind::Great(1) => great::abbey(map, rng, lm, base),
            LandmarkKind::Great(2) => great::carillon(map, rng, lm, base),
            _ => cathedral(map, rng, lm, base),
        }
        for a in between_doors(lm) {
            if !shelled(lm, a) {
                map.bell_frame(lm, a, base);
            }
        }
        for a in precinct_spots(lm, 14).into_iter().flatten() {
            map.shrine(rng, lm, a, base);
        }
        map.generator_rubble(rng, lm, base, IVORY * 0.9)
    }

    fn platform(&self, map: &mut Map, sk: &Skeleton, p: &Platform, _rng: &mut Rng) {
        map.paving(p, &sk.landmarks, IVORY * 0.9);
    }

    fn street(&self, map: &mut Map, sk: &Skeleton, s: &Street, _rng: &mut Rng) {
        if s.rank == Rank::Avenue {
            veins(map, sk, s);
        }
    }

    fn span(&self, map: &mut Map, sk: &Skeleton, s: &Span, _rng: &mut Rng) {
        bell_gate(map, s, sk.ground(s.at));
    }

    fn lot(&self, map: &mut Map, sk: &Skeleton, lot: &Lot, rng: &mut Rng) -> Vec<Drift> {
        let f = &lot.footprint;
        match lot.kind {
            LotKind::Big if rng.chance(0.5) => {
                return ruins::building_in(map, rng, *f, lot.front, &|_| true, &HOUSES);
            }
            LotKind::Big if rng.chance(0.35) => nave(map, rng, f),
            LotKind::Big => {
                let h = rng.range(9.0, 15.0);
                hall(map, rng, f, h, true);
            }
            LotKind::Small => {
                let h = rng.range(4.0, 6.0);
                hall(map, rng, f, h, true);
            }
            LotKind::Raised => cloister(map, rng, f),
            LotKind::Rubble => {
                map.rubble_lot(rng, f, IVORY * 0.85);
                if rng.chance(0.6) {
                    toppled_spire(map, rng, f);
                }
            }
            LotKind::Civic(_)
            | LotKind::Works(_)
            | LotKind::Spires
            | LotKind::Fins
            | LotKind::Boulders => {}
            LotKind::Distant => {
                let out = (-f.center)
                    .max(f.center - Vec2::splat(sk.size))
                    .max_element();
                let h = rng.range(9.0, 18.0) + out * rng.range(0.02, 0.08);
                far_house(map, rng, f, h);
            }
        }
        Vec::new()
    }

    fn base(&self, map: &mut Map, sk: &Skeleton, side: usize, b: &Base, rng: &mut Rng) {
        map.staging_court(rng, side, b, sk.center(), IVORY * 0.9);
    }

    fn edge(&self, map: &mut Map, _sk: &Skeleton, e: &Edge, rng: &mut Rng) {
        match e.kind {
            EdgeKind::Barricade => map.street_barricade(rng, e, IVORY * 0.85),
            EdgeKind::Blockade => map.rubble_bank(
                rng,
                e.start,
                e.along,
                e.inward,
                (e.from, e.to),
                IVORY * 0.85,
            ),
        }
    }

    fn deco(&self, map: &mut Map, sk: &Skeleton, d: &Deco, rng: &mut Rng) {
        let g = sk.ground(d.at);
        match d.kind {
            DecoKind::Monument => relic(map, rng, d.at, d.face, g),
            DecoKind::Mast => mast(map, d.at, d.face, g),
            DecoKind::Fountain => font(map, rng, d.at, g),
            DecoKind::Cover => cover(map, rng, d.at, d.face, g),
        }
    }

    fn beyond(&self, map: &mut Map, sk: &Skeleton, rng: &mut Rng) {
        map.skyline_ground(sk, GROUND);
        map.skyline_lots(rng, sk, |map, rng, lot, h, _, _| {
            far_house(map, rng, lot, 8.0 + h * 0.3)
        });
        for a in sk.avenues().filter(|a| a.out) {
            let (d, p) = (a.dir(), a.to + a.dir() * 220.0);
            let mark = Mark::of(map);
            bell_tower(map, p, -angle(d));
            mark.raise(map, sk.ground(p));
        }
    }

    fn finish(&self, map: &mut Map) {
        for o in &mut map.oboxes {
            let masonry = [Material::Plaster, Material::Rubble].contains(&o.side_mat);
            if o.solid && o.tilt == 0.0 && o.roll == 0.0 && o.chip == 0.0 && masonry {
                o.chip = 0.2;
            }
        }
    }
}

/// A rig standing on `o`'s footprint, its x and z along the footprint's, the
/// long way along z; and the half extents (across, along).
fn stand(o: &OBox, y: f32) -> (Rig, f32, f32) {
    let long_z = o.half.y >= o.half.x;
    let yaw = o.yaw + if long_z { 0.0 } else { FRAC_PI_2 };
    let (w, l) = if long_z {
        (o.half.x, o.half.y)
    } else {
        (o.half.y, o.half.x)
    };
    let rig = Rig {
        pos: Vec3::new(o.center.x, y, o.center.y),
        rot: Quat::from_rotation_y(yaw),
    };
    (rig, w, l)
}

/// A plain part, upright in its rig.
fn put(map: &mut Map, rig: &Rig, at: Vec3, half: Vec3, color: Vec3, mat: Material, solid: bool) {
    rig.part(map, at, half, Quat::IDENTITY, color, mat, solid, 0.0);
}

/// A steep roof in `rig` from height `y`: ridge along z from `z0` to `z1`,
/// `w` to each eave, `rise` high, and a gable closing each end.
#[allow(clippy::too_many_arguments)]
fn roof(
    map: &mut Map,
    rig: &Rig,
    y: f32,
    w: f32,
    (z0, z1): (f32, f32),
    rise: f32,
    wall: Vec3,
    solid: bool,
) {
    let (over, t) = (0.35, 0.18);
    let slope = rise.atan2(w);
    let len = w.hypot(rise) + over;
    let (zm, zh) = ((z0 + z1) * 0.5, (z1 - z0) * 0.5);
    for s in [-1.0f32, 1.0] {
        let c = Vec3::new(
            s * (w - slope.cos() * len * 0.5 + slope.cos() * over),
            y + rise - slope.sin() * len * 0.5,
            zm,
        );
        rig.part(
            map,
            c + Vec3::new(s * slope.sin(), slope.cos(), 0.0) * t,
            Vec3::new(len * 0.5, t, zh + over),
            Quat::from_rotation_z(-s * slope),
            SLATE * if s > 0.0 { 1.0 } else { 0.94 },
            Material::Roof,
            solid,
            0.0,
        );
    }
    put(
        map,
        rig,
        Vec3::new(0.0, y + rise + 0.5, zm),
        Vec3::new(0.14, 0.25, zh + over + 0.1),
        GILT,
        Material::Metal,
        false,
    );
    let n = 10;
    for k in 0..n {
        let (u0, u1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
        put(
            map,
            rig,
            Vec3::new(0.0, y + rise * (u0 + u1) * 0.5, zm),
            Vec3::new(w * (1.0 - u1) + 0.05, rise * 0.5 / n as f32, zh - 0.06),
            wall,
            Material::Plaster,
            false,
        );
    }
    // Under each slope, what the steps leave open.
    for s in [-1.0f32, 1.0] {
        let (t, l) = (0.28, w.hypot(rise) * 0.5);
        let c = Vec3::new(s * w * 0.5, y + rise * 0.5, zm);
        rig.part(
            map,
            c - Vec3::new(s * slope.sin(), slope.cos(), 0.0) * (t + 0.02),
            Vec3::new(l - 0.3, t, zh - 0.03),
            Quat::from_rotation_z(-s * slope),
            wall,
            Material::Plaster,
            false,
            0.0,
        );
    }
    // Finials on the gables.
    for z in [z0, z1] {
        let c = Vec3::new(0.0, y + rise + 1.3, z);
        rig.part(
            map,
            c,
            Vec3::new(0.13, 0.75, 0.13),
            Quat::from_rotation_y(FRAC_PI_4),
            GILT,
            Material::Metal,
            false,
            0.0,
        );
    }
}

/// A pointed window in a wall facing `out` (the rig's x or z, by `side`):
/// glass, lit or not, under a pointed head, a gilt sill.
fn window(map: &mut Map, rig: &Rig, at: Vec3, side: bool, (w, h): (f32, f32), lit: bool) {
    let (color, mat) = if lit {
        (CRYSTAL_LIGHT * 0.6, Material::Glow)
    } else {
        (GLASS, Material::Metal)
    };
    let half = |a: f32, b: f32, t: f32| {
        if side {
            Vec3::new(t, b, a)
        } else {
            Vec3::new(a, b, t)
        }
    };
    put(map, rig, at, half(w, h, 0.08), color, mat, false);
    let turn = if side {
        Quat::from_rotation_x(FRAC_PI_4)
    } else {
        Quat::from_rotation_z(FRAC_PI_4)
    };
    let d = w * std::f32::consts::FRAC_1_SQRT_2;
    rig.part(
        map,
        at + Vec3::Y * h,
        half(d, d, 0.065),
        turn,
        color,
        mat,
        false,
        0.0,
    );
    put(
        map,
        rig,
        at - Vec3::Y * (h + 0.1),
        half(w + 0.2, 0.1, 0.16),
        GILT,
        Material::Metal,
        false,
    );
}

/// A crack mended with gilt, zigzagging up a wall's face at `x` (the face
/// at z `z`).
fn mend(map: &mut Map, rng: &mut Rng, rig: &Rig, x: f32, z: f32, (y0, y1): (f32, f32)) {
    let n = 5;
    let mut a = Vec3::new(x, y0, z);
    for k in 1..=n {
        let b = Vec3::new(
            x + rng.range(-0.7, 0.7),
            y0 + (y1 - y0) * k as f32 / n as f32,
            z,
        );
        map.segment(
            rig.point(a),
            rig.point(b),
            0.06,
            GILT,
            Material::Metal,
            false,
        );
        a = b;
    }
}

/// A hall on `lot`, its wall `h` high: buttressed ivory under a steep slate
/// roof, pinnacles at the corners. Some are shelled: one end broken down.
fn hall(map: &mut Map, rng: &mut Rng, lot: &OBox, h: f32, solid: bool) {
    let (rig, w, l) = stand(lot, 0.0);
    let color = WALLS[rng.index(WALLS.len())] * rng.range(0.94, 1.03);
    let (w, l) = (w - 0.7, l - 0.4);
    let rise = w * rng.range(1.2, 1.6);
    let shelled = rng.chance(0.35);
    let cut = if shelled {
        rng.range(-0.1, 0.35) * l
    } else {
        l
    };
    put(
        map,
        &rig,
        Vec3::new(0.0, h * 0.5 - 0.2, (cut - l) * 0.5),
        Vec3::new(w, h * 0.5 + 0.2, (cut + l) * 0.5),
        color,
        Material::Plaster,
        solid,
    );
    map.oboxes.last_mut().unwrap().chip = 0.01;
    roof(map, &rig, h, w, (-l, cut), rise, color * 0.96, solid);
    if shelled {
        let low = rng.range(0.2, 0.45) * h;
        put(
            map,
            &rig,
            Vec3::new(0.0, low * 0.5 - 0.2, (cut + l) * 0.5),
            Vec3::new(w - 0.05, low * 0.5 + 0.2, (l - cut) * 0.5),
            color * 0.9,
            Material::Plaster,
            solid,
        );
        let o = *map.oboxes.last().unwrap();
        map.ragged_top(rng, &o);
        // The standing part's torn end: broken stubs of wall against it,
        // highest at the sides.
        let n = 5;
        for k in 0..n {
            let u = (k as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let top = (low + (h - low) * (0.25 + 0.6 * u.abs()) * rng.range(0.7, 1.1)).min(h);
            put(
                map,
                &rig,
                Vec3::new(u * w, top * 0.5, cut + rng.range(0.2, 0.5)),
                Vec3::new(w / n as f32 * 0.96 - 0.12, top * 0.5, 0.6),
                color * rng.range(0.88, 0.96),
                Material::Plaster,
                false,
            );
            map.oboxes.last_mut().unwrap().broken = true;
        }
    }
    // Bays down each side: a buttress, a window.
    let bays = ((l * 2.0) / 3.6).round().max(2.0) as i32;
    let step = l * 2.0 / bays as f32;
    for k in 0..=bays {
        let z = -l + step * k as f32;
        let z = z.clamp(-l + 0.45, l - 0.45);
        if z > cut {
            continue;
        }
        for s in [-1.0f32, 1.0] {
            put(
                map,
                &rig,
                Vec3::new(s * (w + 0.25), h * 0.42, z),
                Vec3::new(0.4, h * 0.42, 0.42),
                color * 0.92,
                Material::Plaster,
                solid,
            );
            if k == 0 || k == bays {
                let y = h * 0.84;
                put(
                    map,
                    &rig,
                    Vec3::new(s * (w + 0.1), y + 1.5, z),
                    Vec3::new(0.36, 1.6, 0.36),
                    color * 0.95,
                    Material::Plaster,
                    false,
                );
                rig.part(
                    map,
                    Vec3::new(s * (w + 0.1), y + 3.5, z),
                    Vec3::new(0.2, 0.7, 0.2),
                    Quat::from_rotation_y(FRAC_PI_4),
                    GILT,
                    Material::Metal,
                    false,
                    0.0,
                );
            }
            let mid = z + step * 0.5;
            if k < bays && mid + 0.8 < cut {
                let lit = rng.chance(0.3);
                window(
                    map,
                    &rig,
                    Vec3::new(s * (w + 0.02), h * 0.5, mid),
                    true,
                    (0.55, h * 0.24),
                    lit,
                );
            }
        }
    }
    // The gable ends: a great window; a door's head under it.
    for (z, s) in [(-l, -1.0f32), (cut, 1.0)] {
        if s > 0.0 && shelled {
            continue;
        }
        let lit = rng.chance(0.6);
        window(
            map,
            &rig,
            Vec3::new(0.0, h * 0.55, z + s * 0.02),
            false,
            (w * 0.22, h * 0.28),
            lit,
        );
        if rng.chance(0.4) {
            let x = w * if rng.chance(0.5) { 0.65 } else { -0.65 };
            mend(map, rng, &rig, x, z + s * 0.03, (0.3, h * 0.85));
        }
    }
}

/// A nave on `lot`, open at both ends: two walls of lit lancets, pointed
/// arches and a steep roof over; tanks drive right through.
fn nave(map: &mut Map, rng: &mut Rng, lot: &OBox) {
    let (rig, w, l) = stand(lot, 0.0);
    let color = WALLS[rng.index(WALLS.len())];
    let (h, t) = (rng.range(8.0, 10.5), 0.6);
    let (w, l) = (w - 0.4, l - 0.3);
    for s in [-1.0f32, 1.0] {
        put(
            map,
            &rig,
            Vec3::new(s * (w - t), h * 0.5 - 0.1, 0.0),
            Vec3::new(t, h * 0.5 + 0.1, l),
            color,
            Material::Plaster,
            true,
        );
        map.oboxes.last_mut().unwrap().chip = 0.01;
        for k in -1..=1 {
            let z = k as f32 * l * 0.62;
            for x in [w + 0.02, w - t * 2.0 - 0.02] {
                window(
                    map,
                    &rig,
                    Vec3::new(s * x, h * 0.52, z),
                    true,
                    (0.6, h * 0.26),
                    true,
                );
            }
        }
        // Each end's half of the arch, and one in the middle.
        for z in [-l + 0.4, 0.0, l - 0.4] {
            map.pipe(
                rig.point(Vec3::new(s * (w - t * 2.0), h * 0.72, z)),
                rig.point(Vec3::new(0.0, h + w * 1.2, z)),
                0.3,
                GILT,
                Material::Metal,
                false,
            );
        }
    }
    // Just the slopes: the ends stay open under the gables' arches.
    let rise = w * 1.3;
    let slope = rise.atan2(w);
    let len = w.hypot(rise) + 0.35;
    for s in [-1.0f32, 1.0] {
        rig.part(
            map,
            Vec3::new(s * w * 0.5, h + rise * 0.5, 0.0)
                + Vec3::new(s * slope.sin(), slope.cos(), 0.0) * 0.2,
            Vec3::new(len * 0.5, 0.2, l + 0.3),
            Quat::from_rotation_z(-s * slope),
            SLATE,
            Material::Roof,
            true,
            0.0,
        );
    }
    put(
        map,
        &rig,
        Vec3::new(0.0, h + rise + 0.45, 0.0),
        Vec3::new(0.14, 0.25, l + 0.4),
        GILT,
        Material::Metal,
        false,
    );
    let p = rig.point(Vec3::new(0.0, h * 0.8, 0.0));
    map.lights.push(PointLight {
        pos: p,
        color: CRYSTAL_LIGHT * 5.0,
        radius: 14.0,
    });
}

/// A steeple that came down on `lot`, lying along it: cover from end to end.
fn toppled_spire(map: &mut Map, rng: &mut Rng, lot: &OBox) {
    let (rig, w, l) = stand(lot, 0.0);
    let x = rng.range(-0.4, 0.4) * w;
    let end = if rng.chance(0.5) { 1.0 } else { -1.0 };
    let at = |u: f32| rig.point(Vec3::new(x, 0.0, end * l * (u * 2.0 - 1.0) * 0.9));
    let n = 4;
    for k in 0..n {
        let r = 2.0 - 1.4 * (k as f32 + 0.5) / n as f32;
        let (a, b) = (at(k as f32 / n as f32), at((k as f32 + 0.93) / n as f32));
        let up = Vec3::Y * (r - 0.15);
        let slate = SLATE * rng.range(0.9, 1.1);
        map.pipe(a + up, b + up, r, slate, Material::Roof, true);
    }
    let tip = at(1.0) + Vec3::Y * 0.4;
    map.segment(
        at(0.96) + Vec3::Y * 0.5,
        tip,
        0.12,
        GILT,
        Material::Metal,
        false,
    );
}

/// A cloister on `lot`: a heavy upper storey and steep roof lifted on
/// pillars, pointed arches between them; tanks drive under it.
fn cloister(map: &mut Map, rng: &mut Rng, lot: &OBox) {
    let (rig, w, l) = stand(lot, 0.0);
    let color = WALLS[rng.index(WALLS.len())];
    let (lift, h) = (6.5, rng.range(4.5, 7.5));
    put(
        map,
        &rig,
        Vec3::new(0.0, lift * 0.5, 0.0),
        Vec3::new(w * 0.3, lift * 0.5, l * 0.32),
        color * 0.85,
        Material::Plaster,
        true,
    );
    // The core holds a light.
    for s in [-1.0f32, 1.0] {
        window(
            map,
            &rig,
            Vec3::new(s * (w * 0.3 + 0.02), 2.8, 0.0),
            true,
            (0.6, 1.4),
            true,
        );
    }
    for px in [-1.0f32, 1.0] {
        for pz in [-1.0f32, 0.0, 1.0] {
            put(
                map,
                &rig,
                Vec3::new(px * (w - 1.2), lift * 0.5, pz * (l - 1.2)),
                Vec3::new(0.65, lift * 0.5, 0.65),
                color,
                Material::Plaster,
                true,
            );
            put(
                map,
                &rig,
                Vec3::new(px * (w - 1.2), 4.3, pz * (l - 1.2)),
                Vec3::new(0.8, 0.14, 0.8),
                GILT,
                Material::Metal,
                false,
            );
        }
        // Two pointed arches between the three pillars.
        for pz in [-0.5f32, 0.5] {
            let (x, z, r) = (px * (w - 1.2), pz * (l - 1.2), (l - 1.2) * 0.5 - 0.5);
            for s in [-1.0f32, 1.0] {
                map.segment(
                    rig.point(Vec3::new(x, 4.4, z + s * r)),
                    rig.point(Vec3::new(x, lift - 0.1, z)),
                    0.22,
                    color * 0.94,
                    Material::Plaster,
                    false,
                );
            }
        }
    }
    let y = lift + h;
    put(
        map,
        &rig,
        Vec3::new(0.0, lift + h * 0.5, 0.0),
        Vec3::new(w - 0.1, h * 0.5, l - 0.1),
        color,
        Material::Plaster,
        true,
    );
    map.oboxes.last_mut().unwrap().chip = 0.01;
    put(
        map,
        &rig,
        Vec3::new(0.0, lift + 0.1, 0.0),
        Vec3::new(w + 0.15, 0.28, l + 0.15),
        color * 0.85,
        Material::Plaster,
        false,
    );
    roof(
        map,
        &rig,
        y,
        w - 0.1,
        (-l + 0.1, l - 0.1),
        w * 1.1,
        color * 0.96,
        true,
    );
    let n = (l * 2.0 / 3.0) as i32;
    for k in 0..n {
        let z = -l + (k as f32 + 0.5) * l * 2.0 / n as f32;
        for s in [-1.0f32, 1.0] {
            let lit = rng.chance(0.35);
            window(
                map,
                &rig,
                Vec3::new(s * (w - 0.08), lift + h * 0.5, z),
                true,
                (0.5, h * 0.26),
                lit,
            );
        }
    }
    for s in [-1.0f32, 1.0] {
        window(
            map,
            &rig,
            Vec3::new(0.0, lift + h * 0.5, s * (l - 0.08)),
            false,
            (w * 0.2, h * 0.3),
            true,
        );
    }
}

/// A building far off, only seen: walls, a steep roof, now and then a spire.
fn far_house(map: &mut Map, rng: &mut Rng, lot: &OBox, h: f32) {
    let (rig, w, l) = stand(lot, 0.0);
    let color = WALLS[rng.index(WALLS.len())] * rng.range(0.75, 0.9);
    put(
        map,
        &rig,
        Vec3::new(0.0, h * 0.5 - 0.3, 0.0),
        Vec3::new(w, h * 0.5 + 0.3, l),
        color,
        Material::Plaster,
        false,
    );
    // A row of tall windows each way, a few lit.
    for s in [-1.0f32, 1.0] {
        for k in -1..=1 {
            let lit = rng.chance(0.25);
            let at = Vec3::new(s * (w + 0.04), h * 0.52, k as f32 * l * 0.6);
            window(map, &rig, at, true, (l * 0.12, h * 0.2), lit);
        }
        let lit = rng.chance(0.4);
        let at = Vec3::new(0.0, h * 0.55, s * (l + 0.04));
        window(map, &rig, at, false, (w * 0.2, h * 0.24), lit);
    }
    if rng.chance(0.3) {
        // A tower: a square spire, stepping in.
        let r = w.min(l) * 0.55;
        let top = h + rng.range(8.0, 14.0);
        put(
            map,
            &rig,
            Vec3::new(0.0, (h + top) * 0.5, 0.0),
            Vec3::new(r, (top - h) * 0.5, r),
            color * 0.97,
            Material::Plaster,
            false,
        );
        spire(map, &rig, Vec3::new(0.0, top, 0.0), r, r * 4.0);
        return;
    }
    roof(
        map,
        &rig,
        h,
        w,
        (-l, l),
        w * rng.range(1.0, 1.4),
        color * 0.96,
        false,
    );
}

/// A round spire of slates from `at` up `h`, `r` at the foot.
fn spire(map: &mut Map, rig: &Rig, at: Vec3, r: f32, h: f32) {
    cone(map, rig.point(at), (r * 1.15, 0.0), (0.12, h), 3, |_, _| {
        false
    });
    put(
        map,
        rig,
        at + Vec3::Y * (h + 0.8),
        Vec3::new(0.12, 1.0, 0.12),
        GILT,
        Material::Metal,
        false,
    );
}

/// The cathedral in the landmark's place: a round wall of lancets between
/// buttresses, open at its doors, ribs and a slate cone up to a lantern and
/// spire, the altar's crystal floating under it.
fn cathedral(map: &mut Map, rng: &mut Rng, lm: &Landmark, base: f32) {
    let c = lm.center;
    let at = |y: f32| Vec3::new(c.x, base + y, c.y);
    let polar = |a: f32, r: f32, y: f32| at(y) + Vec3::new(a.cos(), 0.0, a.sin()) * r;
    let fallen = |a: f32, spread: f32| lm.fallen.is_some_and(|f| apart(a, angle(f)) < spread);
    for (k, (a, half)) in ring(lm, lm.radius, 40).into_iter().enumerate() {
        let d = dir(a);
        let yaw = -a + FRAC_PI_2;
        let p = c + d * (lm.radius - WALL_T * 0.5);
        if shelled(lm, a) {
            let h = rng.range(0.8, 4.5);
            map.block(
                p,
                Vec2::new(half, WALL_T * 0.5),
                yaw,
                (base, base + h),
                IVORY * 0.9,
                Material::Plaster,
                true,
            )
            .broken = true;
            continue;
        }
        map.block(
            p,
            Vec2::new(half, WALL_T * 0.5),
            yaw,
            (base, base + WALL_H),
            IVORY,
            Material::Plaster,
            true,
        )
        .chip = 0.01;
        let q = c + d * (lm.radius + 0.1);
        map.block(
            q,
            Vec2::new(half * 1.05, 1.3),
            yaw,
            (base + WALL_H, base + WALL_H + 0.6),
            GILT,
            Material::Metal,
            false,
        );
        let rig = Rig {
            pos: Vec3::new(c.x, base, c.y),
            rot: Quat::from_rotation_y(yaw),
        };
        // The rig's +z is along the spoke.
        let out = if rig.rot.mul_vec3(Vec3::Z).dot(Vec3::new(d.x, 0.0, d.y)) > 0.0 {
            1.0
        } else {
            -1.0
        };
        if k % 3 == 0 {
            let q = c + d * (lm.radius + 0.7);
            map.block(
                q,
                Vec2::new(0.7, 1.2),
                yaw,
                (base, base + WALL_H + 2.5),
                IVORY * 0.94,
                Material::Plaster,
                true,
            );
            map.block(
                q,
                Vec2::splat(0.5),
                yaw + FRAC_PI_4,
                (base + WALL_H + 2.5, base + WALL_H + 4.8),
                GILT,
                Material::Metal,
                false,
            );
        } else {
            for r in [lm.radius + 0.03, lm.radius - WALL_T - 0.03] {
                window(
                    map,
                    &rig,
                    Vec3::new(0.0, 6.8, out * r),
                    false,
                    (half * 0.5, 4.0),
                    true,
                );
            }
        }
    }
    // Each doorway: a pointed gilt arch over it, the wall's line carried on.
    for &d in &lm.doors {
        let a = angle(d);
        let (u, v) = (
            polar(a - 0.15, lm.radius - 0.4, 0.0),
            polar(a + 0.15, lm.radius - 0.4, 0.0),
        );
        let apex = polar(a, lm.radius - 0.4, WALL_H + 3.0);
        for e in [u, v] {
            map.pipe(e + Vec3::Y * 6.0, apex, 0.4, GILT, Material::Metal, false);
        }
    }
    // The ribs, and the roof between them.
    let eave = lm.radius - 0.8;
    for k in 0..16 {
        let a = (k as f32 + 0.5) / 16.0 * TAU;
        if fallen(a, 0.75) {
            continue;
        }
        map.pipe(
            polar(a, eave - 1.2, WALL_H - 0.6),
            polar(a, LANTERN_R, LANTERN_Y - 0.6),
            0.32,
            IVORY * 0.95,
            Material::Plaster,
            false,
        );
    }
    cone(
        map,
        at(0.0),
        (eave, WALL_H + 0.8),
        (LANTERN_R, LANTERN_Y),
        7,
        |a, i| fallen(a, 1.05 + 0.25 * rng.f32() - 0.08 * i as f32),
    );
    // The lantern and the spire.
    for k in 0..8 {
        let a = (k as f32 + 0.5) / 8.0 * TAU;
        map.pipe(
            polar(a, LANTERN_R, LANTERN_Y - 0.6),
            polar(a, LANTERN_R, LANTERN_Y + 7.0),
            0.3,
            GILT,
            Material::Metal,
            false,
        );
        let b = a + TAU / 8.0;
        let m = (a + b) * 0.5;
        for e in [a, b] {
            map.segment(
                polar(e, LANTERN_R, LANTERN_Y + 4.0),
                polar(m, LANTERN_R - 0.3, LANTERN_Y + 6.8),
                0.14,
                GILT,
                Material::Metal,
                false,
            );
        }
    }
    cone(
        map,
        at(0.0),
        (LANTERN_R + 0.5, LANTERN_Y + 7.0),
        (0.3, SPIRE),
        9,
        |_, _| false,
    );
    map.pipe(
        at(SPIRE - 1.0),
        at(SPIRE + 5.0),
        0.14,
        GILT,
        Material::Metal,
        false,
    );
    map.sound(Ambient::Bell, at(LANTERN_Y + 4.0));
    // Pillars inside, clear of the doors.
    for k in 0..8 {
        let a = (k as f32 + 0.5) / 8.0 * TAU;
        if fallen(a, 0.75) || lm.doors.iter().any(|&d| apart(a, angle(d)) < 0.3) {
            continue;
        }
        let p = c + dir(a) * 14.0;
        map.block(
            p,
            Vec2::splat(0.8),
            -a,
            (base, base + WALL_H + 5.0),
            IVORY * 0.96,
            Material::Plaster,
            true,
        );
        map.block(
            p,
            Vec2::splat(1.05),
            -a,
            (base + 7.0, base + 7.4),
            GILT,
            Material::Metal,
            false,
        );
    }
    // The altar and its crystal.
    map.drum(at(0.0), at(1.2), 4.5, IVORY * 0.95);
    map.block(
        c,
        Vec2::splat(1.3),
        0.0,
        (base + 1.2, base + 3.4),
        IVORY,
        Material::Plaster,
        true,
    );
    map.block(
        c,
        Vec2::splat(1.5),
        0.0,
        (base + 3.4, base + 3.7),
        GILT,
        Material::Metal,
        false,
    );
    for (len, up) in [(6.5f32, 1.0f32), (4.0, -1.0)] {
        map.crystals.push(Crystal {
            base: at(9.5),
            axis: Vec3::Y * len * up,
            radius: 1.4,
            sides: 6,
            seed: rng.next_u64() as u32,
            color: CRYSTAL_LIGHT,
            lit: false,
            solid: false,
            raw: false,
        });
    }
    map.sound(Ambient::Altar, at(9.5));
    map.lights.push(PointLight {
        pos: at(10.0),
        color: CRYSTAL_LIGHT * 14.0,
        radius: 26.0,
    });
}

/// A cone of slates round `c`, from (radius, height) `foot` up to `top` in
/// `rings` rings, bar where `gone` says (its angle, its ring).
fn cone(
    map: &mut Map,
    c: Vec3,
    foot: (f32, f32),
    top: (f32, f32),
    rings: usize,
    mut gone: impl FnMut(f32, usize) -> bool,
) {
    let (run, rise) = (foot.0 - top.0, top.1 - foot.1);
    for i in 0..rings {
        let u = (i as f32 + 0.5) / rings as f32;
        let r = foot.0 - run * u;
        let n = ((r * 1.8) as usize).max(10);
        for k in 0..n {
            let a = (k as f32 + 0.5) / n as f32 * TAU;
            if gone(a, i) {
                continue;
            }
            let tangent = Vec3::new(-a.sin(), 0.0, a.cos());
            let normal = Vec3::new(a.cos() * rise, run, a.sin() * rise).normalize();
            let rig = Rig {
                pos: c + Vec3::new(a.cos() * r, foot.1 + rise * u, a.sin() * r),
                rot: frame(tangent, normal),
            };
            put(
                map,
                &rig,
                Vec3::Y * 0.02 * (k % 2) as f32,
                Vec3::new(
                    (r + run * 0.5 / rings as f32) * std::f32::consts::PI / n as f32 * 1.02,
                    0.16,
                    run.hypot(rise) / rings as f32 * 0.53,
                ),
                SLATE * if k % 2 == 0 { 1.0 } else { 0.95 },
                Material::Roof,
                false,
            );
        }
    }
}

/// Light running in the paving down both sides of an avenue, out from the
/// plaza.
fn veins(map: &mut Map, sk: &Skeleton, s: &Street) {
    let d = s.dir();
    let side = Vec2::new(-d.y, d.x);
    let (start, end) = sk.off_plaza_span(s, 2.0);
    // A little short of each bend, where the next stretch's begin.
    let start = start + 2.0;
    let end = if s.out { end - 6.0 } else { end - 2.0 };
    if end - start < 8.0 {
        return;
    }
    // Stretches that cross lie one over the other.
    let lift = ((s.from.x * 3.7 + s.from.y * 5.3) as i32).rem_euclid(9) as f32 * 0.012;
    for k in [-1.0f32, 1.0] {
        let n = ((end - start) / 2.0).max(1.0) as usize;
        let points: Vec<Vec3> = (0..=n)
            .map(|i| {
                let t = start + (end - start) * i as f32 / n as f32;
                let wobble = (i as f32 * 1.7 + k).sin() * 0.35;
                let p = s.from + d * t + side * (k * (s.half - 2.6) + wobble);
                Vec3::new(p.x, sk.ground(p) + 0.1 + lift, p.y)
            })
            .collect();
        map.glow_paths.push(GlowPath {
            widths: vec![0.22; points.len()],
            points,
            color: CRYSTAL_LIGHT * 0.8,
        });
    }
}

/// A gate over a street: two buttressed piers, a pointed arch, a bell hung
/// from its point. Ruined, one pier's broken and its half of the arch gone.
fn bell_gate(map: &mut Map, s: &Span, g: f32) {
    let yaw = (-s.along.y).atan2(s.along.x);
    let rig = Rig {
        pos: Vec3::new(s.at.x, g, s.at.y),
        // Local x across the street, z along it.
        rot: Quat::from_rotation_y(yaw - FRAC_PI_2),
    };
    let (open, w, h) = (s.opening, 2.2, 13.0);
    let x = open + w;
    let apex = Vec3::new(0.0, h + 8.0, 0.0);
    for k in [-1.0f32, 1.0] {
        let broken = s.ruined && k > 0.0;
        let top = if broken { 2.2 } else { h };
        put(
            map,
            &rig,
            Vec3::new(k * x, top * 0.5, 0.0),
            Vec3::new(w, top * 0.5, 2.2),
            IVORY,
            Material::Plaster,
            true,
        );
        map.oboxes.last_mut().unwrap().chip = 0.01;
        if broken {
            // Broken down to a stump, what fell heaped into a slope up it
            // from either side: somewhere to jump from.
            let (slope, rise) = (0.3f32, 2.2);
            let len = rise / slope.sin();
            for z in [-1.0f32, 1.0] {
                let run = len * slope.cos();
                let c = rig.point(Vec3::new(x, 0.0, z * (2.2 + run * 0.5)));
                let down = rig.rot * Vec3::Z * z;
                map.oboxes.push(OBox {
                    center: Vec2::new(c.x, c.z),
                    yaw: down.x.atan2(down.z),
                    half: Vec2::new(w, len * 0.5),
                    y0: g + rise * 0.5 - 0.3,
                    y1: g + rise * 0.5,
                    color: IVORY * 0.9,
                    side_mat: Material::Plaster,
                    top_mat: Material::Plaster,
                    tilt: slope,
                    ramp: true,
                    ..OBox::default()
                });
            }
            continue;
        }
        put(
            map,
            &rig,
            Vec3::new(k * x, h + 0.25, 0.0),
            Vec3::new(w + 0.3, 0.25, 2.5),
            GILT,
            Material::Metal,
            false,
        );
        // A stepped buttress outside, a niche of light each way.
        put(
            map,
            &rig,
            Vec3::new(k * (x + w + 0.7), 4.0, 0.0),
            Vec3::new(0.75, 4.0, 1.5),
            IVORY * 0.92,
            Material::Plaster,
            true,
        );
        for f in [-1.0f32, 1.0] {
            window(
                map,
                &rig,
                Vec3::new(k * x, 7.0, f * 2.22),
                false,
                (0.6, 2.4),
                true,
            );
        }
        spire(map, &rig, Vec3::new(k * x, h + 0.5, 0.0), 1.4, 7.5);
        // Its half of the arch, three ribs deep.
        for z in [-1.5f32, 0.0, 1.5] {
            let foot = rig.point(Vec3::new(k * (open + 0.6), h - 2.0, z));
            let knee = rig.point(Vec3::new(k * open * 0.6, h + 4.2, z));
            let tip = rig.point(apex + Vec3::Z * z);
            map.pipe(foot, knee, 0.42, IVORY * 0.96, Material::Plaster, false);
            map.pipe(knee, tip, 0.4, IVORY * 0.96, Material::Plaster, false);
        }
    }
    if s.ruined {
        return;
    }
    let p = |y: f32| rig.point(apex - Vec3::Y * y);
    map.segment(p(0.0), p(1.4), 0.08, GILT, Material::Metal, false);
    for (y0, y1, r) in [(1.4, 2.0, 0.7), (2.0, 3.6, 1.1), (3.6, 4.5, 1.5)] {
        map.pipe(p(y1), p(y0), r, BRONZE, Material::Metal, false);
    }
    map.disc(p(1.42), Vec3::Y * 0.03, 0.65, BRONZE, Material::Metal);
    map.sound(Ambient::Bell, p(3.0));
}

/// A relic in its case, facing `face`: an ivory plinth, four gilt posts to
/// a point, a shard of crystal between them (some cases are broken open and
/// empty).
fn relic(map: &mut Map, rng: &mut Rng, at: Vec2, face: Vec2, g: f32) {
    let rig = Rig {
        pos: Vec3::new(at.x, g, at.y),
        rot: Quat::from_rotation_y((-face.y).atan2(face.x) + FRAC_PI_2),
    };
    put(
        map,
        &rig,
        Vec3::new(0.0, 1.3, 0.0),
        Vec3::new(1.5, 1.3, 1.5),
        IVORY * 0.95,
        Material::Plaster,
        true,
    );
    put(
        map,
        &rig,
        Vec3::new(0.0, 2.72, 0.0),
        Vec3::new(1.7, 0.12, 1.7),
        GILT,
        Material::Metal,
        false,
    );
    let broken = rng.chance(0.25);
    for (x, z) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let foot = Vec3::new(x * 1.1, 2.84, z * 1.1);
        let knee = Vec3::new(x * 1.1, 5.8, z * 1.1);
        if broken && x > 0.0 {
            map.segment(
                rig.point(foot),
                rig.point(foot + Vec3::Y * rng.range(0.6, 1.6)),
                0.12,
                GILT * 0.8,
                Material::Metal,
                false,
            );
            continue;
        }
        map.segment(
            rig.point(foot),
            rig.point(knee),
            0.13,
            GILT,
            Material::Metal,
            false,
        );
        if !broken {
            map.segment(
                rig.point(knee),
                rig.point(Vec3::new(0.0, 8.4, 0.0)),
                0.11,
                GILT,
                Material::Metal,
                false,
            );
        }
    }
    if broken {
        return;
    }
    for (len, up) in [(1.6f32, 1.0f32), (1.0, -1.0)] {
        map.crystals.push(Crystal {
            base: rig.point(Vec3::new(0.0, 4.4, 0.0)),
            axis: Vec3::Y * len * up,
            radius: 0.45,
            sides: 6,
            seed: rng.next_u64() as u32,
            color: CRYSTAL_LIGHT,
            lit: false,
            solid: false,
            raw: false,
        });
    }
}

/// An ivory post with a long blue banner on a gilt bar, facing along
/// `face`.
fn mast(map: &mut Map, at: Vec2, face: Vec2, g: f32) {
    map.block(
        at,
        Vec2::splat(0.22),
        -angle(face),
        (g, g + 14.0),
        IVORY * 0.9,
        Material::Plaster,
        true,
    );
    map.block(
        at,
        Vec2::splat(0.35),
        -angle(face) + FRAC_PI_4,
        (g + 14.0, g + 14.9),
        GILT,
        Material::Metal,
        false,
    );
    map.side_banner(
        Vec3::new(at.x, g + 13.4, at.y) + Vec3::new(face.x, 0.0, face.y) * 0.2,
        face,
        2.2,
        8.5,
        CLOTH,
        false,
    );
}

/// A font: an eight-sided ivory basin, crystal growing in it.
fn font(map: &mut Map, rng: &mut Rng, c: Vec2, g: f32) {
    let (r, n) = (5.6, 8);
    for k in 0..n {
        let a = (k as f32 + 0.5) / n as f32 * TAU;
        map.block(
            c + dir(a) * r,
            Vec2::new(r * (std::f32::consts::PI / n as f32).tan() + 0.2, 0.45),
            -a + FRAC_PI_2,
            (g - 0.1, g + 1.2),
            IVORY * 0.92,
            Material::Plaster,
            true,
        )
        .chip = 0.01;
    }
    for k in 0..5 {
        let a = k as f32 / 5.0 * TAU + rng.range(-0.3, 0.3);
        let out = if k == 0 { 0.0 } else { 1.7 };
        let lean = if k == 0 { 0.0 } else { rng.range(0.15, 0.35) };
        let h = if k == 0 {
            rng.range(4.5, 6.5)
        } else {
            rng.range(1.6, 3.2)
        };
        let d = dir(a);
        map.crystals.push(Crystal {
            base: Vec3::new(c.x + d.x * out, g - 0.6, c.y + d.y * out),
            axis: Vec3::new(d.x * lean, 1.0, d.y * lean).normalize() * (h + 0.6),
            radius: if k == 0 { 1.0 } else { rng.range(0.4, 0.65) },
            sides: 6,
            seed: rng.next_u64() as u32,
            color: CRYSTAL_LIGHT,
            lit: false,
            solid: true,
            raw: false,
        });
    }
    map.sound(Ambient::Shrine, Vec3::new(c.x, g + 2.0, c.y));
}

/// Low cover at `p` along `face`: a stub of wall, a row of tombs, or a
/// bell that came down.
fn cover(map: &mut Map, rng: &mut Rng, p: Vec2, face: Vec2, g: f32) {
    let yaw = angle(face);
    let ax = Vec2::new(yaw.cos(), -yaw.sin());
    match rng.f32() {
        x if x < 0.45 => {
            let half = Vec2::new(rng.range(3.5, 7.0), 0.5);
            let h = rng.range(1.6, 3.2);
            map.block(
                p,
                half,
                yaw,
                (g - 0.2, g + h),
                WALLS[rng.index(WALLS.len())],
                Material::Plaster,
                true,
            )
            .broken = true;
        }
        x if x < 0.85 => {
            for k in -1..=1 {
                let skew = rng.range(-0.15, 0.15);
                let q = p + ax * k as f32 * 2.7;
                map.block(
                    q,
                    Vec2::new(0.95, 0.75),
                    yaw + skew,
                    (g - 0.1, g + 1.1),
                    IVORY * 0.88,
                    Material::Plaster,
                    true,
                );
                map.block(
                    q,
                    Vec2::new(1.1, 0.9),
                    yaw + skew,
                    (g + 1.1, g + 1.35),
                    IVORY * 0.97,
                    Material::Plaster,
                    false,
                );
            }
        }
        _ => {
            let a = Vec3::new(p.x, g + 1.3, p.y);
            let d = Vec3::new(ax.x, 0.0, ax.y);
            map.pipe(
                a - d * 1.4,
                a + d * 0.2,
                0.95,
                BRONZE,
                Material::Metal,
                true,
            );
            map.pipe(
                a + d * 0.2,
                a + d * 1.4,
                1.3,
                BRONZE * 0.9,
                Material::Metal,
                true,
            );
            map.disc(a - d * 1.46, d * 0.03, 0.88, BRONZE, Material::Metal);
        }
    }
}

/// Closing an avenue's vista, far out: a great bell tower, lit, blue
/// banners down it. Visual only.
fn bell_tower(map: &mut Map, p: Vec2, yaw: f32) {
    let rig = Rig {
        pos: Vec3::new(p.x, 0.0, p.y),
        rot: Quat::from_rotation_y(yaw),
    };
    let (r, h) = (11.0, 84.0);
    put(
        map,
        &rig,
        Vec3::new(0.0, h * 0.5, 0.0),
        Vec3::new(r, h * 0.5, r),
        IVORY * 0.72,
        Material::Plaster,
        false,
    );
    for (x, z) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        put(
            map,
            &rig,
            Vec3::new(x * r, h * 0.5 + 4.0, z * r),
            Vec3::new(2.0, h * 0.5 + 4.0, 2.0),
            IVORY * 0.68,
            Material::Plaster,
            false,
        );
        spire(map, &rig, Vec3::new(x * r, h + 8.0, z * r), 1.9, 12.0);
    }
    put(
        map,
        &rig,
        Vec3::new(0.0, h + 0.6, 0.0),
        Vec3::new(r + 0.8, 0.6, r + 0.8),
        GILT * 0.9,
        Material::Metal,
        false,
    );
    spire(map, &rig, Vec3::new(0.0, h + 1.2, 0.0), r * 0.85, 46.0);
    for s in [-1.0f32, 1.0] {
        window(
            map,
            &rig,
            Vec3::new(s * (r + 0.05), h - 20.0, 0.0),
            true,
            (2.6, 13.0),
            true,
        );
        for z in [-6.0f32, 6.0] {
            put(
                map,
                &rig,
                Vec3::new(s * (r + 0.2), h * 0.45, z),
                Vec3::new(0.1, 22.0, 1.8),
                CLOTH * 0.8,
                Material::Cloth,
                false,
            );
        }
    }
}
