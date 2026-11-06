//! Indexed colour on a 160 × 120 canvas: everything is drawn with palette indices, a
//! waterfall and a sign animate by palette cycling, `Space` crossfades day into night by
//! blending two palette rows, and a canvas-stage effect shades the corners with ordered
//! (Bayer) dithering anchored to the scene. `D` toggles the dithering, `Esc` quits.
//!
//! The sky is not drawn by the painter: a custom pass (`Painter::custom`) on the bottom
//! layer runs a fragment shader that writes a dithered gradient of palette indices with
//! `index_out`, and the painter's hills, waterfall, text and sprite draw on top of it.

use game_toolkit::prelude::*;

const CANVAS: (u32, u32) = (160, 120);

// Palette layout (row 0 = day, row 1 = night).
const SKY: u8 = 1;
const HILL: u8 = 2;
const ROCK: u8 = 3;
const TEXT: u8 = 4;
const SPRITE_BODY: u8 = 5;
const SPRITE_EYE: u8 = 6;
/// The sky towards the top; the custom pass dithers from it down to `SKY`.
const SKY_HIGH: u8 = 7;
/// Eight blues for the waterfall, rotated by a palette cycle.
const WATER: u8 = 16;
/// Four sign colours, rotated faster the other way.
const SIGN: u8 = 32;

/// Darkens towards the corners in steps, dithered with `bayer4` (from the shader prelude),
/// anchored to canvas pixels so it never crawls.
const DITHER_VIGNETTE: &str = r"
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(input_tex, nearest_sampler, in.uv);
    let d = length(in.uv - vec2<f32>(0.5)) * 1.6;
    let shade = clamp((d - 0.45) * 2.0, 0.0, 1.0);
    let t = bayer4(vec2<i32>(in.pos.xy));
    let k = select(1.0, 0.45, shade > t);
    return vec4<f32>(c.rgb * k, c.a);
}
";

/// The sky: a vertical gradient from `SKY_HIGH` to `SKY`, two palette indices dithered with
/// `bayer4`. Prepended with `INDEX_WGSL` (for `index_out`) and `BAYER_WGSL`.
const SKY_SHADER: &str = r"
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let t = clamp(pos.y / 70.0, 0.0, 1.0);
    let lower = t > bayer4(vec2<i32>(pos.xy));
    return index_out(select(SKY_HIGH, SKY, lower));
}
";

/// A render pipeline for the sky pass, matching the painter's target.
fn sky_pipeline(gfx: &Graphics) -> wgpu::RenderPipeline {
    let device = gfx.device();
    let source = format!(
        "const SKY: u32 = {SKY}u;\nconst SKY_HIGH: u32 = {SKY_HIGH}u;\n{INDEX_WGSL}\n{BAYER_WGSL}\n{SKY_SHADER}"
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sky"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("sky"),
        layout: None,
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
                format: gfx.surface_format(),
                // No blending: indices must never be mixed.
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: gfx.depth_stencil(),
        multisample: gfx.multisample(),
        multiview_mask: None,
        cache: None,
    })
}

struct App {
    sky: wgpu::RenderPipeline,
    critter: TextureId,
    dither: PostEffectId,
    night: f32,
    night_target: f32,
}

fn palettes() -> (Palette, Palette) {
    let mut day = Palette::from_hex(&[
        0x000000, 0x8cc8f0, 0x4a9a3a, 0x7a6a5a, 0xffffff, 0xe0703a, 0x101010, 0x5a96d8,
    ]);
    let mut night = Palette::from_hex(&[
        0x000000, 0x101838, 0x18301c, 0x2a2630, 0xd0d0ff, 0x804028, 0x101010, 0x060a1c,
    ]);
    for i in 0..8u8 {
        let v = 120 + u32::from(i) * 16;
        day.set(WATER + i, [40, (v / 2) as u8, v.min(255) as u8, 255]);
        night.set(WATER + i, [10, (v / 5) as u8, (v / 2) as u8, 255]);
    }
    let sign = [
        [255, 60, 60, 255],
        [255, 200, 40, 255],
        [60, 220, 90, 255],
        [60, 120, 255, 255],
    ];
    for (i, c) in sign.iter().enumerate() {
        day.set(SIGN + i as u8, *c);
        night.set(SIGN + i as u8, *c);
    }
    (day, night)
}

/// An 8 × 8 critter as palette indices; 0 is transparent.
fn critter() -> IndexedImage {
    #[rustfmt::skip]
    let rows = [
        "..XXXX..",
        ".XXXXXX.",
        "XXeXXeXX",
        "XXXXXXXX",
        "XXXXXXXX",
        ".XXXXXX.",
        ".X.XX.X.",
        "X..XX..X",
    ];
    let indices = rows
        .iter()
        .flat_map(|r| r.bytes())
        .map(|b| match b {
            b'X' => SPRITE_BODY,
            b'e' => SPRITE_EYE,
            _ => 0,
        })
        .collect();
    IndexedImage {
        width: 8,
        height: 8,
        indices,
        palette: Palette::default(),
        transparent: vec![0],
    }
}

impl Game for App {
    fn init(ctx: &mut Context) -> Result<Self> {
        let (day, night) = palettes();
        ctx.gfx.enable_indexed(&day);
        ctx.gfx.set_palette(1, &night);
        ctx.gfx.add_palette_cycle(PaletteCycle {
            start: WATER,
            len: 8,
            speed: 10.0,
        });
        ctx.gfx.add_palette_cycle(PaletteCycle {
            start: SIGN,
            len: 4,
            speed: -3.0,
        });
        let critter = ctx.gfx.create_indexed_texture(&critter(), Some("critter"));
        let dither = ShaderEffect::new(&ctx.gfx, "dither_vignette", DITHER_VIGNETTE);
        let dither = ctx.gfx.add_post_effect(PostStage::Canvas, dither);
        Ok(Self {
            sky: sky_pipeline(&ctx.gfx),
            critter,
            dither,
            night: 0.0,
            night_target: 0.0,
        })
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        if ctx.input.key_pressed(Key::Space) {
            self.night_target = 1.0 - self.night_target;
        }
        if ctx.input.key_pressed(Key::KeyD) {
            let on = ctx.gfx.post_effect_enabled(self.dither).unwrap_or(false);
            ctx.gfx.set_post_effect_enabled(self.dither, !on);
        }
        self.night += (self.night_target - self.night).clamp(-dt, dt);
        ctx.gfx.set_palette_blend(0, 1, self.night);
        if ctx.input.key_pressed(Key::F12) {
            ctx.gfx.request_screenshot("screenshot.png");
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let (w, h) = ctx.gfx.size();
        let (w, h) = (w as f32, h as f32);
        let t = ctx.time.elapsed.as_secs_f32();
        let mut p = frame.painter(&mut ctx.gfx);

        // The sky, from our own shader, under everything the painter draws.
        let sky = self.sky.clone();
        p.custom(Layer::BOTTOM, move |pass| {
            let mut rp = pass.begin_render_pass("sky");
            rp.set_pipeline(&sky);
            rp.draw(0..3, 0..1);
        });

        // Hills and a cliff.
        p.rect([0.0, h - 30.0], [w, 30.0], Color::index(HILL));
        p.rect([90.0, 30.0], [40.0, h - 30.0], Color::index(ROCK));

        // Waterfall: diagonal stripes of the eight water indices; the cycle moves them.
        for y in 0..(h as i32 - 30) {
            for x in 0..16 {
                let i = ((x + 32 - (y % 32)) / 2 % 8) as u8;
                p.rect(
                    [100.0 + x as f32, 30.0 + y as f32],
                    [1.0, 1.0],
                    Color::index(WATER + i),
                );
            }
        }

        // A sign whose letters cycle through four colours.
        for (k, word) in ["P", "A", "L"].iter().enumerate() {
            p.pixel_text(
                [8.0 + k as f32 * 12.0, 8.0],
                word,
                2,
                Color::index(SIGN + k as u8),
            );
        }
        p.pixel_text([8.0, 28.0], "SPACE night", 1, Color::index(TEXT));

        // Indexed sprite, hopping.
        let hop = (t * 4.0).sin().abs() * 10.0;
        p.sprite(self.critter, [40.0, (h - 46.0 - hop).round()], [16.0, 16.0]);
    }
}

fn main() -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    env_logger::init();
    run::<App>(AppConfig {
        title: "19_palette".into(),
        width: 960,
        height: 720,
        virtual_resolution: Some(CANVAS),
        ..Default::default()
    })
}
