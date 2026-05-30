use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::camera::{Camera2D, CameraUniform};
use crate::frame::Frame;
use crate::primitives::PrimitiveBatcher;
use crate::sprite::SpriteBatcher;
use crate::target::Targets;
use crate::text::TextSystem;
use crate::texture::{TextureId, TextureRegistry};

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
        idesc.backends = wgpu::Backends::PRIMARY;
        let instance = wgpu::Instance::new(idesc);
        let surface = instance
            .create_surface(window.clone())
            .context("create_surface")?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("no compatible adapter")?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("toolkit.device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults()
                    .using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            })
            .await
            .context("request_device")?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let present_mode = if vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
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
            text,
            textures,
            clear_color: [0.0, 0.0, 0.0, 1.0],
            texture_paths: HashMap::new(),
            sample_count,
            depth_format,
            msaa_view,
            depth_view,
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
        self.text.resize(&self.queue, width, height);
    }

    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }

    pub fn size(&self) -> (u32, u32) {
        (self.surface_config.width, self.surface_config.height)
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_config.format
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn load_texture(&mut self, path: impl AsRef<Path>) -> Result<TextureId> {
        let path = path.as_ref();
        let id = self.textures.load_file(&self.device, &self.queue, path)?;
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.texture_paths.insert(canon, id);
        Ok(id)
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
            let canon = p.as_ref().canonicalize().unwrap_or_else(|_| p.as_ref().to_path_buf());
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

    pub fn white_texture(&self) -> TextureId {
        self.textures.white()
    }

    pub fn begin_frame(&mut self) -> Result<Frame> {
        self.queue.write_buffer(
            &self.camera_buf,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj: self.camera.view_proj(),
            }),
        );

        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => anyhow::bail!("surface unavailable: {other:?}"),
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
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
        if let Some(st) = frame.surface_texture.take() {
            self.window.pre_present_notify();
            st.present();
        }
    }

    /// Flush all queued 2D layers (sprites + primitives + text) into the current `frame`.
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
        // Interleave sprites and circles by layer so a circle on layer -1 draws under a
        // sprite on layer 0; within a layer, sprites draw under circles. Text renders once
        // on top (glyphon prepares a single vertex buffer per call, so it is not layered).
        let mut layers = std::collections::BTreeSet::new();
        self.sprites.collect_layers(&mut layers);
        self.primitives.collect_layers(&mut layers);

        // Upload each batcher's instances exactly once before drawing: `queue.write_buffer`
        // is not part of the encoder command stream, so writing per layer pass would clobber
        // earlier layers' instance data.
        self.sprites.upload(&self.device, &self.queue);
        self.primitives.upload(&self.device, &self.queue);

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
                    r: frame.clear_color[0] as f64,
                    g: frame.clear_color[1] as f64,
                    b: frame.clear_color[2] as f64,
                    a: frame.clear_color[3] as f64,
                },
            )))],
            depth_stencil_attachment: targets.depth_attachment(wgpu::LoadOp::Clear(1.0)),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });

        for &layer in &layers {
            self.sprites
                .draw_layer(layer, encoder, &targets, &self.camera_bg, &self.textures);
            self.primitives.draw_layer(layer, encoder, &targets, &self.camera_bg);
        }

        self.text
            .flush(&self.device, &self.queue, encoder, &targets);

        self.sprites.clear();
        self.primitives.clear();
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
                format: config.format,
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
