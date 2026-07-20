+++
title = "Dictionary & Kanji"
weight = 4
+++

The Dictionary & kanji view is a single search box over the active
language's dictionary. For Japanese it answers with both JMdict word
entries and kanji reference cards, including animated stroke-order
diagrams; in a pack language it searches the pack's dictionary. It is also
where you can add a word to spaced repetition without having met it in a
book.

## Searching

For Japanese, type kanji, kana, or rōmaji — 猫, ねこ, and neko all reach
the same entry (rōmaji is transliterated as you type, so tabemashita works
too). A conjugated query like 食べました resolves to its dictionary root,
and a banner above the results explains the form: the typed form, its
root, what kind of word it is, and the grammar of its tail. Results update
as you type; the ✕ button clears the query and refocuses the box.

In a pack language the lookup is folded — case and accents are ignored —
and an inflected query resolves through the pack's grammar table to every
candidate lemma: *suis* surfaces both *être* and *suivre*. Forms the table
doesn't list fall back to suffix rules the builder learned from its own
data.

Koine Greek additionally accepts betacode/Greeklish in the box: *logos*
finds λόγος. Betacode letter values (h = η, q = θ, w = ω, x = χ, c = ξ,
y = ψ, f = φ) and the common digraphs (th, ph, ch, ps) both work, and
betacode diacritic marks are accepted and dropped — matching is
accent-insensitive either way.

Results build in tiers of match closeness: first the word the query is a
*form of*, then exact matches, then prefix matches (so ねこ still surfaces
ねこじた and friends; the prefix tier is capped at 30 entries). Corpus
frequency orders the words within each tier — the everyday *être* beats
the archaic *estre* — and a prefix match never outranks an exact one.

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

Packs built from Wiktionary carry IPA pronunciation: with
Settings → Reading → "Show IPA with dictionary entries" enabled (off by
default), it appears where the kana reading would for Japanese.

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

Character cards exist only for Japanese: in a pack language no cards
appear, and the word list takes the full width of the view.

For Japanese, the right column shows up to six kanji cards. They are
chosen from the kanji characters in your query, in order; if the query
contains no kanji (a kana search), the cards come from the headwords of
the top three word hits instead — so searching ねこ still produces the
card for 猫.

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

When KanjiVG has data for the character, the card animates the stroke
order: the character draws itself one stroke at a time — the stroke being
traced highlighted in accent blue with a pen tip, the not-yet-drawn
strokes ghosted so the glyph keeps its shape — and loops when finished.
Scrolling over the character scrubs it stroke by stroke (one wheel notch
is one stroke; scrolling down advances) and briefly pauses the auto-play
while you scrub.

KanjiVG covers roughly half of KANJIDIC2's characters. For kanji it does
not cover — mostly rare and archaic ones — the card shows the character
large instead of a diagram; everything else on the card is unaffected.

## Kanji chips in the reader

In the reader's word panel, each kanji of the selected headword appears as
a chip under the headword; clicking a chip leaves the book and opens this
view with that character as the query, card and animation included.
Leaving the reader this way credits the partially read page like a pause.
See [Reading](@/docs/reading.md).

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

Pack dictionaries ship their own data and licenses: the Koine Greek pack
is built from MorphGNT, and packs built from Wiktionary use kaikki.org's
Wiktextract data (CC BY-SA 4.0 & GFDL) and hermitdave's FrequencyWords
lists (CC BY-SA 4.0). Every attribution — plus each installed pack's
license line — is consolidated in Settings → General → About.
