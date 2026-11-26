//! The Ithessar's style: near-black worked glass and squared dark stone,
//! plasma in its slits. They shape plasma with crystal, and build the same
//! way: masses that twist as they rise, curved walls, blades leaning in to
//! hold something. Nothing's built for tracks or feet: no stairs, floors
//! that float, openings at any height.
//!
//! - great buildings (`great`): the Sun, the Lens, the Weir.
//! - lots: shard houses; crescents, a curved wall round a court tanks drive
//!   into; houses floating on crystal legs; heaps of broken stone.
//! - medium structures (`civics`): kiln courts, lens halls, terraces.
//! - their power's the plasma river (`channel`), and every lit slit's on it.

mod civics;
mod great;

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

use glam::{Quat, Vec2, Vec3};

use super::shield::{angle, apart, dir, precinct_spots};
use super::skeleton::*;
use super::{Centerpiece, Style, civic};
use crate::map::landmarks::Rig;
use crate::map::ruins::Drift;
use crate::map::{Ambient, Crystal, GlowPath, Map, OBox};
use crate::render::material::Material;
use crate::rng::Rng;
use crate::tank::ithessar::PLASMA;

const GROUND: Vec3 = Vec3::new(0.27, 0.26, 0.28);
const STONE: Vec3 = Vec3::new(0.2, 0.21, 0.25);
const GLASS: Vec3 = Vec3::new(0.03, 0.06, 0.13);
/// Their walls: slate, basalt, a violet stone and worked glass.
const WALLS: [Vec3; 4] = [
    Vec3::new(0.22, 0.23, 0.27),
    Vec3::new(0.15, 0.16, 0.2),
    Vec3::new(0.25, 0.21, 0.28),
    Vec3::new(0.3, 0.3, 0.33),
];
const C: Material = Material::Concrete;
const V: Material = Material::Vhaal;
const G: Material = Material::Glow;

pub(super) struct Ithessar;

impl Style for Ithessar {
    fn ground(&self) -> Vec3 {
        GROUND
    }

    fn road(&self) -> Vec3 {
        Vec3::new(0.42, 0.42, 0.5)
    }

    fn stone(&self) -> civic::Stone {
        civic::Stone {
            wall: STONE,
            mat: C,
            trim: GLASS * 2.0,
            roof: GLASS * 1.6,
            land: GROUND,
        }
    }

    fn civic(&self, map: &mut Map, lot: &Lot, kind: CivicKind, rng: &mut Rng) {
        civics::build(map, rng, lot, kind, self.stone());
    }

    fn hold_name(&self, piece: Centerpiece, kind: LandmarkKind) -> &'static str {
        match piece {
            Centerpiece::Monument => match kind {
                LandmarkKind::Great(1) => "LENS",
                LandmarkKind::Great(2) => "WEIR",
                _ => "SUN",
            },
            Centerpiece::ShieldGenerator(..) => "GENERATOR",
        }
    }

    fn landmark(&self, map: &mut Map, sk: &Skeleton, lm: &Landmark, rng: &mut Rng) -> Vec<Drift> {
        let base = sk.ground(lm.center);
        match lm.kind {
            LandmarkKind::Great(1) => great::lens(map, rng, lm, base),
            LandmarkKind::Great(2) => great::weir(map, rng, lm, base),
            _ => great::sun(map, rng, lm, base),
        }
        for a in precinct_spots(lm, 10).into_iter().flatten() {
            obelisk(map, rng, lm.center + dir(a) * lm.precinct, base, 5.0);
        }
        map.generator_rubble(rng, lm, base, STONE * 1.2)
    }

    fn platform(&self, map: &mut Map, _sk: &Skeleton, p: &Platform, _rng: &mut Rng) {
        map.paving(p, &[], STONE * 1.7);
    }

    fn street(&self, map: &mut Map, sk: &Skeleton, s: &Street, _rng: &mut Rng) {
        if s.rank == Rank::Avenue {
            feed(map, sk, s);
        }
    }

    fn span(&self, map: &mut Map, sk: &Skeleton, s: &Span, rng: &mut Rng) {
        gate(map, rng, s, sk.ground(s.at));
    }

    fn lot(&self, map: &mut Map, sk: &Skeleton, lot: &Lot, rng: &mut Rng) -> Vec<Drift> {
        let f = &lot.footprint;
        match lot.kind {
            LotKind::Big if f.half.min_element() > 5.0 && rng.chance(0.16) => {
                blades(map, rng, f);
            }
            LotKind::Big if f.half.min_element() > 5.0 && rng.chance(0.4) => {
                crescent(map, rng, lot)
            }
            LotKind::Big => {
                let h = rng.range(9.0, 17.0);
                shards(map, rng, f, h, true);
            }
            LotKind::Small => {
                let h = rng.range(4.0, 6.5);
                shards(map, rng, f, h, true);
            }
            LotKind::Raised => floating(map, rng, f),
            LotKind::Rubble => {
                map.rubble_lot(rng, f, STONE * 1.2);
                if rng.chance(0.5) {
                    fallen_blade(map, rng, f);
                }
            }
            LotKind::Distant => {
                let h = rng.range(10.0, 22.0) + sk.out(f.center).max(0.0) * rng.range(0.02, 0.1);
                tower(map, rng, f, h);
            }
            _ => {}
        }
        Vec::new()
    }

    fn base(&self, map: &mut Map, sk: &Skeleton, side: usize, b: &Base, rng: &mut Rng) {
        map.staging_court(rng, side, b, sk.center(), STONE * 1.7);
    }

    fn edge(&self, map: &mut Map, _sk: &Skeleton, e: &Edge, rng: &mut Rng) {
        match e.kind {
            EdgeKind::Barricade => map.street_barricade(rng, e, STONE * 1.3),
            EdgeKind::Blockade => {
                map.rubble_bank(rng, e.start, e.along, e.inward, (e.from, e.to), STONE * 1.3)
            }
        }
    }

    fn deco(&self, map: &mut Map, sk: &Skeleton, d: &Deco, rng: &mut Rng) {
        let g = sk.ground(d.at);
        match d.kind {
            DecoKind::Monument => obelisk(map, rng, d.at, g, 7.0),
            DecoKind::Mast => lamp(map, d.at, g),
            DecoKind::Fountain => pool(map, rng, d.at, g),
            DecoKind::Cover => cover(map, rng, d.at, d.face, g),
        }
    }

    fn beyond(&self, map: &mut Map, sk: &Skeleton, rng: &mut Rng) {
        map.skyline_ground(sk, GROUND);
        map.skyline_lots(rng, sk, |map, rng, lot, h, _, _| {
            tower(map, rng, lot, 12.0 + h * 0.8)
        });
    }

    fn finish(&self, map: &mut Map) {
        for o in &mut map.oboxes {
            if o.solid && o.tilt == 0.0 && o.roll == 0.0 && o.chip == 0.0 && o.side_mat == C {
                o.chip = 0.12;
            }
        }
    }
}

/// A box standing on `o`'s ground: at `at` in its frame, turned `turn` more.
#[allow(clippy::too_many_arguments)]
fn put(
    map: &mut Map,
    o: &OBox,
    at: Vec2,
    half: Vec2,
    turn: f32,
    y: (f32, f32),
    color: Vec3,
    mat: Material,
    solid: bool,
) {
    let (ax, az) = o.axes();
    map.oboxes.push(OBox {
        center: o.center + ax * at.x + az * at.y,
        yaw: o.yaw + turn,
        half,
        y0: y.0,
        y1: y.1,
        color,
        side_mat: mat,
        top_mat: mat,
        solid,
        ..OBox::default()
    });
}

fn crystal(map: &mut Map, rng: &mut Rng, base: Vec3, axis: Vec3, r: f32, lit: bool, solid: bool) {
    map.crystals.push(Crystal {
        base,
        axis,
        radius: r,
        sides: 5 + (rng.next_u64() % 3) as u32,
        seed: rng.next_u64() as u32,
        color: GLASS * rng.range(0.85, 1.3),
        lit,
        solid,
        raw: false,
    });
}

/// A mass twisting as it rises: tiers, each a little smaller and turned a
/// little more, a slit of plasma up its faces.
#[allow(clippy::too_many_arguments)]
fn twist(
    map: &mut Map,
    rng: &mut Rng,
    o: &OBox,
    at: Vec2,
    half: Vec2,
    h: f32,
    color: Vec3,
    solid: bool,
) {
    let turn = rng.range(0.05, 0.13) * if rng.chance(0.5) { 1.0 } else { -1.0 };
    let n = 3;
    for k in 0..n {
        let u = k as f32 / n as f32;
        let hf = half * (1.0 - 0.2 * u);
        let (y0, y1) = (
            h * u - if k == 0 { 0.5 } else { 0.02 * k as f32 },
            h * (u + 1.0 / n as f32),
        );
        put(
            map,
            o,
            at,
            hf,
            turn * k as f32,
            (y0, y1),
            color * (1.0 - 0.07 * k as f32),
            C,
            solid,
        );
        map.oboxes.last_mut().unwrap().chip = 0.01;
        // Its slits, on two faces.
        if rng.chance(0.65) {
            let w = rng.range(0.1, 0.18);
            for e in [-1.0f32, 1.0] {
                let off = Vec2::new(rng.range(-0.5, 0.5) * hf.x, e * (hf.y + 0.01));
                let (s, c) = (turn * k as f32).sin_cos();
                let off = Vec2::new(off.x * c + off.y * s, -off.x * s + off.y * c);
                put(
                    map,
                    o,
                    at + off,
                    Vec2::new(w, 0.05),
                    turn * k as f32,
                    (y0.max(0.0) + (y1 - y0) * 0.18, y1 - (y1 - y0) * 0.14),
                    PLASMA * 0.85,
                    G,
                    false,
                );
            }
        }
    }
    // A cap of worked glass.
    let hf = half * 0.84;
    put(
        map,
        o,
        at,
        hf,
        turn * n as f32,
        (h - 0.03, h + 0.5),
        GLASS * 1.5,
        V,
        false,
    );
}

/// A house: one to three masses, the tallest with a blade of crystal on it.
fn shards(map: &mut Map, rng: &mut Rng, o: &OBox, h: f32, solid: bool) {
    let color = WALLS[rng.index(WALLS.len())] * rng.range(0.92, 1.08);
    let half = o.half * 0.9;
    let split = if half.max_element() > 5.0 {
        rng.int(1, 3)
    } else {
        1
    };
    let long = half.x >= half.y;
    for k in 0..split {
        let w = 1.0 / split as f32;
        let u = (k as f32 + 0.5) * w * 2.0 - 1.0;
        let (at, hf) = if long {
            (
                Vec2::new(u * half.x, rng.range(-0.12, 0.12) * half.y),
                Vec2::new(half.x * w * 0.97, half.y * 0.86),
            )
        } else {
            (
                Vec2::new(rng.range(-0.12, 0.12) * half.x, u * half.y),
                Vec2::new(half.x * 0.86, half.y * w * 0.97),
            )
        };
        // (Turning, it keeps to its ground.)
        let hh = h * if k == 0 { 1.0 } else { rng.range(0.5, 0.85) };
        twist(map, rng, o, at, hf * 0.93, hh, color, solid);
        if k == 0 && rng.chance(0.7) {
            let (ax, az) = o.axes();
            let p = o.center + ax * at.x + az * at.y;
            let lean = Vec3::new(rng.range(-0.25, 0.25), 1.0, rng.range(-0.25, 0.25));
            let (tall, r) = (
                hh * rng.range(0.35, 0.75),
                hf.min_element() * rng.range(0.26, 0.42),
            );
            crystal(
                map,
                rng,
                Vec3::new(p.x, hh + 0.2, p.y),
                lean.normalize() * tall,
                r,
                true,
                false,
            );
        }
    }
}

/// Not a house: a stand of great blades grown on a stone foot, what their
/// glass is cut from.
fn blades(map: &mut Map, rng: &mut Rng, o: &OBox) {
    let r = o.half.min_element();
    put(
        map,
        o,
        Vec2::ZERO,
        o.half * 0.85,
        0.1,
        (-0.5, 1.6),
        STONE * 1.1,
        C,
        true,
    );
    for k in 0..5 {
        let d = dir(k as f32 * TAU / 5.0 + rng.range(-0.3, 0.3));
        let out = if k == 0 { 0.0 } else { r * 0.45 };
        let lean = if k == 0 { 0.0 } else { rng.range(0.12, 0.3) };
        let h = if k == 0 {
            rng.range(14.0, 20.0)
        } else {
            rng.range(7.0, 13.0)
        };
        let wide = r * rng.range(0.16, 0.26);
        let axis = Vec3::new(d.x * lean, 1.0, d.y * lean).normalize() * h;
        let foot = Vec3::new(o.center.x + d.x * out, 0.8, o.center.y + d.y * out);
        crystal(map, rng, foot, axis, wide, k < 2, true);
    }
}

/// A curved wall round a court open to the street: cover from behind, a way
/// in from the front.
fn crescent(map: &mut Map, rng: &mut Rng, lot: &Lot) {
    let o = &lot.footprint;
    let r = o.half.min_element() - 0.7;
    let color = WALLS[rng.index(WALLS.len())];
    let front = if lot.front.0 == 0 {
        Vec2::new(lot.front.1, 0.0)
    } else {
        Vec2::new(0.0, lot.front.1)
    };
    let fa = angle(front);
    let n = 14;
    let h = rng.range(5.0, 8.5);
    for k in 0..n {
        let a = (k as f32 + 0.5) / n as f32 * TAU;
        let off = apart(a, fa);
        if off < 0.95 {
            continue;
        }
        let d = dir(a);
        let broken = rng.chance(0.1);
        let hh = if broken {
            rng.range(1.0, 2.5)
        } else {
            h * (0.55 + 0.45 * off / PI)
        };
        // (Turned with its place: local x along the curve.)
        let turn = -a + FRAC_PI_2;
        put(
            map,
            o,
            d * (r - 0.8),
            Vec2::new(r * PI / n as f32 * 1.04, 0.75),
            turn,
            (-0.5, hh),
            color * (1.0 - 0.02 * (k % 3) as f32),
            C,
            true,
        );
        map.oboxes.last_mut().unwrap().broken = broken;
        if !broken {
            put(
                map,
                o,
                d * (r - 0.8),
                Vec2::new(r * PI / n as f32 * 1.08, 0.92),
                turn,
                (hh - 0.02, hh + 0.35 + 0.004 * k as f32),
                GLASS * 1.6,
                V,
                false,
            );
            if k % 2 == 0 {
                put(
                    map,
                    o,
                    d * (r - 1.56),
                    Vec2::new(0.12, 0.05),
                    turn,
                    (0.8, hh - 0.8),
                    PLASMA * 0.8,
                    G,
                    false,
                );
            }
        }
    }
    // A blade leaning over the court.
    let d = dir(fa + PI);
    let (ax, az) = o.axes();
    let w = ax * d.x + az * d.y;
    let p = o.center + w * (r - 2.4);
    let lean = Vec3::new(-w.x * 0.5, 1.0, -w.y * 0.5).normalize();
    crystal(
        map,
        rng,
        Vec3::new(p.x, -0.5, p.y),
        lean * (h + 4.0),
        0.8,
        true,
        true,
    );
}

/// A house floating on crystal legs round a column of plasma: tanks drive
/// under it.
fn floating(map: &mut Map, rng: &mut Rng, o: &OBox) {
    let (w, l) = (o.half.x, o.half.y);
    let lift = 6.6;
    let color = WALLS[rng.index(WALLS.len())];
    put(
        map,
        o,
        Vec2::ZERO,
        Vec2::splat(w.min(l) * 0.26),
        FRAC_PI_4,
        (-0.4, 1.3),
        STONE,
        C,
        true,
    );
    put(
        map,
        o,
        Vec2::ZERO,
        Vec2::splat(w.min(l) * 0.1),
        FRAC_PI_4,
        (1.25, lift),
        PLASMA,
        G,
        true,
    );
    for (x, z) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let at = Vec2::new(x * (w - 1.3), z * (l - 1.3));
        put(
            map,
            o,
            at,
            Vec2::splat(0.6),
            0.3,
            (-0.4, lift + 0.1),
            GLASS * 1.3,
            V,
            true,
        );
        map.oboxes.last_mut().unwrap().chip = 0.01;
    }
    let h = rng.range(5.0, 9.0);
    for (k, (grow, y0, y1)) in [(1.0, 0.0, 0.5), (0.9, 0.45, 0.8), (0.74, 0.78, 1.0)]
        .into_iter()
        .enumerate()
    {
        put(
            map,
            o,
            Vec2::ZERO,
            o.half * grow * 0.97,
            0.05 * k as f32,
            (lift + h * y0, lift + h * y1),
            color * (1.0 - 0.06 * k as f32),
            C,
            true,
        );
        map.oboxes.last_mut().unwrap().chip = 0.01;
    }
    // A belt of light under its edge.
    put(
        map,
        o,
        Vec2::ZERO,
        o.half * 0.97 + 0.04,
        0.0,
        (lift + 0.5, lift + 0.72),
        PLASMA * 0.7,
        G,
        false,
    );
}

/// A building far off, only seen: three twisting tiers and a lit seam.
fn tower(map: &mut Map, rng: &mut Rng, o: &OBox, h: f32) {
    let color = WALLS[rng.index(WALLS.len())] * rng.range(0.75, 0.95);
    let turn = rng.range(-0.14, 0.14);
    if rng.chance(0.3) {
        // A great blade, on a stone foot.
        put(
            map,
            o,
            Vec2::ZERO,
            o.half * 0.9,
            0.0,
            (-0.6, h * 0.22),
            color,
            C,
            false,
        );
        let lean = Vec3::new(rng.range(-0.16, 0.16), 1.0, rng.range(-0.16, 0.16));
        let foot = Vec3::new(o.center.x, h * 0.18, o.center.y);
        let r = o.half.min_element() * rng.range(0.4, 0.6);
        crystal(map, rng, foot, lean.normalize() * h * 1.1, r, true, false);
        return;
    }
    for (k, (grow, y0, y1)) in [(0.95, 0.0, 0.45), (0.78, 0.44, 0.78), (0.55, 0.77, 1.0)]
        .into_iter()
        .enumerate()
    {
        put(
            map,
            o,
            Vec2::ZERO,
            o.half * grow,
            turn * k as f32,
            (h * y0 - 0.6, h * y1),
            color,
            C,
            false,
        );
        if rng.chance(0.6) {
            let side = rng.index(2);
            let hf = o.half * grow;
            let e = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let (at, half) = if side == 0 {
                (Vec2::new(e * (hf.x + 0.02), 0.0), Vec2::new(0.06, 0.22))
            } else {
                (Vec2::new(0.0, e * (hf.y + 0.02)), Vec2::new(0.22, 0.06))
            };
            let (s, c) = (turn * k as f32).sin_cos();
            let at = Vec2::new(at.x * c + at.y * s, -at.x * s + at.y * c);
            put(
                map,
                o,
                at,
                half,
                turn * k as f32,
                (h * y0 + 1.0, h * y1 - 0.8),
                PLASMA * 0.8,
                G,
                false,
            );
        }
    }
}

/// A blade of worked glass that came down on `o`.
fn fallen_blade(map: &mut Map, rng: &mut Rng, o: &OBox) {
    let a = rng.range(0.0, TAU);
    let l = o.half.min_element() * 0.8;
    let d = dir(a);
    let p = o.center - d * l;
    crystal(
        map,
        rng,
        Vec3::new(p.x, 0.5, p.y),
        Vec3::new(d.x, 0.1, d.y) * l * 2.0,
        1.3,
        false,
        true,
    );
}

/// Plasma fed along an avenue, in a channel in its paving.
fn feed(map: &mut Map, sk: &Skeleton, s: &Street) {
    let (start, end) = sk.off_plaza_span(s, 2.0);
    if end - start < 10.0 {
        return;
    }
    let d = s.dir();
    let side = Vec2::new(-d.y, d.x);
    let lift = ((s.from.x * 3.7 + s.from.y * 5.3) as i32).rem_euclid(9) as f32 * 0.012;
    for k in [-1.0f32, 1.0] {
        let n = ((end - start - 4.0) / 2.0).max(1.0) as usize;
        let points: Vec<Vec3> = (0..=n)
            .map(|i| {
                let t = start + 2.0 + (end - start - 4.0) * i as f32 / n as f32;
                let p = s.from + d * t + side * k * (s.half - 2.2);
                Vec3::new(p.x, sk.ground(p) + 0.1 + lift, p.y)
            })
            .collect();
        map.glow_paths.push(GlowPath {
            widths: vec![0.2; points.len()],
            points,
            color: PLASMA * 0.8,
        });
    }
}

/// A gate: two blades leaning together over the street, a thread of plasma
/// between their tips. Ruined, one's snapped.
fn gate(map: &mut Map, rng: &mut Rng, s: &Span, g: f32) {
    let yaw = (-s.along.y).atan2(s.along.x);
    let rig = Rig {
        pos: Vec3::new(s.at.x, g, s.at.y),
        rot: Quat::from_rotation_y(yaw - FRAC_PI_2),
    };
    let x = s.opening + 2.2;
    for k in [-1.0f32, 1.0] {
        let broken = s.ruined && k > 0.0;
        for (half, y0, y1, c, m) in [
            (Vec3::new(2.0, 0.0, 2.2), -0.4, 3.4, STONE * 1.1, C),
            (Vec3::new(1.6, 0.0, 1.8), 3.35, 4.2, GLASS * 1.6, V),
        ] {
            rig.part(
                map,
                Vec3::new(k * x, (y0 + y1) * 0.5, 0.0),
                Vec3::new(half.x, (y1 - y0) * 0.5, half.z),
                Quat::IDENTITY,
                c,
                m,
                true,
                0.01,
            );
        }
        let foot = rig.point(Vec3::new(k * x, 3.0, 0.0));
        let tip = rig.point(Vec3::new(k * 1.1, 17.0, 0.0));
        let axis = (tip - foot) * if broken { 0.35 } else { 1.0 };
        crystal(map, rng, foot, axis, 1.25, true, false);
    }
    if !s.ruined {
        let (a, b) = (
            rig.point(Vec3::new(-1.0, 16.4, 0.0)),
            rig.point(Vec3::new(1.0, 16.4, 0.0)),
        );
        map.segment(a, b, 0.16, PLASMA, G, false);
        map.sound(Ambient::Core, (a + b) * 0.5);
    }
}

/// A crystal standing on a plinth, `h` tall.
fn obelisk(map: &mut Map, rng: &mut Rng, at: Vec2, g: f32, h: f32) {
    let o = OBox {
        center: at,
        ..OBox::default()
    };
    put(
        map,
        &o,
        Vec2::ZERO,
        Vec2::splat(1.2),
        0.4,
        (g - 0.4, g + 1.3),
        STONE * 1.1,
        C,
        true,
    );
    let lean = Vec3::new(rng.range(-0.1, 0.1), 1.0, rng.range(-0.1, 0.1)).normalize();
    crystal(
        map,
        rng,
        Vec3::new(at.x, g + 0.9, at.y),
        lean * h,
        0.75,
        true,
        false,
    );
}

/// A slim post, plasma caged at its top.
fn lamp(map: &mut Map, at: Vec2, g: f32) {
    let o = OBox {
        center: at,
        ..OBox::default()
    };
    put(
        map,
        &o,
        Vec2::ZERO,
        Vec2::splat(0.22),
        0.0,
        (g - 0.3, g + 7.0),
        GLASS * 1.4,
        V,
        true,
    );
    put(
        map,
        &o,
        Vec2::ZERO,
        Vec2::splat(0.3),
        FRAC_PI_4,
        (g + 6.95, g + 7.7),
        PLASMA,
        G,
        false,
    );
    for k in 0..4 {
        let d = dir(k as f32 * FRAC_PI_2);
        map.segment(
            Vec3::new(at.x + d.x * 0.5, g + 6.4, at.y + d.y * 0.5),
            Vec3::new(at.x + d.x * 0.1, g + 8.7, at.y + d.y * 0.1),
            0.07,
            GLASS * 1.4,
            V,
            false,
        );
    }
}

/// A round basin of plasma, a low stone lip and crystals keeping it.
fn pool(map: &mut Map, rng: &mut Rng, c: Vec2, g: f32) {
    let (r, n) = (5.6, 10);
    for k in 0..n {
        let a = (k as f32 + 0.5) / n as f32 * TAU;
        map.block(
            c + dir(a) * r,
            Vec2::new(r * (PI / n as f32).tan() + 0.15, 0.5),
            -a + FRAC_PI_2,
            (g - 0.2, g + 1.15),
            STONE * 1.1,
            C,
            true,
        )
        .chip = 0.01;
    }
    map.disc(
        Vec3::new(c.x, g + 0.35, c.y),
        Vec3::Y * 0.05,
        r - 0.6,
        PLASMA * 0.6,
        G,
    );
    for k in 0..3 {
        let d = dir(k as f32 * TAU / 3.0 + rng.range(-0.2, 0.2));
        let lean = Vec3::new(-d.x * 0.45, 1.0, -d.y * 0.45).normalize();
        let h = rng.range(3.5, 5.5);
        crystal(
            map,
            rng,
            Vec3::new(c.x + d.x * 3.6, g - 0.4, c.y + d.y * 3.6),
            lean * h,
            0.6,
            true,
            true,
        );
    }
    map.sound(Ambient::Crystal, Vec3::new(c.x, g + 1.5, c.y));
}

/// Low cover: a curved stub of wall, squared blocks, or a shard.
fn cover(map: &mut Map, rng: &mut Rng, p: Vec2, face: Vec2, g: f32) {
    let yaw = angle(face);
    let ax = Vec2::new(yaw.cos(), -yaw.sin());
    let az = Vec2::new(-ax.y, ax.x);
    match rng.f32() {
        x if x < 0.5 => {
            for k in -2i32..=2 {
                let u = k as f32;
                let q = p + ax * u * 2.1 + az * u * u * 0.35;
                let turn = yaw - u * 0.32;
                let h = rng.range(1.5, 2.6);
                map.block(
                    q,
                    Vec2::new(1.15, 0.5),
                    turn,
                    (g - 0.3, g + h),
                    WALLS[0],
                    C,
                    true,
                );
            }
        }
        x if x < 0.85 => {
            for k in -1..=1 {
                let h = rng.range(1.0, 2.2);
                map.block(
                    p + ax * k as f32 * 2.6,
                    Vec2::new(1.0, 0.9),
                    yaw + rng.range(-0.2, 0.2),
                    (g - 0.2, g + h),
                    STONE * rng.range(0.95, 1.2),
                    C,
                    true,
                );
            }
        }
        _ => {
            let q = p - ax * 2.6;
            crystal(
                map,
                rng,
                Vec3::new(q.x, g + 0.3, q.y),
                Vec3::new(ax.x, 0.14, ax.y) * 5.2,
                0.95,
                false,
                true,
            );
        }
    }
}
