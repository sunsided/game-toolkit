//! Audio subsystem built on [`kira`].
//!
//! Minimal first-cut API: load static sound buffers (from disk or bytes) and play them on the
//! main track. Mixer tracks, tweens, spatial audio and streaming are intentionally deferred.
//!
//! On the web, browsers keep an `AudioContext` suspended until the first user gesture, and kira
//! offers no way to resume it. [`Audio`] therefore opens the backend lazily there: sounds can be
//! loaded right away, but playback only starts once [`Audio::start`] has run (the toolkit's core
//! calls it on the first key press, click or touch).

#![forbid(unsafe_code)]

use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Frame, Tween};

#[cfg(feature = "synth")]
mod synth;
#[cfg(feature = "synth")]
pub use synth::Synth;
#[cfg(feature = "synth")]
pub use synthie;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SoundId(pub u32);

pub struct Audio {
    manager: Option<AudioManager<DefaultBackend>>,
    sounds: HashMap<SoundId, StaticSoundData>,
    next_id: u32,
    /// Main-track gain, remembered so a volume set before [`Audio::start`] still applies.
    master_volume: Decibels,
}

impl Audio {
    /// Create the audio subsystem. On native targets this opens the output device immediately;
    /// on the web it defers that to [`Audio::start`].
    pub fn new() -> Result<Self> {
        let mut audio = Self {
            manager: None,
            sounds: HashMap::new(),
            next_id: 1,
            master_volume: Decibels::IDENTITY,
        };
        if !cfg!(target_arch = "wasm32") {
            audio.start()?;
        }
        Ok(audio)
    }

    /// Open the output device if it isn't open yet. Idempotent. On the web this must run
    /// inside a user-gesture handler, otherwise the browser keeps the `AudioContext` muted.
    pub fn start(&mut self) -> Result<()> {
        if self.manager.is_some() {
            return Ok(());
        }
        let mut manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .map_err(|e| anyhow!("kira AudioManager init: {e}"))?;
        manager
            .main_track()
            .set_volume(self.master_volume, Tween::default());
        self.manager = Some(manager);
        Ok(())
    }

    /// Whether the output device is open, i.e. [`Audio::play`] can produce sound.
    pub fn is_started(&self) -> bool {
        self.manager.is_some()
    }

    /// Load a sound file from disk. Not available on the web; use [`Audio::load_sound_bytes`].
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_sound(&mut self, path: impl AsRef<Path>) -> Result<SoundId> {
        let data = StaticSoundData::from_file(path.as_ref())
            .map_err(|e| anyhow!("kira load {}: {e}", path.as_ref().display()))?;
        Ok(self.insert(data))
    }

    /// Decode an encoded sound file (wav, ogg, flac, mp3) held in memory.
    pub fn load_sound_bytes(&mut self, bytes: impl Into<Vec<u8>>) -> Result<SoundId> {
        let data = StaticSoundData::from_cursor(std::io::Cursor::new(bytes.into()))
            .map_err(|e| anyhow!("kira decode: {e}"))?;
        Ok(self.insert(data))
    }

    fn insert(&mut self, data: StaticSoundData) -> SoundId {
        let id = SoundId(self.next_id);
        self.next_id += 1;
        self.sounds.insert(id, data);
        id
    }

    fn manager(&mut self) -> Result<&mut AudioManager<DefaultBackend>> {
        self.manager
            .as_mut()
            .ok_or_else(|| anyhow!("audio not started yet (waiting for a user gesture)"))
    }

    pub fn play(&mut self, id: SoundId) -> Result<StaticSoundHandle> {
        let data = self
            .sounds
            .get(&id)
            .ok_or_else(|| anyhow!("unknown SoundId {id:?}"))?
            .clone();
        self.manager()?
            .play(data)
            .map_err(|e| anyhow!("kira play: {e}"))
    }

    /// Play mono PCM samples (`-1.0..=1.0`) directly, e.g. a buffer rendered by
    /// [`Synth`]. The samples are duplicated to both channels.
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
        self.manager()?
            .play(data)
            .map_err(|e| anyhow!("kira play: {e}"))
    }

    /// Master volume on the default main track, as a linear amplitude
    /// (`1.0` is unity gain, `0.0` is silence). Converted to decibels for kira.
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = if volume <= 0.0 {
            Decibels::SILENCE
        } else {
            Decibels::from(20.0 * volume.log10())
        };
        if let Some(manager) = self.manager.as_mut() {
            manager
                .main_track()
                .set_volume(self.master_volume, Tween::default());
        }
    }
}
