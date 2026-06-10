//! Word identity and knowledge tracking.

use serde::{Deserialize, Serialize};

/// Database identifier of a tracked word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WordId(pub i64);

/// Coarse part-of-speech classes.
///
/// These are deliberately broader than what a morphological analyzer
/// produces; they are the granularity at which vocabulary knowledge is
/// tracked and displayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartOfSpeech {
    Noun,
    ProperNoun,
    Pronoun,
    /// Dependent noun (名詞,非自立): grammaticalized nominalizers like
    /// の・こと・よう・ため that only occur bound to a clause.
    DependentNoun,
    Verb,
    /// い-adjective.
    Adjective,
    /// な-adjective (adjectival noun, 形容動詞).
    AdjectivalNoun,
    Adverb,
    Particle,
    AuxiliaryVerb,
    Conjunction,
    /// Prenominal adjectival (連体詞), e.g. この, 大きな.
    Prenominal,
    Interjection,
    Number,
    Prefix,
    Suffix,
    Symbol,
    Unknown,
}

impl PartOfSpeech {
    /// Content words are the ones worth learning as vocabulary; function
    /// words (particles, auxiliaries, symbols) and bound morphemes
    /// (prefixes/suffixes like 的・化) are tracked but never mined.
    pub fn is_content_word(self) -> bool {
        !matches!(
            self,
            PartOfSpeech::Particle
                | PartOfSpeech::AuxiliaryVerb
                | PartOfSpeech::Symbol
                | PartOfSpeech::Number
                | PartOfSpeech::Prefix
                | PartOfSpeech::Suffix
                | PartOfSpeech::DependentNoun
                | PartOfSpeech::Unknown
        )
    }

    /// Stable string form used for persistence.
    pub fn as_str(self) -> &'static str {
        match self {
            PartOfSpeech::Noun => "noun",
            PartOfSpeech::ProperNoun => "proper_noun",
            PartOfSpeech::Pronoun => "pronoun",
            PartOfSpeech::DependentNoun => "dependent_noun",
            PartOfSpeech::Verb => "verb",
            PartOfSpeech::Adjective => "adjective",
            PartOfSpeech::AdjectivalNoun => "adjectival_noun",
            PartOfSpeech::Adverb => "adverb",
            PartOfSpeech::Particle => "particle",
            PartOfSpeech::AuxiliaryVerb => "auxiliary_verb",
            PartOfSpeech::Conjunction => "conjunction",
            PartOfSpeech::Prenominal => "prenominal",
            PartOfSpeech::Interjection => "interjection",
            PartOfSpeech::Number => "number",
            PartOfSpeech::Prefix => "prefix",
            PartOfSpeech::Suffix => "suffix",
            PartOfSpeech::Symbol => "symbol",
            PartOfSpeech::Unknown => "unknown",
        }
    }

    /// Inverse of [`as_str`](Self::as_str); unknown strings map to `Unknown`.
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "noun" => PartOfSpeech::Noun,
            "proper_noun" => PartOfSpeech::ProperNoun,
            "pronoun" => PartOfSpeech::Pronoun,
            "dependent_noun" => PartOfSpeech::DependentNoun,
            "verb" => PartOfSpeech::Verb,
            "adjective" => PartOfSpeech::Adjective,
            "adjectival_noun" => PartOfSpeech::AdjectivalNoun,
            "adverb" => PartOfSpeech::Adverb,
            "particle" => PartOfSpeech::Particle,
            "auxiliary_verb" => PartOfSpeech::AuxiliaryVerb,
            "conjunction" => PartOfSpeech::Conjunction,
            "prenominal" => PartOfSpeech::Prenominal,
            "interjection" => PartOfSpeech::Interjection,
            "number" => PartOfSpeech::Number,
            "prefix" => PartOfSpeech::Prefix,
            "suffix" => PartOfSpeech::Suffix,
            "symbol" => PartOfSpeech::Symbol,
            _ => PartOfSpeech::Unknown,
        }
    }
}

/// The identity of a vocabulary item: dictionary form + reading + POS class.
///
/// Two tokens are occurrences of the same word iff their keys are equal.
/// The reading is always normalized to hiragana so that 「タベル」 and
/// 「たべる」 do not split a word in two.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WordKey {
    /// Dictionary (base) form, e.g. 食べる for 食べました.
    pub lemma: String,
    /// Hiragana reading of the dictionary form, e.g. たべる.
    pub reading: String,
    pub pos: PartOfSpeech,
}

impl WordKey {
    pub fn new(lemma: impl Into<String>, reading: impl Into<String>, pos: PartOfSpeech) -> Self {
        Self {
            lemma: lemma.into(),
            reading: reading.into(),
            pos,
        }
    }
}

/// How well the user knows a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeStatus {
    /// Never studied and not marked otherwise.
    Unknown,
    /// Has an active SRS card.
    Learning,
    /// Marked known by the user, or graduated out of review.
    Known,
    /// Deliberately excluded (names, transcription noise, …).
    Ignored,
}

impl KnowledgeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            KnowledgeStatus::Unknown => "unknown",
            KnowledgeStatus::Learning => "learning",
            KnowledgeStatus::Known => "known",
            KnowledgeStatus::Ignored => "ignored",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "learning" => KnowledgeStatus::Learning,
            "known" => KnowledgeStatus::Known,
            "ignored" => KnowledgeStatus::Ignored,
            _ => KnowledgeStatus::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pos_string_roundtrip() {
        let all = [
            PartOfSpeech::Noun,
            PartOfSpeech::ProperNoun,
            PartOfSpeech::Pronoun,
            PartOfSpeech::DependentNoun,
            PartOfSpeech::Verb,
            PartOfSpeech::Adjective,
            PartOfSpeech::AdjectivalNoun,
            PartOfSpeech::Adverb,
            PartOfSpeech::Particle,
            PartOfSpeech::AuxiliaryVerb,
            PartOfSpeech::Conjunction,
            PartOfSpeech::Prenominal,
            PartOfSpeech::Interjection,
            PartOfSpeech::Number,
            PartOfSpeech::Prefix,
            PartOfSpeech::Suffix,
            PartOfSpeech::Symbol,
            PartOfSpeech::Unknown,
        ];
        for pos in all {
            assert_eq!(PartOfSpeech::from_str_lossy(pos.as_str()), pos);
        }
    }

    #[test]
    fn status_string_roundtrip() {
        for status in [
            KnowledgeStatus::Unknown,
            KnowledgeStatus::Learning,
            KnowledgeStatus::Known,
            KnowledgeStatus::Ignored,
        ] {
            assert_eq!(KnowledgeStatus::from_str_lossy(status.as_str()), status);
        }
    }

    #[test]
    fn particles_are_not_content_words() {
        assert!(!PartOfSpeech::Particle.is_content_word());
        assert!(!PartOfSpeech::AuxiliaryVerb.is_content_word());
        assert!(PartOfSpeech::Noun.is_content_word());
        assert!(PartOfSpeech::Verb.is_content_word());
    }
}
