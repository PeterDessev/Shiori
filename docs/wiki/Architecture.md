# Architecture

How Shiori is put together: an eight-crate Cargo workspace over a single SQLite database, with an egui shell that pushes all heavy work onto background threads. This page is for contributors; user-facing behavior is covered on the other wiki pages.

## Workspace layout

Eight crates live under `crates/`, all sharing the workspace version, edition, and dependency set from the root `Cargo.toml`.

| Crate | Responsibility |
|---|---|
| `shiori-core` | Shared domain types and errors (Document, Word, knowledge statuses, ids) |
| `shiori-nlp` | Morphological analysis and sentence segmentation (lindera + IPADIC) |
| `shiori-srs` | FSRS spaced-repetition scheduler |
| `shiori-dict` | JMdict dictionary and frequency list acquisition and lookup |
| `shiori-db` | SQLite persistence layer |
| `shiori-app` | Application services: ingestion, mining, reviews, statistics |
| `shiori-llm` | Pluggable LLM explanation and feedback backend |
| `shiori-gui` | egui desktop interface |

`shiori-app` is also where text extraction lives (`.epub`, `.pdf`, Shift_JIS decoding) and where the Sources catalogs (Aozora Bunko, Wikisource) are fetched and parsed.

## Layering

Dependencies point strictly downward:

```
shiori-gui
   └── shiori-app
          └── shiori-db   shiori-dict   shiori-nlp   shiori-srs
                                └── shiori-core
```

- `shiori-gui` talks to `shiori-app` for everything domain-shaped; it also consumes `shiori-llm` directly (the LLM backend depends only on `shiori-core` and knows nothing about the database).
- `shiori-app` orchestrates the four domain crates. It owns policy: what counts as a known word, how a review is graded, what an import does.
- `shiori-db` is deliberately policy-free. It stores and retrieves rows; notably, JMdict entries are persisted as **opaque JSON** in `dict_entries.json` — the db crate never parses them. Interpreting that JSON is `shiori-dict`'s job.
- `shiori-core` holds the types everyone shares and depends on nothing in the workspace.

## SQLite schema (v7)

Defined in `crates/shiori-db/src/schema.rs`. Current tables:

| Table | Purpose |
|---|---|
| `meta` | Key/value store; holds `schema_version` among other stamps |
| `documents` | Imported texts: title/author/publisher/published, reading position (`last_sentence`), unique `content_hash` |
| `sentences` | Sentence text with document, order index, and paragraph number |
| `words` | One row per unique (lemma, reading, pos) with knowledge `status` and optional JMdict `dict_seq` |
| `tokens` | Per-sentence token spans tying surface forms back to `words` |
| `frequency` | Corpus frequency rank per word form |
| `dict_entries` | JMdict entries as opaque JSON, keyed by JMdict sequence number |
| `dict_forms` | Lookup index from a written/kana form to its entry `seq`, with common-form flags |
| `cards` | One FSRS card per word: state, stability, difficulty, `due`, reps/lapses, optional source sentence |
| `review_log` | Every review: rating, timestamp, and post-review FSRS state |
| `reading_sessions` | Active reading time, one row per continuous sitting (`seconds`, `chars`) |
| `conversations` | Production-chat conversation headers |
| `chat_messages` | Ordered chat messages with role and content |
| `chat_annotations` | Paper-style write-up spans over a *user* message (byte offsets, severity, note) |
| `kanji` | KANJIDIC2 reference data joined with KanjiVG stroke paths |
| `jlpt_words` | Community JLPT vocabulary lists, used for level grading |

### Migration pattern

`migrate()` is idempotent and runs on every database open:

1. The base DDL (`SCHEMA_V1` in name only) always describes the **latest** shape of every table and uses `CREATE TABLE IF NOT EXISTS` throughout. A fresh database gets everything in one batch and needs no ALTERs.
2. If `meta.schema_version` exists and is older than the current version (`SCHEMA_VERSION = 7`), incremental `ALTER TABLE` steps run for the changes that modified existing tables (v2 document metadata columns, v3 reading position). Versions that only *added* tables (v4 sessions, v5 chat, v6 kanji, v7 JLPT) need no step — the `IF NOT EXISTS` batch creates them.
3. The current version is stamped back into `meta`.

When you change the schema: edit the base DDL to the new shape, bump `SCHEMA_VERSION`, and add an `if current < N` step **only** if existing tables changed.

## GUI threading model

The egui shell (`crates/shiori-gui/src/app.rs`) never blocks the UI thread. Anything slow — opening the database, first-run downloads, document import, dictionary/LLM calls, Ollama probing and pulls, Sources catalog fetches, backup/export — runs on a spawned background thread that posts its result back over an `std::sync::mpsc` channel as a `Msg` variant (`AppOpened`, `ImportDone`, `ChatReply`, `OllamaPullProgress`, `TransferDone`, …). The frame loop drains the receiver each frame and applies results to state. Startup itself is a small state machine (`Phase`: Starting → NeedsData → Downloading → Ready).

## Heavy work and build time

- Morphological analysis uses lindera with the `embed-ipadic` feature: the IPADIC dictionary is downloaded and **embedded into the binary at build time**. The first build takes a few minutes; afterwards it is cached.
- Because the embedded dictionary is unusably slow unoptimized, the workspace sets `[profile.dev.package."*"] opt-level = 2` — dependencies are always optimized, even in dev builds.
- Reference data (JMdict, frequency list, KANJIDIC2 + KanjiVG, JLPT lists) is downloaded at first run into the data directory and loaded into SQLite; after that the app is fully local.

## Tests

```sh
cargo test --workspace                                # full suite
cargo clippy --workspace --all-targets -- -D warnings # lints
```

- **Unit tests** live inside each crate next to the code they cover (for example, the migration test in `shiori-db/src/schema.rs` verifies a v1 database gains the new columns and re-running `migrate` is a no-op).
- **Integration test**: `crates/shiori-app/tests/pipeline.rs` exercises the whole pipeline end to end — the real lindera analyzer, an in-memory SQLite database, and fixture JMdict/frequency data — through import, lookup, status changes, and reviews.
- **Smoke run** against real downloaded data: `cargo run -p shiori-app --example smoke -- <data-dir> <utf8-text-file> "<title>"`.
