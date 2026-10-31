//! Audio generated while it plays: [`AudioStream`] sources and their [`StreamHandle`].
//!
//! A stream is pulled from the audio callback itself, so it needs no decoder thread and works
//! on the web as well as natively. Keep [`AudioStream::fill`] cheap and non-blocking: it runs
//! on the audio thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use kira::sound::{Sound, SoundData};
use kira::{Frame, info::Info};

/// A source of mono samples, produced on demand at its own sample rate.
///
/// Implemented for any `FnMut(&mut [f32]) + Send + 'static`, so a closure that owns a
/// synthesizer or an emulated sound chip can be played directly.
pub trait AudioStream: Send + 'static {
    /// Overwrite `out` with the next samples in `-1.0..=1.0`.
    fn fill(&mut self, out: &mut [f32]);
}

impl<F: FnMut(&mut [f32]) + Send + 'static> AudioStream for F {
    fn fill(&mut self, out: &mut [f32]) {
        self(out);
    }
}

/// Samples requested from a stream at once.
const CHUNK: usize = 512;

/// State shared between a [`StreamHandle`] and the playing sound.
#[derive(Debug)]
struct Shared {
    /// Linear amplitude as `f32` bits.
    volume: AtomicU32,
    paused: AtomicBool,
    stopped: AtomicBool,
    /// Still queued until audio starts (see [`crate::PlayOptions::deferred`]).
    pending: AtomicBool,
}

/// Controls one playing [`AudioStream`], as returned by [`crate::Audio::play_stream`].
///
/// Dropping the handle does not stop the stream; it plays until [`StreamHandle::stop`].
/// Clones refer to the same stream. Changes apply from the next audio block.
#[derive(Clone, Debug)]
pub struct StreamHandle {
    shared: Arc<Shared>,
}

impl StreamHandle {
    fn new(volume: f32, pending: bool) -> Self {
        Self {
            shared: Arc::new(Shared {
                // Clamped like set_volume: a negative start volume is silence, not an
                // inverted signal.
                volume: AtomicU32::new(volume.max(0.0).to_bits()),
                paused: AtomicBool::new(false),
                stopped: AtomicBool::new(false),
                pending: AtomicBool::new(pending),
            }),
        }
    }

    /// Set the volume as a linear amplitude (`1.0` is the stream's own level, `0.0` silence).
    pub fn set_volume(&self, volume: f32) {
        self.shared
            .volume
            .store(volume.max(0.0).to_bits(), Ordering::Relaxed);
    }

    /// The current linear volume.
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.shared.volume.load(Ordering::Relaxed))
    }

    /// Pause: the stream outputs silence and is not asked for samples until resumed.
    pub fn pause(&self) {
        self.shared.paused.store(true, Ordering::Relaxed);
    }

    /// Resume a paused stream where it left off.
    pub fn resume(&self) {
        self.shared.paused.store(false, Ordering::Relaxed);
    }

    /// Stop the stream for good; it is dropped from the mixer.
    pub fn stop(&self) {
        self.shared.stopped.store(true, Ordering::Relaxed);
    }

    /// Whether the stream is audible: started, not paused and not stopped.
    pub fn is_playing(&self) -> bool {
        !self.is_pending() && !self.is_paused() && !self.is_finished()
    }

    /// Whether the stream is paused.
    pub fn is_paused(&self) -> bool {
        self.shared.paused.load(Ordering::Relaxed)
    }

    /// Whether the stream is still waiting for audio to start.
    pub fn is_pending(&self) -> bool {
        self.shared.pending.load(Ordering::Relaxed)
    }

    /// Whether the stream was stopped.
    pub fn is_finished(&self) -> bool {
        self.shared.stopped.load(Ordering::Relaxed)
    }

    pub(crate) fn mark_started(&self) {
        self.shared.pending.store(false, Ordering::Relaxed);
    }
}

/// A stream ready to hand to kira.
pub(crate) struct StreamData {
    stream: Box<dyn AudioStream>,
    sample_rate: u32,
    handle: StreamHandle,
}

impl StreamData {
    pub(crate) fn new(
        stream: Box<dyn AudioStream>,
        sample_rate: u32,
        volume: f32,
        pending: bool,
    ) -> (Self, StreamHandle) {
        let handle = StreamHandle::new(volume, pending);
        let data = Self {
            stream,
            sample_rate: sample_rate.max(1),
            handle: handle.clone(),
        };
        (data, handle)
    }

    pub(crate) fn handle(&self) -> &StreamHandle {
        &self.handle
    }
}

impl SoundData for StreamData {
    type Error = std::convert::Infallible;
    type Handle = ();

    fn into_sound(self) -> Result<(Box<dyn Sound>, ()), Self::Error> {
        let sound = StreamSound {
            stream: self.stream,
            sample_rate: f64::from(self.sample_rate),
            shared: self.handle.shared,
            chunk: vec![0.0; CHUNK],
            next_in_chunk: CHUNK,
            previous: 0.0,
            current: 0.0,
            phase: 0.0,
            primed: false,
        };
        Ok((Box::new(sound), ()))
    }
}

/// The kira side: pulls chunks from the stream and resamples them linearly to the output
/// rate.
struct StreamSound {
    stream: Box<dyn AudioStream>,
    sample_rate: f64,
    shared: Arc<Shared>,
    chunk: Vec<f32>,
    next_in_chunk: usize,
    /// The two stream samples the output position lies between, and how far along it is.
    previous: f32,
    current: f32,
    phase: f64,
    /// Whether `previous` and `current` hold the first two samples yet.
    primed: bool,
}

impl StreamSound {
    fn next_sample(&mut self) -> f32 {
        if self.next_in_chunk == self.chunk.len() {
            self.stream.fill(&mut self.chunk);
            self.next_in_chunk = 0;
        }
        let s = self.chunk[self.next_in_chunk];
        self.next_in_chunk += 1;
        s
    }

    /// Produces `out` at an output frame duration of `dt` seconds.
    fn render(&mut self, out: &mut [Frame], dt: f64) {
        if self.shared.paused.load(Ordering::Relaxed) || self.shared.stopped.load(Ordering::Relaxed)
        {
            out.fill(Frame::ZERO);
            return;
        }
        if !self.primed {
            self.previous = self.next_sample();
            self.current = self.next_sample();
            self.primed = true;
        }
        let volume = f32::from_bits(self.shared.volume.load(Ordering::Relaxed));
        let step = self.sample_rate * dt;
        for frame in out {
            let s = self.previous + (self.current - self.previous) * self.phase as f32;
            *frame = Frame::from_mono(s * volume);
            self.phase += step;
            while self.phase >= 1.0 {
                self.phase -= 1.0;
                self.previous = self.current;
                self.current = self.next_sample();
            }
        }
    }
}

impl Sound for StreamSound {
    fn process(&mut self, out: &mut [Frame], dt: f64, _info: &Info<'_>) {
        self.render(out, dt);
    }

    fn finished(&self) -> bool {
        self.shared.stopped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sound(stream: impl AudioStream, rate: u32) -> (StreamSound, StreamHandle) {
        sound_at(stream, rate, 1.0)
    }

    fn sound_at(stream: impl AudioStream, rate: u32, volume: f32) -> (StreamSound, StreamHandle) {
        let (data, handle) = StreamData::new(Box::new(stream), rate, volume, false);
        let sound = StreamSound {
            stream: data.stream,
            sample_rate: f64::from(data.sample_rate),
            shared: handle.shared.clone(),
            chunk: vec![0.0; CHUNK],
            next_in_chunk: CHUNK,
            previous: 0.0,
            current: 0.0,
            phase: 0.0,
            primed: false,
        };
        (sound, handle)
    }

    fn ramp() -> impl AudioStream {
        let mut n = 0u32;
        move |out: &mut [f32]| {
            for s in out {
                *s = n as f32;
                n += 1;
            }
        }
    }

    #[test]
    fn same_rate_passes_samples_through() {
        let (mut s, _) = sound(ramp(), 100);
        let mut out = [Frame::ZERO; 4];
        s.render(&mut out, 1.0 / 100.0);
        let left: Vec<f32> = out.iter().map(|f| f.left).collect();
        assert_eq!(left, [0.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn half_rate_interpolates() {
        let (mut s, _) = sound(ramp(), 50);
        let mut out = [Frame::ZERO; 6];
        s.render(&mut out, 1.0 / 100.0);
        let left: Vec<f32> = out.iter().map(|f| f.left).collect();
        assert_eq!(left, [0.0, 0.5, 1.0, 1.5, 2.0, 2.5]);
    }

    #[test]
    fn paused_and_stopped_streams_are_silent() {
        let (mut s, handle) = sound(ramp(), 100);
        let mut out = [Frame::ZERO; 4];
        handle.pause();
        s.render(&mut out, 0.01);
        assert!(out.iter().all(|f| *f == Frame::ZERO));
        handle.resume();
        handle.stop();
        assert!(s.finished() && handle.is_finished() && !handle.is_playing());
    }

    #[test]
    fn the_mixer_pulls_samples_from_a_playing_stream() {
        use std::sync::atomic::AtomicUsize;

        use kira::backend::mock::{MockBackend, MockBackendSettings};
        use kira::{AudioManager, AudioManagerSettings};

        let pulled = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&pulled);
        let stream = move |out: &mut [f32]| {
            counter.fetch_add(out.len(), Ordering::Relaxed);
            out.fill(0.5);
        };
        let mut manager = AudioManager::<MockBackend>::new(AudioManagerSettings {
            backend_settings: MockBackendSettings { sample_rate: 100 },
            ..AudioManagerSettings::default()
        })
        .unwrap();
        let (data, handle) = StreamData::new(Box::new(stream), 100, 1.0, false);
        manager.play(data).unwrap();
        handle.mark_started();
        manager.backend_mut().on_start_processing();
        manager.backend_mut().process();
        assert!(
            pulled.load(Ordering::Relaxed) > 0,
            "stream was never asked for samples"
        );
        assert!(handle.is_playing());

        handle.stop();
        manager.backend_mut().on_start_processing();
        manager.backend_mut().process();
        assert!(handle.is_finished());
    }

    #[test]
    fn negative_start_volume_is_silence() {
        let (mut s, handle) = sound_at(|out: &mut [f32]| out.fill(1.0), 100, -0.5);
        assert_eq!(handle.volume(), 0.0);
        let mut out = [Frame::ZERO; 2];
        s.render(&mut out, 0.01);
        assert!(out.iter().all(|f| f.left == 0.0 && f.right == 0.0));
    }

    #[test]
    fn volume_scales_output() {
        let (mut s, handle) = sound(|out: &mut [f32]| out.fill(1.0), 100);
        handle.set_volume(0.25);
        let mut out = [Frame::ZERO; 3];
        s.render(&mut out, 0.01);
        assert_eq!(out[2].left, 0.25);
    }
}
