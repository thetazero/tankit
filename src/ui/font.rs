//! Built-in stroke font: every glyph is a few polylines on a 6-unit-tall grid,
//! drawn with the same anti-aliased lines as the rest of the UI. Wide,
//! squared shapes with chamfered corners; no textures or font files.

use std::collections::HashMap;
use std::sync::OnceLock;

use glam::Vec2;

/// Glyph grid height. Text size is the cap height in pixels, so one grid unit is `size / HEIGHT`.
pub const HEIGHT: f32 = 6.0;

pub struct Glyph {
    /// Advance width in grid units, before tracking.
    pub width: f32,
    pub strokes: Vec<Vec<Vec2>>,
}

/// (char, width, polylines). Polylines are separated by `|`, points by spaces, `x,y` with y down.
const DEFS: &[(char, f32, &str)] = &[
    ('A', 5.0, "0,6 0,2 2,0 3,0 5,2 5,6|0,4 5,4"),
    ('B', 5.0, "0,3 4,3 5,2 5,1 4,0 0,0 0,6 4,6 5,5 5,4 4,3"),
    ('C', 5.0, "5,0 1,0 0,1 0,5 1,6 5,6"),
    ('D', 5.0, "0,0 4,0 5,1 5,5 4,6 0,6 0,0"),
    ('E', 5.0, "5,0 0,0 0,6 5,6|0,3 4,3"),
    ('F', 5.0, "5,0 0,0 0,6|0,3 4,3"),
    ('G', 5.0, "5,0 1,0 0,1 0,5 1,6 5,6 5,3 3,3"),
    ('H', 5.0, "0,0 0,6|5,0 5,6|0,3 5,3"),
    ('I', 0.0, "0,0 0,6"),
    ('J', 5.0, "5,0 5,5 4,6 1,6 0,5"),
    ('K', 5.0, "0,0 0,6|5,0 2,3 0,3|2,3 5,6"),
    ('L', 5.0, "0,0 0,6 5,6"),
    ('M', 5.0, "0,6 0,0 2.5,3 5,0 5,6"),
    ('N', 5.0, "0,6 0,0 5,6 5,0"),
    ('O', 5.0, "1,0 4,0 5,1 5,5 4,6 1,6 0,5 0,1 1,0"),
    ('P', 5.0, "0,6 0,0 4,0 5,1 5,2 4,3 0,3"),
    ('Q', 5.0, "1,0 4,0 5,1 5,5 4,6 1,6 0,5 0,1 1,0|3,4 5,6"),
    ('R', 5.0, "0,6 0,0 4,0 5,1 5,2 4,3 0,3|2,3 5,6"),
    ('S', 5.0, "5,0 1,0 0,1 0,2 1,3 4,3 5,4 5,5 4,6 0,6"),
    ('T', 5.0, "0,0 5,0|2.5,0 2.5,6"),
    ('U', 5.0, "0,0 0,5 1,6 4,6 5,5 5,0"),
    ('V', 5.0, "0,0 2.5,6 5,0"),
    ('W', 6.0, "0,0 1.5,6 3,2 4.5,6 6,0"),
    ('X', 5.0, "0,0 5,6|5,0 0,6"),
    ('Y', 5.0, "0,0 2.5,3 5,0|2.5,3 2.5,6"),
    ('Z', 5.0, "0,0 5,0 0,6 5,6"),
    ('0', 4.0, "1,0 3,0 4,1 4,5 3,6 1,6 0,5 0,1 1,0"),
    ('1', 4.0, "1,1 2.5,0 2.5,6"),
    ('2', 4.0, "0,1 1,0 3,0 4,1 4,2 0,6 4,6"),
    ('3', 4.0, "0,0 4,0 2,2.5 3,2.5 4,3.5 4,5 3,6 0,6"),
    ('4', 4.0, "3,6 3,0 0,4 4,4"),
    ('5', 4.0, "4,0 0,0 0,2.5 3,2.5 4,3.5 4,5 3,6 0,6"),
    ('6', 4.0, "4,0 1,0 0,1 0,5 1,6 3,6 4,5 4,3.5 3,2.5 0,2.5"),
    ('7', 4.0, "0,0 4,0 1.5,6"),
    ('8', 4.0, "1,0 3,0 4,1 4,2 3,3 1,3 0,2 0,1 1,0|1,3 0,4 0,5 1,6 3,6 4,5 4,4 3,3"),
    ('9', 4.0, "4,3.5 1,3.5 0,2.5 0,1 1,0 3,0 4,1 4,5 3,6 0,6"),
    ('.', 0.0, "0,5.6 0,6"),
    (':', 0.0, "0,1.6 0,2|0,5.6 0,6"),
    ('·', 0.0, "0,2.8 0,3.2"),
    ('/', 3.0, "0,6 3,0"),
    ('-', 3.0, "0,3 3,3"),
    ('+', 4.0, "0,3 4,3|2,1 2,5"),
    ('%', 4.0, "0,6 4,0|0,0 1,0 1,1 0,1 0,0|3,5 4,5 4,6 3,6 3,5"),
    ('(', 1.5, "1.5,0 0,1.5 0,4.5 1.5,6"),
    (')', 1.5, "0,0 1.5,1.5 1.5,4.5 0,6"),
    ('°', 1.2, "0,0 1.2,0 1.2,1.2 0,1.2 0,0"),
    (' ', 3.0, ""),
];

fn table() -> &'static HashMap<char, Glyph> {
    static T: OnceLock<HashMap<char, Glyph>> = OnceLock::new();
    T.get_or_init(|| {
        DEFS.iter()
            .map(|&(c, width, def)| {
                let strokes = def
                    .split('|')
                    .filter(|s| !s.is_empty())
                    .map(|poly| {
                        poly.split_whitespace()
                            .map(|p| {
                                let (x, y) = p.split_once(',').expect("glyph point is x,y");
                                Vec2::new(x.parse().expect("glyph x"), y.parse().expect("glyph y"))
                            })
                            .collect()
                    })
                    .collect();
                (c, Glyph { width, strokes })
            })
            .collect()
    })
}

/// Glyph for `c`; lowercase maps to uppercase. `None` for characters the font doesn't have.
pub fn glyph(c: char) -> Option<&'static Glyph> {
    let t = table();
    t.get(&c).or_else(|| t.get(&c.to_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_parses_and_stays_on_grid() {
        for &(c, width, _) in DEFS {
            let g = glyph(c).unwrap();
            for p in g.strokes.iter().flatten() {
                assert!(p.x >= 0.0 && p.x <= width.max(0.0) + 1e-3, "{c:?} x={} past width {width}", p.x);
                assert!(p.y >= 0.0 && p.y <= HEIGHT, "{c:?} y={} off grid", p.y);
            }
            assert!(c == ' ' || !g.strokes.is_empty(), "{c:?} has no strokes");
        }
    }

    #[test]
    fn covers_letters_digits_and_lowercase() {
        for c in ('A'..='Z').chain('a'..='z').chain('0'..='9') {
            assert!(glyph(c).is_some(), "missing {c:?}");
        }
        assert!(glyph('~').is_none());
    }
}
