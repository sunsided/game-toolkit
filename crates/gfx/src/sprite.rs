use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::target::Targets;
use crate::texture::{TextureId, TextureRegistry};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct QuadVertex {
    pos: [f32; 2],
    uv: [f32; 2],
}

const QUAD_VERTS: &[QuadVertex] = &[
    QuadVertex {
        pos: [0.0, 0.0],
        uv: [0.0, 0.0],
    },
    QuadVertex {
        pos: [1.0, 0.0],
        uv: [1.0, 0.0],
    },
    QuadVertex {
        pos: [1.0, 1.0],
        uv: [1.0, 1.0],
    },
    QuadVertex {
        pos: [0.0, 1.0],
        uv: [0.0, 1.0],
    },
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
    /// Sample the `size` pixel rectangle at `min` (top-left, in pixels) of a texture that is
    /// `tex_size` pixels large, e.g. one sprite of an atlas. Get `tex_size` from
    /// `Graphics::texture_size` / `Painter::texture_size`, which never reports a zero side;
    /// a zero `tex_size` side would produce non-finite UVs and is a debug-build panic.
    pub fn with_region(self, min: [f32; 2], size: [f32; 2], tex_size: [u32; 2]) -> Self {
        debug_assert!(
            tex_size[0] > 0 && tex_size[1] > 0,
            "with_region: zero-sized texture {tex_size:?}"
        );
        let [tw, th] = [tex_size[0] as f32, tex_size[1] as f32];
        self.with_uv(
            [min[0] / tw, min[1] / th],
            [(min[0] + size[0]) / tw, (min[1] + size[1]) / th],
        )
    }
    /// Mirror horizontally by swapping the u bounds. Works with any UV range or region;
    /// apply it after [`with_uv`](Self::with_uv) / [`with_region`](Self::with_region).
    pub fn flip_x(mut self) -> Self {
        std::mem::swap(&mut self.uv_min[0], &mut self.uv_max[0]);
        self
    }
    /// Mirror vertically by swapping the v bounds. Works with any UV range or region;
    /// apply it after [`with_uv`](Self::with_uv) / [`with_region`](Self::with_region).
    pub fn flip_y(mut self) -> Self {
        std::mem::swap(&mut self.uv_min[1], &mut self.uv_max[1]);
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
        sample_count: u32,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let multisample = crate::target::multisample(sample_count);
        let depth_stencil = depth_format.map(crate::target::no_write_depth);
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
            size: (instance_capacity * size_of::<SpriteInstance>()) as u64,
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
                        Some(wgpu::VertexBufferLayout {
                            array_stride: size_of::<QuadVertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
                        }),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: size_of::<SpriteInstance>() as u64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array![
                                2 => Float32x2,
                                3 => Float32x2,
                                4 => Float32x2,
                                5 => Float32x2,
                                6 => Float32x4,
                                7 => Float32,
                            ],
                        }),
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
                depth_stencil: depth_stencil.clone(),
                multisample,
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

    /// Record every layer that has pending sprites, for cross-batcher interleaving.
    pub fn collect_layers(&self, out: &mut std::collections::BTreeSet<i16>) {
        out.extend(self.pending.iter().map(|(k, _)| k.layer));
    }

    /// Sort all pending sprites by layer (then blend, then texture) and upload them to the
    /// instance buffer in one write. Must run before any [`SpriteBatcher::draw_layer`]: the
    /// buffer is written once per frame because `queue.write_buffer` does not interleave with
    /// encoder passes, so a per-pass write would clobber the earlier layers' data.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.pending.is_empty() {
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
                size: (self.instance_capacity * size_of::<SpriteInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        let flat: Vec<SpriteInstance> = self.pending.iter().map(|(_, i)| *i).collect();
        queue.write_buffer(&self.instance_vb, 0, bytemuck::cast_slice(&flat));
    }

    /// Draw the sprites on `layer` (already uploaded by [`SpriteBatcher::upload`]) into the
    /// already-cleared target, grouped by blend then texture to minimize state changes.
    pub fn draw_layer(
        &self,
        layer: i16,
        encoder: &mut wgpu::CommandEncoder,
        targets: &Targets<'_>,
        camera_bg: &wgpu::BindGroup,
        textures: &TextureRegistry,
    ) {
        // `pending` is sorted by layer, so this layer's sprites are a contiguous range.
        let lo = self.pending.partition_point(|(k, _)| k.layer < layer);
        let hi = self.pending.partition_point(|(k, _)| k.layer <= layer);
        if lo == hi {
            return;
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("sprite.pass"),
            color_attachments: &[Some(targets.color_attachment(wgpu::LoadOp::Load))],
            depth_stencil_attachment: targets.depth_attachment(wgpu::LoadOp::Load),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });

        pass.set_bind_group(0, camera_bg, &[]);
        pass.set_vertex_buffer(0, self.quad_vb.slice(..));
        pass.set_index_buffer(self.quad_ib.slice(..), wgpu::IndexFormat::Uint16);
        pass.set_vertex_buffer(1, self.instance_vb.slice(..));

        let mut i = lo;
        while i < hi {
            let key = self.pending[i].0;
            let start = i;
            while i < hi
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
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_region_maps_pixels_to_uvs() {
        let inst = SpriteInstance::at([0.0, 0.0], [32.0, 16.0]).with_region(
            [64.0, 32.0],
            [32.0, 16.0],
            [256, 128],
        );
        assert_eq!(inst.uv_min, [0.25, 0.25]);
        assert_eq!(inst.uv_max, [0.375, 0.375]);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "zero-sized texture")]
    fn with_region_rejects_a_zero_sized_texture_in_debug_builds() {
        let _ =
            SpriteInstance::at([0.0, 0.0], [1.0, 1.0]).with_region([0.0, 0.0], [1.0, 1.0], [0, 64]);
    }

    #[test]
    fn flip_x_swaps_u_bounds_only() {
        let inst = SpriteInstance::at([0.0, 0.0], [1.0, 1.0])
            .with_uv([0.1, 0.2], [0.3, 0.4])
            .flip_x();
        assert_eq!(inst.uv_min, [0.3, 0.2]);
        assert_eq!(inst.uv_max, [0.1, 0.4]);
    }

    #[test]
    fn flip_y_swaps_v_bounds_only() {
        let inst = SpriteInstance::at([0.0, 0.0], [1.0, 1.0])
            .with_uv([0.1, 0.2], [0.3, 0.4])
            .flip_y();
        assert_eq!(inst.uv_min, [0.1, 0.4]);
        assert_eq!(inst.uv_max, [0.3, 0.2]);
    }

    #[test]
    fn flipping_twice_restores_the_region() {
        let inst = SpriteInstance::at([0.0, 0.0], [1.0, 1.0]).with_region(
            [8.0, 8.0],
            [16.0, 16.0],
            [64, 64],
        );
        let back = inst.flip_x().flip_y().flip_x().flip_y();
        assert_eq!(back.uv_min, inst.uv_min);
        assert_eq!(back.uv_max, inst.uv_max);
    }
}
