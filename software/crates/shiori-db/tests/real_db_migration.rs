//! Migration dry-run against a *real* pre-v8 database.
//!
//! Ignored by default; run deliberately with the environment variable
//! pointing at a live database (it is opened read-only and copied via
//! `VACUUM INTO` — the original is never written):
//!
//! ```sh
//! SHIORI_MIGRATION_SOURCE_DB="$APPDATA/shiori/jrc.sqlite3" \
//!     cargo test -p shiori-db --test real_db_migration -- --ignored --nocapture
//! ```

use rusqlite::{Connection, OpenFlags};

#[derive(Debug, PartialEq)]
struct Counts {
    documents: i64,
    sentences: i64,
    tokens: i64,
    words: i64,
    cards: i64,
    review_log: i64,
    dict_entries: i64,
    frequency: i64,
}

fn counts(conn: &Connection) -> Counts {
    let count = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap_or(0)
    };
    Counts {
        documents: count("documents"),
        sentences: count("sentences"),
        tokens: count("tokens"),
        words: count("words"),
        cards: count("cards"),
        review_log: count("review_log"),
        dict_entries: count("dict_entries"),
        frequency: count("frequency"),
    }
}

#[test]
#[ignore = "set SHIORI_MIGRATION_SOURCE_DB to a real database to run"]
fn migrates_a_real_database_copy() {
    let source = std::env::var("SHIORI_MIGRATION_SOURCE_DB")
        .expect("SHIORI_MIGRATION_SOURCE_DB must point at a jrc.sqlite3");

    // Safe snapshot of the (possibly live, WAL-mode) original.
    let copy = std::env::temp_dir().join("shiori-migration-dryrun.sqlite3");
    std::fs::remove_file(&copy).ok();
    {
        let ro = Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open source read-only");
        ro.execute("VACUUM INTO ?1", [copy.to_string_lossy().as_ref()])
            .expect("snapshot the database");
    }

    let before = {
        let conn = Connection::open(&copy).unwrap();
        counts(&conn)
    };
    println!("before: {before:?}");

    // The real thing: open through the crate, which backs up + migrates.
    let db = shiori_db::Db::open(&copy).expect("migration succeeds");
    drop(db);

    // Inspect the migrated file with a plain connection.
    let conn = Connection::open(&copy).unwrap();
    let after = counts(&conn);
    println!("after:  {after:?}");
    assert_eq!(before, after, "no row of user state may be lost");
    let version: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(version, "8");

    // Everything backfilled as Japanese.
    for (table, expected) in [
        ("documents", before.documents),
        ("words", before.words),
        ("conversations", -1),
    ] {
        let ja: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE lang = 'ja'"),
                [],
                |r| r.get(0),
            )
            .unwrap();
        if expected >= 0 {
            assert_eq!(ja, expected, "{table} rows must all be lang='ja'");
        }
    }

    // Referential integrity survived the table rebuilds.
    let violations: i64 = conn
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check()", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);

    // The pre-migration safety copy was written next to the database.
    let mut backup = copy.as_os_str().to_owned();
    backup.push(".v7-backup");
    assert!(std::path::Path::new(&backup).exists());

    // JLPT lists moved to graded_vocab; cache identities preserved.
    let graded: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM graded_vocab WHERE lang='ja' AND scheme='jlpt'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    println!("graded_vocab(ja/jlpt): {graded}");
    let jmdict: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM dict_entries WHERE source='jmdict'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(jmdict, before.dict_entries);
}
