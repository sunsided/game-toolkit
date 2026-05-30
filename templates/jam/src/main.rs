//! {{project-name}} - a game-toolkit jam starter. Window opens, Esc quits, a title is drawn.

use toolkit_prelude::*;

struct Game1 {
    t: f32,
}

impl Game for Game1 {
    fn init(_ctx: &mut Context) -> Result<Self> {
        Ok(Self { t: 0.0 })
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        self.t += dt;
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let (w, h) = ctx.gfx.size();
        let (w, h) = (w as f32, h as f32);

        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.07, 0.08, 0.11, 1.0]);

        // A gently pulsing disc behind the title.
        let r = 70.0 + 12.0 * self.t.sin();
        p.circle([w * 0.5, h * 0.5], r, [0.20, 0.45, 0.75, 0.9]);

        p.text(
            [w * 0.5 - 140.0, h * 0.5 - 16.0],
            "{{project-name}}",
            32.0,
            [1.0, 1.0, 1.0, 1.0],
        );
        p.text(
            [16.0, 16.0],
            "Esc to quit. Edit src/main.rs to begin.",
            18.0,
            [0.7, 0.7, 0.75, 1.0],
        );
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<Game1>(AppConfig {
        title: "{{project-name}}".into(),
        width: 1280,
        height: 720,
        // Load assets relative to the crate, so `cargo run` works from anywhere.
        asset_root: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        ..Default::default()
    })
}
