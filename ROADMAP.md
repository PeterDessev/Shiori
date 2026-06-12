# Roadmap

Plan of record for agreed features. Everything in this file is designed and
considered planned; nothing here is implemented yet unless struck through.
Deferred ideas live at the bottom.

## Removals

### Mining page
Remove the mining view, its nav icon, and `MiningState`. Its purpose is
superseded by mark-known-on-finish and the book info panel. Keep
`jrc_app::mining_candidates` (frequency-ranked unknown words per document) —
it powers the info panel's "top unknown words" list and missed-word
detection.

## Library

### Book info panel
- ⓘ button on each library row opens a right side panel for that book.
- Contents: full metadata, sentence/token counts, difficulty, coverage
  forecast ("you know 87% of tokens; learning these 23 words → 95%"), top
  unknown words by corpus frequency, reading time and velocity (once
  sessions exist), and the mark-known-on-finish button.

### Mark-known-on-finish
- When a book reaches 100% progress, a "Mark remaining words as known"
  button appears in its info panel.
- Sweep: every word in the book still at the default `unknown` status →
  `known`. Words the user explicitly set (learning/known/ignored) are
  untouched.
- Proper nouns: set to `ignored` by the sweep (not skipped — skipping
  leaves them inflating unknown counts) unless the user explicitly assigned
  them a status during reading.
- Confirmation dialog with a count preview.
- Missed-word detection: before the sweep, flag words the user plausibly
  doesn't know but forgot to mark, using signals such as: corpus frequency
  rank far rarer than the user's typical known band; very low occurrence
  count in this book (≤2); JLPT level above the user's graded level.
  Flagged words are listed in the dialog and **default to excluded** from
  the sweep (a false "known" hurts more than a false "unknown"), with
  per-word checkboxes and an "include all" override.

## Reader

### Away / pause
- A reader button plus a rebindable shortcut marks the session "away": a
  centered modal overlay ("Reading paused") over a dimmed page. No blur.
- Resume on any click or key press; the resuming input is swallowed (must
  not trigger a shortcut or page flip).
- Pausing stops the reading-session clock (see Statistics).
- Reading velocity is measured in **characters per minute**; the expected
  reading time of a page is its character count ÷ the user's velocity.
- Engagement = any interaction anywhere in the reader view — text area,
  dictionary panel, page controls. Every interaction resets the away timer.
- Auto-away: when time on the current page reaches 2× its expected reading
  time with no interaction, show the paused modal. **5-second grace**: if
  the user re-engages within 5 s of the modal appearing, it was a hard
  page, not an absence — dismiss the modal and keep the clock running with
  full credit. Otherwise the away is real: pause the clock and credit the
  page with at most 1× its expected reading time (we don't know when the
  user actually left).
- Too-fast filter: a page left in under 0.2× its expected reading time
  (e.g. a 25-char page at 50 chars/min → under ~6 s) was flipped through,
  not read — exclude it from session time and from the velocity stat.
- Until a velocity stat exists, fall back to a flat per-page timeout for
  away detection and skip the too-fast filter.

### Furigana modes
- Four modes: none / unknown words / unknown words, first X instances /
  all. X is configurable.
- Instance-anchored per book: for each word, the first X occurrences *in
  document order* carry furigana; later occurrences never do. Deterministic
  and stable across page flips and window resizes — no exposure counters or
  new tracking state; occurrence indices are computed from token order when
  the book opens.
- Anchored instances apply to `unknown`-status words only; assigning any
  other status (learning/known/ignored) removes furigana everywhere in the
  unknown-based modes.
- Per-book independence: X resets with each book.

## Statistics (kept and expanded)

- Reading velocity (characters/min) and per-book reading time. Requires a
  new `reading_sessions` table logging active reader time, cleaned by the
  away/pause rules above (away cap, 5 s grace, too-fast filter).
- Comfortable reading level: graded against community JLPT vocab lists
  (unofficial since 2010, good enough) cross-checked with frequency-band
  coverage — "highest level where known-share clears a threshold".
- Review forecast: cards due per day for the next N days (from `cards.due`).
- Learning rate: new words/day entering SRS (from card creation dates).
- True retention rate from `review_log`, compared against FSRS prediction.
- Reading calendar heatmap; known-words growth curve.

## Review

- Cross-book example sentences on cards: other sentences from the library
  containing the word (query tokens by word_id). Gated by a toggle under
  Settings → Review.

## AI

### Ollama integration
- Keep the `Explainer` trait as the seam; add an Ollama backend speaking
  its REST API at `localhost:11434`: `GET /api/tags` to list installed
  models, `POST /api/pull` (streamed progress shown in settings) to fetch
  models in-app, `POST /api/chat` for completions.
- Detect whether Ollama is installed/running; if absent, show guidance
  instead of errors.
- Advanced field: custom OpenAI-compatible endpoint (covers LM Studio,
  llama.cpp server, vLLM) — near-free byproduct of the same backend.
- No embedded inference engine, ever.

### Production chat rework
- Persistent chat: `conversations` / `messages` / `annotations` tables.
- One structured LLM call per user message returns `{reply, annotations[]}`.
- Annotations are character spans with severity + note, rendered as colored
  underlines on the user's sent messages ("written up like a paper").
- The assistant replies as a native speaker conversing — never corrects
  inline.
- Level calibration from three signals: recorded vocab stats, the
  complexity of the user's own chat writing (strongest signal, avoids the
  small-initial-vocab cold start), and an explicit challenge slider
  (match me / push me / immerse me).
- Assistant messages run through the lindera pipeline and render with the
  reader's clickable-token UX (phrase grouping, dictionary panel, Learn →
  SRS).
- Clicking a word in a *user* message shows the dictionary entry and, if an
  annotation span overlaps it, the write-up note stacked in the same right
  panel.

## Dictionary + kanji (one view)

- New nav entry with a single search box returning both word entries
  (JMdict) and kanji cards.
- Kanji data: KANJIDIC2 (on/kun readings, meanings, grade, JLPT, stroke
  count, variant/archaic cross-references) + KanjiVG (per-stroke SVG paths;
  render stroke-order diagrams, optionally animated stroke by stroke).
  Both free EDRDG-family licenses; fetched at first run like JMdict.
- Add-to-SRS from search results (cards already allow a null sentence).
- Contextual access: kanji chips under the headword in the reader's word
  panel expand into the kanji card.

## Sources (new nav view)

- Aozora Bunko: fetch the public catalog CSV (`list_person_all_extended`)
  on app start — async, cached in the data dir, offline falls back to the
  last cached copy. Manual reload button in the view. Search runs locally
  against the cache; selecting a result downloads the XHTML and feeds the
  existing import pipeline (already handles Aozora HTML + Shift_JIS).
- Wikisource-ja via the MediaWiki search API.
- NHK Easy News: deferred (no good article index).

## Settings & UX

### Shortcut recording
- Press-to-record: while capturing, track held keys; when the first key is
  released, the snapshot of what was held becomes the binding ("burned in
  on release").
- Single combos only — no chords/sequences. Requires at least one
  non-modifier key.
- Escape cancels capture and is permanently unbindable.
- Conflict detection against existing bindings at capture time.

### Theming & fonts
- Theme presets: dark, light, sepia/e-ink.
- Reader font choice: gothic (Noto Sans JP) and mincho (Noto Serif JP),
  fetched at first run into the data dir (keep the binary lean); Meiryo
  remains the system fallback. Font size and line-spacing sliders.
- Constraint: the selected Japanese font must stay at index 0 of egui's
  proportional family or CJK glyphs clip in text fields.

### Settings reorganization
- Category list panel inside the settings view: General, Appearance,
  Reading, Review, AI, Shortcuts, Data.

### Getting started
- Expanding (CollapsingHeader) sections per page with more detail; top
  level stays short.
- Document the four knowledge statuses (unknown / learning / known /
  ignored): what each means, how it affects stats, and when to use Ignore
  (names, loanwords you read for free).

## Startup & offline

### Offline-first first run
- The first-run setup screen gains a "Continue without dictionary" button;
  the download gate is no longer hard.
- Without the reference data the app runs fully: import, read, mark words,
  SRS, local LLM (Ollama). A dismissible banner notes the dictionary isn't
  installed, **with the retry button in the banner itself** (not buried in
  settings).
- The banner also carries an ⓘ info button that opens a modal spelling out
  exactly what is unavailable without the dictionary (entries, compound
  lookup, frequency ranks, usage registers) and what still works
  (status-based stats, difficulty bands, recommendations, SRS).
- Everywhere dictionary-dependent functionality breaks, say so in place:
  the word panel shows "no dictionary installed" (distinct from "no entry
  found for this word"), the dictionary view shows a no-dictionary notice
  with the same retry, etc.
- The help/getting-started page does **not** document the offline path —
  it isn't the expected execution path; the banner modal and in-place
  notices carry all the explanation.
- After first run everything is already local (JMdict + frequency live in
  the SQLite db); the only network features are LLM calls to remote
  providers and the Sources catalog fetch, which has its own cached
  fallback.

## Data

- Anki export to .apkg; import with lemma matching against the word table
  (SM-2 history seeds FSRS state approximately — set expectations in UI).
- Settings export/import as JSON.
- One-click database backup/restore.

## Deferred / TODO

- NHK Easy News source (needs a usable article index).
- TTS: v1 Windows SAPI → v2 cross-platform via the `tts` crate
  (SAPI/AVSpeech/speech-dispatcher) → v3 optional VOICEVOX integration
  (free local Japanese neural TTS with an HTTP API; same philosophy as
  Ollama).
