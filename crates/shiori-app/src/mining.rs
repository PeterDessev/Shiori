//! Vocabulary mining: which unknown words are most worth learning?

use shiori_core::{DocumentId, KnowledgeStatus, Sentence};
use shiori_db::WordRow;
use shiori_dict::DictEntry;

use crate::{App, Result};

/// An unknown word proposed for study, with everything needed to decide.
#[derive(Debug)]
pub struct MiningCandidate {
    pub word: WordRow,
    /// Occurrences within the mined document.
    pub occurrences: u32,
    /// Corpus frequency rank (1 = most frequent), if the word is in the
    /// frequency list.
    pub corpus_rank: Option<u32>,
    /// The first sentence of the document containing the word — the card's
    /// natural context.
    pub sentence: Sentence,
    /// Resolved dictionary entry, if found.
    pub entry: Option<DictEntry>,
    /// Usefulness score; higher = learn sooner.
    pub score: f64,
}

impl App {
    /// Unknown content words of a document, most useful first.
    pub fn mining_candidates(&self, document: DocumentId) -> Result<Vec<MiningCandidate>> {
        let mut out = Vec::new();
        for doc_word in self.db.document_words(document)? {
            let word = &doc_word.word;
            if word.status != KnowledgeStatus::Unknown || !word.key.pos.is_content_word() {
                continue;
            }
            // Skip out-of-language noise (foreign fragments, punctuation).
            if !self.service().is_target_language(&word.key.lemma) {
                continue;
            }
            let corpus_rank = self.corpus_rank(word)?;
            let entry = self.dictionary_entry_for(word)?;
            let score = usefulness_score(doc_word.occurrences, corpus_rank);
            out.push(MiningCandidate {
                sentence: self.db.sentence(doc_word.first_sentence_id)?,
                occurrences: doc_word.occurrences,
                corpus_rank,
                entry,
                score,
                word: doc_word.word,
            });
        }
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        Ok(out)
    }

    /// Frequency rank of a word, trying the language's lookup forms in
    /// order (Japanese also tries the reading; its list mixes scripts).
    fn corpus_rank(&self, word: &WordRow) -> Result<Option<u32>> {
        let lang = self.active_lang();
        for form in self
            .service()
            .frequency_forms(&word.key.lemma, &word.key.reading)
        {
            if let Some(rank) = self.db.frequency_rank(lang, &form)? {
                return Ok(Some(rank));
            }
        }
        Ok(None)
    }

    /// Resolve (and cache) the dictionary entry for a word.
    pub fn dictionary_entry_for(&self, word: &WordRow) -> Result<Option<DictEntry>> {
        // Cached resolution first.
        if let Some(dict_ref) = &word.dict_ref {
            if let Some(json) = self.db.dict_entry_json(&dict_ref.source, &dict_ref.key)? {
                return Ok(Some(serde_json::from_str(&json).map_err(|e| {
                    crate::AppError::Invalid(format!(
                        "corrupt dictionary entry {}/{}: {e}",
                        dict_ref.source, dict_ref.key
                    ))
                })?));
            }
        }

        let source = self.active_dict_source();
        let keys = self.db.dict_lookup_keys(source, &word.key.lemma)?;
        let mut candidates = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(json) = self.db.dict_entry_json(source, &key)? {
                if let Ok(entry) = serde_json::from_str::<DictEntry>(&json) {
                    candidates.push(entry);
                }
            }
        }
        let best =
            shiori_dict::pick_best_entry(candidates.iter(), &word.key.lemma, &word.key.reading)
                .cloned();
        if let Some(entry) = &best {
            self.db.set_word_dict_ref(
                word.id,
                Some(&shiori_db::DictRef {
                    source: source.to_string(),
                    key: entry.seq().to_string(),
                }),
            )?;
        }
        Ok(best)
    }
}

impl App {
    /// Look up an arbitrary surface string in the dictionary — used for
    /// analyzer-split compounds like 低声 (prefix 低 + noun 声) that JMdict
    /// knows as one word.
    pub fn lookup_compound(&self, surface: &str) -> Result<Option<DictEntry>> {
        let source = self.active_dict_source();
        let keys = self.db.dict_lookup_keys(source, surface)?;
        let mut candidates = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(json) = self.db.dict_entry_json(source, &key)? {
                if let Ok(entry) = serde_json::from_str::<DictEntry>(&json) {
                    candidates.push(entry);
                }
            }
        }
        Ok(shiori_dict::pick_best_entry(candidates.iter(), surface, "").cloned())
    }
}

/// Usefulness of learning a word: grows with in-document occurrences and
/// with corpus frequency (low rank). Words absent from the frequency list
/// rank below equally-frequent listed words.
fn usefulness_score(occurrences: u32, corpus_rank: Option<u32>) -> f64 {
    let occ_component = (1.0 + f64::from(occurrences)).ln() * 2.0;
    let rank_component = match corpus_rank {
        Some(rank) => 12.0 - f64::from(rank).max(1.0).ln(),
        None => 0.0,
    };
    occ_component + rank_component
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_prefers_frequent_corpus_words() {
        // Same in-doc occurrences: lower corpus rank wins.
        let common = usefulness_score(2, Some(100));
        let rare = usefulness_score(2, Some(40_000));
        let unlisted = usefulness_score(2, None);
        assert!(common > rare);
        assert!(rare > unlisted);
    }

    #[test]
    fn score_grows_with_occurrences() {
        assert!(usefulness_score(10, Some(5000)) > usefulness_score(1, Some(5000)));
    }

    #[test]
    fn many_occurrences_can_outweigh_corpus_rank() {
        // A word all over this document beats a slightly-more-common word
        // appearing once.
        assert!(usefulness_score(20, Some(8000)) > usefulness_score(1, Some(3000)));
    }
}
