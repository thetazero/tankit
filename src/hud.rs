//! Driving HUD in the hairline style: readouts in the four corners inside
//! faint brackets, the middle left to the city and the crosshair.
//!
//! Only shows what the sim has today (speed, heading, turret angle, firing).
//! Hull, XP, ammo and reload can use `widgets::segments`, `hairbar` and
//! `pips` once those systems exist.

use glam::Vec2;

use crate::render::{Camera, UiVertex};
use crate::scene::project;
use crate::sim::Tank;
use crate::ui::widgets::{self, label};
use crate::ui::{Align, Painter, TextStyle, theme};

/// Logical screen height; everything is laid out at this size and scaled.
const REF_HEIGHT: f32 = 900.0;
const MARGIN: f32 = 40.0;

/// Eased values so readouts glide instead of snapping.
pub struct Hud {
    speed: f32,
    heading: f32,
    initialized: bool,
}

impl Hud {
    pub fn new() -> Hud {
        Hud { speed: 0.0, heading: 0.0, initialized: false }
    }

    /// `tank` is the player's tank as drawn this frame (interpolated).
    pub fn draw(&mut self, tank: &Tank, seed: u64, camera: &Camera, width: f32, height: f32, dt: f32) -> Vec<UiVertex> {
        let speed = tank.speed().abs() * 3.6;
        let max_speed = tank.hull.max_speed * 3.6;
        // Turning right (negative yaw) should read as an increasing compass heading.
        let heading = (-tank.turret_yaw()).to_degrees().rem_euclid(360.0);

        if !self.initialized {
            (self.speed, self.heading, self.initialized) = (speed, heading, true);
        }
        let k = 1.0 - (-dt * 10.0).exp();
        self.speed += (speed - self.speed) * k;
        let dh = (heading - self.heading + 540.0).rem_euclid(360.0) - 180.0;
        self.heading = (self.heading + dh * k).rem_euclid(360.0);

        let scale = height / REF_HEIGHT;
        let (w, h) = (width / scale, REF_HEIGHT);
        let mut p = Painter::new(scale);

        widgets::corners(&mut p, Vec2::splat(18.0), Vec2::new(w - 18.0, h - 18.0), 22.0, theme::FAINT);

        // Top left: map seed.
        label(&mut p, Vec2::new(MARGIN, MARGIN), "GRID", Align::Left);
        p.text(Vec2::new(MARGIN, MARGIN + 14.0), &seed.to_string(), &TextStyle::value(13.0), Align::Left);

        // Top centre: turret heading.
        let top = Vec2::new(w * 0.5, MARGIN + 6.0);
        widgets::compass(&mut p, top, 360.0, self.heading);
        let hdg = self.heading.round() as u32 % 360;
        p.text(top + Vec2::new(0.0, 32.0), &format!("{hdg:03}"), &TextStyle::value(11.0), Align::Center);

        // Crosshair where the turret points; the gap kicks open when firing.
        let vp = camera.view_proj(width / height);
        // Where the gun actually points, including hull tilt.
        let aim_point = tank.pos + tank.up() * (tank.chassis.hull_top + 0.4) + tank.turret_dir() * 40.0;
        if let Some((x, y)) = project(vp, aim_point, width, height) {
            let kick = (-tank.since_fire * 12.0).exp();
            widgets::crosshair(&mut p, Vec2::new(x, y) / scale, kick, theme::FG);
        }

        // Bottom left: speed.
        let bl = Vec2::new(MARGIN, h - MARGIN);
        label(&mut p, bl + Vec2::new(0.0, -62.0), "SPEED", Align::Left);
        let num = TextStyle::value(22.0);
        let nw = widgets::odometer(&mut p, bl + Vec2::new(0.0, -48.0), self.speed, 3, &num, Align::Left);
        label(&mut p, bl + Vec2::new(nw + 10.0, -48.0 + num.size - TextStyle::LABEL.size), "KM/H", Align::Left);
        widgets::hairbar(&mut p, bl + Vec2::new(0.0, -8.0), 200.0, self.speed / max_speed, theme::FG);

        // Bottom right: turret angle relative to the hull.
        let br = Vec2::new(w - MARGIN, h - MARGIN);
        widgets::turret_gauge(&mut p, br + Vec2::new(-30.0, -30.0), tank.turret_rel);
        let rel = tank.turret_rel.to_degrees().round() as i32;
        let side = if rel > 0 { " L" } else if rel < 0 { " R" } else { "" };
        label(&mut p, br + Vec2::new(-84.0, -62.0), "TURRET", Align::Right);
        p.text(br + Vec2::new(-84.0, -48.0), &format!("{:03}{side}", rel.abs()), &TextStyle::value(14.0), Align::Right);

        // Bottom centre: how to get to the garage.
        let mut key = TextStyle::LABEL;
        key.color = theme::FG;
        let hint = Vec2::new(w * 0.5, h - MARGIN - 8.0);
        let gw = crate::ui::measure("GARAGE", &TextStyle::LABEL);
        p.text(hint - Vec2::new(gw * 0.5 + 10.0, 0.0), "G", &key, Align::Right);
        label(&mut p, hint, "GARAGE", Align::Center);

        p.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::World;
    use crate::render::Camera;
    use glam::Vec3;

    #[test]
    fn draws_at_any_size() {
        let world = World::new(crate::map::Map::generate(21), 0);
        let t = &world.tanks[0];
        // Chase-camera framing: behind the turret, looking where it points.
        let d = t.turret_dir();
        let camera = Camera { pos: t.pos - d * 10.0 + Vec3::Y * 6.0, target: t.pos + d * 6.0, fov_y: 1.1 };
        let mut hud = Hud::new();
        for (w, h) in [(1600.0, 900.0), (800.0, 600.0), (3840.0, 2160.0)] {
            let v = hud.draw(t, 21, &camera, w, h, 1.0 / 60.0);
            assert!(!v.is_empty());
            // A little slack for halos and anti-aliasing at the screen edge.
            assert!(v.iter().all(|v| v.pos[0] > -4.0 && v.pos[0] < w + 4.0 && v.pos[1] > -4.0 && v.pos[1] < h + 4.0));
        }
    }
}
