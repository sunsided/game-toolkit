//! Live synthie music through kira: [`crate::Audio::play_synth`] and its [`SynthHandle`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use kira::Tween;
use kira::sound::{PlaybackState, Sound, SoundData};
use synthie::kira::SynthSoundData;
use synthie::prelude::Sequencer;

use crate::amplitude_to_db;
use crate::handle::tween;
use crate::synth::{AudioEvent, ChannelNo, DrumHit, MidiNote, QueueFull};

/// Controls one live synth, as returned by [`crate::Audio::play_synth`]: volume, pause,
/// resume, stop, and [`AudioEvent`]s sent to it while it plays (a drum hit on a game event, a
/// patch change for the boss fight).
///
/// Clones refer to the same synth. Unlike a [`crate::SoundHandle`], **dropping the last clone
/// stops the synth**: music never ends on its own, so it lives as long as the game keeps a
/// handle to it.
///
/// A synth started with [`crate::PlayOptions::deferred`] before audio is available (on the
/// web, before the first user gesture) is *pending*; everything done to the handle meanwhile,
/// events included, applies once it starts.
#[derive(Clone, Debug)]
pub struct SynthHandle {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    synth: Mutex<synthie::kira::SynthHandle>,
    pending: AtomicBool,
    paused: AtomicBool,
    /// Stopped through the handle (also before it started).
    stopped: AtomicBool,
}

impl SynthHandle {
    fn lock(&self) -> MutexGuard<'_, synthie::kira::SynthHandle> {
        // Every call leaves the synthie handle consistent, so a poisoned lock is harmless.
        self.inner
            .synth
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Send `event` to the synth; it applies at the start of the next audio block (a few
    /// milliseconds). Anything rhythmic belongs in the song, which is sample-accurate.
    ///
    /// # Errors
    ///
    /// Gives the event back when the queue is full (1024 events), e.g. because the synth is
    /// paused or still pending.
    pub fn send(&self, event: AudioEvent) -> Result<(), QueueFull> {
        self.lock().send(event)
    }

    /// Fire a drum hit. See [`SynthHandle::send`].
    pub fn drum(&self, hit: DrumHit) -> Result<(), QueueFull> {
        self.send(AudioEvent::Drum(hit))
    }

    /// Note on for MIDI `note` on `channel`. See [`SynthHandle::send`].
    pub fn note_on(&self, channel: u8, note: u8) -> Result<(), QueueFull> {
        self.send(AudioEvent::NoteOnChannel(
            ChannelNo(channel),
            MidiNote(note),
        ))
    }

    /// Note off for MIDI `note` on `channel`. See [`SynthHandle::send`].
    pub fn note_off(&self, channel: u8, note: u8) -> Result<(), QueueFull> {
        self.send(AudioEvent::NoteOffChannel(
            ChannelNo(channel),
            MidiNote(note),
        ))
    }

    /// Set the volume as a linear amplitude (`1.0` is the synth's own level, `0.0` silence).
    pub fn set_volume(&self, volume: f32) {
        self.fade_volume(volume, Duration::ZERO);
    }

    /// Set the volume in decibels (`0.0` is the synth's own level, `-6.0` roughly half).
    pub fn set_volume_db(&self, db: f32) {
        self.fade_volume_db(db, Duration::ZERO);
    }

    /// Fade the volume to a linear amplitude over `duration`, e.g. to duck the music.
    pub fn fade_volume(&self, volume: f32, duration: Duration) {
        self.fade_volume_db(amplitude_to_db(volume).0, duration);
    }

    /// Fade the volume to `db` decibels over `duration`.
    pub fn fade_volume_db(&self, db: f32, duration: Duration) {
        self.lock().set_volume(kira::Decibels(db), tween(duration));
    }

    /// Pause: the song and its voices stop advancing until [`SynthHandle::resume`].
    pub fn pause(&self) {
        self.inner.paused.store(true, Ordering::Relaxed);
        self.lock().pause(Tween::default());
    }

    /// Resume a paused synth where it left off.
    pub fn resume(&self) {
        self.inner.paused.store(false, Ordering::Relaxed);
        self.lock().resume(Tween::default());
    }

    /// Stop the synth for good. A stopped synth can't be resumed; play the song again.
    pub fn stop(&self) {
        self.inner.stopped.store(true, Ordering::Relaxed);
        self.lock().stop(Tween::default());
    }

    /// Whether the synth is audible: started, not paused and not stopped.
    pub fn is_playing(&self) -> bool {
        !self.is_pending() && !self.is_paused() && !self.is_finished()
    }

    /// Whether the synth is paused (or will start paused, if pending).
    pub fn is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::Relaxed) && !self.is_finished()
    }

    /// Whether the synth is still waiting for audio to start (see
    /// [`crate::PlayOptions::deferred`]).
    pub fn is_pending(&self) -> bool {
        self.inner.pending.load(Ordering::Relaxed) && !self.is_finished()
    }

    /// Whether the synth was stopped.
    pub fn is_finished(&self) -> bool {
        self.inner.stopped.load(Ordering::Relaxed) || self.lock().state() == PlaybackState::Stopped
    }

    /// Whether, as of the last audio block, no voice or drum is sounding. A song may still
    /// start notes on its next row; effect tails may still ring.
    pub fn is_idle(&self) -> bool {
        self.lock().is_idle()
    }

    pub(crate) fn mark_started(&self) {
        self.inner.pending.store(false, Ordering::Relaxed);
    }
}

/// A synth turned into its kira sound ahead of playing, so the handle exists (and queues
/// events) before audio starts.
pub(crate) struct PreparedSynth {
    sound: Box<dyn Sound>,
    handle: SynthHandle,
}

impl PreparedSynth {
    pub(crate) fn new<S: Sequencer + Send + 'static, const N: usize>(
        data: SynthSoundData<S, N>,
        pending: bool,
    ) -> Self {
        let Ok((sound, synth)) = data.into_sound();
        let handle = SynthHandle {
            inner: Arc::new(Inner {
                synth: Mutex::new(synth),
                pending: AtomicBool::new(pending),
                paused: AtomicBool::new(false),
                stopped: AtomicBool::new(false),
            }),
        };
        Self { sound, handle }
    }

    pub(crate) fn handle(&self) -> &SynthHandle {
        &self.handle
    }
}

impl SoundData for PreparedSynth {
    type Error = std::convert::Infallible;
    type Handle = ();

    fn into_sound(self) -> Result<(Box<dyn Sound>, ()), Self::Error> {
        Ok((self.sound, ()))
    }
}

#[cfg(test)]
mod tests {
    use kira::backend::mock::{MockBackend, MockBackendSettings};
    use kira::{AudioManager, AudioManagerSettings};

    use super::*;

    fn manager() -> AudioManager<MockBackend> {
        AudioManager::<MockBackend>::new(AudioManagerSettings {
            backend_settings: MockBackendSettings { sample_rate: 1_000 },
            ..AudioManagerSettings::default()
        })
        .unwrap()
    }

    fn process(manager: &mut AudioManager<MockBackend>) {
        manager.backend_mut().on_start_processing();
        manager.backend_mut().process();
    }

    #[test]
    fn a_pending_handle_queues_events_and_state() {
        let prepared = PreparedSynth::new(SynthSoundData::new(), true);
        let handle = prepared.handle().clone();
        assert!(handle.is_pending() && !handle.is_playing());
        handle.drum(DrumHit::Kick).unwrap();
        handle.pause();
        assert!(handle.is_paused() && !handle.is_finished());

        let mut manager = manager();
        manager.play(prepared).unwrap();
        handle.mark_started();
        process(&mut manager);
        assert!(!handle.is_pending() && handle.is_paused());
        handle.resume();
        process(&mut manager);
        assert!(handle.is_playing());
    }

    #[test]
    fn stop_finishes_and_clones_share_state() {
        let prepared = PreparedSynth::new(SynthSoundData::new(), false);
        let handle = prepared.handle().clone();
        let clone = handle.clone();
        let mut manager = manager();
        manager.play(prepared).unwrap();
        process(&mut manager);
        assert!(clone.is_playing());
        handle.stop();
        assert!(clone.is_finished() && !clone.is_playing() && !clone.is_paused());
    }
}
