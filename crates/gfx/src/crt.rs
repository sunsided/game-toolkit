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
            persistence: 0.1,
            curvature: 0.0,
            vignette: 0.1,
            brightness: 1.1,
            sharpness: 1.0,
        }
    }

    fn params(&self) -> CrtParams {
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
            _pad: [0.0; 2],
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
    _pad: [f32; 2],
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
    uploaded: Option<CrtSettings>,
}

impl CrtEffect {
    pub fn new(gfx: &crate::Graphics, settings: CrtSettings) -> Self {
        let shader = ShaderEffect::with_params(
            gfx,
            "crt.wgsl",
            include_str!("crt.wgsl"),
            &settings.params(),
        )
        .with_history();
        Self {
            shader,
            settings,
            uploaded: Some(settings),
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
        if self.uploaded != Some(self.settings) {
            self.shader.set_params(&self.settings.params());
            self.uploaded = Some(self.settings);
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
        assert_eq!(size_of::<CrtParams>(), 48);
    }
}
