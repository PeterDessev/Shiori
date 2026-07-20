# Language Packs

Shiori grew up Japanese-only; since the multilingual expansion it reads
**any language a pack provides**, with dead languages as first-class
citizens. Japanese stays compiled in (its analyzer is real code);
every other language is *data*: a directory of files the app loads at
runtime, no recompile needed.

## Using a pack

1. Install it under **Settings → Languages**: build one from public
   web sources (see below), pick a pack folder or zip, or paste a
   download URL (optionally with a SHA-256 checksum the download is
   verified against). The language appears immediately — no restart.
   Dropping a pack directory into `<data>/packs/<code>/` by hand and
   restarting works too.
2. Activate the language on the same page (or switch from the home
   page). The pack's reference data imports on first activation, scoped
   so it can never touch another language's data; bundled pre-annotated
   texts import into the library with one click.
3. Everything follows the switch: library, reader, dictionary, mining,
   reviews, statistics, and conversation practice all operate in the
   active language. Nothing mixes — a Greek λόγος and a Spanish `sol`
   can never collide with Japanese words, and difficulty statistics
   never average across languages.

Removing a pack (Settings → Languages) deletes its files but keeps the
language's library, vocabulary, and review history in the database;
everything comes back if the pack is reinstalled.

### What changes in the app, per language

- **Reader** — texts imported from pre-annotated files carry a
  hand-verified lemma, parse, and gloss on every token. The furigana
  slot becomes an interlinear **gloss layer** (same fade modes:
  unknown-only, first-X occurrences). Clicking a word shows the parse
  of *that occurrence* decoded to prose ("verb · imperfect active
  indicative · 3rd person singular").
- **Dictionary** — search is accent/breathing-insensitive; Greek
  additionally accepts betacode and Greeklish (`logos`, `lo/gos`,
  `qeos`). An inflected query resolves through the grammar table to
  every candidate lemma — *suis* surfaces both *être* and *suivre* —
  and falls back to the learned suffix rules for forms the table
  doesn't list. Results build in tiers of match closeness (the word
  the query is a form of, then exact matches, then prefix matches),
  with corpus frequency ordering words within each tier. The kanji
  panel is a Japanese capability; pack languages get the full width
  for word results instead.
- **Statistics** — the JLPT section generalizes: each pack declares its
  own level scheme (Koine Greek grades against GNT frequency tiers —
  a *closed corpus*, so coverage numbers are exact).
- **Conversation practice** — the persona comes from the pack. Dead
  languages disclose the synthetic persona and judge "naturalness"
  against attested usage rather than native intuition. Composition
  exercises and translation drills (round-tripping sentences from your
  own reading) ride the same corrected-chat pipeline. Each language
  can pin its own LLM model (Settings → AI): local models fine for
  Japanese are usually hopeless at Koine.

## Anatomy of a pack

```
packs/grc/
├── manifest.toml       # identity, script, segmentation, prompts, licenses
├── dictionary.jsonl    # entries + pre-folded lookup forms
├── morph_forms.tsv     # full-form table: folded form → lemma + parse
├── frequency.tsv       # folded form → corpus rank
├── tags.tsv            # parse-code segment → human label
├── graded.tsv          # level ordinal, label, form  (level scheme)
├── suffix_rules.tsv    # learned "-o → -ar" lemma-guess rewrites
└── texts/*.siat.jsonl  # pre-annotated texts (the primary reading path)
```

Analysis runs in **tiers**, declared by what the pack ships:

- **Tier 0 — pre-annotated texts (SIAT).** The unit of import for dead
  languages. Every token arrives carrying lemma + parse + gloss, so no
  analyzer runs at all; parse quality is whatever the annotators
  achieved (for the Greek New Testament: hand-verified, better than any
  runtime analyzer).
- **Tier 1 — full-form lookup.** Plain-text imports and chat messages
  resolve tokens through `morph_forms.tsv`: unambiguous forms get their
  lemma and parse. Ambiguous forms resolve by corpus frequency when
  one candidate clearly outranks the rest, and the reader's candidate
  picker lists the alternatives for a one-click per-occurrence fix;
  forms missing from the table try learned suffix rules (validated
  against the dictionary); anything still unresolved keeps the surface
  as lemma (safe, never wrong).
- **Tier 2 — compiled analyzers.** Japanese/Lindera only. New engines
  (RTL layout, no-whitespace segmentation, sandhi) require a Shiori
  release; new *languages within existing engines* need only a pack.

## SIAT — Shiori Annotated Text (v1, draft)

JSONL: one header line, one line per sentence. Byte offsets must tile
the sentence text exactly (validated at parse time).

```json
{"siat":1,"lang":"grc","title":"ΚΑΤΑ ΙΩΑΝΝΗΝ","license":"CC BY 4.0","quality":"gold","citation_scheme":"book.chapter.verse"}
{"p":0,"ref":"John.1.1","text":"Ἐν ἀρχῇ ἦν ὁ λόγος","tokens":[{"s":"Ἐν","l":"ἐν","m":"P","g":"in","start":0,"end":5}, …]}
```

- `quality` is `"gold"` (hand-verified) or `"machine"` — surfaced so
  machine parses are never mistaken for verified ones.
- Text is NFC-normalized; lookup keys are additionally folded
  (lowercase, accents/breathings/iota-subscript stripped, final sigma
  medialized).
- **Reserved fields** — parsed and ignored until an engine implements
  them, so packs can declare them today without a format break:
  per-token `sub` (sub-token lexical units: Hebrew prefixes, Latin
  enclitics), `layers` (toggleable diacritic layers: niqqud, macrons),
  header `dir: "rtl"`, structured `citation_scheme`.

The format stays **draft** until it has been validated against an RTL
and a sub-token prototype (Biblical Hebrew is the natural test); treat
it as stable for LTR, word-per-token languages.

## Building packs in the app: from Wiktionary, no hosting

**Settings → Languages → Build from Wiktionary** generalizes the
Japanese first-run model to ~19 languages: the app downloads public
data from its stable upstream URLs and compiles the pack locally —
there is no catalog, registry, or repository to maintain.

- **Dictionary and grammar** come from kaikki.org's per-language
  Wiktextract dumps (CC BY-SA 4.0 & GFDL). Wiktionary's inflection
  tables (the `forms` arrays) are inverted into the Tier-1 full-form
  table, so every conjugated or declined form resolves to its lemma,
  and each form's Wiktionary tags become a parse the reader decodes to
  prose ("hablaba → hablar · first person · singular · imperfect").
  The tag-decoding table is generated from the same data. Senses keep
  their register labels (colloquial, archaic, vulgar…) mapped onto the
  app's usage-register display, plus usage examples, and IPA
  pronunciation (shown only when enabled under Settings → Reading).
- **Frequency ranks** come from hermitdave's FrequencyWords
  OpenSubtitles lists (CC BY-SA 4.0), where one exists — *lemmatized*:
  each surface form's subtitle mass folds onto its lemma through the
  form table, so *hablar* is ranked by all its conjugations. Graded
  tiers (Top 500 / 1k / 2k / 5k lemmas) are derived from the same
  ranking, lighting up the statistics page's Level section.
- **Ambiguity** in plain-text analysis falls back to those ranks: a
  form with several candidate lemmas resolves to the clearly most
  frequent one; with no signal it safely stays as itself.
- **Elision** is language-aware: French and Italian packs declare their
  elidable words, so *l'eau* tokenizes as *l'* + *eau* (both real
  words) while *aujourd'hui* stays whole. **Contractions** (*au*,
  *im*, *della*) stay one token but count as function words, and the
  reader shows their expansion (*au* = *à* + *le*) with each component
  clickable. Germanic packs **split unknown compounds** against their
  own dictionary (*Arbeitsmaschine* → *arbeit* + *maschine*, linking
  elements included) for display and lookup.

Dumps are large (hundreds of MB up to ~1 GB); the download streams to
disk with progress, resumes interrupted transfers with HTTP ranges, is
kept for retry if a build fails, and is deleted once the pack installs.
The list covers whitespace-tokenized scripts with rich Wiktionary
inflection data; no-whitespace scripts (Chinese) need a segmentation
engine — a Shiori release, not a pack.

## Building packs: `shiori-packc`

The pack compiler is a CI/developer tool — never shipped in the app,
which only consumes finished packs.

```sh
# Koine Greek from a MorphGNT checkout + a lemma→gloss TSV (Dodson):
shiori-packc build-grc --morphgnt sblgnt/ --glosses dodson.tsv \
    --out packs/grc --license "CC BY-SA 4.0"

# A modern language from a kaikki.org Wiktextract dump (+ hermitdave
# OpenSubtitles frequency list):
shiori-packc build-kaikki --input kaikki-es.jsonl --lang es \
    --name Spanish --frequency es_50k.txt --out packs/es \
    --license "CC BY-SA 4.0"
```

`build-grc` converts the 7-column MorphGNT files to SIAT (positional
parse codes decompose into Robinson-style segments), derives corpus
frequency and GNT tiers by lemma, and emits the full-form table from
every attested form. `build-kaikki` inverts Wiktextract `forms` arrays
and `form_of` senses into the lemma table and keeps up to six glosses
per lemma.

### The hosted catalog (machinery ready, UI unwritten)

The full hosted-catalog pipeline exists and is tested — fetch with
offline caching, SHA-256-verified one-click installs, and the
`shiori-packc catalog` generator below — but no catalog is published
and Build-from-Wiktionary covers discovery, so the Languages page has
no browse section. The plumbing to write one against is all in place:
`fetch_pack_catalog` / `install_pack_from_url` and
`DEFAULT_PACK_CATALOG_URL` in `crates/shiori-app/src/packs.rs`, the
catalog state in the GUI, and the `pack_catalog_url` setting. The
browse section UI itself still needs writing; once a catalog is
hosted, point `DEFAULT_PACK_CATALOG_URL` at the published document.
Format:

```json
{
  "catalog": 1,
  "packs": [
    {
      "lang": "grc",
      "name": "Koine Greek",
      "description": "SBLGNT with MorphGNT annotations, Dodson glosses.",
      "license": "CC BY-SA 4.0",
      "url": "https://example.com/packs/grc.zip",
      "sha256": "…hex digest of the zip…",
      "size_bytes": 23456789,
      "version": "2026.07"
    }
  ]
}
```

`lang`, `name`, and `url` are required (entries missing them are
skipped); `sha256` should always be published — installs verify the
download against it. Entries claiming the built-in language are
ignored. `description` comes from the manifest's optional
`description` field.

Don't write the catalog by hand — generate it from finished packs:

```sh
shiori-packc catalog --packs packs/ --base-url https://example.com/packs \
    --out dist/ --version 2026.07
```

This zips every pack under `packs/` into `dist/<lang>.zip`, computes
each zip's real SHA-256 and size, and writes `dist/catalog.json` whose
entries point at `<base-url>/<lang>.zip`. The generated document is
re-parsed with the app's own catalog parser before it is written, so it
can never drift from what the app accepts; the NonCommercial license
gate applies to every pack it touches. Publish by uploading the whole
`dist/` directory to the base URL.

### License policy

packc refuses NonCommercial sources outright (machine-enforced). Known
safe for Koine Greek: SBLGNT (CC BY 4.0) + MorphGNT annotations
(CC BY-SA), Nestle 1904 (PD text, CC0 morphology), Byzantine RP2018
(PD, with parsing), TBESG (CC BY 4.0), Dodson and Abbott-Smith (PD),
LSJ from PerseusDL/lexica (CC BY-SA — **not** the NC gcelano Unicode
conversion). Known **unusable**: PROIEL, CATSS, OpenGNT, CEFRLex, the
Cologne scans. Wiktionary/kaikki data is CC BY-SA + GFDL — fine with
attribution and share-alike on the data.

The manifest's `license` string is user-visible: Settings → General →
About lists each installed pack's license alongside the
Wiktextract/FrequencyWords attributions, and the Languages page shows
it next to the pack.

## Roadmap for packs

- A hosted pack catalog and an onboarding language picker — the app
  already installs hash-verified zips from any URL (Settings →
  Languages); what's missing is a published registry to point it at.
- The full ~900k-form Greek table from Morpheus (regenerated under
  MPL-2.0 in Docker CI) merged with the kaikki grc extract; today the
  full-form table covers every form attested in the GNT.
- Swete LXX and machine-tagged Apostolic Fathers texts, marked
  `quality:"machine"` in the reader.
- Per-pack font downloads (Gentium Plus, OFL); today a wide-coverage
  system font (Segoe UI on Windows) carries polytonic Greek.
