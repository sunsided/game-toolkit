//! Graphics subsystem: wgpu init, 2D sprite batcher, primitives, text, tilemap, painter API.

#![deny(unsafe_code)]

mod camera;
mod frame;
mod graphics;
mod painter;
mod primitives;
mod sprite;
mod text;
mod texture;
mod tilemap;

pub use camera::Camera2D;
pub use frame::Frame;
pub use graphics::Graphics;
pub use painter::Painter;
pub use primitives::CircleInstance;
pub use sprite::{BlendMode, SpriteInstance};
pub use texture::{Texture, TextureId};
pub use tilemap::Tilemap;
