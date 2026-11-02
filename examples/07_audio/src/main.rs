//! Plays a loaded WAV (Space) and synthesized retro effects (1/2/3, a jingle on 5) through
//! `game-toolkit-audio`, plus looping synth music (M to pause/resume, Up/Down for its volume,
//! C for a live crash cymbal) and a tone generated while it plays (4 to start/stop).
//!
//! `ctx.audio` is an `Option<Audio>`: when the audio backend fails to initialize the
//! toolkit keeps running muted rather than aborting. This example mirrors that contract -
//! every audio call is guarded, so it still opens a window and runs with no audio device.
//!
//! The retro effects are rendered by `Synth` (the `synthie` chiptune engine) to PCM and
//! played via `Audio::play_samples`, so no audio file is needed. The jingle on 5 is one
//! `Synth::render_phrase`: overlapping notes and drum hits at times in seconds.
//!
//! The music is a `synth::Song` - lanes of tracker notation at a tempo - played live with
//! `Audio::play_synth`, which sequences it sample-accurately on the audio thread and returns a
//! `SynthHandle`. It is started in `init` with `PlayOptions::looped().deferred()`: on the web,
//! where audio only starts after the first key press or click, the deferred play is queued and
//! begins on that gesture instead of failing.
//!
//! The tone on 4 is an `Audio::play_stream`: a closure that computes samples on the audio
//! thread as the mixer asks for them, the way a running synth or an emulated sound chip would.

use game_toolkit::prelude::*;

struct AudioDemo {
    sound: Option<SoundId>,
    synth: Synth,
    plays: u32,
    music: Option<SynthHandle>,
    music_volume: f32,
    tone: Option<StreamHandle>,
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
        let synth = Synth::new(44_100);
        let music_volume = 0.5;
        let music = ctx.audio.as_mut().and_then(|audio| {
            let options = PlayOptions::looped().volume(music_volume).deferred();
            audio
                .play_synth(song()?, options)
                .inspect_err(|e| log::warn!("music failed to start: {e}"))
                .ok()
        });
        Ok(Self {
            sound,
            synth,
            plays: 0,
            music,
            music_volume,
            tone: None,
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
            // A live event on top of the song: lands within one audio block.
            if ctx.input.key_pressed(Key::KeyC) {
                let _ = music.drum(synth::DrumHit::Crash);
            }
        }

        // A generated tone, toggled on 4. Stopping fades it out; a stopped stream is gone,
        // so the next press starts a fresh one.
        if ctx.input.key_pressed(Key::Digit4)
            && let Some(audio) = ctx.audio.as_mut()
        {
            match self.tone.take() {
                Some(tone) => tone.stop(),
                None => match audio.play_stream(warble(TONE_RATE), TONE_RATE) {
                    Ok(handle) => {
                        handle.set_volume(self.music_volume);
                        self.tone = Some(handle);
                    }
                    Err(e) => log::warn!("tone failed to start: {e}"),
                },
            }
        }

        // Synthesized retro effects on the number keys.
        let pcm = if ctx.input.key_pressed(Key::Digit1) {
            Some(self.synth.blip())
        } else if ctx.input.key_pressed(Key::Digit2) {
            Some(self.synth.coin())
        } else if ctx.input.key_pressed(Key::Digit3) {
            Some(self.synth.thud())
        } else if ctx.input.key_pressed(Key::Digit5) {
            Some(fanfare(&mut self.synth))
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
            "Space: WAV   1: blip   2: coin   3: thud   4: tone   5: jingle   (Esc to quit)"
        } else {
            "No device - running muted. 1-5 synth, Space WAV (Esc to quit)"
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
                "music: playing at {:.0}% (M pause, Up/Down volume, C crash)",
                self.music_volume * 100.0
            ),
        };
        p.text([24.0, 88.0], &music, 20.0, [0.6, 0.8, 1.0, 1.0]);
        let tone = if self.tone.is_some() {
            "tone: streaming (4 to stop)"
        } else {
            "tone: off (4 to start)"
        };
        p.text([24.0, 120.0], tone, 20.0, [0.6, 0.8, 1.0, 1.0]);
    }
}

/// Sample rate the tone is generated at; the stream resamples it to the output rate.
const TONE_RATE: u32 = 22_050;

/// A square-ish lead whose pitch wobbles with a slow vibrato, computed sample by sample.
fn warble(sample_rate: u32) -> impl AudioStream {
    let rate = sample_rate as f32;
    let (mut phase, mut lfo) = (0.0f32, 0.0f32);
    move |out: &mut [f32]| {
        for s in out {
            let freq = 330.0 * (1.0 + 0.03 * (lfo * std::f32::consts::TAU).sin());
            *s = if phase < 0.5 { 0.15 } else { -0.15 };
            phase = (phase + freq / rate).fract();
            lfo = (lfo + 5.0 / rate).fract();
        }
    }
}

/// Four bars in A minor at 128 bpm, one row per eighth note. Each lane is one synth channel;
/// `@n` switches the channel to the song's patch `n`, `.` holds, `-` releases, and the drum
/// lane's `k s h o c` are kick, snare, closed and open hi-hat, and crash.
fn song() -> Option<synth::Song> {
    use synth::{ChannelNo, Song, Tempo, preset};

    let song = Song::new(Tempo::bpm(128.0, 2))
        .patch(preset("PWM Lead")?)
        .patch(preset("C64 Bass")?)
        .lane(
            ChannelNo(0),
            "A4@0 . C5 . E5 . D5 C5 | B4 . G4 . B4 . D5 - |
             A4 . C5 . E5 . A5 G5   | E5 . . . - . . .",
        )
        .and_then(|s| {
            s.lane(
                ChannelNo(1),
                "A2@1 A3 A2 A3 A2 A3 A2 A3 | G2 G3 G2 G3 G2 G3 G2 G3 |
                 F2 F3 F2 F3 F2 F3 F2 F3   | E2 E3 E2 E3 E2 E3 E2 -",
            )
        })
        .and_then(|s| {
            s.lane(
                ChannelNo(3),
                "k+c h s h k k s h | k h s h k k s o |
                 k h s h k k s h   | k h s h k s s s",
            )
        });
    song.inspect_err(|e| log::warn!("song: {e}")).ok()
}

/// A level-clear jingle: a rising arpeggio whose notes overlap, a kick under the first note and
/// a crash under the last, all in one buffer.
fn fanfare(synth: &mut Synth) -> Vec<f32> {
    use synth::{AudioEvent, DrumHit, MidiNote};

    let on = |t, n| (t, AudioEvent::NoteOn(MidiNote(n)));
    let off = |t, n| (t, AudioEvent::NoteOff(MidiNote(n)));
    let params = synth::preset("Plucky Pulse").unwrap_or_default();
    synth.render_phrase(
        &[
            (0.0, AudioEvent::Drum(DrumHit::Kick)),
            on(0.0, 72),
            on(0.08, 76),
            on(0.16, 79),
            (0.24, AudioEvent::Drum(DrumHit::Crash)),
            on(0.24, 84),
            off(0.5, 72),
            off(0.5, 76),
            off(0.5, 79),
            off(0.5, 84),
        ],
        params,
        0.6,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_song_parses_and_sounds() {
        let song = song().expect("song parses");
        assert_eq!(song.len(), 32);
        let pcm = Synth::new(22_050).render_song(song, 0.2);
        assert!(pcm.iter().any(|s| s.abs() > 0.1));
        assert!(!fanfare(&mut Synth::new(22_050)).is_empty());
    }
}
