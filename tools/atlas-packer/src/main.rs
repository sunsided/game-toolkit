//! Offline atlas packer: takes a directory of PNGs and produces a single packed atlas PNG
//! plus a JSON sidecar describing each sub-rect.
//!
//! The JSON matches Aseprite's exported "hash" sheet format, so the result loads directly
//! with `game_toolkit_aseprite::SpriteSheet::from_aseprite_json(gfx, atlas_png, atlas_json)` -
//! no game-toolkit-specific format to teach the runtime.
//!
//! ```text
//! atlas-packer --input sprites/ --output atlas.png --metadata atlas.json [--max-size 2048] [--padding 2]
//! ```

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::Parser;
use etagere::{AtlasAllocator, size2};
use image::RgbaImage;
use serde_json::json;

#[derive(Parser)]
#[command(
    name = "atlas-packer",
    about = "Pack a directory of PNGs into a texture atlas + JSON sidecar"
)]
struct Args {
    /// Directory of input PNGs.
    #[arg(long)]
    input: PathBuf,
    /// Output atlas PNG path.
    #[arg(long)]
    output: PathBuf,
    /// Output metadata JSON path.
    #[arg(long)]
    metadata: PathBuf,
    /// Maximum atlas size (square), in pixels.
    #[arg(long, default_value_t = 2048)]
    max_size: u32,
    /// Gutter between sprites, in pixels.
    #[arg(long, default_value_t = 2)]
    padding: u32,
}

struct Sprite {
    name: String,
    image: RgbaImage,
}

struct Placed {
    src: usize,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Collect PNGs in a deterministic (sorted) order.
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&args.input)
        .with_context(|| format!("read input dir {}", args.input.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
        .collect();
    paths.sort();

    let mut sprites = Vec::with_capacity(paths.len());
    for path in paths {
        let image = image::open(&path)
            .with_context(|| format!("open {}", path.display()))?
            .to_rgba8();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("sprite")
            .to_string();
        sprites.push(Sprite { name, image });
    }
    if sprites.is_empty() {
        bail!("no PNG files found in {}", args.input.display());
    }

    // Pack largest area first for a tighter result.
    let mut order: Vec<usize> = (0..sprites.len()).collect();
    order.sort_by_key(|&i| {
        let (w, h) = sprites[i].image.dimensions();
        std::cmp::Reverse(w as u64 * h as u64)
    });

    let mut atlas = AtlasAllocator::new(size2(args.max_size as i32, args.max_size as i32));
    let pad = args.padding as i32;
    let mut placed: Vec<Placed> = Vec::with_capacity(sprites.len());
    let (mut used_w, mut used_h) = (0u32, 0u32);
    for &src in &order {
        let (w, h) = sprites[src].image.dimensions();
        let alloc = atlas
            .allocate(size2(w as i32 + pad, h as i32 + pad))
            .with_context(|| {
                format!(
                    "'{}' does not fit; increase --max-size (currently {})",
                    sprites[src].name, args.max_size
                )
            })?;
        let x = alloc.rectangle.min.x as u32;
        let y = alloc.rectangle.min.y as u32;
        used_w = used_w.max(x + w);
        used_h = used_h.max(y + h);
        placed.push(Placed { src, x, y, w, h });
    }

    // Blit each sprite into the atlas (straight copy, preserving alpha).
    let mut out = RgbaImage::new(used_w, used_h);
    for p in &placed {
        image::imageops::replace(&mut out, &sprites[p.src].image, p.x as i64, p.y as i64);
    }
    out.save(&args.output)
        .with_context(|| format!("write atlas {}", args.output.display()))?;

    // Aseprite-compatible "hash" sheet JSON (BTreeMap-backed, so frame keys are sorted).
    let atlas_name = args
        .output
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("atlas.png");
    let mut frames = serde_json::Map::new();
    for p in &placed {
        frames.insert(
            sprites[p.src].name.clone(),
            json!({
                "frame": { "x": p.x, "y": p.y, "w": p.w, "h": p.h },
                "rotated": false,
                "trimmed": false,
                "sourceSize": { "w": p.w, "h": p.h },
                "duration": 100
            }),
        );
    }
    let meta = json!({
        "frames": serde_json::Value::Object(frames),
        "meta": {
            "app": "atlas-packer",
            "image": atlas_name,
            "format": "RGBA8888",
            "size": { "w": used_w, "h": used_h }
        }
    });
    std::fs::write(&args.metadata, serde_json::to_string_pretty(&meta)?)
        .with_context(|| format!("write metadata {}", args.metadata.display()))?;

    println!(
        "packed {} sprites into {} ({}x{}) + {}",
        placed.len(),
        args.output.display(),
        used_w,
        used_h,
        args.metadata.display()
    );
    Ok(())
}
