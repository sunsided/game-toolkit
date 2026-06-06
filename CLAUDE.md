# CLAUDE.md - working on game-toolkit

Contributor guide for the **toolkit workspace itself**. (Generated jam projects get a different,
consumer-facing `CLAUDE.md`, synced from `templates/jam-agent-guide.md` - see Templates below.)

## What this is

A Cargo workspace of small, focused crates for 2D-first game jams. `game-toolkit` is the umbrella
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

## The quality gate

- **Clippy with warnings-as-errors is the hard gate** (CI + `task lint`):
  `cargo clippy --workspace --all-targets -- -D warnings`. It must be clean.
- **CI does not gate on formatting.** `.github/workflows/ci.yml` runs only clippy / test / docs
  (+ the templates check) - no `cargo fmt --check`. The tree happens to be rustfmt-clean and the
  local `task check`/`task ci` do run `cargo fmt --all --check`, but since CI won't fail on format,
  don't churn the whole workspace with reformatting. (Generated jam projects *do* gate on fmt via
  their own `task check`; that rule is theirs, not ours.)
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
task atlas -- --input sprites/ --output a.png --metadata a.json   # atlas-packer CLI
task publish:dry           # trial publish in dependency order
```

Examples live in `examples/` (`ex_01_window` … `ex_12_vector`, `bouncing_ball`); the offline
atlas-packer is in `tools/atlas-packer`.

## Templates

`templates/jam` (2D) and `templates/jam-3d` (3D) are `cargo generate` starters. `cargo generate`
copies only the chosen subdir, so each ships its **own** bundled agent doc as `CLAUDE.md`.

- **Source of truth:** `templates/jam-agent-guide.md`. Edit it there.
- After editing, run **`task sync:templates`** to stamp it into `templates/jam/CLAUDE.md` and
  `templates/jam-3d/CLAUDE.md` (byte-identical copies).
- `task check:templates` (folded into `task check`) and a CI step both fail on drift.
- Keep the guide free of literal `{{` - cargo-generate runs files through liquid templating and
  would choke (the same reason `Taskfile.yaml` is in `cargo-generate.toml`'s `exclude`).

## Generating test assets

ffmpeg is broken in this environment. For audio test assets use Python's `wave` module; for
sprites use the Aseprite CLI. Do not reach for ffmpeg.

## Releasing

Releases are tag-driven: pushing a `vX.Y.Z` tag triggers `.github/workflows/release.yml`, which
runs `cargo release publish --workspace --no-confirm --execute` and publishes every non-
`publish = false` crate to crates.io in dependency order (config in `release.toml`). Do not publish
crates by hand; cut a release with `cargo release` so the tag and order are correct.
