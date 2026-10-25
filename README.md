# game-toolkit

[![Crates.io](https://img.shields.io/crates/v/game-toolkit.svg)](https://crates.io/crates/game-toolkit)
[![docs.rs](https://img.shields.io/docsrs/game-toolkit)](https://docs.rs/game-toolkit)
[![license](https://img.shields.io/badge/license-EUPL--1.2-blue.svg)](#license)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)

A Rust workspace of small, focused crates for 2D-first game jams. Drop in, implement
`Game::init` / `update` / `render`, and ship. Each subsystem is its own crate, so a jam
pulls only what it uses.

Built on `wgpu`, `winit` 0.30, `kira`, `glyphon` (cosmic-text), `image`, and `bytemuck`,
with optional `egui` tooling overlays and Aseprite asset loading.

## Quick start

```rust
use game_toolkit::prelude::*;

struct Game1;

impl Game for Game1 {
    fn init(_ctx: &mut Context) -> Result<Self> {
        Ok(Self)
    }

    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.1, 0.1, 0.15, 1.0]);
        p.text([24.0, 24.0], "hello, jam", 28.0, [1.0, 1.0, 1.0, 1.0]);
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<Game1>(AppConfig {
        title: "my-jam".into(),
        width: 800,
        height: 600,
        ..Default::default()
    })
}
```

`AppConfig::asset_root` defaults to `./assets` relative to the working directory. Binaries
that load files and want to run from anywhere (and in the browser) should set it with the
`asset_root!()` macro: the crate's `assets/` directory natively, `assets` next to `index.html`
on the web. Load files through `ctx.load_texture("player.png")` / `ctx.load_sound("boom.wav")`
(and `ctx.load_aseprite(..)` with the `aseprite` feature), which resolve against it.

```rust
asset_root: asset_root!(),
```

`AppConfig` also exposes `msaa_samples` (e.g. `4` for anti-aliasing; default `1`) and
`depth_format` (`Some(wgpu::TextureFormat::Depth32Float)` to allocate a depth buffer;
default `None`). The built-in 2D pipelines never write depth, so enabling it is harmless
and is there to support depth-tested rendering. `AppConfig::random_seed` seeds the deterministic
per-`Context` RNG (`ctx.rng`); a fixed seed reproduces a run.

## Start a new jam

Generate a project from a template with [cargo-generate](https://cargo-generate.github.io):

```sh
cargo generate --git https://github.com/sunsided/game-toolkit templates/jam --name my-jam
# or the 3D starter (spinning cube):
cargo generate --git https://github.com/sunsided/game-toolkit templates/jam-3d --name my-jam
```

Each template is a minimal `Game` (window + Esc-to-quit + a drawn title) depending on
`game-toolkit` with the `ui` feature on, plus an `assets/` directory. `cd my-jam && cargo run`.
Generated projects also include a `CLAUDE.md` cheatsheet of the toolkit API, so coding agents
working in the new repo are productive without first spelunking the toolkit source.

## Crates

| Crate | What it gives you |
|-------|-------------------|
| `game-toolkit-core` | App loop on winit 0.30 `ApplicationHandler`, the `Game` trait, `Context`, time, optional fixed timestep, and a deterministic seedable RNG (`ctx.rng`). |
| `game-toolkit-gfx` | wgpu init + surface management, sprite batcher, SDF circle/ring primitives, glyphon text, atlas tilemap, the `Painter` API. |
| `game-toolkit-input` | Keyboard, mouse and gamepads (via gilrs) with held / just-pressed / just-released semantics, hot-plug, and rumble. |
| `game-toolkit-audio` | Sound loading + playback on `kira`; degrades gracefully to muted when no device is available. Optional chiptune synthesis (`synth` feature, via `synthie`). |
| `game-toolkit-assets` | Asset path resolution, in-memory/preloaded assets (web), optional hot-reload watcher (`notify`). |
| `game-toolkit-aseprite` | Load native `.aseprite` files and exported PNG + JSON sheets into a GPU-ready `SpriteSheet` with animation playback. |
| `game-toolkit-ecs` | Small glue (component data, command queue) for the `sillyecs` compile-time archetype ECS. |
| `game-toolkit-ui` | egui overlay integration for debug tooling (feature-gated). |
| `game-toolkit` | Umbrella crate - depend on this one and `use game_toolkit::prelude::*;`. Features: `ui`, `aseprite`, `ecs`, `synth`, `vector`. |

The dependency direction is one-way: `game-toolkit-aseprite` depends on `game-toolkit-gfx`, never the
reverse, so the renderer stays unaware of asset formats.

## Examples

Run any example with `cargo run -p <package>`.

| Run | Package | Shows |
|-----|---------|-------|
| `01_window` | `ex_01_window` | Minimal window with an animated clear color; Esc quits. |
| `02_sprite` | `ex_02_sprite` | Load a PNG via `ctx.load_texture`, draw it plain, spinning, and tinted. |
| `03_primitives` | `ex_03_primitives` | SDF circles, rings, lines, rectangles; F12 saves a screenshot. |
| `04_text` | `ex_04_text` | Text rendering through glyphon. |
| `05_tilemap` | `ex_05_tilemap` | Instanced atlas tilemap. |
| `06_egui` | `ex_06_egui` | egui debug overlay (uses the `ui` feature). |
| `07_3d` | `ex_07_3d` | Depth-tested perspective cubes (instanced meshes) with a 2D HUD on top. |
| `07_audio` | `ex_07_audio` | Play a WAV (Space) and synthesized retro effects (1/2/3); runs muted with no device. |
| `08_hot_reload` | `ex_08_hot_reload` | Edit `assets/reload_me.png` while it runs and watch the texture update live. |
| `09_aseprite` | `ex_09_aseprite` | Load a native `.aseprite`, pack its frames into an atlas, and play a tagged animation. |
| `10_gamepad` | `ex_10_gamepad` | Live gamepad overlay: sticks, buttons, stick-clicks, and rumble on A. |
| `11_ecs` | `ex_11_ecs` | Bouncing particles driven by the `sillyecs` archetype ECS (build.rs codegen). |
| `12_vector` | `ex_12_vector` | Vector graphics (filled circles, stroked path) via the optional vello backend. |
| `13_web` | `ex_13_web` | Same code natively and in the browser: sprite, text, sound on click, gamepad. `task web`. |
| `bouncing_ball` | `bouncing_ball` | A faux-3D tumbling beachball shaded through the sprite batcher. |

## Audio

`ctx.audio` is an `Option<Audio>` (`None` when there is no output device; the game runs muted).
Every play call returns a `SoundHandle` that controls that instance, so volume, pausing and
looping need no direct `kira` dependency (`kira` is re-exported from the prelude for anything
beyond that, at the toolkit's version):

```rust
let music = ctx.load_sound("music.ogg")?;           // Option<SoundId>; load in init
if let (Some(audio), Some(id)) = (&mut ctx.audio, music) {
    // Loop at 30% volume; `deferred` queues it until the browser allows audio.
    let handle = audio.play_with(id, PlayOptions::looped().volume(0.3).deferred())?;
    handle.set_volume(0.5);                          // or set_volume_db / fade_volume
    handle.pause();                                  // resume, stop, is_playing, is_finished
}
```

Plain `audio.play(id)` plays once. On the web it fails with `AudioError::NotStarted` until the
player's first key press, click or touch, so log or ignore its error rather than `?`-ing it in
`update`. `examples/07_audio` shows one-shots, synthesized effects and a looping track.

## Aseprite

`game-toolkit-aseprite` reads both Aseprite formats into the same `SpriteSheet`:

```rust
// Native binary: frames are flattened and packed into one atlas texture.
let sheet = ctx.load_aseprite("character.aseprite")?;

// Exported sprite sheet: a PNG atlas plus its Aseprite JSON sidecar.
let sheet = ctx.load_aseprite_json("sheet.png", "sheet.json")?;

// Play a tag (or `sheet.full_animation()` when a sheet has no tags).
let mut player = AnimationPlayer::new(&sheet, "walk").unwrap();
player.advance(&sheet, dt);
let (uv_min, uv_max) = player.current_uv(&sheet);
```

`ctx.load_aseprite*` (from the prelude's `LoadAseprite` trait) read through `ctx.assets`, so
they work on the web too. The underlying `SpriteSheet::from_aseprite_bytes` /
`from_aseprite_json_bytes` take bytes; the path-taking `SpriteSheet::load_aseprite` /
`from_aseprite_json` are native-only.

Both the hash and array `frames` layouts of the exported JSON are accepted, and tag
directions (forward / reverse / ping-pong) drive playback.

## Tools

`tools/atlas-packer` is an offline CLI that packs a directory of PNGs into one atlas:

```sh
cargo run -p atlas-packer -- --input sprites/ --output atlas.png --metadata atlas.json
# options: --max-size 2048 --padding 2
```

It writes the atlas PNG plus an Aseprite-compatible JSON sidecar, so the result loads with
no extra code via `SpriteSheet::from_aseprite_json(&mut ctx.gfx, "atlas.png", "atlas.json")`.

`ctx.gfx.request_screenshot("shot.png")` saves a PNG of the next presented frame, read back
from the surface with [miniscreenshot-wgpu](https://github.com/sunsided/miniscreenshot).

## 3D

A small instanced static-mesh path renders under the 2D layers (which composite on top).
It needs a depth buffer (`AppConfig::depth_format`).

```rust
let cube = ctx.gfx.create_mesh(&vertices, &indices); // [MeshVertex] + [u16]
ctx.gfx.camera3d.eye = [0.0, 1.6, 6.0];              // perspective Camera3D

// each frame, via the painter:
let model = transform::mul(&transform::translation([x, 0.0, 0.0]), &transform::rotation_y(t));
p.mesh(cube, model, [0.4, 0.8, 0.45, 1.0]);
```

Lighting is forward-unlit (Lambert from the vertex normal). glTF loading and a perspective
follow-camera are future work.

## Vector graphics

The optional `vector` feature adds a [vello](https://github.com/linebender/vello) backend
for high-quality 2D vector content. `painter.vector(|scene| ...)` hands the frame's
`vello::Scene`; vello renders it to an offscreen target that composites on top of the
sprite/primitive/text layers. It is compute-based and needs higher device limits, so it is
off by default. On adapters without compute shaders (WebGL2 in the browser) vector drawing is
skipped with a logged error.

```rust
use vello::{kurbo::{Affine, Circle}, peniko::{Color, Fill}};
p.vector(|scene| {
    scene.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(240, 140, 168), None,
               &Circle::new((400.0, 300.0), 120.0));
});
```

## Web (wasm32)

The toolkit builds for the browser (`wasm32-unknown-unknown`) from the same game code. wgpu uses
WebGPU where the browser has it and falls back to WebGL2 otherwise; audio goes through WebAudio and
gamepads through the Gamepad API. [trunk](https://trunkrs.dev) does the bundling:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
task web                              # serve examples/13_web at http://localhost:8080
PROFILE=release task web:build        # dist/ ready to upload (itch.io, GitHub Pages)
```

A crate needs an `index.html` (`<link data-trunk rel="rust">` plus `<link data-trunk
rel="copy-dir" href="assets">`) and a `Trunk.toml`; `examples/13_web` and both jam templates ship
one. Differences from native:

- **Assets are preloaded.** The browser can only fetch asynchronously, so before `Game::init`
  the toolkit fetches every file listed in `assets/manifest.txt` (one path per line;
  `task assets:manifest` regenerates it) or in `AppConfig::web_preload`. After that,
  `ctx.load_*` and `ctx.assets.read` work synchronously as usual. The path-taking
  `Graphics::load_texture` / `Audio::load_sound` are native-only; use the `*_bytes` variants or
  the `ctx.load_*` helpers.
- **Audio starts on the first key press, click or touch** (browsers block audio before a user
  gesture). Sounds can be loaded right away; `Audio::play` fails with `AudioError::NotStarted`
  until then, and `PlayOptions::deferred` plays queue until it happens.
- **Not available:** hot-reload (`Assets::watch` is a no-op), `Graphics::request_screenshot`,
  and vello on WebGL2. `ctx.quit()` stops the loop but leaves the page.
- **Text** uses an embedded DejaVu Sans, since there are no system fonts
  (`Graphics::load_font_bytes` adds more).
- Logs and panics go to the browser console (unless the game installs its own logger).
- `AppConfig::canvas_id` renders into an existing `<canvas id="...">`; by default a canvas is
  appended to `<body>`.

A release build of `13_web` is about 6.8 MB of wasm (2.6 MB gzipped).

## Randomness

`game-toolkit-core` ships a small deterministic xorshift64* generator, exposed per-`Context` as
`ctx.rng` and seeded by `AppConfig::random_seed`. The same seed reproduces the same sequence across
runs and platforms, which is handy for debugging and replays. It is not cryptographically secure -
use it for gameplay (procedural generation, jitter, AI variation), not secrets.

```rust
let r = ctx.rng.next_f32();         // [0.0, 1.0)
let face = ctx.rng.range_u32(1..7); // 1..=6
ctx.rng.reseed(seed);               // restart the stream
```

## Development

Common tasks run through [go-task](https://taskfile.dev) (`Taskfile.dist.yaml`):

```sh
task               # list tasks
task ci            # fmt check + clippy + tests
task run:3d        # run an example (see task --list for example:* shortcuts)
task web           # serve examples/13_web in the browser (WEB_DIR=... for another crate)
task check:wasm    # clippy for wasm32-unknown-unknown (part of task check)
task atlas -- --input sprites/ --output a.png --metadata a.json
task publish:dry   # trial publish all crates in order (task publish releases for real)
```

The generated jam templates ship their own `Taskfile.yaml` too.

## Status

First crates.io release: `0.1.0`. The 2D runtime, input (keyboard / mouse / gamepad),
audio, assets, text, tilemap, egui overlay, Aseprite loading, optional depth/MSAA, a
first-cut 3D mesh path, a deterministic seedable RNG, browser (wasm32) builds, ECS glue, the atlas-packer CLI, and
`cargo-generate` jam templates are all in place. Still pre-1.0, so the API may shift between minor versions; roadmap and
open workstreams live in the [toolkit epic](https://github.com/sunsided/game-toolkit/issues/15).

## License

Licensed under the European Union Public Licence v. 1.2 (EUPL-1.2). See
[LICENSE](https://github.com/sunsided/game-toolkit/blob/main/LICENSE), or the
[official text](https://joinup.ec.europa.eu/collection/eupl/eupl-text-eupl-12).
