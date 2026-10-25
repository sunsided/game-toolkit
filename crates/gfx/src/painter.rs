use crate::frame::Frame;
use crate::graphics::Graphics;
use crate::mesh::{MeshId, MeshInstance};
use crate::primitives::CircleInstance;
use crate::shapes::Colors;
use crate::sprite::{BlendMode, SpriteInstance};
use crate::texture::TextureId;
use crate::transform::Mat4;

/// Immediate-mode 2D drawing for one frame; everything queued is flushed when it drops.
///
/// # Draw order
///
/// 2D content is sorted by layer: lower layers draw under higher ones, across sprites,
/// shapes and circles. Within a layer, sprites and sprite-based shapes (`sprite*`, `rect*`,
/// `line*`) draw first, grouped by [`BlendMode`] and then by texture; call order is kept only
/// between those draws that share layer, blend mode and texture. Filled shapes (`quad`,
/// `polygon`, `triangles`) draw next, in call order; circles draw on top.
///
/// Untextured shapes (`rect*`, `line*`) use the internal white texture, which sorts before
/// every loaded texture. So on the same layer and blend mode, a `rect` lands *under* a
/// textured sprite even when it is drawn after it; put it on a higher layer with
/// [`Painter::rect_ex`] to draw it on top. Text is not layered and always draws last.
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

    /// Sprite covering the whole texture (layer 0, alpha blending).
    pub fn sprite(&mut self, tex: TextureId, pos: [f32; 2], size: [f32; 2]) {
        self.gfx
            .sprites
            .draw(tex, 0, BlendMode::Alpha, SpriteInstance::at(pos, size));
    }

    /// Draw the `src_size` pixel rectangle at `src_pos` (top-left, in pixels) of `tex`, e.g. one
    /// sprite of an atlas, into `pos`/`size` (layer 0, alpha blending). Does nothing for an
    /// unknown texture. For flipping or an explicit layer, build the instance with
    /// [`SpriteInstance::with_region`] and draw it with [`Painter::sprite_ex`].
    pub fn sprite_region(
        &mut self,
        tex: TextureId,
        src_pos: [f32; 2],
        src_size: [f32; 2],
        pos: [f32; 2],
        size: [f32; 2],
    ) {
        if let Some(tex_size) = self.gfx.texture_size(tex) {
            let inst = SpriteInstance::at(pos, size).with_region(src_pos, src_size, tex_size);
            self.gfx.sprites.draw(tex, 0, BlendMode::Alpha, inst);
        }
    }

    /// Pixel size of a loaded texture; see [`Graphics::texture_size`].
    pub fn texture_size(&self, tex: TextureId) -> Option<[u32; 2]> {
        self.gfx.texture_size(tex)
    }

    /// Sprite on an explicit layer with an explicit blend mode. See the draw-order notes on
    /// [`Painter`].
    pub fn sprite_ex(
        &mut self,
        tex: TextureId,
        inst: SpriteInstance,
        layer: i16,
        blend: BlendMode,
    ) {
        self.gfx.sprites.draw(tex, layer, blend, inst);
    }

    /// Filled rectangle (layer 0, alpha blending; uses the internal 1×1 white texture).
    pub fn rect(&mut self, pos: [f32; 2], size: [f32; 2], color: [f32; 4]) {
        self.rect_ex(pos, size, color, 0, BlendMode::Alpha);
    }

    /// Filled rectangle on an explicit layer with an explicit blend mode.
    pub fn rect_ex(
        &mut self,
        pos: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        layer: i16,
        blend: BlendMode,
    ) {
        let tex = self.gfx.white_texture();
        self.gfx.sprites.draw(
            tex,
            layer,
            blend,
            SpriteInstance::at(pos, size).with_color(color),
        );
    }

    /// 4 thin lines around the rectangle (layer 0, alpha blending).
    pub fn rect_outline(&mut self, pos: [f32; 2], size: [f32; 2], thickness: f32, color: [f32; 4]) {
        self.rect_outline_ex(pos, size, thickness, color, 0, BlendMode::Alpha);
    }

    /// Rectangle outline on an explicit layer with an explicit blend mode.
    pub fn rect_outline_ex(
        &mut self,
        pos: [f32; 2],
        size: [f32; 2],
        thickness: f32,
        color: [f32; 4],
        layer: i16,
        blend: BlendMode,
    ) {
        let [x, y] = pos;
        let [w, h] = size;
        self.line_ex([x, y], [x + w, y], thickness, color, layer, blend);
        self.line_ex([x + w, y], [x + w, y + h], thickness, color, layer, blend);
        self.line_ex([x + w, y + h], [x, y + h], thickness, color, layer, blend);
        self.line_ex([x, y + h], [x, y], thickness, color, layer, blend);
    }

    /// Line segment, drawn as a rotated thin quad through the sprite batcher (layer 0, alpha
    /// blending).
    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], thickness: f32, color: [f32; 4]) {
        self.line_ex(a, b, thickness, color, 0, BlendMode::Alpha);
    }

    /// Line segment on an explicit layer with an explicit blend mode.
    pub fn line_ex(
        &mut self,
        a: [f32; 2],
        b: [f32; 2],
        thickness: f32,
        color: [f32; 4],
        layer: i16,
        blend: BlendMode,
    ) {
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
        self.gfx.sprites.draw(tex, layer, blend, inst);
    }

    /// Filled convex quad through `points` (in order, either winding), alpha-blended. Lands
    /// above this layer's sprites and rects and below its circles; see [`Painter`].
    pub fn quad(&mut self, points: [[f32; 2]; 4], color: [f32; 4], layer: i16) {
        self.gfx
            .shapes
            .polygon(layer, &points, Colors::Uniform(color));
    }

    /// [`Painter::quad`] with one colour per corner, interpolated across the quad (e.g. a
    /// vertical gradient: the same colour on both top corners, another on both bottom ones).
    pub fn quad_colored(&mut self, points: [[f32; 2]; 4], colors: [[f32; 4]; 4], layer: i16) {
        self.gfx
            .shapes
            .polygon(layer, &points, Colors::PerVertex(&colors));
    }

    /// Filled convex polygon through `points` (in order, either winding), fan-triangulated
    /// from the first point and alpha-blended. Fewer than three points draw nothing. A
    /// concave polygon would draw wrong; split it or use [`Painter::triangles`].
    pub fn polygon(&mut self, points: &[[f32; 2]], color: [f32; 4], layer: i16) {
        self.gfx
            .shapes
            .polygon(layer, points, Colors::Uniform(color));
    }

    /// [`Painter::polygon`] with one colour per point, interpolated across the polygon.
    /// Draws nothing unless `colors.len() == points.len()` (a debug-build panic).
    pub fn polygon_colored(&mut self, points: &[[f32; 2]], colors: &[[f32; 4]], layer: i16) {
        self.gfx
            .shapes
            .polygon(layer, points, Colors::PerVertex(colors));
    }

    /// Filled triangle list: every three `indices` into `vertices` make one triangle,
    /// alpha-blended. Draws nothing if an index is out of range or `indices.len()` is not a
    /// multiple of three (a debug-build panic; release builds log one warning).
    pub fn triangles(
        &mut self,
        vertices: &[[f32; 2]],
        indices: &[u32],
        color: [f32; 4],
        layer: i16,
    ) {
        self.gfx
            .shapes
            .triangles(layer, vertices, Colors::Uniform(color), indices);
    }

    /// [`Painter::triangles`] with one colour per vertex, interpolated across each triangle.
    /// Also draws nothing unless `colors.len() == vertices.len()` (a debug-build panic).
    pub fn triangles_colored(
        &mut self,
        vertices: &[[f32; 2]],
        colors: &[[f32; 4]],
        indices: &[u32],
        layer: i16,
    ) {
        self.gfx
            .shapes
            .triangles(layer, vertices, Colors::PerVertex(colors), indices);
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
    /// [`vello::Scene`]; vector content composites on top of all other layers. The closure is
    /// not called on adapters without compute shaders (WebGL2).
    #[cfg(feature = "vector")]
    pub fn vector<F: FnOnce(&mut vello::Scene)>(&mut self, f: F) {
        if let Some(vector) = self.gfx.vector.as_mut() {
            f(vector.scene());
        }
    }
}

impl<'a> Drop for Painter<'a> {
    fn drop(&mut self) {
        self.gfx.flush_into(self.frame);
    }
}
