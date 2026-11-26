//! Generated maps, in three steps:
//!
//! 1. **Biome** (`Biome`): what kind of ground this is (a city, ...). It
//!    sets the sky and lays out the skeleton with its own grammar: a city
//!    wants a different structure from a canyon whatever goes on it.
//! 2. **Skeleton** (`skeleton`): the map in placeholders: streets, lots
//!    ("a big building", "a building on pillars", "rubble"), the landmark
//!    in the middle, bases, gates over streets, smaller decos, the edge.
//!    Nobody's architecture yet, and checked for play (`check`) on its
//!    greybox: a skeleton that fails is laid out again.
//! 3. **Style** (`Style`): whose place it is. It fills each placeholder in
//!    (the Imperium's rotunda for the landmark, their blocks for the lots,
//!    a triumphal arch for a gate) keeping to its envelope, and dresses
//!    what's beyond the edge. The same skeleton in another style is the
//!    same map, someone else's.
//!
//! A style's fillers are its library: any biome whose skeleton has a
//! monument gets the style's monument, so pieces are shared across every
//! map built in that style.
//!
//! Each biome has its own skeleton: a city's (`skeleton`) is streets and
//! lots; a crystal mine's (`mine::Layout`) is an escarpment, a mine under
//! it, a wash and a railroad, and its styles (`mine::MineStyle`) fill in
//! the lamps, machines and freight.

pub mod check;
pub mod city;
mod civic;
pub mod desert;
mod imperium;
pub mod mine;
mod resonant;
pub mod shield;
pub mod skeleton;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use glam::Vec2;

use self::skeleton::*;
use super::Map;
use super::ruins::Drift;
use crate::faction::Faction;
use crate::rng::Rng;

/// What kind of ground a map is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Biome {
    City,
    /// A crystal mine dug into a desert escarpment.
    Mine,
    /// Open desert: dunes, buttes, spires, perhaps a dry wash.
    Desert,
}

/// Whose place it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Look {
    Imperium,
    Resonant,
    Ithessar,
}

impl Look {
    /// The style `founder` builds cities in: the Imperium's, for those whose
    /// own isn't made yet.
    pub fn of(founder: Faction) -> Look {
        match founder {
            Faction::Resonant => Look::Resonant,
            Faction::Ithessar => Look::Ithessar,
            Faction::Imperium | Faction::Uthenn | Faction::Company => Look::Imperium,
        }
    }
}

/// What stands in the middle, the thing the fight's about. Either keeps to
/// the skeleton's landmark: the same map round it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Centerpiece {
    /// The style's great building (the Imperium's rotunda).
    Monument,
    /// A shield generator: the standard model of the side that holds it,
    /// whoever built the place round it (see `shield`), in the shape it's
    /// in: up, or down (wrecked, or going back up).
    ShieldGenerator(Faction, shield::Condition),
}

/// A map to generate.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Spec {
    pub seed: u64,
    pub biome: Biome,
    pub look: Look,
    pub sides: Vec<Faction>,
    pub centerpiece: Centerpiece,
}

/// A style: fills in each kind of placeholder. Each filler builds onto the
/// map and keeps its solid parts to the placeholder's envelope (see
/// `skeleton`); heaps it banks up come back as drifts for the terrain.
pub(in crate::map) trait Style {
    /// The ground between its buildings.
    fn ground(&self) -> glam::Vec3;
    /// Its roads' color, multiplied into the ground's.
    fn road(&self) -> glam::Vec3;
    /// What it builds medium structures of.
    fn stone(&self) -> civic::Stone;
    /// The Hold site's name on the HUD, with `piece` in the middle.
    fn hold_name(&self, piece: Centerpiece) -> &'static str;
    fn landmark(&self, map: &mut Map, sk: &Skeleton, lm: &Landmark, rng: &mut Rng) -> Vec<Drift>;
    /// Paving and the like on a platform's top.
    fn platform(&self, map: &mut Map, sk: &Skeleton, p: &Platform, rng: &mut Rng);
    fn street(&self, map: &mut Map, sk: &Skeleton, s: &Street, rng: &mut Rng);
    fn span(&self, map: &mut Map, sk: &Skeleton, s: &Span, rng: &mut Rng);
    fn lot(&self, map: &mut Map, sk: &Skeleton, lot: &Lot, rng: &mut Rng) -> Vec<Drift>;
    fn base(&self, map: &mut Map, sk: &Skeleton, side: usize, b: &Base, rng: &mut Rng);
    fn edge(&self, map: &mut Map, sk: &Skeleton, e: &Edge, rng: &mut Rng);
    fn deco(&self, map: &mut Map, sk: &Skeleton, d: &Deco, rng: &mut Rng);
    /// What's seen beyond the edge.
    fn beyond(&self, map: &mut Map, sk: &Skeleton, rng: &mut Rng);
    /// Last touches over the whole map (wear on the masonry).
    fn finish(&self, map: &mut Map);
}

fn style(look: Look) -> &'static dyn Style {
    match look {
        Look::Imperium => &imperium::Imperium,
        Look::Resonant => &resonant::Resonant,
        // Nobody else's city yet: any other look is dressed as theirs.
        Look::Ithessar => &imperium::Imperium,
    }
}

/// What a filler built: the placeholder, and the range of `oboxes` it added.
#[derive(Clone, Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum Claim {
    Lot(usize),
    Deco(usize),
    Span(usize),
}

/// The skeleton `spec` lays out: the first, from its seed on, whose
/// greybox passes `check`.
pub fn skeleton(spec: &Spec) -> Skeleton {
    let mut last = None;
    for attempt in 0..16u64 {
        let mut rng = Rng::new(spec.seed.wrapping_mul(0x9E37_79B9).wrapping_add(attempt));
        let generator = matches!(spec.centerpiece, Centerpiece::ShieldGenerator(..));
        let plan = city::Plan::of(spec.seed, spec.sides.len(), generator);
        let sk = city::skeleton(&mut rng, spec.sides.len(), &plan);
        match check::playable(&sk.greybox(&spec.sides)) {
            Ok(()) => return sk,
            Err(why) => log::debug!("map seed {} attempt {attempt}: {why}", spec.seed),
        }
        last = Some(sk);
    }
    last.unwrap()
}

/// Builds `spec`'s map.
fn build_map(spec: &Spec) -> Map {
    match spec.biome {
        Biome::City => build(spec).0,
        Biome::Mine => mine::build(spec),
        Biome::Desert => desert::build(spec),
    }
}

/// Builds `spec`'s city, and what each filler claimed.
pub fn build(spec: &Spec) -> (Map, Vec<(Claim, std::ops::Range<usize>)>) {
    let sk = skeleton(spec);
    let st = style(spec.look);
    let mut rng = Rng::new(spec.seed ^ 0x5EED_D2E5);
    let mut map = sk.blank(&spec.sides);
    map.atmosphere = city::atmosphere(&mut rng);
    map.hold_name = if sk.landmarks.is_empty() {
        "SQUARE"
    } else {
        st.hold_name(spec.centerpiece)
    };
    map.planet = Some((spec.seed, planet(spec.biome)));
    let mut claims = Vec::new();
    let mut drifts = Vec::new();
    for (k, lm) in sk.landmarks.iter().enumerate() {
        match spec.centerpiece {
            Centerpiece::ShieldGenerator(owner, condition) if k == 0 => {
                // The city round it is the same as round the monument, block
                // for block: the generator draws on its own numbers, and the
                // city's are drawn on past the monument as if it stood here.
                st.landmark(&mut Map::empty(sk.size), &sk, lm, &mut rng);
                let mut own = Rng::new(spec.seed ^ 0x5E1D_0000);
                let mark = shield::Mark::of(&map);
                drifts.extend(shield::build(&mut map, &sk, &mut own, owner));
                let base = sk.ground(lm.center);
                shield::ruin(&mut map, &mut own, lm, base, &mark, owner, condition);
            }
            _ => drifts.extend(st.landmark(&mut map, &sk, lm, &mut rng)),
        }
    }
    for (k, lot) in sk.lots.iter().enumerate() {
        let start = map.oboxes.len();
        match lot.kind {
            LotKind::Civic(kind) => civic::build(&mut map, &mut rng, lot, kind, st.stone()),
            _ => drifts.extend(st.lot(&mut map, &sk, lot, &mut rng)),
        }
        claims.push((Claim::Lot(k), start..map.oboxes.len()));
    }
    let mut terrain = sk.terrain(|p| drifts.iter().map(|d| d.height_at(p)).fold(0.0, f32::max));
    terrain.color = st.ground();
    terrain.material = crate::render::material::Material::Concrete;
    sk.paint(&mut terrain, st.road());
    map.terrain = Some(terrain);
    for p in &sk.platforms {
        st.platform(&mut map, &sk, p, &mut rng);
    }
    for s in &sk.streets {
        st.street(&mut map, &sk, s, &mut rng);
    }
    for (k, s) in sk.spans.iter().enumerate() {
        let start = map.oboxes.len();
        st.span(&mut map, &sk, s, &mut rng);
        claims.push((Claim::Span(k), start..map.oboxes.len()));
    }
    for (k, d) in sk.decos.iter().enumerate() {
        let start = map.oboxes.len();
        st.deco(&mut map, &sk, d, &mut rng);
        claims.push((Claim::Deco(k), start..map.oboxes.len()));
    }
    for (k, b) in sk.bases.iter().enumerate() {
        st.base(&mut map, &sk, k, b, &mut rng);
    }
    for e in &sk.edges {
        st.edge(&mut map, &sk, e, &mut rng);
    }
    st.beyond(&mut map, &sk, &mut rng);
    if let Some(t) = &mut map.scenery_terrain {
        sk.paint(t, st.road());
    }
    st.finish(&mut map);
    map.drop_tiny_hitboxes();
    map.separate_coplanar_faces();
    sk.finish(&mut map);
    map.vistas = vistas(&map, &sk);
    (map, claims)
}

/// The map `spec` makes, built only the first time it's asked for (the
/// last few are kept: the tests and the menus ask for the same ones over
/// and over).
pub fn map(spec: &Spec) -> Map {
    static CACHE: LazyLock<Mutex<HashMap<Spec, Map>>> = LazyLock::new(Default::default);
    if let Some(m) = CACHE.lock().unwrap().get(spec) {
        return m.clone();
    }
    let m = build_map(spec);
    let mut cache = CACHE.lock().unwrap();
    if cache.len() >= 6 {
        cache.clear();
    }
    cache.insert(spec.clone(), m.clone());
    m
}

/// How a world of `biome` looks from orbit.
fn planet(biome: Biome) -> crate::planet::Look {
    match biome {
        Biome::City => city::PLANET,
        Biome::Mine => mine::PLANET,
        Biome::Desert => desert::PALETTES[0].planet,
    }
}

/// The skeleton `spec` lays out, as its greybox.
pub fn greybox(spec: &Spec) -> Map {
    match spec.biome {
        Biome::Mine => return mine::greybox(spec),
        Biome::Desert => return desert::greybox(spec),
        Biome::City => {}
    }
    let sk = skeleton(spec);
    let mut map = sk.greybox(&spec.sides);
    let mut rng = Rng::new(spec.seed ^ 0x5EED_D2E5);
    map.atmosphere = city::atmosphere(&mut rng);
    map.vistas = vistas(&map, &sk);
    map
}

/// Where the menus show each side's tank: in or near its court, in the
/// sun, the camera looking past it into the city.
fn vistas(map: &Map, sk: &Skeleton) -> Vec<(Vec2, f32)> {
    let sun = map.atmosphere.sun_dir.normalize();
    let view = |p: Vec2| {
        // The camera behind the tank, looking past it to the middle.
        let d = sk.center() - p;
        (-d.x).atan2(-d.y)
    };
    sk.bases
        .iter()
        .map(|b| {
            // In the court if the sun's on it, else as near it as it's lit
            // (with the sun low behind the edge, the court's in shadow):
            // nearest the court's middle first.
            let mid = b.at(b.size * 0.55, b.size * 0.55);
            let reach = b.size + 60.0;
            let mut spots = Vec::new();
            let mut u = 12.0;
            while u < reach {
                let mut v = 12.0;
                while v < reach {
                    spots.push(b.at(u, v));
                    v += 3.0;
                }
                u += 3.0;
            }
            spots.sort_by(|a, c| a.distance(mid).total_cmp(&c.distance(mid)));
            let good = |p: Vec2| {
                let g = map.ground_height(p.x, p.y);
                let at = glam::Vec3::new(p.x, g + 1.9, p.y);
                let blocked =
                    |dir: glam::Vec3, len: f32| map.oboxes.iter().any(|o| o.ray_hit(at, dir, len));
                // Nothing under the tank or round it but the ground, and
                // level ground out to the camera.
                let under = map
                    .oboxes
                    .iter()
                    .any(|o| o.distance(p) < 3.4 && o.y1 > g + 0.05 && o.y0 < g + 3.0);
                let v = view(p);
                let back = Vec2::new(v.sin(), v.cos());
                let level = (0..=14).all(|k| {
                    let q = p + back * k as f32;
                    (map.ground_height(q.x, q.y) - g).abs() < 0.3
                });
                level && !under && !blocked(sun, 300.0) && !blocked(glam::Vec3::Y, 60.0)
            };
            let p = spots.into_iter().find(|&p| good(p)).unwrap_or(b.middle());
            (p, view(p))
        })
        .collect()
}

impl Map {
    /// The map turned over east to west: every piece of it reflected across
    /// the arena's middle (x to `size - x`), the sun and wind with it. What
    /// stood on the west side stands on the east, the same way round. A
    /// generated map that's laid out with a lie to it (the crystal mine's
    /// mesa slopes down to the west, its scree is in the east) can come
    /// either way round.
    pub(in crate::map) fn mirror_x(&mut self) {
        let s = self.size;
        let p2 = |p: &mut Vec2| p.x = s - p.x;
        let p3 = |p: &mut glam::Vec3| p.x = s - p.x;
        let d3 = |d: &mut glam::Vec3| d.x = -d.x;
        for o in &mut self.oboxes {
            p2(&mut o.center);
            o.yaw = -o.yaw;
            o.roll = -o.roll;
        }
        for b in &mut self.solids {
            (b.min.x, b.max.x) = (s - b.max.x, s - b.min.x);
        }
        for b in &mut self.boxes {
            (b.min.x, b.max.x) = (s - b.max.x, s - b.min.x);
        }
        for w in &mut self.wedges {
            (w.min.x, w.max.x) = (s - w.max.x, s - w.min.x);
            w.rise = match w.rise {
                super::Rise::PosX => super::Rise::NegX,
                super::Rise::NegX => super::Rise::PosX,
                r => r,
            };
        }
        for c in &mut self.crystals {
            p3(&mut c.base);
            d3(&mut c.axis);
        }
        for c in &mut self.cores {
            p3(&mut c.at);
            c.prongs.iter_mut().for_each(p3);
        }
        for b in &mut self.beams {
            p3(&mut b.from);
            p3(&mut b.to);
        }
        for g in &mut self.glow_paths {
            g.points.iter_mut().for_each(p3);
        }
        self.lights.iter_mut().for_each(|l| p3(&mut l.pos));
        self.sounds.iter_mut().for_each(|e| p3(&mut e.at));
        self.routes.iter_mut().flatten().for_each(p3);
        for v in &mut self.sculpted.verts {
            v.pos[0] = s - v.pos[0];
            v.normal[0] = -v.normal[0];
        }
        // Turned over, every triangle winds the other way: wind it back.
        for t in self.sculpted.indices.chunks_mut(3) {
            t.swap(1, 2);
        }
        for t in [&mut self.terrain, &mut self.scenery_terrain]
            .into_iter()
            .flatten()
        {
            debug_assert!(
                ((t.nx - 1) as f32 * t.cell - (s - 2.0 * t.origin.x)).abs() < 1e-3,
                "a terrain not centered on the arena"
            );
            fn flip<T>(v: &mut [T], nx: usize, nz: usize) {
                if v.len() == nx * nz {
                    for row in v.chunks_mut(nx) {
                        row.reverse();
                    }
                }
            }
            let (nx, nz) = (t.nx, t.nz);
            flip(&mut t.heights, nx, nz);
            flip(&mut t.tint, nx, nz);
            flip(&mut t.normals, nx, nz);
            t.normals.iter_mut().for_each(d3);
        }
        self.open_spots.iter_mut().for_each(p2);
        self.bases.iter_mut().flatten().for_each(p2);
        self.repair_spots.iter_mut().for_each(p2);
        self.raid_stands.iter_mut().for_each(p2);
        if let Some(h) = &mut self.hold_site {
            p2(h);
        }
        for (p, view) in &mut self.vistas {
            p2(p);
            *view = -*view;
        }
        if let Some(y) = &mut self.spawn_yaw {
            *y = -*y;
        }
        d3(&mut self.atmosphere.sun_dir);
        self.atmosphere.wind.x = -self.atmosphere.wind.x;
    }

    /// The map turned a quarter about its middle, counterclockwise seen
    /// from above (x to z, z to `size - x`): everything on it, the sun and
    /// wind with it. With `mirror_x`, a generated map laid out with its big
    /// shapes along one edge can have them along any.
    pub(in crate::map) fn turn(&mut self) {
        let s = self.size;
        let p2 = |p: &mut Vec2| *p = Vec2::new(p.y, s - p.x);
        let p3 = |p: &mut glam::Vec3| *p = glam::Vec3::new(p.z, p.y, s - p.x);
        let d3 = |d: &mut glam::Vec3| *d = glam::Vec3::new(d.z, d.y, -d.x);
        let quarter = std::f32::consts::FRAC_PI_2;
        for o in &mut self.oboxes {
            p2(&mut o.center);
            o.yaw += quarter;
        }
        for b in self.solids.iter_mut() {
            let (lo, hi) = (b.min, b.max);
            b.min = glam::Vec3::new(lo.z, lo.y, s - hi.x);
            b.max = glam::Vec3::new(hi.z, hi.y, s - lo.x);
        }
        for b in self.boxes.iter_mut() {
            let (lo, hi) = (b.min, b.max);
            b.min = glam::Vec3::new(lo.z, lo.y, s - hi.x);
            b.max = glam::Vec3::new(hi.z, hi.y, s - lo.x);
        }
        for w in &mut self.wedges {
            let (lo, hi) = (w.min, w.max);
            w.min = Vec2::new(lo.y, s - hi.x);
            w.max = Vec2::new(hi.y, s - lo.x);
            w.rise = match w.rise {
                super::Rise::PosX => super::Rise::NegZ,
                super::Rise::NegX => super::Rise::PosZ,
                super::Rise::PosZ => super::Rise::PosX,
                super::Rise::NegZ => super::Rise::NegX,
            };
        }
        for c in &mut self.crystals {
            p3(&mut c.base);
            d3(&mut c.axis);
        }
        for c in &mut self.cores {
            p3(&mut c.at);
            c.prongs.iter_mut().for_each(p3);
        }
        for b in &mut self.beams {
            p3(&mut b.from);
            p3(&mut b.to);
        }
        for g in &mut self.glow_paths {
            g.points.iter_mut().for_each(p3);
        }
        self.lights.iter_mut().for_each(|l| p3(&mut l.pos));
        self.sounds.iter_mut().for_each(|e| p3(&mut e.at));
        self.routes.iter_mut().flatten().for_each(p3);
        for v in &mut self.sculpted.verts {
            let (p, n) = (v.pos, v.normal);
            v.pos = [p[2], p[1], s - p[0]];
            v.normal = [n[2], n[1], -n[0]];
        }
        for t in [&mut self.terrain, &mut self.scenery_terrain]
            .into_iter()
            .flatten()
        {
            debug_assert!(t.nx == t.nz, "a terrain not square");
            fn turn<T: Copy>(v: &mut [T], n: usize) {
                if v.len() != n * n {
                    return;
                }
                let old = v.to_vec();
                for iz in 0..n {
                    for ix in 0..n {
                        v[iz * n + ix] = old[ix * n + (n - 1 - iz)];
                    }
                }
            }
            let n = t.nx;
            turn(&mut t.heights, n);
            turn(&mut t.tint, n);
            turn(&mut t.normals, n);
            t.normals.iter_mut().for_each(d3);
        }
        self.open_spots.iter_mut().for_each(p2);
        self.bases.iter_mut().flatten().for_each(p2);
        self.repair_spots.iter_mut().for_each(p2);
        self.raid_stands.iter_mut().for_each(p2);
        if let Some(h) = &mut self.hold_site {
            p2(h);
        }
        for (p, view) in &mut self.vistas {
            p2(p);
            *view += quarter;
        }
        if let Some(y) = &mut self.spawn_yaw {
            *y += quarter;
        }
        d3(&mut self.atmosphere.sun_dir);
        let w = self.atmosphere.wind;
        self.atmosphere.wind = Vec2::new(w.y, -w.x);
    }

    /// The imperial city `seed` names (see `crate::city_spec`).
    pub fn city(seed: u64) -> Map {
        map(&crate::city_spec(seed, Look::Imperium))
    }

    /// The same with a corner each for all four sides: the engine still
    /// takes a free-for-all, though no mission's one.
    #[cfg(test)]
    pub fn four_way_city(seed: u64) -> Map {
        map(&Spec {
            sides: Faction::ALL.to_vec(),
            ..crate::city_spec(seed, Look::Imperium)
        })
    }

    /// The Ithessar crystal mine `seed` names (see `crate::mine_spec`).
    pub fn mine(seed: u64) -> Map {
        // Tests build the same few mines over and over: each once (the
        // first few seeds only, so a sweep over many doesn't keep them all).
        #[cfg(test)]
        if seed < 12 {
            use std::collections::HashMap;
            use std::sync::{Arc, Mutex, OnceLock};
            type Built = Arc<OnceLock<Map>>;
            static MINES: OnceLock<Mutex<HashMap<u64, Built>>> = OnceLock::new();
            let slot = MINES
                .get_or_init(Default::default)
                .lock()
                .unwrap()
                .entry(seed)
                .or_default()
                .clone();
            return slot.get_or_init(|| map(&crate::mine_spec(seed))).clone();
        }
        map(&crate::mine_spec(seed))
    }

    /// The open desert `seed` names (see `crate::desert_spec`).
    pub fn desert(seed: u64) -> Map {
        map(&crate::desert_spec(seed))
    }

    /// The imperial city `seed` names, with `owner`'s shield generator in
    /// the middle in place of the rotunda (see `crate::shield_city_spec`).
    pub fn shield_city(seed: u64, owner: Faction) -> Map {
        map(&crate::shield_city_spec(seed, owner))
    }

    /// The same, the generator in the shape `condition` says: up, wrecked,
    /// or going back up.
    pub fn shield_city_in(seed: u64, owner: Faction, condition: shield::Condition) -> Map {
        map(&crate::shield_spec(
            seed,
            Look::Imperium,
            crate::Generator { owner, condition },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::OBox;

    /// The seeds the tests build: enough to catch a grammar that only
    /// sometimes goes wrong, few enough to build in a debug test run.
    const SEEDS: [u64; 4] = [0, 1, 2, 3];

    /// Every seed in every style of city there is.
    fn cities() -> impl Iterator<Item = (u64, Look)> {
        [Look::Imperium, Look::Resonant]
            .into_iter()
            .flat_map(|look| SEEDS.map(|s| (s, look)))
    }

    fn spec(seed: u64) -> Spec {
        crate::city_spec(seed, Look::Imperium)
    }

    #[test]
    fn every_seed_is_playable_dressed() {
        for (seed, look) in cities() {
            let m = map(&crate::city_spec(seed, look));
            assert_eq!(m.factions, crate::map::DEFAULT_SIDES.to_vec());
            assert_eq!(m.bases.len(), 2);
            if let Err(why) = check::playable(&m) {
                panic!("seed {seed} {look:?}: {why}");
            }
            assert!(
                m.open_spots.len() > 100,
                "seed {seed}: {} open",
                m.open_spots.len()
            );
            assert!(
                m.repair_spots.len() >= 4,
                "seed {seed}: {:?}",
                m.repair_spots
            );
            assert_eq!(m.vistas.len(), 2, "seed {seed}");
        }
    }

    #[test]
    fn different_seeds_make_different_cities() {
        let sk: Vec<Skeleton> = SEEDS.iter().map(|&s| skeleton(&spec(s))).collect();
        for (i, a) in sk.iter().enumerate() {
            for b in &sk[i + 1..] {
                let same = a.size == b.size && a.heart == b.heart && a.lots.len() == b.lots.len();
                assert!(!same, "two seeds laid out the same city");
            }
        }
        // And the same seed, the same city.
        let again = skeleton(&spec(SEEDS[1]));
        assert_eq!(again.lots.len(), sk[1].lots.len());
        assert_eq!(again.heart, sk[1].heart);
    }

    /// Along the edge, never a long way without a building or something
    /// put across it; and each avenue's end is heaped with rubble.
    #[test]
    fn the_edge_is_cordoned() {
        for seed in SEEDS {
            let sk = skeleton(&spec(seed));
            for (start, along, _) in city::sides(sk.size) {
                let mut open = 0.0;
                let mut t = 0.0;
                while t <= sk.size {
                    let p = start + along * t;
                    let in_building = sk.lots.iter().any(|l| {
                        matches!(l.kind, LotKind::Big | LotKind::Small) && {
                            let q = l.footprint.local_point(p).abs() - l.footprint.half;
                            q.max_element() < 0.0
                        }
                    });
                    let across = sk
                        .edges
                        .iter()
                        .any(|e| e.along == along && e.from <= t && t <= e.to);
                    open = if in_building || across {
                        0.0
                    } else {
                        open + 0.5
                    };
                    assert!(open < 40.0, "seed {seed}: the edge is open for 40 m to {p}");
                    t += 0.5;
                }
            }
            for a in sk.avenues().filter(|a| a.out) {
                assert!(
                    sk.edges.iter().any(|e| e.kind == EdgeKind::Blockade
                        && (a.to - e.start).perp_dot(e.along).abs() < 0.01
                        && (e.from..=e.to).contains(&(a.to - e.start).dot(e.along))),
                    "seed {seed}: nothing across the avenue to {}",
                    a.to
                );
            }
        }
    }

    /// The city doesn't stop at the edge: its blocks run on out past it on
    /// every side.
    #[test]
    fn the_city_runs_on_past_the_edge() {
        for seed in SEEDS {
            let sk = skeleton(&spec(seed));
            for (start, _, inward) in city::sides(sk.size) {
                let out = |p: Vec2| -(p - start).dot(inward);
                let near = sk
                    .lots
                    .iter()
                    .filter(|l| (4.0..city::NEAR).contains(&out(l.footprint.center)))
                    .count();
                let far = sk
                    .lots
                    .iter()
                    .filter(|l| out(l.footprint.center) > city::NEAR + 20.0)
                    .count();
                assert!(
                    near >= 4 && far >= 8,
                    "seed {seed}: {near} near, {far} far past {start}"
                );
            }
        }
    }

    /// The courts are cleared out of the city, not put down in the open:
    /// buildings stand right round each one.
    #[test]
    fn buildings_stand_round_every_court() {
        for seed in SEEDS {
            let sk = skeleton(&spec(seed));
            for (k, b) in sk.bases.iter().enumerate() {
                let (a, z) = (b.at(0.0, 0.0), b.at(b.size, b.size));
                let court = OBox {
                    center: (a + z) * 0.5,
                    half: (z - a).abs() * 0.5,
                    ..OBox::default()
                };
                let round = sk
                    .lots
                    .iter()
                    .filter(|l| l.kind != LotKind::Distant && l.footprint.overlaps(&court, 20.0))
                    .count();
                assert!(round >= 4, "seed {seed}: {round} lots round court {k}");
            }
        }
    }

    /// A placeholder's filler keeps its solid parts at tank height to the
    /// placeholder's ground: a lot's to its footprint (and a little
    /// spill), a deco's to its reach, a gate's off the middle of the
    /// street. What drove on the greybox drives on the dressed map.
    #[test]
    fn fillers_keep_to_their_envelopes() {
        for (seed, look) in cities() {
            let sk = skeleton(&spec(seed));
            let (m, claims) = build(&crate::city_spec(seed, look));
            for (claim, range) in claims {
                for o in &m.oboxes[range] {
                    let (y0, _) = o.y_range();
                    let g = sk.ground(o.center);
                    // A ramp (a floor fallen to the street) is there to drive up.
                    if !o.solid || o.ramp || y0 > g + 3.0 {
                        continue;
                    }
                    let shadow = o.base_shadow(3.0);
                    let worst = |inside: &dyn Fn(Vec2) -> f32| {
                        shadow.iter().map(|&p| inside(p)).fold(0.0, f32::max)
                    };
                    match claim {
                        Claim::Lot(k) => {
                            let lot = &sk.lots[k].footprint;
                            let out = worst(&|p| {
                                let q = lot.local_point(p).abs() - lot.half;
                                q.max_element()
                            });
                            assert!(
                                out < LOT_SPILL,
                                "seed {seed} lot {k} {:?}: {out} m out: {o:?}",
                                sk.lots[k]
                            );
                        }
                        Claim::Deco(k) => {
                            let d = &sk.decos[k];
                            let out = worst(&|p| p.distance(d.at) - d.kind.reach());
                            assert!(out < 0.01, "seed {seed} {:?} {k}: {out} m out", d.kind);
                        }
                        Claim::Span(k) => {
                            let s = &sk.spans[k];
                            let side = Vec2::new(-s.along.y, s.along.x);
                            let near = shadow
                                .iter()
                                .map(|&p| (p - s.at).dot(side).abs())
                                .fold(f32::MAX, f32::min);
                            // Its opening stays clear (the ruined one's
                            // attic fell beside the piers).
                            assert!(
                                near > s.opening - 0.6,
                                "seed {seed} gate {k} blocks the street: {near} m from the middle"
                            );
                        }
                    }
                }
            }
        }
    }

    /// Down each avenue's clear strip, from the landmark's precinct to the
    /// edge, nothing solid stands at tank height.
    #[test]
    fn the_avenues_are_open_to_the_middle() {
        for (seed, look) in cities() {
            let sk = skeleton(&spec(seed));
            let m = map(&crate::city_spec(seed, look));
            for a in sk.avenues() {
                let (dir, side) = (a.dir(), Vec2::new(-a.dir().y, a.dir().x));
                // From plaza to plaza, or to the heap at the edge.
                let (t0, t1) = sk.off_plaza_span(a, -10.0);
                let t1 = if a.out { t1 - 8.0 } else { t1 };
                let mut r = t0;
                while r < t1 {
                    for across in [-1.0, 0.0, 1.0] {
                        let p = a.from + dir * r + side * across * (a.clear - 1.0);
                        // (The heap across the edge is the edge's.)
                        let edge = p.min(Vec2::splat(sk.size) - p).min_element();
                        if edge < 11.0 {
                            continue;
                        }
                        assert!(
                            !m.oboxes.iter().any(|o| {
                                let (y0, _) = o.y_range();
                                o.solid && !o.ramp && y0 < sk.ground(p) + 3.0 && o.distance(p) < 1.0
                            }),
                            "seed {seed}: avenue {dir:?} blocked at {r}, {across}: {:?}",
                            m.oboxes.iter().find(|o| {
                                let (y0, _) = o.y_range();
                                o.solid && !o.ramp && y0 < sk.ground(p) + 3.0 && o.distance(p) < 1.0
                            })
                        );
                    }
                    r += 2.0;
                }
            }
        }
    }

    /// The shield generator variants, one per side's model: the same city
    /// for the same seed, the generator in the rotunda's place, as
    /// playable, its doors open, and its light high over the middle.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn the_shield_city_comes_with_any_sides_generator() {
        for (k, owner) in Faction::ALL.into_iter().enumerate() {
            // One seed each (a different one for each side): building a
            // city takes seconds in a debug build.
            // (Seeds whose plain city has a great building: an old town's has
            // none, so the generator's city is another.)
            let seed = [0, 1, 3, 4][k];
            let plain = skeleton(&spec(seed));
            let sk = skeleton(&crate::shield_city_spec(seed, owner));
            assert_eq!(sk.heart, plain.heart);
            assert_eq!(sk.lots.len(), plain.lots.len());
            let (m, our_claims) = build(&crate::shield_city_spec(seed, owner));
            let what = format!("{owner:?} seed {seed}");
            // Every block round it as in the plain city (to the
            // millimetres `separate_coplanar_faces` moves faces by).
            let (city, claims) = build(&spec(seed));
            assert_eq!(claims.len(), our_claims.len(), "{what}");
            for ((_, a), (_, b)) in claims.iter().zip(&our_claims) {
                let (a, b) = (&city.oboxes[a.clone()], &m.oboxes[b.clone()]);
                assert_eq!(a.len(), b.len(), "{what}: the city round it differs");
                for (x, y) in a.iter().zip(b) {
                    assert!(
                        x.center.distance(y.center) < 0.02
                            && (x.half - y.half).abs().max_element() < 0.02
                            && (x.y1 - y.y1).abs() < 0.02,
                        "{what}: the city round it differs: {x:?} {y:?}"
                    );
                }
            }
            assert_eq!(m.hold_name, "GENERATOR");
            assert_eq!(m.hold_site, Some(sk.heart));
            if let Err(why) = check::playable(&m) {
                panic!("{what}: {why}");
            }
            assert!(m.open_spots.len() > 100, "{what}");
            assert_eq!(m.vistas.len(), 2, "{what}");
            let lm = &sk.landmarks[0];
            let g = sk.ground(lm.center);
            assert!(
                m.lights.iter().any(|l| {
                    Vec2::new(l.pos.x, l.pos.z).distance(lm.center) < 1.0 && l.pos.y > g + 8.0
                }),
                "{what}: nothing lit high over the middle"
            );
            // Each door open (as wide as the skeleton keeps it) from the
            // plinth out past the precinct.
            for &d in &lm.doors {
                let side = Vec2::new(-d.y, d.x);
                let mut r = shield::PLINTH_R + 1.5;
                while r < lm.precinct + 4.0 {
                    for across in [-1.0f32, 0.0, 1.0] {
                        let p = lm.center + d * r + side * across;
                        let ground = sk.ground(p);
                        let hit = m.oboxes.iter().find(|o| {
                            let (y0, _) = o.y_range();
                            o.solid && y0 < ground + 3.0 && o.distance(p) < 0.5
                        });
                        assert!(hit.is_none(), "{what}: door {d} blocked at {r}: {hit:?}");
                        assert!(
                            !m.crystals.iter().any(|c| c.solid
                                && Vec2::new(c.base.x, c.base.z).distance(p) < c.radius + 1.0),
                            "{what}: a crystal in door {d} at {r}"
                        );
                    }
                    r += 1.5;
                }
            }
        }
    }

    /// Four seeds in a row are four forms, and there are cities of every
    /// size and density.
    #[test]
    fn cities_vary() {
        use super::city::Plan;
        let plans: Vec<Plan> = (0..64).map(|s| Plan::of(s, 2, false)).collect();
        let forms: std::collections::HashSet<_> = plans[..4].iter().map(|p| p.form).collect();
        assert_eq!(forms.len(), 4);
        let extents: std::collections::HashSet<_> = plans.iter().map(|p| p.extent).collect();
        let densities: std::collections::HashSet<_> = plans.iter().map(|p| p.density).collect();
        assert_eq!((extents.len(), densities.len()), (3, 3));
        // A generator always has a great building's place to stand in.
        for seed in 0..8 {
            let spec = crate::shield_city_spec(seed, Faction::Uthenn);
            assert!(!skeleton(&spec).landmarks.is_empty(), "seed {seed}");
        }
    }

    #[test]
    #[ignore = "a probe"]
    fn sweep_cities() {
        for seed in 0..24 {
            for look in [Look::Imperium, Look::Resonant] {
                let spec = crate::city_spec(seed, look);
                let sk = skeleton(&spec);
                let m = build(&spec).0;
                let r = check::playable(&m);
                let lm = sk.landmarks.first();
                let mut reach = String::new();
                if let Some(lm) = lm {
                    let nav = crate::ai::nav::NavGrid::build(&m);
                    for d in [0.0, 6.0, 10.0, 16.0, 20.0, 24.0, 28.0, 36.0, 44.0, 60.0] {
                        let ok: Vec<u8> = lm
                            .doors
                            .iter()
                            .map(|q| {
                                let p = lm.center + *q * d;
                                let b = m.bases[0][0];
                                !nav.path(b, p).is_empty() as u8
                            })
                            .collect();
                        reach += &format!(" {d}:{ok:?}");
                    }
                }
                println!(
                    "{seed} {look:?} {:?} spots {} repairs {} {r:?}{}",
                    super::city::Plan::of(seed, 2, false),
                    m.open_spots.len(),
                    m.repair_spots.len(),
                    if r.is_err() { reach } else { String::new() }
                );
            }
        }
    }

    /// The plaza's slope is gentle enough to drive up anywhere.
    #[test]
    fn the_plaza_slope_is_drivable() {
        for seed in SEEDS {
            let sk = skeleton(&spec(seed));
            for pl in &sk.platforms {
                for k in 0..16 {
                    let a = k as f32 / 16.0 * std::f32::consts::TAU;
                    let dir = Vec2::new(a.cos(), a.sin());
                    let mut prev = sk.ground(pl.center);
                    for r in 1..(pl.half as i32 * 2) {
                        let h = sk.ground(pl.center + dir * r as f32);
                        assert!((prev - h).abs() < 0.25, "seed {seed}: plaza edge too steep");
                        prev = h;
                    }
                }
            }
        }
    }
}
