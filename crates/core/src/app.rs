use std::time::Duration;

use anyhow::Result;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::WindowId;

use crate::Game;
use crate::context::{Context, GameEvent};

/// Runtime configuration passed to [`run`].
///
/// Construct it with `..Default::default()` so new fields added in future minor releases don't
/// break your call site: `AppConfig { title: "game".into(), ..Default::default() }`.
#[derive(Clone)]
pub struct AppConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
    /// `Some(d)` for deterministic fixed-step `update`; `None` for variable `dt`.
    pub fixed_timestep: Option<Duration>,
    /// Asset root directory. Defaults to `./assets`.
    pub asset_root: std::path::PathBuf,
    /// Seed for deterministic runtime RNG (`ctx.rng`).
    pub random_seed: u64,
    /// Depth attachment format. `None` (default) keeps the 2D path depth-less; `Some(fmt)`
    /// (e.g. `wgpu::TextureFormat::Depth32Float`) allocates a depth buffer for depth-tested
    /// rendering. The built-in 2D pipelines never write depth, so enabling it is harmless.
    pub depth_format: Option<wgpu::TextureFormat>,
    /// MSAA sample count for the surface. `1` (default) disables MSAA; `2`/`4`/`8` enable it
    /// (the value must be supported by the adapter for the surface format).
    pub msaa_samples: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            title: "Game".into(),
            width: 1280,
            height: 720,
            vsync: true,
            fixed_timestep: None,
            asset_root: std::path::PathBuf::from("assets"),
            random_seed: crate::Rng::DEFAULT_SEED,
            depth_format: None,
            msaa_samples: 1,
        }
    }
}

pub fn run<G: Game>(config: AppConfig) -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut runner = AppRunner::<G> {
        config,
        state: None,
        accumulator: Duration::ZERO,
    };
    event_loop.run_app(&mut runner)?;
    Ok(())
}

struct AppRunner<G: Game> {
    config: AppConfig,
    state: Option<RunState<G>>,
    accumulator: Duration,
}

struct RunState<G: Game> {
    ctx: Context,
    game: G,
}

impl<G: Game> ApplicationHandler for AppRunner<G> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let mut ctx = match Context::new(
            event_loop,
            &self.config.title,
            self.config.width,
            self.config.height,
            self.config.vsync,
            self.config.asset_root.clone(),
            self.config.random_seed,
            self.config.depth_format,
            self.config.msaa_samples,
        ) {
            Ok(c) => c,
            Err(e) => {
                log::error!("context init failed: {e:?}");
                event_loop.exit();
                return;
            }
        };
        let game = match G::init(&mut ctx) {
            Ok(g) => g,
            Err(e) => {
                log::error!("game init failed: {e:?}");
                event_loop.exit();
                return;
            }
        };
        ctx.window.request_redraw();
        self.state = Some(RunState { ctx, game });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        // Give the game's raw-event hook first crack (egui etc. needs this).
        let consumed = state.game.raw_window_event(&mut state.ctx, &event);
        match event {
            WindowEvent::CloseRequested => {
                state.game.event(&mut state.ctx, &GameEvent::CloseRequested);
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                state.ctx.gfx.resize(size.width, size.height);
                state.game.event(
                    &mut state.ctx,
                    &GameEvent::Resized {
                        width: size.width,
                        height: size.height,
                    },
                );
            }
            WindowEvent::Focused(focused) => {
                state
                    .ctx
                    .input
                    .handle_window_event(&WindowEvent::Focused(focused));
                state
                    .game
                    .event(&mut state.ctx, &GameEvent::FocusChanged(focused));
            }
            WindowEvent::RedrawRequested => {
                self.frame(event_loop);
            }
            ref other if !consumed => {
                state.ctx.input.handle_window_event(other);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_ref() {
            state.ctx.window.request_redraw();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_mut() {
            state.game.shutdown(&mut state.ctx);
        }
    }
}

impl<G: Game> AppRunner<G> {
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        state.ctx.time.tick();
        state.ctx.input.poll_gamepads();

        match self.config.fixed_timestep {
            Some(step) => {
                self.accumulator += state.ctx.time.delta;
                while self.accumulator >= step {
                    state.game.update(&mut state.ctx, step.as_secs_f32());
                    self.accumulator -= step;
                }
            }
            None => {
                let dt = state.ctx.time.delta.as_secs_f32();
                state.game.update(&mut state.ctx, dt);
            }
        }

        if state.ctx.quit_requested {
            event_loop.exit();
            return;
        }

        state.ctx.input.end_frame();

        match state.ctx.gfx.begin_frame() {
            Ok(mut frame) => {
                state.game.render(&mut state.ctx, &mut frame);
                state.ctx.gfx.present(frame);
            }
            Err(e) => {
                log::warn!("begin_frame failed: {e:?}");
                let (w, h) = state.ctx.gfx.size();
                state.ctx.gfx.resize(w, h);
            }
        }
    }
}
