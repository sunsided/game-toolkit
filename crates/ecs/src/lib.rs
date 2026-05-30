//! Small glue for using [`sillyecs`](https://github.com/sunsided/sillyecs) - a compile-time
//! generated archetype ECS - with game-toolkit.
//!
//! sillyecs is a **build-time code generator**: a game describes its components, archetypes,
//! phases and systems in an `ecs.yaml`, runs sillyecs from its own `build.rs`, and
//! `include!`s the generated `*_gen.rs` files. Because the `World`/component/system types are
//! generated per game, this crate cannot wrap them generically - it only provides the small,
//! reusable pieces that every sillyecs game needs anyway:
//!
//! - [`Vec2`] - a plain 2D vector to alias as component data (`type PositionData = Vec2;`).
//! - [`ChannelQueue`] - an mpsc-backed command queue; implement the generated
//!   `WorldCommandSender` / `WorldCommandReceiver` traits on `ChannelQueue<WorldCommand>`.
//!
//! Drive the generated world from the toolkit loop by calling `world.apply_system_phases()`
//! in `Game::update` (it runs the fixed-update phases from real-time deltas) and reading
//! components back in `Game::render`. See `examples/11_ecs` for the full pattern.
//!
//! Add the generator to the game's `Cargo.toml`:
//!
//! ```toml
//! [build-dependencies]
//! sillyecs = "0.0"
//! ```

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
/// sillyecs generates `WorldCommandSender` / `WorldCommandReceiver` traits (with a generated
/// `WorldCommand` type), so implement them on `ChannelQueue<WorldCommand>` in the game:
///
/// ```ignore
/// impl WorldCommandSender for ChannelQueue<WorldCommand> {
///     type Error = std::sync::mpsc::SendError<WorldCommand>;
///     fn send(&self, c: WorldCommand) -> Result<(), Self::Error> { self.send(c) }
/// }
/// impl WorldCommandReceiver for ChannelQueue<WorldCommand> {
///     type Error = std::sync::mpsc::TryRecvError;
///     fn recv(&self) -> Result<Option<WorldCommand>, Self::Error> { self.try_recv() }
/// }
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

#[cfg(test)]
mod tests {
    use super::*;

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
