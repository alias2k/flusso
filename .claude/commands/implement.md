---
description: Implement a GitHub issue end-to-end — grill, open a draft plan PR with a live checklist, tick it off as you go, ready when green
---

Implement GitHub issue **$ARGUMENTS** from start to finish.

The shape: **open a draft PR up front** whose body is a self-contained implementation spec plus a
**detailed checklist**, then **tick each box as it lands** (committing and pushing per unit) so the
PR is a live progress tracker rather than a reveal at the end. Mark it ready once every box is
checked and CI-parity is green.

**The PR must be resumable.** If this session dies and a cold agent picks it up with no memory of the
discussion, the PR alone must be enough to continue: the approach is described, and each unchecked
box says specifically what to do and where.

## 1. Read the issue

`gh issue view $ARGUMENTS`. The issue describes the **problem**, not the solution, so you do not yet
know how to solve it. Don't assume.

## 2. Grill on the solution

Run **`/grill-with-docs`**. It is the interview primitive plus the domain-modeling discipline: work
the design tree in rounds, ask the whole frontier at once with a recommendation per question, find
facts yourself instead of asking, and as terms and decisions settle, record them in `CONTEXT.md`
(glossary only) and `docs/adr/` (only for hard-to-reverse, surprising, real-trade-off decisions).

Two flusso-specific additions to what it pushes on: the engine's at-least-once and dedup invariants,
and the crate layering. A proposal that violates either is wrong regardless of how good it looks.

Stop only when you could state exactly what you'll build, where, and why, with no open "it depends".

**Each grilling round ends the turn.** Ask the round, then wait for the user's answers before the
next round, and do not branch or open the PR until every round is answered. This holds even when
the session is autonomous or the harness says the user isn't watching: the recommendations are
proposals, and building on them before they're confirmed can make the branch, PR, and first commits
wrong. The one acceptable shortcut is the user saying up front to proceed on the recommendations.

## 3. Branch

Never work on `main`. `git checkout -b <type>/<slug>` where `<type>` matches the change (`feat`,
`fix`, `docs`, `chore`, `refactor`, …).

## 4. Open the plan PR

Push the branch (an empty commit is fine), then `gh pr create --draft`. Body, in order:

- **The implementation description.** The settled decisions and *how* the change works: the
  mechanism, the key types/files/functions, the data flow, any non-obvious choice and why. Enough
  that someone resuming knows the approach without re-deriving it. Record any assumption you
  verified (a probe against a live service, a config shape you checked) so nobody re-verifies it.
- `Closes #<issue>`.
- A `## To implement` checklist, `- [ ]` per line. Each box names the concrete change **and where it
  lands** — the crate or file, and the function or type when it helps. `- [ ] Core: add
  \`Column.enum_order\` + project to \`Mapping.enum_order\` (libs/0-kernel)`, never `- [ ] core
  changes`. Order them so ticking top-to-bottom is a valid build order. This list is the resume
  point.
- A `## Follow-ups (out of scope)` list, with issues filed and linked.
- End with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

Report the PR URL. Keep the body current: if the approach shifts, update it so the PR never lies.

## 5. Work the checklist top-down

Per item: do the work as a self-contained chunk, **run `cargo fmt --all`**, commit, push, then tick
its box (`gh pr edit --body-file …`) so the PR reflects real progress.

Conventional Commits, matching this repo's history (`fix(query): …`, `feat(engine): …`). **No
`Co-Authored-By` or AI-attribution trailer.**

## 6. Alignment gates

`CLAUDE.md` owns the definition of done and is the only current copy. Read the relevant section
rather than working from memory, and put each gate on the checklist as its own box:

| If the change touched… | Read |
| --- | --- |
| a `flusso.toml` or `*.schema.yml` key, tag, sibling, enum token, sink option | "Keeping the designer current" — the editor JSON schemas **and** the designer, its translations, and the two CI guards |
| any crate's behaviour | that crate's `README.md`, which is its `//!` |
| the user-facing surface | the owning mdBook chapter under `docs/src/` |
| layout, commands, invariants, lint policy, format | `CLAUDE.md` itself, in the same change |
| anything a plugin skill teaches | `plugin/ARCHITECTURE.md`, then the skill that owns that meaning |

## 7. Review and verify

Run **`/code-review`** against the merge-base, then fix what it finds.

Then match CI parity in the order `CLAUDE.md` lists under "Match CI before assuming green", with
`--workspace` on the test steps (`default-members = ["apps/cli"]`, so a bare invocation only touches
the CLI). If Docker isn't available for `--run-ignored all`, say so explicitly rather than silently
skipping it.

## 8. Ready for review

Tick the final box, `gh pr ready <number>`, confirm `Closes #<issue>` and the follow-up links are
present, and report the final PR URL.
