//! Chiptune/retro sound synthesis via `synthie`, rendered to PCM you can play with
//! [`crate::Audio::play_samples`].
//!
//! This uses synthie's I/O-free DSP processor, so it never opens the audio device itself -
//! playback still goes through kira like every other sound, avoiding two backends competing
//! for the device.

use synthie::prelude::{AudioEvent, MidiNote, SynthParams, SynthProcessor};

/// A small chiptune synth. Render a note (or a built-in retro effect) to a mono PCM buffer,
/// then play it with [`crate::Audio::play_samples`]. Re-exported `synthie` types
/// ([`synthie::prelude::SynthParams`] etc.) give full patch control via [`Synth::render`].
pub struct Synth {
    processor: SynthProcessor<8>,
    sample_rate: u32,
    target_peak: f32,
}

impl Synth {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            processor: SynthProcessor::new(sample_rate as f32),
            sample_rate,
            target_peak: 0.9,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Peak amplitude each rendered buffer is normalized to (`0.9` by default), so effects
    /// have a consistent, loud level regardless of the patch's gain and filtering. Set to
    /// `0.0` to disable normalization and play synthie's raw output.
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
        let sr = self.sample_rate as f32;
        let hold = (hold_secs.max(0.0) * sr) as usize;
        let release = (release_secs.max(0.0) * sr) as usize;
        let mut buf = vec![0.0f32; hold + release];
        // Panic first so any lingering voice/tail from a previous render does not bleed in.
        let on = [
            AudioEvent::Panic,
            AudioEvent::LoadPatch(Box::new(params)),
            AudioEvent::NoteOn(MidiNote(note)),
        ];
        self.processor.process_block(&on, &mut buf[..hold], 1);
        let off = [AudioEvent::NoteOff(MidiNote(note))];
        self.processor.process_block(&off, &mut buf[hold..], 1);

        // Normalize to a consistent peak: synthie applies a 0.5 master gain plus the patch's
        // own volume/filtering, so raw output is much quieter than a typical sample file.
        if self.target_peak > 0.0 {
            let peak = buf.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
            if peak > 1e-4 {
                let gain = self.target_peak / peak;
                for s in &mut buf {
                    *s *= gain;
                }
            }
        }
        buf
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

    #[test]
    fn render_is_normalized_to_target_peak() {
        let mut synth = Synth::new(44_100);
        let pcm = synth.blip();
        assert!(!pcm.is_empty());
        let peak = pcm.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        assert!((peak - 0.9).abs() < 0.02, "expected ~0.9 peak, got {peak}");
    }

    #[test]
    fn target_peak_zero_disables_normalization() {
        let mut synth = Synth::new(44_100).with_target_peak(0.0);
        let pcm = synth.blip();
        let peak = pcm.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        // Raw synthie output (0.5 master gain etc.) is well below the normalized 0.9.
        assert!(
            peak > 0.0 && peak < 0.6,
            "raw peak unexpectedly high: {peak}"
        );
    }
}
