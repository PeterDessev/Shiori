//! The pack manifest: everything about a language that is configuration
//! rather than code.

use std::path::Path;

use serde::Deserialize;
use shiori_lang::{ExtractProfile, PromptProfile};

use crate::{PackError, Result};

/// `manifest.toml`, deserialized.
///
/// Unknown fields are ignored so newer packs load in older apps; the
/// reserved capabilities (`dir`, `diacritic_layers`, sub-token
/// segmentation) are declared here before any engine implements them so
/// the format never needs a breaking freeze.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    /// Manifest schema version; this loader accepts `1`.
    pub schema: u32,
    /// BCP-47-ish code: "grc", "es".
    pub lang: String,
    /// English display name: "Koine Greek".
    pub name: String,
    /// Dictionary source key for `dict_entries` (e.g. "grc-pack").
    pub dict_source: String,
    /// SPDX-ish license summary for the pack data, shown in the UI.
    #[serde(default)]
    pub license: String,
    /// Token joiner for reconstructing text ("" for scriptio continua).
    #[serde(default = "default_joiner")]
    pub joiner: String,
    /// Sentence-ending characters for the rule tokenizer.
    #[serde(default)]
    pub sentence_enders: Vec<String>,
    /// Unicode codepoint ranges (inclusive) counting as target-language
    /// script, e.g. `[[0x0370, 0x03FF], [0x1F00, 0x1FFF]]`.
    #[serde(default)]
    pub script_ranges: Vec<(u32, u32)>,
    /// Text direction; only "ltr" is implemented today. Reserved so RTL
    /// packs can declare themselves before the engine exists.
    #[serde(default = "default_dir")]
    pub dir: String,
    /// Graded-vocabulary scheme this pack ships (matches graded.tsv).
    #[serde(default)]
    pub graded_scheme: Option<GradedScheme>,
    /// Fonts to download for this language's script.
    #[serde(default)]
    pub fonts: Vec<FontSpec>,
    pub prompt: PromptSection,
    #[serde(default)]
    pub extract: ExtractSection,
}

fn default_joiner() -> String {
    " ".into()
}

fn default_dir() -> String {
    "ltr".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct GradedScheme {
    /// Scheme key in `graded_vocab` (e.g. "gnt-frequency").
    pub key: String,
    /// Display name for stats ("GNT frequency tier").
    pub display: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FontSpec {
    /// Family name registered with the GUI ("Gentium Plus").
    pub name: String,
    /// Cache filename inside `<data_dir>/fonts/`.
    pub file: String,
    /// Download URL (OFL or similarly redistributable fonts only).
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PromptSection {
    pub language_name: String,
    pub chat_persona: String,
    #[serde(default)]
    pub citation_guidance: String,
    #[serde(default)]
    pub grammar_skeleton: String,
    #[serde(default = "default_quote_open")]
    pub quote_open: String,
    #[serde(default = "default_quote_close")]
    pub quote_close: String,
    pub immerse_instruction: String,
    #[serde(default = "default_authority")]
    pub unnatural_authority: String,
    #[serde(default)]
    pub synthetic_disclaimer: Option<String>,
}

fn default_quote_open() -> String {
    "\u{2018}".into()
}

fn default_quote_close() -> String {
    "\u{2019}".into()
}

fn default_authority() -> String {
    "phrasing a native speaker would not use".into()
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExtractSection {
    #[serde(default)]
    pub legacy_encodings: Vec<String>,
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        Self::parse(&raw)
    }

    pub fn parse(raw: &str) -> Result<Self> {
        let manifest: Manifest =
            toml::from_str(raw).map_err(|e| PackError::Manifest(e.to_string()))?;
        if manifest.schema != 1 {
            return Err(PackError::Manifest(format!(
                "unsupported manifest schema {} (this build understands 1)",
                manifest.schema
            )));
        }
        if manifest.lang.is_empty() || manifest.dict_source.is_empty() {
            return Err(PackError::Manifest(
                "lang and dict_source must be set".into(),
            ));
        }
        Ok(manifest)
    }

    pub fn prompt_profile(&self) -> PromptProfile {
        PromptProfile {
            language_name: self.prompt.language_name.clone(),
            chat_persona: self.prompt.chat_persona.clone(),
            citation_guidance: self.prompt.citation_guidance.clone(),
            grammar_skeleton: self.prompt.grammar_skeleton.clone(),
            quote_open: self.prompt.quote_open.clone(),
            quote_close: self.prompt.quote_close.clone(),
            immerse_instruction: self.prompt.immerse_instruction.clone(),
            unnatural_authority: self.prompt.unnatural_authority.clone(),
            synthetic_disclaimer: self.prompt.synthetic_disclaimer.clone(),
        }
    }

    pub fn extract_profile(&self) -> ExtractProfile {
        ExtractProfile {
            legacy_encodings: self.extract.legacy_encodings.clone(),
            japanese_conventions: false,
        }
    }
}

/// The bundled Koine Greek manifest, used until packs are downloadable
/// and as the reference for pack authors.
pub const KOINE_GREEK_MANIFEST: &str = r#"
schema = 1
lang = "grc"
name = "Koine Greek"
dict_source = "grc-pack"
license = "Texts and annotations CC BY / CC BY-SA / public domain; see SOURCES.md"
joiner = " "
sentence_enders = [".", ";", "·", "?", "!"]
# Greek & Coptic, Greek Extended.
script_ranges = [[880, 1023], [7936, 8191]]

[graded_scheme]
key = "gnt-frequency"
display = "GNT tier"

[[fonts]]
name = "Gentium Plus"
file = "GentiumPlus-Regular.ttf"
url = "https://github.com/silnrsi/font-gentium/releases/download/v6.200/GentiumPlus-6.200.zip"

[prompt]
language_name = "Koine Greek"
chat_persona = "an educated first-century writer of Koine Greek"
citation_guidance = "When you cite Greek, give it in Greek script followed by a brief English gloss in parentheses where helpful."
grammar_skeleton = "case endings, verb forms (tense, voice, mood), particles, word order"
quote_open = "‘"
quote_close = "’"
immerse_instruction = "Write unrestricted literary Koine as found in the papyri and the New Testament; the user wants full immersion."
unnatural_authority = "phrasing not attested in Koine usage"
synthetic_disclaimer = "Koine Greek has no living native speakers, so you are a synthetic persona: model your Greek on attested usage (the New Testament, the Septuagint, the Apostolic Fathers, documentary papyri) and, when judging the user's Greek, prefer 'not attested in this period' over appeals to native intuition."

[extract]
legacy_encodings = ["iso-8859-7", "windows-1253"]
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn koine_manifest_parses() {
        let m = Manifest::parse(KOINE_GREEK_MANIFEST).unwrap();
        assert_eq!(m.lang, "grc");
        assert_eq!(m.dict_source, "grc-pack");
        assert_eq!(m.joiner, " ");
        assert!(m.sentence_enders.contains(&"·".to_string()));
        assert_eq!(m.script_ranges.len(), 2);
        assert_eq!(m.graded_scheme.as_ref().unwrap().key, "gnt-frequency");
        let p = m.prompt_profile();
        assert_eq!(p.language_name, "Koine Greek");
        assert!(p.synthetic_disclaimer.is_some());
        assert_eq!(m.extract_profile().legacy_encodings.len(), 2);
        assert!(!m.extract_profile().japanese_conventions);
    }

    #[test]
    fn unknown_fields_are_tolerated() {
        let raw = r#"
schema = 1
lang = "xx"
name = "Testish"
dict_source = "xx-pack"
future_field = "ignored"

[prompt]
language_name = "Testish"
chat_persona = "a speaker"
immerse_instruction = "Write Testish."
"#;
        let m = Manifest::parse(raw).unwrap();
        assert_eq!(m.lang, "xx");
        assert_eq!(m.dir, "ltr");
    }

    #[test]
    fn wrong_schema_is_rejected() {
        let raw = KOINE_GREEK_MANIFEST.replace("schema = 1", "schema = 99");
        assert!(Manifest::parse(&raw).is_err());
    }
}
