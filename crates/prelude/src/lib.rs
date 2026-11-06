//! game-toolkit: a game-jam toolkit for 2D and 3D.
//!
//! This is the umbrella entry-point crate. Depend on `game-toolkit` and write
//! `use game_toolkit::prelude::*;` to pull in the runtime types from the `game-toolkit-*`
//! crates behind one dependency. Optional features: `ui`, `aseprite`, `ecs`, `synth`,
//! `vector`, `crt`, `indexed`, `compute`.
//!
//! New here? The [`guide`] walks through the whole API on one page.

#![forbid(unsafe_code)]

#[cfg(feature = "aseprite")]
mod aseprite_ext;

pub mod guide;

/// GPU compute helpers (feature `compute`): [`Kernel`](compute::Kernel),
/// [`PingPong`](compute::PingPong), [`ReadbackRing`](compute::ReadbackRing),
/// [`StorageTexture`](compute::StorageTexture) and
/// [`ComputeSubmitter`](compute::ComputeSubmitter). The main types are also in the prelude.
#[cfg(feature = "compute")]
pub use game_toolkit_compute as compute;

/// Everything you need to write a game: `use game_toolkit::prelude::*;`.
pub mod prelude {
    pub use anyhow;
    pub use anyhow::Result;
    pub use winit;

    pub use game_toolkit_assets::Assets;
    pub use game_toolkit_audio::{
        Audio, AudioError, AudioStream, PlayOptions, SoundHandle, SoundId, StereoStream,
        StreamHandle, kira,
    };
    pub use game_toolkit_core::{AppConfig, Context, Game, GameEvent, Rng, Time, asset_root, run};
    pub use game_toolkit_gfx::{
        Align, BAYER_WGSL, BitmapFont, BlendMode, Camera, Camera2D, Camera3D, CircleInstance,
        Color, CustomPass, DeviceRequirementsError, Frame, Glyph, Graphics, GridLayout, Layer,
        Mat4, MeshId, MeshInstance, MeshVertex, Painter, PostEffect, PostEffectId, PostPass,
        PostStage, ScaleFilter, ScaleMode, ShaderEffect, SpriteInstance, TextureId, TextureWrap,
        Tilemap, transform, wgpu,
    };
    pub use game_toolkit_input::{
        Axis, Button, Gamepad, GamepadId, Input, Key, MouseButton, PointerMapping, Touch,
    };

    #[cfg(feature = "ui")]
    pub use game_toolkit_ui::{self, Ui, egui};

    #[cfg(feature = "aseprite")]
    pub use crate::aseprite_ext::LoadAseprite;
    #[cfg(feature = "aseprite")]
    pub use game_toolkit_aseprite::{
        self, Animation, AnimationPlayer, Direction, FrameRect, SpriteSheet,
    };

    #[cfg(feature = "synth")]
    pub use game_toolkit_audio::{Synth, SynthHandle, synth, synthie};

    #[cfg(feature = "ecs")]
    pub use game_toolkit_ecs::{self, ChannelQueue, Vec2};

    #[cfg(feature = "vector")]
    pub use game_toolkit_gfx::vello;

    #[cfg(feature = "crt")]
    pub use game_toolkit_gfx::{BloomSource, CrtEffect, CrtMask, CrtSettings};

    #[cfg(feature = "indexed")]
    pub use game_toolkit_gfx::{INDEX_WGSL, IndexedImage, Palette, PaletteCycle, PaletteCycleId};

    #[cfg(feature = "compute")]
    pub use game_toolkit_compute::{
        self, Binding, ComputeSubmitter, Kernel, PingPong, PingPongBindGroups, ReadbackRing,
        StorageTexture,
    };
}
