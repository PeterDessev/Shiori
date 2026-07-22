# Changelog

All notable changes to Shiori are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0] - 2026-07-22

### Added
- **Linux and macOS support.** Shiori now builds, is tested, and ships
  release binaries for Linux (x86_64) and macOS (Apple Silicon and Intel)
  alongside Windows. Everyday CI stays on Windows for fast feedback; every
  tagged release runs the full check suite (`cargo fmt`, `clippy`, tests)
  and builds a stripped binary archive on Windows, Linux, and macOS before
  publishing them all to one GitHub release.

## [0.3.1] - 2026-07-21

### Changed
- Koine (Ancient) Greek and Modern Greek are now fully separate languages
  for book search, sharing no sources: Koine keeps its own Project
  Gutenberg filter (`grc`) and Koine-specific Libraries directory (Perseus,
  First1KGreek, …) and no longer borrows the Modern Greek Wikisource. The
  Build-from-Wiktionary "Ancient Greek" option is renamed "Koine Greek" to
  match the corpus pack, so the language reads as Koine everywhere.

## [0.3.0] - 2026-07-21

### Added
- **Per-language book search.** The Sources view ("Find books online") is
  now language-aware: a language switcher at the top re-scopes every tab
  (and where imports land), and each language searches its own free
  libraries.
  - **Wikisource** is no longer Japanese-only — it queries the active
    language's Wikisource wiki (`<code>.wikisource.org`, resolved per
    language; Ancient Greek falls back to the Modern Greek wiki).
    Multi-part books are collapsed to a single result and imported whole
    as an EPUB via the Wikimedia export tool, rather than one entry per
    chapter.
  - **Project Gutenberg** search via the [Gutendex](https://gutendex.com/)
    API, filtered to the active language, with Gutenberg license
    boilerplate stripped on import.
  - **OPDS distributors**, added per language under the new OPDS tab and
    persisted in settings. Searches OPDS 1.x (Atom, following an
    OpenSearch description and navigation feeds) and OPDS 2.0 (JSON), and
    imports EPUB/PDF/HTML/text. Project Gutenberg and Open Library are
    offered as one-click suggestions.
  - **Libraries** tab: a browsable, bundled directory of free, legal
    digital libraries for the active language, plus multilingual
    aggregators.

## [0.2.0] - 2026-07-19

Shiori grows beyond Japanese: language support is now data-driven, with
installable language packs, a corpus-first Koine Greek pack, a
build-from-Wiktionary pipeline for ~19 modern languages, a home page,
and per-language library, dictionary, reviews, statistics, and practice.

### Added

#### Languages beyond Japanese
- **Data-driven language packs**: install a pack under Settings →
  Languages (or drop its directory into `<data>/packs/<code>/`), activate
  the language, and the whole app — library, reader, dictionary, mining,
  reviews, statistics, chat — operates in it. Nothing mixes across languages, and installing a
  second language can never wipe or collide with Japanese data (schema v8
  adds language/source scoping everywhere, with an automatic pre-migration
  backup and in-place cache migration — no re-download).
- **Settings → Languages**, every installed language in one place:
  activate a language, see what its pack ships (license, dictionary,
  morphology, frequency, graded levels, fonts), import its bundled texts
  into the library with one click, remove a pack (its library and review
  history stay in the database), and install new packs from a folder, a
  zip, or a download URL with optional SHA-256 verification — all live,
  no restart. (A hosted-catalog browser — offline-cached catalog.json
  with one-click verified installs — is fully plumbed and tested but
  not shown in the UI until a catalog is actually published;
  build-from-Wiktionary covers discovery meanwhile.)
- **Home page**: the app now opens on a home view — the active language
  with a quick switcher and a shortcut to the Languages page, cards due
  today (by your local midnight) with a time estimate from your measured
  review pace, a pick-up-where-you-left-off card (progress, estimated
  time left at your reading speed, unknown words ahead, difficulty
  verdict), and the reading-activity calendar.
- **Review and reading statistics are language-scoped**: due counts, the
  review queue, forecasts, retention, intake and matured curves, reading
  time, and reading velocity all follow the active language instead of
  mixing languages (the seconds-per-card pace estimate deliberately
  stays global). Closes the known P3 limitation.
- **Koine Greek as the first pack language**, corpus-first: pre-annotated
  texts (SIAT format, converted from MorphGNT) carry a hand-verified lemma,
  parse, and gloss on every token, so no runtime analyzer exists or is
  needed. The reader's furigana slot doubles as an interlinear gloss layer;
  the word panel decodes each occurrence's parse to prose; stats grade
  against GNT frequency tiers; dictionary search is accent-insensitive and
  accepts betacode/Greeklish.
- **Tier-1 analysis** for plain-text imports and chat in pack languages:
  tokens resolve through the pack's full-form table when unambiguous.
- **Dictionary search in pack languages resolves inflected forms**: a
  query like *suis* resolves through the grammar table to every candidate
  lemma (être and suivre), falling back to the learned suffix rules for
  forms the table doesn't list. Results build in tiers of match closeness
  — lemma-of-the-query, then exact, then prefix matches — with corpus
  frequency ordering words within each tier, so the everyday word always
  beats its rare homograph.
- **Language-aware hints and a consolidated About section**: dictionary,
  online-search, and library empty-state hints follow the active language,
  and Settings → General gathers every data attribution (JMdict, KANJIDIC2,
  KanjiVG, JLPT lists, Leeds frequency, Lindera/IPADIC, Noto fonts, Aozora,
  Wikisource, Wiktextract/FrequencyWords) plus the license line of each
  installed pack in one place.
- **Build from Wiktionary** (Settings → Languages): pick from ~19
  languages and the app downloads public upstream data — kaikki.org's
  Wiktextract dump and hermitdave's frequency list — and compiles the
  pack locally, the same first-run model as the Japanese reference
  bundle; nothing hosted, no catalog to maintain. Wiktionary's
  inflection tables become the grammar: every conjugated form resolves
  to its lemma and its parse decodes to prose in the reader, via a
  generated tag table. Built packs carry per-sense register labels
  (colloquial, archaic…) wired into the usage display, usage examples,
  and IPA pronunciation behind a default-off setting; frequency ranks
  are lemmatized (a verb's conjugations all count toward it) and
  generate Top-500/1k/2k/5k graded tiers for the statistics page;
  ambiguous forms resolve by corpus frequency when one candidate
  clearly dominates; French/Italian elision tokenizes l'eau as l' +
  eau while leaving aujourd'hui whole; and the gigabyte-class
  downloads resume with HTTP ranges instead of restarting.
- **Smarter analysis without an engine**: contractions are pack data —
  au/im/della stay one token but count as function words and expand in
  the reader (au = à + le, components clickable); Germanic packs split
  unknown compounds against their own dictionary (Arbeitsmaschine →
  arbeit + maschine, linking elements understood); forms missing from
  the grammar tables resolve through suffix rules the builder learns
  from its own data, accepted only when the dictionary confirms the
  guess; and a candidate picker in the reader lists every analysis of
  an ambiguous form — one click re-points that occurrence, the manual
  override above the frequency vote.
- **`shiori-packc`**, the CI pack compiler: builds the Greek pack from
  MorphGNT and modern-language packs from kaikki.org Wiktextract dumps
  (+ hermitdave frequency lists), with a machine-enforced gate refusing
  NonCommercial sources. Its `catalog` subcommand zips finished packs
  and emits the hosted `catalog.json` (real SHA-256s and sizes,
  validated against the app's own parser) that the browse section
  consumes.
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

[Unreleased]: https://github.com/PeterDessev/Shiori/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/PeterDessev/Shiori/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/PeterDessev/Shiori/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/PeterDessev/Shiori/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/PeterDessev/Shiori/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/PeterDessev/Shiori/releases/tag/v0.1.0
