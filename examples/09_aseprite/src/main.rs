//! Loads a native `.aseprite` file into a `SpriteSheet` and plays one of its tagged
//! animations with an `AnimationPlayer`. The game-toolkit-aseprite crate flattens each frame,
//! packs them into one atlas texture, and exposes frames as UV rects - drawing a frame is
//! an ordinary sprite with a UV sub-rect, so `game-toolkit-gfx` never learns about Aseprite.
//!
//! The exported PNG + JSON path is available too via
//! `SpriteSheet::from_aseprite_json(gfx, png, json)`.

use game_toolkit::prelude::*;

struct AsepriteDemo {
    sheet: SpriteSheet,
    player: AnimationPlayer,
    tag: String,
}

impl Game for AsepriteDemo {
    fn init(ctx: &mut Context) -> Result<Self> {
        let path = ctx.assets.resolve("character.aseprite");
        let sheet = SpriteSheet::load_aseprite(&mut ctx.gfx, &path)?;
        log::info!(
            "loaded {} frames, {} tag(s)",
            sheet.num_frames(),
            sheet.animations.len()
        );

        // Play the first tag if present, otherwise sweep every frame.
        let (tag, player) = match sheet.animations.values().next() {
            Some(a) => (a.name.clone(), AnimationPlayer::from_animation(a.clone())),
            None => (
                "<all>".to_string(),
                AnimationPlayer::from_animation(sheet.full_animation()),
            ),
        };
        Ok(Self { sheet, player, tag })
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        self.player.advance(&self.sheet, dt);
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let (w, h) = ctx.gfx.size();
        let (w, h) = (w as f32, h as f32);

        let idx = self.player.current_index();
        let frame_size = self.sheet.frame(idx).map(|f| f.size).unwrap_or([16, 16]);
        let scale = 8.0;
        let size = [frame_size[0] as f32 * scale, frame_size[1] as f32 * scale];
        let pos = [w * 0.5 - size[0] * 0.5, h * 0.5 - size[1] * 0.5];
        let (uv_min, uv_max) = self.player.current_uv(&self.sheet);
        let tex = self.sheet.texture;
        let label = format!("09_aseprite: tag '{}' frame {idx} (Esc to quit)", self.tag);

        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.07, 0.08, 0.1, 1.0]);
        p.sprite_ex(
            tex,
            SpriteInstance::at(pos, size).with_uv(uv_min, uv_max),
            0,
            BlendMode::Alpha,
        );
        p.text([16.0, 16.0], &label, 20.0, [0.85, 0.85, 0.9, 1.0]);
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<AsepriteDemo>(AppConfig {
        title: "09_aseprite".into(),
        width: 800,
        height: 600,
        asset_root: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        ..Default::default()
    })
}
