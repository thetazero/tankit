# Notes for Claude

## Keep CPU use low

The user plays the game on this machine while tabs work. Use a few cores at most:

- Build and test with 2 jobs: `cargo build -j 2`, `cargo test -j 2 -- --test-threads=2` (or set `CARGO_BUILD_JOBS=2`).
- Run headless renders and screenshots one at a time, never several in parallel.
