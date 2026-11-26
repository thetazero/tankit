//! The city biome: a city the war has reached, laid out round one great
//! building on a raised plaza in the middle. Avenues run from the plaza out
//! to the edges, each through a gate; between them, districts of blocks on
//! a loose grid of streets, each district's grid turned its own way, with a
//! square here and there. The city is laid out well past the arena's edge
//! (`SPRAWL`), so its streets run on through it; out past `NEAR` its
//! buildings are only seen. Then a court is cleared in a corner for each
//! side, and wherever a street crosses the edge something is heaped across
//! it: rubble at an avenue's end, barriers on a side street (they slow a
//! tank down more than they stop it; past the edge it's out of the fight
//! anyway). Whose city it is comes later (see `imperium`): nothing here is
//! anyone's architecture.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::{Vec2, Vec3};

use super::crossing;
use super::land::Land;
use super::skeleton::*;
use crate::map::OBox;
use crate::rng::Rng;

/// The sky over a city: a cold, hazy morning, the sun low, ash in the air,
/// from a different quarter each seed.
pub fn atmosphere(rng: &mut Rng) -> crate::render::Atmosphere {
    let az = rng.range(0.0, TAU);
    let up = rng.range(0.34, 0.5);
    let flat = (1.0 - up * up).sqrt();
    crate::render::Atmosphere {
        sun_dir: Vec3::new(az.cos() * flat, up, az.sin() * flat),
        sun_color: Vec3::new(2.05, 1.95, 1.8),
        sky_zenith: Vec3::new(0.28, 0.34, 0.44),
        sky_horizon: Vec3::new(0.6, 0.6, 0.58),
        ground_color: Vec3::new(0.26, 0.25, 0.24),
        fog_density: rng.range(0.0009, 0.0016),
        wind: Vec2::new(rng.range(-2.4, 2.4), rng.range(-2.4, 2.4)),
        blowing_sand: 0.25,
        dust: Vec3::new(0.62, 0.6, 0.57),
        settle: crate::render::SAND_SETTLE,
        ambient: 1.0,
        stars: 0.0,
    }
}

/// A city world from orbit: grey-brown land built over, darker basins,
/// slate seas, a lot of ash cloud.
pub const PLANET: crate::planet::Look = crate::planet::Look {
    land: Vec3::new(0.24, 0.22, 0.2),
    basin: Vec3::new(0.13, 0.12, 0.12),
    sea: Vec3::new(0.05, 0.07, 0.09),
    cloud: Vec3::new(0.44, 0.43, 0.42),
    cloudiness: 0.65,
};

/// Half-width of an avenue, kerb to kerb.
pub const AVENUE_HALF: f32 = 14.0;

/// How far past the arena's edge the city is laid out, m.
pub const SPRAWL: f32 = 130.0;

/// Past this far out from the edge a building is only seen, from afar
/// (`LotKind::Distant`).
pub const NEAR: f32 = 34.0;

/// How a city's laid out: its streets, and where the fight is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Form {
    /// One great building near the middle, avenues out to the edges.
    Radial,
    /// No great building: lanes winding in rings round a square.
    OldTown,
    /// Two great buildings off the middle, an avenue between them.
    Twin,
    /// One great building toward an edge, at the end of a long avenue.
    Spine,
}

/// How built up it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Density {
    Cramped,
    Middling,
    Open,
}

/// How big it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Extent {
    Small,
    Middling,
    Large,
}

/// What kind of city a seed names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Plan {
    pub form: Form,
    pub density: Density,
    pub extent: Extent,
}

impl Plan {
    /// The plan of the city `seed` names, for `sides` sides; one with a
    /// great building, if it's to hold a shield `generator`.
    pub fn of(seed: u64, sides: usize, generator: bool) -> Plan {
        // Each form in turn, so any four seeds in a row are four forms.
        let forms = [Form::Radial, Form::Twin, Form::OldTown, Form::Spine];
        let form = forms[(seed % 4) as usize];
        // More than two sides only fit round a middle.
        let form = match form {
            _ if sides > 2 => Form::Radial,
            Form::OldTown if generator => Form::Spine,
            f => f,
        };
        let mut rng = Rng::new(seed ^ 0x91A4_0C17);
        let density = match rng.index(3) {
            0 => Density::Cramped,
            1 => Density::Middling,
            _ => Density::Open,
        };
        let extent = match rng.index(3) {
            0 => Extent::Small,
            1 => Extent::Middling,
            _ => Extent::Large,
        };
        // Two great buildings don't fit in a small city, nor four courts;
        // an old town's never open.
        let extent = match extent {
            Extent::Small if form == Form::Twin || sides > 2 => Extent::Middling,
            e => e,
        };
        let density = match density {
            Density::Open if form == Form::OldTown => Density::Cramped,
            d => d,
        };
        Plan {
            form,
            density,
            extent,
        }
    }
}

impl Density {
    /// Half an avenue's width.
    fn avenue(self) -> f32 {
        match self {
            Density::Cramped => 8.5,
            Density::Middling => AVENUE_HALF,
            Density::Open => 17.0,
        }
    }

    /// How far apart streets branch off.
    fn blocks(self) -> f32 {
        match self {
            Density::Cramped => 40.0,
            Density::Middling => 44.0,
            Density::Open => 56.0,
        }
    }

    /// A lane's half-width.
    fn lane(self) -> (f32, f32) {
        match self {
            Density::Cramped => (3.9, 4.8),
            Density::Middling => (5.4, 7.5),
            Density::Open => (7.0, 9.5),
        }
    }

    /// How far back from the street a building may stand.
    fn setback(self) -> f32 {
        match self {
            Density::Cramped => 0.3,
            Density::Middling => 2.2,
            Density::Open => 6.0,
        }
    }

    /// How many buildings stand shoulder to shoulder between alleys, at
    /// most, and how wide an alley is.
    fn row(self) -> (i32, f32) {
        match self {
            Density::Cramped => (12, 5.6),
            Density::Middling => (6, 6.5),
            Density::Open => (2, 15.0),
        }
    }

    /// Behind the streets: how much to try to build there (tries for every
    /// 100 m²), how close together, and how often with room to drive round.
    fn infill(self) -> (f32, f32, f32) {
        match self {
            Density::Cramped => (12.0, 0.4, 0.05),
            Density::Middling => (1.4, 1.2, 0.5),
            Density::Open => (0.25, 5.6, 1.0),
        }
    }
}

/// Lays out a city for `sides` sides (2 to 4) on `plan`, a court in a
/// corner each.
pub fn skeleton(rng: &mut Rng, sides: usize, plan: &Plan) -> Skeleton {
    let size = 20.0
        * match plan.extent {
            Extent::Small => rng.int(13, 14),
            Extent::Middling => rng.int(15, 18),
            Extent::Large => rng.int(20, 23),
        } as f32;
    let small = plan.extent == Extent::Small;
    let mid = Vec2::splat(size * 0.5);

    // Which way round it lies: quarter turns, and which corners the courts
    // are in (the first two in the order are the sides', opposite across
    // the fight either way).
    let corners = [
        (Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)),
        (Vec2::new(size, 0.0), Vec2::new(-1.0, 1.0)),
        (Vec2::new(size, size), Vec2::new(-1.0, -1.0)),
        (Vec2::new(0.0, size), Vec2::new(1.0, -1.0)),
    ];
    let turn = rng.index(4);
    // (x across, y along) of the middle, turned.
    let turned = |x: f32, y: f32| {
        let (mut u, mut v) = (x, y);
        for _ in 0..turn {
            (u, v) = (-v, u);
        }
        mid + Vec2::new(u, v) * size
    };
    let edge_at = |p: Vec2, d: Vec2| {
        // Where the ray from `p` along `d` leaves the arena.
        let t = |p: f32, d: f32| {
            if d > 1e-4 {
                (size - p) / d
            } else if d < -1e-4 {
                -p / d
            } else {
                f32::MAX
            }
        };
        p + d * t(p.x, d.x).min(t(p.y, d.y))
    };

    // Where the great buildings stand.
    let j = |rng: &mut Rng| rng.range(-0.04, 0.04);
    let at: Vec<Vec2> = match plan.form {
        Form::Radial => vec![mid + Vec2::new(j(rng), j(rng)) * size],
        Form::OldTown => Vec::new(),
        // Along the diagonal across from the courts', so each is as far
        // from both.
        Form::Twin => {
            let d = rng.range(0.17, 0.2);
            vec![turned(-d, d), turned(d, -d)]
        }
        Form::Spine => vec![turned(0.0, rng.range(-0.22, -0.17))],
    };
    let heart = at.first().copied().unwrap_or(mid);

    // The avenues, as (from, to, whether it runs out through the edge).
    let mut ways: Vec<(Vec2, Vec2, bool)> = Vec::new();
    match plan.form {
        Form::Radial => {
            let skip = if rng.chance(0.25) {
                Some(rng.index(4))
            } else {
                None
            };
            for (k, &(start, along, _)) in self::sides(size).iter().enumerate() {
                if skip != Some(k) {
                    ways.push((heart, start + along * size * rng.range(0.4, 0.6), true));
                }
            }
        }
        Form::OldTown => {
            // Crooked streets out from the square.
            let n = rng.int(4, 5);
            let a0 = rng.range(0.0, TAU);
            for k in 0..n {
                let a = a0 + (k as f32 + rng.range(-0.25, 0.25)) / n as f32 * TAU;
                let d = Vec2::new(a.cos(), a.sin());
                ways.push((heart, edge_at(heart, d), true));
            }
        }
        Form::Twin => {
            ways.push((at[0], at[1], false));
            for &p in &at {
                let d = (p - mid).normalize();
                // Out to the two edges by its corner.
                for e in [Vec2::new(d.x.signum(), 0.0), Vec2::new(0.0, d.y.signum())] {
                    let e = Vec2::from_angle(rng.range(-0.4, 0.4)).rotate(e);
                    ways.push((p, edge_at(p, e), true));
                }
            }
        }
        Form::Spine => {
            let d = (turned(0.0, 0.5) - turned(0.0, -0.5)).normalize();
            let cross = Vec2::new(-d.y, d.x);
            ways.push((heart, edge_at(heart, d), true));
            // And a way across, through the building's plaza.
            for s in [-1.0f32, 1.0] {
                let c = Vec2::from_angle(rng.range(-0.45, 0.45)).rotate(cross * s);
                ways.push((heart, edge_at(heart, c), true));
            }
        }
    }
    let mut platforms = Vec::new();
    let which = rng.index(3);
    let mut landmarks = Vec::new();
    for &center in &at {
        let half = if small || plan.form != Form::Radial {
            rng.range(42.0, 48.0)
        } else {
            rng.range(54.0, 66.0)
        };
        platforms.push(Platform {
            center,
            half,
            round: rng.range(14.0, 26.0),
            height: 2.0,
            ramp: 14.0,
        });
        let doors: Vec<Vec2> = ways
            .iter()
            .filter_map(|&(a, b, _)| {
                if a == center {
                    Some((b - a).normalize())
                } else if b == center {
                    Some((a - b).normalize())
                } else {
                    None
                }
            })
            .collect();
        let radius = rng.range(21.0, 26.0);
        let fallen = rng.chance(0.85).then(|| {
            // Between two doors next to each other.
            let mut round: Vec<f32> = doors.iter().map(|d| d.y.atan2(d.x)).collect();
            round.sort_by(f32::total_cmp);
            let k = rng.index(round.len());
            let a0 = round[k];
            let mut a1 = round[(k + 1) % round.len()];
            if a1 <= a0 {
                a1 += TAU;
            }
            let a = (a0 + a1) * 0.5;
            Vec2::new(a.cos(), a.sin())
        });
        landmarks.push(Landmark {
            center,
            radius,
            precinct: (radius + 20.0).min(half - 8.0),
            doors,
            fallen,
            // No two the same in a city.
            kind: LandmarkKind::Great(((which + landmarks.len()) % 3) as u8),
            clear: 0.0,
            reach: (radius + 20.0).min(half - 8.0),
            facing: Vec2::X,
        });
    }

    // A court for each side, anywhere there's room for one: nothing says the
    // sides start as far from the fight as each other. (More than two keep
    // to the corners.)
    let court = if small { 42.0 } else { rng.range(42.0, 48.0) };
    let mut bases: Vec<Base> = Vec::new();
    if sides > 2 {
        bases.extend(corners.iter().take(sides).map(|&(corner, inward)| Base {
            corner: corner + inward * Vec2::new(rng.range(3.0, 16.0), rng.range(3.0, 16.0)),
            inward,
            size: court,
            kind: BaseKind::Court,
            camp: None,
        }));
    }
    let mut tries = 0;
    while bases.len() < sides && tries < 4000 {
        tries += 1;
        // Harder to please at first.
        let ease = 1.0 - tries as f32 / 5000.0;
        let m = Vec2::new(
            rng.range(court * 0.5 + 3.0, size - court * 0.5 - 3.0),
            rng.range(court * 0.5 + 3.0, size - court * 0.5 - 3.0),
        );
        // Open toward the fight.
        let inward = Vec2::new(
            if heart.x > m.x { 1.0 } else { -1.0 },
            if heart.y > m.y { 1.0 } else { -1.0 },
        );
        let off_plazas = platforms
            .iter()
            .map(|pl: &Platform| pl.sdf(m) - pl.ramp)
            .fold(m.distance(heart) - 34.0, f32::min);
        let reach = court * 0.75;
        let off_ways = ways.iter().all(|&(a, b, _)| {
            let s = Street {
                from: a,
                to: b,
                half: 0.0,
                clear: 0.0,
                rank: Rank::Lane,
                out: false,
            };
            s.distance(m) > (reach + 24.0) * ease
        });
        if off_plazas > reach + 10.0
            && off_ways
            && bases
                .iter()
                .all(|b| b.middle().distance(m) > size * 0.42 * ease)
        {
            bases.push(Base {
                corner: m - inward * court * 0.5,
                inward,
                size: court,
                kind: BaseKind::Court,
                camp: None,
            });
        }
    }

    let land = Land {
        platforms,
        ..Land::default()
    };
    let mut sk = Skeleton {
        size,
        land,
        streets: Vec::new(),
        squares: Vec::new(),
        landmarks,
        heart,
        bases,
        lots: Vec::new(),
        spans: Vec::new(),
        edges: Vec::new(),
        decos: Vec::new(),
        repair_spots: Vec::new(),
        crossings: Vec::new(),
        ..Skeleton::default()
    };
    // The land, its great shapes kept off the great buildings and courts.
    let avoid: Vec<(Vec2, f32)> = sk
        .land
        .platforms
        .iter()
        .map(|p| (p.center, p.half * 1.15 + p.ramp))
        .chain(sk.bases.iter().map(|b| (b.middle(), b.size * 0.75)))
        .chain([(heart, 30.0)])
        .collect();
    sk.land = Land {
        platforms: std::mem::take(&mut sk.land.platforms),
        ..Land::rolling(rng, size, &avoid)
    };

    if plan.form == Form::OldTown {
        // The square the fight's about, open in the middle.
        let radius = rng.range(22.0, 28.0);
        sk.squares.push(Square {
            center: heart,
            radius,
        });
        for k in 0..4 {
            let a = (k as f32 + 0.5) * FRAC_PI_2 + rng.range(-0.2, 0.2);
            let d = Vec2::new(a.cos(), a.sin());
            sk.decos.push(Deco {
                at: heart + d * (radius - 6.0),
                face: Vec2::new(-d.y, d.x),
                kind: DecoKind::Cover,
            });
        }
    }
    // The big things are down; the medium ones next, then the roads between
    // them, then everything else along the roads.
    civics(&mut sk, rng, plan);
    let mouths = roads(&mut sk, rng, plan, &ways);
    gates_and_lines(&mut sk, rng);
    plaza_dressing(&mut sk, rng);
    frontages(&mut sk, rng, plan);
    clear_courts(&mut sk, rng);
    cordon(&mut sk, rng, &mouths);

    // Repair drops: down each avenue, twice; between the avenues on each
    // plaza; and out among the streets, where nothing's built over them.
    let mut repairs: Vec<Vec2> = sk
        .avenues()
        .flat_map(|a| {
            let (t0, t1) = sk.off_plaza_span(a, 6.0);
            let at = [0.3, 0.75].map(|f| a.from + a.dir() * (t0 + (t1 - t0) * f));
            at.into_iter().filter(move |_| t1 - t0 > 40.0)
        })
        .collect();
    for lm in &sk.landmarks {
        for b in bisectors(lm) {
            repairs.push(lm.center + b * (lm.precinct + 6.0));
        }
    }
    let mut tries = 0;
    let want = repairs.len() + 6;
    while repairs.len() < want && tries < 2000 {
        tries += 1;
        let p = Vec2::new(rng.range(0.1, 0.9), rng.range(0.1, 0.9)) * size;
        let clear = sk.lots.iter().all(|l| l.footprint.distance(p) > 5.5)
            && sk.bases.iter().all(|b| !b.contains(p, 10.0))
            && sk
                .decos
                .iter()
                .all(|d| d.at.distance(p) > d.kind.reach() + 5.0)
            && sk.off_plazas(p) > 2.0
            && repairs.iter().all(|q| q.distance(p) > 36.0);
        if clear {
            repairs.push(p);
        }
    }
    sk.repair_spots = repairs;
    level(&mut sk);
    sk
}

/// Levels the ground under everything that stands on it, the biggest last:
/// a plaza's level wins over a house's plot by it.
fn level(sk: &mut Skeleton) {
    let mut land = sk.land.clone();
    for l in &sk.lots {
        land.level_box(&l.footprint, 0.6, 3.0);
    }
    for d in &sk.decos {
        land.level_round(d.at, d.kind.reach() * 0.8, 4.0);
    }
    for s in &sk.spans {
        land.level_round(s.at, s.opening + 7.0, 10.0);
    }
    for b in &sk.bases {
        let court = OBox {
            center: b.middle(),
            half: Vec2::splat(b.size * 0.5),
            ..OBox::default()
        };
        land.level_box(&court, 3.0, 18.0);
    }
    for pl in &sk.land.platforms {
        land.level_round(pl.center, pl.half * 1.2 + pl.ramp, 24.0);
    }
    if sk.land.platforms.is_empty() {
        land.level_round(sk.heart, 24.0, 16.0);
    }
    // Ready for the crossings: both ends level with a bridge's deck; a
    // tunnel's way dug down, the crest over it level with its roof.
    for x in &sk.crossings {
        let (d, yaw) = (x.dir(), x.dir().x.atan2(x.dir().y));
        let strip = |t0: f32, t1: f32, w: f32| OBox {
            center: x.from + d * (t0 + t1) * 0.5,
            yaw,
            half: Vec2::new(w, (t1 - t0) * 0.5),
            ..OBox::default()
        };
        match x.kind {
            crossing::Kind::Bridge => {
                land.level_at(strip(-9.0, 0.5, x.half + 1.0), 0.0, 7.0, x.top);
                let len = x.len();
                land.level_at(strip(len - 0.5, len + 9.0, x.half + 1.0), 0.0, 7.0, x.top);
            }
            crossing::Kind::Tunnel => {
                let (a, b) = x.roofed;
                land.level_at(strip(a, b, x.half + 11.0), 0.0, 5.0, x.top);
                // (Flat right across, and up again inside the walls.)
                land.level_at(strip(-5.0, x.len() + 5.0, x.half + 3.2), 0.0, 1.4, x.floor);
            }
        }
    }
    sk.land = land;
}

/// Where the avenues meet each edge (counter-clockwise from the south), as
/// (t from, t to) along it.
type Mouths = [Vec<(f32, f32)>; 4];

/// The edges, counter-clockwise from the south: (start, along, inward).
pub fn sides(size: f32) -> [(Vec2, Vec2, Vec2); 4] {
    [
        (Vec2::new(0.0, 0.0), Vec2::X, Vec2::Y),
        (Vec2::new(size, 0.0), Vec2::Y, -Vec2::X),
        (Vec2::new(size, size), -Vec2::X, -Vec2::Y),
        (Vec2::new(0.0, size), -Vec2::Y, Vec2::X),
    ]
}

/// Each court cleared out of the city: no lot on it or just round it, nor
/// across the way out of either gap in its barriers. Then buildings set
/// square to it wherever there's room round it (where the city's own grid
/// left a street or a gap): it stands among buildings, not in the open.
fn clear_courts(sk: &mut Skeleton, rng: &mut Rng) {
    let rect = |b: &Base, u0: f32, v0: f32, u1: f32, v1: f32| {
        let (a, z) = (b.at(u0, v0), b.at(u1, v1));
        OBox {
            center: (a + z) * 0.5,
            half: (z - a).abs() * 0.5,
            ..OBox::default()
        }
    };
    let mut keep_off = Vec::new();
    for b in &sk.bases {
        let s = b.size;
        // The court and its walls, then the two ways out.
        keep_off.push(rect(b, -1.0, -1.0, s + 1.4, s + 1.4));
        keep_off.push(rect(b, s, s * 0.3, s + 30.0, s * 0.7));
        keep_off.push(rect(b, s * 0.3, s, s * 0.7, s + 30.0));
    }
    sk.lots
        .retain(|l| !keep_off.iter().any(|o| l.footprint.overlaps(o, 3.0)));
    let bases = sk.bases.clone();
    for b in &bases {
        let s = b.size;
        // Round all four sides, a few meters off the wall: (how far along
        // it, along u (else v), behind the court (else in front)).
        let mut spots = Vec::new();
        let mut w = -12.0;
        while w < s + 12.0 {
            for side in [(true, false), (true, true), (false, false), (false, true)] {
                spots.push((w, side.0, side.1));
            }
            w += rng.range(15.0, 19.0);
        }
        for (w, along_u, behind) in spots {
            let half = Vec2::new(rng.range(5.5, 8.0), rng.range(5.5, 8.0));
            let off = |h: f32| {
                if behind {
                    -(h + 4.5)
                } else {
                    s + 1.4 + h + 4.5
                }
            };
            let (u, v) = if along_u {
                (w, off(half.y))
            } else {
                (off(half.x), w)
            };
            let lot = OBox {
                center: b.at(u, v),
                half,
                y0: 0.0,
                y1: 1.0,
                ..OBox::default()
            };
            if lot
                .corners()
                .iter()
                .chain([lot.center].iter())
                .any(|&q| sk.reserved(q))
                || keep_off.iter().any(|o| lot.overlaps(o, 2.5))
                || sk.lots.iter().any(|l| l.footprint.overlaps(&lot, 4.0))
            {
                continue;
            }
            // Its front on the court.
            let to = lot.local_point(b.middle());
            let front = if to.x.abs() > to.y.abs() {
                (0, to.x.signum())
            } else {
                (1, to.y.signum())
            };
            let kind = if rng.chance(0.25) {
                LotKind::Small
            } else {
                LotKind::Big
            };
            sk.lots.push(Lot {
                footprint: lot,
                front,
                kind,
            });
        }
    }
}

/// Where a street crosses the edge, something across it: rubble banked
/// across an avenue's mouth, barriers across a side street. Where the edge
/// runs a long way between buildings (down a street, across open ground),
/// barriers in clumps here and there, not a line all along it: they slow a
/// tank down more than they stop it.
fn cordon(sk: &mut Skeleton, rng: &mut Rng, mouths: &Mouths) {
    let size = sk.size;
    // Where a tank would run into a building on the edge line.
    let walled = |sk: &Skeleton, p: Vec2| {
        sk.lots.iter().any(|l| {
            matches!(l.kind, LotKind::Big | LotKind::Small) && {
                let q = l.footprint.local_point(p).abs() - l.footprint.half;
                q.max_element() < -0.5
            }
        })
    };
    for (k, &(start, along, inward)) in sides(size).iter().enumerate() {
        let edge = |from: f32, to: f32, kind: EdgeKind, off: f32| Edge {
            start: start + inward * off,
            along,
            inward,
            from,
            to,
            kind,
        };
        // The open stretches of the edge, (from, to), a meter at a time,
        // bar the avenues' mouths.
        let mut open: Vec<(f32, f32)> = Vec::new();
        let mut t = -4.0;
        while t <= size + 4.0 {
            let mouth = mouths[k].iter().any(|m| (m.0..=m.1).contains(&t));
            if !mouth && !walled(sk, start + along * t) {
                match open.last_mut() {
                    Some(run) if run.1 >= t - 1.01 => run.1 = t,
                    _ => open.push((t, t)),
                }
            }
            t += 1.0;
        }
        for &(m0, m1) in &mouths[k] {
            // Heaped at the sides, low barriers between: the road's seen
            // to carry on.
            let w = (m1 - m0) * 0.27;
            sk.edges.push(edge(m0, m0 + w, EdgeKind::Blockade, 0.0));
            sk.edges
                .push(edge(m0 + w, m1 - w, EdgeKind::Barricade, 0.0));
            sk.edges.push(edge(m1 - w, m1, EdgeKind::Blockade, 0.0));
        }
        for (from, to) in open {
            if to - from < 3.0 {
                continue;
            }
            if to - from <= 24.0 {
                // A street crossing: across it, into the buildings either
                // side a little.
                let off = rng.range(-2.0, 1.0);
                sk.edges
                    .push(edge(from - 1.5, to + 1.5, EdgeKind::Barricade, off));
                continue;
            }
            let mut t = from + rng.range(0.0, 8.0);
            while t < to {
                let end = (t + rng.range(6.0, 12.0)).min(to);
                let off = rng.range(-7.0, 2.0);
                sk.edges.push(edge(t, end, EdgeKind::Barricade, off));
                t = end + rng.range(14.0, 28.0);
            }
        }
    }
}

/// Directions halfway between each of `lm`'s doors and the next round it.
fn bisectors(lm: &Landmark) -> Vec<Vec2> {
    let mut angles: Vec<f32> = lm.doors.iter().map(|d| d.y.atan2(d.x)).collect();
    angles.sort_by(f32::total_cmp);
    (0..angles.len())
        .map(|k| {
            let a0 = angles[k];
            let mut a1 = angles[(k + 1) % angles.len()];
            if a1 <= a0 {
                a1 += TAU;
            }
            let a = (a0 + a1) * 0.5;
            Vec2::new(a.cos(), a.sin())
        })
        .collect()
}

/// Down each avenue: a gate over it partway out, and monuments and masts
/// lining it.
fn gates_and_lines(sk: &mut Skeleton, rng: &mut Rng) {
    let avenues: Vec<Street> = sk.avenues().copied().collect();
    let ruined = rng.index(avenues.len().max(1));
    for (k, a) in avenues.iter().enumerate() {
        let dir = a.dir();
        let side = Vec2::new(-dir.y, dir.x);
        // The stretch of it between the plazas' slopes and the edge.
        let (start, end) = sk.off_plaza_span(a, 4.0);
        let end = if a.out { end - 12.0 } else { end };
        if end - start < 4.0 {
            continue;
        }
        // One gate on a road, not on every bend of it.
        let gate = (start + end) * 0.5;
        let at = a.from + dir * gate;
        if end - start > 22.0 && sk.spans.iter().all(|s| s.at.distance(at) > 85.0) {
            sk.spans.push(Span {
                at,
                along: dir,
                opening: (a.half - 3.0).min(6.5),
                ruined: k == ruined && rng.chance(0.75),
            });
        }
        let gates: Vec<Vec2> = sk.spans.iter().map(|s| s.at).collect();
        let spacing = rng.range(13.0, 17.0);
        let mut d = start;
        let mut i = 0;
        while d < end {
            for s in [-1.0f32, 1.0] {
                let p = a.from + dir * d + side * s * (a.half - 1.5);
                if gates.iter().any(|g| g.distance(a.from + dir * d) < 11.0)
                    || sk.decos.iter().any(|o| o.at.distance(p) < 8.0)
                    || sk
                        .streets
                        .iter()
                        .any(|o| o.rank == Rank::Lane && o.distance(p) < o.half + 1.5)
                {
                    continue;
                }
                let kind = if i % 2 == 0 {
                    DecoKind::Monument
                } else {
                    DecoKind::Mast
                };
                let face = if kind == DecoKind::Monument {
                    -side * s
                } else {
                    dir
                };
                sk.decos.push(Deco { at: p, face, kind });
            }
            d += spacing;
            i += 1;
        }
    }
}

/// On the plaza and round it: a fountain between each pair of avenues, and
/// low cover scattered on the open ground round its foot.
fn plaza_dressing(sk: &mut Skeleton, rng: &mut Rng) {
    let mut fonts = Vec::new();
    for (lm, plaza) in sk.landmarks.iter().zip(&sk.land.platforms) {
        for b in bisectors(lm) {
            // On the top, well in from its edge, outside the precinct.
            let mut d = lm.precinct + 8.0;
            while plaza.sdf(plaza.center + b * (d + 1.0)) < -10.0 {
                d += 1.0;
            }
            fonts.push(Deco {
                at: plaza.center + b * d,
                face: b,
                kind: DecoKind::Fountain,
            });
        }
    }
    sk.decos.extend(fonts);
    if sk.land.platforms.is_empty() {
        return;
    }
    let c = sk.heart;
    let mut placed: Vec<Vec2> = Vec::new();
    let mut tries = 0;
    while placed.len() < 120 && tries < 6000 {
        tries += 1;
        let p = Vec2::new(rng.range(0.0, sk.size), rng.range(0.0, sk.size));
        let s = sk.off_plazas(p);
        if !(3.0..24.0).contains(&s)
            || sk.streets.iter().any(|st| st.distance(p) < st.half + 3.0)
            || sk.bases.iter().any(|b| b.contains(p, 6.0))
            || placed.iter().any(|q| q.distance(p) < 8.5)
        {
            continue;
        }
        placed.push(p);
        let d = p - c;
        let a =
            d.y.atan2(d.x) + if rng.chance(0.5) { 0.0 } else { FRAC_PI_2 } + rng.range(-0.2, 0.2);
        sk.decos.push(Deco {
            at: p,
            face: Vec2::new(a.cos(), a.sin()),
            kind: DecoKind::Cover,
        });
    }
}

/// The medium structures, spread out between the great ones.
fn civics(sk: &mut Skeleton, rng: &mut Rng, plan: &Plan) {
    let want = match plan.extent {
        Extent::Small => rng.int(2, 4),
        Extent::Middling => rng.int(4, 6),
        Extent::Large => rng.int(6, 9),
    };
    let mut tries = 0;
    let mut have = 0;
    while have < want && tries < 600 {
        tries += 1;
        let kind = match rng.index(3) {
            0 => CivicKind::Courtyard,
            1 => CivicKind::Market,
            _ => CivicKind::Terrace,
        };
        let half = match kind {
            CivicKind::Courtyard => Vec2::new(rng.range(14.0, 19.0), rng.range(14.0, 19.0)),
            CivicKind::Market => Vec2::new(rng.range(9.0, 11.0), rng.range(14.0, 20.0)),
            CivicKind::Terrace => Vec2::new(rng.range(9.0, 12.0), rng.range(15.0, 19.0)),
        };
        let p = Vec2::new(rng.range(0.1, 0.9), rng.range(0.1, 0.9)) * sk.size;
        let lot = OBox {
            center: p,
            yaw: rng.range(0.0, TAU),
            half,
            y0: 0.0,
            y1: 1.0,
            ..OBox::default()
        };
        let r = half.length();
        let clear = sk.land.cuts().iter().all(|c| {
            let (t, off) = c.local(p);
            !(c.floor - r..c.floor + c.bank(t) + r * 0.7).contains(&off.abs())
                && off.abs() > c.floor
        }) && sk.off_plazas(p) > r + 8.0
            && sk
                .squares
                .iter()
                .all(|s| s.center.distance(p) > s.radius + r + 6.0)
            && sk.bases.iter().all(|b| !b.contains(p, r + 14.0))
            && sk
                .lots
                .iter()
                .all(|l| l.footprint.center.distance(p) > l.footprint.half.length() + r + 22.0);
        if clear {
            have += 1;
            sk.lots.push(Lot {
                footprint: lot,
                front: (1, 1.0),
                kind: LotKind::Civic(kind),
            });
        }
    }
}

/// Does the way along `pts`, `half` wide, run through something standing?
fn fouled(sk: &Skeleton, pts: &[Vec2], half: f32, own: &[Vec2]) -> bool {
    pts.windows(2).any(|w| {
        let n = (w[0].distance(w[1]) / 2.0).ceil().max(1.0) as i32;
        (0..=n).any(|k| {
            let p = w[0].lerp(w[1], k as f32 / n as f32);
            sk.bases.iter().any(|b| b.contains(p, half + 3.0))
                || sk.lots.iter().any(|l| l.footprint.distance(p) < half + 1.0)
                || sk
                    .land
                    .platforms
                    .iter()
                    .any(|pl| !own.contains(&pl.center) && pl.sdf(p) < pl.ramp * 0.5 + half)
        })
    })
}

/// A crooked way from `a` to `b`, leaving along `lead` m of straight at
/// each end that's asked for: bent about, its corners cut off.
fn crooked(rng: &mut Rng, a: Vec2, b: Vec2, lead: (f32, f32), bend: f32) -> Vec<Vec2> {
    let d = (b - a).normalize_or(Vec2::X);
    let side = Vec2::new(-d.y, d.x);
    let (a1, b1) = (a + d * lead.0, b - d * lead.1);
    let len = a1.distance(b1);
    let n = (len / 45.0).round().max(1.0) as i32;
    let mut mid = vec![a1];
    let sway = rng.range(-bend, bend) * len;
    for k in 1..n {
        let u = (k as f32 + rng.range(-0.2, 0.2)) / n as f32;
        let off = sway * (u * std::f32::consts::PI).sin() + rng.range(-0.5, 0.5) * bend * 45.0;
        mid.push(a1 + d * len * u + side * off);
    }
    mid.push(b1);
    // Corners cut, once.
    let mut pts = vec![a];
    for w in mid.windows(3) {
        if pts.len() == 1 {
            pts.push(w[0]);
        }
        pts.push(w[1].lerp(w[0], 0.3));
        pts.push(w[1].lerp(w[2], 0.3));
    }
    if mid.len() < 3 {
        pts.push(a1);
    }
    pts.push(b1);
    pts.push(b);
    pts.dedup_by(|p, q| p.distance(*q) < 1.0);
    pts
}

/// The roads: the plan's `ways` first, then a way to every medium structure
/// and out of every court, then side streets off them all. Returns where
/// the great ones cross the edge.
fn roads(sk: &mut Skeleton, rng: &mut Rng, plan: &Plan, ways: &[(Vec2, Vec2, bool)]) -> Mouths {
    let mut mouths: Mouths = Default::default();
    let size = sk.size;
    let town = plan.form == Form::OldTown;
    let lay = |sk: &mut Skeleton, pts: &[Vec2], half: f32, rank: Rank, out: bool| {
        let n = pts.len() - 1;
        for (k, w) in pts.windows(2).enumerate() {
            sk.streets.push(Street {
                from: w[0],
                to: w[1],
                half,
                clear: (half - 4.0).clamp(2.0, 6.0),
                rank,
                out: out && k + 1 == n,
            });
        }
    };
    for &(from, to, out) in ways {
        let half = if town {
            let (a, b) = plan.density.lane();
            rng.range(a, b) + 0.8
        } else {
            plan.density.avenue() * rng.range(0.85, 1.1)
        };
        let reach = |p: Vec2| {
            sk.land
                .platforms
                .iter()
                .find(|pl| pl.center == p)
                .map_or(4.0, |pl| pl.half * 1.15 + pl.ramp)
        };
        let lead = (reach(from), if out { 14.0 } else { reach(to) });
        let mut best = None;
        for t in 0..40 {
            let bend = if town { 0.28 } else { 0.12 } + t as f32 * 0.006;
            let pts = crooked(rng, from, to, lead, bend);
            if !fouled(sk, &pts, half, &[from, to]) {
                best = Some(pts);
                break;
            }
        }
        // No way round: whatever stands in it goes.
        let pts = best.unwrap_or_else(|| {
            let pts = vec![from, to];
            let street = Street {
                from,
                to,
                half,
                clear: 2.0,
                rank: Rank::Lane,
                out: false,
            };
            sk.lots
                .retain(|l| street.distance(l.footprint.center) > half + l.footprint.half.length());
            pts
        });
        let rank = if town { Rank::Lane } else { Rank::Avenue };
        lay(sk, &pts, half, rank, out);
        if !out {
            continue;
        }
        let d = (to - pts[pts.len() - 2]).normalize_or(Vec2::X);
        lay(
            sk,
            &[to, to + d * (SPRAWL + 160.0)],
            half,
            Rank::Beyond,
            false,
        );
        for (k, &(start, along, inward)) in self::sides(size).iter().enumerate() {
            if !town && (to - start).dot(inward).abs() < 0.5 {
                let t = (to - start).dot(along);
                // As wide as it crosses, aslant.
                let w = (half + 1.5) / d.dot(inward).abs().max(0.4);
                mouths[k].push((t - w, t + w));
                break;
            }
        }
    }

    valley_roads(sk, rng, plan);
    crossings(sk);

    // To the nearest road so far from every medium structure's ends, and
    // from each way out of every court.
    let nearest = |sk: &Skeleton, p: Vec2| {
        sk.streets
            .iter()
            .filter(|s| !matches!(s.rank, Rank::Beyond | Rank::Crossing))
            .map(|s| {
                let d = s.to - s.from;
                let t = ((p - s.from).dot(d) / d.length_squared()).clamp(0.0, 1.0);
                s.from + d * t
            })
            .filter(|q| sk.off_plazas(*q) > -4.0 || sk.land.platforms.is_empty())
            .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
    };
    let mut doors: Vec<Vec2> = Vec::new();
    for b in &sk.bases {
        doors.push(b.at(b.size + 13.0, b.size * 0.5));
        doors.push(b.at(b.size * 0.5, b.size + 13.0));
    }
    for l in &sk.lots {
        let (_, az) = l.footprint.axes();
        for e in [-1.0f32, 1.0] {
            doors.push(l.footprint.center + az * e * (l.footprint.half.y + 9.0));
        }
    }
    for p in doors {
        let Some(q) = nearest(sk, p) else { continue };
        let (a, b) = plan.density.lane();
        let half = rng.range(a, b);
        if q.distance(p) < half + 4.0 || fouled(sk, &[p, p], half, &[]) {
            continue;
        }
        for t in 0..20 {
            let pts = crooked(rng, p, q, (3.0, 3.0), 0.1 + t as f32 * 0.015);
            if !fouled(sk, &pts, half, &[]) {
                lay(sk, &pts, half, Rank::Lane, false);
                break;
            }
        }
    }

    // Side streets: off every road, uneven distances apart, at uneven
    // angles, till they meet another or give out.
    let block = plan.density.blocks();
    let mut k = 0;
    let most = 480;
    while k < sk.streets.len() && sk.streets.len() < most {
        let s = sk.streets[k];
        k += 1;
        if s.rank == Rank::Crossing {
            continue;
        }
        let d = s.dir();
        // (Roads are short bends: each takes up where a run of blocks is.)
        let mut t = rng.range(0.0, 1.0) * block;
        while t < s.len() {
            let p = s.from + d * t;
            t += block * rng.range(0.6, 1.5);
            // (Out past the edge what's between the roads is only seen.)
            if p.min_element() < -30.0 || p.max_element() > size + 30.0 {
                continue;
            }
            // Not by where others meet: their paving would run together.
            let crowded = sk.streets.iter().any(|o| {
                o.rank != Rank::Beyond
                    && o.dir().dot(d).abs() < 0.9
                    && o.distance(p) < o.half + s.half + 7.0
            });
            for e in [-1.0f32, 1.0] {
                if crowded || rng.chance(0.3) {
                    continue;
                }
                let mut out =
                    Vec2::from_angle(rng.range(-0.45, 0.45)).rotate(Vec2::new(-d.y, d.x) * e);
                let (a, b) = plan.density.lane();
                let half = rng.range(a, b);
                let want = block * rng.range(1.0, 3.2);
                let mut pts = vec![p];
                let mut at = p;
                let mut len = 0.0;
                let mut met = false;
                let mut near = false;
                while len < want && !met {
                    let step = rng.range(13.0, 22.0);
                    let mut q = at + out * step;
                    // No further than the first road it comes to.
                    let mut go = 2.0;
                    while go < step {
                        let r = at + out * go;
                        let hit = sk.streets.iter().any(|o| {
                            (len > 0.0 || o.distance(p) > o.half + 2.0) && o.distance(r) < o.half
                        });
                        if hit {
                            q = r;
                            met = true;
                            break;
                        }
                        go += 1.0;
                    }
                    if q.min_element() < -SPRAWL
                        || q.max_element() > size + SPRAWL
                        || fouled(sk, &[at, q], half, &[])
                    {
                        break;
                    }
                    for o in &sk.streets {
                        let gap = o.distance(q);
                        // (Not the one it left, nor that one's next bend.)
                        near |= o.distance(p) > block * 0.55
                            && gap > o.half + half
                            && gap < block * 0.5
                            && o.dir().dot(out).abs() > 0.75;
                    }
                    if near && !met {
                        break;
                    }
                    pts.push(q);
                    len += step;
                    at = q;
                    out = Vec2::from_angle(rng.range(-0.3, 0.3)).rotate(out);
                }
                if len > 20.0 {
                    lay(sk, &pts, half, Rank::Lane, false);
                }
            }
        }
    }
    mouths
}

/// A road along each valley's floor or ridge's crest, in from one edge and
/// out at another (now and then there's none).
fn valley_roads(sk: &mut Skeleton, rng: &mut Rng, plan: &Plan) {
    let size = sk.size;
    let inside = |p: Vec2| p.min_element() > 0.0 && p.max_element() < size;
    for c in sk.land.cuts() {
        if rng.chance(0.2) {
            continue;
        }
        let half = rng.range(plan.density.lane().0, plan.density.lane().1 + 1.0);
        let half = half.min(c.floor - 2.5);
        let pts: Vec<Vec2> = (-40..=40)
            .map(|k| c.point(k as f32 * 18.0))
            .filter(|&p| p.min_element() > -SPRAWL - 80.0 && p.max_element() < size + SPRAWL + 80.0)
            .collect();
        for w in pts.windows(2) {
            let rank = if inside(w[0]) && inside(w[1]) {
                Rank::Lane
            } else {
                Rank::Beyond
            };
            sk.streets.push(Street {
                from: w[0],
                to: w[1],
                half,
                clear: 2.0,
                rank,
                out: false,
            });
        }
    }
}

/// Bridges where the plan's roads cross a valley, tunnels where they go
/// through a ridge: straight from rim to rim, or foot to foot.
fn crossings(sk: &mut Skeleton) {
    let cuts = sk.land.cuts();
    let old = std::mem::take(&mut sk.streets);
    // Roads whole, then cut where they cross.
    let mut roads: Vec<(Street, Vec<Vec2>)> = Vec::new();
    for s in old {
        match roads.last_mut() {
            Some((r, pts))
                if r.half == s.half && r.rank == s.rank && pts.last() == Some(&s.from) =>
            {
                pts.push(s.to);
                r.out = s.out;
            }
            _ => roads.push((s, vec![s.from, s.to])),
        }
    }
    for (road, pts) in roads {
        // Fine points along it.
        let mut fine = vec![pts[0]];
        for w in pts.windows(2) {
            let n = (w[0].distance(w[1]) / 3.0).ceil().max(1.0) as i32;
            fine.extend((1..=n).map(|k| w[0].lerp(w[1], k as f32 / n as f32)));
        }
        let mut spans: Vec<(usize, usize, crossing::Crossing)> = Vec::new();
        if road.rank != Rank::Beyond {
            for c in &cuts {
                let on = |p: Vec2| c.depth(p) > 0.03;
                let mut k = 0;
                while k < fine.len() {
                    if !on(fine[k]) {
                        k += 1;
                        continue;
                    }
                    let a = k;
                    while k < fine.len() && on(fine[k]) {
                        k += 1;
                    }
                    let (a, b) = (a.saturating_sub(1), k.min(fine.len() - 1));
                    if let Some(x) = crossing_at(sk, c, fine[a], fine[b], road.half)
                        && a > 0
                        && k < fine.len()
                        && spans.iter().all(|&(p, q, _)| b < p || a > q)
                        && sk
                            .crossings
                            .iter()
                            .all(|o| o.distance((fine[a] + fine[b]) * 0.5) > 34.0)
                    {
                        spans.push((a, b, x));
                    }
                }
            }
        }
        spans.sort_by_key(|s| s.0);
        let lay = |sk: &mut Skeleton, from: usize, to: usize, last: bool| {
            // Back to its own bends, between two of the fine points.
            let mut line = vec![fine[from]];
            for &p in &fine[from + 1..to.max(from + 1)] {
                if pts.contains(&p) {
                    line.push(p);
                }
            }
            line.push(fine[to]);
            let n = line.len() - 1;
            for (k, w) in line.windows(2).enumerate() {
                if w[0].distance(w[1]) > 0.5 {
                    sk.streets.push(Street {
                        from: w[0],
                        to: w[1],
                        out: road.out && last && k + 1 == n,
                        ..road
                    });
                }
            }
        };
        let mut from = 0;
        for &(a, b, x) in &spans {
            if a > from {
                lay(sk, from, a, false);
            }
            sk.streets.push(Street {
                from: x.from,
                to: x.to,
                // (With its walls.)
                half: x.half
                    + match x.kind {
                        crossing::Kind::Bridge => 0.5,
                        crossing::Kind::Tunnel => crossing::ROOF,
                    },
                clear: 2.0,
                rank: Rank::Crossing,
                out: false,
            });
            sk.crossings.push(x);
            from = b;
        }
        if from + 1 < fine.len() {
            lay(sk, from, fine.len() - 1, true);
        }
    }
    // A tunnel's approaches' walls, back from its mouths as far as they
    // keep out of the other roads.
    let streets = &sk.streets;
    for x in &mut sk.crossings {
        let (d, side) = (x.dir(), Vec2::new(-x.dir().y, x.dir().x));
        let fouls = |t: f32| {
            [0.3, crossing::WALL * 0.5, crossing::WALL + 0.3]
                .iter()
                .any(|w| {
                    [-1.0f32, 1.0].iter().any(|e| {
                        let p = x.from + d * t + side * e * (x.half + w);
                        streets
                            .iter()
                            .any(|s| s.rank != Rank::Crossing && s.distance(p) < s.clear + 1.0)
                    })
                })
        };
        let (a, b) = x.roofed;
        let (mut t0, mut t1) = (a, b);
        while t0 > 0.0 && !fouls(t0 - 1.0) {
            t0 = (t0 - 1.0).max(0.0);
        }
        while t1 < x.len() && !fouls(t1 + 1.0) {
            t1 = (t1 + 1.0).min(x.len());
        }
        x.walled = (t0, t1);
    }
}

/// The crossing of `c` from `a` to `b` by a road `half` wide, if one fits.
fn crossing_at(
    sk: &Skeleton,
    c: &super::land::Cut,
    a: Vec2,
    b: Vec2,
    half: f32,
) -> Option<crossing::Crossing> {
    let len = a.distance(b);
    if !(26.0..120.0).contains(&len) || c.local(a).1 * c.local(b).1 > 0.0 {
        return None;
    }
    let inside = |p: Vec2| p.min_element() > 14.0 && p.max_element() < sk.size - 14.0;
    if !inside(a) || !inside(b) || sk.off_plazas(a) < 6.0 || sk.off_plazas(b) < 6.0 {
        return None;
    }
    let r = &sk.land;
    let (ha, hb) = (r.swell(a), r.swell(b));
    let mid = r.swell((a + b) * 0.5);
    let half = half.min(8.5);
    let end = (ha + hb) * 0.5;
    if (ha - hb).abs() > 4.0 {
        return None;
    }
    if c.height < 0.0 {
        // Room to drive under it.
        (end - mid > 5.0).then_some(crossing::Crossing {
            kind: crossing::Kind::Bridge,
            from: a,
            to: b,
            half,
            top: end,
            floor: 0.0,
            roofed: (0.0, 0.0),
            walled: (0.0, len),
        })
    } else {
        // Roofed where the crest's high enough to be.
        let high = |t: f32| r.swell(a + (b - a) * t) > end + 5.6;
        let ts: Vec<f32> = (0..=40)
            .map(|k| k as f32 / 40.0)
            .filter(|&t| high(t))
            .collect();
        let (t0, t1) = (*ts.first()?, *ts.last()?);
        let top = (0..=8)
            .map(|k| r.swell(a + (b - a) * (t0 + (t1 - t0) * k as f32 / 8.0)))
            .fold(f32::MAX, f32::min)
            .min(end + 9.0);
        ((t1 - t0) * len > 10.0).then_some(crossing::Crossing {
            kind: crossing::Kind::Tunnel,
            from: a,
            to: b,
            half: half.min(7.0),
            top,
            floor: end,
            roofed: (t0 * len, t1 * len),
            walled: (0.0, len),
        })
    }
}

/// Buildings along every road, facing it: shoulder to shoulder, an alley
/// every few; then, behind them, a scatter of smaller ones.
fn frontages(sk: &mut Skeleton, rng: &mut Rng, plan: &Plan) {
    let size = sk.size;
    let raised = rng.range(0.12, 0.28);
    let rubble = rng.range(0.05, 0.14);
    let beyond = |q: Vec2| q.min_element() < -SPRAWL || q.max_element() > size + SPRAWL;
    let fits = |sk: &Skeleton, lot: &OBox, margin: f32| {
        // Cheapest first: what's nearby, then the corners.
        let r = lot.half.length();
        if beyond(lot.center)
            || sk.reserved(lot.center)
            || sk.lots.iter().any(|l| {
                let m = if matches!(l.kind, LotKind::Civic(_)) {
                    5.0
                } else {
                    margin
                };
                let f = &l.footprint;
                f.center.distance(lot.center) < r + f.half.length() + m && f.overlaps(lot, m)
            })
        {
            return false;
        }
        let (ax, az) = lot.axes();
        [-1.0f32, 0.0, 1.0].into_iter().all(|u| {
            [-1.0f32, 0.0, 1.0].into_iter().all(|v| {
                let q = lot.center + ax * u * lot.half.x + az * v * lot.half.y;
                !sk.reserved(q) && !beyond(q)
            })
        })
    };
    let kind_at = |rng: &mut Rng, p: Vec2| {
        let out = (-p).max(p - Vec2::splat(size)).max_element();
        let r = rng.f32();
        if out > NEAR {
            LotKind::Distant
        } else if r < raised {
            LotKind::Raised
        } else if r < 1.0 - rubble {
            LotKind::Big
        } else {
            LotKind::Rubble
        }
    };
    let (row, alley) = plan.density.row();
    // Every road whole, bends and all (its stretches are laid end to end),
    // the greatest first: they get the best of it.
    let mut chains: Vec<(f32, Vec<Vec2>)> = Vec::new();
    for s in sk.streets.iter().filter(|s| s.rank != Rank::Crossing) {
        match chains.last_mut() {
            Some((half, pts)) if *half == s.half && pts.last() == Some(&s.from) => pts.push(s.to),
            _ => chains.push((s.half, vec![s.from, s.to])),
        }
    }
    chains.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (road, pts) in chains {
        // The point `t` along it, and its heading there (over a house's
        // width, so a house on a bend faces it squarely).
        let total: f32 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        let at = |t: f32| {
            let mut left = t.clamp(0.0, total);
            for w in pts.windows(2) {
                let l = w[0].distance(w[1]);
                if left <= l {
                    return w[0].lerp(w[1], left / l.max(1e-3));
                }
                left -= l;
            }
            *pts.last().unwrap()
        };
        for e in [-1.0f32, 1.0] {
            let mut t = rng.range(0.0, 4.0);
            let mut run = 0;
            let mut upto = rng.int(2, row);
            while t < total {
                let mut half = Vec2::new(rng.range(4.2, 8.0), rng.range(5.5, 9.5));
                let mut placed = false;
                for size_down in 0..3 {
                    match size_down {
                        0 => {}
                        1 => half = Vec2::new(rng.range(3.4, 4.6), half.y),
                        _ => half = Vec2::new(rng.range(3.0, 4.0), rng.range(3.4, 4.8)),
                    }
                    let mid = at(t + half.x);
                    let d = (at(t + half.x + 4.0) - at(t + half.x - 4.0)).normalize_or(Vec2::X);
                    let side = Vec2::new(-d.y, d.x);
                    // A far block's details need room on its faces.
                    let far = (-mid).max(mid - Vec2::splat(size)).max_element() > NEAR - 24.0;
                    if far {
                        half = half.max(Vec2::splat(6.5));
                    }
                    let set = rng.range(0.0, plan.density.setback());
                    let mut lot = OBox {
                        center: mid + side * e * (road + 0.9 + set + half.y),
                        yaw: -d.y.atan2(d.x) + rng.range(-0.05, 0.05),
                        half,
                        y0: 0.0,
                        y1: 1.0,
                        ..OBox::default()
                    };
                    // On the inside of a bend it stands a little further back.
                    let margin = if far { 1.6 } else { 0.4 };
                    let mut ok = fits(sk, &lot, margin);
                    for _ in 0..3 {
                        if ok {
                            break;
                        }
                        lot.center += side * e * 0.8;
                        ok = fits(sk, &lot, margin);
                    }
                    // Few on the banks: steep ground's mostly left open.
                    if !ok || (sk.land.steep(lot.center) > 0.3 && rng.chance(0.65)) {
                        continue;
                    }
                    let to = lot.local_point(mid);
                    let kind = match kind_at(rng, lot.center) {
                        LotKind::Distant => LotKind::Distant,
                        _ if size_down == 2 => LotKind::Small,
                        k => k,
                    };
                    sk.lots.push(Lot {
                        footprint: lot,
                        front: (1, to.y.signum()),
                        kind,
                    });
                    placed = true;
                    break;
                }
                if placed {
                    t += half.x * 2.0 + rng.range(0.5, 1.0);
                    run += 1;
                    if run >= upto {
                        t += alley * rng.range(0.9, 1.3);
                        run = 0;
                        upto = rng.int(2, row);
                    }
                } else {
                    t += 1.0;
                }
            }
        }
    }
    // Behind: whatever there's room for, square to the nearest street, so
    // a block's backs line up with its fronts.
    let (tries, close, loose) = plan.density.infill();
    let span = size + 2.0 * SPRAWL;
    for _ in 0..(span * span / 100.0 * tries) as i32 {
        let p = Vec2::new(rng.range(0.0, span), rng.range(0.0, span)) - Vec2::splat(SPRAWL);
        let far = (-p).max(p - Vec2::splat(size)).max_element() > NEAR - 24.0;
        // (Out past the edge it's only seen: no need to pack it.)
        if far && !rng.chance((1.5 / tries).min(1.0)) {
            continue;
        }
        let near = sk
            .streets
            .iter()
            .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)));
        let turn = near.map_or(0.0, |s| {
            let d = s.dir();
            -d.y.atan2(d.x)
        });
        let lot = OBox {
            center: p,
            yaw: turn + rng.range(-0.08, 0.08),
            half: if far {
                Vec2::new(rng.range(6.5, 8.5), rng.range(6.5, 8.5))
            } else {
                Vec2::new(rng.range(3.2, 7.0), rng.range(3.2, 7.0))
            },
            y0: 0.0,
            y1: 1.0,
            ..OBox::default()
        };
        let margin = if far {
            2.0
        } else if rng.chance(loose) {
            5.6
        } else {
            close
        };
        if !fits(sk, &lot, margin) || (sk.land.steep(p) > 0.3 && rng.chance(0.75)) {
            continue;
        }
        let kind = match kind_at(rng, p) {
            LotKind::Distant => LotKind::Distant,
            LotKind::Rubble => LotKind::Rubble,
            _ if lot.half.min_element() < 4.6 => LotKind::Small,
            k => k,
        };
        sk.lots.push(Lot {
            footprint: lot,
            front: (1, 1.0),
            kind,
        });
    }
}
