//! Vello vector-graphics backend (feature `vector`).
//!
//! Vello is a compute-based 2D renderer. It renders a [`vello::Scene`] into an offscreen
//! `Rgba8Unorm` storage texture, which is then composited onto the surface with a fullscreen
//! blit after the sprite/primitive/text layers. The caller draws via
//! [`crate::Painter::vector`].

use std::num::NonZeroUsize;

use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

/// Vello requires its target to be `Rgba8Unorm` with `STORAGE_BINDING`.
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub(crate) struct VectorPass {
    renderer: Renderer,
    scene: Scene,
    target_view: wgpu::TextureView,
    /// Set when the caller drew anything this frame; skips work when no vector content.
    has_content: bool,
    composite_pipeline: wgpu::RenderPipeline,
    composite_bgl: wgpu::BindGroupLayout,
    composite_bg: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    width: u32,
    height: u32,
}

impl VectorPass {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let renderer = Renderer::new(
            device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: AaSupport::area_only(),
                num_init_threads: NonZeroUsize::new(1),
                pipeline_cache: None,
            },
        )
        .expect("create vello renderer");

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vector.composite.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("vector_composite.wgsl").into()),
        });
        let composite_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("vector.composite.bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vector.composite.layout"),
            bind_group_layouts: &[Some(&composite_bgl)],
            immediate_size: 0,
        });
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("vector.composite.pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("vector.composite.sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let target_view = make_target(device, width, height);
        let composite_bg = make_bind_group(device, &composite_bgl, &target_view, &sampler);

        Self {
            renderer,
            scene: Scene::new(),
            target_view,
            has_content: false,
            composite_pipeline,
            composite_bgl,
            composite_bg,
            sampler,
            width,
            height,
        }
    }

    /// The scene the caller draws into this frame.
    pub fn scene(&mut self) -> &mut Scene {
        self.has_content = true;
        &mut self.scene
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.target_view = make_target(device, width, height);
        self.composite_bg =
            make_bind_group(device, &self.composite_bgl, &self.target_view, &self.sampler);
    }

    /// Render the queued scene to the offscreen target (a separate vello submit) and composite
    /// it onto `surface_view`. No-op when nothing was drawn this frame.
    pub fn render_and_composite(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        surface_view: &wgpu::TextureView,
    ) {
        if !self.has_content {
            return;
        }
        let params = RenderParams {
            base_color: vello::peniko::Color::TRANSPARENT,
            width: self.width,
            height: self.height,
            antialiasing_method: AaConfig::Area,
        };
        match self
            .renderer
            .render_to_texture(device, queue, &self.scene, &self.target_view, &params)
        {
            Ok(()) => {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("vector.composite.pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: surface_view,
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
                });
                pass.set_pipeline(&self.composite_pipeline);
                pass.set_bind_group(0, &self.composite_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            Err(e) => log::warn!("vello render failed: {e:?}"),
        }
        self.scene.reset();
        self.has_content = false;
    }
}

fn make_target(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("vector.target"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn make_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("vector.composite.bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}
