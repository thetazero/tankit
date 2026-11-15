# tankit

Single-player arcade tank arena shooter: fast tanks, dense procedurally
generated city maps, bots, and an XP + crystals progression loop.

Plain Rust: `winit` + `wgpu`, no engine.

## Run

```sh
cargo run --release              # play in a window (Esc quits)
cargo run --release -- --shot out.png --time 5 --size 1600x900   # headless frame to PNG
cargo test
```
