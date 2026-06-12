//! Kanji reference storage: readings, meanings, grades, and stroke-order
//! path data, keyed by the character itself. List fields are stored as
//! JSON arrays (this crate stays policy-free about their contents).

use rusqlite::params;

use crate::{Db, Result};

/// One kanji row.
#[derive(Debug, Clone)]
pub struct KanjiRow {
    pub literal: String,
    pub grade: Option<u8>,
    pub stroke_count: u8,
    /// Old (pre-2010) JLPT level 1–4.
    pub jlpt: Option<u8>,
    pub freq: Option<u16>,
    pub on_readings: Vec<String>,
    pub kun_readings: Vec<String>,
    pub nanori: Vec<String>,
    pub meanings: Vec<String>,
    pub variants: Vec<String>,
    /// SVG path data per stroke, in stroke order; empty when unavailable.
    pub strokes: Vec<String>,
}

fn to_json(v: &[String]) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "[]".into())
}

fn from_json(s: Option<String>) -> Vec<String> {
    s.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

impl Db {
    /// Replace the kanji table with the given entries, in one transaction.
    pub fn import_kanji<I>(&self, entries: I) -> Result<u64>
    where
        I: IntoIterator<Item = KanjiRow>,
    {
        let tx = self.conn().unchecked_transaction()?;
        tx.execute("DELETE FROM kanji", [])?;
        let mut count = 0u64;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO kanji(literal, grade, stroke_count, jlpt, freq,
                     on_readings, kun_readings, nanori, meanings, variants, strokes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )?;
            for k in entries {
                stmt.execute(params![
                    k.literal,
                    k.grade,
                    k.stroke_count,
                    k.jlpt,
                    k.freq,
                    to_json(&k.on_readings),
                    to_json(&k.kun_readings),
                    to_json(&k.nanori),
                    to_json(&k.meanings),
                    to_json(&k.variants),
                    if k.strokes.is_empty() {
                        None
                    } else {
                        Some(to_json(&k.strokes))
                    },
                ])?;
                count += 1;
            }
        }
        tx.commit()?;
        Ok(count)
    }

    pub fn kanji_count(&self) -> Result<u64> {
        Ok(self
            .conn()
            .query_row("SELECT COUNT(*) FROM kanji", [], |r| r.get::<_, i64>(0))? as u64)
    }

    /// Look one kanji up by the character itself.
    pub fn kanji(&self, literal: &str) -> Result<Option<KanjiRow>> {
        let result = self.conn().query_row(
            "SELECT literal, grade, stroke_count, jlpt, freq, on_readings,
                    kun_readings, nanori, meanings, variants, strokes
             FROM kanji WHERE literal = ?1",
            [literal],
            |r| {
                Ok(KanjiRow {
                    literal: r.get(0)?,
                    grade: r.get(1)?,
                    stroke_count: r.get(2)?,
                    jlpt: r.get(3)?,
                    freq: r.get(4)?,
                    on_readings: from_json(r.get(5)?),
                    kun_readings: from_json(r.get(6)?),
                    nanori: from_json(r.get(7)?),
                    meanings: from_json(r.get(8)?),
                    variants: from_json(r.get(9)?),
                    strokes: from_json(r.get(10)?),
                })
            },
        );
        match result {
            Ok(k) => Ok(Some(k)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> KanjiRow {
        KanjiRow {
            literal: "亜".into(),
            grade: Some(8),
            stroke_count: 7,
            jlpt: Some(1),
            freq: Some(1509),
            on_readings: vec!["ア".into()],
            kun_readings: vec!["つ.ぐ".into()],
            nanori: vec!["や".into()],
            meanings: vec!["Asia".into(), "rank next".into()],
            variants: vec!["亞".into()],
            strokes: vec!["M10,10c1,1 2,2 3,3".into()],
        }
    }

    #[test]
    fn kanji_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.kanji_count().unwrap(), 0);
        assert!(db.kanji("亜").unwrap().is_none());

        let n = db.import_kanji(vec![sample()]).unwrap();
        assert_eq!(n, 1);
        assert_eq!(db.kanji_count().unwrap(), 1);

        let k = db.kanji("亜").unwrap().unwrap();
        assert_eq!(k.grade, Some(8));
        assert_eq!(k.meanings.len(), 2);
        assert_eq!(k.variants, vec!["亞"]);
        assert_eq!(k.strokes.len(), 1);
    }

    #[test]
    fn reimport_replaces() {
        let db = Db::open_in_memory().unwrap();
        db.import_kanji(vec![sample()]).unwrap();
        let mut other = sample();
        other.literal = "何".into();
        other.strokes = Vec::new();
        db.import_kanji(vec![other]).unwrap();
        assert_eq!(db.kanji_count().unwrap(), 1);
        assert!(db.kanji("亜").unwrap().is_none());
        let k = db.kanji("何").unwrap().unwrap();
        assert!(k.strokes.is_empty());
    }
}
