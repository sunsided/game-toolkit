use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Runs simulation steps on the GPU in their own submissions, separate from the frame, and
/// keeps them from piling up while the GPU is also busy drawing.
///
/// Use it to offload a simulation to the GPU at its own rate (a fixed tick in `update`, or
/// as many steps as the GPU keeps up with) while the game renders as usual. Steps go into the
/// shared device queue in submission order, so drawing recorded later in the frame (a sprite
/// of a `StorageTexture`, a custom pass that reads a
/// [`PingPong`](crate::PingPong) buffer as vertices or storage) sees every step submitted
/// before it, with no CPU round trip.
///
/// A submission counts as in flight until the GPU has finished it. [`Self::try_submit`]
/// skips the step while `max_in_flight` are still running, so a simulation heavier than the
/// GPU can handle slows down instead of queueing up seconds of work and delaying the frames
/// behind it. Nothing blocks, on native or the web.
///
/// wgpu has one queue per device, so the GPU interleaves compute and drawing rather than
/// running them truly in parallel; what this buys is that neither waits on the CPU for the
/// other, and simulation load can't stall the frame indefinitely.
///
/// ```no_run
/// # use game_toolkit_compute::ComputeSubmitter;
/// # use game_toolkit_compute::wgpu;
/// # fn update(device: &wgpu::Device, queue: &wgpu::Queue, sim: &ComputeSubmitter) {
/// // In `update`, independent of rendering (`ctx.gfx.device()`, `ctx.gfx.queue()` in a game):
/// sim.try_submit(device, queue, |encoder| {
///     // kernel.dispatch_for(encoder, groups.current(&cells), w, h); cells.swap();
///     // readback.request(encoder, &counter, 0..1);
/// });
/// # }
/// ```
#[derive(Debug)]
pub struct ComputeSubmitter {
    label: String,
    max_in_flight: usize,
    in_flight: Arc<AtomicUsize>,
}

impl ComputeSubmitter {
    /// `max_in_flight` submissions may run at once (at least 1; 2 or 3 keeps the GPU fed
    /// without adding much latency).
    pub fn new(label: &str, max_in_flight: usize) -> Self {
        Self {
            label: label.to_owned(),
            max_in_flight: max_in_flight.max(1),
            in_flight: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Submissions the GPU hasn't finished yet. Polls the device once without waiting, so
    /// finished work is counted out natively; on the web the browser reports it between
    /// frames.
    pub fn in_flight(&self, device: &wgpu::Device) -> usize {
        let _ = device.poll(wgpu::PollType::Poll);
        self.in_flight.load(Ordering::Acquire)
    }

    /// Whether [`Self::try_submit`] would run a step now.
    pub fn ready(&self, device: &wgpu::Device) -> bool {
        self.in_flight(device) < self.max_in_flight
    }

    /// Record one submission with `record` and submit it, unless `max_in_flight` are still
    /// running; returns whether it was submitted. A [`ReadbackRing`](crate::ReadbackRing)
    /// request recorded here maps when this submission completes, not with the frame.
    pub fn try_submit(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        record: impl FnOnce(&mut wgpu::CommandEncoder),
    ) -> bool {
        if !self.ready(device) {
            return false;
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some(&self.label),
        });
        record(&mut encoder);
        self.submit(queue, encoder);
        true
    }

    /// Submit an encoder you recorded yourself, counting it as in flight until done. Doesn't
    /// check the limit; see [`Self::ready`].
    pub fn submit(&self, queue: &wgpu::Queue, encoder: wgpu::CommandEncoder) {
        self.in_flight.fetch_add(1, Ordering::AcqRel);
        queue.submit(std::iter::once(encoder.finish()));
        let counter = Arc::clone(&self.in_flight);
        queue.on_submitted_work_done(move || {
            counter.fetch_sub(1, Ordering::AcqRel);
        });
    }
}
