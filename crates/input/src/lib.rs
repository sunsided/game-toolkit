//! Input subsystem: keyboard + mouse + gamepads + touch, with held / just-pressed /
//! just-released semantics.
//!
//! Call [`Input::handle_window_event`] for every `winit::event::WindowEvent` and
//! [`Input::poll_gamepads`] once per frame before `update`, then [`Input::end_frame`] once
//! per frame after `update` to clear edge state.

#![forbid(unsafe_code)]

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

/// One finger on a touchscreen. Positions are in physical pixels, the same space as
/// [`Input::mouse_pos`]. Obtain via [`Input::touches`], [`Input::touches_started`] or
/// [`Input::touches_ended`].
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Touch {
    id: u64,
    pos: (f32, f32),
    start_pos: (f32, f32),
    delta: (f32, f32),
}

impl Touch {
    /// Stable for as long as the finger stays down; the platform may reuse it afterwards.
    pub fn id(&self) -> u64 {
        self.id
    }
    /// Where the finger is (for an ended touch: where it lifted).
    pub fn pos(&self) -> (f32, f32) {
        self.pos
    }
    /// Where the finger went down. Compare with [`Touch::pos`] to tell a tap from a swipe.
    pub fn start_pos(&self) -> (f32, f32) {
        self.start_pos
    }
    /// How far the finger moved this frame.
    pub fn delta(&self) -> (f32, f32) {
        self.delta
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
    /// Any button on this controller went down this frame.
    pub fn any_button_pressed(&self) -> bool {
        !self.pressed.is_empty()
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
    touches: HashMap<u64, Touch>,
    touches_started: Vec<Touch>,
    touches_ended: Vec<Touch>,
    /// See [`Input::set_touch_emulates_mouse`].
    touch_emulates_mouse: bool,
    /// The finger currently acting as the mouse, while emulation is on.
    mouse_finger: Option<u64>,
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

    /// Any key other than a modifier (Shift, Ctrl, Alt, Super, ...) went down this frame: the
    /// classic "press any key". Modifiers alone don't count, so Alt-Tabbing away or holding
    /// Shift doesn't skip a screen; [`Input::keys_pressed`] still reports them.
    pub fn any_key_pressed(&self) -> bool {
        self.keys_pressed.iter().any(|k| !is_modifier(*k))
    }

    /// Every key that went down this frame, modifiers included, in no particular order (for
    /// "press a key to bind it", cheat codes, typing a name).
    pub fn keys_pressed(&self) -> impl Iterator<Item = Key> + '_ {
        self.keys_pressed.iter().copied()
    }

    /// Any mouse button went down this frame.
    pub fn any_mouse_pressed(&self) -> bool {
        !self.mouse_pressed.is_empty()
    }

    /// Anything at all went down this frame: a non-modifier key ([`Input::any_key_pressed`]),
    /// a mouse button, a button on any connected gamepad, or a finger touching the screen
    /// ([`Input::any_touch_pressed`]). For "press any key to continue", skipping intros, and
    /// waking an idle game.
    ///
    /// ```
    /// use game_toolkit_input::Input;
    ///
    /// enum Screen { Title, Playing }
    ///
    /// fn update(input: &Input, screen: &mut Screen) {
    ///     if matches!(screen, Screen::Title) && input.any_pressed() {
    ///         *screen = Screen::Playing;
    ///     }
    /// }
    ///
    /// let mut screen = Screen::Title;
    /// update(&Input::default(), &mut screen); // nothing pressed yet
    /// assert!(matches!(screen, Screen::Title));
    /// ```
    pub fn any_pressed(&self) -> bool {
        self.any_key_pressed()
            || self.any_mouse_pressed()
            || self.gamepads().any(Gamepad::any_button_pressed)
            || self.any_touch_pressed()
    }

    /// Fingers currently on the screen, in no particular order.
    pub fn touches(&self) -> impl Iterator<Item = &Touch> {
        self.touches.values()
    }

    /// The finger with this [`Touch::id`], if it is still down.
    pub fn touch(&self, id: u64) -> Option<&Touch> {
        self.touches.get(&id)
    }

    /// Fingers that went down this frame, in event order. A tap that starts and ends within
    /// one frame shows up here and in [`Input::touches_ended`], but not in [`Input::touches`].
    pub fn touches_started(&self) -> &[Touch] {
        &self.touches_started
    }

    /// Fingers that lifted this frame, in event order. Touches the platform cancels (e.g. the
    /// browser took over the gesture, or the window lost focus) vanish from
    /// [`Input::touches`] without appearing here, so a cancelled touch never counts as a tap.
    pub fn touches_ended(&self) -> &[Touch] {
        &self.touches_ended
    }

    /// Any finger went down this frame.
    pub fn any_touch_pressed(&self) -> bool {
        !self.touches_started.is_empty()
    }

    /// Let the first finger on a touchscreen act as the mouse, so a game written for the
    /// mouse also works on a phone. Off by default (also settable through
    /// `AppConfig::touch_emulates_mouse`).
    ///
    /// While on, the first finger down moves [`Input::mouse_pos`] to where it touched (without
    /// a [`Input::mouse_delta`] jump), presses and holds [`MouseButton::Left`], drags the
    /// cursor along, and releases the button where it lifts. Other fingers are ignored until
    /// it lifts. A cancelled touch lets go of the button without a [`Input::mouse_released`]
    /// edge, so it never counts as a click. [`Input::touches`] and friends keep working.
    ///
    /// No effect on Windows: the OS already turns touches into mouse events there, which
    /// winit passes on, so emulating them again would press twice.
    pub fn set_touch_emulates_mouse(&mut self, on: bool) {
        self.touch_emulates_mouse = on;
        if !on && self.mouse_finger.take().is_some() {
            self.mouse_held.remove(&MouseButton::Left);
        }
    }

    /// Whether [`Input::set_touch_emulates_mouse`] is on.
    pub fn touch_emulates_mouse(&self) -> bool {
        self.touch_emulates_mouse
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
            WindowEvent::Touch(t) => {
                let pos = (t.location.x as f32, t.location.y as f32);
                self.handle_touch(t.id, t.phase, pos);
            }
            WindowEvent::Focused(false) => {
                self.keys_held.clear();
                self.mouse_held.clear();
                self.touches.clear();
                self.mouse_finger = None;
            }
            _ => {}
        }
    }

    fn handle_touch(&mut self, id: u64, phase: winit::event::TouchPhase, pos: (f32, f32)) {
        use winit::event::TouchPhase;
        if self.touch_emulates_mouse && !cfg!(target_os = "windows") {
            self.emulate_mouse(id, phase, pos);
        }
        match phase {
            TouchPhase::Started => {
                let touch = Touch {
                    id,
                    pos,
                    start_pos: pos,
                    delta: (0.0, 0.0),
                };
                self.touches.insert(id, touch);
                self.touches_started.push(touch);
            }
            TouchPhase::Moved => {
                if let Some(touch) = self.touches.get_mut(&id) {
                    touch.delta.0 += pos.0 - touch.pos.0;
                    touch.delta.1 += pos.1 - touch.pos.1;
                    touch.pos = pos;
                }
            }
            TouchPhase::Ended => {
                if let Some(mut touch) = self.touches.remove(&id) {
                    touch.delta.0 += pos.0 - touch.pos.0;
                    touch.delta.1 += pos.1 - touch.pos.1;
                    touch.pos = pos;
                    self.touches_ended.push(touch);
                }
            }
            TouchPhase::Cancelled => {
                self.touches.remove(&id);
            }
        }
    }

    /// Drive the mouse from the first finger; see [`Input::set_touch_emulates_mouse`].
    fn emulate_mouse(&mut self, id: u64, phase: winit::event::TouchPhase, pos: (f32, f32)) {
        use winit::event::TouchPhase;
        match phase {
            TouchPhase::Started if self.mouse_finger.is_none() => {
                self.mouse_finger = Some(id);
                // Jump without a delta: a finger landing elsewhere isn't a mouse movement.
                self.mouse_pos = pos;
                if self.mouse_held.insert(MouseButton::Left) {
                    self.mouse_pressed.insert(MouseButton::Left);
                }
            }
            _ if self.mouse_finger != Some(id) => {}
            TouchPhase::Started => {}
            TouchPhase::Moved | TouchPhase::Ended => {
                self.mouse_delta.0 += pos.0 - self.mouse_pos.0;
                self.mouse_delta.1 += pos.1 - self.mouse_pos.1;
                self.mouse_pos = pos;
                if phase == TouchPhase::Ended {
                    self.mouse_finger = None;
                    if self.mouse_held.remove(&MouseButton::Left) {
                        self.mouse_released.insert(MouseButton::Left);
                    }
                }
            }
            TouchPhase::Cancelled => {
                self.mouse_finger = None;
                self.mouse_held.remove(&MouseButton::Left);
            }
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
        self.touches_started.clear();
        self.touches_ended.clear();
        for touch in self.touches.values_mut() {
            touch.delta = (0.0, 0.0);
        }
        for pad in self.gamepads.values_mut() {
            pad.pressed.clear();
            pad.released.clear();
        }
    }
}

/// Keys that only modify other keys, ignored by [`Input::any_key_pressed`].
fn is_modifier(k: Key) -> bool {
    matches!(
        k,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::SuperLeft
            | Key::SuperRight
            | Key::Meta
            | Key::Hyper
            | Key::Fn
            | Key::FnLock
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // winit's `KeyEvent` can't be built outside winit, so these drive the edge sets directly,
    // the way `handle_window_event` and `poll_gamepads` fill them.

    #[test]
    fn nothing_pressed_by_default() {
        let input = Input::default();
        assert!(!input.any_key_pressed());
        assert!(!input.any_mouse_pressed());
        assert!(!input.any_pressed());
        assert_eq!(input.keys_pressed().count(), 0);
    }

    #[test]
    fn a_key_counts_but_a_lone_modifier_does_not() {
        let mut input = Input::default();
        input.keys_pressed.insert(Key::ShiftLeft);
        assert!(!input.any_key_pressed());
        assert!(!input.any_pressed());
        assert_eq!(input.keys_pressed().collect::<Vec<_>>(), [Key::ShiftLeft]);

        input.keys_pressed.insert(Key::KeyX);
        assert!(input.any_key_pressed());
        assert!(input.any_pressed());
        assert_eq!(input.keys_pressed().count(), 2);
    }

    #[test]
    fn mouse_buttons_count() {
        let mut input = Input::default();
        input.mouse_pressed.insert(MouseButton::Right);
        assert!(input.any_mouse_pressed());
        assert!(input.any_pressed());
        assert!(!input.any_key_pressed());
    }

    #[test]
    fn end_frame_clears_the_edges() {
        let mut input = Input::default();
        input.keys_pressed.insert(Key::Space);
        input.mouse_pressed.insert(MouseButton::Left);
        input.end_frame();
        assert!(!input.any_pressed());
    }

    fn touch(input: &mut Input, id: u64, phase: winit::event::TouchPhase, x: f64, y: f64) {
        input.handle_window_event(&winit::event::WindowEvent::Touch(winit::event::Touch {
            device_id: winit::event::DeviceId::dummy(),
            phase,
            location: winit::dpi::PhysicalPosition::new(x, y),
            force: None,
            id,
        }));
    }

    #[test]
    fn a_touch_goes_down_moves_and_lifts() {
        use winit::event::TouchPhase::*;
        let mut input = Input::default();

        touch(&mut input, 7, Started, 10.0, 20.0);
        assert!(input.any_touch_pressed());
        assert!(input.any_pressed());
        assert_eq!(input.touches_started().len(), 1);
        assert_eq!(input.touch(7).unwrap().pos(), (10.0, 20.0));
        input.end_frame();
        assert!(!input.any_pressed());
        assert_eq!(input.touches().count(), 1);

        touch(&mut input, 7, Moved, 15.0, 20.0);
        touch(&mut input, 7, Moved, 18.0, 24.0);
        let t = *input.touch(7).unwrap();
        assert_eq!(t.pos(), (18.0, 24.0));
        assert_eq!(t.start_pos(), (10.0, 20.0));
        assert_eq!(t.delta(), (8.0, 4.0));
        input.end_frame();
        assert_eq!(input.touch(7).unwrap().delta(), (0.0, 0.0));

        touch(&mut input, 7, Ended, 20.0, 24.0);
        assert_eq!(input.touches().count(), 0);
        let ended = input.touches_ended();
        assert_eq!(ended.len(), 1);
        assert_eq!(ended[0].pos(), (20.0, 24.0));
        assert_eq!(ended[0].start_pos(), (10.0, 20.0));
        input.end_frame();
        assert!(input.touches_ended().is_empty());
    }

    #[test]
    fn a_tap_within_one_frame_is_started_and_ended() {
        use winit::event::TouchPhase::*;
        let mut input = Input::default();
        touch(&mut input, 1, Started, 5.0, 5.0);
        touch(&mut input, 1, Ended, 5.0, 5.0);
        assert!(input.any_touch_pressed());
        assert_eq!(input.touches_started().len(), 1);
        assert_eq!(input.touches_ended().len(), 1);
        assert_eq!(input.touches().count(), 0);
    }

    #[test]
    fn fingers_are_tracked_separately() {
        use winit::event::TouchPhase::*;
        let mut input = Input::default();
        touch(&mut input, 1, Started, 0.0, 0.0);
        touch(&mut input, 2, Started, 100.0, 0.0);
        touch(&mut input, 2, Moved, 110.0, 0.0);
        assert_eq!(input.touches().count(), 2);
        assert_eq!(input.touch(1).unwrap().pos(), (0.0, 0.0));
        assert_eq!(input.touch(2).unwrap().pos(), (110.0, 0.0));
    }

    #[test]
    fn a_cancelled_touch_vanishes_without_ending() {
        use winit::event::TouchPhase::*;
        let mut input = Input::default();
        touch(&mut input, 3, Started, 0.0, 0.0);
        input.end_frame();
        touch(&mut input, 3, Cancelled, 0.0, 0.0);
        assert_eq!(input.touches().count(), 0);
        assert!(input.touches_ended().is_empty());
    }

    #[test]
    fn losing_focus_drops_touches() {
        let mut input = Input::default();
        touch(&mut input, 4, winit::event::TouchPhase::Started, 0.0, 0.0);
        input.handle_window_event(&winit::event::WindowEvent::Focused(false));
        assert_eq!(input.touches().count(), 0);
    }

    #[test]
    fn touches_leave_the_mouse_alone_by_default() {
        use winit::event::TouchPhase::*;
        let mut input = Input::default();
        touch(&mut input, 1, Started, 50.0, 60.0);
        assert_eq!(input.mouse_pos(), (0.0, 0.0));
        assert!(!input.mouse_held(MouseButton::Left));
        assert!(!input.any_mouse_pressed());
    }

    // Windows promotes touches to mouse events itself, so emulation is off there.
    #[test]
    #[cfg_attr(target_os = "windows", ignore)]
    fn the_first_finger_drives_the_mouse() {
        use winit::event::TouchPhase::*;
        let mut input = Input::default();
        input.set_touch_emulates_mouse(true);

        touch(&mut input, 1, Started, 50.0, 60.0);
        assert!(input.mouse_pressed(MouseButton::Left));
        assert!(input.mouse_held(MouseButton::Left));
        assert_eq!(input.mouse_pos(), (50.0, 60.0));
        assert_eq!(input.mouse_delta(), (0.0, 0.0), "landing is not a movement");
        input.end_frame();

        // A second finger doesn't take over.
        touch(&mut input, 2, Started, 300.0, 300.0);
        touch(&mut input, 2, Moved, 310.0, 300.0);
        assert!(!input.mouse_pressed(MouseButton::Left));
        assert_eq!(input.mouse_pos(), (50.0, 60.0));

        touch(&mut input, 1, Moved, 55.0, 70.0);
        assert_eq!(input.mouse_pos(), (55.0, 70.0));
        assert_eq!(input.mouse_delta(), (5.0, 10.0));
        input.end_frame();

        touch(&mut input, 1, Ended, 57.0, 70.0);
        assert!(input.mouse_released(MouseButton::Left));
        assert!(!input.mouse_held(MouseButton::Left));
        assert_eq!(input.mouse_pos(), (57.0, 70.0));
        input.end_frame();

        // Finger 2 is still down, but only a new finger becomes the mouse.
        touch(&mut input, 2, Moved, 320.0, 300.0);
        assert_eq!(input.mouse_pos(), (57.0, 70.0));
        touch(&mut input, 3, Started, 100.0, 100.0);
        assert!(input.mouse_pressed(MouseButton::Left));
        assert_eq!(input.mouse_pos(), (100.0, 100.0));
    }

    #[test]
    #[cfg_attr(target_os = "windows", ignore)]
    fn a_cancelled_mouse_finger_is_not_a_click() {
        use winit::event::TouchPhase::*;
        let mut input = Input::default();
        input.set_touch_emulates_mouse(true);
        touch(&mut input, 1, Started, 0.0, 0.0);
        input.end_frame();
        touch(&mut input, 1, Cancelled, 0.0, 0.0);
        assert!(!input.mouse_held(MouseButton::Left));
        assert!(!input.mouse_released(MouseButton::Left));
        // The next finger can take over.
        touch(&mut input, 2, Started, 5.0, 5.0);
        assert!(input.mouse_pressed(MouseButton::Left));
    }

    #[test]
    #[cfg_attr(target_os = "windows", ignore)]
    fn turning_emulation_off_lets_go_of_the_button() {
        let mut input = Input::default();
        input.set_touch_emulates_mouse(true);
        touch(&mut input, 1, winit::event::TouchPhase::Started, 0.0, 0.0);
        input.set_touch_emulates_mouse(false);
        assert!(!input.mouse_held(MouseButton::Left));
        touch(&mut input, 1, winit::event::TouchPhase::Ended, 0.0, 0.0);
        assert!(!input.mouse_released(MouseButton::Left));
    }

    #[test]
    fn moves_and_ends_for_unknown_fingers_are_ignored() {
        use winit::event::TouchPhase::*;
        let mut input = Input::default();
        touch(&mut input, 9, Moved, 1.0, 1.0);
        touch(&mut input, 9, Ended, 1.0, 1.0);
        assert_eq!(input.touches().count(), 0);
        assert!(input.touches_ended().is_empty());
    }
}
