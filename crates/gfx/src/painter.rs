use crate::frame::Frame;
use crate::graphics::Graphics;
use crate::mesh::{MeshId, MeshInstance};
use crate::primitives::CircleInstance;
use crate::sprite::{BlendMode, SpriteInstance};
use crate::texture::TextureId;
use crate::transform::Mat4;

pub struct Painter<'a> {
    frame: &'a mut Frame,
    gfx: &'a mut Graphics,
}

impl<'a> Painter<'a> {
    pub(crate) fn new(frame: &'a mut Frame, gfx: &'a mut Graphics) -> Self {
        Self { frame, gfx }
    }

    pub fn clear(&mut self, color: [f32; 4]) {
        self.frame.clear_color = color;
    }

    /// World-space rectangle currently visible through the 2D camera, as `(min, max)`.
    pub fn visible_rect(&self) -> ([f32; 2], [f32; 2]) {
        self.gfx.camera.visible_rect()
    }

    pub fn sprite(&mut self, tex: TextureId, pos: [f32; 2], size: [f32; 2]) {
        self.gfx
            .sprites
            .draw(tex, 0, BlendMode::Alpha, SpriteInstance::at(pos, size));
    }

    pub fn sprite_ex(
        &mut self,
        tex: TextureId,
        inst: SpriteInstance,
        layer: i16,
        blend: BlendMode,
    ) {
        self.gfx.sprites.draw(tex, layer, blend, inst);
    }

    /// Filled rectangle (uses the internal 1×1 white texture).
    pub fn rect(&mut self, pos: [f32; 2], size: [f32; 2], color: [f32; 4]) {
        let tex = self.gfx.white_texture();
        self.gfx.sprites.draw(
            tex,
            0,
            BlendMode::Alpha,
            SpriteInstance::at(pos, size).with_color(color),
        );
    }

    /// 4 thin lines around the rectangle.
    pub fn rect_outline(&mut self, pos: [f32; 2], size: [f32; 2], thickness: f32, color: [f32; 4]) {
        let [x, y] = pos;
        let [w, h] = size;
        self.line([x, y], [x + w, y], thickness, color);
        self.line([x + w, y], [x + w, y + h], thickness, color);
        self.line([x + w, y + h], [x, y + h], thickness, color);
        self.line([x, y + h], [x, y], thickness, color);
    }

    /// Line segment, drawn as a rotated thin quad through the sprite batcher.
    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], thickness: f32, color: [f32; 4]) {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-4 {
            return;
        }
        let angle = dy.atan2(dx);
        let half_t = thickness * 0.5;
        // SpriteInstance positions its quad by top-left; we want the quad centered on the
        // midpoint with width=len, height=thickness and rotation=angle. The shader rotates
        // around the quad's own center, so set `pos` so center lands on the midpoint.
        let cx = (a[0] + b[0]) * 0.5;
        let cy = (a[1] + b[1]) * 0.5;
        let tex = self.gfx.white_texture();
        let inst = SpriteInstance {
            pos: [cx - len * 0.5, cy - half_t],
            size: [len, thickness],
            uv_min: [0.0, 0.0],
            uv_max: [1.0, 1.0],
            color,
            rotation: angle,
            _pad: [0.0; 3],
        };
        self.gfx.sprites.draw(tex, 0, BlendMode::Alpha, inst);
    }

    /// Filled disk (layer 0).
    pub fn circle(&mut self, center: [f32; 2], radius: f32, color: [f32; 4]) {
        self.circle_ex(center, radius, 0.0, color, 0);
    }

    /// Ring (outline circle) with the given pixel thickness (layer 0).
    pub fn circle_outline(
        &mut self,
        center: [f32; 2],
        radius: f32,
        thickness: f32,
        color: [f32; 4],
    ) {
        self.circle_ex(center, radius, thickness.max(1.0), color, 0);
    }

    /// Circle on an explicit layer. `thickness == 0.0` is a filled disk; `> 0.0` is a ring of
    /// that pixel thickness. Lower layers draw under higher ones across all batchers.
    pub fn circle_ex(
        &mut self,
        center: [f32; 2],
        radius: f32,
        thickness: f32,
        color: [f32; 4],
        layer: i16,
    ) {
        self.gfx.primitives.push(
            layer,
            CircleInstance {
                center,
                radius,
                thickness,
                color,
            },
        );
    }

    /// Draw text. Text always renders on top of sprites and circles (it is not layered).
    pub fn text(&mut self, pos: [f32; 2], s: &str, size_px: f32, color: [f32; 4]) {
        self.gfx.text.queue(s, pos, size_px, color);
    }

    /// Draw an instance of a 3D `mesh` with the given column-major `model` transform and
    /// tint. Meshes are depth-tested against [`crate::Graphics::camera3d`] and render under
    /// all 2D content.
    pub fn mesh(&mut self, mesh: MeshId, model: Mat4, color: [f32; 4]) {
        self.gfx.draw_mesh(mesh, MeshInstance::new(model, color));
    }

    /// Draw vector graphics with vello (feature `vector`). The closure receives the frame's
    /// [`vello::Scene`]; vector content composites on top of all other layers.
    #[cfg(feature = "vector")]
    pub fn vector<F: FnOnce(&mut vello::Scene)>(&mut self, f: F) {
        f(self.gfx.vector.scene());
    }
}

impl<'a> Drop for Painter<'a> {
    fn drop(&mut self) {
        self.gfx.flush_into(self.frame);
    }
}
