//! The desert biome: open land nobody's built on. Laid out as a skeleton
//! (`skeleton`): its form (an erg, canyonlands, a basin, an escarpment),
//! the high ground's cliffs and the ramps up them, the landmark's slot,
//! each side's spawns placed for a fair drive, the lots. Checked for play
//! on its greybox (a skeleton that fails is laid out again from the next
//! draw), then dressed (`dress`): the cliffs sculpted as sandstone (the
//! crystal mine's rock faces, without its seams), the landmark and the
//! lots filled in from the library of natural pieces (spires, an arch, a
//! crater's rim, fins, boulder fields), cover wherever the open ground's
//! wide, a wall of boulders round the edge and the desert going on to the
//! horizon past it.
//!
//! Nothing here is anyone's, but for a shield generator (#337): with
//! `Centerpiece::ShieldGenerator` the landmark's slot is a generator's, a
//! graded pad out on the floor with room round it, and the holder's
//! standard model goes up on it (see `dress::Sand::generator`) with a camp
//! round it: tracks beaten out to it, sand fences, supply dumps.

pub mod dress;
pub mod skeleton;

use std::f32::consts::TAU;

use glam::{Vec2, Vec3};

use super::{Centerpiece, Spec};
use crate::ai::nav::NavGrid;
use crate::faction::Faction;
use crate::map::{Map, OBox, Terrain};
use crate::render::material::Material;
use crate::rng::Rng;
pub use skeleton::{Form, LandmarkKind, LotKind, Skeleton};

/// The sand and rock of one desert: a few kinds, one per seed.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub sand: Vec3,
    /// Sand in the troughs, the wash's gravel.
    pub low: Vec3,
    pub stone: Vec3,
    pub stone_dark: Vec3,
    pub stone_pale: Vec3,
    pub planet: crate::planet::Look,
}

pub const PALETTES: [Palette; 3] = [
    // Ochre: yellow sand, buff stone.
    Palette {
        sand: Vec3::new(0.74, 0.58, 0.38),
        low: Vec3::new(0.6, 0.48, 0.34),
        stone: Vec3::new(0.62, 0.45, 0.3),
        stone_dark: Vec3::new(0.44, 0.31, 0.21),
        stone_pale: Vec3::new(0.76, 0.6, 0.44),
        planet: crate::planet::Look {
            land: Vec3::new(0.42, 0.32, 0.18),
            basin: Vec3::new(0.26, 0.19, 0.11),
            sea: Vec3::new(0.04, 0.07, 0.1),
            cloud: Vec3::new(0.5, 0.48, 0.45),
            cloudiness: 0.2,
        },
    },
    // Red: rust sand, red sandstone.
    Palette {
        sand: Vec3::new(0.7, 0.45, 0.3),
        low: Vec3::new(0.56, 0.36, 0.25),
        stone: Vec3::new(0.58, 0.34, 0.22),
        stone_dark: Vec3::new(0.4, 0.23, 0.15),
        stone_pale: Vec3::new(0.7, 0.5, 0.34),
        planet: crate::planet::Look {
            land: Vec3::new(0.38, 0.2, 0.11),
            basin: Vec3::new(0.2, 0.1, 0.06),
            sea: Vec3::new(0.03, 0.06, 0.1),
            cloud: Vec3::new(0.45, 0.42, 0.4),
            cloudiness: 0.25,
        },
    },
    // Pale: bleached sand, grey-brown rock.
    Palette {
        sand: Vec3::new(0.8, 0.7, 0.55),
        low: Vec3::new(0.64, 0.57, 0.47),
        stone: Vec3::new(0.56, 0.47, 0.38),
        stone_dark: Vec3::new(0.38, 0.32, 0.26),
        stone_pale: Vec3::new(0.72, 0.64, 0.53),
        planet: crate::planet::Look {
            land: Vec3::new(0.46, 0.4, 0.3),
            basin: Vec3::new(0.3, 0.26, 0.2),
            sea: Vec3::new(0.05, 0.08, 0.11),
            cloud: Vec3::new(0.55, 0.55, 0.53),
            cloudiness: 0.15,
        },
    },
];

/// The form a seed's desert takes: each in turn, so any four seeds in a
/// row show them all.
pub fn form_of(seed: u64) -> Form {
    Form::ALL[(seed % Form::ALL.len() as u64) as usize]
}

/// The skeleton `spec` lays out: the first, from its seed on, that has a
/// fair place for every side's spawns and whose greybox is playable.
pub fn skeleton(spec: &Spec) -> Skeleton {
    let form = form_of(spec.seed);
    let mut last = None;
    for attempt in 0..24u64 {
        let mut rng =
            Rng::new(spec.seed.wrapping_mul(0x9E37_79B9).wrapping_add(attempt) ^ 0x00DE_5E27);
        let generator = matches!(spec.centerpiece, Centerpiece::ShieldGenerator(..));
        let mut sk = Skeleton::generate(&mut rng, form, generator);
        let bare = greybox_of(&sk, &spec.sides);
        if !sk.place_bases(&mut rng, spec.sides.len(), &NavGrid::build(&bare)) {
            log::debug!(
                "desert seed {} attempt {attempt}: no fair spawns",
                spec.seed
            );
            continue;
        }
        sk.place_lots(&mut rng);
        let grey = greybox_of(&sk, &spec.sides);
        let nav = NavGrid::build(&grey);
        match super::check::playable_on(&grey, &nav).and_then(|()| fair(&sk, &nav)) {
            Ok(()) => return sk,
            Err(why) => log::debug!("desert seed {} attempt {attempt}: {why}", spec.seed),
        }
        last = Some(sk);
    }
    last.expect("no desert laid out in 24 draws")
}

/// Whether no side's drive to the landmark is much longer than another's.
fn fair(sk: &Skeleton, nav: &NavGrid) -> Result<(), String> {
    let lens: Vec<f32> = sk
        .bases
        .iter()
        .map(|&from| {
            let mut at = from;
            let mut len = 0.0;
            for p in nav.path(from, sk.landmark.at) {
                len += at.distance(p);
                at = p;
            }
            len
        })
        .collect();
    let (lo, hi) = lens
        .iter()
        .fold((f32::MAX, 0.0f32), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    if (hi - lo) / hi.max(1.0) > 0.18 {
        return Err(format!("unfair: {lo:.0} m against {hi:.0} m"));
    }
    Ok(())
}

/// A blank map for the skeleton: the arena, no spawns yet.
fn blank(sk: &Skeleton, sides: &[Faction]) -> Map {
    let mut map = Map::empty(sk.size);
    map.factions = sides.to_vec();
    map.bases = Map::no_bases(sides);
    map.open_spots = Vec::new();
    map.spawn_yaw = None;
    map.rock_dust = Some(sk.palette.sand);
    map.hold_site = Some(sk.landmark.at);
    map.hold_name = match sk.landmark.kind {
        LandmarkKind::Needles => "NEEDLES",
        LandmarkKind::Crater => "CRATER",
        LandmarkKind::Arch => "ARCH",
        LandmarkKind::Generator => "GENERATOR",
    };
    map
}

fn ground_grid(sk: &Skeleton) -> Terrain {
    let n = (sk.size / 2.0).round() as usize + 1;
    Terrain::from_fn(Vec2::ZERO, 2.0, n, n, |x, z| sk.ground(x, z))
}

/// The skeleton as its greybox: its ground in grey, the cliffs as the
/// boxes that stop tanks, each placeholder (the landmark, the lots) as a
/// low slab on its reach, the spawns. What `check` drives on.
fn greybox_of(sk: &Skeleton, sides: &[Faction]) -> Map {
    let mut map = blank(sk, sides);
    let mut t = ground_grid(sk);
    t.color = Vec3::splat(0.35);
    t.material = Material::Concrete;
    map.terrain = Some(t);
    for line in sk.cliffs() {
        map.rock_face(&line, sk.high, false, 0.0);
    }
    map.sculpted = Default::default();
    for o in map.oboxes.iter_mut() {
        o.hidden = false;
        o.color = Vec3::new(0.55, 0.45, 0.35);
    }
    let marker = |map: &mut Map, at: Vec2, r: f32, color: Vec3| {
        let g = map.ground_height(at.x, at.y);
        map.oboxes.push(OBox {
            center: at,
            half: Vec2::splat(r * 0.7),
            yaw: TAU / 8.0,
            y0: g - 0.5,
            y1: g + 0.3,
            color,
            solid: false,
            ..OBox::default()
        });
    };
    let lm = sk.landmark;
    marker(&mut map, lm.at, lm.reach, Vec3::new(0.7, 0.3, 0.2));
    for lot in &sk.lots {
        let c = match lot.kind {
            LotKind::Spires => Vec3::new(0.6, 0.5, 0.2),
            LotKind::Fins => Vec3::new(0.3, 0.5, 0.6),
            LotKind::Boulders => Vec3::new(0.4, 0.6, 0.3),
        };
        marker(&mut map, lot.at, lot.reach, c);
    }
    for side in 0..sk.bases.len().min(map.bases.len()) {
        map.bases[side] = sk.spawn_points(side);
    }
    map.open_spots = map.bases.concat();
    map.open_spots.push(lm.at);
    map
}

/// `spec`'s desert undressed.
pub fn greybox(spec: &Spec) -> Map {
    let sk = skeleton(spec);
    let mut m = greybox_of(&sk, &spec.sides);
    m.atmosphere = atmosphere(&mut Rng::new(spec.seed ^ 0x5EED_D2E5), &sk.palette);
    m
}

/// The sky over the desert: a high, hard sun from any quarter, a pale sky,
/// sand blowing.
pub fn atmosphere(rng: &mut Rng, pal: &Palette) -> crate::render::Atmosphere {
    let az = rng.range(0.0, TAU);
    let up = rng.range(0.55, 0.78);
    let flat = 0.81 * (1.0 - up * up).sqrt();
    crate::render::Atmosphere {
        sun_dir: Vec3::new(az.cos() * flat, up * 0.81, az.sin() * flat),
        sun_color: Vec3::new(2.3, 2.05, 1.7),
        sky_zenith: Vec3::new(0.22, 0.4, 0.68),
        sky_horizon: pal.sand * 0.55 + Vec3::new(0.32, 0.32, 0.34),
        ground_color: pal.sand * 0.6,
        fog_density: rng.range(0.0008, 0.0013),
        wind: Vec2::new(rng.range(-3.5, 3.5), rng.range(-3.5, 3.5)),
        blowing_sand: rng.range(0.3, 0.5),
        dust: pal.sand,
        settle: pal.sand,
        ambient: 1.0,
    }
}

/// Builds `spec`'s desert.
pub fn build(spec: &Spec) -> Map {
    build_with_claims(spec).0
}

/// Builds `spec`'s desert, and what each placeholder's filler put down.
pub fn build_with_claims(spec: &Spec) -> (Map, Vec<(dress::Claim, std::ops::Range<usize>)>) {
    let sk = skeleton(spec);
    let mut rng = Rng::new(spec.seed ^ 0x5EED_D2E5);
    let mut m = blank(&sk, &spec.sides);
    m.atmosphere = atmosphere(&mut rng, &sk.palette);
    m.planet = Some((spec.seed, sk.palette.planet));
    m.terrain = Some(dress::terrain(&sk));
    let claims = dress::dress(&mut m, &sk, spec.seed, spec.centerpiece);
    if sk.landmark.kind == LandmarkKind::Generator {
        // The generator's built flush, piece against piece, as a city is.
        m.separate_coplanar_faces();
    }
    (m, claims)
}

#[cfg(test)]
mod tests;
