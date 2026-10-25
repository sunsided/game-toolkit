use std::time::Duration;

// `std::time::Instant::now` panics on wasm32-unknown-unknown; web-time re-exports std's
// `Instant` on native and uses `performance.now()` in the browser.
use web_time::Instant;

/// Frame timing, as `ctx.time`.
pub struct Time {
    /// Time since startup.
    pub elapsed: Duration,
    /// Real time since the previous frame, uncapped. The `dt` passed to `Game::update` is
    /// this capped at [`AppConfig::max_frame_time`](crate::AppConfig::max_frame_time) (or the
    /// fixed step, with [`AppConfig::fixed_timestep`](crate::AppConfig::fixed_timestep)).
    pub delta: Duration,
    /// Frames since startup.
    pub frame: u64,
    /// Smoothed frames per second.
    pub fps: f32,
    /// With a fixed timestep: how far this frame is into the next step, in `0.0..1.0`. Draw
    /// `previous + (current - previous) * alpha` in `render` for motion that's smooth at
    /// display rates above the step rate. Always `0.0` without a fixed timestep.
    pub alpha: f32,
    start: Instant,
    last_frame: Instant,
    fps_accum: f32,
}

impl Time {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            elapsed: Duration::ZERO,
            delta: Duration::ZERO,
            frame: 0,
            fps: 0.0,
            alpha: 0.0,
            start: now,
            last_frame: now,
            fps_accum: 0.0,
        }
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        self.delta = now - self.last_frame;
        self.elapsed = now - self.start;
        self.last_frame = now;
        self.frame += 1;
        let inst = 1.0 / self.delta.as_secs_f32().max(1e-6);
        self.fps_accum = self.fps_accum * 0.95 + inst * 0.05;
        self.fps = self.fps_accum;
    }
}

impl Default for Time {
    fn default() -> Self {
        Self::new()
    }
}
