//! Morphological analysis and sentence segmentation.
//!
//! Wraps the Lindera tokenizer (with the embedded IPADIC dictionary) and
//! turns raw Japanese text into the workspace's [`shiori_core::Token`]s,
//! organized into sentences and paragraphs so that every token keeps its
//! original context.

mod analyzer;
pub mod inflection;
pub mod kana;
mod pos;
pub mod romaji;
pub mod ruby;
mod segment;

pub use analyzer::{AnalyzedParagraph, AnalyzedSentence, AnalyzedText, Analyzer};
pub use inflection::{analyze_inflection, phrase_groups, Inflection};
pub use kana::{hiragana_to_katakana, is_kana_only, katakana_to_hiragana};
pub use romaji::romaji_to_kana;
pub use ruby::{ruby_segments, RubySegment};
pub use segment::{split_paragraphs, split_sentences};

/// Errors produced by the NLP pipeline.
#[derive(Debug, thiserror::Error)]
pub enum NlpError {
    /// The underlying morphological analyzer failed.
    #[error("tokenizer error: {0}")]
    Tokenizer(String),
}
