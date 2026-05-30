//! Curated re-exports. `use toolkit_prelude::*;` and go.

pub use anyhow;
pub use anyhow::Result;
pub use winit;

pub use toolkit_core::{run, AppConfig, Context, Game, GameEvent, Time};
pub use toolkit_gfx::{
    transform, wgpu, BlendMode, Camera, Camera2D, Camera3D, CircleInstance, Frame, Graphics, Mat4,
    MeshId, MeshInstance, MeshVertex, Painter, SpriteInstance, TextureId, Tilemap,
};
pub use toolkit_input::{Axis, Button, Gamepad, GamepadId, Input, Key, MouseButton};
pub use toolkit_audio::{Audio, SoundId};
pub use toolkit_assets::Assets;

#[cfg(feature = "ui")]
pub use toolkit_ui::{self, egui, Ui};

#[cfg(feature = "aseprite")]
pub use toolkit_aseprite::{self, Animation, AnimationPlayer, Direction, FrameRect, SpriteSheet};
