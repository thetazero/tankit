//! The design system, v0 (`docs/design.md`): the tokens and components every
//! screen is to be made from. Settings, the garage and the HUD still
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
pub const MARGIN: f32 = UNIT * 6.0;
/// A panel's column of content.
pub const COLUMN: f32 = UNIT * 27.0;
/// From one action to the next.
pub const ACTION: f32 = UNIT * 7.0;

// Type: one light, wide face in capitals, four sizes.

/// The one thing a page is about.
pub const TITLE: TextStyle = style(26.0);
/// What the screen's about.
pub const HEAD: TextStyle = style(17.0);
const ACTION_TEXT: TextStyle = HEAD;
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

/// The left-hand panel, `h` tall. Text on it has no halo.
pub fn panel(p: &mut Painter, h: f32) {
    p.set_halo(None);
    p.rect(Vec2::ZERO, Vec2::new(MARGIN * 2.0 + COLUMN, h), GROUND);
}

/// How far along you are: an amber line on a faint one.
pub fn bar(p: &mut Painter, at: Vec2, width: f32, frac: f32) {
    let rail = with_alpha(TEXT_2, 0.5);
    p.rect(at, Vec2::new(width, 2.0), rail);
    p.rect(at, Vec2::new(width * frac.clamp(0.0, 1.0), 2.0), AMBER);
}

/// A main action, its caps' top at `at`: amber when selected.
/// Returns its clickable rect (top left, size).
pub fn action(p: &mut Painter, at: Vec2, name: &str, on: bool) -> (Vec2, Vec2) {
    let pad = (ACTION - ACTION_TEXT.size) * 0.5;
    let color = if on { AMBER } else { TEXT };
    p.text(at, name, &ACTION_TEXT.colored(color), Align::Left);
    (
        at - Vec2::new(UNIT * 2.0, pad),
        Vec2::new(COLUMN + UNIT * 4.0, ACTION),
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

/// How tall a button is.
pub const BUTTON: f32 = UNIT * 5.0;

/// A button `width` wide, its top left at `min`: amber in a box if it can be
/// pressed (`on`), flat and grey, saying why, if it can't.
pub fn button(p: &mut Painter, min: Vec2, width: f32, text: &str, on: bool, hover: bool) {
    let size = Vec2::new(width, BUTTON);
    let fill = match (on, hover) {
        (true, true) => 0.06,
        (true, false) => 0.015,
        (false, _) => 0.008,
    };
    p.rect(min, size, with_alpha(TEXT, fill));
    if on {
        p.rect_outline(min, size, 1.0, TEXT_2);
    }
    let style = BODY.colored(if on { AMBER } else { TEXT_2 });
    let at = Vec2::new(min.x + width * 0.5, min.y + (BUTTON - style.size) * 0.5);
    p.text(at, text, &style, Align::Center);
}

/// Where a panel's foot buttons go on a screen `h` tall, each its column wide:
/// the way back at the bottom (0), and the main one on top of it (1).
pub fn foot(h: f32, k: usize) -> Vec2 {
    let up = (BUTTON + UNIT) * k as f32;
    Vec2::new(MARGIN, h - MARGIN - BUTTON - up)
}
