+++
title = "Dictionary & Kanji"
weight = 4
+++

The Dictionary & kanji view is a single search box that answers with both
JMdict word entries and kanji reference cards, including stroke-order
diagrams. It is also where you can add a word to spaced repetition without
having met it in a book.

## Searching

Type Japanese text into the search box — kanji, kana, or any word form
(猫, ねこ, 食べる). Results update as you type; the ✕ button clears the
query and refocuses the box.

Matching runs over the dictionary's form index: a form matches when it
equals the query exactly or starts with it (prefix matching), so ねこ also
surfaces ねこじた and friends. Up to 30 word entries are returned per
query.

If the reference data was never downloaded (you chose "Continue without
dictionary" at first run), the view shows a no-dictionary notice instead of
results; retry the download from the banner. See
[Getting-Started](@/docs/getting-started.md).

## Word entries

The left column lists word entries. Each shows:

- the headword, with its kana reading in parentheses when they differ,
- a status chip (unknown / learning / known / ignored) if you have already
  met the word while reading,
- the first three senses, with their English glosses.

### Learn (SRS) from search

Each entry carries a **➕ Learn (SRS)** button (hidden when the word is
already in `learning` status). Clicking it:

1. Runs the headword through the morphological analyzer to derive the
   canonical word key (lemma, reading, part of speech) — the same key the
   reader uses, so the search hit and the word you later meet in a book are
   one record, not duplicates. If analysis yields nothing, the headword
   itself is used as a noun.
2. Creates the word record if it has never been seen.
3. Starts a review card **without a sentence context**. Cards created while
   reading show the sentence the word came from; cards created from search
   have none until you meet the word in a book. See
   [Reviews-and-SRS](@/docs/reviews-and-srs.md).

## Kanji cards

The right column shows up to six kanji cards. They are chosen from the
kanji characters in your query, in order; if the query contains no kanji
(a kana search), the cards come from the headwords of the top three word
hits instead — so searching ねこ still produces the card for 猫.

Each card contains:

| Field | Meaning |
|-------|---------|
| 音 (on) readings | Sino-Japanese readings, in katakana |
| 訓 (kun) readings | Native readings; a `.` marks where okurigana begins (つ.ぐ) |
| 名乗り (nanori) | Readings used only in names |
| Meanings | English meanings from KANJIDIC2 |
| Stroke count | Accepted count (KANJIDIC2's first value; later values are common miscounts) |
| Grade | Kyōiku grade 1–6, or jōyō (secondary school), or jinmeiyō (name kanji) |
| Old JLPT | Pre-2010 JLPT level 1–4, where listed |
| Frequency | Newspaper frequency rank 1–2500, where listed |
| Variant/archaic forms | Cross-referenced character variants (e.g. 亜 ↔ 亞) |

Grade, JLPT, frequency, and variants only appear when KANJIDIC2 records
them for that character.

### Stroke-order diagrams

When KanjiVG has data for the character, the card draws a numbered
stroke-order diagram: each stroke is rendered from its SVG path, numbered
at its starting point, and colored on a gradient from accent blue (first
stroke) to gray (last stroke) so the order is readable at a glance.

KanjiVG covers roughly half of KANJIDIC2's characters. For kanji it does
not cover — mostly rare and archaic ones — the card shows the character
large instead of a diagram; everything else on the card is unaffected.

## Kanji chips in the reader

You do not have to leave the book to look up a kanji. In the reader's word
panel, each kanji of the selected headword appears as a chip under the
headword; clicking a chip expands the same kanji card inline, diagram
included. See [Reading](@/docs/reading.md).

## Data sources and licenses

The kanji data is downloaded once at first run into the app's data
directory, alongside JMdict, and imported into the local database; after
that, lookups are fully offline.

| Source | Provides | License |
|--------|----------|---------|
| [KANJIDIC2](https://www.edrdg.org/wiki/index.php/KANJIDIC_Project) | Readings, meanings, grades, JLPT, frequency, variants | © EDRDG, [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) |
| [KanjiVG](http://kanjivg.tagaini.net) | Per-stroke SVG path data in stroke order | © Ulrich Apel, [CC BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/) |

KANJIDIC2 is fetched from EDRDG's canonical URL (the file is regenerated
daily; no pinned version exists). KanjiVG is pinned to an immutable GitHub
release tag. Word entries come from JMdict via the jmdict-simplified
project — see [Getting-Started](@/docs/getting-started.md) for the full
reference-data list.
