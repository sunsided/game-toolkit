//! [`Color`] and [`Layer`]: small typed wrappers for the painter's colour and layer arguments.

/// An RGBA colour with components in `0.0..=1.0`.
///
/// Every painter call takes `impl Into<Color>`, so a plain `[r, g, b, a]` array works as well
/// as a `Color`. The values reach the GPU unchanged, exactly like the arrays did: the 8-bit
/// and hex constructors divide by 255 and do no colour-space conversion.
///
/// The surface is sRGB, so sprites, shapes and circles treat the components as linear and a
/// `Color::hex` shows lighter on them than in a CSS colour picker, while text treats the
/// same components as sRGB and matches it.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);
    pub const GRAY: Self = Self::rgb(0.5, 0.5, 0.5);
    pub const RED: Self = Self::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Self = Self::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Self = Self::rgb(0.0, 0.0, 1.0);
    pub const YELLOW: Self = Self::rgb(1.0, 1.0, 0.0);
    pub const CYAN: Self = Self::rgb(0.0, 1.0, 1.0);
    pub const MAGENTA: Self = Self::rgb(1.0, 0.0, 1.0);
    pub const ORANGE: Self = Self::rgb(1.0, 0.5, 0.0);
    pub const GOLD: Self = Self::rgb(1.0, 0.84, 0.0);

    /// Opaque colour from components in `0.0..=1.0`.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    /// Colour from components in `0.0..=1.0`.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Opaque colour from 8-bit components.
    pub const fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::rgba8(r, g, b, 255)
    }

    /// Colour from 8-bit components.
    pub const fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::rgba(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }

    /// Opaque colour from a `0xRRGGBB` literal, as in CSS: `Color::hex(0xff8800)`.
    pub const fn hex(rgb: u32) -> Self {
        Self::rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }

    /// Colour from a `0xRRGGBBAA` literal: `Color::hex_rgba(0xff880080)` is half-transparent.
    pub const fn hex_rgba(rgba: u32) -> Self {
        Self::rgba8(
            (rgba >> 24) as u8,
            (rgba >> 16) as u8,
            (rgba >> 8) as u8,
            rgba as u8,
        )
    }

    /// This colour with alpha replaced by `a`.
    pub const fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Linear interpolation to `other`: `t = 0.0` is `self`, `1.0` is `other`.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self::rgba(
            mix(self.r, other.r),
            mix(self.g, other.g),
            mix(self.b, other.b),
            mix(self.a, other.a),
        )
    }

    /// As `[r, g, b, a]`.
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

impl From<[f32; 4]> for Color {
    fn from([r, g, b, a]: [f32; 4]) -> Self {
        Self { r, g, b, a }
    }
}

impl From<[f32; 3]> for Color {
    fn from([r, g, b]: [f32; 3]) -> Self {
        Self::rgb(r, g, b)
    }
}

impl From<Color> for [f32; 4] {
    fn from(c: Color) -> Self {
        c.to_array()
    }
}

/// A 2D draw layer: lower layers draw under higher ones (see [`crate::Painter`]'s draw-order
/// notes). Every painter call takes `impl Into<Layer>`, so a bare integer works too.
///
/// Name your layers as constants: `const HUD: Layer = Layer(3);`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Layer(pub i16);

impl Layer {
    /// Layer 0, where the calls without a layer argument draw.
    pub const DEFAULT: Self = Self(0);
    /// The lowest layer.
    pub const BOTTOM: Self = Self(i16::MIN);
    /// The highest layer.
    pub const TOP: Self = Self(i16::MAX);
}

impl From<i16> for Layer {
    fn from(layer: i16) -> Self {
        Self(layer)
    }
}

impl From<Layer> for i16 {
    fn from(layer: Layer) -> Self {
        layer.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_rgba8() {
        assert_eq!(Color::hex(0xff0000), Color::RED);
        assert_eq!(
            Color::hex_rgba(0x00ff0080),
            Color::GREEN.with_alpha(128.0 / 255.0)
        );
        assert_eq!(Color::rgb8(0, 0, 255), Color::BLUE);
    }

    #[test]
    fn array_conversions_roundtrip() {
        let c: Color = [0.1, 0.2, 0.3, 0.4].into();
        assert_eq!(<[f32; 4]>::from(c), [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(Color::from([0.1, 0.2, 0.3]).a, 1.0);
    }

    #[test]
    fn lerp_endpoints() {
        assert_eq!(Color::BLACK.lerp(Color::WHITE, 0.0), Color::BLACK);
        assert_eq!(Color::BLACK.lerp(Color::WHITE, 1.0), Color::WHITE);
        assert_eq!(Color::BLACK.lerp(Color::WHITE, 0.5), Color::GRAY);
    }

    #[test]
    fn layer_from_literal() {
        fn take(l: impl Into<Layer>) -> Layer {
            l.into()
        }
        assert_eq!(take(3), Layer(3));
        assert!(Layer::BOTTOM < Layer::DEFAULT && Layer::DEFAULT < Layer::TOP);
    }
}
