//! Egui integration for the toolkit. Use for debug overlays, level inspectors, tweak sliders.
//!
//! Wire it into your game by:
//! 1. construct [`Ui::new`] once in `Game::init`,
//! 2. call [`Ui::on_window_event`] from `Game::event` (you need to forward the raw winit event;
//!    use [`raw_event_passthrough`] or pull the event from `GameEvent` in your own routing),
//! 3. call [`Ui::run`] inside `Game::render`, passing a closure that builds the UI.

#![forbid(unsafe_code)]

use std::sync::Arc;

use anyhow::Result;
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};
use egui_winit::State;
use winit::window::Window;

pub use egui;

pub struct Ui {
    pub ctx: egui::Context,
    state: State,
    renderer: Renderer,
    window: Arc<Window>,
}

impl Ui {
    pub fn new(gfx: &game_toolkit_gfx::Graphics) -> Result<Self> {
        let window = gfx.window().clone();
        let ctx = egui::Context::default();
        let viewport_id = ctx.viewport_id();
        let scale = Some(window.scale_factor() as f32);
        let state = State::new(ctx.clone(), viewport_id, window.as_ref(), scale, None, None);
        let renderer = Renderer::new(
            gfx.device(),
            gfx.surface_format(),
            RendererOptions::default(),
        );
        Ok(Self {
            ctx,
            state,
            renderer,
            window,
        })
    }

    /// Forward a raw winit `WindowEvent` to egui-winit. Call this from your `Game::event`
    /// implementation. Returns whether egui consumed the event (i.e. the game should ignore it).
    pub fn on_window_event(&mut self, event: &winit::event::WindowEvent) -> bool {
        let response = self.state.on_window_event(&self.window, event);
        response.consumed
    }

    /// Build, tessellate and render an egui frame on top of `frame`'s render target.
    /// `build` is the user's `|ctx: &egui::Context| { ... }` closure.
    pub fn run<F>(
        &mut self,
        gfx: &mut game_toolkit_gfx::Graphics,
        frame: &mut game_toolkit_gfx::Frame,
        build: F,
    ) where
        F: FnMut(&egui::Context),
    {
        // Make sure prior 2D layers were submitted.
        gfx.flush_pending(frame);

        let raw_input = self.state.take_egui_input(&self.window);
        // `Context::run` was deprecated in favor of `run_ui` (which yields a `&mut Ui` instead
        // of a `&Context`), but our public API still hands users the `Context` so they can
        // open windows / panels — they aren't editing a single root `Ui`. Silence the warning.
        #[allow(deprecated)]
        let full_output = self.ctx.run(raw_input, build);
        self.state
            .handle_platform_output(&self.window, full_output.platform_output);

        let pixels_per_point = self.ctx.pixels_per_point();
        let clipped = self.ctx.tessellate(full_output.shapes, pixels_per_point);
        let (w, h) = gfx.size();
        let screen = ScreenDescriptor {
            size_in_pixels: [w, h],
            pixels_per_point,
        };

        for (id, image_delta) in &full_output.textures_delta.set {
            self.renderer
                .update_texture(gfx.device(), gfx.queue(), *id, image_delta);
        }

        let (view, encoder) = frame.view_and_encoder();
        let extra_cmds =
            self.renderer
                .update_buffers(gfx.device(), gfx.queue(), encoder, &clipped, &screen);
        if !extra_cmds.is_empty() {
            gfx.queue().submit(extra_cmds);
        }

        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui.pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            self.renderer.render(&mut pass, &clipped, &screen);
        }

        for id in &full_output.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }
}
