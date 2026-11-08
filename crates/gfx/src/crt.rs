//! A built-in CRT filter for the [`PostStage::Window`](crate::PostStage::Window) stage
//! (feature `crt`).

use bytemuck::{Pod, Zeroable};

use crate::color::Color;
use crate::post::{PostEffect, PostPass, ShaderEffect};
use crate::texture::TextureId;

/// The phosphor pattern a [`CrtEffect`] overlays.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum CrtMask {
    None,
    /// Vertical RGB stripes (Trinitron-style).
    #[default]
    ApertureGrille,
    /// RGB triads offset on alternate rows.
    ShadowMask,
    /// Vertical RGB slots, every other triad column shifted down by half a slot, with short
    /// gaps between the slots (most TVs).
    SlotMask,
}

/// A model of the electron beam for [`CrtSettings::beam`]: each source line is lit as a
/// Gaussian stripe that grows with brightness, so bright lines close the gaps between
/// scanlines and dark ones leave them open, and the spot is blurred horizontally, which melts
/// dither patterns into the colours the artists meant. Wants a few window pixels per source
/// line; below about two the gaps fade out instead of aliasing.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CrtBeam {
    /// Beam height (Gaussian sigma, in source lines) for black.
    pub min: f32,
    /// Beam height for full brightness; `min..=max`, around `0.3..0.6`. Above about `0.4`
    /// neighbouring lines overlap and add up, so bright areas get brighter and may clip.
    pub max: f32,
    /// Horizontal spot blur (sigma, in source pixels): `0.3` is sharp, `1.0` a blurry TV.
    pub blur: f32,
}

/// A picture of the game's own for the CRT to show instead of the canvas, see
/// [`CrtEffect::with_source`]: an emulated screen or a video frame, one texel per dot and
/// line, so scanlines and blur follow its lines rather than the canvas rows.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CrtSource {
    /// Painter texture with the picture, uploaded each frame with `update_texture_rgba`.
    pub texture: TextureId,
    /// Where the picture goes, `[x, y, w, h]` in pixels of the stage's image (window pixels
    /// in the [`Window`](crate::PostStage::Window) stage). Outside it the input passes
    /// through unchanged, so a border or UI drawn around the picture stays as it is.
    pub rect: [f32; 4],
    /// The part of the texture to show, `[x, y, w, h]` in texels, cut to the texture. If
    /// nothing is left of it, or `rect` is empty, the effect filters the canvas instead.
    pub region: [f32; 4],
}

impl CrtSource {
    /// This source with its region cut to a texture of `size`; `None` if there is no picture
    /// to show (the comparisons also reject NaN).
    fn fitted(mut self, size: [u32; 2]) -> Option<Self> {
        let [x, y, w, h] = self.region;
        let [tw, th] = size.map(|v| v as f32);
        let (x0, x1) = (x.clamp(0.0, tw), (x + w).clamp(0.0, tw));
        let (y0, y1) = (y.clamp(0.0, th), (y + h).clamp(0.0, th));
        self.region = [x0, y0, x1 - x0, y1 - y0];
        let visible = self.rect[2] > 0.0 && self.rect[3] > 0.0;
        // The shader addresses whole texels, so at least one of them.
        (visible && x1 - x0 >= 1.0 && y1 - y0 >= 1.0).then_some(self)
    }
}

/// What glows when [`CrtSettings::bloom`] is on.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum BloomSource {
    /// Every pixel, by its brightness (default).
    #[default]
    Luminance,
    /// Only pixels drawn with these palette indices (feature `indexed`): a 256-bit set, bit
    /// `i % 32` of word `i / 32` for index `i`, looked up before palette cycling. Lava glows,
    /// equally bright snow doesn't. Build it with [`BloomSource::indices`]. Without an indexed
    /// canvas it falls back to [`Luminance`](BloomSource::Luminance).
    IndexMask([u32; 8]),
}

impl BloomSource {
    /// An [`IndexMask`](BloomSource::IndexMask) of `indices`, e.g. `BloomSource::indices(48..56)`
    /// for a lava ramp.
    pub fn indices(indices: impl IntoIterator<Item = u8>) -> Self {
        let mut mask = [0u32; 8];
        for i in indices {
            mask[usize::from(i / 32)] |= 1 << (i % 32);
        }
        Self::IndexMask(mask)
    }
}

/// Settings for [`CrtEffect`]. Start from a preset and adjust: every value is safe to change
/// at runtime through [`CrtEffect::settings_mut`].
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CrtSettings {
    /// How dark the gaps between canvas rows get, `0..=1`.
    pub scanlines: f32,
    pub mask: CrtMask,
    /// How much the mask dims the colours it doesn't pass, `0..=1`.
    pub mask_strength: f32,
    /// Window pixels per mask stripe; `1.0` gives a 3-pixel RGB triad.
    pub mask_size: f32,
    /// Glow added around bright pixels, `0..=1`.
    pub bloom: f32,
    /// Which pixels glow; default [`BloomSource::Luminance`]. The presets keep the default.
    pub bloom_source: BloomSource,
    /// Phosphor afterglow: how much of the previous frame survives, `0..1`; `0` disables it
    /// (and the history texture it needs).
    pub persistence: f32,
    /// Barrel distortion of the screen, `0` (flat) to about `0.1`.
    pub curvature: f32,
    /// Darkening towards the corners, `0..=1`.
    pub vignette: f32,
    /// Overall gain, compensating for what scanlines and mask take away.
    pub brightness: f32,
    /// `1.0`: hard canvas pixel edges with a one-window-pixel blend; `0.0`: plain bilinear.
    /// Not used with a [`beam`](Self::beam), which blurs instead.
    pub sharpness: f32,
    /// The beam model instead of the plain row darkening (`None`, all presets but
    /// [`monochrome_monitor`](Self::monochrome_monitor)); [`scanlines`](Self::scanlines)
    /// sets how dark its gaps get.
    pub beam: Option<CrtBeam>,
    /// A monochrome monitor: only brightness is shown, in this phosphor colour (sRGB, alpha
    /// ignored), e.g. `Color::hex(0x33ff66)` for green, `Color::hex(0xffb000)` for amber.
    pub monochrome: Option<Color>,
    /// Rounded screen corners: the radius as a fraction of half the picture height, `0..=1`;
    /// `0` for square.
    pub corner_radius: f32,
}

impl Default for CrtSettings {
    fn default() -> Self {
        Self::arcade()
    }
}

impl CrtSettings {
    /// Barely there: soft scanlines, no mask or curvature. Good for making fractional
    /// scaling look even.
    pub fn subtle() -> Self {
        Self {
            scanlines: 0.25,
            mask: CrtMask::None,
            mask_strength: 0.0,
            mask_size: 1.0,
            bloom: 0.05,
            bloom_source: BloomSource::Luminance,
            persistence: 0.0,
            curvature: 0.0,
            vignette: 0.1,
            brightness: 1.05,
            sharpness: 1.0,
            beam: None,
            monochrome: None,
            corner_radius: 0.0,
        }
    }

    /// An arcade monitor: strong scanlines, aperture grille, some bloom and afterglow.
    pub fn arcade() -> Self {
        Self {
            scanlines: 0.55,
            mask: CrtMask::ApertureGrille,
            mask_strength: 0.3,
            mask_size: 1.0,
            bloom: 0.15,
            bloom_source: BloomSource::Luminance,
            persistence: 0.35,
            curvature: 0.03,
            vignette: 0.25,
            brightness: 1.3,
            sharpness: 1.0,
            beam: None,
            monochrome: None,
            corner_radius: 0.0,
        }
    }

    /// A living-room TV: shadow mask, softer image, more curvature and glow.
    pub fn consumer_tv() -> Self {
        Self {
            scanlines: 0.4,
            mask: CrtMask::ShadowMask,
            mask_strength: 0.35,
            mask_size: 1.0,
            bloom: 0.3,
            bloom_source: BloomSource::Luminance,
            persistence: 0.5,
            curvature: 0.06,
            vignette: 0.4,
            brightness: 1.35,
            sharpness: 0.5,
            beam: None,
            monochrome: None,
            corner_radius: 0.0,
        }
    }

    /// A sharp PC monitor: thin scanlines, fine grille, flat.
    pub fn pc_monitor() -> Self {
        Self {
            scanlines: 0.3,
            mask: CrtMask::ApertureGrille,
            mask_strength: 0.15,
            mask_size: 1.0,
            bloom: 0.05,
            bloom_source: BloomSource::Luminance,
            persistence: 0.1,
            curvature: 0.0,
            vignette: 0.1,
            brightness: 1.1,
            sharpness: 1.0,
            beam: None,
            monochrome: None,
            corner_radius: 0.0,
        }
    }

    /// A monochrome monitor (a green or amber screen, a white paper-white one): the beam
    /// model with a soft spot, no mask, a long glow, slightly curved glass with round
    /// corners, everything in shades of `phosphor`.
    pub fn monochrome_monitor(phosphor: Color) -> Self {
        Self {
            scanlines: 0.6,
            mask: CrtMask::None,
            mask_strength: 0.0,
            mask_size: 1.0,
            bloom: 0.35,
            bloom_source: BloomSource::Luminance,
            persistence: 0.0,
            curvature: 0.03,
            vignette: 0.25,
            brightness: 1.15,
            sharpness: 1.0,
            beam: Some(CrtBeam {
                min: 0.3,
                max: 0.55,
                blur: 0.55,
            }),
            monochrome: Some(phosphor),
            corner_radius: 0.03,
        }
    }

    /// The shader parameters; `indexed` says whether the frame has an indexed canvas.
    fn params(&self, indexed: bool, source: Option<&CrtSource>) -> CrtParams {
        let (bloom_from_indices, emissive) = match self.bloom_source {
            BloomSource::IndexMask(mask) if indexed => (
                1,
                [
                    [mask[0], mask[1], mask[2], mask[3]],
                    [mask[4], mask[5], mask[6], mask[7]],
                ],
            ),
            _ => (0, [[0; 4]; 2]),
        };
        CrtParams {
            scanline: self.scanlines,
            mask_strength: self.mask_strength,
            mask_kind: match self.mask {
                CrtMask::None => 0,
                CrtMask::ApertureGrille => 1,
                CrtMask::ShadowMask => 2,
                CrtMask::SlotMask => 3,
            },
            mask_size: self.mask_size,
            bloom: self.bloom,
            persistence: self.persistence,
            curvature: self.curvature,
            vignette: self.vignette,
            brightness: self.brightness,
            sharpness: self.sharpness,
            bloom_from_indices,
            _pad: 0.0,
            emissive,
            beam: self.beam.map_or([0.0; 4], |b| {
                // max below min would make the beam narrow as it brightens.
                let min = b.min.max(0.05);
                [min, b.max.max(min), b.blur.max(0.05), 1.0]
            }),
            mono: self.monochrome.map_or([0.0; 4], |c| {
                let lin = |v: f32| crate::graphics::srgb_to_linear(v) as f32;
                [lin(c.r), lin(c.g), lin(c.b), 1.0]
            }),
            shape: [self.corner_radius.clamp(0.0, 1.0), 0.0, 0.0, 0.0],
            src_rect: source.map_or([0.0; 4], |s| s.rect),
            src_region: source.map_or([0.0; 4], |s| s.region),
        }
    }
}

/// Mirrors `CrtParams` in `crt.wgsl`.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CrtParams {
    scanline: f32,
    mask_strength: f32,
    mask_kind: u32,
    mask_size: f32,
    bloom: f32,
    persistence: f32,
    curvature: f32,
    vignette: f32,
    brightness: f32,
    sharpness: f32,
    bloom_from_indices: u32,
    _pad: f32,
    emissive: [[u32; 4]; 2],
    beam: [f32; 4],
    mono: [f32; 4],
    shape: [f32; 4],
    src_rect: [f32; 4],
    src_region: [f32; 4],
}

/// A CRT look: scanlines locked to canvas rows, an aperture grille or shadow mask, bloom,
/// phosphor afterglow, curvature and vignette. Add it to the
/// [`PostStage::Window`](crate::PostStage::Window) stage, ideally with a virtual resolution
/// (it treats each canvas row as a scanline; without one, each window row):
///
/// ```ignore
/// let crt = ctx.gfx.add_post_effect(PostStage::Window, CrtEffect::new(&ctx.gfx, CrtSettings::arcade()));
/// // later
/// ctx.gfx.post_effect_mut::<CrtEffect>(crt).unwrap().settings_mut().curvature = 0.0;
/// ```
///
/// To filter a picture of the game's own (an emulated screen) instead of the canvas, give it
/// a [`CrtSource`] with [`with_source`](Self::with_source).
pub struct CrtEffect {
    shader: ShaderEffect,
    settings: CrtSettings,
    source: Option<CrtSource>,
    /// The settings, indexed state and source last uploaded.
    uploaded: Option<(CrtSettings, bool, Option<CrtSource>)>,
}

impl CrtEffect {
    pub fn new(gfx: &crate::Graphics, settings: CrtSettings) -> Self {
        Self::with_device(gfx.device(), gfx.surface_format(), settings)
    }

    pub(crate) fn with_device(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        settings: CrtSettings,
    ) -> Self {
        let params = settings.params(false, None);
        let shader = ShaderEffect::build(
            device,
            format,
            "crt.wgsl",
            include_str!("crt.wgsl"),
            Some((
                bytemuck::bytes_of(&params),
                std::any::TypeId::of::<CrtParams>(),
            )),
        )
        .with_history();
        Self {
            shader,
            settings,
            source: None,
            uploaded: Some((settings, false, None)),
        }
    }

    pub fn settings(&self) -> &CrtSettings {
        &self.settings
    }

    /// Change settings in place; uploaded before the next frame.
    pub fn settings_mut(&mut self) -> &mut CrtSettings {
        &mut self.settings
    }

    pub fn set_settings(&mut self, settings: CrtSettings) {
        self.settings = settings;
    }

    /// Show `source` instead of the canvas: its texture, filtered into its rectangle, with
    /// scanlines on its lines. Update the rectangle with [`Self::set_source`] when the layout
    /// changes (a resize).
    ///
    /// ```ignore
    /// let screen = ctx.gfx.create_texture_rgba(640, 400, &pixels, Some("screen"));
    /// let source = CrtSource { texture: screen, rect: [80.0, 60.0, 1280.0, 960.0], region: [0.0, 0.0, 640.0, 400.0] };
    /// let crt = CrtEffect::new(&ctx.gfx, CrtSettings::pc_monitor()).with_source(source);
    /// ctx.gfx.add_post_effect(PostStage::Window, crt);
    /// ```
    #[must_use]
    pub fn with_source(mut self, source: CrtSource) -> Self {
        self.set_source(Some(source));
        self
    }

    /// Change or (`None`) drop the source; see [`Self::with_source`].
    pub fn set_source(&mut self, source: Option<CrtSource>) {
        self.source = source;
    }

    pub fn source(&self) -> Option<CrtSource> {
        self.source
    }
}

impl PostEffect for CrtEffect {
    fn apply(&mut self, pass: &mut PostPass<'_>) {
        // An unknown texture stays as it is, for the shader's own warning.
        let source = self.source.and_then(|s| {
            pass.texture_size(s.texture)
                .map_or(Some(s), |t| s.fitted(t))
        });
        let state = (self.settings, pass.indices.is_some(), source);
        if self.uploaded != Some(state) {
            self.shader
                .set_params(&self.settings.params(state.1, source.as_ref()));
            self.shader.set_source(source.map(|s| s.texture));
            self.uploaded = Some(state);
        }
        self.shader.apply(pass);
    }

    fn uses_history(&self) -> bool {
        self.settings.persistence > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_match_the_wgsl_struct_size() {
        assert_eq!(size_of::<CrtParams>(), 160);
    }

    #[test]
    fn beam_max_is_clamped_to_min() {
        let s = CrtSettings {
            beam: Some(CrtBeam {
                min: 0.5,
                max: 0.2,
                blur: 0.6,
            }),
            ..CrtSettings::arcade()
        };
        let [min, max, ..] = s.params(false, None).beam;
        assert_eq!((min, max), (0.5, 0.5));
    }

    #[test]
    fn source_region_is_cut_to_the_texture() {
        let source = |rect, region| CrtSource {
            texture: TextureId(0),
            rect,
            region,
        };
        let rect = [0.0, 0.0, 64.0, 64.0];
        let fit = |r| source(rect, r).fitted([8, 4]).map(|s| s.region);
        assert_eq!(fit([2.0, 1.0, 4.0, 2.0]), Some([2.0, 1.0, 4.0, 2.0]));
        assert_eq!(fit([6.0, -1.0, 8.0, 3.0]), Some([6.0, 0.0, 2.0, 2.0]));
        assert_eq!(fit([0.0, 0.0, 0.0, 4.0]), None);
        assert_eq!(fit([0.0, 0.0, -8.0, 4.0]), None);
        assert_eq!(fit([9.0, 0.0, 4.0, 4.0]), None);
        assert_eq!(fit([f32::NAN, 0.0, 4.0, 4.0]), None);
        let empty_rect = source([0.0, 0.0, 0.0, 64.0], [0.0, 0.0, 8.0, 4.0]);
        assert_eq!(empty_rect.fitted([8, 4]), None);
    }

    #[test]
    fn index_mask_sets_one_bit_per_index() {
        let BloomSource::IndexMask(m) = BloomSource::indices([0, 31, 32, 255]) else {
            unreachable!()
        };
        assert_eq!(m, [1 | 1 << 31, 1, 0, 0, 0, 0, 0, 1 << 31]);
    }

    #[test]
    fn index_mask_falls_back_to_luminance_without_an_indexed_canvas() {
        let s = CrtSettings {
            bloom_source: BloomSource::indices([5]),
            ..CrtSettings::arcade()
        };
        assert_eq!(s.params(false, None).bloom_from_indices, 0);
        assert_eq!(s.params(true, None).bloom_from_indices, 1);
        assert_eq!(s.params(true, None).emissive[0][0], 1 << 5);
    }
}
