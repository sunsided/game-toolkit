use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::target::Targets;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct PrimVertex {
    pos: [f32; 2],
}

const QUAD_VERTS: &[PrimVertex] = &[
    PrimVertex { pos: [0.0, 0.0] },
    PrimVertex { pos: [1.0, 0.0] },
    PrimVertex { pos: [1.0, 1.0] },
    PrimVertex { pos: [0.0, 1.0] },
];
const QUAD_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug)]
pub struct CircleInstance {
    pub center: [f32; 2],
    pub radius: f32,
    /// `0.0` = filled disk; `>0` = ring of that pixel thickness (outer = radius).
    pub thickness: f32,
    pub color: [f32; 4],
}

pub(crate) struct PrimitiveBatcher {
    quad_vb: wgpu::Buffer,
    quad_ib: wgpu::Buffer,
    instance_vb: wgpu::Buffer,
    capacity: usize,
    pipeline: wgpu::RenderPipeline,
    pending: Vec<(i16, CircleInstance)>,
}

impl PrimitiveBatcher {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bgl: &wgpu::BindGroupLayout,
        sample_count: u32,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let quad_vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("prim.quad_vb"),
            contents: bytemuck::cast_slice(QUAD_VERTS),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let quad_ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("prim.quad_ib"),
            contents: bytemuck::cast_slice(QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
        let capacity = 1024usize;
        let instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("prim.circle_instances"),
            size: (capacity * std::mem::size_of::<CircleInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("prim.circle.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("circle.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("prim.layout"),
            bind_group_layouts: &[Some(camera_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("prim.circle.pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<PrimVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x2],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<CircleInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            2 => Float32x2,
                            3 => Float32,
                            4 => Float32,
                            5 => Float32x4,
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: depth_format.map(crate::target::no_write_depth),
            multisample: crate::target::multisample(sample_count),
            multiview_mask: None,
            cache: None,
        });

        Self {
            quad_vb,
            quad_ib,
            instance_vb,
            capacity,
            pipeline,
            pending: Vec::new(),
        }
    }

    pub fn push(&mut self, layer: i16, inst: CircleInstance) {
        self.pending.push((layer, inst));
    }

    /// Record every layer that has pending circles, for cross-batcher interleaving.
    pub fn collect_layers(&self, out: &mut std::collections::BTreeSet<i16>) {
        out.extend(self.pending.iter().map(|(layer, _)| *layer));
    }

    /// Sort all pending circles by layer and upload them in one write. Must run before any
    /// [`PrimitiveBatcher::draw_layer`]; see [`SpriteBatcher::upload`] for why the buffer is
    /// written exactly once per frame rather than per layer pass.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.pending.is_empty() {
            return;
        }
        self.pending.sort_by_key(|(layer, _)| *layer);
        if self.pending.len() > self.capacity {
            self.capacity = self.pending.len().next_power_of_two();
            self.instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("prim.circle_instances"),
                size: (self.capacity * std::mem::size_of::<CircleInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        let flat: Vec<CircleInstance> = self.pending.iter().map(|(_, c)| *c).collect();
        queue.write_buffer(&self.instance_vb, 0, bytemuck::cast_slice(&flat));
    }

    /// Draw the circles on `layer` (already uploaded by [`PrimitiveBatcher::upload`]) into the
    /// already-cleared target.
    pub fn draw_layer(
        &self,
        layer: i16,
        encoder: &mut wgpu::CommandEncoder,
        targets: &Targets,
        camera_bg: &wgpu::BindGroup,
    ) {
        // `pending` is sorted by layer, so this layer's circles are a contiguous range.
        let lo = self.pending.partition_point(|(l, _)| *l < layer);
        let hi = self.pending.partition_point(|(l, _)| *l <= layer);
        if lo == hi {
            return;
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("prim.circle.pass"),
            color_attachments: &[Some(targets.color_attachment(wgpu::LoadOp::Load))],
            depth_stencil_attachment: targets.depth_attachment(wgpu::LoadOp::Load),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera_bg, &[]);
        pass.set_vertex_buffer(0, self.quad_vb.slice(..));
        pass.set_index_buffer(self.quad_ib.slice(..), wgpu::IndexFormat::Uint16);
        pass.set_vertex_buffer(1, self.instance_vb.slice(..));
        pass.draw_indexed(0..6, 0, lo as u32..hi as u32);
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}
