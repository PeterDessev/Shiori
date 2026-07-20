# Contributing to Shiori

Thanks for your interest. Shiori is a small, focused codebase; the rules
below exist to keep it that way.

## Development setup

- Rust **1.88 or newer** (`rustup` recommended). The workspace pins
  `rusqlite 0.37` specifically to stay compatible with 1.88.
- The **first build needs network access**: lindera downloads and embeds the
  IPADIC morphological dictionary at build time. It takes a few minutes,
  once. The embedded dictionary sits behind shiori-nlp's default-on
  `embed-ipadic` cargo feature, so pack-only work can skip the lindera
  build entirely.
- Windows is the primary platform. The GUI builds elsewhere but is not
  routinely tested there.
- `.cargo/config.toml` sets `-C target-feature=+crt-static` for
  `x86_64-pc-windows-msvc`, so every local and CI build statically links
  the MSVC C runtime — this is why the shipped exe runs without the VC++
  Redistributable.

```sh
cargo build --release
cargo run --release -p shiori-gui
```

The app keeps its data in `%APPDATA%\shiori` (database, downloaded
dictionaries, fonts, archived book copies, installed language packs under
`packs/<code>/`, and the cached pack catalog). Delete that directory for a
factory reset; take a copy of `jrc.sqlite3` to back up your learning state,
or use Settings → Data inside the app.

## Before you push

CI enforces all three; run them locally first:

```sh
cargo fmt --check                                      # formatting
cargo clippy --workspace --all-targets -- -D warnings  # lints
cargo test --workspace                                 # tests
```

New behavior needs tests. The pattern throughout the codebase is unit tests
next to the code plus the end-to-end pipeline test in
`crates/shiori-app/tests/pipeline.rs` — extend whichever fits.

## Commit style: atomic conventional commits

Every commit is **one logical change** with a
[Conventional Commits](https://www.conventionalcommits.org) message:

```
<type>(<scope>): <imperative subject, ≤72 chars>

Body: what the change does and *why* — constraints, trade-offs, and
anything the diff alone doesn't say.
```

- **Types**: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`,
  `ci`.
- **Scope** is the crate short name (`gui`, `app`, `db`, `dict`, `lang`,
  `llm`, `nlp`, `pack`, `packc`, `srs`, `core`) or a comma list when a
  feature genuinely spans layers (`feat(app,gui): …`). Omit it for
  workspace-wide changes.
- **Atomic** means the commit builds and tests green on its own, and its
  subject doesn't need the word "and" for unrelated things. Schema changes,
  the feature using them, and drive-by cleanups are separate commits.

Examples from the actual history:

```
feat(gui): furigana modes with per-book instance anchoring
fix(gui): refresh library progress on navigation; persist position on exit
refactor: appease clippy across the workspace
docs: add feature roadmap as plan of record
```

## Pull requests

- Keep PRs reviewable: one feature or fix per PR, commits staying atomic
  (no "fixup"/"wip" commits — rebase before opening).
- UI changes include a screenshot or short capture.
- If your change affects behavior described in the user guide
  (`site/content/docs/`) or in `docs/wiki/Language-Packs.md`, update the
  page in the same PR.

## Documentation layout

- `README.md` — the front door.
- `CHANGELOG.md` — Keep a Changelog format; the release workflow derives
  release notes from it.
- `ROADMAP.md` — contributor-facing tracker of planned, in-progress, and
  completed work, with per-entry issue links.
- `NEXT_STEPS.md` — the ordered follow-up list for the multilingual work.
- `site/` — the website and user guide (Zola; content in `site/content`,
  deployed to <https://PeterDessev.github.io/Shiori> by
  `.github/workflows/site.yml`).
- `docs/wiki/Language-Packs.md` — the in-repo contributor doc on the pack
  system.

## Data licenses

Shiori ships no dictionary data; it downloads JMdict and KANJIDIC2 (© EDRDG,
CC BY-SA), KanjiVG (© Ulrich Apel, CC BY-SA 3.0), community JLPT lists
(CC BY-SA 4.0), a Leeds-corpus-derived frequency list (CC BY), and the
Noto Sans JP / Noto Serif JP fonts (SIL Open Font License 1.1) at first
run. Build-from-Wiktionary downloads kaikki.org's Wiktextract data
(CC BY-SA 4.0 & GFDL) and hermitdave's FrequencyWords lists (CC BY-SA 4.0);
the `shiori-packc` Koine Greek pack builds from MorphGNT. Installed packs
state their own licenses, shown in About and on the Languages page.
Changes that redistribute or transform any of this data must keep the
attributions in the app's Settings → General → About section and the
README intact.

## License

By contributing you agree your work is dual-licensed under MIT and
Apache-2.0, matching the project.
