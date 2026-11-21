# Design: documentation and download site (GitHub Pages)

## Intent

cecilia has no web presence beyond the GitHub README. The site is for **people who have never
heard of cecilia**: in a few seconds they should see what it is (tab, notation, chant; Guitar Pro
in and out; play-along; keyboard-first), download it for their OS, and find getting-started and
reference docs.

## Goals

- A home page that sells cecilia, in the "Tablature" direction picked by the user: off-white
  paper, six string lines with brass fret numbers across the hero, bold grotesque headline
  (Bricolage Grotesque), palette `#FAFAF7` paper, `#23262B` graphite, `#B8862B` brass,
  `#8A949E` string steel. Lead with guitarists; chant is the third feature section.
- Home sections: hero (headline, lede, OS-aware download button, "Get started", demo GIF) →
  Guitar Pro in/out → play along → chant on the page → keyboard in window and terminal → dark
  download band (detected OS highlighted, other two shown) → footer.
- A Download page listing every file of the latest release grouped by OS, package-manager
  commands, the install script and `SHA256SUMS`.
- Docs in Zensical's standard layout, recoloured and in the same font, nav grouped as
  Start here / Guides / Reference / Project.
- Published to `https://kjmcnamara1.github.io/cecilia/` by a GitHub Actions workflow on pushes to
  `main` touching the site; `mise run docs` serves it locally.

## Non-goals

- No rewrite of the reference docs; no versioned docs; no blog; no custom domain; no analytics.
- No README slimming (the user didn't ask for it).

## Anti-goals

- The docs drifting from the files the Rust tests check (`COMMANDS.md`, `OPTIONS.md`).
- A download button that points at a stale release.
- A templated-looking SaaS landing page.

## Constraints

- `docs/` is the Zensical `docs_dir`, so `docs/COMMANDS.md` etc. are rendered as they are.
  `docs/SPEC.md` is internal: excluded (`exclude_docs`).
- Release asset names (from `release.yml`): `cecilia-vX.Y.Z-universal-apple-darwin.dmg`,
  `cecilia-vX.Y.Z-x86_64.AppImage`, `cecilia-editor_X.Y.Z-1_amd64.deb`,
  `cecilia-editor-X.Y.Z-1.x86_64.rpm`, `cecilia-vX.Y.Z.flatpak`, `cecilia_X.Y.Z_amd64.snap`,
  `cecilia-win-Setup.exe`, `cecilia-vX.Y.Z-x86_64-pc-windows-msvc.zip`, tarballs, `install.sh`,
  `SHA256SUMS`.
- Zensical 0.1.x is young: pin the version.

## Approaches

- **A. Zensical with a custom home template (picked).** One tool, one build; home page is a
  `template: home.html` override in `custom_dir`, sharing header, search and dark mode.
  Trade-off: written against a young template system.
- B. Hand-built landing + Zensical under `/docs/`: full freedom, but two looks and no shared
  search/dark mode.
- C. Starlight (Astro): nice splash pages, but adds Node to a Rust repo and needs doc edits.

## Decided

- Download data is fetched in the browser from
  `https://api.github.com/repos/kjmcnamara1/cecilia/releases/latest` (unauthenticated, 60/h per IP;
  fine). OS from `navigator.userAgentData.platform` / `navigator.userAgent`. Static fallback:
  link to `/releases/latest` and the install script one-liner, which work without JS.
- New pages built from README sections: Getting started, Playing along, Configuration
  (embeds `docs/config.example.toml`), Command line. Changelog page includes `CHANGELOG.md` via
  `pymdownx.snippets`.
- Screenshots captured from cecilia with `--screenshot` into `docs/assets/`.

## Left to the builder (marked as Claude's calls)

- File layout of the theme override and assets; exact copy; screenshot choice.
- Light/dark: docs pages get both schemes; the home page is designed for light, with a dark
  variant derived from the palette.
