+++
title = "Documentation"
sort_by = "weight"
template = "section.html"
page_template = "page.html"
weight = 0
+++

Shiori（栞, "bookmark"）is a Windows-first desktop application for learning
languages through reading — built Japanese-first, with installable language
packs since 0.2.0: a corpus-first Koine Greek pack, and around 19 more
languages built locally from Wiktionary. These pages document what the app
does and how to use it.

## What Shiori is

Shiori is built around **comprehensible input**: the primary activity is reading
real text in the language you're learning, and every other feature exists to
support that. It is not a flashcard driller — it is a reading companion that
happens to teach.

You import books, articles, or any text (paste, `.txt`/`.md`, `.html`
including Aozora Bunko pages, `.epub`, `.pdf`) — pack languages can also
import their bundled texts in one click from Settings → Languages — and the
app analyzes everything, down to the morpheme for Japanese and through
full-form tables and grammar rules for pack languages, while preserving
sentence and paragraph context. While reading
you click any word to see its dictionary entry, usage register, and a
component-by-component explanation of its conjugated form; one click adds it to
spaced-repetition review. The app tracks what you know, grades every document in
your library by difficulty, and tells you what to read next. It opens on a
home page: the active language with a quick switcher, cards due today with a
time estimate, a continue-reading card, and the reading calendar. Everything
is scoped to the active language — library, dictionary, reviews, statistics,
and practice all follow it; nothing mixes across languages.

Shiori runs fully offline after first launch. The only network features are
optional LLM calls for conversation practice, the online catalog fetch in
Sources (which falls back to its local cache), and the language-pack downloads
under Settings → Languages — installing a pack from a URL and Build from
Wiktionary.

## Where to start

New here? Begin with [Getting Started](@/docs/getting-started.md), then
[Reading](@/docs/reading.md) — the heart of the app. The sidebar lists every
topic:

- [Getting Started](@/docs/getting-started.md) — first launch, reference-data download, importing your first text, the four word statuses
- [Reading](@/docs/reading.md) — clickable tokens, conjugation-aware selection, furigana modes, the reading clock
- [Reviews & SRS](@/docs/reviews-and-srs.md) — FSRS scheduling, in-context cards, cross-book examples, the mark-known-on-finish sweep
- [Dictionary & Kanji](@/docs/dictionary-and-kanji.md) — JMdict search, kanji cards with readings, grade, and stroke order
- [Online Sources](@/docs/online-sources.md) — Aozora Bunko and Japanese Wikisource search and one-click import
- [AI & Chat](@/docs/ai-and-chat.md) — conversation practice, annotation underlines, level calibration, LLM backends
- [Languages](@/docs/languages.md) — installing and activating packs, Koine Greek, Build from Wiktionary, per-language data
- [Statistics](@/docs/statistics.md) — reading velocity and calendar, comfortable reading level, forecasts, retention
- [Data & Interop](@/docs/data-and-interop.md) — Anki export/import, settings transfer, database backup and restore
- [Architecture](@/docs/architecture.md) — workspace crates, the data directory, the NLP pipeline
