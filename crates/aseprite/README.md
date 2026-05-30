# toolkit-aseprite

Aseprite loading for [**game-toolkit**](https://github.com/sunsided/game-toolkit). Reads both
native `.aseprite` binaries (via [ah-asefile](https://crates.io/crates/ah-asefile)) and
exported PNG + JSON sprite sheets into a GPU-ready `SpriteSheet`, with a tag-driven
`AnimationPlayer`. The dependency runs one way (aseprite -> gfx), so the renderer stays
unaware of asset formats.

Part of game-toolkit, a Rust workspace of small crates for 2D-first game jams. See the
[workspace README](https://github.com/sunsided/game-toolkit#readme) for the full toolkit,
examples, and quick-start.

## Documentation

API docs: <https://docs.rs/toolkit-aseprite>
