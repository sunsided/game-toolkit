//! Audio subsystem built on [`kira`].
//!
//! Minimal first-cut API: load static sound buffers from disk and play them on the main track.
//! Mixer tracks, tweens, spatial audio and streaming are intentionally deferred.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use kira::manager::backend::DefaultBackend;
use kira::manager::{AudioManager, AudioManagerSettings};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use kira::tween::Tween;
use kira::Frame;

#[cfg(feature = "synth")]
mod synth;
#[cfg(feature = "synth")]
pub use synth::Synth;
#[cfg(feature = "synth")]
pub use synthie;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SoundId(pub u32);

pub struct Audio {
    manager: AudioManager<DefaultBackend>,
    sounds: HashMap<SoundId, StaticSoundData>,
    next_id: u32,
}

impl Audio {
    pub fn new() -> Result<Self> {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|e| anyhow!("kira AudioManager init: {e}"))?;
        Ok(Self {
            manager,
            sounds: HashMap::new(),
            next_id: 1,
        })
    }

    pub fn load_sound(&mut self, path: impl AsRef<Path>) -> Result<SoundId> {
        let data = StaticSoundData::from_file(path.as_ref())
            .map_err(|e| anyhow!("kira load {}: {e}", path.as_ref().display()))?;
        let id = SoundId(self.next_id);
        self.next_id += 1;
        self.sounds.insert(id, data);
        Ok(id)
    }

    pub fn play(&mut self, id: SoundId) -> Result<StaticSoundHandle> {
        let data = self
            .sounds
            .get(&id)
            .ok_or_else(|| anyhow!("unknown SoundId {id:?}"))?
            .clone();
        self.manager
            .play(data)
            .map_err(|e| anyhow!("kira play: {e}"))
    }

    /// Play mono PCM samples (`-1.0..=1.0`) directly, e.g. a buffer rendered by
    /// [`Synth`](crate::Synth). The samples are duplicated to both channels.
    pub fn play_samples(&mut self, samples: &[f32], sample_rate: u32) -> Result<StaticSoundHandle> {
        let frames: Arc<[Frame]> = samples
            .iter()
            .map(|&s| Frame { left: s, right: s })
            .collect();
        let data = StaticSoundData {
            sample_rate,
            frames,
            settings: StaticSoundSettings::default(),
            slice: None,
        };
        self.manager
            .play(data)
            .map_err(|e| anyhow!("kira play: {e}"))
    }

    /// Master volume on the default main track. `1.0` is unity gain.
    pub fn set_master_volume(&mut self, volume: f32) {
        self.manager
            .main_track()
            .set_volume(volume as f64, Tween::default());
    }
}
