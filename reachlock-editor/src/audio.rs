//! Theme preview audio (S101 follow-up).
//!
//! Themes were previewable only as four read-only labels — you could not edit
//! one and could not hear one, which makes authoring music a guessing game.
//!
//! Rendering happens here rather than in core: core stays free of audio IO
//! (iron rule 1), and it already does the musical half — `generate_themed_music`
//! turns a `Theme` into a `MusicIntent` (notes, bpm, root), and `to_wav_bytes`
//! encodes PCM. What is missing in between is a synth, and the editor supplies
//! one.
//!
//! **The three layers mirror `reachlock-client`'s `schedule_intent`** — sine
//! melody scaled by velocity, a triangle drone an octave below the root, and a
//! noise burst on each quarter note — so what an author hears here is close to
//! what the game plays. It is not sample-identical: the client runs these
//! through fundsp with its own fades and filtering, and pulling fundsp into a
//! GUI crate to close that gap is not worth it for a preview. Anywhere the two
//! could drift, the client is the truth.

use reachlock_core::generator::music::MusicIntent;
use reachlock_core::generator::GeneratedAudio;

const SAMPLE_RATE: u32 = 44_100;

/// The pitch mapping the client uses, kept identical on purpose: a preview
/// that transposes differently from the game is worse than no preview.
fn degree_freq(degree: u8, octave: u8, root_hz: u32) -> f64 {
    let semitones = (degree as i32 + octave as i32 * 12 - 12).clamp(0, 108) as u32;
    root_hz as f64 * 2.0f64.powf(semitones as f64 / 12.0)
}

/// Short linear fades. Without them each note starts and stops on a
/// discontinuity, which is audible as a click on every single note and reads
/// as "the theme is broken" rather than "the preview is naive".
fn envelope(i: usize, total: usize) -> f64 {
    const FADE: usize = 220; // ~5ms
    if total <= FADE * 2 {
        return 1.0;
    }
    if i < FADE {
        i as f64 / FADE as f64
    } else if i >= total - FADE {
        (total - i) as f64 / FADE as f64
    } else {
        1.0
    }
}

/// Render an intent to mono PCM.
pub fn render(intent: &MusicIntent) -> GeneratedAudio {
    let bpm = intent.bpm.max(1) as f64;
    // 24 ticks per quarter note, matching the client.
    let tick_sec = 60.0 / bpm / 24.0;
    let end_tick = intent
        .notes
        .iter()
        .map(|n| n.start_tick + n.duration_ticks)
        .max()
        .unwrap_or(96);
    // A tail so the drone's release is not cut off mid-fade.
    let total_samples = ((end_tick as f64 * tick_sec + 1.0) * SAMPLE_RATE as f64) as usize;
    let mut buf = vec![0f64; total_samples.max(SAMPLE_RATE as usize / 4)];

    // Melody.
    for note in &intent.notes {
        if note.velocity == 0 {
            continue;
        }
        let freq = degree_freq(note.degree, note.octave, intent.root_hz);
        let start = (note.start_tick as f64 * tick_sec * SAMPLE_RATE as f64) as usize;
        let len = (note.duration_ticks as f64 * tick_sec * SAMPLE_RATE as f64) as usize;
        let amp = note.velocity as f64 / 127.0 * 0.2;
        for i in 0..len {
            let Some(slot) = buf.get_mut(start + i) else {
                break;
            };
            let t = i as f64 / SAMPLE_RATE as f64;
            *slot += (t * freq * std::f64::consts::TAU).sin() * amp * envelope(i, len);
        }
    }

    // Bass drone, an octave below the root, for the whole piece.
    let bass_hz = intent.root_hz as f64 * 0.5;
    let n = buf.len();
    for (i, slot) in buf.iter_mut().enumerate() {
        let t = i as f64 / SAMPLE_RATE as f64;
        // Triangle from the sine's arcsine — no wavetable needed.
        let tri = (t * bass_hz * std::f64::consts::TAU).sin().asin() * (2.0 / std::f64::consts::PI);
        *slot += tri * 0.12 * envelope(i, n);
    }

    // Rhythm: a short noise burst on each quarter note.
    let quarter = (tick_sec * 24.0 * SAMPLE_RATE as f64) as usize;
    if quarter > 0 {
        let burst = (SAMPLE_RATE as usize / 50).max(1);
        let mut rng: u32 = 0x1234_5678;
        let mut at = 0usize;
        while at < buf.len() {
            for i in 0..burst {
                let Some(slot) = buf.get_mut(at + i) else {
                    break;
                };
                // xorshift: deterministic, and no RNG dependency for what is
                // three-hundredths of a second of hiss.
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let noise = (rng as f64 / u32::MAX as f64) * 2.0 - 1.0;
                *slot += noise * 0.05 * (1.0 - i as f64 / burst as f64);
            }
            at += quarter;
        }
    }

    // Normalize only when clipping, so a quiet theme still sounds quiet —
    // an author judging a mix needs relative loudness preserved.
    let peak = buf.iter().fold(0f64, |m, s| m.max(s.abs()));
    let scale = if peak > 1.0 { 1.0 / peak } else { 1.0 };

    GeneratedAudio {
        sample_rate: SAMPLE_RATE,
        samples: buf
            .iter()
            .map(|s| (s * scale * i16::MAX as f64) as i16)
            .collect(),
    }
}

/// Play PCM on the default output device.
///
/// Returns a handle whose drop stops playback. Errors are strings rather than
/// a panic: a machine with no audio device is a normal state for a content
/// editor, and it must not take the editor down.
pub fn play(audio: &GeneratedAudio) -> Result<Playing, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no audio output device")?;
    let config = device
        .default_output_config()
        .map_err(|e| format!("no usable output config: {e}"))?;

    let channels = config.channels() as usize;
    let out_rate = config.sample_rate().0 as f64;
    let in_rate = audio.sample_rate as f64;
    // Nearest-sample resample. The device rate is often 48k against our 44.1k,
    // and playing the buffer as-is would shift the pitch of the preview.
    let samples: Vec<f32> = audio.samples.iter().map(|s| *s as f32 / 32768.0).collect();

    let mut pos = 0f64;
    let step = in_rate / out_rate;
    let stream = device
        .build_output_stream(
            &config.config(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for frame in data.chunks_mut(channels) {
                    let idx = pos as usize;
                    let v = samples.get(idx).copied().unwrap_or(0.0);
                    for slot in frame.iter_mut() {
                        *slot = v;
                    }
                    pos += step;
                }
            },
            |err| tracing::warn!("audio output error: {err}"),
            None,
        )
        .map_err(|e| format!("could not open the audio stream: {e}"))?;
    stream
        .play()
        .map_err(|e| format!("could not start playback: {e}"))?;
    Ok(Playing { _stream: stream })
}

/// Dropping this stops playback.
pub struct Playing {
    _stream: cpal::Stream,
}

#[cfg(test)]
mod tests {
    use super::*;
    use reachlock_core::generator::music::{
        generate_themed_music, Mood, Scale, Theme, VariationMask,
    };

    fn theme() -> Theme {
        Theme {
            id: "test".into(),
            notes: Vec::new(),
            scale: Scale::MinorPentatonic,
            bpm_range: (90, 120),
            allowed_variations: VariationMask(u16::MAX),
        }
    }

    #[test]
    fn a_theme_renders_to_audible_pcm() {
        let intent = generate_themed_music(7, Mood::Calm, &theme(), 4, 2);
        let audio = render(&intent);
        assert_eq!(audio.sample_rate, SAMPLE_RATE);
        assert!(!audio.samples.is_empty());
        let peak = audio
            .samples
            .iter()
            .map(|s| s.unsigned_abs())
            .max()
            .unwrap();
        assert!(peak > 1000, "rendered near-silence (peak {peak})");
    }

    /// Same theme and seed must render the same audio, like everything else
    /// downstream of a seed in this project.
    #[test]
    fn rendering_is_deterministic() {
        let intent = generate_themed_music(11, Mood::Calm, &theme(), 4, 2);
        assert_eq!(render(&intent).samples, render(&intent).samples);
    }

    /// Clipping is the one case worth rescaling; a quiet theme must stay
    /// quiet so relative loudness is judgeable.
    #[test]
    fn output_never_clips() {
        let intent = generate_themed_music(3, Mood::Tense, &theme(), 8, 2);
        let audio = render(&intent);
        assert!(audio.samples.iter().all(|s| *s != i16::MIN));
    }

    /// The exported WAV must be a real file another tool can open, not just
    /// bytes that happen to start with "RIFF".
    #[test]
    fn the_exported_wav_is_well_formed() {
        use reachlock_core::generator::music::to_wav_bytes;
        let intent = generate_themed_music(5, Mood::Calm, &theme(), 4, 2);
        let bytes = to_wav_bytes(&render(&intent));
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        let riff_len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        assert_eq!(
            riff_len + 8,
            bytes.len(),
            "RIFF length field disagrees with the actual file size"
        );
    }

    #[test]
    fn the_pitch_mapping_matches_the_clients() {
        // Root, then one octave up.
        assert!((degree_freq(12, 1, 220) - 440.0).abs() < 0.001);
        assert!((degree_freq(0, 1, 220) - 220.0).abs() < 0.001);
    }
}
