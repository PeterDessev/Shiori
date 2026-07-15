//! The trait every supported language implements.

use shiori_core::{PartOfSpeech, Token};

use crate::{AnalyzedText, ExtractProfile, Inflection, LangError, PromptProfile, RubySegment};

/// Everything Shiori asks of a language.
///
/// Defaults implement the "plain alphabetic language" behavior: words are
/// their own phrase, no annotation layer, no transliteration, no
/// character cards. Implementations override only what their language
/// actually needs.
pub trait LanguageService: Send + Sync {
    /// BCP-47-ish language code ("ja", "grc", "es").
    fn lang(&self) -> &str;

    /// Dictionary source backing this language ('jmdict' for Japanese).
    fn dict_source(&self) -> &str;

    /// Analyze a whole text: paragraph split, sentence split, tokenize.
    fn analyze(&self, text: &str) -> Result<AnalyzedText, LangError>;

    /// Tokenize a single sentence; byte offsets are relative to it.
    fn tokenize_sentence(&self, sentence: &str) -> Result<Vec<Token>, LangError>;

    /// Group a sentence's tokens into selection/lookup phrases as
    /// `(start, end)` half-open ranges covering all tokens in order.
    /// Default: every token is its own phrase.
    fn phrase_groups(&self, tokens: &[Token]) -> Vec<(usize, usize)> {
        (0..tokens.len()).map(|i| (i, i + 1)).collect()
    }

    /// Describe the grammar of a phrase group's tail. Default: plain.
    fn analyze_inflection(&self, _group: &[Token]) -> Inflection {
        Inflection::default()
    }

    /// Whether `text` is vocabulary of this language (as opposed to
    /// foreign fragments, lone punctuation, digits…). Gates mining,
    /// finish sweeps, clickability, and Anki import field detection.
    fn is_target_language(&self, text: &str) -> bool;

    /// Separator placed between adjacent tokens when reconstructing
    /// running text ("" for Japanese, " " for space-delimited scripts).
    fn joiner(&self) -> &str {
        " "
    }

    /// Transliterate a search-box query into the language's script
    /// (romaji → kana; betacode/Greeklish → polytonic Greek). `None`
    /// when the query is not transliterable input.
    fn search_transliterate(&self, _query: &str) -> Option<String> {
        None
    }

    /// Split a word into display segments with per-segment annotations
    /// (furigana over kanji runs for Japanese). Default: one unannotated
    /// segment — languages whose annotations ride on stored token
    /// glosses don't use this hook.
    fn ruby(&self, text: &str, _reading: &str) -> Vec<RubySegment> {
        vec![RubySegment {
            text: text.to_string(),
            furigana: None,
        }]
    }

    /// Whether `text` contains characters that carry a reading
    /// annotation (kanji for Japanese). Gates the ruby layout path.
    fn has_annotatable_script(&self, _text: &str) -> bool {
        false
    }

    /// POS classes worth retrying as one compound dictionary lookup when
    /// a phrase's surface differs from its head lemma (低声 = 低＋声).
    fn compound_lookup_pos(&self, _pos: PartOfSpeech) -> bool {
        false
    }

    /// Characters of `text` that have per-character reference cards
    /// (kanji cards for Japanese). Empty when the capability is absent.
    fn grapheme_card_chars(&self, _text: &str) -> Vec<char> {
        Vec::new()
    }

    /// Which POS classes count as learnable vocabulary. Mirrors
    /// [`PartOfSpeech::is_content_word`] by default.
    fn is_content_word(&self, pos: PartOfSpeech) -> bool {
        pos.is_content_word()
    }

    /// One-line description of the user's vocabulary size for the chat
    /// partner's system prompt.
    fn level_hint(&self, known_words: u32) -> String {
        format!("The user has about {known_words} recorded known words in this language.")
    }

    /// Language fragments for LLM prompt construction.
    fn prompt_profile(&self) -> &PromptProfile;

    /// File-reading conventions for imports.
    fn extract_profile(&self) -> &ExtractProfile;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Minimal {
        prompt: PromptProfile,
        extract: ExtractProfile,
    }

    impl Minimal {
        fn new() -> Self {
            Self {
                prompt: PromptProfile {
                    language_name: "Testish".into(),
                    chat_persona: "a native Testish speaker".into(),
                    citation_guidance: String::new(),
                    grammar_skeleton: "word order".into(),
                    quote_open: "\"".into(),
                    quote_close: "\"".into(),
                    immerse_instruction: "Write natural Testish.".into(),
                    unnatural_authority: "phrasing a native speaker would not use".into(),
                    synthetic_disclaimer: None,
                },
                extract: ExtractProfile::default(),
            }
        }
    }

    impl LanguageService for Minimal {
        fn lang(&self) -> &str {
            "xx"
        }
        fn dict_source(&self) -> &str {
            "xxdict"
        }
        fn analyze(&self, _text: &str) -> Result<AnalyzedText, LangError> {
            Ok(AnalyzedText::default())
        }
        fn tokenize_sentence(&self, _sentence: &str) -> Result<Vec<Token>, LangError> {
            Ok(Vec::new())
        }
        fn is_target_language(&self, text: &str) -> bool {
            text.chars().any(|c| c.is_alphabetic())
        }
        fn prompt_profile(&self) -> &PromptProfile {
            &self.prompt
        }
        fn extract_profile(&self) -> &ExtractProfile {
            &self.extract
        }
    }

    #[test]
    fn defaults_are_the_plain_language_behavior() {
        let svc = Minimal::new();
        let tokens = vec![
            Token {
                surface: "sol".into(),
                lemma: "sol".into(),
                reading: String::new(),
                pos: PartOfSpeech::Noun,
                start: 0,
                end: 3,
            },
            Token {
                surface: "lucet".into(),
                lemma: "luceo".into(),
                reading: String::new(),
                pos: PartOfSpeech::Verb,
                start: 4,
                end: 9,
            },
        ];
        assert_eq!(svc.phrase_groups(&tokens), vec![(0, 1), (1, 2)]);
        assert!(svc.analyze_inflection(&tokens).is_plain());
        assert_eq!(svc.joiner(), " ");
        assert_eq!(svc.search_transliterate("query"), None);
        assert_eq!(svc.ruby("sol", "").len(), 1);
        assert!(!svc.has_annotatable_script("sol"));
        assert!(svc.grapheme_card_chars("sol").is_empty());
        assert!(svc.is_content_word(PartOfSpeech::Noun));
        assert!(!svc.is_content_word(PartOfSpeech::Particle));
        assert!(svc.level_hint(42).contains("42"));
    }
}
