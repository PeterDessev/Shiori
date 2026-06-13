+++
title = "Getting Started"
weight = 1
+++

Shiori is built from source with Cargo and sets itself up on first launch by
downloading its reference data. This page takes you from a clean checkout to
your first reading session.

## Building from source

You need Rust (rustc 1.88 or later) and a network connection for the first
build.

```sh
cargo build --release -p shiori-gui
cargo run --release -p shiori-gui
```

The first build downloads and embeds the IPADIC morphological dictionary via
lindera. This happens once, at build time, and takes a few minutes; later
builds reuse it. If the first build fails with a download error, check your
network and rebuild.

## First launch

On first launch Shiori shows a setup screen and offers to fetch its reference
data into the data directory:

| Data | Size | Used for |
|------|------|----------|
| JMdict dictionary | ~11 MB | Word definitions, usage registers, cross-references |
| Word frequency list | small | Frequency ranks, difficulty stats, recommendations |
| KANJIDIC2 + KanjiVG | ~5 MB | Kanji cards: readings, meanings, stroke-order diagrams |
| JLPT vocabulary lists | small | Graded reading-level statistics |

Each step is skipped if its data is already imported, so a failed or
interrupted download can be retried and only fetches what is missing.

### Continue without dictionary

The setup screen also has a **Continue without dictionary** button. The app
runs fully without the reference data — you can import, read, mark words, and
review with SRS — but anything dictionary-derived is unavailable: definitions,
compound lookup, frequency ranks, usage registers, and kanji cards. The word
panel will say "No dictionary installed" (distinct from "no entry found for
this word").

While running this way, a dismissible banner at the top of the window carries
a retry button for the download and an info button explaining exactly what is
and is not available. You do not need to dig through settings to retry.

## Where your data lives

Everything lives in `%APPDATA%\shiori` (typically
`C:\Users\<you>\AppData\Roaming\shiori`):

- `jrc.sqlite3` — the database: your library, word statuses, review cards and
  history, reading sessions, chat, and the imported reference data.
- `books\` — archival copies of every file you import, so the original can be
  moved or deleted afterwards.
- Cached downloads (JMdict JSON, kanji archives, fonts, the Aozora catalog).

After first run the app is fully local. The only features that touch the
network are LLM calls to remote providers and the Sources catalog fetch,
which falls back to its cached copy when offline.

## Importing your first book

Two ways in:

- **Drag and drop** — drop one or more files anywhere onto the Library view.
  A file dialog import button is also available there.
- **Sources view** — search Aozora Bunko's public-domain catalog or Japanese
  Wikisource and import a work in one click.

Supported file types:

| Type | Notes |
|------|-------|
| `.txt`, `.md` | UTF-8 or Shift_JIS, detected automatically. Aozora-style headings (title on line 1, author on line 2) prefill the import form. |
| `.html`, `.htm`, `.xhtml` | Aozora Bunko pages work directly; ruby furigana annotations are stripped from the text. |
| `.epub` | Chapters are read in spine order; title and author come from the book's metadata. |
| `.pdf` | Text-layer extraction only — scanned PDFs need OCR first. The title falls back to the filename. |

The import form lets you edit title, author, and other metadata before the
book is added; metadata remains editable from the library afterwards.

## Your first reading session

Open a book from the Library and you get a paged, e-reader-style view. The
core interaction is clicking words:

- **Click any word** to open the dictionary panel on the right: definition,
  usage register, and frequency information.
- **Conjugated phrases are selected whole** — clicking 読んでいる selects the
  full phrase and the panel explains the form component by component.
- The panel's buttons assign a knowledge status: **Learn (SRS)** creates a
  review card with the sentence you found it in, **Known**, **Ignore**, and
  **Reset** set the status directly.

Every word has one of four statuses:

| Status | Meaning |
|--------|---------|
| unknown | The default for every word you have not touched. Counts against your coverage stats. |
| learning | You are studying it — it has an SRS card and appears in reviews. |
| known | You read it without help. Counts toward coverage and your reading level. |
| ignored | Excluded from all statistics. Use it for names and loanwords you read for free. |

Furigana display (including "unknown words only" and "first X instances per
book") is configured in Settings → Reading.

From here, see [Reading](@/docs/reading.md) for the full reader — furigana modes, the
away/pause clock, and the book info panel — and [Reviews-and-SRS](@/docs/reviews-and-srs.md)
for how cards are scheduled and reviewed.