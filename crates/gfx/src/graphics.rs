use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::bitmap_font::BitmapFont;
use crate::camera::{Camera2D, CameraUniform};
use crate::camera3d::{Camera, Camera3D};
use crate::canvas::{self, Blit, Canvas, CanvasRect, RenderTexture, ScaleFilter, ScaleMode};
use crate::color::Color;
use crate::config::{self, GraphicsConfig};
use crate::custom::CustomPass;
use crate::frame::Frame;
use crate::mesh::{MeshBatcher, MeshId, MeshInstance, MeshRegistry, MeshVertex};
use crate::post::{self, PostChain, PostEffect, PostEffectId, PostStage, StageRun};
use crate::primitives::PrimitiveBatcher;
use crate::shapes::ShapeBatcher;
use crate::sprite::SpriteBatcher;
use crate::target::Targets;
use crate::text::TextSystem;
use crate::texture::{TextureId, TextureRegistry, TextureWrap};
#[cfg(feature = "vector")]
use crate::vector::VectorPass;

pub struct Graphics {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    window: Arc<Window>,
    pub camera: Camera2D,
    camera_buf: wgpu::Buffer,
    pub(crate) camera_bg: wgpu::BindGroup,
    pub(crate) sprites: SpriteBatcher,
    pub(crate) primitives: PrimitiveBatcher,
    pub(crate) shapes: ShapeBatcher,
    pub(crate) text: TextSystem,
    pub(crate) textures: TextureRegistry,
    /// Passes queued with `Painter::custom`, run at their layer.
    pub(crate) custom: crate::custom::CustomPasses,
    pub(crate) clear_color: [f32; 4],
    texture_paths: HashMap<PathBuf, TextureId>,
    sample_count: u32,
    depth_format: Option<wgpu::TextureFormat>,
    /// Multisampled color target (resolved to the surface) when `sample_count > 1`.
    msaa_view: Option<wgpu::TextureView>,
    /// Depth attachment when `depth_format` is set. Recreated on resize.
    depth_view: Option<wgpu::TextureView>,
    /// Perspective camera for 3D meshes. Set its fields to move the view.
    pub camera3d: Camera3D,
    camera3d_buf: wgpu::Buffer,
    camera3d_bg: wgpu::BindGroup,
    meshes: MeshRegistry,
    mesh_batcher: MeshBatcher,
    /// Vello vector backend; composites over the 2D layers each frame. `None` on adapters
    /// without compute shaders (WebGL2), where vector drawing is skipped.
    #[cfg(feature = "vector")]
    pub(crate) vector: Option<VectorPass>,
    /// Whether the surface supports `COPY_SRC` (required to read frames back for screenshots).
    capture_copy_src: bool,
    /// Path to write a screenshot of the next presented frame, set by `request_screenshot`.
    pending_screenshot: Option<PathBuf>,
    adapter_info: wgpu::AdapterInfo,
    supports_compute: bool,
    /// The offscreen scene the painter draws into: the virtual-resolution canvas, or a
    /// window-sized one while post effects are registered. `None`: straight to the surface.
    canvas: Option<Canvas>,
    virtual_resolution: Option<(u32, u32)>,
    blit: Blit,
    post: PostChain,
    /// Window-sized target the canvas is scaled into when window-stage effects run.
    window_scratch: Option<RenderTexture>,
    start: web_time::Instant,
    frame_count: u32,
    #[cfg(feature = "indexed")]
    indexed: Option<crate::indexed::IndexedState>,
    /// `disable_indexed` was called mid-frame; applied once the frame is presented.
    #[cfg(feature = "indexed")]
    disable_indexed_after_frame: bool,
    /// Between `begin_frame` and `present`.
    frame_active: bool,
    /// The built-in pixel font, uploaded on first use.
    pub(crate) builtin_font: Option<BitmapFont>,
    scale_mode: ScaleMode,
    scale_filter: ScaleFilter,
    border_color: Color,
}

impl Graphics {
    /// Create the surface and device for `window`. Fails with a
    /// [`DeviceRequirementsError`](crate::DeviceRequirementsError) when the adapter lacks a
    /// limit or feature from `config`.
    pub async fn new(window: Arc<Window>, config: &GraphicsConfig) -> Result<Self> {
        let GraphicsConfig {
            vsync,
            depth_format,
            msaa_samples,
            virtual_resolution,
            scale_mode,
            scale_filter,
            border_color,
            ..
        } = *config;
        let size = window.inner_size();
        let (width, height) = (size.width.max(1), size.height.max(1));

        let mut idesc = wgpu::InstanceDescriptor::new_without_display_handle();
        // Web: WebGPU where the browser has it, WebGL2 otherwise. Native: Vulkan/Metal/DX12.
        idesc.backends = if cfg!(target_arch = "wasm32") {
            wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL
        } else {
            wgpu::Backends::PRIMARY
        };
        // A browser may expose `navigator.gpu` yet hand out no adapter (headless Chrome, blocked
        // GPUs). This probes for a real WebGPU adapter and drops to WebGL2 when there is none;
        // natively it is a plain `Instance::new`.
        let instance = wgpu::util::new_instance_with_webgpu_detection(idesc).await;
        let surface = instance
            .create_surface(window.clone())
            .context("create_surface")?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .context("no compatible adapter")?;

        let info = adapter.get_info();
        let backend = info.backend;
        log::info!("wgpu adapter: {:?} ({backend:?})", info.name);
        let adapter_limits = adapter.limits();
        let (required_limits, required_features) =
            config::merged_requirements(config, backend, &adapter_limits);
        config::check_requirements(
            &info.name,
            backend,
            &adapter_limits,
            adapter.features(),
            &required_limits,
            required_features,
        )?;
        let mut device_desc = wgpu::DeviceDescriptor {
            label: Some("toolkit.device"),
            required_features,
            required_limits,
            memory_hints: wgpu::MemoryHints::Performance,
            ..Default::default()
        };
        if let Some(hook) = &config.device_hook {
            hook(&adapter, &mut device_desc);
        }
        let (device, queue) = adapter
            .request_device(&device_desc)
            .await
            .context("request_device")?;
        let supports_compute = adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS);

        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(caps.formats[0]);
        // Always render through an sRGB view, so shaders write linear color and the hardware
        // encodes it, the same on every platform. Surfaces without an sRGB format (WebGPU
        // canvases) still allow the sRGB variant as a view format.
        let format = surface_format.add_srgb_suffix();
        let present_mode = if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        // Add COPY_SRC when the surface supports it so frames can be read back for
        // screenshots; fall back to render-only otherwise (screenshots become a no-op).
        let capture_copy_src = caps.usages.contains(wgpu::TextureUsages::COPY_SRC);
        let surface_usage = if capture_copy_src {
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC
        } else {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        };
        let surface_config = wgpu::SurfaceConfiguration {
            usage: surface_usage,
            format: surface_format,
            width,
            height,
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: if format == surface_format {
                vec![]
            } else {
                vec![format]
            },
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &surface_config);

        // With a virtual resolution everything the painter draws goes into the canvas, so the
        // cameras, text, attachments and vector target are all sized to it.
        let blit = Blit::new(&device, format);
        let canvas = virtual_resolution.map(|(w, h)| Canvas {
            tex: RenderTexture::new(&device, &blit, format, [w, h], "canvas"),
            follows_window: false,
        });
        let [width, height] = canvas.as_ref().map_or([width, height], Canvas::size);

        let camera = Camera2D::new(width as f32, height as f32);
        let camera_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera.buf"),
            contents: bytemuck::bytes_of(&CameraUniform {
                view_proj: camera.view_proj(),
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera.bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera.bg"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        let camera3d = Camera3D::new(width as f32 / height.max(1) as f32);
        let camera3d_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera3d.buf"),
            contents: bytemuck::bytes_of(&CameraUniform {
                view_proj: camera3d.view_proj(),
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera3d_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera3d.bg"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera3d_buf.as_entire_binding(),
            }],
        });

        let sample_count = msaa_samples.max(1);
        let textures = TextureRegistry::new(&device, &queue);
        let sprites = SpriteBatcher::new(
            &device,
            format,
            &camera_bgl,
            &textures.layout,
            sample_count,
            depth_format,
        );
        let primitives =
            PrimitiveBatcher::new(&device, format, &camera_bgl, sample_count, depth_format);
        let shapes = ShapeBatcher::new(
            &device,
            format,
            &camera_bgl,
            &textures.layout,
            sample_count,
            depth_format,
        );
        let text = TextSystem::new(
            &device,
            &queue,
            format,
            width,
            height,
            sample_count,
            depth_format,
        );
        let (msaa_view, depth_view) =
            make_attachments(&device, [width, height], format, sample_count, depth_format);
        let meshes = MeshRegistry::new();
        let mesh_batcher =
            MeshBatcher::new(&device, format, &camera_bgl, sample_count, depth_format);
        #[cfg(feature = "vector")]
        let vector = if supports_compute {
            Some(VectorPass::new(&device, format, width, height))
        } else {
            log::error!("vector graphics disabled: adapter has no compute shaders ({backend:?})");
            None
        };

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,
            window,
            camera,
            camera_buf,
            camera_bg,
            sprites,
            primitives,
            shapes,
            text,
            textures,
            custom: crate::custom::CustomPasses::default(),
            clear_color: [0.0, 0.0, 0.0, 1.0],
            texture_paths: HashMap::new(),
            sample_count,
            depth_format,
            msaa_view,
            depth_view,
            camera3d,
            camera3d_buf,
            camera3d_bg,
            meshes,
            mesh_batcher,
            #[cfg(feature = "vector")]
            vector,
            capture_copy_src,
            pending_screenshot: None,
            adapter_info: info,
            supports_compute,
            canvas,
            virtual_resolution,
            blit,
            post: PostChain::default(),
            window_scratch: None,
            start: web_time::Instant::now(),
            frame_count: 0,
            builtin_font: None,
            #[cfg(feature = "indexed")]
            indexed: None,
            #[cfg(feature = "indexed")]
            disable_indexed_after_frame: false,
            frame_active: false,
            scale_mode,
            scale_filter,
            border_color,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        // With a virtual resolution the drawing size stays put; only where the canvas lands
        // in the window moves.
        self.sync_canvas();
    }

    /// Make the offscreen canvas match the virtual resolution and post effects, and resize
    /// the render targets if the drawing size changed.
    fn sync_canvas(&mut self) {
        let before = self.canvas.as_ref().map(Canvas::size);
        let (w, h) = self.window_size();
        let wanted = match self.virtual_resolution {
            Some((cw, ch)) => Some(([cw.max(1), ch.max(1)], false)),
            None if !self.post.is_empty() || self.is_indexed() => Some(([w, h], true)),
            None => None,
        };
        let current = self.canvas.as_ref().map(|c| (c.size(), c.follows_window));
        if current != wanted {
            self.canvas = wanted.map(|(size, follows_window)| Canvas {
                tex: RenderTexture::new(
                    &self.device,
                    &self.blit,
                    self.surface_format(),
                    size,
                    "canvas",
                ),
                follows_window,
            });
        }
        let after = self.canvas.as_ref().map(Canvas::size);
        // Without a canvas the window size drives everything; resize on every call then.
        if after != before || after.is_none() {
            let (w, h) = self.size();
            self.resize_render_targets(w, h);
        }
    }

    /// Resize everything that draws at the render size (the canvas, else the window).
    fn resize_render_targets(&mut self, width: u32, height: u32) {
        let (msaa_view, depth_view) = make_attachments(
            &self.device,
            [width, height],
            self.surface_format(),
            self.sample_count,
            self.depth_format,
        );
        self.msaa_view = msaa_view;
        self.depth_view = depth_view;
        self.camera.resize(width as f32, height as f32);
        self.camera3d.resize(width as f32, height as f32);
        self.text.resize(&self.queue, width, height);
        #[cfg(feature = "vector")]
        if let Some(vector) = self.vector.as_mut() {
            vector.resize(&self.device, width, height);
        }
    }

    /// Draw into a fixed-size canvas of `(width, height)` pixels that is scaled onto the
    /// window, or (`None`) straight into the window again. Usually set once through
    /// `AppConfig::virtual_resolution`; changing it at runtime reallocates the canvas.
    /// See [`GraphicsConfig::virtual_resolution`].
    pub fn set_virtual_resolution(&mut self, resolution: Option<(u32, u32)>) {
        // A zero-sized canvas would give the cameras a zero viewport; use at least 1 x 1.
        let resolution = resolution.map(|(w, h)| (w.max(1), h.max(1)));
        if self.virtual_resolution == resolution {
            return;
        }
        self.virtual_resolution = resolution;
        self.sync_canvas();
    }

    /// The virtual resolution set by [`Self::set_virtual_resolution`], if any.
    pub fn virtual_resolution(&self) -> Option<(u32, u32)> {
        self.virtual_resolution
    }

    /// Run `effect` on every frame from now on, after the effects already in `stage`. See
    /// [`PostStage`] for the two stages and [`ShaderEffect`](crate::ShaderEffect) for
    /// single-shader effects. While any effect is registered, the scene is drawn into an
    /// offscreen texture (window-sized without a virtual resolution).
    pub fn add_post_effect(&mut self, stage: PostStage, effect: impl PostEffect) -> PostEffectId {
        let id = self.post.add(stage, Box::new(effect));
        self.sync_canvas();
        id
    }

    /// Whether the canvas holds palette indices (feature `indexed`); always `false` without it.
    pub fn is_indexed(&self) -> bool {
        #[cfg(feature = "indexed")]
        return self.indexed.is_some();
        #[cfg(not(feature = "indexed"))]
        false
    }

    /// Switch to indexed colour with `palette` in row 0: from the next frame on, the painter
    /// draws palette indices ([`Color::index`], textures from
    /// [`Self::load_indexed_texture`]) and the end of the frame looks them up, before post
    /// effects and the upscale. Works with or without a virtual resolution. Keep the
    /// [`PaletteCycle`](crate::PaletteCycle)s and rows, reset by [`Self::disable_indexed`].
    #[cfg(feature = "indexed")]
    pub fn enable_indexed(&mut self, palette: &crate::Palette) {
        // The latest call wins: cancel a `disable_indexed` deferred earlier in this frame.
        self.disable_indexed_after_frame = false;
        let format = self.surface_format();
        let state = self.indexed.get_or_insert_with(|| {
            crate::indexed::IndexedState::new(&self.device, &self.queue, format)
        });
        state.set_palette(&self.queue, 0, palette);
        self.sync_canvas();
    }

    /// Back to direct RGBA drawing; drops palettes and cycles.
    #[cfg(feature = "indexed")]
    pub fn disable_indexed(&mut self) {
        // During a frame, the canvas already holds indices: keep the palette until that frame
        // is resolved and presented, then switch.
        if self.frame_active {
            self.disable_indexed_after_frame = true;
            return;
        }
        self.indexed = None;
        self.sync_canvas();
    }

    /// Fill palette row `row` (`0..PALETTE_ROWS`). Row 0 shows unless
    /// [`Self::set_palette_blend`] picks others. Takes effect this frame; ignored when not
    /// indexed.
    #[cfg(feature = "indexed")]
    pub fn set_palette(&mut self, row: u32, palette: &crate::Palette) {
        if let Some(ix) = self.indexed.as_mut() {
            ix.set_palette(&self.queue, row, palette);
        }
    }

    /// Show row `row_a` crossfaded towards `row_b` by `t` (`0` = all `a`, `1` = all `b`), e.g.
    /// a day-night fade or a damage flash. Default: row 0, no blend.
    #[cfg(feature = "indexed")]
    pub fn set_palette_blend(&mut self, row_a: u32, row_b: u32, t: f32) {
        if let Some(ix) = self.indexed.as_mut() {
            ix.set_blend(row_a, row_b, t);
        }
    }

    /// Start rotating a range of palette entries; `None` when not indexed, when the range is
    /// empty or runs past index 255, or when
    /// [`MAX_PALETTE_CYCLES`](crate::MAX_PALETTE_CYCLES) are already running.
    #[cfg(feature = "indexed")]
    pub fn add_palette_cycle(
        &mut self,
        cycle: crate::PaletteCycle,
    ) -> Option<crate::PaletteCycleId> {
        self.indexed.as_mut()?.add_cycle(cycle)
    }

    /// Stop a palette cycle; returns whether it existed.
    #[cfg(feature = "indexed")]
    pub fn remove_palette_cycle(&mut self, id: crate::PaletteCycleId) -> bool {
        self.indexed.as_mut().is_some_and(|ix| ix.remove_cycle(id))
    }

    /// Upload an [`IndexedImage`](crate::IndexedImage) as a texture of indices for an indexed
    /// canvas: draw it like any sprite (untinted) and its pixels come out in the canvas
    /// palette's colours. Transparent indices (`tRNS` alpha 0) stay transparent.
    #[cfg(feature = "indexed")]
    pub fn create_indexed_texture(
        &mut self,
        image: &crate::IndexedImage,
        label: Option<&str>,
    ) -> TextureId {
        self.create_texture_rgba(image.width, image.height, &image.index_texels(), label)
    }

    /// Decode an indexed PNG and upload its indices ([`Self::create_indexed_texture`]);
    /// returns the texture and the PNG's own palette, e.g. for
    /// [`Self::enable_indexed`].
    #[cfg(feature = "indexed")]
    pub fn load_indexed_texture(
        &mut self,
        png: &[u8],
        label: Option<&str>,
    ) -> Result<(TextureId, crate::Palette)> {
        let image = crate::IndexedImage::from_png(png)?;
        let id = self.create_indexed_texture(&image, label);
        Ok((id, image.palette))
    }

    /// Remove an effect; returns whether it existed.
    pub fn remove_post_effect(&mut self, id: PostEffectId) -> bool {
        let removed = self.post.remove(id);
        self.sync_canvas();
        removed
    }

    /// Switch an effect off (skipped, keeps its place) or back on. Unknown ids are ignored.
    pub fn set_post_effect_enabled(&mut self, id: PostEffectId, enabled: bool) {
        if !self.post.set_enabled(id, enabled) {
            log::warn!("set_post_effect_enabled: unknown {id:?}");
        }
    }

    /// Whether an effect is enabled; `None` for an unknown id.
    pub fn post_effect_enabled(&self, id: PostEffectId) -> Option<bool> {
        self.post.is_enabled(id)
    }

    /// The effect behind `id` as its concrete type, to change its settings, e.g.
    /// `gfx.post_effect_mut::<ShaderEffect>(id).unwrap().set_params(&p)`. `None` for an
    /// unknown id or another type.
    pub fn post_effect_mut<T: PostEffect>(&mut self, id: PostEffectId) -> Option<&mut T> {
        self.post.get_mut(id)?.downcast_mut::<T>()
    }

    /// How the canvas is scaled onto the window. Takes effect on the next frame.
    pub fn set_scale_mode(&mut self, mode: ScaleMode) {
        self.scale_mode = mode;
    }

    pub fn scale_mode(&self) -> ScaleMode {
        self.scale_mode
    }

    /// Nearest (crisp, default) or linear filtering when scaling the canvas.
    pub fn set_scale_filter(&mut self, filter: ScaleFilter) {
        self.scale_filter = filter;
    }

    pub fn scale_filter(&self) -> ScaleFilter {
        self.scale_filter
    }

    /// Colour of the window area around the canvas (letterbox / pillarbox bars).
    pub fn set_border_color(&mut self, color: impl Into<Color>) {
        self.border_color = color.into();
    }

    pub fn border_color(&self) -> Color {
        self.border_color
    }

    /// Where the canvas lands in the window, in physical pixels, or `None` without a virtual
    /// resolution. Use [`CanvasRect::window_to_canvas`] to map window positions; the toolkit's
    /// `Input` already reports the mouse and touches in canvas pixels.
    pub fn canvas_rect(&self) -> Option<CanvasRect> {
        let (w, h) = self.virtual_resolution?;
        Some(canvas::layout(
            [self.surface_config.width, self.surface_config.height],
            [w, h],
            self.scale_mode,
        ))
    }

    /// The offscreen texture the painter draws into, when there is one (a virtual resolution
    /// is set or post effects are registered), for games that record their own passes into it (in [`Self::surface_format`], at
    /// [`Self::size`]). Draw after the painter has flushed (it clears the canvas) and before
    /// [`Self::flush_pending`] or the end of the frame, which scale it onto the window.
    pub fn canvas_view(&self) -> Option<&wgpu::TextureView> {
        self.canvas.as_ref().map(|c| &c.tex.view)
    }

    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    /// The size the painter draws at, in pixels: the virtual resolution when one is set,
    /// else the window's size. Lay out the game with this.
    pub fn size(&self) -> (u32, u32) {
        match &self.canvas {
            Some(c) => (c.size()[0], c.size()[1]),
            None => self.window_size(),
        }
    }

    /// The window's (surface's) size in physical pixels, whether or not a virtual resolution
    /// is set. Overlays drawn onto the window itself (egui) use this.
    pub fn window_size(&self) -> (u32, u32) {
        (self.surface_config.width, self.surface_config.height)
    }

    /// The color format the frame is rendered in: always an sRGB format, possibly a view of a
    /// non-sRGB surface. Build render pipelines that draw into a [`Frame`] with this.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        render_format(&self.surface_config)
    }

    /// Multisample state matching the painter's render target (`AppConfig::msaa_samples`),
    /// for pipelines that draw in a [`Painter::custom`](crate::Painter::custom) pass.
    pub fn multisample(&self) -> wgpu::MultisampleState {
        crate::target::multisample(self.sample_count)
    }

    /// Depth-stencil state matching the painter's render target, for pipelines that draw in
    /// a [`Painter::custom`](crate::Painter::custom) pass: `None` without a depth buffer, else
    /// one that neither tests nor writes depth.
    pub fn depth_stencil(&self) -> Option<wgpu::DepthStencilState> {
        self.depth_format.map(crate::target::no_write_depth)
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// The GPU adapter the device was created on: its name, vendor and backend.
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.adapter_info
    }

    /// The graphics backend in use, e.g. `Vulkan`, `Metal`, `BrowserWebGpu` or `Gl`
    /// (WebGL2 on the web).
    pub fn backend(&self) -> wgpu::Backend {
        self.adapter_info.backend
    }

    /// Whether compute shaders are available. `false` on WebGL2, the web fallback when the
    /// browser has no WebGPU; check this in `Game::init` to show a "WebGPU required" message
    /// instead of failing. The device's limits and features are on [`Self::device`].
    pub fn supports_compute(&self) -> bool {
        self.supports_compute
    }

    /// Load a PNG from disk and remember its path for [`Graphics::refresh_textures`]. Not
    /// available on the web; use [`Graphics::load_texture_bytes`] (or `ctx.load_texture`,
    /// which picks the right one per platform).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_texture(&mut self, path: impl AsRef<Path>) -> Result<TextureId> {
        let path = path.as_ref();
        let id = self.textures.load_file(&self.device, &self.queue, path)?;
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.texture_paths.insert(canon, id);
        Ok(id)
    }

    /// Size `[width, height]` in pixels that `s` takes up drawn at `size_px` (the widest line,
    /// and the height of all lines), for aligning text or sizing a box to fit. Uses the same
    /// layout as drawing, so the result matches `Painter::text` exactly; measuring and then
    /// drawing the same string shapes it once.
    pub fn measure_text(&mut self, s: &str, size_px: f32) -> [f32; 2] {
        self.text.measure(s, size_px)
    }

    /// The built-in 5 × 7 pixel font (6 × 8 cells, printable ASCII), uploaded on first use.
    /// [`Painter::pixel_text`](crate::Painter::pixel_text) draws with it; use this to
    /// [`measure`](BitmapFont::measure) text.
    pub fn builtin_font(&mut self) -> &BitmapFont {
        if self.builtin_font.is_none() {
            self.builtin_font = Some(BitmapFont::builtin(self));
        }
        self.builtin_font.as_ref().expect("just created")
    }

    /// Register an extra font face (TTF/OTF bytes) as a glyph fallback for text rendering.
    pub fn load_font_bytes(&mut self, bytes: Vec<u8>) {
        self.text.load_font_bytes(bytes);
    }

    /// Decode a PNG held in memory and upload it. `label` names the texture in GPU debuggers
    /// and error messages.
    pub fn load_texture_bytes(&mut self, bytes: &[u8], label: Option<&str>) -> Result<TextureId> {
        self.textures
            .load_bytes(&self.device, &self.queue, bytes, label)
    }

    /// Re-decode any texture whose backing file is in `changed_paths` and update its bind
    /// group in place. Returns the number of textures actually reloaded.
    pub fn refresh_textures<I, P>(&mut self, changed_paths: I) -> usize
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut n = 0;
        for p in changed_paths {
            let canon = p
                .as_ref()
                .canonicalize()
                .unwrap_or_else(|_| p.as_ref().to_path_buf());
            if let Some(&id) = self.texture_paths.get(&canon) {
                match self.textures.reload(&self.device, &self.queue, id, &canon) {
                    Ok(()) => n += 1,
                    Err(e) => log::warn!("reload {} failed: {e:?}", canon.display()),
                }
            }
        }
        n
    }

    pub fn create_texture_rgba(
        &mut self,
        width: u32,
        height: u32,
        rgba: &[u8],
        label: Option<&str>,
    ) -> TextureId {
        self.textures
            .create_from_rgba(&self.device, &self.queue, width, height, rgba, label)
    }

    /// Overwrite the pixels of an existing texture without reallocating it, for content that
    /// changes every frame (a software-rendered framebuffer, a video frame). `rgba` must hold
    /// `width * height * 4` bytes for the texture's size; create the texture once with
    /// [`Self::create_texture_rgba`]. Unknown ids and size mismatches are logged and ignored.
    pub fn update_texture_rgba(&mut self, id: TextureId, rgba: &[u8]) {
        if !self.textures.write_rgba(&self.queue, id, rgba) {
            log::warn!("update_texture_rgba: unknown or unwritable {id:?}, or wrong data size");
        }
    }

    /// Make a texture the game created itself drawable like any other [`TextureId`]: the
    /// output of a compute shader, a render target, a video frame decoded on the GPU. The
    /// registry takes ownership of `texture` and samples it through `view` (nearest filtering,
    /// [`TextureWrap::Clamp`] until changed with [`Self::set_texture_wrap`]).
    ///
    /// `texture` needs [`wgpu::TextureUsages::TEXTURE_BINDING`] and `view` a filterable float
    /// format with a 2D dimension. The sprite shader reads `view` as linear color, so use an
    /// sRGB view (e.g. `Rgba8UnormSrgb`) for content authored in sRGB like PNG pixels. To
    /// write such a texture from a compute shader, create it as `Rgba8Unorm` with
    /// `STORAGE_BINDING` and `view_formats: &[Rgba8UnormSrgb]`, store through an `Rgba8Unorm`
    /// view and register an `Rgba8UnormSrgb` one created with `usage:
    /// Some(TEXTURE_BINDING)` (an sRGB view can't carry `STORAGE_BINDING`).
    /// [`Self::update_texture_rgba`] only writes textures with `COPY_DST` and 4 bytes per
    /// texel; others are logged and skipped.
    pub fn register_texture(
        &mut self,
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        label: Option<&str>,
    ) -> TextureId {
        self.textures.register(&self.device, texture, view, label)
    }

    pub fn white_texture(&self) -> TextureId {
        self.textures.white()
    }

    /// Choose what `id` shows for UVs outside `0..1`: [`TextureWrap::Clamp`] (the default for
    /// every texture) stretches the edge texels, [`TextureWrap::Repeat`] and
    /// [`TextureWrap::MirrorRepeat`] tile. Set `Repeat` on a texture to tile it across one
    /// quad (`Painter::quad_textured` with UVs up to `[4.0, 10.0]`, a sprite with
    /// [`crate::SpriteInstance::with_uv`] beyond `1.0`) instead of drawing a quad per tile.
    /// Keep atlases on `Clamp`: their regions sample right up to the edge, and with wrapping
    /// a UV of exactly `1.0` bleeds in the opposite edge. The mode survives hot-reloads.
    /// Unknown ids are ignored.
    pub fn set_texture_wrap(&mut self, id: TextureId, wrap: TextureWrap) {
        if !self.textures.set_wrap(&self.device, id, wrap) {
            log::warn!("set_texture_wrap: unknown {id:?}");
        }
    }

    /// The [`TextureWrap`] mode of `id`, or `None` for an unknown id.
    pub fn texture_wrap(&self, id: TextureId) -> Option<TextureWrap> {
        self.textures.get(id).map(|t| t.wrap)
    }

    /// Pixel size `[width, height]` of a loaded texture, or `None` for an unknown id. Follows
    /// hot-reloads, so it reports the size of the current contents.
    pub fn texture_size(&self, id: TextureId) -> Option<[u32; 2]> {
        self.textures.get(id).map(|t| t.size)
    }

    /// Upload a static mesh (positions + normals, indexed) and return its handle. Meshes are
    /// drawn depth-tested via [`Graphics::draw_mesh`] using the perspective [`Self::camera3d`].
    pub fn create_mesh(&mut self, vertices: &[MeshVertex], indices: &[u16]) -> MeshId {
        self.meshes.create(&self.device, vertices, indices)
    }

    /// Queue one instance of `mesh` for this frame. Meshes render before the 2D layers, so
    /// 2D sprites, primitives and text draw on top of them.
    pub fn draw_mesh(&mut self, mesh: MeshId, instance: MeshInstance) {
        self.mesh_batcher.push(mesh, instance);
    }

    /// Save a PNG of the next presented frame to `path`. The read-back stalls that frame, so
    /// this is for debug/dev use. A no-op (with a warning) if the surface lacks `COPY_SRC`,
    /// and on the web, where there is neither a blocking read-back nor a filesystem.
    pub fn request_screenshot(&mut self, path: impl Into<PathBuf>) {
        if cfg!(target_arch = "wasm32") {
            log::warn!("screenshot unsupported on the web");
            return;
        }
        if !self.capture_copy_src {
            log::warn!("screenshot unsupported: surface does not allow COPY_SRC read-back");
            return;
        }
        self.pending_screenshot = Some(path.into());
    }

    pub fn begin_frame(&mut self) -> Result<Frame> {
        self.frame_count = self.frame_count.wrapping_add(1);
        self.queue.write_buffer(
            &self.camera_buf,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj: self.camera.view_proj(),
            }),
        );
        self.queue.write_buffer(
            &self.camera3d_buf,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj: self.camera3d.view_proj(),
            }),
        );

        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => anyhow::bail!("surface unavailable: {other:?}"),
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor {
                format: Some(render_format(&self.surface_config)),
                ..Default::default()
            });
        let encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame.encoder"),
            });
        self.frame_active = true;
        Ok(Frame {
            encoder: Some(encoder),
            view,
            surface_texture: Some(surface_texture),
            canvas: self.canvas.as_ref().map(|c| c.tex.view.clone()),
            canvas_size: self.canvas.as_ref().map_or([0, 0], Canvas::size),
            canvas_virtual: self.virtual_resolution.is_some(),
            clear_color: self.clear_color,
            flushed: false,
            composited: false,
        })
    }

    pub fn present(&mut self, mut frame: Frame) {
        if !frame.flushed {
            self.flush_into(&mut frame);
        }
        self.composite(&mut frame);

        if let Some(encoder) = frame.encoder.take() {
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // Capture the just-submitted surface before presenting (the queue runs the read-back
        // copy after the render submit, so it sees the finished frame).
        let shot_path = self.pending_screenshot.take();
        if let (Some(path), Some(st)) = (&shot_path, frame.surface_texture.as_ref()) {
            self.save_screenshot(&st.texture, path);
        }

        if let Some(st) = frame.surface_texture.take() {
            self.window.pre_present_notify();
            self.queue.present(st);
        }
        self.frame_active = false;
        #[cfg(feature = "indexed")]
        if std::mem::take(&mut self.disable_indexed_after_frame) {
            self.disable_indexed();
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn save_screenshot(&self, _texture: &wgpu::Texture, _path: &Path) {}

    /// Read the surface texture back to the CPU and save it. Synchronous (blocks on a poll),
    /// so only used for the occasional debug screenshot. `miniscreenshot-wgpu` handles the
    /// staging copy, row-padding strip, and BGRA->RGBA conversion.
    #[cfg(not(target_arch = "wasm32"))]
    fn save_screenshot(&self, texture: &wgpu::Texture, path: &Path) {
        match miniscreenshot_wgpu::capture(&self.device, &self.queue, texture) {
            Ok(shot) => match shot.save(path) {
                Ok(()) => log::info!("saved screenshot {}", path.display()),
                Err(e) => log::warn!("screenshot save failed: {e}"),
            },
            Err(e) => log::warn!("screenshot capture failed: {e}"),
        }
    }

    /// Flush all queued 2D layers (sprites + shapes + primitives + text) into the current
    /// `frame` and, with a virtual resolution, scale the canvas onto the window. Call it
    /// before stacking passes that draw onto the window itself (e.g. egui); after it, the
    /// window view holds the finished scene. The end of the frame does the same if needed.
    pub fn flush_pending(&mut self, frame: &mut Frame) {
        self.flush_into(frame);
        self.composite(frame);
    }

    /// Run the post effects and scale the canvas onto the window, once per frame; nothing
    /// when the frame was drawn straight into the window.
    fn composite(&mut self, frame: &mut Frame) {
        if frame.composited {
            return;
        }
        frame.composited = true;
        // A canvas created mid-frame (an effect added during `render`) wasn't drawn into.
        if frame.canvas.is_none() {
            return;
        }
        let window = [self.surface_config.width, self.surface_config.height];
        let rect = self.canvas_rect().unwrap_or(CanvasRect {
            origin: [0.0, 0.0],
            size: [window[0] as f32, window[1] as f32],
        });
        let format = self.surface_format();
        let b = self.border_color;
        let border = wgpu::Color {
            r: srgb_to_linear(b.r),
            g: srgb_to_linear(b.g),
            b: srgb_to_linear(b.b),
            a: f64::from(b.a),
        };
        let Some(encoder) = frame.encoder.as_mut() else {
            return;
        };
        // The canvas was removed or replaced during the frame (the last effect removed, the
        // virtual resolution changed): show what was drawn into the frame's canvas, without
        // effects, rather than an empty frame.
        let canvas = self
            .canvas
            .as_ref()
            .filter(|c| frame.canvas.as_ref() == Some(&c.tex.view));
        let Some(canvas) = canvas else {
            if let Some(view) = &frame.canvas {
                let rect = if frame.canvas_virtual {
                    canvas::layout(window, frame.canvas_size, self.scale_mode)
                } else {
                    CanvasRect {
                        origin: [0.0, 0.0],
                        size: [window[0] as f32, window[1] as f32],
                    }
                };
                let groups = self.blit.bind_groups(&self.device, view);
                self.blit.draw(
                    encoder,
                    &groups[self.scale_filter.index()],
                    &frame.view,
                    rect,
                    border,
                );
            }
            return;
        };
        let run = StageRun {
            device: &self.device,
            queue: &self.queue,
            blit: &self.blit,
            format,
            globals: post::globals(
                canvas.size(),
                window,
                rect,
                self.start.elapsed().as_secs_f32(),
                self.frame_count,
            ),
        };

        // Indexed canvas: indices become colour before any effect sees the scene.
        #[cfg(feature = "indexed")]
        if let Some(ix) = self.indexed.as_mut() {
            ix.resolve(
                &run,
                encoder,
                &canvas.tex.view,
                canvas.size(),
                self.start.elapsed().as_secs_f64(),
            );
        }
        #[cfg(feature = "indexed")]
        let base = self
            .indexed
            .as_ref()
            .and_then(|ix| ix.resolved.as_ref())
            .unwrap_or(&canvas.tex);
        #[cfg(not(feature = "indexed"))]
        let base = &canvas.tex;

        let post = &mut self.post;
        let canvas_out = post.run(PostStage::Canvas, &run, encoder, &base.view, canvas.size());
        let scene = match canvas_out {
            Some(out) => post.output(PostStage::Canvas, out).unwrap_or(base),
            None => base,
        };

        if !post.active(PostStage::Window) {
            self.blit.draw(
                encoder,
                &scene.blit_bind_groups[self.scale_filter.index()],
                &frame.view,
                rect,
                border,
            );
            return;
        }

        // Window stage: it needs the upscaled frame, border included, as a texture. A
        // window-sized canvas already is that.
        let scene_view = scene.view.clone();
        let window_in = if canvas.follows_window {
            scene_view
        } else {
            if self.window_scratch.as_ref().map(|t| t.size) != Some(window) {
                self.window_scratch = Some(RenderTexture::new(
                    &self.device,
                    &self.blit,
                    format,
                    window,
                    "post.window.input",
                ));
            }
            let scratch = self.window_scratch.as_ref().expect("just created");
            self.blit.draw(
                encoder,
                &scene.blit_bind_groups[self.scale_filter.index()],
                &scratch.view,
                rect,
                border,
            );
            scratch.view.clone()
        };
        let full = CanvasRect {
            origin: [0.0, 0.0],
            size: [window[0] as f32, window[1] as f32],
        };
        if let Some(out) = post.run(PostStage::Window, &run, encoder, &window_in, window)
            && let Some(result) = post.output(PostStage::Window, out)
        {
            self.blit.draw(
                encoder,
                &result.blit_bind_groups[ScaleFilter::Nearest.index()],
                &frame.view,
                full,
                wgpu::Color::BLACK,
            );
        }
    }

    pub(crate) fn flush_into(&mut self, frame: &mut Frame) {
        if frame.flushed {
            return;
        }
        let Some(encoder) = frame.encoder.as_mut() else {
            return;
        };
        // Interleave sprites, filled shapes, circles and layered text by layer so a circle on
        // layer -1 draws under a sprite on layer 0; within a layer, sprites draw first, then
        // shapes, then circles, then text. Unlayered text (`Painter::text`) draws on top.
        let mut layers = std::collections::BTreeSet::new();
        self.sprites.collect_layers(&mut layers);
        self.shapes.collect_layers(&mut layers);
        self.primitives.collect_layers(&mut layers);
        self.text.collect_layers(&mut layers);
        self.custom.collect_layers(&mut layers);

        // Upload each batcher's instances exactly once before drawing: `queue.write_buffer`
        // is not part of the encoder command stream, so writing per layer pass would clobber
        // earlier layers' instance data.
        self.sprites.upload(&self.device, &self.queue);
        self.shapes.upload(&self.device, &self.queue);
        self.primitives.upload(&self.device, &self.queue);
        self.text.prepare(&self.device, &self.queue);

        // Draws go to the canvas when there is one, else to the surface. When multisampling,
        // they target the MSAA texture and resolve to that; otherwise they target it directly.
        let target = frame.canvas.as_ref().unwrap_or(&frame.view);
        let (color, resolve) = match self.msaa_view.as_ref() {
            Some(msaa) => (msaa, Some(target)),
            None => (target, None),
        };
        let targets = Targets {
            color,
            resolve,
            depth: self.depth_view.as_ref(),
        };

        // Clear color (and depth) once up front; every layer pass then loads onto it.
        let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear.pass"),
            color_attachments: &[Some(targets.color_attachment(wgpu::LoadOp::Clear(
                wgpu::Color {
                    // sRGB like every painter color; the clear value is linear.
                    r: srgb_to_linear(frame.clear_color[0]),
                    g: srgb_to_linear(frame.clear_color[1]),
                    b: srgb_to_linear(frame.clear_color[2]),
                    a: frame.clear_color[3] as f64,
                },
            )))],
            depth_stencil_attachment: targets.depth_attachment(wgpu::LoadOp::Clear(1.0)),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });

        // 3D meshes draw first, depth-tested, so the 2D layers (which do not write depth)
        // composite on top of them.
        self.mesh_batcher.draw(
            &self.device,
            &self.queue,
            &self.meshes,
            encoder,
            &targets,
            &self.camera3d_bg,
        );

        let size = frame.canvas.as_ref().map_or(
            [self.surface_config.width, self.surface_config.height],
            |_| frame.canvas_size,
        );
        for &layer in &layers {
            // Custom passes first: within a layer they draw under the painter's own content.
            for f in self.custom.take_layer(layer) {
                f(&mut CustomPass {
                    device: &self.device,
                    queue: &self.queue,
                    encoder,
                    targets,
                    format: self.surface_format(),
                    sample_count: self.sample_count,
                    depth_format: self.depth_format,
                    size,
                });
            }
            self.sprites
                .draw_layer(layer, encoder, &targets, &self.camera_bg, &self.textures);
            self.shapes
                .draw_layer(layer, encoder, &targets, &self.camera_bg, &self.textures);
            self.primitives
                .draw_layer(layer, encoder, &targets, &self.camera_bg);
            self.text.draw_layer(Some(layer), encoder, &targets);
        }

        self.text.draw_layer(None, encoder, &targets);
        self.text.finish();

        // Vector content composites on top of the 2D layers, directly onto the target.
        #[cfg(feature = "vector")]
        if let Some(vector) = self.vector.as_mut() {
            vector.render_and_composite(&self.device, &self.queue, encoder, target);
        }

        self.sprites.clear();
        self.shapes.clear();
        self.primitives.clear();
        self.mesh_batcher.clear();
        self.custom.clear();
        frame.flushed = true;
    }
}

/// Allocate the MSAA color target (when `sample_count > 1`) and depth target (when a depth
/// format is set), both `size` pixels. Returns `(msaa_view, depth_view)`.
fn make_attachments(
    device: &wgpu::Device,
    size: [u32; 2],
    color_format: wgpu::TextureFormat,
    sample_count: u32,
    depth_format: Option<wgpu::TextureFormat>,
) -> (Option<wgpu::TextureView>, Option<wgpu::TextureView>) {
    let size = wgpu::Extent3d {
        width: size[0].max(1),
        height: size[1].max(1),
        depth_or_array_layers: 1,
    };
    let msaa_view = (sample_count > 1).then(|| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("msaa.color"),
                size,
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: color_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    });
    let depth_view = depth_format.map(|format| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size,
                mip_level_count: 1,
                sample_count,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    });
    (msaa_view, depth_view)
}

/// The sRGB format frames are rendered in: the surface's own if it is sRGB, otherwise the
/// sRGB view format `Graphics::new` added.
fn render_format(config: &wgpu::SurfaceConfiguration) -> wgpu::TextureFormat {
    if config.format.is_srgb() {
        return config.format;
    }
    config
        .view_formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .unwrap_or(config.format)
}

/// Decode one sRGB color component to linear, as the shaders do for painter colors.
pub(crate) fn srgb_to_linear(c: f32) -> f64 {
    let c = f64::from(c);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
