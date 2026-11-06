# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `AppConfig::required_limits` and `AppConfig::required_features` let a game request more
  than the toolkit's own device limits and features (e.g. more storage buffers per shader
  stage for compute). They are merged with what the toolkit needs; when the adapter can't
  provide them, init fails with a `DeviceRequirementsError` that names each missing limit and
  feature. `AppConfig::device_hook` edits the device request with the adapter at hand (#87).
- `Graphics::adapter_info()`, `Graphics::backend()` and `Graphics::supports_compute()`, e.g.
  to show a "WebGPU required" message on WebGL2 instead of failing.
- `AppConfig::preferred_limits` / `preferred_features`: limits and features a game would like
  but can do without. Each limit is requested up to what the adapter offers and each feature
  only where the adapter has it, so they never fail init; read the result from
  `ctx.gfx.device().limits()` / `features()`. E.g. 8 storage buffers per stage on WebGPU and
  native, WebGL2's own limits there.
- `AppConfig::continue_without_requirements` (default `false`): when the adapter can't meet
  `required_limits` / `required_features`, start anyway without them and report the
  `DeviceRequirementsError` through `Graphics::unmet_requirements()`, so `Game::init` can draw
  a "WebGPU required" screen through the painter instead of the game never starting.
- `game-toolkit-compute`, a new crate (umbrella feature `compute`, `game_toolkit::compute`)
  for GPU simulations: `Kernel` builds a WGSL compute pipeline with its bind-group layout and
  dispatches it over a grid (workgroup counts rounded up) into the frame's encoder;
  `PingPong<T>` holds a read/write pair of storage buffers with bind groups for both
  orderings; `ReadbackRing<T>` reads small results back through a ring of staging buffers a
  few frames later without blocking, so it works on WebGPU; `StorageTexture` is an RGBA
  texture a kernel writes and the `Painter` draws. `ComputeSubmitter` offloads a simulation:
  steps go in their own submissions from `update`, decoupled from the frame, and are skipped
  while the GPU still has `max_in_flight` of them running, so a heavy simulation can't stall
  rendering; `PingPong` buffers carry `VERTEX` usage so render passes can draw straight from
  simulation state. New example `16_compute`: Game of Life offloaded to the GPU with a
  live-cell count read back (#92).
- `Graphics::register_texture` makes a game's own `wgpu::Texture` (a compute output, a render
  target) drawable as a `TextureId` (#92).
- `game-toolkit-compute` without its default `gfx` feature depends only on `wgpu` (re-exported
  as `game_toolkit_compute::wgpu`) and `bytemuck`, not on game-toolkit-gfx or winit: `Kernel`,
  `PingPong`, `ReadbackRing` and `ComputeSubmitter` work on any wgpu device, e.g. in a
  simulation crate or a headless runner. `StorageTexture` needs `gfx`. CI checks the
  dependency tree.

- Virtual resolution (#88): `AppConfig::virtual_resolution: Option<(u32, u32)>` makes the
  painter (sprites, shapes, text, tilemaps, vector art, 3D meshes) draw into a canvas of that
  size, which is scaled onto the window with `AppConfig::scale_mode` (`ScaleMode::Integer`,
  the default, by whole multiples with a `border_color` border; `Fit`, fractional; `Stretch`)
  and `scale_filter` (`Nearest` or `Linear`). All of them can change at runtime through
  `Graphics::set_virtual_resolution`, `set_scale_mode`, `set_scale_filter` and
  `set_border_color`. `Graphics::canvas_rect()` says where the canvas lands,
  `Graphics::canvas_view()` and `Frame::target_view()` give games the canvas for their own
  passes. New example `17_virtual_resolution`.
- `Input::mouse_pos`, `mouse_delta` and touch positions are reported in canvas pixels while a
  virtual resolution is set (the toolkit installs a `PointerMapping` each frame);
  `Input::mouse_pos_window()` keeps the raw window position.
- `Graphics::window_size()`: the window's size, whatever the drawing size is.
- The pointer mapping is re-synced after every `update` step, so a virtual resolution or scale
  mode changed in `update` applies to the next step's and `render`'s `mouse_pos` and touches
  rather than a frame later.

- Post-processing (#89): `Graphics::add_post_effect(stage, effect)` runs full-screen effects
  after the scene, in `PostStage::Canvas` (at the virtual resolution, before the upscale) or
  `PostStage::Window` (after it), ping-ponging between textures, with an optional history
  texture holding the stage's previous output. `ShaderEffect` wraps one WGSL fragment shader
  (the toolkit prepends `SHADER_PRELUDE` with the bindings and a `post` uniform of frame
  values), with an optional parameter block; implement `PostEffect` for anything else. Effects
  can be removed, switched off and on, and reached by type with `post_effect_mut`.
- Feature `crt`: `CrtEffect` with `CrtSettings` presets (`subtle`, `arcade`, `consumer_tv`,
  `pc_monitor`): scanlines locked to canvas rows, aperture grille or shadow mask, bloom,
  phosphor afterglow, curvature, vignette and sharp-bilinear sampling for fractional scales.
  New example `18_post`.
- `PostPass::indices`: with indexed colour on, every post effect (both stages) gets the
  canvas's palette indices before the lookup, at canvas size. `ShaderEffect` shaders get them as
  `indices_tex`, with `index_at(canvas_px)`, `index_in` / `index_out` and the `post.indexed`
  flag in the prelude.
- `CrtSettings::bloom_source`: `BloomSource::Luminance` (default, as before) or
  `BloomSource::IndexMask` (`BloomSource::indices(48..56)`), where only the listed palette
  indices glow, so a lava ramp blooms and equally bright snow doesn't. It falls back to
  luminance without an indexed canvas.

- Bitmap pixel fonts (#91): `BitmapFont`, drawn with `Painter::bitmap_text` / `bitmap_text_ex`
  at whole-number scales on whole pixels, through the sprite batcher so text sorts with
  sprites by layer. Load one from a glyph sheet (`BitmapFont::from_grid` with a `GridLayout`,
  monospaced or trimmed per glyph; `Context::load_bitmap_font`) or from an AngelCode BMFont
  text `.fnt` with kerning (`BitmapFont::from_bmfont`, `Context::load_bmfont`).
  `BitmapFont::measure` returns whole-pixel sizes. A built-in 5 × 7 ASCII font (6 × 8 cells,
  drawn for the toolkit) works out of the box: `Painter::pixel_text`, `Graphics::builtin_font`.

- Indexed colour (#90), feature `indexed`: `Graphics::enable_indexed(&Palette)` makes the
  canvas hold palette indices, resolved through a palette lookup texture (`Rgba8UnormSrgb`, so
  authored colours come out exactly) before post effects and the upscale. `PALETTE_ROWS`
  palette rows with `set_palette` and `set_palette_blend` for crossfades; `PaletteCycle`
  rotates index ranges over time (`add_palette_cycle`). Draw indices with `Color::index` and
  textures from indexed PNGs, decoded without RGBA conversion (`IndexedImage::from_png`,
  `Graphics::load_indexed_texture`, `Context::load_indexed_texture`). New example `19_palette`.
- `bayer4` / `bayer8` ordered-dithering helpers in the post-effect shader prelude, also as
  `BAYER_WGSL` for games' own shaders.

- `Painter::custom(layer, |pass| ...)` records a game's own GPU work at a painter layer: it
  runs after the clear and the layers below and before that layer's sprites, shapes and text,
  so a shader-generated background (a map, a sky) can sit under painter content in the same
  encoder and submission. `CustomPass::begin_render_pass` attaches the target the way the
  painter does (load, MSAA resolve, depth); `Graphics::multisample()` and
  `Graphics::depth_stencil()` give the matching pipeline state.
- `INDEX_WGSL` (feature `indexed`): `index_out(i)` / `index_in(texel)`, the documented way for
  custom passes and post effects to write and read indices on the indexed canvas. The resolve
  pass uses the same helpers. `19_palette` now draws its dithered sky in a custom pass.

### Changed

- `Graphics::size()` is the size the painter draws at: the virtual resolution when one is set,
  otherwise (as before) the window size. Layout code keeps working with a canvas.
- `Graphics::flush_pending` also scales the canvas onto the window, so egui and other window
  overlays drawn after it land on top at window resolution. `game-toolkit-ui` sizes egui by
  `window_size()`.

- **Breaking:** `Graphics::new` takes a `GraphicsConfig` instead of positional `vsync`,
  `depth_format` and `msaa_samples` arguments. `AppConfig::graphics_config()` builds it.

## [0.10.0] - 2026-11-02

### Added

- Web: the audio stops while the host page has the game's canvas out of the document (an SPA
  route change, a component unmounting). A `MutationObserver` suspends the `AudioContext` the
  moment the canvas is detached and resumes it when it's back; no frame is needed, which
  matters because the browser sends none to a detached canvas. Games hear about it as
  `GameEvent::CanvasAttached(bool)` on the next frame and through `Context::canvas_attached()`
  (#81).
- Web: audio starts without a click on the canvas when the page already had a user gesture and
  the browser allows it (the player clicked through to the game on the host site). A throwaway
  `AudioContext` probes whether autoplay is allowed; where `navigator.userActivation` exists,
  only after a gesture. Chromium answers at once, so audio is started before `Game::init`;
  Firefox and WebKit report a fresh context `suspended` for ~300 ms even when allowed, so the
  probe is polled each frame and gives up after 1.5 s. `Audio::is_started()` tells the game,
  e.g. to skip a "click to start" screen (#82).
- `Context::start_audio_if_allowed()` runs the probe again, for when the host page says the
  player is back and may have interacted with the page since. A no-op natively.
- `Audio::suspend` / `Audio::resume` / `Audio::is_suspended` pause and continue everything that
  plays, on every target. Independent of the detach handling: reattaching the canvas doesn't
  undo a game's own suspend (#81).

### Changed

- **Breaking:** `GameEvent` is now `#[non_exhaustive]` and has the new `CanvasAttached`
  variant, so exhaustive matches on it need a wildcard arm.
- On the web, `Audio` runs on the toolkit's own kira backend (`DetachHandle`), the same cpal
  setup as kira's but with a stream the toolkit can suspend. All sounds now play through a sub
  track (the bus `Audio::suspend` pauses) instead of directly on the main track.

### Known issues

- Web: when the audio output can't be opened (e.g. a headless browser with no output device,
  which reports a destination with 0 channels), the game continues muted as before, but cpal
  leaves the `AudioContext` it created for the failed stream running. Fixed upstream in
  [RustAudio/cpal#1420](https://github.com/RustAudio/cpal/pull/1420); it reaches the toolkit
  once kira moves to a cpal release that includes it.

## [0.9.0] - 2026-11-02

### Added

- `synth::Preset`, synthie's typed preset bank (re-exported with `UnknownPreset`), with six new
  patches: Drive Guitar, Rock Lead, Music Box, Tri Bass, Brass and Echo Lead. The guide,
  README and examples now use `Preset::PwmLead.params()` instead of
  `preset("PWM Lead").unwrap()` (#79).
- Chord cells in the song notation (`E2+B2+E3`, `E2^07c!`), from synthie 0.8.

### Changed

- **Breaking:** the `synthie` re-export is now 0.8 (was 0.7). Among other changes,
  `synthie::pattern::Action` has a new `Chord` variant, so exhaustive matches on it need an arm.
- `synth::preset(name)` parses the name through `Preset::from_str` instead of building the
  whole bank per call. It is meant for names from data (config files, a UI) and still returns
  `None` for unknown names.

## [0.8.0] - 2026-11-02

### Added

- Live chiptune music with the `synth` feature: `Audio::play_synth(song, PlayOptions)` plays a
  synthie `Song` (lanes of tracker notation at a tempo) through kira, sequenced
  sample-accurately on the audio thread, natively and on the web. `play_synth_sound` takes a
  hand-built `SynthSoundData` (custom sequencer, channel count). `PlayOptions` decide looping,
  volume and deferred start like every other play call; a song played once ends by itself after
  its tails (#74).
- `SynthHandle`: volume and fades, pause / resume / stop, and live events (`send`, `drum`,
  `note_on` / `note_off`). A deferred synth queues all of it until audio starts. Dropping the
  last clone stops the synth (#74).
- `Synth::render_phrase` renders timed events (overlapping notes, drum hits) sample-accurately
  into one PCM buffer, and `Synth::render_song` renders one pass of a `Song`, for jingles (#75).
- A public `synth` module (in the prelude) re-exporting what writing a song needs (`Song`,
  `Tempo`, `ChannelNo`, `DrumHit`, `AudioEvent`, ...) plus `preset(name)` for the built-in
  patches.
- `examples/07_audio` plays a synth song (C for a live crash) and a `render_phrase` jingle (5);
  `examples/13_web` plays deferred synth music in the browser (M pauses).
- The guide and README document the notation and both paths; the README links Cheesed! as a
  game made with the toolkit.

### Changed

- **Breaking:** synthie, re-exported as `game_toolkit::prelude::synthie` with the `synth`
  feature, is now 0.7 (was 0.6). Code naming synthie types through the re-export may need
  updating.
- The `synth` feature now enables synthie's `kira` and `arp` features.

## [0.7.0] - 2026-11-01

### Added

- "Press any key" queries on `Input`: `any_pressed` (any non-modifier key, mouse button or
  gamepad button this frame), `any_key_pressed`, `any_mouse_pressed`, and `keys_pressed()`,
  an iterator over every key that went down this frame; plus `Gamepad::any_button_pressed`.
  Modifier keys alone don't count as "any key", so holding Shift or Alt-Tabbing away doesn't
  skip a screen (#67).
- Touch input on `Input`: `touches()` (fingers currently down) and `touch(id)`, plus
  `touches_started()` / `touches_ended()` for this frame's edges. Each `Touch` carries its
  `id`, `pos`, `start_pos` (tap vs. swipe) and per-frame `delta`, in physical pixels like
  `mouse_pos`. Cancelled touches vanish without counting as ended. `any_pressed` now includes
  a finger going down (`any_touch_pressed`) (#69).
- `examples/15_touch`: every finger as a ring, taps vs. swipes, and counters, for checking
  touch on a phone or tablet.
- `task web:lan` (toolkit and both templates) serves the web build on all interfaces, so a
  phone on the same network can open it; trunk prints the LAN URLs. Over plain http browsers
  offer only WebGL2, not WebGPU.
- Opt-in touch-as-mouse: `AppConfig::touch_emulates_mouse` / `Input::set_touch_emulates_mouse`
  let the first finger move `mouse_pos` and press, hold and release `MouseButton::Left`, so a
  mouse-only game works on a phone. Off by default. No effect on Windows, where the OS already
  turns touches into mouse events. `15_touch` has a button to toggle it (#70).

### Changed

- The web example and both templates set `touch-action: none` on the canvas, so the browser
  hands touches to the game instead of scrolling or zooming the page (#69).

## [0.6.1] - 2026-10-31

### Added

- `Audio::play_stream` / `play_stream_with` play audio generated while it plays (a running
  synth, an emulated sound chip): an `AudioStream` (any `FnMut(&mut [f32]) + Send`) is asked
  for mono samples on the audio thread and resampled to the output rate, without a decoder
  thread, so it works on the web. `StreamHandle` controls volume, pause and stop, and ramps
  each change over one audio block so it does not click (#65, #66).
- `Graphics::update_texture_rgba` overwrites an existing texture's pixels in place, for content
  that changes every frame such as a software-rendered framebuffer (#64).
- `examples/14_framebuffer` draws a CPU-rendered plasma through `update_texture_rgba`, and
  `examples/07_audio` plays a generated tone through `play_stream` (key 4) (#66).

## [0.6.0] - 2026-10-28

### Changed

- **Breaking:** synthie 0.6 behind the `synth` feature. `Synth`'s API is unchanged, but the
  re-exported `game_toolkit::synthie` is now 0.6, so games that name synthie types and depend on
  `synthie = "0.4"` themselves need `synthie = "0.6"`.
- `Synth` renders with synthie's voice normalization off, so a note is no longer divided by the
  polyphony (4) before the peak normalization. With `target_peak` at `0.0` the raw output is
  about four times louder than before.

### Fixed

- `Synth` no longer lets a previous render's reverb, delay or chorus tail bleed into the next
  one (synthie 0.6 clears the FX on panic).

## [0.5.0] - 2026-10-25

### Added

- `SoundHandle`, a toolkit-owned handle to a playing sound: `set_volume` (linear) /
  `set_volume_db`, `fade_volume` / `fade_volume_db`, `pause`, `resume`, `stop`, `is_playing`,
  `is_paused`, `is_finished`, and `with_kira` for the underlying kira handle (#38).
- `Audio::play_with` / `play_samples_with` and `PlayOptions` (`looped`, `volume`, `deferred`),
  so looping music at a set volume is one call (#38).
- `PlayOptions::deferred` queues a sound while audio hasn't started (web, before the first user
  gesture) and starts it once it has; its handle is pending until then (#39).
- `kira`, `SoundHandle`, `PlayOptions` and `AudioError` are re-exported from the prelude, so
  games don't need their own `kira` dependency (#51).
- `examples/07_audio` plays a looping track (M pauses, Up/Down change its volume).
- `AppConfig::max_frame_time` (default 250 ms) caps the time one frame advances the game;
  `Duration::ZERO` disables the cap (#45).
- `Time::alpha`: with a fixed timestep, how far the frame is into the next step, for
  interpolating motion in `render` (#45).
- `ChannelQueue` implements sillyecs' `CommandSender` / `CommandReceiver` for any command type,
  so `ChannelQueue<WorldCommand<U>>` is a world command queue without any impls in the game
  (#43).
- The jam agent guide is published as the umbrella crate's `game_toolkit::guide` module on
  docs.rs, generated from the same source as the templates' `CLAUDE.md` (agent-only sections
  are left out). The README links it and gains a Drawing section with the `Painter` calls and
  draw-order rules (#49).
- The jam templates optimize dependencies in dev builds (`[profile.dev.package."*"]
  opt-level = 3`); the README and guide recommend it for other projects (#48).
- Per-texture wrap modes: `Graphics::set_texture_wrap(id, TextureWrap::Repeat)` (or
  `MirrorRepeat`) makes UVs outside `0..1` tile, so a scrolling road or a repeating floor is one
  `quad_textured` instead of a quad per tile. `TextureWrap::Clamp` stays the default, so sprites
  and atlas regions are unaffected; the mode survives hot-reloads. `Graphics::texture_wrap`
  reads it back. `examples/03_primitives` scrolls a tiled road (#37).
- Native debug builds check asset loads against the web preload list (`manifest.txt` under
  the asset root, or `AppConfig::web_preload`) and log a warning, once per file, for files
  the web build wouldn't preload. `Assets::check_web_listed` runs the check for files read by
  path; `Assets::set_web_preload` replaces the list; `parse_manifest` is public (#46).
- `Color` (`rgb` / `rgba` / `rgb8` / `rgba8` / `hex` / `hex_rgba`, named constants,
  `with_alpha`, `lerp`) and `Layer(i16)` (`DEFAULT`, `BOTTOM`, `TOP`). Painter calls take
  `impl Into<Color>` and `impl Into<Layer>`, so existing `[f32; 4]` and integer arguments keep
  compiling (#42).
- Draw builders that queue on drop: `Painter::draw_sprite` (`region`, `uv`, `color`,
  `rotation`, `flip_x` / `flip_y` / `flip_x_if`, `layer`, `blend`), `Painter::draw_rect`
  (`color`, `outline`, `layer`, `blend`) and `Painter::draw_text` (`size`, `color`, `align`,
  `layer`) (#42).
- `SpriteInstance::with_pixel_region(src_pos, src_size)`: an atlas region in pixels, resolved
  against the texture's size when `sprite_ex` draws it, so a layered or mirrored atlas sprite
  needs no `texture_size` lookup (#41).
- Layered text: `Painter::text_ex(pos, s, size_px, color, layer)` sorts with sprites and
  shapes, drawing last within its layer. `Painter::text` still draws above every layer (#40).
- `Graphics::measure_text` / `Painter::measure_text` return a string's `[width, height]` with
  the same layout as drawing, and `Align` (`Left` / `Center` / `Right`) aligns builder text
  around its position (#47).
- `examples/04_text` right-aligns, sizes a box to its label and fades a pause overlay over
  layered text; `examples/02_sprite` draws a mirrored pixel region with the builder.

### Changed

- **Breaking:** `Audio::play` and `Audio::play_samples` return `Result<SoundHandle, AudioError>`
  instead of `anyhow::Result<StaticSoundHandle>`. `AudioError::NotStarted` tells "audio not
  started yet" apart from real failures (`UnknownSound`, `Backend`) (#39). `play` and
  `play_samples` are documented, including the web caveat (#51).
- **Breaking:** sillyecs 0.2, and `game-toolkit-ecs` depends on it. Games need `sillyecs = "0.2"`
  and `sillyecs-build = "0.2"`, and drop their own command-queue impls for `ChannelQueue`, which
  now conflict with sillyecs' blanket impls. The `game-toolkit-ecs` docs ask for sillyecs' `wasm`
  feature on the web (#44), and explain how sillyecs' fixed phases relate to `fixed_timestep`,
  and resetting the world's clock after a gap (#50). `examples/11_ecs` does that reset, includes
  the generated code with `sillyecs::include_ecs!`, and needs no lint allows for it.
- **Colors are sRGB everywhere.** Sprite tints, rects, lines, filled shapes, circles, mesh
  tints and the clear color used to reach the sRGB surface as linear values, so they showed
  lighter than written (and lighter than text in the same color). They are now decoded from
  sRGB in the shaders, so `[0.5, 0.5, 0.5, 1.0]` or `Color::hex(0x808080)` shows as `#808080`
  everywhere. Existing games get darker, more saturated mid-tones; adjust colors tuned by eye
  to the old look (#58).
- Frames always render through an sRGB view: on surfaces without an sRGB format (WebGPU
  canvases) the sRGB variant is used as a view format, so textures, text and colors look the
  same as natively. `Graphics::surface_format` returns that render format.
- `AppConfig::fixed_timestep` documents that `update` may run zero or several times per frame
  and that `render` runs once (#50).
- `task sync:templates` / `task check:templates` and CI's sync check run
  `tools/sync-guide.sh`, which also writes `crates/prelude/src/guide.md`.
- `SpriteInstance::with_color` and `Painter::mesh` / `clear` take `impl Into<Color>`.
- The text atlas is trimmed after each frame, so glyphs no longer drawn free their space.
- The README, crate READMEs, crate metadata and the jam guide describe the toolkit as being for
  2D and 3D game jams instead of "2D-first", and the README's 3D section states what the mesh
  path covers (#62).

### Fixed

- vello content was encoded to sRGB twice when composited, so vector colors showed lighter
  than given; the composite now decodes it first.
- The fixed-timestep accumulator was unbounded: after a long frame the game fast-forwarded
  through every missed step, and could fall into a spiral of death. It is capped at
  `max_frame_time` and reset when the window regains focus. Variable-`dt` games get `dt`
  capped the same way (#45).
- With a fixed timestep, frames that ran no `update` cleared input edges anyway, so key and
  button presses were lost when the display rate was above the step rate. Edges now reach
  the first `update` after they happen, and only once.

## [0.4.0] - 2026-10-25

### Added

- Filled shapes in the layered 2D pass: `Painter::quad` (convex quad), `Painter::polygon`
  (convex, fan-triangulated) and `Painter::triangles` (indexed triangle list). They interleave
  with sprites and circles by layer; within a layer they draw above sprites and rects and below
  circles, grouped by blend mode and then texture like sprites, with call order kept within a
  group (so plain alpha-blended shapes on one layer keep their order). Malformed triangle
  lists draw nothing: a debug-build panic, one logged warning in release builds (#28).
- Per-vertex colours for filled shapes, interpolated across each triangle:
  `Painter::quad_colored`, `polygon_colored` and `triangles_colored` (#33).
- Blend modes for filled shapes: `Painter::quad_ex`, `polygon_ex` and `triangles_ex` take a
  `BlendMode`, like `sprite_ex` (#32).
- Textured filled shapes: `Painter::quad_textured` maps a texture onto a convex quad
  perspective-correctly (as a flat rectangle seen in perspective, e.g. a road segment), and
  `Painter::triangles_textured` maps per-vertex UVs affinely. UVs outside `0..1` clamp to the
  edge texels; textures don't tile yet (#37) (#34).
- `Painter::rect_ex`, `Painter::rect_outline_ex` and `Painter::line_ex`, which take a layer and
  `BlendMode` like `sprite_ex`, so untextured shapes can draw above a sprite scene (#27).
- `Graphics::texture_size` / `Painter::texture_size`, the pixel size of a loaded texture
  (follows hot-reloads) (#26).
- `SpriteInstance::with_region(min, size, tex_size)` for drawing a pixel rectangle of an atlas,
  `SpriteInstance::flip_x` / `flip_y` for mirroring any UV range, and
  `Painter::sprite_region(tex, src_pos, src_size, pos, size)` as the layer-0 shorthand (#26).
- `examples/03_primitives` demonstrates layered, gradient, additive and textured filled shapes,
  including the same road segment textured perspective-correctly and affinely side by side.

### Changed

- The `Painter` docs and the jam agent guide describe the 2D draw order: layers first; within
  a layer sprites and rects, then filled shapes, then circles, each grouped by blend mode and
  texture with call order kept only within a group; text last. Untextured shapes use the white
  texture, which sorts under every loaded texture on the same layer, so a `rect` drawn after a
  sprite on the same layer still lands under it (#27).

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

[Unreleased]: https://github.com/sunsided/game-toolkit/compare/v0.10.0...HEAD
[0.10.0]: https://github.com/sunsided/game-toolkit/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/sunsided/game-toolkit/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/sunsided/game-toolkit/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/sunsided/game-toolkit/compare/v0.6.1...v0.7.0
[0.6.1]: https://github.com/sunsided/game-toolkit/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/sunsided/game-toolkit/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/sunsided/game-toolkit/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/sunsided/game-toolkit/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/sunsided/game-toolkit/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/sunsided/game-toolkit/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/sunsided/game-toolkit/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/sunsided/game-toolkit/releases/tag/v0.1.0
