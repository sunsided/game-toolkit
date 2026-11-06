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
//!
//! Everything records into the frame's encoder, so the compute work runs right before the
//! frame's drawing in the same submission:
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
mod texture;

pub use kernel::{Binding, Kernel, KernelBuilder, workgroups};
pub use ping_pong::{PingPong, PingPongBindGroups};
pub use readback::ReadbackRing;
pub use texture::StorageTexture;
