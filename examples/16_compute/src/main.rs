//! Conway's Game of Life on the GPU, with the `compute` feature.
//!
//! - The grid lives in a `PingPong<u32>`: each generation reads one buffer and writes the
//!   other, then `swap()`s. Its bind groups for both orderings are built once in `init`.
//! - One `Kernel` steps the grid, paints a `StorageTexture` (drawn as an ordinary sprite) and
//!   counts the live cells with an atomic.
//! - The simulation is offloaded: `update` submits generations through a `ComputeSubmitter`
//!   at the simulation's own rate, separately from the frame, and skips a batch while the GPU
//!   is still working on earlier ones. `render` only draws the texture, on the GPU as well,
//!   without the cells ever coming back to the CPU.
//! - A `ReadbackRing` brings the count back to the CPU. It never waits for the GPU, so the
//!   number on screen is a frame or a few old; the "in flight" figure shows how many requests
//!   are still on their way.
//!
//! Hold the left mouse button to draw cells, R to reseed, C to clear, Space to pause, Up/Down
//! to change the speed. On an adapter without compute shaders (WebGL2) it only shows a note.

use game_toolkit::prelude::wgpu::util::DeviceExt;
use game_toolkit::prelude::*;

const WIDTH: u32 = 256;
const HEIGHT: u32 = 160;
const SCALE: f32 = 4.0;
/// Cell value of a live cell (see life.wgsl).
const ALIVE: u32 = 255;
const HUD_BAR: [f32; 4] = [0.0, 0.0, 0.0, 0.85];

/// Everything on the GPU side; `None` when the adapter has no compute shaders.
struct Sim {
    kernel: Kernel,
    cells: PingPong<u32>,
    groups: PingPongBindGroups,
    /// A single `atomic<u32>`, cleared before each generation.
    live: wgpu::Buffer,
    texture: StorageTexture,
    readback: ReadbackRing<u32>,
    /// Bind groups hold their resources, but keep the uniform alive explicitly anyway.
    _params: wgpu::Buffer,
}

struct Life {
    sim: Option<Sim>,
    /// Submits generations separately from the frame, at most two batches in flight.
    submitter: ComputeSubmitter,
    backend: wgpu::Backend,
    paused: bool,
    generations_per_second: f32,
    /// Generations owed, accumulated and submitted in `update`.
    pending: f32,
    live_count: Option<u32>,
    /// Batches skipped because the GPU was still busy (shows the throttle working).
    skipped: u32,
}

impl Sim {
    fn new(ctx: &mut Context) -> Self {
        let device = ctx.gfx.device();
        let kernel = Kernel::builder("life", include_str!("life.wgsl"))
            .workgroup_size([8, 8, 1])
            .bindings(&[
                Binding::StorageRead,                                 // 0: src (PingPong front)
                Binding::StorageReadWrite,                            // 1: dst (PingPong back)
                Binding::Uniform,                                     // 2: params
                Binding::StorageReadWrite,                            // 3: live-cell counter
                Binding::StorageTextureWrite(StorageTexture::FORMAT), // 4: output
            ])
            .build(device);

        let cells = PingPong::new(device, "life.cells", &seed(&mut ctx.rng));
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("life.params"),
            contents: bytemuck::cast_slice(&[WIDTH, HEIGHT, 0, 0]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let live = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("life.live"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let readback = ReadbackRing::new(device, "life.live", 1, 4);
        let texture = StorageTexture::new(&mut ctx.gfx, WIDTH, HEIGHT, "life.texture");

        let device = ctx.gfx.device();
        let groups = cells.bind_groups(
            device,
            &kernel,
            &[
                params.as_entire_binding(),
                live.as_entire_binding(),
                texture.binding(),
            ],
        );
        Self {
            kernel,
            cells,
            groups,
            live,
            texture,
            readback,
            _params: params,
        }
    }

    /// Record one generation into `encoder`.
    fn step(&mut self, encoder: &mut wgpu::CommandEncoder) {
        encoder.clear_buffer(&self.live, 0, None);
        self.kernel
            .dispatch_for(encoder, self.groups.current(&self.cells), WIDTH, HEIGHT);
        self.cells.swap();
    }

    /// Set the cells in a small square around `(cx, cy)` alive.
    fn paint(&self, queue: &wgpu::Queue, cx: i32, cy: i32) {
        const R: i32 = 2;
        let row = [ALIVE; (2 * R + 1) as usize];
        for y in (cy - R).max(0)..=(cy + R).min(HEIGHT as i32 - 1) {
            let x0 = (cx - R).max(0);
            let x1 = (cx + R).min(WIDTH as i32 - 1);
            if x0 > x1 {
                continue;
            }
            let offset = (y as u32 * WIDTH + x0 as u32) as usize;
            self.cells
                .write(queue, offset, &row[..(x1 - x0 + 1) as usize]);
        }
    }
}

/// A random grid with about a quarter of the cells alive.
fn seed(rng: &mut Rng) -> Vec<u32> {
    (0..WIDTH * HEIGHT)
        .map(|_| if rng.next_f32() < 0.25 { ALIVE } else { 0 })
        .collect()
}

impl Game for Life {
    fn init(ctx: &mut Context) -> Result<Self> {
        let backend = ctx.gfx.backend();
        let sim = if ctx.gfx.supports_compute() {
            Some(Sim::new(ctx))
        } else {
            None
        };
        Ok(Self {
            sim,
            submitter: ComputeSubmitter::new("life.step", 2),
            backend,
            paused: false,
            generations_per_second: 20.0,
            // One generation right away, so the texture is filled even when paused.
            pending: 1.0,
            live_count: None,
            skipped: 0,
        })
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        if ctx.input.key_pressed(Key::Space) {
            self.paused = !self.paused;
        }
        if ctx.input.key_pressed(Key::ArrowUp) {
            self.generations_per_second = (self.generations_per_second * 2.0).min(240.0);
        }
        if ctx.input.key_pressed(Key::ArrowDown) {
            self.generations_per_second = (self.generations_per_second / 2.0).max(1.25);
        }
        if !self.paused {
            self.pending += dt * self.generations_per_second;
        }

        let Some(sim) = &mut self.sim else {
            return;
        };
        // CPU writes go to the PingPong's front buffer and land before this frame's steps.
        if ctx.input.key_pressed(Key::KeyR) {
            sim.cells.write(ctx.gfx.queue(), 0, &seed(&mut ctx.rng));
        }
        if ctx.input.key_pressed(Key::KeyC) {
            sim.cells
                .write(ctx.gfx.queue(), 0, &vec![0; (WIDTH * HEIGHT) as usize]);
        }
        if ctx.input.mouse_held(MouseButton::Left) {
            let (mx, my) = ctx.input.mouse_pos();
            sim.paint(
                ctx.gfx.queue(),
                (mx / SCALE).floor() as i32,
                (my / SCALE).floor() as i32,
            );
        }

        // Offload: submit the generations owed in their own submission, independent of the
        // frame. At most a few per batch so a hitch doesn't snowball; while the GPU still has
        // two batches in flight, skip and try again next update.
        let steps = (self.pending.floor() as u32).min(4);
        if steps > 0 {
            let submitted =
                self.submitter
                    .try_submit(ctx.gfx.device(), ctx.gfx.queue(), |encoder| {
                        for _ in 0..steps {
                            sim.step(encoder);
                        }
                        // Skipped (returns false) while every slot is still in flight.
                        sim.readback.request(encoder, &sim.live, 0..1);
                    });
            if submitted {
                self.pending = (self.pending - steps as f32).min(1.0);
            } else {
                self.skipped += 1;
                self.pending = self.pending.min(4.0);
            }
        }

        // Results of earlier requests; keep the newest.
        while let Some(values) = sim.readback.try_take(ctx.gfx.device()) {
            self.live_count = Some(values[0]);
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let Some(sim) = &mut self.sim else {
            let mut p = frame.painter(&mut ctx.gfx);
            p.clear([0.05, 0.07, 0.12, 1.0]);
            p.text(
                [24.0, 24.0],
                &format!(
                    "This example needs compute shaders, which {:?} lacks.\n\
                     In a browser, use one with WebGPU.",
                    self.backend
                ),
                22.0,
                [1.0, 1.0, 1.0, 1.0],
            );
            return;
        };

        // The simulation was submitted in `update`; drawing just samples its texture.
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.0, 0.0, 0.0, 1.0]);
        p.sprite(
            sim.texture.id(),
            [0.0, 0.0],
            [WIDTH as f32 * SCALE, HEIGHT as f32 * SCALE],
        );
        // Bars behind the HUD text, on a layer above the grid sprite.
        let screen_w = WIDTH as f32 * SCALE;
        let screen_h = HEIGHT as f32 * SCALE;
        p.rect_ex([0.0, 0.0], [screen_w, 40.0], HUD_BAR, 1, BlendMode::Alpha);
        p.rect_ex(
            [0.0, screen_h - 38.0],
            [screen_w, 38.0],
            HUD_BAR,
            1,
            BlendMode::Alpha,
        );
        let count = self
            .live_count
            .map_or_else(|| "...".to_owned(), |n| n.to_string());
        p.text(
            [12.0, 12.0],
            &format!(
                "live cells: {count}  (read back, {} in flight)   {:.0} gen/s   {} skipped{}",
                sim.readback.in_flight(),
                self.generations_per_second,
                self.skipped,
                if self.paused { "  [paused]" } else { "" },
            ),
            18.0,
            [1.0, 1.0, 1.0, 1.0],
        );
        p.text(
            [12.0, screen_h - 28.0],
            "LMB draw   R reseed   C clear   Space pause   Up/Down speed   Esc quit",
            16.0,
            [0.8, 0.85, 0.9, 1.0],
        );
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<Life>(AppConfig {
        title: "16_compute".into(),
        width: (WIDTH as f32 * SCALE) as u32,
        height: (HEIGHT as f32 * SCALE) as u32,
        ..Default::default()
    })
}
