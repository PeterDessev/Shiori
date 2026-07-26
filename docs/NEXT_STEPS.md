# Next Steps

The multilingual architecture shipped in 0.2.0 (2026-07-19); the tasks
below are the follow-up work, ordered so each unblocks the next. See
`docs/wiki/Language-Packs.md` for context.

## P0 — Prove it on real data

- [ ] Run `shiori-packc build-grc` against a real MorphGNT SBLGNT
      checkout + a Dodson `lemma→gloss` TSV. Fix whatever the real 140k
      tokens surface that the fixtures didn't (rejoin edge cases, fold
      gaps, unmapped parse codes). Output → `packs/grc/`.
- [ ] **Drive the app end-to-end** (never done yet): fresh data dir →
      pick Koine → open SBLGNT John → click a word, confirm parse +
      glosses render → mine a word → review the card → check GNT-tier
      coverage stats → switch back to Japanese and confirm it's
      unchanged. Expect first-run layout/wiring bugs.

## P1 — Make Greek render and install anywhere

- [ ] Wire the pack font download: `download_font` only handles bare
      `.ttf`; the Gentium URL is a zip. Add zip extraction + read the
      manifest `[[fonts]]` list + install ahead of the system fallback.
      (Greek renders today only via a Greek-covering system font.)
- [ ] (Deferred by choice — build-from-Wiktionary covers discovery.)
      Publish a pack catalog + onboarding language picker. The whole
      client pipeline exists and is tested (offline-cached fetch,
      SHA-256-verified installs, `shiori-packc catalog` generator, GUI
      catalog state, the `pack_catalog_url` setting); the browse
      section UI itself still needs writing against that plumbing once
      a catalog is actually hosted (see the wiki's hosted-catalog
      section).

## P2 — Deepen coverage

- [ ] Full ~900k-form Greek `morph_forms` table via Morpheus (MPL-2.0)
      in Docker CI, merged with kaikki grc. Today the table covers only
      GNT-attested forms, so non-GNT Greek under-lemmatizes.
- [x] Candidate-parse picker UI for ambiguous Tier-1 forms (today
      resolved: ambiguous forms first try the corpus-frequency vote,
      then the reader lists every candidate analysis and one click
      re-points that occurrence).
- [ ] Add Swete LXX (Tier-1) and machine-tagged Apostolic Fathers
      texts; surface the stored `quality: gold|machine` flag in the
      reader.

## P3 — Finish the Japanese-coupling cleanup

- [x] Scope review-history stats by language: `due_forecast`,
      `retention_counts`, `learning_starts_by_day`, `matured_by_day`,
      due/card/review counts, and reading time/velocity now join on the
      words/documents language. Only the seconds-per-card pace estimate
      stays global on purpose (pace is a trait of the user).
- [ ] Anki export: map the parse code + citation into card fields for
      pack languages (guids are already namespaced; fields are still the
      four Japanese-shaped ones).

## P4 — Second-language-family engines (each needs a release, not a pack)

- [ ] Validate SIAT's reserved fields (`sub`, `layers`, `dir`) against a
      Biblical Hebrew prototype (RTL + prefix/suffix decomposition)
      before freezing the format for community authors.
- [x] Modern-language packs without shipping anything: Settings →
      Languages → Build from Wiktionary downloads kaikki.org dumps +
      hermitdave frequency lists and builds the pack locally (~19
      languages, dictionary + inflection grammar + tag decoding).
      Remains: run one big build (es or fr) end-to-end on real data and
      fix what the full dump surfaces that the fixtures didn't.
