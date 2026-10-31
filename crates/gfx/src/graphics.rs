use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::camera::{Camera2D, CameraUniform};
use crate::camera3d::{Camera, Camera3D};
use crate::frame::Frame;
use crate::mesh::{MeshBatcher, MeshId, MeshInstance, MeshRegistry, MeshVertex};
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
}

impl Graphics {
    pub async fn new(
        window: Arc<Window>,
        vsync: bool,
        depth_format: Option<wgpu::TextureFormat>,
        msaa_samples: u32,
    ) -> Result<Self> {
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

        let backend = adapter.get_info().backend;
        log::info!("wgpu adapter: {:?} ({backend:?})", adapter.get_info().name);
        // WebGL2 can't meet even the downlevel limits (and has no compute, so no vello).
        // Otherwise vello needs the standard (non-downlevel) limits; bump them when it's on.
        let required_limits = if backend == wgpu::Backend::Gl {
            wgpu::Limits::downlevel_webgl2_defaults()
        } else if cfg!(feature = "vector") {
            wgpu::Limits::default()
        } else {
            wgpu::Limits::downlevel_defaults()
        }
        .using_resolution(adapter.limits());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("toolkit.device"),
                required_features: wgpu::Features::empty(),
                required_limits,
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            })
            .await
            .context("request_device")?;

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
            make_attachments(&device, &surface_config, sample_count, depth_format);
        let meshes = MeshRegistry::new();
        let mesh_batcher =
            MeshBatcher::new(&device, format, &camera_bgl, sample_count, depth_format);
        #[cfg(feature = "vector")]
        let vector = if adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::COMPUTE_SHADERS)
        {
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
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        let (msaa_view, depth_view) = make_attachments(
            &self.device,
            &self.surface_config,
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

    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    pub fn size(&self) -> (u32, u32) {
        (self.surface_config.width, self.surface_config.height)
    }

    /// The color format the frame is rendered in: always an sRGB format, possibly a view of a
    /// non-sRGB surface. Build render pipelines that draw into a [`Frame`] with this.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        render_format(&self.surface_config)
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
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
            log::warn!("update_texture_rgba: unknown {id:?} or wrong data size");
        }
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
        Ok(Frame {
            encoder: Some(encoder),
            view,
            surface_texture: Some(surface_texture),
            clear_color: self.clear_color,
            flushed: false,
        })
    }

    pub fn present(&mut self, mut frame: Frame) {
        if !frame.flushed {
            self.flush_into(&mut frame);
        }

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

    /// Flush all queued 2D layers (sprites + shapes + primitives + text) into the current `frame`.
    /// Called automatically when a [`crate::Painter`] is dropped; call manually before
    /// stacking other passes (e.g. egui) when no painter was created.
    pub fn flush_pending(&mut self, frame: &mut Frame) {
        self.flush_into(frame);
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

        // Upload each batcher's instances exactly once before drawing: `queue.write_buffer`
        // is not part of the encoder command stream, so writing per layer pass would clobber
        // earlier layers' instance data.
        self.sprites.upload(&self.device, &self.queue);
        self.shapes.upload(&self.device, &self.queue);
        self.primitives.upload(&self.device, &self.queue);
        self.text.prepare(&self.device, &self.queue);

        // When multisampling, draws target the MSAA texture and resolve to the surface;
        // otherwise they target the surface directly.
        let (color, resolve) = match self.msaa_view.as_ref() {
            Some(msaa) => (msaa, Some(&frame.view)),
            None => (&frame.view, None),
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

        for &layer in &layers {
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

        // Vector content composites on top of the 2D layers, directly onto the surface.
        #[cfg(feature = "vector")]
        if let Some(vector) = self.vector.as_mut() {
            vector.render_and_composite(&self.device, &self.queue, encoder, &frame.view);
        }

        self.sprites.clear();
        self.shapes.clear();
        self.primitives.clear();
        self.mesh_batcher.clear();
        frame.flushed = true;
    }
}

/// Allocate the MSAA color target (when `sample_count > 1`) and depth target (when a depth
/// format is set), both sized to the surface. Returns `(msaa_view, depth_view)`.
fn make_attachments(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    sample_count: u32,
    depth_format: Option<wgpu::TextureFormat>,
) -> (Option<wgpu::TextureView>, Option<wgpu::TextureView>) {
    let size = wgpu::Extent3d {
        width: config.width,
        height: config.height,
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
                format: render_format(config),
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
fn srgb_to_linear(c: f32) -> f64 {
    let c = f64::from(c);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
