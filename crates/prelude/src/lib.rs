//! game-toolkit: a game-jam toolkit for 2D and 3D.
//!
//! This is the umbrella entry-point crate. Depend on `game-toolkit` and write
//! `use game_toolkit::prelude::*;` to pull in the runtime types from the `game-toolkit-*`
//! crates behind one dependency. Optional features: `ui`, `aseprite`, `ecs`, `synth`,
//! `vector`.
//!
//! New here? The [`guide`] walks through the whole API on one page.

#![forbid(unsafe_code)]

#[cfg(feature = "aseprite")]
mod aseprite_ext;

pub mod guide;

/// Everything you need to write a game: `use game_toolkit::prelude::*;`.
pub mod prelude {
    pub use anyhow;
    pub use anyhow::Result;
    pub use winit;

    pub use game_toolkit_assets::Assets;
    pub use game_toolkit_audio::{Audio, AudioError, PlayOptions, SoundHandle, SoundId, kira};
    pub use game_toolkit_core::{AppConfig, Context, Game, GameEvent, Rng, Time, asset_root, run};
    pub use game_toolkit_gfx::{
        Align, BlendMode, Camera, Camera2D, Camera3D, CircleInstance, Color, Frame, Graphics,
        Layer, Mat4, MeshId, MeshInstance, MeshVertex, Painter, SpriteInstance, TextureId,
        TextureWrap, Tilemap, transform, wgpu,
    };
    pub use game_toolkit_input::{Axis, Button, Gamepad, GamepadId, Input, Key, MouseButton};

    #[cfg(feature = "ui")]
    pub use game_toolkit_ui::{self, Ui, egui};

    #[cfg(feature = "aseprite")]
    pub use crate::aseprite_ext::LoadAseprite;
    #[cfg(feature = "aseprite")]
    pub use game_toolkit_aseprite::{
        self, Animation, AnimationPlayer, Direction, FrameRect, SpriteSheet,
    };

    #[cfg(feature = "synth")]
    pub use game_toolkit_audio::{Synth, synthie};

    #[cfg(feature = "ecs")]
    pub use game_toolkit_ecs::{self, ChannelQueue, Vec2};

    #[cfg(feature = "vector")]
    pub use game_toolkit_gfx::vello;
}
