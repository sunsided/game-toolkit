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
        if ctx.input.key_pressed(Key::F12) {
            ctx.gfx.request_screenshot("screenshot.png");
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

        // Cross-batcher z-order (#14): sprites and circles interleave by layer, lower under
        // higher. A layer-0 sprite rect sits between a layer -1 circle (peeking out behind
        // it, top-left) and a layer +1 circle (in front of it, bottom-right).
        let bx = w - 150.0;
        let by = 95.0;
        p.circle_ex([bx - 30.0, by - 30.0], 34.0, 0.0, [0.95, 0.30, 0.30, 1.0], -1);
        p.rect([bx - 42.0, by - 42.0], [84.0, 84.0], [0.25, 0.80, 0.45, 1.0]);
        p.circle_ex([bx + 34.0, by + 34.0], 28.0, 0.0, [0.35, 0.55, 1.0, 1.0], 1);
        p.text([bx - 58.0, by + 62.0], "z-order", 18.0, [1.0, 1.0, 1.0, 1.0]);
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<App>(AppConfig {
        title: "03_primitives".into(),
        width: 800,
        height: 600,
        // 4x MSAA smooths the SDF circle and thin-line edges; the depth buffer is allocated
        // but unused by the 2D pipelines (exercises the depth path without changing output).
        msaa_samples: 4,
        depth_format: Some(wgpu::TextureFormat::Depth32Float),
        ..Default::default()
    })
}
