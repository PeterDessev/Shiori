//! Documents, sentences, and tokens.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::word::{PartOfSpeech, WordKey};

/// Database identifier of an imported document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DocumentId(pub i64);

/// Database identifier of a sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SentenceId(pub i64);

/// An imported document (book, article, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: DocumentId,
    /// Language the document was imported under (BCP-47-ish code:
    /// "ja", "grc", "es", …).
    pub lang: String,
    pub title: String,
    /// Author name; empty when unknown.
    pub author: String,
    /// Publisher; empty when unknown.
    pub publisher: String,
    /// Publication date as free text (sources vary wildly); empty when
    /// unknown.
    pub published: String,
    /// Index of the first sentence of the page the user last read.
    pub last_sentence: u32,
    pub added_at: DateTime<Utc>,
}

/// Descriptive metadata supplied when importing a document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentMeta {
    pub title: String,
    pub author: String,
    pub publisher: String,
    pub published: String,
}

impl DocumentMeta {
    pub fn titled(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }
}

/// One sentence of a document, with its position preserved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sentence {
    pub id: SentenceId,
    pub document_id: DocumentId,
    /// 0-based position of the sentence within the document.
    pub index: u32,
    /// 0-based paragraph the sentence belongs to.
    pub paragraph: u32,
    pub text: String,
}

/// One morpheme as produced by the analyzer, located within its sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// Text exactly as it appears in the sentence.
    pub surface: String,
    /// Dictionary (base) form.
    pub lemma: String,
    /// Hiragana reading of the lemma; empty when the analyzer had no entry.
    pub reading: String,
    pub pos: PartOfSpeech,
    /// Byte offset of the first byte of `surface` in the sentence text.
    pub start: usize,
    /// Byte offset one past the last byte of `surface`.
    pub end: usize,
}

impl Token {
    /// The identity under which this token's word is tracked.
    pub fn word_key(&self) -> WordKey {
        WordKey::new(self.lemma.clone(), self.reading.clone(), self.pos)
    }

    /// Whether this token should count toward vocabulary at all.
    pub fn is_content_word(&self) -> bool {
        self.pos.is_content_word()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_word_key_uses_lemma_not_surface() {
        let token = Token {
            surface: "食べました".to_string(),
            lemma: "食べる".to_string(),
            reading: "たべる".to_string(),
            pos: PartOfSpeech::Verb,
            start: 0,
            end: "食べました".len(),
        };
        let key = token.word_key();
        assert_eq!(key.lemma, "食べる");
        assert_eq!(key.reading, "たべる");
        assert_eq!(key.pos, PartOfSpeech::Verb);
    }
}
