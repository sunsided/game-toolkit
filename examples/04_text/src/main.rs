//! Text: plain HUD text, measured and aligned text, and layered text that other 2D content
//! can cover. Space toggles a pause overlay that fades over everything but its own label.

use game_toolkit::prelude::*;

/// Layers, bottom to top: a label box, the overlay, the overlay's caption.
const PANEL: Layer = Layer(1);
const OVERLAY: Layer = Layer(5);
const CAPTION: Layer = Layer(6);

#[derive(Default)]
struct App {
    paused: bool,
}

impl Game for App {
    fn init(_ctx: &mut Context) -> Result<Self> {
        Ok(Self::default())
    }
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        if ctx.input.key_pressed(Key::Space) {
            self.paused = !self.paused;
        }
    }
    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let fps = ctx.time.fps;
        let (w, h) = ctx.gfx.size();
        let (w, h) = (w as f32, h as f32);
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.04, 0.05, 0.08, 1.0]);

        p.text([20.0, 20.0], "Hello, toolkit!", 36.0, [1.0, 1.0, 1.0, 1.0]);
        p.text(
            [20.0, 70.0],
            "Glyphon + cosmic-text rendering on top of wgpu.",
            18.0,
            [0.8, 0.85, 0.9, 1.0],
        );
        // Right-aligned against the window edge: the builder measures the text.
        p.draw_text([w - 20.0, 20.0], &format!("{fps:.1} fps"))
            .size(18.0)
            .color(Color::hex(0x99ff99))
            .align(Align::Right);

        // A box sized to fit its label, using `measure_text` directly. Layered text sits on
        // its layer like a sprite, so the box (layer 1) and label (layer 1, drawn after the
        // box's shapes) can both be covered by the overlay below.
        let label = "measure_text sizes this box";
        let [tw, th] = p.measure_text(label, 22.0);
        let pad = 10.0;
        p.draw_rect([20.0, 140.0], [tw + 2.0 * pad, th + 2.0 * pad])
            .color(Color::hex(0x2a3550))
            .layer(PANEL);
        p.draw_rect([20.0, 140.0], [tw + 2.0 * pad, th + 2.0 * pad])
            .color(Color::hex(0x5b6c9a))
            .outline(2.0)
            .layer(PANEL);
        p.text_ex([20.0 + pad, 140.0 + pad], label, 22.0, Color::WHITE, PANEL);

        p.draw_text([w / 2.0, h - 40.0], "Space: pause overlay   Esc: quit")
            .size(18.0)
            .color(Color::GRAY)
            .align(Align::Center)
            .layer(PANEL);

        if self.paused {
            // Covers all text below layer 5, including the layered labels; unlayered
            // `p.text` would still draw on top.
            p.draw_rect([0.0, 0.0], [w, h])
                .color(Color::BLACK.with_alpha(0.75))
                .layer(OVERLAY);
            p.draw_text([w / 2.0, h / 2.0 - 30.0], "PAUSED")
                .size(60.0)
                .color(Color::GOLD)
                .align(Align::Center)
                .layer(CAPTION);
        }
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
