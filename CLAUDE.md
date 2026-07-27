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

### Website

The marketing + docs site is a Zola project under `site/`. Building it
regenerates `site/public/` (GENERATED, gitignored). Needs Zola installed
(CI pins v0.22.1 — https://www.getzola.org).

```sh
cd site
zola build          # -> site/public/   (use `zola serve` for a live preview)
```

Deployment is automatic: pushing to `master` triggers
`.github/workflows/site.yml`, which builds and publishes to GitHub Pages.
Don't hand-edit `site/public/`.

### Desktop Applications

The `shiori` binary is the Cargo workspace under `software/` (crate
`shiori-gui`). A local build:

```sh
cd software
cargo build --release -p shiori-gui   # -> software/target/release/shiori[.exe]
cargo run   --release -p shiori-gui   # build and launch
```

The first build downloads and embeds the IPADIC morphological dictionary
(needs network, once).

The shipping per-platform artifacts (the `.zip`/`.tar.gz` bundling the
binary with the licenses, README, and CHANGELOG) are built and attached to
a GitHub Release by `.github/workflows/release.yml` on any `v*` tag push.
To reproduce a bundle locally, run the packaging script for your platform
(each builds the release binary, then archives it exactly as CI does):

```sh
scripts/package-desktop.sh [VERSION]     # Linux/macOS -> shiori-<ver>-<plat>.tar.gz
```
```powershell
scripts/package-desktop.ps1 [-Version <ver>]   # Windows -> shiori-<ver>-windows-x86_64.zip
```