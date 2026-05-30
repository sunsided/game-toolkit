# game-toolkit-gfx

The rendering layer of [**game-toolkit**](https://github.com/sunsided/game-toolkit), built on
wgpu: sprite / primitive (SDF circle, line) / text batchers, an atlas tilemap, a `Painter`
API with layered 2D draw order, optional depth buffer + MSAA, an instanced 3D mesh pipeline,
and an optional [vello](https://github.com/linebender/vello) vector backend (`vector` feature).

Part of game-toolkit, a Rust workspace of small crates for 2D-first game jams. Most users
depend on [`game-toolkit`](https://crates.io/crates/game-toolkit) rather than this crate
directly. See the [workspace README](https://github.com/sunsided/game-toolkit#readme) for the
full toolkit, examples, and quick-start.

## Documentation

API docs: <https://docs.rs/game-toolkit-gfx>
