//! GPU compute helpers for game-toolkit: the wgpu plumbing that simulations on the GPU
//! (cellular automata, heat or fluid fields, particles) otherwise repeat.
//!
//! - [`Kernel`]: a WGSL compute shader with its pipeline and bind-group layout, dispatched
//!   over a grid with [`Kernel::dispatch_for`] (workgroup counts rounded up).
//! - [`PingPong`]: two storage buffers, read one and write the other, then
//!   [`swap`](PingPong::swap); bind groups for both orderings are built once.
//! - [`ReadbackRing`]: copies small results into a ring of staging buffers and hands them
//!   back a few frames later, without ever blocking (so it works on WebGPU).
//! - [`StorageTexture`]: an RGBA texture a kernel writes and the `Painter` draws.
//! - [`ComputeSubmitter`]: runs simulation steps in their own submissions, from `update` at
//!   the simulation's own rate, and skips steps while the GPU is still busy with earlier ones.
//!
//! # Offloading a simulation while the game renders
//!
//! The usual setup: the simulation steps on the GPU through a [`ComputeSubmitter`] in
//! `update` (fixed tick or as fast as the GPU keeps up), and rendering draws its state on the
//! GPU too, from a [`StorageTexture`] sprite or straight from a [`PingPong`] buffer in a
//! custom pass (`VERTEX` usage is set). Submissions share the device queue in order, so every
//! frame draws the latest submitted state; nothing waits on the CPU, and the in-flight cap
//! keeps a heavy simulation from delaying frames. Small results come back to the game logic
//! through a [`ReadbackRing`], a few frames late. See `examples/16_compute`.
//!
//! For work that belongs to one frame, record into the frame's encoder instead; it then runs
//! right before that frame's drawing in the same submission:
//!
//! ```no_run
//! # use game_toolkit_compute::{Kernel, PingPong, PingPongBindGroups};
//! # use game_toolkit_gfx::{Frame, Graphics};
//! # fn render(gfx: &mut Graphics, frame: &mut Frame, kernel: &Kernel,
//! #           cells: &mut PingPong<u32>, groups: &PingPongBindGroups) {
//! if let Some(encoder) = frame.encoder_mut() {
//!     kernel.dispatch_for(encoder, groups.current(cells), 256, 256);
//!     cells.swap();
//! }
//! let mut p = frame.painter(gfx);
//! // ... draw ...
//! # }
//! ```
//!
//! Compute needs WebGPU in the browser; on its WebGL2 fallback
//! `Graphics::supports_compute()` is `false`, so check it before building kernels. A kernel
//! with more than four storage buffers or textures per stage needs higher device limits
//! (`AppConfig::required_limits`). WGSL only.
//!
//! Through the umbrella crate this is the `compute` feature: `game_toolkit::compute`.

#![forbid(unsafe_code)]

mod kernel;
mod ping_pong;
mod readback;
mod submit;
mod texture;

pub use kernel::{Binding, Kernel, KernelBuilder, workgroups};
pub use ping_pong::{PingPong, PingPongBindGroups};
pub use readback::ReadbackRing;
pub use submit::ComputeSubmitter;
pub use texture::StorageTexture;
