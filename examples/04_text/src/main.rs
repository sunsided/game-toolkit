use game_toolkit::prelude::*;

struct App;

impl Game for App {
    fn init(_ctx: &mut Context) -> Result<Self> {
        Ok(Self)
    }
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
    }
    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let fps = ctx.time.fps;
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.04, 0.05, 0.08, 1.0]);

        p.text([20.0, 20.0], "Hello, toolkit!", 36.0, [1.0, 1.0, 1.0, 1.0]);
        p.text(
            [20.0, 70.0],
            "Glyphon + cosmic-text rendering on top of wgpu.",
            18.0,
            [0.8, 0.85, 0.9, 1.0],
        );
        p.text(
            [20.0, 110.0],
            &format!("{:.1} fps", fps),
            18.0,
            [0.6, 1.0, 0.6, 1.0],
        );
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<App>(AppConfig {
        title: "04_text".into(),
        width: 800,
        height: 600,
        ..Default::default()
    })
}
