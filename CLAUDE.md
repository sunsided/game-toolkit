# CLAUDE.md - working on game-toolkit

Contributor guide for the **toolkit workspace itself**. (Generated jam projects get their own
`CLAUDE.md`, synced from `templates/jam-agent-guide.md` - see Templates.)

## What this is

Cargo workspace of small crates for 2D/3D game jams. `game-toolkit` is the umbrella crate;
consumers write `use game_toolkit::prelude::*;` (prelude in `crates/prelude/src/lib.rs`
re-exports every subsystem crate's public surface).

| Crate (`crates/…`) | Package | Responsibility |
|--------------------|---------|----------------|
| `core` | `game-toolkit-core` | winit app loop, `Game` trait, `Context`, `Time`, `AppConfig`, `run` |
| `gfx` | `game-toolkit-gfx` | wgpu init + surface, sprite batcher, SDF primitives, glyphon text, tilemap, `Painter`, 3D mesh path |
| `input` | `game-toolkit-input` | keyboard / mouse / gamepad (gilrs) |
| `audio` | `game-toolkit-audio` | sound load + playback on kira; optional `synth` chiptune |
| `assets` | `game-toolkit-assets` | asset-path resolution + hot-reload watcher |
| `aseprite` | `game-toolkit-aseprite` | `.aseprite` + exported-sheet loading into a GPU `SpriteSheet` |
| `ecs` | `game-toolkit-ecs` | glue for the sillyecs archetype ECS |
| `ui` | `game-toolkit-ui` | egui debug-overlay integration (feature-gated) |
| `compute` | `game-toolkit-compute` | GPU compute helpers: WGSL kernels, ping-pong buffers, non-blocking read-back |
| `prelude` | `game-toolkit` | umbrella + prelude; features `ui`, `aseprite`, `ecs`, `synth`, `vector`, `crt`, `indexed`, `compute` |

**One-way dependencies.** `aseprite` and `compute` depend on `gfx`, never the reverse. No
renderer types in `core`, no back-edges.

`#![forbid(unsafe_code)]` in every crate - keep it.

**Every crate must build for `wasm32-unknown-unknown`** (CI `wasm` job / `task check:wasm`, all
features). Native-only code (fs paths, `notify`, `pollster`, blocking GPU read-backs) goes behind
`#[cfg(not(target_arch = "wasm32"))]` with a bytes-based or no-op web counterpart; target deps in
`[target.'cfg(...)'.dependencies]`. Use `web_time::Instant`, never `std::time::Instant` (panics in
browser). Web init is async (`crates/core/src/app.rs` spawns it, finishes in `user_event`); assets
preload from `assets/manifest.txt` before `Game::init`. Test in browser with `task web`.

## Quality gate

- **Hard gate:** `cargo clippy --workspace --all-targets -- -D warnings` (CI + `task lint`),
  run with default and `--all-features`, plus wasm32 clippy + `trunk build` of `examples/13_web`.
  Workspace lints are `warn` but `-D warnings` makes them fatal. sillyecs-generated code
  (`examples/11_ecs`) carries its own allows.
- **CI does not gate on fmt.** Tree is rustfmt-clean and local `task check`/`task ci` run
  `cargo fmt --all --check`, but don't churn the workspace reformatting. (Jam projects do gate on fmt.)
- MSRV job uses `rust-version` in root `Cargo.toml`; bump it together with
  `dtolnay/rust-toolchain@<msrv>` in `.github/workflows/ci.yml`.
- Docs: `RUSTDOCFLAGS=-D warnings cargo doc --workspace --no-deps` - broken intra-doc links fail CI.
- `Cargo.lock` committed; CI builds `--locked`.

## Commands (go-task, `Taskfile.dist.yaml`; `task` lists all)

```sh
task ci                    # check (fmt --check + clippy + templates) then tests
task lint                  # clippy -D warnings   (lint:fix to auto-fix + fmt)
task test                  # nextest if present, else cargo test; doctests separate
task run EXAMPLE=ex_05_tilemap   # run an example by package
task web                   # serve examples/13_web via trunk (WEB_DIR=... to switch; web:lan for LAN)
task check:wasm            # clippy for wasm32, all features
task assets:manifest       # regenerate <WEB_DIR>/assets/manifest.txt
task atlas -- --input sprites/ --output a.png --metadata a.json   # atlas-packer CLI
task publish:dry           # trial publish in dependency order
task labels                # sync GitHub PR labels with .github/labeler.yml
```

PRs are path-labelled (`.github/labeler.yml`); a new crate or top-level area needs an entry there
and in `task labels`. Examples: `examples/` (`ex_01_window` … `ex_19_palette`, `bouncing_ball`);
atlas-packer: `tools/atlas-packer`.

## Templates

`templates/jam` (2D) and `templates/jam-3d` (3D) are `cargo generate` starters, each shipping its
own `CLAUDE.md`.

- **Source of truth:** `templates/jam-agent-guide.md`. After editing run **`task sync:templates`**
  (`tools/sync-guide.sh`): stamps it into both template `CLAUDE.md`s and
  `crates/prelude/src/guide.md` (docs.rs `game_toolkit::guide`; Rust blocks become `rust,ignore`).
- `<!-- agent-only -->` / `<!-- /agent-only -->` (own lines) keep content out of the crate docs.
- `task check:templates` (in `task check`) and CI fail on drift.
- No literal `{{` in the guide (liquid templating; also why `Taskfile.yaml` is in
  `cargo-generate.toml`'s `exclude`).
- Manifests stay `Cargo.toml.liquid` (stops Cargo parsing them as packages via git deps).
- **`task test:templates`** (`tools/check-templates.sh`, CI job) generates each template with a
  short and long name and runs fmt check + clippy (native + wasm32). Keep lines containing
  `{{project-name}}` rustfmt-stable for any name length (e.g. bind it to a `let`).

## Test assets

No ffmpeg (absent in CI). Audio: Python `wave`. Sprites: Aseprite CLI.

## Releasing

Push a `vX.Y.Z` tag; `.github/workflows/release.yml` runs `cargo release publish --workspace`
in dependency order (`release.toml`). Never publish by hand; cut releases with `cargo release`.
