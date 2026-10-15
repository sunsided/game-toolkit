//! `ctx.load_aseprite(..)`: asset-root loading for sprite sheets, on every platform.
//!
//! Lives here rather than in core because core must not depend on the aseprite crate.

use std::path::Path;

use anyhow::Result;
use game_toolkit_aseprite::SpriteSheet;
use game_toolkit_core::Context;

/// Load Aseprite sprite sheets through [`Context::assets`], so the same call works natively
/// and on the web (where assets are preloaded into memory).
pub trait LoadAseprite {
    /// Load a native `.aseprite`/`.ase` file from the asset root.
    fn load_aseprite(&mut self, rel: impl AsRef<Path>) -> Result<SpriteSheet>;

    /// Load an exported sheet (PNG atlas + Aseprite JSON sidecar) from the asset root.
    fn load_aseprite_json(
        &mut self,
        png: impl AsRef<Path>,
        json: impl AsRef<Path>,
    ) -> Result<SpriteSheet>;
}

impl LoadAseprite for Context {
    fn load_aseprite(&mut self, rel: impl AsRef<Path>) -> Result<SpriteSheet> {
        let rel = rel.as_ref();
        let bytes = self.assets.read(rel)?;
        SpriteSheet::from_aseprite_bytes(&mut self.gfx, &bytes, &rel.to_string_lossy())
    }

    fn load_aseprite_json(
        &mut self,
        png: impl AsRef<Path>,
        json: impl AsRef<Path>,
    ) -> Result<SpriteSheet> {
        let png_bytes = self.assets.read(png.as_ref())?;
        let json_bytes = self.assets.read(json.as_ref())?;
        SpriteSheet::from_aseprite_json_bytes(
            &mut self.gfx,
            &png_bytes,
            &json_bytes,
            &png.as_ref().to_string_lossy(),
        )
    }
}
