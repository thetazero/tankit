//! Windowed app: winit event loop, input mapping, fixed-step simulation, rendering.
//!
//! The loop: main menu (missions, garage, quit) → a mission's match → Esc
//! pauses (resume, exit to menu, quit) → when the round ends, Enter plays it
//! again and Esc goes back to the menu. Trust is banked into the pilot profile
//! the moment it's earned, and the profile saves itself on every change.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::audio::{self, Audio};
use crate::combat::{Match, Phase};
use crate::economy::{Part, Payout};
use crate::game::Clock;
use crate::garage::{self, Garage};
use crate::hud::{self, Hud};
use crate::menu::{self, Change, Menu, Nav, Pause, PauseAction};
use crate::mission::{Contract, MISSIONS};
use crate::profile::Profile;
use crate::render::{Camera, Renderer};
use crate::scene::{self, ChaseCam};
use crate::sim::{Command, HullStats};
use crate::tank::TankArt;
use crate::{MapKind, ShowroomArgs, make_map, showroom};

/// Camera height change per second while PageUp/PageDown is held (0..1 range).
const CAM_TILT_SPEED: f32 = 0.8;
/// Main menu camera drift around the parked tank, rad/s.
const MENU_ORBIT: f32 = 0.06;

/// What the window is showing.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    /// Main menu / mission select / garage, over the first mission's map.
    Menu,
    Playing,
    /// The match is frozen under the pause menu.
    Paused,
}

/// How the app starts.
pub struct Launch {
    pub seed: u64,
    pub map_kind: MapKind,
    pub spawn: usize,
    pub bots: usize,
    pub contract: Contract,
    /// Which of the map's sides hires you (0 or 1).
    pub side: usize,
    /// Starting kit; `None` uses the profile's.
    pub loadout: Option<garage::Loadout>,
    /// Open on the main menu (otherwise straight into a match with the settings above).
    pub menu: bool,
    pub showroom: Option<ShowroomArgs>,
}

struct App {
    seed: u64,
    map_kind: MapKind,
    spawn: usize,
    bots: usize,
    contract: Contract,
    /// Which of the map's sides hired you.
    side: usize,
    mode: Mode,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    art: Option<TankArt>,
    showroom: Option<ShowroomArgs>,
    /// The team deathmatch, or the menu's backdrop (its world drives the showroom too, with no bots).
    game: Match,
    profile: Profile,
    /// How much of `game.trust` is already banked into the profile.
    banked: u32,
    /// Whether this round's contract pay has been banked.
    paid: bool,
    menu: Menu,
    pause: Pause,
    /// Main menu camera angle around the parked tank.
    menu_orbit: f32,
    cam: ChaseCam,
    hud: Hud,
    garage: Garage,
    audio: Audio,
    /// Last frame's camera (the garage starts its orbit from it).
    camera: Option<Camera>,
    /// Whether the player was alive last frame (the garage closes on respawn).
    player_alive: bool,
    clock: Clock,
    keys: HashSet<KeyCode>,
    /// Live tuning (player tank): sideways grip multiplier and turn-rate offset.
    tune_grip: f32,
    tune_turn: f32,
    last: Instant,
    fps_timer: f32,
    fps_frames: u32,
    /// Frames per second over the last half second (the debug overlay shows it).
    fps: f32,
}

pub fn run(launch: Launch) -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let profile = match Profile::default_path() {
        Some(path) => Profile::load(path),
        None => {
            log::warn!("no data directory; progress won't be saved");
            Profile::in_memory()
        }
    };
    let loadout = launch.loadout.unwrap_or(profile.loadout);
    let mut game = if launch.showroom.is_some() {
        let mut m = Match::new(
            make_map(launch.seed, launch.map_kind),
            launch.spawn,
            0,
            launch.seed,
        );
        m.world = showroom::world();
        m
    } else if launch.menu {
        menu_match(loadout)
    } else {
        new_match(
            launch.seed,
            launch.map_kind,
            launch.side,
            launch.spawn,
            launch.bots,
            launch.contract,
            loadout,
        )
    };
    if launch.showroom.is_none() {
        game.set_player_levels(profile.levels);
    }
    let menu_orbit = garage::front_angle(&game.world.tanks[0]);
    let volume = profile.settings.volume();
    let mut app = App {
        seed: launch.seed,
        map_kind: launch.map_kind,
        spawn: launch.spawn,
        bots: launch.bots,
        contract: launch.contract,
        side: launch.side,
        mode: if launch.menu && launch.showroom.is_none() {
            Mode::Menu
        } else {
            Mode::Playing
        },
        window: None,
        renderer: None,
        art: None,
        showroom: launch.showroom,
        game,
        profile,
        banked: 0,
        paid: false,
        menu: Menu::new(),
        pause: Pause::new(),
        menu_orbit,
        cam: ChaseCam::new(),
        hud: Hud::new(),
        garage: Garage::new(),
        audio: Audio::new(),
        camera: None,
        player_alive: true,
        clock: Clock::default(),
        keys: HashSet::new(),
        tune_grip: 1.0,
        tune_turn: 0.0,
        last: Instant::now(),
        fps_timer: 0.0,
        fps_frames: 0,
        fps: 0.0,
    };
    app.audio.set_volume(volume);
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// A round under `contract`, hired by the map's side `side`, with the player in `kit`.
pub fn new_match(
    seed: u64,
    map_kind: MapKind,
    side: usize,
    spawn: usize,
    bots: usize,
    contract: Contract,
    kit: garage::Loadout,
) -> Match {
    let mut map = make_map(seed, map_kind);
    if side == 1 {
        map.swap_sides();
    }
    let mut m = Match::new(map, spawn, bots, seed.wrapping_add(spawn as u64));
    m.set_contract(contract);
    m.set_player_hull(kit.0);
    m.world.tanks[0].turret_kind = kit.1;
    m
}

/// The main menu's backdrop: the first mission's map, empty, your tank parked in it.
pub fn menu_match(kit: garage::Loadout) -> Match {
    let first = &MISSIONS[0];
    let mut m = new_match(
        first.seed,
        first.map_kind,
        0,
        0,
        0,
        Contract::Attrition,
        kit,
    );
    m.time_left = f32::INFINITY;
    m
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
                    r.set_static_mesh(&scene::map_mesh(&self.game.world.map));
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
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
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
    /// Swap in a new match (or menu backdrop) and reset everything tied to the old one.
    fn load(&mut self, game: Match) {
        self.game = game;
        self.game.set_player_levels(self.profile.levels);
        self.banked = 0;
        self.paid = false;
        self.cam = ChaseCam::new();
        self.hud = Hud::new();
        self.garage = Garage::new();
        self.player_alive = true;
        if let Some(r) = &mut self.renderer {
            r.set_static_mesh(&scene::map_mesh(&self.game.world.map));
        }
    }

    /// Start a new round on the current map settings, keeping the player's queued kit.
    fn restart(&mut self) {
        let kit = self.queued();
        self.load(new_match(
            self.seed,
            self.map_kind,
            self.side,
            self.spawn,
            self.bots,
            self.contract,
            kit,
        ));
        self.mode = Mode::Playing;
    }

    /// Start mission `MISSIONS[i]`, hired by its side `side`.
    fn start_mission(&mut self, i: usize, side: usize) {
        let m = &MISSIONS[i];
        (self.seed, self.map_kind, self.spawn, self.bots) = (m.seed, m.map_kind, 0, m.bots());
        (self.contract, self.side) = (m.contract, side);
        self.load(new_match(
            self.seed,
            self.map_kind,
            self.side,
            self.spawn,
            self.bots,
            self.contract,
            self.profile.loadout,
        ));
        self.mode = Mode::Playing;
        log::info!(
            "deployed: {} ({}, {}) for the {}",
            m.name,
            m.contract.name(),
            m.map,
            m.sides[side].name()
        );
    }

    fn back_to_menu(&mut self) {
        self.load(menu_match(self.profile.loadout));
        self.menu = Menu::new();
        self.menu_orbit = garage::front_angle(&self.game.world.tanks[0]);
        self.mode = Mode::Menu;
    }

    /// The player's kit for the next respawn (the current one if nothing is queued).
    fn queued(&self) -> garage::Loadout {
        let t = &self.game.world.tanks[0];
        self.game
            .player()
            .next_kit
            .unwrap_or((t.hull_kind, t.turret_kind))
    }

    /// Pick the kit for the next respawn. In a match it's queued for the next
    /// respawn; in the menu the parked tank wears it right away. Either way
    /// it's what you deploy in next time.
    fn queue(&mut self, kit: garage::Loadout) {
        if self.mode == Mode::Menu {
            self.game.set_player_hull(kit.0);
            self.game.world.tanks[0].turret_kind = kit.1;
            self.game.fighters[0].next_kit = None;
        } else {
            let t = &self.game.world.tanks[0];
            self.game.fighters[0].next_kit = (kit != (t.hull_kind, t.turret_kind)).then_some(kit);
        }
        self.profile.set_loadout(kit);
    }

    fn garage_event(&mut self, event: garage::Event) {
        match event {
            garage::Event::Queue(kit) => self.queue(kit),
            // Refusals need no handling: the garage already shows why E won't work.
            garage::Event::Buy(part) => {
                if self.profile.buy(part).is_ok() {
                    log::info!("bought {}", part.name());
                    if let Some(garage::Event::Queue(kit)) =
                        self.garage.bought(self.queued(), &self.profile)
                    {
                        self.queue(kit);
                    }
                }
            }
            garage::Event::Upgrade(part) => {
                if self.profile.upgrade(part).is_ok() {
                    log::info!(
                        "upgraded {} to M{}",
                        part.name(),
                        self.profile.levels.of(part)
                    );
                    // Like a kit change: now in the menu, at the next respawn in battle.
                    if self.mode == Mode::Menu {
                        self.game.set_player_levels(self.profile.levels);
                    } else {
                        self.game.fighters[0].next_levels = Some(self.profile.levels);
                    }
                }
            }
        }
    }

    /// The next owned part after `current` in `list`, for the 1/2 quick-cycle keys.
    fn next_owned<T: Copy + PartialEq>(&self, list: &[T], current: T, part: fn(T) -> Part) -> T {
        let i = list.iter().position(|&k| k == current).unwrap_or(0);
        (1..=list.len())
            .map(|d| list[(i + d) % list.len()])
            .find(|&k| self.profile.owns(part(k)))
            .unwrap_or(current)
    }

    fn nav(code: KeyCode) -> Option<Nav> {
        Some(match code {
            KeyCode::KeyW | KeyCode::ArrowUp => Nav::Up,
            KeyCode::KeyS | KeyCode::ArrowDown => Nav::Down,
            KeyCode::KeyA | KeyCode::ArrowLeft => Nav::Left,
            KeyCode::KeyD | KeyCode::ArrowRight => Nav::Right,
            KeyCode::Escape | KeyCode::Backspace => Nav::Back,
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => Nav::Select,
            _ => return None,
        })
    }

    fn on_key(&mut self, code: KeyCode, event_loop: &ActiveEventLoop) {
        if self.garage.is_open() {
            let input = match code {
                KeyCode::KeyW | KeyCode::ArrowUp => garage::Input::Up,
                KeyCode::KeyS | KeyCode::ArrowDown => garage::Input::Down,
                KeyCode::KeyA | KeyCode::ArrowLeft => garage::Input::Left,
                KeyCode::KeyD | KeyCode::ArrowRight => garage::Input::Right,
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::KeyG => garage::Input::Deploy,
                KeyCode::Escape => garage::Input::Revert,
                KeyCode::KeyE => garage::Input::Act,
                _ => return,
            };
            self.audio.ui(match input {
                garage::Input::Deploy | garage::Input::Act => audio::Ui::Select,
                garage::Input::Revert => audio::Ui::Back,
                _ => audio::Ui::Move,
            });
            if let Some(event) = self.garage.input(input, self.queued(), &self.profile) {
                self.garage_event(event);
            }
            return;
        }
        match self.mode {
            Mode::Menu => {
                let Some(nav) = Self::nav(code) else { return };
                self.nav_sound(nav);
                match self.menu.input(nav) {
                    Some(menu::Action::Deploy(i, side)) => self.start_mission(i, side),
                    Some(menu::Action::Garage) => {
                        if let Some(cam) = self.camera {
                            self.garage.toggle(&self.game.world, &cam, self.queued());
                        }
                    }
                    Some(menu::Action::Setting(c)) => self.change_setting(c),
                    Some(menu::Action::Quit) => event_loop.exit(),
                    None => {}
                }
                return;
            }
            Mode::Paused => {
                let Some(nav) = Self::nav(code) else { return };
                self.nav_sound(nav);
                match self.pause.input(nav) {
                    Some(PauseAction::Resume) => self.mode = Mode::Playing,
                    Some(PauseAction::Setting(c)) => self.change_setting(c),
                    Some(PauseAction::ExitToMenu) => self.back_to_menu(),
                    Some(PauseAction::Quit) => event_loop.exit(),
                    None => {}
                }
                return;
            }
            Mode::Playing => {}
        }
        match code {
            KeyCode::Escape if self.showroom.is_some() => event_loop.exit(),
            // Round over: back to the menu. Otherwise pause.
            KeyCode::Escape if self.game.phase == Phase::Over => self.back_to_menu(),
            KeyCode::Escape => {
                self.audio.ui(audio::Ui::Back);
                self.pause = Pause::new();
                self.mode = Mode::Paused;
            }
            // Garage: park and pick the hull / turret for the next respawn.
            KeyCode::KeyG if self.showroom.is_none() && self.game.phase == Phase::Playing => {
                if let Some(cam) = self.camera {
                    self.garage.toggle(&self.game.world, &cam, self.queued());
                }
            }
            // Play the round again once the clock has run out.
            KeyCode::Enter | KeyCode::NumpadEnter
                if self.showroom.is_none() && self.game.phase == Phase::Over =>
            {
                self.spawn += 1;
                self.restart();
            }
            // Restart at the next spawn point (cycles the proving ground lanes).
            KeyCode::KeyR if self.showroom.is_none() => {
                self.spawn += 1;
                self.restart();
            }
            // New random map.
            KeyCode::F5 if self.showroom.is_none() => {
                self.seed = self
                    .seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407)
                    >> 16;
                self.map_kind = MapKind::City;
                self.restart();
                log::info!("map seed {}", self.seed);
            }
            // Quick-cycle the kit queued for the next respawn.
            KeyCode::Digit1 => {
                let (hull, turret) = self.queued();
                self.queue((self.next_owned(&garage::HULLS, hull, Part::Hull), turret));
            }
            KeyCode::Digit2 => {
                let (hull, turret) = self.queued();
                self.queue((
                    hull,
                    self.next_owned(&garage::TURRETS, turret, Part::Turret),
                ));
            }
            // Live tuning: sideways grip and turn rate.
            KeyCode::BracketLeft => self.tune_grip *= 0.9,
            KeyCode::BracketRight => self.tune_grip *= 1.1,
            KeyCode::Semicolon => self.tune_turn -= 0.1,
            KeyCode::Quote => self.tune_turn += 0.1,
            KeyCode::Backspace => (self.tune_grip, self.tune_turn) = (1.0, 0.0),
            _ => {}
        }
    }

    fn change_setting(&mut self, change: Change) {
        let settings = menu::apply(self.profile.settings, change);
        self.profile.set_settings(settings);
        self.audio.set_volume(settings.volume());
    }

    fn nav_sound(&mut self, nav: Nav) {
        self.audio.ui(match nav {
            Nav::Select => audio::Ui::Select,
            Nav::Back => audio::Ui::Back,
            _ => audio::Ui::Move,
        });
    }

    fn held(&self, codes: &[KeyCode]) -> bool {
        codes.iter().any(|c| self.keys.contains(c))
    }

    fn command(&self) -> Command {
        // Parked in the garage or the menu.
        if self.garage.is_open() || self.mode != Mode::Playing {
            return Command::default();
        }
        let axis = |pos: &[KeyCode], neg: &[KeyCode]| {
            (self.held(pos) as i32 - self.held(neg) as i32) as f32
        };
        Command {
            throttle: axis(
                &[KeyCode::KeyW, KeyCode::ArrowUp],
                &[KeyCode::KeyS, KeyCode::ArrowDown],
            ),
            steer: axis(
                &[KeyCode::KeyA, KeyCode::ArrowLeft],
                &[KeyCode::KeyD, KeyCode::ArrowRight],
            ),
            turret: axis(&[KeyCode::KeyZ], &[KeyCode::KeyX]),
            center_turret: self.held(&[KeyCode::KeyC]),
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32();
        self.last = now;

        let tilt =
            (self.held(&[KeyCode::PageUp]) as i32 - self.held(&[KeyCode::PageDown]) as i32) as f32;
        self.cam.tilt = (self.cam.tilt + tilt * CAM_TILT_SPEED * dt).clamp(0.0, 1.0);

        // Apply live tuning on top of the hull's base stats (survives hull swaps).
        if self.showroom.is_none() {
            let t = &mut self.game.world.tanks[0];
            let base = HullStats::of(t.hull_kind);
            t.hull.grip_lat = base.grip_lat * self.tune_grip;
            t.hull.turn_rate = (base.turn_rate + self.tune_turn).max(0.2);
        }

        let cmd = self.command();
        let fire =
            self.held(&[KeyCode::Space]) && !self.garage.is_open() && self.mode == Mode::Playing;
        if self.mode != Mode::Paused {
            for _ in 0..self.clock.advance(dt) {
                if self.showroom.is_some() {
                    let w = &mut self.game.world;
                    let cmds = showroom::commands(w);
                    w.step(&cmds);
                    showroom::after_step(w);
                } else {
                    self.game.step(cmd, fire);
                }
            }
        }

        // Trust is paid the moment it's earned, to whoever hired you; crystals
        // when the contract is done.
        if self.game.trust > self.banked {
            self.profile
                .add_trust(self.game.factions[0], self.game.trust - self.banked);
            self.banked = self.game.trust;
        }
        if self.mode == Mode::Playing
            && self.showroom.is_none()
            && self.game.phase == Phase::Over
            && !self.paid
        {
            let pay = Payout::of(&self.game);
            self.profile.add_crystals(pay.total());
            self.paid = true;
            log::info!("contract paid {} CR ({pay:?})", pay.total());
        }

        // Respawned (in the new kit) or round over: back to battle.
        let alive = self.game.player().alive;
        if self.mode == Mode::Playing
            && ((alive && !self.player_alive) || self.game.phase != Phase::Playing)
        {
            self.garage.close();
        }
        self.player_alive = alive;
        if self.mode == Mode::Menu && !self.garage.is_open() {
            self.menu_orbit += MENU_ORBIT * dt;
        }
        let queued = self.queued();
        let scores = self.held(&[KeyCode::Tab]);
        if let (Some(r), Some(art)) = (&mut self.renderer, &self.art) {
            let (w, h) = (r.width as f32, r.height as f32);
            let alpha = if self.mode == Mode::Paused {
                1.0
            } else {
                self.clock.alpha()
            };
            let mut tanks = self.game.display_tanks(alpha);
            let mut camera = match (self.mode, self.camera) {
                (Mode::Menu, _) => {
                    garage::orbit_camera(&self.game.world, &tanks[0], self.menu_orbit, w / h)
                }
                // Frozen: hold the last frame's view.
                (Mode::Paused, Some(c)) => c,
                _ => self.cam.update(&self.game.world, &tanks[0], dt),
            };
            if self.mode != Mode::Paused {
                self.garage.update(dt);
                camera = self
                    .garage
                    .camera(camera, &self.game.world, &tanks[0], w / h);
            }
            let driven = tanks[0].clone();
            self.garage.preview(&mut tanks[0]);
            if let Some(room) = self.showroom {
                // Slow auto-orbit; PageUp/PageDown zoom.
                let angle = room.orbit.unwrap_or(self.game.world.time * 0.15);
                camera = showroom::camera(
                    &self.game.world,
                    angle,
                    room.dist * (0.6 + self.cam.tilt),
                    room.focus,
                );
            }
            let mut scene = scene::build(&self.game.world, &tanks, art, camera);
            self.game.effect_instances(alpha, &mut scene.instances);
            // Ambient motion happens around wherever the camera is.
            self.game.ambience.focus = camera.pos;
            scene.particles = self.game.ambience.particles();
            if self.showroom.is_some() {
                scene.shadow_center = glam::Vec3::ZERO;
                scene.shadow_radius = 22.0;
            } else {
                scene.ui = match self.mode {
                    Mode::Menu => {
                        let mut ui = self
                            .menu
                            .draw(&self.profile, h, 1.0 - self.garage.presence());
                        ui.extend(self.garage.draw(&driven, queued, &self.profile, h));
                        ui
                    }
                    Mode::Playing => {
                        let hud = self
                            .hud
                            .draw(&self.game, &tanks, self.seed, &camera, w, h, dt, scores);
                        self.garage.overlay(hud, &driven, queued, &self.profile, h)
                    }
                    Mode::Paused => {
                        let mut ui = menu::dim(
                            self.hud
                                .draw(&self.game, &tanks, self.seed, &camera, w, h, 0.0, false),
                        );
                        ui.extend(self.pause.draw(
                            &self.profile,
                            self.game.factions[0],
                            self.game.trust,
                            w,
                            h,
                        ));
                        ui
                    }
                };
            }
            if self.profile.settings.debug {
                scene.ui.extend(hud::fps_counter(self.fps, w, h));
            }
            r.render(&scene);
            self.camera = Some(camera);
            let sounds = std::mem::take(&mut self.game.sounds);
            self.audio
                .update(&self.game, &sounds, &camera, self.mode != Mode::Paused, dt);
        }

        self.fps_timer += dt;
        self.fps_frames += 1;
        if self.fps_timer >= 0.5 {
            if let Some(w) = &self.window {
                let fps = self.fps_frames as f32 / self.fps_timer;
                self.fps = fps;
                let t = &self.game.world.tanks[0];
                w.set_title(&format!(
                    "tankit  |  {fps:.0} fps  |  {:>4.1} m/s  {}  |  side grip x{:.2} ([ ])  turn {:.1} rad/s (; ')  Backspace reset  |  WASD, Z/X/C turret, Space fire, Tab scores, G garage, Esc pause",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::faction::Faction;

    /// Hired by side 1, the player fights for that faction from its bases.
    #[test]
    fn side_one_puts_the_player_with_the_other_faction() {
        let m = &MISSIONS[0];
        let kit = (
            crate::sim::HullKind::Kataphrakt,
            crate::sim::TurretKind::Kontos,
        );
        let a = new_match(m.seed, m.map_kind, 0, 0, 3, m.contract, kit);
        let b = new_match(m.seed, m.map_kind, 1, 0, 3, m.contract, kit);
        assert_eq!(a.factions, m.sides);
        assert_eq!(b.factions, [m.sides[1], m.sides[0]]);
        assert_eq!(b.world.map.bases[0], a.world.map.bases[1]);
        assert_eq!(b.world.tanks[0].color, Faction::Resonant.paint());
    }
}
