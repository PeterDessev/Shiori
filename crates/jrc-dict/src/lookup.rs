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
    /// See [`pick_best_entry`] for the preference rules.
    pub fn lookup_best(&self, lemma: &str, reading: &str) -> Option<&DictEntry> {
        let candidates = self.lookup(lemma);
        pick_best_entry(candidates.iter().copied(), lemma, reading)
    }
}

/// Choose the most plausible entry for a (lemma, reading) pair among
/// candidates that share a written form, by score:
///
/// - +4 if a kana form matches the reading (行った→いった picks 行く),
/// - +3 if the lemma is written in kana only and the entry is marked
///   "usually written using kana alone" — こと should resolve to 事
///   (thing), not 琴 (the zither), even though both are common,
/// - +2 if any form is marked common.
///
/// Ties keep the first candidate (callers pass common-first ordering).
pub fn pick_best_entry<'a>(
    candidates: impl Iterator<Item = &'a DictEntry>,
    lemma: &str,
    reading: &str,
) -> Option<&'a DictEntry> {
    let kana_only_lemma = is_kana_only(lemma);
    let mut best: Option<(&DictEntry, i32)> = None;
    for entry in candidates {
        let mut score = 0;
        if !reading.is_empty() && entry.kana.iter().any(|k| k.text == reading) {
            score += 4;
        }
        if kana_only_lemma && entry.misc_codes().contains(&"uk") {
            score += 3;
        }
        if entry.is_common() {
            score += 2;
        }
        match best {
            Some((_, s)) if s >= score => {}
            _ => best = Some((entry, score)),
        }
    }
    best.map(|(e, _)| e)
}

/// Hiragana/katakana (incl. ー) only. Duplicated from `jrc-nlp` to keep
/// this crate free of the heavyweight NLP dependency.
fn is_kana_only(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| matches!(c as u32, 0x3041..=0x30FF))
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
    fn kana_only_lemma_prefers_usually_kana_entry() {
        // こと is the kana form of both 琴 (zither) and 事 (thing). Only
        // 事 is marked "uk"; it must win for the kana-only lemma even
        // though 琴 is also common and shares the reading.
        let json = r#"{
          "version": "test",
          "tags": {},
          "words": [
            {"id": "1240650",
             "kanji": [{"common": true, "text": "琴", "tags": []}],
             "kana": [{"common": true, "text": "こと", "tags": []}],
             "sense": [{"partOfSpeech": ["n"], "misc": [],
                        "gloss": [{"text": "koto (zither)"}]}]},
            {"id": "1313580",
             "kanji": [{"common": true, "text": "事", "tags": []}],
             "kana": [{"common": true, "text": "こと", "tags": []}],
             "sense": [{"partOfSpeech": ["n"], "misc": ["uk"],
                        "gloss": [{"text": "thing; matter"}]}]}
          ]
        }"#;
        let d = Dictionary::parse(json).unwrap();
        let hit = d.lookup_best("こと", "こと").unwrap();
        assert_eq!(hit.id, "1313580", "uk entry must win for kana lemma");
        // A kanji lemma still resolves normally.
        let hit = d.lookup_best("琴", "こと").unwrap();
        assert_eq!(hit.id, "1240650");
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
