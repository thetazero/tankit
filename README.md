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

Tanks are procedural (`src/tank/`) and constructed from their armor
plates: each face of the form is one thick plate (V-bow glacis on a center
ridge, lower glacis, creased sloped sides, cheeks, fenders, overlapping
skirts, roof, rear deck, rear), meeting its neighbors at edges where the
form turns, over a dark core. Turrets the same way: a ridged roof, raked
cheeks, and flanks that bulge to a widest line and undercut below it.
Lancer/Bastion and Lance/Maul are proportions and gun fits on shared forms,
and mix freely. The look follows `docs/design-direction.md`.

Machinery that moves with the tank:
- tread links run around the wheels at each side's own speed
- each road wheel hangs on a trailing arm with a shock that works as the
  hull pitches and rolls
- a hydraulic ram raises the gun, and the stabilizer lags hull motion, so the
  ram is always working
- recoil cylinders slide with the barrel
- the ring gear turns under the turret

Surface shading is in `scene.wgsl`, materials 6–12.
