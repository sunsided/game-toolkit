//! Graphics subsystem: wgpu init, 2D sprite batcher, primitives, text, tilemap, painter API.

mod camera;
mod frame;
mod graphics;
mod painter;
mod primitives;
mod sprite;
mod target;
mod text;
mod texture;
mod tilemap;

/// Re-export so downstream crates can name wgpu types (e.g. `wgpu::TextureFormat` for a
/// depth format) without adding their own wgpu dependency or risking a version mismatch.
pub use wgpu;

pub use camera::Camera2D;
pub use frame::Frame;
pub use graphics::Graphics;
pub use painter::Painter;
pub use primitives::CircleInstance;
pub use sprite::{BlendMode, SpriteInstance};
pub use texture::{Texture, TextureId};
pub use tilemap::Tilemap;
