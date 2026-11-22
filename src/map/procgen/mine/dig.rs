//! Digging a laid-out crystal mine (see `Layout`): the earthworks and the
//! rock (the Face and the Crag sculpted, the caves walled, roofed and
//! lined, the wash's rocks, the scree, the boulders and the fill of
//! cover), and the mining gear any mine has (steel sets, the railroad and
//! bridge, the tipple, the headframe, the warehouse, carts, crates, cribs,
//! cutting rigs). What's anyone's in particular comes from the style
//! (`MineStyle`). Also the gameplay: spawns, the places bots head for,
//! their ways round, repair drops, the Raid stands and the menu vistas.
//!
//! The builders here were the Seam's (the hand-built mine this biome
//! replaced), reading the layout instead of its fixed plan.

use std::f32::consts::{FRAC_PI_2, PI, TAU};
use std::ops::{Deref, DerefMut};

use glam::{Quat, Vec2, Vec3, Vec4};

use super::*;
use crate::faction::Faction;
use crate::map::crag;
use crate::map::landmarks::Rig;
use crate::map::rocks;
use crate::map::valley::{noise, noise2};
use crate::map::vault;
use crate::map::{Ambient, Crystal, Map, OBox, Terrain};
use crate::render::material::Material;
use crate::render::mesh::MeshData;
use crate::rng::Rng;
use crate::tank::model::{self, Skin};

/// Sandstone, in a few shades; the desert floor.
pub(super) const STONE: Vec3 = Vec3::new(0.58, 0.34, 0.22);
pub(super) const STONE_DARK: Vec3 = Vec3::new(0.4, 0.23, 0.15);
/// Marks the stand-ins for the blocks that used to stand along the Face.
pub(super) const FACE_BLOCK: Vec3 = Vec3::new(0.123, 0.456, 0.789);
pub(super) const STONE_PALE: Vec3 = Vec3::new(0.7, 0.5, 0.34);
const SAND: Vec3 = Vec3::new(0.66, 0.46, 0.3);
/// Mine waste: pale crushed rock.
const TAILINGS: Vec3 = Vec3::new(0.47, 0.41, 0.35);
/// The mesa's top: weathered rock, and the cave roofs laid flush with it.
pub(super) const MESA_TOP: Vec3 = Vec3::new(0.46, 0.26, 0.17);
/// Raw crystal, in the rock and in the ore: the crystal everyone's here
/// for, nobody's yet. Cloudy blue-grey, dull until it's cut.
pub(super) const RAW: Vec3 = Vec3::new(0.1, 0.125, 0.17);
pub(super) const STEEL: Vec3 = Vec3::new(0.2, 0.21, 0.24);
pub(super) const RUST: Vec3 = Vec3::new(0.36, 0.22, 0.14);
const TIMBER: Vec3 = Vec3::new(0.42, 0.32, 0.22);
/// Black iron, gone brown at the edges: cart wheels and binding.
const IRON: Vec3 = Vec3::new(0.16, 0.13, 0.11);
const CONCRETE: Vec3 = Vec3::new(0.6, 0.57, 0.52);
/// Mine machinery's paint, faded in the sun.
const MACHINE: Vec3 = Vec3::new(0.56, 0.42, 0.2);
/// Fresh-cut stone, and the scorch along a cut.
const CUT: Vec3 = Vec3::new(0.66, 0.47, 0.34);
const SCORCH: Vec3 = Vec3::new(0.2, 0.12, 0.11);

/// A map being dug: the map, the layout it's dug to, the style it's
/// dressed in. Reads as the map (the builders' `self.oboxes` are its).
pub(in crate::map) struct Dig<'a> {
    pub map: &'a mut Map,
    pub l: &'a Layout,
    pub st: &'a dyn MineStyle,
}

impl Deref for Dig<'_> {
    type Target = Map;
    fn deref(&self) -> &Map {
        self.map
    }
}

impl DerefMut for Dig<'_> {
    fn deref_mut(&mut self) -> &mut Map {
        self.map
    }
}

/// A blank map for the layout: the arena, no bases yet.
fn blank(l: &Layout, sides: &[Faction]) -> Map {
    let mut map = Map::empty(l.size);
    map.factions = sides.to_vec();
    map.bases = Map::no_bases(sides);
    map.open_spots = Vec::new();
    map.spawn_yaw = None;
    map.rock_dust = Some(SAND);
    map.hold_site = Some(l.hold);
    map
}

/// The play ground for the layout, painted.
fn terrain(l: &Layout) -> Terrain {
    let n = (l.size / 2.0).round() as usize + 1;
    let mut terrain = Terrain::from_fn(Vec2::ZERO, 2.0, n, n, |x, z| l.ground(x, z));
    terrain.color = Vec3::ONE;
    terrain.material = Material::Sand;
    // Round the caves the ground dips away under the roofs (drawn over
    // it, see `roof_lid` and `crag_lid`): shade it as the rock it was
    // cut from, not as a slope down into the cave, which ringed every
    // roof with a dark edge.
    terrain.normals = (0..terrain.nz)
        .flat_map(|iz| (0..terrain.nx).map(move |ix| (ix, iz)))
        .map(|(ix, iz)| {
            let p = Vec2::new(ix as f32, iz as f32) * terrain.cell;
            if l.in_cave(p) || l.cave_distance(p) > CAVE_WALL + 3.0 {
                return terrain.normal(ix, iz);
            }
            let e = terrain.cell;
            let (dx, dz) = (Vec2::new(e, 0.0), Vec2::new(0.0, e));
            Vec3::new(
                l.uncut(p - dx) - l.uncut(p + dx),
                2.0 * e,
                l.uncut(p - dz) - l.uncut(p + dz),
            )
            .normalize()
        })
        .collect();
    let seam = l.seam.clone();
    terrain.paint(|x, z, h, slope| paint(l, &seam, x, z, h, slope));
    terrain
}

/// Builds the mine `l` lays out for `spec`, dressed in `st`.
pub fn build(l: &Layout, spec: &Spec, st: &dyn MineStyle) -> Map {
    let mut rng = Rng::new(spec.seed ^ 0x5EED_D2E5);
    let mut map = blank(l, &spec.sides);
    map.atmosphere = atmosphere(&mut rng);
    map.hold_name = st.hold_name();
    map.planet = Some((spec.seed, PLANET));
    map.terrain = Some(terrain(l));
    let mut d = Dig {
        map: &mut map,
        l,
        st,
    };
    d.dig(spec.seed);
    map
}

/// The layout undressed: its ground in grey, the caves' walls and roofs and
/// the bridge's deck as plain boxes, the spawns. What `check` drives on.
pub fn greybox(l: &Layout, sides: &[Faction]) -> Map {
    let mut map = blank(l, sides);
    let mut t = Terrain::from_fn(
        Vec2::ZERO,
        2.0,
        (l.size / 2.0).round() as usize + 1,
        (l.size / 2.0).round() as usize + 1,
        |x, z| l.ground(x, z),
    );
    t.color = Vec3::splat(0.35);
    t.material = Material::Concrete;
    map.terrain = Some(t);
    let mut d = Dig {
        map: &mut map,
        l,
        st: &super::ithessar::Ithessar,
    };
    let rng = &mut Rng::new(1);
    d.cave_walls(rng);
    d.roof_slabs();
    d.rail_bridge();
    for o in d.oboxes.iter_mut() {
        o.hidden = false;
        o.color = Vec3::new(0.55, 0.45, 0.35);
    }
    d.mine_spawns();
    map
}

impl Dig<'_> {
    /// Everything, in order: the ways round first (they're on the ground
    /// and the deck), so what's strewn about keeps off them; the rock; the
    /// mine; the line; the set pieces; then the cover, boulders before the
    /// fill so they take the open ground first; the spawns last.
    fn dig(&mut self, seed: u64) {
        let lay = self.l;
        let st = self.st;
        // Each part on dice of its own, so one changing doesn't move the
        // rest.
        let dice = |k: u64| Rng::new(seed.wrapping_mul(0x2545_F491).wrapping_add(k));
        let mut rng = dice(0x5ea3_0213);
        self.mine_routes();
        let seam_rocks = self.seam(&mut rng);
        self.crag_face(&mut rng);
        self.crag_lid();
        self.rough_face(&mut rng);
        self.scree_rocks(&mut rng);
        let mut inside = dice(0x1a5e_0254);
        self.mine_caves(&mut inside);
        self.pillared_hall(&mut inside);
        self.breach(&mut rng);
        self.box_cut(&mut rng);
        self.railroad();
        self.rail_bridge();
        self.wash_rocks(&mut rng, &seam_rocks);
        for &(p, h) in &seam_rocks {
            self.rock_at(p, h);
        }
        let (p, yaw) = lay.wash_rig;
        let g = self.ground_height(p.x, p.y);
        self.cutting_rig(p, yaw, g);
        let (p, yaw) = lay.hall_rig;
        self.cutting_rig(p, yaw, 0.0);
        self.borer_bay(&mut dice(0xb0e3_0254));
        self.mine_supports();
        self.mine_gear();
        self.tipple();
        self.ore_train(&mut rng);
        self.headframe(lay.headframe);
        self.mesa_cover(&mut rng);
        self.dump_cover(&mut rng);
        self.warehouse(&mut rng);
        self.fins_and_hoodoos(&mut rng);
        self.flats_cover(&mut rng);
        self.boulder_fields(&mut dice(0xb01d_0281));
        self.fill_cover(&mut rng);
        st.base(self, 0, lay.yard, &mut rng);
        st.base(self, 1, lay.camp, &mut rng);
        self.edge_rocks(&mut rng);
        self.clear_face_blocks();
        self.mesa_face();
        self.mine_spawns();
        self.raid_stands = self.stands();
        self.mine_horizon(&mut rng);
        self.mine_sounds();
        self.vistas = self.pick_vistas();
    }

    /// What sounds here: the seam's lit crystals singing, the cutting rigs
    /// still running, the borer, the stopped train ticking in the heat.
    fn mine_sounds(&mut self) {
        let lay = self.l;
        let mut found: Vec<(Ambient, Vec3)> = self
            .crystals
            .iter()
            .filter(|c| c.lit && c.radius > 0.6)
            .map(|c| (Ambient::Crystal, c.base + c.axis * 0.35))
            .collect();
        let (p, _) = lay.wash_rig;
        found.push((Ambient::Machine, Vec3::new(p.x, -4.0, p.y)));
        let (p, _) = lay.hall_rig;
        found.push((Ambient::Machine, Vec3::new(p.x, 3.0, p.y)));
        // The borer: its machinery, and the beam's roar on the rock.
        let (from, to, _) = lay.beam;
        found.push((Ambient::Machine, Vec3::new(from.x - 5.0, 3.0, from.z)));
        found.push((Ambient::Core, to));
        let (at, _) = lay.track_at(lay.track_len(3) + 18.0);
        let g = self.ground_height(at.x, at.y);
        found.push((Ambient::Hull, Vec3::new(at.x, g + 2.5, at.y)));
        let (p, _) = line_at(&lay.wash, lay.bridge_s);
        found.push((Ambient::River, Vec3::new(p.x, -6.5, p.y)));
        for (kind, at) in found {
            self.sound(kind, at);
        }
    }

    /// Each side's Raid stand: open, level ground out in front of its
    /// spawns toward the other side.
    fn stands(&self) -> Vec<Vec2> {
        self.l
            .stands
            .iter()
            .map(|&want| {
                let g = |p: Vec2| self.ground_height(p.x, p.y);
                let open = |p: Vec2| {
                    self.mine_clear(p, 6.0)
                        && !self.l.roofed(p)
                        && (0..24).all(|k| {
                            let a = k as f32 * TAU / 24.0;
                            [4.0, 7.0].iter().all(|&r| {
                                let q = p + Vec2::new(a.cos(), a.sin()) * r;
                                self.mine_clear(q, 2.0) && (g(q) - g(p)).abs() < 0.8
                            })
                        })
                };
                (0..400)
                    .map(|k| {
                        let a = k as f32 * 2.4;
                        want + Vec2::new(a.cos(), a.sin()) * (k as f32 * 0.06)
                    })
                    .find(|&p| open(p))
                    .unwrap_or(want)
            })
            .collect()
    }

    /// Where the menus show each side's tank: by the bridge's far end,
    /// either side of it, in the sun, the camera looking past the tank at
    /// the Face, the adit's mouth and the bridge out of it.
    fn pick_vistas(&self) -> Vec<(Vec2, f32)> {
        let lay = self.l;
        let sun = self.atmosphere.sun_dir.normalize();
        let (mouth, end) = lay.bridge();
        let view = |p: Vec2| {
            let d = mouth - p;
            (-d.x).atan2(-d.y)
        };
        let good = |p: Vec2| {
            let g = self.ground_height(p.x, p.y);
            let at = Vec3::new(p.x, g + 1.9, p.y);
            let blocked = |dir: Vec3, len: f32| self.oboxes.iter().any(|o| o.ray_hit(at, dir, len));
            let under = self
                .oboxes
                .iter()
                .any(|o| o.distance(p) < 3.4 && o.y1 > g + 0.05 && o.y0 < g + 3.0)
                || self.crystals.iter().any(|c| c.distance(p) < 3.4);
            let v = view(p);
            let back = Vec2::new(v.sin(), v.cos());
            let level = (0..=14).all(|k| {
                let q = p + back * k as f32;
                (self.ground_height(q.x, q.y) - g).abs() < 0.3
            });
            level
                && !under
                && lay.wash_near(p).0 > 13.0
                && !blocked(sun, 300.0)
                && !blocked(Vec3::Y, 60.0)
        };
        let dir = (end - mouth).normalize();
        let across = Vec2::new(dir.y, -dir.x);
        [-1.0f32, 1.0]
            .iter()
            .map(|&side| {
                let want = end + dir * 4.0 + across * side * 18.0;
                let mut spots = Vec::new();
                let mut u = -30.0;
                while u <= 30.0 {
                    let mut v = -16.0;
                    while v <= 16.0 {
                        spots.push(want + Vec2::new(u, v));
                        v += 3.0;
                    }
                    u += 3.0;
                }
                spots.sort_by(|a, b| a.distance(want).total_cmp(&b.distance(want)));
                let p = spots.into_iter().find(|&p| good(p)).unwrap_or(want);
                (p, view(p))
            })
            .collect()
    }

    /// The borer's bay at the new heading's end: the style's machine,
    /// braced on the floor, its beam held on into the bore it's cutting
    /// east; spoil, raw crystal and the gear round it.
    fn borer_bay(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let st = self.st;
        st.borer(self, rng);
        let (from, to, _) = lay.beam;
        let dir = (to - from).normalize();
        let machine = Vec2::new(from.x - 5.8 * dir.x, from.z - 5.8 * dir.z);
        // Spoil from the bore, heaped either side of its mouth, clear of
        // the beam.
        let (a, b) = lay.bay;
        for &(x, z, size) in &[
            (b.x - 2.2, lay.bore.0.y - 3.0, 1.3),
            (b.x - 4.5, lay.bore.0.y - 4.6, 0.8),
            (b.x - 2.0, lay.bore.1.y + 3.2, 1.2),
            (b.x - 4.0, lay.bore.1.y + 5.0, 0.9),
        ] {
            self.rock_pile(rng, Vec2::new(x, z), size);
        }
        // Raw crystal standing out of the floor and walls in the bay's far
        // corners: the seam they're after, thick here.
        for (x, z, h) in [
            (a.x + 3.0, b.y - 3.0, 3.6),
            (b.x - 3.0, b.y - 2.6, 2.8),
            (a.x + 2.6, a.y + 3.2, 2.4),
        ] {
            self.outcrop(rng, Vec2::new(x, z), h);
        }
        for _ in 0..7 {
            let side = rng.index(3);
            let (p, out) = match side {
                0 => (Vec2::new(rng.range(a.x + 2.0, b.x - 2.0), b.y), Vec2::Y),
                1 => (Vec2::new(a.x, rng.range(a.y + 2.0, b.y - 2.0)), -Vec2::X),
                _ => (Vec2::new(rng.range(a.x + 2.0, b.x - 6.0), a.y), -Vec2::Y),
            };
            // Not over the way in from the heading.
            if side == 2 && p.x > lay.heading.0.x - 1.0 && p.x < lay.heading.1.x + 1.0 {
                continue;
            }
            // A cluster, growing out of the wall where the seam shows.
            let y = rng.range(1.2, 6.5);
            for _ in 0..4 {
                let q = p + out.perp() * rng.range(-0.8, 0.8) + out * 0.3;
                self.shard(
                    rng.next_u64(),
                    Vec3::new(q.x, y + rng.range(-0.5, 0.5) + 1.0, q.y),
                    rng.range(0.5, 1.4),
                    0.26,
                    -out + out.perp() * rng.range(-0.5, 0.5),
                    rng.range(0.6, 1.0),
                    rng.chance(0.6),
                    false,
                );
            }
        }
        // Columns of steel and crystal holding up the bay's high roof.
        for x in [a.x + 4.0, b.x - 7.0] {
            self.prop_column(
                Vec2::new(x, b.y - 4.5),
                lay.drawn_roof(Vec2::new(x, b.y - 4.5)),
            );
        }
        // Floodlights on stands, turned on the borer.
        for &(x, z) in &[(a.x + 2.0, a.y + 2.2), (b.x - 2.0, a.y + 2.0)] {
            self.floodlight(Vec2::new(x, z), machine);
        }
        // An ore cart waiting by the way in, and cut crystal crated.
        self.ore_cart(rng, Vec2::new(lay.heading.1.x + 1.6, a.y + 2.6), FRAC_PI_2);
        // Against the north wall between the columns, clear of them and of
        // the crystal in the corner.
        self.block_stack(Vec2::new(a.x + 9.5, b.y - 3.4), 0.05);
    }

    /// A block of rock, solid (see `rocks::block`).
    pub(super) fn stone(
        &mut self,
        center: Vec2,
        yaw: f32,
        half: Vec2,
        y0: f32,
        y1: f32,
        color: Vec3,
    ) {
        self.oboxes
            .push(rocks::block(center, yaw, half, y0, y1, color));
    }

    /// The box just placed is rock from the rocks toolkit, of this `kind`
    /// (ore heaped in a bin or a car: broken lumps, the heap a block).
    pub(super) fn last_is_rock(&mut self, kind: rocks::Kind) {
        if let Some(o) = self.oboxes.last_mut() {
            o.rock = Some(kind);
        }
    }

    /// A boulder: a lump of rock (drawn and collided as one).
    pub(super) fn boulder(&mut self, rng: &mut Rng, at: Vec2, size: f32, solid: bool) {
        if solid && self.in_the_way(at, size * 1.2) {
            return;
        }
        let g = self.level(at);
        let shades = [STONE, STONE_DARK, STONE_PALE];
        let yaw = rng.range(0.0, TAU);
        let half = Vec2::new(size * rng.range(0.8, 1.1), size * rng.range(0.6, 0.9));
        let top = g + size * rng.range(0.9, 1.4);
        let color = shades[rng.index(shades.len())] * rng.range(0.9, 1.08);
        self.oboxes.push(OBox {
            solid,
            ..rocks::boulder(at, yaw, half, g - size * 0.4, top, color)
        });
    }

    /// A rock bigger than a tank, often with a second block broken off
    /// against it, and a piece of rubble: cover. It grows bigger only where there's room for it (off the ways
    /// round, the fords, the doors): it stays as asked for otherwise.
    pub(super) fn rock_pile(&mut self, rng: &mut Rng, at: Vec2, size: f32) {
        let size = if self.room(at, size * 1.4 + 1.0) {
            size * 1.25
        } else {
            size
        };
        self.boulder(rng, at, size, true);
        if rng.chance(0.5) {
            let a = rng.range(0.0, TAU);
            let q = at + Vec2::new(a.cos(), a.sin()) * size * rng.range(1.0, 1.3);
            let s = size * rng.range(0.55, 0.75);
            if self.room(q, s * 1.2 + 1.0) {
                self.boulder(rng, q, s, true);
            }
        }
        if rng.chance(0.6) {
            let a = rng.range(0.0, TAU);
            let q = at + Vec2::new(a.cos(), a.sin()) * rng.range(size + 1.0, size + 3.0);
            let s = rng.range(0.4, 0.7);
            self.boulder(rng, q, s, false);
        }
    }

    /// One crystal standing in the ground at `at`: `h` tall, `r` thick,
    /// leaning `lean` toward `dir`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn shard(
        &mut self,
        seed: u64,
        at: Vec3,
        h: f32,
        r: f32,
        dir: Vec2,
        lean: f32,
        lit: bool,
        solid: bool,
    ) {
        let d = dir.normalize_or(Vec2::X);
        let axis = Vec3::new(d.x * lean.sin(), lean.cos(), d.y * lean.sin()) * (h + 1.0);
        self.crystals.push(Crystal {
            base: at - Vec3::Y * 1.0,
            axis,
            radius: r,
            sides: 5 + (seed % 3) as u32,
            seed: (seed >> 8) as u32,
            color: RAW * (0.85 + 0.3 * ((seed >> 40) % 1000) as f32 / 1000.0),
            lit,
            solid,
            raw: true,
        });
    }

    /// An outcrop of seam crystal at ground level: one big shard for cover
    /// and small ones round it.
    pub(super) fn outcrop(&mut self, rng: &mut Rng, p: Vec2, h: f32) {
        if self.in_the_way(p, 1.5) {
            return;
        }
        let g = self.ground_height(p.x, p.y);
        let lean_dir = Vec2::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
        self.shard(
            rng.next_u64(),
            Vec3::new(p.x, g, p.y),
            h,
            1.1,
            lean_dir,
            rng.range(0.05, 0.3),
            true,
            true,
        );
        for _ in 0..4 {
            let a = rng.range(0.0, TAU);
            let q = p + Vec2::new(a.cos(), a.sin()) * rng.range(1.6, 2.8);
            let gq = self.ground_height(q.x, q.y);
            self.shard(
                rng.next_u64(),
                Vec3::new(q.x, gq, q.y),
                rng.range(0.8, 1.8),
                0.35,
                q - p,
                rng.range(0.3, 0.7),
                rng.chance(0.4),
                false,
            );
        }
    }

    /// Where an outcrop of crystal might stand out in the open: a rock
    /// about as big, with rubble round it (on dice of its own).
    pub(super) fn outcrop_rock(&mut self, p: Vec2, h: f32) {
        self.rock_at(p, h);
    }

    /// A rock about `h` high at `p`, maybe a smaller one by it, on dice of
    /// its own.
    pub(super) fn rock_at(&mut self, p: Vec2, h: f32) {
        let rng = &mut own_rng(p);
        let size = (h * 0.55).clamp(1.3, 1.9);
        self.boulder(rng, p, size, true);
        if rng.chance(0.5) {
            let a = rng.range(0.0, TAU);
            let q = p + Vec2::new(a.cos(), a.sin()) * size * rng.range(1.0, 1.3);
            let s = size * rng.range(0.55, 0.75);
            if self.room(q, s * 1.2 + 1.0) {
                self.boulder(rng, q, s, true);
            }
        }
    }

    /// The seam: where it runs along the wash floor (on up the Crag's face,
    /// and through the hall), worked out here: the cut faces either side,
    /// and where it cropped out, the spots for rocks (returned: placed once
    /// the wash's other rocks are, which keep their distance).
    pub(super) fn seam(&mut self, rng: &mut Rng) -> Vec<(Vec2, f32)> {
        let lay = self.l;
        let mut rocks = Vec::new();
        let line = lay.seam.clone();
        // Rock along the seam on the wash floor, cover under the bridge and
        // either side of it.
        for (u, h) in [
            (6.0, 3.0),
            (18.0, 3.6),
            (34.0, 2.6),
            (47.0, 4.2),
            (61.0, 3.2),
        ] {
            let (p, t) = line_at(&line, u);
            let p = p + t.perp() * rng.range(-2.5, 2.5);
            if seg(p, lay.bridge().0, lay.bridge().1).0 < DECK_HALF + 2.0
                || !self.room(p, 2.0)
                || lay
                    .ramps
                    .iter()
                    .any(|&(s, e, hw)| seg(p, s, e + (e - s).normalize() * 4.0).0 < hw + 4.0)
            {
                continue;
            }
            rocks.push((p, h));
        }
        // Cut faces: low steps of dark rock either side of the vein where
        // the rig has cut it back.
        for (u, side) in [(6.0, 1.0), (66.0, -1.0f32)] {
            let (p, t) = line_at(&line, u);
            // Toward the bank, off the way down the floor's middle.
            let (_, half) = lay.wash_profile(lay.wash_near(p).1);
            let p = p + t.perp() * side * (half - 1.8);
            if !self.room(p, 1.0) {
                continue;
            }
            let yaw = (-t.y).atan2(t.x) + rng.range(-0.2, 0.2);
            let g = self.ground_height(p.x, p.y);
            self.stone(p, yaw, Vec2::new(3.2, 1.4), g - 0.5, g + 1.5, STONE_DARK);
        }
        rocks
    }

    /// The Crag's face, sculpted along its foot: down its west side, along
    /// the Face (quarried in benches, the natural rock overhanging the cut),
    /// up its east side and in and out of the box cut's slot. Colliders
    /// along it, lintels over its mouths, crystal breaking out of its veins.
    pub(super) fn crag_face(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let feet = crag_feet(self);
        let mut mesh = std::mem::take(&mut self.sculpted);
        let fronts = crag::sculpt(&mut mesh, &feet, &|q| self.ground_height(q.x, q.y));
        self.sculpted = mesh;
        self.face_colliders(&feet, &fronts);
        // Lintels over the mouths in the Face: rock from the face back to
        // where the tunnel's roof starts.
        for (a, b) in lay.mouths() {
            let xc = (a.x + b.x) * 0.5;
            let z0 = lay.face(xc) - 0.1;
            let mut z1 = lay.face(xc);
            while !lay.roofed(Vec2::new(xc, z1)) {
                z1 += 0.5;
            }
            let z1 = (z1 / 2.0).ceil() * 2.0 + 0.2;
            self.stone(
                Vec2::new(xc, (z0 + z1) * 0.5),
                0.0,
                Vec2::new((b.x - a.x) * 0.5 + 0.6, (z1 - z0) * 0.5),
                ROOF.0,
                ROOF.0 + 14.0,
                STONE_DARK,
            );
        }
        // The seams: crystal growing out of them all the way up.
        let n = feet.len();
        let mut placed = 0;
        for i in (0..n).step_by(2) {
            let f = feet[i];
            for (_, y, width, present) in crag::veins_at(&f) {
                if f.mouth && y < ROOF.0 + 0.5 {
                    continue;
                }
                // Not low on the Face itself: the way along its foot is
                // narrow between the wash and the rock.
                let low_front = f.nrm.y < -0.3 && f.p.y < lay.front_limit() && y < f.floor + 4.0;
                if present > 0.5 && !low_front && rng.chance(0.18 + 0.25 * width) {
                    // Now and then a great mass of it, standing out.
                    let big = rng.chance(0.15) && y > f.floor + 5.0;
                    self.wall_crystals(rng, &f, y, big);
                    placed += 1;
                }
            }
        }
        debug_assert!(placed > 30, "{placed} crystal clusters on the Crag");
    }

    /// Colliders for a sculpted face: one thin slab per stretch of it,
    /// from its most forward point at tank height back into the rock.
    pub(super) fn face_colliders(&mut self, feet: &[crag::Foot], fronts: &[Option<Vec2>]) {
        for k in (0..feet.len() - 1).step_by(2) {
            let j = (k + 2).min(feet.len() - 1);
            let (Some(a), Some(b)) = (fronts[k], fronts[j]) else {
                continue;
            };
            let along = b - a;
            let len = along.length();
            if len < 1e-3 || len > 4.0 {
                continue;
            }
            let nrm = (feet[k].nrm + feet[j].nrm).normalize_or(feet[k].nrm);
            let front = (a - feet[k].p).dot(nrm).max((b - feet[j].p).dot(nrm));
            let mid = (feet[k].p + feet[j].p) * 0.5 + nrm * front;
            let depth = 3.0;
            let dir = along / len;
            let floor = feet[k].floor.min(feet[j].floor);
            self.oboxes.push(OBox {
                center: mid - nrm * depth * 0.5,
                yaw: (-dir.y).atan2(dir.x),
                half: Vec2::new(len * 0.5 + 0.35, depth * 0.5),
                y0: floor - 3.0,
                y1: floor + 10.0,
                hidden: true,
                ..OBox::default()
            });
        }
    }

    /// The Face out from under the Crag, sculpted as one rock face like the
    /// Crag's: the mesa's cliff, its lip at the mesa's top, hanging down to
    /// a lintel over the west adit and the breach, petering out where the
    /// mesa slopes down at its west end and into the scree.
    pub(super) fn mesa_face(&mut self) {
        let lay = self.l;
        let mouths = [lay.caves[3], lay.caves[5]];
        // The stretches: west of the Crag, between it and the scree, and
        // east of the scree.
        let spans = [
            (-6.0, lay.crag_west(lay.face(lay.crag_w)) + 1.0),
            (lay.crag_east(lay.face(lay.crag_e)) - 1.0, lay.scree.0 + 4.0),
            (lay.scree.1 - 4.0, lay.size + 6.0),
        ];
        let mut mesh = std::mem::take(&mut self.sculpted);
        let mut all_feet = Vec::new();
        let mut all_fronts = Vec::new();
        for (x0, x1) in spans {
            let mut corners = Vec::new();
            let mut x = x0;
            while x < x1 {
                corners.push(Vec2::new(x, lay.face(x) + 1.2));
                x += 2.0;
            }
            corners.push(Vec2::new(x1, lay.face(x1) + 1.2));
            let pts = crag::resample(&corners, 0.7, 1.2);
            let mut feet = Vec::with_capacity(pts.len());
            let mut along = x0 * 1.7 + 300.0;
            for (i, &(p, dir)) in pts.iter().enumerate() {
                if i > 0 {
                    along += p.distance(pts[i - 1].0);
                }
                let nrm = Vec2::new(dir.y, -dir.x);
                // The rim is at the mesa's top behind it. Where the Face
                // gives way to the scree only part of the rise is cliff
                // (the rest is the scree's slope, up to the same top): the
                // face is only that part, its foot up on the slope.
                // (Where the mesa slopes down at its west end the dunes run
                // up over it: the rim's at the ground behind, not under it.)
                let top = [1.0, 2.5, 4.0]
                    .iter()
                    .map(|&d| lay.uncut(Vec2::new(p.x, lay.face(p.x) + FACE_RISE + d)))
                    .fold(
                        TOP * lay.mesa(Vec2::new(p.x, lay.face(p.x) + FACE_RISE + 1.0)),
                        f32::max,
                    );
                let cliff = top * (1.0 - lay.scree(p.x));
                // Ragged, but never below the top: a rim sunk under the
                // ground behind it leaves the cap over it buried, poking
                // out in patches (seen from the scree, #290).
                // The foot on the highest of the ground just out from it.
                let mut base = [1.0, 1.5, 2.0, 2.5]
                    .iter()
                    .map(|&k| self.level(p + nrm * k))
                    .fold(f32::NEG_INFINITY, f32::max);
                if cliff < top {
                    base = base.max(top - cliff);
                }
                let lip = top + 0.6 * noise(along / 6.0, 31) * smoothstep(0.3, 3.0, top - base);
                let mouth = mouths.iter().any(|(a, b)| p.x > a.x && p.x < b.x);
                let by_mouth = mouths
                    .iter()
                    .map(|(a, b)| (a.x - p.x).max(p.x - b.x).max(0.0))
                    .fold(f32::INFINITY, f32::min);
                // Low where the mesa slopes away: no face at all.
                if top - base < 0.3 && !mouth {
                    if feet.len() > 1 {
                        all_fronts.extend(crag::sculpt(&mut mesh, &feet, &|q| {
                            self.ground_height(q.x, q.y)
                        }));
                        all_feet.append(&mut feet);
                    }
                    feet.clear();
                    continue;
                }
                feet.push(crag::Foot {
                    p,
                    nrm,
                    along,
                    floor: if mouth { ROOF.0 } else { base },
                    lip,
                    cut: -100.0,
                    mouth,
                    relief: 0.8 * smoothstep(0.5, 5.0, by_mouth),
                });
            }
            if feet.len() > 1 {
                all_fronts.extend(crag::sculpt(&mut mesh, &feet, &|q| {
                    self.ground_height(q.x, q.y)
                }));
                all_feet.append(&mut feet);
            }
        }
        self.sculpted = mesh;
        self.face_colliders(&all_feet, &all_fronts);
    }

    /// The Crag's top over its tunnels: drawn as the ground there would be,
    /// over the roofs, so the top reads as one stretch of rock (the roofs
    /// stay for colliding, just under it).
    pub(super) fn crag_lid(&mut self) {
        let lay = self.l;
        let cell = 2.0;
        let lid = |p: Vec2| lay.crag_depth(p) > 3.5 && !within(lay.box_cut, p, 2.5);
        let m = &mut self.sculpted;
        let up = Vec3::Y.to_array();
        let mut iz = 0;
        while (iz as f32) * cell < lay.size + 6.0 {
            for ix in 0..(lay.size / cell) as usize {
                let a = Vec2::new(ix as f32 * cell, iz as f32 * cell);
                let corners = [
                    a,
                    a + Vec2::new(cell, 0.0),
                    a + Vec2::new(cell, cell),
                    a + Vec2::new(0.0, cell),
                ];
                if !corners.iter().all(|&c| lid(c))
                    || !corners.iter().any(|&c| lay.cave_distance(c) < 4.5)
                {
                    continue;
                }
                let base = m.verts.len() as u32;
                for c in corners {
                    let color = paint(lay, &lay.seam, c.x, c.y, CRAG_TOP, 0.0);
                    m.verts.push(crate::render::mesh::Vertex {
                        pos: [c.x, CRAG_TOP + 0.03, c.y],
                        normal: up,
                        color: color.extend(Material::Sand.id()).to_array(),
                    });
                }
                m.indices
                    .extend([base, base + 2, base + 1, base, base + 3, base + 2]);
            }
            iz += 1;
        }
    }

    /// A cluster of crystal growing out of the Crag's face at `f`, height
    /// `y`: bigger low down, where a tank can hide behind it.
    pub(super) fn wall_crystals(&mut self, rng: &mut Rng, f: &crag::Foot, y: f32, big: bool) {
        let low = y < f.floor + 3.0;
        let count = if big {
            4 + rng.index(3)
        } else {
            1 + rng.index(3)
        };
        for k in 0..count {
            let dy = if k == 0 { 0.0 } else { rng.range(-0.8, 0.8) };
            let side = f.nrm.perp() * rng.range(-1.0, 1.0) * k as f32;
            let yy = (y + dy).max(f.floor + 0.3);
            let o = crag::out(f, yy);
            let at = f.p + f.nrm * o + side;
            // Out of the rock and up, turned a little sideways.
            let rise = rng.range(0.4, 1.4);
            let dir = Vec3::new(f.nrm.x, rise, f.nrm.y) + Vec3::new(side.x, 0.0, side.y) * 0.3;
            let dir = dir.normalize();
            let (h, r) = if big {
                let s = if k == 0 { 1.0 } else { rng.range(0.45, 0.8) };
                (rng.range(4.5, 7.0) * s, rng.range(0.9, 1.4) * s)
            } else if low && k == 0 {
                (rng.range(2.4, 3.6), rng.range(0.6, 0.9))
            } else {
                (
                    rng.range(1.4, 3.6) * (1.0 - 0.3 * k as f32 / 3.0),
                    rng.range(0.28, 0.6),
                )
            };
            self.crystals.push(Crystal {
                base: Vec3::new(at.x, yy, at.y) - dir * 0.7,
                axis: dir * (h + 0.7),
                radius: r,
                sides: 5 + rng.index(3) as u32,
                seed: rng.next_u64() as u32,
                color: RAW * rng.range(0.85, 1.15),
                lit: rng.chance(0.65),
                solid: low && k == 0,
                raw: true,
            });
        }
    }

    /// Steel sets holding up the tunnels' roofs: a post up each wall and a
    /// cap across under the arch, braced at its corners, a lamp under it;
    /// every 8 m, where both walls are rock (never across a way in).
    pub(super) fn mine_supports(&mut self) {
        let lay = self.l;
        let runs = [
            (lay.adit, true, 4.0),
            (lay.caves[3], true, 4.0),
            (lay.caves[4], false, 6.0),
            (lay.heading, true, 4.0),
        ];
        for ((a, b), along_z, first) in runs {
            let (lo, hi) = if along_z { (a.y, b.y) } else { (a.x, b.x) };
            let mut t = lo + first;
            while t < hi - 2.0 {
                let (p0, p1, out) = if along_z {
                    (Vec2::new(a.x, t), Vec2::new(b.x, t), Vec2::X)
                } else {
                    (Vec2::new(t, a.y), Vec2::new(t, b.y), Vec2::Y)
                };
                let walls = lay.in_rock(p0 - out * 1.2)
                    && lay.in_rock(p1 + out * 1.2)
                    && lay.in_rock(p0 - out * 1.2 + out.perp() * 1.0)
                    && lay.in_rock(p0 - out * 1.2 - out.perp() * 1.0)
                    && lay.in_rock(p1 + out * 1.2 + out.perp() * 1.0)
                    && lay.in_rock(p1 + out * 1.2 - out.perp() * 1.0);
                if walls && lay.roofed(p0) && lay.roofed(p1) {
                    self.steel_set(p0 + out * 0.3, p1 - out * 0.3);
                }
                t += 8.0;
            }
        }
    }

    /// One steel set, its posts at `a` and `b` (against the walls).
    pub(super) fn steel_set(&mut self, a: Vec2, b: Vec2) {
        let lay = self.l;
        let mid = (a + b) * 0.5;
        let h = lay.drawn_roof(mid) - arch(lay, a).max(arch(lay, b)) - 0.1;
        let across = (b - a).normalize();
        for p in [a, b] {
            self.pipe(
                Vec3::new(p.x, -0.2, p.y),
                Vec3::new(p.x, h, p.y),
                0.26,
                STEEL,
                Material::Metal,
                true,
            );
        }
        // The cap, into the rock at each end (over every tank: it's never
        // in the way).
        let w = a.distance(b) * 0.5 + 0.8;
        self.oboxes.push(OBox {
            center: mid,
            yaw: yaw_along(across.perp()),
            half: Vec2::new(w, 0.22),
            y0: h - 0.05,
            y1: h + 0.45,
            color: STEEL * 0.9,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        // Knee braces.
        for (p, s) in [(a, 1.0), (b, -1.0)] {
            self.segment(
                Vec3::new(p.x, h - 1.3, p.y),
                Vec3::new(p.x + across.x * s * 1.3, h, p.y + across.y * s * 1.3),
                0.1,
                STEEL,
                Material::Metal,
                false,
            );
        }
    }

    /// The mine's gear along its tunnels: the vent duct down the adit, cable
    /// trays along the drift, cut crystal crated in the crosscut.
    pub(super) fn mine_gear(&mut self) {
        let lay = self.l;
        // The duct, high up the adit's west wall, in sections.
        let x = lay.adit.0.x + 0.95;
        let mut z = 158.0;
        while z < 208.0 {
            let (a, b) = (Vec3::new(x, 5.0, z), Vec3::new(x, 5.0, z + 3.9));
            self.pipe(a, b, 0.42, RUST, Material::Rust, false);
            self.pipe(
                b - Vec3::Z * 0.12,
                b + Vec3::Z * 0.1,
                0.5,
                STEEL,
                Material::Metal,
                false,
            );
            z += 4.0;
        }
        // Cable trays along the drift's north wall, plasma conduit in them.
        let (a, b) = lay.caves[4];
        let zt = b.y - 0.45;
        self.oboxes.push(OBox {
            center: Vec2::new((a.x + b.x) * 0.5, zt),
            half: Vec2::new((b.x - a.x) * 0.5 - 1.0, 0.25),
            y0: 3.9,
            y1: 4.0,
            color: STEEL,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: Vec2::new((a.x + b.x) * 0.5, zt),
            half: Vec2::new((b.x - a.x) * 0.5 - 1.0, 0.07),
            y0: 4.0,
            y1: 4.12,
            color: STEEL * 0.4,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        // Cut crystal crated in the crosscut, waiting for the train.
        let (a, b) = lay.crosscut;
        self.block_stack(Vec2::new(b.x - 1.6, b.y - 1.8), 0.1);
        self.block_stack(Vec2::new(a.x + 1.2, a.y + 1.6), -0.15);
    }

    /// A column of steel from the floor to the roof at `top`, its foot and
    /// head spread on plates, a crystal collar lit half way up.
    pub(super) fn prop_column(&mut self, at: Vec2, top: f32) {
        let foot = Vec3::new(at.x, 0.0, at.y);
        let head = Vec3::new(at.x, top + 0.3, at.y);
        self.pipe(
            foot - Vec3::Y * 0.3,
            head,
            0.42,
            STEEL,
            Material::Metal,
            true,
        );
        for (y0, y1) in [(-0.1, 0.25), (top - 0.35, top + 0.3)] {
            self.oboxes.push(OBox {
                center: at,
                half: Vec2::splat(0.85),
                y0,
                y1,
                color: STEEL * 0.8,
                side_mat: Material::Metal,
                top_mat: Material::Metal,
                ..OBox::default()
            });
        }
        self.pipe(
            foot + Vec3::Y * 3.0,
            foot + Vec3::Y * 3.4,
            0.52,
            STEEL * 0.8,
            Material::Metal,
            false,
        );
    }

    /// A floodlight on a steel stand at `at`, its lamp turned toward `look`.
    pub(super) fn floodlight(&mut self, at: Vec2, look: Vec2) {
        let base = Vec3::new(at.x, 0.0, at.y);
        let top = base + Vec3::Y * 4.6;
        for k in 0..3 {
            let a = k as f32 * TAU / 3.0;
            let leg = base + Vec3::new(a.cos(), 0.0, a.sin()) * 0.9;
            self.segment(
                leg,
                top - Vec3::Y * 1.2,
                0.07,
                STEEL,
                Material::Metal,
                false,
            );
        }
        self.pipe(
            base + Vec3::Y * 1.0,
            top,
            0.09,
            STEEL,
            Material::Metal,
            false,
        );
        let d = (look - at).normalize_or(Vec2::X);
        let rig = Rig {
            pos: top,
            rot: Quat::from_rotation_y(yaw_along(d)) * Quat::from_rotation_x(0.35),
        };
        rig.part(
            self,
            Vec3::ZERO,
            Vec3::new(0.7, 0.4, 0.3),
            Quat::IDENTITY,
            STEEL * 0.7,
            Material::Metal,
            false,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, 0.0, 0.31),
            Vec3::new(0.6, 0.3, 0.02),
            Quat::IDENTITY,
            Vec3::new(1.0, 0.85, 0.95),
            Material::Glow,
            false,
            0.0,
        );
    }

    /// Broken natural rock along the rest of the Face, so it reads as stone,
    /// not sand: tall blocks against the cliff, their tops ragged.
    pub(super) fn rough_face(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let mut x = -4.0;
        while x < lay.size + 4.0 {
            let w = rng.range(2.0, 3.4);
            let c = x + w;
            x += w * 2.0 - 0.5;
            if (lay.crag_west(lay.face(c)) + 1.0..lay.crag_east(lay.face(c)) - 1.0).contains(&c)
                || lay.scree(c) > 0.3
            {
                continue;
            }
            if lay.caves.iter().any(|(a, b)| {
                c + w > a.x - 0.8 && c - w < b.x + 0.8 && a.y < lay.face(c) + FACE_RISE
            }) {
                continue;
            }
            let f = lay.face(c);
            let full = lay.mesa(Vec2::new(c, f + FACE_RISE + 1.0));
            if full < 0.15 {
                continue;
            }
            let p = Vec2::new(c, f + 1.4 + rng.range(-0.3, 0.6));
            let top = TOP * full + rng.range(-1.8, 0.6);
            let slant = (lay.face(c + 1.0) - lay.face(c - 1.0)) * 0.5;
            self.stone(
                p,
                -slant.atan() + rng.range(-0.1, 0.1),
                Vec2::new(w, rng.range(1.6, 2.4)),
                -0.6,
                top,
                [STONE, STONE_DARK, STONE_PALE][rng.index(3)] * rng.range(0.85, 1.05),
            );
            // Only a stand-in, so what's placed round it after stays where
            // it was: the Face is sculpted (`mesa_face`), and these blocks
            // that used to stand along it go once the map is laid out
            // (`clear_face_blocks`).
            if let Some(o) = self.oboxes.last_mut() {
                o.hidden = true;
                o.color = FACE_BLOCK;
            }
            // Blocks fallen out of it.
            if rng.chance(0.45) {
                let q = p - Vec2::new(rng.range(-2.0, 2.0), rng.range(3.5, 6.0));
                if lay.wash(q) > -0.3 && !self.on_way(q) {
                    let s = rng.range(1.0, 2.0);
                    self.boulder(rng, q, s, s > 1.4);
                }
            }
        }
    }

    /// The scree where the Face fell in: rock strewn down the slope, big
    /// blocks to hide behind on the climb.
    pub(super) fn scree_rocks(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let mut placed = 0;
        for _ in 0..400 {
            if placed >= 34 {
                break;
            }
            let x = rng.range(lay.scree.0 + 2.0, lay.scree.1 - 2.0);
            let f = lay.face(x);
            let z = rng.range(f - SCREE_RUN + 2.0, f + FACE_RISE);
            let p = Vec2::new(x, z);
            // A worn way up the middle stays clear.
            if lay.ways_up.iter().any(|&(a, b)| seg(p, a, b).0 < 8.0)
                || lay.in_cave(p)
                || lay.wash(p) < -0.3
                || self
                    .oboxes
                    .iter()
                    .any(|o| o.solid && o.center.distance(p) < 6.0)
            {
                continue;
            }
            let big = placed % 3 != 2;
            let s = if big {
                rng.range(1.8, 2.8)
            } else {
                rng.range(0.7, 1.2)
            };
            self.boulder(rng, p, s, big);
            placed += 1;
        }
        // The broken lip at the top: blocks tipped over the edge.
        for k in 0..5 {
            let x = lay.scree.0 + 5.0 + k as f32 * 5.5 + rng.range(-1.0, 1.0);
            if (x - lay.ways_up[1].0.x).abs() < 8.0 {
                continue;
            }
            let p = Vec2::new(x, lay.face(x) + FACE_RISE + rng.range(0.0, 2.0));
            let g = self.ground_height(p.x, p.y);
            let yaw = rng.range(-0.4, 0.4);
            let half = Vec2::new(rng.range(1.2, 2.0), rng.range(0.9, 1.4));
            let top = g + rng.range(1.0, 2.0);
            let tilt = rng.range(-0.3, 0.3);
            let color = STONE * rng.range(0.85, 1.05);
            self.oboxes.push(OBox {
                tilt,
                ..rocks::boulder(p, yaw, half, g - 1.0, top, color)
            });
        }
    }

    /// The mine: rock walls round every corridor, roofs flush with the
    /// mesa, plasma lamps, crystal veins in the walls, timber portals at the
    /// Ithessar's mouths, ore carts.
    pub(super) fn mine_caves(&mut self, rng: &mut Rng) {
        self.cave_walls(rng);
        self.roof_slabs();
        self.cave_dressing(rng);
    }

    /// Walls round every corridor, hidden: the lining's drawn in their
    /// place (`cave_lining`).
    pub(super) fn cave_walls(&mut self, rng: &mut Rng) {
        let lay = self.l;
        // Walls: along every edge of every cave, where the other side is
        // rock (not another cave, the box cut, or open air past the Face).
        let step = 2.0;
        for &(a, b) in &lay.caves {
            let edges = [
                (Vec2::new(a.x, a.y), Vec2::new(b.x, a.y), -Vec2::Y),
                (Vec2::new(a.x, b.y), Vec2::new(b.x, b.y), Vec2::Y),
                (Vec2::new(a.x, a.y), Vec2::new(a.x, b.y), -Vec2::X),
                (Vec2::new(b.x, a.y), Vec2::new(b.x, b.y), Vec2::X),
            ];
            for (e0, e1, out) in edges {
                let len = e0.distance(e1);
                let along = (e1 - e0) / len;
                let n = (len / step).round() as usize;
                let mut start: Option<f32> = None;
                for k in 0..=n {
                    let t = k as f32 * step;
                    let closed = k < n && {
                        let p = e0 + along * (t + step * 0.5) + out * 1.0;
                        lay.in_rock(p) && (0.0..lay.size).contains(&p.y)
                    };
                    match (closed, start) {
                        (true, None) => start = Some(t),
                        (false, Some(s)) => {
                            self.cave_wall(rng, e0 + along * s, e0 + along * t, out);
                            start = None;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// The caves' roofs, flush with the mesa (hidden out on the mesa, where
    /// the roof is drawn as ground: `roof_lid`).
    pub(super) fn roof_slabs(&mut self) {
        let lay = self.l;
        let (lo, hi) = lay.mine_bounds();
        let cell = 2.0;
        // Roofs: the 2 m cells within the walls' reach of a cave, back from
        // the Face's lip, gathered into slabs that don't overlap.
        // Each run is (from, to, (under the Crag, underside)).
        let mut rows: Vec<(f32, Vec<RoofRun>)> = Vec::new();
        let mut z = lo.y;
        while z < lay.size + 6.0 {
            let mut runs = Vec::new();
            let mut x = lo.x;
            let mut start: Option<(f32, (bool, f32))> = None;
            while x <= hi.x {
                let c = Vec2::new(x + cell * 0.5, z + cell * 0.5);
                let on =
                    (x < hi.x && lay.roofed(c)).then(|| (lay.under_crag(c), lay.slab_under(c)));
                match (on, start) {
                    (Some(k), None) => start = Some((x, k)),
                    (on, Some((s, k))) if on != Some(k) => {
                        runs.push((s, x, k));
                        start = on.map(|k| (x, k));
                    }
                    _ => {}
                }
                x += cell;
            }
            rows.push((z, runs));
            z += cell;
        }
        // Merge rows with the same runs into slabs.
        let mut open: Vec<(RoofRun, f32)> = Vec::new();
        let mut slabs: Vec<(Vec2, Vec2, (bool, f32))> = Vec::new();
        for (z, runs) in rows
            .iter()
            .chain(std::iter::once(&(lay.size + 6.0, Vec::new())))
        {
            let mut next = Vec::new();
            for &(run, z0) in &open {
                if runs.contains(&run) {
                    next.push((run, z0));
                } else {
                    slabs.push((Vec2::new(run.0, z0), Vec2::new(run.1, *z), run.2));
                }
            }
            for &run in runs {
                if !next.iter().any(|&(r, _)| r == run) {
                    next.push((run, *z));
                }
            }
            open = next;
        }
        for (a, b, (tall, under)) in slabs {
            self.oboxes.push(OBox {
                center: (a + b) * 0.5,
                half: (b - a) * 0.5,
                y0: under,
                y1: if tall { CRAG_ROOF } else { ROOF.1 },
                color: MESA_TOP,
                side_mat: Material::Rubble,
                top_mat: Material::Sand,
                ramp: true,
                // Out on the mesa the roof is drawn as ground (`roof_lid`):
                // a slab there stood out as a flat rectangle with straight
                // sides, stepping out over the Face's lip. Under the Crag
                // its face and lid hide the slab.
                hidden: !tall,
                ..OBox::default()
            });
        }
    }

    /// The caves dressed: the lid and lining drawn over their boxes, lamps
    /// and crystal veins on the walls, timber portals at the mouths in the
    /// Face, ore carts.
    pub(super) fn cave_dressing(&mut self, rng: &mut Rng) {
        let lay = self.l;
        self.roof_lid();
        self.cave_lining();
        // Lamps: plasma in a crystal cage on the walls every 12 m (the
        // Resonant's breach is lit their own way), and veins of raw crystal
        // breaking out of the walls.
        for &(a, b) in lay.caves[..5].iter().chain(&lay.caves[6..]) {
            let long_z = b.y - a.y > b.x - a.x;
            let len = if long_z { b.y - a.y } else { b.x - a.x };
            let n = (len / 12.0) as usize;
            for k in 0..n {
                let t = (k as f32 + 0.5) / n as f32;
                for side in [0.0f32, 1.0] {
                    let (p, out) = if long_z {
                        let x = a.x + (b.x - a.x) * side;
                        (
                            Vec2::new(x, a.y + (b.y - a.y) * t),
                            Vec2::new(side * 2.0 - 1.0, 0.0),
                        )
                    } else {
                        let z = a.y + (b.y - a.y) * side;
                        (
                            Vec2::new(a.x + (b.x - a.x) * t, z),
                            Vec2::new(0.0, side * 2.0 - 1.0),
                        )
                    };
                    if !lay.in_rock(p + out * 1.0) || !lay.roofed(p) {
                        continue;
                    }
                    if (k + side as usize).is_multiple_of(2) {
                        self.lamp(p - out * 0.15, -out);
                    } else {
                        // A vein: shards growing out of the wall, low.
                        for _ in 0..3 {
                            let q = p + Vec2::new(rng.range(-1.5, 1.5), rng.range(-1.5, 1.5))
                                * out.perp().abs()
                                - out * 0.2;
                            let h = rng.range(0.6, 1.6);
                            self.shard(
                                rng.next_u64(),
                                Vec3::new(q.x, rng.range(0.5, 3.0), q.y),
                                h,
                                0.25,
                                -out,
                                rng.range(0.9, 1.3),
                                true,
                                false,
                            );
                        }
                    }
                }
            }
        }
        // Timber portals at the Ithessar's mouths in the Face.
        for (a, b) in [lay.caves[0], lay.caves[3], lay.heading] {
            let xc = (a.x + b.x) * 0.5;
            let mut z = lay.face(xc);
            while !lay.roofed(Vec2::new(xc, z)) {
                z += 0.5;
            }
            let z = z + 0.6;
            // Standing a little proud of the cave walls, not flush.
            for x in [a.x - 0.25, b.x + 0.25] {
                self.oboxes.push(OBox {
                    center: Vec2::new(x, z),
                    half: Vec2::splat(0.35),
                    y0: -0.3,
                    y1: ROOF.0,
                    color: TIMBER,
                    side_mat: Material::Wood,
                    top_mat: Material::Wood,
                    ..OBox::default()
                });
            }
            self.oboxes.push(OBox {
                center: Vec2::new(xc, z),
                half: Vec2::new((b.x - a.x) * 0.5 + 0.4, 0.4),
                y0: ROOF.0 - 0.7,
                y1: ROOF.0 + 0.02,
                color: TIMBER * 0.9,
                side_mat: Material::Wood,
                top_mat: Material::Wood,
                ..OBox::default()
            });
        }
        // Ore carts: on the drift, and in the west adit.
        for &(x, z, yaw) in &[
            (lay.drift.0.x + 12.0, lay.drift.0.y + 1.6, FRAC_PI_2),
            (lay.drift.1.x - 8.0, lay.drift.1.y - 1.6, FRAC_PI_2 + 0.1),
            (lay.west_adit.0.x + 1.6, lay.hall.0.y - 12.0, 0.0),
        ] {
            self.ore_cart(rng, Vec2::new(x, z), yaw);
        }
    }

    /// The cave roofs out on the mesa, drawn as ground over their hidden
    /// slabs: level with the rock round them and painted like it, their
    /// edges tucked under the terrain where they meet it, drooping with the
    /// Face's lip instead of stepping out over it. Rock below, down to the
    /// ceiling the caves see.
    pub(super) fn roof_lid(&mut self) {
        let lay = self.l;
        let cell = 2.0;
        let (origin, hi) = lay.mine_bounds();
        let (nx, nz) = (
            ((hi.x - origin.x) / cell).round() as i32,
            ((hi.y - origin.y) / cell).round() as i32,
        );
        let at = |ix: i32, iz: i32| origin + Vec2::new(ix as f32, iz as f32) * cell;
        // Over the mouths it runs on down the Face to the roof's underside,
        // so the mouth's top is the cliff, not a slab's 2 m steps.
        let lidded = |ix: i32, iz: i32| {
            (0..nx).contains(&ix) && (0..nz).contains(&iz) && {
                let a = at(ix, iz);
                let c = a + Vec2::splat(cell * 0.5);
                let over_mouth = lay.cave_distance(c) < CAVE_WALL + 0.5
                    && !within(lay.box_cut, c, 0.0)
                    && [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
                        .iter()
                        .any(|&(u, v)| lay.uncut(a + Vec2::new(u, v) * cell) > ROOF.0 + 0.2);
                (lay.roofed(c) || over_mouth) && !lay.under_crag(c)
            }
        };
        // The rock's top at a corner, as if no cave were under it.
        let floor = ROOF.0;
        let height = |ix: i32, iz: i32| {
            let p = at(ix, iz);
            let rock = lay.uncut(p).clamp(floor, ROOF.1);
            let inner = lidded(ix - 1, iz - 1)
                && lidded(ix, iz - 1)
                && lidded(ix - 1, iz)
                && lidded(ix, iz);
            let g = lay.ground(p.x, p.y);
            if !inner && g > rock - 0.3 {
                // Meeting the terrain: on its vertex (the grids line up),
                // so there's no seam.
                g
            } else {
                rock
            }
        };
        let point = |ix: i32, iz: i32| {
            let p = at(ix, iz);
            Vec3::new(p.x, height(ix, iz), p.y)
        };
        // Shaded like the ground round it (see `build_mine`).
        let normal = |ix: i32, iz: i32| {
            let p = at(ix, iz);
            let (dx, dz) = (Vec2::new(cell, 0.0), Vec2::new(0.0, cell));
            let h = |q: Vec2| lay.uncut(q).clamp(floor, ROOF.1);
            Vec3::new(h(p - dx) - h(p + dx), 2.0 * cell, h(p - dz) - h(p + dz)).normalize()
        };
        let m = &mut self.sculpted;
        // A convex polygon, its corners in order round it, wound to face
        // along `n`.
        // Leaving out the triangles `skip` picks.
        let mut poly = |q: &[Vec3],
                        n: &[Vec3],
                        c: &[Vec3],
                        mat: Material,
                        skip: &dyn Fn([Vec3; 3]) -> bool| {
            if q.len() < 3 {
                return;
            }
            let base = m.verts.len() as u32;
            for k in 0..q.len() {
                m.verts.push(crate::render::mesh::Vertex {
                    pos: q[k].to_array(),
                    normal: n[k].to_array(),
                    color: c[k].extend(mat.id()).to_array(),
                });
            }
            let flip = (q[1] - q[0]).cross(q[2] - q[0]).dot(n[0]) < 0.0;
            for k in 1..q.len() as u32 - 1 {
                if skip([q[0], q[k as usize], q[k as usize + 1]]) {
                    continue;
                }
                let (b, c) = if flip { (k + 1, k) } else { (k, k + 1) };
                m.indices.extend([base, base + b, base + c]);
            }
        };
        let rock = |p: Vec3, k: f32| {
            STONE.lerp(STONE_DARK, k) * (1.0 + 0.06 * (0.37 * p.x + 0.8 * (0.11 * p.z).sin()).sin())
        };
        // How far a corner's rock stands over the roof's underside: the lid
        // is cut off where that's 0, on the Face over a mouth.
        let over = |ix: i32, iz: i32| lay.uncut(at(ix, iz)) - ROOF.0;
        let cross = |a: Vec3, b: Vec3, fa: f32, fb: f32| {
            let t = fa / (fa - fb);
            let p = a.lerp(b, t);
            Vec3::new(p.x, ROOF.0, p.z)
        };
        for iz in 0..nz {
            for ix in 0..nx {
                if !lidded(ix, iz) {
                    continue;
                }
                let ids = [(ix, iz), (ix + 1, iz), (ix + 1, iz + 1), (ix, iz + 1)];
                let q = ids.map(|(i, j)| point(i, j));
                let n = ids.map(|(i, j)| normal(i, j));
                let f = ids.map(|(i, j)| over(i, j));
                // The cell, cut back to where the rock stands over the
                // underside.
                let (mut top, mut norms) = (Vec::new(), Vec::new());
                for k in 0..4 {
                    let l = (k + 1) % 4;
                    if f[k] >= 0.0 {
                        top.push(q[k]);
                        norms.push(n[k]);
                    }
                    if (f[k] >= 0.0) != (f[l] >= 0.0) {
                        top.push(cross(q[k], q[l], f[k], f[l]));
                        norms.push(n[k].lerp(n[l], 0.5).normalize());
                    }
                }
                let colors: Vec<Vec3> = top
                    .iter()
                    .zip(&norms)
                    .map(|(p, n)| {
                        paint(
                            lay,
                            &lay.seam,
                            p.x,
                            p.z,
                            p.y,
                            Vec2::new(n.x, n.z).length() / n.y,
                        )
                    })
                    .collect();
                // Where three of the cell's corners are on the terrain and
                // take in the diagonal it's split along, that triangle is
                // the terrain's own: drawing it again would z-fight it (on
                // the Crag's cliff over the bore).
                let on_ground = |p: Vec3| {
                    q.iter().any(|&c| c == p) && (p.y - lay.ground(p.x, p.z)).abs() < 1e-4
                };
                let terrains = |t: [Vec3; 3]| {
                    t.iter().all(|&p| on_ground(p)) && t.contains(&q[0]) && t.contains(&q[2])
                };
                poly(&top, &norms, &colors, Material::Sand, &terrains);
                // The cave's ceiling.
                let under: Vec<Vec3> = top.iter().map(|p| Vec3::new(p.x, ROOF.0, p.z)).collect();
                let shade: Vec<Vec3> = under.iter().map(|&p| rock(p, 0.6)).collect();
                poly(
                    &under,
                    &vec![Vec3::NEG_Y; under.len()],
                    &shade,
                    Material::Rubble,
                    &|_| false,
                );
                // Rock faces where the lid ends (in the terrain, or over a
                // mouth back from the Face).
                for (k, (di, dj)) in [(0, -1), (1, 0), (0, 1), (-1, 0)].into_iter().enumerate() {
                    let l = (k + 1) % 4;
                    if lidded(ix + di, iz + dj) || (f[k] < 0.0 && f[l] < 0.0) {
                        continue;
                    }
                    let a = if f[k] >= 0.0 {
                        q[k]
                    } else {
                        cross(q[k], q[l], f[k], f[l])
                    };
                    let b = if f[l] >= 0.0 {
                        q[l]
                    } else {
                        cross(q[k], q[l], f[k], f[l])
                    };
                    if a.y.max(b.y) < ROOF.0 + 0.01 {
                        continue;
                    }
                    let side = [
                        a,
                        b,
                        Vec3::new(b.x, ROOF.0, b.z),
                        Vec3::new(a.x, ROOF.0, a.z),
                    ];
                    let out = Vec3::new(di as f32, 0.0, dj as f32);
                    let shade = side.map(|p| rock(p, 0.2));
                    poly(&side, &[out; 4], &shade, Material::Rubble, &|_| false);
                }
            }
        }
    }

    /// The mine's inside drawn as smooth rock over its boxes: walls that
    /// wander in and out, arching over into the roof (see `vault`).
    pub(super) fn cave_lining(&mut self) {
        let lay = self.l;
        let wall_here = |c: Vec2, n: Vec2| {
            let back = c + n;
            lay.cave_distance(c) < 1.0
                && lay.in_rock(back)
                && back.y < lay.size
                && !within(lay.box_cut, c, 1.0)
        };
        let cave = vault::Cave {
            sdf: &|p| open_sdf(lay, p),
            roof: &|p| lay.drawn_roof(p),
            arch: &|c| arch(lay, c),
            wall_here: &wall_here,
            roof_here: &|p| lay.roofed(p),
            shade: &|p, n| rock_shade(lay, p, n),
            veins: &|c| rock_veins(lay, c),
            vein_color: RAW,
        };
        let (lo, hi) = lay.mine_bounds();
        vault::line(
            &mut self.sculpted,
            &cave,
            lo,
            Vec2::new(hi.x + 2.0, lay.size + 2.0),
        );
    }

    /// A pillar of rock left standing to hold a roof up, at `at`, `r`
    /// across, up to the roof at `top`: round, flaring into the floor and
    /// the roof, drawn smooth over an eight-sided collider.
    pub(super) fn rock_pillar(&mut self, at: Vec2, r: f32, top: f32) {
        let lay = self.l;
        // Four bars, 45 degrees apart, make the eight sides.
        for k in 0..4 {
            self.oboxes.push(OBox {
                center: at,
                yaw: k as f32 * PI * 0.25,
                half: Vec2::new(r, r * (PI / 8.0).tan()),
                y0: -0.5,
                y1: top + 0.2,
                hidden: true,
                ..OBox::default()
            });
        }
        let seed = (at.x * 7.0 + at.y * 13.0) as u32;
        let radius = |a: f32, y: f32| {
            let flare = smoothstep(1.2, -0.4, y) * 0.12 + smoothstep(top - 2.4, top + 0.2, y) * 0.8;
            r * (1.09 + flare)
                + 0.18 * noise2(a * 1.3, y / 1.7, seed)
                + 0.06 * noise2(a * 4.0, y / 0.6, seed + 1)
        };
        let point = |a: f32, y: f32| {
            let rr = radius(a, y);
            Vec3::new(at.x + a.cos() * rr, y, at.y + a.sin() * rr)
        };
        let (sides, rows) = (28usize, 16usize);
        let y_at = |k: f32| -0.5 + (top + 0.4) * k / rows as f32;
        let m = &mut self.sculpted;
        let base = m.verts.len() as u32;
        for j in 0..=rows {
            for i in 0..=sides {
                let a = i as f32 / sides as f32 * TAU;
                let y = y_at(j as f32);
                let p = point(a, y);
                let e = 0.02;
                let da = point(a + e, y) - point(a - e, y);
                let dy = point(a, y_at(j as f32 + 0.1)) - point(a, y_at(j as f32 - 0.1));
                let mut n = da.cross(dy).normalize_or_zero();
                if n.dot(Vec3::new(a.cos(), 0.0, a.sin())) < 0.0 {
                    n = -n;
                }
                let (c, mat) = rock_shade(lay, p, n);
                m.verts.push(crate::render::mesh::Vertex {
                    pos: p.to_array(),
                    normal: n.to_array(),
                    color: c.extend(mat.id()).to_array(),
                });
            }
        }
        let row = sides as u32 + 1;
        for j in 0..rows as u32 {
            for i in 0..sides as u32 {
                let q = [
                    base + j * row + i,
                    base + j * row + i + 1,
                    base + (j + 1) * row + i + 1,
                    base + (j + 1) * row + i,
                ];
                // Wound to face out.
                m.indices.extend([q[0], q[2], q[1], q[0], q[3], q[2]]);
            }
        }
    }

    /// A plasma lamp on a cave wall at `at`, on a bracket standing out
    /// along `out`.
    pub(super) fn lamp(&mut self, at: Vec2, out: Vec2) {
        let st = self.st;
        st.wall_lamp(self, Vec3::new(at.x, 5.9, at.y), out, 1.5);
    }

    /// A cave wall from `a` to `b`, standing outward (`out`) of the cave.
    pub(super) fn cave_wall(&mut self, rng: &mut Rng, a: Vec2, b: Vec2, out: Vec2) {
        let lay = self.l;
        let len = a.distance(b);
        let along = (b - a) / len;
        // Is there rock past each end, round the corner?
        let ext = (
            lay.in_rock(a - along * 1.0 + out * 1.0),
            lay.in_rock(b + along * 1.0 + out * 1.0),
        );
        // In pieces 4-6 m long, their faces flush (a step between two
        // would catch a tank scraping along the wall and throw it).
        // As high as the room's roof: a high room's, where the Crag's rock
        // is over the wall (not out under the mesa's top).
        let top = |p: Vec2| match lay.high_roof(p - out * 1.0) {
            Some(h) if lay.crag_depth(p + out * CAVE_WALL) > 0.5 => h + 0.3,
            _ => ROOF.0 + 0.3,
        };
        let mut t = 0.0;
        while t < len - 0.01 {
            let piece = rng.range(4.0, 6.0).min(len - t);
            let piece = if len - t - piece < 1.5 {
                len - t
            } else {
                piece
            };
            // Where the height changes, in 2 m pieces (on the roof's grid).
            let at = |d: f32| top(a + along * (t + d));
            let piece = if at(0.5) != at(piece - 0.5) {
                2.0f32.min(len - t)
            } else {
                piece
            };
            let y1 = at(0.5).max(at(piece - 0.5));
            // Each piece runs on past its ends into the rock to close the
            // corners, but not past the wall's own ends where it stops at a
            // way through (it would stand there unseen in the way).
            let back = if t < 0.01 && !ext.0 {
                0.0
            } else {
                CAVE_WALL * 0.5
            };
            let on = if t + piece > len - 0.01 && !ext.1 {
                0.0
            } else {
                CAVE_WALL * 0.5
            };
            let (lo, hi) = (t - back, t + piece + on);
            let c = a + along * ((lo + hi) * 0.5) + out * (CAVE_WALL * 0.5 - 0.02);
            let half = if along.x.abs() > 0.5 {
                Vec2::new((hi - lo) * 0.5, CAVE_WALL * 0.5)
            } else {
                Vec2::new(CAVE_WALL * 0.5, (hi - lo) * 0.5)
            };
            let shade = [STONE, STONE_DARK, STONE_PALE][rng.index(3)] * rng.range(0.85, 1.0);
            self.stone(c, 0.0, half, -0.5, y1, shade);
            // The lining is drawn in its place (see `cave_lining`).
            if let Some(o) = self.oboxes.last_mut() {
                o.hidden = true;
            }
            t += piece;
        }
    }

    /// The hall, where the seam is worked under the rock: pillars left to
    /// hold the roof, the vein glowing across the floor between them,
    /// crystal standing where it hasn't been cut yet, cut crystal crated.
    pub(super) fn pillared_hall(&mut self, rng: &mut Rng) {
        let lay = self.l;
        for &p in &lay.pillars {
            let s = rng.range(2.1, 2.5);
            let top = lay.drawn_roof(p) + 0.3;
            self.rock_pillar(p, s, top);
            // Steel straps round it, to keep it from spalling.
            let band = 2.6 + rng.range(0.0, 0.6);
            for y in [band, band + 1.1] {
                self.pipe(
                    Vec3::new(p.x, y, p.y),
                    Vec3::new(p.x, y + 0.3, p.y),
                    s * 1.12 + 0.15,
                    STEEL * 0.8,
                    Material::Metal,
                    false,
                );
            }
            // A lamp on each pillar's face toward the middle.
            let mid = (lay.hall.0 + lay.hall.1) * 0.5;
            let out = if p.x < mid.x { Vec2::X } else { -Vec2::X };
            self.lamp(p + out * (s * 1.1 + 0.2), out);
        }
        // Crystal not yet cut, against the hall's walls (clear of the way
        // in from the west adit, and far enough apart to leave no pocket
        // between them and the wall).
        let (a, b) = lay.hall;
        for (x, z, h) in [
            (a.x + 4.0, a.y + 14.0, 3.2),
            (a.x + 17.5, a.y + 2.5, 2.4),
            (a.x + 16.0, b.y - 3.0, 2.6),
        ] {
            self.outcrop(rng, Vec2::new(x, z), h);
        }
        // Cut crystal crated for the train, by the crosscut.
        self.block_stack(Vec2::new(b.x - 2.0, b.y - 3.0), 0.0);
    }

    /// Cut crystal crated for the train: iron-bound timber crates, two
    /// stacked in the middle and one either side, and a low one behind
    /// with its lid off, crystal lumps showing dull in it. Plain boxes
    /// where the old cut blocks stood are what tanks hit and hide behind.
    pub(super) fn block_stack(&mut self, at: Vec2, yaw: f32) {
        // Its detail comes from its own stream.
        let rng = &mut own_rng(at);
        let g = self.ground_height(at.x, at.y);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        for (k, (dx, dz, h)) in [
            (-1.9, 0.0, 1.6),
            (0.0, 0.0, 3.0),
            (1.9, 0.0, 1.6),
            (0.0, 1.9, 1.2),
        ]
        .into_iter()
        .enumerate()
        {
            let half = 0.9 - 0.02 * k as f32;
            self.hidden_part(
                &rig,
                Vec3::new(dx, (h - 0.2) * 0.5, dz),
                Vec3::new(half, (h + 0.2) * 0.5, half),
            );
        }
        let turn = |rng: &mut Rng| rng.range(-0.08, 0.08);
        let t = turn(rng);
        self.crystal_crate(rng, &rig, Vec2::new(-1.9, 0.0), 0.0, 0.86, 1.55, t, false);
        let t = turn(rng);
        self.crystal_crate(rng, &rig, Vec2::new(0.0, 0.0), 0.0, 0.88, 1.5, t, false);
        // On the one under it, resting on its lid's battens.
        let t = turn(rng) + 0.15;
        self.crystal_crate(
            rng,
            &rig,
            Vec2::new(0.05, -0.05),
            1.625,
            0.76,
            1.375,
            t,
            false,
        );
        let t = turn(rng);
        self.crystal_crate(rng, &rig, Vec2::new(1.9, 0.0), 0.0, 0.86, 1.55, t, false);
        let t = turn(rng);
        self.crystal_crate(rng, &rig, Vec2::new(0.0, 1.9), 0.0, 0.84, 1.15, t, true);
    }

    /// One crate in `rig`'s frame, centered at `c` with its foot at `y0`,
    /// `half` across and `h` high, turned `turn`: plank sides, each board
    /// its own shade, iron at the corners and banded round, a plank lid,
    /// or (`open`) no lid and raw crystal heaped to the rim.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn crystal_crate(
        &mut self,
        rng: &mut Rng,
        rig: &Rig,
        c: Vec2,
        y0: f32,
        half: f32,
        h: f32,
        turn: f32,
        open: bool,
    ) {
        let rig = Rig {
            pos: rig.point(Vec3::new(c.x, y0, c.y)),
            rot: rig.rot * Quat::from_rotation_y(turn),
        };
        // Drawn sculpted, in the crate's frame: as map boxes, touching all
        // over, they'd be grown apart into each other (see `growths`).
        let mut m = MeshData::default();
        let part = |m: &mut MeshData, c: Vec3, h: Vec3, color: Vec3, mat: Material| {
            let col = color.extend(mat.id());
            m.add_box(c - h, c + h, col, col);
        };
        // Dark inside, seen between the boards, the lid resting on it;
        // lower when open, so the rim stands above what's in it. (Faces
        // here stay a few cm apart, never a hair, or they'd z-fight.)
        let core = if open { h - 0.2 } else { h + 0.005 };
        part(
            &mut m,
            Vec3::new(0.0, core * 0.5, 0.0),
            Vec3::new(half - 0.1, core * 0.5, half - 0.1),
            TIMBER * 0.2,
            Material::Wood,
        );
        // Boards round the sides.
        let n = ((h / 0.3).round() as usize).max(3);
        let pitch = h / n as f32;
        for k in 0..n {
            let y = pitch * (k as f32 + 0.5);
            let bh = pitch * 0.44;
            for s in [-1.0f32, 1.0] {
                let shade = TIMBER * rng.range(0.6, 0.85);
                part(
                    &mut m,
                    Vec3::new(s * (half - 0.03), y, 0.0),
                    Vec3::new(0.03, bh, half - 0.07),
                    shade,
                    Material::Wood,
                );
                let shade = TIMBER * rng.range(0.6, 0.85);
                part(
                    &mut m,
                    Vec3::new(0.0, y, s * (half - 0.03)),
                    Vec3::new(half - 0.07, bh, 0.03),
                    shade,
                    Material::Wood,
                );
            }
        }
        // Iron: angle down each corner, and two bands round over it.
        for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            part(
                &mut m,
                Vec3::new(x * (half - 0.04), h * 0.5, z * (half - 0.04)),
                Vec3::new(0.06, h * 0.5, 0.06),
                IRON * 0.8,
                Material::Metal,
            );
        }
        for y in [h * 0.2, h * 0.8] {
            for s in [-1.0f32, 1.0] {
                part(
                    &mut m,
                    Vec3::new(s * (half + 0.02), y, 0.0),
                    Vec3::new(0.025, 0.045, half + 0.045),
                    IRON * 0.7,
                    Material::Rust,
                );
                part(
                    &mut m,
                    Vec3::new(0.0, y, s * (half + 0.02)),
                    Vec3::new(half + 0.045, 0.045, 0.025),
                    IRON * 0.7,
                    Material::Rust,
                );
            }
        }
        if !open {
            // The lid: three planks across, nailed down on two battens.
            for k in 0..3 {
                let z = (k as f32 - 1.0) * half * 0.66;
                part(
                    &mut m,
                    Vec3::new(0.0, h + 0.04, z),
                    Vec3::new(half + 0.03, 0.035, half * 0.32),
                    TIMBER * rng.range(0.65, 0.9),
                    Material::Wood,
                );
            }
            for s in [-1.0f32, 1.0] {
                part(
                    &mut m,
                    Vec3::new(s * half * 0.6, h + 0.1, 0.0),
                    Vec3::new(0.06, 0.025, half - 0.04),
                    TIMBER * 0.55,
                    Material::Wood,
                );
            }
            self.sculpt(&rig, &m);
            return;
        }
        // Open: lumps of raw crystal heaped to the rim, dull blue-grey,
        // and one long piece laid across them, its faint light up a face.
        let inner = half - 0.12;
        for i in 0..3 {
            for j in 0..3 {
                let u = (i as f32 - 1.0) * inner * 0.62 + rng.range(-0.08, 0.08);
                let v = (j as f32 - 1.0) * inner * 0.62 + rng.range(-0.08, 0.08);
                let mound = 0.12 * (1.0 - (u * u + v * v) / (inner * inner)).max(0.0);
                let at = Vec3::new(u, core + 0.05 + mound, v);
                let s = rng.range(0.22, 0.32);
                let shade = RAW * rng.range(1.0, 1.3);
                ore_lump(&mut m, rng, at, s, shade, Material::Ore);
            }
        }
        self.sculpt(&rig, &m);
        let a = rig.point(Vec3::new(-inner * 0.8, core + 0.22, -inner * 0.3));
        let b = rig.point(Vec3::new(inner * 0.7, core + 0.32, inner * 0.35));
        self.crystals.push(Crystal {
            base: a,
            axis: b - a,
            radius: 0.17,
            sides: 6,
            seed: rng.next_u64() as u32,
            color: RAW * 1.1,
            lit: true,
            solid: false,
            raw: true,
        });
    }

    /// The Resonant's breach: their own tunnel in from the Face to the
    /// drift, rough-cut, rubble at its mouth.
    pub(super) fn breach(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let (a, b) = lay.breach;
        // Rubble they dug out heaped by the mouth.
        let f = lay.face((a.x + b.x) * 0.5);
        for p in [Vec2::new(a.x - 6.0, f - 6.0), Vec2::new(b.x + 6.0, f - 8.0)] {
            self.rock_pile(rng, p, 1.6);
        }
    }

    /// The box cut: an open trench up from the adit's end to the mesa, its
    /// sides faced in cut stone.
    pub(super) fn box_cut(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let (a, b) = lay.box_cut;
        for (z, out) in [(a.y, -1.0f32), (b.y, 1.0)] {
            let mut x = a.x + 2.0;
            while x < b.x {
                let w = rng.range(4.0, 6.0).min(b.x - x);
                let c = x + w * 0.5;
                x += w;
                // Inside the Crag the cut's sides are its sculpted face.
                if c + w * 0.5 < lay.crag_east(z) - 0.5 {
                    continue;
                }
                let top = TOP + 0.3;
                self.stone(
                    Vec2::new(c, z + out * (CAVE_WALL * 0.5 - 0.02)),
                    0.0,
                    Vec2::new(w * 0.5 + 0.01, CAVE_WALL * 0.5),
                    -0.5,
                    top,
                    [STONE, STONE_PALE][rng.index(2)] * rng.range(0.9, 1.0),
                );
            }
        }
    }

    /// The track, from the end of the adit out across the bridge and the
    /// flats to the south edge, and the branch off it into the warehouse:
    /// two rails on sleepers. Visual only.
    pub(super) fn railroad(&mut self) {
        let lay = self.l;
        self.rails(&lay.track, false);
        let branch = lay.branch.clone();
        self.rails(&branch, true);
        // The points lever by the junction, on the outside of the curve.
        let (j, dir) = lay.branch_from();
        let at = j - dir * 2.0 + dir.perp() * 2.8 * lay.branch_side;
        let g = self.ground_height(at.x, at.y);
        self.oboxes.push(OBox {
            center: at,
            yaw: yaw_along(dir),
            half: Vec2::new(0.25, 0.35),
            y0: g - 0.1,
            y1: g + 0.5,
            color: STEEL,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        self.segment(
            Vec3::new(at.x, g + 0.45, at.y),
            Vec3::new(at.x + dir.x * 0.3, g + 1.3, at.y + dir.y * 0.3),
            0.04,
            RUST,
            Material::Rust,
            false,
        );
    }

    /// Rails on sleepers along `line`, following the ground (or the
    /// bridge's deck). A branch leaves out the sleepers where the main
    /// line's already are.
    pub(super) fn rails(&mut self, line: &[Vec2], branch: bool) {
        let lay = self.l;
        let total = line_len(line, line.len() - 1);
        let y_at = |m: &Map, s: f32| {
            let (p, dir) = line_at(line, s);
            if lay.on_bridge(p) {
                DECK_TOP
            } else {
                let side = dir.perp() * 1.5;
                m.ground_height(p.x, p.y)
                    .max(m.ground_height(p.x + side.x, p.y + side.y))
                    .max(m.ground_height(p.x - side.x, p.y - side.y))
            }
        };
        let step = 1.6;
        let n = (total / step) as usize;
        for k in 0..n {
            let s = step * (k as f32 + 0.5);
            let (p, dir) = line_at(line, s);
            if branch && lay.main_line_distance(p) < 2.4 {
                continue;
            }
            let y = y_at(self, s);
            self.oboxes.push(OBox {
                center: p,
                yaw: yaw_along(dir),
                half: Vec2::new(GAUGE * 0.5 + 0.7, 0.18),
                y0: y - 0.1,
                y1: y + 0.08,
                color: TIMBER * 0.8,
                side_mat: Material::Wood,
                top_mat: Material::Wood,
                solid: false,
                ..OBox::default()
            });
        }
        // Rails, in runs of up to 6 m following the ground, broken at the
        // line's bends.
        let mut s = if branch { 1.0 } else { 0.0 };
        let bends: Vec<f32> = (1..line.len()).map(|k| line_len(line, k)).collect();
        while s < total - 0.01 {
            let next_bend = bends
                .iter()
                .copied()
                .find(|&b| b > s + 0.01)
                .unwrap_or(total);
            let e = (s + 6.0).min(next_bend);
            let (pa, dir) = line_at(line, s + 0.001);
            let (pb, _) = line_at(line, e - 0.001);
            let (ya, yb) = (y_at(self, s) + 0.16, y_at(self, e - 0.001) + 0.16);
            for side in [-1.0f32, 1.0] {
                let o = dir.perp() * side * GAUGE * 0.5;
                self.segment(
                    Vec3::new(pa.x + o.x, ya, pa.y + o.y),
                    Vec3::new(pb.x + o.x, yb, pb.y + o.y),
                    0.08,
                    STEEL * 1.4,
                    Material::Metal,
                    false,
                );
            }
            s = e;
        }
    }

    /// The rail bridge out of the adit's mouth over the wash: a deck on
    /// concrete piers, low steel girders along its edges, a truss
    /// over them, tilted plates up onto it at each end.
    pub(super) fn rail_bridge(&mut self) {
        let lay = self.l;
        let (a, b) = lay.bridge();
        let len = a.distance(b);
        let dir = (b - a) / len;
        let yaw = yaw_along(dir);
        let mid = (a + b) * 0.5;
        self.oboxes.push(OBox {
            center: mid,
            yaw,
            half: Vec2::new(DECK_HALF, len * 0.5),
            y0: DECK_TOP - DECK_T,
            y1: DECK_TOP,
            color: STEEL * 1.2,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            ramp: true,
            ..OBox::default()
        });
        // Plates up onto it from the ground at each end.
        for (end, out) in [(a, -dir), (b, dir)] {
            let run = 2.6;
            let rise = DECK_TOP + 0.15;
            let slope = (rise / run).atan();
            let c = end + out * (run * 0.5 - 0.05);
            // Local +z points along `out`; tip its far (+z) end down.
            self.oboxes.push(OBox {
                center: c,
                yaw: yaw_along(out),
                half: Vec2::new(DECK_HALF - 0.4, run * 0.5 + 0.1),
                y0: DECK_TOP * 0.5 - 0.2 - 0.15,
                y1: DECK_TOP * 0.5 + 0.2 - 0.15,
                tilt: slope,
                color: STEEL,
                side_mat: Material::Metal,
                top_mat: Material::Metal,
                ramp: true,
                ..OBox::default()
            });
        }
        // Girders along each edge: low enough to shoot over, high enough to
        // keep a tank on the deck. A truss of posts and diagonals over them.
        let side_dir = dir.perp();
        for side in [-1.0f32, 1.0] {
            let off = side_dir * side * (DECK_HALF - 0.25);
            self.oboxes.push(OBox {
                center: mid + off,
                yaw,
                half: Vec2::new(0.25, len * 0.5),
                y0: DECK_TOP - 0.01,
                y1: DECK_TOP + 0.9,
                color: RUST,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
            let n = (len / 6.0) as usize;
            let at = |k: usize, y: f32| {
                let p = a + dir * (len * k as f32 / n as f32) + off;
                Vec3::new(p.x, y, p.y)
            };
            for k in 0..=n {
                let foot = at(k, DECK_TOP + 0.9);
                self.segment(
                    foot,
                    foot + Vec3::Y * 2.6,
                    0.12,
                    RUST,
                    Material::Rust,
                    false,
                );
                if k < n {
                    let (p, q) = if k % 2 == 0 {
                        (foot, at(k + 1, DECK_TOP + 3.5))
                    } else {
                        (foot + Vec3::Y * 2.6, at(k + 1, DECK_TOP + 0.9))
                    };
                    self.segment(p, q, 0.08, RUST, Material::Rust, false);
                }
            }
            self.segment(
                at(0, DECK_TOP + 3.5),
                at(n, DECK_TOP + 3.5),
                0.14,
                RUST,
                Material::Rust,
                false,
            );
        }
        // Piers: a concrete column under each rail where the wash falls
        // away, a steel cap beam across their tops.
        // Ten meters apart: room to drive between them, and either side of
        // where the wash's middle runs under the deck (the way along its
        // floor).
        let cross = (line_at(&lay.wash, lay.bridge_s).0 - a).dot(dir);
        let mut t = cross - 5.0;
        while t - 10.0 >= 4.0 {
            t -= 10.0;
        }
        while t < 4.0 {
            t += 10.0;
        }
        while t < len - 4.0 {
            let c = a + dir * t;
            let floor = self.ground_height(c.x, c.y);
            t += 10.0;
            if floor > -2.5 {
                continue;
            }
            for side in [-1.0f32, 1.0] {
                let at = c + side_dir * side * 2.4;
                let g = self.ground_height(at.x, at.y);
                self.oboxes.push(OBox {
                    center: at,
                    yaw: yaw + PI / 4.0,
                    half: Vec2::splat(0.8),
                    y0: g - 0.5,
                    y1: DECK_TOP - DECK_T,
                    color: CONCRETE * 0.8,
                    side_mat: Material::Concrete,
                    top_mat: Material::Concrete,
                    ..OBox::default()
                });
            }
            self.oboxes.push(OBox {
                center: c,
                yaw,
                half: Vec2::new(DECK_HALF + 0.3, 0.9),
                y0: DECK_TOP - DECK_T - 0.9,
                y1: DECK_TOP - DECK_T + 0.01,
                color: RUST,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
            let (l, r) = (c - side_dir * 2.4, c + side_dir * 2.4);
            self.segment(
                Vec3::new(l.x, floor + 3.0, l.y),
                Vec3::new(r.x, DECK_TOP - DECK_T - 0.9, r.y),
                0.1,
                STEEL,
                Material::Metal,
                false,
            );
        }
    }

    /// Rock in the wash: blocks fallen from the banks, cover along the
    /// floor.
    pub(super) fn wash_rocks(&mut self, rng: &mut Rng, seam: &[(Vec2, f32)]) {
        let lay = self.l;
        let mut placed = 0;
        for _ in 0..600 {
            if placed >= 13 {
                break;
            }
            let k = rng.index(lay.wash.len() - 1);
            let p = lay.wash[k].lerp(lay.wash[k + 1], rng.f32());
            let (_, along) = lay.wash_near(p);
            let (_, half) = lay.wash_profile(along);
            let dir = (lay.wash[k + 1] - lay.wash[k]).normalize().perp();
            // Toward one side, so the middle of the floor stays a way through.
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let q = p + dir * side * rng.range(half * 0.45, half + 0.5);
            if q.min_element() < 12.0
                || q.max_element() > lay.size - 12.0
                || seg(q, lay.bridge().0, lay.bridge().1).0 < DECK_HALF + 3.0
                || lay
                    .ramps
                    .iter()
                    .chain(&lay.fords)
                    .any(|&(s, e, h)| seg(q, s, e).0 < h + 3.0)
                || self.on_way(q)
                || self
                    .oboxes
                    .iter()
                    .any(|o| o.solid && o.center.distance(q) < 11.0)
                || self
                    .crystals
                    .iter()
                    .filter(|c| c.solid)
                    .map(|c| Vec2::new(c.base.x, c.base.z))
                    .chain(seam.iter().map(|&(p, _)| p))
                    .any(|c| c.distance(q) < 7.0)
            {
                continue;
            }
            let s = rng.range(1.3, 2.2);
            self.rock_pile(rng, q, s);
            placed += 1;
        }
    }

    /// A cutting rig at `at` on ground at `floor`, its boom reaching out
    /// along `yaw`: a painted steel body on four crawler units, a cab on
    /// top, a toothed cutting drum at the end of the boom.
    pub(super) fn cutting_rig(&mut self, at: Vec2, yaw: f32, floor: f32) {
        let rig = Rig {
            pos: Vec3::new(at.x, floor, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        for (x, z) in [(-1.8, -2.2), (1.8, -2.2), (-1.8, 2.2), (1.8, 2.2)] {
            rig.part(
                self,
                Vec3::new(x, 0.45, z),
                Vec3::new(0.8, 0.3, 1.0),
                Quat::IDENTITY,
                STEEL,
                Material::Metal,
                true,
                0.0,
            );
            // The crawler's track, down on the ground under it.
            rig.part(
                self,
                Vec3::new(x, 0.12, z),
                Vec3::new(0.85, 0.12, 1.05),
                Quat::IDENTITY,
                STEEL * 0.6,
                Material::Metal,
                false,
                0.0,
            );
        }
        rig.part(
            self,
            Vec3::new(0.0, 1.7, 0.0),
            Vec3::new(2.2, 0.95, 3.2),
            Quat::IDENTITY,
            MACHINE,
            Material::Painted,
            true,
            0.0,
        );
        rig.part(
            self,
            Vec3::new(0.0, 3.1, -1.6),
            Vec3::new(1.4, 0.5, 1.2),
            Quat::IDENTITY,
            STEEL,
            Material::Metal,
            true,
            0.0,
        );
        let root = Vec3::new(0.0, 2.9, 2.6);
        let elbow = Vec3::new(0.0, 5.0, 6.0);
        let head = Vec3::new(0.0, 1.2, 8.5);
        for x in [-0.6, 0.6] {
            let o = Vec3::new(x, 0.0, 0.0);
            self.segment(
                rig.point(root + o),
                rig.point(elbow + o),
                0.28,
                STEEL,
                Material::Metal,
                false,
            );
            self.segment(
                rig.point(elbow + o),
                rig.point(head + o),
                0.24,
                STEEL,
                Material::Metal,
                false,
            );
        }
        let hc = rig.point(head);
        let axle = rig.rot * Vec3::X * 0.35;
        self.disc(hc, axle, 1.3, STEEL * 1.2, Material::Metal);
        self.disc(hc, axle * 1.25, 0.6, RUST, Material::Rust);
        // Picks round the drum.
        for k in 0..10 {
            let a = k as f32 * TAU / 10.0;
            let out = rig.rot * Vec3::new(0.0, a.cos(), a.sin());
            self.segment(
                hc + out * 1.2,
                hc + out * 1.55,
                0.09,
                STEEL * 0.7,
                Material::Metal,
                false,
            );
        }
    }

    /// A small ore cart at `at` heading `yaw`: an open-top wooden tub,
    /// plank sides bound in iron, on an iron frame and four iron wheels,
    /// heaped over the rim with ore from the Crag. A plain box the size of
    /// the tub is what tanks hit and hide behind.
    pub(super) fn ore_cart(&mut self, rng: &mut Rng, at: Vec2, yaw: f32) {
        // Its detail comes from its own stream.
        let _ = rng;
        if self.in_the_way(at, 1.8) {
            return;
        }
        let rng = &mut own_rng(at);
        let g = self.ground_height(at.x, at.y);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        let part = |m: &mut Map, c: Vec3, h: Vec3, color: Vec3, mat: Material, solid: bool| {
            rig.part(m, c, h, Quat::IDENTITY, color, mat, solid, 0.0);
        };
        // The frame under the tub, and the axles.
        part(
            self,
            Vec3::new(0.0, 0.62, 0.0),
            Vec3::new(0.7, 0.1, 1.45),
            IRON,
            Material::Rust,
            false,
        );
        for z in [-1.0, 1.0] {
            let a = rig.point(Vec3::new(-0.92, 0.35, z));
            let b = rig.point(Vec3::new(0.92, 0.35, z));
            self.segment(a, b, 0.06, IRON * 0.8, Material::Metal, false);
            part(
                self,
                Vec3::new(0.0, 0.5, z),
                Vec3::new(0.75, 0.08, 0.1),
                IRON,
                Material::Rust,
                false,
            );
        }
        // Iron wheels: a flanged rim, spokes' dark between, and a hub.
        for (x, z) in [(-0.95, -1.0), (0.95, -1.0), (-0.95, 1.0), (0.95, 1.0)] {
            let c = rig.point(Vec3::new(x, 0.35, z));
            let axis = rig.rot * Vec3::X;
            self.disc(c, axis * 0.07, 0.35, IRON * 0.75, Material::Metal);
            self.disc(
                c - axis * (0.06 * x.signum()),
                axis * 0.03,
                0.42,
                IRON * 0.65,
                Material::Rust,
            );
            self.disc(
                c + axis * (0.07 * x.signum()),
                axis * 0.05,
                0.12,
                IRON * 1.2,
                Material::Metal,
            );
        }
        // The tub's floor.
        part(
            self,
            Vec3::new(0.0, 0.82, 0.0),
            Vec3::new(0.95, 0.07, 1.5),
            TIMBER * 0.7,
            Material::Wood,
            false,
        );
        // Dark inner boarding, seen through the gaps between the planks.
        for s in [-1.0f32, 1.0] {
            part(
                self,
                Vec3::new(s * 0.94, 1.35, 0.0),
                Vec3::new(0.02, 0.58, 1.5),
                TIMBER * 0.25,
                Material::Wood,
                false,
            );
            part(
                self,
                Vec3::new(0.0, 1.35, s * 1.49),
                Vec3::new(0.9, 0.58, 0.02),
                TIMBER * 0.25,
                Material::Wood,
                false,
            );
        }
        // Plank sides, three boards high, each its own shade.
        let (y0, board) = (0.75, 0.4);
        for k in 0..3 {
            let y = y0 + board * (k as f32 + 0.5);
            let h = board * 0.43;
            for s in [-1.0f32, 1.0] {
                let shade = TIMBER * rng.range(0.55, 0.75);
                part(
                    self,
                    Vec3::new(s * 0.98, y, 0.0),
                    Vec3::new(0.07, h, 1.6),
                    shade,
                    Material::Wood,
                    false,
                );
                let shade = TIMBER * rng.range(0.55, 0.75);
                part(
                    self,
                    Vec3::new(0.0, y, s * 1.53),
                    Vec3::new(0.91, h, 0.07),
                    shade,
                    Material::Wood,
                    false,
                );
            }
        }
        // Iron binding: corner posts, and a strap round the middle of each
        // side.
        for (x, z) in [(-1.0, -1.55), (1.0, -1.55), (-1.0, 1.55), (1.0, 1.55)] {
            part(
                self,
                Vec3::new(x, 1.35, z),
                Vec3::new(0.09, 0.62, 0.09),
                IRON * 0.8,
                Material::Metal,
                false,
            );
        }
        for s in [-1.0f32, 1.0] {
            for z in [-0.55, 0.55] {
                part(
                    self,
                    Vec3::new(s * 1.06, 1.35, z),
                    Vec3::new(0.02, 0.6, 0.08),
                    IRON * 0.8,
                    Material::Metal,
                    false,
                );
            }
            part(
                self,
                Vec3::new(0.0, 1.35, s * 1.61),
                Vec3::new(0.08, 0.6, 0.02),
                IRON * 0.8,
                Material::Metal,
                false,
            );
        }
        // What tanks hit: the old cart's frame and tub, exactly. The fill
        // that places cover after reads these, so the rest of the map keeps
        // its layout.
        self.hidden_part(&rig, Vec3::new(0.0, 0.5, 0.0), Vec3::new(0.9, 0.25, 1.5));
        self.hidden_part(&rig, Vec3::new(0.0, 1.35, 0.0), Vec3::new(1.05, 0.6, 1.6));
        // The ore, heaped over the rim, highest down the middle.
        self.heaped_ore(
            rng,
            &rig,
            |_| 0.88,
            1.43,
            1.8,
            0.5,
            (8, 14),
            16,
            (0.1, 0.2),
            2,
        );
    }

    /// Ore heaped in a tub (see `ore_heap`), drawn sculpted in `rig`'s
    /// frame: the mound, `lumps` loose stones between `size.0` and
    /// `size.1` across lying on it, and `crystals` lumps of raw crystal.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn heaped_ore(
        &mut self,
        rng: &mut Rng,
        rig: &Rig,
        half_w: impl Fn(f32) -> f32 + Copy,
        half_l: f32,
        y_edge: f32,
        rise: f32,
        grid: (usize, usize),
        lumps: usize,
        size: (f32, f32),
        crystals: usize,
    ) {
        let mut m = MeshData::default();
        ore_heap(&mut m, rng, half_w, half_l, y_edge, rise, grid.0, grid.1);
        let shades = [ORE * 0.8, ORE * 0.6, ORE * 0.95, MESA_TOP * 0.6];
        for k in 0..lumps + crystals {
            let (u, v) = (rng.range(-0.75, 0.75), rng.range(-0.8, 0.8));
            let z = v * half_l;
            let at = Vec3::new(u * half_w(z), heap_height(y_edge, rise, u, v), z);
            let s = rng.range(size.0, size.1);
            let (color, mat) = if k < lumps {
                (shades[rng.index(shades.len())], Material::Rubble)
            } else {
                (RAW * 1.15, Material::Ore)
            };
            ore_lump(&mut m, rng, at, s, color, mat);
        }
        self.sculpt(rig, &m);
    }

    /// A collider at `local` in `rig` (half extents `half`), not drawn: the
    /// box round something drawn sculpted.
    pub(super) fn hidden_part(&mut self, rig: &Rig, local: Vec3, half: Vec3) {
        rig.part(
            self,
            local,
            half,
            Quat::IDENTITY,
            Vec3::new(0.03, 0.07, 0.16),
            Material::Vhaal,
            true,
            0.0,
        );
        if let Some(o) = self.oboxes.last_mut() {
            o.hidden = true;
        }
    }

    /// Draws `m`, built in `rig`'s frame, into the sculpted surfaces.
    pub(super) fn sculpt(&mut self, rig: &Rig, m: &MeshData) {
        let base = self.sculpted.verts.len() as u32;
        for v in &m.verts {
            let mut v = *v;
            v.pos = rig.point(Vec3::from(v.pos)).to_array();
            v.normal = (rig.rot * Vec3::from(v.normal)).to_array();
            self.sculpted.verts.push(v);
        }
        self.sculpted
            .indices
            .extend(m.indices.iter().map(|i| i + base));
    }

    /// The ore train, stopped on the flats headed out: the locomotive, two
    /// ore cars behind it under the tipple. Far down the line, two cars off
    /// the rails, one on its side, its load spilled out of it.
    pub(super) fn ore_train(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let s0 = lay.track_len(3) + 6.0;
        // Ore cars under and past the tipple, then the loco ahead (south).
        for (s, loco) in [(s0, false), (s0 + 10.0, false), (s0 + 22.0, true)] {
            let (p, dir) = lay.track_at(s);
            self.rail_car(p, yaw_along(dir), 0.0, loco);
        }
        // The wreck, far down the line, off the side away from the branch.
        let away = -lay.branch_side;
        let (p, dir) = lay.track_at(lay.track_len(5) - 10.0);
        let side = dir.perp() * away;
        self.rail_car(
            p - side * 6.5,
            yaw_along(dir) + 0.35 * away,
            1.5 * away,
            false,
        );
        let (q, dir2) = lay.track_at(lay.track_len(5) + 2.0);
        self.rail_car(
            q - dir2.perp() * away * 1.2,
            yaw_along(dir2) - 0.12 * away,
            0.0,
            false,
        );
        let _ = rng;
    }

    /// A rail car of the style's (see `MineStyle::rail_car`).
    pub(super) fn rail_car(&mut self, at: Vec2, yaw: f32, roll: f32, loco: bool) {
        let st = self.st;
        st.rail_car(self, at, yaw, roll, loco);
    }

    /// The loading tipple over the track south of the bridge: a steel ore
    /// bin heaped over its rim, on four braced legs high enough to run a
    /// train (or a tank) under, its hopper narrowing to a spout over the
    /// rails, and the conveyor climbing to it out of the wash from the
    /// cutting rig and tipping onto the heap.
    pub(super) fn tipple(&mut self) {
        let lay = self.l;
        let (at, dir) = lay.track_at(lay.track_len(3) + 6.0);
        let g = self.ground_height(at.x, at.y);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw_along(dir)),
        };
        // Its detail from its own stream (the car under it has the one at
        // `at`), so nothing else on the map moves.
        let rng = &mut own_rng(at + Vec2::splat(0.5));
        // The legs' tops and the bin's floor, its rim, the hopper's throat
        // and the spout's mouth, the clearance over the rails.
        let (deck, rim, throat, mouth) = (8.0f32, 11.0f32, 6.9f32, 6.1f32);
        let (lx, lz) = (3.6f32, 5.4f32);
        let (bx, bz) = (3.9f32, 5.7f32);
        for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let foot = rig.point(Vec3::new(sx * lx, 0.0, sz * lz));
            let gf = self.ground_height(foot.x, foot.z);
            self.oboxes.push(OBox {
                center: Vec2::new(foot.x, foot.z),
                yaw: yaw_along(dir),
                half: Vec2::splat(0.3),
                y0: gf - 0.3,
                y1: g + deck,
                color: RUST,
                side_mat: Material::Rust,
                top_mat: Material::Rust,
                ..OBox::default()
            });
            // A footing under each leg.
            self.oboxes.push(OBox {
                center: Vec2::new(foot.x, foot.z),
                yaw: yaw_along(dir),
                half: Vec2::splat(0.55),
                y0: gf - 0.3,
                y1: gf + 0.35,
                color: STEEL * 1.4,
                side_mat: Material::Metal,
                top_mat: Material::Metal,
                chip: 0.08,
                ..OBox::default()
            });
        }
        let piece = |m: &mut Map, a: Vec3, b: Vec3, r: f32| {
            m.segment(rig.point(a), rig.point(b), r, RUST, Material::Rust, false);
        };
        // The long sides, outside the rails: a girt at half height and an X
        // in each bay, below it and above it.
        let girt = 4.0;
        for sx in [-1.0f32, 1.0] {
            let x = sx * (lx + 0.05);
            piece(self, Vec3::new(x, girt, -lz), Vec3::new(x, girt, lz), 0.16);
            for (y0, y1) in [(0.5, girt), (girt, deck - 0.4)] {
                piece(self, Vec3::new(x, y0, -lz), Vec3::new(x, y1, lz), 0.11);
                piece(self, Vec3::new(x, y0, lz), Vec3::new(x, y1, -lz), 0.11);
            }
        }
        // Across the track, only knee braces up under the frame, clear of
        // anything that runs under it.
        for sz in [-1.0f32, 1.0] {
            let z = sz * (lz + 0.05);
            for sx in [-1.0f32, 1.0] {
                piece(
                    self,
                    Vec3::new(sx * lx, deck - 2.0, z),
                    Vec3::new(sx * (lx - 1.8), deck - 0.4, z),
                    0.12,
                );
            }
        }
        // The frame round the legs' tops, the bin's floor sitting on it.
        for sx in [-1.0f32, 1.0] {
            rig.part(
                self,
                Vec3::new(sx * lx, deck - 0.25, 0.0),
                Vec3::new(0.3, 0.25, lz + 0.3),
                Quat::IDENTITY,
                RUST,
                Material::Rust,
                false,
                0.0,
            );
        }
        for sz in [-1.0f32, 1.0] {
            rig.part(
                self,
                Vec3::new(0.0, deck - 0.25, sz * lz),
                Vec3::new(lx + 0.3, 0.25, 0.3),
                Quat::IDENTITY,
                RUST,
                Material::Rust,
                false,
                0.0,
            );
        }
        // The bin: steel walls, the hopper under them narrowing to the
        // throat, the spout below that. What shots hit is a box round the
        // walls, as the old bin was.
        let mut m = MeshData::default();
        let inside = Vec3::new(0.0, (deck + rim) * 0.5, 0.0);
        let ring = |y: f32, hx: f32, hz: f32| {
            [
                Vec3::new(hx, y, -hz),
                Vec3::new(hx, y, hz),
                Vec3::new(-hx, y, hz),
                Vec3::new(-hx, y, -hz),
            ]
        };
        let (top, bot) = (ring(rim, bx, bz), ring(deck, bx, bz));
        let (hop, neck) = (ring(deck, bx, bz), ring(throat, 0.9, 1.1));
        let funnel = Vec3::new(0.0, deck + 2.0, 0.0);
        for i in 0..4 {
            let j = (i + 1) % 4;
            model::plate(&mut m, [bot[i], bot[j], top[j], top[i]], 0.14, inside, BIN);
            model::plate(
                &mut m,
                [neck[i], neck[j], hop[j], hop[i]],
                0.14,
                funnel,
                BIN,
            );
        }
        let spout = model::rect(Vec2::ZERO, Vec2::new(0.9, 1.1), 0.12);
        let lip = model::rect(Vec2::new(0.0, -0.3), Vec2::new(0.7, 0.8), 0.12);
        model::solid(
            &mut m,
            &[model::at_y(&lip, mouth), model::at_y(&spout, throat + 0.05)],
            0.04,
            BIN,
        );
        self.sculpt(&rig, &m);
        self.hidden_part(
            &rig,
            Vec3::new(0.0, (deck + rim) * 0.5, 0.0),
            Vec3::new(bx, (rim - deck) * 0.5, bz),
        );
        // Ribs up the walls and an angle round the rim.
        for sx in [-1.0f32, 1.0] {
            for k in 0..5 {
                let z = -bz + 0.4 + k as f32 * (2.0 * bz - 0.8) / 4.0;
                rig.part(
                    self,
                    Vec3::new(sx * (bx + 0.12), (deck + rim) * 0.5, z),
                    Vec3::new(0.1, (rim - deck) * 0.5, 0.14),
                    Quat::IDENTITY,
                    RUST,
                    Material::Rust,
                    false,
                    0.0,
                );
            }
            rig.part(
                self,
                Vec3::new(sx * (bx + 0.1), rim - 0.1, 0.0),
                Vec3::new(0.14, 0.14, bz + 0.2),
                Quat::IDENTITY,
                RUST,
                Material::Rust,
                false,
                0.0,
            );
        }
        for sz in [-1.0f32, 1.0] {
            for k in 0..3 {
                let x = -bx + 0.5 + k as f32 * (bx - 0.5);
                rig.part(
                    self,
                    Vec3::new(x, (deck + rim) * 0.5, sz * (bz + 0.12)),
                    Vec3::new(0.14, (rim - deck) * 0.5, 0.1),
                    Quat::IDENTITY,
                    RUST,
                    Material::Rust,
                    false,
                    0.0,
                );
            }
            rig.part(
                self,
                Vec3::new(0.0, rim - 0.1, sz * (bz + 0.1)),
                Vec3::new(bx + 0.2, 0.14, 0.14),
                Quat::IDENTITY,
                RUST,
                Material::Rust,
                false,
                0.0,
            );
        }
        // The ore, heaped up over the rim, highest under the conveyor's
        // head; raw crystal in it.
        let lump = |m: &mut Map, rng: &mut Rng, at: Vec3, s: f32| {
            let shade = [ORE, ORE * 0.75, ORE * 1.2, MESA_TOP * 0.8][rng.index(4)];
            let turn = Quat::from_euler(
                glam::EulerRot::YXZ,
                rng.range(0.0, TAU),
                rng.range(-0.6, 0.6),
                rng.range(-0.6, 0.6),
            );
            rig.part(
                m,
                at,
                Vec3::new(
                    s * rng.range(0.8, 1.2),
                    s * rng.range(0.55, 0.85),
                    s * rng.range(0.8, 1.2),
                ),
                turn,
                shade * rng.range(0.9, 1.1),
                Material::Rubble,
                false,
                0.0,
            );
            if let Some(o) = m.oboxes.last_mut() {
                o.rock = Some(rocks::Kind::Rubble);
            }
        };
        rig.part(
            self,
            Vec3::new(0.0, rim - 0.35, 0.0),
            Vec3::new(bx - 0.1, 0.3, bz - 0.1),
            Quat::IDENTITY,
            ORE * 0.7,
            Material::Rubble,
            false,
            0.0,
        );
        self.last_is_rock(rocks::Kind::Block);
        let heap = |x: f32, z: f32| {
            let u = (1.0 - (x / bx).powi(2)) * (1.0 - (z / bz).powi(2));
            rim - 0.2 + 1.5 * u.max(0.0)
        };
        for _ in 0..44 {
            let x = rng.range(-bx + 0.5, bx - 0.5);
            let z = rng.range(-bz + 0.5, bz - 0.5);
            let s = rng.range(0.35, 0.75);
            lump(self, rng, Vec3::new(x, heap(x, z) - s * 0.3, z), s);
        }
        for _ in 0..6 {
            let x = rng.range(-2.4, 2.4);
            let z = rng.range(-3.6, 3.6);
            let s = rng.range(0.25, 0.4);
            rig.part(
                self,
                Vec3::new(x, heap(x, z), z),
                Vec3::new(s, s * 0.7, s * 1.3),
                Quat::from_euler(glam::EulerRot::YXZ, rng.range(0.0, TAU), 0.4, 0.3),
                RAW * 1.15,
                Material::Ore,
                false,
                0.0,
            );
        }
        // Ore caught in the spout's mouth, and what's missed the cars and
        // lies along the rails.
        for _ in 0..3 {
            let at = Vec3::new(
                rng.range(-0.4, 0.4),
                mouth - rng.range(0.0, 0.25),
                -0.3 + rng.range(-0.5, 0.5),
            );
            let s = rng.range(0.25, 0.4);
            lump(self, rng, at, s);
        }
        for _ in 0..12 {
            let side = if rng.chance(0.5) { 1.0 } else { -1.0 };
            let x = side * rng.range(1.6, 3.4);
            let z = rng.range(-lz, lz);
            let p = rig.point(Vec3::new(x, 0.0, z));
            let gp = self.ground_height(p.x, p.z) - g;
            let s = rng.range(0.2, 0.42);
            lump(self, rng, Vec3::new(x, gp + s * 0.3, z), s);
        }
        // The conveyor from beside the rig in the wash, up over the bank,
        // to its head over the heap.
        let (rig_at, rig_yaw) = lay.wash_rig;
        let boom = Vec2::new(rig_yaw.sin(), rig_yaw.cos());
        let toward = (at - rig_at).normalize();
        let foot = rig_at + boom * 4.0 + toward * 3.5;
        let a = Vec3::new(foot.x, self.ground_height(foot.x, foot.y) + 1.4, foot.y);
        let b = rig.point(Vec3::new(-bx + 1.2, rim + 1.6, 0.0));
        self.segment(a, b, 0.5, STEEL, Material::Metal, false);
        if let Some(o) = self.oboxes.last_mut() {
            o.half.x = 0.9;
        }
        // Its head drum, and the ore going over it onto the heap.
        let across = (b - a).cross(Vec3::Y).normalize();
        self.segment(
            b - across * 1.0,
            b + across * 1.0,
            0.45,
            RUST,
            Material::Rust,
            false,
        );
        let ahead = (b - a).normalize();
        for k in 0..4 {
            let p = b + ahead * (0.5 + k as f32 * 0.25) + Vec3::Y * (0.4 - k as f32 * 0.35);
            let s = rng.range(0.22, 0.32);
            let at = Vec2::new(p.x, p.z) + Vec2::new(across.x, across.z) * rng.range(-0.5, 0.5);
            let yaw = rng.range(0.0, TAU);
            let color = ORE * rng.range(0.8, 1.2);
            self.oboxes.push(rocks::rubble(
                at,
                yaw,
                Vec2::splat(s),
                p.y - s,
                p.y + s,
                color,
            ));
        }
        // Its supports. The belt crosses the rails just short of the bin,
        // so its last bent stands on the frame's end beam over them, not
        // between them.
        let (la, lb) = (
            rig.rot.inverse() * (a - rig.pos),
            rig.rot.inverse() * (b - rig.pos),
        );
        let over_end = (-lz - la.z) / (lb.z - la.z);
        for f in [0.2f32, 0.42, 0.64, over_end] {
            let p = a.lerp(b, f);
            // Not on the line, the bridge's ends or the ramps (where the
            // belt crosses them it spans them).
            let q = Vec2::new(p.x, p.z);
            if f != over_end
                && (lay.track_distance(q) < DECK_HALF + 4.0
                    || lay.ramps.iter().any(|&(s, e, h)| seg(q, s, e).0 < h + 3.0)
                    || self.on_way(q))
            {
                continue;
            }
            let side = if f == over_end {
                rig.rot * Vec3::X
            } else {
                (b - a).cross(Vec3::Y).normalize()
            };
            for s in [-1.0f32, 1.0] {
                let top = p + side * s - Vec3::Y * 0.4;
                let foot = if f == over_end {
                    g + deck
                } else {
                    self.ground_height(top.x, top.z) - 0.3
                };
                self.segment(
                    Vec3::new(top.x, foot, top.z),
                    top,
                    0.18,
                    RUST,
                    Material::Rust,
                    true,
                );
            }
        }
    }

    /// The headframe over the hall's shaft, on the mesa: four legs and two
    /// back braces of steel, a sheave wheel on top, the hoist house behind,
    /// the shaft collar capped with concrete.
    pub(super) fn headframe(&mut self, at: Vec2) {
        let g = self.ground_height(at.x, at.y);
        let top = g + 16.0;
        let hx = 2.6;
        for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let foot = Vec3::new(at.x + sx * hx, g - 0.2, at.y + sz * hx);
            let head = Vec3::new(at.x + sx * 0.9, top, at.y + sz * 0.9);
            self.segment(foot, head, 0.22, RUST, Material::Rust, true);
        }
        for f in [0.3f32, 0.6, 0.85] {
            let y = g + (top - g) * f;
            let r = hx + (0.9 - hx) * f;
            let c = |sx: f32, sz: f32| Vec3::new(at.x + sx * r, y, at.y + sz * r);
            for (a, b) in [
                (c(-1.0, -1.0), c(1.0, -1.0)),
                (c(1.0, -1.0), c(1.0, 1.0)),
                (c(1.0, 1.0), c(-1.0, 1.0)),
                (c(-1.0, 1.0), c(-1.0, -1.0)),
            ] {
                self.segment(a, b, 0.1, RUST, Material::Rust, false);
            }
        }
        // Back braces west, toward the hoist house.
        for sz in [-1.0f32, 1.0] {
            let foot = Vec3::new(at.x - 9.0, g - 0.2, at.y + sz * 2.0);
            self.segment(
                foot,
                Vec3::new(at.x, top - 1.0, at.y + sz * 0.9),
                0.18,
                RUST,
                Material::Rust,
                true,
            );
        }
        let sheave = Vec3::new(at.x, top + 1.4, at.y);
        self.disc(
            sheave,
            Vec3::new(0.0, 0.0, 0.15),
            1.5,
            STEEL,
            Material::Metal,
        );
        self.oboxes.push(OBox {
            center: at,
            half: Vec2::splat(1.2),
            y0: top - 0.2,
            y1: top + 0.25,
            color: STEEL,
            side_mat: Material::Metal,
            top_mat: Material::Metal,
            solid: false,
            ..OBox::default()
        });
        let house = Vec2::new(at.x - 15.0, at.y);
        self.segment(
            sheave - Vec3::Y * 1.5,
            Vec3::new(at.x, g + 0.5, at.y),
            0.04,
            STEEL * 0.6,
            Material::Rust,
            false,
        );
        self.segment(
            sheave,
            Vec3::new(house.x + 3.2, g + 3.6, house.y),
            0.04,
            STEEL * 0.6,
            Material::Rust,
            false,
        );
        self.oboxes.push(OBox {
            center: at,
            half: Vec2::splat(1.6),
            y0: g - 0.3,
            y1: g + 0.22,
            color: CONCRETE * 0.9,
            side_mat: Material::Concrete,
            top_mat: Material::Concrete,
            ramp: true,
            ..OBox::default()
        });
        self.oboxes.push(OBox {
            center: house,
            half: Vec2::new(3.2, 2.6),
            y0: g - 0.3,
            y1: g + 4.2,
            color: RUST,
            side_mat: Material::Painted,
            top_mat: Material::Rust,
            ..OBox::default()
        });
        self.sound(Ambient::Cable, sheave);
    }

    /// Cover on the mesa: low natural outcrops of rock and piles of it,
    /// wherever there's open top (off the caves' roofs, the box cut, the
    /// ways up, the headframe); timber waiting on the landing below.
    pub(super) fn mesa_cover(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let s = lay.size;
        let mut taken: Vec<Vec2> = Vec::new();
        // Off the bots' ways over the top, by more than a rib's length.
        let ways: Vec<Vec2> = self
            .routes
            .iter()
            .flatten()
            .map(|w| Vec2::new(w.x, w.z))
            .collect();
        let spot = |rng: &mut Rng, taken: &mut Vec<Vec2>| {
            (0..200).find_map(|_| {
                let x = rng.range(8.0, s - 8.0);
                let p = Vec2::new(x, rng.range(lay.face(x) + 12.0, s - 6.0));
                let ok = lay.mesa(p) > 0.97
                    && lay.crag_depth(p) < -6.0
                    && lay.cave_distance(p) > CAVE_WALL + 5.0
                    && !within(lay.box_cut, p, 8.0)
                    && lay.ways_up.iter().all(|&(a, b)| seg(p, a, b).0 > 8.0)
                    && p.distance(lay.headframe) > 16.0
                    && ways.iter().all(|q| q.distance(p) > 11.0)
                    && taken.iter().all(|q| q.distance(p) > 14.0);
                ok.then(|| {
                    taken.push(p);
                    p
                })
            })
        };
        // Rock ribs, a few long ones across the top.
        for _ in 0..rng.int(8, 11) {
            let Some(p) = spot(rng, &mut taken) else {
                break;
            };
            let (yaw, len) = (rng.range(-1.0, 1.0), rng.range(3.5, 6.0));
            let g = self.ground_height(p.x, p.y);
            self.stone(
                p,
                yaw,
                Vec2::new(len, rng.range(1.3, 1.8)),
                g - 1.0,
                g + rng.range(1.8, 2.6),
                [STONE, STONE_DARK, STONE_PALE][rng.index(3)] * rng.range(0.9, 1.0),
            );
            if let Some(o) = self.oboxes.last_mut() {
                o.broken = true;
            }
            let q = p + Vec2::new(rng.range(-6.0, 6.0), rng.range(-4.0, 4.0));
            if lay.in_rock(q) && lay.cave_distance(q) > CAVE_WALL + 3.0 {
                let s = rng.range(1.2, 1.8);
                self.boulder(rng, q, s, true);
            }
        }
        for _ in 0..3 {
            let Some(p) = spot(rng, &mut taken) else {
                break;
            };
            let s = rng.range(1.5, 2.1);
            self.rock_pile(rng, p, s);
        }
        // Timber waiting on the landing below the Face, by the west adit.
        let x = lay.west_adit.0.x - 6.0;
        self.crib(rng, Vec2::new(x, lay.face(x) - 8.5), 0.15);
    }

    /// On and round the dump: ore carts tipped over its edge, boulders on
    /// top, spoil.
    pub(super) fn dump_cover(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let (c, _, h, _) = lay.dump;
        for (dx, dz, yaw) in [(-6.0, 4.0, 0.4), (5.0, -3.0, 1.9), (0.0, 9.0, -0.8)] {
            let p = c + Vec2::new(dx, dz);
            self.ore_cart(rng, p, yaw);
        }
        for (dx, dz, s) in [(8.0, 7.0, 1.6), (-9.0, -6.0, 1.8), (2.0, -10.0, 1.4)] {
            self.rock_pile(rng, c + Vec2::new(dx, dz), s);
        }
        // Spoil heaped round its foot.
        for k in 0..10 {
            let a = k as f32 / 10.0 * TAU + rng.range(-0.2, 0.2);
            let p = c + Vec2::new(a.cos(), a.sin()) * rng.range(24.0, 30.0);
            if (p.x - 0.0).min(lay.size - p.x) < 24.0 || keep_clear(lay, p) || !self.room(p, 2.5) {
                continue;
            }
            let s = rng.range(1.0, 1.9);
            self.boulder(rng, p, s, s > 1.0);
        }
        let _ = h;
    }

    /// The warehouse at the end of the branch: a plain steel shed on a
    /// concrete floor, pitched roof on portal frames, a rail door at the
    /// junction's end, a door at the far end and one in each side, all big
    /// enough to drive through. Open-top bins of crystal-rich ore stand
    /// along both walls, a hopper at the buffer stop. Only the plasma lamps,
    /// and the emblem small on the front, say whose it is.
    pub(super) fn warehouse(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let st = self.st;
        let (at, along) = lay.shed;
        let g = self.ground_height(at.x, at.y);
        let yaw = yaw_along(along);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        let id = Quat::IDENTITY;
        let (hw, hl) = (SHED_HALF.x, SHED_HALF.y);
        let (eaves, ridge) = (SHED_EAVES, SHED_RIDGE);
        // Wall half-thickness; the doors' half-widths and height.
        let t = 0.2;
        let (end_door, side_door, door_h) = (5.5, 3.5, 6.0);
        let clad = Vec3::new(0.58, 0.56, 0.5);
        let dado = Vec3::new(0.42, 0.36, 0.3);
        let frame = Vec3::new(0.3, 0.31, 0.33);
        // The floor.
        rig.part(
            self,
            Vec3::new(0.0, -0.14, 0.0),
            Vec3::new(hw + 0.9, 0.18, hl + 0.9),
            id,
            CONCRETE,
            Material::Concrete,
            false,
            0.0,
        );
        // A wall panel: from `a` to `b` (local x, z) along the wall, `y0` to
        // `y1` up, `half_t` thick each side, with a grimy band low down.
        let panel = |m: &mut Map, a: Vec2, b: Vec2, y0: f32, y1: f32| {
            let mid = (a + b) * 0.5;
            let half = ((b - a).abs() * 0.5).max(Vec2::splat(t));
            rig.part(
                m,
                Vec3::new(mid.x, (y0 + y1) * 0.5, mid.y),
                Vec3::new(half.x, (y1 - y0) * 0.5, half.y),
                id,
                clad * (1.0 + 0.03 * (mid.x * 1.7 + mid.y * 0.9).sin()),
                Material::Painted,
                true,
                0.0,
            );
            if y0 < 0.5 {
                let grow = Vec2::splat(0.04);
                let h = ((b - a).abs() * 0.5).max(Vec2::splat(t)) + grow;
                rig.part(
                    m,
                    Vec3::new(mid.x, 0.55, mid.y),
                    Vec3::new(h.x, 0.65, h.y),
                    id,
                    dado,
                    Material::Concrete,
                    false,
                    0.0,
                );
            }
        };
        // The sides, a door in the middle of each.
        for sx in [-1.0f32, 1.0] {
            let x = sx * hw;
            panel(
                self,
                Vec2::new(x, -hl),
                Vec2::new(x, -side_door),
                0.0,
                eaves,
            );
            panel(self, Vec2::new(x, side_door), Vec2::new(x, hl), 0.0, eaves);
            panel(
                self,
                Vec2::new(x, -side_door),
                Vec2::new(x, side_door),
                door_h,
                eaves,
            );
        }
        // The ends, a door in the middle of each.
        for sz in [-1.0f32, 1.0] {
            let z = sz * hl;
            let w = hw - t;
            panel(self, Vec2::new(-w, z), Vec2::new(-end_door, z), 0.0, eaves);
            panel(self, Vec2::new(end_door, z), Vec2::new(w, z), 0.0, eaves);
            panel(
                self,
                Vec2::new(-end_door, z),
                Vec2::new(end_door, z),
                door_h,
                eaves,
            );
            // The gable over it: a plate under each slope of the roof, its
            // lower edge hidden in the wall.
            let rise = ridge - eaves;
            let a = (rise / hw).atan();
            let len = (hw * hw + rise * rise).sqrt();
            let d = rise / a.cos() + 0.2;
            for (sx, th) in [(1.0f32, 0.15), (-1.0, 0.13)] {
                let n = Vec2::new(-sx * a.sin(), -a.cos());
                let mid = Vec2::new(sx * hw * 0.5, (eaves + ridge) * 0.5) + n * (d * 0.5);
                rig.part(
                    self,
                    Vec3::new(mid.x, mid.y, z),
                    Vec3::new(len * 0.5, d * 0.5, th),
                    Quat::from_rotation_z(-sx * a),
                    clad * 0.97,
                    Material::Painted,
                    false,
                    0.0,
                );
            }
        }
        // The roof: two slopes overhanging the walls, a cap along the ridge.
        let rise = ridge - eaves;
        let a = (rise / hw).atan();
        let over = 0.8;
        let len = ((hw + over) * (hw + over) + (rise + over * a.tan()).powi(2)).sqrt();
        for sx in [-1.0f32, 1.0] {
            let up = Vec2::new(sx * a.sin(), a.cos()) * 0.13;
            let mid = Vec2::new(
                sx * (hw + over) * 0.5,
                ridge - (rise + over * a.tan()) * 0.5,
            ) + up;
            rig.part(
                self,
                Vec3::new(mid.x, mid.y, 0.0),
                Vec3::new(len * 0.5, 0.12, hl + 0.7),
                Quat::from_rotation_z(-sx * a),
                Vec3::new(0.44, 0.44, 0.43),
                Material::Painted,
                true,
                0.0,
            );
        }
        rig.part(
            self,
            Vec3::new(0.0, ridge + 0.28, 0.0),
            Vec3::new(0.35, 0.1, hl + 0.75),
            id,
            frame,
            Material::Metal,
            false,
            0.0,
        );
        // Portal frames inside: columns along the walls, a tie across at
        // the eaves, rafters up under the roof. Plasma lamps hang from the
        // ties over the aisles.
        let mut z = -hl + 6.0;
        while z < hl - 1.0 {
            for sx in [-1.0f32, 1.0] {
                if z.abs() > side_door + 0.4 {
                    rig.part(
                        self,
                        Vec3::new(sx * (hw - t - 0.25), eaves * 0.5, z),
                        Vec3::new(0.25, eaves * 0.5, 0.2),
                        id,
                        frame,
                        Material::Metal,
                        true,
                        0.0,
                    );
                }
                let e = rig.point(Vec3::new(sx * (hw - t), eaves - 0.4, z));
                let r = rig.point(Vec3::new(0.0, ridge - 0.4, z));
                self.segment(e, r, 0.18, frame, Material::Metal, false);
            }
            let (a, b) = (
                rig.point(Vec3::new(-(hw - t), eaves - 0.5, z)),
                rig.point(Vec3::new(hw - t, eaves - 0.5, z)),
            );
            self.segment(a, b, 0.12, frame, Material::Metal, false);
            for x in [-4.5f32, 4.5] {
                st.hanging_lamp(self, rig.point(Vec3::new(x, eaves - 0.6, z)), 1.0, 1.0);
            }
            z += 6.0;
        }
        // Outside: a rolled-up door drum over each door, a plasma lamp
        // either side of it.
        for (c, out, w) in [
            (Vec2::new(0.0, -hl), Vec2::new(0.0, -1.0), end_door),
            (Vec2::new(0.0, hl), Vec2::new(0.0, 1.0), end_door),
            (Vec2::new(-hw, 0.0), Vec2::new(-1.0, 0.0), side_door),
            (Vec2::new(hw, 0.0), Vec2::new(1.0, 0.0), side_door),
        ] {
            let face = c + out * (t + 0.4);
            let across = out.perp();
            let (a, b) = (face - across * (w + 0.3), face + across * (w + 0.3));
            self.pipe(
                rig.point(Vec3::new(a.x, door_h + 0.45, a.y)),
                rig.point(Vec3::new(b.x, door_h + 0.45, b.y)),
                0.42,
                frame * 1.2,
                Material::Painted,
                false,
            );
            for s in [-1.0f32, 1.0] {
                let p = c + out * (t + 0.02) + across * s * (w + 1.2);
                let dir = rig.rot * Vec3::new(out.x, 0.0, out.y);
                st.wall_lamp(
                    self,
                    rig.point(Vec3::new(p.x, door_h - 0.2, p.y)),
                    Vec2::new(dir.x, dir.z),
                    1.0,
                );
            }
        }
        // The emblem, small, high on the front over the rail door: plasma
        // held in crystal, an orb in an almond-shaped lens.
        st.emblem(self, &rig, Vec3::new(0.0, eaves + 1.4, -hl - 0.2), 0.7);
        // The bins of ore along the walls (a gap at each side door), one
        // left out in the aisle by the rail door.
        let colours = [
            Vec3::new(0.52, 0.3, 0.16),
            Vec3::new(0.55, 0.47, 0.25),
            Vec3::new(0.38, 0.4, 0.41),
            Vec3::new(0.3, 0.37, 0.31),
        ];
        let mut spots = Vec::new();
        for sx in [-1.0f32, 1.0] {
            for z in [-14.2f32, -8.4, 8.4, 14.2] {
                spots.push((Vec2::new(sx * 9.0, z), rng.range(-0.04, 0.04)));
            }
        }
        spots.push((Vec2::new(-4.6, -9.0), 0.45));
        for (p, turn) in spots {
            let w = rig.point(Vec3::new(p.x, 0.0, p.y));
            let c = colours[rng.index(colours.len())];
            self.ore_bin(rng, Vec2::new(w.x, w.z), yaw + turn, c);
        }
        // A hopper at the buffer stop, and the stop.
        let car = rig.point(Vec3::new(0.0, 0.0, SHED_BUFFER - 5.4));
        self.rail_car(Vec2::new(car.x, car.z), yaw, 0.0, false);
        rig.part(
            self,
            Vec3::new(0.0, 0.7, SHED_BUFFER + 0.3),
            Vec3::new(1.6, 0.7, 0.3),
            id,
            RUST,
            Material::Rust,
            true,
            0.0,
        );
        for sx in [-1.0f32, 1.0] {
            rig.part(
                self,
                Vec3::new(sx * GAUGE * 0.5, 0.85, SHED_BUFFER - 0.15),
                Vec3::new(0.22, 0.22, 0.2),
                id,
                STEEL,
                Material::Metal,
                false,
                0.0,
            );
        }
    }

    /// An open-top steel bin, tank high, heaped with crystal-rich ore:
    /// broken red rock with raw crystal through it.
    pub(super) fn ore_bin(&mut self, rng: &mut Rng, at: Vec2, yaw: f32, colour: Vec3) {
        let g = self.ground_height(at.x, at.y);
        let rig = Rig {
            pos: Vec3::new(at.x, g, at.y),
            rot: Quat::from_rotation_y(yaw),
        };
        let id = Quat::IDENTITY;
        let (hx, hz, h, t) = (1.5f32, 2.4f32, 1.8f32, 0.08f32);
        let paint = colour * rng.range(0.9, 1.05);
        // Floor, sides and ends, a rolled rim on top.
        rig.part(
            self,
            Vec3::new(0.0, 0.12, 0.0),
            Vec3::new(hx, 0.12, hz),
            id,
            paint * 0.8,
            Material::Painted,
            true,
            0.0,
        );
        for s in [-1.0f32, 1.0] {
            rig.part(
                self,
                Vec3::new(s * (hx - t), h * 0.5 + 0.12, 0.0),
                Vec3::new(t, h * 0.5, hz),
                id,
                paint,
                Material::Painted,
                true,
                0.0,
            );
            rig.part(
                self,
                Vec3::new(0.0, h * 0.5 + 0.12, s * (hz - t)),
                Vec3::new(hx - 2.0 * t, h * 0.5, t),
                id,
                paint,
                Material::Painted,
                true,
                0.0,
            );
            rig.part(
                self,
                Vec3::new(s * (hx - t), h + 0.16, 0.0),
                Vec3::new(t + 0.05, 0.06, hz + 0.05),
                id,
                paint * 0.85,
                Material::Painted,
                false,
                0.0,
            );
            // Ribs down the sides, a lifting lug at each end.
            for rz in [-1.2f32, 0.0, 1.2] {
                rig.part(
                    self,
                    Vec3::new(s * (hx + 0.05), h * 0.5 + 0.12, rz),
                    Vec3::new(0.05, h * 0.5, 0.09),
                    id,
                    paint * 0.9,
                    Material::Painted,
                    false,
                    0.0,
                );
            }
            rig.part(
                self,
                Vec3::new(0.0, h + 0.05, s * (hz + 0.1)),
                Vec3::new(0.3, 0.12, 0.08),
                id,
                STEEL,
                Material::Metal,
                false,
                0.0,
            );
        }
        // The ore, heaped just under the rim.
        let top = h + 0.02;
        for _ in 0..7 {
            let p = Vec3::new(
                rng.range(-hx + 0.5, hx - 0.5),
                top - rng.range(0.25, 0.5),
                rng.range(-hz + 0.5, hz - 0.5),
            );
            let s = rng.range(0.25, 0.5);
            let tip = Quat::from_euler(
                glam::EulerRot::YXZ,
                rng.range(0.0, TAU),
                rng.range(-0.5, 0.5),
                rng.range(-0.5, 0.5),
            );
            let shade = rng.range(0.75, 1.05);
            rig.part(
                self,
                p,
                Vec3::new(s, s * 0.7, s * 1.2),
                tip,
                STONE * shade,
                Material::Rubble,
                false,
                0.0,
            );
            self.last_is_rock(rocks::Kind::Rubble);
        }
        rig.part(
            self,
            Vec3::new(0.0, top - 0.55, 0.0),
            Vec3::new(hx - 2.0 * t - 0.02, 0.3, hz - 2.0 * t - 0.02),
            id,
            STONE_DARK,
            Material::Rubble,
            false,
            0.0,
        );
        self.last_is_rock(rocks::Kind::Block);
        for _ in 0..4 {
            let p = Vec3::new(
                rng.range(-hx + 0.4, hx - 0.4),
                top - 0.1,
                rng.range(-hz + 0.4, hz - 0.4),
            );
            let tip = Quat::from_euler(
                glam::EulerRot::YXZ,
                rng.range(0.0, TAU),
                rng.range(-0.6, 0.6),
                rng.range(-0.6, 0.6),
            );
            let s = rng.range(0.1, 0.18);
            rig.part(
                self,
                p,
                Vec3::new(s, s * 2.6, s),
                tip,
                RAW * (0.85 + (s - 0.1) * 3.75),
                Material::Ore,
                false,
                0.0,
            );
        }
    }

    /// Hoodoos among the fins: sandstone pillars, capped with harder rock.
    pub(super) fn fins_and_hoodoos(&mut self, rng: &mut Rng) {
        let lay = self.l;
        for &(p, h) in &lay.hoodoos {
            let (x, z) = (p.x, p.y);
            let g = self.ground_height(x, z);
            let w = rng.range(1.8, 2.4);
            let lean = Vec2::new(rng.range(-0.3, 0.3), rng.range(-0.3, 0.3));
            // Base, waist, cap: narrower in the middle, the soft rock
            // under the cap weathered round.
            let (yaw, color) = (rng.range(0.0, TAU), STONE * rng.range(0.9, 1.05));
            // (A boulder stands on less of its box than a block: sized up to
            // stand on what the hoodoo always has.)
            let w = w * 1.15;
            let base = rocks::boulder(p, yaw, Vec2::splat(w), g - 1.0, g + h * 0.45, color);
            let (yaw, color) = (rng.range(0.0, TAU), STONE_PALE * rng.range(0.9, 1.0));
            let waist = rocks::boulder(
                p + lean,
                yaw,
                Vec2::splat(w * 0.7),
                g + h * 0.4,
                g + h * 0.85,
                color,
            );
            // The cap: a lump of the harder rock, wider than what holds it up.
            let (yaw, color) = (rng.range(0.0, TAU), STONE_DARK * rng.range(0.9, 1.05));
            let half = Vec2::new(w * 1.15, w * 0.95);
            let cap = rocks::boulder(p + lean * 1.6, yaw, half, g + h * 0.82, g + h, color);
            self.oboxes.extend([base, waist, cap]);
        }
        // The fins' rock, standing out of the sand drifted against them.
        for &(a, b, _) in &lay.fins {
            let len = a.distance(b);
            let dir = (b - a) / len;
            let mut t = 0.8;
            while t < len - 0.8 {
                let piece = rng.range(2.2, 3.6).min(len - 0.8 - t);
                let p = a + dir * (t + piece * 0.5) + dir.perp() * rng.range(-0.3, 0.3);
                let g = self.ground_height(p.x, p.y);
                let rise = lay.fins(p);
                self.stone(
                    p,
                    yaw_along(dir) + rng.range(-0.1, 0.1),
                    Vec2::new(rng.range(0.9, 1.3), piece * 0.5 + 0.2),
                    g - rise - 0.3,
                    g + rng.range(0.6, 1.8),
                    [STONE, STONE_DARK, STONE_PALE][rng.index(3)] * rng.range(0.88, 1.02),
                );
                if let Some(o) = self.oboxes.last_mut() {
                    o.broken = true;
                }
                t += piece;
            }
        }
        // Blocks fallen off the fins.
        for &(a, b, _) in &lay.fins {
            for _ in 0..2 {
                let t = rng.range(0.2, 0.8);
                let side = (b - a).normalize().perp() * if rng.chance(0.5) { 1.0 } else { -1.0 };
                let p = a.lerp(b, t) + side * rng.range(4.5, 6.5);
                let s = rng.range(1.0, 1.8);
                if keep_clear(lay, p) || !self.room(p, s + 1.0) {
                    continue;
                }
                self.boulder(rng, p, s, s > 1.0);
            }
        }
    }

    /// Cover across the rest of the flats and the landing under the Face:
    /// rock piles, crystal outcrops, timber cribs.
    pub(super) fn flats_cover(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let s = lay.size;
        let mut taken: Vec<Vec2> = Vec::new();
        // Somewhere below the Face with room for a thing `r` across, off
        // what's kept clear, `gap` from the others put down here.
        let spot = |d: &Dig, rng: &mut Rng, taken: &mut Vec<Vec2>, r: f32, gap: f32| {
            (0..200).find_map(|_| {
                let x = rng.range(10.0, s - 10.0);
                let p = Vec2::new(x, rng.range(10.0, lay.face(x) - 8.0));
                let ok = !keep_clear(lay, p)
                    && d.room(p, r)
                    && taken.iter().all(|q| q.distance(p) > gap);
                ok.then(|| {
                    taken.push(p);
                    p
                })
            })
        };
        for _ in 0..13 {
            let size = rng.range(1.4, 1.8);
            let Some(p) = spot(self, rng, &mut taken, size * 1.4 + 1.0, 16.0) else {
                break;
            };
            self.rock_pile(rng, p, size);
        }
        for _ in 0..4 {
            let h = rng.range(2.6, 3.4);
            let Some(p) = spot(self, rng, &mut taken, 3.0, 14.0) else {
                break;
            };
            self.outcrop_rock(p, h);
        }
        // Timber cribs: stacked pit props waiting by the west adit's mouth,
        // and by the line, either side of the bridge's end.
        let (_, end) = lay.bridge();
        let (p3, dir) = lay.track_at(lay.track_len(3) + 12.0);
        let x = lay.west_adit.0.x - 2.0;
        let landing = (lay.face(x) + lay.wash_frame(lay.wash_at_x(x)).0.y) * 0.5;
        for (p, yaw) in [
            (p3 - dir.perp() * 7.0 * lay.branch_side, -0.2),
            (Vec2::new(x, landing), 1.2),
            (end + dir.perp() * 14.0, 0.4),
        ] {
            if !keep_clear(lay, p) && lay.wash(p) > -0.3 && self.room(p, 2.5) {
                self.crib(rng, p, yaw);
            }
        }
    }

    /// A timber crib: pit props stacked crosswise, four courses high.
    pub(super) fn crib(&mut self, rng: &mut Rng, p: Vec2, yaw: f32) {
        if self.in_the_way(p, 2.4) {
            return;
        }
        let g = self.ground_height(p.x, p.y);
        for k in 0..4 {
            let y = g + k as f32 * 0.5;
            let along = if k % 2 == 0 { yaw } else { yaw + FRAC_PI_2 };
            for s in [-1.0f32, 1.0] {
                let off = Vec2::from_angle(-along).rotate(Vec2::new(s * 1.3, 0.0));
                self.oboxes.push(OBox {
                    center: p + off,
                    yaw: along,
                    half: Vec2::new(0.3, 1.8),
                    y0: y - 0.2 - 0.01 * k as f32,
                    y1: y + 0.5,
                    color: TIMBER * rng.range(0.85, 1.05),
                    side_mat: Material::Wood,
                    top_mat: Material::Wood,
                    ..OBox::default()
                });
            }
        }
    }

    /// The top of what a tank stands on at `p`, from above: a cave roof,
    /// or the ground.
    pub(super) fn level(&self, p: Vec2) -> f32 {
        let lay = self.l;
        if lay.roofed(p) {
            if lay.under_crag(p) { CRAG_ROOF } else { ROOF.1 }
        } else {
            self.ground_height(p.x, p.y)
        }
    }

    /// Near a point on one of the ways round the bots take, or the Hold
    /// site (kept open).
    /// Nothing to keep clear for driving within `r` of `at`: no way round,
    /// ramp or ford, track, the run out of the warehouse's doors, or the
    /// foot of the Face.
    pub(super) fn room(&self, at: Vec2, r: f32) -> bool {
        let lay = self.l;
        (0..9).all(|k| {
            let q = if k == 0 {
                at
            } else {
                let a = k as f32 / 8.0 * TAU;
                at + Vec2::new(a.cos(), a.sin()) * r
            };
            let f = lay.face(q.x);
            !self.on_way(q)
                && lay.track_distance(q) > 6.0
                && lay.shed_distance(q) > 14.0
                && !(q.y > f - 8.0 && q.y < f + FACE_RISE + 5.0)
                && !lay
                    .ramps
                    .iter()
                    .chain(&lay.fords)
                    .any(|&(s, e, h)| seg(q, s, e).0 < h + 4.0)
                && !lay.ways_up.iter().any(|&(a, b)| seg(q, a, b).0 < 6.0)
        })
    }

    /// Something `r` across at `p` would be in the way of driving: on or
    /// by one of the bots' ways round (its points or the stretches between
    /// them), a haul ramp, a crossing or a way up. Loose cover (boulders,
    /// carts, cribs, crystal) is never put there.
    pub(super) fn in_the_way(&self, p: Vec2, r: f32) -> bool {
        let lay = self.l;
        let flat = |w: &Vec3| Vec2::new(w.x, w.z);
        self.routes.iter().any(|way| {
            way.iter().any(|w| flat(w).distance(p) < r + 3.5)
                || way
                    .windows(2)
                    .any(|w| seg(p, flat(&w[0]), flat(&w[1])).0 < r + 3.0)
        }) || lay
            .ramps
            .iter()
            .chain(&lay.fords)
            .any(|&(a, b, h)| seg(p, a, b).0 < r + h + 1.5)
            || lay.ways_up.iter().any(|&(a, b)| seg(p, a, b).0 < r + 4.0)
    }

    pub(super) fn on_way(&self, q: Vec2) -> bool {
        self.routes
            .iter()
            .flatten()
            .any(|w| Vec2::new(w.x, w.z).distance(q) < 4.5)
            || self.hold_site.is_some_and(|h| h.distance(q) < 7.0)
    }

    /// Something solid to hide behind near `p`, at the ground's level.
    pub(super) fn cover_near(&self, p: Vec2, r: f32) -> bool {
        let g = self.level(p);
        self.oboxes.iter().any(|o| {
            let (lo, hi) = o.y_range();
            o.solid && !o.ramp && hi > g + 1.4 && lo < g + 2.0 && o.distance(p) < r
        }) || self.crystals.iter().any(|c| c.solid && c.distance(p) < r)
            || {
                // A bank or a rise of the ground itself.
                (0..8).any(|k| {
                    let a = k as f32 * TAU / 8.0;
                    let q = p + Vec2::new(a.cos(), a.sin()) * (r * 0.5);
                    self.ground_height(q.x, q.y) > g + 2.0
                })
            }
    }

    /// Fill the gaps: wherever open ground is more than `REACH` m from
    /// anything to hide behind, put something there, in keeping with where
    /// it is. Natural rock on the mesa, in the east and in the wash, the
    /// mine's gear and rock in the west and by the line.
    pub(super) fn fill_cover(&mut self, rng: &mut Rng) {
        let lay = self.l;
        const REACH: f32 = 10.0;
        let mut pts = Vec::new();
        let mut z = 6.0;
        while z < lay.size - 6.0 {
            let mut x = 6.0;
            while x < lay.size - 6.0 {
                pts.push(Vec2::new(x, z));
                x += 4.0;
            }
            z += 4.0;
        }
        // Visit in a random order so the fill isn't laid out in rows.
        for i in (1..pts.len()).rev() {
            let j = rng.index(i + 1);
            pts.swap(i, j);
        }
        for p in pts {
            if keep_clear(lay, p) || self.cover_near(p, REACH) {
                continue;
            }
            // Near it, off what's kept clear: a few tries.
            let Some(q) = (0..10)
                .map(|k| {
                    let r = 2.0 + k as f32;
                    p + Vec2::new(rng.range(-r, r), rng.range(-r, r))
                })
                .find(|&q| !keep_clear(lay, q) && !self.on_way(q))
            else {
                continue;
            };
            let g = self.level(q);
            let roll = rng.f32();
            if lay.roofed(q) {
                let s = rng.range(1.4, 2.1);
                self.rock_pile(rng, q, s);
            } else if g < -3.0 {
                if roll < 0.5 {
                    {
                        let v = rng.range(2.4, 3.6);
                        self.outcrop_rock(q, v);
                    }
                } else {
                    let s = rng.range(1.4, 2.0);
                    self.rock_pile(rng, q, s);
                }
            } else if g > TOP * 0.7 || q.x > lay.crag_e {
                let s = rng.range(1.4, 2.1);
                self.rock_pile(rng, q, s);
            } else if roll < 0.3 {
                {
                    let v = rng.range(0.0, TAU);
                    self.ore_cart(rng, q, v);
                }
                let s = rng.range(0.8, 1.2);
                self.boulder(rng, q + Vec2::new(2.6, 1.0), s, true);
            } else if roll < 0.6 {
                {
                    let v = rng.range(0.0, PI);
                    self.crib(rng, q, v);
                }
            } else if roll < 0.8 {
                {
                    let v = rng.range(2.2, 3.2);
                    self.outcrop_rock(q, v);
                }
            } else {
                let s = rng.range(1.4, 2.0);
                self.rock_pile(rng, q, s);
            }
        }
    }

    /// More boulders (#281): over the open ground, wherever there's room
    /// and nothing else close, a boulder bigger than a tank, now and then
    /// with a second one fallen against it. Each keeps a tank's width and
    /// more from everything else, so the ground stays drivable between.
    pub(super) fn boulder_fields(&mut self, rng: &mut Rng) {
        let lay = self.l;
        const STEP: f32 = 6.0;
        // Clear ground between a new rock and anything else: wider than
        // the widest hull (4.3 m).
        const GAP: f32 = 5.0;
        let mut z = STEP * 0.5;
        while z < lay.size {
            let mut x = STEP * 0.5;
            while x < lay.size {
                let p = Vec2::new(x, z) + Vec2::new(rng.range(-2.5, 2.5), rng.range(-2.5, 2.5));
                let s = rng.range(1.8, 2.8);
                if rng.chance(0.9) && self.boulder_room(p, s, GAP, None) {
                    self.boulder(rng, p, s, true);
                    if rng.chance(0.5) {
                        let a = rng.range(0.0, TAU);
                        let q = p + Vec2::new(a.cos(), a.sin()) * s * rng.range(1.3, 1.6);
                        let t = rng.range(1.8, 2.3);
                        // Fallen against the first, clear of all else.
                        let first = self.oboxes.len() - 1;
                        if self.boulder_room(q, t, GAP, Some(first)) {
                            self.boulder(rng, q, t, true);
                        }
                    }
                }
                x += STEP;
            }
            z += STEP;
        }
    }

    /// Room for a boulder of half-width `s` at `p`, with `gap` of clear
    /// ground round it: off everything kept clear for driving, out of the
    /// caves, on fairly flat ground, and that far from any other piece.
    /// (`except`: a piece it may lean on.)
    pub(super) fn boulder_room(&self, p: Vec2, s: f32, gap: f32, except: Option<usize>) -> bool {
        let lay = self.l;
        let r = s * 1.1;
        if keep_clear(lay, p) || lay.roofed(p) || lay.in_cave(p) || !self.room(p, r + 3.0) {
            return false;
        }
        let g = self.ground_height(p.x, p.y);
        let flat = (0..8).all(|k| {
            let a = k as f32 * TAU / 8.0;
            let q = p + Vec2::new(a.cos(), a.sin()) * (r + 1.5);
            !lay.roofed(q) && (self.ground_height(q.x, q.y) - g).abs() < 1.2
        });
        if !flat {
            return false;
        }
        let near = r + gap;
        let clamp = |min: Vec3, max: Vec3| {
            max.y > g - 1.0
                && min.y < g + 6.0
                && (p.clamp(Vec2::new(min.x, min.z), Vec2::new(max.x, max.z)) - p).length() < near
        };
        !self.oboxes.iter().enumerate().any(|(i, o)| {
            if Some(i) == except {
                return false;
            }
            let (lo, hi) = o.y_range();
            hi > g - 1.0 && lo < g + 6.0 && o.distance(p) < near
        }) && !self.crystals.iter().any(|c| c.distance(p) < near)
            && !self.solids.iter().any(|b| clamp(b.min, b.max))
            && !self.boxes.iter().any(|b| clamp(b.min, b.max))
    }

    /// Take out the stand-ins for the blocks along the Face (see
    /// `rough_face`).
    pub(super) fn clear_face_blocks(&mut self) {
        self.oboxes.retain(|o| !(o.hidden && o.color == FACE_BLOCK));
    }

    /// The edge: a wall of boulders all the way round, on whatever ground
    /// it meets (the flats, the mesa, the adit's floor).
    pub(super) fn edge_rocks(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let shades = [STONE, STONE_DARK, STONE_PALE];
        for (p, inward) in self.edge_points(3.2, 2.0) {
            let at = p + inward * rng.range(0.4, 1.0);
            let q = at.clamp(Vec2::splat(0.5), Vec2::splat(lay.size - 0.5));
            if lay.crag_depth(q) > 1.5 {
                continue;
            }
            let g = self.ground_height(q.x, q.y);
            // Lower inside the adit, under its roof.
            let tall = if lay.cave_distance(q) < 1.0 {
                5.5
            } else {
                rng.range(5.0, 8.0)
            };
            let size = rng.range(3.2, 4.2);
            // Down to the lowest ground under it (on a slope, the wash's
            // bank, its low side mustn't leave a gap under it).
            let low = (0..8)
                .map(|k| {
                    let a = k as f32 * TAU / 8.0;
                    let p = at + Vec2::new(a.cos(), a.sin()) * size * 1.4;
                    self.ground_height(p.x, p.y)
                })
                .fold(g, f32::min);
            let yaw = rng.range(0.0, TAU);
            let half = Vec2::new(size, size * rng.range(0.85, 1.0));
            let color = shades[rng.index(shades.len())] * rng.range(0.85, 1.05);
            self.oboxes.push(rocks::boulder(
                at,
                yaw,
                half,
                low.min(g - 1.5),
                g + tall,
                color,
            ));
        }
    }

    /// The ways round for bots, on every level: over the rail bridge into
    /// the mine and along the wash's floor under it, through the mine (in at the west
    /// adit, through the hall, along the drift, out at the breach), along
    /// the Crag's foot into the new heading, and down the box cut into the
    /// mine.
    pub(super) fn mine_routes(&mut self) {
        let lay = self.l;
        // On whatever's underfoot seen from above (the flats, the wash's
        // floor, the mesa's top), on the bridge's deck, in the mine (under
        // its roof).
        let g = |m: &Dig, x: f32, z: f32| {
            let p = m.open_near(Vec2::new(x, z), 3.5).unwrap_or(Vec2::new(x, z));
            Vec3::new(p.x, m.ground_height(p.x, p.y), p.y)
        };
        let (a, b) = lay.bridge();
        let b_end = b;
        let deck = |t: f32| {
            let p = a.lerp(b, t);
            Vec3::new(p.x, DECK_TOP, p.y)
        };
        let off_deck = |m: &Dig, t: f32| {
            let p = a.lerp(b, t);
            g(m, p.x, p.y)
        };
        let mine = |x: f32, z: f32| Vec3::new(x, 0.0, z);
        let top = |x: f32, z: f32| Vec3::new(x, TOP, z);
        let m = &*self;
        let at = |p: Vec2| g(m, p.x, p.y);
        // On the wash's floor `u` m along it: down its middle, or across to
        // the far side from the cutting rig where it's working.
        let wash = |u: f32| {
            let (p, t) = line_at(&lay.wash, u);
            let p = if p.distance(lay.wash_rig.0) < 10.0 {
                p + Vec2::new(t.y, -t.x) * 4.0
            } else {
                p
            };
            at(p)
        };
        let mid = |r: Rect| (r.0 + r.1) * 0.5;
        let b = lay.bridge_s;
        let ax = mid(lay.adit).x;
        let (wa, br, hd, bc) = (
            mid(lay.west_adit),
            mid(lay.breach),
            lay.heading,
            lay.box_cut,
        );
        let drift_z = mid(lay.drift).y;
        let pillars = lay.pillars.iter().sum::<Vec2>() / 4.0;
        let cross = mid(lay.crosscut);
        // On the landing under the Face (the wash's bank can come close).
        let west_mouth = Vec2::new(wa.x, lay.face(wa.x) - 6.0);
        // Level ground on the landing east of the breach's mouth.
        let east_landing = {
            let level = |p: Vec2| {
                let g = lay.ground(p.x, p.y);
                lay.mesa(p) < 0.02
                    && lay.cave_distance(p) > 3.5
                    && lay.wash(p) > -0.1
                    && (0..8).all(|k| {
                        let a = k as f32 * TAU / 8.0;
                        let q = p + Vec2::new(a.cos(), a.sin()) * 4.0;
                        (lay.ground(q.x, q.y) - g).abs() < 0.5
                    })
            };
            let want = Vec2::new(br.x + 7.0, lay.face(br.x + 7.0) - 6.0);
            (0..12)
                .flat_map(|i| (0..8).map(move |j| (i, j)))
                .map(|(i, j)| {
                    Vec2::new(
                        br.x + 5.0 + i as f32,
                        lay.face(br.x + 5.0 + i as f32) - 5.0 - j as f32,
                    )
                })
                .filter(|&p| level(p))
                .min_by(|p, q| p.distance(want).total_cmp(&q.distance(want)))
                .unwrap_or(want)
        };
        let s_ramp1 = lay.wash_near(lay.ramps[1].1).1;
        let (s0, s1) = (
            lay.wash_near(lay.fords[0].0.lerp(lay.fords[0].1, 0.5)).1,
            lay.wash_near(lay.fords[1].0.lerp(lay.fords[1].1, 0.5)).1,
        );
        // Along the wash's floor between two places on it, every 25 m or so.
        let along = |from: f32, to: f32| {
            let n = ((to - from) / 25.0).ceil().max(1.0) as usize;
            (0..=n)
                .map(|k| wash(from + (to - from) * k as f32 / n as f32))
                .collect::<Vec<_>>()
        };
        let mut under = vec![at(lay.ramps[0].0), at(lay.ramps[0].1)];
        under.extend(along(b - 15.0, s_ramp1.max(b + 10.0)));
        under.extend([at(lay.ramps[1].1), at(lay.ramps[1].0)]);
        let mut wash_way = vec![at(lay.fords[0].0), wash(s0)];
        wash_way.extend(along(s0 + 10.0, s1 - 4.0));
        wash_way.extend([
            wash(s1),
            at(lay.fords[1]
                .0
                .lerp(lay.fords[1].1, 0.5)
                .lerp(lay.fords[1].0, 0.55)),
        ]);
        // Off the bridge's end, on the line short of the tipple.
        let flats = b_end + (b_end - a).normalize() * 5.0;
        let ways = vec![
            // From the flats over the deck and in at the adit's mouth (and
            // the other way, out of the mine onto the flats).
            vec![
                at(flats),
                off_deck(m, 1.15),
                deck(0.8),
                deck(0.2),
                mine(ax, a.y + 7.0),
                mine(ax, a.y + 21.0),
            ],
            // Along the wash's floor under the bridge: down the near ramp,
            // between the piers, up the far one.
            under,
            // The wash end to end, on its floor: down the west crossing by
            // the west spawns, along, up the east one by the east camp.
            wash_way,
            // Through the mine: in at the west adit, through the hall and
            // the crosscut, along the drift, out at the breach.
            vec![
                at(west_mouth),
                mine(wa.x, lay.hall.0.y - 5.0),
                mine(pillars.x, pillars.y),
                mine(cross.x, cross.y),
                mine(ax, drift_z),
                mine((ax + br.x) * 0.5, drift_z),
                mine(br.x, lay.drift.0.y - 6.0),
                mine(br.x, lay.breach.0.y + 6.0),
                at(east_landing),
            ],
            // From the east landing along the Crag's foot and into the new
            // heading.
            vec![
                at(east_landing),
                at(Vec2::new(hd.1.x + 4.0, lay.face(hd.1.x + 4.0) - 5.0)),
                at(Vec2::new(mid(hd).x, lay.face(mid(hd).x) - 5.5)),
                mine(hd.0.x + 3.0, lay.face(hd.0.x + 3.0) + 1.0),
            ],
            // Off the mesa's east down the box cut into the mine, and out
            // at the west adit.
            vec![
                top(bc.1.x + 26.0, lay.size - 24.0),
                top(bc.1.x + 3.0, mid(bc).y),
                at(mid(bc)),
                mine(lay.adit.1.x + 2.0, mid(bc).y),
                mine(ax, drift_z + 1.0),
                mine(cross.x, cross.y),
                mine(pillars.x, pillars.y),
                mine(wa.x, lay.hall.0.y - 8.0),
                at(west_mouth),
            ],
        ];
        self.routes = ways;
    }

    pub(super) fn mine_spawns(&mut self) {
        let lay = self.l;
        // The west side south of the wash's west arm, the east side north
        // of its east arm.
        for side in 0..self.bases.len().min(2) {
            self.bases[side] = lay.spawn_points(side);
        }
        // Each place bots head for at the clear ground nearest where it's
        // meant to be.
        self.open_spots = lay
            .spots
            .iter()
            .filter_map(|&p| self.open_near(p, TANK_CLEAR))
            // Not up on the Crag, where nothing gets.
            .filter(|&q| lay.crag_depth(q) < -4.0)
            .collect();
        // Nor anywhere else nobody can drive to (a corner shut in by the
        // edge's rocks): each one a drive from the first side's spawns.
        if let Some(from) = self.bases.first().and_then(|b| b.first()).copied() {
            let nav = crate::ai::nav::NavGrid::build(self);
            let spots = std::mem::take(&mut self.open_spots);
            self.open_spots = spots
                .into_iter()
                .filter(|&q| !nav.path(from, q).is_empty())
                .collect();
        }
        // Repair drops come down out of the sky: open over them. Four, the
        // first wanted places that have room.
        self.repair_spots = lay
            .repairs
            .iter()
            .filter_map(|&p| {
                (0..60).find_map(|k| {
                    let a = k as f32 * 2.4;
                    let q = p + Vec2::new(a.cos(), a.sin()) * (k as f32 * 0.3);
                    let g = self.ground_height(q.x, q.y);
                    let level = (0..12).all(|k| {
                        let a = k as f32 * TAU / 12.0;
                        let r = q + Vec2::new(a.cos(), a.sin()) * 5.0;
                        (self.ground_height(r.x, r.y) - g).abs() < 0.6
                    });
                    let sky = !self
                        .oboxes
                        .iter()
                        .any(|o| o.ray_hit(Vec3::new(q.x, g + 0.5, q.y), Vec3::Y, 80.0));
                    let open = self.mine_clear(q, TANK_CLEAR)
                        && sky
                        && level
                        && !lay.roofed(q)
                        && !lay.in_cave(q)
                        && !lay.on_bridge(q)
                        && lay.wash(q) > -0.3;
                    open.then_some(q)
                })
            })
            .take(4)
            .collect();
        for side in 0..self.bases.len().min(2) {
            let base = std::mem::take(&mut self.bases[side]);
            self.bases[side] = base
                .into_iter()
                .filter(|&p| self.mine_clear(p, TANK_CLEAR))
                .collect();
        }
    }

    /// The ground nearest `p` (within 10 m) with nothing solid within `r`,
    /// if there's any.
    pub(super) fn open_near(&self, p: Vec2, r: f32) -> Option<Vec2> {
        (0..40).find_map(|k| {
            let a = k as f32 * 2.4;
            let q = p + Vec2::new(a.cos(), a.sin()) * (k as f32 * 0.25);
            self.mine_clear(q, r).then_some(q)
        })
    }

    /// Nothing solid within `r` of a ground point, at the ground's height.
    pub(super) fn mine_clear(&self, p: Vec2, r: f32) -> bool {
        let g = self.ground_height(p.x, p.y);
        !self.oboxes.iter().any(|o| {
            let (lo, hi) = o.y_range();
            o.solid && lo < g + 3.0 && hi > g + 0.3 && o.distance(p) < r
        }) && !self.crystals.iter().any(|c| c.solid && c.distance(p) < r)
    }

    /// Beyond the edge: the desert rising into broken hills, and buttes.
    pub(super) fn mine_horizon(&mut self, rng: &mut Rng) {
        let lay = self.l;
        let (cell, reach) = (4.0f32, 520.0f32);
        let n = ((lay.size + 2.0 * reach) / cell) as usize + 1;
        let mut t = Terrain::from_fn(Vec2::splat(-reach), cell, n, n, |x, z| {
            lay.scenery_ground(x, z)
        });
        t.color = SAND * 0.95;
        t.material = Material::Sand;
        self.scenery_terrain = Some(t);
        for _ in 0..40 {
            let a = rng.range(0.0, TAU);
            let r = rng.range(190.0, 460.0);
            let at = lay.center() + Vec2::new(a.cos(), a.sin()) * r;
            let g = lay.scenery_ground(at.x, at.y) - 2.0;
            let w = rng.range(10.0, 30.0);
            let h = rng.range(12.0, 40.0);
            let yaw = rng.range(0.0, TAU);
            let half = Vec2::new(w, w * rng.range(0.5, 0.9));
            let color = [STONE, STONE_DARK, STONE_PALE][rng.index(3)] * rng.range(0.85, 1.0);
            // Buttes: flat-topped, broken off a mesa long ago.
            self.oboxes.push(OBox {
                solid: false,
                broken: true,
                ..rocks::boulder(at, yaw, half, g, g + h, color)
            });
        }
    }
}

/// The Crag's foot, end to end: down its west side from past the north
/// edge, along the Face, up its east side to the box cut, in round the
/// slot's walls, on up to past the edge.
fn crag_feet(m: &Dig) -> Vec<crag::Foot> {
    let lay = m.l;
    let mut corners = Vec::new();
    let zw = lay.face(lay.crag_w);
    let mut z = lay.size + 6.0;
    while z > zw + 3.0 {
        corners.push(Vec2::new(lay.crag_west(z), z));
        z -= 3.0;
    }
    let (xw, xe) = (lay.crag_west(zw), lay.crag_east(lay.face(lay.crag_e)));
    let mut x = xw;
    while x < xe {
        corners.push(Vec2::new(x, lay.face(x)));
        x += 2.0;
    }
    let ze = lay.face(xe);
    let (s0, s1) = (lay.box_cut.0.y, lay.box_cut.1.y);
    let mut z = ze + 3.0;
    while z < s0 {
        corners.push(Vec2::new(lay.crag_east(z), z));
        z += 3.0;
    }
    corners.push(Vec2::new(lay.crag_east(s0), s0));
    corners.push(Vec2::new(lay.box_cut.0.x, s0));
    corners.push(Vec2::new(lay.box_cut.0.x, s1));
    corners.push(Vec2::new(lay.crag_east(s1), s1));
    let mut z = s1 + 3.0;
    while z < lay.size + 6.0 {
        corners.push(Vec2::new(lay.crag_east(z), z));
        z += 3.0;
    }
    let pts = crag::resample(&corners, 0.7, 1.2);
    let n = pts.len();
    let mut along = 0.0;
    let mut feet = Vec::with_capacity(n);
    for i in 0..n {
        let (p, dir) = pts[i];
        if i > 0 {
            along += p.distance(pts[i - 1].0);
        }
        let nrm = Vec2::new(dir.y, -dir.x);
        let in_slot = p.y > s0 - 0.6 && p.y < s1 + 0.6 && p.x < lay.crag_east(p.y) - 0.5;
        let on_front = nrm.y < -0.7 && p.y < lay.front_limit();
        let mouth = (on_front && lay.mouths().iter().any(|(a, b)| p.x > a.x && p.x < b.x))
            || (in_slot && nrm.x > 0.7 && p.y > s0 + 0.2 && p.y < s1 - 0.2);
        let q = p + nrm * 1.2;
        let base = m.level(q);
        // How sharply the foot turns here, over a few meters.
        let (d0, d1) = (pts[i.saturating_sub(6)].1, pts[(i + 6).min(n - 1)].1);
        let bend = 1.0 - smoothstep(0.25, 1.0, d0.angle_to(d1).abs());
        let by_mouth = lay
            .mouths()
            .iter()
            .map(|(a, b)| (a.x - p.x).max(p.x - b.x).max(0.0))
            .fold(f32::INFINITY, f32::min);
        let plain = if on_front {
            smoothstep(0.5, 5.0, by_mouth)
        } else {
            1.0
        };
        let lip = if in_slot {
            CRAG_TOP
        } else {
            CRAG_TOP
                + 1.6 * ((noise(along / 7.0, 14) * 4.0).floor() / 4.0 - 0.4)
                + 0.9 * (noise(along / 4.0, 16) - 0.5)
                - 3.5 * smoothstep(0.78, 0.92, noise(along / 16.0, 17))
        };
        let cut = if in_slot {
            CRAG_TOP + 1.0
        } else if on_front {
            base + CRAG_CUT + 2.5 * (noise(along / 13.0, 21) - 0.5)
        } else {
            -100.0
        };
        feet.push(crag::Foot {
            p,
            nrm,
            along,
            floor: if mouth { ROOF.0 } else { base },
            lip,
            cut,
            mouth,
            relief: bend * plain * if in_slot { 0.35 } else { 1.0 },
        });
    }
    feet
}

/// Ground the cover fill leaves open: the spawns and the camps, the track
/// and the bridge, the ramps and crossings, the wash's banks, the mine's
/// corridors, the box cut and the run out of its top, the scree's way up, the top of the Face.
pub(super) fn keep_clear(lay: &Layout, p: Vec2) -> bool {
    let spawns = lay.by_spawns(p);
    let (d, along) = lay.wash_near(p);
    let (depth, half) = lay.wash_profile(along);
    let bank = d > half - 1.5 && d < half + WASH_BANK + 2.5;
    // The way along the wash's floor, down its middle.
    let floor_way = depth > 1.0 && d < 4.0;
    let f = lay.face(p.x);
    spawns
        || bank
        || floor_way
        || lay.track_distance(p) < 6.0
        || lay.shed_distance(p) < 6.0
        || lay.shed_doorways(p)
        || seg(p, lay.bridge().0, lay.bridge().1).0 < DECK_HALF + 5.0
        || lay
            .ramps
            .iter()
            .chain(&lay.fords)
            .any(|&(s, e, h)| seg(p, s, e).0 < h + 4.0)
        || lay.cave_distance(p) < 4.0 && lay.mesa(p) > 0.3 && !lay.roofed(p)
        || within(
            (lay.box_cut.0, lay.box_cut.1 + Vec2::new(14.0, 0.0)),
            p,
            4.0,
        )
        || lay.crag_depth(p) > -4.0
        || (p.y > f - 8.0 && p.y < f + FACE_RISE + 5.0)
        || lay.ways_up.iter().any(|&(a, b)| seg(p, a, b).0 < 6.0)
        || lay.spots.iter().any(|&q| p.distance(q) < 6.5)
        || lay.repairs.iter().any(|&q| p.distance(q) < 6.0)
        || p.min_element() < 7.0
        || p.max_element() > lay.size - 7.0
}

/// A stream of its own for the thing at `at`.
pub(super) fn own_rng(at: Vec2) -> Rng {
    Rng::new(((at.x.to_bits() as u64) << 32) | at.y.to_bits() as u64)
}

/// The tipple's bin: plain mine steel, edges worn bright.
const BIN: Skin = Skin::new(
    Vec4::new(STEEL.x, STEEL.y, STEEL.z, Material::Metal.id()),
    Vec4::new(0.34, 0.33, 0.33, Material::Metal.id()),
);

/// Ore from the Crag's red beds.
pub(super) const ORE: Vec3 = Vec3::new(0.36, 0.15, 0.09);

/// The open space underground, as a distance (negative inside): the
/// caves and the box cut, their corners rounded, and the air out past the
/// Face.
fn open_sdf(lay: &Layout, p: Vec2) -> f32 {
    let mut d = p.y - (lay.face(p.x) + FACE_RISE * 0.5);
    d = d.min(vault::rect_sdf(lay.box_cut, 0.3, p));
    for &r in &lay.caves {
        // The bore is round: its end too.
        let round = if r == lay.bore { 3.0 } else { 0.8 };
        d = vault::smin(d, vault::rect_sdf(r, round, p), 1.0);
    }
    d
}

/// How far the walls arch over into the roof: wide in the high rooms,
/// right over in the bore (it's round).
fn arch(lay: &Layout, c: Vec2) -> f32 {
    let near = |r: (Vec2, Vec2), w: f32| 1.0 - smoothstep(0.0, w, rect_distance(r, c));
    let mut a: f32 = 2.2;
    a = a.max(2.2 + 1.7 * near(lay.bore, 1.0));
    a = a.max(2.2 + 1.6 * near(lay.bay, 2.0));
    a = a.max(2.2 + 1.0 * near(lay.hall, 2.0) * smoothstep(3.0, 6.0, lay.crag_depth(c)));
    a
}

/// The veins of raw crystal showing in the mine's walls at edge point
/// `c`: two, low and high, pinching out and swelling, thick where the borer
/// is cutting (they bore where the seam is richest); none in the
/// Resonant's breach.
fn rock_veins(lay: &Layout, c: Vec2) -> [vault::Vein; 2] {
    let mut out = [vault::Vein::default(); 2];
    if rect_distance(lay.caves[5], c) < 1.0 {
        return out;
    }
    let u = c.x + c.y;
    let rich = 1.0 - smoothstep(2.0, 8.0, rect_distance(lay.bay, c));
    for (k, v) in out.iter_mut().enumerate() {
        let k = k as u32;
        let y = [1.5, 4.4][k as usize] + 1.6 * (noise(u / 13.0, 30 + k) * 2.0 - 1.0);
        let present = smoothstep(
            0.45,
            0.65,
            noise(u / 15.0 + 3.7 * k as f32, 34 + k) + 0.35 * rich,
        );
        let half = (0.08 + 0.3 * noise(u / 6.0, 32 + k) + 0.25 * rich) * present;
        v.y = y;
        v.half = half;
    }
    out
}

/// The mine's rock at a point on its lining facing `n`: red beds, stained
/// dark round the veins; the bore freshly cut and scorched, molten at its
/// face.
fn rock_shade(lay: &Layout, p: Vec3, n: Vec3) -> (Vec3, Material) {
    let q = Vec2::new(p.x, p.z);
    let u = p.x + p.z;
    let bed = noise(p.y / 1.3 + 0.4 * noise(u / 9.0, 20), 21);
    let mut c = STONE_DARK.lerp(STONE, bed) * (0.82 + 0.12 * noise(u / 2.3, 22));
    if n.y < -0.6 {
        // The roof: darker, smoke-stained.
        c *= 0.72;
    }
    // Darker round the veins, where the crystal's dust stains it.
    if n.y > -0.6 {
        let near = rock_veins(lay, q)
            .iter()
            .map(|v| {
                (1.0 - smoothstep(v.half, v.half + 0.9, (p.y - v.y).abs()))
                    * (v.half > 0.0) as u8 as f32
            })
            .fold(0.0, f32::max);
        c = c.lerp(STONE_DARK * 0.55, 0.6 * near);
    }
    // The bore: cut clean and smooth, scorched toward its face, where the
    // beam strikes (its glow is drawn with the beam).
    let strike = p.distance(lay.beam.1);
    let bore = 1.0 - smoothstep(0.0, 1.5, rect_distance(lay.bore, q));
    if bore > 0.0 || strike < 7.0 {
        let fresh = c.lerp(CUT, bore.max(1.0 - smoothstep(4.0, 7.0, strike)));
        let scorch =
            (1.0 - smoothstep(2.0, 6.0, strike)).max(0.3 * bore * noise(u * 0.9 + p.y, 38));
        c = fresh.lerp(SCORCH, scorch);
    }
    (c, Material::Cliff)
}

/// A run of roof cells along a row: from, to, and what's over it (under
/// the Crag, the underside's height).
type RoofRun = (f32, f32, (bool, f32));

/// A heap of broken ore filling a tub, in the tub's frame: a rough mound
/// of small flat facets, each its own shade of Crag rock, from `y_edge`
/// round the inside of the rim up to `y_edge + rise` in the middle. Its
/// outline is `half_w(z)` across at each `z`, `half_l` long.
#[allow(clippy::too_many_arguments)]
pub(super) fn ore_heap(
    m: &mut MeshData,
    rng: &mut Rng,
    half_w: impl Fn(f32) -> f32,
    half_l: f32,
    y_edge: f32,
    rise: f32,
    nu: usize,
    nv: usize,
) {
    let shades = [ORE * 0.9, ORE * 0.7, ORE * 0.55, ORE * 1.0, MESA_TOP * 0.6];
    // The grid's points, jittered so no two facets lie the same way.
    let mut pts = vec![Vec3::ZERO; (nu + 1) * (nv + 1)];
    let (cu, cv) = (1.0 / nu as f32, 1.0 / nv as f32);
    for j in 0..=nv {
        for i in 0..=nu {
            let edge = i == 0 || i == nu || j == 0 || j == nv;
            let mut u = i as f32 / nu as f32 * 2.0 - 1.0;
            let mut v = j as f32 / nv as f32 * 2.0 - 1.0;
            if !edge {
                u += rng.range(-0.35, 0.35) * 2.0 * cu;
                v += rng.range(-0.35, 0.35) * 2.0 * cv;
            }
            let z = v * half_l;
            let x = u * half_w(z);
            let mound = (1.0 - u * u).max(0.0).sqrt() * (1.0 - v * v).max(0.0).powf(0.6);
            let y = if edge {
                y_edge
            } else {
                y_edge + rise * mound + rise * rng.range(-0.14, 0.14) * (0.4 + mound)
            };
            pts[j * (nu + 1) + i] = Vec3::new(x, y, z);
        }
    }
    let face = |m: &mut MeshData, a: Vec3, b: Vec3, c: Vec3, rng: &mut Rng| {
        let mut n = (b - a).cross(c - a).normalize_or_zero();
        if n.y < 0.0 {
            n = -n;
        }
        let shade = shades[rng.index(shades.len())] * rng.range(0.85, 1.1);
        m.tri([a, b, c], n, shade.extend(Material::Rubble.id()));
    };
    for j in 0..nv {
        for i in 0..nu {
            let p = |i: usize, j: usize| pts[j * (nu + 1) + i];
            let (a, b, c, d) = (p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1));
            // Split each cell either way, so the facets don't line up.
            if rng.chance(0.5) {
                face(m, a, b, c, rng);
                face(m, a, c, d, rng);
            } else {
                face(m, a, b, d, rng);
                face(m, b, c, d, rng);
            }
        }
    }
}

/// Where an `ore_heap`'s mound stands at (u, v), without its roughness.
pub(super) fn heap_height(y_edge: f32, rise: f32, u: f32, v: f32) -> f32 {
    y_edge + rise * (1.0 - u * u).max(0.0).sqrt() * (1.0 - v * v).max(0.0).powf(0.6)
}

/// One lump of rock (or raw crystal) about `s` across at `c`: a broken,
/// many-faced stone, sunk a little into whatever it sits on.
pub(super) fn ore_lump(
    m: &mut MeshData,
    rng: &mut Rng,
    c: Vec3,
    s: f32,
    color: Vec3,
    mat: Material,
) {
    let turn = Quat::from_euler(
        glam::EulerRot::YXZ,
        rng.range(0.0, TAU),
        rng.range(-0.5, 0.5),
        rng.range(-0.5, 0.5),
    );
    let squash = Vec3::new(
        rng.range(0.8, 1.2),
        rng.range(0.5, 0.75),
        rng.range(0.8, 1.2),
    ) * s;
    let dirs = [
        Vec3::X,
        Vec3::NEG_X,
        Vec3::Y,
        Vec3::NEG_Y,
        Vec3::Z,
        Vec3::NEG_Z,
    ];
    let p: Vec<Vec3> = dirs
        .iter()
        .map(|d| c + turn * (*d * squash * rng.range(0.75, 1.15)))
        .collect();
    // An octahedron's eight faces: each pairs an x end and a z end with
    // the top or the bottom.
    for (a, b) in [(0, 4), (4, 1), (1, 5), (5, 0)] {
        for t in [2, 3] {
            let (pa, pb, pt) = (p[a], p[b], p[t]);
            let mut n = (pb - pa).cross(pt - pa).normalize_or_zero();
            if n.dot((pa + pb + pt) / 3.0 - c) < 0.0 {
                n = -n;
            }
            let shade = color * rng.range(0.85, 1.1);
            m.tri([pa, pb, pt], n, shade.extend(mat.id()));
        }
    }
}
/// The ground's colour: sand on the flats, red rock on the Face and the
/// mesa, the wash floor dark with crystal dust and darker yet along the
/// seam, the dump pale, ballast along the track, the mine's floors dim.
fn paint(lay: &Layout, seam: &[Vec2], x: f32, z: f32, h: f32, slope: f32) -> Vec3 {
    let p = Vec2::new(x, z);
    let mottle =
        1.0 + 0.06 * (0.37 * x + 0.8 * (0.11 * z).sin()).sin() + 0.04 * (0.9 * z - 0.3 * x).sin();
    let mut c = SAND * mottle;
    // Rock where the ground is high or steep.
    let rocky = smoothstep(0.55, 1.2, slope).max(smoothstep(TOP * 0.35, TOP * 0.8, h) * 0.75);
    let rock = STONE.lerp(
        STONE_PALE,
        0.5 + 0.5 * (0.08 * x + 0.3 * (0.2 * z).sin()).sin(),
    ) * mottle;
    c = c.lerp(rock, rocky);
    // The mesa's top.
    c = c.lerp(
        MESA_TOP * mottle,
        smoothstep(TOP * 0.45, TOP * 0.8, h) * (1.0 - smoothstep(0.4, 0.9, slope)),
    );
    // The wash's floor: grey-blue with crystal dust.
    let dust = Vec3::new(0.25, 0.25, 0.3) * mottle;
    c = c.lerp(dust, smoothstep(-4.5, -6.6, h));
    // Darker along the seam.
    let seam = seam
        .windows(2)
        .map(|w| seg(p, w[0], w[1]).0)
        .fold(f32::INFINITY, f32::min);
    c = c.lerp(
        Vec3::new(0.1, 0.12, 0.18),
        (1.0 - smoothstep(1.0, 6.0, seam)) * smoothstep(-3.0, -6.0, h),
    );
    // The dump and the heap: pale crushed rock.
    let waste = (heap(lay.dump, p) / lay.dump.2).max(heap(lay.heap, p) / lay.heap.2);
    c = c.lerp(TAILINGS * mottle, smoothstep(0.05, 0.4, waste));
    // The ways up, worn into the rock and sand.
    let way = lay
        .ways_up
        .iter()
        .map(|&(a, b)| seg(p, a, b).0)
        .fold(f32::INFINITY, f32::min);
    c = c.lerp(
        Vec3::new(0.5, 0.38, 0.28) * mottle,
        0.6 * (1.0 - smoothstep(2.5, 4.0, way)),
    );
    // Ballast along the track (not on the rock over the adit).
    if h < ROOF.0 {
        c = c.lerp(
            Vec3::new(0.36, 0.32, 0.3),
            1.0 - smoothstep(2.4, 3.6, lay.track_distance(p)),
        );
    }
    // The mine's floors (not the roofs over them).
    if lay.mesa(p) > 0.5 && lay.cave_distance(p) < 0.5 && h < ROOF.0 {
        c = STONE_DARK * 0.8 * mottle;
    }
    c
}
