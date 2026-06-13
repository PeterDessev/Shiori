# Shiori website

The marketing landing page + user-guide docs for Shiori, built with
[Zola](https://www.getzola.org/) and deployed to GitHub Pages.

Live at **https://PeterDessev.github.io/Shiori/** once Pages is enabled.

## Layout

```
site/
  config.toml            # base_url, site metadata, theme/highlight settings
  content/
    _index.md            # landing page (renders templates/index.html)
    docs/
      _index.md          # docs overview + sidebar order
      *.md               # one page per guide topic (sourced from docs/wiki)
  templates/
    base.html            # shell: header, footer, <head>
    index.html           # landing page
    section.html         # docs overview
    page.html            # a single docs page (sidebar + prev/next)
  static/
    style.css            # all styling (no build step, no SASS)
    img/                 # screenshots + icons (copied from ../assets)
    favicon.ico
```

The docs pages are derived from the project wiki (`docs/wiki/*`). If you edit a
topic, update it here (this is what readers see).

## Develop locally

Install Zola (https://www.getzola.org/documentation/getting-started/installation/),
then from this `site/` directory:

```sh
zola serve     # live-reload dev server at http://127.0.0.1:1111
zola build     # output to site/public/
```

`zola check` validates internal links.

## Deploy

Pushing to `main` with changes under `site/**` triggers
[`.github/workflows/site.yml`](../.github/workflows/site.yml), which builds with
the pinned Zola version and publishes to Pages.

**One-time repo setup:** Settings → Pages → *Build and deployment* → Source =
**GitHub Actions**.

If you ever move the repo or add a custom domain, update `base_url` in
`config.toml` to match (it must equal the public URL, including any subpath).
