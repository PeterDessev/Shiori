# Data and Interop

Where Shiori keeps your data, how to move cards to and from Anki, and how to
back up or restore the database. Everything described here lives under
**Settings → Data** unless noted otherwise.

## Data directory

All app data lives in `%APPDATA%\shiori` (for example
`C:\Users\you\AppData\Roaming\shiori`). The exact path is shown at the top of
Settings → Data. If you used the app before its rename, the old
`japanese-reading-companion` directory is renamed to `shiori` automatically on
first launch.

| File / folder | Contents |
|---|---|
| `jrc.sqlite3` | The main SQLite database: words, SRS cards, documents, reading sessions, and the imported reference data. The filename predates the Shiori rename and is kept for compatibility. |
| `jmdict-eng.json` | JMdict dictionary (downloaded on first run) |
| `frequency.txt` | Word frequency list |
| `kanjidic2.xml.gz`, `kanjivg.xml.gz` | Kanji readings/meanings and stroke-order data |
| `jlpt_vocab.csv` | JLPT vocabulary lists |
| `aozora_catalog.zip` | Cached Aozora Bunko catalog for [Online-Sources](Online-Sources) |
| `fonts/` | Noto Sans/Serif JP fonts, downloaded on first use |
| `books/` | Archival copies of every imported file |
| `settings.json` | All settings, including LLM API keys |

## Anki export

Settings → Data → Anki → **Export deck (.apkg)** writes every SRS card as a
note with four fields: Expression, Reading, Meaning, Sentence. The package
uses Anki's legacy schema-11 format, which every Anki version can import.

- **Scheduling is approximate.** Shiori schedules with FSRS; Anki's classic
  scheduler is SM-2. At Anki's default 90% desired retention an SM-2 interval
  roughly equals FSRS stability, so the export maps stability → interval and
  difficulty → ease (difficulty 5 ⇄ ease 2500). New cards export as new.
- **Re-exports update, not duplicate.** Note guids are derived from the word
  identity (lemma, reading, part of speech), so importing a newer export into
  the same Anki collection updates the existing notes.
- Exporting with no SRS cards is an error — learn some words first.

## Anki import

Settings → Data → Anki → **Import deck (.apkg)** brings an existing deck into
Shiori's SRS. For each note:

- The first field containing Japanese (HTML stripped) is morphologically
  analyzed, and its head token becomes the canonical word key — the same
  lemma + reading + part-of-speech identity used by reading and reviews.
- SM-2 scheduling seeds the FSRS state approximately: stability ≈ the SM-2
  interval (the 90%-retention equivalence again), difficulty from the ease
  factor, and the due date is preserved (clamped to 0–365 days out).
  Cards never reviewed in Anki import as new cards.
- Imported words with stability of 60 days or more are marked **known**;
  the rest are marked **learning** (see [Reviews-and-SRS](Reviews-and-SRS)).
- **Existing cards are never overwritten.** A note whose word already has a
  card in Shiori is skipped, as are notes with no Japanese field. The result
  message reports imported and skipped counts.

One sharp edge: Anki's current default export uses a newer zstd-compressed
format (`collection.anki21b`). Shiori rejects it with a clear message —
re-export from Anki with **"Support older Anki versions"** checked.

## Settings export and import

Settings → Data → Settings file. **Export settings** saves a copy of
`settings.json` wherever you choose; **Import settings** loads such a file and
applies it immediately. The file includes any LLM API keys you have entered
under Settings → AI, so treat exports as secrets if you share machines.

## Database backup and restore

Settings → Data → Database.

- **Back up database** writes a clean single-file copy via SQLite's
  `VACUUM INTO`. It is safe to run while the app is open — pending
  write-ahead-log contents are folded into the copy.
- **Restore from backup** does not touch the live database immediately. The
  chosen file is staged as `jrc.sqlite3.restore-pending` and swapped in on
  the next launch; restart the app to complete the restore. Your current
  database is kept aside as `jrc.sqlite3.pre-restore`, so a restore is
  reversible until the next one overwrites that file.

## Offline behavior

After the first run all reference data (dictionary, frequency list, kanji
data, JLPT lists) lives locally in the database and data directory. Only two
things need the network afterwards:

- **LLM calls to remote providers** (Anthropic or a remote custom endpoint).
  Local backends via Ollama stay offline — see [AI-and-Chat](AI-and-Chat).
- **Source search** in [Online-Sources](Online-Sources): Wikisource search
  and catalog/book downloads. The Aozora catalog falls back to the last
  cached copy when offline.

Reading, importing, word lookup, reviews, and statistics all work offline.
First-run setup can also proceed without the dictionary download — see
[Getting-Started](Getting-Started).
