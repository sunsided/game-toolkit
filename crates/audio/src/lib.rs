//! Audio subsystem built on [`kira`].
//!
//! Load static sound buffers (from disk or bytes) and play them on the main track. Every play
//! call returns a [`SoundHandle`] that controls that instance: volume (instant or faded), pause,
//! resume, stop, and whether it's still playing. [`PlayOptions`] covers looping, a start volume
//! and deferred start in one call:
//!
//! ```no_run
//! # use game_toolkit_audio::{Audio, PlayOptions};
//! # fn demo(audio: &mut Audio, music: game_toolkit_audio::SoundId) -> anyhow::Result<()> {
//! // Background music: loop at 30% volume, starting as soon as audio is available.
//! let handle = audio.play_with(music, PlayOptions::looped().volume(0.3).deferred())?;
//! handle.pause();
//! handle.resume();
//! handle.fade_volume(0.0, std::time::Duration::from_secs(2));
//! # Ok(()) }
//! ```
//!
//! Mixer tracks, spatial audio and streaming are not wrapped; [`kira`] is re-exported for
//! anything beyond this API, and [`SoundHandle::with_kira`] reaches the underlying handle.
//!
//! On the web, browsers keep an `AudioContext` suspended until the first user gesture, and kira
//! offers no way to resume it. [`Audio`] therefore opens the backend lazily there: sounds can be
//! loaded right away, but playback only starts once [`Audio::start`] has run (the toolkit's core
//! calls it on the first key press, click or touch). Until then plays fail with
//! [`AudioError::NotStarted`], unless they were [`PlayOptions::deferred`], which queues them.

#![forbid(unsafe_code)]

use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use kira::sound::static_sound::{StaticSoundData, StaticSoundSettings};
use kira::{AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Frame, Tween};

mod handle;
use handle::Slot;
pub use handle::SoundHandle;
pub use kira;

#[cfg(feature = "synth")]
mod synth;
#[cfg(feature = "synth")]
pub use synth::Synth;
#[cfg(feature = "synth")]
pub use synthie;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SoundId(pub u32);

/// Why a sound didn't play.
#[derive(Debug)]
#[non_exhaustive]
pub enum AudioError {
    /// Audio hasn't started yet: on the web, the player hasn't pressed a key, clicked or
    /// touched the page. Not a real failure; try again later, check [`Audio::is_started`]
    /// first, or play with [`PlayOptions::deferred`] to queue the sound.
    NotStarted,
    /// The [`SoundId`] wasn't returned by this [`Audio`].
    UnknownSound(SoundId),
    /// kira refused to play the sound (e.g. its sound limit was reached).
    Backend(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStarted => f.write_str("audio not started yet (waiting for a user gesture)"),
            Self::UnknownSound(id) => write!(f, "unknown SoundId {id:?}"),
            Self::Backend(e) => write!(f, "kira play: {e}"),
        }
    }
}

impl std::error::Error for AudioError {}

/// How to play a sound: see [`Audio::play_with`]. The default plays once at the sound's own
/// volume, and fails with [`AudioError::NotStarted`] before audio is available.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PlayOptions {
    /// Loop the whole sound until stopped.
    pub looped: bool,
    /// Linear amplitude (`1.0` is the sound's own level, `0.0` silence).
    pub volume: f32,
    /// If audio hasn't started yet (web, before the first user gesture), queue the sound and
    /// start it once audio starts, instead of failing with [`AudioError::NotStarted`]. Meant
    /// for music and ambience; a one-shot effect would play late, out of context.
    pub deferred: bool,
}

impl Default for PlayOptions {
    fn default() -> Self {
        Self {
            looped: false,
            volume: 1.0,
            deferred: false,
        }
    }
}

impl PlayOptions {
    /// Options for a sound that loops until stopped.
    pub fn looped() -> Self {
        Self {
            looped: true,
            ..Self::default()
        }
    }

    /// Set [`PlayOptions::volume`].
    pub fn volume(self, volume: f32) -> Self {
        Self { volume, ..self }
    }

    /// Set [`PlayOptions::deferred`].
    pub fn deferred(self) -> Self {
        Self {
            deferred: true,
            ..self
        }
    }
}

pub struct Audio {
    manager: Option<AudioManager<DefaultBackend>>,
    sounds: HashMap<SoundId, StaticSoundData>,
    next_id: u32,
    /// Main-track gain, remembered so a volume set before [`Audio::start`] still applies.
    master_volume: Decibels,
    /// [`PlayOptions::deferred`] sounds waiting for [`Audio::start`].
    queued: Vec<(StaticSoundData, SoundHandle)>,
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
            queued: Vec::new(),
        };
        if !cfg!(target_arch = "wasm32") {
            audio.start()?;
        }
        Ok(audio)
    }

    /// Open the output device if it isn't open yet, and start any [`PlayOptions::deferred`]
    /// sounds queued until then. Idempotent. On the web this must run inside a user-gesture
    /// handler, otherwise the browser keeps the `AudioContext` muted.
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
        for (data, handle) in std::mem::take(&mut self.queued) {
            self.start_queued(data, &handle);
        }
        Ok(())
    }

    fn start_queued(&mut self, data: StaticSoundData, handle: &SoundHandle) {
        let mut slot = handle.lock();
        let Slot::Pending { volume, paused } = &*slot else {
            return; // stopped while queued
        };
        let (volume, paused) = (*volume, *paused);
        // An instant volume change replaces the start volume; a fade starts from it once
        // the sound plays, over the duration that was asked for.
        let (data, fade) = match volume {
            Some((db, duration)) if duration.is_zero() => (data.volume(db), None),
            Some((db, duration)) => (data, Some((db, duration))),
            None => (data, None),
        };
        match self.play_now(data) {
            Ok(mut kira) => {
                if let Some((db, duration)) = fade {
                    kira.set_volume(db, handle::tween(duration));
                }
                if paused {
                    kira.pause(Tween::default());
                }
                *slot = Slot::Playing(kira);
            }
            Err(e) => {
                log::warn!("queued sound failed to start: {e}");
                *slot = Slot::Cancelled;
            }
        }
    }

    /// Whether the output device is open, i.e. [`Audio::play`] can produce sound. Always
    /// `true` natively; on the web, `false` until the first user gesture.
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

    /// Play a loaded sound once, at its own volume. The returned [`SoundHandle`] controls this
    /// instance (volume, pause, stop, `is_playing`); dropping it lets the sound play out.
    ///
    /// Fails with [`AudioError::NotStarted`] on the web until the player's first key press,
    /// click or touch (check [`Audio::is_started`]), so don't `?` it in `update`. For music
    /// that should start as soon as it can, use [`Audio::play_with`] and
    /// [`PlayOptions::deferred`].
    pub fn play(&mut self, id: SoundId) -> Result<SoundHandle, AudioError> {
        self.play_with(id, PlayOptions::default())
    }

    /// Play a loaded sound with [`PlayOptions`]: looping, a start volume, and whether to
    /// queue it until audio starts.
    pub fn play_with(
        &mut self,
        id: SoundId,
        options: PlayOptions,
    ) -> Result<SoundHandle, AudioError> {
        let data = self
            .sounds
            .get(&id)
            .ok_or(AudioError::UnknownSound(id))?
            .clone();
        self.play_data(data, options)
    }

    /// Play mono PCM samples (`-1.0..=1.0`) directly, e.g. a buffer rendered by
    /// [`Synth`]. The samples are duplicated to both channels. Fails like [`Audio::play`]
    /// before audio has started.
    pub fn play_samples(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
    ) -> Result<SoundHandle, AudioError> {
        self.play_samples_with(samples, sample_rate, PlayOptions::default())
    }

    /// [`Audio::play_samples`] with [`PlayOptions`].
    pub fn play_samples_with(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        options: PlayOptions,
    ) -> Result<SoundHandle, AudioError> {
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
        self.play_data(data, options)
    }

    fn play_data(
        &mut self,
        data: StaticSoundData,
        options: PlayOptions,
    ) -> Result<SoundHandle, AudioError> {
        let mut data = data.volume(amplitude_to_db(options.volume));
        if options.looped {
            data = data.loop_region(..);
        }
        if self.manager.is_none() && options.deferred {
            let handle = SoundHandle::pending();
            self.queued.push((data, handle.clone()));
            return Ok(handle);
        }
        self.play_now(data).map(SoundHandle::playing)
    }

    fn play_now(
        &mut self,
        data: StaticSoundData,
    ) -> Result<kira::sound::static_sound::StaticSoundHandle, AudioError> {
        self.manager
            .as_mut()
            .ok_or(AudioError::NotStarted)?
            .play(data)
            .map_err(|e| AudioError::Backend(e.to_string()))
    }

    /// Master volume on the default main track, as a linear amplitude
    /// (`1.0` is unity gain, `0.0` is silence). Converted to decibels for kira.
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = amplitude_to_db(volume);
        if let Some(manager) = self.manager.as_mut() {
            manager
                .main_track()
                .set_volume(self.master_volume, Tween::default());
        }
    }
}

/// Linear amplitude (`1.0` unity, `0.0` silence) to kira's decibels.
pub(crate) fn amplitude_to_db(volume: f32) -> Decibels {
    if volume <= 0.0 {
        Decibels::SILENCE
    } else {
        Decibels::from(20.0 * volume.log10())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amplitude_to_db_maps_unity_and_silence() {
        assert_eq!(amplitude_to_db(1.0), Decibels::IDENTITY);
        assert_eq!(amplitude_to_db(0.0), Decibels::SILENCE);
        assert!((amplitude_to_db(0.5).0 + 6.02).abs() < 0.01);
    }

    #[test]
    fn play_options_builders() {
        let o = PlayOptions::looped().volume(0.25).deferred();
        assert!(o.looped && o.deferred);
        assert_eq!(o.volume, 0.25);
        assert_eq!(PlayOptions::default().volume, 1.0);
    }
}
