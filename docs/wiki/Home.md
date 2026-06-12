# Home

Shiori（栞, "bookmark"）is a Windows-first desktop application for learning
Japanese through reading. This wiki documents what the app does and how to
use it.

## What Shiori is

Shiori is built around **comprehensible input**: the primary activity is
reading real Japanese text, and every other feature exists to support that.
It is not a flashcard driller — it is a reading companion that happens to
teach.

You import books, articles, or any Japanese text (paste, `.txt`/`.md`,
`.html` including Aozora Bunko pages, `.epub`, `.pdf`), and the app parses
everything to the morpheme level while preserving sentence and paragraph
context. While reading you click any word to see its dictionary entry, usage
register, and a component-by-component explanation of its conjugated form;
one click adds it to spaced-repetition review. The app tracks what you know,
grades every document in your library by difficulty, and tells you what to
read next.

Shiori runs fully offline after first launch. The only network features are
optional LLM calls for conversation practice and the online catalog fetch in
Sources, which falls back to its local cache.

## The main views

The icon rail on the left edge of the window switches between views. From
top to bottom: **Library** is the home for your imported documents, with
per-book difficulty, coverage forecasts, and an info panel for each book.
The **Reader** opens the current book as clickable pages with configurable
furigana and an in-context dictionary panel; the icon is enabled only while
a document is open. **Review** runs your due flashcards (the rail shows a
due-count badge), each card presenting the word in the sentence it came
from. **Dictionary & kanji** is a single search box over JMdict word entries
and KANJIDIC2 kanji cards with stroke-order diagrams. **Sources** searches
Aozora Bunko and Japanese Wikisource and imports works in one click.
**Statistics** shows reading velocity, a reading calendar, your JLPT-graded
comfortable reading level, review forecasts, and retention. **Production
practice** is an optional LLM chat where a native-speaker persona converses
with you and your mistakes come back as paper-style underlines on your own
messages. **Settings** holds the categories General, Appearance, Reading,
Review, AI, Shortcuts, and Data. At the bottom of the rail, the ❓ icon
opens the in-app getting-started guide.

## Wiki contents

| Page | Covers |
|------|--------|
| [Getting-Started](Getting-Started) | First launch, reference-data download, importing your first text, the four word statuses |
| [Reading](Reading) | The reader: clickable tokens, conjugation-aware selection, furigana modes, paging, away/pause and the reading clock |
| [Reviews-and-SRS](Reviews-and-SRS) | FSRS scheduling, in-context cards, cross-book example sentences, the mark-known-on-finish sweep |
| [Dictionary-and-Kanji](Dictionary-and-Kanji) | JMdict search, kanji cards (readings, meanings, grade, JLPT, stroke order), add-to-SRS from search |
| [Online-Sources](Online-Sources) | Aozora Bunko and Wikisource-ja search, catalog caching, one-click import |
| [AI-and-Chat](AI-and-Chat) | Production chat, annotation underlines, level calibration, backends (Anthropic, Ollama, OpenAI-compatible) |
| [Statistics](Statistics) | Reading velocity and calendar, comfortable reading level, review forecast, learning rate, retention |
| [Data-and-Interop](Data-and-Interop) | Anki .apkg export/import, settings export/import, database backup and restore |
| [Architecture](Architecture) | Workspace crates, data directory, SQLite schema, NLP pipeline |

## Where things live

| Concern | Place |
|---------|-------|
| Imported documents | Library view; metadata editable per book |
| Word lookups while reading | Reader → click a token → right-side word panel |
| Kanji details | Dictionary & kanji view, or kanji chips under a headword in the reader's word panel |
| Furigana, fonts, theme | Settings → Reading and Settings → Appearance |
| LLM configuration | Settings → AI |
| Keyboard shortcuts | Settings → Shortcuts (press-to-record) |
| Backups and Anki export | Settings → Data |
