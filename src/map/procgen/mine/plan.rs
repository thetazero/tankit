//! A mine laid out on the shared skeleton: a cliff wandering across the
//! map at any angle, digs in it wherever there's rock enough behind, each
//! square to the cliff where it stands, ramps up onto the top, the spawns
//! out on the floor. (The dressed mine's still `layout`'s: this takes its
//! place piece by piece.)

use std::f32::consts::TAU;

use glam::Vec2;

use crate::ai::nav::NavGrid;
use crate::faction::Faction;
use crate::map::procgen::check;
use crate::map::procgen::crossing::{Crossing, Kind};
use crate::map::procgen::desert::skeleton::RAMP_RUN;
use crate::map::procgen::land::*;
use crate::map::procgen::mine::line_distance;
use crate::map::procgen::skeleton::*;
use crate::rng::Rng;

/// How far a dig's rooms keep in from the rock's other faces, m.
const ROCK: f32 = 7.0;

/// The rooms of a dig: two ways in from the cliff, a hall joining them, and
/// a heading on from it, the first way in at the middle of its frame.
fn rooms(rng: &mut Rng) -> Vec<Hollow> {
    let room = |x0: f32, y0: f32, x1: f32, y1: f32, roof: f32| Hollow {
        min: Vec2::new(x0.min(x1), y0),
        max: Vec2::new(x0.max(x1), y1),
        floor: 0.0,
        roof,
    };
    let depth = rng.range(22.0, 30.0);
    let hall = rng.range(14.0, 20.0);
    // Which side of the first way in the rest is.
    let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
    let far = rng.range(28.0, 38.0) * side;
    let heading = rng.range(0.35, 0.65) * far;
    vec![
        room(-5.5, -8.0, 5.5, depth + hall, 6.5),
        room(far - 5.0, -8.0, far + 5.0, depth + hall, 6.5),
        room(-5.5 * side, depth, far + 5.0 * side, depth + hall, 9.0),
        room(
            heading - 4.5,
            depth + hall - 1.0,
            heading + 4.5,
            depth + hall + rng.range(8.0, 14.0),
            6.5,
        ),
    ]
}

/// `dig` with pillars left in its hall, between its ways in: toward the back
/// wall, so there's a way past in front of them.
fn pillared(rng: &mut Rng, mut dig: Dig) -> Dig {
    let hall = dig.rooms[2];
    let mid = Vec2::new(
        (hall.min.x + hall.max.x) * 0.5,
        hall.min.y + (hall.max.y - hall.min.y) * 0.68,
    );
    let r = rng.range(1.7, 2.2);
    for s in [-1.0, 1.0] {
        dig.pillars.push((mid + Vec2::X * s * 6.5, r));
    }
    dig
}

/// Whether `dig` fits in `land`: rock all round its rooms but at its
/// mouths, inside an arena `size` across, off the other digs.
fn fits(land: &Land, dig: &Dig, size: f32) -> bool {
    let apart = land
        .digs
        .iter()
        .all(|o| dig.corners().all(|p| o.distance(p) > 2.0 * ROCK));
    apart
        && dig.rooms.iter().all(|r| {
            let n = 4;
            (0..=n).all(|i| {
                (0..=n).all(|j| {
                    let t = Vec2::new(i as f32, j as f32) / n as f32;
                    let q = r.min - ROCK + (r.max - r.min + 2.0 * ROCK) * t;
                    let p = dig.to_map(q);
                    // (Out toward the mouth, it's the cliff.)
                    let deep = q.y < ROCK + 4.0 || land.plateau(p) < -2.0;
                    deep && p.min_element() > 2.0 && p.max_element() < size - 2.0
                })
            })
        })
}

/// `pts` with their corners cut.
fn eased(pts: &[Vec2]) -> Vec<Vec2> {
    let mut out = pts.to_vec();
    for _ in 0..2 {
        let mut s = vec![out[0]];
        for w in out.windows(2) {
            s.push(w[0].lerp(w[1], 0.25));
            s.push(w[0].lerp(w[1], 0.75));
        }
        s.push(*out.last().unwrap());
        out = s;
    }
    out
}

/// Crosses `wash`: a bridge for each track over it, and fords between. None
/// if it doesn't work out: it comes too near something, or a track meets it
/// aslant.
fn cross(land: &mut Land, wash: &Wash, tracks: &[Vec<Vec2>], size: f32) -> Option<Vec<Crossing>> {
    let edge = wash.floor + wash.bank;
    if land.digs.iter().any(|d| wash.distance(d.at) < edge + 24.0) {
        return None;
    }
    let mut over = Vec::new();
    let mut at = Vec::new();
    for t in tracks {
        let (a, b) = (t[1], t[2]);
        let n = (a.distance(b) / 0.5) as usize;
        let near = (0..=n)
            .map(|k| a.lerp(b, k as f32 / n as f32))
            .min_by(|p, q| wash.distance(*p).total_cmp(&wash.distance(*q)))?;
        if wash.distance(near) > 1.0 {
            if wash.distance(near) < edge + 8.0 {
                return None;
            }
            continue;
        }
        let (_, s) = wash.near(near);
        let (_, way, _) = wash.frame(s);
        let dir = (b - a).normalize();
        if way.dot(dir).abs() > 0.55 {
            return None;
        }
        let half = (edge + 4.0) / way.perp_dot(dir).abs();
        over.push(Crossing {
            kind: Kind::Bridge,
            from: near - dir * half,
            to: near + dir * half,
            half: super::DECK_HALF,
            top: super::DECK_TOP,
            floor: 0.0,
            roofed: (0.0, 0.0),
            walled: (0.0, 0.0),
        });
        at.push(s);
    }
    let (lo, hi) = wash.inside(size);
    let mut fords = Vec::new();
    for f in [0.1, 0.3, 0.5, 0.7, 0.9] {
        let s = lo + (hi - lo) * f;
        if hi - lo < 60.0 || at.iter().any(|&b| (b - s).abs() < 24.0) {
            continue;
        }
        let (p, _, n) = wash.frame(s);
        fords.push((p + n * 27.0, p - n * 27.0, 7.0));
    }
    if fords.len() < 2 {
        return None;
    }
    land.fords.extend(fords);
    Some(over)
}

/// A mine for `sides` sides, if this draw holds together.
fn draw(rng: &mut Rng, sides: &[Faction]) -> Result<Skeleton, String> {
    let size = 2.0 * rng.int(125, 160) as f32;
    let mid = Vec2::splat(size * 0.5);
    let mut land = Land {
        dunes: Some(Dunes {
            wind: Vec2::from_angle(rng.range(0.0, TAU)),
            wave: rng.range(38.0, 60.0),
            height: rng.range(0.6, 1.2),
            phase: rng.range(0.0, 1.0),
            calm: Vec::new(),
        }),
        ..Land::default()
    };
    land.high.height = rng.range(11.5, 13.0);

    // The cliff: right across, past the edges, bending on the way.
    let along = Vec2::from_angle(rng.range(0.0, TAU));
    let off = along.perp() * rng.range(0.0, size * 0.12);
    let n = rng.int(3, 5);
    let reach = size * 0.75;
    let line: Vec<Vec2> = (0..=n)
        .map(|k| {
            let t = (k as f32 / n as f32 * 2.0 - 1.0) * reach;
            let sway = if k == 0 || k == n {
                0.0
            } else {
                rng.range(-0.11, 0.11) * size
            };
            mid + off + along * t + along.perp() * sway
        })
        .collect();
    land.high.slabs.push(Slab::Behind { line: line.clone() });

    // Digs along it.
    let want = rng.int(1, 3) as usize;
    for _ in 0..160 {
        if land.digs.len() >= want {
            break;
        }
        let k = rng.index(line.len() - 1);
        let (a, b) = (line[k], line[k + 1]);
        let dig = Dig {
            at: a.lerp(b, rng.range(0.1, 0.9)),
            facing: (b - a).normalize().perp(),
            floor: 0.0,
            rooms: rooms(rng),
            pillars: Vec::new(),
        };
        let dig = pillared(rng, dig);
        if fits(&land, &dig, size) {
            land.digs.push(dig);
        }
    }
    if land.digs.is_empty() {
        return Err("no room for a dig".into());
    }
    for d in &land.digs {
        // Level in front of the mouths.
        let dunes = land.dunes.as_mut().unwrap();
        for r in d.rooms.iter().filter(|r| r.min.y < 0.0) {
            let x = (r.min.x + r.max.x) * 0.5;
            let p = d.at - d.facing.perp() * x - d.facing * 6.0;
            dunes.calm.push((p, 12.0, 24.0));
        }
    }

    // A railroad out of each dig's first way in, straight on to the edge.
    let mut tracks: Vec<Vec<Vec2>> = Vec::new();
    for d in &land.digs {
        let from = d.to_map(Vec2::new(0.0, d.rooms[2].max.y - 3.0));
        let dir = -d.facing;
        let mut end = d.at;
        while end.min_element() > -6.0 && end.max_element() < size + 6.0 {
            end += dir;
        }
        let clear = land.digs.iter().all(|o| {
            o.at == d.at || (0..=40).all(|k| o.distance(d.at.lerp(end, k as f32 / 40.0)) > 8.0)
        }) && tracks
            .iter()
            .all(|t| (0..=40).all(|k| line_distance(t, d.at.lerp(end, k as f32 / 40.0)) > 16.0));
        // (Not out along the cliff and back into it.)
        let out = (2..=40).all(|k| land.plateau(d.at.lerp(end, k as f32 / 40.0)) > 4.0);
        if clear && out {
            tracks.push(vec![from, d.at, end]);
        }
    }
    let dunes = land.dunes.as_mut().unwrap();
    for t in &tracks {
        let n = (t[1].distance(t[2]) / 8.0) as usize;
        for k in 0..=n {
            dunes
                .calm
                .push((t[1].lerp(t[2], k as f32 / n as f32), 6.0, 16.0));
        }
    }

    // A wash along the floor, out from under the cliff, perhaps.
    let mut crossings = Vec::new();
    if rng.chance(0.65) {
        for _ in 0..12 {
            let out = rng.range(62.0, 85.0);
            let pts: Vec<Vec2> = line
                .iter()
                .map(|&p| p - along.perp() * (out + rng.range(-9.0, 9.0)))
                .collect();
            let wash = Wash {
                line: eased(&pts),
                floor: 6.5,
                bank: 5.0,
                depth: 6.0,
                sway: 1.2,
                phase: rng.range(0.0, TAU),
            };
            if let Some(x) = cross(&mut land, &wash, &tracks, size) {
                crossings = x;
                land.washes.push(wash);
                break;
            }
        }
    }

    // Ways up, square to the cliff, off the digs.
    let run = land.high.height * RAMP_RUN;
    let mut tries = 0;
    while land.high.ramps.len() < 2 && tries < 200 {
        tries += 1;
        let k = rng.index(line.len() - 1);
        let (a, b) = (line[k], line[k + 1]);
        let p = a.lerp(b, rng.range(0.15, 0.85));
        let into = (b - a).normalize().perp();
        let ramp = Ramp {
            foot: p - into * run,
            top: p + into * 5.0,
        };
        let inside = [ramp.foot, ramp.top]
            .iter()
            .all(|q| q.min_element() > 14.0 && q.max_element() < size - 14.0);
        let clear = land.digs.iter().all(|d| {
            (0..=8).all(|i| d.distance(ramp.foot.lerp(ramp.top, i as f32 / 8.0)) > RAMP_HALF + 8.0)
        }) && land
            .high
            .ramps
            .iter()
            .all(|r| r.top.distance(ramp.top) > 50.0)
            && (0..=8).all(|i| {
                let q = ramp.foot.lerp(ramp.top, i as f32 / 8.0);
                tracks.iter().all(|t| line_distance(t, q) > RAMP_HALF + 6.0)
                    && land.wash_distance(q) > RAMP_HALF + 16.0
            });
        if inside && clear {
            land.high.ramps.push(ramp);
        }
    }
    if land.high.ramps.is_empty() {
        return Err("no way up".into());
    }

    // The Hold site: out in front of the mouth of the dig nearest the middle.
    land.digs
        .sort_by(|a, b| a.at.distance(mid).total_cmp(&b.at.distance(mid)));
    let first = &land.digs[0];
    let heart = first.at - first.facing * 26.0 - first.facing.perp() * 15.0;
    if (heart - mid).abs().max_element() > size * 0.5 - 45.0 {
        return Err("the Hold site's at the edge".into());
    }
    let mut sk = Skeleton {
        size,
        land,
        landmarks: vec![Landmark {
            center: heart,
            radius: 12.0,
            precinct: 12.0,
            doors: Vec::new(),
            fallen: None,
            kind: LandmarkKind::Open,
            clear: 12.0,
            reach: 18.0,
            facing: along,
        }],
        heart,
        tracks,
        crossings,
        ..Skeleton::default()
    };
    let nav = NavGrid::build(&sk.greybox(sides));
    if !sk.place_bases(rng, sides.len(), &nav) {
        return Err("no fair spawns".into());
    }
    let grey = sk.greybox(sides);
    let nav = NavGrid::build(&grey);
    check::playable_on(&grey, &nav)?;
    // Every hall a drive from every side.
    for d in &sk.land.digs {
        let hall = d.rooms[2];
        let p = d.to_map((hall.min + hall.max) * 0.5);
        for b in sk.base_spots() {
            let g = sk.ground(b);
            let way = nav.route(
                glam::Vec3::new(b.x, g, b.y),
                glam::Vec3::new(p.x, d.floor, p.y),
            );
            if way.last().is_none_or(|w| (w.y - d.floor).abs() > 1.0) {
                return Err("a dig nobody can drive to".into());
            }
        }
    }
    Ok(sk)
}

/// The mine `seed` names: the first, from it on, that holds together.
pub fn skeleton(seed: u64, sides: &[Faction]) -> Option<Skeleton> {
    (0..64u64).find_map(|attempt| {
        let mut rng = Rng::new(seed.wrapping_mul(0x9E37_79B9).wrapping_add(attempt) ^ 0x3_D16);
        draw(&mut rng, sides)
            .map_err(|why| log::debug!("mine seed {seed} attempt {attempt}: {why}"))
            .ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mines come at any angle, with one dig or several, each with a hall
    /// both sides can drive into.
    #[test]
    fn mines_lie_at_any_angle() {
        let sides = [Faction::Ithessar, Faction::Resonant];
        let all: Vec<Skeleton> = (0..3)
            .map(|s| skeleton(s, &sides).unwrap_or_else(|| panic!("seed {s}: no mine")))
            .collect();
        let off_axis = all
            .iter()
            .flat_map(|sk| &sk.land.digs)
            .filter(|d| d.facing.abs().min_element() > 0.2)
            .count();
        assert!(off_axis >= 2, "{off_axis} digs at an angle");
        assert!(
            all.iter().any(|sk| sk.land.digs.len() > 1),
            "never two digs"
        );
    }
}
