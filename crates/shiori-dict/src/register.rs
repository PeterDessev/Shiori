//! Register and nuance classification of JMdict usage tags.
//!
//! JMdict `misc` codes carry exactly the "how is this word actually used"
//! information a reader needs: is it colloquial, archaic, honorific,
//! usually written in kana? This module groups those codes into broad
//! registers for display and filtering.

use serde::{Deserialize, Serialize};

/// Broad usage register of a word or sense.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Register {
    /// Neutral; no special register marking.
    Neutral,
    /// Colloquial, slang, internet slang, vulgar, jocular, children's.
    Colloquial,
    /// Formal or literary language.
    Formal,
    /// Honorific (sonkeigo), humble (kenjougo) or polite (teineigo).
    Honorific,
    /// Archaic, dated, obsolete, poetic.
    Archaic,
    /// Sensitive: derogatory, vulgar or otherwise to be used with care.
    Sensitive,
}

impl Register {
    pub fn label(self) -> &'static str {
        match self {
            Register::Neutral => "neutral",
            Register::Colloquial => "colloquial",
            Register::Formal => "formal/literary",
            Register::Honorific => "keigo",
            Register::Archaic => "archaic/dated",
            Register::Sensitive => "use with care",
        }
    }
}

/// Classify a single JMdict misc code into a register, if it marks one.
pub fn register_of_code(code: &str) -> Option<Register> {
    match code {
        "col" | "sl" | "net-sl" | "joc" | "chn" | "fam" | "m-sl" => Some(Register::Colloquial),
        "form" | "litf" | "poet" => Some(Register::Formal),
        "hon" | "hum" | "pol" => Some(Register::Honorific),
        "arch" | "dated" | "obs" | "rare" | "obsc" => Some(Register::Archaic),
        "derog" | "vulg" | "X" | "sens" => Some(Register::Sensitive),
        _ => None,
    }
}

/// Usage notes worth surfacing prominently that are not registers.
pub fn usage_note_of_code(code: &str) -> Option<&'static str> {
    match code {
        "uk" => Some("usually written in kana"),
        "abbr" => Some("abbreviation"),
        "on-mim" => Some("onomatopoeia/mimetic"),
        "id" => Some("idiomatic"),
        "yoji" => Some("four-character idiom"),
        "male" => Some("male speech"),
        "fem" => Some("female speech"),
        "proverb" => Some("proverb"),
        _ => None,
    }
}

/// Summary of how a word is actually used, derived from its misc codes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageProfile {
    /// Registers present across the entry's senses (deduplicated, in
    /// first-seen order).
    pub registers: Vec<Register>,
    /// Additional short usage notes ("usually written in kana", …).
    pub notes: Vec<String>,
}

impl UsageProfile {
    /// Build a profile from JMdict misc codes.
    pub fn from_misc_codes<'a>(codes: impl IntoIterator<Item = &'a str>) -> Self {
        let mut profile = UsageProfile::default();
        for code in codes {
            if let Some(reg) = register_of_code(code) {
                if !profile.registers.contains(&reg) {
                    profile.registers.push(reg);
                }
            }
            if let Some(note) = usage_note_of_code(code) {
                let note = note.to_string();
                if !profile.notes.contains(&note) {
                    profile.notes.push(note);
                }
            }
        }
        profile
    }

    pub fn is_neutral(&self) -> bool {
        self.registers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_register_codes() {
        assert_eq!(register_of_code("col"), Some(Register::Colloquial));
        assert_eq!(register_of_code("net-sl"), Some(Register::Colloquial));
        assert_eq!(register_of_code("hon"), Some(Register::Honorific));
        assert_eq!(register_of_code("hum"), Some(Register::Honorific));
        assert_eq!(register_of_code("arch"), Some(Register::Archaic));
        assert_eq!(register_of_code("form"), Some(Register::Formal));
        assert_eq!(register_of_code("derog"), Some(Register::Sensitive));
        assert_eq!(register_of_code("n"), None, "POS codes are not registers");
    }

    #[test]
    fn builds_usage_profile() {
        let profile = UsageProfile::from_misc_codes(["uk", "col", "sl", "abbr"]);
        assert_eq!(profile.registers, vec![Register::Colloquial]);
        assert_eq!(
            profile.notes,
            vec!["usually written in kana", "abbreviation"]
        );
        assert!(!profile.is_neutral());
    }

    #[test]
    fn neutral_profile_for_unmarked_words() {
        let profile = UsageProfile::from_misc_codes([]);
        assert!(profile.is_neutral());
        assert!(profile.notes.is_empty());
    }
}
