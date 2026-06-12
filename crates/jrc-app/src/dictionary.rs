//! Dictionary search: one query returns word entries and kanji cards.

use jrc_db::{KanjiRow, WordRow};
use jrc_dict::DictEntry;

use crate::{App, Result};

/// One word result of a dictionary search.
#[derive(Debug)]
pub struct DictSearchHit {
    pub entry: DictEntry,
    /// The tracked word sharing the headword's lemma, if the user has
    /// met it while reading (carries knowledge status).
    pub word: Option<WordRow>,
}

/// Everything one search query returns.
#[derive(Debug, Default)]
pub struct DictSearchResults {
    pub words: Vec<DictSearchHit>,
    pub kanji: Vec<KanjiRow>,
}

impl App {
    /// Search the dictionary by Japanese text (exact and prefix forms),
    /// and surface kanji cards for every kanji in the query — or, for a
    /// kana query, in the top hits' headwords.
    pub fn search_dictionary(&self, query: &str) -> Result<DictSearchResults> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(DictSearchResults::default());
        }

        let mut words = Vec::new();
        for seq in self.db().dict_search_seqs(query, 30)? {
            let Some(json) = self.db().dict_entry_json(seq)? else { continue };
            let Ok(entry) = serde_json::from_str::<DictEntry>(&json) else { continue };
            let word = self
                .db()
                .words_by_lemma(entry.headword())?
                .into_iter()
                .next();
            words.push(DictSearchHit { entry, word });
        }

        // Kanji cards: from the query itself, then from top headwords.
        let mut seen = std::collections::HashSet::new();
        let mut kanji = Vec::new();
        let mut add_from = |text: &str, kanji: &mut Vec<KanjiRow>| -> Result<()> {
            for c in text.chars() {
                if kanji.len() >= 6 {
                    break;
                }
                let s = c.to_string();
                if jrc_nlp::kana::contains_kanji(&s) && seen.insert(c) {
                    if let Some(row) = self.db().kanji(&s)? {
                        kanji.push(row);
                    }
                }
            }
            Ok(())
        };
        add_from(query, &mut kanji)?;
        if kanji.is_empty() {
            for hit in words.iter().take(3) {
                add_from(hit.entry.headword(), &mut kanji)?;
            }
        }

        Ok(DictSearchResults { words, kanji })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jrc_db::DictFormRow;

    fn app_with_dict() -> App {
        let app =
            App::with_db(jrc_db::Db::open_in_memory().unwrap(), std::env::temp_dir()).unwrap();
        let entry_json = serde_json::json!({
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
        app.db()
            .import_dictionary(vec![(
                1467640,
                entry_json,
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
            )])
            .unwrap();
        app.db()
            .import_kanji(vec![jrc_db::KanjiRow {
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
        app
    }

    #[test]
    fn search_finds_words_and_kanji() {
        let app = app_with_dict();
        let results = app.search_dictionary("猫").unwrap();
        assert_eq!(results.words.len(), 1);
        assert_eq!(results.words[0].entry.headword(), "猫");
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
    fn empty_query_is_empty() {
        let app = app_with_dict();
        let results = app.search_dictionary("  ").unwrap();
        assert!(results.words.is_empty() && results.kanji.is_empty());
    }
}
