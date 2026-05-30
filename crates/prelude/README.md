# game-toolkit

A 2D-first Rust toolkit for game jams. This umbrella crate is the one dependency you add;
`use game_toolkit::prelude::*;` pulls in the runtime types from the `game-toolkit-*` crates
(the `Game` trait, `Context`, `Painter`, input, audio, assets, ...).

```rust
use game_toolkit::prelude::*;

struct Game1;
impl Game for Game1 {
    fn init(_ctx: &mut Context) -> Result<Self> { Ok(Self) }
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) { ctx.quit(); }
    }
    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        frame.painter(&mut ctx.gfx).clear([0.1, 0.1, 0.15, 1.0]);
    }
}

fn main() -> Result<()> {
    run::<Game1>(AppConfig { title: "my-jam".into(), ..Default::default() })
}
```

Optional features: `ui` (egui overlay), `aseprite`, `ecs`, `synth` (chiptune audio),
`vector` (vello). See the
[workspace README](https://github.com/sunsided/game-toolkit#readme) for the full toolkit,
examples, and quick-start.

## Documentation

API docs: <https://docs.rs/game-toolkit>
