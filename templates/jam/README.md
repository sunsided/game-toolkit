# {{project-name}}

A 2D game built on [game-toolkit](https://github.com/sunsided/game-toolkit).

## Run

```sh
cargo run
```

A window opens with a ball bouncing around (spinning with its speed), and quits on `Esc`.

## Run in the browser

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
task web                         # http://localhost:8080, rebuilds on save
task web:lan                     # same, but open it on a phone (prints LAN URLs)
PROFILE=release task web:build   # dist/ - zip it and upload to itch.io as an HTML game
```

Files under `assets/` are fetched before the game starts, from the list in
`assets/manifest.txt` (the `web` tasks regenerate it). Sound starts after the first click or key
press. See `CLAUDE.md` ("Shipping to the browser") for the details.

## Layout

```
Cargo.toml      depends on game-toolkit (with the `ui` egui-overlay feature)
src/main.rs     your Game: init / update / render
assets/         art, audio, fonts; AppConfig::asset_root points here
index.html      page for the browser build (trunk); Trunk.toml configures it
Taskfile.yaml   task run / build / test / lint / fmt / web (needs go-task)
CLAUDE.md       agent cheatsheet for the game-toolkit API
```

Write your game by filling in `Game::update` (input + simulation) and `Game::render`
(draw with the `Painter`: `sprite`, `rect`, `circle`, `line`, `text`, ...). See the
toolkit's examples for sprites, audio, tilemaps, Aseprite animations, 3D, and gamepads.
