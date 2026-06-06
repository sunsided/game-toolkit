# Agent guide

This is a [game-toolkit](https://github.com/sunsided/game-toolkit) jam project. The whole
runtime API is re-exported from one prelude, so you don't need to read the toolkit's source to
write a game - this file is the cheatsheet. For depth: [docs.rs/game-toolkit](https://docs.rs/game-toolkit),
and the runnable upstream examples (see the bottom of this file).

```rust
use game_toolkit::prelude::*;
```

## The game loop

Implement the `Game` trait and hand it to `run`. Only `init` / `update` / `render` are required:

```rust
struct MyGame { /* your state */ }

impl Game for MyGame {
    fn init(ctx: &mut Context) -> Result<Self> { Ok(Self { /* ... */ }) }
    fn update(&mut self, ctx: &mut Context, dt: f32) { /* input + simulation, dt in seconds */ }
    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) { /* draw */ }

    // Optional hooks (default no-ops):
    fn event(&mut self, ctx: &mut Context, e: &GameEvent) {}        // Resized / FocusChanged / CloseRequested
    fn raw_window_event(&mut self, ctx: &mut Context, e: &winit::event::WindowEvent) -> bool { false } // return true = consumed (egui)
    fn shutdown(&mut self, ctx: &mut Context) {}
}
```

There is no `fixed_update`: set `AppConfig::fixed_timestep` instead and `update` is called with a
fixed `dt` (possibly several times per frame); leave it `None` for variable `dt`.

## Context

`ctx` carries the whole runtime. Fields:

| Field | Type | Use |
|-------|------|-----|
| `ctx.gfx` | `Graphics` | renderer: textures, meshes, cameras, screenshots, `size()` |
| `ctx.input` | `Input` | keyboard / mouse / gamepad |
| `ctx.audio` | `Option<Audio>` | sound - `None` when no device (runs muted); guard with `if let Some(a) = &mut ctx.audio` |
| `ctx.assets` | `Assets` | asset-path resolution + hot-reload watcher |
| `ctx.time` | `Time` | `elapsed`, `delta` (`Duration`), `frame: u64`, `fps: f32` |
| `ctx.window` | `Arc<Window>` | the winit window |
| `ctx.quit()` | | request shutdown |

## Drawing: the Painter

In `render`, get a `Painter` for the frame, then issue draw calls. The painter flushes on drop.
All positions are pixels (top-left origin); colors are `[r, g, b, a]` in `0.0..=1.0`.

```rust
let mut p = frame.painter(&mut ctx.gfx);
p.clear([0.07, 0.08, 0.11, 1.0]);
p.rect(pos, size, color);                 // also rect_outline(pos, size, thickness, color)
p.circle(center, radius, color);          // also circle_outline(center, radius, thickness, color)
p.line(a, b, thickness, color);
p.text(pos, "hi", 24.0, color);           // text always draws on top
p.sprite(tex, pos, size);                 // tex: TextureId from ctx.gfx.load_texture(...)
p.mesh(mesh, model, color);               // 3D, depth-tested under all 2D content
```

Lower-level variants exist: `sprite_ex(tex, SpriteInstance, layer, BlendMode)`,
`circle_ex(center, radius, thickness, color, layer)`, `visible_rect()`. With the `vector`
feature: `p.vector(|scene| { /* vello::Scene */ })`.

## Input

```rust
ctx.input.key_pressed(Key::Space)   // also key_held / key_released
ctx.input.mouse_pressed(MouseButton::Left) // mouse_held / mouse_released / mouse_pos() / mouse_delta() / scroll()
if let Some(pad) = ctx.input.first_gamepad() {
    pad.button_held(Button::South);
    pad.axis(Axis::LeftStickX);     // -1.0..=1.0
}
ctx.input.set_rumble(pad_id, 0.6, 200); // magnitude, duration_ms
```

## Audio (`Option`)

```rust
if let Some(audio) = &mut ctx.audio {
    let id = audio.load_sound("boom.wav")?;   // returns SoundId
    audio.play(id)?;
}
```

## Graphics essentials

```rust
let (w, h) = ctx.gfx.size();
let tex = ctx.gfx.load_texture("player.png")?;        // -> TextureId
let mesh = ctx.gfx.create_mesh(&vertices, &indices);  // &[MeshVertex], &[u16] -> MeshId
ctx.gfx.camera3d.eye = [2.5, 2.0, 3.5];               // perspective Camera3D (3D path)
ctx.gfx.request_screenshot("shot.png");               // saves the next presented frame
// ctx.gfx.camera is the 2D Camera2D.
```

## AppConfig (in `main`)

```rust
fn main() -> Result<()> {
    env_logger::init();
    run::<MyGame>(AppConfig {
        title: "my-jam".into(),
        width: 1280,
        height: 720,
        // Resolve assets relative to the crate so `cargo run` works from any directory:
        asset_root: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        ..Default::default()
    })
}
```

All fields: `title`, `width`, `height`, `vsync` (default `true`), `fixed_timestep: Option<Duration>`
(default `None`), `asset_root` (default `./assets`), `depth_format: Option<wgpu::TextureFormat>`
(default `None`; set `Some(wgpu::TextureFormat::Depth32Float)` for 3D), `msaa_samples` (default `1`;
`4` for anti-aliasing).

## Features

This template enables `ui`. All optional features and what they add to the prelude:

| Feature | Adds | For |
|---------|------|-----|
| `ui` | `Ui`, `egui` | egui debug overlays |
| `aseprite` | `SpriteSheet`, `AnimationPlayer`, `Animation`, `Direction`, `FrameRect` | load `.aseprite` / exported sheets + play tagged animations |
| `ecs` | `ChannelQueue`, `Vec2` (sillyecs glue) | archetype ECS |
| `synth` | `Synth`, `synthie` | chiptune / procedural audio |
| `vector` | `vello` | high-quality 2D vector graphics |

Enable in `Cargo.toml`: `game-toolkit = { ..., features = ["ui", "aseprite"] }`.

## Assets

Put art / audio / fonts under `assets/`. They load relative to `AppConfig::asset_root`. The
`game-toolkit-assets` watcher can hot-reload changed files; `ctx.gfx.refresh_textures(paths)`
re-uploads changed textures live.

## Commands

This project ships its own `Taskfile.yaml` (needs [go-task](https://taskfile.dev)):

```sh
task run                 # cargo run (PROFILE=release for a release build)
task build               # cargo build
task test                # cargo test
task lint                # clippy, warnings as errors
task fmt                 # cargo fmt --all
task check               # fmt --check AND clippy -D warnings  (run before you call it done)
```

Plain `cargo run` works too. `task check` here enforces `rustfmt` - keep code formatted.

## Conventions

- Reach for the prelude first; don't hand-roll types the toolkit already provides.
- Set `asset_root` via `CARGO_MANIFEST_DIR` (above) so the binary runs from anywhere.
- 2D-first. The 3D mesh path needs `AppConfig::depth_format = Some(..)` (the jam-3d template's
  `src/main.rs` shows a full cube + 2D HUD).
- Keep the `Game` struct small; push reusable systems into modules.

## Upstream examples

Browse the [game-toolkit repo](https://github.com/sunsided/game-toolkit) `examples/` for runnable
references: `01_window`, `02_sprite`, `03_primitives`, `04_text`, `05_tilemap`, `06_egui`,
`07_3d`, `07_audio`, `08_hot_reload`, `09_aseprite`, `10_gamepad`, `11_ecs`, `12_vector`,
`bouncing_ball`.
