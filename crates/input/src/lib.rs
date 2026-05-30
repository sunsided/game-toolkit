//! Input subsystem: keyboard + mouse with held / just-pressed / just-released semantics.
//!
//! Call [`Input::handle_window_event`] for every `winit::event::WindowEvent`, then
//! [`Input::end_frame`] once per frame after game `update` to clear edge state.

use std::collections::HashSet;

pub use winit::keyboard::KeyCode as Key;

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
}

impl Input {
    pub fn new() -> Self {
        Self::default()
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
    }
}
