mod ai;
mod ambience;
mod app;
mod audio;
mod combat;
mod economy;
mod faction;
mod game;
mod garage;
mod hold;
mod hud;
mod map;
mod menu;
mod mission;
mod profile;
mod render;
mod repair;
mod rng;
mod scene;
mod showroom;
mod sim;
mod tank;
mod ui;

use anyhow::{Context, Result, bail};
use glam::Vec3;

const USAGE: &str = "\
tankit [OPTIONS]

  (no options)            open the main menu (missions, garage)
  any of --seed --map --spawn --team-size --contract --employer --hull --turret skips the menu and drops straight into a match
  --seed <n>              match seed: bot brains, spawns, kits (default: random)
  --map <saltmarket|shards|kiln>
                          Saltmarket (desert town, the default), Shard Flats (crystal deposit,
                          the Hold map), or the Kiln (Ithessar ground)
  --spawn <n>             which spawn point to start at
  --team-size <n>         tanks per side, you included (default 6)
  --contract <attrition|hold>
                          game mode: team deathmatch (default) or hold the site
  --employer <faction>    which of the map's two sides hires you (default: its first);
                          your trust in the match goes to them
  --shot <out.png>        render offscreen and save
  --wav <out.wav>         record a match's sound offscreen (autopilot, trigger held) for --time seconds (default 12)
  --time <seconds>        simulated time before the shot (default 1)
  --size <WxH>            shot resolution (default 1600x900)
  --frames <n>            (with --shot) render n frames from --time on, saved as <out>_000.png ...
  --fps <f>               frame rate for --frames (default 30)
  --straight              (with --shot) full throttle straight ahead instead of the weaving autopilot
  --cam <x,y,z,tx,ty,tz>  (with --shot) fixed camera at x,y,z looking at tx,ty,tz, no HUD (map review; also the showroom)
  --hull <kataphrakt|kastron|sethuun|vigil|gravis>  player hull (default kataphrakt; sethuun hovers)
  --turret <name>         player turret: kontos pelekys (Imperium), ammeth ossrin (Ithessar),
                          lumen (Uthenn), hymn (Resonant); default kontos
  --garage                (with --shot) open the garage at the start; --hull/--turret become the pick
  --screen <menu|missions|settings|pause|results>  (with --shot) render that screen
  --debug                 (with --shot) debug mode on: FPS counter
  --screen results        (with --shot) end the round 3 s before the shot: scoreboard and contract pay
  --trust <n>             (with --shot) trust with every faction for menu, garage and pause shots
  --crystals <n>          (with --shot) pilot crystals for menu, garage and pause shots
  --showroom              every turret on an open yard, orbiting camera
  --orbit <degrees>       showroom: fixed camera angle instead of orbiting
  --dist <meters>         showroom: camera distance (default 24)
  --focus <0-5>           showroom: frame one tank
";

pub struct Args {
    pub seed: u64,
    pub map: MapKind,
    pub spawn: usize,
    pub bots: usize,
    pub contract: mission::Contract,
    /// Which of the map's sides hires you (0 or 1).
    pub side: usize,
    pub loadout: garage::Loadout,
    /// --hull or --turret was given.
    kit_given: bool,
    /// A match flag was given: skip the menu.
    play: bool,
    garage: bool,
    screen: Option<Screen>,
    trust: u32,
    crystals: u32,
    debug: bool,
    straight: bool,
    /// Fixed screenshot camera: position and target.
    cam: Option<(Vec3, Vec3)>,
    shot: Option<ShotArgs>,
    /// Record sound to this WAV, for this many seconds.
    wav: Option<(String, f32)>,
    showroom: Option<ShowroomArgs>,
}

impl Args {
    /// An in-memory pilot for headless shots (never saved).
    fn profile(&self) -> profile::Profile {
        let mut p = profile::Profile::in_memory();
        p.set_trust_all(self.trust);
        p.crystals = self.crystals;
        p.settings.debug = self.debug;
        p
    }

    pub fn map(&self) -> map::Map {
        let mut m = make_map(self.map);
        if self.side == 1 {
            m.swap_sides();
        }
        m
    }
}

/// Which arena to load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapKind {
    /// Hand-built desert town on a dry reservoir.
    Saltmarket,
    /// Placeholder: open flats around a crystal deposit (the Hold map).
    ShardFlats,
    /// Ithessar ground: crystal monoliths holding a plasma core.
    Kiln,
}

pub fn make_map(kind: MapKind) -> map::Map {
    match kind {
        MapKind::Saltmarket => map::Map::saltmarket(),
        MapKind::ShardFlats => map::Map::shard_flats(),
        MapKind::Kiln => map::Map::kiln(),
    }
}

#[derive(Clone, Copy)]
pub struct ShowroomArgs {
    pub orbit: Option<f32>,
    pub dist: f32,
    pub focus: Option<usize>,
}

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Menu,
    Missions,
    Settings,
    Pause,
    Results,
}

struct ShotArgs {
    out: String,
    time: f32,
    width: u32,
    height: u32,
    frames: u32,
    fps: f32,
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn"),
    )
    .init();
    let args = parse_args()?;
    if let Some((out, seconds)) = &args.wav {
        return record_wav(&args, out, *seconds);
    }
    match (&args.shot, args.showroom) {
        (Some(shot), Some(room)) => showroom_shot(shot, room, args.cam),
        (Some(shot), None)
            if matches!(
                args.screen,
                Some(Screen::Menu | Screen::Missions | Screen::Settings)
            ) =>
        {
            menu_shot(&args, shot)
        }
        (Some(shot), None) => screenshot(&args, shot),
        (None, showroom) => app::run(app::Launch {
            seed: args.seed,
            map_kind: args.map,
            spawn: args.spawn,
            bots: args.bots,
            contract: args.contract,
            side: args.side,
            loadout: args.kit_given.then_some(args.loadout),
            menu: !args.play,
            showroom,
        }),
    }
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args().skip(1);
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos() as u64
        % 100_000;
    let mut shot: Option<String> = None;
    let mut wav: Option<String> = None;
    let mut time: Option<f32> = None;
    let (mut width, mut height) = (1600, 900);
    let (mut frames, mut fps) = (1u32, 30.0f32);
    let (mut map_kind, mut spawn, mut straight) = (MapKind::Saltmarket, 0usize, false);
    let mut showroom = false;
    let mut cam: Option<(Vec3, Vec3)> = None;
    let mut bots: Option<usize> = None;
    let mut contract = mission::Contract::Attrition;
    let mut employer: Option<faction::Faction> = None;
    let mut loadout = (sim::HullKind::Kataphrakt, sim::TurretKind::Kontos);
    let mut garage = false;
    let (mut kit_given, mut play) = (false, false);
    let mut screen = None;
    let (mut trust, mut crystals) = (0, 0);
    let mut debug = false;
    let mut room = ShowroomArgs {
        orbit: None,
        dist: 24.0,
        focus: None,
    };
    while let Some(a) = args.next() {
        if matches!(
            a.as_str(),
            "--seed"
                | "--map"
                | "--spawn"
                | "--team-size"
                | "--contract"
                | "--employer"
                | "--hull"
                | "--turret"
        ) {
            play = true;
        }
        kit_given |= matches!(a.as_str(), "--hull" | "--turret");
        match a.as_str() {
            "--shot" => shot = Some(args.next().context("--shot needs a path")?),
            "--seed" => seed = args.next().context("--seed needs a number")?.parse()?,
            "--map" => match args.next().as_deref() {
                Some("saltmarket") => map_kind = MapKind::Saltmarket,
                Some("shards") => map_kind = MapKind::ShardFlats,
                Some("kiln") => map_kind = MapKind::Kiln,
                other => {
                    bail!("--map expects saltmarket, shards or kiln, got {other:?}")
                }
            },
            "--spawn" => spawn = args.next().context("--spawn needs a number")?.parse()?,
            "--team-size" => {
                let n: usize = args.next().context("--team-size needs a number")?.parse()?;
                if n == 0 {
                    bail!("--team-size must be at least 1");
                }
                bots = Some(2 * n - 1);
            }
            "--contract" => match args.next().as_deref() {
                Some("attrition") => contract = mission::Contract::Attrition,
                Some("hold") => contract = mission::Contract::Hold,
                other => bail!("--contract expects attrition or hold, got {other:?}"),
            },
            "--employer" => {
                let name = args.next().unwrap_or_default();
                employer =
                    Some(faction::Faction::by_name(&name).with_context(|| {
                        format!("--employer expects a faction name, got {name:?}")
                    })?);
            }
            "--straight" => straight = true,
            "--cam" => {
                let v: Vec<f32> = args
                    .next()
                    .context("--cam needs x,y,z,tx,ty,tz")?
                    .split(',')
                    .map(str::parse)
                    .collect::<Result<_, _>>()?;
                if v.len() != 6 {
                    bail!("--cam needs six numbers: x,y,z,tx,ty,tz");
                }
                cam = Some((Vec3::new(v[0], v[1], v[2]), Vec3::new(v[3], v[4], v[5])));
            }
            "--time" => time = Some(args.next().context("--time needs seconds")?.parse()?),
            "--wav" => wav = Some(args.next().context("--wav needs a path")?),
            "--frames" => frames = args.next().context("--frames needs a count")?.parse()?,
            "--fps" => fps = args.next().context("--fps needs a number")?.parse()?,
            "--size" => {
                let s = args.next().context("--size needs WxH")?;
                let (w, h) = s.split_once('x').context("--size format is WxH")?;
                (width, height) = (w.parse()?, h.parse()?);
            }
            "--hull" => {
                let name = args.next().unwrap_or_default().to_ascii_uppercase();
                loadout.0 = *garage::HULLS
                    .iter()
                    .find(|&&k| garage::hull_name(k) == name)
                    .context("--hull expects kataphrakt, kastron or sethuun")?;
            }
            "--turret" => {
                let name = args.next().unwrap_or_default().to_ascii_uppercase();
                loadout.1 = *garage::TURRETS
                    .iter()
                    .find(|&&k| garage::turret_name(k) == name)
                    .context("--turret expects kontos, pelekys, ammeth, ossrin, lumen or hymn")?;
            }
            "--garage" => garage = true,
            "--screen" => {
                screen = Some(match args.next().as_deref() {
                    Some("menu") => Screen::Menu,
                    Some("missions") => Screen::Missions,
                    Some("settings") => Screen::Settings,
                    Some("pause") => Screen::Pause,
                    Some("results") => Screen::Results,
                    other => {
                        bail!(
                            "--screen expects menu, missions, settings, pause or results, got {other:?}"
                        )
                    }
                })
            }
            "--trust" => trust = args.next().context("--trust needs a number")?.parse()?,
            "--crystals" => crystals = args.next().context("--crystals needs a number")?.parse()?,
            "--debug" => debug = true,
            "--showroom" => showroom = true,
            "--orbit" => {
                room.orbit = Some(
                    args.next()
                        .context("--orbit needs degrees")?
                        .parse::<f32>()?
                        .to_radians(),
                )
            }
            "--dist" => room.dist = args.next().context("--dist needs meters")?.parse()?,
            "--focus" => {
                room.focus = Some(args.next().context("--focus needs a tank index")?.parse()?)
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("unknown argument {other}\n\n{USAGE}"),
        }
    }
    let sides = make_map(map_kind).factions;
    let side = match employer {
        None => 0,
        Some(f) => sides.iter().position(|&s| s == f).with_context(|| {
            format!(
                "--employer: nobody hires {} here; this map is {} against {}",
                f.name(),
                sides[0].name(),
                sides[1].name()
            )
        })?,
    };
    Ok(Args {
        seed,
        side,
        map: map_kind,
        spawn,
        bots: bots.unwrap_or(11),
        contract,
        loadout,
        kit_given,
        play,
        garage,
        screen,
        trust,
        crystals,
        debug,
        straight,
        cam,
        wav: wav.map(|out| (out, time.unwrap_or(12.0))),
        shot: shot.map(|out| ShotArgs {
            out,
            time: time.unwrap_or(1.0),
            width,
            height,
            frames,
            fps,
        }),
        showroom: showroom.then_some(room),
    })
}

/// Headless: record a match's sound to a WAV, then print its levels.
fn record_wav(args: &Args, out: &str, seconds: f32) -> Result<()> {
    const RATE: u32 = 48_000;
    let mut game = combat::Match::new(args.map(), args.spawn, args.bots, args.seed);
    game.set_contract(args.contract);
    game.set_player_hull(args.loadout.0);
    game.world.tanks[0].turret_kind = args.loadout.1;
    let straight = args.straight;
    let volume = profile::Settings::default().volume();
    let buf = audio::record(&mut game, seconds, RATE, volume, |w| {
        if straight {
            sim::Command {
                throttle: 1.0,
                ..Default::default()
            }
        } else {
            game::autopilot(w.time)
        }
    });
    std::fs::write(out, audio::wav(&buf, RATE)).with_context(|| format!("save {out}"))?;
    let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    let rms = (buf.iter().map(|x| x * x).sum::<f32>() / buf.len().max(1) as f32).sqrt();
    let hot = buf.iter().filter(|x| x.abs() > 0.95).count();
    println!(
        "{out}: {seconds} s, peak {peak:.3}, rms {rms:.4} ({:.1} dBFS), {hot} samples near full scale",
        20.0 * rms.max(1e-9).log10()
    );
    Ok(())
}

fn frame_path(shot: &ShotArgs, i: u32) -> String {
    if shot.frames > 1 {
        format!("{}_{i:03}.png", shot.out.trim_end_matches(".png"))
    } else {
        shot.out.clone()
    }
}

fn screenshot(args: &Args, shot: &ShotArgs) -> Result<()> {
    let mut game = combat::Match::new(args.map(), args.spawn, args.bots, args.seed);
    game.set_contract(args.contract);
    if args.garage {
        game.fighters[0].next_kit = Some(args.loadout);
    } else {
        game.set_player_hull(args.loadout.0);
        game.world.tanks[0].turret_kind = args.loadout.1;
    }
    let profile = args.profile();
    game.set_player_levels(profile.levels);
    if args.screen == Some(Screen::Results) {
        game.time_left = (shot.time - 3.0).max(0.1);
    }
    let mut cam = scene::ChaseCam::new();
    let mut garage = garage::Garage::new();
    let mut renderer = render::Renderer::new_headless(shot.width, shot.height)?;
    renderer.set_static_mesh(&scene::map_mesh(&game.world.map));
    let art = tank::TankArt::upload(&mut renderer);
    let mut hud = hud::Hud::new();
    let command = |world: &sim::World| {
        if args.garage {
            sim::Command::default()
        } else if args.straight {
            sim::Command {
                throttle: 1.0,
                ..Default::default()
            }
        } else {
            game::autopilot(world.time)
        }
    };

    // Simulate `seconds` more, keeping the camera in step; returns the latest camera.
    // The autopilot holds the trigger so shots show combat.
    let aspect = shot.width as f32 / shot.height as f32;
    let advance = |m: &mut combat::Match,
                   cam: &mut scene::ChaseCam,
                   garage: &mut garage::Garage,
                   seconds: f32| {
        let mut camera = cam.update(&m.world, &m.world.tanks[0], game::DT);
        if args.garage && !garage.is_open() {
            garage.toggle(&m.world, &camera, args.loadout);
        }
        for _ in 0..(seconds / game::DT).round() as u32 {
            m.ambience.focus = args.cam.map_or(camera.pos, |c| c.0);
            let cmd = command(&m.world);
            m.step(cmd, !args.straight && !args.garage);
            camera = cam.update(&m.world, &m.world.tanks[0], game::DT);
            garage.update(game::DT);
        }
        garage.camera(camera, &m.world, &m.world.tanks[0], aspect)
    };
    let mut camera = advance(&mut game, &mut cam, &mut garage, shot.time);

    for i in 0..shot.frames {
        if i > 0 {
            camera = advance(&mut game, &mut cam, &mut garage, 1.0 / shot.fps);
        }
        let mut tanks = game.display_tanks(1.0);
        let driven = tanks[0].clone();
        garage.preview(&mut tanks[0]);
        let view = match args.cam {
            Some((pos, target)) => render::Camera {
                pos,
                target,
                fov_y: 60f32.to_radians(),
            },
            None => camera,
        };
        let mut frame = scene::build(&game.world, &tanks, &art, view);
        game.effect_instances(1.0, &mut frame.instances);
        frame.particles.extend(game.ambience.particles());
        game.glow_particles(1.0, &mut frame.particles);
        if args.cam.is_none() {
            let (w, h) = (shot.width as f32, shot.height as f32);
            let ui = hud.draw(&game, &tanks, &camera, w, h, game::DT, false);
            frame.ui = garage.overlay(ui, &driven, args.loadout, &profile, h).verts;
            if args.screen == Some(Screen::Pause) {
                frame.ui = menu::dim(std::mem::take(&mut frame.ui));
                frame.ui.extend(
                    menu::Pause::new()
                        .draw(&profile, game.factions[0], game.trust, w, h)
                        .verts,
                );
            }
            if profile.settings.debug {
                frame.ui.extend(hud::fps_counter(0.0, w, h));
            }
        }
        let out = frame_path(shot, i);
        renderer
            .render_to_image(&frame)?
            .save(&out)
            .with_context(|| format!("save {out}"))?;
    }
    let world = &game.world;
    let t = &world.tanks[0];
    for team in 0..2 {
        let board: Vec<String> = game
            .standings()
            .into_iter()
            .filter(|&i| game.fighters[i].team == team)
            .map(|i| {
                format!(
                    "{} {}/{}",
                    hud::callsign(&game, i),
                    game.fighters[i].kills,
                    game.fighters[i].deaths
                )
            })
            .collect();
        println!(
            "{} {} (kills/deaths): {}",
            game.factions[team].name(),
            game.score(team),
            board.join(", ")
        );
    }
    println!(
        "wrote {} frame(s) to {} ({}x{}, t={:.2}s) tank at {:.1?} up {:.2?} wheels down {} face contacts {}",
        shot.frames,
        shot.out,
        shot.width,
        shot.height,
        world.time,
        t.pos,
        t.up(),
        t.wheels_down,
        t.face_contacts
    );
    Ok(())
}

/// The main menu or mission select over the menu backdrop, headless.
fn menu_shot(args: &Args, shot: &ShotArgs) -> Result<()> {
    let mut profile = args.profile();
    profile.loadout = args.loadout;
    let mut game = app::menu_match(profile.loadout);
    let mut menu = menu::Menu::new();
    match args.screen {
        Some(Screen::Missions) => {
            menu.input(menu::Nav::Select);
        }
        Some(Screen::Settings) => {
            menu.input(menu::Nav::Down);
            menu.input(menu::Nav::Down);
            menu.input(menu::Nav::Select);
        }
        _ => {}
    }
    let mut renderer = render::Renderer::new_headless(shot.width, shot.height)?;
    renderer.set_static_mesh(&scene::map_mesh(&game.world.map));
    let art = tank::TankArt::upload(&mut renderer);
    for _ in 0..(shot.time / game::DT).round() as u32 {
        game.step(sim::Command::default(), false);
    }
    let (w, h) = (shot.width as f32, shot.height as f32);
    let angle = garage::front_angle(&game.world.tanks[0]) + shot.time * 0.06;
    let tanks = game.display_tanks(1.0);
    let camera = garage::orbit_camera(&game.world, &tanks[0], angle, w / h);
    let mut frame = scene::build(&game.world, &tanks, &art, camera);
    frame.ui = menu.draw(&profile, h, 1.0).verts;
    if profile.settings.debug {
        frame.ui.extend(hud::fps_counter(0.0, w, h));
    }
    renderer
        .render_to_image(&frame)?
        .save(&shot.out)
        .with_context(|| format!("save {}", shot.out))?;
    println!("wrote menu to {}", shot.out);
    Ok(())
}

fn showroom_shot(shot: &ShotArgs, room: ShowroomArgs, cam: Option<(Vec3, Vec3)>) -> Result<()> {
    let mut world = showroom::world();
    let mut renderer = render::Renderer::new_headless(shot.width, shot.height)?;
    renderer.set_static_mesh(&showroom::ground_mesh());
    let art = tank::TankArt::upload(&mut renderer);
    let advance = |world: &mut sim::World, seconds: f32| {
        for _ in 0..(seconds / game::DT).round() as u32 {
            let cmds = showroom::commands(world);
            world.step(&cmds);
            showroom::after_step(world);
        }
    };
    advance(&mut world, shot.time);
    for i in 0..shot.frames {
        if i > 0 {
            advance(&mut world, 1.0 / shot.fps);
        }
        let angle = room.orbit.unwrap_or(world.time * 0.15 + 0.6);
        let camera = match cam {
            Some((pos, target)) => render::Camera {
                pos,
                target,
                fov_y: 60f32.to_radians(),
            },
            None => showroom::camera(&world, angle, room.dist, room.focus),
        };
        let mut frame = scene::build(&world, &world.tanks, &art, camera);
        frame.shadow_center = Vec3::ZERO;
        frame.shadow_radius = 22.0;
        let out = frame_path(shot, i);
        renderer
            .render_to_image(&frame)?
            .save(&out)
            .with_context(|| format!("save {out}"))?;
    }
    println!(
        "wrote {} frame(s) to {} (showroom, t={:.2}s)",
        shot.frames, shot.out, world.time
    );
    Ok(())
}
