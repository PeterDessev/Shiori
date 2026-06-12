//! Reading-clock bookkeeping for the open document: page timing, manual
//! and automatic away, and the crediting rules that keep the velocity
//! statistic honest.

use std::time::Instant;

use crate::app::{ReaderState, ShioriGui};

/// Flat away threshold (and credit cap) before a velocity stat exists.
pub const FLAT_AWAY_SECS: f64 = 300.0;
/// Seconds the user has to re-engage after the auto-away modal appears
/// before the absence is treated as real.
pub const GRACE_SECS: f64 = 5.0;
/// Never auto-away faster than this, however short the page.
const MIN_AWAY_SECS: f64 = 20.0;

/// Why a page visit ended — the crediting rules differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitEnd {
    /// The user moved to another page: the page was read in full.
    Flip,
    /// The user paused, left the reader, or quit mid-page.
    Pause,
    /// Auto-away confirmed: we don't know when they actually left.
    AutoAway,
}

/// Pause state of the reading clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Away {
    /// The user pressed the pause button or shortcut.
    Manual,
    /// The auto-away modal just appeared; re-engaging within the grace
    /// window means it was a hard page, not an absence.
    Grace { shown: Instant },
    /// A real absence; the clock is stopped.
    Auto,
}

/// Per-sitting reading clock for the open document.
pub struct SessionTracker {
    /// Session row in the database, created lazily on the first credit.
    pub row: Option<i64>,
    /// Characters per second, when enough history exists to trust it.
    pub velocity_cps: Option<f64>,
    /// When the current page became visible; `None` while paused.
    pub page_entered: Option<Instant>,
    pub last_interaction: Instant,
    pub away: Option<Away>,
}

impl SessionTracker {
    pub fn new(velocity_cps: Option<f64>) -> Self {
        let now = Instant::now();
        Self {
            row: None,
            velocity_cps,
            page_entered: Some(now),
            last_interaction: now,
            away: None,
        }
    }

    /// How long this page should take to read, if we can estimate it.
    pub fn expected_secs(&self, page_chars: u32) -> Option<f64> {
        self.velocity_cps
            .filter(|cps| *cps > 0.0)
            .map(|cps| page_chars as f64 / cps)
    }

    /// Idle time after which the away modal appears.
    pub fn away_threshold(&self, page_chars: u32) -> f64 {
        self.expected_secs(page_chars)
            .map(|e| 2.0 * e)
            .unwrap_or(FLAT_AWAY_SECS)
            .max(MIN_AWAY_SECS)
    }

    /// (seconds, chars) to credit for a finished page visit.
    ///
    /// - Flip with a velocity stat: pages left in under 0.2× the expected
    ///   time were flipped through, not read — credit nothing. Otherwise
    ///   credit elapsed time capped at 2× expected.
    /// - Pause mid-page: credit the time, and chars proportional to how
    ///   much of the expected read elapsed.
    /// - Auto-away: we watched 2× the expected time pass without knowing
    ///   when the user left — credit at most one expected read.
    /// - No velocity stat yet: credit time up to the flat cap; chars only
    ///   for completed (flipped) pages.
    pub fn credit(&self, elapsed: f64, page_chars: u32, end: VisitEnd) -> (f64, u64) {
        match (self.expected_secs(page_chars), end) {
            (Some(exp), VisitEnd::Flip) => {
                if elapsed < 0.2 * exp {
                    (0.0, 0)
                } else {
                    (elapsed.min(2.0 * exp), page_chars as u64)
                }
            }
            (Some(exp), VisitEnd::Pause) => {
                let secs = elapsed.min(2.0 * exp);
                let frac = if exp > 0.0 {
                    (secs / exp).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                (secs, (page_chars as f64 * frac) as u64)
            }
            (Some(exp), VisitEnd::AutoAway) => (elapsed.min(exp), page_chars as u64),
            (None, VisitEnd::Flip) => (elapsed.min(FLAT_AWAY_SECS), page_chars as u64),
            (None, _) => (elapsed.min(FLAT_AWAY_SECS), 0),
        }
    }
}

/// Characters on the reader's current page (0 before pagination).
pub fn current_page_chars(reader: &ReaderState) -> u32 {
    if reader.page_starts.is_empty() {
        return 0;
    }
    let page = reader.current_page.min(reader.page_count() - 1);
    let begin = reader.page_starts.get(page).copied().unwrap_or(0);
    let end = reader
        .page_starts
        .get(page + 1)
        .copied()
        .unwrap_or(reader.para_ranges.len())
        .min(reader.para_ranges.len());
    if begin >= end {
        return 0;
    }
    reader.para_ranges[begin..end]
        .iter()
        .flat_map(|&(s0, s1)| reader.sentences[s0..s1].iter())
        .map(|v| v.sentence.text.chars().count() as u32)
        .sum()
}

impl ShioriGui {
    /// End the current page visit, credit it, and stop the clock.
    /// Idempotent: a second call before `enter_page` is a no-op.
    pub fn end_page_visit(&mut self, end: VisitEnd) {
        let Some(reader) = self.reader.as_ref() else {
            return;
        };
        let Some(entered) = reader.session.page_entered else {
            return;
        };
        let page_chars = current_page_chars(reader);
        let elapsed = entered.elapsed().as_secs_f64();
        let (secs, chars) = reader.session.credit(elapsed, page_chars, end);
        let doc = reader.doc.id;
        let row = reader.session.row;

        if let Some(reader) = self.reader.as_mut() {
            reader.session.page_entered = None;
        }
        if secs <= 0.0 {
            return;
        }
        let row = match row {
            Some(row) => Some(row),
            None => self.with_app(|app| app.start_reading_session(doc)),
        };
        let Some(row) = row else { return };
        self.with_app(|app| app.add_reading_time(row, secs, chars));
        if let Some(reader) = self.reader.as_mut() {
            reader.session.row = Some(row);
        }
    }

    /// (Re)start the page clock — after a flip, a resume, or returning to
    /// the reader view. Clears any away state.
    pub fn enter_page(&mut self) {
        if let Some(reader) = self.reader.as_mut() {
            let now = Instant::now();
            reader.session.page_entered = Some(now);
            reader.session.last_interaction = now;
            reader.session.away = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker(cps: Option<f64>) -> SessionTracker {
        SessionTracker::new(cps)
    }

    #[test]
    fn flip_credit_rules() {
        // 1 cps, 100-char page → expected 100s.
        let t = tracker(Some(1.0));
        // Too fast: under 0.2× expected.
        assert_eq!(t.credit(15.0, 100, VisitEnd::Flip), (0.0, 0));
        // Normal read.
        assert_eq!(t.credit(90.0, 100, VisitEnd::Flip), (90.0, 100));
        // Slow read capped at 2×.
        assert_eq!(t.credit(500.0, 100, VisitEnd::Flip), (200.0, 100));
    }

    #[test]
    fn pause_credits_proportional_chars() {
        let t = tracker(Some(1.0));
        let (secs, chars) = t.credit(50.0, 100, VisitEnd::Pause);
        assert_eq!(secs, 50.0);
        assert_eq!(chars, 50);
        // Pausing after a long stall: time capped, chars capped at page.
        let (secs, chars) = t.credit(500.0, 100, VisitEnd::Pause);
        assert_eq!(secs, 200.0);
        assert_eq!(chars, 100);
    }

    #[test]
    fn auto_away_credits_one_expected_read() {
        let t = tracker(Some(1.0));
        // Triggered at 2× expected + grace.
        let (secs, chars) = t.credit(205.0, 100, VisitEnd::AutoAway);
        assert_eq!(secs, 100.0);
        assert_eq!(chars, 100);
    }

    #[test]
    fn cold_start_uses_flat_cap_and_skips_too_fast_filter() {
        let t = tracker(None);
        // Fast flip still counts (no expected to compare against).
        assert_eq!(t.credit(3.0, 100, VisitEnd::Flip), (3.0, 100));
        // Flat cap.
        assert_eq!(t.credit(900.0, 100, VisitEnd::Flip), (FLAT_AWAY_SECS, 100));
        // Pauses credit time but no chars without a basis.
        assert_eq!(t.credit(60.0, 100, VisitEnd::Pause), (60.0, 0));
    }

    #[test]
    fn away_threshold_scales_with_page() {
        let t = tracker(Some(2.0)); // 2 cps
                                    // 300-char page → expected 150s → threshold 300s.
        assert_eq!(t.away_threshold(300), 300.0);
        // Tiny page clamps to the minimum.
        assert_eq!(t.away_threshold(10), MIN_AWAY_SECS);
        // No velocity → flat.
        assert_eq!(tracker(None).away_threshold(300), FLAT_AWAY_SECS);
    }
}
