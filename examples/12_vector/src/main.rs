//! Vector graphics with the optional vello backend.
//!
//! `painter.vector(|scene| ...)` hands the frame's `vello::Scene`; vector content composites
//! on top of the 2D layers. Here: a rotating ring of filled circles and a spinning stroked
//! rounded rectangle, with a 2D text label underneath.

use game_toolkit_prelude::*;
use vello::kurbo::{Affine, Circle, RoundedRect, Stroke};
use vello::peniko::{Color, Fill};

struct VectorDemo;

impl Game for VectorDemo {
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
        let t = ctx.time.elapsed.as_secs_f64();
        let cx = w as f64 * 0.5;
        let cy = h as f64 * 0.5;

        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.06, 0.07, 0.10, 1.0]);
        // Drawn under the vector layer.
        p.text(
            [16.0, 16.0],
            "12_vector: vello vector graphics (Esc to quit)",
            20.0,
            [0.85, 0.85, 0.9, 1.0],
        );

        p.vector(|scene| {
            // A rotating ring of filled circles.
            let n = 10;
            for i in 0..n {
                let a = i as f64 / n as f64 * std::f64::consts::TAU + t * 0.6;
                let x = cx + a.cos() * 180.0;
                let y = cy + a.sin() * 180.0;
                let phase = i as f64 / n as f64;
                let color = Color::from_rgb8(
                    (120.0 + 120.0 * (phase * std::f64::consts::TAU).sin()) as u8,
                    (120.0 + 120.0 * (phase * std::f64::consts::TAU + 2.0).sin()) as u8,
                    (120.0 + 120.0 * (phase * std::f64::consts::TAU + 4.0).sin()) as u8,
                );
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    color,
                    None,
                    &Circle::new((x, y), 30.0),
                );
            }

            // A spinning stroked rounded rectangle in the centre.
            let rr = RoundedRect::new(cx - 110.0, cy - 70.0, cx + 110.0, cy + 70.0, 28.0);
            let spin = Affine::translate((cx, cy))
                * Affine::rotate(t * 0.5)
                * Affine::translate((-cx, -cy));
            scene.stroke(
                &Stroke::new(8.0),
                spin,
                Color::from_rgb8(255, 220, 90),
                None,
                &rr,
            );
        });
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<VectorDemo>(AppConfig {
        title: "12_vector".into(),
        width: 800,
        height: 600,
        ..Default::default()
    })
}
