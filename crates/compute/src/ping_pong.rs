use std::marker::PhantomData;

use bytemuck::Pod;
use game_toolkit_gfx::wgpu::{self, util::DeviceExt};

use crate::Kernel;

/// Two equally sized storage buffers of `T` for simulations that read the last state and
/// write the next: a step reads the [front](Self::front) and writes the [back](Self::back),
/// then [`swap`](Self::swap) makes the result the new front.
///
/// Both buffers have `STORAGE | VERTEX | COPY_SRC | COPY_DST` usage: kernels bind them, the
/// game's own render passes can draw straight from the [front](Self::front) as a
/// vertex/instance buffer (or bind it as `var<storage, read>`, where the adapter allows
/// storage buffers in vertex shaders), a [`ReadbackRing`](crate::ReadbackRing) reads them
/// back, and the CPU writes them. Simulation state never has to leave the GPU to be drawn.
#[derive(Debug)]
pub struct PingPong<T: Pod> {
    buffers: [wgpu::Buffer; 2],
    front: usize,
    len: usize,
    _marker: PhantomData<T>,
}

impl<T: Pod> PingPong<T> {
    /// Create both buffers holding a copy of `initial`.
    ///
    /// # Panics
    ///
    /// If `initial` is empty (wgpu cannot bind an empty storage buffer).
    pub fn new(device: &wgpu::Device, label: &str, initial: &[T]) -> Self {
        assert!(
            !initial.is_empty(),
            "PingPong `{label}` needs at least one element"
        );
        let usage = wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::VERTEX
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST;
        let make = |suffix: &str| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("{label}.{suffix}")),
                contents: bytemuck::cast_slice(initial),
                usage,
            })
        };
        Self {
            buffers: [make("a"), make("b")],
            front: 0,
            len: initial.len(),
            _marker: PhantomData,
        }
    }

    /// Number of `T` elements in each buffer.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Always `false`: [`Self::new`] rejects empty data.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The buffer holding the latest state: what a step reads, and what to draw or read back.
    pub fn front(&self) -> &wgpu::Buffer {
        &self.buffers[self.front]
    }

    /// The buffer the next step writes.
    pub fn back(&self) -> &wgpu::Buffer {
        &self.buffers[1 - self.front]
    }

    /// Make the back buffer the front, after recording a step that wrote it.
    pub fn swap(&mut self) {
        self.front = 1 - self.front;
    }

    /// Overwrite the front buffer from the CPU, starting at element `offset` (e.g. to reset
    /// the simulation or paint cells). Lands before any GPU work submitted after this call.
    ///
    /// # Panics
    ///
    /// If `offset + data.len()` exceeds [`Self::len`], or the byte offset or size is not a
    /// multiple of 4 (wgpu's copy alignment; always fine for 4-byte `T` like `u32`/`f32`).
    pub fn write(&self, queue: &wgpu::Queue, offset: usize, data: &[T]) {
        assert!(
            offset + data.len() <= self.len,
            "PingPong::write of {} elements at {offset} overruns {} elements",
            data.len(),
            self.len
        );
        let byte_offset = offset * size_of::<T>();
        let byte_len = size_of_val(data);
        assert!(
            byte_offset.is_multiple_of(4) && byte_len.is_multiple_of(4),
            "PingPong::write: byte offset {byte_offset} and size {byte_len} must be multiples of 4"
        );
        let byte_offset = byte_offset as wgpu::BufferAddress;
        queue.write_buffer(self.front(), byte_offset, bytemuck::cast_slice(data));
    }

    /// Bind groups for `kernel` in both orderings, `@binding(0)` the buffer read and
    /// `@binding(1)` the one written, followed by `extra` from `@binding(2)` on. Declare the
    /// first two as [`Binding::StorageRead`](crate::Binding::StorageRead) and
    /// [`Binding::StorageReadWrite`](crate::Binding::StorageReadWrite). Pick the one for the
    /// current ordering with [`PingPongBindGroups::current`].
    pub fn bind_groups(
        &self,
        device: &wgpu::Device,
        kernel: &Kernel,
        extra: &[wgpu::BindingResource<'_>],
    ) -> PingPongBindGroups {
        let make = |read: &wgpu::Buffer, write: &wgpu::Buffer| {
            let mut resources = vec![read.as_entire_binding(), write.as_entire_binding()];
            resources.extend_from_slice(extra);
            kernel.bind_group(device, &resources)
        };
        PingPongBindGroups {
            // Index = which buffer is the front when the group is used.
            groups: [
                make(&self.buffers[0], &self.buffers[1]),
                make(&self.buffers[1], &self.buffers[0]),
            ],
        }
    }
}

/// The two bind groups made by [`PingPong::bind_groups`]; they stay valid across swaps.
#[derive(Debug)]
pub struct PingPongBindGroups {
    groups: [wgpu::BindGroup; 2],
}

impl PingPongBindGroups {
    /// The group that reads `buffers`' current front and writes its back.
    pub fn current<T: Pod>(&self, buffers: &PingPong<T>) -> &wgpu::BindGroup {
        &self.groups[buffers.front]
    }
}
