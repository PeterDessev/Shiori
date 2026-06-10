# Japanese Reading Companion

A desktop application for learning Japanese through **comprehensible input**. The
primary activity is reading real Japanese text; every other feature exists to
support that. It is not a flashcard driller — it is a reading companion that
happens to teach.

## What it does

- **Text ingestion** — import books, articles, or any Japanese text. The app
  parses it to the morpheme level while preserving sentence and paragraph
  context.
- **Vocabulary mining** — unknown words are identified, looked up in JMdict,
  and ranked by how useful they are to learn (corpus frequency × document
  frequency).
- **Spaced repetition** — review cards always show the word in the sentence it
  came from, scheduled with the FSRS algorithm.
- **Dictionary** — JMdict (via [jmdict-simplified](https://github.com/scriptin/jmdict-simplified))
  is downloaded automatically on first run, including register/nuance tags
  (formal, colloquial, archaic, literary, …) and cross-references.
- **Reading difficulty stats** — for every document in the library: how much
  you already know, what is just out of reach, and what to read next.
- **LLM explanations (optional)** — connect an LLM backend to explain *why* a
  sentence is constructed the way it is, and to get naturalness feedback on
  your own writing (production mode). The app is fully functional without it.

## Workspace layout

| Crate      | Concern                                                        |
|------------|----------------------------------------------------------------|
| `jrc-core` | Shared domain types and errors                                 |
| `jrc-nlp`  | Morphological analysis and sentence segmentation               |
| `jrc-srs`  | FSRS spaced-repetition scheduler                               |
| `jrc-dict` | JMdict dictionary + frequency list download/lookup             |
| `jrc-db`   | SQLite persistence                                             |
| `jrc-app`  | Application services: ingestion, mining, reviews, stats        |
| `jrc-llm`  | Pluggable LLM explanation/feedback backend                     |
| `jrc-gui`  | egui desktop interface                                         |

## Building

```sh
cargo build --release
cargo run --release -p jrc-gui
```

On first launch the app downloads JMdict and a frequency list into its data
directory. A Japanese-capable system font is picked up automatically.

## Development

```sh
cargo test --workspace                                # full test suite
cargo clippy --workspace --all-targets -- -D warnings # lints
```

An end-to-end smoke run against real data (downloads JMdict on first use,
cached afterwards):

```sh
cargo run -p jrc-app --example smoke -- <data-dir> <utf8-text-file> "<title>"
```

The first build downloads and embeds the IPADIC morphological dictionary
(via lindera), which takes a few minutes once.

## Data sources

- [JMdict](https://www.edrdg.org/jmdict/j_jmdict.html) — © EDRDG, used under
  the [EDRDG licence](https://www.edrdg.org/edrdg/licence.html), fetched via
  the jmdict-simplified project.
- Word frequency — Leeds Internet Corpus frequency list (CC BY), with a
  built-in fallback.
