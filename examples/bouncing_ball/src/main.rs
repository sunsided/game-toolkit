use std::f32::consts::TAU;

use toolkit_prelude::*;

/// Six beachball panel colors, applied to alternating wedges.
const PANELS: [[f32; 4]; 6] = [
    [0.92, 0.20, 0.20, 1.0], // red
    [0.97, 0.55, 0.15, 1.0], // orange
    [0.96, 0.85, 0.20, 1.0], // yellow
    [0.25, 0.70, 0.30, 1.0], // green
    [0.20, 0.50, 0.90, 1.0], // blue
    [0.70, 0.30, 0.80, 1.0], // violet
];

struct BouncingBall {
    pos: [f32; 2],
    vel: [f32; 2],
    size: [f32; 2],
    /// Current spin of the beachball, in radians.
    angle: f32,
}

impl Game for BouncingBall {
    fn init(_ctx: &mut Context) -> Result<Self> {
        Ok(Self {
            pos: [100.0, 100.0],
            vel: [320.0, 240.0],
            size: [48.0, 48.0],
            angle: 0.0,
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

        // Roll without slipping: angular speed = horizontal speed / radius, so the
        // spin direction tracks travel and reverses on every horizontal bounce.
        let r = self.size[0] * 0.5;
        self.angle += (self.vel[0] / r) * dt;
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.05, 0.06, 0.09, 1.0]);

        let r = self.size[0] * 0.5;
        let center = [self.pos[0] + r, self.pos[1] + r];

        // Six colored panels (sprite-batched lines) that rotate with `angle`.
        // The fan covers the full disc, so no base circle is needed - and a base
        // circle would hide the wedges anyway, since the renderer always draws
        // primitives (circles) on top of the sprite layer (lines).
        let step = TAU / PANELS.len() as f32;
        for (i, color) in PANELS.iter().enumerate() {
            let a0 = self.angle + i as f32 * step;
            fill_wedge(&mut p, center, r, a0, a0 + step, *color);
        }

        // White hub and a dark rim (primitives, drawn on top) tidy the wedge
        // tips at the center and the scalloped outer edge of the fan.
        p.circle(center, r * 0.22, [1.0, 1.0, 1.0, 1.0]);
        p.circle_outline(center, r, (r * 0.08).max(1.5), [0.1, 0.1, 0.12, 1.0]);
    }
}

/// Fill a pie wedge `[a0, a1]` with a fan of radial lines from `center` to the rim.
/// The painter has no triangle primitive, so overlapping spokes approximate the fill;
/// spacing is chosen so adjacent rim points stay within one line thickness.
fn fill_wedge(p: &mut Painter, center: [f32; 2], radius: f32, a0: f32, a1: f32, color: [f32; 4]) {
    let thickness = 2.5;
    let span = a1 - a0;
    // Rim gap between two spokes is radius * d_angle; keep it under `thickness`.
    let steps = (span * radius / thickness).ceil().max(1.0) as usize;
    for i in 0..=steps {
        let a = a0 + span * (i as f32 / steps as f32);
        let rim = [center[0] + a.cos() * radius, center[1] + a.sin() * radius];
        p.line(center, rim, thickness, color);
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
