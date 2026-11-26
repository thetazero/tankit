//! Dressing a mine's skeleton (see `plan`): the outside as a desert is, in
//! red rock, and what stands in each dig, placed in the dig's own frame.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::{Vec2, Vec3};

use super::dig::Dig;
use super::{MineStyle, PLANET, atmosphere, plan, style};
use crate::ai::nav::NavGrid;
use crate::map::procgen::Spec;
use crate::map::procgen::desert::{self, Palette};
use crate::map::procgen::land::{self, Hollow};
use crate::map::procgen::skeleton::{Skeleton, TANK_CLEAR};
use crate::map::{Ambient, Map};
use crate::rng::Rng;

/// Red rock and pale sand.
const PALETTE: Palette = Palette {
    sand: Vec3::new(0.72, 0.55, 0.38),
    low: Vec3::new(0.56, 0.4, 0.27),
    stone: Vec3::new(0.55, 0.32, 0.21),
    stone_dark: Vec3::new(0.36, 0.2, 0.14),
    stone_pale: Vec3::new(0.66, 0.46, 0.33),
    planet: PLANET,
};

/// How far in from a wall its lamps and gear stand, m.
const WALL: f32 = 0.35;

/// Builds `spec`'s mine, if there's one.
pub fn build(spec: &Spec) -> Option<(Skeleton, Map)> {
    let sk = plan::skeleton(spec.seed, &spec.sides)?;
    let mut rng = Rng::new(spec.seed ^ 0x5EED_D2E5);
    let st = style(spec.look);
    let sky = atmosphere(&mut rng);
    let (mut map, _) = desert::dressed(&sk, spec, &PALETTE, sky, spec.seed);
    map.hold_name = st.hold_name();
    let mut b = Dig {
        map: &mut map,
        l: &sk,
        st,
        inside: true,
    };
    if sk.spur.is_some() {
        b.railroad();
        b.warehouse(&mut rng);
    } else {
        for t in &sk.tracks {
            b.rails(t, false);
        }
    }
    if let Some(s) = sk.loading {
        let (at, dir) = sk.track_at(s);
        b.tipple_at(at, dir);
        b.loaded_train(s);
    }
    for x in &sk.crossings {
        b.rail_bridge(x);
    }
    for (k, d) in sk.land.digs.iter().enumerate() {
        let mut rng = Rng::new(spec.seed.wrapping_mul(0x2545_F491).wrapping_add(k as u64));
        Inside {
            b: Dig {
                map: &mut map,
                l: &sk,
                st,
                inside: true,
            },
            d,
            st,
        }
        .fill(&mut rng);
    }
    let mut b = Dig {
        map: &mut map,
        l: &sk,
        st,
        inside: false,
    };
    let rocks = b.seam(&mut rng);
    for (p, h) in rocks {
        b.rock_at(p, h);
    }
    if let Some((p, yaw)) = sk.wash_rig {
        let g = b.ground_height(p.x, p.y);
        b.cutting_rig(p, yaw, g);
    }
    for d in &sk.land.digs {
        // A headframe over the hall, on the top.
        let hall = d.rooms[2];
        let p = d.to_map(Vec2::new(
            hall.min.x + (hall.max.x - hall.min.x) * 0.75,
            hall.min.y + 2.0,
        ));
        if sk.land.tower_distance(p) > 8.0 && !sk.keep_clear(p) {
            b.headframe(p);
        }
        // Raw crystal at the cliff's foot between its ways in.
        for x in [0.3, 0.62] {
            let far = (d.rooms[1].min.x + d.rooms[1].max.x) * 0.5;
            let p = d.to_map(Vec2::new(far * x, -4.0));
            if !sk.keep_clear(p) {
                b.outcrop(&mut rng, p, rng_height(x));
            }
        }
    }
    for t in &sk.land.towers {
        for k in 0..7 {
            let a = t.phase + k as f32 * TAU / 7.0;
            let out = Vec2::from_angle(a);
            let mut r = t.radius * 0.7;
            while t.sdf(t.center + out * r) < 0.5 {
                r += 0.5;
            }
            let p = t.center + out * (r + 1.5);
            if sk.land.plateau(p) < -6.0 && !sk.keep_clear(p) {
                b.outcrop(&mut rng, p, 2.6 + (k % 3) as f32);
            }
        }
    }
    for m in &sk.land.mounds {
        for (off, yaw) in [(Vec2::new(-6.0, 4.0), 0.4), (Vec2::new(5.0, -3.0), 1.9)] {
            b.ore_cart(&mut rng, m.at + off, yaw);
        }
        for (off, size) in [(Vec2::new(8.0, 7.0), 1.6), (Vec2::new(-9.0, -6.0), 1.8)] {
            b.rock_pile(&mut rng, m.at + off, size);
        }
    }
    for (k, &at) in sk.base_spots().iter().enumerate() {
        // Away from the fight, off the spawns.
        let out = (at - sk.heart).normalize_or(Vec2::X);
        let spot = [1.0, 0.0, -1.0]
            .map(|t: f32| at + Vec2::from_angle(t * 1.3).rotate(out) * 19.0)
            .into_iter()
            .find(|p| {
                p.min_element() > 14.0
                    && p.max_element() < sk.size - 14.0
                    && sk.land.plateau(*p).abs() > 10.0
            });
        if let Some(p) = spot {
            st.base(&mut b, k, p, &mut rng);
        }
    }
    sounds(&mut map, &sk, st);
    settle(&mut map, &sk);
    Some((sk, map))
}

/// What sounds: lit crystal, the borers, the train, the wash under a bridge.
fn sounds(map: &mut Map, sk: &Skeleton, st: &dyn MineStyle) {
    let lit: Vec<Vec3> = map
        .crystals
        .iter()
        .filter(|c| c.lit && c.radius > 0.6)
        .map(|c| c.base + c.axis * 0.35)
        .collect();
    for at in lit {
        map.sound(Ambient::Crystal, at);
    }
    for d in &sk.land.digs {
        let Some((from, to)) = d.beam else { continue };
        for (kind, p) in [(Ambient::Machine, from), (st.strike_sound(), to)] {
            let p = d.to_map(p);
            map.sound(kind, Vec3::new(p.x, d.floor + 2.5, p.y));
        }
    }
    if let Some(s) = sk.loading {
        let (p, _) = sk.track_at(s + 18.0);
        let g = map.ground_height(p.x, p.y);
        map.sound(Ambient::Hull, Vec3::new(p.x, g + 2.5, p.y));
    }
    for x in &sk.crossings {
        let p = (x.from + x.to) * 0.5;
        let g = map.ground_height(p.x, p.y);
        map.sound(Ambient::River, Vec3::new(p.x, g + 0.5, p.y));
    }
}

/// Once everything's built: the spawns, drops and places bots head for that
/// are still clear, the Raid stands, and the ways round worth taking.
fn settle(map: &mut Map, sk: &Skeleton) {
    let nav = NavGrid::build(map);
    let clear = |m: &Map, p: Vec2, r: f32| {
        let g = m.ground_height(p.x, p.y);
        !m.oboxes.iter().any(|o| {
            let (lo, hi) = o.y_range();
            o.solid && lo < g + 3.0 && hi > g + 0.3 && o.distance(p) < r
        }) && !m.crystals.iter().any(|c| c.solid && c.distance(p) < r)
    };
    for k in 0..map.bases.len() {
        let mut spots = std::mem::take(&mut map.bases[k]);
        spots.retain(|&p| clear(map, p, TANK_CLEAR));
        map.bases[k] = spots;
    }
    let mut spots = std::mem::take(&mut map.open_spots);
    spots.retain(|&p| clear(map, p, TANK_CLEAR));
    map.open_spots = spots;
    let mut spots = std::mem::take(&mut map.repair_spots);
    spots.retain(|&p| clear(map, p, TANK_CLEAR));
    // (Rolling ground leaves few level places: more of them, apart.)
    for &p in &map.open_spots {
        if spots.len() >= 4 {
            break;
        }
        let g = map.ground_height(p.x, p.y);
        let level = (0..8).all(|k| {
            let q = p + Vec2::from_angle(k as f32 * TAU / 8.0) * 4.0;
            (map.ground_height(q.x, q.y) - g).abs() < 0.8
        });
        let up = Vec3::new(p.x, g + 0.5, p.y);
        let sky = !map.oboxes.iter().any(|o| o.ray_hit(up, Vec3::Y, 80.0));
        let apart = spots.iter().all(|q| q.distance(p) > 55.0)
            && map.bases.iter().flatten().all(|q| q.distance(p) > 40.0);
        if level && sky && apart {
            spots.push(p);
        }
    }
    map.repair_spots = spots;
    map.raid_stands = sk
        .stands
        .iter()
        .map(|&want| {
            (0..300)
                .map(|k| want + Vec2::from_angle(k as f32 * 2.4) * (k as f32 * 0.08))
                .find(|&p| clear(map, p, 6.0) && !nav.is_blocked(p))
                .unwrap_or(want)
        })
        .collect();
    let mut ways: Vec<Vec<Vec3>> = Vec::new();
    for d in &sk.land.digs {
        // In at one mouth, through the hall, out at the other.
        let (hall, far) = (d.rooms[2], d.rooms[1]);
        let mid = hall.min.y + (hall.max.y - hall.min.y) * 0.25;
        let x = (far.min.x + far.max.x) * 0.5;
        let at = |x: f32, y: f32| {
            let p = d.to_map(Vec2::new(x, y));
            Vec3::new(p.x, d.floor, p.y)
        };
        ways.push(vec![
            at(0.0, -12.0),
            at(0.0, 8.0),
            at(0.0, mid),
            at(x * 0.5, mid),
            at(x, mid),
            at(x, 8.0),
            at(x, -12.0),
        ]);
    }
    for x in &sk.crossings {
        let on = |t: f32| {
            let p = x.from.lerp(x.to, t);
            Vec3::new(p.x, x.top, p.y)
        };
        ways.push(vec![on(-0.15), on(0.2), on(0.8), on(1.15)]);
    }
    for r in &sk.land.high.ramps {
        let on = |p: Vec2| Vec3::new(p.x, map.ground_height(p.x, p.y), p.y);
        let dir = (r.top - r.foot).normalize();
        ways.push(vec![
            on(r.foot - dir * 4.0),
            on(r.foot.lerp(r.top, 0.5)),
            on(r.top + dir * 6.0),
        ]);
    }
    // Only what a bot can drive, each point on its floor.
    map.routes = ways
        .into_iter()
        .filter(|w| w.iter().all(|&p| nav.floor_at(p).is_some()))
        .collect();
}

/// How tall the crystal `x` of the way along a dig's foot stands.
fn rng_height(x: f32) -> f32 {
    2.4 + 2.0 * x
}

/// A dig being filled.
struct Inside<'a, 'b> {
    b: Dig<'a>,
    d: &'b land::Dig,
    st: &'b dyn MineStyle,
}

impl Inside<'_, '_> {
    /// A direction of the dig's frame, on the map.
    fn way(&self, v: Vec2) -> Vec2 {
        self.d.to_map(v) - self.d.to_map(Vec2::ZERO)
    }

    /// Inside some other room than `r`, or clear of the rock.
    fn opens(&self, p: Vec2) -> bool {
        self.d.distance(self.d.to_map(p)) < -0.2
    }

    fn fill(&mut self, rng: &mut Rng) {
        let rooms = self.d.rooms.clone();
        for r in &rooms {
            self.lamps(r);
        }
        for r in rooms.iter().filter(|r| r.min.y < 0.0) {
            self.sets(r);
        }
        // The hall: carts and crates along its back wall, off the ways
        // through it; a lamp hung over the middle.
        let hall = rooms[2];
        let mid = (hall.min + hall.max) * 0.5;
        let at = self.d.to_map(mid);
        self.st.hanging_lamp(
            &mut self.b,
            Vec3::new(at.x, self.d.floor + hall.roof, at.y),
            1.6,
            1.4,
        );
        let into = self.way(Vec2::Y);
        let yaw = into.x.atan2(into.y);
        if let Some((from, to)) = self.d.beam {
            self.borer(rng, from, to);
        }
        let quarter = (hall.max.x - hall.min.x) * 0.25;
        for s in [-1.0, 1.0] {
            let p = Vec2::new(mid.x + quarter * s, hall.max.y - 2.6);
            // (The heading may go on from there.)
            if self.opens(p + Vec2::Y * 4.0) {
                continue;
            }
            let at = self.d.to_map(p);
            if rng.chance(0.5) {
                self.b.ore_cart(rng, at, yaw + FRAC_PI_2);
            } else {
                self.b.block_stack(at, yaw);
            }
        }
    }

    /// The borer's bay: the machine and its beam, spoil either side of the
    /// bore, raw crystal in the corners away from it, a floodlight on it.
    fn borer(&mut self, rng: &mut Rng, from: Vec2, to: Vec2) {
        let f = self.d.floor + 2.1;
        let at3 = |p: Vec2| {
            let p = self.d.to_map(p);
            Vec3::new(p.x, f, p.y)
        };
        let beam = (at3(from), at3(to), 0.55);
        self.st.borer(&mut self.b, beam, rng);
        let bay = self.d.rooms[4];
        let side = (to.x - from.x).signum();
        // The wall the bore's in, and the one across from it.
        let (wall, back) = if side > 0.0 {
            (bay.max.x, bay.min.x)
        } else {
            (bay.min.x, bay.max.x)
        };
        for (dx, dy, size) in [(2.2, -6.6, 1.2), (4.5, -8.0, 0.8), (2.0, 6.8, 1.1)] {
            let p = self.d.to_map(Vec2::new(wall - side * dx, from.y + dy));
            self.b.rock_pile(rng, p, size);
        }
        // Clusters growing out of the walls.
        for k in 0..6 {
            let t = (k as f32 + 0.5) / 6.0;
            let (p, out) = if k % 2 == 0 {
                (
                    Vec2::new(back, bay.min.y + (bay.max.y - bay.min.y) * t),
                    Vec2::X * side,
                )
            } else {
                (
                    Vec2::new(bay.min.x + (bay.max.x - bay.min.x) * t, bay.max.y),
                    -Vec2::Y,
                )
            };
            let at = self.d.to_map(p - out * 0.4);
            let way = self.way(out);
            for _ in 0..4 {
                let y = self.d.floor + rng.range(1.2, 5.5);
                self.b.shard(
                    rng.next_u64(),
                    Vec3::new(at.x, y, at.y),
                    rng.range(0.7, 1.6),
                    0.28,
                    way + way.perp() * rng.range(-0.5, 0.5),
                    rng.range(0.6, 1.0),
                    rng.chance(0.6),
                    false,
                );
            }
        }
        for (dx, y, h) in [(3.0, bay.max.y - 3.0, 3.4), (2.8, bay.min.y + 3.2, 2.4)] {
            let p = self.d.to_map(Vec2::new(back + side * dx, y));
            self.b.outcrop(rng, p, h);
        }
        let machine = self.d.to_map(from - Vec2::X * side * 5.8);
        let stand = self.d.to_map(Vec2::new(wall - side * 2.0, bay.max.y - 2.2));
        self.b.floodlight(stand, machine);
    }

    /// Lamps along a room's two long walls by turns, where there's wall.
    fn lamps(&mut self, r: &Hollow) {
        let size = r.max - r.min;
        let mid = (r.min + r.max) * 0.5;
        let (along, across, len, half) = if size.x > size.y {
            (Vec2::X, Vec2::Y, size.x, size.y * 0.5)
        } else {
            (Vec2::Y, Vec2::X, size.y, size.x * 0.5)
        };
        let n = (len / 11.0).round().max(1.0) as usize;
        for k in 0..n {
            let t = ((k as f32 + 0.5) / n as f32 - 0.5) * len;
            let s = if k % 2 == 0 { 1.0 } else { -1.0 };
            let wall = mid + along * t + across * s * half;
            // Not where another room opens through it, nor out in the air.
            if wall.y < 3.0 || self.opens(wall + across * s * 1.0) {
                continue;
            }
            let p = self.d.to_map(wall - across * s * WALL);
            let out = self.way(-across * s);
            let h = self.d.floor + 3.9;
            self.st
                .wall_lamp(&mut self.b, Vec3::new(p.x, h, p.y), out, 1.5);
        }
    }

    /// Steel sets holding up a way in.
    fn sets(&mut self, r: &Hollow) {
        let mut y = 3.0;
        while y < r.max.y - 2.0 {
            let (a, b) = (Vec2::new(r.min.x + 0.5, y), Vec2::new(r.max.x - 0.5, y));
            // Both its legs against rock.
            if !self.opens(a - Vec2::X * 1.6) && !self.opens(b + Vec2::X * 1.6) {
                let (a, b) = (self.d.to_map(a), self.d.to_map(b));
                self.b.steel_set_to(a, b, self.d.floor + r.roof - 0.7);
            }
            y += 6.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::procgen::check;

    /// Dressed, a mine's as playable as it was laid out: its sides can get
    /// to each other, and into every hall; its halls are lit.
    #[test]
    fn dressed_mines_are_playable() {
        for seed in 0..2 {
            let (sk, m) = build(&crate::mine_spec(seed, crate::map::procgen::Look::Ithessar))
                .expect("a mine");
            let nav = NavGrid::build(&m);
            if let Err(why) = check::playable_on(&m, &nav) {
                panic!("seed {seed}: {why}");
            }
            let dug = m
                .routes
                .iter()
                .filter(|w| w.iter().any(|p| m.ground_height(p.x, p.z) > p.y + 5.0));
            assert_eq!(
                dug.count(),
                sk.land.digs.len(),
                "seed {seed}: ways through digs"
            );
            assert_eq!(m.raid_stands.len(), 2, "seed {seed}");
            assert!(
                m.repair_spots.len() >= 3,
                "seed {seed}: {:?}",
                m.repair_spots
            );
            for d in &sk.land.digs {
                let hall = d.rooms[2];
                let p = d.to_map((hall.min + hall.max) * 0.5);
                let to = Vec3::new(p.x, d.floor, p.y);
                for b in sk.base_spots() {
                    let from = Vec3::new(b.x, m.ground_height(b.x, b.y), b.y);
                    let way = nav.route(from, to);
                    assert!(
                        way.last().is_some_and(|w| w.y < d.floor + 1.0),
                        "seed {seed}: no way into the hall at {p}"
                    );
                }
                let lit = m.lights.iter().filter(|l| {
                    let q = Vec2::new(l.pos.x, l.pos.z);
                    d.distance(q) < 0.5 && l.pos.y < d.floor + 9.0
                });
                assert!(lit.count() >= 4, "seed {seed}: a dark dig");
            }
        }
    }
}
