use game_toolkit::prelude::*;

struct App {
    /// 8x8 checkerboard, to make texture mapping on the road quads visible.
    checker: TextureId,
}

impl Game for App {
    fn init(ctx: &mut Context) -> Result<Self> {
        let rgba: Vec<u8> = (0..64)
            .flat_map(|i| {
                let light = (i % 8 + i / 8) % 2 == 0;
                if light {
                    [235, 235, 235, 255]
                } else {
                    [60, 60, 70, 255]
                }
            })
            .collect();
        let checker = ctx.gfx.create_texture_rgba(8, 8, &rgba, Some("checker"));
        Ok(Self { checker })
    }
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
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
        p.clear([0.07, 0.08, 0.1, 1.0]);

        p.rect([40.0, 40.0], [120.0, 80.0], [0.9, 0.4, 0.3, 1.0]);
        p.rect_outline([200.0, 40.0], [120.0, 80.0], 3.0, [0.3, 0.9, 0.5, 1.0]);

        let cx = w * 0.5;
        let cy = h * 0.5;
        p.circle([cx, cy], 60.0 + 20.0 * t.sin(), [0.4, 0.7, 1.0, 0.85]);
        p.circle_outline([cx, cy], 110.0, 4.0, [1.0, 0.85, 0.2, 1.0]);

        // animated star of lines
        let n = 24;
        for i in 0..n {
            let a = (i as f32 / n as f32) * std::f32::consts::TAU + t * 0.4;
            let r0 = 30.0;
            let r1 = 150.0 + 30.0 * (t * 1.7 + i as f32).sin();
            let from = [cx + a.cos() * r0, cy + a.sin() * r0];
            let to = [cx + a.cos() * r1, cy + a.sin() * r1];
            p.line(from, to, 2.0, [1.0, 1.0, 1.0, 0.5]);
        }

        // Cross-batcher z-order (#14): sprites and circles interleave by layer, lower under
        // higher. A layer-0 sprite rect sits between a layer -1 circle (peeking out behind
        // it, top-left) and a layer +1 circle (in front of it, bottom-right).
        let bx = w - 150.0;
        let by = 95.0;
        p.circle_ex(
            [bx - 30.0, by - 30.0],
            34.0,
            0.0,
            [0.95, 0.30, 0.30, 1.0],
            -1,
        );
        p.rect(
            [bx - 42.0, by - 42.0],
            [84.0, 84.0],
            [0.25, 0.80, 0.45, 1.0],
        );
        p.circle_ex([bx + 34.0, by + 34.0], 28.0, 0.0, [0.35, 0.55, 1.0, 1.0], 1);
        p.text(
            [bx - 58.0, by + 62.0],
            "z-order",
            18.0,
            [1.0, 1.0, 1.0, 1.0],
        );

        // Filled shapes (#28): a road-like trapezoid strip, a hexagon and a raw triangle list.
        // They interleave by layer too: the trapezoids sit on layer 1, above the layer-0 rect
        // they overlap, with a layer-2 rect on top of them.
        let (rx, ry) = (40.0, h - 200.0);
        p.rect(
            [rx + 60.0, ry - 20.0],
            [60.0, 200.0],
            [0.25, 0.80, 0.45, 1.0],
        );
        let road = [[0.30, 0.30, 0.32, 1.0], [0.38, 0.38, 0.40, 1.0]];
        for row in 0..6 {
            let (y0, y1) = (ry + row as f32 * 28.0, ry + (row + 1) as f32 * 28.0);
            let (hw0, hw1) = (20.0 + row as f32 * 14.0, 20.0 + (row + 1) as f32 * 14.0);
            let cx = rx + 90.0;
            p.quad(
                [
                    [cx - hw0, y0],
                    [cx + hw0, y0],
                    [cx + hw1, y1],
                    [cx - hw1, y1],
                ],
                road[row % 2],
                1,
            );
        }
        p.rect_ex(
            [rx + 75.0, ry + 60.0],
            [30.0, 30.0],
            [0.95, 0.30, 0.30, 1.0],
            2,
            BlendMode::Alpha,
        );

        let (hx, hy, hr) = (w - 150.0, h - 120.0, 60.0);
        let hexagon: Vec<[f32; 2]> = (0..6)
            .map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 6.0 + t * 0.5;
                [hx + a.cos() * hr, hy + a.sin() * hr]
            })
            .collect();
        // Per-vertex colours (#33) are interpolated across the shape.
        let corner_colors = [
            [0.95, 0.30, 0.30, 0.9],
            [0.95, 0.85, 0.20, 0.9],
            [0.30, 0.85, 0.40, 0.9],
            [0.25, 0.80, 0.95, 0.9],
            [0.40, 0.40, 0.95, 0.9],
            [0.85, 0.35, 0.90, 0.9],
        ];
        p.polygon_colored(&hexagon, &corner_colors, 0);
        let (sky_top, sky_bottom) = ([0.10, 0.15, 0.40, 1.0], [0.95, 0.55, 0.30, 1.0]);
        p.quad_colored(
            [[40.0, 180.0], [200.0, 180.0], [200.0, 320.0], [40.0, 320.0]],
            [sky_top, sky_top, sky_bottom, sky_bottom],
            0,
        );

        let bowtie = [
            [cx - 60.0, 40.0],
            [cx - 60.0, 120.0],
            [cx, 80.0],
            [cx + 60.0, 40.0],
            [cx + 60.0, 120.0],
        ];
        p.triangles(&bowtie, &[0, 1, 2, 2, 3, 4], [1.0, 0.85, 0.2, 0.9], 0);

        // Blend modes for shapes (#32): two additive "light beams" brighten where they overlap.
        let beam = [1.0, 0.85, 0.5, 0.35];
        p.quad_ex(
            [
                [70.0, 180.0],
                [110.0, 180.0],
                [190.0, 320.0],
                [110.0, 320.0],
            ],
            beam,
            1,
            BlendMode::Additive,
        );
        p.quad_ex(
            [
                [130.0, 180.0],
                [170.0, 180.0],
                [130.0, 320.0],
                [50.0, 320.0],
            ],
            beam,
            1,
            BlendMode::Additive,
        );

        // Textured shapes (#34): the same road segment twice. `quad_textured` (left) maps the
        // checkerboard in perspective, so far rows shrink; `triangles_textured` (right) maps
        // it affinely, and the two triangles visibly shear along the diagonal.
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let road = |x: f32| {
            [
                [x + 30.0, 200.0],
                [x + 60.0, 200.0],
                [x + 90.0, 400.0],
                [x, 400.0],
            ]
        };
        p.quad_textured(
            self.checker,
            road(590.0),
            uvs,
            [1.0; 4],
            0,
            BlendMode::Alpha,
        );
        p.triangles_textured(
            self.checker,
            &road(695.0),
            &uvs,
            &[0, 1, 2, 0, 2, 3],
            [1.0; 4],
            0,
            BlendMode::Alpha,
        );
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<App>(AppConfig {
        title: "03_primitives".into(),
        width: 800,
        height: 600,
        // 4x MSAA smooths the SDF circle and thin-line edges; the depth buffer is allocated
        // but unused by the 2D pipelines (exercises the depth path without changing output).
        msaa_samples: 4,
        depth_format: Some(wgpu::TextureFormat::Depth32Float),
        ..Default::default()
    })
}
