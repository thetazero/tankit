//! A mine's works: its railroads, the spur to its warehouse and the bridges
//! over its washes, and how far things are from them.

use glam::Vec2;

use super::*;
use crate::map::procgen::crossing::Crossing;
use crate::map::procgen::skeleton::{Skeleton, Spur};

/// A spur off `track`, `junction` m along it, swinging off to the left of
/// the way out (`side` 1) or the right (-1), with its warehouse.
pub(super) fn spur(track: &[Vec2], junction: f32, side: f32) -> Spur {
    let (j, dir) = line_at(track, junction);
    let back = -dir;
    let centre = j + back.perp() * BRANCH_R * side;
    let turn = Vec2::from_angle(BRANCH_TURN * side);
    let n = 12;
    let mut line: Vec<Vec2> = (0..=n)
        .map(|k| {
            centre + Vec2::from_angle(BRANCH_TURN * side * k as f32 / n as f32).rotate(j - centre)
        })
        .collect();
    let end = centre + turn.rotate(j - centre);
    let along = turn.rotate(back);
    let at = end + along * (SHED_LEAD + SHED_HALF.y);
    line.push(at - along * SHED_HALF.y);
    line.push(at + along * SHED_BUFFER);
    Spur {
        line,
        junction,
        side,
        shed: (at, along),
    }
}

impl Skeleton {
    /// The warehouse's middle and its way along (far off, if there's none).
    pub fn shed(&self) -> (Vec2, Vec2) {
        self.spur
            .as_ref()
            .map_or((Vec2::splat(-1000.0), Vec2::Y), |s| s.shed)
    }

    /// Which side of the first track the spur's on.
    pub fn branch_side(&self) -> f32 {
        self.spur.as_ref().map_or(1.0, |s| s.side)
    }

    /// Which wash `x` bridges, and how far along it.
    pub fn bridged(&self, x: &Crossing) -> (usize, f32) {
        let n = (x.len() / 0.25) as usize;
        (0..=n)
            .flat_map(|i| {
                let p = x.from + x.dir() * (i as f32 * 0.25);
                (self.land.washes.iter().enumerate()).map(move |(k, w)| {
                    let (d, s) = w.near(p);
                    (d, k, s)
                })
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map_or((0, 0.0), |(_, k, s)| (k, s))
    }

    /// How near a track, any line or the branch.
    pub fn track_distance(&self, p: Vec2) -> f32 {
        self.spur
            .iter()
            .map(|s| line_distance(&s.line, p))
            .fold(self.main_line_distance(p), f32::min)
    }

    /// How near any main line.
    pub fn main_line_distance(&self, p: Vec2) -> f32 {
        self.tracks
            .iter()
            .map(|l| line_distance(l, p))
            .fold(f32::INFINITY, f32::min)
    }

    /// Position and heading `s` m along the first line.
    pub fn track_at(&self, s: f32) -> (Vec2, Vec2) {
        line_at(&self.tracks[0], s)
    }

    /// Where the branch leaves the first line, and the line's way there.
    pub fn branch_from(&self) -> (Vec2, Vec2) {
        self.track_at(self.spur.as_ref().map_or(0.0, |s| s.junction))
    }

    /// Over a bridge's deck (not its end plates).
    pub fn on_bridge(&self, p: Vec2) -> bool {
        self.crossings.iter().any(|b| {
            let (d, t) = seg(p, b.from, b.to);
            d < DECK_HALF && t > 0.0 && t < 1.0
        })
    }

    /// A point in the warehouse's own frame: (across, along) from its
    /// middle, +across to the left of the way in.
    pub fn shed_local(&self, p: Vec2) -> Vec2 {
        let (at, along) = self.shed();
        let d = p - at;
        Vec2::new(d.dot(Vec2::new(along.y, -along.x)), d.dot(along))
    }

    /// How far outside the warehouse's walls (0 inside).
    pub fn shed_distance(&self, p: Vec2) -> f32 {
        let q = self.shed_local(p).abs() - SHED_HALF;
        q.max(Vec2::ZERO).length()
    }

    /// The ground straight out in front of the warehouse's doors.
    pub fn shed_doorways(&self, p: Vec2) -> bool {
        let l = self.shed_local(p).abs();
        (l.y < 7.0 && l.x < SHED_HALF.x + 16.0) || (l.x < 7.0 && l.y < SHED_HALF.y + 16.0)
    }
}
