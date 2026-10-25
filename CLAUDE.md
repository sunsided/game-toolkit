# CLAUDE.md - working on game-toolkit

Contributor guide for the **toolkit workspace itself**. (Generated jam projects get a different,
consumer-facing `CLAUDE.md`, synced from `templates/jam-agent-guide.md` - see Templates below.)

## What this is

A Cargo workspace of small, focused crates for 2D and 3D game jams. `game-toolkit` is the umbrella
crate; consumers depend on it and write `use game_toolkit::prelude::*;` (the prelude lives in
`crates/prelude/src/lib.rs` and re-exports the public surface of every subsystem crate).

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
| `prelude` | `game-toolkit` | umbrella + prelude; features `ui`, `aseprite`, `ecs`, `synth`, `vector` |

**Dependency direction is one-way.** `game-toolkit-aseprite` depends on `game-toolkit-gfx`, never
the reverse, so the renderer stays unaware of asset formats. Do not pull renderer-specific types
into `game-toolkit-core`, and do not create back-edges that the consumer would not expect.

`#![forbid(unsafe_code)]` holds across the crates - keep it that way.

**Every crate must also build for `wasm32-unknown-unknown`** (CI's `wasm` job and `task check:wasm`
clippy the whole workspace for it, all features). Native-only code (filesystem paths, `notify`,
`pollster`, blocking GPU read-backs) sits behind `#[cfg(not(target_arch = "wasm32"))]` and gets a
bytes-based or no-op counterpart on the web; target-specific deps go in
`[target.'cfg(...)'.dependencies]`. Use `web_time::Instant`, never `std::time::Instant`
(it panics in the browser). Init on the web is async (`crates/core/src/app.rs` spawns it and
finishes in `user_event`), and assets are preloaded from `assets/manifest.txt` before
`Game::init`, which is how the synchronous `Game` API survives. Try changes in a browser with
`task web` (`examples/13_web`).

## The quality gate

- **Clippy with warnings-as-errors is the hard gate** (CI + `task lint`):
  `cargo clippy --workspace --all-targets -- -D warnings`. It must be clean.
- **CI does not gate on formatting.** `.github/workflows/ci.yml` runs only clippy / test / docs
  (+ the templates check) - no `cargo fmt --check`. The tree happens to be rustfmt-clean and the
  local `task check`/`task ci` do run `cargo fmt --all --check`, but since CI won't fail on format,
  don't churn the whole workspace with reformatting. (Generated jam projects *do* gate on fmt via
  their own `task check`; that rule is theirs, not ours.)
- CI runs clippy twice - default features and `--all-features` - plus a wasm32 clippy +
  `trunk build` of `examples/13_web`, and an MSRV job that checks
  the workspace on the toolchain named by `rust-version` in the root `Cargo.toml`. Bump the two
  together (`dtolnay/rust-toolchain@<msrv>` in `.github/workflows/ci.yml`).
- Workspace lints (`[workspace.lints]`) are `warn`-level but `-D warnings` makes them a hard
  gate. sillyecs-generated code (`examples/11_ecs`) carries its own lint allows, so it needs
  none from us.
- Docs build under `RUSTDOCFLAGS=-D warnings` (`cargo doc --workspace --no-deps`) - broken
  intra-doc links fail CI before docs.rs sees them.
- `Cargo.lock` is committed for reproducible CI; CI builds `--locked`.

## Commands (go-task, `Taskfile.dist.yaml`)

```sh
task                       # list tasks
task ci                    # check (fmt --check + clippy + templates) then tests
task check                 # static checks; includes check:templates (see below)
task lint                  # clippy -D warnings   (lint:fix to auto-fix + fmt)
task test                  # nextest if present, else cargo test; doctests run separately
task run EXAMPLE=ex_05_tilemap   # run an example by package; see example:* / run:* aliases
task web                   # serve examples/13_web in a browser via trunk (WEB_DIR=... to switch)
task check:wasm            # clippy for wasm32-unknown-unknown, all features
task assets:manifest       # regenerate <WEB_DIR>/assets/manifest.txt (web preload list)
task atlas -- --input sprites/ --output a.png --metadata a.json   # atlas-packer CLI
task publish:dry           # trial publish in dependency order
task labels                # create/update GitHub PR labels (gh); keep in sync with .github/labeler.yml
```

PRs are labelled by changed paths (`.github/workflows/labeler.yml`, config in
`.github/labeler.yml`). Adding a crate or top-level area means a new entry there and in `task labels`.

Examples live in `examples/` (`ex_01_window` … `ex_13_web`, `bouncing_ball`); the offline
atlas-packer is in `tools/atlas-packer`.

## Templates

`templates/jam` (2D) and `templates/jam-3d` (3D) are `cargo generate` starters. `cargo generate`
copies only the chosen subdir, so each ships its **own** bundled agent doc as `CLAUDE.md`.

- **Source of truth:** `templates/jam-agent-guide.md`. Edit it there.
- After editing, run **`task sync:templates`** (`tools/sync-guide.sh`) to stamp it into
  `templates/jam/CLAUDE.md` and `templates/jam-3d/CLAUDE.md`, and into
  `crates/prelude/src/guide.md`, which docs.rs shows as the `game_toolkit::guide` module.
- Wrap template- or agent-specific lines in `<!-- agent-only -->` / `<!-- /agent-only -->`
  (each on its own line): they reach the `CLAUDE.md` copies but not the crate docs. The crate
  copy also turns its Rust blocks into `rust,ignore` (they are fragments, not doctests).
- `task check:templates` (folded into `task check`) and a CI step both fail on drift.
- Keep the guide free of literal `{{` - cargo-generate runs files through liquid templating and
  would choke (the same reason `Taskfile.yaml` is in `cargo-generate.toml`'s `exclude`).
- The manifests are `Cargo.toml.liquid`: cargo-generate strips `.liquid`, and Cargo doesn't parse
  them as packages when the repo is a git dependency. Don't rename them to anything else.
- **`task test:templates`** (`tools/check-templates.sh`, also a CI job) generates each template
  with a short and a long project name against this checkout and runs `cargo fmt --check` and
  clippy (native + wasm32) on the result. Template code splices in `{{project-name}}`, so keep
  lines that contain it rustfmt-stable for any name length (e.g. bind it to a `let`).

## Generating test assets

Don't rely on ffmpeg for generating assets - it isn't installed in CI and may be absent locally,
so anything that shells out to it won't reproduce. Use Python's `wave` module for audio test
assets and the Aseprite CLI for sprites.

## Releasing

Releases are tag-driven: pushing a `vX.Y.Z` tag triggers `.github/workflows/release.yml`, which
runs `cargo release publish --workspace --no-confirm --execute` and publishes every non-
`publish = false` crate to crates.io in dependency order (config in `release.toml`). Do not publish
crates by hand; cut a release with `cargo release` so the tag and order are correct.
