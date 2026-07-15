//! Japanese as a [`LanguageService`]: the Lindera/IPADIC analyzer plus
//! the kana, romaji, ruby, and phrase-grouping utilities, behind the
//! language-neutral trait the rest of the workspace consumes.

use shiori_core::{PartOfSpeech, Token};
use shiori_lang::{
    AnalyzedText, ExtractProfile, Inflection, LangError, LanguageService, PromptProfile,
    RubySegment,
};

use crate::{Analyzer, NlpError};

/// The Japanese language implementation.
pub struct Japanese {
    analyzer: Analyzer,
    prompt: PromptProfile,
    extract: ExtractProfile,
}

impl Japanese {
    pub fn new() -> Result<Self, NlpError> {
        Ok(Self {
            analyzer: Analyzer::new()?,
            prompt: PromptProfile {
                language_name: "Japanese".into(),
                chat_persona: "a friendly native Japanese speaker".into(),
                citation_guidance: "When you cite Japanese, give it in Japanese script \
                                    followed by a reading in parentheses where helpful."
                    .into(),
                grammar_skeleton: "particles, verb forms, clause structure".into(),
                quote_open: "「".into(),
                quote_close: "」".into(),
                immerse_instruction: "Write natural native Japanese without simplification; \
                                      the user wants full immersion."
                    .into(),
                unnatural_authority: "phrasing a native speaker would not use".into(),
                synthetic_disclaimer: None,
            },
            extract: ExtractProfile {
                legacy_encodings: vec!["shift_jis".into()],
                japanese_conventions: true,
            },
        })
    }
}

fn to_lang_err(e: NlpError) -> LangError {
    LangError::Analysis(e.to_string())
}

impl LanguageService for Japanese {
    fn lang(&self) -> &str {
        "ja"
    }

    fn dict_source(&self) -> &str {
        "jmdict"
    }

    fn analyze(&self, text: &str) -> Result<AnalyzedText, LangError> {
        self.analyzer.analyze(text).map_err(to_lang_err)
    }

    fn tokenize_sentence(&self, sentence: &str) -> Result<Vec<Token>, LangError> {
        self.analyzer
            .tokenize_sentence(sentence)
            .map_err(to_lang_err)
    }

    fn phrase_groups(&self, tokens: &[Token]) -> Vec<(usize, usize)> {
        crate::inflection::phrase_groups(tokens)
    }

    fn analyze_inflection(&self, group: &[Token]) -> Inflection {
        crate::inflection::analyze_inflection(group)
    }

    fn is_target_language(&self, text: &str) -> bool {
        crate::kana::is_japanese(text)
    }

    fn joiner(&self) -> &str {
        ""
    }

    fn search_transliterate(&self, query: &str) -> Option<String> {
        crate::romaji::romaji_to_kana(query)
    }

    fn ruby(&self, text: &str, reading: &str) -> Vec<RubySegment> {
        crate::ruby::ruby_segments(text, reading)
    }

    fn has_annotatable_script(&self, text: &str) -> bool {
        crate::kana::contains_kanji(text)
    }

    fn compound_lookup_pos(&self, pos: PartOfSpeech) -> bool {
        matches!(
            pos,
            PartOfSpeech::Noun
                | PartOfSpeech::ProperNoun
                | PartOfSpeech::AdjectivalNoun
                | PartOfSpeech::Prefix
        )
    }

    fn grapheme_card_chars(&self, text: &str) -> Vec<char> {
        text.chars()
            .filter(|c| crate::kana::contains_kanji(&c.to_string()))
            .collect()
    }

    fn level_hint(&self, known: u32) -> String {
        let level = match known {
            0..=150 => "a beginner (around JLPT N5)",
            151..=600 => "an upper beginner (around JLPT N5–N4)",
            601..=1500 => "lower intermediate (around JLPT N4)",
            1501..=3500 => "intermediate (around JLPT N3)",
            3501..=7000 => "upper intermediate (around JLPT N2)",
            _ => "advanced (around JLPT N1)",
        };
        format!("The user has about {known} recorded known words, suggesting {level}.")
    }

    fn prompt_profile(&self) -> &PromptProfile {
        &self.prompt
    }

    fn extract_profile(&self) -> &ExtractProfile {
        &self.extract
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn service() -> &'static Japanese {
        static S: OnceLock<Japanese> = OnceLock::new();
        S.get_or_init(|| Japanese::new().expect("embedded dictionary should load"))
    }

    #[test]
    fn trait_surface_matches_the_free_functions() {
        let svc = service();
        let tokens = svc.tokenize_sentence("本を読んでいる。").unwrap();
        assert_eq!(
            svc.phrase_groups(&tokens),
            crate::inflection::phrase_groups(&tokens)
        );
        assert!(svc.is_target_language("猫"));
        assert!(!svc.is_target_language("cat"));
        assert_eq!(svc.joiner(), "");
        assert_eq!(svc.search_transliterate("neko").as_deref(), Some("ねこ"));
        assert!(svc.has_annotatable_script("食べる"));
        assert!(!svc.has_annotatable_script("たべる"));
        assert_eq!(svc.grapheme_card_chars("食べ物"), vec!['食', '物']);
        assert!(svc.compound_lookup_pos(PartOfSpeech::Prefix));
        assert!(!svc.compound_lookup_pos(PartOfSpeech::Verb));
    }

    #[test]
    fn level_hint_preserves_the_jlpt_wording() {
        let hint = service().level_hint(0);
        assert!(hint.contains("0 recorded"));
        assert!(hint.contains("N5"));
        assert!(service().level_hint(9999).contains("N1"));
    }
}
