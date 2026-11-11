# Manual channels

The site publishes the manual twice (`.github/workflows/pages.yml`, issue #209):

| Path | Built from | Overlay |
| --- | --- | --- |
| `/flusso/` | `docs/` at the newest release tag of any train | `release/` |
| `/flusso/main/` | `docs/` at `main`'s tip | `main/` |

Each directory here is an mdBook theme override holding only `head.hbs`.
`.github/scripts/build-manuals.sh` points the build at it with
`MDBOOK_OUTPUT__HTML__THEME`, so mdBook takes every other theme file from its
defaults. The overlay comes from `main`, never from the tag, so an old release
needs no change to get it.

- `main/head.hbs`: `noindex`, plus a banner on every page saying the page
  documents unreleased `main`, linking to the released manual.
- `release/head.hbs`: a small link to `/main/`.

`docs/book.toml` must not set its own `theme`, and `docs/theme/` must not
exist: the overlay would replace them, so the build script refuses.
