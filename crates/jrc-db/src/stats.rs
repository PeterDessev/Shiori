//! Aggregate queries for reading-difficulty statistics.

use jrc_core::{DocumentId, KnowledgeStatus};

use crate::{Db, Result};

/// Per-status aggregate over a document's content words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusCount {
    pub status: KnowledgeStatus,
    /// Distinct words with this status.
    pub words: u32,
    /// Token occurrences with this status.
    pub tokens: u32,
}

/// SQL fragment matching `PartOfSpeech::is_content_word` — function words
/// are excluded from comprehension statistics. Keep in sync with
/// `jrc_core::PartOfSpeech::is_content_word` (verified by test below).
const NON_CONTENT_POS: &str = "('particle', 'auxiliary_verb', 'symbol', 'number', 'prefix', \
                               'suffix', 'dependent_noun', 'unknown')";

impl Db {
    /// Status breakdown over the *content words* of a document.
    pub fn document_status_counts(&self, document: DocumentId) -> Result<Vec<StatusCount>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT w.status, COUNT(DISTINCT w.id), COUNT(*)
             FROM tokens t
             JOIN sentences s ON s.id = t.sentence_id
             JOIN words w ON w.id = t.word_id
             WHERE s.document_id = ?1 AND w.pos NOT IN {NON_CONTENT_POS}
             GROUP BY w.status"
        ))?;
        let rows = stmt.query_map([document.0], |r| {
            Ok(StatusCount {
                status: KnowledgeStatus::from_str_lossy(&r.get::<_, String>(0)?),
                words: r.get::<_, i64>(1)? as u32,
                tokens: r.get::<_, i64>(2)? as u32,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::tests::import_fixture;
    use jrc_core::{PartOfSpeech, WordKey};

    #[test]
    fn non_content_pos_list_matches_core() {
        // The SQL filter and PartOfSpeech::is_content_word must agree.
        let all = [
            PartOfSpeech::Noun,
            PartOfSpeech::ProperNoun,
            PartOfSpeech::Pronoun,
            PartOfSpeech::DependentNoun,
            PartOfSpeech::Verb,
            PartOfSpeech::Adjective,
            PartOfSpeech::AdjectivalNoun,
            PartOfSpeech::Adverb,
            PartOfSpeech::Particle,
            PartOfSpeech::AuxiliaryVerb,
            PartOfSpeech::Conjunction,
            PartOfSpeech::Prenominal,
            PartOfSpeech::Interjection,
            PartOfSpeech::Number,
            PartOfSpeech::Prefix,
            PartOfSpeech::Suffix,
            PartOfSpeech::Symbol,
            PartOfSpeech::Unknown,
        ];
        for pos in all {
            let in_sql_list = NON_CONTENT_POS.contains(&format!("'{}'", pos.as_str()));
            assert_eq!(
                in_sql_list,
                !pos.is_content_word(),
                "SQL filter and is_content_word disagree on {pos:?}"
            );
        }
    }

    #[test]
    fn status_counts_cover_content_words_only() {
        let db = Db::open_in_memory().unwrap();
        let doc = import_fixture(&db);

        // Fixture content words: 猫 (x2), 好き, その(prenominal), 走る.
        // Function words: が, は (particles), だ (aux).
        let counts = db.document_status_counts(doc).unwrap();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].status, KnowledgeStatus::Unknown);
        assert_eq!(counts[0].words, 4);
        assert_eq!(counts[0].tokens, 5);

        // Mark 猫 known: 1 word / 2 tokens move over.
        let cat = db
            .find_word(&WordKey::new("猫", "ねこ", PartOfSpeech::Noun))
            .unwrap()
            .unwrap();
        db.set_word_status(cat.id, KnowledgeStatus::Known).unwrap();

        let counts = db.document_status_counts(doc).unwrap();
        let known = counts
            .iter()
            .find(|c| c.status == KnowledgeStatus::Known)
            .unwrap();
        let unknown = counts
            .iter()
            .find(|c| c.status == KnowledgeStatus::Unknown)
            .unwrap();
        assert_eq!((known.words, known.tokens), (1, 2));
        assert_eq!((unknown.words, unknown.tokens), (3, 3));
    }
}
