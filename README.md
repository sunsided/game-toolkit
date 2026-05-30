# game-toolkit

[![license](https://img.shields.io/badge/license-EUPL--1.2-blue.svg)](#license)

A Rust workspace of small, focused crates for 2D-first game jams. Drop in, implement
`Game::init` / `update` / `render`, and ship. Each subsystem is its own crate, so a jam
pulls only what it uses.

Built on `wgpu`, `winit` 0.30, `kira`, `glyphon` (cosmic-text), `image`, and `bytemuck`,
with optional `egui` tooling overlays and Aseprite asset loading.

## Quick start

```rust
use toolkit_prelude::*;

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
that load files and want to run from anywhere should set it explicitly:

```rust
asset_root: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
```

`AppConfig` also exposes `msaa_samples` (e.g. `4` for anti-aliasing; default `1`) and
`depth_format` (`Some(wgpu::TextureFormat::Depth32Float)` to allocate a depth buffer;
default `None`). The built-in 2D pipelines never write depth, so enabling it is harmless
and is there to support depth-tested rendering.

## Start a new jam

Generate a project from a template with [cargo-generate](https://cargo-generate.github.io):

```sh
cargo generate --git https://github.com/sunsided/game-toolkit templates/jam --name my-jam
# or the 3D starter (spinning cube):
cargo generate --git https://github.com/sunsided/game-toolkit templates/jam-3d --name my-jam
```

Each template is a minimal `Game` (window + Esc-to-quit + a drawn title) depending on
`toolkit-prelude` with the `ui` feature on, plus an `assets/` directory. `cd my-jam && cargo run`.

## Crates

| Crate | What it gives you |
|-------|-------------------|
| `toolkit-core` | App loop on winit 0.30 `ApplicationHandler`, the `Game` trait, `Context`, time, optional fixed timestep. |
| `toolkit-gfx` | wgpu init + surface management, sprite batcher, SDF circle/ring primitives, glyphon text, atlas tilemap, the `Painter` API. |
| `toolkit-input` | Keyboard, mouse and gamepads (via gilrs) with held / just-pressed / just-released semantics, hot-plug, and rumble. |
| `toolkit-audio` | Sound loading + playback on `kira`; degrades gracefully to muted when no device is available. Optional chiptune synthesis (`synth` feature, via `synthie`). |
| `toolkit-assets` | Asset path resolution + optional hot-reload watcher (`notify`). |
| `toolkit-aseprite` | Load native `.aseprite` files and exported PNG + JSON sheets into a GPU-ready `SpriteSheet` with animation playback. |
| `toolkit-ui` | egui overlay integration for debug tooling (feature-gated). |
| `toolkit-prelude` | `use toolkit_prelude::*;` re-exports. Features: `ui`, `aseprite`. |

The dependency direction is one-way: `toolkit-aseprite` depends on `toolkit-gfx`, never the
reverse, so the renderer stays unaware of asset formats.

## Examples

Run any example with `cargo run -p <package>`.

| Run | Package | Shows |
|-----|---------|-------|
| `01_window` | `ex_01_window` | Minimal window with an animated clear color; Esc quits. |
| `02_sprite` | `ex_02_sprite` | Load a PNG via `load_texture`, draw it plain, spinning, and tinted. |
| `03_primitives` | `ex_03_primitives` | SDF circles, rings, lines, rectangles; F12 saves a screenshot. |
| `04_text` | `ex_04_text` | Text rendering through glyphon. |
| `05_tilemap` | `ex_05_tilemap` | Instanced atlas tilemap. |
| `06_egui` | `ex_06_egui` | egui debug overlay (uses the `ui` feature). |
| `07_3d` | `ex_07_3d` | Depth-tested perspective cubes (instanced meshes) with a 2D HUD on top. |
| `07_audio` | `ex_07_audio` | Play a WAV (Space) and synthesized retro effects (1/2/3); runs muted with no device. |
| `08_hot_reload` | `ex_08_hot_reload` | Edit `assets/reload_me.png` while it runs and watch the texture update live. |
| `09_aseprite` | `ex_09_aseprite` | Load a native `.aseprite`, pack its frames into an atlas, and play a tagged animation. |
| `10_gamepad` | `ex_10_gamepad` | Live gamepad overlay: sticks, buttons, stick-clicks, and rumble on A. |
| `bouncing_ball` | `bouncing_ball` | A faux-3D tumbling beachball shaded through the sprite batcher. |

## Aseprite

`toolkit-aseprite` reads both Aseprite formats into the same `SpriteSheet`:

```rust
// Native binary: frames are flattened and packed into one atlas texture.
let sheet = SpriteSheet::load_aseprite(&mut ctx.gfx, "character.aseprite")?;

// Exported sprite sheet: a PNG atlas plus its Aseprite JSON sidecar.
let sheet = SpriteSheet::from_aseprite_json(&mut ctx.gfx, "sheet.png", "sheet.json")?;

// Play a tag (or `sheet.full_animation()` when a sheet has no tags).
let mut player = AnimationPlayer::new(&sheet, "walk").unwrap();
player.advance(&sheet, dt);
let (uv_min, uv_max) = player.current_uv(&sheet);
```

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

## Development

Common tasks run through [go-task](https://taskfile.dev) (`Taskfile.dist.yaml`):

```sh
task               # list tasks
task ci            # fmt check + clippy + tests
task run:3d        # run an example (see task --list for example:* shortcuts)
task atlas -- --input sprites/ --output a.png --metadata a.json
```

The generated jam templates ship their own `Taskfile.yaml` too.

## Status

Pre-1.0. The 2D runtime, input, audio, assets, text, tilemap, egui overlay, Aseprite
loading, optional depth/MSAA, and a first-cut 3D mesh path are in place. Roadmap and open
workstreams live in the [toolkit epic](https://github.com/sunsided/game-toolkit/issues/15):
ECS integration, an atlas-packer CLI, a `cargo-generate` jam template, and gamepad support.

## License

Licensed under the European Union Public Licence v. 1.2 (EUPL-1.2). See
[LICENSE](LICENSE), or the
[official text](https://joinup.ec.europa.eu/collection/eupl/eupl-text-eupl-12).
