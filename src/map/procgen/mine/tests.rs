//! The crystal mine, checked over several seeds: what the Seam's tests
//! checked of the one hand-built mine, asked of each generated one.

use std::collections::HashMap;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::sync::{LazyLock, Mutex};

use glam::{Vec2, Vec3};

use super::dig::{FACE_BLOCK, MESA_TOP, keep_clear};
use super::*;
use crate::ai::NavGrid;
use crate::map::OBox;
use crate::render::material::Material;

/// The seeds the tests build: one in a debug run (CI's has two minutes for
/// everything), three optimized. `sweep` runs the checks over many more.
const SEEDS: &[u64] = if cfg!(debug_assertions) {
    &[0]
} else {
    &[0, 1, 2]
};

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

/// Most draws of the grammar hold together; the rest are drawn again.
#[test]
fn draws_mostly_hold_together() {
    let mut ok = 0;
    let mut why = std::collections::BTreeMap::new();
    for k in 0..200u64 {
        let mut rng = Rng::new(k * 7919);
        match Layout::generate(&mut rng) {
            Ok(_) => ok += 1,
            Err(line) => *why.entry(line).or_insert(0) += 1,
        }
    }
    eprintln!("{ok} of 200 draws hold together; the rest failed at lines {why:?}");
    assert!(ok > 60, "{ok} of 200");
}

#[test]
fn different_seeds_make_different_mines() {
    let ls: Vec<Layout> = (0..6).map(lay).collect();
    for (i, a) in ls.iter().enumerate() {
        for b in &ls[i + 1..] {
            let same = a.adit == b.adit && a.wash == b.wash && a.track == b.track;
            assert!(!same, "two seeds laid out the same mine");
        }
    }
    // And the same seed, the same mine.
    let again = lay(1);
    assert_eq!(again.wash, ls[1].wash);
    assert_eq!(again.caves, ls[1].caves);
    assert_eq!(again.track, ls[1].track);
}

/// Each seed's dressed map is playable: every side has its spawns, reaches
/// the Hold site and the other side; and has the rest of what a map needs.
fn check_every_seed_is_playable_dressed(seed: u64) {
    let m = mine(seed);
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
    assert_eq!(m.beams.len(), 1, "seed {seed}");
    assert!(m.routes.len() >= 6, "seed {seed}");
}

#[test]
fn every_seed_is_playable_dressed() {
    for &seed in SEEDS {
        check_every_seed_is_playable_dressed(seed);
    }
}

/// Half the mines are built the other way round: each such one is the
/// mirror image of its layout's mine, piece for piece, and as playable.
#[test]
fn mirrored_mines_are_mirror_images() {
    let seed = (0..20)
        .find(|&s| lay(s).mirror)
        .expect("no mirrored mine in 20 seeds");
    let (l, m, flipped) = (lay(seed), mine(seed), Map::mine(seed));
    let s = l.size;
    let back = |p: Vec2| Vec2::new(s - p.x, p.y);
    assert_eq!(m.oboxes.len(), flipped.oboxes.len());
    for (a, b) in m.oboxes.iter().zip(&flipped.oboxes) {
        assert!(back(a.center).distance(b.center) < 1e-3);
        assert!((a.yaw + b.yaw).abs() < 1e-5 && a.tilt == b.tilt && (a.roll + b.roll).abs() < 1e-5);
    }
    for (a, b) in m.bases.iter().flatten().zip(flipped.bases.iter().flatten()) {
        assert!(back(*a).distance(*b) < 1e-3);
    }
    let g = |m: &Map, p: Vec2| m.ground_height(p.x, p.y);
    for p in [
        Vec2::new(30.3, 40.7),
        mid(l.hall),
        l.hold,
        line_at(&l.wash, l.bridge_s).0,
    ] {
        assert!(
            (g(&m, p) - g(&flipped, back(p))).abs() < 1e-3,
            "ground at {p}"
        );
    }
    assert!(m.atmosphere.sun_dir.x == -flipped.atmosphere.sun_dir.x);
    // And it plays: every side gets everywhere, and drives where it should.
    if let Err(why) = crate::map::procgen::check::playable(&flipped) {
        panic!("seed {seed} turned over: {why}");
    }
    let nav = NavGrid::build(&flipped);
    for p in flipped.routes.iter().flatten() {
        let f = nav.floor_at(*p);
        assert!(f.is_some_and(|f| (f - p.y).abs() < 1.0), "{p} is on {f:?}");
    }
    let (a, b) = l.bridge();
    let (a, b) = (back(a), back(b));
    let dir = (b - a).normalize();
    let (end, low, _) = drive(&flipped, HULLS[0], a - dir * 5.0, yaw_along(dir), 12.0);
    assert!(
        low > -1.5 && seg(flat(end), a, b).1 > 0.9,
        "off the bridge at {end}"
    );
}

/// No block of stone on the map is drawn as a box (#249): each is a rock
/// from the rocks toolkit (#268), or stands in for a sculpted face.
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
    let (wa, br) = (mid(l.west_adit), mid(l.breach));
    for p in [
        Vec2::new(l.hall.0.x + 4.0, mid(l.hall).y),
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

#[test]
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
                || l.crag(q) > 0.5
                || q.max_element() > l.size
                || q.min_element() < 0.0
            {
                continue;
            }
            let under = map.ground_height(p.x, p.z) - p.y;
            if under > 0.05 {
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
fn the_faces_top_is_never_buried() {
    for &seed in SEEDS {
        check_the_faces_top_is_never_buried(seed);
    }
}

/// How far a point is out of the stone drawn for `o` (< 0: in it). A rock
/// is measured to the hull of its surface (what it collides as), a block or
/// plain box to its box.
fn stone_distance(o: &OBox) -> impl Fn(Vec3) -> f32 + '_ {
    use rapier3d::parry::query::PointQuery;
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
#[test]
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

/// The levels stack: the deck over the wash's floor, the mesa's top (or
/// the Crag's) over the mine's floor.
fn check_levels_stack(seed: u64) {
    let l = lay(seed);
    let w = crate::sim::World::empty(mine(seed));
    let (a, b) = l.bridge();
    let p = line_at(&l.wash, l.bridge_s).0;
    let p = a + (b - a).normalize() * (p - a).dot((b - a).normalize());
    assert!(
        (land_on(&w, p, 30.0) - DECK_TOP).abs() < 0.1,
        "seed {seed}: deck at {}",
        land_on(&w, p, 30.0)
    );
    assert!(
        land_on(&w, p, -1.0) < -WASH_DEPTH + 1.0,
        "seed {seed}: wash floor at {}",
        land_on(&w, p, -1.0)
    );
    // Over the adit, the hall and the drift: the mesa's top (or the
    // Crag's), and the mine's floor. (Off the ground's 2 m grid: a ray
    // straight down a grid line can slip between its cells.)
    let off = Vec2::splat(0.3);
    for p in [
        Vec2::new(mid(l.adit).x, l.bay.0.y + 15.0),
        Vec2::new(l.hall.0.x + 2.0, l.hall.0.y + 11.0),
        Vec2::new(l.hall.1.x - 8.0, l.hall.0.y + 14.0),
        mid(l.drift),
    ]
    .map(|p| p + off)
    {
        let roof = if l.under_crag(p) { CRAG_ROOF } else { ROOF.1 };
        assert!(
            (land_on(&w, p, 60.0) - roof).abs() < 0.1,
            "seed {seed}: roof over {p} at {}",
            land_on(&w, p, 60.0)
        );
        assert!(
            land_on(&w, p, ROOF.0 - 0.5).abs() < 0.6,
            "seed {seed}: cave floor under {p} at {}",
            land_on(&w, p, ROOF.0 - 0.5)
        );
    }
    // The Crag over the bay.
    let top = land_on(&w, mid(l.bay), 60.0);
    assert!(top > CRAG_TOP - 1.0, "seed {seed}: the Crag's top at {top}");
}

#[test]
fn levels_stack() {
    for &seed in SEEDS {
        check_levels_stack(seed);
    }
}

/// From each side's spawns, every part of the map is a nav path away, and
/// neither side is much further than the other from what's worth taking.
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
    let (a, b) = l.bridge();
    let dir = (b - a).normalize();
    let s = l.size;
    let goals = [
        ("the bridge's far end", b + dir * 6.0),
        (
            "the wash under the bridge",
            line_at(&l.wash, l.bridge_s - 9.0).0,
        ),
        (
            "the flats",
            l.track_at(l.track_len(5)).0 + dir.perp() * 14.0,
        ),
        (
            "the landing",
            Vec2::new(a.x - 16.0, l.face(a.x - 16.0) - 8.0),
        ),
        (
            "the east landing",
            Vec2::new(mid(l.breach).x + 7.0, l.face(mid(l.breach).x + 7.0) - 14.0),
        ),
        ("the mesa's west", Vec2::new(l.west_ramp.1 - 4.0, s - 30.0)),
        ("the mesa's east", Vec2::new(l.box_cut.1.x + 26.0, s - 24.0)),
        ("the mine", Vec2::new(l.hall.0.x + 2.0, l.hall.0.y + 14.0)),
    ]
    .map(|(name, p)| (name, open(&nav, p)));
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
        for (name, g) in goals {
            assert!(
                !nav.path(p, g).is_empty(),
                "seed {seed}: no path from {p} to {name}"
            );
        }
    }
    // Fair: from the middle of each side's spawns.
    let middle =
        |side: usize| m.bases[side].iter().copied().sum::<Vec2>() / m.bases[side].len() as f32;
    let total = |side: usize| {
        goals
            .iter()
            .map(|&(_, g)| length(&nav.path(middle(side), g), middle(side)))
            .sum::<f32>()
    };
    // Not held close: the Crag splits the mesa. Neither side is shut
    // out of anything, though.
    let (a, b) = (total(0), total(1));
    eprintln!("seed {seed}: {a:.0} m from the west, {b:.0} m from the east");
    assert!(
        (a - b).abs() / a.max(b) < 0.3,
        "seed {seed}: sides are {a:.0} m and {b:.0} m from it all"
    );
}

#[test]
fn every_spawn_reaches_everything_and_the_sides_are_fair() {
    for &seed in SEEDS {
        check_every_spawn_reaches_everything_and_the_sides_are_fair(seed);
    }
}

/// Drive `hull` on `map` from `at` heading `yaw` at full throttle for
/// `secs`: where it ends up, and the lowest and highest it went.
fn drive(map: &Map, hull: crate::sim::HullKind, at: Vec2, yaw: f32, secs: f32) -> (Vec3, f32, f32) {
    let mut w = crate::sim::World::empty(map.clone());
    w.spawn_tank(Vec3::ONE, at, yaw, hull, crate::sim::TurretKind::Kontos);
    let cmd = crate::sim::Command {
        throttle: 1.0,
        ..Default::default()
    };
    let (mut low, mut high) = (f32::INFINITY, f32::NEG_INFINITY);
    for _ in 0..(secs / crate::game::DT) as usize {
        w.step(&[cmd]);
        low = low.min(w.tanks[0].pos.y);
        high = high.max(w.tanks[0].pos.y);
    }
    (w.tanks[0].pos, low, high)
}

const HULLS: [crate::sim::HullKind; 4] = [
    crate::sim::HullKind::Kataphrakt,
    crate::sim::HullKind::Sethuun,
    crate::sim::HullKind::Vigil,
    crate::sim::HullKind::Gravis,
];

/// Every hull drives out of the adit, onto the bridge and across the wash
/// on its deck.
fn check_tanks_cross_the_bridge(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let (a, b) = l.bridge();
    let dir = (b - a).normalize();
    for hull in HULLS {
        let mut w = crate::sim::World::empty(m.clone());
        w.spawn_tank(
            Vec3::ONE,
            a - dir * 5.0,
            yaw_along(dir),
            hull,
            crate::sim::TurretKind::Kontos,
        );
        let cmd = crate::sim::Command {
            throttle: 1.0,
            ..Default::default()
        };
        let mut low = f32::INFINITY;
        for _ in 0..(16.0 / crate::game::DT) as usize {
            w.step(&[cmd]);
            let p = w.tanks[0].pos;
            low = low.min(p.y);
            if seg(flat(p), a, b).1 > 0.99 {
                break;
            }
        }
        let end = w.tanks[0].pos;
        assert!(
            low > -1.5,
            "seed {seed}: {hull:?} fell into the wash ({low})"
        );
        assert!(
            seg(flat(end), a, b).1 > 0.99,
            "seed {seed}: {hull:?} only got to {end}"
        );
    }
}

#[test]
fn tanks_cross_the_bridge() {
    for &seed in SEEDS {
        check_tanks_cross_the_bridge(seed);
    }
}

/// Every hull drives in at the west adit's mouth, up it into the hall.
fn check_tanks_drive_into_the_mine(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let x = mid(l.west_adit).x + 1.0;
    for hull in HULLS {
        let (end, _, _) = drive(&m, hull, Vec2::new(x, l.west_adit.0.y + 2.0), 0.0, 6.0);
        assert!(
            end.z > l.hall.0.y - 5.0,
            "seed {seed}: {hull:?} only got to {end}"
        );
        assert!(end.y < 2.0, "seed {seed}: {hull:?} is up at {end}");
    }
}

#[test]
fn tanks_drive_into_the_mine() {
    for &seed in SEEDS {
        check_tanks_drive_into_the_mine(seed);
    }
}

/// Every hull drives in at the new heading's mouth, up it into the bay (and
/// on into the borer's side: no climbing it either).
fn check_tanks_drive_into_the_heading(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let x = l.heading.0.x + 3.0;
    for hull in HULLS {
        let (end, _, high) = drive(&m, hull, Vec2::new(x, l.face(x) - 4.0), 0.0, 6.0);
        assert!(
            end.z > l.bay.0.y + 2.0,
            "seed {seed}: {hull:?} only got to {end}"
        );
        assert!(high < 3.0, "seed {seed}: {hull:?} went up to {high}");
    }
}

#[test]
fn tanks_drive_into_the_heading() {
    for &seed in SEEDS {
        check_tanks_drive_into_the_heading(seed);
    }
}

/// The Crag stands tall over the mine, and nothing drives up it: a tank
/// driven at its face from the landing stops at its foot.
fn check_the_crag_stops_tanks(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let rock = Vec2::new(l.crag_w + 6.0, l.hall.0.y - 6.0);
    assert!(
        m.ground_height(rock.x, rock.y) > CRAG_TOP - 1.0,
        "seed {seed}"
    );
    // Either side of the adit (clear of the bridge out of it) and of the
    // heading, from the landing under the Face.
    let xs = [
        l.crag_w + 4.0,
        l.adit.0.x - 4.0,
        l.heading.0.x - 3.0,
        l.crag_e - 3.0,
    ];
    for x in xs {
        for hull in HULLS {
            let (end, _, high) = drive(&m, hull, Vec2::new(x, l.face(x) - 5.0), 0.0, 6.0);
            // It may slide along the foot into a mouth, but not into the
            // rock.
            let at = flat(end);
            assert!(
                end.z < l.face(end.x) + 0.5 || l.in_cave(at),
                "seed {seed}: {hull:?} got in to {end} at {x}"
            );
            // (Rearing against the rock where the foot runs slantwise
            // lifts it a little, no more.)
            assert!(high < 5.0, "seed {seed}: {hull:?} climbed to {high} at {x}");
        }
    }
    // Nor up its sides from the mesa (over a rock or two on the way,
    // no more).
    for (p, yaw) in [
        (Vec2::new(l.hall.0.x - 6.0, mid(l.hall).y), FRAC_PI_2),
        (Vec2::new(l.crag_e + 22.0, l.size - 30.0), -FRAC_PI_2),
    ] {
        let (end, _, high) = drive(&m, HULLS[0], p, yaw, 8.0);
        assert!(
            high < TOP + 6.0,
            "seed {seed}: climbed to {high} from {p} ({end})"
        );
        assert!(
            l.crag_depth(flat(end)) < 0.5,
            "seed {seed}: got into the Crag at {end}"
        );
    }
}

#[test]
fn the_crag_stops_tanks() {
    for &seed in SEEDS {
        check_the_crag_stops_tanks(seed);
    }
}

/// Every hull climbs the box cut out of the mine onto the mesa.
fn check_tanks_climb_the_box_cut(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let at = Vec2::new(l.adit.0.x + 8.0, mid(l.box_cut).y);
    for hull in HULLS {
        let (end, _, high) = drive(&m, hull, at, FRAC_PI_2, 10.0);
        assert!(
            end.x > l.box_cut.1.x - 4.0,
            "seed {seed}: {hull:?} only got to {end}"
        );
        assert!(
            high > TOP - 0.5,
            "seed {seed}: {hull:?} only got {high} m up"
        );
    }
}

#[test]
fn tanks_climb_the_box_cut() {
    for &seed in SEEDS {
        check_tanks_climb_the_box_cut(seed);
    }
}

/// The ways up onto the mesa (its west end, the scree) climb from the
/// flat to its top, and nowhere along them is steeper than the heaviest
/// hull climbs (the Gravis tops out a little over 20 degrees).
fn check_tanks_climb_onto_the_mesa(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let g = |p: Vec2| m.ground_height(p.x, p.y);
    for (name, (a, b)) in [("west end", l.ways_up[0]), ("scree", l.ways_up[1])] {
        assert!(
            g(a) < 1.5,
            "seed {seed}: the {name}'s way starts {} up",
            g(a)
        );
        let dir = (b - a).normalize();
        let len = a.distance(b);
        // (Not where it runs on over a cave's roof: the ground's cut away
        // under that.)
        let over_cave = |p: Vec2| l.cave_distance(p) < CAVE_WALL + 1.0;
        let top = (0..=len as usize)
            .map(|k| a + dir * k as f32)
            .filter(|&p| !over_cave(p))
            .map(g)
            .fold(f32::MIN, f32::max);
        assert!(
            top > TOP - 0.5,
            "seed {seed}: the {name}'s way tops out at {top}"
        );
        let mut t = 0.0;
        while t < len - 2.0 {
            let (p, q) = (a + dir * t, a + dir * (t + 2.0));
            if over_cave(p) || over_cave(q) {
                t += 0.5;
                continue;
            }
            let slope = ((g(q) - g(p)) / 2.0).atan().to_degrees();
            assert!(
                slope < 20.0,
                "seed {seed}: the {name} is {slope:.0} degrees at {p}"
            );
            t += 0.5;
        }
    }
}

#[test]
fn tanks_climb_onto_the_mesa() {
    for &seed in SEEDS {
        check_tanks_climb_onto_the_mesa(seed);
    }
}

/// Every hull drives down each haul ramp into the wash, and back up.
fn check_tanks_use_the_wash_ramps(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    for hull in HULLS {
        for (s, e, _) in l.ramps {
            let dir = (e - s).normalize();
            let (end, low, _) = drive(&m, hull, s - dir * 4.0, yaw_along(dir), 6.0);
            assert!(
                low < -WASH_DEPTH + 1.5,
                "seed {seed}: {hull:?} never got down ramp {s} ({end})"
            );
            let (end, _, high) = drive(&m, hull, e + dir * 1.0, yaw_along(-dir), 6.0);
            assert!(
                high > -0.5,
                "seed {seed}: {hull:?} never got up ramp {s} ({end})"
            );
        }
    }
}

#[test]
fn tanks_use_the_wash_ramps() {
    for &seed in SEEDS {
        check_tanks_use_the_wash_ramps(seed);
    }
}

/// Every hull drives across the wash at each crossing, down one bank and
/// up the other, both ways.
fn check_tanks_cross_the_wash_at_the_fords(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    for hull in HULLS {
        for (s, e, _) in l.fords {
            for (a, b) in [(s, e), (e, s)] {
                let dir = (b - a).normalize();
                // Until it's across (then it's done: what's strewn on
                // past the far top isn't the crossing).
                let mut w = crate::sim::World::empty(m.clone());
                w.spawn_tank(
                    Vec3::ONE,
                    a - dir * 4.0,
                    yaw_along(dir),
                    hull,
                    crate::sim::TurretKind::Kontos,
                );
                let cmd = crate::sim::Command {
                    throttle: 1.0,
                    ..Default::default()
                };
                let mut low = f32::INFINITY;
                for _ in 0..(12.0 / crate::game::DT) as usize {
                    w.step(&[cmd]);
                    let p = w.tanks[0].pos;
                    low = low.min(p.y);
                    if (flat(p) - b).dot(dir) > 2.0 {
                        break;
                    }
                }
                let end = w.tanks[0].pos;
                assert!(
                    low < -WASH_DEPTH + 1.5,
                    "seed {seed}: {hull:?} never got down into the wash from {a} ({end})"
                );
                let past = (flat(end) - b).dot(dir);
                assert!(
                    past > -2.0 && end.y > -0.8,
                    "seed {seed}: {hull:?} crossing from {a} only got to {end}"
                );
            }
        }
    }
}

#[test]
fn tanks_cross_the_wash_at_the_fords() {
    for &seed in SEEDS {
        check_tanks_cross_the_wash_at_the_fords(seed);
    }
}

/// Routes keep to the level they're on: along the deck over the wash,
/// along the wash's floor under it, through the mine, over the mesa's top
/// above it.
fn check_routes_keep_to_their_level(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let nav = NavGrid::build(&m);
    let (a, b) = l.bridge();
    let deck = |t: f32| {
        let p = a.lerp(b, t);
        Vec3::new(p.x, DECK_TOP, p.y)
    };
    let floor = |u: f32| {
        let p = line_at(&l.wash, u).0;
        Vec3::new(p.x, m.ground_height(p.x, p.y), p.y)
    };
    let wa = mid(l.west_adit);
    let cases = [
        (deck(0.15), deck(0.85), DECK_TOP),
        // Under the deck, from one side of it to the other.
        (
            floor(l.bridge_s - 12.0),
            floor(l.bridge_s + 12.0),
            -WASH_DEPTH,
        ),
        // From the west adit, through the hall and along the drift.
        (
            Vec3::new(wa.x, 0.0, l.hall.0.y - 10.0),
            Vec3::new(l.drift.1.x - 8.0, 0.0, mid(l.drift).y),
            0.0,
        ),
        // Over the hall's west end, on the roof.
        (
            Vec3::new(l.hall.0.x + 2.0, TOP, l.hall.0.y + 6.0),
            Vec3::new(l.hall.0.x + 2.0, TOP, l.hall.1.y - 2.0),
            TOP,
        ),
    ];
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
            "seed {seed}: route from {a} to {b} leaves its level: {r:?}"
        );
    }
}

#[test]
fn routes_keep_to_their_level() {
    for &seed in SEEDS {
        check_routes_keep_to_their_level(seed);
    }
}

/// Every way round the map the bots take is open ground on its level.
fn check_the_ways_round_are_open(seed: u64) {
    let m = mine(seed);
    let nav = NavGrid::build(&m);
    assert!(m.routes.len() >= 6);
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
}

#[test]
fn the_ways_round_are_open() {
    for &seed in SEEDS {
        check_the_ways_round_are_open(seed);
    }
}

/// Bot `Hymn` (it drives right up to what it's after) from `at` after a
/// parked tank at `goal`, for up to `secs`: how near it got on the
/// target's level.
fn chase(m: &Map, at: Vec2, yaw: f32, goal: Vec2, secs: f32) -> f32 {
    use crate::ai::{Bot, Contact};
    let nav = NavGrid::build(m);
    let mut w = crate::sim::World::empty(m.clone());
    let hull = crate::sim::HullKind::Kataphrakt;
    w.spawn_tank(Vec3::ONE, at, yaw, hull, crate::sim::TurretKind::Hymn);
    w.spawn_tank(Vec3::ONE, goal, 0.0, hull, crate::sim::TurretKind::Kontos);
    let mut bot = Bot::new(5);
    let contacts = [0, 1].map(|team| Contact {
        targetable: true,
        team,
        health: 1.0,
    });
    let mut nearest = f32::INFINITY;
    for _ in 0..(secs / crate::game::DT) as usize {
        let (cmd, _) = bot.think(0, &w, &contacts, None, &[], &nav, crate::game::DT);
        w.step(&[cmd, crate::sim::Command::default()]);
        let (p, q) = (w.tanks[0].pos, w.tanks[1].pos);
        if (p.y - q.y).abs() < 2.0 {
            nearest = nearest.min(p.distance(q));
        }
    }
    nearest
}

/// After a tank on the wash's floor under the bridge, a bot up on the
/// flats by the bridge's end doesn't drive out over it on the deck: it goes
/// down a haul ramp and in under.
fn check_a_bot_gets_under_the_bridge(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let (a, b) = l.bridge();
    let dir = (b - a).normalize();
    let nav = NavGrid::build(&m);
    let from = open(&nav, b + dir * 18.0);
    let goal = line_at(&l.wash, l.bridge_s - 1.0).0;
    let d = chase(&m, from, yaw_along(-dir), goal, 40.0);
    assert!(d < 12.0, "seed {seed}: got no nearer than {d:.1} m");
}

#[test]
fn a_bot_gets_under_the_bridge() {
    for &seed in SEEDS {
        check_a_bot_gets_under_the_bridge(seed);
    }
}

/// After a tank in the pillared hall, a bot on the mesa's top over it finds
/// the way down and in.
fn check_a_bot_gets_into_the_mine_under_it(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    let from = Vec2::new(l.hall.0.x + 2.0, l.hall.1.y - 8.0);
    let goal = Vec2::new(l.hall.0.x + 16.0, l.hall.0.y + 6.0);
    let d = chase(&m, from, FRAC_PI_2, goal, 60.0);
    assert!(d < 12.0, "seed {seed}: got no nearer than {d:.1} m");
}

#[test]
fn a_bot_gets_into_the_mine_under_it() {
    for &seed in SEEDS {
        check_a_bot_gets_into_the_mine_under_it(seed);
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
fn repair_spots_are_open_to_the_sky() {
    for &seed in SEEDS {
        check_repair_spots_are_open_to_the_sky(seed);
    }
}

/// Where a tank `local` in the warehouse's frame (across, along) is in the
/// world, and the yaw that points it `dir` in that frame.
fn in_shed(l: &Layout, local: Vec2, dir: Vec2) -> (Vec2, f32) {
    let (at, along) = l.shed;
    let across = Vec2::new(along.y, -along.x);
    let w = |v: Vec2| across * v.x + along * v.y;
    (at + w(local), yaw_along(w(dir)))
}

/// Every hull drives in at one side door, across between the bins (not up
/// onto them) and out of the other.
fn check_tanks_drive_through_the_warehouse(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    for hull in HULLS {
        let (at, yaw) = in_shed(&l, Vec2::new(-SHED_HALF.x - 6.0, -1.0), Vec2::X);
        let mut w = crate::sim::World::empty(m.clone());
        w.spawn_tank(Vec3::ONE, at, yaw, hull, crate::sim::TurretKind::Kontos);
        let cmd = crate::sim::Command {
            throttle: 1.0,
            ..Default::default()
        };
        // Until it's out of the far door.
        let mut high = f32::NEG_INFINITY;
        let mut p = l.shed_local(at);
        for _ in 0..(8.0 / crate::game::DT) as usize {
            w.step(&[cmd]);
            let t = w.tanks[0].pos;
            p = l.shed_local(flat(t));
            high = high.max(t.y);
            if p.x > SHED_HALF.x + 1.0 {
                break;
            }
        }
        assert!(
            p.x > SHED_HALF.x + 1.0,
            "seed {seed}: {hull:?} only got to {p} (local)"
        );
        assert!(
            high < 2.0 + m.ground_height(at.x, at.y),
            "seed {seed}: {hull:?} went up to {high} inside"
        );
    }
}

#[test]
fn tanks_drive_through_the_warehouse() {
    for &seed in SEEDS {
        check_tanks_drive_through_the_warehouse(seed);
    }
}

/// Every hull drives in at the rail door, along the line past the middle.
fn check_tanks_drive_in_at_the_rail_door(seed: u64) {
    let (l, m) = (lay(seed), mine(seed));
    for hull in HULLS {
        let (at, yaw) = in_shed(&l, Vec2::new(0.0, -SHED_HALF.y - 8.0), Vec2::Y);
        let (end, _, _) = drive(&m, hull, at, yaw, 6.0);
        let p = l.shed_local(flat(end));
        assert!(p.y > -2.0, "seed {seed}: {hull:?} only got to {p} (local)");
    }
}

#[test]
fn tanks_drive_in_at_the_rail_door() {
    for &seed in SEEDS {
        check_tanks_drive_in_at_the_rail_door(seed);
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
            if keep_clear(&l, p) || nav.is_blocked(p) || d.in_the_way(p, 3.0) {
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
    assert!(small < 60, "seed {seed}: {small} small rocks");
    assert!(big > 75, "seed {seed}: only {big} big rocks");
}

#[test]
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
        let (a, b) = l.bridge();
        let (d, t) = seg(q, a, b);
        if l.on_bridge(q) && p.y > -2.0 {
            0
        } else if d < DECK_HALF + 4.0 && t > 0.0 && t < 1.0 && p.y < -3.0 {
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
        let rounds = [(0, 7), (1, 8), (2, 9)];
        let (mut time, mut fighting) = ([0.0f32; 6], [0.0f32; 6]);
        for (map_seed, seed) in rounds {
            let s = survey(map_seed, seed, 180.0);
            eprintln!("map seed {map_seed}, seed {seed}:");
            for (l, name) in LEVELS.iter().enumerate() {
                eprintln!(
                    "  {name:>12}: {:5.1}% of bot time, {:5.1}% of the fighting, {} deaths",
                    s.time[l] * 100.0,
                    s.fighting[l] * 100.0,
                    s.deaths[l]
                );
                time[l] += s.time[l] / rounds.len() as f32;
                fighting[l] += s.fighting[l] / rounds.len() as f32;
            }
            eprintln!(
                "  {} falls, {:.1}% stuck, {:.1}% in the busiest 20 m square",
                s.falls,
                s.stuck * 100.0,
                s.crowd * 100.0
            );
            for (l, name) in LEVELS.iter().enumerate() {
                assert!(
                    s.time[l] > 0.0005,
                    "map seed {map_seed}: nobody went {name}"
                );
            }
            assert!(s.falls <= 6, "map seed {map_seed}: {} falls", s.falls);
            assert!(
                s.stuck < 0.02,
                "map seed {map_seed}: stuck {:.1}%",
                s.stuck * 100.0
            );
            assert!(
                s.crowd < 0.08,
                "map seed {map_seed}: {:.1}% in one square",
                s.crowd * 100.0
            );
        }
        // Over the rounds, some of the fighting is on every level, and a
        // fair share of it off the flats.
        for (l, name) in LEVELS.iter().enumerate() {
            assert!(fighting[l] > 0.002, "nobody fought {name}");
        }
        assert!(
            fighting[5] < 0.9,
            "{:.0}% of the fighting on the flats",
            fighting[5] * 100.0
        );
        assert!(time[3] > 0.02 && time[4] > 0.03, "mine and mesa {time:?}");
    }
}

/// What's round a point: `MINE_PROBE=seed,x,z,r cargo test --release probe
/// -- --ignored --nocapture`. For looking into a failure.
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
    eprintln!(
        "ground {} uncut {} wash {} mesa {} crag_depth {} cave {} keep_clear {}",
        l.ground(p.x, p.y),
        l.uncut(p),
        l.wash(p),
        l.mesa(p),
        l.crag_depth(p),
        l.cave_distance(p),
        keep_clear(&l, p)
    );
    eprintln!(
        "face {} crag_w {} crag_e {} adit {:?} hall {:?} box {:?} west_ramp {:?} ways {:?}",
        l.face(p.x),
        l.crag_w,
        l.crag_e,
        l.adit,
        l.hall,
        l.box_cut,
        l.west_ramp,
        l.ways_up
    );
    eprintln!(
        "fords {:?} ramps {:?} spawns {:?} yard {} camp {}",
        l.fords, l.ramps, l.spawns, l.yard, l.camp
    );
    for o in m.oboxes.iter().filter(|o| o.distance(p) < r) {
        eprintln!(
            "obox {} yaw {:.2} half {} y {:.1}..{:.1} solid {} hidden {} rock {:?} ramp {} color {}",
            o.center, o.yaw, o.half, o.y0, o.y1, o.solid, o.hidden, o.rock, o.ramp, o.color
        );
    }
    for c in m.crystals.iter().filter(|c| c.distance(p) < r) {
        eprintln!("crystal {} solid {} r {}", c.base, c.solid, c.radius);
    }
    let nav = NavGrid::build(&m);
    eprintln!(
        "nav blocked {} floor {:?}",
        nav.is_blocked(p),
        nav.floor_at(Vec3::new(p.x, m.ground_height(p.x, p.y), p.y))
    );
    if v.len() >= 6 {
        let q = Vec2::new(v[4], v[5]);
        let path = nav.path(p, q);
        eprintln!("path to {q}: {} points: {path:?}", path.len());
    }
}

/// A drive, traced: `MINE_DRIVE=seed,hull,x,z,yaw,secs`.
#[test]
#[ignore]
fn trace_drive() {
    let v: Vec<f32> = std::env::var("MINE_DRIVE")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    let m = mine(v[0] as u64);
    let mut w = crate::sim::World::empty(m);
    w.spawn_tank(
        Vec3::ONE,
        Vec2::new(v[2], v[3]),
        v[4],
        HULLS[v[1] as usize],
        crate::sim::TurretKind::Kontos,
    );
    let cmd = crate::sim::Command {
        throttle: 1.0,
        ..Default::default()
    };
    for k in 0..(v[5] / crate::game::DT) as usize {
        w.step(&[cmd]);
        if k % 30 == 0 {
            eprintln!("t {:.1} at {}", k as f32 * crate::game::DT, w.tanks[0].pos);
        }
    }
}

/// A chase, traced: `MINE_CHASE=seed,x,z,yaw,gx,gz,secs`.
#[test]
#[ignore]
fn trace_chase() {
    use crate::ai::{Bot, Contact};
    let v: Vec<f32> = std::env::var("MINE_CHASE")
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.parse().ok())
        .collect();
    let m = mine(v[0] as u64);
    let nav = NavGrid::build(&m);
    let mut w = crate::sim::World::empty(m);
    let hull = crate::sim::HullKind::Kataphrakt;
    w.spawn_tank(
        Vec3::ONE,
        Vec2::new(v[1], v[2]),
        v[3],
        hull,
        crate::sim::TurretKind::Hymn,
    );
    w.spawn_tank(
        Vec3::ONE,
        Vec2::new(v[4], v[5]),
        0.0,
        hull,
        crate::sim::TurretKind::Kontos,
    );
    let mut bot = Bot::new(5);
    let contacts = [0, 1].map(|team| Contact {
        targetable: true,
        team,
        health: 1.0,
    });
    for k in 0..(v[6] / crate::game::DT) as usize {
        let (cmd, _) = bot.think(0, &w, &contacts, None, &[], &nav, crate::game::DT);
        w.step(&[cmd, crate::sim::Command::default()]);
        if k % 120 == 0 {
            eprintln!(
                "t {:.0} bot {} target {}",
                k as f32 * crate::game::DT,
                w.tanks[0].pos,
                w.tanks[1].pos
            );
        }
    }
}

/// Every check above, over many more seeds than a test run builds: a
/// mission rolls a new mine each sortie, so they all have to hold. Reports
/// every failure. `MINE_SWEEP=from,to cargo test --release sweep --
/// --ignored --nocapture` (default 3..23).
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
        v.get(1).copied().unwrap_or(23),
    );
    let checks: [(&str, fn(u64)); 25] = [
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
        ("tanks_cross_the_bridge", check_tanks_cross_the_bridge),
        ("tanks_drive_into_the_mine", check_tanks_drive_into_the_mine),
        (
            "tanks_drive_into_the_heading",
            check_tanks_drive_into_the_heading,
        ),
        ("the_crag_stops_tanks", check_the_crag_stops_tanks),
        ("tanks_climb_the_box_cut", check_tanks_climb_the_box_cut),
        ("tanks_climb_onto_the_mesa", check_tanks_climb_onto_the_mesa),
        ("tanks_use_the_wash_ramps", check_tanks_use_the_wash_ramps),
        (
            "tanks_cross_the_wash_at_the_fords",
            check_tanks_cross_the_wash_at_the_fords,
        ),
        (
            "routes_keep_to_their_level",
            check_routes_keep_to_their_level,
        ),
        ("the_ways_round_are_open", check_the_ways_round_are_open),
        (
            "a_bot_gets_under_the_bridge",
            check_a_bot_gets_under_the_bridge,
        ),
        (
            "a_bot_gets_into_the_mine_under_it",
            check_a_bot_gets_into_the_mine_under_it,
        ),
        ("the_edge_is_rock", check_the_edge_is_rock),
        (
            "repair_spots_are_open_to_the_sky",
            check_repair_spots_are_open_to_the_sky,
        ),
        (
            "tanks_drive_through_the_warehouse",
            check_tanks_drive_through_the_warehouse,
        ),
        (
            "tanks_drive_in_at_the_rail_door",
            check_tanks_drive_in_at_the_rail_door,
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
