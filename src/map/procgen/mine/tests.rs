//! The crystal mine, checked over several seeds (each a different kind:
//! one mine or two, a wash or two or none, buttes), and `sweep` over many
//! more. Mostly cheap checks on what's generated (slopes, clearances, nav
//! paths, routes on their levels), not drives that test the driving.

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::{LazyLock, Mutex};

use glam::{Vec2, Vec3};

use super::dig::{FACE_BLOCK, MESA_TOP, keep_clear};
use super::*;
use crate::ai::NavGrid;
use crate::map::OBox;
use crate::render::material::Material;

/// The seeds the tests build (in an optimized run: unoptimized, building
/// and checking a mine takes too long), three of different kinds. `sweep`
/// runs the checks over many more.
const SEEDS: &[u64] = &[0, 3, 9];

/// The heaviest hull climbs a little over 20 degrees.
const CLIMB: f32 = 20.0;

fn lay(seed: u64) -> Layout {
    static CACHE: LazyLock<Mutex<HashMap<u64, Layout>>> = LazyLock::new(Default::default);
    if let Some(l) = CACHE.lock().unwrap().get(&seed) {
        return l.clone();
    }
    let l = layout(&crate::mine_spec(seed));
    CACHE.lock().unwrap().insert(seed, l.clone());
    l
}

/// The mine `seed` names the way round its layout is: the checks read
/// their places off the layout. (`mirrored_mines_are_mirror_images` checks
/// the other way round.)
fn mine(seed: u64) -> Map {
    static CACHE: LazyLock<Mutex<HashMap<u64, Map>>> = LazyLock::new(Default::default);
    if let Some(m) = CACHE.lock().unwrap().get(&seed) {
        return m.clone();
    }
    let m = dig::build(
        &lay(seed),
        &crate::mine_spec(seed),
        &super::ithessar::Ithessar,
    );
    let mut cache = CACHE.lock().unwrap();
    if cache.len() >= 4 {
        cache.clear();
    }
    cache.insert(seed, m.clone());
    m
}

fn flat(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}

fn mid(r: Rect) -> Vec2 {
    (r.0 + r.1) * 0.5
}

/// A drop straight down from high above `p` lands on this height.
fn land_on(w: &crate::sim::World, p: Vec2, from: f32) -> f32 {
    from - w.raycast(Vec3::new(p.x, from, p.y), -Vec3::Y, from + 20.0)
}

/// The drivable ground nearest `p`.
fn open(nav: &NavGrid, p: Vec2) -> Vec2 {
    (0..200)
        .map(|k| {
            let a = k as f32 * 2.4;
            p + Vec2::new(a.cos(), a.sin()) * (k as f32 * 0.1)
        })
        .find(|&q| !nav.is_blocked(q))
        .unwrap_or(p)
}

/// Most draws of the grammar hold together, and every kind turns up: one
/// mine or two, none to two washes, none to two buttes.
#[test]
fn draws_mostly_hold_together() {
    let mut ok = 0;
    let mut why = std::collections::BTreeMap::new();
    let mut kinds = std::collections::BTreeSet::new();
    for k in 0..200u64 {
        let mut rng = Rng::new(k * 7919);
        match Layout::generate(&mut rng) {
            Ok(l) => {
                ok += 1;
                kinds.insert((l.mines.len(), l.washes.len(), l.buttes.len()));
            }
            Err(line) => *why.entry(line).or_insert(0) += 1,
        }
    }
    eprintln!("{ok} of 200 hold together; the rest failed at lines {why:?}");
    eprintln!("kinds (mines, washes, buttes): {kinds:?}");
    assert!(ok > 70, "{ok} of 200");
    for mines in 1..=2 {
        for washes in 0..=2 {
            assert!(
                kinds.iter().any(|k| k.0 == mines && k.1 == washes),
                "never {mines} mines and {washes} washes"
            );
        }
    }
    assert!(kinds.iter().any(|k| k.2 == 2), "never two buttes");
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn different_seeds_make_different_mines() {
    let ls: Vec<Layout> = (0..6).map(lay).collect();
    for (i, a) in ls.iter().enumerate() {
        for b in &ls[i + 1..] {
            let same = a.caves == b.caves && a.lines == b.lines;
            assert!(!same, "two seeds laid out the same mine");
        }
    }
    let again = lay(1);
    assert_eq!(again.caves, ls[1].caves);
    assert_eq!(again.lines, ls[1].lines);
}

/// Each seed's dressed map is playable: every side has its spawns, reaches
/// the Hold site and the other side; and has the rest of what a map needs.
fn check_every_seed_is_playable_dressed(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    assert_eq!(m.factions.len(), 2);
    if let Err(why) = crate::map::procgen::check::playable(&m) {
        panic!("seed {seed}: {why}");
    }
    assert!(
        m.open_spots.len() >= 20,
        "seed {seed}: {} open",
        m.open_spots.len()
    );
    assert!(
        m.repair_spots.len() >= 3,
        "seed {seed}: {:?}",
        m.repair_spots
    );
    assert_eq!(m.raid_stands.len(), 2, "seed {seed}");
    assert_eq!(m.vistas.len(), 2, "seed {seed}");
    assert_eq!(m.beams.len(), l.mines.len(), "seed {seed}");
    assert!(m.routes.len() >= 3 * l.mines.len(), "seed {seed}");
}

/// Each map built the way round it's meant to be (mirrored, turned) is
/// its layout's map moved there piece for piece, and as playable.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn turned_mines_are_their_layouts_turned() {
    let seed = (0..20)
        .find(|&s| {
            let l = lay(s);
            l.mirror && l.turns % 2 == 1
        })
        .expect("no mirrored, turned mine in 20 seeds");
    let (l, m, built) = (lay(seed), mine(seed), Map::mine(seed));
    let mut again = m.clone();
    l.orient(&mut again);
    assert_eq!(again.oboxes.len(), built.oboxes.len());
    for (a, b) in again.oboxes.iter().zip(&built.oboxes) {
        assert!(a.center.distance(b.center) < 1e-3);
    }
    // The ground moved with it: where a box stands on it, it still does.
    let o = &m.oboxes[m.oboxes.len() / 2];
    let i = m.oboxes.len() / 2;
    let g = |m: &Map, p: Vec2| m.ground_height(p.x, p.y);
    assert!((g(&m, o.center) - g(&built, built.oboxes[i].center)).abs() < 0.05);
    if let Err(why) = crate::map::procgen::check::playable(&built) {
        panic!("seed {seed} turned: {why}");
    }
    let nav = NavGrid::build(&built);
    for p in built.routes.iter().flatten() {
        let f = nav.floor_at(*p);
        assert!(f.is_some_and(|f| (f - p.y).abs() < 1.0), "{p} is on {f:?}");
    }
}

fn check_no_stone_drawn_as_a_box(seed: u64) {
    let map = mine(seed);
    for o in &map.oboxes {
        let stone =
            [Material::Rubble, Material::Cliff].contains(&o.side_mat) && o.top_mat == o.side_mat;
        assert!(
            !stone || o.hidden || o.rock.is_some(),
            "seed {seed}: a stone box drawn at {} ({}..{})",
            o.center,
            o.y0,
            o.y1
        );
        assert!(
            o.color != FACE_BLOCK,
            "seed {seed}: a stand-in left at {}",
            o.center
        );
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn no_stone_drawn_as_a_box() {
    for &seed in SEEDS {
        check_no_stone_drawn_as_a_box(seed);
    }
}

/// Out on the mesa the caves' roofs read as ground (#249): no slab is
/// drawn over them, and the roof drawn in its place is level with the rock
/// round it.
fn check_roofs_are_ground_not_slabs(seed: u64) {
    let (l, map) = (lay(seed), mine(seed));
    for o in &map.oboxes {
        let roof = o.y0 == ROOF.0 && o.y1 == ROOF.1 && o.color == MESA_TOP;
        assert!(
            !roof || o.hidden,
            "seed {seed}: a roof slab drawn at {}",
            o.center
        );
    }
    // Over the hall's west end, the west adit and the breach: the roof's
    // top is drawn just where the ground runs on round it.
    for mn in &l.mines {
        let (wa, br) = (mid(mn.west_adit), mid(mn.breach));
        for p in [
            Vec2::new(mn.hall.0.x + 4.0, mid(mn.hall).y),
            Vec2::new(wa.x, l.face(wa.x) + 10.0),
            Vec2::new(br.x, l.face(br.x) + 14.0),
        ] {
            let top = map
                .sculpted
                .verts
                .iter()
                .filter(|v| Vec2::new(v.pos[0], v.pos[2]).distance(p) < 1.5)
                .map(|v| v.pos[1])
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                (top - l.uncut(p)).abs() < 0.1,
                "seed {seed}: roof at {p} drawn at {top}, rock at {}",
                l.uncut(p)
            );
        }
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn roofs_are_ground_not_slabs() {
    for &seed in SEEDS {
        check_roofs_are_ground_not_slabs(seed);
    }
}

/// The rock's flat top along the Face, the cap back over its rim, is
/// never under the ground (#290): buried in a slope it pokes through in
/// patches. (Its foot is buried on purpose.)
fn check_the_faces_top_is_never_buried(seed: u64) {
    let (l, map) = (lay(seed), mine(seed));
    let (v, ix) = (&map.sculpted.verts, &map.sculpted.indices);
    let mut buried = 0.0;
    let mut worst = (0.0, Vec3::ZERO);
    for t in ix.chunks(3) {
        let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(v[t[k] as usize].pos));
        let cap = t.iter().all(|&k| {
            let c = v[k as usize].color;
            c[3] == Material::Cliff.id() && Vec3::new(c[0], c[1], c[2]) == crate::map::crag::TOP
        });
        if !cap {
            continue;
        }
        let n = (b - a).cross(c - a);
        let area = n.length() * 0.5;
        // Up-facing: the cap over the rim, not the face.
        if area < 1e-4 || n.normalize().y < 0.9 {
            continue;
        }
        for w in [
            Vec3::splat(1.0 / 3.0),
            Vec3::new(0.5, 0.5, 0.0),
            Vec3::new(0.0, 0.5, 0.5),
            Vec3::new(0.5, 0.0, 0.5),
        ] {
            let p = a * w.x + b * w.y + c * w.z;
            // The mesa's Face, in the map: not the mine's insides, under
            // the ground, or the Crag's top, 40 m up.
            let q = Vec2::new(p.x, p.z);
            if l.cave_distance(q) < 3.0
                || l.crag_depth(q) > -3.0
                || q.max_element() > l.size
                || q.min_element() < 0.0
            {
                continue;
            }
            let under = map.ground_height(p.x, p.z) - p.y;
            if under > 0.1 {
                buried += area / 4.0;
            }
            if under > worst.0 {
                worst = (under, p);
            }
        }
    }
    assert!(
        buried == 0.0,
        "seed {seed}: {buried:.1} m2 of the Face's top under the ground, worst {:.2} m at {}",
        worst.0,
        worst.1
    );
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn the_faces_top_is_never_buried() {
    for &seed in SEEDS {
        check_the_faces_top_is_never_buried(seed);
    }
}

/// How far a point is out of the stone drawn for `o` (< 0: in it). A rock
/// is measured to the hull of its surface (what it collides as), a block or
/// plain box to its box.
fn stone_distance(o: &OBox) -> impl Fn(Vec3) -> f32 + '_ {
    let hull = o
        .rock_points()
        .and_then(|pts| rapier3d::parry::shape::SharedShape::convex_hull(&pts));
    move |p: Vec3| {
        if let Some(hull) = &hull {
            let pr = hull.project_local_point(p, true);
            let d = p.distance(pr.point);
            return if pr.is_inside { -d } else { d };
        }
        let h = Vec3::new(o.half.x, (o.y1 - o.y0) * 0.5, o.half.y);
        let q = (o.rotation().inverse() * (p - o.center3())).abs() - h;
        q.max(Vec3::ZERO).length() + q.max_element().min(0.0)
    }
}

/// At every spawn on both sides, swinging the turret all the way round,
/// the chase camera keeps clear of the rock: never in it, never so close
/// its near plane cuts into it (#284).
/// its near plane cuts into it (#284).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn the_chase_camera_keeps_out_of_the_rock_at_spawn() {
    use crate::scene::ChaseCam;
    for &seed in SEEDS {
        let mut w = crate::sim::World::empty(mine(seed));
        let map = w.map.clone();
        w.spawn_tank(
            Vec3::ONE,
            map.bases[0][0],
            0.0,
            crate::sim::HullKind::Kataphrakt,
            crate::sim::TurretKind::Kontos,
        );
        let stone: Vec<(&OBox, _)> = map.oboxes.iter().map(|o| (o, stone_distance(o))).collect();
        let mut failures = Vec::new();
        for side in 0..2 {
            let enemy = &map.bases[1 - side];
            let look = enemy.iter().sum::<Vec2>() / enemy.len() as f32;
            for &spot in &map.bases[side] {
                w.teleport(0, spot, crate::sim::yaw_of(look - spot));
                for _ in 0..5 {
                    w.step(&[crate::sim::Command::default()]);
                }
                let at = flat(w.tanks[0].pos);
                let near: Vec<_> = stone
                    .iter()
                    .filter(|(o, _)| o.distance(at) < 16.0)
                    .collect();
                let mut cam = ChaseCam::new();
                let mut worst = (f32::INFINITY, 0.0);
                for k in 0..=24 {
                    let mut tank = w.tanks[0].clone();
                    tank.turret_rel = k as f32 * TAU / 24.0;
                    for _ in 0..12 {
                        let c = cam.update(&w, &tank, 1.0 / 60.0);
                        let d = near
                            .iter()
                            .map(|(_, d)| d(c.pos))
                            .fold(f32::INFINITY, f32::min);
                        if d < worst.0 {
                            worst = (d, tank.turret_rel);
                        }
                    }
                }
                if worst.0 < 0.3 {
                    failures.push(format!(
                        "seed {seed} side {side} spawn {spot}: camera {:.2} m from rock, turret {:.0}°",
                        worst.0,
                        worst.1.to_degrees()
                    ));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

/// The levels stack: each bridge's deck over its wash's floor, the mesa's
/// top (or a Crag's) over each mine's floor.
fn check_levels_stack(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let w = crate::sim::World::empty(m);
    for br in &l.bridges {
        let (a, b) = (br.a, br.b);
        let dir = (b - a).normalize();
        let c = line_at(&l.washes[br.wash].line, br.s).0;
        let p = a + dir * (c - a).dot(dir);
        assert!(
            (land_on(&w, p, 30.0) - DECK_TOP).abs() < 0.1,
            "seed {seed}: deck at {p}"
        );
        assert!(
            land_on(&w, p, -1.0) < -WASH_DEPTH + 1.0,
            "seed {seed}: wash floor at {p}"
        );
    }
    // (Off the ground's 2 m grid: a ray down a grid line can slip.)
    let off = Vec2::splat(0.3);
    for mn in &l.mines {
        for p in [
            Vec2::new(mid(mn.adit).x, mn.bay.0.y + 15.0),
            Vec2::new(mn.hall.0.x + 2.0, mn.hall.0.y + 11.0),
            mid(mn.drift),
        ]
        .map(|p| p + off)
        {
            let roof = if l.under_crag(p) { CRAG_ROOF } else { ROOF.1 };
            assert!(
                (land_on(&w, p, 60.0) - roof).abs() < 0.1,
                "seed {seed}: roof over {p}"
            );
            assert!(
                land_on(&w, p, ROOF.0 - 0.5).abs() < 0.6,
                "seed {seed}: floor under {p}"
            );
        }
        assert!(
            land_on(&w, mid(mn.bay), 60.0) > CRAG_TOP - 1.0,
            "seed {seed}: no Crag"
        );
    }
}

/// From each side's spawns, everything worth taking is a nav path away
/// (each bridge's ends and the wash under it, each mine's mouths and
/// rooms, the mesa's top, each butte's top), and neither side is much
/// further than the other from it all.
fn check_every_spawn_reaches_everything_and_the_sides_are_fair(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    for (side, base) in m.bases.iter().enumerate() {
        assert!(
            base.len() >= 14,
            "seed {seed}: side {side} has {} spawns",
            base.len()
        );
    }
    let nav = NavGrid::build(&m);
    let s = l.size;
    let mut goals: Vec<(String, Vec2)> = Vec::new();
    for (k, br) in l.bridges.iter().enumerate() {
        let dir = (br.b - br.a).normalize();
        goals.push((format!("bridge {k}'s far end"), br.b + dir * 6.0));
        goals.push((
            format!("the wash under bridge {k}"),
            line_at(&l.washes[br.wash].line, br.s - 9.0).0,
        ));
    }
    for (k, mn) in l.mines.iter().enumerate() {
        let x = mid(mn.adit).x - 10.0;
        goals.push((format!("mine {k}'s landing"), Vec2::new(x, l.face(x) - 6.0)));
        goals.push((
            format!("mine {k}'s hall"),
            Vec2::new(mn.hall.0.x + 2.0, mn.hall.0.y + 14.0),
        ));
        goals.push((
            format!("the mesa east of mine {k}"),
            Vec2::new(mn.box_cut.1.x + 26.0, s - 24.0),
        ));
    }
    goals.push((
        "the mesa's west".into(),
        Vec2::new(l.west_ramp.1 - 4.0, s - 30.0),
    ));
    for (k, b) in l.buttes.iter().enumerate() {
        goals.push((format!("butte {k}'s top"), b.center));
    }
    let goals: Vec<(String, Vec2)> = goals.into_iter().map(|(n, p)| (n, open(&nav, p))).collect();
    let length = |path: &[Vec2], from: Vec2| {
        let mut at = from;
        let mut len = 0.0;
        for &p in path {
            len += at.distance(p);
            at = p;
        }
        len
    };
    for &p in m.open_spots.iter().chain(m.bases.iter().flatten()) {
        for (name, g) in &goals {
            assert!(
                !nav.path(p, *g).is_empty(),
                "seed {seed}: no path from {p} to {name}"
            );
        }
    }
    let middle =
        |side: usize| m.bases[side].iter().copied().sum::<Vec2>() / m.bases[side].len() as f32;
    let total = |side: usize| {
        goals
            .iter()
            .map(|(_, g)| length(&nav.path(middle(side), *g), middle(side)))
            .sum::<f32>()
    };
    let (a, b) = (total(0), total(1));
    eprintln!("seed {seed}: {a:.0} m from the west, {b:.0} m from the east");
    assert!(
        (a - b).abs() / a.max(b) < 0.3,
        "seed {seed}: sides are {a:.0} m and {b:.0} m from it all"
    );
}

/// The ground along `a` to `b`: nowhere steeper than `max` degrees,
/// stepping 0.5 m, skipping stretches over a cave's roof (the ground's cut
/// away under that).
fn steepest(l: &Layout, m: &Map, a: Vec2, b: Vec2) -> (f32, Vec2) {
    let g = |p: Vec2| m.ground_height(p.x, p.y);
    let (dir, len) = ((b - a).normalize(), a.distance(b));
    let mut worst = (0.0, a);
    let mut t = 0.0;
    while t < len - 2.0 {
        let (p, q) = (a + dir * t, a + dir * (t + 2.0));
        if l.cave_distance(p) > CAVE_WALL + 1.0 && l.cave_distance(q) > CAVE_WALL + 1.0 {
            let slope = ((g(q) - g(p)).abs() / 2.0).atan().to_degrees();
            if slope > worst.0 {
                worst = (slope, p);
            }
        }
        t += 0.5;
    }
    worst
}

/// The ways onto high ground (the mesa's haul road, round the Face's west
/// end, each scree, each butte), each ramp down into a wash and each
/// crossing over one, each box cut out of a mine: none steeper anywhere
/// than the heaviest hull climbs. And each way up does go up.
fn check_ways_are_climbable(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let mut ways: Vec<(String, Vec2, Vec2)> = Vec::new();
    for (k, &(a, b)) in l.ways_up.iter().enumerate() {
        ways.push((format!("way up {k}"), a, b));
    }
    for (k, &(a, b, _)) in l.ramps.iter().enumerate() {
        ways.push((format!("ramp {k}"), a, b));
    }
    for (k, &(a, b, _)) in l.fords.iter().enumerate() {
        ways.push((format!("crossing {k}"), a, b));
    }
    for (k, mn) in l.mines.iter().enumerate() {
        let z = mid(mn.box_cut).y;
        ways.push((
            format!("box cut {k}"),
            Vec2::new(mn.box_cut.0.x - 4.0, z),
            Vec2::new(mn.box_cut.1.x + 6.0, z),
        ));
    }
    for (name, a, b) in ways {
        let (slope, at) = steepest(&l, &m, a, b);
        assert!(
            slope < CLIMB,
            "seed {seed}: {name} is {slope:.0} degrees at {at}"
        );
    }
    let g = |p: Vec2| m.ground_height(p.x, p.y);
    for (k, &(a, b)) in l.ways_up.iter().enumerate().filter(|&(k, _)| k != 1) {
        let top = (0..=a.distance(b) as usize)
            .map(|i| a + (b - a).normalize() * i as f32)
            .filter(|&p| l.cave_distance(p) > CAVE_WALL + 1.0)
            .map(g)
            .fold(f32::MIN, f32::max);
        assert!(top > 5.5, "seed {seed}: way up {k} tops out at {top}");
    }
}

/// Every way round the bots take is open ground on its level, and the
/// ways keep to their levels: over each deck, under it on the wash floor,
/// through each mine, over each hall's west end on the mesa's top.
fn check_routes_keep_to_their_level(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let nav = NavGrid::build(&m);
    for (k, p) in m
        .routes
        .iter()
        .enumerate()
        .flat_map(|(k, r)| r.iter().map(move |p| (k, p)))
    {
        let f = nav.floor_at(*p);
        assert!(
            f.is_some_and(|f| (f - p.y).abs() < 1.0),
            "seed {seed}: way {k}'s {p} is on {f:?}"
        );
    }
    let mut cases = Vec::new();
    for br in &l.bridges {
        let deck = |t: f32| {
            let p = br.a.lerp(br.b, t);
            Vec3::new(p.x, DECK_TOP, p.y)
        };
        cases.push((deck(0.15), deck(0.85), DECK_TOP));
        let floor = |u: f32| {
            let p = line_at(&l.washes[br.wash].line, u).0;
            Vec3::new(p.x, m.ground_height(p.x, p.y), p.y)
        };
        cases.push((floor(br.s - 12.0), floor(br.s + 12.0), -WASH_DEPTH));
    }
    for mn in &l.mines {
        let wa = mid(mn.west_adit);
        cases.push((
            Vec3::new(wa.x, 0.0, mn.hall.0.y - 10.0),
            Vec3::new(mn.drift.1.x - 8.0, 0.0, mid(mn.drift).y),
            0.0,
        ));
        cases.push((
            Vec3::new(mn.hall.0.x + 2.0, TOP, mn.hall.0.y + 6.0),
            Vec3::new(mn.hall.0.x + 2.0, TOP, mn.hall.1.y - 2.0),
            TOP,
        ));
    }
    for (a, b, y) in cases {
        let r = nav.route(a, b);
        assert!(!r.is_empty(), "seed {seed}: no route from {a} to {b}");
        let end = *r.last().unwrap();
        assert!(
            end.distance(b) < 3.0,
            "seed {seed}: route from {a} to {b} ends at {end}"
        );
        assert!(
            r.iter().all(|p| (p.y - y).abs() < 1.0),
            "seed {seed}: route from {a} to {b} leaves its level"
        );
    }
}

/// Into the warehouse at each of its doors and out of the others: a nav
/// path through it, on its floor.
fn check_the_warehouse_has_ways_through(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    if l.branch.is_empty() {
        return;
    }
    let nav = NavGrid::build(&m);
    let (at, along) = l.shed;
    let across = Vec2::new(along.y, -along.x);
    let w = |u: f32, v: f32| at + across * u + along * v;
    let doors = [
        w(-SHED_HALF.x - 6.0, 0.0),
        w(SHED_HALF.x + 6.0, 0.0),
        w(0.0, -SHED_HALF.y - 6.0),
    ];
    for (i, &a) in doors.iter().enumerate() {
        for &b in &doors[i + 1..] {
            assert!(
                !nav.path(a, b).is_empty(),
                "seed {seed}: no way from {a} to {b} through the warehouse"
            );
        }
    }
    assert!(
        !nav.is_blocked(at),
        "seed {seed}: the warehouse's floor is blocked"
    );
}

/// Nothing climbs a Crag: its face has colliders all along its front and
/// sides, and its top is out of reach of the nav grid.
fn check_the_crags_stand(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let nav = NavGrid::build(&m);
    for mn in &l.mines {
        let p = Vec2::new(mn.crag_w + 6.0, mn.hall.0.y - 6.0);
        assert!(
            m.ground_height(p.x, p.y) > CRAG_TOP - 1.0,
            "seed {seed}: no Crag at {p}"
        );
        let top = open(&nav, mid(mn.bay));
        assert!(
            nav.path(m.bases[0][0], top).is_empty() || l.crag_depth(top) < 0.0,
            "seed {seed}: a way up onto the Crag at {top}"
        );
    }
}

fn check_the_edge_is_rock(seed: u64) {
    let l = lay(seed);
    // Inside a cave's wall the rock is solid up to the roof: no tank
    // gets in there to reach the edge. Nor up the Crag (its face has
    // colliders of its own).
    mine(seed).assert_edge_walled(0.5, |p| {
        let d = l.cave_distance(p);
        (d > 0.0 && d < CAVE_WALL) || l.crag_depth(p) > -1.0
    });
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn the_edge_is_rock() {
    for &seed in SEEDS {
        check_the_edge_is_rock(seed);
    }
}

fn check_repair_spots_are_open_to_the_sky(seed: u64) {
    let m = mine(seed);
    let w = crate::sim::World::empty(m.clone());
    let nav = NavGrid::build(&m);
    for &p in m.repair_spots.iter().chain(m.hold_site.iter()) {
        assert!(!nav.is_blocked(p), "seed {seed}: {p} is blocked");
        let g = m.ground_height(p.x, p.y);
        assert!(
            (land_on(&w, p, 60.0) - g).abs() < 0.3,
            "seed {seed}: something over {p}"
        );
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn repair_spots_are_open_to_the_sky() {
    for &seed in SEEDS {
        check_repair_spots_are_open_to_the_sky(seed);
    }
}

/// Cover at tank scale everywhere: from any open ground (bar what's kept
/// clear on purpose: spawns, track, ramps, ways up) there's something to
/// hide behind within a short dash.
fn check_cover_everywhere(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let nav = NavGrid::build(&m);
    let mut copy = m.clone();
    let d = Dig {
        map: &mut copy,
        l: &l,
        st: &super::ithessar::Ithessar,
    };
    let mut bare = Vec::new();
    let n = (l.size / 5.0) as usize;
    for iz in 0..n {
        for ix in 0..n {
            let p = Vec2::new(ix as f32 * 5.0 + 2.5, iz as f32 * 5.0 + 2.5);
            // (Not where it's kept open on purpose: by the ways round, the
            // ramps and crossings, the spawns.)
            if keep_clear(&l, p) || nav.is_blocked(p) || d.in_the_way(p, 6.0) {
                continue;
            }
            if !d.cover_near(p, 13.0) {
                bare.push(p);
            }
        }
    }
    assert!(bare.is_empty(), "seed {seed}: no cover near {bare:?}");
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn cover_everywhere() {
    for &seed in SEEDS {
        check_cover_everywhere(seed);
    }
}

/// Not mirrored: the halves are built differently. Far fewer solid pieces
/// have a twin across the middle than not. (Not counting the colliders
/// along the Crag's sides: a Crag standing in the middle has its sides
/// either side of it.)
fn check_the_halves_differ(seed: u64) {
    let m = mine(seed);
    let solid: Vec<&OBox> = m
        .oboxes
        .iter()
        .filter(|o| {
            o.solid
                && !o.hidden
                && o.center.min_element() > 6.0
                && o.center.max_element() < m.size - 6.0
        })
        .collect();
    let twins = solid
        .iter()
        .filter(|o| {
            let p = Vec2::new(m.size - o.center.x, o.center.y);
            solid.iter().any(|q| q.center.distance(p) < 1.5)
        })
        .count();
    assert!(
        twins * 5 < solid.len(),
        "seed {seed}: {twins} of {} pieces are mirrored",
        solid.len()
    );
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn the_halves_differ() {
    for &seed in SEEDS {
        check_the_halves_differ(seed);
    }
}

/// The rocks strewn about run big: more over a tank's width than pebbles
/// (#267), and plenty of them (#281).
fn check_rocks_run_big(seed: u64) {
    let m = mine(seed);
    let (mut small, mut big) = (0, 0);
    for o in &m.oboxes {
        let on_map = o.center.min_element() > 0.0 && o.center.max_element() < m.size;
        // Rock on the ground (not ore heaped in a car), short of the
        // edge's wall.
        if o.rock.is_none()
            || o.broken
            || !on_map
            || o.y0 >= m.ground_height(o.center.x, o.center.y)
        {
            continue;
        }
        let s = o.half.max_element();
        if s < 0.7 {
            small += 1;
        } else if (1.8..3.0).contains(&s) {
            big += 1;
        }
    }
    // (Per 220 m square's worth of map.)
    let area = (m.size / 220.0).powi(2);
    assert!(
        (small as f32) < 60.0 * area,
        "seed {seed}: {small} small rocks"
    );
    assert!(big > 75, "seed {seed}: only {big} big rocks");
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn rocks_run_big() {
    for &seed in SEEDS {
        check_rocks_run_big(seed);
    }
}

mod play {
    use super::*;
    use crate::combat::Match;
    use crate::sim::Command;

    /// Two minutes of a 6v6 round in each mine: both sides score. Slow (run
    /// with `--ignored --release`).
    #[test]
    #[ignore]
    fn bots_fight_through_the_mine() {
        for &seed in SEEDS {
            let mut m = Match::new(mine(seed), 0, 11, 7);
            let steps = (120.0 / crate::game::DT) as usize;
            for _ in 0..steps {
                m.step(Command::default(), false);
            }
            let (a, b) = (m.score(0), m.score(1));
            eprintln!("seed {seed}: score {a} : {b}");
            assert!(a > 0 && b > 0, "seed {seed}: score {a} : {b}");
        }
    }

    /// The levels a tank can be on in the mine.
    const LEVELS: [&str; 6] = ["deck", "under bridge", "wash", "mine", "mesa", "flats"];

    /// Which of `LEVELS` a tank at `p` is on.
    fn level(l: &Layout, p: Vec3) -> usize {
        let q = flat(p);
        let under = l.bridges.iter().any(|b| {
            let (d, t) = seg(q, b.a, b.b);
            d < DECK_HALF + 4.0 && t > 0.0 && t < 1.0
        });
        if l.on_bridge(q) && p.y > -2.0 {
            0
        } else if under && p.y < -3.0 {
            1
        } else if l.wash(q) < -3.0 && p.y < -2.0 {
            2
        } else if l.cave_distance(q) == 0.0 && l.mesa(q) > 0.5 && p.y < 4.0 {
            3
        } else if p.y > 6.0 {
            4
        } else {
            5
        }
    }

    /// What a bots-only round did, level by level.
    struct Survey {
        /// Share of bot time on each level, and of the time bots had an
        /// enemy in sight and in reach.
        time: [f32; 6],
        fighting: [f32; 6],
        deaths: [u32; 6],
        /// Steep drops of more than 3 m nobody threw them off.
        falls: u32,
        /// Share of bot time stopped more than 5 s on end.
        stuck: f32,
        /// The most bot time any one 20 m square of the map had.
        crowd: f32,
    }

    /// `secs` of a 6v6 round of bots on map seed `map_seed`, watched.
    fn survey(map_seed: u64, seed: u64, secs: f32) -> Survey {
        let l = lay(map_seed);
        let mut m = Match::new(mine(map_seed), 0, 11, seed);
        m.bots_only(seed);
        let dt = crate::game::DT;
        let n = m.world.tanks.len();
        let (mut time, mut fighting, mut deaths) = ([0.0f32; 6], [0.0f32; 6], [0u32; 6]);
        let (mut falls, mut stuck) = (0, 0.0);
        let cells = (l.size / 20.0).ceil() as usize + 1;
        let mut heat = vec![0.0f32; cells * cells];
        let mut last = vec![Vec3::ZERO; n];
        let mut alive = vec![true; n];
        let mut still = vec![0.0f32; n];
        let mut launched = vec![f32::NEG_INFINITY; n];
        // Each tank's last second of positions.
        let mut trail: Vec<Vec<(f32, Vec3)>> = vec![Vec::new(); n];
        for k in 0..(secs / dt) as usize {
            m.step(Command::default(), false);
            let now = k as f32 * dt;
            for i in 0..n {
                let (t, f) = (&m.world.tanks[i], &m.fighters[i]);
                let p = t.pos;
                if alive[i] && !f.alive {
                    deaths[level(&l, last[i])] += 1;
                }
                alive[i] = f.alive;
                if !f.alive {
                    still[i] = 0.0;
                    trail[i].clear();
                    continue;
                }
                last[i] = p;
                time[level(&l, p)] += dt;
                let c = (p / 20.0).clamp(Vec3::ZERO, Vec3::splat(cells as f32 - 1.0));
                heat[c.z as usize * cells + c.x as usize] += dt;
                // In a fight: an enemy in sight and in reach (looked for
                // every quarter second).
                if k % 30 == 0 {
                    let eye = p + Vec3::Y * t.gun_height();
                    let seen = (0..n).any(|j| {
                        m.fighters[j].alive
                            && m.fighters[j].team != f.team
                            && m.world.tanks[j].pos.distance(p) < 110.0
                            && crate::ai::can_see(&m.world, eye, &m.world.tanks[j])
                    });
                    if seen {
                        fighting[level(&l, p)] += dt * 30.0;
                    }
                }
                still[i] = if t.speed().abs() < 0.5 {
                    still[i] + dt
                } else {
                    0.0
                };
                if still[i] > 5.0 {
                    stuck += dt;
                }
                // Off something: a drop of more than 3 m inside a second,
                // steeper than any slope, driving on, that no hit or blast
                // threw it into.
                if t.vel.y > 3.0 || f.since_hit < 0.5 {
                    launched[i] = now;
                }
                let h = &mut trail[i];
                h.push((now, p));
                h.retain(|&(s, _)| s > now - 1.0);
                if let Some(&(_, q)) = h.iter().max_by(|a, b| a.1.y.total_cmp(&b.1.y)) {
                    let (drop, across) = (q.y - p.y, Vec2::new(q.x - p.x, q.z - p.z).length());
                    if drop > 3.0 && drop > across * 0.8 && across > 1.0 {
                        if f.arrived > 3.0 && !f.is_husk() && launched[i] < now - 2.0 {
                            falls += 1;
                            eprintln!(
                                "t={now:.0} tank {i} fell {drop:.1} m: {q} to {p} ({:?})",
                                t.hull_kind
                            );
                        }
                        h.clear();
                    }
                }
            }
        }
        let total: f32 = time.iter().sum();
        let fights: f32 = fighting.iter().sum::<f32>().max(1e-3);
        Survey {
            time: time.map(|t| t / total),
            fighting: fighting.map(|t| t / fights),
            deaths,
            falls,
            stuck: stuck / total,
            crowd: heat.iter().fold(0.0f32, |a, &b| a.max(b)) / total,
        }
    }

    /// Bots get round the whole map and fight on every level of it, on
    /// each of several mines: over the bridge and under it, along the wash,
    /// in the mine, on the mesa and out on the flats. They don't fall off
    /// things, sit stuck, or all crowd into one spot. Slow (`--ignored
    /// --release --nocapture`).
    #[test]
    #[ignore]
    fn bots_play_every_level() {
        // A mine with a wash; two mines, a wash and a butte; a mine with no
        // wash and two buttes.
        let rounds = [(0, 7), (8, 8), (11, 9)];
        let (mut time, mut fighting) = ([0.0f32; 6], [0.0f32; 6]);
        let mut problems = Vec::new();
        for (map_seed, seed) in rounds {
            let l = lay(map_seed);
            let s = survey(map_seed, seed, 180.0);
            eprintln!("map seed {map_seed}, seed {seed}:");
            for (k, name) in LEVELS.iter().enumerate() {
                eprintln!(
                    "  {name:>12}: {:5.1}% of bot time, {:5.1}% of the fighting, {} deaths",
                    s.time[k] * 100.0,
                    s.fighting[k] * 100.0,
                    s.deaths[k]
                );
                time[k] += s.time[k] / rounds.len() as f32;
                fighting[k] += s.fighting[k] / rounds.len() as f32;
            }
            eprintln!(
                "  {} falls, {:.1}% stuck, {:.1}% in the busiest 20 m square",
                s.falls,
                s.stuck * 100.0,
                s.crowd * 100.0
            );
            // (Only the levels this mine has: no bridge, no deck.)
            let has = [
                !l.bridges.is_empty(),
                !l.bridges.is_empty(),
                !l.washes.is_empty(),
                true,
                true,
                true,
            ];
            for (k, name) in LEVELS.iter().enumerate() {
                if has[k] && s.time[k] <= 0.0005 {
                    problems.push(format!("map seed {map_seed}: nobody went {name}"));
                }
            }
            if s.falls > 6 {
                problems.push(format!("map seed {map_seed}: {} falls", s.falls));
            }
            if s.stuck >= 0.02 {
                problems.push(format!(
                    "map seed {map_seed}: stuck {:.1}%",
                    s.stuck * 100.0
                ));
            }
            if s.crowd >= 0.08 {
                problems.push(format!(
                    "map seed {map_seed}: {:.1}% in one square",
                    s.crowd * 100.0
                ));
            }
        }
        // Over the rounds, a fair share of the fighting off the flats, and
        // time spent in the mines and up on the mesa.
        if fighting[5] >= 0.9 {
            problems.push(format!(
                "{:.0}% of the fighting on the flats",
                fighting[5] * 100.0
            ));
        }
        if time[3] <= 0.02 || time[4] <= 0.03 {
            problems.push(format!("mine and mesa {time:?}"));
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn every_seed_is_playable_dressed() {
    for &seed in SEEDS {
        check_every_seed_is_playable_dressed(seed);
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn levels_stack() {
    for &seed in SEEDS {
        check_levels_stack(seed);
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn every_spawn_reaches_everything_and_the_sides_are_fair() {
    for &seed in SEEDS {
        check_every_spawn_reaches_everything_and_the_sides_are_fair(seed);
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn ways_are_climbable() {
    for &seed in SEEDS {
        check_ways_are_climbable(seed);
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn routes_keep_to_their_level() {
    for &seed in SEEDS {
        check_routes_keep_to_their_level(seed);
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn the_warehouse_has_ways_through() {
    for &seed in SEEDS {
        check_the_warehouse_has_ways_through(seed);
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized: run with cargo test --release"
)]
fn the_crags_stand() {
    for &seed in SEEDS {
        check_the_crags_stand(seed);
    }
}

/// Every check above, over many more seeds than a test run builds: a
/// mission rolls a new mine each sortie, so they all have to hold. Reports
/// every failure. `MINE_SWEEP=from,to cargo test --release sweep --
/// --ignored --nocapture` (default 3..33).
#[test]
#[ignore]
fn sweep() {
    let v: Vec<u64> = std::env::var("MINE_SWEEP")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    let (from, to) = (
        v.first().copied().unwrap_or(3),
        v.get(1).copied().unwrap_or(33),
    );
    type Check = (&'static str, fn(u64));
    let checks: [Check; 15] = [
        (
            "every_seed_is_playable_dressed",
            check_every_seed_is_playable_dressed,
        ),
        ("no_stone_drawn_as_a_box", check_no_stone_drawn_as_a_box),
        (
            "roofs_are_ground_not_slabs",
            check_roofs_are_ground_not_slabs,
        ),
        (
            "the_faces_top_is_never_buried",
            check_the_faces_top_is_never_buried,
        ),
        ("levels_stack", check_levels_stack),
        (
            "every_spawn_reaches_everything_and_the_sides_are_fair",
            check_every_spawn_reaches_everything_and_the_sides_are_fair,
        ),
        ("ways_are_climbable", check_ways_are_climbable),
        (
            "routes_keep_to_their_level",
            check_routes_keep_to_their_level,
        ),
        (
            "the_warehouse_has_ways_through",
            check_the_warehouse_has_ways_through,
        ),
        ("the_crags_stand", check_the_crags_stand),
        ("the_edge_is_rock", check_the_edge_is_rock),
        (
            "repair_spots_are_open_to_the_sky",
            check_repair_spots_are_open_to_the_sky,
        ),
        ("cover_everywhere", check_cover_everywhere),
        ("the_halves_differ", check_the_halves_differ),
        ("rocks_run_big", check_rocks_run_big),
    ];
    let mut failed = Vec::new();
    for seed in from..to {
        for (name, check) in checks {
            if std::panic::catch_unwind(|| check(seed)).is_err() {
                failed.push(format!("seed {seed}: {name}"));
            }
        }
        eprintln!("seed {seed} done, {} failures so far", failed.len());
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// What's round a point (`MINE_PROBE=seed,x,z,r cargo test --release
/// probe -- --ignored --nocapture`): for looking into a failure.
#[test]
#[ignore]
fn probe() {
    let v: Vec<f32> = std::env::var("MINE_PROBE")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    let (seed, p, r) = (v[0] as u64, Vec2::new(v[1], v[2]), v[3]);
    let (l, m) = (lay(seed), mine(seed));
    let mut copy = m.clone();
    let d = Dig {
        map: &mut copy,
        l: &l,
        st: &super::ithessar::Ithessar,
    };
    eprintln!(
        "ground {} wash {} mesa {} keep_clear {} in_the_way {} cover_near {} size {}",
        l.ground(p.x, p.y),
        l.wash(p),
        l.mesa(p),
        keep_clear(&l, p),
        d.in_the_way(p, 3.0),
        d.cover_near(p, 13.0),
        l.size
    );
    for o in m.oboxes.iter().filter(|o| o.solid && o.distance(p) < r) {
        eprintln!(
            "obox {} half {} y {:.1}..{:.1} rock {:?}",
            o.center, o.half, o.y0, o.y1, o.rock
        );
    }
    for k in 0..16 {
        let a = k as f32 * TAU / 16.0;
        let q = p + Vec2::new(a.cos(), a.sin()) * 9.0;
        eprintln!(
            "  {q}: keep_clear {} in_the_way {}",
            keep_clear(&l, q),
            d.in_the_way(q, 2.5)
        );
    }
}

/// Where `MINE_PATH=seed,x,z` can drive to, sampled over the map: a grid
/// of `.` (reachable), `#` (blocked), ` ` (open but cut off).
#[test]
#[ignore]
fn reach_map() {
    let v: Vec<f32> = std::env::var("MINE_PATH")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    let (seed, from) = (v[0] as u64, Vec2::new(v[1], v[2]));
    let (l, m) = (lay(seed), mine(seed));
    let nav = NavGrid::build(&m);
    // A window (x0, z0, step) if given, else the whole map.
    let (x0, z0, step) = if v.len() >= 6 {
        (v[3], v[4], v[5])
    } else {
        (0.0, 0.0, l.size / 60.0)
    };
    let mn = &l.mines[0];
    eprintln!(
        "face at the adit {} adit {:?} west adit {:?} wash at adit {:?}",
        l.face(mn.adit.0.x),
        mn.adit,
        mn.west_adit,
        l.wash_near(Vec2::new(mn.adit.0.x, l.face(mn.adit.0.x) - 20.0))
    );
    for j in (0..60).rev() {
        let row: String = (0..60)
            .map(|i| {
                let p = Vec2::new(x0 + (i as f32 + 0.5) * step, z0 + (j as f32 + 0.5) * step);
                if nav.is_blocked(p) {
                    '#'
                } else if nav.path(from, p).is_empty() {
                    ' '
                } else {
                    '.'
                }
            })
            .collect();
        eprintln!("{row}");
    }
}

/// What the first seeds lay out (`cargo test --release list_mines --
/// --ignored --nocapture`).
#[test]
#[ignore]
fn list_mines() {
    for seed in 0..16u64 {
        let l = lay(seed);
        eprintln!(
            "seed {seed}: {} m, {} mines, {} washes, {} buttes, {} bridges, mirrored {}, turned {}",
            l.size,
            l.mines.len(),
            l.washes.len(),
            l.buttes.len(),
            l.bridges.len(),
            l.mirror,
            l.turns
        );
    }
}
