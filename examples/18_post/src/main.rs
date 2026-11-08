//! Post-processing: a CRT filter in the window stage and a custom WGSL effect in the canvas
//! stage, over a 320 × 180 scene.
//!
//! Keys: `C` toggles the CRT, `1`-`5` pick a preset (subtle, arcade, TV, PC monitor, green
//! screen with the beam model), `P`
//! toggles the canvas-stage posterize effect, `V` toggles the virtual resolution, `Esc` quits.

use game_toolkit::prelude::*;

const CANVAS: (u32, u32) = (320, 180);

/// Parameters of the posterize shader; mirrors `Params` in the WGSL below (16 bytes).
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Posterize {
    levels: f32,
    _pad: [f32; 3],
}

const POSTERIZE: &str = r"
struct Params { levels: f32 };
@group(1) @binding(0) var<uniform> params: Params;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(input_tex, nearest_sampler, in.uv);
    let q = floor(c.rgb * params.levels + 0.5) / params.levels;
    return vec4<f32>(q, c.a);
}
";

struct App {
    crt: PostEffectId,
    posterize: PostEffectId,
}

impl Game for App {
    fn init(ctx: &mut Context) -> Result<Self> {
        let crt = CrtEffect::new(&ctx.gfx, CrtSettings::arcade());
        let crt = ctx.gfx.add_post_effect(PostStage::Window, crt);
        let posterize = ShaderEffect::with_params(
            &ctx.gfx,
            "posterize",
            POSTERIZE,
            &Posterize {
                levels: 4.0,
                _pad: [0.0; 3],
            },
        );
        let posterize = ctx.gfx.add_post_effect(PostStage::Canvas, posterize);
        ctx.gfx.set_post_effect_enabled(posterize, false);
        Ok(Self { crt, posterize })
    }

    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        let toggle = |gfx: &mut Graphics, id| {
            let on = gfx.post_effect_enabled(id).unwrap_or(false);
            gfx.set_post_effect_enabled(id, !on);
        };
        if ctx.input.key_pressed(Key::KeyC) {
            toggle(&mut ctx.gfx, self.crt);
        }
        if ctx.input.key_pressed(Key::KeyP) {
            toggle(&mut ctx.gfx, self.posterize);
        }
        let preset = [
            (Key::Digit1, CrtSettings::subtle as fn() -> CrtSettings),
            (Key::Digit2, CrtSettings::arcade),
            (Key::Digit3, CrtSettings::consumer_tv),
            (Key::Digit4, CrtSettings::pc_monitor),
            (Key::Digit5, || {
                CrtSettings::monochrome_monitor(Color::hex(0x33ff66))
            }),
        ]
        .into_iter()
        .find(|(k, _)| ctx.input.key_pressed(*k));
        if let Some((_, make)) = preset
            && let Some(crt) = ctx.gfx.post_effect_mut::<CrtEffect>(self.crt)
        {
            crt.set_settings(make());
        }
        if ctx.input.key_pressed(Key::KeyV) {
            let next = match ctx.gfx.virtual_resolution() {
                Some(_) => None,
                None => Some(CANVAS),
            };
            ctx.gfx.set_virtual_resolution(next);
        }
        if ctx.input.key_pressed(Key::F12) {
            ctx.gfx.request_screenshot("screenshot.png");
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let (w, h) = ctx.gfx.size();
        let (w, h) = (w as f32, h as f32);
        let t = ctx.time.elapsed.as_secs_f32();
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.02, 0.02, 0.06, 1.0]);

        // Sky gradient bands (posterize turns them into hard steps).
        for i in 0..16 {
            let f = i as f32 / 15.0;
            p.rect(
                [0.0, i as f32 * h / 24.0],
                [w, h / 24.0 + 1.0],
                [0.15 + 0.5 * f, 0.1 + 0.2 * f, 0.35 - 0.2 * f, 1.0],
            );
        }
        // Sun and ground.
        p.circle([w * 0.5, h * 0.62], h * 0.18, [1.0, 0.75, 0.3, 1.0]);
        p.rect([0.0, h * 0.66], [w, h * 0.34], [0.05, 0.02, 0.1, 1.0]);
        for i in 0..12 {
            let x = (i as f32 / 11.0) * w;
            p.line(
                [w * 0.5, h * 0.66],
                [x * 2.0 - w * 0.5, h],
                1.0,
                [0.9, 0.2, 0.8, 1.0],
            );
        }
        // A fast bright dot to show phosphor afterglow.
        let x = w * 0.5 + (t * 3.0).cos() * w * 0.4;
        let y = h * 0.3 + (t * 5.0).sin() * h * 0.15;
        p.circle([x, y], 3.0, [0.6, 1.0, 0.9, 1.0]);

        p.text(
            [4.0, 4.0],
            "C crt  1-5 preset  P posterize  V canvas",
            10.0,
            Color::WHITE,
        );
    }
}

fn main() -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    env_logger::init();
    run::<App>(AppConfig {
        title: "18_post".into(),
        width: 1280,
        height: 720,
        virtual_resolution: Some(CANVAS),
        scale_mode: ScaleMode::Fit,
        ..Default::default()
    })
}
