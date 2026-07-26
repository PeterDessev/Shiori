//! Review cards and ratings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The four FSRS review grades.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

impl Rating {
    /// Grade as the integer used in the FSRS formulas (1–4).
    pub fn grade(self) -> f64 {
        self as i32 as f64
    }

    pub fn from_i64_lossy(v: i64) -> Self {
        match v {
            1 => Rating::Again,
            2 => Rating::Hard,
            4 => Rating::Easy,
            _ => Rating::Good,
        }
    }
}

/// Lifecycle state of a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardState {
    /// Never reviewed.
    New,
    /// In initial learning steps (minutes-scale intervals).
    Learning,
    /// Graduated; scheduled by the FSRS memory model (days-scale).
    Review,
    /// Lapsed from review; in relearning steps.
    Relearning,
}

impl CardState {
    pub fn as_str(self) -> &'static str {
        match self {
            CardState::New => "new",
            CardState::Learning => "learning",
            CardState::Review => "review",
            CardState::Relearning => "relearning",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "learning" => CardState::Learning,
            "review" => CardState::Review,
            "relearning" => CardState::Relearning,
            _ => CardState::New,
        }
    }
}

/// A spaced-repetition card.
///
/// The card is pure scheduling state; what is being reviewed (word, sentence
/// context) is associated elsewhere.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub state: CardState,
    /// FSRS stability in days. Meaningless while `state == New`.
    pub stability: f64,
    /// FSRS difficulty, clamped to [1, 10]. Meaningless while `state == New`.
    pub difficulty: f64,
    /// When the card should next be shown.
    pub due: DateTime<Utc>,
    pub last_review: Option<DateTime<Utc>>,
    /// Total number of reviews.
    pub reps: u32,
    /// Number of times the card lapsed (Again while in review).
    pub lapses: u32,
    /// Current position within learning/relearning steps.
    pub step: u32,
}

impl Card {
    /// A brand-new card, due immediately.
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            state: CardState::New,
            stability: 0.0,
            difficulty: 0.0,
            due: now,
            last_review: None,
            reps: 0,
            lapses: 0,
            step: 0,
        }
    }

    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        self.due <= now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rating_grades() {
        assert_eq!(Rating::Again.grade(), 1.0);
        assert_eq!(Rating::Easy.grade(), 4.0);
        assert_eq!(Rating::from_i64_lossy(2), Rating::Hard);
        assert_eq!(Rating::from_i64_lossy(99), Rating::Good);
    }

    #[test]
    fn card_state_roundtrip() {
        for s in [
            CardState::New,
            CardState::Learning,
            CardState::Review,
            CardState::Relearning,
        ] {
            assert_eq!(CardState::from_str_lossy(s.as_str()), s);
        }
    }

    #[test]
    fn new_card_is_due_immediately() {
        let now = Utc::now();
        let card = Card::new(now);
        assert!(card.is_due(now));
        assert_eq!(card.state, CardState::New);
        assert_eq!(card.reps, 0);
    }
}
