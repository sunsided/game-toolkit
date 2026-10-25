//! Builders for draw calls with several optional settings, returned by
//! [`Painter::draw_sprite`], [`Painter::draw_rect`] and [`Painter::draw_text`].
//!
//! Each builder queues its draw when it is dropped, which for the usual one-statement chain
//! is the end of the statement:
//!
//! ```ignore
//! p.draw_sprite(tex, pos, size).region(src_pos, src_size).layer(HUD).flip_x_if(mirror);
//! p.draw_rect(pos, size).color(Color::BLACK.with_alpha(0.5)).layer(Layer::TOP);
//! p.draw_text([w / 2.0, 24.0], "PAUSED").size(48.0).align(Align::Center).layer(Layer::TOP);
//! ```

use crate::color::{Color, Layer};
use crate::painter::Painter;
use crate::sprite::{BlendMode, SpriteInstance};
use crate::text::Align;
use crate::texture::TextureId;

/// A sprite draw from [`Painter::draw_sprite`]; queued when dropped. Defaults: the whole
/// texture, untinted, layer 0, alpha blending.
pub struct SpriteDraw<'p, 'a> {
    painter: &'p mut Painter<'a>,
    tex: TextureId,
    inst: SpriteInstance,
    region: Option<([f32; 2], [f32; 2])>,
    flip: [bool; 2],
    layer: Layer,
    blend: BlendMode,
}

impl<'p, 'a> SpriteDraw<'p, 'a> {
    pub(crate) fn new(
        painter: &'p mut Painter<'a>,
        tex: TextureId,
        pos: [f32; 2],
        size: [f32; 2],
    ) -> Self {
        Self {
            painter,
            tex,
            inst: SpriteInstance::at(pos, size),
            region: None,
            flip: [false; 2],
            layer: Layer::DEFAULT,
            blend: BlendMode::Alpha,
        }
    }

    /// Show the `src_size` pixel rectangle at `src_pos` (top-left, in pixels) of the texture,
    /// e.g. one sprite of an atlas. Draws nothing if the texture is unknown.
    pub fn region(mut self, src_pos: [f32; 2], src_size: [f32; 2]) -> Self {
        self.region = Some((src_pos, src_size));
        self
    }

    /// Show the UV rectangle `min..max` instead of the whole texture (or a [`Self::region`]).
    pub fn uv(mut self, min: [f32; 2], max: [f32; 2]) -> Self {
        self.region = None;
        self.inst = self.inst.with_uv(min, max);
        self
    }

    /// Tint (multiplied with the texture).
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.inst = self.inst.with_color(color);
        self
    }

    /// Rotation in radians around the sprite's center.
    pub fn rotation(mut self, radians: f32) -> Self {
        self.inst = self.inst.with_rotation(radians);
        self
    }

    /// Mirror horizontally.
    pub fn flip_x(self) -> Self {
        self.flip_x_if(true)
    }

    /// Mirror vertically.
    pub fn flip_y(mut self) -> Self {
        self.flip[1] = true;
        self
    }

    /// Mirror horizontally if `flip`, e.g. for a character facing left.
    pub fn flip_x_if(mut self, flip: bool) -> Self {
        self.flip[0] = flip;
        self
    }

    pub fn layer(mut self, layer: impl Into<Layer>) -> Self {
        self.layer = layer.into();
        self
    }

    pub fn blend(mut self, blend: BlendMode) -> Self {
        self.blend = blend;
        self
    }
}

impl Drop for SpriteDraw<'_, '_> {
    fn drop(&mut self) {
        let mut inst = self.inst;
        if let Some((pos, size)) = self.region {
            inst = inst.with_pixel_region(pos, size);
        }
        if self.flip[0] {
            inst = inst.flip_x();
        }
        if self.flip[1] {
            inst = inst.flip_y();
        }
        self.painter
            .sprite_ex(self.tex, inst, self.layer, self.blend);
    }
}

/// A rectangle draw from [`Painter::draw_rect`]; queued when dropped. Defaults: filled white,
/// layer 0, alpha blending.
pub struct RectDraw<'p, 'a> {
    painter: &'p mut Painter<'a>,
    pos: [f32; 2],
    size: [f32; 2],
    color: Color,
    outline: Option<f32>,
    layer: Layer,
    blend: BlendMode,
}

impl<'p, 'a> RectDraw<'p, 'a> {
    pub(crate) fn new(painter: &'p mut Painter<'a>, pos: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            painter,
            pos,
            size,
            color: Color::WHITE,
            outline: None,
            layer: Layer::DEFAULT,
            blend: BlendMode::Alpha,
        }
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = color.into();
        self
    }

    /// Draw only the outline, `thickness` pixels wide, instead of filling.
    pub fn outline(mut self, thickness: f32) -> Self {
        self.outline = Some(thickness);
        self
    }

    pub fn layer(mut self, layer: impl Into<Layer>) -> Self {
        self.layer = layer.into();
        self
    }

    pub fn blend(mut self, blend: BlendMode) -> Self {
        self.blend = blend;
        self
    }
}

impl Drop for RectDraw<'_, '_> {
    fn drop(&mut self) {
        let Self {
            pos,
            size,
            color,
            layer,
            blend,
            ..
        } = *self;
        match self.outline {
            Some(t) => self
                .painter
                .rect_outline_ex(pos, size, t, color, layer, blend),
            None => self.painter.rect_ex(pos, size, color, layer, blend),
        }
    }
}

/// A text draw from [`Painter::draw_text`]; queued when dropped. Defaults: 16 px, white,
/// left-aligned, unlayered (on top of everything, like [`Painter::text`]).
pub struct TextDraw<'p, 'a, 's> {
    painter: &'p mut Painter<'a>,
    pos: [f32; 2],
    text: &'s str,
    size_px: f32,
    color: Color,
    align: Align,
    layer: Option<Layer>,
}

impl<'p, 'a, 's> TextDraw<'p, 'a, 's> {
    pub(crate) fn new(painter: &'p mut Painter<'a>, pos: [f32; 2], text: &'s str) -> Self {
        Self {
            painter,
            pos,
            text,
            size_px: 16.0,
            color: Color::WHITE,
            align: Align::Left,
            layer: None,
        }
    }

    /// Text height in pixels.
    pub fn size(mut self, size_px: f32) -> Self {
        self.size_px = size_px;
        self
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = color.into();
        self
    }

    /// Which point of the text the position names: its left edge (default), center or right
    /// edge. The text block is measured with [`Painter::measure_text`] and shifted; lines of
    /// multi-line text stay left-aligned within it. The position is always the top edge.
    pub fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Sort the text with the 2D layers, as [`Painter::text_ex`] does, instead of drawing it
    /// on top of everything.
    pub fn layer(mut self, layer: impl Into<Layer>) -> Self {
        self.layer = Some(layer.into());
        self
    }
}

impl Drop for TextDraw<'_, '_, '_> {
    fn drop(&mut self) {
        let mut pos = self.pos;
        if self.align != Align::Left {
            let [w, _] = self.painter.measure_text(self.text, self.size_px);
            pos[0] -= w * self.align.factor();
        }
        match self.layer {
            Some(layer) => self
                .painter
                .text_ex(pos, self.text, self.size_px, self.color, layer),
            None => self.painter.text(pos, self.text, self.size_px, self.color),
        }
    }
}
