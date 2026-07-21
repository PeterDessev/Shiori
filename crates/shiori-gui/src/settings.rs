//! Persisted user settings (`settings.json` in the data directory).

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const SETTINGS_FILENAME: &str = "settings.json";

/// Configurable keyboard shortcuts, stored as strings like "K",
/// "Space", or "Ctrl+Shift+E" (modifiers in Ctrl, Alt, Shift order).
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
    pub reader_away: String,
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
            reader_away: "P".into(),
        }
    }
}

/// Identifies one rebindable action (one field of [`Shortcuts`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutId {
    ReviewReveal,
    ReviewCorrect,
    ReviewIncorrect,
    ReaderNext,
    ReaderPrev,
    ReaderLearn,
    ReaderKnown,
    ReaderIgnore,
    ReaderExplain,
    ReaderAway,
}

impl Shortcuts {
    /// Every rebindable action with its settings-page label.
    pub const FIELDS: [(ShortcutId, &'static str); 10] = [
        (ShortcutId::ReviewReveal, "Review · show answer"),
        (ShortcutId::ReviewCorrect, "Review · correct"),
        (ShortcutId::ReviewIncorrect, "Review · incorrect"),
        (ShortcutId::ReaderNext, "Reader · next word"),
        (ShortcutId::ReaderPrev, "Reader · previous word"),
        (ShortcutId::ReaderLearn, "Reader · learn word"),
        (ShortcutId::ReaderKnown, "Reader · mark known"),
        (ShortcutId::ReaderIgnore, "Reader · ignore word"),
        (ShortcutId::ReaderExplain, "Reader · explain sentence"),
        (ShortcutId::ReaderAway, "Reader · pause reading"),
    ];

    pub fn get(&self, id: ShortcutId) -> &str {
        match id {
            ShortcutId::ReviewReveal => &self.review_reveal,
            ShortcutId::ReviewCorrect => &self.review_correct,
            ShortcutId::ReviewIncorrect => &self.review_incorrect,
            ShortcutId::ReaderNext => &self.reader_next,
            ShortcutId::ReaderPrev => &self.reader_prev,
            ShortcutId::ReaderLearn => &self.reader_learn,
            ShortcutId::ReaderKnown => &self.reader_known,
            ShortcutId::ReaderIgnore => &self.reader_ignore,
            ShortcutId::ReaderExplain => &self.reader_explain,
            ShortcutId::ReaderAway => &self.reader_away,
        }
    }

    pub fn get_mut(&mut self, id: ShortcutId) -> &mut String {
        match id {
            ShortcutId::ReviewReveal => &mut self.review_reveal,
            ShortcutId::ReviewCorrect => &mut self.review_correct,
            ShortcutId::ReviewIncorrect => &mut self.review_incorrect,
            ShortcutId::ReaderNext => &mut self.reader_next,
            ShortcutId::ReaderPrev => &mut self.reader_prev,
            ShortcutId::ReaderLearn => &mut self.reader_learn,
            ShortcutId::ReaderKnown => &mut self.reader_known,
            ShortcutId::ReaderIgnore => &mut self.reader_ignore,
            ShortcutId::ReaderExplain => &mut self.reader_explain,
            ShortcutId::ReaderAway => &mut self.reader_away,
        }
    }

    /// The label of a binding that already uses `combo`, excluding `except`.
    pub fn conflict(&self, combo: &str, except: ShortcutId) -> Option<&'static str> {
        let target = parse_shortcut(combo)?;
        Self::FIELDS
            .iter()
            .filter(|(id, _)| *id != except)
            .find(|(id, _)| parse_shortcut(self.get(*id)) == Some(target))
            .map(|(_, label)| *label)
    }
}

/// Color theme choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
    /// Warm paper tones for long reading sessions.
    Sepia,
}

/// Which LLM backend powers explanations and production practice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    Anthropic,
    /// Local models via Ollama.
    Ollama,
    /// Any OpenAI-compatible endpoint (LM Studio, llama.cpp, vLLM, …).
    Custom,
}

/// When the reader shows furigana over kanji words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FuriganaMode {
    /// Never.
    None,
    /// Over every word still at unknown status.
    Unknown,
    /// Over the first X instances (in document order, per book) of each
    /// unknown word; later instances stand on their own.
    UnknownFirstX,
    /// Over everything containing kanji.
    All,
}

impl FuriganaMode {
    pub fn label(self) -> &'static str {
        match self {
            FuriganaMode::None => "None",
            FuriganaMode::Unknown => "Unknown words",
            FuriganaMode::UnknownFirstX => "Unknown words, first X instances",
            FuriganaMode::All => "All words",
        }
    }
}

/// Which Japanese font renders the app's CJK text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderFont {
    /// Whatever the operating system provides (Meiryo on Windows).
    System,
    /// Noto Sans JP (gothic) — downloaded on first use.
    NotoSans,
    /// Noto Serif JP (mincho) — downloaded on first use.
    NotoSerif,
}

impl ReaderFont {
    pub fn label(self) -> &'static str {
        match self {
            ReaderFont::System => "System (gothic)",
            ReaderFont::NotoSans => "Noto Sans JP (gothic)",
            ReaderFont::NotoSerif => "Noto Serif JP (mincho)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub llm_provider: LlmProvider,
    /// Anthropic API key for the optional LLM features. Stored locally,
    /// never sent anywhere but the Anthropic API. Falls back to the
    /// ANTHROPIC_API_KEY environment variable when empty.
    pub anthropic_api_key: String,
    /// Model id for the Anthropic backend.
    pub llm_model: String,
    /// Ollama server URL; empty means the default localhost:11434.
    pub ollama_url: String,
    /// Local model to use with Ollama (e.g. "qwen3:8b").
    pub ollama_model: String,
    /// OpenAI-compatible base URL up to /v1 (e.g. http://localhost:1234/v1).
    pub custom_url: String,
    pub custom_api_key: String,
    pub custom_model: String,
    /// Tint unknown words in the reader (off by default; the selection
    /// highlight is always on).
    pub show_unknown_highlights: bool,
    /// Whether the getting-started page has been dismissed.
    pub onboarded: bool,
    pub theme: Theme,
    pub reader_font: ReaderFont,
    /// Reader text size in points.
    pub reader_font_size: f32,
    /// Multiplier on the reader's line and paragraph gaps.
    pub reader_line_spacing: f32,
    pub furigana: FuriganaMode,
    /// X for [`FuriganaMode::UnknownFirstX`].
    pub furigana_first_x: u32,
    /// Show example sentences from other books on review cards.
    pub review_examples: bool,
    /// Show IPA pronunciation with dictionary entries (built packs
    /// carry it; off by default).
    pub show_ipa: bool,
    /// How hard the chat partner's Japanese pushes the user.
    pub chat_challenge: ChatChallenge,
    /// Language the app operates in ("ja", "grc", …); languages beyond
    /// Japanese come from packs in `<data>/packs/`.
    pub active_language: String,
    /// Where the browsable pack catalog is fetched from; empty means
    /// the default hosted catalog.
    pub pack_catalog_url: String,
    /// Per-language LLM model overrides: a local model that handles
    /// Japanese fine may write terrible Koine, so each language can pin
    /// its own model. Empty = use the provider's configured model.
    pub language_models: std::collections::HashMap<String, String>,
    /// User-added OPDS catalog feeds, keyed by language code. Book search
    /// pulls from these distributors alongside Wikisource and Project
    /// Gutenberg for the active language.
    pub opds_catalogs: std::collections::HashMap<String, Vec<OpdsCatalog>>,
    pub shortcuts: Shortcuts,
}

/// One user-added OPDS catalog feed (an [OPDS] distributor). `url` is the
/// root feed; searching follows the feed's advertised search link when it
/// has one, else filters the feed's own entries.
///
/// [OPDS]: https://opds.io/
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpdsCatalog {
    pub name: String,
    pub url: String,
}

/// Persisted form of the production-chat challenge dial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatChallenge {
    Match,
    Push,
    Immerse,
}

impl ChatChallenge {
    pub fn label(self) -> &'static str {
        match self {
            ChatChallenge::Match => "Match my level",
            ChatChallenge::Push => "Push me a little",
            ChatChallenge::Immerse => "Full immersion",
        }
    }

    pub fn to_llm(self) -> shiori_llm::Challenge {
        match self {
            ChatChallenge::Match => shiori_llm::Challenge::Match,
            ChatChallenge::Push => shiori_llm::Challenge::Push,
            ChatChallenge::Immerse => shiori_llm::Challenge::Immerse,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            llm_provider: LlmProvider::Anthropic,
            anthropic_api_key: String::new(),
            llm_model: "claude-opus-4-8".to_string(),
            ollama_url: String::new(),
            ollama_model: String::new(),
            custom_url: String::new(),
            custom_api_key: String::new(),
            custom_model: String::new(),
            show_unknown_highlights: false,
            onboarded: false,
            theme: Theme::Dark,
            reader_font: ReaderFont::System,
            reader_font_size: 21.0,
            reader_line_spacing: 1.0,
            furigana: FuriganaMode::None,
            furigana_first_x: 3,
            review_examples: true,
            show_ipa: false,
            chat_challenge: ChatChallenge::Push,
            active_language: "ja".to_string(),
            pack_catalog_url: String::new(),
            language_models: Default::default(),
            opds_catalogs: Default::default(),
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
        self.save_to(&data_dir.join(SETTINGS_FILENAME))
    }

    /// Write the settings to an arbitrary path (exports).
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// Read settings from an arbitrary path (imports); `None` when the
    /// file isn't a settings export.
    pub fn load_from(path: &Path) -> Option<Self> {
        let json = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&json).ok()
    }

    /// The OPDS distributors the user has added for `lang`.
    pub fn opds_for(&self, lang: &str) -> &[OpdsCatalog] {
        self.opds_catalogs
            .get(lang)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Add an OPDS distributor for `lang`, ignoring blank entries and
    /// exact duplicates (same URL, case-insensitive). Returns whether it
    /// was added.
    pub fn add_opds(&mut self, lang: &str, name: &str, url: &str) -> bool {
        let name = name.trim();
        let url = url.trim();
        if url.is_empty() {
            return false;
        }
        let list = self.opds_catalogs.entry(lang.to_string()).or_default();
        if list.iter().any(|c| c.url.eq_ignore_ascii_case(url)) {
            return false;
        }
        list.push(OpdsCatalog {
            name: if name.is_empty() {
                url.to_string()
            } else {
                name.to_string()
            },
            url: url.to_string(),
        });
        true
    }

    /// Remove the OPDS distributor at `idx` for `lang`.
    pub fn remove_opds(&mut self, lang: &str, idx: usize) {
        if let Some(list) = self.opds_catalogs.get_mut(lang) {
            if idx < list.len() {
                list.remove(idx);
            }
            if list.is_empty() {
                self.opds_catalogs.remove(lang);
            }
        }
    }

    /// The model to use right now: the active language's override when
    /// one is set, otherwise `base`.
    fn model_for_active_language(&self, base: &str) -> String {
        self.language_models
            .get(&self.active_language)
            .map(|m| m.trim())
            .filter(|m| !m.is_empty())
            .unwrap_or(base.trim())
            .to_string()
    }

    /// Build the LLM backend this configuration describes, honoring the
    /// active language's model override.
    pub fn build_explainer(&self) -> std::sync::Arc<dyn shiori_llm::Explainer> {
        match self.llm_provider {
            LlmProvider::Anthropic => {
                let key = self.anthropic_api_key.trim();
                if key.is_empty() {
                    std::sync::Arc::from(shiori_llm::explainer_from_env())
                } else {
                    let base = if self.llm_model.trim().is_empty() {
                        Settings::default().llm_model
                    } else {
                        self.llm_model.trim().to_string()
                    };
                    let model = self.model_for_active_language(&base);
                    std::sync::Arc::new(shiori_llm::AnthropicExplainer::with_model(key, model))
                }
            }
            LlmProvider::Ollama => std::sync::Arc::new(shiori_llm::OllamaExplainer::new(
                self.ollama_url.clone(),
                self.model_for_active_language(&self.ollama_model),
            )),
            LlmProvider::Custom => std::sync::Arc::new(shiori_llm::OpenAiCompatExplainer::new(
                self.custom_url.trim(),
                self.custom_api_key.trim(),
                self.model_for_active_language(&self.custom_model),
            )),
        }
    }
}

/// Parse a key name leniently ("l" works as well as "L").
fn parse_key(name: &str) -> Option<eframe::egui::Key> {
    let trimmed = name.trim();
    eframe::egui::Key::from_name(trimmed)
        .or_else(|| eframe::egui::Key::from_name(&trimmed.to_uppercase()))
}

/// A parsed shortcut: modifier set plus one non-modifier key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: eframe::egui::Key,
}

/// Parse "Ctrl+Shift+K"-style combos. Escape is reserved (cancel) and
/// never parses as a binding.
pub fn parse_shortcut(name: &str) -> Option<Shortcut> {
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut key = None;
    let mut parts = name.split('+').peekable();
    while let Some(part) = parts.next() {
        let part = part.trim();
        if parts.peek().is_some() {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => ctrl = true,
                "alt" => alt = true,
                "shift" => shift = true,
                _ => return None,
            }
        } else {
            key = parse_key(part);
        }
    }
    let key = key?;
    if key == eframe::egui::Key::Escape {
        return None;
    }
    Some(Shortcut {
        ctrl,
        alt,
        shift,
        key,
    })
}

/// Canonical display/storage form of a combo: "Ctrl+Alt+Shift+Key".
pub fn format_shortcut(modifiers: eframe::egui::Modifiers, key: eframe::egui::Key) -> String {
    let mut out = String::new();
    if modifiers.ctrl || modifiers.command {
        out.push_str("Ctrl+");
    }
    if modifiers.alt {
        out.push_str("Alt+");
    }
    if modifiers.shift {
        out.push_str("Shift+");
    }
    out.push_str(key.name());
    out
}

/// Whether the named shortcut was pressed this frame, ignoring keypresses
/// while a text field has focus. Modifiers must match exactly, so "K"
/// does not fire while Ctrl is held.
pub fn shortcut_pressed(ctx: &eframe::egui::Context, name: &str) -> bool {
    let Some(sc) = parse_shortcut(name) else {
        return false;
    };
    if ctx.memory(|m| m.focused().is_some()) {
        return false;
    }
    ctx.input(|i| {
        i.key_pressed(sc.key)
            && (i.modifiers.ctrl || i.modifiers.command) == sc.ctrl
            && i.modifiers.alt == sc.alt
            && i.modifiers.shift == sc.shift
    })
}

/// Whether a combo string is a valid binding (for settings UI validation).
pub fn is_valid_key_name(name: &str) -> bool {
    parse_shortcut(name).is_some()
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
    fn opds_catalogs_round_trip_and_dedupe() {
        let dir = std::env::temp_dir().join("jrc-settings-opds-test");
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Settings::default();
        assert!(s.add_opds("en", "Standard Ebooks", "https://standardebooks.org/feeds/opds"));
        // Blank url rejected; duplicate url (case-insensitive) rejected.
        assert!(!s.add_opds("en", "blank", "   "));
        assert!(!s.add_opds("en", "dup", "HTTPS://STANDARDEBOOKS.ORG/feeds/opds"));
        assert!(s.add_opds("fr", "Ebooks BNR", "https://ebooks-bnr.com/opds/"));
        assert_eq!(s.opds_for("en").len(), 1);
        assert_eq!(s.opds_for("fr").len(), 1);
        assert!(s.opds_for("de").is_empty());
        s.save(&dir).unwrap();

        let loaded = Settings::load(&dir);
        assert_eq!(loaded.opds_for("en")[0].name, "Standard Ebooks");
        assert_eq!(
            loaded.opds_for("fr")[0].url,
            "https://ebooks-bnr.com/opds/"
        );

        let mut loaded = loaded;
        loaded.remove_opds("en", 0);
        assert!(loaded.opds_for("en").is_empty());
        assert!(!loaded.opds_catalogs.contains_key("en"));
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
        for name in [
            "Space",
            "Enter",
            "ArrowRight",
            "A",
            "l",
            "Ctrl+E",
            "ctrl + shift + 4",
        ] {
            assert!(is_valid_key_name(name), "{name} should be valid");
        }
        for name in ["NotAKey", "", "Escape", "Ctrl+Escape", "Meta+K", "Ctrl+"] {
            assert!(!is_valid_key_name(name), "{name} should be invalid");
        }
    }

    #[test]
    fn shortcut_parse_and_format_roundtrip() {
        use eframe::egui::{Key, Modifiers};
        let sc = parse_shortcut("Ctrl+Shift+K").unwrap();
        assert!(sc.ctrl && sc.shift && !sc.alt);
        assert_eq!(sc.key, Key::K);

        let formatted = format_shortcut(
            Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
            Key::K,
        );
        assert_eq!(formatted, "Ctrl+Shift+K");
        assert_eq!(parse_shortcut(&formatted), Some(sc));

        // Plain keys still parse (backward compatible with old settings).
        assert_eq!(parse_shortcut("Space").unwrap().key, Key::Space);
        // command (cmd/win) is treated as ctrl when formatting.
        let cmd = format_shortcut(
            Modifiers {
                command: true,
                ..Default::default()
            },
            Key::E,
        );
        assert_eq!(cmd, "Ctrl+E");
    }

    #[test]
    fn conflict_detection() {
        let sc = Shortcuts::default();
        // ArrowRight is used by both review_correct and reader_next, but
        // conflict() only reports other fields.
        assert_eq!(
            sc.conflict("ArrowRight", ShortcutId::ReaderNext),
            Some("Review · correct")
        );
        // Lenient spelling still collides with the canonical form.
        assert_eq!(
            sc.conflict("ctrl+L", ShortcutId::ReaderLearn),
            None,
            "Ctrl+L differs from plain L"
        );
        assert_eq!(
            sc.conflict("l", ShortcutId::ReviewReveal),
            Some("Reader · learn word")
        );
        assert_eq!(sc.conflict("Ctrl+Shift+9", ShortcutId::ReaderLearn), None);
    }
}
