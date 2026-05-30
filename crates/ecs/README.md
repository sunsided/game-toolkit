# toolkit-ecs

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

API docs: <https://docs.rs/toolkit-ecs>
