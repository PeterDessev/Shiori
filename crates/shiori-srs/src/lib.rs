//! Spaced-repetition scheduling with the FSRS-5 algorithm.
//!
//! Implements the Free Spaced Repetition Scheduler memory model:
//! each card carries a *stability* (days until retrievability falls to 90%)
//! and a *difficulty* (1–10). Reviews update both via the published FSRS-5
//! formulas; short initial "learning steps" are handled Anki-style before a
//! card enters long-term review.
//!
//! The scheduler is deterministic (no interval fuzz) and pure: it never
//! touches a clock or a database, which keeps it trivially testable.

mod card;
mod scheduler;

pub use card::{Card, CardState, Rating};
pub use scheduler::{Scheduler, SchedulerConfig, DEFAULT_PARAMETERS};
