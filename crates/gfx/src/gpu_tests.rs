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

/// A `w x h` texture in the canvas format filled with `rgba` bytes.
fn texture_with(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    size: [u32; 2],
    rgba: &[u8],
) -> wgpu::TextureView {
    use wgpu::util::DeviceExt as _;
    device
        .create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            rgba,
        )
        .create_view(&Default::default())
}

#[cfg(feature = "crt")]
#[test]
fn crt_index_mask_blooms_only_emissive_indices() {
    use crate::canvas::CanvasRect;
    use crate::crt::{BloomSource, CrtEffect, CrtMask, CrtSettings};
    use crate::post::{PostEffect, PostPass, PostStage};

    let Some((device, queue)) = device() else {
        return;
    };
    const LAVA: u8 = 2;
    const SNOW: u8 = 3;
    let size = [16u32, 16];
    let (lava_px, snow_px) = ([4u32, 8], [12u32, 8]);
    let at = |p: [u32; 2]| ((p[1] * size[0] + p[0]) * 4) as usize;
    // The resolved colours: black, with one white lava and one white snow pixel.
    let mut colors = vec![0u8; (size[0] * size[1] * 4) as usize];
    let mut indices = colors.clone();
    for px in colors.chunks_mut(4).chain(indices.chunks_mut(4)) {
        px[3] = 255;
    }
    for (p, index) in [(lava_px, LAVA), (snow_px, SNOW)] {
        colors[at(p)..at(p) + 3].copy_from_slice(&[255, 255, 255]);
        indices[at(p)] = index; // stored byte = index, as `index_out` writes it
    }
    let input = texture_with(&device, &queue, size, &colors);
    let index_view = texture_with(&device, &queue, size, &indices);
    let (out, out_view) = target(&device, size);

    let mut crt = CrtEffect::with_device(
        &device,
        FORMAT,
        CrtSettings {
            scanlines: 0.0,
            mask: CrtMask::None,
            mask_strength: 0.0,
            bloom: 1.0,
            bloom_source: BloomSource::indices([LAVA]),
            persistence: 0.0,
            curvature: 0.0,
            vignette: 0.0,
            brightness: 1.0,
            ..CrtSettings::arcade()
        },
    );
    let textures = TextureRegistry::new(&device, &queue);
    let mut encoder = device.create_command_encoder(&Default::default());
    let rect = CanvasRect {
        origin: [0.0, 0.0],
        size: [size[0] as f32, size[1] as f32],
    };
    let mut globals = crate::post::globals(size, size, rect, 0.0, 0);
    globals.size = [size[0] as f32, size[1] as f32];
    crt.apply(&mut PostPass {
        device: &device,
        queue: &queue,
        encoder: &mut encoder,
        input: &input,
        output: &out_view,
        history: None,
        indices: Some(&index_view),
        format: FORMAT,
        stage: PostStage::Window,
        size,
        globals,
        textures: &textures,
    });
    let pixels = read_back(&device, &queue, encoder, &out, size);
    let beside = |p: [u32; 2]| pixels[at([p[0] + 1, p[1]])];
    assert!(beside(lava_px) > 20, "lava glows: {}", beside(lava_px));
    assert_eq!(beside(snow_px), 0, "snow doesn't glow");
}

/// A `ShaderEffect` with a source texture reads it as `source_tex`, sees `post.source == 1u`,
/// and follows `write_rgba` updates to that texture; without one, `source_tex` is the input.
#[test]
fn shader_effect_reads_its_source_texture() {
    use crate::canvas::CanvasRect;
    use crate::post::{PostEffect, PostPass, PostStage, ShaderEffect};

    let Some((device, queue)) = device() else {
        return;
    };
    let size = [4u32, 4];
    // The scene: opaque blue. The game's source: a 2 x 2 texture, first red then green.
    let blue: Vec<u8> = [0u8, 0, 255, 255].repeat((size[0] * size[1]) as usize);
    let input = texture_with(&device, &queue, size, &blue);
    let mut textures = TextureRegistry::new(&device, &queue);
    let red = [255u8, 0, 0, 255].repeat(4);
    let source = textures.create_from_rgba(&device, &queue, 2, 2, &red, Some("source"));
    let mut effect = ShaderEffect::build(
        &device,
        FORMAT,
        "source",
        "@fragment fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
            if (post.source == 1u) {
                return textureSample(source_tex, nearest_sampler, in.uv);
            }
            return textureSample(input_tex, nearest_sampler, in.uv);
        }",
        None,
    )
    .with_source(source);

    let run = |effect: &mut ShaderEffect, textures: &TextureRegistry| {
        let (out, out_view) = target(&device, size);
        let mut encoder = device.create_command_encoder(&Default::default());
        let rect = CanvasRect {
            origin: [0.0, 0.0],
            size: [size[0] as f32, size[1] as f32],
        };
        let mut globals = crate::post::globals(size, size, rect, 0.0, 0);
        globals.size = [size[0] as f32, size[1] as f32];
        effect.apply(&mut PostPass {
            device: &device,
            queue: &queue,
            encoder: &mut encoder,
            input: &input,
            output: &out_view,
            history: None,
            indices: None,
            format: FORMAT,
            stage: PostStage::Canvas,
            size,
            globals,
            textures,
        });
        let pixels = read_back(&device, &queue, encoder, &out, size);
        [pixels[0], pixels[1], pixels[2]]
    };

    assert_eq!(run(&mut effect, &textures), [255, 0, 0]);
    // Updated in place, the way a game uploads a new frame each frame.
    let green = [0u8, 255, 0, 255].repeat(4);
    assert!(textures.write_rgba(&queue, source, &green));
    assert_eq!(run(&mut effect, &textures), [0, 255, 0]);
    effect.set_source(None);
    assert_eq!(run(&mut effect, &textures), [0, 0, 255]);
}

/// A `CrtEffect` with a [`CrtSource`] shows that texture inside its rectangle, with the beam
/// model's gaps between its lines (not the canvas rows), and passes the input through outside
/// it; monochrome shows only the phosphor colour and rounded corners cut the rectangle's
/// corners.
#[cfg(feature = "crt")]
#[test]
fn crt_source_is_filtered_into_its_rectangle() {
    use crate::canvas::CanvasRect;
    use crate::crt::{CrtBeam, CrtEffect, CrtMask, CrtSettings, CrtSource};
    use crate::post::{PostEffect, PostPass, PostStage};

    let Some((device, queue)) = device() else {
        return;
    };
    let size = [32u32, 32];
    let blue: Vec<u8> = [0u8, 0, 255, 255].repeat((size[0] * size[1]) as usize);
    let input = texture_with(&device, &queue, size, &blue);
    let mut textures = TextureRegistry::new(&device, &queue);
    let white = [255u8; 4].repeat(16);
    let screen = textures.create_from_rgba(&device, &queue, 4, 4, &white, Some("screen"));
    // 4 x 4 source pixels into 16 x 16 output pixels at (8, 8): 4 pixels per line.
    let source = CrtSource {
        texture: screen,
        rect: [8.0, 8.0, 16.0, 16.0],
        region: [0.0, 0.0, 4.0, 4.0],
    };
    let settings = CrtSettings {
        scanlines: 1.0,
        mask: CrtMask::None,
        bloom: 0.0,
        persistence: 0.0,
        curvature: 0.0,
        vignette: 0.0,
        brightness: 1.0,
        beam: Some(CrtBeam {
            min: 0.2,
            max: 0.2,
            blur: 0.3,
        }),
        ..CrtSettings::arcade()
    };
    let run = |settings: CrtSettings| {
        let mut crt = CrtEffect::with_device(&device, FORMAT, settings).with_source(source);
        let (out, out_view) = target(&device, size);
        let mut encoder = device.create_command_encoder(&Default::default());
        let rect = CanvasRect {
            origin: [0.0, 0.0],
            size: [size[0] as f32, size[1] as f32],
        };
        let mut globals = crate::post::globals(size, size, rect, 0.0, 0);
        globals.size = [size[0] as f32, size[1] as f32];
        crt.apply(&mut PostPass {
            device: &device,
            queue: &queue,
            encoder: &mut encoder,
            input: &input,
            output: &out_view,
            history: None,
            indices: None,
            format: FORMAT,
            stage: PostStage::Window,
            size,
            globals,
            textures: &textures,
        });
        let pixels = read_back(&device, &queue, encoder, &out, size);
        move |x: u32, y: u32| {
            let i = ((y * size[0] + x) * 4) as usize;
            [pixels[i], pixels[i + 1], pixels[i + 2]]
        }
    };

    let px = run(settings);
    assert_eq!(px(2, 2), [0, 0, 255], "outside the rectangle: the input");
    // Source line 1 is centred on output row 8 + 6 = 14; row 12 is near the gap above it.
    let (line, gap) = (px(16, 14)[0], px(16, 12)[0]);
    assert!(line > 240 && gap < 200, "line {line}, gap {gap}");
    assert_eq!(px(16, 14)[1], line, "white stays white");

    let px = run(CrtSettings {
        monochrome: Some(Color::rgb(0.0, 1.0, 0.0)),
        corner_radius: 0.3,
        ..settings
    });
    let [r, g, b] = px(16, 14);
    assert!(r == 0 && b == 0 && g > 200, "green phosphor: {r} {g} {b}");
    assert_eq!(px(8, 8), [0, 0, 0], "the corner is cut off");
}
