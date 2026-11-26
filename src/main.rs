mod ai;
mod ambience;
mod app;
mod arena;
mod arrival;
mod audio;
mod bay;
mod bench;
mod binds;
mod cluster_view;
mod combat;
mod conditions;
mod economy;
mod faction;
mod fleet_model;
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
mod work_view;

use anyhow::{Context, Result, bail};
use glam::Vec3;

const USAGE: &str = "\
tankit [OPTIONS]

  (no options)            open the main menu (missions, garage)
  any of --seed --map-seed --map --time-of-day --spawn --team-size --contract --employer --hull --turret skips the menu and drops straight into a match
  --seed <n>              match seed: bot brains, spawns, kits (default: random)
  --map <saltmarket|sanctum|kiln|city|shield-site|foundry|mine|desert|shield-desert>
                          Saltmarket (desert town, the default), the Sanctum (a Resonant church
                          on a mesa, the Hold map), the Kiln (Ithessar ground), the city
                          (a generated city, see --look), the shield site (the same, a shield
                          generator in place of its great building), a hull foundry
                          (generated: see --look and --ground), or the mine (a generated Ithessar
                          crystal mine: caves, rails and stacked levels), the desert (generated open
                          desert) or the shield desert (the same, a shield generator in its landmark's slot)
  --map-seed <n>          which one a generated map (city, shield-site, foundry, mine, desert, shield-desert) builds (default: the --seed)
  --time-of-day <day|dusk|night>  what the map's fought in (default: day, its own sky)
  --generator <faction>   (with --map shield-site or shield-desert) whose shield generator stands in it:
                          imperium (the default), resonant, uthenn or ithessar
  --generator-down <n>    (with --map shield-site or shield-desert) the generator's down, n ticks of repair
                          to go: 4 a wreck nobody's started on, 1 to 3 going back up (default 0: up)
  --look <imperium|resonant|uthenn|ithessar>
                          (with --map city, shield-site or foundry) who built it (default imperium;
                          uthenn and ithessar only have foundries of their own)
  --ground <desert|tundra|grassland>  (with --map foundry) what kind of world it's on (default desert)
  --in-desert             (with --map city or shield-site) the city stands in the desert
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
  --learning <0-1>        (with --preview) how far learning that part (or its next tier, if owned) is
  --own <part>            (with --shot) the pilot has bought this hull or turret (repeatable)
  --screen <menu|missions|companies|contract|settings|controls|rebind|pause|results>  (with --shot) render that screen
  --screen pause-settings (with --shot) the pause box with its settings open
  --screen pause --garage (with --shot) the garage opened from the pause box, the match frozen under it
  --debug                 (with --shot) debug mode on: FPS counter
  --screen tab            (with --shot) Tab held: the board
  --screen dead           (with --shot) the player is destroyed 3.5 s before the shot: the death
                          screen and its respawn countdown
  --screen results        (with --shot) end the round 4 s before the shot (0.3 s before the first of
                          --frames): scoreboard, renown gained, crystal tally
                          and (with --learned) a schematic reverse-engineered
  --screen returning      (with --shot) the results, with CONTRACT pressed: the loading panel over
                          them while the contract screen's made ready
  --screen kill           (with --shot) the player kills the nearest enemy, its gear at T2.5, half a
                          second before the shot: the scrap popups
  --screen carry          (with --shot --contract raid) the player starts on the enemy's flag,
                          facing home, and carries it
  --learned <part>        (with --screen results) the round's wrecks make this part's schematic
                          yours, e.g. 'pelekys'
  --scrap <part>          (with --screen results) the round's kills took apart a T2.5 wreck of this
                          part (repeatable), e.g. 'kataphrakt'
  --site <n>              (with --screen missions|site) select the theater's nth contested site
  --transfer <jump|move|renew|warp>
                          (with --screen missions) the war goes on until your company moves onto its
                          next contract (in another system, the same one, or the same contract
                          renewed), as if after a battle (a fresh menu, the old contract from the
                          profile): the shot is --time seconds into the transfer playing over the
                          system view (past the summary of the contract that ended); warp: you
                          switch company to the first other ship with work, and your hull warps
  --summary               (with --transfer) stop at the summary of how the old contract ended, before
                          the transfer plays
  --failed                (with --summary) wait for a contract that failed (else the first to end)
  --tab <n>               (with --screen companies|contract) the nth filter on: 1-4 work for, 5-8 fight, 9 take, 10 hold
  --won                   (with --screen missions) your company just won the battle at --site
                          (default the first it fights for), under that site's contract, and
                          the rest of the war ticked with it, as back from a battle
  --quiet <n>             (with --won) the war goes on n ticks more before the missions screen
  --waiting               (with --screen missions) your company's between contracts: the war
                          goes on until there's work, over the system you were last in
  --ticks                 (with --won) the tick summary instead, the screen before the missions
                          screen: what the war did to the system's hulls since the battle, and why
  --shields-down          (with --screen missions|site) every shield generator on the system's first
                          world is damaged, so its shields are down
  --own-hulls <n>         (with --front --hired) n of your company's hulls are your own: its bots
                          leave them, so their seats go over to the realm sooner
  --defend                (with --screen missions|site) the war goes on until your company is hired
                          to hold one of its employer's worlds, not to take one; (with --front
                          --hired) the battle is one where a company's hired to hold the world
                          (from 0; default: the first your side fights for)
  --renown <n>            (with --shot) Company renown (your rank) for menu, garage and pause shots
  --crystals <n>          (with --shot) pilot crystals for menu, garage and pause shots
  --upgrade <steps>       (with --shot) every part the pilot owns is this many upgrade steps up (10 a mark)
  --known <steps>         (with --shot) every part the pilot owns is known (buildable) to this many steps
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
    /// --time-of-day.
    pub conditions: conditions::Conditions,
    pub spawn: usize,
    pub bots: usize,
    pub contract: mission::Contract,
    /// Which of the map's sides hires you.
    pub side: usize,
    /// --employer, for menu shots.
    employer: Option<faction::Faction>,
    /// --learned, for results shots: a part whose schematic the round's
    /// wrecks make yours (#438).
    learned: Option<economy::Part>,
    /// --scrap, for results shots: parts whose wrecks the round's kills
    /// took apart, each a T2.5 wreck (#474).
    scrap: Vec<economy::Part>,
    /// --site, for theater shots: index into the contested sites.
    site: Option<usize>,
    /// --transfer, for missions shots: the move onto the next contract,
    /// `--time` seconds in: to another system (`Jump`), within the one
    /// it's in (`Move`), or the same contract renewed (`Renew`).
    transfer: Option<TransferKind>,
    /// --summary, with --transfer: screenshot the contract summary card
    /// instead of the transfer animation.
    summary: bool,
    /// --failed, with --summary: show a failed contract.
    failed: bool,
    /// --won, for missions shots: your company just won its battle.
    won: bool,
    /// --tab, for companies shots: the tab of who you'd fight (#454).
    tab: usize,
    /// --quiet, with --won: the galaxy ticks this many more times after the
    /// battle before the missions screen (#447).
    quiet: u32,
    /// --waiting, with --screen missions: between contracts.
    waiting: bool,
    /// --ticks, with --won: the tick summary screen (#475).
    ticks: bool,
    /// --shields-down, for missions shots: the first world's generators
    /// are all damaged (#477).
    shields_down: bool,
    /// --defend, for missions shots: your company's on a defense contract.
    defend: bool,
    /// --own-hulls, for battle shots: your own hulls out of your company's.
    own_hulls: Option<u32>,
    pub loadout: garage::Loadout,
    /// --hull or --turret was given.
    kit_given: bool,
    /// A match flag was given: skip the menu.
    play: bool,
    garage: bool,
    /// Parts the shot's pilot owns.
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
    /// Shots: upgrade steps known (buildable) on every owned part (#451).
    known: Option<u8>,
    /// Shots: how far learning the --preview part, or its next tier if
    /// owned, is (0..1, #483).
    learning: Option<f32>,
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
                if let Some(k) = self.known {
                    p.known.set(part, k.max(1) as f32);
                }
            }
        }
        if let (Some(part), Some(f)) = (self.preview, self.learning) {
            let base = match p.owns(part) {
                true => p.known.of(part).floor().max(p.levels.of(part) as f32),
                false => 0.0,
            };
            p.known.set(part, base + f.clamp(0.0, 0.99));
        }
        p
    }

    pub fn map(&self) -> map::Map {
        let mut m = match (self.greybox, self.map) {
            (true, MapKind::City(l, _)) => map::procgen::greybox(&city_spec(self.map_seed, l)),
            (true, MapKind::Mine) => map::procgen::greybox(&mine_spec(self.map_seed)),
            (true, MapKind::Foundry(l, g)) => {
                map::procgen::greybox(&foundry_spec(self.map_seed, l, g))
            }
            (true, MapKind::Desert) => map::procgen::greybox(&desert_spec(self.map_seed)),
            (true, MapKind::ShieldSite(l, _, g)) => {
                map::procgen::greybox(&shield_spec(self.map_seed, l, g))
            }
            (true, MapKind::ShieldDesert(g)) => {
                map::procgen::greybox(&shield_desert_spec(self.map_seed, g))
            }
            _ => make_map(self.map, self.map_seed),
        };
        self.conditions.apply(&mut m);
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
    /// A city in someone's style, half ruined round its great building:
    /// two sides. Generated: a different city for each map seed.
    City(map::procgen::Look, map::procgen::setting::Setting),
    /// The same city, a shield generator in the middle in place of its great
    /// building (#341): its holder's standard model, in the shape it's in.
    /// The same layout for the same seed.
    ShieldSite(
        map::procgen::Look,
        map::procgen::setting::Setting,
        Generator,
    ),
    /// A hull foundry, in someone's style on some kind of ground.
    /// Generated: a different one for each map seed.
    Foundry(map::procgen::Look, map::procgen::foundry::Ground),
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
        MapKind::City(l, s) => map::procgen::map(&city_spec(seed, l).set_in(s)),
        MapKind::ShieldSite(l, s, g) => map::procgen::map(&shield_spec(seed, l, g).set_in(s)),
        MapKind::Foundry(l, g) => map::procgen::map(&foundry_spec(seed, l, g)),
        MapKind::Mine => map::procgen::map(&mine_spec(seed)),
        MapKind::Desert => map::procgen::map(&desert_spec(seed)),
        MapKind::ShieldDesert(g) => map::procgen::map(&shield_desert_spec(seed, g)),
    }
}

/// The city `seed` names in `look`: the default matchup (which a theater's
/// sides stand in for), in opposite corners.
pub fn city_spec(seed: u64, look: map::procgen::Look) -> map::procgen::Spec {
    map::procgen::Spec {
        seed,
        biome: map::procgen::Biome::City,
        look,
        setting: Default::default(),
        sides: map::DEFAULT_SIDES.to_vec(),
        centerpiece: map::procgen::Centerpiece::Monument,
    }
}

/// The imperial city `seed` names, `owner`'s shield generator in its
/// middle (whoever holds the city now).
pub fn shield_city_spec(seed: u64, owner: faction::Faction) -> map::procgen::Spec {
    shield_spec(seed, map::procgen::Look::Imperium, Generator::up(owner))
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

/// The city `seed` names in `look` with generator `g` in its middle.
pub fn shield_spec(seed: u64, look: map::procgen::Look, g: Generator) -> map::procgen::Spec {
    map::procgen::Spec {
        centerpiece: map::procgen::Centerpiece::ShieldGenerator(g.owner, g.condition),
        ..city_spec(seed, look)
    }
}

/// The Ithessar crystal mine `seed` names: the Ithessar in the west, the
/// Resonant in the east.
pub fn mine_spec(seed: u64) -> map::procgen::Spec {
    map::procgen::Spec {
        seed,
        biome: map::procgen::Biome::Mine,
        setting: Default::default(),
        look: map::procgen::Look::Ithessar,
        sides: vec![faction::Faction::Ithessar, faction::Faction::Resonant],
        centerpiece: map::procgen::Centerpiece::Monument,
    }
}

/// The hull foundry `seed` names in `look` on `ground`: the default matchup.
pub fn foundry_spec(
    seed: u64,
    look: map::procgen::Look,
    ground: map::procgen::foundry::Ground,
) -> map::procgen::Spec {
    map::procgen::Spec {
        seed,
        biome: map::procgen::Biome::Foundry(ground),
        look,
        sides: map::DEFAULT_SIDES.to_vec(),
        centerpiece: map::procgen::Centerpiece::Monument,
        setting: Default::default(),
    }
}

/// The open desert `seed` names: the default matchup.
pub fn desert_spec(seed: u64) -> map::procgen::Spec {
    map::procgen::Spec {
        seed,
        biome: map::procgen::Biome::Desert,
        // Nobody's: the look only matters once something's built there.
        setting: Default::default(),
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
    /// Switching company (#449): your hull warps to another ship.
    Warp,
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
            TransferKind::Warp => sa.id != sb.id,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    /// Tab held: the board.
    Tab,
    Menu,
    Missions,
    /// A contested site of the theater.
    Site,
    Settings,
    /// Settings' key bindings.
    Controls,
    /// The other companies' ships to switch to (#365).
    Companies,
    /// One of those ships' contract, previewed before you sign on (#453).
    Contract,
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
    /// Left the battle from the pause box: it plays out without you, under
    /// a progress bar (#427).
    Leaving,
    /// The round's over and CONTRACT's been pressed: the loading panel
    /// over it while the contract screen's made ready (#452).
    Returning,
    /// A kill of yours half a second before the shot, its wreck's gear well
    /// past what you know: the scrap popups (#444).
    Kill,
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
                        | Screen::Contract
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
            conditions: args.conditions,
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
    let mut conditions = conditions::Conditions::default();
    let mut look: Option<map::procgen::Look> = None;
    let mut ground: Option<map::procgen::foundry::Ground> = None;
    let mut in_desert = false;
    let mut generator_down = 0u8;
    let mut learned = None;
    let mut scrap = Vec::new();
    let mut site = None;
    let mut transfer = None;
    let mut summary = false;
    let mut waiting = false;
    let mut failed = false;
    let mut won = false;
    let mut shields_down = false;
    let mut tab = 0;
    let mut quiet = 0;
    let mut ticks = false;
    let mut defend = false;
    let mut own_hulls: Option<u32> = None;
    let mut loadout = (sim::HullKind::Kataphrakt, sim::TurretKind::Kontos);
    let mut garage = false;
    let mut own = Vec::new();
    let mut preview = None;
    let (mut kit_given, mut play) = (false, false);
    let mut screen = None;
    let (mut renown, mut crystals, mut earned) = (0, 0, 0);
    let mut upgrade = 0;
    let mut known = None;
    let mut learning = None;
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
                | "--time-of-day"
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
            "--time-of-day" => {
                let name = args.next().unwrap_or_default();
                conditions.time = conditions::TimeOfDay::by_name(&name).with_context(|| {
                    format!("--time-of-day expects day, dusk or night, got {name:?}")
                })?;
            }
            "--greybox" => greybox = true,
            "--no-lod" => render::lod::disable(),
            "--map" => match args.next().as_deref() {
                Some("saltmarket") => map_kind = MapKind::Saltmarket,
                Some("sanctum") => map_kind = MapKind::Sanctum,
                Some("kiln") => map_kind = MapKind::Kiln,
                Some("city") => {
                    map_kind = MapKind::City(map::procgen::Look::Imperium, Default::default())
                }
                Some("shield-site") => {
                    map_kind = MapKind::ShieldSite(
                        map::procgen::Look::Imperium,
                        Default::default(),
                        Generator::up(faction::Faction::Imperium),
                    )
                }
                Some("foundry") => {
                    map_kind = MapKind::Foundry(
                        map::procgen::Look::Imperium,
                        map::procgen::foundry::Ground::Desert,
                    )
                }
                Some("mine") => map_kind = MapKind::Mine,
                Some("desert") => map_kind = MapKind::Desert,
                Some("shield-desert") => {
                    map_kind = MapKind::ShieldDesert(Generator::up(faction::Faction::Imperium))
                }
                other => {
                    bail!(
                        "--map expects saltmarket, sanctum, kiln, city, shield-site, foundry, mine, desert or shield-desert, got {other:?}"
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
            "--in-desert" => in_desert = true,
            "--look" => {
                look = Some(match args.next().as_deref() {
                    Some("imperium") => map::procgen::Look::Imperium,
                    Some("resonant") => map::procgen::Look::Resonant,
                    Some("uthenn") => map::procgen::Look::Uthenn,
                    Some("ithessar") => map::procgen::Look::Ithessar,
                    other => {
                        bail!(
                            "--look expects imperium, resonant, uthenn or ithessar, got {other:?}"
                        )
                    }
                })
            }
            "--ground" => {
                use map::procgen::foundry::Ground;
                ground = Some(match args.next().as_deref() {
                    Some("desert") => Ground::Desert,
                    Some("tundra") => Ground::Tundra,
                    Some("grassland") => Ground::Grassland,
                    other => {
                        bail!("--ground expects desert, tundra or grassland, got {other:?}")
                    }
                })
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
            "--learned" => {
                let name = args.next().unwrap_or_default();
                learned = Some(
                    economy::Part::by_name(&name)
                        .with_context(|| format!("--learned: no part called {name:?}"))?,
                );
            }
            "--scrap" => {
                let name = args.next().unwrap_or_default();
                scrap.push(
                    economy::Part::by_name(&name)
                        .with_context(|| format!("--scrap: no part called {name:?}"))?,
                );
            }
            "--site" => site = Some(args.next().context("--site needs a number")?.parse()?),
            "--transfer" => {
                transfer = Some(match args.next().as_deref() {
                    Some("jump") => TransferKind::Jump,
                    Some("warp") => TransferKind::Warp,
                    Some("move") => TransferKind::Move,
                    Some("renew") => TransferKind::Renew,
                    other => bail!("--transfer expects jump, move, renew or warp, got {other:?}"),
                })
            }
            "--summary" => summary = true,
            "--failed" => failed = true,
            "--won" => won = true,
            "--shields-down" => shields_down = true,
            "--tab" => tab = args.next().context("--tab needs a number")?.parse()?,
            "--quiet" => quiet = args.next().context("--quiet needs a number")?.parse()?,
            "--ticks" => ticks = true,
            "--waiting" => waiting = true,
            "--defend" => defend = true,
            "--own-hulls" => {
                own_hulls = Some(args.next().context("--own-hulls needs a number")?.parse()?)
            }
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
            "--judge" => {
                // --judge <ticks> <seed,seed..> [arms|replay]
                let ticks = args.next().context("--judge needs a tick count")?.parse()?;
                let seeds: Vec<u64> = args
                    .next()
                    .context("--judge needs seeds")?
                    .split(',')
                    .map(|x| x.parse())
                    .collect::<Result<_, _>>()?;
                match args.next().as_deref() {
                    Some("replay") => galaxy::judge::replay(&seeds),
                    arms => galaxy::judge::run(ticks, &seeds, arms.is_some()),
                }
                std::process::exit(0);
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
                    Some("contract") => Screen::Contract,
                    Some("rebind") => Screen::Rebind,
                    Some("pause") => Screen::Pause,
                    Some("pause-settings") => Screen::PauseSettings,
                    Some("tab") => Screen::Tab,
                    Some("results") => Screen::Results,
                    Some("dead") => Screen::Dead,
                    Some("spent") => Screen::Spent,
                    Some("spent-over") => Screen::SpentOver,
                    Some("leaving") => Screen::Leaving,
                    Some("returning") => Screen::Returning,
                    Some("kill") => Screen::Kill,
                    Some("carry") => Screen::Carry,
                    other => {
                        bail!(
                            "--screen expects menu, missions, site, settings, controls, rebind, pause, results, dead, spent, spent-over, leaving, returning, kill or carry, got {other:?}"
                        )
                    }
                })
            }
            "--renown" => renown = args.next().context("--renown needs a number")?.parse()?,
            "--crystals" => crystals = args.next().context("--crystals needs a number")?.parse()?,
            "--upgrade" => upgrade = args.next().context("--upgrade needs a number")?.parse()?,
            "--known" => known = Some(args.next().context("--known needs a number")?.parse()?),
            "--learning" => {
                learning = Some(args.next().context("--learning needs a number")?.parse()?)
            }
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
    if let (MapKind::Foundry(_, g), Some(ground)) = (&mut map_kind, ground) {
        *g = ground;
    }
    if let (MapKind::City(_, s) | MapKind::ShieldSite(_, s, _), true) = (&mut map_kind, in_desert) {
        *s = map::procgen::setting::Setting::Desert;
    }
    if let (
        MapKind::City(l, _) | MapKind::ShieldSite(l, _, _) | MapKind::Foundry(l, _),
        Some(look),
    ) = (&mut map_kind, look)
    {
        *l = look;
    }
    if let MapKind::ShieldSite(_, _, g) | MapKind::ShieldDesert(g) = &mut map_kind {
        if let Some(owner) = generator {
            g.owner = owner;
        }
        // (The galaxy's generators are only working or damaged, #422; a
        // shot can still show any stage of the art.)
        g.condition = map::procgen::shield::Condition::down(generator_down, 4);
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
                | Screen::Contract
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
        learned,
        scrap,
        site,
        transfer,
        summary,
        waiting,
        failed,
        won,
        shields_down,
        tab,
        quiet,
        ticks,
        map: map_kind,
        conditions,
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
        known,
        learning,
        debug,
        straight,
        defend,
        own_hulls,
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
        game.set_own_hulls(args.own_hulls.unwrap_or(g.own_left()));
        profile.ship = g.crew;
        game.crew = profile.company_name();
    }
    if let Some(h) = &args.hulls {
        game.set_allocation(h);
    }
    // All set up: the round starts, its tanks fielded.
    game.start();
    if matches!(args.screen, Some(Screen::Results | Screen::Returning)) {
        // A still shows the results settled; a clip starts as they begin to count.
        let lead = if shot.frames > 1 { 0.3 } else { 4.0 };
        game.time_left = (shot.time - lead).max(0.1);
        game.renown = args.earned;
        if let Some(part) = args.learned {
            // One wreck short of its first step: this round's kill makes it yours.
            let mut known = profile.known;
            known.set(part, 0.99);
            game.known = Some(known);
            game.fighters[0].scrap.push((part, economy::MAX_LEVEL));
        }
        if !args.scrap.is_empty() {
            game.known.get_or_insert(profile.known);
            for &part in &args.scrap {
                game.fighters[0].scrap.push((part, 25));
            }
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
        garage.camera(camera, &m.world, &m.world.tanks[0])
    };
    let mut camera = if matches!(args.screen, Some(Screen::Spent | Screen::SpentOver)) {
        // Your company's last hull goes, with you in it, and a respawn's
        // wait later you're out: the shot catches the fast-forward, or
        // (spent-over) a few seconds of the results it ends on.
        let lead = (combat::RESPAWN_TIME + 1.0).min(shot.time);
        advance(&mut game, &mut cam, &mut garage, shot.time - lead);
        let me = game.fighters[0].team;
        game.set_company_hulls(me, 0, 0);
        game.own_hulls = 0;
        game.destroy_player();
        let camera = advance(&mut game, &mut cam, &mut garage, lead);
        if args.screen == Some(Screen::SpentOver) {
            game.fast_forward(usize::MAX);
            advance(&mut game, &mut cam, &mut garage, 3.0)
        } else {
            camera
        }
    } else if args.screen == Some(Screen::Leaving) {
        // Left at the shot's time; caught some way into the fast-forward.
        let camera = advance(&mut game, &mut cam, &mut garage, shot.time);
        game.leave();
        let to = (game.progress() + 0.4).min(0.9);
        while game.playing_out() && game.progress() < to {
            game.fast_forward(60);
        }
        camera
    } else if args.screen == Some(Screen::Kill) {
        // Your kill half a second before the shot, its wreck built to T2.5.
        let lead = 0.5f32.min(shot.time);
        advance(&mut game, &mut cam, &mut garage, shot.time - lead);
        game.known = Some(profile.known);
        let me = game.fighters[0].team;
        for f in game.fighters.iter_mut().filter(|f| f.team != me) {
            f.levels = economy::Levels::all(25);
        }
        game.kill_nearest_enemy()
            .context("--screen kill needs an enemy in play")?;
        advance(&mut game, &mut cam, &mut garage, lead)
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
            // A stand-in for what the battle did to the war: you attacked a
            // foundry, and took it if you won.
            if game.phase == combat::Phase::Over && hud.debrief.is_none() {
                hud.debrief = Some(hud::WarResult {
                    world: "KESSARA II".to_string(),
                    site: "HULL FOUNDRY",
                    verdict: hud::Verdict::of(false, game.leader() == Some(0)),
                });
            }
            let ui = hud
                .draw(
                    &game,
                    &tanks,
                    &camera,
                    w,
                    h,
                    game::DT,
                    args.screen == Some(Screen::Tab),
                    hud::Ledger::before(&profile, &game, 0, false),
                )
                .verts;
            // Paused, the box is under the garage (opened from it, with --garage), as in the game.
            let ui = if args.screen == Some(Screen::Leaving) {
                let mut ui = menu::dim(ui);
                ui.extend(hud::playing_out(&game, w, h));
                ui
            } else if args.screen == Some(Screen::Returning) {
                let mut ui = menu::dim(ui);
                ui.extend(hud::loading("ROUND OVER", "LOADING", 0.4, w, h));
                ui
            } else if matches!(args.screen, Some(Screen::Pause | Screen::PauseSettings)) {
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

/// The main menu or mission select over the menu backdrop, headless.
fn menu_shot(args: &Args, shot: &ShotArgs) -> Result<()> {
    let mut profile = args.profile();
    profile.loadout = args.loadout;
    let mut menu = menu::Menu::new();
    // --ticks: the tick summary, over the missions screen's system.
    let mut summary = None;
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
        None | Some(Screen::Menu) => {
            let mut g = galaxy::Galaxy::warmed(galaxy::save::SEED);
            g.await_work_staying(prefer, profile.rank().level, 100);
            profile.ship = g.crew;
            menu.set_cluster(cluster_view::Cluster::of(&g));
            menu.advance(shot.time);
        }
        Some(Screen::Missions | Screen::Site) => {
            menu.input(menu::Nav::Select);
            let (crew, t) = war();
            profile.ship = crew;
            menu.set_theater(t);
            if args.waiting {
                // Your company's between contracts: the war's gone on a while
                // with nothing for you, over the system you were last in.
                menu.set_theater(None);
                menu.waited(37);
            }
            if let Some(kind) = args.transfer {
                let mut g = galaxy::Galaxy::warmed(galaxy::save::SEED);
                let rank = profile.rank().level;
                let mut t = g.await_work_staying(prefer, rank, 100).0;
                menu.set_theater(t.clone());
                if kind == TransferKind::Warp {
                    // As the app does on SIGN ON: switch to the first other
                    // ship with a battle for you, then on to its theater.
                    let id = g
                        .open_work(rank)
                        .iter()
                        .find(|o| o.flight == 0 && Some(o.id) != g.crew)
                        .map(|o| o.id)
                        .context("no other ship with work to sign on with")?;
                    anyhow::ensure!(g.sign_on(id), "couldn't sign on with ship {id}");
                    menu.switched();
                    menu.set_theater(g.aboard(rank));
                }
                // A warp's already set going; else wait for a move of that kind.
                let ticks = if kind == TransferKind::Warp { 0 } else { 2000 };
                for _ in 0..ticks {
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
                        if let Some(old) = shown.and_then(|s| g.shown(&s)) {
                            menu.remember(old);
                        }
                        menu.set_theater(next.clone());
                        let won = menu.summary().is_none_or(|e| e.outcome.won());
                        if !(args.failed && won) {
                            break;
                        }
                    }
                    // Another kind of move: start again from there.
                    if next.is_some() {
                        t = next.clone();
                        menu.set_theater(next);
                    }
                }
                profile.ship = g.crew;
                let kind = menu
                    .next_move()
                    .zip(menu.theater())
                    .is_some_and(|(tr, t)| kind.is(&tr.from, t));
                anyhow::ensure!(kind, "no transfer of that kind in 2000 ticks");
                if args.summary {
                    anyhow::ensure!(
                        menu.summary().is_some(),
                        "no summary: the contract didn't end"
                    );
                    anyhow::ensure!(
                        !(args.failed && menu.summary().is_some_and(|e| e.outcome.won())),
                        "no failed contract in 2000 ticks"
                    );
                } else {
                    menu.go_on();
                }
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
                let (fought_at, owners) = (g.tick, g.owners());
                let s = g.step_played(&galaxy::battle::Played {
                    battle: b.clone(),
                    lost: vec![0, 0],
                    company: vec![vec![], vec![]],
                    winner: Some(0),
                    own_lost: 0,
                    scrap: Vec::new(),
                });
                log::info!("won: {}", galaxy::battle::describe(&b, &s));
                // As the app does: the summary's of the battle's tick alone.
                if args.ticks {
                    summary = Some(g.tick_summary(b.system, fought_at, &owners));
                }
                for _ in 0..args.quiet {
                    g.step();
                }
                // As the app does: the war goes on until there's work (#447).
                let (t, _) = g.await_work_staying(prefer, rank, 100);
                profile.ship = g.crew;
                menu.set_theater(t);
            }
            if args.shields_down
                && let Some(mut t) = menu.theater().cloned()
            {
                for s in t
                    .sites
                    .iter_mut()
                    .filter(|s| s.planet == 0 && s.is_shield())
                {
                    s.kind = galaxy::SiteKind::Shield { damaged: true };
                }
                menu.set_theater(Some(t));
            }
            if let Some(k) = args.site {
                menu.pick_site(k);
            }
            if args.screen == Some(Screen::Site) {
                menu.input(menu::Nav::Select);
            }
        }
        Some(Screen::Companies | Screen::Contract) => {
            let mut g = galaxy::Galaxy::warmed(galaxy::save::SEED);
            // (on till there's a board worth the name)
            while g.open_work(1).len() < 8 {
                g.step();
            }
            g.await_work_staying(prefer, profile.rank().level, 100);
            profile.ship = g.crew;
            menu.input(menu::Nav::Down);
            menu.input(menu::Nav::Select);
            menu.set_cluster(cluster_view::Cluster::of(&g));
            let rank = profile.rank().level;
            let mine = g.my_work(rank);
            menu.board.set(g.open_work(rank), mine);
            if args.tab > 0 {
                menu.board.toggle(args.tab - 1);
            }
            menu.advance(shot.time);
            if args.screen == Some(Screen::Contract) {
                menu.input(menu::Nav::Select);
            }
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
    let backdrop = menu.backdrop().unwrap_or(app::MENU_BACKDROP);
    let mut game = app::backdrop_match(backdrop, profile.loadout);
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
        None => match menu.cluster() {
            Some((c, time)) => cluster_view::scene(c, time, w / h),
            None => match menu.deal() {
                Some((o, time)) => {
                    let kit = profile.loadout;
                    work_view::scene(&art, &tanks[0], kit, o, time, w / h)
                }
                None => scene::build(&game.world, &tanks, &art, camera),
            },
        },
    };
    frame.ui = match &summary {
        Some(t) => {
            let state = hud::SummaryState {
                loading: false,
                going: false,
                time: 0.0,
                hover: None,
            };
            hud::tick_summary(t, state, w, h).verts
        }
        None => menu.draw(&profile, w, h, 1.0).verts,
    };
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
