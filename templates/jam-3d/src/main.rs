//! {{project-name}} - a 3D game-toolkit jam starter: one depth-tested spinning cube with a
//! 2D HUD on top. Window opens, Esc quits.

use toolkit_prelude::*;

struct Game1 {
    cube: MeshId,
}

/// A unit cube centered at the origin, as 24 vertices with per-face normals.
fn unit_cube() -> (Vec<MeshVertex>, Vec<u16>) {
    let h = 0.5;
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

impl Game for Game1 {
    fn init(ctx: &mut Context) -> Result<Self> {
        let (vertices, indices) = unit_cube();
        let cube = ctx.gfx.create_mesh(&vertices, &indices);
        ctx.gfx.camera3d.eye = [2.5, 2.0, 3.5];
        Ok(Self { cube })
    }

    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let t = ctx.time.elapsed.as_secs_f32();
        let model = transform::mul(&transform::rotation_y(t), &transform::rotation_x(t * 0.5));

        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.05, 0.06, 0.09, 1.0]);
        p.mesh(self.cube, model, [0.45, 0.75, 0.95, 1.0]);
        p.text([16.0, 16.0], "{{project-name}} (Esc to quit)", 22.0, [0.9, 0.9, 0.95, 1.0]);
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<Game1>(AppConfig {
        title: "{{project-name}}".into(),
        width: 1280,
        height: 720,
        // 3D needs a depth buffer; MSAA smooths the cube edges.
        depth_format: Some(wgpu::TextureFormat::Depth32Float),
        msaa_samples: 4,
        asset_root: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        ..Default::default()
    })
}
