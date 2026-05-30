# game-toolkit-ecs

[![Crates.io](https://img.shields.io/crates/v/game-toolkit-ecs.svg)](https://crates.io/crates/game-toolkit-ecs)
[![docs.rs](https://img.shields.io/docsrs/game-toolkit-ecs)](https://docs.rs/game-toolkit-ecs)
[![license](https://img.shields.io/badge/license-EUPL--1.2-blue.svg)](https://github.com/sunsided/game-toolkit/blob/main/LICENSE)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)

Small glue for using the [sillyecs](https://github.com/sunsided/sillyecs) compile-time
archetype ECS with [**game-toolkit**](https://github.com/sunsided/game-toolkit): a `Vec2` to
alias as component data and a `ChannelQueue` command queue.

sillyecs is a build-time code generator: a game describes its components/archetypes/phases/
systems in an `ecs.yaml`, runs sillyecs from its own `build.rs`, and `include!`s the
generated code. Because those types are generated per game, this crate provides only the
small reusable pieces; see the workspace's `11_ecs` example for the full pattern.

Part of game-toolkit, a Rust workspace of small crates for 2D-first game jams. See the
[workspace README](https://github.com/sunsided/game-toolkit#readme) for the full toolkit,
examples, and quick-start.

## Documentation

API docs: <https://docs.rs/game-toolkit-ecs>
