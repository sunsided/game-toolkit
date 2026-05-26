use toolkit_prelude::*;

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
        let (w, h) = ctx.gfx.size();
        let (w, h) = (w as f32, h as f32);
        let t = ctx.time.elapsed.as_secs_f32();
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.07, 0.08, 0.1, 1.0]);

        p.rect([40.0, 40.0], [120.0, 80.0], [0.9, 0.4, 0.3, 1.0]);
        p.rect_outline([200.0, 40.0], [120.0, 80.0], 3.0, [0.3, 0.9, 0.5, 1.0]);

        let cx = w * 0.5;
        let cy = h * 0.5;
        p.circle([cx, cy], 60.0 + 20.0 * t.sin(), [0.4, 0.7, 1.0, 0.85]);
        p.circle_outline([cx, cy], 110.0, 4.0, [1.0, 0.85, 0.2, 1.0]);

        // animated star of lines
        let n = 24;
        for i in 0..n {
            let a = (i as f32 / n as f32) * std::f32::consts::TAU + t * 0.4;
            let r0 = 30.0;
            let r1 = 150.0 + 30.0 * (t * 1.7 + i as f32).sin();
            let from = [cx + a.cos() * r0, cy + a.sin() * r0];
            let to = [cx + a.cos() * r1, cy + a.sin() * r1];
            p.line(from, to, 2.0, [1.0, 1.0, 1.0, 0.5]);
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<App>(AppConfig {
        title: "03_primitives".into(),
        width: 800,
        height: 600,
        ..Default::default()
    })
}
