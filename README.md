<div align="center">

<img src="assets/icon/shiori-128.png" alt="Shiori" width="96">

# Shiori（栞）

**Learn Japanese by actually reading Japanese.**

*Shiori — 栞, "bookmark" — is a desktop reading companion built around
comprehensible input: the primary activity is reading real Japanese text,
and every other feature exists to support that.*

[![CI](https://github.com/PeterDessev/Shiori/actions/workflows/ci.yml/badge.svg)](https://github.com/PeterDessev/Shiori/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Platform: Windows x86_64](https://img.shields.io/badge/platform-Windows%20x86__64-0078D6.svg)](#getting-started)

<img src="assets/screenshots/reader-demo.gif" alt="Reading in Shiori: furigana over unknown words, one click for the dictionary panel, one keypress to start learning a word" width="850">

</div>

---

Import any book. Read it. Click the words you don't know — Shiori shows the
dictionary entry, the usage register, the conjugation explained piece by
piece — and one click later the word is in spaced repetition, anchored to
the exact sentence you found it in. The app tracks every word you've ever
met, grades each book in your library against what you know, and tells you
what to read next.

No accounts, no subscription, no cloud. One executable and a folder of
SQLite.

## The interesting parts

### A reader that knows what you don't

Furigana appears only over words you haven't learned — and in its
strictest mode, only over the **first few occurrences of each word per
book**, anchored to those exact spots: scaffolding that fades as you read
deeper. Unknown words get a subtle tint. Clicking a conjugated verb selects
the whole phrase (読んでいる, not 読) and explains the form component by
component.

<img src="assets/screenshots/reader.png" alt="The reader with instance-anchored furigana, unknown-word tinting, and the dictionary panel explaining そっと" width="850">

The reading clock is honest: pages you flip through in under a fifth of the
expected time don't count, the app pauses itself when you wander off (with
a five-second grace period for genuinely hard pages), and your reading
velocity in characters per minute feeds everything from away detection to
the statistics page.

### Conversation practice that doesn't interrupt

Chat with a native-speaker persona that **converses with you — it never
corrects you mid-conversation**. Instead, your messages come back marked up
like a paper: red underlines for grammar errors, orange for phrasing a
native wouldn't use, with the explanation one hover away. Every word in the
chat is clickable, just like the reader.

<img src="assets/screenshots/chat.png" alt="Production chat: the partner converses while mistakes get paper-style underlines; clicking 面白いでした shows the dictionary entry and the write-up note together" width="850">

Bring your own brain: Anthropic's API, **any local model through Ollama**
(pull models from inside the app; nothing leaves your machine), or any
OpenAI-compatible endpoint. A challenge dial sets whether the partner
matches your level, pushes slightly above it, or goes full native.

### A dictionary with stroke order built in

Search JMdict by kanji, kana, or any word form. Every kanji in your query
gets a card: readings, meanings, school grade, and a numbered stroke-order
diagram drawn from KanjiVG data. Add any hit straight to spaced repetition
from the search results.

<img src="assets/screenshots/dictionary.png" alt="Dictionary view: word entries with prefix matches, and the 食 kanji card with a numbered stroke-order diagram" width="850">

### Books from the internet, one click away

Search Aozora Bunko's 17,000+ public-domain works and Japanese Wikisource,
then import straight into your library — Shift_JIS, ruby markup and all.
Aozora's catalog is downloaded once and cached, so every search after that
runs locally and instantly.

<img src="assets/screenshots/sources.png" alt="Sources view searching the Aozora Bunko catalog" width="850">

<details>
<summary><b>More screenshots</b> — library with per-book analytics, statistics that change behavior</summary>

#### Library
Every book shows your progress, known-word share, and difficulty verdict.
The info panel adds a coverage forecast ("learning the top 20 unknown words
lifts coverage from 87% to 95%"), your reading time, and the most useful
unknown words. Finish a book and one click promotes every word still marked
unknown to **known** (proper nouns become **ignored**) — with rare or
out-of-band words flagged for you to confirm before the sweep, so a stray
"known" never inflates your stats.

<img src="assets/screenshots/library.png" alt="Library with the book info panel: coverage forecast, reading time, most useful unknown words" width="850">

#### Statistics that change behavior

Reading velocity and a reading calendar, a comfortable-reading-level grade
against JLPT vocabulary lists, review forecasts, true retention, and
per-book difficulty — the numbers that actually tell you what to do next.

<img src="assets/screenshots/stats.png" alt="Statistics: JLPT level grading, review forecast, reading calendar" width="850">

</details>

## Everything else

- **FSRS spaced repetition** — cards always show the word inside the
  sentence you found it in, framed by its neighbors, plus example sentences
  from your other books.
- **Anki interop** — export your cards with scheduling, or import an
  existing deck (SM-2 state seeds FSRS).
- **Four knowledge statuses** — unknown / learning / known / ignored, so
  names and noise never pollute your stats.
- **Press-to-record shortcuts** with modifier combos, dark/light/sepia
  themes, gothic or mincho Japanese fonts, adjustable reader typography.
- **Offline-first** — after the first-run data download everything except
  LLM calls and online search works without a network. Your data is one
  SQLite file with one-click backup and restore, and your settings export
  to a single JSON.
- **Import anything** — `.txt`, `.md`, `.html` (Aozora), `.epub`, `.pdf`,
  UTF-8 or Shift_JIS, by file dialog or drag-and-drop.

## Getting started

**Download** — grab the latest `shiori-*-windows-x86_64.zip` from 
[Releases](https://github.com/PeterDessev/Shiori/releases), unzip, run
`shiori.exe`. On first launch the app downloads its reference data (JMdict,
frequency list, kanji data with stroke order, JLPT lists — ~20 MB total)
and you're reading.

Shiori is built and released for **Windows x86_64 only** — that's the only
target the CI tests and ships. The source has no hard OS lock, so building
on macOS or Linux may well work, but it's untested and unsupported for now.

**Build from source**:

```sh
cargo build --release -p shiori-gui   # first build embeds the IPADIC
./target/release/shiori               # dictionary and needs network, once
```

Building needs a recent stable Rust toolchain (CI builds on `stable`). The
first build downloads and embeds the IPADIC morphological dictionary, so it
needs network access once and takes a few minutes.

**Learn more** — the [user guide](docs/wiki/Home.md) covers every feature:
[Getting Started](docs/wiki/Getting-Started.md) ·
[Reading](docs/wiki/Reading.md) ·
[Reviews & SRS](docs/wiki/Reviews-and-SRS.md) ·
[Dictionary & Kanji](docs/wiki/Dictionary-and-Kanji.md) ·
[Online Sources](docs/wiki/Online-Sources.md) ·
[AI & Chat](docs/wiki/AI-and-Chat.md) ·
[Statistics](docs/wiki/Statistics.md) ·
[Data & Interop](docs/wiki/Data-and-Interop.md) ·
[Architecture](docs/wiki/Architecture.md)

## Roadmap & non-goals

The [roadmap](ROADMAP.md) is a plan of record: as of mid-2026 everything in
it ships except two deferred items — an **NHK Easy News** source (waiting on
a usable article index) and **text-to-speech** (planned in stages, ending
in optional local [VOICEVOX](https://voicevox.hiroshiba.jp/)). One hard
non-goal: **no embedded LLM inference engine, ever** — local AI is delegated
to Ollama or any OpenAI-compatible server you point Shiori at.

## Troubleshooting & getting help

Found a bug or have a question? Open an issue on the
[tracker](https://github.com/PeterDessev/Shiori/issues). A couple of known rough
edges worth knowing up front:

- KanjiVG stroke-order data covers only about half of the kanji in
  KANJIDIC2, so rarer characters fall back to a plain large glyph with no
  numbered diagram.
- Conversation practice and online search are the only features that reach
  the network after first run; everything else works fully offline.

## Contributing

Contributions are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers
development setup, the test suite, and the commit conventions (atomic
conventional commits). The codebase is a Cargo workspace split by concern:

| Crate         | Concern                                                  |
|---------------|----------------------------------------------------------|
| `shiori-core` | Shared domain types and errors                           |
| `shiori-nlp`  | Morphological analysis and sentence segmentation         |
| `shiori-srs`  | FSRS spaced-repetition scheduler                         |
| `shiori-dict` | JMdict, KANJIDIC2/KanjiVG, JLPT, frequency data          |
| `shiori-db`   | SQLite persistence, Anki .apkg read/write                |
| `shiori-app`  | Application services: ingestion, reviews, stats, sources |
| `shiori-llm`  | LLM backends: Anthropic, Ollama, OpenAI-compatible       |
| `shiori-gui`  | egui desktop interface                                   |

## Data sources

Shiori ships no dictionary data; it downloads everything on first run:

- [JMdict](https://www.edrdg.org/jmdict/j_jmdict.html) and
  [KANJIDIC2](https://www.edrdg.org/wiki/index.php/KANJIDIC_Project) —
  © the [EDRDG](https://www.edrdg.org/), used under the
  [EDRDG licence](https://www.edrdg.org/edrdg/licence.html) (CC BY-SA);
  JMdict fetched via
  [jmdict-simplified](https://github.com/scriptin/jmdict-simplified).
- [KanjiVG](https://kanjivg.tagaini.net/) stroke-order data — © Ulrich
  Apel, CC BY-SA 3.0.
- JLPT vocabulary lists —
  [stephenmk/yomitan-jlpt-vocab](https://github.com/stephenmk/yomitan-jlpt-vocab)
  (CC BY-SA 4.0, over Jonathan Waller's CC BY data).
- Word frequency — Leeds Internet Corpus derived list (CC BY).
- Books — [Aozora Bunko](https://www.aozora.gr.jp/) (public domain) and
  [Japanese Wikisource](https://ja.wikisource.org/).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at
your option.
