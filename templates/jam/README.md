# {{project-name}}

A 2D game built on [game-toolkit](https://github.com/sunsided/game-toolkit).

## Run

```sh
cargo run
```

A window opens with a ball bouncing around (spinning with its speed), and quits on `Esc`.

## Layout

```
Cargo.toml      depends on toolkit-prelude (with the `ui` egui-overlay feature)
src/main.rs     your Game: init / update / render
assets/         art, audio, fonts; AppConfig::asset_root points here
Taskfile.yaml   task run / build / test / lint / fmt (needs go-task)
```

Write your game by filling in `Game::update` (input + simulation) and `Game::render`
(draw with the `Painter`: `sprite`, `rect`, `circle`, `line`, `text`, ...). See the
toolkit's examples for sprites, audio, tilemaps, Aseprite animations, 3D, and gamepads.
