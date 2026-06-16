<!--
--------------------------------------
           Shiori Roadmap
--------------------------------------
This file tracks planned, active, and completed work on Shiori.
It is intended for contributors and maintainers.

ENTRY FORMAT:

### Title of Work
- **Type:** Feature | Bug | Refactor | Documentation
- **Status:** Planned | In Progress | Done
- **Issue:** [#<number>](https://github.com/PeterDessev/shiori/issues/<number>)
- **Suggested by:** @<github-handle>
- **Authored by:** @<github-handle> (leave blank if unassigned)
- **Completed:** YYYY-MM-DD (leave blank if not yet done)

> A short description of the work. For features, describe the intended
> behavior and motivation. For bugs, include reliable reproduction steps.

GUIDELINES:
- Open a GitHub issue before adding an entry here if you are reporting a bug.
- Keep descriptions concise — link to the issue for extended discussion.
- Move entries to Done with a completion date when the PR merges.
- If work is abandoned, mark it Done and note the reason in the description.
-->

# Shiori Roadmap

## Planned

Accepted work that has not yet been started.

---
<!-- 
### Example Planned Entry
- **Type:** Feature
- **Status:** Planned
- **Issue:** [#0](https://github.com/<org>/shiori/issues/0)
- **Suggested by:** @contributor
- **Authored by:**

> Brief description of the intended feature and why it is useful.

--- -->

### Improve Markdown rendering in the reading view
- **Type:** Feature
- **Status:** Planned
- **Issue:** None
- **Suggested by:** @PeterDessev
- **Authored by:**

> The reader renders the tutor's sentence explanations as Markdown, but
> the current renderer is limited: table cells do not wrap (forcing
> horizontal scrolling), emoji are not displayed, and heading sizing is
> coarse. Improve Markdown rendering in the reading view — wrap table
> cells (or adopt a better table layout), give headings and inline styles
> cleaner, more consistent typography, and handle content the current
> renderer struggles with more gracefully.

---

### More consistent and stylish scroll bars
- **Type:** Feature
- **Status:** Planned
- **Issue:** None
- **Suggested by:** @PeterDessev
- **Authored by:**

> Scroll bars vary in placement and styling across the app. Make them
> more consistent and stylish — a uniform width and placement flush to
> the panel edge, a cohesive look across every view (reader side panel,
> dictionary, modals, lists), and theme-aware colors that fit Shiori's
> light, dark, and sepia themes.

---

## In Progress

Work that is actively being developed.

---
<!-- 
### Example In Progress Entry
- **Type:** Bug
- **Status:** In Progress
- **Issue:** [#0](https://github.com/<org>/shiori/issues/0)
- **Suggested by:** @contributor
- **Authored by:** @developer

> Brief description of the bug. Reproduction steps:
> 1. Step one
> 2. Step two
> 3. Observed behavior vs. expected behavior

--- -->

## Done

Completed and merged work, newest first.

---
<!-- 
### Example Completed Entry
- **Type:** Refactor
- **Status:** Done
- **Issue:** [#0](https://github.com/<org>/shiori/issues/0)
- **Suggested by:** @contributor
- **Authored by:** @developer
- **Completed:** YYYY-MM-DD

> Brief description of what was done.

--- -->