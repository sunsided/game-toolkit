//! Visualizes the first connected gamepad: stick positions, face/shoulder buttons, and a
//! rumble pulse on the South (A) button. Hot-plug works - connect a pad while it runs.
//!
//! Gamepad support degrades gracefully: with no backend or no controller the window still
//! opens and shows a prompt.

use game_toolkit_prelude::*;

struct App;

/// A frame's worth of gamepad state, snapshotted so drawing borrows nothing live.
struct Pad {
    name: String,
    left: [f32; 2],
    right: [f32; 2],
    l3: bool,
    r3: bool,
    south: bool,
    east: bool,
    west: bool,
    north: bool,
    lb: bool,
    rb: bool,
    lt: bool,
    rt: bool,
}

fn snapshot(pad: &Gamepad) -> Pad {
    Pad {
        name: pad.name().to_string(),
        left: [pad.axis(Axis::LeftStickX), pad.axis(Axis::LeftStickY)],
        right: [pad.axis(Axis::RightStickX), pad.axis(Axis::RightStickY)],
        l3: pad.button_held(Button::LeftThumb),
        r3: pad.button_held(Button::RightThumb),
        south: pad.button_held(Button::South),
        east: pad.button_held(Button::East),
        west: pad.button_held(Button::West),
        north: pad.button_held(Button::North),
        lb: pad.button_held(Button::LeftTrigger),
        rb: pad.button_held(Button::RightTrigger),
        lt: pad.button_held(Button::LeftTrigger2),
        rt: pad.button_held(Button::RightTrigger2),
    }
}

fn stick(p: &mut Painter, center: [f32; 2], axes: [f32; 2], clicked: bool, label: &str) {
    let r = 70.0;
    // Clicking the stick (L3/R3) fills the ring to highlight it.
    if clicked {
        p.circle(center, r, [0.30, 0.45, 0.30, 1.0]);
    }
    p.circle_outline(center, r, 3.0, [0.5, 0.5, 0.6, 1.0]);
    // Screen y is down, so negate the stick's up-positive Y.
    let dot = [center[0] + axes[0] * r, center[1] - axes[1] * r];
    let dot_color = if clicked {
        [1.0, 0.85, 0.2, 1.0]
    } else {
        [0.4, 0.8, 1.0, 1.0]
    };
    p.circle(dot, 12.0, dot_color);
    p.text([center[0] - 24.0, center[1] + r + 8.0], label, 18.0, [0.8, 0.8, 0.85, 1.0]);
}

fn button(p: &mut Painter, center: [f32; 2], on: bool, label: &str) {
    let color = if on {
        [1.0, 0.85, 0.2, 1.0]
    } else {
        [0.30, 0.32, 0.38, 1.0]
    };
    p.circle(center, 18.0, color);
    p.text([center[0] - 6.0, center[1] - 9.0], label, 18.0, [0.1, 0.1, 0.12, 1.0]);
}

impl Game for App {
    fn init(_ctx: &mut Context) -> Result<Self> {
        Ok(Self)
    }

    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        // Rumble on South press. Resolve the id, dropping the borrow before set_rumble.
        let rumble = ctx
            .input
            .first_gamepad()
            .filter(|p| p.button_pressed(Button::South))
            .map(|p| p.id());
        if let Some(id) = rumble {
            ctx.input.set_rumble(id, 0.7, 200);
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let pad = ctx.input.first_gamepad().map(snapshot);

        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.06, 0.07, 0.1, 1.0]);
        p.text([16.0, 16.0], "10_gamepad (Esc to quit)", 22.0, [0.9, 0.9, 0.95, 1.0]);

        let Some(pad) = pad else {
            p.text(
                [16.0, 56.0],
                "No gamepad connected - plug one in (hot-plug works).",
                20.0,
                [0.9, 0.6, 0.4, 1.0],
            );
            return;
        };

        p.text([16.0, 56.0], &format!("pad: {}", pad.name), 18.0, [0.6, 0.85, 0.7, 1.0]);

        stick(&mut p, [200.0, 320.0], pad.left, pad.l3, "L stick (L3)");
        stick(&mut p, [430.0, 320.0], pad.right, pad.r3, "R stick (R3)");

        // Face buttons in a diamond (N top, S bottom, W left, E right).
        let fc = [660.0, 320.0];
        button(&mut p, [fc[0], fc[1] - 40.0], pad.north, "Y");
        button(&mut p, [fc[0], fc[1] + 40.0], pad.south, "A");
        button(&mut p, [fc[0] - 40.0, fc[1]], pad.west, "X");
        button(&mut p, [fc[0] + 40.0, fc[1]], pad.east, "B");

        // Shoulders / triggers.
        button(&mut p, [180.0, 120.0], pad.lb, "LB");
        button(&mut p, [180.0, 170.0], pad.lt, "LT");
        button(&mut p, [690.0, 120.0], pad.rb, "RB");
        button(&mut p, [690.0, 170.0], pad.rt, "RT");

        p.text([520.0, 430.0], "Press A to rumble", 18.0, [0.8, 0.8, 0.85, 1.0]);
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<App>(AppConfig {
        title: "10_gamepad".into(),
        width: 880,
        height: 500,
        ..Default::default()
    })
}
