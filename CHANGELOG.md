# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `game-toolkit-aseprite` crate: load native `.aseprite` binaries (via `ah-asefile`) and
  exported PNG + JSON sheets into a GPU-ready `SpriteSheet`, with `AnimationPlayer` for
  tag-driven playback (forward / reverse / ping-pong). Exposed through the prelude behind
  an `aseprite` feature.
- Examples: `02_sprite` (PNG loading via `load_texture`), `07_audio` (kira playback),
  `08_hot_reload` (live texture reload via `notify`), `09_aseprite` (native `.aseprite`
  animation).
- Top-level `README.md` with quick-start, crate matrix, and example index.
- Per-crate `README.md` and Cargo `description` metadata for clean crates.io pages.
- This changelog.

[Unreleased]: https://github.com/sunsided/game-toolkit/commits/main
