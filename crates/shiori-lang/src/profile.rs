//! Per-language profile data consumed outside the analysis pipeline:
//! prompt construction (`shiori-llm`) and file extraction (`shiori-app`).

/// The language-specific fragments the LLM prompt builders interpolate.
///
/// Pure data so prompt construction stays unit-testable in `shiori-llm`
/// without depending on any language implementation.
#[derive(Debug, Clone)]
pub struct PromptProfile {
    /// English name of the language ("Japanese", "Koine Greek").
    pub language_name: String,
    /// The conversation persona, e.g. "a friendly native Japanese
    /// speaker" or "an educated first-century writer of Koine Greek".
    pub chat_persona: String,
    /// How to cite the language in explanations, e.g. "When you cite
    /// Japanese, give it in Japanese script followed by a reading in
    /// parentheses where helpful."
    pub citation_guidance: String,
    /// The grammatical skeleton enumeration for sentence explanations,
    /// e.g. "particles, verb forms, clause structure".
    pub grammar_skeleton: String,
    /// Quote brackets used around a focus word (「」 for Japanese).
    pub quote_open: String,
    pub quote_close: String,
    /// The full-immersion challenge instruction.
    pub immerse_instruction: String,
    /// The authority for "unnatural" judgements: "phrasing a native
    /// speaker would not use" for living languages, attested usage for
    /// dead ones.
    pub unnatural_authority: String,
    /// For dead languages: an extra system-prompt paragraph disclosing
    /// the synthetic persona and reframing corrections around attested
    /// usage. `None` for living languages.
    pub synthetic_disclaimer: Option<String>,
}

/// How to read files for this language.
#[derive(Debug, Clone, Default)]
pub struct ExtractProfile {
    /// Legacy encodings (WHATWG labels) to try, in order, when a file is
    /// not valid UTF-8. Japanese: `["shift_jis"]`.
    pub legacy_encodings: Vec<String>,
    /// Apply Japanese text conventions during extraction: Aozora Bunko
    /// ruby markup, 。-terminated heading heuristics.
    pub japanese_conventions: bool,
}
