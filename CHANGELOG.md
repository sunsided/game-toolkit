# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Breaking:** moved the render stack to wgpu 30 - glyphon 0.12, egui / egui-wgpu / egui-winit
  0.37 and vello 0.11 move with it. Consumer code that names `wgpu`, `egui` or `vello` types
  directly must follow those crates' upgrade notes (e.g. `Context::run` → `Context::run_pass`).
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

[Unreleased]: https://github.com/sunsided/game-toolkit/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/sunsided/game-toolkit/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/sunsided/game-toolkit/releases/tag/v0.1.0
