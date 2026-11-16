# tankit

Single-player arcade tank arena shooter: fast tanks, dense procedurally
generated city maps, bots, and an XP + crystals progression loop.

Look: rugged, solid metal, exposed machinery — see [docs/design-direction.md](docs/design-direction.md).

Plain Rust: `winit` + `wgpu`, no engine.

## Run

```sh
cargo run --release                          # play (random map)
cargo run --release -- --seed 21             # play a specific map
cargo run --release -- --map proving         # physics test course (jumps, ramps, bumps, stairs, hills)
cargo run --release -- --seed 21 --shot out.png --time 5 --size 1600x900   # headless frame to PNG
cargo run --release -- --hull bastion --turret maul   # start with a different loadout
cargo run --release -- --seed 21 --garage --time 2 --shot garage.png   # headless frame with the garage open
cargo run --release -- --showroom            # every hull x turret combo, animated, orbiting camera
cargo run --release -- --showroom --focus 0 --dist 8 --orbit 120 --time 3 --shot tank.png
cargo run --release -- --map proving --spawn 2 --straight --time 1.2 --frames 90 --shot clip/f.png   # frame sequence
cargo test
```

## Controls

Tanki-style: the camera rides behind the turret, and the turret is turned with keys.

| Key | Action |
|---|---|
| W/S or ↑/↓ | throttle / reverse |
| A/D or ←/→ | turn hull |
| Z / X | rotate turret left / right (it turns with the hull) |
| C | swing turret back to center |
| PageUp / PageDown | camera height |
| Space | fire (no projectiles yet: recoil kicks the hull, muzzle flash) |
| G | garage: park, orbit the tank, swap hull and turret (see below) |
| 1 / 2 | quick-cycle hull / turret |
| R | respawn at the next spawn point (cycles the proving ground lanes) |
| F5 | new random map |
| [ / ] | live tuning: sideways grip ×0.9 / ×1.1 |
| ; / ' | live tuning: turn rate −/+ 0.1 rad/s |
| Backspace | reset live tuning |
| Esc | quit |

## Garage

Press **G** mid-battle. The tank parks, the HUD fades out and the camera
swings round to orbit it, with a panel listing the hulls and turrets and
their stats. Parts fit to the tank as you pick them, so you see what you're
getting.

| Key | Action |
|---|---|
| W/S or ↑/↓ | previous / next part |
| A/D or ←/→ | switch between hull and turret |
| Enter or G | deploy with this loadout |
| Esc | revert to the loadout you came in with |

Stat bars are scaled to the best part in the kit; a faint tick marks where
the part you came in with sat.

![garage](docs/screenshots/garage.png)

## Tank kit

Tanks are procedural (`src/tank/`), assembled from beveled armor plates and
chamfered lofted solids, so they read as built from panels. Two hulls
(Lancer: light wedge; Bastion: heavy, segmented skirts) and two turrets
(Lance: long rail cannon; Maul: twin short cannons) mix freely. Light plates
take the team color; dark composite, metal and the orange light strips don't.

Animated: separate tread links running around the wheels at each side's own
speed (so a pivot turn counter-rotates them), spinning road wheels and
sprockets, turret yaw, gun pitch, barrel recoil with muzzle flash (twin guns
alternate), hull pitch/roll and a little rumble, and pulsing light strips.
Surface detail (carbon weave, panel seams, edge wear) is in the shader,
materials 6–11 in `scene.wgsl`.

## Physics

Tanks are rapier3d rigid bodies with honest gravity and contacts. The
tank-specific part (`src/sim.rs`) is the track model: a spring ray per road
wheel carries the weight, and every point where a track touches the world
(road wheels, or the swept-up track run around the sprocket and idler) pushes
along the belt's direction of travel there, limited by friction times that
contact's load. Climbing, pivot turns, and skid steering all come out of that.
Road wheels and the tread belt in the art follow the suspension.

