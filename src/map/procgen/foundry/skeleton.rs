//! A hull foundry's skeleton: the works in placeholders, before anyone's
//! architecture is on it. Laid out one way round (the whole map's turned and
//! mirrored by its seed once it's dressed): the rail yard along the south,
//! the stockyard under its gantries north of it, the halls in a row across
//! the middle (the great hall between two others, or between two casting
//! yards), the slag heaps in the north, a side's spawns at each end.
//!
//! East mirrors west but for what's small (cover, what's fallen in), so the
//! sides' drives to the middle match.

use glam::{Vec2, Vec3};

use crate::map::procgen::skeleton::smoothstep;
use crate::map::{Map, OBox, Terrain};
use crate::render::material::Material;
use crate::rng::Rng;

/// Clearance a spawn needs from anything solid (the Gravis's corner radius).
pub const TANK_CLEAR: f32 = 4.6;
/// How thick a hall's walls are.
pub const WALL_T: f32 = 1.2;
/// The top of a hall's doorways.
pub const DOOR_H: f32 = 8.0;
/// The ground kept clear round a side's spawns (a half width).
pub const PAD: f32 = 16.0;
/// A wagon, half its size and how tall.
pub const WAGON: Vec2 = Vec2::new(4.5, 1.45);
pub const WAGON_H: f32 = 3.0;
/// How wide half a gantry's leg is: along its runway, and across it.
pub const LEG: Vec2 = Vec2::new(1.6, 0.8);
/// The works' wall, half as thick, and how tall.
pub const FENCE_T: f32 = 0.8;
pub const FENCE_H: f32 = 5.0;

/// A wall of a hall: which way it faces.
pub const WEST: usize = 0;
pub const EAST: usize = 1;
pub const SOUTH: usize = 2;
pub const NORTH: usize = 3;

pub fn out(side: usize) -> Vec2 {
    [Vec2::NEG_X, Vec2::X, Vec2::NEG_Y, Vec2::Y][side]
}

/// A way through a hall's wall, `at` m along it from its middle.
#[derive(Clone, Copy, Debug)]
pub struct Door {
    pub side: usize,
    pub at: f32,
    pub half: f32,
}

#[derive(Clone, Debug)]
pub struct Hall {
    pub center: Vec2,
    pub half: Vec2,
    pub h: f32,
    pub doors: Vec<Door>,
    pub panels: Vec<Panel>,
    /// The roof's bays along x: which still have their roof on.
    pub bays: Vec<bool>,
}

/// A stretch of a hall's wall between its doors, standing to `top`.
#[derive(Clone, Copy, Debug)]
pub struct Panel {
    pub center: Vec2,
    pub half: Vec2,
    pub out: Vec2,
    pub top: f32,
    pub broken: bool,
}

impl Panel {
    /// Along the wall.
    pub fn along(&self) -> Vec2 {
        self.out.perp().abs()
    }

    /// Half its length along the wall.
    pub fn reach(&self) -> f32 {
        self.half.dot(self.along())
    }
}

impl Hall {
    pub fn contains(&self, p: Vec2, margin: f32) -> bool {
        ((p - self.center).abs() - self.half).max_element() < margin
    }

    /// Where a wall's middle stands, and how long half of it is.
    pub fn wall(&self, side: usize) -> (Vec2, f32) {
        let o = out(side);
        let mid = self.center + o * (self.half - Vec2::splat(WALL_T * 0.5));
        (mid, self.half.dot(o.perp().abs()))
    }

    /// A doorway's middle.
    pub fn door(&self, d: &Door) -> Vec2 {
        self.wall(d.side).0 + out(d.side).perp().abs() * d.at
    }

    /// Each wall cut into panels between its doors. Some are broken down:
    /// more of them, the more `ruin` (0..1).
    fn build_walls(&mut self, rng: &mut Rng, ruin: f32) {
        for side in 0..4 {
            let (mid, half) = self.wall(side);
            let along = out(side).perp().abs();
            // A long wall runs its whole length; the ends fit between them.
            let half = if side < 2 { half - WALL_T } else { half };
            let mut gaps: Vec<(f32, f32)> = self
                .doors
                .iter()
                .filter(|d| d.side == side)
                .map(|d| (d.at - d.half, d.at + d.half))
                .collect();
            gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
            gaps.push((half, half));
            let mut from = -half;
            for (lo, hi) in gaps {
                let len = lo - from;
                let n = (len / 6.0).round().max(1.0) as usize;
                for k in 0..n {
                    if len < 0.1 {
                        break;
                    }
                    let seg = len / n as f32;
                    let t = from + seg * (k as f32 + 0.5);
                    let broken = rng.chance(ruin);
                    let top = if broken {
                        rng.range(3.4, self.h * 0.75)
                    } else {
                        self.h
                    };
                    self.panels.push(Panel {
                        center: mid + along * t,
                        half: along * seg * 0.5 + out(side).abs() * WALL_T * 0.5,
                        out: out(side),
                        top,
                        broken,
                    });
                }
                from = hi;
            }
        }
        let bays = (self.half.x / 4.0).round() as usize;
        self.bays = (0..bays).map(|_| rng.chance(0.75 - ruin)).collect();
    }
}

/// A crane on a portal over a yard: its legs `span` m either side of `at`
/// along z, the beam between them over everything.
#[derive(Clone, Copy, Debug)]
pub struct Gantry {
    pub at: Vec2,
    pub span: f32,
    pub h: f32,
    /// How far along x its runway rails go either way.
    pub run: f32,
    /// Where the hoist stands along the beam (-1..1).
    pub hoist: f32,
}

impl Gantry {
    pub fn legs(&self) -> [Vec2; 2] {
        [-1.0f32, 1.0].map(|s| self.at + Vec2::Y * s * self.span)
    }
}

/// What's standing about in the yards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StockKind {
    /// A hull, cast but not finished.
    Hull,
    /// Billets, stacked.
    Billets,
    /// A slag pot or a ladle.
    Pot,
}

/// Something standing in a yard: its solid parts keep to `half` round `at`.
#[derive(Clone, Copy, Debug)]
pub struct Stock {
    pub at: Vec2,
    pub half: Vec2,
    pub kind: StockKind,
}

#[derive(Clone, Copy, Debug)]
pub struct Wagon {
    pub at: Vec2,
    pub loaded: bool,
}

/// A heap of slag: the ground itself, gentle enough to drive over.
#[derive(Clone, Copy, Debug)]
pub struct Slag {
    pub at: Vec2,
    pub radius: f32,
    pub height: f32,
}

/// Something round, `r` about (a furnace, a chimney).
#[derive(Clone, Copy, Debug)]
pub struct Round {
    pub at: Vec2,
    pub r: f32,
    pub h: f32,
}

#[derive(Clone, Debug)]
pub struct Skeleton {
    /// Arena spans [0, size] on x and z.
    pub size: f32,
    /// The first is the great hall.
    pub halls: Vec<Hall>,
    pub furnaces: Vec<Round>,
    pub stacks: Vec<Round>,
    pub gantries: Vec<Gantry>,
    pub stock: Vec<Stock>,
    /// Each track's z, south to north: they run the arena's width.
    pub tracks: Vec<f32>,
    pub wagons: Vec<Wagon>,
    pub slag: Vec<Slag>,
    /// Each side's spawns: the west's, the east's.
    pub bases: [Vec2; 2],
    /// The great hall's casting floor: the Hold site.
    pub hold: Vec2,
    pub repair_spots: Vec<Vec2>,
}

impl Skeleton {
    pub fn generate(rng: &mut Rng) -> Skeleton {
        let size = 4.0 * rng.int(60, 64) as f32;
        let mid = size * 0.5;
        let flip = |p: Vec2| Vec2::new(size - p.x, p.y);

        let count = rng.int(3, 4);
        let pitch = rng.range(10.5, 11.5);
        let tracks: Vec<f32> = (0..count).map(|k| 13.0 + k as f32 * pitch).collect();
        let yard_top = tracks[tracks.len() - 1] + 8.0;

        // The great hall, the furnaces along its north wall, its casting
        // floor south of them.
        let half = Vec2::new(rng.range(30.0, 34.0), rng.range(21.0, 24.0));
        let span = rng.range(11.0, 13.0);
        let strip = yard_top + span;
        let center = Vec2::new(mid, strip + span + rng.range(11.0, 14.0) + half.y);
        let mut doors = Vec::new();
        for side in [WEST, EAST] {
            doors.push(Door {
                side,
                at: -5.5,
                half: 5.5,
            });
        }
        let south = rng.range(13.0, 17.0);
        for s in [-1.0, 1.0] {
            doors.push(Door {
                side: SOUTH,
                at: s * south,
                half: 5.0,
            });
            doors.push(Door {
                side: NORTH,
                at: s * 10.5,
                half: 4.6,
            });
        }
        let mut halls = vec![Hall {
            center,
            half,
            h: rng.range(15.0, 18.0),
            doors,
            panels: Vec::new(),
            bays: Vec::new(),
        }];
        let furnaces: Vec<Round> = [-21.0, 0.0, 21.0]
            .map(|x| Round {
                at: center + Vec2::new(x, half.y - 7.0),
                r: 4.6,
                h: 11.0,
            })
            .to_vec();
        let stack_h = rng.range(28.0, 38.0);
        let mut stacks: Vec<Round> = furnaces
            .iter()
            .map(|f| Round {
                at: Vec2::new(f.at.x, center.y + half.y + 3.6),
                r: 1.9,
                h: stack_h + (f.at.x - mid).abs() * -0.25,
            })
            .collect();

        // Either side of it: a hall, or a casting yard under a gantry. The
        // same on both sides, so neither's way in is the harder.
        let lane = rng.range(14.0, 16.5);
        let flank = Vec2::new(rng.range(13.0, 16.0), rng.range(17.0, 23.0));
        let shift = rng.range(-6.0, 6.0);
        let flank_at = center + Vec2::new(-(half.x + lane + flank.x), shift);
        let halls_flank = rng.chance(0.6);
        let mut gantries = Vec::new();
        if halls_flank {
            let h = rng.range(10.0, 12.5);
            let z = rng.range(-5.0, 5.0);
            for s in [-1.0f32, 1.0] {
                let doors = vec![
                    Door {
                        side: WEST,
                        at: z,
                        half: 5.0,
                    },
                    Door {
                        side: EAST,
                        at: z,
                        half: 5.0,
                    },
                    Door {
                        side: SOUTH,
                        at: 0.0,
                        half: 4.6,
                    },
                    Door {
                        side: NORTH,
                        at: 0.0,
                        half: 4.6,
                    },
                ];
                halls.push(Hall {
                    center: if s < 0.0 { flank_at } else { flip(flank_at) },
                    half: flank,
                    h,
                    doors,
                    panels: Vec::new(),
                    bays: Vec::new(),
                });
            }
            // One chimney to each.
            let c = flank_at + Vec2::new(-flank.x + 3.0, flank.y + 3.2);
            for at in [c, flip(c)] {
                stacks.push(Round {
                    at,
                    r: 1.5,
                    h: rng.range(19.0, 24.0),
                });
            }
        } else {
            for at in [flank_at, flip(flank_at)] {
                gantries.push(Gantry {
                    at,
                    span: flank.y.min(18.0),
                    h: 12.0,
                    run: flank.x - 2.0,
                    hoist: rng.range(-0.6, 0.6),
                });
            }
        }
        let ruin = rng.range(0.1, 0.28);
        for h in &mut halls {
            h.build_walls(rng, ruin);
        }

        // The stockyard's gantries, over the strip between the rails and
        // the halls.
        let pairs: &[f32] = if rng.chance(0.5) {
            &[26.0]
        } else {
            &[22.0, 64.0]
        };
        let gantry_h = rng.range(11.0, 14.0);
        for &d in pairs {
            for s in [-1.0f32, 1.0] {
                let x = d + rng.range(-5.0, 5.0);
                gantries.push(Gantry {
                    at: Vec2::new(mid + s * x, strip),
                    span,
                    h: gantry_h,
                    run: 14.0,
                    hoist: rng.range(-0.7, 0.7),
                });
            }
        }

        let z = center.y + rng.range(-8.0, 8.0);
        let bases = [Vec2::new(24.0, z), Vec2::new(size - 24.0, z)];
        let mut sk = Skeleton {
            size,
            hold: center - Vec2::new(0.0, 6.0),
            halls,
            furnaces,
            stacks,
            gantries,
            stock: Vec::new(),
            tracks,
            wagons: Vec::new(),
            slag: Vec::new(),
            bases,
            repair_spots: Vec::new(),
        };
        sk.heap_slag(rng);
        sk.shunt(rng);
        sk.lay_stock(rng);
        sk.drop_repairs(rng);
        sk
    }

    /// The halls' north edge.
    fn works_top(&self) -> f32 {
        self.halls
            .iter()
            .map(|h| h.center.y + h.half.y)
            .fold(0.0, f32::max)
    }

    /// Slag, tipped in heaps across the north: one astride the middle, or a
    /// pair, and more out toward the ends.
    fn heap_slag(&mut self, rng: &mut Rng) {
        let (s, mid) = (self.size, self.size * 0.5);
        let z0 = self.works_top() + 12.0;
        let depth = s - z0;
        let mut put = |x: f32, r: f32, z: f32| {
            // Its foot stays off the halls.
            self.slag.push(Slag {
                at: Vec2::new(x, z.max(z0 + r)),
                radius: r,
                height: r * 0.28,
            });
        };
        let most = (depth * 0.45).min(28.0);
        let mut x = if rng.chance(0.5) {
            put(mid, most * rng.range(0.85, 1.0), z0 + depth * 0.5);
            most * 2.0
        } else {
            rng.range(22.0, 28.0)
        };
        while x < mid - 24.0 {
            let r = rng.range(0.6, 0.95) * most;
            let z = z0 + depth * rng.range(0.4, 0.75);
            put(mid - x, r, z);
            put(mid + x, r, z);
            x += r + rng.range(16.0, 24.0);
        }
    }

    /// Wagons standing on the tracks in short strings, gaps between them
    /// to drive through.
    fn shunt(&mut self, rng: &mut Rng) {
        let tracks = self.tracks.clone();
        for z in tracks {
            let mut x = rng.range(16.0, 30.0);
            loop {
                let cars = rng.int(1, 3);
                for _ in 0..cars {
                    if x + WAGON.x * 2.0 > self.size * 0.5 - 4.0 {
                        break;
                    }
                    let c = x + WAGON.x;
                    for at in [c, self.size - c] {
                        self.wagons.push(Wagon {
                            at: Vec2::new(at, z),
                            loaded: rng.chance(0.7),
                        });
                    }
                    x += WAGON.x * 2.0 + 0.9;
                }
                x += rng.range(11.0, 22.0);
                if x > self.size * 0.5 - 14.0 {
                    break;
                }
            }
        }
    }

    /// Room for something `half` big at `p`, with `gap` m clear round it.
    fn room(&self, p: Vec2, half: Vec2, gap: f32) -> bool {
        let r = half.max_element();
        let apart = |q: Vec2, h: Vec2| ((p - q).abs() - h - half).max_element() > gap;
        let inside = p.min_element() > r + 8.0 && p.max_element() < self.size - r - 8.0;
        // Not on the slag: it's cover itself, and a slope.
        let level = [-1.0f32, 0.0, 1.0].iter().all(|&x| {
            [-1.0f32, 0.0, 1.0]
                .iter()
                .all(|&z| self.ground(p + half * Vec2::new(x, z)) < 0.25)
        });
        inside
            && level
            && self
                .bases
                .iter()
                .all(|&b| apart(b, Vec2::splat(PAD)) && {
                    // The way out of it toward the middle.
                    let to = b.lerp(self.hold, 0.25);
                    apart(to, Vec2::new(12.0, 5.0))
                })
            && self.stock.iter().all(|s| apart(s.at, s.half))
            && self.wagons.iter().all(|w| apart(w.at, WAGON))
            && self
                .gantries
                .iter()
                .flat_map(|g| g.legs())
                .all(|l| apart(l, LEG))
            && self
                .stacks
                .iter()
                .chain(&self.furnaces)
                .all(|f| apart(f.at, Vec2::splat(f.r)))
            && self.halls.iter().all(|h| {
                h.panels.iter().all(|w| apart(w.center, w.half))
                    && h.doors.iter().all(|d| {
                        let o = out(d.side);
                        apart(
                            h.door(d),
                            o.perp().abs() * (d.half - 1.0) + o.abs() * 10.0 - Vec2::splat(gap),
                        )
                    })
            })
            // The Hold site, most of it open.
            && p.distance(self.hold) > 11.0 + r
    }

    /// Hulls, billets and pots standing about: rows of them under the
    /// gantries and in the halls, then wherever the ground's bare.
    fn lay_stock(&mut self, rng: &mut Rng) {
        let size = self.size;
        let shape = |rng: &mut Rng| match rng.index(3) {
            0 => (StockKind::Hull, Vec2::new(3.2, 1.9)),
            1 => (StockKind::Billets, Vec2::new(2.4, 1.3)),
            _ => (StockKind::Pot, Vec2::splat(1.7)),
        };
        // Where, and how close together.
        let mut spots: Vec<(Vec2, Vec2, i32)> = Vec::new();
        for g in &self.gantries {
            spots.push((g.at, Vec2::new(g.run + 2.0, g.span - 3.0), 5));
        }
        for h in &self.halls {
            spots.push((h.center, h.half - Vec2::splat(3.0), 4));
        }
        for (c, reach, want) in spots {
            let mut put = 0;
            for _ in 0..80 {
                let p = c + Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * reach;
                let (kind, half) = shape(rng);
                if put < want && self.room(p, half, 5.2) {
                    self.stock.push(Stock { at: p, half, kind });
                    put += 1;
                }
            }
        }
        // Then cover wherever there's none near.
        for _ in 0..900 {
            let p = Vec2::new(rng.range(0.0, size), rng.range(0.0, size));
            let (kind, half) = shape(rng);
            let bare = self.cover_distance(p) > 13.0;
            if bare && self.room(p, half, 5.2) {
                self.stock.push(Stock { at: p, half, kind });
            }
        }
    }

    /// How far `p` is from anything to hide behind.
    pub fn cover_distance(&self, p: Vec2) -> f32 {
        let d = |q: Vec2, h: Vec2| ((p - q).abs() - h).max(Vec2::ZERO).length();
        let stock = self.stock.iter().map(|s| d(s.at, s.half));
        let wagons = self.wagons.iter().map(|w| d(w.at, WAGON));
        let legs = self
            .gantries
            .iter()
            .flat_map(|g| g.legs())
            .map(|l| d(l, LEG));
        let walls = self
            .halls
            .iter()
            .flat_map(|h| &h.panels)
            .map(|w| d(w.center, w.half));
        stock
            .chain(wagons)
            .chain(legs)
            .chain(walls)
            .fold(f32::MAX, f32::min)
    }

    /// Repair drops, under open sky: in the rail yard, behind the halls and
    /// off each end of the great hall.
    fn drop_repairs(&mut self, rng: &mut Rng) {
        let mid = self.size * 0.5;
        let great = &self.halls[0];
        let lane = great.center.x - great.half.x - 7.0;
        let south = great.center.y - great.half.y - 6.0;
        let want = [
            Vec2::new(mid, self.tracks[self.tracks.len() - 1] + 7.0),
            Vec2::new(mid, self.works_top() + 12.0),
            Vec2::new(lane, south),
            Vec2::new(self.size - lane, south),
        ];
        for w in want {
            let found = (0..200).find_map(|k| {
                let q = w + Vec2::from_angle(k as f32 * 2.4) * (k as f32 * 0.12);
                let sky = self.halls.iter().all(|h| !h.contains(q, 3.0))
                    && self
                        .gantries
                        .iter()
                        .all(|g| (q.x - g.at.x).abs() > 6.0 || (q.y - g.at.y).abs() > g.span + 4.0);
                let level = (0..8).all(|j| {
                    let r = q + Vec2::from_angle(j as f32 * 0.785) * 5.0;
                    (self.ground(r) - self.ground(q)).abs() < 0.8
                });
                (sky && level && self.room(q, Vec2::splat(TANK_CLEAR), 0.0)).then_some(q)
            });
            self.repair_spots.extend(found);
        }
        // Which pair come first differs.
        if rng.chance(0.5) {
            self.repair_spots.reverse();
        }
    }

    /// Ground height: flat but for the slag.
    pub fn ground(&self, p: Vec2) -> f32 {
        self.slag
            .iter()
            .map(|s| {
                let d = p - s.at;
                let a = d.y.atan2(d.x);
                // Not quite round.
                let r = s.radius * (0.94 + 0.06 * (3.0 * a + s.at.x).sin());
                s.height * (1.0 - smoothstep(r * 0.22, r, d.length()))
            })
            .fold(0.0, f32::max)
    }

    pub fn terrain(&self) -> Terrain {
        let n = (self.size / 2.0).round() as usize + 1;
        Terrain::from_fn(Vec2::ZERO, 2.0, n, n, |x, z| self.ground(Vec2::new(x, z)))
    }

    /// Where a side's tanks spawn.
    pub fn spawn_points(&self, side: usize) -> Vec<Vec2> {
        let c = self.bases[side];
        (0..25)
            .map(|k| c + Vec2::new((k % 5) as f32 - 2.0, (k / 5) as f32 - 2.0) * 5.5)
            .collect()
    }

    /// The works' wall, all round the edge: (middle, half) of each panel,
    /// the way in, and whether a track runs out under it (a gate).
    pub fn fence(&self) -> Vec<(Vec2, Vec2, Vec2, bool)> {
        let s = self.size;
        let n = (s / 6.0).round() as usize;
        let seg = s / n as f32;
        let mut v = Vec::new();
        for (inward, start, along) in [
            (Vec2::Y, Vec2::new(0.0, 0.0), Vec2::X),
            (-Vec2::Y, Vec2::new(0.0, s), Vec2::X),
            (Vec2::X, Vec2::new(0.0, 0.0), Vec2::Y),
            (-Vec2::X, Vec2::new(s, 0.0), Vec2::Y),
        ] {
            for k in 0..n {
                let c = start + along * seg * (k as f32 + 0.5) + inward * 0.3;
                let half = along * seg * 0.5 + inward.abs() * FENCE_T;
                let gate =
                    along == Vec2::Y && self.tracks.iter().any(|z| (z - c.y).abs() < seg * 0.5);
                v.push((c, half, inward, gate));
            }
        }
        v
    }

    /// The fence's foot and top at `c`: `half` along it.
    pub fn fence_y(&self, c: Vec2, half: Vec2) -> (f32, f32) {
        let hs = [-1.0f32, 0.0, 1.0].map(|t| self.ground(c + half * t));
        let lo = hs.iter().fold(f32::MAX, |a, &b| a.min(b));
        let hi = hs.iter().fold(f32::MIN, |a, &b| a.max(b));
        (lo - 0.8, hi + FENCE_H)
    }

    /// Ways round for bots: along the rail yard, through the halls, over
    /// the slag behind them.
    pub fn routes(&self) -> Vec<Vec<Vec3>> {
        let s = self.size;
        let last = self.tracks[self.tracks.len() - 1] + 6.5;
        let top = self.works_top() + 10.0;
        let z = self.hold.y;
        let lines: [&[Vec2]; 3] = [
            &[
                Vec2::new(44.0, last),
                Vec2::new(s * 0.5, last),
                Vec2::new(s - 44.0, last),
            ],
            &[Vec2::new(s * 0.5 - 40.0, z), Vec2::new(s * 0.5 + 40.0, z)],
            &[
                Vec2::new(44.0, top),
                Vec2::new(s * 0.5, top),
                Vec2::new(s - 44.0, top),
            ],
        ];
        lines
            .iter()
            .map(|l| {
                l.iter()
                    .map(|&p| Vec3::new(p.x, self.ground(p), p.y))
                    .collect()
            })
            .collect()
    }

    /// The skeleton as plain boxes on `map` (its ground and spawns not laid
    /// yet): what's solid, where it's solid.
    pub fn boxes(&self, map: &mut Map) {
        let mut put = |c: Vec2, half: Vec2, y: (f32, f32), color: Vec3| {
            map.oboxes.push(OBox {
                center: c,
                half,
                y0: y.0,
                y1: y.1,
                color,
                side_mat: Material::Concrete,
                top_mat: Material::Concrete,
                ..OBox::default()
            });
        };
        for h in &self.halls {
            for w in &h.panels {
                put(w.center, w.half, (-0.3, w.top), Vec3::new(0.5, 0.5, 0.55));
            }
        }
        for f in self.furnaces.iter().chain(&self.stacks) {
            put(
                f.at,
                Vec2::splat(f.r * 0.9),
                (-0.3, f.h),
                Vec3::new(0.75, 0.4, 0.25),
            );
        }
        for g in &self.gantries {
            for l in g.legs() {
                put(l, LEG, (-0.3, g.h), Vec3::new(0.7, 0.6, 0.3));
            }
            put(
                g.at,
                Vec2::new(1.4, g.span),
                (g.h, g.h + 1.6),
                Vec3::new(0.7, 0.6, 0.3),
            );
        }
        for s in &self.stock {
            let y = self.ground(s.at);
            put(s.at, s.half, (y - 0.6, y + 2.0), Vec3::new(0.35, 0.5, 0.35));
        }
        for w in &self.wagons {
            put(w.at, WAGON, (-0.1, WAGON_H), Vec3::new(0.35, 0.4, 0.6));
        }
        for (c, half, _, _) in self.fence() {
            put(c, half, self.fence_y(c, half), Vec3::splat(0.45));
        }
    }
}
