//! Agent layer (S101).
//!
//! Provider abstraction now; tool registry, MCP frontend, and the Plan/Build
//! loop land on top of it. The pre-S101 one-shot generation path in
//! [`crate::ai`] is untouched and stays the offline fallback: iron rule 6 says
//! offline is first-class, and a local model with no tool-calling support must
//! still be able to author a document.

pub mod mode;
pub mod provider;
pub mod tools;

use std::path::PathBuf;

use provider::{Caps, Provider};

/// Which wire format a profile speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ProviderKind {
    /// Anything speaking `/v1/chat/completions` — Ollama, llama.cpp, vLLM,
    /// LM Studio, OpenRouter, OpenAI. Local and cloud alike.
    OpenAiCompatible,
    /// Anthropic's Messages API.
    Anthropic,
}

impl ProviderKind {
    pub fn label(&self) -> &'static str {
        match self {
            ProviderKind::OpenAiCompatible => "OpenAI-compatible",
            ProviderKind::Anthropic => "Anthropic",
        }
    }
}

/// One named endpoint the author can switch to.
///
/// `vision` and `tools` are declared per profile rather than sniffed: there is
/// no reliable capability probe across these endpoints, several local servers
/// hard-error on an unrecognised request field rather than ignoring it, and a
/// wrong guess turns every request into a failure instead of a degradation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProviderProfile {
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    #[serde(default)]
    pub vision: bool,
    #[serde(default)]
    pub tools: bool,
}

impl ProviderProfile {
    /// The offline default: a local Ollama with neither capability claimed.
    pub fn local_default() -> Self {
        ProviderProfile {
            name: "Local (Ollama)".into(),
            kind: ProviderKind::OpenAiCompatible,
            base_url: crate::ai::DEFAULT_API_BASE_URL.into(),
            api_key: crate::ai::DEFAULT_API_KEY.into(),
            model: crate::ai::DEFAULT_MODEL.into(),
            max_tokens: crate::ai::DEFAULT_MAX_TOKENS,
            vision: false,
            tools: false,
        }
    }

    pub fn caps(&self) -> Caps {
        Caps {
            vision: self.vision,
            tools: self.tools,
        }
    }

    /// Build a live provider for this profile.
    pub fn build(&self) -> Result<Box<dyn Provider>, String> {
        match self.kind {
            ProviderKind::OpenAiCompatible => Ok(Box::new(provider::openai::OpenAiCompat::new(
                &self.name,
                &self.base_url,
                &self.api_key,
                &self.model,
                self.caps(),
            )?)),
            ProviderKind::Anthropic => Ok(Box::new(provider::anthropic::Anthropic::new(
                &self.name,
                &self.base_url,
                &self.api_key,
                &self.model,
                self.caps(),
            )?)),
        }
    }
}

/// Every configured profile plus which one is live.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AgentConfig {
    pub profiles: Vec<ProviderProfile>,
    /// Index into `profiles`. Out-of-range is clamped on load rather than
    /// panicking — a hand-edited settings file should not take the editor down.
    pub active: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        AgentConfig {
            profiles: vec![ProviderProfile::local_default()],
            active: 0,
        }
    }
}

impl AgentConfig {
    pub fn active(&self) -> &ProviderProfile {
        &self.profiles[self.active.min(self.profiles.len().saturating_sub(1))]
    }

    fn clamp(&mut self) {
        if self.profiles.is_empty() {
            self.profiles.push(ProviderProfile::local_default());
        }
        if self.active >= self.profiles.len() {
            self.active = 0;
        }
    }
}

pub const SETTINGS_PATH: &str = "save/editor-settings.ron";

fn settings_path() -> PathBuf {
    PathBuf::from(SETTINGS_PATH)
}

/// Parse settings text, migrating the pre-S101 single-config shape.
///
/// The old file was a bare `AiConfig` (`api_base_url`, `api_key`, `model`,
/// `max_tokens`). Parsing it as the new shape fails, and the old loader
/// swallowed every error with `unwrap_or_default()` — so without an explicit
/// migration the first launch after this change would silently reset the
/// author's endpoint and API key to the Ollama defaults and then overwrite the
/// file on the next save. Try the new shape, fall back to the old one and wrap
/// it as the first profile, and only then give up to defaults.
///
/// Split from [`load_config`] so the migration can be tested on its own: the
/// settings path is relative to the process cwd, which under
/// `cargo test -p reachlock-editor` is the crate directory rather than the
/// workspace root — a test that went through the filesystem would silently
/// exercise a missing file instead of the migration.
pub fn parse_config(text: &str) -> AgentConfig {
    if let Ok(mut cfg) = ron::from_str::<AgentConfig>(text) {
        cfg.clamp();
        return cfg;
    }
    if let Ok(old) = ron::from_str::<crate::ai::AiConfig>(text) {
        tracing::info!("migrating pre-S101 editor settings to a provider profile");
        return AgentConfig {
            profiles: vec![ProviderProfile {
                name: "Migrated".into(),
                kind: ProviderKind::OpenAiCompatible,
                base_url: old.api_base_url,
                api_key: old.api_key,
                model: old.model,
                max_tokens: old.max_tokens,
                vision: false,
                tools: false,
            }],
            active: 0,
        };
    }
    tracing::warn!("editor settings file is unreadable; using defaults");
    AgentConfig::default()
}

/// Read and parse the settings file. A missing file is the first-run case.
pub fn load_config() -> AgentConfig {
    match std::fs::read_to_string(settings_path()) {
        Ok(text) => parse_config(&text),
        Err(_) => AgentConfig::default(),
    }
}

pub fn save_config(config: &AgentConfig) -> std::io::Result<()> {
    if let Some(parent) = settings_path().parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = ron::ser::to_string_pretty(config, ron::ser::PrettyConfig::default())
        .map_err(std::io::Error::other)?;
    std::fs::write(settings_path(), text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact shape the pre-S101 editor wrote. If this stops round-tripping
    /// into a profile, every existing install silently loses its endpoint and
    /// key on first launch.
    #[test]
    fn migrates_the_old_single_config_shape() {
        let old = r#"(
    api_base_url: "https://api.example.com/v1",
    api_key: "sk-secret",
    model: "some-model",
    max_tokens: 8192,
)"#;
        let parsed: crate::ai::AiConfig = ron::from_str(old).expect("old shape still parses");
        assert_eq!(parsed.api_base_url, "https://api.example.com/v1");
        assert_eq!(parsed.api_key, "sk-secret");
        assert_eq!(parsed.max_tokens, 8192);
    }

    /// A real pre-S101 file must come back as a profile with the author's
    /// endpoint and key intact — not as the Ollama defaults.
    #[test]
    fn migration_preserves_the_authors_endpoint_and_key() {
        let cfg = parse_config(
            r#"(
    api_base_url: "https://my-private-endpoint.example/v1",
    api_key: "sk-do-not-lose-me",
    model: "my-tuned-model",
    max_tokens: 8192,
)"#,
        );
        assert_eq!(cfg.profiles.len(), 1);
        let p = cfg.active();
        assert_eq!(p.base_url, "https://my-private-endpoint.example/v1");
        assert_eq!(p.api_key, "sk-do-not-lose-me");
        assert_eq!(p.model, "my-tuned-model");
        assert_eq!(p.max_tokens, 8192);
    }

    /// Garbage must not masquerade as a successful migration that silently
    /// swaps in defaults — but it must also not take the editor down.
    #[test]
    fn unparseable_settings_fall_back_to_defaults() {
        let cfg = parse_config("this is not RON at all {{{");
        assert_eq!(cfg, AgentConfig::default());
    }

    #[test]
    fn new_shape_round_trips() {
        let cfg = AgentConfig {
            profiles: vec![ProviderProfile::local_default()],
            active: 0,
        };
        let text = ron::ser::to_string_pretty(&cfg, ron::ser::PrettyConfig::default()).unwrap();
        let back: AgentConfig = ron::from_str(&text).expect("round trip");
        assert_eq!(back, cfg);
    }

    /// A hand-edited `active` past the end of the list must not panic the
    /// editor on startup.
    #[test]
    fn out_of_range_active_is_clamped() {
        let mut cfg = AgentConfig {
            profiles: vec![ProviderProfile::local_default()],
            active: 7,
        };
        cfg.clamp();
        assert_eq!(cfg.active, 0);
        assert_eq!(cfg.active().name, "Local (Ollama)");
    }

    #[test]
    fn an_empty_profile_list_is_repaired() {
        let mut cfg = AgentConfig {
            profiles: vec![],
            active: 0,
        };
        cfg.clamp();
        assert_eq!(cfg.profiles.len(), 1);
    }
}
