use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use game_toolkit_audio::SoundId;
use game_toolkit_gfx::TextureId;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

use crate::Rng;
use crate::app::AppConfig;
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

/// The asynchronous half of context setup: GPU init and (on the web) asset preloading.
/// Produced by [`init_parts`], turned into a [`Context`] by [`Context::from_parts`].
pub(crate) struct Parts {
    gfx: game_toolkit_gfx::Graphics,
    assets: game_toolkit_assets::Assets,
}

/// Create the game window. On the web it renders into the `<canvas>` named by
/// [`AppConfig::canvas_id`], or into a new canvas appended to `<body>`.
pub(crate) fn create_window(
    event_loop: &ActiveEventLoop,
    config: &AppConfig,
) -> Result<Arc<Window>> {
    let attrs = Window::default_attributes()
        .with_title(&config.title)
        .with_inner_size(winit::dpi::LogicalSize::new(config.width, config.height));
    #[cfg(target_arch = "wasm32")]
    let attrs = crate::web::canvas_attributes(attrs, config.canvas_id.as_deref())?;
    Ok(Arc::new(event_loop.create_window(attrs)?))
}

/// Initialize the GPU and the asset store. Awaited with `pollster` on native; on the web it
/// runs as a browser task because adapter/device requests resolve as JS promises.
pub(crate) async fn init_parts(window: Arc<Window>, config: AppConfig) -> Result<Parts> {
    let gfx = game_toolkit_gfx::Graphics::new(
        window,
        config.vsync,
        config.depth_format,
        config.msaa_samples,
    )
    .await?;
    let assets = open_assets(&config).await?;
    Ok(Parts { gfx, assets })
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::unused_async)]
async fn open_assets(config: &AppConfig) -> Result<game_toolkit_assets::Assets> {
    use game_toolkit_assets::Assets;
    let mut assets = Assets::new(&config.asset_root).or_else(|e| {
        log::warn!(
            "asset root {} unusable ({e}); falling back to cwd",
            config.asset_root.display()
        );
        Assets::new(".")
    })?;
    // Debug builds warn about assets the web build wouldn't preload.
    if let Some(paths) = &config.web_preload {
        assets.set_web_preload(paths);
    }
    Ok(assets)
}

#[cfg(target_arch = "wasm32")]
async fn open_assets(config: &AppConfig) -> Result<game_toolkit_assets::Assets> {
    let mut assets = game_toolkit_assets::Assets::new(&config.asset_root)?;
    let paths = match &config.web_preload {
        Some(paths) => paths.clone(),
        None => assets.fetch_manifest().await?,
    };
    log::info!("preloading {} asset(s)", paths.len());
    assets.preload(paths).await?;
    Ok(assets)
}

impl Context {
    pub(crate) fn from_parts(window: Arc<Window>, parts: Parts, config: &AppConfig) -> Self {
        let audio = match game_toolkit_audio::Audio::new() {
            Ok(a) => Some(a),
            Err(e) => {
                log::warn!("audio init failed, continuing muted: {e}");
                None
            }
        };
        let mut input = game_toolkit_input::Input::new();
        input.set_touch_emulates_mouse(config.touch_emulates_mouse);
        Self {
            gfx: parts.gfx,
            audio,
            input,
            assets: parts.assets,
            time: Time::new(),
            rng: Rng::new(config.random_seed),
            window,
            quit_requested: false,
        }
    }

    pub fn quit(&mut self) {
        self.quit_requested = true;
    }

    /// Load a PNG from the asset root. Works on every platform: natively it reads the file and
    /// registers it for hot-reload ([`game_toolkit_gfx::Graphics::refresh_textures`]); on the
    /// web it decodes the preloaded bytes.
    pub fn load_texture(&mut self, rel: impl AsRef<Path>) -> Result<TextureId> {
        let rel = rel.as_ref();
        #[cfg(not(target_arch = "wasm32"))]
        if !self.assets.is_loaded(rel) {
            let id = self.gfx.load_texture(self.assets.resolve(rel))?;
            self.assets.check_web_listed(rel);
            return Ok(id);
        }
        let bytes = self.assets.read(rel)?;
        self.gfx
            .load_texture_bytes(&bytes, Some(&rel.to_string_lossy()))
    }

    /// Load a sound (wav/ogg/flac/mp3) from the asset root. `Ok(None)` when audio is muted
    /// because the backend failed to initialize.
    pub fn load_sound(&mut self, rel: impl AsRef<Path>) -> Result<Option<SoundId>> {
        let Some(audio) = self.audio.as_mut() else {
            return Ok(None);
        };
        let bytes = self.assets.read(rel)?;
        audio.load_sound_bytes(bytes).map(Some)
    }

    /// Open the audio device on the first user gesture. Browsers keep audio suspended until
    /// then; natively the device is already open and this is a no-op.
    pub(crate) fn start_audio(&mut self) {
        if let Some(audio) = self.audio.as_mut()
            && !audio.is_started()
            && let Err(e) = audio.start()
        {
            log::warn!("audio start failed, continuing muted: {e}");
            self.audio = None;
        }
    }
}
