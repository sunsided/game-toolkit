use std::sync::Arc;

use anyhow::Result;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::time::Time;

pub enum GameEvent {
    Resized { width: u32, height: u32 },
    FocusChanged(bool),
    CloseRequested,
}

pub struct Context {
    pub gfx: toolkit_gfx::Graphics,
    pub audio: Option<toolkit_audio::Audio>,
    pub input: toolkit_input::Input,
    pub assets: toolkit_assets::Assets,
    pub time: Time,
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
        depth_format: Option<wgpu::TextureFormat>,
        msaa_samples: u32,
    ) -> Result<Self> {
        let attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(winit::dpi::LogicalSize::new(width, height));
        let window = Arc::new(event_loop.create_window(attrs)?);
        let gfx = pollster::block_on(toolkit_gfx::Graphics::new(
            window.clone(),
            vsync,
            depth_format,
            msaa_samples,
        ))?;
        let audio = match toolkit_audio::Audio::new() {
            Ok(a) => Some(a),
            Err(e) => {
                log::warn!("audio init failed, continuing muted: {e}");
                None
            }
        };
        let input = toolkit_input::Input::new();
        let assets = toolkit_assets::Assets::new(&asset_root).unwrap_or_else(|e| {
            log::warn!(
                "asset root {} unusable ({e}); falling back to cwd",
                asset_root.display()
            );
            toolkit_assets::Assets::new(".").expect("cwd should resolve")
        });
        Ok(Self {
            gfx,
            audio,
            input,
            assets,
            time: Time::new(),
            window,
            quit_requested: false,
        })
    }

    pub fn quit(&mut self) {
        self.quit_requested = true;
    }
}
