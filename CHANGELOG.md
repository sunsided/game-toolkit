# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `Painter::rect_ex`, `Painter::rect_outline_ex` and `Painter::line_ex`, which take a layer and
  `BlendMode` like `sprite_ex`, so untextured shapes can draw above a sprite scene (#27).
- `Painter` docs describe the 2D draw order: layers first, then blend mode and texture, with
  call order kept only within one of those groups. Untextured shapes use the white texture,
  which sorts under every loaded texture on the same layer.
- `Graphics::texture_size` / `Painter::texture_size`, the pixel size of a loaded texture
  (follows hot-reloads) (#26).
- `SpriteInstance::with_region(min, size, tex_size)` for drawing a pixel rectangle of an atlas,
  `SpriteInstance::flip_x` / `flip_y` for mirroring any UV range, and
  `Painter::sprite_region(tex, src_pos, src_size, pos, size)` as the layer-0 shorthand (#26).

## [0.3.1] - 2026-10-25

### Fixed

- Generated jam projects (`templates/jam`, `templates/jam-3d`) have a `Cargo.toml` again; the
  manifest was left behind as `Cargo.toml.template`, so `cargo run` failed until it was renamed
  by hand.
- Generated jam projects pass their own `cargo fmt --check` for any project name length.
- Tagged releases create a GitHub release with that version's changelog section after
  publishing to crates.io.

### Changed

- `game-toolkit-ecs` docs and `examples/11_ecs` target sillyecs 0.0.7. The toolkit's API is
  unchanged, but games that move to sillyecs 0.0.7 must adapt: the code generator now lives
  in `sillyecs-build` (`[build-dependencies]`, `use sillyecs_build::EcsCode;` in `build.rs`),
  and `sillyecs` becomes a regular dependency because the generated code refers to its runtime
  types (`sillyecs::EntityId`, ...). Generated `WorldCommand` is generic over a user-command
  type, so the command queue must implement `WorldUserCommand` and the world
  `WorldUserCommandHandler` (`std::convert::Infallible` works when a game has no custom
  commands), and per-entity lookups are now `get_<archetype>_entity` from the generated
  `EntityAccess` trait. `examples/11_ecs` shows the updated pattern.

## [0.3.0] - 2026-10-15

### Added

- Browser support: the toolkit builds and runs on `wasm32-unknown-unknown` from the same game
  code. wgpu uses WebGPU where the browser has it and falls back to WebGL2 (also when a browser
  exposes WebGPU but offers no adapter); audio goes through WebAudio and gamepads through the
  Gamepad API. Bundling is done with [trunk](https://trunkrs.dev).
- On the web, the files listed in `assets/manifest.txt` (or the new `AppConfig::web_preload`)
  are fetched before `Game::init`, so asset loading stays synchronous. `AppConfig::canvas_id`
  renders into an existing `<canvas>`; by default one is appended to `<body>`. Logs and panics go
  to the browser console, and audio starts on the first key press, click or touch.
- `ctx.load_texture(path)` and `ctx.load_sound(path)`, which load from the asset root on every
  platform (natively, `load_texture` still registers the file for hot-reload), and the prelude's
  `LoadAseprite` trait for `ctx.load_aseprite` / `ctx.load_aseprite_json` (`aseprite` feature).
- `asset_root!()`: the calling crate's `assets/` directory natively, `assets` next to the page on
  the web.
- Bytes-based loaders: `Graphics::load_texture_bytes`, `Graphics::load_font_bytes`,
  `Audio::load_sound_bytes`, `SpriteSheet::from_aseprite_bytes` and
  `SpriteSheet::from_aseprite_json_bytes`, plus `Assets::insert` / `Assets::is_loaded` for
  in-memory assets (consulted before the filesystem).
- `Audio::start` / `Audio::is_started`: on the web the audio backend opens lazily on the first
  user gesture; natively it opens immediately as before.
- `examples/13_web`, which runs natively and in the browser, and trunk setup (`index.html`,
  `Trunk.toml`, `assets/manifest.txt`) plus `web`, `web:build`, `assets:manifest` and
  `check:wasm` tasks in both jam templates.
- A CI job that lints the workspace for wasm32 and builds the web example.

### Changed

- **Breaking:** moved the render stack to wgpu 30 - glyphon 0.12, egui / egui-wgpu / egui-winit
  0.37 and vello 0.11 move with it. Consumer code that names `wgpu`, `egui` or `vello` types
  directly must follow those crates' upgrade notes (e.g. `Context::run` → `Context::run_pass`).
- **Breaking:** `Graphics::load_texture`, `Audio::load_sound`, `SpriteSheet::load_aseprite` and
  `SpriteSheet::from_aseprite_json` take file paths and are now native-only (they no longer exist
  on wasm32). Prefer the new `ctx.load_*` helpers, which work everywhere.
- The vello vector pass is skipped, with a logged error, on adapters without compute shaders
  (WebGL2) instead of failing device setup.
- Startup resizes the surface to the window's current size before `Game::init`.
- egui is built with `default_fonts`, which egui 0.37 no longer enables by default; without it the
  debug overlay rendered no text.
- Minimum supported Rust version is now declared: `rust-version = "1.95"` (set by egui 0.37), with
  Cargo's MSRV-aware resolver 3 and a CI job that checks the workspace on exactly that toolchain.
- pollster 1.0; lockfile refreshed to the latest compatible versions.

### Fixed

- The 2D jam template no longer fails its own `task check` on clippy's `needless_range_loop`.

## [0.2.0] - 2026-06-08

### Added

- Deterministic pseudo-random generator in `game-toolkit-core` (`Rng`, an xorshift64* generator),
  exposed as `ctx.rng` and seeded by the new `AppConfig::random_seed` field; re-exported from the
  prelude. For a given seed the sequence is reproducible across runs and platforms. Not
  cryptographically secure - intended for gameplay randomness only.
- A bundled `CLAUDE.md` agent cheatsheet in the generated jam templates (2D and 3D), documenting
  the toolkit API surface so freshly generated projects carry it. It is kept byte-identical to the
  canonical `templates/jam-agent-guide.md` by a `task sync:templates` step and a CI drift guard. A
  contributor-focused root `CLAUDE.md` was added alongside it.

### Fixed

- Stop Cargo from parsing the jam template manifests when `game-toolkit` is consumed as a git
  dependency.

## [0.1.0] - 2026-05-30

Initial public release. A 2D-first Rust game-jam toolkit split into focused crates behind
the `game-toolkit` umbrella; add one dependency and `use game_toolkit::prelude::*;` to start.

### Added

- Core runtime (`game-toolkit-core`): the `Game` trait (`init` / `update` / `render`), the
  winit 0.30 run loop, `Context`, and frame timing with an optional fixed timestep.
- Rendering (`game-toolkit-gfx`): wgpu sprite / primitive / text batchers, an atlas tilemap, a
  layered `Painter`, optional depth buffer + MSAA, an instanced 3D mesh pipeline, and an
  optional vello vector backend (`vector` feature).
- Input (`game-toolkit-input`): keyboard, mouse and gamepad (gilrs) with held / just-pressed /
  just-released semantics, hot-plug and rumble.
- Audio (`game-toolkit-audio`): kira sound loading and playback, with optional synthie
  chiptune synthesis (`synth` feature).
- Assets (`game-toolkit-assets`): asset-root path resolution and optional file hot-reload via
  `notify`.
- Aseprite (`game-toolkit-aseprite`): load native `.aseprite` binaries (via `ah-asefile`) and
  exported PNG + JSON sheets into a GPU-ready `SpriteSheet`, with `AnimationPlayer` for
  tag-driven playback (forward / reverse / ping-pong). Behind the prelude's `aseprite` feature.
- ECS (`game-toolkit-ecs`): glue for the sillyecs compile-time archetype ECS (`Vec2`,
  `ChannelQueue`), behind the `ecs` feature.
- UI (`game-toolkit-ui`): egui debug-overlay integration (egui-wgpu + egui-winit), behind the
  `ui` feature.
- `atlas-packer` CLI for packing a directory of PNGs into a texture atlas plus an
  Aseprite-compatible JSON sidecar.
- `cargo-generate` jam templates (2D and 3D).
- 14 runnable examples under `examples/`, plus a Taskfile to build, lint, test and run them.
- Top-level and per-crate `README.md`s, crates.io metadata, and `#![forbid(unsafe_code)]`
  across every crate.

[Unreleased]: https://github.com/sunsided/game-toolkit/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/sunsided/game-toolkit/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/sunsided/game-toolkit/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/sunsided/game-toolkit/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/sunsided/game-toolkit/releases/tag/v0.1.0
