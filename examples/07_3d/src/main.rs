//! Three depth-tested cubes spinning under a perspective camera, with a 2D HUD on top.
//!
//! Shows the 3D path: upload a mesh once with `create_mesh`, then `painter.mesh(id, model,
//! color)` per instance. The mesh pass is depth-tested against `gfx.camera3d`; the 2D text
//! composites over it.

use game_toolkit_prelude::*;

struct Cubes {
    cube: MeshId,
}

/// A unit cube (centered at the origin) as 24 vertices with per-face normals.
fn unit_cube() -> (Vec<MeshVertex>, Vec<u16>) {
    let h = 0.5;
    // (normal, four corners in a loop) for each of the six faces.
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([1.0, 0.0, 0.0], [[h, -h, -h], [h, h, -h], [h, h, h], [h, -h, h]]),
        ([-1.0, 0.0, 0.0], [[-h, -h, h], [-h, h, h], [-h, h, -h], [-h, -h, -h]]),
        ([0.0, 1.0, 0.0], [[-h, h, -h], [-h, h, h], [h, h, h], [h, h, -h]]),
        ([0.0, -1.0, 0.0], [[-h, -h, h], [-h, -h, -h], [h, -h, -h], [h, -h, h]]),
        ([0.0, 0.0, 1.0], [[-h, -h, h], [h, -h, h], [h, h, h], [-h, h, h]]),
        ([0.0, 0.0, -1.0], [[h, -h, -h], [-h, -h, -h], [-h, h, -h], [h, h, -h]]),
    ];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, corners) in faces {
        let base = vertices.len() as u16;
        for c in corners {
            vertices.push(MeshVertex::new(c, normal));
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (vertices, indices)
}

impl Game for Cubes {
    fn init(ctx: &mut Context) -> Result<Self> {
        let (vertices, indices) = unit_cube();
        let cube = ctx.gfx.create_mesh(&vertices, &indices);
        // Look slightly down at the row of cubes.
        ctx.gfx.camera3d.eye = [0.0, 1.6, 6.0];
        ctx.gfx.camera3d.target = [0.0, 0.0, 0.0];
        Ok(Self { cube })
    }

    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let t = ctx.time.elapsed.as_secs_f32();
        let cubes = [
            (-2.4_f32, [0.90, 0.35, 0.35, 1.0], 1.0_f32),
            (0.0, [0.40, 0.80, 0.45, 1.0], -1.3),
            (2.4, [0.40, 0.55, 0.95, 1.0], 0.7),
        ];

        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.04, 0.05, 0.08, 1.0]);

        for (x, color, speed) in cubes {
            let spin = transform::mul(
                &transform::rotation_y(t * speed),
                &transform::rotation_x(t * 0.6),
            );
            let model = transform::mul(&transform::translation([x, 0.0, 0.0]), &spin);
            p.mesh(self.cube, model, color);
        }

        p.text(
            [16.0, 16.0],
            "07_3d: depth-tested cubes, 2D HUD on top (Esc to quit)",
            22.0,
            [0.9, 0.9, 0.95, 1.0],
        );
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<Cubes>(AppConfig {
        title: "07_3d".into(),
        width: 900,
        height: 600,
        // 3D needs a depth buffer; MSAA smooths the cube edges.
        depth_format: Some(wgpu::TextureFormat::Depth32Float),
        msaa_samples: 4,
        ..Default::default()
    })
}
