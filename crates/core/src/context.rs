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

#[non_exhaustive]
pub enum GameEvent {
    Resized {
        width: u32,
        height: u32,
    },
    FocusChanged(bool),
    CloseRequested,
    /// Web only: the host page took the game's canvas out of the document (`false`: an SPA
    /// route change, a component unmounting) or put it back (`true`). The browser sends no
    /// frames to a detached canvas, so both arrive together, on the first frame after it
    /// returns; the audio output is already suspended in between (see
    /// [`Audio::suspend`](game_toolkit_audio::Audio::suspend) to keep it paused, e.g. behind a
    /// pause menu). Also see [`Context::canvas_attached`].
    CanvasAttached(bool),
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
    pub(crate) canvas_attached: bool,
    /// Waiting for the browser to say whether audio may start without a gesture.
    #[cfg(target_arch = "wasm32")]
    pub(crate) autoplay: Option<crate::web::AutoplayProbe>,
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
    let gfx = game_toolkit_gfx::Graphics::new(window, &config.graphics_config()).await?;
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
            canvas_attached: true,
            #[cfg(target_arch = "wasm32")]
            autoplay: None,
        }
    }

    /// Point the input's mouse and touch mapping at where the canvas lands in the window.
    pub(crate) fn sync_pointer_mapping(&mut self) {
        let mapping = self.gfx.canvas_rect().map(|rect| {
            let (w, h) = self.gfx.size();
            let scale = rect.canvas_per_window([w, h]);
            game_toolkit_input::PointerMapping {
                origin: (rect.origin[0], rect.origin[1]),
                scale: (scale[0], scale[1]),
            }
        });
        self.input.set_pointer_mapping(mapping);
    }

    pub fn quit(&mut self) {
        self.quit_requested = true;
    }

    /// Whether the game's canvas is in the page, as of the last [`GameEvent::CanvasAttached`].
    /// Always `true` natively.
    pub fn canvas_attached(&self) -> bool {
        self.canvas_attached
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

    /// Load a pixel font from a glyph sheet PNG in the asset root, laid out as `layout`
    /// describes (e.g. `GridLayout::ascii(6, 8)`).
    pub fn load_bitmap_font(
        &mut self,
        rel: impl AsRef<Path>,
        layout: &game_toolkit_gfx::GridLayout,
    ) -> Result<game_toolkit_gfx::BitmapFont> {
        let bytes = self.assets.read(rel)?;
        game_toolkit_gfx::BitmapFont::from_grid(&mut self.gfx, &bytes, layout)
    }

    /// Load an AngelCode BMFont (text `.fnt`, one page) from the asset root; its page image is
    /// read from the file the `.fnt` names, relative to the `.fnt`. On the web, list both in
    /// the preload manifest.
    pub fn load_bmfont(&mut self, rel: impl AsRef<Path>) -> Result<game_toolkit_gfx::BitmapFont> {
        let rel = rel.as_ref();
        let fnt = String::from_utf8(self.assets.read(rel)?)
            .map_err(|e| anyhow::anyhow!("{}: not UTF-8 ({e})", rel.display()))?;
        let page = game_toolkit_gfx::BitmapFont::bmfont_page_file(&fnt)
            .ok_or_else(|| anyhow::anyhow!("{}: no page file", rel.display()))?;
        let page = rel.parent().unwrap_or(Path::new("")).join(page);
        let png = self.assets.read(&page)?;
        game_toolkit_gfx::BitmapFont::from_bmfont(&mut self.gfx, &fnt, &png)
    }

    /// Load an indexed-colour PNG from the asset root as a texture of palette indices, for an
    /// indexed canvas (feature `indexed`); also returns the PNG's palette. See
    /// [`Graphics::enable_indexed`](game_toolkit_gfx::Graphics::enable_indexed).
    #[cfg(feature = "indexed")]
    pub fn load_indexed_texture(
        &mut self,
        rel: impl AsRef<Path>,
    ) -> Result<(TextureId, game_toolkit_gfx::Palette)> {
        let rel = rel.as_ref();
        let bytes = self.assets.read(rel)?;
        self.gfx
            .load_indexed_texture(&bytes, Some(&rel.to_string_lossy()))
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

    /// Open the audio device on the first user gesture, or at startup on the web if the page
    /// already had one. Browsers keep audio suspended until then; natively the device is
    /// already open and this is a no-op.
    pub(crate) fn start_audio(&mut self) {
        if let Some(audio) = self.audio.as_mut()
            && !audio.is_started()
            && let Err(e) = audio.start()
        {
            log::warn!("audio start failed, continuing muted: {e}");
            self.audio = None;
        }
    }

    /// Start audio as soon as the browser allows it without another user gesture, e.g. when
    /// the host page tells the game the player is back after interacting with the page. The
    /// toolkit does this by itself at startup; call it again when a gesture may have happened
    /// on the page since (outside the canvas, so the toolkit didn't see it).
    ///
    /// On the web it asks the browser and, if the answer is yes, starts the audio right away
    /// or within about 1.5 s (Firefox and WebKit take a moment to say so). Otherwise audio
    /// keeps waiting for the first key press, click or touch on the canvas. Natively the
    /// device is open from the start and this does nothing.
    pub fn start_audio_if_allowed(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if self.audio.as_ref().is_some_and(|a| !a.is_started()) && self.autoplay.is_none() {
            self.autoplay = crate::web::AutoplayProbe::start();
            self.poll_autoplay();
        }
    }

    /// Act on the autoplay probe's answer, if there is one yet.
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn poll_autoplay(&mut self) {
        use crate::web::Autoplay;
        let Some(probe) = &self.autoplay else {
            return;
        };
        // A gesture on the canvas may have started the audio in the meantime.
        let started = self
            .audio
            .as_ref()
            .is_none_or(game_toolkit_audio::Audio::is_started);
        let answer = if started {
            Autoplay::Denied
        } else {
            probe.poll()
        };
        match answer {
            Autoplay::Pending => {}
            Autoplay::Denied => self.autoplay = None,
            Autoplay::Allowed => {
                self.autoplay = None;
                log::info!("audio allowed without a gesture, starting it now");
                self.start_audio();
            }
        }
    }
}
