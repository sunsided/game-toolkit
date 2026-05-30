# game-toolkit-audio

[![Crates.io](https://img.shields.io/crates/v/game-toolkit-audio.svg)](https://crates.io/crates/game-toolkit-audio)
[![docs.rs](https://img.shields.io/docsrs/game-toolkit-audio)](https://docs.rs/game-toolkit-audio)
[![license](https://img.shields.io/badge/license-EUPL--1.2-blue.svg)](https://github.com/sunsided/game-toolkit/blob/main/LICENSE)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)

Audio for [**game-toolkit**](https://github.com/sunsided/game-toolkit): sound loading and
playback on [kira](https://crates.io/crates/kira) (degrades gracefully to muted when no
device is available), plus optional chiptune synthesis via
[synthie](https://crates.io/crates/synthie) behind the `synth` feature.

Part of game-toolkit, a Rust workspace of small crates for 2D-first game jams. Most users
depend on [`game-toolkit`](https://crates.io/crates/game-toolkit) rather than this crate
directly. See the [workspace README](https://github.com/sunsided/game-toolkit#readme) for the
full toolkit, examples, and quick-start.

## Documentation

API docs: <https://docs.rs/game-toolkit-audio>
