//! Dressing a mine's skeleton (see `plan`): the outside as a desert is, in
//! red rock, and what stands in each dig, placed in the dig's own frame.

use std::f32::consts::{FRAC_PI_2, TAU};

use glam::{Vec2, Vec3};

use super::dig::{self, Dig};
use super::{MineStyle, PLANET, atmosphere, plan, style};
use crate::map::Map;
use crate::map::procgen::Spec;
use crate::map::procgen::desert::{self, Palette};
use crate::map::procgen::land::{self, Hollow};
use crate::map::procgen::skeleton::Skeleton;
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
    for (k, d) in sk.land.digs.iter().enumerate() {
        let mut rng = Rng::new(spec.seed.wrapping_mul(0x2545_F491).wrapping_add(k as u64));
        Inside {
            b: Dig {
                map: &mut map,
                l: &sk,
                st,
            },
            d,
            st,
        }
        .fill(&mut rng);
    }
    Some((sk, map))
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
    use crate::ai::nav::NavGrid;
    use crate::map::procgen::check;

    /// Dressed, a mine's as playable as it was laid out: its sides can get
    /// to each other, and into every hall; its halls are lit.
    #[test]
    fn dressed_mines_are_playable() {
        for seed in 0..3 {
            let (sk, m) = build(&crate::mine_spec(seed)).expect("a mine");
            let nav = NavGrid::build(&m);
            if let Err(why) = check::playable_on(&m, &nav) {
                panic!("seed {seed}: {why}");
            }
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
