# Changelog

All notable changes to Shiori are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### Languages beyond Japanese
- **Data-driven language packs**: drop a pack directory into
  `<data>/packs/<code>/`, pick the language under Settings → General, and the
  whole app — library, reader, dictionary, mining, reviews, statistics,
  chat — operates in it. Nothing mixes across languages, and installing a
  second language can never wipe or collide with Japanese data (schema v8
  adds language/source scoping everywhere, with an automatic pre-migration
  backup and in-place cache migration — no re-download).
- **Koine Greek as the first pack language**, corpus-first: pre-annotated
  texts (SIAT format, converted from MorphGNT) carry a hand-verified lemma,
  parse, and gloss on every token, so no runtime analyzer exists or is
  needed. The reader's furigana slot doubles as an interlinear gloss layer;
  the word panel decodes each occurrence's parse to prose; stats grade
  against GNT frequency tiers; dictionary search is accent-insensitive and
  accepts betacode/Greeklish.
- **Tier-1 analysis** for plain-text imports and chat in pack languages:
  tokens resolve through the pack's full-form table when unambiguous.
- **`shiori-packc`**, the CI pack compiler: builds the Greek pack from
  MorphGNT and modern-language packs from kaikki.org Wiktextract dumps
  (+ hermitdave frequency lists), with a machine-enforced gate refusing
  NonCommercial sources.
- **Per-language production practice**: pack-defined personas (dead
  languages disclose the synthetic persona and judge against attested
  usage), composition exercises, translation drills over sentences from
  your own reading, and per-language LLM model overrides.
- The `LanguageService` abstraction (new `shiori-lang` crate) with Japanese
  as the first implementation; the embedded IPADIC moved behind a default-on
  cargo feature; a golden token fixture proves Japanese analysis is
  bit-identical across the refactor.

## [0.1.0] - 2026-06-16

The first release of Shiori — a desktop Japanese reading companion built
around comprehensible input.

### Added

#### Reader
- A paged reader that imports `.txt`, `.md`, `.html` (Aozora), `.epub`, and
  `.pdf`, in UTF-8 or Shift_JIS, by file dialog or drag-and-drop.
- Furigana modes with **per-book instance anchoring** — readings show only over
  unknown words and, in the strictest mode, only over the first few occurrences
  of each word per book, pinned to those exact spots.
- Unknown-word tinting, with one click selecting a whole conjugated phrase
  (読んでいる, not 読) and explaining the inflection component by component.
- Reading sessions with **away detection**: pages flipped through far faster
  than expected don't count, the clock pauses when you wander off (with a
  short grace period), and reading velocity in characters per minute feeds the
  statistics.
- Resume-where-you-left-off, accurate page counts, and reading position
  persisted across exit.
- Ruby-markup segmentation for per-character furigana.
- Tutor sentence explanations render as **Markdown**, wrapping to the side
  panel with a magnifier button that opens the write-up in a centered modal
  spanning the content area without disturbing the page beneath.

#### Dictionary & kanji
- A dictionary view: search JMdict by kanji, kana, or any word form, with a
  kanji card (readings, meanings, school grade, KanjiVG stroke order) for
  every kanji in the query.
- Search resolves **conjugations and rōmaji** — type a verb in any inflected
  form, or type in Latin letters, and it transliterates to kana and looks up
  the dictionary form.
- Dictionary entries enriched with **part-of-speech labels, JLPT level, and
  example sentences**; JMdict part-of-speech codes are expanded to readable
  labels.
- **Animated stroke order**: each stroke traces in order and loops, and
  scrolling over a character scrubs it stroke-by-stroke (snapping to whole
  strokes, one per wheel notch) and briefly pauses auto-play.
- Word cards highlight the looked-up word inside its example sentences, and a
  🔎 button opens a **word-detail modal** — the full entry, every example, and
  a kanji card per character — dismissed by click-away, Escape, or ✕.
- Add any search hit straight to spaced repetition.

#### Reviews & spaced repetition
- FSRS-5 spaced-repetition scheduler with configurable learning steps.
- Review cards show each word inside the sentence you found it in, framed by
  its neighbours, plus **cross-book example sentences** drawn from your other
  books.
- Four knowledge statuses — unknown / learning / known / ignored.
- **Mark-known-on-finish**: finishing a book promotes still-unknown words to
  known (proper nouns become ignored), with rare or out-of-band words flagged
  for confirmation first.
- Mark-forgotten flow returns known words to SRS rotation.

#### Conversation practice & LLM
- A pluggable explainer/chat layer with an **Anthropic** backend and an
  offline fallback.
- **Ollama** backend and support for any custom OpenAI-compatible endpoint.
- Production chat with a native-speaker persona that converses without
  interrupting, marking mistakes up paper-style (grammar vs. phrasing) with the
  explanation one hover away; every word in the chat is clickable.

#### Online sources
- A sources view to search **Aozora Bunko** and **Japanese Wikisource** and
  import works straight into the library; the Aozora catalogue is cached after
  first download so later searches run locally.

#### Library & statistics
- A book info side panel with per-book progress, known-word share, and a
  difficulty verdict.
- Statistics expansion: reading velocity and calendar, a comfortable-level
  grade against JLPT vocabulary lists, review forecasts, true retention, and
  per-book difficulty.

#### Data & interop
- **Anki export/import** — export cards with scheduling, or import a deck
  (SM-2 state seeds FSRS).
- One-click SQLite database backup and restore, and settings export/import as a
  single JSON file.
- Editable document metadata with auto-extraction, and archival copies of
  imported books.

#### Interface
- egui desktop app with sidebar navigation, a sortable/table library, themes,
  and onboarding.
- Dark / light / **sepia** themes, selectable gothic or mincho Japanese fonts,
  and adjustable reader typography sliders.
- **Press-to-record shortcuts** with modifier combos.
- A getting-started guide covering every feature with fold-out detail.
- Application icon featuring the 栞 kanji.

#### NLP
- Morphological analysis pipeline built on lindera/IPADIC.
- Phrase grouping and inflection analysis; prefixes bound to following nouns;
  a lexical part-of-speech class.

#### Project
- Cargo workspace scaffolded crate-per-concern (`core`, `nlp`, `srs`, `dict`,
  `db`, `app`, `llm`, `gui`).
- CI workflows for lint, test, and release; MIT and Apache-2.0 license texts;
  contributing guide, user-guide wiki, feature roadmap, and project website.

### Changed
- **Rebranded from jrc to Shiori** — crates and binary renamed, with a
  data-directory migration.
- Offline-first startup: the app launches and works without the dictionary
  downloaded, fetching reference data on first run.
- Start at a larger default zoom, with reset returning to it.
- Removed the standalone mining page (mining now flows through the reader).

### Fixed
- Panel scrolling no longer flips reader pages.
- Chat tokens no longer render reversed in user bubbles.
- The setup screen now lists all reference-data downloads.
- Library progress refreshes on navigation and reading position persists on
  exit.
- Japanese font baseline alignment and a crisp Latin font fallback.

[Unreleased]: https://github.com/PeterDessev/Shiori/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/PeterDessev/Shiori/releases/tag/v0.1.0
