//! Loads a PNG with `ctx.load_texture` and draws it three ways: plain, spinning,
//! and tinted, plus a pixel region of it, mirrored, through the `draw_sprite` builder.
//! Exercises the PNG decode -> upload -> render path end to end.

use game_toolkit::prelude::*;

struct Sprite {
    tex: TextureId,
    size: [f32; 2],
}

impl Game for Sprite {
    fn init(ctx: &mut Context) -> Result<Self> {
        // Relative to the asset root configured below, so the example loads regardless of
        // the process working directory.
        let tex = ctx.load_texture("sprite.png")?;
        Ok(Self {
            tex,
            size: [128.0, 128.0],
        })
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
        let s = self.size;
        let cy = h * 0.5 - s[1] * 0.5;

        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.08, 0.08, 0.11, 1.0]);

        // Plain.
        p.sprite(self.tex, [w * 0.5 - s[0] * 1.9, cy], s);
        // Spinning (rotation builder).
        p.sprite_ex(
            self.tex,
            SpriteInstance::at([w * 0.5 - s[0] * 0.5, cy], s).with_rotation(t),
            0,
            BlendMode::Alpha,
        );
        // Tinted (color builder).
        p.sprite_ex(
            self.tex,
            SpriteInstance::at([w * 0.5 + s[0] * 0.9, cy], s).with_color([0.4, 0.8, 1.0, 1.0]),
            0,
            BlendMode::Alpha,
        );

        // Builder: the top-left quarter by pixel rectangle (no texture-size lookup needed),
        // on a higher layer than the row above, flipping to face the other way every second.
        p.draw_sprite(self.tex, [w * 0.5 - s[0] * 0.5, cy + s[1] * 1.2], s)
            .region([0.0, 0.0], [64.0, 64.0])
            .flip_x_if(t.fract() < 0.5)
            .layer(1);

        p.text(
            [16.0, 16.0],
            "02_sprite: plain / spinning / tinted, and a mirrored region (Esc to quit)",
            20.0,
            [0.85, 0.85, 0.9, 1.0],
        );
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<Sprite>(AppConfig {
        title: "02_sprite".into(),
        width: 800,
        height: 600,
        asset_root: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        ..Default::default()
    })
}
