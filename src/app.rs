//! Windowed app: winit event loop, input mapping, fixed-step simulation, rendering.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::game::Clock;
use crate::render::Renderer;
use crate::scene::{self, ChaseCam};
use crate::sim::{Command, HullKind, TurretKind, World, wrap_angle};
use crate::tank::TankArt;
use crate::{ShowroomArgs, showroom};

const MOUSE_SENS: f32 = 0.0025;
const KEY_TURRET_SPEED: f32 = 2.2;

struct App {
    seed: u64,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    art: Option<TankArt>,
    showroom: Option<ShowroomArgs>,
    world: World,
    cam: ChaseCam,
    clock: Clock,
    keys: HashSet<KeyCode>,
    grabbed: bool,
    last: Instant,
    fps_timer: f32,
    fps_frames: u32,
}

pub fn run(seed: u64, showroom: Option<ShowroomArgs>) -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let world = if showroom.is_some() { showroom::world(seed) } else { World::new(seed) };
    let mut cam = ChaseCam::new();
    cam.yaw = world.tanks[0].yaw;
    let mut app = App {
        seed,
        window: None,
        renderer: None,
        art: None,
        showroom,
        world,
        cam,
        clock: Clock::default(),
        keys: HashSet::new(),
        grabbed: false,
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
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                if !self.grabbed {
                    self.set_grab(true);
                } else {
                    // Visual only until shooting lands: recoil, flash, hull kick.
                    self.world.tanks[0].fire_fx();
                }
            }
            WindowEvent::Focused(false) => {
                self.keys.clear();
                self.set_grab(false);
            }
            WindowEvent::Resized(size) => {
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => self.frame(),
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.grabbed {
                self.cam.yaw = wrap_angle(self.cam.yaw - delta.0 as f32 * MOUSE_SENS);
                self.cam.tilt = (self.cam.tilt + delta.1 as f32 * MOUSE_SENS * 0.6).clamp(0.0, 1.0);
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

impl App {
    fn on_key(&mut self, code: KeyCode, event_loop: &ActiveEventLoop) {
        match code {
            KeyCode::Escape => {
                if self.grabbed {
                    self.set_grab(false);
                } else {
                    event_loop.exit();
                }
            }
            // New random map.
            KeyCode::F5 => {
                self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) >> 16;
                self.world = World::new(self.seed);
                self.cam = ChaseCam::new();
                self.cam.yaw = self.world.tanks[0].yaw;
                if let Some(r) = &mut self.renderer {
                    r.set_static_mesh(&scene::map_mesh(&self.world.map));
                }
                log::info!("map seed {}", self.seed);
            }
            // Cycle through the tank kit.
            KeyCode::Digit1 => {
                let t = &mut self.world.tanks[0];
                t.hull_kind = if t.hull_kind == HullKind::Lancer { HullKind::Bastion } else { HullKind::Lancer };
            }
            KeyCode::Digit2 => {
                let t = &mut self.world.tanks[0];
                t.turret_kind = if t.turret_kind == TurretKind::Lance { TurretKind::Maul } else { TurretKind::Lance };
            }
            KeyCode::Space => self.world.tanks[0].fire_fx(),
            // Snap the aim back over the hull.
            KeyCode::KeyC => self.cam.yaw = self.world.tanks[0].yaw,
            _ => {}
        }
    }

    fn set_grab(&mut self, grab: bool) {
        let Some(w) = &self.window else { return };
        if grab {
            let ok = w
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| w.set_cursor_grab(CursorGrabMode::Confined))
                .is_ok();
            w.set_cursor_visible(!ok);
            self.grabbed = ok;
        } else {
            let _ = w.set_cursor_grab(CursorGrabMode::None);
            w.set_cursor_visible(true);
            self.grabbed = false;
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
            aim_yaw: self.cam.yaw,
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32();
        self.last = now;

        // Keyboard turret control (Z/X or Q/E) for players who prefer it to the mouse.
        let turn = (self.held(&[KeyCode::KeyZ, KeyCode::KeyQ]) as i32 - self.held(&[KeyCode::KeyX, KeyCode::KeyE]) as i32) as f32;
        self.cam.yaw = wrap_angle(self.cam.yaw + turn * KEY_TURRET_SPEED * dt);

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
            let mut camera = self.cam.update(&self.world, &self.world.tanks[0], dt);
            if let Some(room) = self.showroom {
                // Mouse (once captured) or the slow auto-orbit picks the angle.
                let angle = room.orbit.unwrap_or(0.0) + if self.grabbed { self.cam.yaw } else { self.world.time * 0.15 };
                camera = showroom::camera(&self.world, angle, room.dist * (0.6 + self.cam.tilt), room.focus);
            }
            let mut scene = scene::build(&self.world, art, camera, r.width as f32, r.height as f32);
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
                let hint = if self.grabbed { "Esc: release mouse" } else { "click to aim with mouse" };
                w.set_title(&format!("tankit  |  {fps:.0} fps  |  seed {}  |  {hint}  |  F5: new map", self.seed));
            }
            self.fps_timer = 0.0;
            self.fps_frames = 0;
        }
    }
}
