//! A mine's works: its railroads, a spur to a warehouse and the bridges
//! over its washes, and how far things are from them.

use glam::Vec2;

use super::*;
use crate::map::OBox;
use crate::map::procgen::crossing::Crossing;
use crate::map::procgen::skeleton::{Lot, LotKind, Skeleton};

/// A spur off `track`, `junction` m along it, swinging off to the left of
/// the way out (`side` 1) or the right (-1): its line, and the warehouse at
/// its end.
pub(super) fn spur(track: &[Vec2], junction: f32, side: f32) -> (Vec<Vec2>, Lot) {
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
    let shed = Lot {
        footprint: OBox {
            center: at,
            yaw: yaw_along(along),
            half: SHED_HALF,
            ..OBox::default()
        },
        front: (1, -1.0),
        kind: LotKind::Warehouse,
    };
    (line, shed)
}

impl Skeleton {
    /// The track the `k`th is a spur off, if it's one, and how far along that
    /// it leaves.
    pub fn off_track(&self, k: usize) -> Option<(usize, f32)> {
        let from = *self.tracks[k].first()?;
        (0..k)
            .find(|&m| line_distance(&self.tracks[m], from) < 0.1)
            .map(|m| (m, along(&self.tracks[m], from)))
    }

    /// Its lots of one kind.
    pub fn lots_of(&self, kind: LotKind) -> impl Iterator<Item = &Lot> {
        self.lots.iter().filter(move |l| l.kind == kind)
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

    /// How near any track.
    pub fn track_distance(&self, p: Vec2) -> f32 {
        self.tracks
            .iter()
            .map(|l| line_distance(l, p))
            .fold(f32::INFINITY, f32::min)
    }

    /// Over a bridge's deck (not its end plates).
    pub fn on_bridge(&self, p: Vec2) -> bool {
        self.crossings.iter().any(|b| {
            let (d, t) = seg(p, b.from, b.to);
            d < DECK_HALF && t > 0.0 && t < 1.0
        })
    }

    /// How far outside every warehouse's walls (0 inside).
    pub fn shed_distance(&self, p: Vec2) -> f32 {
        self.lots_of(LotKind::Warehouse)
            .map(|l| {
                let q = l.footprint.local_point(p).abs() - l.footprint.half;
                q.max(Vec2::ZERO).length()
            })
            .fold(f32::INFINITY, f32::min)
    }

    /// The ground straight out in front of a warehouse's doors.
    pub fn shed_doorways(&self, p: Vec2) -> bool {
        self.lots_of(LotKind::Warehouse).any(|l| {
            let (q, half) = (l.footprint.local_point(p).abs(), l.footprint.half);
            (q.y < 7.0 && q.x < half.x + 16.0) || (q.x < 7.0 && q.y < half.y + 16.0)
        })
    }
}
