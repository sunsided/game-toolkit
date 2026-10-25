//! Plays a loaded WAV (Space) and synthesized retro effects (1/2/3) through `game-toolkit-audio`,
//! plus a looping "music" track (M to pause/resume, Up/Down for its volume).
//!
//! `ctx.audio` is an `Option<Audio>`: when the audio backend fails to initialize the
//! toolkit keeps running muted rather than aborting. This example mirrors that contract -
//! every audio call is guarded, so it still opens a window and runs with no audio device.
//!
//! The retro effects are rendered by `Synth` (the `synthie` chiptune engine) to PCM and
//! played via `Audio::play_samples`, so no audio file is needed.
//!
//! Every play call returns a `SoundHandle` for that instance. The music is started in `init`
//! with `PlayOptions::looped().deferred()`: on the web, where audio only starts after the first
//! key press or click, the deferred play is queued and begins on that gesture instead of failing.

use game_toolkit::prelude::*;

struct AudioDemo {
    sound: Option<SoundId>,
    synth: Synth,
    plays: u32,
    music: Option<SoundHandle>,
    music_volume: f32,
}

impl Game for AudioDemo {
    fn init(ctx: &mut Context) -> Result<Self> {
        // `Ok(None)` means the audio backend is unavailable and the game runs muted.
        let sound = match ctx.load_sound("blip.wav") {
            Ok(Some(id)) => Some(id),
            Ok(None) => {
                log::warn!("audio backend unavailable; running muted");
                None
            }
            Err(e) => {
                log::warn!("could not load blip.wav: {e}");
                None
            }
        };
        let mut synth = Synth::new(44_100);
        let music_volume = 0.3;
        // A synthesized arpeggio stands in for a music track; `play_with` works the same for
        // a loaded `SoundId`.
        let music = ctx.audio.as_mut().and_then(|audio| {
            let pcm = arpeggio(&mut synth);
            let options = PlayOptions::looped().volume(music_volume).deferred();
            audio
                .play_samples_with(&pcm, synth.sample_rate(), options)
                .inspect_err(|e| log::warn!("music failed to start: {e}"))
                .ok()
        });
        Ok(Self {
            sound,
            synth,
            plays: 0,
            music,
            music_volume,
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

        if let Some(music) = &self.music {
            if ctx.input.key_pressed(Key::KeyM) {
                if music.is_paused() {
                    music.resume();
                } else {
                    music.pause();
                }
            }
            let step = if ctx.input.key_pressed(Key::ArrowUp) {
                0.1
            } else if ctx.input.key_pressed(Key::ArrowDown) {
                -0.1
            } else {
                0.0
            };
            if step != 0.0 {
                self.music_volume = (self.music_volume + step).clamp(0.0, 1.0);
                music.set_volume(self.music_volume);
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
        let music = match &self.music {
            None => "music: unavailable".to_string(),
            Some(m) if m.is_pending() => "music: waiting for a key press or click".to_string(),
            Some(m) if m.is_paused() => "music: paused (M)".to_string(),
            Some(_) => format!(
                "music: playing at {:.0}% (M pause, Up/Down volume)",
                self.music_volume * 100.0
            ),
        };
        p.text([24.0, 88.0], &music, 20.0, [0.6, 0.8, 1.0, 1.0]);
    }
}

/// Four rising notes, rendered back to back into one buffer to loop.
fn arpeggio(synth: &mut Synth) -> Vec<f32> {
    let params = synthie::prelude::SynthParams::default();
    [57, 60, 64, 67]
        .into_iter()
        .flat_map(|note| synth.render(params.clone(), note, 0.18, 0.07))
        .collect()
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
