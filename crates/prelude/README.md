# toolkit-prelude

The one-import entry point to [**game-toolkit**](https://github.com/sunsided/game-toolkit):
`use toolkit_prelude::*;` re-exports the toolkit's runtime types (the `Game` trait, `Context`,
`Painter`, input, audio, assets, ...).

```rust
use toolkit_prelude::*;

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

API docs: <https://docs.rs/toolkit-prelude>
