//! Windowed app: winit event loop, input mapping, fixed-step simulation, rendering.
//!
//! The loop: main menu (missions, garage, quit) → a mission's match → Esc
//! pauses (resume, exit to menu, quit) → when the round ends, Enter plays it
//! again and Esc goes back to the menu. Reputation is banked into the pilot profile
//! the moment it's earned, and the profile saves itself on every change.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::audio::{self, Audio};
use crate::bay::{self, BayArt};
use crate::combat::{Match, Phase};
use crate::economy::{Part, Payout};
use crate::game::Clock;
use crate::garage::{self, Garage};
use crate::hud::{self, Hud, Ledger};
use crate::keys;
use crate::menu::{self, Change, Menu, Nav, Pause, PauseAction};
use crate::mission::{Contract, MISSIONS};
use crate::profile::Profile;
use crate::render::mesh::MeshData;
use crate::render::{Camera, Renderer};
use crate::scene::{self, ChaseCam};
use crate::sim::Command;
use crate::tank::TankArt;
use crate::ui::Hits;
use crate::{MapKind, ShowroomArgs, make_map, showroom};
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// Camera height change per second while PageUp/PageDown is held (0..1 range).
const CAM_TILT_SPEED: f32 = 0.8;
/// Main menu camera drift around the parked tank, rad/s.
const MENU_ORBIT: f32 = 0.06;
/// Seconds a contract stays selected before the menu backdrop moves to its map.
const BACKDROP_SETTLE: f32 = 0.25;
/// Seconds the menu scene takes to dip to black before a backdrop swap, and
/// to come back up after it.
const DIP_OUT: f32 = 0.18;
const DIP_IN: f32 = 0.4;

/// Something clickable on screen, by the component that drew it.
#[derive(Clone, Copy, PartialEq, Debug)]
enum UiHit {
    Menu(menu::Hit),
    Pause(menu::Hit),
    Garage(garage::Hit),
    Hud(hud::Hit),
}

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
    /// Which of the map's sides hires you.
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
    /// The mission's rating, in upgrade steps (see `Match::set_rating`).
    rating: u8,
    /// Which of the map's sides hired you.
    side: usize,
    mode: Mode,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    art: Option<TankArt>,
    bay: Option<BayArt>,
    showroom: Option<ShowroomArgs>,
    /// The team deathmatch, or the menu's backdrop (its world drives the showroom too, with no bots).
    game: Match,
    profile: Profile,
    /// How much of `game.reputation` is already banked into the profile.
    banked: u32,
    /// Whether this round's contract pay has been banked.
    paid: bool,
    menu: Menu,
    pause: Pause,
    /// Main menu camera angle around the parked tank.
    menu_orbit: f32,
    /// The mission (and side) whose map the menu backdrop shows.
    backdrop: (usize, usize),
    /// How long the mission screen has pointed at a different contract than
    /// the backdrop shows: it follows once the selection settles.
    backdrop_wait: f32,
    /// A backdrop being built off the main thread (a big map takes most of a
    /// second), for the mission and side it's of.
    backdrop_job: Option<((usize, usize), Receiver<(Match, MeshData)>)>,
    /// A built backdrop waiting for the screen to go dark before it's swapped in.
    backdrop_ready: Option<((usize, usize), Match, MeshData)>,
    /// How dark the menu scene is (0..1): it dips to black around a backdrop swap.
    backdrop_dark: f32,
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
    /// What each key held down pressed, so its release lets go of the same thing.
    held: HashMap<PhysicalKey, KeyCode>,
    /// Mouse pointer in window pixels, while it's over the window.
    pointer: Option<glam::Vec2>,
    /// What can be clicked, as of the last frame drawn.
    hits: Hits<UiHit>,
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
        let mut m = Match::new(make_map(launch.map_kind), launch.spawn, 0, launch.seed);
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
        rating: 0,
        side: launch.side,
        mode: if launch.menu && launch.showroom.is_none() {
            Mode::Menu
        } else {
            Mode::Playing
        },
        window: None,
        renderer: None,
        art: None,
        bay: None,
        showroom: launch.showroom,
        game,
        profile,
        banked: 0,
        paid: false,
        menu: Menu::new(),
        pause: Pause::new(),
        menu_orbit,
        backdrop: (0, 0),
        backdrop_wait: 0.0,
        backdrop_job: None,
        backdrop_ready: None,
        backdrop_dark: 0.0,
        cam: ChaseCam::new(),
        hud: Hud::new(),
        garage: Garage::new(),
        audio: Audio::new(),
        camera: None,
        player_alive: true,
        clock: Clock::default(),
        keys: HashSet::new(),
        held: HashMap::new(),
        pointer: None,
        hits: Hits::new(),
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
    let mut map = make_map(map_kind);
    if side > 0 {
        map.swap_sides(side);
    }
    let mut m = Match::new(map, spawn, bots, seed.wrapping_add(spawn as u64));
    m.set_contract(contract);
    m.set_player_hull(kit.0);
    m.world.tanks[0].turret_kind = kit.1;
    m
}

/// Black over the whole menu scene at `dark` (0..1), eased.
fn backdrop_dip(dark: f32, w: f32, h: f32) -> Vec<crate::render::UiVertex> {
    if dark <= 0.0 {
        return Vec::new();
    }
    let k = dark * dark * (3.0 - 2.0 * dark);
    let mut p = crate::ui::Painter::new(1.0);
    p.rect(glam::Vec2::ZERO, glam::Vec2::new(w, h), [0.0, 0.0, 0.0, k]);
    p.finish()
}

/// The main menu's backdrop: the first mission's map, empty, your tank parked in it.
pub fn menu_match(kit: garage::Loadout) -> Match {
    backdrop_match(0, 0, kit)
}

/// Mission `MISSIONS[i]`'s map, empty, your tank parked at side `side`'s base:
/// the menu's backdrop while that contract is selected.
pub fn backdrop_match(i: usize, side: usize, kit: garage::Loadout) -> Match {
    let mission = &MISSIONS[i];
    let mut m = new_match(
        mission.seed,
        mission.map_kind,
        side,
        0,
        0,
        Contract::Attrition,
        kit,
    );
    m.time_left = f32::INFINITY;
    // Already parked: no arrival (or its sound) each time the backdrop
    // changes, and no shield to drop with a cue.
    let me = &mut m.fighters[0];
    me.arrived = crate::arrival::ARRIVAL_TIME;
    me.shield = 0.0;
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
                self.bay = Some(BayArt::upload(&mut r));
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
                // Release what this key pressed, even if what it means changed meanwhile.
                let code = match event.state {
                    ElementState::Pressed => keys::code(event.physical_key, &event.logical_key),
                    ElementState::Released => self.held.remove(&event.physical_key),
                };
                let Some(code) = code else {
                    return;
                };
                match event.state {
                    ElementState::Pressed => {
                        if !event.repeat {
                            self.on_key(code, event_loop);
                        }
                        self.held.insert(event.physical_key, code);
                        self.keys.insert(code);
                    }
                    ElementState::Released => {
                        self.keys.remove(&code);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let at = glam::Vec2::new(position.x as f32, position.y as f32);
                self.pointer = Some(at);
                self.on_hover(at);
            }
            WindowEvent::CursorLeft { .. } => self.pointer = None,
            WindowEvent::MouseWheel { delta, .. } if self.garage.is_open() => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                self.garage.scroll(-lines * 60.0);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } => self.on_click(button, event_loop),
            WindowEvent::Focused(false) => {
                self.keys.clear();
                self.held.clear();
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

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

impl App {
    /// Swap in a new match (or menu backdrop) and reset everything tied to the old one.
    fn load(&mut self, game: Match) {
        let mesh = scene::map_mesh(&game.world.map);
        self.load_built(game, &mesh);
    }

    /// `load`, with the map's mesh already built.
    fn load_built(&mut self, game: Match, mesh: &MeshData) {
        self.game = game;
        self.game.set_player_levels(self.profile.levels);
        self.banked = 0;
        self.paid = false;
        self.cam = ChaseCam::new();
        self.hud = Hud::new();
        self.garage = Garage::new();
        self.player_alive = true;
        if let Some(r) = &mut self.renderer {
            r.set_static_mesh(mesh);
        }
    }

    /// Start a new round on the current map settings, keeping the player's queued kit.
    fn restart(&mut self) {
        let kit = self.queued();
        let mut m = new_match(
            self.seed,
            self.map_kind,
            self.side,
            self.spawn,
            self.bots,
            self.contract,
            kit,
        );
        m.set_rating(self.rating);
        self.load(m);
        self.mode = Mode::Playing;
    }

    /// Start mission `MISSIONS[i]`, hired by its side `side`.
    fn start_mission(&mut self, i: usize, side: usize) {
        let m = &MISSIONS[i];
        (self.seed, self.map_kind, self.spawn, self.bots) = (m.seed, m.map_kind, 0, m.bots());
        (self.contract, self.side, self.rating) = (m.contract, side, m.rating);
        let mut game = new_match(
            self.seed,
            self.map_kind,
            self.side,
            self.spawn,
            self.bots,
            self.contract,
            self.profile.loadout,
        );
        game.set_rating(self.rating);
        self.load(game);
        self.mode = Mode::Playing;
        log::info!(
            "deployed: {} ({} {}, {}) for the {}",
            m.name,
            m.rating_label(),
            m.contract.name(),
            m.map,
            m.sides[side].name()
        );
    }

    /// Puts the menu backdrop on the selected contract's map once the
    /// selection has held still a moment (scrolling past doesn't load each
    /// map). The map is built on a worker thread so the menu never stalls,
    /// while the scene dips to black; it's swapped in in the dark and the
    /// scene comes back up on it.
    fn follow_backdrop(&mut self, dt: f32) {
        if let Some((key, job)) = &self.backdrop_job {
            match job.try_recv() {
                Ok((game, mesh)) => {
                    self.backdrop_ready = Some((*key, game, mesh));
                    self.backdrop_job = None;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.backdrop_job = None,
            }
        }
        if self.backdrop_job.is_some() || self.backdrop_ready.is_some() {
            self.backdrop_dark = (self.backdrop_dark + dt / DIP_OUT).min(1.0);
            if self.backdrop_dark >= 1.0
                && let Some((key, game, mesh)) = self.backdrop_ready.take()
            {
                self.load_built(game, &mesh);
                self.backdrop = key;
                self.menu_orbit = garage::front_angle(&self.game.world.tanks[0]);
            }
            return;
        }
        self.backdrop_dark = (self.backdrop_dark - dt / DIP_IN).max(0.0);
        match self.menu.backdrop() {
            Some(want) if want != self.backdrop => {
                self.backdrop_wait += dt;
                if self.backdrop_wait >= BACKDROP_SETTLE {
                    self.backdrop_wait = 0.0;
                    let kit = self.profile.loadout;
                    let (tx, rx) = mpsc::channel();
                    std::thread::spawn(move || {
                        let started = Instant::now();
                        let game = backdrop_match(want.0, want.1, kit);
                        let mesh = scene::map_mesh(&game.world.map);
                        log::debug!(
                            "menu backdrop: {} built in {:.0?}",
                            MISSIONS[want.0].map,
                            started.elapsed()
                        );
                        // The menu may have moved on: then nobody's listening.
                        let _ = tx.send((game, mesh));
                    });
                    self.backdrop_job = Some((want, rx));
                }
            }
            _ => self.backdrop_wait = 0.0,
        }
    }

    fn back_to_menu(&mut self) {
        self.load(menu_match(self.profile.loadout));
        self.backdrop = (0, 0);
        self.backdrop_job = None;
        self.backdrop_ready = None;
        self.backdrop_dark = 0.0;
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

    /// A garage key or button pressed, and what it asks for: play its sound
    /// and do it. E (equip or buy) has a sound of its own for what it did;
    /// everything else clicks like a menu.
    fn garage_acted(&mut self, input: garage::Input, event: Option<garage::Event>) {
        match (input, event) {
            (garage::Input::Act, Some(event)) => self.garage_event(event, true),
            (_, event) => {
                self.garage_sound(input);
                if let Some(event) = event {
                    self.garage_event(event, false);
                }
            }
        }
    }

    /// Do what the garage asked. `voiced`: play the depot bay's sound for
    /// it (a swap or a purchase), not just the menu click already played.
    fn garage_event(&mut self, event: garage::Event, voiced: bool) {
        match event {
            garage::Event::Queue(kit) => {
                if voiced {
                    let swap = if kit.0 != self.queued().0 {
                        audio::GarageCue::Hull
                    } else {
                        audio::GarageCue::Turret
                    };
                    self.audio.garage(swap);
                }
                self.queue(kit)
            }
            // Refusals need no handling beyond the buzz: the garage already shows why E won't work.
            garage::Event::Buy(part) => {
                let bought = self.profile.buy(part).is_ok();
                if voiced {
                    self.audio.garage(if bought {
                        audio::GarageCue::Bought
                    } else {
                        audio::GarageCue::Refused
                    });
                }
                if bought {
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
                        "upgraded {} to {}",
                        part.name(),
                        crate::economy::level_label(self.profile.levels.of(part))
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
                KeyCode::KeyU => garage::Input::Upgrade,
                KeyCode::Tab | KeyCode::KeyQ => garage::Input::Switch,
                _ => return,
            };
            let event = self.garage.input(input, self.queued(), &self.profile);
            self.garage_acted(input, event);
            return;
        }
        match self.mode {
            Mode::Menu => {
                let Some(nav) = Self::nav(code) else { return };
                self.nav_sound(nav);
                let action = self.menu.input(nav);
                self.menu_action(action, event_loop);
                return;
            }
            Mode::Paused => {
                let Some(nav) = Self::nav(code) else { return };
                self.nav_sound(nav);
                let action = self.pause.input(nav);
                self.pause_action(action, event_loop);
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
            _ => {}
        }
    }

    fn garage_sound(&mut self, input: garage::Input) {
        self.audio.ui(match input {
            garage::Input::Deploy | garage::Input::Act | garage::Input::Upgrade => {
                audio::Ui::Select
            }
            garage::Input::Revert => audio::Ui::Back,
            _ => audio::Ui::Move,
        });
    }

    fn menu_action(&mut self, action: Option<menu::Action>, event_loop: &ActiveEventLoop) {
        match action {
            Some(menu::Action::Deploy(i, side)) => {
                let m = &MISSIONS[i];
                if m.open_at(self.profile.standing(m.sides[side]).level) {
                    self.start_mission(i, side);
                } else {
                    // Not trusted with this one yet.
                    self.audio.garage(audio::GarageCue::Refused);
                }
            }
            Some(menu::Action::Garage) => {
                if let Some(cam) = self.camera {
                    self.garage.toggle(&self.game.world, &cam, self.queued());
                }
            }
            Some(menu::Action::Setting(c)) => self.change_setting(c),
            Some(menu::Action::Quit) => event_loop.exit(),
            None => {}
        }
    }

    fn pause_action(&mut self, action: Option<PauseAction>, event_loop: &ActiveEventLoop) {
        match action {
            Some(PauseAction::Resume) => self.mode = Mode::Playing,
            Some(PauseAction::Setting(c)) => self.change_setting(c),
            Some(PauseAction::ExitToMenu) => self.back_to_menu(),
            Some(PauseAction::Quit) => event_loop.exit(),
            None => {}
        }
    }

    /// The pointer moved: whatever it's over takes the selection, as W/S would.
    fn on_hover(&mut self, at: glam::Vec2) {
        let hit = self.hits.at(at).map(|(h, _)| h);
        self.garage.hover(match hit {
            Some(UiHit::Garage(h)) => Some(h),
            _ => None,
        });
        let hud = match hit {
            Some(UiHit::Hud(h)) => Some(h),
            _ => None,
        };
        let lit = hud.is_some() && hud != self.hud.hover;
        self.hud.hover = hud;
        let Some(hit) = hit else {
            return;
        };
        let moved = match hit {
            UiHit::Menu(h) => self.menu.hover(h),
            UiHit::Pause(h) => self.pause.hover(h),
            // Garage hover only lights the row: a click previews it.
            UiHit::Garage(_) => false,
            UiHit::Hud(_) => lit,
        };
        if moved {
            self.audio.ui(audio::Ui::Move);
        }
    }

    /// Left click: whatever's under the pointer, as if its key were pressed.
    /// Right click: back, like Esc (but never pauses the battle).
    fn on_click(&mut self, button: MouseButton, event_loop: &ActiveEventLoop) {
        match button {
            MouseButton::Left => {}
            MouseButton::Right | MouseButton::Back => {
                if self.garage.is_open() || self.mode != Mode::Playing {
                    self.on_key(KeyCode::Escape, event_loop);
                    self.hits.clear();
                }
                return;
            }
            _ => return,
        }
        let Some((hit, at)) = self.pointer.and_then(|p| self.hits.at(p)) else {
            return;
        };
        // The click may change the screen; nothing is clickable until it's drawn again.
        self.hits.clear();
        match hit {
            UiHit::Menu(h) => {
                let (nav, action) = self.menu.click(h, at, &self.profile.settings);
                self.nav_sound(nav);
                self.menu_action(action, event_loop);
            }
            UiHit::Pause(h) => {
                let (nav, action) = self.pause.click(h, at, &self.profile.settings);
                self.nav_sound(nav);
                self.pause_action(action, event_loop);
            }
            UiHit::Hud(h) => self.on_key(
                match h {
                    hud::Hit::Again => KeyCode::Enter,
                    hud::Hit::Menu => KeyCode::Escape,
                },
                event_loop,
            ),
            UiHit::Garage(h) => {
                let input = match h {
                    garage::Hit::Key(input) => input,
                    garage::Hit::Part(_) | garage::Hit::Tab(_) => garage::Input::Down,
                };
                let event = self.garage.click(h, self.queued(), &self.profile);
                self.garage_acted(input, event);
            }
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

        // Reputation is paid the moment it's earned, to whoever hired you; crystals
        // when the contract is done.
        if self.game.reputation > self.banked {
            self.profile
                .add_reputation(self.game.factions[0], self.game.reputation - self.banked);
            self.banked = self.game.reputation;
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
            self.follow_backdrop(dt);
        }
        let queued = self.queued();
        let scores = self.held(&[KeyCode::Tab]);
        if let (Some(r), Some(art), Some(bay)) = (&mut self.renderer, &self.art, &self.bay) {
            let (w, h) = (r.width as f32, r.height as f32);
            let alpha = if self.mode == Mode::Paused {
                1.0
            } else {
                self.clock.alpha()
            };
            let mut tanks = self.game.display_tanks(alpha);
            // The camera follows the tank where it really is, not where an
            // arrival draws it.
            let anchor = self.game.camera_tank(0, alpha);
            let mut camera = match (self.mode, self.camera) {
                (Mode::Menu, _) => {
                    garage::orbit_camera(&self.game.world, &anchor, self.menu_orbit, w / h)
                }
                // Frozen: hold the last frame's view.
                (Mode::Paused, Some(c)) => c,
                _ => self.cam.update(&self.game.world, &anchor, dt),
            };
            if self.mode != Mode::Paused {
                self.garage.update(dt);
                camera = self.garage.camera(camera, &self.game.world, &anchor, w / h);
            }
            let driven = tanks[0].clone();
            // A couple of card thumbnails a frame, so opening doesn't hitch.
            self.garage
                .render_thumbs(r, art, &self.game.world, &driven, queued, h, 2);
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
            let mut scene = if self.garage.in_bay() {
                let view = self.garage.bay_camera(w / h);
                bay::scene(
                    art,
                    bay,
                    &tanks[0],
                    self.garage.spin(),
                    self.game.world.time,
                    view,
                )
            } else {
                let mut scene = scene::build(&self.game.world, &tanks, art, camera);
                self.game.effect_instances(alpha, &mut scene.instances);
                // Ambient motion happens around wherever the camera is.
                self.game.ambience.focus = camera.pos;
                scene.particles.extend(self.game.ambience.particles());
                self.game.glow_particles(alpha, &mut scene.particles);
                scene
            };
            if self.showroom.is_some() {
                scene.shadow_center = glam::Vec3::ZERO;
                scene.shadow_radius = 22.0;
            } else {
                let (ui, hits) = match self.mode {
                    Mode::Menu => {
                        let menu = self
                            .menu
                            .draw(&self.profile, h, 1.0 - self.garage.presence());
                        let garage = self.garage.draw(&driven, queued, &self.profile, w, h);
                        // The dip around a backdrop swap, under the panel.
                        let mut ui = backdrop_dip(self.backdrop_dark, w, h);
                        ui.extend(menu.verts);
                        ui.extend(garage.verts);
                        // The garage covers the menu panel while it's open.
                        let mut hits = if self.garage.is_open() {
                            Hits::new()
                        } else {
                            menu.hits.map(UiHit::Menu)
                        };
                        hits.extend(garage.hits.map(UiHit::Garage));
                        (ui, hits)
                    }
                    Mode::Playing => {
                        let ledger =
                            Ledger::before(&self.profile, &self.game, self.banked, self.paid);
                        let hud = self
                            .hud
                            .draw(&self.game, &tanks, &camera, w, h, dt, scores, ledger);
                        let layer =
                            self.garage
                                .overlay(hud.verts, &driven, queued, &self.profile, w, h);
                        // The garage covers the HUD while it's open.
                        let mut hits = if self.garage.is_open() {
                            Hits::new()
                        } else {
                            hud.hits.map(UiHit::Hud)
                        };
                        hits.extend(layer.hits.map(UiHit::Garage));
                        (layer.verts, hits)
                    }
                    Mode::Paused => {
                        let mut ui = menu::dim(
                            self.hud
                                .draw(
                                    &self.game,
                                    &tanks,
                                    &camera,
                                    w,
                                    h,
                                    0.0,
                                    false,
                                    Ledger::before(
                                        &self.profile,
                                        &self.game,
                                        self.banked,
                                        self.paid,
                                    ),
                                )
                                .verts,
                        );
                        let pause = self.pause.draw(
                            &self.profile,
                            self.game.factions[0],
                            self.game.reputation,
                            w,
                            h,
                        );
                        ui.extend(pause.verts);
                        (ui, pause.hits.map(UiHit::Pause))
                    }
                };
                scene.ui = ui;
                self.hits = hits;
            }
            if self.profile.settings.debug {
                scene.ui.extend(hud::fps_counter(self.fps, w, h));
            }
            r.render(&scene);
            self.camera = Some(camera);
            self.audio.set_turntable(self.garage.in_bay());
            let sounds = std::mem::take(&mut self.game.sounds);
            self.audio
                .update(&self.game, &sounds, &camera, self.mode != Mode::Paused, dt);
        }

        self.fps_timer += dt;
        self.fps_frames += 1;
        if self.fps_timer >= 0.5 {
            self.fps = self.fps_frames as f32 / self.fps_timer;
            self.fps_timer = 0.0;
            self.fps_frames = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::faction::Faction;

    /// Moving the menu between contracts swaps the backdrop's map: the tank is
    /// already parked there, so nothing arrives, rings or drops a shield.
    #[test]
    fn a_menu_backdrop_is_silent() {
        let kit = (
            crate::sim::HullKind::Kataphrakt,
            crate::sim::TurretKind::Kontos,
        );
        let mut m = backdrop_match(0, 1, kit);
        assert_eq!(m.factions[0], MISSIONS[0].sides[1]);
        for _ in 0..(4.0 / crate::game::DT) as usize {
            m.step(crate::sim::Command::default(), false);
        }
        let loud: Vec<_> = m
            .sounds
            .iter()
            .filter(|s| !matches!(s, crate::combat::Sound::Knock(_)))
            .collect();
        assert!(loud.is_empty(), "{loud:?}");
    }

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
        assert_eq!(b.factions[0], Faction::Resonant);
    }
}
