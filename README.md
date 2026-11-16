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
| 1 / 2 | cycle hull / turret |
| R | respawn at the next spawn point (cycles the proving ground lanes) |
| F5 | new random map |
| [ / ] | live tuning: sideways grip ×0.9 / ×1.1 |
| ; / ' | live tuning: turn rate −/+ 0.1 rad/s |
| Backspace | reset live tuning |
| Esc | quit |

## Tank kit

Tanks are procedural (`src/tank/`) and constructed from their armor
plates: each face of the form is one thick plate (V-bow glacis on a center
ridge, lower glacis, creased sloped sides, cheeks, fenders, overlapping
skirts, roof, rear deck, rear), meeting its neighbors at edges where the
form turns, over a dark core. Turrets the same way: a ridged roof, raked
cheeks, and flanks that bulge to a widest line and undercut below it.
Lancer/Bastion and Lance/Maul are proportions and gun fits on shared forms,
and mix freely. Proportions are deliberately exaggerated (wide tracks, a big
turret set forward, a thick gun, a roofline leaning into a long wedge nose).
Ambient occlusion is baked into the vertex colors at startup
(`MeshData::bake_ao`), so joints, creases and overhangs read dark.

Rendering for vehicles: procedural surface textures are generated at startup
(`render/surface.rs`: hammered/dented plate normals, scratches, chips, paint
blotching, roughness variation, grime streaks; tileable, mipmapped) and
sampled triplanar in each part's object space, so they need no UVs and move
with the part. Lighting on vehicles is physically based (GGX highlights,
metal-tinted reflections, roughness-blurred sky reflection). A finer second
texture octave keeps narrow faces (plate edges, bevels) detailed; bevels chip
through to bare steel in patches.

Ambient occlusion: screen-space (`shaders/ssao.wgsl`, a pass between the
scene and post) for contact between separate parts, the ground and the city,
on top of the per-vertex bake for small parts. The look follows `docs/design-direction.md`.

Machinery that moves with the tank:
- tread links run around the wheels at each side's own speed
- each road wheel hangs on a trailing arm with a shock that follows its
  real suspension travel (from the physics)
- a hydraulic ram raises the gun, and the stabilizer lags hull motion, so the
  ram is always working
- recoil cylinders slide with the barrel
- the ring gear turns under the turret

Surface shading is in `scene.wgsl`, materials 6–12.

## Physics

Tanks are rapier3d rigid bodies with honest gravity and contacts. The
tank-specific part (`src/sim.rs`) is the track model: a spring ray per road
wheel carries the weight, and every point where a track touches the world
(road wheels, or the swept-up track run around the sprocket and idler) pushes
along the belt's direction of travel there, limited by friction times that
contact's load. Climbing, pivot turns, and skid steering all come out of that.
Road wheels and the tread belt in the art follow the suspension.

