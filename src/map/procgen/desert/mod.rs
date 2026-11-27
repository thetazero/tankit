//! The desert biome: open land nobody's built on. Laid out as a skeleton
//! (`skeleton`): its form (an erg, canyonlands, a basin, an escarpment),
//! the high ground's cliffs and the ramps up them, the landmark's slot,
//! each side's spawns placed for a fair drive, the lots, a rig at work in a
//! wash if there's one, great rocks on the high ground. Checked for play on
//! its greybox (a skeleton that fails is laid out again from the next
//! draw), then dressed (`dress`): the cliffs sculpted as sandstone, the
//! landmark and the lots filled in from the library of pieces (spires, an
//! arch, a crater's rim, fins, boulder fields), cover wherever the open
//! ground's wide, a wall of boulders round the edge and the desert going on
//! to the horizon past it.
//!
//! A reservoir town (`Biome::Reservoir`) is a desert with people in it: a
//! layout of its own (`town`), dressed by the same hands.
//!
//! The same land under snow or grass is `land`'s (`Biome::Open`'s ground).
//!
//! Nothing here is anyone's, but for a shield generator (#337): with
//! `Centerpiece::ShieldGenerator` the landmark's slot is a generator's, a
//! graded pad out on the floor with room round it, and the holder's
//! standard model goes up on it (see `dress::Sand::generator`) with a camp
//! round it: tracks beaten out to it, sand fences, supply dumps.

pub mod dress;
mod land;
pub mod skeleton;
pub mod town;

use std::f32::consts::TAU;

use glam::{Vec2, Vec3};

use super::foundry::Ground;
use super::memo::{Memo, SKELETONS};
use super::{Biome, Centerpiece, Spec};
use crate::ai::nav::NavGrid;
use crate::faction::Faction;
use crate::map::Map;
use crate::render::material::Material;
use crate::rng::Rng;
pub use skeleton::{Form, LandmarkKind, Skeleton};

/// The sand and rock of one desert: a few kinds, one per seed.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub sand: Vec3,
    /// Sand in the troughs, the wash's gravel.
    pub low: Vec3,
    pub stone: Vec3,
    pub stone_dark: Vec3,
    pub stone_pale: Vec3,
    /// What the ground's made of (sand here; other grounds' is snow or grass).
    pub material: Material,
    /// Weathered rock on the high ground's top, and how much of it shows.
    pub weathered: Vec3,
    pub exposed: f32,
    /// The ground where it's worn or blown bare, in patches, if it is.
    pub bare: Option<Vec3>,
    pub planet: crate::planet::Look,
}

/// Weathered rock, on a desert's high ground.
pub const WEATHERED: Vec3 = Vec3::new(0.46, 0.27, 0.18);

pub const PALETTES: [Palette; 3] = [
    // Ochre: yellow sand, buff stone.
    Palette {
        sand: Vec3::new(0.74, 0.58, 0.38),
        low: Vec3::new(0.6, 0.48, 0.34),
        stone: Vec3::new(0.62, 0.45, 0.3),
        stone_dark: Vec3::new(0.44, 0.31, 0.21),
        stone_pale: Vec3::new(0.76, 0.6, 0.44),
        material: Material::Sand,
        weathered: WEATHERED,
        exposed: 1.0,
        bare: None,
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
        material: Material::Sand,
        weathered: WEATHERED,
        exposed: 1.0,
        bare: None,
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
        material: Material::Sand,
        weathered: WEATHERED,
        exposed: 1.0,
        bare: None,
        planet: crate::planet::Look {
            land: Vec3::new(0.46, 0.4, 0.3),
            basin: Vec3::new(0.3, 0.26, 0.2),
            sea: Vec3::new(0.05, 0.08, 0.11),
            cloud: Vec3::new(0.55, 0.55, 0.53),
            cloudiness: 0.15,
        },
    },
];

/// The colors a seed's desert comes in.
pub fn palette(seed: u64) -> Palette {
    PALETTES[Rng::new(seed ^ 0x9A1E_77E0).index(PALETTES.len())]
}

/// The form a seed's desert takes: each in turn, so any four seeds in a
/// row show them all.
pub fn form_of(seed: u64) -> Form {
    Form::ALL[(seed % Form::ALL.len() as u64) as usize]
}

/// The skeleton `spec` lays out: the first, from its seed on, that has a
/// fair place for every side's spawns and whose greybox is playable. Worked
/// out only the first time it's asked for (the last few are kept).
pub fn skeleton(spec: &Spec) -> Skeleton {
    static KEPT: Memo<Spec, Skeleton, SKELETONS> = Memo::new();
    KEPT.get(spec, || lay_out(spec))
}

fn lay_out(spec: &Spec) -> Skeleton {
    let form = form_of(spec.seed);
    let town = spec.biome == Biome::Reservoir;
    let mut last = None;
    for attempt in 0..24u64 {
        let mut rng =
            Rng::new(spec.seed.wrapping_mul(0x9E37_79B9).wrapping_add(attempt) ^ 0x00DE_5E27);
        let generator = matches!(spec.centerpiece, Centerpiece::ShieldGenerator(..));
        let mut sk = match town {
            true => Skeleton::town(&mut rng, town::source_of(spec.seed)),
            false => Skeleton::desert(&mut rng, form, generator),
        };
        let bare = sk.greybox(&spec.sides);
        if !sk.place_bases(&mut rng, spec.sides.len(), &NavGrid::build(&bare)) {
            log::debug!(
                "desert seed {} attempt {attempt}: no fair spawns",
                spec.seed
            );
            continue;
        }
        if generator && !town {
            sk.place_doors(&mut Rng::new(spec.seed ^ 0xD00B_5EED));
        }
        match town {
            true => sk.place_houses(&mut rng),
            false => {
                sk.place_lots(&mut rng, form);
                sk.work_wash(&mut rng);
                sk.place_rocks(&mut rng);
            }
        }
        if town && sk.houses().count() < town::MIN_HOUSES {
            log::debug!("town seed {} attempt {attempt}: too few houses", spec.seed);
            last = last.or(Some(sk));
            continue;
        }
        let grey = sk.greybox(&spec.sides);
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
        .base_spots()
        .iter()
        .map(|&from| {
            let mut at = from;
            let mut len = 0.0;
            for p in nav.path(from, sk.landmarks[0].center) {
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
    map.skeleton = Some(std::sync::Arc::new(sk.clone()));
    map.factions = sides.to_vec();
    map.bases = Map::no_bases(sides);
    map.open_spots = Vec::new();
    map.spawn_yaw = None;
    map.hold_site = Some(sk.landmarks[0].center);
    map.hold_name = match sk.landmarks[0].kind {
        LandmarkKind::Needles => "NEEDLES",
        LandmarkKind::Crater => "CRATER",
        LandmarkKind::Arch => "ARCH",
        LandmarkKind::Dam => "DAM",
        LandmarkKind::Well => "WELL",
        LandmarkKind::Open => "FLATS",
        LandmarkKind::Generator | LandmarkKind::Great(_) => "GENERATOR",
    };
    map
}

/// The ground `spec`'s on.
fn ground_of(spec: &Spec) -> Ground {
    match spec.biome {
        Biome::Open(ground) => ground,
        _ => Ground::Desert,
    }
}

/// `spec`'s colors and its sky.
fn look(spec: &Spec) -> (Palette, crate::render::Atmosphere) {
    let mut rng = Rng::new(spec.seed ^ 0x5EED_D2E5);
    match ground_of(spec) {
        Ground::Desert => {
            let pal = palette(spec.seed);
            (pal, atmosphere(&mut rng, &pal))
        }
        ground => {
            let pal = land::palette(ground, spec.seed);
            (pal, land::atmosphere(ground, &mut rng))
        }
    }
}

/// `spec`'s desert undressed.
pub fn greybox(spec: &Spec) -> Map {
    let sk = skeleton(spec);
    let mut m = sk.greybox(&spec.sides);
    m.atmosphere = look(spec).1;
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
        stars: 0.0,
    }
}

/// Builds `spec`'s desert.
pub fn build(spec: &Spec) -> Map {
    build_with_claims(spec).0
}

/// Builds `spec`'s desert, and what each placeholder's filler put down.
pub fn build_with_claims(spec: &Spec) -> (Map, Vec<(dress::Claim, std::ops::Range<usize>)>) {
    let sk = skeleton(spec);
    let (pal, sky) = look(spec);
    let mut built = dressed(&sk, spec, &pal, sky, spec.seed);
    if spec.biome == Biome::Reservoir {
        // What its houses and boulders come down to isn't on the greybox: if
        // they've shut somebody in, they fall another way.
        for attempt in 1..6u64 {
            match super::check::playable(&built.0) {
                Ok(()) => break,
                Err(why) => log::debug!("town seed {} dressed: {why}", spec.seed),
            }
            built = dressed(&sk, spec, &pal, sky, spec.seed ^ (attempt << 40));
        }
    }
    land::growth(&mut built.0, &sk, ground_of(spec), &pal, spec.seed);
    built
}

/// `sk` dressed as a desert in `pal`, under `sky`, its pieces drawn from
/// `dice`.
pub(in crate::map::procgen) fn dressed(
    sk: &Skeleton,
    spec: &Spec,
    pal: &Palette,
    sky: crate::render::Atmosphere,
    dice: u64,
) -> (Map, Vec<(dress::Claim, std::ops::Range<usize>)>) {
    let mut m = blank(sk, &spec.sides);
    m.atmosphere = sky;
    m.rock_dust = Some(pal.sand);
    m.planet = Some((spec.seed, pal.planet));
    m.terrain = Some(dress::terrain(sk, *pal));
    let mut ground = sk.land.surface(sk.size, 1.0, |_| 0.0);
    ground.rock = pal.stone;
    m.solid = Some(ground.into());
    let claims = dress::dress(&mut m, sk, *pal, dice, spec.centerpiece);
    if sk.landmarks[0].kind == LandmarkKind::Generator || spec.biome == Biome::Reservoir {
        // Built flush, piece against piece, as a city is.
        m.separate_coplanar_faces();
    }
    (m, claims)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod town_tests;
