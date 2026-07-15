//! Reading-difficulty statistics and "what should I read next?".

use shiori_core::{DocumentId, KnowledgeStatus};
use shiori_db::DocumentSummary;

use crate::{App, Result};

/// How hard a document currently is for the user, by unknown-token share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DifficultyBand {
    /// < 2% unknown tokens: smooth reading, little to learn.
    Comfortable,
    /// 2–5% unknown: comprehensible input sweet spot.
    SweetSpot,
    /// 5–10% unknown: doable with effort.
    Challenging,
    /// > 10% unknown: too far ahead for now.
    TooHard,
}

impl DifficultyBand {
    pub fn label(self) -> &'static str {
        match self {
            DifficultyBand::Comfortable => "comfortable",
            DifficultyBand::SweetSpot => "sweet spot",
            DifficultyBand::Challenging => "challenging",
            DifficultyBand::TooHard => "too hard",
        }
    }

    fn from_unknown_share(share: f64) -> Self {
        if share < 0.02 {
            DifficultyBand::Comfortable
        } else if share < 0.05 {
            DifficultyBand::SweetSpot
        } else if share < 0.10 {
            DifficultyBand::Challenging
        } else {
            DifficultyBand::TooHard
        }
    }
}

/// Comprehension statistics for one document (content words only;
/// particles and other function words are excluded).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocStats {
    pub content_tokens: u32,
    pub known_tokens: u32,
    pub learning_tokens: u32,
    pub unknown_tokens: u32,
    pub ignored_tokens: u32,
    pub distinct_unknown_words: u32,
    pub band: DifficultyBand,
}

impl DocStats {
    /// Share of content tokens the user already knows (known + ignored).
    pub fn known_share(&self) -> f64 {
        self.share(self.known_tokens + self.ignored_tokens)
    }

    /// Share of content tokens currently being learned — "just out of
    /// reach".
    pub fn learning_share(&self) -> f64 {
        self.share(self.learning_tokens)
    }

    /// Share of content tokens never studied — "too far ahead" when large.
    pub fn unknown_share(&self) -> f64 {
        self.share(self.unknown_tokens)
    }

    fn share(&self, n: u32) -> f64 {
        if self.content_tokens == 0 {
            0.0
        } else {
            f64::from(n) / f64::from(self.content_tokens)
        }
    }
}

/// A document proposed as the next thing to read.
#[derive(Debug)]
pub struct Recommendation {
    pub summary: DocumentSummary,
    pub stats: DocStats,
    /// Lower = better next read.
    pub score: f64,
}

/// Everything the statistics page shows beyond per-document difficulty.
#[derive(Debug, Default)]
pub struct StatsOverview {
    /// Characters per minute, when enough history exists.
    pub velocity_cpm: Option<f64>,
    pub total_reading_seconds: f64,
    pub total_reading_chars: u64,
    /// Credited reading seconds per day (for the calendar heatmap).
    pub reading_by_day: Vec<(String, f64)>,
    /// Cards becoming due per day, next two weeks (overdue under today).
    pub due_forecast: Vec<(String, u32)>,
    /// New words entering the SRS per day, averaged over the last 30.
    pub learning_rate_30d: f64,
    /// Share of correct reviews over the last 30 days.
    pub retention_30d: Option<f64>,
    /// Cumulative count of words whose card stability matured past the
    /// known threshold, per day.
    pub matured_by_day: Vec<(String, u32)>,
    /// Known share per graded-vocabulary level, easiest first (JLPT for
    /// Japanese, GNT frequency tiers for Koine Greek…).
    pub levels: Vec<shiori_db::GradedShare>,
    /// Display name of the level scheme ("JLPT", "GNT tier").
    pub level_scheme: String,
    /// "Comfortable reading level" derived from the level shares.
    pub comfortable_level: Option<String>,
    /// (rank bound, known words within it) coverage of the corpus.
    pub rank_bands: Vec<(u32, u32)>,
}

impl App {
    /// Aggregate the expanded statistics page in one call.
    pub fn stats_overview(&self) -> Result<StatsOverview> {
        let totals = self.db().reading_totals()?;
        let velocity_cpm = self.reading_velocity_cps()?.map(|cps| cps * 60.0);

        let starts = self.db().learning_starts_by_day()?;
        let today = chrono::Utc::now().date_naive();
        let recent: u32 = starts
            .iter()
            .filter(|(day, _)| {
                chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
                    .map(|d| (today - d).num_days() < 30)
                    .unwrap_or(false)
            })
            .map(|(_, n)| n)
            .sum();

        let (correct, total) = self.db().retention_counts(30)?;
        let (levels, level_scheme) = match self.service().graded_scheme() {
            Some((key, display)) => (
                self.db().graded_known_shares(self.active_lang(), &key)?,
                display,
            ),
            None => (Vec::new(), String::new()),
        };
        // Comfortable level: hardest level where this and every easier
        // level is at least half known. levels are sorted easiest-first.
        let mut comfortable_level = None;
        for share in &levels {
            if share.total > 0 && f64::from(share.known) / f64::from(share.total) >= 0.5 {
                comfortable_level = Some(format!("around {level_scheme} {}", share.label));
            } else {
                break;
            }
        }

        Ok(StatsOverview {
            velocity_cpm,
            total_reading_seconds: totals.seconds,
            total_reading_chars: totals.chars,
            reading_by_day: self.db().reading_seconds_by_day()?,
            due_forecast: self.db().due_forecast(14)?,
            learning_rate_30d: f64::from(recent) / 30.0,
            retention_30d: (total > 0).then(|| f64::from(correct) / f64::from(total)),
            matured_by_day: self.db().matured_by_day(60.0)?,
            levels,
            level_scheme,
            comfortable_level,
            rank_bands: self
                .db()
                .known_in_rank_bands(self.active_lang(), &[1000, 2000, 5000, 10000])?,
        })
    }
}

impl App {
    pub fn document_stats(&self, document: DocumentId) -> Result<DocStats> {
        let counts = self.db.document_status_counts(document)?;
        let mut stats = DocStats {
            content_tokens: 0,
            known_tokens: 0,
            learning_tokens: 0,
            unknown_tokens: 0,
            ignored_tokens: 0,
            distinct_unknown_words: 0,
            band: DifficultyBand::Comfortable,
        };
        for c in counts {
            stats.content_tokens += c.tokens;
            match c.status {
                KnowledgeStatus::Known => stats.known_tokens += c.tokens,
                KnowledgeStatus::Learning => stats.learning_tokens += c.tokens,
                KnowledgeStatus::Ignored => stats.ignored_tokens += c.tokens,
                KnowledgeStatus::Unknown => {
                    stats.unknown_tokens += c.tokens;
                    stats.distinct_unknown_words += c.words;
                }
            }
        }
        stats.band = DifficultyBand::from_unknown_share(stats.unknown_share());
        Ok(stats)
    }

    /// All documents ranked as next reads: closest to the comprehensible-
    /// input sweet spot first.
    ///
    /// The score is the distance of the unknown-token share from an ideal
    /// 3.5%, with a penalty above the ideal (frustration costs more than
    /// boredom).
    pub fn recommendations(&self) -> Result<Vec<Recommendation>> {
        let mut recs = Vec::new();
        for summary in self.db.list_documents()? {
            let stats = self.document_stats(summary.document.id)?;
            let share = stats.unknown_share();
            const IDEAL: f64 = 0.035;
            let score = if share >= IDEAL {
                (share - IDEAL) * 2.0
            } else {
                IDEAL - share
            };
            recs.push(Recommendation {
                summary,
                stats,
                score,
            });
        }
        recs.sort_by(|a, b| a.score.total_cmp(&b.score));
        Ok(recs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difficulty_bands() {
        assert_eq!(
            DifficultyBand::from_unknown_share(0.0),
            DifficultyBand::Comfortable
        );
        assert_eq!(
            DifficultyBand::from_unknown_share(0.03),
            DifficultyBand::SweetSpot
        );
        assert_eq!(
            DifficultyBand::from_unknown_share(0.07),
            DifficultyBand::Challenging
        );
        assert_eq!(
            DifficultyBand::from_unknown_share(0.5),
            DifficultyBand::TooHard
        );
    }

    #[test]
    fn shares_handle_empty_documents() {
        let stats = DocStats {
            content_tokens: 0,
            known_tokens: 0,
            learning_tokens: 0,
            unknown_tokens: 0,
            ignored_tokens: 0,
            distinct_unknown_words: 0,
            band: DifficultyBand::Comfortable,
        };
        assert_eq!(stats.known_share(), 0.0);
        assert_eq!(stats.unknown_share(), 0.0);
    }
}
