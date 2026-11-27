# tankit

Single-player arcade tank arena shooter: fast tanks, hand-built and
procedural maps, bots, and a mercenary progression loop: one Company rank, schematics stolen
on missions, and crystals for upgrades.

Setting: [docs/setting.md](docs/setting.md). UI design system: [docs/design.md](docs/design.md).

Plain Rust: `winit` + `wgpu`, no engine.

This README says what each part is and where it lives. Numbers, lists and
tuning (maps, missions, damage, renown, timings) live in the code, and the code is
the reference for them.

## Run

```sh
cargo run --release              # main menu
cargo run --release -- --help    # every flag: maps, loadouts, headless screenshots, showroom
cargo test
```

Any match flag (`--map`, `--seed`, `--team-size`, ...) skips the menu and drops
straight into a round. `--shot` renders headless to a PNG instead of opening a
window, and `--screen` does the same for a menu screen. `--wav` records a
round's sound headless to a WAV and prints its levels.
`--bench <seconds>` plays a round headless at 60 frames a second and prints
what each part of the frame costs (sim, scene, HUD, each GPU pass, sound),
then its slowest frames part by part; `--size` sets the resolution, `--map` the map.
Distant things are drawn with fewer triangles (level of detail, `src/render/lod.rs`):
each mesh and each 24 m cell of the map gets coarser copies, and the renderer
draws the coarsest that strays under half a pixel from the full mesh.
`--no-lod` draws everything at full detail, to compare against (with `--bench`
or `--shot`).
In a window, a frame that takes over 50 ms (or a gap of over 100 ms between
frames, or a wait on the window's next image) is logged as a `hitch` warning
saying where the time went, with the machine's memory and disk pressure, and
appended to `hitches.log` next to the profile.
`--arena <rounds>` plays bot-only rounds headless and prints each side's wins
and each gun's kills and deaths, and what each round's sim cost a step: every side runs the old bots (`src/ai/baseline.rs`)
except the one `--improve <faction>` names, which runs the current ones.
`--duel` does the same for one bot against one on open ground.

## Controls

Tanki-style: the camera rides behind the turret, and the turret is turned with keys.
These are the defaults: every one can be rebound under Settings → Controls
(in the main menu or the pause menu), two keys per control, and saved with the
profile. Esc, Enter and Backspace always work the menus, and the arrow keys
always move through menus and the garage.

| Key | Action |
|---|---|
| W/S or ↑/↓ | throttle / reverse |
| A/D or ←/→ | turn hull |
| Z / X | rotate turret left / right (it turns with the hull) |
| C | swing turret back to center |
| Space | fire (hold to keep firing) |
| Q / E (or PageUp / PageDown) | tilt the camera up / down |
| R | reset the camera tilt |
| Tab | scoreboard |
| G | garage |
| 1 / 2 | quick-cycle the hull / turret for the next respawn |
| Esc | pause; on from the debrief once the round is over |
| Enter | on from the debrief once the round is over |

Menus and the garage list their own keys on screen.

## How it fits together

| Part | What it is | Code |
|---|---|---|
| Game loop | Contract screen → deploy (the map loads on a worker thread) → battle → debrief → the tick's summary → the contract screen. There, in order: how your contract ended, if it did; waiting for work while there's none (the war ticks on screen); the ship's move to the next contract; the next contract. Leaving a battle from the pause menu lets it play out without you, then goes on to its debrief. | `src/app.rs`, `src/menu.rs` |
| Menus | Main menu (over a slowly turning view of the galaxy from your ship), employer and mission select, settings and pause, drawn in the hairline style. The contract page shows the system you're fighting for in 3D, your side's fights there to pick from, the enemy's gear, what a win pays and your hulls left; DEPLOY goes straight in. Find work is a table of the contracts on offer, filtered by who you'd work for, who you'd fight and the job; one opened is the deal: the two sides' tanks facing off, the pay, and the contract you'd leave. The attackers' fleets are in orbit and the world glows its holder's color, so whose side you're on is whose fleet you fly with, or whose world your company's on. | `src/menu.rs`, `src/cluster_view.rs`, `src/work_view.rs`, `src/ui/design.rs`, `src/app.rs` |
| Missions | Dead Town is Attrition and Low Water Hold in a reservoir town (generated: a small desert settlement below a broken dam and its dry lake bed, or round a well), Holy Ground is Hold on the Sanctum (generated: a Resonant church on a mesa, on desert, tundra or grassland by its world; seed 0 is the hand-made one), Cold Iron is Attrition and Hot Metal is Hold in a hull foundry (generated, in its builder's style, on desert, tundra or grassland by its world), Deep Vein is Attrition for the Ithessar against the Resonant in a crystal mine (generated, a new one each sortie, on desert, tundra or grassland by its world: a mine dug into a cliff, on levels stacked over each other: a mine under the mesa's top, a rail bridge out of the adit over a dry wash; in the mine its builders' borer (the Ithessar's plasma, the Imperium's thermal lance, the Resonant's choir, the Uthenn's light) holds its beam on the rock, and driving into it destroys you; a site's mine is in its founder's style). Stolen Flag is Raid in a crystal mine, for either side. Dead Line is Attrition and Night Freight is Raid in a rail depot (generated, in its builder's style, on desert, tundra or grassland by its world: a freight yard, lanes between rows of standing cars, gantries over them). Dead Weight is Attrition and Full Count Hold in a tungsten hoard (generated, the Uthenn's, on desert, tundra or grassland by its world: stacks of cubes on a grid round a raised counting floor; an outpost is one where its world has a lot of cubes stashed). Dry Wind is Hold and Long Shadows Attrition on the open desert (generated, in four forms: an erg, canyonlands, a basin, an escarpment). Broken Streets is Attrition and High Seat Hold in a city (generated, in its founder's style, on desert, tundra or grassland by its world). Shield Wall is Hold for a shield generator in a city, Bare Sky the same out in open land round its camp (each generated, on desert, tundra or grassland by its world). Any of a map's sides can hire you: you pick the employer, then one of their contracts. Most contracts name a schematic, stolen from a side you fight there, as the prize for a win. `--contract` and `--employer` pick them outside the menu. | `src/mission.rs` |
| Pilot profile | Renown with the Company (and the numbered, titled rank it adds up to), crystals, stolen schematics and their upgrades, and the kit you deploy in. Autosaved as JSON in the OS data directory (`~/.local/share/tankit/` on Linux); delete it to start over. | `src/profile.rs` |
| Deathmatch | Timed rounds, every tank for itself. Covers the weapons (shells, bouncing plasma, the long-range beam that overheats on the tank's thermal mass, flame), kills, renown with the Company (paid on the kill), the Vigil running on as a husk after it's broken, respawns (10 s dead, then a spawn shield), and vertical auto-aim. | `src/combat.rs` |
| Arrivals | How a tank arrives when it spawns or respawns, its team's way: the Imperium drop it down a lance from orbit onto their emblem, the Resonant kindle a dark vessel with gathering crystal light and a toll, the Uthenn print it under a green scan plane as tungsten cubes drop in, the Ithessar lower it out of a plasma orb. Purely looks, over inside the spawn shield. | `src/arrival.rs` |
| Hold | King of the hill, side against side: secure the crystal deposit. Only your side in the ring banks time for it; both sides in it contest it. First side to the target wins, otherwise the side with the most time held when the clock runs out. Each tank's own time on the site orders the scoreboard and pays the HOLD line. Bots make for the site and fight whoever comes near it. | `src/hold.rs` |
| Raid | One-sided capture the flag, attack and defend, side against side. Each side has a stand at its base. A round, one side defends its flag on its stand; the other takes it and carries it to its own stand. Killing the carrier drops the flag: an attacker picks it up again, a defender who touches it sends it home, and it goes home by itself if nobody touches it for a while. The attackers win a round by capturing; the defenders win it by holding out until the clock runs out. Sides swap each round, everyone back at base. After four rounds the side with more captures wins, and with equal captures the one whose captures took less time in all. The carrier drives a little slower, can still fire, and is marked to everyone. Bots play both sides: attackers go for the flag and escort its carrier, the carrier runs for home, defenders guard the stand and hunt the carrier down. | `src/raid.rs` |
| Repair drops | Crates parachute onto fixed drop zones on a timer and mend part of a damaged tank's hull. | `src/repair.rs` |
| Garage | Pick the hull and turret you spawn in next, and upgrade parts, from the menu or mid-battle. Parts are listed under their maker, each pictured alone with how far along it is; the one picked has its stats and what you can do with it in a card. It opens in the bay: a hangar aboard a Company ship in orbit, the tank turning on a turntable under a roof viewport, a long viewport behind it onto a planet (generated from a seed, `src/planet.rs`) and the stars, cut to through a dip to black. | `src/garage.rs`, `src/bay.rs`, `src/planet.rs` |
| Crystals | What a finished contract pays (completion, placement, contribution, kill bounty; the round-over screen shows it as one tally), and upgrade levels with their cost and stat bonus. | `src/economy.rs` |
| Bots | A* over a nav grid built from the map, driving with the same `Command` as the player (no physics shortcuts). Where a deck or a cave's roof stands over the ground, the grid has the floor under it too, and routes keep to their level (over the bridge or under it, through the mine or over the mesa above it), clear of drops. Making for a fight, bots often take one of the map's ways round, so a side spreads over every level. Each bot fights the way its gun wants (ranged guns hold their distance and circle, the flamethrower drives in nose-on), leads its shots, waits for a rocking gun to come level, banks plasma off a wall at a tank out of sight, and fetches repair crates when hurt. The bots as they were before are kept, frozen, as the yardstick `--arena` measures against. | `src/ai/`, `src/arena.rs` |
| Maps | Each module's doc comment describes its map. Generated maps (the imperial city, the crystal mine, the open desert and the reservoir town, a new one each sortie) are built in three steps: a **biome** lays out a **skeleton** of placeholders that knows nothing of whose map it is (a city's: streets, lots, the landmark, bases, gates, decos, the edge; a mine's, laid out in layers: landforms (one to three mines in an escarpment, each with two mouths a good way apart joined by a drift, a dry wash more often than not, buttes), the railroads bridging them, ramps and crossings stitching them together, the sides' spawns placed far apart with a mine on the way between them, the arena sized to fit and turned any way), the skeleton is checked for play on its greybox (`--greybox` shows it), and a **style** fills it in (the Imperium's rotunda, arches, statues and blocks; a mine's lamps, borer and freight, in any of the four factions' styles). Tests build their own ground (a walled yard plus kickers, bumps, stairs and hills) from `src/map/test_kit.rs`, which never ships. | `src/map.rs`, `src/map/`, `src/map/procgen/` |
| Tank kit | Procedural hulls and turrets that mix freely (yours painted over in the Company's amber), built from beveled plates, with tread links, wheels, recoil and suspension all animated. Surface detail is in the shader. The Ithessar kit is its own: knapped crystal, a hover hull (Sethuun) with gimballing emitters, and turrets that charge an orb between claws (Ammeth) or spin up crystal needles (Ossrin). The Resonant hull (Vigil) is a narrow keel with its plates floating on shafts of light, its crystal turning under a gilt bell arch, and a swinging bell. The Uthenn hull (Gravis) is the biggest: a pale shell over seven road wheels a side with a tall raked citadel stepped up on it, green seams, and radiator fins on the rear deck. | `src/tank/`, `src/tank/ithessar.rs`, `src/tank/resonant.rs`, `src/tank/uthenn.rs`, `src/shaders/scene.wgsl` |
| Physics | rapier3d rigid bodies plus a track model: spring rays per road wheel carry the weight, and every track contact pushes along the belt, limited by friction times load. Climbing, pivot turns and skid steering all come out of that. The hover hull swaps the tracks for six plasma springs: thrust, braking and sideways grip all push through the cushion, so it's fast, drifts through turns and glides over low ledges. | `src/sim.rs` |
| Conditions | A layer over any map: the time of day (day, the map's own sky; dusk; night, by moon and stars), and the weather on top of it (clear, or on desert ground a sandstorm: nobody makes out a tank more than 65 m off, and bots' sight, aim assist and enemies' health bars stop there too). It makes over the sky, light, haze and wind only, so a map's layout and cover stay as they are. A site's come of its map seed, so it keeps them each visit; `--time-of-day` and `--weather` pick them outside the menu. | `src/conditions.rs` |
| HUD | Hairline readouts, kill feed, name tags, renown popups, a death screen counting down to the respawn, the scoreboard (Tab), and at round over the same board opened up round its tables: victory, defeat or draw and what became of the site above, and below your rank and crystals going up and the parts you've got further with. Text is Inter Medium (SIL Open Font License, `assets/fonts/`; the main menu's is Michroma), drawn from a distance-field atlas built at startup so it stays smooth at any size. | `src/hud.rs`, `src/ui/`, `src/ui/font.rs` |
| Sound | All synthesized at runtime, no sample files. Each tank's engine, track clatter and turret motor follow its physics; each weapon has its own shot (the beam hums and the flamer roars while held); impacts, armour hits, kills, repairs and menus have one-shots, and under Hold the site being taken, lost or contested and each payout for holding it have their own cues. Each map's places sound as you near them, ducking under the fight: the Ithessar's crystals sing and their cores crackle, the Foundry's crucible roars and its presses clank, the Sanctum's altar is a choir and its bell stirs, the imperial city's banners flap and its dome moans, the reservoir town's cables sing, its stall cloths snap and its boats creak. A horn starts the round, the clock ticks its last ten seconds (harder for the last three), and a horn ends it with a sting for a win, a draw or a loss. Placed by distance and side from the camera. Volume is on the settings panel. Output via `cpal` (ALSA on Linux: building needs `libasound2-dev`). | `src/audio/` |

## Screenshots

`docs/screenshots/` holds renders from `--shot`.

![main menu](docs/screenshots/menu.png)
![garage](docs/screenshots/garage.png)
![deathmatch](docs/screenshots/deathmatch.png)
![hold: the ring around the crystal deposit](docs/screenshots/hold.png)
