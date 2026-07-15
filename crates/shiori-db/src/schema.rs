//! Schema definition and migrations.

use rusqlite::Connection;

use crate::Result;

/// Current schema version. Bump when adding migration steps.
const SCHEMA_VERSION: i64 = 8;

/// Base DDL, always at the *latest* shape (fresh databases need no ALTERs;
/// only databases stamped with an older version get the incremental steps).
///
/// Since v8 every user-facing table carries a language dimension and the
/// reference caches are keyed by language/source, so a second language can
/// neither collide with nor wipe another's data.
const BASE_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS documents (
    id            INTEGER PRIMARY KEY,
    lang          TEXT NOT NULL DEFAULT 'ja',
    title         TEXT NOT NULL,
    author        TEXT NOT NULL DEFAULT '',
    publisher     TEXT NOT NULL DEFAULT '',
    published     TEXT NOT NULL DEFAULT '',
    last_sentence INTEGER NOT NULL DEFAULT 0,
    added_at      TEXT NOT NULL,
    content_hash  TEXT NOT NULL,
    UNIQUE(lang, content_hash)
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
    id          INTEGER PRIMARY KEY,
    lang        TEXT NOT NULL DEFAULT 'ja',
    lemma       TEXT NOT NULL,
    reading     TEXT NOT NULL,
    pos         TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'unknown',
    dict_source TEXT,
    dict_key    TEXT,
    UNIQUE(lang, lemma, reading, pos)
);
CREATE INDEX IF NOT EXISTS idx_words_status ON words(status);

-- morph: the language pack's parse code for this occurrence (e.g. Greek
-- "V-AAI-3S"); gloss: a short per-occurrence gloss. Both NULL for tokens
-- produced by a runtime analyzer (Japanese). Tokens are immutable after
-- import; re-annotating a text means re-importing the document.
CREATE TABLE IF NOT EXISTS tokens (
    id          INTEGER PRIMARY KEY,
    sentence_id INTEGER NOT NULL REFERENCES sentences(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    word_id     INTEGER NOT NULL REFERENCES words(id),
    surface     TEXT NOT NULL,
    start       INTEGER NOT NULL,
    end         INTEGER NOT NULL,
    morph       TEXT,
    gloss       TEXT
);
CREATE INDEX IF NOT EXISTS idx_tokens_sentence ON tokens(sentence_id, idx);
CREATE INDEX IF NOT EXISTS idx_tokens_word ON tokens(word_id);

CREATE TABLE IF NOT EXISTS frequency (
    lang TEXT NOT NULL,
    word TEXT NOT NULL,
    rank INTEGER NOT NULL,
    PRIMARY KEY (lang, word)
);

-- Entries are keyed by (source, entry_key): 'jmdict' uses the numeric
-- JMdict sequence id as text; other lexicons may key by lemma+homograph.
CREATE TABLE IF NOT EXISTS dict_entries (
    source    TEXT NOT NULL,
    entry_key TEXT NOT NULL,
    json      TEXT NOT NULL,
    PRIMARY KEY (source, entry_key)
);

-- role: 'orthographic' (kanji spelling, written form), 'phonetic' (kana
-- reading), or 'canonical' (a lexicon's citation form).
CREATE TABLE IF NOT EXISTS dict_forms (
    source    TEXT NOT NULL,
    text      TEXT NOT NULL,
    entry_key TEXT NOT NULL,
    role      TEXT NOT NULL,
    is_common INTEGER NOT NULL,
    PRIMARY KEY (source, text, entry_key),
    FOREIGN KEY (source, entry_key)
        REFERENCES dict_entries(source, entry_key) ON DELETE CASCADE
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
    lang       TEXT NOT NULL DEFAULT 'ja',
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

-- v8: graded vocabulary lists (JLPT for Japanese, frequency tiers for
-- Koine Greek, …). level_ord ascends with difficulty (1 = easiest);
-- kana-only JLPT entries store form='' and match on alt_form.
CREATE TABLE IF NOT EXISTS graded_vocab (
    lang        TEXT NOT NULL,
    scheme      TEXT NOT NULL,
    level_ord   INTEGER NOT NULL,
    level_label TEXT NOT NULL,
    form        TEXT NOT NULL,
    alt_form    TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (lang, scheme, level_ord, form, alt_form)
);

-- v8: per-source tag decoding (POS codes, register codes, parse codes),
-- populated by language packs. Japanese labels stay compiled in for now.
CREATE TABLE IF NOT EXISTS dict_tags (
    source TEXT NOT NULL,
    code   TEXT NOT NULL,
    label  TEXT NOT NULL,
    PRIMARY KEY (source, code)
);

-- v8: full-form morphology lookup (Tier-1 analysis for languages without
-- a runtime analyzer). form_folded uses the pack's lookup normalization.
CREATE TABLE IF NOT EXISTS morph_forms (
    lang        TEXT NOT NULL,
    form_folded TEXT NOT NULL,
    lemma       TEXT NOT NULL,
    morph       TEXT NOT NULL,
    source      TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (lang, form_folded, lemma, morph)
);

-- v6: kanji reference data (KANJIDIC2 joined with KanjiVG strokes).
-- Japanese-only by design; other scripts get their own capability tables.
CREATE TABLE IF NOT EXISTS kanji (
    literal      TEXT PRIMARY KEY,
    grade        INTEGER,
    stroke_count INTEGER NOT NULL,
    jlpt         INTEGER,
    freq         INTEGER,
    on_readings  TEXT NOT NULL,
    kun_readings TEXT NOT NULL,
    nanori       TEXT NOT NULL,
    meanings     TEXT NOT NULL,
    variants     TEXT NOT NULL,
    strokes      TEXT
);
"#;

/// Bring the schema up to [`SCHEMA_VERSION`]. Idempotent.
///
/// The base DDL uses `IF NOT EXISTS` and already contains the latest
/// column set, so fresh databases need no ALTERs; only databases stamped
/// with an older version get the incremental steps.
pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(BASE_SCHEMA)?;
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
        if current < 8 {
            migrate_v8(conn)?;
        }
    }

    conn.execute(
        "INSERT INTO meta(key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        [SCHEMA_VERSION.to_string()],
    )?;
    Ok(())
}

/// v8: the language dimension. Existing rows backfill as 'ja'.
///
/// SQLite cannot alter table constraints, so tables whose keys change
/// (words, documents, dict_entries, dict_forms, frequency) are rebuilt via
/// CREATE + INSERT-SELECT + DROP + RENAME. Foreign keys are switched off
/// around the transaction so child tables (tokens, cards, review_log,
/// sentences, reading_sessions) keep their rows and simply follow the
/// rebuilt tables by name. Each step is guarded by a column-presence check
/// so a partially migrated database (or the base-DDL-created tables of the
/// v1 test fixture) is handled correctly.
fn migrate_v8(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let run = || -> Result<()> {
        let tx = conn.unchecked_transaction()?;

        if !has_column(&tx, "words", "lang")? {
            tx.execute_batch(
                "CREATE TABLE words_v8 (
                     id          INTEGER PRIMARY KEY,
                     lang        TEXT NOT NULL DEFAULT 'ja',
                     lemma       TEXT NOT NULL,
                     reading     TEXT NOT NULL,
                     pos         TEXT NOT NULL,
                     status      TEXT NOT NULL DEFAULT 'unknown',
                     dict_source TEXT,
                     dict_key    TEXT,
                     UNIQUE(lang, lemma, reading, pos)
                 );
                 INSERT INTO words_v8(id, lang, lemma, reading, pos, status,
                                      dict_source, dict_key)
                 SELECT id, 'ja', lemma, reading, pos, status,
                        CASE WHEN dict_seq IS NULL THEN NULL ELSE 'jmdict' END,
                        CASE WHEN dict_seq IS NULL THEN NULL
                             ELSE CAST(dict_seq AS TEXT) END
                 FROM words;
                 DROP TABLE words;
                 ALTER TABLE words_v8 RENAME TO words;
                 CREATE INDEX IF NOT EXISTS idx_words_status ON words(status);",
            )?;
        }

        if !has_column(&tx, "documents", "lang")? {
            tx.execute_batch(
                "CREATE TABLE documents_v8 (
                     id            INTEGER PRIMARY KEY,
                     lang          TEXT NOT NULL DEFAULT 'ja',
                     title         TEXT NOT NULL,
                     author        TEXT NOT NULL DEFAULT '',
                     publisher     TEXT NOT NULL DEFAULT '',
                     published     TEXT NOT NULL DEFAULT '',
                     last_sentence INTEGER NOT NULL DEFAULT 0,
                     added_at      TEXT NOT NULL,
                     content_hash  TEXT NOT NULL,
                     UNIQUE(lang, content_hash)
                 );
                 INSERT INTO documents_v8(id, lang, title, author, publisher,
                                          published, last_sentence, added_at,
                                          content_hash)
                 SELECT id, 'ja', title, author, publisher, published,
                        last_sentence, added_at, content_hash
                 FROM documents;
                 DROP TABLE documents;
                 ALTER TABLE documents_v8 RENAME TO documents;",
            )?;
        }

        if !has_column(&tx, "tokens", "morph")? {
            tx.execute_batch(
                "ALTER TABLE tokens ADD COLUMN morph TEXT;
                 ALTER TABLE tokens ADD COLUMN gloss TEXT;",
            )?;
        }

        if !has_column(&tx, "conversations", "lang")? {
            tx.execute_batch(
                "ALTER TABLE conversations ADD COLUMN lang TEXT NOT NULL DEFAULT 'ja';",
            )?;
        }

        // Real pre-v8 databases were seen carrying chat messages whose
        // conversation no longer exists (deleted while foreign keys were
        // off in some earlier version). They are unreachable from the UI;
        // sweep them so the migrated database passes an integrity check.
        tx.execute_batch(
            "DELETE FROM chat_annotations WHERE message_id IN
                 (SELECT id FROM chat_messages WHERE conversation_id NOT IN
                     (SELECT id FROM conversations));
             DELETE FROM chat_messages WHERE conversation_id NOT IN
                 (SELECT id FROM conversations);",
        )?;

        if !has_column(&tx, "dict_entries", "source")? {
            tx.execute_batch(
                "CREATE TABLE dict_entries_v8 (
                     source    TEXT NOT NULL,
                     entry_key TEXT NOT NULL,
                     json      TEXT NOT NULL,
                     PRIMARY KEY (source, entry_key)
                 );
                 INSERT INTO dict_entries_v8(source, entry_key, json)
                 SELECT 'jmdict', CAST(seq AS TEXT), json FROM dict_entries;
                 DROP TABLE dict_entries;
                 ALTER TABLE dict_entries_v8 RENAME TO dict_entries;",
            )?;
        }

        if !has_column(&tx, "dict_forms", "source")? {
            tx.execute_batch(
                "CREATE TABLE dict_forms_v8 (
                     source    TEXT NOT NULL,
                     text      TEXT NOT NULL,
                     entry_key TEXT NOT NULL,
                     role      TEXT NOT NULL,
                     is_common INTEGER NOT NULL,
                     PRIMARY KEY (source, text, entry_key),
                     FOREIGN KEY (source, entry_key)
                         REFERENCES dict_entries(source, entry_key)
                         ON DELETE CASCADE
                 );
                 INSERT INTO dict_forms_v8(source, text, entry_key, role, is_common)
                 SELECT 'jmdict', text, CAST(seq AS TEXT),
                        CASE WHEN is_kana THEN 'phonetic' ELSE 'orthographic' END,
                        is_common
                 FROM dict_forms;
                 DROP TABLE dict_forms;
                 ALTER TABLE dict_forms_v8 RENAME TO dict_forms;",
            )?;
        }

        if !has_column(&tx, "frequency", "lang")? {
            tx.execute_batch(
                "CREATE TABLE frequency_v8 (
                     lang TEXT NOT NULL,
                     word TEXT NOT NULL,
                     rank INTEGER NOT NULL,
                     PRIMARY KEY (lang, word)
                 );
                 INSERT INTO frequency_v8(lang, word, rank)
                 SELECT 'ja', word, rank FROM frequency;
                 DROP TABLE frequency;
                 ALTER TABLE frequency_v8 RENAME TO frequency;",
            )?;
        }

        if table_exists(&tx, "jlpt_words")? {
            // JLPT levels count down toward difficulty (N5 easiest); the
            // generic ordinal counts up (1 easiest).
            tx.execute_batch(
                "INSERT OR IGNORE INTO graded_vocab(lang, scheme, level_ord,
                                                    level_label, form, alt_form)
                 SELECT 'ja', 'jlpt', 6 - level, 'N' || level, word, kana
                 FROM jlpt_words;
                 DROP TABLE jlpt_words;",
            )?;
        }

        tx.commit()?;
        Ok(())
    };
    let result = run();
    conn.pragma_update(None, "foreign_keys", "ON")?;
    result
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
        [table, column],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |r| r.get(0),
    )?;
    Ok(n > 0)
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
            .query_row(
                "SELECT author FROM documents WHERE title = 'old doc'",
                [],
                |r| r.get(0),
            )
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
        let lang: String = conn
            .query_row(
                "SELECT lang FROM documents WHERE title = 'old doc'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(lang, "ja");
        let version: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION.to_string());

        // And running migrate again is a no-op.
        migrate(&conn).unwrap();
    }

    /// The exact v7 DDL, for migration tests: a database as a real
    /// pre-multilingual install would have it.
    pub(crate) const V7_SCHEMA: &str = r#"
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE documents (
    id            INTEGER PRIMARY KEY,
    title         TEXT NOT NULL,
    author        TEXT NOT NULL DEFAULT '',
    publisher     TEXT NOT NULL DEFAULT '',
    published     TEXT NOT NULL DEFAULT '',
    last_sentence INTEGER NOT NULL DEFAULT 0,
    added_at      TEXT NOT NULL,
    content_hash  TEXT NOT NULL UNIQUE
);
CREATE TABLE sentences (
    id          INTEGER PRIMARY KEY,
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    paragraph   INTEGER NOT NULL,
    text        TEXT NOT NULL
);
CREATE TABLE words (
    id       INTEGER PRIMARY KEY,
    lemma    TEXT NOT NULL,
    reading  TEXT NOT NULL,
    pos      TEXT NOT NULL,
    status   TEXT NOT NULL DEFAULT 'unknown',
    dict_seq INTEGER,
    UNIQUE(lemma, reading, pos)
);
CREATE TABLE tokens (
    id          INTEGER PRIMARY KEY,
    sentence_id INTEGER NOT NULL REFERENCES sentences(id) ON DELETE CASCADE,
    idx         INTEGER NOT NULL,
    word_id     INTEGER NOT NULL REFERENCES words(id),
    surface     TEXT NOT NULL,
    start       INTEGER NOT NULL,
    end         INTEGER NOT NULL
);
CREATE TABLE frequency (word TEXT PRIMARY KEY, rank INTEGER NOT NULL);
CREATE TABLE dict_entries (seq INTEGER PRIMARY KEY, json TEXT NOT NULL);
CREATE TABLE dict_forms (
    text      TEXT NOT NULL,
    seq       INTEGER NOT NULL REFERENCES dict_entries(seq) ON DELETE CASCADE,
    is_kana   INTEGER NOT NULL,
    is_common INTEGER NOT NULL,
    PRIMARY KEY (text, seq)
);
CREATE TABLE cards (
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
CREATE TABLE review_log (
    id          INTEGER PRIMARY KEY,
    word_id     INTEGER NOT NULL REFERENCES words(id) ON DELETE CASCADE,
    rating      INTEGER NOT NULL,
    reviewed_at TEXT NOT NULL,
    stability   REAL NOT NULL,
    difficulty  REAL NOT NULL
);
CREATE TABLE reading_sessions (
    id          INTEGER PRIMARY KEY,
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    started_at  TEXT NOT NULL,
    ended_at    TEXT NOT NULL,
    seconds     REAL NOT NULL DEFAULT 0,
    chars       INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE conversations (
    id         INTEGER PRIMARY KEY,
    started_at TEXT NOT NULL,
    title      TEXT NOT NULL DEFAULT ''
);
CREATE TABLE chat_messages (
    id              INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    idx             INTEGER NOT NULL,
    role            TEXT NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL
);
CREATE TABLE chat_annotations (
    id         INTEGER PRIMARY KEY,
    message_id INTEGER NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
    start      INTEGER NOT NULL,
    end        INTEGER NOT NULL,
    severity   TEXT NOT NULL,
    note       TEXT NOT NULL
);
CREATE TABLE jlpt_words (
    level INTEGER NOT NULL,
    word  TEXT NOT NULL,
    kana  TEXT NOT NULL,
    PRIMARY KEY (level, word, kana)
);
CREATE TABLE kanji (
    literal      TEXT PRIMARY KEY,
    grade        INTEGER,
    stroke_count INTEGER NOT NULL,
    jlpt         INTEGER,
    freq         INTEGER,
    on_readings  TEXT NOT NULL,
    kun_readings TEXT NOT NULL,
    nanori       TEXT NOT NULL,
    meanings     TEXT NOT NULL,
    variants     TEXT NOT NULL,
    strokes      TEXT
);
INSERT INTO meta(key, value) VALUES ('schema_version', '7');
"#;

    /// Build a populated v7 database: a document with tokens, a tracked
    /// word with an SRS card and review history, dictionary/frequency
    /// caches, and JLPT lists.
    fn populated_v7() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(V7_SCHEMA).unwrap();
        conn.execute_batch(
            "INSERT INTO documents(id, title, added_at, content_hash)
                 VALUES (1, '猫の本', '2026-01-01T00:00:00Z', 'hash-1');
             INSERT INTO sentences(id, document_id, idx, paragraph, text)
                 VALUES (10, 1, 0, 0, '猫が好きだ。');
             INSERT INTO words(id, lemma, reading, pos, status, dict_seq)
                 VALUES (100, '猫', 'ねこ', 'noun', 'known', 1467640),
                        (101, 'が', 'が', 'particle', 'unknown', NULL);
             INSERT INTO tokens(sentence_id, idx, word_id, surface, start, end)
                 VALUES (10, 0, 100, '猫', 0, 3), (10, 1, 101, 'が', 3, 6);
             INSERT INTO cards(word_id, sentence_id, state, stability,
                               difficulty, due, reps, lapses, step)
                 VALUES (100, 10, 'review', 42.0, 5.0,
                         '2026-02-01T00:00:00Z', 7, 1, 0);
             INSERT INTO review_log(word_id, rating, reviewed_at, stability,
                                    difficulty)
                 VALUES (100, 3, '2026-01-15T00:00:00Z', 42.0, 5.0);
             INSERT INTO dict_entries(seq, json)
                 VALUES (1467640, '{\"id\":\"1467640\"}');
             INSERT INTO dict_forms(text, seq, is_kana, is_common)
                 VALUES ('猫', 1467640, 0, 1), ('ねこ', 1467640, 1, 1);
             INSERT INTO frequency(word, rank) VALUES ('猫', 500);
             INSERT INTO jlpt_words(level, word, kana)
                 VALUES (5, '猫', 'ねこ'), (5, '', 'する');
             INSERT INTO conversations(id, started_at, title)
                 VALUES (1, '2026-01-01T00:00:00Z', '天気');",
        )
        .unwrap();
        conn
    }

    #[test]
    fn v7_to_v8_preserves_user_state() {
        let conn = populated_v7();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrate(&conn).unwrap();

        // Words carry lang='ja' and the dict reference moved to
        // (source, key) form.
        let (lang, source, key): (String, String, String) = conn
            .query_row(
                "SELECT lang, dict_source, dict_key FROM words WHERE id = 100",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (lang.as_str(), source.as_str(), key.as_str()),
            ("ja", "jmdict", "1467640")
        );

        // The SRS card and review history still hang off the same word id.
        let (stability, reps): (f64, i64) = conn
            .query_row(
                "SELECT stability, reps FROM cards WHERE word_id = 100",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((stability, reps), (42.0, 7));
        let reviews: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM review_log WHERE word_id = 100",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(reviews, 1);

        // Tokens survive and gained the (empty) annotation columns.
        let (surfaces, morphs): (i64, i64) = conn
            .query_row("SELECT COUNT(*), COUNT(morph) FROM tokens", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((surfaces, morphs), (2, 0));

        // Reference caches migrated in place — no re-download needed.
        let dict: (String, String) = conn
            .query_row("SELECT source, entry_key FROM dict_entries", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((dict.0.as_str(), dict.1.as_str()), ("jmdict", "1467640"));
        let phonetic: String = conn
            .query_row(
                "SELECT role FROM dict_forms WHERE text = 'ねこ'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(phonetic, "phonetic");
        let rank: i64 = conn
            .query_row(
                "SELECT rank FROM frequency WHERE lang = 'ja' AND word = '猫'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rank, 500);

        // JLPT rows became graded_vocab under the generic ordinal
        // (N5 = easiest = ord 1) and the old table is gone.
        let (ord, label): (i64, String) = conn
            .query_row(
                "SELECT level_ord, level_label FROM graded_vocab
                 WHERE lang = 'ja' AND scheme = 'jlpt' AND form = '猫'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((ord, label.as_str()), (1, "N5"));
        assert!(!table_exists(&conn, "jlpt_words").unwrap());

        // Conversations carry a language.
        let conv_lang: String = conn
            .query_row("SELECT lang FROM conversations WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(conv_lang, "ja");

        // Idempotent.
        migrate(&conn).unwrap();
    }

    #[test]
    fn v8_allows_same_key_in_two_languages() {
        let conn = populated_v7();
        migrate(&conn).unwrap();
        // A Latin-script pair that would have collided under the v7
        // UNIQUE(lemma, reading, pos).
        conn.execute_batch(
            "INSERT INTO words(lang, lemma, reading, pos) VALUES ('es', 'sol', '', 'noun');
             INSERT INTO words(lang, lemma, reading, pos) VALUES ('la', 'sol', '', 'noun');",
        )
        .unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM words WHERE lemma = 'sol'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 2);
        // But within one language the identity is still unique.
        assert!(conn
            .execute(
                "INSERT INTO words(lang, lemma, reading, pos) VALUES ('es', 'sol', '', 'noun')",
                [],
            )
            .is_err());
    }

    #[test]
    fn v7_foreign_keys_still_enforced_after_rebuild() {
        let conn = populated_v7();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrate(&conn).unwrap();
        // Deleting the document must still cascade into sentences/tokens.
        conn.execute("DELETE FROM documents WHERE id = 1", [])
            .unwrap();
        let tokens: i64 = conn
            .query_row("SELECT COUNT(*) FROM tokens", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tokens, 0);
        // And the FK indirection survived the rename: no dangling refs.
        let violations: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check()", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(violations, 0);
    }
}
