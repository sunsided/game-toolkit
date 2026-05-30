# atlas-packer

A CLI from [**game-toolkit**](https://github.com/sunsided/game-toolkit) that packs a directory
of PNGs into a single texture atlas plus an Aseprite-compatible JSON sidecar - so the result
loads with no extra code via `game-toolkit-aseprite`'s `SpriteSheet::from_aseprite_json`.

```sh
atlas-packer --input sprites/ --output atlas.png --metadata atlas.json [--max-size 2048] [--padding 2]
```

It bin-packs with [etagere](https://crates.io/crates/etagere) (largest area first), blits each
sprite into a tightly-sized atlas, and writes the atlas PNG plus the JSON map of sub-rects.

Part of game-toolkit, a Rust workspace of small crates for 2D-first game jams. See the
[workspace README](https://github.com/sunsided/game-toolkit#readme) for the full toolkit.
