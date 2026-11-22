//! Is a map playable: can every side get to the fight? Run on the greybox
//! before a skeleton is dressed (a skeleton that fails is laid out again
//! from the next seed), and by the tests on the dressed map too.

use glam::Vec2;

use crate::ai::nav::NavGrid;
use crate::map::Map;

/// The fewest spawns a side's base may have.
pub const MIN_SPAWNS: usize = 12;

/// Why `map` isn't playable, if it isn't: a base short of spawns, or a
/// side that can't drive to the Hold site or to another side's base.
pub fn playable(map: &Map) -> Result<(), String> {
    playable_on(map, &NavGrid::build(map))
}

/// `playable`, on `map`'s nav grid already built.
pub fn playable_on(map: &Map, nav: &NavGrid) -> Result<(), String> {
    for (k, b) in map.bases.iter().enumerate() {
        if b.len() < MIN_SPAWNS {
            return Err(format!("side {k} has {} spawns", b.len()));
        }
    }
    let mid = |b: &[Vec2]| b.iter().copied().sum::<Vec2>() / b.len() as f32;
    let hold = map.hold_site.unwrap_or(map.center());
    for (k, b) in map.bases.iter().enumerate() {
        let from = mid(b);
        if nav.path(from, hold).is_empty() {
            return Err(format!("side {k} can't reach the hold site"));
        }
        for (j, other) in map.bases.iter().enumerate().skip(k + 1) {
            if nav.path(from, mid(other)).is_empty() {
                return Err(format!("side {k} can't reach side {j}"));
            }
        }
    }
    Ok(())
}
