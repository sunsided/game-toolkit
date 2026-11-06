//! Draw a small scene into a 320 × 180 canvas and scale it onto the window.
//!
//! Keys: `1` integer scaling, `2` fractional fit, `3` stretch, `L` toggles linear filtering,
//! `V` toggles the virtual resolution off and on. The crosshair follows the mouse in canvas
//! pixels; resize the window to see the border change.

use game_toolkit::prelude::*;

const CANVAS: (u32, u32) = (320, 180);

struct App {
    checker: TextureId,
}

impl Game for App {
    fn init(ctx: &mut Context) -> Result<Self> {
        // 4x4 two-tone tile, scaled up per canvas pixel to show the pixel grid.
        let rgba: Vec<u8> = (0..16)
            .flat_map(|i| {
                if (i % 4 + i / 4) % 2 == 0 {
                    [70, 120, 70, 255]
                } else {
                    [60, 100, 60, 255]
                }
            })
            .collect();
        let checker = ctx.gfx.create_texture_rgba(4, 4, &rgba, Some("ground"));
        ctx.gfx.set_texture_wrap(checker, TextureWrap::Repeat);
        Ok(Self { checker })
    }

    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        if ctx.input.key_pressed(Key::Digit1) {
            ctx.gfx.set_scale_mode(ScaleMode::Integer);
        }
        if ctx.input.key_pressed(Key::Digit2) {
            ctx.gfx.set_scale_mode(ScaleMode::Fit);
        }
        if ctx.input.key_pressed(Key::Digit3) {
            ctx.gfx.set_scale_mode(ScaleMode::Stretch);
        }
        if ctx.input.key_pressed(Key::KeyL) {
            let next = match ctx.gfx.scale_filter() {
                ScaleFilter::Nearest => ScaleFilter::Linear,
                ScaleFilter::Linear => ScaleFilter::Nearest,
            };
            ctx.gfx.set_scale_filter(next);
        }
        if ctx.input.key_pressed(Key::KeyV) {
            let next = match ctx.gfx.virtual_resolution() {
                Some(_) => None,
                None => Some(CANVAS),
            };
            ctx.gfx.set_virtual_resolution(next);
        }
        if ctx.input.key_pressed(Key::F12) {
            ctx.gfx.request_screenshot("screenshot.png");
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let (w, h) = ctx.gfx.size();
        let (w, h) = (w as f32, h as f32);
        let t = ctx.time.elapsed.as_secs_f32();
        let mouse = ctx.input.mouse_pos();
        let status = format!(
            "{:?} / {:?} / {}",
            ctx.gfx.scale_mode(),
            ctx.gfx.scale_filter(),
            match ctx.gfx.virtual_resolution() {
                Some((cw, ch)) => format!("{cw}x{ch}"),
                None => "window".into(),
            }
        );
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.35, 0.55, 0.85, 1.0]);

        // Ground: one quad with repeating UVs, 4 px per tile.
        let ground_y = h - 40.0;
        p.quad_textured(
            self.checker,
            [[0.0, ground_y], [w, ground_y], [w, h], [0.0, h]],
            [
                [0.0, 0.0],
                [w / 4.0, 0.0],
                [w / 4.0, 40.0 / 4.0],
                [0.0, 40.0 / 4.0],
            ],
            Color::WHITE,
            0,
            BlendMode::Alpha,
        );

        // Sun and a bouncing box.
        p.circle([w - 40.0, 34.0], 14.0, [1.0, 0.85, 0.3, 1.0]);
        let x = 40.0 + (t * 0.8).sin().abs() * (w - 100.0);
        let y = ground_y - 16.0 - (t * 3.0).sin().abs() * 40.0;
        p.rect([x.round(), y.round()], [16.0, 16.0], [0.9, 0.3, 0.3, 1.0]);

        p.text([4.0, 4.0], &status, 10.0, Color::WHITE);
        p.text(
            [4.0, 16.0],
            "1/2/3 scale  L filter  V canvas",
            10.0,
            [1.0, 1.0, 1.0, 0.7],
        );

        // Crosshair at the mouse, in canvas pixels.
        let (mx, my) = (mouse.0.floor(), mouse.1.floor());
        p.rect([mx - 4.0, my], [9.0, 1.0], Color::BLACK);
        p.rect([mx, my - 4.0], [1.0, 9.0], Color::BLACK);
    }
}

fn main() -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    env_logger::init();
    run::<App>(AppConfig {
        title: "17_virtual_resolution".into(),
        width: 1280,
        height: 720,
        virtual_resolution: Some(CANVAS),
        border_color: Color::rgb(0.05, 0.05, 0.07),
        ..Default::default()
    })
}
