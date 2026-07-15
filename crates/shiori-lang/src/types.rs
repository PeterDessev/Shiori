//! Language-neutral analysis result types.
//!
//! These used to live in `shiori-nlp`; they moved here so the trait (and
//! pack-driven implementations) can use them without dragging in the
//! Japanese tokenizer.

use shiori_core::Token;

/// A fully analyzed text: paragraphs of sentences of tokens.
#[derive(Debug, Clone, Default)]
pub struct AnalyzedText {
    pub paragraphs: Vec<AnalyzedParagraph>,
}

#[derive(Debug, Clone, Default)]
pub struct AnalyzedParagraph {
    pub sentences: Vec<AnalyzedSentence>,
}

#[derive(Debug, Clone)]
pub struct AnalyzedSentence {
    pub text: String,
    pub tokens: Vec<Token>,
}

impl AnalyzedText {
    /// Iterate over all sentences in document order.
    pub fn sentences(&self) -> impl Iterator<Item = &AnalyzedSentence> {
        self.paragraphs.iter().flat_map(|p| p.sentences.iter())
    }

    /// Total number of tokens.
    pub fn token_count(&self) -> usize {
        self.sentences().map(|s| s.tokens.len()).sum()
    }
}

/// What a conjugated phrase is doing grammatically.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inflection {
    /// Headline for well-known constructions, e.g.
    /// "〜ている — ongoing action or resulting state (te-iru form)".
    pub summary: Option<String>,
    /// One line per grammatical component after the stem, in order.
    pub parts: Vec<String>,
}

impl Inflection {
    pub fn is_plain(&self) -> bool {
        self.summary.is_none() && self.parts.is_empty()
    }
}

/// One display segment of a word: the surface text and the annotation to
/// show above it (`None` for segments that read as themselves).
///
/// For Japanese this is furigana over kanji runs; for pack languages the
/// same slot carries interlinear glosses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RubySegment {
    pub text: String,
    pub furigana: Option<String>,
}
