//! [`SoundHandle`]: a toolkit-owned handle to one playing (or queued) sound instance.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use kira::sound::PlaybackState;
use kira::sound::static_sound::StaticSoundHandle;
use kira::{Decibels, Tween};

use crate::amplitude_to_db;

/// Controls one playing sound instance, as returned by [`crate::Audio::play`] and friends.
///
/// Dropping the handle does not stop the sound; it keeps playing until it ends (or forever,
/// if it loops). Clones refer to the same instance.
///
/// A handle can exist before the sound actually plays: a sound started with
/// [`crate::PlayOptions::deferred`] before audio is available (on the web, before the first
/// user gesture) is *pending*. Volume, pause and stop calls on a pending handle are remembered
/// and applied when the sound starts.
#[derive(Clone, Debug)]
pub struct SoundHandle {
    slot: Arc<Mutex<Slot>>,
}

#[derive(Debug)]
pub(crate) enum Slot {
    /// Queued until audio starts, with the state to apply once it does: the last volume
    /// change (target and fade duration) and whether it was paused.
    Pending {
        volume: Option<(Decibels, Duration)>,
        paused: bool,
    },
    Playing(StaticSoundHandle),
    /// Stopped while still pending; it never plays.
    Cancelled,
}

impl SoundHandle {
    pub(crate) fn playing(handle: StaticSoundHandle) -> Self {
        Self::from_slot(Slot::Playing(handle))
    }

    pub(crate) fn pending() -> Self {
        Self::from_slot(Slot::Pending {
            volume: None,
            paused: false,
        })
    }

    fn from_slot(slot: Slot) -> Self {
        Self {
            slot: Arc::new(Mutex::new(slot)),
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Slot> {
        // A panic while holding the lock can't leave the slot half-updated, so ignore poison.
        self.slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Set the volume as a linear amplitude (`1.0` is the sound's own level, `0.0` silence).
    pub fn set_volume(&self, volume: f32) {
        self.set_volume_db(amplitude_to_db(volume).0);
    }

    /// Set the volume in decibels (`0.0` is the sound's own level, `-6.0` roughly half).
    pub fn set_volume_db(&self, db: f32) {
        self.fade_volume_db(db, Duration::ZERO);
    }

    /// Fade the volume to a linear amplitude over `duration`.
    pub fn fade_volume(&self, volume: f32, duration: Duration) {
        self.fade_volume_db(amplitude_to_db(volume).0, duration);
    }

    /// Fade the volume to `db` decibels over `duration`.
    pub fn fade_volume_db(&self, db: f32, duration: Duration) {
        match &mut *self.lock() {
            // Replayed when the sound starts, so a fade-in requested early still fades.
            Slot::Pending { volume, .. } => *volume = Some((Decibels(db), duration)),
            Slot::Playing(h) => h.set_volume(Decibels(db), tween(duration)),
            Slot::Cancelled => {}
        }
    }

    /// Pause playback. [`SoundHandle::resume`] continues where it left off.
    pub fn pause(&self) {
        match &mut *self.lock() {
            Slot::Pending { paused, .. } => *paused = true,
            Slot::Playing(h) => h.pause(Tween::default()),
            Slot::Cancelled => {}
        }
    }

    /// Resume a paused sound.
    pub fn resume(&self) {
        match &mut *self.lock() {
            Slot::Pending { paused, .. } => *paused = false,
            Slot::Playing(h) => h.resume(Tween::default()),
            Slot::Cancelled => {}
        }
    }

    /// Stop the sound for good. A stopped sound can't be resumed; play it again instead.
    pub fn stop(&self) {
        let mut slot = self.lock();
        match &mut *slot {
            Slot::Pending { .. } => *slot = Slot::Cancelled,
            Slot::Playing(h) => h.stop(Tween::default()),
            Slot::Cancelled => {}
        }
    }

    /// Whether the sound is currently audible: playing, or fading in or out. `false` while
    /// paused, pending or finished.
    pub fn is_playing(&self) -> bool {
        match &*self.lock() {
            Slot::Playing(h) => h.state().is_advancing(),
            Slot::Pending { .. } | Slot::Cancelled => false,
        }
    }

    /// Whether the sound is paused (or will start paused, if pending).
    pub fn is_paused(&self) -> bool {
        match &*self.lock() {
            Slot::Playing(h) => h.state() == PlaybackState::Paused,
            Slot::Pending { paused, .. } => *paused,
            Slot::Cancelled => false,
        }
    }

    /// Whether the sound is still waiting for audio to start (see [`crate::PlayOptions::deferred`]).
    pub fn is_pending(&self) -> bool {
        matches!(&*self.lock(), Slot::Pending { .. })
    }

    /// Whether the sound has ended or was stopped. A looping sound only finishes when stopped.
    pub fn is_finished(&self) -> bool {
        match &*self.lock() {
            Slot::Playing(h) => h.state() == PlaybackState::Stopped,
            Slot::Pending { .. } => false,
            Slot::Cancelled => true,
        }
    }

    /// Run `f` on the underlying [`kira`] handle, for anything this type doesn't cover
    /// (playback rate, panning, seeking). `None` while the sound is pending or was cancelled.
    pub fn with_kira<R>(&self, f: impl FnOnce(&mut StaticSoundHandle) -> R) -> Option<R> {
        match &mut *self.lock() {
            Slot::Playing(h) => Some(f(h)),
            Slot::Pending { .. } | Slot::Cancelled => None,
        }
    }
}

pub(crate) fn tween(duration: Duration) -> Tween {
    if duration.is_zero() {
        Tween::default()
    } else {
        Tween {
            duration,
            ..Tween::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_handle_remembers_state() {
        let h = SoundHandle::pending();
        assert!(h.is_pending() && !h.is_playing() && !h.is_finished());
        h.set_volume_db(-12.0);
        h.pause();
        assert!(h.is_paused());
        match &*h.lock() {
            Slot::Pending { volume, paused } => {
                assert_eq!(*volume, Some((Decibels(-12.0), Duration::ZERO)));
                assert!(*paused);
            }
            other => panic!("expected pending, got {other:?}"),
        }
        h.resume();
        assert!(!h.is_paused());
        h.fade_volume_db(-3.0, Duration::from_secs(2));
        assert!(matches!(
            &*h.lock(),
            Slot::Pending { volume: Some((Decibels(db), d)), .. }
                if *db == -3.0 && *d == Duration::from_secs(2)
        ));
    }

    #[test]
    fn stopping_a_pending_handle_cancels_it() {
        let h = SoundHandle::pending();
        let clone = h.clone();
        h.stop();
        assert!(clone.is_finished() && !clone.is_pending());
        assert!(clone.with_kira(|_| ()).is_none());
    }
}
