//! Graphics subsystem: wgpu init, 2D sprite batcher, primitives, text, tilemap, painter API.

mod camera;
mod camera3d;
mod frame;
mod graphics;
mod mesh;
mod painter;
mod primitives;
mod sprite;
mod target;
mod text;
mod texture;
mod tilemap;
pub mod transform;

/// Re-export so downstream crates can name wgpu types (e.g. `wgpu::TextureFormat` for a
/// depth format) without adding their own wgpu dependency or risking a version mismatch.
pub use wgpu;

pub use camera::Camera2D;
pub use camera3d::{Camera, Camera3D};
pub use frame::Frame;
pub use graphics::Graphics;
pub use mesh::{MeshId, MeshInstance, MeshVertex};
pub use painter::Painter;
pub use primitives::CircleInstance;
pub use sprite::{BlendMode, SpriteInstance};
pub use texture::{Texture, TextureId};
pub use tilemap::Tilemap;
pub use transform::Mat4;
