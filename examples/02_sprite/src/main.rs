//! Loads a PNG with `Graphics::load_texture` and draws it three ways: plain, spinning,
//! and tinted. Exercises the PNG decode -> upload -> render path end to end.

use game_toolkit_prelude::*;

struct Sprite {
    tex: TextureId,
    size: [f32; 2],
}

impl Game for Sprite {
    fn init(ctx: &mut Context) -> Result<Self> {
        // `resolve` joins against the asset root configured below, yielding an absolute
        // path so the example loads regardless of the process working directory.
        let path = ctx.assets.resolve("sprite.png");
        let tex = ctx.gfx.load_texture(&path)?;
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

        p.text(
            [16.0, 16.0],
            "02_sprite: plain / spinning / tinted (Esc to quit)",
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
