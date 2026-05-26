use toolkit_prelude::*;

struct BouncingBall {
    pos: [f32; 2],
    vel: [f32; 2],
    size: [f32; 2],
}

impl Game for BouncingBall {
    fn init(_ctx: &mut Context) -> Result<Self> {
        Ok(Self {
            pos: [100.0, 100.0],
            vel: [320.0, 240.0],
            size: [48.0, 48.0],
        })
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }

        self.pos[0] += self.vel[0] * dt;
        self.pos[1] += self.vel[1] * dt;

        let (w, h) = ctx.gfx.size();
        let (w, h) = (w as f32, h as f32);
        if self.pos[0] < 0.0 {
            self.pos[0] = 0.0;
            self.vel[0] = self.vel[0].abs();
        }
        if self.pos[0] + self.size[0] > w {
            self.pos[0] = w - self.size[0];
            self.vel[0] = -self.vel[0].abs();
        }
        if self.pos[1] < 0.0 {
            self.pos[1] = 0.0;
            self.vel[1] = self.vel[1].abs();
        }
        if self.pos[1] + self.size[1] > h {
            self.pos[1] = h - self.size[1];
            self.vel[1] = -self.vel[1].abs();
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.05, 0.06, 0.09, 1.0]);
        p.rect(self.pos, self.size, [1.0, 0.4, 0.2, 1.0]);
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<BouncingBall>(AppConfig {
        title: "bouncing_ball".into(),
        width: 800,
        height: 600,
        ..Default::default()
    })
}
