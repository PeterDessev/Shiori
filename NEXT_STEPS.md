# Next Steps

The multilingual architecture is code-complete and unit-tested, but no
real pack has been built or run in the GUI. Tasks below are ordered:
each unblocks the next. See `docs/wiki/Language-Packs.md` for context.

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
- [ ] Hosted packs + onboarding language picker: one hash-verified zip
      per language, downloaded on first use, instead of folder-drop.

## P2 — Deepen coverage

- [ ] Full ~900k-form Greek `morph_forms` table via Morpheus (MPL-2.0)
      in Docker CI, merged with kaikki grc. Today the table covers only
      GNT-attested forms, so non-GNT Greek under-lemmatizes.
- [ ] Candidate-parse picker UI for ambiguous Tier-1 forms (today
      ambiguity safely keeps the surface as lemma — no user resolution).
- [ ] Add Swete LXX (Tier-1) and machine-tagged Apostolic Fathers
      texts; surface the stored `quality: gold|machine` flag in the
      reader.

## P3 — Finish the Japanese-coupling cleanup

- [ ] Scope review-history stats by language: `due_forecast`,
      `retention_counts`, `learning_starts_by_day`, `matured_by_day`
      still query `review_log`/`cards` globally, mixing languages for a
      multi-language user (single-language users unaffected).
- [ ] Anki export: map the parse code + citation into card fields for
      pack languages (guids are already namespaced; fields are still the
      four Japanese-shaped ones).

## P4 — Second-language-family engines (each needs a release, not a pack)

- [ ] Validate SIAT's reserved fields (`sub`, `layers`, `dir`) against a
      Biblical Hebrew prototype (RTL + prefix/suffix decomposition)
      before freezing the format for community authors.
- [ ] Ship a modern-language pack (es/fr/de) from `build-kaikki` as the
      reference community pack.
