//! SQLite persistence layer.
//!
//! One database file holds everything: imported documents (down to the
//! token level), the user's per-word knowledge state, SRS cards and review
//! history, plus locally cached dictionary entries and frequency ranks.
//!
//! The crate exposes plain-data rows and keeps policy out: ranking,
//! scheduling decisions and dictionary semantics live in `shiori-app`.
//! Dictionary entries are stored as opaque JSON strings so this crate does
//! not depend on `shiori-dict`.

pub mod anki;
mod cards;
mod chat;
mod dict;
mod documents;
mod kanji;
mod schema;
mod sessions;
mod stats;
mod words;

pub use cards::CardRow;
pub use chat::{ChatAnnotationRow, ChatMessageRow, ConversationRow};
pub use dict::DictFormRow;
pub use documents::{DocumentSummary, NewSentence, NewToken, TokenRow};
pub use kanji::KanjiRow;
pub use sessions::ReadingTotals;
pub use stats::{JlptShare, StatusCount};
pub use words::{DocWord, WordRow};

use std::path::Path;

use rusqlite::Connection;

/// Errors from the persistence layer.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("not found: {0}")]
    NotFound(&'static str),
}

pub type Result<T, E = DbError> = std::result::Result<T, E>;

/// Handle to the application database.
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (creating and migrating if necessary) the database at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            // Creating the parent directory here keeps first-run setup in
            // one place; failure surfaces as the rusqlite open error below.
            let _ = std::fs::create_dir_all(parent);
        }
        Self::from_connection(Connection::open(path)?)
    }

    /// Open a fresh in-memory database (used by tests).
    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let db = Self { conn };
        schema::migrate(&db.conn)?;
        Ok(db)
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Write a clean, single-file copy of the live database (safe while
    /// open; WAL contents are folded in).
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| {
                DbError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
            })?;
        }
        self.conn
            .execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_and_migrates_in_memory() {
        let db = Db::open_in_memory().unwrap();
        let version: i64 = db
            .conn()
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
            .parse()
            .unwrap();
        assert!(version >= 1);
    }

    #[test]
    fn migration_is_idempotent() {
        let db = Db::open_in_memory().unwrap();
        schema::migrate(db.conn()).unwrap();
        schema::migrate(db.conn()).unwrap();
    }
}
