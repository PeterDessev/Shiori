+++
title = "Getting Started"
weight = 1
+++

Shiori ships as a prebuilt Windows binary and sets itself up on first launch
by downloading its reference data. This page takes you from a download — or
a clean checkout, if you prefer to build — to your first reading session.

## Download

Grab the latest `shiori-*-windows-x86_64.zip` from
[GitHub Releases](https://github.com/PeterDessev/Shiori/releases), unzip it
anywhere, and run `shiori.exe`. The zip contains the executable plus the
licenses, README, and changelog. The binary is statically linked against the
MSVC C runtime, so it runs on stock Windows 10/11 with no VC++
Redistributable and nothing else to install.

Windows x86_64 is the only supported target — the only one the CI tests and
ships. The source has no hard OS lock, so building on macOS or Linux may
well work, but it is untested and unsupported for now.

## Building from source

The alternative to the release zip. You need Rust (rustc 1.88 or later) and
a network connection for the first build.

```sh
cargo build --release -p shiori-gui
cargo run --release -p shiori-gui
```

The first build downloads and embeds the IPADIC morphological dictionary via
lindera (behind a default-on cargo feature). This happens once, at build
time, and takes a few minutes; later builds reuse it. If the first build
fails with a download error, check your network and rebuild.

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

### After setup

Once setup and the one-time welcome guide finish, the app opens on a
**Home** view: the active language with a quick switcher and a "Manage
languages" shortcut, cards due today with a time estimate from your
measured review pace, a continue-reading card for the book you are in the
middle of (progress, estimated time left, unknown words ahead, difficulty
verdict), and the reading-activity calendar.

## Choosing and adding languages

The first-launch reference-data table above is the Japanese bundle. Every
other language installs as a data-driven language pack under
**Settings → Languages**:

- **Activate** a language to switch the whole app — library, reader,
  dictionary, reviews, statistics, chat — into it. Nothing mixes across
  languages.
- **Install** a pack from a folder, a zip, or a download URL, with optional
  SHA-256 verification. The page shows what each installed pack ships
  (license, dictionary, morphology, frequency, graded levels, fonts).
- **Import bundled texts** — a pack that ships texts (the Koine Greek pack
  does) adds them to the library with one click.
- **Remove** a pack — its library and review history stay in the database.
- **Build from Wiktionary** — pick from ~19 languages (French, German,
  Spanish, Latin, Koine Greek, Korean, …) and the app downloads
  kaikki.org's Wiktextract dump plus hermitdave's frequency list and
  compiles the pack locally, the same first-run model as the Japanese
  bundle. The dumps are gigabyte-class; interrupted downloads resume
  instead of restarting.

The pack format itself is documented in
[Language-Packs](https://github.com/PeterDessev/Shiori/blob/master/docs/wiki/Language-Packs.md).

## Where your data lives

Everything lives in `%APPDATA%\shiori` (typically
`C:\Users\<you>\AppData\Roaming\shiori`):

- `jrc.sqlite3` — the database: your library, word statuses, review cards and
  history, reading sessions, chat, and the imported reference data.
- `books\` — archival copies of every file you import, so the original can be
  moved or deleted afterwards.
- `packs\<code>\` — each installed language pack (Settings → Languages
  installs into it; a pack directory can also be dropped there by hand).
- Cached downloads (JMdict JSON, kanji archives, fonts, the Aozora catalog).

After first run the app is fully local. The only features that touch the
network are LLM calls to remote providers, the Sources catalog fetch (which
falls back to its cached copy when offline), installing a language pack
from a URL, and Build-from-Wiktionary downloads.

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

The same click-to-look-up flow applies in a pack language, and for
pre-annotated texts (the Koine Greek pack) the furigana slot shows an
interlinear gloss while the word panel decodes each occurrence's parse to
prose — the [Reading](@/docs/reading.md) page has the details.

From here, see [Reading](@/docs/reading.md) for the full reader — furigana modes, the
away/pause clock, and the book info panel — and [Reviews-and-SRS](@/docs/reviews-and-srs.md)
for how cards are scheduled and reviewed.