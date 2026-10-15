//! Aseprite loading for the toolkit.
//!
//! Two entry points produce the same GPU-ready [`SpriteSheet`]:
//!
//! - [`SpriteSheet::from_aseprite_bytes`] reads a native `.aseprite`/`.ase` binary (via
//!   [`ah_asefile`]), flattens every frame and packs them into a single atlas texture.
//! - [`SpriteSheet::from_aseprite_json_bytes`] reads an exported sprite-sheet: a PNG atlas plus
//!   its Aseprite JSON sidecar (both the hash and array `frames` layouts are accepted).
//!
//! Natively, `SpriteSheet::load_aseprite` / `SpriteSheet::from_aseprite_json` take file paths
//! instead; the toolkit prelude's `ctx.load_aseprite*` read from the asset root on every
//! platform, including the web.
//!
//! Both expose frames as UV rects plus named [`Animation`]s lifted from Aseprite tags, and
//! [`AnimationPlayer`] advances a tag over time.
//!
//! The dependency runs one way: this crate depends on `game-toolkit-gfx`, never the reverse, so
//! the renderer stays unaware of asset formats.

#![forbid(unsafe_code)]

mod anim;
mod json;
mod sheet;
mod tag;

pub use anim::AnimationPlayer;
pub use sheet::{FrameRect, SpriteSheet};
pub use tag::{Animation, Direction};
