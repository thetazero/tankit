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
            Density::Cramped => 11.0,
            Density::Middling => AVENUE_HALF,
            Density::Open => 17.0,
        }
    }

    /// From one lot to the next, at least and at most.
    fn step(self) -> (f32, f32) {
        match self {
            Density::Cramped => (23.5, 25.5),
            Density::Middling => (27.0, 31.0),
            Density::Open => (35.0, 41.0),
        }
    }

    /// How many lots are left empty.
    fn empty(self) -> f32 {
        match self {
            Density::Cramped => 0.02,
            Density::Middling => 0.07,
            Density::Open => 0.16,
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
    let avenue = plan.density.avenue();

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
        Form::OldTown => {}
        Form::Twin => {
            ways.push((at[0], at[1], false));
            for &p in &at {
                let d = (p - mid).normalize();
                // Out to the two edges by its corner.
                for e in [Vec2::new(d.x.signum(), 0.0), Vec2::new(0.0, d.y.signum())] {
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
                ways.push((heart, edge_at(heart, cross * s), true));
            }
        }
    }
    let mut mouths: Mouths = Default::default();
    let mut streets = Vec::new();
    for &(from, to, out) in &ways {
        streets.push(Street {
            from,
            to,
            half: avenue,
            clear: 6.0,
            rank: Rank::Avenue,
            out,
        });
        if !out {
            continue;
        }
        for (k, &(start, along, inward)) in self::sides(size).iter().enumerate() {
            if (to - start).dot(inward).abs() < 0.5 {
                let t = (to - start).dot(along);
                mouths[k].push((t - avenue - 3.0, t + avenue + 3.0));
                break;
            }
        }
    }

    let mut platforms = Vec::new();
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
        });
    }

    // A court in a corner for each side: opposite corners, as far from the
    // fight either way; a spine's at its far end.
    let order: Vec<usize> = match (plan.form, sides) {
        (Form::Spine, _) => {
            let far = [turned(-0.5, 0.5), turned(0.5, 0.5)];
            far.iter()
                .map(|f| {
                    (0..4)
                        .min_by(|&a, &b| {
                            corners[a]
                                .0
                                .distance(*f)
                                .total_cmp(&corners[b].0.distance(*f))
                        })
                        .unwrap()
                })
                .collect()
        }
        (Form::Twin, _) => {
            let far = turned(0.5, 0.5);
            let k = (0..4)
                .min_by(|&a, &b| {
                    corners[a]
                        .0
                        .distance(far)
                        .total_cmp(&corners[b].0.distance(far))
                })
                .unwrap();
            vec![k, (k + 2) % 4]
        }
        (_, 0..=2) => vec![turn, (turn + 2) % 4],
        (_, 3) => vec![0, 1, 2],
        _ => vec![0, 1, 2, 3],
    };
    let court = if small { 42.0 } else { rng.range(42.0, 48.0) };
    // Each a little in from its corner: the city's buildings stand behind
    // it too. (A spine's by the same amount, as fair as its corners are.)
    let same = Vec2::new(rng.range(3.0, 16.0), rng.range(3.0, 16.0));
    let in_by = if small { 8.0 } else { 16.0 };
    let bases = order
        .iter()
        .map(|&k| Base {
            corner: corners[k].0
                + corners[k].1
                    * if plan.form == Form::Radial {
                        Vec2::new(rng.range(3.0, in_by), rng.range(3.0, in_by))
                    } else {
                        Vec2::splat(same.x.min(in_by))
                    },
            inward: corners[k].1,
            size: court,
        })
        .collect();

    let mut sk = Skeleton {
        size,
        platforms,
        streets,
        squares: Vec::new(),
        landmarks,
        heart,
        bases,
        lots: Vec::new(),
        spans: Vec::new(),
        edges: Vec::new(),
        decos: Vec::new(),
        repair_spots: Vec::new(),
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
    squares(&mut sk, rng);
    gates_and_lines(&mut sk, rng);
    plaza_dressing(&mut sk, rng);
    districts(&mut sk, rng, plan);
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
    sk
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
            sk.edges.push(edge(m0, m1, EdgeKind::Blockade, 0.0));
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

/// One to three squares out in the districts, a fountain or a monument in
/// each.
fn squares(sk: &mut Skeleton, rng: &mut Rng) {
    let want = sk.squares.len() as i32 + rng.int(1, 3);
    let mut tries = 0;
    while (sk.squares.len() as i32) < want && tries < 400 {
        tries += 1;
        let p = Vec2::new(rng.range(0.0, sk.size), rng.range(0.0, sk.size));
        let r = rng.range(11.0, 16.0);
        let s = sk.size;
        let inside = p.x > r + 30.0 && p.y > r + 30.0 && p.x < s - r - 30.0 && p.y < s - r - 30.0;
        let off_streets = sk
            .streets
            .iter()
            .all(|st| st.distance(p) > st.half + r + 6.0);
        let off_plaza = sk.off_plazas(p) > r + 14.0;
        let off_bases = sk.bases.iter().all(|b| !b.contains(p, r + 16.0));
        let apart = sk
            .squares
            .iter()
            .all(|q| q.center.distance(p) > q.radius + r + 40.0);
        if inside && off_streets && off_plaza && off_bases && apart {
            sk.squares.push(Square {
                center: p,
                radius: r,
            });
            let kind = if rng.chance(0.5) {
                DecoKind::Fountain
            } else {
                DecoKind::Monument
            };
            let face = (sk.heart - p).normalize_or(Vec2::X);
            sk.decos.push(Deco { at: p, face, kind });
        }
    }
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
        if end - start < 24.0 {
            continue;
        }
        let gate = rng.range(start + 9.0, (end - 14.0).max(start + 10.0));
        sk.spans.push(Span {
            at: a.from + dir * gate,
            along: dir,
            opening: 6.5,
            ruined: k == ruined && rng.chance(0.75),
        });
        let spacing = rng.range(13.0, 17.0);
        let mut d = start;
        let mut i = 0;
        while d < end {
            for s in [-1.0f32, 1.0] {
                if (d - gate).abs() < 9.0 {
                    continue;
                }
                let p = a.from + dir * d + side * s * (a.half - 1.5);
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
    for (lm, plaza) in sk.landmarks.iter().zip(&sk.platforms) {
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
    if sk.platforms.is_empty() {
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

/// The blocks between the avenues: lots on a grid, each district (the
/// wedge between two avenues) on its own grid turned along one of them.
fn districts(sk: &mut Skeleton, rng: &mut Rng, plan: &Plan) {
    let c = sk.heart;
    let mut angles: Vec<f32> = sk
        .avenues()
        .filter(|a| a.from == c || a.to == c)
        .map(|a| {
            let d = if a.from == c { a.dir() } else { -a.dir() };
            d.y.atan2(d.x).rem_euclid(TAU)
        })
        .collect();
    if angles.is_empty() {
        angles.push(rng.range(0.0, TAU));
    }
    angles.sort_by(f32::total_cmp);
    let n = angles.len();
    // How the city's built this seed: how many slabs on pillars, how much
    // is already flattened.
    let raised = rng.range(0.12, 0.28);
    let rubble = rng.range(0.05, 0.14);
    for k in 0..n {
        let a0 = angles[k];
        let mut a1 = angles[(k + 1) % n];
        if a1 <= a0 {
            a1 += TAU;
        }
        // The grid runs along one of the two avenues, or square to the map.
        let turn = match rng.index(3) {
            0 => a0,
            1 => a1,
            _ => 0.0,
        };
        let (u, v) = (
            Vec2::new(turn.cos(), turn.sin()),
            Vec2::new(-turn.sin(), turn.cos()),
        );
        let (s0, s1) = plan.density.step();
        let step = rng.range(s0, s1);
        let off = Vec2::new(rng.range(0.0, step), rng.range(0.0, step));
        // Over the whole city from wherever its heart is (an old town's
        // rings need more round them than across).
        let reach = ((sk.size + SPRAWL) / step) as i32 + 3;
        let reach = if plan.form == Form::OldTown {
            reach * 7
        } else {
            reach
        };
        let size = sk.size;
        for i in -reach..=reach {
            for j in -reach..=reach {
                let q = Vec2::new(i as f32, j as f32) * step + off;
                let mut turn = turn;
                let mut p =
                    c + u * q.x + v * q.y + Vec2::new(rng.range(-1.5, 1.5), rng.range(-1.5, 1.5));
                if plan.form == Form::OldTown {
                    // In rings round the square, each ring's lots turned to
                    // it, the lanes between them never lining up.
                    let ring = i.max(0) as f32;
                    let r = 38.0 + ring * step;
                    let n = (TAU * r / step).floor();
                    if i < 0 || j < 0 || j as f32 >= n {
                        continue;
                    }
                    let a = (j as f32 + ring * 0.37) / n * TAU + (ring * 1.9).sin() * 0.3;
                    p = c + Vec2::new(a.cos(), a.sin()) * (r + rng.range(-1.5, 1.5));
                    turn = a;
                }
                let d = p - c;
                let ang = d.y.atan2(d.x).rem_euclid(TAU);
                let ang = if ang < a0 { ang + TAU } else { ang };
                if !(a0..a1).contains(&ang) {
                    continue;
                }
                // How far out past the edge it stands (negative: inside).
                let out = (-p).max(p - Vec2::splat(size)).max_element();
                let r = rng.f32();
                let kind = if out > NEAR {
                    LotKind::Distant
                } else if r < raised {
                    LotKind::Raised
                } else if r < 1.0 - rubble {
                    if rng.chance(0.18) {
                        LotKind::Small
                    } else {
                        LotKind::Big
                    }
                } else {
                    LotKind::Rubble
                };
                let half = if kind == LotKind::Small {
                    Vec2::new(rng.range(4.5, 6.5), rng.range(4.5, 6.5))
                } else {
                    Vec2::new(rng.range(7.5, 9.0), rng.range(6.5, 9.0))
                };
                let quarter = if rng.chance(0.5) { 0.0 } else { FRAC_PI_2 };
                let yaw = -turn + quarter + rng.range(-0.03, 0.03);
                let lot = OBox {
                    center: p,
                    yaw,
                    half,
                    y0: 0.0,
                    y1: 1.0,
                    ..OBox::default()
                };
                let beyond = |q: Vec2| q.min_element() < -SPRAWL || q.max_element() > size + SPRAWL;
                if lot
                    .corners()
                    .iter()
                    .chain([p].iter())
                    .any(|&q| sk.reserved(q) || beyond(q))
                    || sk.lots.iter().any(|l| l.footprint.overlaps(&lot, 5.0))
                {
                    continue;
                }
                if rng.chance(plan.density.empty()) {
                    // An empty lot: a little more open ground.
                    continue;
                }
                let to_mid = lot.local_point(c);
                let front = if (to_mid.x / half.x).abs() > (to_mid.y / half.y).abs() {
                    (0, to_mid.x.signum())
                } else {
                    (1, to_mid.y.signum())
                };
                sk.lots.push(Lot {
                    footprint: lot,
                    front,
                    kind,
                });
            }
        }
    }
}
