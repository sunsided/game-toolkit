use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use bytemuck::{Pod, Zeroable};

use crate::sprite::BlendMode;
use crate::target::Targets;
use crate::texture::{TextureId, TextureRegistry};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug, PartialEq)]
struct ShapeVertex {
    pos: [f32; 2],
    /// Texture coordinate pre-multiplied by `q`, plus `q` itself. The fragment shader samples
    /// at `uvq.xy / uvq.z`, which makes quads perspective-correct; `q = 1` is plain affine.
    uvq: [f32; 3],
    color: [f32; 4],
}

/// What a group of shapes is drawn with. Groups sort by layer, then blend mode, then texture
/// (the same order as sprites); call order is kept within a group.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShapeKey {
    pub layer: i16,
    pub blend: BlendMode,
    pub texture: TextureId,
}

impl ShapeKey {
    fn sort_key(&self) -> (i16, u8, u32) {
        (self.layer, self.blend as u8, self.texture.0)
    }
}

impl Ord for ShapeKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

impl PartialOrd for ShapeKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
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

/// Texture coordinates for a shape: none (untextured shapes sample the white texture),
/// plain per-vertex UVs, or UVs already in `(u·q, v·q, q)` form.
#[derive(Copy, Clone)]
pub(crate) enum Uvs<'a> {
    None,
    Affine(&'a [[f32; 2]]),
    Projective(&'a [[f32; 3]]),
}

impl Uvs<'_> {
    fn fits(&self, vertex_count: usize) -> bool {
        match self {
            Uvs::None => true,
            Uvs::Affine(uvs) => uvs.len() == vertex_count,
            Uvs::Projective(uvqs) => uvqs.len() == vertex_count,
        }
    }

    fn at(&self, i: usize) -> [f32; 3] {
        match self {
            Uvs::None => [0.0, 0.0, 1.0],
            Uvs::Affine(uvs) => [uvs[i][0], uvs[i][1], 1.0],
            Uvs::Projective(uvqs) => uvqs[i],
        }
    }
}

/// Perspective-correct texture coordinates for a convex quad `points` (in order) showing the
/// rectangle with corner UVs `uvs`, as `(u·q, v·q, q)`.
///
/// The quad is treated as a planar rectangle seen in perspective. Its diagonals cross at a
/// point splitting the first diagonal at `t` and the second at `s` (`0..1`, from corner 0 and
/// corner 1); each corner's `q` is the full diagonal length over the distance from the
/// crossing to the *opposite* corner. A parallelogram gives equal `q`s, i.e. the affine
/// mapping. Concave, self-intersecting or degenerate quads have no such crossing and fall
/// back to affine (`q = 1`).
pub(crate) fn quad_uvq(points: [[f32; 2]; 4], uvs: [[f32; 2]; 4]) -> [[f32; 3]; 4] {
    let sub = |a: [f32; 2], b: [f32; 2]| [a[0] - b[0], a[1] - b[1]];
    let cross = |a: [f32; 2], b: [f32; 2]| a[0] * b[1] - a[1] * b[0];
    let affine = uvs.map(|[u, v]| [u, v, 1.0]);

    let d0 = sub(points[2], points[0]);
    let d1 = sub(points[3], points[1]);
    let denom = cross(d0, d1);
    if denom.abs() <= f32::EPSILON * (d0[0].abs() + d0[1].abs()) * (d1[0].abs() + d1[1].abs()) {
        return affine;
    }
    let p = sub(points[1], points[0]);
    let t = cross(p, d1) / denom;
    let s = cross(p, d0) / denom;
    if !(t > 0.0 && t < 1.0 && s > 0.0 && s < 1.0) {
        return affine;
    }
    let q = [1.0 / (1.0 - t), 1.0 / (1.0 - s), 1.0 / t, 1.0 / s];
    std::array::from_fn(|i| [uvs[i][0] * q[i], uvs[i][1] * q[i], q[i]])
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

/// Each drawn group with its range into the combined index buffer, in [`ShapeKey`] order.
type GroupRanges = Vec<(ShapeKey, Range<u32>)>;

/// One group's triangles: vertices plus indices local to this group's vertex list.
#[derive(Default)]
struct GroupGeometry {
    vertices: Vec<ShapeVertex>,
    indices: Vec<u32>,
}

/// CPU-side geometry for filled shapes, bucketed by [`ShapeKey`]. Kept free of GPU state so
/// the triangulation and grouping can be unit-tested.
#[derive(Default)]
struct ShapeBuffer {
    groups: BTreeMap<ShapeKey, GroupGeometry>,
}

impl ShapeBuffer {
    /// Queue an indexed triangle list. Draws nothing (see [`reject`]) when an index is out of
    /// range, the index count is not a multiple of three, or there is not one colour (or UV)
    /// per vertex.
    fn triangles(
        &mut self,
        key: ShapeKey,
        points: &[[f32; 2]],
        colors: Colors<'_>,
        uvs: Uvs<'_>,
        indices: &[u32],
    ) {
        if !indices.len().is_multiple_of(3) || indices.iter().any(|&i| i as usize >= points.len()) {
            return reject(format_args!(
                "triangles: {} indices into {} vertices; need whole triangles, all in range",
                indices.len(),
                points.len()
            ));
        }
        if !colors.fits(points.len()) || !uvs.fits(points.len()) {
            return reject(format_args!("shape: need one color / uv per vertex"));
        }
        if indices.is_empty() {
            return;
        }
        let base = self.push_vertices(key, points, colors, uvs);
        let geo = self.groups.entry(key).or_default();
        geo.indices.extend(indices.iter().map(|&i| base + i));
    }

    /// Queue a convex polygon, fan-triangulated from its first point. Fewer than three
    /// points draw nothing; so does a colour or UV count that does not match (see
    /// [`reject`]).
    fn polygon(&mut self, key: ShapeKey, points: &[[f32; 2]], colors: Colors<'_>, uvs: Uvs<'_>) {
        if !colors.fits(points.len()) || !uvs.fits(points.len()) {
            return reject(format_args!("shape: need one color / uv per vertex"));
        }
        if points.len() < 3 {
            return;
        }
        let base = self.push_vertices(key, points, colors, uvs);
        let geo = self.groups.entry(key).or_default();
        for i in 1..points.len() as u32 - 1 {
            geo.indices.extend([base, base + i, base + i + 1]);
        }
    }

    /// Append `points` with their colours and UVs to `key`'s group and return the index of
    /// the first one.
    fn push_vertices(
        &mut self,
        key: ShapeKey,
        points: &[[f32; 2]],
        colors: Colors<'_>,
        uvs: Uvs<'_>,
    ) -> u32 {
        let geo = self.groups.entry(key).or_default();
        let base = geo.vertices.len() as u32;
        geo.vertices
            .extend(points.iter().enumerate().map(|(i, &pos)| ShapeVertex {
                pos,
                uvq: uvs.at(i),
                color: colors.at(i),
            }));
        base
    }

    fn collect_layers(&self, out: &mut std::collections::BTreeSet<i16>) {
        out.extend(self.groups.keys().map(|k| k.layer));
    }

    /// Concatenate every group (in [`ShapeKey`] order) into one vertex and one index list,
    /// with indices rebased onto the combined vertex list. Returns each group's index range.
    fn flatten(&self, vertices: &mut Vec<ShapeVertex>, indices: &mut Vec<u32>) -> GroupRanges {
        vertices.clear();
        indices.clear();
        let mut ranges = Vec::with_capacity(self.groups.len());
        for (&key, geo) in &self.groups {
            let base = vertices.len() as u32;
            let start = indices.len() as u32;
            vertices.extend_from_slice(&geo.vertices);
            indices.extend(geo.indices.iter().map(|&i| base + i));
            ranges.push((key, start..indices.len() as u32));
        }
        ranges
    }

    fn clear(&mut self) {
        self.groups.clear();
    }
}

/// Draws filled triangles (quads, convex polygons, raw triangle lists) interleaved with
/// sprites and circles by layer. Within a layer, one draw call per (blend mode, texture)
/// group, so call order is kept within a group.
pub(crate) struct ShapeBatcher {
    shapes: ShapeBuffer,
    vertices: Vec<ShapeVertex>,
    indices: Vec<u32>,
    ranges: GroupRanges,
    vb: wgpu::Buffer,
    ib: wgpu::Buffer,
    vertex_capacity: usize,
    index_capacity: usize,
    pipelines: HashMap<BlendMode, wgpu::RenderPipeline>,
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
        texture_bgl: &wgpu::BindGroupLayout,
        sample_count: u32,
        depth_format: Option<wgpu::TextureFormat>,
    ) -> Self {
        let vertex_capacity = 4096usize;
        let index_capacity = 8192usize;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shape.shader"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(include_str!("srgb.wgsl"), include_str!("shape.wgsl")).into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shape.layout"),
            bind_group_layouts: &[Some(camera_bgl), Some(texture_bgl)],
            immediate_size: 0,
        });
        let make_pipeline = |blend: BlendMode| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&format!("shape.{}", blend.label())),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<ShapeVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x2,
                            1 => Float32x3,
                            2 => Float32x4,
                        ],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: Some(blend.state()),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                // No culling: callers may wind their points either way.
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: depth_format.map(crate::target::no_write_depth),
                multisample: crate::target::multisample(sample_count),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipelines = BlendMode::ALL
            .into_iter()
            .map(|blend| (blend, make_pipeline(blend)))
            .collect();

        Self {
            shapes: ShapeBuffer::default(),
            vertices: Vec::new(),
            indices: Vec::new(),
            ranges: Vec::new(),
            vb: vertex_buffer(device, vertex_capacity),
            ib: index_buffer(device, index_capacity),
            vertex_capacity,
            index_capacity,
            pipelines,
        }
    }

    pub fn triangles(
        &mut self,
        key: ShapeKey,
        points: &[[f32; 2]],
        colors: Colors<'_>,
        uvs: Uvs<'_>,
        indices: &[u32],
    ) {
        self.shapes.triangles(key, points, colors, uvs, indices);
    }

    pub fn polygon(
        &mut self,
        key: ShapeKey,
        points: &[[f32; 2]],
        colors: Colors<'_>,
        uvs: Uvs<'_>,
    ) {
        self.shapes.polygon(key, points, colors, uvs);
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
    /// already-cleared target, one draw call per (blend mode, texture) group.
    pub fn draw_layer(
        &self,
        layer: i16,
        encoder: &mut wgpu::CommandEncoder,
        targets: &Targets<'_>,
        camera_bg: &wgpu::BindGroup,
        textures: &TextureRegistry,
    ) {
        // `ranges` is sorted by key, so this layer's groups are a contiguous run.
        let lo = self.ranges.partition_point(|(k, _)| k.layer < layer);
        let hi = self.ranges.partition_point(|(k, _)| k.layer <= layer);
        if lo == hi {
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
        pass.set_bind_group(0, camera_bg, &[]);
        pass.set_vertex_buffer(0, self.vb.slice(..));
        pass.set_index_buffer(self.ib.slice(..), wgpu::IndexFormat::Uint32);
        for (key, range) in &self.ranges[lo..hi] {
            pass.set_pipeline(&self.pipelines[&key.blend]);
            pass.set_bind_group(1, textures.bind_group(key.texture), &[]);
            pass.draw_indexed(range.clone(), 0, 0..1);
        }
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

    const WHITE: TextureId = TextureId(1);
    const ATLAS: TextureId = TextureId(2);

    /// Key of a plain alpha-blended shape on `layer`.
    fn key(layer: i16) -> ShapeKey {
        ShapeKey {
            layer,
            blend: BlendMode::Alpha,
            texture: WHITE,
        }
    }

    fn layers(ranges: &GroupRanges) -> Vec<(i16, Range<u32>)> {
        ranges.iter().map(|(k, r)| (k.layer, r.clone())).collect()
    }

    fn flatten(buf: &ShapeBuffer) -> (Vec<ShapeVertex>, Vec<u32>, GroupRanges) {
        let (mut v, mut i) = (Vec::new(), Vec::new());
        let ranges = buf.flatten(&mut v, &mut i);
        (v, i, ranges)
    }

    /// Perspective-interpolate `a`..`b` at `t` the way the GPU does: lerp `uvq`, divide by q.
    fn uv_between(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 2] {
        let l: [f32; 3] = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
        [l[0] / l[2], l[1] / l[2]]
    }

    fn assert_close(a: [f32; 2], b: [f32; 2]) {
        assert!(
            (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5,
            "{a:?} != {b:?}"
        );
    }

    const UNIT: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

    #[test]
    fn groups_split_a_layer_by_blend_then_texture() {
        let mut buf = ShapeBuffer::default();
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let textured = ShapeKey {
            texture: ATLAS,
            ..key(0)
        };
        let additive = ShapeKey {
            blend: BlendMode::Additive,
            ..key(0)
        };
        // Queued in reverse of the draw order.
        buf.triangles(additive, &tri, Colors::Uniform(RED), Uvs::None, &[0, 1, 2]);
        buf.triangles(
            textured,
            &tri,
            Colors::Uniform(RED),
            Uvs::Affine(&UNIT[..3]),
            &[0, 1, 2],
        );
        buf.triangles(key(0), &tri, Colors::Uniform(RED), Uvs::None, &[0, 1, 2]);
        let (_, _, ranges) = flatten(&buf);
        let keys: Vec<ShapeKey> = ranges.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, [key(0), textured, additive]);
        let mut layers = std::collections::BTreeSet::new();
        buf.collect_layers(&mut layers);
        assert_eq!(layers.into_iter().collect::<Vec<_>>(), [0]);
    }

    #[test]
    fn untextured_shapes_sample_the_texture_origin_affinely() {
        let mut buf = ShapeBuffer::default();
        buf.polygon(key(0), &UNIT, Colors::Uniform(RED), Uvs::None);
        let (v, _, _) = flatten(&buf);
        assert!(v.iter().all(|v| v.uvq == [0.0, 0.0, 1.0]));
    }

    #[test]
    fn affine_uvs_get_q_of_one() {
        let mut buf = ShapeBuffer::default();
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let uvs = [[0.25, 0.5], [0.75, 0.5], [0.25, 1.0]];
        buf.triangles(
            key(0),
            &tri,
            Colors::Uniform(RED),
            Uvs::Affine(&uvs),
            &[0, 1, 2],
        );
        let (v, _, _) = flatten(&buf);
        let got: Vec<[f32; 3]> = v.iter().map(|v| v.uvq).collect();
        assert_eq!(got, [[0.25, 0.5, 1.0], [0.75, 0.5, 1.0], [0.25, 1.0, 1.0]]);
    }

    #[test]
    fn parallelogram_quads_map_affinely() {
        let points = [[0.0, 0.0], [4.0, 0.0], [6.0, 2.0], [2.0, 2.0]];
        let uvq = quad_uvq(points, UNIT);
        // Equal q on every corner: dividing it back out gives the plain UVs everywhere.
        assert!(uvq.iter().all(|c| (c[2] - uvq[0][2]).abs() < 1e-6));
        for i in 0..4 {
            assert_close([uvq[i][0] / uvq[i][2], uvq[i][1] / uvq[i][2]], UNIT[i]);
        }
    }

    #[test]
    fn trapezoid_maps_its_diagonal_crossing_to_the_texture_centre() {
        // A road segment: narrow far edge on top, wide near edge at the bottom.
        let points = [[3.0, 0.0], [5.0, 0.0], [8.0, 4.0], [0.0, 4.0]];
        let uvq = quad_uvq(points, UNIT);
        // The diagonals cross at (4, 0.8): a fifth of the way along each diagonal.
        let t = 0.2;
        let s = 0.2;
        assert_close(uv_between(uvq[0], uvq[2], t), [0.5, 0.5]);
        assert_close(uv_between(uvq[1], uvq[3], s), [0.5, 0.5]);
        // Affine UVs would put the centre at the diagonals' midpoints; perspective doesn't.
        assert!((uv_between(uvq[0], uvq[2], 0.5)[1] - 0.5).abs() > 0.1);
    }

    #[test]
    fn concave_and_degenerate_quads_fall_back_to_affine() {
        let concave = [[0.0, 0.0], [4.0, 0.0], [1.0, 1.0], [0.0, 4.0]];
        let collapsed = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
        for points in [concave, collapsed] {
            let uvq = quad_uvq(points, UNIT);
            assert_eq!(uvq, UNIT.map(|[u, v]| [u, v, 1.0]), "{points:?}");
        }
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "one color / uv per vertex")]
    fn mismatched_uv_count_panics_in_debug_builds() {
        let mut buf = ShapeBuffer::default();
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        buf.triangles(
            key(0),
            &tri,
            Colors::Uniform(RED),
            Uvs::Affine(&UNIT[..2]),
            &[0, 1, 2],
        );
    }

    #[test]
    fn polygon_is_fan_triangulated() {
        let mut buf = ShapeBuffer::default();
        let pentagon = [[0.0, 0.0], [2.0, 0.0], [3.0, 1.0], [1.0, 2.0], [-1.0, 1.0]];
        buf.polygon(key(0), &pentagon, Colors::Uniform(RED), Uvs::None);
        let (v, i, ranges) = flatten(&buf);
        assert_eq!(v.len(), 5);
        assert_eq!(i, [0, 1, 2, 0, 2, 3, 0, 3, 4]);
        assert_eq!(layers(&ranges), [(0, 0..9)]);
    }

    #[test]
    fn degenerate_polygons_draw_nothing() {
        let mut buf = ShapeBuffer::default();
        buf.polygon(
            key(0),
            &[[0.0, 0.0], [1.0, 1.0]],
            Colors::Uniform(RED),
            Uvs::None,
        );
        let (v, i, ranges) = flatten(&buf);
        assert!(v.is_empty() && i.is_empty() && ranges.is_empty());
    }

    #[test]
    fn layers_flatten_in_order_with_rebased_indices() {
        let mut buf = ShapeBuffer::default();
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        buf.triangles(key(2), &tri, Colors::Uniform(RED), Uvs::None, &[0, 1, 2]);
        buf.polygon(
            key(-1),
            &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            Colors::Uniform(BLUE),
            Uvs::None,
        );
        let (v, i, ranges) = flatten(&buf);
        // Layer -1 (the quad) comes first even though it was queued second.
        assert_eq!(layers(&ranges), [(-1, 0..6), (2, 6..9)]);
        assert_eq!(&i[..6], [0, 1, 2, 0, 2, 3]);
        assert_eq!(&i[6..], [4, 5, 6]);
        assert_eq!(v[0].color, BLUE);
        assert_eq!(v[4].color, RED);
    }

    #[test]
    fn same_layer_keeps_call_order() {
        let mut buf = ShapeBuffer::default();
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        buf.triangles(key(0), &tri, Colors::Uniform(RED), Uvs::None, &[0, 1, 2]);
        buf.triangles(key(0), &tri, Colors::Uniform(BLUE), Uvs::None, &[2, 1, 0]);
        let (v, i, ranges) = flatten(&buf);
        assert_eq!(layers(&ranges), [(0, 0..6)]);
        assert_eq!(i, [0, 1, 2, 5, 4, 3]);
        assert_eq!(v[3].color, BLUE);
    }

    #[test]
    fn per_vertex_colors_follow_their_vertices() {
        let mut buf = ShapeBuffer::default();
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let colors = [RED, BLUE, RED];
        buf.triangles(
            key(0),
            &tri,
            Colors::PerVertex(&colors),
            Uvs::None,
            &[0, 1, 2],
        );
        buf.polygon(
            key(0),
            &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            Colors::PerVertex(&[BLUE, BLUE, RED, RED]),
            Uvs::None,
        );
        let (v, i, _) = flatten(&buf);
        let got: Vec<[f32; 4]> = v.iter().map(|v| v.color).collect();
        assert_eq!(got, [RED, BLUE, RED, BLUE, BLUE, RED, RED]);
        assert_eq!(i, [0, 1, 2, 3, 4, 5, 3, 5, 6]);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "one color / uv per vertex")]
    fn mismatched_color_count_panics_in_debug_builds() {
        let mut buf = ShapeBuffer::default();
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        buf.triangles(
            key(0),
            &tri,
            Colors::PerVertex(&[RED, BLUE]),
            Uvs::None,
            &[0, 1, 2],
        );
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn mismatched_color_count_draws_nothing_in_release_builds() {
        let mut buf = ShapeBuffer::default();
        let quad = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        buf.polygon(key(0), &quad, Colors::PerVertex(&[RED]), Uvs::None);
        let tri = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        buf.triangles(
            key(0),
            &tri,
            Colors::PerVertex(&[RED, BLUE]),
            Uvs::None,
            &[0, 1, 2],
        );
        let (v, i, ranges) = flatten(&buf);
        assert!(v.is_empty() && i.is_empty() && ranges.is_empty());
    }

    #[test]
    #[cfg(not(debug_assertions))]
    fn malformed_triangles_draw_nothing_in_release_builds() {
        let mut buf = ShapeBuffer::default();
        buf.triangles(
            key(0),
            &[[0.0, 0.0], [1.0, 0.0]],
            Colors::Uniform(RED),
            Uvs::None,
            &[0, 1, 2],
        );
        buf.triangles(
            key(0),
            &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            Colors::Uniform(RED),
            Uvs::None,
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
            key(0),
            &[[0.0, 0.0], [1.0, 0.0]],
            Colors::Uniform(RED),
            Uvs::None,
            &[0, 1, 2],
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "need whole triangles, all in range")]
    fn partial_triangles_panic_in_debug_builds() {
        let mut buf = ShapeBuffer::default();
        buf.triangles(
            key(0),
            &[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            Colors::Uniform(RED),
            Uvs::None,
            &[0, 1],
        );
    }
}
