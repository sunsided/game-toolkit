use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use crate::Game;
use crate::context::{self, Context, GameEvent, Parts};

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
    /// `Some(step)` calls `update` with a fixed `dt` of `step`: zero, one or several times per
    /// frame, as many as the real time elapsed allows. `None` (default) calls it once per
    /// frame with the frame's `dt`. Either way `render` runs once per frame; with a fixed
    /// step, [`Time::alpha`](crate::Time::alpha) says how far the frame is into the next step,
    /// for interpolating motion between steps.
    ///
    /// Leave this `None` if the game steps a simulation itself, e.g. a sillyecs world whose
    /// `apply_system_phases` runs its own fixed phases from real time; otherwise those fixed
    /// phases nest inside the toolkit's steps.
    ///
    /// Input edges (`key_pressed`, `mouse_delta`, ...) are seen by the first `update` of a
    /// frame and carried over to the next frame when no step ran, so presses aren't dropped.
    pub fixed_timestep: Option<Duration>,
    /// Upper bound on the time one frame advances the game (default 250 ms). After a long
    /// frame - the window was dragged, the tab was in the background, a breakpoint hit - the
    /// game slows down for that frame instead of fast-forwarding through the gap: `dt` is
    /// capped at this, and with a fixed timestep at most `max_frame_time / step` updates run.
    /// [`Time::delta`](crate::Time::delta) still reports the real frame time.
    /// `Duration::ZERO` disables the cap.
    pub max_frame_time: Duration,
    /// Asset root directory. Defaults to `./assets`. On the web it is a URL path relative to
    /// the page, and the files listed in its `manifest.txt` are fetched before `Game::init`.
    pub asset_root: std::path::PathBuf,
    /// Web only: asset paths to preload instead of reading `manifest.txt`. Ignored natively.
    pub web_preload: Option<Vec<String>>,
    /// Web only: `id` of an existing `<canvas>` to render into. `None` (default) appends a new
    /// canvas to `<body>`. Ignored natively.
    pub canvas_id: Option<String>,
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
            max_frame_time: Duration::from_millis(250),
            asset_root: std::path::PathBuf::from("assets"),
            web_preload: None,
            canvas_id: None,
            random_seed: crate::Rng::DEFAULT_SEED,
            depth_format: None,
            msaa_samples: 1,
        }
    }
}

/// Sent by the web init task once the GPU is up and the assets are preloaded. Unused natively,
/// where init completes synchronously inside `resumed`.
type InitDone = Box<(Arc<Window>, Result<Parts>)>;

/// Open the window and run `G` until it quits.
///
/// Natively this blocks until the window closes. On the web it installs a panic hook and a
/// console logger (unless a logger is already set), hands the loop to the browser and returns
/// immediately; `G::init` runs once the GPU is ready and the assets are preloaded.
pub fn run<G: Game>(config: AppConfig) -> Result<()> {
    #[cfg(target_arch = "wasm32")]
    crate::web::install_hooks();
    let event_loop = EventLoop::<InitDone>::with_user_event().build()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut runner = AppRunner::<G> {
        config,
        state: None,
        accumulator: Duration::ZERO,
        #[cfg(target_arch = "wasm32")]
        proxy: event_loop.create_proxy(),
        #[cfg(target_arch = "wasm32")]
        initializing: false,
    };
    #[cfg(not(target_arch = "wasm32"))]
    event_loop.run_app(&mut runner)?;
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(runner);
    }
    Ok(())
}

struct AppRunner<G: Game> {
    config: AppConfig,
    state: Option<RunState<G>>,
    accumulator: Duration,
    #[cfg(target_arch = "wasm32")]
    proxy: winit::event_loop::EventLoopProxy<InitDone>,
    /// Set while the web init task is in flight, so another `resumed` doesn't start a second.
    #[cfg(target_arch = "wasm32")]
    initializing: bool,
}

struct RunState<G: Game> {
    ctx: Context,
    game: G,
}

impl<G: Game> ApplicationHandler<InitDone> for AppRunner<G> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        if self.initializing {
            return;
        }
        let window = match context::create_window(event_loop, &self.config) {
            Ok(w) => w,
            Err(e) => {
                log::error!("window creation failed: {e:?}");
                event_loop.exit();
                return;
            }
        };
        // Adapter/device requests are async. Natively we can block on them; the browser main
        // thread can't block, so there init runs as a task that reports back via `user_event`.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let parts =
                pollster::block_on(context::init_parts(window.clone(), self.config.clone()));
            self.finish_init(event_loop, window, parts);
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.initializing = true;
            let proxy = self.proxy.clone();
            let config = self.config.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let parts = context::init_parts(window.clone(), config).await;
                if proxy.send_event(Box::new((window, parts))).is_err() {
                    log::error!("event loop closed before init finished");
                }
            });
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: InitDone) {
        let (window, parts) = *event;
        #[cfg(target_arch = "wasm32")]
        {
            self.initializing = false;
        }
        self.finish_init(event_loop, window, parts);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        // The first key, click or touch is the user gesture browsers require before audio may
        // play. Natively the device is already open and this is a no-op.
        if is_user_gesture(&event) {
            state.ctx.start_audio();
        }
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
                if focused {
                    // Don't replay time accumulated while the window was in the background.
                    self.accumulator = Duration::ZERO;
                }
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

/// How far `accumulator` is into the next fixed `step`, in `0.0..1.0`.
fn alpha(accumulator: Duration, step: Duration) -> f32 {
    if step.is_zero() {
        return 0.0;
    }
    (accumulator.as_secs_f64() / step.as_secs_f64()).clamp(0.0, 1.0) as f32
}

fn is_user_gesture(event: &WindowEvent) -> bool {
    match event {
        WindowEvent::KeyboardInput { event, .. } => event.state == ElementState::Pressed,
        WindowEvent::MouseInput { state, .. } => *state == ElementState::Pressed,
        WindowEvent::Touch(_) => true,
        _ => false,
    }
}

impl<G: Game> AppRunner<G> {
    /// Second half of startup, once the window, GPU and assets exist: build the [`Context`]
    /// and call `G::init`.
    fn finish_init(
        &mut self,
        event_loop: &ActiveEventLoop,
        window: Arc<Window>,
        parts: Result<Parts>,
    ) {
        let parts = match parts {
            Ok(p) => p,
            Err(e) => {
                log::error!("context init failed: {e:?}");
                event_loop.exit();
                return;
            }
        };
        let mut ctx = Context::from_parts(window, parts, &self.config);
        // On the web, init is async and the canvas's real size arrives as a `Resized` event
        // while it runs, which `window_event` drops (no state yet). Catch up before `G::init`
        // so it sees the true surface size.
        let size = ctx.window.inner_size();
        ctx.gfx.resize(size.width, size.height);
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

    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        state.ctx.time.tick();
        state.ctx.input.poll_gamepads();

        let delta = match self.config.max_frame_time {
            Duration::ZERO => state.ctx.time.delta,
            cap => state.ctx.time.delta.min(cap),
        };
        match self.config.fixed_timestep {
            Some(step) => {
                self.accumulator += delta;
                let mut updated = false;
                while self.accumulator >= step {
                    state.game.update(&mut state.ctx, step.as_secs_f32());
                    self.accumulator -= step;
                    // Edges belong to the first step; later steps this frame shouldn't see
                    // the same press again. With no step at all they wait for the next frame.
                    if !updated {
                        state.ctx.input.end_frame();
                        updated = true;
                    }
                }
                state.ctx.time.alpha = alpha(self.accumulator, step);
            }
            None => {
                state.game.update(&mut state.ctx, delta.as_secs_f32());
                state.ctx.input.end_frame();
            }
        }

        if state.ctx.quit_requested {
            event_loop.exit();
            return;
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpha_is_fraction_of_step() {
        let step = Duration::from_millis(10);
        assert_eq!(alpha(Duration::ZERO, step), 0.0);
        assert!((alpha(Duration::from_millis(5), step) - 0.5).abs() < 1e-6);
        assert_eq!(alpha(Duration::from_millis(5), Duration::ZERO), 0.0);
    }
}
