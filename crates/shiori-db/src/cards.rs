//! SRS card storage and review log.

use chrono::{DateTime, Utc};
use rusqlite::params;
use shiori_core::{SentenceId, WordId};
use shiori_srs::{Card, CardState, Rating};

use crate::{Db, Result};

/// A stored card: scheduling state plus the sentence that provides its
/// reading context.
#[derive(Debug, Clone)]
pub struct CardRow {
    pub word_id: WordId,
    pub sentence_id: Option<SentenceId>,
    pub card: Card,
}

fn row_to_card(r: &rusqlite::Row<'_>) -> rusqlite::Result<CardRow> {
    Ok(CardRow {
        word_id: WordId(r.get(0)?),
        sentence_id: r.get::<_, Option<i64>>(1)?.map(SentenceId),
        card: Card {
            state: CardState::from_str_lossy(&r.get::<_, String>(2)?),
            stability: r.get(3)?,
            difficulty: r.get(4)?,
            due: r.get(5)?,
            last_review: r.get(6)?,
            reps: r.get::<_, i64>(7)? as u32,
            lapses: r.get::<_, i64>(8)? as u32,
            step: r.get::<_, i64>(9)? as u32,
        },
    })
}

const CARD_COLS: &str =
    "word_id, sentence_id, state, stability, difficulty, due, last_review, reps, lapses, step";

impl Db {
    /// Create or update the card for a word.
    pub fn upsert_card(
        &self,
        word_id: WordId,
        sentence_id: Option<SentenceId>,
        card: &Card,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO cards(word_id, sentence_id, state, stability, difficulty,
                               due, last_review, reps, lapses, step)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(word_id) DO UPDATE SET
                sentence_id = excluded.sentence_id,
                state = excluded.state,
                stability = excluded.stability,
                difficulty = excluded.difficulty,
                due = excluded.due,
                last_review = excluded.last_review,
                reps = excluded.reps,
                lapses = excluded.lapses,
                step = excluded.step",
            params![
                word_id.0,
                sentence_id.map(|s| s.0),
                card.state.as_str(),
                card.stability,
                card.difficulty,
                card.due,
                card.last_review,
                card.reps as i64,
                card.lapses as i64,
                card.step as i64,
            ],
        )?;
        Ok(())
    }

    pub fn card(&self, word_id: WordId) -> Result<Option<CardRow>> {
        let result = self.conn().query_row(
            &format!("SELECT {CARD_COLS} FROM cards WHERE word_id = ?1"),
            [word_id.0],
            row_to_card,
        );
        match result {
            Ok(c) => Ok(Some(c)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn delete_card(&self, word_id: WordId) -> Result<()> {
        self.conn()
            .execute("DELETE FROM cards WHERE word_id = ?1", [word_id.0])?;
        Ok(())
    }

    /// Every card (for exports).
    pub fn all_cards(&self) -> Result<Vec<CardRow>> {
        let mut stmt = self
            .conn()
            .prepare(&format!("SELECT {CARD_COLS} FROM cards ORDER BY word_id"))?;
        let rows = stmt.query_map([], row_to_card)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Cards due at `now`, most overdue first.
    pub fn due_cards(&self, now: DateTime<Utc>, limit: u32) -> Result<Vec<CardRow>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {CARD_COLS} FROM cards WHERE due <= ?1 ORDER BY due LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![now, limit as i64], row_to_card)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn due_count(&self, now: DateTime<Utc>) -> Result<u64> {
        let n: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM cards WHERE due <= ?1",
            params![now],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    pub fn card_count(&self) -> Result<u64> {
        let n: i64 = self
            .conn()
            .query_row("SELECT COUNT(*) FROM cards", [], |r| r.get(0))?;
        Ok(n as u64)
    }

    /// Append one review to the log.
    pub fn log_review(
        &self,
        word_id: WordId,
        rating: Rating,
        reviewed_at: DateTime<Utc>,
        stability: f64,
        difficulty: f64,
    ) -> Result<()> {
        self.conn().execute(
            "INSERT INTO review_log(word_id, rating, reviewed_at, stability, difficulty)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![word_id.0, rating as i64, reviewed_at, stability, difficulty],
        )?;
        Ok(())
    }

    pub fn review_count(&self) -> Result<u64> {
        let n: i64 = self
            .conn()
            .query_row("SELECT COUNT(*) FROM review_log", [], |r| r.get(0))?;
        Ok(n as u64)
    }

    /// Number of reviews done on the calendar day containing `now` (UTC).
    pub fn reviews_on_day(&self, now: DateTime<Utc>) -> Result<u64> {
        let day_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
        let day_end = day_start + chrono::Duration::days(1);
        let n: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM review_log WHERE reviewed_at >= ?1 AND reviewed_at < ?2",
            params![day_start, day_end],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::tests::import_fixture;
    use shiori_core::{KnowledgeStatus, PartOfSpeech, WordKey};
    use shiori_srs::Scheduler;

    fn word_id(db: &Db, lemma: &str, reading: &str, pos: PartOfSpeech) -> WordId {
        db.find_word(&WordKey::new(lemma, reading, pos))
            .unwrap()
            .unwrap()
            .id
    }

    #[test]
    fn card_roundtrip_and_due_query() {
        let db = Db::open_in_memory().unwrap();
        let doc = import_fixture(&db);
        let sentences = db.sentences(doc).unwrap();
        let now = Utc::now();

        let cat = word_id(&db, "猫", "ねこ", PartOfSpeech::Noun);
        let run = word_id(&db, "走る", "はしる", PartOfSpeech::Verb);

        let scheduler = Scheduler::default();
        let card_due = scheduler.review(&Card::new(now), shiori_srs::Rating::Again, now);
        let card_future = scheduler.review(&Card::new(now), shiori_srs::Rating::Easy, now);

        db.upsert_card(cat, Some(sentences[0].id), &card_due)
            .unwrap();
        db.upsert_card(run, Some(sentences[1].id), &card_future)
            .unwrap();
        db.set_word_status(cat, KnowledgeStatus::Learning).unwrap();
        db.set_word_status(run, KnowledgeStatus::Learning).unwrap();

        // Stored card round-trips exactly.
        let stored = db.card(cat).unwrap().unwrap();
        assert_eq!(stored.card, card_due);
        assert_eq!(stored.sentence_id, Some(sentences[0].id));

        // Only the Again-card is due within 5 minutes.
        let soon = now + chrono::Duration::minutes(5);
        let due = db.due_cards(soon, 10).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].word_id, cat);
        assert_eq!(db.due_count(soon).unwrap(), 1);
        assert_eq!(db.card_count().unwrap(), 2);

        // Upsert updates in place.
        let updated = scheduler.review(&card_due, shiori_srs::Rating::Good, soon);
        db.upsert_card(cat, Some(sentences[0].id), &updated)
            .unwrap();
        assert_eq!(db.card_count().unwrap(), 2);
        assert_eq!(db.card(cat).unwrap().unwrap().card, updated);
    }

    #[test]
    fn review_log_counts() {
        let db = Db::open_in_memory().unwrap();
        import_fixture(&db);
        let now = Utc::now();
        let cat = word_id(&db, "猫", "ねこ", PartOfSpeech::Noun);

        db.log_review(cat, Rating::Good, now, 3.0, 5.0).unwrap();
        db.log_review(cat, Rating::Again, now, 1.0, 6.0).unwrap();
        db.log_review(cat, Rating::Good, now - chrono::Duration::days(2), 2.0, 5.0)
            .unwrap();

        assert_eq!(db.review_count().unwrap(), 3);
        assert_eq!(db.reviews_on_day(now).unwrap(), 2);
    }

    #[test]
    fn delete_card() {
        let db = Db::open_in_memory().unwrap();
        import_fixture(&db);
        let now = Utc::now();
        let cat = word_id(&db, "猫", "ねこ", PartOfSpeech::Noun);
        db.upsert_card(cat, None, &Card::new(now)).unwrap();
        assert!(db.card(cat).unwrap().is_some());
        db.delete_card(cat).unwrap();
        assert!(db.card(cat).unwrap().is_none());
    }
}
