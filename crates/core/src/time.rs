use std::time::{Duration, Instant};

pub struct Time {
    pub elapsed: Duration,
    pub delta: Duration,
    pub frame: u64,
    pub fps: f32,
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
