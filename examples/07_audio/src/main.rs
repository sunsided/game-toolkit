//! Plays a loaded WAV (Space) and synthesized retro effects (1/2/3) through `game-toolkit-audio`.
//!
//! `ctx.audio` is an `Option<Audio>`: when the audio backend fails to initialize the
//! toolkit keeps running muted rather than aborting. This example mirrors that contract -
//! every audio call is guarded, so it still opens a window and runs with no audio device.
//!
//! The retro effects are rendered by `Synth` (the `synthie` chiptune engine) to PCM and
//! played via `Audio::play_samples`, so no audio file is needed.

use game_toolkit::prelude::*;

struct AudioDemo {
    sound: Option<SoundId>,
    synth: Synth,
    plays: u32,
}

impl Game for AudioDemo {
    fn init(ctx: &mut Context) -> Result<Self> {
        let path = ctx.assets.resolve("blip.wav");
        // Resolve the path before borrowing `ctx.audio` so both borrows don't overlap.
        let sound = match ctx.audio.as_mut() {
            Some(audio) => match audio.load_sound(&path) {
                Ok(id) => Some(id),
                Err(e) => {
                    log::warn!("could not load {}: {e}", path.display());
                    None
                }
            },
            None => {
                log::warn!("audio backend unavailable; running muted");
                None
            }
        };
        Ok(Self {
            sound,
            synth: Synth::new(44_100),
            plays: 0,
        })
    }

    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        if ctx.input.key_pressed(Key::Space)
            && let (Some(audio), Some(id)) = (ctx.audio.as_mut(), self.sound)
        {
            match audio.play(id) {
                Ok(_) => self.plays += 1,
                Err(e) => log::warn!("play failed: {e}"),
            }
        }

        // Synthesized retro effects on the number keys.
        let pcm = if ctx.input.key_pressed(Key::Digit1) {
            Some(self.synth.blip())
        } else if ctx.input.key_pressed(Key::Digit2) {
            Some(self.synth.coin())
        } else if ctx.input.key_pressed(Key::Digit3) {
            Some(self.synth.thud())
        } else {
            None
        };
        if let Some(pcm) = pcm
            && let Some(audio) = ctx.audio.as_mut()
        {
            match audio.play_samples(&pcm, self.synth.sample_rate()) {
                Ok(_) => self.plays += 1,
                Err(e) => log::warn!("synth play failed: {e}"),
            }
        }
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.06, 0.07, 0.1, 1.0]);
        let status = if self.sound.is_some() {
            "Space: WAV   1: blip   2: coin   3: thud   (Esc to quit)"
        } else {
            "No device - running muted. 1/2/3 synth, Space WAV (Esc to quit)"
        };
        p.text([24.0, 24.0], status, 20.0, [0.9, 0.9, 0.95, 1.0]);
        p.text(
            [24.0, 56.0],
            &format!("plays: {}", self.plays),
            20.0,
            [0.6, 0.8, 1.0, 1.0],
        );
    }
}

fn main() -> Result<()> {
    env_logger::init();
    run::<AudioDemo>(AppConfig {
        title: "07_audio".into(),
        width: 680,
        height: 360,
        asset_root: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        ..Default::default()
    })
}
