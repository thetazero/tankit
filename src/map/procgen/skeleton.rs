//! The skeleton: a map laid out in placeholders, before anyone's
//! architecture is put on it. A biome's grammar (see `city`) makes one from
//! a seed; a style (see `imperium`) fills each placeholder in.
//!
//! Nothing here knows whose map it is. A lot is "a big building", not an
//! Imperium block; the landmark is "the big thing in the middle with doors
//! on these sides", not a rotunda. Each placeholder is also an envelope the
//! style must keep to: a lot's solid parts stay on its footprint, a
//! street's clear strip stays clear, a landmark's doors stay open. That's
//! what lets the skeleton be checked for play (see `check`) before it's
//! dressed: the greybox (`Skeleton::greybox`) is the skeleton drawn as
//! plain boxes, and what drives on the greybox drives on the dressed map.

use glam::{Vec2, Vec3};

use super::land::Land;
use crate::faction::Faction;
use crate::map::{Map, OBox, Terrain};
use crate::render::material::Material;

/// The arena, laid out.
#[derive(Clone, Debug, Default)]
pub struct Skeleton {
    /// Arena spans [0, size] on x and z.
    pub size: f32,
    /// The arena, if it isn't all of that.
    pub band: Option<std::sync::Arc<crate::map::Band>>,
    /// The ground: its landforms, and the height they make.
    pub land: Land,
    pub streets: Vec<Street>,
    /// Open ground among the buildings, with a deco in the middle.
    pub squares: Vec<Square>,
    /// The great buildings, if there are any; the first is the Hold site.
    /// Each stands on the platform of the same number.
    pub landmarks: Vec<Landmark>,
    /// What the fight is about: the Hold site.
    pub heart: Vec2,
    /// Round spawns are kept clear and level this far (a desert's).
    pub pad: f32,
    /// One per side, in side order.
    pub bases: Vec<Base>,
    pub lots: Vec<Lot>,
    /// Things standing over a street (a gate, a bridge), leaving it open.
    pub spans: Vec<Span>,
    /// What lies across the streets where they cross the arena's edge.
    pub edges: Vec<Edge>,
    /// Smaller things standing on the ground.
    pub decos: Vec<Deco>,
    pub repair_spots: Vec<Vec2>,
    /// What joins its great buildings to its river.
    pub conduits: Vec<super::conduit::Conduit>,
    /// Its river's side channels, to the houses.
    pub rills: Vec<super::rill::Rill>,
    /// Where roads cross valleys and ridges, and railroads washes.
    pub crossings: Vec<super::crossing::Crossing>,
    /// Railroads; one that starts on another's a spur off it.
    pub tracks: Vec<Vec<Vec2>>,
    /// Where each side's Raid stand is wanted.
    pub stands: Vec<Vec2>,
}

/// Raised ground: a rounded square, flat on top, sloping down over `ramp`
/// at its edge (drivable all round).
#[derive(Clone, Copy, Debug)]
pub struct Platform {
    pub center: Vec2,
    pub half: f32,
    /// Corner radius.
    pub round: f32,
    pub height: f32,
    pub ramp: f32,
}

impl Platform {
    /// Signed distance to the top's edge (negative on top).
    pub fn sdf(&self, p: Vec2) -> f32 {
        let q = (p - self.center).abs() - Vec2::splat(self.half - self.round);
        q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0) - self.round
    }

    pub fn height_at(&self, p: Vec2) -> f32 {
        self.height * (1.0 - smoothstep(0.0, self.ramp, self.sdf(p)))
    }
}

pub(crate) fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How important a street is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rank {
    /// A wide way from the middle out to the edge: the long sight lines.
    Avenue,
    /// An ordinary street between buildings.
    Lane,
    /// A railway: driven over, wagons standing on it.
    Rail,
    /// A road's run over a bridge or through a tunnel (see `crossing`).
    Crossing,
    /// A road's run beyond the arena's edge, only seen.
    Beyond,
}

/// A straight street from `from` to `to`.
#[derive(Clone, Copy, Debug)]
pub struct Street {
    pub from: Vec2,
    pub to: Vec2,
    /// Half its width, kerb to kerb: no building stands in it.
    pub half: f32,
    /// Half the strip down its middle kept clear at tank height (decos
    /// stand between it and the kerb).
    #[cfg_attr(not(test), allow(dead_code))]
    pub clear: f32,
    pub rank: Rank,
    /// It runs out through the arena's edge (else it ends inside).
    pub out: bool,
}

impl Street {
    pub fn dir(&self) -> Vec2 {
        (self.to - self.from).normalize()
    }

    pub fn len(&self) -> f32 {
        self.from.distance(self.to)
    }

    /// Distance from the center line (as a segment).
    pub fn distance(&self, p: Vec2) -> f32 {
        let d = self.to - self.from;
        let t = ((p - self.from).dot(d) / d.length_squared()).clamp(0.0, 1.0);
        p.distance(self.from + d * t)
    }
}

/// Open ground among the buildings.
#[derive(Clone, Copy, Debug)]
pub struct Square {
    pub center: Vec2,
    pub radius: f32,
}

/// What stands at a landmark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LandmarkKind {
    /// A great building (a style's own, or a shield generator).
    /// (Which of a style's great buildings, 0 to 2.)
    Great(u8),
    /// A ring of sandstone spires.
    Needles,
    /// An old impact crater: a bowl, its rim heaped with blocks.
    Crater,
    /// A great natural arch spanning the open ground.
    Arch,
    /// A shield generator out in the open, on a graded pad.
    Generator,
    /// A breach broken through a dam.
    Dam,
    /// A town's well, in its sunken court.
    Well,
    /// Open ground, nothing on it.
    Open,
}

/// The big thing in the middle: round, `radius` across, standing on the
/// ground at `center`, with a `precinct` round it (a ring the style may
/// stand things on, open where the streets come in).
#[derive(Clone, Debug)]
pub struct Landmark {
    pub center: Vec2,
    pub radius: f32,
    pub precinct: f32,
    /// Ways in, as directions out from the middle: these stay open.
    pub doors: Vec<Vec2>,
    /// The way a side of it has fallen in, if it has (war damage).
    pub fallen: Option<Vec2>,
    pub kind: LandmarkKind,
    /// Nothing solid at tank height within this (0: only its doors open).
    pub clear: f32,
    /// Its filler keeps to this.
    pub reach: f32,
    /// What it's turned across (an arch's span).
    pub facing: Vec2,
}

/// A side's base: a square court in a corner, `size` on a side, along two
/// of the map's edges. `inward` points from the corner into the map.
#[derive(Clone, Copy, Debug)]
pub struct Base {
    pub corner: Vec2,
    pub inward: Vec2,
    pub size: f32,
    pub kind: BaseKind,
}

/// What a base is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseKind {
    /// A walled court.
    Court,
    /// Open ground, the circle in the square.
    Pad,
}

impl Base {
    /// Open ground `radius` round `at`.
    pub fn pad(at: Vec2, radius: f32) -> Base {
        Base {
            corner: at - Vec2::splat(radius),
            inward: Vec2::ONE,
            size: radius * 2.0,
            kind: BaseKind::Pad,
        }
    }

    /// The point `u` m in along x from the corner and `v` along z.
    pub fn at(&self, u: f32, v: f32) -> Vec2 {
        self.corner + Vec2::new(self.inward.x * u, self.inward.y * v)
    }

    /// Where its side's tanks may start: a grid over it.
    pub fn spawn_points(&self) -> Vec<Vec2> {
        if self.kind == BaseKind::Court {
            return (0..25)
                .map(|k| self.at(9.0 + (k / 5) as f32 * 7.0, 9.0 + (k % 5) as f32 * 7.0))
                .collect();
        }
        let (c, r, step) = (self.middle(), self.size * 0.5 - 5.0, 4.5);
        let n = (r / step) as i32;
        (-n..=n)
            .flat_map(|i| (-n..=n).map(move |j| c + Vec2::new(i as f32, j as f32) * step))
            .filter(|p| p.distance(c) <= r)
            .collect()
    }

    /// The middle of it.
    pub fn middle(&self) -> Vec2 {
        self.at(self.size * 0.5, self.size * 0.5)
    }

    /// Inside the court, grown by `margin`.
    pub fn contains(&self, p: Vec2, margin: f32) -> bool {
        let q = (p - self.corner) * self.inward - self.size * 0.5;
        q.abs().max_element() < self.size * 0.5 + margin
    }
}

/// What a medium structure is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CivicKind {
    Courtyard,
    Market,
    Terrace,
}

/// What a lot holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LotKind {
    /// A big building, several storeys.
    Big,
    /// A low building.
    Small,
    /// A building lifted on pillars: tanks drive under it.
    Raised,
    /// A lot that's been flattened: heaps to drive between.
    Rubble,
    /// A medium structure (see `civic`).
    Civic(CivicKind),
    /// A building far out past the edge, only ever seen from afar.
    Distant,
    /// A cluster of hoodoos.
    Spires,
    /// Fins of rock standing out of the ground.
    Fins,
    /// A field of boulders.
    Boulders,
    /// A rock or two, cover on its own.
    Outcrop,
    /// A low face where rock's been cut back, along its footprint.
    Cut,
    /// A machine at work in the open, reaching out the way it's turned.
    Rig,
    /// A shed at a track's end, driven through: a door in each side, the
    /// rails in by its front.
    Warehouse,
    /// What loads trains, standing over a track that runs along it.
    Loader,
    /// Part of a works.
    Works(super::foundry::Part),
}

/// A building plot: its footprint (`y0`, `y1` unused), the side facing the
/// street (local axis and sign), and what stands on it.
#[derive(Clone, Copy, Debug)]
pub struct Lot {
    pub footprint: OBox,
    pub front: (usize, f32),
    pub kind: LotKind,
}

impl Lot {
    /// A round plot, `reach` about `at`, facing no way in particular.
    pub fn round(kind: LotKind, at: Vec2, reach: f32) -> Lot {
        Lot {
            footprint: OBox {
                center: at,
                half: Vec2::splat(reach),
                ..OBox::default()
            },
            front: (1, 1.0),
            kind,
        }
    }

    /// The same, turned to `yaw`.
    pub fn turned(kind: LotKind, at: Vec2, reach: f32, yaw: f32) -> Lot {
        let mut lot = Lot::round(kind, at, reach);
        lot.footprint.yaw = yaw;
        lot
    }

    pub fn at(&self) -> Vec2 {
        self.footprint.center
    }

    /// How far a round plot's filler may reach.
    pub fn reach(&self) -> f32 {
        self.footprint.half.x
    }
}

/// Something standing over a street at `at` (across `along`, the street's
/// direction): piers either side of an opening `opening` m each side of
/// the center line, which stays clear.
#[derive(Clone, Copy, Debug)]
pub struct Span {
    pub at: Vec2,
    pub along: Vec2,
    pub opening: f32,
    /// Half broken down.
    pub ruined: bool,
}

/// What a stretch of the edge is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    /// Across a street: barriers and blocks, with gaps a tank might squeeze
    /// through (past the edge it's out of the fight anyway).
    Barricade,
    /// Across an avenue's end: a heap too high to climb, low enough to see over.
    Blockade,
}

/// A stretch of the edge: from `start + along * from` to `start + along *
/// to`, the arena lying toward `inward`.
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub start: Vec2,
    pub along: Vec2,
    pub inward: Vec2,
    pub from: f32,
    pub to: f32,
    pub kind: EdgeKind,
}

/// What a deco is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecoKind {
    /// A figure on a plinth, facing `face`.
    Monument,
    /// A tall pole with something flying from it, across `face`.
    Mast,
    /// A round basin, 6 m across, with something standing in it.
    Fountain,
    /// Low cover: a broken wall, barriers, something knocked over.
    Cover,
}

#[derive(Clone, Copy, Debug)]
pub struct Deco {
    pub at: Vec2,
    pub face: Vec2,
    pub kind: DecoKind,
}

#[cfg_attr(not(test), allow(dead_code))]
impl DecoKind {
    /// How far its solid parts may reach from where it stands, m.
    pub fn reach(self) -> f32 {
        match self {
            DecoKind::Monument => 3.0,
            DecoKind::Mast => 1.6,
            DecoKind::Fountain => 7.0,
            DecoKind::Cover => 8.0,
        }
    }
}

/// How far a lot's solid parts at tank height may stand off its footprint
/// (a collapsed floor sloping to the street, a heap spilling out).
#[cfg_attr(not(test), allow(dead_code))]
pub const LOT_SPILL: f32 = 4.0;

impl Skeleton {
    pub fn center(&self) -> Vec2 {
        self.heart
    }

    /// The middle of each side's base.
    pub fn base_spots(&self) -> Vec<Vec2> {
        self.bases.iter().map(Base::middle).collect()
    }

    /// Ground height before anything's heaped on it.
    pub fn ground(&self, p: Vec2) -> f32 {
        self.land.height(p)
    }

    /// How far `p` is off every plaza's slope (negative: on one).
    pub fn off_plazas(&self, p: Vec2) -> f32 {
        self.land
            .platforms
            .iter()
            .map(|pl| pl.sdf(p) - pl.ramp)
            .fold(f32::MAX, f32::min)
    }

    /// The stretch of street `s` clear of every plaza, from and to along it.
    pub fn off_plaza_span(&self, s: &Street, margin: f32) -> (f32, f32) {
        let on = |t: f32| self.off_plazas(s.from + s.dir() * t) < margin;
        let (mut a, mut b) = (0.0, s.len());
        while a < b && on(a) {
            a += 1.0;
        }
        while b > a && on(b) {
            b -= 1.0;
        }
        (a, b)
    }

    pub fn avenues(&self) -> impl Iterator<Item = &Street> {
        self.streets.iter().filter(|s| s.rank == Rank::Avenue)
    }

    /// How far past the arena's edge `p` is, m (negative: inside).
    pub fn out(&self, p: Vec2) -> f32 {
        match &self.band {
            Some(b) => b.out(p),
            None => (-p).max(p - Vec2::splat(self.size)).max_element(),
        }
    }

    /// Ground kept open for streets and set pieces: no lot may cover it.
    /// (The courts are cleared out of the city once it's laid out.)
    pub fn reserved(&self, p: Vec2) -> bool {
        let street = self.streets.iter().any(|s| s.distance(p) < s.half + 0.4);
        let plaza = self.off_plazas(p) < 4.0;
        let square = self
            .squares
            .iter()
            .any(|s| s.center.distance(p) < s.radius + 4.0);
        let river = self.land.channel.as_ref().is_some_and(|c| c.wet(p, 0.6));
        street || plaza || square || river
    }

    /// A blank map the size of the arena, for `factions`, with the
    /// skeleton's gameplay already on it (Hold site, repair drops); spawns
    /// and open spots come once it's built (`finish`).
    pub fn blank(&self, factions: &[Faction]) -> Map {
        let mut map = Map::empty(self.size);
        map.band = self.band.clone();
        map.factions = factions.to_vec();
        map.bases = Map::no_bases(factions);
        map.open_spots = Vec::new();
        map.spawn_yaw = None;
        map.hold_site = Some(self.heart);
        map.repair_spots = self.repair_spots.clone();
        map.skeleton = Some(std::sync::Arc::new(self.clone()));
        map
    }

    /// How much of the ground at `p` is road, 0 to 1.
    pub fn road(&self, p: Vec2) -> f32 {
        self.streets
            .iter()
            .filter(|s| s.rank != Rank::Crossing)
            // (Most are nowhere near: their boxes are quick to rule out.)
            .filter(|s| {
                let reach = Vec2::splat(s.half + 1.2);
                p.cmpgt(s.from.min(s.to) - reach).all() && p.cmplt(s.from.max(s.to) + reach).all()
            })
            .map(|s| 1.0 - smoothstep(s.half - 0.8, s.half + 1.2, s.distance(p)))
            .fold(0.0, f32::max)
            .max(
                self.squares
                    .iter()
                    .map(|q| 1.0 - smoothstep(q.radius - 1.0, q.radius + 1.5, q.center.distance(p)))
                    .fold(0.0, f32::max),
            )
    }

    /// Paints `t`'s ground: `road` (multiplied into its color) down the
    /// streets.
    pub fn paint(&self, t: &mut Terrain, road: Vec3) {
        t.tint = (0..t.nz)
            .flat_map(|iz| (0..t.nx).map(move |ix| (ix, iz)))
            .map(|(ix, iz)| {
                let p = t.origin + Vec2::new(ix as f32, iz as f32) * t.cell;
                // Pale along the kerb, so the road's edge reads.
                let k = self.road(p);
                Vec3::ONE.lerp(road, k) * (1.0 + 0.7 * k * (1.0 - k))
            })
            .collect();
    }

    /// The ground under the arena: the platforms, and `heaps` on them.
    pub fn terrain(&self, heaps: impl Fn(Vec2) -> f32) -> Terrain {
        let cell = 2.0;
        let n = (self.size / cell).round() as usize + 1;
        Terrain::from_fn(Vec2::ZERO, cell, n, n, |x, z| {
            let p = Vec2::new(x, z);
            self.ground(p) + heaps(p)
        })
    }

    /// Puts down the railways' track: rails on sleepers, over the ground or
    /// a bridge's deck. Nothing that stops a tank.
    pub fn lay_rails(&self, map: &mut Map) {
        // (Over or through a crossing only along it: not across its way.)
        let y = |p: Vec2, d: Vec2| {
            let along = |c: &&super::crossing::Crossing| {
                c.dir().dot(d).abs() > 0.8 && c.distance(p) < c.half
            };
            let way = self.crossings.iter().find(along).map(|c| match c.kind {
                super::crossing::Kind::Bridge => c.top.max(self.ground(p)),
                super::crossing::Kind::Tunnel => c.floor,
            });
            way.unwrap_or_else(|| self.ground(p))
        };
        for line in &self.tracks {
            for w in line.windows(2) {
                let len = w[0].distance(w[1]);
                let d = (w[1] - w[0]) / len.max(1e-3);
                let yaw = d.x.atan2(d.y);
                let n = (len / 1.6).round().max(1.0) as i32;
                for k in 0..n {
                    let p = w[0] + d * len * (k as f32 + 0.5) / n as f32;
                    let h = y(p, d);
                    map.oboxes.push(OBox {
                        center: p,
                        yaw,
                        half: Vec2::new(1.4, 0.18),
                        y0: h - 0.3,
                        y1: h + 0.08,
                        color: Vec3::new(0.3, 0.23, 0.17),
                        side_mat: Material::Wood,
                        top_mat: Material::Wood,
                        solid: false,
                        ..OBox::default()
                    });
                }
                let side = Vec2::new(-d.y, d.x) * 0.75;
                let (a, b) = (y(w[0], d) + 0.16, y(w[1], d) + 0.16);
                for s in [side, -side] {
                    let (p, q) = (w[0] + s, w[1] + s);
                    map.segment(
                        Vec3::new(p.x, a, p.y),
                        Vec3::new(q.x, b, q.y),
                        0.08,
                        Vec3::new(0.36, 0.34, 0.33),
                        Material::Metal,
                        false,
                    );
                }
            }
        }
    }

    /// Once everything's built: each side's spawns in its court, the open
    /// spots, and the repair drops that landed clear.
    pub fn finish(&self, map: &mut Map) {
        let clear = |m: &Map, p: Vec2| {
            !m.oboxes
                .iter()
                .any(|o| o.solid && o.distance(p) < TANK_CLEAR)
        };
        for (k, b) in self.bases.iter().enumerate() {
            let spots = b
                .spawn_points()
                .into_iter()
                .filter(|&p| clear(map, p))
                .collect();
            map.bases[k] = spots;
        }
        let step = 10.0;
        let n = (self.size / step) as i32;
        let mut open = Vec::new();
        for i in 0..n {
            for j in 0..n {
                let p = Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * step;
                if clear(map, p)
                    && (self.band.is_none() || self.out(p) < -4.0)
                    && self
                        .landmarks
                        .iter()
                        .all(|l| p.distance(l.center) > l.radius + 2.0)
                {
                    open.push(p);
                }
            }
        }
        map.open_spots = open;
        let repairs = std::mem::take(&mut map.repair_spots);
        map.repair_spots = repairs
            .into_iter()
            .filter(|&p| clear(map, p) && (self.band.is_none() || self.out(p) < -4.0))
            .collect();
    }

    /// The skeleton drawn as plain boxes: what `check` drives on, and what
    /// `--greybox` shows. Each kind of placeholder has its own color.
    pub fn greybox(&self, factions: &[Faction]) -> Map {
        let mut map = self.blank(factions);
        let mut terrain = self.terrain(|_| 0.0);
        terrain.color = Vec3::splat(0.3);
        terrain.material = Material::Concrete;
        map.terrain = Some(terrain);
        let put = |map: &mut Map, center: Vec2, yaw: f32, half: Vec2, y: (f32, f32), c: Vec3| {
            map.oboxes.push(OBox {
                center,
                yaw,
                half,
                y0: y.0,
                y1: y.1,
                color: c,
                side_mat: Material::Concrete,
                top_mat: Material::Concrete,
                ..OBox::default()
            });
        };
        let yaw_of = |d: Vec2| (-d.y).atan2(d.x);
        // (What's dug is too fine for the coarse one to say if it's open.)
        let dug = !self.land.digs.is_empty() || !self.land.bores.is_empty();
        let cell = if dug { 1.0 } else { 2.0 };
        let mut ground = self.land.surface(self.size, cell, |_| 0.0);
        ground.rock = Vec3::new(0.55, 0.45, 0.35);
        map.solid = Some(ground.into());
        // The landmark: a ring wall, open at its doors and where it fell;
        // anything without doors, a low slab on its reach.
        for lm in &self.landmarks {
            let g = self.ground(lm.center);
            if !matches!(lm.kind, LandmarkKind::Great(_)) && lm.doors.is_empty() {
                put(
                    &mut map,
                    lm.center,
                    std::f32::consts::FRAC_PI_4,
                    Vec2::splat(lm.reach * 0.7),
                    (g - 0.5, g + 0.3),
                    Vec3::new(0.7, 0.3, 0.2),
                );
                map.oboxes.last_mut().unwrap().solid = false;
                continue;
            }
            let segs = 40;
            for k in 0..segs {
                let a = (k as f32 + 0.5) / segs as f32 * std::f32::consts::TAU;
                let dir = Vec2::new(a.cos(), a.sin());
                if lm.doors.iter().any(|d| d.dot(dir) > 0.985) {
                    continue;
                }
                let h = if lm.fallen.is_some_and(|f| f.dot(dir) > 0.73) {
                    3.0
                } else {
                    16.0
                };
                let half = Vec2::new(lm.radius * std::f32::consts::PI / segs as f32 * 1.02, 1.3);
                put(
                    &mut map,
                    lm.center + dir * (lm.radius - 1.3),
                    -a + std::f32::consts::FRAC_PI_2,
                    half,
                    (g, g + h),
                    Vec3::new(0.7, 0.6, 0.3),
                );
            }
            put(
                &mut map,
                lm.center,
                0.0,
                Vec2::splat(1.6),
                (g, g + 8.0),
                Vec3::new(0.7, 0.6, 0.3),
            );
        }
        let stone = super::civic::Stone {
            wall: Vec3::splat(0.6),
            mat: Material::Concrete,
            trim: Vec3::splat(0.5),
            roof: Vec3::splat(0.5),
            land: Vec3::splat(0.3),
        };
        if let Some(c) = &self.land.channel {
            c.build(&mut map, self.size);
            map.rivers = std::sync::Arc::new(vec![c.river()]);
        }
        for c in &self.crossings {
            c.build(&mut map, stone, None, |p| self.ground(p));
        }
        self.lay_rails(&mut map);
        for lot in &self.lots {
            let f = &lot.footprint;
            if let LotKind::Works(part) = lot.kind {
                super::foundry::build_part(&mut map, f, part, self.ground(f.center), None);
                continue;
            }
            let start = map.oboxes.len();
            let (h, c) = match lot.kind {
                LotKind::Works(_) => unreachable!("built above"),
                LotKind::Big => (16.0, Vec3::new(0.45, 0.45, 0.5)),
                LotKind::Small => (6.0, Vec3::new(0.55, 0.55, 0.6)),
                LotKind::Raised => (0.0, Vec3::new(0.35, 0.45, 0.6)),
                LotKind::Rubble => (1.2, Vec3::new(0.5, 0.42, 0.35)),
                LotKind::Civic(_) => (2.0, Vec3::new(0.6, 0.5, 0.5)),
                LotKind::Distant => (16.0, Vec3::new(0.38, 0.38, 0.4)),
                LotKind::Spires => (0.3, Vec3::new(0.6, 0.5, 0.2)),
                LotKind::Fins => (0.3, Vec3::new(0.3, 0.5, 0.6)),
                LotKind::Boulders | LotKind::Outcrop => (0.3, Vec3::new(0.4, 0.6, 0.3)),
                LotKind::Cut => (0.3, Vec3::new(0.3, 0.5, 0.6)),
                LotKind::Rig => (2.0, Vec3::new(0.7, 0.55, 0.2)),
                LotKind::Warehouse => (0.3, Vec3::new(0.55, 0.5, 0.45)),
                LotKind::Loader => (0.3, Vec3::new(0.7, 0.4, 0.2)),
            };
            match lot.kind {
                LotKind::Raised => {
                    // A core and pillars; the mass up over the street.
                    let (ax, az) = f.axes();
                    put(&mut map, f.center, f.yaw, f.half * 0.35, (0.0, 6.5), c);
                    for px in [-1.0f32, 1.0] {
                        for pz in [-1.0f32, 1.0] {
                            let p =
                                f.center + ax * px * (f.half.x - 1.2) + az * pz * (f.half.y - 1.2);
                            put(&mut map, p, f.yaw, Vec2::splat(0.7), (0.0, 6.5), c);
                        }
                    }
                    put(&mut map, f.center, f.yaw, f.half, (6.5, 18.0), c);
                }
                LotKind::Rubble => {
                    // Not solid: heaps a tank winds between.
                    put(&mut map, f.center, f.yaw, f.half * 0.8, (0.0, h), c);
                    map.oboxes.last_mut().unwrap().solid = false;
                }
                LotKind::Distant
                | LotKind::Civic(_)
                | LotKind::Spires
                | LotKind::Fins
                | LotKind::Boulders
                | LotKind::Outcrop
                | LotKind::Cut
                | LotKind::Rig
                | LotKind::Warehouse
                | LotKind::Loader => {
                    put(&mut map, f.center, f.yaw, f.half, (0.0, h), c);
                    map.oboxes.last_mut().unwrap().solid = false;
                }
                _ => put(&mut map, f.center, f.yaw, f.half, (0.0, h), c),
            }
            lift(&mut map.oboxes[start..], self.ground(f.center));
        }
        for s in &self.spans {
            let start = map.oboxes.len();
            let side = Vec2::new(-s.along.y, s.along.x);
            for k in [-1.0f32, 1.0] {
                let p = s.at + side * k * (s.opening + 3.0);
                let top = if s.ruined && k > 0.0 { 7.0 } else { 20.0 };
                put(
                    &mut map,
                    p,
                    yaw_of(s.along),
                    Vec2::new(2.5, 3.0),
                    (0.0, top),
                    Vec3::new(0.8, 0.5, 0.3),
                );
            }
            lift(&mut map.oboxes[start..], self.ground(s.at));
        }
        for e in &self.edges {
            let len = e.to - e.from;
            let start = map.oboxes.len();
            let c = e.start + e.along * (e.from + len * 0.5) - e.inward * 6.0;
            let half = (e.along * len * 0.5 + e.inward * 6.5).abs();
            let (h, col) = match e.kind {
                EdgeKind::Barricade => (1.2, Vec3::new(0.6, 0.55, 0.45)),
                EdgeKind::Blockade => (5.0, Vec3::new(0.5, 0.42, 0.35)),
            };
            put(&mut map, c, 0.0, half, (-0.5, h), col);
            lift(&mut map.oboxes[start..], self.ground(c));
        }
        for (k, b) in self.bases.iter().enumerate() {
            if b.kind != BaseKind::Court {
                continue;
            }
            let start = map.oboxes.len();
            let col = factions.get(k).map_or(Vec3::ONE, |f| f.paint());
            let s = b.size;
            // Walls on the two edge sides, barriers with a gap on the others.
            for (u0, v0, u1, v1, h) in [
                (0.0, 0.0, s + 1.4, 1.8, 6.5),
                (0.0, 0.0, 1.8, s + 1.4, 6.5),
                (2.0, s, s * 0.35, s + 1.4, 1.7),
                (s * 0.65, s, s + 1.4, s + 1.4, 1.7),
                (s, 2.0, s + 1.4, s * 0.35, 1.7),
                (s, s * 0.65, s + 1.4, s, 1.7),
            ] {
                let (a, z) = (b.at(u0, v0), b.at(u1, v1));
                put(
                    &mut map,
                    (a + z) * 0.5,
                    0.0,
                    (z - a).abs() * 0.5,
                    (0.0, h),
                    col,
                );
            }
            lift(&mut map.oboxes[start..], self.ground(b.middle()));
        }
        for d in &self.decos {
            let g = self.ground(d.at);
            let (half, h) = match d.kind {
                DecoKind::Monument => (Vec2::splat(1.6), 8.0),
                DecoKind::Mast => (Vec2::splat(0.2), 14.0),
                DecoKind::Fountain => (Vec2::splat(6.0), 1.2),
                DecoKind::Cover => (Vec2::new(3.0, 0.6), 1.6),
            };
            put(
                &mut map,
                d.at,
                yaw_of(d.face),
                half,
                (g - 0.1, g + h),
                Vec3::new(0.75, 0.75, 0.7),
            );
        }
        self.finish(&mut map);
        map
    }
}

/// Clearance a spawn needs from anything solid: the biggest hull's (the
/// Gravis's) corner radius, so it can face any way without touching.
pub const TANK_CLEAR: f32 = 4.6;

/// Raises `boxes` by `h`.
pub fn lift(boxes: &mut [OBox], h: f32) {
    for o in boxes {
        o.y0 += h;
        o.y1 += h;
    }
}

/// How much of a map there was, to raise what's built on it after.
pub struct Mark {
    boxes: usize,
    sculpted: usize,
    crystals: usize,
    paths: usize,
    lights: usize,
    sounds: usize,
}

impl Mark {
    pub fn of(map: &Map) -> Mark {
        Mark {
            boxes: map.oboxes.len(),
            sculpted: map.sculpted.verts.len(),
            crystals: map.crystals.len(),
            paths: map.glow_paths.len(),
            lights: map.lights.len(),
            sounds: map.sounds.len(),
        }
    }

    /// Raises everything built since by `h`: a piece made as if its ground
    /// were at 0, onto the ground it's on.
    pub fn raise(&self, map: &mut Map, h: f32) {
        if h == 0.0 {
            return;
        }
        lift(&mut map.oboxes[self.boxes..], h);
        for v in &mut map.sculpted.verts[self.sculpted..] {
            v.pos[1] += h;
        }
        for c in &mut map.crystals[self.crystals..] {
            c.base.y += h;
        }
        for g in &mut map.glow_paths[self.paths..] {
            g.points.iter_mut().for_each(|p| p.y += h);
        }
        for l in &mut map.lights[self.lights..] {
            l.pos.y += h;
        }
        for s in &mut map.sounds[self.sounds..] {
            s.at.y += h;
        }
    }

    /// Runs the foot of everything built since, on ground `h` high, down into
    /// the lowest ground under it: ground isn't always level right across
    /// what stands on it, and nothing's to hang over the low side.
    pub fn found(&self, map: &mut Map, sk: &Skeleton, h: f32) {
        for o in &mut map.oboxes[self.boxes..] {
            found(o, sk, h);
        }
        // What's sculpted goes down with it.
        let foot = map.oboxes[self.boxes..]
            .iter()
            .map(|o| o.y0)
            .fold(h - ROOT, f32::min);
        for v in &mut map.sculpted.verts[self.sculpted..] {
            if v.pos[1] < h - 0.3 {
                let g = sk.ground(Vec2::new(v.pos[0], v.pos[2]));
                v.pos[1] = v.pos[1].min(foot).min(g - ROOT);
            }
        }
        for c in &mut map.crystals[self.crystals..] {
            let up = c.axis.normalize_or(glam::Vec3::Y);
            // (Not those that stand on something, or lie.)
            if c.base.y > h + 1.0 || up.y < 0.5 {
                continue;
            }
            let at = Vec2::new(c.base.x, c.base.z);
            let drop = c.base.y - (lowest(sk, at, c.radius) - ROOT);
            if drop > 0.0 {
                let down = up * (drop / up.y);
                c.base -= down;
                c.axis += down;
            }
        }
    }

    /// Lays everything loose built since, on ground `h` high, each on the
    /// ground where it lies.
    pub fn strew(&self, map: &mut Map, sk: &Skeleton, h: f32) {
        for o in &mut map.oboxes[self.boxes..] {
            let by = sk.ground(o.center) - h;
            lift(std::slice::from_mut(o), by);
        }
        for c in &mut map.crystals[self.crystals..] {
            let ground = |p: glam::Vec3| sk.ground(Vec2::new(p.x, p.z));
            let (a, z) = (ground(c.base), ground(c.base + c.axis));
            c.base.y += a - h;
            c.axis.y += z - a;
        }
    }
}

/// How far into the ground a foot goes: past what the ground's mesh rounds off.
const ROOT: f32 = 0.4;

/// The lowest the ground is within `r` of `p`.
fn lowest(sk: &Skeleton, p: Vec2, r: f32) -> f32 {
    (0..8)
        .map(|k| p + Vec2::from_angle(k as f32 * std::f32::consts::FRAC_PI_4) * r)
        .chain([p])
        .map(|q| sk.ground(q))
        .fold(f32::MAX, f32::min)
}

/// Runs `o`, made to stand on ground `h` high, down into the lowest ground
/// under it, if it's upright and its foot's on that ground.
pub fn found(o: &mut OBox, sk: &Skeleton, h: f32) {
    if o.is_turned() || o.ramp || o.y0 > h + 0.1 {
        return;
    }
    // (Every meter: what it stands on can dip in the middle.)
    let (ax, az) = o.axes();
    let n = o.half.ceil().as_ivec2().max(glam::IVec2::ONE);
    let mut low = f32::MAX;
    for i in -n.x..=n.x {
        for j in -n.y..=n.y {
            let u = o.half.x * i as f32 / n.x as f32;
            let v = o.half.y * j as f32 / n.y as f32;
            low = low.min(sk.ground(o.center + ax * u + az * v));
        }
    }
    o.y0 = o.y0.min(low - ROOT);
}
