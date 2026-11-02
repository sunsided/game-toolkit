//! The web audio backend: kira's cpal backend, plus a way to suspend the `AudioContext`.
//!
//! kira's own `CpalBackend` keeps its cpal stream private, so nothing outside it can pause the
//! output. This is the same backend (cpal's web host renders kira's mixer into an
//! `AudioContext`), except that the stream is shared with a [`DetachHandle`]: pausing the
//! stream suspends the `AudioContext`, which stops its clock and the mixer with it.

use std::cell::RefCell;
use std::rc::Rc;

use kira::backend::cpal::Error;
use kira::backend::cpal::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use kira::backend::cpal::cpal::{self, Stream};
use kira::backend::{Backend, Renderer};

/// Suspends [`Audio`](crate::Audio)'s output on the web while the game's canvas is detached
/// from the page, from outside a frame (a browser callback): when the host page removes the
/// canvas, the browser stops delivering frames, so the game itself can't react.
///
/// Get one from [`Audio::detach_handle`](crate::Audio::detach_handle). The toolkit's core
/// already drives it from a `MutationObserver` on the canvas; games don't need to.
///
/// This is separate from [`Audio::suspend`](crate::Audio::suspend): reattaching resumes the
/// output, but sounds the game suspended stay suspended.
#[derive(Clone, Default)]
pub struct DetachHandle(Rc<RefCell<Output>>);

#[derive(Default)]
struct Output {
    /// The running cpal stream, once [`Audio::start`](crate::Audio::start) has opened it.
    stream: Option<Stream>,
    detached: bool,
}

impl DetachHandle {
    /// Mark the canvas detached (`true`: suspend the `AudioContext`) or attached again
    /// (`false`: resume it). Before audio has started, only remembers the state.
    pub fn set_detached(&self, detached: bool) {
        let mut out = self.0.borrow_mut();
        if out.detached == detached {
            return;
        }
        out.detached = detached;
        if let Some(stream) = &out.stream {
            apply(stream, detached);
        }
    }

    /// Whether the output is suspended because the canvas is detached.
    pub fn is_detached(&self) -> bool {
        self.0.borrow().detached
    }
}

/// Pause (suspend the `AudioContext`) or play (resume it) `stream`.
fn apply(stream: &Stream, detached: bool) {
    let result = if detached {
        stream.pause()
    } else {
        stream.play()
    };
    if let Err(e) = result {
        log::warn!("audio {}: {e}", if detached { "suspend" } else { "resume" });
    }
}

/// kira [`Backend`] for the web; see the module docs.
pub(crate) struct WebBackend {
    output: DetachHandle,
    pending: Option<(cpal::Device, cpal::StreamConfig)>,
}

impl Backend for WebBackend {
    type Settings = DetachHandle;
    type Error = Error;

    fn setup(output: DetachHandle, _internal_buffer_size: usize) -> Result<(Self, u32), Error> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(Error::NoDefaultOutputDevice)?;
        let config = device.default_output_config()?.config();
        let sample_rate = config.sample_rate;
        let backend = Self {
            output,
            pending: Some((device, config)),
        };
        Ok((backend, sample_rate))
    }

    fn start(&mut self, mut renderer: Renderer) -> Result<(), Error> {
        let (device, config) = self
            .pending
            .take()
            .expect("kira starts a backend only once");
        let channels = config.channels;
        let stream = device.build_output_stream(
            config,
            move |data: &mut [f32], _| {
                renderer.on_start_processing();
                renderer.process(data, channels);
            },
            |e| log::warn!("audio stream error: {e}"),
            None,
        )?;
        let mut out = self.output.0.borrow_mut();
        if out.detached {
            apply(&stream, true);
        } else {
            stream.play()?;
        }
        out.stream = Some(stream);
        Ok(())
    }
}

impl Drop for WebBackend {
    /// The stream belongs to the backend: close it with the `AudioManager`, even if a
    /// [`DetachHandle`] outlives it.
    fn drop(&mut self) {
        self.output.0.borrow_mut().stream = None;
    }
}
