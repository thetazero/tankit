//! A mine laid out on the shared skeleton: a cliff wandering across the
//! map at any angle, digs in it wherever there's rock enough behind, each
//! square to the cliff where it stands, ramps up onto the top, the spawns
//! as far apart as they come.

use std::f32::consts::TAU;

use glam::Vec2;

use crate::ai::nav::NavGrid;
use crate::faction::Faction;
use crate::map::procgen::check;
use crate::map::procgen::crossing::{Crossing, Kind};
use crate::map::procgen::desert::skeleton::{Form, PAD, RAMP_RUN};
use crate::map::procgen::land::*;
use crate::map::procgen::mine::{SHED_HALF, line_at, line_distance, line_len};
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

/// `dig` with a bay at the end of its heading, and a borer in it cutting a
/// bore out to one side.
fn bored(rng: &mut Rng, dig: &Dig) -> Dig {
    let mut dig = dig.clone();
    let heading = dig.rooms[3];
    let x = (heading.min.x + heading.max.x) * 0.5;
    let y0 = heading.max.y - 1.0;
    let (half, deep) = (rng.range(10.0, 12.0), rng.range(20.0, 24.0));
    let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
    let mid = y0 + deep * 0.5;
    dig.rooms.push(Hollow {
        min: Vec2::new(x - half, y0),
        max: Vec2::new(x + half, y0 + deep),
        floor: 0.0,
        roof: 9.0,
    });
    let (near, far) = (x + side * (half - 1.0), x + side * (half + 4.0));
    dig.rooms.push(Hollow {
        min: Vec2::new(near.min(far), mid - 3.5),
        max: Vec2::new(near.max(far), mid + 3.5),
        floor: 0.0,
        roof: 6.0,
    });
    dig.beam = Some((Vec2::new(x + side * (half - 5.4), mid), Vec2::new(far, mid)));
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

/// A point `t` (0..1) of the way along a cliff, and the way into it there,
/// taken over `r` m either side.
fn place(line: &[Vec2], t: f32, r: f32) -> (Vec2, Vec2) {
    let total = line_len(line, line.len() - 1);
    let s = (total * t).clamp(r, total - r);
    let (a, b) = (line_at(line, s - r).0, line_at(line, s + r).0);
    (line_at(line, s).0, (b - a).normalize().perp())
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

/// The track that goes over `x` laid straight across it.
fn straighten(tracks: &mut [Vec<Vec2>], x: &Crossing) {
    let mid = (x.from + x.to) * 0.5;
    let Some(t) = tracks
        .iter_mut()
        .min_by(|a, b| line_distance(a, mid).total_cmp(&line_distance(b, mid)))
    else {
        return;
    };
    let dir = x.dir();
    let (a, b) = (x.from - dir * 4.0, x.to + dir * 4.0);
    let along = |p: Vec2| (p - a).dot(dir);
    let len = a.distance(b);
    // (Its points before the bridge, the bridge, those past it: the first
    // two are the dig's.)
    let before: Vec<Vec2> = t
        .iter()
        .enumerate()
        .filter(|(k, p)| *k < 2 || along(**p) < -2.0)
        .map(|(_, p)| *p)
        .collect();
    let after: Vec<Vec2> = t[2..]
        .iter()
        .copied()
        .filter(|&p| along(p) > len + 2.0)
        .collect();
    *t = [before, vec![a, b], after].concat();
}

/// Crosses `wash`: a bridge for each track over it, and fords between. None
/// if it doesn't work out: it comes too near something, or a track meets it
/// aslant.
fn cross(land: &mut Land, wash: &Wash, tracks: &[Vec<Vec2>], size: f32) -> Option<Vec<Crossing>> {
    // (At its widest.)
    let edge = wash.floor + wash.sway + wash.bank * 1.6;
    if land.digs.iter().any(|d| wash.distance(d.at) < edge + 24.0) {
        return None;
    }
    let mut over = Vec::new();
    let mut at = Vec::new();
    for t in tracks {
        let (first, total) = (line_len(t, 1), line_len(t, t.len() - 1));
        let n = ((total - first) / 0.5) as usize;
        let (near, dir) = (0..n)
            .map(|k| line_at(t, first + k as f32 * 0.5))
            .min_by(|p, q| wash.distance(p.0).total_cmp(&wash.distance(q.0)))?;
        if wash.distance(near) > 1.0 {
            if wash.distance(near) < edge + 8.0 {
                return None;
            }
            continue;
        }
        let (_, s) = wash.near(near);
        let (_, way, _) = wash.frame(s);
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
    // Only a couple cut on purpose: most of the ways over are where a bank's
    // easy.
    for f in [0.2, 0.75] {
        let s = lo + (hi - lo) * f;
        if hi - lo < 60.0 || at.iter().any(|&b| (b - s).abs() < 30.0) {
            continue;
        }
        let (p, t, n) = wash.frame(s);
        let slant = t * if f < 0.5 { 9.0 } else { -9.0 };
        fords.push((p + n * 31.0 + slant, p - n * 31.0 - slant, 4.5));
    }
    if fords.is_empty() {
        return None;
    }
    land.fords.extend(fords);
    Some(over)
}

/// A warehouse on a spur off the first track, and where along the track its
/// trains load, wherever there's room for each.
fn works(
    rng: &mut Rng,
    land: &mut Land,
    tracks: &[Vec<Vec2>],
    crossings: &[Crossing],
    size: f32,
) -> (Option<Spur>, Option<f32>) {
    let Some(track) = tracks.first() else {
        return (None, None);
    };
    let out = track[0].distance(track[1]);
    let total = line_len(track, track.len() - 1);
    let off_bridges = |s: f32, r: f32| {
        let (p, _) = line_at(track, s);
        crossings.iter().all(|x| x.distance(p) > r)
    };
    let fits = |s: &Spur| {
        let (at, along) = s.shed;
        let across = along.perp();
        let n = 6;
        (0..=n).all(|i| {
            (0..=n).all(|k| {
                let u = (i as f32 * 2.0 / n as f32 - 1.0) * (SHED_HALF.x + 12.0);
                let v = (k as f32 * 2.0 / n as f32 - 1.0) * (SHED_HALF.y + 12.0);
                let p = at + across * u + along * v;
                p.min_element() > 6.0
                    && p.max_element() < size - 6.0
                    && land.plateau(p) > 12.0
                    && land.wash_distance(p) > 18.0
                    && tracks[1..].iter().all(|t| line_distance(t, p) > 8.0)
            })
        })
    };
    let mut spots = Vec::new();
    let mut s = out + 45.0;
    while s < total - 30.0 {
        for side in [-1.0, 1.0] {
            if off_bridges(s, 12.0) {
                spots.push((s, side));
            }
        }
        s += 5.0;
    }
    let mut spur = None;
    while spur.is_none() && !spots.is_empty() {
        let (s, side) = spots.swap_remove(rng.index(spots.len()));
        spur = Some(super::works::spur(track, s, side)).filter(|s| fits(s));
    }
    if let Some(s) = &spur {
        let dunes = land.dunes.as_mut().unwrap();
        dunes.calm.push((s.shed.0, 30.0, 44.0));
        for p in &s.line {
            dunes.calm.push((*p, 7.0, 16.0));
        }
    }
    // The tipple: clear of the junction and the bridges, its train too.
    let mut loading = None;
    let mut s = out + 20.0;
    while loading.is_none() && s < total - 50.0 {
        let clear = [0.0, 10.0, 22.0, 28.0]
            .iter()
            .all(|d| off_bridges(s + d, 10.0))
            && spur
                .as_ref()
                .is_none_or(|j| !(s - 24.0..s + 50.0).contains(&j.junction));
        if clear {
            loading = Some(s);
        }
        s += 4.0;
    }
    (spur, loading)
}

/// Each side's spawns, as far from each other's as the ground allows (near
/// enough): nobody starts in sight of anybody. Not a word about how far
/// each is from the fight. False if there's no such choice.
fn spawns(sk: &mut Skeleton, rng: &mut Rng, sides: usize, nav: &NavGrid) -> bool {
    let size = sk.size;
    let land = &sk.land;
    let mut sites: Vec<Vec2> = Vec::new();
    for _ in 0..1500 {
        let m = PAD + 6.0;
        let p = Vec2::new(rng.range(m, size - m), rng.range(m, size - m));
        let ok = land.plateau(p).abs() > PAD + 4.0
            && land.tower_distance(p) > PAD + 5.0
            && land.ramp_distance(p) > PAD + 10.0
            && land.wash_distance(p) > PAD + 18.0
            && land
                .digs
                .iter()
                .all(|d| d.distance(p) > 10.0 && (d.at - d.facing * 12.0).distance(p) > PAD + 14.0)
            && land.mounds.iter().all(|m| m.at.distance(p) > PAD + 34.0)
            && sk.tracks.iter().all(|t| line_distance(t, p) > PAD + 4.0)
            && sk.shed_distance(p) > PAD + 10.0
            && sk.heart.distance(p) > size * 0.25;
        if ok && !nav.path(p, sk.heart).is_empty() {
            sites.push(p);
        }
        if sites.len() >= 60 {
            break;
        }
    }
    // Start from each, adding whichever's farthest from all so far.
    let mut best: Option<(f32, Vec<Vec2>)> = None;
    for &first in &sites {
        let mut pick = vec![first];
        while pick.len() < sides {
            let gap = |p: &Vec2| {
                pick.iter()
                    .map(|q| q.distance(*p))
                    .fold(f32::INFINITY, f32::min)
            };
            let Some(next) = sites
                .iter()
                .copied()
                .max_by(|a, b| gap(a).total_cmp(&gap(b)))
            else {
                return false;
            };
            pick.push(next);
        }
        let spread = (0..sides)
            .flat_map(|i| (0..i).map(move |j| (i, j)))
            .map(|(i, j)| pick[i].distance(pick[j]))
            .fold(f32::INFINITY, f32::min);
        // (Any of the farther ones will do, not always the corners.)
        let score = spread * rng.range(0.88, 1.0);
        if best.as_ref().is_none_or(|b| score > b.0) {
            best = Some((score, pick));
        }
    }
    let Some((_, pick)) = best else { return false };
    let apart = if sides <= 2 { 0.68 } else { 0.4 };
    let far = (0..sides)
        .flat_map(|i| (0..i).map(move |j| (i, j)))
        .all(|(i, j)| pick[i].distance(pick[j]) > size * apart);
    if !far {
        return false;
    }
    for &at in &pick {
        sk.land.high.level.push((at, PAD, PAD + 22.0));
        if let Some(d) = &mut sk.land.dunes {
            d.calm.push((at, PAD - 4.0, PAD + 14.0));
        }
    }
    sk.bases = pick.into_iter().map(|at| Base::pad(at, PAD)).collect();
    true
}

/// A mine for `sides` sides, if this draw holds together.
fn draw(rng: &mut Rng, sides: &[Faction]) -> Result<Skeleton, String> {
    let size = 2.0 * rng.int(125, 160) as f32;
    let mid = Vec2::splat(size * 0.5);
    let mut land = Land {
        dunes: Some(Dunes {
            wind: Vec2::from_angle(rng.range(0.0, TAU)),
            wave: rng.range(38.0, 60.0),
            height: rng.range(1.4, 2.4),
            phase: rng.range(0.0, 1.0),
            calm: Vec::new(),
        }),
        ..Land::default()
    };
    land.high.height = rng.range(11.5, 13.0);
    land.high.relief = rng.range(11.0, 16.0);
    land.high.talus = rng.range(2.5, 4.0);

    // The cliff: right across, past the edges, bending on the way.
    let along = Vec2::from_angle(rng.range(0.0, TAU));
    // (Less of it high than low.)
    let off = along.perp() * rng.range(0.04, 0.16) * size;
    let n = rng.int(7, 10);
    let reach = size * 0.75;
    // Two long swings and whatever it does on the way.
    let (a0, a1) = (rng.range(0.0, TAU), rng.range(0.0, TAU));
    let (w0, w1) = (rng.range(0.07, 0.13) * size, rng.range(0.03, 0.06) * size);
    let pts: Vec<Vec2> = (0..=n)
        .map(|k| {
            let t = k as f32 / n as f32 * 2.0 - 1.0;
            let sway = w0 * (t * 2.6 + a0).sin()
                + w1 * (t * 6.5 + a1).sin()
                + rng.range(-0.035, 0.035) * size;
            mid + off + along * t * reach + along.perp() * sway
        })
        .collect();
    let line = eased(&pts);
    land.high.slabs.push(Slab::Behind { line: line.clone() });

    // Canyons cut back into it, on through it or blind.
    for _ in 0..rng.int(1, 2) {
        let (from, into) = place(&line, rng.range(0.2, 0.8), 14.0);
        let mut at = from - into * 14.0;
        let mut dir = Vec2::from_angle(rng.range(-0.4, 0.4)).rotate(into);
        let mut way = vec![at];
        let bend = rng.range(-0.12, 0.12);
        let long = rng.range(60.0, 170.0);
        let mut gone = 0.0;
        while gone < long {
            dir = Vec2::from_angle(bend + rng.range(-0.25, 0.25)).rotate(dir);
            at += dir * 12.0;
            gone += 12.0;
            way.push(at);
        }
        land.high.channels.push(Channel {
            line: eased(&way),
            half: rng.range(8.0, 12.0),
        });
    }
    // Mesas out on the floor.
    let mut tries = 0;
    let mut mesas = Vec::new();
    let want = rng.int(1, 3) as usize;
    while mesas.len() < want && tries < 120 {
        tries += 1;
        let radius = rng.range(9.0, 18.0);
        let c = Vec2::new(rng.range(0.1, 0.9), rng.range(0.1, 0.9)) * size;
        let apart = mesas
            .iter()
            .all(|&(q, r): &(Vec2, f32)| q.distance(c) > r + radius + 35.0);
        if land.plateau(c) > radius * 1.25 + 36.0 && apart {
            mesas.push((c, radius));
        }
    }
    for (center, radius) in mesas {
        land.high.slabs.push(Slab::Island {
            center,
            radius,
            phase: rng.range(0.0, TAU),
        });
    }

    // Digs along it.
    let want = rng.int(1, 3) as usize;
    for _ in 0..160 {
        if land.digs.len() >= want {
            break;
        }
        let (at, facing) = place(&line, rng.range(0.1, 0.9), 14.0);
        let dig = Dig {
            at,
            facing,
            floor: 0.0,
            rooms: rooms(rng),
            pillars: Vec::new(),
            beam: None,
        };
        let dig = pillared(rng, dig);
        // A borer at work in it, if there's the rock.
        let bored = bored(rng, &dig);
        let dig = if fits(&land, &bored, size) {
            bored
        } else {
            dig
        };
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
        // Straight out of the mouth, then bending as it goes.
        let mut dir = -d.facing;
        let mut end = d.at + dir * 34.0;
        let mut t = vec![from, d.at, end];
        let bend = rng.range(-0.1, 0.1);
        while end.min_element() > -6.0 && end.max_element() < size + 6.0 && t.len() < 40 {
            dir = Vec2::from_angle(bend + rng.range(-0.06, 0.06)).rotate(dir);
            end += dir * 14.0;
            t.push(end);
        }
        let total = line_len(&t, t.len() - 1);
        let first = line_len(&t, 1);
        let all: Vec<Vec2> = (0..=60)
            .map(|k| line_at(&t, first + (total - first) * k as f32 / 60.0 * 0.999).0)
            .collect();
        let clear = all.iter().all(|&p| {
            land.digs
                .iter()
                .all(|o| o.at == d.at || o.distance(p) > 8.0)
                && tracks.iter().all(|o| line_distance(o, p) > 16.0)
        });
        // (Not out along the cliff and back into it.)
        let out = all[6..].iter().all(|&p| land.plateau(p) > 5.0);
        if clear && out {
            tracks.push(t);
        }
    }
    let dunes = land.dunes.as_mut().unwrap();
    for t in &tracks {
        let (first, total) = (line_len(t, 1), line_len(t, t.len() - 1));
        let n = ((total - first) / 8.0) as usize;
        for k in 0..=n {
            let p = line_at(t, first + (total - first) * k as f32 / n as f32 * 0.999).0;
            dunes.calm.push((p, 6.0, 18.0));
        }
    }

    // A wash along the floor, out from under the cliff, perhaps.
    let mut crossings = Vec::new();
    if rng.chance(0.65) {
        for _ in 0..12 {
            let out = rng.range(62.0, 85.0);
            let bend = (rng.range(0.0, TAU), rng.range(8.0, 18.0));
            let pts: Vec<Vec2> = pts
                .iter()
                .enumerate()
                .map(|(k, &p)| {
                    let sway = bend.1 * (k as f32 * 0.9 + bend.0).sin();
                    p - along.perp() * (out + sway + rng.range(-7.0, 7.0))
                })
                .collect();
            let wash = Wash {
                line: eased(&pts),
                floor: 6.5,
                bank: 4.5,
                depth: rng.range(5.4, 6.4),
                sway: 2.2,
                phase: rng.range(0.0, TAU),
                ragged: true,
            };
            if let Some(x) = cross(&mut land, &wash, &tracks, size) {
                for x in &x {
                    straighten(&mut tracks, x);
                }
                crossings = x;
                land.washes.push(wash);
                break;
            }
        }
    }

    // A great rock over some of the digs, back from the cliff.
    for k in 0..land.digs.len() {
        let d = &land.digs[k];
        let hall = d.rooms[2];
        let t = Tower {
            center: d.to_map(Vec2::new(
                (hall.min.x + hall.max.x) * 0.5,
                hall.max.y + rng.range(10.0, 16.0),
            )),
            radius: rng.range(17.0, 24.0),
            height: rng.range(16.0, 24.0),
            phase: rng.range(0.0, TAU),
        };
        let r = t.radius * 1.25;
        let inside = t.center.min_element() > r + 8.0 && t.center.max_element() < size - r - 8.0;
        if inside && land.plateau(t.center) < -(r + 5.0) && rng.chance(0.8) {
            land.towers.push(t);
        }
    }

    // Lesser rocks standing about on the top.
    let mut tries = 0;
    let want = land.towers.len() + rng.int(3, 6) as usize;
    while land.towers.len() < want && tries < 200 {
        tries += 1;
        let t = Tower {
            center: Vec2::new(rng.range(0.06, 0.94), rng.range(0.06, 0.94)) * size,
            radius: rng.range(5.0, 11.0),
            height: rng.range(7.0, 15.0),
            phase: rng.range(0.0, TAU),
        };
        let r = t.radius * 1.25;
        let inside = t.center.min_element() > r + 8.0 && t.center.max_element() < size - r - 8.0;
        if inside && land.plateau(t.center) < -(r + 6.0) && land.tower_distance(t.center) > r + 22.0
        {
            land.towers.push(t);
        }
    }

    let (spur, loading) = works(rng, &mut land, &tracks, &crossings, size);

    // The dump, out on the floor.
    for _ in 0..200 {
        let m = Mound {
            at: Vec2::new(rng.range(0.15, 0.85), rng.range(0.15, 0.85)) * size,
            radius: rng.range(12.0, 15.0),
            height: rng.range(3.6, 4.6),
            run: 11.0,
        };
        let reach = m.radius * 1.4 + m.run;
        let clear = land.plateau(m.at) > reach + 6.0
            && land.wash_distance(m.at) > reach + 14.0
            && tracks.iter().all(|t| line_distance(t, m.at) > reach + 4.0)
            && spur
                .as_ref()
                .is_none_or(|s| s.shed.0.distance(m.at) > reach + 34.0)
            && land
                .digs
                .iter()
                .all(|d| (d.at - d.facing * 26.0).distance(m.at) > reach + 34.0);
        if clear {
            land.mounds.push(m);
            break;
        }
    }

    // Ways up, square to the cliff, off the digs.
    let run = land.high.height * RAMP_RUN;
    let mut tries = 0;
    while land.high.ramps.len() < 2 && tries < 200 {
        tries += 1;
        let (p, into) = place(&line, rng.range(0.1, 0.9), 10.0);
        // (Slantwise up it, as like as not.)
        let into = Vec2::from_angle(rng.range(-0.5, 0.5)).rotate(into);
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
                    && land.mounds.iter().all(|m| m.at.distance(q) > 40.0)
                    && land.tower_distance(q) > RAMP_HALF + 8.0
                    && spur
                        .as_ref()
                        .is_none_or(|s| s.shed.0.distance(q) > RAMP_HALF + 30.0)
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
        pad: PAD,
        tracks,
        crossings,
        spur,
        loading,
        ..Skeleton::default()
    };
    if sk.shed_distance(heart) < 16.0 {
        return Err("the Hold site's by the warehouse".into());
    }
    let nav = NavGrid::build(&sk.greybox(sides));
    if !spawns(&mut sk, rng, sides.len(), &nav) {
        return Err("no room to spawn apart".into());
    }
    sk.place_lots(rng, Form::Escarpment);
    // The seam along a wash's floor, a rig cutting it, off the bridges and
    // the fords.
    if let Some(w) = sk.land.washes.first() {
        let (lo, hi) = w.inside(size);
        let mut s = lo + 30.0;
        while s < hi - 30.0 {
            let (p, way, n) = w.frame(s);
            let clear = sk.crossings.iter().all(|x| x.distance(p) > 14.0)
                && sk
                    .land
                    .fords
                    .iter()
                    .all(|&(a, b, h)| crate::map::procgen::mine::seg(p, a, b).0 > h + 9.0);
            if clear {
                sk.seam = (0..=20)
                    .map(|k| {
                        let u = s - 30.0 + 3.0 * k as f32;
                        let (p, _, n) = w.frame(u);
                        p + n * (1.6 * (u * 0.21).sin() - 1.0)
                    })
                    .collect();
                sk.wash_rig = Some((p - n * 3.5, way.x.atan2(way.y)));
                break;
            }
            s += 4.0;
        }
    }
    sk.stands = sk
        .base_spots()
        .iter()
        .map(|&b| b + (heart - b).normalize_or(Vec2::X) * (PAD + 8.0))
        .collect();
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
pub fn skeleton(seed: u64, sides: &[Faction]) -> Skeleton {
    (0..64u64)
        .find_map(|attempt| {
            let mut rng = Rng::new(seed.wrapping_mul(0x9E37_79B9).wrapping_add(attempt) ^ 0x3_D16);
            draw(&mut rng, sides)
                .map_err(|why| log::debug!("mine seed {seed} attempt {attempt}: {why}"))
                .ok()
        })
        .expect("no crystal mine laid out in 64 draws")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mines come at any angle, with one dig or several, each with a hall
    /// both sides can drive into.
    #[test]
    fn mines_lie_at_any_angle() {
        let sides = [Faction::Ithessar, Faction::Resonant];
        let all: Vec<Skeleton> = (0..3).map(|s| skeleton(s, &sides)).collect();
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
        for sk in &all {
            let h = sk.heart;
            assert!(
                h.min_element() > 40.0 && h.max_element() < sk.size - 40.0,
                "{h}"
            );
            let b = sk.base_spots();
            assert!(b[0].distance(b[1]) > sk.size * 0.68, "{b:?}");
        }
    }
}
