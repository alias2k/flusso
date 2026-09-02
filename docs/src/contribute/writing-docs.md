# Writing docs

Prose docs follow `docs/STYLE.md`; agent-facing docs follow the `writing-for-agents` standard; code comments follow `CLAUDE.md`. Each fact has one home.

## Three corpora, three standards

| Corpus | Where | Standard |
| --- | --- | --- |
| The manual and READMEs | `docs/src/`, every `README.md` | `docs/STYLE.md`: impersonal voice, expert body with newcomer callouts, page types with templates, one fact one home |
| Agent-facing docs | `plugin/**`, `.claude/commands/**`, `CLAUDE.md` | `plugin/ARCHITECTURE.md` and `writing-for-agents`: one corpus, three consumers; pointers over restatement |
| Code comments | `///`, `//!`, the rare inline gotcha | `CLAUDE.md` "Conventions": self-descriptive code, detailed module docs, doc comments on the public surface |

## The book

The mdBook under `docs/` has seven parts. Six are for a reader with a job; **Reference** is where facts live, and every other page links there instead of restating a key, flag, default, metric, or endpoint. A page is a tutorial, a how-to, a reference, or an explanation, and follows that type's template in `STYLE.md`. Every page opens with a one-line TL;DR and stays under about 200 lines.

Build and check it locally:

```sh
mdbook build docs
lychee --offline docs/book
```

CI does the same on every PR and also asserts that every `alias2k.github.io/flusso/<path>` URL in the repository's markdown and Rust sources resolves to a page in the fresh build.

When a page moves, add an `[output.html.redirect]` entry in `docs/book.toml` and update `CLAUDE.md`'s pointers and any plugin skill naming the old path; `agent_docs_paths.rs` fails the build on a dangling repo path.

## READMEs

A README is a landing, not a manual: what the crate or directory is, one example, links into the book. Crate READMEs are the crates.io and docs.rs landing and are `include_str!`'d as the crate's `//!`, so they must stay crate-local (`cargo package` only bundles files under the crate dir) and must not `{{#include}}` anything. The root `README.md` is the GitHub pitch.

## The definition of done

`CLAUDE.md` owns it. When a change touches a user-authored surface (a `*.schema.yml` or `flusso.toml` key, a type tag, an enum token, a sink option), the same change updates the embedded JSON Schemas, the designer and its two locale catalogs, and the owning Reference page. When a change makes `CLAUDE.md` wrong, fix it in the same change.

## Where this shows up

- `docs/STYLE.md` for the templates.
- [Testing](testing.md) for the CI jobs that guard the docs.
