use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use bytemuck::Pod;
use game_toolkit_gfx::wgpu;

/// Map state of one request, written by the `map_buffer_on_submit` callback.
const PENDING: u8 = 0;
const MAPPED: u8 = 1;
const FAILED: u8 = 2;

/// Reads small results back from the GPU without stalling the frame.
///
/// [`request`](Self::request) records a copy from a GPU buffer into one of `N` staging
/// buffers and asks wgpu to map it once the frame is submitted. The data shows up in
/// [`try_take`](Self::try_take) a frame or a few later, when the GPU has caught up; the game
/// keeps running meanwhile and works with slightly stale numbers (a live-cell count, a score
/// computed on the GPU, a probe of a simulated field). Nothing here blocks, so it works the
/// same on WebGPU, where the browser maps buffers asynchronously.
///
/// Each staging buffer holds one request until it is taken; with all of them in flight,
/// `request` skips instead of waiting. Three or four slots cover the usual latency.
///
/// ```no_run
/// # use game_toolkit_compute::ReadbackRing;
/// # use game_toolkit_gfx::wgpu;
/// # fn demo(device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, counter: &wgpu::Buffer) {
/// let mut ring = ReadbackRing::<u32>::new(device, "count", 1, 3);
/// // Every frame, after the work that fills `counter` is recorded into the frame's encoder:
/// ring.request(encoder, counter, 0..1);
/// // Every frame (any time, it doesn't need the encoder):
/// while let Some(values) = ring.try_take(device) {
///     println!("count from a few frames ago: {}", values[0]);
/// }
/// # }
/// ```
#[derive(Debug)]
pub struct ReadbackRing<T: Pod> {
    staging: Vec<wgpu::Buffer>,
    /// Per slot: the state flag shared with that request's map callback.
    status: Vec<Option<Arc<AtomicU8>>>,
    slots: Slots,
    capacity: usize,
    _marker: PhantomData<T>,
}

impl<T: Pod> ReadbackRing<T> {
    /// `slots` staging buffers, each holding up to `capacity` elements of `T`.
    ///
    /// # Panics
    ///
    /// If `capacity` or `slots` is zero, or `T`'s size is not a non-zero multiple of 4 bytes
    /// (wgpu copies and maps in 4-byte units; use `u32`, `f32`, `[f32; 2]`, or pad your struct).
    pub fn new(device: &wgpu::Device, label: &str, capacity: usize, slots: usize) -> Self {
        assert!(capacity > 0, "ReadbackRing `{label}` needs a capacity");
        assert!(slots > 0, "ReadbackRing `{label}` needs at least one slot");
        assert!(
            size_of::<T>() > 0 && size_of::<T>().is_multiple_of(4),
            "ReadbackRing `{label}`: element size {} is not a multiple of 4 bytes",
            size_of::<T>()
        );
        let staging = (0..slots)
            .map(|i| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("{label}.staging{i}")),
                    size: (capacity * size_of::<T>()) as wgpu::BufferAddress,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        Self {
            staging,
            status: vec![None; slots],
            slots: Slots::new(slots),
            capacity,
            _marker: PhantomData,
        }
    }

    /// Maximum elements per request.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Requests recorded but not taken yet.
    pub fn in_flight(&self) -> usize {
        self.slots.in_flight()
    }

    /// Record a copy of elements `range` of `src` (which needs `COPY_SRC` usage) into a free
    /// staging buffer, mapped for reading once `encoder` is submitted. Record it after the
    /// commands that produce the data. Returns `false` and records nothing when every slot is
    /// still in flight (take results more often, or add slots) or `range` is empty or longer
    /// than [`Self::capacity`].
    pub fn request(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        src: &wgpu::Buffer,
        range: Range<usize>,
    ) -> bool {
        let len = range.len();
        if len == 0 || len > self.capacity {
            log::warn!(
                "ReadbackRing::request: range {range:?} is empty or exceeds capacity {}",
                self.capacity
            );
            return false;
        }
        let Some(slot) = self.slots.acquire(len) else {
            return false;
        };
        let size = (len * size_of::<T>()) as wgpu::BufferAddress;
        let offset = (range.start * size_of::<T>()) as wgpu::BufferAddress;
        let staging = &self.staging[slot];
        encoder.copy_buffer_to_buffer(src, offset, staging, 0, Some(size));

        let flag = Arc::new(AtomicU8::new(PENDING));
        let in_callback = Arc::clone(&flag);
        encoder.map_buffer_on_submit(staging, wgpu::MapMode::Read, 0..size, move |result| {
            let state = if result.is_ok() { MAPPED } else { FAILED };
            in_callback.store(state, Ordering::Release);
        });
        self.status[slot] = Some(flag);
        true
    }

    /// The oldest request whose data has arrived, if any; `None` while the GPU is still
    /// busy. Never blocks: natively it polls the device once without waiting, on the web the
    /// browser delivers the mapping between frames. Call it every frame, or in a `while let`
    /// loop to drain everything that arrived and keep the newest.
    pub fn try_take(&mut self, device: &wgpu::Device) -> Option<Vec<T>> {
        // Non-blocking; any error shows up as a failed map below.
        let _ = device.poll(wgpu::PollType::Poll);

        for slot in self.slots.oldest_first() {
            let Some(flag) = &self.status[slot] else {
                continue;
            };
            // Read the count before the state: the callback stores the state, then drops its
            // clone, so a count of 1 with the state still pending means it never ran.
            let abandoned = Arc::strong_count(flag) == 1;
            match flag.load(Ordering::Acquire) {
                MAPPED => {
                    let data = self.read(slot);
                    self.release(slot);
                    if data.is_some() {
                        return data;
                    }
                }
                FAILED => self.release(slot),
                // The encoder holding the request was dropped without being submitted.
                _ if abandoned => self.release(slot),
                _ => {}
            }
        }
        None
    }

    fn read(&self, slot: usize) -> Option<Vec<T>> {
        let len = self.slots.len_of(slot)?;
        let size = (len * size_of::<T>()) as wgpu::BufferAddress;
        let buffer = &self.staging[slot];
        let data = match buffer.get_mapped_range(0..size) {
            // `pod_collect_to_vec` copies, so the mapping's alignment doesn't matter.
            Ok(view) => Some(bytemuck::pod_collect_to_vec(&view)),
            Err(e) => {
                log::warn!("ReadbackRing: mapped range unavailable: {e}");
                None
            }
        };
        buffer.unmap();
        data
    }

    fn release(&mut self, slot: usize) {
        self.status[slot] = None;
        self.slots.release(slot);
    }
}

/// GPU-free bookkeeping of which staging buffers are free and in what order the busy ones
/// were requested.
#[derive(Debug)]
struct Slots {
    slots: Vec<Slot>,
    next_seq: u64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Slot {
    Free,
    InFlight { seq: u64, len: usize },
}

impl Slots {
    fn new(n: usize) -> Self {
        Self {
            slots: vec![Slot::Free; n],
            next_seq: 0,
        }
    }

    /// Claim a free slot for a request of `len` elements.
    fn acquire(&mut self, len: usize) -> Option<usize> {
        let i = self.slots.iter().position(|s| *s == Slot::Free)?;
        self.slots[i] = Slot::InFlight {
            seq: self.next_seq,
            len,
        };
        self.next_seq += 1;
        Some(i)
    }

    fn release(&mut self, i: usize) {
        self.slots[i] = Slot::Free;
    }

    fn len_of(&self, i: usize) -> Option<usize> {
        match self.slots[i] {
            Slot::InFlight { len, .. } => Some(len),
            Slot::Free => None,
        }
    }

    fn in_flight(&self) -> usize {
        self.slots.iter().filter(|s| **s != Slot::Free).count()
    }

    /// In-flight slot indices, oldest request first.
    fn oldest_first(&self) -> Vec<usize> {
        let mut busy: Vec<(u64, usize)> = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match s {
                Slot::InFlight { seq, .. } => Some((*seq, i)),
                Slot::Free => None,
            })
            .collect();
        busy.sort_unstable();
        busy.into_iter().map(|(_, i)| i).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_until_full_then_skip() {
        let mut s = Slots::new(2);
        assert_eq!(s.acquire(1), Some(0));
        assert_eq!(s.acquire(3), Some(1));
        assert_eq!(s.acquire(1), None);
        assert_eq!(s.in_flight(), 2);
    }

    #[test]
    fn released_slot_is_reused_and_ordered_after_older_ones() {
        let mut s = Slots::new(3);
        let a = s.acquire(1).unwrap();
        let b = s.acquire(2).unwrap();
        let c = s.acquire(3).unwrap();
        assert_eq!(s.oldest_first(), vec![a, b, c]);

        s.release(a);
        assert_eq!(s.len_of(a), None);
        let d = s.acquire(4).unwrap();
        assert_eq!(d, a, "the freed slot is reused");
        assert_eq!(s.len_of(d), Some(4));
        // The reused slot holds the newest request, so it comes last.
        assert_eq!(s.oldest_first(), vec![b, c, d]);
    }

    #[test]
    fn empty_ring_has_nothing_in_flight() {
        let s = Slots::new(4);
        assert_eq!(s.in_flight(), 0);
        assert!(s.oldest_first().is_empty());
    }
}
