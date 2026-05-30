//! Deserialization of Aseprite's exported sprite-sheet JSON sidecar.
//!
//! Aseprite emits `frames` either as a JSON object keyed by frame name (the "hash" layout)
//! or as an array (the "array" layout). Both are accepted; an [`IndexMap`] preserves the
//! document order of the hash layout so animation tag indices stay correct.
//!
//! Packing options this loader cannot represent (rotated or trimmed frames) and tag
//! directions it does not model are rejected with an error rather than mis-rendered.

use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use serde::Deserialize;

use crate::tag::{Animation, Direction};

/// Pixel rectangle of one frame inside the atlas: `(x, y, w, h)`.
pub(crate) type FrameRectPx = (u32, u32, u32, u32);

/// Parsed sprite-sheet metadata, normalized away from the on-disk JSON shape.
#[derive(Debug)]
pub(crate) struct ParsedSheet {
    pub sheet_size: (u32, u32),
    /// One entry per frame, in playback order: pixel rect + duration in milliseconds.
    pub frames: Vec<(FrameRectPx, u32)>,
    pub tags: Vec<Animation>,
}

#[derive(Deserialize)]
struct AseJson {
    frames: Frames,
    meta: Meta,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Frames {
    Hash(IndexMap<String, FrameEntry>),
    List(Vec<FrameEntry>),
}

#[derive(Deserialize)]
struct FrameEntry {
    frame: Rect,
    #[serde(default = "default_duration")]
    duration: u32,
    #[serde(default)]
    rotated: bool,
    #[serde(default)]
    trimmed: bool,
}

#[derive(Deserialize)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[derive(Deserialize)]
struct Meta {
    size: Size,
    #[serde(default, rename = "frameTags")]
    frame_tags: Vec<TagEntry>,
}

#[derive(Deserialize)]
struct Size {
    w: u32,
    h: u32,
}

#[derive(Deserialize)]
struct TagEntry {
    name: String,
    from: usize,
    to: usize,
    #[serde(default)]
    direction: String,
}

fn default_duration() -> u32 {
    100
}

pub(crate) fn parse(bytes: &[u8]) -> Result<ParsedSheet> {
    let doc: AseJson = serde_json::from_slice(bytes).context("parse Aseprite JSON")?;
    let entries: Vec<FrameEntry> = match doc.frames {
        Frames::Hash(map) => map.into_values().collect(),
        Frames::List(list) => list,
    };
    let frames = entries
        .into_iter()
        .enumerate()
        .map(|(i, e)| to_frame(i, e))
        .collect::<Result<Vec<_>>>()?;
    let tags = doc
        .meta
        .frame_tags
        .into_iter()
        .map(parse_tag)
        .collect::<Result<Vec<_>>>()?;
    Ok(ParsedSheet {
        sheet_size: (doc.meta.size.w, doc.meta.size.h),
        frames,
        tags,
    })
}

/// Reject packing options the UV-rect model cannot represent rather than mis-render them:
/// a rotated frame would need a rotated quad, and a trimmed frame's atlas rect is smaller
/// than and offset from the source, so it would draw cropped and mispositioned.
fn to_frame(index: usize, e: FrameEntry) -> Result<(FrameRectPx, u32)> {
    if e.rotated {
        bail!("frame {index} is rotated in the atlas; re-export with rotation disabled");
    }
    if e.trimmed {
        bail!("frame {index} is trimmed; re-export with trim disabled");
    }
    Ok(((e.frame.x, e.frame.y, e.frame.w, e.frame.h), e.duration))
}

fn parse_tag(t: TagEntry) -> Result<Animation> {
    // Missing direction defaults to forward; an unrecognized value (e.g. a reverse
    // ping-pong export this enum does not model) is an error, not a silent fallback.
    let direction = match t.direction.as_str() {
        "" | "forward" => Direction::Forward,
        "reverse" => Direction::Reverse,
        "pingpong" => Direction::PingPong,
        other => bail!("tag {:?} has unsupported direction {other:?}", t.name),
    };
    Ok(Animation {
        name: t.name,
        from: t.from,
        to: t.to,
        direction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = r#"{
        "frames": {
            "walk 0.aseprite": { "frame": {"x":0,"y":0,"w":16,"h":16}, "duration": 120 },
            "walk 1.aseprite": { "frame": {"x":16,"y":0,"w":16,"h":16}, "duration": 80 }
        },
        "meta": {
            "size": {"w":32,"h":16},
            "frameTags": [ {"name":"walk","from":0,"to":1,"direction":"pingpong"} ]
        }
    }"#;

    const ARRAY: &str = r#"{
        "frames": [
            { "filename":"a", "frame": {"x":0,"y":0,"w":16,"h":16}, "duration": 100 },
            { "filename":"b", "frame": {"x":16,"y":0,"w":16,"h":16}, "duration": 100 }
        ],
        "meta": { "size": {"w":32,"h":16} }
    }"#;

    #[test]
    fn parses_hash_layout_in_order() {
        let p = parse(HASH.as_bytes()).unwrap();
        assert_eq!(p.sheet_size, (32, 16));
        assert_eq!(p.frames.len(), 2);
        assert_eq!(p.frames[0], ((0, 0, 16, 16), 120));
        assert_eq!(p.frames[1], ((16, 0, 16, 16), 80));
        assert_eq!(p.tags.len(), 1);
        assert_eq!(p.tags[0].direction, Direction::PingPong);
        assert_eq!((p.tags[0].from, p.tags[0].to), (0, 1));
    }

    #[test]
    fn parses_array_layout_without_tags() {
        let p = parse(ARRAY.as_bytes()).unwrap();
        assert_eq!(p.frames.len(), 2);
        assert!(p.tags.is_empty());
    }

    #[test]
    fn rejects_rotated_frame() {
        let json = r#"{"frames":[{"frame":{"x":0,"y":0,"w":8,"h":8},"rotated":true}],
                       "meta":{"size":{"w":8,"h":8}}}"#;
        let err = parse(json.as_bytes()).unwrap_err().to_string();
        assert!(err.contains("rotated"), "{err}");
    }

    #[test]
    fn rejects_trimmed_frame() {
        let json = r#"{"frames":[{"frame":{"x":0,"y":0,"w":8,"h":8},"trimmed":true}],
                       "meta":{"size":{"w":8,"h":8}}}"#;
        let err = parse(json.as_bytes()).unwrap_err().to_string();
        assert!(err.contains("trimmed"), "{err}");
    }

    #[test]
    fn rejects_unknown_direction() {
        let json = r#"{"frames":[{"frame":{"x":0,"y":0,"w":8,"h":8}}],
                       "meta":{"size":{"w":8,"h":8},
                       "frameTags":[{"name":"t","from":0,"to":0,"direction":"pingpong_reverse"}]}}"#;
        let err = parse(json.as_bytes()).unwrap_err().to_string();
        assert!(err.contains("unsupported direction"), "{err}");
    }
}
