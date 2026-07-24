//! Procedural SFX (S49): deterministic sound effect generation from seed.
//! Each SFX type produces a short PCM buffer that the client plays via
//! bevy_audio. Pure & deterministic — same seed + type always produces
//! the same audio (important for determinism and for authored story beats).

use serde::{Deserialize, Serialize};

use crate::generator::GeneratedAudio;
use crate::util::rng::SeededRng;

/// Categories of SFX the generator can produce. Each type has a characteristic
/// waveform, envelope, and frequency range — all deterministic from seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SfxKind {
    /// Short UI confirmation beep.
    UiConfirm,
    /// Warning alarm (hull damage, boarding alert).
    Alarm,
    /// Door or airlock opening.
    DoorOpen,
    /// Heavy door or blast door closing.
    DoorClose,
    /// Predecessor artifact resonance pulse.
    ArtifactPulse,
    /// Engine hum / drive spool-up.
    EngineHum,
    /// Weapon fire (laser bullet impact variant).
    WeaponFire,
    /// Explosion (distant or near).
    Explosion,
    /// Generic footstep.
    Footstep,
    /// Subspace signal burst (story beat).
    SignalPulse,
}

impl SfxKind {
    fn sample_count(self) -> usize {
        match self {
            SfxKind::UiConfirm => 8000,      // ~180ms at 44100
            SfxKind::Alarm => 44100,         // 1s
            SfxKind::DoorOpen => 22050,      // 0.5s
            SfxKind::DoorClose => 22050,
            SfxKind::ArtifactPulse => 44100, // 1s
            SfxKind::EngineHum => 44100,     // 1s
            SfxKind::WeaponFire => 4000,     // ~90ms
            SfxKind::Explosion => 44100,     // 1s
            SfxKind::Footstep => 4000,
            SfxKind::SignalPulse => 44100,
        }
    }
}

/// Generate a deterministic SFX audio clip from a seed and kind.
/// Produces mono 44100 Hz PCM i16 samples.
pub fn generate_sfx(seed: u64, kind: SfxKind) -> GeneratedAudio {
    let sample_rate = 44100;
    let total = kind.sample_count();
    let mut rng = SeededRng::new(seed);
    let mut samples = Vec::with_capacity(total);

    for i in 0..total {
        let t = i as f64 / sample_rate as f64;
        let frac = i as f64 / total as f64;
        let sample: f64 = match kind {
            SfxKind::UiConfirm => {
                // Short sine beep with fast decay.
                let freq = 880.0 + rng.next_below(440) as f64;
                let env = (1.0 - frac).max(0.0);
                (t * freq * std::f64::consts::TAU).sin() * env * 0.5
            }
            SfxKind::Alarm => {
                // Alternating tone square wave.
                let freq = if (i / 22050) % 2 == 0 { 440.0 } else { 880.0 };
                let phase = (t * freq) % 1.0;
                let env = 1.0 - frac * 0.3;
                if phase < 0.5 { env * 0.5 } else { -env * 0.5 }
            }
            SfxKind::DoorOpen => {
                // Rising pitch with noise burst.
                let freq = 100.0 + (200.0 * frac);
                let noise = (rng.next_u64() as f64 / u64::MAX as f64) * 0.1;
                (t * freq * std::f64::consts::TAU).sin() * (1.0 - frac) * 0.4 + noise
            }
            SfxKind::DoorClose => {
                // Falling pitch with thud.
                let freq = 300.0 - (150.0 * frac);
                let env = (1.0 - frac).max(0.0).powf(0.5);
                (t * freq * std::f64::consts::TAU).sin() * env * 0.4
            }
            SfxKind::ArtifactPulse => {
                // Resonant sine with reverb-like echo (simple delay).
                let freq = 220.0 + (frac * 440.0);
                let mut s = (t * freq * std::f64::consts::TAU).sin();
                if i > 2000 {
                    s += 0.3 * (t * freq * std::f64::consts::TAU - 0.02).sin();
                }
                let env = ((1.0 - frac) * std::f64::consts::PI).sin().max(0.0);
                s * env * 0.4
            }
            SfxKind::EngineHum => {
                // Low rumble with harmonic.
                let s1 = (t * 55.0 * std::f64::consts::TAU).sin() * 0.4;
                let s2 = (t * 110.0 * std::f64::consts::TAU).sin() * 0.2;
                let noise = (rng.next_u64() as f64 / u64::MAX as f64) * 0.05;
                s1 + s2 + noise
            }
            SfxKind::WeaponFire => {
                // Sharp impulse with noise decay.
                let noise = (rng.next_u64() as f64 / u64::MAX as f64) * 2.0 - 1.0;
                let env = (1.0 - frac * 4.0).max(0.0);
                noise * env * 0.6
            }
            SfxKind::Explosion => {
                // Low rumble with noise burst, decaying.
                let noise = (rng.next_u64() as f64 / u64::MAX as f64) * 2.0 - 1.0;
                let low = (t * 40.0 * std::f64::consts::TAU).sin() * 0.3;
                let env = (1.0 - frac).max(0.0).powf(0.3);
                (noise * 0.5 + low) * env * 0.6
            }
            SfxKind::Footstep => {
                // Short impulse.
                let noise = (rng.next_u64() as f64 / u64::MAX as f64) * 2.0 - 1.0;
                let env = (1.0 - frac * 3.0).max(0.0);
                noise * env * 0.5
            }
            SfxKind::SignalPulse => {
                // Rising pitch with echo — a "transmission" sound.
                let freq = 400.0 + (frac * 1200.0);
                let s = (t * freq * std::f64::consts::TAU).sin();
                let echo = if i > total / 2 {
                    0.3 * (t * freq * std::f64::consts::TAU - 0.015).sin()
                } else {
                    0.0
                };
                let env = (1.0 - frac).max(0.0);
                (s + echo) * env * 0.4
            }
        };
        // Clamp to i16 range.
        let clamped = sample.clamp(-1.0, 1.0);
        samples.push((clamped * i16::MAX as f64) as i16);
    }

    GeneratedAudio { sample_rate, samples }
}

/// A triggered SFX event — queued by game systems and drained by the client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SfxEvent {
    pub kind: SfxKind,
    pub seed: u64,
    pub gain: f32,
}

impl SfxEvent {
    pub fn new(kind: SfxKind, seed: u64) -> Self {
        SfxEvent { kind, seed, gain: 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_sfx() {
        let a = generate_sfx(42, SfxKind::Alarm);
        let b = generate_sfx(42, SfxKind::Alarm);
        assert_eq!(a, b);
    }

    #[test]
    fn different_kinds_differ() {
        let a = generate_sfx(0, SfxKind::UiConfirm);
        let b = generate_sfx(0, SfxKind::Alarm);
        assert_ne!(a, b);
    }

    #[test]
    fn sample_count_matches_kind() {
        for kind in &[
            SfxKind::UiConfirm, SfxKind::Alarm, SfxKind::DoorOpen,
            SfxKind::WeaponFire, SfxKind::Footstep, SfxKind::Explosion,
            SfxKind::SignalPulse,
        ] {
            let audio = generate_sfx(1, *kind);
            assert_eq!(audio.samples.len(), kind.sample_count());
        }
    }
}
