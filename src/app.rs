//! Windowed app: winit event loop, input mapping, fixed-step simulation, rendering.
//!
//! The loop: main menu (missions, garage, quit) → a mission's match → Esc
//! pauses (resume, garage, settings, exit to menu, quit) → when the round ends, Enter plays it
//! again and Esc goes back to the menu. Renown is banked into the pilot profile
//! the moment it's earned, a won mission's schematic when the round ends, and
//! the profile saves itself on every change.

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
use crate::binds::{Action, Binds};
use crate::combat::{Match, Phase};
use crate::economy::{Part, Payout};
use crate::faction::Faction;
use crate::galaxy::battle::{self, Battle};
use crate::galaxy::save::War;
use crate::galaxy::theater::{Offer, Shown, Sortie, TickSummary};
use crate::game::Clock;
use crate::garage::{self, Garage};
use crate::hitch::{self, Laps};
use crate::hud::{self, Hud, Ledger};
use crate::keys;
use crate::menu::{self, Change, Menu, Nav, Pause, PauseAction};
use crate::mission::{Contract, MISSIONS};
use crate::profile::Profile;
use crate::render::StaticWorld;
use crate::render::{Camera, Renderer};
use crate::scene::{self, ChaseCam};
use crate::sim::Command;
use crate::tank::TankArt;
use crate::theater_view::{self, SystemArt};
use crate::ui::Hits;
use crate::{MapKind, ShowroomArgs, make_map, showroom};
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// Camera height change per second while PageUp/PageDown is held (0..1 range).
const CAM_TILT_SPEED: f32 = 0.8;
/// How fast (tilt/s) a camera reset eases the tilt back home.
const CAM_RESET_SPEED: f32 = 2.5;
/// A frame whose own work takes longer than this is logged with what it
/// spent the time on, so a hitch mid-match can be traced to its cause.
const HITCH_WORK: f32 = 0.05;
/// Likewise a gap this long between frames that the frame's work doesn't
/// explain (the window, the driver or the OS held the loop).
const HITCH_GAP: f32 = 0.1;
/// Of each frame, how long the match may fast-forward without you once
/// you're out for good (#373), well under a hitch.
const FAST_FORWARD_BUDGET: std::time::Duration = std::time::Duration::from_millis(12);
/// Steps fast-forwarded between looks at the clock.
const FAST_FORWARD_CHUNK: usize = 30;
/// Once you've left a battle (#427) there's nothing else to draw but its
/// progress, so it may take most of each frame.
const LEAVING_BUDGET: std::time::Duration = std::time::Duration::from_millis(30);
/// Seconds a contract stays selected before the menu backdrop moves to its map.
const BACKDROP_SETTLE: f32 = 0.25;
/// Copy what the profile knows onto your pilot, and its company onto an
/// older galaxy save that has none. Back pay that's reached you goes into
/// the profile.
fn adopt(g: &mut crate::galaxy::Galaxy, profile: &mut crate::profile::Profile) {
    g.know(profile.known);
    profile.add_crystals(g.take_wallet());
    if g.crew.is_none() {
        g.crew = profile.ship;
    }
}

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

/// Where you go once a battle you've left has played out (#427).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Leaving {
    Menu,
    Quit,
}

/// Going back to the menu after a round (#452): the menu's map is built
/// on a worker thread, so the screen keeps drawing (the tick summary, or a
/// loading panel) instead of freezing.
struct Returning {
    /// To the contract screen (else the menu's first screen).
    missions: bool,
    /// The menu's map, being built.
    job: Option<Receiver<(Match, StaticWorld)>>,
    /// The menu's map, built.
    built: Option<(Match, StaticWorld)>,
    /// Seconds it's been going (turns the spinner).
    time: f32,
    /// Back from a battle in the war (#475): the tick summary's up while
    /// the contract screen loads, until you go on.
    summary: Option<Summary>,
}

/// Going into a battle: its map's built on a worker thread, under a
/// loading panel over the contract screen.
struct Deploying {
    sortie: Sortie,
    job: Receiver<(Match, StaticWorld)>,
    /// Seconds it's been going (turns the spinner).
    time: f32,
    /// The system it's in, on the panel.
    title: String,
}

/// The tick summary screen on the way back to the contract screen (#475):
/// what your battle's tick did to its system, and no more.
struct Summary {
    view: TickSummary,
    /// CONTINUE's pressed: on to the contract screen once it's loaded.
    going: bool,
}

/// How much of each frame the war may go on in while you wait for work on
/// the contract screen: the rest draws it.
const WAIT_BUDGET: std::time::Duration = std::time::Duration::from_millis(12);

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
    /// Which map a generated kind builds.
    pub map_seed: u64,
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
    /// Which map a generated kind builds: the sortie site's (kept for a replay).
    map_seed: u64,
    map_kind: MapKind,
    spawn: usize,
    bots: usize,
    contract: Contract,
    /// Which of the map's sides hired you.
    side: usize,
    /// The galaxy's war, which a mission's battles are drawn from and fed
    /// back into.
    war: War,
    /// This round's battle in it (missions only), until it's settled.
    battle: Option<Battle>,
    /// Rounds are missions: drawn from the galaxy.
    in_war: bool,
    /// What your last battle's tick did to its system, until the tick
    /// summary shows it (#447, #475).
    tick_summary: Option<TickSummary>,
    /// The theater site this round is fought at (a replay fights it again).
    sortie: Option<Sortie>,
    mode: Mode,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    art: Option<TankArt>,
    bay: Option<BayArt>,
    /// The missions screen's system view.
    system: Option<SystemArt>,
    showroom: Option<ShowroomArgs>,
    /// The team deathmatch, or the menu's backdrop (its world drives the showroom too, with no bots).
    game: Match,
    profile: Profile,
    /// How much of `game.renown` is already banked into the profile.
    banked: u32,
    /// Whether this round's contract pay has been banked.
    paid: bool,
    /// You've left the battle (#427): it's playing out without you, and
    /// then you go here.
    leaving: Option<Leaving>,
    /// On the way back to the menu after a round (#452).
    returning: Option<Returning>,
    /// On the way into a battle.
    deploying: Option<Deploying>,
    /// A battle you quit in has played out: the game can close.
    quit: bool,
    menu: Menu,
    pause: Pause,
    /// The menu camera's angle around the parked tank, which it sways about.
    menu_orbit: f32,
    /// Seconds the menu has been up (drives the camera's sway).
    menu_time: f32,
    /// The map (and side) the menu backdrop shows.
    backdrop: Backdrop,
    /// How long the mission screen has pointed at a different contract than
    /// the backdrop shows: it follows once the selection settles.
    backdrop_wait: f32,
    /// A backdrop being built off the main thread (a big map takes most of a
    /// second), for the mission and side it's of.
    backdrop_job: Option<(Backdrop, Receiver<(Match, StaticWorld)>)>,
    /// A built backdrop waiting for the screen to go dark before it's swapped in.
    backdrop_ready: Option<(Backdrop, Match, StaticWorld)>,
    /// How dark the menu scene is (0..1): it dips to black around a backdrop swap.
    backdrop_dark: f32,
    cam: ChaseCam,
    /// Easing the camera back to its usual tilt (a camera reset), until it's
    /// there or a tilt key takes over.
    cam_homing: bool,
    hud: Hud,
    garage: Garage,
    audio: Audio,
    /// Last frame's camera (the garage starts its orbit from it).
    camera: Option<Camera>,
    /// Last frame's chase camera, before the garage swung it round: what a
    /// paused match holds still under the garage's orbit.
    chase: Option<Camera>,
    /// Whether the player was alive last frame (the garage closes on respawn).
    player_alive: bool,
    clock: Clock,
    keys: HashSet<KeyCode>,
    /// What each key held down pressed, so its release lets go of the same thing.
    held: HashMap<PhysicalKey, KeyCode>,
    /// Mouse pointer in window pixels, while it's over the window.
    pointer: Option<glam::Vec2>,
    /// Wheel turned on the menu and not yet scrolled, in notches.
    wheel: f32,
    /// What can be clicked, as of the last frame drawn.
    hits: Hits<UiHit>,
    last: Instant,
    /// How long the last frame's own work took (for telling a gap from a slow frame).
    last_work: f32,
    /// A map was just loaded: the next frame's gap is the load, not a hitch.
    loaded: bool,
    fps_timer: f32,
    fps_frames: u32,
    /// Frames per second over the last half second (the debug overlay shows it).
    fps: f32,
}

pub fn run(launch: Launch) -> anyhow::Result<()> {
    hitch::start();
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
            make_map(launch.map_kind, launch.map_seed),
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
            launch.map_seed,
            launch.side,
            launch.spawn,
            launch.bots,
            launch.contract,
            loadout,
            None,
        )
    };
    if launch.showroom.is_none() {
        game.set_player_levels(profile.levels);
        game.start();
    }
    let menu_orbit = garage::front_angle(&game.world.tanks[0]);
    let volume = profile.settings.volume();
    let mut app = App {
        seed: launch.seed,
        map_seed: launch.map_seed,
        map_kind: launch.map_kind,
        spawn: launch.spawn,
        bots: launch.bots,
        contract: launch.contract,
        side: launch.side,
        war: War::load(),
        battle: None,
        in_war: false,
        tick_summary: None,
        sortie: None,
        mode: if launch.menu && launch.showroom.is_none() {
            Mode::Menu
        } else {
            Mode::Playing
        },
        window: None,
        renderer: None,
        art: None,
        bay: None,
        system: None,
        showroom: launch.showroom,
        game,
        profile,
        banked: 0,
        paid: false,
        leaving: None,
        returning: None,
        deploying: None,
        quit: false,
        menu: Menu::new(),
        pause: Pause::new(),
        menu_orbit,
        menu_time: 0.0,
        backdrop: (MISSIONS[0].map_kind, 0, 0),
        backdrop_wait: 0.0,
        backdrop_job: None,
        backdrop_ready: None,
        backdrop_dark: 0.0,
        cam: ChaseCam::new(),
        cam_homing: false,
        hud: Hud::new(),
        garage: Garage::new(),
        audio: Audio::new(),
        camera: None,
        chase: None,
        player_alive: true,
        clock: Clock::default(),
        keys: HashSet::new(),
        held: HashMap::new(),
        pointer: None,
        wheel: 0.0,
        hits: Hits::new(),
        last: Instant::now(),
        last_work: 0.0,
        loaded: true,
        fps_timer: 0.0,
        fps_frames: 0,
        fps: 0.0,
    };
    app.audio.set_volume(volume);
    event_loop.run_app(&mut app)?;
    Ok(())
}

/// A round under `contract`, hired by the map's side `side`, with the player in `kit`.
#[allow(clippy::too_many_arguments)]
pub fn new_match(
    seed: u64,
    map_kind: MapKind,
    map_seed: u64,
    side: usize,
    spawn: usize,
    bots: usize,
    contract: Contract,
    kit: garage::Loadout,
    factions: Option<&[Faction]>,
) -> Match {
    let mut map = make_map(map_kind, map_seed);
    // A theater's site, fought on a borrowed map: the theater's sides, not the map's.
    if let Some(f) = factions
        && f.len() == map.factions.len()
    {
        map.factions = f.to_vec();
    }
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

/// A menu backdrop: a map (its kind, and which one for a generated kind),
/// and the side whose vista it's seen from.
type Backdrop = (MapKind, u64, usize);

/// The main menu's backdrop: the first mission's map, empty, your tank parked at its vista.
pub fn menu_match(kit: garage::Loadout) -> Match {
    backdrop_match(MISSIONS[0].map_kind, 0, 0, kit)
}

/// Map `kind` (the one `map_seed` names, for a generated kind), empty, your tank
/// parked at side `side`'s vista: the menu's backdrop while a site fought
/// on it is selected.
pub fn backdrop_match(kind: MapKind, map_seed: u64, side: usize, kit: garage::Loadout) -> Match {
    let mut m = new_match(
        0,
        kind,
        map_seed,
        side,
        0,
        0,
        Contract::Attrition,
        kit,
        None,
    );
    m.time_left = f32::INFINITY;
    m.start();
    let v = crate::vista::of(&m.world.map, kind, side);
    crate::vista::park(&mut m.world, v);
    // Already parked: no arrival (or its sound) each time the backdrop
    // changes, and no shield to drop with a cue.
    let me = &mut m.fighters[0];
    me.arrived = crate::arrival::ARRIVAL_TIME;
    me.shield = 0.0;
    // Nor the link as you're fielded.
    m.sounds.clear();
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
                self.system = Some(SystemArt::upload(&mut r));
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
            WindowEvent::CloseRequested => self.leave(Leaving::Quit, event_loop),
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
            // The board of work scrolls (#454).
            WindowEvent::MouseWheel { delta, .. } if self.mode == Mode::Menu => {
                self.wheel += match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                let notches = self.wheel.trunc();
                self.wheel -= notches;
                if notches != 0.0 && self.menu.scroll(-notches as i32) {
                    self.nav_sound(menu::Nav::Down);
                }
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.quit {
            event_loop.exit();
            return;
        }
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

impl App {
    /// Swap in a new match (or menu backdrop), its map's mesh built, and
    /// reset everything tied to the old one.
    fn load_built(&mut self, game: Match, world: StaticWorld) {
        self.game = game;
        self.game.set_player_levels(self.profile.levels);
        // All set up (a battle's hulls drawn): the round starts.
        self.game.start();
        self.banked = 0;
        self.paid = false;
        self.cam = ChaseCam::new();
        self.cam_homing = false;
        self.hud = Hud::new();
        self.garage = Garage::new();
        self.player_alive = true;
        self.loaded = true;
        if let Some(r) = &mut self.renderer {
            r.set_static_world(world);
        }
    }

    /// Draw this round's battle from the galaxy: at the sortie's site, with
    /// your company. None if it isn't there: you only fight in your
    /// company's battles (#362).
    fn draw_battle(&mut self, m: &mut Match) -> Option<Battle> {
        let o = self.sortie?;
        let rank = self.profile.rank().level;
        let g = self.war.get();
        g.claim_own(rank);
        let mut b = battle::deploy_at(g, m, o.system, o.site)?;
        m.set_own_hulls(g.own_left());
        // For the battle log (#494): the galaxy as it's drawn.
        b.setup = Some(Box::new(crate::galaxy::ledger::Setup::of(g, &b, m)));
        Some(b)
    }

    /// Deploy into a theater's site: its mission's map and contract, the
    /// theater's sides (you with your employer, as side 0).
    fn start_sortie(&mut self, o: Sortie) {
        if self.deploying.is_some() {
            return;
        }
        let m = &MISSIONS[o.mission];
        let (seed, bots, contract, kit) = (m.seed, m.bots(), m.contract, self.profile.loadout);
        // The map's built on a worker thread, under a loading panel (a big
        // one takes most of a second); the battle's drawn from the galaxy
        // once it's in (`pump_deploy`).
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let started = Instant::now();
            let game = new_match(
                seed,
                o.map_kind,
                o.map_seed,
                0,
                0,
                bots,
                contract,
                kit,
                Some(&o.factions),
            );
            let mesh = StaticWorld::new(&scene::map_mesh(&game.world.map));
            log::debug!("battle map built in {:.0?}", started.elapsed());
            let _ = tx.send((game, mesh));
        });
        let world = crate::galaxy::theater::system_name(o.system);
        self.deploying = Some(Deploying {
            sortie: o,
            job: rx,
            time: 0.0,
            title: world,
        });
    }

    /// A frame of deploying: once the battle's map is built, draw the
    /// battle from the galaxy and go into it.
    fn pump_deploy(&mut self, dt: f32) {
        let Some(d) = &mut self.deploying else { return };
        d.time += dt;
        let (mut game, world) = match d.job.try_recv() {
            Ok(built) => built,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.deploying = None;
                return;
            }
        };
        let o = self.deploying.take().expect("deploying").sortie;
        self.battle = None;
        let m = &MISSIONS[o.mission];
        (self.seed, self.map_kind, self.spawn, self.bots) = (m.seed, o.map_kind, 0, m.bots());
        // A generated map is the site's own, the same each time it's fought over.
        self.map_seed = o.map_seed;
        (self.contract, self.side) = (m.contract, 0);
        self.sortie = Some(o);
        // What happened to your company has been told by now.
        self.war.get().crew_news = None;
        // The kit you'd picked by now, if it changed while the map was built.
        let kit = self.profile.loadout;
        game.set_player_hull(kit.0);
        game.world.tanks[0].turret_kind = kit.1;
        game.known = Some(self.profile.known);
        game.crew = self.profile.company_name();
        let Some(b) = self.draw_battle(&mut game) else {
            // Your company isn't in this one (#362).
            self.sortie = None;
            self.audio.garage(audio::GarageCue::Refused);
            return;
        };
        self.in_war = true;
        self.battle = Some(b);
        self.load_built(game, world);
        self.mode = Mode::Playing;
        log::info!(
            "deployed: {} site {} ({}: {}, {}) for the {} against the {} (their gear {})",
            battle::system_name(o.system),
            o.site,
            m.name,
            m.contract.name(),
            m.map,
            o.factions[0].name(),
            o.factions[1].name(),
            crate::economy::level_label(self.game.rating),
        );
    }

    /// Put your company's theater on the missions screen, if it's showing
    /// and isn't already up to date with the galaxy. It doesn't move the
    /// war on: while there's no work for you, `pump_waiting` does, where
    /// you can see it.
    fn sync_theater(&mut self) {
        self.sync_theater_after(0, false);
    }

    /// The war goes on a slice of this frame while your company's between
    /// contracts on the contract screen, until it has work (or a ship to
    /// choose); the screen counts the ticks.
    fn pump_waiting(&mut self) {
        if self.mode != Mode::Menu || self.garage.is_open() || !self.menu.waiting() {
            return;
        }
        let rank = self.profile.rank().level;
        let Some(g) = self.war.try_get() else { return };
        adopt(g, &mut self.profile);
        let start = Instant::now();
        let mut left = u32::MAX;
        let done = g.await_work_slice(None, rank, &mut left, || start.elapsed() >= WAIT_BUDGET);
        let ticked = u32::MAX - left;
        self.menu.waited(ticked);
        if done {
            self.sync_theater_after(0, ticked > 0);
        } else if ticked > 0 {
            // What ended meanwhile (your contract, say) shows now.
            let ended = g.ended.clone();
            self.menu.set_ended(&ended);
        }
    }

    /// `sync_theater`, with the war going on at most `ticks` ticks for you
    /// to have work. `changed`: the galaxy's changed already, so it's saved.
    fn sync_theater_after(&mut self, ticks: u32, changed: bool) {
        if !self.menu.wants_theater() {
            return;
        }
        let rank = self.profile.rank().level;
        let g = self.war.get();
        adopt(g, &mut self.profile);
        let choice = g.crew_choice();
        let shown = self.menu.choice().map(|c| c.choice) == choice.as_ref().map(|c| c.choice);
        if shown
            && self.menu.theater().is_some_and(|t| {
                t.tick == g.tick && t.ship.as_ref().is_some_and(|s| Some(s.id) == g.crew)
            })
        {
            return;
        }
        // No battle for you this tick: the war goes on until there is one.
        let before = g.crew;
        let (t, ticked) = g.await_work(None, rank, ticks);
        let crew = g.crew;
        let choice = g.crew_choice();
        if changed || ticked || crew != before {
            self.war.save();
        }
        self.profile.board(crew);
        // The contract you were last shown, from before this menu (a battle
        // or a launch ago): a move on from it plays now (#386).
        if !self.menu.remembers() {
            let g = self.war.get();
            if let Some(old) = self.profile.shown.as_ref().and_then(|s| g.shown(s)) {
                self.menu.remember(old);
            }
        }
        // How the contract you were last shown ended, if it has.
        let ended = self.war.get().ended.clone();
        self.menu.set_ended(&ended);
        self.menu.set_choice(choice);
        self.menu.set_theater(t);
        self.keep_shown();
    }

    /// Keep the contract the missions screen has shown you in the profile.
    fn keep_shown(&mut self) {
        let shown = self.menu.shown().and_then(Shown::of);
        self.profile.set_shown(shown);
    }

    /// Keep the board of work the galaxy's, while it's showing (#365).
    fn sync_ships(&mut self) {
        if !self.menu.wants_ships() {
            return;
        }
        let rank = self.profile.rank().level;
        let g = self.war.get();
        adopt(g, &mut self.profile);
        let mine = g.aboard(rank).and_then(|t| Offer::of(&t));
        self.menu.board.set(g.open_work(rank), mine);
    }

    /// Leave your company for ship `id` (#365), then on to its theater.
    fn switch_crew(&mut self, id: u32) {
        let g = self.war.get();
        if g.switch_crew(id) {
            let crew = g.crew;
            self.war.save();
            self.profile.board(crew);
            self.menu.switched();
            self.menu.screen = menu::Screen::Missions;
        }
        self.sync_ships();
        self.sync_theater();
    }

    /// Go with ship `id`, of those your company's split or join offered (#364).
    fn choose_crew(&mut self, id: u32) {
        let g = self.war.get();
        if g.choose_crew(id) {
            let crew = g.crew;
            self.war.save();
            self.profile.board(crew);
        }
        self.sync_theater();
    }

    /// Keep the profile's company (shown throughout the menus) the galaxy's,
    /// once it's loaded; a new pilot joins one as soon as there's one at work.
    fn sync_crew(&mut self) {
        let Some(g) = self.war.try_get() else { return };
        adopt(g, &mut self.profile);
        let before = g.crew;
        g.enlist(None);
        let crew = g.crew;
        if self.menu.wants_cluster(g.tick, crew) {
            self.menu.set_cluster(crate::cluster_view::Cluster::of(g));
        }
        if crew != before {
            self.war.save();
        }
        self.profile.board(crew);
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
                self.load_built(game, mesh);
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
                        let game = backdrop_match(want.0, want.1, want.2, kit);
                        let mesh = StaticWorld::new(&scene::map_mesh(&game.world.map));
                        log::debug!(
                            "menu backdrop: {:?} built in {:.0?}",
                            want.0,
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

    /// Leave the match for the menu, or close the game. A battle still on
    /// isn't left there (#427): it plays out without you, fast, under a
    /// progress bar (see `frame`), and is settled like any finished one
    /// before you go.
    fn leave(&mut self, to: Leaving, event_loop: &ActiveEventLoop) {
        if self.leaving.is_some() {
            return;
        }
        let on = self.battle.is_some()
            && self.showroom.is_none()
            && self.mode != Mode::Menu
            && self.game.phase == Phase::Playing;
        if !on {
            match to {
                Leaving::Menu => self.back_to_menu(),
                Leaving::Quit => event_loop.exit(),
            }
            return;
        }
        log::info!("left the battle: it plays out without you");
        self.game.leave();
        self.garage.close();
        self.leaving = Some(to);
        self.mode = Mode::Playing;
    }

    /// A battle you left has played out and been settled: on to its
    /// debrief, like any battle's end, or close the game.
    fn left(&mut self) {
        if self.leaving.take() == Some(Leaving::Quit) {
            self.quit = true;
        }
    }

    /// The battle's over: back to the contract screen (the missions screen),
    /// to pick the next fight (#381).
    fn back_to_missions(&mut self) {
        self.start_return(true);
    }

    /// Start going back to the menu (the contract screen if `missions`):
    /// the battle stays up, dimmed under a loading panel, until the menu's
    /// ready (see `Returning`, `pump_return`).
    fn start_return(&mut self, missions: bool) {
        if self.returning.is_some() {
            return;
        }
        let kit = self.profile.loadout;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let started = Instant::now();
            let game = menu_match(kit);
            let mesh = StaticWorld::new(&scene::map_mesh(&game.world.map));
            log::debug!("menu built in {:.0?}", started.elapsed());
            let _ = tx.send((game, mesh));
        });
        self.returning = Some(Returning {
            missions,
            job: Some(rx),
            built: None,
            time: 0.0,
            summary: self
                .tick_summary
                .take()
                .filter(|_| missions)
                .map(|view| Summary { view, going: false }),
        });
    }

    /// A frame of going back to the menu: take the menu's map if it's
    /// built, and once it is (and the tick summary's been read), go.
    fn pump_return(&mut self, dt: f32) {
        let Some(r) = &mut self.returning else { return };
        r.time += dt;
        if let Some(job) = &r.job {
            match job.try_recv() {
                Ok(built) => {
                    r.built = Some(built);
                    r.job = None;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => r.job = None,
            }
        }
        let read = r.summary.as_ref().is_none_or(|s| s.going);
        if r.job.is_some() || !read {
            return;
        }
        let Some(r) = self.returning.take() else {
            return;
        };
        let (game, world) = r.built.unwrap_or_else(|| {
            let game = menu_match(self.profile.loadout);
            let world = StaticWorld::new(&scene::map_mesh(&game.world.map));
            (game, world)
        });
        self.back_to_menu_built(game, world);
        if r.missions {
            self.menu.screen = menu::Screen::Missions;
            self.sync_theater();
        }
    }

    fn back_to_menu(&mut self) {
        let game = menu_match(self.profile.loadout);
        let world = StaticWorld::new(&scene::map_mesh(&game.world.map));
        self.back_to_menu_built(game, world);
    }

    /// `back_to_menu`, with the menu's map already built.
    fn back_to_menu_built(&mut self, game: Match, world: StaticWorld) {
        // Settled by now (or never deployed into).
        self.battle = None;
        self.in_war = false;
        self.sortie = None;
        self.load_built(game, world);
        self.backdrop = (MISSIONS[0].map_kind, 0, 0);
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
    /// and do it. E (equip) has a sound of its own for what it did;
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
        // Opened from the pause box: DEPLOY goes back to battle, Esc back to the box.
        if self.mode == Mode::Paused && input == garage::Input::Deploy && !self.garage.is_open() {
            self.mode = Mode::Playing;
        }
    }

    /// Do what the garage asked. `voiced`: play the bay's sound for
    /// it (a swap or a refusal), not just the menu click already played.
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
            // A part you don't have: the garage already says whose wrecks teach it.
            garage::Event::Locked(_) => {
                if voiced {
                    self.audio.garage(audio::GarageCue::Refused);
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

    /// What `code` does in a menu: the arrows, Enter and Esc always, and the
    /// driving keys as bound. Space selects unless it drives.
    fn nav(code: KeyCode, binds: &Binds) -> Option<Nav> {
        let driving = [
            (Action::Forward, Nav::Up),
            (Action::Reverse, Nav::Down),
            (Action::TurnLeft, Nav::Left),
            (Action::TurnRight, Nav::Right),
        ];
        Some(match code {
            KeyCode::ArrowUp => Nav::Up,
            KeyCode::ArrowDown => Nav::Down,
            KeyCode::ArrowLeft => Nav::Left,
            KeyCode::ArrowRight => Nav::Right,
            KeyCode::Escape | KeyCode::Backspace => Nav::Back,
            KeyCode::Enter | KeyCode::NumpadEnter => Nav::Select,
            _ => match driving.iter().find(|(a, _)| binds.is(*a, code)) {
                Some(&(_, nav)) => nav,
                None if code == KeyCode::Space => Nav::Select,
                None => return None,
            },
        })
    }

    /// What `code` does in the garage: menu navigation, Esc puts the kit back,
    /// and the garage's keys as bound (the garage key deploys).
    fn garage_input(code: KeyCode, binds: &Binds) -> Option<garage::Input> {
        let bound = [
            (Action::Garage, garage::Input::Deploy),
            (Action::Equip, garage::Input::Act),
            (Action::Upgrade, garage::Input::Upgrade),
            (Action::SwitchTab, garage::Input::Switch),
        ];
        if let Some(&(_, input)) = bound.iter().find(|(a, _)| binds.is(*a, code)) {
            return Some(input);
        }
        Some(match (code, Self::nav(code, binds)?) {
            (KeyCode::Escape, _) => garage::Input::Revert,
            (_, Nav::Up) => garage::Input::Up,
            (_, Nav::Down) => garage::Input::Down,
            (_, Nav::Left) => garage::Input::Left,
            (_, Nav::Right) => garage::Input::Right,
            (KeyCode::Enter | KeyCode::NumpadEnter, Nav::Select) => garage::Input::Deploy,
            _ => return None,
        })
    }

    fn on_key(&mut self, code: KeyCode, event_loop: &ActiveEventLoop) {
        // Left: nothing to do but wait for the battle to play out.
        if self.leaving.is_some() {
            return;
        }
        let binds = self.profile.settings.binds;
        if self.garage.is_open() {
            let Some(input) = Self::garage_input(code, &binds) else {
                return;
            };
            let event = self.garage.input(input, self.queued(), &self.profile);
            self.garage_acted(input, event);
            return;
        }
        match self.mode {
            // Rebinding: the key goes on the slot, whatever it is.
            Mode::Menu if self.menu.capturing() => {
                let action = self.menu.capture(code);
                self.capture_sound(code, action.is_some() || !self.menu.capturing());
                self.menu_action(action, event_loop);
                return;
            }
            Mode::Paused if self.pause.capturing() => {
                let action = self.pause.capture(code);
                self.capture_sound(code, action.is_some() || !self.pause.capturing());
                self.pause_action(action, event_loop);
                return;
            }
            // Deploying: nothing to do but wait for the map.
            Mode::Menu if self.deploying.is_some() => return,
            Mode::Menu => {
                let Some(nav) = Self::nav(code, &binds) else {
                    return;
                };
                self.nav_sound(nav);
                let action = self.menu.input(nav);
                self.menu_action(action, event_loop);
                return;
            }
            Mode::Paused => {
                let Some(nav) = Self::nav(code, &binds) else {
                    return;
                };
                self.nav_sound(nav);
                let action = self.pause.input(nav);
                self.pause_action(action, event_loop);
                return;
            }
            Mode::Playing => {}
        }
        // On the way back to the contract screen: Enter or Esc goes on
        // from the tick summary (#475); nothing else does anything.
        if let Some(r) = &mut self.returning {
            if matches!(
                code,
                KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Escape
            ) && let Some(sum) = &mut r.summary
                && !sum.going
            {
                sum.going = true;
                self.audio.ui(audio::Ui::Select);
            }
            return;
        }
        let action = binds.find(
            code,
            &[
                Action::Garage,
                Action::NextHull,
                Action::NextTurret,
                Action::CameraReset,
            ],
        );
        match code {
            KeyCode::Escape if self.showroom.is_some() => event_loop.exit(),
            // Round over: back to the contract screen. Otherwise pause.
            KeyCode::Escape if self.game.phase == Phase::Over => self.back_to_missions(),
            KeyCode::Escape => {
                self.audio.ui(audio::Ui::Back);
                self.pause = Pause::new();
                self.mode = Mode::Paused;
            }
            // Garage: park and pick the hull / turret for the next respawn.
            _ if action == Some(Action::Garage)
                && self.showroom.is_none()
                && self.game.phase == Phase::Playing =>
            {
                if let Some(cam) = self.camera {
                    self.garage.toggle(&self.game.world, &cam, self.queued());
                }
            }
            // Round over: Enter goes on, like the one button there.
            KeyCode::Enter | KeyCode::NumpadEnter
                if self.showroom.is_none() && self.game.phase == Phase::Over =>
            {
                self.back_to_missions();
            }
            _ if action == Some(Action::CameraReset) => self.cam_homing = true,
            // Quick-cycle the kit queued for the next respawn.
            _ if action == Some(Action::NextHull) => {
                let (hull, turret) = self.queued();
                self.queue((self.next_owned(&garage::HULLS, hull, Part::Hull), turret));
            }
            _ if action == Some(Action::NextTurret) => {
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
        self.sync_theater();
        self.sync_ships();
        match action {
            Some(menu::Action::Fight(o)) => {
                // Into a battle on the contract on screen: you've seen it.
                self.menu.settle();
                self.keep_shown();
                self.start_sortie(o);
            }
            Some(menu::Action::Refused) => {
                // Your side isn't fighting there this tick.
                self.audio.garage(audio::GarageCue::Refused);
            }
            Some(menu::Action::Crew(id)) => self.choose_crew(id),
            Some(menu::Action::Switch(id)) => self.switch_crew(id),
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
            // Over the frozen match; what you pick there is your next respawn's kit.
            Some(PauseAction::Garage) => {
                if let Some(cam) = self.camera {
                    self.garage.toggle(&self.game.world, &cam, self.queued());
                }
            }
            Some(PauseAction::Setting(c)) => self.change_setting(c),
            Some(PauseAction::ExitToMenu) => self.leave(Leaving::Menu, event_loop),
            Some(PauseAction::Quit) => self.leave(Leaving::Quit, event_loop),
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
        // Off the BACK button (onto anything else, or nothing): unlit.
        if !matches!(hit, Some(UiHit::Menu(_))) {
            self.menu.unhover();
        }
        if !matches!(hit, Some(UiHit::Pause(_))) {
            self.pause.unhover();
        }
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
        if self.leaving.is_some() {
            return;
        }
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
            UiHit::Hud(hud::Hit::Missions | hud::Hit::Continue) => {
                self.on_key(KeyCode::Escape, event_loop)
            }
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

    /// The click for a key pressed while rebinding: `done` if it ended the wait.
    fn capture_sound(&mut self, code: KeyCode, done: bool) {
        if done {
            self.audio.ui(if code == KeyCode::Escape {
                audio::Ui::Back
            } else {
                audio::Ui::Select
            });
        }
    }

    fn nav_sound(&mut self, nav: Nav) {
        self.audio.ui(match nav {
            Nav::Select => audio::Ui::Select,
            Nav::Back => audio::Ui::Back,
            _ => audio::Ui::Move,
        });
    }

    /// Is a key for `action` held down?
    fn held(&self, action: Action) -> bool {
        self.profile
            .settings
            .binds
            .held(action, &|c| self.keys.contains(&c))
    }

    fn command(&self) -> Command {
        // Parked in the garage or the menu.
        if self.garage.is_open() || self.mode != Mode::Playing {
            return Command::default();
        }
        let axis = |pos, neg| (self.held(pos) as i32 - self.held(neg) as i32) as f32;
        Command {
            throttle: axis(Action::Forward, Action::Reverse),
            steer: axis(Action::TurnLeft, Action::TurnRight),
            turret: axis(Action::TurretLeft, Action::TurretRight),
            center_turret: self.held(Action::CenterTurret),
        }
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32();
        self.last = now;
        let mut laps = Laps::new(now);
        let gap = dt - self.last_work;
        if gap > HITCH_GAP && !self.loaded {
            hitch::report(format_args!(
                "hitch: {:.0} ms between frames, {:.0} ms of it outside the frame's work",
                dt * 1e3,
                gap * 1e3
            ));
        }
        self.loaded = false;

        // Tilt only in battle: in the garage Q/E switch tabs and equip.
        if self.mode == Mode::Playing && !self.garage.is_open() {
            let (up, down) = (self.held(Action::CameraUp), self.held(Action::CameraDown));
            let (tilt, homing) = tilt_step(self.cam.tilt, up, down, self.cam_homing, dt);
            (self.cam.tilt, self.cam_homing) = (tilt, homing);
        }

        let cmd = self.command();
        let fire = self.held(Action::Fire) && !self.garage.is_open() && self.mode == Mode::Playing;
        let mut steps = 0;
        if self.mode != Mode::Paused {
            steps = self.clock.advance(dt);
            for _ in 0..steps {
                if self.showroom.is_some() {
                    let w = &mut self.game.world;
                    let cmds = showroom::commands(w);
                    w.step(&cmds);
                    showroom::after_step(w);
                } else {
                    self.game.step(cmd, fire);
                }
            }
            // Out for good (#373), or left (#427): the rest of the match
            // plays out without you, as fast as a slice of each frame allows.
            if self.showroom.is_none() && self.game.playing_out() {
                let budget = if self.leaving.is_some() {
                    LEAVING_BUDGET
                } else {
                    FAST_FORWARD_BUDGET
                };
                let start = Instant::now();
                while self.game.playing_out() && start.elapsed() < budget {
                    self.game.fast_forward(FAST_FORWARD_CHUNK);
                }
            }
        }

        // Renown is paid the moment it's earned, to the Company; crystals, and the
        // schematic if you won it, when the contract is done.
        if self.game.renown > self.banked {
            self.profile.add_renown(self.game.renown - self.banked);
            self.banked = self.game.renown;
        }
        if self.mode == Mode::Playing
            && self.showroom.is_none()
            && self.game.phase == Phase::Over
            && !self.paid
        {
            let pay = Payout::of(&self.game);
            // Your company pays you, out of its own vault.
            let paid = match self.battle {
                Some(_) => self.war.get().pay_you(pay.total()),
                None => pay.total(),
            };
            self.profile.add_crystals(paid);
            // Take apart what you wrecked (#438): a schematic or two may be yours now.
            let new = self.profile.scrap(&self.game.player().scrap);
            for part in &new {
                log::info!("reverse-engineered the {} schematic", part.name());
            }
            if !new.is_empty() {
                self.audio.garage(audio::GarageCue::Bought);
            }
            self.paid = true;
            log::info!("contract paid {paid} of {} CR ({pay:?})", pay.total());
            if let Some(b) = self.battle.take() {
                let g = self.war.get();
                let (tick, owners) = (g.tick, g.owners());
                let site = b.site.map(|t| g.sites[t]);
                let settled = battle::conclude(g, &b, &self.game);
                self.tick_summary = Some(g.tick_summary(b.system, tick, &owners));
                // What it did to the site it was for, for the debrief.
                self.hud.debrief = site.map(|site| hud::WarResult {
                    world: crate::galaxy::theater::planet_name(
                        &crate::galaxy::theater::system_name(b.system),
                        site.planet,
                    ),
                    site: crate::galaxy::theater::kind_name(site.kind),
                    verdict: hud::Verdict::of(site.owner as usize == b.sides[0], settled.taken),
                });
                log::info!("{}", battle::describe(&b, &settled));
                crate::galaxy::ledger::record(self.war.get(), &b, &self.game, &settled);
                self.war.save();
            }
        }
        // Left, and it's played out and been settled: go.
        if self.leaving.is_some() && self.paid {
            self.left();
        }
        self.pump_return(dt);
        self.pump_deploy(dt);
        laps.lap("sim");

        // Respawned (in the new kit) or round over: back to battle.
        let alive = self.game.player().alive;
        if self.mode == Mode::Playing
            && ((alive && !self.player_alive) || self.game.phase != Phase::Playing)
        {
            self.garage.close();
        }
        self.player_alive = alive;
        if self.mode == Mode::Menu && !self.garage.is_open() {
            self.menu_time += dt;
            self.menu.advance(dt);
            self.keep_shown();
            self.follow_backdrop(dt);
            self.sync_crew();
            self.pump_waiting();
        }
        let queued = self.queued();
        let scores = self.held(Action::Scoreboard);
        if let (Some(r), Some(bay)) = (&mut self.renderer, &mut self.bay) {
            // The planet out of the bay is the world this map is on.
            bay.show_world(r, self.game.world.map.planet);
        }
        let menu_up = self.mode == Mode::Menu;
        // The system view, on the missions screen (the bay takes over while
        // the garage is open over it).
        let system = (self.mode == Mode::Menu)
            .then(|| self.menu.system())
            .flatten();
        if let (Some(r), Some(art), Some(t)) = (&mut self.renderer, &mut self.system, system) {
            art.show(r, theater_view::on_screen(t, self.menu.transfer()));
        }
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
                (Mode::Menu, _) => crate::vista::camera(
                    &self.game.world,
                    &anchor,
                    crate::vista::sway(self.menu_orbit, self.menu_time),
                    w / h,
                ),
                // Frozen: hold the last frame's view (the garage may still orbit it).
                (Mode::Paused, Some(c)) => self.chase.unwrap_or(c),
                _ => self.cam.update(&self.game.world, &anchor, dt),
            };
            self.chase = Some(camera);
            self.garage.update(dt);
            camera = self.garage.camera(camera, &self.game.world, &anchor, w / h);
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
            } else if let Some((c, time)) = self.menu.cluster().filter(|_| menu_up) {
                crate::cluster_view::scene(c, time, w / h)
            } else if let (Some(t), Some(sys)) = (system, &self.system) {
                theater_view::scene(sys, t, self.menu.transfer(), self.menu_time, w / h)
            } else if let Some((o, time)) = self.menu.deal().filter(|_| menu_up) {
                crate::work_view::scene(art, &driven, queued, o, time, w / h)
            } else {
                let mut scene = scene::build(&self.game.world, &tanks, art, camera);
                self.game.effect_instances(alpha, &mut scene.instances);
                // Ambient motion happens around wherever the camera is.
                self.game.ambience.focus = camera.pos;
                scene.particles.extend(self.game.ambience.particles());
                self.game.glow_particles(alpha, &mut scene.particles);
                scene
            };
            laps.lap("scene");
            if self.showroom.is_some() {
                scene.shadow_center = glam::Vec3::ZERO;
                scene.shadow_radius = 22.0;
            } else {
                let (ui, hits) = match self.mode {
                    Mode::Menu => {
                        let menu =
                            self.menu
                                .draw(&self.profile, w, h, 1.0 - self.garage.presence());
                        let garage = self.garage.draw(&driven, queued, &self.profile, w, h);
                        // The dip around a backdrop swap, under the panel.
                        let mut ui = if system.is_some()
                            || self.menu.cluster().is_some()
                            || self.menu.deal().is_some()
                        {
                            Vec::new()
                        } else {
                            backdrop_dip(self.backdrop_dark, w, h)
                        };
                        ui.extend(menu.verts);
                        ui.extend(garage.verts);
                        // Deploying: the loading panel over it all, and
                        // nothing to click.
                        if let Some(d) = &self.deploying {
                            ui.extend(hud::loading("DEPLOYING", &d.title, d.time, w, h));
                            (ui, Hits::new())
                        } else {
                            // The garage covers the menu panel while it's open.
                            let mut hits = if self.garage.is_open() {
                                Hits::new()
                            } else {
                                menu.hits.map(UiHit::Menu)
                            };
                            hits.extend(garage.hits.map(UiHit::Garage));
                            (ui, hits)
                        }
                    }
                    Mode::Playing if self.returning.is_some() => {
                        let ledger =
                            Ledger::before(&self.profile, &self.game, self.banked, self.paid);
                        let mut ui = menu::dim(
                            self.hud
                                .draw(&self.game, &tanks, &camera, w, h, dt, false, ledger)
                                .verts,
                        );
                        let r = self.returning.as_ref().expect("returning");
                        match &r.summary {
                            // The tick summary, the contract screen loading behind it (#475).
                            Some(sum) => {
                                let view = &sum.view;
                                let state = hud::SummaryState {
                                    loading: r.job.is_some(),
                                    going: sum.going,
                                    time: r.time,
                                    hover: self.hud.hover,
                                };
                                let layer = hud::tick_summary(view, state, w, h);
                                ui.extend(layer.verts);
                                (ui, layer.hits.map(UiHit::Hud))
                            }
                            None => {
                                ui.extend(hud::loading("ROUND OVER", "LOADING", r.time, w, h));
                                (ui, Hits::new())
                            }
                        }
                    }
                    Mode::Playing if self.leaving.is_some() => {
                        let ledger =
                            Ledger::before(&self.profile, &self.game, self.banked, self.paid);
                        let mut ui = menu::dim(
                            self.hud
                                .draw(&self.game, &tanks, &camera, w, h, dt, false, ledger)
                                .verts,
                        );
                        ui.extend(hud::playing_out(&self.game, w, h));
                        (ui, Hits::new())
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
                        let pause = self.pause.draw(&self.profile, self.game.renown, w, h);
                        ui.extend(pause.verts);
                        // The garage, opened from the box, fades the box out under it.
                        let layer = self
                            .garage
                            .overlay(ui, &driven, queued, &self.profile, w, h);
                        let mut hits = if self.garage.is_open() {
                            Hits::new()
                        } else {
                            pause.hits.map(UiHit::Pause)
                        };
                        hits.extend(layer.hits.map(UiHit::Garage));
                        (layer.verts, hits)
                    }
                };
                scene.ui = ui;
                self.hits = hits;
            }
            if self.profile.settings.debug {
                scene.ui.extend(hud::fps_counter(self.fps, w, h));
            }
            laps.lap("ui");
            r.render(&scene);
            laps.lap("render");
            self.camera = Some(camera);
            self.audio.set_turntable(self.garage.in_bay());
            let sounds = std::mem::take(&mut self.game.sounds);
            self.audio
                .update(&self.game, &sounds, &camera, self.mode != Mode::Paused, dt);
            laps.lap("audio");
        }
        self.last_work = laps.total();
        if self.last_work > HITCH_WORK {
            hitch::report(format_args!(
                "hitch: a {:.0} ms frame ({steps} sim steps at {:.1} s): {}",
                self.last_work * 1e3,
                self.game.world.time,
                laps
            ));
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

/// One frame of the chase camera's tilt: the tilt keys move it (and cancel a
/// reset); otherwise a reset eases it home. Returns the tilt and whether it's
/// still easing home.
fn tilt_step(tilt: f32, up: bool, down: bool, homing: bool, dt: f32) -> (f32, bool) {
    if up || down {
        let dir = (up as i32 - down as i32) as f32;
        return ((tilt + dir * CAM_TILT_SPEED * dt).clamp(0.0, 1.0), false);
    }
    if !homing {
        return (tilt, false);
    }
    let gap = scene::TILT_HOME - tilt;
    let step = CAM_RESET_SPEED * dt;
    if gap.abs() <= step {
        (scene::TILT_HOME, false)
    } else {
        (tilt + step.copysign(gap), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_tilts_and_resets() {
        let dt = 1.0 / 60.0;
        // Held up, it climbs to the top and stops there.
        let mut t = scene::TILT_HOME;
        for _ in 0..200 {
            t = tilt_step(t, true, false, false, dt).0;
        }
        assert_eq!(t, 1.0);
        // A reset eases it back home, a step at a time, then stops.
        let (mut homing, mut frames) = (true, 0);
        while homing {
            let before = t;
            (t, homing) = tilt_step(t, false, false, homing, dt);
            assert!(before - t <= CAM_RESET_SPEED * dt + 1e-6);
            frames += 1;
        }
        assert_eq!(t, scene::TILT_HOME);
        assert!(frames > 5, "snapped back in {frames} frames");
        // A tilt key during a reset takes over.
        let (t2, homing) = tilt_step(0.0, false, true, true, dt);
        assert_eq!((t2, homing), (0.0, false));
    }

    /// Moving the menu between contracts swaps the backdrop's map: the tank is
    /// already parked there, so nothing arrives, rings or drops a shield.
    #[test]
    fn a_menu_backdrop_is_silent() {
        let kit = (
            crate::sim::HullKind::Kataphrakt,
            crate::sim::TurretKind::Kontos,
        );
        let mut m = backdrop_match(MISSIONS[0].map_kind, 0, 1, kit);
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

    #[test]
    fn menus_and_the_garage_follow_the_driving_keys() {
        let mut binds = Binds::default();
        assert_eq!(App::nav(KeyCode::KeyW, &binds), Some(Nav::Up));
        assert_eq!(App::nav(KeyCode::Space, &binds), Some(Nav::Select));
        // ESDF driving: E moves up the lists, W does nothing, arrows still work.
        binds.bind(Action::Forward, 0, KeyCode::KeyE);
        binds.bind(Action::Reverse, 0, KeyCode::KeyD);
        binds.bind(Action::TurnLeft, 0, KeyCode::KeyS);
        binds.bind(Action::TurnRight, 0, KeyCode::KeyF);
        assert_eq!(App::nav(KeyCode::KeyE, &binds), Some(Nav::Up));
        assert_eq!(App::nav(KeyCode::KeyW, &binds), None);
        assert_eq!(App::nav(KeyCode::ArrowUp, &binds), Some(Nav::Up));
        // E left the garage's equip key; put it on R.
        assert_eq!(
            App::garage_input(KeyCode::KeyE, &binds),
            Some(garage::Input::Up)
        );
        assert_eq!(App::garage_input(KeyCode::KeyR, &binds), None);
        binds.bind(Action::Equip, 0, KeyCode::KeyR);
        assert_eq!(
            App::garage_input(KeyCode::KeyR, &binds),
            Some(garage::Input::Act)
        );
        assert_eq!(
            App::garage_input(KeyCode::Enter, &binds),
            Some(garage::Input::Deploy)
        );
        assert_eq!(
            App::garage_input(KeyCode::KeyG, &binds),
            Some(garage::Input::Deploy)
        );
        assert_eq!(
            App::garage_input(KeyCode::Escape, &binds),
            Some(garage::Input::Revert)
        );
        assert_eq!(App::garage_input(KeyCode::Space, &binds), None);
        // Space driving forward moves up the menus rather than selecting.
        binds.bind(Action::Forward, 1, KeyCode::Space);
        assert_eq!(App::nav(KeyCode::Space, &binds), Some(Nav::Up));
    }

    /// Hired by side 1, the player fights for that faction from its bases.
    #[test]
    fn side_one_puts_the_player_with_the_other_faction() {
        let m = &MISSIONS[0];
        let kit = (
            crate::sim::HullKind::Kataphrakt,
            crate::sim::TurretKind::Kontos,
        );
        let a = new_match(m.seed, m.map_kind, m.seed, 0, 0, 3, m.contract, kit, None);
        let b = new_match(m.seed, m.map_kind, m.seed, 1, 0, 3, m.contract, kit, None);
        assert_eq!(a.factions, m.sides);
        assert_eq!(b.factions, [m.sides[1], m.sides[0]]);
        assert_eq!(b.world.map.bases[0], a.world.map.bases[1]);
        assert_eq!(b.factions[0], Faction::Resonant);
    }
}
