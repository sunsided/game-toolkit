//! Small glue for using [`sillyecs`](https://github.com/sunsided/sillyecs) - a compile-time
//! generated archetype ECS - with game-toolkit.
//!
//! sillyecs is a **build-time code generator**: a game describes its components, archetypes,
//! phases and systems in an `ecs.yaml`, runs sillyecs from its own `build.rs`, and includes
//! the generated code with `sillyecs::include_ecs!()`. Because the `World`/component/system types are
//! generated per game, this crate cannot wrap them generically - it only provides the small,
//! reusable pieces that every sillyecs game needs anyway:
//!
//! - [`Vec2`] - a plain 2D vector to alias as component data (`type PositionData = Vec2;`).
//! - [`ChannelQueue`] - an mpsc-backed command queue. It implements sillyecs'
//!   [`CommandSender`](sillyecs::CommandSender) / [`CommandReceiver`](sillyecs::CommandReceiver)
//!   for any command type, so `ChannelQueue<WorldCommand<U>>` is a world command queue as is.
//!
//! Drive the generated world from the toolkit loop by calling `world.apply_system_phases()`
//! in `Game::update` and reading components back in `Game::render`. See `examples/11_ecs` for
//! the full pattern.
//!
//! Add the generator and its runtime support crate to the game's `Cargo.toml`. On the web,
//! sillyecs needs its `wasm` feature: it times phases with `Instant`, and
//! `std::time::Instant` panics on `wasm32-unknown-unknown`.
//!
//! ```toml
//! [dependencies]
//! sillyecs = "0.2"
//!
//! [target.'cfg(target_arch = "wasm32")'.dependencies]
//! sillyecs = { version = "0.2", features = ["wasm"] }
//!
//! [build-dependencies]
//! sillyecs-build = "0.2"
//! ```
//!
//! # Timing
//!
//! sillyecs steps its fixed-update phases itself: each `apply_system_phases` call measures
//! the real time since the last one and runs each fixed phase as often as that allows. So:
//!
//! - **Leave `AppConfig::fixed_timestep` at `None`.** With both, the ECS's fixed phases run
//!   inside the toolkit's fixed steps and the two accumulators fight. `update` is then called
//!   once per frame, and so is `apply_system_phases`.
//! - **`render` runs once per frame** and sillyecs gives no interpolation factor, so motion
//!   is only as smooth as the fixed phase's rate. Pick a rate at or above the display's.
//! - **Reset the world's clock after a gap.** sillyecs reads the clock itself, so the toolkit's
//!   `AppConfig::max_frame_time` cap doesn't reach it: after the window was in the
//!   background, a tab was hidden or a breakpoint hit, the next `apply_system_phases` runs
//!   every fixed step it missed. Call the generated `world.reset_frame_context()` when focus
//!   comes back and after a long frame:
//!
//! ```ignore
//! fn update(&mut self, ctx: &mut Context, _dt: f32) {
//!     if ctx.time.delta > std::time::Duration::from_millis(250) {
//!         self.world.reset_frame_context(); // skip the gap instead of fast-forwarding
//!     }
//!     self.world.apply_system_phases();
//! }
//!
//! fn event(&mut self, _ctx: &mut Context, event: &GameEvent) {
//!     if let GameEvent::FocusChanged(true) = event {
//!         self.world.reset_frame_context();
//!     }
//! }
//! ```

#![forbid(unsafe_code)]

use std::sync::mpsc::{Receiver, SendError, Sender, TryRecvError, channel};

/// A plain 2D vector, convenient as ECS component data (position, velocity, ...).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl From<[f32; 2]> for Vec2 {
    fn from([x, y]: [f32; 2]) -> Self {
        Self { x, y }
    }
}

impl From<Vec2> for [f32; 2] {
    fn from(v: Vec2) -> Self {
        [v.x, v.y]
    }
}

/// An mpsc-backed, single-threaded command queue for a sillyecs world.
///
/// It implements [`CommandSender`](sillyecs::CommandSender) and
/// [`CommandReceiver`](sillyecs::CommandReceiver) for any command type, and sillyecs implements
/// its generated `WorldCommandSender` / `WorldCommandReceiver` for those, so a world takes it
/// as is:
///
/// ```ignore
/// type World = MainWorld<NoOpPhaseEvents, ChannelQueue<WorldCommand<MyUserCommand>>>;
/// let world = World::new(&factory, ChannelQueue::new());
/// ```
pub struct ChannelQueue<C> {
    sender: Sender<C>,
    receiver: Receiver<C>,
}

impl<C> ChannelQueue<C> {
    pub fn new() -> Self {
        let (sender, receiver) = channel();
        Self { sender, receiver }
    }

    /// Enqueue a command.
    pub fn send(&self, command: C) -> Result<(), SendError<C>> {
        self.sender.send(command)
    }

    /// Dequeue one command, or `None` if the queue is empty. Maps `Disconnected` to an error.
    pub fn try_recv(&self) -> Result<Option<C>, TryRecvError> {
        match self.receiver.try_recv() {
            Ok(c) => Ok(Some(c)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

impl<C> Default for ChannelQueue<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> sillyecs::CommandSender for ChannelQueue<C> {
    type Command = C;
    type Error = SendError<C>;

    fn send(&self, command: C) -> Result<(), Self::Error> {
        Self::send(self, command)
    }
}

impl<C> sillyecs::CommandReceiver for ChannelQueue<C> {
    type Command = C;
    type Error = TryRecvError;

    fn recv(&self) -> Result<Option<C>, Self::Error> {
        self.try_recv()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_queue_implements_the_sillyecs_traits() {
        fn roundtrip<Q>(q: &Q)
        where
            Q: sillyecs::CommandSender<Command = u32> + sillyecs::CommandReceiver<Command = u32>,
        {
            sillyecs::CommandSender::send(q, 3).unwrap();
            assert_eq!(sillyecs::CommandReceiver::recv(q).unwrap(), Some(3));
            assert_eq!(sillyecs::CommandReceiver::recv(q).unwrap(), None);
        }
        roundtrip(&ChannelQueue::new());
    }

    #[test]
    fn channel_queue_roundtrips() {
        let q: ChannelQueue<u32> = ChannelQueue::new();
        assert_eq!(q.try_recv().unwrap(), None);
        q.send(7).unwrap();
        q.send(9).unwrap();
        assert_eq!(q.try_recv().unwrap(), Some(7));
        assert_eq!(q.try_recv().unwrap(), Some(9));
        assert_eq!(q.try_recv().unwrap(), None);
    }

    #[test]
    fn vec2_conversions() {
        let v = Vec2::from([1.0, 2.0]);
        assert_eq!(v, Vec2::new(1.0, 2.0));
        assert_eq!(<[f32; 2]>::from(v), [1.0, 2.0]);
    }
}
