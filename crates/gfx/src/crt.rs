//! A built-in CRT filter for the [`PostStage::Window`](crate::PostStage::Window) stage
//! (feature `crt`).

use bytemuck::{Pod, Zeroable};

use crate::post::{PostEffect, PostPass, ShaderEffect};

/// The phosphor pattern a [`CrtEffect`] overlays.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum CrtMask {
    None,
    /// Vertical RGB stripes (Trinitron-style).
    #[default]
    ApertureGrille,
    /// RGB triads offset on alternate rows.
    ShadowMask,
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
    pub sharpness: f32,
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
        }
    }

    /// The shader parameters; `indexed` says whether the frame has an indexed canvas.
    fn params(&self, indexed: bool) -> CrtParams {
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
pub struct CrtEffect {
    shader: ShaderEffect,
    settings: CrtSettings,
    /// The settings and indexed state last uploaded.
    uploaded: Option<(CrtSettings, bool)>,
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
        let params = settings.params(false);
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
            uploaded: Some((settings, false)),
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
}

impl PostEffect for CrtEffect {
    fn apply(&mut self, pass: &mut PostPass<'_>) {
        let state = (self.settings, pass.indices.is_some());
        if self.uploaded != Some(state) {
            self.shader.set_params(&self.settings.params(state.1));
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
        assert_eq!(size_of::<CrtParams>(), 80);
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
        assert_eq!(s.params(false).bloom_from_indices, 0);
        assert_eq!(s.params(true).bloom_from_indices, 1);
        assert_eq!(s.params(true).emissive[0][0], 1 << 5);
    }
}
