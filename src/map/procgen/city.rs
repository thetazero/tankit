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

/// Lays out a city for `sides` sides (2 to 4), one corner court each.
pub fn skeleton(rng: &mut Rng, sides: usize) -> Skeleton {
    let size = 20.0 * rng.int(16, 18) as f32;
    let mid = size * 0.5;
    let center =
        Vec2::splat(mid) + Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * size * 0.04;

    let half = rng.range(54.0, 66.0);
    let plaza = Platform {
        center,
        half,
        round: rng.range(14.0, 26.0),
        height: 2.0,
        ramp: 14.0,
    };
    let radius = rng.range(21.0, 26.0);

    // The avenues: one to each edge (now and then one edge goes without),
    // meeting it somewhere near its middle.
    let skip = if rng.chance(0.25) {
        Some(rng.index(4))
    } else {
        None
    };
    let mut streets = Vec::new();
    // The edges, counter-clockwise from the south: (start, along, inward).
    let sides4 = self::sides(size);
    // Mouths along each edge, as (t from, t to).
    let mut mouths: Mouths = Default::default();
    for (k, &(start, along, _)) in sides4.iter().enumerate() {
        if skip == Some(k) {
            continue;
        }
        let t = size * rng.range(0.4, 0.6);
        let to = start + along * t;
        streets.push(Street {
            from: center,
            to,
            half: AVENUE_HALF,
            clear: 6.0,
            rank: Rank::Avenue,
        });
        mouths[k].push((t - AVENUE_HALF - 3.0, t + AVENUE_HALF + 3.0));
    }
    let doors: Vec<Vec2> = streets.iter().map(|s| s.dir()).collect();
    let fallen = rng.chance(0.85).then(|| {
        // Between two doors.
        let k = rng.index(doors.len());
        (doors[k] + doors[(k + 1) % doors.len()]).normalize_or(doors[k])
    });
    let landmark = Landmark {
        center,
        radius,
        precinct: (radius + 20.0).min(half - 8.0),
        doors: doors.clone(),
        fallen,
    };

    // A court in a corner for each side, opposite corners first.
    let corners = [
        (Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)),
        (Vec2::new(size, 0.0), Vec2::new(-1.0, 1.0)),
        (Vec2::new(size, size), Vec2::new(-1.0, -1.0)),
        (Vec2::new(0.0, size), Vec2::new(1.0, -1.0)),
    ];
    let order: &[usize] = match sides {
        0..=2 => &[0, 2],
        3 => &[0, 1, 2],
        _ => &[0, 1, 2, 3],
    };
    let court = rng.range(42.0, 48.0);
    // Each a little in from its corner, by its own amount: the city's
    // buildings stand behind it too.
    let bases = order
        .iter()
        .map(|&k| Base {
            corner: corners[k].0
                + corners[k].1 * Vec2::new(rng.range(3.0, 16.0), rng.range(3.0, 16.0)),
            inward: corners[k].1,
            size: court,
        })
        .collect();

    let mut sk = Skeleton {
        size,
        platforms: vec![plaza],
        streets,
        squares: Vec::new(),
        landmark,
        bases,
        lots: Vec::new(),
        spans: Vec::new(),
        edges: Vec::new(),
        decos: Vec::new(),
        repair_spots: Vec::new(),
    };

    squares(&mut sk, rng);
    gates_and_lines(&mut sk, rng);
    plaza_dressing(&mut sk, rng);
    districts(&mut sk, rng);
    clear_courts(&mut sk, rng);
    cordon(&mut sk, rng, &mouths);

    // Repair drops: down the middle of each avenue, twice, and between
    // the avenues on the plaza where they land clear.
    let mut repairs: Vec<Vec2> = sk
        .avenues()
        .flat_map(|a| [0.55, 0.8].map(|f| a.from + a.dir() * a.len() * f))
        .collect();
    for b in bisectors(&sk) {
        repairs.push(center + b * (sk.landmark.precinct + 14.0));
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

/// Directions halfway between each avenue and the next round the middle.
fn bisectors(sk: &Skeleton) -> Vec<Vec2> {
    let mut angles: Vec<f32> = sk
        .avenues()
        .map(|a| {
            let d = a.dir();
            d.y.atan2(d.x)
        })
        .collect();
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
    let want = rng.int(1, 3);
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
        let off_plaza = sk.plaza().sdf(p) > sk.plaza().ramp + r + 14.0;
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
            let face = (sk.landmark.center - p).normalize();
            sk.decos.push(Deco { at: p, face, kind });
        }
    }
}

/// Down each avenue: a gate over it partway out, and monuments and masts
/// lining it.
fn gates_and_lines(sk: &mut Skeleton, rng: &mut Rng) {
    let plaza = *sk.plaza();
    let avenues: Vec<Street> = sk.avenues().copied().collect();
    let ruined = rng.index(avenues.len().max(1));
    for (k, a) in avenues.iter().enumerate() {
        let dir = a.dir();
        let side = Vec2::new(-dir.y, dir.x);
        // Where the avenue leaves the plaza's slope.
        let mut start = 0.0;
        while plaza.sdf(a.from + dir * start) < plaza.ramp + 4.0 {
            start += 1.0;
        }
        let end = a.len() - 12.0;
        let gate = rng.range(start + 18.0, (end - 28.0).max(start + 20.0));
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
    let plaza = *sk.plaza();
    let c = plaza.center;
    for b in bisectors(sk) {
        // On the top, well in from its edge, outside the precinct.
        let mut d = sk.landmark.precinct + 8.0;
        while plaza.sdf(c + b * (d + 1.0)) < -10.0 {
            d += 1.0;
        }
        sk.decos.push(Deco {
            at: c + b * d,
            face: b,
            kind: DecoKind::Fountain,
        });
    }
    let mut placed: Vec<Vec2> = Vec::new();
    let mut tries = 0;
    while placed.len() < 120 && tries < 6000 {
        tries += 1;
        let p = Vec2::new(rng.range(0.0, sk.size), rng.range(0.0, sk.size));
        let s = plaza.sdf(p);
        if !(plaza.ramp + 3.0..plaza.ramp + 24.0).contains(&s)
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
fn districts(sk: &mut Skeleton, rng: &mut Rng) {
    let c = sk.landmark.center;
    let mut angles: Vec<f32> = sk
        .avenues()
        .map(|a| {
            let d = a.dir();
            d.y.atan2(d.x).rem_euclid(TAU)
        })
        .collect();
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
        let step = rng.range(26.0, 30.0);
        let off = Vec2::new(rng.range(0.0, step), rng.range(0.0, step));
        let reach = ((sk.size * 0.5 + SPRAWL) / step) as i32 + 3;
        let size = sk.size;
        for i in -reach..=reach {
            for j in -reach..=reach {
                let q = Vec2::new(i as f32, j as f32) * step + off;
                let p =
                    c + u * q.x + v * q.y + Vec2::new(rng.range(-1.5, 1.5), rng.range(-1.5, 1.5));
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
                if rng.chance(0.06) {
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
