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
cargo test
```

## Controls

| Input | Action |
|---|---|
| W/S or ↑/↓ | throttle / reverse |
| A/D or ←/→ | turn hull |
| Mouse (click to capture) | aim turret / orbit camera, vertical = camera height |
| Q/E or Z/X | rotate turret with keys |
| C | snap aim back over the hull |
| F5 | new random map |
| Esc | release mouse, press again to quit |
