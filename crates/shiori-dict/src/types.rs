//! JMdict entry types, mirroring the jmdict-simplified JSON schema.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// The whole jmdict-simplified file.
#[derive(Debug, Clone, Deserialize)]
pub struct JmdictFile {
    pub version: String,
    /// Tag code → human-readable description (e.g. "col" → "colloquial").
    pub tags: HashMap<String, String>,
    pub words: Vec<DictEntry>,
}

impl JmdictFile {
    pub fn parse(json: &str) -> Result<Self, crate::DictError> {
        Ok(serde_json::from_str(json)?)
    }
}

/// One dictionary entry (a JMdict word).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictEntry {
    /// JMdict sequence id (stringly-typed in the source JSON).
    pub id: String,
    #[serde(default)]
    pub kanji: Vec<Form>,
    #[serde(default)]
    pub kana: Vec<Form>,
    #[serde(default, rename = "sense")]
    pub senses: Vec<Sense>,
}

/// A written form (kanji spelling or kana reading) of an entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Form {
    pub text: String,
    #[serde(default)]
    pub common: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// One sense (meaning) of an entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sense {
    #[serde(default, rename = "partOfSpeech")]
    pub part_of_speech: Vec<String>,
    #[serde(default)]
    pub gloss: Vec<Gloss>,
    /// Register/usage tag codes: "col", "arch", "hon", "uk", "sl", …
    #[serde(default)]
    pub misc: Vec<String>,
    /// Subject field codes: "comp", "med", …
    #[serde(default)]
    pub field: Vec<String>,
    #[serde(default)]
    pub dialect: Vec<String>,
    /// Free-form usage notes.
    #[serde(default)]
    pub info: Vec<String>,
    /// Cross-references to related entries; each is a list of strings
    /// (form, optional reading) possibly ending with a sense number.
    #[serde(default)]
    pub related: Vec<Vec<serde_json::Value>>,
    #[serde(default)]
    pub antonym: Vec<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gloss {
    pub text: String,
}

impl DictEntry {
    /// Numeric JMdict sequence id.
    pub fn seq(&self) -> i64 {
        self.id.parse().unwrap_or(0)
    }

    /// The headword to display: first common kanji form, else first kanji
    /// form, else the first kana form.
    pub fn headword(&self) -> &str {
        self.kanji
            .iter()
            .find(|f| f.common)
            .or_else(|| self.kanji.first())
            .map(|f| f.text.as_str())
            .or_else(|| self.kana.first().map(|f| f.text.as_str()))
            .unwrap_or("")
    }

    /// The primary reading (first common kana form, else first kana form).
    pub fn reading(&self) -> &str {
        self.kana
            .iter()
            .find(|f| f.common)
            .or_else(|| self.kana.first())
            .map(|f| f.text.as_str())
            .unwrap_or("")
    }

    /// Whether any form is marked common.
    pub fn is_common(&self) -> bool {
        self.kanji.iter().chain(self.kana.iter()).any(|f| f.common)
    }

    /// English glosses of the first sense, joined for compact display.
    pub fn short_gloss(&self) -> String {
        self.senses
            .first()
            .map(|s| {
                s.gloss
                    .iter()
                    .map(|g| g.text.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            })
            .unwrap_or_default()
    }

    /// Distinct part-of-speech labels across all senses, in first-seen
    /// order — the word class, verb paradigm, and transitivity, rendered
    /// for display (e.g. `["Godan verb (-ru)", "transitive verb"]`).
    /// Codes this crate does not name are kept verbatim.
    pub fn pos_labels(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for sense in &self.senses {
            for code in &sense.part_of_speech {
                let label = crate::pos::pos_label(code).unwrap_or(code).to_string();
                if !out.contains(&label) {
                    out.push(label);
                }
            }
        }
        out
    }

    /// All distinct misc (register/usage) codes across senses.
    pub fn misc_codes(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for sense in &self.senses {
            for code in &sense.misc {
                if !out.contains(&code.as_str()) {
                    out.push(code);
                }
            }
        }
        out
    }

    /// Cross-referenced related words, rendered for display
    /// (e.g. `手の内・てのうち`).
    pub fn related_words(&self) -> Vec<String> {
        self.senses
            .iter()
            .flat_map(|s| s.related.iter())
            .filter_map(|x| render_xref(x))
            .collect()
    }

    /// Antonyms, rendered for display.
    pub fn antonyms(&self) -> Vec<String> {
        self.senses
            .iter()
            .flat_map(|s| s.antonym.iter())
            .filter_map(|x| render_xref(x))
            .collect()
    }
}

fn render_xref(xref: &[serde_json::Value]) -> Option<String> {
    let parts: Vec<&str> = xref.iter().filter_map(|v| v.as_str()).collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("・"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) const FIXTURE: &str = r#"{
      "version": "test",
      "tags": {
        "col": "colloquial",
        "uk": "word usually written using kana alone",
        "hon": "honorific or respectful (sonkeigo) language",
        "n": "noun (common) (futsuumeishi)",
        "v1": "Ichidan verb"
      },
      "words": [
        {
          "id": "1358280",
          "kanji": [{"common": true, "text": "食べる", "tags": []}],
          "kana": [{"common": true, "text": "たべる", "tags": [], "appliesToKanji": ["*"]}],
          "sense": [
            {
              "partOfSpeech": ["v1"],
              "related": [["食う","くう"]],
              "antonym": [],
              "field": [],
              "dialect": [],
              "misc": [],
              "info": [],
              "gloss": [{"lang": "eng", "text": "to eat"}]
            },
            {
              "partOfSpeech": ["v1"],
              "related": [],
              "antonym": [],
              "field": [],
              "dialect": [],
              "misc": ["col"],
              "info": [],
              "gloss": [{"lang": "eng", "text": "to live on (e.g. a salary)"}]
            }
          ]
        },
        {
          "id": "1577100",
          "kanji": [{"common": false, "text": "召し上がる", "tags": []}],
          "kana": [{"common": true, "text": "めしあがる", "tags": []}],
          "sense": [
            {
              "partOfSpeech": ["v5r"],
              "related": [["食べる"]],
              "antonym": [],
              "misc": ["hon"],
              "gloss": [{"lang": "eng", "text": "to eat (honorific)"}]
            }
          ]
        }
      ]
    }"#;

    #[test]
    fn parses_fixture() {
        let file = JmdictFile::parse(FIXTURE).unwrap();
        assert_eq!(file.words.len(), 2);
        assert_eq!(file.tags["col"], "colloquial");

        let taberu = &file.words[0];
        assert_eq!(taberu.seq(), 1358280);
        assert_eq!(taberu.headword(), "食べる");
        assert_eq!(taberu.reading(), "たべる");
        assert!(taberu.is_common());
        assert_eq!(taberu.short_gloss(), "to eat");
        assert_eq!(taberu.misc_codes(), vec!["col"]);
        assert_eq!(taberu.related_words(), vec!["食う・くう"]);
        assert_eq!(taberu.pos_labels(), vec!["Ichidan verb"]);
        assert_eq!(file.words[1].pos_labels(), vec!["Godan verb (-ru)"]);
    }

    #[test]
    fn kana_only_headword_falls_back() {
        let entry = DictEntry {
            id: "1".into(),
            kanji: vec![],
            kana: vec![Form {
                text: "それ".into(),
                common: true,
                tags: vec![],
            }],
            senses: vec![],
        };
        assert_eq!(entry.headword(), "それ");
        assert_eq!(entry.short_gloss(), "");
    }
}
