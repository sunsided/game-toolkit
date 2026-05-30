# {{project-name}}

A 3D game built on [game-toolkit](https://github.com/sunsided/game-toolkit): a depth-tested
spinning cube under a perspective camera, with a 2D HUD on top.

## Run

```sh
cargo run
```

A window opens, spins a cube, and quits on `Esc`.

## Layout

```
Cargo.toml      depends on game-toolkit (with the `ui` egui-overlay feature)
src/main.rs     your Game: uploads a mesh, drives camera3d, draws meshes + 2D HUD
assets/         art, audio, fonts; AppConfig::asset_root points here
Taskfile.yaml   task run / build / test / lint / fmt (needs go-task)
```

Meshes (`MeshVertex` positions + normals) are uploaded once with `ctx.gfx.create_mesh` and
drawn with `painter.mesh(id, model, color)`, depth-tested against `ctx.gfx.camera3d`. The 2D
`Painter` (text, sprites, shapes) composites on top. 3D needs a depth buffer, set via
`AppConfig::depth_format`.
