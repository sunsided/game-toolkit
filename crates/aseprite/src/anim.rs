//! Time-driven playback of a single [`Animation`] tag over a [`SpriteSheet`].

use crate::sheet::SpriteSheet;
use crate::tag::{Animation, Direction};

/// Advances one tag's frames using each frame's own duration.
///
/// ```ignore
/// let mut player = AnimationPlayer::new(&sheet, "walk").unwrap();
/// // each frame:
/// player.advance(&sheet, dt);
/// let (uv_min, uv_max) = player.current_uv(&sheet);
/// painter.sprite_ex(sheet.texture, SpriteInstance::at(pos, size).with_uv(uv_min, uv_max), 0, BlendMode::Alpha);
/// ```
pub struct AnimationPlayer {
    anim: Animation,
    /// Offset within the tag span, `0..anim.len()`.
    local: usize,
    /// Seconds spent on the current frame.
    elapsed: f32,
    /// Ping-pong sweep direction; unused for forward/reverse.
    forward: bool,
}

impl AnimationPlayer {
    /// Create a player for tag `name`, or `None` if the sheet has no such tag.
    pub fn new(sheet: &SpriteSheet, name: &str) -> Option<Self> {
        Some(Self::from_animation(sheet.animation(name)?.clone()))
    }

    /// Create a player directly from an [`Animation`] (e.g. [`SpriteSheet::full_animation`]
    /// for sheets without tags).
    pub fn from_animation(anim: Animation) -> Self {
        let (local, forward) = match anim.direction {
            Direction::Reverse => (anim.len().saturating_sub(1), false),
            _ => (0, true),
        };
        Self {
            anim,
            local,
            elapsed: 0.0,
            forward,
        }
    }

    /// Global frame index of the frame currently showing.
    pub fn current_index(&self) -> usize {
        self.anim.from + self.local
    }

    /// UV rect of the frame currently showing, clamped to the sheet's frame range.
    pub fn current_uv(&self, sheet: &SpriteSheet) -> ([f32; 2], [f32; 2]) {
        let last = sheet.frames.len().saturating_sub(1);
        let frame = &sheet.frames[self.current_index().min(last)];
        (frame.uv_min, frame.uv_max)
    }

    /// Accumulate `dt` seconds, stepping through as many frames as their durations allow.
    pub fn advance(&mut self, sheet: &SpriteSheet, dt: f32) {
        let len = self.anim.len();
        if len <= 1 {
            return;
        }
        self.elapsed += dt;
        // Guard against a zero-duration frame spinning the loop forever.
        loop {
            let dur = sheet
                .frames
                .get(self.current_index())
                .map(|f| f.duration)
                .unwrap_or(0.1)
                .max(1e-4);
            if self.elapsed < dur {
                break;
            }
            self.elapsed -= dur;
            self.step(len);
        }
    }

    fn step(&mut self, len: usize) {
        match self.anim.direction {
            Direction::Forward => self.local = (self.local + 1) % len,
            Direction::Reverse => {
                self.local = if self.local == 0 { len - 1 } else { self.local - 1 }
            }
            Direction::PingPong => {
                if self.forward {
                    if self.local + 1 >= len {
                        self.forward = false;
                        self.local = len.saturating_sub(2);
                    } else {
                        self.local += 1;
                    }
                } else if self.local == 0 {
                    self.forward = true;
                    self.local = (len > 1) as usize;
                } else {
                    self.local -= 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use toolkit_gfx::TextureId;

    use super::AnimationPlayer;
    use crate::sheet::{FrameRect, SpriteSheet};
    use crate::tag::{Animation, Direction};

    /// A sheet of `n` frames, each lasting `dur` seconds, tagged "t" with `dir`.
    fn sheet(n: usize, dur: f32, dir: Direction) -> SpriteSheet {
        let frames = (0..n)
            .map(|_| FrameRect {
                uv_min: [0.0, 0.0],
                uv_max: [1.0, 1.0],
                size: [1, 1],
                duration: dur,
            })
            .collect();
        let mut animations = HashMap::new();
        animations.insert(
            "t".to_string(),
            Animation {
                name: "t".to_string(),
                from: 0,
                to: n - 1,
                direction: dir,
            },
        );
        SpriteSheet {
            texture: TextureId(0),
            size: [1, 1],
            frames,
            animations,
        }
    }

    #[test]
    fn forward_wraps() {
        let s = sheet(3, 0.1, Direction::Forward);
        let mut p = AnimationPlayer::new(&s, "t").unwrap();
        assert_eq!(p.current_index(), 0);
        p.advance(&s, 0.1);
        assert_eq!(p.current_index(), 1);
        p.advance(&s, 0.1);
        assert_eq!(p.current_index(), 2);
        p.advance(&s, 0.1);
        assert_eq!(p.current_index(), 0);
    }

    #[test]
    fn dt_below_duration_holds_frame() {
        let s = sheet(3, 0.1, Direction::Forward);
        let mut p = AnimationPlayer::new(&s, "t").unwrap();
        p.advance(&s, 0.05);
        assert_eq!(p.current_index(), 0);
        p.advance(&s, 0.05); // accumulates to one full frame
        assert_eq!(p.current_index(), 1);
    }

    #[test]
    fn duration_rollover_steps_multiple_frames() {
        let s = sheet(4, 0.1, Direction::Forward);
        let mut p = AnimationPlayer::new(&s, "t").unwrap();
        p.advance(&s, 0.25); // two full frames consumed, 0.05 left over
        assert_eq!(p.current_index(), 2);
    }

    #[test]
    fn reverse_starts_at_end_and_wraps() {
        let s = sheet(3, 0.1, Direction::Reverse);
        let mut p = AnimationPlayer::new(&s, "t").unwrap();
        assert_eq!(p.current_index(), 2);
        p.advance(&s, 0.1);
        assert_eq!(p.current_index(), 1);
        p.advance(&s, 0.1);
        assert_eq!(p.current_index(), 0);
        p.advance(&s, 0.1);
        assert_eq!(p.current_index(), 2);
    }

    #[test]
    fn pingpong_bounces_at_endpoints() {
        let s = sheet(3, 0.1, Direction::PingPong);
        let mut p = AnimationPlayer::new(&s, "t").unwrap();
        let seq: Vec<usize> = (0..6)
            .map(|_| {
                let i = p.current_index();
                p.advance(&s, 0.1);
                i
            })
            .collect();
        assert_eq!(seq, vec![0, 1, 2, 1, 0, 1]);
    }

    #[test]
    fn single_frame_tag_is_stable() {
        let s = sheet(1, 0.1, Direction::Forward);
        let mut p = AnimationPlayer::new(&s, "t").unwrap();
        p.advance(&s, 1.0);
        assert_eq!(p.current_index(), 0);
    }
}
