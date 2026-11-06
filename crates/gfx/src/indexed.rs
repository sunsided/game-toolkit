//! Indexed colour (feature `indexed`): draw palette indices, resolve them through a palette
//! lookup texture at the end of the frame, cycle and crossfade palettes for free.
//!
//! The index of each canvas pixel travels in its red channel. Painter colours made with
//! [`Color::index`](crate::Color::index) and textures from
//! [`Graphics::load_indexed_texture`](crate::Graphics::load_indexed_texture) carry an index
//! exactly through the sRGB canvas (nearest sampling, alpha 0 or 1), and the resolve pass
//! looks it up in the [`Palette`] before any post effect runs.

use std::io::Cursor;

use anyhow::{Context as _, Result, bail};
use bytemuck::{Pod, Zeroable};

use crate::canvas::RenderTexture;
use crate::post::StageRun;

/// How many palette rows the lookup texture holds: [`Graphics::set_palette`] fills them,
/// [`Graphics::set_palette_blend`] crossfades between two. Sixteen covers a base palette with
/// times of day and eras or biomes; the lookup texture is only 16 KiB.
///
/// [`Graphics::set_palette`]: crate::Graphics::set_palette
/// [`Graphics::set_palette_blend`]: crate::Graphics::set_palette_blend
pub const PALETTE_ROWS: u32 = 16;

/// WGSL for passes that write or read the indexed canvas themselves, e.g. a
/// [`Painter::custom`](crate::Painter::custom) background or a post effect sampling
/// [`PostPass::indices`](crate::PostPass::indices):
///
/// - `index_out(i: u32) -> vec4<f32>`: return it from a fragment shader to store index `i`;
/// - `index_in(texel: vec4<f32>) -> u32`: the index of a texel loaded from the canvas.
///
/// This is the contract for custom passes: the canvas stores the index in the red channel
/// of an sRGB colour format, and these two hide the encoding. Draw indices with blending off
/// (or alpha 1) so they are never mixed. Prepend it to your shader source.
pub const INDEX_WGSL: &str = include_str!("index.wgsl");

/// Most palette cycles active at once.
pub const MAX_PALETTE_CYCLES: usize = 16;

/// 256 sRGB colours, indexed by the values drawn into an indexed canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    colors: [[u8; 4]; 256],
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            colors: [[0, 0, 0, 255]; 256],
        }
    }
}

impl Palette {
    /// The first `colors.len()` (at most 256) entries from `colors`, sRGB RGBA bytes; the rest
    /// black.
    pub fn from_colors(colors: &[[u8; 4]]) -> Self {
        let mut p = Self::default();
        for (dst, src) in p.colors.iter_mut().zip(colors) {
            *dst = *src;
        }
        p
    }

    /// Entries from a packed `0xRRGGBB` list, all opaque.
    pub fn from_hex(colors: &[u32]) -> Self {
        let rgba: Vec<[u8; 4]> = colors
            .iter()
            .map(|c| [(c >> 16) as u8, (c >> 8) as u8, *c as u8, 255])
            .collect();
        Self::from_colors(&rgba)
    }

    pub fn get(&self, index: u8) -> [u8; 4] {
        self.colors[index as usize]
    }

    pub fn set(&mut self, index: u8, rgba: [u8; 4]) {
        self.colors[index as usize] = rgba;
    }

    pub fn colors(&self) -> &[[u8; 4]; 256] {
        &self.colors
    }

    pub fn colors_mut(&mut self) -> &mut [[u8; 4]; 256] {
        &mut self.colors
    }

    fn bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.colors)
    }
}

/// Rotate a range of palette entries over time: what was drawn with index `start + k` shows
/// the colour of `start + (k + offset) % len`. Classic water, fire and conveyor animation
/// without redrawing a pixel. Added with
/// [`Graphics::add_palette_cycle`](crate::Graphics::add_palette_cycle).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PaletteCycle {
    /// First index of the range.
    pub start: u8,
    /// Number of entries in the range (`start + len` must not exceed 256).
    pub len: u16,
    /// Steps per second; negative runs the other way.
    pub speed: f32,
}

impl PaletteCycle {
    /// The rotation at `time` seconds, in `0..len`. Computed in `f64`, so cycles keep
    /// stepping evenly however long the game runs.
    pub fn offset_at(&self, time: f64) -> u32 {
        let len = i64::from(self.len.max(1));
        ((time * f64::from(self.speed)).floor() as i64).rem_euclid(len) as u32
    }

    /// Whether the range is non-empty and fits in the palette.
    pub fn is_valid(&self) -> bool {
        self.len > 0 && u32::from(self.start) + u32::from(self.len) <= 256
    }
}

/// Handle to a cycle added with [`Graphics::add_palette_cycle`](crate::Graphics::add_palette_cycle).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PaletteCycleId(u64);

/// An image whose pixels are palette indices, decoded from an indexed PNG.
#[derive(Clone, Debug)]
pub struct IndexedImage {
    pub width: u32,
    pub height: u32,
    /// One index per pixel, row by row.
    pub indices: Vec<u8>,
    /// The PNG's own palette (`PLTE` with `tRNS` alpha).
    pub palette: Palette,
    /// Indices whose `tRNS` alpha is zero: drawn transparent.
    pub transparent: Vec<u8>,
}

impl IndexedImage {
    /// Decode an indexed-colour PNG (any bit depth) without converting it to RGBA.
    pub fn from_png(bytes: &[u8]) -> Result<Self> {
        let mut decoder = png::Decoder::new(Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::IDENTITY);
        let mut reader = decoder.read_info().context("read PNG header")?;
        let info = reader.info();
        if info.color_type != png::ColorType::Indexed {
            bail!("not an indexed-colour PNG ({:?})", info.color_type);
        }
        let bits = info.bit_depth as u8;
        let plte = info.palette.as_deref().unwrap_or(&[]).to_vec();
        let trns = info.trns.as_deref().unwrap_or(&[]).to_vec();
        let mut buf = vec![0; reader.output_buffer_size().context("PNG too large")?];
        let out = reader.next_frame(&mut buf).context("decode PNG")?;
        let (width, height) = (out.width, out.height);
        let mut indices = Vec::with_capacity((width * height) as usize);
        for row in buf.chunks(out.line_size).take(height as usize) {
            indices.extend(unpack_row(row, bits, width));
        }
        let mut palette = Palette::default();
        let mut transparent = Vec::new();
        for (i, rgb) in plte.as_chunks::<3>().0.iter().take(256).enumerate() {
            let a = trns.get(i).copied().unwrap_or(255);
            palette.colors[i] = [rgb[0], rgb[1], rgb[2], a];
            if a == 0 {
                transparent.push(i as u8);
            }
        }
        Ok(Self {
            width,
            height,
            indices,
            palette,
            transparent,
        })
    }

    /// RGBA texels carrying the indices for an indexed canvas: red = index, alpha 0 for
    /// transparent indices and 255 otherwise.
    pub fn index_texels(&self) -> Vec<u8> {
        let mut transparent = [false; 256];
        for &i in &self.transparent {
            transparent[i as usize] = true;
        }
        self.indices
            .iter()
            .flat_map(|&i| [i, 0, 0, if transparent[i as usize] { 0 } else { 255 }])
            .collect()
    }
}

/// Unpack one PNG row of `bits`-bit indices.
fn unpack_row(row: &[u8], bits: u8, width: u32) -> impl Iterator<Item = u8> + '_ {
    let per_byte = 8 / u32::from(bits.max(1));
    let mask = ((1u16 << bits) - 1) as u8;
    (0..width).map(move |x| {
        let byte = row[(x / per_byte) as usize];
        let shift = 8 - u32::from(bits) * (x % per_byte + 1);
        (byte >> shift) & mask
    })
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct ResolveUniform {
    row_a: u32,
    row_b: u32,
    blend: f32,
    cycle_count: u32,
    /// `(start, len, offset, 0)` per cycle.
    cycles: [[u32; 4]; MAX_PALETTE_CYCLES],
}

/// Palette lookup texture, cycles and the resolve pass.
pub(crate) struct IndexedState {
    palette_tex: wgpu::Texture,
    palette_view: wgpu::TextureView,
    uniform: wgpu::Buffer,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    pub resolved: Option<RenderTexture>,
    row_a: u32,
    row_b: u32,
    blend: f32,
    cycles: Vec<(PaletteCycleId, PaletteCycle)>,
    next_cycle: u64,
}

impl IndexedState {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let palette_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("palette"),
            size: wgpu::Extent3d {
                width: 256,
                height: PALETTE_ROWS,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // sRGB: lookups decode to linear and the sRGB target encodes them back, so the
            // authored bytes come out exactly.
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let palette_view = palette_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("palette.resolve.uniform"),
            size: size_of::<ResolveUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tex = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("palette.resolve.bgl"),
            entries: &[
                tex(0),
                tex(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("palette_resolve.wgsl"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("index.wgsl"),
                    include_str!("palette_resolve.wgsl")
                )
                .into(),
            ),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("palette.resolve.layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("palette.resolve"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let mut me = Self {
            palette_tex,
            palette_view,
            uniform,
            layout,
            pipeline,
            resolved: None,
            row_a: 0,
            row_b: 0,
            blend: 0.0,
            cycles: Vec::new(),
            next_cycle: 0,
        };
        for row in 0..PALETTE_ROWS {
            me.set_palette(queue, row, &Palette::default());
        }
        me
    }

    pub fn set_palette(&mut self, queue: &wgpu::Queue, row: u32, palette: &Palette) {
        if row >= PALETTE_ROWS {
            log::warn!("set_palette: row {row} out of range (0..{PALETTE_ROWS})");
            return;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.palette_tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: row, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            palette.bytes(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256 * 4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 256,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn set_blend(&mut self, row_a: u32, row_b: u32, t: f32) {
        self.row_a = row_a.min(PALETTE_ROWS - 1);
        self.row_b = row_b.min(PALETTE_ROWS - 1);
        self.blend = t.clamp(0.0, 1.0);
    }

    pub fn add_cycle(&mut self, cycle: PaletteCycle) -> Option<PaletteCycleId> {
        if !cycle.is_valid() {
            log::warn!(
                "add_palette_cycle: range {}..{} is empty or past index 255",
                cycle.start,
                u32::from(cycle.start) + u32::from(cycle.len)
            );
            return None;
        }
        if self.cycles.len() >= MAX_PALETTE_CYCLES {
            log::warn!("add_palette_cycle: at most {MAX_PALETTE_CYCLES} cycles");
            return None;
        }
        self.next_cycle += 1;
        let id = PaletteCycleId(self.next_cycle);
        self.cycles.push((id, cycle));
        Some(id)
    }

    pub fn remove_cycle(&mut self, id: PaletteCycleId) -> bool {
        let before = self.cycles.len();
        self.cycles.retain(|(c, _)| *c != id);
        self.cycles.len() != before
    }

    /// Resolve `indices` (the canvas) into `self.resolved`, `size` pixels.
    pub fn resolve(
        &mut self,
        run: &StageRun<'_>,
        encoder: &mut wgpu::CommandEncoder,
        indices: &wgpu::TextureView,
        size: [u32; 2],
        time: f64,
    ) {
        let StageRun {
            device,
            queue,
            blit,
            format,
            ..
        } = *run;
        if self.resolved.as_ref().map(|r| r.size) != Some(size) {
            self.resolved = Some(RenderTexture::new(
                device,
                blit,
                format,
                size,
                "palette.resolved",
            ));
        }
        let mut uniform = ResolveUniform {
            row_a: self.row_a,
            row_b: self.row_b,
            blend: self.blend,
            cycle_count: self.cycles.len() as u32,
            cycles: [[0; 4]; MAX_PALETTE_CYCLES],
        };
        for (slot, (_, c)) in uniform.cycles.iter_mut().zip(&self.cycles) {
            // `add_cycle` only accepts ranges that fit, so `len` is the cycle's own.
            let len = u32::from(c.len);
            *slot = [u32::from(c.start), len, c.offset_at(time), 0];
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&uniform));
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("palette.resolve.bg"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(indices),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.palette_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        });
        let Some(out) = &self.resolved else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("palette.resolve"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &out.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_offsets_wrap_both_ways() {
        let c = PaletteCycle {
            start: 16,
            len: 4,
            speed: 2.0,
        };
        assert_eq!(c.offset_at(0.0), 0);
        assert_eq!(c.offset_at(0.5), 1);
        assert_eq!(c.offset_at(2.0), 0);
        let back = PaletteCycle { speed: -2.0, ..c };
        assert_eq!(back.offset_at(0.5), 3);
        // Still steps after weeks of running at a high speed (f32 would stall here).
        let fast = PaletteCycle { speed: 1000.0, ..c };
        let t = 30.0 * 24.0 * 3600.0;
        assert_ne!(fast.offset_at(t), fast.offset_at(t + 0.001));
    }

    #[test]
    fn cycles_must_fit_the_palette() {
        let ok = PaletteCycle {
            start: 248,
            len: 8,
            speed: 1.0,
        };
        assert!(ok.is_valid());
        assert!(!PaletteCycle { len: 9, ..ok }.is_valid());
        assert!(!PaletteCycle { len: 0, ..ok }.is_valid());
    }

    #[test]
    fn unpacks_sub_byte_rows() {
        // 2-bit: 0b00_01_10_11 -> 0, 1, 2, 3
        assert_eq!(
            unpack_row(&[0b0001_1011], 2, 4).collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        // 4-bit, odd width.
        assert_eq!(
            unpack_row(&[0xA5, 0xF0], 4, 3).collect::<Vec<_>>(),
            [10, 5, 15]
        );
        assert_eq!(unpack_row(&[7, 9], 8, 2).collect::<Vec<_>>(), [7, 9]);
    }

    #[test]
    fn decodes_indexed_png_with_transparency() {
        let mut bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut bytes, 2, 1);
            enc.set_color(png::ColorType::Indexed);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_palette(vec![0, 0, 0, 255, 0, 0]);
            enc.set_trns(vec![0, 255]);
            let mut w = enc.write_header().unwrap();
            w.write_image_data(&[1, 0]).unwrap();
        }
        let img = IndexedImage::from_png(&bytes).unwrap();
        assert_eq!(img.indices, [1, 0]);
        assert_eq!(img.palette.get(1), [255, 0, 0, 255]);
        assert_eq!(img.transparent, [0]);
        assert_eq!(img.index_texels(), [1, 0, 0, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn resolve_uniform_layout_matches_wgsl() {
        assert_eq!(size_of::<ResolveUniform>(), 16 + 16 * MAX_PALETTE_CYCLES);
    }
}
