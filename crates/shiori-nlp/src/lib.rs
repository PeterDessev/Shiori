//! Japanese morphological analysis and sentence segmentation.
//!
//! Wraps the Lindera tokenizer (with the embedded IPADIC dictionary) and
//! turns raw Japanese text into the workspace's [`shiori_core::Token`]s,
//! organized into sentences and paragraphs so that every token keeps its
//! original context. The [`Japanese`] type packages all of it as the
//! workspace's `LanguageService` implementation for Japanese.
//!
//! The heavy analyzer sits behind the default `embed-ipadic` feature; the
//! kana/romaji/ruby/inflection utilities build without it.

#[cfg(feature = "embed-ipadic")]
mod analyzer;
pub mod inflection;
#[cfg(feature = "embed-ipadic")]
mod japanese;
pub mod kana;
mod pos;
pub mod romaji;
pub mod ruby;
mod segment;

#[cfg(feature = "embed-ipadic")]
pub use analyzer::Analyzer;
pub use inflection::{analyze_inflection, phrase_groups};
#[cfg(feature = "embed-ipadic")]
pub use japanese::Japanese;
pub use kana::{hiragana_to_katakana, is_kana_only, katakana_to_hiragana};
pub use romaji::romaji_to_kana;
pub use ruby::ruby_segments;
pub use segment::{split_paragraphs, split_sentences};
// Shared analysis types moved to shiori-lang; re-exported for callers
// that still name them through this crate.
pub use shiori_lang::{AnalyzedParagraph, AnalyzedSentence, AnalyzedText, Inflection, RubySegment};

/// Errors produced by the NLP pipeline.
#[derive(Debug, thiserror::Error)]
pub enum NlpError {
    /// The underlying morphological analyzer failed.
    #[error("tokenizer error: {0}")]
    Tokenizer(String),
}
