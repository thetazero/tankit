//! A shield generator that's down (#341): each side's own model, wrecked,
//! and once its holder sets to work on it, going back up.
//!
//! **Wrecked.** Whatever the model, it's knocked down the same way: its
//! light goes out (no glow, no beam, no core, no hum: what glowed is
//! scorched dark), and the generator itself snaps a few meters up. What
//! stood over the break falls in one piece toward the shelled side, over
//! the broken ring, its top on the ground; whatever lit or reached up to it
//! from the plant is gone, and its pieces lie round the plinth. Crystals
//! snap, their tops lying beside the stumps; a crystal held up on light
//! falls and shatters. So the Imperium's mast lies snapped across the
//! precinct, the Resonant's gilt spire lies on its side with the crystal
//! in pieces under it, the Uthenn's cubes are tumbled, and the Ithessar's
//! crystals stand broken round a dais with no plasma over it.
//!
//! **Under repair.** The wreck's hauled off and the generator goes back up
//! from the break, as far as the repairs have got (`Condition::Down`'s
//! `done` of `of`): scaffolding round it up to there, a tower crane over
//! it lifting the next piece in, work lights, parts stacked by the crane.
//! The scaffolding keeps to the plinth and the crane stands outside the Hold
//! ring, so the fight has the same ground as round a generator that's up.
//! Still dark: it's not up until it's done.

use std::f32::consts::TAU;

use glam::{EulerRot, Quat, Vec2, Vec3};

use super::{PLINTH_R, angle, between_doors, dir, shelled};
use crate::faction::Faction;
use crate::hold;
use crate::map::procgen::skeleton::Landmark;
use crate::map::{Ambient, Crystal, Map, OBox, rocks};
use crate::render::PointLight;
use crate::render::material::Material;
use crate::rng::Rng;

/// What shape a shield generator is in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Condition {
    /// Up: lit, its shield thrown up.
    #[default]
    Up,
    /// Knocked down, `done` of the `of` steps of repair it takes done:
    /// none, a wreck nobody's started on; more, going back up.
    Down { done: u8, of: u8 },
}

impl Condition {
    /// A damaged generator, as the galaxy has them (#422): scaffolded, half
    /// rebuilt.
    pub const DAMAGED: Condition = Condition::Down { done: 2, of: 4 };

    /// A generator down with `left` of `of` ticks of repair still to go
    /// (none left: it's up).
    pub fn down(left: u8, of: u8) -> Condition {
        match left {
            0 => Condition::Up,
            _ => Condition::Down {
                done: of.saturating_sub(left),
                of,
            },
        }
    }

    /// How far its repairs have got (0..1), if it's down.
    pub fn repaired(self) -> Option<f32> {
        match self {
            Condition::Up => None,
            Condition::Down { done, of } => Some((done as f32 / of.max(1) as f32).min(1.0)),
        }
    }
}

/// Where each of the map's lists stood before the generator went up: what
/// lies past these is the generator's.
pub(in crate::map::procgen) struct Mark {
    oboxes: usize,
    lights: usize,
    glow_paths: usize,
    beams: usize,
    cores: usize,
    crystals: usize,
    sounds: usize,
}

impl Mark {
    pub(in crate::map::procgen) fn of(m: &Map) -> Mark {
        Mark {
            oboxes: m.oboxes.len(),
            lights: m.lights.len(),
            glow_paths: m.glow_paths.len(),
            beams: m.beams.len(),
            cores: m.cores.len(),
            crystals: m.crystals.len(),
            sounds: m.sounds.len(),
        }
    }
}

/// How a side's model breaks, over the ground it stands on.
struct Wreck {
    /// Where the generator snaps.
    cut: f32,
    /// How high it stands whole (its crown, its heart): how high the
    /// scaffolding has to go to put it back.
    height: f32,
    /// What it breaks into.
    debris: Vec3,
    debris_mat: Material,
}

fn wreck(owner: Faction) -> Wreck {
    match owner {
        // The mast snaps over the blast wall.
        Faction::Imperium | Faction::Company => Wreck {
            cut: 8.5,
            height: 38.0,
            debris: Vec3::new(0.24, 0.25, 0.27),
            debris_mat: Material::Metal,
        },
        // The reliquary's posts snap over the screen wall.
        Faction::Resonant => Wreck {
            cut: 6.5,
            height: 27.0,
            debris: Vec3::new(0.72, 0.68, 0.59),
            debris_mat: Material::Plaster,
        },
        // The stack goes over above its first cube.
        Faction::Uthenn => Wreck {
            cut: 8.6,
            height: 32.0,
            debris: Vec3::new(0.2, 0.24, 0.3),
            debris_mat: Material::Metal,
        },
        // Nothing stands in the middle but the dais: the crystals round it
        // snap, and the heart they held goes out.
        Faction::Ithessar => Wreck {
            cut: 3.0,
            height: 24.0,
            debris: Vec3::new(0.03, 0.08, 0.2),
            debris_mat: Material::Vhaal,
        },
    }
}

/// What a scorched glow looks like.
const SCORCHED: Vec3 = Vec3::new(0.05, 0.05, 0.055);
/// The scaffolding's tubes.
const TUBE: Vec3 = Vec3::new(0.5, 0.5, 0.48);
const BOARD: Vec3 = Vec3::new(0.46, 0.36, 0.24);
/// How far the scaffold's outer tubes stand off its inner ones.
const SCAFFOLD_DEPTH: f32 = 1.0;
/// Half across the crane's pad, and the room it and its parts want.
const CRANE_PAD: f32 = 1.6;
const CRANE_ROOM: f32 = 4.6;
/// The crane's paint.
const CRANE: Vec3 = Vec3::new(0.8, 0.58, 0.1);
/// Work lights: sodium-warm.
const WORK_LIGHT: Vec3 = Vec3::new(1.0, 0.72, 0.42);

/// Its half extents along its own axes.
fn halves(o: &OBox) -> Vec3 {
    Vec3::new(o.half.x, (o.y1 - o.y0) * 0.5, o.half.y)
}

/// Put it at `c` (its center in 3D), `h` across along its own axes.
fn place(o: &mut OBox, c: Vec3, h: Vec3) {
    o.center = Vec2::new(c.x, c.z);
    o.half = Vec2::new(h.x, h.z);
    o.y0 = c.y - h.y;
    o.y1 = c.y + h.y;
}

/// Its own axes in the world, and its half extent along each.
fn axes(o: &OBox) -> [(Vec3, f32); 3] {
    let (r, h) = (o.rotation(), halves(o));
    [(r * Vec3::X, h.x), (r * Vec3::Y, h.y), (r * Vec3::Z, h.z)]
}

/// Its lowest and highest points.
fn span_y(o: &OBox) -> (f32, f32) {
    let ext: f32 = axes(o).iter().map(|(u, e)| u.y.abs() * e).sum();
    let cy = (o.y0 + o.y1) * 0.5;
    (cy - ext, cy + ext)
}

/// Cut it across at height `y`, along whichever of its axes stands most
/// upright: the piece below and the piece above, and whether the cut is
/// its top face (drawn broken). None if it doesn't reach across `y`, or
/// lies too flat to cut.
fn split(o: &OBox, y: f32) -> Option<(OBox, OBox, bool)> {
    let ax = axes(o);
    let i = (0..3)
        .max_by(|&a, &b| (ax[a].0.y.abs() * ax[a].1).total_cmp(&(ax[b].0.y.abs() * ax[b].1)))?;
    let (u, e) = ax[i];
    if u.y.abs() < 0.5 {
        return None;
    }
    let c = o.center3();
    let t = (y - c.y) / u.y;
    if t <= -e + 0.15 || t >= e - 0.15 {
        return None;
    }
    let h = halves(o);
    let piece = |a: f32, b: f32| {
        let mut p = *o;
        let mut hh = h;
        hh[i] = (b - a) * 0.5;
        place(&mut p, c + u * (a + b) * 0.5, hh);
        p
    };
    let (lo, hi) = if u.y > 0.0 {
        (piece(-e, t), piece(t, e))
    } else {
        (piece(t, e), piece(-e, t))
    };
    Some((lo, hi, i == 1 && u.y > 0.0))
}

/// Turn it about `pivot` by `q`, as part of something rigid.
fn turn(o: &mut OBox, pivot: Vec3, q: Quat) {
    let h = halves(o);
    let c = pivot + q * (o.center3() - pivot);
    let (yaw, tilt, roll) = (q * o.rotation()).to_euler(EulerRot::YXZ);
    (o.yaw, o.tilt, o.roll) = (yaw, tilt, roll);
    place(o, c, h);
}

/// Is `p` in one of the ways in (a door and the way out from it), within
/// `pad` of its line?
fn in_doorway(lm: &Landmark, p: Vec2, pad: f32) -> bool {
    lm.doors.iter().any(|&d| {
        let q = p - lm.center;
        q.dot(d) > 0.0 && q.perp_dot(d).abs() < pad + 5.0
    })
}

/// Does it lie across a way in anywhere along its length?
fn blocks_door(lm: &Landmark, o: &OBox) -> bool {
    let c = o.center3();
    let reach = Vec2::new(o.half.x, o.half.y)
        .max_element()
        .max((o.y1 - o.y0) * 0.5);
    axes(o)
        .iter()
        .flat_map(|&(u, e)| [c + u * e, c - u * e])
        .chain([c])
        .any(|p| in_doorway(lm, Vec2::new(p.x, p.z), reach.min(2.0)))
}

fn is_glow(o: &OBox) -> bool {
    o.side_mat == Material::Glow || o.top_mat == Material::Glow
}

/// Knock down (or, partly repaired, put partly back up) the generator
/// `owner` put up in landmark `lm`'s place, on ground at `base`: everything
/// past `mark` in the map's lists is the generator's.
pub(in crate::map::procgen) fn ruin(
    map: &mut Map,
    rng: &mut Rng,
    lm: &Landmark,
    base: f32,
    mark: &Mark,
    owner: Faction,
    condition: Condition,
) {
    let Some(k) = condition.repaired() else {
        return;
    };
    let repairing = k > 0.0;
    let w = wreck(owner);
    let c = lm.center;
    // Which way it fell: toward the shelled side, over the broken ring.
    let fall = lm.fallen.unwrap_or_else(|| dir(between_doors(lm)[0]));

    // Its light goes out: no glow on the ground, no beam, no core, no hum.
    map.lights.truncate(mark.lights);
    map.glow_paths.truncate(mark.glow_paths);
    map.beams.truncate(mark.beams);
    map.cores.truncate(mark.cores);
    let sounds: Vec<_> = map
        .sounds
        .drain(mark.sounds..)
        .filter(|s| matches!(s.kind, Ambient::Cable | Ambient::Bell | Ambient::Banner))
        .collect();
    map.sounds.extend(sounds);

    // Repairs put it back up from the break, a step at a time.
    let cut = base + w.cut;
    let standing = cut + (base + w.height - cut) * k;
    let core_r = PLINTH_R + 0.5;
    let ring_r = lm.radius - 1.0;
    let mut pieces = Vec::new();
    for o in map.oboxes.split_off(mark.oboxes) {
        let core = o.center.distance(c) < core_r;
        match split(&o, standing).filter(|_| core) {
            Some((mut lo, hi, top)) => {
                lo.broken |= top && !repairing && !is_glow(&lo);
                pieces.push(lo);
                pieces.push(hi);
            }
            None => pieces.push(o),
        }
    }
    let mut fallen = Vec::new();
    for mut o in pieces {
        let d = o.center.distance(c);
        let mid = o.center3().y;
        if is_glow(&o) {
            // Lamps and seams scorched dark; what glowed up high is gone.
            if mid > standing && d < ring_r {
                continue;
            }
            o.color = SCORCHED;
            o.side_mat = Material::Metal;
            o.top_mat = Material::Metal;
            map.oboxes.push(o);
        } else if d < core_r && mid > standing {
            // Over the break: it falls (or, being repaired, isn't back yet).
            if !repairing {
                fallen.push(o);
            }
        } else if d < ring_r && d >= core_r && mid > cut + 4.0 {
            // What reached up to it from the plant (conduits, a crown).
        } else {
            map.oboxes.push(o);
        }
    }
    if !repairing {
        topple(map, lm, base, cut, fall, fallen);
    }
    crystals(map, rng, lm, base, mark, k);
    if repairing {
        let r = map.oboxes[mark.oboxes..]
            .iter()
            .filter(|o| o.center.distance(c) < core_r && span_y(o).1 > base + 1.6)
            .flat_map(|o| o.shadow())
            .map(|p| p.distance(c))
            .fold(2.5f32, f32::max)
            + 0.35;
        map.scaffold(
            lm,
            base,
            r.min(PLINTH_R - SCAFFOLD_DEPTH - 0.1),
            standing + 1.5,
        );
        let foot = crane_foot(map, lm, base, mark, fall);
        map.crane(
            rng,
            lm,
            base,
            foot,
            (standing + 12.0).max(base + 22.0),
            standing + 3.0,
            w.debris,
        );
        map.sound(Ambient::Machine, Vec3::new(c.x, standing, c.y));
    } else {
        map.wreckage(rng, lm, base, &w);
        map.sound(Ambient::Hull, Vec3::new(c.x, cut, c.y));
    }
}

/// Where the crane stands: outside the Hold ring and inside the landmark's
/// wall, clear of its plant and ways in with room for the parts stacked by
/// it, as near away from `fall` as there's room.
fn crane_foot(map: &Map, lm: &Landmark, base: f32, mark: &Mark, fall: Vec2) -> Vec2 {
    let standing: Vec<&OBox> = map.oboxes[mark.oboxes..]
        .iter()
        .filter(|o| o.solid && span_y(o).0 < base + 4.0 && span_y(o).1 > base + 0.3)
        .collect();
    // How far from anything, as far as it needs.
    let room = |p: Vec2| {
        let boxes = standing.iter().map(|o| o.distance(p));
        let crystals = map.crystals[mark.crystals..].iter();
        let doors = lm.doors.iter().filter_map(|&d| {
            let q = p - lm.center;
            (q.dot(d) > 0.0).then(|| q.perp_dot(d).abs() - 5.0)
        });
        boxes
            .chain(crystals.filter(|c| c.solid).map(|c| c.distance(p)))
            .chain(doors)
            .fold(CRANE_ROOM, f32::min)
    };
    let away = angle(-fall);
    let nearest = hold::RADIUS + CRANE_PAD * 1.5;
    let n = 72;
    let mut best = (f32::MIN, lm.center - fall * nearest);
    let mut d = nearest;
    while d < (lm.radius - 3.0).max(nearest + 0.1) {
        for k in 0..n {
            // From straight away from the fall, round both ways.
            let turn = (k + 1) / 2;
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let a = away + side * turn as f32 / n as f32 * TAU;
            if shelled(lm, a) {
                continue;
            }
            let p = lm.center + dir(a) * d;
            let score = room(p) - turn as f32 * 0.01;
            if score > best.0 {
                best = (score, p);
            }
        }
        d += 0.5;
    }
    best.1
}

/// What stood over the break falls together toward `fall`, about the
/// break's edge, its top coming down on the ground. Anything farther out
/// than it can lie in the precinct shatters.
fn topple(map: &mut Map, lm: &Landmark, base: f32, cut: f32, fall: Vec2, pieces: Vec<OBox>) {
    let reach = lm.radius + 6.0;
    let pieces: Vec<OBox> = pieces
        .into_iter()
        .filter(|o| span_y(o).1 - cut < reach)
        .collect();
    let Some(top) = pieces.iter().map(|o| span_y(o).1 - cut).reduce(f32::max) else {
        return;
    };
    let pivot = Vec3::new(lm.center.x, cut, lm.center.y) + Vec3::new(fall.x, 0.0, fall.y) * 1.5;
    // Turned past level till its top's on the ground.
    let drop = (base + 1.2 - cut) / top.max(cut - base + 0.5);
    let tip = drop.clamp(-1.0, 1.0).acos();
    let axis = Vec3::Y.cross(Vec3::new(fall.x, 0.0, fall.y)).normalize();
    let q = Quat::from_axis_angle(axis, tip);
    for mut o in pieces {
        turn(&mut o, pivot, q);
        if o.solid && blocks_door(lm, &o) {
            o.solid = false;
        }
        o.chip = o.chip.max(0.04);
        map.oboxes.push(o);
    }
}

/// The generator's crystals: dark; standing ones snapped (or, being
/// repaired, growing back), one held up on light fallen and shattered (or,
/// being repaired, not back yet).
fn crystals(map: &mut Map, rng: &mut Rng, lm: &Landmark, base: f32, mark: &Mark, k: f32) {
    let ring_r = lm.radius - 1.0;
    for mut cr in map.crystals.split_off(mark.crystals) {
        let foot = Vec2::new(cr.base.x, cr.base.z);
        let was_lit = cr.lit;
        cr.lit = false;
        if foot.distance(lm.center) >= ring_r || !was_lit {
            map.crystals.push(cr);
            continue;
        }
        let len = cr.axis.length();
        if cr.base.y > base + 1.0 {
            if k > 0.0 {
                continue;
            }
            // Fallen off its light and shattered round the plinth.
            for _ in 0..5 {
                let a = rng.range(0.0, TAU);
                let p = lm.center + dir(a) * rng.range(PLINTH_R * 0.4, PLINTH_R + 3.0);
                let lie = dir(rng.range(0.0, TAU)) * len * rng.range(0.25, 0.45);
                map.crystals.push(Crystal {
                    base: Vec3::new(p.x, base + 0.2, p.y),
                    axis: Vec3::new(lie.x, rng.range(0.2, 0.8), lie.y),
                    radius: cr.radius * rng.range(0.35, 0.55),
                    seed: rng.next_u64() as u32,
                    solid: false,
                    ..cr
                });
            }
            continue;
        }
        if len < 5.0 {
            map.crystals.push(cr);
            continue;
        }
        if k > 0.0 {
            // Growing back from the stump.
            cr.axis *= 0.3 + 0.7 * k;
            map.crystals.push(cr);
            continue;
        }
        // Snapped: a stump, and its top lying beside it.
        let full = cr.axis;
        cr.axis = full * 0.3;
        map.crystals.push(cr);
        let side = if rng.chance(0.5) { 0.9 } else { -0.9 };
        let fell = dir(angle(foot - lm.center) + side);
        let at = foot + Vec2::new(full.x, full.z) * 0.3 + fell * (cr.radius + 0.6);
        let top = Crystal {
            base: Vec3::new(at.x, base - 0.4, at.y),
            axis: Vec3::new(fell.x, 0.0, fell.y) * len * 0.62 + Vec3::Y * 1.0,
            radius: cr.radius * 0.8,
            seed: rng.next_u64() as u32,
            ..cr
        };
        let tip = at + fell * len * 0.62;
        let solid = !(0..=4).any(|s| in_doorway(lm, at.lerp(tip, s as f32 / 4.0), top.radius));
        map.crystals.push(Crystal { solid, ..top });
    }
}

impl Map {
    /// What it broke into, strewn round the plinth.
    fn wreckage(&mut self, rng: &mut Rng, lm: &Landmark, base: f32, w: &Wreck) {
        for _ in 0..22 {
            let a = rng.range(0.0, TAU);
            let p = lm.center + dir(a) * rng.range(PLINTH_R + 0.8, PLINTH_R + 9.0);
            if in_doorway(lm, p, 0.0) && rng.chance(0.7) {
                continue;
            }
            let s = rng.range(0.35, 1.3);
            let half = Vec2::new(s, s * rng.range(0.4, 0.9));
            let top = base + s * rng.range(0.3, 0.8);
            let piece = if rng.chance(0.5) {
                OBox {
                    side_mat: w.debris_mat,
                    top_mat: w.debris_mat,
                    rock: None,
                    chip: s * 0.1,
                    broken: true,
                    ..rocks::rubble(p, rng.range(0.0, TAU), half, base - 0.2, top, w.debris)
                }
            } else {
                rocks::rubble(
                    p,
                    rng.range(0.0, TAU),
                    half,
                    base - 0.3,
                    top,
                    w.debris * 0.8,
                )
            };
            self.oboxes.push(OBox {
                tilt: rng.range(-0.5, 0.5),
                roll: rng.range(-0.4, 0.4),
                solid: false,
                ..piece
            });
        }
    }

    /// A tube scaffold round the middle, `r` out, two tubes deep, boarded
    /// every other lift, up to `top`; work lights on its top lift.
    fn scaffold(&mut self, lm: &Landmark, base: f32, r: f32, top: f32) {
        let c = lm.center;
        // Posts close enough that no tank fits between them.
        let n = 12;
        let depth = SCAFFOLD_DEPTH;
        let lift = 2.2;
        let lifts = ((top - base) / lift).ceil().max(2.0) as usize;
        let h = base + lifts as f32 * lift;
        let corner = |i: usize, r: f32| c + dir((i % n) as f32 / n as f32 * TAU) * r;
        let at = |p: Vec2, y: f32| Vec3::new(p.x, y, p.y);
        let tube =
            |m: &mut Map, a: Vec3, b: Vec3| m.segment(a, b, 0.055, TUBE, Material::Metal, true);
        for i in 0..n {
            for rr in [r, r + depth] {
                let p = corner(i, rr);
                tube(self, at(p, base), at(p, h + 1.0));
            }
            // Braced on the outside, every other face.
            if i % 2 == 0 {
                for l in (0..lifts).step_by(2) {
                    let y = base + l as f32 * lift;
                    let y1 = (y + lift * 2.0).min(h);
                    tube(
                        self,
                        at(corner(i, r + depth), y),
                        at(corner(i + 1, r + depth), y1),
                    );
                }
            }
        }
        for l in 1..=lifts {
            let y = base + l as f32 * lift;
            for i in 0..n {
                for rr in [r, r + depth] {
                    tube(self, at(corner(i, rr), y), at(corner(i + 1, rr), y));
                }
                tube(self, at(corner(i, r), y), at(corner(i, r + depth), y));
                // Boards across every other lift, and the top one.
                if l % 2 == 0 || l == lifts {
                    let (a, b) = (corner(i, r + depth * 0.5), corner(i + 1, r + depth * 0.5));
                    let along = b - a;
                    self.block(
                        (a + b) * 0.5,
                        Vec2::new(along.length() * 0.5 - 0.08, depth * 0.5 - 0.1),
                        -angle(along),
                        (y + 0.07, y + 0.12),
                        BOARD,
                        Material::Wood,
                        true,
                    );
                }
            }
        }
        // A guard rail round the top lift.
        for i in 0..n {
            let y = h + 1.0;
            tube(
                self,
                at(corner(i, r + depth), y),
                at(corner(i + 1, r + depth), y),
            );
        }
        // Work lights on the top lift, turned in on the job.
        for i in (0..n).step_by(3) {
            let p = corner(i, r + depth);
            let lamp = at(p, h + 1.3);
            self.block(
                p,
                Vec2::splat(0.22),
                0.0,
                (h + 1.1, h + 1.45),
                WORK_LIGHT,
                Material::Glow,
                false,
            );
            self.lights.push(PointLight {
                pos: lamp + Vec3::Y * 0.6,
                color: WORK_LIGHT * 4.5,
                radius: 15.0,
            });
        }
    }

    /// A tower crane on `foot`, its jib `height` up and out over the
    /// middle, the next piece of the generator (`part`'s colour) hanging
    /// from its hook at `hook`.
    #[allow(clippy::too_many_arguments)]
    fn crane(
        &mut self,
        rng: &mut Rng,
        lm: &Landmark,
        base: f32,
        foot: Vec2,
        height: f32,
        hook: f32,
        part: Vec3,
    ) {
        let c = lm.center;
        let to = (c - foot).normalize_or(Vec2::X);
        let side = to.perp();
        let at = |p: Vec2, y: f32| Vec3::new(p.x, y, p.y);
        let chord = |m: &mut Map, a: Vec3, b: Vec3, r: f32| {
            m.segment(a, b, r, CRANE, Material::Painted, true)
        };
        // Its foot: a concrete pad.
        self.block(
            foot,
            Vec2::splat(CRANE_PAD),
            -angle(to),
            (base - 0.2, base + 0.8),
            Vec3::splat(0.5),
            Material::Concrete,
            true,
        );
        // The mast: four chords, laced.
        let w = 0.8;
        let corners: Vec<Vec2> = [(1.0, 1.0), (1.0, -1.0), (-1.0, -1.0), (-1.0, 1.0)]
            .iter()
            .map(|&(u, v)| foot + to * u * w + side * v * w)
            .collect();
        for &p in &corners {
            chord(self, at(p, base + 0.8), at(p, height), 0.1);
        }
        let step = 2.4;
        let mut y = base + 0.8;
        while y + step <= height + 0.01 {
            for q in 0..4 {
                let (a, b) = (corners[q], corners[(q + 1) % 4]);
                chord(self, at(a, y + step), at(b, y + step), 0.05);
                chord(self, at(a, y), at(b, y + step), 0.04);
            }
            y += step;
        }
        // The slewing deck and cab.
        self.block(
            foot,
            Vec2::splat(1.2),
            -angle(to),
            (height, height + 0.6),
            CRANE * 0.8,
            Material::Painted,
            true,
        );
        let cab = foot + side * 1.5 + to * 0.6;
        self.block(
            cab,
            Vec2::new(0.8, 0.7),
            -angle(to),
            (height + 0.6, height + 2.4),
            CRANE,
            Material::Painted,
            true,
        );
        // The jib out over the middle, and the counter-jib behind.
        let jib = foot.distance(c) + 10.0;
        let top = height + 0.6;
        for s in [-0.55f32, 0.55] {
            chord(
                self,
                at(foot + side * s, top),
                at(foot + side * s + to * jib, top),
                0.07,
            );
        }
        chord(
            self,
            at(foot, top + 1.1),
            at(foot + to * jib, top + 1.1),
            0.06,
        );
        let mut d = 0.0;
        while d + 2.0 <= jib {
            for s in [-0.55f32, 0.55] {
                chord(
                    self,
                    at(foot + side * s + to * d, top),
                    at(foot + to * (d + 2.0), top + 1.1),
                    0.035,
                );
            }
            d += 2.0;
        }
        let back = 7.0;
        for s in [-0.55f32, 0.55] {
            chord(
                self,
                at(foot + side * s, top),
                at(foot + side * s - to * back, top),
                0.07,
            );
        }
        self.block(
            foot - to * (back - 1.2),
            Vec2::new(1.0, 1.3),
            -angle(to),
            (top - 2.2, top),
            Vec3::splat(0.42),
            Material::Concrete,
            true,
        );
        // The cat head, and its ties out to both ends.
        let apex = at(foot, top + 5.0);
        chord(self, at(foot, top), apex, 0.12);
        chord(self, apex, at(foot + to * jib * 0.7, top + 1.1), 0.03);
        chord(self, apex, at(foot - to * back, top), 0.03);
        // Warning lights on the cat head and the jib's tip.
        for p in [apex + Vec3::Y * 0.2, at(foot + to * jib, top + 1.3)] {
            self.block(
                Vec2::new(p.x, p.z),
                Vec2::splat(0.14),
                0.0,
                (p.y, p.y + 0.25),
                Vec3::new(1.0, 0.15, 0.08),
                Material::Glow,
                false,
            );
        }
        // The trolley over the middle, the hook, and the piece on it.
        let over = foot + to * foot.distance(c);
        self.block(
            over,
            Vec2::new(0.6, 0.8),
            -angle(to),
            (top - 0.5, top),
            CRANE * 0.7,
            Material::Painted,
            true,
        );
        let hang = hook + 2.6;
        for s in [-0.2f32, 0.2] {
            self.segment(
                at(over + side * s, top - 0.5),
                at(over + side * s, hang + 0.6),
                0.02,
                Vec3::splat(0.12),
                Material::Metal,
                false,
            );
        }
        self.block(
            over,
            Vec2::splat(0.35),
            0.0,
            (hang, hang + 0.6),
            CRANE,
            Material::Painted,
            true,
        );
        let yaw = rng.range(0.0, TAU);
        for s in [-1.0f32, 1.0] {
            let corner = over + Vec2::from_angle(-yaw) * 1.1 * s;
            self.segment(
                at(over, hang),
                at(corner, hook + 0.9),
                0.02,
                Vec3::splat(0.12),
                Material::Metal,
                false,
            );
        }
        self.block(
            over,
            Vec2::new(1.3, 0.9),
            yaw,
            (hook - 0.7, hook + 0.9),
            part,
            Material::Metal,
            true,
        )
        .chip = 0.05;
        // Parts stacked by its foot, waiting to go up.
        for q in 0..5 {
            let p =
                foot + side * (if q % 2 == 0 { 2.6 } else { -2.6 }) - to * (q as f32 * 0.9 - 1.0);
            let s = rng.range(0.6, 0.9);
            let y0 = base + if q == 4 { 1.2 } else { 0.0 };
            self.block(
                p,
                Vec2::splat(s),
                -angle(to) + rng.range(-0.2, 0.2),
                (y0, y0 + s * 1.3),
                part * rng.range(0.85, 1.1),
                Material::Metal,
                true,
            )
            .chip = 0.04;
        }
        let lamp = at(foot + side * 2.6, base + 3.4);
        self.segment(
            at(foot + side * 2.6 + to * 0.8, base),
            lamp,
            0.06,
            TUBE,
            Material::Metal,
            true,
        );
        self.lights.push(PointLight {
            pos: lamp,
            color: WORK_LIGHT * 3.0,
            radius: 12.0,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;

    /// The Euler angles `turn` decomposes into are the ones `OBox::rotation`
    /// composes.
    #[test]
    fn turning_keeps_a_box_rigid() {
        let mut o = OBox {
            center: Vec2::new(3.0, -2.0),
            yaw: 0.7,
            tilt: -0.4,
            roll: 1.1,
            half: Vec2::new(0.5, 2.0),
            y0: 9.0,
            y1: 10.0,
            ..OBox::default()
        };
        let pivot = Vec3::new(1.0, 5.0, 1.0);
        let q = Quat::from_axis_angle(Vec3::new(1.0, 0.0, 1.0).normalize(), 1.9);
        let corner = |o: &OBox| o.center3() + o.rotation() * halves(o);
        let want = pivot + q * (corner(&o) - pivot);
        turn(&mut o, pivot, q);
        assert!(
            corner(&o).distance(want) < 1e-4,
            "{:?} vs {want:?}",
            corner(&o)
        );
    }

    /// Cutting a box keeps its two pieces end to end, the cut at the height asked for.
    #[test]
    fn a_cut_box_is_two_pieces_end_to_end() {
        let o = OBox {
            tilt: -FRAC_PI_2 * 0.9,
            half: Vec2::new(0.3, 6.0),
            y0: 9.7,
            y1: 10.3,
            ..OBox::default()
        };
        let (lo, hi, _) = split(&o, 12.0).unwrap();
        assert!((span_y(&lo).1 - span_y(&hi).0).abs() < 0.7);
        assert!(span_y(&lo).0 < span_y(&hi).0);
        assert!((halves(&lo).z + halves(&hi).z - 6.0).abs() < 1e-4);
    }

    /// A generator being repaired leaves as much of the Hold ring to drive
    /// as one that's up, and nothing of its scaffold can be driven through.
    #[test]
    fn repairs_keep_out_of_the_hold_ring() {
        use crate::map::procgen::{Look, build, skeleton};
        for owner in [
            Faction::Imperium,
            Faction::Resonant,
            Faction::Uthenn,
            Faction::Ithessar,
        ] {
            let spec = |condition| {
                crate::shield_spec(2, Look::Imperium, crate::Generator { owner, condition })
            };
            let up = spec(Condition::Up);
            let lm = skeleton(&up).landmarks[0].clone();
            let open = |m: &Map| {
                let mut n = 0;
                for k in 0..120 {
                    let mut r = PLINTH_R + 0.6;
                    while r < hold::RADIUS {
                        let p = lm.center + dir(k as f32 / 120.0 * TAU) * r;
                        let g = m.ground_height(p.x, p.y);
                        n += !m.oboxes.iter().any(|o| {
                            let (y0, y1) = span_y(o);
                            o.solid && y0 < g + 3.0 && y1 > g + 0.4 && o.distance(p) < 0.5
                        }) as u32;
                        r += 0.5;
                    }
                }
                n
            };
            let (whole, _) = build(&up);
            let (mending, _) = build(&spec(Condition::DAMAGED));
            assert!(open(&mending) >= open(&whole), "{owner:?}");
            for m in &mending.oboxes {
                let tube = m.color == TUBE || m.color == BOARD || m.color == CRANE;
                assert!(m.solid || !tube, "{owner:?}: {m:?}");
            }
        }
    }
}
