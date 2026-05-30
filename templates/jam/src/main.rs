//! {{project-name}} - a game-toolkit jam starter: a ball bounces around the window and
//! spins with its horizontal speed. Esc quits. Edit `update` (physics/input) and `render`
//! (drawing) to make it yours.

use game_toolkit::prelude::*;

struct Game1 {
    pos: [f32; 2],
    vel: [f32; 2],
    radius: f32,
    spin: f32,
}

impl Game for Game1 {
    fn init(_ctx: &mut Context) -> Result<Self> {
        Ok(Self {
            pos: [240.0, 180.0],
            vel: [320.0, 240.0],
            radius: 48.0,
            spin: 0.0,
        })
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }

        let (w, h) = ctx.gfx.size();
        let bounds = [w as f32, h as f32];
        self.pos[0] += self.vel[0] * dt;
        self.pos[1] += self.vel[1] * dt;

        // Bounce off each edge, keeping the ball fully on screen.
        for axis in 0..2 {
            if self.pos[axis] - self.radius < 0.0 {
                self.pos[axis] = self.radius;
                self.vel[axis] = self.vel[axis].abs();
            } else if self.pos[axis] + self.radius > bounds[axis] {
                self.pos[axis] = bounds[axis] - self.radius;
                self.vel[axis] = -self.vel[axis].abs();
            }
        }

        // Roll the spin marker with horizontal speed.
        self.spin += self.vel[0] / self.radius * dt;
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.07, 0.08, 0.11, 1.0]);

        // Ball body, a marker that rotates to show the spin, and a rim.
        p.circle(self.pos, self.radius, [0.30, 0.65, 0.95, 1.0]);
        let marker = [
            self.pos[0] + self.spin.cos() * self.radius * 0.55,
            self.pos[1] + self.spin.sin() * self.radius * 0.55,
        ];
        p.circle(marker, self.radius * 0.18, [1.0, 0.9, 0.3, 1.0]);
        p.circle_outline(self.pos, self.radius, 3.0, [1.0, 1.0, 1.0, 0.85]);

        p.text(
            [16.0, 16.0],
            "{{project-name}} (Esc to quit)",
            22.0,
            [0.9, 0.9, 0.95, 1.0],
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
