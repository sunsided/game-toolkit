//! Input subsystem: keyboard + mouse + gamepads, with held / just-pressed / just-released
//! semantics.
//!
//! Call [`Input::handle_window_event`] for every `winit::event::WindowEvent` and
//! [`Input::poll_gamepads`] once per frame before `update`, then [`Input::end_frame`] once
//! per frame after `update` to clear edge state.

use std::collections::{HashMap, HashSet};

pub use winit::keyboard::KeyCode as Key;

use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Replay, Ticks};
pub use gilrs::{Axis, Button, GamepadId};
use gilrs::{Event, Gilrs};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u16),
}

impl From<winit::event::MouseButton> for MouseButton {
    fn from(b: winit::event::MouseButton) -> Self {
        match b {
            winit::event::MouseButton::Left => MouseButton::Left,
            winit::event::MouseButton::Right => MouseButton::Right,
            winit::event::MouseButton::Middle => MouseButton::Middle,
            winit::event::MouseButton::Back => MouseButton::Other(3),
            winit::event::MouseButton::Forward => MouseButton::Other(4),
            winit::event::MouseButton::Other(n) => MouseButton::Other(n),
        }
    }
}

/// Per-controller state, with the same held / just-pressed / just-released model as the
/// keyboard. Obtain via [`Input::gamepads`] or [`Input::first_gamepad`].
pub struct Gamepad {
    id: GamepadId,
    name: String,
    connected: bool,
    held: HashSet<Button>,
    pressed: HashSet<Button>,
    released: HashSet<Button>,
    axes: HashMap<Axis, f32>,
}

impl Gamepad {
    fn new(id: GamepadId) -> Self {
        Self {
            id,
            name: String::new(),
            connected: true,
            held: HashSet::new(),
            pressed: HashSet::new(),
            released: HashSet::new(),
            axes: HashMap::new(),
        }
    }

    pub fn id(&self) -> GamepadId {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn is_connected(&self) -> bool {
        self.connected
    }
    pub fn button_held(&self, b: Button) -> bool {
        self.held.contains(&b)
    }
    pub fn button_pressed(&self, b: Button) -> bool {
        self.pressed.contains(&b)
    }
    pub fn button_released(&self, b: Button) -> bool {
        self.released.contains(&b)
    }
    /// Axis value in `[-1, 1]` (triggers in `[0, 1]`); `0.0` if never reported.
    pub fn axis(&self, axis: Axis) -> f32 {
        self.axes.get(&axis).copied().unwrap_or(0.0)
    }
}

#[derive(Default)]
pub struct Input {
    keys_held: HashSet<Key>,
    keys_pressed: HashSet<Key>,
    keys_released: HashSet<Key>,
    mouse_held: HashSet<MouseButton>,
    mouse_pressed: HashSet<MouseButton>,
    mouse_released: HashSet<MouseButton>,
    mouse_pos: (f32, f32),
    mouse_delta: (f32, f32),
    scroll: (f32, f32),
    /// `None` when no gamepad backend could initialize (the toolkit keeps running).
    gilrs: Option<Gilrs>,
    gamepads: HashMap<GamepadId, Gamepad>,
    /// Active rumble effects kept alive for their duration (bounded ring).
    rumble: Vec<gilrs::ff::Effect>,
}

impl Input {
    pub fn new() -> Self {
        let mut me = Self::default();
        match Gilrs::new() {
            Ok(g) => {
                // Seed pads already connected at startup; gilrs only emits `Connected` for
                // hot-plugs, so without this a present controller is unknown until it moves.
                for (id, gp) in g.gamepads() {
                    let mut pad = Gamepad::new(id);
                    pad.name = gp.name().to_string();
                    me.gamepads.insert(id, pad);
                }
                me.gilrs = Some(g);
            }
            Err(e) => log::warn!("gamepad backend unavailable, continuing without it: {e}"),
        }
        me
    }

    /// Iterator over currently connected gamepads.
    pub fn gamepads(&self) -> impl Iterator<Item = &Gamepad> {
        self.gamepads.values().filter(|g| g.connected)
    }

    /// The first connected gamepad, if any. Convenient for single-player input.
    pub fn first_gamepad(&self) -> Option<&Gamepad> {
        self.gamepads.values().find(|g| g.connected)
    }

    /// Look up a connected gamepad by id.
    pub fn gamepad(&self, id: GamepadId) -> Option<&Gamepad> {
        self.gamepads.get(&id).filter(|g| g.connected)
    }

    /// Drain pending gamepad events into per-pad state, tracking hot-plug. Call once per
    /// frame before `update`.
    pub fn poll_gamepads(&mut self) {
        let Some(gilrs) = self.gilrs.as_mut() else {
            return;
        };
        while let Some(Event { id, event, .. }) = gilrs.next_event() {
            use gilrs::EventType::*;
            let pad = self.gamepads.entry(id).or_insert_with(|| Gamepad::new(id));
            match event {
                Connected => {
                    pad.connected = true;
                    pad.name = gilrs.gamepad(id).name().to_string();
                }
                Disconnected | Dropped => {
                    pad.connected = false;
                    pad.held.clear();
                    pad.axes.clear();
                }
                ButtonPressed(b, _) => {
                    pad.held.insert(b);
                    pad.pressed.insert(b);
                }
                ButtonReleased(b, _) => {
                    pad.held.remove(&b);
                    pad.released.insert(b);
                }
                AxisChanged(axis, value, _) => {
                    pad.axes.insert(axis, value);
                }
                _ => {}
            }
        }
    }

    /// Rumble `id` at `magnitude` (`0..=1`) for `duration_ms`. No-op when the pad has no
    /// force feedback or no backend is available.
    pub fn set_rumble(&mut self, id: GamepadId, magnitude: f32, duration_ms: u32) {
        let Some(gilrs) = self.gilrs.as_mut() else {
            return;
        };
        if !gilrs
            .connected_gamepad(id)
            .is_some_and(|g| g.is_ff_supported())
        {
            return;
        }
        let mag = (magnitude.clamp(0.0, 1.0) * f32::from(u16::MAX)) as u16;
        let effect = EffectBuilder::new()
            .add_effect(BaseEffect {
                kind: BaseEffectType::Strong { magnitude: mag },
                scheduling: Replay {
                    play_for: Ticks::from_ms(duration_ms),
                    ..Default::default()
                },
                envelope: Default::default(),
            })
            .gamepads(&[id])
            .finish(gilrs);
        if let Ok(effect) = effect {
            let _ = effect.play();
            // Keep the handle alive so the effect is not dropped (and stopped) immediately;
            // bound the buffer so finished effects are eventually released.
            self.rumble.push(effect);
            if self.rumble.len() > 16 {
                self.rumble.remove(0);
            }
        }
    }

    pub fn key_held(&self, k: Key) -> bool {
        self.keys_held.contains(&k)
    }
    pub fn key_pressed(&self, k: Key) -> bool {
        self.keys_pressed.contains(&k)
    }
    pub fn key_released(&self, k: Key) -> bool {
        self.keys_released.contains(&k)
    }

    pub fn mouse_held(&self, b: MouseButton) -> bool {
        self.mouse_held.contains(&b)
    }
    pub fn mouse_pressed(&self, b: MouseButton) -> bool {
        self.mouse_pressed.contains(&b)
    }
    pub fn mouse_released(&self, b: MouseButton) -> bool {
        self.mouse_released.contains(&b)
    }

    pub fn mouse_pos(&self) -> (f32, f32) {
        self.mouse_pos
    }
    pub fn mouse_delta(&self) -> (f32, f32) {
        self.mouse_delta
    }
    pub fn scroll(&self) -> (f32, f32) {
        self.scroll
    }

    pub fn handle_window_event(&mut self, event: &winit::event::WindowEvent) {
        use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                let winit::keyboard::PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                match event.state {
                    ElementState::Pressed => {
                        if self.keys_held.insert(code) {
                            self.keys_pressed.insert(code);
                        }
                    }
                    ElementState::Released => {
                        if self.keys_held.remove(&code) {
                            self.keys_released.insert(code);
                        }
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let b = MouseButton::from(*button);
                match state {
                    ElementState::Pressed => {
                        if self.mouse_held.insert(b) {
                            self.mouse_pressed.insert(b);
                        }
                    }
                    ElementState::Released => {
                        if self.mouse_held.remove(&b) {
                            self.mouse_released.insert(b);
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let new = (position.x as f32, position.y as f32);
                self.mouse_delta.0 += new.0 - self.mouse_pos.0;
                self.mouse_delta.1 += new.1 - self.mouse_pos.1;
                self.mouse_pos = new;
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(x, y) => {
                    self.scroll.0 += x;
                    self.scroll.1 += y;
                }
                MouseScrollDelta::PixelDelta(p) => {
                    self.scroll.0 += p.x as f32;
                    self.scroll.1 += p.y as f32;
                }
            },
            WindowEvent::Focused(false) => {
                self.keys_held.clear();
                self.mouse_held.clear();
            }
            _ => {}
        }
    }

    /// Call once per frame after `update` to clear edge / delta state.
    pub fn end_frame(&mut self) {
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.mouse_pressed.clear();
        self.mouse_released.clear();
        self.mouse_delta = (0.0, 0.0);
        self.scroll = (0.0, 0.0);
        for pad in self.gamepads.values_mut() {
            pad.pressed.clear();
            pad.released.clear();
        }
    }
}
