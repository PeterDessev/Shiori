+++
title = "Languages"
weight = 7
+++

Shiori grew up Japanese-only; since 0.2.0 it reads any language a
**language pack** provides, with dead languages as first-class citizens.
Japanese stays compiled in — its analyzer is real code — while every other
language is *data*: a directory of files the app loads at runtime, no
recompile, no restart. This page covers installing and switching languages,
the Koine Greek pack, building packs from Wiktionary, and what changes in
the app when the active language is not Japanese.

The one rule underneath it all: **nothing mixes across languages**. Library,
reader, dictionary, reviews, statistics, and conversation practice all
operate in the active language; a Greek λόγος and a Spanish *sol* can never
collide with your Japanese words, and difficulty statistics never average
across languages.

## What a pack is

A pack is a folder (or zip) with a `manifest.toml` and the language's data
files: a dictionary, a full-form morphology table (every inflected form
mapped to its lemma and parse), a frequency list, graded vocabulary levels,
and optionally pre-annotated texts ready to import. The app only consumes
finished packs — the pack *compiler* (`shiori-packc`) is a separate
developer tool, and the format itself is documented in
[Language-Packs](https://github.com/PeterDessev/Shiori/blob/master/docs/wiki/Language-Packs.md)
if you want to build or author one.

## Settings → Languages

Everything language-related lives on one settings page.

### Activating

The **Active language** dropdown switches the whole app. On first
activation a pack's reference data (dictionary, frequency, graded levels)
imports into the database, scoped so it can never touch another language's
data; after that, switching is instant.

### Installed languages

Each installed language gets a card showing what its pack ships: the
license, then the data files present — dictionary, full-form morphology,
frequency list, graded levels (with the scheme's name), and any fonts the
manifest declares. A pack that ships texts (the Koine Greek pack does)
shows their count next to an **Import into library** button: one click adds
the pre-annotated texts to that language's library, skipping any already
imported. The button appears once the language is active.

### Installing a pack

Three ways in, under **Add a language**:

- **Install from folder** — pick an unzipped pack directory.
- **Install from zip** — pick a pack zip.
- **Download URL** — paste a URL, optionally with a SHA-256 checksum; the
  download is verified against it before installing.

Installs take effect immediately — the language appears in the dropdown
with no restart. The low-tech route also works: **Open packs folder**
reveals the data directory's `packs/<code>/`, and a pack directory dropped
there by hand is picked up on the next start.

### Removing a pack — and your data

**Remove** deletes the pack's files from the data directory, nothing more:
your library, vocabulary, and review history for that language stay in the
database and come back if the pack is reinstalled. The confirmation dialog
says exactly this. The active language cannot be removed — switch to
another one first.

## Switching from the home page

The app opens on a home view whose header carries a **Language** dropdown
and a **Manage languages** shortcut to the settings page, so switching
never requires a trip through settings. Everything on the page follows the
switch: cards due today (counted by your local midnight, with a time
estimate from your measured review pace), the continue-reading card
(progress, estimated time left at your reading speed, unknown words ahead,
difficulty verdict), and the reading-activity calendar.

## Koine Greek: the corpus-first pack

The first pack language is Koine Greek, and it takes the opposite approach
to a runtime analyzer: the texts come **pre-annotated**. The pack's texts
are converted from MorphGNT, so every token arrives carrying a
hand-verified lemma, parse, and gloss — better than any analyzer could do,
because humans checked each word. For the reader this means:

- **Interlinear glosses** — the furigana slot doubles as a gloss layer:
  each word's English gloss is drawn over it, governed by the same fade
  modes as furigana (unknown-only, first-X occurrences per book).
- **Parse decoded to prose** — clicking a word shows the parse of *that
  occurrence*, decoded: "verb · imperfect active indicative · 3rd person
  singular", not a cryptic tag.
- **Exact statistics** — the pack grades against GNT frequency tiers
  (Core 50×+, 30×+, 20×+, 10×+, 5×+), which track the classic
  read-the-GNT vocabulary curricula. The Greek New Testament is a closed
  corpus, so coverage numbers are exact, not estimates.
- **Forgiving search** — dictionary lookup ignores accents, breathings,
  and case, and the search box additionally accepts betacode and Greeklish:
  *logos* or *lo/gos* finds λόγος. See
  [Dictionary & Kanji](@/docs/dictionary-and-kanji.md) for the details.

Polytonic Greek currently renders through a wide-coverage system font;
packs can declare fonts in their manifest, but per-pack font downloads are
not wired up yet.

The Greek pack is compiled by `shiori-packc` from MorphGNT and installs
like any other pack — folder, zip, or URL. (The Build-from-Wiktionary list
below also offers *Ancient Greek*: that is a different, dictionary-driven
pack built from Wiktionary data, without the pre-annotated GNT corpus.
Both packs share the language code `grc`, so only one can be installed at
a time — installing either replaces the other, while your library and
review history survive the swap.)

## Build from Wiktionary

**Settings → Languages → Build from Wiktionary** generalizes the Japanese
first-run model to ~19 languages: Czech, Danish, Dutch, Finnish, French,
German, Hungarian, Indonesian, Italian, Korean, Latin, Polish, Portuguese,
Romanian, Russian, Spanish, Swedish, Turkish, and Ancient Greek. Pick one,
click **Build**, and the app downloads public data from its stable upstream
URLs and compiles the pack locally — nothing is hosted or maintained by
Shiori.

Two things are downloaded:

- **kaikki.org's Wiktextract dump** for the language (CC BY-SA 4.0 & GFDL)
  — the dictionary and the full inflection tables.
- **hermitdave's FrequencyWords list** (CC BY-SA 4.0) — subtitle-corpus
  frequency ranks, where a list exists. Latin and Ancient Greek have none,
  and the row says so; the pack still builds, just without ranks.

Each row shows the approximate download size — dumps are large, hundreds
of MB up to ~1 GB. The download streams to disk with progress, an
interrupted transfer **resumes** with an HTTP range request instead of
restarting, and a failed build keeps the downloaded file so a retry
doesn't re-fetch; once the pack installs, the gigabyte-class dump is
deleted to reclaim the space. Already-installed languages show a
**Rebuild** button instead.

What the built pack includes:

- **Inflection tables as grammar** — Wiktionary's per-word form tables are
  inverted into the full-form lookup table, so every conjugated or
  declined form resolves to its lemma, and each form's tags decode to
  prose in the reader ("hablaba → hablar · first person · singular ·
  imperfect").
- **Register labels** — senses keep their labels (colloquial, archaic,
  vulgar, …), mapped onto the usage-register display.
- **Usage examples** attached to senses.
- **IPA pronunciation** — shown with dictionary entries only when
  Settings → Reading → "Show IPA with dictionary entries" is enabled
  (off by default).
- **Lemmatized frequency and graded tiers** — each surface form's subtitle
  mass folds onto its lemma, so a verb is ranked by all its conjugations,
  and Top 500 / 1k / 2k / 5k tiers light up the
  [Statistics](@/docs/statistics.md) page's level section.

The list covers whitespace-tokenized scripts with rich Wiktionary
inflection data. No-whitespace scripts (Chinese) need a segmentation
engine — a Shiori release, not a pack.

## How analysis works without an engine

Pack languages have no compiled analyzer, and mostly don't need one:

- **Pre-annotated texts** carry their analysis with them — nothing runs at
  all.
- **Plain-text imports and chat** resolve each token through the pack's
  full-form table: unambiguous forms get their lemma and parse directly.
- **Ambiguous forms** resolve by corpus frequency when one candidate
  clearly outranks the rest — and the reader's word panel shows an
  "Ambiguous form" box listing every candidate with its decoded parse, so
  one click re-points that single occurrence. The manual override always
  beats the frequency vote.
- **Contractions are pack data** — *au*, *im*, *della* stay one token but
  count as function words, and the panel shows the expansion (*au* = *à* +
  *le*) with each component clickable. French and Italian elision is
  handled the same way: *l'eau* tokenizes as *l'* + *eau*, while
  *aujourd'hui* stays whole.
- **Germanic compounds** split against the pack's own dictionary when the
  whole word has no entry — *Arbeitsmaschine* → *arbeit* + *maschine*,
  linking elements understood — with each part clickable.
- **Forms missing from the table** try suffix rules the pack builder
  learned from its own data, accepted only when the dictionary confirms
  the guessed lemma. Anything still unresolved keeps the surface form as
  its own lemma — safe, never wrong.

The same machinery powers dictionary search: an inflected query like
*suis* resolves to every candidate lemma (*être* and *suivre*), and
results build in tiers of match closeness with frequency ordering inside
each tier — see [Dictionary & Kanji](@/docs/dictionary-and-kanji.md).

## Practice in a pack language

Conversation practice follows the active language, with the persona
defined by the pack. A dead language makes no pretense of native speakers:
the Koine Greek pack discloses the synthetic persona up front and judges
your writing against *attested usage* in the period's texts rather than
native intuition. Composition exercises and translation drills over
sentences from your own reading work in every language.

Because a local model that handles Japanese fine may write terrible Koine,
each language can pin its own LLM model: with a pack language active,
Settings → AI shows a **Model override** field (blank means the provider's
default). See [AI & Chat](@/docs/ai-and-chat.md).

## The hosted catalog

A browse-and-install catalog — an offline-cached `catalog.json` with
one-click SHA-256-verified installs, generated by `shiori-packc catalog` —
is fully plumbed and tested, but no catalog is published yet, so the
Languages page shows no browse section. Build-from-Wiktionary covers
discovery for modern languages meanwhile, and install-from-URL handles
anything hosted elsewhere. When a catalog is published, the browse UI will
appear against the machinery that already exists.

## Where pack data lives

Installed packs live in the data directory under `packs/<code>/`, and
Build-from-Wiktionary caches its downloads under `web-sources/` until the
build succeeds. The database scopes everything by language — see
[Data & Interop](@/docs/data-and-interop.md) for the full data-directory
layout and what survives a pack removal.
