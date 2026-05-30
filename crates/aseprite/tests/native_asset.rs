//! Validates the checked-in native `.aseprite` example asset against the exact `ah-asefile`
//! API surface `SpriteSheet::load_aseprite` depends on. Pure parse - no GPU needed.

use std::path::PathBuf;

use ah_asefile::{AnimationDirection, AsepriteFile};

fn asset() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/09_aseprite/assets/character.aseprite")
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
