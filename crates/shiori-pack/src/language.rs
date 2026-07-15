//! The pack-backed `LanguageService`.
//!
//! Dead languages read pre-annotated texts, so this service's analyzer
//! is deliberately simple: NFC-normalize, split paragraphs on blank
//! lines, sentences on the manifest's enders, and tokens on
//! whitespace/punctuation. It exists for chat messages and plain-text
//! imports; annotated imports bypass it entirely.

use shiori_core::{PartOfSpeech, Token};
use shiori_lang::{
    AnalyzedParagraph, AnalyzedSentence, AnalyzedText, ExtractProfile, LangError, LanguageService,
    PromptProfile,
};
use unicode_normalization::UnicodeNormalization;

use crate::Manifest;

/// A language implemented entirely from pack data.
pub struct PackLanguage {
    lang: String,
    dict_source: String,
    joiner: String,
    sentence_enders: Vec<char>,
    script_ranges: Vec<(u32, u32)>,
    graded_scheme: Option<(String, String)>,
    prompt: PromptProfile,
    extract: ExtractProfile,
}

impl PackLanguage {
    pub fn new(manifest: &Manifest) -> Self {
        Self {
            lang: manifest.lang.clone(),
            dict_source: manifest.dict_source.clone(),
            joiner: manifest.joiner.clone(),
            sentence_enders: manifest
                .sentence_enders
                .iter()
                .filter_map(|s| s.chars().next())
                .collect(),
            script_ranges: manifest.script_ranges.clone(),
            graded_scheme: manifest
                .graded_scheme
                .as_ref()
                .map(|s| (s.key.clone(), s.display.clone())),
            prompt: manifest.prompt_profile(),
            extract: manifest.extract_profile(),
        }
    }

    fn in_script(&self, c: char) -> bool {
        let cp = c as u32;
        self.script_ranges.iter().any(|&(lo, hi)| (lo..=hi).contains(&cp))
            // Basic Greek letters etc. that NFC may produce outside the
            // declared ranges are covered by the ranges themselves; plain
            // ASCII letters are never target script for packs that
            // declare ranges.
            || (self.script_ranges.is_empty() && c.is_alphabetic())
    }

    fn split_sentences<'a>(&self, paragraph: &'a str) -> Vec<&'a str> {
        let mut out = Vec::new();
        let mut start = 0;
        let bytes_end = paragraph.len();
        for (i, c) in paragraph.char_indices() {
            if self.sentence_enders.contains(&c) {
                let end = i + c.len_utf8();
                let piece = paragraph[start..end].trim();
                if !piece.is_empty() {
                    out.push(piece);
                }
                start = end;
            }
        }
        let tail = paragraph[start..bytes_end].trim();
        if !tail.is_empty() {
            out.push(tail);
        }
        out
    }
}

/// Fold text for lookups: NFC → lowercase → strip combining marks
/// (accents, breathings, iota subscript) → final sigma to medial.
///
/// Pack dictionaries index their forms pre-folded with this same
/// function (via `shiori-packc`), so the tokenizer, the search box, and
/// `dict_forms` always agree on the key.
pub fn fold_lookup(text: &str) -> String {
    text.nfc()
        .collect::<String>()
        .to_lowercase()
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .map(|c| if c == 'ς' { 'σ' } else { c })
        .collect::<String>()
        .nfc()
        .collect()
}

fn is_combining_mark(c: char) -> bool {
    matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF)
}

/// NFC-normalize text at the import boundary.
pub fn normalize_nfc(text: &str) -> String {
    text.nfc().collect()
}

impl LanguageService for PackLanguage {
    fn lang(&self) -> &str {
        &self.lang
    }

    fn dict_source(&self) -> &str {
        &self.dict_source
    }

    fn analyze(&self, text: &str) -> Result<AnalyzedText, LangError> {
        let text = normalize_nfc(text);
        let mut paragraphs = Vec::new();
        for para in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
            let para = para.replace('\n', " ");
            let mut sentences = Vec::new();
            for sentence in self.split_sentences(&para) {
                sentences.push(AnalyzedSentence {
                    text: sentence.to_string(),
                    tokens: self.tokenize_sentence(sentence)?,
                });
            }
            if !sentences.is_empty() {
                paragraphs.push(AnalyzedParagraph { sentences });
            }
        }
        Ok(AnalyzedText { paragraphs })
    }

    /// Whitespace/punctuation tokenizer. Lemma = surface (a Tier-1
    /// full-form table refines this when the pack ships one); reading
    /// stays empty — pack languages carry annotations on tokens, not
    /// readings.
    fn tokenize_sentence(&self, sentence: &str) -> Result<Vec<Token>, LangError> {
        let mut out = Vec::new();
        let mut word_start: Option<usize> = None;
        let flush = |out: &mut Vec<Token>, start: usize, end: usize, sentence: &str| {
            let surface = &sentence[start..end];
            out.push(Token {
                surface: surface.to_string(),
                lemma: surface.to_string(),
                reading: String::new(),
                pos: if surface.chars().all(|c| !c.is_alphabetic()) {
                    PartOfSpeech::Symbol
                } else {
                    PartOfSpeech::Unknown
                },
                start,
                end,
            });
        };
        for (i, c) in sentence.char_indices() {
            let is_word = self.in_script(c) || c.is_alphanumeric() || c == '\u{2019}' || c == '᾽';
            match (is_word, word_start) {
                (true, None) => word_start = Some(i),
                (false, Some(start)) => {
                    flush(&mut out, start, i, sentence);
                    word_start = None;
                    if !c.is_whitespace() {
                        flush(&mut out, i, i + c.len_utf8(), sentence);
                    }
                }
                (false, None) => {
                    if !c.is_whitespace() {
                        flush(&mut out, i, i + c.len_utf8(), sentence);
                    }
                }
                (true, Some(_)) => {}
            }
        }
        if let Some(start) = word_start {
            flush(&mut out, start, sentence.len(), sentence);
        }
        Ok(out)
    }

    fn is_target_language(&self, text: &str) -> bool {
        text.chars().any(|c| self.in_script(c))
    }

    fn joiner(&self) -> &str {
        &self.joiner
    }

    fn normalize_lookup(&self, text: &str) -> String {
        fold_lookup(text)
    }

    fn frequency_forms(&self, lemma: &str, _reading: &str) -> Vec<String> {
        // Pack frequency lists are keyed by folded lemma.
        vec![fold_lookup(lemma)]
    }

    fn graded_scheme(&self) -> Option<(String, String)> {
        self.graded_scheme.clone()
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
    use crate::manifest::KOINE_GREEK_MANIFEST;

    fn grc() -> PackLanguage {
        PackLanguage::new(&Manifest::parse(KOINE_GREEK_MANIFEST).unwrap())
    }

    #[test]
    fn greek_text_tokenizes_with_offsets() {
        let svc = grc();
        let sentence = "Ἐν ἀρχῇ ἦν ὁ λόγος, καὶ ὁ λόγος ἦν πρὸς τὸν θεόν.";
        let tokens = svc.tokenize_sentence(sentence).unwrap();
        for t in &tokens {
            assert_eq!(&sentence[t.start..t.end], t.surface);
        }
        let words: Vec<&str> = tokens
            .iter()
            .filter(|t| t.pos != PartOfSpeech::Symbol)
            .map(|t| t.surface.as_str())
            .collect();
        assert_eq!(words[0], "Ἐν");
        assert_eq!(words[4], "λόγος");
        // Punctuation split off as its own symbol tokens.
        assert!(tokens.iter().any(|t| t.surface == ","));
    }

    #[test]
    fn greek_sentence_enders_split() {
        let svc = grc();
        let analyzed = svc
            .analyze("οὗτος ἦν ἐν ἀρχῇ πρὸς τὸν θεόν· πάντα δι᾽ αὐτοῦ ἐγένετο. τί οὖν;")
            .unwrap();
        assert_eq!(analyzed.paragraphs.len(), 1);
        assert_eq!(analyzed.paragraphs[0].sentences.len(), 3);
    }

    #[test]
    fn target_language_is_script_based() {
        let svc = grc();
        assert!(svc.is_target_language("λόγος"));
        assert!(svc.is_target_language("Ἐν"));
        assert!(!svc.is_target_language("logos"));
        assert!(!svc.is_target_language("猫"));
        assert!(!svc.is_target_language("…"));
    }

    #[test]
    fn folding_strips_accents_breathings_and_final_sigma() {
        assert_eq!(fold_lookup("λόγος"), "λογοσ");
        assert_eq!(fold_lookup("Ἐν"), "εν");
        assert_eq!(fold_lookup("ἀρχῇ"), "αρχη");
        assert_eq!(fold_lookup("ᾧ"), "ω");
        // NFD input folds identically to NFC input.
        let nfd: String = "λόγος"
            .chars()
            .flat_map(|c| c.to_string().chars().collect::<Vec<_>>())
            .collect();
        assert_eq!(fold_lookup(&nfd), fold_lookup("λόγος"));
    }

    #[test]
    fn defaults_flow_from_the_trait() {
        let svc = grc();
        let tokens = svc.tokenize_sentence("ὁ λόγος").unwrap();
        assert_eq!(svc.phrase_groups(&tokens), vec![(0, 1), (1, 2)]);
        assert!(svc.analyze_inflection(&tokens).is_plain());
        assert_eq!(svc.joiner(), " ");
        assert!(svc.prompt_profile().synthetic_disclaimer.is_some());
    }
}
