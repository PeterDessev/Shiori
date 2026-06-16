//! Dictionary search: one query returns word entries and kanji cards.
//!
//! The search box accepts kanji, kana, and rōmaji (which is transliterated
//! to kana first), and it understands conjugated input: a query like
//! 食べました or `tabemashita` is reduced to its dictionary root 食べる so
//! the root entry is found, and the conjugation itself is described.

use std::collections::HashSet;

use shiori_core::{PartOfSpeech, Sentence, WordId};
use shiori_db::{KanjiRow, WordRow};
use shiori_dict::DictEntry;
use shiori_nlp::Inflection;

use crate::{App, Result};

/// One word result of a dictionary search.
#[derive(Debug)]
pub struct DictSearchHit {
    pub entry: DictEntry,
    /// The tracked word sharing the headword's lemma, if the user has
    /// met it while reading (carries knowledge status and an id for
    /// pulling example sentences).
    pub word: Option<WordRow>,
    /// JLPT level the word belongs to (5 = N5 easiest … 1 = N1), if listed.
    pub jlpt: Option<u8>,
}

/// What the typed form *is*, when it is a conjugated or compounded unit:
/// the dictionary root it reduces to and the grammar of its tail.
#[derive(Debug, Clone)]
pub struct QueryAnalysis {
    /// The whole conjugated surface the analyzer recognized (e.g. 食べました).
    pub surface: String,
    /// Dictionary root the analyzer reduced it to (e.g. 食べる).
    pub lemma: String,
    /// Hiragana reading of the root.
    pub reading: String,
    /// Coarse part of speech of the root.
    pub pos: PartOfSpeech,
    /// Grammar of the tail after the stem (summary + per-component notes).
    pub inflection: Inflection,
}

/// One example sentence for a dictionary word: the sentence itself, the
/// title of the document it came from, and the byte ranges within the text
/// where the looked-up word's tokens occur (for highlighting).
#[derive(Debug, Clone)]
pub struct DictExample {
    pub sentence: Sentence,
    pub title: String,
    /// Byte ranges of the word's occurrences in `sentence.text`; empty when
    /// no token could be located (e.g. stored offsets and text disagree).
    pub highlights: Vec<(usize, usize)>,
}

/// Everything one search query returns.
#[derive(Debug, Default)]
pub struct DictSearchResults {
    pub words: Vec<DictSearchHit>,
    pub kanji: Vec<KanjiRow>,
    /// Set when the query was a conjugated/compounded form: explains it.
    pub analysis: Option<QueryAnalysis>,
}

impl App {
    /// Search the dictionary by Japanese text or rōmaji.
    ///
    /// Rōmaji is transliterated to kana first; a conjugated query is also
    /// reduced to its dictionary root so the root entry surfaces. Kanji
    /// cards are pulled from every kanji in the (kana-normalized) query —
    /// or, for an all-kana query, from the top hits' headwords.
    pub fn search_dictionary(&self, query: &str) -> Result<DictSearchResults> {
        let raw = query.trim();
        if raw.is_empty() {
            return Ok(DictSearchResults::default());
        }
        // rōmaji → kana; anything already Japanese is searched verbatim.
        let search = shiori_nlp::romaji_to_kana(raw).unwrap_or_else(|| raw.to_string());

        // Reduce a conjugated/compounded query to its dictionary root.
        let analysis = self.analyze_query(&search);

        let mut seen = HashSet::new();
        let mut words = Vec::new();

        // Root-form entries first, so a conjugated query leads with the
        // word it is a form of.
        if let Some(a) = &analysis {
            for seq in self.db().dict_lookup_seqs(&a.lemma)? {
                if seen.insert(seq) {
                    if let Some(hit) = self.build_hit(seq)? {
                        words.push(hit);
                    }
                }
            }
        }
        // Literal exact/prefix matches on the typed (kana-normalized) form.
        for seq in self.db().dict_search_seqs(&search, 30)? {
            if seen.insert(seq) {
                if let Some(hit) = self.build_hit(seq)? {
                    words.push(hit);
                }
            }
        }

        // Kanji cards: from the query itself, then from top headwords.
        let mut seen_kanji = HashSet::new();
        let mut kanji = Vec::new();
        let mut add_from = |text: &str, kanji: &mut Vec<KanjiRow>| -> Result<()> {
            for c in text.chars() {
                if kanji.len() >= 6 {
                    break;
                }
                let s = c.to_string();
                if shiori_nlp::kana::contains_kanji(&s) && seen_kanji.insert(c) {
                    if let Some(row) = self.db().kanji(&s)? {
                        kanji.push(row);
                    }
                }
            }
            Ok(())
        };
        add_from(&search, &mut kanji)?;
        if kanji.is_empty() {
            for hit in words.iter().take(3) {
                add_from(hit.entry.headword(), &mut kanji)?;
            }
        }

        Ok(DictSearchResults {
            words,
            kanji,
            analysis,
        })
    }

    /// Example sentences from the user's library that use a tracked word —
    /// the contexts it appears in across imported books, i.e. the material
    /// feeding the SRS. Each carries the byte ranges where the word appears,
    /// so callers can highlight it. Empty until the word turns up in
    /// something read.
    pub fn word_examples(&self, word_id: WordId, limit: u32) -> Result<Vec<DictExample>> {
        let mut out = Vec::new();
        for (sentence, title) in self
            .db()
            .word_example_sentences(word_id, None, None, limit)?
        {
            let highlights = self
                .db()
                .sentence_tokens(sentence.id)?
                .into_iter()
                .filter(|t| t.word_id == word_id)
                .map(|t| (t.token.start, t.token.end))
                .collect();
            out.push(DictExample {
                sentence,
                title,
                highlights,
            });
        }
        Ok(out)
    }

    /// Build a search hit from a dictionary sequence id, resolving its
    /// tracked word and JLPT level. Returns `None` for a missing or
    /// corrupt entry.
    fn build_hit(&self, seq: i64) -> Result<Option<DictSearchHit>> {
        let Some(json) = self.db().dict_entry_json(seq)? else {
            return Ok(None);
        };
        let Ok(entry) = serde_json::from_str::<DictEntry>(&json) else {
            return Ok(None);
        };
        let word = self
            .db()
            .words_by_lemma(entry.headword())?
            .into_iter()
            .next();
        let kanji = entry.kanji.first().map(|f| f.text.as_str()).unwrap_or("");
        let jlpt = self.db().jlpt_level(kanji, entry.reading())?;
        Ok(Some(DictSearchHit { entry, word, jlpt }))
    }

    /// If the query is a conjugated or compounded Japanese form, describe
    /// it: its dictionary root and the grammar of its tail. Plain
    /// dictionary forms (a bare noun, an unconjugated verb) return `None`.
    fn analyze_query(&self, text: &str) -> Option<QueryAnalysis> {
        if !shiori_nlp::kana::is_japanese(text) {
            return None;
        }
        let tokens = self.analyzer().tokenize_sentence(text).ok()?;
        let groups = shiori_nlp::phrase_groups(&tokens);
        let &(start, end) = groups.first()?;
        let group = &tokens[start..end];
        let head = group.first()?;
        if !head.pos.is_lexical() {
            return None;
        }
        let surface: String = group.iter().map(|t| t.surface.as_str()).collect();
        let inflection = shiori_nlp::analyze_inflection(group);
        // Only worth surfacing when the typed form is not already the plain
        // dictionary form (i.e. it is conjugated or compounded).
        if inflection.is_plain() && surface == head.lemma {
            return None;
        }
        Some(QueryAnalysis {
            surface,
            lemma: head.lemma.clone(),
            reading: head.reading.clone(),
            pos: head.pos,
            inflection,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiori_db::DictFormRow;

    fn app_with_dict() -> App {
        let app = App::with_db(
            shiori_db::Db::open_in_memory().unwrap(),
            std::env::temp_dir(),
        )
        .unwrap();
        let neko_json = serde_json::json!({
            "id": "1467640",
            "kanji": [{"common": true, "text": "猫", "tags": []}],
            "kana": [{"common": true, "text": "ねこ", "tags": [], "appliesToKanji": ["*"]}],
            "sense": [{
                "partOfSpeech": ["n"], "appliesToKanji": ["*"], "appliesToKana": ["*"],
                "related": [], "antonym": [], "field": [], "dialect": [],
                "misc": [], "info": [], "languageSource": [],
                "gloss": [{"lang": "eng", "gender": null, "type": null, "text": "cat"}]
            }]
        })
        .to_string();
        let taberu_json = serde_json::json!({
            "id": "1358280",
            "kanji": [{"common": true, "text": "食べる", "tags": []}],
            "kana": [{"common": true, "text": "たべる", "tags": [], "appliesToKanji": ["*"]}],
            "sense": [{
                "partOfSpeech": ["v1", "vt"], "appliesToKanji": ["*"], "appliesToKana": ["*"],
                "related": [], "antonym": [], "field": [], "dialect": [],
                "misc": [], "info": [], "languageSource": [],
                "gloss": [{"lang": "eng", "gender": null, "type": null, "text": "to eat"}]
            }]
        })
        .to_string();
        app.db()
            .import_dictionary(vec![
                (
                    1467640,
                    neko_json,
                    vec![
                        DictFormRow {
                            text: "猫".into(),
                            is_kana: false,
                            is_common: true,
                        },
                        DictFormRow {
                            text: "ねこ".into(),
                            is_kana: true,
                            is_common: true,
                        },
                    ],
                ),
                (
                    1358280,
                    taberu_json,
                    vec![
                        DictFormRow {
                            text: "食べる".into(),
                            is_kana: false,
                            is_common: true,
                        },
                        DictFormRow {
                            text: "たべる".into(),
                            is_kana: true,
                            is_common: true,
                        },
                    ],
                ),
            ])
            .unwrap();
        app.db()
            .import_kanji(vec![shiori_db::KanjiRow {
                literal: "猫".into(),
                grade: Some(8),
                stroke_count: 11,
                jlpt: Some(2),
                freq: None,
                on_readings: vec!["ビョウ".into()],
                kun_readings: vec!["ねこ".into()],
                nanori: vec![],
                meanings: vec!["cat".into()],
                variants: vec![],
                strokes: vec![],
            }])
            .unwrap();
        app.db()
            .import_jlpt(vec![(5, "猫".into(), "ねこ".into())])
            .unwrap();
        app
    }

    #[test]
    fn search_finds_words_and_kanji() {
        let app = app_with_dict();
        let results = app.search_dictionary("猫").unwrap();
        assert_eq!(results.words.len(), 1);
        assert_eq!(results.words[0].entry.headword(), "猫");
        assert_eq!(results.words[0].jlpt, Some(5));
        assert!(results.analysis.is_none(), "a bare noun is not conjugated");
        assert_eq!(results.kanji.len(), 1);
        assert_eq!(results.kanji[0].stroke_count, 11);
    }

    #[test]
    fn kana_search_pulls_kanji_from_headwords() {
        let app = app_with_dict();
        let results = app.search_dictionary("ねこ").unwrap();
        assert_eq!(results.words.len(), 1);
        // The query has no kanji, so cards come from the hit's headword.
        assert_eq!(results.kanji.len(), 1);
        assert_eq!(results.kanji[0].literal, "猫");
    }

    #[test]
    fn romaji_query_is_transliterated() {
        let app = app_with_dict();
        // Lower-case rōmaji → hiragana → finds the kana headword.
        let results = app.search_dictionary("neko").unwrap();
        assert_eq!(results.words.len(), 1);
        assert_eq!(results.words[0].entry.headword(), "猫");
    }

    #[test]
    fn conjugated_query_finds_root_and_is_analyzed() {
        let app = app_with_dict();
        // 食べました is not a dictionary form, but it must surface 食べる.
        let results = app.search_dictionary("食べました").unwrap();
        assert!(
            results.words.iter().any(|h| h.entry.headword() == "食べる"),
            "the dictionary root must be found"
        );
        let analysis = results.analysis.expect("conjugation should be analyzed");
        assert_eq!(analysis.lemma, "食べる");
        let summary = analysis.inflection.summary.expect("polite past summary");
        assert!(summary.contains("ました"), "{summary}");
    }

    #[test]
    fn conjugated_romaji_query_finds_root() {
        let app = app_with_dict();
        // rōmaji + conjugation together: tabemashita → たべました → 食べる.
        // The all-kana form lemmatizes to the kana root たべる, which still
        // resolves to the 食べる entry through its kana spelling.
        let results = app.search_dictionary("tabemashita").unwrap();
        assert!(results.words.iter().any(|h| h.entry.headword() == "食べる"));
        let analysis = results.analysis.expect("conjugation should be analyzed");
        assert_eq!(analysis.lemma, "たべる");
        assert!(analysis
            .inflection
            .summary
            .as_deref()
            .is_some_and(|s| s.contains("ました")));
    }

    #[test]
    fn empty_query_is_empty() {
        let app = app_with_dict();
        let results = app.search_dictionary("  ").unwrap();
        assert!(results.words.is_empty() && results.kanji.is_empty());
    }

    #[test]
    fn word_examples_carry_highlight_ranges() {
        use shiori_db::{NewSentence, NewToken};
        let app = app_with_dict();
        // 猫 sits after その (two 3-byte kana) in this sentence.
        let text = "その猫が好き。";
        let neko_start = "その".len(); // 6
        let neko_end = neko_start + "猫".len(); // 9
        let sentences = vec![NewSentence {
            paragraph: 0,
            text: text.into(),
            tokens: vec![
                NewToken {
                    surface: "その".into(),
                    lemma: "その".into(),
                    reading: "その".into(),
                    pos: PartOfSpeech::Prenominal,
                    start: 0,
                    end: neko_start,
                },
                NewToken {
                    surface: "猫".into(),
                    lemma: "猫".into(),
                    reading: "ねこ".into(),
                    pos: PartOfSpeech::Noun,
                    start: neko_start,
                    end: neko_end,
                },
                NewToken {
                    surface: "が".into(),
                    lemma: "が".into(),
                    reading: "が".into(),
                    pos: PartOfSpeech::Particle,
                    start: neko_end,
                    end: neko_end + "が".len(),
                },
            ],
        }];
        app.db()
            .import_document(
                &shiori_core::DocumentMeta {
                    title: "fixture".into(),
                    ..Default::default()
                },
                "hash-examples",
                chrono::Utc::now(),
                &sentences,
            )
            .unwrap();

        let word = app
            .db()
            .words_by_lemma("猫")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let examples = app.word_examples(word.id, 10).unwrap();
        assert_eq!(examples.len(), 1);
        assert_eq!(examples[0].sentence.text, text);
        // The looked-up word's byte range is reported for highlighting.
        assert_eq!(examples[0].highlights, vec![(neko_start, neko_end)]);
    }
}
