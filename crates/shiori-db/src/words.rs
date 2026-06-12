//! Word knowledge state.

use rusqlite::params;
use shiori_core::{DocumentId, KnowledgeStatus, PartOfSpeech, SentenceId, WordId, WordKey};

use crate::{Db, DbError, Result};

/// A tracked word with its knowledge state.
#[derive(Debug, Clone)]
pub struct WordRow {
    pub id: WordId,
    pub key: WordKey,
    pub status: KnowledgeStatus,
    /// JMdict sequence id this word resolved to, if any.
    pub dict_seq: Option<i64>,
}

/// A word as it occurs within one document.
#[derive(Debug, Clone)]
pub struct DocWord {
    pub word: WordRow,
    /// Number of occurrences in the document.
    pub occurrences: u32,
    /// First sentence the word appears in (natural card context).
    pub first_sentence_id: SentenceId,
}

fn row_to_word(r: &rusqlite::Row<'_>) -> rusqlite::Result<WordRow> {
    Ok(WordRow {
        id: WordId(r.get(0)?),
        key: WordKey {
            lemma: r.get(1)?,
            reading: r.get(2)?,
            pos: PartOfSpeech::from_str_lossy(&r.get::<_, String>(3)?),
        },
        status: KnowledgeStatus::from_str_lossy(&r.get::<_, String>(4)?),
        dict_seq: r.get(5)?,
    })
}

const WORD_COLS: &str = "id, lemma, reading, pos, status, dict_seq";

impl Db {
    pub fn word(&self, id: WordId) -> Result<WordRow> {
        self.conn()
            .query_row(
                &format!("SELECT {WORD_COLS} FROM words WHERE id = ?1"),
                [id.0],
                row_to_word,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DbError::NotFound("word"),
                e => e.into(),
            })
    }

    /// All tracked words sharing a lemma (any reading/POS).
    pub fn words_by_lemma(&self, lemma: &str) -> Result<Vec<WordRow>> {
        let mut stmt = self
            .conn()
            .prepare(&format!("SELECT {WORD_COLS} FROM words WHERE lemma = ?1"))?;
        let rows = stmt.query_map([lemma], row_to_word)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The word for a key, inserted at the default status if new.
    pub fn ensure_word(&self, key: &WordKey) -> Result<WordRow> {
        if let Some(word) = self.find_word(key)? {
            return Ok(word);
        }
        self.conn().execute(
            "INSERT INTO words(lemma, reading, pos) VALUES (?1, ?2, ?3)",
            params![key.lemma, key.reading, key.pos.as_str()],
        )?;
        self.find_word(key)?.ok_or(DbError::NotFound("word"))
    }

    pub fn find_word(&self, key: &WordKey) -> Result<Option<WordRow>> {
        let result = self.conn().query_row(
            &format!(
                "SELECT {WORD_COLS} FROM words WHERE lemma = ?1 AND reading = ?2 AND pos = ?3"
            ),
            params![key.lemma, key.reading, key.pos.as_str()],
            row_to_word,
        );
        match result {
            Ok(w) => Ok(Some(w)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_word_status(&self, id: WordId, status: KnowledgeStatus) -> Result<()> {
        let n = self.conn().execute(
            "UPDATE words SET status = ?2 WHERE id = ?1",
            params![id.0, status.as_str()],
        )?;
        if n == 0 {
            return Err(DbError::NotFound("word"));
        }
        Ok(())
    }

    /// Set many words to one status in a single transaction.
    pub fn bulk_set_status(&self, ids: &[WordId], status: KnowledgeStatus) -> Result<()> {
        let tx = self.conn().unchecked_transaction()?;
        {
            let mut stmt = tx.prepare("UPDATE words SET status = ?2 WHERE id = ?1")?;
            for id in ids {
                stmt.execute(params![id.0, status.as_str()])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Corpus frequency ranks of every known word that has one, sorted
    /// ascending. The shape of this distribution is the user's "known
    /// band" for missed-word detection.
    pub fn known_word_ranks(&self) -> Result<Vec<u32>> {
        let mut stmt = self.conn().prepare(
            "SELECT f.rank FROM words w JOIN frequency f ON f.word = w.lemma
             WHERE w.status = 'known' ORDER BY f.rank",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, i64>(0).map(|n| n as u32))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn set_word_dict_seq(&self, id: WordId, seq: Option<i64>) -> Result<()> {
        self.conn().execute(
            "UPDATE words SET dict_seq = ?2 WHERE id = ?1",
            params![id.0, seq],
        )?;
        Ok(())
    }

    /// Global counts of words per knowledge status.
    pub fn word_status_counts(&self) -> Result<Vec<(KnowledgeStatus, u32)>> {
        let mut stmt = self
            .conn()
            .prepare("SELECT status, COUNT(*) FROM words GROUP BY status")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                KnowledgeStatus::from_str_lossy(&r.get::<_, String>(0)?),
                r.get::<_, i64>(1)? as u32,
            ))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Distinct words of a document with occurrence counts and the first
    /// sentence each appears in, most frequent first.
    pub fn document_words(&self, document: DocumentId) -> Result<Vec<DocWord>> {
        let mut stmt = self.conn().prepare(
            "SELECT w.id, w.lemma, w.reading, w.pos, w.status, w.dict_seq,
                    COUNT(*) AS occurrences,
                    MIN(s.id) AS first_sentence
             FROM tokens t
             JOIN sentences s ON s.id = t.sentence_id
             JOIN words w ON w.id = t.word_id
             WHERE s.document_id = ?1
             GROUP BY w.id
             ORDER BY occurrences DESC, w.id",
        )?;
        let rows = stmt.query_map([document.0], |r| {
            Ok(DocWord {
                word: row_to_word(r)?,
                occurrences: r.get::<_, i64>(6)? as u32,
                first_sentence_id: SentenceId(r.get(7)?),
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::tests::import_fixture;

    #[test]
    fn find_and_update_word_status() {
        let db = Db::open_in_memory().unwrap();
        import_fixture(&db);

        let key = WordKey::new("猫", "ねこ", PartOfSpeech::Noun);
        let word = db.find_word(&key).unwrap().expect("猫 was imported");
        assert_eq!(word.status, KnowledgeStatus::Unknown);

        db.set_word_status(word.id, KnowledgeStatus::Known).unwrap();
        assert_eq!(db.word(word.id).unwrap().status, KnowledgeStatus::Known);

        db.set_word_dict_seq(word.id, Some(1467640)).unwrap();
        assert_eq!(db.word(word.id).unwrap().dict_seq, Some(1467640));
    }

    #[test]
    fn missing_word_errors() {
        let db = Db::open_in_memory().unwrap();
        assert!(matches!(
            db.word(WordId(999)),
            Err(DbError::NotFound("word"))
        ));
        assert!(db
            .find_word(&WordKey::new("ない", "ない", PartOfSpeech::Noun))
            .unwrap()
            .is_none());
        assert!(db
            .set_word_status(WordId(999), KnowledgeStatus::Known)
            .is_err());
    }

    #[test]
    fn document_words_aggregates_occurrences() {
        let db = Db::open_in_memory().unwrap();
        let doc = import_fixture(&db);
        let words = db.document_words(doc).unwrap();

        // 猫 occurs twice and must sort first.
        assert_eq!(words[0].word.key.lemma, "猫");
        assert_eq!(words[0].occurrences, 2);
        // Its first occurrence is in the first sentence.
        let sentences = db.sentences(doc).unwrap();
        assert_eq!(words[0].first_sentence_id, sentences[0].id);
        // 7 distinct words total (猫 deduplicated).
        assert_eq!(words.len(), 7);
    }

    #[test]
    fn status_counts() {
        let db = Db::open_in_memory().unwrap();
        import_fixture(&db);
        let counts = db.word_status_counts().unwrap();
        let unknown = counts
            .iter()
            .find(|(s, _)| *s == KnowledgeStatus::Unknown)
            .unwrap();
        assert_eq!(unknown.1, 7);
    }
}
