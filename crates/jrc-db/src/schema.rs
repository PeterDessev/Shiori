//! Schema definition and migrations.

use rusqlite::Connection;

use crate::Result;

/// Current schema version. Bump when adding migration steps.
const SCHEMA_VERSION: i64 = 1;

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS documents (
    id           INTEGER PRIMARY KEY,
    title        TEXT NOT NULL,
    added_at     TEXT NOT NULL,
    content_hash TEXT NOT NULL UNIQUE
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
"#;

/// Bring the schema up to [`SCHEMA_VERSION`]. Idempotent.
pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA_V1)?;
    let current: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .ok();
    let current: i64 = current.and_then(|v| v.parse().ok()).unwrap_or(0);
    if current < SCHEMA_VERSION {
        // Future versioned migration steps go here, gated on `current`.
        conn.execute(
            "INSERT INTO meta(key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = ?1",
            [SCHEMA_VERSION.to_string()],
        )?;
    }
    Ok(())
}
