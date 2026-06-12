//! Anki .apkg writing and reading (legacy schema 11 packages).
//!
//! An .apkg is a zip holding a SQLite database (`collection.anki2`, or
//! `collection.anki21` when present) plus a `media` JSON map. The newer
//! zstd format (`collection.anki21b` + `meta`) is rejected with a clear
//! message — its bundled `collection.anki2` is a decoy stub.

use std::io::{Read, Write};
use std::path::Path;

use rusqlite::{params, Connection};

use crate::{DbError, Result};

/// Fixed identifiers so re-exports update rather than duplicate.
const MODEL_ID: i64 = 1718281828459;
const DECK_ID: i64 = 1718281828460;
/// Collection creation epoch (seconds); review due days count from here.
const CRT: i64 = 1_600_000_000;

/// One note to export, with optional scheduling.
#[derive(Debug, Clone)]
pub struct AnkiNote {
    /// Stable dedup id — derive from the word identity.
    pub guid: String,
    /// Expression, reading, meaning, context sentence.
    pub fields: [String; 4],
    /// `None` exports the card as new.
    pub schedule: Option<AnkiSchedule>,
}

/// SM-2-shaped scheduling for one exported card.
#[derive(Debug, Clone, Copy)]
pub struct AnkiSchedule {
    /// Days from today until due (clamped at 0 = due now).
    pub due_in_days: i64,
    /// Current interval in days.
    pub interval_days: u32,
    /// Ease in permille (2500 = 2.5).
    pub factor: u32,
    pub reps: u32,
    pub lapses: u32,
}

/// One note read from an imported package.
#[derive(Debug, Clone)]
pub struct ImportedNote {
    pub fields: Vec<String>,
    /// Days until due relative to import time (negative = overdue).
    pub due_in_days: Option<i64>,
    pub interval_days: u32,
    /// Ease in permille; 0 when the card never graduated.
    pub factor: u32,
    pub reps: u32,
    pub lapses: u32,
    /// True for cards with review history (type/queue ≥ 2).
    pub reviewed: bool,
}

const APKG_DDL: &str = r#"
CREATE TABLE col (
    id integer primary key, crt integer not null, mod integer not null,
    scm integer not null, ver integer not null, dty integer not null,
    usn integer not null, ls integer not null,
    conf text not null, models text not null, decks text not null,
    dconf text not null, tags text not null
);
CREATE TABLE notes (
    id integer primary key, guid text not null, mid integer not null,
    mod integer not null, usn integer not null, tags text not null,
    flds text not null, sfld integer not null, csum integer not null,
    flags integer not null, data text not null
);
CREATE TABLE cards (
    id integer primary key, nid integer not null, did integer not null,
    ord integer not null, mod integer not null, usn integer not null,
    type integer not null, queue integer not null, due integer not null,
    ivl integer not null, factor integer not null, reps integer not null,
    lapses integer not null, left integer not null, odue integer not null,
    odid integer not null, flags integer not null, data text not null
);
CREATE TABLE revlog (
    id integer primary key, cid integer not null, usn integer not null,
    ease integer not null, ivl integer not null, lastIvl integer not null,
    factor integer not null, time integer not null, type integer not null
);
CREATE TABLE graves (usn integer not null, oid integer not null, type integer not null);
CREATE INDEX ix_notes_usn on notes (usn);
CREATE INDEX ix_cards_usn on cards (usn);
CREATE INDEX ix_revlog_usn on revlog (usn);
CREATE INDEX ix_cards_nid on cards (nid);
CREATE INDEX ix_cards_sched on cards (did, queue, due);
CREATEINDEXMARKER
CREATE INDEX ix_notes_csum on notes (csum);
"#;

fn col_json(deck_name: &str, now_s: i64) -> (String, String, String, String) {
    let conf = serde_json::json!({
        "activeDecks": [1], "addToCur": true, "collapseTime": 1200,
        "curDeck": 1, "curModel": MODEL_ID.to_string(), "dueCounts": true,
        "estTimes": true, "newBury": true, "newSpread": 0, "nextPos": 1,
        "sortBackwards": false, "sortType": "noteFld", "timeLim": 0
    });
    let fields: Vec<serde_json::Value> = ["Expression", "Reading", "Meaning", "Sentence"]
        .iter()
        .enumerate()
        .map(|(ord, name)| {
            serde_json::json!({
                "name": name, "media": [], "sticky": false, "rtl": false,
                "ord": ord, "font": "Liberation Sans", "size": 20
            })
        })
        .collect();
    let models = serde_json::json!({
        MODEL_ID.to_string(): {
            "id": MODEL_ID.to_string(),
            "name": "Shiori",
            "type": 0, "mod": now_s, "usn": 0, "sortf": 0, "did": DECK_ID,
            "css": ".card { font-family: sans-serif; font-size: 26px; text-align: center; }",
            "flds": fields,
            "tmpls": [{
                "name": "Recognition",
                "qfmt": "{{Expression}}<br><span style=\"font-size:16px\">{{Sentence}}</span>",
                "afmt": "{{FrontSide}}<hr id=answer>{{Reading}}<br>{{Meaning}}",
                "bqfmt": "", "bafmt": "", "ord": 0, "did": null
            }],
            "req": [[0, "all", [0]]],
            "tags": [], "vers": []
        }
    });
    let decks = serde_json::json!({
        "1": {
            "id": 1, "name": "Default", "desc": "", "collapsed": false,
            "conf": 1, "dyn": 0, "extendNew": 10, "extendRev": 50,
            "lrnToday": [0, 0], "newToday": [0, 0], "revToday": [0, 0],
            "timeToday": [0, 0], "mod": now_s, "usn": 0
        },
        DECK_ID.to_string(): {
            "id": DECK_ID, "name": deck_name, "desc": "", "collapsed": false,
            "conf": 1, "dyn": 0, "extendNew": 10, "extendRev": 50,
            "lrnToday": [0, 0], "newToday": [0, 0], "revToday": [0, 0],
            "timeToday": [0, 0], "mod": now_s, "usn": 0
        }
    });
    let dconf = serde_json::json!({
        "1": {
            "id": 1, "name": "Default", "autoplay": true, "maxTaken": 60,
            "mod": 0, "usn": 0, "timer": 0, "replayq": true,
            "new": {"bury": true, "delays": [1, 10], "initialFactor": 2500,
                     "ints": [1, 4, 7], "order": 1, "perDay": 20, "separate": true},
            "rev": {"bury": true, "ease4": 1.3, "fuzz": 0.05, "ivlFct": 1,
                     "maxIvl": 36500, "minSpace": 1, "perDay": 100},
            "lapse": {"delays": [10], "leechAction": 0, "leechFails": 8,
                      "minInt": 1, "mult": 0}
        }
    });
    (
        conf.to_string(),
        models.to_string(),
        decks.to_string(),
        dconf.to_string(),
    )
}

/// Write a legacy .apkg containing one deck of notes.
pub fn write_apkg(path: &Path, deck_name: &str, notes: &[AnkiNote]) -> Result<()> {
    let dir = std::env::temp_dir().join(format!("jrc-apkg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(io_err)?;
    let db_path = dir.join("collection.anki2");
    std::fs::remove_file(&db_path).ok();

    let now_ms = chrono::Utc::now().timestamp_millis();
    let now_s = now_ms / 1000;
    {
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(&APKG_DDL.replace("CREATEINDEXMARKER", ""))?;
        let (conf, models, decks, dconf) = col_json(deck_name, now_s);
        conn.execute(
            "INSERT INTO col VALUES (1, ?1, ?2, ?2, 11, 0, 0, 0, ?3, ?4, ?5, ?6, '{}')",
            params![CRT, now_ms, conf, models, decks, dconf],
        )?;

        let tx = conn.unchecked_transaction()?;
        {
            let mut insert_note = tx.prepare(
                "INSERT INTO notes VALUES (?1, ?2, ?3, ?4, -1, '  ', ?5, ?6, 0, 0, '')",
            )?;
            let mut insert_card = tx.prepare(
                "INSERT INTO cards VALUES (?1, ?2, ?3, 0, ?4, -1, ?5, ?6, ?7, ?8, ?9,
                 ?10, ?11, 0, 0, 0, 0, '')",
            )?;
            let today_from_crt = (now_s - CRT) / 86_400;
            for (i, note) in notes.iter().enumerate() {
                let note_id = now_ms + i as i64;
                let flds = note.fields.join("\u{1f}");
                insert_note.execute(params![
                    note_id,
                    note.guid,
                    MODEL_ID,
                    now_s,
                    flds,
                    note.fields[0]
                ])?;
                let card_id = now_ms + notes.len() as i64 + i as i64;
                match &note.schedule {
                    Some(s) => {
                        let due = today_from_crt + s.due_in_days.max(0);
                        insert_card.execute(params![
                            card_id, note_id, DECK_ID, now_s,
                            2i64, 2i64, due,
                            s.interval_days, s.factor.max(1300), s.reps, s.lapses
                        ])?;
                    }
                    None => {
                        insert_card.execute(params![
                            card_id, note_id, DECK_ID, now_s,
                            0i64, 0i64, i as i64, 0i64, 0i64, 0i64, 0i64
                        ])?;
                    }
                }
            }
        }
        tx.commit()?;
    }

    let db_bytes = std::fs::read(&db_path).map_err(io_err)?;
    std::fs::remove_dir_all(&dir).ok();

    let file = std::fs::File::create(path).map_err(io_err)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("collection.anki2", options).map_err(zip_err)?;
    zip.write_all(&db_bytes).map_err(io_err)?;
    zip.start_file("media", options).map_err(zip_err)?;
    zip.write_all(b"{}").map_err(io_err)?;
    zip.finish().map_err(zip_err)?;
    Ok(())
}

/// Read the notes (with scheduling) out of a legacy .apkg.
pub fn read_apkg(path: &Path) -> Result<Vec<ImportedNote>> {
    let file = std::fs::File::open(path).map_err(io_err)?;
    let mut archive = zip::ZipArchive::new(file).map_err(zip_err)?;

    let names: Vec<String> = archive.file_names().map(String::from).collect();
    if names.iter().any(|n| n == "meta" || n == "collection.anki21b") {
        return Err(DbError::NotFound(
            "this .apkg uses Anki's newer format; re-export it with \
             \"Support older Anki versions\" checked",
        ));
    }
    // Anki's own detection order: anki21 holds the real data when present.
    let member = if names.iter().any(|n| n == "collection.anki21") {
        "collection.anki21"
    } else if names.iter().any(|n| n == "collection.anki2") {
        "collection.anki2"
    } else {
        return Err(DbError::NotFound("no collection database in .apkg"));
    };

    let mut bytes = Vec::new();
    archive
        .by_name(member)
        .map_err(zip_err)?
        .read_to_end(&mut bytes)
        .map_err(io_err)?;
    let tmp = std::env::temp_dir().join(format!("jrc-apkg-import-{}.db", std::process::id()));
    std::fs::write(&tmp, &bytes).map_err(io_err)?;

    let result = read_collection(&tmp);
    std::fs::remove_file(&tmp).ok();
    result
}

fn read_collection(db_path: &Path) -> Result<Vec<ImportedNote>> {
    let conn = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let crt: i64 = conn.query_row("SELECT crt FROM col", [], |r| r.get(0))?;
    let today_from_crt = (chrono::Utc::now().timestamp() - crt) / 86_400;

    let mut stmt = conn.prepare(
        "SELECT n.flds, c.queue, c.due, c.ivl, c.factor, c.reps, c.lapses, c.type
         FROM notes n
         LEFT JOIN cards c ON c.nid = n.id
         GROUP BY n.id",
    )?;
    let rows = stmt.query_map([], |r| {
        let flds: String = r.get(0)?;
        let queue: Option<i64> = r.get(1)?;
        let due: Option<i64> = r.get(2)?;
        let ivl: Option<i64> = r.get(3)?;
        let factor: Option<i64> = r.get(4)?;
        let reps: Option<i64> = r.get(5)?;
        let lapses: Option<i64> = r.get(6)?;
        let ctype: Option<i64> = r.get(7)?;
        let queue = queue.unwrap_or(0);
        let reviewed = ctype.unwrap_or(0) >= 2 || queue == 2 || queue == 3;
        // Review-queue due values are days since the collection epoch.
        let due_in_days = (reviewed && (queue == 2 || queue == 3 || queue < 0))
            .then(|| due.unwrap_or(0) - today_from_crt);
        Ok(ImportedNote {
            fields: flds.split('\u{1f}').map(String::from).collect(),
            due_in_days,
            interval_days: ivl.unwrap_or(0).max(0) as u32,
            factor: factor.unwrap_or(0).max(0) as u32,
            reps: reps.unwrap_or(0).max(0) as u32,
            lapses: lapses.unwrap_or(0).max(0) as u32,
            reviewed,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

fn io_err(e: std::io::Error) -> DbError {
    DbError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn zip_err(e: zip::result::ZipError) -> DbError {
    DbError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apkg_roundtrip() {
        let path = std::env::temp_dir().join("jrc-test-roundtrip.apkg");
        let notes = vec![
            AnkiNote {
                guid: "jrc-猫-ねこ".into(),
                fields: [
                    "猫".into(),
                    "ねこ".into(),
                    "cat".into(),
                    "猫が好きだ。".into(),
                ],
                schedule: Some(AnkiSchedule {
                    due_in_days: 3,
                    interval_days: 21,
                    factor: 2500,
                    reps: 7,
                    lapses: 1,
                }),
            },
            AnkiNote {
                guid: "jrc-走る-はしる".into(),
                fields: ["走る".into(), "はしる".into(), "to run".into(), "".into()],
                schedule: None,
            },
        ];
        write_apkg(&path, "JRC export", &notes).unwrap();

        let imported = read_apkg(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(imported.len(), 2);

        let cat = imported.iter().find(|n| n.fields[0] == "猫").unwrap();
        assert_eq!(cat.fields[1], "ねこ");
        assert_eq!(cat.interval_days, 21);
        assert_eq!(cat.factor, 2500);
        assert_eq!(cat.reps, 7);
        assert!(cat.reviewed);
        assert_eq!(cat.due_in_days, Some(3));

        let run = imported.iter().find(|n| n.fields[0] == "走る").unwrap();
        assert!(!run.reviewed);
        assert_eq!(run.interval_days, 0);
    }

    #[test]
    fn new_format_is_rejected_clearly() {
        let path = std::env::temp_dir().join("jrc-test-newfmt.apkg");
        {
            let file = std::fs::File::create(&path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("meta", options).unwrap();
            zip.write_all(b"x").unwrap();
            zip.start_file("collection.anki21b", options).unwrap();
            zip.write_all(b"zstd-stuff").unwrap();
            // The decoy stub that must NOT be fallen through to.
            zip.start_file("collection.anki2", options).unwrap();
            zip.write_all(b"stub").unwrap();
            zip.finish().unwrap();
        }
        let err = read_apkg(&path).unwrap_err();
        std::fs::remove_file(&path).ok();
        assert!(err.to_string().contains("newer format"));
    }
}
