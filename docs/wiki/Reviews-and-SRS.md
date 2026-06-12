# Reviews and SRS

Shiori schedules vocabulary reviews with the FSRS algorithm, and every card
shows the word in the sentence you originally found it in. Reviews exist to
support reading, not the other way around.

## The four knowledge statuses

Every word in the database has exactly one status. Statistics count *content
tokens* only — particles and other function words are excluded everywhere.

| Status | Meaning | Effect on stats |
|---|---|---|
| **Unknown** | Never studied, never marked. The default for every word. | Counts toward a book's unknown share, which determines its difficulty band (under 2% unknown = comfortable, under 5% = sweet spot, under 10% = challenging, otherwise too hard). |
| **Learning** | Has an active SRS card. | Counts as "just out of reach" — the learning share shown in coverage forecasts ("learning these 23 words → 95%"). |
| **Known** | Marked known by you, or auto-promoted out of review. | Counts toward known share and coverage. |
| **Ignored** | Deliberately excluded — names, transcription noise. | Treated as known for coverage purposes (known share = known + ignored tokens), but never offered for study and excluded from unknown counts. |

Status changes and what they do to cards:

- **Learn** (in the reader's word panel) creates a card anchored to the
  sentence you clicked it in, due immediately, and sets the word to learning.
  If a card already exists, nothing changes.
- **Mark known** and **Ignore** both delete any existing card.
- Resetting a word to unknown also deletes its card.
- **I forgot this** on a known word creates a fresh card due immediately and
  puts the word back to learning. It keeps the old context sentence unless
  you trigger it from a new one.
- The finish-the-book sweep (see [Library](Library)) bulk-promotes untouched
  unknown words to known and untouched proper nouns to ignored.

## Scheduling: FSRS-5

The scheduler is a hand-implemented FSRS-5 (the published formulas with the
population-optimized default weights `w0..w18`). Each card tracks a
*stability* (days until recall probability decays to the target retention)
and a *difficulty* (1–10), updated on every answer. Cards move through four
states: new → learning (minute-scale steps) → review (day-scale FSRS
intervals), with a lapse sending the card to relearning.

Shiori deliberately shows two answer buttons, not four:

| Button | FSRS grade |
|---|---|
| ✓ Correct | Good |
| ✗ Incorrect | Again |

Each button shows the interval the card will get if you press it. Hard and
Easy are not exposed in the UI.

Every answer is written to the review log (rating, time, resulting stability
and difficulty), which feeds the retention statistic on the
[Statistics](Statistics) page.

### Auto-promotion to known

When a card's stability reaches **60 days**, the word's status is promoted
to known. The card itself keeps being scheduled and reviewed — only the
status changes, so reading statistics stop counting the word against you.

The sharp edge: status follows stability on *every* answer. If a promoted
word lapses and its stability drops back below 60 days, the word returns to
learning until it matures again.

## What a card looks like

The front of a card is the sentence the word was mined from, with the target
word highlighted in place, framed by the sentence immediately before and
after in muted text. Only if the source document has been deleted does the
card fall back to showing the bare word.

Revealing the answer shows the reading, a short dictionary gloss, and usage
register tags (colloquial, formal, archaic, …) when the entry has them.

### Cross-book example sentences

After the answer is revealed, up to three other sentences from your library
that use the word appear under "Elsewhere in your library", with their book
titles — sentences from *other* books are listed first. This is controlled
by **Settings → Review → "Show example sentences from other books on
cards"**, on by default.

## Shortcuts

| Action | Default key |
|---|---|
| Show answer | `Space` |
| Correct | `→` (Right arrow) |
| Incorrect | `←` (Left arrow) |

All three are rebindable under **Settings → Shortcuts**. The reveal shortcut
only works before the answer is shown; the grading shortcuts only after.

## The due badge and queue

- The Review icon in the sidebar shows a small count next to it whenever
  cards are due; its tooltip reads "Review — N due". The count refreshes as
  you answer cards and mark words.
- The queue serves due cards most-overdue-first. The header above each card
  shows how many remain, plus the card's state, repetition count, and
  lapses.
- A 14-day due forecast (overdue cards grouped under today) is on the
  [Statistics](Statistics) page.

When nothing is due, the review view says so and suggests you go read —
which is, after all, the point. See [Reading](Reading) for how words get
into the queue in the first place.
