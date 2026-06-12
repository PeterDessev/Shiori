//! Reading-session storage: active reading time and characters read,
//! one row per continuous sitting with a document.

use chrono::{DateTime, Utc};
use rusqlite::params;
use shiori_core::DocumentId;

use crate::{Db, Result};

/// Aggregate reading activity, for velocity and per-book time stats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReadingTotals {
    pub seconds: f64,
    pub chars: u64,
}

impl Db {
    /// Open a new (empty) reading session row; time is added as it is
    /// earned via [`Db::add_reading_time`].
    pub fn start_reading_session(&self, document: DocumentId, at: DateTime<Utc>) -> Result<i64> {
        self.conn().execute(
            "INSERT INTO reading_sessions(document_id, started_at, ended_at, seconds, chars)
             VALUES (?1, ?2, ?2, 0, 0)",
            params![document.0, at],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    /// Credit active reading time to a session.
    pub fn add_reading_time(
        &self,
        session: i64,
        seconds: f64,
        chars: u64,
        at: DateTime<Utc>,
    ) -> Result<()> {
        self.conn().execute(
            "UPDATE reading_sessions
             SET seconds = seconds + ?2, chars = chars + ?3, ended_at = ?4
             WHERE id = ?1",
            params![session, seconds, chars as i64, at],
        )?;
        Ok(())
    }

    /// Total credited reading across all documents.
    pub fn reading_totals(&self) -> Result<ReadingTotals> {
        self.conn()
            .query_row(
                "SELECT COALESCE(SUM(seconds), 0), COALESCE(SUM(chars), 0)
                 FROM reading_sessions",
                [],
                |r| {
                    Ok(ReadingTotals {
                        seconds: r.get(0)?,
                        chars: r.get::<_, i64>(1)? as u64,
                    })
                },
            )
            .map_err(Into::into)
    }

    /// Total credited reading for one document.
    pub fn document_reading_totals(&self, document: DocumentId) -> Result<ReadingTotals> {
        self.conn()
            .query_row(
                "SELECT COALESCE(SUM(seconds), 0), COALESCE(SUM(chars), 0)
                 FROM reading_sessions WHERE document_id = ?1",
                [document.0],
                |r| {
                    Ok(ReadingTotals {
                        seconds: r.get(0)?,
                        chars: r.get::<_, i64>(1)? as u64,
                    })
                },
            )
            .map_err(Into::into)
    }

    /// Credited seconds per calendar day (UTC), oldest first.
    pub fn reading_seconds_by_day(&self) -> Result<Vec<(String, f64)>> {
        let mut stmt = self.conn().prepare(
            "SELECT date(started_at), SUM(seconds) FROM reading_sessions
             GROUP BY date(started_at) ORDER BY date(started_at)",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::tests::import_fixture;

    #[test]
    fn sessions_accumulate_and_aggregate() {
        let db = Db::open_in_memory().unwrap();
        let doc = import_fixture(&db);
        let now = Utc::now();

        let s1 = db.start_reading_session(doc, now).unwrap();
        db.add_reading_time(s1, 120.0, 800, now).unwrap();
        db.add_reading_time(s1, 60.0, 400, now).unwrap();
        let s2 = db.start_reading_session(doc, now).unwrap();
        db.add_reading_time(s2, 30.0, 200, now).unwrap();

        let totals = db.reading_totals().unwrap();
        assert_eq!(totals.seconds, 210.0);
        assert_eq!(totals.chars, 1400);

        let doc_totals = db.document_reading_totals(doc).unwrap();
        assert_eq!(doc_totals.seconds, 210.0);

        let by_day = db.reading_seconds_by_day().unwrap();
        assert_eq!(by_day.len(), 1);
        assert_eq!(by_day[0].1, 210.0);
    }

    #[test]
    fn empty_totals_are_zero() {
        let db = Db::open_in_memory().unwrap();
        let totals = db.reading_totals().unwrap();
        assert_eq!(totals.seconds, 0.0);
        assert_eq!(totals.chars, 0);
    }

    #[test]
    fn sessions_cascade_with_document() {
        let db = Db::open_in_memory().unwrap();
        let doc = import_fixture(&db);
        let s = db.start_reading_session(doc, Utc::now()).unwrap();
        db.add_reading_time(s, 10.0, 50, Utc::now()).unwrap();
        db.delete_document(doc).unwrap();
        assert_eq!(db.reading_totals().unwrap().seconds, 0.0);
    }
}
