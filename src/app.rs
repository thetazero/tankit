//! Windowed app: winit event loop driving the fixed-step game and the renderer.

use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::game::{Clock, Game};
use crate::render::{Frame, Renderer};

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    game: Game,
    clock: Clock,
    last: Instant,
    fps_timer: f32,
    fps_frames: u32,
}

pub fn run() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        window: None,
        renderer: None,
        game: Game::default(),
        clock: Clock::default(),
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
            Ok(r) => self.renderer = Some(r),
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
            WindowEvent::KeyboardInput {
                event: KeyEvent { physical_key: PhysicalKey::Code(KeyCode::Escape), state: ElementState::Pressed, .. },
                ..
            } => event_loop.exit(),
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
    fn frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32();
        self.last = now;

        for _ in 0..self.clock.advance(dt) {
            self.game.step();
        }

        if let Some(r) = &mut self.renderer {
            r.render(&Frame { time: self.game.time() });
        }

        self.fps_timer += dt;
        self.fps_frames += 1;
        if self.fps_timer >= 0.5 {
            if let Some(w) = &self.window {
                let fps = self.fps_frames as f32 / self.fps_timer;
                w.set_title(&format!("tankit  |  {fps:.0} fps  |  tick {}", self.game.ticks));
            }
            self.fps_timer = 0.0;
            self.fps_frames = 0;
        }
    }
}
