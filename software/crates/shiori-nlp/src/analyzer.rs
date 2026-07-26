//! The morphological analyzer: Lindera + IPADIC behind a workspace-shaped API.

use std::collections::HashMap;
use std::sync::Mutex;

use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use lindera::tokenizer::Tokenizer;
use shiori_core::{PartOfSpeech, Token};
use shiori_lang::{AnalyzedParagraph, AnalyzedSentence, AnalyzedText};

use crate::kana::{is_kana_only, katakana_to_hiragana};
use crate::pos::map_pos;
use crate::segment::{split_paragraphs, split_sentences};
use crate::NlpError;

// IPADIC detail field indices.
const D_POS_MAJOR: usize = 0;
const D_POS_SUB: usize = 1;
const D_BASE_FORM: usize = 6;
const D_READING: usize = 7;

/// Morphological analyzer with the embedded IPADIC dictionary.
///
/// Construction is relatively expensive (the dictionary is deserialized);
/// create one and reuse it.
pub struct Analyzer {
    tokenizer: Tokenizer,
    /// Cache of dictionary-form → hiragana reading, filled lazily when
    /// conjugated tokens force a second lookup of their base form.
    lemma_readings: Mutex<HashMap<String, String>>,
}

impl Analyzer {
    pub fn new() -> Result<Self, NlpError> {
        let dictionary =
            load_dictionary("embedded://ipadic").map_err(|e| NlpError::Tokenizer(e.to_string()))?;
        let segmenter = Segmenter::new(Mode::Normal, dictionary, None);
        Ok(Self {
            tokenizer: Tokenizer::new(segmenter),
            lemma_readings: Mutex::new(HashMap::new()),
        })
    }

    /// Analyze a whole document: paragraph split, sentence split, tokenize.
    pub fn analyze(&self, text: &str) -> Result<AnalyzedText, NlpError> {
        let mut paragraphs = Vec::new();
        for para in split_paragraphs(text) {
            let mut sentences = Vec::new();
            for sentence in split_sentences(para) {
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

    /// Tokenize a single sentence into workspace tokens.
    ///
    /// Whitespace-only tokens are dropped. Byte offsets are relative to the
    /// sentence string passed in.
    pub fn tokenize_sentence(&self, sentence: &str) -> Result<Vec<Token>, NlpError> {
        let mut lindera_tokens = self
            .tokenizer
            .tokenize(sentence)
            .map_err(|e| NlpError::Tokenizer(e.to_string()))?;

        let mut out = Vec::with_capacity(lindera_tokens.len());
        for token in lindera_tokens.iter_mut() {
            let surface = token.surface.to_string();
            if surface.trim().is_empty() {
                continue;
            }
            let (start, end) = (token.byte_start, token.byte_end);
            let details = token.details();

            let major = details.get(D_POS_MAJOR).copied().unwrap_or("UNK");
            let sub = details.get(D_POS_SUB).copied().unwrap_or("*");
            let pos = if major == "UNK" {
                PartOfSpeech::Unknown
            } else {
                map_pos(major, sub)
            };

            let base = details
                .get(D_BASE_FORM)
                .copied()
                .filter(|b| !b.is_empty() && *b != "*")
                .unwrap_or(surface.as_str())
                .to_string();

            let surface_reading = details
                .get(D_READING)
                .copied()
                .filter(|r| !r.is_empty() && *r != "*")
                .map(katakana_to_hiragana);

            let reading = if base == surface {
                surface_reading.unwrap_or_else(|| {
                    if is_kana_only(&surface) {
                        katakana_to_hiragana(&surface)
                    } else {
                        String::new()
                    }
                })
            } else {
                self.lemma_reading(&base)
            };

            out.push(Token {
                surface,
                lemma: base,
                reading,
                pos,
                start,
                end,
            });
        }
        Ok(out)
    }

    /// Hiragana reading of a dictionary form.
    ///
    /// IPADIC only carries the reading of the *surface* form, so for
    /// conjugated tokens the base form is re-tokenized once and cached.
    fn lemma_reading(&self, base: &str) -> String {
        if let Some(hit) = self
            .lemma_readings
            .lock()
            .ok()
            .and_then(|m| m.get(base).cloned())
        {
            return hit;
        }

        let reading = self.compute_lemma_reading(base);
        if let Ok(mut map) = self.lemma_readings.lock() {
            map.insert(base.to_string(), reading.clone());
        }
        reading
    }

    fn compute_lemma_reading(&self, base: &str) -> String {
        if let Ok(mut tokens) = self.tokenizer.tokenize(base) {
            if tokens.len() == 1 {
                let details = tokens[0].details();
                if let Some(r) = details
                    .get(D_READING)
                    .copied()
                    .filter(|r| !r.is_empty() && *r != "*")
                {
                    return katakana_to_hiragana(r);
                }
            }
        }
        if is_kana_only(base) {
            katakana_to_hiragana(base)
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn analyzer() -> &'static Analyzer {
        static A: OnceLock<Analyzer> = OnceLock::new();
        A.get_or_init(|| Analyzer::new().expect("embedded dictionary should load"))
    }

    #[test]
    fn tokenizes_simple_sentence() {
        let tokens = analyzer().tokenize_sentence("私は学生です。").unwrap();
        let surfaces: Vec<&str> = tokens.iter().map(|t| t.surface.as_str()).collect();
        assert_eq!(surfaces, vec!["私", "は", "学生", "です", "。"]);

        assert_eq!(tokens[0].pos, PartOfSpeech::Pronoun);
        assert_eq!(tokens[0].reading, "わたし");
        assert_eq!(tokens[1].pos, PartOfSpeech::Particle);
        assert_eq!(tokens[2].pos, PartOfSpeech::Noun);
        assert_eq!(tokens[2].reading, "がくせい");
        assert_eq!(tokens[3].pos, PartOfSpeech::AuxiliaryVerb);
        assert_eq!(tokens[4].pos, PartOfSpeech::Symbol);
    }

    #[test]
    fn lemmatizes_conjugated_verbs_with_base_reading() {
        let tokens = analyzer()
            .tokenize_sentence("昨日寿司を食べました。")
            .unwrap();
        let eat = tokens
            .iter()
            .find(|t| t.lemma == "食べる")
            .expect("食べました should lemmatize to 食べる");
        assert_eq!(eat.pos, PartOfSpeech::Verb);
        assert_eq!(eat.reading, "たべる", "reading must be of the base form");
    }

    #[test]
    fn byte_offsets_reconstruct_surfaces() {
        let sentence = "猫がソファーの上で寝ている。";
        let tokens = analyzer().tokenize_sentence(sentence).unwrap();
        assert!(!tokens.is_empty());
        for t in &tokens {
            assert_eq!(&sentence[t.start..t.end], t.surface);
        }
    }

    #[test]
    fn katakana_loanwords_get_hiragana_readings() {
        let tokens = analyzer().tokenize_sentence("コーヒーを飲む。").unwrap();
        let coffee = tokens.iter().find(|t| t.surface == "コーヒー").unwrap();
        assert_eq!(coffee.pos, PartOfSpeech::Noun);
        assert_eq!(coffee.reading, "こーひー");
    }

    #[test]
    fn analyze_preserves_paragraph_and_sentence_structure() {
        let text = "猫が好きだ。犬も好きだ。\n\n鳥は苦手だ。";
        let analyzed = analyzer().analyze(text).unwrap();
        assert_eq!(analyzed.paragraphs.len(), 2);
        assert_eq!(analyzed.paragraphs[0].sentences.len(), 2);
        assert_eq!(analyzed.paragraphs[1].sentences.len(), 1);
        assert_eq!(analyzed.paragraphs[0].sentences[0].text, "猫が好きだ。");
        assert!(analyzed.token_count() > 0);
    }

    #[test]
    fn empty_input_yields_empty_analysis() {
        let analyzed = analyzer().analyze("").unwrap();
        assert!(analyzed.paragraphs.is_empty());
        assert_eq!(analyzed.token_count(), 0);
    }

    #[test]
    fn whitespace_tokens_are_dropped() {
        let tokens = analyzer().tokenize_sentence("はい そうです").unwrap();
        assert!(tokens.iter().all(|t| !t.surface.trim().is_empty()));
    }
}
