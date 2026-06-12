# Contributing to Shiori

Thanks for your interest. Shiori is a small, focused codebase; the rules
below exist to keep it that way.

## Development setup

- Rust **1.88 or newer** (`rustup` recommended). The workspace pins
  `rusqlite 0.37` specifically to stay compatible with 1.88.
- The **first build needs network access**: lindera downloads and embeds the
  IPADIC morphological dictionary at build time. It takes a few minutes,
  once.
- Windows is the primary platform. The GUI builds elsewhere but is not
  routinely tested there.

```sh
cargo build --release
cargo run --release -p shiori-gui
```

The app keeps its data in `%APPDATA%\shiori` (database, downloaded
dictionaries, fonts, archived book copies). Delete that directory for a
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
- **Scope** is the crate short name (`gui`, `app`, `db`, `dict`, `llm`,
  `nlp`, `srs`, `core`) or a comma list when a feature genuinely spans
  layers (`feat(app,gui): …`). Omit it for workspace-wide changes.
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
- If your change affects behavior described in the wiki (`docs/wiki/`),
  update the page in the same PR.

## Documentation layout

- `README.md` — the front door.
- `ROADMAP.md` — design record of shipped features plus the deferred list.
- `docs/wiki/` — the user guide and contributor docs. These pages mirror the
  GitHub wiki; after changes land on the default branch, sync them to the
  wiki with:

  ```sh
  git clone https://github.com/OWNER/REPO.wiki.git
  cp docs/wiki/*.md REPO.wiki/ && cd REPO.wiki
  git add -A && git commit -m "docs: sync wiki from docs/wiki" && git push
  ```

## Data licenses

Shiori ships no dictionary data; it downloads JMdict and KANJIDIC2 (© EDRDG,
CC BY-SA), KanjiVG (© Ulrich Apel, CC BY-SA 3.0), community JLPT lists
(CC BY-SA 4.0), and a Leeds-corpus-derived frequency list (CC BY) at first
run. Changes that redistribute or transform that data must keep the
attributions in the app's Settings → Data page and the README intact.

## License

By contributing you agree your work is dual-licensed under MIT and
Apache-2.0, matching the project.
