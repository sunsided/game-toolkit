//! The compute helpers on a device made here with plain wgpu: no window, no game-toolkit-gfx
//! (run with `--no-default-features` to check that too). Skips when there is no GPU adapter.

use game_toolkit_compute::wgpu;
use game_toolkit_compute::{Binding, ComputeSubmitter, Kernel, PingPong, ReadbackRing};

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }));
    let Ok(adapter) = adapter else {
        eprintln!("no GPU adapter; skipping");
        return None;
    };
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

/// Each step adds 1 to every element (read front, write back); after `STEPS` steps through
/// a `ComputeSubmitter`, a `ReadbackRing` brings the values back.
#[test]
fn simulation_steps_and_reads_back_without_a_window() {
    const LEN: u32 = 1000;
    const STEPS: u32 = 5;
    let Some((device, queue)) = device() else {
        return;
    };
    let kernel = Kernel::builder(
        "inc",
        "@group(0) @binding(0) var<storage, read> src: array<u32>;
         @group(0) @binding(1) var<storage, read_write> dst: array<u32>;
         @compute @workgroup_size(WORKGROUP_SIZE_X)
         fn main(@builtin(global_invocation_id) id: vec3<u32>) {
             if id.x < arrayLength(&dst) { dst[id.x] = src[id.x] + 1u; }
         }",
    )
    .workgroup_size([64, 1, 1])
    .bindings(&[Binding::StorageRead, Binding::StorageReadWrite])
    .build(&device);
    let initial: Vec<u32> = (0..LEN).collect();
    let mut cells = PingPong::new(&device, "cells", &initial);
    let groups = cells.bind_groups(&device, &kernel, &[]);
    let sim = ComputeSubmitter::new("sim", 2);
    let mut ring = ReadbackRing::<u32>::new(&device, "cells", LEN as usize, 2);

    let mut steps = 0;
    while steps < STEPS {
        if sim.try_submit(&device, &queue, |encoder| {
            kernel.dispatch_for(encoder, groups.current(&cells), LEN, 1);
            cells.swap();
        }) {
            steps += 1;
        } else {
            device.poll(wgpu::PollType::Poll).expect("poll");
        }
    }
    let mut encoder = device.create_command_encoder(&Default::default());
    assert!(ring.request(&mut encoder, cells.front(), 0..LEN as usize));
    queue.submit([encoder.finish()]);

    let values = loop {
        if let Some(values) = ring.try_take(&device) {
            break values;
        }
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
    };
    let expected: Vec<u32> = (0..LEN).map(|i| i + STEPS).collect();
    assert_eq!(values, expected);
}
