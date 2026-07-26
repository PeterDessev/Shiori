//! The `LanguageService` abstraction: everything Shiori asks of a language.
//!
//! One implementation exists per supported language. Japanese wraps the
//! Lindera/IPADIC analyzer (`shiori-nlp`); pack-driven languages (Koine
//! Greek onward) are powered by downloadable data — pre-annotated texts
//! and full-form lookup tables — through the generic engines in
//! `shiori-pack`.
//!
//! The trait is deliberately shaped by what callers actually use, not by
//! what a language "is": segmentation and lemmatization for import,
//! predicates and display hooks for the reader, transliteration for
//! search, and profile data for prompts and file extraction.

mod profile;
mod service;
mod types;

pub use profile::{ExtractProfile, PromptProfile};
pub use service::LanguageService;
pub use types::{AnalyzedParagraph, AnalyzedSentence, AnalyzedText, Inflection, RubySegment};

/// Errors surfaced by language services.
#[derive(Debug, thiserror::Error)]
pub enum LangError {
    /// The underlying analyzer/segmenter failed.
    #[error("analysis error: {0}")]
    Analysis(String),

    /// The language's data (pack, table) is missing or malformed.
    #[error("language data error: {0}")]
    Data(String),
}
