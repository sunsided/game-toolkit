//! Validates a checked-in native `.aseprite` fixture against the exact `ah-asefile` API
//! surface `SpriteSheet::load_aseprite` depends on. Pure parse - no GPU needed.
//!
//! The fixture lives under this crate (`tests/data/`) so the crate's tests stay
//! self-contained in a packaged or standalone checkout.

use std::path::PathBuf;

use ah_asefile::{AnimationDirection, AsepriteFile};

fn asset() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/character.aseprite")
}

#[test]
fn reads_frames_and_walk_tag() {
    let ase = AsepriteFile::read_file(&asset()).expect("read character.aseprite");
    assert_eq!(ase.size(), (32, 32));
    assert_eq!(ase.num_frames(), 6);
    // Every frame was authored at 120 ms.
    for i in 0..ase.num_frames() {
        assert_eq!(ase.frame(i).duration(), 120, "frame {i} duration");
    }
    assert_eq!(ase.num_tags(), 1);
    let tag = ase.tag(0);
    assert_eq!(tag.name(), "walk");
    assert_eq!((tag.from_frame(), tag.to_frame()), (0, 5));
    assert_eq!(tag.animation_direction(), AnimationDirection::PingPong);
}
