# Language Packs

Shiori grew up Japanese-only; since the multilingual expansion it reads
**any language a pack provides**, with dead languages as first-class
citizens. Japanese stays compiled in (its analyzer is real code);
every other language is *data*: a directory of files the app loads at
runtime, no recompile needed.

## Using a pack

1. Drop the pack directory into `<data>/packs/<code>/` (e.g.
   `packs/grc/`) and restart Shiori.
2. Pick the language under **Settings → General → Active language**.
   The pack's reference data imports on first activation, scoped so it
   can never touch another language's data.
3. Everything follows the switch: library, reader, dictionary, mining,
   reviews, statistics, and conversation practice all operate in the
   active language. Nothing mixes — a Greek λόγος and a Spanish `sol`
   can never collide with Japanese words, and difficulty statistics
   never average across languages.

### What changes in the app, per language

- **Reader** — texts imported from pre-annotated files carry a
  hand-verified lemma, parse, and gloss on every token. The furigana
  slot becomes an interlinear **gloss layer** (same fade modes:
  unknown-only, first-X occurrences). Clicking a word shows the parse
  of *that occurrence* decoded to prose ("verb · imperfect active
  indicative · 3rd person singular").
- **Dictionary** — search is accent/breathing-insensitive; Greek
  additionally accepts betacode and Greeklish (`logos`, `lo/gos`,
  `qeos`). The kanji panel is a Japanese capability; packs bring their
  own reference panels instead.
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
  lemma and parse; ambiguous or unknown forms keep the surface as lemma
  (safe, never wrong).
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

### License policy

packc refuses NonCommercial sources outright (machine-enforced). Known
safe for Koine Greek: SBLGNT (CC BY 4.0) + MorphGNT annotations
(CC BY-SA), Nestle 1904 (PD text, CC0 morphology), Byzantine RP2018
(PD, with parsing), TBESG (CC BY 4.0), Dodson and Abbott-Smith (PD),
LSJ from PerseusDL/lexica (CC BY-SA — **not** the NC gcelano Unicode
conversion). Known **unusable**: PROIEL, CATSS, OpenGNT, CEFRLex, the
Cologne scans. Wiktionary/kaikki data is CC BY-SA + GFDL — fine with
attribution and share-alike on the data.

## Roadmap for packs

- Hosted pack downloads (one hash-verified zip per language) and an
  onboarding language picker — today packs install by folder drop.
- The full ~900k-form Greek table from Morpheus (regenerated under
  MPL-2.0 in Docker CI) merged with the kaikki grc extract; today the
  full-form table covers every form attested in the GNT.
- A candidate-parse picker for ambiguous Tier-1 forms (today ambiguity
  safely keeps the surface as lemma).
- Swete LXX and machine-tagged Apostolic Fathers texts, marked
  `quality:"machine"` in the reader.
- Per-pack font downloads (Gentium Plus, OFL); today a wide-coverage
  system font (Segoe UI on Windows) carries polytonic Greek.
