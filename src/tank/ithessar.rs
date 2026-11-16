//! Ithessar kit: the hover hull and the plasma turrets.
//!
//! The Ithessar shape plasma with crystal. Their machines are knapped from
//! dark worked crystal (vhaal) instead of plated: long faceted blades, split
//! down the spine where the plasma that drives them shows through. Nothing
//! is bolted on; parts float where the field holds them.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::{Vec2, Vec3, Vec4};

use super::model::{
    METAL, PAINT, Rig, Skin, TurretModel, at_y, at_z, ngon, plate, plate2, round, shard, solid,
    turret_ring, v,
};
use crate::render::material::Material;
use crate::render::mesh::MeshData;

/// Worked crystal: the Ithessar's armor. Crystal is blue; theirs is cut
/// and dense, so it reads deep blue. The color that's theirs is the rose
/// plasma it holds (raw blue crystal is the Resonant's).
pub const VHAAL: Skin = Skin::new(
    Vec4::new(0.016, 0.045, 0.12, Material::Vhaal.id()),
    Vec4::new(0.05, 0.12, 0.28, Material::Vhaal.id()),
);
/// Loose crystal held in the field, lit rose by the plasma inside it.
const SHARD: Skin = Skin::new(
    Vec4::new(0.42, 0.13, 0.3, Material::Glow.id()),
    Vec4::new(0.6, 0.22, 0.42, Material::Glow.id()),
);
/// Plasma showing through seams in the crystal (pulses).
const SEAM: Skin = Skin::new(
    Vec4::new(1.0, 0.3, 0.55, Material::Glow.id()),
    Vec4::new(1.0, 0.45, 0.65, Material::Glow.id()),
);
/// Dark alloy for the few cradles that hold crystal.
const CRADLE: Skin = Skin::new(
    Vec4::new(0.12, 0.11, 0.13, Material::Metal.id()),
    Vec4::new(0.22, 0.2, 0.23, Material::Metal.id()),
);

/// Plasma color for the parts drawn as emissive instances (unlit, color
/// times the instance's glow strength).
pub const PLASMA: Vec3 = Vec3::new(1.0, 0.16, 0.52);
pub const PLASMA_HOT: Vec3 = Vec3::new(1.0, 0.7, 0.86);

fn glow_color(c: Vec3) -> Skin {
    let c = c.extend(0.0);
    Skin::new(c, c)
}

/// The hull is modelled with its belly at 0.4; it rides this much higher
/// (the physics chassis's emitters are `road_y` up).
pub const RIDE: f32 = 0.3;

/// The hover hull's art. Everything but `body` is drawn per frame with its
/// own motion and glow.
pub struct HoverModel {
    pub body: MeshData,
    /// Plasma core and flank vents: drawn emissive, pulsing.
    pub glow: MeshData,
    /// One emitter's cradle (lens down, at its origin) and its lens.
    pub emitter: MeshData,
    pub lens: MeshData,
    /// A loose crystal, 1 m long along +y; drawn at each of `shards`.
    pub shard: MeshData,
    /// Where loose crystals float (body frame) and their length.
    pub shards: Vec<(Vec3, f32)>,
    /// Emitter cradle height in the model.
    pub emitter_y: f32,
    /// Turret ring position.
    pub mount: Vec3,
}

/// Small fixed jitter so knapped facets aren't perfectly regular.
fn jitter(i: usize, k: usize) -> f32 {
    let h = ((i * 7919 + k * 104_729) % 1013) as f32 / 1013.0;
    (h - 0.5) * 2.0
}

/// One carapace half's cross-section at a station along the hull (x > 0,
/// the right half is mirrored). `slot` is the half-width of the split down
/// the spine.
fn half_section(i: usize, z: f32, w: f32, belly: f32, top: f32, slot: f32) -> Vec<Vec3> {
    let h = top - belly;
    let span = w - slot;
    // A wedge: tall at the spine, the roof falling away to a sharp outer
    // edge low on the flank, a shallow keel underneath.
    let pts = [
        Vec2::new(slot, belly + 0.22 * h),
        Vec2::new(slot, top),
        Vec2::new(slot + 0.35 * span, top - 0.12 * h),
        Vec2::new(slot + 0.78 * span, belly + 0.5 * h),
        Vec2::new(w, belly + 0.24 * h),
        Vec2::new(slot + 0.8 * span, belly + 0.04 * h),
        Vec2::new(slot + 0.3 * span, belly),
    ];
    pts.iter()
        .enumerate()
        .map(|(k, p)| Vec3::new(p.x, p.y + jitter(i, k) * 0.025 * h, z))
        .collect()
}

/// Stations along the hull: (z, half-width, belly, top, slot).
const STATIONS: [(f32, f32, f32, f32, f32); 8] = [
    (2.85, 0.2, 0.66, 0.8, 0.04),
    (2.3, 0.66, 0.5, 1.0, 0.07),
    (1.3, 1.16, 0.42, 1.2, 0.11),
    (0.0, 1.4, 0.4, 1.3, 0.14),
    (-1.25, 1.38, 0.42, 1.36, 0.16),
    (-1.95, 1.14, 0.58, 1.42, 0.18),
    (-2.55, 0.74, 0.92, 1.46, 0.2),
    // The tail sweeps up and narrows to a blade.
    (-3.1, 0.32, 1.22, 1.46, 0.2),
];

/// Carapace surface at (z, fraction of the way from slot to the outer edge),
/// for placing things on it: (outer x, ridge top) by linear interpolation.
fn station_at(z: f32) -> (f32, f32, f32, f32) {
    for w in STATIONS.windows(2) {
        let (a, b) = (w[0], w[1]);
        if z <= a.0 && z >= b.0 {
            let f = (a.0 - z) / (a.0 - b.0);
            let l = |x: f32, y: f32| x + (y - x) * f;
            return (l(a.1, b.1), l(a.2, b.2), l(a.3, b.3), l(a.4, b.4));
        }
    }
    let s = if z > 0.0 {
        STATIONS[0]
    } else {
        STATIONS[STATIONS.len() - 1]
    };
    (s.1, s.2, s.3, s.4)
}

/// Sethuun: a long knapped-crystal blade split down its spine, floating on
/// six plasma emitters, with swept tail vanes and loose crystals riding in
/// its field.
pub fn sethuun() -> HoverModel {
    let mut body = MeshData::default();
    for side in [1.0f32, -1.0] {
        let rings: Vec<Vec<Vec3>> = STATIONS
            .iter()
            .enumerate()
            .map(|(i, &(z, w, belly, top, slot))| {
                half_section(i, z, w, belly, top, slot)
                    .into_iter()
                    .map(|p| Vec3::new(p.x * side, p.y, p.z))
                    .collect()
            })
            .collect();
        solid(&mut body, &rings, 0.03, VHAAL);
    }
    // The keel under the split, so the spine shows plasma, not daylight.
    solid(
        &mut body,
        &[
            at_z(
                &super::model::rect(Vec2::new(0.0, 0.62), Vec2::new(0.13, 0.2), 0.04),
                -1.9,
            ),
            at_z(
                &super::model::rect(Vec2::new(0.0, 0.62), Vec2::new(0.13, 0.2), 0.04),
                2.4,
            ),
        ],
        0.03,
        VHAAL,
    );

    // The side's color: panels laid along each half's roof, from behind the
    // turret to the stern (where the chase camera sees them), and a pair
    // flanking the prow. Flat on the crystal so nothing snags the gun.
    let roof = |z: f32, f: f32| {
        let (w, b, t, sl) = station_at(z);
        let span = w - sl;
        let h = t - b;
        let p = Vec2::new(sl + 0.04, t).lerp(Vec2::new(sl + 0.35 * span, t - 0.12 * h), f);
        v(p.x, p.y + 0.02, z)
    };
    for (z0, z1) in [(-1.0f32, -2.45f32), (2.25, 1.15)] {
        plate2(
            &mut body,
            [
                roof(z0, 0.15),
                roof(z0, 0.95),
                roof(z1, 0.95),
                roof(z1, 0.15),
            ],
            0.03,
            v(0.0, 0.6, (z0 + z1) * 0.5),
            PAINT,
        );
    }
    // Glow: the core along the spine, and three vents down each flank.
    let mut glow = MeshData::default();
    let core = glow_color(PLASMA);
    let hot = glow_color(PLASMA_HOT);
    round(
        &mut glow,
        &[
            at_z(&ngon(6, 0.05, Vec2::new(0.0, 0.9)), 2.5),
            at_z(&ngon(6, 0.12, Vec2::new(0.0, 0.98)), 1.7),
            at_z(&ngon(6, 0.13, Vec2::new(0.0, 1.0)), -1.6),
            at_z(&ngon(6, 0.12, Vec2::new(0.0, 1.0)), -2.6),
        ],
        0.0,
        core,
    );
    round(
        &mut glow,
        &[
            at_z(&ngon(6, 0.04, Vec2::new(0.0, 1.04)), 1.5),
            at_z(&ngon(6, 0.05, Vec2::new(0.0, 1.06)), -1.4),
        ],
        0.0,
        hot,
    );
    for k in 0..3 {
        let z = -0.9 + k as f32 * 0.55;
        for s in [1.0f32, -1.0] {
            let (w0, b0, t0, sl0) = station_at(z);
            let (w1, b1, t1, sl1) = station_at(z + 0.32);
            // On the falling facet above the outer edge.
            let pt = |w: f32, b: f32, t: f32, sl: f32, f: f32, z: f32| {
                let h = t - b;
                let top = Vec2::new(sl + 0.78 * (w - sl), b + 0.5 * h);
                let low = Vec2::new(w, b + 0.24 * h);
                let p = top.lerp(low, f);
                // Just proud of the facet (it faces out and up).
                v((p.x + 0.012) * s, p.y + 0.012, z)
            };
            plate(
                &mut glow,
                [
                    pt(w0, b0, t0, sl0, 0.2, z),
                    pt(w0, b0, t0, sl0, 0.75, z),
                    pt(w1, b1, t1, sl1, 0.75, z + 0.22),
                    pt(w1, b1, t1, sl1, 0.2, z + 0.32),
                ],
                0.02,
                v(0.0, 0.8, z),
                core,
            );
        }
    }

    // Emitter: a hexagonal cradle with three claws gripping a downward lens.
    let mut emitter = MeshData::default();
    round(
        &mut emitter,
        &[
            at_y(&ngon(6, 0.16, Vec2::ZERO), 0.14),
            at_y(&ngon(6, 0.22, Vec2::ZERO), 0.06),
            at_y(&ngon(6, 0.2, Vec2::ZERO), -0.02),
        ],
        0.02,
        CRADLE,
    );
    let mut lens = MeshData::default();
    round(
        &mut lens,
        &[
            at_y(&ngon(6, 0.15, Vec2::ZERO), 0.0),
            at_y(&ngon(6, 0.12, Vec2::ZERO), -0.07),
            at_y(&ngon(6, 0.02, Vec2::ZERO), -0.14),
        ],
        0.0,
        hot,
    );

    let mut loose = MeshData::default();
    shard(&mut loose, Vec3::ZERO, Vec3::Y, 1.0, 0.16, SHARD);

    HoverModel {
        body,
        glow,
        emitter,
        lens,
        shard: loose,
        shards: vec![
            // The eye ahead of the prow.
            (v(0.0, 1.0, 3.3), 0.5),
        ],
        emitter_y: 0.36,
        mount: v(0.0, 1.3, -0.15),
    }
}

/// A faceted low crown for the turrets: `n`-sided rings, stretched along z.
fn crown(m: &mut MeshData, r: f32, len: f32, h: f32, cz: f32) {
    let ring = |rr: f32, y: f32, k: usize| -> Vec<Vec3> {
        ngon(7, 1.0, Vec2::ZERO)
            .iter()
            .enumerate()
            .map(|(j, p)| {
                let j = jitter(k, j) * 0.03;
                Vec3::new(p.x * rr * (1.0 + j), y, cz + p.y * rr * len * (1.0 - j))
            })
            .collect()
    };
    solid(
        m,
        &[
            ring(r * 0.95, 0.1, 0),
            ring(r, 0.1 + h * 0.45, 1),
            ring(r * 0.72, 0.1 + h * 0.85, 2),
            ring(r * 0.3, 0.1 + h, 3),
        ],
        0.03,
        VHAAL,
    );
}

/// Ammeth: a cradle of three crystal claws. Plasma gathers between them
/// into an orb as it recharges; the claws spring open to throw it.
pub fn ammeth() -> TurretModel {
    let mut head = MeshData::default();
    turret_ring(&mut head);
    crown(&mut head, 0.92, 1.25, 0.62, -0.2);
    // Swept ears in the side's color, and a crest of shards at the back.
    plate2(
        &mut head,
        [
            v(0.55, 0.42, -0.2),
            v(0.85, 0.36, -0.55),
            v(1.0, 0.62, -1.35),
            v(0.8, 0.66, -1.15),
        ],
        0.05,
        v(0.0, 0.4, -0.6),
        PAINT,
    );
    for (x, lean, len) in [
        (0.0f32, 0.0f32, 0.75f32),
        (0.2, 0.35, 0.5),
        (-0.2, -0.35, 0.5),
    ] {
        shard(
            &mut head,
            v(x, 0.6, -0.85),
            Vec3::new(lean, 1.0, -0.9),
            len,
            0.09,
            SHARD,
        );
    }

    // Collar round the gun axis, a bright ring at its mouth.
    let mut mantlet = MeshData::default();
    round(
        &mut mantlet,
        &[
            at_z(&ngon(6, 0.24, Vec2::ZERO), -0.45),
            at_z(&ngon(6, 0.42, Vec2::ZERO), -0.15),
            at_z(&ngon(6, 0.42, Vec2::ZERO), 0.25),
            at_z(&ngon(6, 0.3, Vec2::ZERO), 0.45),
        ],
        0.03,
        VHAAL,
    );
    round(
        &mut mantlet,
        &[
            at_z(&ngon(6, 0.2, Vec2::ZERO), 0.44),
            at_z(&ngon(6, 0.2, Vec2::ZERO), 0.48),
        ],
        0.0,
        SEAM,
    );

    // One claw, outward along +y: bows out to hold the orb, hooks in at the tip.
    let mut claw = MeshData::default();
    let path = [
        (0.0f32, 0.0f32, 0.1f32),
        (0.5, 0.12, 0.11),
        (1.1, 0.3, 0.1),
        (1.7, 0.26, 0.08),
        (2.15, 0.05, 0.05),
        (2.4, -0.12, 0.012),
    ];
    let rings: Vec<Vec<Vec3>> = path
        .iter()
        .map(|&(z, y, r)| {
            // Flattened hexagon: wider across than deep.
            ngon(6, r, Vec2::ZERO)
                .iter()
                .map(|p| Vec3::new(p.x * 1.4, y + p.y, z))
                .collect()
        })
        .collect();
    solid(&mut claw, &rings, 0.01, VHAAL);
    // Its inner edge glows: the face that holds the plasma.
    let inner: Vec<Vec<Vec3>> = path[1..5]
        .iter()
        .map(|&(z, y, r)| {
            at_z(
                &super::model::rect(Vec2::new(0.0, y - r * 0.85), Vec2::new(r * 0.5, 0.03), 0.01),
                z,
            )
        })
        .collect();
    solid(&mut claw, &inner, 0.0, SEAM);

    let r = 0.34;
    TurretModel {
        head,
        head_light: MeshData::default(),
        barrel_light: MeshData::default(),
        mantlet,
        barrel: claw,
        pivot: v(0.0, 0.5, 0.55),
        barrels: (0..3)
            .map(|k| {
                let a = FRAC_PI_2 + k as f32 / 3.0 * TAU;
                v(a.cos() * r, a.sin() * r, 0.1)
            })
            .collect(),
        muzzle: 1.45,
        recoil: 0.0,
        rig: Rig::Cradle {
            orb: 1.45,
            size: 0.8,
        },
    }
}

/// Ossrin: three crystal needles turning round the gun axis inside two
/// focusing rings. They spin up as it fires; each needle throws a bolt as
/// it comes round.
pub fn ossrin() -> TurretModel {
    let mut head = MeshData::default();
    turret_ring(&mut head);
    crown(&mut head, 0.86, 1.4, 0.52, -0.25);
    // A dorsal blade, and side fins in the side's color.
    plate(
        &mut head,
        [
            v(0.0, 0.55, 0.3),
            v(0.0, 0.62, -1.3),
            v(0.0, 1.05, -1.55),
            v(0.0, 0.92, -0.5),
        ],
        0.06,
        v(0.4, 0.4, -0.6),
        VHAAL,
    );
    plate2(
        &mut head,
        [
            v(0.7, 0.3, 0.1),
            v(0.92, 0.28, -0.4),
            v(1.08, 0.48, -1.25),
            v(0.86, 0.5, -0.9),
        ],
        0.05,
        v(0.0, 0.4, -0.5),
        PAINT,
    );
    // Seams along the crown's flanks.
    for s in [1.0f32, -1.0] {
        plate(
            &mut head,
            [
                v(s * 0.86, 0.36, 0.2),
                v(s * 0.86, 0.36, -0.9),
                v(s * 0.84, 0.3, -0.9),
                v(s * 0.84, 0.3, 0.2),
            ],
            0.015,
            v(0.0, 0.33, -0.3),
            SEAM,
        );
    }

    // Two focusing rings on a spindle; the needles turn inside them.
    let mut mantlet = MeshData::default();
    round(
        &mut mantlet,
        &[
            at_z(&ngon(8, 0.09, Vec2::ZERO), -0.5),
            at_z(&ngon(8, 0.09, Vec2::ZERO), 1.9),
        ],
        0.01,
        METAL,
    );
    round(
        &mut mantlet,
        &[
            at_z(&ngon(6, 0.2, Vec2::ZERO), -0.55),
            at_z(&ngon(6, 0.36, Vec2::ZERO), -0.3),
            at_z(&ngon(6, 0.36, Vec2::ZERO), 0.0),
            at_z(&ngon(6, 0.3, Vec2::ZERO), 0.15),
        ],
        0.03,
        VHAAL,
    );
    for (z, r) in [(0.95f32, 0.38f32), (1.75, 0.33)] {
        // A ring: six blocks round the axis, a glowing band inside.
        for k in 0..6 {
            let a = k as f32 / 6.0 * TAU;
            let d = Vec2::new(a.cos(), a.sin());
            let t = Vec2::new(-d.y, d.x);
            let (i0, o0) = (d * (r - 0.08), d * r);
            let half = 0.19;
            let quad = |rr: Vec2| {
                [
                    rr - t * half * rr.length() / r,
                    rr + t * half * rr.length() / r,
                ]
            };
            let [a0, a1] = quad(i0);
            let [b0, b1] = quad(o0);
            let ring = |zz: f32| -> Vec<Vec3> {
                [a0, a1, b1, b0]
                    .iter()
                    .map(|p| Vec3::new(p.x, p.y, zz))
                    .collect()
            };
            solid(
                &mut mantlet,
                &[ring(z - 0.07), ring(z + 0.07)],
                0.015,
                CRADLE,
            );
        }
        round(
            &mut mantlet,
            &[
                at_z(&ngon(12, r - 0.08, Vec2::ZERO), z - 0.03),
                at_z(&ngon(12, r - 0.08, Vec2::ZERO), z + 0.03),
            ],
            0.0,
            SEAM,
        );
    }

    let mut needle = MeshData::default();
    shard(&mut needle, v(0.0, 0.0, 0.1), Vec3::Z, 2.25, 0.075, SHARD);

    let r = 0.17;
    TurretModel {
        head,
        head_light: MeshData::default(),
        barrel_light: MeshData::default(),
        mantlet,
        barrel: needle,
        pivot: v(0.0, 0.44, 0.62),
        barrels: (0..3)
            .map(|k| {
                let a = FRAC_PI_2 + k as f32 / 3.0 * TAU;
                v(a.cos() * r, a.sin() * r, 0.0)
            })
            .collect(),
        muzzle: 2.35,
        recoil: 0.0,
        rig: Rig::Spinner,
    }
}
