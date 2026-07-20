+++
title = "Reading"
weight = 2
+++

The reader presents an open document as e-reader-style pages of clickable
text in the active language — Japanese or an installed language pack — with
a dictionary panel on the right. It opens from the library or the home
page's continue-reading card; the Reader icon in the rail is enabled only
while a document is open.

## Pages and navigation

Text is paginated to fit the window — there is no in-page scrolling. To turn
pages:

- scroll the mouse wheel,
- press **PgUp** / **PgDn**, or
- click the **◀ / ▶** buttons in the bottom bar.

The bottom bar shows `page N / M` and a thin progress strip representing how
far into the book the end of the current page is. Resizing the window
repaginates the book and keeps you on the paragraph you were reading.

**ArrowRight / ArrowLeft** (rebindable in Settings → Shortcuts) move the
selection to the next or previous phrase group, following it across page
boundaries.

## Clicking words and phrase groups

Every token in the book's language is clickable. Tokens that belong together
are selected as one *phrase group*, highlighted in a single color:

- **Conjugated verbs select with their endings.** Clicking 読んでいる selects
  the whole phrase, and the panel's **Form** box explains the conjugation
  component by component (te-iru, polite past, passive, causative, …).
- **Nominal compounds** (noun + suffix, prefix + noun) are tried against the
  dictionary as a single word. When that succeeds, the compound takes the
  headline and the clicked token is shown below as its component.

Selected words are always highlighted. An optional checkbox in
Settings → Reading additionally tints every unknown word in the text.

## The dictionary panel

The right side panel shows the headword with furigana positioned over each
kanji run, the surface form from the text when it differs from the lemma,
part of speech, current knowledge status, corpus frequency rank, usage
register tags (colloquial, formal, archaic, …), and the numbered dictionary
senses — JMdict for Japanese, the pack's dictionary otherwise — with
cross-references and antonyms.

Pack languages add a few panel elements of their own:

- **Decoded parse** — in pre-annotated texts (the Koine Greek pack), an
  italic "this form: …" line decodes the clicked occurrence's stored parse
  to prose.
- **IPA** — with Settings → Reading → Pronunciation → "Show IPA with
  dictionary entries" enabled (off by default), entries from packs built
  from Wiktionary show their IPA under the headword. Japanese readings are
  unaffected.
- **Contractions** — a fused function word (French au = à + le) shows its
  components under the headword, each a clickable button that looks the
  part up in the [Dictionary-and-Kanji](@/docs/dictionary-and-kanji.md)
  view. Leaving the reader this way credits the page like a pause, same as
  the kanji chips below.
- **Compound splitting** — when a word in a Germanic pack has no entry of
  its own, the panel shows "compound of: …" with each part clickable
  (Arbeitsmaschine → arbeit + maschine): no entry for the whole word, but
  its parts are in the dictionary.
- **Ambiguous forms** — when a form has more than one possible analysis,
  an "Ambiguous form" box lists every candidate (lemma plus decoded
  parse); one click re-points that single occurrence — the manual override
  above the frequency vote.

The panel's buttons assign a knowledge status:

| Action | Shortcut | Effect |
|--------|----------|--------|
| ➕ Learn (SRS) | L | Sets the word to *learning* and creates a review card from the current sentence |
| ✔ Known | K | Sets the word to *known* |
| ↺ Forgot this | — | Shown instead of Known for known words; puts the word back into review rotation |
| 🚫 Ignore | I | Sets the word to *ignored* (names, loanwords you read for free) |
| Reset | — | Returns the word to *unknown* |

Buttons only appear when they would change something (e.g. no Ignore button
on an already-ignored word). Shortcuts are rebindable in
Settings → Shortcuts.

**Kanji chips:** under the headword, one small button per unique kanji in
the lemma. Clicking a chip opens that character's card — readings, meanings,
stroke order — in the [Dictionary-and-Kanji](@/docs/dictionary-and-kanji.md) view.
Leaving the reader this way credits the partially read page like a pause.

If an LLM backend is configured (Settings → AI), an **Explain this
sentence** button (shortcut E) asks it about the selected sentence. Without
a dictionary installed, the panel says so explicitly — distinct from "no
entry found for this word".

## Furigana and glosses

Settings → Reading → Furigana offers four modes:

| Mode | Behavior |
|------|----------|
| None | No furigana |
| Unknown words | Furigana over every word still at *unknown* status |
| Unknown words, first X instances | Furigana over the first X occurrences of each unknown word per book |
| All words | Furigana everywhere |

X is set next to the mode (1–50, default 3). The first-X mode is
**instance-anchored per book**: occurrence indices are computed in reading
order when the book opens, so the same X occurrences carry furigana no
matter how you flip around or resize the window — later occurrences never
do. X applies independently to each book.

The unknown-based modes track status live: assigning any status
(learning/known/ignored) removes a word's furigana everywhere. For
Japanese, furigana is drawn only over tokens containing kanji, and a
conjugated stem keeps just the reading of its kanji run (走っ from 走る
shows はし, not はしる).

In pack languages with pre-annotated texts (the Koine Greek pack), the
furigana slot doubles as an interlinear gloss layer: each token's
hand-verified gloss is drawn over it, governed by the same four modes and
the same live status tracking — no kanji requirement applies.

## Reading position

Your position — the first sentence of the current page — is saved on every
page flip and again when the app closes. Reopening the book returns you to
the saved page.

Reaching the **last page** saves a one-past-the-end position, which the
library reads as **100% (finished)**: it completes the progress display and
unlocks the finish sweep in the book's info panel. Opening a finished book
again lands on its last page.

## Pause and the reading clock

Reading time and velocity (characters per minute) are tracked per sitting;
see [Statistics](@/docs/statistics.md). The **⏸ button** in the bottom bar, or its
shortcut (default P, rebindable in Settings → Shortcuts), pauses the clock
and shows a "Reading paused" overlay over the dimmed page. Any click or key
press resumes — and that resuming input is swallowed, so it cannot flip a
page or trigger a shortcut.

**Auto-away:** any interaction anywhere in the reader — a click, a scroll, a
key press — resets an idle timer. When idle time on a page reaches 2× the
page's expected reading time (its character count ÷ your measured velocity;
never less than 20 seconds), the paused overlay appears with a **5-second
grace**: re-engaging within 5 seconds means it was a hard page, not an
absence — the overlay is dismissed and the clock keeps running with full
credit. Otherwise the absence is treated as real and the clock stops. Before
a velocity statistic exists, a flat 5-minute idle threshold is used instead.

What gets credited when a page visit ends:

| How the visit ended | Time credited | Characters credited |
|---------------------|---------------|---------------------|
| Page flip | Elapsed time, capped at 2× expected. Under 0.2× expected the page was flipped through, not read — nothing is credited | Full page (or none if too fast) |
| Pause (button, leaving the reader, quitting mid-page) | Elapsed time, capped at 2× expected | Proportional to elapsed ÷ expected, capped at the full page |
| Auto-away confirmed | Elapsed time, capped at 1× expected — the app cannot know when you actually left | Full page |
| No velocity stat yet | Elapsed time, capped at 5 minutes; the too-fast filter is off | Full page on a flip, none otherwise |

## Finishing a book

When a book reaches 100%, its info panel in the library offers **Mark
remaining words as known**. The sweep sets every word still at the default
*unknown* status to *known*; untouched **proper nouns become *ignored***
(names are not vocabulary). Words you explicitly marked while reading —
learning, known, or ignored — are never overwritten.

Before applying, the confirmation dialog flags **suspicious** words you
plausibly never actually learned. A word is suspicious only when both
signals fire: its corpus frequency rank is far beyond where your known
vocabulary lives (or it is absent from the frequency list entirely), *and*
it appears at most twice in the book. Suspicious words are **excluded from
the sweep by default** — a false "known" hurts more than a false "unknown" —
with per-word checkboxes to include them. The rarity signal needs at least
50 known words before it activates. See [Reviews-and-SRS](@/docs/reviews-and-srs.md)
for how newly known words affect scheduling.