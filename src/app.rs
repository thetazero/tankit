//! Windowed app: winit event loop, input mapping, fixed-step simulation, rendering.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::game::Clock;
use crate::render::Renderer;
use crate::scene::{self, ChaseCam};
use crate::sim::{Command, HullKind, HullStats, TurretKind, World};
use crate::tank::TankArt;
use crate::{ShowroomArgs, make_map, showroom};

/// Camera height change per second while PageUp/PageDown is held (0..1 range).
const CAM_TILT_SPEED: f32 = 0.8;

struct App {
    seed: u64,
    proving: bool,
    spawn: usize,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    art: Option<TankArt>,
    showroom: Option<ShowroomArgs>,
    world: World,
    cam: ChaseCam,
    clock: Clock,
    keys: HashSet<KeyCode>,
    /// Live tuning (player tank): sideways grip multiplier and turn-rate offset.
    tune_grip: f32,
    tune_turn: f32,
    last: Instant,
    fps_timer: f32,
    fps_frames: u32,
}

pub fn run(seed: u64, proving: bool, spawn: usize, showroom: Option<ShowroomArgs>) -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let world = if showroom.is_some() { showroom::world() } else { World::new(make_map(seed, proving), spawn) };
    let mut app = App {
        seed,
        proving,
        spawn,
        window: None,
        renderer: None,
        art: None,
        showroom,
        world,
        cam: ChaseCam::new(),
        clock: Clock::default(),
        keys: HashSet::new(),
        tune_grip: 1.0,
        tune_turn: 0.0,
        last: Instant::now(),
        fps_timer: 0.0,
        fps_frames: 0,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("tankit")
            .with_inner_size(winit::dpi::LogicalSize::new(1600.0, 900.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        match Renderer::new_windowed(window.clone()) {
            Ok(mut r) => {
                if self.showroom.is_some() {
                    r.set_static_mesh(&showroom::ground_mesh());
                } else {
                    r.set_static_mesh(&scene::map_mesh(&self.world.map));
                }
                self.art = Some(TankArt::upload(&mut r));
                self.renderer = Some(r);
            }
            Err(e) => {
                log::error!("renderer init failed: {e:#}");
                event_loop.exit();
                return;
            }
        }
        self.window = Some(window);
        self.last = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else { return };
                match event.state {
                    ElementState::Pressed => {
                        if !event.repeat {
                            self.on_key(code, event_loop);
                        }
                        self.keys.insert(code);
                    }
                    ElementState::Released => {
                        self.keys.remove(&code);
                    }
                }
            }
            WindowEvent::Focused(false) => self.keys.clear(),
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

impl App {
    /// Rebuild the world on the current map settings, keeping the player's tank kit.
    fn restart(&mut self) {
        let (hull, turret) = (self.world.tanks[0].hull_kind, self.world.tanks[0].turret_kind);
        self.world = World::new(make_map(self.seed, self.proving), self.spawn);
        self.world.set_hull(0, hull);
        self.world.tanks[0].turret_kind = turret;
        self.cam = ChaseCam::new();
        if let Some(r) = &mut self.renderer {
            r.set_static_mesh(&scene::map_mesh(&self.world.map));
        }
    }

    fn on_key(&mut self, code: KeyCode, event_loop: &ActiveEventLoop) {
        match code {
            KeyCode::Escape => event_loop.exit(),
            // Restart at the next spawn point (cycles the proving ground lanes).
            KeyCode::KeyR if self.showroom.is_none() => {
                self.spawn += 1;
                self.restart();
            }
            // New random map.
            KeyCode::F5 if self.showroom.is_none() => {
                self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) >> 16;
                self.proving = false;
                self.restart();
                log::info!("map seed {}", self.seed);
            }
            // Cycle through the tank kit.
            KeyCode::Digit1 => {
                let next = if self.world.tanks[0].hull_kind == HullKind::Lancer { HullKind::Bastion } else { HullKind::Lancer };
                self.world.set_hull(0, next);
            }
            KeyCode::Digit2 => {
                let t = &mut self.world.tanks[0];
                t.turret_kind = if t.turret_kind == TurretKind::Lance { TurretKind::Maul } else { TurretKind::Lance };
            }
            // No projectiles yet: recoil, flash, and a real kick to the hull.
            KeyCode::Space => self.world.fire(0),
            // Live tuning: sideways grip and turn rate.
            KeyCode::BracketLeft => self.tune_grip *= 0.9,
            KeyCode::BracketRight => self.tune_grip *= 1.1,
            KeyCode::Semicolon => self.tune_turn -= 0.1,
            KeyCode::Quote => self.tune_turn += 0.1,
            KeyCode::Backspace => (self.tune_grip, self.tune_turn) = (1.0, 0.0),
            _ => {}
        }
    }

    fn held(&self, codes: &[KeyCode]) -> bool {
        codes.iter().any(|c| self.keys.contains(c))
    }

    fn command(&self) -> Command {
        let axis = |pos: &[KeyCode], neg: &[KeyCode]| (self.held(pos) as i32 - self.held(neg) as i32) as f32;
        Command {
            throttle: axis(&[KeyCode::KeyW, KeyCode::ArrowUp], &[KeyCode::KeyS, KeyCode::ArrowDown]),
            steer: axis(&[KeyCode::KeyA, KeyCode::ArrowLeft], &[KeyCode::KeyD, KeyCode::ArrowRight]),
            turret: axis(&[KeyCode::KeyZ], &[KeyCode::KeyX]),
            center_turret: self.held(&[KeyCode::KeyC]),
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32();
        self.last = now;

        let tilt = (self.held(&[KeyCode::PageUp]) as i32 - self.held(&[KeyCode::PageDown]) as i32) as f32;
        self.cam.tilt = (self.cam.tilt + tilt * CAM_TILT_SPEED * dt).clamp(0.0, 1.0);

        // Apply live tuning on top of the hull's base stats (survives hull swaps).
        if self.showroom.is_none() {
            let t = &mut self.world.tanks[0];
            let base = HullStats::of(t.hull_kind);
            t.hull.grip_lat = base.grip_lat * self.tune_grip;
            t.hull.grip_lat_turning = base.grip_lat_turning * self.tune_grip;
            t.hull.turn_rate = (base.turn_rate + self.tune_turn).max(0.2);
        }

        let cmd = self.command();
        for _ in 0..self.clock.advance(dt) {
            if self.showroom.is_some() {
                let cmds = showroom::commands(&self.world);
                self.world.step(&cmds);
                showroom::after_step(&mut self.world);
            } else {
                self.world.step(&[cmd]);
            }
        }

        if let (Some(r), Some(art)) = (&mut self.renderer, &self.art) {
            let alpha = self.clock.alpha();
            let tanks: Vec<_> = self.world.tanks.iter().map(|t| t.interpolated(alpha)).collect();
            let mut camera = self.cam.update(&self.world, &tanks[0], dt);
            if let Some(room) = self.showroom {
                // Slow auto-orbit; PageUp/PageDown zoom.
                let angle = room.orbit.unwrap_or(self.world.time * 0.15);
                camera = showroom::camera(&self.world, angle, room.dist * (0.6 + self.cam.tilt), room.focus);
            }
            let mut scene = scene::build(&self.world, &tanks, art, camera, r.width as f32, r.height as f32);
            if self.showroom.is_some() {
                scene.ui.clear();
                scene.shadow_center = glam::Vec3::ZERO;
                scene.shadow_radius = 22.0;
            }
            r.render(&scene);
        }

        self.fps_timer += dt;
        self.fps_frames += 1;
        if self.fps_timer >= 0.5 {
            if let Some(w) = &self.window {
                let fps = self.fps_frames as f32 / self.fps_timer;
                let t = &self.world.tanks[0];
                w.set_title(&format!(
                    "tankit  |  {fps:.0} fps  |  {:>4.1} m/s  {}  |  side grip x{:.2} ([ ])  turn {:.1} rad/s (; ')  Backspace reset  |  WASD, Z/X/C turret, Space fire, 1/2 parts, R respawn",
                    t.speed(),
                    if t.grounded() { "ground" } else { "AIR" },
                    self.tune_grip,
                    t.hull.turn_rate,
                ));
            }
            self.fps_timer = 0.0;
            self.fps_frames = 0;
        }
    }
}
