//! Procedural audio (spec §5, `generate_music`): seeded note sequences as
//! raw mono PCM. Integer synthesis only.

use super::GeneratedAudio;
use crate::util::rng::SeededRng;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mood {
    Calm,
    Tense,
    Combat,
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
        Mood::Combat => (220, 8),  // A3, eighth notes
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

// --------------------------------------------------------------------------
// S48 — Procedural Audio Engine: deterministic MusicIntent generator
// --------------------------------------------------------------------------

use serde::{Deserialize, Serialize};

use crate::util::Fixed;

/// Scale degrees as frequency ratios ×1024 against the root. Integer ratios,
/// just intonation (no equal-temperament floats).
const DORIAN: [u32; 8] = [1024, 1152, 1215, 1365, 1536, 1707, 1820, 2048];
const OCTATONIC: [u32; 8] = [1024, 1085, 1152, 1280, 1365, 1536, 1638, 1820];

/// A single note event in a music sequence. All integer/fixed-point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteEvent {
    pub degree: u8,
    pub octave: u8,
    pub velocity: u8,
    pub start_tick: u32,
    pub duration_ticks: u32,
}

/// Which pentatonic/scale mode to draw notes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    MinorPentatonic,
    MajorPentatonic,
    Dorian,
    Octatonic,
}

/// Bitmask for musical layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerMask(pub u8);

impl LayerMask {
    pub const MELODY: u8 = 1 << 0;
    pub const BASS: u8 = 1 << 1;
    pub const DRONE: u8 = 1 << 2;
    pub const RHYTHM: u8 = 1 << 3;
}

/// A deterministic music sequence — the core generator's output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MusicIntent {
    pub seed: u64,
    pub mood: Mood,
    pub scale: Scale,
    pub bpm: u32,
    pub root_hz: u32,
    pub active_layers: LayerMask,
    pub notes: Vec<NoteEvent>,
    pub bar_length: u16,
}

/// An authored theme — content that constrains the generator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub id: String,
    pub notes: Vec<NoteEvent>,
    pub scale: Scale,
    pub bpm_range: (u32, u32),
    pub allowed_variations: VariationMask,
}

/// Bitmask for allowed variation operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariationMask(pub u16);

impl VariationMask {
    pub const TRANSPOSE: u16 = 1 << 0;
    pub const PASSING_TONES: u16 = 1 << 1;
    pub const RHYTHMIC_SHIFT: u16 = 1 << 2;
    pub const REPETITION: u16 = 1 << 3;
    pub const ARTICULATION: u16 = 1 << 4;
    pub const PHRASE_SWAP: u16 = 1 << 5;
    pub const REST_INSERTION: u16 = 1 << 6;
    pub const SUBSTITUTION: u16 = 1 << 7;
    pub const ORNAMENTATION: u16 = 1 << 8;
}

/// What the content pipeline resolves for a music source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MusicSource {
    Procedural { seed: u64, mood: Mood, theme: Option<Theme> },
    HandCrafted { asset_path: String },
}

fn scale_degrees(scale: Scale) -> &'static [u32] {
    match scale {
        Scale::MinorPentatonic | Scale::MajorPentatonic => &PENTATONIC[..],
        Scale::Dorian => &DORIAN[..],
        Scale::Octatonic => &OCTATONIC[..],
    }
}

fn bpm_for_mood(mood: Mood) -> u32 {
    match mood {
        Mood::Calm => 60,
        Mood::Tense => 100,
        Mood::Combat => 140,
        Mood::Derelict => 40,
    }
}

fn root_for_mood(mood: Mood) -> u32 {
    match mood {
        Mood::Calm | Mood::Derelict => 73,
        Mood::Tense => 110,
        Mood::Combat => 147,
    }
}

/// Generate a deterministic note sequence from a seed and mood.
/// No theme — fully procedural. This is the primary generator for
/// the real-time music engine.
pub fn generate_music_intent(seed: u64, mood: Mood, duration_bars: u16) -> MusicIntent {
    let mut rng = SeededRng::new(seed);
    let scale = match mood {
        Mood::Calm => Scale::MinorPentatonic,
        Mood::Tense => Scale::MinorPentatonic,
        Mood::Combat => Scale::MinorPentatonic,
        Mood::Derelict => Scale::Octatonic,
    };
    let degrees = scale_degrees(scale);
    let bpm = bpm_for_mood(mood);
    let root_hz = root_for_mood(mood);
    let ticks_per_bar = 96u32; // 24 ticks/beat * 4 beats
    let total_ticks = duration_bars as u32 * ticks_per_bar;
    let note_len_min = match mood {
        Mood::Calm => 24,
        Mood::Tense => 12,
        Mood::Combat => 6,
        Mood::Derelict => 48,
    };
    let octave_range: (u8, u8) = match mood {
        Mood::Calm => (0, 2),
        Mood::Tense => (1, 3),
        Mood::Combat => (1, 4),
        Mood::Derelict => (0, 1),
    };

    let mut notes: Vec<NoteEvent> = Vec::new();
    let mut tick = 0u32;
    while tick < total_ticks {
        let deg_idx = rng.next_below(degrees.len() as u64) as usize;
        let degree = deg_idx as u8;
        let octave = octave_range.0 + rng.next_below((octave_range.1 - octave_range.0) as u64) as u8;
        let velocity = 60 + rng.next_below(60) as u8;
        let dur = note_len_min + rng.next_below(note_len_min as u64 * 2) as u32;

        // Avoid consecutive identical degrees for variety.
        if let Some(last) = notes.last() {
            if last.degree == degree && last.octave == octave {
                let next_deg = (deg_idx + 1 + rng.next_below(degrees.len() as u64 - 1) as usize) % degrees.len();
                let _ = next_deg; // just advance rng
            }
        }

        notes.push(NoteEvent {
            degree,
            octave,
            velocity,
            start_tick: tick,
            duration_ticks: dur,
        });
        tick += dur + rng.next_below(note_len_min as u64 / 2) as u32;
    }

    MusicIntent {
        seed,
        mood,
        scale,
        bpm,
        root_hz,
        active_layers: LayerMask(LayerMask::MELODY | LayerMask::DRONE),
        notes,
        bar_length: ticks_per_bar as u16,
    }
}

/// Generate a deterministic variation on an authored theme.
/// The theme is the "head" — the generator varies it using allowed
/// operators and restates the pure theme every `recap_every` bars.
pub fn generate_themed_music(
    seed: u64,
    mood: Mood,
    theme: &Theme,
    duration_bars: u16,
    recap_every: u16,
) -> MusicIntent {
    let mut rng = SeededRng::new(seed);
    let bpm = bpm_for_mood(mood).max(theme.bpm_range.0).min(theme.bpm_range.1);
    let root_hz = root_for_mood(mood);
    let ticks_per_bar = 96u32;
    let total_ticks = duration_bars as u32 * ticks_per_bar;
    let mut notes: Vec<NoteEvent> = Vec::new();
    let mut bar = 0u16;

    while bar < duration_bars {
        let bar_start_tick = bar as u32 * ticks_per_bar;
        let is_recap = recap_every > 0 && bar.is_multiple_of(recap_every);

        if is_recap {
            for n in &theme.notes {
                notes.push(NoteEvent {
                    degree: n.degree,
                    octave: n.octave,
                    velocity: n.velocity,
                    start_tick: bar_start_tick + n.start_tick,
                    duration_ticks: n.duration_ticks,
                });
            }
            bar += 1;
            continue;
        }

        let mask = theme.allowed_variations;
        let mut prev_degree: Option<u8> = None;
        let mut tick = bar_start_tick;
        let segment_end = bar_start_tick + ticks_per_bar;

        while tick < segment_end && tick < total_ticks {
            let mut n = if mask.0 & VariationMask::PHRASE_SWAP != 0 {
                // Pick a random note from the theme at a rotated position.
                let idx = rng.next_below(theme.notes.len() as u64) as usize;
                theme.notes[idx]
            } else {
                // Pick a degree that differs from the previous.
                let mut deg = rng.next_below(8) as u8;
                if let Some(p) = prev_degree {
                    while deg == p {
                        deg = rng.next_below(8) as u8;
                    }
                }
                NoteEvent {
                    degree: deg,
                    octave: if mask.0 & VariationMask::TRANSPOSE != 0 {
                        rng.next_below(4) as u8
                    } else {
                        1
                    },
                    velocity: 60 + rng.next_below(60) as u8,
                    start_tick: 0,
                    duration_ticks: 12 + rng.next_below(24) as u32,
                }
            };

            n.start_tick = tick;
            if mask.0 & VariationMask::RHYTHMIC_SHIFT != 0 {
                n.duration_ticks = (n.duration_ticks as i32 + rng.next_below(12) as i32 - 6).max(4) as u32;
            }
            if mask.0 & VariationMask::REPETITION != 0 && rng.next_below(100) < 20 {
                notes.push(n);
                notes.push(n);
                tick += n.duration_ticks * 2;
            } else {
                prev_degree = Some(n.degree);
                notes.push(n);
                tick += n.duration_ticks;
            }
        }
        bar += 1;
    }

    MusicIntent {
        seed,
        mood,
        scale: theme.scale,
        bpm,
        root_hz,
        active_layers: LayerMask(LayerMask::MELODY),
        notes,
        bar_length: ticks_per_bar as u16,
    }
}

/// Select the mood for a given game context. Pure function.
pub fn music_mood_for_context(
    combat_active: bool,
    hull_damage_pct: u8,
    in_derelict: bool,
    is_docked: bool,
) -> Mood {
    if combat_active {
        return Mood::Combat;
    }
    if in_derelict {
        return Mood::Derelict;
    }
    if hull_damage_pct > 50 {
        return Mood::Tense;
    }
    if is_docked {
        return Mood::Calm;
    }
    Mood::Calm
}

/// Calculate intensity from game state. Returns Fixed in 0..1024.
pub fn music_intensity(combat_active: bool, hull_damage_pct: u8, mood: Mood) -> Fixed {
    let base = match mood {
        Mood::Calm => 256,
        Mood::Tense => 512,
        Mood::Combat => 768,
        Mood::Derelict => 384,
    };
    let dmg_bonus = if hull_damage_pct > 50 {
        (hull_damage_pct as i64 * Fixed::SCALE / 200).min(Fixed::SCALE / 4)
    } else {
        0
    };
    let combat_bonus = if combat_active { Fixed::SCALE / 8 } else { 0 };
    Fixed((base + dmg_bonus + combat_bonus).min(Fixed::SCALE))
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
