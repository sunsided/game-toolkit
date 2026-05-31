use std::sync::Arc;

use anyhow::Result;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::Rng;
use crate::time::Time;

pub enum GameEvent {
    Resized { width: u32, height: u32 },
    FocusChanged(bool),
    CloseRequested,
}

pub struct Context {
    pub gfx: game_toolkit_gfx::Graphics,
    pub audio: Option<game_toolkit_audio::Audio>,
    pub input: game_toolkit_input::Input,
    pub assets: game_toolkit_assets::Assets,
    pub time: Time,
    pub rng: Rng,
    pub window: Arc<Window>,
    pub(crate) quit_requested: bool,
}

impl Context {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        event_loop: &ActiveEventLoop,
        title: &str,
        width: u32,
        height: u32,
        vsync: bool,
        asset_root: std::path::PathBuf,
        random_seed: u64,
        depth_format: Option<wgpu::TextureFormat>,
        msaa_samples: u32,
    ) -> Result<Self> {
        let attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(winit::dpi::LogicalSize::new(width, height));
        let window = Arc::new(event_loop.create_window(attrs)?);
        let gfx = pollster::block_on(game_toolkit_gfx::Graphics::new(
            window.clone(),
            vsync,
            depth_format,
            msaa_samples,
        ))?;
        let audio = match game_toolkit_audio::Audio::new() {
            Ok(a) => Some(a),
            Err(e) => {
                log::warn!("audio init failed, continuing muted: {e}");
                None
            }
        };
        let input = game_toolkit_input::Input::new();
        let assets = game_toolkit_assets::Assets::new(&asset_root).unwrap_or_else(|e| {
            log::warn!(
                "asset root {} unusable ({e}); falling back to cwd",
                asset_root.display()
            );
            game_toolkit_assets::Assets::new(".").expect("cwd should resolve")
        });
        Ok(Self {
            gfx,
            audio,
            input,
            assets,
            time: Time::new(),
            rng: Rng::new(random_seed),
            window,
            quit_requested: false,
        })
    }

    pub fn quit(&mut self) {
        self.quit_requested = true;
    }
}
