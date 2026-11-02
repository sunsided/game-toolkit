//! Runs natively *and* in the browser from the same code: a sprite, text, a sound on click or
//! Space, looping synth music (M to pause), and the first gamepad's left stick. Build for the web with
//! `task web EXAMPLE=13_web` (trunk) and open http://localhost:8080.
//!
//! What's different on the web, and how this example handles it:
//! - Assets are fetched before `init`, from the list in `assets/manifest.txt`, so the
//!   synchronous `ctx.load_texture` / `ctx.load_sound` calls work unchanged.
//! - Browsers start audio only after a user gesture; the toolkit opens the device on the first
//!   key press, click or touch, so the first click both unlocks audio and plays the blip. The
//!   music is started in `init` with `PlayOptions::deferred`, so it waits for that click.
//! - Gamepads show up only after a button press (a browser privacy rule).

use game_toolkit::prelude::*;

struct Web {
    tex: TextureId,
    blip: Option<SoundId>,
    music: Option<SynthHandle>,
    pos: [f32; 2],
    clicks: u32,
}

impl Game for Web {
    fn init(ctx: &mut Context) -> Result<Self> {
        let tex = ctx.load_texture("sprite.png")?;
        let blip = ctx.load_sound("blip.wav")?;
        let music = ctx.audio.as_mut().and_then(|audio| {
            audio
                .play_synth(song(), PlayOptions::looped().volume(0.4).deferred())
                .inspect_err(|e| log::warn!("music: {e}"))
                .ok()
        });
        let (w, h) = ctx.gfx.size();
        Ok(Self {
            tex,
            blip,
            music,
            pos: [w as f32 * 0.5, h as f32 * 0.5],
            clicks: 0,
        })
    }

    fn update(&mut self, ctx: &mut Context, dt: f32) {
        if ctx.input.key_pressed(Key::Escape) {
            ctx.quit();
        }
        if ctx.input.mouse_pressed(MouseButton::Left) || ctx.input.key_pressed(Key::Space) {
            self.clicks += 1;
            if let (Some(audio), Some(id)) = (ctx.audio.as_mut(), self.blip)
                && let Err(e) = audio.play(id)
            {
                log::warn!("play: {e}");
            }
        }
        if ctx.input.key_pressed(Key::KeyM)
            && let Some(music) = &self.music
        {
            if music.is_paused() {
                music.resume();
            } else {
                music.pause();
            }
        }
        let mut dir = [0.0f32, 0.0f32];
        if let Some(pad) = ctx.input.first_gamepad() {
            dir = [pad.axis(Axis::LeftStickX), -pad.axis(Axis::LeftStickY)];
        }
        if ctx.input.key_held(Key::ArrowLeft) {
            dir[0] -= 1.0;
        }
        if ctx.input.key_held(Key::ArrowRight) {
            dir[0] += 1.0;
        }
        if ctx.input.key_held(Key::ArrowUp) {
            dir[1] -= 1.0;
        }
        if ctx.input.key_held(Key::ArrowDown) {
            dir[1] += 1.0;
        }
        self.pos[0] += dir[0] * 300.0 * dt;
        self.pos[1] += dir[1] * 300.0 * dt;
    }

    fn render(&mut self, ctx: &mut Context, frame: &mut Frame) {
        let t = ctx.time.elapsed.as_secs_f32();
        let fps = ctx.time.fps;
        let pad = ctx.input.first_gamepad().map_or_else(
            || "none (press a button)".to_string(),
            |p| p.name().to_string(),
        );
        let audio = match ctx.audio.as_ref() {
            Some(a) if a.is_started() => "on",
            Some(_) => "waiting for a click",
            None => "unavailable",
        };

        let mut p = frame.painter(&mut ctx.gfx);
        p.clear([0.08, 0.08, 0.11, 1.0]);
        let s = [128.0, 128.0];
        p.sprite_ex(
            self.tex,
            SpriteInstance::at([self.pos[0] - s[0] * 0.5, self.pos[1] - s[1] * 0.5], s)
                .with_rotation(t),
            0,
            BlendMode::Alpha,
        );
        let fg = [0.85, 0.85, 0.9, 1.0];
        p.text(
            [16.0, 16.0],
            "13_web: same code, native and browser",
            22.0,
            fg,
        );
        p.text(
            [16.0, 46.0],
            "arrows / left stick move, click or Space plays a blip, M pauses the music",
            18.0,
            fg,
        );
        p.text(
            [16.0, 72.0],
            &format!(
                "audio: {audio}  clicks: {}  gamepad: {pad}  fps: {fps:.0}",
                self.clicks
            ),
            18.0,
            [0.6, 0.8, 1.0, 1.0],
        );
    }
}

/// Two bars of bass, hats and a backbeat at 110 bpm, one row per eighth note.
fn song() -> synth::Song {
    use synth::{ChannelNo, Preset, Song, Tempo};

    Song::new(Tempo::bpm(110.0, 2))
        .patch(Preset::C64Bass.params())
        .lane(
            ChannelNo(0),
            "D2@0 D3 D2 D3 F2 F3 G2 G3 | D2 D3 D2 D3 C2 C3 A1 -",
        )
        .and_then(|s| s.lane(ChannelNo(3), "k h s h k k s h | k h s h k h s o"))
        .expect("the song's notation is valid")
}

fn main() -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    env_logger::init();

    run::<Web>(AppConfig {
        title: "13_web".into(),
        width: 960,
        height: 540,
        // The crate's assets/ natively; "assets" next to index.html on the web.
        asset_root: asset_root!(),
        canvas_id: Some("game".into()),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_song_parses() {
        assert_eq!(super::song().len(), 16);
    }
}
