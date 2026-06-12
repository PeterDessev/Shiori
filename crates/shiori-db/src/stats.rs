//! Aggregate queries for reading-difficulty statistics.

use shiori_core::{DocumentId, KnowledgeStatus};

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
/// `shiori_core::PartOfSpeech::is_content_word` (verified by test below).
const NON_CONTENT_POS: &str = "('particle', 'auxiliary_verb', 'symbol', 'number', 'prefix', \
                               'suffix', 'dependent_noun', 'unknown')";

/// Known-word share of one JLPT level's vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JlptShare {
    /// 5 (easiest) … 1 (hardest).
    pub level: u8,
    pub known: u32,
    pub total: u32,
}

impl Db {
    /// Replace the JLPT vocabulary lists.
    pub fn import_jlpt<I>(&self, words: I) -> Result<u64>
    where
        I: IntoIterator<Item = (u8, String, String)>,
    {
        let tx = self.conn().unchecked_transaction()?;
        tx.execute("DELETE FROM jlpt_words", [])?;
        let mut count = 0u64;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO jlpt_words(level, word, kana) VALUES (?1, ?2, ?3)",
            )?;
            for (level, word, kana) in words {
                stmt.execute(rusqlite::params![level, word, kana])?;
                count += 1;
            }
        }
        tx.commit()?;
        Ok(count)
    }

    pub fn jlpt_count(&self) -> Result<u64> {
        Ok(self
            .conn()
            .query_row("SELECT COUNT(*) FROM jlpt_words", [], |r| {
                r.get::<_, i64>(0)
            })? as u64)
    }

    /// Per level: how much of that level's vocabulary the user knows.
    /// Kanji-form words match on lemma; kana-only words match on a
    /// kana lemma.
    pub fn jlpt_known_shares(&self) -> Result<Vec<JlptShare>> {
        let mut stmt = self.conn().prepare(
            "SELECT j.level, COUNT(*),
                    SUM(EXISTS(
                        SELECT 1 FROM words w
                        WHERE w.status = 'known'
                          AND w.lemma = CASE WHEN j.word = '' THEN j.kana ELSE j.word END
                    ))
             FROM jlpt_words j GROUP BY j.level ORDER BY j.level DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(JlptShare {
                level: r.get::<_, i64>(0)? as u8,
                total: r.get::<_, i64>(1)? as u32,
                known: r.get::<_, i64>(2)? as u32,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Cards becoming due per day for the next `days` days; overdue
    /// cards count under today.
    pub fn due_forecast(&self, days: u32) -> Result<Vec<(String, u32)>> {
        let mut stmt = self.conn().prepare(
            "SELECT MAX(date(due), date('now')) AS day, COUNT(*)
             FROM cards
             WHERE date(due) <= date('now', '+' || ?1 || ' days')
             GROUP BY day ORDER BY day",
        )?;
        let rows = stmt.query_map([days], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u32)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Words whose first review fell on each day (SRS intake rate).
    pub fn learning_starts_by_day(&self) -> Result<Vec<(String, u32)>> {
        let mut stmt = self.conn().prepare(
            "SELECT day, COUNT(*) FROM (
                 SELECT word_id, date(MIN(reviewed_at)) AS day
                 FROM review_log GROUP BY word_id
             ) GROUP BY day ORDER BY day",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u32)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// (correct, total) reviews within the last `days` days. FSRS Good
    /// ratings count as correct.
    pub fn retention_counts(&self, days: u32) -> Result<(u32, u32)> {
        self.conn()
            .query_row(
                "SELECT COALESCE(SUM(rating >= 3), 0), COUNT(*)
                 FROM review_log
                 WHERE reviewed_at >= datetime('now', '-' || ?1 || ' days')",
                [days],
                |r| Ok((r.get::<_, i64>(0)? as u32, r.get::<_, i64>(1)? as u32)),
            )
            .map_err(Into::into)
    }

    /// Words crossing the given stability for the first time, per day —
    /// the closest reconstructable "vocabulary matured" curve.
    pub fn matured_by_day(&self, stability: f64) -> Result<Vec<(String, u32)>> {
        let mut stmt = self.conn().prepare(
            "SELECT day, COUNT(*) FROM (
                 SELECT word_id, date(MIN(reviewed_at)) AS day
                 FROM review_log WHERE stability >= ?1 GROUP BY word_id
             ) GROUP BY day ORDER BY day",
        )?;
        let rows = stmt.query_map([stability], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u32)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Known-word counts within corpus frequency rank bands: for each
    /// bound, how many of the `bound` most frequent words are known.
    pub fn known_in_rank_bands(&self, bounds: &[u32]) -> Result<Vec<(u32, u32)>> {
        let mut out = Vec::new();
        for &bound in bounds {
            let known: i64 = self.conn().query_row(
                "SELECT COUNT(DISTINCT f.word) FROM frequency f
                 JOIN words w ON w.lemma = f.word AND w.status = 'known'
                 WHERE f.rank <= ?1",
                [bound],
                |r| r.get(0),
            )?;
            out.push((bound, known as u32));
        }
        Ok(out)
    }

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
    use shiori_core::{PartOfSpeech, WordKey};

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
    fn jlpt_shares_match_known_words() {
        let db = Db::open_in_memory().unwrap();
        import_fixture(&db);
        db.import_jlpt(vec![
            (5, "猫".into(), "ねこ".into()),
            (5, "".into(), "する".into()),
            (1, "薔薇".into(), "ばら".into()),
        ])
        .unwrap();
        assert_eq!(db.jlpt_count().unwrap(), 3);

        // Nothing known yet.
        let shares = db.jlpt_known_shares().unwrap();
        assert_eq!(shares.len(), 2);
        assert!(shares.iter().all(|s| s.known == 0));

        // Knowing 猫 moves N5 to 1/2; levels sort easiest-first.
        let cat = db
            .find_word(&WordKey::new("猫", "ねこ", PartOfSpeech::Noun))
            .unwrap()
            .unwrap();
        db.set_word_status(cat.id, KnowledgeStatus::Known).unwrap();
        let shares = db.jlpt_known_shares().unwrap();
        assert_eq!(shares[0].level, 5);
        assert_eq!((shares[0].known, shares[0].total), (1, 2));
        assert_eq!((shares[1].known, shares[1].total), (0, 1));
    }

    #[test]
    fn retention_and_forecast_queries_run() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.retention_counts(30).unwrap(), (0, 0));
        assert!(db.due_forecast(14).unwrap().is_empty());
        assert!(db.learning_starts_by_day().unwrap().is_empty());
        assert!(db.matured_by_day(60.0).unwrap().is_empty());
        assert_eq!(db.known_in_rank_bands(&[1000]).unwrap(), vec![(1000, 0)]);
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
