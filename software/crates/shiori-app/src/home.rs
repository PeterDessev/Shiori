//! Home-screen aggregates: today's review load with a pace estimate,
//! and the "pick up where you left off" card.

use chrono::{Duration, Utc};
use shiori_core::KnowledgeStatus;
use shiori_db::{DocumentSummary, ReadingTotals};

use crate::{App, DocStats, Result};

/// Inter-review gaps above this many seconds are away-time, not card
/// time, and are excluded from the pace estimate.
const MAX_PACE_GAP_SECONDS: f64 = 60.0;

/// A pace estimate needs at least this many usable gaps.
const MIN_PACE_SAMPLES: usize = 5;

/// How many recent reviews feed the pace estimate.
const PACE_WINDOW: u32 = 200;

/// The book to pick back up, with everything the home page says about it.
#[derive(Debug)]
pub struct ContinueReading {
    pub summary: DocumentSummary,
    /// Whole-document difficulty (the same numbers the library shows).
    pub stats: DocStats,
    /// Share of sentences behind the reading position, 0..=1.
    pub progress: f64,
    /// Credited reading time in this document so far.
    pub reading: ReadingTotals,
    /// Characters of text from the reading position to the end.
    pub remaining_chars: u64,
    /// Distinct never-studied content words in the remaining text.
    pub remaining_unknown_words: u32,
    /// Time to finish at the user's measured velocity, when known.
    pub est_remaining_seconds: Option<f64>,
}

impl App {
    /// The active language's cards due by the end of the user's *local*
    /// day — what "due today" means on the home page; includes overdue
    /// cards.
    pub fn due_today(&self) -> Result<u64> {
        let tomorrow = chrono::Local::now().date_naive() + Duration::days(1);
        let end_of_day = tomorrow
            .and_hms_opt(0, 0, 0)
            .expect("midnight is a valid time")
            .and_local_timezone(chrono::Local)
            .earliest()
            .map(|t| t.with_timezone(&Utc))
            // A timezone without a local midnight tomorrow (DST edge):
            // fall back to 24h from now.
            .unwrap_or_else(|| Utc::now() + Duration::days(1));
        Ok(self.db().due_count(self.active_lang(), end_of_day)?)
    }

    /// Median seconds per card, measured from the gaps between
    /// consecutive reviews in the log. `None` until enough reviews have
    /// been done back-to-back to make the number meaningful.
    pub fn review_pace_seconds(&self) -> Result<Option<f64>> {
        let times = self.db().recent_review_times(PACE_WINDOW)?;
        let mut gaps: Vec<f64> = times
            .windows(2)
            .map(|w| (w[0] - w[1]).num_milliseconds() as f64 / 1000.0)
            .filter(|s| *s > 0.0 && *s <= MAX_PACE_GAP_SECONDS)
            .collect();
        if gaps.len() < MIN_PACE_SAMPLES {
            return Ok(None);
        }
        gaps.sort_by(f64::total_cmp);
        Ok(Some(gaps[gaps.len() / 2]))
    }

    /// The book to pick back up in the active language: the most
    /// recently read unfinished document, falling back to any unfinished
    /// one with a saved position. `None` when nothing is in progress.
    pub fn continue_reading(&self) -> Result<Option<ContinueReading>> {
        let mut docs = self.db().list_documents()?;
        docs.retain(|d| d.document.lang == self.active_lang());
        let unfinished = |d: &&DocumentSummary| {
            d.sentence_count > 0 && d.document.last_sentence < d.sentence_count
        };

        let recent = self.db().recently_read_documents(self.active_lang(), 20)?;
        let summary = recent
            .iter()
            .find_map(|(id, _)| {
                docs.iter()
                    .filter(unfinished)
                    .find(|d| d.document.id == *id)
            })
            .or_else(|| {
                // Never clocked (imported before session tracking, or read
                // elsewhere): any book with a saved position still counts.
                docs.iter()
                    .filter(unfinished)
                    .find(|d| d.document.last_sentence > 0)
            });
        let Some(summary) = summary.cloned() else {
            return Ok(None);
        };

        let id = summary.document.id;
        let from = summary.document.last_sentence;
        let stats = self.document_stats(id)?;
        let reading = self.db().document_reading_totals(id)?;
        let remaining_chars = self.db().remaining_chars(id, from)?;
        let remaining_unknown_words = self
            .db()
            .document_status_counts_from(id, from)?
            .iter()
            .filter(|c| c.status == KnowledgeStatus::Unknown)
            .map(|c| c.words)
            .sum();
        let est_remaining_seconds = self
            .reading_velocity_cps()?
            .filter(|v| *v > 0.0)
            .map(|v| remaining_chars as f64 / v);

        Ok(Some(ContinueReading {
            progress: f64::from(from) / f64::from(summary.sentence_count.max(1)),
            summary,
            stats,
            reading,
            remaining_chars,
            remaining_unknown_words,
            est_remaining_seconds,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiori_srs::{Card, Rating};

    fn app() -> App {
        App::with_db(
            shiori_db::Db::open_in_memory().unwrap(),
            std::env::temp_dir(),
        )
        .unwrap()
    }

    fn import_two_sentence_doc(app: &App, title: &str, hash: &str) -> shiori_core::DocumentId {
        let sentences = vec![
            shiori_db::NewSentence {
                paragraph: 0,
                text: "猫が好きだ。".into(),
                tokens: vec![],
            },
            shiori_db::NewSentence {
                paragraph: 1,
                text: "その猫は走る。".into(),
                tokens: vec![],
            },
        ];
        app.db()
            .import_document(
                "ja",
                &shiori_core::DocumentMeta::titled(title),
                hash,
                Utc::now(),
                &sentences,
            )
            .unwrap()
    }

    #[test]
    fn due_today_counts_cards_due_later_today() {
        let app = app();
        let word = app
            .db()
            .ensure_word(
                "ja",
                &shiori_core::WordKey::new("猫", "ねこ", shiori_core::PartOfSpeech::Noun),
            )
            .unwrap();

        // A card due right now.
        app.db()
            .upsert_card(word.id, None, &Card::new(Utc::now()))
            .unwrap();
        assert_eq!(app.due_today().unwrap(), 1);

        // Pushed 3 days out it no longer counts.
        let mut card = Card::new(Utc::now());
        card.due = Utc::now() + Duration::days(3);
        app.db().upsert_card(word.id, None, &card).unwrap();
        assert_eq!(app.due_today().unwrap(), 0);
    }

    #[test]
    fn review_pace_ignores_breaks_and_needs_samples() {
        let app = app();
        let word = app
            .db()
            .ensure_word(
                "ja",
                &shiori_core::WordKey::new("猫", "ねこ", shiori_core::PartOfSpeech::Noun),
            )
            .unwrap();
        assert_eq!(app.review_pace_seconds().unwrap(), None);

        // Ten reviews 8 seconds apart, with an hour-long break in the
        // middle that must not skew the estimate.
        let mut at = Utc::now() - Duration::hours(2);
        for i in 0..10 {
            if i == 5 {
                at += Duration::hours(1);
            }
            app.db()
                .log_review(word.id, Rating::Good, at, 3.0, 5.0)
                .unwrap();
            at += Duration::seconds(8);
        }
        let pace = app.review_pace_seconds().unwrap().unwrap();
        assert!((pace - 8.0).abs() < 0.5, "pace {pace} should be ~8s");
    }

    #[test]
    fn review_pace_needs_enough_samples() {
        let app = app();
        let word = app
            .db()
            .ensure_word(
                "ja",
                &shiori_core::WordKey::new("猫", "ねこ", shiori_core::PartOfSpeech::Noun),
            )
            .unwrap();
        let mut at = Utc::now();
        for _ in 0..3 {
            app.db()
                .log_review(word.id, Rating::Good, at, 3.0, 5.0)
                .unwrap();
            at += Duration::seconds(8);
        }
        assert_eq!(app.review_pace_seconds().unwrap(), None);
    }

    #[test]
    fn continue_reading_picks_the_last_read_unfinished_book() {
        let app = app();
        assert!(app.continue_reading().unwrap().is_none());

        let older = import_two_sentence_doc(&app, "older", "h1");
        let newer = import_two_sentence_doc(&app, "newer", "h2");
        let finished = import_two_sentence_doc(&app, "done", "h3");

        // No positions, no sessions: nothing in progress yet.
        assert!(app.continue_reading().unwrap().is_none());

        // A saved position alone (no clocked session) is enough.
        app.db().set_reading_position(older, 1).unwrap();
        let cont = app.continue_reading().unwrap().unwrap();
        assert_eq!(cont.summary.document.id, older);
        assert!((cont.progress - 0.5).abs() < 1e-9);
        assert_eq!(cont.remaining_chars, 7);

        // Reading sessions rank by recency; the finished book is skipped
        // even when read most recently.
        app.db().set_reading_position(finished, 2).unwrap();
        let now = Utc::now();
        let s = app
            .db()
            .start_reading_session(newer, now - Duration::hours(1))
            .unwrap();
        app.db()
            .add_reading_time(s, 60.0, 100, now - Duration::hours(1))
            .unwrap();
        app.db().start_reading_session(finished, now).unwrap();

        let cont = app.continue_reading().unwrap().unwrap();
        assert_eq!(cont.summary.document.id, newer);
        assert_eq!(cont.reading.seconds, 60.0);
        // Not enough total reading for a velocity, so no time estimate.
        assert_eq!(cont.est_remaining_seconds, None);
    }
}
