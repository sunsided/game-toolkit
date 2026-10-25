use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use bytemuck::{Pod, Zeroable};

use crate::target::Targets;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug, PartialEq)]
struct ShapeVertex {
    pos: [f32; 2],
    color: [f32; 4],
}

/// Vertex colours for a shape: one for the whole shape, or one per vertex (interpolated
/// across each triangle).
#[derive(Copy, Clone)]
pub(crate) enum Colors<'a> {
    Uniform([f32; 4]),
    PerVertex(&'a [[f32; 4]]),
}

impl Colors<'_> {
    /// Whether these colours fit a shape with `vertex_count` vertices.
    fn fits(&self, vertex_count: usize) -> bool {
        match self {
            Colors::Uniform(_) => true,
            Colors::PerVertex(colors) => colors.len() == vertex_count,
        }
    }

    fn at(&self, i: usize) -> [f32; 4] {
        match self {
            Colors::Uniform(c) => *c,
            Colors::PerVertex(colors) => colors[i],
        }
    }
}

/// Logs a skipped malformed shape. Debug builds panic; release builds warn once per process,
/// since shape calls typically run every frame and would flood the log.
fn reject(msg: std::fmt::Arguments<'_>) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    debug_assert!(false, "{msg}");
    if !WARNED.swap(true, Ordering::Relaxed) {
        log::warn!("{msg}; skipping (further malformed shapes are not logged)");
    }
}

/// Each drawn layer with its range into the combined index buffer, in layer order.
type LayerRanges = Vec<(i16, Range<u32>)>;

/// One layer's triangles: vertices plus indices local to this layer's vertex list.
#[derive(Default)]
struct LayerGeometry {
    vertices: Vec<ShapeVertex>,
    indices: Vec<u32>,
}

/// CPU-side geometry for filled shapes, bucketed by layer. Kept free of GPU state so the
/// triangulation and layer bookkeeping can be unit-tested.
#[derive(Default)]
struct ShapeBuffer {
    layers: BTreeMap<i16, LayerGeometry>,
}

impl ShapeBuffer {
    /// Queue an indexed triangle list. Draws nothing (see [`reject`]) when an index is out of
    /// range, the index count is not a multiple of three, or there is not one colour per
    /// vertex.
    fn triangles(&mut self, layer: i16, points: &[[f32; 2]], colors: Colors<'_>, indices: &[u32]) {
        if !indices.len().is_multiple_of(3) || indices.iter().any(|&i| i as usize >= points.len()) {
            return reject(format_args!(
                "triangles: {} indices into {} vertices; need whole triangles, all in range",
                indices.len(),
                points.len()
            ));
        }
        if !colors.fits(points.len()) {
            return reject(format_args!("shape: need one color per vertex"));
        }
        if indices.is_empty() {
            return;
        }
        let base = self.push_vertices(layer, points, colors);
        let geo = self.layers.entry(layer).or_default();
        geo.indices.extend(indices.iter().map(|&i| base + i));
    }

    /// Queue a convex polygon, fan-triangulated from its first point. Fewer than three
    /// points draw nothing; so does a colour count that does not match (see [`reject`]).
    fn polygon(&mut self, layer: i16, points: &[[f32; 2]], colors: Colors<'_>) {
        if !colors.fits(points.len()) {
            return reject(format_args!("shape: need one color per vertex"));
        }
        if points.len() < 3 {
            return;
        }
        let base = self.push_vertices(layer, points, colors);
        let geo = self.layers.entry(layer).or_default();
        for i in 1..points.len() as u32 - 1 {
            geo.indices.extend([base, base + i, base + i + 1]);
        }
    }

    /// Append `points` with their colours to `layer` and return the index of the first one.
    fn push_vertices(&mut self, layer: i16, points: &[[f32; 2]], colors: Colors<'_>) -> u32 {
        let geo = self.layers.entry(layer).or_default();
        let base = geo.vertices.len() as u32;
        geo.vertices
            .extend(points.iter().enumerate().map(|(i, &pos)| ShapeVertex {
                pos,
                color: colors.at(i),
            }));
        base
    }

    fn collect_layers(&self, out: &mut std::collections::BTreeSet<i16>) {
        out.extend(self.layers.keys().copied());
    }

    /// Concatenate every layer (in layer order) into one vertex and one index list, with
    /// indices rebased onto the combined vertex list. Returns each layer's index range.
    fn flatten(&self, vertices: &mut Vec<ShapeVertex>, indices: &mut Vec<u32>) -> LayerRanges {
        vertices.clear();
        indices.clear();
        let mut ranges = Vec::with_capacity(self.layers.len());
        for (&layer, geo) in &self.layers {
            let base = vertices.len() as u32;
            let start = indices.len() as u32;
            vertices.extend_from_slice(&geo.vertices);
            indices.extend(geo.indices.iter().map(|&i| base + i));
            ranges.push((layer, start..indices.len() as u32));
        }
        ranges
    }

    fn clear(&mut self) {
        self.layers.clear();
    }
}

/// Draws filled triangles (quads, convex polygons, raw triangle lists) interleaved with
/// sprites and circles by layer. Untextured, alpha-blended; one draw call per layer, so
/// call order is kept within a layer.
pub(crate) struct ShapeBatcher {
    shapes: ShapeBuffer,
    vertices: Vec<ShapeVertex>,
    indices: Vec<u32>,
    ranges: LayerRanges,
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    vertex_capacity: usize,
    index_capacity: usize,
    pipeline: wgpu::RenderPipeline,
}

fn vertex_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shape.vertices"),
        size: (capacity * size_of::<ShapeVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn index_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shape.indices"),
        size: (capacity * size_of::<u32>()) as u64,
        usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl ShapeBatcher {
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        camera_bgl: &wgpu::BindGroupLayout,
        sample_count: u32,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let vertex_capacity = 4096usize;
        let index_capacity = 8192usize;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shape.shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shape.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shape.layout"),
            bind_group_layouts: &[Some(camera_bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shape.pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<ShapeVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                })],
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
            // No culling: callers may wind their points either way.
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: depth_format.map(crate::target::no_write_depth),
            multisample: crate::target::multisample(sample_count),
            multiview_mask: None,
            cache: None,
        });

        Self {
            shapes: ShapeBuffer::default(),
            vertices: Vec::new(),
            indices: Vec::new(),
            ranges: Vec::new(),
            vb: vertex_buffer(device, vertex_capacity),
            ib: index_buffer(device, index_capacity),
            vertex_capacity,
            index_capacity,
            pipeline,
        }
    }

    pub fn triangles(
        &mut self,
        layer: i16,
        points: &[[f32; 2]],
        colors: Colors<'_>,
        indices: &[u32],
    ) {
        self.shapes.triangles(layer, points, colors, indices);
    }

    pub fn polygon(&mut self, layer: i16, points: &[[f32; 2]], colors: Colors<'_>) {
        self.shapes.polygon(layer, points, colors);
    }

    /// Record every layer that has pending shapes, for cross-batcher interleaving.
    pub fn collect_layers(&self, out: &mut std::collections::BTreeSet<i16>) {
        self.shapes.collect_layers(out);
    }

    /// Upload all pending shapes in one write per buffer. Must run before any
    /// [`ShapeBatcher::draw_layer`]; see `SpriteBatcher::upload` for why.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.ranges = self.shapes.flatten(&mut self.vertices, &mut self.indices);
        if self.indices.is_empty() {
            return;
        }
        if self.vertices.len() > self.vertex_capacity {
            self.vertex_capacity = self.vertices.len().next_power_of_two();
            self.vb = vertex_buffer(device, self.vertex_capacity);
        }
        if self.indices.len() > self.index_capacity {
            self.index_capacity = self.indices.len().next_power_of_two();
            self.ib = index_buffer(device, self.index_capacity);
        }
        queue.write_buffer(&self.vb, 0, bytemuck::cast_slice(&self.vertices));
        queue.write_buffer(&self.ib, 0, bytemuck::cast_slice(&self.indices));
    }

    /// Draw the shapes on `layer` (already uploaded by [`ShapeBatcher::upload`]) into the
    /// already-cleared target.
    pub fn draw_layer(
        &self,
        layer: i16,
        encoder: &mut wgpu::CommandEncoder,
        targets: &Targets<'_>,
        camera_bg: &wgpu::BindGroup,
    ) {
        let Some((_, range)) = self.ranges.iter().find(|(l, _)| *l == layer) else {
            return;
        };
        if range.is_empty() {
            return;
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shape.pass"),
            color_attachments: &[Some(targets.color_attachment(wgpu::LoadOp::Load))],
            depth_stencil_attachment: targets.depth_attachment(wgpu::LoadOp::Load),
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera_bg, &[]);
        pass.set_vertex_buffer(0, self.vb.slice(..));
        pass.set_index_buffer(self.ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(range.clone(), 0, 0..1);
    }

    pub fn clear(&mut self) {
        self.shapes.clear();
        self.ranges.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

    fn flatten(buf: &ShapeBuffer) -> (Vec<ShapeVertex>, Vec<u32>, LayerRanges) {
        let (mut v, mut i) = (Vec::new(), Vec::new());
        let ranges = buf.flatten(&mut v, &mut i);
        (v, i, ranges)
    }

    #[test]
    fn polygon_is_fan_triangulated() {
        let mut buf = ShapeBuffer::default();
        let pentagon = [[0.0, 0.0], [2.0, 0.0], [3.0, 1.0], [1.0, 2.0], [-1.0, 1.0]];
        buf.polygon(0, &pentagon, Colors::Uniform(RED));
        let (v, i, ranges) = flatten(&buf);
        assert_eq!(v.len(), 5);
        assert_eq!(i, [0, 1, 2, 0, 2, 3, 0, 3, 4]);
        assert_eq!(ranges, [(0, 0..9)]);
    }

    #[test]
    fn degenerate_polygons_draw_nothing() {
        let mut buf = ShapeBuffer::default();
        buf.polygon(0, &[[0.0, 0.0], [1.0, 1.0]], Colors::Uniform(RED));
        let (v, i, ranges) = flatten(&buf);
        assert!(v.is_empty() && i.is_empty() && ranges.is_empty());
    }

    #[test]
    fn layers_flatten_in_order_with_rebased_indices() {
        let mut buf = ShapeBuffer::default();
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        buf.triangles(2, &tri, Colors::Uniform(RED), &[0, 1, 2]);
        buf.polygon(
            -1,
            &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            Colors::Uniform(BLUE),
        );
        let (v, i, ranges) = flatten(&buf);
        // Layer -1 (the quad) comes first even though it was queued second.
        assert_eq!(ranges, [(-1, 0..6), (2, 6..9)]);
        assert_eq!(&i[..6], [0, 1, 2, 0, 2, 3]);
        assert_eq!(&i[6..], [4, 5, 6]);
        assert_eq!(v[0].color, BLUE);
        assert_eq!(v[4].color, RED);
    }

    #[test]
    fn same_layer_keeps_call_order() {
        let mut buf = ShapeBuffer::default();
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        buf.triangles(0, &tri, Colors::Uniform(RED), &[0, 1, 2]);
        buf.triangles(0, &tri, Colors::Uniform(BLUE), &[2, 1, 0]);
        let (v, i, ranges) = flatten(&buf);
        assert_eq!(ranges, [(0, 0..6)]);
        assert_eq!(i, [0, 1, 2, 5, 4, 3]);
        assert_eq!(v[3].color, BLUE);
    }

    #[test]
    fn per_vertex_colors_follow_their_vertices() {
        let mut buf = ShapeBuffer::default();
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let colors = [RED, BLUE, RED];
        buf.triangles(0, &tri, Colors::PerVertex(&colors), &[0, 1, 2]);
        buf.polygon(
            0,
            &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            Colors::PerVertex(&[BLUE, BLUE, RED, RED]),
        );
        let (v, i, _) = flatten(&buf);
        let got: Vec<[f32; 4]> = v.iter().map(|v| v.color).collect();
        assert_eq!(got, [RED, BLUE, RED, BLUE, BLUE, RED, RED]);
        assert_eq!(i, [0, 1, 2, 3, 4, 5, 3, 5, 6]);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "one color per vertex")]
    fn mismatched_color_count_panics_in_debug_builds() {
        let mut buf = ShapeBuffer::default();
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        buf.triangles(0, &tri, Colors::PerVertex(&[RED, BLUE]), &[0, 1, 2]);
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn mismatched_color_count_draws_nothing_in_release_builds() {
        let mut buf = ShapeBuffer::default();
        let quad = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        buf.polygon(0, &quad, Colors::PerVertex(&[RED]));
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        buf.triangles(0, &tri, Colors::PerVertex(&[RED, BLUE]), &[0, 1, 2]);
        let (v, i, ranges) = flatten(&buf);
        assert!(v.is_empty() && i.is_empty() && ranges.is_empty());
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn malformed_triangles_draw_nothing_in_release_builds() {
        let mut buf = ShapeBuffer::default();
        buf.triangles(
            0,
            &[[0.0, 0.0], [1.0, 0.0]],
            Colors::Uniform(RED),
            &[0, 1, 2],
        );
        buf.triangles(
            0,
            &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            Colors::Uniform(RED),
            &[0, 1],
        );
        let (v, i, ranges) = flatten(&buf);
        assert!(v.is_empty() && i.is_empty() && ranges.is_empty());
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "need whole triangles, all in range")]
    fn out_of_range_indices_panic_in_debug_builds() {
        let mut buf = ShapeBuffer::default();
        buf.triangles(
            0,
            &[[0.0, 0.0], [1.0, 0.0]],
            Colors::Uniform(RED),
            &[0, 1, 2],
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "need whole triangles, all in range")]
    fn partial_triangles_panic_in_debug_builds() {
        let mut buf = ShapeBuffer::default();
        buf.triangles(
            0,
            &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            Colors::Uniform(RED),
            &[0, 1],
        );
    }
}
