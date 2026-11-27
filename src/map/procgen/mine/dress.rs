//! Dressing a mine's skeleton (see `plan`): the outside as a desert is, in
//! its world's rock (`super::land`), and what stands in each dig, placed in the dig's own frame.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::{Vec2, Vec3};

use super::dig::Dig;
use super::{MineStyle, along, land as ground_land, line_at, line_distance, plan, style};
use crate::ai::nav::NavGrid;
use crate::map::procgen::Spec;
use crate::map::procgen::desert;
use crate::map::procgen::foundry::Ground;
use crate::map::procgen::land::{self, Hollow};
use crate::map::procgen::skeleton::{Lot, LotKind, Skeleton, TANK_CLEAR};
use crate::map::{Ambient, Map};
use crate::rng::Rng;

/// How far in from a wall its lamps and gear stand, m.
const WALL: f32 = 0.35;

/// Builds `spec`'s mine on `ground`.
pub fn build(spec: &Spec, ground: Ground) -> (Skeleton, Map) {
    let sk = plan::skeleton(spec.seed, &spec.sides);
    let mut rng = Rng::new(spec.seed ^ 0x5EED_D2E5);
    let st = style(spec.look);
    let sky = ground_land::atmosphere(ground, &mut rng);
    let (pal, loose) = (ground_land::palette(ground), ground_land::loose(ground));
    let (mut map, _) = desert::dressed(&sk, spec, &pal, sky, spec.seed);
    map.hold_name = st.hold_name();
    let mut b = Dig {
        map: &mut map,
        l: &sk,
        st,
        look: spec.look,
        loose,
        inside: true,
    };
    b.railroad();
    for l in sk.lots_of(LotKind::Warehouse) {
        b.warehouse(&mut rng, l);
    }
    for (l, track, s) in loaders(&sk) {
        b.tipple_at(l.at(), l.footprint.axes().1);
        b.loaded_train(track, s);
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
                look: spec.look,
                loose,
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
        look: spec.look,
        loose,
        inside: false,
    };
    for d in &sk.land.digs {
        // A headframe over the hall, on the top.
        let hall = d.rooms[1];
        let p = d.to_map(Vec2::new(
            hall.min.x + (hall.max.x - hall.min.x) * 0.75,
            hall.min.y + 2.0,
        ));
        if sk.land.tower_distance(p) > 8.0 && !sk.keep_clear(p) {
            b.headframe(p);
        }
        // Raw crystal at the cliff's foot by its first way in.
        for x in [0.3, 0.62] {
            let far = if hall.max.x > -hall.min.x {
                hall.max.x
            } else {
                hall.min.x
            } - 5.0;
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
    // (Its own dice: what stands in the mine's the same on any ground.)
    let mut rng = Rng::new(spec.seed ^ 0x0006_2055);
    ground_land::growth(&mut map, &sk, ground, pal.sand, &mut rng, |_| true);
    (sk, map)
}

/// The loaders, each with the track it's over and how far along that.
fn loaders(sk: &Skeleton) -> impl Iterator<Item = (&Lot, &[Vec2], f32)> {
    sk.lots_of(LotKind::Loader).filter_map(|l| {
        let near = |t: &&Vec<Vec2>| line_distance(t, l.at());
        let track = sk
            .tracks
            .iter()
            .min_by(|a, b| near(a).total_cmp(&near(b)))?;
        Some((l, &track[..], along(track, l.at())))
    })
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
    for (_, track, s) in loaders(sk) {
        let (p, _) = line_at(track, s + 18.0);
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
/// are still clear, and the Raid stands.
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
                .find(|&p| {
                    // (Open to drive up to from all round, too.)
                    let round = |r: f32| {
                        (0..24).all(|i| {
                            let a = i as f32 / 24.0 * TAU;
                            !nav.is_blocked(p + Vec2::from_angle(a) * r)
                        })
                    };
                    clear(map, p, 6.0) && !nav.is_blocked(p) && round(4.0)
                })
                .unwrap_or(want)
        })
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

    /// Well inside the rock.
    fn deep(&self, p: Vec2) -> bool {
        self.b.l.land.plateau(self.d.to_map(p)) < -2.0
    }

    fn fill(&mut self, rng: &mut Rng) {
        let rooms = self.d.rooms.clone();
        for r in &rooms {
            self.lamps(r);
        }
        // (The ways in: the first, and the drift to the other.)
        self.sets(&rooms[0]);
        for r in &rooms[self.d.drift_from()..] {
            self.sets(r);
            self.vein(rng, r);
        }
        // The hall: carts and crates along its back wall, off the ways
        // through it; a lamp hung over the middle.
        let hall = rooms[1];
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
        let bay = self.d.rooms[3];
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
            if !self.deep(wall) || self.opens(wall + across * s * 1.0) {
                continue;
            }
            let p = self.d.to_map(wall - across * s * WALL);
            let out = self.way(-across * s);
            let h = self.d.floor + 3.9;
            self.st
                .wall_lamp(&mut self.b, Vec3::new(p.x, h, p.y), out, 1.5);
        }
    }

    /// Steel sets holding up a way in or a drift, along it.
    fn sets(&mut self, r: &Hollow) {
        let size = r.max - r.min;
        let mid = (r.min + r.max) * 0.5;
        let (along, across, len, half) = if size.x > size.y {
            (Vec2::X, Vec2::Y, size.x, size.y * 0.5)
        } else {
            (Vec2::Y, Vec2::X, size.y, size.x * 0.5)
        };
        let n = (len / 6.0) as usize;
        for k in 0..n {
            let c = mid + along * ((k as f32 + 0.5) * 6.0 - n as f32 * 3.0);
            let (a, b) = (c - across * (half - 0.5), c + across * (half - 0.5));
            // Both its legs against rock.
            let held = self.deep(a) && self.deep(b);
            if held && !self.opens(a - across * 1.6) && !self.opens(b + across * 1.6) {
                let (a, b) = (self.d.to_map(a), self.d.to_map(b));
                self.b.steel_set_to(a, b, self.d.floor + r.roof - 0.7);
            }
        }
    }

    /// Raw crystal in a drift's walls.
    fn vein(&mut self, rng: &mut Rng, r: &Hollow) {
        let size = r.max - r.min;
        let mid = (r.min + r.max) * 0.5;
        let (along, across, len, half) = if size.x > size.y {
            (Vec2::X, Vec2::Y, size.x, size.y * 0.5)
        } else {
            (Vec2::Y, Vec2::X, size.y, size.x * 0.5)
        };
        for k in 0..(len / 18.0) as usize {
            let s = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let t = (k as f32 + rng.range(0.2, 0.8)) * 18.0 - len * 0.5;
            let wall = mid + along * t + across * s * half;
            // (Not at a mouth, where the wall falls away.)
            let inside = self.b.l.land.plateau(self.d.to_map(wall)) < -9.0;
            if !inside || self.opens(wall + across * s) {
                continue;
            }
            let at = self.d.to_map(wall + across * s * 0.4);
            let way = self.way(-across * s);
            for _ in 0..3 {
                let y = self.d.floor + rng.range(1.2, 4.2);
                self.b.shard(
                    rng.next_u64(),
                    Vec3::new(at.x, y, at.y),
                    rng.range(0.7, 1.4),
                    0.26,
                    way + way.perp() * rng.range(-0.5, 0.5),
                    rng.range(0.6, 1.0),
                    rng.chance(0.4),
                    false,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::procgen::{Look, check};

    /// Dressed, a mine's as playable as it was laid out: its sides can get
    /// to each other, and into every hall; its halls are lit.
    #[test]
    fn dressed_mines_are_playable() {
        for seed in 0..2 {
            let (sk, mut m) = build(
                &crate::mine_spec(seed, Look::Ithessar, Ground::Desert),
                Ground::Desert,
            );
            crate::map::procgen::ways::lay(&mut m);
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
                let hall = d.rooms[1];
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

    /// The same mine dressed as any faction's: laid out the same, playable,
    /// each its own to look at.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn any_faction_can_have_the_mine() {
        let (sk, theirs) = build(
            &crate::mine_spec(0, Look::Ithessar, Ground::Desert),
            Ground::Desert,
        );
        let line = |m: &Map| m.beams.iter().map(|b| (b.from, b.to)).collect::<Vec<_>>();
        assert!(!theirs.beams.is_empty());
        let mut colors = vec![theirs.beams[0].color];
        for look in [Look::Imperium, Look::Resonant, Look::Uthenn] {
            let (other, m) = build(&crate::mine_spec(0, look, Ground::Desert), Ground::Desert);
            if let Err(why) = check::playable(&m) {
                panic!("{look:?}: {why}");
            }
            assert_eq!(other.base_spots(), sk.base_spots(), "{look:?}");
            assert_eq!(line(&m), line(&theirs), "{look:?}");
            assert!(m.cores.is_empty(), "{look:?}: a plasma core");
            assert!(!colors.contains(&m.beams[0].color), "{look:?}");
            colors.push(m.beams[0].color);
        }
    }

    /// A seed's the same mine on any ground: only its looks differ. Snow
    /// lies on a tundra one's roofs, grass grows round a grassland one.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn grounds_differ_only_in_looks() {
        let built = Ground::ALL.map(|g| build(&crate::mine_spec(1, Look::Imperium, g), g).1);
        let solid = |m: &Map| {
            let of = m.oboxes.iter().filter(|o| o.solid);
            of.map(|o| (o.center, o.yaw, o.half, o.y0, o.y1))
                .collect::<Vec<_>>()
        };
        let [desert, tundra, grass] = &built;
        for m in [tundra, grass] {
            assert!(solid(m) == solid(desert));
            assert_eq!(m.bases, desert.bases);
            assert_eq!(m.repair_spots, desert.repair_spots);
            assert!(m.atmosphere.settle != desert.atmosphere.settle);
            assert!(check::playable(m).is_ok());
        }
        assert!(tundra.oboxes.len() > desert.oboxes.len() + 50);
        assert!(grass.oboxes.len() > desert.oboxes.len() + 50);
    }
}
