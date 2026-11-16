# tankit

Single-player arcade tank arena shooter: fast tanks, dense procedurally
generated city maps, bots, and an XP + crystals progression loop.

Look: rugged, solid metal, exposed machinery — see [docs/design-direction.md](docs/design-direction.md).

Plain Rust: `winit` + `wgpu`, no engine.

## Run

```sh
cargo run --release                          # play (random map)
cargo run --release -- --seed 21             # play a specific map
cargo run --release -- --seed 21 --shot out.png --time 5 --size 1600x900   # headless frame to PNG
cargo run --release -- --showroom            # every hull x turret combo, animated, orbiting camera
cargo run --release -- --showroom --focus 0 --dist 8 --orbit 120 --time 3 --shot tank.png
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
| Space | fire (visual only for now: recoil, muzzle flash, hull kick) |
| 1 / 2 | cycle hull / turret |
| F5 | new random map |
| Esc | quit |

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
