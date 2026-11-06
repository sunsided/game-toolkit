//! Tests that render on a real GPU adapter, without a window. They skip (pass with a note)
//! when the machine has no adapter, e.g. a CI runner without a software rasterizer.

use wgpu::util::DeviceExt;

use crate::camera::{Camera2D, CameraUniform};
use crate::color::Color;
use crate::sprite::{BlendMode, SpriteBatcher, SpriteInstance};
use crate::target::Targets;
use crate::texture::TextureRegistry;

/// The canvas format on every platform the toolkit renders to (an sRGB colour target).
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))
    .ok();
    let Some(adapter) = adapter else {
        eprintln!("no GPU adapter; skipping");
        return None;
    };
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("test.device"),
        required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
        ..Default::default()
    }))
    .ok()
}

fn target(device: &wgpu::Device, size: [u32; 2]) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test.target"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    (tex, view)
}

/// Copy `tex` (`size`, 4 bytes per pixel) back to the CPU, tightly packed.
fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    mut encoder: wgpu::CommandEncoder,
    tex: &wgpu::Texture,
    size: [u32; 2],
) -> Vec<u8> {
    let row = (size[0] * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("test.readback"),
        size: u64::from(row * size[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        tex.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(size[1]),
            },
        },
        wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    buf.slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");
    let data = buf.slice(..).get_mapped_range().expect("mapped range");
    data.chunks(row as usize)
        .flat_map(|r| &r[..(size[0] * 4) as usize])
        .copied()
        .collect()
}

/// A full-screen pass running `fs_main` from `source` (after `index.wgsl`) into `view`,
/// with `bind` optionally bound at group 0.
fn full_screen(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    source: &str,
    bind: Option<(&wgpu::BindGroupLayout, &wgpu::BindGroup)>,
) {
    const VS: &str = "
        @vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
            let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
            return vec4<f32>(p * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
        }";
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("test.shader"),
        source: wgpu::ShaderSource::Wgsl(
            format!("{}\n{VS}\n{source}", include_str!("index.wgsl")).into(),
        ),
    });
    let layouts: Vec<Option<&wgpu::BindGroupLayout>> = bind.iter().map(|(l, _)| Some(*l)).collect();
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &layouts,
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("test.pipeline"),
        layout: Some(&layout),
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
            targets: &[Some(FORMAT.into())],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("test.pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    pass.set_pipeline(&pipeline);
    if let Some((_, bg)) = bind {
        pass.set_bind_group(0, bg, &[]);
    }
    pass.draw(0..3, 0..1);
}

#[test]
fn index_out_stores_every_index_exactly_and_index_in_reads_it_back() {
    let Some((device, queue)) = device() else {
        return;
    };
    let size = [256, 1];
    let (written, written_view) = target(&device, size);
    let (copied, copied_view) = target(&device, size);
    let mut encoder = device.create_command_encoder(&Default::default());
    full_screen(
        &device,
        &mut encoder,
        &written_view,
        "@fragment fn fs_main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
            return index_out(u32(p.x));
        }",
        None,
    );
    // Read the indices back with `index_in` and write them again: a lossless round trip.
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&written_view),
        }],
    });
    full_screen(
        &device,
        &mut encoder,
        &copied_view,
        "@group(0) @binding(0) var src: texture_2d<f32>;
        @fragment fn fs_main(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
            return index_out(index_in(textureLoad(src, vec2<i32>(p.xy), 0)));
        }",
        Some((&bgl, &bg)),
    );
    let pixels = read_back(&device, &queue, encoder, &written, size);
    let red: Vec<u8> = pixels.chunks(4).map(|p| p[0]).collect();
    assert_eq!(red, (0..=255).collect::<Vec<u8>>());

    let encoder = device.create_command_encoder(&Default::default());
    let pixels = read_back(&device, &queue, encoder, &copied, size);
    let red: Vec<u8> = pixels.chunks(4).map(|p| p[0]).collect();
    assert_eq!(red, (0..=255).collect::<Vec<u8>>());
}

#[test]
fn bitmap_text_tinted_with_an_index_writes_exactly_that_index() {
    let Some((device, queue)) = device() else {
        return;
    };
    let (sheet_size, sheet) = crate::bitmap_font::builtin_sheet();
    let size = sheet_size;
    let camera = Camera2D::new(size[0] as f32, size[1] as f32);
    let camera_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::bytes_of(&CameraUniform {
            view_proj: camera.view_proj(),
        }),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
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
        label: None,
        layout: &camera_bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: camera_buf.as_entire_binding(),
        }],
    });
    let mut textures = TextureRegistry::new(&device, &queue);
    let font = textures.create_from_rgba(&device, &queue, size[0], size[1], &sheet, None);
    let mut sprites = SpriteBatcher::new(&device, FORMAT, &camera_bgl, &textures.layout, 1, None);

    const BACKGROUND: u8 = 9;
    for index in [1u8, 77, 128, 200, 255] {
        let (tex, view) = target(&device, size);
        let mut encoder = device.create_command_encoder(&Default::default());
        let targets = Targets {
            color: &view,
            resolve: None,
            depth: None,
        };
        // Clear to the background index the way the painter does (`Color::index` → linear).
        let bg = Color::index(BACKGROUND);
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(targets.color_attachment(wgpu::LoadOp::Clear(
                wgpu::Color {
                    r: crate::graphics::srgb_to_linear(bg.r),
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            )))],
            ..Default::default()
        }));
        // The whole glyph sheet as one sprite, tinted like `Painter::bitmap_text` tints.
        sprites.draw(
            font,
            0,
            BlendMode::Alpha,
            SpriteInstance::at([0.0, 0.0], [size[0] as f32, size[1] as f32])
                .with_color(Color::index(index)),
        );
        sprites.upload(&device, &queue);
        sprites.draw_layer(0, &mut encoder, &targets, &camera_bg, &textures);
        sprites.clear();

        let pixels = read_back(&device, &queue, encoder, &tex, size);
        for (texel, out) in sheet.chunks(4).zip(pixels.chunks(4)) {
            let want = if texel[3] == 255 { index } else { BACKGROUND };
            assert_eq!(out[0], want, "index {index}");
        }
    }
}
