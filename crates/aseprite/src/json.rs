//! Deserialization of Aseprite's exported sprite-sheet JSON sidecar.
//!
//! Aseprite emits `frames` either as a JSON object keyed by frame name (the "hash" layout)
//! or as an array (the "array" layout). Both are accepted; an [`IndexMap`] preserves the
//! document order of the hash layout so animation tag indices stay correct.

use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::Deserialize;

use crate::tag::{Animation, Direction};

/// Pixel rectangle of one frame inside the atlas: `(x, y, w, h)`.
pub(crate) type FrameRectPx = (u32, u32, u32, u32);

/// Parsed sprite-sheet metadata, normalized away from the on-disk JSON shape.
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
    let frames = match doc.frames {
        Frames::Hash(map) => map.into_values().map(to_frame).collect(),
        Frames::List(list) => list.into_iter().map(to_frame).collect(),
    };
    let tags = doc
        .meta
        .frame_tags
        .into_iter()
        .map(|t| Animation {
            from: t.from,
            to: t.to,
            direction: match t.direction.as_str() {
                "reverse" => Direction::Reverse,
                "pingpong" => Direction::PingPong,
                _ => Direction::Forward,
            },
            name: t.name,
        })
        .collect();
    Ok(ParsedSheet {
        sheet_size: (doc.meta.size.w, doc.meta.size.h),
        frames,
        tags,
    })
}

fn to_frame(e: FrameEntry) -> (FrameRectPx, u32) {
    ((e.frame.x, e.frame.y, e.frame.w, e.frame.h), e.duration)
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
}
