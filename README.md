# Shiori（栞）

*Shiori* — 栞, "bookmark" — is a desktop application for learning Japanese
through **comprehensible input**. The
primary activity is reading real Japanese text; every other feature exists to
support that. It is not a flashcard driller — it is a reading companion that
happens to teach.

## What it does

- **Text ingestion** — import books, articles, or any Japanese text: paste,
  `.txt`/`.md` (UTF-8 or Shift_JIS), `.html` (Aozora Bunko pages — furigana
  ruby is stripped), `.epub`, and `.pdf`. The app parses everything to the
  morpheme level while preserving sentence and paragraph context.
- **In-context learning** — click any word while reading to see its entry,
  usage register, and conjugation explained; one click adds it to reviews.
  Configurable furigana (including "first X instances per book"), a book
  info panel with coverage forecasts, and a finish-the-book sweep that
  marks untouched words known.
- **Spaced repetition** — review cards always show the word in the sentence it
  came from, scheduled with the FSRS algorithm.
- **Dictionary** — JMdict (via [jmdict-simplified](https://github.com/scriptin/jmdict-simplified))
  is downloaded automatically on first run, including register/nuance tags
  (formal, colloquial, archaic, literary, …) and cross-references.
- **Reading difficulty stats** — for every document in the library: how much
  you already know, what is just out of reach, and what to read next.
- **Conjugation-aware reading** — clicking 読んでいる selects the whole
  conjugated phrase and the panel explains the form (te-iru, polite past,
  passive, causative, …) component by component.
- **Dictionary & kanji** — search JMdict directly; kanji cards show
  readings, meanings, grades, and stroke-order diagrams (KANJIDIC2 +
  KanjiVG).
- **Online sources** — search Aozora Bunko's public-domain catalog and
  Japanese Wikisource, and import works in one click.
- **Conversation practice (optional LLM)** — chat with a native-speaker
  persona that converses rather than corrects; mistakes come back as
  paper-style underlines on your own messages. Backends: Anthropic, local
  models via Ollama, or any OpenAI-compatible server. The app is fully
  functional without an LLM.
- **Statistics that matter** — reading velocity and calendar, JLPT-graded
  comfortable reading level, review forecasts and retention.
- **Anki interop** — export your cards to .apkg (with scheduling) or import
  an existing deck; one-click database backup.

## Workspace layout

| Crate      | Concern                                                        |
|------------|----------------------------------------------------------------|
| `shiori-core` | Shared domain types and errors                                 |
| `shiori-nlp`  | Morphological analysis and sentence segmentation               |
| `shiori-srs`  | FSRS spaced-repetition scheduler                               |
| `shiori-dict` | JMdict dictionary + frequency list download/lookup             |
| `shiori-db`   | SQLite persistence                                             |
| `shiori-app`  | Application services: ingestion, mining, reviews, stats        |
| `shiori-llm`  | Pluggable LLM explanation/feedback backend                     |
| `shiori-gui`  | egui desktop interface                                         |

## Building

```sh
cargo build --release
cargo run --release -p shiori-gui
```

On first launch the app downloads its reference data (JMdict, a frequency
list, KANJIDIC2 + KanjiVG kanji data, and JLPT vocabulary lists) into its
data directory. A Japanese-capable system font is picked up automatically.

## Development

```sh
cargo test --workspace                                # full test suite
cargo clippy --workspace --all-targets -- -D warnings # lints
```

An end-to-end smoke run against real data (downloads JMdict on first use,
cached afterwards):

```sh
cargo run -p shiori-app --example smoke -- <data-dir> <utf8-text-file> "<title>"
```

The first build downloads and embeds the IPADIC morphological dictionary
(via lindera), which takes a few minutes once.

## Data sources

- [JMdict](https://www.edrdg.org/jmdict/j_jmdict.html) — © EDRDG, used under
  the [EDRDG licence](https://www.edrdg.org/edrdg/licence.html), fetched via
  the jmdict-simplified project.
- Word frequency — Leeds Internet Corpus frequency list (CC BY), with a
  built-in fallback.
