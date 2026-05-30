//! Instanced static-mesh rendering: a registry of uploaded meshes plus a depth-tested
//! pipeline. Meshes draw before the 2D layers so the 2D painter (HUD, text) sits on top.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::target::Targets;
use crate::transform::Mat4;

/// Handle to a mesh uploaded via [`crate::Graphics::create_mesh`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeshId(pub u32);

/// One mesh vertex: position and normal, both in model space.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug)]
pub struct MeshVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
}

impl MeshVertex {
    pub fn new(pos: [f32; 3], normal: [f32; 3]) -> Self {
        Self { pos, normal }
    }
}

/// One instance of a mesh: a column-major model matrix and a tint color.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug)]
pub struct MeshInstance {
    pub model: Mat4,
    pub color: [f32; 4],
}

impl MeshInstance {
    pub fn new(model: Mat4, color: [f32; 4]) -> Self {
        Self { model, color }
    }
}

struct MeshGpu {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
}

/// Owns the GPU buffers for every uploaded mesh.
pub(crate) struct MeshRegistry {
    map: HashMap<MeshId, MeshGpu>,
    next: u32,
}

impl MeshRegistry {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            next: 1,
        }
    }

    pub fn create(
        &mut self,
        device: &wgpu::Device,
        vertices: &[MeshVertex],
        indices: &[u16],
    ) -> MeshId {
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh.vertices"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh.indices"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let id = MeshId(self.next);
        self.next += 1;
        self.map.insert(
            id,
            MeshGpu {
                vertex_buf,
                index_buf,
                index_count: indices.len() as u32,
            },
        );
        id
    }
}

pub(crate) struct MeshBatcher {
    pipeline: wgpu::RenderPipeline,
    instance_vb: wgpu::Buffer,
    capacity: usize,
    pending: Vec<(MeshId, MeshInstance)>,
}

impl MeshBatcher {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bgl: &wgpu::BindGroupLayout,
        sample_count: u32,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let capacity = 256usize;
        let instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mesh.instances"),
            size: (capacity * std::mem::size_of::<MeshInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mesh.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("mesh.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh.layout"),
            bind_group_layouts: &[Some(camera_bgl)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh.pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<MeshVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<MeshInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            2 => Float32x4,
                            3 => Float32x4,
                            4 => Float32x4,
                            5 => Float32x4,
                            6 => Float32x4,
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
            // No backface culling: an opaque mesh relies on the depth test to hide its back
            // faces, which keeps callers from having to match a specific winding order.
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: depth_format.map(crate::target::depth_test),
            multisample: crate::target::multisample(sample_count),
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            instance_vb,
            capacity,
            pending: Vec::new(),
        }
    }

    pub fn push(&mut self, id: MeshId, instance: MeshInstance) {
        self.pending.push((id, instance));
    }

    pub fn draw(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        registry: &MeshRegistry,
        encoder: &mut wgpu::CommandEncoder,
        targets: &Targets,
        camera_bg: &wgpu::BindGroup,
    ) {
        if self.pending.is_empty() {
            return;
        }
        // Group instances by mesh so each mesh's vertex/index buffers bind once.
        self.pending.sort_by_key(|(id, _)| *id);
        if self.pending.len() > self.capacity {
            self.capacity = self.pending.len().next_power_of_two();
            self.instance_vb = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("mesh.instances"),
                size: (self.capacity * std::mem::size_of::<MeshInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        let flat: Vec<MeshInstance> = self.pending.iter().map(|(_, i)| *i).collect();
        queue.write_buffer(&self.instance_vb, 0, bytemuck::cast_slice(&flat));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mesh.pass"),
            color_attachments: &[Some(targets.color_attachment(wgpu::LoadOp::Load))],
            depth_stencil_attachment: targets.depth_attachment(wgpu::LoadOp::Load),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera_bg, &[]);
        pass.set_vertex_buffer(1, self.instance_vb.slice(..));

        let mut i = 0;
        while i < self.pending.len() {
            let id = self.pending[i].0;
            let start = i;
            while i < self.pending.len() && self.pending[i].0 == id {
                i += 1;
            }
            let Some(mesh) = registry.map.get(&id) else {
                continue;
            };
            pass.set_vertex_buffer(0, mesh.vertex_buf.slice(..));
            pass.set_index_buffer(mesh.index_buf.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..mesh.index_count, 0, (start as u32)..(i as u32));
        }
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}
