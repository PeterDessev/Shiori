# Online-Sources

The Sources view (🌐 in the icon rail) searches two free online libraries — Aozora
Bunko and Japanese Wikisource — and imports works into your library in one click.

## The view

Two tabs sit next to the heading: **青空文庫** (Aozora Bunko) and **Wikisource**.
A single search box serves both; its behavior differs per tab, as described
below. While an import is running, a spinner with an "importing N…" counter
appears in the header, and the Library nav icon shows the same count.

Imported works go through the normal import pipeline and land in the
[Library](Getting-Started) exactly like a file import: they are tokenized,
difficulty-graded, and openable in the [Reader](Reading) immediately. The
library refreshes as soon as each import finishes.

## Aozora Bunko

Aozora Bunko (青空文庫) is a volunteer archive of public-domain Japanese
literature.

### The catalog

- Shiori fetches Aozora's full catalog CSV once, in the background, when the
  app starts. Per Aozora's politeness guidance, the download comes from the
  project's GitHub mirror, not aozora.gr.jp itself.
- The catalog is cached in the data directory (`aozora_catalog.zip`). If you
  are offline, the last cached copy is used; if there is no cached copy yet,
  the view shows "Catalog not available" until you reconnect.
- The **⟳ Reload catalog** button next to the search box deletes the cache and
  fetches the current catalog.

Only importable entries are kept from the catalog:

| Filter | Rule |
|--------|------|
| Copyright | Public-domain works only (copyright column is なし) |
| Hosting | Works whose XHTML file is hosted on `aozora.gr.jp` only |
| Role | One row per work, the author (著者) row — translator/editor duplicates are dropped |

### Searching

Search is local against the cached catalog — instant and offline. Typing in
the search box filters as you type; a result matches when the query is a
substring of the work's **title**, **title reading** (kana), or **author**, so
both 坊っちゃん and なつめ find their books. The first 50 matches are shown,
each with the author and the work's orthography (e.g. 新字新仮名). With an
empty query the view shows how many works are available.

### Importing

Clicking **⬇ Import** downloads the work's XHTML file — from the GitHub
mirror first, falling back to aozora.gr.jp — and feeds it through the HTML
import pipeline (furigana ruby markup is stripped). Aozora's servers send no
charset header, so the file is decoded according to the catalog's encoding
column: UTF-8 when the catalog says so, Shift_JIS otherwise. Title and author
come from the catalog; the publisher field is set to 青空文庫.

## Wikisource

The Wikisource tab searches Japanese Wikisource (ja.wikisource.org) — classic
literature, historical documents, speeches, and law texts.

### Searching

Unlike the Aozora tab, this is a remote query: type a query and press Enter or
click **Search**. Shiori runs a MediaWiki full-text search over mainspace
articles and shows up to 20 hits, each with its title, word count, and a text
snippet around the match.

### Importing

Clicking **⬇ Import** fetches the page's rendered (Parsoid) HTML from the
MediaWiki REST API, strips it to plain text, and imports it. The page title
becomes the document title and the publisher field is set to Wikisource; there
is no author metadata in the search results, so the author field is left empty
(you can edit it afterwards in the library).

## Politeness

Shiori is deliberately gentle with both services:

- Every request carries a descriptive User-Agent identifying the app, as the
  MediaWiki API policy requires.
- Requests run serially — catalog fetches, searches, and imports happen one at
  a time, never in parallel bursts.
- The bulk Aozora catalog comes from the GitHub mirror rather than the Aozora
  site, and individual work downloads also try the mirror first.
- Wikisource search requests pass `maxlag=5`, so they back off automatically
  when the servers are under load.

## Offline behavior

After the first successful catalog fetch, Aozora search works fully offline.
Importing a work and anything on the Wikisource tab require a connection.
These catalog and import fetches, plus optional LLM calls, are the app's only
network features — see [Getting-Started](Getting-Started) for the first-run
reference-data download.
