//! Touch input: every finger on the screen as a ring with a line back to where it went down,
//! a burst where a finger lifts (green for a tap, blue for a swipe), and counters. The mouse
//! draws a small crosshair so it's easy to see touches don't move it.
//!
//! Best on a phone or tablet: `task web:lan WEB_DIR=examples/15_touch`, then open one of the
//! LAN addresses trunk prints on a device in the same network. On a desktop with a
//! touchscreen, `task example:touch` runs it natively.

use game_toolkit::prelude::*;

/// A finger that moved less than this (in logical pixels) between down and up was a tap.
const TAP_SLOP: f32 = 12.0;
/// How long a lift burst stays on screen, in seconds.
const BURST_TIME: f32 = 0.6;

/// Distinct colors so simultaneous fingers are easy to tell apart.
const FINGER_COLORS: [[f32; 4]; 5] = [
    [1.0, 0.45, 0.35, 1.0],
    [1.0, 0.8, 0.3, 1.0],
    [0.45, 0.9, 0.5, 1.0],
    [0.4, 0.7, 1.0, 1.0],
    [0.8, 0.5, 1.0, 1.0],
];

struct Burst {
    pos: (f32, f32),
    tap: bool,
    age: f32,
}

#[derive(Default)]
struct TouchDemo {
    taps: u32,
    swipes: u32,
    most_fingers: usize,
    any_presses: u32,
    bursts: Vec<Burst>,
}

impl Game for TouchDemo {
    fn init(_ctx: &mut Context) -> Result<Self> {
        Ok(Self::default())
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        let scale = ctx.window.scale_factor() as f32;
        let input = &ctx.input;

        if input.any_pressed() {
            self.any_presses += 1;
        }
        self.most_fingers = self.most_fingers.max(input.touches().count());
        for t in input.touches_ended() {
            let (dx, dy) = (t.pos().0 - t.start_pos().0, t.pos().1 - t.start_pos().1);
            let tap = (dx * dx + dy * dy).sqrt() < TAP_SLOP * scale;
            if tap {
                self.taps += 1;
            } else {
                self.swipes += 1;
            }
            self.bursts.push(Burst {
                pos: t.pos(),
                tap,
                age: 0.0,
            });
        }
        for b in &mut self.bursts {
            b.age += dt;
        }
        self.bursts.retain(|b| b.age < BURST_TIME);
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        // Touch and mouse positions are physical pixels; scale sizes so they read the same on a
        // phone (device pixel ratio 2-3) as on a desktop monitor.
        let scale = ctx.window.scale_factor() as f32;
        let input = &ctx.input;
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.08, 0.08, 0.11, 1.0]);

        for b in &self.bursts {
            let k = b.age / BURST_TIME;
            let rgb = if b.tap {
                [0.45, 0.95, 0.55]
            } else {
                [0.4, 0.7, 1.0]
            };
            p.circle_outline(
                [b.pos.0, b.pos.1],
                (24.0 + 60.0 * k) * scale,
                3.0 * scale,
                [rgb[0], rgb[1], rgb[2], 1.0 - k],
            );
        }

        for t in input.touches() {
            let c = FINGER_COLORS[(t.id() % FINGER_COLORS.len() as u64) as usize];
            let (start, pos) = ([t.start_pos().0, t.start_pos().1], [t.pos().0, t.pos().1]);
            p.circle(start, 6.0 * scale, c);
            p.line(start, pos, 3.0 * scale, [c[0], c[1], c[2], 0.5]);
            p.circle_outline(pos, 44.0 * scale, 4.0 * scale, c);
            p.text(
                [pos[0] + 50.0 * scale, pos[1] - 50.0 * scale],
                &format!("#{}", t.id()),
                16.0 * scale,
                c,
            );
        }

        let (mx, my) = input.mouse_pos();
        let arm = 8.0 * scale;
        let grey = [0.5, 0.5, 0.55, 1.0];
        p.line([mx - arm, my], [mx + arm, my], 1.5 * scale, grey);
        p.line([mx, my - arm], [mx, my + arm], 1.5 * scale, grey);

        let fg = [0.85, 0.85, 0.9, 1.0];
        let (x, line) = (12.0 * scale, 24.0 * scale);
        p.text(
            [x, x],
            "15_touch: put fingers on the screen",
            20.0 * scale,
            fg,
        );
        p.text(
            [x, x + line],
            &format!(
                "down: {}  most at once: {}",
                input.touches().count(),
                self.most_fingers
            ),
            16.0 * scale,
            [0.6, 0.8, 1.0, 1.0],
        );
        p.text(
            [x, x + 2.0 * line],
            &format!(
                "taps: {}  swipes: {}  any_pressed: {}",
                self.taps, self.swipes, self.any_presses
            ),
            16.0 * scale,
            [0.6, 0.8, 1.0, 1.0],
        );
    }
}

fn main() -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    env_logger::init();

    run::<TouchDemo>(AppConfig {
        title: "15_touch".into(),
        width: 960,
        height: 540,
        canvas_id: Some("game".into()),
        // No assets: skip the manifest fetch on the web.
        web_preload: Some(Vec::new()),
        ..Default::default()
    })
}
