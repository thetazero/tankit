//! An icon for each of the Company's titles, drawn in hairlines like the
//! factions' emblems (`emblem`).

use std::f32::consts::{PI, TAU};

use glam::Vec2;

use super::emblem::{arc, disc};
use super::{Painter, Rgba};
use crate::profile::Title;

/// `title`'s icon centred on `c`, `size` logical px across, strokes `weight` wide.
pub fn rank_icon(p: &mut Painter, title: Title, c: Vec2, size: f32, weight: f32, color: Rgba) {
    let u = size * 0.5;
    let at = |x: f32, y: f32| c + Vec2::new(x, y) * u;
    let path = |p: &mut Painter, pts: &[(f32, f32)]| {
        let pts: Vec<Vec2> = pts.iter().map(|&(x, y)| at(x, y)).collect();
        p.polyline(&pts, weight, color);
    };
    // A chevron, its point at `y`.
    let chevron =
        |p: &mut Painter, y: f32| path(p, &[(-0.75, y + 0.55), (0.0, y), (0.75, y + 0.55)]);
    // A five-pointed star, `r` from middle to point.
    let star = |p: &mut Painter, mid: (f32, f32), r: f32| {
        let pts: Vec<(f32, f32)> = (0..=10)
            .map(|i| {
                let a = -PI / 2.0 + TAU * i as f32 / 10.0;
                let r = if i % 2 == 0 { r } else { r * 0.42 };
                (mid.0 + r * a.cos(), mid.1 + r * a.sin())
            })
            .collect();
        path(p, &pts);
    };
    match title {
        // A dog tag, fresh from the stamp.
        Title::Recruit => {
            path(
                p,
                &[
                    (-0.3, -0.95),
                    (0.3, -0.95),
                    (0.6, -0.65),
                    (0.6, 0.65),
                    (0.3, 0.95),
                    (-0.3, 0.95),
                    (-0.6, 0.65),
                    (-0.6, -0.65),
                    (-0.3, -0.95),
                ],
            );
            arc(p, at(0.0, -0.6), 0.14 * u, 0.0, TAU, weight, color);
            for (y, half) in [(-0.1, 0.32), (0.2, 0.32), (0.5, 0.16)] {
                path(p, &[(-0.32, y), (-0.32 + 2.0 * half, y)]);
            }
        }
        // A coin: you're paid.
        Title::Hireling => {
            arc(p, c, 0.85 * u, 0.0, TAU, weight, color);
            path(
                p,
                &[
                    (0.0, -0.4),
                    (0.4, 0.0),
                    (0.0, 0.4),
                    (-0.4, 0.0),
                    (0.0, -0.4),
                ],
            );
        }
        // A lance flying its own pennant.
        Title::Freelance => {
            path(p, &[(-0.5, 1.0), (-0.5, -0.65)]);
            path(
                p,
                &[
                    (-0.5, -1.0),
                    (-0.36, -0.75),
                    (-0.5, -0.55),
                    (-0.64, -0.75),
                    (-0.5, -1.0),
                ],
            );
            path(
                p,
                &[
                    (-0.5, -0.4),
                    (0.8, -0.4),
                    (0.4, -0.05),
                    (0.8, 0.3),
                    (-0.5, 0.3),
                ],
            );
        }
        // A shell in its case.
        Title::Gunhand => {
            path(
                p,
                &[
                    (-0.36, -0.1),
                    (-0.3, -0.5),
                    (0.0, -1.0),
                    (0.3, -0.5),
                    (0.36, -0.1),
                ],
            );
            path(
                p,
                &[
                    (-0.36, -0.1),
                    (0.36, -0.1),
                    (0.36, 0.75),
                    (-0.36, 0.75),
                    (-0.36, -0.1),
                ],
            );
            path(p, &[(-0.36, 0.75), (-0.5, 1.0), (0.5, 1.0), (0.36, 0.75)]);
        }
        // A shield that's been hit, and held.
        Title::Veteran => {
            path(
                p,
                &[
                    (0.0, -0.95),
                    (0.75, -0.7),
                    (0.75, 0.1),
                    (0.45, 0.65),
                    (0.0, 1.0),
                    (-0.45, 0.65),
                    (-0.75, 0.1),
                    (-0.75, -0.7),
                    (0.0, -0.95),
                ],
            );
            path(p, &[(0.4, -0.45), (0.0, -0.1), (0.15, 0.1), (-0.3, 0.5)]);
        }
        // Three chevrons.
        Title::Sergeant => {
            for y in [-0.9, -0.28, 0.34] {
                chevron(p, y);
            }
        }
        // A star.
        Title::Lieutenant => star(p, (0.0, 0.08), 1.0),
        // A star in a wreath.
        Title::Captain => {
            star(p, (0.0, -0.1), 0.55);
            arc(p, c, 0.9 * u, -0.25 * PI, 0.42 * PI, weight, color);
            arc(p, c, 0.9 * u, 0.58 * PI, 1.25 * PI, weight, color);
            // Its leaves, pointing up and in.
            for side in [-1.0, 1.0] {
                for a in [-0.15_f32, 0.2, 0.55, 0.9] {
                    let root = (side * 0.9 * a.cos(), 0.9 * a.sin());
                    let tip = (side * 0.66 * (a - 0.2).cos(), 0.66 * (a - 0.2).sin());
                    path(p, &[root, tip]);
                }
            }
        }
        // A star over crossed batons.
        Title::Marshal => {
            star(p, (0.0, -0.5), 0.48);
            for side in [-1.0, 1.0] {
                // The baton's four corners, then a band at either end.
                let (from, to) = (Vec2::new(-0.8 * side, 0.95), Vec2::new(0.8 * side, -0.15));
                let along = (to - from).normalize();
                let half = along.perp() * 0.12;
                let v = |k: f32, s: f32| {
                    let q = from + (to - from) * k + half * s;
                    (q.x, q.y)
                };
                path(
                    p,
                    &[
                        v(0.0, 1.0),
                        v(1.0, 1.0),
                        v(1.0, -1.0),
                        v(0.0, -1.0),
                        v(0.0, 1.0),
                    ],
                );
                for k in [0.16, 0.84] {
                    path(p, &[v(k, 1.0), v(k, -1.0)]);
                }
            }
        }
        // A crown.
        Title::Warlord => {
            path(
                p,
                &[
                    (-0.75, 0.45),
                    (-0.9, -0.4),
                    (-0.42, 0.0),
                    (0.0, -0.65),
                    (0.42, 0.0),
                    (0.9, -0.4),
                    (0.75, 0.45),
                    (-0.75, 0.45),
                ],
            );
            path(p, &[(-0.75, 0.75), (0.75, 0.75)]);
            for (x, y) in [(-0.9, -0.55), (0.0, -0.8), (0.9, -0.55)] {
                disc(p, at(x, y), 0.12 * u, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every title's icon draws something, no two alike, inside its size.
    #[test]
    fn each_title_has_its_own_icon() {
        let mut seen: Vec<Vec<[f32; 2]>> = Vec::new();
        for title in Title::ALL {
            let mut p = Painter::new(1.0);
            p.set_halo(None);
            rank_icon(&mut p, title, Vec2::ZERO, 40.0, 1.0, [1.0; 4]);
            let verts: Vec<[f32; 2]> = p.finish().iter().map(|v| v.pos).collect();
            assert!(!verts.is_empty(), "{title:?}");
            assert!(
                verts.iter().flatten().all(|v| v.abs() < 24.0),
                "{title:?} spills"
            );
            assert!(!seen.contains(&verts), "{title:?} isn't its own");
            seen.push(verts);
        }
    }
}
