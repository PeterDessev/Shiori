# shiori

A desktop Japanese-reading companion (Rust/egui): import a book, read it,
click unknown words for dictionary/conjugation help, and drop them into
spaced repetition anchored to the sentence — producing cross-platform
`shiori` binaries built from the Cargo workspace under `software/`.
Status: active. Current milestone: adoption.

## Index

| Path | Role | Purpose | Notes |
|---|---|---|---|
| docs/ | SOURCE | documentation |  |
| software/ | SOURCE | host / pipeline code |  |

## State

- 2026-07-26: adopted into the Project Organization Standard v1.1 by keel — detected Rust (Cargo.toml); code under software/.
- (rotate entries older than the newest ~10 into docs/log.md)

## Conventions

- Naming: lowercase, hyphen-separated, ASCII, no spaces; dated artifacts use a YYYY-MM-DD prefix.
- Units: SI unless a file header states otherwise; timestamps UTC.
- Commits: Conventional Commits — `<type>[(scope)][!]: <description>` with types feat, fix, docs, refactor, test, chore, build, ci; cite REQ-/ADR- IDs in the footer where applicable.
- Layout follows the Project Organization Standard v1.1; check with `keel validate .`.

## Guardrails

- Never edit any GENERATED path; regenerate via the commands below.
- Ask before deleting anything outside build/.
- Never read or write credential material (.env, key files).

## Regeneration

No generated artifact classes yet.
