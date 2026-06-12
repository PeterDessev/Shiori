//! Export/import: Anki decks, plus database backup hooks.
//!
//! FSRS and SM-2 describe memory differently; the conversions here are
//! the standard approximations. With Anki's default 90% desired
//! retention an SM-2 interval is roughly the FSRS stability, and ease
//! maps linearly onto difficulty around the 2.5 ⇄ 5.0 midpoint.

use std::path::Path;

use chrono::Utc;
use shiori_core::KnowledgeStatus;
use shiori_db::anki::{AnkiNote, AnkiSchedule, ImportedNote};
use shiori_srs::{Card, CardState};

use crate::{App, AppError, Result};

/// Stability at which a word counts as known (matches the review flow).
const KNOWN_STABILITY_DAYS: f64 = 60.0;

fn factor_from_difficulty(difficulty: f64) -> u32 {
    // difficulty 5 → 2500; each difficulty point ≈ 300 permille of ease.
    ((2500.0 + (5.0 - difficulty) * 300.0).clamp(1300.0, 3700.0)) as u32
}

fn difficulty_from_factor(factor: u32) -> f64 {
    let factor = if factor == 0 { 2500 } else { factor };
    (5.0 + (2500.0 - f64::from(factor)) / 300.0).clamp(1.0, 10.0)
}

impl App {
    /// Export every SRS card as an Anki deck. Returns the note count.
    pub fn export_apkg(&self, path: &Path) -> Result<usize> {
        let now = Utc::now();
        let mut notes = Vec::new();
        for row in self.db().all_cards()? {
            let word = self.db().word(row.word_id)?;
            let meaning = self
                .dictionary_entry_for(&word)?
                .map(|e| e.short_gloss())
                .unwrap_or_default();
            let sentence = match row.sentence_id {
                Some(id) => self.db().sentence(id).map(|s| s.text).unwrap_or_default(),
                None => String::new(),
            };
            let schedule = (row.card.state != CardState::New).then(|| AnkiSchedule {
                due_in_days: (row.card.due - now).num_days(),
                interval_days: row.card.stability.round().max(1.0) as u32,
                factor: factor_from_difficulty(row.card.difficulty),
                reps: row.card.reps,
                lapses: row.card.lapses,
            });
            notes.push(AnkiNote {
                // The "jrc-" prefix predates the Shiori rename; it must
                // stay stable or re-imports into Anki would duplicate
                // every previously exported note.
                guid: format!(
                    "jrc-{}-{}-{}",
                    word.key.lemma,
                    word.key.reading,
                    word.key.pos.as_str()
                ),
                fields: [
                    word.key.lemma.clone(),
                    word.key.reading.clone(),
                    meaning,
                    sentence,
                ],
                schedule,
            });
        }
        if notes.is_empty() {
            return Err(AppError::Invalid(
                "no SRS cards to export — learn some words first".into(),
            ));
        }
        let count = notes.len();
        shiori_db::anki::write_apkg(path, "Shiori", &notes)?;
        Ok(count)
    }

    /// Import an Anki deck: each note's first Japanese field becomes (or
    /// matches) a tracked word; SM-2 scheduling seeds the FSRS state
    /// approximately. Words that already have a card are skipped.
    /// Returns (imported, skipped).
    pub fn import_apkg(&self, path: &Path) -> Result<(usize, usize)> {
        let notes = shiori_db::anki::read_apkg(path)?;
        let now = Utc::now();
        let mut imported = 0;
        let mut skipped = 0;
        for note in &notes {
            match self.import_note(note, now) {
                Ok(true) => imported += 1,
                Ok(false) => skipped += 1,
                Err(_) => skipped += 1,
            }
        }
        Ok((imported, skipped))
    }

    fn import_note(&self, note: &ImportedNote, now: chrono::DateTime<Utc>) -> Result<bool> {
        // The expression is the first field containing Japanese.
        let Some(expression) = note
            .fields
            .iter()
            .map(|f| crate::extract::strip_html(f))
            .find(|f| shiori_nlp::kana::is_japanese(f.trim()))
        else {
            return Ok(false);
        };
        let expression = expression.trim().to_string();
        let analyzed = self.analyze_chat_text(&expression)?;
        let Some(head) = analyzed.first().and_then(|(tokens, _)| tokens.first()) else {
            return Ok(false);
        };
        let key = shiori_core::WordKey {
            lemma: head.token.lemma.clone(),
            reading: head.token.reading.clone(),
            pos: head.token.pos,
        };
        let word = self.db().ensure_word(&key)?;
        if self.db().card(word.id)?.is_some() {
            // Never clobber existing scheduling.
            return Ok(false);
        }

        let card = if note.reviewed && note.interval_days > 0 {
            // SM-2 → FSRS: at 90% retention, stability ≈ interval.
            let stability = f64::from(note.interval_days).max(0.1);
            Card {
                state: CardState::Review,
                stability,
                difficulty: difficulty_from_factor(note.factor),
                due: now + chrono::Duration::days(note.due_in_days.unwrap_or(0).clamp(0, 365)),
                last_review: None,
                reps: note.reps,
                lapses: note.lapses,
                step: 0,
            }
        } else {
            Card::new(now)
        };
        let status = if card.stability >= KNOWN_STABILITY_DAYS {
            KnowledgeStatus::Known
        } else {
            KnowledgeStatus::Learning
        };
        self.db().upsert_card(word.id, None, &card)?;
        self.db().set_word_status(word.id, status)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factor_difficulty_mapping_roundtrips_midpoint() {
        assert_eq!(factor_from_difficulty(5.0), 2500);
        assert!((difficulty_from_factor(2500) - 5.0).abs() < 1e-9);
        // Hard words: low ease, high difficulty.
        assert!(difficulty_from_factor(1300) > 8.0);
        assert!(factor_from_difficulty(10.0) == 1300);
        // Missing factor defaults to the midpoint.
        assert!((difficulty_from_factor(0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn apkg_export_import_roundtrip() {
        let app = App::with_db(
            shiori_db::Db::open_in_memory().unwrap(),
            std::env::temp_dir(),
        )
        .unwrap();
        // A learning word with scheduling.
        let word = app
            .ensure_word(&shiori_core::WordKey::new(
                "勉強",
                "べんきょう",
                shiori_core::PartOfSpeech::Noun,
            ))
            .unwrap();
        let card = Card {
            state: CardState::Review,
            stability: 21.0,
            difficulty: 5.0,
            due: Utc::now() + chrono::Duration::days(3),
            last_review: Some(Utc::now()),
            reps: 7,
            lapses: 1,
            step: 0,
        };
        app.db().upsert_card(word.id, None, &card).unwrap();

        let path = std::env::temp_dir().join("jrc-transfer-roundtrip.apkg");
        assert_eq!(app.export_apkg(&path).unwrap(), 1);

        // Import into a fresh database.
        let other = App::with_db(
            shiori_db::Db::open_in_memory().unwrap(),
            std::env::temp_dir(),
        )
        .unwrap();
        let (imported, skipped) = other.import_apkg(&path).unwrap();
        assert_eq!((imported, skipped), (1, 0));

        let word = other
            .db()
            .find_word(&shiori_core::WordKey::new(
                "勉強",
                "べんきょう",
                shiori_core::PartOfSpeech::Noun,
            ))
            .unwrap()
            .expect("imported word exists");
        assert_eq!(word.status, KnowledgeStatus::Learning);
        let card = other.db().card(word.id).unwrap().unwrap();
        assert!((card.card.stability - 21.0).abs() < 0.5);
        assert_eq!(card.card.reps, 7);

        // Re-import skips (card exists).
        let (imported, skipped) = other.import_apkg(&path).unwrap();
        assert_eq!((imported, skipped), (0, 1));
        std::fs::remove_file(&path).ok();
    }
}
