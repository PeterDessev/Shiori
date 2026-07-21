+++
title = "Online Sources"
weight = 5
+++

The Sources view (🌐 in the icon rail) — "Find books online" — searches free,
legal digital libraries and imports works into your library in one click.
Book search is **per language**: the active language decides which libraries
are searched and where imports land.

## The view

A **Language** switcher sits next to the heading. It is the same active
language used everywhere else in the app; changing it here re-scopes every
tab and clears the previous language's results. Below it, a row of tabs shows
the sources available for that language:

| Tab | When it appears | What it searches |
|-----|-----------------|------------------|
| **青空文庫** (Aozora Bunko) | Japanese only | Cached local catalog of public-domain Japanese literature |
| **Wikisource** | Languages with a Wikisource wiki | The language's `<code>.wikisource.org` |
| **Project Gutenberg** | Languages Gutenberg indexes | Project Gutenberg, filtered to the language, via the Gutendex API |
| **OPDS** | Always | The OPDS distributors you add for the language |
| **Libraries** | Always | A browsable directory of libraries for the language (read-only) |

A single search box serves the search tabs; its behavior differs per tab.
While an import is running, a spinner with an "importing N…" counter appears
in the header, and the Library nav icon shows the same count.

Imported works go through the normal import pipeline and land in the
[Library](@/docs/getting-started.md) exactly like a file import: they are
tokenized, difficulty-graded, and openable in the [Reader](@/docs/reading.md)
immediately. Imports land in the **active** language's library and run through
its analyzer, so the language switcher doubles as "import this book *as* this
language."

## Aozora Bunko

Aozora Bunko (青空文庫) is a volunteer archive of public-domain Japanese
literature. This tab appears only when Japanese is active.

### The catalog

- Shiori fetches Aozora's full catalog CSV once, in the background, when the
  app starts. Per Aozora's politeness guidance, the download comes from the
  project's GitHub mirror, not aozora.gr.jp itself.
- The catalog is cached in the data directory (`aozora_catalog.zip`). If you
  are offline, the last cached copy is used; if there is no cached copy yet,
  the view shows "Catalog not available" until you reconnect.
- The **⟳ Reload catalog** button next to the search box deletes the cache and
  fetches the current catalog.

Only importable entries are kept: public-domain works only (copyright column
なし), hosted on `aozora.gr.jp`, one row per work (the 著者 author row).

### Searching and importing

Search is local against the cached catalog — instant and offline — matching
the query as a substring of the **title**, **title reading** (kana), or
**author**. Clicking **⬇ Import** downloads the work's XHTML (GitHub mirror
first, aozora.gr.jp as fallback) and feeds it through the HTML pipeline
(furigana ruby stripped). Aozora sends no charset header, so the file is
decoded per the catalog's encoding column (UTF-8 or Shift_JIS). The publisher
field is set to 青空文庫.

## Wikisource

The Wikisource tab queries the active language's Wikisource wiki — classic
literature, historical documents, speeches, and law texts. The subdomain is
resolved from the language code (`fr` → fr.wikisource.org, `la` → Vicifons,
and so on); Ancient Greek, which has no dedicated wiki, falls back to the
Modern Greek Wikisource.

Type a query and press Enter or click **Search**: Shiori runs a MediaWiki
full-text search over mainspace (readable prose, not the `Page:` proofreading
namespace) and shows up to 20 hits with title, word count, and a snippet.
Clicking **⬇ Import** fetches the page's rendered HTML from the MediaWiki REST
API, strips it to text, and imports it — title from the page, publisher set to
the wiki's host, author left empty (editable afterward in the library).

## Project Gutenberg

The Project Gutenberg tab searches [Gutendex](https://gutendex.com/), a JSON
API over Gutenberg's catalog, filtered to the active language (Gutenberg
indexes records by **romanized** metadata, so search "Akutagawa", not "芥川").
Results list the title, author, and download count.

Clicking **⬇ Import** downloads the book, preferring a UTF-8 plain-text
edition (falling back to HTML, then EPUB). Project Gutenberg's license header
and footer are stripped so only the work itself is imported; the publisher is
set to Project Gutenberg.

## OPDS distributors

[OPDS](https://opds.io/) (Open Publication Distribution System) is the
standard catalog format e-book libraries expose. You add distributors per
language and search them here.

- **Add a distributor** with the form at the bottom of the tab: a name and the
  feed's root URL (`https://…`). Two anonymous, multilingual feeds — Project
  Gutenberg and Open Library — are offered as one-click suggestions. Added
  distributors are saved in your settings, per language.
- **Search** the selected distributor with the search box. Shiori follows the
  feed's advertised search (an OpenSearch description for OPDS 1.x, or a
  templated search link for OPDS 2.0); feeds without search are listed and
  filtered locally, and navigation-only feeds (like Gutenberg's) are followed
  one hop to reach the books.
- **Import** with **⬇ Import**: EPUB and PDF go through the file pipeline
  (extracted and copied into your library), HTML and plain text are imported
  directly. Both OPDS 1.x (Atom) and OPDS 2.0 (JSON) feeds are supported.

Some catalogs (e.g. Standard Ebooks' patron feed) require authentication;
those are not supported yet — use their public pages or another distributor.

## Libraries

The Libraries tab is a read-only directory, compiled from public catalog
metadata, of free and legal digital libraries: the collections dedicated to
the active language, followed by multilingual aggregators (Internet Archive,
Open Library, HathiTrust, …). Each entry links out to the site. Use it to
discover where a language's books live — then download a file and import it
from the Library page, or add the site's OPDS feed under the OPDS tab to search
it in-app.

## Politeness

Shiori is deliberately gentle with every service:

- Every request carries a descriptive User-Agent identifying the app, as the
  MediaWiki and Gutendex policies expect.
- Requests run serially — catalog fetches, searches, and imports happen one at
  a time, never in parallel bursts.
- The bulk Aozora catalog comes from the GitHub mirror rather than the Aozora
  site, and individual work downloads also try the mirror first.
- Wikisource search requests pass `maxlag=5`, so they back off automatically
  when the servers are under load.

## Offline behavior

After the first successful catalog fetch, Aozora search works fully offline.
Everything else — Wikisource, Project Gutenberg, OPDS, and all imports —
requires a connection. These fetches, optional LLM calls, and the language-pack
downloads under Settings → Languages are the app's only network features — see
[Getting-Started](@/docs/getting-started.md) for the first-run reference-data
download.
