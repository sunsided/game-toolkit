//! Animation tag model shared by the native and JSON loaders.

/// Playback direction for a tagged animation, mirroring Aseprite's tag directions.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Count up from `from` to `to`, then wrap.
    Forward,
    /// Count down from `to` to `from`, then wrap.
    Reverse,
    /// Count up to `to`, then back down to `from`, repeating.
    PingPong,
}

/// A named animation: an inclusive range of frame indices plus a playback direction.
///
/// `from` and `to` index into [`crate::SpriteSheet::frames`].
#[derive(Clone, Debug)]
pub struct Animation {
    pub name: String,
    /// First frame index (inclusive).
    pub from: usize,
    /// Last frame index (inclusive).
    pub to: usize,
    pub direction: Direction,
}

impl Animation {
    /// Number of frames spanned by the tag (always at least 1 for a valid tag).
    pub fn len(&self) -> usize {
        self.to.saturating_sub(self.from) + 1
    }

    /// A tag always spans at least its single `from` frame; provided for lint symmetry.
    pub fn is_empty(&self) -> bool {
        self.to < self.from
    }
}
