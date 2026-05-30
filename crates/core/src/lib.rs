//! Core: app loop, [`Game`] trait, [`Context`], time.

#![forbid(unsafe_code)]

mod app;
mod context;
mod time;

pub use app::{AppConfig, run};
pub use context::{Context, GameEvent};
pub use time::Time;

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
