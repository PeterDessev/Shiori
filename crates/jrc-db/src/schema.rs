//! Schema definition and migrations.

use rusqlite::Connection;

use crate::Result;

/// Current schema version. Bump when adding migration steps.
const SCHEMA_VERSION: i64 = 5;

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS documents (
    id            INTEGER PRIMARY KEY,
    title         TEXT NOT NULL,
    author        TEXT NOT NULL DEFAULT '',
    publisher     TEXT NOT NULL DEFAULT '',
    published     TEXT NOT NULL DEFAULT '',
    last_sentence INTEGER NOT NULL DEFAULT 0,
    added_at      TEXT NOT NULL,
    content_hash  TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS sentences (
    id          INTEGER PRIMARY KEY,
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    paragraph   INTEGER NOT NULL,
    text        TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sentences_doc ON sentences(document_id, idx);

CREATE TABLE IF NOT EXISTS words (
    id       INTEGER PRIMARY KEY,
    lemma    TEXT NOT NULL,
    reading  TEXT NOT NULL,
    pos      TEXT NOT NULL,
    status   TEXT NOT NULL DEFAULT 'unknown',
    dict_seq INTEGER,
    UNIQUE(lemma, reading, pos)
);
CREATE INDEX IF NOT EXISTS idx_words_status ON words(status);

CREATE TABLE IF NOT EXISTS tokens (
    id          INTEGER PRIMARY KEY,
    sentence_id INTEGER NOT NULL REFERENCES sentences(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    word_id     INTEGER NOT NULL REFERENCES words(id),
    surface     TEXT NOT NULL,
    start       INTEGER NOT NULL,
    end         INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_tokens_sentence ON tokens(sentence_id, idx);
CREATE INDEX IF NOT EXISTS idx_tokens_word ON tokens(word_id);

CREATE TABLE IF NOT EXISTS frequency (
    word TEXT PRIMARY KEY,
    rank INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS dict_entries (
    seq  INTEGER PRIMARY KEY,
    json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS dict_forms (
    text      TEXT NOT NULL,
    seq       INTEGER NOT NULL REFERENCES dict_entries(seq) ON DELETE CASCADE,
    is_kana   INTEGER NOT NULL,
    is_common INTEGER NOT NULL,
    PRIMARY KEY (text, seq)
);

CREATE TABLE IF NOT EXISTS cards (
    word_id     INTEGER PRIMARY KEY REFERENCES words(id) ON DELETE CASCADE,
    sentence_id INTEGER REFERENCES sentences(id) ON DELETE SET NULL,
    state       TEXT NOT NULL,
    stability   REAL NOT NULL,
    difficulty  REAL NOT NULL,
    due         TEXT NOT NULL,
    last_review TEXT,
    reps        INTEGER NOT NULL,
    lapses      INTEGER NOT NULL,
    step        INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_cards_due ON cards(due);

CREATE TABLE IF NOT EXISTS review_log (
    id          INTEGER PRIMARY KEY,
    word_id     INTEGER NOT NULL REFERENCES words(id) ON DELETE CASCADE,
    rating      INTEGER NOT NULL,
    reviewed_at TEXT NOT NULL,
    stability   REAL NOT NULL,
    difficulty  REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_review_log_word ON review_log(word_id);

-- v4: active reading time, one row per continuous sitting.
CREATE TABLE IF NOT EXISTS reading_sessions (
    id          INTEGER PRIMARY KEY,
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    started_at  TEXT NOT NULL,
    ended_at    TEXT NOT NULL,
    seconds     REAL NOT NULL DEFAULT 0,
    chars       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_sessions_doc ON reading_sessions(document_id);

-- v5: production-practice chat with paper-style write-ups.
CREATE TABLE IF NOT EXISTS conversations (
    id         INTEGER PRIMARY KEY,
    started_at TEXT NOT NULL,
    title      TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS chat_messages (
    id              INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    idx             INTEGER NOT NULL,
    role            TEXT NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_chat_messages_conv
    ON chat_messages(conversation_id, idx);

-- Write-up spans over a *user* message (byte offsets into content).
CREATE TABLE IF NOT EXISTS chat_annotations (
    id         INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
    start      INTEGER NOT NULL,
    end        INTEGER NOT NULL,
    severity   TEXT NOT NULL,
    note       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_chat_annotations_msg
    ON chat_annotations(message_id);
"#;

/// Bring the schema up to [`SCHEMA_VERSION`]. Idempotent.
///
/// The base DDL uses `IF NOT EXISTS` and already contains the latest
/// column set, so fresh databases need no ALTERs; only databases stamped
/// with an older version get the incremental steps.
pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_V1)?;
    let stored: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .ok();

    // No version row means the batch above just created everything at the
    // current shape; just stamp it.
    if let Some(stored) = stored {
        let current: i64 = stored.parse().unwrap_or(0);
        if current < 2 {
            // v2: document metadata columns.
            conn.execute_batch(
                "ALTER TABLE documents ADD COLUMN author    TEXT NOT NULL DEFAULT '';
                 ALTER TABLE documents ADD COLUMN publisher TEXT NOT NULL DEFAULT '';
                 ALTER TABLE documents ADD COLUMN published TEXT NOT NULL DEFAULT '';",
            )?;
        }
        if current < 3 {
            // v3: reading position.
            conn.execute_batch(
                "ALTER TABLE documents ADD COLUMN last_sentence INTEGER NOT NULL DEFAULT 0;",
            )?;
        }
    }

    conn.execute(
        "INSERT INTO meta(key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        [SCHEMA_VERSION.to_string()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A database created by schema v1 (no metadata columns) must gain
    /// them on open.
    #[test]
    fn migrates_v1_documents_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE documents (
                 id           INTEGER PRIMARY KEY,
                 title        TEXT NOT NULL,
                 added_at     TEXT NOT NULL,
                 content_hash TEXT NOT NULL UNIQUE
             );
             INSERT INTO meta(key, value) VALUES ('schema_version', '1');
             INSERT INTO documents(title, added_at, content_hash)
                 VALUES ('old doc', '2026-01-01T00:00:00Z', 'h1');",
        )
        .unwrap();

        migrate(&conn).unwrap();

        let author: String = conn
            .query_row("SELECT author FROM documents WHERE title = 'old doc'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(author, "");
        let last_sentence: i64 = conn
            .query_row(
                "SELECT last_sentence FROM documents WHERE title = 'old doc'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(last_sentence, 0);
        let version: String = conn
            .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION.to_string());

        // And running migrate again is a no-op.
        migrate(&conn).unwrap();
    }
}
