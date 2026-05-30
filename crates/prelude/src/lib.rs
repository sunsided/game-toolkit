//! game-toolkit: a 2D-first game-jam toolkit.
//!
//! This is the umbrella entry-point crate. Depend on `game-toolkit` and write
//! `use game_toolkit::prelude::*;` to pull in the runtime types from the `game-toolkit-*`
//! crates behind one dependency. Optional features: `ui`, `aseprite`, `ecs`, `synth`,
//! `vector`.

/// Everything you need to write a game: `use game_toolkit::prelude::*;`.
pub mod prelude {
    pub use anyhow;
    pub use anyhow::Result;
    pub use winit;

    pub use game_toolkit_core::{run, AppConfig, Context, Game, GameEvent, Time};
    pub use game_toolkit_gfx::{
        transform, wgpu, BlendMode, Camera, Camera2D, Camera3D, CircleInstance, Frame, Graphics,
        Mat4, MeshId, MeshInstance, MeshVertex, Painter, SpriteInstance, TextureId, Tilemap,
    };
    pub use game_toolkit_input::{Axis, Button, Gamepad, GamepadId, Input, Key, MouseButton};
    pub use game_toolkit_audio::{Audio, SoundId};
    pub use game_toolkit_assets::Assets;

    #[cfg(feature = "ui")]
    pub use game_toolkit_ui::{self, egui, Ui};

    #[cfg(feature = "aseprite")]
    pub use game_toolkit_aseprite::{
        self, Animation, AnimationPlayer, Direction, FrameRect, SpriteSheet,
    };

    #[cfg(feature = "synth")]
    pub use game_toolkit_audio::{synthie, Synth};

    #[cfg(feature = "ecs")]
    pub use game_toolkit_ecs::{self, ChannelQueue, Vec2};

    #[cfg(feature = "vector")]
    pub use game_toolkit_gfx::vello;
}
