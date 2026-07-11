//! Procedural audio (spec §5, `generate_music`): seeded note sequences as
//! raw mono PCM. Integer synthesis only.

use super::GeneratedAudio;
use crate::util::rng::SeededRng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Calm,
    Tense,
    Derelict,
}

/// Minor pentatonic degrees as frequency ratios ×1024 against the root.
/// Integer ratios: no equal-temperament floats, deliberately just intonation.
const PENTATONIC: [u32; 5] = [1024, 1229, 1365, 1536, 1820]; // 1, 6/5, 4/3, 3/2, 16/9

pub const SAMPLE_RATE: u32 = 22050;

/// A seeded two-note square-wave tone — the spike demo sound.
pub fn generate_tone(seed: u64, sample_rate: u32, seconds: u32) -> Vec<i16> {
    let mut rng = SeededRng::new(seed);
    let root_hz = 110 + rng.next_below(110) as u32;
    let fifth_hz = root_hz * 3 / 2;

    let total = (sample_rate * seconds) as usize;
    let mut samples = Vec::with_capacity(total);
    for i in 0..total {
        let hz = if i < total / 2 { root_hz } else { fifth_hz };
        let period = sample_rate / hz;
        let high = (i as u32 % period) < period / 2;
        samples.push(if high { 6000i16 } else { -6000i16 });
    }
    samples
}

/// A seeded melodic phrase: `duration_secs` of pentatonic notes with a
/// linear decay envelope. Mood selects register and note length.
pub fn generate_music(seed: u64, mood: Mood, duration_secs: u32) -> GeneratedAudio {
    let mut rng = SeededRng::new(seed);
    let (root_hz, note_len_div): (u32, u32) = match mood {
        Mood::Calm => (110, 2),    // A2, half-second notes
        Mood::Tense => (147, 4),   // D3, quarter-second notes
        Mood::Derelict => (73, 1), // D2, whole-second drones
    };
    let note_samples = SAMPLE_RATE / note_len_div;
    let total = (SAMPLE_RATE * duration_secs) as usize;

    let mut samples = Vec::with_capacity(total);
    let mut written = 0usize;
    while written < total {
        let degree = PENTATONIC[rng.next_below(PENTATONIC.len() as u64) as usize];
        let octave = 1 + rng.next_below(2) as u32; // 1x or 2x register
        let hz = root_hz * degree * octave / 1024;
        let period = (SAMPLE_RATE / hz.max(1)).max(2);

        let n = (note_samples as usize).min(total - written);
        for i in 0..n {
            let high = (i as u32 % period) < period / 2;
            // Linear decay envelope in integer math.
            let env = 8000 * (n - i) as i64 / n as i64;
            samples.push(if high { env as i16 } else { -(env as i16) });
        }
        written += n;
    }

    GeneratedAudio {
        sample_rate: SAMPLE_RATE,
        samples,
    }
}

/// Wrap mono PCM in a WAV container. Not a generator — a plain data
/// transform shared by the client bridge (bevy_audio decodes WAV) and the
/// CLI (`gen music --wav out.wav`).
pub fn to_wav_bytes(audio: &GeneratedAudio) -> Vec<u8> {
    let data_len = (audio.samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&audio.sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(audio.sample_rate * 2).to_le_bytes()); // byte rate
    bytes.extend_from_slice(&2u16.to_le_bytes()); // block align
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for s in &audio.samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_well_formed() {
        let audio = generate_music(1, Mood::Calm, 1);
        let wav = to_wav_bytes(&audio);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(wav.len(), 44 + audio.samples.len() * 2);
    }

    #[test]
    fn deterministic() {
        assert_eq!(
            generate_music(5, Mood::Calm, 2),
            generate_music(5, Mood::Calm, 2)
        );
    }

    #[test]
    fn exact_duration() {
        let audio = generate_music(5, Mood::Tense, 3);
        assert_eq!(audio.samples.len(), (SAMPLE_RATE * 3) as usize);
    }

    #[test]
    fn moods_differ() {
        assert_ne!(
            generate_music(5, Mood::Calm, 1),
            generate_music(5, Mood::Derelict, 1)
        );
    }
}
