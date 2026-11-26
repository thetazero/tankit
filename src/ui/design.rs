//! The design system, v0 (`docs/design.md`): the tokens and components every
//! screen is to be made from. Only the main menu is so far; the rest still
//! use `theme` and `widgets`.

use glam::Vec2;

use super::font::Face;
use super::{Align, Painter, Rgba, TextStyle, measure, with_alpha};

// Color (linear light). Each has one job.

/// Behind everything on a panel.
pub const GROUND: Rgba = [0.011, 0.0115, 0.0125, 1.0];
/// What you read, and what you read second.
pub const TEXT: Rgba = [0.70, 0.71, 0.73, 1.0];
pub const TEXT_2: Rgba = [0.36, 0.365, 0.38, 1.0];
/// Yours: what's selected, your progress, where you are.
pub const AMBER: Rgba = [1.0, 0.5, 0.05, 1.0];
/// Amber's complement: crystal, and nothing else.
pub const BLUE: Rgba = [0.12, 0.42, 1.0, 1.0];

// Space: everything's a multiple of one unit.

pub const UNIT: f32 = 8.0;
pub const MARGIN: f32 = UNIT * 8.0;
/// A panel's column of content.
pub const COLUMN: f32 = UNIT * 40.0;
/// From one action to the next.
pub const ACTION: f32 = UNIT * 7.0;

// Type: one light, wide face in capitals, three sizes.

const ACTION_TEXT: TextStyle = style(17.0);
pub const BODY: TextStyle = style(11.0);
pub const SMALL: TextStyle = style(8.5);

const fn style(size: f32) -> TextStyle {
    TextStyle {
        size,
        tracking: size * 0.1,
        weight: 1.0,
        color: TEXT,
        lower: false,
        face: Face::Michroma,
    }
}

impl TextStyle {
    pub fn colored(self, color: Rgba) -> TextStyle {
        TextStyle { color, ..self }
    }
}

/// The signature shape: a corner cut at 45 degrees.
const CUT: f32 = UNIT * 6.0;

/// The left-hand panel, `h` tall, its top right corner cut. Text on it has no halo.
pub fn panel(p: &mut Painter, h: f32) {
    let w = MARGIN * 2.0 + COLUMN;
    p.poly(
        &[
            Vec2::ZERO,
            Vec2::new(w - CUT, 0.0),
            Vec2::new(w, CUT),
            Vec2::new(w, h),
            Vec2::new(0.0, h),
        ],
        GROUND,
    );
    p.set_halo(None);
    p.line(Vec2::new(w - CUT, 0.0), Vec2::new(w, CUT), 2.0, AMBER);
}

/// How far along you are: an amber line on a faint one.
pub fn bar(p: &mut Painter, at: Vec2, width: f32, frac: f32) {
    let rail = with_alpha(TEXT_2, 0.5);
    p.rect(at, Vec2::new(width, 2.0), rail);
    p.rect(at, Vec2::new(width * frac.clamp(0.0, 1.0), 2.0), AMBER);
}

/// A main action, its caps' top at `at`: amber with a mark when selected.
/// Returns its clickable rect (top left, size).
pub fn action(p: &mut Painter, at: Vec2, name: &str, on: bool) -> (Vec2, Vec2) {
    let pad = (ACTION - ACTION_TEXT.size) * 0.5;
    if on {
        let c = Vec2::new(at.x - UNIT * 2.0, at.y + ACTION_TEXT.size * 0.5);
        let r = UNIT * 0.5;
        p.poly(
            &[
                c - Vec2::new(0.0, r),
                c + Vec2::new(r, 0.0),
                c + Vec2::new(0.0, r),
                c - Vec2::new(r, 0.0),
            ],
            AMBER,
        );
    }
    let color = if on { AMBER } else { TEXT };
    p.text(at, name, &ACTION_TEXT.colored(color), Align::Left);
    (
        at - Vec2::new(UNIT * 3.0, pad),
        Vec2::new(COLUMN + UNIT * 3.0, ACTION),
    )
}

/// A small action, off to the side. Returns its clickable rect.
pub fn link(p: &mut Painter, at: Vec2, name: &str, on: bool) -> (Vec2, Vec2) {
    let color = if on { AMBER } else { TEXT_2 };
    p.text(at, name, &SMALL.colored(color), Align::Left);
    (
        at - Vec2::splat(UNIT),
        Vec2::new(measure(name, &SMALL), SMALL.size) + Vec2::splat(UNIT * 2.0),
    )
}
