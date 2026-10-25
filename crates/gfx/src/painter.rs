use crate::color::{Color, Layer};
use crate::draw::{RectDraw, SpriteDraw, TextDraw};
use crate::frame::Frame;
use crate::graphics::Graphics;
use crate::mesh::{MeshId, MeshInstance};
use crate::primitives::CircleInstance;
use crate::shapes::{Colors, ShapeKey, Uvs, quad_uvq};
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
/// between those draws that share layer, blend mode and texture. Filled shapes (`quad*`,
/// `polygon*`, `triangles*`) draw next, grouped the same way, in call order within a group
/// (so plain alpha-blended shapes on one layer always keep call order); circles draw on top.
///
/// Untextured shapes (`rect*`, `line*`) use the internal white texture, which sorts before
/// every loaded texture. So on the same layer and blend mode, a `rect` lands *under* a
/// textured sprite even when it is drawn after it; put it on a higher layer with
/// [`Painter::rect_ex`] to draw it on top. Text from [`Painter::text_ex`] draws last within its
/// layer; text from [`Painter::text`] draws on top of every layer.
///
/// # Arguments
///
/// Colours are `impl Into<Color>` and layers `impl Into<Layer>`, so `[r, g, b, a]` arrays and
/// plain integers work as well as [`Color`] and [`Layer`]. For draws with several optional
/// settings, [`Painter::draw_sprite`], [`Painter::draw_rect`] and [`Painter::draw_text`]
/// return builders instead of taking long positional argument lists.
pub struct Painter<'a> {
    frame: &'a mut Frame,
    gfx: &'a mut Graphics,
}

impl<'a> Painter<'a> {
    pub(crate) fn new(frame: &'a mut Frame, gfx: &'a mut Graphics) -> Self {
        Self { frame, gfx }
    }

    pub fn clear(&mut self, color: impl Into<Color>) {
        self.frame.clear_color = color.into().to_array();
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
    /// unknown texture. For flipping or an explicit layer, use [`Painter::draw_sprite`] with
    /// `.region(..)`, or build the instance with [`SpriteInstance::with_pixel_region`] and draw
    /// it with [`Painter::sprite_ex`].
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

    /// Start a sprite draw with optional settings: an atlas region in pixels, tint,
    /// rotation, flips, layer and blend mode. It is queued when the returned builder drops,
    /// usually at the end of the statement:
    ///
    /// ```ignore
    /// p.draw_sprite(atlas, pos, [32.0, 32.0])
    ///     .region([64.0, 0.0], [16.0, 16.0])
    ///     .layer(CARS)
    ///     .flip_x_if(facing_left);
    /// ```
    pub fn draw_sprite(
        &mut self,
        tex: TextureId,
        pos: [f32; 2],
        size: [f32; 2],
    ) -> SpriteDraw<'_, 'a> {
        SpriteDraw::new(self, tex, pos, size)
    }

    /// Start a rectangle draw with optional settings: colour, outline, layer and blend mode.
    /// Queued when the returned builder drops: `p.draw_rect(pos, size).color(c).layer(HUD);`.
    pub fn draw_rect(&mut self, pos: [f32; 2], size: [f32; 2]) -> RectDraw<'_, 'a> {
        RectDraw::new(self, pos, size)
    }

    /// Start a text draw with optional settings: size, colour, alignment and layer. Queued
    /// when the returned builder drops:
    /// `p.draw_text([w / 2.0, 20.0], "GAME OVER").size(48.0).align(Align::Center);`.
    pub fn draw_text<'s>(&mut self, pos: [f32; 2], s: &'s str) -> TextDraw<'_, 'a, 's> {
        TextDraw::new(self, pos, s)
    }

    /// Pixel size of a loaded texture; see [`Graphics::texture_size`].
    pub fn texture_size(&self, tex: TextureId) -> Option<[u32; 2]> {
        self.gfx.texture_size(tex)
    }

    /// Sprite on an explicit layer with an explicit blend mode. See the draw-order notes on
    /// [`Painter`]. An instance built with [`SpriteInstance::with_pixel_region`] gets its UVs
    /// from the texture's size here, and draws nothing for an unknown texture.
    pub fn sprite_ex(
        &mut self,
        tex: TextureId,
        inst: SpriteInstance,
        layer: impl Into<Layer>,
        blend: BlendMode,
    ) {
        let layer = layer.into().0;
        // A `with_pixel_region` instance holds its region in pixels until the texture's size
        // is known; resolve it to UVs here, or draw nothing for an unknown texture.
        let inst = if inst.has_pixel_region() {
            let Some(tex_size) = self.gfx.texture_size(tex) else {
                return;
            };
            inst.resolve_pixel_region(tex_size)
        } else {
            inst
        };
        self.gfx.sprites.draw(tex, layer, blend, inst);
    }

    /// Filled rectangle (layer 0, alpha blending; uses the internal 1×1 white texture).
    pub fn rect(&mut self, pos: [f32; 2], size: [f32; 2], color: impl Into<Color>) {
        self.rect_ex(pos, size, color, 0, BlendMode::Alpha);
    }

    /// Filled rectangle on an explicit layer with an explicit blend mode.
    pub fn rect_ex(
        &mut self,
        pos: [f32; 2],
        size: [f32; 2],
        color: impl Into<Color>,
        layer: impl Into<Layer>,
        blend: BlendMode,
    ) {
        let tex = self.gfx.white_texture();
        self.gfx.sprites.draw(
            tex,
            layer.into().0,
            blend,
            SpriteInstance::at(pos, size).with_color(color),
        );
    }

    /// 4 thin lines around the rectangle (layer 0, alpha blending).
    pub fn rect_outline(
        &mut self,
        pos: [f32; 2],
        size: [f32; 2],
        thickness: f32,
        color: impl Into<Color>,
    ) {
        self.rect_outline_ex(pos, size, thickness, color, 0, BlendMode::Alpha);
    }

    /// Rectangle outline on an explicit layer with an explicit blend mode.
    pub fn rect_outline_ex(
        &mut self,
        pos: [f32; 2],
        size: [f32; 2],
        thickness: f32,
        color: impl Into<Color>,
        layer: impl Into<Layer>,
        blend: BlendMode,
    ) {
        let (color, layer) = (color.into(), layer.into());
        let [x, y] = pos;
        let [w, h] = size;
        self.line_ex([x, y], [x + w, y], thickness, color, layer, blend);
        self.line_ex([x + w, y], [x + w, y + h], thickness, color, layer, blend);
        self.line_ex([x + w, y + h], [x, y + h], thickness, color, layer, blend);
        self.line_ex([x, y + h], [x, y], thickness, color, layer, blend);
    }

    /// Line segment, drawn as a rotated thin quad through the sprite batcher (layer 0, alpha
    /// blending).
    pub fn line(&mut self, a: [f32; 2], b: [f32; 2], thickness: f32, color: impl Into<Color>) {
        self.line_ex(a, b, thickness, color, 0, BlendMode::Alpha);
    }

    /// Line segment on an explicit layer with an explicit blend mode.
    pub fn line_ex(
        &mut self,
        a: [f32; 2],
        b: [f32; 2],
        thickness: f32,
        color: impl Into<Color>,
        layer: impl Into<Layer>,
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
            color: color.into().to_array(),
            rotation: angle,
            _pad: [0.0; 3],
        };
        self.gfx.sprites.draw(tex, layer.into().0, blend, inst);
    }

    /// Group key for an untextured shape: it samples the white texture, like `rect`.
    fn plain(&self, layer: impl Into<Layer>, blend: BlendMode) -> ShapeKey {
        ShapeKey {
            layer: layer.into().0,
            blend,
            texture: self.gfx.white_texture(),
        }
    }

    /// Filled convex quad through `points` (in order, either winding), alpha-blended. Lands
    /// above this layer's sprites and rects and below its circles; see [`Painter`].
    pub fn quad(
        &mut self,
        points: [[f32; 2]; 4],
        color: impl Into<Color>,
        layer: impl Into<Layer>,
    ) {
        self.quad_ex(points, color, layer, BlendMode::Alpha);
    }

    /// [`Painter::quad`] with an explicit blend mode.
    pub fn quad_ex(
        &mut self,
        points: [[f32; 2]; 4],
        color: impl Into<Color>,
        layer: impl Into<Layer>,
        blend: BlendMode,
    ) {
        let key = self.plain(layer, blend);
        self.gfx.shapes.polygon(
            key,
            &points,
            Colors::Uniform(color.into().to_array()),
            Uvs::None,
        );
    }

    /// [`Painter::quad`] with one colour per corner, interpolated across the quad (e.g. a
    /// vertical gradient: the same colour on both top corners, another on both bottom ones).
    pub fn quad_colored(
        &mut self,
        points: [[f32; 2]; 4],
        colors: [impl Into<Color>; 4],
        layer: impl Into<Layer>,
    ) {
        let colors = colors.map(|c| c.into().to_array());
        let key = self.plain(layer, BlendMode::Alpha);
        self.gfx
            .shapes
            .polygon(key, &points, Colors::PerVertex(&colors), Uvs::None);
    }

    /// Convex quad through `points` showing `tex`, with `uvs` the texture coordinates of the
    /// four corners (`[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]` for the whole
    /// texture), tinted by `color`. UVs outside `0..1` clamp to the edge texels (textures
    /// do not tile), so draw a tiling surface as one quad per tile.
    ///
    /// Texturing is perspective-correct: the quad is treated as a flat rectangle seen in
    /// perspective (a road segment, a mode-7 floor), so a trapezoid shows the texture
    /// foreshortened rather than sheared along its diagonal. Concave or self-intersecting
    /// quads fall back to plain affine texturing.
    pub fn quad_textured(
        &mut self,
        tex: TextureId,
        points: [[f32; 2]; 4],
        uvs: [[f32; 2]; 4],
        color: impl Into<Color>,
        layer: impl Into<Layer>,
        blend: BlendMode,
    ) {
        let key = ShapeKey {
            layer: layer.into().0,
            blend,
            texture: tex,
        };
        let uvq = quad_uvq(points, uvs);
        self.gfx.shapes.polygon(
            key,
            &points,
            Colors::Uniform(color.into().to_array()),
            Uvs::Projective(&uvq),
        );
    }

    /// Filled convex polygon through `points` (in order, either winding), fan-triangulated
    /// from the first point and alpha-blended. Fewer than three points draw nothing. A
    /// concave polygon would draw wrong; split it or use [`Painter::triangles`].
    pub fn polygon(
        &mut self,
        points: &[[f32; 2]],
        color: impl Into<Color>,
        layer: impl Into<Layer>,
    ) {
        self.polygon_ex(points, color, layer, BlendMode::Alpha);
    }

    /// [`Painter::polygon`] with an explicit blend mode.
    pub fn polygon_ex(
        &mut self,
        points: &[[f32; 2]],
        color: impl Into<Color>,
        layer: impl Into<Layer>,
        blend: BlendMode,
    ) {
        let key = self.plain(layer, blend);
        self.gfx.shapes.polygon(
            key,
            points,
            Colors::Uniform(color.into().to_array()),
            Uvs::None,
        );
    }

    /// [`Painter::polygon`] with one colour per point, interpolated across the polygon.
    /// Draws nothing unless `colors.len() == points.len()` (a debug-build panic).
    pub fn polygon_colored(
        &mut self,
        points: &[[f32; 2]],
        colors: &[[f32; 4]],
        layer: impl Into<Layer>,
    ) {
        let key = self.plain(layer, BlendMode::Alpha);
        self.gfx
            .shapes
            .polygon(key, points, Colors::PerVertex(colors), Uvs::None);
    }

    /// Filled triangle list: every three `indices` into `vertices` make one triangle,
    /// alpha-blended. Draws nothing if an index is out of range or `indices.len()` is not a
    /// multiple of three (a debug-build panic; release builds log one warning).
    pub fn triangles(
        &mut self,
        vertices: &[[f32; 2]],
        indices: &[u32],
        color: impl Into<Color>,
        layer: impl Into<Layer>,
    ) {
        self.triangles_ex(vertices, indices, color, layer, BlendMode::Alpha);
    }

    /// [`Painter::triangles`] with an explicit blend mode.
    pub fn triangles_ex(
        &mut self,
        vertices: &[[f32; 2]],
        indices: &[u32],
        color: impl Into<Color>,
        layer: impl Into<Layer>,
        blend: BlendMode,
    ) {
        let key = self.plain(layer, blend);
        self.gfx.shapes.triangles(
            key,
            vertices,
            Colors::Uniform(color.into().to_array()),
            Uvs::None,
            indices,
        );
    }

    /// [`Painter::triangles`] with one colour per vertex, interpolated across each triangle.
    /// Also draws nothing unless `colors.len() == vertices.len()` (a debug-build panic).
    pub fn triangles_colored(
        &mut self,
        vertices: &[[f32; 2]],
        indices: &[u32],
        colors: &[[f32; 4]],
        layer: impl Into<Layer>,
    ) {
        let key = self.plain(layer, BlendMode::Alpha);
        self.gfx
            .shapes
            .triangles(key, vertices, Colors::PerVertex(colors), Uvs::None, indices);
    }

    /// Triangle list showing `tex`, with `uvs` one texture coordinate per vertex, tinted by
    /// `color`. UVs outside `0..1` clamp to the edge texels, as for
    /// [`Painter::quad_textured`]. UVs are interpolated affinely (a triangle list carries no perspective); for
    /// a perspective-correct quad use [`Painter::quad_textured`]. Draws nothing unless
    /// `uvs.len() == vertices.len()` and the indices are valid, as for
    /// [`Painter::triangles`].
    #[allow(clippy::too_many_arguments)]
    pub fn triangles_textured(
        &mut self,
        tex: TextureId,
        vertices: &[[f32; 2]],
        uvs: &[[f32; 2]],
        indices: &[u32],
        color: impl Into<Color>,
        layer: impl Into<Layer>,
        blend: BlendMode,
    ) {
        let key = ShapeKey {
            layer: layer.into().0,
            blend,
            texture: tex,
        };
        self.gfx.shapes.triangles(
            key,
            vertices,
            Colors::Uniform(color.into().to_array()),
            Uvs::Affine(uvs),
            indices,
        );
    }

    /// Filled disk (layer 0).
    pub fn circle(&mut self, center: [f32; 2], radius: f32, color: impl Into<Color>) {
        self.circle_ex(center, radius, 0.0, color, 0);
    }

    /// Ring (outline circle) with the given pixel thickness (layer 0).
    pub fn circle_outline(
        &mut self,
        center: [f32; 2],
        radius: f32,
        thickness: f32,
        color: impl Into<Color>,
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
        color: impl Into<Color>,
        layer: impl Into<Layer>,
    ) {
        self.gfx.primitives.push(
            layer.into().0,
            CircleInstance {
                center,
                radius,
                thickness,
                color: color.into().to_array(),
            },
        );
    }

    /// Draw text with its top-left corner at `pos`, `size_px` pixels high. Text drawn this way
    /// renders on top of every layer; use [`Painter::text_ex`] to put it on one. Positions
    /// are screen pixels: text doesn't follow the 2D camera.
    pub fn text(&mut self, pos: [f32; 2], s: &str, size_px: f32, color: impl Into<Color>) {
        self.gfx
            .text
            .queue(s, pos, size_px, color.into().to_array(), None);
    }

    /// [`Painter::text`] on an explicit layer, sorted with the other 2D content: it covers
    /// lower layers and is covered by higher ones. Within its layer, text draws on top of the
    /// layer's sprites, shapes and circles. Use it for a fade or pause overlay over the HUD,
    /// or a dialog box over labels.
    pub fn text_ex(
        &mut self,
        pos: [f32; 2],
        s: &str,
        size_px: f32,
        color: impl Into<Color>,
        layer: impl Into<Layer>,
    ) {
        self.gfx.text.queue(
            s,
            pos,
            size_px,
            color.into().to_array(),
            Some(layer.into().0),
        );
    }

    /// Size `[width, height]` in pixels that `s` takes up when drawn with
    /// [`Painter::text`] at `size_px`, for centering or right-aligning it, or sizing a box
    /// to fit. Same as [`Graphics::measure_text`].
    pub fn measure_text(&mut self, s: &str, size_px: f32) -> [f32; 2] {
        self.gfx.measure_text(s, size_px)
    }

    /// Draw an instance of a 3D `mesh` with the given column-major `model` transform and
    /// tint. Meshes are depth-tested against [`crate::Graphics::camera3d`] and render under
    /// all 2D content.
    pub fn mesh(&mut self, mesh: MeshId, model: Mat4, color: impl Into<Color>) {
        self.gfx
            .draw_mesh(mesh, MeshInstance::new(model, color.into().to_array()));
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
