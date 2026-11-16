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
use crate::sim::{Command, World};

/// Camera height change per second while PageUp/PageDown is held (0..1 range).
const CAM_TILT_SPEED: f32 = 0.8;

struct App {
    seed: u64,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    world: World,
    cam: ChaseCam,
    clock: Clock,
    keys: HashSet<KeyCode>,
    last: Instant,
    fps_timer: f32,
    fps_frames: u32,
}

pub fn run(seed: u64) -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let world = World::new(seed);
    let cam = ChaseCam::new();
    let mut app = App {
        seed,
        window: None,
        renderer: None,
        world,
        cam,
        clock: Clock::default(),
        keys: HashSet::new(),
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
                r.set_static_mesh(&scene::map_mesh(&self.world.map));
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
    fn on_key(&mut self, code: KeyCode, event_loop: &ActiveEventLoop) {
        match code {
            KeyCode::Escape => event_loop.exit(),
            // New random map.
            KeyCode::F5 => {
                self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407) >> 16;
                self.world = World::new(self.seed);
                self.cam = ChaseCam::new();
                if let Some(r) = &mut self.renderer {
                    r.set_static_mesh(&scene::map_mesh(&self.world.map));
                }
                log::info!("map seed {}", self.seed);
            }
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

        let cmd = self.command();
        for _ in 0..self.clock.advance(dt) {
            self.world.step(&[cmd]);
        }

        if let Some(r) = &mut self.renderer {
            let camera = self.cam.update(&self.world, &self.world.tanks[0], dt);
            let scene = scene::build(&self.world, camera, r.width as f32, r.height as f32);
            r.render(&scene);
        }

        self.fps_timer += dt;
        self.fps_frames += 1;
        if self.fps_timer >= 0.5 {
            if let Some(w) = &self.window {
                let fps = self.fps_frames as f32 / self.fps_timer;
                w.set_title(&format!(
                    "tankit  |  {fps:.0} fps  |  seed {}  |  WASD drive, Z/X turret, C center, PgUp/PgDn camera, F5 new map",
                    self.seed
                ));
            }
            self.fps_timer = 0.0;
            self.fps_frames = 0;
        }
    }
}
