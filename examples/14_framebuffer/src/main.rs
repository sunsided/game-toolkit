//! A software-rendered framebuffer: a 160x120 plasma computed on the CPU every frame, uploaded
//! with `Graphics::update_texture_rgba` and drawn scaled up as one sprite.
//!
//! The texture is created once in `init` with `create_texture_rgba`; each frame only its pixels
//! are overwritten, so no texture is allocated after startup. This is the path for anything that
//! produces raw pixels: an emulator's screen, a raycaster, a cellular automaton, a video frame.
//! Textures sample with nearest filtering, so the upscaled pixels stay crisp.

use game_toolkit::prelude::*;

const WIDTH: u32 = 160;
const HEIGHT: u32 = 120;
const SCALE: f32 = 5.0;

struct Framebuffer {
    texture: TextureId,
    pixels: Vec<u8>,
    paused: bool,
    t: f32,
}

impl Game for Framebuffer {
    fn init(ctx: &mut Context) -> Result<Self> {
        let pixels = vec![0; (WIDTH * HEIGHT * 4) as usize];
        let texture = ctx
            .gfx
            .create_texture_rgba(WIDTH, HEIGHT, &pixels, Some("framebuffer"));
        Ok(Self {
            texture,
            pixels,
            paused: false,
            t: 0.0,
        })
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        if ctx.input.key_pressed(Key::Space) {
            self.paused = !self.paused;
        }
        if !self.paused {
            self.t += dt;
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        plasma(&mut self.pixels, self.t);
        // Same size as at creation; a mismatched buffer is logged and ignored.
        ctx.gfx.update_texture_rgba(self.texture, &self.pixels);

        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.0, 0.0, 0.0, 1.0]);
        p.sprite(
            self.texture,
            [0.0, 0.0],
            [WIDTH as f32 * SCALE, HEIGHT as f32 * SCALE],
        );
        p.text(
            [12.0, 12.0],
            "CPU pixels -> update_texture_rgba   (Space pause, Esc quit)",
            18.0,
            [1.0, 1.0, 1.0, 1.0],
        );
    }
}

/// The classic sum-of-sines plasma, written straight into an RGBA8 buffer.
fn plasma(pixels: &mut [u8], t: f32) {
    for (i, px) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = (i as u32 % WIDTH) as f32 / WIDTH as f32;
        let y = (i as u32 / WIDTH) as f32 / HEIGHT as f32;
        let v = (x * 10.0 + t).sin()
            + (y * 8.0 - t * 1.3).sin()
            + ((x + y) * 6.0 + t * 0.7).sin()
            + ((x - 0.5).hypot(y - 0.5) * 14.0 - t * 2.0).sin();
        let phase = v * std::f32::consts::PI * 0.5;
        let channel = |offset: f32| ((phase + offset).sin() * 0.5 + 0.5) * 255.0;
        px[0] = channel(0.0) as u8;
        px[1] = channel(2.1) as u8;
        px[2] = channel(4.2) as u8;
        px[3] = 255;
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<Framebuffer>(AppConfig {
        title: "14_framebuffer".into(),
        width: (WIDTH as f32 * SCALE) as u32,
        height: (HEIGHT as f32 * SCALE) as u32,
        ..Default::default()
    })
}
