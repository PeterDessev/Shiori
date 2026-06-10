//! In-memory dictionary with exact-form lookup.

use std::collections::HashMap;

use crate::types::{DictEntry, JmdictFile};

/// A loaded dictionary, indexed by every written form (kanji and kana).
#[derive(Debug, Default)]
pub struct Dictionary {
    entries: Vec<DictEntry>,
    /// Tag code → human-readable description.
    tags: HashMap<String, String>,
    /// form text → indices into `entries`.
    by_form: HashMap<String, Vec<u32>>,
}

impl Dictionary {
    pub fn from_file(file: JmdictFile) -> Self {
        let mut by_form: HashMap<String, Vec<u32>> = HashMap::new();
        for (i, entry) in file.words.iter().enumerate() {
            for form in entry.kanji.iter().chain(entry.kana.iter()) {
                let slot = by_form.entry(form.text.clone()).or_default();
                if !slot.contains(&(i as u32)) {
                    slot.push(i as u32);
                }
            }
        }
        Self {
            entries: file.words,
            tags: file.tags,
            by_form,
        }
    }

    pub fn parse(json: &str) -> Result<Self, crate::DictError> {
        Ok(Self::from_file(JmdictFile::parse(json)?))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[DictEntry] {
        &self.entries
    }

    /// Human-readable description of a JMdict tag code.
    pub fn describe_tag(&self, code: &str) -> Option<&str> {
        self.tags.get(code).map(String::as_str)
    }

    /// All entries having `text` as one of their written forms.
    pub fn lookup(&self, text: &str) -> Vec<&DictEntry> {
        self.by_form
            .get(text)
            .map(|idxs| idxs.iter().map(|&i| &self.entries[i as usize]).collect())
            .unwrap_or_default()
    }

    /// Best-effort lookup of a word identified by lemma and (hiragana)
    /// reading, as produced by the NLP pipeline.
    ///
    /// Candidates matching the lemma are preferred in this order:
    /// 1. entries whose kana forms include the reading (homograph
    ///    disambiguation: 行った→いった picks 行く, not 行なう),
    /// 2. entries marked common,
    /// 3. anything else with the right form.
    pub fn lookup_best(&self, lemma: &str, reading: &str) -> Option<&DictEntry> {
        let candidates = self.lookup(lemma);
        if candidates.is_empty() {
            return None;
        }
        let reading_matches =
            |e: &DictEntry| !reading.is_empty() && e.kana.iter().any(|k| k.text == reading);

        candidates
            .iter()
            .find(|e| reading_matches(e) && e.is_common())
            .or_else(|| candidates.iter().find(|e| reading_matches(e)))
            .or_else(|| candidates.iter().find(|e| e.is_common()))
            .or_else(|| candidates.first())
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
      "version": "test",
      "tags": {"hon": "honorific or respectful (sonkeigo) language"},
      "words": [
        {
          "id": "1",
          "kanji": [{"common": true, "text": "行く", "tags": []}],
          "kana": [{"common": true, "text": "いく", "tags": []}],
          "sense": [{"partOfSpeech": ["v5k-s"], "gloss": [{"text": "to go"}]}]
        },
        {
          "id": "2",
          "kanji": [{"common": false, "text": "行く", "tags": []}],
          "kana": [{"common": false, "text": "ゆく", "tags": []}],
          "sense": [{"partOfSpeech": ["v5k-s"], "gloss": [{"text": "to go (literary)"}]}]
        },
        {
          "id": "3",
          "kanji": [],
          "kana": [{"common": true, "text": "それ", "tags": []}],
          "sense": [{"partOfSpeech": ["pn"], "gloss": [{"text": "that"}]}]
        }
      ]
    }"#;

    fn dict() -> Dictionary {
        Dictionary::parse(FIXTURE).unwrap()
    }

    #[test]
    fn lookup_by_kanji_and_kana_forms() {
        let d = dict();
        assert_eq!(d.lookup("行く").len(), 2);
        assert_eq!(d.lookup("いく").len(), 1);
        assert_eq!(d.lookup("それ").len(), 1);
        assert!(d.lookup("存在しない").is_empty());
    }

    #[test]
    fn lookup_best_prefers_reading_match() {
        let d = dict();
        let hit = d.lookup_best("行く", "ゆく").unwrap();
        assert_eq!(hit.id, "2", "reading must disambiguate homographs");
        let hit = d.lookup_best("行く", "いく").unwrap();
        assert_eq!(hit.id, "1");
    }

    #[test]
    fn lookup_best_falls_back_to_common_entry() {
        let d = dict();
        let hit = d.lookup_best("行く", "").unwrap();
        assert_eq!(hit.id, "1", "common entry wins without a reading");
        assert!(d.lookup_best("ない単語", "ない").is_none());
    }

    #[test]
    fn tag_descriptions_are_available() {
        let d = dict();
        assert_eq!(
            d.describe_tag("hon"),
            Some("honorific or respectful (sonkeigo) language")
        );
        assert_eq!(d.describe_tag("nope"), None);
    }
}
