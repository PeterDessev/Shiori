//! Cached dictionary entries and frequency ranks.
//!
//! Entries are opaque JSON strings (for Japanese, jmdict-simplified
//! per-word objects); interpreting them is `shiori-app`'s job via
//! `shiori-dict`. Every entry belongs to a `source` (e.g. 'jmdict') and
//! every frequency row to a language, so imports replace only their own
//! source's rows.

use rusqlite::params;

use crate::{Db, Result};

/// What kind of written form a dictionary form is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormRole {
    /// A spelling (kanji form, or the written form of an alphabetic
    /// language).
    Orthographic,
    /// A pronunciation form (kana reading).
    Phonetic,
    /// A lexicon's citation form (dictionary headword as cited).
    Canonical,
}

impl FormRole {
    pub fn as_str(self) -> &'static str {
        match self {
            FormRole::Orthographic => "orthographic",
            FormRole::Phonetic => "phonetic",
            FormRole::Canonical => "canonical",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "phonetic" => FormRole::Phonetic,
            "canonical" => FormRole::Canonical,
            _ => FormRole::Orthographic,
        }
    }
}

/// One written form of a dictionary entry, for the lookup index.
#[derive(Debug, Clone)]
pub struct DictFormRow {
    pub text: String,
    pub role: FormRole,
    pub is_common: bool,
}

impl Db {
    pub fn dict_entry_count(&self, source: &str) -> Result<u64> {
        let n: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM dict_entries WHERE source = ?1",
            [source],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// Bulk-import one source's dictionary entries in one transaction,
    /// replacing any previous copy of *that source* (idempotent
    /// re-import; other sources are untouched).
    pub fn import_dictionary<I>(&self, source: &str, entries: I) -> Result<u64>
    where
        I: IntoIterator<Item = (String, String, Vec<DictFormRow>)>,
    {
        let tx = self.conn().unchecked_transaction()?;
        tx.execute("DELETE FROM dict_forms WHERE source = ?1", [source])?;
        tx.execute("DELETE FROM dict_entries WHERE source = ?1", [source])?;
        let mut count = 0u64;
        {
            let mut insert_entry = tx.prepare(
                "INSERT OR REPLACE INTO dict_entries(source, entry_key, json)
                 VALUES (?1, ?2, ?3)",
            )?;
            let mut insert_form = tx.prepare(
                "INSERT OR IGNORE INTO dict_forms(source, text, entry_key, role, is_common)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (entry_key, json, forms) in entries {
                insert_entry.execute(params![source, entry_key, json])?;
                for form in forms {
                    insert_form.execute(params![
                        source,
                        form.text,
                        entry_key,
                        form.role.as_str(),
                        form.is_common
                    ])?;
                }
                count += 1;
            }
        }
        tx.commit()?;
        Ok(count)
    }

    /// Entry keys of a source's entries having `text` as a written form,
    /// common entries first.
    pub fn dict_lookup_keys(&self, source: &str, text: &str) -> Result<Vec<String>> {
        // Numeric keys (JMdict sequence ids) order numerically, so results
        // match the pre-v8 integer ordering; non-numeric keys tie at 0 and
        // fall back to text order.
        let mut stmt = self.conn().prepare(
            "SELECT entry_key FROM dict_forms WHERE source = ?1 AND text = ?2
             ORDER BY is_common DESC, CAST(entry_key AS INTEGER), entry_key",
        )?;
        let rows = stmt.query_map([source, text], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Entry keys whose forms start with `prefix`, exact matches and
    /// common entries first. Powers the dictionary search box.
    pub fn dict_search_keys(&self, source: &str, prefix: &str, limit: u32) -> Result<Vec<String>> {
        // LIKE with an escaped prefix; % and _ in user input are literal.
        let escaped = prefix
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let mut stmt = self.conn().prepare(
            "SELECT entry_key, MAX(text = ?2) AS exact, MAX(is_common) AS common
             FROM dict_forms WHERE source = ?1 AND text LIKE ?3 || '%' ESCAPE '\\'
             GROUP BY entry_key
             ORDER BY exact DESC, common DESC, LENGTH(MIN(text)),
                      CAST(entry_key AS INTEGER), entry_key
             LIMIT ?4",
        )?;
        let rows = stmt.query_map(params![source, prefix, escaped, limit], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn dict_entry_json(&self, source: &str, entry_key: &str) -> Result<Option<String>> {
        let result = self.conn().query_row(
            "SELECT json FROM dict_entries WHERE source = ?1 AND entry_key = ?2",
            [source, entry_key],
            |r| r.get(0),
        );
        match result {
            Ok(json) => Ok(Some(json)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Bulk-import one language's frequency ranks, replacing any previous
    /// list for that language only.
    pub fn import_frequency<'a, I>(&self, lang: &str, ranks: I) -> Result<u64>
    where
        I: IntoIterator<Item = (&'a str, u32)>,
    {
        let tx = self.conn().unchecked_transaction()?;
        tx.execute("DELETE FROM frequency WHERE lang = ?1", [lang])?;
        let mut count = 0u64;
        {
            let mut insert = tx.prepare(
                "INSERT OR REPLACE INTO frequency(lang, word, rank) VALUES (?1, ?2, ?3)",
            )?;
            for (word, rank) in ranks {
                insert.execute(params![lang, word, rank])?;
                count += 1;
            }
        }
        tx.commit()?;
        Ok(count)
    }

    pub fn frequency_rank(&self, lang: &str, word: &str) -> Result<Option<u32>> {
        let result = self.conn().query_row(
            "SELECT rank FROM frequency WHERE lang = ?1 AND word = ?2",
            [lang, word],
            |r| r.get::<_, i64>(0),
        );
        match result {
            Ok(rank) => Ok(Some(rank as u32)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn frequency_count(&self, lang: &str) -> Result<u64> {
        let n: i64 = self.conn().query_row(
            "SELECT COUNT(*) FROM frequency WHERE lang = ?1",
            [lang],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(text: &str, role: FormRole, is_common: bool) -> DictFormRow {
        DictFormRow {
            text: text.into(),
            role,
            is_common,
        }
    }

    #[test]
    fn dictionary_import_and_lookup() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.dict_entry_count("jmdict").unwrap(), 0);

        db.import_dictionary(
            "jmdict",
            [
                (
                    "100".to_string(),
                    r#"{"id":"100"}"#.to_string(),
                    vec![
                        form("行く", FormRole::Orthographic, true),
                        form("いく", FormRole::Phonetic, true),
                    ],
                ),
                (
                    "200".to_string(),
                    r#"{"id":"200"}"#.to_string(),
                    vec![form("行く", FormRole::Orthographic, false)],
                ),
            ],
        )
        .unwrap();

        assert_eq!(db.dict_entry_count("jmdict").unwrap(), 2);
        // Common entry sorts first.
        assert_eq!(
            db.dict_lookup_keys("jmdict", "行く").unwrap(),
            vec!["100", "200"]
        );
        assert_eq!(db.dict_lookup_keys("jmdict", "いく").unwrap(), vec!["100"]);
        assert!(db.dict_lookup_keys("jmdict", "ない").unwrap().is_empty());
        // Other sources don't see these forms.
        assert!(db.dict_lookup_keys("lsj", "行く").unwrap().is_empty());
        assert_eq!(
            db.dict_entry_json("jmdict", "100").unwrap().as_deref(),
            Some(r#"{"id":"100"}"#)
        );
        assert_eq!(db.dict_entry_json("jmdict", "999").unwrap(), None);
    }

    #[test]
    fn numeric_keys_order_numerically() {
        let db = Db::open_in_memory().unwrap();
        db.import_dictionary(
            "jmdict",
            [
                (
                    "99".to_string(),
                    "{}".to_string(),
                    vec![form("語", FormRole::Orthographic, false)],
                ),
                (
                    "100".to_string(),
                    "{}".to_string(),
                    vec![form("語", FormRole::Orthographic, false)],
                ),
            ],
        )
        .unwrap();
        // Lexicographic TEXT order would put "100" before "99".
        assert_eq!(
            db.dict_lookup_keys("jmdict", "語").unwrap(),
            vec!["99", "100"]
        );
    }

    #[test]
    fn reimport_replaces_only_own_source() {
        let db = Db::open_in_memory().unwrap();
        db.import_dictionary("jmdict", [("1".to_string(), "{}".to_string(), vec![])])
            .unwrap();
        db.import_dictionary("lsj", [("λόγος".to_string(), "{}".to_string(), vec![])])
            .unwrap();
        // Re-importing jmdict replaces jmdict…
        db.import_dictionary("jmdict", [("2".to_string(), "{}".to_string(), vec![])])
            .unwrap();
        assert_eq!(db.dict_entry_count("jmdict").unwrap(), 1);
        assert_eq!(db.dict_entry_json("jmdict", "1").unwrap(), None);
        // …but never touches another source.
        assert_eq!(db.dict_entry_count("lsj").unwrap(), 1);
        assert!(db.dict_entry_json("lsj", "λόγος").unwrap().is_some());
    }

    #[test]
    fn frequency_import_and_rank_scoped_by_language() {
        let db = Db::open_in_memory().unwrap();
        db.import_frequency("ja", [("の", 1), ("猫", 500)]).unwrap();
        db.import_frequency("grc", [("καί", 1)]).unwrap();
        assert_eq!(db.frequency_rank("ja", "の").unwrap(), Some(1));
        assert_eq!(db.frequency_rank("ja", "猫").unwrap(), Some(500));
        assert_eq!(db.frequency_rank("ja", "珍語").unwrap(), None);
        assert_eq!(db.frequency_rank("grc", "καί").unwrap(), Some(1));
        assert_eq!(db.frequency_rank("grc", "猫").unwrap(), None);
        assert_eq!(db.frequency_count("ja").unwrap(), 2);
        assert_eq!(db.frequency_count("grc").unwrap(), 1);
        // Re-importing one language leaves the other intact.
        db.import_frequency("ja", [("犬", 700)]).unwrap();
        assert_eq!(db.frequency_count("ja").unwrap(), 1);
        assert_eq!(db.frequency_count("grc").unwrap(), 1);
    }
}
