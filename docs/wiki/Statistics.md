# Statistics

The Statistics view (sidebar) summarizes your vocabulary, your graded reading level, the health of your reviews, your reading time, and how hard each book in your library currently is. Everything on the page is computed live from the database.

## Vocabulary

A count of distinct words by knowledge status:

| Row | Meaning |
|---|---|
| Known | Words you marked known (or that a finish-the-book sweep marked known) |
| Learning | Words currently in the SRS |
| Seen but unknown | Words that appeared in imported text but were never studied |
| Ignored | Words you told the app not to count (names, loanwords) |

## Comfortable reading level

Shiori grades you against community JLPT vocabulary lists (the JLPT has published no official lists since 2010; the community lists are good enough for grading). The lists are downloaded with the other reference data on first run.

For each level N5–N1 the page shows a progress bar: how many of that level's words you have marked **known** (kanji-form entries match your words by lemma; kana-only entries match on the kana lemma), out of the level's total.

Your **comfortable reading level** is the hardest level where that level *and every easier level* is at least 50% known. The check walks from N5 upward and stops at the first level below the threshold, so a strong N2 share cannot compensate for a weak N4 share. If even N5 is below 50%, the page shows "not enough known vocabulary yet" instead of a level.

As a cross-check, the **corpus coverage** line reports how many of the most frequent words in the frequency corpus you know, in rank bands: top 1k, 2k, 5k, and 10k, each shown as a percentage of the band size.

## Reviews

| Stat | Computation |
|---|---|
| Active cards | Number of cards in the SRS |
| Due now | Cards whose due time has passed |
| Reviews today / all time | Counts from the review log |
| Retention (30 days) | Share of reviews in the last 30 days rated Good or better (FSRS rating ≥ 3). Shown as — until you have reviews in the window. |
| New words/day (30 days) | Words whose *first-ever* review fell in the last 30 days, divided by 30 |

### Due forecast

A bar per day for the next 14 days showing how many cards become due that day. Overdue cards are not dropped — they are counted under today, so the first bar is "everything you would clear by reviewing now".

## Reading

Reading time comes from the reading-session clock, which runs while a book is open in the reader and is cleaned by the away rules: pausing stops the clock, an auto-away page is credited at most 1× its expected reading time, and pages flipped through too fast are excluded entirely. See [Reading](Reading) for the exact rules. Until you have recorded any time, this section shows a note that the clock runs while a book is open.

The summary line shows:

- **Total time** — sum of credited session seconds.
- **Characters** — total characters credited alongside that time.
- **Velocity (chars/min)** — total credited characters ÷ total credited time. This appears only after **10 minutes** (600 seconds) of credited reading; before that the line says velocity appears after ~10 minutes. Because the too-fast filter removes skimmed pages from both the time and the character totals, the velocity reflects pages you actually read.

### Reading calendar

A GitHub-style heatmap of the last ~18 weeks, one cell per day, colored by credited reading minutes that day. Color saturates at 60 minutes; days with no reading stay faint.

### Words matured

A running count of words whose card **stability** (the FSRS memory-strength estimate, in days) crossed 60 for the first time — the point where the scheduler considers the word effectively known. Each word is counted once, on the day of its first review at or above that stability.

## Reading difficulty

A table with one row per document in your library. All percentages are over **content words only** — particles, auxiliary verbs, numbers, symbols, prefixes, suffixes, and dependent nouns are excluded, since you read them for free.

| Column | Meaning |
|---|---|
| Known | Share of content tokens you know — known **plus ignored** tokens |
| Learning | Share in the SRS ("just out of reach") |
| Unknown | Share never studied |
| Verdict | Difficulty band from the unknown share |

The bands:

| Band | Unknown share |
|---|---|
| comfortable | under 2% — smooth reading, little to learn |
| sweet spot | 2–5% — the comprehensible-input sweet spot |
| challenging | 5–10% — doable with effort |
| too hard | over 10% — too far ahead for now |

The same numbers drive the "what to read next" ranking in the library: documents are scored by distance of their unknown share from an ideal 3.5%, with being *above* the ideal penalized twice as heavily as being below it — frustration costs more than boredom.
