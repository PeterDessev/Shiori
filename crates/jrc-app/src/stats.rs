//! Reading-difficulty statistics and "what should I read next?".

use jrc_core::{DocumentId, KnowledgeStatus};
use jrc_db::DocumentSummary;

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
