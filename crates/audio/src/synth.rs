//! Chiptune/retro sound synthesis via [`synthie`].
//!
//! Two ways to use it, both through kira so the synth never opens the audio device itself
//! (two backends would compete for it, and on the web only one may play):
//!
//! - **Effects and jingles** are rendered ahead of time to mono PCM by [`Synth`] and played
//!   with [`crate::Audio::play_samples`]: a single note ([`Synth::render`]), a timed phrase of
//!   notes and drum hits ([`Synth::render_phrase`]) or a one-shot [`Song`]
//!   ([`Synth::render_song`]).
//! - **Music** plays live: [`crate::Audio::play_synth`] streams a [`Song`] (lanes of tracker
//!   notation at a tempo, looping) through kira and returns a [`crate::SynthHandle`] for
//!   volume, pause, stop and live [`AudioEvent`]s.
//!
//! This module re-exports what writing a song or a patch needs, so a game rarely has to name
//! [`synthie`] itself:
//!
//! ```
//! use game_toolkit_audio::synth::{ChannelNo, Song, Tempo, preset};
//!
//! // One row per eighth note at 132 bpm; lanes are whitespace-separated cells.
//! let song = Song::new(Tempo::bpm(132.0, 2))
//!     .patch(preset("C64 Bass").unwrap())
//!     .lane(ChannelNo(0), "E2@0 E2 G2 E2 | A2 . G2 -")?
//!     .lane(ChannelNo(3), "k h s h | k k s h")?;
//! assert_eq!(song.len(), 8);
//! # Ok::<(), game_toolkit_audio::synth::ParseError>(())
//! ```
//!
//! See [`synthie::pattern`] for the notation.

use synthie::prelude::{SynthProcessor, VoiceNormalization};

pub use synthie::kira::QueueFull;
pub use synthie::params::{AudioEvent, ChannelNo, DrumHit, MidiNote, Patch, SynthParams};
pub use synthie::pattern::{ParseError, Song, SongPlayer, Tempo};
pub use synthie::presets::sid::default_patches;
pub use synthie::{presets, songs};

/// Synthesis channels of a [`Synth`]; channels in a phrase or song must be below this.
const CHANNELS: usize = 8;

/// The parameters of the built-in preset called `name` (e.g. `"C64 Bass"`, `"Arp Lead"`,
/// `"PWM Lead"`, `"Tri Pad"`; see [`default_patches`] for all of them), or `None` if there
/// is no such preset.
pub fn preset(name: &str) -> Option<SynthParams> {
    default_patches()
        .into_iter()
        .find(|p| p.name == name)
        .map(|p| p.params)
}

/// A small chiptune synth that renders to mono PCM buffers, played with
/// [`crate::Audio::play_samples`]. Render one note ([`Synth::render`]), a timed phrase
/// ([`Synth::render_phrase`]), a one-shot [`Song`] ([`Synth::render_song`]) or a built-in
/// retro effect ([`Synth::blip`], [`Synth::coin`], [`Synth::thud`]).
pub struct Synth {
    processor: SynthProcessor<CHANNELS>,
    sample_rate: u32,
    target_peak: f32,
}

impl Synth {
    pub fn new(sample_rate: u32) -> Self {
        let mut processor = SynthProcessor::new(sample_rate as f32);
        // Renders are short and sparse, so dividing by the polyphony would only cost level;
        // normalization to `target_peak` sets the final level anyway.
        processor.set_voice_normalization(VoiceNormalization::Off);
        Self {
            processor,
            sample_rate,
            target_peak: 0.9,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Peak amplitude each rendered buffer is normalized to (`0.9` by default), so effects
    /// have a consistent, loud level regardless of the patch's gain and filtering. Set to
    /// `0.0` to disable normalization and keep synthie's raw output, e.g. to keep the
    /// relative levels of several renders that are mixed later.
    pub fn set_target_peak(&mut self, peak: f32) {
        self.target_peak = peak;
    }

    /// Builder form of [`Synth::set_target_peak`].
    pub fn with_target_peak(mut self, peak: f32) -> Self {
        self.target_peak = peak;
        self
    }

    /// Render one MIDI `note` with `params`: hold for `hold_secs`, then release and render
    /// `release_secs` of envelope tail. Returns mono PCM in `-1.0..=1.0`.
    pub fn render(
        &mut self,
        params: SynthParams,
        note: u8,
        hold_secs: f32,
        release_secs: f32,
    ) -> Vec<f32> {
        let hold = hold_secs.max(0.0);
        self.render_phrase(
            &[
                (0.0, AudioEvent::NoteOn(MidiNote(note))),
                (hold, AudioEvent::NoteOff(MidiNote(note))),
            ],
            params,
            release_secs,
        )
    }

    /// Render a short phrase: `events` at times in seconds from the start (in any order),
    /// applied on the exact sample, with `params` loaded on channel 0 (where
    /// [`AudioEvent::NoteOn`] plays), then `tail_secs` after the last event for release and
    /// effect tails. Notes may overlap and share the tail; drum hits ring out on their own.
    /// Normalized like [`Synth::render`].
    ///
    /// ```
    /// # use game_toolkit_audio::Synth;
    /// use game_toolkit_audio::synth::{AudioEvent, DrumHit, MidiNote, SynthParams};
    ///
    /// let mut synth = Synth::new(44_100);
    /// let fanfare = synth.render_phrase(
    ///     &[
    ///         (0.00, AudioEvent::Drum(DrumHit::Kick)),
    ///         (0.00, AudioEvent::NoteOn(MidiNote(60))),
    ///         (0.08, AudioEvent::NoteOn(MidiNote(64))),
    ///         (0.16, AudioEvent::NoteOn(MidiNote(67))),
    ///         (0.40, AudioEvent::NoteOff(MidiNote(60))),
    ///         (0.40, AudioEvent::NoteOff(MidiNote(64))),
    ///         (0.40, AudioEvent::NoteOff(MidiNote(67))),
    ///     ],
    ///     SynthParams::default(),
    ///     0.5,
    /// );
    /// assert_eq!(fanfare.len(), (0.9 * 44_100.0) as usize);
    /// ```
    pub fn render_phrase(
        &mut self,
        events: &[(f32, AudioEvent)],
        params: SynthParams,
        tail_secs: f32,
    ) -> Vec<f32> {
        let sr = self.sample_rate as f32;
        let mut timed: Vec<(usize, AudioEvent)> = events
            .iter()
            .map(|(t, e)| ((t.max(0.0) * sr) as usize, e.clone()))
            .collect();
        // Stable, so events at the same time keep their order.
        timed.sort_by_key(|(frame, _)| *frame);
        let setup = vec![AudioEvent::LoadPatch(Box::new(params))];
        let body = timed.last().map_or(0, |(frame, _)| *frame);
        self.render_timed(setup, timed, body, tail_secs)
    }

    /// Render one pass of `song` (its notes released at the end, as if
    /// [`Song::one_shot`]) plus `tail_secs` for release and effect tails, normalized like
    /// [`Synth::render`]. Handy for jingles written in tracker notation; channels start on
    /// the default patch until a lane switches with `@n`. For music that loops, stream the
    /// song with [`crate::Audio::play_synth`] instead.
    ///
    /// # Panics
    ///
    /// If a lane plays on a channel `>= 8`.
    pub fn render_song(&mut self, song: Song, tail_secs: f32) -> Vec<f32> {
        use synthie::prelude::Sequencer;

        assert!(
            song.lanes
                .iter()
                .all(|l| usize::from(l.channel.0) < CHANNELS),
            "Synth::render_song: lanes must play on channels 0..{CHANNELS}"
        );
        let mut player = SongPlayer::new(song.one_shot());
        let sr = self.sample_rate as f32;
        let mut timed = Vec::new();
        let mut events = Vec::new();
        let mut frame = 0usize;
        // The tick after the last row releases every note and finishes the song, so the
        // pass lasts until then even if the last rows only hold.
        loop {
            events.clear();
            let next = player.tick(sr, &mut events);
            timed.extend(events.drain(..).map(|e| (frame, e)));
            if player.is_finished() {
                break;
            }
            frame += next.max(1);
        }
        let setup = (0..CHANNELS)
            .map(|ch| AudioEvent::LoadPatchChannel(ChannelNo(ch as u8), Box::default()))
            .collect();
        self.render_timed(setup, timed, frame, tail_secs)
    }

    /// Render `timed` events (sorted by frame, none after `body`) after `setup`, for `body`
    /// frames plus `tail_secs`, and normalize.
    fn render_timed(
        &mut self,
        setup: Vec<AudioEvent>,
        timed: Vec<(usize, AudioEvent)>,
        body: usize,
        tail_secs: f32,
    ) -> Vec<f32> {
        let tail = (tail_secs.max(0.0) * self.sample_rate as f32) as usize;
        let end = body + tail;
        let mut buf = vec![0.0f32; end];

        // Panic first so any lingering voice or FX tail from a previous render does not
        // bleed in.
        let mut pending = vec![AudioEvent::Panic];
        pending.extend(setup);
        let mut at = 0;
        let mut timed = timed.into_iter().peekable();
        loop {
            while let Some((_, event)) = timed.next_if(|(frame, _)| *frame <= at) {
                pending.push(event);
            }
            let until = timed.peek().map_or(end, |(frame, _)| *frame);
            self.processor
                .process_block(&pending, &mut buf[at..until], 1);
            pending.clear();
            at = until;
            if at >= end {
                break;
            }
        }
        self.normalize(&mut buf);
        buf
    }

    /// Scale `buf` to [`Synth::set_target_peak`]: synthie applies its master gain
    /// (`DEFAULT_MASTER_GAIN`, 0.5) plus the patch's own volume and filtering, so raw output
    /// is much quieter than a typical sample file.
    fn normalize(&self, buf: &mut [f32]) {
        if self.target_peak <= 0.0 {
            return;
        }
        let peak = buf.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
        if peak > 1e-4 {
            let gain = self.target_peak / peak;
            for s in buf {
                *s *= gain;
            }
        }
    }

    /// A short high blip - a UI tick or pickup.
    pub fn blip(&mut self) -> Vec<f32> {
        self.render(SynthParams::default(), 84, 0.04, 0.08)
    }

    /// A classic two-tone coin jingle.
    pub fn coin(&mut self) -> Vec<f32> {
        let mut out = self.render(SynthParams::default(), 88, 0.06, 0.04);
        out.extend(self.render(SynthParams::default(), 95, 0.10, 0.12));
        out
    }

    /// A low, longer thud for hits or explosions.
    pub fn thud(&mut self) -> Vec<f32> {
        self.render(SynthParams::default(), 40, 0.10, 0.25)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(pcm: &[f32]) -> f32 {
        pcm.iter().fold(0.0f32, |m, &x| m.max(x.abs()))
    }

    #[test]
    fn render_is_normalized_to_target_peak() {
        let mut synth = Synth::new(44_100);
        let pcm = synth.blip();
        assert!(!pcm.is_empty());
        let peak = peak(&pcm);
        assert!((peak - 0.9).abs() < 0.02, "expected ~0.9 peak, got {peak}");
    }

    #[test]
    fn target_peak_zero_disables_normalization() {
        let mut synth = Synth::new(44_100).with_target_peak(0.0);
        let peak = peak(&synth.blip());
        // Raw synthie output (0.5 master gain, 0.7 patch volume) is well below the normalized 0.9.
        assert!(
            peak > 0.0 && peak < 0.6,
            "raw peak unexpectedly high: {peak}"
        );
    }

    #[test]
    fn render_keeps_its_length() {
        let mut synth = Synth::new(1_000);
        assert_eq!(
            synth.render(SynthParams::default(), 60, 0.1, 0.2).len(),
            300
        );
    }

    #[test]
    fn phrase_overlaps_notes_and_drums_in_one_buffer() {
        let mut synth = Synth::new(44_100).with_target_peak(0.0);
        let note = |t, n| (t, AudioEvent::NoteOn(MidiNote(n)));
        let pcm = synth.render_phrase(
            &[
                note(0.0, 60),
                note(0.1, 64),
                (0.2, AudioEvent::Drum(DrumHit::Snare)),
                (0.3, AudioEvent::NoteOff(MidiNote(60))),
                (0.3, AudioEvent::NoteOff(MidiNote(64))),
            ],
            SynthParams::default(),
            0.2,
        );
        assert_eq!(pcm.len(), (0.5 * 44_100.0) as usize);
        // Sounding in every segment between events.
        for (from, to) in [(0.0, 0.1), (0.1, 0.2), (0.2, 0.3)] {
            let seg = &pcm[(from * 44_100.0) as usize..(to * 44_100.0) as usize];
            assert!(peak(seg) > 0.01, "silent between {from} and {to}");
        }
        // The two notes overlap: louder (or at least different) than the first alone.
        let alone =
            Synth::new(44_100)
                .with_target_peak(0.0)
                .render(SynthParams::default(), 60, 0.3, 0.2);
        let seg = 4_410..8_820;
        assert_ne!(pcm[seg.clone()], alone[seg]);
    }

    #[test]
    fn phrase_events_land_on_their_sample_in_any_order() {
        let mut synth = Synth::new(1_000).with_target_peak(0.0);
        let pcm = synth.render_phrase(
            &[
                (0.25, AudioEvent::NoteOff(MidiNote(60))),
                (0.1, AudioEvent::NoteOn(MidiNote(60))),
            ],
            SynthParams::default(),
            0.1,
        );
        assert_eq!(pcm.len(), 350);
        assert!(pcm[..100].iter().all(|&s| s == 0.0), "sound before NoteOn");
        assert!(peak(&pcm[100..250]) > 0.01);
    }

    #[test]
    fn phrases_do_not_bleed_into_each_other() {
        let mut synth = Synth::new(44_100);
        let first = synth.render_phrase(&[], SynthParams::default(), 0.1);
        assert!(first.iter().all(|&s| s == 0.0));
        let _ = synth.render(SynthParams::default(), 60, 0.5, 0.0); // cut off mid-note
        let after = synth.render_phrase(&[], SynthParams::default(), 0.1);
        assert!(after.iter().all(|&s| s == 0.0), "previous note leaked");
    }

    #[test]
    fn song_renders_one_pass_plus_tail() {
        let song = Song::new(Tempo::rows_per_minute(600.0)) // 0.1 s per row
            .lane(ChannelNo(0), "C4 E4 G4 -")
            .unwrap()
            .lane(ChannelNo(1), "k . s .")
            .unwrap();
        let mut synth = Synth::new(1_000);
        let pcm = synth.render_song(song, 0.5);
        // Four rows of 100 frames; the last tick (releasing every note) lands at 400.
        assert_eq!(pcm.len(), 400 + 500);
        assert!((peak(&pcm) - 0.9).abs() < 0.02);
    }

    #[test]
    fn presets_are_found_by_name() {
        assert!(preset("C64 Bass").is_some());
        assert!(preset("no such preset").is_none());
    }
}
