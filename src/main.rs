mod ai;
mod ambience;
mod app;
mod arena;
mod arrival;
mod audio;
mod bay;
mod bench;
mod binds;
mod combat;
mod economy;
mod faction;
mod fx;
mod galaxy;
mod game;
mod garage;
mod hitch;
mod hold;
mod hud;
mod keys;
mod map;
mod menu;
mod mission;
mod planet;
mod profile;
mod raid;
mod render;
mod repair;
mod rng;
mod scene;
mod ship_model;
mod showroom;
mod sim;
mod sweep;
mod tank;
mod theater_view;
mod thumbs;
mod ui;
mod vista;

use anyhow::{Context, Result, bail};
use glam::Vec3;

const USAGE: &str = "\
tankit [OPTIONS]

  (no options)            open the main menu (missions, garage)
  any of --seed --map-seed --map --spawn --team-size --contract --employer --hull --turret skips the menu and drops straight into a match
  --seed <n>              match seed: bot brains, spawns, kits (default: random)
  --map <saltmarket|sanctum|kiln|city|shield-city|shield-site|foundry|mine|desert|shield-desert>
                          Saltmarket (desert town, the default), the Sanctum (a Resonant church
                          on a mesa, the Hold map), the Kiln (Ithessar ground), the city
                          (a generated Imperium city, all four factions), the shield city (the same
                          city, a shield generator in place of its rotunda; shield-site: the same
                          on two sides, as a theater's generator is fought on), the Foundry (a
                          wrecked Uthenn tungsten cube factory), or the mine (a generated Ithessar
                          crystal mine: caves, rails and stacked levels), the desert (generated open
                          desert) or the shield desert (the same, a shield generator in its landmark's slot)
  --map-seed <n>          which one a generated map (city, shield-city, shield-site, mine, desert, shield-desert) builds (default: the --seed)
  --generator <faction>   (with --map shield-city, shield-site or shield-desert) whose shield generator stands in it:
                          imperium (the default), resonant, uthenn or ithessar
  --generator-down <n>    (with --map shield-city, shield-site or shield-desert) the generator's down, n ticks of repair
                          to go: 4 a wreck nobody's started on, 1 to 3 going back up (default 0: up)
  --greybox               a generated map's skeleton, as plain boxes, before it's dressed
  --no-lod                draw everything at full detail however far off (to compare against)
  --spawn <n>             which spawn point to start at
  --team-size <n>         tanks per side, you included (default 6)
  --contract <attrition|hold|raid>
                          game mode: a raid on an enemy camp for its hulls (default), hold the site, or raid
                          (one-sided capture the flag, two-sided maps only)
  --employer <faction>    which of the map's sides hires you (default: its first); with
                          --screen missions|site, a ship on contract for that side
                          (with --screen factions|missions: whose missions the menu shows)
  --bench <seconds>       play a round headless at 60 fps (after --time s of warmup, default 3)
                          and print what each part of the frame cost (sim, scene, HUD, GPU, sound)
  --arena <rounds>        play bot-only rounds headless (seeds from --seed up) and print each
                          side's wins; every bot runs the frozen baseline brain unless --improve
  --galaxy <ticks>        run the galaxy's forever war on its own (from --seed) for that many ticks,
                          print each faction's share and how long a tick takes; with --shot,
                          save a chart of the run there
  --improve <faction>     (with --arena) that side's bots run the current brain
  --front                 (with --arena or --shot) each round is a battle drawn from a galaxy at war
                          (from --seed; never the one you play in): the sides fight on hull
                          allocations, and --arena prints the galaxy's numbers before and after
  --hired                 (with --front --shot) the battle is where a company is under contract
                          for your side (the most merc-heavy there is), so your side has mercs
  --hulls <n,n,...>       (with --shot) each side's hull allocation, all told
  --duel <hull,turret,hull,turret,metres,count>
                          one bot against one on open ground, that far apart, count times: the
                          first kit under the current brain (then the baseline) against the
                          second under the baseline; prints how often the first wins
  --shot <out.png>        render offscreen and save
  --wav <out.wav>         record a match's sound offscreen (autopilot, trigger held) for --time seconds (default 12)
  --time <seconds>        simulated time before the shot (default 1)
  --size <WxH>            shot resolution (default 1600x900)
  --frames <n>            (with --shot) render n frames from --time on, saved as <out>_000.png ...
  --fps <f>               frame rate for --frames (default 30)
  --straight              (with --shot) full throttle straight ahead instead of the weaving autopilot
  --tilt <0..1>           (with --shot) chase camera tilt: 0 low behind the tank, 1 high (default 0.35)
  --cam <x,y,z,tx,ty,tz>  (with --shot) fixed camera at x,y,z looking at tx,ty,tz, no HUD (map review; also the showroom)
  --hull <kataphrakt|kastron|sethuun|vigil|gravis>  player hull (default kataphrakt; sethuun hovers)
  --turret <name>         player turret: kontos pelekys (Imperium), ammeth ossrin (Ithessar),
                          lumen (Uthenn), hymn knell (Resonant); default kontos
  --garage                (with --shot) open the garage at the start, previewing --hull/--turret
                          (the parts of it you own are equipped, starters otherwise)
  --preview <part>        (with --garage) highlight this part instead, --hull/--turret equipped
  --own <part>            (with --shot) the pilot has bought this hull or turret (repeatable)
  --screen <menu|missions|companies|settings|controls|rebind|pause|results>  (with --shot) render that screen
  --screen pause-settings (with --shot) the pause box with its settings open
  --screen pause --garage (with --shot) the garage opened from the pause box, the match frozen under it
  --debug                 (with --shot) debug mode on: FPS counter
  --screen dead           (with --shot) the player is destroyed 3.5 s before the shot: the death
                          screen and its respawn countdown
  --screen results        (with --shot) end the round 4 s before the shot (0.3 s before the first of
                          --frames): scoreboard, renown gained, crystal tally
                          and (with --mission, if you won) the schematic stolen
  --screen carry          (with --shot --contract raid) the player starts on the enemy's flag,
                          facing home, and carries it
  --mission <name>        (with --screen results) the round was this mission, by name, e.g.
                          'four winds', for its prize
  --site <n>              (with --screen missions|site) select the theater's nth contested site
  --transfer <jump|move|renew>
                          (with --screen missions) the war goes on until your company moves onto its
                          next contract (in another system, the same one, or the same contract
                          renewed), as if after a battle (a fresh menu, the old contract from the
                          profile): the shot is --time seconds into the transfer playing over the
                          system view
  --choice <split|merge>  (with --screen missions) your company splits in two, or merges into
                          another, and you choose which ship to go with
  --won                   (with --screen missions) your company just won the battle at --site
                          (default the first it fights for), under that site's contract, and
                          the rest of the war ticked with it, as back from a battle
  --defend                (with --screen missions|site) the war goes on until your company is hired
                          to hold one of its employer's worlds, not to take one; (with --front
                          --hired) the battle is one where a company's hired to hold the world
                          (from 0; default: the first your side fights for)
  --renown <n>            (with --shot) Company renown (your rank) for menu, garage and pause shots
  --crystals <n>          (with --shot) pilot crystals for menu, garage and pause shots
  --upgrade <steps>       (with --shot) every part the pilot owns is this many upgrade steps up (10 a mark)
  --earned <n>            (with --screen results) renown earned this match, for the results graphic
  --showroom              every turret on an open yard, orbiting camera
  --orbit <degrees>       showroom: fixed camera angle instead of orbiting
  --dist <meters>         showroom: camera distance (default 24)
  --focus <0-6>           showroom: frame one tank
";

pub struct Args {
    pub seed: u64,
    /// Which map a generated kind builds (--map-seed, else the seed).
    pub map_seed: u64,
    /// --greybox: a generated map's skeleton, undressed.
    pub greybox: bool,
    pub map: MapKind,
    pub spawn: usize,
    pub bots: usize,
    pub contract: mission::Contract,
    /// Which of the map's sides hires you.
    pub side: usize,
    /// --employer, for menu shots.
    employer: Option<faction::Faction>,
    /// --mission, for results shots: index into `MISSIONS`.
    mission: Option<usize>,
    /// --site, for theater shots: index into the contested sites.
    site: Option<usize>,
    /// --transfer, for missions shots: the move onto the next contract,
    /// `--time` seconds in: to another system (`Jump`), within the one
    /// it's in (`Move`), or the same contract renewed (`Renew`).
    transfer: Option<TransferKind>,
    /// --choice, for missions shots: your company splits (false) or joins
    /// another (true), and the screen asks which ship you go with.
    choice: Option<bool>,
    /// --won, for missions shots: your company just won its battle.
    won: bool,
    /// --defend, for missions shots: your company's on a defense contract.
    defend: bool,
    pub loadout: garage::Loadout,
    /// --hull or --turret was given.
    kit_given: bool,
    /// A match flag was given: skip the menu.
    play: bool,
    garage: bool,
    /// Parts the shot's pilot has stolen.
    own: Vec<economy::Part>,
    /// Garage shot: highlight this part instead of --hull/--turret.
    preview: Option<economy::Part>,
    screen: Option<Screen>,
    renown: u32,
    crystals: u32,
    /// Results shot: renown earned in the match.
    earned: u32,
    /// Shots: upgrade steps on every owned part.
    upgrade: u8,
    debug: bool,
    straight: bool,
    /// Shots: the chase camera's tilt.
    tilt: f32,
    /// Fixed screenshot camera: position and target.
    cam: Option<(Vec3, Vec3)>,
    shot: Option<ShotArgs>,
    /// Record sound to this WAV, for this many seconds.
    wav: Option<(String, f32)>,
    /// Benchmark this many seconds of a round, after this much warmup.
    bench: Option<(f32, f32, u32, u32)>,
    showroom: Option<ShowroomArgs>,
    /// Bot-only rounds to play headless, and which side runs the current
    /// bots against the baseline (None: every side baseline).
    arena: Option<(u32, Option<faction::Faction>)>,
    /// Duels: (hull, turret) under the current brain against one under
    /// the baseline, this far apart, this many times.
    duel: Option<(garage::Loadout, garage::Loadout, f32, u32)>,
    /// Run the galaxy's war on its own for this many ticks.
    galaxy: Option<u64>,
    /// Rounds are battles drawn from a galaxy (`--front`).
    pub front: bool,
    /// --hired: the --front battle is one with mercs on your side.
    pub hired: bool,
    /// Each side's hull allocation, for shots.
    hulls: Option<Vec<u32>>,
}

impl Args {
    /// An in-memory pilot for headless shots (never saved).
    fn profile(&self) -> profile::Profile {
        let mut p = profile::Profile::in_memory();
        p.renown = self.renown;
        p.crystals = self.crystals;
        p.settings.debug = self.debug;
        for &part in &self.own {
            p.grant(part);
        }
        for part in economy::PARTS {
            if p.owns(part) {
                p.levels.set(part, self.upgrade);
            }
        }
        p
    }

    pub fn map(&self) -> map::Map {
        let mut m = match (self.greybox, self.map) {
            (true, MapKind::City) => map::procgen::greybox(&city_spec(self.map_seed)),
            (true, MapKind::Mine) => map::procgen::greybox(&mine_spec(self.map_seed)),
            (true, MapKind::Desert) => map::procgen::greybox(&desert_spec(self.map_seed)),
            (true, MapKind::ShieldCity(g)) => {
                map::procgen::greybox(&shield_spec(self.map_seed, g, false))
            }
            (true, MapKind::ShieldSite(g)) => {
                map::procgen::greybox(&shield_spec(self.map_seed, g, true))
            }
            (true, MapKind::ShieldDesert(g)) => {
                map::procgen::greybox(&shield_desert_spec(self.map_seed, g))
            }
            _ => make_map(self.map, self.map_seed),
        };
        if self.side > 0 {
            m.swap_sides(self.side);
        }
        m
    }
}

/// Which arena to load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapKind {
    /// Hand-built desert town on a dry reservoir.
    Saltmarket,
    /// The Sanctum: a Resonant church on a mesa (the Hold map).
    Sanctum,
    /// Ithessar ground: crystal monoliths holding a plasma core.
    Kiln,
    /// An Imperium city, half ruined round a domed rotunda: four sides.
    /// Generated: a different city for each map seed.
    City,
    /// The same city with a shield generator in the middle in place of the
    /// rotunda, the standard model of the side that holds it: the same
    /// layout for the same seed.
    ShieldCity(Generator),
    /// The shield city as a theater's shield generator site is fought on
    /// (#341): two sides (the theater's), the generator its holder's, in
    /// the shape the site's in.
    ShieldSite(Generator),
    /// The Foundry: a wrecked Uthenn tungsten cube factory.
    Foundry,
    /// An Ithessar crystal mine in a desert escarpment, on three levels.
    /// Generated: a different mine for each map seed.
    Mine,
    /// Open desert: dunes, buttes and spires of sandstone round the
    /// Needles in the middle. Generated: a different desert for each seed.
    Desert,
    /// The desert with a shield generator in its landmark's slot (#337),
    /// the standard model of the side that holds it, and its camp.
    ShieldDesert(Generator),
}

/// The map `kind` names; `seed` picks which one, for a generated kind (the
/// hand-built ones are always the same).
pub fn make_map(kind: MapKind, seed: u64) -> map::Map {
    match kind {
        MapKind::Saltmarket => map::Map::saltmarket(),
        MapKind::Sanctum => map::Map::sanctum(),
        MapKind::Kiln => map::Map::kiln(),
        MapKind::City => map::procgen::map(&city_spec(seed)),
        MapKind::ShieldCity(g) => map::procgen::map(&shield_spec(seed, g, false)),
        MapKind::ShieldSite(g) => map::procgen::map(&shield_spec(seed, g, true)),
        MapKind::Foundry => map::Map::foundry(),
        MapKind::Mine => map::procgen::map(&mine_spec(seed)),
        MapKind::Desert => map::procgen::map(&desert_spec(seed)),
        MapKind::ShieldDesert(g) => map::procgen::map(&shield_desert_spec(seed, g)),
    }
}

/// The imperial city `seed` names: four sides, one corner each.
pub fn city_spec(seed: u64) -> map::procgen::Spec {
    map::procgen::Spec {
        seed,
        biome: map::procgen::Biome::City,
        look: map::procgen::Look::Imperium,
        sides: faction::Faction::ALL.to_vec(),
        centerpiece: map::procgen::Centerpiece::Monument,
    }
}

/// The imperial city `seed` names, `owner`'s shield generator in its
/// middle (whoever holds the city now).
pub fn shield_city_spec(seed: u64, owner: faction::Faction) -> map::procgen::Spec {
    shield_spec(seed, Generator::up(owner), false)
}

/// A shield generator for a map: whose model it is, and what shape it's in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Generator {
    pub owner: faction::Faction,
    pub condition: map::procgen::shield::Condition,
}

impl Generator {
    pub const fn up(owner: faction::Faction) -> Generator {
        Generator {
            owner,
            condition: map::procgen::shield::Condition::Up,
        }
    }
}

/// The imperial city `seed` names with generator `g` in its middle: all
/// four sides, or for a theater's site, two (the default matchup, which the
/// theater's sides stand in for).
pub fn shield_spec(seed: u64, g: Generator, site: bool) -> map::procgen::Spec {
    let sides = match site {
        true => map::DEFAULT_SIDES.to_vec(),
        false => faction::Faction::ALL.to_vec(),
    };
    map::procgen::Spec {
        centerpiece: map::procgen::Centerpiece::ShieldGenerator(g.owner, g.condition),
        sides,
        ..city_spec(seed)
    }
}

/// The Ithessar crystal mine `seed` names: the Ithessar in the west, the
/// Resonant in the east.
pub fn mine_spec(seed: u64) -> map::procgen::Spec {
    map::procgen::Spec {
        seed,
        biome: map::procgen::Biome::Mine,
        look: map::procgen::Look::Ithessar,
        sides: vec![faction::Faction::Ithessar, faction::Faction::Resonant],
        centerpiece: map::procgen::Centerpiece::Monument,
    }
}

/// The open desert `seed` names: the default matchup.
pub fn desert_spec(seed: u64) -> map::procgen::Spec {
    map::procgen::Spec {
        seed,
        biome: map::procgen::Biome::Desert,
        // Nobody's: the look only matters once something's built there.
        look: map::procgen::Look::Imperium,
        sides: map::DEFAULT_SIDES.to_vec(),
        centerpiece: map::procgen::Centerpiece::Monument,
    }
}

/// The desert `seed` names with generator `g` in its landmark's slot.
pub fn shield_desert_spec(seed: u64, g: Generator) -> map::procgen::Spec {
    map::procgen::Spec {
        centerpiece: map::procgen::Centerpiece::ShieldGenerator(g.owner, g.condition),
        ..desert_spec(seed)
    }
}

#[derive(Clone, Copy)]
pub struct ShowroomArgs {
    pub orbit: Option<f32>,
    pub dist: f32,
    pub focus: Option<usize>,
}

/// Which move onto the next contract a `--transfer` shot waits for.
#[derive(Clone, Copy, Debug, PartialEq)]
enum TransferKind {
    Jump,
    Move,
    Renew,
}

impl TransferKind {
    /// Going from `a` to `b` is a move of this kind.
    fn is(self, a: &galaxy::theater::Theater, b: &galaxy::theater::Theater) -> bool {
        if !theater_view::Transfer::between(a, b) {
            return false;
        }
        let (sa, sb) = (a.ship.as_ref().unwrap(), b.ship.as_ref().unwrap());
        let renewed =
            sa.id == sb.id && sa.realm == sb.realm && sa.side == sb.side && a.system == b.system;
        match self {
            TransferKind::Jump => a.system != b.system,
            TransferKind::Move => a.system == b.system && !renewed,
            TransferKind::Renew => renewed,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Menu,
    Missions,
    /// A contested site of the theater.
    Site,
    Settings,
    /// Settings' key bindings.
    Controls,
    /// The other companies' ships to switch to (#365).
    Companies,
    /// The key bindings, waiting for the key to put on FIRE's second slot.
    Rebind,
    Pause,
    /// The pause box with its settings open.
    PauseSettings,
    Results,
    /// The death screen: destroyed, counting down to the respawn.
    Dead,
    /// Your company's last hull destroyed, you in it: you're out, and the
    /// match fast-forwards without you (needs `--front --hired`).
    Spent,
    /// Likewise, once the fast-forward has played the match out: the results.
    SpentOver,
    /// Raid: the player starts on the enemy's flag, and carries it.
    Carry,
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
    if let Some((warmup, seconds, w, h)) = args.bench {
        let mut game = combat::Match::new(args.map(), args.spawn, args.bots, args.seed);
        game.start();
        game.set_contract(args.contract);
        game.set_player_hull(args.loadout.0);
        game.world.tanks[0].turret_kind = args.loadout.1;
        return bench::run(game, w, h, warmup, seconds);
    }
    if let Some(ticks) = args.galaxy {
        return galaxy::run(args.seed, ticks, args.shot.as_ref().map(|s| s.out.as_str()));
    }
    if let Some((a, b, start, reps)) = args.duel {
        return arena::duel(a, b, start, reps);
    }
    if let Some((rounds, improve)) = args.arena {
        return arena::run(&args, rounds, improve);
    }
    if let Some((out, seconds)) = &args.wav {
        return record_wav(&args, out, *seconds);
    }
    match (&args.shot, args.showroom) {
        (Some(shot), Some(room)) => showroom_shot(shot, room, args.cam),
        (Some(shot), None)
            if matches!(
                args.screen,
                Some(
                    Screen::Menu
                        | Screen::Missions
                        | Screen::Site
                        | Screen::Settings
                        | Screen::Companies
                        | Screen::Controls
                        | Screen::Rebind
                )
            ) =>
        {
            menu_shot(&args, shot)
        }
        (Some(shot), None) => screenshot(&args, shot),
        (None, showroom) => app::run(app::Launch {
            seed: args.seed,
            map_seed: args.map_seed,
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

/// A part named on the command line (any hull or turret name).
fn part_arg(name: Option<String>, flag: &str) -> Result<economy::Part> {
    let name = name.with_context(|| format!("{flag} needs a part name"))?;
    economy::PARTS
        .into_iter()
        .find(|p| p.name().eq_ignore_ascii_case(&name))
        .with_context(|| format!("{flag}: no part called {name}"))
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args().skip(1);
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos() as u64
        % 100_000;
    let mut shot: Option<String> = None;
    let mut map_seed: Option<u64> = None;
    let mut greybox = false;
    let mut wav: Option<String> = None;
    let mut bench: Option<f32> = None;
    let mut arena: Option<u32> = None;
    let mut galaxy: Option<u64> = None;
    let mut front = false;
    let mut hired = false;
    let mut hulls: Option<Vec<u32>> = None;
    let mut improve: Option<faction::Faction> = None;
    let mut duel: Option<(garage::Loadout, garage::Loadout, f32, u32)> = None;
    let mut time: Option<f32> = None;
    let (mut width, mut height) = (1600, 900);
    let (mut frames, mut fps) = (1u32, 30.0f32);
    let (mut map_kind, mut spawn, mut straight) = (MapKind::Saltmarket, 0usize, false);
    let mut tilt = scene::TILT_HOME;
    let mut showroom = false;
    let mut cam: Option<(Vec3, Vec3)> = None;
    let mut team_size: Option<usize> = None;
    let mut contract = mission::Contract::Attrition;
    let mut employer: Option<faction::Faction> = None;
    let mut generator: Option<faction::Faction> = None;
    let mut generator_down = 0u8;
    let mut mission = None;
    let mut site = None;
    let mut transfer = None;
    let mut choice = None;
    let mut won = false;
    let mut defend = false;
    let mut loadout = (sim::HullKind::Kataphrakt, sim::TurretKind::Kontos);
    let mut garage = false;
    let mut own = Vec::new();
    let mut preview = None;
    let (mut kit_given, mut play) = (false, false);
    let mut screen = None;
    let (mut renown, mut crystals, mut earned) = (0, 0, 0);
    let mut upgrade = 0;
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
                | "--map-seed"
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
            "--map-seed" => {
                map_seed = Some(args.next().context("--map-seed needs a number")?.parse()?)
            }
            "--greybox" => greybox = true,
            "--no-lod" => render::lod::disable(),
            "--map" => match args.next().as_deref() {
                Some("saltmarket") => map_kind = MapKind::Saltmarket,
                Some("sanctum") => map_kind = MapKind::Sanctum,
                Some("kiln") => map_kind = MapKind::Kiln,
                Some("city") => map_kind = MapKind::City,
                Some("shield-city") => {
                    map_kind = MapKind::ShieldCity(Generator::up(faction::Faction::Imperium))
                }
                Some("shield-site") => {
                    map_kind = MapKind::ShieldSite(Generator::up(faction::Faction::Imperium))
                }
                Some("foundry") => map_kind = MapKind::Foundry,
                Some("mine") => map_kind = MapKind::Mine,
                Some("desert") => map_kind = MapKind::Desert,
                Some("shield-desert") => {
                    map_kind = MapKind::ShieldDesert(Generator::up(faction::Faction::Imperium))
                }
                other => {
                    bail!(
                        "--map expects saltmarket, sanctum, kiln, city, shield-city, shield-site, foundry, mine, desert or shield-desert, got {other:?}"
                    )
                }
            },
            "--spawn" => spawn = args.next().context("--spawn needs a number")?.parse()?,
            "--team-size" => {
                let n: usize = args.next().context("--team-size needs a number")?.parse()?;
                if n == 0 {
                    bail!("--team-size must be at least 1");
                }
                team_size = Some(n);
            }
            "--contract" => match args.next().as_deref() {
                Some("attrition") => contract = mission::Contract::Attrition,
                Some("hold") => contract = mission::Contract::Hold,
                Some("raid") => contract = mission::Contract::Raid,
                other => bail!("--contract expects attrition, hold or raid, got {other:?}"),
            },
            "--generator" => {
                let name = args.next().unwrap_or_default();
                generator = Some(faction::Faction::by_name(&name).with_context(|| {
                    format!("--generator expects a faction name, got {name:?}")
                })?);
            }
            "--generator-down" => {
                generator_down = args
                    .next()
                    .context("--generator-down needs a number of ticks")?
                    .parse()?;
            }
            "--employer" => {
                let name = args.next().unwrap_or_default();
                employer =
                    Some(faction::Faction::by_name(&name).with_context(|| {
                        format!("--employer expects a faction name, got {name:?}")
                    })?);
            }
            "--mission" => {
                let name = args.next().unwrap_or_default();
                mission = Some(
                    mission::MISSIONS
                        .iter()
                        .position(|m| m.name.eq_ignore_ascii_case(&name))
                        .with_context(|| format!("--mission: no mission called {name:?}"))?,
                );
            }
            "--site" => site = Some(args.next().context("--site needs a number")?.parse()?),
            "--transfer" => {
                transfer = Some(match args.next().as_deref() {
                    Some("jump") => TransferKind::Jump,
                    Some("move") => TransferKind::Move,
                    Some("renew") => TransferKind::Renew,
                    other => bail!("--transfer expects jump, move or renew, got {other:?}"),
                })
            }
            "--choice" => {
                choice = Some(match args.next().as_deref() {
                    Some("split") => false,
                    Some("merge") => true,
                    other => bail!("--choice expects split or merge, got {other:?}"),
                })
            }
            "--won" => won = true,
            "--defend" => defend = true,
            "--straight" => straight = true,
            "--tilt" => {
                tilt = args
                    .next()
                    .context("--tilt needs a number")?
                    .parse::<f32>()?;
                anyhow::ensure!((0.0..=1.0).contains(&tilt), "--tilt goes from 0 to 1");
            }
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
            "--bench" => bench = Some(args.next().context("--bench needs seconds")?.parse()?),
            "--arena" => {
                arena = Some(
                    args.next()
                        .context("--arena needs a round count")?
                        .parse()?,
                )
            }
            "--galaxy" => {
                galaxy = Some(
                    args.next()
                        .context("--galaxy needs a tick count")?
                        .parse()?,
                )
            }
            "--front" => front = true,
            "--hired" => hired = true,
            "--hulls" => {
                let v = args.next().context("--hulls needs n,n,...")?;
                hulls = Some(
                    v.split(',')
                        .map(|n| n.parse())
                        .collect::<Result<_, _>>()
                        .context("--hulls needs whole numbers")?,
                )
            }
            "--duel" => {
                let spec = args
                    .next()
                    .context("--duel needs hull,turret,hull,turret,metres,count")?;
                let v: Vec<&str> = spec.split(',').collect();
                if v.len() != 6 {
                    bail!("--duel needs hull,turret,hull,turret,metres,count, got {spec:?}");
                }
                let hull = |n: &str| match economy::Part::by_name(n) {
                    Some(economy::Part::Hull(h)) => Ok(h),
                    _ => Err(anyhow::anyhow!("--duel: no hull called {n:?}")),
                };
                let gun = |n: &str| match economy::Part::by_name(n) {
                    Some(economy::Part::Turret(t)) => Ok(t),
                    _ => Err(anyhow::anyhow!("--duel: no turret called {n:?}")),
                };
                duel = Some((
                    (hull(v[0])?, gun(v[1])?),
                    (hull(v[2])?, gun(v[3])?),
                    v[4].parse()?,
                    v[5].parse()?,
                ));
            }
            "--improve" => {
                let name = args.next().context("--improve needs a faction name")?;
                improve =
                    Some(faction::Faction::by_name(&name).with_context(|| {
                        format!("--improve expects a faction name, got {name:?}")
                    })?);
            }
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
                    .context(
                        "--turret expects kontos, pelekys, ammeth, ossrin, lumen, hymn or knell",
                    )?;
            }
            "--garage" => garage = true,
            "--own" => own.push(part_arg(args.next(), "--own")?),
            "--preview" => preview = Some(part_arg(args.next(), "--preview")?),
            "--screen" => {
                screen = Some(match args.next().as_deref() {
                    Some("menu") => Screen::Menu,
                    Some("missions") => Screen::Missions,
                    Some("site") => Screen::Site,
                    Some("settings") => Screen::Settings,
                    Some("controls") => Screen::Controls,
                    Some("companies") => Screen::Companies,
                    Some("rebind") => Screen::Rebind,
                    Some("pause") => Screen::Pause,
                    Some("pause-settings") => Screen::PauseSettings,
                    Some("results") => Screen::Results,
                    Some("dead") => Screen::Dead,
                    Some("spent") => Screen::Spent,
                    Some("spent-over") => Screen::SpentOver,
                    Some("carry") => Screen::Carry,
                    other => {
                        bail!(
                            "--screen expects menu, missions, site, settings, controls, rebind, pause, results, dead, spent, spent-over or carry, got {other:?}"
                        )
                    }
                })
            }
            "--renown" => renown = args.next().context("--renown needs a number")?.parse()?,
            "--crystals" => crystals = args.next().context("--crystals needs a number")?.parse()?,
            "--upgrade" => upgrade = args.next().context("--upgrade needs a number")?.parse()?,
            "--earned" => earned = args.next().context("--earned needs a number")?.parse()?,
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
    let map_seed = map_seed.unwrap_or(seed);
    if let MapKind::ShieldCity(g) | MapKind::ShieldSite(g) | MapKind::ShieldDesert(g) =
        &mut map_kind
    {
        if let Some(owner) = generator {
            g.owner = owner;
        }
        g.condition = map::procgen::shield::Condition::down(generator_down, galaxy::REPAIR_TICKS);
    }
    let sides = make_map(map_kind, map_seed).factions;
    let menu_shot = matches!(
        screen,
        Some(
            Screen::Menu
                | Screen::Missions
                | Screen::Site
                | Screen::Settings
                | Screen::Companies
                | Screen::Controls
                | Screen::Rebind
        )
    );
    let side = match employer {
        None => 0,
        // The menu's ship works for whoever hired it (--employer picks one
        // working for that side, if there is one).
        Some(_) if menu_shot => 0,
        Some(f) => sides.iter().position(|&s| s == f).with_context(|| {
            format!(
                "--employer: nobody hires {} here; this map is {}",
                f.name(),
                sides
                    .iter()
                    .map(|s| s.name())
                    .collect::<Vec<_>>()
                    .join(" against ")
            )
        })?,
    };
    if contract == mission::Contract::Raid && sides.len() != 2 {
        bail!("--contract raid takes a map with two sides");
    }
    Ok(Args {
        seed,
        map_seed,
        greybox,
        side,
        employer,
        mission,
        site,
        transfer,
        choice,
        won,
        map: map_kind,
        spawn,
        // Every side filled to the team size around you.
        bots: sides.len() * team_size.unwrap_or(6) - 1,
        contract,
        loadout,
        kit_given,
        play,
        garage,
        own,
        preview,
        screen,
        renown,
        crystals,
        earned,
        upgrade,
        debug,
        straight,
        defend,
        tilt,
        cam,
        wav: wav.map(|out| (out, time.unwrap_or(12.0))),
        bench: bench.map(|s| (time.unwrap_or(3.0), s, width, height)),
        arena: arena.map(|n| (n, improve)),
        front,
        hired,
        hulls,
        duel,
        galaxy,
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
    game.start();
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
    let mut profile = args.profile();
    // In the garage: --hull/--turret are previewed; what's owned of them is equipped.
    let equipped = (
        if profile.owns(economy::Part::Hull(args.loadout.0)) {
            args.loadout.0
        } else {
            sim::HullKind::Kataphrakt
        },
        if profile.owns(economy::Part::Turret(args.loadout.1)) {
            args.loadout.1
        } else {
            sim::TurretKind::Kontos
        },
    );
    if args.garage {
        game.fighters[0].next_kit = Some(equipped);
    } else {
        game.set_player_hull(args.loadout.0);
        game.world.tanks[0].turret_kind = args.loadout.1;
    }
    game.set_player_levels(profile.levels);
    if args.front {
        let mut g = galaxy::Galaxy::warmed(args.seed);
        let b = match args.hired {
            true => galaxy::battle::deploy_hired(&mut g, &mut game, args.defend),
            false => None,
        }
        .unwrap_or_else(|| galaxy::battle::deploy(&mut g, &mut game));
        // With the company fighting for you there, if there is one; else the
        // one a new pilot would join.
        let side = game.factions.first().map(|f| f.index());
        g.crew = b.crew.or_else(|| {
            g.companies
                .iter()
                .find(|c| {
                    c.contract.is_some_and(|k| {
                        k.target as usize == b.system && Some(k.side as usize) == side
                    })
                })
                .map(|c| c.id)
        });
        g.enlist(side);
        g.claim_own(profile.rank().level);
        game.set_own_hulls(g.own_left());
        profile.ship = g.crew;
        game.crew = profile.company_name();
    }
    if let Some(h) = &args.hulls {
        game.set_allocation(h);
    }
    // All set up: the round starts, its tanks fielded.
    game.start();
    if args.screen == Some(Screen::Results) {
        // A still shows the results settled; a clip starts as they begin to count.
        let lead = if shot.frames > 1 { 0.3 } else { 4.0 };
        game.time_left = (shot.time - lead).max(0.1);
        game.renown = args.earned;
        if let Some(i) = args.mission {
            game.prize = mission::MISSIONS[i]
                .prize(args.side)
                .filter(|&p| !profile.owns(p));
        }
    }
    if args.screen == Some(Screen::Carry) {
        let r = game
            .raid
            .as_ref()
            .context("--screen carry needs --contract raid")?;
        let (flag, home) = (r.flag_at(&game.world.tanks), r.capture_point());
        let to = glam::Vec2::new(home.x - flag.x, home.z - flag.z);
        game.world
            .teleport(0, glam::Vec2::new(flag.x, flag.z), sim::yaw_of(to));
    }
    let mut cam = scene::ChaseCam::new();
    cam.tilt = args.tilt;
    let mut garage = garage::Garage::new();
    let mut renderer = render::Renderer::new_headless(shot.width, shot.height)?;
    renderer.set_static_mesh(&scene::map_mesh(&game.world.map));
    let art = tank::TankArt::upload(&mut renderer);
    let mut bay_art = bay::BayArt::upload(&mut renderer);
    bay_art.show_world(&mut renderer, game.world.map.planet);
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
            garage.toggle(&m.world, &camera, equipped);
            garage.browse(match args.preview {
                Some(part) => garage::with_part(equipped, part),
                None => args.loadout,
            });
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
    let mut camera = if matches!(args.screen, Some(Screen::Spent | Screen::SpentOver)) {
        // Your company's last hull goes, with you in it, and a respawn's
        // wait later you're out: the shot catches the fast-forward, or
        // (spent-over) a few seconds of the results it ends on.
        let lead = (combat::RESPAWN_TIME + 1.0).min(shot.time);
        advance(&mut game, &mut cam, &mut garage, shot.time - lead);
        let me = game.fighters[0].team;
        if let Some(h) = game.company_hulls.get_mut(me) {
            *h = 0;
        }
        game.own_hulls = 0;
        game.destroy_player();
        let camera = advance(&mut game, &mut cam, &mut garage, lead);
        if args.screen == Some(Screen::SpentOver) {
            game.fast_forward(usize::MAX);
            advance(&mut game, &mut cam, &mut garage, 3.0)
        } else {
            camera
        }
    } else if args.screen == Some(Screen::Dead) {
        // Destroyed by the nearest enemy a few seconds before the shot.
        let lead = 3.5f32.min(shot.time);
        advance(&mut game, &mut cam, &mut garage, shot.time - lead);
        game.destroy_player();
        advance(&mut game, &mut cam, &mut garage, lead)
    } else {
        advance(&mut game, &mut cam, &mut garage, shot.time)
    };

    for i in 0..shot.frames {
        if i > 0 {
            camera = advance(&mut game, &mut cam, &mut garage, 1.0 / shot.fps);
        }
        let mut tanks = game.display_tanks(1.0);
        let driven = tanks[0].clone();
        garage.render_thumbs(
            &mut renderer,
            &art,
            &game.world,
            &driven,
            equipped,
            shot.height as f32,
            usize::MAX,
        );
        garage.preview(&mut tanks[0]);
        let view = match args.cam {
            Some((pos, target)) => render::Camera {
                pos,
                target,
                fov_y: 60f32.to_radians(),
            },
            None => camera,
        };
        let mut frame = if garage.in_bay() && args.cam.is_none() {
            let view = garage.bay_camera(shot.width as f32 / shot.height as f32);
            bay::scene(
                &art,
                &bay_art,
                &tanks[0],
                garage.spin(),
                game.world.time,
                view,
            )
        } else {
            let mut frame = scene::build(&game.world, &tanks, &art, view);
            game.effect_instances(1.0, &mut frame.instances);
            frame.particles.extend(game.ambience.particles());
            game.glow_particles(1.0, &mut frame.particles);
            frame
        };
        if args.cam.is_none() {
            let (w, h) = (shot.width as f32, shot.height as f32);
            let ui = hud
                .draw(
                    &game,
                    &tanks,
                    &camera,
                    w,
                    h,
                    game::DT,
                    false,
                    hud::Ledger::before(&profile, &game, 0, false),
                )
                .verts;
            // Paused, the box is under the garage (opened from it, with --garage), as in the game.
            let ui = if matches!(args.screen, Some(Screen::Pause | Screen::PauseSettings)) {
                let mut ui = menu::dim(ui);
                let mut pause = menu::Pause::new();
                if args.screen == Some(Screen::PauseSettings) {
                    // Down to SETTINGS, and open it.
                    for nav in [menu::Nav::Down, menu::Nav::Down, menu::Nav::Select] {
                        pause.input(nav);
                    }
                }
                ui.extend(pause.draw(&profile, game.renown, w, h).verts);
                ui
            } else {
                ui
            };
            frame.ui = garage.overlay(ui, &driven, equipped, &profile, w, h).verts;
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
    for team in 0..game.factions.len() {
        // One pilot each (#389): a merc marked, one out of the fight too.
        let board: Vec<String> = game
            .pilots()
            .into_iter()
            .filter(|p| p.team == team)
            .map(|p| {
                format!(
                    "{}{} {}/{}{}",
                    if p.company && p.seat != 0 {
                        "merc "
                    } else {
                        ""
                    },
                    if p.seat == 0 {
                        "YOU"
                    } else {
                        garage::turret_name(p.turret)
                    },
                    p.kills,
                    p.deaths,
                    if p.active { "" } else { " (out)" }
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

/// Your company splits (or, `joined`, merges into another), leaving you the
/// choice of ship; after a split, the war goes on a little, while that
/// choice stays open, until one of the two halves is at work (so they differ).
fn force_choice(g: &mut galaxy::Galaxy, joined: bool) -> Result<()> {
    let c = (0..g.companies.len())
        .find(|&c| g.companies[c].contract.is_none())
        .context("no idle company")?;
    g.crew = Some(g.companies[c].id);
    let co = &mut g.companies[c];
    if joined {
        (co.crystals, co.hulls) = (10.0, 0.0);
    } else {
        co.crystals = g.rules.merc_split * 1.6;
    }
    g.split_and_join();
    anyhow::ensure!(g.crew_choice().is_some(), "no choice of ship");
    if joined {
        // The ships it could go with are at work or not already: and a
        // small one of them might join another of its own if we waited.
        return Ok(());
    }
    for _ in 0..300 {
        let Some(v) = g.crew_choice() else { break };
        if v.ships.iter().any(|b| b.work.is_some()) {
            return Ok(());
        }
        let keep = (v.choice, g.crew);
        g.step();
        // A split or join of its own on the way: keep the one we're showing.
        if g.crew_choice.is_none_or(|c| c != keep.0) {
            g.crew_choice = Some(keep.0);
            g.crew = keep.1;
        }
    }
    anyhow::ensure!(g.crew_choice().is_some(), "the choice closed");
    Ok(())
}

/// The main menu or mission select over the menu backdrop, headless.
fn menu_shot(args: &Args, shot: &ShotArgs) -> Result<()> {
    let mut profile = args.profile();
    profile.loadout = args.loadout;
    let mut menu = menu::Menu::new();
    // A new pilot's galaxy, with the company they join (one working for
    // --employer, if there is one).
    let prefer = args.employer.map(|f| f.index());
    let war = || {
        let mut g = galaxy::Galaxy::warmed(galaxy::save::SEED);
        let rank = profile.rank().level;
        let t = if args.defend {
            defending(&mut g, prefer, rank)
        } else {
            g.await_work_staying(prefer, rank, 100).0
        };
        (g.crew, t)
    };
    match args.screen {
        None | Some(Screen::Menu) => profile.ship = war().0,
        Some(Screen::Missions | Screen::Site) => {
            menu.input(menu::Nav::Select);
            let (crew, t) = war();
            profile.ship = crew;
            menu.set_theater(t);
            if let Some(joined) = args.choice {
                let mut g = galaxy::Galaxy::warmed(galaxy::save::SEED);
                let rank = profile.rank().level;
                g.await_work_staying(prefer, rank, 100);
                force_choice(&mut g, joined)?;
                profile.ship = g.crew;
                menu.set_choice(g.crew_choice());
                menu.set_theater(g.aboard(rank));
            }
            if let Some(kind) = args.transfer {
                let mut g = galaxy::Galaxy::warmed(galaxy::save::SEED);
                let rank = profile.rank().level;
                let mut t = g.await_work_staying(prefer, rank, 100).0;
                menu.set_theater(t.clone());
                for _ in 0..2000 {
                    g.step();
                    let (next, _) = g.await_work_staying(prefer, rank, 100);
                    let moved = |a: &Option<galaxy::theater::Theater>| {
                        a.as_ref()
                            .zip(next.as_ref())
                            .is_some_and(|(a, b)| kind.is(a, b))
                    };
                    if moved(&t) {
                        // As the app does back from a battle (or relaunched):
                        // a fresh menu, and the old contract from the profile.
                        let shown = t.as_ref().and_then(galaxy::theater::Shown::of);
                        menu = menu::Menu::new();
                        menu.input(menu::Nav::Select);
                        if let Some(old) = shown.and_then(|s| g.shown(&s, rank)) {
                            menu.remember(old);
                        }
                        menu.set_theater(next);
                        break;
                    }
                    // Another kind of move: start again from there.
                    if next.is_some() {
                        t = next.clone();
                        menu.set_theater(next);
                    }
                }
                profile.ship = g.crew;
                let kind = menu
                    .transfer()
                    .zip(menu.theater())
                    .is_some_and(|(tr, t)| kind.is(&tr.from, t));
                anyhow::ensure!(kind, "no transfer of that kind in 2000 ticks");
                for _ in 0..(shot.time / 0.01).round() as u32 {
                    menu.advance(0.01);
                }
            }
            if args.won {
                // As back from a won battle: settled, then the war's tick.
                let mut g = galaxy::Galaxy::warmed(galaxy::save::SEED);
                let rank = profile.rank().level;
                let t = g
                    .await_work_staying(prefer, rank, 100)
                    .0
                    .context("no work for your company")?;
                let o = t
                    .sortie(args.site.unwrap_or(0))
                    .context("your side doesn't fight there")?;
                let b = g
                    .muster_crew(o.system, &o.factions, Some(o.site))
                    .context("your company isn't in it")?;
                let stake = galaxy::battle::Stake::of(mission::MISSIONS[o.mission].contract);
                let s = g.settle(&b, &[0, 0], &[0, 0], Some(0), stake);
                g.step_played(b.system);
                log::info!("won: {}", galaxy::battle::describe(&b, &s));
                profile.ship = g.crew;
                menu.set_theater(g.aboard(rank));
            }
            if let Some(k) = args.site {
                menu.pick_site(k);
            }
            if args.screen == Some(Screen::Site) {
                menu.input(menu::Nav::Select);
            }
        }
        Some(Screen::Companies) => {
            let mut g = galaxy::Galaxy::warmed(galaxy::save::SEED);
            g.await_work_staying(prefer, profile.rank().level, 100);
            profile.ship = g.crew;
            menu.input(menu::Nav::Down);
            menu.input(menu::Nav::Select);
            menu.set_ships(g.other_ships(5));
            menu.input(menu::Nav::Down);
        }
        Some(Screen::Settings | Screen::Controls | Screen::Rebind) => {
            for _ in 0..3 {
                menu.input(menu::Nav::Down);
            }
            menu.input(menu::Nav::Select);
            if args.screen != Some(Screen::Settings) {
                // Up wraps round to CONTROLS.
                menu.input(menu::Nav::Up);
                menu.input(menu::Nav::Select);
            }
            if args.screen == Some(Screen::Rebind) {
                for _ in 0..binds::Action::Fire.index() {
                    menu.input(menu::Nav::Down);
                }
                menu.input(menu::Nav::Right);
                menu.input(menu::Nav::Select);
            }
        }
        _ => {}
    }
    // The backdrop follows the selected contract, as in the window.
    let (kind, map_seed, side) = menu
        .backdrop()
        .unwrap_or((mission::MISSIONS[0].map_kind, 0, 0));
    let mut game = app::backdrop_match(kind, map_seed, side, profile.loadout);
    let mut renderer = render::Renderer::new_headless(shot.width, shot.height)?;
    renderer.set_static_mesh(&scene::map_mesh(&game.world.map));
    let art = tank::TankArt::upload(&mut renderer);
    for _ in 0..(shot.time / game::DT).round() as u32 {
        game.step(sim::Command::default(), false);
    }
    let (w, h) = (shot.width as f32, shot.height as f32);
    let angle = vista::sway(garage::front_angle(&game.world.tanks[0]), shot.time);
    let tanks = game.display_tanks(1.0);
    let camera = vista::camera(&game.world, &tanks[0], angle, w / h);
    let mut frame = match menu.system() {
        // The missions screen: the system, in 3D.
        Some(t) => {
            let mut sys = theater_view::SystemArt::upload(&mut renderer);
            sys.show(&mut renderer, theater_view::on_screen(t, menu.transfer()));
            theater_view::scene(&sys, t, menu.transfer(), shot.time, w / h)
        }
        None => scene::build(&game.world, &tanks, &art, camera),
    };
    frame.ui = menu.draw(&profile, w, h, 1.0).verts;
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

/// The war ticked on until a company hired to hold one of its employer's
/// worlds (for side `prefer`, if given) has a battle for you, with you in
/// it: that theater, or None if there's none in 2000 ticks.
fn defending(
    g: &mut galaxy::Galaxy,
    prefer: Option<usize>,
    rank: u32,
) -> Option<galaxy::theater::Theater> {
    for _ in 0..2000 {
        let ids: Vec<u32> = g
            .companies
            .iter()
            .filter(|c| {
                c.contract
                    .is_some_and(|k| k.defend && prefer.is_none_or(|f| k.side as usize == f))
            })
            .map(|c| c.id)
            .collect();
        for id in ids {
            g.crew = Some(id);
            if let Some(t) = g.aboard(rank) {
                return Some(t);
            }
        }
        g.step();
    }
    None
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
