//! What stands over a bridge's deck, each faction's its own: the Imperium's
//! chains between stone towers, the Resonant's gilt hoops, the Uthenn's
//! masts and stays, the Ithessar's glass arches. All of it's outside the
//! deck's rails or well over a tank, so it's driven the same whoever's it is.

use glam::{Vec2, Vec3};

use super::Look;
use crate::map::Map;
use crate::render::material::Material;

/// A bridge to dress: its deck from end to end.
#[derive(Clone, Copy)]
pub struct Deck {
    pub from: Vec2,
    pub to: Vec2,
    /// Half its width.
    pub half: f32,
    pub top: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Chains,
    Hoops,
    Stays,
    Arches,
}

impl Form {
    pub fn of(look: Look) -> Form {
        match look {
            Look::Imperium => Form::Chains,
            Look::Resonant => Form::Hoops,
            Look::Uthenn => Form::Stays,
            Look::Ithessar => Form::Arches,
        }
    }
}

type Paint = (Vec3, Material);

/// What a faction builds with: what stands, its trim, its cables.
struct Paints {
    frame: Paint,
    trim: Paint,
    cable: Paint,
}

fn paints(look: Look) -> Paints {
    let c = Vec3::new;
    let (frame, trim, cable) = match look {
        Look::Imperium => (
            (c(0.54, 0.53, 0.51), Material::Concrete),
            (c(0.62, 0.46, 0.16), Material::Metal),
            (c(0.16, 0.14, 0.13), Material::Metal),
        ),
        Look::Resonant => (
            (c(0.62, 0.47, 0.22), Material::Metal),
            (c(0.74, 0.69, 0.58), Material::Plaster),
            (c(0.2, 0.4, 0.9), Material::Glow),
        ),
        Look::Uthenn => (
            (c(0.44, 0.49, 0.46), Material::Metal),
            (c(0.15, 0.9, 0.35), Material::Glow),
            (c(0.12, 0.15, 0.14), Material::Metal),
        ),
        Look::Ithessar => (
            (c(0.06, 0.09, 0.17), Material::Vhaal),
            (c(0.28, 0.2, 0.14), Material::Metal),
            (c(1.0, 0.16, 0.52), Material::Glow),
        ),
    };
    Paints { frame, trim, cable }
}

/// Over this, above a deck, nothing's solid: shells pass, and the camera.
const SOLID_TO: f32 = 4.5;
/// How far outside the deck's edge it stands.
const OUT: f32 = 0.5;

struct Builder<'a> {
    map: &'a mut Map,
    d: Deck,
    dir: Vec2,
    side: Vec2,
    len: f32,
}

impl Builder<'_> {
    /// `t` along, `x` across, `y` over the deck.
    fn at(&self, (t, x, y): (f32, f32, f32)) -> Vec3 {
        let p = self.d.from + self.dir * t + self.side * x;
        Vec3::new(p.x, self.d.top + y, p.y)
    }

    fn bar(&mut self, a: (f32, f32, f32), b: (f32, f32, f32), r: f32, paint: Paint) {
        // (Only the thick, low parts, not what hangs under the deck.)
        let low = a.2.min(b.2);
        let solid = r >= 0.3 && (-0.1..SOLID_TO).contains(&low);
        let upright = (a.0, a.1) == (b.0, b.1);
        let (a, b) = (self.at(a), self.at(b));
        self.map.segment(a, b, r, paint.0, paint.1, solid);
        if upright && let Some(o) = self.map.oboxes.last_mut() {
            // (Square to the deck.)
            o.yaw = self.dir.x.atan2(self.dir.y);
        }
    }

    /// A post of shrinking blocks at (`t`, `x`), up to `h`, its foot under
    /// the deck.
    fn post(&mut self, t: f32, x: f32, h: f32, r: f32, p: &Paints) {
        self.bar((t, x, -3.5), (t, x, 0.0), r, p.frame);
        self.bar((t, x, 0.0), (t, x, h * 0.45), r * 0.95, p.frame);
        self.bar((t, x, h * 0.45), (t, x, h), r * 0.75, p.frame);
        self.bar((t, x, h), (t, x, h + 0.7), r * 0.5, p.trim);
    }
}

/// Dresses `d` the way `look` does, in `form`.
pub fn dress(map: &mut Map, d: Deck, look: Look, form: Form) {
    let len = d.from.distance(d.to);
    let dir = (d.to - d.from) / len;
    let p = paints(look);
    let mut b = Builder {
        map,
        d,
        dir,
        side: dir.perp(),
        len,
    };
    match form {
        Form::Chains => chains(&mut b, &p),
        Form::Hoops => hoops(&mut b, &p),
        Form::Stays => stays(&mut b, &p),
        Form::Arches => arches(&mut b, &p),
    }
}

/// Two stone gateways, a chain slung between them along each side.
fn chains(b: &mut Builder, p: &Paints) {
    let x = b.d.half + OUT;
    let h = (b.len * 0.2).clamp(8.0, 12.0);
    let (t0, t1) = (b.len * 0.17, b.len * 0.83);
    for t in [t0, t1] {
        for e in [-1.0f32, 1.0] {
            b.post(t, e * x, h, 0.75, p);
        }
        b.bar((t, -x, h * 0.85), (t, x, h * 0.85), 0.45, p.frame);
        b.bar((t, -x, h * 0.96), (t, x, h * 0.96), 0.16, p.trim);
    }
    let n = 12;
    for e in [-1.0f32, 1.0] {
        let x = e * x;
        // Down to each end.
        b.bar((t0, x, h), (0.5, x, 0.5), 0.12, p.cable);
        b.bar((t1, x, h), (b.len - 0.5, x, 0.5), 0.12, p.cable);
        let at = |k: i32| {
            let u = k as f32 / n as f32;
            let sag = (2.0 * u - 1.0).powi(2);
            (t0 + (t1 - t0) * u, x, 1.6 + (h - 1.6) * sag)
        };
        for k in 0..n {
            b.bar(at(k), at(k + 1), 0.12, p.cable);
            if k > 0 {
                let (t, x, y) = at(k);
                b.bar((t, x, -0.4), (t, x, y), 0.05, p.cable);
            }
        }
    }
}

/// Pointed hoops over the deck, taller toward the middle, a ridge along
/// their tops and a shaft of light hung from each.
fn hoops(b: &mut Builder, p: &Paints) {
    let x = b.d.half + OUT;
    let n = (b.len / 9.0).round().max(3.0) as i32;
    let tip = |k: i32| {
        let u = (k as f32 + 0.5) / n as f32;
        let h = 7.5 + 3.5 * (u * std::f32::consts::PI).sin();
        (b.len * u, h)
    };
    let tips: Vec<(f32, f32)> = (0..n).map(tip).collect();
    for &(t, h) in &tips {
        for e in [-1.0f32, 1.0] {
            let line = [
                (1.0, -1.2),
                (1.0, h * 0.45),
                (0.85, h * 0.7),
                (0.5, h * 0.89),
                (0.0, h),
            ];
            for w in line.windows(2) {
                b.bar(
                    (t, e * x * w[0].0, w[0].1),
                    (t, e * x * w[1].0, w[1].1),
                    0.3,
                    p.frame,
                );
            }
        }
        b.bar((t, 0.0, h - 1.5), (t, 0.0, h - 0.2), 0.09, p.cable);
        b.bar((t, 0.0, h), (t, 0.0, h + 1.1), 0.12, p.trim);
    }
    for w in tips.windows(2) {
        b.bar((w[0].0, 0.0, w[0].1), (w[1].0, 0.0, w[1].1), 0.15, p.trim);
    }
}

/// A pair of masts raked back at one end, stays fanned from them along the
/// deck.
fn stays(b: &mut Builder, p: &Paints) {
    let x = b.d.half + OUT;
    let h = (b.len * 0.36).clamp(11.0, 20.0);
    let (foot, head) = (b.len * 0.2, b.len * 0.08);
    for e in [-1.0f32, 1.0] {
        let x = e * x;
        let lean = |f: f32| (foot + (head - foot) * f, x, h * f);
        b.bar((foot, x, -3.5), lean(0.0), 0.6, p.frame);
        b.bar(lean(0.0), lean(0.4), 0.55, p.frame);
        b.bar(lean(0.4), lean(1.0), 0.4, p.frame);
        b.bar(lean(1.0), lean(1.05), 0.2, p.trim);
        let n = 7;
        for k in 0..n {
            let f = k as f32 / (n - 1) as f32;
            let t = b.len * (0.36 + 0.58 * f);
            b.bar(lean(0.5 + 0.45 * f), (t, x, 0.0), 0.06, p.cable);
        }
        b.bar(lean(0.95), (0.4, x, 0.0), 0.08, p.cable);
        // A seam along the deck's edge.
        b.bar((foot + 1.0, x, 0.25), (b.len - 1.0, x, 0.25), 0.07, p.trim);
    }
    let t = foot + (head - foot) * 0.75;
    b.bar((t, -x, h * 0.75), (t, x, h * 0.75), 0.25, p.frame);
}

/// An arch along each side, leaning in toward the other, the deck hung from
/// them.
fn arches(b: &mut Builder, p: &Paints) {
    let h = (b.len * 0.2).clamp(8.0, 12.0);
    let n = 16;
    let at = |b: &Builder, k: i32, e: f32| {
        let u = k as f32 / n as f32;
        let f = 4.0 * u * (1.0 - u);
        let x = b.d.half + OUT - f * b.d.half * 0.3;
        (1.2 + (b.len - 2.4) * u, e * x, -1.0 + (h + 1.0) * f)
    };
    for e in [-1.0f32, 1.0] {
        for k in 0..n {
            let (l, r) = (at(b, k, e), at(b, k + 1, e));
            let thick = 0.6 - 0.25 * l.2.min(r.2).max(0.0) / h;
            b.bar(l, r, thick, p.frame);
            if k > 1 && k % 2 == 0 {
                b.bar((l.0, e * (b.d.half + OUT), -0.3), l, 0.06, p.cable);
            }
        }
    }
    for k in [5, 8, 11] {
        let (l, r) = (at(b, k, -1.0), at(b, k, 1.0));
        b.bar(l, r, 0.22, p.trim);
    }
}
