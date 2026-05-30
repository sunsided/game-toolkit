use game_toolkit::prelude::*;

struct App {
    ui: Ui,
    color: [f32; 4],
    size: f32,
}

impl Game for App {
    fn init(ctx: &mut Context) -> Result<Self> {
        let ui = Ui::new(&ctx.gfx)?;
        Ok(Self {
            ui,
            color: [0.9, 0.4, 0.3, 1.0],
            size: 120.0,
        })
    }

    fn raw_window_event(&mut self, _ctx: &mut Context, event: &winit::event::WindowEvent) -> bool {
        self.ui.on_window_event(event)
    }

    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let (w, h) = ctx.gfx.size();
        {
            let mut p = frame.painter(&mut ctx.gfx);
            p.clear([0.05, 0.06, 0.08, 1.0]);
            p.rect(
                [(w as f32 - self.size) * 0.5, (h as f32 - self.size) * 0.5],
                [self.size, self.size],
                self.color,
            );
        }

        let color = &mut self.color;
        let size = &mut self.size;
        self.ui.run(&mut ctx.gfx, frame, |ectx| {
            egui::Window::new("inspector")
                .resizable(true)
                .show(ectx, |ui| {
                    ui.label("Tweak the box live:");
                    ui.add(egui::Slider::new(size, 16.0..=400.0).text("size"));
                    let mut rgb = [color[0], color[1], color[2]];
                    if ui.color_edit_button_rgb(&mut rgb).changed() {
                        color[0] = rgb[0];
                        color[1] = rgb[1];
                        color[2] = rgb[2];
                    }
                    ui.add(egui::Slider::new(&mut color[3], 0.0..=1.0).text("alpha"));
                });
        });
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<App>(AppConfig {
        title: "06_egui".into(),
        width: 1024,
        height: 720,
        ..Default::default()
    })
}
