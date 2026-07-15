//! Finishing a book: sweep the words the user never touched to `known`,
//! with detection of words they plausibly missed marking.

use shiori_core::{DocumentId, KnowledgeStatus, PartOfSpeech, WordId};
use shiori_db::WordRow;

use crate::{App, Result};

/// Don't run the known-band signal on fewer known ranks than this.
const MIN_BAND_SAMPLE: usize = 50;
/// A word this many times rarer than the band edge is suspicious.
const BAND_SLACK: u32 = 2;

/// One word the finish sweep would touch.
#[derive(Debug)]
pub struct SweepCandidate {
    pub word: WordRow,
    pub occurrences: u32,
    pub corpus_rank: Option<u32>,
    /// Plausibly *not* actually known — surfaced for review and excluded
    /// from the sweep by default.
    pub suspicious: bool,
    pub reasons: Vec<String>,
}

/// Everything a finish sweep would do, for the confirmation dialog.
#[derive(Debug, Default)]
pub struct SweepPlan {
    /// Untouched ordinary words → `known` (suspicious ones opt-in).
    pub to_known: Vec<SweepCandidate>,
    /// Untouched proper nouns → `ignored` (names aren't vocabulary).
    pub to_ignored: Vec<SweepCandidate>,
}

impl App {
    /// Plan the finish sweep for a document: every still-`unknown` word,
    /// split into ordinary words and proper nouns, with missed-word
    /// suspicion flags.
    pub fn finish_sweep_plan(&self, document: DocumentId) -> Result<SweepPlan> {
        // The user's known band: where their known vocabulary lives in
        // the corpus frequency ranking.
        let ranks = self.db().known_word_ranks(self.active_lang())?;
        let band_edge = if ranks.len() >= MIN_BAND_SAMPLE {
            Some(ranks[ranks.len() * 9 / 10])
        } else {
            None
        };

        let mut plan = SweepPlan::default();
        for doc_word in self.db().document_words(document)? {
            let word = doc_word.word;
            if word.status != KnowledgeStatus::Unknown {
                continue;
            }
            if !shiori_nlp::kana::is_japanese(&word.key.lemma) {
                continue;
            }
            let corpus_rank = self
                .db()
                .frequency_rank(self.active_lang(), &word.key.lemma)?;

            if word.key.pos == PartOfSpeech::ProperNoun {
                plan.to_ignored.push(SweepCandidate {
                    word,
                    occurrences: doc_word.occurrences,
                    corpus_rank,
                    suspicious: false,
                    reasons: Vec::new(),
                });
                continue;
            }

            let mut reasons = Vec::new();
            if let Some(edge) = band_edge {
                let beyond_band = match corpus_rank {
                    Some(rank) => rank > edge.saturating_mul(BAND_SLACK),
                    // Absent from the frequency list entirely: rarer than
                    // anything ranked.
                    None => true,
                };
                if beyond_band {
                    reasons.push(match corpus_rank {
                        Some(rank) => format!("rank #{rank}, far beyond your known vocabulary"),
                        None => "not in the frequency list (very rare)".to_string(),
                    });
                }
            }
            if doc_word.occurrences <= 2 {
                reasons.push(format!(
                    "appears only {} time{} in this book",
                    doc_word.occurrences,
                    if doc_word.occurrences == 1 { "" } else { "s" }
                ));
            }
            // A future JLPT-level signal slots in here once level data
            // exists (see ROADMAP: statistics expansion).
            plan.to_known.push(SweepCandidate {
                word,
                occurrences: doc_word.occurrences,
                corpus_rank,
                // Both signals must fire: a common word seen once is just
                // a short book, and a rare word seen often was learned by
                // exposure.
                suspicious: reasons.len() >= 2,
                reasons,
            });
        }
        Ok(plan)
    }

    /// Apply a (possibly user-edited) sweep.
    pub fn apply_finish_sweep(&self, known: &[WordId], ignored: &[WordId]) -> Result<()> {
        self.db().bulk_set_status(known, KnowledgeStatus::Known)?;
        self.db()
            .bulk_set_status(ignored, KnowledgeStatus::Ignored)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use shiori_db::{NewSentence, NewToken};

    fn tok(surface: &str, pos: PartOfSpeech) -> NewToken {
        NewToken {
            surface: surface.into(),
            lemma: surface.into(),
            reading: "ヨミ".into(),
            pos,
            start: 0,
            end: surface.len(),
            morph: None,
            gloss: None,
        }
    }

    fn app_with_doc() -> (App, DocumentId) {
        let app = App::with_db(
            shiori_db::Db::open_in_memory().unwrap(),
            std::env::temp_dir(),
        )
        .unwrap();
        let sentences = vec![NewSentence {
            paragraph: 0,
            text: "メロスは猫と京都へ走る。".into(),
            tokens: vec![
                tok("メロス", PartOfSpeech::ProperNoun),
                tok("猫", PartOfSpeech::Noun),
                tok("京都", PartOfSpeech::ProperNoun),
                tok("走る", PartOfSpeech::Verb),
            ],
        }];
        let doc = app
            .db()
            .import_document(
                "ja",
                &shiori_core::DocumentMeta {
                    title: "t".into(),
                    ..Default::default()
                },
                "hash",
                Utc::now(),
                &sentences,
            )
            .unwrap();
        (app, doc)
    }

    #[test]
    fn proper_nouns_split_from_ordinary_words() {
        let (app, doc) = app_with_doc();
        let plan = app.finish_sweep_plan(doc).unwrap();
        let ignored: Vec<&str> = plan
            .to_ignored
            .iter()
            .map(|c| c.word.key.lemma.as_str())
            .collect();
        assert!(ignored.contains(&"メロス"));
        assert!(ignored.contains(&"京都"));
        let known: Vec<&str> = plan
            .to_known
            .iter()
            .map(|c| c.word.key.lemma.as_str())
            .collect();
        assert!(known.contains(&"猫"));
        assert!(known.contains(&"走る"));
    }

    #[test]
    fn explicitly_marked_words_survive_the_sweep() {
        let (app, doc) = app_with_doc();
        // The user marked メロス as learning during reading.
        let melos = app
            .db()
            .find_word(
                "ja",
                &shiori_core::WordKey::new("メロス", "ヨミ", PartOfSpeech::ProperNoun),
            )
            .unwrap()
            .unwrap();
        app.db()
            .set_word_status(melos.id, KnowledgeStatus::Learning)
            .unwrap();

        let plan = app.finish_sweep_plan(doc).unwrap();
        assert!(plan.to_ignored.iter().all(|c| c.word.key.lemma != "メロス"));

        let known: Vec<WordId> = plan.to_known.iter().map(|c| c.word.id).collect();
        let ignored: Vec<WordId> = plan.to_ignored.iter().map(|c| c.word.id).collect();
        app.apply_finish_sweep(&known, &ignored).unwrap();

        assert_eq!(
            app.db().word(melos.id).unwrap().status,
            KnowledgeStatus::Learning,
            "explicit choices are never overwritten"
        );
        let cat = app
            .db()
            .find_word(
                "ja",
                &shiori_core::WordKey::new("猫", "ヨミ", PartOfSpeech::Noun),
            )
            .unwrap()
            .unwrap();
        assert_eq!(cat.status, KnowledgeStatus::Known);
        let kyoto = app
            .db()
            .find_word(
                "ja",
                &shiori_core::WordKey::new("京都", "ヨミ", PartOfSpeech::ProperNoun),
            )
            .unwrap()
            .unwrap();
        assert_eq!(kyoto.status, KnowledgeStatus::Ignored);
    }

    #[test]
    fn suspicion_needs_a_known_band_and_both_signals() {
        let (app, doc) = app_with_doc();
        // Fewer than 50 known ranks: the band signal is off, and a word
        // appearing once is not enough on its own.
        let plan = app.finish_sweep_plan(doc).unwrap();
        assert!(plan.to_known.iter().all(|c| !c.suspicious));
    }
}
