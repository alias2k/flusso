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

## READMEs

`STYLE.md`'s README section applies. The one constraint worth repeating because it's mechanical: crate READMEs are `include_str!`'d as the crate's `//!` and `cargo package` only bundles files under the crate directory, so a README must stay crate-local and must not `{{#include}}` anything.

## The definition of done

`CLAUDE.md` owns it; read its "Keeping the designer current" and "Keeping this file current" sections before shipping a change that touches a user-authored surface. The short version: the docs are part of the change, not a follow-up.

## Where this shows up

- `docs/STYLE.md` for the templates.
- [Testing](testing.md) for the CI jobs that guard the docs.
