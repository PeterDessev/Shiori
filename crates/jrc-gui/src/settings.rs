//! Persisted user settings (`settings.json` in the data directory).

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const SETTINGS_FILENAME: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Anthropic API key for the optional LLM features. Stored locally,
    /// never sent anywhere but the Anthropic API. Falls back to the
    /// ANTHROPIC_API_KEY environment variable when empty.
    pub anthropic_api_key: String,
    /// Model id for the LLM backend.
    pub llm_model: String,
    /// Tint unknown words in the reader (off by default; the selection
    /// highlight is always on).
    pub show_unknown_highlights: bool,
    /// Whether the getting-started page has been dismissed.
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            anthropic_api_key: String::new(),
            llm_model: "claude-opus-4-8".to_string(),
            show_unknown_highlights: false,
            onboarded: false,
        }
    }
}

impl Settings {
    pub fn load(data_dir: &Path) -> Self {
        std::fs::read_to_string(data_dir.join(SETTINGS_FILENAME))
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(data_dir.join(SETTINGS_FILENAME), json)
    }

    /// Build the LLM backend this configuration describes.
    pub fn build_explainer(&self) -> std::sync::Arc<dyn jrc_llm::Explainer> {
        let key = self.anthropic_api_key.trim();
        if key.is_empty() {
            std::sync::Arc::from(jrc_llm::explainer_from_env())
        } else {
            let model = if self.llm_model.trim().is_empty() {
                Settings::default().llm_model
            } else {
                self.llm_model.trim().to_string()
            };
            std::sync::Arc::new(jrc_llm::AnthropicExplainer::with_model(key, model))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_disk() {
        let dir = std::env::temp_dir().join("jrc-settings-test");
        std::fs::create_dir_all(&dir).unwrap();
        let s = Settings {
            anthropic_api_key: "sk-test".into(),
            onboarded: true,
            show_unknown_highlights: true,
            ..Default::default()
        };
        s.save(&dir).unwrap();

        let loaded = Settings::load(&dir);
        assert_eq!(loaded.anthropic_api_key, "sk-test");
        assert!(loaded.onboarded);
        assert!(loaded.show_unknown_highlights);
        assert_eq!(loaded.llm_model, "claude-opus-4-8");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_or_corrupt_file_yields_defaults() {
        let dir = std::env::temp_dir().join("jrc-settings-test-missing");
        let s = Settings::load(&dir);
        assert!(!s.onboarded);
        assert!(s.anthropic_api_key.is_empty());
    }

    #[test]
    fn explainer_uses_key_when_set() {
        let mut s = Settings::default();
        assert!(!s.build_explainer().is_available() || std::env::var("ANTHROPIC_API_KEY").is_ok());
        s.anthropic_api_key = "sk-test".into();
        let backend = s.build_explainer();
        assert!(backend.is_available());
        assert_eq!(backend.name(), "Anthropic");
    }
}
