//! Persisted user settings (`settings.json` in the data directory).

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const SETTINGS_FILENAME: &str = "settings.json";

/// Configurable keyboard shortcuts, stored as egui key names
/// (e.g. "Space", "Enter", "ArrowRight", "K").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shortcuts {
    pub review_reveal: String,
    pub review_correct: String,
    pub review_incorrect: String,
    pub reader_next: String,
    pub reader_prev: String,
    pub reader_learn: String,
    pub reader_known: String,
    pub reader_ignore: String,
    pub reader_explain: String,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            review_reveal: "Space".into(),
            review_correct: "ArrowRight".into(),
            review_incorrect: "ArrowLeft".into(),
            reader_next: "ArrowRight".into(),
            reader_prev: "ArrowLeft".into(),
            reader_learn: "L".into(),
            reader_known: "K".into(),
            reader_ignore: "I".into(),
            reader_explain: "E".into(),
        }
    }
}

/// Color theme choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub theme: Theme,
    pub shortcuts: Shortcuts,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            anthropic_api_key: String::new(),
            llm_model: "claude-opus-4-8".to_string(),
            show_unknown_highlights: false,
            onboarded: false,
            theme: Theme::Dark,
            shortcuts: Shortcuts::default(),
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

/// Parse a key name leniently ("l" works as well as "L").
fn parse_key(name: &str) -> Option<eframe::egui::Key> {
    let trimmed = name.trim();
    eframe::egui::Key::from_name(trimmed)
        .or_else(|| eframe::egui::Key::from_name(&trimmed.to_uppercase()))
}

/// Whether the named shortcut was pressed this frame, ignoring keypresses
/// while a text field has focus.
pub fn shortcut_pressed(ctx: &eframe::egui::Context, name: &str) -> bool {
    let Some(key) = parse_key(name) else {
        return false;
    };
    if ctx.memory(|m| m.focused().is_some()) {
        return false;
    }
    ctx.input(|i| i.key_pressed(key))
}

/// Whether a shortcut name is a valid egui key name (for settings UI
/// validation).
pub fn is_valid_key_name(name: &str) -> bool {
    parse_key(name).is_some()
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
            theme: Theme::Light,
            shortcuts: Shortcuts {
                review_correct: "Enter".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        s.save(&dir).unwrap();

        let loaded = Settings::load(&dir);
        assert_eq!(loaded.anthropic_api_key, "sk-test");
        assert!(loaded.onboarded);
        assert!(loaded.show_unknown_highlights);
        assert_eq!(loaded.theme, Theme::Light);
        assert_eq!(loaded.shortcuts.review_correct, "Enter");
        assert_eq!(loaded.shortcuts.review_reveal, "Space");
        assert_eq!(loaded.llm_model, "claude-opus-4-8");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_or_corrupt_file_yields_defaults() {
        let dir = std::env::temp_dir().join("jrc-settings-test-missing");
        let s = Settings::load(&dir);
        assert!(!s.onboarded);
        assert!(s.anthropic_api_key.is_empty());
        assert_eq!(s.theme, Theme::Dark);
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

    #[test]
    fn key_name_validation() {
        for name in ["Space", "Enter", "ArrowRight", "A", "l"] {
            assert!(is_valid_key_name(name), "{name} should be valid");
        }
        assert!(!is_valid_key_name("NotAKey"));
        assert!(!is_valid_key_name(""));
    }
}
