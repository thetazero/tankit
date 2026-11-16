//! The Capital: an Imperium city the war has reached, half ruined, still
//! kept by ritual. A free-for-all: all four factions, one corner each.
//!
//! 340 m square (+x east, +z north), symmetric in all four directions:
//!
//! - middle: a raised plaza, and on it the rotunda, a domed basilica in the
//!   Roman way (a drum of concrete, an oculus in the dome) with the dome
//!   and drum fallen in on one side. Inside, the emblem on its dais. A ring
//!   of columns stands around it, some fallen.
//! - four processional avenues run from the plaza to the edges, each
//!   through a triumphal arch, lined with statues on plinths and crimson
//!   banners: the long sight lines.
//! - between them, districts of ordinary concrete blocks (many shelled) and
//!   a few brutalist slabs on pillars you can drive under: close quarters.
//! - a staging court in each corner, one per side.
//!
//! The ruin is Roman scale in raw concrete: mass-made monuments, ritual for
//! its own sake (see docs/setting.md).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{Mat3, Quat, Vec2, Vec3};

use super::landmarks::Rig;
use super::ruins::{self, Drift, Style};
use super::{Map, OBox, Terrain};
use crate::faction::Faction;
use crate::render::material::Material;
use crate::rng::Rng;

const SIZE: f32 = 340.0;
const MID: f32 = SIZE * 0.5;
const CENTER: Vec2 = Vec2::new(MID, MID);
/// The plaza: a rounded square raised `PLAZA_H`, sloping down over
/// `PLAZA_RAMP` at its edge (drivable all round).
const PLAZA_H: f32 = 2.0;
const PLAZA_HALF: f32 = 62.0;
const PLAZA_ROUND: f32 = 20.0;
const PLAZA_RAMP: f32 = 14.0;
/// Half-width of the four avenues.
const AVENUE_HALF: f32 = 14.0;
/// The rotunda's drum: outer radius, wall thickness, height.
const ROTUNDA_R: f32 = 24.0;
const DRUM_T: f32 = 2.6;
const DRUM_H: f32 = 16.0;
/// The ring of columns round the rotunda.
const COLONNADE_R: f32 = 44.0;
/// How far out along each avenue its arch stands.
const ARCH_AT: f32 = 96.0;
/// Each corner's staging court.
const COURT: f32 = 46.0;
/// Clearance a spawn needs from anything solid.
const TANK_CLEAR: f32 = 3.5;

const GROUND: Vec3 = Vec3::new(0.32, 0.32, 0.33);
const CONCRETE: Vec3 = Vec3::new(0.54, 0.53, 0.51);
const CONCRETE_DARK: Vec3 = Vec3::new(0.3, 0.3, 0.3);
const STONE: Vec3 = Vec3::new(0.86, 0.83, 0.76);
const CRIMSON: Vec3 = Vec3::new(0.46, 0.05, 0.06);
const GOLD: Vec3 = Vec3::new(0.86, 0.66, 0.28);

const CONCRETES: [Vec3; 5] = [
    Vec3::new(0.56, 0.55, 0.52),
    Vec3::new(0.48, 0.48, 0.47),
    Vec3::new(0.6, 0.57, 0.52),
    Vec3::new(0.44, 0.44, 0.45),
    Vec3::new(0.52, 0.49, 0.45),
];
/// The ordinary blocks: cast concrete in greys.
const CAST: Style = Style {
    colors: &CONCRETES,
    wall: Material::Concrete,
};

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Signed distance to the plaza's rounded-square edge (negative inside).
fn plaza_sdf(p: Vec2) -> f32 {
    let q = (p - CENTER).abs() - Vec2::splat(PLAZA_HALF - PLAZA_ROUND);
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - PLAZA_ROUND
}

/// Ground height before rubble: flat, with the plaza raised in the middle.
pub fn ground(x: f32, z: f32) -> f32 {
    PLAZA_H * (1.0 - smoothstep(0.0, PLAZA_RAMP, plaza_sdf(Vec2::new(x, z))))
}

/// The four directions out from the middle: east, north, west, south.
const AXES: [Vec2; 4] = [
    Vec2::new(1.0, 0.0),
    Vec2::new(0.0, 1.0),
    Vec2::new(-1.0, 0.0),
    Vec2::new(0.0, -1.0),
];

/// Corner courts, one per side in `Faction::ALL` order: southwest,
/// southeast, northeast, northwest. Each is (corner, direction inward).
fn courts() -> [(Vec2, Vec2); 4] {
    [
        (Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)),
        (Vec2::new(SIZE, 0.0), Vec2::new(-1.0, 1.0)),
        (Vec2::new(SIZE, SIZE), Vec2::new(-1.0, -1.0)),
        (Vec2::new(0.0, SIZE), Vec2::new(1.0, -1.0)),
    ]
}

/// Ground kept open for streets and set pieces: the avenues, the plaza and
/// its slope, the corner courts and a margin inside the edge.
fn reserved(p: Vec2) -> bool {
    let d = p - CENTER;
    let avenue = d.x.abs() < AVENUE_HALF + 7.0 || d.y.abs() < AVENUE_HALF + 7.0;
    let plaza = plaza_sdf(p) < PLAZA_RAMP + 10.0;
    let court = courts().iter().any(|&(c, _)| {
        let q = (p - c).abs();
        q.x < COURT + 8.0 && q.y < COURT + 8.0
    });
    let edge = p.x < 6.0 || p.y < 6.0 || p.x > SIZE - 6.0 || p.y > SIZE - 6.0;
    avenue || plaza || court || edge
}

/// A rotation whose local x, y, z axes are `x`, `y` and their cross.
fn frame(x: Vec3, y: Vec3) -> Quat {
    let z = x.cross(y);
    Quat::from_mat3(&Mat3::from_cols(x, y, z))
}

impl Map {
    pub fn capital() -> Map {
        let factions = Faction::ALL.to_vec();
        let mut map = Map {
            size: SIZE,
            solids: Vec::new(),
            boxes: Vec::new(),
            trees: Vec::new(),
            wedges: Vec::new(),
            oboxes: Vec::new(),
            terrain: None,
            scenery_terrain: None,
            roads: Vec::new(),
            open_spots: Vec::new(),
            spawn_yaw: None,
            // A cold, hazy morning: pale sun low in the east, ash in the air.
            atmosphere: crate::render::Atmosphere {
                sun_dir: Vec3::new(0.66, 0.36, -0.4),
                sun_color: Vec3::new(2.15, 1.95, 1.7),
                sky_zenith: Vec3::new(0.28, 0.34, 0.44),
                sky_horizon: Vec3::new(0.6, 0.6, 0.58),
                ground_color: Vec3::new(0.26, 0.25, 0.24),
                fog_density: 0.0012,
                wind: Vec2::new(2.0, 1.2),
                blowing_sand: 0.25,
                dust: Vec3::new(0.62, 0.6, 0.57),
            },
            bases: Map::no_bases(&factions),
            factions,
            repair_spots: Vec::new(),
            hold_site: Some(CENTER),
            cores: Vec::new(),
            crystals: Vec::new(),
            glow_paths: Vec::new(),
        };
        let mut rng = Rng::new(0xC4_917A);
        let mut drifts = map.districts(&mut rng);
        drifts.extend(map.rotunda(&mut rng));
        let mut terrain = Terrain::from_fn(Vec2::ZERO, 2.0, 171, 171, |x, z| {
            let p = Vec2::new(x, z);
            ground(x, z) + drifts.iter().map(|d| d.height_at(p)).fold(0.0, f32::max)
        });
        terrain.color = GROUND;
        terrain.material = Material::Concrete;
        map.terrain = Some(terrain);
        map.plaza_paving();
        map.plaza_ring_cover(&mut rng);
        map.colonnade(&mut rng);
        for (k, &dir) in AXES.iter().enumerate() {
            map.avenue(&mut rng, dir);
            map.arch(&mut rng, dir, k == 3);
        }
        map.staging_courts(&mut rng);
        map.capital_spawns();
        map.perimeter(false);
        map.skyline(&mut rng);
        for o in &mut map.oboxes {
            let masonry =
                [Material::Plaster, Material::Concrete, Material::Rubble].contains(&o.side_mat);
            if o.solid && o.tilt == 0.0 && o.chip == 0.0 && masonry {
                o.chip = 0.25;
            }
        }
        map.drop_tiny_hitboxes();
        map.separate_coplanar_faces();
        map
    }

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
    fn banner(&mut self, top: Vec3, out: Vec2, w: f32, len: f32) {
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
            CRIMSON,
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
        let d = top + n * 0.2 - Vec3::Y * w * 0.75;
        self.disc(d, n * 0.04, w * 0.28, GOLD, Material::Metal);
    }

    // -----------------------------------------------------------------------
    // The middle

    /// Large paving slabs over the plaza top (visual; the terrain carries you).
    fn plaza_paving(&mut self) {
        let step = 8.0;
        let n = (PLAZA_HALF * 2.0 / step) as i32;
        for i in 0..n {
            for j in 0..n {
                let c = CENTER - Vec2::splat(PLAZA_HALF)
                    + Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * step;
                // Only where the top is flat, and not under the rotunda.
                if plaza_sdf(c) > -step * 0.75 || c.distance(CENTER) < ROTUNDA_R + 2.0 {
                    continue;
                }
                let shade = 0.94 + 0.06 * (((i * 7 + j * 13) % 5) as f32 / 4.0);
                self.slab(
                    Vec3::new(c.x - step * 0.48, PLAZA_H - 0.1, c.y - step * 0.48),
                    Vec3::new(c.x + step * 0.48, PLAZA_H + 0.06, c.y + step * 0.48),
                    0.0,
                    STONE * shade,
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
    fn rotunda(&mut self, rng: &mut Rng) -> Vec<Drift> {
        let mut drifts = Vec::new();
        let base = PLAZA_H;
        let segs = 40;
        let collapsed = |a: f32| {
            let d = (a - PI * 0.25 + PI).rem_euclid(TAU) - PI;
            d.abs() < 0.75
        };
        let door = |a: f32| {
            (0..4).any(|k| {
                let d = (a - k as f32 * FRAC_PI_2 + PI).rem_euclid(TAU) - PI;
                d.abs() < 0.16
            })
        };
        let r_mid = ROTUNDA_R - DRUM_T * 0.5;
        let seg_half = r_mid * PI / segs as f32 * 1.03;
        for k in 0..segs {
            let a = (k as f32 + 0.5) / segs as f32 * TAU;
            let dir = Vec2::new(a.cos(), a.sin());
            let c = CENTER + dir * r_mid;
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
                continue;
            }
            let (l, u) = wall(0.0, DRUM_H);
            self.slab(l, u, yaw, CONCRETE, Material::Concrete, true);
            // Buttress pilasters on every fourth pier.
            if k % 4 == 2 {
                let p = CENTER + dir * (ROTUNDA_R + 0.6);
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
            let p = CENTER + dir * (ROTUNDA_R + 0.2);
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
            let p = CENTER + dir * (ROTUNDA_R + 0.05);
            self.banner(Vec3::new(p.x, base + DRUM_H - 0.6, p.y), dir, 3.0, 11.0);
        }

        // The dome: rings of plates from the drum's top to the oculus, skipped
        // where it fell in (with a ragged edge).
        let dome_base = base + DRUM_H + 1.0;
        let rings = 7;
        let r = ROTUNDA_R - 0.6;
        for i in 0..rings {
            let p0 = i as f32 / rings as f32 * FRAC_PI_2 * 0.86;
            let p1 = (i + 1) as f32 / rings as f32 * FRAC_PI_2 * 0.86;
            let phi = (p0 + p1) * 0.5;
            let n = (segs as f32 * phi.cos()).round().max(10.0) as usize;
            for k in 0..n {
                let a = (k as f32 + 0.5) / n as f32 * TAU;
                let d = (a - PI * 0.25 + PI).rem_euclid(TAU) - PI;
                // Gone over the fallen drum and up past the crown, ragged.
                if d.abs() < 0.95 + 0.25 * rng.f32() + 0.12 * i as f32 {
                    continue;
                }
                let radial = Vec3::new(a.cos() * phi.cos(), phi.sin(), a.sin() * phi.cos());
                let tangent = Vec3::new(-a.sin(), 0.0, a.cos());
                let c = Vec3::new(CENTER.x, dome_base, CENTER.y) + radial * r;
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
            Vec3::new(CENTER.x, dome_base, CENTER.y)
                + Vec3::new(a.cos() * phi.cos(), phi.sin(), a.sin() * phi.cos()) * inner
        };
        for k in 0..20 {
            let a = (k as f32) / 20.0 * TAU;
            let d = (a - PI * 0.25 + PI).rem_euclid(TAU) - PI;
            if d.abs() < 1.05 {
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
                        ((u + v) * 0.5 - Vec3::new(CENTER.x, dome_base, CENTER.y)).normalize(),
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
                pos: Vec3::new(CENTER.x, dome_base, CENTER.y) + radial * (r + 0.3),
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
            let p = CENTER + Vec2::new(a.cos(), a.sin()) * 14.0;
            if collapsed(a) {
                // Fallen inward, lying across the floor.
                let along = Vec2::new(-a.sin(), a.cos()) * 0.4 - Vec2::new(a.cos(), a.sin());
                let along = along.normalize();
                let e = p + along * 12.0;
                self.pipe(
                    Vec3::new(p.x, base + 1.0, p.y),
                    Vec3::new(e.x, base + 1.0, e.y),
                    1.0,
                    STONE,
                    Material::Concrete,
                    true,
                );
                continue;
            }
            self.column(Vec3::new(p.x, base, p.y), 1.0, DRUM_H - 1.0, false);
        }
        self.pipe(
            Vec3::new(CENTER.x, base, CENTER.y),
            Vec3::new(CENTER.x, base + 1.2, CENTER.y),
            4.5,
            STONE,
            Material::Concrete,
            true,
        );
        for out in AXES {
            let at = CENTER + out * 1.2;
            self.standing_emblem(Vec3::new(at.x, base + 4.4, at.y), out, 2.0);
        }

        // Rubble where it fell: heaps banked against the broken wall, chunks
        // of dome scattered inside and out.
        let fall = Vec2::new(1.0, 1.0).normalize();
        for (side, sign) in [(-1.0f32, 1.0f32), (1.0, -1.0)] {
            let p = CENTER + fall * (ROTUNDA_R + side * 4.0);
            drifts.push(Drift {
                b: OBox {
                    center: p,
                    yaw: -PI * 0.25,
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
            let a = PI * 0.25 + rng.range(-0.7, 0.7);
            let d = rng.range(6.0, ROTUNDA_R + 14.0);
            let p = CENTER + Vec2::new(a.cos(), a.sin()) * d;
            let s = rng.range(0.8, 2.6);
            self.oboxes.push(OBox {
                center: p,
                yaw: rng.range(0.0, TAU),
                half: Vec2::new(s, s * rng.range(0.4, 0.9)),
                y0: base - 0.3,
                y1: base + s * rng.range(0.3, 0.7),
                tilt: rng.range(-0.4, 0.4),
                roll: rng.range(-0.3, 0.3),
                color: CONCRETE * rng.range(0.85, 1.0),
                side_mat: Material::Rubble,
                top_mat: Material::Rubble,
                solid: s > 1.6,
                chip: 0.6,
                ..OBox::default()
            });
        }
        drifts
    }

    /// The open ground round the plaza: stubs of broken wall, concrete
    /// blocks and fallen column drums for cover, kept off the avenues.
    fn plaza_ring_cover(&mut self, rng: &mut Rng) {
        let mut placed: Vec<Vec2> = Vec::new();
        let mut tries = 0;
        while placed.len() < 56 && tries < 3000 {
            tries += 1;
            let p = Vec2::new(rng.range(0.0, SIZE), rng.range(0.0, SIZE));
            let d = p - CENTER;
            let s = plaza_sdf(p);
            if !(PLAZA_RAMP + 3.0..PLAZA_RAMP + 24.0).contains(&s)
                || d.x.abs() < AVENUE_HALF + 3.0
                || d.y.abs() < AVENUE_HALF + 3.0
                || placed.iter().any(|q| q.distance(p) < 11.0)
            {
                continue;
            }
            placed.push(p);
            let yaw = (d.y).atan2(d.x)
                + if rng.f32() < 0.5 { 0.0 } else { FRAC_PI_2 }
                + rng.range(-0.2, 0.2);
            match rng.f32() {
                x if x < 0.5 => {
                    // A stub of wall, broken along the top.
                    let h = rng.range(1.6, 3.4);
                    self.oboxes.push(OBox {
                        center: p,
                        yaw,
                        half: Vec2::new(rng.range(3.0, 6.0), 0.45),
                        y0: -0.2,
                        y1: h,
                        color: CONCRETES[rng.index(CONCRETES.len())],
                        side_mat: Material::Concrete,
                        top_mat: Material::Concrete,
                        broken: true,
                        ..OBox::default()
                    });
                }
                x if x < 0.8 => {
                    // A row of cast barrier blocks, one knocked askew.
                    let (ax, _) = (Vec2::new(yaw.cos(), -yaw.sin()), ());
                    for k in -1..=1 {
                        let c = p + ax * k as f32 * 2.3;
                        let skew = if k == 1 { rng.range(0.2, 0.6) } else { 0.0 };
                        self.oboxes.push(OBox {
                            center: c,
                            yaw: yaw + skew,
                            half: Vec2::new(1.0, 0.7),
                            y0: 0.0,
                            y1: 1.3,
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
                    self.pipe(
                        Vec3::new(u.x, 1.0, u.y),
                        Vec3::new(v.x, 1.0, v.y),
                        1.05,
                        STONE * 0.95,
                        Material::Concrete,
                        true,
                    );
                }
            }
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
        self.pipe(
            at + Vec3::Y * 0.8,
            at + Vec3::Y * (h - 0.6),
            r,
            STONE,
            Material::Concrete,
            true,
        );
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
    fn colonnade(&mut self, rng: &mut Rng) {
        let n = 36;
        let h = 11.0;
        let mut standing = vec![false; n];
        for (k, up) in standing.iter_mut().enumerate() {
            let a = (k as f32 + 0.5) / n as f32 * TAU;
            // Open on the avenues.
            let gate = (0..4).any(|q| {
                let d = (a - q as f32 * FRAC_PI_2 + PI).rem_euclid(TAU) - PI;
                d.abs() < 0.2
            });
            if gate {
                continue;
            }
            let dir = Vec2::new(a.cos(), a.sin());
            let p = CENTER + dir * COLONNADE_R;
            let at = Vec3::new(p.x, PLAZA_H, p.y);
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
                        self.pipe(
                            Vec3::new(u.x, PLAZA_H + 0.9, u.y),
                            Vec3::new(v.x, PLAZA_H + 0.9, v.y),
                            0.9,
                            STONE,
                            Material::Concrete,
                            true,
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
        let r_beam = COLONNADE_R;
        for k in 0..n {
            let k1 = (k + 1) % n;
            if !(standing[k] && standing[k1]) {
                continue;
            }
            let a = (k as f32 + 1.0) / n as f32 * TAU;
            let dir = Vec2::new(a.cos(), a.sin());
            let p = CENTER + dir * r_beam;
            let half = r_beam * PI / n as f32 + 1.2;
            self.slab(
                Vec3::new(p.x - half, PLAZA_H + h, p.y - 1.1),
                Vec3::new(p.x + half, PLAZA_H + h + 1.4, p.y + 1.1),
                -a + FRAC_PI_2,
                STONE * 0.95,
                Material::Concrete,
                false,
            );
        }
    }

    // -----------------------------------------------------------------------
    // The avenues

    /// An avenue from the plaza out to the edge along `dir`: statues on
    /// plinths down both sides (some toppled), banner masts between them.
    fn avenue(&mut self, rng: &mut Rng, dir: Vec2) {
        let side = Vec2::new(-dir.y, dir.x);
        let start = PLAZA_HALF + PLAZA_RAMP + 4.0;
        let mut d = start;
        let mut k = 0;
        while d < MID - 12.0 {
            for s in [-1.0f32, 1.0] {
                let p = CENTER + dir * d + side * s * (AVENUE_HALF - 1.5);
                // Skip the arch's footprint.
                if (d - ARCH_AT).abs() < 9.0 {
                    continue;
                }
                if k % 2 == 0 {
                    self.statue(rng, p, -side * s);
                } else {
                    self.mast(p, dir);
                }
            }
            d += 15.0;
            k += 1;
        }
    }

    /// A statue on a plinth facing `face`: a robed figure in blocks. Some
    /// stand, some have lost their heads, some lie toppled beside the plinth.
    fn statue(&mut self, rng: &mut Rng, at: Vec2, face: Vec2) {
        let g = 0.0;
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
    fn mast(&mut self, at: Vec2, along: Vec2) {
        let h = 14.0;
        self.pipe(
            Vec3::new(at.x, 0.0, at.y),
            Vec3::new(at.x, h, at.y),
            0.18,
            CONCRETE_DARK,
            Material::Painted,
            true,
        );
        let a = Vec3::new(along.x, 0.0, along.y);
        let top = Vec3::new(at.x, h - 0.4, at.y);
        self.pipe(
            top - a * 1.4,
            top + a * 1.4,
            0.08,
            GOLD,
            Material::Metal,
            false,
        );
        // The banner hangs in the avenue's direction, facing across it.
        let face = Vec2::new(-along.y, along.x);
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
    fn arch(&mut self, rng: &mut Rng, dir: Vec2, ruined: bool) {
        let c = CENTER + dir * ARCH_AT;
        let yaw = (-dir.y).atan2(dir.x);
        let rig = Rig {
            pos: Vec3::new(c.x, 0.0, c.y),
            // Local x across the avenue, z along it.
            rot: Quat::from_rotation_y(yaw - FRAC_PI_2),
        };
        let (opening, pier_w, depth, pier_h) = (6.5, 6.0, 5.0, 13.0);
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
                0.0,
            );
            if broken {
                self.oboxes.last_mut().unwrap().broken = true;
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
            // The curve's height under this strip, taken at its lower edge.
            let edge = x0.abs().max(x1.abs()).min(opening);
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
                0.0,
            );
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
                Vec3::new(0.55, opening * PI / arc_n as f32 * 0.52, depth * 0.5 + 0.12),
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
            for (x, tilt, roll) in [(-10.5f32, 0.25f32, 0.1f32), (10.5, -0.2, -0.35)] {
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
            let e = rig.point(Vec3::new(-10.5, 2.3, 6.0 + depth * 0.5 + 0.2));
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
            0.0,
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

    /// Blocks between the avenues: ordinary concrete buildings (many shelled)
    /// on a loose grid of streets, a brutalist slab on pillars here and
    /// there, and the odd empty lot of rubble.
    fn districts(&mut self, rng: &mut Rng) -> Vec<Drift> {
        let mut drifts = Vec::new();
        let step = 34.0;
        let n = (SIZE / step) as i32;
        for i in 0..n {
            for j in 0..n {
                let c = Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * step
                    + Vec2::new(rng.range(-3.0, 3.0), rng.range(-3.0, 3.0));
                let half = Vec2::new(rng.range(8.5, 12.5), rng.range(7.5, 11.5));
                let yaw = if rng.f32() < 0.5 { 0.0 } else { FRAC_PI_2 } + rng.range(-0.05, 0.05);
                let lot = OBox {
                    center: c,
                    yaw,
                    half,
                    y0: 0.0,
                    y1: 1.0,
                    ..OBox::default()
                };
                if lot.corners().iter().chain([c].iter()).any(|&p| reserved(p)) {
                    continue;
                }
                // The face toward the middle is the front.
                let to_mid = lot.local_point(CENTER);
                let front = if (to_mid.x / half.x).abs() > (to_mid.y / half.y).abs() {
                    (0, to_mid.x.signum())
                } else {
                    (1, to_mid.y.signum())
                };
                match rng.f32() {
                    x if x < 0.22 => self.brutalist(rng, &lot, front),
                    x if x < 0.9 => {
                        drifts.extend(ruins::building_in(self, rng, lot, front, &|_| true, &CAST))
                    }
                    _ => self.rubble_lot(rng, &lot),
                }
            }
        }
        drifts
    }

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
        let color = CONCRETES[rng.index(CONCRETES.len())] * rng.range(0.92, 1.0);
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
            color * 0.9,
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
                self.oboxes.last_mut().unwrap().broken = true;
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
        }
        // Fins up the front, and the banner.
        if !shelled {
            let mut x = -hw + 1.5;
            while x < hw - 1.0 {
                put(
                    self,
                    Vec3::new(x, (lift + top) * 0.5, hd + over + 0.45),
                    Vec3::new(0.18, (top - lift) * 0.5, 0.45),
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
    fn rubble_lot(&mut self, rng: &mut Rng, lot: &OBox) {
        for _ in 0..rng.int(5, 10) {
            let q = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * lot.half * 0.8;
            let (ax, az) = lot.axes();
            let p = lot.center + ax * q.x + az * q.y;
            let s = rng.range(0.8, 2.4);
            self.oboxes.push(OBox {
                center: p,
                yaw: rng.range(0.0, TAU),
                half: Vec2::new(s, s * rng.range(0.4, 1.0)),
                y0: -0.3,
                y1: s * rng.range(0.4, 0.9),
                tilt: rng.range(-0.3, 0.3),
                roll: rng.range(-0.3, 0.3),
                color: CONCRETE * rng.range(0.8, 1.0),
                side_mat: Material::Rubble,
                top_mat: Material::Rubble,
                solid: s > 1.5,
                chip: 0.6,
                ..OBox::default()
            });
        }
    }

    // -----------------------------------------------------------------------
    // Bases, spawns, the edge

    /// Each corner: a staging court behind low barriers, open toward the
    /// middle, with two masts flying its side's color.
    fn staging_courts(&mut self, _rng: &mut Rng) {
        let factions = self.factions.clone();
        for (k, (corner, inward)) in courts().into_iter().enumerate() {
            let at = |u: f32, v: f32| corner + Vec2::new(inward.x * u, inward.y * v);
            // Barriers along the two inner edges, with a wide gap in each.
            for (u0, v0, u1, v1) in [
                (2.0, COURT, COURT * 0.35, COURT + 1.4),
                (COURT * 0.65, COURT, COURT + 1.4, COURT + 1.4),
                (COURT, 2.0, COURT + 1.4, COURT * 0.35),
                (COURT, COURT * 0.65, COURT + 1.4, COURT),
            ] {
                let (a, b) = (at(u0, v0), at(u1, v1));
                let (lo, hi) = (a.min(b), a.max(b));
                self.slab(
                    Vec3::new(lo.x, 0.0, lo.y),
                    Vec3::new(hi.x, 1.7, hi.y),
                    0.0,
                    CONCRETE * 0.85,
                    Material::Concrete,
                    true,
                );
            }
            let color = factions[k].paint();
            for (u, v) in [(COURT - 3.0, COURT * 0.5), (COURT * 0.5, COURT - 3.0)] {
                let p = at(u, v);
                self.pipe(
                    Vec3::new(p.x, 0.0, p.y),
                    Vec3::new(p.x, 12.0, p.y),
                    0.16,
                    CONCRETE_DARK,
                    Material::Painted,
                    true,
                );
                let rig = Rig {
                    pos: Vec3::new(p.x, 8.4, p.y),
                    rot: Quat::from_rotation_y(PI * 0.25),
                };
                rig.part(
                    self,
                    Vec3::new(1.4, 0.0, 0.0),
                    Vec3::new(1.2, 3.4, 0.05),
                    Quat::IDENTITY,
                    color,
                    Material::Cloth,
                    false,
                    0.0,
                );
            }
        }
    }

    fn capital_spawns(&mut self) {
        let clear = |m: &Map, p: Vec2| {
            !m.oboxes
                .iter()
                .any(|o| o.solid && o.distance(p) < TANK_CLEAR)
        };
        for (k, (corner, inward)) in courts().into_iter().enumerate() {
            let mut spots = Vec::new();
            for i in 0..5 {
                for j in 0..5 {
                    let p = corner
                        + Vec2::new(inward.x, inward.y)
                            * Vec2::new(9.0 + i as f32 * 7.0, 9.0 + j as f32 * 7.0);
                    if clear(self, p) {
                        spots.push(p);
                    }
                }
            }
            self.bases[k] = spots;
        }
        let mut open = Vec::new();
        let step = 10.0;
        let n = (SIZE / step) as i32;
        for i in 0..n {
            for j in 0..n {
                let p = Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * step;
                if clear(self, p) && p.distance(CENTER) > ROTUNDA_R + 2.0 {
                    open.push(p);
                }
            }
        }
        self.open_spots = open;
        // Repair crates: halfway down each avenue, and on the plaza's
        // diagonals between the colonnade and the edge.
        let mut repairs = Vec::new();
        for &d in &AXES {
            repairs.push(CENTER + d * 130.0);
        }
        for k in 0..4 {
            let a = PI * 0.25 + k as f32 * FRAC_PI_2;
            repairs.push(CENTER + Vec2::new(a.cos(), a.sin()) * 58.0);
        }
        self.repair_spots = repairs.into_iter().filter(|&p| clear(self, p)).collect();
    }

    /// Beyond the edge the city goes on: concrete towers and slabs (some
    /// broken off), leaving the avenues' vistas open. Visual only.
    fn skyline(&mut self, rng: &mut Rng) {
        let mut placed: Vec<OBox> = Vec::new();
        for _ in 0..900 {
            let p = Vec2::new(
                rng.range(-160.0, SIZE + 160.0),
                rng.range(-160.0, SIZE + 160.0),
            );
            let outside = p.x < -24.0 || p.y < -24.0 || p.x > SIZE + 24.0 || p.y > SIZE + 24.0;
            let d = p - CENTER;
            let vista = d.x.abs() < AVENUE_HALF + 10.0 || d.y.abs() < AVENUE_HALF + 10.0;
            if !outside || vista {
                continue;
            }
            let far = ((p - CENTER).abs().max_element() - MID).max(0.0);
            let half = Vec2::new(rng.range(7.0, 18.0), rng.range(7.0, 18.0));
            let lot = OBox {
                center: p,
                half,
                yaw: if rng.f32() < 0.5 { 0.0 } else { FRAC_PI_2 },
                ..OBox::default()
            };
            if placed.iter().any(|o| o.overlaps(&lot, 4.0)) {
                continue;
            }
            let h = rng.range(10.0, 26.0) + far * rng.range(0.1, 0.35);
            // Darker than the city inside: they sit back in the haze.
            let color = CONCRETES[rng.index(CONCRETES.len())] * rng.range(0.6, 0.75);
            self.oboxes.push(OBox {
                y0: 0.0,
                y1: h,
                color,
                side_mat: Material::Concrete,
                top_mat: Material::Concrete,
                solid: false,
                broken: rng.f32() < 0.35,
                ..lot
            });
            placed.push(lot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_sides_each_with_a_court_full_of_spawns() {
        let m = Map::capital();
        assert_eq!(m.factions, Faction::ALL.to_vec());
        assert_eq!(m.bases.len(), 4);
        for (k, base) in m.bases.iter().enumerate() {
            assert!(base.len() >= 12, "side {k} has {} spawns", base.len());
        }
        assert!(m.open_spots.len() > 100);
        assert!(m.repair_spots.len() >= 4);
    }

    #[test]
    fn plaza_is_raised_and_its_slope_drivable() {
        let m = Map::capital();
        assert!((m.ground_height(MID + 40.0, MID) - PLAZA_H).abs() < 0.05);
        assert!(m.ground_height(MID + 120.0, MID).abs() < 0.05);
        let mut prev = m.ground_height(MID + 40.0, MID);
        let mut x = MID + 40.0;
        while x < MID + 100.0 {
            x += 1.0;
            let h = ground(x, MID);
            assert!((prev - h).abs() < 0.25, "plaza edge too steep at x={x}");
            prev = h;
        }
    }

    #[test]
    fn the_avenues_are_open_to_the_middle() {
        let m = Map::capital();
        // Down each avenue's center line, from the edge to the colonnade,
        // nothing solid stands at tank height (the ruined arch's fallen
        // attic lies to one side of the line).
        for &d in &AXES {
            let mut r = COLONNADE_R + 4.0;
            while r < MID - 8.0 {
                let p = CENTER + d * r + Vec2::new(-d.y, d.x) * 3.0;
                assert!(
                    !m.oboxes.iter().any(|o| {
                        let (y0, _) = o.y_range();
                        o.solid && y0 < 3.0 && o.distance(p) < 1.0
                    }),
                    "avenue {d:?} blocked at {r}"
                );
                r += 2.0;
            }
        }
    }
}
