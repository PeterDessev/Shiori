//! Review flow: starting cards, fetching the queue, answering.

use chrono::Utc;
use jrc_core::{KnowledgeStatus, Sentence, SentenceId, WordId};
use jrc_db::WordRow;
use jrc_dict::DictEntry;
use jrc_srs::{Card, Rating};

use crate::{App, AppError, Result};

/// Once a card's stability passes this many days, the word counts as
/// "known" for reading statistics (the card keeps being scheduled).
const KNOWN_STABILITY_DAYS: f64 = 60.0;

/// Everything needed to render one review.
#[derive(Debug)]
pub struct ReviewItem {
    pub word: WordRow,
    pub card: Card,
    /// The sentence the word was mined from. Cards always show the word in
    /// context; this is only `None` if the source document was deleted.
    pub sentence: Option<Sentence>,
    pub entry: Option<DictEntry>,
}

impl App {
    /// Put a word into the SRS, anchored to a context sentence.
    pub fn start_learning(&self, word_id: WordId, sentence_id: SentenceId) -> Result<()> {
        if self.db.card(word_id)?.is_some() {
            return Ok(()); // already learning
        }
        let card = Card::new(Utc::now());
        self.db.upsert_card(word_id, Some(sentence_id), &card)?;
        self.db.set_word_status(word_id, KnowledgeStatus::Learning)?;
        Ok(())
    }

    /// Mark a word as already known (no card needed).
    pub fn mark_known(&self, word_id: WordId) -> Result<()> {
        self.db.delete_card(word_id)?;
        self.db.set_word_status(word_id, KnowledgeStatus::Known)?;
        Ok(())
    }

    /// Exclude a word from mining and statistics (names, noise).
    pub fn ignore_word(&self, word_id: WordId) -> Result<()> {
        self.db.delete_card(word_id)?;
        self.db.set_word_status(word_id, KnowledgeStatus::Ignored)?;
        Ok(())
    }

    /// Reset a word to unknown, dropping any card.
    pub fn reset_word(&self, word_id: WordId) -> Result<()> {
        self.db.delete_card(word_id)?;
        self.db.set_word_status(word_id, KnowledgeStatus::Unknown)?;
        Ok(())
    }

    /// "I forgot this": put a previously known word back into rotation
    /// with a fresh card due immediately. Keeps the old context sentence
    /// when no new one is given.
    pub fn mark_forgotten(&self, word_id: WordId, sentence_id: Option<SentenceId>) -> Result<()> {
        let existing = self.db.card(word_id)?;
        let sentence = sentence_id.or(existing.and_then(|c| c.sentence_id));
        self.db
            .upsert_card(word_id, sentence, &Card::new(Utc::now()))?;
        self.db.set_word_status(word_id, KnowledgeStatus::Learning)?;
        Ok(())
    }

    /// The review queue: due cards with their context, most overdue first.
    pub fn due_reviews(&self, limit: u32) -> Result<Vec<ReviewItem>> {
        let now = Utc::now();
        let mut items = Vec::new();
        for row in self.db.due_cards(now, limit)? {
            let word = self.db.word(row.word_id)?;
            let sentence = match row.sentence_id {
                Some(id) => self.db.sentence(id).ok(),
                None => None,
            };
            let entry = self.dictionary_entry_for(&word)?;
            items.push(ReviewItem {
                word,
                card: row.card,
                sentence,
                entry,
            });
        }
        Ok(items)
    }

    pub fn due_count(&self) -> Result<u64> {
        Ok(self.db.due_count(Utc::now())?)
    }

    /// Answer the current review for `word_id` and persist the outcome.
    ///
    /// Returns the updated card. When stability crosses
    /// [`KNOWN_STABILITY_DAYS`], the word's status is promoted to Known
    /// (scheduling continues regardless).
    pub fn answer_review(&self, word_id: WordId, rating: Rating) -> Result<Card> {
        let now = Utc::now();
        let row = self
            .db
            .card(word_id)?
            .ok_or_else(|| AppError::Invalid(format!("no card for word {}", word_id.0)))?;
        let updated = self.scheduler.review(&row.card, rating, now);
        self.db.upsert_card(word_id, row.sentence_id, &updated)?;
        self.db
            .log_review(word_id, rating, now, updated.stability, updated.difficulty)?;

        let status = if updated.stability >= KNOWN_STABILITY_DAYS {
            KnowledgeStatus::Known
        } else {
            KnowledgeStatus::Learning
        };
        self.db.set_word_status(word_id, status)?;
        Ok(updated)
    }
}
