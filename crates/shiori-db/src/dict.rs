//! Cached dictionary entries and frequency ranks.
//!
//! Entries are opaque JSON strings (jmdict-simplified per-word objects);
//! interpreting them is `shiori-app`'s job via `shiori-dict`.

use rusqlite::params;

use crate::{Db, Result};

/// One written form of a dictionary entry, for the lookup index.
#[derive(Debug, Clone)]
pub struct DictFormRow {
    pub text: String,
    pub is_kana: bool,
    pub is_common: bool,
}

impl Db {
    pub fn dict_entry_count(&self) -> Result<u64> {
        let n: i64 = self
            .conn()
            .query_row("SELECT COUNT(*) FROM dict_entries", [], |r| r.get(0))?;
        Ok(n as u64)
    }

    /// Bulk-import dictionary entries in one transaction, replacing any
    /// previous copy (idempotent re-import).
    pub fn import_dictionary<I>(&self, entries: I) -> Result<u64>
    where
        I: IntoIterator<Item = (i64, String, Vec<DictFormRow>)>,
    {
        let tx = self.conn().unchecked_transaction()?;
        tx.execute("DELETE FROM dict_forms", [])?;
        tx.execute("DELETE FROM dict_entries", [])?;
        let mut count = 0u64;
        {
            let mut insert_entry =
                tx.prepare("INSERT OR REPLACE INTO dict_entries(seq, json) VALUES (?1, ?2)")?;
            let mut insert_form = tx.prepare(
                "INSERT OR IGNORE INTO dict_forms(text, seq, is_kana, is_common)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (seq, json, forms) in entries {
                insert_entry.execute(params![seq, json])?;
                for form in forms {
                    insert_form.execute(params![form.text, seq, form.is_kana, form.is_common])?;
                }
                count += 1;
            }
        }
        tx.commit()?;
        Ok(count)
    }

    /// Sequence ids of entries having `text` as a written form, common
    /// entries first.
    pub fn dict_lookup_seqs(&self, text: &str) -> Result<Vec<i64>> {
        let mut stmt = self.conn().prepare(
            "SELECT seq FROM dict_forms WHERE text = ?1
             ORDER BY is_common DESC, seq",
        )?;
        let rows = stmt.query_map([text], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Sequence ids whose forms start with `prefix`, exact matches and
    /// common entries first. Powers the dictionary search box.
    pub fn dict_search_seqs(&self, prefix: &str, limit: u32) -> Result<Vec<i64>> {
        // LIKE with an escaped prefix; % and _ in user input are literal.
        let escaped = prefix
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let mut stmt = self.conn().prepare(
            "SELECT seq, MAX(text = ?1) AS exact, MAX(is_common) AS common
             FROM dict_forms WHERE text LIKE ?2 || '%' ESCAPE '\\'
             GROUP BY seq
             ORDER BY exact DESC, common DESC, LENGTH(MIN(text)), seq
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![prefix, escaped, limit], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn dict_entry_json(&self, seq: i64) -> Result<Option<String>> {
        let result =
            self.conn()
                .query_row("SELECT json FROM dict_entries WHERE seq = ?1", [seq], |r| {
                    r.get(0)
                });
        match result {
            Ok(json) => Ok(Some(json)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Bulk-import frequency ranks, replacing any previous list.
    pub fn import_frequency<'a, I>(&self, ranks: I) -> Result<u64>
    where
        I: IntoIterator<Item = (&'a str, u32)>,
    {
        let tx = self.conn().unchecked_transaction()?;
        tx.execute("DELETE FROM frequency", [])?;
        let mut count = 0u64;
        {
            let mut insert =
                tx.prepare("INSERT OR REPLACE INTO frequency(word, rank) VALUES (?1, ?2)")?;
            for (word, rank) in ranks {
                insert.execute(params![word, rank])?;
                count += 1;
            }
        }
        tx.commit()?;
        Ok(count)
    }

    pub fn frequency_rank(&self, word: &str) -> Result<Option<u32>> {
        let result =
            self.conn()
                .query_row("SELECT rank FROM frequency WHERE word = ?1", [word], |r| {
                    r.get::<_, i64>(0)
                });
        match result {
            Ok(rank) => Ok(Some(rank as u32)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn frequency_count(&self) -> Result<u64> {
        let n: i64 = self
            .conn()
            .query_row("SELECT COUNT(*) FROM frequency", [], |r| r.get(0))?;
        Ok(n as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_import_and_lookup() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.dict_entry_count().unwrap(), 0);

        db.import_dictionary([
            (
                100,
                r#"{"id":"100"}"#.to_string(),
                vec![
                    DictFormRow {
                        text: "行く".into(),
                        is_kana: false,
                        is_common: true,
                    },
                    DictFormRow {
                        text: "いく".into(),
                        is_kana: true,
                        is_common: true,
                    },
                ],
            ),
            (
                200,
                r#"{"id":"200"}"#.to_string(),
                vec![DictFormRow {
                    text: "行く".into(),
                    is_kana: false,
                    is_common: false,
                }],
            ),
        ])
        .unwrap();

        assert_eq!(db.dict_entry_count().unwrap(), 2);
        // Common entry sorts first.
        assert_eq!(db.dict_lookup_seqs("行く").unwrap(), vec![100, 200]);
        assert_eq!(db.dict_lookup_seqs("いく").unwrap(), vec![100]);
        assert!(db.dict_lookup_seqs("ない").unwrap().is_empty());
        assert_eq!(
            db.dict_entry_json(100).unwrap().as_deref(),
            Some(r#"{"id":"100"}"#)
        );
        assert_eq!(db.dict_entry_json(999).unwrap(), None);
    }

    #[test]
    fn reimport_replaces_dictionary() {
        let db = Db::open_in_memory().unwrap();
        db.import_dictionary([(1, "{}".to_string(), vec![])])
            .unwrap();
        db.import_dictionary([(2, "{}".to_string(), vec![])])
            .unwrap();
        assert_eq!(db.dict_entry_count().unwrap(), 1);
        assert_eq!(db.dict_entry_json(1).unwrap(), None);
    }

    #[test]
    fn frequency_import_and_rank() {
        let db = Db::open_in_memory().unwrap();
        db.import_frequency([("の", 1), ("猫", 500)]).unwrap();
        assert_eq!(db.frequency_rank("の").unwrap(), Some(1));
        assert_eq!(db.frequency_rank("猫").unwrap(), Some(500));
        assert_eq!(db.frequency_rank("珍語").unwrap(), None);
        assert_eq!(db.frequency_count().unwrap(), 2);
    }
}
