# {{project-name}}

A 3D game built on [game-toolkit](https://github.com/sunsided/game-toolkit): a depth-tested
spinning cube under a perspective camera, with a 2D HUD on top.

## Run

```sh
cargo run
```

A window opens, spins a cube, and quits on `Esc`.

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
src/main.rs     your Game: uploads a mesh, drives camera3d, draws meshes + 2D HUD
assets/         art, audio, fonts; AppConfig::asset_root points here
index.html      page for the browser build (trunk); Trunk.toml configures it
Taskfile.yaml   task run / build / test / lint / fmt / web (needs go-task)
CLAUDE.md       agent cheatsheet for the game-toolkit API
```

Meshes (`MeshVertex` positions + normals) are uploaded once with `ctx.gfx.create_mesh` and
drawn with `painter.mesh(id, model, color)`, depth-tested against `ctx.gfx.camera3d`. The 2D
`Painter` (text, sprites, shapes) composites on top. 3D needs a depth buffer, set via
`AppConfig::depth_format`.
