use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::texture::{TextureId, TextureRegistry};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct QuadVertex {
    pos: [f32; 2],
    uv: [f32; 2],
}

const QUAD_VERTS: &[QuadVertex] = &[
    QuadVertex { pos: [0.0, 0.0], uv: [0.0, 0.0] },
    QuadVertex { pos: [1.0, 0.0], uv: [1.0, 0.0] },
    QuadVertex { pos: [1.0, 1.0], uv: [1.0, 1.0] },
    QuadVertex { pos: [0.0, 1.0], uv: [0.0, 1.0] },
];
const QUAD_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug)]
pub struct SpriteInstance {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    pub color: [f32; 4],
    pub rotation: f32,
    pub _pad: [f32; 3],
}

impl SpriteInstance {
    pub fn at(pos: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            pos,
            size,
            uv_min: [0.0, 0.0],
            uv_max: [1.0, 1.0],
            color: [1.0; 4],
            rotation: 0.0,
            _pad: [0.0; 3],
        }
    }
    pub fn with_color(mut self, c: [f32; 4]) -> Self {
        self.color = c;
        self
    }
    pub fn with_rotation(mut self, r: f32) -> Self {
        self.rotation = r;
        self
    }
    pub fn with_uv(mut self, min: [f32; 2], max: [f32; 2]) -> Self {
        self.uv_min = min;
        self.uv_max = max;
        self
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum BlendMode {
    Alpha,
    Additive,
    Premultiplied,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
struct BatchKey {
    texture: TextureId,
    layer: i16,
    blend: BlendMode,
}

pub(crate) struct SpriteBatcher {
    quad_vb: wgpu::Buffer,
    quad_ib: wgpu::Buffer,
    instance_vb: wgpu::Buffer,
    instance_capacity: usize,
    pipelines: HashMap<BlendMode, wgpu::RenderPipeline>,
    pending: Vec<(BatchKey, SpriteInstance)>,
}

impl SpriteBatcher {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bgl: &wgpu::BindGroupLayout,
        texture_bgl: &wgpu::BindGroupLayout,
    ) -> Self {
        let quad_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sprite.quad_vb"),
            contents: bytemuck::cast_slice(QUAD_VERTS),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let quad_ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sprite.quad_ib"),
            contents: bytemuck::cast_slice(QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_capacity = 4096usize;
        let instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite.instances"),
            size: (instance_capacity * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sprite.wgsl").into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sprite.layout"),
            bind_group_layouts: &[Some(camera_bgl), Some(texture_bgl)],
            immediate_size: 0,
        });

        let make_pipeline = |blend: wgpu::BlendState, label: &'static str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[
                        wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<QuadVertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
                        },
                        wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<SpriteInstance>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array![
                                2 => Float32x2,
                                3 => Float32x2,
                                4 => Float32x2,
                                5 => Float32x2,
                                6 => Float32x4,
                                7 => Float32,
                            ],
                        },
                    ],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };

        let mut pipelines = HashMap::new();
        pipelines.insert(
            BlendMode::Alpha,
            make_pipeline(wgpu::BlendState::ALPHA_BLENDING, "sprite.alpha"),
        );
        pipelines.insert(
            BlendMode::Premultiplied,
            make_pipeline(
                wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
                "sprite.premul",
            ),
        );
        pipelines.insert(
            BlendMode::Additive,
            make_pipeline(
                wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent::OVER,
                },
                "sprite.add",
            ),
        );

        Self {
            quad_vb,
            quad_ib,
            instance_vb,
            instance_capacity,
            pipelines,
            pending: Vec::new(),
        }
    }

    pub fn draw(&mut self, tex: TextureId, layer: i16, blend: BlendMode, inst: SpriteInstance) {
        self.pending.push((
            BatchKey {
                texture: tex,
                layer,
                blend,
            },
            inst,
        ));
    }

    // Passes the wgpu handles needed for one render pass; grouping them into a
    // struct would not make call sites clearer.
    #[allow(clippy::too_many_arguments)]
    pub fn flush(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        camera_bg: &wgpu::BindGroup,
        textures: &TextureRegistry,
        load_op: wgpu::LoadOp<wgpu::Color>,
    ) {
        if self.pending.is_empty() {
            // Still need to clear if requested.
            let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sprite.clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            return;
        }

        self.pending.sort_by(|a, b| {
            a.0.layer
                .cmp(&b.0.layer)
                .then((a.0.blend as u8).cmp(&(b.0.blend as u8)))
                .then(a.0.texture.0.cmp(&b.0.texture.0))
        });

        if self.pending.len() > self.instance_capacity {
            self.instance_capacity = self.pending.len().next_power_of_two();
            self.instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sprite.instances"),
                size: (self.instance_capacity * std::mem::size_of::<SpriteInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        let flat: Vec<SpriteInstance> = self.pending.iter().map(|(_, i)| *i).collect();
        queue.write_buffer(&self.instance_vb, 0, bytemuck::cast_slice(&flat));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sprite.pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: load_op,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });

        pass.set_bind_group(0, camera_bg, &[]);
        pass.set_vertex_buffer(0, self.quad_vb.slice(..));
        pass.set_index_buffer(self.quad_ib.slice(..), wgpu::IndexFormat::Uint16);
        pass.set_vertex_buffer(1, self.instance_vb.slice(..));

        let mut i = 0;
        while i < self.pending.len() {
            let key = self.pending[i].0;
            let start = i;
            while i < self.pending.len()
                && self.pending[i].0.blend == key.blend
                && self.pending[i].0.texture == key.texture
            {
                i += 1;
            }
            let count = (i - start) as u32;
            pass.set_pipeline(&self.pipelines[&key.blend]);
            pass.set_bind_group(1, textures.bind_group(key.texture), &[]);
            pass.draw_indexed(0..6, 0, (start as u32)..(start as u32 + count));
        }

        drop(pass);
        self.pending.clear();
    }
}
