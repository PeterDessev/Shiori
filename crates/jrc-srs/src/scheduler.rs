//! The FSRS-5 scheduler.
//!
//! Formula reference: <https://github.com/open-spaced-repetition/fsrs4anki/wiki/The-Algorithm>
//! (FSRS-5 revision). Parameter names `w0..w18` follow the paper/wiki.

use chrono::{DateTime, Duration, Utc};

use crate::card::{Card, CardState, Rating};

/// Power-law forgetting curve constants chosen so that
/// `R(t = S) = 0.9` exactly: R(t, S) = (1 + FACTOR·t/S)^DECAY.
const DECAY: f64 = -0.5;
const FACTOR: f64 = 19.0 / 81.0;

const MIN_STABILITY: f64 = 0.01;
const MIN_DIFFICULTY: f64 = 1.0;
const MAX_DIFFICULTY: f64 = 10.0;

/// Default FSRS-5 parameters (the published population-optimized weights).
pub const DEFAULT_PARAMETERS: [f64; 19] = [
    0.40255, 1.18385, 3.173, 15.69105, 7.1949, 0.5345, 1.4604, 0.0046, 1.54575, 0.1192, 1.01925,
    1.9395, 0.11, 0.29605, 2.2698, 0.2315, 2.9898, 0.51655, 0.6621,
];

/// Scheduler tuning knobs.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// FSRS weights `w0..w18`.
    pub parameters: [f64; 19],
    /// Probability of recall the scheduler aims for at review time.
    pub desired_retention: f64,
    /// Hard cap on intervals, in days.
    pub maximum_interval_days: f64,
    /// Intra-day steps a new card passes through before graduating.
    pub learning_steps_minutes: Vec<i64>,
    /// Steps a lapsed card passes through before returning to review.
    pub relearning_steps_minutes: Vec<i64>,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            parameters: DEFAULT_PARAMETERS,
            desired_retention: 0.9,
            maximum_interval_days: 36500.0,
            learning_steps_minutes: vec![1, 10],
            relearning_steps_minutes: vec![10],
        }
    }
}

/// Pure FSRS-5 scheduler. All methods take and return plain data.
#[derive(Debug, Clone, Default)]
pub struct Scheduler {
    config: SchedulerConfig,
}

impl Scheduler {
    pub fn new(config: SchedulerConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &SchedulerConfig {
        &self.config
    }

    fn w(&self, i: usize) -> f64 {
        self.config.parameters[i]
    }

    /// Probability that `card` is still recalled at `now`.
    ///
    /// New cards and cards without a previous review are defined to be at
    /// retrievability 1.0.
    pub fn retrievability(&self, card: &Card, now: DateTime<Utc>) -> f64 {
        match (card.state, card.last_review) {
            (CardState::New, _) | (_, None) => 1.0,
            (_, Some(last)) => {
                let elapsed = elapsed_days(last, now);
                retrievability(elapsed, card.stability.max(MIN_STABILITY))
            }
        }
    }

    /// Apply a review at `now` and return the updated card.
    pub fn review(&self, card: &Card, rating: Rating, now: DateTime<Utc>) -> Card {
        let mut next = card.clone();
        next.reps += 1;

        match card.state {
            CardState::New => {
                next.stability = self.initial_stability(rating);
                next.difficulty = self.initial_difficulty(rating);
                match rating {
                    Rating::Easy => self.graduate(&mut next, now),
                    Rating::Good => {
                        self.advance_learning(&mut next, 1, now);
                    }
                    Rating::Again | Rating::Hard => {
                        self.advance_learning(&mut next, 0, now);
                    }
                }
            }
            CardState::Learning | CardState::Relearning => {
                // Same-day (short-term) stability update.
                next.stability = self.short_term_stability(card.stability, rating);
                next.difficulty = self.next_difficulty(card.difficulty, rating);
                match rating {
                    Rating::Again => self.advance_learning(&mut next, 0, now),
                    Rating::Hard => self.advance_learning(&mut next, card.step, now),
                    Rating::Good => self.advance_learning(&mut next, card.step + 1, now),
                    Rating::Easy => self.graduate(&mut next, now),
                }
            }
            CardState::Review => {
                let elapsed = card
                    .last_review
                    .map(|last| elapsed_days(last, now))
                    .unwrap_or(0.0);
                let r = retrievability(elapsed, card.stability.max(MIN_STABILITY));
                next.difficulty = self.next_difficulty(card.difficulty, rating);
                if rating == Rating::Again {
                    next.lapses += 1;
                    next.stability = self.forget_stability(card.stability, card.difficulty, r);
                    next.state = CardState::Relearning;
                    self.advance_learning(&mut next, 0, now);
                } else {
                    next.stability =
                        self.recall_stability(card.stability, card.difficulty, r, rating);
                    self.schedule_review(&mut next, now);
                }
            }
        }

        next.last_review = Some(now);
        next
    }

    /// The interval (in days) the scheduler would assign a review-state card
    /// with the given stability.
    pub fn next_interval_days(&self, stability: f64) -> f64 {
        let r = self.config.desired_retention.clamp(0.7, 0.99);
        let days = stability / FACTOR * (r.powf(1.0 / DECAY) - 1.0);
        days.clamp(1.0, self.config.maximum_interval_days)
    }

    // ---- FSRS-5 formulas -------------------------------------------------

    /// S0(G) = w_{G-1}
    fn initial_stability(&self, rating: Rating) -> f64 {
        self.w(rating as usize - 1).max(MIN_STABILITY)
    }

    /// D0(G) = w4 − e^{w5·(G−1)} + 1
    fn initial_difficulty(&self, rating: Rating) -> f64 {
        let g = rating.grade();
        (self.w(4) - (self.w(5) * (g - 1.0)).exp() + 1.0).clamp(MIN_DIFFICULTY, MAX_DIFFICULTY)
    }

    /// Linear-damped difficulty step with mean reversion toward D0(Easy).
    fn next_difficulty(&self, difficulty: f64, rating: Rating) -> f64 {
        let delta = -self.w(6) * (rating.grade() - 3.0);
        let damped = difficulty + delta * (MAX_DIFFICULTY - difficulty) / 9.0;
        let easy_d0 = self.w(4) - (self.w(5) * 3.0).exp() + 1.0;
        (self.w(7) * easy_d0 + (1.0 - self.w(7)) * damped).clamp(MIN_DIFFICULTY, MAX_DIFFICULTY)
    }

    /// Stability after a successful review (Hard/Good/Easy).
    fn recall_stability(&self, s: f64, d: f64, r: f64, rating: Rating) -> f64 {
        let hard_penalty = if rating == Rating::Hard { self.w(15) } else { 1.0 };
        let easy_bonus = if rating == Rating::Easy { self.w(16) } else { 1.0 };
        let s = s.max(MIN_STABILITY);
        let growth = self.w(8).exp()
            * (11.0 - d)
            * s.powf(-self.w(9))
            * ((self.w(10) * (1.0 - r)).exp() - 1.0)
            * hard_penalty
            * easy_bonus;
        (s * (growth + 1.0)).max(MIN_STABILITY)
    }

    /// Stability after forgetting (Again in review). Never exceeds the
    /// pre-lapse stability.
    fn forget_stability(&self, s: f64, d: f64, r: f64) -> f64 {
        let s = s.max(MIN_STABILITY);
        let s_f = self.w(11)
            * d.powf(-self.w(12))
            * ((s + 1.0).powf(self.w(13)) - 1.0)
            * (self.w(14) * (1.0 - r)).exp();
        s_f.clamp(MIN_STABILITY, s)
    }

    /// Same-day review stability update: S' = S·e^{w17·(G−3+w18)}.
    fn short_term_stability(&self, s: f64, rating: Rating) -> f64 {
        let s = s.max(MIN_STABILITY);
        (s * (self.w(17) * (rating.grade() - 3.0 + self.w(18))).exp()).max(MIN_STABILITY)
    }

    // ---- state transitions ----------------------------------------------

    /// Move a learning/relearning card to `step`, graduating if past the end.
    fn advance_learning(&self, card: &mut Card, step: u32, now: DateTime<Utc>) {
        let steps = match card.state {
            CardState::Relearning => &self.config.relearning_steps_minutes,
            _ => &self.config.learning_steps_minutes,
        };
        match steps.get(step as usize) {
            Some(&minutes) => {
                if card.state == CardState::New {
                    card.state = CardState::Learning;
                }
                card.step = step;
                card.due = now + Duration::minutes(minutes);
            }
            None => self.graduate(card, now),
        }
    }

    /// Promote to review state and schedule by stability.
    fn graduate(&self, card: &mut Card, now: DateTime<Utc>) {
        card.state = CardState::Review;
        card.step = 0;
        self.schedule_review(card, now);
    }

    fn schedule_review(&self, card: &mut Card, now: DateTime<Utc>) {
        let days = self.next_interval_days(card.stability);
        card.due = now + Duration::seconds((days * 86400.0).round() as i64);
    }
}

fn elapsed_days(from: DateTime<Utc>, to: DateTime<Utc>) -> f64 {
    ((to - from).num_seconds().max(0) as f64) / 86400.0
}

/// R(t, S) = (1 + FACTOR·t/S)^DECAY
fn retrievability(elapsed_days: f64, stability: f64) -> f64 {
    (1.0 + FACTOR * elapsed_days / stability).powf(DECAY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 9, 0, 0).unwrap()
    }

    fn days(n: f64) -> Duration {
        Duration::seconds((n * 86400.0) as i64)
    }

    fn sched() -> Scheduler {
        Scheduler::default()
    }

    /// Walk a card to review state by answering Good through the steps.
    fn graduated_card(s: &Scheduler) -> (Card, DateTime<Utc>) {
        let mut now = t0();
        let mut card = Card::new(now);
        loop {
            card = s.review(&card, Rating::Good, now);
            if card.state == CardState::Review {
                return (card, now);
            }
            now = card.due;
        }
    }

    #[test]
    fn interval_equals_stability_at_default_retention() {
        // With the chosen FACTOR/DECAY, R(S, S) = 0.9, so the next interval
        // at desired_retention 0.9 must equal the stability itself.
        let s = sched();
        for stability in [1.0, 5.0, 42.0, 300.0] {
            assert!((s.next_interval_days(stability) - stability).abs() < 1e-9);
        }
    }

    #[test]
    fn retrievability_decays_from_one() {
        assert!((retrievability(0.0, 5.0) - 1.0).abs() < 1e-12);
        assert!((retrievability(5.0, 5.0) - 0.9).abs() < 1e-12);
        let r1 = retrievability(1.0, 5.0);
        let r10 = retrievability(10.0, 5.0);
        assert!(r1 > r10);
        assert!(r10 > 0.0);
    }

    #[test]
    fn initial_difficulty_is_monotone_in_grade() {
        let s = sched();
        let d: Vec<f64> = [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy]
            .iter()
            .map(|&r| s.initial_difficulty(r))
            .collect();
        assert!(d[0] > d[1] && d[1] > d[2] && d[2] > d[3]);
        // Published FSRS-5 defaults: D0(Good) ≈ 5.28.
        assert!((d[2] - 5.282).abs() < 0.01);
        for v in d {
            assert!((MIN_DIFFICULTY..=MAX_DIFFICULTY).contains(&v));
        }
    }

    #[test]
    fn initial_stability_uses_first_four_weights() {
        let s = sched();
        assert_eq!(s.initial_stability(Rating::Again), DEFAULT_PARAMETERS[0]);
        assert_eq!(s.initial_stability(Rating::Easy), DEFAULT_PARAMETERS[3]);
    }

    #[test]
    fn new_card_good_enters_second_learning_step() {
        let s = sched();
        let card = s.review(&Card::new(t0()), Rating::Good, t0());
        assert_eq!(card.state, CardState::Learning);
        assert_eq!(card.step, 1);
        assert_eq!(card.due, t0() + Duration::minutes(10));
        assert_eq!(card.reps, 1);
    }

    #[test]
    fn new_card_again_enters_first_learning_step() {
        let s = sched();
        let card = s.review(&Card::new(t0()), Rating::Again, t0());
        assert_eq!(card.state, CardState::Learning);
        assert_eq!(card.step, 0);
        assert_eq!(card.due, t0() + Duration::minutes(1));
    }

    #[test]
    fn new_card_easy_graduates_immediately() {
        let s = sched();
        let card = s.review(&Card::new(t0()), Rating::Easy, t0());
        assert_eq!(card.state, CardState::Review);
        // Due ≈ now + S0(Easy) days.
        let expected = t0() + days(DEFAULT_PARAMETERS[3]);
        let diff = (card.due - expected).num_seconds().abs();
        assert!(diff < 60, "due {} vs expected {}", card.due, expected);
    }

    #[test]
    fn learning_good_through_steps_graduates() {
        let s = sched();
        let (card, _) = graduated_card(&s);
        assert_eq!(card.state, CardState::Review);
        assert_eq!(card.reps, 2); // two Good answers with default [1, 10] steps
        assert!(card.stability > 0.0);
    }

    #[test]
    fn review_good_grows_stability_and_interval() {
        let s = sched();
        let (card, graduated_at) = graduated_card(&s);
        let review_at = graduated_at + days(card.stability.max(1.0));
        let after = s.review(&card, Rating::Good, review_at);
        assert_eq!(after.state, CardState::Review);
        assert!(
            after.stability > card.stability,
            "stability should grow: {} -> {}",
            card.stability,
            after.stability
        );
        assert!(after.due > review_at + days(after.stability * 0.9));
    }

    #[test]
    fn review_again_lapses_into_relearning() {
        let s = sched();
        let (card, graduated_at) = graduated_card(&s);
        let review_at = graduated_at + days(card.stability.max(1.0));
        let after = s.review(&card, Rating::Again, review_at);
        assert_eq!(after.state, CardState::Relearning);
        assert_eq!(after.lapses, 1);
        assert!(
            after.stability < card.stability,
            "lapse must shrink stability"
        );
        assert_eq!(after.due, review_at + Duration::minutes(10));

        // Relearning Good with default single step graduates back to review.
        let back = s.review(&after, Rating::Good, after.due);
        assert_eq!(back.state, CardState::Review);
    }

    #[test]
    fn harder_ratings_give_shorter_intervals() {
        let s = sched();
        let (card, graduated_at) = graduated_card(&s);
        let review_at = graduated_at + days(card.stability.max(1.0));
        let hard = s.review(&card, Rating::Hard, review_at);
        let good = s.review(&card, Rating::Good, review_at);
        let easy = s.review(&card, Rating::Easy, review_at);
        assert!(hard.stability < good.stability);
        assert!(good.stability < easy.stability);
        assert!(hard.due < good.due);
        assert!(good.due < easy.due);
    }

    #[test]
    fn difficulty_moves_with_ratings_and_stays_clamped() {
        let s = sched();
        let (mut card, mut now) = graduated_card(&s);
        let d_start = card.difficulty;

        // Repeated Again pushes difficulty up but never above 10.
        for _ in 0..30 {
            now = card.due.max(now + days(1.0));
            card = s.review(&card, Rating::Again, now);
            // Climb out of relearning so the next Again is a review lapse.
            now = card.due;
            card = s.review(&card, Rating::Good, now);
        }
        assert!(card.difficulty > d_start);
        assert!(card.difficulty <= MAX_DIFFICULTY);

        // Repeated Easy pulls difficulty down but never below 1.
        for _ in 0..50 {
            now = card.due;
            card = s.review(&card, Rating::Easy, now);
        }
        assert!(card.difficulty >= MIN_DIFFICULTY);
        assert!(card.difficulty < d_start);
    }

    #[test]
    fn forget_stability_never_exceeds_previous() {
        let s = sched();
        for (st, d, r) in [(0.5, 3.0, 0.95), (10.0, 5.0, 0.8), (200.0, 9.0, 0.4)] {
            let f = s.forget_stability(st, d, r);
            assert!(f <= st);
            assert!(f >= MIN_STABILITY);
        }
    }

    #[test]
    fn maximum_interval_is_enforced() {
        let mut config = SchedulerConfig::default();
        config.maximum_interval_days = 30.0;
        let s = Scheduler::new(config);
        assert_eq!(s.next_interval_days(10_000.0), 30.0);
    }

    #[test]
    fn long_simulation_stays_finite_and_sane() {
        let s = sched();
        let (mut card, mut now) = graduated_card(&s);
        // A deterministic mix of ratings over many reviews.
        let ratings = [
            Rating::Good,
            Rating::Good,
            Rating::Hard,
            Rating::Good,
            Rating::Easy,
            Rating::Again,
            Rating::Good,
        ];
        for (i, _) in (0..200).enumerate() {
            let rating = ratings[i % ratings.len()];
            now = card.due.max(now);
            card = s.review(&card, rating, now);
            assert!(card.stability.is_finite() && card.stability > 0.0);
            assert!((MIN_DIFFICULTY..=MAX_DIFFICULTY).contains(&card.difficulty));
            assert!(card.due > now - Duration::seconds(1));
        }
        assert!(card.reps >= 200);
    }

    #[test]
    fn retrievability_of_new_card_is_one() {
        let s = sched();
        let card = Card::new(t0());
        assert_eq!(s.retrievability(&card, t0() + days(100.0)), 1.0);
    }

    #[test]
    fn card_serde_roundtrip() {
        let s = sched();
        let (card, _) = graduated_card(&s);
        let json = serde_json::to_string(&card).unwrap();
        let back: Card = serde_json::from_str(&json).unwrap();
        assert_eq!(card, back);
    }
}
