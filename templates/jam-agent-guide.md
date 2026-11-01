<!-- source note
Source of truth for the game-toolkit guide. `task sync:templates` (tools/sync-guide.sh) stamps
it into templates/*/CLAUDE.md whole, and into crates/prelude/src/guide.md (docs.rs) without the
agent-only blocks. Edit here, never the copies. No literal double braces (cargo-generate).
-->
<!-- agent-only -->
# Agent guide

This is a [game-toolkit](https://github.com/sunsided/game-toolkit) jam project. The whole
runtime API is re-exported from one prelude, so you don't need to read the toolkit's source to
write a game - this file is the cheatsheet. For depth: [docs.rs/game-toolkit](https://docs.rs/game-toolkit),
and the runnable upstream examples (see the bottom of this file).
<!-- /agent-only -->

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
fixed `dt` (zero, one or several times per frame); leave it `None` for variable `dt`. `render`
runs once per frame either way; with a fixed step, `ctx.time.alpha` (`0.0..1.0`) is how far the
frame is into the next step, for interpolating positions. `dt` is capped at
`AppConfig::max_frame_time` (250 ms), so a long frame or time in the background slows the game
down for one frame instead of fast-forwarding it. With a sillyecs world, leave `fixed_timestep`
at `None`: the world runs its own fixed phases (see the `game-toolkit-ecs` docs).

## Context

`ctx` carries the whole runtime. Fields:

| Field | Type | Use |
|-------|------|-----|
| `ctx.gfx` | `Graphics` | renderer: textures, meshes, cameras, screenshots, `size()` |
| `ctx.input` | `Input` | keyboard / mouse / gamepad |
| `ctx.audio` | `Option<Audio>` | sound - `None` when no device (runs muted); guard with `if let Some(a) = &mut ctx.audio` |
| `ctx.assets` | `Assets` | asset bytes (`read`), path resolution, hot-reload watcher |
| `ctx.time` | `Time` | `elapsed`, `delta` (`Duration`), `frame: u64`, `fps: f32`, `alpha: f32` (fixed-step interpolation) |
| `ctx.rng` | `Rng` | deterministic xorshift64* RNG: `next_f32()`, `range_u32(a..b)`, `range_usize(a..b)`, `reseed(seed)` |
| `ctx.window` | `Arc<Window>` | the winit window |
| `ctx.quit()` | | request shutdown |
| `ctx.load_texture(p)` | `Result<TextureId>` | PNG from the asset root (native + web, hot-reloadable natively) |
| `ctx.load_sound(p)` | `Result<Option<SoundId>>` | wav/ogg/flac/mp3 from the asset root; `Ok(None)` when muted |

## Drawing: the Painter

In `render`, get a `Painter` for the frame, then issue draw calls. The painter flushes on drop.
All positions are pixels (top-left origin). Colors are `impl Into<Color>`: a `Color`
(`Color::WHITE`, `Color::hex(0xff8800)`, `Color::rgb8(..)`, `.with_alpha(0.5)`, `.lerp(..)`) or
an `[r, g, b, a]` array in `0.0..=1.0`. Layers are `impl Into<Layer>`: a bare integer or a named
`const HUD: Layer = Layer(3);`.

```rust
let mut p = frame.painter(&mut ctx.gfx);
p.clear([0.07, 0.08, 0.11, 1.0]);
p.rect(pos, size, color);                 // also rect_outline(pos, size, thickness, color)
p.circle(center, radius, color);          // also circle_outline(center, radius, thickness, color)
p.line(a, b, thickness, color);
p.quad([a, b, c, d], color, layer);       // filled convex quad; also polygon(&points, color, layer)
p.triangles(&verts, &indices, color, layer); // raw filled triangle list
// gradients: quad_colored(points, [c0, c1, c2, c3], layer), polygon_colored, triangles_colored
// blend modes: quad_ex / polygon_ex / triangles_ex(..., layer, BlendMode::Additive)
// textured, perspective-correct (road segments, mode-7 floors). UVs past 0..1 clamp, or
// tile after ctx.gfx.set_texture_wrap(tex, TextureWrap::Repeat) (keep atlases on Clamp):
p.quad_textured(tex, [a, b, c, d], [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], color, layer, BlendMode::Alpha);
p.text(pos, "hi", 24.0, color);           // unlayered: on top of everything
p.text_ex(pos, "hi", 24.0, color, layer); // layered: covered by higher layers (pause overlay)
let [w, h] = p.measure_text("hi", 24.0);  // also ctx.gfx.measure_text, outside render
p.sprite(tex, pos, size);                 // tex: TextureId from ctx.load_texture(...)
p.sprite_region(tex, src_pos, src_size, pos, size); // pixel rect of an atlas
p.mesh(mesh, model, color);               // 3D, depth-tested under all 2D content
```

Builders for draws with several options; each queues its draw when dropped, at the end
of the statement:

```rust
p.draw_sprite(tex, pos, size).region(src_pos, src_size).layer(CARS).flip_x_if(facing_left);
p.draw_rect(pos, size).color(Color::BLACK.with_alpha(0.6)).layer(Layer::TOP); // .outline(2.0)
p.draw_text([w / 2.0, 20.0], "PAUSED").size(48.0).align(Align::Center).layer(HUD);
```

Sprite builder: `region` (pixels), `uv`, `color`, `rotation`, `flip_x` / `flip_y` /
`flip_x_if(bool)`, `layer`, `blend`. Text builder: `size` (default 16), `color`, `align`
(`Left` / `Center` / `Right`, around `pos.x`), `layer` (default: unlayered).

Lower-level variants take an explicit layer: `sprite_ex(tex, SpriteInstance, layer, BlendMode)`,
`rect_ex(pos, size, color, layer, BlendMode)` (also `rect_outline_ex`, `line_ex`),
`circle_ex(center, radius, thickness, color, layer)`. Also `visible_rect()`. With the `vector`
feature: `p.vector(|scene| { /* vello::Scene */ })`.

Draw order: lower layers draw under higher ones. Within one layer, call order holds only for
draws with the same blend mode and texture, and untextured shapes (`rect`, `line`) sort under
textured sprites. Filled shapes (`quad*`, `polygon*`, `triangles*`) draw above a layer's
sprites and rects and below its circles, grouped by blend mode and texture like sprites; plain
alpha-blended shapes on one layer keep call order. Layered text (`text_ex`, or `draw_text`
with `.layer`) draws last within its layer; `p.text` draws above every layer. To draw a HUD
rect over a sprite scene, give it a higher layer with `rect_ex` instead of relying on call
order. Text positions are screen pixels: text doesn't follow the 2D camera.

Atlas sprites with a layer or mirroring: `p.draw_sprite(..).region(..)` above, or
`SpriteInstance::at(pos, size).with_pixel_region(src_pos, src_size).flip_x()` into `sprite_ex`
(the painter looks the texture size up). `flip_x` / `flip_y` work on any UV range.

## Input

```rust
ctx.input.key_pressed(Key::Space)   // also key_held / key_released
ctx.input.mouse_pressed(MouseButton::Left) // mouse_held / mouse_released / mouse_pos() / mouse_delta() / scroll()
ctx.input.any_pressed()             // "press any key": any non-modifier key, mouse button or pad button
// also any_key_pressed / any_mouse_pressed / keys_pressed() (iterator) / pad.any_button_pressed()
if let Some(pad) = ctx.input.first_gamepad() {
    let id = pad.id();
    let move_x = pad.axis(Axis::LeftStickX);   // -1.0..=1.0, feed into movement
    if pad.button_held(Button::South) {        // also button_pressed / button_released
        ctx.input.set_rumble(id, 0.6, 200);    // magnitude, duration_ms
    }
}
```

## Audio (`Option`)

```rust
// In init: load once. `None` = no audio device, the game runs muted.
let boom: Option<SoundId> = ctx.load_sound("boom.wav")?;
let music: Option<SoundId> = ctx.load_sound("music.ogg")?;

// Later: one-shot effect.
if let (Some(audio), Some(id)) = (&mut ctx.audio, boom) {
    let _ = audio.play(id); // -> Result<SoundHandle, AudioError>
}

// Music, e.g. in init: loop at 30% volume. `deferred` queues it until audio may start (web).
if let (Some(audio), Some(id)) = (&mut ctx.audio, music) {
    let handle = audio.play_with(id, PlayOptions::looped().volume(0.3).deferred())?;
    handle.set_volume(0.5); // linear; also set_volume_db, fade_volume(v, Duration)
    handle.pause();         // resume, stop, is_playing, is_paused, is_finished
}
```

Keep the `SoundHandle` to control the sound later; dropping it lets the sound play out.
`play_samples` / `play_samples_with` take mono PCM (e.g. from `Synth`). For audio produced while
it plays (a running synth, an emulated sound chip), `play_stream(source, sample_rate)` /
`play_stream_with` call `source` (any `FnMut(&mut [f32]) + Send`, or an `AudioStream`) for mono
samples on the audio thread, and return a `StreamHandle` (`set_volume`, `pause`, `resume`,
`stop`). It works on the web too. The prelude re-exports `kira` for anything else
(`handle.with_kira(|h| ...)`); don't add your own `kira` dependency.

In the browser, audio starts on the player's first key press / click / touch (a browser rule);
before that `play` fails with `AudioError::NotStarted` (check `audio.is_started()`), so don't
`?` it in `update` - log or ignore it, or use `PlayOptions::deferred` for music.

## Randomness

`ctx.rng` is a deterministic xorshift64* generator: the same `AppConfig::random_seed` reproduces
the same run, which is handy for debugging and replays.

```rust
let r = ctx.rng.next_f32();                  // [0.0, 1.0)
let face = ctx.rng.range_u32(1..7);          // 1..=6
let i = ctx.rng.range_usize(0..items.len()); // panics if the range is empty
ctx.rng.reseed(42);                          // restart the stream
```

Not cryptographically secure - gameplay use only (procedural generation, jitter, AI variation).

## Graphics essentials

```rust
let (w, h) = ctx.gfx.size();
let tex = ctx.load_texture("player.png")?;            // -> TextureId (relative to asset_root)
let mesh = ctx.gfx.create_mesh(&vertices, &indices);  // &[MeshVertex], &[u16] -> MeshId
ctx.gfx.camera3d.eye = [2.5, 2.0, 3.5];               // perspective Camera3D (3D path)
ctx.gfx.request_screenshot("shot.png");               // saves the next presented frame (native only)
// ctx.gfx.camera is the 2D Camera2D.
```

## AppConfig (in `main`)

```rust
fn main() -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    env_logger::init();
    run::<MyGame>(AppConfig {
        title: "my-jam".into(),
        width: 1280,
        height: 720,
        // The crate's assets/ natively (so `cargo run` works from any directory),
        // "assets" next to index.html on the web:
        asset_root: asset_root!(),
        canvas_id: Some("game".into()), // web: render into index.html's <canvas id="game">
        ..Default::default()
    })
}
```

All fields: `title`, `width`, `height`, `vsync` (default `true`), `fixed_timestep: Option<Duration>`
(default `None`), `max_frame_time: Duration` (default 250 ms; caps `dt`), `asset_root` (default `./assets`), `web_preload: Option<Vec<String>>` (web: files to
fetch instead of `assets/manifest.txt`), `canvas_id: Option<String>` (web: existing canvas; `None`
appends one), `random_seed: u64` (seeds `ctx.rng`; default is
a fixed nonzero constant, so runs are reproducible until you change it), `depth_format:
Option<wgpu::TextureFormat>` (default `None`; set `Some(wgpu::TextureFormat::Depth32Float)` for 3D),
`msaa_samples` (default `1`; `4` for anti-aliasing).

## Features

<!-- agent-only -->
This template enables `ui`.
<!-- /agent-only -->
All optional features and what they add to the prelude:

| Feature | Adds | For |
|---------|------|-----|
| `ui` | `Ui`, `egui` | egui debug overlays |
| `aseprite` | `SpriteSheet`, `AnimationPlayer`, `Animation`, `Direction`, `FrameRect`, `LoadAseprite` (`ctx.load_aseprite(p)`) | load `.aseprite` / exported sheets + play tagged animations |
| `ecs` | `ChannelQueue`, `Vec2` (sillyecs glue) | archetype ECS |
| `synth` | `Synth`, `synthie` | chiptune / procedural audio |
| `vector` | `vello` | high-quality 2D vector graphics (WebGPU only: skipped on WebGL2) |

Enable in `Cargo.toml`: `game-toolkit = { ..., features = ["ui", "aseprite"] }`.

## Fast dev builds

Unoptimized dependencies make debug builds of a game slow enough to be unplayable (audio
decoding, image decoding, thousands of sprites per frame). Optimize just the dependencies, so
your own code still compiles fast and debugs well. The jam templates ship this in `Cargo.toml`:

```toml
[profile.dev.package."*"]
opt-level = 3
```

## Assets

Put art / audio / fonts under `assets/`. They load relative to `AppConfig::asset_root`; use the
`ctx.load_*` helpers (or `ctx.assets.read(p)` for raw bytes) so the same code works on the web.
Natively, `ctx.assets.watch()` + `ctx.assets.drain_changes()` hot-reload changed files, and
`ctx.gfx.refresh_textures(paths)` re-uploads textures loaded with `ctx.load_texture` live.

## Shipping to the browser

The same code builds for the web (wasm32, WebGPU with a WebGL2 fallback) via
[trunk](https://trunkrs.dev): `rustup target add wasm32-unknown-unknown`,
`cargo install trunk --locked`, then `task web` (dev server) or `PROFILE=release task web:build`
in a jam-template project, or `trunk serve` / `trunk build --release` directly (zip `dist/`
and upload it to itch.io as an HTML game).

What's different on the web:

- **Assets are preloaded.** Before `init`, the toolkit fetches every file listed in
  `assets/manifest.txt`; `task web` / `task web:build` regenerate it (`task assets:manifest`).
  A file missing from the manifest fails to load at runtime. Native debug builds warn
  (`asset ... is not listed in .../manifest.txt`) when the game loads a file the manifest
  doesn't list, so watch the log after adding assets.
- **No filesystem paths.** Load through `ctx.load_texture` / `ctx.load_sound` /
  `ctx.load_aseprite` / `ctx.assets.read`, not `std::fs` or the path-taking `ctx.gfx.load_texture`
  (native only).
- **Audio after the first gesture**, gamepads after the first button press (browser rules).
- **Not available:** hot-reload (`watch` is a no-op), `request_screenshot`, `ctx.quit()` just
  stops the loop. `std::time::Instant::now()` panics in wasm - use `ctx.time`.
- Logs and panics go to the browser console.

<!-- agent-only -->
## Commands

This project ships its own `Taskfile.yaml` (needs [go-task](https://taskfile.dev)):

```sh
task run                 # cargo run (PROFILE=release for a release build)
task build               # cargo build
task test                # cargo test
task lint                # clippy, warnings as errors
task fmt                 # cargo fmt --all
task check               # fmt --check AND clippy -D warnings  (run before you call it done)
task check:wasm          # clippy for the browser target
task web                 # serve in the browser at http://localhost:8080 (trunk)
task web:build           # browser build into dist/ (PROFILE=release for upload)
```

Plain `cargo run` works too. `task check` here enforces `rustfmt` - keep code formatted.

## Conventions

- Reach for the prelude first; don't hand-roll types the toolkit already provides.
- Set `asset_root: asset_root!()` (above) so the binary runs from anywhere and on the web.
- 2D and 3D share one frame: meshes draw first, depth-tested, and all 2D content composites on
  top. The 3D path needs `AppConfig::depth_format = Some(..)` (the jam-3d template's
  `src/main.rs` shows a full cube + 2D HUD).
- Keep the `Game` struct small; push reusable systems into modules.
<!-- /agent-only -->

## Upstream examples

Browse the [game-toolkit repo](https://github.com/sunsided/game-toolkit) `examples/` for runnable
references: `01_window`, `02_sprite`, `03_primitives`, `04_text`, `05_tilemap`, `06_egui`,
`07_3d`, `07_audio`, `08_hot_reload`, `09_aseprite`, `10_gamepad`, `11_ecs`, `12_vector`,
`13_web` (runs natively and in the browser), `bouncing_ball`.
