//! Last passes over a map built of many pieces: bringing down what damage
//! left hanging, dropping colliders too small to matter, and keeping flush
//! faces from flickering.

use glam::{Vec2, Vec3};

use super::{Map, OBox};
use crate::render::material::Material;
use crate::rng::Rng;

impl Map {
    /// Small things on the ground (wall stubs, rubble lumps lower than a road
    /// wheel's travel, thin specks) shouldn't stop or jolt a tank: they keep
    /// their looks but lose their colliders. Taller pieces stay solid and the
    /// suspension takes them.
    pub(super) fn drop_tiny_hitboxes(&mut self) {
        let terrain = self.terrain.as_ref();
        for o in &mut self.oboxes {
            let g = terrain.map_or(0.0, |t| t.height_at(o.center.x, o.center.y));
            let on_ground = o.y0 <= g + 0.3;
            let low = o.y1 - g < 0.45;
            let speck = o.half.x.max(o.half.y) < 0.35 && o.y1 - g < 1.2;
            if o.solid && !o.ramp && o.tilt == 0.0 && on_ground && (low || speck) {
                o.solid = false;
            }
        }
    }

    /// Pieces built flush against each other share face planes, which flicker
    /// (z-fight). Grow each piece by a different few millimeters (cycling
    /// through 11 sizes, so neighbors built one after another always differ)
    /// so no two faces are ever exactly coplanar. Invisible at play scale.
    pub(super) fn separate_coplanar_faces(&mut self) {
        // Each piece gets a "color" 0..29 different from every piece it
        // touches (greedy, over a spatial grid), and its offsets come from the
        // color: sideways c, up 13c+3, down 17c+8 (mod 29, 0.45 mm steps).
        // 13 and 17 are invertible mod 29, so touching pieces differ in all
        // three, guaranteed rather than likely.
        const COLORS: usize = 29;
        let step = |c: usize| 0.002 + 0.00045 * (c % COLORS) as f32;
        let side = |c: usize| step(c * 11);
        let up = |c: usize| step(c * 13 + 3);
        let down = |c: usize| step(c * 17 + 8);
        // Footprint circles for every piece (rotated boxes, then plain boxes).
        let mut shapes: Vec<(Vec2, f32, f32, f32)> = self
            .oboxes
            .iter()
            .map(|o| (o.center, o.half.length(), o.y0, o.y1))
            .collect();
        shapes.extend(self.boxes.iter().map(|b| {
            let c = Vec2::new(b.min.x + b.max.x, b.min.z + b.max.z) * 0.5;
            (
                c,
                (Vec2::new(b.max.x - b.min.x, b.max.z - b.min.z) * 0.5).length(),
                b.min.y,
                b.max.y,
            )
        }));
        let cell = 16.0;
        let key = |p: Vec2| ((p.x / cell).floor() as i32, (p.y / cell).floor() as i32);
        let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> =
            std::collections::HashMap::new();
        for (i, s) in shapes.iter().enumerate() {
            grid.entry(key(s.0)).or_default().push(i);
        }
        let mut color = vec![usize::MAX; shapes.len()];
        for i in 0..shapes.len() {
            let (c, r, y0, y1) = shapes[i];
            let mut used = [false; COLORS];
            let (kx, kz) = key(c);
            let reach = (r / cell).ceil() as i32 + 1;
            for dx in -reach..=reach {
                for dz in -reach..=reach {
                    for &j in grid.get(&(kx + dx, kz + dz)).into_iter().flatten() {
                        let (cj, rj, y0j, y1j) = shapes[j];
                        let touch =
                            c.distance(cj) < r + rj + 0.05 && y0 < y1j + 0.05 && y0j < y1 + 0.05;
                        if j != i && color[j] != usize::MAX && touch {
                            used[color[j]] = true;
                        }
                    }
                }
            }
            color[i] = (0..COLORS).find(|&k| !used[k]).unwrap_or(i % COLORS);
        }
        let n = self.oboxes.len();
        let terrain = self.terrain.as_ref();
        for (i, o) in self.oboxes.iter_mut().enumerate() {
            let k = color[i];
            o.half += Vec2::splat(side(k));
            o.y1 += up(k);
            // Bottoms in the air (slab undersides, upper storeys) too; ones on
            // the ground stay put.
            if o.y0 > terrain.map_or(0.0, |t| t.height_at(o.center.x, o.center.y)) + 0.05 {
                o.y0 -= down(k);
            }
        }
        for (i, b) in self.boxes.iter_mut().enumerate() {
            let k = color[n + i];
            let (e, u) = (side(k), up(k));
            b.min -= Vec3::new(e, 0.0, e);
            b.max += Vec3::new(e, u, e);
        }
    }

    /// Bring down whatever the damage left hanging in the air, onto what's
    /// under it (the floor below, the ground): floor slabs lie there tipped,
    /// broken-off wall becomes rubble, and props (roof tanks, dishes,
    /// panels) come down whole and land upright. Rebar that stuck out of a
    /// fallen piece goes, and so does what would land on a collapsed-floor
    /// ramp.
    pub(super) fn settle(&mut self) {
        let floating = self.floating();
        if floating.is_empty() {
            return;
        }
        let n = self.oboxes.len();
        let mut fall = vec![false; n];
        for &i in &floating {
            fall[i] = true;
        }
        let rebar =
            |o: &OBox| o.side_mat == Material::Rust && o.is_turned() && o.half.min_element() < 0.05;
        let masonry = |o: &OBox| {
            [Material::Plaster, Material::Concrete, Material::Rubble].contains(&o.side_mat)
        };
        // Bounds of a piece, tipped ones by their corners.
        let span = |o: &OBox| {
            if o.is_turned() {
                let ys = o.corners3().map(|c| c.y);
                (
                    ys.iter().copied().fold(f32::INFINITY, f32::min),
                    ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                )
            } else {
                (o.y0, o.y1)
            }
        };
        let near = |a: &OBox, b: &OBox, margin: f32| {
            b.center.distance(a.center) < b.half.length() + a.half.length() + margin + 0.1
                && b.overlaps(a, margin)
        };
        // What stays standing, for things to land on.
        let standing: Vec<OBox> = (0..n)
            .filter(|&j| !fall[j])
            .map(|j| self.oboxes[j])
            .collect();
        let standing_index = Index::new(standing.iter().enumerate());
        let terrain = self.terrain.clone();
        let ground_at = |p: Vec2| terrain.as_ref().map_or(0.0, |t| t.height_at(p.x, p.y));
        // Highest standing top (or ground) under `o`, no higher than `below`.
        let floor_under = |o: &OBox, below: f32| {
            let mut floor = [o.corners()[0], o.corners()[2], o.center]
                .iter()
                .map(|&p| ground_at(p))
                .fold(f32::NEG_INFINITY, f32::max);
            for j in standing_index.near(o, 0.1) {
                let b = &standing[j];
                let top = span(b).1;
                if top <= below + 0.1 && top > floor && near(b, o, -0.05) {
                    floor = top;
                }
            }
            floor
        };

        // Props, as rigid groups of touching parts.
        let props: Vec<usize> = floating
            .iter()
            .copied()
            .filter(|&i| !masonry(&self.oboxes[i]) && !rebar(&self.oboxes[i]))
            .collect();
        let mut group: Vec<usize> = (0..props.len()).collect();
        fn root(p: &mut [usize], mut i: usize) -> usize {
            while p[i] != i {
                p[i] = p[p[i]];
                i = p[i];
            }
            i
        }
        let prop_index = Index::new(props.iter().map(|&i| &self.oboxes[i]).enumerate());
        for x in 0..props.len() {
            for y in prop_index.near(&self.oboxes[props[x]], 0.2) {
                if y <= x {
                    continue;
                }
                let (a, b) = (&self.oboxes[props[x]], &self.oboxes[props[y]]);
                let ((a0, a1), (b0, b1)) = (span(a), span(b));
                if a0 <= b1 + 0.1 && b0 <= a1 + 0.1 && near(a, b, 0.1) {
                    let (rx, ry) = (root(&mut group, x), root(&mut group, y));
                    group[rx] = ry;
                }
            }
        }
        let mut groups: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
        for (x, &i) in props.iter().enumerate() {
            let r = root(&mut group, x);
            groups.entry(r).or_default().push(i);
        }
        for members in groups.values() {
            // Drop as far as the first part to land allows.
            let drop = members
                .iter()
                .map(|&i| {
                    let o = &self.oboxes[i];
                    let bottom = span(o).0;
                    bottom - floor_under(o, bottom)
                })
                .fold(f32::INFINITY, f32::min)
                .max(0.0);
            for &i in members {
                let o = &mut self.oboxes[i];
                o.y0 -= drop;
                o.y1 -= drop;
                o.solid = false;
            }
        }

        // Masonry, lowest first, so higher slabs land on lower ones. Floor
        // slabs come down whole and lie tipped; a broken-off piece of wall
        // ends up a lump of rubble.
        // Its own random stream, so nothing else in the map changes with it.
        let mut rng = Rng::new(0x5e771e);
        let mut order: Vec<usize> = floating
            .iter()
            .copied()
            .filter(|&i| masonry(&self.oboxes[i]))
            .collect();
        order.sort_by(|&a, &b| self.oboxes[a].y0.total_cmp(&self.oboxes[b].y0));
        // Slabs already down: footprint and the height they now reach.
        let mut fallen: Vec<(OBox, f32)> = Vec::new();
        // Nothing comes down onto a collapsed floor a tank drives up: what
        // fell there went over the edge, out of the way.
        let ramps: Vec<OBox> = standing.iter().filter(|o| o.is_ramp()).copied().collect();
        let ramp_index = Index::new(ramps.iter().enumerate());
        let mut gone = vec![false; n];
        for &i in &order {
            let mut o = self.oboxes[i];
            if ramp_index
                .near(&o, 3.6)
                .iter()
                .any(|&r| ramps[r].overlaps(&o, 3.5))
            {
                gone[i] = true;
                continue;
            }
            let slab = o.side_mat == Material::Concrete && o.y1 - o.y0 < 0.45;
            if !slab {
                o.half = o.half.min(Vec2::splat(rng.range(0.4, 0.65)));
                o.y1 = o.y0 + (o.y1 - o.y0).min(rng.range(0.5, 0.8));
                o.side_mat = Material::Rubble;
                o.top_mat = Material::Rubble;
                o.broken = false;
            }
            let mut floor = floor_under(&o, o.y0);
            for (b, top) in &fallen {
                if *top > floor && *top <= o.y1 && near(b, &o, -0.05) {
                    floor = *top;
                }
            }
            // Tipped, one edge down in the dirt.
            let tilt = rng.range(0.12, 0.35) * if rng.chance(0.5) { 1.0 } else { -1.0 };
            let h = o.y1 - o.y0;
            let mid = floor + h * 0.5 * tilt.cos() + o.half.y * tilt.abs().sin() - 0.12;
            o.yaw += rng.range(-0.25, 0.25) + if slab { 0.0 } else { rng.range(0.0, 3.0) };
            o.tilt = tilt;
            o.y0 = mid - h * 0.5;
            o.y1 = mid + h * 0.5;
            o.solid = false;
            o.chip = 0.0;
            self.oboxes[i] = o;
            if slab {
                fallen.push((o, span(&o).1));
            }
        }

        // Rebar left hanging off a piece that fell.
        let mut k = 0;
        self.oboxes.retain(|o| {
            k += 1;
            !(gone[k - 1] || fall[k - 1] && rebar(o))
        });
    }

    /// Pieces that nothing holds up: not on the ground, and not resting
    /// (through a chain of pieces) on something that is.
    pub(super) fn floating(&self) -> Vec<usize> {
        let held = self.support();
        (0..held.len()).filter(|&i| held[i].is_none()).collect()
    }

    /// For each piece, what holds it up: itself if it's on the ground,
    /// another piece's index, or None if nothing does.
    ///
    /// A piece is held by one it sits on, or from the side by ones it's built
    /// in between (a lintel between piers, a floor between walls), which must
    /// rise well past its bottom (not just meet it at a floor line) and hold
    /// it from opposite sides: one pier can't hold up a beam. A floor counts
    /// as one plate of all its touching slabs.
    fn support(&self) -> Vec<Option<usize>> {
        let n = self.oboxes.len();
        // Vertical extent of each piece (turned ones by their corners).
        let span: Vec<(f32, f32)> = self
            .oboxes
            .iter()
            .map(|o| {
                if o.is_turned() {
                    let ys = o.corners3().map(|c| c.y);
                    (
                        ys.iter().copied().fold(f32::INFINITY, f32::min),
                        ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                    )
                } else {
                    (o.y0, o.y1)
                }
            })
            .collect();
        let grounded = |i: usize| {
            let o = &self.oboxes[i];
            let mut pts = o.corners().to_vec();
            pts.push(o.center);
            let g = pts
                .iter()
                .map(|&p| self.ground_height(p.x, p.y))
                .fold(f32::NEG_INFINITY, f32::max);
            span[i].0 <= g + 0.1
        };
        let index = Index::new(self.oboxes.iter().enumerate());
        let neighbors = |i: usize| {
            let b = &self.oboxes[i];
            let mut out = index.near(b, 0.2);
            out.retain(|&j| {
                let a = &self.oboxes[j];
                j != i && a.center.distance(b.center) < a.half.length() + b.half.length() + 0.2
            });
            out
        };
        // Floors: touching slabs at one level make one plate.
        let slab = |i: usize| {
            let o = &self.oboxes[i];
            o.side_mat == Material::Concrete && !o.is_turned() && o.y1 - o.y0 < 0.45
        };
        let mut plate: Vec<usize> = (0..n).collect();
        fn root(p: &mut [usize], mut i: usize) -> usize {
            while p[i] != i {
                p[i] = p[p[i]];
                i = p[i];
            }
            i
        }
        for i in (0..n).filter(|&i| slab(i)) {
            for j in neighbors(i) {
                let level = (self.oboxes[i].y1 - self.oboxes[j].y1).abs() < 0.1;
                if slab(j) && level && self.oboxes[i].overlaps(&self.oboxes[j], 0.12) {
                    let (ri, rj) = (root(&mut plate, i), root(&mut plate, j));
                    plate[ri] = rj;
                }
            }
        }
        let mut cells: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for i in 0..n {
            let r = root(&mut plate, i);
            cells.entry(r).or_default().push(i);
        }

        let mut held: Vec<Option<usize>> = vec![None; n];
        // Which ways each piece (each plate, for floors) is held from the
        // side so far: directions to its holders in its own frame.
        let mut sides: Vec<Vec<Vec2>> = vec![Vec::new(); n];
        let mut queue: Vec<usize> = (0..n).filter(|&i| grounded(i)).collect();
        for &i in &queue {
            held[i] = Some(i);
        }
        while let Some(i) = queue.pop() {
            let b = &self.oboxes[i];
            let (b0, b1) = span[i];
            for j in neighbors(i) {
                if held[j].is_some() {
                    continue;
                }
                let a = &self.oboxes[j];
                let a0 = span[j].0;
                // Tipped pieces (props' slanted parts, rebar, wires) hang on
                // whatever they touch.
                let hangs =
                    a.is_turned() && b1 >= a0 - 0.1 && b0 <= span[j].1 + 0.1 && a.overlaps(b, 0.1);
                let under = b0 <= a0 + 0.1;
                let on_top = hangs || (under && b1 >= a0 - 0.1 && a.overlaps(b, -0.02));
                let beside = !on_top && under && b1 >= a0 + 0.25 && a.overlaps(b, 0.12);
                let unit = root(&mut plate, j);
                if beside {
                    // Which of its faces the holder is at: by the holder's
                    // point nearest its middle.
                    let (bx, bz) = b.axes();
                    let l = b.local_point(a.center).clamp(-b.half, b.half);
                    let touch = b.center + bx * l.x + bz * l.y;
                    let q = a.local_point(touch) / a.half.max(Vec2::splat(0.05));
                    let dir = if q.x.abs() > q.y.abs() {
                        Vec2::new(q.x.signum(), 0.0)
                    } else {
                        Vec2::new(0.0, q.y.signum())
                    };
                    if !sides[unit].contains(&dir) {
                        sides[unit].push(dir);
                    }
                }
                // Masonry needs holding from both sides; a prop's sign or
                // shelf can be fixed to one post.
                let masonry = [Material::Plaster, Material::Concrete].contains(&a.side_mat);
                let both_sides = !masonry || sides[unit].iter().any(|d| sides[unit].contains(&-*d));
                if on_top || (beside && both_sides) {
                    for &k in &cells[&unit] {
                        if held[k].is_none() {
                            held[k] = Some(i);
                            queue.push(k);
                        }
                    }
                }
            }
        }
        held
    }
}

/// Pieces by where they are: each is filed under every grid cell its
/// footprint's bounding circle covers.
struct Index {
    cells: std::collections::HashMap<(i32, i32), Vec<usize>>,
}

impl Index {
    const CELL: f32 = 4.0;

    fn new<'a>(pieces: impl Iterator<Item = (usize, &'a OBox)>) -> Index {
        let mut cells: std::collections::HashMap<(i32, i32), Vec<usize>> = Default::default();
        for (i, o) in pieces {
            let ((x0, z0), (x1, z1)) = Index::range(o.center, o.half.length());
            for x in x0..=x1 {
                for z in z0..=z1 {
                    cells.entry((x, z)).or_default().push(i);
                }
            }
        }
        Index { cells }
    }

    fn range(c: Vec2, r: f32) -> ((i32, i32), (i32, i32)) {
        let k = |v: f32| (v / Index::CELL).floor() as i32;
        ((k(c.x - r), k(c.y - r)), (k(c.x + r), k(c.y + r)))
    }

    /// Pieces whose footprints might come within `margin` of `o`'s.
    fn near(&self, o: &OBox, margin: f32) -> Vec<usize> {
        let ((x0, z0), (x1, z1)) = Index::range(o.center, o.half.length() + margin);
        let mut out = Vec::new();
        for x in x0..=x1 {
            for z in z0..=z1 {
                out.extend(self.cells.get(&(x, z)).into_iter().flatten());
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rotated box blocks exactly where its footprint says (physics, nav and
    /// rendering all share OBox's rotation convention).
    #[test]
    fn rotated_boxes_collide_where_they_are() {
        let mut map = Map::empty(100.0);
        let b = OBox {
            center: Vec2::new(50.0, 50.0),
            yaw: 0.7,
            half: Vec2::new(6.0, 2.0),
            y0: 0.0,
            y1: 4.0,
            ..OBox::default()
        };
        map.oboxes.push(b);
        let w = crate::sim::World::empty(map);
        for k in 0..16 {
            let a = k as f32 / 16.0 * std::f32::consts::TAU;
            let dir = Vec2::new(a.cos(), a.sin());
            let origin = b.center - dir * 20.0;
            let hit = w.raycast(
                Vec3::new(origin.x, 1.0, origin.y),
                Vec3::new(dir.x, 0.0, dir.y),
                40.0,
            );
            let expect = (0..4000)
                .map(|i| i as f32 * 0.01)
                .find(|&t| b.distance(origin + dir * t) == 0.0)
                .unwrap();
            assert!((hit - expect).abs() < 0.05, "angle {a}: {hit}, {expect}");
        }
    }
}
