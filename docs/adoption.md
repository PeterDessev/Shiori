# Adopting shiori into the standard

keel scaffolded a compliant skeleton and moved what it could classify.
Finish the remaining steps by hand (standard §12):

1. [x] `CLAUDE.md` created — fill in the Identity purpose line and the
newest State entry.
2. [x] Sort `_triage/` into canonical directories. (nothing was quarantined)
3. [ ] Declare roles: move every generated file under `exports/` or
`build/`, and add a `MANIFEST.md` to any directory holding opaque
files (binaries, archives).
4. [ ] Lock inputs: raw data under `data/raw/`, vendored code under
`third_party/`, both IMMUTABLE.
5. [ ] Write companion notes for the three most-consulted PDFs under
`docs/datasheets/`.
6. [ ] Add a Regeneration row to `CLAUDE.md` for every generated
artifact class, and verify each command runs.
7. [ ] Create `scripts/` entry points for the routine operations you
did by hand this month.
8. [ ] Empty `_triage/`, run `keel validate .`, then commit.

Run `keel validate .` at any point to see what still deviates.
