# Shiori website

The marketing landing page + user-guide docs for Shiori, built with
[Zola](https://www.getzola.org/) and deployed to GitHub Pages.

Live at **https://PeterDessev.github.io/Shiori/**.

## Layout

```
site/
  config.toml            # base_url, site metadata, GitHub links
  content/
    _index.md            # landing page (renders templates/index.html)
    docs/
      _index.md          # docs overview + sidebar order
      *.md               # one page per guide topic
  templates/
    base.html            # shell: header, footer, <head>
    index.html           # landing page
    section.html         # docs overview
    page.html            # a single docs page (sidebar + prev/next)
  static/
    style.css            # all styling (no build step, no SASS)
    img/                 # screenshots + icons (copied from ../assets)
    favicon.ico          # favicon + PWA set (favicon-16/32, apple-touch-icon-180,
    *.png                #   android-chrome-192/512, maskable-512) from ../assets/icon/web
    site.webmanifest     # PWA manifest (icons + theme); wired into base.html <head>
```

`content/docs/` is the canonical user guide — edit these pages directly (this
is what readers see). They were originally moved here from `docs/wiki/`, which
now holds only `Language-Packs.md`, the pack-format reference.

To add a docs page, create `content/docs/<slug>.md` with only `title` and
`weight` in its TOML front matter; the sidebar and prev/next links follow
`weight` (`docs/_index.md` sets `sort_by = "weight"`), so no template edits
are needed.

## Develop locally

Install Zola (https://www.getzola.org/documentation/getting-started/installation/),
then from this `site/` directory:

```sh
zola serve     # live-reload dev server at http://127.0.0.1:1111
zola build     # output to site/public/
```

`zola check` validates internal links.

## Deploy

Pushing to `master` (the default branch) with changes under `site/**` — or to
the workflow file itself — triggers
[`.github/workflows/site.yml`](../.github/workflows/site.yml), which builds with
the pinned Zola version and publishes to Pages. The workflow can also be run
manually from the Actions tab (`workflow_dispatch`).

**One-time repo setup:** Settings → Pages → *Build and deployment* → Source =
**GitHub Actions**.

If you ever move the repo or add a custom domain, update `base_url` in
`config.toml` to match (it must equal the public URL, including any subpath).
