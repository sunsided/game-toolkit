//! Texture hot-reload: load a PNG, watch the asset root, and re-upload the texture in
//! place whenever the file changes on disk. Edit `assets/reload_me.png` while it runs and
//! the window updates without a restart - the `TextureId` stays stable across reloads.
//!
//! Watcher notes: `notify` can emit several events per save (editors often write a temp
//! file then rename), and platforms differ in which `EventKind`s they report. `Assets`
//! de-dupes paths within a frame via a `HashSet`, but a single save may still surface over
//! one or more frames, so expect occasional repeat "reloaded" lines - that is the OS
//! coalescing behavior, not a bug.

use toolkit_prelude::*;

struct HotReload {
    tex: TextureId,
}

impl Game for HotReload {
    fn init(ctx: &mut Context) -> Result<Self> {
        let path = ctx.assets.resolve("reload_me.png");
        let tex = ctx.gfx.load_texture(&path)?;
        ctx.assets.watch()?;
        log::info!("watching {} - edit it to see a live reload", path.display());
        Ok(Self { tex })
    }

    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        let changed = ctx.assets.drain_changes();
        if !changed.is_empty() {
            let n = ctx.gfx.refresh_textures(&changed);
            if n > 0 {
                log::info!("reloaded {n} texture(s)");
            }
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let (w, h) = ctx.gfx.size();
        let (w, h) = (w as f32, h as f32);
        let side = (w.min(h) * 0.6).floor();
        let pos = [(w - side) * 0.5, (h - side) * 0.5];

        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.1, 0.1, 0.12, 1.0]);
        p.sprite(self.tex, pos, [side, side]);
        p.text(
            [16.0, 16.0],
            "08_hot_reload: edit assets/reload_me.png (Esc to quit)",
            20.0,
            [0.85, 0.85, 0.9, 1.0],
        );
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<HotReload>(AppConfig {
        title: "08_hot_reload".into(),
        width: 700,
        height: 700,
        asset_root: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        ..Default::default()
    })
}
