//! Runtime sprite-sheet: a single atlas texture plus per-frame UV rects and named tags.

use std::collections::HashMap;
use std::path::Path;

use ah_asefile::{AnimationDirection, AsepriteFile};
use anyhow::{anyhow, Context, Result};
use image::RgbaImage;
use game_toolkit_gfx::{Graphics, TextureId};

use crate::json;
use crate::tag::{Animation, Direction};

/// One frame's placement inside the atlas, ready to feed a sprite's UV rect.
#[derive(Copy, Clone, Debug)]
pub struct FrameRect {
    pub uv_min: [f32; 2],
    pub uv_max: [f32; 2],
    /// Frame size in pixels.
    pub size: [u32; 2],
    /// Frame duration in seconds.
    pub duration: f32,
}

/// A loaded sprite sheet: one atlas [`TextureId`], its frames as UV rects, and the
/// animations parsed from Aseprite tags (keyed by tag name).
pub struct SpriteSheet {
    pub texture: TextureId,
    /// Atlas texture size in pixels.
    pub size: [u32; 2],
    pub frames: Vec<FrameRect>,
    pub animations: HashMap<String, Animation>,
}

impl SpriteSheet {
    /// Load a native `.aseprite`/`.ase` file: flatten every frame, pack them into a single
    /// grid atlas, upload it, and lift the tags into [`Animation`]s.
    pub fn load_aseprite(gfx: &mut Graphics, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let ase = AsepriteFile::read_file(path)
            .map_err(|e| anyhow!("read aseprite {}: {e}", path.display()))?;

        let fw = ase.width() as u32;
        let fh = ase.height() as u32;
        let n = ase.num_frames();
        if n == 0 || fw == 0 || fh == 0 {
            return Err(anyhow!("{} has no drawable frames", path.display()));
        }

        // All native frames share the canvas size, so a uniform grid packs them tightly.
        let cols = (n as f64).sqrt().ceil() as u32;
        let cols = cols.max(1);
        let rows = n.div_ceil(cols);
        let atlas_w = cols * fw;
        let atlas_h = rows * fh;

        let mut atlas = vec![0u8; (atlas_w * atlas_h * 4) as usize];
        let mut frames = Vec::with_capacity(n as usize);
        for i in 0..n {
            let frame = ase.frame(i);
            let img = frame.image();
            let cx = (i % cols) * fw;
            let cy = (i / cols) * fh;
            blit(&mut atlas, atlas_w, &img, cx, cy);
            frames.push(uv_frame(cx, cy, fw, fh, atlas_w, atlas_h, frame.duration()));
        }

        let texture = gfx.create_texture_rgba(atlas_w, atlas_h, &atlas, Some("aseprite.atlas"));

        let mut animations = HashMap::new();
        for t in 0..ase.num_tags() {
            let tag = ase.tag(t);
            let direction = match tag.animation_direction() {
                AnimationDirection::Forward => Direction::Forward,
                AnimationDirection::Reverse => Direction::Reverse,
                AnimationDirection::PingPong => Direction::PingPong,
            };
            let name = tag.name().to_string();
            animations.insert(
                name.clone(),
                Animation {
                    name,
                    from: tag.from_frame() as usize,
                    to: tag.to_frame() as usize,
                    direction,
                },
            );
        }

        Ok(Self {
            texture,
            size: [atlas_w, atlas_h],
            frames,
            animations,
        })
    }

    /// Load an exported sprite-sheet: a PNG atlas plus its Aseprite JSON sidecar. The PNG is
    /// uploaded as-is and frame UVs are computed from the JSON pixel rects.
    pub fn from_aseprite_json(
        gfx: &mut Graphics,
        png_path: impl AsRef<Path>,
        json_path: impl AsRef<Path>,
    ) -> Result<Self> {
        let json_path = json_path.as_ref();
        let bytes = std::fs::read(json_path)
            .with_context(|| format!("read {}", json_path.display()))?;
        let parsed = json::parse(&bytes)?;
        let (sw, sh) = parsed.sheet_size;
        if sw == 0 || sh == 0 {
            return Err(anyhow!("{} reports a zero-size sheet", json_path.display()));
        }
        // Reject an empty sheet before uploading: a frameless SpriteSheet would panic the
        // moment anything indexes `frames[0]` (e.g. AnimationPlayer::current_uv).
        if parsed.frames.is_empty() {
            return Err(anyhow!("{} contains no frames", json_path.display()));
        }

        let texture = gfx.load_texture(png_path.as_ref())?;
        let frames = parsed
            .frames
            .into_iter()
            .map(|((x, y, w, h), dur)| uv_frame(x, y, w, h, sw, sh, dur))
            .collect();
        let animations = parsed
            .tags
            .into_iter()
            .map(|a| (a.name.clone(), a))
            .collect();

        Ok(Self {
            texture,
            size: [sw, sh],
            frames,
            animations,
        })
    }

    /// Frame count.
    pub fn num_frames(&self) -> usize {
        self.frames.len()
    }

    /// Look up a single frame's placement.
    pub fn frame(&self, index: usize) -> Option<&FrameRect> {
        self.frames.get(index)
    }

    /// Look up an animation by tag name.
    pub fn animation(&self, name: &str) -> Option<&Animation> {
        self.animations.get(name)
    }

    /// A forward animation spanning every frame, for sheets that carry no tags.
    pub fn full_animation(&self) -> Animation {
        Animation {
            name: "<all>".to_string(),
            from: 0,
            to: self.num_frames().saturating_sub(1),
            direction: Direction::Forward,
        }
    }
}

/// Build a [`FrameRect`] for a frame at pixel `(x, y, w, h)` in an atlas of `(aw, ah)`.
fn uv_frame(x: u32, y: u32, w: u32, h: u32, aw: u32, ah: u32, duration_ms: u32) -> FrameRect {
    let aw = aw as f32;
    let ah = ah as f32;
    FrameRect {
        uv_min: [x as f32 / aw, y as f32 / ah],
        uv_max: [(x + w) as f32 / aw, (y + h) as f32 / ah],
        size: [w, h],
        duration: duration_ms as f32 / 1000.0,
    }
}

/// Copy `src` into `dst` (an RGBA8 buffer `dst_w` pixels wide) at top-left `(ox, oy)`.
fn blit(dst: &mut [u8], dst_w: u32, src: &RgbaImage, ox: u32, oy: u32) {
    let (sw, sh) = src.dimensions();
    let src = src.as_raw();
    let row_bytes = (sw * 4) as usize;
    for row in 0..sh {
        let s = (row * sw * 4) as usize;
        let d = (((oy + row) * dst_w + ox) * 4) as usize;
        dst[d..d + row_bytes].copy_from_slice(&src[s..s + row_bytes]);
    }
}
