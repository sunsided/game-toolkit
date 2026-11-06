//! Settings [`Graphics::new`](crate::Graphics::new) creates the device and surface with.

use std::fmt;

use crate::canvas::{ScaleFilter, ScaleMode};
use crate::color::Color;

/// Called with the adapter and the device descriptor the toolkit is about to request, after
/// it has merged its own limits and features with [`GraphicsConfig::required_limits`] and
/// [`GraphicsConfig::required_features`]. Change the descriptor to request more (or less)
/// once you have looked at what the adapter offers, e.g. `adapter.features()`.
///
/// Whatever the hook asks for is passed to `request_device` as is, without the clear
/// "missing limit" error the config fields get.
///
/// A closure can capture runtime settings (a user's quality preset, a command-line flag):
///
/// ```
/// # use std::sync::Arc;
/// # use game_toolkit_gfx::{DeviceHook, wgpu};
/// let want_f16 = true; // e.g. from a settings file
/// let hook: DeviceHook = Arc::new(move |adapter, desc| {
///     if want_f16 && adapter.features().contains(wgpu::Features::SHADER_F16) {
///         desc.required_features |= wgpu::Features::SHADER_F16;
///     }
/// });
/// ```
pub type DeviceHook =
    std::sync::Arc<dyn Fn(&wgpu::Adapter, &mut wgpu::DeviceDescriptor<'_>) + Send + Sync>;

/// Settings for [`Graphics::new`](crate::Graphics::new).
///
/// Construct it with `..Default::default()` so fields added in future minor releases don't
/// break your call site. Games usually set these through `AppConfig`, which forwards them.
#[derive(Clone)]
pub struct GraphicsConfig {
    pub vsync: bool,
    /// Depth attachment format, `None` for none. See `AppConfig::depth_format`.
    pub depth_format: Option<wgpu::TextureFormat>,
    /// MSAA sample count; `1` disables MSAA.
    pub msaa_samples: u32,
    /// Device limits the game needs beyond the toolkit's own. `None` (default) requests only
    /// what the toolkit needs: `Limits::downlevel_defaults()` (`Limits::default()` with the
    /// `vector` feature, `downlevel_webgl2_defaults()` on WebGL2). With `Some(limits)`, each
    /// limit is the better of the two, so asking for e.g. `max_storage_buffers_per_shader_stage:
    /// 8` on top of `Limits::downlevel_defaults()` raises only that one.
    ///
    /// If the adapter can't provide a limit, [`Graphics::new`](crate::Graphics::new) fails
    /// with a [`DeviceRequirementsError`] naming it.
    pub required_limits: Option<wgpu::Limits>,
    /// Optional device features the game needs (`SHADER_F16`, `TIMESTAMP_QUERY`, ...).
    /// Default `Features::empty()`. Missing ones fail init with a [`DeviceRequirementsError`].
    pub required_features: wgpu::Features,
    /// Last look at the device request before it is sent; see [`DeviceHook`].
    pub device_hook: Option<DeviceHook>,
    /// Draw into a fixed-size canvas of `(width, height)` pixels (e.g. `(320, 180)`) and
    /// scale it onto the window with [`Self::scale_mode`], for crisp pixel art. `None`
    /// (default) draws straight into the window.
    ///
    /// With a canvas, everything the painter draws (sprites, shapes, text, tilemaps, vector
    /// art, 3D meshes) lands in it; [`Graphics::size`](crate::Graphics::size) and the
    /// cameras report the canvas size, and the toolkit's input reports the mouse and touches
    /// in canvas pixels. [`Graphics::window_size`](crate::Graphics::window_size) and
    /// `Frame::view` still give the window.
    pub virtual_resolution: Option<(u32, u32)>,
    /// How the canvas is scaled onto the window; default [`ScaleMode::Integer`].
    pub scale_mode: ScaleMode,
    /// Filter for scaling the canvas; default [`ScaleFilter::Nearest`].
    pub scale_filter: ScaleFilter,
    /// Colour around the canvas; default black.
    pub border_color: Color,
}

impl Default for GraphicsConfig {
    fn default() -> Self {
        Self {
            vsync: true,
            depth_format: None,
            msaa_samples: 1,
            required_limits: None,
            required_features: wgpu::Features::empty(),
            device_hook: None,
            virtual_resolution: None,
            scale_mode: ScaleMode::Integer,
            scale_filter: ScaleFilter::Nearest,
            border_color: Color::BLACK,
        }
    }
}

impl fmt::Debug for GraphicsConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphicsConfig")
            .field("vsync", &self.vsync)
            .field("depth_format", &self.depth_format)
            .field("msaa_samples", &self.msaa_samples)
            .field("required_limits", &self.required_limits)
            .field("required_features", &self.required_features)
            .field("device_hook", &self.device_hook.is_some())
            .field("virtual_resolution", &self.virtual_resolution)
            .field("scale_mode", &self.scale_mode)
            .field("scale_filter", &self.scale_filter)
            .field("border_color", &self.border_color)
            .finish()
    }
}

/// One device limit the adapter can't provide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingLimit {
    /// The `wgpu::Limits` field, e.g. `max_storage_buffers_per_shader_stage`.
    pub name: &'static str,
    pub requested: u64,
    pub available: u64,
}

/// The adapter can't provide the limits or features the game asked for in
/// [`GraphicsConfig`]. Returned (inside the `anyhow::Error`) by
/// [`Graphics::new`](crate::Graphics::new); `downcast_ref` it to tell it apart from other init
/// failures.
///
/// WebGL2 (the web fallback when the browser has no WebGPU) has no compute shaders and much
/// lower limits, so a game that requires compute fails here there. To show a "WebGPU
/// required" screen instead, don't require the limits; check
/// [`Graphics::supports_compute`](crate::Graphics::supports_compute) and
/// `ctx.gfx.device().limits()` in `Game::init`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceRequirementsError {
    /// The adapter's name, as wgpu reports it.
    pub adapter: String,
    pub backend: wgpu::Backend,
    pub missing_limits: Vec<MissingLimit>,
    pub missing_features: wgpu::Features,
}

impl fmt::Display for DeviceRequirementsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "adapter {:?} ({:?}) can't provide the requested device",
            self.adapter, self.backend
        )?;
        if !self.missing_features.is_empty() {
            write!(f, "; missing features: {:?}", self.missing_features)?;
        }
        for m in &self.missing_limits {
            write!(
                f,
                "; limit {}: requested {}, available {}",
                m.name, m.requested, m.available
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for DeviceRequirementsError {}

/// The limits the toolkit itself needs on `backend`.
pub(crate) fn toolkit_limits(backend: wgpu::Backend) -> wgpu::Limits {
    // WebGL2 can't meet even the downlevel limits (and has no compute, so no vello).
    // Otherwise vello needs the standard (non-downlevel) limits; bump them when it's on.
    if backend == wgpu::Backend::Gl {
        wgpu::Limits::downlevel_webgl2_defaults()
    } else if cfg!(feature = "vector") {
        wgpu::Limits::default()
    } else {
        wgpu::Limits::downlevel_defaults()
    }
}

/// Merge the toolkit's limits for `backend` with the game's, and the features likewise.
/// Texture dimensions follow the adapter (as before: the toolkit asks for the largest the
/// adapter supports) unless the game asked for more.
pub(crate) fn merged_requirements(
    config: &GraphicsConfig,
    backend: wgpu::Backend,
    adapter_limits: &wgpu::Limits,
) -> (wgpu::Limits, wgpu::Features) {
    let mut limits = toolkit_limits(backend).using_resolution(adapter_limits.clone());
    if let Some(extra) = &config.required_limits {
        limits = limits.or_better_values_from(extra);
    }
    (limits, config.required_features)
}

/// Check `limits` and `features` against what the adapter offers.
pub(crate) fn check_requirements(
    adapter: &str,
    backend: wgpu::Backend,
    adapter_limits: &wgpu::Limits,
    adapter_features: wgpu::Features,
    limits: &wgpu::Limits,
    features: wgpu::Features,
) -> Result<(), DeviceRequirementsError> {
    let mut missing_limits = Vec::new();
    limits.check_limits_with_fail_fn(adapter_limits, false, |name, requested, available| {
        missing_limits.push(MissingLimit {
            name,
            requested,
            available,
        });
    });
    let missing_features = features - adapter_features;
    if missing_limits.is_empty() && missing_features.is_empty() {
        return Ok(());
    }
    Err(DeviceRequirementsError {
        adapter: adapter.to_owned(),
        backend,
        missing_limits,
        missing_features,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_limits_raise_only_what_they_ask_for() {
        let config = GraphicsConfig {
            required_limits: Some(wgpu::Limits {
                max_storage_buffers_per_shader_stage: 8,
                ..wgpu::Limits::downlevel_defaults()
            }),
            ..Default::default()
        };
        let adapter = wgpu::Limits::default();
        let (limits, _) = merged_requirements(&config, wgpu::Backend::Vulkan, &adapter);
        assert_eq!(limits.max_storage_buffers_per_shader_stage, 8);
        let base = toolkit_limits(wgpu::Backend::Vulkan);
        assert_eq!(limits.max_bind_groups, base.max_bind_groups);
        // Texture sizes follow the adapter.
        assert_eq!(
            limits.max_texture_dimension_2d,
            adapter.max_texture_dimension_2d
        );
    }

    #[test]
    fn missing_limits_and_features_are_named() {
        let adapter = wgpu::Limits::downlevel_defaults();
        let wanted = wgpu::Limits {
            max_storage_buffers_per_shader_stage: 16,
            ..adapter.clone()
        };
        let err = check_requirements(
            "test",
            wgpu::Backend::Vulkan,
            &adapter,
            wgpu::Features::empty(),
            &wanted,
            wgpu::Features::SHADER_F16,
        )
        .unwrap_err();
        assert_eq!(
            err.missing_limits,
            vec![MissingLimit {
                name: "max_storage_buffers_per_shader_stage",
                requested: 16,
                available: u64::from(adapter.max_storage_buffers_per_shader_stage),
            }]
        );
        assert_eq!(err.missing_features, wgpu::Features::SHADER_F16);
        let msg = err.to_string();
        assert!(
            msg.contains("max_storage_buffers_per_shader_stage"),
            "{msg}"
        );
        assert!(msg.contains("SHADER_F16"), "{msg}");
    }

    #[test]
    fn satisfiable_requirements_pass() {
        let adapter = wgpu::Limits::default();
        assert!(
            check_requirements(
                "test",
                wgpu::Backend::Vulkan,
                &adapter,
                wgpu::Features::empty(),
                &wgpu::Limits::downlevel_defaults(),
                wgpu::Features::empty(),
            )
            .is_ok()
        );
    }
}
