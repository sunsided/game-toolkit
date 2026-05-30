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
        let mut p = frame.painter(&mut ctx.gfx);
        let t = ctx.time.elapsed.as_secs_f32();
        let r = 0.5 + 0.5 * (t * 0.7).sin();
        let g = 0.5 + 0.5 * (t * 0.9).sin();
        let b = 0.5 + 0.5 * (t * 1.1).sin();
        p.clear([r * 0.3, g * 0.3, b * 0.3, 1.0]);
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<App>(AppConfig {
        title: "01_window — clear color".into(),
        width: 800,
        height: 600,
        ..Default::default()
    })
}
