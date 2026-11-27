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
//! Every biome's grammar lays out the one `Skeleton`: the ground in
//! `land`'s landforms, the rest in shared placeholders.

mod banks;
pub mod channel;
pub mod check;
pub mod city;
mod civic;
mod conduit;
pub mod crossing;
pub mod depot;
pub mod desert;
pub mod foundry;
mod imperium;
mod ithessar;
pub mod land;
mod memo;
pub mod mine;
mod resonant;
pub mod setting;
pub mod shield;
pub mod skeleton;
pub mod ways;

use glam::Vec2;

use self::memo::Memo;
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
    /// A small town in the desert, round a dry reservoir's dam or a well.
    Reservoir,
    /// A hull foundry, on whichever ground.
    Foundry(foundry::Ground),
    /// A rail depot on desert.
    Depot,
}

/// Whose place it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Look {
    Imperium,
    Resonant,
    Uthenn,
    Ithessar,
}

impl Look {
    /// `founder`'s own style, for what every faction has a style of (hull
    /// foundries).
    pub fn own(founder: Faction) -> Look {
        match founder {
            Faction::Imperium | Faction::Company => Look::Imperium,
            Faction::Resonant => Look::Resonant,
            Faction::Uthenn => Look::Uthenn,
            Faction::Ithessar => Look::Ithessar,
        }
    }

    /// Whose style it is.
    pub fn faction(self) -> Faction {
        match self {
            Look::Imperium => Faction::Imperium,
            Look::Resonant => Faction::Resonant,
            Look::Uthenn => Faction::Uthenn,
            Look::Ithessar => Faction::Ithessar,
        }
    }

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
    /// (Of a city.)
    pub setting: setting::Setting,
    pub sides: Vec<Faction>,
    pub centerpiece: Centerpiece,
}

impl Spec {
    /// The same, set in `setting`.
    pub fn set_in(self, setting: setting::Setting) -> Spec {
        Spec { setting, ..self }
    }
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
    /// A medium structure, its own or anyone's (see `civic`).
    fn civic(&self, map: &mut Map, lot: &Lot, kind: CivicKind, rng: &mut Rng);
    /// The Hold site's name on the HUD, with `piece` in the middle.
    fn hold_name(&self, piece: Centerpiece, kind: LandmarkKind) -> &'static str;
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
        Look::Ithessar => &ithessar::Ithessar,
        Look::Uthenn => &imperium::Imperium,
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

/// The city skeleton `spec` lays out: the first, from its seed on, whose
/// greybox passes `check`. Worked out only the first time it's asked for
/// (the last few are kept): whose city it is and whose generator stands in
/// it don't change it.
pub fn skeleton(spec: &Spec) -> Skeleton {
    type Key = (u64, usize, city::Plan);
    static KEPT: Memo<Key, Skeleton, 8> = Memo::new();
    let generator = matches!(spec.centerpiece, Centerpiece::ShieldGenerator(..));
    let mut plan = city::Plan::of(spec.seed, spec.sides.len(), generator);
    if spec.look == Look::Ithessar {
        // Their cities run on a plasma river, and it takes room.
        plan.river = true;
        plan.extent = city::Extent::Large;
        plan.form = city::Form::Banks;
    }
    let key = (spec.seed, spec.sides.len(), plan);
    KEPT.get(&key, || lay_out(spec, &plan))
}

fn lay_out(spec: &Spec, plan: &city::Plan) -> Skeleton {
    let mut last = None;
    for attempt in 0..16u64 {
        let mut rng = Rng::new(spec.seed.wrapping_mul(0x9E37_79B9).wrapping_add(attempt));
        let sk = city::skeleton(&mut rng, spec.sides.len(), plan);
        let whole = sk.bases.len() >= spec.sides.len() && (!plan.river || !sk.landmarks.is_empty());
        match check::playable(&sk.greybox(&spec.sides)) {
            Ok(()) if whole => return sk,
            Ok(()) => {}
            Err(why) => log::debug!("map seed {} attempt {attempt}: {why}", spec.seed),
        }
        last = Some(sk);
    }
    last.unwrap()
}

/// Builds `spec`'s map.
fn build_map(spec: &Spec) -> Map {
    let mut map = match spec.biome {
        Biome::City => build(spec).0,
        Biome::Mine => mine::build(spec),
        Biome::Desert | Biome::Reservoir => desert::build(spec),
        Biome::Foundry(ground) => foundry::build(spec, ground),
        Biome::Depot => depot::build(spec),
    };
    ways::lay(&mut map);
    map
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
        st.hold_name(spec.centerpiece, sk.landmarks[0].kind)
    };
    // Every other great building's a site too (but by a shield generator,
    // which is what that fight's for).
    if spec.centerpiece == Centerpiece::Monument {
        map.hold_more = sk
            .landmarks
            .iter()
            .skip(1)
            .map(|l| (l.center, st.hold_name(spec.centerpiece, l.kind)))
            .collect();
    }
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
        let mark = skeleton::Mark::of(&map);
        match lot.kind {
            LotKind::Civic(kind) => st.civic(&mut map, lot, kind, &mut rng),
            _ => drifts.extend(st.lot(&mut map, &sk, lot, &mut rng)),
        }
        mark.raise(&mut map, sk.ground(lot.footprint.center));
        claims.push((Claim::Lot(k), start..map.oboxes.len()));
    }
    let mut terrain = sk.terrain(|p| drifts.iter().map(|d| d.height_at(p)).fold(0.0, f32::max));
    terrain.color = st.ground();
    terrain.material = crate::render::material::Material::Concrete;
    sk.paint(&mut terrain, st.road());
    map.terrain = Some(terrain);
    for p in &sk.land.platforms {
        let mark = skeleton::Mark::of(&map);
        st.platform(&mut map, &sk, p, &mut rng);
        mark.raise(&mut map, sk.land.base(p.center));
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
    for c in &sk.crossings {
        c.build(&mut map, st.stone(), |p| sk.ground(p));
    }
    if let Some(c) = &sk.land.channel {
        c.build(&mut map, sk.size);
        map.rivers = std::sync::Arc::new(vec![c.river()]);
    }
    for (k, b) in sk.bases.iter().enumerate() {
        let mark = skeleton::Mark::of(&map);
        st.base(&mut map, &sk, k, b, &mut rng);
        mark.raise(&mut map, sk.ground(b.middle()));
    }
    for e in &sk.edges {
        // Heaps and barriers, each on the ground where it lies.
        let start = map.oboxes.len();
        st.edge(&mut map, &sk, e, &mut rng);
        if let Some(c) = &sk.land.channel {
            // (None of it in the river.)
            let heap = map.oboxes.split_off(start);
            map.oboxes.extend(
                heap.into_iter()
                    .filter(|o| !c.wet(o.center, o.half.length())),
            );
        }
        for o in &mut map.oboxes[start..] {
            let h = sk.ground(o.center);
            skeleton::lift(std::slice::from_mut(o), h);
        }
    }
    st.beyond(&mut map, &sk, &mut rng);
    if let Some(t) = &mut map.scenery_terrain {
        sk.paint(t, st.road());
    }
    st.finish(&mut map);
    if spec.setting == setting::Setting::Desert {
        setting::desert(&mut map, &sk, spec.seed, st.road());
    }
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
    // (The whole suite runs in one process, on a few dozen maps.)
    const KEEP: usize = if cfg!(test) { 24 } else { 6 };
    static KEPT: Memo<Spec, Map, KEEP> = Memo::new();
    KEPT.get(spec, || build_map(spec))
}

/// How a world of `biome` looks from orbit.
fn planet(biome: Biome) -> crate::planet::Look {
    match biome {
        Biome::City => city::PLANET,
        Biome::Mine => mine::PLANET,
        Biome::Desert | Biome::Reservoir => desert::PALETTES[0].planet,
        Biome::Foundry(ground) => ground.planet(),
        Biome::Depot => foundry::Ground::Desert.planet(),
    }
}

/// The skeleton `spec` lays out, as its greybox.
pub fn greybox(spec: &Spec) -> Map {
    match spec.biome {
        Biome::Mine => return mine::greybox(spec),
        Biome::Desert | Biome::Reservoir => return desert::greybox(spec),
        Biome::Foundry(ground) => return foundry::greybox(spec, ground),
        Biome::Depot => return depot::greybox(spec),
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

/// Where the menus show each side's tank: by its spawns, in the sun,
/// the camera looking past it toward the middle.
fn sunlit_views(m: &Map, bases: &[Vec2], middle: Vec2) -> Vec<(Vec2, f32)> {
    let sun = m.atmosphere.sun_dir.normalize();
    let lm = middle;
    let view = |p: Vec2| {
        let d = lm - p;
        (-d.x).atan2(-d.y)
    };
    bases
        .iter()
        .map(|&b| {
            let want = b.lerp(lm, 0.12);
            let mut spots = Vec::new();
            let mut u = -30.0;
            while u <= 30.0 {
                let mut v = -30.0;
                while v <= 30.0 {
                    spots.push(want + Vec2::new(u, v));
                    v += 3.0;
                }
                u += 3.0;
            }
            spots.sort_by(|a, c| a.distance(want).total_cmp(&c.distance(want)));
            let good = |p: Vec2| {
                let g = m.ground_height(p.x, p.y);
                let at = glam::Vec3::new(p.x, g + 1.9, p.y);
                let blocked =
                    |dir: glam::Vec3, len: f32| m.oboxes.iter().any(|o| o.ray_hit(at, dir, len));
                let under = m
                    .oboxes
                    .iter()
                    .any(|o| o.distance(p) < 3.4 && o.y1 > g + 0.05 && o.y0 < g + 3.0);
                let v = view(p);
                let back = Vec2::new(v.sin(), v.cos());
                let level = (0..=14).all(|k| {
                    let q = p + back * k as f32;
                    (m.ground_height(q.x, q.y) - g).abs() < 0.5
                });
                level && !under && !blocked(sun, 300.0) && !blocked(glam::Vec3::Y, 60.0)
            };
            let p = spots.into_iter().find(|&p| good(p)).unwrap_or(b);
            (p, view(p))
        })
        .collect()
}

impl Map {
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
        map(&crate::mine_spec(seed, Look::Ithessar))
    }

    /// The Imperium's hull foundry `seed` names, on desert (see
    /// `crate::foundry_spec`).
    #[cfg(test)]
    pub fn foundry(seed: u64) -> Map {
        map(&crate::foundry_spec(
            seed,
            Look::Imperium,
            foundry::Ground::Desert,
        ))
    }

    /// The Imperium's rail depot `seed` names (see `crate::depot_spec`).
    #[cfg(test)]
    pub fn depot(seed: u64) -> Map {
        map(&crate::depot_spec(seed, Look::Imperium))
    }

    /// The open desert `seed` names (see `crate::desert_spec`).
    pub fn desert(seed: u64) -> Map {
        map(&crate::desert_spec(seed))
    }

    /// The reservoir town `seed` names (see `crate::reservoir_spec`).
    pub fn reservoir(seed: u64) -> Map {
        map(&crate::reservoir_spec(seed))
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
        [Look::Imperium, Look::Resonant, Look::Ithessar]
            .into_iter()
            // (The Ithessar's are the biggest: not unoptimized.)
            .filter(|&l| l != Look::Ithessar || !cfg!(debug_assertions))
            .flat_map(|look| SEEDS.map(|s| (s, look)))
            .filter(|&(s, l)| l != Look::Ithessar || s < 2)
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
                    sk.edges
                        .iter()
                        .any(|e| (a.to - e.start).perp_dot(e.along).abs() < 0.01
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
            let sk = skeleton(&crate::city_spec(seed, look));
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
            let sk = skeleton(&crate::city_spec(seed, look));
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
                        if sk.out(p) > -11.0 {
                            continue;
                        }
                        // (Once: the ground's slow to work out.)
                        let g = sk.ground(p);
                        let hit = m.oboxes.iter().find(|o| {
                            let (y0, _) = o.y_range();
                            o.solid && !o.ramp && y0 < g + 3.0 && o.distance(p) < 1.0
                        });
                        assert!(
                            hit.is_none(),
                            "seed {seed}: avenue {dir:?} blocked at {r}, {across}: {hit:?}"
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

    /// Five seeds in a row are five forms, and there are cities of every
    /// size and density.
    #[test]
    fn cities_vary() {
        use super::city::Plan;
        let plans: Vec<Plan> = (0..64).map(|s| Plan::of(s, 2, false)).collect();
        let forms: std::collections::HashSet<_> = plans[..5].iter().map(|p| p.form).collect();
        assert_eq!(forms.len(), 5);
        let extents: std::collections::HashSet<_> = plans.iter().map(|p| p.extent).collect();
        let densities: std::collections::HashSet<_> = plans.iter().map(|p| p.density).collect();
        assert_eq!((extents.len(), densities.len()), (3, 3));
        // A generator always has a great building's place to stand in.
        for seed in 0..8 {
            let spec = crate::shield_city_spec(seed, Faction::Uthenn);
            assert!(!skeleton(&spec).landmarks.is_empty(), "seed {seed}");
        }
    }

    /// How much of a city's ground (off its plazas and courts) is built on.
    fn built(sk: &Skeleton) -> f32 {
        let (mut all, mut on) = (0, 0);
        let n = (sk.size / 3.0) as i32;
        for i in 0..n {
            for j in 0..n {
                let p = Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * 3.0;
                if sk.off_plazas(p) < 4.0 || sk.bases.iter().any(|b| b.contains(p, 4.0)) {
                    continue;
                }
                all += 1;
                on += sk
                    .lots
                    .iter()
                    .any(|l| l.kind != LotKind::Rubble && l.footprint.distance(p) <= 0.0)
                    as i32;
            }
        }
        on as f32 / all as f32
    }

    #[test]
    #[ignore = "a probe"]
    fn how_hilly() {
        for seed in 0..16 {
            let sk = skeleton(&spec(seed));
            let n = (sk.size / 4.0) as i32;
            let (mut lo, mut hi, mut steep) = (f32::MAX, f32::MIN, 0.0f32);
            let (mut gl, mut gh) = (f32::MAX, f32::MIN);
            for i in 0..n {
                for j in 0..n {
                    let p = Vec2::new(i as f32, j as f32) * 4.0;
                    let h = sk.land.swell(p);
                    (lo, hi) = (lo.min(h), hi.max(h));
                    let g = sk.land.base(p);
                    (gl, gh) = (gl.min(g), gh.max(g));
                    steep = steep.max((sk.land.swell(p + Vec2::X * 4.0) - h).abs() / 4.0);
                }
            }
            println!(
                "hilly {seed} size {} land {lo:.1}..{hi:.1} ground {gl:.1}..{gh:.1} steepest {steep:.2} {}",
                sk.size,
                sk.land.describe()
                    + &sk
                        .crossings
                        .iter()
                        .map(|c| format!(
                            " | {:?} {:.0} m, {:.0} to {:.0}, floor {:.1} top {:.1}",
                            c.kind,
                            c.len(),
                            c.from,
                            c.to,
                            c.floor,
                            c.top
                        ))
                        .collect::<String>()
            );
        }
    }

    #[test]
    #[ignore = "a probe"]
    fn where_greats() {
        for look in [Look::Imperium, Look::Ithessar] {
            for seed in 0..10 {
                let sk = skeleton(&crate::city_spec(seed, look));
                for l in &sk.landmarks {
                    let (c, g) = (l.center, sk.ground(l.center));
                    println!(
                        "great {seed} {:?} {:.0} {:.0} {g:.1} {look:?}",
                        l.kind, c.x, c.y
                    );
                }
            }
        }
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn river_bridges_go_over_the_river() {
        for seed in 0..3 {
            let spec = crate::city_spec(seed, Look::Ithessar);
            let sk = skeleton(&spec);
            let m = build(&spec).0;
            if let Err(why) = check::playable(&m) {
                panic!("seed {seed}: {why}");
            }
            let c = sk.land.channel.as_ref().unwrap();
            assert!(c.bridges.len() >= 3, "seed {seed}: {}", c.bridges.len());
            let nav = crate::ai::nav::NavGrid::build(&m);
            let mut crossed = 0;
            for b in &c.bridges {
                let [a, z] = b.ends;
                let deck = c.deck(b);
                for k in 0..=40 {
                    for e in [-1.0f32, 0.0, 1.0] {
                        let d = (z - a).normalize();
                        let p = a.lerp(z, k as f32 / 40.0) + Vec2::new(-d.y, d.x) * e * b.half;
                        let Some(y) = m.rivers[0].surface(p, 0.0) else {
                            continue;
                        };
                        assert!(
                            deck - y > 4.0,
                            "seed {seed}: a deck only {:.1} m over the river",
                            deck - y
                        );
                    }
                }
                let path = nav.path(a, z);
                let len: f32 = path.windows(2).map(|w| w[0].distance(w[1])).sum();
                if path.is_empty() || len >= a.distance(z) * 1.6 {
                    println!(
                        "seed {seed} bridge {:.0}: {len:.0} m against {:.0}, ends blocked {} {}, middle {}",
                        c.point(b.t, 0.0),
                        a.distance(z),
                        nav.is_blocked(a),
                        nav.is_blocked(z),
                        nav.is_blocked(c.point(b.t, 0.0))
                    );
                }
                crossed += (!path.is_empty() && len < a.distance(z) * 1.6) as usize;
            }
            assert!(
                crossed * 10 >= c.bridges.len() * 8,
                "seed {seed}: bots cross {crossed} of {} bridges",
                c.bridges.len()
            );
        }
    }

    /// Every Sun and Weir is joined to the river, and what carries that
    /// stands in no road.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn suns_and_weirs_are_joined_to_the_river() {
        let mut seen = 0;
        for seed in 0..6 {
            let sk = skeleton(&crate::city_spec(seed, Look::Ithessar));
            let c = sk.land.channel.as_ref().unwrap();
            for (k, lm) in sk.landmarks.iter().enumerate() {
                let Some(conduit) = sk.conduits.iter().find(|c| c.landmark == k) else {
                    assert_eq!(lm.kind, LandmarkKind::Great(1), "seed {seed}");
                    continue;
                };
                seen += 1;
                assert!(
                    c.local(conduit.way[2]).1.abs() < c.half,
                    "seed {seed}: it ends short of the river"
                );
                assert!(conduit.portals.len() >= 3, "seed {seed}: {conduit:?}");
                for leg in conduit.legs() {
                    for s in &sk.streets {
                        let keep = if s.rank == Rank::Avenue {
                            s.clear
                        } else {
                            s.half
                        };
                        assert!(
                            s.distance(leg) > keep,
                            "seed {seed}: a leg at {leg} in {s:?}"
                        );
                    }
                }
            }
        }
        assert!(seen >= 3, "{seen}");
    }

    /// Bots drive over a river city's bridges, on their own and two meeting
    /// head on, without stopping on the way, climbing a parapet or going in.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn bots_cross_river_bridges() {
        use glam::Vec3;

        use crate::ai::{Bot, Contact, Objective, nav::NavGrid};
        use crate::game::DT;
        use crate::sim::{HullKind, TurretKind, World};
        let mut runs = 0;
        let mut bad = Vec::new();
        // (In open, middling and cramped cities.)
        for seed in [0, 1, 3] {
            let m = build(&crate::city_spec(seed, Look::Ithessar)).0;
            let c = m.skeleton.as_ref().unwrap().land.channel.clone().unwrap();
            let nav = NavGrid::build(&m);
            for hull in [HullKind::Kataphrakt, HullKind::Sethuun, HullKind::Gravis] {
                for b in &c.bridges {
                    let d = (b.ends[1] - b.ends[0]).normalize();
                    // (Off the grid: a hover hull set down right along its
                    // lines can miss the ground.)
                    let ends = [(b.ends[0], -d), (b.ends[1], d)].map(|(p, out)| {
                        nav.nearest_open_point(p + out * 8.0).unwrap() + Vec2::new(0.13, 0.17)
                    });
                    for two in [false, true] {
                        let mut w = World::empty(m.clone());
                        let n = if two { 2 } else { 1 };
                        for k in 0..n {
                            let to = ends[1 - k] - ends[k];
                            w.spawn_tank(
                                Vec3::ONE,
                                ends[k],
                                to.x.atan2(to.y),
                                hull,
                                TurretKind::Kontos,
                            );
                        }
                        let contacts = [0, 1].map(|_| Contact {
                            targetable: false,
                            team: 0,
                            health: 1.0,
                        });
                        let mut bots = [Bot::new(seed + 5), Bot::new(seed + 9)];
                        let mut done = [None; 2];
                        let mut tilt = 1.0f32;
                        let mut stopped = 0.0;
                        let mut wet = false;
                        let mut t = 0.0;
                        while t < 30.0 && done[..n].iter().any(|d| d.is_none()) {
                            let cmds: Vec<_> = (0..n)
                                .map(|k| {
                                    let o = Objective {
                                        at: ends[1 - k],
                                        radius: 2.0,
                                        level: f32::INFINITY,
                                        fight: false,
                                    };
                                    bots[k].think(k, &w, &contacts, Some(o), &[], &nav, DT).0
                                })
                                .collect();
                            w.step(&cmds);
                            t += DT;
                            for k in 0..n {
                                let q = &w.tanks[k];
                                let p = Vec2::new(q.pos.x, q.pos.z);
                                if p.distance(ends[1 - k]) < 6.0 {
                                    done[k].get_or_insert(t);
                                }
                                if t > 2.0 && done[k].is_none() {
                                    tilt = tilt.min(q.up().y);
                                    if q.speed().abs() < 1.0 {
                                        stopped += DT;
                                    }
                                }
                                wet |= q.pos.y < c.quay_at(b.t) - 2.0;
                            }
                        }
                        runs += 1;
                        let late = done[..n].iter().any(|d| d.is_none());
                        if late || tilt < 0.9 || wet || stopped > if two { 2.0 } else { 0.5 } {
                            bad.push((seed, hull, b.t, two, done, tilt, stopped, wet));
                        }
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{} of {runs}: {bad:#?}", bad.len());
    }

    /// Draws the layouts of a few river cities, as they're seen from above
    /// (`DRAW_TO`: where).
    #[test]
    #[ignore = "a probe"]
    fn draw_rivers() {
        let Ok(dir) = std::env::var("DRAW_TO") else {
            return;
        };
        for seed in 0..6 {
            let sk = skeleton(&crate::city_spec(seed, Look::Ithessar));
            let c = sk.land.channel.as_ref().unwrap();
            let n = (sk.size / 2.0) as usize;
            let mut px = Vec::with_capacity(n * n * 3);
            for j in 0..n {
                for i in 0..n {
                    let p = Vec2::new(i as f32 + 0.5, j as f32 + 0.5) * 2.0;
                    let lot = sk.lots.iter().find(|l| l.footprint.distance(p) <= 0.0);
                    let rgb: [u8; 3] = if c.bridges.iter().any(|b| {
                        let d = b.ends[1] - b.ends[0];
                        let t = ((p - b.ends[0]).dot(d) / d.length_squared()).clamp(0.0, 1.0);
                        p.distance(b.ends[0] + d * t) < b.half
                    }) {
                        [250, 250, 250]
                    } else if c.local(p).1.abs() < c.half {
                        [255, 40, 150]
                    } else if c.wet(p, 0.0) {
                        [60, 60, 90]
                    } else if sk.landmarks.iter().any(|l| l.center.distance(p) < l.radius) {
                        [240, 200, 60]
                    } else if sk.bases.iter().any(|b| b.contains(p, 0.0)) {
                        [60, 160, 255]
                    } else if sk.edges.iter().any(|e| {
                        let q = p - e.start;
                        let t = q.dot(e.along);
                        t > e.from && t < e.to && q.perp_dot(e.along).abs() < 2.5
                    }) {
                        [230, 60, 40]
                    } else if let Some(l) = lot {
                        match l.kind {
                            LotKind::Distant => [45, 45, 45],
                            LotKind::Civic(_) => [90, 180, 110],
                            _ => [130, 120, 110],
                        }
                    } else if sk.road(p) > 0.5 {
                        [95, 85, 70]
                    } else {
                        [70, 64, 56]
                    };
                    let dim = if sk.out(p) > 0.0 { 2 } else { 1 };
                    px.extend(rgb.map(|v| v / dim));
                }
            }
            let mut out = format!("P6 {n} {n} 255\n").into_bytes();
            out.extend(px);
            std::fs::write(format!("{dir}/plan{seed}.ppm"), out).unwrap();
            println!(
                "river {seed} size {} bridges {} great {} courts {} lots {} {:?}",
                sk.size,
                c.bridges.len(),
                sk.landmarks.len(),
                sk.bases.len(),
                sk.lots.len(),
                check::playable(&sk.greybox(&crate::city_spec(seed, Look::Ithessar).sides))
            );
        }
    }

    #[test]
    #[ignore = "a probe"]
    fn where_civics() {
        for seed in [4, 8] {
            let sk = skeleton(&spec(seed));
            for l in &sk.lots {
                if let LotKind::Civic(k) = l.kind {
                    let c = l.footprint.center;
                    println!(
                        "civic {seed} {k:?} {:.0} {:.0} {:.1}",
                        c.x,
                        c.y,
                        sk.ground(c)
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "a probe"]
    fn how_built_up() {
        for seed in 0..24 {
            let sk = skeleton(&spec(seed));
            let plan = super::city::Plan::of(seed, 2, false);
            println!(
                "built {seed} {:?} {:?} {:?} {:.3}",
                plan.density,
                plan.form,
                plan.extent,
                built(&sk)
            );
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

    /// A cramped city's more built up than a middling one, and that than an
    /// open one.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "slow unoptimized: run with cargo test --release"
    )]
    fn density_shows() {
        use super::city::{Density, Plan};
        let mean = |density: Density| {
            let of: Vec<f32> = (0..24)
                .filter(|&s| Plan::of(s, 2, false).density == density)
                .map(|s| built(&skeleton(&spec(s))))
                .collect();
            of.iter().sum::<f32>() / of.len() as f32
        };
        let (c, m, o) = (
            mean(Density::Cramped),
            mean(Density::Middling),
            mean(Density::Open),
        );
        assert!(c > m * 1.25 && m > o * 1.15, "{c:.2} {m:.2} {o:.2}");
    }

    /// The plaza's slope is gentle enough to drive up anywhere.
    #[test]
    fn the_plaza_slope_is_drivable() {
        for seed in SEEDS {
            let sk = skeleton(&spec(seed));
            for pl in &sk.land.platforms {
                for k in 0..16 {
                    let a = k as f32 / 16.0 * std::f32::consts::TAU;
                    let dir = Vec2::new(a.cos(), a.sin());
                    let mut prev = pl.height_at(pl.center);
                    for r in 1..(pl.half as i32 * 2) {
                        let h = pl.height_at(pl.center + dir * r as f32);
                        assert!((prev - h).abs() < 0.25, "seed {seed}: plaza edge too steep");
                        prev = h;
                    }
                }
            }
        }
    }
}
