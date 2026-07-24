use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceParams {
    pub pitch: u16,
    pub speed: u16,
    pub accent: String,
}

impl Default for VoiceParams {
    fn default() -> Self {
        VoiceParams {
            pitch: 512,
            speed: 512,
            accent: "en-us".into(),
        }
    }
}

pub struct SpeechSynthesizer;

impl SpeechSynthesizer {
    pub fn synthesize(text: &str, params: &VoiceParams, _seed: u64) -> (Vec<f32>, u32) {
        let sample_rate = 22050;
        let base_freq = 80.0 + (params.pitch as f32 / 1024.0) * 160.0;
        let speed_factor = 0.05 + (params.speed as f32 / 1024.0) * 0.15;
        let mut samples = Vec::new();

        for ch in text.chars() {
            if ch.is_whitespace() {
                let gap = (sample_rate as f32 * 0.05) as usize;
                samples.extend(std::iter::repeat_n(0.0f32, gap));
                continue;
            }
            let char_freq = base_freq + (ch as u32 % 24) as f32 * 10.0;
            let dur = (sample_rate as f32 * speed_factor) as usize;
            for t in 0..dur {
                let phase =
                    2.0 * std::f32::consts::PI * char_freq * t as f32 / sample_rate as f32;
                let amp = 0.3 * (1.0 - t as f32 / dur as f32);
                samples.push(phase.sin() * amp);
            }
        }

        (samples, sample_rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_synthesis() {
        let params = VoiceParams::default();
        let (a, _) = SpeechSynthesizer::synthesize("hello world", &params, 42);
        let (b, _) = SpeechSynthesizer::synthesize("hello world", &params, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn different_text_differs() {
        let params = VoiceParams::default();
        let (a, _) = SpeechSynthesizer::synthesize("hello", &params, 0);
        let (b, _) = SpeechSynthesizer::synthesize("world", &params, 0);
        assert_ne!(a, b);
    }

    #[test]
    fn produces_output() {
        let params = VoiceParams::default();
        let (samples, rate) = SpeechSynthesizer::synthesize("test", &params, 99);
        assert!(!samples.is_empty());
        assert_eq!(rate, 22050);
    }
}
