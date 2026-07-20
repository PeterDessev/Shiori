+++
title = "Architecture"
weight = 10
+++

How Shiori is put together: an eleven-crate Cargo workspace over a single SQLite database, with an egui shell that pushes all heavy work onto background threads. This page is for contributors; user-facing behavior is covered on the other docs pages.

## Workspace layout

Eleven crates live under `crates/`, all sharing the workspace version, edition, and dependency set from the root `Cargo.toml`.

| Crate | Responsibility |
|---|---|
| `shiori-core` | Shared domain types and errors (Document, Word, knowledge statuses, ids) |
| `shiori-lang` | The `LanguageService` trait — everything Shiori asks of a language (depends only on `shiori-core`) |
| `shiori-nlp` | Japanese: morphological analysis and sentence segmentation (lindera + IPADIC behind a default-on `embed-ipadic` feature) plus the Japanese `LanguageService` implementation |
| `shiori-pack` | Data-driven language packs: manifest loading, SIAT pre-annotated texts, kaikki.org Wiktextract parsing, betacode — `PackLanguage` implements `LanguageService` |
| `shiori-packc` | The CI pack-compiler CLI: builds the Koine Greek pack from MorphGNT and Wiktionary packs from kaikki.org dumps, has a `catalog` subcommand, and refuses NonCommercial sources outright. Never shipped in the app |
| `shiori-srs` | FSRS spaced-repetition scheduler |
| `shiori-dict` | JMdict dictionary and frequency list acquisition and lookup |
| `shiori-db` | SQLite persistence layer |
| `shiori-app` | Application services: ingestion, mining, reviews, statistics |
| `shiori-llm` | Pluggable LLM explanation and feedback backend |
| `shiori-gui` | egui desktop interface |

`shiori-app` is also where text extraction lives (`.epub`, `.pdf`, Shift_JIS decoding), where the Sources catalogs (Aozora Bunko, Wikisource) are fetched and parsed, and — since 0.2.0 — where language packs are discovered, installed, and removed under `<data>/packs/<code>/` (`packs.rs`) and where the Build-from-Wiktionary pipeline downloads its inputs into `<data>/web-sources/` with resumable HTTP range requests (`web_packs.rs`).

## Layering

Dependencies point strictly downward:

```
shiori-gui
   ├── shiori-app
   │      └── shiori-db   shiori-dict   shiori-nlp   shiori-pack   shiori-srs
   └── shiori-llm
                     shiori-lang        ← nlp, pack, llm, app, gui
                        └── shiori-core ← everyone

shiori-packc ── shiori-pack             (CI tool, never shipped in the app)
```

- `shiori-gui` talks to `shiori-app` for everything domain-shaped; it also consumes `shiori-llm` directly (the LLM backend depends only on `shiori-core` and `shiori-lang` and knows nothing about the database).
- `shiori-app` orchestrates the five domain crates. It owns policy: what counts as a known word, how a review is graded, what an import does, which `LanguageService` is active.
- `shiori-lang` sits below every language implementation: `shiori-nlp` (Japanese) and `shiori-pack` (`PackLanguage`) both implement its `LanguageService` trait. It depends only on `shiori-core`.
- `shiori-db` is deliberately policy-free. It stores and retrieves rows; notably, dictionary entries are persisted as **opaque JSON** in `dict_entries.json`, keyed by `(source, entry_key)` — the db crate never parses them. Interpreting the `jmdict` source is `shiori-dict`'s job; pack sources are `shiori-pack`'s. (`shiori-db` also depends on `shiori-srs` for the card-state types it stores.)
- `shiori-packc` is the pack compiler run in CI; it depends on `shiori-pack` and is never part of the app binary.
- `shiori-core` holds the types everyone shares and depends on nothing in the workspace.

## The LanguageService trait

Since 0.2.0 every language behavior routes through one trait: `LanguageService` in `crates/shiori-lang/src/service.rs` — "everything Shiori asks of a language". Analysis (`analyze`, `tokenize_sentence`), phrase grouping and inflection description, the dictionary source, search transliteration (romaji → kana, betacode/Greeklish → polytonic Greek), lookup normalization, contractions, ruby annotation, and per-character reference cards are all trait methods; the defaults implement a plain alphabetic language, so implementations override only what their language actually needs. Two implementations exist: Japanese in `crates/shiori-nlp/src/japanese.rs` and the pack-backed `PackLanguage` in `crates/shiori-pack/src/language.rs`. `shiori-app` resolves the active language's service, and every import, lookup, and analysis flows through it.

## SQLite schema (v8)

Defined in `crates/shiori-db/src/schema.rs`. Since v8 every user-facing table carries a language dimension and the reference caches are keyed by language or source, so a second language can neither collide with nor wipe another's data. Current tables:

| Table | Purpose |
|---|---|
| `meta` | Key/value store; holds `schema_version` among other stamps |
| `documents` | Imported texts: language, title/author/publisher/published, reading position (`last_sentence`), unique `(lang, content_hash)` |
| `sentences` | Sentence text with document, order index, and paragraph number |
| `words` | One row per unique (lang, lemma, reading, pos) with knowledge `status` and an optional dictionary link (`dict_source` + `dict_key`) |
| `tokens` | Per-sentence token spans tying surface forms back to `words`, with optional per-occurrence `morph` and `gloss` columns for pre-annotated pack texts (the Koine Greek interlinear) |
| `frequency` | Corpus frequency rank per word form, keyed by (lang, word) |
| `dict_entries` | Dictionary entries as opaque JSON, keyed by (source, entry_key) — `jmdict` keys by sequence number, pack sources by their own scheme |
| `dict_forms` | Lookup index from a form to its entry, keyed by (source, text, entry_key), with a `role` column (`orthographic`, `phonetic`, `canonical`) and common-form flag |
| `cards` | One FSRS card per word: state, stability, difficulty, `due`, reps/lapses, optional source sentence |
| `review_log` | Every review: rating, timestamp, and post-review FSRS state |
| `reading_sessions` | Active reading time, one row per continuous sitting (`seconds`, `chars`) |
| `conversations` | Production-chat conversation headers, one language each |
| `chat_messages` | Ordered chat messages with role and content |
| `chat_annotations` | Paper-style write-up spans over a *user* message (byte offsets, severity, note) |
| `kanji` | KANJIDIC2 reference data joined with KanjiVG stroke paths |
| `graded_vocab` | Per-language graded vocabulary lists: JLPT for Japanese, GNT frequency tiers for Koine Greek, Top-500/1k/2k/5k tiers for Wiktionary-built packs |
| `dict_tags` | Per-source tag decoding (POS, register, and parse codes), populated by language packs |
| `morph_forms` | Full-form morphology lookup backing Tier-1 analysis for languages without a runtime analyzer |

### Migration pattern

`migrate()` is idempotent and runs on every database open:

1. The base DDL (`SCHEMA_V1` in name only) always describes the **latest** shape of every table and uses `CREATE TABLE IF NOT EXISTS` throughout. A fresh database gets everything in one batch and needs no ALTERs.
2. If `meta.schema_version` exists and is older than the current version (`SCHEMA_VERSION = 8`), incremental steps run. v2 (document metadata columns) and v3 (reading position) are plain `ALTER TABLE`s; versions that only *added* tables (v4 sessions, v5 chat, v6 kanji) need no step — the `IF NOT EXISTS` batch creates them. v8 — the language dimension — is bigger: SQLite cannot alter table constraints, so the tables whose keys changed (`words`, `documents`, `dict_entries`, `dict_forms`, `frequency`) are rebuilt via CREATE + INSERT-SELECT + DROP + RENAME inside one transaction with foreign keys switched off, existing rows backfilling as `ja`. Each step is guarded by a column-presence check, so a partially migrated database is handled correctly. Before any of this touches a pre-v8 database, `Db::open` takes a one-time file copy `jrc.sqlite3.v7-backup` via `VACUUM INTO` (skipped if the file already exists).
3. The current version is stamped back into `meta`.

When you change the schema: edit the base DDL to the new shape, bump `SCHEMA_VERSION`, and add an `if current < N` step **only** if existing tables changed.

## GUI threading model

The egui shell (`crates/shiori-gui/src/app.rs`) never blocks the UI thread. Anything slow — opening the database, first-run downloads, document import, dictionary/LLM calls, Ollama probing and pulls, Sources catalog fetches, backup/export, pack catalog fetches, pack installs/downloads/removals, Build-from-Wiktionary builds — runs on a spawned background thread that posts its result back over an `std::sync::mpsc` channel as a `Msg` variant (`AppOpened`, `ImportDone`, `ChatReply`, `OllamaPullProgress`, `TransferDone`, `PackCatalog`, …). The frame loop drains the receiver each frame and applies results to state. Startup itself is a small state machine (`Phase`: Starting → NeedsData → Downloading → Ready).

## Heavy work and build time

- Morphological analysis uses lindera with the IPADIC dictionary downloaded and **embedded into the binary at build time**. The embedded dictionary sits behind `shiori-nlp`'s own default-on `embed-ipadic` cargo feature — the kana, romaji, ruby, and inflection utilities build without it, so a lindera build break can never block a pack-only build. The first build takes a few minutes; afterwards it is cached.
- Because the embedded dictionary is unusably slow unoptimized, the workspace sets `[profile.dev.package."*"] opt-level = 2` — dependencies are always optimized, even in dev builds.
- Reference data (JMdict, frequency list, KANJIDIC2 + KanjiVG, JLPT lists) is downloaded at first run into the data directory and loaded into SQLite; after that the app is fully local.

## Tests

```sh
cargo test --workspace                                # full suite
cargo clippy --workspace --all-targets -- -D warnings # lints
```

- **Unit tests** live inside each crate next to the code they cover (for example, the migration test in `shiori-db/src/schema.rs` verifies a v1 database gains the new columns and re-running `migrate` is a no-op).
- **Integration test**: `crates/shiori-app/tests/pipeline.rs` exercises the whole pipeline end to end — the real lindera analyzer, an in-memory SQLite database, and fixture JMdict/frequency data — through import, lookup, status changes, and reviews.
- **Golden token snapshot**: `crates/shiori-nlp/tests/token_snapshot.rs` (with its `fixtures/`) pins Japanese analysis output bit-for-bit, proving the `LanguageService` refactor changed nothing about tokenization.
- **Real-database migration**: `crates/shiori-db/tests/real_db_migration.rs` runs the v8 migration against a real pre-0.2.0 database, including the `.v7-backup` safety copy.
- **Smoke run** against real downloaded data: `cargo run -p shiori-app --example smoke -- <data-dir> <utf8-text-file> "<title>"`.
