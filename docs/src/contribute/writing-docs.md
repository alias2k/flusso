# Writing docs

Prose docs follow `docs/STYLE.md`; agent-facing docs follow the `writing-for-agents` standard; code comments follow `CLAUDE.md`. Each fact has one home.

## Three corpora, three standards

| Corpus | Where | Standard |
| --- | --- | --- |
| The manual and READMEs | `docs/src/`, every `README.md` | `docs/STYLE.md`: impersonal voice, expert body with newcomer callouts, page types with templates, one fact one home |
| Agent-facing docs | `plugin/**`, `.claude/commands/**`, `CLAUDE.md` | `plugin/ARCHITECTURE.md` and `writing-for-agents`: one corpus, three consumers; pointers over restatement |
| Code comments | `///`, `//!`, the rare inline gotcha | `CLAUDE.md` "Conventions": self-descriptive code, detailed module docs, doc comments on the public surface |

## The book

The mdBook under `docs/` has seven parts; `docs/src/SUMMARY.md` is the tree. **Reference** is where facts live and every other page links there. `STYLE.md` defines the page types, their templates, and the length and linking rules; read it before adding a page.

Build and check it locally:

```sh
mdbook build docs
lychee --offline docs/book
```

CI does the same on every PR and also asserts that every `alias2k.github.io/flusso/<path>` URL in the repository's markdown and Rust sources resolves to a page in the fresh build.

When a page moves, add an `[output.html.redirect]` entry in `docs/book.toml` and update `CLAUDE.md`'s pointers and any plugin skill naming the old path; `agent_docs_paths.rs` fails the build on a dangling repo path.

## Where the book is published

The Pages site serves the book twice. The root, `alias2k.github.io/flusso/`, is `docs/` as of the newest release tag of any train; `/main/` is `docs/` as of `main`, with a banner saying it is unreleased and `noindex`. A docs change shows up under `/main/` when it merges and at the root with the next release.

- `.github/scripts/build-manuals.sh` builds both, each from a `git archive` of its ref.
- `docs/channels/` holds the two `head.hbs` overlays (the banner, the link between the two), passed as the theme directory at build time. They come from `main`, so an old tag builds unchanged. The book itself must not have a `theme/` directory or a `theme` key; the script refuses one.
- Deep links in crate READMEs and Rust docs point at the root: they reach readers through crates.io at release, when the root has caught up. Until then a link to a page newer than the last release is broken for someone reading the README on GitHub.
- Deep links in the plugin (`plugin/**`) point at `/main/`: the plugin is installed from the repository, so it describes `main`.
- The link check resolves both forms against the same fresh build.

## READMEs

`STYLE.md`'s README section applies. The one constraint worth repeating because it's mechanical: crate READMEs are `include_str!`'d as the crate's `//!` and `cargo package` only bundles files under the crate directory, so a README must stay crate-local and must not `{{#include}}` anything.

## The definition of done

`CLAUDE.md` owns it; read its "Keeping the designer current" and "Keeping this file current" sections before shipping a change that touches a user-authored surface. The short version: the docs are part of the change, not a follow-up.

## Where this shows up

- `docs/STYLE.md` for the templates.
- [Testing](testing.md) for the CI jobs that guard the docs.
