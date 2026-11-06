//! Graphics subsystem: wgpu init, 2D sprite batcher, primitives, text, tilemap, painter API.

#![forbid(unsafe_code)]

mod bitmap_font;
mod camera;
mod camera3d;
mod canvas;
mod color;
mod config;
#[cfg(feature = "crt")]
mod crt;
mod custom;
mod draw;
mod frame;
#[cfg(all(test, feature = "indexed", not(target_arch = "wasm32")))]
mod gpu_tests;
mod graphics;
#[cfg(feature = "indexed")]
mod indexed;
mod mesh;
mod painter;
mod post;
mod primitives;
mod shapes;
mod sprite;
mod target;
mod text;
mod texture;
mod tilemap;
pub mod transform;
#[cfg(feature = "vector")]
mod vector;

/// Re-export so downstream crates can name wgpu types (e.g. `wgpu::TextureFormat` for a
/// depth format) without adding their own wgpu dependency or risking a version mismatch.
pub use wgpu;

/// Re-export of the vello vector renderer (feature `vector`), so callers can build a
/// `vello::Scene` for [`Painter::vector`].
#[cfg(feature = "vector")]
pub use vello;

pub use bitmap_font::{BitmapFont, Glyph, GridLayout};
pub use camera::Camera2D;
pub use camera3d::{Camera, Camera3D};
pub use canvas::{CanvasRect, ScaleFilter, ScaleMode};
pub use color::{Color, Layer};
pub use config::{DeviceHook, DeviceRequirementsError, GraphicsConfig, MissingLimit};
#[cfg(feature = "crt")]
pub use crt::{BloomSource, CrtEffect, CrtMask, CrtSettings};
pub use custom::CustomPass;
pub use draw::{RectDraw, SpriteDraw, TextDraw};
pub use frame::Frame;
pub use graphics::Graphics;
#[cfg(feature = "indexed")]
pub use indexed::{
    INDEX_WGSL, IndexedImage, MAX_PALETTE_CYCLES, PALETTE_ROWS, Palette, PaletteCycle,
    PaletteCycleId,
};
pub use mesh::{MeshId, MeshInstance, MeshVertex};
pub use painter::Painter;
pub use post::{
    BAYER_WGSL, PostEffect, PostEffectId, PostGlobals, PostPass, PostStage, SHADER_PRELUDE,
    ShaderEffect,
};
pub use primitives::CircleInstance;
pub use sprite::{BlendMode, SpriteInstance};
pub use text::Align;
pub use texture::{Texture, TextureId, TextureWrap};
pub use tilemap::Tilemap;
pub use transform::Mat4;
