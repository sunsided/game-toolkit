//! Core: app loop, [`Game`] trait, [`Context`], time.

#![forbid(unsafe_code)]

mod app;
mod context;
mod rng;
mod time;
#[cfg(target_arch = "wasm32")]
mod web;

pub use app::{AppConfig, run};
pub use context::{Context, GameEvent};
pub use rng::Rng;
pub use time::Time;

/// The calling crate's `assets/` directory, for [`AppConfig::asset_root`].
///
/// Natively this is `$CARGO_MANIFEST_DIR/assets`, so `cargo run` finds the assets from any
/// working directory. On the web it is `assets`, a URL relative to the page (where trunk's
/// `copy-dir` puts the folder).
#[macro_export]
macro_rules! asset_root {
    () => {{
        #[cfg(not(target_arch = "wasm32"))]
        let root = ::std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
        #[cfg(target_arch = "wasm32")]
        let root = ::std::path::PathBuf::from("assets");
        root
    }};
}

pub trait Game: 'static + Sized {
    fn init(ctx: &mut Context) -> anyhow::Result<Self>;
    fn update(&mut self, ctx: &mut Context, dt: f32);
    fn render(&mut self, ctx: &mut Context, frame: &mut game_toolkit_gfx::Frame);
    fn event(&mut self, _ctx: &mut Context, _event: &GameEvent) {}
    /// Raw winit window-event hook. Return `true` to mark the event consumed so it is not
    /// forwarded to the toolkit's [`game_toolkit_input::Input`] state. Use this for egui or other
    /// overlays that need first crack at events.
    fn raw_window_event(&mut self, _ctx: &mut Context, _event: &winit::event::WindowEvent) -> bool {
        false
    }
    fn shutdown(&mut self, _ctx: &mut Context) {}
}
